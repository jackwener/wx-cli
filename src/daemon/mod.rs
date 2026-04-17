pub mod cache;
pub mod query;
pub mod server;

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config;

/// daemon 入口
///
/// 当 WX_DAEMON_MODE 环境变量设置时，main() 调用此函数
pub fn run() {
    let rt = tokio::runtime::Runtime::new().expect("无法创建 tokio runtime");
    if let Err(e) = rt.block_on(async_run()) {
        eprintln!("[daemon] 启动失败: {}", e);
        std::process::exit(1);
    }
}

async fn async_run() -> Result<()> {
    // 确保工作目录存在
    let cli_dir = config::cli_dir();
    tokio::fs::create_dir_all(&cli_dir).await?;
    tokio::fs::create_dir_all(config::cache_dir()).await?;

    // 写 PID 文件
    let pid = std::process::id();
    tokio::fs::write(config::pid_path(), pid.to_string()).await?;

    // 注册 SIGTERM / SIGINT 处理
    setup_signal_handler().await;

    eprintln!("[daemon] wx-daemon 启动 (PID {})", pid);

    // 加载配置
    let cfg = config::load_config()?;
    eprintln!("[daemon] DB_DIR: {}", cfg.db_dir.display());

    // 加载密钥
    let keys_content = tokio::fs::read_to_string(&cfg.keys_file).await
        .map_err(|e| anyhow::anyhow!("读取密钥文件 {:?} 失败: {}", cfg.keys_file, e))?;
    let keys_raw: serde_json::Value = serde_json::from_str(&keys_content)?;
    let all_keys = extract_keys(&keys_raw);
    eprintln!("[daemon] 密钥数量: {}", all_keys.len());

    // 初始化 DbCache
    let db = Arc::new(cache::DbCache::new(cfg.db_dir.clone(), all_keys.clone()).await?);

    // 收集消息 DB 列表
    let msg_db_keys: Vec<String> = all_keys.keys()
        .filter(|k| {
            let k = k.replace('\\', "/");
            k.contains("message/message_") && k.ends_with(".db")
                && !k.contains("_fts") && !k.contains("_resource")
        })
        .cloned()
        .collect();

    // 预热：加载联系人 + 解密 session.db
    eprintln!("[daemon] 预热...");
    let names_raw = query::load_names(&*db).await.unwrap_or_else(|e| {
        eprintln!("[daemon] 加载联系人失败: {}", e);
        query::Names {
            map: HashMap::new(),
            md5_to_uname: HashMap::new(),
            msg_db_keys: Vec::new(),
            verify_flags: HashMap::new(),
        }
    });
    let mut names = names_raw;
    names.msg_db_keys = msg_db_keys;

    let _ = db.get("session/session.db").await;
    eprintln!("[daemon] 预热完成，联系人 {} 个", names.map.len());

    let names_arc = Arc::new(std::sync::RwLock::new(names));

    // 启动 IPC server（阻塞）
    server::serve(Arc::clone(&db), Arc::clone(&names_arc)).await?;

    Ok(())
}

/// 从 all_keys.json 提取 rel_key -> enc_key 映射
///
/// 兼容两种格式：
/// - `{ "rel/path.db": { "enc_key": "hex" } }`（Python 版原生格式）
/// - `{ "rel/path.db": "hex" }`（简化格式）
fn extract_keys(json: &serde_json::Value) -> HashMap<String, String> {
    let mut result = HashMap::new();
    if let Some(obj) = json.as_object() {
        for (k, v) in obj {
            if k.starts_with('_') { continue; }
            let enc_key = if let Some(s) = v.as_str() {
                s.to_string()
            } else if let Some(obj2) = v.as_object() {
                obj2.get("enc_key")
                    .and_then(|e| e.as_str())
                    .unwrap_or_default()
                    .to_string()
            } else {
                continue;
            };
            if !enc_key.is_empty() {
                // 统一路径分隔符
                let rel = k.replace('\\', "/");
                result.insert(rel, enc_key);
            }
        }
    }
    result
}

/// 设置信号处理（Unix: SIGTERM/SIGINT）
async fn setup_signal_handler() {
    #[cfg(unix)]
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("无法监听 SIGTERM");
        let mut int = signal(SignalKind::interrupt()).expect("无法监听 SIGINT");
        tokio::select! {
            _ = term.recv() => {},
            _ = int.recv() => {},
        }
        cleanup_and_exit();
    });
}

fn cleanup_and_exit() {
    let _ = std::fs::remove_file(config::sock_path());
    let _ = std::fs::remove_file(config::pid_path());
    std::process::exit(0);
}
