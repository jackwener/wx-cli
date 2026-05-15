# S01: Transport abstraction layer

**Goal:** Define transport traits (Listener/Connector), implement TCP + Unix + Windows named pipe, add `wx daemon start` subcommand. Refactor server.rs to use shared connection handler. cargo check passes on all platforms.
**Demo:** Refactor complete, `cargo check` passes on all platforms, existing behavior unchanged. Transport traits defined and implemented for Unix socket + Windows named pipe.

## Must-Haves

- `src/transport/mod.rs` exists with `TransportAddr`, `Listener`, `Connector` traits and `handle_connection` generic function
- `TcpListener` and `TcpConnector` implemented
- `server.rs` refactored: `handle_connection` extracted, accepts `Option<&str>` tcp_addr, listens on local + TCP simultaneously
- `src/cli/daemon_cmd.rs` has `DaemonCommands::Start { tcp: Option<String> }`
- `cargo check` passes on macOS (current), x86_64-unknown-linux-gnu, and x86_64-pc-windows-msvc
- Existing local transport behavior unchanged (no `--tcp` still works)

## Proof Level

- This slice proves: contract

## Integration Closure

- Upstream surfaces consumed: `src/ipc.rs` (Request/Response), `src/daemon/cache.rs` (DbCache), `src/daemon/query.rs` (Names), `src/config.rs` (paths)
- New wiring: `src/transport/` module with Listener/Connector traits; `server::serve` accepts optional tcp_addr; `daemon start` subcommand added
- What remains: S02 adds global `--tcp` CLI flag and client-side TCP connector; S03 wires CLI commands to use TCP; S04 does end-to-end smoke test

## Verification

- Daemon logs show which transports are active: `[server] 监听 {path}` for local, `[server] 监听 TCP {addr}` for TCP. Bind errors abort daemon startup with clear message.

## Tasks

- [ ] **T01: Create transport module with traits, generic handler, and TCP implementation** `est:2h`
  **Why**: Establish the transport abstraction layer — the core deliverable of S01. Define traits that abstract over Unix socket, Windows named pipe, and TCP. Extract the duplicated JSON-line protocol handling from server.rs into a generic `handle_connection` function.
  - Files: `src/transport/mod.rs`, `src/main.rs`
  - Verify: cargo check && test -f src/transport/mod.rs && grep -q "pub trait Listener" src/transport/mod.rs && grep -q "pub trait Connector" src/transport/mod.rs && grep -q "pub async fn handle_connection" src/transport/mod.rs && grep -q "pub struct TcpListener" src/transport/mod.rs && grep -q "pub struct TcpConnector" src/transport/mod.rs

- [ ] **T02: Refactor server.rs and add `wx daemon start` subcommand** `est:2h`
  **Why**: Wire the transport module into the daemon server, enable TCP listening alongside local transport, and add the `daemon start` subcommand (R005). This closes the server-side of the transport abstraction.
  - Files: `src/daemon/server.rs`, `src/daemon/mod.rs`, `src/cli/daemon_cmd.rs`, `src/cli/mod.rs`
  - Verify: cargo check && grep -q "pub async fn start_daemon" src/daemon/mod.rs && grep -q "Start {" src/cli/daemon_cmd.rs && grep -q "tcp_addr: Option<&str>" src/daemon/server.rs && grep -q "handle_connection" src/daemon/server.rs && ! grep -q "handle_connection_unix" src/daemon/server.rs

- [ ] **T03: Cross-platform compilation verification on all three targets** `est:1h`
  **Why**: R006 requires code compiles on macOS, Linux, and Windows. This is the final proof that the transport abstraction works across all platforms.
  - Files: `src/transport/mod.rs`, `src/daemon/server.rs`, `src/daemon/mod.rs`, `Cargo.toml`
  - Verify: cargo check && cargo check --target x86_64-unknown-linux-gnu && cargo check --target x86_64-pc-windows-msvc

## Files Likely Touched

- src/transport/mod.rs
- src/main.rs
- src/daemon/server.rs
- src/daemon/mod.rs
- src/cli/daemon_cmd.rs
- src/cli/mod.rs
- Cargo.toml
