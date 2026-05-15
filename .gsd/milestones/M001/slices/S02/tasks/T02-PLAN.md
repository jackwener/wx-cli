---
estimated_steps: 9
estimated_files: 2
skills_used: []
---

# T02: Wire --tcp into daemon status/stop/logs commands and verify end-to-end

Update `src/cli/daemon_cmd.rs` to:
1. `DaemonCommands::Status` — when --tcp addr is set, check TCP liveness via `is_alive_tcp`; report "listening on TCP {addr}" vs "listening on local socket"
2. `DaemonCommands::Stop` — when --tcp is set, warn that TCP daemon must be stopped manually (it's a separate process)
3. `DaemonCommands::Logs` — unchanged, logs go to same file
4. Update the `cmd_daemon` function signature to accept tcp_addr

Then verify:
1. `cargo check` passes
2. Unit tests in transport module pass: `TcpConnector` implements `Connector`, `TcpListener` implements `Listener`
3. Existing `transport_addr_variants` test still passes

## Inputs

- `src/cli/daemon_cmd.rs`
- `src/cli/transport.rs`
- `src/cli/mod.rs`

## Expected Output

- `src/cli/daemon_cmd.rs`

## Verification

cargo check 2>&1 | tail -5 && cargo test transport -- --nocapture 2>&1 | tail -10
