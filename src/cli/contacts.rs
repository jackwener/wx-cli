use anyhow::Result;
use crate::ipc::Request;
use super::transport;
use super::output::{resolve, print_value};

pub fn cmd_contacts(query: Option<String>, limit: usize, json: bool, tcp_addr: Option<&str>) -> Result<()> {
    let resp = transport::send(Request::Contacts { query, limit }, tcp_addr)?;
    let contacts = resp.data.get("contacts")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    print_value(&contacts, &resolve(json))
}
