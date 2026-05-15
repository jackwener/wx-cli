---
estimated_steps: 9
estimated_files: 1
skills_used: []
---

# T02: TCP vs local transport data comparison test

Write an integration test that queries the daemon via both TCP and local transport, verifying responses are identical.

Steps:
1. Add test `test_tcp_matches_local_sessions` in same integration test module
2. Start daemon same as T01
3. Query sessions via TCP: `send_tcp(Request::Sessions{limit: 20}, &addr)`
4. Query sessions via local: `send(Request::Sessions{limit: 20}, None)`
5. Compare: parse both as `serde_json::Value`, assert deep equality
6. Mark with `#[ignore]` since it requires WeChat data to be present — can be run manually with `cargo test -- --ignored`
7. Kill daemon subprocess

## Inputs

- ``src/cli/transport.rs``

## Expected Output

- ``src/cli/transport.rs``

## Verification

cargo test tcp_integration_tests -- --include-ignored 2>&1 | grep -E '(test.*ok|test.*FAILED|running [0-9]+ test)'
