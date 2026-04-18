use anyhow::Result;
use crate::ipc::Request;
use super::history::{parse_time, parse_time_end};
use super::transport;
use super::output::{resolve, print_value};

pub fn cmd_sns_notifications(
    limit: usize,
    since: Option<String>,
    until: Option<String>,
    include_read: bool,
    json: bool,
) -> Result<()> {
    let since_ts = since.as_deref().map(parse_time).transpose()?;
    let until_ts = until.as_deref().map(parse_time_end).transpose()?;

    let req = Request::SnsNotifications {
        limit,
        since: since_ts,
        until: until_ts,
        include_read,
    };
    let resp = transport::send(req)?;
    let data = resp.data.get("notifications")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    print_value(&data, &resolve(json))
}
