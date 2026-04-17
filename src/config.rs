use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub db_dir: PathBuf,
    pub keys_file: PathBuf,
    pub decrypted_dir: PathBuf,
    #[serde(default)]
    pub wechat_process: String,
}

/// 从 <exe_dir>/config.json 或 $HOME/.wx-cli/config.json 加载配置
pub fn load_config() -> Result<Config> {
    let config_path = find_config_file()?;
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("读取 config.json 失败: {}", config_path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| "config.json 格式错误")?;

    let db_dir = raw.get("db_dir")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(default_db_dir);

    let base_dir = config_path.parent().unwrap_or(Path::new("."));

    let keys_file = raw.get("keys_file")
        .and_then(|v| v.as_str())
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() { p } else { base_dir.join(p) }
        })
        .unwrap_or_else(|| base_dir.join("all_keys.json"));

    let decrypted_dir = raw.get("decrypted_dir")
        .and_then(|v| v.as_str())
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() { p } else { base_dir.join(p) }
        })
        .unwrap_or_else(|| base_dir.join("decrypted"));

    let wechat_process = raw.get("wechat_process")
        .and_then(|v| v.as_str())
        .unwrap_or(default_wechat_process())
        .to_string();

    Ok(Config {
        db_dir,
        keys_file,
        decrypted_dir,
        wechat_process,
    })
}

fn find_config_file() -> Result<PathBuf> {
    // 1. 优先查找可执行文件同目录
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("config.json");
            if p.exists() {
                return Ok(p);
            }
        }
    }
    // 2. 当前工作目录
    let cwd = std::env::current_dir().unwrap_or_default().join("config.json");
    if cwd.exists() {
        return Ok(cwd);
    }
    // 3. ~/.wx-cli/config.json
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".wx-cli").join("config.json");
        if p.exists() {
            return Ok(p);
        }
    }
    // 返回默认路径（可能不存在，调用方负责处理）
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return Ok(dir.join("config.json"));
        }
    }
    Ok(PathBuf::from("config.json"))
}

/// 真实用户的 home 目录。
///
/// 当通过 sudo 调用时，`dirs::home_dir()` 会返回 `/var/root`（或 `/root`），
/// 导致 `~/.wx-cli/` 落在 root 名下，daemon 后续以普通用户身份启动时无法读写。
/// 这里优先使用 sudo 注入的 `SUDO_USER` 反查真实用户的 home。
pub fn effective_home() -> Option<PathBuf> {
    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        if !sudo_user.is_empty() && sudo_user != "root" {
            #[cfg(target_os = "macos")]
            let candidate = PathBuf::from("/Users").join(&sudo_user);
            #[cfg(target_os = "linux")]
            let candidate = PathBuf::from("/home").join(&sudo_user);
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            let candidate = dirs::home_dir().unwrap_or_default();
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    dirs::home_dir()
}

pub fn cli_dir() -> PathBuf {
    effective_home()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".wx-cli")
}

pub fn sock_path() -> PathBuf {
    cli_dir().join("daemon.sock")
}

pub fn pid_path() -> PathBuf {
    cli_dir().join("daemon.pid")
}

pub fn log_path() -> PathBuf {
    cli_dir().join("daemon.log")
}

pub fn cache_dir() -> PathBuf {
    cli_dir().join("cache")
}

pub fn mtime_file() -> PathBuf {
    cache_dir().join("_mtimes.json")
}

fn default_db_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .unwrap_or_default()
            .join("Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files")
    }
    #[cfg(target_os = "linux")]
    {
        dirs::home_dir()
            .unwrap_or_default()
            .join("Documents/xwechat_files")
    }
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
            .join("Tencent/xwechat")
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        PathBuf::from(".")
    }
}

fn default_wechat_process() -> &'static str {
    #[cfg(target_os = "macos")]
    { "WeChat" }
    #[cfg(target_os = "linux")]
    { "wechat" }
    #[cfg(target_os = "windows")]
    { "Weixin.exe" }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    { "WeChat" }
}

/// 自动检测微信 db_storage 目录
pub fn auto_detect_db_dir() -> Option<PathBuf> {
    detect_db_dir_impl()
}

#[cfg(target_os = "macos")]
fn detect_db_dir_impl() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    // 支持 sudo 环境
    let home = if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        if !sudo_user.is_empty() {
            PathBuf::from("/Users").join(&sudo_user)
        } else {
            home
        }
    } else {
        home
    };

    let base = home.join("Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files");
    if !base.exists() {
        return None;
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let storage = entry.path().join("db_storage");
            if storage.is_dir() {
                candidates.push(storage);
            }
        }
    }
    candidates.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    candidates.into_iter().next_back()
}

#[cfg(target_os = "linux")]
fn detect_db_dir_impl() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let sudo_home = std::env::var("SUDO_USER").ok()
        .filter(|s| !s.is_empty())
        .map(|u| PathBuf::from("/home").join(u));

    let mut candidates: Vec<PathBuf> = Vec::new();
    for base_home in [Some(home.clone()), sudo_home].into_iter().flatten() {
        let xwechat = base_home.join("Documents/xwechat_files");
        if xwechat.exists() {
            if let Ok(entries) = std::fs::read_dir(&xwechat) {
                for entry in entries.flatten() {
                    let storage = entry.path().join("db_storage");
                    if storage.is_dir() {
                        candidates.push(storage);
                    }
                }
            }
        }
        let old = base_home.join(".local/share/weixin/data/db_storage");
        if old.is_dir() {
            candidates.push(old);
        }
    }
    candidates.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    candidates.into_iter().next_back()
}

#[cfg(target_os = "windows")]
fn detect_db_dir_impl() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    let config_dir = PathBuf::from(&appdata).join("Tencent/xwechat/config");
    if !config_dir.exists() {
        return None;
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&config_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "ini").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let data_root = content.trim().to_string();
                    if PathBuf::from(&data_root).is_dir() {
                        let pattern = PathBuf::from(&data_root)
                            .join("xwechat_files");
                        if let Ok(entries2) = std::fs::read_dir(&pattern) {
                            for entry2 in entries2.flatten() {
                                let storage = entry2.path().join("db_storage");
                                if storage.is_dir() {
                                    candidates.push(storage);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    candidates.into_iter().next()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn detect_db_dir_impl() -> Option<PathBuf> {
    None
}
