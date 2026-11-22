# fatherload_netcode

Sans-IO multiplayer netcode for Fatherload (see `multiplayer-architecture-plan.md`, sections
5-8, 11.2, 15 and 16): server-authoritative state replication, client-side prediction of the
local rig and interpolation of remote rigs.

No sockets, threads, async runtimes or wall clock: time is always passed in as a `Duration`.
Bytes move through `fatherload_quic_transport` in the app and through `LinkSim` in tests.

## Modules

- `simulation`: the `NetSimulation` trait sessions are generic over, plus `Relevance`.
  `NetSimulation::host_only_action` names session settings only the host's own seats may
  change (the world: `SetEnemyDifficulty`); the host acknowledges such an action from a remote
  client but never applies it (`HostClientStats::host_only_refused`).
- `world`: `NetSimulation` for the real `fatherload_simulation::WorldState` (`WorldOp` op log:
  terrain `Tile` writes and per-player `Discovery`; 16x16 terrain chunks). The keyframe is the
  world itself, whose terrain serializes as generation settings plus the tiles that differ
  from generation: the client regenerates the rest and checks the terrain digest, so a
  keyframe from an unknown or different generator is a codec error rather than a desync. A
  chunk snapshot is the chunk's `TileDiffs` the same way.
- `host`: `HostSession`, the only authority. Fixed-tick accumulator (never advances on packet
  arrival), input buffering by target tick, held/neutral input, late actions applied once,
  local seats (single-player and split-screen are a host with no remote clients), lobby,
  reconnect reservations, epochs, and per-client `ServerFrame`s at each client's snapshot rate.
- `client`: `ClientSession`: authority + predicted worlds, pending inputs, reconciliation and
  smoothing, clock sync (`clock`), remote interpolation (`interpolation`), event de-duplication.
- `protocol`: `ClientMessage` / `ServerMessage` and the versioned postcard codec
  (`PROTOCOL_VERSION` 8).
- `params`: `SessionParams` (tick rate) and live `NetTuning`.
- `link`, `harness` (feature `harness`), `toy`: deterministic lossy link, the end-to-end
  harness with scripted bots, and a small toy simulation for unit tests.

## Per-client replication

**Positions far from the origin (F7, wave 7).** A `Pose` is `origin + (x, y)`: the
simulation's frame origin in tiles (`world::frame_origin`) plus the frame-local position, so
`x`/`y` stay small and exact anywhere. `Pose::delta` subtracts origins exactly first;
`distance`, `lerp`, smoothing and the snapshot buffer's extrapolation go through it, so a frame
move between ticks never shows as a correction. `Relevance::origin` is the viewer pose's origin:
`contains_pose` rebases a pose, and the world's chunk relevance measures chunk boxes from it, so
relevance holds when players are far from the origin and from each other. A client applies the
per-tick live section (`S::Entities`, which carries the world's frame) **before** player states
and entity records, so a frame move shifts the replica first. `PROTOCOL_VERSION` 15.

Each frame is built for one client from its `Relevance` (its player, position and
`relevance_radius`, infinite by default):

- per-tick state of relevant players and per-tick entities (bombs, id counters), low-rate
  markers for the rest;
- incremental entities (`entity_revisions` / `entity_record` / `apply_entity_delta`): the host
  keeps each client's `(key, revision)` baseline and sends only the records of relevant
  entities that are new or changed, plus the keys of entities that are gone or left relevance
  (`entity_updates`, `entity_despawns`); a keyframe resets the baseline to every entity, so
  resting (sleeping) entities cost nothing per frame;
- op-log entries the client may see (`op_relevant`, e.g. only its own discovery) and, with a
  finite radius, only in relevant chunks (`op_chunk` / `chunk_relevant`); chunks whose ops were
  skipped are refreshed (`chunk_snapshot`) in `chunk_refreshes` once relevant again;
- on-change sections whose hash changed since that client's baseline, limited to sections it
  may see (`section_visible`), so another player's loadout never reaches it;
- events of relevant players that the client may see (`event_visible`, e.g. only its own action
  results);
- keyframes (`Welcome`, `Keyframe`) from `keyframe_for(viewer)`, redacted the same way.

Real-world keyframe sizes (postcard, before = every tile, after = diffs from generation):
fresh standard world 113 KB -> 304 B, heavily dug standard world (6,890 changed tiles)
112 KB -> 11.6 KB, fresh Large world 170 KB -> 305 B, heavily dug Large world (10,202 changed
tiles) 168 KB -> 16.7 KB. A refresh of a dug chunk is ~30-100 B instead of ~540 B, and 1 B for
an untouched chunk (`tests/world_keyframes.rs` prints the report).

Replica checks use `visible_state_hash(viewer)`: the host puts it in `debug_hash` every
`debug_hash_interval` frames (infinite relevance only) and reports it per client per tick when
`record_tick_hashes` is set; the client compares it with its authority.

## Tests

Wave 6 QA added `world_pilots_on_the_moon_and_in_the_high_sky_predict_and_converge`: one
crewmate on foot on the Moon's pad (low gravity) and one under the Moon's underside (the
highest open sky) under a lossy, jittery link; it checks convergence, bounded queues and that
the pilots really climbed out and jumped.

`cargo test -p fatherload_netcode`: component tests, toy and real-world end-to-end harness runs
(0 ms and lossy links, 30/60/120 Hz, 2/4/16 clients, join in progress, reconnect, epochs,
private state reaching only its owner), and terrain relevance convergence with a finite radius.
