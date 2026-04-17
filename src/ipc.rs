use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// CLI 向 daemon 发送的请求（换行符分隔 JSON，与 Python 版兼容）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Sessions {
        #[serde(default = "default_limit_20")]
        limit: usize,
    },
    History {
        chat: String,
        #[serde(default = "default_limit_50")]
        limit: usize,
        #[serde(default)]
        offset: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        since: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        until: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        msg_type: Option<i64>,
    },
    Search {
        keyword: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        chats: Option<Vec<String>>,
        #[serde(default = "default_limit_20")]
        limit: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        since: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        until: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        msg_type: Option<i64>,
    },
    Contacts {
        #[serde(skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default = "default_limit_50")]
        limit: usize,
    },
    Unread {
        #[serde(default = "default_limit_20")]
        limit: usize,
        /// 按会话类型过滤：private / group / official / folded / all，支持多选
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<Vec<String>>,
    },
    Members {
        chat: String,
    },
    NewMessages {
        /// 上次检查时各会话的 last_timestamp 快照（username -> ts）
        /// None 表示首次运行，会返回 new_state 供下次使用
        #[serde(skip_serializing_if = "Option::is_none")]
        state: Option<HashMap<String, i64>>,
        #[serde(default = "default_limit_200")]
        limit: usize,
    },
    Stats {
        chat: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        since: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        until: Option<i64>,
    },
    Favorites {
        #[serde(default = "default_limit_50")]
        limit: usize,
        /// 类型过滤：1=文本,2=图片,5=文章,19=名片,20=视频
        #[serde(skip_serializing_if = "Option::is_none")]
        fav_type: Option<i64>,
        /// 内容关键词搜索
        #[serde(skip_serializing_if = "Option::is_none")]
        query: Option<String>,
    },
}


/// daemon 的响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(flatten)]
    pub data: Value,
}

impl Response {
    pub fn ok(data: Value) -> Self {
        Self { ok: true, error: None, data }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self { ok: false, error: Some(msg.into()), data: Value::Null }
    }

    pub fn to_json_line(&self) -> anyhow::Result<String> {
        let s = serde_json::to_string(self)?;
        Ok(s + "\n")
    }
}

fn default_limit_20() -> usize { 20 }
fn default_limit_50() -> usize { 50 }
fn default_limit_200() -> usize { 200 }
