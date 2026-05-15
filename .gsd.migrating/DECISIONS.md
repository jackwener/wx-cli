# Decisions Register

<!-- Append-only. Never edit or remove existing rows.
     To reverse a decision, add a new row that supersedes it.
     Read this file at the start of any planning or research phase. -->

| # | When | Scope | Decision | Choice | Rationale | Revisable? | Made By |
|---|------|-------|----------|--------|-----------|------------|---------|
| D001 |  | architecture | Transport abstraction via traits | Listener and Connector traits with shared protocol.rs, implementations for Unix/Windows/TCP | Eliminates ~50 lines of duplicated JSON-line protocol handling, provides clear extension point for future transports | Yes | collaborative |
| D002 |  | architecture | Global --tcp CLI flag for transport selection | Global clap flag on root Cli struct, inherited by all subcommands | Discoverable, consistent UX. User specifies once, affects all commands | Yes | human |
| D003 |  | architecture | No built-in TCP security | No TLS, no auth tokens, no IP whitelist in this milestone. Bind exactly as user specifies. | User handles firewall/ACL at OS level. TLS adds cert management and dependency complexity. Can be added later non-breaking. | Yes | collaborative |
| D004 |  | architecture | One request per connection protocol model | One JSON-line request per connection, no keepalive or pooling | Matches existing behavior, minimal complexity, sufficient for CLI usage patterns | Yes | agent |
