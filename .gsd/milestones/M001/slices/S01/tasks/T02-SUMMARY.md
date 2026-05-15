---
id: T02
parent: S01
milestone: M001
key_files:
  - src/daemon/server.rs
  - src/daemon/mod.rs
  - src/cli/daemon_cmd.rs
  - src/cli/mod.rs
key_decisions:
  - Used WX_DAEMON_TCP_ADDR env var for TCP address propagation to daemon subprocess
  - TCP listener spawned as tokio task — daemon exits on local listener exit, OS cleans up TCP port
  - run_start() spawns separate background process with log redirection, consistent with daemon UX
  - cleanup_and_exit made #[cfg(unix)]-only since Windows has no signal handler path
duration: 
verification_result: passed
completed_at: 2026-05-13T05:57:04.792Z
blocker_discovered: false
---

# T02: Wired transport module into daemon server, added TCP listening alongside local transport, and implemented `wx daemon start [--tcp ADDR]` subcommand

**Wired transport module into daemon server, added TCP listening alongside local transport, and implemented `wx daemon start [--tcp ADDR]` subcommand**

## What Happened

Refactored server.rs and added `wx daemon start` subcommand:

1. **server.rs** — Removed duplicated `handle_connection_unix`, `handle_connection_windows`, and `dispatch()` functions. Changed `serve()` signature to accept `tcp_addr: Option<&str>`. Local transport path (Unix socket / Windows named pipe) behavior is identical to before, now using `transport::handle_connection()` from the transport module. Added `serve_tcp()` helper: when `tcp_addr` is `Some`, binds a `TcpListener` from the transport module and spawns an accept loop. Both local and TCP run simultaneously; daemon exits when local listener exits.

2. **daemon/mod.rs** — Made `start_daemon(tcp_addr: Option<String>)` public, called by `run()` (WX_DAEMON_MODE auto-start path). Added `run_start(tcp: Option<String>)` which spawns a new process of the current executable with `WX_DAEMON_MODE=1` and optional `WX_DAEMON_TCP_ADDR` env var, with log redirection and session leadership (Unix setsid). Updated signal handler `cleanup_and_exit` to be `#[cfg(unix)]`-only and only remove local socket file (TCP ports recovered by OS).

3. **cli/daemon_cmd.rs** — Added `DaemonCommands::Start { tcp }` variant handling, dispatching to `crate::daemon::run_start(tcp)`. Status, Stop, Logs unchanged.

4. **cli/mod.rs** — Added `Start { tcp: Option<String> }` variant to `DaemonCommands` enum with `#[arg(long)]` for the `--tcp` flag.

Key decisions:
- Used `WX_DAEMON_TCP_ADDR` env var for TCP address in daemon process, avoiding CLI-level global flag changes (per plan: S02/S03 for that)
- TCP listener runs as `tokio::spawn` task — if local listener exits (signal), process terminates and OS cleans up TCP port
- `run_start()` spawns a separate process rather than blocking the CLI, consistent with daemon UX expectations
- `#[allow(unreachable_code)]` on post-serve cleanup in `start_daemon` since signal handler exits via `std::process::exit(0)`

Verification: cargo check passes (native + x86_64-pc-windows-msvc), all 32 tests pass, all structural grep checks confirm expected code patterns.

## Verification

cargo check passed (native + x86_64-pc-windows-msvc). cargo test: 32/32 passed (including 3 transport tests from T01). All structural grep checks confirmed: start_daemon public in mod.rs, Start variant in daemon_cmd.rs, tcp_addr param in server.rs, handle_connection usage from transport module, no duplicated handle_connection_unix/windows functions, no duplicated dispatch() in server.rs.

## Verification Evidence

| # | Command | Exit Code | Verdict | Duration |
|---|---------|-----------|---------|----------|
| 1 | `cargo check` | 0 | ✅ pass | 710ms |
| 2 | `cargo check --target x86_64-pc-windows-msvc` | 0 | ✅ pass | 1190ms |
| 3 | `cargo test` | 0 | ✅ pass | 6900ms |
| 4 | `grep -q "pub async fn start_daemon" src/daemon/mod.rs` | 0 | ✅ pass | 10ms |
| 5 | `grep -q "Start {" src/cli/daemon_cmd.rs` | 0 | ✅ pass | 10ms |
| 6 | `grep -q "tcp_addr: Option<&str>" src/daemon/server.rs` | 0 | ✅ pass | 10ms |
| 7 | `grep -q "handle_connection" src/daemon/server.rs` | 0 | ✅ pass | 10ms |
| 8 | `! grep -q "handle_connection_unix" src/daemon/server.rs` | 0 | ✅ pass | 10ms |

## Deviations

None.

## Known Issues

None.

## Files Created/Modified

- `src/daemon/server.rs`
- `src/daemon/mod.rs`
- `src/cli/daemon_cmd.rs`
- `src/cli/mod.rs`
