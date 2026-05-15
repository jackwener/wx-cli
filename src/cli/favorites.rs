use anyhow::Result;
use crate::ipc::Request;
use super::transport;
use super::output::{resolve, print_value};

fn parse_fav_type(s: &str) -> Option<i64> {
    match s {
        "text"    => Some(1),
        "image"   => Some(2),
        "article" => Some(5),
        "card"    => Some(19),
        "video"   => Some(20),
        _         => None,
    }
}

pub fn cmd_favorites(
    limit: usize,
    fav_type: Option<String>,
    query: Option<String>,
    json: bool,
    tcp_addr: Option<&str>,
) -> Result<()> {
    let type_val = fav_type.as_deref().and_then(parse_fav_type);
    let resp = transport::send(Request::Favorites { limit, fav_type: type_val, query }, tcp_addr)?;
    let items = resp.data.get("items")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    print_value(&items, &resolve(json))
}
