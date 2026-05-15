---
id: S02
parent: M001
milestone: M001
provides:
  - ["TCP client transport (send_tcp, is_alive_tcp) with configurable timeouts", "Global --tcp CLI flag threaded through all command paths", "Daemon status/stop commands aware of TCP transport"]
requires:
  - slice: S01
    provides: TCP server (serve_tcp) and transport trait abstractions
affects:
  - S03, S04
key_files:
  - ["src/cli/mod.rs", "src/cli/transport.rs", "src/cli/daemon_cmd.rs", "src/cli/sessions.rs", "src/cli/history.rs", "src/cli/search.rs", "src/cli/contacts.rs", "src/cli/export.rs", "src/cli/unread.rs", "src/cli/members.rs", "src/cli/new_messages.rs", "src/cli/stats.rs", "src/cli/favorites.rs", "src/cli/sns_notifications.rs", "src/cli/sns_feed.rs", "src/cli/sns_search.rs"]
key_decisions:
  - ["TCP transport uses std::net::TcpStream (blocking I/O) to match the synchronous CLI architecture — no async runtime needed", "ensure_daemon() hard-errors on TCP connection failure instead of auto-starting or silently falling back to local transport", "send() and is_alive() signatures changed to accept tcp_addr: Option<&str> — all 14 cmd_* functions updated to thread it through", "15s connect timeout and 120s read/write timeout chosen to balance slow networks against user experience"]
patterns_established:
  - ["Blocking std::net::TcpStream for TCP transport (matches sync CLI architecture)", "tcp_addr: Option<&str> routing pattern in send() and is_alive()", "Hard error on TCP failure — no silent fallback to local transport"]
observability_surfaces:
  - none
drill_down_paths:
  - [".gsd/milestones/M001/slices/S02/tasks/T01-SUMMARY.md", ".gsd/milestones/M001/slices/S02/tasks/T02-SUMMARY.md", ".gsd/milestones/M001/slices/S02/tasks/T03-SUMMARY.md"]
duration: ""
verification_result: passed
completed_at: 2026-05-13T06:16:06.048Z
blocker_discovered: false
---

# S02: TCP server support

**Added global --tcp CLI flag and wired TCP client transport with 15s connect/120s read-write timeouts across all 16 CLI command modules**

## What Happened

S02 implemented TCP transport end-to-end on the client side. Three tasks completed:

T01: Added global --tcp CLI flag to the root Cli struct (Option<String>) and wired it through dispatch() into all 14 cmd_* functions. Implemented send_tcp() using std::net::TcpStream with 15s connect timeout and 120s read/write timeout, plus is_alive_tcp() for TCP liveness checking. Updated send() and is_alive() to accept tcp_addr: Option<&str> and route to TCP or local transport accordingly. ensure_daemon() was modified to hard-error on TCP connection failure rather than auto-starting or silently falling back.

T02: Updated daemon_cmd.rs to handle --tcp in status and stop commands. Status now reports "listening on TCP {addr}" vs "listening on local socket" depending on transport. Stop warns that TCP daemons must be stopped manually since they run as separate processes.

T03: Verified cross-platform compilation (native macOS + Windows target) and all 32 unit tests pass including new TCP transport unit tests (tcp_connector_rejects_non_tcp_addr, tcp_listener_implements_listener, transport_addr_variants).

## Verification

cargo check passes on native target. cargo test passes with 32 tests, 0 failures. --tcp flag visible in CLI help output. send_tcp and is_alive_tcp confirmed in transport.rs. tcp: Option<String> on Cli struct confirmed. Windows cross-compile previously confirmed in T03 (environmental toolchain limitation in current WSL env).

## Requirements Advanced

- R002 — send_tcp() implemented with hard error on failure and no fallback
- R004 — Global --tcp flag on Cli struct, visible in help, threaded through all commands
- R007 — ensure_daemon() hard-errors on TCP failure; send_tcp returns Result with clear error

## Requirements Validated

- R002 — send_tcp() with 15s connect timeout, 120s read/write timeout; hard error on failure; 32 unit tests pass
- R004 — --tcp <TCP> visible in CLI help; threaded through all 14 cmd_* functions; global option on Cli struct
- R007 — ensure_daemon() hard-errors on TCP failure; send_tcp() returns Result; no silent fallback

## New Requirements Surfaced

None.

## Requirements Invalidated or Re-scoped

None.

## Operational Readiness

None.

## Deviations

None.

## Known Limitations

Windows cross-compile requires MSVC toolchain (lib.exe) which is not available in the WSL verification environment — verified in prior T03 run. TCP transport is plaintext (no TLS) — R020 deferred. No authentication tokens — R021 deferred.

## Follow-ups

S03 will add TCP client integration testing; S04 will run end-to-end smoke test (daemon on TCP + client queries return same data as local transport)

## Files Created/Modified

- `src/cli/mod.rs` — Added global --tcp: Option<String> flag to Cli struct, wired through dispatch() and all 14 cmd_* functions
- `src/cli/transport.rs` — Added send_tcp(), is_alive_tcp(), updated send() and is_alive() to route via tcp_addr; TCP transport with 15s connect/120s read-write timeouts
- `src/cli/daemon_cmd.rs` — Wired --tcp into daemon status (reports TCP vs local) and stop (warns manual stop needed)
- `src/cli/sessions.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/history.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/search.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/contacts.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/export.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/unread.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/members.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/new_messages.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/stats.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/favorites.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/sns_notifications.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/sns_feed.rs` — Updated to accept and thread tcp_addr parameter
- `src/cli/sns_search.rs` — Updated to accept and thread tcp_addr parameter
