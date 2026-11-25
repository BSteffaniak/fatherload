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

## Wave 11 space-gen: the seamless space generator (`src/space.rs`, `src/space/*`)

Expansion plan §15 F12: above the Moon the world keeps going, generated per **space sector**
as a pure function of `(world seed, sector)`. Pure data and queries; no rendering, no world
state, nothing saved or replicated (every peer generates the same bytes). Only worlds whose
generator has space get it: `space::has_space(generator)` is V11 (`has_descents`), and
`Space::for_world(settings)` returns `None` for V1-V10. No other generator, golden digest, save
format or protocol changed.

### Units and the sector size

- Positions are **absolute `i64` tiles** (`SpaceTile`), converted exactly to and from
  `WorldPos` (`SpaceTile::pos` / `SpaceTile::of_pos`, `From<WorldPos>`); never `f32`
  absolute coordinates. y grows down, as everywhere.
- A space sector is `SPACE_SECTOR_TILES` = 2^22 = 4,194,304 tiles a side
  (`SPACE_SECTOR_WORLD_SECTORS` = 8,192 `WorldPos` sectors), coordinates `SpaceSector { x:
  i32, y: i32 }` with home at `(0, 0)` (`space::HOME_SECTOR` = `star_chart::HOME_SECTOR`).
  Sized for the Star Drive: a system fits in one 2^21-tile cell (`SYSTEM_MAX_EXTENT` ≈ 1.0 M
  tiles from its star), so cruise (a system in a minute or two, ~10^4 tiles/s at the top of
  its ramp) stays within a sector or two and warp crosses sectors (charts lie 3-48 sectors
  out).
- Placement uses integer trigonometry (`space::cos_sin`, Q30, Taylor to x^13, error < 1e-9)
  so native and WASM place every planet on the same tile; `f64` is used only for distances
  and the gravity/time-scale answers (IEEE `+ - * / sqrt`, deterministic too).

### What a sector holds (`space::generate_sector`)

- **Cells:** 2 x 2 per sector. 1-3 hold a star system (45 % / 40 % / 15 %), each staying
  inside its cell (centre jittered by what its extent leaves); the free cells hold the
  interstellar things. A sector's first system always has a planet (charts point at it).
- **Stars** (`Star`, class = `star_chart::StarClass`, so charts and space share it): radius,
  heat radius (hulls burn inside; no orbit starts there), temperature, light (per mille of
  home's sun), flare period and phase (white dwarfs never flare, red dwarfs often), surface
  gravity. Twin suns are two stars around a barycentre.
- **Bodies** (`SpaceBody`; `BodyKind::{Planet, Moon, CraterRock, HomeWorld, HomeMoon}`): static
  orbits (radius + angle, never moving), placed outward in **non-overlapping annuli** (each
  planet's annulus is its orbit ± its `extent`: moons, rings and a margin). Planets
  1,500-10,000 tiles, moons and crater rocks 400-1,000 (`MIN_LANDABLE_RADIUS` 400; smaller is
  an asteroid). Each carries its **theme** (`Theme`: the twelve §16 themes plus `Barren` for
  small rocks and `Home`/`Moon`), picked by orbit temperature (equilibrium temperature from the
  star's class, radius and distance, scaled so home's orbit is temperate), and its physical
  parameters: `gravity_milli_g`, `day_seconds` (0 = tidally locked), `atmosphere` (vacuum,
  thin, breathable, thick, toxic, spores, scalding), `temperature_c`, `surface_columns` (≈ 2πR,
  a multiple of 32), `rows` (= the radius; the polar track halves columns toward the core),
  `strata_rows` (§16: 800-2,500 for planets), an optional `Ring`, and a `ground_seed` for Wave
  12's ground generator. Names: the system's name + a numeral, moons a letter
  (`StarSystem::body_name`).
- **Asteroid belts** (`AsteroidBelt`): annuli with a density, a favoured `AsteroidOre` (iron,
  nickel, ice, cobalt, platinum, iridium, voidglass; Wave 12 maps them to items) and angular
  segments of ~8,192 tiles. Asteroids are generated **per segment on demand**
  (`asteroids_in_segment`, a pure function, so `AsteroidId { sector, system, belt, segment,
  index }` names one rock forever): 3-399 tiles, an ore and a yield.
- **Comets** (nucleus + tail end pointing straight away from the star), **derelicts**
  (freighter, probe, builders' hulk, wreck field; a loot seed) in a system's outer band and
  adrift in free cells, **stations** (`StationKind` trade post / shipyard / outpost / relay;
  placeholders for Wave 12's hubs) parked 60° ahead of or behind a planet on its orbit.
- **Nebulae:** a nebula star's own (around its system) and interstellar ones on a 4-sector
  grid (`NEBULA_CELL_SECTORS`; 0.8-2.4 sectors in radius, never over home), each with a colour
  and a sensor **fog** (per mille at the centre, `fog * (1 - (d/r)²)`).
- **Black holes** (7 % of sectors with a free cell, never within 3 sectors of home): horizon,
  lensing radius (the screen effect's), time scale at the horizon (`time_scale_at` eases to 1
  at the lensing rim), gravity.
- **Wormholes:** regions of 8 x 8 sectors (`WORMHOLE_REGION_SECTORS`) are paired by an
  **involution** (`partner` flips bits 2-4 of the region coordinates, chosen by a hash of the
  bits it leaves alone, so `partner(partner(r)) == r`, never `r`): mouths lie 24-150 sectors
  apart, 45 % of pairs exist, each mouth sits in its sector's first free cell and knows its
  exit (`WormholeMouth::{exit, exit_sector}`). Both ends are found from either side with no
  search.

### Home (`space/home.rs`)

The home sector holds one system at its centre: a yellow sun (24,000 tiles), the **home
world** (body 0, `HOME_WORLD_BODY`, nominal radius 10,000, 1 g, orbit 260,000 straight "up"
from the sun), the **Moon** (body 1, `MOON_BODY`, radius 640, 36,000 tiles straight up from
home), then the generator's own sister planets outward. These two are space-side stand-ins
(map, scanner, gravity, the crossing); their ground stays the existing world and Moon,
unchanged. `Space::home_departure()` is where a craft leaving the top of the home world's sky
enters space: 256 tiles above the Moon's top, straight up.

### Star charts lead to real systems (`star_chart.rs`, adjusted)

`chart_at_site` keeps its sector ring and depth rule, then asks the generator
(`space::chart_target`) which of that sector's systems with worlds it names: the chart's
`system` is now the generator's `StarSystem::id`, its `class` and `worlds` the generator's star
class and planet count, its `name` the system's name. `space::resolve_chart` /
`Space::chart_system` maps a chart to `SystemRef { sector, system }`; a chart read before this
wave (id hashed from its plinth) resolves to a system of its sector picked by that id. The
`StarChart` format is unchanged (no save bump); two plinths naming the same star add it once.
`StarClass` gained `Default` (yellow).

### Queries (`Space`, `space/query.rs`)

`Space::new(seed)` keeps a bounded LRU cache of generated sectors (`CACHE_SECTORS` 96, behind
a `Mutex` so queries take `&self`; cloning gives an empty cache; the cache is never state).
Every query takes `impl Into<SpaceTile>` (so a `WorldPos` works) and is clamped to
`MAX_QUERY_RADIUS` (4 sectors):

- `sector`, `sectors_near`, `systems_near` (nearest first), `system(SystemRef)`,
  `body(SpaceBodyId)`, `bodies_near` (nearest surface first, with `BodyHit::altitude`);
- `gravity_sources_near(at) -> Vec<GravitySource>` (stars, bodies, black holes whose pull
  reaches: `influence` is where it falls to 2 milli-g, capped at one sector) with
  `GravitySource::pull_g(WorldPos)` (inverse square outside, linear inside, in g, from the
  exact `WorldPos` delta) and `strength()` for blending; `gravity_g(at)` sums them;
- `nearest_station(at, max_sectors)` (ring by ring), `asteroids_near(at, radius)` (only the
  segments the disc can touch; equal to a brute-force search, tested);
- `fog_at`, `nebula_at` (fog + colour), `time_scale_at`, `wormhole_at`;
- `scan(at, range)`: stars (x4 range), black holes (x2), bodies, stations, derelicts, comets
  and wormholes; fog where the scanner is cuts its range by up to 70 %, fog at a target by up
  to 50 % more; each `ScanContact` has a signal strength;
- `home_system`, `home_world`, `moon`, `home_departure`, `chart_system`.

### Report, tests and numbers

- `fatherload --space-report [--seed N] [--sectors R]` (`space::report::space_report`, plus
  timing in the CLI). Default seed, 8 sectors (289): 494 systems (red 158, yellow 129, twin
  74, white 51, nebula 47, blue 35), ~1,740 planets, ~1,980 moons, ~290 crater rocks; every
  §16 theme within 1 sector of home; 310 belts, 329 comets, 388 derelicts, 237 stations, 23
  black holes, 55 nebulae, 2 wormhole pairs.
- **Speed:** a sector generates in ~3 µs on average, ~10 µs at worst (release); 1,089 sectors
  in 3.2 ms. A release-only test asserts < 3 ms per sector.
- `GOLDEN_SPACE_DIGEST` (`0x0c0c_b46c_12ea_7136` since the Wave 11 integration added Halcyon
  Dock to the home system; `0xf303_d220_59a6_d7db` before) pins the 25 sectors within two of home on
  the default seed (`StateHasher` over the serialized sectors). Change it only on purpose.
- Tests (`space/tests.rs`, `space/trig.rs`): same seed gives the same bytes (postcard) and
  different seeds differ, including far sectors at the `i32` extremes; the cache equals the
  generator; only V11 has space; systems well formed (inside their cell, the extent bound,
  orbits where their radius says, star-orbit annuli and moon annuli never overlap, nothing
  inside a star's heat, radii in range, columns ≈ 2πR, stations/comets/derelicts clear of
  bodies, interstellar things clear of systems, systems clear of each other) over 81 sectors x
  4 seeds; every planet theme within 3 sectors of home on 8 seeds; the home layout; every
  chart (360 sites, 3 seeds) resolves to a real system matching its id, class, worlds and
  name, legacy ids too; wormholes pair both ways and the partner map is an involution;
  asteroid queries equal brute force; gravity is ~1 g on the home world, weakens with height
  and fades inside; the scanner sees the Moon and the sun from the departure point; fog and
  time dilation; the cache bound; the report.

### Seams (TODO-SEAM for the integrator)

- **w11-regions:** `SpaceBodyId { sector, system, body }` is the payload of `place::BodyId`'s
  space variant; `BodyId::Home` maps to `HOME_WORLD_BODY` and `BodyId::Moon` to `MOON_BODY`.
  Unify (re-export or `From` both ways).
- **w11-polar / planet:** read `SpaceBody::{radius, rows, surface_columns, strata_rows,
  gravity_milli_g, ring}`; if the polar grid wants a different column rounding, change
  `space::surface_columns` (it moves the golden digest).
- **w11-flight:** gravity via `gravity_sources_near` / `pull_g` (in g; scale to px/s² with
  the home world's gravity), the departure point, `wormhole_at` for transit, `time_scale_at`,
  `fog_at` for the nav HUD, `nearest_station` for docking, `asteroids_near` for mining from
  flight. Keep one `Space` per world (built from the world's settings) for its cache.
- **w11-space-render:** star colours from `StarClass::colour`, nebula colours and fog, comet
  tails, rings, black-hole lensing radii, `Atmosphere::visible` for the rim glow.
- (Wired in the Wave 11 integration: see "Wave 11 integration" below. Space is still never
  state: `Space::shared(seed)` is a process-wide cache per seed.)

## Wave 11 space-render: space and round bodies on screen (`game/src/rendering/space.rs`, `game/src/rendering/space/*`)

The game-side drawing of space and of round bodies (expansion plan F12). Nothing in the
simulation changed: no format, protocol, digest or event code (820-839 stay free). The renderer
draws a plain-data **`SpaceRenderView`** per seat and reads a body's ground through a
**`PolarStrip`** trait, both this track's local interface (`rendering/space/view.rs`), so it
could be built beside the space-gen, polar, flight and regions tracks; the integrator fills them
from the real state (see the seams below).

### What is drawn (`SpaceRenderer::draw`, in this order)

- **Deep sky** (`starfield.rs`): three parallax layers of stars hashed per cell from the sky
  seed (the world seed: every peer sees the same sky), sliding with the craft's exact travel
  (`WorldPos::px_f64` times a tiny factor per layer) and never scaling with the zoom; slow faint
  twinkles; the galaxy's band (denser stars and cloud haze on repeating spines) and a few far
  galaxies. Under the Star Drive the stars stretch into streaks along the heading.
- **Nebulae** (overlapping point-filtered cloud quads in their two colours, young stars inside;
  inside one the view takes on its colour), **wormholes** (dark mouth with the far side's stars,
  four swirling arms, a bright rim), **stars** by `star_chart::StarClass` (glow, spikes square to
  the screen, banded disc, drifting spots, slow prominences; binaries circle each other; nebula
  stars sit in their own cloud), **black-hole discs**, **asteroid belts** (polar cells round the
  star, a few hundred on screen at any zoom: lumpy lit rocks with craters and ore glints near the
  camera, dust specks far off, so a belt reads as a ring from a system away), **round bodies**,
  loose **asteroids**, **comets** (nucleus, coma, a curved dust tail and a straight ion tail
  streaming away from the star) and **derelicts and stations** (placeholder pixel art in a 64-art-
  pixel grid, turned with the structure; Wave 12 replaces it).
- **Black holes**: everything above is drawn into a view-sized target when a hole is on screen,
  then put on screen through the lens shader (a point lens: the sky bends round it into a
  photon ring, the far side of the disc shows above and below, the horizon's shadow is black);
  the disc's near half is drawn again over the shadow. The lens strength follows the screen
  effects setting (`EffectsIntensity::scale(Lensing)`); with it off (or without the shader)
  the shadow and the disc are drawn plainly. Nothing flashes.
- **Round bodies from space** (`bodies.rs`, the planet shader): a lit sphere in pixel-art light
  steps with an ordered dither, theme-tinted land (three-octave value noise on the turned
  sphere) or a gas giant's weather stripes, the terminator facing the nearest star, a cold trace
  of colour on the night side and an atmosphere rim and glow brighter on the day side. The quad
  turns with the view, so a body's own pattern stays its own when the player rolls. `BodyTheme`
  (Barren, Ice, Desert, Jungle, Ocean, Volcanic, Crystal, Fungal, Machine, GasGiant, Dead) is a
  placeholder for the space generator's themes. CPU fallback: a dark disc with its lit half.
- **The ground** (`polar.rs`, below).
- **Cruise lines and warp** (`motion.rs`): thin lines streaming past the craft in cruise; long
  streaks, the stars' own streaks and a blue tunnel at the view's edges in warp. Their length
  and count follow the setting (the motion effects, never fully off: they are how speed reads).
- **The craft**, upright at the view's centre, two points per art pixel at every zoom.

**The camera is locked to the craft**: the world is drawn turned by `-roll` round it
(`Projection`), the craft and anything drawn after (the HUD) stay upright. Every position is a
`WorldPos` projected through an exact `f64` delta from the camera, clamped far out (a star a
light year away never makes a NaN); the fixtures put their systems a billion sectors from home.

### Polar ground rendering (`polar.rs`, `shaders::POLAR_BODY`)

- A body's tiles are painted **flat, in strip coordinates, with the home world's own tile art**
  (`tile_art::paint_tile` on the strip's `Terrain`): a **fine window** of art pixels round the
  craft (96 x 64 tiles, growing to 240 x 240 to cover a zoomed-out view; it moves in 16-tile
  steps) and, when the view reaches past it (zoomed out, or close to the centre where the deep
  rings span a wide angle) or some of its cells are not painted yet, a **coarse map** of one
  texel per tile (the mean colour of each material's tile art; the whole ring of a small world,
  a 2,048 x 1,024 window of a big one).
- **The fine art is repainted a little at a time** (`polar/fine.rs`): tile art costs about
  10 us a tile, so a whole 96 x 80 window is ~60 ms and a zoomed-out 240 x 240 one ~0.5 s. A
  moved or resized window copies every cell it already held (each band row by its own origin
  shift); a changed tile (`PolarGround::mark_tile_dirty`) repaints just its rows (and the
  neighbours' rows, whose edges depend on it) and uploads just those rows; new cells hold a
  marker texel (alpha 1/255) that the shader reads as "use the coarse map here" and are painted
  nearest the craft first, 384 a frame. A strip whose `revision` changes with no tile marked
  repaints every cell that way (the old art stays until then). The coarse map patches marked
  rows too. Scene capture waits until the art is complete.
- The **polar shader** bends both into the ring per pixel: screen point -> the craft's local
  frame (undoing the roll) -> radius and angle round the body's centre, worked out relative to
  the camera (`r - r_c` without cancelling, the camera's column as `f64` on the CPU, per-band
  offsets into each window), so a 31,416-column world is as exact as a small moon. Columns halve
  below each `PolarGeometry::halvings` row (eight at most: bands 0..7 in two `vec4`s, band 8 in
  `uBand8`; R = 5,000 uses all eight before its core). Each band has its own ring
  (`columns_in_band` = `surface_columns >> band`, so an odd ring like the moon's 2,513 rounds
  down) and angles map to band columns in proportion to it (`band_column` /
  `surface_column`), so the wrap seam stays exact in every band. Strata are concentric
  rings, the **core chamber** glows at the centre and lights the rings round it. Day and night
  follow the body's star (`sun_angle`), deep ground keeps a dim ambient (the strata stay
  readable) and the craft's lamp lights round it; the sky's atmosphere thins with altitude
  (none on an airless moon: the stars show). `highp` where the browser has it.
- **Horizon curvature** is exact: R = 400 drops 16 px over 20 tiles at 32 px per tile, R =
  5,000 about 1.3 px (unit tests `a_small_world_shows_its_curve_at_the_horizon`).
- Fallback without the shader: the fine window as **curved quads** (4-tile segments per row,
  lit per quad), the plan's other route; the `space_polar_quads` scene draws it on purpose.
- **Things on the ground** (`GroundThing`: pilot, creature, drop, parked rig) are drawn in the
  local up frame of where they stand (`RingCamera::place` gives the point and the turn), lit like
  the ground there (day side, night side, lamp).
- From space, `bodies::ground_blend` fades the ground in over the disc between 0.9 and 3.5
  points per tile, so the zoom from a round world to the ground under the craft is seamless.
- `SpaceCamera::zoom_for_altitude(altitude, radius, view_height)`: the ground zoom for the
  first rows, then a harmonic blend toward a zoom that keeps the body below the craft (the whole
  body far out); never below the system zoom. A suggestion for the flight track.

### Seams (TODO-SEAM, for the integrator)

- `SpaceRenderView` (`view.rs`): fill per seat from the space generator (`simulation::space`:
  stars with `StarClass`, bodies with `BodyId` -> `BodyView::id`, theme, radius, atmosphere,
  spin; belts, loose asteroids, nebulae, comets, structures, black holes, wormholes) and the
  flight state (`simulation::flight`: craft position and orientation -> `SpaceCamera`, cruise and
  warp -> `FlightMotion`, thrusters -> `CraftView`). `BodyTheme` maps from the generator's
  themes. `sky_seed` is the world seed.
- `PolarStrip` + `PolarGeometry`: the polar track's ring geometry (`radius_tiles`,
  `surface_row`, `surface_columns`, `halvings`, `core_radius_tiles`; `PolarGeometry::standard`
  is the plan's rule as this track read it) and the body's strip `Terrain`, with `tile_at` saying
  where band `k`'s column `c` is stored (default: terrain column `c`) and `revision` changing
  with every tile edit. `GroundView` carries the camera's surface column (`f64`), row and roll
  against the local up, the night light and the things standing there. The star's direction
  comes from `SpaceRenderView::light_toward`. The two cameras must agree (the fixtures'
  `follow_ground` shows how): with `angle = column / surface_columns * TAU + body.spin`, the
  space camera's centre is the body's centre plus `(sin angle, -cos angle) * radius_of_row(row)`
  tiles, and `SpaceCamera::roll = angle + GroundView::roll` (the craft's own orientation; the
  local up only enters through `angle`, so gravity never turns the view).
- Tell the renderer of every tile change on the seat's body: `PolarGround::mark_tile_dirty`
  (reached as `SpaceRenderer::polar`) with the strip's terrain position, from the same frame
  reports that drive the home terrain's `mark_tile_dirty`. Without it a dig still shows, but by
  repainting the whole window over a few frames. It assumes `PolarStrip::tile_at` keeps rows.
- `CraftView::look`: the seat's `RigLook::from_player` (paint job, tiers) so split-screen seats
  tell their craft apart; the pilot and parked rigs on the ground still use default looks
  (`GroundThingKind` could carry a `RigLook` / `PilotLook` the same way).
- `GameRenderer::sync_space` / `render_space` draw one view over the whole screen today (the
  scenes and the browser page). In play: call `sync_space` from `sync` and draw each seat's view
  in its viewport from `render_seat` (the space target and the polar windows are per renderer;
  split-screen seats in different places want one `SpaceRenderer` per seat), then the seat's HUD
  over it as usual (it stays upright). Field screen effects still go through the post-process
  stack; the space renderer's own roll replaces the post-process roll pass while in space.
- Fixture terrains (`fixtures.rs`) stand in for real planets: a barren R = 400 moon (lunar tiles
  in concentric strata, caves, a lava ring and the core, hills on the generator's line, an
  optional shaft) and a 2,048-column window of a jungle R = 5,000 world (the legacy generator).
  The home tile art's edges look at terrain neighbours, so across a halving row they meet a
  differently-spaced row (barely visible; the polar track may want edge hints).

### Scenes, tests and timing

- Screenshot scenes (`scene_capture/space_scenes.rs`, staged views drawn instead of the game):
  `space_deep_starfield`, `space_star_system`, `space_planet_gallery`, `space_approach_moon`
  (R = 400 from 600 tiles), `space_approach_planet` (R = 5,000 from 2,000 tiles),
  `space_moon_hover`, `space_moon_surface` (the curved horizon, things upright),
  `space_moon_night`, `space_moon_rolled` (the craft turned 120 degrees by the player),
  `space_planet_surface`, `space_core_shaft`, `space_polar_quads`, `space_nebula`,
  `space_black_hole`, `space_asteroid_belt`, `space_comet`, `space_station_derelict`,
  `space_wormhole`, `space_warp`, `space_cruise`, `space_moon_flyover` (the ground panning 40
  columns a second under the craft: the incremental repaint under `--frame-timing`).
  `--frame-timing N` times them too.
- The same views run in the app: `fatherload --space-view <name>` on the desktop,
  `?space=<name>` in the browser; `node scripts/perf-web-space.cjs [names]` times the WASM build
  in each (the same flags as `perf-web.cjs`).
- Tests: shader sources well formed for GLSL 330 and ES 100 (no reserved `noise1..4` names:
  Apple's GL compiler refuses a function called `noise3`), projection and roll, ring layout
  (tiles 0.7x-1.4x square, the core), the zoom curve, horizon drops, glow and cloud textures,
  fixtures (the camera stands at its row's radius; the shaft reaches the core), every fixture
  has a scene, band 8 and odd rings (`big_worlds_reach_the_eighth_band_...`), and the fine
  art's incremental repaint (a moved window's art equals a fresh paint texel for texel; a marked
  tile repaints only its rows; an unexplained revision repaints everything over frames).
- Browser timing (WASM release, 1280 x 720, Chrome, machine under a load average of 30-50 from
  the parallel tracks): **GPU: every space scene at 60.0 fps**, 1.3-1.7 ms of frame work (2.7 ms
  in the belt and the quads fallback). Software (SwiftShader, CPU fill): planet gallery 44 fps,
  warp 42, moon surface 30, deep starfield 19, core shaft 14, black hole 13 (the lens pass),
  asteroid belt 10, nebula 7 (big overlapping cloud quads). Desktop release CPU frame work
  (`--frame-timing 120`, loaded machine): medians 1.8-8 ms.
- Review re-timing after the incremental repaint (GPU Chrome, load average ~60): the moon
  flyover (ground panning 40 columns a second) 60.0 fps, mean 1.9 ms, p95 5.3 ms, max 13 ms;
  core shaft, surfaces, black hole, nebula and cruise 60.0 fps at 1.3-1.7 ms mean; belt 2.9 ms,
  quads fallback 3.6 ms mean (p95 12.9). Before it, a window step at the ground zoom cost a
  ~60 ms repaint (desktop release, measured), i.e. a visible hitch every 16 tiles of flight and
  on every dig. Software rendering was not re-timed.

## Replication of entities

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
- **Content** (`story/arcs.rs`, `story/arcs/*.rs`): 92 arcs in batches (first 8; frontier 101-110;
  village 201-210; sky 301-308; mystery 401-408, a chain from "Blue Lights Below" to "The Heart
  of the Moon" that ties the deep's lights and hum to the Moon; home 501-506; night 601-607;
  pilot 701-707; crew 801-803; Wave 6: tombs, crypts and ruins 901-908, the night market
  1001-1006, far places 1101-1103, 1201-1204, 1301-1304, see "Wave 6 content" below).
  `ARC_TEMPLATES` chains `arcs::BATCHES`; each arc runs on the
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

## Wave 6 content (W6 content track: stories, people, sky towns, the colony, homes)

Data and runtime content only: no generated tile changes (the V1-V10 golden digests are
unchanged); every new enum variant is appended, so old saves load.

- **Story places** (`quest::StoryPlace`, appended; bound in `quest::binding::story_place`,
  each with fallbacks so every arc binds on V5-V8 too): `Tomb` (the nearest side-view tomb
  site on the ground or under it, `tomb_sites::sites_touching`, never the sky's; else any site,
  a sealed vault, a cavern), `Crypt` (a top-down crypt first), `Ruin` (the nearest
  `PilotSpaceKind::Ruin`, V9; else a derelict, a cavern), `NightMarket` (the trading post of
  the nearest other surface village, a contact settlement; else the giver's own; else the
  town), `Homestead` (the nearest listed plot, `plot::listed_sites_between`; else the giver's
  home) and `SkyTown` (the nearest sky town of the lowest town row whose id names no sky
  city, a contact settlement; else an island, the ridge; gated by
  `milestone::sky_town_capable`). New `BoundTarget`s (`Tomb`, `Ruin`, `Market`, `Plot`,
  `SkyTown`) name them in `describe_place`. The Moon's place now names the colony's
  settlement on a filled Moon, so `Speak` beats there find a colonist.
- **Objectives** (`quest::Objective`, appended): `NightHunt { count }` (WP-NIGHT's
  `NightHook::Hunt`: the owner's or crew's kills of nocturnal kinds, read from `EnemyKilled`
  events in `EventFacts`) and `Dawn { target }` (`NightHook::SurviveTheNight`: present at the
  target, out of bed, on a night tick, and still there within `DAWN_GRACE_MILLIS` after the
  night ends). `Gather` hooks are `Deliver` of night materials.
- **Arc effects** (`people::story::ArcEffect`, appended): `Furniture(piece)` (the
  `grant_furniture` seam) and `UnsealTomb` (every target that is a tomb door,
  `tomb_sites::site_with_door`, opens through `unseal_tomb_site`; its `TombSeal` event is
  raised). `ArcKind::Tomb` ("Tombs") must bind a tomb or crypt (lint).
- **Arcs** (`story/arcs/{tombs,night_market,far_places}.rs`, ids 901-908, 1001-1006,
  1101-1103, 1201-1204, 1301-1304): 25 arcs on those places, with furniture rewards and
  night hooks. Triggers deliberately use wants whose existing arcs need no slot, so the old
  slot-bound and weak-trigger arcs keep their givers (`every_arc_is_offered_by_someone`).
- **People**: `Culture::Lunar` (appended, not in `Culture::ALL`; `Culture::EVERY` has all
  nine) with its name table; lunar roles `Role::{Hydroponist, DomeTech, Selenographer,
  Quartermaster}` (`Role::LUNAR`, `LUNAR_POOL`; not in `ALL` or `SKY_POOL`, so old worlds'
  people are unchanged). `people::colony`: the colony settlement `LUNAR_COLONY`
  (`colony_settlement_id`, six colonists in fixed trades, `Settlement::colony` with the dome,
  the hab module they sleep in, a hydroponics bench and the stores) joins
  `routine::settlement` and the people pass (`settlements_near_actors`). Local colour
  (`dialogue::local`, `Topic::Local`): a line about home per culture on later visits (its own
  roll, `LOCAL_PERCENT`). Culture furniture (`offers::signature_furniture`,
  `sells_signature`): makers and traders of every culture append a `PersonDeal::Furniture`
  row with their people's piece (hydroponists also sell grow racks).
- **Sky town ids** (fix): `continent::sky_city` only decodes cells inside the streamed world,
  so a sky town's id no longer names a far-off sky city and sky towns get their own folk.
  Ids of sky towns in archipelago block column 0 still equal real sky cities' ids (the
  cities keep them, as in old saves): those few towns above the spawn have no folk, and story
  binding skips them (`world::wave6_content_tests::
  sky_town_ids_only_collide_with_cities_above_the_spawn`). Disjoint id spaces need an id
  change and a save migration (left to the integrator).
- **Homes**: 17 `Furniture` pieces (culture pieces, `SKY_CATALOGUE`, `TOMB_TROPHIES`, story
  rewards; `CACHE_FINDS` unchanged so old caches find the same antiques), four `Blueprint`s
  (`CloudCottage` and `Aerie` for sky islands, `CraterDome` for the Moon, `Longhouse`
  anywhere; `BlueprintSite::Moon` plans count as lunar habitats for `in_lunar_habitat`). A
  cloud hammock sleeps (`Home::has_bed` reads the service), a relic plinth displays.
  Guardians give a `GuardianMask` beside the trophy case; the first finished run a
  `RelicPlinth`.
- **Sky land office**: `SiteKey::SkyIsle(town, index)` sites (`plot::sky_sites_of`: one per
  small isle beside a sky town's town isle with a level, open stretch of turf, never a
  crumbling isle; medium or small; pure in the settings) are sold at the town's hall
  (`WorldState::at_sky_land_office`, `land_office_listing`, `land_office_catalogue` with
  the sky pieces) and only there (`BuyPlot` of a `SkyIsle` key elsewhere is
  `NotAtLandOffice`); surveyed sky spots keep clear of them; sky plots refuse a basement; sky
  towns' builders (and the colony's on the Moon) bid for sky and Moon plots.
- **Formats**: `PROTOCOL_VERSION` 14 and save format 12 (both for the appended variants;
  version 8-11 saves load).

Tests: `people/story/arc_tests.rs` (the lints over all 92 arcs), `world/arc_tests.rs`
(`every_wave6_arc_binds_to_its_real_places_and_plays_through_on_a_v10_world`: every new arc
binds the real tomb, crypt, ruin, night market, homestead, sky town and colony contact on a
V10 world and a bot plays it to its climax; `a_tomb_story_ending_breaks_the_seal_and_pays_furniture`),
`world/quests.rs` `night_hook_tests`, `world/wave6_content_tests.rs` (the sky land office,
sky town folk, colonists talking and trading, culture furniture), `people/colony.rs`,
`people/dialogue/local.rs`, `home/decor.rs` `wave6_tests` (furniture and blueprint tables).

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
- **Saves across versions** (game crate, see its README): every save format 1-12 and every
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

## Wave 6 integration

The four wave 6 tracks (collected fixes, art and audio, QA, content) sit on one linear branch
in that order.

- **Formats**: one `PROTOCOL_VERSION` bump for the wave (13 to 14; its doc lists the fixes'
  deal-by-id village purchase and the content's new variants) and one save bump (11 to 12,
  content only; formats 8 to 11 load as is). The QA round-trip test covers format 11 and 12
  files separately.
- **Worldgen**: no change; the V1-V10 golden digests stand. The only worldgen-side edit is the
  content's range check in `continent::sky_city`, which answers `None` only outside the
  streamed world.
- **Event codes**: no track added gameplay event codes.
- **Seams**: `milestone` holds both the fixes' `island_hopper` (sky calibration) and the
  content's `sky_town_capable`. The art pass's colossus fits the unchanged
  `EnemyKind::contact_half`; the fixes' rock mimic retune touches only its bite. Village deals
  are bought by id (fixes); content's furniture rows are person trades (`PersonDeal::Furniture`,
  appended, still by row). The tomb balance (fixes) is unchanged by QA, whose timer audit made
  no simulation change.

## Wave 7 positions: world positions without limits (F7, `src/position.rs`, `src/world/frame.rs`)

Gameplay positions were plain `f32` world pixels, which resolve only 1/8 px at 2^20 px. F7
splits a position in two:

- **`WorldPos`** (`position.rs`): an exact absolute position, an `i64` sector per axis
  (`SECTOR_TILES` = 512 tiles, `SECTOR_PX` = 2^14 px a side) plus an `f32` pixel offset inside
  it. It reaches ±2^63 sectors (far past 10^12 tiles). Helpers: `from_legacy_px` (any finite
  `f32` splits losslessly), `from_tile`, `tile_center`, `offset`, `delta` / `delta_f64`
  (exact sector difference first), `distance`, `tile` (`i64` tiles), `tile_position` (`i32`,
  `None` off the grid), `px_f64`, `to_legacy_px`.
- **`Frame`**: the sector the world's **local pixel space** is centred on, held by the terrain
  (`Terrain::frame`, saved in its snapshot, hashed only when it is not the fixed origin
  frame). Every stored gameplay `f32` position (rigs, pilots and their ropes, creatures,
  shots, drops, charges, boulders, gas, people's walking legs, tomb and crypt actors, lost
  cargo, the tether hook, parked crews, the player template) is a pixel offset from it, so
  every system runs the same `f32` code as before and collides in that local frame. Tile
  coordinates stay absolute (`i32` `TilePosition`, ±2.1 x 10^9 columns): only conversions go
  through the frame.
- A **floating** frame re-centres on its anchor (the first player's actor) as the last step of
  `simulate_tick` (`WorldState::recentre_frame`) once the anchor is more than one sector
  (`RECENTRE_SECTORS`) off on an axis; only that axis moves, by whole sectors, so the anchor's
  local position always stays inside ±2^14 px where `f32` holds the 1/1024 px simulation grid
  exactly. `WorldState::move_frame` shifts every local position (`Reframe`), nothing moves in
  absolute terms. A **fixed** frame never moves: it stays at sector (0, 0), so its local pixels
  are the old world pixels bit for bit.

### The choice and the numbers

`position_cost_report` (`cargo test -p fatherload_simulation --release position_cost_report --
--ignored --nocapture`; a V5 arena with 4,000 drops spawned, 600 ticks at 60 Hz, best of three
alternating runs on a loaded machine; the drops merge into about 32 stacks within the run, so
the tick rows time a rig and a few dozen entities, while the frame move times all 4,000; the
review re-run measured 710 / 711 µs, 0.48 ms and 1.43 / 1.10 / 11.6 ns):

| Measure | Result |
|---|---|
| Busy tick, fixed frame | 714 µs |
| Busy tick, floating frame (no move) | 721 µs (same within noise: the frame costs nothing per tick) |
| Moving the frame of the world with 4,000 entities | 0.48 ms (once per 512 tiles of travel) |
| Synthetic body step (integrate, quantize, 3 tile lookups), `f32` frame-local | 1.4 ns |
| ... `i64` fixed-point sub-pixels, converted for the `f32` physics | 1.1 ns |
| ... a `WorldPos` (sector + `f32`) per body, normalized each move | 12-128 ns |

Per-body sectors cost 10-100x per step and would make every distance between two bodies a
sector subtraction; `i64` fixed point is as cheap per step but every `f32` physics site would
need a conversion in and out (thousands of sites, while four other tracks write new code
against the `f32` API). The shared floating frame costs nothing per tick, keeps every system's
code (and every V1-V10 result) unchanged, and pays a sub-millisecond shift once per sector of
travel. Chosen: **`i64` sector + `f32` local offset, with the sector shared per frame**. F11's
"many places at once" generalizes it to one frame per simulated region (the `Reframe` machinery
is what moves state between them).

### Generator policy and seams

- `GeneratorVersion::floating_frame()` (exhaustive): V1-V10 answer `false` (fixed frame; their
  positions, saves, hashes and replays are unchanged). **V11 must answer `true`**; a new world
  then starts with `Frame::FLOATING` (`Frame::for_generator`).
- `GeneratorVersion::column_limit()` (exhaustive): V5-V10 keep `STREAMED_COLUMN_LIMIT`
  (131,072; their village, sky town and continent placement is sized to it). V11 may raise it
  to at least 10^9; positions are exact out to `i32::MAX` columns. `Terrain::column_limit()`
  reads it; `Terrain::set_column_limit_for_tests` raises it for far test and tool worlds (saved
  in the snapshot as `column_limit`).
- `WorldState::{frame, world_pos, local_pos, actor_world_pos}`; `set_floating_frame(bool)` and
  `teleport_rig_to(player, WorldPos)` for tests, tools and new-game options;
  `relocate_frame(Frame)` (doc-hidden) re-expresses a world in another frame (the screenshot
  `--frame-shift` check).
- **New state with positions** implements `position::Reframe` and is moved in
  `WorldState::reframe_positions`, which destructures every `WorldState` field (the compiler
  asks); per-kind entity state is matched exhaustively in `EntityState::reframe`.

### Porting code (for the other wave 7 tracks and later waves)

| Old (world pixels) | New (frame-relative) |
|---|---|
| `(x / TILE_SIZE).floor() as i32` | `frame.col(x)` / `frame.row(y)`; `.ceil()`: `col_ceil` / `row_ceil` |
| `TilePosition { x: (x / T).floor() .., y: .. }` | `frame.tile_at(x, y)` |
| `tile.x as f32 * TILE_SIZE` | `frame.rel_col(tile.x) * TILE_SIZE` or `frame.col_px(tile.x)` |
| `(tile.x as f32 + 0.5) * TILE_SIZE, ..` | `frame.tile_center(tile)` / `frame.centre_of(x, y)` |
| `player.tile_position(TILE_SIZE)` | `player.tile_in(frame)` |
| `environment_at(settings, x, y)` | `environment_in(settings, frame, x, y)` (also `moon::vacuum_in`, `moon::sheltered_in`, `village::building_in`) |
| a pure function of absolute place given `(x, y)` | `frame.legacy_px(x, y)` (exact at the origin, tile-scale far out) |
| a constant authored in world pixels (`12.0 * TILE_SIZE`, a spawn point) | `frame.from_legacy_px(x, y)` |
| altitude `(-y / TILE_SIZE)` | `(-frame.legacy_y(y) / TILE_SIZE)` |

`frame` is `self.frame()` in `WorldState` methods (bind `let frame = self.frame();` before a
mutable borrow), `terrain.frame()` where a `&Terrain` is at hand, `world.pilot.frame()` through
`PilotTerrain::frame` (default: the fixed frame). Every helper is the exact expression it
replaces in the origin frame, which is why V1-V10 results are bit for bit unchanged.
`Player::tile_position`, `world::tile_center_px` and `enemy::tile_centre` keep their old
meaning (the origin frame) for tests and fixed-frame scenes. The tomb trap geometry
(`tomb::traps::{blade_pivot, blade_tip, crusher_block, dart_position, sand_surface,
fog_centre}`), `WorldState::outside_spot`, `world::drops::blast_spray_velocity` and
`authoritative_mine_target` take the frame as their first argument. Quest and arc
prerequisites read the rig through the frame (`Prerequisite::met_in`, `OfferContext::frame`;
`Prerequisite::met` is the origin-frame reading for tests).

Presentation (game crate, `view_frame`): besides `frame()`, patterns anchored to the world grid
use `pattern_start_x` / `pattern_phase_x/y` / `pattern_anchor_x` (cell index and phase of the
frame origin, so cell-hashed patterns keep their look across a frame move) and animation phases
seeded by place use `phase_x/y` (the old world pixel). Snapping to art or physical pixels breaks
ties upward on both sides of zero (`pixel_scale::round_half_up`), so the grid never depends on
the side of the frame origin a sprite is on.

### Netcode and saves

- `LiveEntities::frame` (sent every tick) carries the frame; `apply_live_entities` moves the
  replica into it (`move_frame`) **before** anything else of the frame is applied (netcode's
  `apply_replication` now applies the live section first), so records measured in the new
  frame land cleanly. Every peer shifts its own copy with the same `f32` operations, so
  replicas and hashes agree; entity revisions do not change.
- `Pose::origin` / `Relevance::origin` (netcode): a pose is the frame-local position in tiles
  plus the frame origin tile (`world::frame_origin`); distances, smoothing, interpolation and
  relevance go through exact origin deltas (`Pose::delta`, `rebased`,
  `Relevance::contains_pose`; chunk boxes relative to the relevance origin), so they hold
  however far from the origin and from each other players are. `PROTOCOL_VERSION` 15.
- Save format 13 (game crate): `terrain.frame` and `terrain.column_limit`. Format 8-12 files
  load as is, as the fixed frame at the origin: their `f32` pixels are the same positions bit
  for bit (`WorldPos::from_legacy_px` of each is exact).

### Tests

- `position::tests`: lossless legacy round trips (to -9 x 10^15 px), the origin-frame helpers
  equal the old expressions, sub-pixel steps and nearby deltas stay exact 10^6, 10^9 and
  ±10^12 tiles out, recentring moves whole sectors on the drifted axis only, tiles past the
  `i32` grid saturate (outside every world).
- `world::frame_tests`: a rig hovering, flying into a wall and landing 10^6 px, 10^9 px and
  10^9 tiles out matches the near flight **bit for bit** (every tick's position, velocity, hull
  and fuel) at 30, 60 and 120 Hz; far runs are deterministic (state hash); a 1,200-tile drive
  in a floating frame hops sectors with no jump in the rig's absolute position, a dropped item
  keeps its place, and the drive ends within 0.5 px of the fixed-frame drive; every old
  generator keeps the fixed frame; a replica follows a frame move.
- `netcode tests::world_far`: three clients over 150 ms / 3 % loss with duplication and
  reordering play 10^9 tiles out while the frame hops; every client authority hash matches the
  host at every compared frame.
- Game: a format 12 save with a rig at 4.2 million px loads exactly, and a floating frame
  round-trips in format 13; `frame_tests::depth_prerequisites_read_the_rig_through_the_frame`;
  `--frame-shift 1,-1` renders all 444 screenshot scenes one sector off their staged frame
  (review run): 381 are pixel-identical and the rest differ in under 1 % of pixels (crypt scenes
  up to 0.8 %: one-pixel mortar seams from GPU rounding at larger coordinates, and enemies a
  pixel apart after hundreds of ticks of `f32` physics at a different magnitude); everything
  else is under 0.11 %. The first review pass found and fixed real frame bugs this way (shops'
  doors, home and plaza lights, vent columns, village doors, pattern phases).
- V1-V10 golden digests and every existing determinism, replay and save hash are unchanged
  (the fixed frame at the origin is not hashed and all helpers reduce to the old expressions).

### Known limits

- One frame per world, anchored on the first player: a co-op partner far from the anchor keeps
  `f32` precision relative to it (the pre-F7 behaviour, no worse); per-region frames are F11.
- Tile coordinates are `i32`: positions resolve at any distance (`WorldPos`), but terrain ends at
  ±2.1 x 10^9 columns (beyond, tiles saturate and read as walls). 10^12 tiles is covered by the
  position types; gameplay tests run to 10^9 tiles.
- Rendering reads the same frame (see the game crate's `view_frame`); presentation-only
  heuristics that read the absolute place (sky parallax, the town heading) go through
  `legacy_px`, accurate to a tile or so 10^9 tiles out. The dev online bots still assume the
  town is near.
- No gameplay event codes were added (350-359 stay free).
- A replica that misses the live sections of two frame moves on one axis (or the host
  teleporting its anchor several sectors) shifts once by the total where the host shifted in
  steps: entities on the far side of the shift can differ by an `f32` ulp until their next
  record (a hash mismatch heals through the usual keyframe). A frame-move log would close it.
- V5-V10 world generation is only meant to reach `STREAMED_COLUMN_LIMIT`: at 10^9 columns its
  cave field is wrong (the `far_billion_tiles` scene shows alternate tunnel columns with a cave
  back wall). Gameplay tests there carve their own arenas; V11's generator must be checked out
  to its `column_limit`.
- Balance setups (`balance::{climb, building, combat}`) and the dev online bots still place
  rigs in world pixels; they are correct on fixed frames and on a floating world before its
  first frame move only.

## Wave 7 strata: the Descent framework and generator V11 (`src/descent.rs`, `src/descent/registry.rs`, `src/worldgen/descent.rs`, `src/worldgen/descent/landmarks.rs`, expansion plan F8)

### The stratum registry (`crate::descent`)

- **`Descent`** (8, top to bottom): name, numeral, target rows of a mean world
  (`target_rows`, back to back to 13,000), target hours (0 to 90), story beat, its strata
  (`strata`, `first_stratum`, `last_stratum`) and its gate (`gate`, held by the last stratum).
- **`Stratum`** (41, `#[repr(u8)]`, `Stratum::ALL` order = registry order) and
  **`StratumDef`** in `registry::STRATA`, read through `Stratum::def` (by reference: indexing
  the `const` table by value copies all of it in debug builds). Fields: name, signature rule,
  `share_per_mille` of its Descent (each Descent sums to 1000), `host_band` (the classic
  `Band` whose rules a placeholder borrows), `soft` / `hard` rock (`TileKind`), `palette`
  (`StratumPalette` of `Rgb`: rock, hard, back wall, accent), `ores` (`OreCurve`: mineral, peak
  and spread per mille through the stratum, weight; `ore_weights(along)`), `layer_resource`
  (a name plus the `StrategicResourceKind` standing in), `creatures` (`(EnemyKind, weight)`,
  existing underground kinds), `hazards` (`(HazardKind, weight)`, `hazard_weights()`), `field`
  (`FieldProfile`), `music` (`MusicCue`, one per Descent), `ambience` (`AmbienceCue`),
  `landmarks` (`LandmarkKind`), `variants` (2-3 names; a world picks one with
  `worldgen::descent::variant`) and `gate` (`Option<GateDef>`, only on a Descent's last
  stratum).
- Descents I and II are the twelve classic bands (`Stratum::of_band`, `is_classic`): host band
  = own band, rock = the band's materials, creatures = `enemy::band_roster`; their registry
  ores and hazards describe the classic curves (the generator keeps the classic rules for
  them). Every older world names its layers through `Stratum::of_band`.
- **`FieldProfile`** (plain integers; `FieldProfile::NEUTRAL` is the `Default`, `is_neutral`):
  `gravity_milli` (x, y in thousandths of standard gravity, `(0, 1000)` standard),
  `time_scale_permille`, `friction_permille`, `drag_permille`, `light_permille`,
  `mirror_controls` and `screen: &[ScreenEffectLevel]` (`ScreenEffect` id plus intensity per
  mille, in drawing order). Placeholder values follow §14 (Glow Sea light 250, Brine Glaciers
  friction 300, Underground Ocean drag 1800, Time Sludge time 800, Pixel Rot pixelate and
  palette reduce, Prism Desert chromatic aberration, Singularity Fields lensing, the Sunforge
  exposure and bloom, Pearl Abyss desaturate and vignette, ...). Read by the field effects
  since the Wave 7 integration (`descent::fields`, see "Wave 7 integration" below).
- **Gates** (`GateDef`: `DescentBoss`, `requires: &[GateRequirement]`, `rewards:
  &[DescentKey]`): every key a gate asks for is a reward of an earlier gate, every
  `DescentKey` is a reward exactly once, and the Seal Gate asks `EveryEarlierKey`.
  `gate_passable(gate, &GateProgress)` checks a crew's drill tier, keys and beaten bosses (bit
  sets by declaration order).

### Generator V11 (`GeneratorVersion::V11`, `worldgen::descent`)

- `GeneratorVersion::V11` is V10 (surface, sky with V9's bands, Moon, tombs and every other
  `has_*` switch) plus `has_descents()`. `CURRENT` stays **V10**;
  `GeneratorVersion::DEEP_DESCENT` names the preview. `WorldSize::descent_height_tiles`: 13,014
  rows for Standard (13,000 rows of strata below the ground line, plus the sky rows and the
  bedrock floor), 13,614 for Large.
- **Layout** (`layout(settings) -> DescentLayout { tops: [i32; 41], floor }`, a `const fn` of
  the seed): each Descent draws its thickness (0.9-1.1 x its target share of the rows), each
  stratum its share of the Descent (0.7-1.3 x the registry share, at least
  `MIN_STRATUM_ROWS` = 96), leftovers to the Descent's last stratum. `rows`, `descent_rows`,
  `stratum_of_row` and `along` (per mille through the stratum) answer from the layout rows;
  `stratum_at` / `stratum_in` add the ragged boundaries: `boundary_shape(index)` is (4, 6)
  under the topsoil, (12, 14) inside a Descent and (18, 20) between Descents (wave amplitude,
  half zone), with V4's lens model under salts of their own (V4-V10 are untouched).
  `descent_at`, `stratum_solid_top_row`, `stratum_shallowest_row`, `boundary_column_depth`,
  `boundary_depth_range`.
- **Classic rules through host bands**: `strata::band_at` of a V11 world answers the stratum's
  host band, so every band-keyed rule (heat, protection, POI kinds, cave biomes, lairs, drops,
  quests) works at every depth; `strata::nominal_row` / `nominal_depth` map each stratum's rows
  onto its host band's classic nominal rows (depth curves, hazard gates and heat read a
  placeholder stratum as the band it borrows). `scaled_row` inverts the mapping over Descents
  I and II only, where it is monotone (nominal 220 = the end of Descent II), in closed form
  (exactly the row a binary search of `nominal_row` finds; a test checks every value); `scaled_span`
  stretches like Descents I and II; `band_layout_rows`, `band_solid_top_row` and
  `boundary_bases` answer the classic band's own stratum in Descents I and II. Nominal rows
  are therefore **not monotone** over a whole V11 world: never binary-search `nominal_row`
  past Descent II.
- **Registry-driven generation below Descent II** (`streamed::base::Ctx`): `Ctx::descent`
  caches the layout; `Ctx::stratum`, `Ctx::nominal_row`; `rock_kind` uses the stratum's
  `soft` / `hard`; veins, pods and motherlodes pick minerals with `Ctx::mineral_weights` /
  `mineral_from_hash` (the stratum's `ore_weights(along)`, minus minerals too hard for the
  host band's drill), hazard clusters with `Ctx::hazard_weights` (the registry's). POI tiers:
  the classic bands' tiers plus one set per deep stratum (keys `4096 + stratum * 64 +
  ordinal`, dealing the host band's kinds). Creatures: `Terrain::creature_roster` (the
  stratum's table) replaces `band_roster` in the director, and `tier_at` uses
  `Stratum::enemy_tier` (11 below Descent II). `Terrain::stratum_at` and `layer_name` name
  the layer anywhere.
- **The Hollow World** (`hollow_rows`, `hollow_open`, `fixed_kind`): the Inner Sky is open air
  from 40 rows under its top to 48 rows above its bottom (both waving 14 rows), 300+ rows tall
  in every column, with floating stones (noise blobs) and stalactites. It is part of the base
  layer, so caves, hazards and ore only touch the stones.
- **Landmarks** (`landmarks`): `landmark_generator(kind)` resolves every `LandmarkKind`
  (`DescentStamp` for outposts, gate chambers and anomaly rooms; `HollowCavern`; the existing
  POI, cave-ecosystem, village and tomb generators). Stamped in `assemble` after the pilot
  spaces, before the tombs: one **outpost** per Descent (`outpost`: a 26x9 plated hall with
  glass skylights and doors both sides, 40-160 columns either side of the spawn column, in
  the Descent's `Outpost` stratum; the Hollow World's stands on the cavern's ground), a **gate
  chamber** per Descent (`gate_chamber`: a 44x12 hall of ruin brick, pillars and glyphs just
  above the Descent's last transition zone, or the floor; all drillable, so a gate never
  blocks the way down) and **anomaly rooms** (`ANOMALY_REGION` 80x64; 45 % of the regions
  whose centre row is in an `AnomalyRoom` stratum hold a 14x8 crystal-lined room with a
  glyph). `landmarks_in(settings, rect)` lists them, `landmark_tile` paints them.
- **Golden digest**: `worldgen::descent::tests::GOLDEN_V11_DIGEST` pins the town, the middle of
  every Descent, every outpost and gate chamber, the cavern, an anomaly room and the floor. The
  V1-V10 digests are unchanged. Generating V11 chunks costs about a quarter more than V10's
  at the same rows (release, 16,000 chunks of rows 160-800 over 400 columns: 1.57 ms against
  1.25 ms per chunk); deeper V11 chunks cost about the same.

### Streaming budget

- **Chunk caching**: unchanged (`streamed::CHUNKS`, 4,096 chunks; loaded chunks per rig within
  `STREAM_LOAD_RADIUS_CHUNKS`, with hysteresis). Depth adds no cache pressure: a rig holds at
  most about 81 chunks loaded anywhere.
- **Save compression** (`terrain::PackedChunks`, `TerrainSnapshot::packed`): a streamed
  world's edits are one byte stream: chunks as zigzag steps, each chunk's diffs as runs of
  equal tiles `(gap, length, code)` (code 0 repeats the previous run's tile) in row- or
  column-major order, whichever is shorter (a shaft dug down a chunk is one run). Base64 text
  in JSON, raw bytes in postcard. Snapshots write `packed` and leave the old `chunks` list
  empty; `from_snapshot` still reads `chunks` (format 12 saves and older).
- **WASM memory** (`terrain::WASM_TERRAIN_BUDGET_BYTES` = 256 MiB, measured by
  `tests/deep_descent_memory.rs` with a counting allocator): a V11 world with a shaft to the
  floor and a 240-wide gallery in every Descent, the rig ridden down the whole shaft (every
  chunk on the way generated): **19.8 MiB** of heap settled and at peak (native 64-bit
  pointers: an upper bound for the 32-bit browser heap), at most 81 chunks loaded, 913 edited
  chunks, about 12 KiB of terrain save (JSON). The emscripten build grows its memory up to 2 GiB
  (`ALLOW_MEMORY_GROWTH`). **In the browser** (release WASM build, headless Chrome,
  `scripts/perf-web.cjs`, which now reports the WASM memory size, a high-water mark since
  linear memory never shrinks): the `descent_explored` scene (a shaft to the floor and a
  240-wide gallery in every Descent, the rig at the bottom), `descent_8_heart` and
  `descent_5_hollow_world` all end at **156.3 MiB**, the same as the classic `night_town` and
  `sky_city` scenes: startup (textures, audio, caches) sets the high-water mark and a deep V11
  world adds nothing measurable. Frame work stays about 1.1-1.2 ms. The browser run loads the
  saved world at the bottom; riding the whole shaft down is measured natively only.

### Seams for the other Wave 7 tracks and later waves

- **Field effects (F9)**: read `terrain.stratum_at(tile).def().field` (or
  `worldgen::descent::stratum_at`); every value is a plain integer, neutral by default. Blend at
  boundaries with `DescentLayout::along` or across the ragged `stratum_at`.
- **Logistics and progression (F10)**: outposts and gate chambers are found with
  `landmarks::outpost` / `gate_chamber` / `landmarks_in` (bounds, Descent, stratum); gates are
  data (`Descent::gate`) with `gate_passable` (nothing enforces them yet). Target rows and
  hours per Descent for the descent harness: `Descent::target_rows` / `target_hours`.
  `balance::BalanceConfig::generator` (`--balance-report --generator v11`) runs the economy
  bots on V11.
- **World positions (F7)**: V11 rows reach 13,014 (416,448 world pixels; `f32` still resolves a
  fraction of a pixel there).
- **Wave 8+ content**: replace a stratum's placeholder values in `registry.rs` (rock, ores,
  hazards, creatures, field, cues, variants) without touching the layout; new landmark kinds
  append to `LandmarkKind` and get a `landmark_generator` arm. A stratum stops borrowing its
  host band's rules wherever a rule reads the stratum instead of `band_at`.
- **Formats**: one `PROTOCOL_VERSION` bump (14 to 15: `GeneratorVersion::V11` and the packed
  keyframe chunks) and one save bump (12 to 13; formats 8 to 12 load as is). No new gameplay
  event codes (the "entering" banner is presentation, from the rig's row).

## Wave 7 effects (F9 field effects and craft rotation)

The Part III foundations' effects track: deterministic per-region fields, player-controlled
craft rotation, and (in the game crate) the post-process pipeline and rolling camera.

- **Fields** (`src/field.rs`, `src/field/{presets,trig}.rs`, `src/world/fields.rs`): one
  query, `WorldState::fields_at(x, y)` (or `field::sample(zones, settings, x, y)` where the
  world is split-borrowed), answers a `FieldSample`: gravity vector (standard gravities, any
  direction; it multiplies the F6 altitude environment), time scale, ground friction, air drag,
  control mirroring, light, sound occlusion, free rotation and the 13 screen effect levels with
  grade and fog colours. `FieldSample::NEUTRAL` everywhere today: neutral fields take every
  classic path bit for bit (no V1-V10 digest or existing hash changed).
- **Seam for the strata track (wired in the Wave 7 integration, `descent::fields`)**: `field::FieldProfile` is the plain-data
  per-stratum profile (gravity, time, friction, drag, light, occlusion, mirroring, free
  rotation, `ScreenEffects { levels: [u8; 13], grade_rgb, fog_rgb }`). Register the stratum
  registry's lookup, a pure `fn(&WorldSettings, x, y) -> Option<FieldProfile>`, in
  `field::STRATUM_FIELD_SOURCES` (it holds `descent::fields::stratum_field_source`; the source
  signature gained the frame: `fn(&WorldSettings, Frame, x, y)`, local pixels). Blend across stratum boundaries inside the
  source with `field::boundary_blend(row, boundary_row, half_width)` and
  `FieldProfile::lerp`. `ScreenEffect` and `FieldPreset` append only.
- **Authored zones**: `FieldZone` (tile rectangle, feathered edge, profile, optional preset)
  in `FieldZones`, the new `WorldState::fields` (saved with serde default, hashed only once
  non-empty, replicated as the appended shared `SectionKey::Fields`). Zones stack as
  departures from neutral and blend linearly across their feather (smoothstep). Anomaly
  rooms of a generator should become zones (or a source) at world build; bosses can place
  zones in the tick. Dev: `WorldState::place_field_zone` / `clear_field_zones` before play,
  `TestModeAction::{PlaceFieldZone(FieldPreset), ClearFieldZones}` in play (the game's
  test-mode pause entries), 14 presets (`GravityFlip`, `GravityShear`, `Weightless`,
  `SlowTime`, `FastTime`, `Mirror`, `PixelRot`, `Dark`, `Prism`, `Haze`, `Lens`, `Fog`,
  `Ice`, `Showcase`).
- **Readers**: rigs (`world/movement.rs`), pilots (`PilotView::gravity_at`, time scale,
  mirroring, sideways drift in `advance_pilots`), enemies, drops and charges
  (`BodyParams::in_field` / `at_fields`: new `gravity_x` and `time_scale` fields, 0 and 1 in
  every literal). Mirroring flips the stick and rotation input (`ControlState::mirrored`) in
  `simulate_tick`. Not yet field-aware (documented seams): hanging boulders, enemy shots,
  drill directions (still world axes) and the drill timers; time scale slows motion and fuel
  burn only.
- **Craft rotation** (`src/attitude.rs`): `Player::attitude: RigAttitude { orientation,
  angular_velocity }` (per tick, in `PlayerRig` / `PlayerTickState`, so predicted and
  reconciled like the pose). `ControlState` gains the rotation thrusters (`with_rotation`,
  hundredths) and assist bits (`with_assists`). Only the player turns the rig: lean limit
  `BASE_TILT_LIMIT` (35 degrees) flying where rotation is limited, plus `TILT_LIMIT_PER_TIER`
  per `AttitudeGear::tilt_rating` (upgrade hook in `HazardGear::attitude`, no shop line yet:
  F10 adds it; test mode fits everything); unlimited where `FieldSample::rotation_unlimited`
  (shear, upside down, weightless, `free_rotation`). Resting or drilling settles level to the
  gravity frame (exactly 0 on the home world, so the classic path resumes). Gyro stabiliser
  and horizon hold work only when fitted **and** switched on (off by default). Rotation is
  integrated in fixed 240 Hz sub-steps on a 1/65536 rad grid with deterministic trig
  (`field::trig`), so it is tick-rate safe and cross-platform. A rotation that would swing a
  corner into rock is refused (`rig_collides` uses the turned corners).
- **Field drive**: non-neutral fields or a leaning rig use `field_velocity`: the classic drive
  in the gravity's frame; the main thruster pushes along the rig's own up axis (leaning while
  thrusting steers, `TILT_STEER_GAIN`), braking scales with friction (ground) or drag (air),
  exact drag integration with the time-scaled tick. V9 jet streams are not applied in the
  field drive (seam).
- **Formats**: `PROTOCOL_VERSION` 15 (netcode doc lists it). Saves: no format bump needed
  (every new field has a serde default; old saves load level and zone-free). No event codes
  added (370-379 stay free for this track's follow-ups).
- **Tests**: `field/tests.rs` (sampling, feathered blending, stacking, boundary blend),
  `attitude.rs` (limits, settling, momentum, assists, tick rates), `world/field_tests.rs`
  (sideways and upside-down falls, drops sideways, time scale at 20-120 Hz, mirroring, lean
  limit, craft-relative thrust, settling, 360 degrees, corners into rock, neutral bit-for-bit,
  zone replication and hashing, test-mode zones), netcode
  `world_rigs_rotate_in_fields_predict_and_converge`.

## Wave 7 fluids (Part III F9: `src/fluid.rs`, `src/world/fluids.rs`)

Fluids v1: water, acid, brine and liquid metal as deterministic cellular fluids, plus the
swim/submarine movement mode. Seams for the other wave 7 tracks and later waves are listed last.

**Storage: fluid is terrain.** A fluid cell is a `TileKind::Fluid(FluidKind)` tile (appended
variant; kind code `FLUID_TILE_CODE_BASE` = `0xA00 | FluidKind::code`, clear of `0x100`-`0x7FF`
and `Built`'s `0xB00`) whose `durability` byte is the level: `FLUID_FULL` (100) per full tile, up
to `FLUID_MAX` (250) when compressed deep in a column. `TileKind::is_open` (air or fluid) is
what moves through: `Terrain::is_solid_at`, the drill's open-target checks, `chip_tile`, pilot
blocking, spawn clearance and the sky isles' collapse use it, so rigs, pilots, entities and drops
pass through fluid. Blasts leave fluid alone (they open its walls); a solid tile written over a
fluid cell removes it. Because fluid is tiles, saves (`TileDiffs`), keyframes, stale-chunk
refreshes, the terrain digest and the state hash cover it with no new section, and only cells
that changed cross the wire (the terrain op log): a settled lake writes nothing. Code that asks
"is this tile open?" should use `is_open()`, not `== TileKind::Air` (the existing `== Air` checks
were audited: enemy spawning, burrowing and the balance bots treat fluid as not-open on purpose).

**The fluid pass** (`WorldState::advance_fluids`, last in `simulate_tick` before the terrain
journal closes): every tile written this tick wakes the fluid cells beside it
(`FluidBreached` when an opened tile exposes fluid, once per tick). Then the steps due by the
tick run: `fluid::fluid_steps_by(tick, hz)` gives `FLUID_STEPS_PER_SECOND` (20) per second of
world time at every tick rate, so fluid is tick-rate safe
(`fluids_are_deterministic_and_tick_rate_safe` compares 20, 30 and 60 Hz cell for cell). A step
takes at most `FLUID_STEP_BUDGET` (2,048) woken cells, round robin from a cursor, and:

1. runs reactions and density swaps in position order, in place (a denser fluid swaps whole
   with a lighter one under it; `FluidKind::density`: water < acid < brine < liquid metal);
2. proposes moves from one snapshot: fall (`stable_lower`, the classic compressible split:
   `FLUID_COMPRESSION` extra per row, so pressure grows with depth and fluid climbs U-bends),
   then spread to each side `(base - side + 1) / viscosity`, then climb when compressed past a
   full tile;
3. applies the moves in order as transfers (clamped by the target's room; a move into a cell
   another fluid took first is held back), writes only the cells that changed, and wakes those
   cells and their fluid neighbours. Unchanged cells go back to sleep.

Volume is exactly conserved except by the defined sinks: quenching lava (`QUENCH_WATER_COST`),
cooling metal (`COOL_WATER_COST` of water; the metal sets into `COOLED_METAL_ORE`, iron ore, when
it is at least `METAL_ORE_LEVEL` or the edge of a bigger body, else slag dust), acid eating a
soft tile (`ACID_DISSOLVE_COST`; `acid_dissolves`: dirt, clay, sand, mud, snow; about one bite
per 2 s, `ACID_DISSOLVE_ODDS`, a fixed hash of position and step), and a solid tile written over
fluid. `FluidReacted` fires once per reaction per step. A settled surface may slope by a level
or so per column (integer spreading stops at a difference of one).

**The schedule** (`FluidSchedule`, the world's `fluids` field): the woken cells and the cursor.
It is saved (a save mid-flood resumes exactly: `a_save_mid_flood_resumes_exactly`) but neither
hashed nor replicated, and `redacted_for` clears it in client keyframes: a client's authority
never ticks, and its predicted copy wakes fluid from its own predicted terrain writes (drilling
into a pocket starts the flood at once); host tile writes correct it through the op log.
Staging seam: `WorldState::wake_fluids(rect)` next to `terrain_mut` (fluid written straight into
the terrain sits still until woken).

**Bodies.** `WorldState::submersion(x, y, half_height)` samples five points down a body against
each tile's fluid surface (`Submersion { share, kind, depth_rows }`). Rigs
(`apply_fluid_to_rig`, after the movement step): buoyancy `RIG_GRAVITY * buoyancy * share`
(water 0.82 sinks slowly, brine 1.12 bobs at the surface, molten metal 1.6), drag
`1 / (1 + drag * share * dt)`; at `SUBMARINE_SHARE` (0.6) the rig is a submarine: drill held off
the ground dives (`SUBMARINE_DIVE_ACCELERATION`), speed is capped at `SUBMARINE_TOP_SPEED`, the
pumps burn `SUBMARINE_FUEL_PER_SECOND` while it steers; `RigSubmerged` / `RigSurfaced` fire as
it crosses. Damage: pressure past the hull's rating (`PRESSURE_DAMAGE_PER_ROW` per row over),
acid (`hull_damage_per_second * share`, less acid resistance) and molten metal (through lava
plating, less heat resistance). The old static cave water (V6 flooded caves, `water_at`) is
unchanged. Pilots: `PilotView::water` counts any fluid at least half full (the existing swim
mode, breath and `PilotSplashed`), `PilotTerrain::swim_sink` (a new trait method, `SWIM_SINK` by
default) lifts swimmers in brine and metal, and acid and metal hurt (`pilot_fluid_damage`).
Creatures: `EnemyKind::can_swim` (false for every kind today); `keep_out_of_fluid` puts a
non-swimmer back where it was when a step would carry its middle into fluid it was not already
in (flyers and walkers, after `physics::step_body`). Enemy spawning already refuses fluid
(`air` means `TileKind::Air`) and `floor` no longer counts fluid as ground.

**Events** (netcode codes 380-383): `FluidBreached { kind, x, y }`, `FluidReacted { reaction,
x, y }` (tile coordinates), `RigSubmerged { player_id, kind }`, `RigSurfaced { player_id, kind }`.
Codes 384-389 stay free in the fluids range.

**Formats.** `PROTOCOL_VERSION` 15 (fluid tiles, the events, `TestModeAction::PourTestPools`) and
save format 13 (fluid tiles in the diffs, the schedule); formats 8-12 load as is. No worldgen
change: the V1-V10 golden digests stand (V1-V10 never place fluid).

**Known limits (measured in review).** Levelling is diffusive: every move is a local transfer,
so a wide body evens out in time that grows with the square of its width. A cave pool or a
breached pocket of a few dozen cells settles in seconds, but `fluid_lake_cost_report` (ignored;
`cargo test --release -p fatherload_simulation fluid_lake_cost_report -- --ignored --nocapture`)
floods a 100 x 50 cavern from 2,000 full cells and the lake is still levelling after 600 s of
game time. Over that run it averaged about 140 tile writes per tick, peaking near the budget at
about 1,950, and each write is a terrain op sent to every client the chunk is relevant to. CPU
cost averaged 0.6 ms per 60 Hz tick on a heavily loaded machine. Volume stays exact.
Compression also squeezes a deep column: 35 rows of water poured into a shaft settle into 25
compressed rows, so a tall body's surface drops as it wakes. V11 strata should keep generated
bodies small (the default `FluidPocketRule` radii do) until a faster levelling scheme lands,
such as pressure propagated along connected runs. Rig motion in fluid is tick-rate stable at
steady state (the same terminal speeds at 20, 30 and 60 Hz). It applies buoyancy and drag after
the movement step, though, so positions drift apart by a constant offset of about half a tile
between 20 Hz and 60 Hz while a rig speeds up or slows down.

**Seams for other tracks and later waves:**

- *Generation (F8, the V11 generator; wired in the Wave 7 integration)*:
  `fluid::fluid_pockets_in_chunk(seed, chunk, &[FluidPocketRule], tile_at) -> Vec<(TilePosition,
  Tile)>` is pure and chunk-local (it reads and writes only tiles inside the 16x16 chunk): call
  it per generated chunk after caves, with the rules of the stratum the chunk is in, and write
  its tiles into the chunk. Rules are data a stratum registry can hold:
  `FluidPocketRule::sealed(kind, per_mille)` (an elliptical aquifer replacing diggable rock whose
  whole rim stays rock, so it sits still until breached) and `FluidPocketRule::cave_pools(kind,
  per_mille)` (the bottom rows of walled, floored cave basins). Generated fluid is full tiles at
  rest and costs nothing until a neighbour changes.
- *F9 field effects*: a per-region field registry can read `WorldState::submersion` /
  `Terrain::fluid_at` for buoyancy and drag; each fluid's own buoyancy, drag, viscosity and damage
  live on `FluidKind`.
- *Descent IV gear (F10 progression)*: `WorldState::submersion_gear(player) -> SubmersionGear {
  pressure_rows, acid_resistance, heat_resistance, handling }` (today `STOCK_PRESSURE_ROWS` 24 +
  8 per pressure bracing tier) is where the Pressure Hull, Bio-Filter, Cryo Hull and ballast or
  sonar upgrades plug in; `WorldState::pilot_breathes_in(player, kind)` (false today) is the
  rebreather / Bio-Filter seam for pilots.
- *Swimming creatures (Descent IV)*: answer `true` from `EnemyKind::can_swim`.
- *Dev and tools*: `TestModeAction::PourTestPools` (pause menu "Pour Test Pools" in a test-mode
  world) writes `fluid::demo_basins(origin)`, one basin per fluid; the screenshot and balance
  tools can stage the same tiles. Rendering reads `Terrain::fluid_at` only (no snapshot field);
  terrain chunks paint fluid as open ground and skip repaints for open-to-open changes, and
  `rendering/world/fluids.rs` draws the fluid live.

## Wave 7 logistics: depth logistics, progression curves and the descent harness (F10)

Expansion plan Part III §15 F10, so a 13,000-row dig stays a journey instead of a commute.
Modules: `src/logistics.rs` (state, commands, prices), `src/logistics/sites.rs` (the generator
hooks), `src/world/logistics.rs` (the tick), `src/depth_progression.rs` (rules, Descent layout,
deep tiers, rig systems, curves) and `src/balance/descent.rs` (the harness). Tests:
`world/logistics_tests.rs`, the modules' own, `balance::descent::tests`, and the game's
`local_game/tests/logistics_tests.rs`.

### Rules: classic worlds never change

`depth_progression::ProgressionRules` is `Classic` for every generator up to V10 and `Descent`
for every generator after V10 (`ProgressionRules::for_generator` compares versions, so
generator V11 gets the Descent rules the moment F8 adds it, with no edit here). It is stored in
the `Logistics` section (`WorldState::progression_rules`, set at world creation from the
generator; scenarios and tests switch with `WorldState::set_progression_rules` before play).
Classic worlds keep the fifteen-tier ladders, classic prices and no rig systems on sale; their
`Logistics` stays empty and is left out of the state hash, so their hashes are unchanged. The
logistics tools themselves (beacons, built lifts, dive charges) work on every world; outposts
and ancient lifts exist only where a generator placed them.

### Lifts

- `LiftShaft { column, stations, origin, restored, cab_row }`: stations are the rows a rig stands
  on to board. `LogisticsCommand::RideLift { shaft, to_row }` from a station (rig on the shaft's
  column within `STATION_REACH_ROWS`) starts a `LiftRide { from_row, to_row, start_tick, ticks }`:
  `ticks = ticks_for_millis(ride_millis(rows, rows_per_second))` (ancient 45 rows/s, built 20),
  so a ride is the same length in seconds at every tick rate and fully determined by its start
  tick. While riding, the rig neither drills nor flies (`simulate_tick` skips its drill intent
  and movement step), `advance_logistics` pins it to the cab (`LiftRide::row_at`) every tick,
  burns no fuel, cannot climb out (`can_exit_rig`, `TogglePilot` refused), and lands resting on
  the arrival station's floor. A wrecked rider drops out of its ride. The path must be open
  except the lift's own station floors (and, on an ancient lift, its rail).
- **Ancient lifts** are a rail of `BuiltTile::RuinWall` (indestructible, so nobody falls down a
  1,000-row shaft) broken at each station by a three-wide, two-high pocket on a `Plating`
  floor. They start dead: `RestoreLift` works only from the lowest station
  (`ANCIENT_LIFT_RESTORE_PRICE` x the Descent's price percent), so a lift shortens the trips
  back up once you have dug to its bottom, never the first descent.
- **Built lifts**: `PlaceLiftStation` on the rig's tile (underground, `LIFT_STATION_PRICE` x
  percent) starts a lift on that column or adds a station to it; an open tile under a station
  gets a `Plating` floor (a tile op) so a rig can stand at the mouth of a shaft or halfway down
  it. `RemoveLiftStation` (owner only) refunds half and removes the floor it placed.
  `MAX_LIFT_STATIONS`, `MAX_BUILT_LIFTS_PER_PLAYER`.

### Deep outposts

`Outpost { name, column, floor_row, descent, claimed_by, reached }`: a room
`OUTPOST_HALF_WIDTH` either side of its column and `OUTPOST_HEIGHT` high. A rig inside it the
first time marks it reached (`OutpostReached`): it joins the crew's travel network. Its counter
(`LogisticsCommand::Outpost { outpost, service }`): refuel and repair at
`OUTPOST_FUEL_UNIT` / `OUTPOST_REPAIR_UNIT` credits a unit (unclaimed, claimed), selling through
the depot's own sale (`sell_canonical_cargo`) less the outpost's cut (`OUTPOST_SALE_PERCENT`
75 / 95), dive charges, `Claim` (`OUTPOST_CLAIM_PRICE` x percent; crew-wide), and `SetHome` on a
claimed outpost, which makes rescues land there (`WorldState::rescue_home` falls back to it after
a home bed and a base home point).

### Depth beacons and travel

`PlaceBeacon` on the rig's tile (`BEACON_MIN_DEPTH_ROWS` under the local surface, at least
`BEACON_SPACING_TILES` from another beacon, `MAX_BEACONS_PER_PLAYER`, `BEACON_PRICE` x percent).
`Travel { to: TravelNode }` from any stop (`travel_stop_of`: within `BEACON_REACH_TILES` of any
crew beacon, inside a reached outpost, or in the town: the buildings' span plus
`TOWN_TRAVEL_MARGIN`, from the surface up to `TOWN_TRAVEL_CEILING_ROWS` above it, so a sky isle or
the Moon over the town is not a stop) to the town, any beacon or any reached outpost,
for `travel_cost(rows)` fuel and credits (4 + rows/80 fuel, 10 + rows/20 credits: a 350-row jump
costs 8 fuel and 27 credits, the whole dig about 170 fuel and 660 credits). The rig arrives at
rest, its drill and deep-run pressure cleared (`BeaconTravelled` carries both ends for the
effects). A buried destination refuses the jump.

### Dive charges (the momentum tool)

`BuyDiveCharges` (in town or at an outpost; `DIVE_CHARGE_PACK` 3 for 240 credits, up to
`MAX_DIVE_CHARGES`). `FireDiveCharge` cuts plain rock and ore straight down under the rig for up
to `DIVE_CHARGE_ROWS` rows (stopping at hazards, relics, built pieces and bedrock; ore it cuts goes
into the bay), snaps the rig onto the column and kicks it down at `DIVE_KICK_SPEED`. Until it
settles (`DIVE_SETTLE_MILLIS` still, or `DIVE_MAX_MILLIS`), the movement step's crash damage is
off (`Logistics::is_diving`).

### Progression curves to the Heart

- **Deep tiers**: on Descent worlds the six core lines run to `DESCENT_MAX_UPGRADE_LEVEL` (22,
  `UpgradeKind::max_level_for`, carried in `UpgradeOffer::max_level`), capped by the deepest
  Descent anyone reached (`core_tier_cap`: 6, 10, 13, 15, 17, 19, 21, 22; a locked tier is
  `UpgradeLock::Descent(index)`). Tiers up to the classic 15 keep their classic prices; past it
  each costs `DEEP_TIER_GROWTH_PERCENT` (135) more (`upgrade_cost_for`). The stat curves were
  already formulas and extend monotonically (tank ~30,900 fuel, hull ~5,100, bay ~1,270 slots at
  tier 22); the deep tiers have their own names (`upgrade_tier_name` names one for a core
  `level` of 15 or more, so a caller naming a maxed classic line clamps `level` below
  `UpgradeOffer::max_level`) and previews (`upgrade_preview::upgrade_preview_for(.., rules)`;
  the classic `upgrade_preview` stops at 15).
- **Rig systems**: eight new one-tier upgrade lines appended to `UpgradeKind` (shop offers
  18-25): `DescentPressureHull`, `BioFilter`, `LampII`, `SonarSuite`, `CryoHull`,
  `GravityAnchor`, `PhaseDrill`, `HeartKey` (`UpgradeKind::rig_system` ->
  `depth_progression::RigSystem`), stored as bits in `HazardGear::rig_systems`. Classic worlds
  never list them (`economy::shop_line_count(rules)` is 18 there, the lines before the rig
  systems, so a classic shop keeps its 18 rows; `upgrade_lock` still answers
  `UpgradeLock::DescentOnly` for them). On Descent worlds each goes on sale once anyone
  reaches the last stratum of the Descent before the one it opens
  (`RigSystem::sold_from_row`, `UpgradeLock::Depth`).
  `economy::grant_rig_system` fits one without the shop (a later boss reward).
- **Gates** (`descent_gate`, `missing_gate`): Descent III needs the Pressure Hull, IV the
  Bio-Filter, V the Cryo Hull, VI the Gravity Anchor, VII the Phase Drill, VIII the Heart Key
  (every gate above too). The first, stub effect: on Descent worlds a rig in a Descent whose gate
  it lacks loses `GATE_DAMAGE_PER_SECOND` hull a second, with one `DescentGateBlocked` warning
  per deeper Descent. Initial effects of the companions, Descent worlds only: the Bio-Filter
  halves gas damage (`BIO_FILTER_GAS_PERCENT`), the Sonar Suite adds
  `SONAR_SUITE_SCAN_BONUS_TILES` to the scanner's reach, Lamp II widens the headlamp
  (`LAMP_II_LIGHT_SCALE`, the renderer's `PlayerPresentation::lamp_scale`).
- **Price curves**: `descent_value_percent` (what ore found in a Descent sells for, 100 to 230 %;
  the stratum registry's ores take their values from it) and `descent_price_percent` (outposts,
  beacons and stations, +60 % a Descent).

### Seams for the other tracks and later waves

- **Generator hooks (F8 / integrator):** `logistics::sites::LogisticsSites` with
  `place_ancient_lift(column, top_row, bottom_row, station_spacing)` and
  `place_outpost(column, floor_row, descent, name)`, or the default layout
  `LogisticsSites::plan(seed, layout, spawn_column)` (one outpost per Descent on one seeded
  column near town, Descent I's at its middle, the rest 40 rows under their Descent's top, and
  the builders' cable: an ancient lift from just under the surface to the first outpost and from
  each outpost down to the next on the column beside them). The V11 generator asks
  `LogisticsSites::tile_at(position)` for every tile it generates (`Some(kind)` overrides its
  own; a pure function, so chunks carve identically in any order), and world creation calls
  `WorldState::install_logistics_sites(&sites, false)`. Worlds whose generator did not carve
  them (scenarios, tests, scenes) pass `stamp: true`. V1-V10 generation is untouched: their
  golden digests stand.
  Chained ancient lifts share their meeting station (one lift's bottom row is the next one's
  top, on the same column); `Logistics::stations_at(tile)` lists every shaft stopping there and
  the dialog offers each one's rides (`Logistics::station_at` returns only the first).
  **Row frame:** the plan, `DescentLayout` and the gates use rows below the ground line as
  terrain rows directly (the ground line is about row 6 today); if V11 puts its ground line
  elsewhere, offset the rows in `layout_for` / the generator's call to `plan`.
- **Descent layout adapter (F8):** `depth_progression::DescentLayout` (the eight Descents and 41
  strata as row ranges below the ground line). `PlanLayout` models §14's table;
  `depth_progression::layout_for(settings)` is the one place that picks it, so the stratum
  registry replaces it there. Everything here (tier caps, system sale rows, gates, the logistics
  price percent, `WorldState::descent_of_row`) reads it.
- **Stratum registry adapter (harness):** `balance::descent::StratumRegistry`; `PlanStrata`
  derives rock, ore, hazards, heat and content per stratum from the plan. Implement it for the
  real registry and pass it to `run_descent`.
- **Gate effects (Waves 8-10):** each gate's crush is a stub: replace `GATE_DAMAGE_PER_SECOND`
  per Descent with its mechanic (pressure, spores, cold, gravity, alloy) by reading
  `missing_gate`; bosses grant systems with `grant_rig_system`. The Phase Drill, Gravity Anchor
  and Cryo Hull have no effect yet beyond their gates (`HazardGear::has_system`).
- **Events 390-399:** `LiftRideStarted`, `LiftArrived`, `LiftRestored`, `LogisticsPlaced`,
  `OutpostReached`, `OutpostClaimed`, `OutpostServiced` (`outpost: u32::MAX` for charges bought
  in town), `BeaconTravelled`, `DiveChargeFired`, `DescentGateBlocked`. Their sounds reuse
  existing patches (no new audio patch).
- **Formats:** `PROTOCOL_VERSION` 15 (the new command, section, gear field, upgrade kinds and
  events) and save format 13 (formats 8-12 load as is; every new field has a serde default).
  The integrator collapses both with the other tracks'.
- **Shared-file edits** (all additive): `simulate_tick` (the riding skip, the logistics command
  branch, the Bio-Filter on gas, `advance_logistics` after `advance_bases`), `movement` (dive
  landings), `commands` (the rules-aware shop purchase, the Sonar Suite's reach), `bases`
  (`rescue_home`'s outpost fallback), `pilot` (`can_exit_rig` while riding), the replication
  coverage tables, and `economy` (the appended upgrade kinds, `UpgradeOffer::max_level`,
  `*_for(rules)` variants beside the classic functions, which keep their behaviour).

### Game side

One dialog (`ModalScreen::Logistics`, `game/src/shell/logistics.rs`) serves every stop: the use
button opens it at an outpost, a lift station or one's own beacon (the touch and keyboard use
prompt says so, `ViewSnapshot::logistics_stop`), and the inventory's last row ("Depth
logistics") opens it anywhere, so keyboard, gamepad, touch and split-screen seats all reach it
through inputs they already have (no new action or prompt). Its rows are rebuilt from the world
every frame with the command each sends and the simulation's own verdict and price
(`WorldState::check_logistics`). The world draws lift rails, station signs and lamps and the cab
(glued to its rider by the ride's ticks), outposts (sign, the crew's flag once claimed, counter
terminal, fuel pump, lamps) and beacons (`rendering/world/logistics.rs`); the shop shows the
deep tiers and rig systems on Descent worlds. Scenes: `logistics_ancient_lift`,
`logistics_lift_station`, `logistics_outpost`, `logistics_outpost_counter`,
`logistics_beacon_travel`, `logistics_built_lift`, `logistics_dive_charge`,
`shop_descent_systems`.

### The descent harness

`fatherload --balance-report --only descent` (or `cargo test --release -p
fatherload_simulation descent_report -- --ignored --nocapture`). A 60-90 hour run cannot go
through the fixed tick in a report and the V11 strata do not exist yet, so the probe is a model
on real numbers: the Descent ladders, caps and prices, the rig systems, the tank, bay and hull
curves, the drill's chip formula with the pace between tiles measured through the tick
(`measure_dig`: a rig drilling a column of hard rock; 0.43 s a tile at 60 Hz), fuel burn, and
the logistics prices. The strata come through `StratumRegistry` (`PlanStrata` until F8). The bot
must make its rig rated for the next stratum (drill, hull, radiator, bay, tank, engine, and the
gate system at a Descent's end), plants a beacon every `BEACON_SPACING_ROWS`, claims outposts
once rich, sells in town (or at a claimed outpost) and pays each stratum's authored content
minutes. It reports time per stratum and per Descent against §14, the gaps between new things
(arrival, creatures, landmark, story beat, upgrade tiers, gates), deaths, and fuel on everyday
trips against an expedition (outpost to the Descent's bottom and back without beacons).

First result (60 Hz, the plan registry):

| Descent | Ends at (h) | §14 target (h) | Everyday trip fuel (max % of tank) | Expedition fuel (% of tank) | Deaths |
|---|---|---|---|---|---|
| I The Crust | 8.0 | 5-8 | 56 | 64 | 0 |
| II The Old Deep | 16.9 | 13-18 | 17 | 17 | 0 |
| III The Living Dark | 24.2 | 22-29 | 13 | 9 | 0 |
| IV The Drowned and Frozen | 32.3 | 31-40 | 11 | 5 | 0 |
| V The Hollow World | 40.6 | 42-53 | 13 | 4 | 0 |
| VI The Strange Layers | 52.1 | 53-66 | 11 | 3 | 0 |
| VII The Ancients | 67.1 | 66-82 | 12 | 2 | 0 |
| VIII The Heart | 76.5 | 75-90 | 11 | 2 | 1 |

Whole run 76.5 h (target 60-90 h); every gap between new things is at most 40 minutes (longest
30 min, at the Seal Gate's grind for the Heart Key); gate systems bought at 16.9, 24.2, 32.3,
40.6, 52.1 and 67.1 h. What it found and changed on the way: the first ore value curve
(3,200 % of classic at the Heart) let one trip buy a whole Descent's tiers, so it is now
100-230 %; beacon travel at 25 + rows/4 credits cost more than a Crust haul, so it is now
10 + rows/20; and on a 1,100-row Crust a stock tank cannot fly a trip below about row 250 without
a beacon (the bot stalls at Grey Stone without one). Open for Wave 8+: everyday trips are calm,
but expeditions are not stressful past Descent II (the deep tanks dwarf a Descent's climb), the
Descents V-VI run a little ahead of §14, and the harness's hazards almost never wreck a rig;
the stratum registry's real hazards, fuel drains and ores should replace `PlanStrata` and the
table be measured again.

## Wave 7 integration: the seams wired together

The five Wave 7 tracks (F7 positions, F8 strata, F9 effects, F9 fluids, F10 logistics) were
stacked in that order on bb455f4, each later track ported to the F7 frame, and their seams
connected. Formats: one `PROTOCOL_VERSION` (15) and one save format (13) for the whole wave
(`save::tests::every_supported_save_format_loads_plays_and_round_trips` loads formats 1-12
without any wave 7 field, plus a V11 world in format 13). Event codes: fluids 380-383,
logistics 390-399 (350-379 stay free). Fluids' five audio patches stay at the end of
`patches!`. V10 stays `GeneratorVersion::CURRENT`; V11 is "Deep Descent (preview)".

### Positions in the later tracks (F7 porting)

- **V11 floats its frame** (`GeneratorVersion::floating_frame`: V11 `true`), so a dig 13,000
  rows down re-centres the frame about every 512 rows. `column_limit` stays
  `STREAMED_COLUMN_LIMIT` for V11 (its sky, Moon, villages and tombs are V10's, sized to it).
- Ported to the frame: the field query (`field::sample_in(zones, settings, frame, x, y)`,
  `FieldZone::weight_in`, `FieldZones::weights_at(frame, ..)`; `field::sample` is the origin
  frame reading for tests), the rotated corners (`rig_collides` reads `terrain.frame()`), the
  dev field zone's tile, fluid submersion and the keep-out check, the swimmer's sink tile, the
  pilot's fluid air, the PourTestPools tile, the logistics tick (rig tiles, `column_x` /
  `resting_y` in the frame: lift cabs, landings, dive snaps and rescue homes), the scanner's
  centre, and in the game the stratum banner, depth ruler and map footer, the fluid and
  logistics renderers (`view_frame`) and the underwater audio check. `reframe_positions`
  names `fields`, `fluids` and `logistics` (tiles, never moved).

### Strata feed the field effects (F8 -> F9)

`descent::fields::stratum_field_source` is registered in `field::STRATUM_FIELD_SOURCES`:

- `descent::fields::field_profile(stratum, strength)` converts the registry's integer
  `descent::FieldProfile` to `field::FieldProfile` (gravity, time, grip, drag; light clamps to
  1 and light past 1000 per mille reads as exposure; the grade leans to the stratum's accent
  colour, the fog to its back wall; every registry `ScreenEffect` maps to a pass except
  `Blur`, which has none yet).
- The source answers `None` (neutral, the classic paths) in every older world, above the first
  stratum and wherever the strata near the point are neutral; otherwise the stratum's field
  blended over `BLEND_HALF_ROWS` (8) across each layout boundary.
- New registry fields (neutral by default): `FieldProfile::free_rotation` (a 360-degree zone),
  `FieldProfile::regions` (`FieldRegions::GravityTurns { per_mille, turns }` /
  `Mirrored { per_mille }` in seeded `FIELD_REGION_TILES` (48 x 32) cells feathered over 6
  tiles) and `FieldProfile::deepening` (screen effects from half strength at the stratum's top
  to full at its bottom).
- Placeholder effects per §14: Gravity Shear (gravity turned 90/180/270 degrees in 45 % of its
  cells, free rotation everywhere, wobble), Upside-Down Isles (gravity flipped in 35 % of its
  cells, free rotation), Mirror Strata (screen and controls mirrored in 40 % of its cells),
  Pixel Rot (pixelate and palette deepening), Inner Sky (glare from its 1400 light, a warm
  grade), plus every value the strata track already set (Glow Sea dark, Underground Ocean
  drag, Brine Glaciers ice, Time Sludge slow time, Sunforge glare and bloom, Pearl Abyss,
  Prism Desert, Singularity Fields, ...).
- The layout is cached per thread (`descent::fields::cached_layout`, a pure function, so the
  cache never changes an answer); a field sample is one cached lookup plus a few operations.
- Rigs, pilots, creatures, drops and charges read it through `fields_at`, and each seat's
  screen-effect stack through `render_snapshot` (the game's existing path).
- Tests: `descent::fields::tests` (older worlds and quiet strata neutral, visible placeholder
  effects, region turns and mirrors, smooth boundary blend, the same field in any frame, a rig
  spinning past 180 degrees in the Gravity Shear while leaning only to 35 degrees at home).
  The descent bot hops a region cell lower when a turned cell stalls its straight-down dig.

### V11 generation calls the fluids and logistics hooks (F8 -> F9 fluids, F10)

- **Fluids**: `StratumDef::fluids: &[FluidPocketRule]` (empty by default). `assemble` calls
  `fluid::fluid_pockets_in_chunk` last, over the finished chunk, with the rules of the stratum
  at the chunk's centre: water in the Fossil Oceans, Glow Sea, Underground Ocean, Geyser
  Fields, Merrow City and Pearl Abyss; brine in the Brine Glaciers and Pearl Abyss; acid in the
  Flesh Caverns, Time Sludge and Dark Matter Veins; liquid metal in the Mantle Storm Zone, the
  Sunforge, Engine Halls, Core Ocean and the Heart. Generated fluid is full and at rest until
  something breaches it.
- **Logistics**: every outpost hall of a world now stands on one seeded column
  (`landmarks::outpost_column`, 60-139 columns to one side of the spawn), and
  `landmarks::logistics_sites(settings)` turns the halls into logistics outposts (floor = the
  hall's floor) and runs the builders' cable beside them (`landmarks::lift_column`, two columns
  outside the far door): an ancient lift from `LIFT_TOP_ROW` (6 rows under the ground line) to
  the first outpost and from each outpost to the next. `Ctx::logistics` carves its tiles over
  the landmarks; `WorldState::from_import_with_params` installs its records (`stamp: false`)
  on every V11 world.
- **Depth rules**: `depth_progression::layout_for(settings)` now returns a `WorldLayout`: the
  registry's seeded rows (`RegistryLayout`, terrain rows) on V11 worlds, the plan table
  otherwise; tier caps, rig-system sale rows, gates and logistics prices read it.
- `GOLDEN_V11_DIGEST` is updated for the fluids, the outposts' column and the cable; the V1-V10
  digests are unchanged. Tests: `world::deep_wiring_tests` (fluid pockets per stratum and none
  in V10, outposts and the cable carved and installed, the registry layout, an ancient lift ride
  of more than two sectors with the frame following the rider).

### The descent harness on the real registry (F8 -> F10)

`balance::descent::RegistryStrata` implements `StratumRegistry` (and its new `descents()`)
from the registry on the default seed's V11 layout: names, seeded rows from the ground line,
common rock, ore values from the registry's ore curves (times the Descent value curve), hazard
weights, and novelties and content minutes from creatures, set pieces (outposts, gate chambers,
anomaly rooms, the cavern, settlements) and field or fluid mechanics. Drill and radiator tiers
and deep rock toughness stay modelled. `--balance-report --only descent` runs it (`PlanStrata`
remains for comparison). The integration run: 76.0 h to the Heart (target 60-90 h), all gaps
between new things 40 minutes or less (longest 37), 1 death, gate systems at 21.3 / 26.5 /
33.0 / 39.1 / 53.8 / 66.8 h. Descent I takes 14.3 h against the plan's 5-8 h: the registry's
Crust carries the classic ore curves (mostly copper in the Topsoil), poorer than the plan
model's mix, so the bot grinds the Crust at tier 1 with trips longer than one tank (168 % of
it). Wave 8's Crust re-pacing has to answer that.

### Still open

- The Hollow World outpost can sit over a cave (its plated floor holds; the strata test now
  asks that most of the floor rests on rock).
- Placeholder strata borrow their host band's heat: a stock rig deep in a V11 world overheats
  in seconds (scenes and the wiring tests fit a protected rig).
- Region cells are a placeholder for authored gravity and mirror zones; the Mirror Strata's
  twins, Echo Caves' sonar and the Prism Desert's colour keys are Wave 9 content.
- The fluids' slow levelling of wide bodies and deep-column compression (see the fluids
  section) are why the generated pockets stay small.

## Wave 8 progression: Descent I-III progression, deep economy and V11 readiness

The progression track of Wave 8 (expansion plan §13 "making a long dig fun", §14 cross-Descent
systems, F10). Modules: `src/depth_progression.rs` (the new curves and rules, after "Wave 8"),
`src/workshop.rs` (layer resources and recipes), `src/world/logistics.rs` (the tick and
commands), `src/balance/descent.rs` (the probe); tests in `world/deep_progression_tests.rs`,
`workshop::tests`, `balance::descent::tests`, and the game's `shell::onboarding` and
`shell::world_setup` tests. **Classic worlds never change**: everything below is gated on
`ProgressionRules::Descent` or `GeneratorVersion::has_descents`; V1-V11 generation is
untouched (no golden digest changed, `GOLDEN_V11_DIGEST` included).

### Upgrade tiers per Descent

| Line | Per Descent | Rule |
|---|---|---|
| Drill, hull, fuel tank, cargo bay, engine, radiator (cooling) | `core_tier_cap` 6, 10, 13, 15, 17, 19, 21, 22 | Wave 7, unchanged |
| **Pressure Rating** (`UpgradeKind::PressureRating`, offer 26) | tier n = rated for Descent n (stock 0 = the Crust), 7 tiers | `depth_line_cap(reached)`: on sale once someone reached the Descent it is rated for |
| **Headlamp** (`UpgradeKind::Headlamp`, offer 27) | one tier per Descent, 7 tiers | same cap; `headlamp_light_scale` (+15 % reach a tier) multiplies Lamp II's in `PlayerPresentation::lamp_scale` |

- Prices: `depth_line_cost` (Pressure 3,000 to 560,000; Headlamp 1,500 to 360,000), tier names
  per Descent (`depth_line_tier_name`: "Old Deep Seals", "Lure Lamp", ...), previews and
  `upgrade_effect` lines. Stored in `HazardGear::{pressure_rating, headlamp}`. Classic shops
  still list their 18 lines (`CLASSIC_SHOP_LINES`); both new lines are `is_descent_only`.
- **Pressure** (`pressure_fuel_percent`): thruster burn is +35 % per Descent below the rating
  (at most 4 deficits), read every tick by the movement step (`pressure_burn_factor`, exactly
  1.0 on classic worlds, so their movement is bit for bit unchanged). Felt on arriving in a
  Descent; its tier goes on sale the same moment.
- **Rig systems are felt before they are sold** (`UpgradeLock::Unfelt`): the Wave 7 sale row is
  still needed, and then the crew must have felt the need (`Logistics::felt_systems`, crew-wide
  bits): a gate system the moment a rig crosses into the Descent it gates, Lamp II where the
  field light is at most `LAMP_II_FELT_LIGHT` (the Glow Sea), the Sonar Suite on reaching its
  sale stratum (Wave 9 refines). `upgrade_lock_for` takes the felt bits (a new argument).
- **The gate crush ramps** (`gate_damage_per_second`): 3 hull/s on the gate's first row up to
  the Wave 7 stub's 14/s over `GATE_RAMP_ROWS` (40), so a dip to feel it is survivable.
- **Fair placeholders below Descent III** (`placeholder_heat_cap_units`): on V11 worlds the
  band heat of Descents IV-VIII is capped at what the radiator tier on sale before the Descent
  opened cools (`radiator_cooling(core_tier_cap(d - 1))`, no shielding needed). A rig with the
  previous Descent's caps and the gate systems holds its hull in the Heart (test). Waves 9-10
  replace the cap with each stratum's own heat.

### Deep shops

`LogisticsCommand::OutpostUpgrade { outpost, offer }` buys the next tier of a shop line at an
outpost's counter (`WorldState::outpost_offer` checks: inside the outpost, the shop's own lock,
`outpost_stocks(descent, kind, next)`; price `OUTPOST_SHOP_PERCENT` 115 % unclaimed, 100 %
claimed). Stock: core tiers to the outpost Descent's cap, Pressure Rating and Headlamp up to the
tier rated for the Descent below, hazard counters, scanner, bombs and every rig system; never
sky/Moon gear or the drill mount. `economy::fit_next_tier` fits a tier without charging.

### Layer resources and workshops

- `ItemKind::Layer(Stratum)` (appended; `ItemCategory::Material`, no cargo slots), named by the
  registry's `layer_resource.name`, so the content tracks' names flow through unchanged. On
  Deep Descent worlds, drilling a rock tile (one that yields a block) drops one about every
  `LAYER_YIELD_ONE_IN` (22) tiles, picked by `workshop::layer_yield(seed, tile)` (a hash, so
  every client agrees); it pops out beside the block (`descent_dig_extras`, called from the
  drill chip). Drops glow in the stratum's accent colour; six pixel-art shapes by material
  (`sprites::deep_gear::layer_resource`).
- `LogisticsCommand::Craft { outpost, recipe }` works a `workshop::RECIPES` recipe (17, one per
  stratum of Descents I-III, append only) at an outpost whose Descent is at least the recipe's
  (`outpost_crafts`). Outputs: `Repair`, `Refuel`, `DiveCharges`, `Bombs`, and `Part(kind)`:
  the next tier of a line for the resources plus `PART_PRICE_PERCENT` (40 %) of its shop price
  (the tier must be on sale and stocked there).
- Value curves: `layer_resource_worth(stratum)` (50 + 12 per stratum index, times the Descent
  value curve) and `deep_ore_value(descent, rank)`, the curve new deep ores should take their
  values from. **Ore pays its Descent** (`haul_premium_percent`): the deepest Descent a
  player's haul came from since the last sale (`Logistics::haul_descent`, set when the drill
  breaks ore) multiplies the depot's and outposts' ore payout by `descent_value_percent`; a
  sale resets it, and so does a bay with no ore left (the tick drops the entry: a wreck or a
  village trade does not carry a Descent III premium into the next Crust haul). The depot's
  sell row reads the same `Logistics::haul_premium_percent`, so the payout it shows is the
  payout the sale makes. Known limit: the premium is per haul, not per ore, so one deep ore
  lifts a mixed bay. The Wave 7 value curve was only modelled before; now it is real.
- **Seam for the content tracks**: if a track adds its own layer resource items or tiles, map
  them onto `ItemKind::Layer(stratum)` (or change `descent_dig_extras`); recipes only read
  `ItemKind::Layer` counts.

### V11 for new worlds

`worldgen::DEEP_DESCENT_FOR_NEW_WORLDS` (on in this branch) makes `GeneratorVersion::NEW_WORLD`
the Deep Descent; the new-game dialog starts on it and its World row still offers
**Classic (V10)** (`GeneratorVersion::CLASSIC`). `GeneratorVersion::CURRENT` (what
`WorldSettings::new` and the tests use) stays V10, and saves keep their generator. The dialog's
rows did not change, so the browser tests' navigation (nine downs to Start) still holds; they
now start a Deep Descent world. `--deep-descent` / `--classic` pick the starting world type.
**The integrator flips the constant off unless `DescentRun::pacing_passes(3)` holds** (the
probe's Descent I-III check, below).

### Onboarding

`game/src/shell/onboarding.rs`: each seat's `SeatHints` shows a one-line tip in its toast the
first time it meets a Descent system (`Hint`: gate crush, underrated seals, a system on sale,
an outpost, an ancient lift, a layer resource, fluid, strange gravity, slick ground, the dark,
beacons). Once per seat, never in a dialog or over a fresh message, at most one every
`HINT_SPACING_TICKS`, only on Deep Descent worlds, skipped when the rig already shows the lesson.
Reads the replicated world, so it works online, in split-screen and by touch.

### The descent probe's pacing check

`DescentStratum::markers` are each stratum's new things from the registry's real content
(`registry_markers`): its arrival, creatures met there first (the classic strata's band
roster), its signature rule, fluids and field physics, screen effects met first, landmarks
(set pieces always, other kinds the first time), story logs (`STORY_LOG_SOURCES`: the Descents'
story beats; **the story track appends its expedition-log source there**) and the workshop
recipe it opens. The bot adds what it really buys (tiers, seals, rig systems, workshop parts)
and the gate. `StratumRun::events` time them; `longest_gap_minutes` measures from the last new
thing before the stratum to the next one's arrival. `DescentRun::pacing_failures(3)` /
`pacing_passes(3)`: every stratum of Descents I-III waits at most `PACING_MAX_GAP_MINUTES` (40)
for something new, costs at most 2 deaths and has everyday trips within one tank. The bot now
buys the bay, tank and drill that make trips pay (about a tier per stratum), rates its seals per
Descent, dips past a gate before buying its system (`GATE_DIP_MINUTES`) and crafts parts from
the layer resources its trips dig (per stratum).

This branch's run (60 Hz, default seed, the Wave 7 registry content):

| Descent | Ends at (h) | §14 target (h) | Everyday trip fuel (max % of tank) | Deaths |
|---|---|---|---|---|
| I The Crust | 6.1 | 5-8 | 48 | 0 |
| II The Old Deep | 13.7 | 13-18 | 26 | 0 |
| III The Living Dark | 19.8 | 22-29 | 17 | 0 |
| (whole run, VIII The Heart) | 67.7 | 60-90 | 12 | 3 |

| Stratum | Minutes | New things | Creatures | Mechanics | Landmarks | Story | Upgrades (crafted) | Effects | Longest gap (min) | Check | Deaths | Everyday fuel % |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| I Topsoil | 83 | 12 | 0 | 1 | 3 | 1 | 6 (0) | 0 | 12 | pass | 0 | 16 |
| I Clay Beds | 38 | 5 | 2 | 1 | 0 | 0 | 1 (0) | 0 | 12 | pass | 0 | 28 |
| I Grey Stone | 98 | 12 | 2 | 1 | 2 | 0 | 6 (0) | 0 | 14 | pass | 0 | 48 |
| I Slate Shelves | 62 | 5 | 1 | 1 | 0 | 0 | 2 (1) | 0 | 15 | pass | 0 | 25 |
| I Basalt Columns | 83 | 12 | 1 | 1 | 1 | 0 | 7 (0) | 0 | 17 | pass | 0 | 31 |
| II Crystal Faults | 70 | 18 | 0 | 1 | 1 | 1 | 14 (1) | 0 | 14 | pass | 0 | 26 |
| II Fossil Oceans | 63 | 11 | 0 | 2 | 0 | 0 | 8 (0) | 0 | 16 | pass | 0 | 11 |
| II Pressure Cathedrals | 63 | 8 | 1 | 1 | 0 | 0 | 5 (0) | 0 | 16 | pass | 0 | 15 |
| II Radioactive Hollow | 69 | 11 | 1 | 1 | 0 | 0 | 7 (0) | 1 | 16 | pass | 0 | 13 |
| II Ancient Machine Layer | 57 | 3 | 0 | 1 | 0 | 0 | 1 (0) | 0 | 19 | pass | 0 | 11 |
| II Void Geode Fields | 64 | 5 | 0 | 1 | 0 | 0 | 3 (0) | 0 | 21 | pass | 0 | 11 |
| II Mantle Storm Zone | 71 | 9 | 1 | 2 | 1 | 0 | 3 (0) | 0 | 21 | pass | 0 | 11 |
| III Fungal Forest | 75 | 21 | 0 | 1 | 1 | 1 | 16 (11) | 1 | 13 | pass | 0 | 17 |
| III Root Labyrinth | 64 | 4 | 0 | 1 | 0 | 0 | 2 (0) | 0 | 21 | pass | 0 | 11 |
| III The Hive | 72 | 6 | 1 | 1 | 1 | 0 | 2 (0) | 0 | 21 | pass | 0 | 11 |
| III Flesh Caverns | 72 | 6 | 0 | 2 | 0 | 0 | 2 (0) | 1 | 14 | pass | 0 | 10 |
| III Glow Sea | 83 | 10 | 0 | 3 | 1 | 0 | 3 (0) | 1 | 14 | pass | 0 | 10 |

Descents I-III pass the check. What changed it: the bot used to buy only what the next stratum
demanded, so it never bought a bay or drill in the Crust, lost money on every trip below the
Clay Beds and took 14.2 h there (everyday trips 168 % of the tank); buying what makes trips pay
brings the Crust to 6.1 h with trips under half a tank. Open for the integrator: the Living Dark
ends about 2 h early because its strata still have Wave 7's placeholder creature tables (the
content tracks' 2-4 creatures each add about 15-30 content minutes a stratum); re-run the probe
after stacking them (`cargo test --release -p fatherload_simulation descent_report -- --ignored
--nocapture`). The deeper Descents run ahead of §14 until Waves 9-10 author them.

### Formats and seams

- `PROTOCOL_VERSION` 16 (the two commands, `ItemKind::Layer`, the two upgrade kinds, the
  `HazardGear` and `Logistics` fields) and save format 14 (formats 8-13 load as is; the
  every-format test covers format 13 without the new fields). The integrator collapses both.
- No new gameplay events and no audio patches (the dialog's own sounds and toasts).
- Shared-file edits (all additive): `economy` (appended upgrade kinds, `UpgradeLock::{Unfelt,
  NotStockedHere}`, `upgrade_lock_for`'s felt argument, `fit_next_tier`, `UpgradeKind::name`),
  `item` (`ItemKind::Layer`), `protection::HazardGear`, `logistics` (`LogisticsCommand`
  variants, `Logistics` fields), `world::{commands (dig extras, felt lock), movement (pressure
  burn), simulate (placeholder heat cap), progression (haul premium), drops}`,
  `worldgen::mod` (the new-world constants), and in the game the shop icons and lock tags,
  drops, the logistics dialog and HUD, the New World dialog, the lamp scale, a `SeatUi` field
  and its tracking call, the save/protocol docs and the sprite atlas (appended at the end).

## Wave 8 crust: Descent I authored (`src/crust.rs`, `src/world/crust.rs`, `src/worldgen/descent/crust.rs`, `src/worldgen/streamed/crust_pass.rs`)

Descent I of a Deep Descent world (generator V11) is authored for real: every Crust stratum has
its own rules, materials, ores, creatures, landmark, music and story beat, and the Descent ends
at the Sinkhole. Nothing here touches V1-V10 (`crust::crust_rules` is `has_descents()`): their
golden digests, hashes and replays are unchanged.

### The strata

| Stratum | Signature rule | Materials and ores | Natives | Landmark | Music / ambience | Log |
|---|---|---|---|---|---|---|
| Topsoil | rain-soaked **mud** (`Surface(Mud)`): the drive bogs down to 45 % of top speed and half its push, the thrusters fight the suction (`ground_grip`, `MUD_*`) | root mats under the surface, mud lenses and mud pools on cave floors, burrows (thicker in "Burrow Warren", muddier in "Rain Mud"); copper, iron, silver; density 1150 | burrow mole, mud leech | the lost miner's cabin (`crust::cabin`: log walls, his strongbox, an old entrance shaft of rubble from the surface) | `Topsoil` / `Burrows` | 0 "The cabin" |
| Clay Beds | **aquifers**: sealed water pockets (the registry's `FluidPocketRule::sealed(Water, 260)`, radius 3-6, plus cave pools) that flood the tunnel that breaches them (the Wave 7 fluids); rock beside one shows damp stains | clay pots (`ArtifactKind::ClayPot`, 70 cr; more in "Potters' Beds"); iron, silver, gold; density 1100 | potter wasp, mud leech | (aquifer seams) | `ClayBeds` / `Aquifer` | 1 "The clay" |
| Grey Stone | **old mine works** (`worldgen::descent::crust::mine_works`, one per 128x44 region in three of five): a timbered gallery on a rail line, a stope with a strongbox, a ramp down through a junction to a lower gallery. Rails are ridden (below), timber sets fail when cut or undercut, first gas pockets (gas weight 6) | `CrustTile::{Rail, Timber}`, gravel; silver, gold, a little emerald; density 1050 | mine bat, timber beetle | **Gravel Hollow** and the Descent I outpost | `GreyStone` / `MineWorks` | 2 "The works" |
| Slate Shelves | **slate slides**: slick shelves (`CrustTile::Slate`, long bands two rows thick) keep a rig sliding; a shelf left hanging over an opened tile shudders and slides down (`SLIDE_*`) | stalactite galleries (`CaveTile::Stalactite` under cave ceilings, every other one in "Stalactite Galleries"); gold, emerald; density 1000 | slate skink, shelf crab | (stalactite galleries) | `SlateShelves` / `SlateDrip` | 3 "The shelves" |
| Basalt Columns | **lava tubes** (`crust::lava_tube_cell`: one 70-150 column tube per 160x34 cell in two of three, gently waving, lined with tube glass, rare lava pools in the floor) and **the first heat**: 3 heat units (a tier 2 radiator), a tube one level more (`LAVA_TUBE_HEAT_UNITS`) | basalt split into columns (`CrustTile::BasaltColumn`, easier than hard rock), tube glass; emerald, ruby, a little diamond; density 1000 | tube crawler (spits cinders), cinder moth | the Sinkhole (gate) | `BasaltColumns` / `LavaTube` | 4 "The tubes", 5 "The Sinkhole" |

`Stratum::uses_registry_rules` (new) is true for the Crust and every stratum below Descent II,
so V11 reads the Crust's registry ores, hazards and `StratumDef::ore_density_permille` (new,
1000 by default; it scales the richness of veins and clusters, `Ctx::ore_richness`). The Old
Deep keeps the classic curves until its track authors it. The renderer keeps the classic band
art for the Crust's base rock; the new tiles bring the look (`game/src/rendering/terrain/
tile_art/crust_art.rs`: wet mud, root mats, adzed timber with iron straps, steel rails on
sleepers, slate sheets with a wet sheen, hexagonal basalt, ropy tube glass, cabin logs, plank
walls and lamps inside the cabin and the camp, timber posts and lanterns behind a gallery, the
cracked glow of a tube's walls).

### Mechanics (`world/crust.rs`, all in `simulate_tick`)

- **Mine carts** (`crust::CartRide`, `Player::cart`, per tick in `PlayerRig` / `PlayerTickState`
  so it is predicted and reconciled like the pose). A rig resting on a rail with the stick held
  along it (not thrusting or drilling) boards. Each tick runs the fixed `CART_STEPS_PER_SECOND`
  (240) integer steps due by the tick (`cart_steps_by`), so rides take the same world time at
  every tick rate: the stick pushes (`CART_PUSH`) or brakes (braking past a stop turns the cart
  round), friction slows, ramps pull (`CART_SLOPE`), top speed 10 tiles/s. At a junction the
  stick held down takes the lower branch (`next_rail`, `is_junction`; the branch dives under the
  main line's rail). Thrust jumps off with the cart's speed; the end of the line, or rubble on
  it, ends the ride. Rides burn no fuel. While riding the rig neither drills nor flies
  (`simulate_tick` skips both, as for lift rides). Events `CartBoarded`, `CartSwitched`,
  `CartLeft`.
- **Timber sets**: a tile opened beside a beam, or straight under one through open ground (the
  floor under a set's posts), dooms the set: `SupportCreaked` now, and after
  `SUPPORT_CREAK_MILLIS` (1.6 s) the beam and up to `SUPPORT_SPAN` columns either side come
  down with `SUPPORT_ROOF_ROWS` of roof as boulders, the fallen roof lying as rubble on the
  gallery floor (never over a rig): `SupportCollapsed`. Timber beetles chew through sets (below).
- **Slate slides**: a tile opened under a shelf: `SlateShudder`, then after
  `SLIDE_WARNING_MILLIS` every slate tile of that shelf (up to `SLIDE_MAX_TILES`) left over
  open ground falls as a boulder: `SlateSlid`.
- **Heat per stratum** (`crust::stratum_heat_units`, read by the heat damage through
  `WorldState::heat_units_at`): the Crust as above; every other stratum on a gate-appropriate
  curve, cool for a rig at the previous Descent's tier cap at the top of a Descent and climbing
  to just under what the Descent's own cap covers (the Wave 7 issue where placeholders borrowed
  their host band's heat and cooked a stock rig is gone). Older worlds keep band heat bit for
  bit. `LavaTubeEntered` once per entry.
- **Strongboxes** (the cabin, stopes, the Sinkhole and the Fathers' camp): drilled open they pay
  `STRONGBOX_CREDITS` (double the first time) and read the stratum's expedition log
  (`crust::CRUST_LOGS`, the Fathers' expedition thread: "the world was shaped"):
  `StrongboxOpened { log, first }`; `CrustState::logs` keeps the logs read.
- **The Deep Claim gate**: on a Descent world a rig below Descent I without the crew's Deep
  Claim is warned once a trip (`DeepClaimMissing`) and takes the gate crush
  (`GATE_DAMAGE_PER_SECOND`); where a deeper Descent's own system is missing, that gate's crush
  applies instead. `WorldState::grant_deep_claim` gives it without the set piece (tests, tools,
  scenes that start deeper).

### The Sinkhole (the gate set piece)

`crust::sinkhole_site`: Descent I's gate chamber is the core chamber (the core,
`CrustTile::SinkholeCore`, glowing in the floor, the Fathers' strongbox beside it, the middle
cleared of pillars); a funnel of crumbling rubble walls rises 64 rows from its ceiling to the
lip; the Fathers' timbered adit runs east out of the funnel's top past the collapse zone to
their log-walled camp.

Drilling the core out starts the run (`SinkholeRun`, in `CrustState`): `SinkholeRumble`, then
after `SINKHOLE_RUMBLE_MILLIS` (3 s) the collapse front climbs `SINKHOLE_FRONT_ROWS_PER_SECOND`
(3 rows/s; a stock rig climbs about 6-7, a laden one more than 5) filling every open tile of
the zone with rubble and shaking boulders off the walls a few rows above it. A rig that gets
above the lip, or into the adit past the funnel's wall (which the collapse never fills,
`SinkholeSite::collapses`), is clear (`SinkholeEscaped`); slipping out of the zone's side down
at the chamber or under it is not (`SinkholeSite::clear_of`). A rig the front
reaches is dug out and set down in the camp with `SINKHOLE_CAUGHT_HULL_SHARE` of its hull lost,
never wrecked (`SinkholeCaught`). When the front tops out: if anyone got clear the crew holds
the **Deep Claim** and every rig gets **Drill II** (drill tier at least `DRILL_II_LEVEL`, 7):
`DeepClaimGranted` (a rig joining the world later gets its Drill II on its first tick); if
nobody did, the zone is put back as generated, core and all, for another
try (`SinkholeReset`). No countdown can be lost by waiting: the front only moves once the core
is out, and its speed is a fixed, fair share of the slowest rig's climb: a stock engine with
a half-full bay and the tier 2 radiator the Basalt Columns ask for still gets out
(`a_stock_engine_rig_with_half_a_bay_still_outclimbs_the_sinkhole`; review fix: the adit
inside the zone used to fill too, and the 30-odd tiles along it at a stock engine's 2.25
tiles/s lost the race).

### Gravel Hollow

`crust::gravel_hollow`: a Deep Descent world's Grey Stone holds Gravel Hollow, the underground
settlement of the town's own site 0 (`placement::underground_id(0)`, which no older world
places), a village hall beside the Descent I outpost (sharing its floor, a gravel lane to its
door): Deepfolk miners with their market, inn, houses and specialty, people walking their
routines, rumours and story seeds, all from the existing village machinery
(`worldgen::streamed::villages::gravel_hollow_hall`; every village lookup finds it). The
outpost's counter does the fuel, repairs and selling.

### Creatures (`enemy.rs`, `world/enemies.rs`)

Nine new kinds appended to `EnemyKind` (all follow the body rules: `step_body`,
`free_from_rock`, `keep_off_bodies`, `site_fits`, contact boxes sized to their sprites, line of
sight): **burrow mole** (burrows after the drill, rare digging, lunges from the wall), **mud
leech** (slow; its bite barely dents the hull but drinks `MUD_LEECH_FUEL_SIP` fuel: `CrustCreature
{ LeechLatched }`), **potter wasp** (fast swarmer, drops a clay pot now and then), **mine bat**
(colony bat of the works), **timber beetle** (at rest it chews through a timber set over it,
which then fails like a cut beam: `CrustCreature { BeetleChewed }`), **slate skink** (fast,
long lunges), **shelf crab** (armoured under its slate plate, a heavy pinch), **tube crawler**
(spits cinders, fire-proof), **cinder moth** (drawn to heat and lamps, fire-proof). Each Crust
stratum's table lists two natives and one classic kind; cave habitats favour them. Sprites in
`game/src/rendering/pixel_art/sprites/enemies.rs`.

### Pacing (the descent probe)

`balance::descent::RegistryStrata` now reads each stratum's real heat (radiator tier from
`stratum_heat_units`), counts the Crust's signature rules as mechanics, and gives the bot
Gravel Hollow's outpost as Descent I's base once claimed (`StratumRegistry::outpost_row`, new;
the plan keeps its rule) and Drill II at the Sinkhole. With the Crust's richer ores (the Clay
Beds' silver and gold above all): **Descent I ends at 6.1 h** (target 5-8 h; was 14.3 h) with
everyday trips at most **64 % of a tank** (was 168 %), every Crust stratum 54-98 minutes and no
gap between new things over 14 minutes there. `the_authored_crust_takes_five_to_eight_hours_on_sane_trips`
holds it. Whole run 65.7 h; with heat per stratum the Descents below run ahead of §14 (Living
Dark ends at 18.5 h against 22-29): their tracks' authored strata and gates should bring them
back (note for the integrator).

| Descent | Ends at (h) | Target (h) | Everyday trip fuel (max % of tank) |
|---|---|---|---|
| I The Crust | 6.1 | 5-8 | 64 |
| II The Old Deep | 13.1 | 13-18 | 15 |

### Audio (appended at the end of `patches!`)

`cart_clunk`, `cart_switch`, `timber_creak`, `roof_collapse`, `sinkhole_rumble`,
`deep_claim_fanfare`, `tube_breath`, and the loops `loop_crust_topsoil` (a fingerpicked G
pentatonic line over a soft drone and creaking roots), `loop_crust_clay` (a D dorian marimba,
gurgles and drips), `loop_crust_works` (a reedy E drone, a slow work song, far pick clinks),
`loop_crust_slate` (a glassy pad in fifths and ringing drips), `loop_crust_basalt` (a hot A
drone with a sour second, a slow heave, crackle) and `loop_sinkhole` (the chase: toms, a
climbing ostinato, brass stabs and the roar). The director plays the bed of the Crust stratum a
seat digs in (`SeatLoop::CrustBed`, appended) and the chase for every seat near the Sinkhole
while it collapses (`SeatLoop::Sinkhole`); the registry's `MusicCue` / `AmbienceCue` gained the
Crust's own entries (appended).

### Seams and formats

- **Tiles**: `TileKind::Crust(CrustTile)` (code range `CRUST_TILE_CODE_BASE` = `0xC00`),
  `ArtifactKind::ClayPot` (appended). Their art, colours, minimap and drill sounds are wired.
- **State**: `CrustState` (`WorldState::crust`, serde default, hashed only once non-empty,
  replicated as the appended `SectionKey::Crust`); `Player::cart` per tick.
- **Events 400-415** (netcode codes): `CartBoarded`, `CartSwitched`, `CartLeft`,
  `SupportCreaked`, `SupportCollapsed`, `SlateShudder`, `SlateSlid`, `StrongboxOpened`,
  `SinkholeRumble`, `SinkholeEscaped`, `SinkholeCaught`, `SinkholeReset`, `DeepClaimGranted`,
  `DeepClaimMissing`, `LavaTubeEntered`, `CrustCreature` (416-419 free).
- **Formats**: `PROTOCOL_VERSION` 16 (once for this track). No save format bump: every new
  field has a serde default and older saves load as before.
- **Generation**: `GOLDEN_V11_DIGEST` updated (the Crust's features, Gravel Hollow, the
  Sinkhole); V1-V10 digests unchanged.
- **Input**: no new action or prompt (carts ride on the stick and thrust; junctions on drill
  down), so keyboard, gamepad, touch and split-screen seats all play it.
- **Shared-file edits** (additive): the registry's Crust entries and `StratumDef` fields,
  `simulate_tick` (the cart branch beside the lift-ride skip, ground grip, the heat read, two
  mined-tile arms, `advance_crust` after logistics), `rescue` (drops the cart), the enemy
  tables, the village lookups (Gravel Hollow), the descent probe, the enemy body test's spacing,
  the audio quality test's low-centroid list.
- **Tests**: `crust::tests` (codes, rails, cart physics, heat curve),
  `worldgen::descent::crust_tests` (every stratum's signature generated, mine works and their
  junction, the cabin and the Sinkhole stamped, long lava tubes, no Crust tiles before V11),
  `world::crust_tests` (cart rides with a junction and without fuel, main line and jumping off,
  the same ride time at 30/60/120 Hz, mud and slate grip, a set collapsing after its creak, a
  shelf sliding, an aquifer flooding the tunnel, the tubes' heat, strongbox logs, the Deep
  Claim gate, a bot winning the Sinkhole, a rig caught and the site resetting, every stratum's
  natives, the leech's fuel bite, the beetle bringing a set down, Gravel Hollow peopled beside
  the outpost), `balance::descent` (Descent I in 5-8 h).
- **Scenes**: `crust_topsoil`, `crust_cabin`, `crust_clay_beds`, `crust_aquifer_flood`,
  `crust_grey_stone`, `crust_cart_ride`, `crust_support_collapse`, `crust_gravel_hollow`,
  `crust_slate_shelves`, `crust_lava_tube`, `crust_creatures`, `crust_sinkhole`,
  `crust_sinkhole_escape`.

### Review fixes (Wave 8 crust review)

- **The Sinkhole is winnable on a stock engine**: the adit past the funnel's wall no longer
  fills (`SinkholeSite::collapses`), so the race is the climb, not a 30-tile drive at 2.25
  tiles/s through the zone; leaving the zone's side below the adit or under the chamber is no
  longer an escape (`clear_of`). Rigs that join after the Sinkhole get Drill II.
- **Heat everywhere reads `crust::heat_units`**: the rig's damage (as before), a pilot on foot
  (it still read the host band's heat) and the game's heat tint and HEAT badge (they read band
  heat / classic depth, so they lit in cool V11 strata). Older worlds are unchanged.
- Scenes: `crust_sinkhole_escape` now shows the collapse rising under the rig;
  `crust_lava_tube` stands mid-tube.

### Still open

- The shop's radiator preview (`upgrade_preview`) still names the classic band a tier covers,
  not V11 strata.
- No screen shake or flash for the Sinkhole's rumble or a roof collapse (only sound).
- The Crust keeps the classic band art for its base rock (the registry palettes describe it);
  only the new tiles and backdrops are new art.
- Junction branches dive under the main line's rail for a tile (a rig on the branch overlaps it
  briefly; a ride that ends there is lifted free).
- A collapse's rubble lies only on the gallery floor row; boulders do the rest.
- The expedition logs are the Crust's own (`CRUST_LOGS`); the story track's log system can
  adopt them.

## Wave 8 old deep: Descent II, the Old Deep (`src/old_deep.rs`, `src/old_deep/{sites,refraction,fields}.rs`, `src/world/old_deep.rs`)

Descent II of a V11 world (rows about 1,100-2,700, the seven classic deep strata, each about
230 rows) now has authored content: every stratum has a signature rule introduced gently and
then pushed, its own materials (`TileKind::Deep(DeepTile)`, codes `0xD00`), a layer resource,
native creatures, a landmark and a Fathers' expedition log; the Descent ends at the Star Core
boss, whose defeat grants the Pressure Hull. Classic worlds never see any of it: only V11
generation places `DeepTile`s, every rule reads only them or the V11 layout, and
`advance_old_deep` returns at once in a world without Descents (V1-V10 digests and hashes are
unchanged; `GOLDEN_V11_DIGEST` is updated).

### Generation (`old_deep::sites`, called from `assemble` after the landmarks, before the cable)

- `carve_tile(settings, layout, x, y, current)` is a pure per-tile function. Each Descent II
  stratum lays a region grid (`region_size`); a region whose centre row is in that stratum may
  hold one feature drawn wholly inside it, and a tile asks only its stratum's and its two
  neighbours' regions, so chunks carve identically in any order. Seed variants
  (`descent::variant`) shift each stratum's feature mix ("Singing Faults" more faults, "Prism
  Seams" more grottos, and so on). Features replace only rock, ore, hazards and soft cave
  dressing (`carvable`), never built pieces, ruins, caches, POIs or fluid.
- Features: crystal faults (resonant crystal two wide with prisms at the ends and latent gems
  behind) and prism grottos (Crystal Faults); fossil skeletons from three templates in shale
  beds, amber blobs and gas basins (Fossil Oceans); grand naves with pillars and jet vents
  (Pressure Cathedrals); glow pits ringed with glow rock and isotopes, some with a mutation
  relic (Radioactive Hollow); machine halls with belt floors, piston ceilings, doors and a
  builder tablet (Ancient Machine Layer); void-glass geodes with blooms (Void Geode Fields);
  storm-vent pockets in cinder rock (Mantle Storm Zone); plus scattered singles.
- Set pieces, once per world: the **Cathedral Lift nave** (`cathedral`: a 54x28 vaulted nave
  in the middle of the Pressure Cathedrals with two ledges, three `LiftValve`s and climbing
  jets, centred on `cathedral_lift_column`, the column outside Cathedral Station's far door);
  the **Glowmother's den** (`glowmother_den`); a **Fathers' camp** per stratum
  (`expedition_camp`: a plated niche with an `ExpeditionLog`); and the **Star Core arena**
  (`star_core_arena`, built over Descent II's gate chamber: indestructible `CoreWall` sides and
  floor, a diggable cinder roof, the shielded core on a pedestal, two floor pylons and two on
  side platforms).
- `landmarks::logistics_sites` appends the **Cathedral Lift** as a ninth ancient lift, from
  Cathedral Station's floor to the nave's floor (stations only at its ends).
- `fields::geode_field_source` is registered first in `field::STRATUM_FIELD_SOURCES`: inside a
  geode's hollow the pull is `GEODE_GRAVITY` (0.32 g) pointing out from the heart to the shell,
  drag drops to 0.35, rotation is free, a light wobble and violet grade show; it blends the
  stratum source's field underneath and answers `None` everywhere else.

### Rules (`world/old_deep.rs`; all timing from world milliseconds, so every tick rate agrees)

| Stratum | Rule | Counter-play |
|---|---|---|
| Crystal Faults | **Resonance**: breaking resonant crystal shatters the connected crystal within `resonance_reach(drill)` (1 tile for a slow drill, up to 4); 5+ shards without a Resonance Drill shake a cave-in loose. **Refraction** (`refraction::refract`, pure): a prism the headlamp reaches (7 tiles, clear line through air or crystal) splits the lamp into three beams (straight on and 45 degrees each side) running 14 tiles through air and crystal; tiles along them are lit and latent gems (plain-stone look, Diamond when broken) show. `WorldState::refraction_for(player)`. | The Resonance Drill harvests the shards (crystal lenses drop); slower drills contain chains; prisms find the gems. |
| Fossil Oceans | **Gas tides** (`gas_tide`, 90 s cycle, high 30-55 s): at high tide gas pockets under open air within reach breathe a gas cloud once per tide. **Amber**: a rig pressed against amber is held to `AMBER_SPEED_CAP`. **Skeletons**: a bone taken with at most one rock side left (`bone_exposed`) comes out whole (a fossil artifact), else it crumbles (no drop); a whole skeleton pays `SKELETON_BOUNTY_PER_BONE` per bone. | Watch the tide panel; drill amber away; dig round bones first. |
| Pressure Cathedrals | **Pressure jets** (`jet_active`, 4.2 s period, 1.5 s blast, a 0.7 s steam tell; vents out of step by column) throw a rig in the column above upward. **The Cathedral Lift**: holding a valve 3 s (`VALVE_HOLD_MILLIS`, within reach) opens it; while any valve is held every jet in the nave is calm (co-op: one holds, one climbs); three valves restore the lift (`CathedralLiftRestored`). | Time the jets, or ride them to the ledges. |
| Radioactive Hollow | **The dose meter** (`Dose`, 1 Hz): +4/s in the hollow, +2 per glow rock and +4 per isotope or relic within 3 tiles (up to +18); steps at 400 / 700 / 1000 (`DoseWarning`) cost fuel (20 % / 45 % of a unit a second), blind the scanner and finally hull; it drains 6/s outside (18/s near an outpost). **Mutation relics** grant one of `Mutation::{Lumen, LeadSkin, Regrowth, FuelGland}` for 5 minutes (+150 dose). | `dose_shield_percent`: gas scrubber tiers 15 % each, the Pressure Hull part 30 %, Lead Skin 75 %. Short dives. |
| Ancient Machine Layer | **Machinery** wakes as a rig comes within 8 tiles of a hall (`MachineryWoke`, `woken_halls`): belts carry resting rigs at `CONVEYOR_SPEED`, pistons slam (`piston_extension`, 3.6 s) and strike a rig a ram lands on (`PistonStruck`, 14 hull, shoved down). Builder tablets tell the builders' story. | Time the rams; bombs break housings. |
| Void Geode Fields | **Geode gravity** (the field source above): rigs float and turn to land on the walls. | Rotation thrusters; cargo weight. |
| Mantle Storm Zone | **Heat storms** (`storm_phase`, 150 s: calm, 15 s gathering, 30 s raging): a rig in open cave air (`STORM_EXPOSED_OPEN_TILES` of the 7x7 round it open) loses `STORM_DAMAGE_PER_SECOND`; storm vents near rigs pour molten metal (5 cells each storm, real fluid). | Dig in (a tight tunnel shelters); the Thermal Hull halves it, heat shield tiers cut it. |

**The Star Core** (the gate, `advance_star_core`): dormant until a rig enters the arena; then
shielded while any pylon stands, throwing flare fans (fireball shots, faster and wider as it
weakens) and sparks (magma wisps, up to 3). Breaking all four pylons opens the core for
`CORE_EXPOSED_MILLIS` (16 s): drilling it out is a strike (`CORE_STRIKES` = 3); a strike or the
window closing relights every pylon. The third strike spends the core (`FirstCoreSpent`),
breaches the arena floor, grants `RigSystem::PressureHull` to every rig, and reads the reveal
log (`LOG_CORE_REVEAL`: the core was a lock, not the bottom). No countdown: an arena empty for
10 s puts an unfinished fight back to sleep, whole. **The Glowmother** (mini-boss): wakes when a
rig nears her den, births glow grubs (Spawner), and her fall is remembered.

**Creatures** (appended `EnemyKind`s, all on the generic body rules: `step_body`,
`free_from_rock`, `keep_off_bodies`, `site_fits`, contact boxes sized to the sprites, sight and
hearing): Prism Moth (flying swooper drawn from far off), Fossil Eel (burrowing lunger through
shale), Jet Ray (broad swooper), Glow Grub (walking pack biter), the Glowmother (spawner
mini-boss), Piston Warden (machine lunger), Void Jelly (slow flying bolt-spitter), Magma
Salamander (walking fire-spitter). The registry's Descent II creature tables mix each new kind
with two classic kinds (V11 only; classic worlds keep `band_roster`); each drops its layer
resource now and then. The registry's signatures and layer resource names (Resonant Shards,
Amber Resin, Compressed Basalt, Glow Isotopes, Machine Alloy, Void Glass, Mantle Fragments)
are updated.

### State, events and formats

- `WorldState::old_deep: OldDeep` (serde default; hashed only when not empty; the appended
  shared section `SectionKey::OldDeep`): doses, mutations, woken halls, skeleton digs, the
  Cathedral Lift, the Star Core fight, the Glowmother, logs read, storm-vent pours and tide
  breaths. `OldDeep::forget` runs when a player leaves. Doses change once a second (the
  section is not resent every tick).
- Events 420-438 (`ResonanceChain`, `FossilBoneFreed`, `SkeletonCompleted`, `GasTideTurned`,
  `AmberStuck`, `CathedralValveOpened`, `CathedralLiftRestored`, `DoseWarning`,
  `MutationGained`, `MachineryWoke`, `PistonStruck`, `HeatStormChanged`, `LavaPoured`,
  `DeepLogRead`, `StarCoreAwoke`, `StarCorePhase`, `StarCoreBeaten`, `GlowmotherAwoke`,
  `GlowmotherDefeated`); 439 is free.
- `PROTOCOL_VERSION` 16 (one bump: the tiles, creatures, section and events). No save format
  bump: every new field has a serde default.
- Shared-file edits (all additive): `TileKind::Deep` arms in `terrain.rs`, persistence,
  `item.rs`, `commands.rs` (chip seconds); the `assemble` hook; `field::STRATUM_FIELD_SOURCES`
  (the geode source first); `landmarks::logistics_sites` (the Cathedral Lift);
  `simulate_tick` (`advance_old_deep` after `advance_logistics`, the mined-tile arm); the drill
  drop guard (`deep_drop_allowed`); `hatch_brood` (the Glowmother's brood); the registry's
  Descent II entries; `balance::descent::RegistryStrata` (counts `old_deep::authored_content`).

### Game side

Tile art (`rendering/terrain/tile_art/deep_art.rs`: faceted crystal and void glass, banded
shale, honey amber, ivory bones, carved nave ashlar, jet grates, valve wheels, glow veins,
isotope spears, the relic, riveted casings, chevroned belts, hazard-striped housings, glyph
tablets, cinder, the core wall, pylons, the core and the Fathers' crate); creature sprites
(`pixel_art/sprites/old_deep.rs`); the world layer (`rendering/world/old_deep.rs`: glows,
jet plumes and tells, rams, belt motion, valve-hold rings, the core's shield and pylon beams,
each seat's own refracted beams and lit gems drawn after the darkness pass, with the beams also
lighting the cave darkness, and storm embers); the per-seat panel and screen effects
(`rendering/old_deep_hud.rs`: dose meter and mutation, storm clock, gas tide, valves, the
Star Core's phase and strikes; the dose's green grade and vignette and the storm's haze go
through the field-effect stack, so the accessibility setting and photosensitivity limits apply;
nothing flashes); news, fanfares and the story logs' text (`local_game/old_deep_news.rs`,
`FanfareKind::OldDeep`); audio (appended at the end of `patches!`: seven stratum beds, the Star
Core theme, the geiger crackle that follows the dose, and nine one-shots; every new event has a
cue). Touch and split screen need nothing new: every interaction is drilling, flying or standing
near something. Scenes: `old_deep_crystal_faults`, `old_deep_fossil_oceans`,
`old_deep_pressure_cathedrals`, `old_deep_cathedral_lift`, `old_deep_radioactive_hollow`,
`old_deep_glowmother`, `old_deep_machine_layer`, `old_deep_void_geodes`, `old_deep_mantle_storm`,
`old_deep_star_core`, `old_deep_star_core_exposed`, `old_deep_fathers_log`.

### Tests and the probe

`old_deep::tests` (codes, tick-rate-safe cycles, tides, storms, pistons, dose),
`old_deep::refraction::tests`, `old_deep::fields::tests`, `old_deep::sites::tests` (every
stratum's materials, the set pieces where the tick looks for them, none in older worlds),
`world::old_deep_tests` (classic worlds untouched; resonance reach and harvest; refraction and
Lumen; bones whole or crumbled and a skeleton's bounty; amber; the gas tide; jets at 30 and 60 Hz;
three held valves restore the Cathedral Lift; dose, gear and drain; a mutation relic; a hall
waking and its belt carrying a rig; a storm scorching an open cave but not a dug-in rig; **a bot
with Descent II kit flies the arena, breaks the pylons and strikes the core three times**,
winning the Pressure Hull, with the Descent III gate crushing the rig before and letting it pass
after; an emptied arena puts the core back to sleep; geode gravity in the world; the descent
probe). The enemy body tests now hold the eight new kinds too.

Descent probe (`--balance-report --only descent`, default seed): Descent II takes **8.9 h**
(14.3 h to 23.2 h; §14's span is 7-16 h), every Old Deep stratum's longest gap between new
things is 9 minutes or less, no deaths. Descent I still ends at 14.3 h on the registry's Crust
(the Crust track's re-pacing answers that); after a 7 h Crust, Descent II ends about 15.9 h in
(target 13-18 h). `world::old_deep_tests::the_descent_probe_paces_the_old_deep_like_the_plan`
holds the span and the gaps.

### Seams and known limits (for the integrator and later waves)

- Cathedral Station has fuel, repair, sale, dive charges, claim and respawn (Wave 7 outposts);
  it has no resident people yet (the people system's settlements are surface-only): a later
  wave can seat a camp there.
- `RestoreLift` can still pay to restore the Cathedral Lift from its bottom station; the valves
  are the free way.
- Flares use `ShotKind::Fireball` with `shooter: MagmaWisp`; the core is a tile set piece, not
  an entity (no health bar entity; the panel shows its strikes).
- Pistons push and strike but do not write terrain (the ram is a hazard volume drawn live).
- Code families: `0xD00` for `DeepTile`; if another Wave 8 track chose the same family, one
  of us moves.

### Review notes (Wave 8 old deep review)

- Fixed: the tuple-keyed `OldDeep::skeletons` and `OldDeep::pours` maps are saved as lists of
  pairs (`pair_map`, `storm_pours`). Saves are JSON, and JSON object keys must be strings, so
  any V11 save failed once a bone was dug or a storm vent poured
  (`the_old_deep_state_survives_json_saves_and_the_wire`).
- Fixed: a rig riding a lift keeps its dose (riders are left out of the rules' rigs, and the
  dose pass used to drop every meter not in that list).
- Fixed: a glow grub's bite raises the bitten rig's dose by `GLOW_GRUB_BITE_DOSE`, as its
  description says (`a_glow_grubs_bite_raises_the_dose_and_a_lift_ride_keeps_it`).
- Fixed: the Glowmother's fall counts when a rig is within `GLOWMOTHER_WITNESS_TILES` of her
  den, not only inside its mouth. That range stays inside the director's despawn reach, so a
  despawn never counts as a kill.
- Still weak: the new creatures are new stats and sprites on the existing behaviour styles
  (swoop, lunge, contact, ranged, spawner). The Prism Moth is not drawn to beams and the Jet
  Ray does not ride jets. The storm's screen haze shows to a sheltered rig too. Bones still
  read as abstract shapes rather than a skeleton. The Star Core is a single tile on a
  pedestal, which looks small for a gate boss. The Glowmother is barely bigger than the rig.
- Pre-existing, seen while testing: the depth reward (`DeepRewardGranted`) drops its ore on
  the tile right of a rig, which can block a creature's line of sight in a staged test room.

## Wave 8 Living Dark: Descent III authored (`src/living_dark.rs`, `src/world/living_dark.rs`, `src/world/enemies/living.rs`, `src/worldgen/descent/living.rs`)

The Living Dark (rows about 2,700-4,200 of a V11 world, §14 Descent III) stops being a
placeholder. Biology takes over: each stratum's walls are alive in their own way, its signature
rule shows up gently near its top and is pushed by its bottom, it has its own rock, palette,
ores, layer resource, creatures, set pieces, music and ambience, and the Descent ends at the
Mycelial Mind, whose Bio-Filter and Lamp II open Descent IV.

### The strata (registry, `descent/registry.rs`)

| Stratum | Signature rule (pushed deeper) | Creatures | Set piece | Ores, layer resource | Music, ambience |
|---|---|---|---|---|---|
| Fungal Forest | giant mushroom caps bounce rigs; spore pods (and puffers' lobs) burst into spore clouds that blur and colour-shift the screen and sting a hull; its field's blur and grade deepen (`deepening`) | spore puffer, cap crawler, glow gnats | cap groves (more and taller deeper) | silver, emerald, platinum; Spore Silk | `FungalForest` / `Spores` |
| Root Labyrinth | every tile dug grows back as root (150 s at the top, 75 s at the bottom, jittered so a tunnel closes tile by tile); root axes and fire keep paths open | root snapper, bark beetle, thorn wisp | heartroots (a taproot with a diamond knot) | gold, ruby, diamond; Heartwood | `RootLabyrinth` / `Roots` |
| The Hive | two factions: the Amber Court (friendly: royal jelly, errands) and the Swarm (soldier packs, a brood matriarch); stealing from the Court sours it | Swarm soldier, Swarm lancer, Court drone; mini-boss the brood matriarch | the Amber Court's hall, brood galleries, the matriarch's gallery | gold, platinum, uranium; Royal Jelly | `Hive` / `Hive` |
| Flesh Caverns | its rock is flesh and heals back (40 s top, 20 s bottom); gullets' acid tides rise and fall on a 90 s cycle; a heartbeat (quicker deeper and while the tide rises) drives the music and the walls' pulse | maw worm (bores flesh), bile polyp (buds leechlings), leechling | gullets with acid glands | ruby, diamond, uranium; Living Sinew | `FleshCaverns` (heartbeat) / `Heartbeat` |
| Glow Sea | near-total darkness (field light 60 per mille): the only light is creatures', the rig lamp a candle (`GLOW_SEA_LAMP_SCALE` 0.3) until Lamp II | lure fish, glow jelly, angler (a light on a rod as bait) | abyssal halls (water pooled in them), the Mind's hollow | diamond, platinum, mythril; Lure Light | `GlowSea` / `Abyss` |

`MusicCue`, `AmbienceCue` and `LandmarkKind::LivingDark` (generator `LandmarkGenerator::LivingDark`)
are appended; the Flesh Caverns' rock is `TileKind::Cave(CaveTile::Flesh)` (counted as plain rock
by the deposit and hazard passes, `streamed::base::is_plain_rock`, so ore sits in flesh).

### Generation (V11 only; `worldgen/descent/living.rs`)

Run by `streamed::assemble` after the Descent landmarks and before the logistics sites, a pure
function of the settings and the tile (chunk order never matters), only over chunks near the
Living Dark's rows:

- **Stamps**: at most one per `LIVING_REGION` (56 x 40) cell whose centre is in a living stratum,
  rolled more often deeper (`base + extra * along`), kept clear of stratum edges, the fixed
  stamps and the outposts' column: cap groves, heartroots, brood galleries, gullets
  (`LivingStamp::gullet_glands`), abyssal halls. Fixed per world: the **Amber Court**
  (`court_hall`: a 64 x 18 comb hall with arches, a royal larder and doors, 70-120 columns from
  the spawn on the side away from the outposts), the **matriarch's gallery**
  (`matriarch_gallery`, past the Court and deeper) and the **Mind's hollow** (`mind_hollow`,
  `mind_floor_row`: Descent III's gate chamber overgrown into a mycelium cavern round the
  builders' pillars). Only plain rock, ore and hazards are carved.
- **Living rock** (`living_rock`): mycelium veins in the Fungal Forest, wandering root lines in
  the Root Labyrinth (denser deeper; vertical taproots, three times as often in the "Taproot
  Shafts" variant), comb seams in the Hive.
- Seed variants pick densities (Cap Canopy groves, Brood Galleries, Acid Gullets, Angler Deeps).
- New `CaveTile`s (appended, codes 15-21): `Root`, `Flesh`, `Comb`, `JellyCell`, `SporePod`,
  `AcidGland`, `CharredRoot`.
- **`GOLDEN_V11_DIGEST` changed** (now `0xf7eb_a7d0_2a09_fff0`); the V1-V10 golden digests are
  unchanged (nothing here runs before V11).

### Mechanics (`world/living_dark.rs`, after the late entity phase, before the fluid pass)

- **Openings**: the terrain journal now keeps the kind each write replaced
  (`Terrain::journal_with_before`, never serialized), so the tick tells a dug root, a burst spore
  pod or a theft from the Court apart from any other write. Openings in the Root Labyrinth and
  Flesh Caverns are scheduled to grow back (`LivingSchedule`), unless scorched or inside an
  outpost; the regrowth runs at most `REGROW_BUDGET` (24) tiles a tick and waits
  `REGROW_RETRY_MILLIS` while a rig, pilot or creature is in the tile (nothing is ever sealed in).
- **Root axes** (`LivingDarkAction::FireRootAxe`, bought at outposts, in town or at the Court):
  cut a 3-wide shaft 12 rows up through roots, flesh and plain rock and scorch it and its walls
  (never regrows); **fire** (incendiary blasts, `living_dark_blasts`) burns roots away for good.
- **Spores**: a pod burst by drill, blast or a rig brushing it (`SPORE_POD_BRUSH_PIXELS`), a
  puffer's spore ball or the Mind's orb (`ShotKind::Spore` / `MindSpore`,
  `burst_spore_shot`) leaves a `FieldPreset::Spores` zone (blur, a green grade, a touch of
  chromatic split) for `SPORE_CLOUD_MILLIS` / `SPORE_SHOT_CLOUD_MILLIS`, at most
  `MAX_SPORE_CLOUDS`; it stings a rig without the Bio-Filter (`SPORE_DAMAGE_PER_SECOND`), and
  `WorldState::view_fields_for` drops spore zones from a Bio-Filter rig's view.
- **Bounces**: mushroom caps bounce as before (`MUSHROOM_BOUNCE_*`); a rig falling onto a cap
  crawler's cap bounces off it too (`LivingDarkEvent::CapBounce`).
- **Acid tide** (`tide_at`, `heartbeat_bpm`): a pure function of world time (tick-rate safe):
  rising 35 s, high 10 s, ebbing 35 s, low 10 s. Every `TIDE_STEP_MILLIS` the gullets within
  `TIDE_REACH_TILES` of a rig fill their open basin with acid up to the level
  (`TIDE_HIGH_PERMILLE` of the gullet at high water) and drain what is above it, waking the
  fluid pass (acid flows into tunnels dug from a gullet). The Bio-Filter keeps acid out
  (`submersion_gear().acid_resistance` 1).
- **The Glow Sea** (`glow_light_at`, `lamp_scale_for`): its light at a point is its ambient
  (`GLOW_SEA_AMBIENT`) plus every light-giving creature's pool (`EnemyKind::light_tiles`) and
  every rig lamp (a candle without Lamp II, `LAMP_II_LIGHT_SCALE` with it).
- **The Hive** (`HiveState`, crew-wide): standing -100..100; errands in order
  (`Errand::SoldiersAtTheGate`: cull 8 Swarm soldiers or lancers; `TheStolenBrood`: break 6
  jelly cells in the matriarch's gallery; `TheMatriarch`: slay her) counted in the tick (kills
  through `living_dark_kill`, broken cells through the journal), turned in at the Court for
  standing and credits. Drilling the Court's comb or jelly costs `COURT_THEFT_STANDING` each;
  under `COURT_HOSTILE_BELOW` its drones hunt rigs and its counter only takes amends. The last
  errand is a **choice**: slay the matriarch (`SwarmState::Broken`: no more soldiers are bred,
  `living_spawn_allowed`) or **broker a truce** (`SwarmState::Truce`: its soldiers stand down,
  the Court's standing drops). Honoured (standing 50+) halves the jelly's price.
- **The Mycelial Mind** (`world/enemies/living.rs`): wakes when a rig enters its hollow, with
  three nerve nodes (`EnemyKind::MindNode`) that shield it (`mind_shields`: every hit ignored);
  it lobs spore orbs, calls a spore storm onto every rig in its hollow every
  `MIND_STORM_MILLIS` (faster each cycle) and possesses the nearest creature (or raises two spore
  thralls) every `MIND_POSSESS_MILLIS`, at most `MAX_THRALLS` at once: a possessed creature
  (`EnemyState::phase == POSSESSED_PHASE`) senses rigs through rock and is never leashed. With
  its nodes dead it is exposed for `MIND_EXPOSED_MILLIS`, then they regrow. Its death
  (`mind_fell`) grants every rig the **Bio-Filter and Lamp II** (`grant_rig_system`), and
  rigs that join later are fitted too; with no rig in its hollow for `MIND_SLEEP_MILLIS` it
  sleeps and heals. On a Peaceful world (no creatures) it yields its gate to the first rig in
  its hollow (the Hive's errands, which start with a cull, cannot be finished there).
- **The brood matriarch** appears in her gallery when a rig comes near (until the Swarm breaks),
  and breeds Swarm soldiers (`brood_of`; bile polyps bud leechlings, cave nests still hatch bats).
- **Gate**: the Bio-Filter and Lamp II are boss rewards (`RigSystem::boss_reward`), never sold
  on Descent worlds (`UpgradeLock::Boss`); the descent harness grants them at the Descent's end
  after `BOSS_FIGHT_MINUTES` (25).

### Creatures (`enemy.rs`, appended `EnemyKind`s)

| Kind | Body | Behaviour | Counter |
|---|---|---|---|
| `SporePuffer` | walker | lobs spore balls (clouds where they land) | close in; Bio-Filter |
| `CapCrawler` | walker | charges; its cap bounces rigs that land on it, turns bullets | blasts, fire |
| `GlowGnat` | flyer swarm | bites; a small light | anything |
| `RootSnapper` | walker, dormant | lies still in a tunnel, snaps when close (leash 6) | keep away, fire |
| `BarkBeetle` | burrower | bores roots and rock (rarely, `BITE_MILLIS`) | blasts |
| `ThornWisp` | flyer | thorn volleys | close in, fire |
| `HiveSoldier` | flyer pack | the Swarm's swarm; stands down in a truce | blasts, fire |
| `HiveDrone` | flyer | the Court's worker: harmless unless the Court is hostile | do not steal |
| `AcidLancer` | walker | acid volleys | break line of sight |
| `SwarmMatriarch` | walker, elite | breeds soldiers in her gallery | explosives |
| `MawWorm` | burrower | bores flesh, lunges | stop drilling |
| `BilePolyp` | stationary | buds leechlings | explosives |
| `Leechling` | flyer, swims | small biter | anything |
| `LureFish` | flyer, swims | harmless, a light | follow it |
| `GlowJelly` | flyer, swims | drifting sting, a strong light | keep clear |
| `Angler` | flyer, swims, dormant | a light dangled as bait; lunges hard when close | do not follow the bait |
| `MycelialMind` | stationary boss | orbs, storms, possession; shielded by nodes | kill the nodes, then it |
| `MindNode` | stationary | shields the Mind | anything, fire |

All follow the body rules (`step_body`, `free_from_rock`, `keep_off_bodies`, `site_fits`,
contact boxes sized to their sprites, `CONTACT_MARGIN` / `STANDOFF_PIXELS`, line-of-sight
noticing); `living_notice` decides the factions and thralls. `EnemyKind::can_swim` answers
`true` for the Glow Sea's fish and jellies and the leechlings. Shots: `Spore`, `Thorn`,
`MindSpore`.

### State, formats and events

- `LivingDark` (world field, appended `SectionKey::LivingDark`, saved with serde defaults,
  hashed once non-empty so older worlds keep their hashes): root axes per rig, `HiveState`,
  `MindState`, the spore clouds' expiries. `LivingSchedule` (saved, never hashed or replicated,
  cleared in client keyframes like the fluids' schedule): tiles waiting to grow back, scorched
  tiles, the tides' applied levels.
- Commands: `LogisticsCommand::LivingDark(LivingDarkAction)` (root axes and the Court's
  `CourtService`s), checked by `check_living_dark` (refusals `LogisticsRefusal::LivingDark`),
  so the depth logistics dialog serves them with the inputs every seat already has.
- `field::ScreenEffect::Blur` appended (`SCREEN_EFFECT_COUNT` 14); `ScreenEffects::levels` is
  saved and sent as a list (older 13-level lists pad with zeros); the registry's `Blur` maps to
  it. `FieldPreset::Spores` appended.
- **`PROTOCOL_VERSION` 16** (one bump for the track); **no save format bump** (every new field
  has a serde default; format 13 files load as before).
- Events: one `GameplayEventKind::LivingDark { what, player_id, x, y }`, netcode codes
  `440 + LivingDarkEvent::code()` (440-459; 20 kinds).
- Dev seams: `release_spore_cloud`, `dig_living_tiles`, `flood_gullets_near`.

### Game side

- Tile art for the new cave tiles (tangled root strands, wet flesh lobes, honeycomb, jelly
  cells, a hanging spore pod, an acid gland, charcoal), and a back wall of its own for every
  living stratum's open ground (`tile_art/living_art.rs`).
- `rendering/world/living_dark.rs`: the Flesh Caverns' walls pulse with the heartbeat (low
  contrast, under two beats a second, scaled by the effects setting), and the Glow Sea's dark is
  drawn in the world, cut back only by living lights and lamps (the seat's post-process light is
  lifted there so the screen is not darkened twice).
- The post-process stack gains a **blur** pass (both GLSL variants, `uE`), eased and limited like
  every effect (Reduced lets 45 % through, Off none).
- Eighteen creature sprites and three shots (`pixel_art/sprites/living.rs`), with glows and
  the angler's bait; eleven audio patches appended at the end of `patches!` (five one-shots,
  six music loops) and the director's `living_dark_music` (the heartbeat loop's rate follows
  `heartbeat_bpm`; the Mind's theme replaces the Glow Sea's while it fights near).
- The logistics dialog becomes the Amber Court's counter in its hall (royal jelly, errands with
  their progress, the truce, amends, the crew's standing) and lists root axes in the field kit;
  the use prompt reads "Visit the Amber Court". The shop tags the boss rewards "BOSS".
- Scenes: `living_fungal_grove`, `living_spore_cloud`, `living_root_labyrinth`,
  `living_hive_court`, `living_hive_court_counter`, `living_hive_swarm`,
  `living_hive_matriarch`, `living_flesh_gullet`, `living_glow_sea`, `living_glow_sea_lamp_ii`,
  `living_mycelial_mind`, `living_mind_exposed`.

### Tests

`world/living_dark_tests.rs` (bounce off a cap and a cap crawler; a burst pod's cloud: blur and
grade on screen, the sting, the Bio-Filter's view, the clearing; roots growing back over minutes,
never into a rig, never where an axe cut; fire burning roots; flesh healing faster deeper; the
Hive's errands, theft, hostility, amends and truce, replicated through the section and hashed;
the matriarch and the broken Swarm; the acid tide rising and ebbing alike at 20 and 60 Hz; the
heartbeat; creature-only light and Lamp II; **a bot beating the Mycelial Mind** with a railgun
(nodes, exposure, possession) and the crew's rewards; a Peaceful world's open gate; determinism
and older worlds untouched), `worldgen/descent/living/tests.rs` (living rock in every stratum;
an ignored ASCII dump), `balance::descent::tests::the_living_dark_ends_on_the_plans_hours_and_its_gate_is_won_not_bought`
(the registry harness: Descent III ends inside §14's 22-29 h, the Bio-Filter won at its end,
no gap over 40 minutes), and the registry, field and landmark tests updated for the authored
strata.

### Descent probe

`--balance-report --only descent` on the real registry (default seed, this branch): Descent III
ends at **27.8 h** (§14: 22-29 h; it was 27.4 h with the Bio-Filter bought), the Mind's rewards
won at the Descent's end, the whole run 75.7 h, every gap between new things 40 minutes or less. The
Living Dark itself takes 6.3 h of the run (21.5 h to 27.8 h) because the registry's Crust still
runs long (14.3 h against 5-8 h): once the Crust track's re-pacing lands, the window
(`the_living_dark_ends_on_the_plans_hours_and_its_gate_is_won_not_bought`) needs a re-check at
integration.

### Review fixes (Wave 8 Living Dark review)

- **Auto-aim spares the peaceful** (`WorldState::living_peaceful`): the Amber Court's drones
  while it is friendly, the Swarm under a truce and lure fish are never locked onto, so a rig
  with auto-aim on (the touch and gamepad default) no longer sours the Court by standing in
  its hall.
- **Lifts stay open**: roots and flesh never grow back into a lift's shaft (its column from a
  row above its top station to a row below its bottom one).
- **The Mind's balance**: the boss bot fought with a top-tier hull (2,480) while the descent
  probe has a rig reach the Glow Sea at hull tier 9 (980), where it came out with under 20 %
  of its hull. The Mind's orbs now hit for 4 (was 6) and it has 300 health (was 420); the bot
  fights at hull tier 10 (`HULL_AT_THE_MIND`) and must keep 20 %.
- **Spores sting for real**: `SPORE_DAMAGE_PER_SECOND` is scaled by the depth's creature scale
  (`enemy::tier_scale`, about 1.8 hull a second in the Living Dark; it was a flat 1, which a
  deep hull never noticed).
- **Readable boss**: glowing nerve tendrils wire each living node to the Mind, and an exposed
  Mind shows a raw, throbbing core (`rendering/world/living_dark.rs::draw_mind_nerves`).
- **News lines** for each local seat (`local_game::announce_living_dark`): the Mind waking,
  exposed, regrowing its nodes and falling (with its rewards), the Court seeing its comb taken
  or turning hostile, the Swarm breaking, and the acid tide rising (seats in the Flesh Caverns).
- The brood matriarch never appears on a Peaceful world.

### Seams and known limits

- The Hive's errands are authored for the Court (the steward's three errands), not story arcs of
  the people engine: the Court has no generated residents yet (a Hive culture, insect folk art
  and routines would make it a full settlement; the errands' `Errand::brief` text and standing
  are ready to bind to a steward character).
- Possession reuses the crypt lord's `EnemyState::phase` byte (`POSSESSED_PHASE`) instead of a new
  field, so enemy records and hashes keep their layout.
- Spore clouds are field zones: up to `MAX_SPORE_CLOUDS` of the 64 zones a world keeps.
- The living strata still borrow their host band's heat (the integrator's heat-per-stratum work
  from the Crust track should give them their own).
- The descent harness models the boss as fixed minutes, not a simulated fight.

## Wave 8 story: the Fathers' expedition and anomaly rooms (`src/expedition.rs`, `src/expedition/{logs,memories,sites,anomaly}.rs`, `src/world/expedition.rs`)

Expansion plan §13 ("following the builders' cable") and §14's cross-Descent systems: the
story backbone of the Deep Descent for Descents I to III, the throughline design for the whole
Descent (below), and the anomaly rooms.

### The story

A generation ago six people from the town went down the builders' cable to find out what hums:
**Amos Varga** (lead driller; his daughter told him "don't forget to look up"), **Ines Calloway**
(surveyor; she keys every canister to the one before), **Dov Halloran** (radio; his drop line,
then his tapes), **Ruth "Bran" Brannigan** (medic and cook), **Teo Marsh** (the young mechanic)
and **Sefa Rhee** (naturalist). The town called them the Fathers ("two of us are mothers and Teo
is barely shaving"); Bran painted THE FATHERS' LOAD on the sled. Nobody came back up. One by one
they stay behind, each with a settlement that later remembers them: Teo with the Hive (III),
Sefa in Merrow City (IV), Bran with the jungle tribes (V), Dov dies under the inner sun (V),
Ines in the Archive of Stars (VII), and Amos reaches the Heart alone (VIII).

What the logs reveal, Descent by Descent: the world was **shaped** (I: clay laid in courses,
shelves that all lean to the cable, identical basalt columns, the Crust as a lid) -> the
**builders were here long ago** (II: sleeping machines, steps for longer legs, bones laid out in
rows, the ball-on-a-cable glyph; the Star Core is only their lamp) -> **life adapted to the
dark** (III: the Hive's Lamp-Bearers, walls that beat with the hum, and the queen's carving of
the cable running from a sleeping engine at the Heart up to the Moon, the tomb page's "cable
from the deep core to the hanging stone") -> civilisations below (IV) -> the hollow planet
and its inner sun (V) -> reality thinning near the machine (VI) -> the builders' city and
archive (VII) -> the Heart is a sleeping Star Drive (VIII).

### The throughline (about 80 logs; Wave 8 implements 1-30)

Logs are written in depth order and read in order. Later waves append to `logs::LOGS` (and
`ECHOES`, `CHAPTERS`, `memories::MEMORIES`) in the same order; placement, the journal, the
milestones and the cards read the tables only. Each row: number, stratum, site, author, beat.

| # | Stratum | Site | Author | Beat |
|---|---|---|---|---|
| 1 | Topsoil | camp | Amos | Six of us, one sled: THE FATHERS' LOAD. Down until we find what hums. |
| 2 | Topsoil | cairn | Teo | Roots this deep all grow down, toward something warm. |
| 3 | Clay Beds | camp | Bran | An aquifer floods the tent; the clay comes up in plates. |
| 4 | Clay Beds | cairn | Ines | The clay is laid in courses, like masonry. The cipher begins: read us in order. |
| 5 | Grey Stone | camp | Amos | Gravel Hollow: Old Tully's grandad found the rails already laid. |
| 6 | Grey Stone | rig | Teo | A one-piece rig older than any yard, its cab facing the cable. |
| 7 | Slate Shelves | camp | Bran | A shelf slides away with the stove; every shelf tilts toward the cable. |
| 8 | Slate Shelves | cairn | Ines | The cable: a plumb rail no drill scratches. A building's spine. |
| 9 | Basalt Columns | camp | Sefa | Identical columns. "Shaped. The world was shaped." |
| 10 | Basalt Columns | rig | Amos | The floor gives way (the Sinkhole): the Crust is a lid. Chapter I. |
| 11 | Crystal Faults | camp | Sefa | Singing glass; the note comes back up Dov's line a day late. |
| 12 | Fossil Oceans | camp | Sefa | A sea floor on end; gas breathes in the basins like a tide. |
| 13 | Fossil Oceans | rig | Sefa | Skeletons in sorted rows, skulls to the cable: a collection. |
| 14 | Pressure Cathedrals | camp | Ines | A hall too big for the lamp, steps for longer legs, a dead lift. |
| 15 | Pressure Cathedrals | cairn | Teo | A valve turned; the key left in the cairn (the Cathedral Lift). |
| 16 | Radioactive Hollow | camp | Bran | Counters ticking, the lead tent, Dov's shaking hands, iodine. |
| 17 | Ancient Machine Layer | rig | Amos | Doors that slide open: machines not ruined but waiting. |
| 18 | Ancient Machine Layer | camp | Ines | The ball-on-a-cable glyph: the builders, and their cellar. |
| 19 | Void Geode Fields | cairn | Teo | Weightless somersaults; gravity held back like a door. |
| 20 | Mantle Storm Zone | camp | Dov | A heat storm cuts the drop line; he will record and leave tapes. |
| 21 | Mantle Storm Zone | rig | Amos | The Star Core is not the bottom, only the builders' lamp. Chapter II. |
| 22 | Fungal Forest | camp | Sefa | Mushrooms that throw you; spores swim the lamps; all grows to the hum. |
| 23 | Fungal Forest | cairn | Bran | "Nobody licks anything." Spore paste in the cairn. |
| 24 | Root Labyrinth | camp | Amos | Roots regrow behind them; they never touch the cable. |
| 25 | Root Labyrinth | rig | Ines | Lost two days; she stops drawing walls, draws only the cable. |
| 26 | The Hive | camp | Sefa | Talking insects; the workers trade, the soldiers don't; the Lamp-Bearers. |
| 27 | The Hive | cairn | Amos | Teo stays to keep the nursery's builder pumps singing. |
| 28 | Flesh Caverns | camp | Bran | Walls that heal and beat: the hum all their lives was a heartbeat. |
| 29 | Glow Sea | camp | Sefa | No light but what life makes; an angler's lamp. |
| 30 | Glow Sea | rig | Ines | The queen's carving: an engine at the Heart, a cable to the Moon. Chapter III. |
| 31 | Underground Ocean | camp | Amos | The sled becomes a raft; a sea under the rock. |
| 32 | Underground Ocean | rig | Sefa | A drowned builder sub; the eel-dragon passes under them. |
| 33 | Brine Glaciers | camp | Bran | Frostbite; Dov loses two fingers and jokes about the dials. |
| 34 | Brine Glaciers | cairn | Ines | Builder figures frozen in clear ice around the cable. |
| 35 | Geyser Fields | camp | Dov | Riding a geyser up; the tapes soaked, one survives. |
| 36 | Merrow City | camp | Sefa | The fish-folk city in its air bubble; they knew the Lamp-Bearers. |
| 37 | Merrow City | cairn | Amos | Sefa stays to study the merrow and their archive. |
| 38 | Merrow City | camp | Ines | Merrow songs: the builders came down from the stars. |
| 39 | Pearl Abyss | camp | Bran | Pressure; Dov's heart is failing; she keeps it from Amos. |
| 40 | Pearl Abyss | rig | Amos | The Leviathan; the floor is a ceiling, and they fall through. Chapter IV. |
| 41 | Inner Sky | camp | Ines | A sun inside the world, with its own day and night. |
| 42 | Inner Sky | cairn | Dov | His last full tape: he records the inner sunrise. |
| 43 | Megafauna Jungle | camp | Bran | The jungle tribes; she nurses a chief's child through a fever. |
| 44 | Megafauna Jungle | rig | Amos | The rig wrecked by a beast; they ride on foot, then on its back. |
| 45 | Megafauna Jungle | camp | Bran | Bran stays: "someone has to be a mother down here." |
| 46 | Upside-Down Isles | camp | Ines | Walking upside down; the cable runs straight through the inner sun. |
| 47 | Upside-Down Isles | cairn | Dov | His last recording; he knows. |
| 48 | The Sunforge | camp | Amos | Dov is buried in the sun's light. |
| 49 | The Sunforge | cairn | Ines | The inner sun is a builder lamp: a lamp for the lamp. |
| 50 | The Sunforge | rig | Amos | The Sun Serpent; a builder locker holds an anchor that walks on walls. Chapter V. |
| 51 | Mirror Strata | camp | Ines | She meets herself, and her twin writes the same log. |
| 52 | Mirror Strata | cairn | Amos | Two of every canister: which is real? (Ines's cipher tells them.) |
| 53 | Gravity Shear | camp | Ines | Walking the walls; the cable is the only up. |
| 54 | Pixel Rot | rig | Amos | Ines's maps lose resolution; the rock turns to data. |
| 55 | Pixel Rot | camp | Ines | The world is drawn from a model; the builders kept the plans. |
| 56 | Time Sludge | camp | Amos | A day in an hour; his beard comes out grey. |
| 57 | Time Sludge | cairn | Ines | Logs out of order in time: why the cipher keys each to the last. |
| 58 | Echo Caves | camp | Amos | Total dark, blind hunters; they travel by echo. |
| 59 | Prism Desert | camp | Ines | Colour-keyed doors open to her lamp filters. |
| 60 | Prism Desert | rig | Amos | The Glitch rewrites the room; a drill that phases through alloy. Chapter VI. |
| 61 | Buried Megacity | camp | Ines | Streets, trams, towers; the archivist wakes and speaks. |
| 62 | Buried Megacity | cairn | Amos | The archivist knew they were coming: "you are late". |
| 63 | Buried Megacity | camp | Ines | The planet is the builders' ship's engine housing; the Moon its dock. |
| 64 | Engine Halls | rig | Amos | Crushers on a timed beat; Amos's leg broken. |
| 65 | Engine Halls | camp | Ines | The halls are the drive's governors, idling for an age. |
| 66 | Archive of Stars | camp | Ines | Star charts: the first coordinates beyond the Moon. |
| 67 | Archive of Stars | cairn | Ines | Ines stays to read the archive; Amos goes on alone. |
| 68 | Sleeping Garrison | camp | Amos | Alone among war machines; he writes to his daughter. |
| 69 | Sleeping Garrison | rig | Amos | The garrison wakes in waves; he runs. |
| 70 | The Seal Gate | camp | Amos | The gate wants a key from every Descent; he has none. |
| 71 | The Seal Gate | cairn | Amos | A letter home, left where the next crew will pass. |
| 72 | The Seal Gate | rig | Amos | The Gatekeeper lets him through: it knew the cable mark on his sled. Chapter VII. |
| 73 | Dark Matter Veins | camp | Amos | The more he carries, the lighter he gets. |
| 74 | Dark Matter Veins | cairn | Amos | The hum is the drive's idle, a heart beating for an engine. |
| 75 | Singularity Fields | camp | Amos | Light bends; he sees the Moon through the rock. |
| 76 | Singularity Fields | rig | Amos | The Fathers' Load gives out; on foot from here. |
| 77 | Core Ocean | camp | Amos | A sea of liquid metal; he rides the mag rails. |
| 78 | Core Ocean | cairn | Amos | A last letter to the five who stayed. |
| 79 | The Heart | camp | Amos | The Star Drive, asleep. It needs a rig and a key, and he has neither. |
| 80 | The Heart | rig | Amos | "You came down after us. Wake it. Go up." Chapter VIII. |

Echoes (in depth order): Dov's radio echoes run through Descent I to the Mantle Storm Zone's
"last transmission"; his ghost tapes run from below the Star Core to the Sunforge (log 48),
after which the archivist's recordings of the builders take over (Descents VI to VIII).
Memories: Gravel Hollow, the Old Deep camp and the Hive now; Merrow City (Sefa), the jungle
tribes (Bran), the Megacity archive (Ines) and the Heart outpost (Amos's letters) later.

### Implemented (Descents I-III)

- **Content** (`expedition::logs`): `LOGS` (30 `ExpeditionLog`s: day, title, author `Member`,
  stratum, `SiteKind` camp / rig / marker, text), `ECHOES` (12 `Echo`s: 6 radio, 6 ghost, each at
  a stratum and per mille through it), `CHAPTERS` (titles and revelations of I-III). Every
  stratum of Descents I-III holds one to two logs.
- **Placement** (`expedition::sites`): one `ExpeditionSite` per log on the far side of the
  builders' cable from the town (`landmarks::lift_column` + 24 to 120 columns), in its log's
  stratum: a stratum with `k` logs splits its rows (24 clear of each ragged boundary) into `k`
  slots and log `j` takes slot `j`, so every log lies below the one before in every world
  (`later_logs_lie_deeper_beside_the_cable`). Per-stratum hook: `placements_for(stratum)`.
  Rooms: a camp is 15x7 (walls included), a wreck 12x6 on scrap plating, a cairn 8x6; the
  room is air on a plank floor, its walls stay the stratum's rock. `sites(settings)` is cached
  per thread (pure).
- **The generator pass**: `sites::story_stamps(settings, rect)` (the log sites and anomaly
  rooms touching a chunk) is called in `streamed::assemble` as a pass of its own after the Descent
  landmarks and before the builders' cable (so the cable always wins), and generated fluid stays
  out of the story's rooms. Only V11 changes; **`GOLDEN_V11_DIGEST` is updated** (the golden
  chunks now also pin the first and last log sites and a story anomaly room; every chunk pinned
  before stays the same). V1-V10 digests are unchanged.
- **The journal** (`ExpeditionState`, `WorldState::expedition`, shared): `found` (logs read: a
  prefix, `1..=found`), `sealed` (later canisters reached early), `echoes` (a prefix of
  `ECHOES`), `next_echo_tick`, `anomalies` (rooms entered, by region) and `anomaly_kinds` (bits).
  Saved with serde defaults (older saves load empty), hashed only once non-empty (older
  worlds' hashes stand), replicated as the appended shared `SectionKey::Expedition`, so co-op
  partners share the journal. `set_expedition` is for tests, scenes and tools.
- **The tick** (`advance_expedition`, after `advance_logistics`, Deep Descent worlds only, each
  player's actor tile: the pilot on foot or the rig): a canister whose turn has come is read
  (`ExpeditionLogFound`, its `log_bounty` credits to the finder) and any canisters sealed after it
  open in turn; a later one is sealed once (`ExpeditionLogSealed`). The next echo plays when any
  actor is at or below its row (`ExpeditionEcho`), one at a time `ECHO_GAP_MILLIS` (12 s) apart
  through `ticks_for_millis` (tick-rate safe). A room's first entry raises `AnomalyEntered`
  (`first_of_kind` for the crew's first of that kind).
- **Settlement memories** (`expedition::memories`): 15 `SettlementMemory` lines with stable ids
  (`gravel_hollow.smith.rails`, `old_deep_camp.quartermaster.splice`, `hive.pump_tender.wrench_one`,
  ...) for Gravel Hollow, the Old Deep camp and the Hive; `after_log` holds a line back until the
  crew has read that far. `lines_for(settlement, &state)` and `memory(id)` are the seam: **the
  integrator wires them into each settlement's people** (the settlement tracks own the people).
- **Anomaly rooms** (`expedition::anomaly`): `ANOMALY_REGION` 112x96 regions, 22 % hold an
  18x10 room anywhere below the first 40 rows and above the floor's last 40 (not in the Inner
  Sky), clear of the cable, the Descent landmarks and the log sites. Kinds (`AnomalyKind`):
  `GravityFlip` (gravity up, free rotation, a cache on the ceiling, builder-brick walls),
  `PixelRot` (pixelate + palette + scanlines, crystal walls), `TimeFrozen` (time scale 0.15,
  desaturated cold grade, a relic cache floating mid-air, obsidian walls) and `Mirror` (screen
  and controls mirrored, glass walls, twin caches). The linings can be harder than the rock
  around them (obsidian, crystal) or not diggable at all (building glass), so every room has a
  way in (`AnomalyRoom::is_door`): a rig digs the stratum's ordinary rock to it. Most rooms have
  a doorway three rows tall at the floor in both side walls. The upturned room instead has a
  two-wide hatch in the middle of its floor that a rig climbs up through and then falls on up
  to the ceiling. Through a side door the flip turned the stick round on the threshold (the
  stick drives along the gravity frame's horizontal, a Wave 7 rule), and a driving rig was
  pushed straight back out. The room fields cover the open room only, so the doorways stay
  ordinary ground. The upturned room switches at a hard edge; the others fade in over one
  tile. Bot tests play the way in rather than teleporting:
  `a_rig_climbs_in_through_the_hatch_and_falls_up_in_the_upturned_room` and
  `a_rig_drives_into_the_mirror_room_through_a_doorway`. No room lies in a stratum whose own drill
  cannot open a treasure cache (the Topsoil), and the stopped room's relic cache falls back to a
  treasure cache where the stratum's drill cannot open a relic cache (`AnomalyRoom::cache_tile`;
  review fix, `every_anomaly_room_opens_through_a_door_and_its_caches_open_with_the_local_drill`
  pins it). Rooms and log sites also keep off the pilot-only ruins (stamped before the story
  pass, which would cut them open) and the tomb entrances (stamped after it):
  `sites::crosses_older_spaces`; a room is dropped, a log site tries a few further distances out
  along its row (`story_stamps_keep_off_pilot_spaces_and_tomb_entrances`). The rule is a Wave 7 field:
  `anomaly_field_source` is registered first in `field::STRATUM_FIELD_SOURCES` and answers only
  inside a room's zone (`AnomalyRoom::zone`, feathered over the walls), laying the room's
  departures over the stratum's profile (`anomaly::over`: a pixel-rot room in the Gravity Shear
  still turns gravity) and `None` elsewhere. A small per-thread cache keeps the per-tick lookups
  cheap. Screen effects are steady levels through the game's effects-intensity setting.
- **Milestones**: `TrailFound`, `ShapedWorld`, `BuildersCellar`, `GrowsInTheDark` (a chapter's
  logs all read), `GhostVoices`, `FirstAnomaly`, `EveryAnomaly`, each unlocking a new paint
  (`Khaki`, `Ochre`, `Bronze`, `Lumen`, `Phantom`, `Anomaly`, `Glitch`). `MilestoneFacts` gains
  `expedition_logs`, `expedition_echoes` and `anomaly_kinds` (crew-wide, so every partner earns
  them).
- **Events 490-493**: `ExpeditionLogFound`, `ExpeditionLogSealed`, `ExpeditionEcho`,
  `AnomalyEntered`. **Formats**: `PROTOCOL_VERSION` 16 (the section, events, milestones and
  paints); no save format bump (every new field has a serde default).
- **Game**: `ModalScreen::ExpeditionLog` (`shell/expedition.rs`, `rendering/expedition_hud.rs`)
  after the tomb codex in the map's pages on Deep Descent worlds: the trail (logs, echoes,
  anomalies, the lead to the next canister), chapters with their read logs as rows and their
  revelations, the selected entry pinned under the list; rows by keys, pad, mouse and touch
  (`menu_pick`). Story moment cards (`StoryMomentKind::{ExpeditionLog, RadioEcho,
  GhostRecording, Anomaly}`, `rendering/expedition_card.rs`) go up on every local seat for logs
  and echoes, on the entering seat for a room. The world draws camps (tent, lamp on a rope, sled
  "F.L.", stove, bedroll, pickaxe), wrecks and cairns with their canister (waiting, sealed,
  open) and each room's tells (`rendering/world/expedition.rs`; luminous parts after the
  darkness pass). Audio patches appended at the end of `patches!`: `radio_echo`, `ghost_voice`
  (procedural formant voices, never words), `log_canister`, `anomaly_hum`. Scenes:
  `expedition_camp`, `expedition_log_read`, `expedition_cairn`, `expedition_journal`,
  `expedition_radio_echo`, `expedition_ghost_recording`, `anomaly_gravity_flip`,
  `anomaly_pixel_rot`, `anomaly_time_frozen`, `anomaly_mirror`.
- **Tests**: `expedition::tests` (depth order of logs and echoes, every log below the one before
  beside the cable for several seeds, deterministic and seeded placement, nothing on older
  worlds, sites off the landmarks, rooms rare, varied and at every depth, their fields inside
  and gone six tiles out, the same field in a far frame, memories' ids and gating),
  `world::expedition_tests` (sites carved, log 3 sealed until 1 opens 1-3 in order, echoes in
  order and spaced at 30/60/120 Hz, every room kind entered with its field felt and milestones
  earned, save/load and hashing, co-op replication of the shared section) and the game's
  `local_game::tests::expedition_tests` (the card, the page cycle, a touch pick, a sealed
  canister's line, an echo's card, classic worlds unchanged).

### Seams for the integrator and later waves

- The strata tracks own each stratum's content; the expedition's placement is its own pass
  and keeps clear of the Descent landmarks and the cable. If a stratum track stamps a
  settlement or set piece where a site lands, move the site with `placements_for` (slots) or
  keep the site's room clear in that track's stamp.
- Wire `memories::lines_for` into Gravel Hollow, the Old Deep camp and the Hive (stable ids).
- Descents IV-VIII: append logs 31-80 (table above), echoes, chapters and memories; give
  `log_bounty` a look against the deep economy; the gates' set pieces can raise their own story
  cards through `StoryMomentKind`.
- The Strange Layers' own crystal anomaly rooms (Wave 7 landmarks) carry no rule yet; they could
  take an `AnomalyKind` the same way.


## Wave 8 integration: Descents I-III stacked and wired

The five Wave 8 tracks (progression, the Crust, the Old Deep, the Living Dark, the Fathers'
expedition story) are stacked in that order on top of Wave 7, each track's commits picked
linearly. What the integration changed and wired, besides resolving the shared files (enum
`ALL` arrays and appended variants in stack order, event-code ranges 400-415 / 420-438 /
440-459 / 490-493, `patches!` appended crust, old deep, living dark, story):

### Formats

- `PROTOCOL_VERSION` 16, one bump for the wave (its doc lists every track's additions). Save
  format 14, one bump (progression's); every other track's fields are serde defaults, and the
  every-format save test still loads formats 8-13 for classic and Deep Descent worlds.
- `GOLDEN_V11_DIGEST` updated (`0xaa10_819f_130e_3f04`): the four content tracks' generation
  plus the story sites kept off the strata's set pieces. V1-V10 digests unchanged.
- Fixes found while stacking:
  - `SectionKey::Crust` was never in `SectionKey::SHARED`, so online clients never saw the
    Sinkhole run, the Deep Claim or the logs read. It is shared now.
  - `CrustState::{collapses, slides}` and `LivingSchedule::{due, waiting, scorched, tides}`
    were maps keyed by tiles. Saves are JSON and JSON keys must be strings, so a V11 save
    failed while a timber set was failing or a root was waiting to regrow. They save as lists
    of pairs now (`crate::pair_list`).
  - The two tracks' private `after_millis` and `read_log` helpers collided; the Old Deep's
    `after_millis` is shared and the story's is `read_expedition_log`.

### Mechanics that met

- **Heat**: the Crust's per-stratum heat (`crust::heat_units`) is what every V11 rig takes,
  capped in the placeholder Descents by progression's `placeholder_heat_excess_cap`.
- **The Deep Claim gate** crushes any rig below Descent I without it, so the Old Deep's,
  Living Dark's and progression's test worlds now hold it (`grant_deep_claim`).
- **Brood**: the Glowmother breeds glow grubs through the Living Dark's `living::brood_of`.
- **Lamp**: `lamp_scale_for` multiplies the Headlamp line's tier scale into Lamp II's beam,
  but no Headlamp tier lights the Glow Sea (only Lamp II does).
- **Fields and effects**: the Old Deep's dose screen (`deep_screen`) is laid over the
  Living Dark's per-seat view fields (the Bio-Filter's clear view).
- **The creature body test** spaces the cave kinds 22 to a row, three columns apart, with
  later rows half a gap over, so the Crust's and the Old Deep's kinds all fit the arena; the
  Living Dark's keep their own arena.

### The gates match the shops

- **Drill II** is one tier past the Crust's drill cap (`core_tier_cap(0) + 1`): the shop
  only sells it once the Old Deep is reached, and only the Sinkhole's Deep Claim gets a rig
  there.
- **The Pressure Hull is won from the Star Core** (`RigSystem::boss_reward` is
  `DescentBoss::StarCore`, `UpgradeLock::Boss` in every shop), like the Mycelial Mind's
  Bio-Filter and Lamp II. Before, the progression shop sold it at 60,000, which skipped the
  Old Deep's gate. The descent probe fights each boss (`BOSS_FIGHT_MINUTES`) and grants every
  reward of that boss. The shop and felt-need tests use the Cryo Hull instead.
- Test: `wave8_seam_tests::the_shops_match_the_gates_the_descents_hold`.

### The story in the strata

- **The Old Deep's Fathers' camps** used to tell their own day-numbered logs, which
  contradicted the story track's journal (the same days, different events). Each camp now
  holds the expedition's first log of its stratum (`old_deep::expedition_log_of_camp`), and
  drilling it reaches that canister through the journal (`reach_expedition_canister`). The
  log opens in order or stays sealed, and the journal's card tells it. The builder tablets
  and the core's revelation keep their own news. The Crust's strongbox logs stay a hired
  hand's diary in their own voice.
- **The expedition's camps and the anomaly rooms keep off the strata's set pieces**
  (`expedition::sites::crosses_strata_set_pieces`): the cabin, Gravel Hollow, the Sinkhole,
  the mine works, the Cathedral, the Star Core arena, the Glowmother's den, the Fathers'
  camps and the Living Dark's stamps. The story pass is stamped after them; on seed 0 log
  5's camp cut through the Grey Stone's mine works. A log site moves along its row, then to
  other rows of its slot; a room is dropped.
- **The settlements remember the Fathers** (`expedition::memories::line_for_visit`, opened as
  the crew reads the trail, newest first):
  - Gravel Hollow's people pass a line on when asked for news, and on every other chat.
  - A seat arriving at Gravel Hollow's or Cathedral Station's outpost, or at the Amber
    Court, gets a "They remember the Fathers" story card (`StoryMomentKind::Remembered`).
    It shows one line a settlement a day and reads only replicated state, so it works
    online and per seat.
- The descent probe's story sources include the expedition's logs and echoes
  (`STORY_LOG_SOURCES`).

### Music

Each of the 17 strata of Descents I-III plays its own bed: the Crust's five, the Old Deep's
seven, the Living Dark's five. The Sinkhole, Star Core and Mycelial Mind themes take over in
their fights. Test: `audio::director::tests::every_stratum_of_descents_one_to_three_plays_its_own_piece`.

### The descent probe and the default world

`cargo test --release -p fatherload_simulation descent_report -- --ignored --nocapture` on the
integrated registry (default seed):

| Descent | Ends at (h) | §14 target (h) | Everyday trip fuel (max % of tank) | Deaths |
|---|---|---|---|---|
| I The Crust | 6.4 | 5-8 | 49 | 0 |
| II The Old Deep | 16.0 | 13-18 | 15 | 0 |
| III The Living Dark | 23.0 | 22-29 | 17 | 0 |
| (whole run, VIII The Heart) | 71.4 | 60-90 | 12 | 2 |

All 17 strata of Descents I-III pass the 20-40 minute check (the longest wait is 14 minutes,
in the Void Geode Fields and the Mantle Storm Zone), with no deaths. The Star Core's Pressure
Hull is won at 14.5 h and the Mind's Bio-Filter at 21.7 h. Descents IV-VIII still end 1-6 h
early, and seven of their placeholder strata wait more than 40 minutes; Waves 9-10 author
them. Since I-III pass, **new worlds start on V11**: `GeneratorVersion::NEW_WORLD`, from the
progression track. "Classic (V10)" stays selectable in the New World dialog, and
`GeneratorVersion::CURRENT` stays V10.

### Scenes

Deep scenes start with the Fathers' echoes heard, so the first echo's card no longer covers
them. `old_deep_fathers_log` reads its camp's expedition log, and
`settlement_memory_cathedral` shows Cathedral Station's camp remembering the Fathers.

### Still open

- The Crust's strongbox diary and the Old Deep's builder tablets are separate from the
  journal (they are not numbered expedition logs).
- Cathedral Station's camp and the Hive have no resident people: their memories are cards,
  not conversations.
- The probe models every gate fight as fixed minutes.

## Wave 9 drowned: Descent IV, the Drowned and Frozen (`src/drowned.rs`, `src/world/drowned.rs`, `src/world/enemies/drowned.rs`, `src/worldgen/descent/drowned.rs`)

Descent IV of a V11 world (rows about 4,200-5,800, §14) stops being a placeholder: water and
ice everywhere, each with a new movement rule, a friendly fish-folk city in an air bubble, and
the Leviathan at the gate. Nothing here touches V1-V10: every rule returns at once on worlds
without Descents (`drowned::drowned_rules`), only V11 generation places the new tiles, and the
V1-V10 golden digests are unchanged. **`GOLDEN_V11_DIGEST` changed** to `0xa443_78a8_e403_3f8b`.

### The strata (registry, `descent/registry.rs`)

| Stratum | Signature rule (introduced gently, pushed deeper) | Natives | Set pieces | Rock, ores, layer resource | Music / ambience | Story beat |
|---|---|---|---|---|---|---|
| The Underground Ocean | **submarine mode**: ballast trims the sub to rise, hover or sink; **gyre currents** circle every basin (weak at the top, strong at the bottom, half again in "Current Gyres"); the scanner becomes a **sonar ping** | reef shark (circles between dashes), ink squid (bites, inks the water black, darts away); mini-boss **the eel-dragon** | flooded **basins** (kelp forests, coral, a breath of air under the roof, bigger deeper), the eel-dragon's **grotto**, the **drowned builder sub** | seabed rock, coral seams; platinum, diamond, mythril; Deep Pearl | `UndergroundOcean` / `Reef` | the drowned sub |
| Brine Glaciers | **ice physics** (rigs slide on brine ice, pack ice and rime), **frozen-over tunnels** (dug tiles refreeze as rime, 70 s at the top, 35 s at the bottom), **hull frost** (a cold meter that slows the rig until a thermal vent, an outpost or the city warms it) | frost mite (bites frost the hull), tusk seal (a straight belly-slide charge) | **ice galleries**, **vent grottos** (common at the top, rare deeper), the **frozen figures** round the cable | brine ice / pack ice; diamond, platinum, uranium; Brine Ice | `BrineGlaciers` / `Ice` | builder figures in the ice |
| Geyser Fields | **geysers**: each vent blows on its own world-time beat, telegraphed by a 1.5 s gurgle; a rig in the chimney is thrown upward (an elevator), one on the throat is scalded | scald beetle (rides the jets up), sinter snail (armoured; seals tunnels behind it with sinter) | geyser **chimneys** (taller deeper), **Old Faithful** (a 5-wide, 60-row shaft on a 16 s beat) | sinter; platinum, mythril, uranium; Sinter Crust | `GeyserFields` / `Geysers` | Dov's soaked tape |
| Merrow City | a **friendly city** of fish-folk in an air bubble: residents with routines, dialogue and trades (the village machinery), the Descent IV outpost's shop, fuel and repairs, the merrow's **aquarium furniture line** | spinepuff (harmless until close, then a ball of spines), reef shark | **Merrow City**: a glass dome round the outpost and the merrow's hall, the sea held round it, a glass airlock tube, a harbour under the streets with two **moon pools**; reef terraces | reef stone; emerald, platinum, diamond; Coral Glass | `MerrowCity` / `Bubble` | the merrow remember the Fathers, and Sefa |
| Pearl Abyss | **crushing pressure** (`abyss_pressure`: the trench's depth plus the water over the hull) against the seals (`pressure_capacity`: the Pressure Rating's Abyssal Seals hold its upper part, the Cryo Hull all of it); past them the hull cracks; **the screen darkens and desaturates as the pressure rises**; **pearl clams** gape and bite on a beat | lure angler (its light drags rigs toward its jaws), gulper eel (a long gape, a gulp that stalls the rig) | **pearl beds**, **the Leviathan's arena** (a 104 x 40 flooded hall, clear of the cable) | pillow basalt; diamond, mythril, uranium; Abyss Pearl | `PearlAbyss` / `Trench` | the floor is a ceiling |

`MusicCue` (six), `AmbienceCue` (four), `LandmarkKind::Drowned` (generator
`LandmarkGenerator::Drowned`), thirteen `CaveTile`s (codes 22-34: `Seabed`, `Kelp`, `Coral`,
`BrineIce`, `PackIce`, `Rime`, `ThermalVent`, `FrozenFigure`, `Sinter`, `GeyserVent`,
`PillowBasalt`, `PearlClam`, `MerrowStone`) are appended. The strata's rock counts as plain rock
for ore and hazards (`streamed::base::is_plain_rock`).

### Generation (V11 only; `worldgen/descent/drowned.rs`)

A pure function of the settings and the tile, run by `streamed::assemble` after the Living
Dark's pass (before the Sinkhole, the Old Deep, the story and the cable, which carve over it):
region stamps (`DROWNED_REGION` 72 x 48, one per region at most, rolled more often deeper,
kept off stratum edges, the fixed stamps, the city and the outposts' column) and fixed stamps
(`eel_grotto`, `drowned_sub`, `frozen_figures`, `old_faithful`, `leviathan_arena`,
`merrow_city`). Generated water is full fluid tiles at rest (it costs nothing until a
neighbour changes; breaching a basin from the side drains it through the budgeted fluid pass).
The fluid-pocket pass skips the city's bounds (its streets stay dry), and caves keep out of it
(`villages::keeps_caves_out`). `set_piece_bounds` lists the fixed set pieces for the story
pass. `basins_in` gives the tick its gyres.

**Merrow City** (`MerrowSite`): the merrow's settlement hall (`HALL_WIDTH` x
`HALL_AIR_ROWS`) beside the Descent IV outpost on the spawn side, sharing its floor (like
Gravel Hollow); a half-ellipse dome 24 rows tall over both, cut at the cable's rail on the far
side; `Built(Glass)` round it, sea water six tiles thick outside the glass, a coral rim; a glass
airlock tube on the hall's side at the floor; the harbour under the floor with two moon pools.
The village is `placement::underground_id(drowned::MERROW_SITE)` (one past the last site any
world places), culture `Culture::Merrow`, name "Merrow City", found by every village lookup
(`villages::merrow_hall`, `special_halls`).

### Rules (`world/drowned.rs`; all timing from world milliseconds or per-second rates)

- **Ballast** (`RigWater::ballast`, per mille, in the rig's per-tick state `Player::water`, so
  it is predicted and reconciled like the pose): under water (`SUBMARINE_SHARE`), thrust blows
  the tanks and the drill floods them (`BALLAST_RATE_PER_SECOND`), let go and the trim holds;
  a neutral trim carries the weight the water does not (the sub hovers), a full one pushes
  `BALLAST_ACCELERATION` up or down. Out of water the tanks vent to neutral.
  `apply_submarine` runs after the fluid step. A sinking sub within `SETTLE_REACH` (4 px) of
  the bottom settles onto it (searched in quarter pixels) and rests, so its drill bites: the
  movement sweep stops a rig a step short of a floor, and at 20 Hz the water's sink speed kept
  that step longer than the gap tick after tick (a flooded shaft stalled the cheating bot).
- **Currents** (`current_at`: a basin's gyre, `gyre_velocity`, and the Leviathan's maelstrom
  toward its maw) carry a submerged rig as a drift (`CURRENT_DRIFT` of the current's speed,
  through `nudge_rig`, which only moves a rig where it fits): the drive's braking cannot cancel
  the sea.
- **Sonar** (`sonar_ping`, `sonar_reach`, `sonar_pings`, `sonar_light_at`, `drowned::sonar_light`):
  the scanner under water (or anywhere in Descent IV) also pings: a ring spreading
  `SONAR_RING_TILES_PER_SECOND`, fading by `SONAR_PING_MILLIS`, mapping the tiles in reach
  (12 tiles, 26 with the Sonar Suite); one ping a second per rig at most, however long the
  scanner is held. **Seam for the Echo Caves (Descent VI)**: call
  `WorldState::sonar_ping(player, output)` and read `sonar_light_at(x, y)` for the moment a
  ping lights total darkness.
- **Ice** (`apply_drowned_drive`, after the ground grip): on brine ice, pack ice and rime the
  drive's push takes `ICE_TRACTION` and braking almost nothing (`ICE_DRAG_PER_SECOND`); the
  rig slides the distance the drive braked away.
- **Hull frost** (`RigWater::frost_micro`, millionths, so a slow freeze moves every tick at
  every rate): +12/s at the glaciers' top to +34/s at the bottom (twice in brine), -260/s by a
  thermal vent, -400/s at an outpost or in the city, -70/s elsewhere; frost mites add 70 a bite.
  It takes up to `FROST_SLOW_MAX` (55 %) of every push and of the top speed, never hull: the
  frost slows, it never kills. The Cryo Hull shrugs it off. `FrostWarning` at 600.
- **Refreezing**: openings in the Brine Glaciers (outside outposts, lift columns and the city)
  are scheduled on the Living Dark's regrowth schedule as `Regrowth::Rime` (appended), so the
  regrowth budget and the never-into-a-body rule apply; `TunnelRefroze` when they freeze.
- **Geysers** (`geyser_phase`, `geyser_jet_at`): a vent's period is 7-10 s by column (Old
  Faithful 16 s), the last 1.8 s erupting after a 1.5 s gurgle; a rig whose column runs open
  down to a vent within `GEYSER_MAX_ROWS` is thrown up at `GEYSER_LIFT_SPEED` (Old Faithful
  1.3 x); within `GEYSER_SCALD_ROWS` of the throat it is scalded. Pure world time: every tick
  rate erupts together.
- **Pressure** (`abyss_pressure_for`): over the seals, hull is lost in proportion to the
  overshoot (`PRESSURE_DAMAGE_PER_SECOND` per 100, times the depth's creature scale);
  `PressureWarning` every 3 s while it bites. On Descent worlds the Pressure Rating and the
  Pressure Hull also deepen the fluids' pressure rows (`submersion_gear`), so a Descent IV rig
  can swim its deep basins.
- **Clams** (`clam_open`, 2.6 s of every 6 s): drilled open while gaping, an Abyss Pearl
  (`ItemKind::Layer`) and `CLAM_PEARL_CREDITS`; shut, it bites the nearest rig and closes back.
- **Lore** (`DrownedLore`, one per stratum, `DrownedState::lore`): found entering the wreck's
  hull, the frozen figures' gallery, Old Faithful, the city (with `CityReached`) and on the
  Leviathan's fall; the game shows each as a story card.
- **Lure anglers** drag rigs in their light (9 tiles, line of sight) at 55 px/s.

### Creatures (`enemy.rs`, `world/enemies/drowned.rs`)

Eleven appended `EnemyKind`s on the generic body rules (`step_body`, `free_from_rock`,
`keep_off_bodies`, `site_fits`, contact boxes sized to the sprites, `CONTACT_MARGIN` /
`STANDOFF_PIXELS`, line-of-sight noticing), each with its own twist: reef shark (circles its
prey between dashes, `drowned_chase`), ink squid (inks and flees on a bite), frost mite (frost
on a bite), tusk seal (a straight belly slide until a wall, `drowned_after_move`), scald beetle
(rides erupting jets), sinter snail (seals the open tile behind it every 6 s while idle, never
over a body or fluid), spinepuff (sight 3: harmless until close, drawn swollen while hunting),
lure angler (the drag), gulper eel (a gulp stalls the bitten rig), the eel-dragon and the
Leviathan. Sea creatures can swim and appear swimming in deep water (`drowned_swim_site`). The
bosses never roam (`drowned_spawn_allowed`). `ShotKind::PressureBubble` appended.

- **The eel-dragon** (`advance_eel_dragon`): wakes when a rig comes within 18 tiles of its
  grotto (hostile worlds only); besides its lunges, every 7 s it crackles for 1.3 s
  (`EelDragonPulse`, drawn as sparks and its ring) and then jolts every rig in the water within
  5 tiles. Its death pays `EEL_BOUNTY` to the killer.
- **The Leviathan** (`advance_leviathan`, the gate): wakes when a rig enters its arena. Its pose
  is a pure function of its move and the world time since the move began (`LeviathanMove`),
  so it swims alike at every tick rate. *Circling* (above 2/3): laps the arena, and every 5.5 s
  stops, flares its eyes and roars (1.4 s, the charge's path drawn in red), then charges across
  to 8 tiles past where the target was. *The maelstrom* (above 1/3): alternates charges with
  sinking to the floor and opening its maw for 5 s (the whirlpool pulls rigs in, the open maw
  takes `MAW_DAMAGE_FACTOR` damage), then spits pressure bubbles at every rig. *Thrashing*:
  1.0 s tells, each charge shakes three boulders off the roof over each rig, and it calls two
  reef sharks once. No clock: with no rig in its arena for 30 s it sinks back to sleep, healed.
  On a Peaceful world it yields its gate to the first rig in the arena. Its death fits **the
  Sonar Suite and the Cryo Hull** to every rig (later joiners too): `RigSystem::boss_reward`
  answers `DescentBoss::Leviathan` for both, so no shop sells them on Descent worlds.
  `MAX_HIT_RADIUS` grew to 1.5 tiles so its 44-pixel body is a fair target.

### State, events and formats

- `DrownedState` (`WorldState::drowned`, serde default, hashed only once non-empty, the
  appended shared section `SectionKey::Drowned` in `SHARED`): sonar pings, ink clouds (field
  zones of `FieldPreset::Dark`), the eel-dragon, the Leviathan, the lore found.
  `Player::water` (`RigWater`) rides `PlayerRig` / `PlayerTickState`.
- Events: `GameplayEventKind::Drowned { what, player_id, x, y }`, netcode codes
  `520 + DrownedEvent::code()` (520-539; 20 kinds).
- **`PROTOCOL_VERSION` 17** (one bump for the track). **No save format bump**: every new field
  has a serde default.
- The merrow: `Culture::Merrow` (in `EVERY`, not `ALL`; names, local colour, roles: traders,
  storytellers, scholars; cloister style; a fountain), `Furniture::{Aquarium, CoralLamp,
  KelpPlanter, ShellChair, PearlMirror}` (`AQUARIUM_LINE`; the reef aquarium is the merrow's
  signature piece, their makers sell the rest of the line).
- Shared-file edits (all additive): registry, `cave.rs`, `enemy.rs`, `event.rs`, `descent.rs`
  enums, `landmarks.rs`, `assemble.rs` (the pass and the dry city), `villages.rs` (Merrow
  City's hall), `layout.rs`, `people/*`, `home/decor.rs`, `simulate.rs` (two movement hooks,
  the scanner hook, `advance_drowned` after the Living Dark), `world/enemies.rs` (notice,
  habitat, chase, after-move, bite, maw damage, kill hooks), `world/living_dark.rs` (the rime
  arm of the regrowth), `fluids.rs` (pressure rows), `depth_progression.rs` (boss rewards),
  `balance/descent.rs` (authored content and the lore as story markers).

### Game side

Tile art and back walls for every stratum (`tile_art/drowned_art.rs`: barnacled seabed,
kelp fronds, branching coral, bubbly brine ice, fractured pack ice, feathered rime, amber vents,
figures in clear ice, banded sinter, geyser throats, pillow basalt, giant clams, reef stone);
eleven creature sprites and the bubble (`sprites/drowned.rs`; the Leviathan a 52 x 18 sprite
drawn at twice the scale, three frames: swim A/B, maw agape); the world layer
(`rendering/world/drowned.rs`: geyser tells and columns, clams gaping with a pearl, vent
shimmer, current motes along each gyre and spiralling into the maelstrom, fish past Merrow
City's glass, the Leviathan's eye flare and red charge line, its breathing maw, the eel-dragon's
crackle, sonar rings breaking on rock and echoing creature outlines, frost riming the rigs;
sea creatures are drawn again over the water's darkness so they read, and the Pearl Abyss keeps
field light 820 per mille so the Leviathan stays visible as the pressure greys the view);
the per-seat panel and screen (`rendering/drowned_hud.rs`: ballast gauge, frost meter,
pressure meter against the seals, the Leviathan's health, phase and what to do; the abyss's
pressure desaturates, vignettes and darkens the view and frost grades it cold, all through the
field-effect stack, so the accessibility setting applies and nothing flashes); the merrow's
look (teal scales, crest fins, gills, shell necklaces, kelp and sea-glass cloth) and the
aquarium line's furniture art; news lines and lore story cards (`StoryMomentKind::DeepLore`,
appended); audio (appended at the end of `patches!`: sonar ping, geyser blast, ice crack, hull
groan, clam snap, the Leviathan's roar, the merrow chime, five stratum beds and the
Leviathan's theme; `drowned_music` in the director). Input needs nothing new: ballast is
thrust and drill, the sonar is the scanner, so keyboard, pads, touch and split-screen seats all
play it. Scenes: `drowned_ocean_basin`, `drowned_sonar`, `drowned_eel_dragon`,
`drowned_builder_sub`, `drowned_glaciers`, `drowned_frozen_figures`, `drowned_thermal_vent`,
`drowned_geyser`, `drowned_old_faithful`, `drowned_merrow_city`, `drowned_merrow_arrival`,
`drowned_moon_pool`, `drowned_pearl_abyss`, `drowned_leviathan`, `drowned_leviathan_roar`,
`drowned_leviathan_maelstrom`.

### Tests

`drowned::tests` (geysers' beat with a tell before every eruption, pressure and seals, frost,
clams, the sonar ring, gyres, the Leviathan's phases), `worldgen::descent::drowned::tests`
(every stratum's rock and set pieces, the fixed stamps where their functions say, a dry
Merrow City with glass, sea and moon pools, determinism, none on older worlds; an ignored ASCII
dump), `world::drowned_tests` (older worlds untouched; ballast rises, hovers and sinks, alike
at 20/30/60 Hz; a basin's gyre carries a hovering sub; the scanner's sonar and the Sonar
Suite's reach; ice slides where stone brakes; frost builds, slows, thaws at a vent and the Cryo
Hull ignores it; a tunnel refreezes but never on a rig; a geyser throws a rig up, erupts at the
same moment at 20/30/60 Hz and scalds a rig on its throat; pressure past weak seals, held by
the Cryo Hull; clams pay or bite; Merrow City peopled under its bubble; every stratum's
natives; mites, squid, angler and snail at work; the eel-dragon's wake, tell and bounty; **a
bot with Descent IV kit (hull tier 11, the Abyssal Seals, a railgun) beats the Leviathan**
through all three phases with a fifth of its hull left, winning the Sonar Suite and the Cryo
Hull, the Descent V gate crushing before and passing after; the gate rewards are boss rewards;
an empty arena puts it to sleep; Peaceful yields the gate; JSON saves, the section and the
hash; a flooded sub settles on the bottom and drills through it at 20-120 Hz). The audio
director's every-stratum test covers Descent IV's five beds.

Shared tests adjusted for the authored strata (additively, each with a note): the registry's
music check accepts Descent IV's own pieces and its landmark check `LandmarkGenerator::Drowned`;
the Pearl Abyss desaturates from 0.15 at its top (the rest comes with the pressure on each
seat) and the ocean's water is the submarine's rather than a field drag; the Pearl Abyss and
Geyser Fields rocks were retinted to stay distinct from every other Descent's; the Leviathan
is the one enemy past a 20-pixel radius and keeps its own path (out of the generic body
sweep, held by the bot fight instead); the frost mite's radius is 6; the unknown-tile-code
probe moved past the new cave codes; and the "rig systems go on sale once felt" tests use the
Gravity Anchor, the first gate system still sold now the Cryo Hull is the Leviathan's.

### Descent probe

`descent_report` on the registry (default seed): Descent IV ends at **33.0 h** (Ocean 25.0,
Glaciers 26.8, Geysers 28.6, Merrow City 30.6, Pearl Abyss 33.0; the task's target 31-40 h, §14
26-36 h); the Cryo Hull is won at 31.1 h; Descent IV's longest gap between new things is 17
minutes (13-17 per stratum), no deaths; the whole run 73.0 h.

### Seams and known limits

- The expedition's logs 31-40 belong to the story track: this track's story beats are its own
  lore cards, and `set_piece_bounds` lists the set pieces the story's sites should keep off
  (`expedition::sites::crosses_strata_set_pieces` does not read it yet: the integrator wires
  it).
- Merrow City's buildings use the cloister style (pale stone and blue tile) rather than a
  coral-and-shell style of their own; its residents are drawn as merrow.
- Pilots on foot swim as before; currents, geysers and frost act on rigs only.
- The Leviathan moves on its own path and passes over terrain inside its arena (the arena is
  open water); the probe still models the fight as fixed minutes.
- Breaching a big basin's side drains it through the budgeted fluid pass (slow levelling of
  wide bodies, see "Wave 7 fluids").
- Weapons hit the Leviathan within its 44-pixel hit circle while its drawn body (and contact
  box) is about 184 x 56: auto-aim and shots at its trunk land, a manual shot at the tip of
  its head or tail passes by. A hit shape of its own would touch every hit test.
- The Underground Ocean is many separate basins (one region stamp per 72 x 48 at most), not
  one connected sea. (The Hollow Seals, tier 4, would hold the whole abyss, but the depth-line
  cap keeps them locked until a rig has reached Descent V, so the Cryo Hull is what lifts it.)

### Wave 9 drowned review

An independent review re-ran the probe (same numbers: Descent IV ends at 33.0 h, Cryo Hull at
31.1 h, gaps 13-17 minutes) and the screenshot scenes, and fixed: submerged rigs are drawn
again over the water's tint and the deep's dark (a submarine all but vanished in every
Descent IV scene); the eel-dragon is drawn half again larger, a mini-boss, its contact box
grown to `(54, 15)` to match; the hull-frost pass skips warm rigs with no frost.

## Wave 9 hollow: Descent V, the Hollow World (`src/hollow.rs`, `src/hollow/fields.rs`, `src/world/hollow.rs`, `src/world/enemies/hollow.rs`, `src/worldgen/descent/hollow.rs`)

The Hollow World (rows about 5,800-7,600 of a V11 world, §14 Descent V) stops being a
placeholder. The planet is hollow: a rig breaks through a rock sky into a giant open cavern
with its own inner sun and its own day. Every stratum has a signature rule introduced gently
and pushed deeper, its own rock (a `TileKind::Hollow` family), palette, ores and layer
resource, native creatures with their own behaviour, set pieces, a music bed and an ambience
bed, and the Descent ends at the Sun Serpent, whose Gravity Anchor opens Descent VI.

### The strata (registry, `descent/registry.rs`)

| Stratum | Signature rule (pushed deeper) | Creatures | Set pieces | Rock, ores, layer resource | Music, ambience |
|---|---|---|---|---|---|
| Inner Sky | the **breakthrough** (reveal: wide camera easing in, light flood, card, sting); **wind bands** (`hollow::wind_at`) carry a flying rig, calm under the rock sky and gusty deeper; the **inner day** | sky manta (wind-glider, peaceful until struck), stoop roc (sees only what passes below, dives; roosts at night), gust bladder (drifts on the wind, bursts into a shove) | the cavern (base layer), a **sky-stone archipelago** (`sky_stone`, denser in "Stone Archipelago"), the Fathers' **Breach** (`breach`) | sky stone, diamond/platinum/gold; Sky Amber (`SkyAmber` in the stones' skins) | `InnerSky` / `Wind` |
| Megafauna Jungle | **riding**: the pilot climbs onto a crestback (`PilotAction::Use`) and runs faster, jumps higher, no fall damage; **tar** floors grip a rig; fangrunner packs hunt at night | crestback (grazes by day, sleeps at night, flees fangrunners; rideable), skyneck (towering, stamps if hurt), fangrunner (packs circle, then leap together on a shared world-time rhythm; see farther at night) | **jungle halls** (`jungle_open`, bigger deeper), **giant trees** (trunks floor to roof, crowns under the roof), tar pits ("Tar Pits" variant), giant bones, the **Sunfolk Kraal** in its valley | loam, emerald/gold/ruby; Giant Hide (bones) | `MegafaunaJungle` / `Jungle` |
| Upside-Down Isles | **local inverted gravity**: under each hanging isle gravity points up (`hollow::fields::isle_field_source`); rotation is free stratum-wide and the player flips the rig with its thrusters: feet first is cheered (`LandedUpsideDown`), roof first dents the hull (`RoofLanding`, `ROOF_LANDING_DEGREES`) | hangclaw (hangs still under an isle, drops on rigs below, climbs back), lode mite (clings to a rig, drags and nibbles until the rig spins hard: `SHAKE_SPIN`) | the **isle sea** (`isle_sea_rows`), **isles** (`Isle`: flat undersides with hanging grass, crowned tops, stalks to the roof for the upper ones, lodestone) | isle rock, mythril/platinum/diamond; Lodestone | `UpsideDownIsles` / `Updraft` |
| The Sunforge | **overexposure**: in the inner sun's light (nothing solid below for `SHADE_SCAN_ROWS`, along the day's slant) a rig's exposure climbs (more by day, twice at the bottom, radiators blunt it), the screen blooms, past the scorch line the hull burns; floating stones' **shade** cools it, so routes hop shadow to shadow | cinder drake (fiery in the light, sluggish and silent in shade), shade stalker (hunts only in shade, never leaves it), flarefly (bursts into heat: exposure and hull) | the **forge sky** (`forge_rows`, small stones close enough to hop), lava lakes and slag, sun glass, the **Sun Well** | sun rock, ruby/mythril/diamond; Sun Glass | `Sunforge` / `Forge` |

`MusicCue::{InnerSky, MegafaunaJungle, UpsideDownIsles, Sunforge, SunSerpent}`,
`AmbienceCue::{Jungle, Updraft, Forge}` and `LandmarkKind::HollowWorld` (generator
`LandmarkGenerator::HollowWorld`) are appended.

### The inner sun's day

`hollow::inner_sun_at(world_millis)` (16 minutes: dawn, day, dusk, night) is a pure function of
the world clock's time (`WorldState::clock().world_millis()`: it saves, replicates and moves
with a skipped night), so every peer and tick rate agrees. It drives the seat's light in the
Hollow World (`render_snapshot`: never black at night), the Sunforge's heat and glare, the
light's slant (shadows lean through the day), the creatures (crestbacks sleep, rocs roost,
fangrunners see farther at night) and the HUD's countdown. Dawn and night are announced
(`InnerDawn`, `InnerNight`) once a crew has broken through.

### Generation (V11 only; `worldgen/descent/hollow.rs`)

One pass in `streamed::assemble` after the Living Dark's (pure functions of the settings and
the tile): the jungle halls, trees, floors, tar and bones; the isle sea and its isles; the
forge sky, its floor and seams; the Breach; the Sun Well (a ring of indestructible builder
stone, radius `WELL_RADIUS`, round the inner sun's body with four shade pillars and four
doorways, grown round Descent V's gate chamber); sky amber and the archipelago. It never carves
the outposts, anomaly rooms or the Sunfolk hall. The registry's rock for each stratum is a
`HollowTile` (`TileKind::Hollow`, codes `0x900`), counted as plain rock by the deposit and cave
passes (`HollowTile::plain_rock`). The story's camps and rooms keep off the open caverns and
set pieces (`expedition::sites::crosses_strata_set_pieces` asks `hollow::crosses_set_pieces`).
**`GOLDEN_V11_DIGEST` changed**; the V1-V10 digests are unchanged.

### The Sunfolk Kraal (the jungle tribes)

A real settlement from the village machinery (residents, routines, market, inn, specialty,
quests, rumours): the underground hall of the reserved site `SUNFOLK_SITE` (one past every placed
site, so its id never meets another), standing on the floor of the Sunfolk valley
(`sunfolk_valley`, 100-150 columns past the outposts' column), wired everywhere Gravel Hollow
is (lookups, features, stamps, cave keep-out). Its people are Deepfolk-cultured for now (a
Sunfolk culture with its own names, art and dialogue is a seam for the content wave).

### The Gravity Anchor (the Sun Serpent's reward)

A rig system with a button (`/`, the arrow-keys seat's Numpad 9, a controller's scanner bumper
held, the touch ANCHOR button under the rotation pair). `LogisticsCommand::Hollow(
HollowAction::ToggleAnchor)` engages it: the rig's own down, as the player has turned it with
the rotation thrusters and snapped to the nearest quarter (`anchor_quarter`), becomes its
gravity for `ANCHOR_MILLIS` (rotation free meanwhile), so it walks on walls and ceilings; it
lets go early on a second press, then recharges for `ANCHOR_COOLDOWN_MILLIS`. Applied in the
rig's movement step (`WorldState::anchor_quarter_for`), so it is predicted like the pose. Never
sold (`UpgradeLock::Boss` through `RigSystem::boss_reward`).

### The Sun Serpent (`world/enemies/hollow.rs`)

Wakes when a rig enters the Sun Well: a head (`EnemyKind::SunSerpent`, drawn half as large
again) and twelve coils (`SerpentCoil`), four carrying glowing **sun scales**. Every point of
its body is a pure function of world time and the fight's replicated state
(`WorldState::serpent_point`: an orbit breathing between `SERPENT_ORBIT_TILES`, each coil
`COIL_LAG_MILLIS` behind the one ahead along the same path), so hosts and clients agree and the
section only changes on the fight's beats. Plain coils turn every hit; a scale cracks when
broken; with all four cracked the serpent **falls from its orbit** (`SerpentExposed`) and its
head takes damage for `SERPENT_EXPOSED_MILLIS`, then the scales blaze back and it orbits
faster. It **lunges** at a marked point after a tell (`SerpentTell`, its head glows) and the
inner sun **flares** after a warning (`FlareWarning`: the sun swells smoothly, no flashing):
rigs it can see burn, the pillars' long shadows are safe (`WorldState::sun_sees`). No
countdown: with no rig in the well for `SERPENT_SLEEP_MILLIS` it sinks back and heals. Its fall
fits every rig with the Gravity Anchor (and later joiners). On a Peaceful world the well yields
its gate unfought.

### Creatures (`enemy.rs`, appended `EnemyKind`s; `world/enemies/hollow.rs`)

Thirteen kinds (`SkyManta`, `StoopRoc`, `GustBladder`, `Crestback`, `Skyneck`, `Fangrunner`,
`Hangclaw`, `LodeMite`, `CinderDrake`, `ShadeStalker`, `Flarefly`, `SunSerpent`,
`SerpentCoil`) with stats, contact boxes sized to their sprites, loot and resistances. They
follow the body rules through the generic state machine (`step_body`, `free_from_rock`,
`keep_off_bodies`, `site_fits`, `CONTACT_MARGIN` / `STANDOFF_PIXELS`, line of sight), with three
hooks: `hollow_notice` (who they see), `hollow_behave` (their own movement: wind-gliding,
night roosts, herd flight, pack circling, sun power, shade-bound) and `hollow_habitat` (hangclaws
under rock, stalkers in shade, the serpent only in its well). A ridden crestback, the serpent
and clinging mites are carried by the Hollow tick instead (`hollow_holds`). Auto-aim spares the
peaceful (`hollow_peaceful`).

### State, formats and events

- `Hollow` (world field, appended shared `SectionKey::Hollow`, saved with serde defaults,
  hashed once non-empty): breakthrough ticks, exposure, anchors, rides, the landing check, isle
  landings, the sun's phase, the serpent's fight. Entity ids and world times only; the lunge's
  mark moves with a frame shift.
- `LogisticsCommand::Hollow(HollowAction)` (refusals `LogisticsRefusal::Hollow`).
- Events: `GameplayEventKind::Hollow { what, player_id, x, y }`, netcode codes
  `540 + HollowEvent::code()` (540-559, all twenty used).
- **`PROTOCOL_VERSION` 17** (this track's one bump; the integrator collapses). No save format
  bump (every new field has a serde default).

### Game side

- Tile art for every Hollow tile (`tile_art/hollow_art.rs`); open air in the three great caverns
  paints transparent over the **hollow backdrop** (`rendering/world/hollow.rs`: a sky with the
  inner sun low over the far ground, parallax far isles, the far side's lights at night; cooler
  and crowded with hanging isles over the isle sea; blazing amber over the forge), the jungle's
  halls show their own back wall. The Sun Well draws the sun's glow, the flare's swell and burn
  with the pillars' shadows, and the serpent's body as one gold band through its coils.
- Thirteen sprites (`pixel_art/sprites/hollow.rs`), a rider sits up on its crestback.
- HUD (`rendering/hollow_hud.rs`): the breakthrough card, and a panel for the serpent's fight,
  sunlight (exposure bar, in light / in shade / scorching), the ride, the anchor (ready, holding,
  recharging) and the inner day's countdown.
- Input and prompts for the anchor on every device (keyboard, both keyboard seats, controller,
  touch); `Prompt::Anchor`, `PadAction::Anchor`, `TouchControl::Anchor`.
- Audio: fifteen patches appended at the end of `patches!` (`audio/patch/hollow.rs`: five music
  beds, three ambience beds, the breakthrough's sting, gust, bellow, roc cry, the serpent's roar,
  the flare's swell, the anchor's thunk), `hollow_cue` and `hollow_voice`. The director's
  `hollow_music` plays each stratum's piece and bed (eight seat loop slots), the Sun Serpent's
  theme in place of the Sunforge's while it is awake near.
- Scenes: `hollow_breakthrough`, `hollow_inner_sky`, `hollow_inner_night`, `hollow_breach`,
  `hollow_jungle`, `hollow_jungle_night`, `hollow_sunfolk_village`, `hollow_ride`, `hollow_isles`,
  `hollow_upside_down_landing`, `hollow_sunforge`, `hollow_sunforge_shade`,
  `hollow_gravity_anchor`, `hollow_sun_serpent`, `hollow_serpent_flare`.

### Tests

`world/hollow_tests.rs` (the inner day at 20-120 Hz; the breakthrough once, dawn and night;
wind alike at 30-120 Hz; riding faster and getting down; inverted gravity and a feet-first
landing at 30 and 60 Hz, a roof landing; exposure in the light and cooling in the shade; the
Gravity Anchor holding a turned rig to a wall, letting go, recharging, the toggle; gust
bladders and clinging mites; the section's replication, hash and JSON; **a bot beating the
Sun Serpent** with a railgun at the Sunforge's arrival hull tier and keeping 20 % of its hull,
the Gravity Anchor won; a Peaceful world's open gate; older worlds untouched),
`worldgen/descent/hollow/tests.rs` (halls, trees, tar; isles and their grass; the well, its
pillars and doorways; the forge's stones; the Breach; the Sunfolk hall and its people),
`hollow::tests`, `hollow::fields::tests`.

### Descent probe

`hollow::authored_content` and `authored_markers` feed the probe. Default seed, this branch
(Descent IV still a placeholder, ending at 30.0 h): the Hollow World leaves the Inner Sky at
32.4 h, the Megafauna Jungle at 34.6 h, the Upside-Down Isles at 36.9 h and the Sunforge at
**39.7 h** (whole run 73.0 h), the longest gap between new things 11-22 minutes per stratum, no
deaths. Once Descent IV's track lengthens its chapter, Descent V ends inside §14's 42-53 h
window; re-check at integration.

### Seams and known limits

- The Sunfolk's culture is Deepfolk for now (names, art and dialogue of their own are content
  work); their memories of the Fathers are for the story track (`expedition::memories`).
- The Breach is a set piece without a log; the story track places Descent V's logs (they keep
  off the open caverns).
- Wind acts on rigs only (pilots and drops are not blown); the field drive's wind is a nudge.
- The Gravity Anchor's gravity applies to the rig, not to a pilot on foot.
- The Sun Serpent's flare damage reads line of sight through tiles (the pillars and walls).
- The descent probe still models the gate fight as fixed minutes.

### Review pass (Wave 9 hollow review)

- The Sunforge's scorch (`EXPOSURE_DAMAGE_PER_SECOND`) is 4.5 (was 2.2): about 1 % of a
  Sunforge-tier hull a second, so a long crossing in the light hurts and the shade routes
  matter. The Sun Serpent bot still wins with 57 % of its hull.
- The Sun Well is an arena on screen: inside and near it the seat's camera pulls back to
  `hollow_hud::ARENA_WIDE` (0.7, eased by distance over `ARENA_EASE_TILES`, so it never
  jumps), the well has its own ember sky behind its open air, the inner sun a drawn body
  (slowly turning rays, no flashing), and the forge's exposure and bloom ease off inside it
  (a flare still glares). Before, the serpent's orbit and lunges came from off screen over a
  flat yellow glare.
- The Inner Sky's inner sun is drawn like a distant sun (no parallax), so it is in sight at the
  breakthrough and wherever the rig flies; before, it hung at the cavern's far horizon,
  hundreds of rows below any view.
- The inner night darkens the scene a little more (light x0.48 at night, was x0.6), still
  readable.
- Probe re-run on this branch: the same numbers as above (Inner Sky 32.4 h, Jungle 34.6 h,
  Isles 36.9 h, Sunforge 39.7 h, whole run 73.0 h); the probe does not model exposure.

## Wave 9 strange-a: Descent VI part A, the distortion strata (`src/distortion.rs`, `src/distortion/{mirror,shear,rot,fields}.rs`, `src/world/distortion.rs`, `src/world/enemies/distortion.rs`, `src/worldgen/descent/distortion.rs`)

The first three Strange Layers of a Deep Descent world (rows about 7,600-8,600 of V11, §14
Descent VI) stop being Wave 7 placeholder region cells. Reality thins near the builders'
machine, and each stratum bends one thing a player trusts: which way is left, which way is
down, and how much of the world can be seen. Each rule starts gently near the stratum's top and
is pushed by its bottom. Nothing here runs before V11 (`distortion::has_distortion` is
`has_descents()`): the V1-V10 digests, hashes and replays are unchanged.

### The strata (registry, `descent/registry.rs`)

| Stratum | Signature rule (gentle, then pushed) | Creatures | Landmark | Ores, layer resource | Music |
|---|---|---|---|---|---|
| Mirror Strata | **mirror halls** (`distortion::mirror`): halls where the screen, the stick, the rotation thrusters and the aim mirror; their rock is symmetric about the axis to the tile; framed by silvered glass in the rock and a shimmering curtain on screen. A quarter of the 64x40 cells hold a narrow hall near the top, most cells a wide, tall one near the bottom. **Mirror twins** step out of the glass and copy a rig's moves and shots, mirrored. **Mirror ore** lies in pairs at mirrored columns, one real, one reflection | mirror twin, glassback, gazer | the Looking-Glass Hall (its glass mirror on the axis, a strongbox and its reflection); **Parallax Station** (the Descent VI outpost and its research settlement) | silver, diamond, platinum; Mirror Ore | `MirrorStrata` (a palindrome) |
| Gravity Shear | **shear halls** (`distortion::shear`): rooms whose gravity turns 90 or 270 degrees (walls) near the top, 180 (ceilings) from a third of the way down, any of them at the bottom, with plated turned floors, shelves, a prize hanging at the far end and flow-stone frames; flow lines on screen. The rig turns itself with its rotation thrusters (free rotation in the stratum); the camera follows the rig | tumbler, lodestar, shear mantis | the Shear Spire: a hollow tower where down is the nearest wall, a cache on its central plinth | platinum, mythril, uranium; Shear Stone | `GravityShear` (a turning arpeggio) |
| Pixel Rot | **rot tiers** (`distortion::rot`): five bands of rows, each lower-resolution than the last: bigger pixels, fewer colours, glitching bands (the new `ScreenEffect::Glitch`); from the second tier the rock itself is quantized into blocks 2-5 tiles on a side (blocky voids, rotten blocks), with a scanline of rotten blocks where the resolution first drops. Whole **data ore** blocks pay a data core | glitchling, bitworm, dead pixel | the Dead Screen: a builder picture in giant pixels on its chamber's back wall (the cable from the hanging stone to a sleeping star) | uranium, mythril, diamond; Data Ore | `PixelRot` (a chiptune that bit-crushes tier by tier) |

`MusicCue::{MirrorStrata, GravityShear, PixelRot}`, `AmbienceCue::{Glass, Shear, Static}`,
`LandmarkKind::Distortion` (generator `LandmarkGenerator::Distortion`) and
`FieldRegions::Authored(AuthoredRegions)` are appended. Seed variants: "Glass Halls" (more glass
lining), "Twin Galleries" (more halls), "Silver Seams" (more ore pairs); "Turned Halls" /
"Shear Lines" (more shear halls); "Low Res" (more data) / "Glitch Fields" (more rotten blocks).
The Descent VI outpost is renamed **Parallax Station** (`logistics::sites::OUTPOST_NAMES`).

### Generation (V11 only; `worldgen/descent/distortion.rs`)

One pass in `streamed::assemble`, after the Living Dark's and before the Old Deep's, only over
chunks near the three strata's rows; a pure function of the settings and the tile.
`plan_for(chunk)` gathers the halls, regions and landmarks touching the chunk and
`distortion_tile` answers per tile. The Descent landmarks (outposts, gate chambers, anomaly
rooms) are left as stamped and every hall keeps clear of them; mirror halls keep 96 columns
from the outposts' column (Parallax Station, its settlement and the cable stay plain).

- New tile family `TileKind::Warped(WarpedTile)` (codes `0x1600 | code`, clear of every other
  family): `MirrorGlass`, `MirrorOre`, `MirrorGhost`, `ShearStone`, `ShearPlate`, `DataOre`,
  `RotBlock`, `ScreenPixel`. Hardness 2-6; items: mirror ore, flow stone and plates, data ore
  and screen pixels yield their stratum's `ItemKind::Layer`; glass and reflections yield
  nothing.
- **Parallax Station's settlement** (`distortion::parallax_station`, a village filed under
  `distortion::PARALLAX_SITE` = `MAX_SITE + 6`, so no generated village shares it): an
  underground hall beside the outpost on the spawn side, sharing its floor, a plated deck with
  mirror glass set in it and skylights, built and peopled by the village engine (Deepfolk, named
  "Parallax Station"). Wired into every `worldgen::streamed::villages` lookup next to Gravel
  Hollow, and into `village::layout::identity`.
- The expedition's camps and anomaly rooms keep off the halls, the Spire, the Dead Screen and
  the settlement (`expedition::sites::crosses_strata_set_pieces`).
- **`GOLDEN_V11_DIGEST` changed** (now `0x2333_2b2f_bbba_c172`); V1-V10 digests unchanged.

### Fields (`distortion::fields`, through `descent::fields::with_regions`)

The registry gives each stratum `FieldRegions::Authored(..)`; the stratum field source hands
its profile to `distortion::fields::authored`:

- **Mirror halls**: `mirror_controls` and a whole-screen `Mirror` level, feathered over 3 tiles
  at the hall's side edges only (its floor and ceiling are rock, so a rig standing in a hall is
  fully mirrored and never sees a half-faded double image).
- **Shear halls**: gravity turned to the hall's direction with a little heat-haze wobble,
  feathered over 4 tiles; the Spire's gravity points at its nearest wall.
- **Rot tiers**: pixelate 16-52 %, palette 12-48 %, glitch 4-30 % by tier, easing over 6 rows at
  each tier boundary (no pop).

Pure functions of the settings and the position (cached layout), so hosts and predicting
clients agree.

### Mechanics (`world/distortion.rs`, after the Living Dark's pass in `simulate_tick`)

- **Mirrored controls and aim** (`WorldState::mirror_controls`): wherever the fields mirror (or
  a gazer holds the rig) `simulate_tick` swaps the stick, the rotation thrusters and the weapon's
  horizontal aim, so a mouse or stick aimed at the mirrored screen's right fires at the world's
  left, under the cursor. (This also applies to the Wave 8 mirror anomaly room.)
- **Mirror twins**: a rig well inside a hall (field weight 0.6, 3+ tiles from the axis) that is
  not resting gets a twin at its reflection (`TwinAppeared`). The twin moves with the rig's
  velocity mirrored, plus a gentle pull back onto the exact reflection (2.5/s, at most 90 px/s),
  so rock on its side of the axis strands it while the rig flies on. It bites on contact, and
  every `WeaponFired` of its rig this tick is copied as a `MirrorShard` from the twin along the
  mirrored direction (`TwinFired`; bombs and charges are not copied). It fades when its rig
  leaves the hall or goes 40 tiles away (`TwinFaded`); a broken twin's hall rests 45 s
  (`Distortion::twin_cooldowns`, saved as pairs).
- **Mirror ore**: real ore yields Mirror Ore and a mythril (`distortion_dig_extras`); a
  reflection gives at once, nicks the hull (1.2 x the depth's creature scale) and pays nothing.
  The tells are the renderer's: a reflection has no shadows and no rim shade, a cooler cast, its
  glint on the far side, and it breathes with a slow cold sheen in the world view.
- **Gazers**: once one notices a rig its eye opens over 0.8 s, then while it sees the rig the
  rig's controls mirror and its screen too (`Distortion::gazed`, a 450 ms hold refreshed while
  the gaze lasts, through `WorldState::distortion_view_fields`).
- **Data cores**: the opened-tile journal is read each tick; the last tile of a data ore block
  (tier 1+, a whole block on the rot grid, `rot::block_at`) pays three Data Ore and a mythril
  however it was broken (`DataCore`).
- **Rot pulses**: a dead pixel's `RotPulse` shot rots the struck rig's view for 2.5 s (extra
  pixelation, palette and glitch, `Distortion::rotted`).
- Cues: `MirrorEntered` / `MirrorLeft`, `ShearEntered`, `RotDeepened` (each deeper tier, once per
  rig), and the crew's first find of each landmark (`DistortionLandmarkFound`, its story beat).

### Creatures (`enemy.rs`, appended; `world/enemies/distortion.rs`)

| Kind | Body | Behaviour | Counter |
|---|---|---|---|
| `MirrorTwin` | flyer, its own drive | the rig's mirror image: moves and fires as the rig does, mirrored | strand it on rock its side of the axis; shoot from where its copy hits a wall; bombs |
| `Glassback` | walker, lunge | its mirrored face throws kinetic and piercing hits back as a shard (`glassback_reflects`) | flank it, hit it from above, blasts |
| `Gazer` | stationary | its gaze mirrors the controls of the rig it sees | break line of sight, or shoot with the aim reversed |
| `Tumbler` | rolls under the local gravity | rolls downhill toward its prey along whatever the floor is; a shear boundary changes its way | step off its line; lure it across a boundary |
| `Lodestar` | flyer | drags every rig within 7 tiles (clear line) toward itself, up to 110 px/s at point blank, never into rock | thrust away, shoot it |
| `ShearMantis` | walker, dormant | climbs to the ceiling (against the local gravity), drops along gravity on a rig passing under | look up, whichever way up is |
| `Glitchling` | flyer, teleports | shows a ghost where it will land (on the rot grid beside its prey, `EnemyState::waypoint`) for 0.52 s, then is there and bites | move off the ghost |
| `Bitworm` | flyer on the grid | runs straight lines along the axes, square turns toward its prey or off a wall | step off its line |
| `DeadPixel` | slow flyer, ranged | slow rot pulses that rot the view | dodge, blasts |

All follow the body rules (`step_body`, `free_from_rock`, `keep_off_bodies`, contact boxes sized
to their sprites, `CONTACT_MARGIN` / `STANDOFF_PIXELS`, line-of-sight noticing; none digs).
Shots `MirrorShard` and `RotPulse` are appended. Twins never roam (the director's cave habitat
says no); the rest roam their strata's caves.

### State, formats and events

- `Distortion` (world field, appended shared `SectionKey::Distortion` in `SectionKey::SHARED`,
  saved with serde defaults, hashed only once non-empty): gazes, rot pulses, hall cooldowns
  (pairs), landmarks found, the deepest rot tier each rig has seen, rigs inside a hall / a
  turned field. A leaving player's entries are forgotten.
- **`PROTOCOL_VERSION` 17** (one bump for the track: the warped tiles, nine creatures, two shots,
  `ScreenEffect::Glitch`, the section and the events); **no save format bump** (serde
  defaults; `ScreenEffects::levels` already loads shorter lists).
- `field::ScreenEffect::Glitch` and `descent::ScreenEffect::Glitch` appended
  (`SCREEN_EFFECT_COUNT` 15).
- Events: `GameplayEventKind::Distortion { what, player_id, x, y }`, netcode codes
  `560 + DistortionEvent::code()` (560-577), and `DistortionLandmarkFound { landmark,
  player_id }` (579).
- Dev seam: `WorldState::mark_distortion_landmarks_found` (before play: scenes).

### Game side

- Tile art (`tile_art/distortion_art.rs`): diagonal glass panes; real and reflected mirror ore
  (see the tells above); streaked flow stone and riveted shear plates; data ore in cells that
  grow with the rot; flat rotten blocks; Pixel Rot's plain rock as flat blocks that coarsen with
  the tier; the Dead Screen's picture on its chamber's back wall. Back walls of their own: a dim
  hall of mirror panes, strata tipped on their side, a coarse grid of dark squares.
- World overlays (`rendering/world/distortion.rs`): mirror halls' silver curtains with drifting
  sparkles and a faint axis line; breathing reflections; flow lines streaming the way gravity
  pulls in every shear hall and the Spire; the creatures' tells (a twin's shimmer, the gazer's
  open eye, a lodestar's contracting rings, a glitchling's landing ghost, a bitworm's square
  segments, a dead pixel's fringe). Slow (under a cycle a second) and scaled by the effects
  setting where they are decoration.
- Post-process: a **glitch** pass (horizontal slices slipping sideways with a small colour split,
  new slices at most 1.5 times a second, no brightness change); Reduced lets a quarter through.
  The **mirror** stays whole under Reduced (a half mirror is a double image, the harder thing to
  look at); Off turns the screen mirror off while the controls stay mirrored (gameplay).
- Nine sprites and two shots (`pixel_art/sprites/distortion.rs`); ten audio patches appended at
  the end of `patches!` (five one-shots: a reversed glass chime, a shatter, a gravity groan, a
  bit-crushed glitch, an eight-bit data chirp; five loops) and the director's
  `distortion_music`, Pixel Rot's bed crossfading clean -> crushed -> ruined by tier
  (`distortion_bed_mix`).
- News lines per seat (`local_game::announce_distortion`) and a "A STRANGE PLACE" story card
  (`StoryMomentKind::StrangePlace`) when the crew finds a landmark. No new actions or prompts:
  every seat (keyboard, gamepad, touch, split-screen, online) plays it with the inputs it has.
- Scenes: `distortion_mirror_hall`, `distortion_mirror_hall_reduced`,
  `distortion_looking_glass_hall`, `distortion_gazer`, `distortion_shear_wall`,
  `distortion_shear_ceiling`, `distortion_shear_spire`, `distortion_pixel_rot_shallow`,
  `distortion_pixel_rot_deep`, `distortion_pixel_rot_deep_reduced`, `distortion_dead_screen`,
  `distortion_parallax_station`.

### Tests

`world/distortion_tests.rs`: a hall flips the screen, stick and aim (and Parallax Station's
column never mirrors); mirrored steering is the same at 20 and 60 Hz and on replay; a twin
steps out at the reflection, moves mirrored, copies a shot and fades when its rig leaves; a
broken twin's hall rests; a gazer mirrors a rig's controls and screen and the hold lapses; real
mirror ore pays mythril and the layer resource, a reflection shatters and nicks the hull; shear
halls and the Spire turn gravity as declared and a rig falls onto a hall's turned floor; Pixel
Rot's levels rise tier by tier and ease across a boundary; a glitchling shows its landing and
lands there on the grid; a whole data block pays a data core; each landmark is found once; the
section replicates and older worlds stay untouched; a lodestar drags a rig; a mantis hangs and
drops; a glassback throws frontal shots back and is hurt from behind; the creatures keep the
body rules in a fight; Parallax Station is a peopled settlement on the outpost's floor.
`worldgen/descent/distortion/tests.rs` (symmetric halls with paired ore, halls crowding and
widening deeper, the Looking-Glass Hall's strongbox and reflection, plated turned floors, the
Spire, quantized blocks with data, the Dead Screen, older worlds; an ignored ASCII dump),
`distortion/tests.rs`, the registry/field/landmark tests updated for the authored strata,
netcode `world_mirrored_controls_predict_and_converge` (a mirror field over the bots online, no
hash mismatch) and the director's
`the_distortion_strata_play_their_beds_and_pixel_rot_crushes_deeper`.

### Descent probe

`balance::descent::registry_markers` adds each stratum's authored rules and places
(`distortion::novelties`) as markers, and `RegistryStrata` counts `distortion::authored_content`
and `LandmarkKind::Distortion` in each stratum's content minutes.

### Seams and known limits

- The Gravity Anchor (Descent V's reward, the Hollow World track's) does nothing special in the
  Gravity Shear yet; it could pin a rig to a turned floor.
- A lodestar's drag moves the rig's position on the host and in the predicting client alike, but
  a client sees the lodestar a frame late, so a dragged rig gets small corrections (like enemy
  knockback).
- A gazer's hold reaches clients through the replicated section, so a client predicting its own
  rig mirrors its stick a round trip late at the gaze's start and end (corrected on reconcile).
  Mirror halls themselves are pure fields and predicted exactly.
- Twins copy hardpoint weapon fire only (`WeaponFired`); bombs and charges are the counter.
- Parallax Station's people are generic Deepfolk villagers; researchers' own routines, errands
  and the Fathers' memories there (the story track's `memories`) are open.
- Other deep settlements of Wave 9 must file under other sites than `PARALLAX_SITE`.
- The descent probe was not re-run in this track (the integrator re-runs it after stacking the
  six tracks).

### Review fixes (w9-strange-a-review)

- **Leashed glitchlings and bitworms go home.** A glitchling had no `Return` arm, so once its
  leash caught it, it hung in place for good (too far from home ever to hunt again). It now
  blinks home and settles; a bitworm runs the grid home at full speed and settles there.
- **The aim reticle is reflected with mirrored controls.** The simulation reverses a mirrored
  rig's sideways aim, so the reticle is drawn reflected about the muzzle too
  (`WorldState::rig_controls_mirrored`, the one predicate for `simulate_tick` and the game's
  `weapon_aim::reticle`). It used to sit on the far side from where shots went.
- **The glitch pass is mediump-safe.** The band hash used `sin(..) * 43758` with a step up to
  1000 x 78, past the 2^14 range a GLES2 `mediump` float guarantees on phones. The step now
  reaches the shader as a 0..1 phase and the hash uses small constants.
- New tests: a tumbler falls sideways onto a wall hall's turned floor, a dead pixel's pulse
  rots the view of the rig it hits (and wears off), leashed glitchlings and bitworms go home,
  and a mirrored rig's reticle is reflected (game).
- Still open after review: the Gravity Shear's flow stone (pink hatching) reads loud next to
  the stratum's purple rock. A mirror hall's top and bottom edges are rock, so a rig that digs
  in through them flips at once (the glass in the frame is the only warning). The Spire scene
  still catches the camera mid-turn.

## Wave 9 strange-b: Descent VI part B, the far Strange Layers (`src/far_strange.rs`, `src/world/far_strange.rs`, `src/world/enemies/far_strange.rs`, `src/worldgen/descent/far_strange.rs`)

Expansion plan §14, Descent VI strata 4-6 and its gate: near the builders' machine, time, sound
and light come apart. Everything is V11 only (`has_descents()`); V1-V10 digests, hashes and
replays are unchanged.

### The strata (registry, `descent/registry.rs`)

| Stratum | Signature rule (gently, then pushed) | Creatures | Set pieces | Ores, resource | Music / ambience |
|---|---|---|---|---|---|
| Time Sludge | **slow pools** (time 0.62 at its top to 0.32 at its bottom) and **fast lanes** (1.3 to 1.85): generated pockets that grow commoner and stronger deeper ("Slow Pools" / "Fast Lanes" lean the mix); the stratum itself runs at 0.85. **Time bubbles** (`ExplosiveKind::TimeBubble`) slow every creature in their bubble to 0.22 | tick mite (lives at 2.1x, crumbles after 24 s), amber sloth (lives at 0.5x; its touch holds a rig in amber, 0.3x for 2.2 s), rewind newt (badly hurt, it rewinds 3 s of position and health, twice, never inside a bubble) | the **Stopped Clock** (a builders' hall striped slow and fast under a handless dial), slow pools rimmed with chronoglass, fast lanes with studded floors | platinum, uranium; Chrono Amber | `TimeSludge` (a lurching music box) / `Clockwork` |
| Echo Caves | **total darkness** (field light 20 per mille; lamps a pinprick, `ECHO_LAMP_SCALE`): a scanner press is a **sonar ping** whose wave lights the cave as it passes and fades (`sonar_reveal`; 14 tiles, 32 with the Sonar Suite). **Blind hunters hear noise** (`rig_noise_tiles`: rolling 3, thrust 9, drilling 12, a ping 26 tiles; nothing at rest) | echo stalker (blind, hears noise along open paths, half as far through rock), shrieker (on finding prey it screams: a free ping and every stalker within 28 tiles of its prey comes), hush moth (drifts to the loudest rig; inside its hush a rig is silent but its pings reach a third as far) | the **Choir** (a hall of echo quartz pillars), echo hollows; **echo quartz** rings when struck (a free 22-tile ping) or drilled | mythril, diamond; Echo Quartz | `EchoCaves` (notes answered by their echoes) / `Echoes` |
| Prism Desert | **colours split** (chromatic aberration up to 560 per mille, half that at its top; a softened slate-and-mauve palette); **colour-keyed walls** (`CaveTile::{PrismRed, PrismGreen, PrismBlue}`, unbreakable) open only in light of their colour: a rig's **lamp filter** (`LampFilter`, per rig, shared state) opens matching walls within 5 tiles in sight (coloured glass lets its colour through), and they close 2.5 s after the light leaves, never on a body | prism scarab (its shell turns red, green, blue every 4 s: x1.6 damage under the matching light, x0.2 otherwise; blasts always land), splitter spire (a fan of one red, one green and one blue bolt; a bolt of the rig's filter colour passes through it), lens wraith (unseen, and never auto-aimed, except under its own colour) | **keyed curtains** (a cave split by one keyed wall, two deeper, a cache behind), **spectrum vaults** (red, then green, then blue nested round a relic cache), the **Rewrite Hall** | diamond, emerald; Prism Sand | `PrismDesert` (a three-way split arpeggio) / `Prism` |
| Gate | **The Glitch** (`EnemyKind::TheGlitch`, a 30x30 sprite drawn at 1.5x) in the Rewrite Hall (Descent VI's gate chamber grown by `ARENA_MARGIN`, builder alloy walls and floor, doors at both ends, alloy ledges): at 80/60/40/20 % health it **rewrites the hall's rule** (`GlitchRule`: gravity up, mirrored screen and controls, time x1.6, sideways gravity with pixel rot), each telegraphed for 3 s (it flickers, cannot attack, cannot be hurt; the HUD names the next rule). It blinks (its ghost shows where 1.1 s first) and sheds glitch shards (two at most) from its third rule. It sleeps and heals after 30 s with nobody in the hall; no countdown | The Glitch, glitch shards | | | `TheGlitch` (bit-crushed, self-rewriting drums) |

Reward: the **Phase Drill** (`RigSystem::boss_reward` is `TheGlitch`; never sold, `UpgradeLock::Boss`),
fitted to every rig of the crew (and to rigs that join later). Only the Phase Drill cuts
**builder alloy** (`CaveTile::BuilderAlloy`, blast-proof): three ragged rows of it under the
Prism Desert's boundary zone across every column (`alloy_seal`) seal Descent VII. The Hall's
own walls are alloy too.

### Generation (`worldgen/descent/far_strange.rs`, a pass of `streamed::assemble` after the Old Deep and before the story)

Stamps per `FAR_REGION` (48 x 36) rolled per stratum, more likely deeper (`stamps_with`,
`stamp_tile`), plus the fixed Stopped Clock, Choir and Rewrite Hall; far rock (chronoglass veins,
quartz specks, crystal); the alloy seal. The time pockets and the clock hall's stripes are a
field source (`far_field_source`, registered in `field::STRATUM_FIELD_SOURCES` after the anomaly
rooms): every reader of `fields_at` feels them, so rigs, pilots, creatures and drops all live at
their time, deterministically and tick-rate safe. The expedition's story pass keeps its camps and
anomaly rooms off these stamps (`crosses_strata_set_pieces`). The Rewrite Hall clears a
derelict's wreckage or a pool from its floor. **`GOLDEN_V11_DIGEST` is updated**
(`0x380f_1e09_eb47_e5dd`); V1-V10 digests unchanged.

### Mechanics (`world/far_strange.rs`, after the Living Dark's pass, before the fluid pass)

- Pings (`sonar_ping` from `PlayerCommand::UseScanner` in the Echo Caves; `push_ping`) live in the
  shared `FarStrange::pings` until their wave has crossed and faded (`sonar_alive`); the renderer
  and `sonar_light_at` read the same pure `sonar_reveal`.
- Keyed walls are rewritten by rule (`Terrain::set_kind_by_rule`, they are unbreakable), and the
  open ones wait in the host-only `FarSchedule::open_keyed` (saved through `pair_list`, never
  hashed or replicated, cleared in client keyframes); clients see terrain ops.
- Creature tempo (`world/enemies/far_strange.rs`): `advance_enemy` runs a creature at its tempo
  (`far_tempo`: its own times a bubble's) by scaling the tick's seconds for its motion and
  moving its timers onto its own clock (`shift_enemy_timers`: the clock advances
  `floor((t + 1) tempo) - floor(t tempo)` ticks a tick), so slow and fast creatures agree at every
  tick rate. Amber multiplies a rig's field time scale in the movement step.
- Hearing (`far_notice`) overrides the generic perception for stalkers and moths; The Glitch and
  its shards know every rig in the Hall.
- Landmarks found (`FarLandmark`) are shared; their story beats go up as cards (below).

### State, formats, events

- `FarStrange` (world field, appended shared `SectionKey::FarStrange`, saved with serde defaults,
  hashed once non-empty): lamp filters, pings, amber, landmarks found, `GlitchState`.
  `FarSchedule` (saved, never hashed or replicated): open keyed walls, newts' memories (local
  points, reframed with the frame), quartz and ping times, shriek times, pockets, speeds.
- Commands: `LogisticsCommand::FarStrange(FarStrangeAction::{CycleFilter, SetFilter,
  BuyTimeBubbles})` (refusals `LogisticsRefusal::FarStrange`), served by the depth logistics
  dialog and, beside a keyed wall, by the **use button** (`filter_swap_here`; the use prompt
  reads "Swap the lamp to green"), so keyboard, pad, touch and every seat have it.
- Appended: six `CaveTile`s (codes 22-27), eleven `EnemyKind`s, four `ShotKind`s,
  `ExplosiveKind::TimeBubble` (item id 11; outposts and town, not the shack), `MusicCue::{TimeSludge,
  EchoCaves, PrismDesert, TheGlitch}`, `AmbienceCue::{Clockwork, Echoes, Prism}`,
  `LandmarkKind::FarStrange`.
- Events: `GameplayEventKind::FarStrange { what, player_id, x, y }`, netcode codes
  `580 + FarStrangeEvent::code()` (580-598).
- **`PROTOCOL_VERSION` 17** (one bump for the track; the integrator collapses Wave 9's). **No save
  format bump** (every new field has a serde default).
- Dev and scene seams: `set_lamp_filter`, `stage_sonar_ping`, `set_far_strange`,
  `stage_glitch_rule`, `far_landmark_at`, `glitch_arena`.

### Game side

- Tile art (`tile_art/far_art.rs`): hexagonal echo quartz, stained-glass keyed walls, builder
  alloy plates with circuit seams, chronoglass; each far stratum's own back wall (honey-gold in
  slow pools, racing streaks in fast lanes, black ripples in the Echo Caves, spectrum sand bands).
- World (`rendering/world/far_strange.rs`): motes and streaks in pockets, the Stopped Clock's dial,
  time bubbles, amber on a rig, the Echo Caves' darkness cut back only by ping waves (cyan rings),
  quartz and a pinprick lamp (the seat's post-process light is lifted there, like the Glow Sea),
  keyed walls' glow and a filtered lamp's colour pool, the Rewrite Hall's glyphs in the rule's
  colour, the rewrite's slow wash and torn scanlines, the blink ghost, The Glitch's torn colour
  channels. Everything that moves is slow and low contrast, scaled by the effects setting.
- Sprites (`pixel_art/sprites/far_strange.rs`): eleven creatures and four shots; scarabs and
  wraiths are tinted their colour; a lens wraith is not drawn for a seat whose filter does not
  match.
- HUD (`rendering/far_strange_hud.rs`): time (slow pool, fast lane, amber, bubbles aboard), noise
  in the Echo Caves, the lamp filter, The Glitch's rule and the rule being written.
- News and cards (`local_game::announce_far_strange`): a landmark's story beat as a card on every
  local seat; The Glitch waking, rewriting (with the next rule) and falling; amber, a shriek and a
  pocket crossing for the seat they concern.
- Audio (appended at the end of `patches!`, `audio/patch/far_strange.rs`): eight one-shots (sonar
  ping with echoes, time bubble, rewind, shriek, quartz ring, keyed glass, The Glitch's rewrite
  stutter and blink), three music beds, three ambience beds and The Glitch's theme
  (`far_strange_music` in the director).
- Scenes: `far_time_slow_pool`, `far_time_fast_lane`, `far_clock_hall`, `far_time_bubble`,
  `far_echo_dark`, `far_echo_ping`, `far_choir`, `far_prism_curtain`, `far_prism_lit`,
  `far_spectrum_vault`, `far_glitch_baseline`, `far_glitch_rewriting`, `far_glitch_gravity_up`,
  `far_glitch_mirrored`, `far_glitch_time_warp`, `far_glitch_sideways`.

### Tests

`world/far_strange_tests.rs`: slow pools and fast lanes bending a rig's fall alike at 20 and
60 Hz, a pocket crossing announced once, a time bubble slowing a creature and lapsing, time
bubbles sold at an outpost, tick mites crumbling at the same world time at 20 and 60 Hz, amber
slowing a rig, a newt rewinding (and not inside a bubble), a scanner ping lighting and fading,
the Sonar Suite's reach and a hush moth's muffling, stalkers hearing a drilling rig but not a
still one, a shriek calling a stalker, quartz ringing, keyed walls opening only to their colour,
closing after, never on a rig, the scarab's colour damage, a spire's bolt passing through the
matching lamp, the wraith unseen, a landmark's beat told once, the alloy seal refusing a drill
and a blast and yielding to the Phase Drill, the Phase Drill boss-won, **a bot with Descent VI
kit (railgun, hull tier 13) beating The Glitch through every rule in order**, each rewrite
telegraphed for the full warning and immune, every rule felt, more than 20 % hull left, the Phase
Drill fitted and the rules restored; a peaceful Hall yields its gate and a lonely Hall sleeps;
the state saves, replicates and hashes and older worlds keep their hashes; determinism; the
creatures keep the body rules in their own arena (they left the shared cave arena like the
Living Dark's). `far_strange::tests` and `worldgen::descent::far_strange::tests` cover the
rules' maths and the generation (every stamp kind rolled, the seal across every column, the
vault's rings, the pockets' field).

### Seams and known limits

- The Sonar Suite: the Drowned track builds the full Sonar Suite; this track's sonar is the
  scanner's ping in the Echo Caves behind `sonar_ping` / `push_ping` / `sonar_reveal` /
  `sonar_light_at`. The integrator unifies them (the suite's longer reach is already honoured).
- The story track owns logs 56-60 (Time Sludge, Echo Caves, Prism Desert); the far landmarks'
  beats are separate cards, and the story pass already keeps its sites off these stamps.
- Strange-a owns Descent VI's outpost (the Mirror Strata) and strata 1-3; the Time Sludge's
  `StrangeLayers` music cue is replaced here only for strata 4-6.
- Tempo slows a creature's motion and timers, not its shots in flight (a bubble does not catch
  bolts).
- Keyed walls opened by one rig's light close for everyone once it leaves; there is no ghost
  outline of an open keyed wall on clients (the schedule is host-only).

### Review (Wave 9 strange-b review)

Fixed:

- **None of the far creatures drew.** The eleven creature sprites, the four shots and the time
  bubble's two sprites were never added to `sprites::ALL`, so the atlas left them out: The
  Glitch showed only as its halo and health bar. Test:
  `rendering::world::enemies::tests::every_enemy_and_shot_sprite_is_in_the_atlas`.
- **The Glitch's rules were barely felt.** Its field zone covered only the hall's inside,
  feathered from that edge, so a rig resting on the floor felt about a quarter of the rule
  and never fell up or slid sideways. The zone now spans the whole hall, walls and floor
  included. Test: `every_rule_holds_at_full_strength_against_the_halls_floor_and_gravity_up_lifts_the_rig`.
  With the rules really felt the bot (which never rotated) lost, so it now works the
  rotation thrusters by hand (feet towards gravity, the stick along gravity's horizontal) as
  a player must, and The Glitch keeps at most **two** shards (`MAX_GLITCH_SHARDS`), not three.
  The bot wins every rule in order with more than a fifth of its hull left.
- **The alloy seal had holes.** The seal skipped built and POI tiles, so a ruin's gate, a
  keycard door or a vault seal in its rows opened a way into Descent VII, and the expedition's
  story rooms could carve through it. The seal now overwrites everything but bedrock and
  `crosses_strata_set_pieces` keeps story rooms off it (`crosses_alloy_seal`). An ancient
  lift's rail can still cross it: it is unbreakable masonry and the lift is restored only from
  its station below. Test: `the_generated_seal_has_no_gap_in_any_column` (2,400 columns of
  three seeds, the generated tiles). `GOLDEN_V11_DIGEST` is unchanged (its chunks miss the
  changed tiles).
- **A hovering rig was silent.** Noise judged thrust from the rig's velocity, so a rig hovering
  or pressed under a ceiling made no noise. The tick now records whose thrusters fire
  (`FarSchedule::thrusting`, from the held controls). Test:
  `a_hovering_rig_is_loud_its_thrusters_not_its_speed_make_the_noise`.
- The registry test let the far strata's own music through, and the Glitch scenes frame it on
  one of its blink spots (it was inside an alloy ledge, behind the rule panel).

The descent probe (`descent_report`, default seed) after the fixes:

| Stratum | Minutes | Leaves at (h) | Longest gap (min) |
|---|---|---|---|
| VI Time Sludge | 136 | 43.9 | 22 |
| VI Echo Caves | 136 | 46.2 | 14 |
| VI Prism Desert | 172 | 49.1 | 14 |

The Phase Drill is won at 46.7 h. Descent VI ends at 49.1 h against the §14 target of 53-66 h
(its first three strata are strange-a's). The whole run is 70.2 h.

Still weak:

- The creatures are small next to the rig (a tick mite is a few pixels). The Choir scene is
  lit and does not show its quartz pillars well.
- The bot needs the rotation thrusters to win: players who never learnt them in the Gravity
  Shear will struggle under the gravity rules.
- The Glitch sleeps if a rig waits in a doorway (the doors are outside the hall's inside), so
  its shards vanish too.

## Wave 9 story: the Fathers in Descents IV-VI, residents and the codex (`src/expedition/{logs,memories,sites,anomaly,codex}.rs`, `src/people/deep_folk.rs`)

Expansion plan §13 (the revelations of Descents IV-VI) and §14's cross-Descent systems, on
the Wave 8 story's tables and passes. Everything is V11-only, deterministic and read from
state the world already saves and shares.

### The trail through Descents IV-VI (`expedition::logs`)

- **Logs 31-60** follow the throughline table above row by row (strata, sites, authors and
  beats), in three chapters: **IV "The People Below"** (the raft, the drowned builder sub and
  the eel-dragon, Dov's fingers, the builder figures laid out in the ice, the geyser ride, the
  merrow city, Sefa stays, the merrow song "the Tall Ones came down from the stars on a
  thread", Dov's failing heart, the Leviathan breaking the sea floor), **V "The Hollow World"**
  (a sun inside the world, Dov's last full tape, the chief's child, the rig crushed and the
  grazer Survey Two, Bran stays, the cable through the inner sun, Dov's last recording, his
  grave, the sun as a builder lamp, the Serpent's locker and its wall-walking anchor) and **VI
  "Where the World Wears Thin"** (the other Ines, two of every canister and why the cipher,
  the only up, low resolution, the plans of the world, a day in an hour, the Glitch and the
  drill through builder metal; below, their city). `CHAPTERS` grows to six; logs stay under
  340 characters and titles are unique.
- **Echoes 13-26**: Dov's ghost tapes run on to the Upside-Down Isles (log 47, his last).
  Below his grave the builders' own voices take over: `EchoKind::Builder` ("A BUILDER ECHO",
  `Echo::voice` is "Builder voices", the text is Ines's margin translation), five of them from
  the Sunforge to the Prism Desert. Once, out of time in the Time Sludge, Dov's voice comes
  back ("Amos buried me three canisters back").
- **Sites**: `SiteKind::Grave` (log 48, Dov's grave in the Sunforge, 10x6, the canister at its
  foot). A site in the Inner Sky stands on a tapering stone of the stratum's hard rock
  (`ExpeditionSite::{footing_rows, in_footing, extent}`; the chunk query uses the extent).
  `sites::DEEPER_SET_PIECES` is the seam the Descent IV-VI tracks append their set-piece
  queries to (`SetPieceQuery`); it starts with the registry's Descent IV-VIII landmarks, so
  logs 1-30 keep their Wave 8 places. The story pass, the journal and the probe read the tables
  only, so placement needed no other change.
- **Game**: the deeper camps are dressed for their Descent (`CampDress`: a raft camp with frost
  on the canvas, fuel drums under the sled and a paddle; a jungle camp under giant leaves with
  a hammock, a tusk and flowers; a thin camp whose mirror twin tent and ghost sled stand faintly
  beside it, its corners rotted to blocks), the grave is drawn (a pale outlined headstone cut
  "D.H." with his radio's wave, his headset hung on it, a mound and sun-flowers, a warm glow
  after the darkness pass), the builder echo has its own card (`StoryMomentKind::BuilderEcho`:
  the ball-on-a-cable glyph and slow rings of sound) and the journal counts builder echoes and
  colours chapters IV-VI.

### The codex (`expedition::codex`, `ModalScreen::ExpeditionCodex`)

The Wave 8 open item: the Crust's strongbox diary and the Old Deep's builder tablets were not
in the journal. They are a page of their own after the numbered journal (the page cycle runs
tomb codex, journal, **codex**, map): **A Hired Hand's Diary** (`crust::CRUST_LOGS`, read
from `CrustState::logs`, with where each strongbox lies) and **Builder Tablets** (the four
tablets and the Star Core's revelation from `OldDeep::logs_read`, then the deeper anomaly
rooms' inscriptions from `ExpeditionState::anomaly_kinds`). Found entries are rows (keys, pad,
mouse, touch), the rest a count; the selected entry's words are pinned under the list. The
tablet texts moved into the simulation (`codex::{BUILDER_TABLETS, CORE_REVELATION}`); the
game's `old_deep_news::deep_log` reads them. The codex has no state of its own: the `Crust`,
`OldDeep` and `Expedition` sections are all in `SectionKey::SHARED`, so partners and reloaded
saves show the same pages (`world::expedition_tests::the_codex_saves_loads_and_reaches_co_op_partners`).
`WorldState::expedition_codex(page)`; `set_codex_finds_for_tests` for tests and scenes.

### Deeper anomaly rooms (`expedition::anomaly`)

`AnomalyKind` gains four kinds found only from Descent IV down (`AnomalyKind::DEEP`,
`DEEP_FROM`; rooms above keep `SHALLOW`, Wave 8's four, so shallow rooms are unchanged):

| Kind | Rule (field) | Lining | Caches | Way in |
|---|---|---|---|---|
| `Sideways` "The Sideways Room" | gravity to the right (`GravityShear` + free rotation, warm grade), hard edge, above a two-row floor strip where down is still down (`SIDEWAYS_FLOOR_ROWS`) | builder pillars | on the right wall | one doorway, in the left wall at the floor |
| `Sunlit` "The Sunlit Room" | exposure, bloom, a warm grade and a little haze | builder brick | on the floor | both side doorways |
| `Weightless` "The Floating Room" | next to no gravity, free rotation, violet grade | volcanic glass | two, under the ceiling | both side doorways |
| `Lensing` "The Bent Room" | lensing and a cold grade (`Lens`) | crystal | on the floor | both side doorways |

Each carries `AnomalyKind::inscription`, a builder line the room's card quotes and the codex
keeps. The game draws their tells (rubble on the right wall and dust streaming to it, light
shafts and a brass sun-disc, drifting stones, rings of bent light round a dark knot), all slow
and steady. `Milestone::EveryAnomaly` stays "all four kinds" (`SHALLOW`), so it is not pushed
past Descent IV. Bots: `a_rig_falls_to_the_sideways_rooms_right_wall_and_finds_its_way_out`
plays the way in and out; `entering_the_deeper_rooms_feels_their_rules_and_fills_the_codex`
checks every rule and the codex.

Review fix: without the floor strip the sideways room was a trap. A rig pinned to the right
wall cannot turn (its corners swing into the wall) and cannot drive off it, and one resting
on the floor under sideways gravity could neither turn nor move. Now the rig lifts off the
strip into the sideways air, falls to the wall, and holding left slides it down the wall into
the strip and out of the door.

### Residents of Cathedral Station and the Amber Court (`people::deep_folk`)

The other Wave 8 open item: both places told their memories only on a card. Each is now a
settlement like the Moon colony: fixed ids `CATHEDRAL_STATION` / `AMBER_COURT` (never a village,
camp, sky town or city), an authored cast of five (`DeepSettlement::cast`: name, trade,
greeting and the memory **speaker key** they speak for), a population from the seed with the
cast's names and trades, and a routine view `Settlement::deep_hall` (underground: shown once
explored; they work and sleep in the hall). `routine::settlement` and the world's
`settlements_near_actors` know them, so they spawn, walk, talk, trade, take gifts, remember
players and join stories like any resident, single-player, split-screen and online alike.

- **Cathedral Station**: Hesper Quill (quartermaster), Oskar Danner (prospector), Lida Marr
  (medic), Corwin Vale (surveyor) and Mags Pell (cook), in the half of the outpost hall away
  from the lift's door, so a rig off the lift meets the counter first.
- **The Amber Court**: Thrum of the Comb (worker elder), Wrench-Thrice of the Nursery (pump
  tender), Hum-Softly of the Brood (nurse), Mandible of the Guard and the Voice of the Queen,
  across the Court's hall (each works their own stretch of it: `Settlement::deep_hall` spreads
  the work spots and the plaza spans the hall, so five people never stand in one heap; review
  fix). They are insects: the game draws them as hive workers half again a
  person's size (banded abdomen, six legs, compound eyes, feelers, mandibles that click as they
  talk, each holding their trade) with portraits of their own, and they keep to their own words
  (no village small talk). Nobody comes out while the Court is hostile. The Court's comb pillars
  are worn through at the floor (`worldgen::descent::living::court`) so they walk bay to bay.
- **Conversations**: a resident's Chat is their greeting, then (station folk) the day's word,
  then **their own** memory of the Fathers in their own voice, newest first
  (`memories::line_by_for_visit`). In the hall a resident in reach takes the use key and the
  prompt before the counter (`shell::deep_resident_in_reach`). The Wave 8 arrival cards for
  these two places are gone; Gravel Hollow keeps its card, and the outposts of Descents IV-VI
  show one until their settlements' people are wired.

### Memories for the deeper settlements (`expedition::memories`)

- Ids are `settlement.speaker.topic`; `SettlementMemory::speaker_key` is the middle part.
  Nine new Cathedral Station and Hive lines open with Descents IV-VI's logs (the quartermaster
  sold Dov the drums, the medic on his fingers, the cook asks you to sing at his grave, the Hive
  elder on the Wet-Singers, the queen's voice on the little sun ...).
- **Merrow City** (Sefa, 6 lines: archivist, singer, dock keeper, child), **the jungle tribes**
  (Bran, 5 lines: chief Ruth-Little, healer, hunter, child) and **the Prism Relay** (Ines and
  Amos passing through, 5 lines: relay keeper, glass cutter, drifter) are appended
  `Settlement`s, each with lines open from the first visit and more as the trail is read.
- **Registration seam** (for the Descent tracks' settlements, the integrator wires):
  `Settlement::place_names` / `of_place_name` find a settlement by its population's name
  ("Merrow City", "Merrow Air Dock", "the Jungle Tribes", "Sunward Camp", "Prism Relay" ...),
  and `Settlement::speaker_for_role` casts their generated people by trade. The world's talk
  (`world/people.rs`) already uses both: any settlement whose name registers remembers the
  Fathers, every other Chat turning to them, in the speaker's own voice where their trade
  speaks for a memory, otherwise passed on with the speaker named.

### Formats, tests and scenes

- `PROTOCOL_VERSION` 17 (the deeper `AnomalyKind`s in events, the residents' NPC entities); no
  save format change (no new fields; `ExpeditionState` keeps anomaly kinds as bits).
  **`GOLDEN_V11_DIGEST` updated** to `0x3927_e260_5846_588a` (the last pinned log site is log
  60's now); V1-V10 digests unchanged. The Court's worn pillars and the deeper rooms lie in no
  pinned chunk.
- Tests: `expedition::tests` (logs and echoes in depth order through Descent VI, the members
  stay behind in order and write no more after, chapters end in their Descent's last stratum,
  rooms of every kind with the deeper ones only from Descent IV, their fields, doors and caches,
  the deeper settlements' lines and registration), `people::deep_folk::tests` (casts, routines
  that keep to the hall day and night, every resident speaks), `world::deep_folk_tests`
  (residents come out at both halls and each tells their own memory in conversation, the hostile
  Court stays shut, partners meet the same people across a save), `world::expedition_tests`
  (the deeper rooms and the codex, the sideways room played, the codex saved, loaded and
  replicated) and the game's `expedition_tests` (the codex page by touch and tabs, residents in
  place of the Cathedral Station card).
- Scenes: `residents_cathedral_station`, `residents_amber_court`, `residents_amber_court_hall`,
  `expedition_raft_camp`, `expedition_sky_camp`, `expedition_jungle_camp`,
  `expedition_dov_grave`, `expedition_thin_camp`, `expedition_codex`,
  `expedition_builder_echo`, `anomaly_sideways`, `anomaly_sunlit`, `anomaly_weightless`,
  `anomaly_lensing` (the Wave 8 `settlement_memory_cathedral` card scene is replaced by the
  residents' scenes).

### Seams and still open

- **Merrow City, the jungle tribes and the Prism Relay**: when the Descent tracks build them,
  name their populations one of `place_names` (or add their names there) and, if their trades
  differ, rows to `speaker_for_role`; then drop that outpost's arrival card in
  `LocalGame::announce_settlement_memories`. Their set pieces go into `DEEPER_SET_PIECES`.
- **The Inner Sky's logs** (41, 42) stand on their own floating stone wherever the slot puts
  them; once the Hollow World track places its floating stones, `placements_for` can move them
  onto one.
- **Per-settlement story arcs** (2-3 quests per new settlement on the people/story engine) are
  not written: the arc engine seeds templates from character traits and binds places classic
  bands describe, so settlement-bound arcs need a place trigger and the deeper settlements in
  the arc tests' binding worlds. The residents already take part in the existing arcs.

## Wave 9 progression: Descents IV-VI economy, supplies, climate, hints and the probe

The progression track of Wave 9 (expansion plan §13, §14 cross-Descent systems, F10).
Modules: `src/workshop.rs` (deep supplies and recipes), `src/logistics.rs` (the supply kit),
`src/world/logistics.rs` (buying, crafting, `take_supply`), `src/depth_progression.rs` (after
"Wave 9": climate, specialties; the boss rewards), `src/balance/descent/mechanics.rs` (the
probe's movement rules and boss fights); tests in `world/wave9_progression_tests.rs`,
`workshop::tests`, `depth_progression::tests`, `balance::descent::{tests, mechanics::tests}`
and the game's `shell::onboarding` tests. **No generation changed**: V1-V11 golden digests,
`GOLDEN_V11_DIGEST` included, stand.

### Upgrade tiers through Descent VI

The ladders already ran to the Heart (Wave 7-8: core caps 15 / 17 / 19 for Descents IV / V /
VI, Pressure Rating and Headlamp one tier per Descent, deep tier prices +135 % a tier). What
changed:
- **The gate systems of IV-VI are boss-won, never sold** (`RigSystem::boss_reward`): the
  Leviathan's Sonar Suite and Cryo Hull, the Sun Serpent's Gravity Anchor and the Glitch's
  Phase Drill, matching the registry's gate rewards. Every shop shows them
  `UpgradeLock::Boss`; the Heart Key stays on sale until Wave 10. **Seam for the Descent
  tracks:** each boss's death must call `economy::grant_rig_system` for its rewards (as the
  Star Core and the Mycelial Mind do), or the next Descent stays shut.
- **Deep settlements' specialties** (`outpost_specialty`, `outpost_price_percent`): Merrow
  City's counter (Descent IV) sells Pressure Rating and Engine tiers, the Sunward Camp
  (V) Radiator and Heat Shielding, the Prism Relay (VI) Headlamp and Drill at
  `OUTPOST_SPECIALTY_PERCENT` (85 %) instead of 115 / 100 %. A counter lists only its own
  Descent's boss prize among the locked rig systems.
- Tried and reverted: steeper deep tiers (+150 % a tier) slowed the Strange Layers by 2.7 h,
  but as a grind (four hour-long trips in the Mirror Strata, a 51-minute wait between new
  things), so the curve stays at +135 %.

### Deep supplies (the economy is this track's, the effect the Descent tracks')

`workshop::DeepSupply` (append only; index = wire and save id), each answering one
mechanic, crafted from its stratum's layer resource and sold at the deep counters:

| Supply | Stratum | Recipe | Sold from | Base price |
|---|---|---|---|---|
| Ballast Tank | Underground Ocean | 5 Deep Pearl -> 2 | Hive Gate (III) | 320 |
| Frost Heater | Brine Glaciers | 5 Brine Ice -> 2 | Hive Gate (III) | 330 |
| Flare Buoy | Pearl Abyss | 4 Abyss Pearl -> 3 | Merrow (IV) | 200 |
| Beast Bait | Megafauna Jungle | 5 Giant Hide -> 3 | Sunward Camp (V) | 260 |
| Shade Canopy | The Sunforge | 5 Sun Glass -> 2 | Sunward Camp (V) | 410 |
| Time-Bubble Bomb | Time Sludge | 5 Chrono Amber -> 2 | Prism Relay (VI) | 450 |
| Echo Charge | Echo Caves | 4 Echo Quartz -> 3 | Prism Relay (VI) | 250 |
| Lamp Filter | Prism Desert | 5 Prism Sand -> 2 | Prism Relay (VI) | 480 |

- Ballast and heaters are stocked a Descent early because the Drowned and Frozen's outpost
  is its fourth stratum (`DeepSupply::sold_from_descent`). Prices scale with the counter's
  Descent (`supply_price`, the outposts' `scaled_price`) and the counter's percent; a recipe's
  resources are worth about what buying its output at its own Descent's counter costs (test).
- **The kit**: `Logistics::supplies` (player -> counts by index, up to `MAX_SUPPLY_STACK` 9
  each; an emptied kit is dropped). `LogisticsCommand::BuySupply { outpost, supply }`
  (`WorldState::supply_offer` checks: inside the outpost, `outpost_sells_supply`, not full)
  and `CraftOutput::Supply(supply, count)` recipes fill it.
- **Seam for the Descent tracks / integrator:** spend one with `WorldState::take_supply(player,
  supply) -> bool` from inside the tick when the mechanic uses it (blow ballast, light a
  heater, throw a bomb, fit a filter). The use action (a `LogisticsCommand` or the track's own
  command) is the integrator's to wire; `WorldState::supplies_of` reads the kit for the HUD.
  If a track names a supply differently, rename it here (names are display only).
- 17 recipes appended to `RECIPES` (34 in all), one or more per stratum of Descents IV-VI:
  the eight supplies and nine parts (Sinter Steam Turbine: Engine; Merrow Coral Seals:
  Pressure Rating; Sky-amber Tank Liner: Fuel Tank; Lodestone Keel Plates: Hull; Sunforged
  Radiator Core: Radiator; Mirror-ore Cargo Racks: Cargo Bay; Shear-stone Drill Crown: Drill;
  Data-ore Scanner Core: Scanner; Prism-ground Beam Lens: Headlamp). One part per line per
  Descent; never a rig system. Layer resource values keep Wave 8's curve
  (`layer_resource_worth`), and new deep ores should take `deep_ore_value(descent, rank)`.

### Climate: fair heat and cold per stratum (`deep_climate`)

| Stratum | Heat | Cold | | Stratum | Heat | Cold |
|---|---|---|---|---|---|---|
| Underground Ocean | 16 | 0 | | Upside-Down Isles | 22 | 0 |
| Brine Glaciers | 8 | 12 | | The Sunforge | 29 | 0 |
| Geyser Fields | 23 | 0 | | Mirror Strata | 24 | 0 |
| Merrow City | 17 | 0 | | Gravity Shear | 26 | 0 |
| Pearl Abyss | 19 | 8 | | Pixel Rot | 27 | 0 |
| Inner Sky | 21 | 0 | | Time Sludge | 28 | 0 |
| Megafauna Jungle | 25 | 0 | | Echo Caves | 18 | 10 |
| | | | | Prism Desert | 30 | 0 |

- Heat units as `crate::protection` counts them; `crust::stratum_heat_units` reads the table,
  so the tick's band heat, the probe and the HUD agree. Fair by test: every stratum is covered
  by its Descent's radiator cap plus at most two Heat Shielding tiers, a Descent's first
  stratum is at most 3 units (a warning) over a rig arriving with the previous cap, and a rig
  with the cap and two shields loses no hull anywhere in IV-VI (a tick test). The placeholder
  heat cap stays for VII-VIII only.
- **Cold** is a seam for the Descent tracks' hull frost: `cold_excess(cold, heater_burning,
  cryo_hull)` (a Frost Heater covers `FROST_HEATER_COVER_UNITS` 12, the Cryo Hull
  `CRYO_HULL_COVER_UNITS` 15), `COLD_DAMAGE_PER_EXCESS_UNIT` 0.5 hull/s (at most 6 hull/s bare).
  Not wired into the tick here, so the Brine Glaciers' track decides how frost bites.
- Pressure stays per Descent (the Pressure Rating line, rated tier = Descent); the gates stay
  the boss fights above.

### First-time hints (`game/src/shell/onboarding.rs`)

Eight more per-seat hints (appended `Hint`s): submarine water (Descent IV+ water or brine),
deep cold (a cold stratum without the Cryo Hull), the inner sky (open air in it), the inner
sun's glare (a Hollow World stratum whose registry light is at least the probe's
`GLARE_LIGHT_PERMILLE`, the Sunforge: the sampled field light stops at daylight, so it cannot
be read there), turned gravity (down not down), a mirror zone, slow or fast time,
and the stratum's deep supply when the kit holds none. They read replicated fields and
sections, so they work online, per split-screen seat and by touch; their words name no key.

### The descent probe (`balance::descent::mechanics`)

Each stratum's registry fields become movement rules: submarine travel where the water is
(drag over 1000, or the share of water and brine pockets; 55 % speed), open sky flown at
9 rows/s and stone-hopping between floating ore, turned-gravity and mirrored cells (the Gravity
Anchor eases turned gravity), slow time (everything takes 1/scale), the dark (the Sonar Suite
eases it) and the glare. Descents I-III keep the Wave 8 model. The first meeting of a rule
costs learning minutes and is new; each stratum's supply is crafted from the bay or bought (one
a trip, two for the crossing); the expedition fuel reads the rules too. **Boss fights** are
modelled (`boss_fight`): the winning attempt, a learning attempt from the Star Core on, and one
more for each hull tier under `boss_ready_hull` (each failed attempt is a death, 40 % of a fight
and two minutes back): Star Core 24 min, Mind 27, Leviathan 33, Sun Serpent 36, Glitch 44.
The pacing table covers Descents I-VI.

This branch (60 Hz, default seed, IV-VI still the Wave 7 placeholder strata):

| Descent | Ends at (h) | Target (h) | Everyday fuel (max % of tank) | Expedition fuel (%) | Deaths |
|---|---|---|---|---|---|
| I The Crust | 6.4 | 5-8 | 49 | 28 | 0 |
| II The Old Deep | 16.0 | 13-18 | 15 | 10 | 1 |
| III The Living Dark | 23.0 | 22-29 | 17 | 4 | 1 |
| IV The Drowned and Frozen | 31.0 | 31-40 | 14 | 4 | 1 |
| V The Hollow World | 39.0 | 42-53 | 15 | 4 | 1 |
| VI The Strange Layers | 51.9 | 53-66 | 14 | 2 | 1 |
| (whole run, VIII) | 69.9 | 75-90 | 12 | 2 | 7 |

| Stratum | Minutes | New things | Mechanics | Longest gap (min) | Check |
|---|---|---|---|---|---|
| IV Underground Ocean | 99 | 20 | 5 | 14 | pass |
| IV Brine Glaciers | 81 | 11 | 4 | 14 | pass |
| IV Geyser Fields | 83 | 6 | 3 | 17 | pass |
| IV Merrow City | 89 | 6 | 2 | 17 | pass |
| IV Pearl Abyss | 126 | 11 | 3 | 18 | pass |
| V Inner Sky | 129 | 19 | 2 | 22 | pass |
| V Megafauna Jungle | 90 | 6 | 1 | 22 | pass |
| V Upside-Down Isles | 120 | 4 | 2 | 40 | FAIL |
| V The Sunforge | 140 | 13 | 3 | 40 | FAIL |
| VI Mirror Strata | 132 | 17 | 2 | 22 | pass |
| VI Gravity Shear | 121 | 7 | 1 | 30 | pass |
| VI Pixel Rot | 108 | 6 | 1 | 30 | pass |
| VI Time Sludge | 135 | 8 | 4 | 22 | pass |
| VI Echo Caves | 115 | 7 | 2 | 29 | pass |
| VI Prism Desert | 161 | 10 | 1 | 29 | pass |

- Descents I-III still pass (the bosses' deaths are one each, within the check's two). The
  modelled rules and fights add 0.7 h to Descent IV, 0.5 h to V and 2.6 h to VI.
- **Open for the integrator:** on placeholder strata no creature in IV-VI is new (their tables
  reuse earlier kinds), so the Upside-Down Isles and the Sunforge wait 40 minutes; the content
  tracks' creatures, set pieces and authored mechanics fill them. **The Hollow World ends 3 h
  early** (39.0 against 42-53) and the Strange Layers 1 h early: the economy does not slow them
  without a grind (the deep income buys each Descent's caps in about one trip a stratum, so the
  time is the crossing and the content). Re-run the probe after stacking (`cargo test --release
  -p fatherload_simulation descent_report -- --ignored --nocapture`); if V is still short,
  give its authored content minutes (the jungle tribes, beast riding, the Sun Serpent's arena)
  as `old_deep::authored_content` does, rather than raising prices.
- Fuel: everyday trips stay calm (under 15 % of the tank); expeditions are still not
  stressful past Descent II (the deep tanks dwarf a Descent's climb, even with the inner
  sky flown both ways).

### Formats, scenes and seams

- `PROTOCOL_VERSION` 17 (`LogisticsCommand::BuySupply`, `Logistics::supplies`; the integrator
  collapses it with the other tracks'). No save format bump: the kit is a serde default, and
  an old save loads with an empty kit (test). No new events or audio patches (the dialog's own
  sounds and toasts).
- Game: the logistics dialog's **Deep supplies** section (`LogisticsGroup::Supplies`) and
  **Supply kit** status, supply recipes in the workshop (this Descent's recipes first), the
  inventory's kit lines, eight 9x9 supply icons (`sprites::deep_supplies`, appended to the
  atlas). Scenes: `deep_counter_supplies`, `deep_counter_shop`, `deep_workshop_supplies`.
- Shared-file edits (all additive): `logistics` (the field, helpers and appended command),
  `world::logistics` (appended refusals and arms), `depth_progression` (boss rewards, the
  placeholder cap's range, the Wave 9 section), `crust::stratum_heat_units` (the climate arm),
  `workshop` (appended recipes, `CraftOutput::Supply`), `world/mod.rs` (the test module), the
  netcode protocol doc, and in the game the dialog, HUD, onboarding, sprites and scenes.

## Wave 9 integration: Descents IV-VI stacked and wired

The six Wave 9 tracks (drowned, hollow, strange-a, strange-b, story, progression) are stacked
in that order on one linear branch, every commit compiling and passing clippy. What the
stacking decided, and the seams wired on top:

### Shared files and formats

- **Appended enums in track order**: `EnemyKind` (drowned, hollow, distortion, far kinds; 118
  in `ALL`), `ShotKind`, `MusicCue` / `AmbienceCue`, `LandmarkKind` / `LandmarkGenerator`,
  `SectionKey` (`Drowned`, `Hollow`, `Distortion`, `FarStrange`, all in `SectionKey::SHARED`),
  `StoryMomentKind`, the game's `SoundCue` / `Patch` / `SeatLoop` (audio patches appended at the
  end of `patches!` in track order) and the sprite atlas.
- **Tile codes**: the drowned and far tracks both appended `CaveTile`s from code 22. The far
  Strange Layers' six now take codes 35-40 (`EchoQuartz` .. `Chronoglass`); the Drowned and
  Frozen keep 22-34. `TileKind::Hollow` (`0x900`) and `TileKind::Warped` (`0x1600`) families do
  not overlap any other range.
- **Village sites**: Merrow City and the Sunfolk Kraal both took `MAX_SITE + 1`; the Sunfolk
  Kraal is now `MAX_SITE + 2` (Parallax Station keeps `MAX_SITE + 6`). The fixed deep halls
  (`streamed::villages::deep_halls`) are Gravel Hollow, Merrow City and the Sunfolk Kraal;
  Parallax Station keeps its own hall code.
- **Event codes**: drowned 520-539 (all twenty used), hollow 540-559, distortion 560-579, far
  580-599, story 490-493 (no new codes), progression none.
- **Sounds**: both Descent IV and the Echo Caves named a `SonarPing` cue and patch; the Echo
  Caves' is `SoundCue::EchoPing` / `Patch::EchoPing` (`"echo_ping"`).
- **`PROTOCOL_VERSION` 17**, one bump for the wave (the netcode doc lists every track's wire
  change). **No save format bump** (still 14): every new world and player field has a serde
  default, tile-keyed maps (`Distortion::twin_cooldowns`, the far track's keyed walls) save as
  `crate::pair_list`, and an older save loads with every Wave 9 section empty (test).
- **Generation**: only V11 changed. `GOLDEN_V11_DIGEST` is `0x4e9d_92cc_a15c_769d`; the V1-V10
  digests are unchanged.
- Small fits where tracks met: the far track's tempo wrapper around `advance_enemy` keeps the
  distortion track's `start_kind` dispatch and the hollow track's held creatures; enemy damage
  multiplies the far track's time scaling and the Leviathan's maw, then the Sun Serpent's
  armour; the weapon lock skips the Living Dark's peaceful folk, the Hollow World's mounts and
  the far track's unseen creatures; the far track's `FieldRegions` reading in the probe learnt
  the distortion strata's `Authored` regions.

### Seams wired

- **One Sonar Suite** (`world/sonar.rs`): `sonar_medium` (the Echo Caves' dark, or water and
  any Descent IV stratum), `sonar_reach_tiles` (further with the Leviathan's Sonar Suite in
  both), `sonar_ping` and the scanner's one hook `sonar_scanner`, and one light,
  `sonar_light_at` / `sonar_light_at_tile`, the brightest of both media. Each medium keeps its
  own pings in its own section (`drowned_sonar_*`, `echo_sonar_*`); no state was added.
- **The story in the new strata**: the Drowned and Frozen's set pieces joined
  `sites::DEEPER_SET_PIECES` (the hollow, distortion and far tracks had added theirs to
  `crosses_strata_set_pieces`). The Hollow World's open skies (the Inner Sky, the isles' sea,
  the Sunforge's sky) no longer count as set pieces: log sites there stand on the story's
  floating stone (`ExpeditionSite::footing_rows`), and placement tries more rows and columns
  after the old tries, so all sixty sites keep off every set piece on every test seed.
- **Settlements remember the Fathers**: Merrow City, the Sunfolk Kraal and Parallax Station are
  registered by their built names (`memories::Settlement::place_names`), so their generated
  people tell the Fathers' memories in conversation (in their own voice where their trade
  speaks for one); the outposts' arrival cards are gone (Gravel Hollow keeps its card).
- **Progression against the gates**: the Sonar Suite and Cryo Hull from the Leviathan, the
  Gravity Anchor from the Sun Serpent and the Phase Drill from The Glitch are each granted by
  the boss's death and never sold (`wave9_progression_tests`, the three bot fights).
- **Deep supplies spent where they bite** (no new input; works per seat, online and by touch):
  an **Echo Charge** gives a rig without the Sonar Suite one ping as long as the Suite's in the
  Echo Caves; a **Frost Heater** burns the hull frost off as it reaches the warning line; a
  **Shade Canopy** cools a rig back down as the Sunforge's light starts to scorch. The other
  five (Ballast Tank, Flare Buoy, Beast Bait, Time-Bubble Bomb, Lamp Filter) are still sold and
  crafted without an effect of their own (the far track's time bubbles and lamp filters are
  its own charges and swaps).
- **Music**: the director plays each Descent IV-VI stratum's piece and bed and the
  Leviathan's, the Sun Serpent's and The Glitch's themes (`drowned_music`, `hollow_music`,
  `distortion_music`, `far_strange_music`).
- **Probe**: the Underground Ocean's basins (`drowned::submarine_share_permille`) and the
  isles' turned gravity (`hollow::turned_gravity_share_permille`) are read beside the registry's
  fields; the glare starts at 1300 per mille (the authored Sunforge; the Inner Sky is 1250), so
  the probe's glare and the glare hint both see it.
- Shared tests fitted: the Glitch bot aims through the mirrored rule (the distortion strata
  mirror the aim with the screen), the Sun Serpent keeps the body rules in its own fight test
  (with every Descent's kinds in the shared body arena it lands on the rig), the Geyser
  Fields' sinter takes a paler cream (it looked like the Inner Sky and the Sunforge), and Pixel
  Rot's block test allows for the story's sites standing in it.
- Tests: `world::sonar_tests`, `world::wave9_seam_tests` (settlements, the story's sites,
  shared sections, a V11 save and keyframe with all the new state), the supplies' tests in
  `sonar_tests`, `drowned_tests` and `hollow_tests`.

### The descent probe (integrated V11, default seed, 60 Hz)

| Descent | Ends at (h) | Target (h) | Deaths |
|---|---|---|---|
| I The Crust | 6.4 | 5-8 | 0 |
| II The Old Deep | 16.0 | 13-18 | 1 |
| III The Living Dark | 23.0 | 22-29 | 1 |
| IV The Drowned and Frozen | 33.6 | 31-40 | 1 |
| V The Hollow World | 44.6 | 42-53 | 1 |
| VI The Strange Layers | 59.9 | 53-66 | 1 |
| (whole run, VIII) | 78.0 | 75-90 | 7 |

Every Descent I-VI stratum passes the 20-40 minute novelty check; the longest waits in IV-VI
are 9-16 minutes (Upside-Down Isles and the Sunforge 16). Gate systems: Cryo Hull at 31.6 h,
Gravity Anchor at 42.2 h, Phase Drill at 57.5 h. Strata still over 40 minutes are all in the
placeholder Descent VII (the Archive of Stars, Sleeping Garrison, the Seal Gate). No retune
was needed.

### Still open

- Five deep supplies have no effect yet (above). The far track's time bubbles and lamp
  filters stay its own purchases; a later wave could make the supplies those charges.
- Carried from the track reviews: the Leviathan's hit circle is smaller than its drawn body;
  the Underground Ocean is many basins, not one sea; currents, geysers, frost, wind and the
  anchor act on the rig only, not a pilot on foot; the probe counts boss fights as modelled
  minutes; Merrow City, the Sunfolk and Parallax Station borrow existing cultures' buildings;
  far creatures are small beside the rig; no settlement quest arcs for the new settlements.

## Wave 10 polish: the open items of Waves 8-9 and the 60-90 h balance pass

The polish track of Wave 10 (`src/world/supplies.rs`, `src/world/pilot_weather.rs`,
`workshop::SupplyEffects`, `combat::HitArea`, `crust::heat_danger_units`,
`balance::descent::{mechanics::BOT_FIGHTS, balance_misses}`; tests in
`world/wave10_polish_tests.rs`, the bot fight tests and the game's
`rendering::world::enemies` tests). Every item of "Still open" above is closed but the
settlements' quest arcs (a content track's).

### 1. Every deep supply has an effect (`world/supplies.rs`)

Spent automatically from the kit where its stratum bites (no new input: every seat, online,
browser and touch), each with a `GameplayEventKind::Supply` event (codes 720-724) for the
seat's news line and a sound:

| Supply | Spent when | Effect |
|---|---|---|
| Ballast Tank | a submerged rig falls to 30 % hull or the abyss's pressure cracks it | an emergency blow: 6 s of full lift (+260 px/s² over a full trim) that no current, maelstrom or lure holds (`apply_submarine` reads `ballast_blowing`) |
| Flare Buoy | a lure angler's light catches a rig | a buoy lit where the rig is for 45 s: its warm light (drawn over the dark) reaches 10 tiles and every lure angler in it is dazzled (drags nobody, notices nobody: `supply_notice`) |
| Beast Bait | a hunting predator of the Hollow World (fangrunner, stoop roc, hangclaw, shade stalker, cinder drake) chases a rig | thrown 6 tiles past the beast: every hunting predator within 14 tiles gives up its prey and makes the bait its home for 25 s |
| Time-Bubble Bomb | (thrown like any charge) | **one item with the far track's time bubble**: bought or crafted it becomes an `ExplosiveKind::TimeBubble` charge (`supplies_of` reads the charges; an older save's kit bubbles fold into charges) |
| Lamp Filter | the lamp is first tinted, and as a set wears out | **one item with the far track's lamp filters**: a tinted lamp needs a set of coloured glass (`FarStrangeRefusal::NoFilterGlass` without one); a set lasts 10 minutes, the next is fitted from the kit, and with none left the lamp goes white |

The Frost Heater, Shade Canopy and Echo Charge keep Wave 9's effects. State:
`Logistics::supply_effects` (blowing ballast, flares and bait in the world as absolute tiles,
fitted filter glass; serde default, so older saves load; part of the logistics section, so it
replicates and hashes). Game: `rendering/world/supplies.rs` draws the buoy and its light pool
(breathing slowly with the effects setting, never a strobe) and the bait with its scent; scenes
`polish_flare_buoy`, `polish_beast_bait`.

Tests: `a_ballast_tank_blows_when_the_sea_crushes_a_sub_and_it_surges_up` (one tank of two
blows, the sub rises 16+ px past a bare one; a healthy sub keeps its tanks),
`a_flare_buoy_dazzles_the_lure_anglers_so_they_drag_nobody`,
`beast_bait_draws_a_hunting_predator_off_the_rig` (less hull lost than without),
`time_bubble_bombs_are_the_rigs_time_bubble_charges`,
`a_tinted_lamp_needs_lamp_filter_glass_and_a_set_wears_out`,
`supplies_at_work_save_replicate_and_hash`.

### 2. The HEAT warning reads real danger

`crust::heat_danger_units(terrain, tile, radiator, shield)`: the heat past the rig's cooling
(capped as the tick caps placeholder strata). The tick's band heat damage and the HUD's HEAT
badge both read it, so the badge shows only where the heat cracks this rig's hull, on every
world. Tests: `the_heat_warning_reads_the_heat_past_the_rigs_cooling_not_the_strata_heat`
(every Descent IV-VI stratum is hot, covered by its cap and two shields, dangerous to a stock
rig) and `a_covered_rig_loses_no_hull_where_the_heat_warning_stays_off`.

### 3. Hit areas match the drawn bodies

- `combat::HitArea` (`EntityState::hit_area`, `EnemyKind::hit_area`): shots, rays, point-blank
  hits, the pilot's arms and flak hit a creature's terrain circle **or** the ellipse of its drawn
  body (`HIT_OVER_CONTACT` 1.15 times its contact box; a walker's rises over its centre as the
  game stands it on its feet). The Leviathan's 104 x 34 px body is now hit along its whole
  length (its circle was 44 px). `MAX_HIT_RADIUS` is 3.5 tiles (a test keeps every kind in it).
- Contact boxes fixed to their art where they were off by more than the body rules allow: the
  Hollow Matriarch (28 x 31), The Glitch (38 x 37), the Sun Serpent (33 x 22), the cinder drake,
  the piston warden, the spectre, the Crust's creatures (burrow mole, mud leech, potter wasp,
  mine bat, timber beetle, slate skink, shelf crab, tube crawler, cinder moth), glow grub, frost
  mite, maw worm, leechling and lure fish.
- The far Strange Layers' ten creatures are drawn half again as large (`sprite_scale`), their
  contact boxes grown with them (a tick mite 12 x 8, an echo stalker 24 x 16: the size of the
  rig).
- Test (game): `every_contact_box_and_hit_area_matches_the_drawn_body`: every kind's contact
  box is 72-115 % of its painted half extents (a moth swarm is a cloud), its hit ellipse covers
  80 % or more of them, and its reach fits `MAX_HIT_RADIUS`. The Leviathan's bot fight is
  re-measured below (64 s, 67 % hull: fair and winnable).

### 4. The deep's forces on a pilot on foot (`world/pilot_weather.rs`)

Scaled to a body on foot (the pilot stays upright; positions only move where the pilot's box
fits; per-second rates, every tick rate agrees): **currents** carry a swimming pilot (70 % of the
rig's drift), **geysers** throw a pilot in an erupting jet up (80 % of the lift) and scald one at
the throat, the **glaciers' cold** slows a pilot's walk by up to 45 % deeper in the ice (none by
a vent, in an outpost or the city, or with the Cryo Hull), the Inner Sky's **wind** carries an
airborne pilot (60 %). The **Gravity Anchor engages on foot** (keyboard, pad and a touch ANCHOR
button in the on-foot cluster, `FAR_OUTER`): it plants the pilot's boots for its hold (plain
downward gravity under turned fields, no current, wind or sideways field moves it). Tests:
`a_basins_current_carries_a_swimming_pilot_unless_the_anchor_plants_its_boots`,
`the_inner_skys_wind_carries_an_airborne_pilot`, `an_erupting_geyser_throws_a_pilot_up_its_shaft`,
`the_glaciers_cold_slows_a_pilots_walk_but_not_with_the_cryo_hull`.

### 5. The Underground Ocean is one sea (V11 generation)

Basins roll in nearly every ocean cell (900-1000 per mille), 50-72 tiles wide and 22-42 deep,
hanging from the tops of their cells so a row of basins shares one sea level within a few rows.
Neighbouring basins are joined by `DrownedStampKind::Channel` (appended) passages walled with
seabed and coral: sideways, a 5-row passage along the sea's surface just above the higher of
the two basins' water; downwards, a passage out over the upper basin's water to a 5-wide shaft
just outside its rim (east, else west, whichever keeps clear of the neighbouring basins), down
to the lower basin's surface and in over its water. A sub surfaces, crosses and dives again.

The passages are open air, not water, on purpose: the first cut flooded them, so a basin
drilled out from below drained the whole sea down the shaft and kept the fluid pass busy for
minutes (the deep-descent bot stalled in the ocean). Now no water joins two basins, and a
drilled-out basin drains alone, as it always did. `drowned_tile` opens a passage through the
basins' rims first (keeping a basin's own water where the passage meets it). Passages keep off
the fixed set pieces, Merrow City and the outposts' column (the lift's column stays dry
between the sea's two halves). Test: `the_underground_oceans_basins_are_joined_into_one_sea`
(three seeds: sailing from one basin through water and the passages reaches 90 %+ of the
basins on a 600-column side, and flooding the water alone never leaves the first basin).
**`GOLDEN_V11_DIGEST` updated** to `0x3918_28b8_49df_663c` (a passage's chunk is pinned too);
V1-V10 digests unchanged.

### 6. The deep settlements build their own

`BuildingStyle::{Coral, Jungle, Machine}` (appended; `BuildingStyle::for_village`): Merrow City
(culture Merrow) builds coral walls with shell bands and sea fans under scallop-shell domes;
the Sunfolk Kraal reed walls in a basket weave on stilts under straw thatch with crossed ridge
poles; Parallax Station riveted alloy plates, a conduit and slowly breathing instrument
panels under flat vented roofs with an antenna. Their discovery cards read "coral-and-shell",
"stilt-and-thatch" and "machine-plated" settlements; interiors use the stone, timber and scrap
sets. Test: `merrow_city_the_sunfolk_kraal_and_parallax_station_build_in_their_own_styles`
(Gravel Hollow stays rock-cut). Scenes: `drowned_merrow_city`, `hollow_sunfolk_village`,
`distortion_parallax_station`.

### 7. Boss fights modelled from the bot fights

`mechanics::bot_fight` holds each gate boss's measured bot win (world seconds, hull kept);
`record_bot_fight` in the five bot tests holds them to it (a rework that moves a fight by more
than -40 %/+50 %, or the hull kept by 25 points, fails until re-measured). The probe's fight
is `FIGHT_SETUP_MINUTES` (6) plus the bot's time at `PLAYER_FIGHT_PACE` (4x), a third longer
for a tight fight (the bot kept under 40 %), one learning try, one more per armour tier short:

| Boss | Bot win (30 Hz) | Hull kept | Probe fight (ready rig) |
|---|---|---|---|
| Star Core | 95 s | 100 % | 12 min + a try |
| Mycelial Mind | 118 s | 64 % | 14 min + a try |
| Leviathan | 64 s | 67 % | 10 min + a try |
| Sun Serpent | 298 s | 76 % | 26 min + a try |
| The Glitch | 96 s | 43 % | 12 min + a try |

The Sinkhole (an escape, not a fight) and Descents VII-VIII's bosses keep modelled minutes
until their tracks add bot fights (add a `bot_fight` row and call `record_bot_fight`).

### 8. The Glow Sea reads

The Living Dark's darkest scenes were the Glow Sea's (`living_glow_sea`, `living_mycelial_mind`):
its overlay lifted from 0.95 to 0.8 and tinted a deep-sea blue-black (`GLOW_SEA_INK`), so the
rock's silhouettes read faintly while the living lights still own the scene. The other Living
Dark scenes were already readable.

### 9. The balance pass (`balance::descent`)

- **Tooling**: `DescentRun::balance_misses(descents)` lists every miss against the plan (a
  Descent outside `TARGET_HOURS`, more than `BALANCE_MAX_DEATHS_PER_DESCENT` 2 deaths in one,
  everyday trips over `BALANCE_MAX_EVERYDAY_FUEL_PERCENT` 60 % of the tank, every stratum's
  20-40 minute pacing check, the whole run outside 60-90 h once all eight count);
  `balance_warnings` reads expedition stress (under 15 % of the tank). The report prints both;
  `descent_balance_pass` (ignored, release) fails on any miss in the first
  `BALANCE_PASS_DESCENTS` Descents. **Integrator: set it to 8 once VII-VIII land and run
  `cargo test --release -p fatherload_simulation descent_balance_pass -- --ignored --nocapture`.**
- **Tuning**: the probe never counted the Living Dark's authored content
  (`living_dark_content`: bouncing caps and spores, regrowth, the Hive's factions, healing walls
  and tides, the dark; its groves, knots, court, nest and gullet); counted, Descent III sits in
  its window (24.1 h, not 22.9 h on its floor) and every later Descent moves 1.2 h toward its
  window's middle. The boss model above shortened the Leviathan's and the Glitch's fights.

The probe (default seed, 60 Hz, this branch; VII-VIII still placeholders):

| Descent | Ends at (h) | Target (h) | Everyday fuel (max %) | Expedition fuel (%) | Deaths |
|---|---|---|---|---|---|
| I The Crust | 6.4 | 5-8 | 49 | 28 | 0 |
| II The Old Deep | 15.9 | 13-18 | 15 | 10 | 1 |
| III The Living Dark | 24.1 | 22-29 | 17 | 4 | 1 |
| IV The Drowned and Frozen | 34.4 | 31-40 | 14 | 4 | 1 |
| V The Hollow World | 45.3 | 42-53 | 14 | 4 | 1 |
| VI The Strange Layers | 60.3 | 53-66 | 13 | 2 | 1 |
| VII The Ancients (placeholder) | 69.9 | 66-82 | 13 | 2 | 0 |
| VIII The Heart (placeholder) | 78.3 | 75-90 | 12 | 2 | 2 |

(Re-run in the Wave 10 review, after the walker hit-area fix re-measured the Mycelial Mind's
and the Sun Serpent's bot fights.) Whole run 78.3 h (60-90), 7 deaths. Every Descent I-VI stratum passes the 20-40 minute check
(longest waits 4-16 minutes; the Upside-Down Isles and the Sunforge 16); the balance pass meets
every target for I-VI. Gate systems: Pressure Hull 14.4 h, Bio-Filter 22.6 h, Cryo Hull 32.4 h,
Gravity Anchor 42.9 h, Phase Drill 57.9 h. Placeholder strata over 40 minutes: the Archive of
Stars, Sleeping Garrison and the Seal Gate (the Ancients track fills them).

### Formats, seams and still open

- **`PROTOCOL_VERSION` 18** (one bump for the track: `Logistics::supply_effects`, the supply
  events, `BuildingStyle`'s appended variants; the integrator collapses Wave 10's). **No save
  format bump**: the new field has a serde default (an older kit's time bubbles fold into
  charges on the first tick).
- Shared-file edits, all additive: `event.rs` (appended `Supply`), `netcode/world.rs` (codes
  720-739), `logistics.rs` (the field), `workshop.rs` (state, constants, `SupplyEvent`; the
  supplies' counter text), `far_strange.rs` (appended refusal), `world/far_strange.rs` (the glass
  check), `world/drowned.rs` (the blow, the flare hook; `near_vent` visible), `world/enemies.rs`
  (the notice hook first), `world/pilot.rs` (chill, weather, anchored view), `world/hollow.rs`
  (the anchor on foot), `world/logistics.rs` (the anchor on foot; `supplies_of`/`take_supply`
  moved to `world/supplies.rs`), `enemy.rs` (contact boxes, `hit_area`), `entity.rs`,
  `combat.rs`, the weapons' hit tests, `village/layout.rs`, `worldgen/descent/drowned.rs`, the
  balance probe; in the game the villages' art, the Glow Sea, enemy scales, the HUD heat badge,
  touch, audio cues, news and scenes.
- **Still open**: expeditions are not yet stressful past Descent I (2-10 % of the tank: the
  deep tanks dwarf a Descent's climb; making them bite is a fuel economy change, reported by
  `balance_warnings`); the new settlements' quest arcs (a content track's).

## Wave 10 ancients-a: Descent VII part A, the builders' buried city (`src/ancient_city.rs`, `src/ancient_city/{movers,archivist}.rs`, `src/star_chart.rs`, `src/world/ancient_city.rs`, `src/world/enemies/ancient_city.rs`, `src/worldgen/descent/ancient_city.rs`)

Expansion plan §13-§14, Descent VII strata 1-3: the builders' buried world, where the story
climax approaches. Everything is V11 only (`has_descents()`); V1-V10 digests, hashes and
replays are unchanged. Under the far track's alloy seal (only the Phase Drill cuts it), builder
alloy portals frame the way into a city that still keeps order.

### The strata (registry, `descent/registry.rs`)

| Stratum | Signature rule (gently, then pushed) | Creatures | Set pieces | Ores, resource | Music / ambience |
|---|---|---|---|---|---|
| Buried Megacity | **a city that keeps order**: avenues on fixed levels (`avenue_levels`, 38 or 44 rows apart by variant), many with a **tram line** that runs a timetable, **towers** between one avenue and the next whose **lifts** call at the upper street, every floor and the lower street; drilling the city's fabric (paving, tower glass, lamps) raises the **city alarm** (`CityAlarm`: +140 a tile, cools 45/s; at 400 the city watches, at 1000 it **hunts** for 20 s). Deeper levels are more broken ("Fallen Towers" more gaps and rubble, "Tram Avenues" more lines) | maintenance drone (mends the city; hunts only an alarmed rig or one that shot it), scrap crawler (its bite carries off the best ore in the bay; kill it and the ore drops), street sentinel (sees only what its searchlight cone shows, fires bolts) | the **Lantern Library** (the archivist's tower beside the cable), the **Archive Post** (Descent VII's outpost with residents), alloy portals under the seal | mythril, platinum; Ancient Alloy (alloy ore) | `BuriedMegacity` (a lonely civic anthem) / `CityHum` |
| Engine Halls | **the Great Engine's beat**: every piston, crusher and belt runs on one stroke (`engine_beat_millis`, 4.4 s at the top to 3.0 s at the bottom). **Pistons** lift a rig resting on their heads (drive off at the top onto a ledge, or **thrust off a rising head to be thrown** past it); **crushers** glow amber for 1.2 s before every slam (16 hull scaled, once a slam, shoved out); **belts** carry. The music's groove follows the beat | cog roller (charges along floors), spark wisp (its jolt drains fuel), rivet hound (pounces from afar) | **engine halls** (piston staircases to a cache, belts, crushers over them), **piston shafts** (a zig-zag of pistons and ledges; "Piston Shafts" leans to them), **crusher lines** (a wave of crushers over a belt; "Crusher Lines"), the **Great Engine** (two staircases to a gallery relic under a turning flywheel) | uranium, mythril; Engine Core | `EngineHalls` (a machine groove at the beat) / `EngineBeat` |
| The Archive of Stars | **sealed vaults**: rooms of the builders' unbreakable masonry whose locks are puzzles of the existing mechanisms (`VaultKind`): a **plate** that holds its gate open a breath after the weight leaves (rest, then dash), a **lever up a crawlway** only a pilot fits (WP-PILOT's rig vault), **coloured glass** opened by the rig's lamp filter (the far track's keyed walls), and the deep **grand vault** (plate, gate, plate, gate, red, green, blue). Each holds a **star chart** on a plinth; shallow vaults are simple, deep ones grand | page moth (nibbles), index spider (drops from the shelving), archive warden (slow, armoured, reads a rig with a beam before it fires) | **stacks** (shelf halls with mezzanines), vaults, the **Orrery** (the builders' sky; three free charts) | diamond, mythril; Star Glass | `ArchiveOfStars` (starlit chimes, a choir pad) / `Stacks` |

The Archive's light is dimmed to 760 per mille with a deep blue grade; the Engine Halls have a
light heat haze and vignette that deepen; the Megacity a warm lamp grade.

### The archivist (`ancient_city::archivist`, the companion)

A builder machine, **Index**, keeper of the Lantern Library. It wakes for a player whose rig
or pilot comes within 9 tiles of an archivist core (the Library's, on its top floor, or the
Archive Post's, in the outpost hall's ceiling) and from then on **follows** that player as a
little drone (`Archivist`: a frame-relative point stepped every tick towards a spot behind and
above its rig or pilot on whole quarter seconds of world time, so every tick rate agrees and the section changes four times a second; the game draws it gliding between its last two points; it blinks back past 18 tiles). Each player has their
own (`AncientCity::archivists`, keyed by player, kept when the player leaves so it is waiting
when they return), saved and replicated in the shared section, so split-screen seats and online
partners each see theirs and everyone else's.

- **It talks** (`LINES`, 62 authored lines with stable ids, each said once): a greeting
  ("You are late."), hints at the machinery (trams, lifts, pistons, crushers) and each kind of
  vault, a word on each of the nine creatures it first sees, the strata as they come (through
  the Heart), the alarm, low fuel and hull, the landmarks, the first, third, sixth and tenth
  star charts, the Fathers' logs 61-73 as the crew reads them (Ines asking what the trams are
  for, Amos's leg, Ines staying in the Archive), and, after 150 s of quiet in the builders'
  world, musings in order. Lines are spaced 12 s apart (3.5 s for news that cannot wait) and
  chosen in the tick from what is happening, so every peer agrees; the event
  `ArchivistSpoke` carries the line id and the game shows it in the seat's speech box.
- **It scans** every 18 s: the three most valuable ore tiles (or caches) within 9 tiles are
  marked for 7 s (`Archivist::scan`); a changed scan chirps.

### Star charts (`crate::star_chart`, the seam for Wave 11's space generator)

A chart is read when a rig or pilot comes within 1.75 tiles of an unread plinth (the plinth
turns read by rule, a terrain op). Everything about a chart is a pure function of the world
seed and the plinth's tile (`chart_at_site`): a stable system id (`system_id`, never zero), a
galactic sector relative to home (`HOME_SECTOR` (0, 0); the ring `CHART_RING` 3-48 sectors,
deeper plinths further out; integer maths so every peer agrees), a star class (`StarClass`:
yellow, red, blue, white, twin, nebula) and how many worlds circle it (at least one), and the
builders' name (`star_name`). The crew's list is per world, in the order found, never shrinks
or reorders (`AncientCity::charts`, at most `MAX_CHARTS` 512); `WorldState::star_charts()` and
`star_chart_known(system)` read it. **Wave 11**: seed a system from `StarChart::system`, place
it at `sector`, honour `class` and `worlds`. The game's **star charts page**
(`ModalScreen::StarCharts`, after the codex in the map's page cycle) lists them and draws the
charted sky beside the dialog.

### Generation (V11 only; `worldgen/descent/ancient_city.rs`, a pass of `streamed::assemble` after the far Strange Layers and before the story)

`plan_for` gathers a chunk's stamps (towers and the library first, then avenues, then the
engine and archive set pieces, rolled per `ENGINE_REGION` 64 x 48 and `ARCHIVE_REGION` 60 x 40
cell, likelier deeper) and `city_tile` answers per tile; city rock dresses plain rock (alloy ore
veins, pipes and engine core ore, star glass specks and archive stone). Everything keeps clear
of the cable and the outpost halls' columns (`clear_columns`) and the Descent landmarks; the
engine and archive set pieces and towers give way to V9's pilot-only spaces (unbreakable
masonry), and the library cuts through one. Vaults are hand-authored layouts
(`vault_layout`, mirrored by seed). Movers come from the same stamps (`movers_with`,
`mover_at`). The story's sites keep off every stamp (`sites::DEEPER_SET_PIECES` gains
`crosses_ancient_city`). **`GOLDEN_V11_DIGEST` is updated**; V1-V10 digests unchanged.

### The machinery (`ancient_city::movers`; pure functions of world time)

- A tram car or lift cab runs a `Track` (stops, speed, a dwell at each stop, a phase seeded
  per line): out to the last stop and back, each leg eased (fixed-point smoothstep), positions
  in integer milli-tiles from world milliseconds. Trams cruise 9 tiles/s (half again faster
  mid-leg) and wait 3.6 s; lifts 6 tiles/s and wait 2.8 s.
- A piston or crusher runs a `Cycle` of its hall's beat with a phase per machine (a staircase
  rises in turn; a crusher line slams in a wave): pistons rest (the last 0.9 s hissing), rise,
  hold high, fall; crushers rest (the last 1.2 s their lamp warming), slam (accelerating), hold
  down, rise.
- **Rides** (`CityRide`, in the shared section): a rig resting in a car or cab while it waits
  at a stop, or on a piston's head, boards; the movement step and the drill are skipped while
  aboard (like a lift ride) and `advance_ancient_city` carries it to the mover's place every
  tick, with or without input; the stick walks it along the car, cab or head (off a piston's
  edge any time, off a car or cab only at a stop), thrust jumps off (a rising head throws it:
  1.6x the head's speed plus 180 px/s). A pilot cannot climb out mid-ride. Every tick
  rate agrees (tests compare 20 and 60 Hz).

### State, events, formats

- `AncientCity` (world field, appended shared `SectionKey::AncientCity`, serde defaults, hashed
  once non-empty): rides, alarms, archivists, charts, vaults opened, landmarks found.
  `CitySchedule` (saved, never hashed or replicated, cleared in client keyframes): crusher
  hits, the ore crawlers carry, launches, the logs each archivist has remarked on, and the
  tick's ride sticks (not saved).
- Tiles: `TileKind::City(CityTile)`, a new family at code base **`0x1C00`** (19 tiles; the
  machinery's fixed parts and the plinths are indestructible and blast-proof).
- Appended: nine `EnemyKind`s (`MaintenanceDrone` .. `IndexSpider`, with contact boxes matching
  their sprites), two `ShotKind`s (`SentinelBolt`, `ReadingBeam`), `MusicCue::{BuriedMegacity,
  EngineHalls, ArchiveOfStars}`, `AmbienceCue::{CityHum, EngineBeat, Stacks}`,
  `LandmarkKind::AncientCity`, `memories::Settlement::ArchivePost` (10 lines; the story track
  may re-key the last two to logs 64 and 66 once they exist), `DeepSettlement::ArchivePost`
  (`ARCHIVE_POST` village id `0x3FFF_FC00`).
- Events: `GameplayEventKind::AncientCity { what, player_id, x, y, detail }`, netcode codes
  `630 + CityEvent::code()` (630-647).
- **`PROTOCOL_VERSION` 18** (one bump for the track). **No save format bump.**
- Shared-file edits (additive): the far track's keyed walls and lamp-filter swap also work in
  the Archive of Stars (`world/far_strange.rs`); `simulate_tick` skips drill and movement for a
  city rider and records its stick; `can_exit_rig` refuses mid-ride; enemy notice, strike and
  kill hooks; the descent probe reads `ancient_city::authored_content`.
- Dev and scene seams: `wake_archivist_for`, `stage_archivist_line`, `stage_star_chart`,
  `set_ancient_city`, `stage_city_alarm`, `city_movers_near`, `city_stamps_near`,
  `city_landmark_bounds`, `archivist_cores`, `lantern_library`.

### The Archive Post (`people::deep_folk`)

Descent VII's outpost hall is a third deep settlement: **Keeper-Unit Nine** (quartermaster, a
builder machine), **Ada Lantry** (lamplighter), **Pell Corran** (tinker), **Sweep** (a
cleaning machine) and **Curator Thessaly** (a builder machine who watched Ines Calloway stay to
read the Archive). The Lamplighters' grandparents came down after a crew that never went home
and kept the street lamps burning. They live in the half of the hall away from the lift's door
and tell their own memories of the Fathers in conversation (`Settlement::ArchivePost`, ids
`archive_post.*`), registered by name (`"The Archive Post"`, `"Megacity Archive Post"`).
`deep_folk::is_automaton` names the machines (the game may draw them so).

### Game side

- Tile art (`tile_art/city_art.rs`): pale city stone blocks, tower glass with lit windows, tram
  rails and yellow-edged platforms, hanging lamps, alloy ore, lift rails, the archivist's
  core; riveted engine plating, belts with chevrons, hazard-striped housings, glowing engine
  core ore, copper pipes; book-spine shelving, starred archive stone, star glass and chart
  plinths. Back walls: a night city of lit facades on the avenues, panelled tower interiors,
  dark plating with pipes and gauges in the halls, a starry blue in the Archive.
- World (`rendering/world/ancient_city.rs`): tram cars in cream and teal under their wire,
  lift cabs on their cables, piston heads on their rams with steam and an amber lamp before
  they rise, crushers with guide rods and a warning lamp (amber warming, red falling), belts'
  chevrons running, the Great Engine's flywheel turning on the beat, the Orrery's spheres, the
  plinths' gold light, every player's archivist drone (lens lit and a speech glyph while it
  talks) and the seat's own scan brackets. Slow, low contrast, scaled by the effects setting.
- Sprites (`pixel_art/sprites/ancient_city.rs`): nine creatures, two shots and the archivist.
- HUD (`rendering/ancient_city_hud.rs`): per seat, the Megacity's alarm (HUNTED in red), the
  ride's next stop, the Engine Halls' beat as a sweeping metronome bar, the charts read; the
  archivist's words in a speech box with its portrait; the star charts page.
- News and cards (`local_game::announce_ancient_city`): landmarks' beats and each star chart
  as cards on every local seat; the alarm, a crusher, a theft, a jolt and a launch for the
  seat concerned.
- Audio (appended at the end of `patches!`, `audio/patch/ancient_city.rs`, `city_*` names):
  twelve one-shots (tram bell, lift chime, piston hiss and thump, crusher slam, archivist chirp
  and scan, chart chord, vault lock, the alarm's soft two-tone call, a crawler's snatch, a
  wisp's zap), three music beds (the Engine Halls' at the beat: `ancient_city_music`) and three
  ambience beds. `SEAT_LOOP_SLOTS` grew to 128 (64 were all used).
- Scenes: `city_megacity_avenue`, `city_tram_ride`, `city_tower_lift`,
  `city_lantern_library`, `city_lantern_library_hall`, `city_archivist`, `city_alarm_hunt`,
  `city_archive_post`, `city_archive_post_talk`, `city_portals`, `city_engine_hall`,
  `city_crusher_line`, `city_piston_shaft`, `city_great_engine`, `city_archive_stacks`,
  `city_vault_plate`, `city_vault_lever`, `city_vault_light`, `city_vault_grand`,
  `city_orrery`, `city_star_charts`.

### Tests

`world/ancient_city_tests.rs`: a rig boards a tram at a station, rides it to the next and steps
off onto the platform, in the same places at 20 and 60 Hz; a lift carries a rig from one avenue
down to the next; a piston lifts a rig on its head alike at 20 and 60 Hz, and thrusting off a
rising head throws the rig past the stroke; a crusher strikes a rig under it once a slam and
shoves it out; belts carry; cutting city glass raises the alarm, it cools, a calm city's drone
ignores the rig and a full alarm sets it on; the archivist wakes by its core (its greeting), not
for a partner far away, follows its rig (alike at 20 and 60 Hz), marks ore, is saved, travels in
the section, and wakes separately for the partner at the Archive Post; the Orrery's plinths
chart stars that are kept, saved and shared, never twice; **bots solve a plate vault (rest,
dash), a light vault (filters red, green, blue) and a grand vault**, and a lever vault's gate
and plinth work; a scrap crawler steals ore and drops it when killed, a spark wisp drinks fuel;
the nine creatures keep the body rules in their own arena; older worlds keep their hashes and
the city is deterministic. `worldgen::descent::ancient_city::tests`: vault layouts well formed
(every gate's mechanism in reach), avenues, trams, towers and lifts with room to move, the
library's core and lift, engine machinery with room and on the beat, every kind of vault
carved whole, the Orrery's plinths, portals, older worlds untouched; plus the pure maths in
`ancient_city::{tests, movers::tests, archivist::tests}` and `star_chart::tests`, the Archive
Post's residents in `people::deep_folk` and `world::deep_folk_tests`.

### The descent probe (this track alone, default seed, 60 Hz)

| Stratum | Minutes | Leaves at (h) | Longest gap (min) |
|---|---|---|---|
| VII Buried Megacity | 163 | 62.6 | 11 |
| VII Engine Halls | 148 | 65.1 | 13 |
| VII The Archive of Stars | 148 | 67.6 | 15 |

Descent VII ends at 71.4 h (target 66-82) and the whole run at 79.8 h (75-90), 7 deaths; every
stratum brings something new within 40 minutes (the placeholder Sleeping Garrison and Seal Gate
wait 38, for part B). The Heart Key lands at 69.6 h. No retune was needed.

### Still open

- The archivist's lines are its own table (`archivist::LINES`), not yet routed through the
  story track's journal; the story track may key the Fathers' log remarks to its log ids.
- The Archive Post's residents use the people system's generic chatter beside their own
  memories, so a builder machine like Curator Thessaly also has an age ("Scholar, 66") and
  everyday quips. The city's creatures are small next to the rig. The rock around the Megacity's
  avenues uses the stratum's ordinary palette; only the city's own tiles are new.
- The `city_great_engine` scene shows the floor and the flywheel, not the piston staircases.

### Review (Wave 10 ancients-a review)

- **Crushers and rams no longer push a rig into rock.** They used to shove a rig to the
  nearer side whatever was there. In the crusher test world an artifact stood beside the
  crusher, so the rig ended up inside it, and the old test passed only because the stuck rig
  stayed clear of the crusher. Now the shove goes to the other side when rock fills the nearer
  one (`free_shove_side`). The crusher test checks the rig right after the strike, since the
  belt carries it back for the next slam. A new test puts rock on one side and checks the rig
  goes out the other.
- **The lever vault is tested end to end.** A pilot climbs out, stands in the crawlway, pulls
  the lever, the gate opens, and the rig drives in and reads the chart. The old test only
  teleported the rig past the gate.
- **The city tile family moved to code base `0x1C00`.** The heart track's `CoreTile` also used
  `0x1A00` on its branch. Only the codes changed, not the world, but the digest folds in tile
  codes, so **`GOLDEN_V11_DIGEST` is now `0x7048_e2dc_ef30_15bf`**. V1-V10 digests are
  unchanged.
- The codex's "Turn the page" goes to the star charts only on Deep Descent worlds. Elsewhere it
  goes straight to the map.
- `city_vault_grand` now places the rig between the second gate and the glass, so the whole
  vault is in view.
- Re-ran the descent probe (release, default seed, 60 Hz). It matches the table above:
  Megacity 163 min / 62.6 h, Engine Halls 148 / 65.1, Archive 148 / 67.6, Descent VII
  71.4 h, whole run 79.8 h, 7 deaths, longest gap 39 min (the part-B placeholders).

## Wave 10 ancients-b: Descent VII part B, the Sleeping Garrison, the Seal Gate and the Gatekeeper Colossus (`src/garrison.rs`, `src/world/garrison.rs`, `src/world/enemies/garrison.rs`, `src/worldgen/descent/garrison.rs`)

Expansion plan §14, Descent VII strata 4-5 and its gate. Everything is V11 only
(`has_descents()`); V1-V10 digests, hashes and replays are unchanged.

### The strata (registry, `descent/registry.rs`)

| Stratum | Signature rule (gently, then pushed) | Creatures | Set pieces | Ores, resource | Music / ambience |
|---|---|---|---|---|---|
| Sleeping Garrison | **the garrison wakes in waves**: a rig walking into a fort stirs it; after an 18 s build window (`STIR_MILLIS`) machines climb out of its bays wave after wave (`wave_roster`: more and heavier waves deeper, half as many again per extra rig, at most twelve out), each wave telegraphed by its bays pulsing red for `WAVE_TELL_MILLIS` (5 s), a 9 s breather between; the last wave beaten, **the garrison sleeps again** and its armoury pays every rig holding the fort (credits at the Descent's price, war plate, ancient alloy). Nobody holding the fort for `ABANDON_MILLIS` (15 s): it **stands down** (its machines go back in, the waves start over; no other penalty). **Base defence** with the build kit (below) | war sentry (walks, red visor bolt), lancer drone (hangs, then dives with its lance), siege walker (four legs; a long crouch and a glowing mortar before a lobbed shell that breaks defences) | barracks forts (`FORT_REGION` 120x80, a third of the cells at the top to most at the bottom; three waves) and the **Grand Bastion** (one per world, five waves, the biggest armoury, racks of sleeping machines, its story card) | mythril, uranium; War Plate | `SleepingGarrison` (martial snare, low brass ostinato) / `Barracks` (dormant hum, a ratchet) |
| The Seal Gate | **tremors** (`tremor_period`: every 42 s at the top to 20 s at the bottom): the stratum shakes and stones fall from the ceilings over every rig in it (2-4 a rig, `tremor_stones`), each marked by dust pouring and a red mark where it lands for `TREMOR_TELL_MILLIS` (1.4 s); none in a peaceful world | seal warden (hovering gilded sentinel; its ring flares before a slow orb), gilded crawler (rears and lunges) | the **Seal Court** (six key altars before the door) and the **Processional Way** (a pillared tunnel at the court's floor, 46 columns, that a rig digging down near it hits); the **Seal Gate** | mythril, platinum; Seal Gold (veins in the rock) | `SealGate` (gilded pads, a deep bell) / `SealHall` (resonance, trickling dust) |
| Gate | **the Seal Gate** opens to the keys of every Descent (`SealKey`: Deep Claim, Pressure Hull, Bio-Filter, Sonar Suite and Cryo Hull, Gravity Anchor, Phase Drill): six sockets on the door light for the keys the seat's rig carries; a rig carrying them all at the door lights them one by one (`SOCKET_MILLIS`) and the door sinks into the floor row by row (`DOOR_ROW_MILLIS`); a rig missing one is told which, once. Behind it the **Gatekeeper Colossus** (`EnemyKind::GatekeeperColossus`, three stages, `ColossusStage`) across a **collapsing hall** | the Colossus and its head, fists and legs (each a hit area drawn where it is), stage three's war sentries | the Gatekeeper's hall (88x34, seal stone nothing digs) | | `GatekeeperColossus` (huge drums, brass, a grinding ostinato) |

Reward: the **Heart Key** (`RigSystem::boss_reward` is the Colossus; never sold,
`UpgradeLock::Boss`), fitted to every rig of the crew (and rigs that join later). Only a rig
carrying it drills the **Heart Seal** (`CaveTile::HeartSeal`, blast-proof): three ragged rows
across every column under the Seal Gate's boundary zone (`heart_seal`) seal Descent VIII.

### The Gatekeeper Colossus (`world/enemies/garrison.rs`)

- **Body**: the core (the torso, hit radius 48, the most `MAX_HIT_RADIUS` allows) with four
  part entities (`ColossusPart`: head r33, two fists r30, legs r45) placed every tick at fixed
  offsets from the core (`rest_offset`) or where a fist's attack has it, so every hit area is
  where its sprite is drawn (the game draws each part at 1.5x art scale: the sprite's half size
  is its hit radius). A hit on a part passes to the core (`garrison_damage`: the head x1.6, the
  fists x0.4 armour and x1 while they rest after striking, the legs x0.7); parts never die.
- **Stage one, the Guard**: it stands on the hall's far floor, swaying four tiles towards the
  middle and back; every 4.2 s a fist rises over the nearest rig's column (a red mark and a
  column of red light on the floor, `FIST_TELL_MILLIS` 1.3 s) and slams (a shockwave 2.4 tiles
  each side, `SLAM_WAVE`); its eye fires fans of teal bolts.
- **The first collapse** (at 66 %): telegraphed for `COLLAPSE_TELL_MILLIS` (4 s: it roars, its
  core flares, the cracks glow and dust pours; it cannot attack or be hurt), then the **cracked
  floor** falls into the crypt (`collapse_writes`) with rubble ramps at both ends.
- **Stage two, the Fall**: it stands in the crypt; its fists sweep the crypt floor end to end
  (a glowing line first; rise over them) while the vault sheds stones over the rigs (dust and a
  mark first, `RUBBLE_VOLLEY`).
- **The second collapse** (at 33 %): the **cracked vault** comes down, leaving three rubble
  platforms in the air; its legs fall away.
- **Stage three, Unbound**: the bare core (x1.3 damage) hovers after the rigs, slamming faster
  (1 s tells) and calling up to two war sentries every 14 s.
- No solid tile is ever written onto a body (`tile_occupied`); no countdown fails the fight.
  With nobody in the hall for 30 s it sleeps and heals to its stage's top; **the hall keeps its
  collapses** and it wakes at that stage. Its death fits the Heart Key and opens the crypt's
  floor (`victory_shaft`) on the way down. A peaceful world yields the gate at once.

### The build kit's defences (`base::Buildable::Defence`, `garrison::DefenceKind`)

A fourth build group, **Defences** (number key 4), in the palette with everything else, so
keyboard, pad, touch and every seat build them with the existing build mode:

| Defence | Footprint | Hit points | What it does |
|---|---|---|---|
| Auto-Turret | 1x2, on a floor | 160 | shoots the nearest hostile it sees within 11 tiles every 0.6 s (14 kinetic, kill credit to its builder); its tracer is in the state (`Defence::fired`) |
| War-Plate Barricade | 1x1 solid (`CaveTile::WarBarricade`, unbreakable and blast-proof) | 240 | a wall machines must batter down |
| Shock Trap | 1x1, on a floor | 90 | zaps what walks over it (26, EMP on machines: they are stunned), re-arms in 2.5 s |

- They go up **in any claim** the builder may build in (base defence against night raids too)
  and **inside a garrison fort's walls without a claim** (field works). No claim may be staked
  over a fort (`BuildRefusal::InFort`: a claim's safe zone would turn the waves' blows).
  Locked until a crew reaches the Sleeping Garrison (`BuildRefusal::NeedsGarrison`; test mode:
  always). Credits scale with the Descent (`WorldState::defence_cost`), blocks from the hopper.
- Garrison machines batter the defences they reach every second (`batter_damage`: sentry 14,
  walker 34, drone 8); a siege shell breaks those it lands among (`SHELL_BLAST`). Taking one
  down with the build kit refunds half its credits. Barricades are terrain tiles (terrain ops);
  turrets and traps live in the `Garrison` section.

### Generation (`worldgen/descent/garrison.rs`, a pass of `streamed::assemble` after the far Strange Layers' and before the story)

Forts (`forts_with`, `fort_tile`: war-plate walls two thick, gates four rows tall in both side
walls, war bays every ninth column of the ceiling and one in each wall, two ledges, the hold
pylon), the Grand Bastion (`grand_bastion`), the Gatekeeper's hall (`colossus_hall`, on the
side of the spawn away from the outposts' lift so the cable never crosses it; `hall_tile`
clears everything but bedrock), the Seal Court and the Processional Way (`court_tile`), the
Heart Seal over everything but bedrock, and the rock (war-plate scrap, seal gold veins). The
expedition's story pass keeps its sites off all of them (`crosses_set_pieces`, in
`crosses_strata_set_pieces`). **`GOLDEN_V11_DIGEST` is updated** (`0xd527_8215_2d11_3606`);
V1-V10 digests unchanged.

### State, formats and events

- `Garrison` (world field, appended shared `SectionKey::Garrison`, saved with serde defaults,
  hashed once non-empty): forts that stirred (keyed by their pylon, saved through
  `pair_list`), defences, the Seal Gate, the Colossus's fight, falling stones, the tremor clock,
  landmarks found. `GarrisonSchedule` (saved, never hashed or replicated, cleared in client
  keyframes): whom the gate refused. No local points are stored (tiles, ticks and entity ids
  only), so the frame needs no reframing.
- Appended: twelve `CaveTile`s with **codes 160-171** (kept clear of the other tracks' appended
  codes), nine `EnemyKind`s (`WarSentry` .. `ColossusLegs`), five `ShotKind`s (`SentryBolt`,
  `SiegeShell`, `WardenOrb`, `ColossusBolt`, `FallingStone`), `Buildable::Defence`,
  `BuildGroup::Defences`, `BuildRefusal::{NeedsGarrison, InFort}`, `MusicCue::{SleepingGarrison,
  SealGate, GatekeeperColossus}`, `AmbienceCue::{Barracks, SealHall}`, `LandmarkKind::Garrison`.
- Events: `GameplayEventKind::Garrison { what, player_id, x, y }`, netcode codes
  `650 + GarrisonEvent::code()` (650-669, all twenty used).
- **`PROTOCOL_VERSION` 18** (one bump for the track). **No save format bump** (every new field
  has a serde default; an older save loads with the section empty, test).
- Hooks: the drill refuses the Heart Seal without the Heart Key (`heart_seal_refuses`); siege
  shells break defences where they burst (`garrison_shell_burst`); the Colossus's parts pass hits
  on (`garrison_damage`), the unbound core takes more (`garrison_damage_factor`), the collapse
  shrugs hits off (`garrison_ignores_damage`); a fort's machines know its holders and the
  Colossus its hall (`garrison_notice`); the Colossus and its parts never roam.
- Dev and scene seams: `stage_defence`, `stage_fort_wave`, `stage_colossus`,
  `stage_colossus_fist`, `set_garrison`, `colossus_hall`, `seal_door`, `seal_court`,
  `seal_keys_of`, `missing_seal_key`, `forts_near`, `fort_at`, `garrison_landmark_at`.
- Shared files touched additively: `depth_progression` (the Heart Key is boss-won; the tests
  that bought it now check it is never sold), the probe's authored content
  (`garrison::authored_content`).

### Game side

- Tile art (`tile_art/garrison_art.rs`): riveted olive war plate with worn chevrons, slatted
  bays behind a red lens, amber-banded pylons, hazard-edged barricades, dark bronze seal stone
  with amber joints (glowing cracks on the cracked floor and vault), the gold door with engraved
  rings, the crimson Heart Seal veined white, seal gold, rubble, key altars; back walls: dark
  olive bulkheads, umber stone with gilded glyphs, and the Gatekeeper's hall's bronze with gold
  pilasters.
- Sprites (`pixel_art/sprites/garrison.rs`, generated from ASCII art): the five creatures, the
  Colossus's torso (guard, roaring, unbound), head (calm, glaring), fist (clenched, red-hot),
  legs, five shots, the turret (idle, firing) and the trap (armed, spent).
- World (`rendering/world/garrison.rs`): fort racks of sleeping machines (green lamps asleep,
  red when up), the hold pylon's band of amber light while a garrison is up, lit bays pulsing
  faster as a wave nears, turrets with tracers, traps arcing, damage bars; the door's six
  sockets in their keys' colours (lit per the seat's rig, then one by one as it opens) and a
  gold glow as it sinks; a raised fist's red mark and column, a sweep's glowing line, the
  collapse's glowing cracks and pouring dust, dust and a landing mark over every falling stone.
  Every pulse is slow and steadies with the effects setting off.
- HUD (`rendering/garrison_hud.rs`): the fort's wave of all, machines left, the build window and
  tells (tells, never fail countdowns) and "leave and it stands down"; at the door the six keys
  held or missing; in the hall the Colossus's stage, health and the collapse warning (a panel at
  the top middle, clear of the fight).
- News and cards (`local_game::announce_garrison`): landmark cards on every seat; a fort's
  waves, tremors and the Colossus's fight for every local seat; the gate's verdict naming the
  missing key and the rewards for the seat they concern.
- Audio (appended at the end of `patches!`, `audio/patch/garrison.rs`, names prefixed `ab_`):
  ten one-shots (turret shot, trap zap, klaxon, bay opening, defence breaking, fist slam, hall
  collapse, socket chime, door grind, tremor), two music beds, two ambience beds and the
  Colossus's theme (`garrison_music` in the director).
- Scenes: `ab_garrison_telegraph`, `ab_garrison_defence`, `ab_grand_bastion`,
  `ab_seal_gate_sockets`, `ab_seal_gate_opening`, `ab_seal_tremor`, `ab_colossus_guard`,
  `ab_colossus_slam`, `ab_colossus_collapsing`, `ab_colossus_fallen`, `ab_colossus_unbound`.

### Tests

`world/garrison_tests.rs`: the places generated in the world (pylon, door, cracked floor, the
Heart Seal under the hall); a fort stirring, telegraphing from its lit bays, sleeping after its
last wave and paying its armoury; a fort standing down and starting over; waves waking at the
same world time at 20 and 60 Hz and the garrison deterministic; defences placed in a fort
without a claim but not in the open, locked before the garrison, refunded, no claim over a fort;
a turret shooting a drone down (its tracer in the state), a trap stunning a sentry, a walker
battering a barricade down; defences saved, replicated, hashed and older saves loading empty;
tremor stones telegraphed by dust at every tick rate; the Seal Gate refusing each missing key
(saying which, once) and opening with all six, its sockets lit one by one; **a bot with Descent
VII kit (railgun, hull tier 15) beating the Colossus through every stage in order** across the
collapsing hall, each collapse telegraphed for the full warning and immune, more than 15 % hull
left, the Heart Key fitted and the floor opened; the parts drawn where they hit and passing hits
to the core; a peaceful hall yielding and a lonely one sleeping with its collapse kept; the
Heart Key boss-won, the Heart Seal refusing a drill and a blast without it and yielding to it;
the garrison creatures keeping the body rules. `garrison::tests` and
`worldgen::descent::garrison::tests` cover the rules' maths and the generation.

### Descent probe

The probe counts this track's content (`garrison::authored_content`: waking garrisons, base
defence, garrison forts and the Grand Bastion; tremors, the gate's sockets, the Seal Court and
the Colossus) beside the registry's new creatures, music and landmarks. Default seed, 60 Hz:

| Stratum | Minutes | Leaves at (h) | Longest gap (min) |
|---|---|---|---|
| VII Sleeping Garrison | 150 | 68.3 | 56 |
| VII The Seal Gate | 194 | 71.5 | 22 |

The Heart Key is won at 69.1 h; Descent VII ends at 71.5 h (target 66-82) and the whole run
at 79.9 h (target 75-90). The Sleeping Garrison's 56-minute gap is the one carried in from the
placeholder Archive of Stars above it (ancients-a's stratum: the gap runs from its last new
thing to the garrison's arrival); inside the garrison the new things come every 20 minutes or
so. The Seal Gate (Wave 9's open item) now passes.

### Seams and known limits

- The Heart track owns Descent VIII: the Heart Seal is this track's (the boundary under the Seal
  Gate); a later pass over its rows must leave it whole.
- The story track owns logs 68-72 (the garrison and the gate); the landmarks' beats are separate
  cards, and the story's sites keep off these places.
- Ancients-a owns strata 1-3 and Descent VII's outpost; the Phase Drill's alloy seal opens onto
  its Buried Megacity.
- Defences need a claim outside forts (there is no free-standing field work in the open), and
  they target any hostile, not just garrison machines; a turret's shot is hitscan.
- Machines batter defences by reach, not by path: a sentry stuck behind a barricade it cannot
  reach shoots over it.
- The fists' tell frame flickers rather than following its exact tell (the renderer reads the
  marks from the state).

### Review (Wave 10 ancients-b review)

- **The unbound core's lane**: the second collapse's side platforms sat on the very row the
  bare core hovers along, so it drifted through them while chasing a rig. The platforms now
  sit one high in the middle (`hall.min.y + 6`) and two low at the sides (`+ 15`), the core
  hovers at `+ 11` between them, and `the_collapses_open_the_crypt_then_the_vault` checks the
  core's head-to-fists span (with its bob) misses every platform row. Runtime writes only: no
  digest moves.
- The Grand Bastion's note from Amos no longer names Mara (the town's salvage keeper, with no
  tie to the Fathers); it addresses whoever comes down after them.
- Re-checked: the probe (Heart Key 69.1 h, Descent VII ends 71.5 h, whole run 79.9 h, the
  Sleeping Garrison's 56-minute gap carried from the Archive of Stars above); the bot beats
  the Colossus in about 140 s of fight at 30 Hz with about 70 % hull left on hull tier 15
  (fair, perhaps on the easy side for a Descent VII gate with a maxed rack).
- Still weak: the Seal Gate's door is two tiles wide, so it reads as a glowing pillar with
  sockets more than a three-storey door; the slam's red floor mark is faint beside the rig;
  the hall's back wall reads near black (gold pilasters on dark bronze); scene captures of the
  forts show the milestone and tip toasts over the fort.

## Wave 10 heart: Descent VIII, the Heart, the final boss and the ending (`src/heart.rs`, `src/heart/{fields,ending}.rs`, `src/world/heart.rs`, `src/world/enemies/heart.rs`, `src/worldgen/descent/heart.rs`)

The last chapter of the Descent (rows about 11,400-13,000 of a V11 world, §14 Descent VIII)
stops being a placeholder. The planet's core is a sleeping **Star Drive** and the builders'
cable runs from it to the Moon. Every stratum has a signature rule introduced gently and pushed
deeper, its own rock (a `TileKind::Core` family), palette, ores and layer resource, native
creatures, set pieces, a music bed and an ambience bed, and a story beat; the Descent ends at
the Heart, the game's last fight, and the **ending**: the cable lights, the Star Drive becomes
the crew's engine and space past the Moon opens. The game keeps going afterwards (§18
decision 5).

### The strata (registry, `descent/registry.rs`)

| Stratum | Signature rule (pushed deeper) | Creatures | Set pieces | Rock, ores, layer resource | Music, ambience |
|---|---|---|---|---|---|
| Dark Matter Veins | **negative-mass ore**: every lump of dark matter in the bay takes 12.5 % of the rig's weight away (`heart::cargo_mass_permille`; eight lumps make an empty rig weightless, more and it **falls up** onto the ceilings); ordinary cargo adds 2.5 % back each once dark matter is aboard, so the load is planned; below a quarter of its weight the rig turns freely and the player rolls it over to thrust "down" (nothing turns it for them); dark matter weighs nothing on the drive | null moth (drawn to the rig carrying the most dark matter; its touch drinks a lump, `MothDrank`), upfall beetle (full of dark matter: hangs on roofs, drops on a rig straight below, floats back up), vein gnawer (burrower) | veins denser deeper, **weightless halls** with dark matter hanging from their roofs ("Weightless Halls" bigger and more of them), **the Upfall** (a shaft crusted with dark matter, a builder hauler stuck to its roof), Heart's Edge (the last outpost) | vein shale, null stone; mythril, uranium, diamond; Dark Matter | `DarkMatterVeins` (a weightless waltz, reversed celesta) / `VoidHum` |
| Singularity Fields | **gravity wells** (`heart::fields::well_field_source`, a field source): round caverns with a singularity at the middle; gravity turns toward the point and grows to 1.85 g there, so rigs, pilots, creatures, drops and charges all fall in; rotation is free in the strong pull; inside 2.6 tiles the **horizon** crushes a hull; light bends: **lensing** grows toward the point (a point-lens pass in the post-process, both GLSL variants) | orbiter (circles its well, slings itself out at a rig), accretion mote (swirls in a well's disc), horizon lurker (rooted on rims, fires bolts) | wells on a seeded grid, bigger deeper ("Lens Fields" fewer and bigger), event glass round each rim, **the Lens** (the greatest well) | event shale, horizon rock; mythril, diamond; Event Glass | `SingularityFields` (a bending drone, stretched bells) / `Gravity` |
| Core Ocean | **a liquid metal sea** (Wave 7 fluids, generated full and at rest) in basins between rock pillars under a rock sky, crossed on **magnetic rails** (`heart::MAG_RAIL`: the Crust's cart code with magnetic tuning, top speed 19 tiles/s): an upper line across the whole sea (tunnelled through the pillars, **boosters** early in each basin throw a rig to 17.5 tiles/s), a lower line in every basin reached by **switches** (hold down at the junction) with a **jump** over a gap onto a line one row lower, and the **magnetic catch** that grabs a rig touching down on a rail at speed; pylons stand in the metal; molten metal burns a rig that falls in | mercury eel (swims in the metal, leaps at a rig above; spawns only in metal), rail leech (crawls the rails), slag skimmer (low over the metal, spits slag) | the sea and its rails, ore islands near each basin's east end, **the Ferry Terminal** at the cable | core basalt, iron crust; platinum, mythril; Core Metal | `CoreOcean` (a rolling barcarolle over the rails' hum) / `MetalSea` |
| The Heart | **the sleeping Star Drive** at the root of the builders' cable (which runs indestructible through the rock from Heart's Edge down to the chamber, hanging free over the open places): the **Heart Key's seal** in the chamber's three doors (`WorldState::heart_seal_refuses`: only a rig with the Heart Key drills it), the final fight and the ending | corona wisp (a burning wisp of the drive's light), cable warden (builder guard machine, elite) | **the Heart's chamber** (radius 24 tiles at the cable's column near the world's bottom, a shell nothing breaks, a bowl floor, four ledges), star shards thick round it | star rock, drive alloy; mythril, diamond; Star Shard | `TheHeart` (a slow, enormous heartbeat under choir pads) / `StarDrive` |

`MusicCue::{DarkMatterVeins, SingularityFields, CoreOcean, TheHeart, HeartAwakening, Ending}`,
`AmbienceCue::{VoidHum, Gravity, MetalSea, StarDrive}` and `LandmarkKind::Heart` (generator
`LandmarkGenerator::Heart`) are appended.

### Generation (V11 only; `worldgen/descent/heart.rs`)

One pass in `streamed::assemble` after the far Strange Layers' (pure functions of the settings
and the tile): halls, veins and the Upfall; wells, their rims and the Lens; the metal sea, its
pillars, rails, ramps, boosters, pylons, islands and the Ferry Terminal; the cable; the Heart's
chamber and its star shards. It never carves the outposts or anomaly rooms. The registry's rock
for each stratum is a `CoreTile` (`TileKind::Core`, codes **`0x1A00`**), counted as plain rock
by the deposit, cave and hazard passes (`CoreTile::plain_rock`). The story's camps and rooms keep
off the set pieces and the sea's band (`heart::crosses_set_pieces`, appended to
`expedition::sites::DEEPER_SET_PIECES`). **`GOLDEN_V11_DIGEST` changed**; the V1-V10 digests are
unchanged.

### The Heart's fight (`world/enemies/heart.rs`)

Wakes when a living rig enters the chamber: the core (`EnemyKind::StarHeart`, its hit area its
drawn body: radius 72 px, contact 76 px) and six drive coils (`DriveCoil`). Every timing is a pure
function of world time since the phase began (`heart::pulse_at`, `breath_at`, `beam_angle`,
`coil_angle`), so hosts and clients agree and the section only changes on the fight's beats.

1. **The coils**: the six coils orbit the sealed core (it ignores every hit) and fire slow plasma
   bolts in turn round the ring; every 11 s the drive **pulls** (a contracting ring of light for
   1.8 s first, `PulseTell`, then 2.2 s of pull toward the core). Break all six.
2. **Ignition**: the core opens; two beams sweep the chamber (fading in for 2.4 s before they
   burn) and sparks fly (`HeartSpark`). Strike the core.
3. **Collapse** (below 52 %): the core becomes a singularity, pulls steadily and is armoured
   except while it **exhales** (it brightens for 1.6 s, `ExhaleTell`, then 3.9 s open, pushing
   out).

The core's touch burns. No countdown: with no rig in the chamber for 20 s it sinks back to sleep
and heals. When it falls it **wakes for good** (`heart_wakes`): `space_unlocked` is set, every
rig of the crew gets the **Star Drive** (later joiners too), and the cable lights
(`cable_lit_millis`, the ending's clock). On a Peaceful world the chamber yields unfought. The
gate (`GateDef`) still rewards `DescentKey::StarDrive`.

### Space and the Star Drive (the seam Wave 11 reads)

- `WorldState::space_unlocked()` (the world flag), `WorldState::has_star_drive(player)` (per
  player, every rig of the crew) and `WorldState::space_access(player) -> SpaceAccess {
  unlocked, star_drive, cable_lit_millis }` with `SpaceAccess::may_leave`. Saved and replicated
  in the shared `Heart` section. (The Star Drive is a set in that section, not a `RigSystem`
  bit: `rig_systems` is a full `u8`.)
- In the sky (above the ground line) a rig with the Star Drive gets `STAR_DRIVE_SKY_BOOST` (2.2x)
  on its engine's push, top speed and climb cap (`WorldState::star_drive_boost`, in the movement
  step; 1 everywhere else, so the classic physics stay exact).
- Flying more than 20 rows above the Moon after the ending fires `StarsOpen` once per rig and the
  HUD shows "THE STARS ARE OPEN".
- `WorldState::cable_path() -> CablePath { column, heart, surface_row, moon, moon_top }` for
  drawing the cable's light.

### The ending (`heart::ending`)

The simulation records one fact, `Heart::cable_lit_millis`; each seat plays the sequence for
itself from then (`EndingView`, a pure state machine of the time since the cable lit and the
seat's own skips): **Awakening** (0-8 s, in the world: the camera pulls back, the light leaves
the Heart and climbs the cable), **Ascent** (8-22 s: the planet in cutaway, the light climbing
ring by ring through all eight Descents, out of the ground and across the sky to the Moon, which
lights up), **Credits** (22-84 s, original names), **Epilogue** (16 s or until dismissed), then
back to play. Confirm, use, back, cancel or pause skips the seat's own view (to the epilogue,
then closes it), never before 1.2 s into a phase; on touch the pause button does. Split-screen
seats and online players each skip their own. Every rig is shielded from harm while it plays
(`Heart::ending_shields`, `ENDING_SHIELD_MILLIS`), then hulls take damage again.

### State, formats and events

- `Heart` (world field, appended shared `SectionKey::Heart`, saved with serde defaults, hashed
  once non-empty): floating rigs, rigs in a well, landmarks found, the fight, `space_unlocked`,
  the Star Drives, `cable_lit_millis`, rigs that saw the stars.
- Events: `GameplayEventKind::Heart { what, player_id, x, y }` (netcode `670 + HeartEvent::code`,
  670-694 used) and `HeartLandmarkFound { player_id, landmark }` (699).
- **`PROTOCOL_VERSION` 18** (this track's one bump; the integrator collapses). No save format
  bump (every new field has a serde default).
- The Crust's carts gained `crust::CartTuning` / `step_cart_tuned` (mine carts keep `MINE_CART`,
  bit for bit).

### Game side

- Tile art for every Core tile (`tile_art/heart_art.rs`): layered violet shale, dark matter as
  violet shards growing upward, banded event shale and pink event glass, rusty basalt columns,
  the rails' steel beam with its teal magnet strip on a see-through truss (ramps drawn slanted),
  boosters' lime strip, riveted pylons, warm star rock, drive alloy with gold seams, the
  chamber's circuit-lined shell, the seal's keyhole, the braided cable; each stratum's back wall.
  Liquid metal now glows molten (amber over rust).
- World (`rendering/world/heart.rs`): halos round glowing Core tiles; each well's horizon and
  turning accretion disc; the cable's dark line through the open deep and, after the ending, its
  light climbing from the Heart up the cable, out of the ground at its column and across the sky
  to the Moon's underside (drawn over the dark; seen from the Heart, the surface and the Moon);
  the Star Drive in its chamber with each phase's tell (the pulse's contracting ring, infall
  streaks, the two beams, the dark core brightening and the exhale's ring), awake and turning
  after.
- Lensing (`rendering/post_process.rs`): `uLens` (the well's point on the view, its reach and
  strength): a point lens with an Einstein ring and the horizon's shadow, in both GLSL variants,
  its strength the eased Lensing level (so the accessibility setting governs it); elsewhere the
  old centred bulge.
- HUD (`rendering/heart_hud.rs`): the weight meter with dark matter aboard, a well's pull, the
  rail ride, the Heart's fight (coils, health, the pulse, the beams, the breath), "the stars are
  open"; the camera pulls back and leans toward the Star Drive in its chamber and during the
  awakening. Story cards for the four landmarks (`StoryMomentKind::HeartPlace`) and news lines.
- The ending's screens (`rendering/ending.rs`): the awakening's title, the cutaway, the credits
  over the stars with the lit cable, the epilogue card; they cover the seat's view and cards.
- Sprites (`pixel_art/sprites/heart.rs`) for the fourteen kinds (the Heart itself is drawn at full
  size by the world renderer).
- Audio: twenty patches appended at the end of `patches!` (`audio/patch/heart.rs`: four music
  beds, the Heart's fight theme, the ending's theme, four ambience beds, the cable's light, the
  drive's inhale, pull, ignition and exhale, a coil breaking, dark matter's inverted chime, the
  rails' zap, a well's groan, the metal's splash), `heart_cue`; the director's `heart_music`
  plays each stratum's piece and bed, the fight's theme in the chamber, the ending's theme over
  everything while the seat's ending plays.
- Scenes: `heart_dark_matter_veins`, `heart_falling_up`, `heart_upfall`,
  `heart_singularity_fields`, `heart_lens`, `heart_core_ocean`, `heart_rail_switch`,
  `heart_ferry_terminal`, `heart_approach`, `heart_phase_coils`, `heart_phase_ignition`,
  `heart_phase_collapse`, `heart_awakening`, `heart_cable_surface`, `heart_cable_moon`,
  `heart_ending_ascent`, `heart_ending_moon`, `heart_credits`, `heart_epilogue`,
  `heart_stars_open`.

### Tests

`world/heart_tests.rs`: dark matter lightening a rig until it falls up, ore bringing it down,
alike at 30 and 60 Hz; a light rig turning freely and settling; a well pulling a rig in, bit for
bit twice and alike at 30/60 Hz, bending the light; the horizon crushing a hull; a mag-rail ride
that boards, boosts, switches to the low line, jumps and is caught; the Heart waking and sleeping;
**a bot with full kit waking the Heart through all three phases** (every pull told first, struck
while it exhales, more than 20 % of its hull left); a Peaceful world's open chamber;
`space_unlocked` and the Star Drive saved (JSON), replicated (the section) and reaching a later
joiner; the ending's shield; the Star Drive's sky boost and the stars over the Moon; the Heart
Key's seal; older worlds untouched; the creatures keeping the body rules; a null moth drinking.
`heart::ending` tests the state machine (phases in order, a skip to the epilogue and a close,
each seat skipping only its own, the light's climb). `heart::tests`, `heart::fields::tests`,
`worldgen/descent/heart/tests.rs` (the sea and rails, wells, the chamber and cable, veins and
halls, story rooms kept off).

### Seams and known limits

- Fluids are not pulled by the wells (a fluid cell has no position to move; the wells act on
  every body).
- The probe models the magnetic rails as ordinary travel and the Heart as fixed minutes
  (`boss_pattern`); its markers count the Heart's novelties and set pieces.
- The ending's camera does not travel up the cable: the awakening is in the world and the
  climb is the cutaway (the terrain far up is not streamed for a seat).
- Logs 73-80 (Amos alone in Descent VIII) are the story track's; the landmark cards here are
  written to agree with them.
- Wave 11 reads `space_access`; "the stars are open" is a message and the Star Drive a boost
  until space flight exists.
- Review fix: dark matter is a layer resource and takes no bay slot, so the weight
  (`WorldState::cargo_mass_of`) and the drive's cargo load count every slot of ordinary cargo
  (they used to subtract the lumps from the slots, so ore weighed less than the rule says);
  `dark_matter_makes_a_rig_lighter_until_it_falls_up_and_ore_brings_it_down` pins the world's
  weight to `cargo_mass_permille`.

### The probe (Wave 10 heart)

`fatherload --balance-report --only descent` on this branch: Descent VII ends at 69.6 h,
Dark Matter Veins at 72.3 h, Singularity Fields at 74.7 h, Core Ocean at 77.3 h and the Heart at
**80.1 h** (target 75-90 h), whole run 80.1 h; Descent VIII's strata wait at most 15-17 minutes
between new things (the over-40-minute gaps the probe still reports are Descent VII's, the
ancients tracks' placeholders on this branch). The Heart's fight scenes and the ending's scenes
start with the stratum banner and tips already read, so the captures show the fight.

## Wave 10 story: the finale, the deep settlements' stories and Descent progress (`src/expedition/{logs,memories,progress,ending,archivist}.rs`, `src/people/story/arcs/deep_settlements.rs`, `src/world/expedition.rs`)

Expansion plan §13 (following the builders' cable: the builders' city and archive, then the
Heart, a sleeping Star Drive; the ending opens space) and §18 decision 5 (the ending opens
space but the game keeps going), on the Wave 8 and 9 story's tables and passes. Everything is
V11-only, deterministic and read from state the world already saves and shares.

### The trail's end (`expedition::logs`)

- **Logs 61-80** follow the throughline table above row by row (strata, sites, authors and
  beats), in two chapters. **VII "The Builders' City"**: the streets and the archivist ("you are
  late"), the planet as the housing of an engine with the Moon its dock, Amos's leg broken in
  the Engine Halls, the halls as the drive's governors (the town's hum is its idle), the star
  charts (the first coordinates past the Moon), **Ines stays** in the Archive (log 67) to read
  a copy of the world and teach the archivist to read aloud, Amos alone among the war machines
  writing to his daughter **Wren**, the garrison waking, the Seal Gate's hollows for a key from
  every Descent, his letter home, and the Gatekeeper letting him through for the cable mark Bran
  painted on the sled. **VIII "The Star Drive"**: ore that weighs less than nothing, the hum as
  the drive's idle, the Moon seen through the rock in a gravity well ("I looked up. You were
  right."), the sled folding (he straps its name board to his back with Dov's recorder), the
  mag rails over the metal sea, a last letter to the five who stayed, the Star Drive asleep ("a
  star in a cage... I'll sit with it, and keep it") and log 80, **"Wake It"**: "You came down
  after us. Of course you did. Wake it. Go up. Wren, if it's you or yours: look up."
- **The Fathers' fate**: one by one they stayed with what was worth staying for (Teo, Sefa,
  Bran, Dov's grave, Ines); Amos reached the Heart with no rig and no key, tied the sled's
  name board to the cable root mark-up and kept the drive company until his lamp went out. His
  last site is drawn as **his chair**: the board across two stones, his rail crutch as its back
  with his hat on it, Dov's recorder on the seat, the tow rope to the gold-lit cable root.
- **Echoes 27-34**: the city's own recordings (the archivist translates: the last tram, the
  governors' song, the garrison's orders, the keys), **Ines reading aloud** in the Archive, and
  in Descent VIII **Amos on Dov's recorder** (the last echo: "When you wake it, I'll hear. (The
  tape runs out.)").
- `CHAPTERS` has all eight; every stratum of every Descent holds at least one log; logs stay
  under 340 characters with unique titles; days keep rising.

### The ending (`expedition::ending`)

`ENDING_TITLE` "The Cable Lights", `ENDING_LEAD` and `EPILOGUE`: eight beats bottom to top as
the light runs up the cable past each of the Fathers (Amos's chair, Ines's voice in the
Archive, Dov's headset singing one flat bar, Ruth-Little carrying the children out, Merrow City
singing Sefa's verse, Teo's pumps keeping time, **Wren Varga on her porch** watching the cable
light to the Moon, and the dock waking: "The Star Drive is your rig's engine now, and the sky
past the Moon is open. The Descent is over. Look up."). `ending_text()` keeps it for the
journal. The world goes on after it (§18 decision 5).

### Descent progress (`expedition::progress`, `ExpeditionState::{descents, bosses, cable_lit}`)

- `ExpeditionState` gains `descents` (bits by `progress::descent_bit`), `bosses` (bits by
  `progress::boss_bit`) and `cable_lit`, with serde defaults (older journals load empty), in the
  shared `Expedition` section, so partners and reloaded saves agree.
- The tick (`WorldState::track_descent_progress`, inside `advance_expedition`): an actor below
  the ground line marks its Descent reached (and every Descent above it as passed), raising one
  `GameplayEventKind::Story { what: StoryEvent::DescentReached(descent) }`. Every gate boss is
  checked each tick (`WorldState::descent_boss_beaten`): the Sinkhole by the Deep Claim, the Star
  Core by its spent phase, the Mind, the Leviathan, the Sun Serpent and the Glitch by their
  fights' `beaten`; a newly beaten boss raises `StoryEvent::BossBeaten(boss)`. Beating the
  Heart lights the cable once (`StoryEvent::CableLit`, `cable_lit`).
- **Seam for the Ancients and Heart tracks**: the Gatekeeper Colossus and the Heart have no
  fight state on this branch. Their fights call **`WorldState::note_descent_boss_beaten(boss)`**
  inside the tick when won (idempotent; a no-op on classic worlds); the next expedition pass
  raises the event and, for the Heart, the ending. Noted wins live in the high byte of `bosses`.
- `progress::descent_rows(state)` (per Descent: reached, its gate boss and whether it is won,
  logs read of its chapter, the chapter title once read) and `totals(state)` drive the page;
  `arrival_line(descent)` is each Descent's card ("Streets: lamps on poles, trams on rails, and
  a voice that has waited an age to say hello.").
- **Events 700-702** (the story's range): `StoryEvent::{DescentReached, BossBeaten, CableLit}`
  as `700 + code`.

### Milestones (achievements)

21 appended `Milestone`s, each with its own appended `PaintColor` reward (ramps and accents in
`rendering/rig_art/paint.rs`), all crew-wide from the shared journal (`MilestoneFacts` gains
`descents`, `bosses`, `cable_lit`) so every partner earns them, each earned once
(`Player::earn_milestone`):
- **each Descent reached** (II-VIII): Past the Lid, Where It Grows, Under the Sea, A Sun
  Inside, Thin Places, Swept Streets, Heartward;
- **each boss**: Out of the Sinkhole, Core Breaker, Clear-Headed, Leviathan's Bane, Sun Chaser,
  Bug Fixed, Colossus Toppled, and the Heart's is **Look Up** (the ending, platinum);
- **each later chapter read** (IV-VIII): The People Below, The Hollow World, Where the World
  Wears Thin, The Builders' City, The Star Drive;
- **all logs**: The Whole Trail (platinum).

The trophy shelf holds seven medals a row so all 47 stay on screen.

### The archivist's voice (`expedition::archivist`, for the Ancients track's companion)

30 `ArchivistLine`s with stable ids (`archivist.topic.name`) on every Descent, each of the
Fathers, the expedition, the Heart, the stars and after the end, each opened by the trail
(`after_log`), the Descents reached or the lit cable, so the companion never spoils. Learned
from Ines, it speaks with her turns of phrase ("Go on."). **Registration API** (the integrator
wires the companion to it): `remark(state, here, visit)` (what it says unprompted where the
crew is: the place on even visits, the Fathers on odd ones, after the end what comes next),
`answer(topic, state, visit)` and `topics(state)` (a talk menu of open topics), and the
`*_from(tables, ...)` forms that take the companion track's own `ArchivistLine` tables beside
`ARCHIVIST_LINES` in the same rotation and gating.

### The Descent VII and VIII outposts remember (`expedition::memories`)

`Settlement::{Megacity, HeartCamp}` (appended) with 11 lines (the outpost keeper, the tram
driver with Calloway's ticket, the scholar, a salvager who found Bran's splint, a child; the
Heart camp's keeper and a drifter who saw Amos's chair) and 5 more for the older settlements
(Cathedral Station's surveyor on the city, the queen's voice on the waking, Merrow City on the
charts, Ruth-Little's "everyone look up", the Prism Relay's needle). The Megacity's keeper tells
of **Wren Varga**, who came as far as the gold wall years ago and left a note: "I looked up
every night." Registration: `memories::ANCIENTS_OUTPOST_NAMES` and `HEART_CAMP_NAMES` (plan
names: "Megacity Outpost", "Lamplight Station", "Heartfall Camp", ...); **the integrator adds
the Ancients and Heart tracks' real settlement names there**, and their generated people speak
by trade (`speaker_for_role`: keeper, tram driver, scholar, salvager, child, drifter).

### The deep settlements' own stories (`people/story/arcs/deep_settlements.rs`, ids 1401-1463)

18 arcs on the people/story engine, two or three for every deep settlement, each tied to the
Fathers the settlement remembers and paid in the deep economy's coin (credits within the tier
bounds plus ending credits, layer resources, furniture, settlement goodwill, unlocked offers):

| Settlement | Arcs |
|---|---|
| Gravel Hollow (I) | Where the Rails Go (the rails are the builders'), It Worked (Teo's lucky stone), Brannigan's Stew (mostly onions) |
| Cathedral Station (II) | Calloway's Benchmarks, Halloran's Splice (a spare tape of his hymn), One a Day, Dov (Bran's lead tent) |
| The Amber Court (III) | The Wrench-One's Pumps (washers FOR THE NEXT WRENCH), The Humming Song (the walls hum Bran's song back) |
| Merrow City (IV) | Sefa's Second Verse (a song about rain), Pearls for the Bubble, A Jar of Rain (go up to the surface for it) |
| The Sunfolk Kraal (V) | Bran's Garden (FEVER, COUGH, SAD), The Other Half of the Board (AND THE SUNFOLK) |
| Parallax Station (VI) | Two Clocks (time breathes), The Real Canister (read in order) |
| The Megacity outpost (VII) | The Last Tram (Calloway's voice), I Looked Up Every Night (Wren's note and Amos's flower), Coordinates for the Archivist (the rig logged as a ship) |

Engine additions (all appended): `ArcKind::Deep` ("The Deep"), `Trigger::Settlement(memories::Settlement)`
(matched by the population's registered name, weight 60 so a settlement's own stories come
first there and are offered nowhere else; `SeedSource::Settlement`), a deep story is told once
(`offer_for` skips one finished with anyone), and two story places: `StoryPlace::FathersSite`
(the nearest expedition site, preferring the giver's own Descent; `BoundTarget::Expedition`)
and `StoryPlace::DeepSeam` (a seam in the settlement's own stratum below and beside it, clear of
the cable; `BoundTarget::Seam`), each with fallbacks so every arc still binds on V5-V8. Fix:
`StoryPlace::Home` for Cathedral Station's and the Amber Court's people bound the town; it binds
their hall now. Arc lints: up to 130 arcs; deep arcs go to the trail, a seam or home and are
offered only by their settlement; no other arc carries a settlement trigger.

### Game side

- A new Descent's card on every local seat (`StoryMomentKind::DescentReached`, "A NEW DESCENT",
  a lamp sliding down a plumb line); a log or echo raised the same tick takes the card.
- **The Descent progress page** (`ModalScreen::DescentProgress`, `shell/expedition.rs`,
  `rendering/expedition_hud.rs`) after the codex in the page cycle (tomb codex, journal, codex,
  **progress**, map): totals (Descents reached, gates won, logs found, the Heart asleep or
  awake), a row per Descent (its name once reached, its gate and whether won, logs read of its
  chapter), the selected Descent's chapter revelation or arrival line pinned under the list.
  Rows by keys, pad, mouse and touch (`menu_pick`).
- **The ending** (`ModalScreen::Ending`): when the Heart wakes every local seat gets the ending's
  pages: the cable drawn from the Star Drive to the Moon with a station per beat, lit up to the
  page, a soft glint climbing it (no flashing), the beat's place and words beside it. Confirm or
  a tap anywhere turns the page (the whole screen is one pointer row, and `pick_row` keeps the
  page); back closes it; the last page closes it and play goes on.
  Once the cable is lit, choosing the Heart's row on the progress page reads the ending again
  (review fix: it otherwise played once and no page kept it).
- Camps dressed for Descent VII (a builder street: tram rails, a street lamp that came on for
  them with its cold blue light, Amos's rail crutch from log 64) and VIII (Amos alone: the name
  board leaning on the tent, the sled gone, his crutch, gold motes rising), and Amos's chair at
  the last site with the cable root's warm light.
- Audio appended at the end of `patches!`: `story_descent_call` (a bell call over the cable's
  hum) and `story_cable_lit` (chords climbing from the Heart to the Moon under rising bells).

### Formats

- `PROTOCOL_VERSION` 18 (the journal's new fields, the story events, the finale milestones and
  paints, the story places and `ArcKind::Deep`; the integrator collapses Wave 10's bumps). No
  save format change (every new field is a serde default; appended enum variants).
- **`GOLDEN_V11_DIGEST` updated** to `0x6697_981c_edd7_02b3` (the story pass now places logs
  61-80; the last pinned log site is log 80's at the Heart). V1-V10 digests unchanged.

### Tests and scenes

- `expedition::tests`: logs and echoes in depth order through all eight Descents (80 logs,
  every stratum, chapters ending in their Descent's last stratum, Ines staying at 67 and Amos
  alone after, the last echo Amos's), placement deterministic, seeded and off the landmarks for
  all 80, the archivist's lines gated and rotated (extra tables join), the ending passing every
  Father, the progress rows against the registry's gates, the VII and VIII outposts' memories.
- `world::story_finale_tests`: Descents reached once with the ones above, milestones firing
  once and reaching partners; gate bosses tracked (own state and noted) and the Heart lighting
  the cable once with Look Up; logs 61-80 read in order with the sealed 80 opening after 79 and
  the chapter and trail milestones; the new fields saved, loaded, hashed, migrated from older
  journals and replicated; classic worlds untouched.
- `world::deep_arc_tests`: on a V11 world every deep settlement's residents offer its own arcs
  first and only its own, and a bot takes each on, plays it at its real places through the
  tick, turns it in and is paid; the Megacity's arcs bind (from a stand-in giver until the
  outpost registers) and play through; deep places bind deterministically inside the
  settlement's Descent. `people::story::arc_tests` lint the new arcs with the rest.
- Game: `expedition_tests` (a new Descent's card on both seats, the progress page by tabs and
  touch, the ending on every seat paged by taps and played once).
- Scenes: `story_megacity_camp`, `story_letter_home`, `story_heart_camp`, `story_last_log`,
  `story_last_site`, `story_descent_card`, `story_descent_progress`, `story_ending`,
  `story_ending_last`, `story_merrow_quest`, `story_merrow_rain`. Every expedition scene's
  journal now counts all Descents reached, so no new Descent's card covers it.

### Seams and still open

- **Integrator**: call `note_descent_boss_beaten(DescentBoss::GatekeeperColossus)` from the
  Ancients' gate fight and `note_descent_boss_beaten(DescentBoss::TheHeart)` from the Heart's
  (or read their states in `descent_boss_beaten`); add the VII and VIII outposts' real names to
  `ANCIENTS_OUTPOST_NAMES` / `HEART_CAMP_NAMES` (their people then tell the memories and offer
  the Megacity's arcs); wire the companion to `archivist::remark` / `answer` / `topics`; add the
  Ancients' and Heart's set pieces to `sites::DEEPER_SET_PIECES` so logs 61-80 keep off them
  (and update `GOLDEN_V11_DIGEST` if a site moves).
- The ending card opens on the `CableLit` event; the Heart track's own set piece (the cable
  lighting in the world, the Star Drive fitted) plays alongside it.
- Deep arcs have no prerequisites (they are only offered in their settlements, which only a rig
  that got there can visit).

## Wave 10 integration: Descents VII-VIII, the ending and the 60-90 h balance pass

The five Wave 10 tracks are stacked in this order on one linear branch: polish, ancients-a,
ancients-b, heart, story. Every commit compiles and passes clippy. Below are what the
stacking decided and the seams wired on top (`world/wave10_integration_tests.rs`).

### Shared files and formats

- **Appended enums, in track order**:
  - `EnemyKind`: the city's nine kinds, the garrison's nine, the Heart's fourteen; 150 in
    `ALL`.
  - `ShotKind`, `MusicCue` / `AmbienceCue`, and `LandmarkKind` / `LandmarkGenerator`
    (`AncientCity`, `Garrison`, `Heart`; 16 in `ALL`).
  - `SectionKey`: `AncientCity`, `Garrison` and `Heart`, all in `SectionKey::SHARED` (31).
  - `GameplayEventKind`: `Supply`, `AncientCity`, `Garrison`, `Heart` /
    `HeartLandmarkFound` and `Story`.
  - `StoryMomentKind` and `ModalScreen`: `StarCharts`, `DescentProgress`, `Ending`.
  - The game's `SoundCue` / `Patch` / `SeatLoop`. The audio patches are appended at the end
    of `patches!` in track order, and no patch name repeats.
- **Tile codes**:
  - The buried city's `TileKind::City` family is at `0x1C00`, and the Heart's
    `TileKind::Core` at `0x1A00` (the ancients-a review moved the city off the Heart's
    base).
  - The garrison's twelve `CaveTile`s take codes 160-171.
  - No range overlaps another.
- **Event codes**: polish 720-724, ancients-a 630-647, ancients-b 650-669, heart 670-694
  and 699, story 700-702.
- **Village sites**: only ancients-a adds one, the Archive Post (`deep_folk::ARCHIVE_POST`,
  `0x3FFF_FC00`).
- **`PROTOCOL_VERSION` 18**: one bump for the wave. The netcode doc lists every track's wire
  changes under one entry.
- **Save format**: stays 14. Every new field has a serde default, and the old-save
  migration test still loads formats 1-14.
- **Seat loops**: `SEAT_LOOP_SLOTS` is 128. The city's six beds, the garrison's five and
  the Heart's ten fit with room to spare. (The ancients-b commit that says 96 kept the
  integrated 128.)
- **`GOLDEN_V11_DIGEST` is `0xab0f_567c_0490_b4c9`**. The V1-V10 digests are unchanged.

### Conflicts the stacking resolved

- **Two Heart Seals, one key**: ancients-b seals Descent VIII under `CaveTile::HeartSeal`,
  and the heart track seals its chamber with `CoreTile::HeartSeal`.
  `WorldState::heart_seal_refuses` (in `world/garrison.rs`) now guards both. Both yield
  only to the Heart Key, which only the Colossus hands out.
- **Descent VII's outpost**: the story track wrote memories and arcs for a placeholder
  `Settlement::Megacity`. They are now the Archive Post's:
  - `Settlement::ArchivePost`, the ancients-a residents.
  - Speakers keyed `keeper` / `lamplighter` / `curator` / `tinker` / `sweeper`.
  - `ANCIENTS_OUTPOST_NAMES` is the Archive Post's names.
  - The deep arc tests play its arcs at the real Archive Post.
  - `HEART_CAMP_NAMES` starts with "Heart's Edge", Descent VIII's logistics outpost.
- **The codex's page cycle**: codex -> Descent progress -> star charts (Deep Descent worlds
  only) -> map. Tab-back reverses it. Each page's "Turn the page" row names the page it
  goes to.
- **The Heart Seal stays whole**: the Heart's own pass had cut weightless halls and veins
  into the seal band over Descent VIII, opening a way round the Heart Key in places.
  - Halls now keep off the seal's rows, and the pass never overwrites a seal tile.
  - `the_heart_seal_is_whole_over_the_hearts_own_halls` checks three seeds. The only
    other kinds in the band are bedrock and the cable's lift shaft.
- **The browser build**: the Heart's ending wraps its credits with `ui_kit::paragraph`,
  which is no longer desktop-only. With the GPU, the Heart's fight, the lensing wells, the
  garrison's defence and the Engine Halls all hold 60 fps in the browser, with 2-4 ms of
  frame work.
- **The probe**: each authored Descent's mechanics and set pieces are counted once
  (`living_dark_content`, `ancient_city::authored_content`, `garrison::authored_content`,
  `heart::authored_content`).

### Seams wired

1. **The archivist speaks the story's lines** (`ancient_city::archivist::STORY_LINE_BASE`).
   - Every other musing of the companion is one of the story track's 30 `ARCHIVIST_LINES`.
     The line is chosen by `expedition::archivist::remark` for the Descent the companion is
     in.
   - Each line is said once, under an id past 1000. The speech box reads it back with
     `archivist::line_text`.
   - Test: `the_archivist_muses_in_the_story_tracks_words`.
2. **The gate fights drive the story**. `descent_boss_beaten` reads
   `garrison.colossus.beaten` and `heart.fight.beaten`.
   - Beating the Colossus raises the story's `BossBeaten`.
   - The Heart waking (`heart_wakes`: `space_unlocked`, the Star Drive on every rig, the
     cable's light) also sets the story's `cable_lit` and `CableLit`.
   - Test: `the_gate_fights_drive_the_story_and_waking_the_heart_ends_it`.
3. **One ending, in order**. The story's ending pages wait for each seat's own Heart
   sequence to end or be skipped (`SeatUi::story_ending_pending`). The sequence is the
   awakening, the cutaway, the credits and the epilogue. After it, the seat reads "The
   Cable Lights". The pages can be read again from the Descent progress page.
4. **Logs 61-80 lie in Descents VII-VIII, off every set piece**:
   - the garrison's, through `crosses_set_pieces`;
   - the city's and the Heart's, through `DEEPER_SET_PIECES`.
   - Test (three seeds): `the_last_logs_lie_in_the_builders_world_off_its_set_pieces`.
5. **The Archive Post remembers the Fathers, and tells its arcs**. See the Archive Post
   entry under "Conflicts the stacking resolved". Tests:
   `the_ancients_and_heart_outposts_remember_the_fathers` and
   `every_deep_settlement_tells_its_own_stories_and_a_bot_sees_them_through` (seven
   settlements).
6. **The Seal Gate checks every key** (ancients-b's six sockets), and the Heart Key gates
   Descent VIII. Test: `the_heart_key_opens_both_seals_and_only_it_does`.
7. **Star charts for Wave 11**: `WorldState::star_charts()` / `star_chart_known()`. They
   are saved and replicated in the `AncientCity` section.
8. **The polish's rules on the new content**:
   - The HEAT warning test now covers Descents VII and VIII.
   - Hit areas come from every new kind's contact box. The game's art test holds all 150
     kinds.
   - The pilot on foot feels the Heart's wells, because they are a field.
9. **Per-stratum music beds**: every VII-VIII stratum has its piece and bed, plus the
   Colossus's theme, the Heart's fight and the ending.
10. **Saves**: a V11 world carrying the star charts, the Colossus's win, the Heart's flags,
    the ending's clock and the story's progress saves and loads to the same state hash.
    Test: `wave_ten_state_is_shared_and_round_trips_a_save`.

### The balance pass, all eight Descents

`BALANCE_PASS_DESCENTS` is 8. The Colossus's and the Heart's bot fights record their wins
(133 s with 71 % hull; 69 s with 63 % hull), so the probe models their fights from them.
`descent_balance_seeds` (release, `--ignored`) plays three seeds.

| Descent (h, cumulative) | target | default seed | seed 7 | seed 0xC0FFEE |
|---|---|---|---|---|
| I | 5-8 | 6.4 | 7.3 | 6.6 |
| II | 13-18 | 15.9 | 16.8 | 16.0 |
| III | 22-29 | 24.1 | 25.0 | 24.3 |
| IV | 31-40 | 34.4 | 35.3 | 34.6 |
| V | 42-53 | 45.3 | 46.3 | 45.5 |
| VI | 53-66 | 60.3 | 61.1 | 60.4 |
| VII | 66-82 | 73.3 | 74.1 | 73.4 |
| VIII | 75-90 | 83.8 | 84.6 | 83.9 |
| Whole run (60-90), deaths | | 83.8, 7 | 84.6, 7 | 83.9, 7 |
| Longest wait for something new | 20-40 | 16 min | 18 min | 16 min |

On the default seed, the Heart Key is won at 70.9 h. The Descent VII-VIII strata (default
seed):

| Stratum | minutes | ends (h) | longest wait (min) |
|---|---|---|---|
| Buried Megacity | 163 | 63.0 | 9 |
| Engine Halls | 148 | 65.5 | 11 |
| The Archive of Stars | 148 | 67.9 | 11 |
| Sleeping Garrison | 150 | 70.4 | 12 |
| The Seal Gate | 170 | 73.3 | 13 |
| Dark Matter Veins | 161 | 76.0 | 10 |
| Singularity Fields | 149 | 78.4 | 12 |
| Core Ocean | 155 | 81.0 | 12 |
| The Heart | 167 | 83.8 | 13 |

### Still open

- **Descent VIII has no resident settlement**. Heart's Edge is a logistics outpost with no
  people, so the story's `HeartCamp` memories and arcs wait for a population with one of
  `HEART_CAMP_NAMES`.
- **The archivist does not take questions**: the story's `topics` / `answer` menu is
  not wired. The companion only speaks unprompted, in its musings.
- **Two stings for one moment**: the Heart's `CableLit` sting and the story's
  `StoryCableLit` fire on the same moment. The Heart's ducks the mix.
- **Fuel is not stressful**: expeditions past Descent II still burn only 2-10 % of the
  tank. The balance pass reports this as a warning.
- **The final boss is short**: the bot wins the Heart's fight faster than the Leviathan's.
  It fights with the top railgun, about 11 minutes for a player.

## Wave 11 regions: many places at once (F11, `src/place.rs`, `src/world/places.rs`)

Expansion plan §15 F11. Players can be in different places at once: on the home world (at any
depth), on the Moon, on a space body, or in free flight, and the host simulates every one of
them each tick, while places nobody is in stay frozen.

### Vocabulary (`crate::place`, shared with every Wave 11 track)

| Type | Meaning |
|---|---|
| `SpaceBodyKey { sector_x, sector_y, system, body }` | a body the space generator placed (TODO-SEAM w11-space-gen: its own id converts into this or replaces it) |
| `BodyId::{Home, Moon, Space(SpaceBodyKey)}` | a place with its own ground |
| `Place::{Space, OnBody(BodyId)}` (`Place::HOME`) | where a player is |
| `StoreKey::{Home, Body(SpaceBodyKey), Space}` | the ground one pass of the tick runs over: terrain, entities, fluids. `Home` is the world's own (the home world, its sky and the Moon); `code()` is the netcode region tag (0 for home) |
| `RegionId { store, cell_x, cell_y }` | one `SECTOR_TILES` (512)-square cell of a store; the 3 x 3 cells round every occupant are awake (`occupied_cells`, `AWAKE_CELL_RADIUS`) |
| `BodyGenerator` | `extent()` (columns, rows, `wraps`), `surface_row`, `chunk_tiles(chunk)` (pure), `arrival_tile`, `fingerprint`. `FlatTestGenerator` (a flat dirt-and-stone strip, ±2,048 columns, 256 rows) stands in for every body; `EmptySpaceGenerator` is free flight's vacuum |
| `generator_for(store, seed)` | the one place the planet track wires its generator in (TODO-SEAM w11-polar: `crate::planet`) |
| `BodySource` | a store's generator with a small chunk cache; `Terrain::for_body(source, frame, edits)` builds a body's ground |

**The Moon is unchanged.** It still lives in the home terrain above its fixed column and plays
and generates exactly as before (no digest moved). `BodyId::Moon` only names it:
`WorldState::place_of` answers `OnBody(Moon)` for a home-store actor in the Moon's footprint
(`on_moon`); the Moon needs no store of its own.

### The world

`WorldState::places` (`Places`, serde default, saved, hashed only once someone has left the
home store): the place of every player away from the home store (`players`, pair list; a
parked player keeps its place) and the ground of every store someone has been to (`stores`:
`BodyWorld { terrain, entities, fluids }`, created lazily on first arrival from the world seed
and the key, kept frozen while empty). A body's terrain is a streamed `Terrain` whose chunks
come from its generator (`Terrain::for_body`); its settings record is the legacy generator's,
which turns every settings-driven content system (villages, sky, Moon, Descents, tombs, world
features, caves, POIs) off on the body, and its band/heat rules are the classic ones until the
planet track gives bodies their own (TODO-SEAM w11-polar). A body's terrain is never saved as a
`TerrainSnapshot`: `BodyWorld` serializes its store key and seed (the generator is rebuilt from
them), the generator's fingerprint and the terrain digest (both checked on load; a mismatch is
an error), its frame, its edited chunks as a `PackedChunks` stream, its entities and fluids.

Queries: `place_of`, `store_of`, `recorded_place`, `players_in(store)`, `store_terrain`,
`store_entities`, `store_frame`, `store_actor_position`, `rig_world_pos`, `occupied_regions`,
`active_store()` / `in_ground_pass()` (inside the tick: which ground is swapped in; the seam
gravity, flight and life support read, TODO-SEAM w11-flight / w11-polar), `ticks_by_region`.

**Travel: `move_to_place(player, place, WorldPos) -> Result<(), PlaceError>`** (used by flight
and landing, TODO-SEAM w11-flight). Teleport-safe: the rig keeps cargo, fuel, hull, velocity,
spin and every other piece of state (the whole `Player` is `reframe`d, so its ropes and hooks
move with it); its pilot on foot goes along at the same offset (moved between entity stores);
a drill in progress and a cart ride end. An empty destination store's floating frame first
moves onto the arrival's sector, so the arriving rig's local position is small and exact; the
chunks round the arrival are generated at once (`ARRIVAL_CHUNK_RADIUS`). Inside the tick the
move is queued and applied after the tick's last pass (no pass ever sees a player half moved),
then `GameplayEventKind::PlaceChanged { player_id, from, to }` (netcode code 740) reports it.
It runs inside the tick on the host and in every client's prediction alike, so it works online.
`PlaceError::{UnknownPlayer, OutsideGround}`. Test mode: `TestModeAction::WarpToPlace(place)`
(appended) warps to a place's arrival point (a body's generator's, the Moon's top, the spawn
point at home, the origin of free flight). `with_store_ground(store, |world| ..)` (doc-hidden,
outside the tick) swaps a store's ground in for tools, tests and generators that put things on
a body with the ordinary spawn helpers.

### The tick (`simulate_tick` -> `simulate_places`)

A world that never left the home store and does not tick by region runs its one pass exactly
as before (`simulate_pass`; the tick counter now advances once in `simulate_tick` after the
last pass, which is where it always was). Otherwise:

1. **The world pass** over the home store: the players away are stashed out (with their drills
   and drill timers), the inputs filtered to the players at home, every system runs (clock,
   economy, world events, Descents, people, quests, milestones, ...).
2. **One ground pass per other occupied store**: the store's `BodyWorld` is swapped into the
   world (terrain, entity store, fluid schedule), only its players are in, only the ground
   systems run (inputs and rig movement, commands, pilots, gas, heat, bombs, drilling, entity
   phases, pickups, test-mode refills, fluids, the journal and entity waking, chunk streaming
   with a per-tick budget, the store's own frame recentring). Building, infrastructure, rescue,
   the Moon's counter, lifts, homes, tombs and beds are `place::home_only_command`s, ignored
   away from home (TODO-SEAM w11-flight: rescue away from home is life support's). Discovery
   is the home map's: a body's reveals are dropped (TODO-SEAM w11-space-render: body maps).
   A store's frame move shifts only its own entities and players (`move_ground_frame`); the
   world's homes, sites and lost cargo stay in the home frame.
3. Queued moves apply; `PlaceChanged` events follow.

Entity ids and revisions stay unique across stores: every pass continues the world's counters
(the home store's `next_id` / `next_revision` are the world's), so replication keys need no
store. Tile writes of other stores come out of the tick as `TickOutput::store_terrain_ops`
(tagged with the store; `terrain_ops` stay the home world's). Events of every pass merge in
order (`TickOutput::merge_pass`).

**Regions: occupied ticking, sleeping frozen.** On V11+ worlds (`ticks_by_region`,
`GeneratorVersion::has_descents`) every pass computes the awake cells round its occupants (rigs
and pilots): entities outside them are frozen (`frozen_at`, checked in `advance_entities`):
kept, not ticked, their revision unchanged (so they cost nothing per tick or on the wire), and
they wake exactly where they stopped when someone comes back (expiries still fall due by tick).
A whole store nobody is in is frozen likewise. V1-V10 worlds tick every awake entity as before
(their golden digests, replays and hashes are untouched; V11's generation digest is unchanged:
`GOLDEN_V11_DIGEST` stays `0xab0f_567c_0490_b4c9`). Wake and sleep are pure functions of the
players' positions at the start of the pass, so they are deterministic.

**The per-tick budget** (`place.rs`): at most `MAX_AWAKE_STORES_PER_TICK` (8) stores besides the
home store get a pass each tick (more can only happen with more than eight players in eight
different places; the rest wait frozen and are served first next tick, round robin in key order
through `Places::cursor`, so nobody starves); a store's chunk streaming generates at most
`STORE_STREAM_CHUNKS_PER_TICK` (24) chunks per tick, nearest first (`Terrain::
stream_around_budgeted`), after the arrival chunks; inside every store only the 3 x 3 sectors
round each occupant tick. A tick therefore costs about one ordinary tick per occupied store.
Measured by `four_players_in_four_places_stay_within_the_tick_budget` (debug, a V11 world,
loaded machine): one rig at home 0.21 ms/tick; four rigs in four places (town, 3,000 rows
down, a space body and free flight) 0.51 ms/tick, worst 4.1 ms. The test asserts four places
stay under ten single-place ticks (plus 4 ms of slack for loaded machines).

### Hashing, replication and saves

* `simulation_state_hash` covers the places of the live players and every store's ground
  (fingerprint, frame, edits, digest, entities); `visible_state_hash(viewer)` covers the
  ground of the viewer's own store only (the home terrain's digest and entity records are left
  out while the viewer is away; the entity counters stay in). Nothing is added while the
  places are empty: older worlds hash as before.
* `PlayerTickState::place` (per tick): a rig's place travels with its position (which is in
  that place's store frame); a replica records it (`set_place_record`, creating the store).
* `LiveEntities::store_frames`: the frames of the stores the receiver holds (`live_entities_for
  (store)`), applied before anything else of the frame (creating stores a replica has not seen).
* Netcode (`fatherload_netcode::world`): chunks are `StoreChunk { store, chunk }`, entity records
  `PlacedEntity { store, entity }`, other stores' tile writes `WorldOp::StoreTile(store, change)`.
  A client sees the chunks, ops and entities of its own player's store only (a spectator sees
  every store), at any relevance radius; ops of other stores are skipped and their chunks marked
  stale, so coming back refreshes exactly what was missed. Poses carry `region` (`StoreKey::code`)
  and the store's frame origin; a place change changes the pose mode (the client snaps); a finite
  relevance never counts a player in another region as near. `region_origin(world, region)`
  rebases a pose onto its own place's frame. Keyframes carry every store (they are full
  snapshots); the first relevance frame then drops what the client may not hold.
* Saves (game format 15): `places` with every body's packed edits; formats 1-14 load into the
  home world (`Places::default`).
* Compression: a body's edits use the same in-house `PackedChunks` stream as the home terrain
  (runs of equal tiles, varints, base64 in JSON): a dug shaft is one run. No compression crate
  was added: none is in the workspace, the edits are already run-length packed (a lightly dug
  body is a few dozen bytes), and the browser build gains nothing from a second codec.

### Game side

`ViewSnapshot::{place, place_scene}`: a seat away from the home store gets a `PlaceScene` (its
store's terrain, entities, frame, rigs and pilots) and a camera in that store's frame; the home
world's readings (fields, wells, Hollow and Heart views, doors, counters, markers) are cleared
for it (`away_view`). `RenderSnapshot::players` holds the home store's rigs only.
`rendering::world::place_view` is the stub hook the space-render track fills (TODO-SEAM
w11-space-render): stars fixed to the absolute sky, a thin-air glow over a body's horizon, the
body's ground drawn with the game's own tile art (one terrain cache per store, lit round its
rigs), drops, pilots and rigs, then the seat's usual HUD. `view_frame::scoped(frame)` draws and
listens in a place's frame. Audio: an away seat hears its rig and its place (a body's low wind,
free flight's suit hum; the home world's ambience and music are not its own); `vacuum` follows
the place. Changed body tiles reach the caches as `FrameReport::changed_store_tiles` (host:
from `WorldOp::StoreTile`; client: digest-checked store edits). Scenes: `place_body_landed`,
`place_free_flight`, `split_two_places`, `split_four_places`.

### Tests

* `world::places_tests`: two players in two places both play each tick (a body dug, nothing
  leaking into home); an empty place freezes and wakes where it stopped, deterministically
  (two runs agree bit for bit); far home regions freeze on V11 (and never on V10); four
  players in four places within the budget; more stores than the budget take turns; a world
  with two bodies (edits, a drop) round-trips JSON and postcard to the same hash and plays on
  identically; a tampered body is refused; moving keeps cargo and momentum, refuses unknown
  players and places off the ground, and the test-mode warp goes through the tick; the Moon is
  a home-store body; entity ids stay unique across stores; a world that never left home hashes
  as before. `place::tests`: places map to stores, the flat generator is pure and seeded,
  awake cells.
* `netcode tests::world_places`: a client warps to a body, digs it while the host digs at home:
  the body's ground reaches it exactly, the home ops are withheld, only the body's entities are
  its, its visible hash matches the host's at every compared tick, and coming home refreshes the
  skipped chunks; a finite relevance keeps players in other places out.
* Game: `every_supported_save_format_loads_plays_and_round_trips` covers formats 1-15 (older
  ones load with nobody away; a format 15 world with a dug body plays and round-trips); the
  place scenes build headlessly (`every_scene_builds_headlessly`).

### Seams and known limits

* TODO-SEAM w11-polar: `generator_for` returns the flat test ground for every body; the
  polar/planet track's generator replaces it (and decides how a wrapping strip's seam works:
  `BodyExtent::wraps` is recorded, the terrain does not wrap yet). Bodies' heat and bands are
  the classic ones; radial gravity reads `active_store()`.
* TODO-SEAM w11-flight: Star Drive flight, landing and take-off call `move_to_place`; free
  flight's physics run in the space store's ground pass (today it has ordinary gravity over
  vacuum, as nothing replaces it yet); rescue and life support away from home.
* TODO-SEAM w11-space-gen: `SpaceBodyKey` unifies with the space model's body id.
* TODO-SEAM w11-space-render: `place_view` (space, polar planets, the flight HUD, camera roll
  away from home), body maps and discovery, place music.
* One frame per store, anchored on its first occupant: two players far apart in the same store
  (free flight is one store) keep `f32` precision relative to that anchor, as F7 documented
  for the home world. Splitting free flight into proximity clusters is a follow-up.
* A parked player keeps its place (parking is not leaving); its rig and pilot move with its
  own store's frame (`reframe_parked_in`) and never with the home frame, so it comes back
  exactly where it was parked (review fix: parking used to drop the place, so the rig came
  back on the home world at the body's coordinates).
* Infrastructure, scanner markers and collapse warnings are world lists (home only; building
  away from home is refused).
* `PROTOCOL_VERSION` 19, save format 15 (one bump each).

## Wave 11 polar: round bodies, curved tiles, radial gravity and planet ground (`src/polar.rs`, `src/polar/trig.rs`, `src/planet.rs`, `src/planet/{theme,landing}.rs`, `src/world/planet.rs`, expansion plan F12)

The polar track of Wave 11 (space foundations). It owns `simulation::polar` (geometry) and
`simulation::planet` (planet ground): a round body is a ring-tiled world whose gameplay runs
on its unrolled, wrapping strip with the existing terrain, dig, cave, fluid, drop, rig and
pilot code.

### Polar geometry (`crate::polar`)

- **`PolarGrid::new(radius)`**: rings of tiles, row = depth below the datum radius `R` (the
  top edge of row 0), column = angle clockwise from the body's top. The surface ring has
  `W0 = round(2 pi R / 512) * 512` columns (a whole number of sectors, so a lap of the strip is
  a whole number of `SECTOR_TILES`: R = 400 has 2,560 columns, 5 sectors, tiles 0.98 square).
- **Columns halve toward the core** at fixed rows (`halving_rows`) wherever a ring's tiles
  would get narrower than `1/sqrt 2` of a square, so every ring keeps its tiles between 0.707
  and 1.414 square down to the **core** (`core_radius = clamp(R / 12, 24, 640)`). R = 400 halves
  4 times (2,560 → 160 columns at the core, rows 0-366). A tile of ring `k` spans `2^k` strip
  columns (`span_at`); `tile_of`, `first_strip_column`, `neighbours` (left/right wrap, up/down
  cross a halving boundary: one inner tile under two outer ones).
- **Conversions**: strip point `(u, v)` (fractional surface columns, rows) ↔ body-centred
  Cartesian tiles (`+y` down, column 0 straight up) via `to_cartesian` / `from_cartesian`;
  `tile_point` / `point_tile` for a ring tile plus sub-tile offset; `to_world` / `from_world`
  for `WorldPos` in space around a body centre (`offset_world` splits whole sectors off in
  `f64`, so it is exact to the sub-pixel 10^12 tiles out); `local_frame(u)` gives local up,
  down, the tangent and the angle of up (what a craft's orientation reads as level there).
- **Deterministic trigonometry** (`polar::trig`): `sin_cos`, `atan`, `atan2` in `f64` from basic
  IEEE arithmetic (Cody-Waite reduction, Taylor series), identical on every platform including
  WASM, within 4e-15 of the platform maths.
- **The F12 table** is a test (`the_f12_table_bow_and_width_difference`): tile bow
  `32 / (8 R)` px (0.01 / 0.003 / 0.001 px at R = 400 / 1,500 / 5,000), width difference
  `1 / R` (0.25 / 0.07 / 0.02 %), horizon drop 20 tiles away 16 / 4.3 / 1.3 px.
- `MIN_LANDABLE_RADIUS` = 400 (moons and craters 400-1,000, planets 1,500-10,000; smaller rocks
  are asteroids mined from flight).

### Planet ground (`crate::planet`)

- **`PlanetParams { seed, radius, gravity_milli, theme }`** (serde, saved and replicated):
  `PlanetParams::new(seed, radius, theme)` clamps the radius to 400 and gives a placeholder
  gravity (0.33 g at R = 400, rising to 1.2 g); `gravity()` clamps to 0.25..2.5 g (above the
  field model's weightless line, so the smallest moon still has a "down").
- **`PlanetGround`** generates the strip as a pure function of the params: periodic value-noise
  hills (lattices of 32 and 128 columns, which divide every lap, so the seam is seamless), the
  theme's crust, two or three concentric strata with ragged boundaries, accent rock, ores per
  stratum, `caves_per_256_columns` elliptic caves (kept in shape across the rings), and a
  ring-shaped **core chamber** (`CORE_CHAMBER_ROWS` = 8 open rows with a few pillars, its floor
  lined with the theme's core ore) over two rows of indestructible bedrock (`CORE_FLOOR_ROWS`).
  Generation is per **ring tile**: every strip cell of a tile reads alike.
- **Theme registry** (`planet::theme`, append-only, the id is the index): `BARREN` (grey
  regolith moon), `ROCKY` (sand, dirt, stone, hard rock), `ICE` (snow, moon ice, granite). Each
  theme has a `PlanetPalette` (sky, atmosphere glow, surface, rock, deep, core colours) for the
  renderer, a crust, strata (`share_permille` sums to 1,000; rock, accent, ores per 10,000),
  a core ore, a cave density and a hill height. Waves 12-13 append real themes here; only
  existing `TileKind`s are used (no new tile codes).
- **`BodyGround`** is this track's minimal body-generator trait (TODO-SEAM, the regions track
  owns `BodyId`/`Place` and the real interface): `generated_tile`, `surface_row`,
  `strip_columns`, `top_row`, `height`, `gravity`. `PlanetGround` implements it.

### The strip is a streamed `Terrain` (`Terrain::planet`)

`Terrain::planet(params)` is an ordinary streamed terrain with a `planet` field:

- chunks come from the planet generator (`terrain::base_chunk`, memoized per process by
  params and wrapped chunk); `stream_around` wraps chunk columns and measures distance round
  the ring;
- **columns wrap**: `slot()` folds every column into `0..W0`, so a tile dug on one lap is dug on
  all of them, and edits are stored (and digested) at the wrapped position;
- **ring tiles stay whole**: a write that makes or breaks solid ground below a halving row
  writes every strip cell of its ring tile (open-to-open writes such as fluids flowing stay per
  cell, so fluid mass is conserved); drilling one cell of an inner ring opens the whole tile;
- `surface_row`, `min_y` (the strip top: hills plus 24 sky rows) and `height` (to the core) are
  the planet's; `layer_name` reads the theme's strata ("Core Chamber" at the bottom); the home
  world's caves, POIs, features and villages are switched off (`cave_at`, `caves_touching`,
  `pois_touching`, `features_in`, `feature`, `nearest_feature`);
- settings: `Terrain::planet_settings(seed)` (generator V5: no sky, Moon, villages, cave
  ecosystems or Descents; bands and rosters stand in for the classic rules); column limit
  `PLANET_COLUMN_LIMIT` (2^28) because positions never go far;
- the snapshot carries `TerrainSnapshot::planet` (serde default `None`, so every existing save
  loads unchanged) and a planet generator probe in its digest; the world hash adds the params
  only for planets (no existing hash or golden digest changed).

### The world on a planet (`world/planet.rs`)

- **`WorldState::new_planet_world(player, params, sim)`**: a single-rig world whose terrain is
  the strip, the rig three rows above column 64, and the body's **gravity zone**
  (`planet_gravity_zone`: a field zone over every tile, gravity `(0, g)`, free rotation).
  Gravity points at the body's centre, which in strip coordinates is straight down, so rigs,
  pilots, creatures, drops, charges and fluids all move in their local up/down frame through
  the existing F9 field readers with no other change. `tick_planet` (first thing in
  `simulate_tick`) puts the zone back if a test-mode "clear zones" removed it.
- **The frame wraps by whole laps**: `recentre_frame` labels a planet's floating frame within
  one lap (`wrapped_frame`: `sector_x` in `0..lap_sectors`), and `move_frame` shifts positions
  only by the nearest copy of the target round the strip (`nearest_lap_frame`). Relabelling a
  frame by a lap moves no position at all, so a rig drives round forever with bit-exact local
  positions; replicas that receive a relabelled frame shift by the same nearest copy.
- **Everything else stays on the nearest lap** (review fix): the frame follows only its anchor,
  so after the anchor drove once round, every other local position (a drop left on the ground,
  a creature, another player's rig) used to sit a lap away in local pixels, out of sight and
  reach. `tick_planet` now calls `wrap_to_nearest_lap`, which moves every rig, lost-cargo site
  and entity more than half a lap from the frame's origin by whole laps (exact: a lap is whole
  sectors). Entities that move get a new revision so replicas receive them.
- `WorldState::planet()`; the home town's doors are closed on a planet
  (`player_surface_zone` is `None`).

### Landing is a skill (`planet::landing`, in the movement step)

On a round body nothing turns the craft but the player (gravity never rotates it) and contact:

- **Flight**: rotation is free (no lean limit), thrusters spin it and spin keeps going; horizon
  hold and the gyro still work when fitted and switched on (off by default).
- **Touchdown** (`touchdown(lean, fall_speed, legs)`, lean measured from the ground's slope under
  the tracks): within `safe_angle` (12°, +12° per **landing legs** tier) it is clean; past it
  the craft **slides** the way it leans and, harder or more tilted, **scrapes** its hull
  (`SCRAPE_DAMAGE` per px/s per radian of excess); past `tip_angle` (50°, +10° per tier) it
  **tips over** (`TIP_DAMAGE` + scrape). Hull damage goes through the rig's mitigation and
  invulnerability.
- **Resting** (`rest_target`): on its tracks the craft settles onto the slope under it
  (`slope_angle` from the ground's top just outside each track, capped at 25°, walls ignored)
  at the attitude model's settle rate: a roll of a few tenths of a second, never a snap.
  Tipped, it settles onto its side (or its roof past 135°), cannot drive, and the player
  **rights** it by holding the rotation thrusters toward upright (`righting`): the craft rolls
  over its corner (contact lifts it up to `PLANET_ROLL_LIFT_PX` = 6 px) and settles back on its
  tracks.
- **Landing legs**: `AttitudeGear::landing_legs` (0-2; serde default 0). Upgrade hook: nothing
  sells them yet (space content); test mode fits them.
- **Events** (netcode 780-799): `GameplayEventKind::Planet { what: PlanetEvent, player_id, x,
  y }` with `Landed`, `Slid`, `HullScraped`, `Tipped`, `Righted`. The game: `Slid` clunks,
  `HullScraped` / `Tipped` play the hull hit, `Righted` the lever clunk (no new patches);
  messages for scraped, tipped ("Hold the rotation thrusters toward upright") and righted.
- `WorldState::step_rig_movement_into` is the movement step with its events (the old
  `step_rig_movement` is a test wrapper).

### Formats

- `PROTOCOL_VERSION` 19 (`TerrainSnapshot::planet`, `AttitudeGear::landing_legs`, events
  780-799). Saves: no format bump (both fields have serde defaults; old saves load as before).
- No `TileKind` codes, sound patches or world sections added. V1-V11 golden digests unchanged.

### Tests

- `polar::tests` (11): whole-sector surface rings, every ring 0.70-1.42 square for R 400-10,000,
  the R = 400 rings, the F12 table, strip/Cartesian round trips (2,000 random points per radius),
  ring tile + offset round trips through the strip and Cartesian frame, wrap, halving
  neighbours both ways (every column of every boundary), local frames, `WorldPos` round trips at
  10^9 and 3 x 10^12 sectors, purity; `polar::trig` against the platform maths.
- `planet::tests` (8): purity and cache, seamless wrap, hills and crust, ring tiles generate
  alike, strata/ores/caves/core chamber, every theme, params, a planet `Terrain` reads its
  ground and wraps. `planet::landing::tests` (5).
- `world::planet_tests`: **the moon bot** (R = 400, test-mode rig) lands, drives a full lap
  clockwise (the frame stays within its 5 sectors), digs to the core chamber (every halving
  ring's tile in the shaft is open whole) and flies back out above the hills; the bot twice at
  30 Hz gives the same state hash and events, and plays through at 120 Hz; gravity is the body's
  (fall distance matches 0.33 g) and survives a cleared zone list; gravity never turns the
  craft and the thrusters turn it freely; clean, sliding/scraping, legged and tipping
  touchdowns, the roll back onto the tracks (never more than 0.2 rad a tick), righting a tipped
  craft; one tile per place round the laps; JSON save and postcard keyframe round trips with
  mirrored edits; a replica follows the frame across the seam.
- netcode `tests::world_planet`: three clients over 120 ms / 2 % loss with duplication and
  reordering play a radius-400 moon from just short of its seam; every client matches the host.
- Game review scenes (drawn flat by the ordinary renderer until the polar renderer lands):
  `planet_moon_surface`, `planet_moon_spawn`, `planet_moon_halving_ring`,
  `planet_moon_core_chamber`, `planet_rocky_surface`, `planet_ice_surface`,
  `planet_tipped_over`. The game shows the home sky and sun over a planet for now (the
  space-render / regions seam); the home town's pointer and ambience are off on a planet, and
  the status line says how far round the body the rig is.

### Seams for the integrator and the other Wave 11 tracks

- **Regions** (`simulation::place`): implement the body generator for a planet `BodyId` with
  `PlanetGround` (or adapt `BodyGround`), build the body region's terrain with
  `Terrain::planet(params)` and add `planet_gravity_zone(&params)` to its fields; replace
  `new_planet_world` (a world that *is* one planet) with a body region.
- **Space gen** (`simulation::space`): give each landable body `PlanetParams` (seed from the
  world seed and its coordinates, radius ≥ `MIN_LANDABLE_RADIUS`, gravity, a theme id from the
  registry).
- **Flight** (`simulation::flight`): crossing into a body, place the craft with
  `PolarGrid::from_world(centre, pos)` (strip `u, v`) and turn its orientation into the local
  frame by subtracting `local_frame(u).angle` (leaving, add it back); never set the orientation
  for the player. Re-entry, docking and auto-land (the optional assist) build on
  `planet::landing`.
- **Space render**: draw the strip into a texture and bend it with the polar mapping
  (`to_cartesian`), or use `local_frame` per sprite; palettes from `PlanetTheme::palette`. A
  tile of ring `k` spans `2^k` strip cells: draw it as one curved tile.

### Known limits

- In strip coordinates a strip cell at depth is physically narrower (`r / R`): actors keep
  their strip size, so near the core a rig crosses a ring tile in several cells and moves
  physically slower sideways. The renderer may draw actors at a constant physical size.
- Gravity is constant with depth (a real body's pull falls toward the centre).
- No milestones or story for a lap or the core yet (content waves); no settlements, creatures
  or features of the planet's own (the classic band rosters stand in).
- A remote rig's netcode pose jumps by a lap when the host relabels its frame (interpolation
  sees a lap-long move once per lap); poses could compare modulo the lap.
- **Tile-keyed state is not lap-aware yet** (found in review): drill targets, bases, ladders
  and lifts, scan markers and the other maps keyed by absolute tile store the label the tile
  had when they were made. A frame labelled within `0..lap_sectors` gives the sector just
  before the seam two labels (about `-512..0` or `W0 - 512..W0`, depending on which way the
  anchor came), so a structure built there can be missed when the rig comes back the other
  way round. The terrain itself is unaffected (`slot()` wraps). The fix belongs with F11's
  per-body regions: wrap tile keys into `0..W0` where they are stored and compare tiles
  modulo the lap. Parked crew rigs are not lap-wrapped either.

## Wave 11 flight: the Star Drive, rotation in space, docking, life support and the nav HUD (`src/flight.rs`, `src/flight/{space,physics,nav}.rs`, `src/world/flight.rs`)

The flight track of Wave 11 (expansion plan F12 "Flight in space", §18 decision 1 and 4). A
rig with the Star Drive flies up past the Moon into space with no loading screen, turns all
the way round on its own thrusters, cruises across star systems, warps between stars, docks
at stations and lands on worlds. Life support and the nav HUD come with it.

### Leaving home and coming back

- **The space line** (`WorldState::space_line_row`) is `SPACE_ENTRY_ROWS` (400) rows above the
  Moon's top. Only Deep Descent worlds (V11+, `space_enabled`) have one.
- A rig crosses it into space when `space_access(player).may_leave()` holds (the Heart
  ending gives every rig the Star Drive). It must also be in the rig and not wrecked.
  Crossing raises `FlightEvent::EnteredSpace`.
- On crossing, the rig appears at `space::home_launch()`, just outside the home world's
  approach sphere. It keeps the climb's velocity and turn. Home's up is space's up there.
- **The Moon stays as it is**: it is part of the home world's sky. In space the home world
  is a round body (`BodyKind::Home`), drawn with its Moon. The home world gets no Moon
  pointer.
- **The parked rig**: while away, the rig's home-world position is parked at
  `WorldState::space_park()`, `PARK_ABOVE_ROWS` over the space line above the Moon's column.
  - Every away rig parks at the same spot, so netcode relevance keeps flyers in reach of
    each other.
  - The classic movement and the home world's hazard pass (vacuum, cold, heat) skip away
    rigs.
  - Commands that need the home world are refused in space. The scanner pings instead, and
    the rescue call is the beacon.
- **Coming home**: flying into the home world's approach sphere hands the rig back to the
  home sky, over the column it left from (`RigFlight::exit_column`), `RETURN_BELOW_ROWS`
  under the line.
  - Its velocity and orientation are turned from space's frame into the sky's: home's up
    where it came in becomes up, so a rig coming in belly first is level.
  - The home world's lean limit then eases it back inside ±35° as before.

### State and the step

- **`Player::flight: RigFlight`** is per tick, predicted and reconciled like the pose (it
  is in `PlayerRig` and `PlayerTickState`). It holds:
  - `place: FlightPlace` (Home, Space, Approach, Landed, Docked);
  - the exact space position `pos: WorldPos` (space pixels, 32 to a tile; never `f32`
    absolutes);
  - the velocity, the `StarDrive` mode (Off, Cruise, Spool, Warp with its `WarpRun`), hull
    heat, seconds of air, the exit column and the warnings already raised.
- **`WorldState::step_star_flight`** replaces the classic movement for an away rig, in the
  per-player input loop of `simulate_tick`. It handles:
  - the warp's spool and jump, then `physics::step_flight` (rotation, thrust, cruise,
    gravity, contact, heat), then fuel, hull damage, edge events and the handover home.
- **`WorldState::advance_star_flight`** runs after the per-player loop. It moves rigs that
  crossed the line into space, then runs life support and the beacon.
- Integration runs in fixed internal steps of at most 1/240 s
  (`physics::FLIGHT_STEP_SECONDS`), with deterministic trig. A warp's position is a closed
  form of world time (`physics::warp_position`). The result is deterministic and tick-rate
  safe; the tests compare 30, 60 and 120 Hz.

### Rotation in space (the player's, always)

- `attitude::AttitudeContext` gained `drag_scale`, `inertia` and `max_spin`. The home world
  passes 1, 1 and infinity, so its step is unchanged bit for bit.
- In space there is no lean limit and no angular drag. A spin keeps going until the player
  counter-thrusts (angular momentum is conserved), and the thrusters top out at
  `MAX_SPACE_SPIN` (3.5 rad/s).
- A laden bay spins up slower (`flight::rotation_inertia`: a full bay doubles the inertia).
  The rotation thrusters burn `ROTATION_FUEL_SHARE` of the main thruster's fuel.
- **Gravity never turns the rig.** `space::gravity_at` sums every nearby body's pull
  (`g = g_surface (R / r)^2`, blending between bodies). It only accelerates, and the tests
  check an idle tilted rig keeps its exact angle while it falls.
- **Thrust is craft-relative**: the main and retro thrusters push along the rig's up axis
  and the side thrusters along its right axis. `FlightControls::screen_relative` (an
  accessibility option) pushes along the chart's axes instead, and the game keeps that
  seat's camera chart-up.
- The camera is locked to the craft (game side), and the HUD stays upright.

### The Star Drive

- **Cruise** (`FlightCommand::ToggleCruise`, only in open space) follows the heading.
  - Its speed limit is the distance to the nearest surface over `CRUISE_TAU_SECONDS` (10 s),
    clamped to 600 px/s .. 20,000 tiles/s (`physics::cruise_limit`). Leaving a body it ramps
    up; nearing one it slows by itself.
  - Entering any approach sphere drops it out, capped to the impulse top speed.
  - Crossing a star system (120,000-250,000 tiles) takes 60-120 s, at 30, 60 and 120 Hz
    alike (`crossing_a_star_system_on_cruise_takes_a_minute_or_two`).
  - Forward throttles up, back brakes, rotation steers. Cruise burns 2 fuel a second.
- **Warp** (`FlightCommand::Warp`) goes to the waypoint's star system. It needs:
  - open space, and a heading within `WARP_ALIGN` (6°) of the bearing;
  - no surface within `WARP_CLEARANCE` (5,000 tiles);
  - `warp_fuel` aboard: 60 plus 8 a galactic sector.
- A warp then runs in three steps:
  - It spools for `WARP_SPOOL_MILLIS` (2 s), coasting; turning past 25° breaks it.
  - It crosses the real straight line to `WARP_ARRIVAL` (200,000 tiles) short of the star,
    eased (slow out, fast across, slow in), in 6 s plus 0.4 s a sector (30 s at most).
  - It drops into cruise at the system's edge.
- Passing within half the clearance of a body outside the target system pulls it out
  (`WarpInterrupted`, detail 1), and so does turning off the line.
- Refusals raise `WarpRefused` with a `WarpRefusal` code: no target, not aligned, gravity
  well, fuel, or not in free flight.
- **Online**, the flight state is per tick, so other players see a rig streak. A warping
  rig's velocity is the warp's real speed (the game draws the streak from it).

### Approach, re-entry heat and landing

- Inside a body's approach sphere, the place is `Approach(body)` and the view zooms in. The
  smallest sphere wins, so a station near a world takes over.
- **Re-entry heat** builds in a body's air:
  - It counts the speed into the ground plus 0.3 of the tangential speed, over
    `REENTRY_SAFE_SPEED` (450 px/s), times the air's density. A steep fast dive heats; a
    shallow glide does not.
  - Stars' coronas heat regardless of speed. Heat cools by 0.3 a second.
  - Over 1 the hull burns (30 hull a second for each unit over). `HeatWarning` fires on the
    edge past 0.6.
  - The **Re-entry Shield** upgrade (3 tiers) raises the safe speed and divides the heating.
- **Landing is a skill** (on `BodyKind::Planet`):
  - Touching down slower than 330 px/s into the ground and within `LANDING_TILT` (29°) of the
    local up lands (`Landed`). The rig then rests and settles level by contact, never by a
    camera snap.
  - A tilted touchdown scrapes the hull, bounces and tips the rig over (spin toward its
    lean). A fast one crashes (`HullScraped` with the damage).
  - Thrust that beats gravity along the local up takes off (`TookOff`).
  - A star's surface burns and pushes the rig back out. A station's hull stops it.
- **Auto-land** (`FlightGear::auto_land`, plus the seat's switch) acts only while both are
  on, within 10 tiles of a landable surface and descending. It holds the descent to
  150 px/s and levels to the local down. Without it the same drop lands hard.

### Life support and the emergency beacon

- `RigFlight::air` is seconds of air: `air_capacity(life_support tier)`, 360 s plus 360 a
  tier (the Moon's `LifeSupport` upgrade).
  - It drains in space, on approach and landed on airless worlds.
  - It refills at 30 a second landed on a breathable world, fully when docked, and on leaving
    home.
- `AirLow` warns at 50 %, 25 % and 10 %. The warnings re-arm over 60 %.
- At zero air, or with a wrecked hull, the **emergency beacon** tows the rig home: to the
  rescue crew's drop point, with at least half a hull and half a tank, for `beacon_fee` (120
  plus 5 % of the credits, never more than held). The beacon is also a command (the nav
  chart), so a stranded rig with no fuel or credits always gets home. It is never an
  unwinnable countdown.

### Stations (the stub Wave 12 fills)

- To dock (`FlightCommand::Use`), the rig must be within `DOCK_RANGE` (3 tiles) of the port,
  slower than `DOCK_SPEED` (70 px/s), within `DOCK_ALIGN` (15°) of the port's axis (its up
  axis pointing into the port), and off the drive.
- Docked, it is clamped (it holds still whatever the sticks do) and its air is full. Use
  again undocks it, backing off at 60 px/s.
- `FlightCommand::Station(StationService)` sells fuel (2 credits a unit), repairs (4 a hull
  point) and flight gear: the Nav Suite (3 tiers), the Re-entry Shield (3), the Gyro
  Stabiliser, Horizon Hold and Auto-Land.
- The home system's station (Halcyon Dock) sits 7,000 tiles across and 10,500 tiles up from
  the home world's centre, so the straight line from the launch point clears the home
  world's 8,500-tile approach sphere (flying through it hands the rig back to the home sky).
  Test: `a_bot_flies_from_home_to_the_station_and_docks_by_hand`, a bot with only the
  player's controls and no assists, cruises there, lines up and docks in about 65 s.

### Assists (all off by default)

- The gyro stabiliser and horizon hold are the Wave 7 attitude gear and switches. In space,
  horizon hold levels to the strongest body's local down, and does nothing where gravity is
  under 2 px/s².
- Auto-land is described above.
- Each works only when fitted **and** switched on (`ControlState::assists`, and the new
  `ControlState::flight` bits). Test: `assists_and_flight_switches_are_off_by_default`.

### Navigation (`flight::nav`, a ship upgrade)

- `FlightShared` is a world field in the appended shared `SectionKey::Flight` section. It is
  saved with serde defaults and hashed only once non-empty. It holds:
  - each player's waypoint (`NavTarget::System` or `Body`) and the crew's shared waypoint
    (`ShareWaypoint`);
  - systems found by flying (a warp's arrival) or scanning;
  - each player's last `ScanPing`.
- `WorldState::known_systems()` is home, then the Archive's charts, then the finds.
- **The scanner pings in space** (`PlayerCommand::UseScanner`, 2 s apart). It finds every
  system whose star is within `ping_range(nav tier)`: 40,000 tiles with no suite, up to
  1.2 million with the top one. A nebula star's fog (within 220,000 tiles of it) halves the
  range.
- The nav HUD's readouts (distance, `eta_seconds`, alignment) are presentation helpers here.

### Placeholders and seams (for the integrator)

- **TODO-SEAM(w11-regions)**: `FlightPlace` / `BodyRef` stand in for `place::Place` /
  `BodyId`, and the home world is `space::HOME_BODY`.
  - `FlightPlace::Landed` on a generated body should call `move_to_place(OnBody(..))` to hand
    over to the polar ground.
  - The parked home position stands in for per-region frames (F11).
- **TODO-SEAM(w11-space-gen)**: `space::PlaceholderSpace` is the galaxy:
  - home in galactic sector (0, 0), each Archive chart's system in its sector (y up the
    plane), and procedural systems in two sectors of three;
  - a star, 1-6 worlds of 400-3,000 tiles on static orbits, and one station per system.
  - `space::SpaceQuery` (`system_at`, `bodies_of`, `systems_near`, `bodies_near`, `system`,
    `body`) is all flight reads. The generator implements it and `WorldState::space()`
    returns the generator.
- **TODO-SEAM(w11-polar)**: a landed rig rests on the body's surface circle; the polar ground
  replaces it.
- **TODO-SEAM(w11-space-render)**: the game's `rendering/space_view.rs` draws placeholder
  space art (discs, glows, a starfield) and keeps the HUD and flight cues.

### Formats and codes

- **`PROTOCOL_VERSION` 19** (this track's one bump):
  - the `ControlState::flight` byte, `Player::flight` / `PlayerTickState::flight` and
    `HazardGear::flight`;
  - `PlayerCommand::Flight`, `TestModeAction::GrantStarDrive`, and the `Flight` world field
    and section.
  - An away rig poses at the shared park spot with netcode's `SPACE_POSE_MODE`.
- **Save format**: no bump. Every new field has a serde default.
- **Events 800-819**: `GameplayEventKind::Flight { what: FlightEvent, player_id, detail }`.
- No V1-V11 golden digest changed: generation is untouched.

### Tests

- `flight/tests.rs` covers:
  - conserved spin and counter-thrust, the top spin, a laden bay, rotation fuel;
  - gravity that pulls but never turns, blending between two bodies;
  - craft- and screen-relative thrust;
  - the cruise crossing in 60-120 s at three tick rates;
  - the warp's closed form, re-entry heat (steep, shallow, shielded);
  - landing, scraping, tipping and crashing, resting and take-off, auto-land only when
    fitted and on;
  - assists off by default, and the placeholder galaxy (deterministic, charts honoured,
    landable bodies 400+ tiles).
- `world/star_flight_tests.rs` covers:
  - leaving home with the Star Drive and not without it, the test-mode grant on V11 only;
  - the lean limit at home and its lifting in space, coming home belly-first;
  - the warp's spool, time and fuel at 30 and 60 Hz, refusal off the bearing, and a gravity
    well breaking it;
  - docking (misaligned refused) and the station's fuel and repairs;
  - air warnings and the beacon (also with no fuel or credits), the ping;
  - determinism and agreement at 30, 60 and 120 Hz;
  - saves and replication (keyframe, per-tick state, the `Flight` section).
- Netcode `world_rigs_fly_in_space_predict_and_converge`: two clients over 60 ms / 2 % loss
  fly in space and watch one rig warp. Every client converges with no hash mismatch.

### Game side

- **Controls in space** (`input_mapping::space_actions`, exhaustive touch answers):
  - the stick thrusts (up forward, down back, sideways strafe), and the rotation keys, d-pad
    or touch pair turn the rig;
  - the anchor button (`/`, Num9, LB held, touch CRUISE) toggles cruise;
  - the bomb button (touch WARP) warps to the waypoint, and use (touch DOCK) docks or
    undocks;
  - the scanner pings, and the map button opens the **nav chart**.
- Options gained **Auto-land** and **Space thrust: craft / screen** (saved; off and craft by
  default). The pause menu's test-mode entries gained **Grant Star Drive**.
- **The nav chart** (`ModalScreen::NavChart`) lists, with a nav suite, the known systems and
  the bodies of the current system to set a waypoint on. It also offers the warp, sharing or
  clearing the waypoint, the station's counter when docked, and the emergency beacon (with
  its fee). Without a nav suite it charts only the way home.
- **The station's counter** (`ModalScreen::Station`) opens on docking.
- **The space view** (`rendering/space_view.rs`, placeholder art):
  - the camera is locked to the craft, and the zoom follows the nearest surface;
  - a three-layer parallax starfield that streaks with speed;
  - lit discs for the bodies: star coronas, atmosphere glow, the home world with its Moon;
  - stations with solar wings, a pulsing port light and a dashed approach guide;
  - crewmates (streaks in warp), the warp's tunnel tinted by the target star, re-entry
    plasma and the ping's ring.
  - The warp tunnel and streaks scale with the screen effects setting. Nothing flashes.
- **The HUD**:
  - where the rig is and the drive (impulse, cruise speed, spool, warp %);
  - hull, fuel, air and heat bars, and plain early warnings (heat, air, fuel);
  - the docking guide (range, speed, alignment ticked off), and the prompts;
  - with a nav suite, the nav panel (systems three sectors around, the waypoint, distance,
    ETA, warp alignment) and edge arrows to the waypoint and every crewmate.
- Scenes (`scene_capture/flight_scenes.rs`): `space_leaving_home`, `space_full_turn`,
  `space_cruise`, `space_warp`, `space_nav_hud`, `space_nav_chart`, `space_docking`,
  `space_station`, `space_reentry`, `space_landed`, `space_split_screen`, and `touch_space`.

## Wave 11 carryover: the Wave 10 leftovers

The open items of the Wave 10 reports, closed (`ancient_city::questions`, `world/ancient_city.rs`,
`people::deep_folk`, `expedition::memories`, `people/story/arcs/deep_settlements.rs`,
`heart.rs`, `world/enemies/heart.rs`, `depth_progression::supply_strain`, `balance::descent`,
`world/supplies.rs`, `crust::pilot_heat_danger_levels`; game: `shell/archivist.rs`,
`rendering/ancient_city_hud.rs`, `rendering/pixel_art/font.rs`, `audio/director.rs`,
`rendering/heart_hud.rs`, `rendering/world/heart.rs`).

### 1. Asking the archivist (`ancient_city::questions`)

- `LogisticsCommand::AskArchivist(ArchivistQuestion)` (appended) puts a question to the
  player's **own** archivist. It is checked before the rig's tile, so it works in the rig, on
  a ride and on foot; with no archivist it is refused (`LogisticsRefusal::NoArchivist`,
  appended). It answers at once (no line gap: it was asked).
- **Questions** (`questions(state)`): **nearby ore** (it scans `ASK_SCAN_REACH_TILES` 16,
  wider than its own sweep, marks the best three like a scan and says what and which way),
  **our finds** (the logs read, the last one's title and author, the stars charted), then
  every open **lore** topic of the story's `expedition::archivist::topics` (the Descents
  reached, the Fathers and each of them, the Heart, the stars, after the end). Lore answers are
  the story's own lines, taking turns by `Archivist::asked` (serde default).
- **Answers are line ids** like every other line, so the `ArchivistSpoke` event, the
  archivist's `line` and the speech box carry them unchanged. The ore and finds answers are
  ids whose words are a pure function of the id (`answer_words`): `NO_ORE_ANSWER` 1999,
  `ORE_ANSWER_BASE` 2000 + find * 9 + direction, `FINDS_ANSWER_BASE` 3000 + logs * 513 +
  charts. `archivist::line_words` reads any line (its own, the story's past 1000, an answer).
  Nothing new is replicated beyond the command and the counter.
- **Game**: the **Ask Index** page (`ModalScreen::Archivist`, appended): the questions as
  rows (keys, pad, mouse and touch: a tap selects and asks), the fresh answer under the list
  and in the seat's speech box. It opens from the inventory's last row ("Ask Index", while an
  archivist rides with the seat's player) and sits after the star charts in the map's page
  cycle (the star charts' "Turn the page" goes to it, else to the map). Every seat asks for
  its own player; online the question is an ordinary command. Scene `city_ask_index`.
- Tests: `a_player_asks_its_archivist_about_ore_finds_and_lore` (refused without one, ore
  straight below named and marked, nothing near, finds, lore in turn, on foot, a partner
  refused, the answer in the section and the save), `questions::tests`, the game's
  `each_seat_asks_its_own_archivist_from_the_inventory` (two seats, inventory row, tap,
  keys, the page cycle) and `shell::archivist::tests`.

### 2. Heart's Edge has residents (`people::deep_folk::HeartsEdge`)

- `DeepSettlement::HeartsEdge` (appended; `HEARTS_EDGE` village id `0x3FFF_FB00`): Descent
  VIII's outpost hall over the Dark Matter Veins (`landmarks::outpost(.., Descent::Heart)`),
  its people in the half away from the lift's door like the Archive Post's. Five residents
  who followed the trail down long after the Fathers and stayed: **Bryn Ashcombe** (keeper),
  **Coll Fenn** (drifter), **Nan Orrin** (the listener, who counts the Heart's hum),
  **Ivo Strand** (rail hand) and **Elm Maddox** (medic). They remember the Fathers by what
  they found: Amos's plate on the lift door, the tally on the wall, the wiped magnet strips,
  the crutch's marks in the rock.
- Registered by name ("Heart's Edge", already first in `HEART_CAMP_NAMES`), so the story's
  `Settlement::HeartCamp` memories come up: seven new lines (`heart_camp.keeper.plate`,
  `listener.count`/`moon`, `railhand.strips`/`face`, `medic.peg`/`splint`; `MEMORIES` 73),
  and `speaker_for_role` gives the camp's storyteller and healer their keys.
- **Two arcs of its own** (ids 1464-1465, `Trigger::Settlement(HeartCamp)`): **Gone Down**
  (carry Amos's door plate on foot to his last camp; BACK SOON. HA.) and **Ninety Beats** (read
  the hum in the veins with the scanner: the ninety-first beat is the rig's own engine).
- Tests: `hearts_edge_has_residents_who_remember_the_fathers_and_offer_its_stories`, the deep
  folk cast test (four halls, 20 residents with lines), and the deep arc bot
  (`every_deep_settlement_tells_its_own_stories_and_a_bot_sees_them_through`, now eight
  settlements). Scenes `residents_hearts_edge`, `residents_hearts_edge_hall`.

### 3. The title font's M and W

The bold face smears each glyph one pixel right but never closes a one-pixel gap, so the
diagonals of M, N and W thickened on one side only: bold W read as N ("THE HEART NAKES") and M
as a lopsided N. `BOLD_GLYPHS` draws the three by hand (M and W seven columns, symmetric, the
vee open in the middle). Test: `bold_m_n_and_w_are_drawn_by_hand_and_read_apart`. Checked on
`heart_awakening` and `city_megacity_avenue`.

### 4. One sting at a time

The Heart's `CableLit` sting (`HeartCableLight`) and the story's `StoryCableLit` fire on the same
moment. The audio director now sequences the finale stings (`sequenced_sting`): one plays,
the other waits `STING_SECONDS` (3.6 s, as the first's tail lets go) and plays once; other
sounds are not held. Test: `the_hearts_sting_and_the_storys_sting_play_one_after_the_other`.

### 5. Deep expeditions are stressful: the supply line (`depth_progression::supply_strain`)

The deep tanks dwarfed a Descent's climb (expeditions 2-10 % of the tank). On a Deep Descent
world below the Crust, a rig more than `SUPPLY_FREE_ROWS` (450) rows below its nearest **supply
stop** (`Logistics::supply_row_above`: an outpost, a restored lift's station, any depth beacon,
or the surface) pays, for every second of thrust, a **share of its tank** on top of its burn,
growing over `SUPPLY_RAMP_ROWS` (1000) to `SUPPLY_STRAIN_MAX` (0.16 % a second). A share, not
units, so a deep tank does not dwarf it: the long climb home from a new Descent's bottom costs,
whatever the tier, while everyday trips from a beacon stay calm. `WorldState::supply_strain_for`
(movement step; not in test mode). The gate fights' arenas are destinations, not the open
deep: no strain in the Heart's chamber, the Gatekeeper's hall or the Glitch's and the
Leviathan's arenas. The HUD shows **FAR FROM SUPPLY** (`ViewSnapshot::supply_strain`). Classic worlds and the
Crust never feel it. Test: `far_below_every_supply_stop_thrust_strains_the_tank_and_a_beacon_eases_it`.

Review fixes: a rig standing in an outpost's hall is above the hall's floor row, so the first
cut skipped that outpost and measured from the stop before it (Cathedral Station and Heart's
Edge showed FAR FROM SUPPLY, and charged it, in their own halls). A stop up to
`logistics::SUPPLY_STOP_SLACK_ROWS` (16) rows below the rig now counts as the rig's own; the
test checks every outpost's hall. The game's onboarding adds a one-time tip per seat
(`Hint::SupplyLine`, appended) the first time the strain bites: what it is, and that a depth
beacon extends the line.

The descent probe models it on every trip (`supply_strain_share` over the climb back to the
trip's stop) and on every expedition (the Descent's span, plus two minutes at the bottom), and
now **fights the Heart** at the end of Descent VIII from its bot fight (the gate loop never did:
the Heart rewards no rig system). `descent_balance_pass` prints each Descent's fuel.

| Descent | ends (h), default | seed 7 | seed 0xC0FFEE | target | everyday fuel | expedition fuel (was) |
|---|---|---|---|---|---|---|
| I | 6.4 | 7.3 | 6.6 | 5-8 | 49 % | 28 % (28) |
| II | 15.9 | 16.8 | 16.0 | 13-18 | 15 % | 33 % (10) |
| III | 24.1 | 25.0 | 24.3 | 22-29 | 17 % | 24 % (4) |
| IV | 34.4 | 35.3 | 34.6 | 31-40 | 14 % | 30 % (4) |
| V | 45.3 | 46.3 | 45.5 | 42-53 | 14 % | 29 % (4) |
| VI | 60.3 | 61.1 | 60.4 | 53-66 | 13 % | 30 % (2) |
| VII | 73.3 | 74.1 | 73.4 | 66-82 | 13 % | 29 % (2) |
| VIII | 84.2 | 85.0 | 84.3 | 75-90 | 12 % | 25 % (2) |

Whole run 84.2 / 85.0 / 84.3 h, 8 deaths (the Heart's learning try is now counted), longest
wait for something new 38 min (the Heart's stratum with its fight; inside 20-40). No warnings
and no misses on any seed (`descent_balance_pass`, `descent_balance_seeds`).

### 6. The Heart is a proper finale (five beats)

`HeartPhase::{Reforge, Eclipse}` (appended), `HeartEvent::{Reforged, Eclipse}` (appended,
netcode 695-696), `HeartFight::{inner, rekindled}` (serde defaults):

1. **The coils** (as before).
2. **Ignition** (as before) until `REFORGE_AT_PERMILLE` (78 %).
3. **Reforge**: the core reseals and calls `INNER_COIL_COUNT` (3) coils that orbit close
   (`INNER_ORBIT_TILES` 6.5) the other way round (`inner_coil_angle`) and fire every
   `INNER_FIRE_MILLIS`; the gravity pulses return, and the inner coils are **shielded except
   while the drive pulls** (they glow gold then; a copper cage while calm): strike them in the
   pull, which drags the rig toward the core. Breaking them **rekindles** the core.
4. **Ignition, rekindled**: its two beams turn back the other way, faster
   (`rekindled_beam_angle`), until `COLLAPSE_AT_PERMILLE` (52 %).
5. **Collapse** (as before) until `ECLIPSE_AT_PERMILLE` (26 %), then the **Eclipse**: the
   breathing goes on and one beam sweeps the chamber while the core holds its breath; it goes
   dark as the core exhales (`eclipse_beam`), the window to strike.

Every timing stays a pure function of the phase's world time (`fight_beams` gives the tick and
the renderer the same beams). Fairness: in the breathing phases sparks come every
`BREATHING_SPARK_MILLIS` (10 s), at most `BREATHING_MAX_SPARKS` (3), and each **exhale blows
them out**, so the long phases never swarm and the strike window is clear. The core holds
1560 hull (760). HUD lines, news, sounds and scenes `heart_phase_reforge`,
`heart_phase_eclipse`.

**Bot fight re-measured** (`a_bot_with_full_kit_wakes_the_heart_through_all_five_phases`; the bot
now also shoots sparks while the core is armoured, as a player would): **163 s with 55 % hull**
(was 69 s, 63 %), phases coils 17 s, ignition 13 s, reforge 29 s, rekindled 15 s, collapse 44
s, eclipse 44 s. The probe's player fight is 6 + 163 s x 4 = **about 17 minutes** (was 11),
plus one learning try.

### 7. Smaller fixes

- **Beast Bait lets go**: a baited predator's old home is kept (`SupplyEffects::baited_homes`,
  serde default) and given back when its bait is spent or out of reach. Test:
  `a_baited_predator_goes_back_to_its_old_home_when_the_bait_is_spent`.
- **The HEAT badge on foot** reads the pilot's suit where the pilot stands
  (`crust::pilot_heat_danger_levels`, which the pilot's burn reads too), not the parked rig's
  radiator. Test: `on_foot_the_heat_warning_reads_the_pilots_suit_not_the_rigs_cooling`.
- **Old tinted lamps wear off**: a lamp an older save tinted with no glass fitted gets one set's
  life on the next tick (`glaze_unglazed_lamps`), then wears like any other. Test:
  `an_older_saves_tinted_lamp_without_glass_wears_off`.

### 8. The deep posts talk about their own place

`Cast::chatter` (three lines each): the Archive Post's and Heart's Edge's residents say their
own everyday talk (`deep_folk::chatter`, by day and visit) in place of the people system's
generic chatter, between their greeting and their memory; the builders' machines show
"builder machine" for an age in the talk header. Cathedral Station keeps the generic chatter,
the Hive folk keep to their own words. Test:
`the_archive_post_and_hearts_edge_talk_about_their_own_place`.

### Formats and seams

- **`PROTOCOL_VERSION` 19** (this track's one bump; the integrator collapses Wave 11's):
  `LogisticsCommand::AskArchivist`, `Archivist::asked`, `SupplyEffects::baited_homes`, the
  Heart's appended phases, events and fight fields, Heart's Edge's residents in NPC entities.
- **No save format bump**: every new field has a serde default; enum variants are appended.
- **Generation unchanged**: the V1-V11 golden digests are untouched (residents, memories and
  arcs are not terrain).
- Event codes: only the Heart's own range (695-696); no new carryover codes were needed.
- Seams for the Wave 11 tracks: `ArchivistQuestion` is the place to add space questions
  (append variants; answers can be new id ranges past 44,600); the supply line reads
  `Logistics::supply_row_above`, so a future stop kind (a docked ship, a space station) only
  has to be counted there.

## Wave 11 integration: space foundations stacked and wired

The six Wave 11 tracks (regions, space-gen, polar, flight, space-render, carryover) are stacked
linearly on the Wave 10 master (f33d989) and their seams wired so the whole loop plays end to
end: the Heart's ending gives the Star Drive, a rig flies up past the Moon into space, warps to
a system from the Archive's star charts, lands on a round moon, drives all the way round it,
digs toward its core, takes off, warps home, docks at Halcyon Dock and flies back into the home
sky; single-player, online with another player elsewhere, and split screen with seats in two
places.

### Formats

- `PROTOCOL_VERSION` **19**: one bump for the wave (regions, polar, flight and carryover each
  bumped to 19; the doc on the constant lists every track's changes).
- Save format **15** (the regions track's `places`; flight, polar and carryover need none:
  serde defaults). `every_supported_save_format_loads_plays_and_round_trips` covers 1-15.
- Golden digests: V1-V10 and `GOLDEN_V11_DIGEST` (`0xab0f_567c_0490_b4c9`) unchanged (home
  generation is untouched). `GOLDEN_SPACE_DIGEST` is `0x0c0c_b46c_12ea_7136` (Halcyon Dock, the
  home system's station, added to the generator; nothing else of space moved).
- Event codes: regions 740, polar 780-784, flight 800-819 (carryover adds none). No new
  `TileKind` codes, sound patches or world sections beyond the tracks' (`SectionKey::Flight`).

### Conflicts the stacking resolved

- `terrain.rs`: the regions track's body source (`Terrain::for_body`, extent) and the polar
  track's planet strip (`Terrain::planet`) both live in `Terrain`; one chunk source
  (`base_chunk(settings, body, planet, chunk)`), and every content switch-off reads either.
- `GameplayEventKind`, `event_cue`, `effects`, netcode's event codes, `TestModeAction`
  (`WarpToPlace`, `GrantStarDrive`), the `WorldState` exhaustive destructures (`places`,
  `place_pass`, `flight`): appended in track order. `remove_player_with` forgets both the place
  (unless parked) and the flight waypoint.
- `simulate.rs`: flight's world pass (`advance_star_flight`) runs only in the home world pass.
- Movement: the polar planet attitude context gained flight's `drag_scale`/`inertia`/`max_spin`
  (identity values: the ground keeps its behaviour).
- Scenes: flight's `space_cruise` / `space_warp` are `space_flight_cruise` /
  `space_flight_warp` (the space renderer's staged views keep theirs). The shell's
  `ModalScreen` gained `NavChart`, `Station` and `Archivist`.
- The space-gen reviewer's unfinished platform-determinism fix (exact integer extents, no libm
  `hypot` or `mul_add` in space) is committed on its own; the golden digest did not move.

### Seams wired

- **One body id.** `place::SpaceBodyKey` **is** `space::SpaceBodyId`; `BodyId::{Home, Moon}`
  map to the generator's `HOME_WORLD_BODY` / `MOON_BODY` (`BodyId::space_id`, `of_space`).
  Flight's `BodyRef` (any flight body: suns, bodies, stations) maps to and from it
  (`GeneratedSpace::space_body`, `body_ref`); the renderer's `BodyView::id` is its `mix()`.
- **Planet ground in the regions' stores.** `BodyWorld::generate` builds a generated body's
  store as the polar track's planet (`place::planet_params_for`: the space body's ground seed,
  radius, gravity and theme; `planet_theme_of` maps the twelve themes onto the three ground
  themes so far), created lazily on first arrival, ticked when occupied (one ground pass,
  `tick_planet` keeps its gravity zone and lap wrap), frozen when empty, saved by store and seed
  with a fingerprint and digest check and its edits packed, replicated per store by relevance.
  Its gravity zone is the store's own field zone (swapped in with its ground: home field zones
  never reach a body). The ground frame is lap-aware (`wrapped_frame`, `nearest_lap_frame`); a
  ground pass leaves the world's lost-cargo sites alone. A key that names no landable body of
  the world's space keeps the flat test ground (tests, tools).
- **Flight in the real galaxy.** `flight::GeneratedSpace` implements `SpaceQuery` over
  `Space::shared(seed)` (one cache per world): every system of a sector (1-3), its suns, its
  bodies (planets, moons, crater rocks: gravity `gravity_milli_g` x the rig's own 440 px/s²; air
  by atmosphere), its stations; galactic sectors are space sectors (2^22 tiles). The departure
  is the generator's (above the Moon); the home world and the Moon are `BodyKind::Home`
  (flying into either approach sphere hands the rig back to the home sky). Halcyon Dock is a
  real station of the home system (`HALCYON_DOCK_OFFSET`, home centre + (3,000, -42,000)
  tiles, outside the home world's and the Moon's reach), its port facing the departure point.
  Charts resolve to real systems (`chart_system`, legacy charts too); the scanner's nebula
  check reads the generator's fog. `place_of` answers `Place::Space` for a rig under the Star
  Drive (its home-store slot parked over the Moon, its space position exact in
  `RigFlight::pos`).
- **Landing and take-off** (`world/body_flight.rs`). A rig in a landable body's approach
  sphere that comes down `LAND_INSIDE_ROWS` (8) inside the top of the body's strip is handed to
  its store (`move_to_place(OnBody(Space(id)))`): the strip point under it
  (`PolarGrid::from_world`), its velocity turned into the strip's frame (along the ground,
  toward the core) and its orientation measured from the local up there; nothing levels it, and
  the polar track's landing skill (touchdown, slide, scrape, tip, legs) takes over.
  `FlightPlace::Landed` now means "on a body's ground" (not away: the ground code moves it).
  Climbing `TAKE_OFF_ABOVE_ROWS` (2) over the strip's top hands it back to space at the
  matching point round the body (`PolarGrid::to_world`), turned and moving as it left. Life
  support counts on a body (refilling on a breathable world); the beacon (air out, wrecked, the
  rescue or beacon command) tows a rig on a body home to the spawn point.
- **Rendering.** `rendering/space/live.rs` gathers what the generator placed round a camera as
  the renderer's views; `rendering/space_seat.rs` draws each seat: in free flight the real
  galaxy (flight's stations, pings, crewmates, warp tunnel, rig and HUD over it), on a body its
  round ground through the polar renderer (`LiveStrip`: the polar grid's rings, a ring tile of
  band k kept across 2^k strip columns), the camera locked to the craft (its turn relative to
  the local up), the HUD upright. Every seat keeps its own polar ground textures; a body
  store's tile writes mark that seat's ground (`mark_tile_dirty` wired). The flight zoom
  frames a black hole's disc. The stratum banner stays quiet away from the home world.
- **The bot** (`flight::autopilot::SpaceBot`, doc-hidden): legs of a journey flown with the
  player's controls only (rotation thrusters, main/retro/side thrust, cruise, warp, use; no
  assist): leave, warp, waypoint, land, drive, dig, take off, dock, undock, home.

### End-to-end tests

- `world::space_journey_tests`: the whole loop above on a V11 world (the Heart's ending via
  `heart_wakes`, a chart pushed as if read at a plinth), twice at 30 Hz (bit-identical end
  state) and once at 60 Hz. About 19,400 ticks at 30 Hz (10.8 minutes of play): the warp to
  the chart 54 s, the approach and landing 146 s, the lap of a 3,072-column moon 93 s, 24 rows
  down in 26 s, the warp home 56 s, cruising in and docking 171 s, home 98 s.
- netcode `tests::world_space_journey`: a client flies the journey online (a quarter lap and
  8 rows of digging) while the host's own rig digs at home; the client's bot reads only its own
  world, its view matches the host's at every leg, it holds the moon's strip exactly, and home
  catches up on return.
- game `local_game::tests::space_seat_tests`: split screen, one seat on the charted moon's
  round ground, the other docked at Halcyon Dock; each seat's view shows its own place and the
  game ticks both.
- Scenes on real worlds: `space_landed` (a home-system world's store), `space_moon_approach`,
  `space_moon_ground`, `space_moon_far_side`, `space_moon_shaft`, `space_split_places`,
  `space_near_black_hole`; flight's other scenes now draw the real galaxy.

### Still open (Waves 12-13)

- Free flight's store (`StoreKey::Space`) is only the test-mode warp's: a rig under the Star
  Drive stays in the home store's parked slot; players far apart in free flight never share a
  ground pass (their positions are exact `WorldPos`es).
- Ground themes: three (barren, rocky, ice) stand in for the twelve space themes; no planet
  settlements, creatures, weather or day length yet; a planet's heat and bands are the classic
  ones. A rig on a body keeps the home world's clock and economy.
- Coming home always puts the rig over the column it left from; the Moon's approach sphere
  hands it back like the home world's.
- Black holes, wormholes, nebula fog and asteroids are drawn but have no flight physics yet
  (no lensing pull, transit or mining from flight); stars pull with flight's per-class
  surface gravity.
- Tile-keyed state on a body is not lap-aware yet (the polar track's known limit); body maps
  and discovery are the home world's.
- Browser: the polar ground and the galaxy draw in WebGL1 (GLSL ES 100) as the space-render
  track timed; see the integration report for this stack's numbers.

## Wave 12 systems: what space's things do, and the Wave 11 flight leftovers (`src/flight/systems.rs`, `src/world/space_systems.rs`, game `rendering/space_systems_view.rs`, `rendering/body_map.rs`)

Expansion plan §16 "Star systems". Wave 11 drew stars, black holes, wormholes, nebulae, belts
and comets; this track gives them physics, and closes the flight track's open items. Everything
is a pure function of the world seed (the flight body list), the rig's exact `WorldPos` and the
world clock, integrated inside flight's fixed substeps, so host, predicting clients and every
tick rate agree. No generation changed: V1-V10, `GOLDEN_V11_DIGEST` and `GOLDEN_SPACE_DIGEST`
(`0x0c0c_b46c_12ea_7136`) are untouched.

### Flight bodies grew (`flight::space`)

- `FlightBody::extra: systems::BodyExtra` (`Star(StarFacts)`, `Hole(HoleFacts)`,
  `Mouth(MouthFacts)`, `Rock(RockFacts)`, `Comet(CometFacts)`), and `BodyKind` gained (appended)
  `BlackHole`, `Wormhole`, `Asteroid`, `Comet`.
- `GeneratedSpace::bodies_near` adds the black holes and wormhole mouths of the 3 x 3 galactic
  sectors (`interstellar_near`; `BodyRef` index `HOLE_INDEX` 250 / `MOUTH_INDEX` 251, the
  system field carrying their own id and sector) and every system's comets (`COMET_INDEX` 230+).
  Stars carry their heat radius, light and flare period/phase from the generator.
- Belt rocks are looked up per tick round the rig (`GeneratedSpace::rocks_near`,
  `WorldState::flight_bodies_near`: `ROCK_QUERY_TILES` plus a quarter of a tick's travel),
  minus the ones the crew mined out. Rocks and comets never pace the cruise
  (`nearest_surface` skips them): they bump it out instead.

### Stars: heat, light, flares, no landing

- **Radiance** (`radiant_heat`): `RADIANT_HEAT` (1.1/s) x light share (0.5-2, the home sun 1,
  blue stars hottest) x `(heat_radius / r)^2`, out to 6 heat radii. At the heat radius the hull
  heats past burning in a second or two; at twice it the rate is under the hull's cooling, so a
  rig can hang there. The re-entry shield divides it.
- **Flares** (`flare_at`): a pure function of the world clock and the star's period/phase:
  `Quiet { next }`, `Warning` (`FLARE_WARNING_MILLIS` 15 s ahead), `Burst` (6 s, eased in and
  out over 1.2 s). A flare reaches `FLARE_REACH_HEAT_RADII` (3.5) and heats at `FLARE_HEAT`
  (6/s at the heat radius, square law) unless a world, station or rock stands between the star
  and the rig (`shadowed`): shelter behind a body or fly out of reach. White dwarfs never
  flare.
- **Glare** (`star_glare`, presentation): light over the home sun's glares from farther, a
  flare doubles it, blocked by the same shadows; the game washes the view with it (capped at
  0.42 alpha x the Exposure setting, smooth, never flashing).
- Stars can't be landed on (their surface burns 40 hull/s and pushes the rig back, as before).

### Black holes: a real well, time dilation, a fair horizon

- A hole is a flight body with the generator's horizon as its radius and its gravity there
  (30-60 g): `gravity_at` blends it with every other source; it accelerates, never turns. Its
  approach sphere is the lensing radius (the view zooms, a cruise drops out at the rim).
- **Time dilation**: `Space::time_scale_at` (IEEE `f64`, the same bits everywhere) slows the
  rig's own clock: life support drains by it, and `RigFlight::lag_seconds` (per tick,
  replicated) counts how far its clock fell behind the crew's. The HUD shows `TIME x0.42  CLOCK
  -2m 14s`. Host-safe online: it is a pure function of the replicated position.
- **Warnings** (`hole_warning`, alert bits 4-6, each once until the rig climbs back out):
  `Well` inside the lensing, `Strain` when the pull passes `HOLE_STRAIN_SHARE` (55 %) of the
  rig's best thrust, `NoReturn` past `no_return_radius` (pull = thrust). The game draws the
  no-return radius as a dashed red ring. From the strain line, full thrust still climbs out
  (tested).
- **The horizon** is a rescue, never a game over: the beacon's tug pulls the rig home for the
  beacon's fee, the hold's ore and finds lost to the hole (`HoleRescued`).

### Wormholes: flying through

- Crossing into a mouth starts `StarDrive::Transit(TransitRun)` (appended): a closed form of time
  like the warp (`TRANSIT_MILLIS` 3.2 s, `warp_position`), really crossing to
  `transit_exit` (past the far mouth's rim by `TRANSIT_EXIT_TILES` along the heading it came in
  with, leaving at `TRANSIT_EXIT_SPEED`), so it never falls straight back in. Rotation is held
  still through the tunnel; warp and cruise are refused while in it.
- The crew learns the pair (`SystemsShared::wormholes`, `found_wormhole`, `wormhole_from` from
  either end); the nav HUD marks every mouth within two sectors as `WORMHOLE ?` or `WORMHOLE >
  x,y` once flown.

### Nebulae: sensor fog

- The scanner's ping reach is cut by the fog where the rig is (`fog_range_share`: up to 70 %),
  on top of the nebula star's halving. Nav markers (waypoint, crewmates, wormholes) fade with
  `marker_visibility`; the view takes the nebula's tint (Fog setting). `NebulaEntered` /
  `NebulaLeft` are edge events.
- TODO-SEAM(w12-hostiles): `WorldState::nebula_cover(pos)` is the hook for pirates hiding in
  the fog.

### Asteroids and comets: mining from flight

- Belt rocks are solid (`contact`: rest against them, or bounce with a dent past
  `ROCK_SAFE_SPEED`; a cruise or warp drops out).
- **The drill** bites when the rig's underside rests against a rock (`drill_contact`, within
  `DRILL_FACING`): a unit every `contact_mine_seconds(drill tier)` (1.6 s at tier 1).
- **The Mining Beam** (`FlightGearKind::MiningBeam`, appended; `FlightGear::mining_beam`,
  3,400 credits at a station, test mode fits it): the trigger mines the nearest rock within
  `BEAM_RANGE` (14 tiles) of the nose's cone, a unit a second, 1 fuel a second.
- Each rock gives its generator's `yield_units` of its ore (`asteroid_mineral`; TODO-SEAM
  (w12-planets): iron, silver, skystone, ruby, platinum, diamond and uranium stand in until a
  track adds space items) and is mined out for the whole crew (`SystemsShared::mined`, keyed by
  the generator's `AsteroidId`, bounded at `MAX_MINED_RECORDS`); a mined-out rock leaves flight
  and the screen.
- **Comets**: inside a tail (`in_comet_tail`, widening from the nucleus) dust gathers into the
  bay, a unit every 2.5 s, `COMET_HARVEST_CAP` (6) per comet for the crew (`COMET_MINERAL`,
  mythril for now).

### The Wave 11 flight leftovers

- **Free flight lives in the Space store.** A rig crossing the space line (or taking off from a
  body) is moved with `move_to_place(Place::Space, pos)`; in `StoreKey::Space`'s ground pass
  it flies, its store position follows its exact space position every tick
  (`sync_space_store_position`) and the store's floating frame follows it, so netcode poses,
  relevance and streaming are the store's (two flyers near each other share a region). Life
  support runs in that pass (`advance_space_store`). Coming home, docking rescues and the
  beacon move it back with `move_to_place(Place::HOME, ..)`. Rigs from older saves (parked in
  the home store) move into the Space store on their next tick; a rig past the store's ground
  (`within_space_store`, ±2^29 tiles) flies parked in the home store as in Wave 11.
- **Coming home where you choose**: the home world's approach sphere hands the rig to the home
  sky under the point it approached (`home_column_under`: the arc round the round home world
  from the Moon's column, `HOME_RADIUS_TILES` columns a radian), not the column it left from.
- **The Moon**: its approach sphere only zooms the view in; coming down within
  `MOON_LAND_HEIGHT` (60 tiles) of its surface hands the rig to the home sky over the matching
  point of the Moon (`moon_arrival`: the round Moon's upper half across its width, 30 rows over
  its top; from below, under it). A fly-by never snaps home.
- **The parking hold**: an idle rig (no translation thrust) in open space (or a station's
  sphere), slower than `PARK_HOLD_SPEED` and where gravity is under `PARK_HOLD_MAX_GRAVITY`
  (about a quarter g), holds station: its drift dies away and gravity is held off. Its turn is
  never touched; any thrust lets go. The departure point over the Moon is such a place, so an
  idle rig there never drifts home (`ParkingHold`, HUD `PARKING HOLD`).
- **Deterministic maths**: `field::trig::hypot`, `polar::trig::{hypot, ln}` and
  `WorldPos::span` (plain IEEE) replace every platform `hypot` / `ln` in flight, the polar
  crossing and the places' relevance. `WorldPos::distance` stays as the home world always
  measured it.
- **Lap-aware tile keys**: `Terrain::lap_tile` / `same_tile` fold a round body's columns into
  one lap; drill continuation, the drill intent's reuse and the pilot's pick compare tiles
  round the seam, and discovery is stored by lap tile.
- **Body maps**: each body store keeps its players' own map (`BodyWorld::discovered`, swapped
  in with its ground, saved with the body, forgotten with a leaving player);
  `WorldState::store_discovered(store, player)`. The home map is untouched by a body. The game's
  map button on a body opens the **Body Map** (`rendering/body_map.rs`): the strip round the
  rig, wrapping with no edge, the body's top marked, every rig in the store.

### Events, formats and codes

- `GameplayEventKind::SpaceSystems { what: SystemsEvent, player_id, detail }`, netcode 860-878:
  `StarHeat`, `FlareWarning` (seconds), `FlareStruck`, `FlareSheltered`, `HoleWell`,
  `HoleStrain`, `HoleNoReturn`, `HoleRescued` (fee), `WormholeEntered` (1: a new pair),
  `WormholeExited`, `NebulaEntered` (fog), `NebulaLeft`, `OreMined` (ore index),
  `AsteroidDepleted`, `HoldFull`, `CometDust` (units), `RockBump` (damage), `ParkingHold`,
  `MoonApproach`.
- **`PROTOCOL_VERSION` 20** (this track's one bump): `RigFlight::{lag_seconds, mining,
  gathering, marks}`, `StarDrive::Transit`, `FlightGear::mining_beam`, the new body kinds, the
  `space_systems` world field in the appended shared `SectionKey::SpaceSystems` section, the
  events. Saves: no format bump (serde defaults; `BodyWorld`'s saved data gained `discovered`
  with a default).
- Sounds (appended at the end of `patches!`): `systems_flare_warning`, `systems_flare_burst`,
  `systems_hole_warning`, `systems_transit`, `systems_ore_crunch`.

### Game side

- `rendering/space_systems_view.rs`: the world layer (no-return and lensing rings, drill
  sparks, the beam in the ore's colour, comet dust, the wormhole tunnel's rings, the nebula
  tint, glare and flare washes, all scaled by the screen effects setting) and the HUD
  (hole warnings, the flare forecast, `TIME x / CLOCK -`, `SENSORS FOGGED`, the drilling /
  beam and comet gauges with units left, `PARKING HOLD`), wormhole edge markers and faded nav
  markers in fog.
- Belt scenery is gravel only now; the boulders are the real rocks (`LiveSpace::refresh_rocks`
  each frame, drawn by `draw_rock_shaped` within a few per cent of their contact radius, with
  their ore's glint).
- `local_game` voices the systems events like flight's; touch needs nothing new (the beam is
  the fire button, the drill is contact).
- Scenes (`scene_capture/systems_scenes.rs`): `systems_star_flare`, `systems_blue_glare`,
  `systems_black_hole_dilation`, `systems_wormhole_transit`, `systems_nebula_fog`,
  `systems_mining_asteroid`, `systems_mining_beam`, `systems_comet_tail`,
  `systems_moon_approach`, `systems_moon_arrival`, `systems_parking_hold`.

### Tests

- `flight/systems_tests.rs`: flare cycle (telegraphed, eased, periodic, never for a white
  dwarf), radiance (burns inside, hangs at twice, class light), shelter, glare, hole warnings
  and the strain line's escape margin, horizon and mouth crossings stop the step, transit exit,
  fog, drill facing and the beam's cone, rocks solid with dents, comet tails, the parking hold
  (holds, never turns, lets go on thrust, never near a world), the Moon's sphere vs handover,
  the crew's wormhole record.
- `world/space_systems_tests.rs` (V11 world, real generated things near home): a black hole's
  well (dilation, lag, slower air, pulled not turned, escape from the strain line, the
  no-return warning and the tug rescue with the hold lost), a wormhole transit to its pair
  (route learned, out past the far rim, not back in), nebula fog on the ping, mining a belt rock
  by drill then beam until it crumbles (cargo, record, save and section round trip), comet
  dust, a star's radiance and flare with the warning first, the Moon fly-by and landing, coming
  home under the point approached, the parking hold at 30 and 60 Hz, and determinism (twice at
  30 Hz bit for bit; 60 Hz agrees).
- `world/body_map_tests.rs`: a body's own map, round the lap, saved, the home map untouched.
- `star_flight_tests::flying_back_into_the_home_world_hands_the_rig_to_its_sky` now expects the
  column under the point approached.

### Seams and still open

- TODO-SEAM(w12-hostiles): `nebula_cover` for pirates in nebulae.
- TODO-SEAM(w12-planets / stations): the mined ores and comet dust map to stand-in minerals in
  `asteroid_mineral` / `COMET_MINERAL`; the Mining Beam is a `FlightGearKind` the stations
  track's shops can list.
- Body maps travel in keyframes with the body (every player's); the op log's discovery stays
  the home world's, so a client's body map is its own prediction until the next keyframe.
- Flares come every 5-120 minutes (the generator's periods); a crew rarely meets one by chance,
  so the HUD forecasts the next within two minutes.
- Economy (for the integrator): until space items exist, iridium mines out as diamond and
  voidglass as uranium, and a rock yields `radius² / 8 + 4` units (a 30-tile boulder about
  110), so one lucky rare rock near home can fill a bay with deep-tier home ore in a quarter of
  an hour. Map them to space items (or cap the rare ores' yields) when the seam is wired.
- Body maps travel to every client in the body's keyframe (each player's map, not redacted
  per player like the home map).

### Review notes (Wave 12 systems review)

- Real belt rocks are drawn round to their contact radius with more sides as they grow, a cast
  of their ore in the stone, and ore flecks on a rock-anchored grid over the screen cells they
  cover (`draw_ore_veins`): a boulder filling the view used to read as a flat ten-sided polygon
  in a random tone (an ice rock drew rust-brown) with three tiny glints.
- The systems readouts (`plate_x`) stay inside a split-screen seat's half view (they were cut
  off at its right edge).
- The mined-rock record drops its lowest-keyed rock past `MAX_MINED_RECORDS`, not the oldest
  (the record keeps no ages; the docs said "oldest").

## Wave 12 bodylife: living bodies (`src/bodylife.rs`, `src/bodylife/{clock,weather,climate,market}.rs`, `src/planet/hooks.rs`, `src/world/bodylife.rs`)

Expansion plan §16 WP-SPACE ("each a world like the home ground level: ... weather, day
length, gravity strength"). Every landable body gets a life of its own: its clock and day,
weather, climate and air, gravity and economy, and the planet theme framework that the theme
tracks build their worlds on.

**Nothing new is stored, saved or replicated.** A body's life (`BodyLife`) is a pure function
of the world seed, the space body (`space::SpaceBody`: day length, mean temperature,
atmosphere, gravity, ground seed) and world time, memoized per process
(`world::body_life_of`). Every peer reads the same hour, weather and prices from the same tick
at every tick rate. Bodies run on **world time by the tick** (`WorldState::body_millis`,
`millis_at(tick)`): a night slept away at home jumps the home clock only. Scenes and tests set
it with `set_body_millis` (doc-hidden). A lone planet world (`new_planet_world`) gets a life
too (`BodyLife::of_planet`: the theme's typical climate, a 20-minute day).

### The clock (`bodylife::clock::BodyClock`)

- `BodyClock::new(day_seconds, columns, seed)`: the space body's day (480-2,400 s; `0` is
  **tidally locked**), its columns and a seeded phase.
- `local_permille(millis, column)`: local time, thousandths of a day from local midnight (500
  is noon). The hour advances with the column, so noon sweeps round the body once a day and a
  quarter lap is a quarter day; columns wrap. Two seats on two bodies (or at two columns of
  one) read their own hour.
- **Tidally locked**: column 0 has its star overhead forever, half a lap round is eternal
  midnight, the terminators lie a quarter lap either way; the hour never moves.
- `slice`, `phase` (the home clock's `DayPhase`), `hours_minutes`, `day`, `local_day`
  (markets turn over at local midnight), `elevation` (the sun's sine, deterministic trig),
  `sun_angle` (from the local up, clockwise), `turn` (column 0's continuous local time, what
  the renderer lights by), `daylight(millis, column, night)` (per mille, a smooth twilight
  between `NIGHT_ELEVATION` and `DAY_ELEVATION`), `millis_when(after, column, permille)` and
  `column_at(millis, permille)` (scenes, tests).

### Weather (`bodylife::weather`)

- **The registry** `WEATHER_REGISTRY: &[&[WeatherKind]]` is a list of per-track lists; every
  kind carries its own `WeatherId` from its track's range (bodylife `0..=63`, planets-a
  `64..=127`, planets-b `128..=191`, aliens `192..=223`, stations `224..=239`, hostiles
  `240..=255`, later waves 256+). A theme track defines `pub const MY_WEATHER: &[WeatherKind]`
  and appends `MY_WEATHER` to the registry; `weather_kind(id)` (const) finds it, and unknown
  ids read as clear.
- A `WeatherKind` is data: `needs_air`, `night_only`, sustained `wind` and `gust` (px/s),
  `visibility_permille`, `light_permille`, `temperature_c`, `hull_damage_milli` and
  `suit_damage_milli` (a second, under the open sky), `fouls_air`, its `look`
  (`WeatherParticles::{None, Rain, Snow, Dust, Ash, Spores, Aurora}`, append only: the game
  matches every variant; colour, fog colour, sky glow) and its ambience `sound` name.
- **This track's generic kinds**: `CLEAR` (0), `STORM` (1: rain and gales, gloom and fog, and
  a little hail that stings a rig or pilot left in the open), `SNOW` (2: drifting snow,
  -28 °C, white fog), `AURORA` (3: night-only curtains that light the dark). Sandstorms, ash
  fall, acid rain, spore drift and the rest are the theme tracks' to register.
- **The forecast** `forecast(seed, table, atmosphere, millis, night)`: time is cut into
  `SPELL_MILLIS` (150 s) spells. Each spell's kind is drawn from the theme's weather table by
  a hash of the body's seed and the spell (an airless body gets only kinds that need no air).
  A kind fades in and out over `RAMP_MILLIS` (20 s) where the spell before or after differs.
  The wind blows one way a spell and gusts as an integer triangle wave (`GUST_MILLIS`); thin
  air pushes less, thick air more (`air_push_permille`). `WeatherNow` carries the kind,
  intensity, wind, visibility, light, temperature change, hull and suit damage and fouled
  air. `next_spell_of` finds the next spell of a kind (scenes, tests, an outlook).

### Climate and air (`bodylife::climate::BodyClimate`)

- **Temperature**: the body's mean (the space generator's: its star at its distance plus
  theme and air), a day-night swing by its air (`day_swing_c`: bare rock ±85 °C, a thick sky
  ±5 °C; a locked body's sides swing half again as far) and the weather's change. Underground
  the swing and the weather fade within `SKIN_ROWS` (24) while the inner heat rises
  quadratically to `core_c()` at the core chamber (bigger, heavier bodies keep more; themes
  add `ThemeClimate::core_bias_c`).
- **Heat and cold replace the home bands on a body**: every `HEAT_STEP_C` (45 °C) over
  `HEAT_START_C` (70 °C) is a heat level (`HEAT_UNITS_PER_LEVEL` units; the radiator and heat
  shielding cover them as at home), every `COLD_STEP_C` (22 °C) under `COLD_START_C` (-40 °C)
  a cold level (the de-icer covers them). The tick's hazard pass reads `body_heat_units_at`
  and `body_cold_at` for rigs, and the pilot's suit reads the same (`pilot_gear`); the classic
  `crust::heat_danger_units` and the sky's cold apply only off a body.
- **Pressure** (kPa: `surface_pressure_kpa` by air, scaled by gravity, rising underground) is a
  reading for the HUD.
- **Air**: breathable only under a breathable atmosphere and not while the weather fouls it.
  A pilot on foot breathes there (`pilot_air_at`; else `Thin`: the suit's oxygen, the rig's
  air line), and the rig's life support refills only there (`advance_body_flight`, per rig,
  where it stands).

### Weather in play (`world/bodylife.rs`)

- Under the open sky (`open_sky_at`: nothing solid up to the strip's top, within
  `SHELTER_DEPTH_ROWS` of the surface; caves, shafts and overhangs shelter):
  - an **airborne rig** moves relative to the wind's air (`movement.rs`: the field path steps
    the velocity relative to the air, the classic path coasts with it like the Inner Sky's
    wind). It is tick-rate safe (`the_wind_drifts_the_same_at_every_tick_rate`);
  - an **airborne pilot** is nudged at `PILOT_WIND_SHARE` where its box fits, and the suit
    takes the weather's suit damage (`pilot_weather`);
  - a **rig** takes the weather's hull damage through its mitigation (the hazard pass);
  - a **pilot** under the open sky also takes its theme's corrosive air
    (`ThemeHazards::corrosive_air_milli`, suit damage a second; 0 on the placeholders).
- `body_visibility_at(x, y)`: the share of a clear day's sight (TODO-SEAM w12-aliens: scale
  creatures' line-of-sight noticing by it).
- **Events** (netcode 880-899): `GameplayEventKind::BodyLife { what: BodyLifeEvent, player_id,
  detail }`: `WeatherTurned` (detail: the new weather id), `Dawn` and `Dusk` at the player's
  column (edges between the tick's time and the next's, so every tick rate hears each once),
  and `Sold` (detail: credits). They run in each body store's ground pass and in a lone
  planet world.

### Gravity (one number per body)

`BodyLife::gravity_milli` (`effective_gravity_milli`: the space body's gravity within the
ground's playable 0.25-2.5 g) is the ground's field-zone strength, flight's approach gravity
(`flight::GeneratedSpace` now clamps a body's pull the same way, so a light moon pulls the
same in its sky as on its ground) and the HUD's number (`body_gravity_of`). Rigs, pilots,
drops, charges and creatures feel it through the planet's gravity zone (`BodyParams::at_fields`,
the pilot's `fields_at`). Fluids do not: the cellular fluid model flows a fixed amount a tick
whatever the gravity (review note; a theme track that wants slow or fast liquids scales it). Test:
`gravity_scales_with_each_body_and_shows_one_number` (a rig falls about four times as far on
a 1.2 g body as on a 0.3 g one).

### Economy (`bodylife::market`)

- Credits are universal. `BodyPrices::new(seed, common, sectors)`: ore the body's ground is
  rich in (`planet::hooks::common_minerals`: every stratum's ores and the core ore) sells at
  `COMMON_PERMILLE` (72 %), the rest at `SCARCE_PERMILLE` (125 %) plus
  `DISTANCE_PERMILLE_PER_SECTOR` (4 %) a galactic sector from home (at most +60 %), each with
  ±8 % seeded jitter; artifacts sell at 120 %.
- `LocalMarket::new(prices, settlement, day)`: a settlement's (or a station's) own jitter, two
  **wanted** goods each local day (+60 %) and a **glut** (-40 %, never a wanted good).
- `WorldState::body_market(player, settlement, column)` (the market at a settlement's column
  now) and `WorldState::sell_cargo_at_market(player, &market, output)` (pays minerals and
  artifacts at the market's prices, with no home premiums or rebates, and raises `Sold`).
  TODO-SEAM w12-aliens / w12-stations: settlement and station counters call them through a
  command of their own. The home depot's `SellCargo` stays refused away from home.

### The planet theme framework (`planet::hooks`)

A theme is its data (`PlanetTheme` in `PLANET_THEMES`, unchanged) plus its hooks:

```rust
pub trait PlanetThemeHooks: Sync {
    fn id(&self) -> PlanetThemeId;
    fn data(&self) -> &'static PlanetTheme;                            // default: theme(id)
    fn surface_rows(&self, ground: &PlanetGround, column: i64) -> i32; // the surface shaper
    fn strata(&self) -> &'static [PlanetStratum];
    fn ore_at(&self, stratum: &PlanetStratum, roll: u16) -> Option<TileKind>; // the ore table
    fn feature_tile(&self, ground, column, row, depth) -> Option<TileKind>;   // landmarks, secrets
    fn secret(&self, ground: &PlanetGround) -> Option<SecretSite>;
    fn creature_spawns(&self) -> &'static [CreatureSpawn];
    fn weather_table(&self) -> &'static [WeatherOdds];
    fn settlement_sites(&self, ground: &PlanetGround, count: usize) -> Vec<i32>;
    fn climate(&self) -> ThemeClimate;
    fn hazards(&self) -> ThemeHazards;
    fn music_bed(&self) -> MusicBed;
}
```

- Every hook has a default that reads the data, so a theme registered as data alone plays
  (`DataTheme`, the generic hooks). A theme track implements the trait on its own type and
  appends `&MyTheme` to `THEME_HOOKS`; `hooks(id)` returns its hooks (or the generic ones).
- `PlanetGround` generates **through the hooks** for every tile: the shaper (clamped to the
  hill limit), then `feature_tile` before the strata (landmarks, ruins, surface growth, the
  secret; above or below the surface), then the strata and the ore table. Generation stays a
  pure function of the params, and the per-ring-tile rule is unchanged.
- **Ported placeholders**: `BarrenTheme` (clear skies, the odd aurora; music
  `bodylife_barren`), `RockyTheme` (storms, aurora; a +40 °C core), `IceTheme` (snow, storms,
  bright auroras; a -60 °C core). Their ground is the Wave 11 ground (plus the secret on
  small bodies).
- **One secret each**: every body up to `SMALL_BODY_RADIUS` (1,000: craters and small moons)
  keeps one `SecretSite` (`default_secret`). It is a 9 x 5 vault of hard rock
  `SECRET_COVER_ROWS` under the crust at a seeded column inside one lap, holding an artifact
  (void pearl, machine sigil, crystal prism or radiant shell) on its floor between seams of
  the theme's core ore. `PlanetGround::secret()` names it (for a scanner hint or a story).
- `flattest_sites(ground, count)` is the default settlement finder (the flattest 24-column
  stretch of each share of the lap). `CreatureSpawn` (creature id, weight, depth band,
  night-only) is data the fauna and aliens tracks spawn (TODO-SEAM w12-aliens). `MusicBed`
  (name, tempo, brightness) names a theme's bed (TODO-SEAM: the theme tracks add the patches;
  today the game plays a body's ambience by its weather).

### Game side

- `render_snapshot::BodyLifeView` (in `PlaceBody::life`): the body's name, the reading where
  the seat's player is (hour, light, elevation, weather, climate, exposure), gravity, air,
  whether it is locked, and its night light.
- **Each body lit by its own clock**: `LiveSpace::turn_bodies(millis)` gives every gathered
  generated body its clock's sun (`BodyView::sun`, `live::body_sun`: noon at column 0 straight
  up, sweeping round with the day). Its disc and its round ground are lit from there
  (`light_toward_body`), so a body's approach and its ground agree and every column shows its
  own hour. The polar shader's sky goes dark at night and warms toward the terminator.
- **Weather over the view** (`rendering/body_life.rs`, `draw_weather`): rain streaks along the
  wind, drifting snow, grit, ash and spores, the storm's gloom, a fog thicker at the edges,
  and the aurora's curtains (additive, swaying slowly). It is drawn in the body's local frame
  turned by the craft's roll (the camera stays locked to the craft), fixed in the world as
  the craft moves, and scaled by the screen-effects setting (fewer particles at Reduced, none
  at Off; fog and gloom stay, softer). Nothing flashes. Plain raylib shapes (browser-safe).
- **The body badge** replaces the clock badge on a body: the name, the local hour (DAY SIDE,
  TWILIGHT or NIGHT SIDE when locked), temperature, gravity and pressure (kPa; none shown in
  vacuum), air, and the weather and wind.
- **Audio**: `away_ambience` hears the body: the wind rises with a storm's gusts, rain hisses
  (`RainLoop`), snow and grit whistle (`HighWindLoop`), an aurora shimmers softly
  (`SkyAirLoop`), and an airless ground is near silent. Event cues: the weather turning
  (thunder, a gust, a chime), dawn, dusk and a sale (`body_life_cue`). No new patches.
- Fixes found on the way: a seat on a body no longer gets the home world's village banners
  (its strip coordinates are not the home world's), and nebula clouds and the galaxy's band
  are faint from a body's ground (they painted the whole night sky at ground zoom).
- Scenes: `bodylife_terminator`, `bodylife_night`, `bodylife_locked_day`,
  `bodylife_locked_night`, `bodylife_locked_terminator`, `bodylife_storm`, `bodylife_snow`,
  `bodylife_aurora` (real bodies of the scene world's space).

### Formats and codes

- `PROTOCOL_VERSION` **20** (this track's one bump: the body-life events 880-883).
- No save format bump, no new world sections, no `TileKind` codes, no sound patches.
- Generation: home untouched (V1-V10 and `GOLDEN_V11_DIGEST` unchanged); space generation
  untouched (`GOLDEN_SPACE_DIGEST` unchanged). Planet ground changes only by the secret vault
  on bodies up to radius 1,000.

### Tests

- `bodylife::tests`: each body keeps its own clock (days, phases, quarter laps, wrap, noon,
  midnight, the sun's angle, `millis_when`); tidal locking (eternal day and night sides,
  terminators, the wider swing); weather determinism (pure, every kind comes up, seeds differ,
  nothing needing air on an airless body, aurora only in the dark, the registry); fading and
  effects (no jumps, a full-strength storm, thin against thick air); the temperature, air and
  pressure model; per-body prices and local markets; the theme registry round trip; the
  secret vault; settlement sites; one gravity per body.
- `world::bodylife_tests`: a storm carries an airborne rig and a cave stops it (and the storm
  hides the view); the drift agrees at 30, 60 and 120 Hz; dawn is heard at the rig's column
  and the night is dark and cold; the climate replaces the home bands (deep heat, the airless
  night's cold bites a stock rig, no damage at noon); a pilot breathes only a breathable sky;
  gravity scales a fall; two seats on two real bodies read their own clocks and air, and life
  support refills only where it is breathable; flight's pull matches the ground's; a body's
  market pays its own prices; weather turns replay bit for bit.
- Game: `split_screen_seats_in_two_places_each_see_their_own` checks the seat's body life;
  the scenes build headlessly.

### Seams for the integrator and the other Wave 12 tracks

- **Theme tracks (planets-a/b)**: implement `PlanetThemeHooks` for each theme and append it to
  `THEME_HOOKS`; register weather kinds in their id range (`WEATHER_REGISTRY`), point
  `weather_table` at them, and refine `place::planet_theme_of`. Use `feature_tile` for
  landmarks and settlement buildings, and `settlement_sites` for where they stand.
- **Aliens and stations**: open counters on `body_market` / `sell_cargo_at_market`; spawn
  `creature_spawns`; scale noticing by `body_visibility_at`; night-only spawns can read
  `BodyLife::clock.daylight`.
- **Hostiles**: storms and fog are cover (`body_visibility_at`).
- Known limits: a body's strip does not turn in space (flight's handover is static), so the
  renderer lights a turning body from its clock's sun rather than the star's bearing (the lit
  face of a turning world follows its clock); weather is one per body (no fronts by longitude,
  the aurora aside); the theme music beds are names only. When more stores are occupied
  than `MAX_AWAKE_STORES_PER_TICK` the round-robin skips a store's tick, and a dawn, dusk or
  weather-turn edge that falls in a skipped tick is not heard (the state itself, a pure
  function of time, is never wrong).

## Wave 12 planets-a: the wild worlds: jungle, ocean archipelago and glacier (`src/planet/wilds/`, `src/planet/world.rs`, `src/world/wilds.rs`, `src/world/enemies/wilds.rs`)

Expansion plan §16 WP-SPACE "planet themes": three planet themes, each a world like the home
ground level. The space generator's `Theme::Jungle`, `Theme::OceanArchipelago` and
`Theme::Ice` land on them (`place::planet_theme_of`); the Wave 11 `ICE` placeholder stays
registered for bodies saved with it.

### Registration (`planet::theme`, `planet::world`)

- **Theme registry** (append only): `PlanetThemeId::JUNGLE` (3), `OCEAN` (4), `GLACIER` (5),
  each a `PlanetTheme` (palette, crust, three strata with rock, accent and ores, core ore) in
  `wilds::{jungle, ocean, glacier}`.
- **Theme worlds** (`planet::world`, new, shared with planets-b and Wave 13): a
  `ThemeWorld` gives a theme its own `surface_row`, `chunk_tiles` (features over the generic
  strata, ores, caves and core chamber), `core_floor_row`, `extra_sky_rows`, and the
  `settlement_site` / `landmark_site` hooks. `world_of(theme)` is the registry (append match
  arms). `PlanetGround` dispatches to it (`surface_row_of`, `chunk_tiles`, `new`), so every
  other theme generates bit for bit as before.
- **Depth** (`wilds::strata_rows`, `core_floor_row`): a planet's strata run 800 rows at
  radius 1,500 to 2,500 at 8,000 and up (1,715 at 5,000), ending on the core chamber, which
  sits on the indestructible heart of the body (the rows below it are bedrock); a small body
  (radius up to 1,000) is strata all the way to its true core. (The space body's own
  `strata_rows` is not in `PlanetParams`; TODO-SEAM w12-systems if it should drive this.)
- **Tiles**: `TileKind::Wild(WildTile)`, codes `0x2000 | code` (35 tiles, append only), with
  hardness, durability, item, `is_slick`, `is_treasure` and `glow`.

### The three worlds

| Theme | Surface | Strata and signature rules | Landmark |
|---|---|---|---|
| Jungle | the generic hills under giant trees (one in most 32-column cells: trunk, canopy, vines, a taproot) | **Root Tangle**: living roots snag a rig rubbing against them (`ROOT_GRIP_PER_SECOND`); **Spore Caves**: extra caves floored with puffcaps that burst into spore clouds when drilled (toxic air); **Heartwood Deep**: hard heartwood, amber with fossils, lifesap nodes that mend 12 % of the hull when drilled | the Elder Tree: a 12-wide trunk 58 rows tall on a levelled rise, a crown, giant roots, a hollow with a door and the Elder Tree's seed |
| Ocean archipelago | one elevation field: islands over a planet-wide sea (fluid water from row 0 to the seabed), reefs, kelp and pearl shells in the shallows | **Reef Shelf**: waterlogged (sea-water pockets flood a shaft); **Drowned Ruins**: every cave flooded, sealed halls of air (ruin stone, glowing glyphs, a relic) in flooded galleries; **The Trench**: black smokers (heat) with rich ore round them, flooded caves, pressure | the Drowned Spire: a ruined tower out in the open sea, flooded below its floor, a dry room with windows, a hole in its roof and a drowned relic |
| Glacier | terraced ice sheets (5-row steps) under snow, crevasses (a third bridged by rime), ice spires, frozen seas (a sea-ice lid over cold water) | **Glacier Ice**: slick ice (Descent IV's ice physics), tunnels refreeze as rime at night and thaw by day, rime breaks under a resting rig; **Under-Ice Sea**: lenses of water under sea ice, cold water bites three times as hard; **Permafrost Caves**: ice caves lined with blue ice, frost crystals, icicles that crack and fall on a rig passing under them | the Frozen Titan: a beast frozen in clear ice under the glacier (skull, spine, ribs), its tusk breaking the surface, its heart in its ribs |

- **Settlement site** (`ThemeWorld::settlement_site`, for the aliens track): 40 columns of
  level, dry, clear ground at a seeded column (an island on the ocean, never a crevasse or a
  tree), eased into the hills over 12 columns. Creatures leave it alone
  (`WorldState::wilds_safe_zone`); elsewhere a wild world's surface is the creatures' too
  (the home world's "the surface is safe" rule does not hold there).
- **Landmark**: half a lap from the settlement. **Secret** (small bodies only, radius up to
  1,000): a cairn on the surface a quarter lap from the settlement, a chamber 18 rows under it
  holding a buried cache (`WildTile::Cache`). TODO-SEAM w12-systems: the space generator only
  makes ice small bodies of these themes; jungle and ocean moons get the secret once it tags
  them.

### Play (`world/wilds.rs`, `planet::wilds::life`)

- **Hazards** (`PlanetHazard`, tagged for planet gear): jungle toxic air (spore-laden air in
  the Spore Caves fouls the hull), ocean heat (within 2 tiles of a smoker) and pressure (in
  the Ruins' and Trench's water), glacier cold everywhere (fuel for the heaters; x2 in a
  blizzard on the open surface, x3 in water), high gravity on bodies of 1.4 g and up.
  Counters: the Descent's rig systems (`descent_counter`: Cryo Hull, Bio-Filter, Pressure
  Hull; the Thermal Hull part takes 75 % of the heat) and `WorldState::planet_gear_counters`
  (TODO-SEAM w12-stations: the planet gear line answers there). Only rigs feel them.
- **Submarine mode and slick ice** are Descent IV's movement hooks, switched on for wild
  worlds (`WorldState::wild_water_rules`); `drowned::is_ice` knows the slick wild tiles.
- **Vines** are open air a pilot on foot climbs like a ladder (`WorldState::wild_vine_at`,
  `jungle::vine_at`).
- **Weather and day** (`life::weather_at`, `is_night`, `day_seconds`): pure functions of the
  body and world time; one 150 s cycle in three is a storm (downpour, squall or blizzard by
  theme) that ramps up and down; squalls and blizzards push craft over the open surface; the
  storm's rise is an event. TODO-SEAM w12-bodylife: the per-body weather and clock replace
  these schedules (the theme's `WildTheme::weather` is what it should pick; the day should be
  the space body's `day_seconds`).
- **Events** (netcode 900-919): `GameplayEventKind::Wilds { what: WildsEvent, .. }`:
  roots snag, spore burst, lifesap mended, scalded, pressure creak, cold bite, icicle crack,
  refroze, landmark relic, secret cache, storm rising, rime broke.

### Creatures (`enemy.rs`, `world/enemies/wilds.rs`)

Twelve appended `EnemyKind`s, four per theme, all following the enemy body rules in the
planet's local frame (spawned through `site_fits`, contact boxes fitted to their art):

| Theme | Neutral | Hostile |
|---|---|---|
| Jungle | Trundlehorn (megafauna, herds bed down at night, charges only when struck) | Vinestalker (hangs under the canopy, sees only below, drops), Mossprowler (lies flat until close, pounces), Sporewing (Spore Caves swarm) |
| Ocean | Shellstrider (great turtle), Glimmerfin (school, scatters) | Sawjaw (circles while its dash recharges), Tidesnapper (beach crab) |
| Glacier | Woolhorn (megafauna), Rimeskipper (hops away) | Rimewolf (pack circles and leaps together; farther sight in a blizzard), Crevassewyrm (burrows through ice and permafrost only) |

Spawning (`advance_wild_creatures`, every 2 s): up to 6 round each rig within 44 tiles,
16-34 tiles out (out of view), from the theme's roster by habitat (surface, hanging, water,
the caves of a stratum); Peaceful spawns only the neutral fauna; creatures 80 tiles from every
rig leave. Swimmers never leave the water (`wilds_after_move`).

### Game side

- Tile art (`rendering/terrain/tile_art/wilds_art.rs`): every wild tile, the strata's back
  walls, vines and kelp in the open air, the sea painted into its tiles (the polar renderer
  has no live fluid pass), snow on the glacier's tops, ore set in the stratum's own rock.
- Creature sprites (`rendering/pixel_art/sprites/wilds.rs`; the megafauna at 1.5x), drawn on
  round bodies too: the polar renderer's ground things gained `GroundThingKind::Enemy`.
- Music (`audio/patch/wilds.rs`, appended at the end of `patches!`): `loop_planet_jungle`
  (marimba, log drums, frogs, a far trumpeting call), `loop_planet_ocean` (breakers, steel
  drum, gulls), `loop_planet_glacier` (cold fifths, wind, glass bells, the glacier's groan)
  and `loop_planet_storm`; the director plays the body's bed under its sky (thinning with
  depth) and the storm on the open surface (`wild_world_ambience`).
- Cues and messages for every `WildsEvent`.
- Scenes (`scene_capture/wilds_scenes.rs`, on the nearest planet of each theme to home on the
  flight scenes' seed): `wild_<theme>_approach`, `_surface`, `_landmark`, two strata each
  (`wild_jungle_spore_caves`, `_heartwood`, `wild_ocean_ruins`, `_trench`,
  `wild_glacier_sea`, `_permafrost`) and creatures (`wild_jungle_creatures`; the ocean and
  glacier surfaces have theirs).
- `--space-report` lists the ground theme and strata every tag lands on.

### Formats and digests

- `PROTOCOL_VERSION` 20 (wild tiles, the appended creatures, events 900-919). No save format
  bump (tile codes and enum variants are additive; no new world sections: everything lives in
  the terrain and entities).
- V1-V10, `GOLDEN_V11_DIGEST` and `GOLDEN_SPACE_DIGEST` unchanged (space generation is
  untouched; only the tag-to-ground map moved). The wild worlds are pinned by
  `GOLDEN_WILDS_DIGESTS` (`planet/wilds/tests.rs`).

### Tests

- `planet::wilds::tests`: registration and purity, golden digests, strata depth (small and
  big bodies, every stratum on the way down, the core chamber), each theme's features (trees,
  vines, puffcaps, lifesap, the Elder Tree; sea, islands, ruins, smokers, the spire; frozen
  seas, crevasses, icicles, the titan), level clear settlement sites, small-body secrets,
  seamless wrap, hazard tags, weather and day purity, the space tags, `--space-report`.
- `world::wilds_tests`: a digging bot reaches every stratum and the core chamber of a small
  (R 640) and a big (R 5,000) world of each theme; deterministic and tick-rate safe; neutral
  fauna never attack first and fight back; vinestalkers see only below; swimmers stay in the
  sea; spawns come from the theme's roster (Peaceful: neutral only); the settlement is safe;
  puffcaps, lifesap, roots, cold (and the Cryo Hull), icicles, refreeze and rime, smoker heat,
  the frozen sea and the storm's push.

### Seams and known limits

- TODO-SEAM w12-bodylife: weather and day (above). TODO-SEAM w12-stations: planet gear
  (`planet_gear_counters`). TODO-SEAM w12-aliens: the settlement site is level ground; the
  aliens track builds the town on `ThemeWorld::settlement_site` (and may want hides and shells
  from the fauna's loot). TODO-SEAM w12-systems: jungle and ocean small moons, the space
  body's `strata_rows`.
- Planets-b registers its themes in the same `world_of` match and theme array (append).
- Tile-keyed state on a body is still not lap-aware (Wave 11's limit); the refreeze sweep
  only touches tiles round the rigs.
- Big features near a ring tile boundary deep down (halls, smokers) can be clipped at their
  hashed cell's edge; a ring tile of an inner band is still generated whole.
- **Old saves** (integrator): a body's ground params are recomputed from the space body on
  load (`place::planet_params_for`), so a Wave 11 save that dug into an ice-, jungle- or
  ocean-tagged body (ground `ICE` or `BARREN` then) reloads with that body's saved tile edits
  over the new glacier, jungle or ocean ground (and its new strata depth). Nothing crashes
  and rigs are freed from rock, but the old tunnels land in new ground. If that matters for
  released saves, bump the save format once for the wave and drop those bodies' edits.
- **Ocean fluid cost**: a shaft drilled from the seabed (or into the Reef Shelf's pockets
  and the Ruins' flooded caves) drains the planet-wide sea through the Wave 7 fluid engine
  until the shaft is full. Measured with `wild_world_tour` (release, desktop, 30 Hz): about
  1,000-1,500 woken cells for minutes, 1.3-4.7 ms a tick on average with ticks of 40-140 ms
  while it drains; every other theme stays under 0.15 ms a tick on average. The step budget
  (`FLUID_STEP_BUDGET`) bounds it, but the browser should be timed on an ocean dig; a cheaper
  sea (a level-held sea surface, or a sleeping far sea) is engine work for a later wave.

### Review fixes (Wave 12 planets-a review)

- A storm's rise is told to each rig near the open surface (`WildsEvent::StormRising` now
  carries the player, never `None`), an icicle's crack to the rig under it; nothing of a
  body reaches a seat in another place (`a_real_glacier_body_plays_its_rules_for_its_rig_
  only_while_the_crew_is_split`, a real ice-tagged body of the galaxy in its own store).
- The glacier's cold (everywhere) reminds every 20 s instead of every 3 s.
- An idle vinestalker hangs still (dormant) instead of wandering the sky.
- The game tells a seat each wild stratum's rule the first time its rig enters it
  (`LocalGame::announce_wild_strata`); wild water is painted with ripples, not star dots;
  the Frozen Titan's tusk is one two-tile-thick curve.
- **Tiers**: the wild worlds had the home topsoil's enemy tier (band 0 at the surface), so
  their creatures bit for 9-16 against a post-Heart rig's ~2,500 hull and the smoker, the
  Trench's pressure and the spore air barely scratched it. `WorldState::wild_tier` now gives
  a wild world tier 8 at the surface and one more per stratum (up to 11, the deep Descents'
  top): creatures' health and bite scale by it (`tier_at`), and the hazards' hull damage is
  scaled by `tier_scale` like every Descent hazard. TODO-SEAM w12-systems/w12-hostiles: a
  body's danger by distance from home may move `WILD_BASE_TIER`.
- The sea's neutral swimmers (shellstriders, glimmerfins) keep to the open sea; only the
  sawjaw follows a rig down into flooded caves and shafts.
- `wild_world_tour` (ignored): a ten-minute bot tour of each theme that prints the strata,
  encounters, events and tick cost.

## Replication

Every field belongs to exactly one section kind (the compiler enforces it through exhaustive
destructuring in `section`, `remove_player`, `redact_player` and `split_player`):

| Kind | Contents | Who receives it |
|---|---|---|
| Per tick | rig pose, velocity, fuel, hull, weapon state (capacitor, cooldown, aim, trigger), drill/scanner timers, deep-run pressure; the charge serial, entity id/revision counters and the local pixel frame (`LiveEntities`, applied first, see "Wave 7 positions") | clients the rig is relevant to |
| Per tick, delta | entities (gas clouds, boulders, drops, charges, projectiles, targets, ...): records of entities new to the client or changed since its last frame (by revision), and despawns (gone or out of relevance) | clients the entity is relevant to |
| Op log | terrain tile writes | everyone, filtered by 16x16 terrain chunk relevance (stale chunks are refreshed with `terrain_chunk`: the chunk's diffs from generation) |
| Op log | per-player discovery | the owner only |
| On change | `RigBuild(player)`: upgrade levels drawn on the rig (drill, engine, cargo bay, hull, radiator, scanner, bulkheads, drill mount), the rig's cosmetics and its mounted weapon | everyone |
| On change | `Loadout(player)`: cargo bay and block hopper, credits, upgrades, weapon rack, kits, rig parts, POI journal, quest log (solo quests, story arcs, reputation, blueprint fragments), salvage, rescue count, deep reward milestone, service transactions, cosmetic unlocks | the owner only |
| On change | economy, contracts, progression (incl. `won_game`), town, world events, infrastructure, markers, RNG, timers (incl. twin-relay vault deadlines), session constants (tick timing, player template, world settings), crew quests, the enemy director (difficulty, rig i-frames, spawn cooldowns, cleared lairs) | everyone |
| On change | `RigBuild(player)`: upgrade levels drawn on the rig (drill, engine, cargo bay, hull, radiator, scanner, bulkheads, drill mount) and the rig's cosmetics | everyone |
| On change | `Loadout(player)`: cargo bay and block hopper, credits, upgrades, kits, rig parts, POI journal, quest log (solo quests, story arcs, reputation, blueprint fragments), salvage, rescue count, deep reward milestone, service transactions, cosmetic unlocks | the owner only |
| On change | economy, contracts, progression (incl. `won_game`), town, world events, infrastructure, markers, RNG, timers (incl. twin-relay vault deadlines), session constants (tick timing, player template, world settings), crew quests, bases (claims, modules, fixtures, homes), the world clock (`SectionKey::Clock`: day length, skipped nights, sleep rule, sleepers), people (character memories, conversations, perks, story marks), the Fathers' expedition (`SectionKey::Expedition`: logs read, sealed canisters, echoes, anomaly rooms; Wave 8) | everyone |

Because other players' private state never reaches a client, replica checks compare
`visible_state_hash(viewer)`: private state is hashed only for the viewer and per-player data is
hashed player by player, so host and a correct client authority agree exactly. Terrain
dimensions, settings and the tiles that differ from generation travel only in keyframes
(`redacted_for(viewer)`); the receiver regenerates the rest.
