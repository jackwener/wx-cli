//! 把 `AttachmentId` 翻译成本地 `.dat` 路径。
//!
//! 流程：
//!   1. `chat` username → `ChatName2Id.rowid`（资源库）
//!   2. `(chat_id, local_id)` + `ORDER BY message_create_time DESC LIMIT 1` →
//!      `MessageResourceInfo.packed_info`
//!   3. 从 `packed_info` (protobuf) 提取 32 字节 ASCII hex MD5
//!   4. 在 `<wxchat_base>/msg/attach/<md5(chat)>/<YYYY-MM>/Img/<md5>[_t|_h].dat`
//!      下找对应文件，按 full > _h > _t 优先级选一个
//!
//! `<wxchat_base>` 由 daemon 已知（同 `db_dir` 的父目录），路径 layout 平台差异：
//! - Linux: `~/Documents/xwechat_files/<wxid>`
//! - macOS: `~/Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files/<wxid>`
//!   ⚠️  msg/attach/... 子树 layout 待我用真实账号验证；上游 docstring 只写了 Windows
//! - Windows: `<root>\xwechat_files\<wxid>`（root 从 `%APPDATA%\Tencent\xwechat\config\*.ini` 读）

use anyhow::{anyhow, Context, Result};
use chrono::TimeZone;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

use super::AttachmentId;

/// 单条 attachment 在资源库 + 本地 attach 树下的解析结果。
#[derive(Debug, Clone)]
pub struct ResolvedAttachment {
    pub id: AttachmentId,
    /// 从 `packed_info` 提取出的资源 MD5（小写 hex）
    pub md5: String,
    /// 命中的本地 .dat 路径（按 full > _h > _t 优先级选一个）
    pub dat_path: PathBuf,
    /// 文件 size（字节）
    pub size: u64,
}

/// 仅 schema lookup（不去找本地 .dat）。
/// 用于 `wx attachments` 列表时填 `md5` 字段——文件可能根本不在本地。
#[derive(Debug, Clone)]
pub struct AttachmentMetadata {
    pub md5: String,
}

/// 用 `(chat, local_id)` 查 message_resource.db 拿 file md5。
///
/// 调用方传已经解密好的 `message_resource.db` 路径（由 daemon 的 `DBCache` 准备）。
/// 同步函数 — caller 在 `spawn_blocking` 里跑。
pub fn lookup_md5_blocking(
    resource_db_path: &Path,
    chat: &str,
    local_id: i64,
    create_time: i64,
    msg_local_type_lo32: i64,
) -> Result<Option<AttachmentMetadata>> {
    let conn = Connection::open_with_flags(
        resource_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .with_context(|| format!("打开 message_resource.db {:?}", resource_db_path))?;

    // 1) ChatName2Id: user_name -> rowid
    let chat_id: Option<i64> = conn
        .query_row(
            "SELECT rowid FROM ChatName2Id WHERE user_name = ?1",
            [chat],
            |row| row.get(0),
        )
        .ok();
    let Some(chat_id) = chat_id else {
        return Ok(None);
    };

    // 2) MessageResourceInfo:
    //    同 chat 内 local_id 会复用，所以先用 create_time 精确命中；
    //    若资源库里的时间戳跟 message_N.db 不完全对齐，再 fallback 到“同 local_id/type 取最新”
    //    message_local_type 高 32 bit 是版本/会话 flag，低 32 bit 才是真实类型
    let packed_exact: Option<Vec<u8>> = conn
        .query_row(
            "SELECT packed_info FROM MessageResourceInfo
             WHERE chat_id = ?1
               AND message_local_id = ?2
               AND (message_local_type = ?3 OR message_local_type % 4294967296 = ?3)
               AND message_create_time = ?4
             ORDER BY rowid DESC
             LIMIT 1",
            rusqlite::params![chat_id, local_id, msg_local_type_lo32, create_time],
            |row| row.get(0),
        )
        .ok();

    let packed: Option<Vec<u8>> = packed_exact.or_else(|| conn
        .query_row(
            "SELECT packed_info FROM MessageResourceInfo
             WHERE chat_id = ?1
               AND message_local_id = ?2
               AND (message_local_type = ?3 OR message_local_type % 4294967296 = ?3)
             ORDER BY message_create_time DESC
             LIMIT 1",
            rusqlite::params![chat_id, local_id, msg_local_type_lo32],
            |row| row.get(0),
        )
        .ok());

    let Some(blob) = packed else {
        return Ok(None);
    };
    Ok(extract_md5_from_packed_info(&blob).map(|md5| AttachmentMetadata { md5 }))
}

/// 从 `MessageResourceInfo.packed_info` (protobuf) 提取 32 字节 ASCII hex md5。
///
/// 主路径：搜 4 字节 marker `12 22 0a 20`（field=2 LEN, length=34, sub field=1 LEN, length=32），
/// 紧跟 32 字节 ASCII hex。
/// Fallback：扫整个 blob 找连续 32 字节合法 hex 字符。
pub fn extract_md5_from_packed_info(blob: &[u8]) -> Option<String> {
    const MARKER: &[u8; 4] = &[0x12, 0x22, 0x0A, 0x20];

    // 主路径
    if let Some(pos) = find_subslice(blob, MARKER) {
        let start = pos + MARKER.len();
        if start + 32 <= blob.len() {
            if let Ok(s) = std::str::from_utf8(&blob[start..start + 32]) {
                if s.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(s.to_ascii_lowercase());
                }
            }
        }
    }

    // Fallback：连续 32 字节合法 hex
    if blob.len() >= 32 {
        for start in 0..=blob.len() - 32 {
            let chunk = &blob[start..start + 32];
            if let Ok(s) = std::str::from_utf8(chunk) {
                if s.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(s.to_ascii_lowercase());
                }
            }
        }
    }
    None
}

/// 简单的子串扫描（避免拉 memchr/memmem 依赖；blob 通常 < 1KB）
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

/// 在 `<attach_root>/<md5(chat)>/<YYYY-MM>/Img/<md5>[_t|_h].dat` 下找文件。
///
/// 优先级：full > `_h`（HD thumbnail）> `_t`（thumbnail）。返回最优的一个；
/// 找不到返回 None。
///
/// `attach_root` = `<wxchat_base>/msg/attach`。
/// `create_time` 用于先定位 `<YYYY-MM>` 子目录；找不到时再 fallback 全月份扫描，
/// 因为 WeChat 的 `YYYY-MM` 目录有时跟消息时间差 1 个月（按收到时间归档）。
pub fn find_dat_file(
    attach_root: &Path,
    chat: &str,
    file_md5: &str,
    create_time: i64,
) -> Option<PathBuf> {
    let chat_hash = format!("{:x}", md5::compute(chat.as_bytes()));
    let chat_dir = attach_root.join(&chat_hash);
    if !chat_dir.is_dir() {
        return None;
    }

    // 第一步：试 create_time 当月 + 前后各一个月（共 3 个候选目录）
    let candidates_ym: Vec<String> = three_month_candidates(create_time);
    for ym in &candidates_ym {
        let img_dir = chat_dir.join(ym).join("Img");
        if let Some(p) = pick_best_in_img_dir(&img_dir, file_md5) {
            return Some(p);
        }
    }

    // 第二步 fallback：扫整个 chat_dir 的所有月份子目录
    let entries = std::fs::read_dir(&chat_dir).ok()?;
    let mut all_months: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // 已经试过的 3 个候选可以跳过，但成本极小；保留全量扫
    all_months.sort();
    for month_dir in all_months {
        let img_dir = month_dir.join("Img");
        if let Some(p) = pick_best_in_img_dir(&img_dir, file_md5) {
            return Some(p);
        }
    }
    None
}

fn pick_best_in_img_dir(img_dir: &Path, file_md5: &str) -> Option<PathBuf> {
    if !img_dir.is_dir() {
        return None;
    }
    let full = img_dir.join(format!("{}.dat", file_md5));
    if full.is_file() {
        return Some(full);
    }
    let hd = img_dir.join(format!("{}_h.dat", file_md5));
    if hd.is_file() {
        return Some(hd);
    }
    let thumb = img_dir.join(format!("{}_t.dat", file_md5));
    if thumb.is_file() {
        return Some(thumb);
    }
    None
}

fn three_month_candidates(unix_ts: i64) -> Vec<String> {
    use chrono::{Datelike, Duration};
    let dt = match chrono::Local.timestamp_opt(unix_ts, 0).single() {
        Some(d) => d,
        None => return Vec::new(),
    };
    let prev = dt - Duration::days(31);
    let next = dt + Duration::days(31);
    [prev, dt, next]
        .iter()
        .map(|d| format!("{:04}-{:02}", d.year(), d.month()))
        .collect()
}

/// 把 `<wxchat_base>` （即 `db_storage` 父目录）拼成 `<base>/msg/attach`。
pub fn attach_root_for(wxchat_base: &Path) -> PathBuf {
    wxchat_base.join("msg").join("attach")
}

/// 完整流程：用 `attachment_id` 拿 md5 + 找 .dat。失败返回带具体诊断信息的 `Err`。
///
/// `resource_db_path` 由 daemon 提供（DBCache 已经解密好）；
/// `attach_root` 由 caller 拼好（`attach_root_for(wxchat_base)`）。
/// 同步函数 — caller 在 `spawn_blocking` 里跑。
pub fn resolve_blocking(
    id: &AttachmentId,
    resource_db_path: &Path,
    attach_root: &Path,
) -> Result<ResolvedAttachment> {
    let lo32_type: i64 = match id.kind {
        super::AttachmentKind::Image => 3,
        super::AttachmentKind::Voice => 34,
        super::AttachmentKind::Video => 43,
        super::AttachmentKind::File => 49,
    };

    let meta = lookup_md5_blocking(
        resource_db_path,
        &id.chat,
        id.local_id,
        id.create_time,
        lo32_type,
    )?
        .ok_or_else(|| {
            anyhow!(
                "message_resource.db 中找不到 chat={} local_id={} type={} 的资源行（可能是非附件消息或资源库未同步）",
                id.chat,
                id.local_id,
                lo32_type
            )
        })?;

    let dat_path = find_dat_file(attach_root, &id.chat, &meta.md5, id.create_time).ok_or_else(
        || {
            anyhow!(
                "找不到本地 .dat（md5={} chat={} create_time={}）— 微信可能尚未下载该附件，或附件已被清理",
                meta.md5,
                id.chat,
                id.create_time
            )
        },
    )?;
    let size = std::fs::metadata(&dat_path).map(|m| m.len()).unwrap_or(0);

    Ok(ResolvedAttachment { id: id.clone(), md5: meta.md5, dat_path, size })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_md5_main_path() {
        // 构造一段含 12 22 0a 20 marker 的 blob
        let mut blob = vec![0xAA, 0xBB, 0xCC];
        blob.extend_from_slice(&[0x12, 0x22, 0x0A, 0x20]);
        blob.extend_from_slice(b"deadbeefcafebabe1234567890abcdef");
        blob.extend_from_slice(&[0xFF, 0xFF]);
        assert_eq!(
            extract_md5_from_packed_info(&blob),
            Some("deadbeefcafebabe1234567890abcdef".to_string())
        );
    }

    #[test]
    fn extract_md5_fallback_no_marker() {
        // 没有 marker，但 blob 里有合法 32 字节 hex
        let mut blob = vec![0xFF, 0x00];
        blob.extend_from_slice(b"00112233445566778899aabbccddeeff");
        blob.extend_from_slice(&[0x01]);
        assert_eq!(
            extract_md5_from_packed_info(&blob),
            Some("00112233445566778899aabbccddeeff".to_string())
        );
    }

    #[test]
    fn extract_md5_uppercase_normalized_to_lower() {
        let mut blob = vec![0x12, 0x22, 0x0A, 0x20];
        blob.extend_from_slice(b"DEADBEEFCAFEBABE1234567890ABCDEF");
        // 上游/CI/本地 file md5 都是 lowercase；强制小写化避免大小写不一致导致命中失败
        assert_eq!(
            extract_md5_from_packed_info(&blob),
            Some("deadbeefcafebabe1234567890abcdef".to_string())
        );
    }

    #[test]
    fn extract_md5_returns_none_on_garbage() {
        let blob = vec![0; 16];
        assert!(extract_md5_from_packed_info(&blob).is_none());
    }

    #[test]
    fn lookup_md5_prefers_exact_create_time_over_latest_reuse() {
        let dir = tempdir_for_test();
        let db_path = dir.join("message_resource.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE ChatName2Id (user_name TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ChatName2Id (rowid, user_name) VALUES (1, 'room@chatroom')",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE MessageResourceInfo (
                chat_id INTEGER,
                message_local_id INTEGER,
                message_local_type INTEGER,
                message_create_time INTEGER,
                packed_info BLOB
            )",
            [],
        )
        .unwrap();

        let old_blob = {
            let mut blob = vec![0x12, 0x22, 0x0A, 0x20];
            blob.extend_from_slice(b"11111111111111111111111111111111");
            blob
        };
        let new_blob = {
            let mut blob = vec![0x12, 0x22, 0x0A, 0x20];
            blob.extend_from_slice(b"22222222222222222222222222222222");
            blob
        };

        conn.execute(
            "INSERT INTO MessageResourceInfo
             (chat_id, message_local_id, message_local_type, message_create_time, packed_info)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![1i64, 7i64, 3i64, 1000i64, old_blob],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO MessageResourceInfo
             (chat_id, message_local_id, message_local_type, message_create_time, packed_info)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![1i64, 7i64, 3i64, 2000i64, new_blob],
        )
        .unwrap();

        let old = lookup_md5_blocking(&db_path, "room@chatroom", 7, 1000, 3)
            .unwrap()
            .unwrap();
        let new = lookup_md5_blocking(&db_path, "room@chatroom", 7, 2000, 3)
            .unwrap()
            .unwrap();
        assert_eq!(old.md5, "11111111111111111111111111111111");
        assert_eq!(new.md5, "22222222222222222222222222222222");
    }

    #[test]
    fn three_month_candidates_includes_prev_curr_next() {
        // 2025-08-15 (mid-month) → 2025-07, 2025-08, 2025-09
        let ts = chrono::Local
            .with_ymd_and_hms(2025, 8, 15, 12, 0, 0)
            .unwrap()
            .timestamp();
        let v = three_month_candidates(ts);
        assert!(v.contains(&"2025-07".to_string()));
        assert!(v.contains(&"2025-08".to_string()));
        assert!(v.contains(&"2025-09".to_string()));
    }

    #[test]
    fn pick_best_prefers_full_then_h_then_t() {
        let tmp = tempdir_for_test();
        let img = tmp.join("Img");
        std::fs::create_dir_all(&img).unwrap();
        let md5 = "abcd1234";
        std::fs::write(img.join(format!("{}_t.dat", md5)), b"thumb").unwrap();
        std::fs::write(img.join(format!("{}_h.dat", md5)), b"hd").unwrap();
        // 只有 _t / _h 时取 _h
        assert_eq!(
            pick_best_in_img_dir(&img, md5).unwrap().file_name().unwrap(),
            format!("{}_h.dat", md5).as_str()
        );
        // 加 full 后取 full
        std::fs::write(img.join(format!("{}.dat", md5)), b"full").unwrap();
        assert_eq!(
            pick_best_in_img_dir(&img, md5).unwrap().file_name().unwrap(),
            format!("{}.dat", md5).as_str()
        );
    }

    fn tempdir_for_test() -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("wx-cli-attach-test-{}-{}", pid, nanos));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
