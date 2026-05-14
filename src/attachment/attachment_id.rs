//! 不透明附件 ID — 跨 CLI / IPC 的圆 trip 句柄。
//!
//! 编码：`base64url_no_pad(serde_json(payload))`。
//! 选择 base64url(json) 而不是紧凑 bit-pack：
//! - phase 1 求稳，不发明二进制协议
//! - 后面加字段（`resource_md5` / `decoder_hint` 之类）老 CLI 不 break
//! - debug 直接 base64 -d | jq 看字段
//!
//! ⚠️ `local_id` 在同一 chat 内会被 WeChat 复用（实测同 chat 最多 7 条同 local_id），
//! 所以 `(chat, local_id, create_time)` 三元组才是定位资源行的最小集。

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Image,
    Video,
    File,
    Voice,
}

impl AttachmentKind {
    /// 从 message.local_type 推 attachment kind（只覆盖 phase 1 关心的几种）。
    /// 高 32 bit 是版本/会话 flag，要先 mask 到低 32 bit。
    pub fn from_local_type(local_type: i64) -> Option<Self> {
        let lo = (local_type as u64) & 0xFFFF_FFFF;
        match lo {
            3 => Some(AttachmentKind::Image),
            34 => Some(AttachmentKind::Voice),
            43 => Some(AttachmentKind::Video),
            // type=49 是 appmsg，里面 subtype=6 才是文件；这里偏宽松返回 File，
            // 由 resolver 进一步根据 appmsg subtype 决定是否真的能 extract
            49 => Some(AttachmentKind::File),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AttachmentKind::Image => "image",
            AttachmentKind::Video => "video",
            AttachmentKind::File => "file",
            AttachmentKind::Voice => "voice",
        }
    }
}

/// 附件 ID payload（序列化后 base64url 编码）。
///
/// `v` 是版本字段，将来 schema 变了可以走分支兼容。当前 v=1。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentId {
    /// payload schema version
    pub v: u32,
    /// 会话 username（同时用于 ChatName2Id 查 chat_id 和拼 attach 路径）
    pub chat: String,
    /// 消息行的 local_id
    pub local_id: i64,
    /// 消息行的 create_time（unix 秒）— 用于 disambiguate 同 chat 内 local_id 复用
    pub create_time: i64,
    /// 附件类别
    pub kind: AttachmentKind,
    /// 可选 hint：消息所在 message_N.db 的 N。给定时 resolver 可跳过 shard 扫描；
    /// 缺省时 resolver 会按 `find_msg_tables` 逻辑全量扫
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db: Option<u8>,
}

impl AttachmentId {
    pub fn encode(&self) -> Result<String> {
        let json = serde_json::to_vec(self).context("序列化 AttachmentId")?;
        Ok(URL_SAFE_NO_PAD.encode(json))
    }

    pub fn decode(s: &str) -> Result<Self> {
        let bytes = URL_SAFE_NO_PAD
            .decode(s.trim())
            .map_err(|e| anyhow!("attachment_id 不是合法 base64url: {}", e))?;
        let id: AttachmentId =
            serde_json::from_slice(&bytes).context("attachment_id payload 非合法 JSON")?;
        if id.v != 1 {
            return Err(anyhow!("不支持的 attachment_id 版本 v={}", id.v));
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_minimal() {
        let id = AttachmentId {
            v: 1,
            chat: "wxid_abc".to_string(),
            local_id: 12345,
            create_time: 1_715_678_901,
            kind: AttachmentKind::Image,
            db: None,
        };
        let s = id.encode().unwrap();
        let back = AttachmentId::decode(&s).unwrap();
        assert_eq!(back.chat, id.chat);
        assert_eq!(back.local_id, id.local_id);
        assert_eq!(back.create_time, id.create_time);
        assert_eq!(back.kind, id.kind);
        assert_eq!(back.db, id.db);
    }

    #[test]
    fn round_trip_with_db_hint() {
        let id = AttachmentId {
            v: 1,
            chat: "1234@chatroom".to_string(),
            local_id: 42,
            create_time: 1,
            kind: AttachmentKind::Image,
            db: Some(2),
        };
        let s = id.encode().unwrap();
        assert!(!s.contains('=')); // base64url no-pad
        let back = AttachmentId::decode(&s).unwrap();
        assert_eq!(back.db, Some(2));
    }

    #[test]
    fn local_type_mask_high_bits() {
        // monitor_web.py 里 image push 路径：高位带 flag，低 32 bit 是 3
        let high_flag = (0xDEAD_BEEFu64 << 32) as i64 | 3;
        assert_eq!(
            AttachmentKind::from_local_type(high_flag),
            Some(AttachmentKind::Image)
        );
    }

    #[test]
    fn rejects_unknown_version() {
        let id = AttachmentId {
            v: 99,
            chat: "x".to_string(),
            local_id: 0,
            create_time: 0,
            kind: AttachmentKind::Image,
            db: None,
        };
        let s = id.encode().unwrap();
        assert!(AttachmentId::decode(&s).is_err());
    }
}
