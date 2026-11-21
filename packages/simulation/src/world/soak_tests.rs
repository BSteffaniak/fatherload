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
    // The roamer flies a few hundred tiles out and back, hopping over the ground.
    let tick = world.simulation_tick().get();
    let leg = tick / (u64::from(world.tick_rate_hz()) * 90);
    let horizontal = if leg.is_multiple_of(2) { 1.0 } else { -1.0 };
    let thrust = world
        .player(ROAMER)
        .is_some_and(|rig| rig.velocity_y >= 0.0 && tick % 40 < 14);
    let roam = inputs_for(
        ROAMER,
        crate::ControlState::new(horizontal, thrust, false),
        Vec::new(),
    );
    inputs.players.extend(roam.players);
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
    while world.clock().day() < start_day + days {
        step(&mut world, &mut bots);
        most_entities = most_entities.max(world.entities().iter().len());
        if world.clock().day() != day {
            day = world.clock().day();
            let sizes = field_sizes(&world);
            println!(
                "day {day}: entities {} ({} awake)",
                world.entities().iter().len(),
                world.entities().awake_count()
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
