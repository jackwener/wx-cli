mod init;
pub mod sessions;
pub mod history;
pub mod search;
pub mod send;
pub mod contacts;
pub mod export;
pub mod daemon_cmd;
pub mod transport;
pub mod output;
pub mod unread;
pub mod members;
pub mod new_messages;
pub mod stats;
pub mod favorites;
pub mod sns_notifications;
pub mod sns_feed;
pub mod sns_search;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// wx — 微信本地数据 CLI
#[derive(Parser)]
#[command(name = "wx", version = env!("CARGO_PKG_VERSION"), about = "wx — 微信本地数据 CLI")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 初始化：检测数据目录并扫描加密密钥
    Init {
        /// 强制重新扫描（覆盖已有配置）
        #[arg(long)]
        force: bool,
    },
    /// 列出最近会话
    Sessions {
        /// 会话数量
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 查看聊天记录
    History {
        /// 聊天对象名称（支持模糊匹配）
        chat: String,
        /// 消息数量
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,
        /// 分页偏移
        #[arg(long, default_value = "0")]
        offset: usize,
        /// 起始时间 YYYY-MM-DD
        #[arg(long)]
        since: Option<String>,
        /// 结束时间 YYYY-MM-DD
        #[arg(long)]
        until: Option<String>,
        /// 消息类型过滤 [text|image|voice|video|sticker|location|link|file|call|system]
        #[arg(long = "type", value_name = "TYPE",
              value_parser = ["text","image","voice","video","sticker","location","link","file","call","system"])]
        msg_type: Option<String>,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 搜索消息
    Search {
        /// 搜索关键词
        keyword: String,
        /// 限定聊天（可多次指定）
        #[arg(long = "in", value_name = "CHAT")]
        chats: Vec<String>,
        /// 结果数量
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
        /// 起始时间 YYYY-MM-DD
        #[arg(long)]
        since: Option<String>,
        /// 结束时间 YYYY-MM-DD
        #[arg(long)]
        until: Option<String>,
        /// 消息类型过滤 [text|image|voice|video|sticker|location|link|file|call|system]
        #[arg(long = "type", value_name = "TYPE",
              value_parser = ["text","image","voice","video","sticker","location","link","file","call","system"])]
        msg_type: Option<String>,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 发送微信消息（macOS 屏幕自动化）
    Send {
        /// 聊天对象名称
        chat: String,
        /// 要发送的消息
        #[arg(allow_hyphen_values = true)]
        message: String,
    },
    /// 查看联系人
    Contacts {
        /// 按名字过滤
        #[arg(short = 'q', long)]
        query: Option<String>,
        /// 显示数量
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 导出聊天记录到文件
    Export {
        /// 聊天对象名称
        chat: String,
        /// 起始时间 YYYY-MM-DD
        #[arg(long)]
        since: Option<String>,
        /// 结束时间 YYYY-MM-DD
        #[arg(long)]
        until: Option<String>,
        /// 最多导出条数
        #[arg(short = 'n', long, default_value = "500")]
        limit: usize,
        /// 输出格式 [markdown|txt|json|yaml]
        #[arg(short = 'f', long, default_value = "markdown", value_parser = ["markdown", "txt", "json", "yaml"])]
        format: String,
        /// 输出文件（默认 stdout）
        #[arg(short = 'o', long)]
        output: Option<String>,
    },
    /// 显示有未读消息的会话
    Unread {
        /// 显示数量
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
        /// 按会话类型过滤，逗号分隔。示例：--filter private,group 只看真人的未读
        #[arg(long, value_name = "TYPES", value_delimiter = ',',
              value_parser = ["all", "private", "group", "official", "folded"])]
        filter: Vec<String>,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 查看群成员
    Members {
        /// 群聊名称（支持模糊匹配）
        chat: String,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 获取自上次检查以来的新消息
    NewMessages {
        /// 显示数量上限
        #[arg(short = 'n', long, default_value = "200")]
        limit: usize,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 聊天统计分析
    Stats {
        /// 聊天对象名称（支持模糊匹配）
        chat: String,
        /// 起始时间 YYYY-MM-DD
        #[arg(long)]
        since: Option<String>,
        /// 结束时间 YYYY-MM-DD
        #[arg(long)]
        until: Option<String>,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 查看微信收藏内容
    Favorites {
        /// 显示数量
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,
        /// 类型过滤 [text|image|article|card|video]
        #[arg(long = "type", value_name = "TYPE",
              value_parser = ["text","image","article","card","video"])]
        fav_type: Option<String>,
        /// 内容关键词搜索
        #[arg(short = 'q', long)]
        query: Option<String>,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 朋友圈互动通知：别人对我的朋友圈点赞/评论 + 我评过的帖子下的跟帖
    SnsNotifications {
        /// 显示数量
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,
        /// 起始时间 YYYY-MM-DD
        #[arg(long)]
        since: Option<String>,
        /// 结束时间 YYYY-MM-DD
        #[arg(long)]
        until: Option<String>,
        /// 包含已读通知（默认仅未读）
        #[arg(long)]
        include_read: bool,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 朋友圈时间线：按时间/作者筛选本地缓存的朋友圈
    SnsFeed {
        /// 显示数量
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
        /// 起始时间 YYYY-MM-DD
        #[arg(long)]
        since: Option<String>,
        /// 结束时间 YYYY-MM-DD
        #[arg(long)]
        until: Option<String>,
        /// 只看指定作者（昵称 / 备注名 / 微信 ID，模糊匹配）
        #[arg(long)]
        user: Option<String>,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 朋友圈全文搜索：匹配正文关键词
    SnsSearch {
        /// 关键词
        keyword: String,
        /// 结果数量
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
        /// 起始时间 YYYY-MM-DD
        #[arg(long)]
        since: Option<String>,
        /// 结束时间 YYYY-MM-DD
        #[arg(long)]
        until: Option<String>,
        /// 限定作者（昵称 / 备注名 / 微信 ID）
        #[arg(long)]
        user: Option<String>,
        /// 输出 JSON（默认 YAML）
        #[arg(long)]
        json: bool,
    },
    /// 管理 wx-daemon
    Daemon {
        #[command(subcommand)]
        cmd: DaemonCommands,
    },
}

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// 查看 daemon 运行状态
    Status,
    /// 停止 daemon
    Stop,
    /// 查看 daemon 日志
    Logs {
        /// 持续输出（tail -f）
        #[arg(short = 'f', long)]
        follow: bool,
        /// 显示最近 N 行
        #[arg(short = 'n', long, default_value = "50")]
        lines: usize,
    },
}

pub fn run() {
    let cli = Cli::parse();
    if let Err(e) = dispatch(cli) {
        eprintln!("错误: {}", e);
        std::process::exit(1);
    }
}

fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init { force } => init::cmd_init(force),
        Commands::Sessions { limit, json } => sessions::cmd_sessions(limit, json),
        Commands::History { chat, limit, offset, since, until, msg_type, json } => {
            history::cmd_history(chat, limit, offset, since, until, msg_type, json)
        }
        Commands::Search { keyword, chats, limit, since, until, msg_type, json } => {
            search::cmd_search(keyword, chats, limit, since, until, msg_type, json)
        }
        Commands::Send { chat, message } => send::cmd_send(chat, message),
        Commands::Contacts { query, limit, json } => contacts::cmd_contacts(query, limit, json),
        Commands::Export { chat, since, until, limit, format, output } => {
            export::cmd_export(chat, since, until, limit, format, output)
        }
        Commands::Unread { limit, filter, json } => unread::cmd_unread(limit, filter, json),
        Commands::Members { chat, json } => members::cmd_members(chat, json),
        Commands::NewMessages { limit, json } => new_messages::cmd_new_messages(limit, json),
        Commands::Stats { chat, since, until, json } => {
            stats::cmd_stats(chat, since, until, json)
        }
        Commands::Favorites { limit, fav_type, query, json } => {
            favorites::cmd_favorites(limit, fav_type, query, json)
        }
        Commands::SnsNotifications { limit, since, until, include_read, json } => {
            sns_notifications::cmd_sns_notifications(limit, since, until, include_read, json)
        }
        Commands::SnsFeed { limit, since, until, user, json } => {
            sns_feed::cmd_sns_feed(limit, since, until, user, json)
        }
        Commands::SnsSearch { keyword, limit, since, until, user, json } => {
            sns_search::cmd_sns_search(keyword, limit, since, until, user, json)
        }
        Commands::Daemon { cmd } => daemon_cmd::cmd_daemon(cmd),
    }
}
