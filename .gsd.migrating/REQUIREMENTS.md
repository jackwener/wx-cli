# Requirements

## Active

### R001 — TCP transport on server
- Class: core-capability
- Status: active
- Description: Daemon listens on TCP when `--tcp host:port` is specified, in addition to local transport
- Why it matters: Enables remote clients to query WeChat data over network
- Source: user
- Primary owning slice: M001/S01
- Supporting slices: M001/S02
- Validation: unmapped
- Notes: Bind exactly as user specifies, no TLS, no IP whitelist

### R002 — TCP transport on client
- Class: core-capability
- Status: active
- Description: Client connects via TCP when `--tcp host:port` is specified, with no local fallback
- Why it matters: Users explicitly choosing TCP must connect to that address
- Source: user
- Primary owning slice: M001/S02
- Supporting slices: none
- Validation: unmapped
- Notes: Hard error if connection fails, no silent fallback

### R003 — Transport abstraction layer
- Class: quality-attribute
- Status: active
- Description: Transport layer uses trait-based abstraction (Listener/Connector) to eliminate platform duplication
- Why it matters: Makes adding new transports (TCP, future TLS) easy without duplicating protocol logic
- Source: inferred
- Primary owning slice: M001/S01
- Supporting slices: M001/S02, M001/S03
- Validation: unmapped
- Notes: Must support Unix socket, Windows named pipe, and TCP

### R004 — Global `--tcp` CLI flag
- Class: primary-user-loop
- Status: active
- Description: `--tcp host:port` is a global CLI flag, affecting all commands including `daemon status`, `daemon logs`, and all query commands
- Why it matters: Discoverable, consistent interface for TCP across all commands
- Source: user
- Primary owning slice: M001/S02
- Supporting slices: none
- Validation: unmapped
- Notes: Replaces env var approach, cleaner UX

### R005 — Daemon start command
- Class: primary-user-loop
- Status: active
- Description: New `wx daemon start` subcommand to explicitly start the daemon with configurable options
- Why it matters: Currently daemon auto-starts on first query; explicit start gives user control over transport config
- Source: user
- Primary owning slice: M001/S01
- Supporting slices: none
- Validation: unmapped
- Notes: Should support `--tcp` flag

### R006 — Cross-platform compilation
- Class: constraint
- Status: active
- Description: Code compiles on macOS, Linux, and Windows (`cargo check` on all targets)
- Why it matters: Project is cross-platform by design, TCP must work on all three
- Source: inferred
- Primary owning slice: M001/S01
- Supporting slices: M001/S02, M001/S03
- Validation: unmapped
- Notes: TcpListener/TcpStream are std library, should be trivial

### R007 — Error handling for TCP failures
- Class: failure-visibility
- Status: active
- Description: TCP bind/connect failures produce clear error messages with no silent fallback
- Why it matters: Users need to know when transport configuration fails
- Source: inferred
- Primary owning slice: M001/S02
- Supporting slices: none
- Validation: unmapped
- Notes: 15s connect timeout, 120s read/write timeout

### R008 — Integration: CLI ↔ daemon over TCP
- Class: integration
- Status: active
- Description: End-to-end verification: CLI and daemon communicate successfully over TCP on localhost
- Why it matters: Proves the transport actually works, not just compiles
- Source: inferred
- Primary owning slice: M001/S04
- Supporting slices: none
- Validation: unmapped
- Notes: Manual smoke test sufficient, no automated integration tests

## Deferred

### R020 — TLS encryption for TCP transport
- Class: compliance/security
- Status: deferred
- Description: Optional TLS encryption on TCP transport for secure remote access
- Why it matters: Plaintext TCP exposes chat data to network sniffing
- Source: inferred
- Primary owning slice: none
- Supporting slices: none
- Validation: unmapped
- Notes: Deferred — adds tokio-rustls dependency and cert management complexity

### R021 — Authentication tokens for TCP
- Class: compliance/security
- Status: deferred
- Description: Token-based authentication for TCP connections
- Why it matters: Prevents unauthorized access to WeChat data over network
- Source: inferred
- Primary owning slice: none
- Supporting slices: none
- Validation: unmapped
- Notes: Deferred — requires protocol change (Auth request type)

### R022 — TCP connection keepalive
- Class: quality-attribute
- Status: deferred
- Description: Persistent TCP connections with keepalive for reduced latency
- Why it matters: Current one-request-per-connection model has connection overhead
- Source: inferred
- Primary owning slice: none
- Supporting slices: none
- Validation: unmapped
- Notes: Deferred — requires protocol and connection management changes

## Out of Scope

### R030 — Network-level access control
- Class: constraint
- Status: out-of-scope
- Description: IP whitelisting, firewall rules, or network ACLs within the application
- Why it matters: Prevents scope creep into network security management
- Source: user
- Primary owning slice: none
- Supporting slices: none
- Validation: n/a
- Notes: User handles firewall/ACL at OS level

## Traceability

| ID | Class | Status | Primary owner | Supporting | Proof |
|---|---|---|---|---|---|
| R001 | core-capability | active | M001/S01 | M001/S02 | unmapped |
| R002 | core-capability | active | M001/S02 | none | unmapped |
| R003 | quality-attribute | active | M001/S01 | M001/S02, M001/S03 | unmapped |
| R004 | primary-user-loop | active | M001/S02 | none | unmapped |
| R005 | primary-user-loop | active | M001/S01 | none | unmapped |
| R006 | constraint | active | M001/S01 | M001/S02, M001/S03 | unmapped |
| R007 | failure-visibility | active | M001/S02 | none | unmapped |
| R008 | integration | active | M001/S04 | none | unmapped |
| R020 | compliance/security | deferred | none | none | unmapped |
| R021 | compliance/security | deferred | none | none | unmapped |
| R022 | quality-attribute | deferred | none | none | unmapped |
| R030 | constraint | out-of-scope | none | none | n/a |

## Coverage Summary

- Active requirements: 8
- Mapped to slices: 8
- Validated: 0
- Unmapped active requirements: 0