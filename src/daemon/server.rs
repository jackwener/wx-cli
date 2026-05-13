use anyhow::Result;
use std::sync::Arc;

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
                eprintln!("[server] TCP 监听错误: {}", e);
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
    eprintln!("[server] 监听 TCP {}", addr);

    // TcpListener::accept 返回 Pin<Box<dyn Future>>，需要 Box::pin 包装循环
    let mut listener = listener;
    loop {
        let stream = listener.accept().await?;
        let db2 = Arc::clone(&db);
        let names2 = Arc::clone(&names);
        tokio::spawn(async move {
            if let Err(e) = transport::handle_connection(stream, &db2, &names2).await {
                eprintln!("[server] 连接处理错误: {}", e);
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

    eprintln!("[server] 监听 {}", sock_path.display());

    loop {
        let (stream, _) = listener.accept().await?;
        let db2 = Arc::clone(&db);
        let names2 = Arc::clone(&names);

        tokio::spawn(async move {
            if let Err(e) = transport::handle_connection(stream, &db2, &names2).await {
                eprintln!("[server] 连接处理错误: {}", e);
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

    eprintln!("[server] 监听 \\\\.\\pipe\\wx-cli-daemon");

    loop {
        let conn = listener.accept().await?;
        let db2 = Arc::clone(&db);
        let names2 = Arc::clone(&names);

        tokio::spawn(async move {
            if let Err(e) = transport::handle_connection(conn, &db2, &names2).await {
                eprintln!("[server] 连接处理错误: {}", e);
            }
        });
    }
}
