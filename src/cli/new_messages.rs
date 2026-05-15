use anyhow::Result;
use std::collections::HashMap;
use crate::ipc::Request;
use super::transport;
use super::output::{resolve, print_value};

fn state_file() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".wx-cli")
        .join("last_check.json")
}

/// 加载上次的 per-session 时间戳快照
/// 格式：{ "sessions": { "username": timestamp, ... } }
/// 旧格式（只有 timestamp 字段）直接丢弃，重新全量获取
fn load_state() -> Option<HashMap<String, i64>> {
    let data = std::fs::read_to_string(state_file()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&data).ok()?;
    // 旧格式（只有 timestamp 字段）没有 sessions key → 返回 None 触发首次运行逻辑
    let map: HashMap<String, i64> = v.get("sessions")?
        .as_object()?
        .iter()
        .filter_map(|(k, v)| v.as_i64().map(|ts| (k.clone(), ts)))
        .collect();
    // 空 map 也是合法状态（账号无任何会话），返回 Some(empty) 而非 None
    // 这样不会误触发全量历史拉取
    Some(map)
}

fn save_state(new_state: &HashMap<String, i64>) -> Result<()> {
    let path = state_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string(&serde_json::json!({ "sessions": new_state }))?)?;
    Ok(())
}

pub fn cmd_new_messages(limit: usize, json: bool, tcp_addr: Option<&str>) -> Result<()> {
    let state = load_state();
    let resp = transport::send(Request::NewMessages { state, limit }, tcp_addr)?;

    // 保存 daemon 返回的 new_state
    if let Some(obj) = resp.data.get("new_state").and_then(|v| v.as_object()) {
        let map: HashMap<String, i64> = obj.iter()
            .filter_map(|(k, v)| v.as_i64().map(|ts| (k.clone(), ts)))
            .collect();
        if !map.is_empty() {
            let _ = save_state(&map);
        }
    }

    let messages = resp.data.get("messages")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    print_value(&messages, &resolve(json))
}
