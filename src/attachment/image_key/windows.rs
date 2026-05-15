//! Windows V2 image AES key 提取。
//!
//! 扫 `Weixin.exe` 进程内存，匹配模式 `[A-Za-z0-9]{32}` / `[A-Za-z0-9]{16}`，
//! 然后用 V2 模板 AES block 反验，控制 false positive。

use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_EXECUTE_READWRITE,
    PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOCACHE, PAGE_NOACCESS, PAGE_READWRITE,
    PAGE_WRITECOMBINE, PAGE_WRITECOPY,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

use crate::config;

use super::{
    ascii_alnum_candidates, attach_root_for_db_dir, configured_db_dir_for_wxid,
    derive_xor_key_from_v2_dat, find_v2_template_ciphertexts, verify_aes_key, ImageKeyMaterial,
    ImageKeyProvider,
};

const CHUNK_SIZE: usize = 2 * 1024 * 1024;
const MAX_REGION_SIZE: usize = 50 * 1024 * 1024;

pub struct WindowsImageKeyProvider {
    configured_db_dir: Result<PathBuf, String>,
    cache: Mutex<HashMap<String, ImageKeyMaterial>>,
}

impl WindowsImageKeyProvider {
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

impl ImageKeyProvider for WindowsImageKeyProvider {
    fn get_key(&self, wxid: &str) -> Result<ImageKeyMaterial> {
        let cache_key = wxid.trim().to_string();
        if let Some(found) = self.cache.lock().unwrap().get(&cache_key).copied() {
            return Ok(found);
        }

        let configured_db_dir = self
            .configured_db_dir
            .as_ref()
            .map_err(|err| anyhow::anyhow!("读取 config.db_dir 失败: {}", err))?;
        let db_dir = configured_db_dir_for_wxid(configured_db_dir, wxid);
        let attach_dir = attach_root_for_db_dir(&db_dir);
        let key = derive_key_for_paths(&attach_dir)?;
        self.cache.lock().unwrap().insert(cache_key, key);
        Ok(key)
    }
}

fn derive_key_for_paths(attach_dir: &std::path::Path) -> Result<ImageKeyMaterial> {
    let templates = find_v2_template_ciphertexts(attach_dir, 3, 64)?;
    if templates.is_empty() {
        bail!("在 {} 下找不到 V2 模板文件", attach_dir.display());
    }
    let xor_key = derive_xor_key_from_v2_dat(attach_dir, 10, 3)?
        .map(|(key, _, _)| key)
        .unwrap_or(0x88);

    let pid = find_wechat_pid().context("找不到 Weixin.exe 进程，请确认微信正在运行")?;
    let process = unsafe {
        OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, false, pid)
            .context("OpenProcess 失败，请以管理员权限运行")?
    };

    let aes_key = scan_memory_for_key(process, &templates);
    unsafe {
        let _ = CloseHandle(process);
    }

    Ok(ImageKeyMaterial {
        aes_key: aes_key?,
        xor_key,
    })
}

fn find_wechat_pid() -> Option<u32> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()? };
    let mut entry = PROCESSENTRY32 {
        dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
        ..Default::default()
    };

    unsafe {
        if Process32First(snapshot, &mut entry).is_err() {
            let _ = CloseHandle(snapshot);
            return None;
        }
        loop {
            let name =
                std::ffi::CStr::from_ptr(entry.szExeFile.as_ptr() as *const i8).to_string_lossy();
            if name.eq_ignore_ascii_case("Weixin.exe") {
                let pid = entry.th32ProcessID;
                let _ = CloseHandle(snapshot);
                return Some(pid);
            }
            if Process32Next(snapshot, &mut entry).is_err() {
                break;
            }
        }
        let _ = CloseHandle(snapshot);
    }
    None
}

fn scan_memory_for_key(process: HANDLE, templates: &[[u8; 16]]) -> Result<[u8; 16]> {
    let mut seen = HashSet::<[u8; 16]>::new();
    let mut address = 0usize;

    loop {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let ret = unsafe {
            VirtualQueryEx(
                process,
                Some(address as *const _),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if ret == 0 {
            break;
        }

        let base = mbi.BaseAddress as usize;
        let size = mbi.RegionSize;
        if mbi.State == MEM_COMMIT && is_candidate_page(mbi.Protect.0) && size <= MAX_REGION_SIZE {
            if let Some(aes_key) = scan_region(process, base, size, templates, &mut seen)? {
                return Ok(aes_key);
            }
        }

        address = base.saturating_add(size);
        if address == 0 {
            break;
        }
    }

    bail!("Windows 进程内存里没有找到可验证的 V2 AES key")
}

fn scan_region(
    process: HANDLE,
    base: usize,
    size: usize,
    templates: &[[u8; 16]],
    seen: &mut HashSet<[u8; 16]>,
) -> Result<Option<[u8; 16]>> {
    let overlap = 31usize;
    let mut offset = 0usize;

    while offset < size {
        let chunk_size = std::cmp::min(CHUNK_SIZE, size - offset);
        let addr = base + offset;
        let mut buf = vec![0u8; chunk_size];
        let mut bytes_read = 0usize;

        let ok = unsafe {
            ReadProcessMemory(
                process,
                addr as *const _,
                buf.as_mut_ptr() as *mut _,
                chunk_size,
                Some(&mut bytes_read),
            )
            .is_ok()
        };

        if ok && bytes_read > 0 {
            buf.truncate(bytes_read);
            if let Some(key) = scan_candidate_buffer(&buf, templates, seen) {
                return Ok(Some(key));
            }
        }

        offset += if chunk_size > overlap {
            chunk_size - overlap
        } else {
            chunk_size
        };
    }

    Ok(None)
}

fn scan_candidate_buffer(
    buf: &[u8],
    templates: &[[u8; 16]],
    seen: &mut HashSet<[u8; 16]>,
) -> Option<[u8; 16]> {
    for candidate in ascii_alnum_candidates(buf, 32) {
        let mut key = [0u8; 16];
        key.copy_from_slice(&candidate[..16]);
        if seen.insert(key) && verify_aes_key(&key, templates) {
            return Some(key);
        }
    }
    for candidate in ascii_alnum_candidates(buf, 16) {
        let mut key = [0u8; 16];
        key.copy_from_slice(candidate);
        if seen.insert(key) && verify_aes_key(&key, templates) {
            return Some(key);
        }
    }
    None
}

fn is_candidate_page(protect: u32) -> bool {
    if protect == PAGE_NOACCESS.0 || (protect & PAGE_GUARD.0) != 0 {
        return false;
    }
    let base = protect & !(PAGE_GUARD.0 | PAGE_NOCACHE.0 | PAGE_WRITECOMBINE.0);
    matches!(
        base,
        value if value == PAGE_READWRITE.0
            || value == PAGE_WRITECOPY.0
            || value == PAGE_EXECUTE_READWRITE.0
            || value == PAGE_EXECUTE_WRITECOPY.0
    )
}
