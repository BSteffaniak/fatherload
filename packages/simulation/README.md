# drillgame_simulation

Canonical, deterministic Drillgame gameplay simulation: `WorldState`, its single fixed-tick
transition, terrain, rigs, drilling, hazards, economy, contracts, expeditions, rescue,
deterministic RNG, map discovery, replication sections and state hashing.

No IO, clocks, rendering, async runtime or networking dependencies (`serde` only).

## The transition

`WorldState::simulate_tick(tick, &TickInputs) -> TickOutput` is the only gameplay transition.
Every other mutator is `pub(crate)` and runs inside it. Outside this crate it is called only by
`drillgame_netcode`'s `NetSimulation` impl (host ticks and client prediction) and by tests.

- `TickInputs`: per player, held `ControlState` (quantized horizontal axis, thrust, drill) and
  one-shot `TickAction`s (`PlayerCommand`: bomb, scanner, infrastructure, rescue and every town
  service transaction, validated against the acting rig's building).
- `TickOutput`: gameplay events with stable `(tick, ordinal)` ids, one `ActionOutcome` per
  action, the tick's terrain tile writes (`terrain_ops`) and newly discovered tiles per player
  (`discoveries`).

The tick rate is a session parameter (`SimulationParams`, default 60 Hz); gameplay constants
are authored per second and derived per tick once at world creation (`TickTiming`).

## World modules (`src/world/`)

| Module | Owns |
|---|---|
| `simulate` | the tick: inputs, hazards, bombs, drilling, day cycle, escape, deep rewards |
| `movement`, `commands`, `services`, `progression`, `hazards`, `rescue` | gameplay rules used by the tick |
| `roster` | `add_player` / `remove_player`; spawn placement on an unbounded grid so any number of rigs spawn without overlap |
| `discovery` | `DiscoveredTiles` (16x16 chunk bitsets; positions in JSON saves, bitsets in binary keyframes), clamped allocation-free reveals, `Discovery` op-log entries |
| `player_sections` | how a `Player` splits into per-tick `PlayerRig`, public `RigBuild` and private `PlayerInventory` |
| `replication` | per-tick state, live entities, op log and on-change sections (`SectionKey` / `SectionPayload`) |
| `visibility` | what one client may see: `SectionKey::visible_to`, `redacted_for`, `visible_state_hash` |
| `state_hash` | the stable world hash (`simulation_state_hash`) and the client-visible variant |
| `import`, `timing` | building worlds (new game, legacy imports) and per-tick constants |

## Replication

Every field belongs to exactly one section kind (the compiler enforces it through exhaustive
destructuring in `section`, `remove_player`, `redact_player` and `split_player`):

| Kind | Contents | Who receives it |
|---|---|---|
| Per tick | rig pose, velocity, fuel, hull, drill/scanner timers, deep-run pressure; gas clouds, boulders, bombs | clients the rig/entity is relevant to |
| Op log | terrain tile writes | everyone, filtered by 16x16 terrain chunk relevance (stale chunks are refreshed whole with `terrain_chunk`) |
| Op log | per-player discovery | the owner only |
| On change | `RigBuild(player)`: upgrade levels drawn on the rig | everyone |
| On change | `Loadout(player)`: inventory, credits, upgrades, kits, rig parts, salvage, rescue count, deep reward milestone, service transactions | the owner only |
| On change | economy, contracts, progression (incl. `won_game`), town, world events, infrastructure, markers, RNG, timers, session constants | everyone |

Because other players' private state never reaches a client, replica checks compare
`visible_state_hash(viewer)`: private state is hashed only for the viewer and per-player data is
hashed player by player, so host and a correct client authority agree exactly. Terrain
dimensions, seed and initial tiles travel only in keyframes (`redacted_for(viewer)`).
