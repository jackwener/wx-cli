---
id: T02
parent: S02
milestone: M001
key_files:
  - src/cli/daemon_cmd.rs
key_decisions:
  - (none)
duration: 
verification_result: passed
completed_at: 2026-05-13T06:10:55.526Z
blocker_discovered: false
---

# T02: Wired --tcp into daemon stop command with manual-stop warning; status already reports TCP vs local

**Wired --tcp into daemon stop command with manual-stop warning; status already reports TCP vs local**

## What Happened

Wired `tcp_addr` into `cmd_stop` — when --tcp is set, warns that TCP daemon is a separate process and must be stopped manually (kill/taskkill PID). `cmd_daemon` already accepted `tcp_addr` from T01; now properly passes it through to both `cmd_status` and `cmd_stop`. `cmd_status` already reports TCP vs local transport (inherited from T01). `cmd_logs` remains unchanged — logs always go to the same file regardless of transport.

## Verification

cargo check passed with only a pre-existing unrelated warning (unused `bail` import in scanner/windows.rs). All 3 transport tests passed: tcp_connector_rejects_non_tcp_addr, tcp_listener_implements_listener, transport_addr_variants.

## Verification Evidence

| # | Command | Exit Code | Verdict | Duration |
|---|---------|-----------|---------|----------|
| 1 | `cargo check 2>&1 | tail -20` | 0 | ✅ pass | 880ms |
| 2 | `cargo test transport -- --nocapture 2>&1 | tail -30` | 0 | ✅ pass | 2470ms |

## Deviations

None.

## Known Issues

None.

## Files Created/Modified

- `src/cli/daemon_cmd.rs`
