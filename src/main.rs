mod config;
mod ipc;
mod crypto;
mod scanner;
mod daemon;
mod cli;
pub mod transport;
mod attachment;

fn main() {
    if std::env::var("WX_DAEMON_MODE").is_ok() {
        init_logging();
        daemon::run();
    } else {
        cli::run();
    }
}

fn init_logging() {
    use tracing_subscriber::EnvFilter;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::fs::File;

    // CLI 路径不需要 tracing — 只输出用户可见的 stdout/stderr。
    // daemon 路径：stderr 已通过 run_start() 重定向到 daemon.log，
    // 但我们也直接写入日志文件以覆盖直接设置 WX_DAEMON_MODE=1 的情况。
    static LOG_FILE: OnceLock<Arc<Mutex<Option<File>>>> = OnceLock::new();
    let file_entry = LOG_FILE.get_or_init(|| {
        let _ = std::fs::create_dir_all(config::cli_dir());
        Arc::new(Mutex::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(config::log_path())
                .ok(),
        ))
    });

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn")
    });
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .with_env_filter(env_filter)
        .with_writer(move || {
            let file_entry = Arc::clone(file_entry);
            let guard = file_entry.lock().unwrap();
            let b: Box<dyn std::io::Write + Send> = guard
                .as_ref()
                .and_then(|f| f.try_clone().ok())
                .map(|f| Box::new(f) as Box<dyn std::io::Write + Send>)
                .unwrap_or_else(|| Box::new(std::io::stderr()));
            TeeWriter {
                a: Box::new(std::io::stderr()),
                b,
            }
        })
        .init();
}

/// 同时写入两个 Write 目标
struct TeeWriter {
    a: Box<dyn std::io::Write + Send>,
    b: Box<dyn std::io::Write + Send>,
}

impl std::io::Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = self.b.write_all(buf);
        self.a.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = self.b.flush();
        self.a.flush()
    }
}
