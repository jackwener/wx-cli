---
id: T01
parent: S02
milestone: M001
key_files:
  - src/cli/mod.rs
  - src/cli/transport.rs
  - src/cli/daemon_cmd.rs
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
key_decisions:
  - TCP transport uses std::net::TcpStream (blocking, matching sync CLI architecture)
  - ensure_daemon() hard-errors on TCP connection failure instead of auto-starting or silently falling back
  - send() and is_alive() signatures changed to accept tcp_addr: Option<&str> — all 14 cmd_* functions updated to thread it through
duration: 
verification_result: passed
completed_at: 2026-05-13T06:09:39.581Z
blocker_discovered: false
---

# T01: Added global --tcp CLI flag and wired TCP transport with 15s connect/120s read-write timeouts, no silent fallback

**Added global --tcp CLI flag and wired TCP transport with 15s connect/120s read-write timeouts, no silent fallback**

## What Happened

Added `--tcp` as a global CLI argument on the root `Cli` struct in `src/cli/mod.rs`, taking `Option<String>`. Updated `dispatch()` to extract and pass `tcp_addr: Option<&str>` to all 14 `cmd_*` functions across the CLI module. Rewrote `src/cli/transport.rs`: added `send_tcp(req, addr)` using `TcpStream::connect_timeout` with 15s connect timeout and 120s read/write timeout; added `is_alive_tcp(addr)` for TCP liveness check via ping; updated `send()` and `is_alive()` to accept `tcp_addr: Option<&str>` and route to TCP functions when present; updated `ensure_daemon()` to skip auto-start and produce a hard error with address + OS errno when `--tcp` is specified but daemon is unreachable. Updated `cmd_daemon()` and `cmd_status()` to accept and report TCP address. All `#[cfg(unix)]`/`#[cfg(windows)]` guards preserved for local transport paths. `cargo check` passes on native (x86_64-pc-windows-msvc) target; Linux cross-compile toolchain not installed on this Windows machine but code is platform-agnostic for the new TCP paths.

## Verification

cargo check passes on native and x86_64-pc-windows-msvc targets. CLI help shows --tcp <TCP> as global option. send_tcp and is_alive_tcp confirmed in transport.rs. tcp: Option<String> on Cli struct confirmed.

## Verification Evidence

| # | Command | Exit Code | Verdict | Duration |
|---|---------|-----------|---------|----------|
| 1 | `cargo check 2>&1 | tail -5` | 0 | ✅ pass | 12100ms |
| 2 | `cargo check --target x86_64-pc-windows-msvc 2>&1 | tail -5` | 0 | ✅ pass | 12600ms |
| 3 | `grep -c 'tcp: Option<String>' src/cli/mod.rs` | 0 | ✅ pass | 50ms |
| 4 | `grep -q 'send_tcp' src/cli/transport.rs` | 0 | ✅ pass | 30ms |
| 5 | `grep -q 'is_alive_tcp' src/cli/transport.rs` | 0 | ✅ pass | 30ms |
| 6 | `cargo run -- --help 2>&1 | grep tcp` | 0 | ✅ pass | 9950ms |

## Deviations

None.

## Known Issues

None.

## Files Created/Modified

- `src/cli/mod.rs`
- `src/cli/transport.rs`
- `src/cli/daemon_cmd.rs`
- `src/cli/sessions.rs`
- `src/cli/history.rs`
- `src/cli/search.rs`
- `src/cli/contacts.rs`
- `src/cli/export.rs`
- `src/cli/unread.rs`
- `src/cli/members.rs`
- `src/cli/new_messages.rs`
- `src/cli/stats.rs`
- `src/cli/favorites.rs`
- `src/cli/sns_notifications.rs`
- `src/cli/sns_feed.rs`
- `src/cli/sns_search.rs`
