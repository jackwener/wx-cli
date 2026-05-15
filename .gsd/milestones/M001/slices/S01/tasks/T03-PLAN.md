---
estimated_steps: 15
estimated_files: 4
skills_used: []
---

# T03: Cross-platform compilation verification on all three targets

**Why**: R006 requires code compiles on macOS, Linux, and Windows. This is the final proof that the transport abstraction works across all platforms.

**Steps**:
1. Run `cargo check` (current platform — macOS)
2. Run `cargo check --target x86_64-unknown-linux-gnu`
3. Run `cargo check --target x86_64-pc-windows-msvc`
4. If any target fails, fix conditional compilation issues:
   - Check `#[cfg(unix)]` / `#[cfg(windows)]` annotations are correct
   - Ensure transport module handles `#[cfg(not(any(unix, windows)))]` gracefully
   - Verify `interprocess` crate is still only in `[target.'cfg(windows)'.dependencies]`
   - Verify `libc` is still only in `[target.'cfg(unix)'.dependencies]`
5. Run `cargo clippy` on current platform for lint warnings

**Constraints**:
- All three targets must pass with zero errors
- Warnings should be minimized but non-blocking
- Do NOT modify Cargo.toml dependency structure unless required for compilation

## Inputs

- `src/transport/mod.rs`
- `src/daemon/server.rs`
- `src/daemon/mod.rs`
- `src/cli/daemon_cmd.rs`
- `src/cli/mod.rs`
- `Cargo.toml`

## Expected Output

- `src/transport/mod.rs`
- `src/daemon/server.rs`
- `src/daemon/mod.rs`
- `Cargo.toml`

## Verification

cargo check && cargo check --target x86_64-unknown-linux-gnu && cargo check --target x86_64-pc-windows-msvc
