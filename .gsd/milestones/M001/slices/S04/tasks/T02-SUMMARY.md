---
id: T02
parent: S04
milestone: M001
key_files:
  - src/cli/transport.rs
key_decisions:
  - Sequential TCP-then-local approach to avoid dual-daemon database contention: query via TCP first, terminate, then query via local transport
duration: 
verification_result: passed
completed_at: 2026-05-13T06:40:15.810Z
blocker_discovered: false
---

# T02: Added TCP vs local transport data comparison test that queries sessions via both transports and asserts deep equality

**Added TCP vs local transport data comparison test that queries sessions via both transports and asserts deep equality**

## What Happened

Added `test_tcp_matches_local_sessions` to the `tcp_integration_tests` module in `src/cli/transport.rs`. The test: (1) spawns the wx daemon on TCP using an ephemeral port, (2) queries sessions via `send_tcp(Request::Sessions{limit: 20}, &addr)`, (3) terminates the TCP daemon with SIGTERM, (4) queries sessions via local transport using `send(Request::Sessions{limit: 20}, None)` which auto-starts on Unix socket, (5) serializes both responses' data to `serde_json::Value` and asserts deep equality with a diff-friendly error message. Marked `#[ignore]` since it requires WeChat data to be present — run manually with `cargo test -- --ignored test_tcp_matches_local_sessions`. The test follows the same daemon lifecycle pattern as T01 (spawn → wait_for_ready → query → SIGTERM → wait).

## Verification

Ran cargo check — compiles with no errors. Ran cargo test — all 35 tests pass including 3 in integration_tests module. The new #[ignore] test is correctly registered and will be picked up on Unix with --include-ignored. Ran cargo check --target x86_64-pc-windows-msvc — passes (tcp_integration_tests module correctly excluded on Windows).

## Verification Evidence

| # | Command | Exit Code | Verdict | Duration |
|---|---------|-----------|---------|----------|
| 1 | `cargo check` | 0 | ✅ pass | 900ms |
| 2 | `cargo test 2>&1 | grep -E '(test.*ok|test.*FAILED|running [0-9]+ test|test result)'` | 0 | ✅ pass | 2060ms |
| 3 | `cargo check --target x86_64-pc-windows-msvc` | 0 | ✅ pass | 880ms |
| 4 | `cargo test tcp_integration_tests -- --include-ignored 2>&1 | grep -E '(test.*ok|test.*FAILED|running [0-9]+ test)'` | 0 | ✅ pass | 1500ms |

## Deviations

None.

## Known Issues

None.

## Files Created/Modified

- `src/cli/transport.rs`
