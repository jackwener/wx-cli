# S03: TCP client + global --tcp flag

**Goal:** Prove the TCP client works end-to-end: add integration tests exercising send_tcp() with a real mock TCP server, and verify cross-platform compilation. The code implementation was completed in S02; this slice adds test coverage and integration verification.
**Demo:** `wx sessions --tcp 127.0.0.1:9876` connects via TCP and returns session data

## Must-Haves

- cargo test passes with all existing + new TCP integration tests
- cargo check passes on native and x86_64-pc-windows-msvc targets
- New tests cover: successful TCP request/response round-trip, is_alive_tcp false for unreachable port, connection refused error path

## Integration Closure

Consumes S01 transport::handle_connection (shared server handler) and S02 cli::transport::send_tcp() (blocking TCP client). New wiring: test module in transport.rs that spawns a tokio mock TCP server and exercises the client code path. What remains: S04 side-by-side comparison of TCP vs local transport results with real daemon.

## Verification

- None — tests run locally, no new runtime observability surfaces added

## Tasks

- [ ] **T01: Add TCP client integration tests with mock server** `est:45m`
  Add integration tests to src/cli/transport.rs that exercise send_tcp() and is_alive_tcp() against a real mock TCP server.
  - Files: `src/cli/transport.rs`
  - Verify: cargo test integration_tests -- --test-threads=1

- [ ] **T02: Verify cross-platform compilation and full test suite** `est:15m`
  Verify that all code compiles and tests pass across platforms.
  - Verify: cargo check && cargo test && cargo check --target x86_64-pc-windows-msvc

## Files Likely Touched

- src/cli/transport.rs
