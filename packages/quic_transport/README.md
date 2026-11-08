# drillgame_quic_transport

QUIC byte transport for Drillgame: one network thread with a persistent Tokio runtime,
an ordered reliable control stream plus QUIC datagrams per connection, self-signed
certificates with host descriptors, LAN mDNS discovery, and a built-in link conditioner
(`DRILLGAME_NET_SIM="latency=80,jitter=15,loss=2,dup=0"`).

It moves bytes only and knows nothing about gameplay. See the crate-level docs in
`src/lib.rs` for the API (`NetHandle`, `NetCommand`, `NetEvent`).
