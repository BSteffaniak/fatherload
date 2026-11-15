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
| `enemies` | enemy spawning (the director), AI, attacks on rigs, the enemy damage model (`damage_enemy`, reached through `damage_entity`), blasts and scares on enemies, deaths and loot, safe zones, difficulty (see [Enemies](#enemies-srcenemyrs-srcworldenemiesrs)) |
| `bombs` | the explosives pass over charge entities: chain reactions and every explosive kind (`crate::explosive`): sticking, remote/sequenced detonation (after a 250 ms arming delay), cluster bomblets, shaped tunnels, loot-safe mining blasts, incendiary rings, flare pulses, smoke screens (`smoke_at`, `smoke_screens`), scare bangs, blast pushes on rigs and blast-feeling entities (`push_from_blasts`), blast damage (`damage_from_blasts`) and the throw preview |
| `weapons` | hardpoint weapons (`crate::weapon`): each rig's trigger, capacitor and aim per tick (free or auto-aim, `auto_aim_target`), projectiles as entities, hitscan rays (`active_beam`), hits on targets, charges, drops and plain rock, the damage API (`damage_entity`, `crate::combat`), practice targets (`spawn_target_dummy`) and the shack's weapon services |
| `movement`, `commands`, `services`, `progression`, `hazards`, `rescue` | gameplay rules used by the tick |
| `quests` | quest services (accept, turn in, abandon at the giver's counter), objectives stepping inside the tick, twists, rewards, reputation rebates, blueprint fragments |
| `roster` | `add_player` / `remove_player`; spawn placement on an unbounded grid so any number of rigs spawn without overlap |
| `crew` | persistent crew identity (WP-PERSIST, types in `crate::crew`): `set_crew_member` / `crew_member` (profile, name, local seat per rig), `player_for_profile`, `park_player` / `unpark_player` (a player and all per-player state leave the live world into the saved `CrewRoster` and come back exactly under the same id), `parked_players`, `all_player_ids`. The roster saves with the world, is not hashed, and `redacted_for` strips it from client keyframes |
| `discovery` | `DiscoveredTiles` (16x16 chunk bitsets; positions in JSON saves, bitsets in binary keyframes), clamped allocation-free reveals, `Discovery` op-log entries |
| `player_sections` | how a `Player` splits into per-tick `PlayerRig`, public `RigBuild` (visible tiers and cosmetics) and private `PlayerInventory` |
| `bases` | base building (WP-BASE): `check_build` / `apply_build_command` for every `BuildCommand`, fuel station synthesis, lift ladders, rescue home points (see Base building below) |
| `pilot` | the pilot on foot (Part II F5): exit and board, the pilot pass (movement with the owner's controls, air, lava, regeneration, the hand pick), throws, pickups into the pack, damage, strikes and blasts, death and respawn, parking, gear, crawlways (see [The pilot on foot](#the-pilot-on-foot-srcpilotrs-srcworldpilotrs)) |
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

### Surface biomes and the heightmap (`worldgen/biome.rs`, generator V7)

`GeneratorVersion::V7` (the current generator) is V6 (cave ecosystems and villages) with a real
surface (WP-SURFACE). V5 and V6 worlds keep their placeholder swell and their golden digests;
`GeneratorVersion::has_biomes` tells them apart, and a V7 golden digest
(`cave_tests::golden_v7_chunk_digests_are_pinned`) pins the new output. Everything below is a pure integer function of the settings and a column, so
chunks stay seamless and order-independent.

- **Biome spans** of about `BIOME_SPAN_COLUMNS` (360, jittered) columns. A climate cycle of
  `CLIMATE_CYCLE_SPANS` spans (cold, temperate, hot, temperate; phase set by the seed) deals
  each zone's spans from a shuffled deck holding every biome of that climate, so every biome
  appears every ~4,300 columns. The town's span is plains, a forest lies west of it.
- **Biomes** (`Biome`): plains (swells), forest (rolling hills), desert (dunes), badlands
  (terraced mesas with cliffs), tundra (low hills), mountains (ridged peaks up to
  `MAX_ELEVATION_ROWS` = 80 rows above the classic surface, ledges and short cliffs, caves into
  the rock via `mountain_cave`, ore graded by altitude) and wetlands (basins down to
  `MAX_DIP_ROWS` below it, holding decorative water from `LAKE_ROW`, see `lake_row`).
  Heights blend over `BIOME_BLEND_COLUMNS` either side of a boundary (`BiomeBlend`).
- **Ground** (`ground_kind`, `ground_material`): biome crusts and masses made of
  `TileKind::Surface(SurfaceMaterial)` (sand, sandstone, red rock, snow, granite, mud; hardness
  on the classic ladder, mountain granite like stone) plus dirt and clay; shallow biome ore in
  them (desert gold, tundra silver, bog iron; mountain pods richer higher up). Surface
  materials break into the classic `BlockKind` of the same hardness (snow melts).
- **Rows above 0**: a V7 world's first row is `-SKY_HEADROOM_ROWS` (`Terrain::min_y`,
  `streamed::top_row`); its mountains live there. Above it is open sky with no tiles. Code that
  walks rows uses `min_y()..height()` (as columns use `min_x()..=max_x()`).
- **Flat ground**: the town's columns, and a **flat site** under every surface village
  (`village::placement::surface_site`): `flat_sites_in(settings, first, last)`, `flat_site_at`,
  `nearest_flat_site` return `FlatSite { center, first_column, last_column, row, biome }`, the
  village's span with its ground row (the raw biome elevation at the village centre, clamped
  so lake sites sit on a raised bank and mountain sites on a shelf at most 24 rows up). The
  heightmap eases into each over `FLAT_FADE_COLUMNS`, so the village stands on level ground at
  `row` (`streamed::village_ground_row`), and its landscape follows the site's biome
  (`village::layout::site_landscape`).
- **Flora** (`flora_at`): presentation data for what stands on a column (grass, flowers,
  bushes, oaks, pines, snowy pines, cacti, dead brush, boulders, reeds, lily pads).
- Columns are memoized per thread (`column`), a cache that never changes a result.

`worldgen/biome/tests.rs` checks determinism and cache independence, bounds and step heights,
the flat town, flat and spaced sites, biome fairness (every biome within 8,000 columns of town on
every seed, none over 40%), tall mountains and lakes, seamless biome ground across chunks,
materials by biome and flora by biome.

**Surface rules elsewhere.** The local surface replaces the old flat-surface constants:
`quest::surface_row_at` (back on the surface, the stay-down twist, escort hauls, diggable
quest spots), world-event hazards (only below the local ground), cave crusts (from the classic
surface under mountains, which carve their own caves). **No flight ceiling (R17):**
`AUTHORITATIVE_SKY_FLIGHT_HEIGHT_TILES` is now the numeric edge of space (131,072 tiles, where
`f32` positions still resolve a fraction of a pixel); fuel is the real limit. Spawn placement
stacks slots only `SPAWN_SKY_ROWS_TILES` up; reveals, streaming and relevance simply find
nothing to do in the empty sky.

### The sky and altitude physics (`worldgen/sky.rs`, `sky.rs`, `environment.rs`, `climb.rs`, generator V8)

Expansion plan Part II F6. `GeneratorVersion::V8` (the current generator, `has_sky`) is V7's
ground exactly (`worldgen::sky::tests::v8_ground_is_exactly_v7_ground`) with a sky; V1-V7 output
and digests are unchanged and a V8 golden digest
(`worldgen::sky::tests::chunk_digests_of_v8_skies_are_pinned`) pins the new output.

**The climb table** (`climb.rs`): `CLIMB_CEILINGS[tank][engine][intake]` is the highest
altitude (rows above row 0) a rig of each fuel tank tier (1-15), engine tier (1, 4, 8, 12, 15)
and altitude intake tier (0, 2, 4, 6) reaches holding thrust from rest 100 rows up in still
air (the density profile, standard gravity, no wind or updrafts). `climb::ceiling(t, e, i)` and
`climb::fraction` define every band; `world::climb_tests::climb_table_matches_the_simulation`
re-measures a sample against the real movement step, `climb_table_report` (ignored, run in
release) prints the whole table, and `the_table_unlocks_the_sky_in_order` pins the design
claims. Rebalancing tanks, engines or the air moves the islands with them (as a new generator
version: it changes V8 output).

**Bands** (`sky::SkyBand`, `SkyBand::at_altitude` / `at_row`; rows above row 0 in
`worldgen::sky`): low isles from `LOW_ISLES_BOTTOM_ROWS` (65 % of a tier 2 tank + tier 4 engine,
about 220), the cloud archipelago from `ARCHIPELAGO_BOTTOM_ROWS` (a tier 4 rig's full reach,
about 540), the dead-air gap from `DEAD_AIR_BOTTOM_ROWS` (90 % of the best rig without intakes,
about 4,480), sky continents from `CONTINENTS_BOTTOM_ROWS` (85 % of a tier 13 tank, top engine
and intakes, about 8,080), orbit from `ORBIT_BOTTOM_ROWS` (about 9,030) and the Moon's underside
at `MOON_BOTTOM_ROWS` (85 % of a maxed rig, about 10,320). A V8 world's first row is
`-SKY_CEILING_ROWS` (the Moon plus 700 rows of space, about 11,300; `Terrain::min_y`); rigs may
fly higher into open space.

**Placement** (`worldgen::sky`): pure integer functions of the settings. Each band has a 2D
cell grid (column x altitude): low isle cells (`LOW_ISLE_CELL`, 192x100) hold a cluster of up to
three small isles, archipelago cells (`ARCHIPELAGO_CELL`, 320x176) a chain of two to four medium
islands, continent cells (`CONTINENT_CELL_COLUMNS`, 1,400 wide, one row) at most one continent
footprint 320-680 tiles wide; nothing in the dead-air gap or orbit. Every land stays inside its
cell, so a chunk reads only the cells it overlaps (seamless, order-independent). The Moon
(`moon`, `moon_column`) is one landmass 2,000-2,900 tiles wide whose underside's lowest point is
at `MOON_BOTTOM_ROWS`, `MOON_DEPTH_ROWS` (240) thick, above a column 3,200-4,800 tiles from town.
`SkyLand` describes a land (`kind`, `biome`, `center_x`, `top_row`, `half_width`, `depth`;
`bounds`, `surface_row(x)`, `tile_at(x, y)`, `feature()`), `lands_touching(settings, rect)` /
`land(settings, id)` / `tile_at(settings, position)` query them. Shapes: islands are a turf
top (`SkyTile::Turf(SkyBiome)`), soil, a rock underside in the island's palette
(`SkyTile::Rock(SkyBiome)`: meadow, blossom, frost, amber) with crystal pockets, and roots and
crystal hanging below; continents are placeholder `ContinentRock` under turf; the Moon is
`Regolith` over `MoonRock` with craters and crystal hanging under it (Wave 4 fills continents and
the Moon with content). Sky tiles are `TileKind::Sky(SkyTile)` (codes `0x600`), mineable on
the classic hardness ladder, breaking into the matching classic block.

**Streaming**: a chunk above the ground layer that touches no land is the one shared chunk of
air (`streamed::generated_chunk` never assembles or caches it), so empty sky costs nothing;
edits in the sky are ordinary streamed edits (saves, keyframes, chunk refreshes).

**Features**: islands, continents and the Moon are regional features (`Layer::Sky`,
`FeatureKind::SkyIsland` with `FeatureDetail::SkyIsland(biome)`, `SkyContinent`, `Moon`):
`features_in`, `feature(id)` and `nearest_feature` (which searches a V8 world's whole sky) find
them.

**Updrafts** (`Updraft`, `updrafts_touching`, `updraft_at`): a cell grid (`UPDRAFT_CELL`,
144x192) from `UPDRAFT_FLOOR_ROWS` up through the island and continent bands (none in the
dead-air gap) holds at most one column of rising air 4-9 tiles wide and 80-170 tall, pushing
560-900 px/s² up (more than a rig's gravity), tapering over its top `UPDRAFT_TAPER_ROWS`.

**Altitude physics** (`environment.rs`): `environment_at(settings, x, y)` gives an
`Environment` { `gravity` (scale), `air_density`, `cold`, `wind_x` (air velocity px/s),
`updraft` (px/s² up) }. It is exactly `Environment::STANDARD` on older generators and below
`STANDARD_BELOW_ROWS` (120 rows up), so ground and underground gameplay is bit for bit what it
was (the movement step takes the classic path whenever `is_standard()`). Only basic arithmetic
is used (no transcendental functions), so every platform agrees.

- *Air*: `air_density_profile` (full to `FULL_AIR_ROWS`, then
  `AIR_SCALE_ROWS / (AIR_SCALE_ROWS + rows above)`), fading to vacuum across the orbit floor
  (`air_density`). `Environment::thrust_efficiency(intake)` = density + the share of the rest the
  Altitude Intake tier recovers (`intake_recovery`, up to 48 %). In the movement step the
  efficiency scales lift, the climb-speed cap (`climb_cap_factor`: less thrust climbs slower,
  thin air drags less) and fuel burn (`burn_factor`: thin air runs rich); air density scales
  vertical drag (`drag_retention`) and the drive's coasting brake, and the fall-speed cap rises
  in thin air (`fall_cap_factor`). Airborne rigs drive relative to the wind
  (`DriveTuning::in_thin_air`, then `step_tick(v - wind) + wind`), so they coast with it.
- *Cold*: `cold_level` (0 below the archipelago, rising to 4 at the gap, 7 at the continents,
  10 on the Moon); `cold_excess(cold, de_icer)` after the De-Icer line's cover (`cold_cover`, 2
  levels per tier, the insulation hook) burns `COLD_DAMAGE_PER_EXCESS_LEVEL` hull per second per
  level in the hazard pass.
- *Wind*: `wind_at` blends per-seed layers every `WIND_LAYER_ROWS` (64) above `WIND_FLOOR_ROWS`,
  peaking by band (crosswinds of up to 210 px/s in the dead-air gap, none in orbit), with slow
  gusts along the layer.
- *Gravity*: `gravity_scale`: 1.0 up through the islands and the gap, `CONTINENT_GRAVITY` (0.85)
  in the continent band, `ORBIT_GRAVITY` (0.04) above the orbit floor, `MOON_GRAVITY` (0.3) on
  and around the Moon (fading out over `MOON_HALO_COLUMNS` and `MOON_HALO_ROWS`), blended
  smoothly. Applied to the rig, drops, charges (gravity bombs), enemies, enemy shots and arcing
  projectiles (`physics::BodyParams::in_environment` / `at` scale a body's gravity and subtract
  updrafts; weightless flyers stay weightless). The pilot on foot (T15) reads
  `Environment::gravity` / `body_gravity` the same way.

**Upgrades**: two shop lines appended after the hazard counters (offer indices 14 and 15,
`UPGRADE_OFFER_COUNT` 16): `UpgradeKind::AltitudeIntake` (6 tiers) and `UpgradeKind::DeIcer`
(6 tiers), stored in `HazardGear::altitude_intake` / `de_icer` (serde defaults; private
loadout). Aether thrusters (a sky-crafted engine part) are reserved for Wave 4.

**Events** (`world/altitude.rs`, stateless edges between a rig's position before and after its
move): `SkyBandEntered { player_id, band }` climbing into a higher band, `UpdraftCaught` and
`RigIcing` (crossing into cold the de-icer does not cover); netcode codes 110-112.

**Precision**: the Moon's top is about 11,300 rows up, where an `f32` world y still resolves a
sixteenth of a pixel (`world::sky_tests::positions_stay_precise_at_moon_height`).

Tests: `worldgen/sky/tests.rs` (bands, determinism, lands in their bands, empty gap and
orbit, the Moon's size and place, island anatomy, seamless chunks and the shared empty chunk,
V8 ground = V7 ground, features, updrafts, the golden digest), `environment/tests.rs`
(standard ground, air, intakes, cold, gravity fields, wind), `world/climb_tests.rs` (the
harness) and `world/sky_tests.rs` (orbit drift, lunar gravity, bombs and drops in the fields,
updrafts, wind, cold, reachability by tier in the real sky including flying up into the Moon,
precision, saves and keyframes of island edits, determinism and tick-rate safety).

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
rich V3+ worlds, plus golden digests pinned to `GeneratorVersion::V1`, `V2`, `V3` and `V4` (and
`V5` in `worldgen/streamed/tests.rs`). `worldgen/poi_tests.rs`
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
WP-DROPS' `spawn_loot` (kind lifetimes, cell cap; the magnet collects them) with
`CaveCacheOpened`. Other cave rock (mycelium, silt, ruin brick, fossil ribs, stalactites,
obsidian) breaks into the nearest plain building block (`BlockKind::from_tile`). Movement: in cave water a rig is
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
`GasCloud`, `Boulder` (both migrated from ad-hoc world lists without behaviour changes),
`Drop` (a floating `ItemStack` with magnet / dump / "blocked" marks; see "Drops and pickups"
below), `Critter` (WP-CAVES ambient life), `Npc` (WP-PEOPLE, see "People in the world"), `Enemy` / `EnemyShot` (see
[Enemies](#enemies-srcenemyrs-srcworldenemiesrs)), `Charge` (every live explosive, see
below), `Projectile` (a weapon shot in flight) and `Target` (a practice target with
`combat::Health`, the reference damageable kind).

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

Explosive charges are `EntityKind::Charge` entities: motion in the body, everything else
(serial, fuse deadline, kind, owner, stuck side, burn stage) in `entity::ChargeState`. They move
in their own pass (`EntityPhase::Charges`, the bomb pass between hazard damage and drilling)
with the shared `physics::step_body` / `step_body_sticky` (`BodyParams::BOMB`) and
`physics::apply_blast` kicks; `WorldState::bombs()` views them as `PlacedBomb`s in serial order
and `insert_charge` adds one to a scenario. The move kept chain reactions and every explosive
bit for bit: `world/charge_equivalence_tests.rs` replays scenarios whose traces were recorded
with the old charge list. Charges replicate as relevance-filtered entity deltas like any other
entity; only their serial counter rides in `LiveEntities`.

### Weapons and damage (`weapon`, `combat`, `world/weapons.rs`)

WP-WEAPONS part 2. `weapon::WeaponKind` (rivet gun, harpoon, scatter blaster, plasma cutter,
rocket launcher, railgun) with `stats(mk)` per upgrade tier, prices, shack levels, the
capacitor curve (`battery_capacity`, `battery_recharge`, recharging from fuel at
`FUEL_PER_ENERGY`) and `AmmoKind::Rocket`. A rig owns weapons in its private
`Player::weapon_rack`, mounts one (`Player::mounted_weapon`, public in `RigBuild`) and keeps its
live `Player::weapon` state (energy, cooldown deadline, aim, trigger) in the per-tick
`PlayerRig`. Firing is held input: `ControlState::weapon` (`WeaponControl`: trigger, auto-aim
flag, `i8` aim vector normalized with `sqrt` only), so shots are predicted like movement;
`PlayerCommand::MountWeapon` swaps weapons and the Explosives Shack services `BuyWeapon`,
`UpgradeWeapon`, `UpgradeBattery` and `BuyAmmo` sell them. Projectiles fly in the late entity
phase with swept substeps (damageable entities, live charges, which they set off, harpoon loot
and terrain); the cutter and railgun are hitscan rays (`cast_ray`: pierce, tunnelling through
plain tiles). Weapons only break plain soil and rock (`weapon_broke_tile`, the one place they
change terrain). Shots never collide with rigs: friendly fire is off.

The damage seam: give a new damageable kind's state a `combat::Health` (add its arm to
`EntityState::health` / `health_mut`). Enemies (T10) report a health view through
`EntityState::health` (so shots, rays and auto-aim find them) but no `health_mut`:
`damage_entity` routes them to the enemy model (`damage_enemy`: weapons map to `DamageKind`
by gun, railgun / harpoon pierce, the cutter burns, rockets blast), and `damage_from_blasts`
skips them because `enemies_feel_blasts` owns their blast damage, EMP stuns and scares. Then `WorldState::damage_entity(id, Damage, output)`
(armour, armour piercing, knockback for `feels_blasts` kinds, `EntityDamaged` /
`EntityDestroyed` events, despawn on death) hurts it, projectiles and rays hit it, auto-aim
targets it (unless it is inside a smoke screen), explosives damage it by distance
(`combat::explosive_damage`) and EMP pulses hurt it if `Health::machine`. Scare charges are the
cue to flee (`explosive::SCARE_RADIUS`, `SCARE_MILLIS`) and `smoke_at` hides the player from
creature senses. Events: `WeaponFired`, `WeaponDry`, `WeaponMounted`, `ProjectileImpact`,
`EntityDamaged`, `EntityDestroyed`.

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
ammo, explosives, cosmetics and quest items take none. Special explosives are
`ItemKind::Explosive` items (`ExplosiveKind::item`, stable ids) in the cargo bay; plain bombs stay
the `Player::bombs` count (`Player::explosive_count` / `add_explosives` / `take_explosive` /
`carried_explosives` cover both). Rocket ammo is `ItemKind::Ammo` (`AmmoKind::item`); weapons
have stable `ItemKind::Weapon` ids (`WeaponKind::item`) for loot, and a picked-up weapon item
goes to the rack through `Player::receive_weapon`.

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

Expansion plan WP-VILLAGE, on [`GeneratorVersion::V6`] and V7 worlds
(`GeneratorVersion::has_villages`, the single switch; V5 and older generate exactly as before
and the V5 golden digest is unchanged). A village is a pure function of `(WorldSettings, site)`:
nothing about it is saved or replicated except what players did there.

- **Placement** (`village::placement`, const and integer-only): surface site `n` (`n > 0` east,
  `n < 0` west, never 0) is centred `450 + (|n| - 1) * 600` columns from the town's centre
  (`TOWN_CENTER_COLUMN`, the middle of `surface::town_span()`: column 115), jittered by up to 100, so neighbours are 400-800 columns apart and the first
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
- **Terrain hooks**: on V6, `streamed::surface_row` = `raw_surface_row` (the placeholder
  heightmap) flattened across each village span to `village_ground_row(settings, centre)` (the
  raw row at the village's centre), easing back over `FLATTEN_FADE_COLUMNS` (16). On V7 the
  biome heightmap is the surface and already holds every village on its flat shelf (see
  surface biomes), so all three read the biome heightmap; on V7 caves (classic caverns and cave
  ecosystems, through `Ctx::cave_allowed`) also keep clear of village ground and 6 tiles around
  every hall (`streamed::villages::keeps_caves_out`). Chunk assembly then runs the village stamps last
  (`streamed::villages::stamps_touching`): hazard and POI tiles under the village (8 rows) and
  around a hall (3 tiles) turn back into the band's soft rock, building lots get two rows of
  foundation plus a staggered third, the plaza and landing pad are paved, and halls are carved
  with a two-row foundation floor.
- **Layout grammar** (`village::layout::build`): landscape (`site_landscape`: on V7 the site's
  biome, plains / forest and wetlands -> forest / mountains and tundra -> hills / desert -> mesa /
  badlands -> mesa or scrapland; on V6 a coarse noise band along the surface) -> culture (on V7
  hills are always Highland and scrapland always Scrapper) ->
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

## Enemies (`src/enemy.rs`, `src/world/enemies.rs`)

Expansion plan WP-ENEMIES (T10): creatures and machines that live in caves and the deep layers.
They are a reason to arm up, not constant harassment: the surface and the town are safe, density
and danger grow with depth, and a Peaceful world has none.

| Kind | Family | Moves | Attack (tell) | Counter |
|---|---|---|---|---|
| Cave bat | creature | flies in packs | bites on contact | anything; a lit flare lures the swarm |
| Burrower | creature | swims through soft rock | lunges (rumble, glinting eyes) | stop drilling (it hears drills through rock), bombs |
| Acid spitter | creature | walks | lobbed acid (puffs up) | close in or break line of sight |
| Rock mimic | creature, armoured | dormant boulder, then walks | lunge (wakes when you come close) | explosives |
| Shellback | creature, armoured | slow walker | charge (wind-up) | explosives, armour piercing |
| Magma wisp | creature | flies (mantle) | fireballs (glow) | kinetic and explosives; fire-proof |
| Brood nest | creature | stationary (lairs) | hatches bats while a rig is near | destroy it |
| Brood lord | creature, elite | walks (lairs) | acid volleys and charges | explosives, keep moving |
| Mining drone | machine | flies | ram (beep) | EMP |
| Sentry turret | machine | stationary | energy bolts (charge glow) | EMP, explosives |
| Rogue excavator | machine | walks, chews through soft rock | charge (engine rev) | EMP, explosives |
| Ancient guardian | machine, elite | hovers (lairs) | bolt volleys; shielded | EMP drops the shield, then explosives |

- **Data** (`enemy.rs`): `EnemyKind::stats` (family, locomotion, attack style, radius, health,
  shield, speeds, sight / hearing / leash, contact damage, wind-up and cooldown, range,
  knockback), `resistance(DamageKind)` (armour folds to explosives, machines feel EMPs,
  creatures burn), `band_roster(band)` (what roams each band, weighted), `lair_occupant`,
  `loot(kind, tier, roll)` and `ShotKind` (acid lobs, fireballs, bolts). Health and damage
  scale with the depth tier the enemy spawned at (`tier_scale`, the band index), the crew size
  (+30 % health per extra rig) and the difficulty.
- **Entities**: an enemy is `EntityState::Enemy(EnemyState)` (mode, deadlines, target, home
  tile, path waypoint, facing, hurt / burn timers, lair, parent nest, seed, last attacker); a
  projectile is `EntityState::EnemyShot` with an expiry. Both update in the late entity phase,
  never sleep, and replicate, save and hash through the entity store; an enemy that did not
  change in a tick keeps its revision.
- **Director** (`advance_enemy_director`, before the late phase): each second it despawns
  enemies more than `DESPAWN_TILES` from every rig, and for each rig deeper than
  `MIN_SPAWN_DEPTH_ROWS` below the surface and outside a safe zone, under a per-rig cap (3 +
  tier / 2, +3 on Hard) and a world cap, rolls a spawn (chance by tier, difficulty and the
  world's `HazardIntensity`) in a chunk on a ring 22-30 tiles out, which then cools down for
  40 s (less on Hard). The spot must be out of every rig's view, in the dark, outside safe
  zones and fit the kind (flyers and walkers need a cave around them, walkers a floor,
  burrowers soft rock); bats come in packs. Every two seconds lair chunks (one in seven, in
  bands with lairs, holding a cavern) near a rig get their occupant (nests, turret pairs,
  elites with escorts) unless the lair was cleared (`EnemyDirector::cleared_lairs`).
  Everything is hashed from the seed, the tick and positions: no RNG state.
- **AI** (`advance_enemy`): perception every `THINK_MILLIS` (sight with `physics::line_clear`,
  drilling rigs heard through rock, flares luring bats, rigs in safe zones ignored), then a mode
  machine (`EnemyMode`: idle wander, dormant, chase, wind-up, strike, recover, return home
  healing when leashed or the target is lost for 4 s, flee when badly hurt, EMP stun).
  Movement by locomotion (flying with `steer_toward`, walking with step hops, burrowing,
  stationary) and, when the way is blocked, `physics::find_tile_path` (360 tiles, at most one
  search per enemy every `PATH_SLOT_TICKS` ticks, staggered by id, cutting corners to the
  farthest visible path tile).
- **Hitting rigs**: contact (live during a lunge or for swarmers when the attack is ready) and
  shots go through `strike_rig`: no hit in safe zones, on disabled rigs, in god mode (test-mode
  invulnerability or no-clip) or inside the rig's i-frames (`RIG_GUARD_MILLIS`); otherwise the
  rig is knocked back (capped under the safe landing speed), its i-frames start
  (`EnemyDirector::rig_guard`) and it takes the scaled damage. Events: `RigStruck`, then
  `PlayerDamaged`.
- **Damage model** (`WorldState::damage_enemy(id, enemy::Damage, &mut TickOutput) ->
  enemy::DamageOutcome`; weapons reach it through the shared `damage_entity` of
  `crate::combat`): `Damage { amount, kind: DamageKind, source: DamageSource, impulse }` applies
  resistance, shields (only EMPs break them), EMP stuns, burning, aggro on the attacking player,
  flinch impulses and fleeing; a kill drops `loot` through WP-DROPS' `spawn_loot` (each
  kind's lifetime, the per-cell cap), clears its lair, counts in `EnemyDirector::kills` and emits `EnemyKilled { by }`.
  `enemies_near(x, y, radius)` finds targets. Call it inside the tick.
- **Explosives** (`enemies_feel_blasts`, called from the bomb pass right after
  `push_from_blasts` on the same pre-blast terrain): every blast kind hurts enemies in reach
  with a clear line (burrowers through rock) and pushes them; incendiaries set creatures
  alight; EMPs (`EMP_RADIUS`, through rock) stun and hurt machines and drop shields; flares
  only lure; scare charges send creatures fleeing (`SCARE_RADIUS`, `SCARE_MILLIS`; machines,
  elites and nests ignore them). The `Blast` carries its owner for kill credit. A rig inside a
  smoke screen (`smoke_screens`) is hidden from enemy sight (not from hearing).
- **Difficulty**: `EnemyDifficulty` (Peaceful / Normal / Hard) in the `EnemyDirector`, set before
  play with `set_enemy_difficulty` or in play with `PlayerCommand::SetEnemyDifficulty` (event
  `EnemyDifficultyChanged`); Peaceful clears every hostile and spawns none.
- **Safe zones** (`enemy_safe_zone`): the surface strip, the town and the top of its shafts,
  `BASE_SAFE_TILES` around placed infrastructure, `VILLAGE_SAFE_TILES` around every village and
  underground hall (`worldgen::streamed::in_village_area`, geometry only) and every claimed
  base (`Bases::is_safe_zone`, WP-BASE).
- **Caves** (WP-CAVES, V6): inside a cave ecosystem its `CaveSpawnTags` triple the weight of
  roster kinds that live there (`cave_habitat_fits`: swarmers / bats, burrowers, nests /
  spitters and mimics, aquatic / shellbacks, heat / magma wisps, machines in ruins) and its
  `danger_per_mille` thins spawns below 1000; a chunk centred in a cave with `elite_lair` room
  is a lair one time in `CAVE_LAIR_ODDS`, on top of the plain hashed caverns.
- **Events**: `EnemyAlerted`, `EnemyWindup`, `EnemyFired`, `EnemyHurt`, `EnemyKilled`,
  `EnemyStunned`, `RigStruck`, `EnemyDifficultyChanged` (all with world positions for sound).
- **Replication**: enemies and shots ride the entity deltas (clients interpolate them with
  `Body::lerp_position` and predict them like any entity); the director is the shared
  `SectionKey::Enemies` section. The state hash skips an untouched director, so worlds from
  before enemies hash as they did.

Performance (`enemy_benchmark_report`, `cargo test --release -p fatherload_simulation
enemy_benchmark_report -- --ignored --nocapture`): the world cap of 64 mixed enemies hunting two
rigs in a big cavern costs about 34 µs per tick on top of a 12 µs quiet tick (worst tick about
0.65 ms, when many path searches land together); an enemy that holds still keeps its revision,
so dormant mimics and idle turrets cost no replication.

`world/enemy_tests.rs` covers spawn rules (dark caves, out of view, caps, safe zones, Hard vs
Normal, Peaceful), AI (chase and bite with i-frames and knockback, spitter wind-up and acid,
dormant mimics, leash, burrowers hearing drills, flares, nests, step hops, paths through gaps),
damage (armour, bombs, EMP and shields, fire, loot, cleared lairs), god mode, determinism, saves
and replication.

## Base building (`src/base.rs`, `src/world/bases.rs`)

Expansion plan WP-BASE. A [`base::Bases`] (one shared on-change section, `SectionKey::Bases`,
saved with the world) holds every claim, module, fixture and home point; solid pieces are
terrain tiles.

- **Claims.** A claim beacon (`ModuleKind::Beacon`) stakes a claim: a square of
  `CLAIM_RADIUS` (10) tiles each way around it, owned by its builder, never overlapping
  another claim or the town (its lots and the first `TOWN_NO_BUILD_ROWS` rows under them).
  `claim_limit` (two, plus one every two bands reached) caps claims per player; each extra
  beacon costs more (`beacon_price`). A beacon comes down only once its claim is bare.
- **Pieces.** `BuiltTile` (block walls, plating, glass, planks) are `TileKind::Built` terrain
  tiles: solid, replicated and saved by the op log and diffs, and `is_indestructible` (no drill,
  test mode included, and no blast breaks them; `blast_radius` and shaped-charge lanes skip
  them). Tile codes use their own range (`BUILT_TILE_CODE_BASE`, 0xB00). `TileFixtures` hold a
  back wall and a passable front `Fixture` (door, lift ladder, lamp, decorations).
- **Modules** (`ModuleKind`, `PlacedModule`): footprints placed by their bottom-left tile, open
  tiles on solid ground, inside one claim. Storage chest, crew quarters, generator, fuel station,
  repair bay, cargo uplink, refinery, workshop and the surface-only homestead; later ones unlock by band
  (`ModuleKind::unlock_band`). Each claim's power (`Bases::power`, `WorldState::claim_power`) is
  its beacon's cell plus generators (geothermal: more in hotter bands); modules draw power in
  placement order (`WorldState::base_module_powered`). Powered fuel stations synthesize fuel on a
  fixed schedule (`advance_bases`, every `FUEL_SYNTH_PERIOD_MILLIS`); refuelling from a station
  is free, supply drops cost credits (`supply_drop_offer`). Powered refineries refine the ore
  left in them on the same schedule (`REFINE_PER_PERIOD` lumps, each worth
  `REFINERY_BONUS_PERCENT` more than the market pays) into credits collected with
  `CollectRefined`; workshops press hopper blocks plus a few credits into explosives
  (`ModuleService::Craft`, `workshop_recipe`: bomb, C4, mining charge). A floor tile holding up a
  module or a standing fixture does not come down (`BuildRefusal::Supporting`). Not built yet
  (plan §7 core list): the turret (waits for WP-ENEMIES targets), the garage module (the paint bay
  lives in the town's Repair Garage interior) and the teleport pad (later).
- **Commands.** `PlayerCommand::Build(BuildCommand)`: `Place { item, at, facing }`,
  `Remove { at }` (module, else solid piece, else front fixture, else back wall, with refunds:
  blocks back to the hopper and spilled as item drops when it is full, 75 % of a module's
  credits) and `Use { structure, service }`. `WorldState::check_build` returns the verdict and
  `BuildQuote` (cost or refund) the tick applies, or a `BuildRefusal` with a message; build
  mode previews with it. Every action is host-authoritative (clients do not predict it).
  Pieces take a block from the hopper or buy it (`block_credit_price`); modules take credits plus
  blocks (cheapest kinds first, the rest bought).
- **Permissions.** `ClaimAccess`: the crew builds and uses (default), crew uses / owner builds,
  or owner only; the beacon's `CycleAccess` service is owner-only.
- **Lift ladders and homes.** A rig centred on a ladder burns no fuel, climbs while thrust is
  held even with an empty tank and falls no faster than `LIFT_SLIDE_SPEED`
  (`apply_base_lift`, after the movement step). `SetHome` on a bed or homestead makes rescues
  land there (`WorldState::rescue_home`).
- **For other tracks.** `Bases::is_safe_zone` (claims plus `CLAIM_SAFE_MARGIN`) is where enemies
  must not spawn. Blocks come from `Player::block_hopper` (`ItemKind::Block`), which WP-DROPS
  fills.
- **Events.** `BasePlaced`, `BaseRemoved`, `BaseServiceUsed` (netcode event codes 70-72).

`world/base_tests.rs` covers placement rules, refunds, fixtures, blast-proofing, the fuel
station refuelling a stranded rig at depth, synthesis and supply drops, power order, storage,
the uplink, lift ladders, home points, access, claim limits, save round trips, replication
through terrain ops and the section, and determinism.

## World clock (`src/clock.rs`, `src/world/clock.rs`)

Part II F4. Time of day is derived, never stepped: `world_millis = tick * 1000 / hz +
ClockState::offset_millis`, so it is deterministic, tick-rate safe and replicates for free.

- **Reading the time.** `WorldState::clock()` returns a `WorldClock` (`Copy`): `day()` (from 1,
  rolls over at midnight), `slice()` (0..96 from midnight), `time_permille()`, `hours_minutes()`,
  `phase()` (`DayPhase::{Night, Dawn, Day, Dusk}`), `phase_progress()`, `is_night()`,
  `light_level()` (open-sky permille: 1000 by day, 110-200 at night by the Moon), `moon_phase()`
  (`MoonPhase`, an 8-day cycle offset by the seed) and `millis_until_dawn()`; presentation floats
  `day_fraction()`, `sun_angle()` (0 rising left, `PI/2` noon, `PI` setting right),
  `sun_height()`, `moon_cycle()`. Phase boundaries (slices): night 78..18 round midnight, dawn
  18..23, day 23..73, dusk 73..78; with the default 48-minute day that is ~30 minutes of light and
  18 of night. New worlds start at slice 32 (08:00) of day 1. Shortcuts: `WorldState::is_night()`,
  `day_length()`.
- **Settings and saves.** `DayLength::{Short, Standard, Long}` (24 / 48 / 72 real minutes) is set
  at world creation (`WorldState::set_day_length`, the New World dialog). `ClockState` (the
  `clock` world field, serde default: worlds and saves from before load with the standard day)
  holds the day length, the offset (start time plus skipped nights), the `SleepRule`, the
  sleepers and the sleep countdown. It replicates as the shared `SectionKey::Clock` section
  (changes only when someone sleeps or the rule changes) and is hashed only once it differs from
  a new world's standard clock.
- **Time that should pass while the crew sleeps** (contractor construction, plan decision 3) must
  use `WorldClock::world_millis()`, not ticks: sleeping moves only the clock.
- **Day events.** `GameplayEventKind::{Dawn, Dusk, NewDay, NightSkipped}` (`day` carried) fire in
  the tick that crosses the boundary (event codes 110-113), plus `GotUp` and `SleepRuleChanged`
  (114-115). Audio: rooster and birds at dawn, crickets at dusk, a bell at midnight (heard by
  seats under the sky), a lullaby on a skipped night, and a night cricket loop.
- **Beds and the sleep vote.** `ModuleService::Sleep` on a bed or homestead, or the inn's
  `VillageDeal::Room` (`INN_ROOM_PRICE` credits), puts a player in bed at dusk or night
  (`BuildRefusal::NotTired` otherwise). When every live player is in bed (`SleepRule::Everyone`)
  or anyone is (`SleepRule::Anyone`, `PlayerCommand::Sleep(SleepCommand::SetRule)`, offered to
  the host), a `SLEEP_FADE_MILLIS` countdown runs and the clock jumps to the next dawn.
  `SleepCommand::GetUp`, driving more than a tile from the bed, the bed coming down, leaving or
  parking all get a player up. Queries: `is_in_bed`, `awake_players`,
  `sleep_countdown_seconds`, `seconds_since_night_skipped`.
- **Light for gameplay.** `WorldState::light_at(x, y)` (tile coordinates, permille, integer
  math): the open sky's light fading out over `SKY_LIGHT_DEPTH_TILES` under the local ground
  (the underground is dark whatever the time), or the brightest source in reach: rig lamps,
  lamp fixtures, village and town streets, lava, magma vents and glowing cave growths. Night
  monsters (T25) and others may branch on it.
- **The Moon's place.** `WorldState::moon_anchor()` returns a `MoonAnchor` (column, underside
  row). On a V8+ world it is the real Moon landmass (`worldgen::sky::moon`: its column and
  `row_of(MOON_BOTTOM_ROWS)`); older generators fall back to `clock::default_moon_anchor(seed)`
  (a column 2,400-4,800 tiles east or west of town, row `DEFAULT_MOON_ROW`). The game draws the
  phased backdrop Moon above the anchor and, on V8, fades it out as the real Moon's looming
  underside (`sky_lands`) takes over from the continents up.
- **Scenario setup.** `set_world_time_millis` (tests and screenshot scenes).
- **Formats.** `PROTOCOL_VERSION` 11 and save format 9 (version 8 saves load as is) cover the
  clock, shared with the other wave 3 tracks (one bump each for the whole wave).

`world/clock_tests.rs` covers determinism, tick-rate safety, each boundary's event, beds being
for the night, sleeping alone, the crew vote and the host's rule, getting up, departing players,
light levels, saves (including version 8 files), replication and hashes.

## The pilot on foot (`src/pilot.rs`, `src/world/pilot.rs`)

Expansion plan Part II F5. A player who climbs out of the rig becomes a pilot: an
[`EntityKind::Pilot`] entity owned by the player ([`pilot::PilotState`]; it replicates, saves,
hashes and is relevance-filtered like every entity). There is at most one per player, and the
entity is the single source of truth for "on foot" (`WorldState::pilot_of`, `is_on_foot`,
`pilots`).

- **Exit / board** (`PlayerCommand::TogglePilot`, predicted on clients): a rig slower than
  `EXIT_MAX_SPEED` lets its pilot out beside it (`can_exit_rig`); a pilot within `BOARD_REACH`
  of its rig boards it (`can_board_rig`), and the personal pack empties into the rig (what fits).
  Events `PilotExited`, `PilotBoarded { transferred }`. The parked rig keeps its cargo, gets
  neutral controls (no thrust, drill or weapon), is still hurt by enemies, hazards and blasts,
  and its roof is a one-way platform for its own pilot.
- **Controls**: while on foot the player's held `ControlState` drives the pilot in its own pass
  right after the inputs (`advance_pilots`): `horizontal` walks (a held direction breaks into a
  run after `RUN_AFTER_SECONDS`), `thrust` (up) climbs ladders / ropes and jumps elsewhere,
  `drill_down` crouches, crawls and climbs down, the new `ControlState::jump` jumps (held in
  the air: the jetpack), and `weapon.fire` swings the hand pick along `weapon.aim` (down while
  crouching, up while holding up, else the facing). Bomb commands throw from the pilot's hands
  (`pilot_throw`, a little loft on level throws, `THROW_COOLDOWN_SECONDS`); the scanner and
  infrastructure kits are rig-only.
- **The body** (`pilot::step_pilot`, pure, over the `PilotTerrain` trait): a 25 x 50 px hitbox
  (28 px crouched) with the feet at `Body (x, y)`; integrated in fixed 240 Hz steps, so 30, 60,
  120 and 240 Hz sessions trace the same path. Coyote time, jump buffering, variable jump
  height, ladders / ropes / lift ladders (`Fixture::Ladder`, the new `Fixture::Rope`), ledge
  grabs and pull-ups, swimming in cave water, the jetpack (`PilotGear::jetpack` tiers of 1.5 s,
  recharging only on the ground), and landing speed for fall damage (`fall_damage`: a fall of
  about five tiles is free, twelve kill an unarmoured pilot).
- **Survival**: health and air from the suit (`PilotGear::max_health`, `max_oxygen`); air
  drains under water, in gas clouds and beside gas pockets, and above `THIN_AIR_ROW`
  (`WorldState::pilot_air_at` is the seam for F6 air density and the Moon's vacuum), and
  suffocation hurts. Lava burns; health regenerates after `REGEN_AFTER_SECONDS` unhurt.
  Damage: `WorldState::damage_pilot` (also reached from `damage_entity`; pilots report no
  `health()`, so weapons and auto-aim never find crewmates), `strike_pilot` for enemies (safe
  zones, strike guard, knockback, difficulty scaling), blasts (`pilots_feel_blasts`, own bombs
  included). Events `PilotHurt`, `PilotLanded { hard }`, `PilotOxygenLow { empty }`,
  `PilotSplashed`, `PilotLedgeGrab`, `PilotJumped`, `JetpackIgnited`, `JetpackEmpty`.
- **Death**: at zero health the pilot drops half of its first three pack stacks as drops and
  respawns in the rig (what the pack kept goes aboard), or (rig wrecked) on foot at the
  player's home point (`PilotDied`).
- **Radio for a pickup**: `PlayerCommand::Rescue` on foot hauls the pilot straight back aboard
  its rig for `PILOT_RECALL_FEE` credits (pack emptied as when boarding, `PilotBoarded`), so a
  pilot in a pit the pick cannot dig, or beside a wrecked rig, is never stuck. The rig's own
  rescue then follows from the rig as usual.
- **Hand pick**: chips only soft dirt-like tiles (`pick_seconds`: dirt, sand, snow, mud 0.9 s;
  clay 1.6 s) within `PICK_REACH`; a broken tile drops through the drop system
  (`spawn_tile_drop`); anything harder clinks (`PickStruck { broke }`, `PickClinked`).
  `WorldState::pick_target` tells the renderer which tile.
- **Pack**: `Player::pilot_pack` (private loadout, capacity `PilotGear::pack_capacity`);
  pilots collect drops they touch into it (`advance_pilot_pickups`, no magnet), and
  `PlayerCommand::PilotPack { item, count, to_rig }` swaps stacks by hand (in the rig or beside
  it; `PackTransferred`).
- **Gear**: `Player::pilot_gear` (private), bought with `SessionServiceRequest::BuyPilotGear` on
  the Explosives Shack's rack (`PilotGearKind::price`); a pilot already out is refitted.
- **Enemies** perceive pilots as prey (seen, never heard: only drilling rigs are heard), chase
  whichever of a player's bodies is nearer, and bite or shoot pilots they touch.
- **Counters and buildings** use the actor: `actor_position` / `actor_tile` (the pilot's
  middle on foot, else the rig) drive `player_surface_zone`, `player_village_building`,
  `check_build` (build mode and module menus reach from the pilot) and the netcode pose
  (relevance, smoothing; the on-foot pose mode is one constant). Counters that work on the rig
  itself (`services::command_services_rig`: fuel, repair, paint, selling cargo or blocks,
  upgrades, and the village sell / refuel / repair deals) also need the rig parked within
  `RIG_SERVICE_REACH` of the pilot (`WorldState::rig_within_service_reach`).
- **Pilot-only spaces**: `BuiltTile::VentGrate` (code 7) is solid to rigs, drops, charges and
  enemies but pilots pass it (`pilot::pilot_passable`); `WorldState::carve_crawlway` cuts a
  1-tile-high tunnel capped by grates. Content is tagged with `poi::SpaceAccess`
  (`RequiresPilot`, `RequiresRig`, `Both`): `PoiKind::access`, `CaveKind::access` (chasms are
  rig-only), `BuiltTile::access`, `poi::CRAWLWAY_ACCESS`.
- **Persistence**: parking a player on foot parks the pilot too (`ParkedPlayer::pilot`,
  restored where it stood unless that spot was filled in); removing a player removes its pilot.
- **Scenario helpers**: `place_pilot`, `edit_pilot`, `carve_crawlway` (tests and screenshot
  scenes).

`pilot/tests.rs` covers the body on hand-made grids (walk and run, jump height, short hops,
coyote time, buffering, 1-tile shafts, crawlways, low ceilings, ladders, ledges, fall damage,
swimming, the jetpack, tick-rate independence at 30-240 Hz); `world/pilot_tests.rs` covers exit
and board, controls, the pack, the pick, throws and blasts, oxygen, death drops and respawn,
falls and strikes, god mode, crawlways, gear, parking, saves (JSON and postcard, identical
continuation), replication (redacted keyframes, entity deltas, identical prediction) and
tick-rate independence; the netcode harness bots climb out, jump, pick and board every cycle
(zero corrections at zero latency, convergence under loss).

## People in the world (`src/people/{clock,routine,camps,offers,contractor,ledger}.rs`, `src/world/people.rs`)

Expansion plan WP-PEOPLE (wave 3, T17), on worlds with villages (`GeneratorVersion::has_villages`,
V6 to V8). Village residents and the town's folk are `EntityKind::Npc` entities.

- **Clock** (`people::clock`): `ClockSource` (`day_time() -> DayTime`, `is_night()`).
  `WorldState::people_clock()` is the only place people ask the time, and it reads T14's world
  clock (`clock::WorldClock` implements `ClockSource`: day from 0, minute of its 24-hour face),
  so routines follow the world's day length and a skipped night wakes everyone at dawn. People's
  own schedule calls 20:00 to 06:00 night. `TickClock` (tick, tick rate,
  `DEFAULT_DAY_LENGTH_MINUTES` = 48; tick 0 is 08:00) reads the same as a fresh standard world's
  clock, and `TickClock::tick_at(time, hz, day_length)` (its inverse) stays for tests and
  scenes.
- **Settlements and routines** (`people::routine`): `Settlement` is a village's (or the town's,
  `TOWN_VILLAGE` = `VillageId(0)`, `TOWN_FOLK` frontier residents of `town_population`, homes above
  the shops) routine view: lots, plaza, and per resident a home lot, a `Work` (a counter, an
  outdoor spot by their trade, or roaming) and habits rolled from personality (`offset` -45..45
  minutes, `homebody`, `night_owl`, walking `pace`). `Settlement::stop_at(index, DayTime)` is pure:
  `Activity` (sleeping, morning, working, lunch, unwinding, at the inn, resting on every seventh
  day) and a `Place` (`Inside(lot)` or `Outside { column, roam }`). `routine::settlement(settings,
  id)` and `routine::character(settings, CharacterId)` are memoized lookups (the renderer uses
  them too).
- **Wanderers' camps** (`people::camps`): traders, prospectors, surveyors (travelling
  scientists) and smugglers met on the open surface. The surface is cut into `CAMP_SPACING`
  (360) column slots; `camp(settings, camp_id(slot))` tries a few seeded spots per slot and
  pitches on the first that is flat across the camp, dry, off the mountains and clear of the
  town, every village and the trail milestones (memoized per slot). A `Camp` has a `CampKind`,
  a campfire column, a ground row, a culture from the biome and 1-3 wanderers
  (`camp_population`: the lead has the camp's trade, companions travel with them). Camp ids
  carry `CAMP_FLAG` (`is_camp`) so they never name a village; `routine::settlement` builds a
  camp as a `Settlement` with no lots (`Settlement::camp`), so wanderers sleep, work and gather
  around the fire and talk, trade, take gifts and remember players exactly like residents.
  `camps_between(settings, first, last)` lists them for population and the renderer. Camps are
  not enemy safe zones (WP-NIGHT may want to make lit camps safe).
- **NPC entities** (`world/people.rs`): `NpcState` (character, activity, `inside` lot, target,
  facing, `talking_to`, idle pause). Every `PEOPLE_REFRESH_MILLIS` the residents of every
  settlement an actor is within `SPAWN_MARGIN_COLUMNS` (and `NEAR_ROWS`) of spawn at their
  routine's place; settlements with no actor within `DESPAWN_MARGIN_COLUMNS` empty out
  (`despawn_many`). Every tick each person walks toward their stop at their pace, steps inside
  at a door (hidden) and out again when the routine moves on, potters within the stop's roam with
  pauses, and stands still facing a player who is talking to them. An entity is updated only when
  something changed, so resting people cost no replication; walking people replicate as entity
  deltas and interpolate (`Body::lerp_position`). People have no health and do not feel blasts;
  every settlement is an enemy safe zone.
- **Actions**: `PlayerCommand::People(PeopleCommand)`: `Talk { npc, topic: TalkTopic }` (chat,
  rumours, life, building work), `Trade { npc, offer }`, `Gift { npc, item }`, `Goodbye`, each
  naming the NPC entity. Validated in the tick by `can_reach_npc` (within `TALK_REACH_TILES` of
  the person outdoors, or at the door of the building they are inside) and resolved by
  `people_command` (before `apply_player_command`). Talking runs `people::talk` / `say` on the
  person's saved `CharacterState`; a chat once a day adds goodwill, and a rumour once a day per
  person marks the nearest unknown site as an echo. Word gets around: every change in a
  person's affinity for a player ripples once to their circle (`Population::reputation_ripple`:
  family half, friends and sweethearts a third, rivals a quarter the other way). **Actor:**
  `WorldState::people_actor_position(player)` is `actor_position`: the pilot's middle while on
  foot (T15), else the rig, so a pilot talks to people on the street or at a door.
- **Offers** (`people::offers`): `offers_of(character)` rolls each person's trade list from their
  role offer: `PersonDeal::Goods` (a one-of-a-kind rig part, a patch-up, a tonic that is a timed
  `Blessing`, cosmetics, a pack of charges), `MapSecret`, `Rumour`, `Apprenticeship(Perk)`
  (`Haggler`: 90 % prices at village counters and with people; `Surveyor`: one more site per
  rumour or secret; `Charmer`: goodwill half again). `price_for` scales by standing (grudge
  125 %, friends 90 %, close 80 %), `standing_allows` gates rows, `gift_reaction` and
  `favourite_mineral` judge gifts. Goods resolve through `apply_goods` (shared with village
  counters).
- **Contractors** (`people::contractor`): `contractor_profile(character)` (builders' trades only):
  `ContractorStyle`, speed / quality / price; `ContractorProfile::quote(BuildJob)`,
  `ContractorHire` (with `progress_per_mille(now)`, world time only) for WP-HOME to keep, and
  `pitch` for the talk panel.
- **Ledger** (`people::ledger`): `WorldState::people()` is the `PeopleLedger`: a `CharacterState`
  per character (each character's memory of each player: reputation is the affinity,
  `-100..=100`, banded into a `Standing`) and a `PersonJournal` per player (the open
  `Conversation`, so every peer shows the same lines; one-of-a-kind purchases; perks; daily
  rumour and chat marks). Player ids survive parking and rejoining, so everyone remembers a
  returning player. Saved with the world (serde default), replicated as the public `People`
  section, hashed only once non-empty (worlds without people keep their hashes).
- **Events** (netcode codes 140-145): `PersonTalked`, `PersonTraded`, `PersonGifted`,
  `AffinityChanged`, `RumourMarked` and the generic `PeopleInteraction`.

Tests: `world/people_tests.rs` (population around rigs, the town and camps, routines by the clock,
deterministic walking, reach, dialogue variety and goodwill, trades and their gates, gifts,
word spreading to family and friends, persistence through saves and parking, replication of
people and conversations, safety, tick-rate safe routines) and the unit tests in the modules
above.

## Wave 3 seams (T17 people)

- **Clock:** wired at integration: `WorldState::people_clock()` reads T14's `WorldClock`.
- **Pilot:** wired at integration: `WorldState::people_actor_position()` is `actor_position`.
  Interaction is one command (`PeopleCommand::Talk { npc, .. }`) whatever the actor.
- **Contractors:** `people::contractor` is ready for WP-HOME (T24) to hire.
- **Formats:** `PROTOCOL_VERSION` 11 (NPC entities, `PlayerCommand::People`, the `People`
  section, events 140-145) and save format 9 (the `people` ledger; version 8 files load).

## Wave 2 seams (integration notes)

Where the wave 2 tracks meet, for the tracks that build on them:

- **Generators V6 and V7:** caves (`has_cave_biomes`) and villages (`has_villages`) both switch
  on with `GeneratorVersion::V6`; the V6 golden digest pins caves and villages together. V7 adds
  the surface biomes (`has_biomes`, T8) on top: the biome heightmap replaces `raw_surface_row`,
  its flat sites are the village sites and feed `site_landscape`, village stamps reach shelves
  above row 0, and caves keep clear of villages and halls. The feature layers are `Cave = 8`,
  `Village = 9`.
- **Unlimited altitude (R17):** no flight ceiling anywhere (fuel is the limit); code that walks
  rows uses `Terrain::min_y()` (enemy movement treats only rows above it as open sky, enemy fog
  in the renderer starts at the local ground), and parked rigs keep their altitude.
- **Damage:** `WorldState::damage_entity` (`crate::combat`) is the one entry point. Enemies
  report a `Health` view and are routed to `damage_enemy`; blasts reach enemies only through
  `enemies_feel_blasts` (scares and EMPs included) and other damageable kinds through
  `damage_from_blasts`.
- **Safe zones:** `enemy_safe_zone` = surface strip, town, infrastructure, every claim
  (`Bases::is_safe_zone`) and every village or hall (`streamed::in_village_area`). Claims may
  not overlap a village (`BuildRefusal::InVillage`).
- **Loot:** enemy kills and cave caches spill through `spawn_loot` (the drop rules); weapons
  drop the blocks they shoot out (`weapon_broke_tile`); every broken tile drops, cave rock
  included.
- **Persistence:** every per-player field the tracks added lives inside `Player` (hopper,
  collect toggle, weapon rack and mount, explosive items, village journal, found caves,
  cosmetics) and so parks and unparks with the rig; claims and home points stay keyed by the
  player id; parked players keep their armed C4 (`remove_player_with`).
- **Event codes** (netcode): drops 60-63, caves 70-72, villages 45-46, enemies 80-87, weapons
  90-95, bases 100-102. `PROTOCOL_VERSION` 10 (9 for wave 2, 10 for T8's V7 worlds, surface tiles at codes 0x500 and
  chunks above row 0) and save format 8 (unchanged: an older build rejects a V7 save by its
  unknown generator) cover all of wave 2.

## Wave 3 seams: the sky (T16)

- **Generator V8** is current: V7's ground plus the sky (`has_sky`); `Terrain::min_y` of a V8
  world is about -11,300 (the Moon plus space). Code that allocates per row from `min_y` must
  clamp (the escort path search stops at the ground layer's headroom); per-chunk and
  radius-limited loops need nothing.
- **Gravity, air and wind are per place** (`environment::environment_at`); new movers should
  scale their gravity with `Environment::gravity` (or `BodyParams::at`) so they behave in orbit
  and on the Moon. The pilot (T15) and sky enemies (Wave 4) hook in there.
- **Sky lands API** for renderers and content: `worldgen::sky::{moon, moon_column,
  lands_touching, land, updrafts_touching, updraft_at}`, the band constants and
  `sky::SkyBand`. Wave 4 fills continents (`SkyLandKind::Continent`) and the Moon with content
  (towns, ores, dungeons) inside their footprints.
- **T14 (sky rendering)**: wired at integration: `moon_anchor()` follows `moon_column`, so the
  clock's phased Moon backdrop hangs above the real Moon from the ground; the
  game's `rendering/world/sky_lands.rs` already draws cloud seas, wind and updraft cues and the
  Moon looming from the continents up, and stretches the altitude colour bands over V8's bands
  (`gradient_y` / `world_y_of_gradient`); the backdrop Moon fades out as the looming one fades in.
- **Event codes** 116-118 (altitude), `PROTOCOL_VERSION` 11; save format unchanged (an older
  build rejects a V8 save by its unknown generator; `HazardGear` fields default).

## Wave 3 seams: the pilot (T15)

- **Clock** (T14): nothing in the pilot reads the clock; its helmet lamp lights the night light
  map in the game. Night danger and shop hours can gate on `is_on_foot` / `pilots()`.
- **Air and gravity** (T16 / F6, WP-MOON), wired at integration: `WorldState::pilot_air_at` is
  the one place air is decided; on a V8 sky the air is thin below
  `pilot::PILOT_THIN_AIR_DENSITY` of `environment::air_density` (about 800 rows up, and the
  vacuum of orbit and the Moon), older worlds keep `THIN_AIR_ROW`. `PilotTerrain::gravity_at`
  (default `pilot::GRAVITY`) is read once per tick; the world answers
  `environment_at(..).body_gravity(GRAVITY)`, so gravity fields and updrafts pull on the pilot
  (jump speed stays, so the pilot leaps about three times as high on the Moon and floats in
  orbit; the jetpack lifts harder). Ground gravity is exactly `GRAVITY`.
- **Sleep** (T14): beds judge the sleeper's body by `actor_tile`, so a pilot can lie down on
  foot and gets up by walking off.
- **People** (T17): NPCs can stand in walk-in rooms; `actor_position` is where a talking player
  is. Quests can require the pilot with `poi::SpaceAccess`.
- **Rewards** (T18): `PilotGearKind` tiers are purchases like upgrades (`BuyPilotGear`).
- **Balance** (T19): pilot tuning lives at the top of `pilot.rs` (speeds, jump, fall damage,
  oxygen, health, pick seconds, pack sizes, gear prices).
- **Generators** (V8+): `WorldState::carve_crawlway`'s shape (grates at both ends of a 1-tile
  tunnel) is what a generator stamps for crawlways; V1-V7 output is unchanged.
- **Formats**: `PROTOCOL_VERSION` 11 (pilot entity, `ControlState::jump`, the new commands,
  services, events 120-133, pilot gear and pack in the loadout, vent grates, ropes) and save
  format 9 (version 8 files load unchanged).

## Replication

Every field belongs to exactly one section kind (the compiler enforces it through exhaustive
destructuring in `section`, `remove_player`, `redact_player` and `split_player`):

| Kind | Contents | Who receives it |
|---|---|---|
| Per tick | rig pose, velocity, fuel, hull, weapon state (capacitor, cooldown, aim, trigger), drill/scanner timers, deep-run pressure; the charge serial and entity id/revision counters (`LiveEntities`) | clients the rig is relevant to |
| Per tick, delta | entities (gas clouds, boulders, drops, charges, projectiles, targets, ...): records of entities new to the client or changed since its last frame (by revision), and despawns (gone or out of relevance) | clients the entity is relevant to |
| Op log | terrain tile writes | everyone, filtered by 16x16 terrain chunk relevance (stale chunks are refreshed with `terrain_chunk`: the chunk's diffs from generation) |
| Op log | per-player discovery | the owner only |
| On change | `RigBuild(player)`: upgrade levels drawn on the rig (drill, engine, cargo bay, hull, radiator, scanner, bulkheads, drill mount), the rig's cosmetics and its mounted weapon | everyone |
| On change | `Loadout(player)`: cargo bay and block hopper, credits, upgrades, weapon rack, kits, rig parts, POI journal, quest log (solo quests, reputation, blueprint fragments), salvage, rescue count, deep reward milestone, service transactions, cosmetic unlocks | the owner only |
| On change | economy, contracts, progression (incl. `won_game`), town, world events, infrastructure, markers, RNG, timers (incl. twin-relay vault deadlines), session constants (tick timing, player template, world settings), crew quests, the enemy director (difficulty, rig i-frames, spawn cooldowns, cleared lairs) | everyone |
| On change | `RigBuild(player)`: upgrade levels drawn on the rig (drill, engine, cargo bay, hull, radiator, scanner, bulkheads, drill mount) and the rig's cosmetics | everyone |
| On change | `Loadout(player)`: cargo bay and block hopper, credits, upgrades, kits, rig parts, POI journal, quest log (solo quests, reputation, blueprint fragments), salvage, rescue count, deep reward milestone, service transactions, cosmetic unlocks | the owner only |
| On change | economy, contracts, progression (incl. `won_game`), town, world events, infrastructure, markers, RNG, timers (incl. twin-relay vault deadlines), session constants (tick timing, player template, world settings), crew quests, bases (claims, modules, fixtures, homes), the world clock (`SectionKey::Clock`: day length, skipped nights, sleep rule, sleepers), people (character memories, conversations, perks) | everyone |

Because other players' private state never reaches a client, replica checks compare
`visible_state_hash(viewer)`: private state is hashed only for the viewer and per-player data is
hashed player by player, so host and a correct client authority agree exactly. Terrain
dimensions, settings and the tiles that differ from generation travel only in keyframes
(`redacted_for(viewer)`); the receiver regenerates the rest.
