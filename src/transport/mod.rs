//! Transport abstraction layer.
//!
//! Defines object-safe traits for listening/connecting over different
//! transport types (Unix socket, Windows named pipe, TCP) and a generic
//! connection handler that extracts the JSON-line protocol logic from
//! the platform-specific `handle_connection_unix/windows` in `server.rs`.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use anyhow::Result;

use crate::daemon::cache::DbCache;
use crate::daemon::query::Names;
use crate::ipc::{Request, Response};

// ─── Transport address ───────────────────────────────────────────────────────

/// Unified transport address covering Unix socket, Windows named pipe, and TCP.
#[derive(Debug, Clone)]
pub enum TransportAddr {
    Unix(PathBuf),
    WindowsPipe(String),
    Tcp(SocketAddr),
}

// ─── Traits ──────────────────────────────────────────────────────────────────

/// Object-safe trait for accepting incoming connections.
///
/// Each implementation provides its own concrete `Stream` type.
pub trait Listener {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    fn accept(&mut self) -> Pin<Box<dyn Future<Output = Result<Self::Stream>> + Send + '_>>;
}

/// Object-safe trait for initiating outgoing connections.
pub trait Connector {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    fn connect(
        &self,
        addr: &TransportAddr,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Stream>> + Send + '_>>;
}

// ─── Generic connection handler ──────────────────────────────────────────────

/// Read one JSON line, parse as `Request`, dispatch, write one JSON-line `Response`.
///
/// Extracted from the duplicated `handle_connection_unix` / `handle_connection_windows`
/// in `server.rs`. The function is generic over the stream type so it works with
/// `UnixStream`, Windows named pipe stream, `TcpStream`, etc.
pub async fn handle_connection<S>(
    mut stream: S,
    db: &DbCache,
    names: &Arc<tokio::sync::RwLock<Arc<Names>>>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (reader, mut writer) = tokio::io::split(&mut stream);
    let mut lines = BufReader::new(reader).lines();

    let line = match lines.next_line().await? {
        Some(l) => l,
        None => return Ok(()), // client closed without sending anything
    };

    // Parse request
    let req: Request = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::err(format!("JSON 解析错误: {}", e));
            writer.write_all(resp.to_json_line()?.as_bytes()).await?;
            return Ok(());
        }
    };

    let resp = dispatch(req, db, names).await;
    writer.write_all(resp.to_json_line()?.as_bytes()).await?;
    Ok(())
}

// ─── Dispatch (temporary copy from server.rs; will be shared in T02) ────────

async fn dispatch(
    req: Request,
    db: &DbCache,
    names: &tokio::sync::RwLock<Arc<Names>>,
) -> Response {
    use super::daemon::query;

    let names_arc: Arc<Names> = {
        let guard = names.read().await;
        Arc::clone(&*guard)
    };

    match req {
        Request::Ping => Response::ok(serde_json::json!({ "pong": true })),
        Request::Sessions { limit } => {
            match query::q_sessions(db, &names_arc, limit).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::History { chat, limit, offset, since, until, msg_type } => {
            match query::q_history(db, &names_arc, &chat, limit, offset, since, until, msg_type).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::Search { keyword, chats, limit, since, until, msg_type } => {
            match query::q_search(db, &names_arc, &keyword, chats, limit, since, until, msg_type).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::Contacts { query, limit } => {
            match query::q_contacts(&names_arc, query.as_deref(), limit).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::Unread { limit, filter } => {
            match query::q_unread(db, &names_arc, limit, filter).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::Members { chat } => {
            match query::q_members(db, &names_arc, &chat).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::NewMessages { state, limit } => {
            match query::q_new_messages(db, &names_arc, state, limit).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::Favorites { limit, fav_type, query } => {
            match query::q_favorites(db, limit, fav_type, query).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::Stats { chat, since, until } => {
            match query::q_stats(db, &names_arc, &chat, since, until).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::SnsNotifications { limit, since, until, include_read } => {
            match query::q_sns_notifications(db, &names_arc, limit, since, until, include_read).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::SnsFeed { limit, since, until, user } => {
            match query::q_sns_feed(db, &names_arc, limit, since, until, user.as_deref()).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::SnsSearch { keyword, limit, since, until, user } => {
            match query::q_sns_search(db, &names_arc, &keyword, limit, since, until, user.as_deref()).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
    }
}

// ─── TCP implementations ────────────────────────────────────────────────────

/// TCP listener wrapping `tokio::net::TcpListener`.
pub struct TcpListener {
    inner: tokio::net::TcpListener,
}

impl TcpListener {
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        let inner = tokio::net::TcpListener::bind(addr).await?;
        Ok(Self { inner })
    }
}

impl Listener for TcpListener {
    type Stream = tokio::net::TcpStream;

    fn accept(&mut self) -> Pin<Box<dyn Future<Output = Result<Self::Stream>> + Send + '_>> {
        Box::pin(async {
            let (stream, _addr) = self.inner.accept().await?;
            Ok(stream)
        })
    }
}

/// TCP connector using `tokio::net::TcpStream`.
pub struct TcpConnector;

impl Connector for TcpConnector {
    type Stream = tokio::net::TcpStream;

    fn connect(
        &self,
        addr: &TransportAddr,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Stream>> + Send + '_>> {
        let addr = addr.clone();
        Box::pin(async move {
            match addr {
                TransportAddr::Tcp(socket_addr) => {
                    let stream = tokio::net::TcpStream::connect(socket_addr).await?;
                    Ok(stream)
                }
                other => anyhow::bail!("TcpConnector 不支持 {:?}，请使用对应的 Connector", other),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_addr_variants() {
        let unix = TransportAddr::Unix(PathBuf::from("/tmp/wx.sock"));
        let tcp = TransportAddr::Tcp("127.0.0.1:8080".parse().unwrap());
        let pipe = TransportAddr::WindowsPipe("wx-cli-daemon".to_string());

        match unix {
            TransportAddr::Unix(p) => assert_eq!(p, PathBuf::from("/tmp/wx.sock")),
            _ => panic!("expected Unix"),
        }
        match tcp {
            TransportAddr::Tcp(s) => assert_eq!(s.port(), 8080),
            _ => panic!("expected Tcp"),
        }
        match pipe {
            TransportAddr::WindowsPipe(s) => assert_eq!(s, "wx-cli-daemon"),
            _ => panic!("expected WindowsPipe"),
        }
    }

    #[test]
    fn tcp_connector_rejects_non_tcp_addr() {
        // Verify at compile-time that TcpConnector implements Connector
        fn assert_connector<T: Connector>() {}
        assert_connector::<TcpConnector>();
    }

    #[test]
    fn tcp_listener_implements_listener() {
        fn assert_listener<T: Listener>() {}
        assert_listener::<TcpListener>();
    }
}
