# wx-cli

> description: "wx-cli — 从本地微信数据库查询聊天记录、联系人、会话、收藏等。用户提到微信聊天记录、联系人、消息历史、群成员、收藏内容时，使用此 skill 安装并调用 wx-cli。"

## Triggers

- 查微信聊天记录
- 微信消息历史
- 微信联系人
- 微信群成员
- 微信收藏
- wechat history / messages / contacts
- wx-cli
- 帮我看看微信里
- 搜索微信消息

## Prerequisites

- macOS（Apple Silicon / Intel）或 Linux
- 微信桌面版 4.x 已安装并登录
- Node.js >= 14（npm 安装方式）或 curl（shell 安装方式）
- 首次 `wx init` 需要 `sudo`（内存扫描提取密钥）

---

## 安装

### 方式一：npm（推荐）

```bash
npm install -g @jackwener/wx-cli
```

### 方式二：curl

```bash
curl -fsSL https://raw.githubusercontent.com/jackwener/wx-cli/main/install.sh | bash
```

安装后验证：

```bash
wx --version
```

---

## 初始化（首次使用，只需一次）

### macOS（必须按顺序执行）

**第一步：对 WeChat 重新签名**（只需做一次，WeChat 更新后需重做）

```bash
codesign --force --deep --sign - /Applications/WeChat.app
```

如果报错 `signature in use` 或某个 dylib 签名损坏，先修复再签名：

```bash
codesign --remove-signature "/Applications/WeChat.app/Contents/Frameworks/vlc_plugins/librtp_mpeg4_plugin.dylib"
codesign --force --deep --sign - /Applications/WeChat.app
```

**第二步：重启 WeChat**

```bash
killall WeChat && open /Applications/WeChat.app
# 等待微信完全登录后再继续
```

**第三步：初始化**

```bash
sudo wx init
```

### Linux

```bash
sudo wx init
```

`wx init` 会自动：
1. 检测微信数据目录
2. 扫描进程内存，提取所有数据库密钥
3. 写入 `~/.wx-cli/config.json`

初始化完成后，后续所有命令无需 `sudo`，daemon 在首次调用时自动启动。

---

## 命令速查

所有命令默认输出 YAML。加 `--json` 切换为 JSON（适合程序处理）。

### 会话与消息

```bash
# 最近 20 个会话
wx sessions

# 有未读消息的会话
wx unread

# 上次检查后的新消息（增量）
wx new-messages
wx new-messages --json          # JSON 输出，适合 agent 解析

# 聊天记录（支持昵称/备注名）
wx history "张三"
wx history "AI群" --since 2026-04-01 --until 2026-04-15 -n 100

# 全库搜索
wx search "关键词"
wx search "会议" --in "工作群" --since 2026-01-01
```

### 联系人与群组

```bash
# 联系人列表 / 搜索
wx contacts
wx contacts -q "李"

# 群成员列表
wx members "AI交流群"
```

### 收藏与统计

```bash
# 全部收藏
wx favorites

# 按类型筛选：text / image / article / card / video
wx favorites --type image

# 搜索收藏内容
wx favorites -q "关键词"

# 聊天统计（发言人、消息类型、活跃时段）
wx stats "AI群"
wx stats "AI群" --since 2026-01-01
```

### 导出

```bash
# 导出为 Markdown（默认）
wx export "张三" --format markdown -o chat.md

# 导出为 JSON
wx export "AI群" --since 2026-01-01 --format json -o chat.json
```

### Daemon 管理

```bash
wx daemon status
wx daemon stop
wx daemon logs --follow
```

---

## Agent 使用建议

查询结果需要程序处理时，统一加 `--json`：

```bash
wx sessions --json
wx new-messages --json
wx search "关键词" --json
wx history "张三" --json -n 50
```

CHAT 参数支持昵称、备注名、微信 ID，模糊匹配。不确定准确名称时，先用 `wx contacts -q` 搜索。

---

## 数据文件位置

```
~/.wx-cli/
├── config.json       # 配置
├── all_keys.json     # 数据库密钥（敏感，勿分享）
├── daemon.sock       # Unix socket
├── daemon.pid / .log
└── cache/            # 解密后的数据库缓存
```

---

## 常见问题

**微信重启后密钥失效**：重新运行 `sudo wx init --force`（微信必须正在运行）。

**daemon 无响应**：`wx daemon stop` 后重新调用任意命令自动重启。

**找不到聊天**：用 `wx contacts -q` 确认昵称/备注名，或用微信 ID 直接查询。
