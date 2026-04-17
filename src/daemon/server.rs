use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::ipc::{Request, Response};
use super::cache::DbCache;
use super::query::Names;

/// 启动 IPC server（Unix socket / Windows named pipe）
pub async fn serve(
    db: Arc<DbCache>,
    names: Arc<tokio::sync::RwLock<Arc<Names>>>,
) -> Result<()> {
    #[cfg(unix)]
    serve_unix(db, names).await?;
    #[cfg(windows)]
    serve_windows(db, names).await?;
    Ok(())
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

    eprintln!("[server] 监听 {}", sock_path.display());

    loop {
        let (stream, _) = listener.accept().await?;
        let db2 = Arc::clone(&db);
        let names2 = Arc::clone(&names);

        tokio::spawn(async move {
            if let Err(e) = handle_connection_unix(stream, db2, names2).await {
                eprintln!("[server] 连接处理错误: {}", e);
            }
        });
    }
}

#[cfg(unix)]
async fn handle_connection_unix(
    stream: tokio::net::UnixStream,
    db: Arc<DbCache>,
    names: Arc<tokio::sync::RwLock<Arc<Names>>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let line = match lines.next_line().await? {
        Some(l) => l,
        None => return Ok(()),
    };

    // 解析请求
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

    eprintln!("[server] 监听 \\\\.\\pipe\\wx-cli-daemon");

    loop {
        let conn = listener.accept().await?;
        let db2 = Arc::clone(&db);
        let names2 = Arc::clone(&names);

        tokio::spawn(async move {
            if let Err(e) = handle_connection_windows(conn, db2, names2).await {
                eprintln!("[server] 连接处理错误: {}", e);
            }
        });
    }
}

#[cfg(windows)]
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
    }
}
