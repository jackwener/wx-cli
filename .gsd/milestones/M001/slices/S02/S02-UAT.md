# S02: TCP server support — UAT

**Milestone:** M001
**Written:** 2026-05-13T06:16:06.049Z

# S02: TCP server support — UAT

**Milestone:** M001
**Written:** 2025-05-13

## UAT Type

- UAT mode: artifact-driven
- Why this mode is sufficient: TCP transport is verified through compilation checks and unit tests; end-to-end network testing requires a running daemon (covered in S04)

## Preconditions

- Project compiles: `cargo check` passes
- `--tcp` flag is available on all commands

## Smoke Test

```
cargo run -- --help | grep tcp
```
Expected: `--tcp <TCP>` appears in output with description about connecting via TCP.

## Test Cases

### 1. Global --tcp flag availability

1. Run `cargo run -- --help`
2. **Expected:** `--tcp <TCP>` listed as a global option (not on a subcommand)

### 2. TCP transport function existence

1. Verify `send_tcp` and `is_alive_tcp` exist in `src/cli/transport.rs`
2. **Expected:** Both functions found via grep

### 3. Daemon status reports TCP

1. In daemon_cmd.rs, verify status command checks `is_alive_tcp` when tcp_addr is set
2. **Expected:** Status output distinguishes TCP vs local transport

### 4. Daemon stop warns for TCP

1. In daemon_cmd.rs, verify stop command warns when tcp_addr is set
2. **Expected:** Warning that TCP daemon must be stopped manually

## Edge Cases

### TCP bind failure (port in use)

1. This slice handles the client side; bind errors are handled by the server (S01)
2. **Expected:** Server produces clear error message with address and errno

### TCP connect failure

1. Attempt `wx sessions --tcp 127.0.0.1:9999` with nothing listening on port 9999
2. **Expected:** Hard error with address and errno (not silent fallback to local transport)

## Failure Signals

- `cargo check` fails — TCP code has compilation errors
- `--tcp` flag not in help — flag not wired correctly
- Unit tests fail — transport routing broken

## Not Proven By This UAT

- Actual TCP data exchange (requires running daemon — S04 covers this)
- TLS encryption (R020 deferred)
- Authentication tokens (R021 deferred)
- Connection keepalive (R022 deferred)
- Network-level access control (R030 out of scope)

## Notes for Tester

- Windows cross-compile requires MSVC toolchain installed (lib.exe available)
- TCP timeouts: 15s connect, 120s read/write
- ensure_daemon() will NOT auto-start when --tcp is specified — this is intentional
