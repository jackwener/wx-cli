use anyhow::{Context, Result};
use serde_json::json;
use std::collections::HashMap;

use crate::cli::transport::{stop_daemon, StopDaemonOutcome};
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
                let keys_file = cfg
                    .get("keys_file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("all_keys.json");
                let keys_path = if std::path::Path::new(keys_file).is_absolute() {
                    std::path::PathBuf::from(keys_file)
                } else {
                    config_path
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .join(keys_file)
                };
                if !db_dir.is_empty()
                    && !db_dir.contains("your_wxid")
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

    // === 权限边界 ===
    // 扫描完成后立即 drop 到调用用户身份，后续文件写入都是用户属主。
    // 未来 daemon（由 `wx sessions` 以用户身份 fork）才能往 ~/.wx-cli/
    // 写 socket/log/pid。
    #[cfg(unix)]
    drop_privileges_if_sudo()?;

    // --force：先停 daemon 并清空解密缓存，避免旧缓存与新密钥 mtime 一致仍被复用
    if force {
        println!("(--force) 停止 wx-daemon 并清空解密缓存…");
        match stop_daemon()? {
            StopDaemonOutcome::NoPidFile => {}
            StopDaemonOutcome::Stopped(pid) => println!("已停止 wx-daemon (PID {})", pid),
            StopDaemonOutcome::StalePid(pid) => {
                println!("wx-daemon (PID {}) 已不在运行，已清理残留文件", pid);
            }
        }
        clear_decrypt_cache()?;
        println!("已清空 {}", config::cache_dir().display());
    }

    // 确保父目录存在（如 ~/.wx-cli/），必须在任何写入之前
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }

    // Step 3: 保存 all_keys.json
    let keys_file_path = config_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("all_keys.json");

    let mut keys_json = serde_json::Map::new();
    for entry in &entries {
        keys_json.insert(
            entry.db_name.clone(),
            json!({
                "enc_key": entry.enc_key,
            }),
        );
    }
    std::fs::write(&keys_file_path, serde_json::to_string_pretty(&keys_json)?)
        .context("写入 all_keys.json 失败")?;
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
    cfg.entry("keys_file".into())
        .or_insert_with(|| json!("all_keys.json"));
    cfg.entry("decrypted_dir".into())
        .or_insert_with(|| json!("decrypted"));

    std::fs::write(&config_path, serde_json::to_string_pretty(&cfg)?)
        .context("写入 config.json 失败")?;
    println!("配置已保存: {}", config_path.display());
    println!("初始化完成，可以使用 wx sessions / wx history 等命令了");

    Ok(())
}

/// 删除 `~/.wx-cli/cache`（含 `_mtimes.json` 与已解密 DB），并重建空目录。
fn clear_decrypt_cache() -> Result<()> {
    let dir = config::cache_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("删除解密缓存目录失败: {}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("创建解密缓存目录失败: {}", dir.display()))?;
    Ok(())
}

/// 如果当前以 root 身份运行且是通过 sudo 启动的，drop 到调用用户身份，
/// 并迁移旧版本遗留的 root 属主 `~/.wx-cli/`。
///
/// 只影响本进程；daemon（后续 fork）会继承调用用户身份。
#[cfg(unix)]
fn drop_privileges_if_sudo() -> Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    // 当前不是 root（用户直接以非 root 跑的 `wx init`）→ 什么都不做
    if unsafe { libc::geteuid() } != 0 {
        return Ok(());
    }

    let sudo_uid: Option<u32> = std::env::var("SUDO_UID").ok().and_then(|s| s.parse().ok());
    let sudo_gid: Option<u32> = std::env::var("SUDO_GID").ok().and_then(|s| s.parse().ok());
    let (uid, gid) = match (sudo_uid, sudo_gid) {
        (Some(u), Some(g)) if u != 0 => (u, g),
        // 直接以 root 登陆（非 sudo），没有"调用用户"可还原 → 保持 root
        _ => return Ok(()),
    };

    // 迁移旧版本遗留：如果 ~/.wx-cli/ 已存在且属 root，把它 chown 回调用用户，
    // 顺便把 raw key 文件的权限也收紧到 0600（旧版默认 0644，世界可读等于泄露）。
    // 这些必须在 setuid 之前做：chown 需要 root，chmod 也只有属主或 root 能改。
    let cli_dir = config::cli_dir();
    if cli_dir.exists() {
        let _ = chown_recursive(&cli_dir, uid, gid);
        let _ = tighten_perms(&cli_dir);
    }

    // 设置 umask，让后续 create 出来的文件/目录默认是 0600 / 0700。
    unsafe {
        libc::umask(0o077);
    }

    // 必须先 setgid 再 setuid：一旦 uid 降下来就没法再改 gid 了。
    unsafe {
        if libc::setgid(gid) != 0 {
            anyhow::bail!("setgid({}) 失败: {}", gid, std::io::Error::last_os_error());
        }
        if libc::setuid(uid) != 0 {
            anyhow::bail!("setuid({}) 失败: {}", uid, std::io::Error::last_os_error());
        }
    }

    // chown 递归实现
    fn chown_recursive(path: &Path, uid: u32, gid: u32) -> std::io::Result<()> {
        chown_one(path, uid, gid)?;
        let md = std::fs::symlink_metadata(path)?;
        if md.is_dir() {
            for entry in std::fs::read_dir(path)? {
                chown_recursive(&entry?.path(), uid, gid)?;
            }
        }
        Ok(())
    }
    fn chown_one(path: &Path, uid: u32, gid: u32) -> std::io::Result<()> {
        let c = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL")
        })?;
        if unsafe { libc::chown(c.as_ptr(), uid, gid) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// 目录收紧到 0700，所有 *.json 文件（含 all_keys.json 这类 raw key）收紧到 0600。
    fn tighten_perms(cli_dir: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(cli_dir, std::fs::Permissions::from_mode(0o700))?;
        for entry in std::fs::read_dir(cli_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        }
        Ok(())
    }

    Ok(())
}

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
