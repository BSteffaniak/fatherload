# drillgame_quic_transport

QUIC byte transport for Drillgame. It moves bytes only and knows nothing about gameplay or
netcode messages; `drillgame_game::online::bridge` maps netcode messages onto it.

- `handle`: `NetHandle::start` spawns one network thread owning one persistent multi-thread
  Tokio runtime. The game loop sends `NetCommand`s (`Host`, `Join`, `Send`, `Disconnect`,
  `Shutdown`) and drains `NetEvent`s (`HostReady`, `Connected`, `Message`, `Disconnected`,
  `Stats`, `Error`, `ShutdownComplete`) without ever blocking.
- `network`: per connection, one long-lived bidirectional control stream with length-prefixed
  frames (everything reliable, in order) plus QUIC datagrams for unreliable messages (falling
  back to the stream when datagrams are unavailable or too large). Reader tasks are never
  wrapped in timeouts, so frames cannot be lost to cancellation.
- `conditioner`: built-in link conditioner applied on the network thread
  (`DRILLGAME_NET_SIM="latency=80,jitter=15,loss=2,dup=0"`).
- `tls`: self-signed certificates and host descriptors (pinned certificate + address).
- `lan`: mDNS publishing/browsing of hosted sessions and descriptor fetches.
- `config`, `api`: transport configuration and the public command/event types.

See the crate-level docs in `src/lib.rs` for the full API and connection lifecycle. The
real-QUIC integration tests in `tests/loopback.rs` run host and clients through the real
network thread on localhost.
