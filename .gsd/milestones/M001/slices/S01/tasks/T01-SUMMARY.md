---
id: T01
parent: S01
milestone: M001
key_files:
  - src/transport/mod.rs
  - src/main.rs
key_decisions:
  - Used Pin<Box<dyn Future>> instead of async fn for object-safe Listener/Connector traits
  - Temporarily duplicated dispatch() from server.rs in transport module to make handle_connection self-contained (will be shared in T02)
  - TcpConnector::connect returns error for non-Tcp TransportAddr variants
duration: 
verification_result: passed
completed_at: 2026-05-13T05:46:50.964Z
blocker_discovered: false
---

# T01: Created transport module with object-safe Listener/Connector traits, generic handle_connection, and TcpListener/TcpConnector implementations

**Created transport module with object-safe Listener/Connector traits, generic handle_connection, and TcpListener/TcpConnector implementations**

## What Happened

Created `src/transport/mod.rs` with:

1. **TransportAddr enum** with `Unix(PathBuf)`, `WindowsPipe(String)`, and `Tcp(SocketAddr)` variants.

2. **Object-safe Listener trait** — uses `Pin<Box<dyn Future>>` return type for `accept()` instead of `async fn`, making it object-safe. Associated `Stream` type bounds: `AsyncRead + AsyncWrite + Unpin + Send + 'static`.

3. **Object-safe Connector trait** — same pattern, `connect()` returns boxed future. Accepts `&TransportAddr` for routing.

4. **Generic `handle_connection<S>`** — async function accepting any `S: AsyncRead + AsyncWrite + Unpin`. Reads one JSON line, parses `Request`, dispatches, writes one JSON-line `Response`. Extracted logic from duplicated `handle_connection_unix/windows` in server.rs.

5. **TcpListener** — wraps `tokio::net::TcpListener`, implements `Listener` with `Stream = TcpStream`.

6. **TcpConnector** — implements `Connector` with `Stream = TcpStream`. Returns error for non-Tcp addresses.

7. **dispatch()** — temporary copy from server.rs (same logic) so `handle_connection` is self-contained. Will be shared in T02 per plan.

8. Added `pub mod transport;` to `src/main.rs`.

Added 3 unit tests: TransportAddr variant construction, TcpConnector implements Connector trait, TcpListener implements Listener trait. All pass.

server.rs left untouched per plan (moved in T02).

## Verification

cargo check passed (native + x86_64-pc-windows-msvc cross-target). cargo test transport: 3/3 passed. Structural grep verified: TransportAddr enum, Listener trait, Connector trait, handle_connection fn, TcpListener struct, TcpConnector struct all present in src/transport/mod.rs.

## Verification Evidence

| # | Command | Exit Code | Verdict | Duration |
|---|---------|-----------|---------|----------|
| 1 | `cargo check` | 0 | ✅ pass | 19020ms |
| 2 | `cargo check --target x86_64-pc-windows-msvc` | 0 | ✅ pass | 13620ms |
| 3 | `cargo test transport` | 0 | ✅ pass | 20770ms |
| 4 | `grep -q pub trait Listener src/transport/mod.rs && grep -q pub trait Connector src/transport/mod.rs && grep -q pub async fn handle_connection src/transport/mod.rs && grep -q pub struct TcpListener src/transport/mod.rs && grep -q pub struct TcpConnector src/transport/mod.rs` | 0 | ✅ pass | 100ms |

## Deviations

None.

## Known Issues

None.

## Files Created/Modified

- `src/transport/mod.rs`
- `src/main.rs`
