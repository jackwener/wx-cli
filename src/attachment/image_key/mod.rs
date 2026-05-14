//! V2 image AES key 提取 — 平台相关。
//!
//! 路径：
//! - macOS：磁盘派生（`key_<uin>_*.statistic` 文件名拿 uin → `md5(str(uin) + wxid)[:16]`）
//!   + brute-force fallback（`md5(str(uin))[:4] == wxid_suffix` 枚举 2^24）
//! - Windows：扫 `Weixin.exe` 内存，匹配 `[a-zA-Z0-9]{32}` 候选，按已知 AES ciphertext-block
//!   反验（`find_image_key.py` / `find_image_key.c` 已写实）
//! - Linux：上游空白；当前不实现，遇到 V2 .dat 返回 unsupported 错误

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

use anyhow::Result;
use regex::bytes::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::attachment::decoder::{detect_image_format, V2_MAGIC};

/// V2 图片真正需要的是两份材料：
/// - 16 字节 ASCII AES key
/// - XOR key（macOS 上来自 uin & 0xff，不是总能硬编码成 0x88）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageKeyMaterial {
    pub aes_key: [u8; 16],
    pub xor_key: u8,
}

/// 单个 wxid 的 V2 image key 提取接口。
///
/// 实现者负责跨调用缓存（一台机器上同一 wxid 的 image key 在微信不重启时通常稳定）。
pub trait ImageKeyProvider {
    fn get_key(&self, wxid: &str) -> Result<ImageKeyMaterial>;

    fn get_aes_key(&self, wxid: &str) -> Result<[u8; 16]> {
        Ok(self.get_key(wxid)?.aes_key)
    }

    fn get_xor_key(&self, wxid: &str) -> Result<u8> {
        Ok(self.get_key(wxid)?.xor_key)
    }
}

/// 平台默认实现。
pub fn default_provider() -> Option<Box<dyn ImageKeyProvider + Send + Sync>> {
    #[cfg(target_os = "macos")]
    {
        return Some(Box::new(macos::MacosImageKeyProvider::from_current_config()));
    }
    #[cfg(target_os = "windows")]
    {
        return Some(Box::new(windows::WindowsImageKeyProvider::from_current_config()));
    }
    #[cfg(target_os = "linux")]
    {
        return Some(Box::new(linux::LinuxImageKeyProvider));
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

pub(crate) fn configured_db_dir_for_wxid(configured_db_dir: &Path, requested_wxid: &str) -> PathBuf {
    if requested_wxid.trim().is_empty() {
        return configured_db_dir.to_path_buf();
    }

    let configured_leaf = wxid_from_db_dir(configured_db_dir);
    if let Some(leaf) = configured_leaf.as_deref() {
        if same_wxid(leaf, requested_wxid) {
            return configured_db_dir.to_path_buf();
        }
    }

    xwechat_files_root(configured_db_dir)
        .map(|root| root.join(requested_wxid).join("db_storage"))
        .unwrap_or_else(|| configured_db_dir.to_path_buf())
}

pub(crate) fn wxid_from_db_dir(db_dir: &Path) -> Option<String> {
    let mut components = db_dir
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned());
    while let Some(component) = components.next() {
        if component == "xwechat_files" {
            return components.next();
        }
    }
    None
}

pub(crate) fn xwechat_files_root(db_dir: &Path) -> Option<PathBuf> {
    let parts: Vec<_> = db_dir
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    let idx = parts.iter().position(|part| part == "xwechat_files")?;
    Some(join_components(&parts[..=idx]))
}

pub(crate) fn normalize_wxid(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    if let Some(stripped) = raw.strip_prefix("wxid_") {
        let head = stripped.split('_').next().unwrap_or(stripped);
        return format!("wxid_{}", head);
    }
    if let Some((base, suffix)) = raw.rsplit_once('_') {
        if suffix.len() == 4 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return base.to_string();
        }
    }
    raw.to_string()
}

pub(crate) fn same_wxid(a: &str, b: &str) -> bool {
    a == b || normalize_wxid(a) == normalize_wxid(b)
}

pub(crate) fn join_components(parts: &[String]) -> PathBuf {
    let mut out = if parts.first().map(|part| part.is_empty()).unwrap_or(false) {
        PathBuf::from("/")
    } else {
        PathBuf::new()
    };
    for part in parts {
        if part.is_empty() {
            continue;
        }
        out.push(part);
    }
    out
}

pub(crate) fn attach_root_for_db_dir(db_dir: &Path) -> PathBuf {
    db_dir
        .parent()
        .map(|base| base.join("msg").join("attach"))
        .unwrap_or_else(|| PathBuf::from("msg/attach"))
}

pub(crate) fn find_v2_template_ciphertexts(
    attach_dir: &Path,
    max_templates: usize,
    max_files: usize,
) -> Result<Vec<[u8; 16]>> {
    if !attach_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut out = collect_templates_with_suffix(attach_dir, "_t.dat", max_templates, max_files)?;
    if out.is_empty() {
        out = collect_templates_with_suffix(attach_dir, ".dat", max_templates, max_files)?;
    }
    Ok(out)
}

pub(crate) fn derive_xor_key_from_v2_dat(
    attach_dir: &Path,
    sample: usize,
    min_samples: usize,
) -> Result<Option<(u8, usize, usize)>> {
    if !attach_dir.is_dir() {
        return Ok(None);
    }
    let mut votes = Vec::new();
    visit_files(attach_dir, &mut |path| -> Result<bool> {
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            return Ok(false);
        };
        if !name.ends_with(".dat") {
            return Ok(false);
        }

        let meta = fs::metadata(path)?;
        if meta.len() < 0x20 {
            return Ok(false);
        }

        let bytes = fs::read(path)?;
        if bytes.starts_with(&V2_MAGIC) {
            let last = *bytes.last().unwrap();
            votes.push(last ^ 0xD9);
            if votes.len() >= sample {
                return Ok(true);
            }
        }
        Ok(false)
    })?;

    if votes.len() < min_samples {
        return Ok(None);
    }

    let mut counts = [0usize; 256];
    for vote in &votes {
        counts[*vote as usize] += 1;
    }
    let (xor_key, top_votes) = counts
        .iter()
        .enumerate()
        .max_by_key(|(_, count)| *count)
        .map(|(idx, count)| (idx as u8, *count))
        .expect("votes 非空");
    Ok(Some((xor_key, top_votes, votes.len())))
}

pub(crate) fn verify_aes_key(aes_key: &[u8; 16], templates: &[[u8; 16]]) -> bool {
    !templates.is_empty()
        && templates
            .iter()
            .all(|template| decrypt_template_block(aes_key, template).is_some())
}

pub(crate) fn ascii_alnum_candidates<'a>(buf: &'a [u8], len: usize) -> Vec<&'a [u8]> {
    let re = match len {
        16 => regex16(),
        32 => regex32(),
        _ => return Vec::new(),
    };

    re.find_iter(buf)
        .filter_map(|matched| {
            let start = matched.start();
            let end = matched.end();
            let left_ok = start == 0 || !buf[start - 1].is_ascii_alphanumeric();
            let right_ok = end == buf.len() || !buf[end].is_ascii_alphanumeric();
            (left_ok && right_ok).then_some(&buf[start..end])
        })
        .collect()
}

fn collect_templates_with_suffix(
    dir: &Path,
    suffix: &str,
    max_templates: usize,
    max_files: usize,
) -> Result<Vec<[u8; 16]>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut examined = 0usize;
    visit_files(dir, &mut |path| -> Result<bool> {
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            return Ok(false);
        };
        if !name.ends_with(suffix) {
            return Ok(false);
        }
        examined += 1;
        let bytes = fs::read(path)?;
        if bytes.len() >= 0x1F && bytes.starts_with(&V2_MAGIC) {
            let template: [u8; 16] = bytes[0x0F..0x1F].try_into().unwrap();
            if seen.insert(template) {
                out.push(template);
                if out.len() >= max_templates {
                    return Ok(true);
                }
            }
        }
        Ok(examined >= max_files && !out.is_empty())
    })?;
    Ok(out)
}

fn visit_files<F>(dir: &Path, f: &mut F) -> Result<bool>
where
    F: FnMut(&Path) -> Result<bool>,
{
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            if visit_files(&path, f)? {
                return Ok(true);
            }
            continue;
        }
        if f(&path)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn decrypt_template_block(aes_key: &[u8; 16], ciphertext: &[u8; 16]) -> Option<&'static str> {
    use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};

    let cipher = aes::Aes128::new(aes_key.into());
    let mut block = GenericArray::clone_from_slice(ciphertext);
    cipher.decrypt_block(&mut block);
    let block: [u8; 16] = block.as_slice().try_into().ok()?;
    let format = detect_image_format(&block);
    (format != "bin").then_some(format)
}

fn regex16() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z0-9]{16}").unwrap())
}

fn regex32() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z0-9]{32}").unwrap())
}

#[cfg(test)]
mod tests {
    use super::{ascii_alnum_candidates, normalize_wxid, same_wxid};

    #[test]
    fn regex_candidates_respect_boundaries() {
        let buf = b"xx 0123456789ABCDef yy";
        let hits = ascii_alnum_candidates(buf, 16);
        assert_eq!(hits, vec![&buf[3..19]]);
    }

    #[test]
    fn regex_candidates_ignore_embedded_runs() {
        let buf = b"x0123456789ABCDefz";
        assert!(ascii_alnum_candidates(buf, 16).is_empty());
    }

    #[test]
    fn wxid_normalization_matches_expected_forms() {
        assert_eq!(normalize_wxid("wxid_abc_def"), "wxid_abc");
        assert_eq!(normalize_wxid("your_wxid_a1b2"), "your_wxid");
        assert!(same_wxid("your_wxid_a1b2", "your_wxid"));
    }
}
