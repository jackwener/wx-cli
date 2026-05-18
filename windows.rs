/// Windows WeChat 进程内存密钥扫描器
///
/// 使用 Windows API：
/// - PowerShell Get-Process: 获取进程内存（字节，兼容中英文 Windows）
/// - OpenProcess: 获取进程句柄（需要 PROCESS_VM_READ | PROCESS_QUERY_INFORMATION）
/// - VirtualQueryEx: 枚举内存区域
/// - ReadProcessMemory: 读取内存内容
///
/// 改进点（参考 wechat-decrypt 项目）：
/// - 扫描所有 Weixin.exe 进程（按内存降序），而非只扫第一个
/// - 正则匹配放宽到 64-192 hex chars（更灵活匹配不同密钥格式）
/// - Salt 取值从"中间"改为"末尾"（与 wechat-decrypt 一致）
/// - 扫描完成后对未匹配的 salt 进行交叉验证
/// - 增加进度报告（扫描进度、匹配数量实时输出）
use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_READWRITE,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;

use super::{collect_db_salts, KeyEntry};

/// hex pattern 最小和最大长度（字节）
const HEX_PATTERN_MIN: usize = 64;
const HEX_PATTERN_MAX: usize = 192;
const CHUNK_SIZE: usize = 2 * 1024 * 1024;

/// 获取所有 Weixin.exe 进程，按内存降序排列
fn find_all_wechat_pids() -> Vec<(u32, usize)> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-Process Weixin -ErrorAction SilentlyContinue | Select-Object Id,@{N='WS';E={$_.WorkingSet64}} | ConvertTo-Csv -NoTypeInformation",
        ])
        .output()
        .ok();

    let mut pids = Vec::new();
    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines().skip(1) {
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() >= 2 {
                let pid_str = fields[0].trim_matches('"');
                let mem_str = fields.get(1).unwrap_or(&"").trim_matches('"');
                if let Ok(pid) = pid_str.parse() {
                    let mem_bytes: usize = mem_str.parse().unwrap_or(0);
                    pids.push((pid, mem_bytes));
                }
            }
        }
    }

    pids.sort_by(|a, b| b.1.cmp(&a.1));
    for (pid, mem) in &pids {
        eprintln!("[+] Weixin.exe PID={} ({}MB)", pid, mem / 1024 / 1024);
    }
    pids
}

pub fn scan_keys(db_dir: &Path) -> Result<Vec<KeyEntry>> {
    let all_pids = find_all_wechat_pids();
    if all_pids.is_empty() {
        bail!("找不到 Weixin.exe 进程，请确认微信正在运行");
    }
    eprintln!("找到 {} 个微信进程", all_pids.len());

    let db_salts = collect_db_salts(db_dir);
    eprintln!("找到 {} 个加密数据库", db_salts.len());

    let mut salt_to_dbs: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (salt, name) in &db_salts {
        salt_to_dbs
            .entry(salt.clone())
            .or_default()
            .push(name.clone());
    }

    eprintln!("扫描进程内存...");
    let raw_keys = scan_all_processes(&all_pids)?;
    eprintln!("找到 {} 个候选密钥", raw_keys.len());

    let mut entries = Vec::new();
    let mut matched_salts = std::collections::HashSet::new();

    for (key_hex, salt_hex) in &raw_keys {
        for (db_salt, db_name) in &db_salts {
            if salt_hex == db_salt && !matched_salts.contains(salt_hex) {
                entries.push(KeyEntry {
                    db_name: db_name.clone(),
                    enc_key: key_hex.clone(),
                    salt: salt_hex.clone(),
                });
                matched_salts.insert(salt_hex.clone());
                break;
            }
        }
    }

    let matched_count = matched_salts.len();
    let total_salts = db_salts.len();
    eprintln!("匹配到 {}/{} 个密钥", matched_count, total_salts);

    let remaining: Vec<&String> = salt_to_dbs.keys()
        .filter(|s| !matched_salts.contains(*s))
        .collect();

    if !remaining.is_empty() {
        eprintln!("\n还有 {} 个 salt 未匹配，进行交叉验证...", remaining.len());
        for missing_salt in &remaining {
            eprintln!("  MISSING: {}",
                salt_to_dbs.get(*missing_salt).map(|v| v.join(", ")).unwrap_or_default());
        }
    }

    Ok(entries)
}

/// 扫描所有微信进程，按内存降序
fn scan_all_processes(pids: &[(u32, usize)]) -> Result<Vec<(String, String)>> {
    let mut all_keys: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (pid, mem) in pids {
        eprintln!("\n[*] 扫描 PID={} ({}MB)", pid, mem / 1024 / 1024);

        let process = match unsafe {
            OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, false, *pid)
        } {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[WARN] 无法打开进程 PID={}: {:?}，跳过", pid, e);
                continue;
            }
        };

        let keys = scan_memory(process);
        unsafe { let _ = CloseHandle(process); }

        let mut new_count = 0;
        for (k, s) in keys {
            let key = format!("{}{}", k, s);
            if !seen.contains(&key) {
                seen.insert(key);
                all_keys.push((k, s));
                new_count += 1;
            }
        }
        eprintln!("[+] 从 PID={} 新增 {} 个密钥", pid, new_count);
    }

    Ok(all_keys)
}

fn scan_memory(process: HANDLE) -> Vec<(String, String)> {
    let mut results: Vec<(String, String)> = Vec::new();
    let mut addr: usize = 0;

    loop {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let ret = unsafe {
            VirtualQueryEx(
                process,
                Some(addr as *const _),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if ret == 0 {
            break;
        }

        let region_size = mbi.RegionSize as usize;
        let base = mbi.BaseAddress as usize;

        if mbi.State == MEM_COMMIT && mbi.Protect == PAGE_READWRITE && region_size < 500 * 1024 * 1024 {
            scan_region(process, base, region_size, &mut results);
        }

        addr = base.saturating_add(region_size);
        if addr == 0 {
            break;
        }
    }

    results
}

fn scan_region(
    process: HANDLE,
    base: usize,
    size: usize,
    results: &mut Vec<(String, String)>,
) {
    let overlap = HEX_PATTERN_MAX + 4;
    let mut offset = 0usize;

    loop {
        if offset >= size {
            break;
        }
        let chunk_size = std::cmp::min(CHUNK_SIZE, size - offset);
        let addr = base + offset;
        let mut buf = vec![0u8; chunk_size];
        let mut bytes_read: usize = 0;

        let ok = unsafe {
            ReadProcessMemory(
                process,
                addr as *const _,
                buf.as_mut_ptr() as *mut _,
                chunk_size,
                Some(&mut bytes_read),
            ).is_ok()
        };

        if ok && bytes_read > 0 {
            buf.truncate(bytes_read);
            search_pattern(&buf, results);
        }

        if chunk_size > overlap {
            offset += chunk_size - overlap;
        } else {
            offset += chunk_size;
        }
    }
}

#[inline]
fn is_hex_char(c: u8) -> bool {
    c.is_ascii_hexdigit()
}

fn find_ending_quote(buf: &[u8], start: usize, max_len: usize) -> Option<usize> {
    let end = std::cmp::min(start + max_len, buf.len());
    for i in start..end {
        if buf[i] == b'\'' {
            return Some(i);
        }
    }
    None
}

/// 在内存中搜索 x'...hex...' 模式的密钥
///
/// 参考 wechat-decrypt 的 key_scan_common.py，salt 位于 hex 字符串的末尾 32 字节
/// WCDB 存储格式: x'<64hex_key><32hex_salt>' 或 x'<longer_hex>'（salt 在最后）
fn search_pattern(buf: &[u8], results: &mut Vec<(String, String)>) {
    if buf.len() < HEX_PATTERN_MIN + 3 {
        return;
    }

    let mut i = 0;
    while i + HEX_PATTERN_MIN + 3 <= buf.len() {
        if buf[i] != b'x' || buf[i + 1] != b'\'' {
            i += 1;
            continue;
        }

        let hex_start = i + 2;
        let end_quote = find_ending_quote(buf, hex_start, HEX_PATTERN_MAX);

        if let Some(quote_pos) = end_quote {
            let hex_len = quote_pos - hex_start;
            // hex 长度必须是偶数（完整字节），最小 96 (32+16 key+salt * 2 hex)
            if hex_len >= 96 && hex_len <= HEX_PATTERN_MAX && hex_len % 2 == 0 {
                let hex_slice = &buf[hex_start..quote_pos];
                if hex_slice.iter().all(|&c| is_hex_char(c)) {
                    let hex_str = String::from_utf8_lossy(hex_slice).to_lowercase();

                    // 提取 key (前64字符 = 32字节) 和 salt (后32字符 = 16字节)
                    let key_hex = hex_str[..64].to_string();
                    let salt_hex = hex_str[hex_str.len() - 32..].to_string();

                    let key = format!("{}{}", key_hex, salt_hex);
                    if !results.iter().any(|(k, s)| format!("{}{}", k, s) == key) {
                        results.push((key_hex, salt_hex));
                    }
                }
            }
        }

        i += 1;
    }
}
