# fatherload_simulation

Canonical, deterministic Fatherload gameplay simulation: `WorldState`, its single fixed-tick
transition, terrain, rigs, drilling, hazards, economy, contracts, expeditions, rescue,
deterministic RNG, map discovery, replication sections and state hashing.

No IO, clocks, rendering, async runtime or networking dependencies (`serde` only).

## The transition

`WorldState::simulate_tick(tick, &TickInputs) -> TickOutput` is the only gameplay transition.
Every other mutator is `pub(crate)` and runs inside it. Outside this crate it is called only by
`fatherload_netcode`'s `NetSimulation` impl (host ticks and client prediction) and by tests.

- `TickInputs`: per player, held `ControlState` (quantized horizontal axis, thrust, drill) and
  one-shot `TickAction`s (`PlayerCommand`: bomb, scanner, infrastructure, rescue, test-mode
  toggles and warps, and every town service transaction, validated against the acting rig's
  building).

Drilling (`world/commands.rs`): the rig drills down or sideways only while resting on solid
ground (`DrillContact`). The drill mount upgrade (`Player::drill_mount_level`, in the public
`RigBuild`) adds sideways drilling in flight (tier 1, gimbal) and drilling up into a ceiling the
rig thrusts against (tier 2, crown bore, `DrillDirection::Up`). The shop gates both tiers on the
world's deepest tile reaching a band (`economy::upgrade_locked_by`). Drill pace: a drill tier
sets the chip rate (`player::drill_speed`, floored at `MIN_DRILL_SECONDS_PER_CHIP`, about three
ticks at 60 Hz) and, from tier 6, the durability each chip takes off (`player::drill_bite`, up to
5), so a maxed drill clears hard rock in about 0.15 s at every tick rate. Lava and magma vents
are drillable (they turn to air and burn the rig, see hazard counters); building foundations
and vault fixtures are not, except in test mode. `terrain::is_indestructible` names tiles no
drill can ever dig (the place for a bedrock floor); every drill path checks it.

Upgrades (`economy.rs`): fourteen shop lines, offer index = position in `UpgradeKind::ALL` (the
drill mount stays at `DRILL_MOUNT_OFFER_INDEX` 8). The core lines (drill, fuel tank, cargo bay,
engine, hull, radiator) run to tier 15, the scanner to 10, each hazard counter to 6-8. Prices
grow geometrically per tier from a cheap first purchase (`upgrade_cost`); stat curves live in
`player.rs` (`cargo_capacity_for_level` ~300 at the top, `fuel_capacity_for_level` ~3,000,
`hull_capacity_for_level` ~2,500, `engine_power` ~4x thrust with climb speed capped; horizontal
top speed follows `world::engine_top_speed_tiles`, ~5x stock at the top tier).
Capacities are stored, so a save from the old five-tier curves keeps its levels; loading it
(`WorldState::catch_up_capacity_curves`, run by the game's save loader) lifts the tank and bay
onto the new curve for the fitted tiers, keeping bonuses earned on top. Levels load whatever
their value.

Hazard counters (`protection.rs`, `Player::hazard_gear`, private loadout): Heat Shielding,
Lava Plating, Gas Scrubber, Pressure Bracing (pressure pockets, explosive pockets, magma vents)
and Boulder Deflector (boulders, cave-ins). Each stops `1 - (1 - tier/max)^2` of its hazard, all
of it at the top tier. Heat is per band, not per row: each band has a heat level
(`band_heat_level`, 0 in the topsoil and clay up to 10 in the mantle storm zone) and the rig's
cooling (`heat_cooling`, counted in thirds of a level: a whole level per radiator tier to
tier 5, a third per radiator tier after that, a whole level per shielding tier) covers that
much; each uncovered third burns `HEAT_DAMAGE_PER_EXCESS_UNIT` hull per second (3 per level). Deep-run instability builds from the
basalt deeps down and the pressure fuel drain/scanner lockout starts at the fossil oceans (both
by band). Boulders loosened by mining are one per 80 stone tiles with a 1.5 s warning,
rarer still for late drills (`boulder_spawn_chance`: odds fall with the square of the drill's
bite, so a top drill loosens no more boulders per minute than a tier 5 one).

Test mode (`test_mode.rs`, `world/test_mode.rs`): `WorldState::enable_test_mode` (before play)
maxes every rig (hazard counters included) and the player template, fills the parts locker and
grants credits. Test-mode rigs drill any tile but an indestructible one in a single chip,
ignoring hardness, on the tick the drill touches it (`GOD_MODE_SECONDS_PER_CHIP`), and drive
`GOD_MODE_SPEED_MULTIPLIER` (2.2x) faster than the top engine (thrust and climb cap
`GOD_MODE_THRUST_MULTIPLIER`, 1.5x); with the invulnerable toggle or no-clip flight on (`WorldState::damage_immune`) no source
damages a hull (and no damage events fire);
`PlayerCommand::TestMode` toggles invulnerable hulls / infinite fuel (refilled every tick) /
no-clip flight (`world/no_clip.rs`: no gravity, collision, damage or drilling; turning it off
frees rigs left in rock) or
warps the acting rig into a carved, hazard-free pocket at the top of a band. Depth records and
deep-reward milestones are skipped. The toggles replicate as the shared `SectionKey::TestMode`
section and save with the world.
- `TickOutput`: gameplay events with stable `(tick, ordinal)` ids, one `ActionOutcome` per
  action, the tick's terrain tile writes (`terrain_ops`) and newly discovered tiles per player
  (`discoveries`).

The tick rate is a session parameter (`SimulationParams`, default 60 Hz); gameplay constants
are authored per second and derived per tick once at world creation (`TickTiming`).

## World modules (`src/world/`)

| Module | Owns |
|---|---|
| `simulate` | the tick: inputs, entity phases, hazards, bombs, drilling, day cycle, escape, deep rewards |
| `drops` | WP-DROPS: drops from broken tiles (drill, blasts, fire, shattering boulders), the pickup pass (nearest rig with room, magnet pulls, "full" warnings), merging, per-cell and per-blast caps, lifetimes, ore shatter, dumping and selling blocks, the collect toggle |
| `entities` | the world side of the entity store: spawn helpers, the two entity phases and each kind's update (gas clouds, boulders, drops), blasts, waking on terrain changes, entity replication |
| `bombs` | loose-charge physics, chain reactions and every explosive kind (`crate::explosive`): sticking, remote/sequenced detonation, cluster bomblets, shaped tunnels, loot-safe mining blasts, incendiary rings, flare pulses, blast pushes on rigs and blast-feeling entities (`push_from_blasts`) and the throw preview |
| `movement`, `commands`, `services`, `progression`, `hazards`, `rescue` | gameplay rules used by the tick |
| `quests` | quest services (accept, turn in, abandon at the giver's counter), objectives stepping inside the tick, twists, rewards, reputation rebates, blueprint fragments |
| `roster` | `add_player` / `remove_player`; spawn placement on an unbounded grid so any number of rigs spawn without overlap |
| `discovery` | `DiscoveredTiles` (16x16 chunk bitsets; positions in JSON saves, bitsets in binary keyframes), clamped allocation-free reveals, `Discovery` op-log entries |
| `player_sections` | how a `Player` splits into per-tick `PlayerRig`, public `RigBuild` (visible tiers and cosmetics) and private `PlayerInventory` |
| `garage` | the paint bay service (`SessionServiceRequest::CustomizeRig`, validated against the player's cosmetic unlocks; free) and `rig_cosmetics` |
| `replication` | per-tick state, live entities, op log and on-change sections (`SectionKey` / `SectionPayload`) |
| `visibility` | what one client may see: `SectionKey::visible_to`, `redacted_for`, `visible_state_hash` |
| `state_hash` | the stable world hash (`simulation_state_hash`) and the client-visible variant |
| `import`, `timing` | building worlds (new game, legacy imports) and per-tick constants |
| `test_mode` | test-mode setup, per-tick refills and the band warp |

## World generation (`src/worldgen/`)

A world is a pure function of its `WorldSettings` (seed, `WorldSize`, `ResourceRichness`,
`HazardIntensity`, `GeneratorVersion`). The settings live in the `Terrain` (saved, hashed, and
carried in the session section); `Terrain::generate` builds a world and `Terrain::band_at` /
`deep_stratum_at` answer which layer a tile is in. Saves without settings are legacy worlds
(`GeneratorVersion::Legacy`, the original fixed-depth generator).

### Compact terrain (`terrain/persistence.rs`)

A `Terrain` serializes as a `TerrainSnapshot`: its `WorldSettings`, size, digest and only the
tiles that differ from freshly generated terrain (`TileDiffs`, row-major, written as runs of
`Tile::code`s). Saves (v6) and network keyframes both use it. Deserializing regenerates the base
(`Terrain::regenerate`), applies the diffs and checks the digest; a mismatch
(`TerrainRestoreError::DigestMismatch`: a snapshot from a different generator build, or
corrupted) and an unknown generator version are errors, never panics. Legacy-generator worlds
of any size (and old saves extended with `ensure_depth`) work the same way, since the legacy
generator is deterministic too. Other APIs: `generation_diffs` / `from_generation_diffs`
(position-keyed diffs, applicable in any order and idempotent), `chunk_diffs` /
`apply_chunk_diffs` for relevance refreshes, and `FullTerrain`, the pre-v6 every-tile form kept
for save migration.

Changing what a generator version produces breaks its existing saves (the digest check refuses
them): ship generator changes as a new `GeneratorVersion`.

The generated base is memoized per process (a few bases keyed by settings and size): a host, its
clients and tests rebuild the same world many times. Whole-world generation is kept, not lazy
per-chunk generation: caves, hazards and deposits are placed per region across chunk borders,
so a chunk cannot be generated alone cheaply, and a whole world is fast enough. Measured with
`generation_timing_report` (`cargo test -p fatherload_simulation generation_timing_report --
--ignored --nocapture`):

| World | Debug generate | Release generate | Restore with cached base (debug / release) |
|---|---|---|---|
| V4 standard (240x860) | 489 ms | 44 ms | 6.4 ms / 1.1 ms |
| V4 large (360x900) | 1086 ms | 68 ms | 9.5 ms / 1.6 ms |
| V3 standard (240x220) | 122 ms | 11.1 ms | 2.1 ms / 0.3 ms |
| Legacy (240x220) | 14 ms | 1.4 ms | 2.2 ms / 0.5 ms |

A compact snapshot of a lightly dug world stays around 6 KB whatever the world's depth.

Generation is integer-only (fixed-point value noise, splitmix hashing, region-seeded RNG) so a
seed is bit-identical everywhere:

| Module | Owns |
|---|---|
| `strata` | the band stack (`Band`): fixed order, per-seed boundary bases (V1/V2: small shifts; V3: a cumulative thickness model with minimum gaps; V4: the same model for every band of a world about four times deeper, `WorldSize::deep_height_tiles`), waves along x and interleaved fingers (V4: wide transition zones of lenses), the V4 bedrock floor (`bedrock_top_row`), `band_layout_rows` for placement passes, and `nominal_row` / `scaled_row` / `nominal_depth` / `scaled_span`, which map a world's rows onto the classic ~220-row stack so depth rules authored against it (mineral curves, hazard gates, heat, deep-run pressure, deep rewards, quest depths) stretch with deeper worlds |
| `caves` | smoothed noise caverns, worm tunnels and deep chambers; crust, town and spawn-shaft protection |
| `hazards` | clustered hazard pockets by band rules, scaled by intensity; clearance around spawn and town |
| `deposits` | per-mineral depth curves (classic nominal rows), veins / pods / motherlodes, per-band ore budgets, the starter field, artifact beds (V4: more per band, spread through it), prefab slot reservation (V1); V3+ rich worlds thin deep ore specks into fewer, chunkier deposits |
| `prefabs` | hand-authored POI rooms as char grids with a shared legend (size limits, random fills, switch slots) |
| `pois` | V2+: POI placement in a per-band region grid (guaranteed kinds, random fills, depth windows, spacing, hazard clearance, spawn/town exclusion; V4 stacks a row of regions per tier of about 22 rows, fills more of them at random, and adds an extra supply cache per tier, keeping the classic POI density per row) and stamping (flips, shuffled lock orders, band ore/artifacts/guardian hazards); empty regions become reserved prefab slots |
| `features` | `FeatureIndex`: caverns, chambers, motherlodes, artifact beds, hazard clusters, stratum transitions, prefab slots, points of interest and (V6) villages, queryable by kind, band, depth, region or proximity, plus POI queries (`pois`, `pois_of(kind)`, `pois_in_band`, `poi_at(position)`, `nearest_poi(position, kind)`) for quests to bind to (`WorldState::world_features`, recomputed from settings and cached per settings, never replicated) |
| `report` | `WorldStats` and the distribution report behind `--worldgen-report` |
| `noise` | integer value / fractal / ridge noise and `RegionRng` |

The fairness suite (`worldgen/tests.rs`) checks every sampled seed for a fixed surface and
town, caves and hazards clear of spawn and the town, clustered hazards, a starter field worth
the first upgrades, per-band ore value within target, ore never harder than its band's drill, a
path to the bottom with each band's own drill, artifacts in every stratum and feature counts in
bounds, V3 and V4 band thicknesses within bounds and varying by seed, V4 transition zones that
interleave over many rows, a solid V4 bedrock floor that nothing breaks, few deep ore specks on
rich V3+ worlds, plus golden digests pinned to `GeneratorVersion::V1`, `V2`, `V3` and `V4`. `worldgen/poi_tests.rs`
checks POI counts per band and per kind, caches in the clay, grey stone and slate, spacing,
spawn/town/hazard clearance, nothing undiggable below spawn, and that every vault's doors,
switches, relays and seals can be reached and worked from spawn. `FATHERLOAD_WORLDGEN_SEEDS=300
cargo test --release -p fatherload_simulation worldgen` runs it over more seeds.

## Cave ecosystems (`src/cave.rs`, `src/worldgen/streamed/cave_biomes.rs`, `src/world/caves.rs`)

Expansion plan WP-CAVES, generator V6 (`GeneratorVersion::has_cave_biomes`; V1-V5 are
unchanged, their golden digests still pass, and a V6 golden digest pins the new output).

**Generation.** The plane is split into `CAVE_REGION` (96x64) cells; a cell holds at most one cave,
chosen by its band (`kind_weights`: no geodes or ruins above the grey stone, no lava tubes above the
basalt, deep strata favour their own kinds) from the region's RNG. A cave reads only the base layer
and the POI layer (it skips regions whose footprint would come within 4 tiles of a POI, the spawn
shaft or the starter field) and its whole footprint stays inside its region, so chunks consult only
the regions they overlap and stay seamless and order-independent. A cave is a raster of
`BiomeCell`s: `Open`/`Water` interior, `Wall` (plain rock that worm tunnels and caverns may still
open: natural entrances), `Shell(kind)` (dressed walls that also yield) and `Solid(kind)` (decor,
loot and hazards that always stand). The cave layer (`Window`) consults the rasters, so hazards,
POIs and deposits work around caves. Every interior is one connected area (smaller pieces are
walled up), the feature centre is an open tile, nothing in a cave is undrillable or harder than its
band's drill (`CaveTile::hardness`).

| Kind | Shape and dressing | Loot | Light / critters |
|---|---|---|---|
| Grand cavern | wide flat-floored hall, stalagmites, stalactites | cache 80 %, gem vein 70 % | dark / bats, fireflies |
| Crystal geode | round hollow, crystal shell and spikes | gems in the shell, cache 35 % | glowing / glow moths |
| Flooded cave | lake under a water line, silt bed, glow algae | sunken cache 75 %, vein 40 % | dim / cave fish, fireflies |
| Lava tube | meandering obsidian-lined tube, lava pools on the floor | vein 55 %, cache 30 % | glowing / ember moths |
| Fungal grotto | mycelium, giant mushrooms (bouncy caps), glowcaps, spore gas | cache 40 % | glowing / fireflies, moths |
| Worm burrows | 2-3 winding tunnels from a bulb, bone ribs | cache 50 % | dark / bats |
| Ancient ruins | brick hall, pillars, glyphs, rubble, corridor out | relic cache (always) | dim / bats |
| Chasm | tall wandering crack, ledges, stalactites, bottom chamber | cache 60 %, vein 60 % | dark / bats |
| Cave network | 3-5 chambers joined by tunnels | cache 35 %, vein 50 % | dark / fireflies, bats |

**Reading caves (for other tracks).** `Terrain::cave_at(tile) -> Option<CaveSample>` (kind,
feature id, interior, water), `Terrain::cave_kind_at(tile)`, `Terrain::water_at(tile)` and
`Terrain::caves_touching(rect)` (the rasters). `CaveKind::spawn_tags()` is what WP-ENEMIES reads
(`nests`, `swarmers`, `aquatic`, `heat`, `machines`, `burrowers`, `elite_lair`,
`danger_per_mille`), e.g. `terrain.cave_kind_at(tile).map(CaveKind::spawn_tags)`;
`CaveKind::light()` says whether a cave is dark. Caves are regional features
(`FeatureKind::Cave`, `FeatureDetail::Cave(kind)`, id layer 8), so quests and stories bind to them
through `features_in` / `nearest_feature`: a quest target `TargetSpec::Feature { kind:
FeatureKind::Cave, .. }` binds to a cave's open centre (falling back to a large cavern on worlds
without caves), and caves are `WorldFactKind::CaveSystem` story facts (`WorldFact::near`).

**Gameplay (`world/caves.rs`, all in the tick).** Entering a cave's interior records it in the
player's `PoiJournal::caves` and fires `CaveDiscovered`. A `CaveTile::Stalactite` with open air
between it and a rig up to 7 rows below breaks off (`StalactiteFell`) as a falling boulder (fast,
0.45 s warning; the boulder deflector counters it). Drilling a `TreasureCache` / `RelicCache` pays
`cave::cache_loot` (credits by depth tier, plus gems or a relic) and spills the items as drops via
`spawn_item_drop` (WP-DROPS collects them) with `CaveCacheOpened`. Movement: in cave water a rig is
buoyed (80 % of gravity) and dragged (`1 / (1 + 3 dt)` per tick); landing on a `MushroomCap` faster
than 120 px/s bounces it back at 85 % (at least 300 px/s) with no landing damage.

**Critters** are `EntityKind::Critter` entities (`CritterState`: kind, cave, slot, roam, phase).
Every 500 ms `advance_critters` fills the missing slots (one per 30 interior tiles, 3-8) of caves
within 40x28 tiles of a rig at deterministic homes with open space around them (fish only in
water) and despawns critters 64+ tiles from every rig (`despawn_many`). A critter's body never
moves and its record never changes after spawning (it replicates once); presentation animates it
with `CritterState::offset_at(seconds)`. Harmless: no collision, damage or blast push.

Tests: `worldgen/streamed/cave_tests.rs` (every kind on every seed, density, connected and
reachable interiors, clearances, loot bounds, still water, order independence and seamless windows,
V5 without caves, ids, the V6 golden digest; `FATHERLOAD_WORLDGEN_SEEDS` for more seeds) and
`world/cave_tests.rs` (discovery, stalactites, caches, critters, water, mushroom bounce).

## Points of interest (`src/poi.rs`, `src/world/pois.rs`)

Generator v2 worlds hold supply caches, derelict rigs, vaults (sequence switches, pressure
seals, twin relays), formations (fossil beds, geodes) and rival claims (`PoiKind`,
`PoiVariant`). They are built from `TileKind::Poi(PoiTile)` fixtures: crates, wreckage and
salvage cores, vault walls/doors/seals/switches/relays/caches (vault fixtures are undrillable
and blast-proof; a seal vents only under a bomb), fossil shale, geode shells, outpost plating
and lockers. A POI's state lives in its tiles (opened crates are gone, switches light, doors
vanish), so it replicates through the terrain op log and saves with the terrain; the twin-relay
deadlines ride in the timers section and each player's `PoiJournal` (known sites, recovered
logs) in their private loadout.

Drilling a crate, salvage core, vault cache or outpost locker pays a deterministic reward from
the world seed, the tile and its depth tier (tables in `poi.rs`; tuned as a top-up next to ore
sales); a vault hands out an unowned rare part first, then a fragment of the opener's next
vault blueprint, and a permanent fitting once those blueprints are all finished. Entering a POI
records a visit; a scanner pulse records the POIs in range as echoes (tiers 1-2) or identified
silhouettes (tier 3+, or the relic scanner). Events: `PoiOpened`, `PoiDiscovered`,
`VaultOpened`, `VaultLockReset`.

## Side quests (`src/quest.rs`, `src/quest/`, `src/world/quests.rs`)

Plan §8's quest engine. Quests are data (`quest/content.rs`): a `QuestDef` names its giver
(`QuestGiver`: the keepers Kade, Mara, Nix, Iona and Pip, plus Director Vale's HQ board), tier,
hook (why it is fun), `Prerequisite`s (depth reached, a POI kind visited, logs recovered,
reputation, other quests, upgrade levels), `TargetSpec`s to bind, stages of `Objective`
primitives, `Twist`s, a `QuestReward` and authored offer/progress/complete lines. Ids are
stable and saved (Kade 1xx, Mara 2xx, Nix 3xx, Iona 4xx, Pip 5xx, board templates 9xx).

| Primitive | Done when |
|---|---|
| `Deliver` | the owner carries the goods (handed over at turn-in; last stage only) |
| `Reach`, `ReachDepth`, `ReturnToSurface` | a rig gets there |
| `Dig` | rigs drill N tiles in and around the target |
| `Scan` | a scanner pulse near the target |
| `OpenPoi` | the target vault's lock opens, or a crate/core/cache/locker inside it is drilled |
| `Survive` | rigs spend N seconds in and around the target (a hazard pocket) |
| `PickUp` | the owner collects the quest parcel (the fragile twist then counts damage) |
| `Build` | infrastructure of a kind stands at the target |
| `Detonate` | bombs go off at the marked spots in order (a wrong spot resets) |
| `RecoverLog` | the target derelict's salvage core is drilled |
| `Simultaneous` (co-op) | rigs on every marked spot within a crew window, stretched toward the solo one when the crew has fewer rigs than spots (solo: one rig, a long window) |
| `Relay` (co-op) | a marker reaches the target; it overheats after N rows and must be handed to another rig, which can't hand it straight back (solo: it drops and cools) |
| `HoldAndClimb` (co-op) | a rig reaches the goal while the switch is held, anchored by infrastructure, or (solo) still latched |
| `Escort` (co-op) | a slow haul crawls through open tunnel to the surface while rigs walk beside it (faster with two) |
| `SplitDelivery` (co-op) | goods dropped at two buildings within a window (solo: a longer one; a stale half spoils) |

Twists: `TimeLimit`, `Fragile` (hull damage cap), `NoScanner`, `NoBombs`, `StayDown`. A failed
quest is dropped and counted; the giver offers it again.

- **Binding** (`quest::binding`): accepting binds each target to a real feature or POI through
  the feature index (unvisited first, then nearest; places other active quests use are
  avoided) and lays out marked spots; same world, player and quest bind the same places. A
  feature target with nothing in its bands falls back to the nearest bands that have one, then
  to a stand-in kind (an artifact bed for a motherlode, a cavern for a chamber or a cave); a row of marks
  that doesn't fit slides inward or stands as a column.
  Board postings (`board_offers`: three a day from the seed and the market day) carry a twist
  pick and a binding salt so they spread over different places.
- **State**: a player's `QuestLog` (active solo quests, completions, failures, reputation,
  blueprint fragments, deepest row, visited POI kinds, the board ledger) is part of `Player` and
  rides in the private `Loadout` section; quests accepted in crew mode (co-op quests with other
  rigs present) live in `CrewQuests` in the shared `Quests` section so every rig sees their
  markers. Everything saves with the world (serde defaults keep older saves loading) and feeds
  the state hash.
- **Rules**: `AcceptQuest`, `TurnInQuest` and `AbandonQuest` are town services validated at the
  giver's counter; objectives advance near the end of each tick from what the tick did (tiles
  mined, blasts, scanner pulses, vault openings, rig positions, infrastructure). Durations are
  authored in milliseconds, so quests behave the same at any tick rate. Crew helpers get a
  quarter of the credits and half the reputation.
- **Reputation** ranks (`REPUTATION_RANKS`: 10/25/50/90) unlock deeper chains and give rebates:
  Pip on fuel, Iona on repairs, Nix on charges (4 % a rank), Kade on cargo sales (+2 % a rank).
- **Blueprints**: `BlueprintId`s collect fragments from quests and vaults; a finished blueprint
  unlocks a rig part (Hauler Engine, Cargo Balloon, Pressure Hull) or records a recipe for later
  systems (dampener, tow winch, outpost refinery).
- **Reading** (`quest::describe`): objective lines, twists, time left and map/world markers.
- **Events**: `QuestAccepted`, `QuestStageDone`, `QuestReady`, `QuestCompleted` (the fanfare),
  `QuestFailed`, `BlueprintCompleted`.

The content suite (`quest/content_tests.rs`) checks that every reference resolves, the unlock
graph is acyclic and fully reachable from a fresh game, co-op primitives have solo variants,
rewards fit their tier's bounds (`QuestTier::reward_bounds`), no quest uses only basic
primitives without a twist or co-op, the first batch keeps its shape (25-35 quests, 4+ givers,
8+ co-op) and every quest binds on 50 seeds, every world option on one seed and seeds that
once failed (`FATHERLOAD_QUEST_SEEDS` for more seeds).
`world/quest_tests.rs` plays quests through the tick: solo and crew completions, twists,
rewards, the board, rebates, replication, saves and determinism.

## Entities, physics and items (`src/entity.rs`, `src/physics.rs`, `src/item.rs`, `src/world/entities.rs`)

Expansion plan §3 F1. Wave 2 tracks (drops, enemies, projectiles, NPCs, structures) build on
this API.

### The entity store

Every live world object that moves or expires on its own is an `entity::Entity` in the world's
`EntityStore` (`WorldState::entities()`): a stable `EntityId` (never reused), a `Body`
(position, velocity, previous position for interpolation, rest counter, `asleep`), a spawn tick,
an optional expiry tick, a replication `revision` and per-kind `EntityState`. Today's kinds:
`GasCloud`, `Boulder` (both migrated from ad-hoc world lists without behaviour changes) and
`Drop` (a floating `ItemStack` with magnet / dump / "blocked" marks; see "Drops and pickups"
below).

- **Order.** Entities are kept in id order and every pass over them is in id order.
- **Derived indices** (never saved, hashed or replicated; rebuilt on load): a spatial hash on
  the terrain chunk grid (`SpatialCell`, 16x16 tiles; `ids_in_rect`, `ids_in_radius`), the
  awake set, per-kind sets (`ids_of`, `of_kind`, `count_of`) and an expiry queue
  (`take_expired`).
- **Mutation** goes through `EntityStore::update(id, |entity| ...)`, which assigns a fresh
  revision and keeps every index in step; `spawn` and `despawn` likewise. Removing many
  entities at once (a magnet sweep, a merge pass, a mass expiry) should use
  `despawn_many(&ids)`: one compacting pass instead of one shift of the id-ordered store per
  entity (`take_expired` and replicated despawns already do). Inside the crate,
  `WorldState::spawn_entity`, `spawn_gas_cloud`, `spawn_boulder` and the public
  `spawn_item_drop` are the spawn helpers.
- **Tick.** `simulate_tick` runs `EntityPhase::Early` right after inputs (gas clouds age before
  hazard damage reads them) and `EntityPhase::Late` after drilling, bombs and hazard spawns
  (expiries first, then boulders and drops). Only awake entities update. Every explosive's blast
  pushes blast-feeling kinds (`EntityKind::feels_blasts`) inside the bomb pass
  (`push_from_blasts`, with `physics::Blast::explosive` for the kind's reach and push, on the
  same pre-blast terrain as rigs). At the end of the
  tick every changed tile wakes the sleeping entities within 1.5 tiles, so a drop on a dug-out
  tile falls on the next tick.
- **Sleep.** A sleep-capable kind (`EntityKind::can_sleep`) that rests (motionless and
  supported) for `ENTITY_SLEEP_AFTER_MILLIS` (300 ms, tick-rate independent) falls asleep:
  it is skipped by the tick and, since its revision stops changing, by replication.

Adding a kind: add an `EntityState` / `EntityKind` variant with its phase, sleep and blast
rules, add its update to `WorldState::advance_entities`, and a spawn helper. Replication,
saves, keyframes, hashing, relevance and the spatial index cover it automatically.

Bombs and every other explosive charge still live in their own list (`world/bombs.rs`,
`PlacedBomb` with a `kind`); the plan moves them onto the store in wave 2. `physics::step_body` with
`BodyParams::BOMB` already reproduces their motion bit for bit (checked against live bombs by
`shared_body_physics_reproduces_bomb_motion_exactly`), so that move is a storage change.

### Physics helpers (`physics`)

Pure functions, no RNG or clocks: `solid_at` (open sky, walled bottom, and walled sides at the
world's first and last column: column 0 and the width for bounded worlds, far beyond reach for
endless ones), `collides`
(a square body sampled at corners, edges and centre), `supported`, `step_body` (gravity,
axis-separated swept substeps of at most two pixels, bounce, floor friction, rest snapping,
lift-out of an overlapping spawn, quantization; per-kind `BodyParams`, e.g. `BOMB`, `DROP`),
`settle` (rest counting and sleep), `line_clear`, `Blast` / `apply_blast` (the bomb chain kick
generalized; `Blast::explosive` for any explosive kind: linear falloff, line of sight, upward lift, deterministic split of coincident
bodies), `steer_toward` (accelerate toward a point with a speed cap) and `find_tile_path` (a
budgeted, deterministic 4-connected BFS over open tiles for creature AI).

### Replication of entities

Entities replicate per client as deltas: the host keeps each client's `(EntityId, revision)`
list and every frame sends the records of relevant entities that are new or changed, and the
ids of entities that are gone or left relevance (`WorldState::entity_revisions(center,
radius)`, `entity_record`, `apply_entity_delta`; netcode's `NetSimulation::entity_revisions` /
`entity_record` / `apply_entity_delta`). The id and revision counters ride in `LiveEntities`.
Keyframes and saves carry the whole store. For presentation, `Body::lerp_position(alpha)`
interpolates between ticks and `EntityStore::interpolated_positions(previous, alpha)` between
two replicated snapshots.

### Items and inventory (`item`)

`ItemKind` covers minerals, artifacts, strategic materials, building blocks (`BlockKind`:
dirt, clay, stone, hard rock; `BlockKind::from_tile`, `ItemKind::from_tile`), modules,
weapons, ammo, explosives, cosmetics and quest items (the open-ended categories are small
stable id newtypes, `ModuleId`, `WeaponId`, ..., whose tables belong to the tracks that add
them). An `ItemContainer` holds a count per kind (kind order, no zero counts; serialized as a
list of `ItemStack`s) with typed views (`minerals()`, `artifacts()`, `materials()`,
`blocks()`, `mineral(kind)`, ...). Each kind names its container (`ItemKind::stowage`) and slot
cost (`ItemKind::slot_cost`): minerals, artifacts, modules and weapons take a slot; materials,
ammo, explosives, cosmetics and quest items take none.

A rig carries a `cargo_bay` (minerals, artifacts, materials, ...; capacity `cargo_capacity`
slots, so ore and artifacts count exactly as before) and a separate `block_hopper` for
building blocks (capacity `block_hopper_capacity`, 60 stock) so dirt never crowds ore out of
the bay. `Player::stow(kind, count)` routes an item to its container and takes only what fits
(`room_for`, `carried`). Lost-cargo sites hold an `ItemContainer` too. The game's save format 7
migrates the old per-rig `cargo` / `artifacts` / `materials` maps, lost-cargo maps and the old
gas cloud / boulder lists on load.

### Drops and pickups (WP-DROPS, `world/drops.rs`)

Every broken tile drops (`WorldState::spawn_tile_drop`, `ItemKind::from_tile`): the drill's
drop pops toward the rig, a blast's drops fountain out of the crater (`blast_spray_velocity`),
incendiary fire bakes the soft ground it burns into blocks and a boulder shattering on rock
leaves a stone block. Drilling semantics are unchanged: the drilled tile's drop lands inside
`DROP_COLLECT_RADIUS` (1.6 tiles), so the pickup pass at the end of the same tick stows it; a
full bay now leaves the ore floating instead of losing it.

- **Pickup pass** (end of the Late entity phase, host and predicting clients alike): each drop
  within a rig's magnet reach (`magnet_radius`: 3 tiles stock, +0.25 per cargo bay tier, line
  of sight to start a pull) goes to the nearest rig with room (ties: lower player id), which
  collects it inside the collect radius (`GameplayEventKind::ItemsCollected` with the drop's
  position) or pulls it in (`DropState::pulled_by`: free flight, `MAGNET_PULL_SPEED`). A drop in
  a rig's collect radius that does not fit is marked `blocked` and the rig hears
  `PickupBlocked { stowage }` once per stack that appears.
- **Stowage**: `Player::stow` routes ore, artifacts and materials to the cargo bay and blocks
  to the block hopper, whose capacity grows with the cargo bay (`block_hopper_capacity_for_level`:
  60 + 20 per tier). `Player::collect_blocks` (private loadout, `PlayerCommand::SetCollectBlocks`)
  makes the magnet ignore blocks. `PlayerCommand::DumpBlocks` tips blocks out as drops the
  dumping rig never auto-collects (`DropState::ignored_by`, 20 s lifetime);
  `SessionServiceRequest::SellBlocks` sells the hopper at the depot (`block_sale_value`: 1-4 cr).
- **Clutter**: a drop that comes to rest merges into a resting same-kind stack within
  `DROP_MERGE_RADIUS`; lifetimes are 45 s for blocks, 10 min for ore and materials, none for
  artifacts (`drop_lifetime_millis`); a 16x16-tile cell holds at most `DROPS_PER_CELL_CAP` (40)
  drops (a new one joins a same-kind stack, else the oldest same-kind pair merges, else the
  oldest block stack goes; ore is never deleted); one blast sprays at most
  `MAX_DROPS_PER_BLAST` (24) separate drops; resting drops sleep.
- **Blast loot**: `ore_survives_blast` shatters about one ore tile in three for plain blasts and
  one in six for mining charges (`BlastShape::LootSafe` now breaks ore instead of leaving it);
  artifacts always survive. `Terrain::blast_radius` reports what it broke (`BlastResult::broken`).

Economy recheck (`drop_economy_report`, release, 4 seeds, 90 s of a rig drilling straight down
or bomb-diving: a plain bomb every 3 s at 17 cr each, or a mining charge every 4 s, hoovering
the loot; base values, charge costs subtracted, per minute):

| Depth (nominal row) | Drill tier | Drill net | Bombs net | Mining charges net |
|---|---|---|---|---|
| 14 | 3 | 158 | -134 | -1,220 |
| 60 | 5 | 1,125 | 1,189 | 2,755 |
| 60 | 8 | 2,461 | 1,189 | 2,755 |
| 120 | 10 | 6,656 | 4,405 | 11,305 |
| 120 | 14 | 6,802 | 4,405 | 11,305 |

Plain bombs are tunnelling tools near the surface, a fast earner for an under-drilled rig in
the middle strata and behind a strong drill deep down. Mining charges, now the loot tool, were
repriced from 160 to 300 cr a pair so they pay off only where ore is rich (about 1.1x a tier 8
drill mid-depth, 1.7x deep, before the real overhead of placing, waiting and trips to the
surface). `bombing_stays_balanced_against_drilling` pins those ratios. (One mid-depth
bomb run frees a Star Core, which starts the escape and its cave-ins, just as drilling it would.) Block sales add at most
a few percent to drilling income.

### Performance

`entity_benchmark_report` (`cargo test -p fatherload_simulation entity_benchmark_report --
--ignored --nocapture`, add `--release`): a cave with 5,000 drops resting on ledges (asleep
after their first 300 ms), 300 drops kept bouncing by repeated blasts, 60 gas clouds and a
stream of boulders, about 5,365 entities with about 510 awake on average, 600 ticks at 60 Hz,
plus the host's per-client relevance scan (20-tile radius, about 2,900 entities in range):

| Profile | Tick without entities | Tick with entities (mean / worst) | Entity cost per tick | Per-client relevance scan | Full entity snapshot |
|---|---|---|---|---|---|
| Release | 1.4 µs | 91 µs / 1.5 ms | 89 µs | 106 µs | 199 KB |
| Debug | 22 µs | 4.5 ms / 149 ms | 4.5 ms | 5.2 ms | 199 KB |

The worst tick is the first one, when all 5,000 drops are still awake; steady state is the
mean. (Measured before WP-DROPS. With drop merging the cave's 5,000 piled drops collapse into
about 330 stacks within their first 300 ms, the merges spread over several ticks and found with
the allocation-free `EntityStore::nearest_in_radius`; the same benchmark on a heavily loaded
machine, where the entity-free tick ran 9x slower than above, measured a 355 µs mean and a
32 ms worst tick, the merge burst.) Sleeping drops cost an index entry and nothing per tick; the per-tick cost follows the
awake count. The browser build has no networking, so the relevance scan only runs on desktop
hosts (release). A snapshot is about 37 bytes per entity; WP-DROPS stack merging keeps counts
low. Removing half the store at once (2,683 entities) takes about 0.9 ms with `despawn_many`
versus about 18 ms one `despawn` at a time (release), so bulk removals should batch.

## Villages (`src/village.rs`, `src/village/`, `src/world/villages.rs`, `src/worldgen/streamed/villages.rs`)

Expansion plan WP-VILLAGE, on [`GeneratorVersion::V6`] worlds
(`GeneratorVersion::has_villages`, the single switch; V5 and older generate exactly as before
and the V5 golden digest is unchanged). A village is a pure function of `(WorldSettings, site)`:
nothing about it is saved or replicated except what players did there.

- **Placement** (`village::placement`, const and integer-only): surface site `n` (`n > 0` east,
  `n < 0` west, never 0) is centred `450 + (|n| - 1) * 600` columns from the town's centre
  (column 115), jittered by up to 100, so neighbours are 400-800 columns apart and the first
  ones are a short trip out. Size (hamlet / village / town: 64 / 88 / 112 columns) is a hash of
  the site; `surface_site_at(seed, x, margin)` answers "which village is column x in" in O(1).
  About one site in four (and always one of the two second sites) has an underground settlement:
  a 60-column, 10-row hall 40-120 nominal rows down (`strata::scaled_row`), 150-270 columns from
  the surface village (away from town for the first sites), slid in steps until it is 6 tiles
  clear of every point of interest (`streamed::villages::resolve_hall`).
- **Ids**: `VillageId` (the people framework's) = zigzag(site) << 1, | 1 for the underground
  settlement (`placement::surface_id`, `underground_id`, `decode_id`). `CharacterId`s of
  residents name their village, so a resident is found from an id alone.
- **Feature index**: villages are a regional layer (`index::Layer::Village`, `rx` = site, `ry` 0
  surface / 1 underground) of kind `FeatureKind::Village` with `FeatureDetail::Village(id)`
  (`WorldFeature::village()`), so `Terrain::features_in`, `Terrain::nearest_feature` and
  `Terrain::feature` find them anywhere; `village::nearest_village` wraps the outward search.
- **Terrain hooks**: `streamed::surface_row` = `raw_surface_row` (the heightmap; the
  surface-biome track replaces it) flattened across each village span to
  `village_ground_row(settings, centre)` (the raw row at the village's centre), easing back over
  `FLATTEN_FADE_COLUMNS` (16). Chunk assembly then runs the village stamps last
  (`streamed::villages::stamps_touching`): hazard and POI tiles under the village (8 rows) and
  around a hall (3 tiles) turn back into the band's soft rock, building lots get two rows of
  foundation plus a staggered third, the plaza and landing pad are paved, and halls are carved
  with a two-row foundation floor.
- **Layout grammar** (`village::layout::build`): landscape (`site_landscape`, a coarse noise band
  along the surface until biomes exist: the second hook for the surface track) -> culture ->
  `BuildingStyle`; a `Specialty`, prosperity 1-5, an optional `VillageProblem`; 4-12 residents
  from `people::generate_population`; then the street: hall and market flank the plaza, the
  specialty's building, inn and workshop alternate outward with houses, farms / towers / mine
  heads take the ends, spare columns widen the gaps, decorations fill them, the landing pad takes
  one end and signposts stand outside both edges. Keepers are residents picked by role, homes
  are dealt in family order, the eldest lives at the hall. `Village::area` is the rectangle a rig
  counts as "in the village" in; `Village::building_at_pixel` the door rule (in the lot's columns,
  on or up to 3 tiles above the village ground). `village::milestone_at(settings, x)` is the
  trail milestone at column `x` (every `MILESTONE_SPACING` columns outside villages and the town,
  pointing at the nearest surface place); the renderer draws the trail and posts from it.
- **Lookups** (memoized per process like streamed regions; a miss only costs time):
  `village::village(settings, id)`, `villages_between`, `village_at(position)`,
  `building_at(x, y)`, `nearest_village`, `village_identity` (name and culture without building
  the village), and `village_facts` (the story-worthy `WorldFact`s within 160 columns, cached
  per village; `Village::story_seeds` feeds them to `people::story_seeds` with the village's
  problem).
- **Services** (`village::services`, `world/villages.rs`): `Village::offers(building)` lists a
  counter's `VillageOffer`s (a `VillageDeal` and a flat price, or 0 for metered deals). They are
  bought with `SessionServiceRequest::Village { village, building, offer, service }`, validated
  in `command_allowed_here` (the rig must stand at that building's door, `service` must match
  the building) and resolved inside the tick (`apply_village_deal`): selling at a percentage of
  the market price (an assay office: one mineral only, well over the depot price), metered
  refuelling and repairs at a percentage of the town's unit prices, a rare part into the shared
  `rig_part_inventory`, a cosmetic unlock, a pack of special charges with no shack-level gate,
  charting (the nearest 3 unknown POIs within 420 columns learned as identified with a map reveal
  around each, and neighbouring villages charted), a timed `Blessing` (Ward: 70 % of hull damage
  in `apply_hazard_damage`; Thrift: 75 % of thruster fuel in movement; Fortune: +10 % on every
  cargo sale) and a rumour (the nearest unknown POI within 320 columns as an echo).
- **State**: `Player::villages` (`VillageJournal`: visited and charted villages, the active
  blessing) rides in the private `Loadout` section and saves with the player (serde defaults).
  Every tick, after map reveals, a rig entering a village for the first time is recorded with
  `GameplayEventKind::VillageDiscovered`; a bought deal emits `VillageServiceUsed`.
- **Report**: `village::report::village_report` behind `fatherload --village-report`.
- **Cost** (`village_timing_report`, release): 160 surface chunks across a village take about
  14 ms on V6 against 13 ms on V5 (the flattening check is a few integer ops per column and
  skips hashing away from sites); building a village (layout and residents) takes under 0.1 ms.

Tests: `village/tests.rs` (determinism, layout inside the span with a service and a specialty
in every village, housing, variety across cultures / sizes / specialties, flat foundations under
open sky with no hazards below, carved halls clear of POIs, the feature index and position
lookups, signposts, facts and story seeds), `village/placement.rs` (spacing, id round trips,
column lookups, rarity of halls) and `world/village_tests.rs` (discovery, deals validated at
the door, the assay office, blessings, charting, the journal saved and private).

## Replication

Every field belongs to exactly one section kind (the compiler enforces it through exhaustive
destructuring in `section`, `remove_player`, `redact_player` and `split_player`):

| Kind | Contents | Who receives it |
|---|---|---|
| Per tick | rig pose, velocity, fuel, hull, drill/scanner timers, deep-run pressure; bombs and the entity id/revision counters (`LiveEntities`) | clients the rig/bomb is relevant to |
| Per tick, delta | entities (gas clouds, boulders, drops, ...): records of entities new to the client or changed since its last frame (by revision), and despawns (gone or out of relevance) | clients the entity is relevant to |
| Op log | terrain tile writes | everyone, filtered by 16x16 terrain chunk relevance (stale chunks are refreshed with `terrain_chunk`: the chunk's diffs from generation) |
| Op log | per-player discovery | the owner only |
| On change | `RigBuild(player)`: upgrade levels drawn on the rig (drill, engine, cargo bay, hull, radiator, scanner, bulkheads, drill mount) and the rig's cosmetics | everyone |
| On change | `Loadout(player)`: cargo bay and block hopper, credits, upgrades, kits, rig parts, POI journal, quest log (solo quests, reputation, blueprint fragments), salvage, rescue count, deep reward milestone, service transactions, cosmetic unlocks | the owner only |
| On change | economy, contracts, progression (incl. `won_game`), town, world events, infrastructure, markers, RNG, timers (incl. twin-relay vault deadlines), session constants (tick timing, player template, world settings), crew quests | everyone |

Because other players' private state never reaches a client, replica checks compare
`visible_state_hash(viewer)`: private state is hashed only for the viewer and per-player data is
hashed player by player, so host and a correct client authority agree exactly. Terrain
dimensions, settings and the tiles that differ from generation travel only in keyframes
(`redacted_for(viewer)`); the receiver regenerates the rest.
