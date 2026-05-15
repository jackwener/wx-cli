# S04: Integration smoke test

**Goal:** Real end-to-end TCP integration test: spawn the actual wx daemon binary with --tcp, connect client via TCP, and verify the round-trip works. Also verify TCP responses match local socket responses for the same query.
**Demo:** Daemon on TCP + client queries return same data as local transport

## Must-Haves

- T01: TCP integration test passes (daemon starts, client connects via TCP, ping round-trip succeeds, daemon killed cleanly)
- T02: TCP vs local comparison test either passes (data matches) or is skipped with clear message (no WeChat data)
- `cargo test` passes all 35+ tests including new integration tests
- `cargo check` passes
- Cross-platform compilation (`cargo check --target x86_64-pc-windows-msvc`) passes

## Proof Level

- This slice proves: integration

## Integration Closure

Upstream surfaces consumed: daemon/mod.rs (WX_DAEMON_TCP_ADDR env var), server.rs (serve with TCP), cli/transport.rs (send_tcp). New wiring: integration test module that spawns real daemon binary + connects via real TCP client. What remains: nothing — this is the final slice of M001.

## Verification

- No new observability surfaces added; tests exercise existing eprintln! daemon log output and TCP response paths.

## Tasks

- [ ] **T01: Real TCP daemon-client integration test** `est:1h`
  Write an integration test in `src/cli/transport.rs` under a `#[cfg(test)]` mod that spawns the actual `wx` daemon binary, connects via TCP, and verifies a full request/response round-trip.
  - Files: `src/cli/transport.rs`
  - Verify: cargo test tcp_integration_tests 2>&1 | grep -E '(test.*ok|test.*FAILED|running [0-9]+ test)'

- [ ] **T02: TCP vs local transport data comparison test** `est:45m`
  Write an integration test that queries the daemon via both TCP and local transport, verifying responses are identical.
  - Files: `src/cli/transport.rs`
  - Verify: cargo test tcp_integration_tests -- --include-ignored 2>&1 | grep -E '(test.*ok|test.*FAILED|running [0-9]+ test)'

## Files Likely Touched

- src/cli/transport.rs
