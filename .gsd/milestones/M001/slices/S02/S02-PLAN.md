# S02: TCP server support

**Goal:** Enable TCP transport end-to-end: `wx daemon start --tcp 127.0.0.1:9876` starts daemon listening on TCP, and all query commands support `--tcp 127.0.0.1:9876` to connect via TCP instead of local transport. TCP bind/connect failures produce clear errors with no silent fallback (15s connect timeout, 120s read/write timeout).
**Demo:** `wx daemon start --tcp 127.0.0.1:9876` starts daemon listening on TCP port 9876

## Must-Haves

- 1. `wx daemon start --tcp 127.0.0.1:9876` starts daemon and logs TCP listening message. 2. All query commands (`sessions`, `history`, `search`, `contacts`, etc.) accept `--tcp host:port` flag. 3. When --tcp is specified, requests route through TCP to the daemon, not local transport. 4. TCP bind failure gives clear error (e.g. port in use). 5. TCP connect failure gives clear error (no silent fallback). 6. `cargo check` passes on all platforms.

## Integration Closure

TCP server already wired in S01 (server.rs serve_tcp). This slice wires TCP into the client transport path (cli/transport.rs send/send_unix/send_windows) and the CLI struct. S03 will add client-side TCP in a future slice.

## Verification

- daemon logs show TCP bind address; is_alive() and status report TCP connectivity; TCP error messages include address and errno

## Tasks

- [ ] **T01: Add global --tcp CLI flag and wire into transport module** `est:2h`
  Add `--tcp` flag as a global argument on the root `Cli` struct in `src/cli/mod.rs`, not on individual subcommands. The flag takes `Option<String>` (e.g., `Some("127.0.0.1:9876")`). Wire this through the `dispatch()` function so every command path receives the TCP address. Modify all `cmd_*` functions in `src/cli/` to accept an optional `tcp_addr: Option<&str>` parameter. Update `src/cli/transport.rs`:
  1. Add `send_tcp(req: Request, addr: &str) -> Result<Response>` function using `std::net::TcpStream` with 15s connect timeout and 120s read/write timeout
  2. Add `is_alive_tcp(addr: &str) -> bool` for TCP liveness check
  3. Update `send()` to accept `tcp_addr: Option<&str>`, routing to `send_tcp` when present
  4. Update `is_alive()` to accept `tcp_addr: Option<&str>`, routing to `is_alive_tcp` when present
  5. Update `ensure_daemon()` — when --tcp is specified, do NOT auto-start daemon (user explicitly chose TCP); if connection fails, hard error with clear message
  - Files: `src/cli/mod.rs`, `src/cli/transport.rs`, `src/cli/sessions.rs`, `src/cli/history.rs`, `src/cli/search.rs`, `src/cli/contacts.rs`, `src/cli/export.rs`, `src/cli/unread.rs`, `src/cli/members.rs`, `src/cli/new_messages.rs`, `src/cli/stats.rs`, `src/cli/favorites.rs`, `src/cli/sns_notifications.rs`, `src/cli/sns_feed.rs`, `src/cli/sns_search.rs`, `src/cli/daemon_cmd.rs`
  - Verify: cargo check 2>&1 | tail -5; grep -c 'tcp: Option<String>' src/cli/mod.rs; grep -q 'send_tcp' src/cli/transport.rs; grep -q 'is_alive_tcp' src/cli/transport.rs

- [ ] **T02: Wire --tcp into daemon status/stop/logs commands and verify end-to-end** `est:1h`
  Update `src/cli/daemon_cmd.rs` to:
  1. `DaemonCommands::Status` — when --tcp addr is set, check TCP liveness via `is_alive_tcp`; report "listening on TCP {addr}" vs "listening on local socket"
  2. `DaemonCommands::Stop` — when --tcp is set, warn that TCP daemon must be stopped manually (it's a separate process)
  3. `DaemonCommands::Logs` — unchanged, logs go to same file
  4. Update the `cmd_daemon` function signature to accept tcp_addr
  - Files: `src/cli/daemon_cmd.rs`, `src/cli/transport.rs`
  - Verify: cargo check 2>&1 | tail -5 && cargo test transport -- --nocapture 2>&1 | tail -10

- [ ] **T03: Cross-platform compilation verification** `est:30m`
  Verify that all changes compile on all target platforms:
  1. `cargo check` (native/macOS)
  2. `cargo check --target x86_64-pc-windows-msvc` (Windows cross-compile)
  3. `cargo test` to ensure unit tests pass
  - Files: `src/cli/mod.rs`, `src/cli/transport.rs`, `src/cli/daemon_cmd.rs`
  - Verify: cargo check 2>&1 | tail -5 && cargo check --target x86_64-pc-windows-msvc 2>&1 | tail -5 && cargo test 2>&1 | tail -10

## Files Likely Touched

- src/cli/mod.rs
- src/cli/transport.rs
- src/cli/sessions.rs
- src/cli/history.rs
- src/cli/search.rs
- src/cli/contacts.rs
- src/cli/export.rs
- src/cli/unread.rs
- src/cli/members.rs
- src/cli/new_messages.rs
- src/cli/stats.rs
- src/cli/favorites.rs
- src/cli/sns_notifications.rs
- src/cli/sns_feed.rs
- src/cli/sns_search.rs
- src/cli/daemon_cmd.rs
