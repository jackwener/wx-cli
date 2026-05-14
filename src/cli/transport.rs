use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use crate::config;
use crate::ipc::{Request, Response};

const STARTUP_TIMEOUT_SECS: u64 = 15;

/// 检查 daemon 是否存活
pub fn is_alive() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::net::UnixStream;
        let sock_path = config::sock_path();
        if !sock_path.exists() {
            return false;
        }
        let mut stream = match UnixStream::connect(&sock_path) {
            Ok(s) => s,
            Err(_) => return false,
        };
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

        let req = serde_json::json!({"cmd": "ping"});
        if write!(stream, "{}\n", req).is_err() {
            return false;
        }
        let mut line = String::new();
        let mut reader = BufReader::new(&stream);
        if reader.read_line(&mut line).is_err() {
            return false;
        }
        serde_json::from_str::<serde_json::Value>(&line)
            .ok()
            .and_then(|v| v.get("pong").and_then(|p| p.as_bool()))
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        use interprocess::local_socket::{prelude::*, GenericNamespaced, Stream};
        // 必须用 interprocess 自己的连接 API，和 server 保持一致
        match "wx-cli-daemon".to_ns_name::<GenericNamespaced>() {
            Ok(name) => Stream::connect(name).is_ok(),
            Err(_) => false,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// [`stop_daemon`] 的返回值，供 CLI 提示或 `wx init --force` 选用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopDaemonOutcome {
    /// 无 `daemon.pid`
    NoPidFile,
    /// 已向进程发送停止信号并清理 pid/socket
    Stopped(u32),
    /// 进程已不存在，仅清理 pid/socket
    StalePid(u32),
}

/// 停止 wx-daemon（与 `wx daemon stop` 同一套逻辑）。
pub fn stop_daemon() -> Result<StopDaemonOutcome> {
    let pid_path = config::pid_path();
    if !pid_path.exists() {
        return Ok(StopDaemonOutcome::NoPidFile);
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: u32 = pid_str
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("PID 文件格式错误"))?;

    #[cfg(unix)]
    {
        let ret = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if ret != 0 {
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if errno == libc::ESRCH {
                let _ = std::fs::remove_file(config::sock_path());
                let _ = std::fs::remove_file(&pid_path);
                return Ok(StopDaemonOutcome::StalePid(pid));
            }
            anyhow::bail!("发送 SIGTERM 失败 (errno {})", errno);
        }
        let _ = std::fs::remove_file(config::sock_path());
        let _ = std::fs::remove_file(&pid_path);
        return Ok(StopDaemonOutcome::Stopped(pid));
    }

    #[cfg(windows)]
    {
        let out = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
        let ok = matches!(&out, Ok(o) if o.status.success());
        let _ = std::fs::remove_file(config::sock_path());
        let _ = std::fs::remove_file(&pid_path);
        return Ok(if ok {
            StopDaemonOutcome::Stopped(pid)
        } else {
            StopDaemonOutcome::StalePid(pid)
        });
    }

    #[cfg(not(any(unix, windows)))]
    {
        anyhow::bail!("当前平台不支持 stop_daemon");
    }
}

/// 确保 daemon 运行，必要时自动启动
pub fn ensure_daemon() -> Result<()> {
    if is_alive() {
        return Ok(());
    }
    eprintln!("启动 wx-daemon...");
    start_daemon()?;
    Ok(())
}

/// 启动 daemon 前检查 `~/.wx-cli/` 可写，给出比"超时"更明确的错误。
///
/// 典型坑：旧版本 `sudo wx init` 把目录留成 root 属主，非 root 的 daemon
/// 连 socket/log 都建不了，会静默失败 15s 超时。
fn preflight_cli_dir_writable() -> Result<()> {
    let cli_dir = config::cli_dir();
    std::fs::create_dir_all(&cli_dir)
        .with_context(|| format!("创建 {} 失败", cli_dir.display()))?;

    let probe = cli_dir.join(".daemon_probe");
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let dir = cli_dir.display();
            if cfg!(unix) {
                bail!(
                    "无法写入 {dir}（权限不足）\n\n\
                     这通常是老版本的 `sudo wx init` 把目录属主留成了 root。\n\
                     修复：\n\n    \
                     sudo chown -R $(whoami) {dir}\n\n\
                     （新版已修复此问题，下次 init 不会再发生）",
                )
            } else {
                bail!("无法写入 {dir}: {e}")
            }
        }
        Err(e) => bail!("无法写入 {}: {}", cli_dir.display(), e),
    }
}

/// 启动 daemon 进程（自身二进制，设置 WX_DAEMON_MODE=1）
fn start_daemon() -> Result<()> {
    let exe = std::env::current_exe().context("无法获取当前可执行文件路径")?;

    // 预检：当前用户是否能写 ~/.wx-cli/。如果不能，给出可操作的错误信息，
    // 而不是 spawn 一个注定失败的 daemon 然后超时 15s。
    preflight_cli_dir_writable()?;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // 日志文件：~/.wx-cli/daemon.log
        let log_path = config::log_path();
        // 确保父目录存在
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let (stdout_stdio, stderr_stdio) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .and_then(|f| f.try_clone().map(|g| (f, g)))
            .map(|(f, g)| (std::process::Stdio::from(f), std::process::Stdio::from(g)))
            .unwrap_or_else(|_| (std::process::Stdio::null(), std::process::Stdio::null()));
        let mut cmd = std::process::Command::new(&exe);
        cmd.env("WX_DAEMON_MODE", "1")
            .stdin(std::process::Stdio::null())
            .stdout(stdout_stdio)
            .stderr(stderr_stdio);
        // SAFETY: setsid() 在 fork 后的子进程中调用，使 daemon 脱离控制终端
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let _ = cmd.spawn().context("无法启动 daemon 进程")?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let log_path = config::log_path();
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let (stdout_stdio, stderr_stdio) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .and_then(|f| f.try_clone().map(|g| (f, g)))
            .map(|(f, g)| (std::process::Stdio::from(f), std::process::Stdio::from(g)))
            .unwrap_or_else(|_| (std::process::Stdio::null(), std::process::Stdio::null()));
        let _ = std::process::Command::new(&exe)
            .env("WX_DAEMON_MODE", "1")
            .stdin(std::process::Stdio::null())
            .stdout(stdout_stdio)
            .stderr(stderr_stdio)
            .creation_flags(0x00000008) // DETACHED_PROCESS
            .spawn()
            .context("无法启动 daemon 进程")?;
    }

    // 等待 daemon 就绪（最多 STARTUP_TIMEOUT_SECS 秒）
    let deadline = std::time::Instant::now() + Duration::from_secs(STARTUP_TIMEOUT_SECS);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(300));
        if is_alive() {
            return Ok(());
        }
    }

    bail!(
        "wx-daemon 启动超时（>{}s）\n请查看日志: {}",
        STARTUP_TIMEOUT_SECS,
        config::log_path().display()
    )
}

/// 向 daemon 发送请求并返回响应
pub fn send(req: Request) -> Result<Response> {
    ensure_daemon()?;

    #[cfg(unix)]
    {
        send_unix(req)
    }
    #[cfg(windows)]
    {
        send_windows(req)
    }
    #[cfg(not(any(unix, windows)))]
    {
        bail!("不支持当前平台")
    }
}

#[cfg(unix)]
fn send_unix(req: Request) -> Result<Response> {
    use std::os::unix::net::UnixStream;
    let sock_path = config::sock_path();
    let mut stream = UnixStream::connect(&sock_path).context("连接 daemon socket 失败")?;
    stream.set_read_timeout(Some(Duration::from_secs(120))).ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(120)))
        .ok();

    let req_str = serde_json::to_string(&req)? + "\n";
    stream.write_all(req_str.as_bytes())?;

    let mut line = String::new();
    let mut reader = BufReader::new(&stream);
    reader.read_line(&mut line)?;

    let resp: Response = serde_json::from_str(&line).context("解析 daemon 响应失败")?;

    if !resp.ok {
        bail!("{}", resp.error.as_deref().unwrap_or("未知错误"));
    }

    Ok(resp)
}

#[cfg(windows)]
fn send_windows(req: Request) -> Result<Response> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, Stream};

    let name = "wx-cli-daemon"
        .to_ns_name::<GenericNamespaced>()
        .context("构造 pipe name 失败")?;
    let stream = Stream::connect(name).context("连接 daemon named pipe 失败")?;

    // interprocess::Stream 同时实现 Read + Write，但需要拆分读写端
    let mut reader = BufReader::new(stream);

    let req_str = serde_json::to_string(&req)? + "\n";
    reader.get_mut().write_all(req_str.as_bytes())?;

    let mut line = String::new();
    reader.read_line(&mut line)?;

    let resp: Response = serde_json::from_str(&line).context("解析 daemon 响应失败")?;

    if !resp.ok {
        bail!("{}", resp.error.as_deref().unwrap_or("未知错误"));
    }

    Ok(resp)
}
