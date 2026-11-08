# drillgame_netcode

Sans-IO multiplayer netcode for Drillgame (see `multiplayer-architecture-plan.md`, sections
5-8, 11.2, 15 and 16): server-authoritative state replication, client-side prediction of the
local rig and interpolation of remote rigs.

No sockets, threads, async runtimes or wall clock: time is always passed in as a `Duration`.
Bytes move through `drillgame_quic_transport` in the app and through `LinkSim` in tests.

## Modules

- `simulation`: the `NetSimulation` trait sessions are generic over, plus `Relevance`.
- `world`: `NetSimulation` for the real `drillgame_simulation::WorldState` (`WorldOp` op log:
  terrain `Tile` writes and per-player `Discovery`; 16x16 terrain chunks).
- `host`: `HostSession`, the only authority. Fixed-tick accumulator (never advances on packet
  arrival), input buffering by target tick, held/neutral input, late actions applied once,
  local seats (single-player and split-screen are a host with no remote clients), lobby,
  reconnect reservations, epochs, and per-client `ServerFrame`s at each client's snapshot rate.
- `client`: `ClientSession`: authority + predicted worlds, pending inputs, reconciliation and
  smoothing, clock sync (`clock`), remote interpolation (`interpolation`), event de-duplication.
- `protocol`: `ClientMessage` / `ServerMessage` and the versioned postcard codec
  (`PROTOCOL_VERSION` 3).
- `params`: `SessionParams` (tick rate) and live `NetTuning`.
- `link`, `harness` (feature `harness`), `toy`: deterministic lossy link, the end-to-end
  harness with scripted bots, and a small toy simulation for unit tests.

## Per-client replication

Each frame is built for one client from its `Relevance` (its player, position and
`relevance_radius`, infinite by default):

- per-tick state of relevant players and entities, low-rate markers for the rest;
- op-log entries the client may see (`op_relevant`, e.g. only its own discovery) and, with a
  finite radius, only in relevant chunks (`op_chunk` / `chunk_relevant`); chunks whose ops were
  skipped are sent whole (`chunk_snapshot`) in `chunk_refreshes` once relevant again;
- on-change sections whose hash changed since that client's baseline, limited to sections it
  may see (`section_visible`), so another player's loadout never reaches it;
- keyframes (`Welcome`, `Keyframe`) from `keyframe_for(viewer)`, redacted the same way.

Replica checks use `visible_state_hash(viewer)`: the host puts it in `debug_hash` every
`debug_hash_interval` frames (infinite relevance only) and reports it per client per tick when
`record_tick_hashes` is set; the client compares it with its authority.

## Tests

`cargo test -p drillgame_netcode`: component tests, toy and real-world end-to-end harness runs
(0 ms and lossy links, 30/60/120 Hz, 2/4/16 clients, join in progress, reconnect, epochs,
private state reaching only its owner), and terrain relevance convergence with a finite radius.
