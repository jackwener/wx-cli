//! macOS V2 image AES key 提取。
//!
//! 主路径：从 `key_<uin>_*.statistic` 文件名拿 uin，然后
//! `md5(str(uin) + normalize(wxid)).hex()[:16]` 派生 AES key。
//!
//! fallback：通过 `md5(str(uin))[:4] == wxid_suffix` + `uin & 0xff == xor_key`
//! 把搜索空间压到 2^24，再用 V2 模板反验 AES key。

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use crate::config;

use super::{
    attach_root_for_db_dir, configured_db_dir_for_wxid, derive_xor_key_from_v2_dat,
    find_v2_template_ciphertexts, join_components, normalize_wxid, verify_aes_key, wxid_from_db_dir,
    ImageKeyMaterial, ImageKeyProvider,
};

pub struct MacosImageKeyProvider {
    configured_db_dir: Result<PathBuf, String>,
    cache: Mutex<HashMap<String, ImageKeyMaterial>>,
}

impl MacosImageKeyProvider {
    pub fn from_current_config() -> Self {
        let configured_db_dir = config::load_config()
            .map(|cfg| cfg.db_dir)
            .map_err(|err| err.to_string());
        Self {
            configured_db_dir,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

impl ImageKeyProvider for MacosImageKeyProvider {
    fn get_key(&self, wxid: &str) -> Result<ImageKeyMaterial> {
        let cache_key = normalize_wxid(wxid);
        if let Some(found) = self.cache.lock().unwrap().get(&cache_key).copied() {
            return Ok(found);
        }

        let configured_db_dir = self
            .configured_db_dir
            .as_ref()
            .map_err(|err| anyhow::anyhow!("读取 config.db_dir 失败: {}", err))?;
        let db_dir = configured_db_dir_for_wxid(configured_db_dir, wxid);
        let attach_dir = attach_root_for_db_dir(&db_dir);
        let key = derive_key_for_paths(&db_dir, &attach_dir)?;
        self.cache.lock().unwrap().insert(cache_key, key);
        Ok(key)
    }
}

fn derive_key_for_paths(db_dir: &Path, attach_dir: &Path) -> Result<ImageKeyMaterial> {
    let templates = find_v2_template_ciphertexts(attach_dir, 3, 64)?;
    if templates.is_empty() {
        bail!("在 {} 下找不到 V2 模板文件", attach_dir.display());
    }

    if let Some(found) = find_via_kvcomm(db_dir, &templates)? {
        return Ok(found);
    }

    let (wxid_full, wxid_norm, suffix) =
        extract_wxid_parts(db_dir).context("db_dir 不含可用于 fallback 的 wxid 4 位后缀")?;
    let (xor_key, _votes, _total) = derive_xor_key_from_v2_dat(attach_dir, 10, 3)?
        .context("V2 .dat 样本不足，无法投票反推 xor_key")?;

    for wxid in preferred_wxid_candidates(&wxid_full, &wxid_norm) {
        if let Some(aes_key) = bruteforce_aes_key(xor_key, &suffix, wxid, &templates)? {
            return Ok(ImageKeyMaterial { aes_key, xor_key });
        }
    }

    bail!("macOS V2 图片 key 派生失败")
}

fn find_via_kvcomm(db_dir: &Path, templates: &[[u8; 16]]) -> Result<Option<ImageKeyMaterial>> {
    let Some(kvcomm_dir) = find_existing_kvcomm_dir(db_dir) else {
        return Ok(None);
    };

    let codes = collect_kvcomm_codes(&kvcomm_dir)?;
    if codes.is_empty() {
        return Ok(None);
    }
    let wxids = collect_wxid_candidates(db_dir);
    if wxids.is_empty() {
        return Ok(None);
    }

    for wxid in wxids {
        for code in &codes {
            let candidate = derive_image_key_material(*code, &wxid);
            if verify_aes_key(&candidate.aes_key, templates) {
                return Ok(Some(candidate));
            }
        }
    }
    Ok(None)
}

fn derive_image_key_material(code: u32, wxid: &str) -> ImageKeyMaterial {
    let xor_key = (code & 0xFF) as u8;
    let digest = format!("{:x}", md5::compute(format!("{}{}", code, wxid)));
    let mut aes_key = [0u8; 16];
    aes_key.copy_from_slice(&digest.as_bytes()[..16]);
    ImageKeyMaterial { aes_key, xor_key }
}

fn collect_wxid_candidates(db_dir: &Path) -> Vec<String> {
    let Some(raw) = wxid_from_db_dir(db_dir) else {
        return Vec::new();
    };
    let mut out = vec![raw.clone()];
    let normalized = normalize_wxid(&raw);
    if normalized != raw {
        out.push(normalized);
    }
    out
}

fn extract_wxid_parts(db_dir: &Path) -> Option<(String, String, String)> {
    let raw = wxid_from_db_dir(db_dir)?;
    let idx = raw.rfind('_')?;
    let suffix = &raw[idx + 1..];
    if suffix.len() != 4 || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some((raw.clone(), normalize_wxid(&raw), suffix.to_ascii_lowercase()))
}

fn preferred_wxid_candidates<'a>(raw: &'a str, normalized: &'a str) -> Vec<&'a str> {
    if raw == normalized {
        vec![raw]
    } else {
        vec![normalized, raw]
    }
}

fn derive_kvcomm_dir_candidates(db_dir: &Path) -> Vec<PathBuf> {
    let parts: Vec<String> = db_dir
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();

    let mut candidates = Vec::new();
    if let Some(idx) = parts.iter().position(|part| part == "xwechat_files") {
        let documents_root = join_components(&parts[..idx]);
        candidates.push(documents_root.join("app_data/net/kvcomm"));
        candidates.push(documents_root.join("xwechat/net/kvcomm"));
        if idx >= 1 {
            let container_root = join_components(&parts[..idx - 1]);
            candidates.push(
                container_root
                    .join("Library/Application Support/com.tencent.xinWeChat/xwechat/net/kvcomm"),
            );
            candidates.push(
                container_root.join("Library/Application Support/com.tencent.xinWeChat/net/kvcomm"),
            );
        }
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(
            home.join("Library/Containers/com.tencent.xinWeChat/Data/Documents/app_data/net/kvcomm"),
        );
    }

    let mut dedup = Vec::new();
    for candidate in candidates {
        if !dedup.contains(&candidate) {
            dedup.push(candidate);
        }
    }
    dedup
}

fn find_existing_kvcomm_dir(db_dir: &Path) -> Option<PathBuf> {
    derive_kvcomm_dir_candidates(db_dir)
        .into_iter()
        .find(|path| path.is_dir())
}

fn collect_kvcomm_codes(kvcomm_dir: &Path) -> Result<Vec<u32>> {
    let mut codes = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(kvcomm_dir)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(|value| value.to_string()) else {
            continue;
        };
        let Some(rest) = name.strip_prefix("key_") else {
            continue;
        };
        let Some((code, _)) = rest.split_once('_') else {
            continue;
        };
        if let Ok(code) = code.parse::<u32>() {
            codes.insert(code);
        }
    }
    Ok(codes.into_iter().collect())
}

fn bruteforce_aes_key(
    xor_key: u8,
    suffix_hex: &str,
    wxid: &str,
    templates: &[[u8; 16]],
) -> Result<Option<[u8; 16]>> {
    let suffix = hex_prefix_to_bytes(suffix_hex)?;
    let workers = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .max(1);
    let total = 1u32 << 24;
    let chunk = total / workers as u32;
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let wxid = Arc::new(wxid.as_bytes().to_vec());
    let templates = Arc::new(templates.to_vec());

    std::thread::scope(|scope| {
        for idx in 0..workers {
            let start = idx as u32 * chunk;
            let end = if idx + 1 == workers {
                total
            } else {
                (idx as u32 + 1) * chunk
            };
            let stop = Arc::clone(&stop);
            let tx = tx.clone();
            let wxid = Arc::clone(&wxid);
            let templates = Arc::clone(&templates);
            scope.spawn(move || {
                for upper in start..end {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let uin = (upper << 8) | xor_key as u32;
                    let uin_ascii = uin.to_string();
                    let digest = md5::compute(uin_ascii.as_bytes());
                    if digest.0[0] != suffix[0] || digest.0[1] != suffix[1] {
                        continue;
                    }

                    let mut input = Vec::with_capacity(uin_ascii.len() + wxid.len());
                    input.extend_from_slice(uin_ascii.as_bytes());
                    input.extend_from_slice(&wxid);
                    let aes_hex = format!("{:x}", md5::compute(input));
                    let mut aes_key = [0u8; 16];
                    aes_key.copy_from_slice(&aes_hex.as_bytes()[..16]);
                    if verify_aes_key(&aes_key, &templates) {
                        stop.store(true, Ordering::Relaxed);
                        let _ = tx.send(aes_key);
                        break;
                    }
                }
            });
        }
    });
    drop(tx);
    Ok(rx.try_iter().next())
}

fn hex_prefix_to_bytes(hex: &str) -> Result<[u8; 2]> {
    if hex.len() != 4 {
        bail!("wxid suffix 不是 4 位 hex: {}", hex);
    }
    let hi = u8::from_str_radix(&hex[..2], 16)?;
    let lo = u8::from_str_radix(&hex[2..], 16)?;
    Ok([hi, lo])
}

#[cfg(test)]
mod tests {
    use super::{derive_key_for_paths, find_existing_kvcomm_dir};
    use super::collect_wxid_candidates;
    use crate::attachment::image_key::normalize_wxid;
    use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
    use aes::Aes128;
    use std::fs;
    use std::path::Path;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "wx-cli-image-key-macos-{}-{:?}",
            label,
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_v2_template(path: &Path, aes_key: &[u8; 16], xor_key: u8, plaintext: &[u8; 16]) {
        let cipher = Aes128::new(aes_key.into());
        let mut block = GenericArray::clone_from_slice(plaintext);
        cipher.encrypt_block(&mut block);

        let mut data = Vec::new();
        data.extend_from_slice(&crate::attachment::decoder::V2_MAGIC);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.push(0);
        data.extend_from_slice(&block);
        data.push(0);
        data.push(0xD9 ^ xor_key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }

    #[test]
    fn normalize_wxid_matches_expected_shapes() {
        assert_eq!(normalize_wxid("wxid_abc_def"), "wxid_abc");
        assert_eq!(normalize_wxid("your_wxid_a1b2"), "your_wxid");
        assert_eq!(normalize_wxid("plain"), "plain");
    }

    #[test]
    fn kvcomm_path_detection_works() {
        let dir = temp_dir("kvcomm");
        let db_dir = dir.join(
            "Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files/your_wxid_a1b2/db_storage",
        );
        let kvcomm = dir.join(
            "Library/Containers/com.tencent.xinWeChat/Data/Documents/app_data/net/kvcomm",
        );
        fs::create_dir_all(&db_dir).unwrap();
        fs::create_dir_all(&kvcomm).unwrap();
        assert_eq!(find_existing_kvcomm_dir(&db_dir), Some(kvcomm));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn derives_key_via_kvcomm() {
        let dir = temp_dir("via-kvcomm");
        let db_dir = dir.join(
            "Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files/your_wxid_a1b2/db_storage",
        );
        let attach = dir.join(
            "Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files/your_wxid_a1b2/msg/attach/chat/2026-05/Img",
        );
        let kvcomm = dir.join(
            "Library/Containers/com.tencent.xinWeChat/Data/Documents/app_data/net/kvcomm",
        );
        fs::create_dir_all(&db_dir).unwrap();
        fs::create_dir_all(&kvcomm).unwrap();
        fs::write(kvcomm.join("key_42_x.statistic"), b"").unwrap();

        let digest = format!("{:x}", md5::compute("42your_wxid"));
        let mut aes_key = [0u8; 16];
        aes_key.copy_from_slice(&digest.as_bytes()[..16]);
        write_v2_template(
            &attach.join("sample_t.dat"),
            &aes_key,
            42,
            b"\xFF\xD8\xFFtemplate-001!",
        );

        let derived = derive_key_for_paths(&db_dir, db_dir.parent().unwrap().join("msg/attach").as_path())
            .unwrap();
        assert_eq!(derived.aes_key, aes_key);
        assert_eq!(derived.xor_key, 42);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn derives_key_via_bruteforce_fallback() {
        let dir = temp_dir("via-fallback");
        let suffix = format!("{:x}", md5::compute("42"))
            .chars()
            .take(4)
            .collect::<String>();
        let raw_wxid = format!("mywxid_{}", suffix);
        let db_dir = dir.join(format!(
            "Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files/{}/db_storage",
            raw_wxid
        ));
        let attach = dir.join(format!(
            "Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files/{}/msg/attach/chat/2026-05/Img",
            raw_wxid
        ));
        fs::create_dir_all(&db_dir).unwrap();

        let digest = format!("{:x}", md5::compute("42mywxid"));
        let mut aes_key = [0u8; 16];
        aes_key.copy_from_slice(&digest.as_bytes()[..16]);
        for idx in 0..3 {
            write_v2_template(
                &attach.join(format!("sample{}_t.dat", idx)),
                &aes_key,
                42,
                b"\xFF\xD8\xFFtemplate-001!",
            );
        }

        let derived = derive_key_for_paths(&db_dir, db_dir.parent().unwrap().join("msg/attach").as_path())
            .unwrap();
        assert_eq!(derived.aes_key, aes_key);
        assert_eq!(derived.xor_key, 42);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn collects_raw_and_normalized_wxid() {
        let dir = temp_dir("wxid");
        let db_dir = dir.join(
            "Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files/your_wxid_a1b2/db_storage",
        );
        fs::create_dir_all(&db_dir).unwrap();
        let wxids = collect_wxid_candidates(&db_dir);
        assert_eq!(wxids, vec!["your_wxid_a1b2".to_string(), "your_wxid".to_string()]);
        let _ = fs::remove_dir_all(dir);
    }
}
