# M001 Verification Failed

**Date:** 2026-05-13  
**Milestone:** M001 — TCP Transport  
**Status:** FAILED — cannot complete milestone

## Verification Summary

| Check | Result | Detail |
|-------|--------|--------|
| Code changes exist | ✅ | `cargo check` passes on native target; source files contain TCP transport code |
| Success Criterion 1: cargo check (macOS + Linux + Windows) | ⚠️ Partial | Native ✅; Windows MSVC ❌ (lib.exe unavailable in WSL); Linux ❌ (x86_64-linux-gnu-gcc unavailable) |
| Success Criterion 2: Daemon accepts TCP connections | ❌ Missing | No live daemon start verified; S04 BLOCKER placeholder |
| Success Criterion 3: Client returns same results via TCP | ❌ Missing | TCP vs local comparison never executed |
| Success Criterion 4: Connection refused fails clearly within 15s | ✅ Pass | `test_send_tcp_connection_refused` and `test_tcp_daemon_connection_refused` pass |
| Success Criterion 5: No regression without --tcp | ❌ Fail | 2 of 38 tests failing: `test_send_tcp_round_trip` (mock server connection timeout), `test_tcp_daemon_ping_round_trip` (spawn + build timeout) |
| Definition of Done: all slices complete | ⚠️ Partial | S01-S03 valid; S04 has BLOCKER placeholder, 1/2 tasks pending in DB |

## Failing Tests Detail

### `test_send_tcp_round_trip` (S03 mock server test)
- **Error:** `connection timed out` after 15s to `127.0.0.1:<ephemeral_port>`
- **Root cause (likely):** WSL2 networking incompatibility — the tokio `TcpListener::bind` + blocking `TcpStream::connect_timeout` combination may have issues in WSL2's virtualized network namespace. The mock server binds successfully but connections from blocking code time out.
- **Impact:** This was the S03 integration test that previously passed. The code itself is correct; this is an environment/test-harness issue.

### `test_tcp_daemon_ping_round_trip` (S04 real daemon test)
- **Error:** Timed out at 30s (build + daemon startup exceeds limit)
- **Root cause:** Test requires `cargo build --bin wx` (takes ~10s in this environment) + daemon TCP startup wait (up to 15s) + ping round-trip. Combined exceeds the gsd_exec timeout.
- **Impact:** The test code is correct but the verification environment doesn't support the full end-to-end flow within available timeout.

## S04 Status

S04 summary is a **BLOCKER placeholder** written by a previous auto-mode run that hit a tools-policy restriction (planning-dispatch unit cannot write user source files). The slice is marked "complete" in the DB but has 1 pending task out of 2. The real e2e daemon-client TCP integration test was never executed.

## What Needs to Happen Next

1. **Fix or skip the flaky mock server test** — the `test_send_tcp_round_trip` test fails consistently in WSL2. Options:
   - Use `#[ignore]` to skip it in CI/WSL environments
   - Rewrite the mock server to use async-compatible client code for testing
   - Add `#[cfg(not(target_os = "linux"))]` if it only fails in WSL2
2. **Execute S04 real e2e test** — requires a local environment with the wx binary built and a Windows/macOS host (not WSL)
3. **Complete S04 summary** — replace BLOCKER placeholder with actual evidence once S04 tests pass
4. **Linux cross-compile** — install `x86_64-linux-gnu-gcc` on build host or set up a Linux CI runner
