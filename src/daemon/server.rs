use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info};

use crate::ipc::{Request, Response};
use crate::transport::{self, Listener};
use super::cache::DbCache;
use super::query::Names;

/// 启动 IPC server（Unix socket / Windows named pipe + 可选 TCP）
///
/// 当 `tcp_addr` 为 `Some` 时，同时监听 TCP 端口；daemon 在 local listener 退出时退出。
pub async fn serve(
    db: Arc<DbCache>,
    names: Arc<tokio::sync::RwLock<Arc<Names>>>,
    tcp_addr: Option<&str>,
) -> Result<()> {
    // TCP 先启动为后台任务
    if let Some(addr) = tcp_addr {
        let socket_addr: std::net::SocketAddr = addr.parse().map_err(|e| {
            anyhow::anyhow!("TCP 地址解析失败 '{}': {}", addr, e)
        })?;
        let db_tcp = Arc::clone(&db);
        let names_tcp = Arc::clone(&names);
        tokio::spawn(async move {
            if let Err(e) = serve_tcp(socket_addr, db_tcp, names_tcp).await {
                error!(error = %e, "TCP 监听错误");
            }
        });
    }

    #[cfg(unix)]
    serve_unix(db, names).await?;
    #[cfg(windows)]
    serve_windows(db, names).await?;
    Ok(())
}

async fn serve_tcp(
    addr: std::net::SocketAddr,
    db: Arc<DbCache>,
    names: Arc<tokio::sync::RwLock<Arc<Names>>>,
) -> Result<()> {
    let listener = transport::TcpListener::bind(addr).await?;
    info!("监听 TCP {}", addr);

    // TcpListener::accept 返回 Pin<Box<dyn Future>>，需要 Box::pin 包装循环
    let mut listener = listener;
    loop {
        let stream = listener.accept().await?;
        let db2 = Arc::clone(&db);
        let names2 = Arc::clone(&names);
        tokio::spawn(async move {
            if let Err(e) = transport::handle_connection(stream, &db2, &names2).await {
                error!(error = %e, "TCP 连接处理错误");
            }
        });
    }
}

#[cfg(unix)]
async fn serve_unix(
    db: Arc<DbCache>,
    names: Arc<tokio::sync::RwLock<Arc<Names>>>,
) -> Result<()> {
    use tokio::net::UnixListener;
    let sock_path = crate::config::sock_path();

    // 删除旧 socket 文件
    if sock_path.exists() {
        let _ = tokio::fs::remove_file(&sock_path).await;
    }

    let listener = UnixListener::bind(&sock_path)?;
    // 设置权限 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o600))?;
    }

    info!("监听 Unix socket {}", sock_path.display());

    loop {
        let (stream, _) = listener.accept().await?;
        let db2 = Arc::clone(&db);
        let names2 = Arc::clone(&names);

        tokio::spawn(async move {
            if let Err(e) = transport::handle_connection(stream, &db2, &names2).await {
                error!(error = %e, "连接处理错误");
            }
        });
    }
}

#[cfg(windows)]
async fn serve_windows(
    db: Arc<DbCache>,
    names: Arc<tokio::sync::RwLock<Arc<Names>>>,
) -> Result<()> {
    use interprocess::local_socket::{
        tokio::prelude::*, GenericNamespaced, ListenerOptions,
    };

    // interprocess 的 GenericNamespaced 在 Windows 上会自动拼接 `\\.\pipe\` 前缀，
    // 这里必须传相对名；client 端用 `\\.\pipe\wx-cli-daemon` 直接打开可以对上
    let name = "wx-cli-daemon".to_ns_name::<GenericNamespaced>()?;
    let opts = ListenerOptions::new().name(name);
    let listener = opts.create_tokio()?;

    info!("监听 Windows named pipe \\\\.\\pipe\\wx-cli-daemon");

    loop {
        let conn = listener.accept().await?;
        let db2 = Arc::clone(&db);
        let names2 = Arc::clone(&names);

        tokio::spawn(async move {
            if let Err(e) = transport::handle_connection(conn, &db2, &names2).await {
                error!(error = %e, "连接处理错误");
            }
        });
    }
}

#[cfg(windows)]
#[allow(dead_code)]
async fn handle_connection_windows(
    conn: interprocess::local_socket::tokio::Stream,
    db: Arc<DbCache>,
    names: Arc<tokio::sync::RwLock<Arc<Names>>>,
) -> Result<()> {
    let (reader, mut writer) = tokio::io::split(conn);
    let mut lines = BufReader::new(reader).lines();

    let line = match lines.next_line().await? {
        Some(l) => l,
        None => return Ok(()),
    };

    let req: Request = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::err(format!("JSON 解析错误: {}", e));
            writer.write_all(resp.to_json_line()?.as_bytes()).await?;
            return Ok(());
        }
    };

    let resp = dispatch(req, &db, &names).await;
    writer.write_all(resp.to_json_line()?.as_bytes()).await?;
    Ok(())
}

#[allow(dead_code)]
async fn dispatch(
    req: Request,
    db: &DbCache,
    names: &tokio::sync::RwLock<Arc<Names>>,
) -> Response {
    use crate::ipc::Request::*;
    use super::query;

    // 取 guard → O(1) clone Arc → 立即 drop 锁。后续 await 期间不持有锁，
    // 多个并发 IPC 请求可以真正并行。Names 本身不可变（由 daemon 启动时
    // 一次性构建），共享 Arc 即可。
    let names_arc: Arc<Names> = {
        let guard = names.read().await;
        Arc::clone(&*guard)
    };

    match req {
        Ping => Response::ok(serde_json::json!({ "pong": true })),
        Sessions { limit } => {
            match query::q_sessions(db, &names_arc, limit).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        History { chat, limit, offset, since, until, msg_type } => {
            match query::q_history(db, &names_arc, &chat, limit, offset, since, until, msg_type).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Search { keyword, chats, limit, since, until, msg_type } => {
            match query::q_search(db, &names_arc, &keyword, chats, limit, since, until, msg_type).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Contacts { query, limit } => {
            match query::q_contacts(&names_arc, query.as_deref(), limit).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Unread { limit, filter } => {
            match query::q_unread(db, &names_arc, limit, filter).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Members { chat } => {
            match query::q_members(db, &names_arc, &chat).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        NewMessages { state, limit } => {
            match query::q_new_messages(db, &names_arc, state, limit).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Favorites { limit, fav_type, query } => {
            match query::q_favorites(db, limit, fav_type, query).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Stats { chat, since, until } => {
            match query::q_stats(db, &names_arc, &chat, since, until).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        SnsNotifications { limit, since, until, include_read } => {
            match query::q_sns_notifications(db, &names_arc, limit, since, until, include_read).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        SnsFeed { limit, since, until, user } => {
            match query::q_sns_feed(db, &names_arc, limit, since, until, user.as_deref()).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        SnsSearch { keyword, limit, since, until, user } => {
            match query::q_sns_search(db, &names_arc, &keyword, limit, since, until, user.as_deref()).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        ReloadConfig => {
            Response::ok(serde_json::json!({ "reloading": true }))
        }
        BizArticles { limit, account, since, until, unread } => {
            match query::q_biz_articles(db, &names_arc, limit, account, since, until, unread).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Attachments { chat, kinds, limit, offset, since, until } => {
            match query::q_attachments(db, &names_arc, &chat, kinds, limit, offset, since, until).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Extract { attachment_id, output, overwrite } => {
            match query::q_extract(db, &names_arc, &attachment_id, &output, overwrite).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
    }
}
