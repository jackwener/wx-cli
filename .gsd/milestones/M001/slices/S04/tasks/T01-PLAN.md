---
estimated_steps: 14
estimated_files: 1
skills_used: []
---

# T01: Real TCP daemon-client integration test

Write an integration test in `src/cli/transport.rs` under a `#[cfg(test)]` mod that spawns the actual `wx` daemon binary, connects via TCP, and verifies a full request/response round-trip.

Steps:
1. Add `#[cfg(test)] mod tcp_integration_tests;` section in `src/cli/transport.rs` (inline in existing test area)
2. Test `test_tcp_daemon_ping_round_trip`:
   - Run `cargo build --bin wx` to ensure binary exists at `target/debug/wx`
   - Pick a free port: use `std::net::TcpListener::bind("127.0.0.1:0")` to get an OS-assigned ephemeral port, then drop it
   - Spawn daemon subprocess: `WX_DAEMON_MODE=1 WX_DAEMON_TCP_ADDR=127.0.0.1:<port>` environment set on the spawned command
   - Wait for readiness: poll `is_alive_tcp(addr)` in a loop (max 15s, 300ms intervals)
   - Send `send_tcp(Request::Ping, &addr)` and assert response contains `pong == true`
   - Kill daemon subprocess (SIGTERM on Unix)
   - Verify process exited (exit code 0)
3. Test `test_tcp_daemon_connection_refused`: verify `send_tcp` returns `Err` when no daemon listening
4. Each test uses unique port to avoid conflicts
5. Tests are `#[cfg(unix)]` only

## Inputs

- ``src/cli/transport.rs``
- ``src/daemon/mod.rs``
- ``src/daemon/server.rs``
- ``src/ipc.rs``
- ``src/main.rs``

## Expected Output

- ``src/cli/transport.rs``

## Verification

cargo test tcp_integration_tests 2>&1 | grep -E '(test.*ok|test.*FAILED|running [0-9]+ test)'

## Observability Impact

Tests exercise the real daemon binary's TCP listen path and client's TCP connect path — any regression will be caught by test failure.
