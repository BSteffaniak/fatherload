//! Falling boulders: where they hang, when they shatter, how often they come down.
//!
//! `boulder_frequency_report` (ignored; `cargo test -p fatherload_simulation --release
//! boulder_frequency_report -- --ignored --nocapture`) counts boulders over seeded runs: the
//! economy bot mining, a rig parked under a collapse surge and a rig parked in the deeps.

use std::collections::BTreeMap;

use crate::{
    PlayerId, SimulationParams, TickInputs, WorldState,
    balance::{
        bot::{MinerBot, Strategy},
        economy::{BOT_PLAYER, inputs_for, new_world},
    },
    entity::{EntityId, EntityKind, EntityState},
    progression::WorldEventKind,
    protection::HazardGear,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{Band, WorldSettings},
};

/// What a run's boulders did.
#[derive(Clone, Copy, Debug, Default)]
struct BoulderTally {
    /// Boulders spawned.
    spawned: u32,
    /// Boulders spawned with their centre inside solid ground.
    in_rock: u32,
    /// Boulders gone (before their expiry, with no rig near) having fallen less than half a
    /// tile from where they hung: broken without a real fall.
    broke_unfallen: u32,
}

/// Watches the entity store tick by tick and tallies boulders.
#[derive(Default)]
struct BoulderWatch {
    tally: BoulderTally,
    /// Live boulders: id -> (spawn y, last seen (x, y), expiry tick).
    live: BTreeMap<EntityId, (f32, (f32, f32), u64)>,
}

impl BoulderWatch {
    fn observe(&mut self, world: &WorldState) {
        let now = world.simulation_tick().get();
        let mut seen = BTreeMap::new();
        for entity in world.entities().of_kind(EntityKind::Boulder) {
            let EntityState::Boulder(state) = entity.state else {
                continue;
            };
            let (spawn_y, _, expiry) = self.live.get(&entity.id).copied().unwrap_or_else(|| {
                self.tally.spawned += 1;
                if world
                    .terrain()
                    .is_solid_at(tile_of(entity.body.x, entity.body.y))
                {
                    self.tally.in_rock += 1;
                }
                (
                    entity.body.y,
                    (entity.body.x, entity.body.y),
                    state.expiry_tick.get(),
                )
            });
            seen.insert(entity.id, (spawn_y, (entity.body.x, entity.body.y), expiry));
        }
        for (id, (spawn_y, (last_x, last_y), expiry)) in &self.live {
            let rig_near = world
                .players()
                .values()
                .any(|player| (player.x - last_x).hypot(player.y - last_y) < TILE_SIZE * 2.0);
            if !seen.contains_key(id)
                && now < *expiry
                && !rig_near
                && last_y - spawn_y < TILE_SIZE * 0.5
            {
                self.tally.broke_unfallen += 1;
            }
        }
        self.live = seen;
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded test coordinates map to tiles"
)]
fn tile_of(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "small tile coordinates convert to pixels"
)]
fn px(tile: i32) -> f32 {
    tile as f32 * TILE_SIZE
}

/// The economy bot mining `seed`'s world for `minutes`.
fn mining_run(seed: u64, strategy: Strategy, minutes: u32) -> BoulderTally {
    let mut world = new_world(seed, 60);
    let mut bot = MinerBot::new(BOT_PLAYER, strategy);
    let mut watch = BoulderWatch::default();
    for _ in 0..60 * 60 * u64::from(minutes) {
        let (controls, actions) = bot.decide(&world);
        let inputs = inputs_for(BOT_PLAYER, controls, actions);
        let output = world.simulate_tick(world.simulation_tick(), &inputs);
        bot.observe(&world, &output);
        watch.observe(&world);
    }
    watch.tally
}

const PARKED: PlayerId = PlayerId::new(1);

/// A fresh world for `seed` with its rig parked in a carved 3 x 4 pocket whose floor is `row`
/// (rig hull topped up every tick, deflector maxed so nothing ends the run).
fn parked_world(seed: u64, column: i32, row: i32) -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        PARKED,
        &WorldSettings::new(seed),
        SimulationParams::new(60),
    );
    carve_pocket(&mut world, column, row);
    let player = world.player_mut(PARKED).expect("player");
    player.x = TILE_SIZE.mul_add(0.5, px(column));
    player.y = px(row + 1) - 11.0;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    player.hazard_gear = HazardGear::MAXED;
    world
}

/// Carves a 3 x 4 pocket whose floor is `row`, walled in stone.
fn carve_pocket(world: &mut WorldState, column: i32, row: i32) {
    for x in column - 2..=column + 2 {
        for y in row - 5..=row + 1 {
            let open = (column - 1..=column + 1).contains(&x) && (row - 3..=row).contains(&y);
            let kind = if open { TileKind::Air } else { TileKind::Stone };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
}

fn run_parked(world: &mut WorldState, minutes: u32) -> BoulderTally {
    let mut watch = BoulderWatch::default();
    let inputs = TickInputs::default();
    for _ in 0..60 * 60 * u64::from(minutes) {
        if let Some(player) = world.player_mut(PARKED) {
            player.hull = player.max_hull();
            player.fuel = player.fuel_capacity;
        }
        let _ = world.simulate_tick(world.simulation_tick(), &inputs);
        watch.observe(world);
    }
    watch.tally
}

/// A rig parked 30 rows down under an endless collapse surge.
fn collapse_surge_run(seed: u64, minutes: u32) -> BoulderTally {
    let mut world = parked_world(seed, 60, 40);
    world
        .world_event_severity
        .insert(WorldEventKind::CollapseSurge, 2);
    world
        .world_event_days_remaining
        .insert(WorldEventKind::CollapseSurge, 1_000);
    run_parked(&mut world, minutes)
}

/// A rig parked in the crystal faults with a diamond aboard (deep-run pressure).
fn deep_pressure_run(seed: u64, minutes: u32) -> BoulderTally {
    let settings = WorldSettings::new(seed);
    let column = 60;
    let row = crate::worldgen::strata::band_solid_top_row(
        &settings,
        Band::Deep(crate::terrain::DeepStratum::CrystalFaults),
        column,
    ) + 4;
    let mut world = parked_world(seed, column, row);
    world
        .player_mut(PARKED)
        .expect("player")
        .cargo_bay
        .insert(crate::terrain::MineralKind::Diamond, 1);
    run_parked(&mut world, minutes)
}

/// A rig roaming the crystal faults with a diamond aboard: every minute it moves on to a fresh
/// pocket 30 columns along (new, unsettled ground), the way an exploring rig keeps meeting
/// deep-run cave-ins that a parked one sits out under settled ground.
fn deep_roaming_run(seed: u64, minutes: u32) -> BoulderTally {
    let settings = WorldSettings::new(seed);
    let row_at = |column| {
        crate::worldgen::strata::band_solid_top_row(
            &settings,
            Band::Deep(crate::terrain::DeepStratum::CrystalFaults),
            column,
        ) + 4
    };
    let mut column = 60;
    let mut world = parked_world(seed, column, row_at(column));
    world
        .player_mut(PARKED)
        .expect("player")
        .cargo_bay
        .insert(crate::terrain::MineralKind::Diamond, 1);
    let mut watch = BoulderWatch::default();
    let inputs = TickInputs::default();
    for tick in 0..60 * 60 * u64::from(minutes) {
        if tick > 0 && tick % (60 * 60) == 0 {
            column += 30;
            let row = row_at(column);
            carve_pocket(&mut world, column, row);
            let player = world.player_mut(PARKED).expect("player");
            player.x = TILE_SIZE.mul_add(0.5, px(column));
            player.y = px(row + 1) - 11.0;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
        }
        if let Some(player) = world.player_mut(PARKED) {
            player.hull = player.max_hull();
            player.fuel = player.fuel_capacity;
        }
        let _ = world.simulate_tick(world.simulation_tick(), &inputs);
        watch.observe(&world);
    }
    watch.tally
}

fn add(total: &mut BoulderTally, run: BoulderTally) {
    total.spawned += run.spawned;
    total.in_rock += run.in_rock;
    total.broke_unfallen += run.broke_unfallen;
}

#[test]
#[ignore = "frequency report: run in release with -- --ignored --nocapture"]
fn boulder_frequency_report() {
    const SEEDS: [u64; 4] = [11, 4_242, 77, 9_001];
    const MINING_MINUTES: u32 = 20;
    const PARKED_MINUTES: u32 = 30;
    let mut lines = Vec::new();
    for (name, minutes, run) in [
        (
            "mining (prospector)",
            MINING_MINUTES,
            &(|seed| mining_run(seed, Strategy::Prospector, MINING_MINUTES))
                as &dyn Fn(u64) -> BoulderTally,
        ),
        ("mining (shaft)", MINING_MINUTES, &|seed| {
            mining_run(seed, Strategy::Shaft, MINING_MINUTES)
        }),
        ("parked under a collapse surge", PARKED_MINUTES, &|seed| {
            collapse_surge_run(seed, PARKED_MINUTES)
        }),
        ("parked in the crystal faults", PARKED_MINUTES, &|seed| {
            deep_pressure_run(seed, PARKED_MINUTES)
        }),
        ("roaming the crystal faults", PARKED_MINUTES, &|seed| {
            deep_roaming_run(seed, PARKED_MINUTES)
        }),
    ] {
        let mut total = BoulderTally::default();
        for seed in SEEDS {
            add(&mut total, run(seed));
        }
        #[allow(clippy::cast_precision_loss, reason = "small counts")]
        let per_hour = f64::from(total.spawned) * 60.0 / (f64::from(minutes) * SEEDS.len() as f64);
        lines.push(format!(
            "| {name} | {} | {per_hour:.1} | {} | {} |",
            total.spawned, total.in_rock, total.broke_unfallen
        ));
    }
    println!(
        "| Scenario ({} seeds) | Boulders | Per hour | Spawned in rock | Broke without falling |",
        SEEDS.len()
    );
    println!("|---|---|---|---|---|");
    for line in lines {
        println!("{line}");
    }
}

/// Every boulder alive in `world`: its tile and the tile under it.
fn boulder_tiles(world: &WorldState) -> Vec<(TilePosition, TilePosition)> {
    world
        .falling_boulders()
        .iter()
        .map(|boulder| {
            let tile = tile_of(boulder.x, boulder.y);
            (
                tile,
                TilePosition {
                    x: tile.x,
                    y: tile.y + 1,
                },
            )
        })
        .collect()
}

/// Ticks `world` with idle inputs `ticks` times, watching its boulders; returns how many
/// ticks reported new boulders.
fn idle(world: &mut WorldState, ticks: u64, watch: &mut BoulderWatch) -> u32 {
    let inputs = TickInputs::default();
    let mut changed = 0;
    for _ in 0..ticks {
        let output = world.simulate_tick(world.simulation_tick(), &inputs);
        if output
            .events
            .iter()
            .any(|event| event.kind == crate::GameplayEventKind::BouldersChanged)
        {
            changed += 1;
        }
        watch.observe(world);
    }
    changed
}

fn rig_point(world: &WorldState) -> (f32, f32) {
    let rig = world.player(PARKED).expect("rig");
    (rig.x, rig.y)
}

#[test]
fn a_cut_in_a_low_tunnel_loosens_no_boulder_into_the_rock() {
    // Drilling sideways along a one-tile-high tunnel: the tile over the cut is rock and so is
    // the tile under it. The old spawn put the boulder in the rock over the cut, where it
    // "shattered" the moment its warning ran out.
    let mut world = parked_world(11, 60, 40);
    let cut = TilePosition { x: 63, y: 40 };
    let _ = world.terrain_mut().set_kind(cut, TileKind::Air);
    assert!(!world.loosen_mining_boulder(cut, 1), "nowhere to fall");
    assert!(world.falling_boulders().is_empty());

    // Drilling down out of the pocket: the boulder hangs in the open tile over the cut.
    let down = TilePosition { x: 60, y: 41 };
    let _ = world.terrain_mut().set_kind(down, TileKind::Air);
    assert!(world.loosen_mining_boulder(down, 1));
    let tiles = boulder_tiles(&world);
    assert_eq!(tiles.len(), 1);
    assert_eq!(tiles[0].0, TilePosition { x: 60, y: 40 });

    // Drilling up into the ceiling: the boulder hangs in the cut, over the rig.
    let mut world = parked_world(11, 60, 40);
    let up = TilePosition { x: 60, y: 36 };
    let _ = world.terrain_mut().set_kind(up, TileKind::Air);
    assert!(world.loosen_mining_boulder(up, 1));
    assert_eq!(boulder_tiles(&world)[0].0, up);
}

#[test]
fn cave_ins_hang_only_in_open_ground_with_room_to_fall() {
    // A one-tile-high tunnel has no room for a cave-in at all.
    let mut tunnel = parked_world(11, 60, 40);
    for y in 37..=39 {
        for x in 59..=61 {
            let _ = tunnel
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Stone);
        }
    }
    let (x, y) = rig_point(&tunnel);
    assert!(!tunnel.spawn_cave_in(x, y));
    assert!(tunnel.falling_boulders().is_empty());

    // A room gets boulders in every open column; a column walled off at the rig's height
    // gets none.
    let mut room = parked_world(11, 60, 40);
    for y in 37..=40 {
        let _ = room
            .terrain_mut()
            .set_kind(TilePosition { x: 61, y }, TileKind::Stone);
    }
    let (x, y) = rig_point(&room);
    assert!(room.spawn_cave_in(x, y));
    let tiles = boulder_tiles(&room);
    assert_eq!(tiles.len(), 2, "the walled-off column gets none");
    for (tile, below) in tiles {
        assert!(!room.terrain().is_solid_at(tile), "{tile:?} is open");
        assert!(!room.terrain().is_solid_at(below), "room under {tile:?}");
    }
}

#[test]
fn no_boulder_starts_in_rock_or_breaks_without_an_impact() {
    // Cave-ins and mining boulders all over a generated world's open ground (the rig far
    // away, so only the ground stops them): each hangs in open ground with room under it and
    // falls a real distance before it shatters.
    let mut world = parked_world(4_242, 60, 12);
    let mut spots = Vec::new();
    'scan: for y in (30..240).step_by(3) {
        for x in (-150..150).step_by(5) {
            let tile = TilePosition { x, y };
            if !world.terrain().is_solid_at(tile) {
                spots.push(tile);
                if spots.len() >= 120 {
                    break 'scan;
                }
            }
        }
    }
    assert!(spots.len() >= 40, "the world has caves to test in");
    let mut watch = BoulderWatch::default();
    for (index, tile) in spots.iter().enumerate() {
        if index % 2 == 0 {
            let _ = world.spawn_cave_in(
                TILE_SIZE.mul_add(0.5, px(tile.x)),
                TILE_SIZE.mul_add(0.5, px(tile.y)),
            );
        } else {
            let _ = world.loosen_mining_boulder(*tile, 1);
        }
    }
    for (tile, below) in boulder_tiles(&world) {
        assert!(!world.terrain().is_solid_at(tile), "{tile:?} hangs in rock");
        assert!(!world.terrain().is_solid_at(below), "{tile:?} has no room");
    }
    watch.observe(&world);
    assert!(watch.tally.spawned >= 20, "{:?}", watch.tally);
    let _ = idle(&mut world, 60 * 8, &mut watch);
    assert!(
        world.falling_boulders().is_empty(),
        "every boulder came down"
    );
    assert_eq!(watch.tally.in_rock, 0, "{:?}", watch.tally);
    assert_eq!(watch.tally.broke_unfallen, 0, "{:?}", watch.tally);
}

/// A rig parked in a two-high room 30 rows down under an endless collapse surge.
fn surge_world(hz: u32) -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        PARKED,
        &WorldSettings::new(11),
        SimulationParams::new(hz),
    );
    let template = parked_world(11, 60, 40);
    for x in 58..=62 {
        for y in 35..=41 {
            let tile = TilePosition { x, y };
            let kind = template
                .terrain()
                .tile(tile)
                .map_or(TileKind::Air, |found| found.kind);
            let _ = world.terrain_mut().set_kind(tile, kind);
        }
    }
    let rig = template.player(PARKED).expect("rig").clone();
    *world.player_mut(PARKED).expect("rig") = rig;
    world
        .world_event_severity
        .insert(WorldEventKind::CollapseSurge, 2);
    world
        .world_event_days_remaining
        .insert(WorldEventKind::CollapseSurge, 1_000);
    world
}

#[test]
fn settled_ground_holds_and_a_cleared_spot_never_retriggers() {
    use super::hazards::{COLLAPSE_SURGE_PERIOD_MILLIS, SETTLED_GROUND_MILLIS};
    let mut world = surge_world(60);
    let mut watch = BoulderWatch::default();
    let ticks = |millis: u64| millis * 60 / 1_000;
    // The first surge roll brings the room's ceiling down once...
    let first = idle(
        &mut world,
        ticks(COLLAPSE_SURGE_PERIOD_MILLIS) + 1,
        &mut watch,
    );
    assert_eq!(first, 1, "one cave-in");
    assert_eq!(watch.tally.spawned, 3);
    assert!(!world.progression().settled_ground.is_empty());
    // ...and the rolls after it find settled ground until the hold runs out.
    let held = idle(
        &mut world,
        ticks(SETTLED_GROUND_MILLIS - COLLAPSE_SURGE_PERIOD_MILLIS) - 2,
        &mut watch,
    );
    assert_eq!(held, 0, "settled ground holds");
    assert_eq!(watch.tally.spawned, 3);
    // A mining boulder next to it holds too.
    let floor = TilePosition { x: 60, y: 41 };
    let _ = world.terrain_mut().set_kind(floor, TileKind::Air);
    assert!(!world.loosen_mining_boulder(floor, 1));
    let _ = world.terrain_mut().set_kind(floor, TileKind::Stone);
    // Once it has run out (and been pruned) the ground may move again.
    let _ = idle(
        &mut world,
        ticks(COLLAPSE_SURGE_PERIOD_MILLIS) + 4,
        &mut watch,
    );
    assert_eq!(watch.tally.spawned, 6, "the next roll after the hold");
    assert_eq!(watch.tally.broke_unfallen, 0);
}

#[test]
fn a_fallen_stalactite_spot_stays_clear() {
    use crate::cave::CaveTile;
    let probe = parked_world(77, 60, 40);
    if !probe.terrain().settings().generator.has_cave_biomes() {
        return;
    }
    // A tall shaft with a loose stalactite near its top, over the rig.
    let seed = probe.terrain().seed();
    let column = (40..80)
        .find(|&x| super::caves::stalactite_is_loose(seed, TilePosition { x, y: 34 }))
        .expect("a loose stalactite column");
    let mut world = parked_world(77, column, 40);
    for y in 33..=40 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x: column, y }, TileKind::Air);
    }
    let hanging = TilePosition { x: column, y: 34 };
    let _ = world
        .terrain_mut()
        .set_kind(hanging, TileKind::Cave(CaveTile::Stalactite));
    let mut watch = BoulderWatch::default();
    let _ = idle(&mut world, 1, &mut watch);
    assert_eq!(watch.tally.spawned, 1, "the loose stalactite falls");
    let _ = idle(&mut world, 60 * 240, &mut watch);
    assert_eq!(watch.tally.spawned, 1, "nothing more ever falls there");
    assert_eq!(watch.tally.broke_unfallen, 0);
}

#[test]
fn collapses_keep_the_same_schedule_at_every_tick_rate() {
    // Cave-in times (ms) and boulder counts agree at 30, 60 and 120 Hz.
    let run = |hz: u32| {
        let mut world = surge_world(hz);
        let mut watch = BoulderWatch::default();
        let mut times = Vec::new();
        let inputs = TickInputs::default();
        for _ in 0..u64::from(hz) * 230 {
            let output = world.simulate_tick(world.simulation_tick(), &inputs);
            if output
                .events
                .iter()
                .any(|event| event.kind == crate::GameplayEventKind::BouldersChanged)
            {
                times.push(world.simulation_tick().get() * 1_000 / u64::from(hz));
            }
            watch.observe(&world);
        }
        (times, watch.tally.spawned, watch.tally.broke_unfallen)
    };
    let reference = run(60);
    assert_eq!(reference.0, [45_000, 225_000], "{reference:?}");
    for hz in [30, 120] {
        let other = run(hz);
        assert_eq!(other.0.len(), reference.0.len(), "{hz} Hz: {other:?}");
        assert_eq!(other.1, reference.1, "{hz} Hz boulders");
        assert_eq!(other.2, 0);
        for (a, b) in other.0.iter().zip(&reference.0) {
            assert!(a.abs_diff(*b) <= 40, "{hz} Hz: {other:?} vs {reference:?}");
        }
    }
}

#[test]
fn collapses_are_deterministic() {
    let run = || {
        let mut world = surge_world(60);
        let mut watch = BoulderWatch::default();
        let _ = idle(&mut world, 60 * 100, &mut watch);
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn settled_ground_saves_replicates_and_old_saves_load() {
    let mut world = surge_world(60);
    let mut watch = BoulderWatch::default();
    let _ = idle(&mut world, 60 * 46, &mut watch);
    let settled = world.progression().settled_ground.clone();
    assert!(!settled.is_empty());

    // Saves (JSON and the compact form) keep it.
    let json: WorldState =
        serde_json::from_str(&serde_json::to_string(&world).expect("save")).expect("load");
    assert_eq!(json.progression().settled_ground, settled);
    assert_eq!(json.simulation_state_hash(), world.simulation_state_hash());
    let compact: WorldState =
        postcard::from_bytes(&postcard::to_allocvec(&world).expect("save")).expect("load");
    assert_eq!(compact.progression().settled_ground, settled);

    // A replica gets it with the timers section.
    let mut replica = surge_world(60);
    let section = world
        .section(super::replication::SectionKey::Timers)
        .expect("timers");
    replica.apply_section(super::replication::SectionKey::Timers, section);
    assert_eq!(replica.progression().settled_ground, settled);

    // Saves from before settled ground load with none, and a world with none hashes as before.
    let mut value = serde_json::to_value(&world).expect("save");
    value["progression"]
        .as_object_mut()
        .expect("progression")
        .remove("settled_ground");
    let old: WorldState = serde_json::from_value(value).expect("old save");
    assert!(old.progression().settled_ground.is_empty());
    let mut cleared = world.clone();
    cleared.progression.settled_ground = crate::hazard::SettledGround::default();
    assert_eq!(cleared.simulation_state_hash(), old.simulation_state_hash());
    assert_ne!(
        cleared.simulation_state_hash(),
        world.simulation_state_hash()
    );
}
