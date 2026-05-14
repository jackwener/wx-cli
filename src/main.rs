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

    // CLI 路径不需要 tracing — 只输出用户可见的 stdout/stderr。
    // daemon 路径：tracing 直接写入 ~/.wx-cli/daemon.log，
    // 不依赖父进程的 stderr 重定向（避免重复写入）。
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info")
    });

    let _ = std::fs::create_dir_all(config::cli_dir());
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(config::log_path())
        .ok();

    match log_file {
        Some(file) => {
            tracing_subscriber::fmt()
                .with_target(false)
                .with_level(true)
                .with_env_filter(env_filter)
                .with_writer(file)
                .init();
        }
        None => {
            // 文件打开失败时退回到 stderr，确保日志不会静默丢失
            tracing_subscriber::fmt()
                .with_target(false)
                .with_level(true)
                .with_env_filter(env_filter)
                .with_writer(std::io::stderr)
                .init();
        }
    }
}
