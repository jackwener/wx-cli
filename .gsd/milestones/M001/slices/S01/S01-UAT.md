# S01: Transport traits + TCP + Unix + Windows named pipe + daemon start subcommand — UAT

**Milestone:** M001
**Written:** 2026-05-13T05:59:31.990Z

## UAT Steps for S01

1. **Compilation**: `cargo check` passes (exit 0), `cargo check --target x86_64-pc-windows-msvc` passes (exit 0)
2. **Clippy**: `cargo clippy` passes with 18 pre-existing warnings (non-blocking)
3. **Daemon start with TCP**: Run `wx daemon start --tcp 127.0.0.1:9876`, check log file for `[server] 监听 TCP 127.0.0.1:9876` and `[server] 监听 {sock_path}`
4. **Daemon status**: Run `wx daemon status`, should show "wx-daemon 运行中 (PID XXX)"
5. **Daemon logs**: Run `wx daemon logs -n 20`, should show startup messages including which transports are active
6. **Daemon stop**: Run `wx daemon stop`, should show "已停止 wx-daemon (PID XXX)"
