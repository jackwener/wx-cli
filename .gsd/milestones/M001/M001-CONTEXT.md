# M001: TCP Transport

**Gathered:** 2026-01-13
**Status:** Ready for planning

## Project Description

Add TCP socket transport to wx-cli's daemon communication layer, enabling remote clients to query WeChat data over the network. Refactor the existing platform-specific IPC code into a trait-based abstraction to eliminate duplication and make future transport additions easy.

## Why This Milestone

Currently wx-cli only supports local IPC (Unix sockets on macOS/Linux, named pipes on Windows). This limits usage to the same machine as the WeChat daemon. Adding TCP transport enables remote access, containerized deployments, and multi-machine setups.

## User-Visible Outcome

### When this milestone is complete, the user can:

- Start the daemon with TCP listening: `wx daemon start --tcp 127.0.0.1:9876`
- Query WeChat data over TCP: `wx sessions --tcp 127.0.0.1:9876`
- Use all existing commands without `--tcp` and get unchanged local behavior
- Check daemon status and logs over TCP: `wx daemon status --tcp 127.0.0.1:9876`

### Entry point / environment

- Entry point: `wx` CLI command with global `--tcp host:port` flag
- Environment: local dev or remote machine (TCP network)
- Live dependencies involved: wx-daemon process

## Completion Class

- Contract complete means: Transport traits defined, all three implementations compile, protocol handling is shared
- Integration complete means: Daemon listens on local + TCP simultaneously, client connects via TCP and gets correct response
- Operational complete means: Daemon starts with `--tcp`, handles bind errors cleanly, client fails with clear error when TCP unreachable

## Final Integrated Acceptance

To call this milestone complete, we must prove:

- `cargo check` passes on macOS, Linux, and Windows targets
- Daemon started with `--tcp 127.0.0.1:9876` accepts TCP connections and responds correctly
- Client with `--tcp 127.0.0.1:9876` returns same results as local transport
- Client with `--tcp 127.0.0.1:9999` (unreachable) fails with clear error within 15s
- Commands without `--tcp` still work via local transport (no regression)

## Architectural Decisions

### Transport abstraction via traits

**Decision:** Use `Listener` and `Connector` traits to abstract transport primitives, implement for Unix socket, Windows named pipe, and TCP.

**Rationale:** Current code has ~50 lines of duplicated JSON-line protocol handling across Unix/Windows. Traits eliminate duplication and provide clear extension point for future transports (TLS, WebSocket).

**Alternatives Considered:**
- Continue #[cfg] branching — current approach, hard to extend, duplicative
- `interprocess` crate for all transports — doesn't support TCP natively
- Abstract at protocol level only — would still need per-platform listener/connection code

### One request per connection (unchanged)

**Decision:** Keep existing protocol model — one JSON-line request per connection, no keepalive or pooling.

**Rationale:** Matches existing behavior, minimal complexity, sufficient for CLI usage patterns.

**Alternatives Considered:**
- Persistent connections with multiplexing — adds protocol complexity, not needed for CLI
- Connection pooling — overkill for single-client CLI tool

### Global CLI flag for TCP

**Decision:** `--tcp host:port` as global clap flag on root `Cli` struct, inherited by all subcommands.

**Rationale:** Discoverable, consistent UX. User specifies once, affects all commands.

**Alternatives Considered:**
- Environment variables — hidden, harder to discover
- Per-subcommand flag — repetitive, inconsistent
- Config file only — requires edit before use

### No built-in TCP security

**Decision:** No TLS, no auth tokens, no IP whitelist in this milestone. Bind exactly as user specifies.

**Rationale:** User handles firewall/ACL at OS level. Adding TLS requires cert management, tokio-rustls dependency, and significantly more complexity. Can be added later non-breaking.

**Alternatives Considered:**
- Default to localhost-only — too restrictive, user should control bind address
- Built-in IP whitelist — adds config complexity, OS firewall is better tool

## Error Handling Strategy

- **TCP bind failure:** `"TCP bind failed on {addr}: {reason}"` — daemon aborts startup
- **TCP connection failure:** `"Failed to connect to {addr}: {reason}"` — hard error, no fallback
- **Connection timeout:** 15s connect, 120s read/write (matches existing)
- **Connection dropped mid-request:** `"Connection lost: daemon closed or network error"`
- **Mixed transport mismatch:** `"No daemon listening on {addr}"` — same as current "daemon not alive" path
- **No `--tcp`:** Existing local transport behavior, no change

## Risks and Unknowns

- Windows named pipe refactoring may require `interprocess` crate changes — the crate's API differs from std Unix sockets
- `daemon start` subcommand needs to handle existing auto-start behavior (currently daemon starts on first query via `ensure_daemon()`)

## Existing Codebase / Prior Art

- `src/daemon/server.rs` — current IPC server, needs refactoring to use Listener trait
- `src/cli/transport.rs` — current IPC client, needs refactoring to use Connector trait
- `src/ipc.rs` — protocol types (Request/Response), well-abstracted, no changes needed
- `src/config.rs` — needs tcp_addr field extension

## Relevant Requirements

- R001 — TCP transport on server (M001/S01)
- R002 — TCP transport on client (M001/S02)
- R003 — Transport abstraction layer (M001/S01)
- R004 — Global `--tcp` CLI flag (M001/S02)
- R005 — Daemon start command (M001/S01)
- R006 — Cross-platform compilation (M001/S01)
- R007 — Error handling for TCP failures (M001/S02)
- R008 — Integration: CLI ↔ daemon over TCP (M001/S04)

## Scope

### In Scope

- Trait-based transport abstraction (Listener, Connector)
- TCP implementation (TcpListener, TcpStream)
- Global `--tcp host:port` CLI flag
- `wx daemon start` subcommand
- Error handling for TCP failures
- Cross-platform compilation

### Out of Scope / Non-Goals

- TLS encryption
- Authentication tokens
- IP whitelisting
- Connection pooling / keepalive
- Changing the JSON-line protocol

## Technical Constraints

- Must maintain backwards compatibility: no `--tcp` = existing behavior
- tokio is already a dependency (TcpListener/TcpStream available)
- `interprocess` crate for Windows named pipes — API differs from std

## Integration Points

- `src/daemon/server.rs` → `src/transport/` — server uses Listener trait
- `src/cli/transport.rs` → `src/transport/` — client uses Connector trait
- `src/config.rs` → optional tcp_addr field
- `src/cli/mod.rs` → global --tcp flag on Cli struct

## Testing Requirements

- `cargo check` on x86_64-unknown-linux-gnu, x86_64-pc-windows-msvc, and current platform
- Unit tests for transport::protocol.rs (JSON round-trip)
- Existing scanner tests continue passing
- Manual smoke test: daemon on TCP, client queries over TCP

## Acceptance Criteria

- S01: Transport traits defined, all implementations compile on all platforms, existing behavior unchanged
- S02: `wx daemon start --tcp 127.0.0.1:9876` starts daemon listening on TCP
- S03: `wx sessions --tcp 127.0.0.1:9876` connects via TCP and returns correct results
- S04: End-to-end TCP communication verified manually on localhost

## Open Questions

- None — scope confirmed, architecture agreed, error strategy defined