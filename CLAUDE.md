# wx-cli Project Rules

## After Every Code Change

**Rust 代码改动后，必须立刻运行：**

```bash
cargo check
```

不允许在 `cargo check` 通过之前提交或推送。

**改动涉及跨平台代码（`#[cfg(...)]` / `Cargo.toml` dependencies）时，额外运行：**

```bash
cargo check --target x86_64-unknown-linux-gnu
cargo check --target x86_64-pc-windows-gnu   # 在 macOS 上用这个，msvc 需要 MSVC 工具链
```

macOS 上需要一次性安装 target 和交叉编译器：

```bash
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64   # 提供 x86_64-w64-mingw32-gcc，zstd-sys 等 C 依赖需要
```

这两条 check 命令用于提前暴露 Linux/Windows 特有的编译错误，**只做类型检查**（不 link）。

## IPC / 跨平台同库约定

动任何 IPC / 网络代码时：**两端必须用同一个库、同一套 API**。例如 server 用 `interprocess::local_socket::tokio::Listener`，client 就必须用 `interprocess::local_socket::Stream::connect`，不能用 `std::fs::OpenOptions` 打开同名路径——即使 kernel 名字对上了，底层的 framing / overlapped 模式也不兼容。

## 消息解析坑

- WeChat 4.x 的 `local_type` 经常把 subtype 编进高 32 位：例如 `5<<32 | 49`、`19<<32 | 49`、`57<<32 | 49`
- CLI 里的 `--type link|file` 语义是按 base type 过滤，所以 SQL 必须比较低 32 位，不能直接写 `local_type = 49`
- 合并聊天记录 / 文件 / 引用消息的 XML 实测通常就在 `message_content`，并由 `WCDB_CT_message_content = 4` 表示 zstd 压缩；不要先假设这类内容只会出现在 `compress_content`
- `type=19` 的合并聊天记录里，`recorditem` 常常是 CDATA 包的一层内嵌 XML，需要先解 CDATA 再解析 `recordinfo/datalist/dataitem`

## 本地验证坑

- CLI 实际通过 Unix socket 复用后台 `wx-daemon`；如果你刚切到新编译的二进制做验证，但旧 daemon 还活着，看到的仍然会是旧进程算出来的结果
- 验证“刚发的新消息”时，先 `wx daemon stop`，再重跑目标命令
- 本地真实数据探针放在 `src/daemon/query.rs` 的 `#[ignore]` 测试里，需要时显式运行

## Cargo.toml 修改规则

- 修改版本号后，必须运行 `cargo update --workspace` 更新 Cargo.lock
- 添加/移动 `[target.'cfg(...)'.dependencies]` section 时，确认后续依赖没有被意外归入该 section（TOML section 持续到下一个 header）
- 改完后运行 `cargo check` 验证

## Git 规则

- 每次 commit 后必须 push（`git push wx-cli main`）
- 打 tag 前确认 `cargo check` 和 `cargo update --workspace` 都已完成
- remote 使用 `wx-cli`（SSH），不用 `origin`

## 平台兼容性检查清单

改动以下内容时必须做跨平台 check：

- [ ] `libc::` 调用 → 确认函数在 Linux 和 macOS 都存在（`__error` 是 macOS 专属，用 `std::io::Error::last_os_error()` 代替）
- [ ] `#[cfg(unix)]` 块 → unix 包括 macOS 和 Linux，不能用 macOS 专属 API
- [ ] `Cargo.toml` dependency section 顺序 → 检查是否有 dep 意外落入 target section
- [ ] Windows named pipe 代码 → 确认函数都已定义，trait import 齐全

## CI 结构

```
check job（ubuntu）
  └── cargo check --target linux-x86, linux-arm64, windows-x86
        ↓ 通过后
build jobs（5平台并行）
        ↓ 全部通过后
publish-npm job
```
