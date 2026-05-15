# S03: TCP client + global --tcp flag — UAT

**Milestone:** M001
**Written:** 2026-05-13T06:27:54.985Z

# S03: TCP client + global --tcp flag — UAT

**Milestone:** M001
**Written:** 2026-05-13

## UAT Type

- UAT mode: artifact-driven
- Why this mode is sufficient: This slice adds test coverage and compilation verification only — no new runtime behavior beyond what S02 shipped. The three integration tests + 35/35 full suite pass + cross-platform compilation serve as the verification gate.

## Preconditions

- Cargo toolchain installed with x86_64-pc-windows-msvc target
- No external services required — tests use in-process mock TCP server

## Smoke Test

Run `cargo test` and confirm all 35 tests pass, including the 3 new TCP integration tests.

## Test Cases

### 1. TCP round-trip: send_tcp() returns valid response

1. Run `cargo test test_send_tcp_round_trip -- --test-threads=1`
2. **Expected:** test passes — mock server on ephemeral port receives request, sends valid Response, send_tcp() parses it and returns Ok

### 2. Connection refused: send_tcp() errors on unreachable port

1. Run `cargo test test_send_tcp_connection_refused -- --test-threads=1`
2. **Expected:** test passes — send_tcp() returns Err for port 59876 with no listener

### 3. Liveness check: is_alive_tcp() returns false for unused port

1. Run `cargo test test_is_alive_tcp_false -- --test-threads=1`
2. **Expected:** test passes — is_alive_tcp("127.0.0.1:59877") returns false

### 4. Cross-platform compilation

1. Run `cargo check --target x86_64-pc-windows-msvc`
2. **Expected:** exit 0, no errors

### 5. CLI --tcp flag visibility

1. Run `cargo run -- --help` and `cargo run -- daemon start --help`
2. **Expected:** both show --tcp flag in output

## Edge Cases

### No listener on target port
- Covered by test_send_tcp_connection_refused — hard error returned, no silent fallback

### Unreachable host
- Covered by is_alive_tcp_false test — returns false without hanging

## Failure Signals

- cargo test exit code != 0 or fewer than 35 tests passing
- cargo check exit code != 0 on any target
- --tcp flag missing from CLI help

## Not Proven By This UAT

- End-to-end daemon TCP server + client communication with real WeChat data (covered by S04)
- TCP transport performance under load
- Linux cross-compile (environment limitation — no x86_64-linux-gnu-gcc on this Windows machine)

## Notes for Tester

- Tests use #[tokio::test(flavor = "multi_thread")] — must run with --test-threads=1 to avoid port conflicts between concurrent mock servers
- Minor unused import warning for `bail` in src/scanner/windows.rs is pre-existing and unrelated to this slice
