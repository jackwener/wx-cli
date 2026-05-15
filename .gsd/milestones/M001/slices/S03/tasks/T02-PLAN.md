---
estimated_steps: 10
estimated_files: 4
skills_used: []
---

# T02: Verify cross-platform compilation and full test suite

Verify that all code compiles and tests pass across platforms.

Why: Confirm S01+S02+S03 changes don't break compilation on any target platform.

Do:
1. Run `cargo check` — must pass on native target
2. Run `cargo test` — all tests must pass (32 existing + 3 new = 35)
3. Run `cargo check --target x86_64-pc-windows-msvc` — must pass
4. Verify `wx --help` shows --tcp flag
5. Verify `wx daemon start --help` shows --tcp flag

Verify: All commands succeed with exit code 0

Done when: cargo check, cargo test, and Windows cross-check all pass; --tcp flag visible in CLI help

## Inputs

- `src/cli/transport.rs`
- `src/cli/mod.rs`
- `src/daemon/mod.rs`
- `src/transport/mod.rs`

## Expected Output

- Update the implementation and proof artifacts needed for this task.

## Verification

cargo check && cargo test && cargo check --target x86_64-pc-windows-msvc
