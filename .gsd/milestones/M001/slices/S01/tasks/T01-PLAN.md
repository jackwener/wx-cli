---
estimated_steps: 16
estimated_files: 2
skills_used: []
---

# T01: Create transport module with traits, generic handler, and TCP implementation

**Why**: Establish the transport abstraction layer — the core deliverable of S01. Define traits that abstract over Unix socket, Windows named pipe, and TCP. Extract the duplicated JSON-line protocol handling from server.rs into a generic `handle_connection` function.

**Steps**:
1. Create `src/transport/mod.rs` with module declarations
2. Define `TransportAddr` enum with variants: `Unix(PathBuf)`, `WindowsPipe(String)`, `Tcp(SocketAddr)`
3. Define `Listener` trait (object-safe): `type Stream` (Send + AsyncRead + AsyncWrite + Unpin), `async fn accept(&mut self) -> Result<Self::Stream>`
4. Define `Connector` trait (object-safe): same `Stream` type, `async fn connect(&self, addr: &TransportAddr) -> Result<Self::Stream>`
5. Implement `handle_connection<S>` as an async generic function accepting `S: AsyncRead + AsyncWrite + Unpin`, `&DbCache`, `&Arc<tokio::sync::RwLock<Arc<Names>>>` — reads one JSON line, parses `Request`, calls `dispatch`, writes one JSON-line `Response` (extracted from current handle_connection_unix/handle_connection_windows in server.rs)
6. Implement `struct TcpListener` wrapping `tokio::net::TcpListener` with `Listener` impl
7. Implement `struct TcpConnector` with `Connector` impl using `tokio::net::TcpStream`
8. Add `pub mod transport;` to `src/main.rs`
9. Keep existing server.rs/handler functions untouched at this point (moved in T02)

**Constraints**:
- `Listener` and `Connector` must be object-safe (no `Self` in method params/returns beyond standard patterns)
- `handle_connection` must be `pub(crate)` for server.rs to use
- Do NOT modify ipc.rs (protocol types are already well-abstracted)
- TcpListener/TcpConnector use std `tokio::net` — already available as dependency

## Inputs

- `src/ipc.rs`
- `src/daemon/server.rs`
- `src/daemon/cache.rs`
- `src/daemon/query.rs`
- `Cargo.toml`

## Expected Output

- `src/transport/mod.rs`

## Verification

cargo check && test -f src/transport/mod.rs && grep -q "pub trait Listener" src/transport/mod.rs && grep -q "pub trait Connector" src/transport/mod.rs && grep -q "pub async fn handle_connection" src/transport/mod.rs && grep -q "pub struct TcpListener" src/transport/mod.rs && grep -q "pub struct TcpConnector" src/transport/mod.rs
