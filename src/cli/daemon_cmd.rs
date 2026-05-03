use crate::cli::transport;
use crate::cli::DaemonCommands;
use crate::config;
use anyhow::Result;

pub fn cmd_daemon(cmd: DaemonCommands) -> Result<()> {
    match cmd {
        DaemonCommands::Status => cmd_status(),
        DaemonCommands::Stop => cmd_stop(),
        DaemonCommands::Logs { follow, lines } => cmd_logs(follow, lines),
    }
}

fn cmd_status() -> Result<()> {
    if transport::is_alive() {
        let pid_path = config::pid_path();
        let pid = std::fs::read_to_string(&pid_path)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "?".into());
        println!("wx-daemon 运行中 (PID {})", pid);
    } else {
        println!("wx-daemon 未运行");
    }
    Ok(())
}

fn cmd_stop() -> Result<()> {
    match transport::stop_daemon()? {
        transport::StopDaemonOutcome::NoPidFile => println!("daemon 未运行"),
        transport::StopDaemonOutcome::Stopped(pid) => println!("已停止 wx-daemon (PID {})", pid),
        transport::StopDaemonOutcome::StalePid(pid) => {
            println!("wx-daemon (PID {}) 已不在运行，清理残留文件", pid);
        }
    }
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
