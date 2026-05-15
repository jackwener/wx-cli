# M001: TCP Transport

**Vision:** Add TCP socket transport to wx-cli's daemon communication layer with trait-based abstraction, enabling remote clients to query WeChat data over the network.

## Slices

- [ ] **S01: Transport abstraction layer** `risk:high` `depends:[]`
  > After this: Refactor complete, `cargo check` passes on all platforms, existing behavior unchanged. Transport traits defined and implemented for Unix socket + Windows named pipe.

- [ ] **S02: TCP server support** `risk:medium` `depends:[S01]`
  > After this: `wx daemon start --tcp 127.0.0.1:9876` starts daemon listening on TCP port 9876

- [ ] **S03: TCP client + global --tcp flag** `risk:medium` `depends:[S01]`
  > After this: `wx sessions --tcp 127.0.0.1:9876` connects via TCP and returns session data

- [ ] **S04: Integration smoke test** `risk:low` `depends:[S02,S03]`
  > After this: Daemon on TCP + client queries return same data as local transport

## Boundary Map

Not provided.
