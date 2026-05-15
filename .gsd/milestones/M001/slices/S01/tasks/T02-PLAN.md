---
estimated_steps: 26
estimated_files: 4
skills_used: []
---

# T02: Refactor server.rs and add `wx daemon start` subcommand

**Why**: Wire the transport module into the daemon server, enable TCP listening alongside local transport, and add the `daemon start` subcommand (R005). This closes the server-side of the transport abstraction.

**Steps**:
1. Refactor `src/daemon/server.rs`:
   a. Remove `handle_connection_unix` and `handle_connection_windows` (duplicated — now use `transport::handle_connection`)
   b. Change `serve()` signature to `async fn serve(db: Arc<DbCache>, names: Arc<...>, tcp_addr: Option<&str>) -> Result<()>`
   c. Local transport path (unchanged behavior): bind Unix socket or named pipe as before, accept loop calling `transport::handle_connection(stream, db, names).await`
   d. If `tcp_addr` is `Some(addr)`: parse to `SocketAddr`, bind `tokio::net::TcpListener`, spawn accept loop as `tokio::spawn` that calls `transport::handle_connection`
   e. Local + TCP run simultaneously; daemon exits when local listener exits
2. Refactor `src/daemon/mod.rs`:
   a. Add `async fn start_daemon(tcp_addr: Option<String>) -> Result<()>`
   b. Extract shared daemon init logic (PID, signal handler, config, keys, DbCache, names) into a helper
   c. `run()` (existing WX_DAEMON_MODE path) calls `start_daemon(None)`
   d. Add `fn run_start(tcp_addr: Option<String>)` for the `daemon start` subcommand
3. Refactor `src/cli/daemon_cmd.rs`:
   a. Add `DaemonCommands::Start { tcp: Option<String> }` variant
   b. Handle `Start` by calling `daemon::run_start(tcp)`
   c. Keep `Status`, `Stop`, `Logs` unchanged
4. Refactor `src/cli/mod.rs`:
   a. Add `tcp: Option<String>` field to `DaemonCommands::Start` via clap `#[arg(long)]`
5. Update `src/daemon/mod.rs` signal handler: cleanup should only remove local socket file, not TCP

**Constraints**:
- When `tcp_addr` is `None`, behavior is IDENTICAL to current (local only)
- When `tcp_addr` is `Some`, daemon listens on BOTH local and TCP
- `run()` (WX_DAEMON_MODE) must continue to work for auto-start — calls `start_daemon(None)`
- Error on TCP bind: daemon prints clear error and exits (no silent fallback)
- Do NOT add global `--tcp` flag to Cli struct yet — that's S02/S03

## Inputs

- `src/daemon/server.rs`
- `src/daemon/mod.rs`
- `src/cli/daemon_cmd.rs`
- `src/cli/mod.rs`
- `src/transport/mod.rs`

## Expected Output

- `src/daemon/server.rs`
- `src/daemon/mod.rs`
- `src/cli/daemon_cmd.rs`

## Verification

cargo check && grep -q "pub async fn start_daemon" src/daemon/mod.rs && grep -q "Start {" src/cli/daemon_cmd.rs && grep -q "tcp_addr: Option<&str>" src/daemon/server.rs && grep -q "handle_connection" src/daemon/server.rs && ! grep -q "handle_connection_unix" src/daemon/server.rs
