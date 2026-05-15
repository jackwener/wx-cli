use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config;
use crate::ipc::{Request, Response};

const STARTUP_TIMEOUT_SECS: u64 = 15;
const TCP_CONNECT_TIMEOUT_SECS: u64 = 15;
const TCP_RW_TIMEOUT_SECS: u64 = 120;

#[cfg(unix)]
const STOP_TIMEOUT_MS: u64 = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PidFile {
    pid: u32,
    #[serde(default)]
    exe: Option<PathBuf>,
}

/// 检查 daemon 是否存活
pub fn is_alive(tcp_addr: Option<&str>) -> bool {
    if let Some(addr) = tcp_addr {
        return is_alive_tcp(addr);
    }

    #[cfg(unix)]
    {
        ping_unix().unwrap_or(false)
    }
    #[cfg(windows)]
    {
        ping_windows().unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// TCP liveness check: send ping via TCP, return true if pong received
pub fn is_alive_tcp(addr: &str) -> bool {
    let tcp_addr = match addr.parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let mut stream = match TcpStream::connect_timeout(
        &tcp_addr,
        Duration::from_secs(TCP_CONNECT_TIMEOUT_SECS),
    ) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let req = serde_json::json!({"cmd": "ping"});
    if write!(stream, "{}\n", req).is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(&line)
        .ok()
        .and_then(|v| v.get("pong").and_then(|p| p.as_bool()))
        .unwrap_or(false)
}

/// 确保 daemon 运行，必要时自动启动
/// 当指定 tcp_addr 时，不会自动启动 daemon（用户显式选择了 TCP 模式）
pub fn ensure_daemon(tcp_addr: Option<&str>) -> Result<()> {
    if is_alive(tcp_addr) {
        return Ok(());
    }

    // TCP 模式下不自动启动 daemon，直接报错
    if tcp_addr.is_some() {
        let addr = tcp_addr.unwrap();
        bail!(
            "无法连接到 TCP daemon ({})：{}\n请确认 daemon 已通过 `wx daemon start --tcp {}` 启动",
            addr,
            std::io::Error::last_os_error(),
            addr,
        );
    }

    eprintln!("启动 wx-daemon...");
    start_daemon()?;
    Ok(())
}

/// 停止 daemon（如果正在运行）
pub fn stop_daemon() -> Result<()> {
    let pid_path = config::pid_path();
    let pid_file = read_pid_file(&pid_path)?;
    let daemon_alive = is_alive(None);

    match pid_file {
        Some(pid_file) => {
            let belongs = pid_belongs_to_daemon(&pid_file)?;
            if daemon_alive && !belongs {
                bail!(
                    "daemon 正在运行，但 {} 指向的 PID {} 无法确认属于当前 wx-daemon",
                    pid_path.display(),
                    pid_file.pid
                );
            }
            if belongs {
                terminate_pid(pid_file.pid)?;
            }
        }
        None if daemon_alive => {
            bail!(
                "daemon 正在运行，但 {} 缺失或损坏，无法安全停止",
                pid_path.display()
            );
        }
        None => {}
    }

    cleanup_ipc_files();
    Ok(())
}

/// 启动 daemon 进程（自身二进制，设置 WX_DAEMON_MODE=1）
///
/// tracing 已在子进程 main() 中直接写入 daemon.log，无需重定向 stdout/stderr。
fn start_daemon() -> Result<()> {
    let exe = std::env::current_exe().context("无法获取当前可执行文件路径")?;
    let child_pid: u32;

    // 预检：当前用户是否能写 ~/.wx-cli/。如果不能，给出可操作的错误信息，
    // 而不是 spawn 一个注定失败的 daemon 然后超时 15s。
    let cli_dir = config::cli_dir();
    if let Err(e) = std::fs::create_dir_all(&cli_dir) {
        bail!("无法创建 {}: {}", cli_dir.display(), e);
    }

    // 日志文件：~/.wx-cli/daemon.log
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
        let child = cmd.spawn().context("无法启动 daemon 进程")?;
        child_pid = child.id();
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
        let child = std::process::Command::new(&exe)
            .env("WX_DAEMON_MODE", "1")
            .stdin(std::process::Stdio::null())
            .stdout(stdout_stdio)
            .stderr(stderr_stdio)
            .creation_flags(0x00000008) // DETACHED_PROCESS
            .spawn()
            .context("无法启动 daemon 进程")?;
        child_pid = child.id();
    }

    // 等待 daemon 就绪（最多 STARTUP_TIMEOUT_SECS 秒）
    let deadline = std::time::Instant::now() + Duration::from_secs(STARTUP_TIMEOUT_SECS);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(300));
        if is_alive(None) {
            write_pid_file(child_pid, &exe)?;
            return Ok(());
        }
    }

    bail!(
        "wx-daemon 启动超时（>{}s）\n请查看日志: {}",
        STARTUP_TIMEOUT_SECS,
        config::log_path().display()
    )
}

fn write_pid_file(pid: u32, exe: &Path) -> Result<()> {
    if let Some(parent) = config::pid_path().parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 {} 失败", parent.display()))?;
    }
    let pid_file = PidFile {
        pid,
        exe: Some(exe.to_path_buf()),
    };
    let content = serde_json::to_string(&pid_file)?;
    std::fs::write(config::pid_path(), content)
        .with_context(|| format!("写入 {} 失败", config::pid_path().display()))?;
    Ok(())
}

fn read_pid_file(path: &Path) -> Result<Option<PidFile>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("读取 {} 失败", path.display())),
    };
    if let Ok(pid_file) = serde_json::from_str::<PidFile>(&content) {
        return Ok(Some(pid_file));
    }
    if let Ok(pid) = content.trim().parse::<u32>() {
        return Ok(Some(PidFile {
            pid,
            exe: std::env::current_exe().ok(),
        }));
    }
    bail!("{} 不是合法的 PID 文件", path.display())
}

fn cleanup_ipc_files() {
    let _ = std::fs::remove_file(config::sock_path());
    let _ = std::fs::remove_file(config::pid_path());
}

#[cfg(unix)]
fn ping_unix() -> Result<bool> {
    use std::os::unix::net::UnixStream;
    let sock_path = config::sock_path();
    if !sock_path.exists() {
        return Ok(false);
    }
    let mut stream = UnixStream::connect(&sock_path)?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

    let req = serde_json::to_string(&Request::Ping)? + "\n";
    stream.write_all(req.as_bytes())?;

    let mut line = String::new();
    let mut reader = BufReader::new(&stream);
    reader.read_line(&mut line)?;

    let resp: Response = serde_json::from_str(&line)?;
    Ok(resp.ok && resp.data.get("pong").and_then(|p| p.as_bool()) == Some(true))
}

#[cfg(windows)]
fn ping_windows() -> Result<bool> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, Stream};

    let name = "wx-cli-daemon".to_ns_name::<GenericNamespaced>()?;
    let stream = Stream::connect(name)?;
    let mut reader = BufReader::new(stream);

    let req = serde_json::to_string(&Request::Ping)? + "\n";
    reader.get_mut().write_all(req.as_bytes())?;

    let mut line = String::new();
    reader.read_line(&mut line)?;

    let resp: Response = serde_json::from_str(&line)?;
    Ok(resp.ok && resp.data.get("pong").and_then(|p| p.as_bool()) == Some(true))
}

fn pid_belongs_to_daemon(pid_file: &PidFile) -> Result<bool> {
    let expected_exe = pid_file
        .exe
        .clone()
        .or_else(|| std::env::current_exe().ok());
    #[cfg(unix)]
    {
        unix_pid_matches_daemon(pid_file.pid, expected_exe.as_deref())
    }
    #[cfg(windows)]
    {
        windows_pid_matches_daemon(pid_file.pid, expected_exe.as_deref())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = expected_exe;
        Ok(true)
    }
}

#[cfg(unix)]
fn unix_pid_matches_daemon(pid: u32, expected_exe: Option<&Path>) -> Result<bool> {
    let Some(expected_exe) = expected_exe else {
        return Ok(false);
    };
    let output = std::process::Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .with_context(|| format!("读取 PID {} 的 command 失败", pid))?;
    if !output.status.success() {
        return Ok(false);
    }
    let command = String::from_utf8_lossy(&output.stdout);
    let expected = expected_exe.to_string_lossy();
    if command.contains(expected.as_ref()) {
        return Ok(true);
    }
    let Some(exe_name) = expected_exe.file_name().and_then(|name| name.to_str()) else {
        return Ok(false);
    };
    Ok(command
        .split_whitespace()
        .any(|part| part == exe_name || part.ends_with(&format!("/{}", exe_name))))
}

#[cfg(windows)]
fn windows_pid_matches_daemon(pid: u32, expected_exe: Option<&Path>) -> Result<bool> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let Some(expected_exe) = expected_exe else {
        return Ok(false);
    };
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(handle) => handle,
        Err(_) => return Ok(false),
    };

    let mut buf = vec![0u16; 260];
    let mut len = buf.len() as u32;
    let actual = unsafe {
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        result
    };
    if actual.is_err() {
        return Ok(false);
    }

    let actual_path = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
    Ok(normalize_exe_path(&actual_path) == normalize_exe_path(expected_exe))
}

#[cfg(windows)]
fn normalize_exe_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn terminate_pid(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        terminate_pid_unix(pid)
    }
    #[cfg(windows)]
    {
        terminate_pid_windows(pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        Ok(())
    }
}

#[cfg(unix)]
fn terminate_pid_unix(pid: u32) -> Result<()> {
    let rc = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    if rc != 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }
        bail!("停止 PID {} 失败: {}", pid, err);
    }

    let deadline = std::time::Instant::now() + Duration::from_millis(STOP_TIMEOUT_MS);
    while std::time::Instant::now() < deadline {
        if !unix_process_exists(pid) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    bail!("等待 PID {} 退出超时", pid)
}

#[cfg(unix)]
fn unix_process_exists(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as i32, 0) };
    if rc == 0 {
        return true;
    }
    let err = std::io::Error::last_os_error();
    err.raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn terminate_pid_windows(pid: u32) -> Result<()> {
    let status = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .status()
        .with_context(|| format!("执行 taskkill /PID {} 失败", pid))?;
    if !status.success() {
        bail!("停止 PID {} 失败: taskkill exit {:?}", pid, status.code());
    }
    Ok(())
}

/// 向 daemon 发送请求并返回响应
pub fn send(req: Request, tcp_addr: Option<&str>) -> Result<Response> {
    if let Some(addr) = tcp_addr {
        return send_tcp(req, addr);
    }

    ensure_daemon(None)?;

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

/// 通过 TCP 发送请求并返回响应
pub fn send_tcp(req: Request, addr: &str) -> Result<Response> {
    let mut stream = TcpStream::connect_timeout(
        &addr.parse().context("TCP 地址格式无效")?,
        Duration::from_secs(TCP_CONNECT_TIMEOUT_SECS),
    )
    .context(format!("连接 TCP daemon ({}) 失败", addr))?;

    stream
        .set_read_timeout(Some(Duration::from_secs(TCP_RW_TIMEOUT_SECS)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(TCP_RW_TIMEOUT_SECS)))
        .ok();

    let req_str = serde_json::to_string(&req)? + "\n";
    stream.write_all(req_str.as_bytes())?;

    let mut line = String::new();
    let mut reader = BufReader::new(&stream);
    reader.read_line(&mut line)?;

    let resp: Response = serde_json::from_str(&line)
        .context("解析 daemon 响应失败")?;

    if !resp.ok {
        bail!("{}", resp.error.as_deref().unwrap_or("未知错误"));
    }

    Ok(resp)
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

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::ipc::{Request, Response};
    use serde_json::json;
    use std::net::SocketAddr;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Spawn a mock TCP server that responds to one request with the given JSON data.
    /// Returns the bound address (with the actual random port).
    async fn spawn_mock_server(response_body: serde_json::Value) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();

            // Read one line (the request)
            let mut buf_reader = tokio::io::BufReader::new(reader);
            let mut line = String::new();
            buf_reader.read_line(&mut line).await.unwrap();

            // Write response as a JSON line
            let resp = Response {
                ok: true,
                error: None,
                data: response_body,
            };
            let resp_str = serde_json::to_string(&resp).unwrap() + "\n";
            writer.write_all(resp_str.as_bytes()).await.unwrap();
            writer.shutdown().await.unwrap();
        });

        addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_send_tcp_round_trip() {
        let addr = spawn_mock_server(json!({
            "sessions": [{"name": "test"}]
        }))
        .await;

        let resp = send_tcp(Request::Sessions { limit: 20 }, &addr.to_string()).unwrap();
        assert!(resp.ok, "Response should be ok");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_send_tcp_connection_refused() {
        // Port 59876 is very unlikely to have a listener
        let result = send_tcp(Request::Sessions { limit: 20 }, "127.0.0.1:59876");
        assert!(result.is_err(), "Expected connection refused error");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_is_alive_tcp_false() {
        // Port 59877 is very unlikely to have a listener
        let result = is_alive_tcp("127.0.0.1:59877");
        assert!(!result, "Expected is_alive_tcp to return false for unused port");
    }
}

/// Real TCP daemon integration tests — spawn the actual `wx` daemon binary,
/// connect via TCP, and verify end-to-end request/response round-trip.
///
/// These tests are `#[cfg(unix)]` only and require the `wx` binary to have
/// been built with `cargo build --bin wx`.
#[cfg(unix)]
#[cfg(test)]
mod tcp_integration_tests {
    use super::*;
    use crate::ipc::Request;
    use std::process::Command;

    /// Build the `wx` binary so the daemon subprocess is available.
    fn ensure_binary() -> std::path::PathBuf {
        let status = Command::new("cargo")
            .args(["build", "--bin", "wx"])
            .output()
            .expect("cargo build failed to execute");
        if !status.status.success() {
            panic!(
                "cargo build --bin wx failed:\n{}",
                String::from_utf8_lossy(&status.stderr)
            );
        }
        // Binary path: target/debug/wx
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("target/debug/wx");
        assert!(p.exists(), "wx binary not found at {:?}", p);
        p
    }

    /// Wait for the daemon TCP endpoint to become ready.
    fn wait_for_tcp_ready(addr: &str) -> bool {
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_secs(STARTUP_TIMEOUT_SECS);
        while std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(300));
            if is_alive_tcp(addr) {
                return true;
            }
        }
        false
    }

    #[test]
    fn test_tcp_daemon_ping_round_trip() {
        let binary = ensure_binary();

        // Pick a free ephemeral port
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0")
                .expect("failed to bind ephemeral port");
            listener.local_addr().unwrap().port()
        };
        let addr = format!("127.0.0.1:{}", port);

        // Spawn the daemon subprocess in TCP-only mode
        let mut child = Command::new(&binary)
            .env("WX_DAEMON_MODE", "1")
            .env("WX_DAEMON_TCP_ADDR", &addr)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to spawn wx daemon");

        let pid = child.id();
        eprintln!("[test] spawned daemon PID {}", pid);

        // Wait for TCP readiness
        if !wait_for_tcp_ready(&addr) {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "daemon did not become ready on {} within {}s (PID {})",
                addr, STARTUP_TIMEOUT_SECS, pid
            );
        }
        eprintln!("[test] daemon ready on {}", addr);

        // Send Ping request and verify pong
        let resp = send_tcp(Request::Ping, &addr)
            .expect("send_tcp(Ping) should succeed");
        assert!(resp.ok, "Response ok flag should be true");

        let pong = resp.data.get("pong").and_then(|v| v.as_bool());
        assert!(
            pong == Some(true),
            "Expected pong=true in response, got: {:?}",
            resp.data
        );

        // Terminate daemon
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };

        // Verify clean exit
        let exit_status = child.wait().expect("failed to wait on daemon");
        assert!(
            exit_status.success(),
            "daemon should exit cleanly, got: {:?}",
            exit_status
        );
    }

    #[test]
    fn test_tcp_daemon_connection_refused() {
        // Port 59889 is very unlikely to have a listener
        let addr = "127.0.0.1:59889";
        let result = send_tcp(Request::Ping, addr);
        assert!(
            result.is_err(),
            "Expected connection refused error when no daemon is listening on {}",
            addr
        );
    }

    /// Compare TCP and local transport responses for the same query.
    /// Marked `#[ignore]` because it requires WeChat data to be present on the machine.
    /// Run manually: `cargo test -- --ignored test_tcp_matches_local_sessions`
    #[test]
    #[ignore]
    fn test_tcp_matches_local_sessions() {
        let binary = ensure_binary();

        // Pick a free ephemeral port for TCP daemon
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0")
                .expect("failed to bind ephemeral port");
            listener.local_addr().unwrap().port()
        };
        let tcp_addr = format!("127.0.0.1:{}", port);

        // --- Phase 1: Query via TCP ---
        let mut tcp_child = Command::new(&binary)
            .env("WX_DAEMON_MODE", "1")
            .env("WX_DAEMON_TCP_ADDR", &tcp_addr)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to spawn wx daemon (TCP)");
        let tcp_pid = tcp_child.id();
        eprintln!("[test] TCP daemon PID {}", tcp_pid);

        if !wait_for_tcp_ready(&tcp_addr) {
            let _ = tcp_child.kill();
            let _ = tcp_child.wait();
            panic!("TCP daemon did not become ready within {}s", STARTUP_TIMEOUT_SECS);
        }
        eprintln!("[test] TCP daemon ready on {}", tcp_addr);

        let tcp_resp = send_tcp(Request::Sessions { limit: 20 }, &tcp_addr)
            .expect("send_tcp(Sessions) should succeed");

        // Terminate TCP daemon
        unsafe { libc::kill(tcp_pid as libc::pid_t, libc::SIGTERM) };
        let _ = tcp_child.wait();
        eprintln!("[test] TCP daemon terminated");

        // --- Phase 2: Query via local transport ---
        // send() with tcp_addr=None will auto-start a daemon on the Unix socket
        let local_resp = send(Request::Sessions { limit: 20 }, None)
            .expect("send(Sessions) via local transport should succeed");

        // --- Phase 3: Deep-compare responses ---
        let tcp_data = serde_json::to_value(&tcp_resp.data)
            .expect("tcp_resp.data should be serializable");
        let local_data = serde_json::to_value(&local_resp.data)
            .expect("local_resp.data should be serializable");

        assert_eq!(
            tcp_data, local_data,
            "TCP and local transport responses differ!\nTCP:   {}\nLocal: {}",
            serde_json::to_string_pretty(&tcp_data).unwrap(),
            serde_json::to_string_pretty(&local_data).unwrap(),
        );
        eprintln!("[test] TCP and local responses match ✓");
    }
}
