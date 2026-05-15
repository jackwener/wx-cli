---
estimated_steps: 19
estimated_files: 1
skills_used: []
---

# T01: Add TCP client integration tests with mock server

Add integration tests to src/cli/transport.rs that exercise send_tcp() and is_alive_tcp() against a real mock TCP server.

Why: S02 implemented the TCP client code but has no integration tests. This slice must prove the client works end-to-end.

Files: `src/cli/transport.rs`

Do:
1. Add a `#[cfg(test)] mod integration_tests` module to `src/cli/transport.rs`
2. Implement three `#[tokio::test(flavor = "multi_thread")]` tests (multi_thread required because send_tcp uses blocking std::net::TcpStream while the mock server uses async tokio):
   - `test_send_tcp_round_trip`: Spawn a mock TCP server on a random port that responds to {"cmd":"sessions","limit":20} with {"ok":true,"sessions":[{"name":"test"}]}. Call send_tcp(Request::Sessions{limit:20}, addr) and assert Response.ok == true.
   - `test_send_tcp_connection_refused`: Call send_tcp against a port with no listener. Assert Err is returned.
   - `test_is_alive_tcp_false`: Call is_alive_tcp against a random unused port. Assert false.
3. The mock server should:
   - Bind tokio::net::TcpListener to 127.0.0.1:0 (random port)
   - Accept one connection
   - Read one line of JSON
   - Respond with a valid JSON-line Response: {"ok":true,"sessions":[{"name":"test"}]}
   - Close connection
4. Use `use crate::ipc::{Request, Response}` for test types
5. Keep tests self-contained — no external dependencies needed beyond existing tokio and serde_json

Verify: `cargo test integration_tests` — all 3 tests pass

Done when: 3 new integration tests exist and pass, covering success, connection failure, and liveness check paths

## Inputs

- `src/cli/transport.rs`
- `src/ipc.rs`
- `Cargo.toml`

## Expected Output

- `src/cli/transport.rs`

## Verification

cargo test integration_tests -- --test-threads=1
