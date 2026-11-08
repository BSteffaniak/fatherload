//! Flight feel: thrust is a force, so short human-rate taps feather, hover and land softly,
//! while a long free fall still hurts.

use crate::{
    ControlState, PlayerId, SimulationParams, SimulationTick, TickInputs, WorldState,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{AUTHORITATIVE_MAX_CLIMB_SPEED, TickTiming};

const PLAYER: PlayerId = PlayerId::new(1);
const HZ: u32 = 60;
/// How often a simulated human re-decides whether to hold thrust (100 ms).
const DECISION_TICKS: u64 = 6;

/// A world with open air above a flat dirt floor; returns the world and the floor's y in pixels.
fn open_air_world() -> (WorldState, f32) {
    let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(HZ));
    let spawn = world
        .player(PLAYER)
        .expect("player")
        .tile_position(TILE_SIZE);
    for x in spawn.x - 12..=spawn.x + 12 {
        for y in -12..spawn.y + 30 {
            let kind = if y <= spawn.y {
                TileKind::Air
            } else {
                TileKind::Dirt
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "small tile coordinates convert to pixels"
    )]
    let floor = (spawn.y + 1) as f32 * TILE_SIZE;
    (world, floor)
}

/// Places the rig `tiles` above the floor, at rest.
fn lift(world: &mut WorldState, floor: f32, tiles: f32) {
    let player = world.player_mut(PLAYER).expect("player");
    player.y = tiles.mul_add(-TILE_SIZE, TILE_SIZE.mul_add(-0.5, floor));
    player.velocity_y = 0.0;
    player.velocity_x = 0.0;
    player.fuel = player.fuel_capacity;
}

fn step(world: &mut WorldState, tick: &mut u64, thrust: bool) {
    let inputs = TickInputs::held(PLAYER, ControlState::new(0.0, thrust, false));
    let _ = world.simulate_tick(SimulationTick::new(*tick), &inputs);
    *tick += 1;
}

#[test]
fn thrust_ramps_up_instead_of_snapping_to_full_climb() {
    let (mut world, floor) = open_air_world();
    lift(&mut world, floor, 6.0);
    let mut tick = 0;
    step(&mut world, &mut tick, true);
    let first = -world.player(PLAYER).expect("player").velocity_y;
    assert!(
        first < AUTHORITATIVE_MAX_CLIMB_SPEED * 0.2,
        "one tick of thrust should only start the climb, got {first}"
    );
    for _ in 0..30 {
        step(&mut world, &mut tick, true);
    }
    let held = -world.player(PLAYER).expect("player").velocity_y;
    assert!(
        held > AUTHORITATIVE_MAX_CLIMB_SPEED * 0.95,
        "half a second of thrust should reach climb speed, got {held}"
    );
}

#[test]
fn human_rate_tapping_hovers_within_a_tile() {
    let (mut world, floor) = open_air_world();
    lift(&mut world, floor, 6.0);
    let mut tick = 0;
    let (mut highest, mut lowest) = (f32::MAX, f32::MIN);
    for decision in 0..40 {
        let falling = world.player(PLAYER).expect("player").velocity_y > 0.0;
        for _ in 0..DECISION_TICKS {
            step(&mut world, &mut tick, falling);
        }
        if decision >= 10 {
            let y = world.player(PLAYER).expect("player").y;
            highest = highest.min(y);
            lowest = lowest.max(y);
        }
    }
    assert!(
        lowest - highest < TILE_SIZE,
        "tapping thrust at 10 Hz should hold altitude, drifted {} px",
        lowest - highest
    );
}

#[test]
fn feathered_descent_lands_without_damage() {
    let (mut world, floor) = open_air_world();
    lift(&mut world, floor, 8.0);
    let hull = world.player(PLAYER).expect("player").hull;
    let mut tick = 0;
    for _ in 0..200 {
        let player = world.player(PLAYER).expect("player");
        // Hold a gentle sink rate: thrust whenever falling faster than ~2 tiles/s.
        let thrust = player.velocity_y > 2.0 * TILE_SIZE;
        for _ in 0..DECISION_TICKS {
            step(&mut world, &mut tick, thrust);
        }
    }
    let player = world.player(PLAYER).expect("player");
    assert!(
        player.y > floor - TILE_SIZE,
        "rig should have landed, y {}",
        player.y
    );
    assert!(
        (player.hull - hull).abs() < f32::EPSILON,
        "a feathered landing must not damage the hull"
    );
}

#[test]
fn long_free_fall_still_damages_the_hull() {
    let (mut world, floor) = open_air_world();
    lift(&mut world, floor, 11.0);
    let hull = world.player(PLAYER).expect("player").hull;
    let mut tick = 0;
    for _ in 0..240 {
        step(&mut world, &mut tick, false);
    }
    assert!(world.player(PLAYER).expect("player").hull < hull);
}

#[test]
fn timing_from_older_saves_recomputes_vertical_values() {
    let current = TickTiming::new(SimulationParams::new(HZ));
    let mut json = serde_json::to_value(current).expect("encode");
    let object = json.as_object_mut().expect("object");
    object.remove("vertical_drag_per_tick");
    object.remove("vertical_acceleration_gain");
    let restored: TickTiming = serde_json::from_value(json).expect("decode");
    assert_eq!(restored, current);
}
