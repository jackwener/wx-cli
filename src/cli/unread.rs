use anyhow::Result;
use crate::ipc::Request;
use super::transport;
use super::output::{resolve, print_value};

pub fn cmd_unread(limit: usize, filter: Vec<String>, json: bool) -> Result<()> {
    // 空或含 "all" 视为不过滤；其他值已被 clap value_parser 验证过，直接透传给 daemon。
    let filter_vec = if filter.is_empty() || filter.iter().any(|s| s == "all") {
        None
    } else {
        Some(filter)
    };
    let resp = transport::send(Request::Unread { limit, filter: filter_vec })?;
    let data = resp.data.get("sessions")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    print_value(&data, &resolve(json))
}
