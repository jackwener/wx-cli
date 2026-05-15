use anyhow::Result;
use crate::ipc::Request;
use super::transport;
use super::history::{parse_time, parse_time_end};
use super::output::{resolve, print_value};

pub fn cmd_stats(
    chat: String,
    since: Option<String>,
    until: Option<String>,
    json: bool,
    tcp_addr: Option<&str>,
) -> Result<()> {
    let since_ts = since.as_deref().map(parse_time).transpose()?;
    let until_ts = until.as_deref().map(parse_time_end).transpose()?;

    let resp = transport::send(Request::Stats { chat, since: since_ts, until: until_ts }, tcp_addr)?;
    print_value(&resp.data, &resolve(json))
}
