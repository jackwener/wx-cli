use anyhow::Result;
use crate::ipc::Request;
use super::transport;
use super::history::{parse_time, parse_time_end};

pub fn cmd_export(
    chat: String,
    since: Option<String>,
    until: Option<String>,
    limit: usize,
    format: String,
    output: Option<String>,
    tcp_addr: Option<&str>,
) -> Result<()> {
    let since_ts = since.as_deref().map(parse_time).transpose()?;
    let until_ts = until.as_deref().map(parse_time_end).transpose()?;

    let req = Request::History {
        chat,
        limit,
        offset: 0,
        since: since_ts,
        until: until_ts,
        msg_type: None,
    };

    let resp = transport::send(req, tcp_addr)?;
    let messages = resp.data["messages"].as_array().cloned().unwrap_or_default();
    let chat_name = resp.data["chat"].as_str().unwrap_or("").to_string();
    let is_group = resp.data["is_group"].as_bool().unwrap_or(false);
    let count = messages.len();

    let text = match format.as_str() {
        "json" => serde_json::to_string_pretty(&resp.data)?,
        "txt" => {
            let group_str = if is_group { "[群]" } else { "" };
            let mut lines = vec![format!("=== {}{} ({} 条) ===\n", chat_name, group_str, count)];
            for m in &messages {
                let time = m["time"].as_str().unwrap_or("");
                let sender = m["sender"].as_str().unwrap_or("");
                let content = m["content"].as_str().unwrap_or("");
                let sender_str = if !sender.is_empty() { format!("{}: ", sender) } else { String::new() };
                lines.push(format!("[{}] {}{}", time, sender_str, content));
            }
            lines.join("\n")
        }
        _ => {
            // markdown (default)
            let group_str = if is_group { "（群聊）" } else { "" };
            let mut lines = vec![
                format!("# {}{}", chat_name, group_str),
                format!("\n> 导出 {} 条消息\n", count),
            ];
            for m in &messages {
                let time = m["time"].as_str().unwrap_or("");
                let sender = m["sender"].as_str().unwrap_or("");
                let content = m["content"].as_str().unwrap_or("").replace('\n', "\n> ");
                let sender_md = if !sender.is_empty() { format!("**{}**: ", sender) } else { String::new() };
                lines.push(format!("### {}\n\n{}{}\n", time, sender_md, content));
            }
            lines.join("\n")
        }
    };

    match output {
        Some(path) => {
            std::fs::write(&path, &text)?;
            println!("已导出 {} 条消息到 {}", count, path);
        }
        None => println!("{}", text),
    }

    Ok(())
}
