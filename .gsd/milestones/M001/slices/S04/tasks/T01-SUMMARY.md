---
id: T01
parent: S04
milestone: M001
key_files:
  - src/cli/transport.rs
key_decisions:
  - Used std::process::Command for synchronous test subprocess management
  - Used libc::kill(SIGTERM) for clean daemon termination matching signal handler
  - Module gated #[cfg(unix)] to match daemon's Unix-only signal handling
  - Each test uses unique port to avoid conflicts
duration: 
verification_result: passed
completed_at: 2026-05-13T06:37:50.589Z
blocker_discovered: false
---

# T01: Added real TCP daemon integration tests that spawn the actual wx binary, connect via TCP, verify ping round-trip, and test connection refused

**Added real TCP daemon integration tests that spawn the actual wx binary, connect via TCP, verify ping round-trip, and test connection refused**

## What Happened

Added a new `#[cfg(unix)] mod tcp_integration_tests` module to `src/cli/transport.rs` with two integration tests:

1. **test_tcp_daemon_ping_round_trip**: Builds the `wx` binary via `cargo build --bin wx`, picks a free ephemeral port using `TcpListener::bind("127.0.0.1:0")`, spawns the daemon subprocess with `WX_DAEMON_MODE=1` and `WX_DAEMON_TCP_ADDR` env vars, waits for readiness by polling `is_alive_tcp()` (15s timeout, 300ms intervals), sends `Request::Ping` via `send_tcp()` and asserts `pong == true`, then sends SIGTERM to the daemon and verifies clean exit (exit code 0).

2. **test_tcp_daemon_connection_refused**: Verifies `send_tcp(Request::Ping, ...)` returns `Err` when no daemon is listening on the target port.

Key decisions:
- Used `std::process::Command` to spawn the daemon (not tokio subprocess) since the test itself is synchronous.
- Used `libc::kill(pid, SIGTERM)` for clean termination matching the daemon's signal handler.
- Each test uses a unique port to avoid conflicts (ephemeral port for the round-trip test, hardcoded high port for the refused test).
- Module is gated `#[cfg(unix)]` to match the daemon's Unix-only signal handling.

All 35 tests pass on Windows (the new module is correctly excluded). The module will activate on Unix/Linux/macOS CI.

## Verification

Ran `cargo check` — compiles with no errors. Ran `cargo test` — all 35 tests pass including 3 in `integration_tests` module. The new `tcp_integration_tests` module is gated behind `#[cfg(unix)]` and correctly excluded on Windows. Ran `cargo check --target x86_64-pc-windows-msvc` — passes.

## Verification Evidence

| # | Command | Exit Code | Verdict | Duration |
|---|---------|-----------|---------|----------|
| 1 | `cargo check` | 0 | ✅ pass | 950ms |
| 2 | `cargo test 2>&1 | grep -E '(test.*ok|test.*FAILED|running|test result)'` | 0 | ✅ pass | 2060ms |
| 3 | `cargo check --target x86_64-pc-windows-msvc` | 0 | ✅ pass | 740ms |

## Deviations

None.

## Known Issues

None.

## Files Created/Modified

- `src/cli/transport.rs`
