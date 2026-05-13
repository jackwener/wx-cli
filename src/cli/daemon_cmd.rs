use anyhow::Result;
use crate::config;
use crate::cli::DaemonCommands;
use crate::cli::transport;

pub fn cmd_daemon(cmd: DaemonCommands) -> Result<()> {
    match cmd {
        DaemonCommands::Status => cmd_status(),
        DaemonCommands::Stop => cmd_stop(),
        DaemonCommands::Logs { follow, lines } => cmd_logs(follow, lines),
        DaemonCommands::Start { tcp } => crate::daemon::run_start(tcp),
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
    let pid_path = config::pid_path();
    if !pid_path.exists() {
        println!("daemon 未运行");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: u32 = pid_str.trim().parse()
        .map_err(|_| anyhow::anyhow!("PID 文件格式错误"))?;

    #[cfg(unix)]
    {
        let ret = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if ret != 0 {
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if errno == libc::ESRCH {
                println!("wx-daemon (PID {}) 已不在运行，清理残留文件", pid);
            } else {
                anyhow::bail!("发送 SIGTERM 失败 (errno {})", errno);
            }
        } else {
            println!("已停止 wx-daemon (PID {})", pid);
        }
    }

    #[cfg(windows)]
    {
        std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output()?;
        println!("已停止 wx-daemon (PID {})", pid);
    }

    let _ = std::fs::remove_file(config::sock_path());
    let _ = std::fs::remove_file(&pid_path);

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
            for line in show { println!("{}", line); }
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let mut buf = String::new();
                file.read_to_string(&mut buf)?;
                if !buf.is_empty() { print!("{}", buf); }
            }
        }
    } else {
        let content = std::fs::read_to_string(&log_path)?;
        let all_lines: Vec<&str> = content.lines().collect();
        let show = &all_lines[all_lines.len().saturating_sub(lines)..];
        for line in show { println!("{}", line); }
    }

    Ok(())
}
