use crate::cli::transport;
use crate::cli::DaemonCommands;
use crate::config;
use anyhow::Result;

pub fn cmd_daemon(cmd: DaemonCommands, tcp_addr: Option<&str>) -> Result<()> {
    match cmd {
        DaemonCommands::Status => cmd_status(tcp_addr),
        DaemonCommands::Stop => cmd_stop(tcp_addr),
        DaemonCommands::Logs { follow, lines } => cmd_logs(follow, lines),
        DaemonCommands::Start { tcp } => crate::daemon::run_start(tcp.or_else(|| tcp_addr.map(String::from))),
    }
}

fn cmd_status(tcp_addr: Option<&str>) -> Result<()> {
    if transport::is_alive(tcp_addr) {
        let pid_path = config::pid_path();
        let pid = std::fs::read_to_string(&pid_path)
            .map(|s| {
                serde_json::from_str::<serde_json::Value>(&s)
                    .ok()
                    .and_then(|v| v.get("pid").and_then(|p| p.as_u64()))
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| s.trim().to_string())
            })
            .unwrap_or_else(|_| "?".into());
        if let Some(addr) = tcp_addr {
            println!("wx-daemon 运行中 (TCP {})", addr);
        } else {
            println!("wx-daemon 运行中 (PID {})", pid);
        }
    } else {
        println!("wx-daemon 未运行");
    }
    Ok(())
}

fn cmd_stop(tcp_addr: Option<&str>) -> Result<()> {
    // TCP daemon is a separate process — cannot stop via PID file
    if let Some(addr) = tcp_addr {
        eprintln!(
            "⚠ TCP daemon ({}) 是一个独立进程，无法通过 `wx daemon stop` 停止。\n\
             请手动关闭该进程（例如 kill / taskkill PID）。",
            addr
        );
        return Ok(());
    }

    if !transport::is_alive(tcp_addr) {
        println!("daemon 未运行");
        return Ok(());
    }

    transport::stop_daemon()?;
    println!("已停止 wx-daemon");
    Ok(())
}

fn cmd_logs(follow: bool, lines: usize) -> Result<()> {
    let log_path = config::log_path();
    if !log_path.exists() {
        println!("暂无日志");
        return Ok(());
    }

    if follow {
        #[cfg(unix)]
        {
            std::process::Command::new("tail")
                .args([&format!("-{}", lines), "-f", &log_path.to_string_lossy()])
                .status()?;
        }
        #[cfg(windows)]
        {
            use std::io::{Read, Seek, SeekFrom};
            let mut file = std::fs::File::open(&log_path)?;
            let len = file.seek(SeekFrom::End(0))?;
            let start = len.saturating_sub((lines as u64) * 200);
            file.seek(SeekFrom::Start(start))?;
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            let all_lines: Vec<&str> = content.lines().collect();
            let show = &all_lines[all_lines.len().saturating_sub(lines)..];
            for line in show {
                println!("{}", line);
            }
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let mut buf = String::new();
                file.read_to_string(&mut buf)?;
                if !buf.is_empty() {
                    print!("{}", buf);
                }
            }
        }
    } else {
        let content = std::fs::read_to_string(&log_path)?;
        let all_lines: Vec<&str> = content.lines().collect();
        let show = &all_lines[all_lines.len().saturating_sub(lines)..];
        for line in show {
            println!("{}", line);
        }
    }

    Ok(())
}
