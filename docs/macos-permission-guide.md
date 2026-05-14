# macOS WeChat 密钥提取：权限与签名完全指南

> 基于多台机器 (macOS 10.15 ~ 15.x, Intel + Apple Silicon) 的实测经验总结。

## 核心结论

能否从微信进程提取加密密钥，取决于 **两个独立问题**：

| 问题 | 控制什么 | 关键因素 |
|------|---------|---------|
| `task_for_pid()` 能否成功 | 读取进程内存 | **目标 App 的代码签名** |
| `codesign` 能否重签名 | 修改 App 文件 | **调用者的完全磁盘访问** |

---

## 一、task_for_pid 权限（读取微信内存）

### 决定因素：微信 App 的 Hardened Runtime

```bash
# 检查微信签名状态
codesign -dv /Applications/WeChat.app 2>&1 | grep -E "Signature|flags"
```

#### 情况 A：Ad-hoc 签名（无 Hardened Runtime）

```
flags=0x2(adhoc)
Signature=adhoc
TeamIdentifier=not set
```

**原因**: 安装过防撤回补丁等第三方修改工具，App 被重新签名。

**权限要求**: 只需 `sudo`，任何上下文（Terminal、SSH、cron）都能成功。

```bash
# SSH 远程直接可用
sudo ./find_all_keys_macos
```

#### 情况 B：Apple 官方签名（有 Hardened Runtime）

```
flags=0x10000(runtime)
Signature size=9092
Authority=...Apple...
```

**原因**: App Store 下载或官方 DMG 安装，未经修改。

**权限要求**: `sudo` + 本机 GUI 终端 + TCC "开发者工具"授权。SSH **不可行**。

```
taskgated 检查流程:
  目标有 hardened runtime?
    YES → 检查调用者的"负责应用"是否有 TCC DeveloperTool 授权
          SSH 的负责应用是 sshd → 无法获得 TCC 授权 → 拒绝
          Terminal.app 可以弹窗获得授权 → 允许
    NO  → root (sudo) 即可 → 允许
```

### 实测数据

| 机器 | macOS | WeChat 签名 | 本机 Terminal sudo | SSH sudo |
|------|-------|------------|-------------------|---------|
| MacBook (macOS 15.x) | 15.x | **ad-hoc** (防撤回补丁) | ✅ | ✅ |
| Mac mini (Catalina) | 10.15.8 | Apple 官方 runtime | ✅ | ❌ |
| MacBook Pro (Big Sur) | 11.1 | Apple 官方 runtime | ✅ | ❌ |

### SSH 下穷举过的所有方法（Apple 签名时全部失败）

| 方法 | 结果 | 错误信息 |
|------|------|---------|
| `sudo frida -p <pid>` | ❌ | unable to access process |
| `lldb -p <pid>` | ❌ | non-interactive debug session |
| `sudo gcore <pid>` | ❌ | insufficient privilege |
| 带 debugger entitlement 的 C 程序 | ❌ | KERN_FAILURE=5 |
| `launchctl asuser` (用户会话) | ❌ | task_for_pid=5 |
| LaunchAgent (Aqua GUI 会话) | ❌ | 非 root，需要 sudo |
| LaunchDaemon (root) | ❌ | 系统域无 GUI 上下文 |
| `launchctl submit` (root) | ❌ | 同上 |
| `osascript` 操控 Terminal.app | ❌ | 需要辅助功能权限/挂起 |
| 修改 TCC.db 给 sshd 授权 | ❌ | SIP 保护，restricted 只读 |
| `vmmap` / `heap` | ⚠️ | 只能看元数据，无法读内存 |

---

## 二、codesign 权限（重签名微信 App）

如果微信是 Apple 官方签名，需要重签名为 ad-hoc 来解锁 SSH 提取。

### 问题：SSH 下 codesign 可能失败

```
$ sudo codesign --force --deep --sign - /Applications/WeChat.app
/Applications/WeChat.app: Operation not permitted
In subcomponent: /Applications/WeChat.app/Contents/MacOS/WeChatAppEx.app
```

**原因**: SSH 进程没有「完全磁盘访问」(Full Disk Access, FDA) 权限，无法修改 `/Applications` 下的 App bundle 文件。

### 给 SSH 授予完全磁盘访问

在目标机器的 **GUI** 上操作：

```
系统偏好设置 → 安全性与隐私 → 隐私 → 完全磁盘访问
点击 🔒 解锁 → 点 + 号 → Cmd+Shift+G 输入路径
```

**必须添加这两个**（缺一不可）：

| 路径 | 说明 |
|------|------|
| `/usr/sbin/sshd` | SSH 守护进程 |
| `/usr/libexec/sshd-keygen-wrapper` | SSH 的实际执行进程（负责应用） |

> ⚠️ 添加后必须**断开 SSH 重新连接**！TCC 权限在进程启动时检查，不会热更新。

### 验证 FDA 是否生效

```bash
# 重连 SSH 后执行
cat ~/Library/Application\ Support/com.apple.TCC/TCC.db > /dev/null 2>&1 && echo "FDA: YES" || echo "FDA: NO"
```

TCC.db 是受保护文件，只有 FDA 进程能读取。

### 完整流程：SSH 远程重签名微信

```bash
# 0. 前提：SSH 已有 FDA（上面的步骤）

# 1. 确认微信已退出
kill $(pgrep -x WeChat) 2>/dev/null
sleep 2
pgrep -x WeChat && echo "还在运行！" || echo "已退出"

# 2. 清除扩展属性（可选，防止干扰）
sudo xattr -cr /Applications/WeChat.app

# 3. Ad-hoc 重签名
sudo codesign --force --deep --sign - /Applications/WeChat.app

# 4. 验证签名
codesign -dv /Applications/WeChat.app 2>&1 | grep -E "Signature|flags"
# 期望: flags=0x2(adhoc), Signature=adhoc

# 5. 用户需在 GUI 上重新打开微信并登录
# （或者 SSH 执行 open，但用户仍需在 GUI 上完成登录）
open /Applications/WeChat.app
```

### 注意事项

| 事项 | 说明 |
|------|------|
| 微信必须先退出 | 运行中的 App，其 dylib/binary 被占用，codesign 会报 `internal error` |
| **重签名后必须重启微信** | 已运行的进程仍使用旧签名的内存映像，task_for_pid 仍会失败。必须 kill 后重新启动 |
| 重签名后需重新登录微信 | 签名变更会使登录态失效 |
| 自动更新可能覆盖签名 | 微信更新后变回 Apple 签名，需要再次重签 |
| 小程序可能受影响 | 部分小程序校验签名，ad-hoc 可能报安全错误 |

---

## 三、权限矩阵总结

| 操作 | 需要的权限 | SSH 需要额外配置 |
|------|-----------|-----------------|
| 读取微信数据库文件 | 文件系统权限（通常有） | 无 |
| `task_for_pid` (ad-hoc App) | sudo | 无 |
| `task_for_pid` (Apple 签名 App) | sudo + TCC DeveloperTool | **不可行**，必须本机 Terminal |
| `codesign` 重签名 App | sudo + FDA | SSH 需添加 sshd + sshd-keygen-wrapper 到 FDA |
| 修改 TCC.db | sudo + 关闭 SIP | **不推荐** |

### 完全远程操作清单（一次性 GUI 配置）

只需在目标机器 GUI 上做一次，之后 SSH 永久可用：

1. **完全磁盘访问** → 添加 `/usr/sbin/sshd` 和 `/usr/libexec/sshd-keygen-wrapper`
2. SSH 连入 → `sudo codesign --force --deep --sign - /Applications/WeChat.app`
3. 用户在 GUI 重开微信并登录
4. 之后 SSH 永久可以 `sudo` 提取密钥，微信重启也不影响（除非更新覆盖签名）

---

## 四、常见误区

| 误区 | 真相 |
|------|------|
| "需要给终端完全磁盘访问才能调试" | ❌ FDA 控制文件访问，不控制进程调试 |
| "需要给终端开发者工具权限" | ⚠️ 仅当目标 App 有 hardened runtime 时才需要 |
| "SSH 下永远无法提取密钥" | ❌ 目标 App 是 ad-hoc 签名时，SSH sudo 可以 |
| "macOS 版本决定了能否 SSH 调试" | ❌ 主要取决于目标 App 的签名状态 |
| "SIP 阻止了调试微信" | ❌ SIP 只保护系统进程，微信不受 SIP 保护 |
| "加了 sshd 到 FDA 就行" | ❌ 还需要加 `sshd-keygen-wrapper`，且要重连 SSH |
| "微信开着也能重签名" | ❌ 运行中的 binary/dylib 被占用，codesign 会失败 |

---

## 五、重签名后微信权限 silent 失效

### 现象

完成 ad-hoc 重签名后，微信任意以下功能都可能"看起来已授权但实际被拒绝"：

- 截图 / 屏幕共享（`ScreenCapture`）
- 视频通话 / 扫码（`Camera`）
- 语音消息 / 通话（`Microphone`）
- 自动化、第三方输入法（`AppleEvents`）
- 同步通讯录（`AddressBook`）
- 文件发送 / 接收（`SystemPolicyDocumentsFolder` / `Downloads` / `Desktop`）

System Settings 里通常仍看到"微信.app"开关是 ON，但运行时权限校验失败。微信会反复弹"需要开启 X 权限"。

### 根因（第一性原理）

macOS TCC（Transparency, Consent, and Control）按 **bundle id + csreq** 联合校验权限。`csreq`（code requirement）是从 app 的 code signature 推导出的二进制 blob，存在 `/Library/Application Support/com.apple.TCC/TCC.db` 的 `access` 表里，每条 ~160 字节。

`codesign --force --deep --sign -` 把 WeChat 从官方签名换成 ad-hoc 签名（甚至 ad-hoc → ad-hoc 重签也会变），新进程的 csreq 跟旧记录里那条对不上 —— tccd 拒绝。

System Settings UI 只按 client 显示开关、不重算 csreq，所以视觉上是"已授权"，运行时实际拒绝。这是 silent drift。

### 修复步骤

把 WeChat 在 TCC 里的旧记录全部抹掉，让 macOS 在下次微信请求权限时按新签名重新生成 csreq：

```bash
for s in ScreenCapture Camera Microphone AppleEvents AddressBook \
         SystemPolicyDocumentsFolder SystemPolicyDownloadsFolder SystemPolicyDesktopFolder; do
  tccutil reset "$s" com.tencent.xinWeChat
done
```

`tccutil` 对没有授权过的 service 会报 "No such bundle identifier"，这是 no-op，不影响其他 service 的 reset。

之后退出并重新打开微信，按 GUI 提示重新允许：

```bash
killall WeChat
open /Applications/WeChat.app
```

> 这一步**应当由用户/agent 手动执行**，不在 `wx init` 里自动跑——TCC 重置会让用户的现有授权失效，需要由人决定时机。

#### macOS 26 的 UI 拆分

在 macOS 26 上，**隐私与安全 → 录屏与系统录音** 显示为两块，容易踩坑：

| 区域 | 作用 |
|------|------|
| **录屏与系统录音**（上半区） | 录制屏幕内容 + 系统音频；微信截图、屏幕共享需要这一项 |
| **仅系统录音**（下半区） | 只录系统音频；只打开这一项**不能**修复微信截图 |

把 WeChat 加进上半区；只勾下半区的"仅系统录音"无效。

### 验证

确认 WeChat 当前是 ad-hoc 签名（这是修复前提）：

```bash
codesign -dv --verbose=4 /Applications/WeChat.app 2>&1 | grep -E "Signature|flags|TeamIdentifier"
```

期望看到：

```text
flags=0x2(adhoc)
Signature=adhoc
TeamIdentifier=not set
```

最直接的功能验证：在微信里使用截图、视频通话、麦克风等功能，按 GUI 弹窗的"允许"重新授权一次，之后正常工作。
