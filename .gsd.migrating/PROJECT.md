# wx-cli

## What This Is

A cross-platform Rust CLI tool for extracting and querying local WeChat 4.x data. Decrypts SQLCipher-encrypted databases, caches decrypted copies with mtime-aware invalidation, and provides a daemon-based IPC architecture for fast repeated queries. Currently uses Unix sockets (macOS/Linux) and Windows named pipes for local-only communication.

## Core Value

Query your local WeChat chat history, contacts, and moments from the command line with millisecond response times — data never leaves your machine.

## Project Shape

- **Complexity:** simple
- **Why:** Well-defined scope, existing codebase with clear module boundaries, single transport addition with refactoring

## Current State

Version 0.1.10. Fully functional CLI with 17 subcommands. Daemon auto-starts on first query. Cross-platform (macOS, Linux, Windows). No integration tests. Local IPC only.

## Architecture / Key Patterns

- Single binary: client and daemon (`WX_DAEMON_MODE` env var)
- Daemon uses tokio async runtime, Unix socket / Windows named pipe IPC
- JSON-line protocol: one request per connection
- mtime-aware decryption cache in `~/.wx-cli/cache/`
- Platform-specific memory scanners for SQLCipher key extraction
- All queries executed via rusqlite on decrypted DBs

## Capability Contract

See `.gsd/REQUIREMENTS.md` for the explicit capability contract.

## Milestone Sequence

- [ ] M001: TCP Transport — Add `--tcp host:port` global flag and TCP transport support to daemon and client