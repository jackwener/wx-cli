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

/// 从当前工作目录 / <exe_dir> / $HOME/.wx-cli 加载配置
pub fn load_config() -> Result<Config> {
    let config_path = find_config_file()?;
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("读取 config.json 失败: {}", config_path.display()))?;
    let raw: serde_json::Value =
        serde_json::from_str(&content).with_context(|| "config.json 格式错误")?;

    let db_dir = raw
        .get("db_dir")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(default_db_dir);

    let base_dir = config_path.parent().unwrap_or(Path::new("."));

    let keys_file = raw
        .get("keys_file")
        .and_then(|v| v.as_str())
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() {
                p
            } else {
                base_dir.join(p)
            }
        })
        .unwrap_or_else(|| base_dir.join("all_keys.json"));

    let decrypted_dir = raw
        .get("decrypted_dir")
        .and_then(|v| v.as_str())
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() {
                p
            } else {
                base_dir.join(p)
            }
        })
        .unwrap_or_else(|| base_dir.join("decrypted"));

    let wechat_process = raw
        .get("wechat_process")
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
    let cwd_dir = std::env::current_dir().ok();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from));
    let cli_home = cli_home_dir();
    let home_dir = (cli_home != PathBuf::from("/tmp")).then_some(cli_home.as_path());

    if let Some(path) = find_existing_config_path(cwd_dir.as_deref(), exe_dir.as_deref(), home_dir)
    {
        return Ok(path);
    }

    Ok(default_config_path(
        cwd_dir.as_deref(),
        exe_dir.as_deref(),
        home_dir,
    ))
}

fn find_existing_config_path(
    cwd_dir: Option<&Path>,
    exe_dir: Option<&Path>,
    home_dir: Option<&Path>,
) -> Option<PathBuf> {
    let candidates = [
        cwd_dir.map(config_path_in_dir),
        exe_dir.map(config_path_in_dir),
        home_dir.map(home_config_path),
    ];
    candidates.into_iter().flatten().find(|path| path.exists())
}

fn default_config_path(
    cwd_dir: Option<&Path>,
    exe_dir: Option<&Path>,
    home_dir: Option<&Path>,
) -> PathBuf {
    cwd_dir
        .map(config_path_in_dir)
        .or_else(|| exe_dir.map(config_path_in_dir))
        .or_else(|| home_dir.map(home_config_path))
        .unwrap_or_else(|| PathBuf::from("config.json"))
}

fn config_path_in_dir(dir: &Path) -> PathBuf {
    dir.join("config.json")
}

fn home_config_path(home_dir: &Path) -> PathBuf {
    home_dir.join(".wx-cli").join("config.json")
}

pub fn cli_dir() -> PathBuf {
    cli_home_dir().join(".wx-cli")
}

fn cli_home_dir() -> PathBuf {
    resolve_cli_home(
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")),
        sudo_user_home_dir(),
    )
}

fn resolve_cli_home(default_home: PathBuf, sudo_home: Option<PathBuf>) -> PathBuf {
    sudo_home.unwrap_or(default_home)
}

#[cfg(unix)]
fn sudo_user_home_dir() -> Option<PathBuf> {
    use std::ffi::{CStr, CString};

    let sudo_user = std::env::var("SUDO_USER").ok()?;
    let sudo_user = sudo_user.trim();
    if sudo_user.is_empty() {
        return None;
    }

    let c_user = CString::new(sudo_user).ok()?;
    unsafe {
        let pwd = libc::getpwnam(c_user.as_ptr());
        if pwd.is_null() || (*pwd).pw_dir.is_null() {
            return None;
        }
        let dir = CStr::from_ptr((*pwd).pw_dir).to_str().ok()?;
        Some(PathBuf::from(dir))
    }
}

#[cfg(not(unix))]
fn sudo_user_home_dir() -> Option<PathBuf> {
    None
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
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("Tencent/xwechat")
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        PathBuf::from(".")
    }
}

fn default_wechat_process() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "WeChat"
    }
    #[cfg(target_os = "linux")]
    {
        "wechat"
    }
    #[cfg(target_os = "windows")]
    {
        "Weixin.exe"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        "WeChat"
    }
}

/// 自动检测微信 db_storage 目录
pub fn auto_detect_db_dir() -> Option<PathBuf> {
    detect_db_dir_impl()
}

#[cfg(target_os = "macos")]
fn detect_db_dir_impl() -> Option<PathBuf> {
    let home = sudo_user_home_dir().or_else(dirs::home_dir)?;

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
    let sudo_home = sudo_user_home_dir();

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
        // 排序：取 db_storage 目录下所有 .db 文件的最新 mtime，而非目录自身的 mtime
        // 这样当收到新消息时（只有 .db 文件被更新），能正确识别最新目录
        latest_db_mtime(p).unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    candidates.into_iter().next_back()
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
/// 递归查找 db_storage 目录下所有 .db 文件的最新 mtime
fn latest_db_mtime(dir: &Path) -> Option<std::time::SystemTime> {
    let mut latest = None;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let mtime = if path.is_dir() {
                latest_db_mtime(&path).unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            } else if path.extension().and_then(|s| s.to_str()) == Some("db") {
                entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            } else {
                continue;
            };
            latest = Some(latest.map_or(mtime, |cur| if mtime > cur { mtime } else { cur }));
        }
    }
    latest
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
                    let Some(data_root) = resolve_windows_data_root(content.trim()) else {
                        continue;
                    };
                    if data_root.is_dir() {
                        let pattern = data_root.join("xwechat_files");
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
    candidates.sort_by_key(|p| latest_db_mtime(p).unwrap_or(std::time::SystemTime::UNIX_EPOCH));
    candidates.into_iter().next_back()
}

/// Resolve the data-root path that Weixin writes to its `*.ini` file under
/// `%APPDATA%\Tencent\xwechat\config\`.
///
/// Observed forms in the wild:
///   - A plain absolute path, e.g. `D:\WeChatFiles`.
///   - The literal token `MyDocument:` (sometimes with a trailing slash),
///     which is not a real filesystem path. Empirically this denotes
///     "the current user's Documents folder"; users who relocated
///     Documents to e.g. `D:\Documents` saw auto-detect fail silently
///     because `PathBuf::from("MyDocument:").is_dir()` is false.
///
/// We accept either form. For the `MyDocument:` token we resolve via
/// `SHGetKnownFolderPath(FOLDERID_Documents)`, which respects the standard
/// shell-folder redirect at
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders\Personal`.
#[cfg(target_os = "windows")]
fn resolve_windows_data_root(content: &str) -> Option<PathBuf> {
    let trimmed = content.trim();
    // Strip an optional trailing slash so `MyDocument:\` and `MyDocument:/` also match.
    let stripped = trimmed
        .strip_suffix(['\\', '/'])
        .unwrap_or(trimmed);
    if stripped.eq_ignore_ascii_case("MyDocument:") {
        return known_documents_dir();
    }
    Some(PathBuf::from(trimmed))
}

#[cfg(target_os = "windows")]
fn known_documents_dir() -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{
        FOLDERID_Documents, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
    };

    // SAFETY: standard Win32 known-folder API. SHGetKnownFolderPath either returns
    // a heap-allocated PWSTR that the caller must free with CoTaskMemFree, or an
    // error — in which case the out-pointer is not allocated. We free on every
    // success path. Passing a null token (HANDLE::default()) means "the calling
    // user", which is exactly what we want.
    unsafe {
        let pwstr =
            SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, HANDLE::default()).ok()?;
        if pwstr.0.is_null() {
            return None;
        }
        // Walk the NUL-terminated wide string to compute its length.
        let mut len = 0usize;
        while *pwstr.0.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(pwstr.0, len);
        let os_str = OsString::from_wide(slice);
        CoTaskMemFree(Some(pwstr.0 as *const _));
        let path = PathBuf::from(os_str);
        if path.as_os_str().is_empty() {
            None
        } else {
            Some(path)
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn detect_db_dir_impl() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::{
        config_path_in_dir, default_config_path, find_existing_config_path, home_config_path,
        resolve_cli_home,
    };
    #[cfg(target_os = "windows")]
    use super::{known_documents_dir, resolve_windows_data_root};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = format!(
            "wx-cli-config-test-{}-{}-{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_cli_home_prefers_sudo_home_when_present() {
        let home = resolve_cli_home(PathBuf::from("/root"), Some(PathBuf::from("/Users/alice")));
        assert_eq!(home, PathBuf::from("/Users/alice"));
    }

    #[test]
    fn resolve_cli_home_falls_back_to_default_home() {
        let home = resolve_cli_home(PathBuf::from("/root"), None);
        assert_eq!(home, PathBuf::from("/root"));
    }

    #[test]
    fn config_path_prefers_cwd_over_exe_and_home() {
        let cwd = temp_dir("cwd");
        let exe = temp_dir("exe");
        let home = temp_dir("home");
        fs::write(config_path_in_dir(&cwd), "{}").unwrap();
        fs::write(config_path_in_dir(&exe), "{}").unwrap();
        fs::create_dir_all(home.join(".wx-cli")).unwrap();
        fs::write(home_config_path(&home), "{}").unwrap();

        let path = find_existing_config_path(Some(&cwd), Some(&exe), Some(&home)).unwrap();
        assert_eq!(path, config_path_in_dir(&cwd));

        fs::remove_dir_all(cwd).unwrap();
        fs::remove_dir_all(exe).unwrap();
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn default_config_path_matches_init_write_order() {
        let cwd = PathBuf::from("/tmp/cwd");
        let exe = PathBuf::from("/tmp/exe");
        let home = PathBuf::from("/tmp/home");

        let path = default_config_path(Some(&cwd), Some(&exe), Some(&home));
        assert_eq!(path, cwd.join("config.json"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn resolve_windows_data_root_passes_through_absolute_path() {
        let p = resolve_windows_data_root("D:\\WeChatFiles").unwrap();
        assert_eq!(p, PathBuf::from("D:\\WeChatFiles"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn resolve_windows_data_root_recognises_mydocument_keyword() {
        // Should match the keyword exactly (case-insensitive, with or without trailing slash)
        // and resolve to a non-empty Documents path via SHGetKnownFolderPath.
        let docs = known_documents_dir().expect("Documents known folder must resolve");
        for keyword in ["MyDocument:", "mydocument:", "MyDocument:\\", "MyDocument:/"] {
            let resolved = resolve_windows_data_root(keyword)
                .unwrap_or_else(|| panic!("keyword {keyword:?} should resolve"));
            assert_eq!(resolved, docs, "keyword {keyword:?}");
        }
    }
}
