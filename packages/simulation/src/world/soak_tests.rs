//! Long sessions (wave 6 QA): many in-game days with bots, watching every part of the world
//! for growth that never stops.

use crate::{
    PlayerId, SimulationParams, TickInputs, WorldState,
    balance::{
        bot::{MinerBot, Strategy},
        economy::inputs_for,
    },
    clock::DayLength,
    enemy::EnemyDifficulty,
    worldgen::WorldSettings,
};

const FIRST: PlayerId = PlayerId::new(0);
const SECOND: PlayerId = PlayerId::new(1);
/// Stays in town all session (village routines, contractors, night at the edge of town).
const IDLER: PlayerId = PlayerId::new(2);
/// Roams the surface east and west (villages, camps, night monsters, sky enemies).
const ROAMER: PlayerId = PlayerId::new(3);

/// A two-rig world on the short day, Normal enemies, two mining bots.
fn soak_world(seed: u64, hz: u32) -> (WorldState, [MinerBot; 2]) {
    let mut world = WorldState::new_game_with_settings(
        FIRST,
        &WorldSettings::new(seed),
        SimulationParams::new(hz),
    );
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    world.set_day_length(DayLength::Short);
    world.add_player(SECOND);
    world.add_player(IDLER);
    world.add_player(ROAMER);
    (
        world,
        [
            MinerBot::new(FIRST, Strategy::Prospector),
            MinerBot::new(SECOND, Strategy::Charges),
        ],
    )
}

fn step(world: &mut WorldState, bots: &mut [MinerBot; 2]) {
    let mut inputs = TickInputs::default();
    for (bot, player) in bots.iter_mut().zip([FIRST, SECOND]) {
        let (controls, actions) = bot.decide(world);
        let one = inputs_for(player, controls, actions);
        inputs.players.extend(one.players);
    }
    // The roamer flies out and back, hopping over the ground, each leg 90 s longer than the
    // last (east 90 s, west 180 s, east 270 s, ...), so it sweeps ever wider ground on both
    // sides of town instead of shuttling over the same few hundred tiles all session.
    let tick = world.simulation_tick().get();
    let mut leg = 0_u64;
    let mut leg_end = u64::from(world.tick_rate_hz()) * 90;
    while tick >= leg_end {
        leg += 1;
        leg_end += u64::from(world.tick_rate_hz()) * 90 * (leg + 1);
    }
    let horizontal = if leg.is_multiple_of(2) { 1.0 } else { -1.0 };
    // It keeps a few tiles above the highest ground just ahead (hops alone stalled it against
    // the first cliff each way for the whole session).
    let thrust = world.player(ROAMER).is_some_and(|rig| {
        #[allow(clippy::cast_possible_truncation, reason = "tile columns")]
        let column = (rig.x / crate::TILE_SIZE).floor() as i32;
        #[allow(clippy::cast_possible_truncation, reason = "tile columns")]
        let ahead = horizontal as i32;
        let highest = (-2..=10)
            .map(|step| world.terrain().surface_row(column + step * ahead))
            .min()
            .unwrap_or(0);
        #[allow(clippy::cast_precision_loss, reason = "tile rows")]
        let cruise = (highest - 4) as f32 * crate::TILE_SIZE;
        rig.y > cruise && rig.velocity_y > -120.0
    });
    let roam = inputs_for(
        ROAMER,
        crate::ControlState::new(horizontal, thrust, false),
        Vec::new(),
    );
    inputs.players.extend(roam.players);
    // The roamer is the soak's eyes on the surface, not a player under test: kept fuelled and
    // whole so it keeps flying all session (it used to run dry early and sit, which left the
    // villages, camps and night monsters out east and west unvisited).
    if let Some(rig) = world.player_mut(ROAMER) {
        rig.fuel = rig.fuel_capacity;
        rig.hull = rig.max_hull();
    }
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    for bot in bots.iter_mut() {
        bot.observe(world, &output);
    }
}

/// Serialized size of every part of the world, three object levels deep.
fn field_sizes(world: &WorldState) -> Vec<(String, usize)> {
    fn walk(prefix: &str, value: &serde_json::Value, depth: u32, sizes: &mut Vec<(String, usize)>) {
        match value {
            serde_json::Value::Object(fields) if depth > 0 => {
                for (name, field) in fields {
                    let path = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{prefix}.{name}")
                    };
                    walk(&path, field, depth - 1, sizes);
                }
            }
            other => sizes.push((prefix.to_owned(), other.to_string().len())),
        }
    }
    let value = serde_json::to_value(world).expect("world serializes");
    let mut sizes = Vec::new();
    walk("", &value, 3, &mut sizes);
    sizes
}

/// Parts of the world that grow with ground explored and dug (bounded by the world, not by
/// time), and the live entities (checked by count instead).
fn grows_with_exploration(path: &str) -> bool {
    path.starts_with("discovered_tiles")
        || path.starts_with("terrain")
        || path.starts_with("entities")
}

/// `cargo test -p fatherload_simulation --release soak_growth_report -- --ignored --nocapture`
/// (`SOAK_DAYS` sets the length, 12 by default): four rigs (two mining bots, one idle in town,
/// one roaming the surface) on the short day for many in-game days. Prints the serialized size
/// of every part of the world at the end of each day, and fails if any part not tied to
/// exploration keeps growing through the second half of the session or the entity store
/// swells.
#[test]
#[ignore = "report: many in-game days of bot play (run with --release)"]
fn soak_growth_report() {
    let days: u32 = std::env::var("SOAK_DAYS")
        .ok()
        .and_then(|days| days.parse().ok())
        .unwrap_or(12);
    let hz = 20;
    let (mut world, mut bots) = soak_world(0x50AC, hz);
    let start_day = world.clock().day();
    let mut previous: Vec<(String, usize)> = field_sizes(&world);
    let mut midway: Option<Vec<(String, usize)>> = None;
    let mut most_entities = 0;
    let mut day = start_day;
    let mut roamed = (f32::MAX, f32::MIN);
    while world.clock().day() < start_day + days {
        step(&mut world, &mut bots);
        most_entities = most_entities.max(world.entities().iter().len());
        if let Some(rig) = world.player(ROAMER) {
            roamed = (roamed.0.min(rig.x), roamed.1.max(rig.x));
        }
        if world.clock().day() != day {
            day = world.clock().day();
            let sizes = field_sizes(&world);
            println!(
                "day {day}: entities {} ({} awake), roamer over x {:.0}..{:.0} tiles",
                world.entities().iter().len(),
                world.entities().awake_count(),
                roamed.0 / crate::TILE_SIZE,
                roamed.1 / crate::TILE_SIZE
            );
            for (name, size) in &sizes {
                let before = previous
                    .iter()
                    .find(|(other, _)| other == name)
                    .map_or(0, |(_, size)| *size);
                if *size > 2_000 || size.abs_diff(before) > 500 {
                    println!(
                        "  {name:48} {size:>9} ({:+})",
                        size.cast_signed() - before.cast_signed()
                    );
                }
            }
            if day == start_day + days / 2 {
                midway = Some(sizes.clone());
            }
            previous = sizes;
        }
    }
    assert!(
        most_entities < 2_000,
        "the entity store swelled to {most_entities}"
    );
    // The surface side of the soak only counts if the roamer really covered ground (12 days:
    // about 3,000 tiles across both sides of town).
    if days >= 12 {
        let span = (roamed.1 - roamed.0) / crate::TILE_SIZE;
        assert!(span > 1_500.0, "the roamer covered only {span:.0} tiles");
    }
    let midway = midway.expect("the session passed its midpoint");
    for (name, size) in &previous {
        if grows_with_exploration(name) {
            continue;
        }
        let before = midway
            .iter()
            .find(|(other, _)| other == name)
            .map_or(0, |(_, size)| *size);
        assert!(
            *size <= (before * 2).max(before + 8_000),
            "{name} kept growing through the second half: {before} -> {size} bytes"
        );
    }
}
