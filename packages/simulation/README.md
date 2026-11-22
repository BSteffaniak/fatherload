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
`player.rs` (`cargo_capacity_for_level` 12 stock, +10, +15, +20, ... ~600 at the top;
`fuel_capacity_for_level` 450 stock, +250, +370, +490, ... ~14,900 at the top;
`hull_capacity_for_level` ~2,500, `engine_power` ~4x thrust with climb speed capped; horizontal
top speed follows `world::engine_top_speed_tiles`, ~5x stock at the top tier).

**Fuel and cargo, second curves (playtest fix).** Fuel should only bite on deep and long runs and
sky expeditions, not everyday trips: the yardstick is the balance harness's everyday trip
(`balance::trip`: drill 50 rows down, mine two minutes, fly home), which burns about 160 fuel,
about 35 % of the 450 stock tank (it could not be flown on the old 100). Each tank tier adds far
more than before and the top tank holds ~14,900 (was ~3,000); burn rates
(`fuel_burn_efficiency`, `AUTHORITATIVE_FUEL_BURN_PER_SECOND`, 0.45 per tile drilled) are
unchanged. The cargo bay keeps its 12 stock slots but grows +10, +15, +20, ... (~600 at the
top, was ~300), and the block hopper grows 30 per bay tier (`BLOCK_HOPPER_PER_TIER`, was 20).
The old curves stay as `fuel_capacity_v2` / `cargo_capacity_v2` for migration.

Capacities are stored, so a save from an older curve (the five-tier curves, or the first
fifteen-tier curves) keeps its levels; loading it (`WorldState::catch_up_capacity_curves`, run
by the game's save loader, and therefore by a host before it serves the world online) lifts the
tank, bay and hopper onto the current curve for the fitted tiers, keeping bonuses earned on top
(auxiliary tanks, cargo racks), and scales the fuel on board with the tank (half full stays half
full). Running it again changes nothing. Levels load whatever their value. **Code that builds a
rig by hand must take capacities from these functions** (or `Player::new`), never literals like
100 fuel / 12 slots: the stock tank is now 450.

Hazard counters (`protection.rs`, `Player::hazard_gear`, private loadout): Heat Shielding,
Lava Plating, Gas Scrubber, Pressure Bracing (pressure pockets, explosive pockets, magma vents)
and Boulder Deflector (boulders, cave-ins). Each stops `1 - (1 - tier/max)^2` of its hazard, all
of it at the top tier. Heat is per band, not per row: each band has a heat level
(`band_heat_level`, 0 in the topsoil and clay up to 10 in the mantle storm zone) and the rig's
cooling (`heat_cooling`, counted in thirds of a level: a whole level per radiator tier to
tier 5, a third per radiator tier after that, a whole level per shielding tier) covers that
much; each uncovered third burns `HEAT_DAMAGE_PER_EXCESS_UNIT` hull per second (3 per level). Deep-run instability builds from the
basalt deeps down and the pressure fuel drain/scanner lockout starts at the fossil oceans (both
by band). Boulders loosened by mining are one per 400 stone tiles (was 80) with a 2 s warning
(was 1.5 s), rarer still for late drills (`boulder_spawn_chance`: odds fall with the square of
the drill's bite, so a top drill loosens no more boulders per minute than a tier 5 one).

**Falling boulders and cave-ins (`world/hazards.rs`, `world/entities.rs`, playtest fix).** Rules
every boulder source follows (new ones must too):

- *Placement*: a boulder only ever hangs in open ground with an open tile under it to fall
  through (`WorldState::boulder_hang_tile`). A mining boulder hangs in the open tile over the
  cut (the cut itself under a ceiling, else the tile above it); a sideways cut in a
  one-tile-high tunnel loosens none. A cave-in (`spawn_cave_in`) hangs up to three boulders in
  the rig's column and its two neighbours, up to three rows up (under the ceiling when it is
  that close); a column solid at the rig's height or without room to fall gets none, so a
  one-high tunnel gets no cave-in. Boulders hang `BOULDER_RADIUS` (11 px) under the top of their
  tile. The old spawns put mining boulders in the rock over sideways cuts and cave-ins two rows
  up regardless of rock; those "shattered" the moment their warning ran out.
- *Impact*: a falling boulder shatters on a rig or when its lower edge (`y + BOULDER_RADIUS`)
  meets solid ground, leaving its rubble on top of what it hit; nothing else breaks it.
- *Settled ground* (`hazard::SettledGround`, `CanonicalProgressionState::settled_ground`, saved,
  hashed only once non-empty, replicated in the timers section): every collapse settles its
  8 x 8-tile cell for `SETTLED_GROUND_MILLIS` (10 min; was 3), and no random collapse starts in or next
  to a settled cell. Random collapses go through `loosen_ground` (underground only: 3+ rows under
  the local surface): mining boulders, collapse surges, deep-run cave-ins, earthquakes and
  stalactites (`try_cave_in` for cave-ins). Nothing calls `spawn_cave_in` directly any more (an explosive pocket's
  blast goes through `try_cave_in` too), so no boulder ever starts above ground. The Star Core
  escape is gone: it rained a cave-in on the first player every 750 ms wherever that rig went
  (the surface included), filled the tunnel in under it and wrecked every rig when its hidden
  two minutes ran out. Freeing the core now only emits `EscapeStarted`; a countdown carried in
  by an old save or replica is dropped (`clear_legacy_escape`). An earthquake's one cave-in
  falls over the deepest rig, not the first player.
- *Rates*: a collapse surge rolls a cave-in every 5 min (`COLLAPSE_SURGE_PERIOD_MILLIS`; was
  every 3 s for its whole three-to-four market days, the "falling over and over" of the
  playtest, then every 90 s); deep-run instability builds at a quarter of its first rate
  (`DEEP_INSTABILITY_BUILD_SCALE`), a full meter empties (was: back to 45, refilling within a
  minute or two) and its cave-in hurts only through its boulders (the 4-hull jolt is gone); one stalactite in three is loose (`stalactite_is_loose`, a fixed per-tile
  roll; the rest never fall); cave-in warnings are 1.5 s plus 0.25 s per column out (were 0.9 s
  and 0.2 s). The renderer adds ceiling dust that thickens and a harder shake near the drop.
- `boulder_frequency_report` (`cargo test -p fatherload_simulation --release
  boulder_frequency_report -- --ignored --nocapture`; 4 seeds, the economy bot mining 20 min,
  rigs parked or roaming 30 min; every seed's world hits a collapse surge about 16 min in)
  before / after this fix:

  | Scenario | Boulders per hour before | after | Spawned in rock before | after | Broke without falling before | after |
  |---|---|---|---|---|---|---|
  | Mining (prospector bot, one seed hit a collapse surge) | 407 | 13.5 | 153 | 0 | 176 | 0 |
  | Mining (shaft bot) | 9 | 0 | 8 | 0 | 8 | 0 |
  | Parked under a collapse surge | 3,600 | 60 | 0 | 0 | 0 | 0 |
  | Parked in the crystal faults (deep-run pressure, world events) | 1,866 | 30 | 0 | 0 | 0 | 0 |
  | Roaming the crystal faults (a fresh pocket every minute) | not measured | 72 | - | 0 | - | 0 |

  The roaming row is the worst case for settled ground (the rig always stands on fresh ground
  in a room tall enough to hang boulders; a one-high tunnel gets no cave-in at all). With a
  45 s surge roll and the old instability build it was 102 per hour, a cave-in about every
  minute and a half for the whole run, which is why both were cut further. With the second
  fuel and cargo curves on top (bots stay down longer on the bigger tanks) the mining rows read
  15.0 (prospector) and 2.2 (shaft) per hour; the parked and roaming rows are unchanged, and
  nothing spawns in rock or breaks without falling.

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
| `night` | night-time gameplay (WP-NIGHT): night falling and lifting, night monsters rising round exposed actors and prowling, the rare new-moon spawn, burrowing at dawn, night safety (`night_safe_zone`, `hostile_safe_zone`, palisades), lures, nocturnal ore (see [Night-time gameplay](#night-time-gameplay-srcnightrs-srcworldnightrs)) |
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
| `tombs` | tomb and crypt runs (WP-TOMBS): seals, entering as a party, writing floors into the run vault, ambushes, the guardian, traps, shrines, caches, relic picks, downed members and revives, wipes, unlocks and summaries (see [Tombs and crypts](#tombs-and-crypts-srctombrs-srctomb-srcworldtombsrs-srcworldgenstreamedtomb_sitesrs-generator-v10)) |
| `homes` | contractor-built homes (WP-HOME): deeds at land offices and surveyed spots, contracts stepped in world time, site crews, decorating, beds, the garage bay, cellar and lift (see [Contractor-built homes](#contractor-built-homes-srchomers-srchome-srcworldhomesrs)) |
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

**The climb tables** (`climb.rs`): a `ClimbTable` `[tank][engine][intake]` holds the highest
altitude (rows above row 0) a rig of each fuel tank tier (1-15), engine tier (1, 4, 8, 12, 15)
and altitude intake tier (0, 2, 4, 6) reaches holding thrust from rest 100 rows up in still
air (the density profile, standard gravity, no wind or updrafts). There are two:
`CLIMB_CEILINGS` (the current fuel curve; read by the gates, `climb::ceiling` / `climb::reach`)
and `V8_CLIMB_CEILINGS` (frozen: measured on the first fifteen-tier fuel curve, the table V8's
bands were calibrated from). `world::climb_tests::climb_table_matches_the_simulation`
re-measures a sample of the current table against the real movement step, `climb_table_report`
(ignored, run in release, one thread per tank tier) prints it, and
`the_table_unlocks_the_sky_in_order` pins the design claims for both tables. Without intakes no
tank climbs past about 5,250 rows (thin air); with them the second fuel curve climbs about four
times higher than the first at the top (a maxed rig 54,140 rows, was 12,150).

**Band tables per generator version** (`worldgen::sky::SkyBands`,
`GeneratorVersion::sky_bands()`; `has_sky()` is `sky_bands().is_some()`): each sky generator has
its own frozen band table, and placement (`lands_touching`, `land`, `moon`, updrafts,
`streamed::top_row`) reads the world's, so rebalancing fuel never moves an existing world's
islands. `SkyBands::calibrated(&table)` is the rule (fractions of a climb table, below);
`V8_SKY_BANDS` holds V8's values as literals (`v8_bands_are_frozen` checks they equal
`calibrated(&V8_CLIMB_CEILINGS)`; the V8 golden digest is unchanged). **V9's sky** (adopted at the wave 4 integration on top of the playtest fixes): the literal
next bands on V8's physics (500 / 1,350 / 4,720 / 35,500 / 39,680 / 46,010) put the sky
continents and the Moon above 2^20 world pixels, where `f32` positions resolve only 1/8 px: the
pilot stuck on the Moon facility's ledges and the Moon precision test failed. So V9 steepened
thin-air burn for its worlds and re-measured, as planned: `environment::V9_THIN_AIR_BURN` = 30
(V8 keeps `THIN_AIR_BURN` = 8; `Environment::thin_air_burn`, `thin_air_burn_of(generator)`),
`climb::V9_CLIMB_CEILINGS` measured on it (`v9_climb_table_report`; a maxed rig climbs 18,080
rows, without intakes 5,240), and `V9_SKY_BANDS = calibrated(&V9_CLIMB_CEILINGS)` frozen as
literals: low isles 310, archipelago 740, dead air 4,710, continents 12,010, orbit 13,430, Moon
15,360 (`v9_bands_are_frozen`, which also checks the Moon's top stays under 2^19 px, the 1/32 px
regime V8's sky and every pilot space were built in). `NEXT_SKY_BANDS` is the same calibration
(equal to V9's today, `next_bands_are_pinned`). Pacing on V9: a stock rig climbs 330 rows (the
lowest isles' turf is at 350: the first tank upgrade, 340 cr, reaches them), a tier 4 tank and
engine reach the archipelago, the cheapest dead-air crossing is tank 13 / engine 8 / intakes 6
(203,910 cr) and the Moon's top needs tank 14 / engine 12 / intakes 6 (or tank 15 / engine 8).
**Every reader goes through the world's bands**: `SkyBands::climb` is the climb table a sky's
gates compare with (V8: today's `CLIMB_CEILINGS`; V9: its own), read by
`milestone::climb_reach` / `island_capable` / `crosses_dead_air` / `sky_refuel_level`, the tank
previews and the second-act probe; the environment (vacuum fade at the orbit floor, cold,
gravity fields, wind bands), `SkyBand::at_row_of(settings, row)` (altitude events, sky enemy
tiers, the hostile sky, the HUDs), sky caches, buoys, collapsing isles, sky cities and lakes,
quest isles, the Moon anchor, the save fingerprint, the map, tile art, audio, the sky renderer
and the sky scenes all take the world's `SkyBands`. `SkyBand::at_altitude` / `at_row` /
`bottom_rows` and the `*_BOTTOM_ROWS` constants are V8's only. Breathing and the hull's seal
read **breathable air** (`Environment::breathable`, `environment::breathable_air`): the
density at the matching height of a V8 sky, so every sky's continents hold air and its orbit is
vacuum; thrust, burn and drag still read the measured density profile (`air_density`).

**Bands** (`sky::SkyBand`, `SkyBand::at_altitude` / `at_row` in a V8 sky, `at_altitude_in`
for any `SkyBands`; rows above row 0 in `worldgen::sky`), V8's: low isles from `LOW_ISLES_BOTTOM_ROWS` (65 % of a tier 2 tank + tier 4 engine,
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

### Wave 4 integrated: generator V9 and the shared rules

The seven wave 4 tracks (T20-T26) land as one generator and one format bump. The sections
below describe each track; this one lists what the integration unified.

- **One generator V9** (`GeneratorVersion::V9`, current until V10 added tombs): V8 plus the furnished low isles,
  archipelagos, sky towns and jet streams (`has_sky_isles`, T20), the filled sky continents
  (`has_sky_content`, T21), the filled Moon (`has_moon_content`, T22) and pilot-only spaces in
  the ground (`has_pilot_spaces`, T23). V1-V8 output and digests are unchanged. V9 output is
  pinned by four digests: the isles (`worldgen::sky::isle_tests`), the continents
  (`worldgen::sky::continent::tests`, re-pinned at integration because the continent tiles
  moved), the Moon (`worldgen::moon::tests`) and the pilot spaces
  (`worldgen::streamed::pilot_spaces::tests`). Changing what V9 generates now needs a V10.
- **Codes**: sky tiles 0x1D-0x25 are the isles' (T20), 0x26-0x30 the continents' (T21);
  built tiles 8-69 are the ruin pieces (T23) and 70 the palisade (T25); weapon item ids 7-10 are
  the Act II weapons, 11-13 the Moon's and 101-105 the pilot's arms; artifacts and strategic
  materials are appended in track order. Event codes: 200-203 sky isles, 210-219 continents and
  sky enemies, 220-229 the Moon, 230-239 the pilot, 240-249 homes, 250-253 the night, 260-265
  stories. `PROTOCOL_VERSION` 12 and save format 10 cover the whole wave (versions 8 and 9
  load).
- **The pressurized hull is one ladder** (`HazardGear::pressurized_hull`,
  `moon::PRESSURIZED_HULL_MAX_LEVEL` = 3): tier 1 is the cold seal a cloud farm crafts from
  cloud silver (T20) or the shop sells, tier 2 the vacuum seal the Moon needs
  (`moon::VACUUM_SEAL_TIER`) and tier 3 the void-rated plating (`moon::HULL_ARMOUR_TIER`).
  Every tier keeps one cold level out. The shop line starts at 4,800 cr (the vacuum seal and
  plating keep T22's 12,000 and 30,000). The intake, de-icer, pressurized hull and life support
  are all locked as `UpgradeLock::NoSky` on worlds without a sky; so is the pilot's thermal
  lining.
- **One pilot mechanism**: sky temple fittings (T21) are `mechanism::Usable::TempleFitting`, so
  the use button (`PilotAction::Use`), the prompt and the touch USE button work them like ruin
  levers, chests and keycard doors (T23), and a pick swing still does
  (`WorldState::work_temple_fitting` is the one entry). Temple keystones ride the key ring like
  keycards (`mechanism::rides_key_ring`). One `LeverClunk` sound serves every lever.
- **Safe zones**: `enemy_safe_zone` answers the Moon first (`lunar_safe_zone`: the colony and
  home plots), then the hostile sky (`sky_safe_zone`: sky cities, sky towns through the village
  rule, infrastructure, claims and home plots), then the ground. Night monsters ask
  `night_safe_zone`, which also keeps home plots safe. A finished lunar habitat holds air and
  warmth (`WorldState::in_lunar_habitat`). The pilot's cold exposure spares the Moon's heated
  rooms, habitats, sky town and sky city streets and temple halls (`WorldState::pilot_sheltered`).
- **People**: sky cities (T21) are settlements of the sky towns' cultures (T20,
  `Culture::Aeronaut` / `Cloudborn`) with the sky occupations.
- **Stories** (T26) bind a Moon place at the lunar colony and a sky continent place in its sky
  city on V9 worlds.
- **Balance**: `balance::second_act` (`--balance-report --only second-act`) prices the climb
  gates (low isles, dead-air gap, the Moon checklist), the sky refuel ceilings, the Act II and
  Moon weapons, the pilot's Moon kit and homes, in credits and minutes of late-game mining.

**Rebased onto the playtest fixes** (reconciliation, see also the sky section above):
- *Sky bands*: V9 has its own bands, thin-air burn and climb table, and nothing in wave 4 reads
  V8's band constants any more; V9's isle, continent and Moon digests were re-pinned (V1-V8
  unchanged).
- *Dead-air gate*: buoys (720 fuel, a third of the tank plus 720), sky town and base refuels
  (`sky_refuel_level` against the world's bands and climb table) and the second-act probe are
  re-measured on V9's table: no rig short of the gap is carried over it by sky fuel.
- *Enemy bodies*: every wave 4 kind moves through the shared enemy step (`step_body`,
  `free_from_rock`, `keep_off_bodies`) and has a real `EnemyKind::contact_half` sized to its
  art; airship turrets riding their hull take the nearest open spot when the mount point would
  sit in rock; dune stalkers den in the crust they hunt in and bite through it rarely
  (`diggable_by`, `BITE_MILLIS`); spawns search up to four tiles for open space. `world::enemy_body_tests::
  wave_four_families_keep_their_bodies_in_their_habitats` soaks the sky families on a cloud
  archipelago isle, the night monsters on the night surface and the Moon's creatures on the
  regolith. Pilot weapons go through `launch_projectiles` and the Act II and Moon guns through
  the projectile and ray paths, so all get the hardpoint-to-muzzle sweep; the gravity gun also
  grabs point-blank targets.
- *Boulders*: a collapsing isle sheds a rock only from a tile with open air under it
  (`boulder_hang_tile`), centred in the tile it emptied.
- *Formats*: `PROTOCOL_VERSION` 12 and save format 10, one bump each over master's 11 / 9;
  `catch_up_capacity_curves` still runs on every load.


### Sky isles and sky towns (`worldgen/sky/{isles,towns,winds}.rs`, `world/sky_isles.rs`, `skycraft.rs`, generator V9)

Expansion plan WP-SKY (wave 4, T20). `GeneratorVersion::V9` (current, `has_sky_isles`) keeps
V8's island placement rule in its own, taller bands (`worldgen::sky::isle_tests::
v9_keeps_the_bare_placement_outside_town_cells`; V9's bands and thin air are described under
the sky section above) and furnishes the low isles and archipelago; V8 and older output is
unchanged and `GOLDEN_V9_DIGEST` pins the new output.

- **Island content** (`isles`, `SkyLand::content() -> Option<&IsleContent>`): skystone ore
  (`MineralKind::Skystone`, hardness 2) in every isle's rock, cloud silver
  (`MineralKind::CloudSilver`, hardness 3) deeper in archipelago rock, rare sky artifacts
  (`ArtifactKind::WindAstrolabe`, `CloudPearl`), and per island (rolled from its shape seed) a
  sky cache, a nest on the turf, a ruin (two pillars and a stone floor), a spring at the rim
  (the renderer pours a waterfall from it), an updraft vent, a keystone (a crumbling isle), a
  fuel buoy pad (about half the low isle clusters, on the biggest isle, never crumbling) and
  vines among the roots. New `SkyTile`s (codes 0x1D-0x25): `Cache`, `Nest`, `RuinBrick`,
  `RuinPillar`, `Spring`, `Vines`, `Keystone`, `Vent`, `BuoyPad`. Island cells are memoized
  (`cell_lands`); a buoy's pad is flat.
- **Sky towns** (`towns`): the archipelago's cells form blocks of 3x2 (`TOWN_BLOCK`, 960
  columns by 352 rows); one cell per block (a hash) holds a flat-topped `SkyLandKind::TownIsle`
  with one or two isles beside it and a sky town on it. A sky town is a village
  (`VillageSetting::Sky(SkyTownKind)`, built by `village::layout::build` from
  `SiteSpec::Sky`) with id `placement::sky_town_id(bx, by)` (`SKY_TOWN_FLAG`, bit 30), so every
  village API (lookups, `village_at`, `village::sky_towns_in` for 2D areas (column queries like
  `villages_between` keep to the ground), `in_village_area` safe zones, regional
  `FeatureKind::Village` features with `ry >= SKY_TOWN_RY`, people routines, NPCs, counters,
  interiors, the journal) works for them. Kinds: airship dock, floating market, sky monastery,
  cloud-farmer village (`sky::SkyTownKind`), in two sky cultures (`Culture::Aeronaut`,
  `Culture::Cloudborn`, `layout::sky_culture`) whose residents roll from `Role::SKY_POOL`
  (with the sky occupations `Role::Aeronaut`, `Monk`, `StormChaser`, `Smuggler`; ground
  cultures keep `Role::ALL`, so old worlds' people are unchanged). Specialty buildings
  (`BuildingKind::AirshipDock`, `Bazaar`, `Monastery`, `Granary`; `VillageService` likewise):
  fuel and aether thrusters; skystone and cloud silver bought at 160 %+, a smuggled part and a
  double pack of charges; blessings, a cell for the night and cloud-silver de-icers; cheap
  condensed fuel and the pressurized hull. Lookups: `sky_town_site`, `sky_towns_touching`,
  `sky_town_at`, `towns::sky_towns_between`, `towns::sky_town_of`.
- **Sky crafting** (`skycraft`): `VillageDeal::SkyCraft { recipe, tier }` rows (every tier,
  only the next sells) cost credits **and** sky ore from the bay (`skycraft::cost`):
  `SkyRecipe::AetherThrusters` (tiers 1-3, `HazardGear::aether_thrusters`: in thin air they win
  back side thrust, `aether_drive_factor`, and grip the wind, `aether_wind_grip`; never lift),
  `PressurizedHull` (tier 1 of 3, `HazardGear::pressurized_hull`, keeps out one cold level per
  tier: `HazardGear::cold_cover`/`cold_excess`; tiers 2-3 are a seam for T21/T22) and `DeIcer`
  (the shop line's tiers at 60 % of the credits). Test mode maxes both new fields.
- **Fuel buoys** (`BuoySite`, `isles::buoy_of`, `WorldState::buoy_at_rig` / `buoy_used`):
  `SessionServiceRequest::SkyBuoy { isle }` from a rig parked on the pad buys one refill per
  player per buoy (`VillageJournal::buoys`, serde default) for `BUOY_PRICE` (450): at most
  `BUOY_FUEL` (720, sized for the second fuel curve: a tier 1-3 tank fills right up) and never
  past `buoy_fill_level` (a third of the tank plus 720). A rig
  leaving a buoy is never better off than one that climbed to the band's top on a full tank,
  so buoys cannot open the dead-air gap; `world::sky_isle_tests::
  buoys_never_open_the_dead_air_gap` measures it for every near-gap loadout of the climb table
  (and `milestone::crosses_dead_air` stays table-only).
- **Sky refuels are rationed** (`milestone::sky_refuel_level`): sky town counters, sky
  traders' metered fuel and base fuel stations in the island bands (low isles up to the
  continents' floor) top a rig that cannot cross the dead-air gap up only to the share of
  its tank a full-tank climb from the ground would still hold there (the share of its
  `climb_reach` still above it), and at least `SKY_REFUEL_FLOOR_PERCENT` (25 %). Without it a
  full tank bought at a high sky town carried tier 10-12 tanks with intakes over the gap.
  Rigs that already cross fill right up. `world::sky_isle_tests::
  sky_refuels_never_open_the_dead_air_gap` measures every near-gap loadout from the highest
  sky town and from mid-archipelago.
- **Collapsing isles** (`world/sky_isles.rs`, stateless): an island whose `Keystone` tile is
  gone (drill, blast, gravity bomb, gun, pick) sheds its lowest tiles every
  `COLLAPSE_STEP_MILLIS` near rigs: ore and finds fall as drops, rock as falling boulders;
  half gone, it cracks the keystones of crumbling isles within `CHAIN_RADIUS` (chain
  reactions). Keystones broken in a tick raise `IsleCollapsing` (read from the terrain
  journal, so every way of breaking one counts).
- **Wind-riding** (`winds`): every archipelago chain carries a `JetStream` (a 10-row ribbon a
  few rows over its highest turf, `JET_STREAM_SPEED` 480 px/s one way, easing at the ends; it grabs an airborne rig with `JET_STREAM_GRIP` whatever the air's density) and
  every vent breathes a column of rising air (`vent_updraft_at`, `VENT_RISE_ROWS` 60,
  `VENT_STRENGTH` 860). `environment_at` adds both on V9 (`wind_x`, `updraft`).
- **Events** (netcode 200-203): `SkyCacheOpened`, `BuoyRefuelled`, `IsleCollapsing`,
  `JetStreamCaught`; `UpdraftCaught` also fires for vents. Sky crafting reports through
  `VillageServiceUsed`.
- **Shop** (open items from wave 3): `economy::upgrade_lock` (`UpgradeLock::{Band, NoSky}`)
  locks the Altitude Intake and De-Icer on worlds without a sky (shop, purchase check and the
  balance bots read it); `upgrade_preview::upgrade_preview_in` / `preview_between_in` take
  `has_sky` and drop the island and dead-air lines on such worlds.

Tests: `worldgen/sky/isle_tests.rs` (V9 switch, V8 placement kept, content, tiles where the
content says, seamless deterministic chunks, one flat town per block with its village,
regional village features, jet streams and vents, the golden digest), `world/sky_isle_tests.rs`
(caches, buoy economics and the dead-air gate, saves, sky crafting at the dock, every town
kind's offers, a gravity bomb collapsing a cluster, isles standing otherwise, jet stream
riding, determinism and keyframes) and `skycraft.rs`.

### Sky continents, sky enemies and Act II weapons (Wave 4 T21, generator V9)

`GeneratorVersion::V9` (current, `has_sky_content`) is V8 with the sky continents filled in;
everything else (ground, islands, the Moon) generates exactly as V8
(`worldgen::sky::continent::tests::only_v9_fills_continents_and_v9_matches_v8_elsewhere`), and a
V9 golden digest pins the continents (`chunk_digests_of_v9_continents_are_pinned`).

**Continents** (`worldgen::sky::continent`): every V9 continent `SkyLand` carries a
`ContinentPlan` (`SkyLand::plan`), a pure function of its shape seed and geometry. Its width
(less a 30-column rim) is cut into five zones and one of four layouts assigns a **city** (two
zones, flattened street with foundation paving), a **temple** plain, a **mountain** zone (one to
three peaks up to `MAX_PEAK_ROWS`, storm glass veins, frosted summits) and a **lake** (water a
row under the shore, `lake_water_at`, folded into `Terrain::water_at` so pilots swim and the
renderer draws it). The body has winding caves, a bedrock-like **aether core**
(`SkyTile::AetherCore`, `is_unbreakable`) wrapped in **aether crystal**, storm glass, and rare
deep aether veins. Harpy nests (`ContinentPlan::nests`) sit on the tallest summits whose flat
ledge really is the surface (`ledge_nests`), with eggs.
New sky tiles (codes 0x26-0x30): `AetherCore`, `AetherCrystal`, `StormGlass` (break into the
strategic materials `StrategicResourceKind::AetherCrystal` / `StormGlass`), `TempleBrick`,
`TempleDoor`, `KeystoneDoor`, `TempleLever`, `TempleKeystone`, `TempleCache` (all
indestructible: `SkyTile::is_indestructible`, honoured by `is_indestructible`, blasts and
weapons), `HarpyNest` and `HarpyEgg` (breaks into `ArtifactKind::HarpyEgg`); temple vaults pay
`ArtifactKind::SkyRelic`. Queries: `continents_touching`, `temples_touching`, `nests_touching`,
`sky_city`, `sky_city_at`, `sky_cities_between`, `city_of`, `SkyLand::underside_row`.

**Temples** (`TEMPLE`, `TemplePlan`): a 50x23 template stamped on the temple plain (mirrored
at random). A shrine on the surface is entered by crawling through a vent grate
(`poi::SpaceAccess::RequiresPilot`: rigs are stopped); a stair leads to the lever hall, a lever
gate (`D`), the keystone hall (keystone `k` and a sentry), a crawlway down to the vault's
keystone gate (`K`) and relic chests (`C`). `temples_open_in_order_on_foot_and_keep_rigs_out`
walks it with a pilot reachability search. **Fittings** (`world/sky_temples.rs`,
`WorldState::work_temple_fitting`) are worked with the hand pick's swing (no new input on any
device): the lever opens gates within `LEVER_REACH`, the keystone drops as
`item::TEMPLE_KEYSTONE` (quest item 210), a keystone gate spends one from the pilot's pack (or from the rig's hold, where boarding
stows it), a
chest spills `relic_loot` through the drop rules. Events `TempleLeverPulled`,
`TempleGateOpened`, `RelicChestOpened`.

**Sky cities** (`SkyCity`, `people::sky_city`): a long street of towers, halls, a market, a
monastery, hangars and homes (presentation, like village buildings) and a plaza. Each city is a
people `Settlement` (`Settlement::sky_city`, ids with `SKY_CITY_FLAG`) whose residents take sky
trades (aeronauts, cloud monks, cloud merchants, smugglers, storm-chasers...) and live by the
usual routines; `world/people.rs` populates cities near actors like villages and camps.

**The hostile sky and sky enemies** (`enemy.rs`, `world/enemies/sky.rs`): on V9 worlds the sky
from the archipelago up is hostile (`WorldState::hostile_sky`); `enemy_safe_zone` keeps it safe
only in sky cities (`SKY_CITY_SAFE_TILES`), around infrastructure and in claimed bases
(`sky_safe_zone`). New kinds: `SkyRay`, `Wyvern` (swoopers, `AttackStyle::Swoop`: climb to a
perch above the prey, hang, dive, climb away; the wyvern also spits fireballs), `StormElemental`
(`AttackStyle::Lightning`: a telegraphed instant arc; a struck rig is jolted, capacitor drained
and weapon jammed for `JOLT_MILLIS`), `PirateAirship` (elite machine, `AttackStyle::Boarding`:
holds station above the prey and drops `SkyPirate` boarders, jetpack cutlass fighters), its two
`AirshipTurret`s (ride the hull, `TURRET_MOUNTS`, lob `ShotKind::Cannonball`; sink with it) and
`HarpyNester` (guards its nest, `NEST_GUARD_TILES`, until an egg is gone). `sky_roster(band)` and
`sky_tier(band)` set who roams each band and how tough; `DamageKind::Shock` is new (arcs:
machines take more, storm elementals none). The director spawns them each second around actors
in the hostile sky (`spawn_sky_enemies`) and keeps nesters on nests and a sentry and storm
elemental at each temple (`fill_sky_lairs`, cleared like lairs). Gravity fields apply (stunned
flyers fall by `BodyParams::at`); damage goes through `damage_entity`, loot through
`spawn_loot`, Peaceful clears them. `WorldState::spawn_airship` spawns a hull with its turrets
(scenes, tests). Event `SkyEnemyAction { action: Swoop | BoardersDropped | AirshipDowned |
NesterEnraged }`, `LightningArc`.

**Act II weapons** (`weapon.rs`, `world/sky_weapons.rs`): `WeaponKind::ArcRifle`
(`Delivery::Chain`: hits the first target, then leaps `chains` times within `chain_reach`,
losing a quarter a leap), `GravityGun` (`Delivery::Field`: while held, drains energy per second,
pulls drops, live charges, boulders and non-elite enemies to a hold point `HOLD_REACH` ahead
(held enemies are stunned; drops reach the rig and are collected), rips a loose soft block
every `RIP_MILLIS`; on release flings what it holds and hurts thrown enemies), `TetherHarpoon`
(`Delivery::Tether`: a fresh pull hooks rock within range; while held the rig cannot get farther
than the line, `WorldState::apply_tether` after movement, thrust reels in to `MIN_TETHER`;
`RigWeaponState::tether` is per-tick state so it predicts like movement) and `FlakCannon`
(projectiles with `burst_radius`: proximity fuse, bursts on rock or at range, area damage with
falloff; airships take explosive damage x1.8). They are `act_two()`: the shack sells them only
on worlds with sky content and they cost `materials()` (storm glass, aether crystal) on top of
credits. Events `ArcChained`, `GravityThrow`, `TetherAttached`, `TetherReleased`, `FlakBurst`.

Tests: `worldgen/sky/continent/tests.rs` (V9 vs V8, plans, bodies, temples walked on foot,
cities, the digest), `people/sky_city.rs`, `world/sky_continent_tests.rs` (hostile sky and
safe cities, spawning and Peaceful, swoops, lightning jolts, airships with turrets and boarders
sinking together, nesters, temple guards, temple fittings by pick swing, every Act II weapon,
buying with materials, saves, keyframes and identical replay).

### The filled Moon (`moon.rs`, `worldgen/moon.rs`, `world/moon.rs`, generator V9)

Expansion plan WP-MOON (wave 4, T22). `GeneratorVersion::V9` (current, `has_moon_content`) is
V8 with the Moon filled in; everything below the Moon is exactly V8's
(`worldgen::moon::tests::v9_ground_is_exactly_v8`), V1-V8 output is unchanged and a V9
golden digest (`chunk_digests_of_v9_moons_are_pinned`) pins the Moon. The V8 footprint
(`sky::moon`, `moon_column`) is unchanged: on V9 the `SkyLand` is `is_lunar()` and its
`tile_at` / `surface_row` delegate to the layout.

**Layout** (`worldgen::moon::layout(settings)` / `layout_of(land)`, memoized per thread, a
pure function of the Moon's shape seed): `MoonLayout` { `colony` (a `Site` 40 columns each way,
220-380 columns from the centre, flattened), `habitats` (3 sites), `facility`
(`FacilityPlan`: the temple site on the other side of the centre, its `door`, `temple_room`,
`shaft`, `hall`, `pylons` and lighting `order`, `gate`, `arena`, `conduits`, `seal`,
`corridor`, `core_shaft`; `rooms()`, `bounds()`, `in_door`, `arena_center`), `core` (the
chamber's middle, `CORE_DEPTH_ROWS` under the centre's surface), `tubes` (4 `LavaTube`s:
`center_row(x)`, `is_open`, `skylight`, `placed`), `ice_fields`, `forests`, `maria` }. Queries:
`surface_row(x)` (flattened under sites), `colony_area()`, `colony_pressurized(x, y)`,
`pad()`, `tube_at`, `in_core_chamber`, `tile_at(x, y)`. Places never overlap (the test
`layouts_are_deterministic_and_keep_their_places_apart`).

**Tiles** (`moon::LunarTile`, `TileKind::Lunar`, codes `0x700`): regolith, mare dust, lunar
rock, deep rock (the underside's last 28 rows), tube basalt, moon ice, helium-3 regolith, alien
alloy veins, relic caches, crystal stalks and blooms; colony hull, glass and pad, habitat
panels; facility wall and glyph, veil (`pilot_passable`), pylon / lit pylon, gate, conduit,
core seal, core shell and the core. `indestructible()` tiles (colony, facility, core) report
the bedrock hardness and are `terrain::is_unbreakable`: no drill, blast or pick breaks them;
the world changes them by rule (`Terrain::set_kind_by_rule`, crate-only). Breaking a tile
yields `LunarTile::item()` (moon ice, helium-3, alien alloy, lunar relics, crystal lenses, or
the classic block of its hardness). `glow()` is the light it gives off.

**Items**: `StrategicResourceKind::{Helium3, MoonIce, AlienAlloy}` and
`ArtifactKind::{LunarRelic, WardenEye}` (appended; `ArtifactKind::is_lunar`).

**Approach** (`moon::{MoonRequirement, approach_checklist, missing_requirements,
vacuum_breach, vacuum_damage_per_second, vacuum_at}`): two shop lines appended after the
de-icer (offer indices 16 and 17, `UPGRADE_OFFER_COUNT` 18): `UpgradeKind::PressurizedHull`
(2 tiers; tier 2 turns a fifth of Moon enemies' hits) and `UpgradeKind::LifeSupport` (3 tiers,
the pilot's air line `life_support_reach_tiles` 6 / 12 / 24), stored in
`HazardGear::{pressurized_hull, life_support}` (serde defaults). In vacuum (`vacuum_at`: a V8+
sky's air density under `VACUUM_AIR_DENSITY`, never inside the colony's pressurized rooms) a
rig without them takes `VACUUM_HULL_DAMAGE_PER_SECOND` + `VACUUM_CREW_DAMAGE_PER_SECOND` in the
hazard pass; `GameplayEventKind::MoonVacuumBreach` fires on the edge into vacuum. Fuel and
intakes gate the climb physically (the climb table); the checklist names them too
(`MOON_FUEL_TANK_TIER`, `MOON_INTAKE_TIER`, `MOON_DE_ICER_TIER`).

**Progress** (`moon::LunarProgress`, `Player::lunar`, private loadout, serde defaults):
`landed`, `core_reached`, `warden_defeated`, and the colony's one-offs `reactor`
(`burn_factor`, in movement) and `alloy_drill` (`drill_bonus`, in the drill's bite).

**World rules** (`world/moon.rs`): `moon_layout`, `in_vacuum`, `in_colony`, `on_moon`,
`player_in_colony`, `at_colony_counter` (the use button opens the counter: on foot anywhere
in the colony, the rig only over the pad, so elsewhere it still lets the pilot out),
`lunar_air(owner, x, y)` (the pilot breathes in the colony, in the facility
(`MoonLayout::pressurized` = `colony_pressurized` or `facility_air`, which `vacuum_at` also
honours) and within its rig's air line; `breathe` asks it when the air is thin), `warden_awake`, `core_sealed`,
`colony_offer_ready`, `uplink_destination`, `lunar_safe_zone` (`enemy_safe_zone` asks it
first: on the Moon only the colony is safe), `strike_facility` (scenes). The first rest on the
Moon's top emits `MoonLanded` (once per player). `PlayerCommand::Moon(MoonCommand::Colony(offer))`
takes a `ColonyOffer` inside the colony (the rig within `RIG_SERVICE_REACH` when on foot):
`Refuel`, `Repair`, `MeltIce` (`MOON_ICE_FUEL` each), `SellHaul` (`haul_value`), `Reactor`,
`AlloyDrill`, `BuyWeapon` / `UpgradeWeapon` (the Lunar Cutter, the Low-G Mortar; prices
`weapon_price`, credits plus helium-3 or alloy), emitting `ColonyServiceUsed`.

**The way down**: the shafts' ledges cover two of their three columns, alternating sides, so
nothing drops straight through (an 80-row fall in Moon gravity reaches terminal speed and kills
a suitless pilot); the core shaft counts its ledges from `CORE_FOOT_LEDGE_ROWS` (a ledge inside
the shell's veil), and the chamber has a floor (`CORE_FLOOR_ROWS`) and two glowing plinths
(`CORE_SIDE_PLINTH_ROWS`, `CORE_PLINTH_ROWS`) stepping from the heart's crown back up to it
(`world::moon_tests::a_bare_pilot_climbs_down_to_the_core_unhurt_and_back_up_without_a_jetpack`).

**The facility**: the hand pick on a pylon (`pick_facility`) lights it if it is next in
`order`, else puts every lit one out (`FacilityPylon`); the third opens the gate
(`FacilityGateOpened`). A conduit struck by the pick becomes facility stone and hits every
Warden with an EMP (shield gone, stunned: `WardenStaggered`). `advance_moon` (each tick, after
the enemy director): someone in the arena while the seal stands and no Warden lives wakes one
(`WardenAwakened`; shieldless if both conduits are gone), or on Peaceful opens the seal at
once; a pilot within `CORE_REACH_TILES` of the core sets `core_reached` (`MoonCoreReached`,
the story's hook). The Warden's death (`kill_enemy`) calls `warden_fell`: the seal opens, its
killer and everyone in the facility get `warden_defeated` and the Void Lance
(`WardenDefeated`). All facility state is terrain, so it saves and replicates with no new
section.

**Enemies**: `EnemyKind::{RegolithCrawler, CrystalMite, DerelictAutomaton, Warden}`
(`is_lunar`), `enemy::lunar_roster` (mites in forests, automatons near habitats) at
`LUNAR_TIER`; `spawn_lunar_enemies` runs on the director's spawn check around rigs and pilots on
the Moon (surface or the tube they are in). `despawn_far_enemies` counts pilots too (a pilot
deep in the facility keeps its Warden).

**Weapons**: `WeaponKind::{LunarCutter, LowGMortar, VoidLance}` (`is_lunar`, `is_beam`; item ids
7-9; `shack_level` `u8::MAX`, so the shack never stocks or tunes them).

**The uplink**: `ModuleKind::LunarUplink` (3x2, 2 power, `ModuleService::Beam`), placed once the
builder `landed` (`BuildRefusal::NeedsMoonLanding`); `Beam` refuses on foot (`BoardFirst`) and
without a target (`NoUplinkTarget`): from the Moon to the first usable uplink below (the
player's own first), else the town; from below to the first usable lunar uplink.
`UplinkBeamed { to_moon }` follows the applied command.

**Events** 220-229 (netcode): `MoonVacuumBreach`, `MoonLanded`, `ColonyServiceUsed`,
`FacilityPylon`, `FacilityGateOpened`, `WardenAwakened`, `WardenStaggered`, `WardenDefeated`,
`MoonCoreReached`, `UplinkBeamed`.

Tests: `worldgen/moon/tests.rs` (V8 keeps its placeholder, layouts deterministic and apart, the
colony on flat ground with air inside, the facility from veil to core, tubes and riches, chunks
= layout across borders, V9 below the Moon = V8, the golden digest) and `world/moon_tests.rs`
(vacuum damage and the breach warning, the landing once, low gravity for rigs and pilots, air
on foot, the colony's rules and deals, reactor and drill, the pylon puzzle, the Warden and the
seal, Peaceful, the core, Moon creatures and the colony's safety, uplinks both ways, falling
home, saves and keyframes, determinism, precision at the Moon).

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
0.45 s warning; the boulder deflector counters it) when it is one of the loose ones (one in
three, `stalactite_is_loose`) and the ground there is not settled (see falling boulders above). Drilling a `TreasureCache` / `RelicCache` pays
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
  60 + 30 per tier). `Player::collect_blocks` (private loadout, `PlayerCommand::SetCollectBlocks`)
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
the loot; base values, charge costs subtracted, per minute). Before and after the T19 pacing
pass (mining charges crack hard gems, see below):

| Depth (nominal row) | Drill tier | Drill net | Bombs net | Mining charges net (before) | Mining charges net (after) |
|---|---|---|---|---|---|
| 14 | 3 | 158 | -134 | -1,220 | -1,210 |
| 60 | 5 | 1,125 | 1,134 | 2,755 | 2,118 |
| 60 | 8 | 2,450 | 1,134 | 2,755 | 2,118 |
| 120 | 10 | 6,276 | 4,457 | 11,305 | 9,100 |
| 120 | 14 | 6,336 | 4,457 | 11,305 | 9,100 |

Plain bombs are tunnelling tools near the surface, a fast earner for an under-drilled rig in
the middle strata and behind a strong drill deep down. Mining charges (300 cr a pair at the
shack, which stocks them from level 2) clear rock three times faster than a drill but their
soft blast now cracks one hard gem in three (stone-hard ores and up, like any blast) while
still lifting soft ore out whole (one in six cracks): mid-depth they earn about 0.86x a tier 8
drill (they used to edge it out at 1.12x), deep down 1.4x (was 1.7x), before the real
overhead of placing, retreating, waiting and trips to the surface (the economy bot's charges
strategy measures that loop). A base workshop presses them for 12 blocks and 90 cr (was 8
and 30, a fifth of the shack price, which made crafted charges the money printer).
`bombing_stays_balanced_against_drilling` pins both, shack and workshop price. (One mid-depth
bomb run frees a Star Core, which is announced just as drilling it would be.) Block sales add at most a few percent to drilling income.

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
  per village; `Village::story_seeds` feeds them to `people::story_seeds` (WP-STORY's
  `people::story::run::offer_for` picks each person's arc at play time) with the village's
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
| Burrower | creature | dens in soft rock, crawls open tunnels; hunting, bites one tile out of its way every few seconds at most (never ore) | lunges (rumble, glinting eyes) | break line of sight, bombs |
| Acid spitter | creature | walks | lobbed acid (puffs up) | close in or break line of sight |
| Rock mimic | creature, armoured | dormant boulder, then walks | lunge (wakes when you come close) | explosives |
| Shellback | creature, armoured | slow walker | charge (wind-up) | explosives, armour piercing |
| Magma wisp | creature | flies (mantle) | fireballs (glow) | kinetic and explosives; fire-proof |
| Brood nest | creature | stationary (lairs) | hatches bats while a rig is near | destroy it |
| Brood lord | creature, elite | walks (lairs) | acid volleys and charges | explosives, keep moving |
| Mining drone | machine | flies | ram (beep) | EMP |
| Sentry turret | machine | stationary | energy bolts (charge glow) | EMP, explosives |
| Rogue excavator | machine | walks; hunting, bites one tile of soft rock out of its way every few seconds at most | charge (engine rev) | EMP, explosives |
| Ancient guardian | machine, elite | hovers (lairs) | bolt volleys; shielded | EMP drops the shield, then explosives |

- **Data** (`enemy.rs`): `EnemyKind::stats` (family, locomotion, attack style, radius, health,
  shield, speeds, sight / hearing / leash, contact damage, wind-up and cooldown, range,
  knockback), `resistance(DamageKind)` (armour folds to explosives, machines feel EMPs,
  creatures burn), `band_roster(band)` (what roams each band, weighted), `lair_occupant`,
  `loot(kind, tier, roll)` and `ShotKind` (acid lobs, fireballs, bolts). Health and damage
  scale with the depth tier the enemy spawned at (`tier_scale`, the band index), the crew size
  (+30 % health per extra rig) and the difficulty.
- **Perception** (`perceive`): nothing is noticed through rock. Prey is seen within
  `sight_tiles` with a clear line (`physics::sight_clear`, which also refuses to look between
  two blocks touching at a corner), and kept for `LOSE_TARGET_MILLIS` (4 s) out of sight; a hit
  turns an enemy on its attacker. Drilling gives nothing away (burrowers used to hear drills
  through rock and sense rigs within three tiles of any rock). Only footstep-feeling kinds
  (the dune stalker) use `hearing_tiles`, along an open path at most 1.5 times that long.
- **Digging** (`bite_due`, `bite_box`, `dig_ahead`): burrowers and excavators path through
  open tiles like everything else. While hunting, one may bite a single diggable tile out of
  its way on its slot of the `BITE_MILLIS` (4 s) cycle; they used to clear every tile they
  touched, every tick (burrowers) or every 350 ms (excavators). A burrower still digs its den
  when it appears.
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
  burrowers soft rock, where they dig their den as they appear, or an open pocket against a
  soft wall) with the body in open space; bats come in packs. Every two seconds lair chunks (one in seven, in
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
- **Bodies (playtest fix: enemies obey terrain and never overlap the rig)**: every enemy
  collides with exactly the tiles a rig does (`Terrain::is_solid_at`: rock, built pieces,
  closed vault doors, everything but air) with swept `physics::step_body` motion; nothing
  ghosts through rock. Burrowers dig real tunnels instead: clinging to rock (solid within
  `CLING_PIXELS`) they crawl without gravity and bite the soft rock just ahead out of the
  terrain (`diggable`: dirt, clay, stone, hard rock; never ore, fixtures, built pieces or
  bedrock, never in a safe zone; the tiles go through the terrain journal like the rogue
  excavator's), fall in open air, and lurk instead of wandering while idle. Spawns land only in
  open space (`site_fits` checks the body; `spawn_enemy_with` digs a burrower's den out of the
  soft rock it appears in, then moves any other overlapping spot to the nearest open one with
  `physics::free_spot_near`), and an enemy left inside rock by a terrain
  change (a boulder, a built piece, an old save) is resolved out the shortest way at the start
  of its tick (`free_from_rock`, up to a tile; a burrower with no room digs a pocket);
  knockback and blasts only change velocity, so they cannot embed one. Stationary fixtures
  never move. Enemies touch rigs and pilots on foot but never overlap them
  (`keep_off_bodies`, after movement): an overlapping body is pushed out along the shallower
  axis to exactly touching the rig's collision square or the pilot's box (never into rock) and
  its velocity into the body is cancelled. Contact attacks reach `CONTACT_MARGIN` (2 px)
  beyond touching (box test), a landed hit springs the attacker back (`RECOIL_SPEED`), and
  melee enemies waiting for their next attack hang back `STANDOFF_PIXELS` out of contact;
  ranged ones keep their distance as before. Other code moving enemies must keep these
  invariants (go through `step_body`, call `free_from_rock` after teleporting one).
- **Point-blank shots** (`world/weapons.rs`): an enemy touching the rig can sit between the
  hardpoint and the muzzle, where a shot spawned at the muzzle never meets it. Each projectile
  pellet first sweeps the hardpoint-to-muzzle stretch of the aim line and hits what it finds
  there (a spent pellet never spawns); hitscan rays test the same stretch before marching out
  from the muzzle. The muzzle offset itself is unchanged.
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
  `EnemyDifficultyChanged`); Peaceful clears every hostile and spawns none. Online only the host may
  change it (netcode drops a client's command, `NetSimulation::host_only_action`).
- **Balance pass (T19, measured by the balance harness):** burrowers hear drills 10 tiles off
  (was 16), wind up for 850 ms (650) and bite for 7 (10) every 3 s (1.9), and weigh 2 (3) in
  the clay beds' roster: an unarmed rig drilling the clay beds lost about 190 hull a minute to
  them, now about 30. Brood lords have 420 health (600), ancient guardians 480 plus a 180
  shield (700 + 300). See the root README's balance section for the numbers.
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

## Night-time gameplay (`src/night.rs`, `src/world/night.rs`)

Expansion plan WP-NIGHT (wave 4, T25). The open surface is dangerous at night; towns,
villages, camps, homes and lit bases are safe; Peaceful turns it off (plan §11 decision 4).

- **Night monsters** (`EnemyKind::ShadowHound`, `DuneStalker`, `WillOWisp`, `MothSwarm`,
  `UmbralAlpha`; `night::is_nocturnal`): ordinary enemies (stats, resistances, loot, the AI,
  the damage model, replication) that only the night pass spawns. Hounds hunt in packs and
  pounce; stalkers swim through soft surface crusts (`night::stalker_ground`) and hear
  footsteps (`night::feels_footsteps`: prey within hearing that an open path of tiles reaches,
  around corners but never through rock; `sound_reaches`); wisps throw
  `ShotKind::Wispfire`, shrug off kinetic hits and give light; moth swarms are one entity, a
  contact swarmer that burns easily; the alpha is an elite. They drop night-only materials
  (`StrategicResourceKind::MothSilk`, `ShadowPelt`, `StalkerFang`, `WispEssence`,
  `UmbralHeart`). Tier (health and damage) grows with distance from town (`night::night_tier`).
- **The night pass** (`WorldState::advance_night`, after the enemy director each tick): the
  night under way is `night::night_of(clock)` (named by its dusk's day; monsters rise from the
  second half of dusk, `night::monsters_rise`). Turning into a night records it in the
  director's `NightLedger` and fires `NightFell { night, moon }`; the dawn after fires
  `NightSurvived { night, kills }` and every night monster flees for
  `RETREAT_MILLIS` (+ a stagger) and leaves with `NightMonsterBurrowed`. Each second, round
  every **exposed actor** (a live, awake rig or pilot outside every night-safe place) a monster
  of the biome's roster (`night::night_roster`, hounds in packs) may rise 24-34 columns away,
  out of view, under a local cap (`NIGHT_LOCAL_CAP` + `night::moon_extra_cap` + Hard's extra)
  and the world caps; the odds scale with difficulty, the world's hazards and
  `night::moon_density_percent` (full moon 160 %). On a new moon (`is_rare_spawn_night`) the
  umbral alpha and two hounds rise once a night (`RareNightSpawn`, `NightLedger::rare_night`).
  Idle monsters scent the nearest exposed actor within `SCENT_TILES` and prowl to it by moving
  their home; light-shunning kinds stop at the last spot before a safe place, so they lurk at
  the edge of a lamp's pool or a fence. Shallow nocturnal ore (`night::glows_at_night`:
  silver, emerald, diamond, platinum within `NOCTURNAL_GLOW_DEPTH` rows) near an exposed actor
  reveals itself on that player's map. Everything is hashed from the seed, tick and positions.
- **Safety** (`WorldState::night_safe_zone`): deeper than `NIGHT_SURFACE_DEPTH` rows under the
  local ground or more than `NIGHT_SKY_ROWS` above it, the town, villages and halls,
  infrastructure, wanderers' camps (`CAMP_SAFE_TILES`, the open item), homes (a claim holding
  a bed or a homestead), a claim spot fenced by `BuiltTile::Palisade` posts on both sides
  within `PALISADE_REACH` (`palisade_guarded`) and fixture light of at least
  `NIGHT_SAFE_LIGHT` (`WorldState::fixture_light_at`: lamp fixtures, settlement streets,
  campfires). `WorldState::hostile_safe_zone(attacker, tile)` is the rule every enemy check
  uses (perception, giving up the chase, turning back, `strike_rig`, `strike_pilot_as`):
  night monsters use the night rule, all others `enemy_safe_zone` (which still keeps the whole
  surface strip safe from cave enemies).
- **Light matters** (`night::light_response`): hounds, stalkers and the alpha shun light (the
  safety rule above) and flee a lit flare within `FLARE_SCARE_TILES`; moth swarms circle the
  nearest lamp or flare within `MOTH_LURE_TILES` instead of hunting (`WorldState::lure_for`,
  shared with cave bats and flares); wisps don't care. `WorldState::light_at` now counts
  pilots' helmet lamps (open item), turns off the lamps of rigs whose crew is in bed, and adds
  wisps and, at night, nocturnal ore and campfires.
- **Night economy** (`village::services::MarketTime`, `WorldState::market_time`,
  `Village::offers_at`): from late dusk to dawn a village's trading post is the night market
  (`services::night_market`: `VillageDeal::SellNightGoods` at 100 % + prosperity of
  `night::night_material_value`, flares, scare charges, a ward, a rumour and a curio that
  changes nightly) and the workshop, armory, tailor, assay office and map room are shut
  (`VillageService::closes_at_night`, no offers); the inn, depot, shrine and hall trade as
  ever. Deals are validated against the offers at the tick's time.
- **People** keep the sky's night: `people::clock`'s night is the clock's 19:30-04:30 (open
  item), derived from `clock::NIGHT_START_SLICE` / `DAWN_START_SLICE`.
- **For story arcs (T26)**: `WorldState::night_ledger()` (`active_night`, `night_kills`,
  `kills_by_kind` / `kills_of`, `nights_survived`, `rare_night`), `night::NightHook`
  (`Hunt`, `Gather`, `SurviveTheNight`, `SlayTheAlpha`) with `night::night_hooks(biome, moon)`
  and `signature_drop`, `night_is_dangerous()`, `night_monsters()`, and the events above.
- **For homes (T24)**: a home is night-safe once its claim holds a bed or homestead; a
  contracted home that should count earlier can be added to `in_home_claim`.
- **Balance** (probe `night`, `BALANCE_ONLY=night`): a parked, unarmed stock rig on open
  ground loses about 80 hull a minute on an ordinary night (first hit after ~30 s), about 140
  under a full moon and about 220 on a new moon once the alpha finds it; under a lamp, nothing.
- **Formats**: the ledger rides inside the `EnemyDirector` (serde default; the `Enemies`
  section; hashed with it). `PROTOCOL_VERSION` 12 (new enemy, shot, material, tile and deal
  variants and events 250-253) and save format 10 (version 9 files load as is).

`world/night_tests.rs` covers rising after dusk and burrowing at dawn, nothing by day,
underground, in town or on Peaceful, safety by lamps, palisades, homes, camps, villages and the
town, a hound striking in the open but not under a lamp, moths circling a lamp, the pilot's
lamp, flares scaring hounds, the Moon (brighter full moons, the alpha once on a new moon),
night drops and the ledger, the night market and shut counters, nocturnal ore, people's night,
determinism, saves, replication and tick-rate safety.

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

## Pilot content (`src/pilot/arsenal.rs`, `src/mechanism.rs`, `src/world/{pilot_arms,pilot_gear,mechanisms}.rs`, `src/worldgen/streamed/pilot_spaces.rs`)

Expansion plan WP-PILOT (wave 4, T23), on top of F5.

- **Personal arms** ([`pilot::arsenal`]): `WeaponKind::{Sidearm, Rifle, Shotgun, Wrench,
  HandPick}` (`WeaponKind::is_personal`, never in `WeaponKind::ALL`, never mounted). A
  player's `pilot_arsenal` (private loadout: owned arms, personal ammo
  `AmmoKind::{Cartridge, Shell}` up to `MAX_PILOT_AMMO`, the held arm) is mirrored onto
  `PilotState::weapon` (public). On foot the trigger (`WorldState::use_pilot_hands`) fires the
  held gun through the rig's projectile entities (kills, drops and `damage_entity` work as for
  rig weapons; friendly fire stays off), swings the wrench, or works the hand pick (a creature
  in reach takes the swing as `HandPick`). Guns repeat while held at their cooldown; aim is the
  free aim or `pilot_auto_aim_target`. `PlayerCommand::Pilot(PilotAction::{Equip,
  CycleWeapon})` swaps (also from the rig). Bought with `SessionServiceRequest::PilotArms` at
  the Explosives Shack and `VillageDeal::PilotArms` at armories. Balance: every arm is below
  the starter rivet gun's damage per second.
- **Gear** (`PilotGear`, every field `serde(default)`): armour (`armor_factor` on strikes and
  blasts), thermal lining (`cold_cover`; cold damage at altitude, the F6 open item), heat weave
  (`heat_cover` vs `band_heat_level`), headlamp (`headlamp_radius_tiles`, the game's light map),
  climbing boots (wall slide, wall jump, `fall_damage_in`), glider, a hovering jetpack from
  `HOVER_JETPACK_TIER`, grapple reach and the rig link (recall; sentry at `RIG_SENTRY_TIER`).
  `PilotGearKind` lists every piece with tiers, prices and blurbs (`upgraded`); gear that only
  matters under a sky (`needs_sky`: the thermal lining) is refused on skyless worlds.
- **Rope** (`PilotMode::Tether`, `PilotState::tether`): `PilotAction::Rope { aim }` lets go,
  clips onto the own rig's winch within `BOARD_REACH` (`WINCH_LENGTH`, anchor =
  `winch_anchor`, followed every tick through `PilotControls::anchor`), or casts the grapple
  (`grapple_hit`). The body step keeps the pilot within the rope's length (`apply_rope`); up
  reels in, down pays out. A grapple whose rock is mined away, or a winch whose rig is wrecked,
  lets go (`refresh_rope`).
- **Recall** (`PilotAction::RecallRig`): a budgeted breadth-first tile path from rig to pilot,
  recomputed every tick (`rig_recall_waypoint`); the rig flies it kinematically instead of its
  movement step (`step_rig_recall`, `RECALL_SPEED`, a little fuel) and stops beside the pilot.
  **Sentry** (`rig_sentry_control`): the parked rig's controls get a weapon control aimed at the
  nearest creature within `SENTRY_COVER` of its pilot.
- **Mechanisms** ([`mechanism`]): ruin `BuiltTile`s (codes 8-69 of the built range, all
  unbreakable, never built or deconstructed: `BuiltTile::is_ruin`): `RuinWall`, `Gate` under a
  `GateLintel(channel)`, `Lever { channel, on }`, `PressurePlate { channel, down }`,
  `KeycardDoor(color)`, `Chest { open, card }`. `PilotAction::Use` works the nearest
  (`pilot_usable`, `USE_REACH`; beside the rig, `pilot_fixture_first` says whether the use
  button works the fixture or boards: the nearer wins). Levers flip every gate of their channel
  within `REACH_COLUMNS` x `REACH_ROWS`; plates (`advance_plates`, once a tick) are pressed by pilots'
  feet and resting rigs and release `PLATE_HOLD_SECONDS` after the weight leaves (timers in
  `Bases::plates`, so they save and replicate), never closing a gate on a body. Keycards are
  quest items `0xC100 + colour` (`keycard_item`) on the key ring: always picked up, even into a
  full pack, and a card that rode into the rig on boarding still opens its door. Chests spill
  `chest_loot(seed, tile)` (richer deeper) and their card. `WorldState::stamp_tiles` writes
  hand-made ruins for tests and scenes.
- **Generation** (`GeneratorVersion::V9`, `has_pilot_spaces`): `pilot_spaces::spaces_touching`
  places one space per `REGION` cell (`SPACE_PERCENT`), underground, off the town and clear of
  villages: `PilotSpaceKind::{Ruin, RigVault, CrawlwayCave}` with fixed layouts (mirrored at
  random), stamped last in `assemble`. Access tags: `PilotSpaceKind::access`
  (`RequiresPilot`; the vault `Both`); `nearest_space` binds quests. Golden digest
  `GOLDEN_V9_DIGEST`; V1-V8 output is unchanged.
- **Events** 230-239: `PilotFired`, `PilotWeaponDry`, `PilotWeaponSwapped`, `PilotRope`,
  `PilotGlide`, `MechanismUsed`, `ChestOpened`, `RigRecall`, `PilotExposed`, `PilotWallJumped`.
- Not built: the co-op ride-along seat (optional in the plan; a second player can stand on a
  rig's roof only as its own pilot).

`pilot/arsenal.rs`, `mechanism.rs` and `pilot_spaces.rs` test the data and the generator
(layouts, placement, chunk seams, the V9 digest); `world/pilot_content_tests.rs` covers the
arms (shots, ammo, dry clicks, melee, swaps, shops), gear (hover, glide, boots, grapple,
winch, armour, cold), levers, plates (incl. a rig on a plate), chests and keycards, recall
and the sentry, and saves (JSON, postcard), replication (private arsenal, public pilot) and
identical continuation.

## Wave 4 seams: pilot content (T23)

- **Generator V9** is mine; other wave 4 tracks that also need a new generator should fold
  into the same V9 at integration (my content is gated on `has_pilot_spaces`, the digest test
  lives in `pilot_spaces.rs`).
- **Sky / Moon (T20-T22)**: cold damage reads `environment_at(..).cold` against
  `PilotGear::cold_cover` (four thermal tiers cover the Moon's `MAX_COLD`); sky towns can sell
  personal arms through `VillageDeal::PilotArms`; Moon facilities can stamp ruin pieces
  (`stamp_tiles` / `BuiltTile` mechanisms) and `PilotSpace` layouts.
- **Night (T25)**: the headlamp tier is `PilotGear::headlamp_radius_tiles` (the game's light
  map uses it); `WorldState::light_at` still has no pilot lamp.
- **Story (T26)**: `pilot_spaces::nearest_space` and `PilotSpaceKind::access` bind pilot-only
  arcs; keycard quest items sit at `0xC100+`.
- **Formats**: `PROTOCOL_VERSION` 12 and save format 10 (versions 8 and 9 load unchanged).

## People in the world (`src/people/{clock,routine,camps,offers,contractor,ledger}.rs`, `src/world/people.rs`)

Expansion plan WP-PEOPLE (wave 3, T17), on worlds with villages (`GeneratorVersion::has_villages`,
V6 to V8). Village residents and the town's folk are `EntityKind::Npc` entities.

- **Clock** (`people::clock`): `ClockSource` (`day_time() -> DayTime`, `is_night()`).
  `WorldState::people_clock()` is the only place people ask the time, and it reads T14's world
  clock (`clock::WorldClock` implements `ClockSource`: day from 0, minute of its 24-hour face),
  so routines follow the world's day length and a skipped night wakes everyone at dawn. People's
  own schedule calls the clock's 19:30 to 04:30 night (WP-NIGHT aligned it). `TickClock` (tick, tick rate,
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
  not enemy safe zones, but they are night-safe (WP-NIGHT `night_safe_zone`).
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

## Story arcs (`src/people/story.rs`, `src/people/story/`, `src/world/arcs.rs`)

Expansion plan WP-STORY (wave 4, T26), on the people framework and the quest engine. Arcs are
authored data; everything at play time is a pure function of the settings, the regional feature
index, the giver (rebuilt from the seed) and the player's `ArcLog`, so every peer agrees.

- **Arc grammar** (`people::story`): an `ArcTemplate` has an id (stable, saved), title,
  `ArcKind` (Frontier, Village, Sky, Mystery, Home, Night, Pilot, Crew), hook / twist /
  consequence, tier, quest `Prerequisite`s, `Trigger`s (want, fear, value, quirk, relationship,
  absent person, village problem, player history, role), role nudges, `Slot`s (an absent
  person, a neighbour, a world fact for a target), quest `TargetSpec`s and stages of
  `Objective`s, one beat title and one `ArcStep` per stage (`journal` story text, the giver's
  `nudge`, and the `contact`'s line for a `Speak` beat), a `climax`, twists, a `QuestReward`,
  `ArcOutcome`s (the default ending, chosen endings, an abandoned ending; each with `effects`,
  the giver's `line` and the journal `epilogue`), a `pitch` in every voice (`voices(terse, warm,
  soft)`: gruff/dry, cheerful/earnest, nervous/poetic; the dialogue tone filter colours each),
  optional per-tone thanks, and `contact_other` (a `Speak` beat at home goes to the neighbour
  slot's person). Text variables (`ARC_VARS`): giver, village, player, focus/focus_relation,
  other/other_word, place/band/depth (the first place slot's target), dest/where (target 0),
  dest2/where2 (target 1), contact.
- **Content** (`story/arcs.rs`, `story/arcs/*.rs`): 67 arcs in batches (first 8; frontier 101-110;
  village 201-210; sky 301-308; mystery 401-408, a chain from "Blue Lights Below" to "The Heart
  of the Moon" that ties the deep's lights and hum to the Moon; home 501-506; night 601-607;
  pilot 701-707; crew 801-803). `ARC_TEMPLATES` chains `arcs::BATCHES`; each arc runs on the
  quest engine as `QuestId(ARC_QUEST_BASE + id)` (`ARC_QUEST_BASE` 20,000;
  `QuestId::def`/`QuestId::arc`, `ArcId::quest`).
- **Story places** (`quest::StoryPlace`, `TargetSpec::Story`, bound in `quest::binding`): the
  nearest other surface village (else the town), an underground hall (else a cavern), a camp
  (else a village, else the town), the town, the giver's home, a sky island (else the highest
  ridge within 240 columns), a sky continent (else an island, else the ridge) and the Moon
  (else the deepest chamber), all through regional queries from the giver's settlement
  (`bind_quest_from` returns what each target bound to as a `BoundTarget`). Every arc binds on
  V5-V8 worlds, with and without sky (lint), and from villages far out east and west; worlds
  without a sky don't offer sky-continent arcs. On a world with a sky an arc is only offered
  once the rig can climb to its sky places by the measured climb table
  (`milestone::island_capable` for isles, the Act II gate `milestone::crosses_dead_air` for
  continents and the Moon); the authored tank prerequisites still apply everywhere.
- **New objective primitives** (`quest::Objective`): `OnFoot` (the pilot reaches the target),
  `HandDeliver` (the pilot at a building door, a town counter or a camp's fire inside the
  target), `Vigil` (seconds at the target counted only at night), `Speak` (talk to the arc's
  contact; bound without one, reaching the target counts) and `Settle` (a base module of a kind
  standing in a claim the owner holds; WP-HOME's homes are claims too). Pilot primitives read
  `actor_tile`; markers and objective lines cover all of them (`describe::objective_line_named`
  names places the story's way).
- **Runtime** (`people::story::run`): `offer_for` (a promised follow-up first, else the best
  trigger score with role fit and a jitter rotated every `OFFER_ROTATION_DAYS`; arcs finished
  with this giver, running, locked by prerequisites are skipped, ones finished with someone
  else score lower), `bind_arc` (binds from the giver's settlement, names every place as an
  `ArcPlace`, picks the `ArcContact`, starts relays at the giver), `ArcCast` (text kept as text:
  `vars`, `render`, `voiced`), `ArcRun` (cast + the `ActiveQuest`; `journal_text`,
  `story_so_far`, `objective_lines`), `ArcRecord` (history; `epilogue`), `ending_reply`, and
  the `ArcLog` (open `offer`, `active` (at most `MAX_ACTIVE_ARCS` = 3), `history` (the newest
  `HISTORY_LIMIT` = 12: the log rides in the loadout section, resent whole on change), promised
  `follow_ups`, `finished` (kept for good)).
- **Rules** (`world/arcs.rs`): `PeopleCommand::AskWork { npc }` pitches (binding the offer) or
  nudges / asks how it went; `AcceptArc { npc }`; `TurnInArc { npc, outcome }` (only an outcome
  of `ArcTemplate::turn_in_choices`; deliveries handed over; the quest reward paid with its
  reputation as goodwill; missing deliveries get a reply naming them); the ending's
  `ArcEffect`s: `Reputation`, `Mood`, `Prosperity` (the ledger's `prosperity` per settlement:
  `WorldState::village_now` / `PeopleLedger::village_now` shift the village's 1-5 prosperity,
  which prices its counters and fills the meeting hall's stars), `Relationship` (the ledger's
  `bonds`: word of the player spreads between the pair on the new terms,
  `RelationshipKind::reputation_share`), `UnlockOffer`
  (the journal's `unlocked`: that person's rows sell as to a friend), `FollowUp`, `Credits`,
  `Cosmetic`, `Settlement` (goodwill with every resident); the giver remembers
  `MemoryFact::ArcCompleted`, word spreads); `AbandonArc { arc }` (anywhere; the abandoned
  ending, `MemoryFact::ArcAbandoned`). Arcs step inside `advance_quests` (a failed twist loses
  the arc); a `Speak` beat lands when the player talks to the contact (`arc_contact_lines`).
  Queries: `arc_log`, `arc_talk(player, npc)` (`ArcTalk`: nothing / has work / pitched /
  running / ready with endings), `arc_offer`, `arc_markers`.
- **State**: the `ArcLog` is `QuestLog::arcs` (private loadout; saved; serde default); the
  ledger's `prosperity`, `bonds` and `PersonJournal::unlocked` are public (`People` section).
- **Events** (codes 260-265): `ArcOffered`, `ArcAccepted`, `ArcStepDone`, `ArcClimax`,
  `ArcCompleted`, `ArcAbandoned`. Audio: a curious hook, a quill per beat, a music-box climax,
  a warm cadence at the end (patches `story_hook`, `story_quill`, `story_climax`,
  `story_resolved`).
- **Report**: `people::report::arc_report` behind `fatherload --story-report --arcs`.

Tests: `people/story/arc_tests.rs` (shape, text and variable lints, no reused lines, honest
kinds, variety of beat shapes and primitives, no dead ends, rewards in bounds, every arc offered
by someone across 240 villages, follow-ups and prerequisites, every arc binding and rendering on
V5-V8 worlds with stand-ins where there is no sky, deterministic binding, memories) and
`world/arc_tests.rs` (pitch and take on, reach, every arc played through the tick to its climax
by a bot (going places, talking to contacts, keeping night watches, walking on foot; digging and
bombing marked done as the quest engine's own tests cover them), endings with rewards, memory
and history, giving up, saves (old saves load), replication of the loadout and ledger,
determinism, two seats with their own stories).

## Wave 4 seams: stories (T26)

- **Homes (T24):** `Objective::Settle` counts any module in a claim the owner holds; a
  contracted home is a claim, so home arcs complete with WP-HOME's houses too. Contractor arcs
  read `people::contractor` roles through triggers (Smith, Apprentice, Mechanic...).
- **Night (T25):** `Objective::Vigil` counts while `WorldState::is_night()`; night arcs need no
  monsters, and peaceful worlds keep them.
- **Sky / Moon (T20-T22):** sky places bind to `FeatureKind::SkyIsland`, `SkyContinent` and
  `Moon` regional features; new sky towns become ordinary settlements if they are
  `village`/`routine` settlements (then they give stories and are contacts). A sky-town
  `StoryPlace` can be added next to `Village`.
- **Pilot (T23):** `OnFoot` and `HandDeliver` read `is_on_foot` / `actor_tile`; crawlways and
  lever ruins can become `Feature` targets once a generator places them.
- **Formats:** `PROTOCOL_VERSION` 12 (the people commands, objectives, story places, arc log in
  the loadout, ledger story marks, events 260-265) and save format 10 (serde defaults: version
  8 and 9 files load).

## Tombs and crypts (`src/tomb.rs`, `src/tomb/`, `src/world/tombs.rs`, `src/worldgen/streamed/tomb_sites.rs`, generator V10)

Expansion plan WP-TOMBS, wave 5 T27: the shared core that the side-view tombs (T28) and the
top-down crypts (T29) build on. **Each place is one or the other** (user decision): a site's
[`SiteStyle`](src/tomb.rs) (`SideView` or `TopDown`) is fixed by the seed, drawn on its
entrance art and map marker, and is the same for every floor of that place.

- **Generator V10** (`GeneratorVersion::V10`, now current, `has_tombs`): V9 plus tomb sites and
  the run vault; V9's sky bands, climb table and thin air are kept, so everything else is
  exactly V9's (V1-V9 output and digests are unchanged; V10 is pinned by
  `worldgen::streamed::tomb_sites::tests::v10_generation_is_pinned`).
- **Sites** (`tomb_sites::sites_touching`, pure per placement cell): `SiteSetting::Deep` (a
  third of the `DEEP_REGION` cells below row 140, carved into its own pocket; tier 1-5 by
  depth), `Mesa` (one per `MESA_CELL` columns where the biome is badlands or desert and the
  ground is level, tier 1), `OldQuarter` (about half the surface villages, 22 rows under the
  village, tier 2), `SkyContinent` (most continents, clear of the city, temple, lake and
  nests, tier 4) and `Moon` (two, one of each style, clear of the colony, habitats, facility
  and lava tubes, tier 5). About half of each tier is each style (bit 40 of the site roll). A
  `TombSite` carries its id (`SiteId`: setting and cell), style, tier, `SiteOpening`, entrance
  origin, door tile, mirroring and seed. Entrances (`entrance_cells`): a stepped sandstone
  pyramid with a sealed door in an alcove (side view) or a crypt-stone mausoleum on carved
  pillars over a sealed floor hatch (top down); surface sites stand on the highest ground of
  their footprint on a `PLINTH_ROWS` plinth, underground ones carve a pocket a rig can dig
  to. Stamped last in `assemble`. `site_with_door`, `nearest_site` look them up.
- **Openings** (`SiteOpening`, rolled per site): `Key` (half: a `TOMB_KEY` quest item, used
  up; keys ride the key ring, one ruin chest in five holds one on V10 worlds, one tomb cache
  in six), `Explosives` (three in ten: a blast within `SEAL_BLAST_REACH` of the door,
  `WorldState::blast_tomb_seals` from the bomb pass) and `Story` (two in ten: a player who has
  seen a story arc through; `WorldState::unseal_tomb_site(site, by, output)` is the arc
  binding's entry point).
- **The run vault** (`tomb_sites::VAULT`): `SLOTS` (16) run slots of `SLOT_SIZE` (168 x 48)
  tiles of sealed bedrock under columns -15,000.. (f32 keeps 1/32 px there), rows 180..276.
  Nothing digs, blasts or picks bedrock, so a slot is reached only through a site. No V10
  feature, point of interest or pilot space touches the vault (`vault_hides`: the feature
  index, `pois_touching`, `feature` and `spaces_touching` leave them out), so nothing points
  quests, markers or scanners into the sealed rock.
  `in_vault` / `WorldState::in_run_space` mark run spaces: no cave water or critters
  (`Terrain::cave_at` answers `None`), no band heat on the pilot, never an enemy safe zone,
  torchlight in `light_at` (`tomb::RUN_SPACE_LIGHT`).
- **Floors** (`tomb::floor`): `plan_floor(site, run, floor)` is a pure room graph
  (`FloorPlan`, `RoomNode`, `RoomKind`): entry, a shuffle of trap hall, puzzle and ambush (more
  rolled with difficulty), a shrine in the middle of the main path, the vault branching off a
  puzzle or ambush room, the boss room before the stairs on the last floor. `floor_seed(site
  seed, run counter, floor)`; `floor_difficulty(tier, floor)` ramps per floor; `floors_for_tier`
  gives 3-6 floors. The site's run counter (`Tombs::site_runs`) makes every run fresh.
- **Realizers** (`tomb::realize`, **the T28 / T29 seam**): `FloorRealizer` (`style`,
  `realize(&FloorPlan) -> RealizedFloor`), chosen by `realizer_for(style)`. A
  `RealizedFloor` is cells relative to the slot corner (the rest stays bedrock), the rooms'
  interiors, the party's start tile and `SpawnSlot`s (`SpawnRole::{Guard, Ambusher, Boss}`);
  it must stay inside `MAX_FLOOR_SIZE` and be pure. `SideViewRealizer` builds platformer rooms
  on one floor line (three-tall doorways, spike pits two deep, lever or plate gates, the vault
  up a step and ledge stair through a ceiling hole, the shrine, caches, the boss room, the
  stairs), all reachable by the stock pilot (T28 replaced it: see "Side-view tombs" below).
  Top-down sites use T29's `crypt::CryptRealizer` (see "Top-down crypts" below; T27's
  `TopDownRealizer` placeholder was removed in wave 6). `bestiary(style, role, variant)`
  names the enemy kind (side view: T28's `side_view::bestiary`; top down: T29's
  `crypt::bestiary`); the boss's health is
  `boss_health_scale(site tier)` (`BOSS_HEALTH_SCALE`: 0.5x its stock at tier 1 up to 0.7x at
  tier 5 since wave 6, on top of the creature tier).
  `TrapKind` (`Spikes`, `TRAP_KINDS` codes reserved) with `damage_per_second`.
- **Tiles** (`BuiltTile` codes 72-119): `TombWall(style)`, `TombPillar(style)` (pilots pass),
  `TombDoor { style, sealed }`, `TombShrine { lit }`, `TombStairs`, `TombExit`,
  `TombCache { open }`, `TombTrap { kind, hidden }` (pilots pass into it and are hurt). All
  unbreakable (`is_ruin`), `access` pilot-only. Doors, shrines, stairs, the way out and caches
  are `mechanism::Usable` tomb variants, so the use button (keyboard, gamepad, the touch USE
  button) works them and the prompt shows their verb.
- **Runs** (`world/tombs.rs`): the use button at an open door starts a run for the opener and
  every crewmate on foot within `PARTY_REACH`, in the lowest free slot; the party arrives at
  the floor's start, the pilot's `PilotView` follows the style and the rig waits outside (it
  takes no damage while its player is in a run). `TombRun` holds the site, run counter, slot,
  floor, `RunPhase` (`Exploring`, `Picking`), members (`RunMember`: relics, offers, unbanked
  and banked loot, downed state), rooms, spawns, sprung ambushes, the guardian. Guards spawn
  with the floor, ambushes and the guardian when a standing member walks into their room
  (`TombAmbush`); rooms reveal on the map for the whole party as they walk in; traps hurt
  pilots inside them. The stairs end the floor (the last floor's once the guardian is down:
  the run is won, `Finished`), relic picks follow (`TombCommand::PickRelic`, timed out after
  `PICK_TIMEOUT_MILLIS`; nothing hurts the party while it picks, and creatures in the vault
  never fall to the enemy director's far-despawn, so a guardian is felled only by killing it),
  then the next floor is written over the last one. The first floor's
  way out ends the run with everything banked (`Retreated`).
- **Roguelite stakes**: caches pay `cache_value` (tier squared) times the loot relics into the
  member's unbanked loot (sometimes a tomb key or a lore page); a shrine banks everyone's
  loot (credits paid at once), revives the downed, heals once and saves the run (the game
  writes the save on `TombShrineRested`). A member at zero health is downed
  (`down_tomb_member`, from `kill_pilot`); a standing crewmate beside them for
  `REVIVE_MILLIS` (`REVIVE_REACH`) or a shrine revives them (`REVIVE_SHARE` health); when every
  member is down the run ends (`Wiped`) and unbanked loot is lost. Finishing pays the
  `guardian_hoard`. Leaving the session or being parked mid-run abandons it.
- **Relics** (`tomb::relic`, data in `RELICS`): 12 perks (Spring Heels double jump, Ricochet
  Rounds, Trap-Sight Lantern, Vampiric Pick, Glider Boost, Iron Lungs, Quickstep, Featherfall,
  Gravedigger, Bone Charm, Hunter's Mark, Second Wind) and 3 bargains (blessing plus curse:
  Pact of Greed, Glass Cannon, Restless Soul), with `SYNERGIES` (Skywalker, Trick Shot,
  Bloodbound). `offers(seed, floor, member, held, style)` gives each member their own 3 perks
  and a bargain (`Relic::suits`: since T29 every relic suits both styles; `Relic::blurb_in`
  gives the crypt meaning of the movement perks). Effects (`RelicEffects`) hook in through `PilotBoons` on the pilot entity (extra
  jumps, move and jump speed, glide, featherfall, trap sight; `step_pilot` reads them), the
  pilot's max health and regeneration, `damage_pilot` (damage taken), `damage_entity`
  (damage dealt, the vampiric heal), projectiles (`ProjectileState::bounces`, ricochets off
  rock) and caches (loot).
- **Unlocks** (`TombUnlocks`, per player, permanent): the relic codex, tomb maps (sites
  entered: their markers stay on the map), cleared sites, guardians defeated (each grants a
  `TrophyCase` through `grant_furniture` and the Crown decal), lore pages (`LORE_PAGES`, the
  builders of the Moon and the deep core), runs, finishes and the best haul. `RunSummary`
  (per player, with a serial) feeds the death / summary screen.
- **The world field** `Tombs` (`runs`, `site_runs`, `unsealed`, `unlocks`, `summaries`):
  serde default, saved, hashed only once non-empty, replicated as `SectionKey::Tombs` to
  everyone; floors travel as terrain ops, the party as pilot entities.
- **Events** (codes 270-279): `TombEntered`, `TombSeal`, `TombAmbush`, `TombGuardianDefeated`,
  `TombFloorCleared`, `TombRelicPicked`, `TombShrineRested`, `TombCacheOpened`,
  `TombMemberDowned`, `TombRunEnded`.
- **Balance** (`balance::tombs`, `--balance-report --only tombs`): `TombBot` (also used by the
  world tests) plays one run per seed, tier and style on Normal enemies, with kit fitting the
  tier; the report gives run length, cleared / wiped / timeout counts and banked and lost loot
  per tier and style, and `loot_on_offer` the credits of every cache plus the hoard in a whole
  run (and, since wave 6, wipes by tier and style, with the room of a lone member's wipe). First numbers (4 seeds): the bot clears 2 of 4 tier 1 tombs in about 1.6 minutes and
  mostly falls to the placeholder guardian deeper down (it never dodges); loot on offer grows
  from about 900 cr (tier 1) to 3,800, 15,000, 36,000-42,000 and 75,000-90,000 (tier 5).
  Creatures spawn at `enemy_tier(run)` (one under the site's tier, plus one per two floors),
  well under the strata's band tiers: they fight pilots, not rigs.
- **Scenario helpers**: `scenario_enter_tomb`, `scenario_edit_tomb_member`,
  `scenario_tomb_stairs`, `scenario_end_tomb_run`, `scenario_tomb_unlocks`,
  `scenario_down_tomb_member`.

`tomb/*` unit tests cover the grammar, determinism and fresh runs, every plan realizing inside
the slot with every room, relic data, synergies and offers; `tomb_sites` tests cover V10-only
placement in every setting, both styles on the sky and the Moon, chunk seams and fixed styles,
the vault and the V10 digest; `world/tomb_tests.rs` covers keys, blasts and story seals,
entering as a party, a scripted bot clearing a side-view and a top-down run, the top-down walk,
shrines and the way out, wipes, the safe rig, co-op revives and wipes, individual relic picks,
relic effects, the guardian, saves mid-floor (JSON and postcard), replication and hashing,
tick-rate safety; the netcode e2e test plays relic picks over the wire.

## Top-down crypts (`src/tomb/crypt.rs`, `src/tomb/crypt/layout.rs`, `src/pilot/top_down.rs`, `src/world/crypts.rs`, `src/world/enemies/crypt.rs`)

Expansion plan WP-TOMBS, wave 5 T29: the top-down half of the tombs, on T27's core. Every
site whose `SiteStyle` is `TopDown` plays it (the style is still fixed per site by the seed).

- **The pilot from above** (`pilot::top_down`, `step_top_down`, chosen by `PilotState::view`):
  no gravity; the held directions walk eight ways (`TOP_DOWN_SPEED`, a full diagonal no faster
  than straight, `TOP_DOWN_ACCEL` / `TOP_DOWN_BRAKE` so knocks slide off); a square footprint
  (`TOP_DOWN_FOOTPRINT` deep, `PILOT_HALF_WIDTH` wide: `PilotState::height` answers the
  footprint in top-down, so every box test, middle, reach and contact uses it) swept per axis
  against `PilotTerrain::blocks` (diagonals slide along walls) with a **corner nudge**: walking
  straight at a corner the footprint clips by up to `CORNER_NUDGE` px slides round it. The
  jump button is a **dodge roll** (`ROLL_SECONDS` at `ROLL_SPEED` along the held direction or
  the heading, unsteerable, then `ROLL_COOLDOWN_SECONDS`); `PilotState::roll_guarded` (all but
  the last `ROLL_RECOVERY_SECONDS`) makes `damage_pilot` and `strike_pilot_as` miss, crypt
  shots pass through and traps and pits ignore the pilot; firing waits for the roll to end.
  Relics: each `PilotBoons::extra_jumps` rolls `ROLL_SECONDS_PER_EXTRA_JUMP` longer,
  `glide_pct` rolls faster (further), `move_pct` walks faster, `featherfall` takes the hurt out
  of pits. New `PilotState` fields (serde default): `roll`, `roll_x/y`, `roll_cooldown`,
  `heading_x/y` (the last walk direction, quantized; auto-aim and the art face it).
  `PilotStepReport::rolled` emits `CryptRoll`. A top-down strike knocks straight away (no
  side-view lift). Same fixed 240 Hz steps as the platformer: tick-rate safe.
- **Layout** (`crypt::layout`, `CryptRealizer`, swapped into `realizer_for(TopDown)`): the
  plan's rooms on a `GRID` of 13 x 4 cells (11 x 9 interiors, shared walls), the main path a
  depth-first winding walk from column 0 (candidate sides ordered by a hash of the floor
  seed, east preferred; backtracking; a straight-row fallback), the vault on a free side of
  its parent, the boss hall 2 x 2 cells. Doorways are three tiles; a puzzle room's way on is
  always an east or west doorway holding a lever gate (lintel above, three bars, the lever in
  a corner, spikes round it from danger 5). Rooms: entry (start, the way out in a corner on
  floor 0), shrine, stairs, vault (caches on the far wall, a guard at danger 6+), ambush
  (four pillars, 2-6 ambushers), trap hall (a theme by variant, a second at danger 7+: a
  spike field rippling along the crossing, arrow-plate lines with a gap, boulder chutes on a
  side wall, flame-vent lines taking turns, cracked floor and pits round a safe wiggling lane
  two wide; turning rooms trade arrows and boulders for flames; a guard at danger 4+), boss
  (the crypt lord, four pillars). `CryptFloor { realized, traps, cells }`. Pure; tested for
  reachability of every room and fixture, gates under lintels in their lever's reach, safe
  pit lanes, clear spawns and every trap kind appearing.
- **Traps are overlays, not tiles** (`CryptTrap { at, kind, dir, phase, hidden, room }`,
  `CryptTrapKind`: `Spikes`, `ArrowPlate`, `BoulderChute`, `FlameJet`, `CrackedFloor`, `Pit`),
  kept in the run's `TombRun::crypt` (`CryptState`: traps in tile order, the floor's start
  tick, `crumbling` and `plates` maps saved as pair lists because saves are JSON), written by
  `prepare_crypt_floor` from `write_tomb_floor`. The floor under them is plain air, so
  creatures path, shots fly and sight lines pass over traps as over the floor (T27's
  `TombTrap` tiles and `advance_traps` stay side view's). Cycling traps (`CryptTrap::stage`,
  `progress`, `releases`; cycles in `CryptTrapKind::cycle`) run on milliseconds since the
  floor started (from the tick count: tick-rate safe) and always pass through a full
  `Warning` before `Active`. `world/crypts.rs` (`advance_crypt`, from the tombs pass while
  exploring): active spikes / flames hurt standing members whose footprint covers them
  (`damage_per_second` x `trap_tier_scale(tier)` x difficulty, the worst one only); chutes
  release a boulder; a plate stepped on clicks and `PLATE_DELAY_MILLIS` later looses arrows
  from both walls of its room along its line, re-arming after `PLATE_REARM_MILLIS`; a cracked
  floor stepped on gives way `CRUMBLE_MILLIS` later and stays a `Pit`; walking into a pit
  costs `PIT_FALL_SHARE` of max health (none with Featherfall) and climbs out at the nearest
  safe floor (`crypt_safe_floor`), briefly guarded. One `CryptTrap` event per room, kind and
  stage at a time. Like every tomb hazard, traps do nothing on Peaceful (difficulty 0).
- **Creatures** (`EnemyKind::{CryptSkeleton, Spectre, Cultist, CryptGolem, CryptLord}`,
  appended; `EnemyKind::is_crypt`, `passes_walls`; `crypt::bestiary(role, variant)`: ambushes
  mostly skeletons with spectres, cultists and now and then a golem, guards skeletons,
  cultists and golems, the boss the lord). `world/enemies/crypt.rs` drives them instead of
  the generic AI (`advance_enemy` hands them over): no gravity, `route` (the shared budgeted
  path search) and the shared swept body step with zero gravity and no bounce, then
  `free_from_rock` (rounding can tip a flush body a hair into a wall) and `keep_off_bodies`:
  the master body rules hold. **Spectres** are the one designed exception: no terrain
  collision or sight-line test, clamped to their floor, unable to start an attack from inside
  a wall, still pushed off pilots (`keep_ghost_off`). Skeletons hold flank stations round
  their prey (by seed) and lunge after a rattle; cultists keep `CASTER_RANGE`, circle and cast
  `ShotKind::Hex` (a fan of three every third cast); golems raise their fists (`CryptTell`
  with `CryptAttack::Slam`) and slam everything within `SLAM_REACH` (`CryptSlam`), winding up
  only at prey in line of sight and striking only pilots it can see (no slams through a wall). **The crypt
  lord** wakes on sight (phase 1, `CryptLordPhase`), then `lord_phase(health share)` moves it
  to 2 and 3: each new phase roars (`LORD_ROAR_MILLIS` pause) and calls up three skeletons, then
  two spectres (`CryptSummon`). Patterns cycle per phase (`lord_patterns`): fan of 5 and ring
  of 12 hex bolts; then charges (contact during the dash, ending at walls); then a spiral of
  three rotating waves of 8; every attack telegraphed by `CryptTell { attack, aim }`, faster
  in phase 3. Its health is `boss_health_scale(site tier)` like the guardian's. Directions
  come from a literal 15-degree table (no trigonometry: identical on every platform).
  `EnemyState::{phase, pattern, volley}` (serde default) hold the lord's and casters' state.
- **Crypt shots** (`ShotKind::{Hex, Arrow, Boulder}`, `ShotKind::is_crypt`) take
  `advance_crypt_shot`: straight, broken by solid tiles (never by trap overlays); hexes and
  arrows break on the first non-rolling pilot they meet, boulders roll through (one strike
  each, the strike guard does the rest). Trap shots strike as `CryptSkeleton` (only the run
  space's safety rules read the kind).
- **Events** (codes 290-296): `CryptRoll`, `CryptTrap { kind, fired }`, `CryptPitFall`,
  `CryptLordPhase`, `CryptTell { kind, attack, aim }`, `CryptSlam`, `CryptSummon`.
- **Balance** (`balance::crypt_bot::CryptBot`, which `TombBot` hands crypt runs to; the probe
  and the world tests use it): breadth-first paths over the floor that keep off pits and
  cracks (and plates when they can), waiting square on a safe tile while spikes or flames
  ahead are warning or up; the goals are an unpulled lever, an unlit shrine, unopened caches,
  a spot six tiles off the lord with a clear shot, then the stairs; it shoots the nearest
  creature in sight, rolls away from lunges and slams, off the lord's charge line and
  sideways to incoming shots, never into a pit. `--balance-report --only tombs` (4 seeds, 60
  Hz, after the wave 6 boss retune): crypts cleared 4/4 at tier 1 (1.6 min), 4/4 at tier 2,
  3/4 at tier 3, 0/4 at tier 4, 2/4 at tier 5; crypt wipes mostly at the lord and in ambushes
  ("Crypt wipes by room" and "Wipes by tier and style" notes).
  `crypt_bot::floor_text` prints a floor with its traps for debugging.
- **Scenario helpers**: `scenario_tomb_floor(player, floor)` (straight to a floor, the lord's
  hall), `scenario_crypt_traps(player, traps, started_millis_ago)`; `crypt_floor_millis(run)`
  for presentation.

Tests: `pilot/top_down_tests.rs` (eight ways, no clipping at any angle or speed, sliding,
corner nudges, roll distance, i-frames and cooldown, relic boons, tick rates);
`tomb/crypt/layout_tests.rs`; `tomb/crypt.rs` (cycles warn before they bite, chute releases
at any tick size, tile sets, bestiary); `world/crypt_tests.rs` (the pilot and creatures keep
to the walls through a bot fight, rolls through spikes, strikes and pits, cracks and plates
telegraph, spectres through walls, the bot beats the lord through three phases, the bot
clears a crypt of every tier on Peaceful, traps tick-rate safe, saves mid-floor with trap
state); the netcode e2e `world_crypt_runs_predict_rolls_and_converge`; the game's split
screen with mixed view modes and the crypt touch layout.

## Wave 6 fixes (collected open items from waves 3-5)

- **Tomb bosses** (`tomb::realize::BOSS_HEALTH_SCALE`): the guardian's and crypt lord's
  multiplier is a gentle table (0.5, 0.55, 0.6, 0.65, 0.7 by site tier) instead of
  0.25 + 0.25 x tier, because the creature tier (`world::tombs::enemy_tier`) already grows with
  the site tier and the floor: tier 5's guardian is about 3x tier 1's, not 6.6x
  (`the_guardians_health_climbs_gently_with_the_tier`). Probe (`--balance-report --only tombs
  --seeds 4`), cleared of 4 per tier, tomb / crypt: before 4/4, 2/4, 2/1, 1/0, 0/2; after 4/4,
  3/4, 2/3, 3/0, 1/2. Tier 3's remaining tomb wipes are a guardian and a fall on floor 0; crypt
  tier 4's lord wipes do not move with its health (they are the lord's damage against kit
  capped at tier 3). The probe now notes wipes by tier and style and names the room of a lone
  member's wipe (the run ended in the same tick, which the old note read as "a pit").
- **Free lift and the dead-air gap** (`environment::{gap_lift_share, jet_stream_in_air,
  LIFT_FADE_ROWS}`, `milestone::{free_lift_share, free_lift_fuel_limit,
  FREE_LIFT_FUEL_MARGIN}`): updrafts and vents fade over the 24 rows under the gap's floor,
  nothing lifts and no jet stream runs inside it (no generated lift reaches it today: the
  highest V9 vent air tops out 38 rows under it), and free lift is rationed like sky fuel: a rig
  that cannot cross the gap is lifted in full only while its tank is at least 4 % short of what
  climbing across from where it is would take (the climb table's estimate, a lower bound of
  the real climb), and not at all at that limit, so a free ride never leaves it able to cross.
  Rigs far short (every rig of the island bands' own tiers), crossers and everything outside
  the island bands ride as before. The altitude events only report updrafts that really lift.
  Tests: `free_lift_never_reaches_into_the_dead_air_gap`, `free_lift_rides_never_open_the_dead_air_gap`
  (every near-gap loadout carried free to the top of the lift and to mid-archipelago with the
  most fuel lift still carries, measured with the real movement step) and
  `an_updraft_will_not_lift_a_laden_rig_that_cannot_cross_the_gap` (a laden tier 12 rig rose
  17 rows on free lift before; now none, while a rationed one, a stock rig and a crosser ride).
- **Garage bay repairs** cost `home::GARAGE_REPAIR_UNIT_COST` (1 cr a point, half the town
  garage's), billed per point started, tick-rate safe.
- **Island Hopper** is earned against the world's own calibration (`milestone::island_hopper`,
  `SkyBands::calibration`, a new field next to `climb`; placement and digests unchanged).
- **Trap-Sight** is per seat in the game (`rendering::world::tombs::seat_trap_sight`): only the
  carrier's own screen shows hidden traps, in side-view tombs as in crypts.
- **Hand pick**: `pilot::pick_seconds` also takes soft sky ground (island turf and soil, roots,
  vines, the Moon's regolith) at the soft-dirt speed; rock, crystal, caches, nests, springs and
  temple stone still clink.
- **Village purchases** name their deal: `SessionServiceRequest::Village { deal: VillageDeal, .. }`
  (was `offer: u8`, the row). The tick buys that deal only if the counter offers it at that
  tick, so the dusk or dawn restock can never swap the item
  (`a_purchase_names_its_deal_so_the_dusk_restock_never_swaps_it`). `PROTOCOL_VERSION` 14.
- **Mechanism reach** is scoped to the mechanism's own space (`MechanismScope`: a generated
  pilot space by origin, a tomb run slot, or the open world): a lever or plate never works a
  gate of the neighbouring space (`a_lever_never_reaches_a_gate_in_the_neighbouring_space`).
- **Dawn banner** (game, `night_text::DawnWitnesses`): no "DAWN" banner after a night slept away,
  nor for a seat whose player was still in bed as the night lifted (one who lay down for a
  moment and got up again stayed up and still sees it).
- **Crypt trap shots** honour Sandwalker Sandals per pilot hit (see "Wave 5 integrated").
- **Rock mimic**: contact 8 (was 14) every 2 s (was 1.3 s): with its lunge now connecting it
  cost a third of a band's typical hull in a starter-gun duel (46 of 140 in the grey stone, 412
  of 1,180 in the void geodes); now 13 and 134 (`rock_mimics_bite_fairly`). The up-to-1-px
  overlap of an enemy pinned against a wall by a ramming rig is left as is (the rig moves before
  enemies and rock wins the push; `OVERLAP_TOLERANCE` documents it).
- **Hull warning** on other rigs' nameplates is a share of their max hull
  (`LOW_HULL_FRACTION`, 35 %, the HUD's), not a fixed 30.
- **Phone layout**: the clock badge steps left of the thumb cluster's buttons
  (`clock_hud::badge_rect`, `touch_controls::cluster_rects`) where they rise level with it on a
  landscape phone (checked at 844x390 and 390x844).
- **Cleanup**: the tracked `screen.rs.orig`, T27's `TopDownRealizer` and other dead code are gone.
- **Formats**: `PROTOCOL_VERSION` 14 (the village deal); no save format change; no worldgen
  change (V1-V10 digests unchanged). No new event codes.

## Wave 5 integrated: side-view tombs and top-down crypts together

T27 (core), T28 (side view) and T29 (crypts) on one branch. Each site is one style, fixed by
its seed (`TombSite::style`, shown on its entrance art and map marker), the same on every floor;
generator V10 alone places sites (V1-V9 digests unchanged, V10's pinned once by T27).

- **One kind list**: `EnemyKind::ALL` is 39 long: T28's tomb creatures (`Mummy` ..
  `HollowMatriarch`, `EnemyKind::is_tomb`) then T29's crypt creatures (`CryptSkeleton` ..
  `CryptLord`, `EnemyKind::is_crypt`). Both builders named their shots `Arrow` / `Hex`, so
  T28 keeps `ShotKind::{Arrow, Rubble, Hex}` (archer arrows that drop, colossus rubble, the
  matriarch's slow hex orbs) and T29's are `ShotKind::{CryptHex, CryptArrow, Boulder}`
  (`ShotKind::is_crypt`: they fly over trap overlays and only crypt masonry stops them).
  Each style's art and cue keep their own look (`tomb_art` vs `crypt_art`, `FogBurst` vs
  `HexBolt`).
- **One scenario**: `WorldState::scenario_tomb_floor` (T29's, in `world/crypts.rs`) serves
  both styles; it also resets the run to `Exploring`.
- **Shared relics**: every relic is offered in both styles (`Relic::suits`). Sandwalker
  Sandals and their synergies (`RelicEffects::trap_pct`) now blunt a crypt's overlays and pit
  falls as they do side-view traps and T27's spikes; since wave 6 also a trap's arrows and
  boulders, which are spawned once for the party and scaled for the pilot they hit
  (`advance_crypt_shot`). Loot, banking, unlocks, the codex and trophies are T27's and do not care
  which style a run is.
- **Per seat**: a seat's view, camera, minimap, music and HUD follow its own player's run
  (`PilotView`): in split screen one seat can crawl a crypt while the other plays a
  side-view tomb or flies the rig. The pilot HUD shows ROLL (the dodge roll's recovery, in
  percent) in place of JET in a crypt.
- **Ticks**: `advance_side_view_run` and `advance_crypt` both run in the tombs pass; each
  returns at once for the other style.
- **Formats**: one `PROTOCOL_VERSION` (13) and one save format (11) for the whole wave;
  events 270-279 (T27), 280-281 (T28), 290-296 (T29).

## Wave 5 seams: top-down crypts (T29) for the integrator and T28

- **Shared core touched (additive)**: `realize::realizer_for` and `bestiary` route
  `TopDown` to `crypt` (T27's `TopDownRealizer` is left in place, unused: delete it when
  merging if nothing else needs it); `TombRun::crypt` (serde default); `PilotState::{roll,
  roll_x, roll_y, roll_cooldown, heading_x, heading_y}` and `PilotState::height` answering the
  footprint in top-down; `PilotStepReport::rolled`; `EnemyState::{phase, pattern, volley}`;
  five `EnemyKind`s and three `ShotKind`s appended (`CryptHex`, `CryptArrow`, `Boulder`
  after integration; `EnemyKind::ALL` is 39 long with T28's); hooks
  `advance_crypt` (tombs pass) and `prepare_crypt_floor` (`write_tomb_floor`); the generic AI
  and shot advance hand crypt kinds to `world/enemies/crypt.rs`; `Relic::suits` is now true
  everywhere (`blurb_in` for crypt meanings); `damage_pilot` / `strike_pilot_as` respect
  `roll_guarded`; pilot weapons hold fire mid-roll and auto-aim faces the heading.
- **Not used by crypts**: `TrapKind` / `BuiltTile::TombTrap` / `advance_traps` (all T28's).
- **Formats**: folded into `PROTOCOL_VERSION` 13 and save format 11 (noted there); events
  290-296.
- **Presentation hooks** (game): `rendering/world/crypt.rs` replaces the whole world draw for
  a view whose player is in a crypt (per seat); `render_snapshot::CRYPT_ZOOM` zooms such a
  view; `rendering/crypt_map.rs` takes the minimap; `audio::director::crypt_music` plays
  `Patch::CryptLoop` / `CryptBossLoop` (appended at the end of the patch list).

## Wave 5 seams: tombs core (T27) for T28 and T29

- **T28 side-view content**: replace `SideViewRealizer` (keep the `FloorRealizer` contract:
  pure, inside `MAX_FLOOR_SIZE`, the start tile open, every room reachable with the stock
  pilot). New traps: append `TrapKind` variants (tile `kind` index, at most `TRAP_KINDS`),
  give them `damage_per_second` or a behaviour in `WorldState::advance_traps`; their art goes
  in the game's `tile_art/built_art.rs` (`paint_tomb_trap`). New enemies and bosses: add
  `EnemyKind`s as usual and return them from `realize::bestiary` for `SiteStyle::SideView`;
  the boss's health is scaled by `realize::boss_health_scale(site tier)` and its creature
  tier by `world::tombs::enemy_tier` (tune both against `--balance-report --only tombs`,
  whose notes say in which room the bot's wipes happened). Tile sets: `BuiltTile::TombWall(style)` is the masonry (art `paint_tomb_wall`, ramps
  `tomb_ramp`); add culture variants by new built codes or by drawing per site setting. Run
  music: the game's `audio::cues::event_cue` maps the tomb events to placeholder cues today;
  the director can switch music on `world.tomb_run_of(player)`.
- **T29 top-down crypts**: the pilot's top-down mode is `PilotState::view ==
  PilotView::TopDown` (set by `refresh_tomb_pilot` from the run's style); replace
  `pilot::step_top_down` (movement, dodge roll, collision footprint) and the game's
  placeholder floor (`rendering/world/tombs.rs::draw_tomb_floors`) and camera. Replace
  `TopDownRealizer`; crypt enemies and the crypt lord come from `bestiary` for
  `SiteStyle::TopDown` (top-down placeholders fly because enemies still fall under gravity
  in the vault). Touch: the top-down mode reads the same held directions (up / down /
  horizontal), so the d-pad already walks eight ways.
- **Both**: rooms and spawns arrive in `TombRun::rooms` / `spawns` (absolute tiles); ambushes
  spring per room (`TombRun::sprung`); the guardian is `TombRun::boss`, its fall sets
  `boss_defeated`. Relics: add `Relic` variants and `RELICS` rows (append only: the index is
  the codex entry). Story arcs (T26) bind a site with `tomb_sites::nearest_site` and open it
  with `unseal_tomb_site`; lore pages are `tomb::LORE_PAGES`.
- **Formats**: `PROTOCOL_VERSION` 13 and save format 11 for the whole wave (the integrator
  collapses T28 / T29 bumps into these); events 270-279 are T27's, T28 has 280-289 and T29
  290-299.

## Side-view tombs (`src/tomb/{side_view,culture,traps}.rs`, `src/world/{tomb_traps,enemies/tomb}.rs`, wave 5 T28)

The side-view half of WP-TOMBS on T27's core: a platformer roguelite on the pilot's own
movement and personal weapons. Nothing here changes generation (V1-V10 digests unchanged: the
entrances stay T27's set-0 masonry on the map and are painted, doors included, in their culture by the game).

- **Cultures** (`tomb::culture::TombCulture`, from `TombSite::setting`): `Sandstone` (mesas),
  `Catacomb` (deep strata, old quarters), `SkyTemple` (sky continents), `LunarVault` (the
  Moon). A culture is a masonry set of `BuiltTile::TombWall` / `TombPillar`: 0 is T27's
  sandstone (code 72/74), 2-4 are new (built codes 120-125, `culture::masonry_extra`).
  `FloorPlan::setting` (new field, filled by `plan_floor`) carries the setting to the
  realizer. `floods_with_water` makes sky and lunar sluices pour water.
- **The realizer** (`tomb::side_view::SideViewRealizer`, `realizer_for(SideView)`): the main
  path west to east on one doorway line (`FLOOR_ROW`), a masonry hull round the whole floor.
  Templates per room kind, rolled from the room's variant, the culture's weights and the
  danger: trap halls (`TrapTemplate`: spike pits with a dart thrower over the east doorway
  from danger 5 under a 4-row ceiling, a blade gallery, a crusher run, a collapsing bridge, a
  sand basin, a fog corridor), puzzles (`PuzzleTemplate`: a lever mound kept within
  `mechanism::REACH_COLUMNS` of its gate, a pit floored with pressure plates a dash from the
  gate), ambushes (`AmbushTemplate`: floating ledges, a mezzanine that stops two short of the
  east wall), the shrine between pillars, the guardian's arena (two side platforms and a high
  one), the stairs, and the vault above a puzzle or ambush room up a step and three ledges on
  its east end (a third of the time behind a hidden false wall). Secret cellars (a hidden false
  floor slab over a one-row drop with a cache) in some entry, shrine and stairs rooms. Every
  main-path room is crossable by the stock pilot (one-tile steps, two-row jumps, two-wide gaps)
  and by the tombs bot. No moving platforms (no moving solid bodies in the simulation yet).
- **Traps** (`TrapKind` indices 1-7, appended: `Blade`, `Crusher`, `Crumble`, `DartWall`,
  `SandSluice`, `FogVent`, `FalseWall`; `pilot_passable()`: only spikes and false walls let a
  pilot in, `pilot::pilot_passable` asks it). Every moving trap is a **pure function of the
  world clock** (`tomb::traps`: `world_millis(tick, hz)` is tick-rate safe; no `sin`, only
  basic float arithmetic, so desktop and browser agree): `blade_tip`, `crusher_state` /
  `crusher_block`, `dart_state` / `dart_position` (a dart thrower's `hidden` bit is its facing,
  `side_view::dart_facing`), `sand_state` / `sand_surface` (per room interior), `fog_state` /
  `fog_hurts`. Each has a tell (a crusher shakes `CRUSHER_SHAKE_MILLIS`, a dart's eye charges
  `DART_CHARGE_MILLIS`, sand trickles `SAND_TELL_MILLIS`, wisps gather `FOG_GATHER_MILLIS`) and
  a safe window or strip. `WorldState::advance_side_view_run` (`world/tomb_traps.rs`, called
  from the tombs pass right after the spike check) scans the traps around each standing member,
  hurts by their damage rates (`BLADE_DPS`, `CRUSHER_DPS`, ...) times the enemy difficulty's
  damage (nothing on Peaceful, like T27's spikes) and the member's `RelicEffects::trap_pct`,
  shoves a pilot out from under a crusher, crumbles slabs a grounded member stands on
  (`TombRun::crumbling`, `CrumblingSlab`: falls after `CRUMBLE_MILLIS`, sets again
  `REFORM_MILLIS` later when nothing stands in the hole) and reports `TombTrapSprung` near the
  party (a blade as it swishes through the bottom of its swing, so `BladeSwish` plays).
- **Creatures** (`EnemyKind::{Mummy, CryptBat, SkeletalArcher, AnimatedStatue}`, appended; tomb
  runs only, never in caves or the director's rosters): the shared enemy machinery and body
  rules (`step_body`, `free_from_rock`, `keep_off_bodies`, `contact_half`). Statues start
  dormant like rock mimics. New shots `ShotKind::{Arrow, Rubble, Hex}`.
  `side_view::bestiary(role, variant)` is `realize::bestiary` for side view.
- **Guardians** (`EnemyKind::{SunColossus, HollowMatriarch}`, `world/enemies/tomb.rs`): the
  phase is `side_view::guardian_phase(health fraction)` (2 below `GUARDIAN_PHASE_TWO`, 3 at
  `GUARDIAN_PHASE_THREE`); the move is `guardian_move(kind, fraction, wind-up deadline)`, a
  pure rotation by the second, so the wind-up, the release and the renderer agree. Colossus:
  `LeapSlam` (a leap at the prey; landing throws rubble both ways), then `RubbleToss` and
  `RaiseDead` (up to `MAX_RAISED` mummies), enraged in phase 3 (faster, three-fifths
  cooldowns). Matriarch: `HexFan`, then `HexRing` and `SummonBats` (up to `MAX_BATS`), then a
  `Dive` that hurts on contact. Hooks in `advance_enemy` / `behave` (three guarded lines) and a
  ranged enemy's contact now counts in `Strike` (only the dive ever strikes). The run tracks the
  phase (`TombRun::boss_phase`) and reports `TombGuardianPhase`. Guardians **sense intruders
  without line of sight** (`perceive` treats them like footstep-feelers, within their hearing):
  otherwise a pilot under a crypt platform made them forget, walk home (`EnemyMode::Return`)
  and regenerate back into phase 1. The colossus **skips path routing** (`move_enemy` steers
  it straight at its goal: paths round the arena's platforms stalled its wide body), may wind
  up with rock in the way, and its leap overshoots prey under its ledge by
  `LEDGE_OVERSHOOT_TILES` so it comes down beside them.
- **Relics**: `Relic::Sandwalker` (appended: traps hurt half), `RelicEffects::trap_pct` (serde
  default; also scales T27's spike damage), and four appended `SYNERGIES`: Cloud Dancer (Spring
  Heels + Featherfall), Updraft (Glider Boost + Featherfall), Tomb Runner (Quickstep +
  Sandwalker) and Seer's Path (Trap-Sight Lantern + Sandwalker).
- **Events** 280-281: `TombTrapSprung { kind, x, y }`, `TombGuardianPhase { kind, phase, x, y }`.
- **Formats**: no new bump; the additions ride the wave's `PROTOCOL_VERSION` 13 / save format
  11 (listed in `netcode::protocol`). New fields are serde-default.
- **The bot** (`balance::tomb_side::SideBot`, inside `TombBot`): predicts its body over
  `LOOK_AHEAD_MILLIS` walking on, waiting or backing off against every timed trap near it,
  never stops on a crumbling slab, waits at a sand basin's edge for a long dry spell, stops over
  a plate pit, goes back for a lever it passed (`SideBot::lever_behind`) and walks back to a
  guardian that hung back. Debug: `print_where_the_bot_sticks`, `print_what_hurts_the_bot`
  (ignored tests).
- **Balance** (`--balance-report --only tombs`, 4 seeds): side-view tombs clear 4/4 at tier 1
  (0.9 min), 2/4 at tiers 2 and 3 (1-1.2 min), 1/4 at tier 4 (2.3 min) and 0/4 at tier 5 (3
  wipes, 1 timeout against the guardian), wipes nearly all at the guardian (the bot never dodges and fights with the rifle alone: a
  tier-5 floor-6 guardian has about 1,500-1,750 health against the rifle's ~27 per second).
- **Scenario helpers**: `scenario_next_tomb_run(site, run)`, `scenario_tomb_floor(player,
  floor)`, `scenario_tomb_guardian(player, kind)`, `scenario_hurt_enemy_to(id, fraction)`.

Tests: `tomb::side_view` (cultures, every trap built, traps set where their shapes need room,
every gate has its lintel and a mechanism in reach, both guardians), `tomb::traps` (the series
against the library, tick-rate safety, tells and safe windows, a blade clearing a standing
pilot at the top of its swing), `tomb::culture` (codes round-trip, T27's codes unmoved),
`world::enemies::tomb` (distinct patterns per phase), `world/side_tomb_tests.rs` (blades at
30/60/120 Hz and on two peers, the safe strip, a crusher's tell, slam and shove, crumbling
slabs, the sand basin, tomb creatures never in rock or on the pilot (also against a moving,
shooting pilot at 30/60/120 Hz with raised and summoned creatures), both guardians beaten by
the scripted bot through phases 2 and 3, a guardian never forgetting or healing while a pilot
shelters under a platform, a run cleared on every tier, an armed tier-1 clear on Normal, a
party of two clearing together, the phase saved in the run), and the netcode end-to-end
`world_side_view_tomb_traps_replicate` (a collapsing bridge falls online and every client
converges).

**Physics seam (found by the tomb creature test):** `physics::step_body` quantizes positions
to the subpixel grid at the end of a step; rounding to the nearest subpixel could tip a body
resting a hair short of a tile into it (an animated statue's lunge at 30 Hz ended a tick in the
floor). The step now rounds the other way on the axis that clears it in that case only
(`on_subpixel_grid`); every step that did not end in rock is unchanged.

## Wave 5 seams: side-view tombs (T28)

- **T29 (crypts)**: `TrapKind` indices 1-7 are side view's; append the crypt's traps from 8.
  Built codes 120-125 are taken (culture masonry). `realize::Canvas` gained `with_masonry`
  and `hull` (both styles may use them). `realize::bestiary` now delegates the side-view arms
  to `side_view::bestiary`; the top-down arms are untouched. `contact_attack` lets a ranged
  enemy hurt in `Strike` (a crypt lord's charge can use it). `Relic::suits` still offers
  Sandwalker in crypts (spikes and T29's traps read `trap_pct` if they go through
  `RelicEffects::scale`).
- **Integrator**: T28 added no protocol or save bump (fold into 13 / 11 with T29's). Events
  280-281 are used; 282-289 are free.
- **Stories (T26)**: `TombCulture::of_setting` and `TombSite::setting` tell an arc which builders
  a tomb belongs to.

## Upgrade rewards and milestones (`src/milestone.rs`, `src/upgrade_preview.rs`, `src/world/milestones.rs`)

WP-FEEL part 2 (wave 3, T18). Deterministic rules only; the game draws the moments.

- **Upgrade previews.** `upgrade_preview(player, kind, sky)` (next tier) and
  `preview_between(player, kind, from, to, sky)` (`sky`: the world's
  `GeneratorVersion::sky_bands()`, `None` without a sky: then no sky lines) return an
  `UpgradePreview`: before/after `StatBar`s
  (scaled to the line's top tier) and measured change lines, most important first ("Now cuts
  hard rock", "Drills granite 1.2x faster (0.67 s -> 0.55 s a tile)", "Climbs 50% higher on a
  full tank", "Runs cool down to the Slate Shelves", "Immune to lava burns!"). They use the
  tick's own formulas: `dig_seconds` (base chip seconds, `drill_speed`, `drill_bite`,
  `MIN_DRILL_SECONDS_PER_CHIP`), `thrust_seconds` / `climb_speed_tiles` /
  `climb_ceiling_tiles` (tank size, burn efficiency, capped climb speed; at sea level, worlds
  without a sky) or `thin_air_ceiling_tiles` (the measured climb table above its 100-row start,
  worlds with a sky: thin air caps big tanks without intakes, and the tank preview then says
  so instead of a 0 % gain), `cooled_band`,
  `protection_fraction`, `engine_top_speed_tiles`; the "Dig speed" bar is
  `dig_speed_multiplier` (the measured hard-rock speed-up over a stock drill, flat where a tier
  adds nothing). The game's first-use drill toast also reads `dig_seconds`. Stock parts, empty
  bay, sea-level air. The
  base chip table moved to `world::base_drill_seconds_per_chip` so the preview and the drill
  share it. Tiers that change nothing on common rock (the chip-rate floor, e.g. drill 9 -> 10)
  say so instead of inventing a gain.
- **Milestones.** 19 `Milestone`s in four groups (upgrades, depths, discoveries, sky), each
  with a `Medal` and one or two reward cosmetics (`Milestone::rewards`, all new non-starter
  items: 5 paints, 3 flames, the corkscrew drill, 2 patterns, 6 decals, the depth trophy roof
  piece and the trail lantern). Rules are state-based (`Milestone::met` over the `Player` and
  `MilestoneFacts`), so nothing is missed and old saves earn what they already did.
  `WorldState::advance_milestones` (once per tick, after village discovery) updates each
  player's `MilestoneRecords` (deepest row and band, highest altitude above the local ground,
  kills credited by `EnemyKilled { by }`, artifact pickups, upgrades bought), records newly met
  milestones in `Player::milestones` (`MilestoneLog`: tick earned, records, `primed`), unlocks
  their rewards and emits `MilestoneEarned`. A log's first check is silent (old saves and fresh
  rigs do not get a burst of toasts). Test-mode worlds count nothing.
- **Vendors.** Village cosmetic vendors stock `CosmeticItem::is_vendor_stock` only (the six
  wave-2 unlocks), so milestone rewards are earned, never sold, and village offers are byte-for-
  byte what they were.
- **Purchase moment.** An applied `BuyUpgrade` (command or service) is followed by
  `UpgradeInstalled { player_id, upgrade, level }` (level now fitted; bomb packs report 0).
- **Sky gates.** `milestone::climb_reach(player)` reads the current fuel curve's measured
  climb table (`climb::reach(tank, engine, intake)`, engine and intake rounded down to the
  measured tiers) and compares it with **the world's own bands**:
  `island_capable(player, &bands)` is a reach of `bands.island_reach_rows()` (40 rows into the
  low isles) and `crosses_dead_air(player, &bands)` a reach of `bands.dead_air_reach_rows()`
  (the continents' floor). `MilestoneFacts::sky_bands` (was `has_sky: bool`) carries the
  world's `GeneratorVersion::sky_bands()`; callers outside the world (story quests, previews)
  must pass the world's bands too, never assume V8's. On next-generator bands the design
  holds: tank 2 (stock engine) reaches the isles, the stock rig does not; crossing the dead air
  needs a near-max tank and intakes (tank 13 + engine 15 + intake 6, or tank 15 + intake 4).
  On V8 worlds (calibrated on the old, smaller fuel curve) the bigger tanks open the bands
  earlier: a stock tank already reaches V8's low isles and tank 10 + engine 15 + intake 2
  crosses V8's dead air; no rig crosses without intakes. (Wave 6: `IslandCapableTank` reads
  `milestone::island_hopper`, which also asks the table the bands were calibrated from,
  `SkyBands::calibration`, so a stock rig in a V8 world no longer earns it at once.) They decide the `IslandCapableTank` and `ActTwoUnlocked`
  (Act I -> II) milestones and the shop's "Reaches the low sky isles!" / "Crosses the dead-air
  gap" lines on the tank, engine and Altitude Intake previews. `HighFlyer` now needs 420 tiles
  above the ground (a stock rig climbs about 400 on a full tank). The shop also previews the Altitude Intake (thin-air thrust) and De-Icer (cold
  cover) lines.
- **Replication and saves.** `Player::milestones` is private: it travels in the owner's
  `Loadout` section (`PlayerInventory::milestones`), is saved with the player (serde default:
  old saves load with an unprimed log) and parks with the rig. `PROTOCOL_VERSION` 11.
- **Events.** `UpgradeInstalled` 180 and `MilestoneEarned` 181 (netcode codes). Online, a
  predicting client also runs `advance_milestones` on its predicted world, so it can present a
  milestone twice (predicted, then authoritative a few ticks apart): the game drops a repeat of
  the same milestone within 20 s (banner and fanfare).

`milestone.rs`, `upgrade_preview.rs` and `world/milestones.rs` test the unlock rules (tiers,
depth bands, sky gates, silent priming, test mode), rewards (each locked item awarded once,
never sold), persistence (serde, loadout replication, redaction, parking) and stat-delta
correctness against the formulas.

## Balance harness (`src/balance.rs`, `src/balance/`)

Expansion plan T19: deterministic bot players drive the real tick (held `ControlState`s and
`PlayerCommand`s only, through `simulate_tick`) across seeds and print a Markdown report.
Probes may set a scenario up directly (carve an arena, hand a rig a weapon) but never
shortcut the tick. Same config, same report, on every machine.

- **Report:** `fatherload --balance-report [--seeds N] [--minutes M] [--hz H] [--only
  economy,progression,combat,exposure,building,climb,trip] [--strategies shaft,prospector,charges]
  [--threads T]` (release build), or `cargo test --release -p fatherload_simulation
  balance_report -- --ignored --nocapture` with `BALANCE_SEEDS`, `BALANCE_MINUTES`,
  `BALANCE_ONLY`. `trace_economy_bot` (ignored; `BALANCE_TRACE=1 BOT_SEED BOT_STRATEGY
  BOT_MINUTES`, `BALANCE_TRACE_HAZARD=1`) prints one bot's run second by second.
- **API:** `balance::run_report(&BalanceConfig) -> BalanceReport` (`to_markdown()`,
  `metrics()`), `run_probes` for a custom probe list, `BalanceConfig::seeds(n)`.
- **Economy bot** (`bot::MinerBot`, `economy::run_economy(seed, strategy, minutes, hz) ->
  EconomyRun`): a fresh world, a stock rig, shafts in the gaps between the town's buildings,
  sideways tunnels to visible ore (`Strategy::Prospector`), mining charges on ore-rich rows
  pressed at its own surface workshop (`Strategy::Charges`), straight shafts only
  (`Strategy::Shaft`, the naive baseline: the stock drill cannot bite clay, topsoil from one
  shaft barely pays for its fuel, so it never affords drill II and goes broke), hazard and hard-rock side-steps,
  fuel reserve measured on earlier climbs, trips home to sell, refuel, repair and buy
  upgrades (cheapest weighted line, a reserve kept), a rivet gun once affordable, rescues when
  stranded. `EconomyRun` keeps per-minute samples, purchases, band times, damage by enemy
  kind, the rescue log and a trip log (`BotStats::trip_log`: fuel burned shaft mouth to town
  against the tank flown, depth, length and why it turned home).
- **Progression:** time to each tier of the core lines (the prospector's runs: the shaft
  baseline never upgrades and would drag the medians down), fuel per trip by the tank flown
  (metrics `fuel.tank{t}.median_tank_percent` / `.p90_tank_percent` / `.fuel_turn_percent`),
  depth and earning rate over time, the minute each band was entered.
- **Everyday trip** (`trip::everyday_trip(seed, tank, hz) -> TripResult`, probe `trip`): a
  scripted rig with the first drill upgrade sinks a shaft beside town (the nearest column clear
  of buildings it can drill solid 60 rows down), drills 50 rows, mines for two minutes
  (8-tile tunnels each side, two rows deeper, again; a one-second hop every 20 s) and flies
  home to land beside the shaft. Fuel burned and the share of each tank (metric
  `trip.tank{t}.tank_percent`): the fuel curve's yardstick. `BALANCE_TRACE_TRIP=1` traces it.
- **Combat** (`combat::duel(kind, band, loadout, hz) -> DuelResult`): a sealed arena at a
  band's depth (bedrock past a stone shell, lair chunks marked cleared through
  `WorldState::mark_lair_cleared`, the deep reward tile cleared), one enemy, a rig with one of
  `combat::LOADOUTS` aiming at it. **Exposure** (`combat::exposure(seed, band, seconds, hz)`):
  an unarmed rig of `band_gear(band)` drilling down from a band's top on Normal; enemy damage
  by kind per minute. Measuring rigs are topped up to a huge hull every tick and read losses
  against it.
- **Building** (`building::run_building(hz)`): two claims of every module in a slate cavern
  through real `BuildCommand`s from a hopper big enough for all of them, so the credits and
  blocks columns are each module's real price (a player short of blocks pays their market price
  on top); minutes of income at the unlock band, the fuel station's synthesis per minute.
- **Climb** (`climb::climb(seed, tank, engine, hz) -> ClimbResult`, the plan's T19 "climb
  bots"): a rig of the given fuel-tank and engine tiers holds thrust straight up from the
  surface until the tank is dry; apex, seconds to it and the turn-back altitude (half the tank
  burned) per `climb::TANK_TIERS` x `climb::ENGINE_TIERS`, metrics
  `climb.tank{t}.engine{e}.apex_tiles` / `.turn_back_tiles`. It reads only the tick, so T16's
  altitude physics (air density, cold, gravity fields) shows up in it on integration; T16's
  own climb harness stays the authority for sky band placement. In a V8 sky without intakes
  every climb tops out where thin air leaves the engine too weak: about 480 tiles for a stock
  engine, 990 for engine 6 and up; a stock rig climbs about 400 tiles on its 450-fuel tank
  (120 on the old 100).
- **Pinned tests** (`balance/tests.rs`): the bot mines, sells and upgrades without rescues;
  early burrowers bite softly; elites fall in set-piece time; armour still matters; bigger
  tanks and engines climb higher (turn back past half the apex); an everyday trip burns under
  40 % of a stock tank; the building probe pays list prices.
- **Wave 4 hooks:** implement `balance::BalanceProbe` (`name`, `run(&mut ProbeContext) ->
  ReportSection`) and add it to `standard_probes()`. `ProbeContext::economy()` shares the
  economy runs, so a probe can price things in minutes of income; `ReportSection::metric`
  names numbers for pinned tests. Planned: a sky probe (time to each island band, on top of
  the climb probe's altitude table), a Moon probe and a pilot probe; new
  `Strategy` variants for new loops (sky hopping, pilot salvage).

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

## Wave 4 seams: the night (T25)

- **Event codes** 250-253 (`NightFell`, `NightSurvived`, `RareNightSpawn`,
  `NightMonsterBurrowed`).
- **Enemy kinds** are appended to `EnemyKind` (and `EnemyKind::ALL`); sky and Moon enemies
  (T21, T22) append after them. Cave-habitat and safe-zone matches list them explicitly.
- **Safe zones**: new enemy checks should call `hostile_safe_zone(Some(kind), tile)`; sky
  enemies that hunt the open sky want a rule like `night_safe_zone` rather than
  `enemy_safe_zone` (whose surface strip covers the whole sky).
- **Light**: `fixture_light_at` is the fixed light other tracks' lamps (homes, sky towns)
  should feed so night safety follows them.

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

## Wave 4 seams: sky isles and towns (T20)

- **Generator V9** is current (`has_sky_isles`); other wave 4 tracks that change generation
  should extend V9 (the integrator re-pins `GOLDEN_V9_DIGEST`). Content lives in island cells
  (`worldgen::sky::cell_lands`); continents and the Moon are untouched (T21/T22).
- **Sky towns are villages** (`VillageSetting::Sky`): story arcs (T26) and night rules (T25)
  get them through the village APIs; they are safe zones via `in_village_area`.
- **Gear seams**: `HazardGear::pressurized_hull` (tiers 2-3 reserved for orbit and the Moon,
  T22) and `HazardGear::cold_cover()` (pilot suits, T23, can add to it); aether thrusters
  never change the climb table.
- **Ids and codes**: `SKY_TOWN_FLAG` (bit 30 of `VillageId`), sky tiles 0x1D-0x25, minerals
  `Skystone`/`CloudSilver` and artifacts `WindAstrolabe`/`CloudPearl` appended (their tile
  codes follow), events 200-203, `PROTOCOL_VERSION` 12; save format unchanged (new fields
  default, an older build rejects a V9 save by its unknown generator).

## Contractor-built homes (`src/home.rs`, `src/home/`, `src/world/homes.rs`)

Expansion plan WP-HOME (wave 4, T24). One shared world field, [`home::Homes`] (serde default:
older saves load with none), replicated as the `SectionKey::Homes` on-change section and hashed
only once anything was bought.

- **Plots and deeds** (`home::plot`, `home::Plot`, `home::Deed`). `PlotSize` (small 11, medium
  16, large 24, estate 34 tiles wide) and `PlotSetting` (town, village outskirts, lakeside,
  clifftop, meadow, wilds, sky island, Moon) price a deed. Listed sites are pure functions of the
  settings: `plot::listed_sites_between(settings, first, last)` / `listed_site(settings, key)`
  (`SiteKey::Town(i)`, `Village(id, end)`, `Scenic(slot)`; level ground clear of the town,
  villages, camps and trail milestones; scenic spots every `SCENIC_SPACING` columns on biome
  worlds). `WorldState::survey_plot(player, size)` checks a free spot under the actor (open
  ground near the surface or on a sky land's turf, steps of at most 4 rows, clear of the town,
  villages, claims, plots and listed sites) and prices it with `plot::survey_price` (cheaper far
  from town, plus `GRADING_PRICE_PER_TILE` for earth moved). A `Deed` has the owner, co-owners
  (decorate, hire, sleep) and a `HomeAccess` (`Crew` decorates, `Visitors` default, `Owners`).
  Claims and plots never overlap (`BuildRefusal::OnPlot`; claims well under a plot are fine, the
  lift runs to them). `Homes::is_safe_zone` is added to `enemy_safe_zone`.
- **Blueprints and expansions** (`home::blueprint`). `Blueprint` (cabin, cottage, house,
  workshop home, manor, sky villa only on a sky land, lunar habitat only on the Moon): footprint,
  smallest plot, rooms by floor, `BuildJob` (usual tenth-days and price; big plots add grounds
  time) and the blocks it uses. `Expansion`: a room (`ROOM_WIDTH_TILES` wider), a floor, a
  garage bay, a basement (digs the cellar), a private lift (needs the basement and a claim of
  the owner's within `LIFT_REACH_COLUMNS` below). `HomeJob` is either.
- **Contracts** (`home::construction`). `HomeCommand::Hire` signs a `Construction` with a
  contractor from `WorldState::contractors_for(plot, client)` (builders' trades of the town and
  villages within `CONTRACTOR_REACH_COLUMNS`, nearest first; the builders' guild only where
  there are none, as on worlds without people): `people::contractor` quotes the job by
  personality; repeat clients (`Homes::jobs_with`) get `REPEAT_CLIENT_DISCOUNT_PERCENT` off and
  supplied hopper blocks `SUPPLIED_BLOCK_DISCOUNT` each (capped). Progress is
  `Construction::progress_per_mille(world_millis)`: elapsed **world time** minus holds plus
  rushes, so sleeping moves it on and it is tick-rate safe. `BuildStage` (survey, foundation,
  frame, walls, roof, finishing, done) is announced once each (`ConstructionStage`); reaching
  the foundation levels the house's ground (terrain ops: rock cleared above, a masonry footing
  on the ground row, dips filled). `plan_events` rolls story beats per contract
  (`SiteEventKind::Shortage` holds work until answered or `shortage_patience`,
  `Request { item }` raises quality by one when answered with the item, `Anecdote`); answering
  is `HomeCommand::AnswerEvent`. `Construction::rush_quote` skips half the work left for
  credits. Hand-over makes the `Home` (or applies the expansion), records the job, warms the
  contractor (`HANDOVER_GOODWILL`) and gives a housewarming piece by quality.
- **Site crews.** During `WORK_START_MINUTE..WORK_END_MINUTE` the contractor and helpers from
  their settlement are NPC entities on site with `NpcState::site` set (the people pass leaves
  them alone and their village copies step aside; `world/homes.rs` walks them between stations).
  Talking to them is the ordinary `PeopleCommand::Talk`; the building-work topic with your own
  contractor answers with the job's stage and finish (`site_talk`).
- **Homes and decor** (`home::Home`, `home::decor`). Rooms by floor (basement -1) with a
  `RoomDecor`: wallpaper, flooring and a `ROOM_COLUMNS` x `ROOM_ROWS` grid (floor, wall,
  ceiling) of `Furniture` (widths, rows, rugs under floor pieces). Services:
  `UsePiece` on a bed (owner: home point, `Homes::home_point_of`, used first by `rescue_home`;
  at dusk or night `BedSpot::Home` puts the player to bed), a stove or fireplace (heals the
  pilot), a telescope; `Stash` (chests and wardrobes, `Home::storage_capacity`), `Display`
  (trophy cases take artifacts and rare ore from the bay), `CraftFurniture` (needs a workbench,
  blocks plus credits), `Place` / `PickUp` / `Restyle`. Stocks (`Homes::furniture_of`) come from
  `BuyFurniture` at a land office, crafting, housewarming gifts, antique pieces found in cave
  caches (`decor::cache_find`: relic caches always, one treasure cache in three; reported on
  `CaveCacheOpened::furniture`) and `WorldState::grant_furniture` (the seam for quest rewards). A rig parked in a garage bay regains
  `GARAGE_REPAIR_PER_SECOND` hull at `GARAGE_REPAIR_UNIT_COST` (1 cr, half the town's) a point,
  paid as the hull starts into each point (tick-rate safe); out of credits it stops at the last
  point paid for. `Hatch` drops the pilot into the cellar under the house (vent
  grate hatch: rigs cannot follow); `RideLift` runs between the basement and the claim
  (`lift_claim`, `lift_home_from_here`). A home with a lamp lights its plot in `light_at`.
- **Where actions happen.** `check_home(player, &command)` gives the verdict and `HomeQuote`
  the tick applies (`HomeRefusal` says why not). Listed deeds and furniture: at a land office
  (`at_land_office`: the town HQ or a village hall); everything else: the actor on the plot
  (`Plot::at_site`), which is where the pilot stands while the game shows the interior.
- **Events** (netcode codes 240-249): `PlotBought`, `ContractSigned`, `ConstructionStage`,
  `HomeCompleted`, `SiteEventRaised`, `SiteEventAnswered`, `FurniturePlaced`,
  `FurnitureRemoved`, `HomeRushed` and the generic `HomeInteraction`.
- **Scenario helpers** (tests and screenshot scenes): `scenario_home` (a deed, optionally a
  finished home), `scenario_edit_plot`, `scenario_expand`, `grant_furniture`.

`home/*` unit tests cover sites, prices, stages, holds, rushes, events, supply caps and the
decor grid; `world/home_tests.rs` covers deeds at the land office and surveyed spots, claims vs
plots, scheduling in world time and sleeping, every stage and the foundation, shortages,
requests, rushing, supplied blocks, blueprint rules, site crews and talking to the contractor,
decorating and permissions, trophies, furniture buying and crafting, beds and home points, the
garage, the basement, cellar and lift, saves (JSON, postcard, saves from before homes),
replication, hashing and tick-rate safety.

## Wave 4 seams: homes (T24)

- **Sky islands and the Moon** (T20, T22): surveyed plots on a sky land's turf are
  `PlotSetting::SkyIsland` / `Moon` (sky villas and lunar habitats need them); sky towns can list
  their own sites by adding a `SiteKey` arm to `plot::listed_sites_between` and a land office
  by extending `WorldState::at_land_office`.
- **Night** (T25): plots are in `enemy_safe_zone` and a lit home lights its plot in `light_at`;
  `Home::lit` says whether a lamp burns.
- **Story arcs** (T26): `grant_furniture` pays furniture rewards; `GameplayEventKind::HomeCompleted`
  and `ContractSigned` are hooks for home arcs; contractors remember clients
  (`Homes::jobs_with`, `MemoryFact::Traded` at signing).
- **Formats**: `PROTOCOL_VERSION` 12 (the `Homes` section, `PlayerCommand::Home`,
  `NpcState::site`, `BedSpot::Home`, `BuildRefusal::OnPlot`, events 240-249) and save format 10
  (versions 8 and 9 load).

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

## Wave 4 seams: sky continents and sky enemies (T21)

- **Generator V9** (`has_sky_content`) adds continent content only; T20 (islands, sky towns) and
  T22 (the Moon) can share it (the integrator collapses generator bumps). V9 continents carry
  `SkyLand::plan()`; V8 continents stay placeholders.
- **Safe zones in the sky**: `WorldState::sky_safe_zone` is the one place sky towns (T20) and
  lit bases/night rules (T25) add their safety; `hostile_sky` decides where sky enemies roam
  (archipelago and up on V9).
- **Lever doors** (T23 WP-PILOT): `WorldState::work_temple_fitting` is the single entry point
  for levers, keystones, keystone gates and relic chests; today the pick swing calls it, a
  pilot interact action can call it too, and ruins can reuse the temple tiles.
- **Boarders on foot** (T23 personal weapons): sky pirates are ordinary enemies, so a pilot
  fights them today with thrown bombs; T23's personal weapons reach them through
  `damage_entity` with nothing sky-specific to add. (The hand pick does not hit enemies.)
- **Sky folk** (T20 cultures): `people::sky_city::sky_city_population` uses Highland names and
  existing roles standing in for sky trades; swap in T20's sky cultures there.
- **Pilot cold** (T23) and **light** (T25): glowing sky tiles have `SkyTile::glows`; the game's
  `tile_art::sky_glow` gives their colour for halos and the light map.
- **Formats**: `PROTOCOL_VERSION` 12, save format 10 (version 9 files load as is), event codes
  210-219.

## Wave 4 seams: the Moon (T22)

- **Generator V9** is current (V8 + the filled Moon). Other wave 4 tracks that also add a V9
  (sky isles, continents) should merge into one version with every `has_*` switch on, re-pin
  the digest, and keep `has_moon_content` true for it.
- **Story (T26)**: `GameplayEventKind::MoonCoreReached` / `LunarProgress::core_reached` is the
  endgame mystery hook, `WardenDefeated` and `LunarProgress::warden_defeated` the facility's
  end; `MoonLanded` the landing. Quests can target `moon_layout()` places.
- **Pilot gear (T23)**: the pilot breathes on the Moon only through `lunar_air`; a suit with its
  own air can extend `breathe` there. The facility's veil is `pilot_passable`.
- **Homes (T24)**: a lunar habitat plot can stand on `on_moon` ground outside
  `colony_area()`.
- **Night (T25)**: `lunar_safe_zone` answers first on the Moon (only the colony is safe).
- **Sky lock reasons (T20)**: the Moon lines are not locked by world; a lock reason for worlds
  without a sky should cover `PressurizedHull` and `LifeSupport` too.
- **Formats**: `PROTOCOL_VERSION` 12 (see the protocol notes); save format unchanged (an older
  build rejects a V9 save by its generator; every new field defaults).

## Wave 6 seams: art and audio pass

Presentation only: nothing in the simulation changed. No `PROTOCOL_VERSION` or save format
bump, no worldgen change (the V1-V10 golden digests stand). The renderer reads two new derived
facts per pilot in `RenderSnapshot` (`PilotPresentation::low_gravity` from
`environment::environment_at(..).gravity`, and `downed` from the pilot's tomb run member), and
the audio director reads `village::village_at`, `surface::town_span`, `moon::sheltered_at` with
the Moon layout's colony site, and the clock's `DayPhase` to pick and crossfade ambience. No new
gameplay event codes. The Sun Colossus' art was redrawn to its existing
`EnemyKind::contact_half` (64 px wide at its 1.5x draw scale); its contact box is unchanged.

## Wave 6 QA: long sessions, saves across versions and tomb timers

- **Soak** (`world/soak_tests.rs`, `cargo test -p fatherload_simulation --release
  soak_growth_report -- --ignored --nocapture`, `SOAK_DAYS` sets the length, 12 by default):
  four rigs on the short day at 20 Hz (a prospector and a charges miner bot, one rig idle in
  town, one roaming the surface) for 12 in-game days (about 50 s in release). It prints the
  serialized size of every part of the world three levels deep at each midnight and fails if
  any part not tied to exploration (discovered tiles, terrain chunks; live entities are
  checked by count, under 2,000) more than doubles (and grows by more than 8 KB) through the
  second half. Result: nothing grows without bound. Only the discovered tiles and the dug
  chunks grow, with ground explored; the entity store stays at 10-90 entities; service
  transactions level off at about 7 KB; people memories (`MEMORY_EVENTS`), conversations
  (`CONVERSATION_LINES`), the no-repeat window, the host's late actions, the client's event
  dedup set and the input buffers are all capped already. Review follow-up: the roamer used to
  hop into the first cliff each way and shuttle over the same ~300 tiles (x -12..286) all
  session, so the surface (villages, camps, night monsters further out) was barely soaked. It
  now cruises a few tiles above the highest ground just ahead, kept fuelled and whole, on legs
  90 s longer each time; over 12 days it sweeps about 3,100 tiles (x -1,430..1,673), and the
  report fails if it covers under 1,500. Same result with that coverage: only discovered tiles
  and terrain chunks grow, entities stay at 27-82, service transactions flat at about 7 KB.
- **Saves across versions** (game crate, see its README): every save format 1-11 and every
  generator (legacy, V1-V10) loads, plays, saves and reloads exactly; a V10 world saved
  mid-crypt continues identically. No V1-V10 digest changed (no worldgen change in wave 6).
- **Tomb timers and the tick rate**: every tomb and crypt timer is either a pure function of
  `tomb::traps::world_millis(tick, hz)` (side-view traps, crumbling slabs) or a tick deadline
  derived with `TickTiming::ticks_for_millis` (crypt plates, cracked floors and shots,
  revives, the relic-pick timeout), so all of them are tick-rate safe within a session. A
  world's rate is saved with it (`TickTiming`), and a load never changes it: `Runtime::load_world`
  rebuilds the host at the save's rate (and refuses a save at another rate while remote
  clients are connected). So a save loaded mid-run cannot shift a trap; the cross-version
  test saves a 20 Hz world mid-crypt and replays it next to the unsaved one. (The trap
  presentation also uses the world's tick rate; a frame-rate bug in the shell's animation
  clock is fixed in the game crate.)
- **Online at the far ends** (netcode
  `world_pilots_on_the_moon_and_in_the_high_sky_predict_and_converge`): a crewmate on the
  Moon's pad climbs out, walks and jumps in low gravity while another flies under the Moon's
  underside (`y` past -100,000 px), on a 60 ms, 10 ms jitter, 2 % loss link: every client
  converges on the host and every queue stays bounded.

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
| On change | `Loadout(player)`: cargo bay and block hopper, credits, upgrades, weapon rack, kits, rig parts, POI journal, quest log (solo quests, story arcs, reputation, blueprint fragments), salvage, rescue count, deep reward milestone, service transactions, cosmetic unlocks | the owner only |
| On change | economy, contracts, progression (incl. `won_game`), town, world events, infrastructure, markers, RNG, timers (incl. twin-relay vault deadlines), session constants (tick timing, player template, world settings), crew quests, the enemy director (difficulty, rig i-frames, spawn cooldowns, cleared lairs) | everyone |
| On change | `RigBuild(player)`: upgrade levels drawn on the rig (drill, engine, cargo bay, hull, radiator, scanner, bulkheads, drill mount) and the rig's cosmetics | everyone |
| On change | `Loadout(player)`: cargo bay and block hopper, credits, upgrades, kits, rig parts, POI journal, quest log (solo quests, story arcs, reputation, blueprint fragments), salvage, rescue count, deep reward milestone, service transactions, cosmetic unlocks | the owner only |
| On change | economy, contracts, progression (incl. `won_game`), town, world events, infrastructure, markers, RNG, timers (incl. twin-relay vault deadlines), session constants (tick timing, player template, world settings), crew quests, bases (claims, modules, fixtures, homes), the world clock (`SectionKey::Clock`: day length, skipped nights, sleep rule, sleepers), people (character memories, conversations, perks, story marks) | everyone |

Because other players' private state never reaches a client, replica checks compare
`visible_state_hash(viewer)`: private state is hashed only for the viewer and per-player data is
hashed player by player, so host and a correct client authority agree exactly. Terrain
dimensions, settings and the tiles that differ from generation travel only in keyframes
(`redacted_for(viewer)`); the receiver regenerates the rest.
