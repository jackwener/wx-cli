/// macOS WeChat 进程内存密钥扫描器
///
/// 翻译自 find_all_keys_macos.c，使用 Mach VM API：
/// - task_for_pid: 获取目标进程的 task port（需要 root 权限）
/// - mach_vm_region: 枚举内存区域
/// - mach_vm_read: 读取内存块
///
/// 注意：
/// 1. 需要以 root (sudo) 运行
/// 2. WeChat 需要进行 ad-hoc 签名
/// 3. 在内存中搜索 x'<64hex><32hex>' 格式的 SQLCipher 密钥
use anyhow::{bail, Context, Result};
use std::path::Path;

use super::{collect_db_salts, KeyEntry};

// Mach 相关常量
const KERN_SUCCESS: i32 = 0;
const VM_PROT_READ: i32 = 1;
const VM_PROT_WRITE: i32 = 2;
const VM_REGION_BASIC_INFO_64: i32 = 9;
const CHUNK_SIZE: usize = 2 * 1024 * 1024; // 2MB
const HEX_PATTERN_LEN: usize = 96; // 64(key) + 32(salt)

// vm_region_basic_info_64 结构体
#[repr(C)]
struct VmRegionBasicInfo64 {
    protection: i32,
    max_protection: i32,
    inheritance: u32,
    shared: u32,
    reserved: u32,
    _offset: u64,
    behavior: i32,
    user_wired_count: u16,
}

// Mach FFI 声明
#[allow(non_camel_case_types)]
type kern_return_t = i32;
#[allow(non_camel_case_types)]
type mach_port_t = u32;
#[allow(non_camel_case_types)]
type mach_vm_address_t = u64;
#[allow(non_camel_case_types)]
type mach_vm_size_t = u64;
#[allow(non_camel_case_types)]
type mach_msg_type_number_t = u32;
#[allow(non_camel_case_types)]
type vm_offset_t = usize;
#[allow(non_camel_case_types, dead_code)]
type vm_prot_t = i32;

extern "C" {
    fn mach_task_self() -> mach_port_t;
    fn task_for_pid(host: mach_port_t, pid: libc::pid_t, task: *mut mach_port_t) -> kern_return_t;
    fn mach_vm_region(
        task: mach_port_t,
        address: *mut mach_vm_address_t,
        size: *mut mach_vm_size_t,
        flavor: i32,
        info: *mut VmRegionBasicInfo64,
        info_count: *mut mach_msg_type_number_t,
        obj_name: *mut mach_port_t,
    ) -> kern_return_t;
    fn mach_vm_read(
        task: mach_port_t,
        addr: mach_vm_address_t,
        size: mach_vm_size_t,
        data: *mut vm_offset_t,
        data_cnt: *mut mach_msg_type_number_t,
    ) -> kern_return_t;
    fn mach_vm_deallocate(
        task: mach_port_t,
        addr: mach_vm_address_t,
        size: mach_vm_size_t,
    ) -> kern_return_t;
}

/// 查找 WeChat 进程的 PID
fn find_wechat_pid() -> Option<libc::pid_t> {
    // 使用 pgrep -x WeChat 查找（与 C 版本一致）
    let output = std::process::Command::new("pgrep")
        .args(["-x", "WeChat"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout);
    s.trim().parse().ok()
}

/// 判断字节是否是 ASCII 十六进制字符
#[inline]
fn is_hex_char(c: u8) -> bool {
    c.is_ascii_hexdigit()
}

pub fn scan_keys(db_dir: &Path) -> Result<Vec<KeyEntry>> {
    // 1. 查找 WeChat PID
    let pid = find_wechat_pid()
        .context("找不到 WeChat 进程，请确认 WeChat 正在运行")?;
    eprintln!("WeChat PID: {}", pid);

    // 2. 获取 task port
    // SAFETY: task_for_pid 是标准 Mach API，参数合法
    let task = unsafe {
        let mut task: mach_port_t = 0;
        let kr = task_for_pid(mach_task_self(), pid, &mut task);
        if kr != KERN_SUCCESS {
            bail!(
                "task_for_pid 失败 (kr={})。请按以下步骤修复：\n\
                \n\
                  1. 对 WeChat 重新签名（只需做一次）：\n\
                     codesign --force --deep --sign - /Applications/WeChat.app\n\
                \n\
                  2. 重启 WeChat：\n\
                     killall WeChat && open /Applications/WeChat.app\n\
                \n\
                  3. 再次运行（需要 root）：\n\
                     sudo wx init\n\
                \n\
                如果 codesign 报 \"signature in use\"，先执行：\n\
                     codesign --remove-signature /Applications/WeChat.app/Contents/Frameworks/vlc_plugins/librtp_mpeg4_plugin.dylib\n\
                     codesign --force --deep --sign - /Applications/WeChat.app",
                kr
            );
        }
        task
    };
    eprintln!("Got task port: {}", task);

    // 3. 收集数据库 salt 映射
    eprintln!("扫描数据库文件...");
    let db_salts = collect_db_salts(db_dir);
    eprintln!("找到 {} 个加密数据库", db_salts.len());

    // 4. 扫描进程内存
    eprintln!("扫描进程内存寻找密钥...");
    let raw_keys = scan_memory(task)?;
    eprintln!("找到 {} 个候选密钥", raw_keys.len());

    // 5. 将密钥与数据库 salt 匹配
    let mut entries = Vec::new();
    for (key_hex, salt_hex) in &raw_keys {
        for (db_salt, db_name) in &db_salts {
            if salt_hex == db_salt {
                entries.push(KeyEntry {
                    db_name: db_name.clone(),
                    enc_key: key_hex.clone(),
                    salt: salt_hex.clone(),
                });
                break;
            }
        }
    }

    eprintln!("匹配到 {}/{} 个密钥", entries.len(), raw_keys.len());
    Ok(entries)
}

/// 扫描进程内存，返回 (key_hex, salt_hex) 列表
fn scan_memory(task: mach_port_t) -> Result<Vec<(String, String)>> {
    let mut results: Vec<(String, String)> = Vec::new();
    let mut addr: mach_vm_address_t = 0;

    // VM_REGION_BASIC_INFO_COUNT_64 = 9（来自 <mach/vm_region.h>，固定值，不能用 sizeof 计算）
    let info_count_expected: mach_msg_type_number_t = 9;

    loop {
        let mut size: mach_vm_size_t = 0;
        let mut info = VmRegionBasicInfo64 {
            protection: 0, max_protection: 0, inheritance: 0,
            shared: 0, reserved: 0, _offset: 0, behavior: 0, user_wired_count: 0,
        };
        let mut info_count: mach_msg_type_number_t = info_count_expected;
        let mut obj_name: mach_port_t = 0;

        // SAFETY: mach_vm_region 枚举虚拟内存区域，所有参数合法
        let kr = unsafe {
            mach_vm_region(
                task,
                &mut addr,
                &mut size,
                VM_REGION_BASIC_INFO_64,
                &mut info,
                &mut info_count,
                &mut obj_name,
            )
        };

        if kr != KERN_SUCCESS {
            break;
        }
        if size == 0 {
            addr = addr.saturating_add(1);
            continue;
        }

        // 只扫描可读可写区域（密钥通常存在于堆内存）
        if (info.protection & (VM_PROT_READ | VM_PROT_WRITE)) == (VM_PROT_READ | VM_PROT_WRITE) {
            scan_region(task, addr, size, &mut results);
        }

        addr = addr.saturating_add(size);
    }

    Ok(results)
}

/// 扫描单个内存区域，按 CHUNK_SIZE 分块读取
fn scan_region(
    task: mach_port_t,
    addr: mach_vm_address_t,
    size: mach_vm_size_t,
    results: &mut Vec<(String, String)>,
) {
    let end = addr + size;
    let mut ca = addr;

    while ca < end {
        let cs = std::cmp::min(end - ca, CHUNK_SIZE as u64);

        let mut data: vm_offset_t = 0;
        let mut dc: mach_msg_type_number_t = 0;

        // SAFETY: mach_vm_read 读取目标进程内存到内核缓冲区，
        // 返回的 data 指针指向通过 vm_allocate 分配的内存，
        // 必须用 mach_vm_deallocate 释放
        let kr = unsafe {
            mach_vm_read(task, ca, cs, &mut data, &mut dc)
        };

        if kr == KERN_SUCCESS {
            // SAFETY: data 是 mach_vm_read 返回的有效指针，dc 是字节数
            let buf: &[u8] = unsafe {
                std::slice::from_raw_parts(data as *const u8, dc as usize)
            };

            search_pattern(buf, results);

            // SAFETY: 释放 mach_vm_read 分配的内核内存
            unsafe {
                mach_vm_deallocate(mach_task_self(), data as u64, dc as u64);
            }
        }

        // 保留 (HEX_PATTERN_LEN + 3) 字节重叠以处理跨块边界的模式
        let overlap = HEX_PATTERN_LEN + 3;
        if cs as usize > overlap {
            ca += cs - overlap as u64;
        } else {
            ca += cs;
        }
    }
}

/// 在缓冲区中搜索 x'<96个十六进制字符>' 模式
///
/// 格式：x'<64hex(key)><32hex(salt)>'（总计 99 字节）
pub(crate) fn search_pattern(buf: &[u8], results: &mut Vec<(String, String)>) {
    let total = HEX_PATTERN_LEN + 3; // x' + 96 hex + '
    if buf.len() < total {
        return;
    }

    let mut i = 0;
    while i + total <= buf.len() {
        if buf[i] != b'x' || buf[i + 1] != b'\'' {
            i += 1;
            continue;
        }

        // 验证后续 96 字节都是十六进制字符
        let hex_start = i + 2;
        let all_hex = buf[hex_start..hex_start + HEX_PATTERN_LEN]
            .iter()
            .all(|&c| is_hex_char(c));

        if !all_hex {
            i += 1;
            continue;
        }

        // 验证结尾的单引号
        if buf[hex_start + HEX_PATTERN_LEN] != b'\'' {
            i += 1;
            continue;
        }

        // 提取 key_hex 和 salt_hex，统一转小写
        let key_hex = String::from_utf8_lossy(&buf[hex_start..hex_start + 64])
            .to_lowercase();
        let salt_hex = String::from_utf8_lossy(&buf[hex_start + 64..hex_start + 96])
            .to_lowercase();

        // 去重检查
        let is_dup = results.iter().any(|(k, s)| k == &key_hex && s == &salt_hex);
        if !is_dup {
            results.push((key_hex, salt_hex));
        }

        i += total;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一条合法的 x'<key><salt>' 模式字节串
    fn make_pattern(key: &[u8; 64], salt: &[u8; 32]) -> Vec<u8> {
        let mut v = vec![b'x', b'\''];
        v.extend_from_slice(key);
        v.extend_from_slice(salt);
        v.push(b'\'');
        v
    }

    #[test]
    fn test_is_hex_char_valid() {
        for c in b'0'..=b'9' { assert!(is_hex_char(c), "digit {}", c as char); }
        for c in b'a'..=b'f' { assert!(is_hex_char(c), "lower {}", c as char); }
        for c in b'A'..=b'F' { assert!(is_hex_char(c), "upper {}", c as char); }
    }

    #[test]
    fn test_is_hex_char_invalid() {
        for c in [b'g', b'G', b'x', b'\'', b' ', b'\0', b'z', b'Z'] {
            assert!(!is_hex_char(c), "expected non-hex: {}", c as char);
        }
    }

    #[test]
    fn test_search_pattern_basic() {
        let key  = [b'a'; 64];
        let salt = [b'b'; 32];
        let buf = make_pattern(&key, &salt);
        let mut results = Vec::new();
        search_pattern(&buf, &mut results);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "a".repeat(64));
        assert_eq!(results[0].1, "b".repeat(32));
    }

    #[test]
    fn test_search_pattern_uppercase_lowercased() {
        // 大写十六进制字符应被统一转为小写
        let key  = [b'A'; 64];
        let salt = [b'B'; 32];
        let buf = make_pattern(&key, &salt);
        let mut results = Vec::new();
        search_pattern(&buf, &mut results);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "a".repeat(64));
        assert_eq!(results[0].1, "b".repeat(32));
    }

    #[test]
    fn test_search_pattern_not_all_hex() {
        // 96 个十六进制字符中有一个非法字符 → 不匹配
        let mut buf = vec![b'x', b'\''];
        buf.extend_from_slice(&[b'a'; 95]);
        buf.push(b'g'); // 'g' 不是合法十六进制字符
        buf.push(b'\'');
        let mut results = Vec::new();
        search_pattern(&buf, &mut results);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_pattern_wrong_closing_quote() {
        // 结尾引号错误 → 不匹配
        let mut buf = vec![b'x', b'\''];
        buf.extend_from_slice(&[b'a'; 96]);
        buf.push(b'"'); // 应为 b'\''
        let mut results = Vec::new();
        search_pattern(&buf, &mut results);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_pattern_dedup() {
        // 相同模式出现两次 → 只保留一条
        let key  = [b'1'; 64];
        let salt = [b'2'; 32];
        let pattern = make_pattern(&key, &salt);
        let mut buf = pattern.clone();
        buf.extend_from_slice(&pattern);
        let mut results = Vec::new();
        search_pattern(&buf, &mut results);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_pattern_multiple_distinct() {
        // 两个不同的合法模式 → 各自独立捕获
        let key1  = [b'a'; 64]; let salt1 = [b'b'; 32];
        let key2  = [b'c'; 64]; let salt2 = [b'd'; 32];
        let mut buf = make_pattern(&key1, &salt1);
        buf.extend_from_slice(&make_pattern(&key2, &salt2));
        let mut results = Vec::new();
        search_pattern(&buf, &mut results);
        assert_eq!(results.len(), 2);
        let keys: Vec<&str> = results.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"a".repeat(64).as_str()));
        assert!(keys.contains(&"c".repeat(64).as_str()));
    }

    #[test]
    fn test_search_pattern_embedded_in_garbage() {
        // 模式夹在垃圾字节中间，仍应找到
        let mut buf = vec![0xFFu8; 50];
        let key  = [b'e'; 64];
        let salt = [b'f'; 32];
        buf.extend_from_slice(&make_pattern(&key, &salt));
        buf.extend_from_slice(&[0x00u8; 50]);
        let mut results = Vec::new();
        search_pattern(&buf, &mut results);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_pattern_too_short() {
        // 缓冲区太小，无法容纳完整模式
        let buf = [b'x', b'\'', b'a', b'b'];
        let mut results = Vec::new();
        search_pattern(&buf, &mut results);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_pattern_empty_buf() {
        let mut results = Vec::new();
        search_pattern(&[], &mut results);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_pattern_real_hex_mix() {
        // 合法的混合大小写十六进制（0-9, a-f, A-F）
        let mut key = [b'0'; 64];
        for (i, c) in b"0123456789abcdefABCDEF0123456789abcdef0123456789abcdef01234567".iter().enumerate() {
            if i < 64 { key[i] = *c; }
        }
        let salt = [b'9'; 32];
        let buf = make_pattern(&key, &salt);
        let mut results = Vec::new();
        search_pattern(&buf, &mut results);
        assert_eq!(results.len(), 1);
        // 结果应全小写
        assert!(results[0].0.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }
}
