use anyhow::{Context, Result};
use serde_json::json;
use std::collections::HashMap;

use crate::config;
use crate::scanner;

pub fn cmd_init(force: bool) -> Result<()> {
    // 查找 config.json
    let config_path = find_or_create_config_path();

    // 检查是否已初始化
    if !force && config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&content) {
                let db_dir = cfg.get("db_dir").and_then(|v| v.as_str()).unwrap_or("");
                let keys_file = cfg.get("keys_file").and_then(|v| v.as_str()).unwrap_or("all_keys.json");
                let keys_path = if std::path::Path::new(keys_file).is_absolute() {
                    std::path::PathBuf::from(keys_file)
                } else {
                    config_path.parent().unwrap_or(std::path::Path::new("."))
                        .join(keys_file)
                };
                if !db_dir.is_empty() && !db_dir.contains("your_wxid")
                    && std::path::Path::new(db_dir).exists()
                    && keys_path.exists()
                {
                    println!("已初始化，数据目录: {}", db_dir);
                    println!("如需重新扫描密钥，使用 --force");
                    return Ok(());
                }
            }
        }
    }

    // Step 1: 检测 db_dir
    println!("检测微信数据目录...");
    let db_dir = config::auto_detect_db_dir()
        .context("未能自动检测到微信数据目录\n请手动编辑 config.json 中的 db_dir 字段")?;
    println!("找到数据目录: {}", db_dir.display());

    // Step 2: 扫描密钥（需要 root/sudo）
    println!("扫描加密密钥（需要 root 权限）...");
    let entries = scanner::scan_keys(&db_dir)?;

    // Step 3: 保存 all_keys.json
    // 先确保父目录存在（如 ~/.wx-cli/），避免首次运行时写入失败。
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
        chown_to_sudo_user(parent);
    }

    let keys_file_path = config_path.parent()
        .unwrap_or(std::path::Path::new("."))
        .join("all_keys.json");

    let mut keys_json = serde_json::Map::new();
    for entry in &entries {
        keys_json.insert(entry.db_name.clone(), json!({
            "enc_key": entry.enc_key,
        }));
    }
    std::fs::write(&keys_file_path, serde_json::to_string_pretty(&keys_json)?)
        .with_context(|| format!("写入 all_keys.json 失败: {}", keys_file_path.display()))?;
    chown_to_sudo_user(&keys_file_path);
    println!("成功提取 {} 个数据库密钥", entries.len());
    println!("密钥已保存: {}", keys_file_path.display());

    // Step 4: 保存 config.json
    let mut cfg = HashMap::new();
    // 读取已有配置
    if config_path.exists() {
        if let Ok(c) = std::fs::read_to_string(&config_path) {
            if let Ok(v) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&c) {
                for (k, val) in v {
                    cfg.insert(k, val);
                }
            }
        }
    }
    cfg.insert("db_dir".into(), json!(db_dir.to_string_lossy()));
    cfg.entry("keys_file".into()).or_insert_with(|| json!("all_keys.json"));
    cfg.entry("decrypted_dir".into()).or_insert_with(|| json!("decrypted"));

    std::fs::write(&config_path, serde_json::to_string_pretty(&cfg)?)
        .with_context(|| format!("写入 config.json 失败: {}", config_path.display()))?;
    chown_to_sudo_user(&config_path);
    println!("配置已保存: {}", config_path.display());
    println!("初始化完成，可以使用 wx sessions / wx history 等命令了");

    Ok(())
}

/// 当通过 sudo 调用时，把刚写入的文件/目录归还给真实用户，否则后续以
/// 普通用户身份运行的 daemon / CLI 无法读写它们。
#[cfg(unix)]
fn chown_to_sudo_user(path: &std::path::Path) {
    let (Some(uid), Some(gid)) = (
        std::env::var("SUDO_UID").ok().and_then(|s| s.parse::<u32>().ok()),
        std::env::var("SUDO_GID").ok().and_then(|s| s.parse::<u32>().ok()),
    ) else {
        return;
    };
    // 静默忽略 chown 失败：路径可能在 root 拥有的卷上，或本身已属于该用户。
    let _ = std::os::unix::fs::chown(path, Some(uid), Some(gid));
}

#[cfg(not(unix))]
fn chown_to_sudo_user(_path: &std::path::Path) {}

fn find_or_create_config_path() -> std::path::PathBuf {
    // 如果当前工作目录或可执行文件目录已有 config.json，沿用它（支持便携模式）
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join("config.json");
        if p.exists() {
            return p;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("config.json");
            if p.exists() {
                return p;
            }
        }
    }
    // 默认写入 ~/.wx-cli/config.json（与 load_config 的最终查找路径保持一致）
    config::cli_dir().join("config.json")
}
