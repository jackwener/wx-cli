---
estimated_steps: 14
estimated_files: 16
skills_used: []
---

# T01: Add global --tcp CLI flag and wire into transport module

Add `--tcp` flag as a global argument on the root `Cli` struct in `src/cli/mod.rs`, not on individual subcommands. The flag takes `Option<String>` (e.g., `Some("127.0.0.1:9876")`). Wire this through the `dispatch()` function so every command path receives the TCP address. Modify all `cmd_*` functions in `src/cli/` to accept an optional `tcp_addr: Option<&str>` parameter. Update `src/cli/transport.rs`:
1. Add `send_tcp(req: Request, addr: &str) -> Result<Response>` function using `std::net::TcpStream` with 15s connect timeout and 120s read/write timeout
2. Add `is_alive_tcp(addr: &str) -> bool` for TCP liveness check
3. Update `send()` to accept `tcp_addr: Option<&str>`, routing to `send_tcp` when present
4. Update `is_alive()` to accept `tcp_addr: Option<&str>`, routing to `is_alive_tcp` when present
5. Update `ensure_daemon()` — when --tcp is specified, do NOT auto-start daemon (user explicitly chose TCP); if connection fails, hard error with clear message

Must-haves:
- 15s connect timeout on TcpStream
- 120s read/write timeout
- No silent fallback when --tcp specified
- Hard error with address and OS error on connection failure

Constraints:
- Use std::net::TcpStream (blocking, since CLI is sync)
- Keep #[cfg(unix)] / #[cfg(windows)] guards intact for local transport paths

## Inputs

- `src/cli/mod.rs`
- `src/cli/transport.rs`
- `src/cli/daemon_cmd.rs`
- `src/cli/sessions.rs`
- `src/ipc.rs`

## Expected Output

- `src/cli/mod.rs`
- `src/cli/transport.rs`

## Verification

cargo check 2>&1 | tail -5; grep -c 'tcp: Option<String>' src/cli/mod.rs; grep -q 'send_tcp' src/cli/transport.rs; grep -q 'is_alive_tcp' src/cli/transport.rs
