---
phase: milestone-closeout
phase_name: M001 TCP Transport
project: wx-cli
generated: "2026-05-13T14:58:00Z"
counts:
  decisions: 7
  lessons: 5
  patterns: 6
  surprises: 2
missing_artifacts: []
---

### Decisions

- **Transport abstraction via traits**: Chose `Listener` and `Connector` object-safe traits to abstract Unix socket, Windows named pipe, and TCP transports over continuing `#[cfg]` branching or adopting the `interprocess` crate. Rationale: eliminates ~50 lines of duplicated JSON-line protocol handling, provides clear extension point for future transports. Source: M001-CONTEXT.md/Architectural Decisions
- **Blocking `std::net::TcpStream` for TCP transport**: Chose blocking I/O over async tokio TCP to match the synchronous CLI architecture — no async runtime needed in the client process. Source: S02-SUMMARY.md/Key Decisions
- **`ensure_daemon()` hard-errors on TCP failure**: Chose to hard-error on TCP connection failure instead of auto-starting or silently falling back to local transport. Rationale: user explicitly requested TCP, silent fallback would mask misconfiguration. Source: S02-SUMMARY.md/Key Decisions
- **15s connect / 120s read-write timeouts**: Chosen to balance slow networks against user experience. Source: S02-SUMMARY.md/Key Decisions
- **Global `--tcp` CLI flag**: Placed `tcp: Option<String>` on the root `Cli` struct inherited by all subcommands, over per-subcommand flags or environment variables. Rationale: discoverable, consistent UX. Source: M001-CONTEXT.md/Architectural Decisions
- **One request per connection (unchanged protocol)**: Kept existing JSON-line protocol model — one request per connection, no keepalive or pooling. Rationale: matches existing behavior, minimal complexity, sufficient for CLI usage. Source: M001-CONTEXT.md/Architectural Decisions
- **Sequential TCP-then-local approach for data comparison**: Query via TCP first, terminate daemon, then query via local transport to avoid dual-daemon database contention. Source: S04-T02-SUMMARY.md/Key Decisions

### Lessons

- **`Pin<Box<dyn Future>>` needed for object-safe trait methods**: Trait methods returning async values must use `Pin<Box<dyn Future<Output = T>>>` to be object-safe in Rust, since `async fn` in traits requires `Sized` Self. Source: S01-SUMMARY.md/What Happened
- **Cross-platform `cargo check` on Windows host requires MSVC toolchain**: `cargo check --target x86_64-pc-windows-msvc` requires `lib.exe` from Visual Studio Build Tools, which is not available in WSL or minimal CI environments. Code correctness can still be verified via `#[cfg]` review when the toolchain is missing. Source: S02-SUMMARY.md/Known Limitations
- **`tcp_addr: Option<&str>` routing must be threaded through ALL command functions**: Every `cmd_*` function needed updating to accept and pass through the `tcp_addr` parameter — missing even one would break the `--tcp` flag for that subcommand. Source: S02-SUMMARY.md/What Happened
- **`#[tokio::test(flavor = "multi_thread")]` needed for blocking + async interop**: Tests calling blocking `send_tcp()` alongside async mock servers require the multi-threaded tokio runtime to avoid deadlocks. Source: S03-SUMMARY.md/Patterns Established
- **`stream.into_split()` enables independent read/write in mock server tests**: Splitting the TCP stream allows the mock server to read requests and write responses on independent halves, matching real server behavior. Source: S03-SUMMARY.md/Patterns Established

### Patterns

- **Generic `handle_connection` function shared across transports**: A single async generic function handles the JSON-line protocol for all transport types (Unix, Windows pipe, TCP), eliminating duplication. Source: S01-SUMMARY.md/What Happened
- **`tcp_addr: Option<&str>` routing pattern in `send()` and `is_alive()`**: Both functions accept an optional TCP address; when `Some`, route to `send_tcp()`/`is_alive_tcp()`, otherwise use local transport. Applied uniformly across all 14+ command functions. Source: S02-SUMMARY.md/Patterns Established
- **Hard error on TCP failure — no silent fallback**: All TCP code paths return `Result` with descriptive errors; no code path silently falls back to local transport when TCP is requested. Source: S02-SUMMARY.md/Patterns Established
- **Multi-threaded tokio test for blocking + async interop**: `#[tokio::test(flavor = "multi_thread")]` enables tests that mix blocking network calls with async mock servers. Source: S03-SUMMARY.md/Patterns Established
- **Mock TCP server with `stream.into_split()`**: Test mock servers split TCP streams for independent read/write, matching real server architecture. Source: S03-SUMMARY.md/Patterns Established
- **Daemon subprocess lifecycle for integration tests**: Spawn daemon with unique env vars (`WX_DAEMON_MODE=1`, `WX_DAEMON_TCP_ADDR`), poll `is_alive_tcp()` for readiness, SIGTERM for clean shutdown, verify exit code 0. Source: S04-T01-SUMMARY.md/What Happened

### Surprises

- **Linux cross-compile blocked by missing C toolchain on Windows host**: Despite Rust being cross-platform, the `cc` crate requires a C cross-compiler (`x86_64-linux-gnu-gcc`) for Linux targets on Windows. This is an environmental limitation, not a code issue. Source: S01-SUMMARY.md/What Happened
- **`#[cfg(unix)]` gated integration tests run on MINGW64**: The `tcp_integration_tests` module gated with `#[cfg(unix)]` unexpectedly compiles and runs under MINGW64/git bash on Windows because MINGW64 reports itself as a Unix-like environment, causing daemon subprocess tests to fail due to Windows-specific process handling. Source: S04-T01-SUMMARY.md/What Happened
