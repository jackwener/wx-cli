use anyhow::Result;

use crate::ipc::Request;
use super::history::{parse_time, parse_time_end};
use super::output::{print_value, resolve};
use super::transport;

/// `wx attachments` — 列出指定会话的附件消息（默认 image，可多选）。
///
/// 输出每条 `attachment_id`，再传给 `wx extract` 才真正读 message_resource.db
/// 与本地 .dat 解码。这一步只查 `Msg_<chat>` 表，几千条群聊也能秒返。
pub fn cmd_attachments(
    chat: String,
    kinds: Vec<String>,
    limit: usize,
    offset: usize,
    since: Option<String>,
    until: Option<String>,
    json: bool,
) -> Result<()> {
    let since_ts = since.as_deref().map(parse_time).transpose()?;
    let until_ts = until.as_deref().map(parse_time_end).transpose()?;

    // CLI 收上来的 Vec<String> 为空时按默认（image）走，让 daemon 决定 fallback。
    let kinds_param = if kinds.is_empty() { None } else { Some(kinds) };

    let req = Request::Attachments {
        chat,
        kinds: kinds_param,
        limit,
        offset,
        since: since_ts,
        until: until_ts,
    };
    let resp = transport::send(req)?;
    let data = resp
        .data
        .get("attachments")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    print_value(&data, &resolve(json))
}
