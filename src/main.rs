mod config;
mod ipc;
mod crypto;
mod scanner;
mod daemon;
mod cli;
pub mod transport;

fn main() {
    if std::env::var("WX_DAEMON_MODE").is_ok() {
        daemon::run();
    } else {
        cli::run();
    }
}
