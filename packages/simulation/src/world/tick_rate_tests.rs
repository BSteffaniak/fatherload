//! Gameplay must feel the same at every session tick rate (plan §16).
//!
//! The same scripted timeline (authored in seconds) runs at 30, 60, 120 and 240 Hz. End
//! positions, fuel use, hull and dig times must agree within small tolerances; integration
//! differences between rates are expected to be small, never proportional to the rate.

use crate::{
    ControlState, InputSequence, PlayerId, SimulationParams, SimulationTick,
    TerrainRevisionTracker, TickInputs, WorldState,
    input::PlayerTickInput,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(1);

/// One scripted segment: held controls from `start` seconds until the next segment.
struct Segment {
    start: f32,
    horizontal: f32,
    thrust: bool,
    drill_down: bool,
}

const fn segment(start: f32, horizontal: f32, thrust: bool, drill_down: bool) -> Segment {
    Segment {
        start,
        horizontal,
        thrust,
        drill_down,
    }
}

/// Settle, drive right, dig down, fly up out of the shaft, drift right in the air, land.
const TIMELINE: [Segment; 7] = [
    segment(0.0, 0.0, false, false),
    segment(1.0, 1.0, false, false),
    segment(2.5, 0.0, false, true),
    segment(4.5, 0.0, true, false),
    segment(7.5, 0.6, true, false),
    segment(8.5, 0.0, false, false),
    segment(9.0, 0.0, false, false),
];
const END_SECONDS: u64 = 13;

#[derive(Debug)]
struct Outcome {
    x: f32,
    y: f32,
    fuel_used: f32,
    hull: f32,
    tiles_dug: usize,
    first_dig_seconds: f32,
    last_dig_seconds: f32,
    max_height: f32,
}

/// A world with a flat, uniform dirt ground under the spawn, so the run measures movement and
/// digging rather than generated-terrain quirks.
fn flat_world(hz: u32) -> WorldState {
    let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(hz));
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
    world
}

#[allow(
    clippy::cast_precision_loss,
    reason = "tick counts become timeline seconds"
)]
fn run(hz: u32) -> Outcome {
    let mut world = flat_world(hz);
    let fuel_before = world.player(PLAYER).expect("player").fuel;
    let mut revisions = TerrainRevisionTracker::default();
    let ticks = u64::from(hz) * END_SECONDS;
    let mut tiles_dug = 0;
    let mut first_dig_seconds = f32::NAN;
    let mut last_dig_seconds = f32::NAN;
    let mut max_height = f32::MIN;
    for tick in 0..ticks {
        let seconds = tick as f32 / hz as f32;
        let segment = TIMELINE
            .iter()
            .rev()
            .find(|segment| seconds >= segment.start)
            .expect("timeline starts at zero");
        let mut inputs = TickInputs::default();
        inputs.players.insert(
            PLAYER,
            PlayerTickInput {
                controls: Some((
                    InputSequence::new(u32::try_from(tick).unwrap_or(u32::MAX)),
                    ControlState::new(segment.horizontal, segment.thrust, segment.drill_down),
                )),
                actions: Vec::new(),
            },
        );
        let output = world.simulate_tick(SimulationTick::new(tick), &inputs, &mut revisions);
        let dug = output
            .terrain_ops
            .iter()
            .filter(|change| change.tile.kind == TileKind::Air)
            .count();
        if dug > 0 {
            let at = (tick + 1) as f32 / hz as f32;
            if tiles_dug == 0 {
                first_dig_seconds = at;
            }
            last_dig_seconds = at;
            tiles_dug += dug;
        }
        let player = world.player(PLAYER).expect("player");
        max_height = max_height.max(-player.y);
    }
    let player = world.player(PLAYER).expect("player");
    Outcome {
        x: player.x,
        y: player.y,
        fuel_used: fuel_before - player.fuel,
        hull: player.hull,
        tiles_dug,
        first_dig_seconds,
        last_dig_seconds,
        max_height,
    }
}

#[test]
#[allow(
    clippy::cast_precision_loss,
    reason = "small tile counts become a dig pace"
)]
fn same_timeline_matches_across_tick_rates() {
    let reference = run(60);
    assert!(reference.tiles_dug >= 3, "timeline digs: {reference:?}");
    assert!(
        reference.fuel_used > 1.0,
        "timeline burns fuel: {reference:?}"
    );
    for hz in [30, 120, 240] {
        let outcome = run(hz);
        let message = format!("{hz} Hz {outcome:?} vs 60 Hz {reference:?}");
        // Positions within a quarter tile, apex within half a tile.
        assert!(
            (outcome.x - reference.x).abs() <= TILE_SIZE * 0.25,
            "x: {message}"
        );
        assert!(
            (outcome.y - reference.y).abs() <= TILE_SIZE * 0.25,
            "y: {message}"
        );
        assert!(
            (outcome.max_height - reference.max_height).abs() <= TILE_SIZE * 0.5,
            "apex: {message}"
        );
        // Fuel use within 5 %.
        assert!(
            (outcome.fuel_used - reference.fuel_used).abs() <= reference.fuel_used * 0.05,
            "fuel: {message}"
        );
        assert!(
            (outcome.hull - reference.hull).abs() <= 1.0,
            "hull: {message}"
        );
        // Dig pace within 15 %: drill periods round up to whole ticks, which costs at most one
        // tick per chip, so at most one tile of difference over the digging segment.
        assert!(
            outcome.tiles_dug.abs_diff(reference.tiles_dug) <= 1,
            "tiles: {message}"
        );
        let pace =
            |o: &Outcome| (o.last_dig_seconds - o.first_dig_seconds) / (o.tiles_dug as f32 - 1.0);
        assert!(
            (pace(&outcome) - pace(&reference)).abs() <= pace(&reference) * 0.15,
            "dig pace: {message}"
        );
        assert!(
            (outcome.first_dig_seconds - reference.first_dig_seconds).abs()
                <= (reference.first_dig_seconds - 2.5).mul_add(0.15, 1.0 / 30.0),
            "first dig: {message}"
        );
    }
}

#[test]
fn horizontal_speed_does_not_depend_on_tick_rate() {
    let distance = |hz: u32| {
        let mut world = flat_world(hz);
        let start = world.player(PLAYER).expect("player").x;
        let mut revisions = TerrainRevisionTracker::default();
        let ticks = u64::from(hz);
        for tick in 0..ticks {
            let mut inputs = TickInputs::default();
            inputs.players.insert(
                PLAYER,
                PlayerTickInput {
                    controls: Some((InputSequence::new(0), ControlState::new(1.0, false, false))),
                    actions: Vec::new(),
                },
            );
            let _ = world.simulate_tick(SimulationTick::new(tick), &inputs, &mut revisions);
        }
        world.player(PLAYER).expect("player").x - start
    };
    let reference = distance(60);
    assert!(
        reference > TILE_SIZE,
        "rig moves over a tile per second: {reference}"
    );
    for hz in [30, 120, 240] {
        let moved = distance(hz);
        assert!(
            (moved - reference).abs() <= reference * 0.03,
            "{hz} Hz moved {moved}, 60 Hz moved {reference}"
        );
    }
}
