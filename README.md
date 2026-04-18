<div align="center">

# wx-cli

**从命令行查询本地微信数据**

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)](#安装)
[![Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org)

会话 · 聊天记录 · 搜索 · 联系人 · 群成员 · 收藏 · 统计 · 导出

</div>

---

## AI Agent Skill

通过 [skills CLI](https://github.com/vercel-labs/skills) 一键安装到 Claude Code、Cursor、Codex 等 agent：

```bash
npx skills add jackwener/wx-cli
```

或全局安装：

```bash
npx skills add jackwener/wx-cli -g
```

安装后 agent 会自动读取 `SKILL.md`，了解如何安装和调用 wx-cli。

---

## 特性

- **零依赖安装** — 单一 Rust 二进制，一行命令装完
- **毫秒级响应** — 后台 daemon 持久缓存解密数据库，mtime 不变则复用
- **AI 友好** — 默认 YAML 输出，更省 token & 易读；`--json` 可切换为 JSON（方便 `jq` 处理等）
- **完全本地** — 数据不出本机，实时解密，无需全量预解密

---

## 安装

**npm（推荐，全平台）**

```bash
npm install -g @jackwener/wx-cli
```

**macOS / Linux（curl）**

```bash
curl -fsSL https://raw.githubusercontent.com/jackwener/wx-cli/main/install.sh | bash
```

**Windows**（PowerShell，以管理员身份运行）

```powershell
irm https://raw.githubusercontent.com/jackwener/wx-cli/main/install.ps1 | iex
```

<details>
<summary>其他安装方式</summary>

**手动下载**

从 [Releases](https://github.com/jackwener/wx-cli/releases) 下载对应平台文件：

| 平台 | 文件 |
|------|------|
| macOS Apple Silicon | `wx-macos-arm64` |
| macOS Intel | `wx-macos-x86_64` |
| Linux x86_64 | `wx-linux-x86_64` |
| Linux arm64 | `wx-linux-arm64` |
| Windows x86_64 | `wx-windows-x86_64.exe` |

macOS / Linux：`chmod +x wx && sudo mv wx /usr/local/bin/`

**从源码构建**

```bash
git clone git@github.com:jackwener/wx-cli.git && cd wx-cli
cargo build --release
# 产物：target/release/wx（Windows: wx.exe）
```

</details>

---

## 快速开始

保持微信运行，然后初始化（只需一次）：

**macOS**（需要先对微信做 ad-hoc 签名，才能扫描其内存）

```bash
# 1. 签名（只需做一次，WeChat 更新后重做）
codesign --force --deep --sign - /Applications/WeChat.app

# 2. 重启微信，等待完全登录
killall WeChat && open /Applications/WeChat.app

# 3. 初始化
sudo wx init
```

> 如果 `codesign` 报 `signature in use`，先执行：
> ```bash
> codesign --remove-signature "/Applications/WeChat.app/Contents/Frameworks/vlc_plugins/librtp_mpeg4_plugin.dylib"
> codesign --force --deep --sign - /Applications/WeChat.app
> ```

**Linux**

```bash
sudo wx init
```

**Windows**（以管理员身份运行 PowerShell）

```powershell
wx init
```

验证安装：

```bash
wx sessions
```

能看到最近会话即表示一切正常。daemon 在首次调用时自动启动。

---

## 命令

### 消息

```bash
wx sessions                                      # 最近 20 个会话
wx unread                                        # 有未读消息的会话
wx unread --filter private,group                 # 只看真人未读（过滤公众号/折叠入口）
wx new-messages                                  # 上次检查后的新消息（增量）
wx history "张三"                                # 最近 50 条记录
wx history "张三" -n 2000                        # 拉更多历史消息
wx history "AI群" --since 2026-04-01 --until 2026-04-15
wx search "关键词"                               # 全库搜索
wx search "关键词" -n 500                        # 放宽搜索结果条数
wx search "会议" --in "工作群" --since 2026-01-01
```

`history` / `search` / `export` 都支持 `-n` / `--limit` 指定条数。默认值只是为了避免一次性输出过多消息，不是硬上限。

会话/消息输出里都带 `chat_type` 字段，取值为 `private` / `group` / `official_account` / `folded`。`official_account` 涵盖公众号、订阅号、服务号及 `mphelper` / `qqsafe` 等系统通知；`folded` 对应微信里的"订阅号折叠"和"折叠群聊"两个聚合入口。

### 联系人 & 群组

```bash
wx contacts                  # 联系人列表
wx contacts --query "李"     # 按名字搜索
wx members "AI交流群"        # 群成员列表
```

### 收藏 & 统计

```bash
wx favorites                          # 全部收藏
wx favorites --type image             # 按类型筛选（text/image/article/card/video）
wx favorites --query "关键词"         # 搜索收藏内容
wx stats "AI群"                       # 聊天统计
wx stats "AI群" --since 2026-01-01   # 指定时间范围
```

### 导出

```bash
wx export "张三" --format markdown -o chat.md
wx export "张三" -n 2000 --format markdown -o chat.md
wx export "AI群" --since 2026-01-01 --format json
```

### 输出格式

默认输出 YAML，更省 token & 易读；`--json` 可切换为 JSON（方便 `jq` 处理等）：

```bash
wx sessions --json
wx search "关键词" --json | jq '.[0].content'
wx new-messages --json
```

### Daemon 管理

```bash
wx daemon status
wx daemon stop
wx daemon logs --follow
```

---

## 架构

```
wx (CLI) ──Unix socket──▶ wx-daemon (后台进程)
                              │
                    ┌─────────┴──────────┐
               DBCache               联系人缓存
           (mtime 感知复用)
```

daemon 首次解密后将数据库和 mtime 持久化到 `~/.wx-cli/cache/`。重启后 mtime 未变则直接复用，无需重解密。

```
~/.wx-cli/
├── config.json       # 配置
├── all_keys.json     # 数据库密钥
├── daemon.sock       # Unix socket
├── daemon.pid / .log
└── cache/
    ├── _mtimes.json  # mtime 索引
    └── *.db          # 解密后的数据库
```

---

## 原理

微信 4.x 使用 SQLCipher 4 加密本地数据库（AES-256-CBC + HMAC-SHA512，PBKDF2 256,000 次迭代）。WCDB 在进程内存中缓存派生后的 raw key，格式为 `x'<64hex_key><32hex_salt>'`。

wx-cli 通过 macOS Mach VM API（`mach_vm_region` + `mach_vm_read`）或 Linux `/proc/<pid>/mem` 扫描微信进程内存，匹配该模式提取密钥，daemon 按需解密并缓存。

---

## 致谢

本项目受 [ylytdeng/wechat-decrypt](https://github.com/ylytdeng/wechat-decrypt) 启发，在其基础上进行了重新设计与实现。感谢原作者的研究与探索。

---

## 免责声明

本工具仅用于学习和研究目的，用于解密**自己的**微信数据。请遵守相关法律法规，不得用于未经授权的数据访问。
