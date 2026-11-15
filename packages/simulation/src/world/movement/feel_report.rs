//! The measured movement-feel table (WP-FEEL acceptance) and the properties it must keep.
//!
//! Every number comes from the real fixed-tick simulation on a flat, open dirt corridor (the
//! flight rows from the drive model the simulation uses in the air). Print the table with
//!
//! ```text
//! cargo test -p fatherload_simulation feel_report -- --ignored --nocapture
//! ```

use std::fmt::Write as _;

use crate::{
    ControlState, PlayerId, SimulationParams, SimulationTick, TickInputs, WorldState,
    player::{ENGINE_CLIMB_CAP, engine_power},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    tick::TickRate,
};

use super::{
    AIR_TOP_SPEED_FACTOR, AUTHORITATIVE_MAX_CLIMB_SPEED, DriveTuning, GOD_MODE_SPEED_MULTIPLIER,
    GOD_MODE_THRUST_MULTIPLIER, engine_top_speed_tiles,
};

const PLAYER: PlayerId = PlayerId::new(1);
/// Corridor start column.
const START_COLUMN: i32 = 6;
/// The longest run any measurement needs (ticks at 60 Hz).
const RUN_LIMIT_SECONDS: u32 = 60;
/// A move this long (px, one art pixel) is visible on screen.
const FIRST_MOVE_PIXELS: f32 = 2.0;

/// A rig on the corridor: an engine tier, or god mode.
#[derive(Clone, Copy, Debug)]
enum Rig {
    Tier(u8),
    God,
}

impl Rig {
    fn label(self) -> String {
        match self {
            Self::Tier(level) => format!("Tier {level}"),
            Self::God => "God mode".to_owned(),
        }
    }
}

/// Measured feel of one rig on the ground.
#[derive(Clone, Copy, Debug)]
struct Feel {
    /// Top ground speed reached (tiles/s).
    top_speed: f32,
    /// Seconds from rest to 90 % of top speed.
    ramp_seconds: f32,
    /// Seconds from rest to cross 100 tiles.
    cross_100_seconds: f32,
    /// Seconds from rest to half of top speed (how quickly the rig answers the stick).
    response_seconds: f32,
    /// Seconds from rest until the rig has visibly moved (one art pixel).
    first_move_seconds: f32,
    /// Tiles from top speed to a stop, coasting.
    stop_tiles: f32,
    /// Seconds from top speed to a stop, coasting.
    stop_seconds: f32,
    /// Seconds from full speed one way to half speed the other way.
    reverse_seconds: f32,
    /// Pixels a 100 ms tap moves the rig from rest.
    tap_pixels: f32,
}

fn corridor_world(rig: Rig, hz: u32) -> WorldState {
    let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(hz));
    match rig {
        Rig::Tier(level) => world.player_mut(PLAYER).expect("player").engine_level = level,
        Rig::God => world.enable_test_mode(),
    }
    let floor = world
        .player(PLAYER)
        .expect("player")
        .tile_position(TILE_SIZE)
        .y
        + 1;
    for x in 1..world.terrain().width() - 1 {
        for y in floor - 4..=floor {
            let kind = if y == floor {
                TileKind::Dirt
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    let player = world.player_mut(PLAYER).expect("player");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    let (x, y) = (START_COLUMN as f32, floor as f32);
    player.x = TILE_SIZE.mul_add(x, TILE_SIZE * 0.5);
    player.y = TILE_SIZE.mul_add(y, -11.0);
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    world
}

struct Run {
    world: WorldState,
    tick: u64,
    hz: u32,
}

impl Run {
    fn new(rig: Rig, hz: u32) -> Self {
        let mut run = Self {
            world: corridor_world(rig, hz),
            tick: 0,
            hz,
        };
        // Settle onto the floor.
        for _ in 0..hz / 2 {
            run.step(0.0);
        }
        run
    }

    fn step(&mut self, horizontal: f32) {
        let inputs = TickInputs::held(PLAYER, ControlState::new(horizontal, false, false));
        let _ = self
            .world
            .simulate_tick(SimulationTick::new(self.tick), &inputs);
        self.tick += 1;
    }

    fn x(&self) -> f32 {
        self.world.player(PLAYER).expect("player").x
    }

    fn velocity(&self) -> f32 {
        self.world.player(PLAYER).expect("player").velocity_x
    }

    #[allow(clippy::cast_precision_loss, reason = "tick counts become seconds")]
    fn seconds(&self, ticks: u32) -> f32 {
        ticks as f32 / self.hz as f32
    }

    /// Holds `horizontal` until `done` (at most the run limit), returning the ticks taken.
    fn hold_until(&mut self, horizontal: f32, mut done: impl FnMut(&Self) -> bool) -> u32 {
        let mut ticks = 0;
        while !done(self) && ticks < RUN_LIMIT_SECONDS * self.hz {
            self.step(horizontal);
            ticks += 1;
        }
        ticks
    }
}

fn measure(rig: Rig, hz: u32) -> Feel {
    // Top speed: hold right for three seconds on a fresh corridor.
    let mut run = Run::new(rig, hz);
    let _ = run.hold_until(1.0, |run| run.tick >= u64::from(run.hz) * 3);
    let top = run.velocity();

    let mut run = Run::new(rig, hz);
    let start = run.x();
    let (mut first_move, mut response, mut ramp) = (None, None, None);
    let mut ticks = 0;
    let cross = run.hold_until(1.0, |run| {
        let moved = run.x() - start;
        let velocity = run.velocity();
        for (mark, reached) in [
            (&mut first_move, moved >= FIRST_MOVE_PIXELS),
            (&mut response, velocity >= top * 0.5),
            (&mut ramp, velocity >= top * 0.9),
        ] {
            if reached && mark.is_none() {
                *mark = Some(ticks);
            }
        }
        ticks += 1;
        moved >= TILE_SIZE * 100.0
    });
    let mark = |mark: Option<u32>| mark.unwrap_or(cross);
    let (first_move, response, ramp) = (mark(first_move), mark(response), mark(ramp));
    // Reach top speed, then coast to a stop.
    let _ = run.hold_until(1.0, |run| run.velocity() >= top * 0.999);
    let coast_from = run.x();
    let stop = run.hold_until(0.0, |run| run.velocity() <= 0.0);
    let stop_tiles = (run.x() - coast_from) / TILE_SIZE;

    let mut run = Run::new(rig, hz);
    let _ = run.hold_until(1.0, |run| run.velocity() >= top * 0.999);
    let reverse = run.hold_until(-1.0, |run| run.velocity() <= -top * 0.5);

    let mut run = Run::new(rig, hz);
    let start = run.x();
    for _ in 0..hz / 10 {
        run.step(1.0);
    }
    let _ = run.hold_until(0.0, |run| run.velocity() <= 0.0);
    Feel {
        top_speed: top / TILE_SIZE,
        ramp_seconds: run.seconds(ramp),
        cross_100_seconds: run.seconds(cross),
        response_seconds: run.seconds(response),
        first_move_seconds: run.seconds(first_move),
        stop_tiles,
        stop_seconds: run.seconds(stop),
        reverse_seconds: run.seconds(reverse),
        tap_pixels: run.x() - start,
    }
}

/// Top climb speed (tiles/s) of `rig` (stock parts, empty bay).
fn climb(rig: Rig) -> f32 {
    let (level, god) = match rig {
        Rig::Tier(level) => (level, 1.0),
        Rig::God => (15, GOD_MODE_THRUST_MULTIPLIER),
    };
    AUTHORITATIVE_MAX_CLIMB_SPEED * engine_power(level).min(ENGINE_CLIMB_CAP) * god / TILE_SIZE
}

/// Flight (airborne) top speed and coasting stop of `rig`, from the drive model.
#[allow(
    clippy::while_float,
    reason = "runs until the drive brings the rig to rest"
)]
fn flight(rig: Rig) -> (f32, f32) {
    let top = match rig {
        Rig::Tier(level) => engine_top_speed_tiles(level),
        Rig::God => engine_top_speed_tiles(15) * GOD_MODE_SPEED_MULTIPLIER,
    } * TILE_SIZE
        * AIR_TOP_SPEED_FACTOR;
    let tuning = DriveTuning::new(top, false);
    let rate = TickRate::new(60);
    let (mut velocity, mut distance) = (top, 0.0);
    while velocity > 0.0 {
        velocity = tuning.step_tick(velocity, 0.0, rate);
        distance = velocity.mul_add(rate.delta_seconds(), distance);
    }
    (top / TILE_SIZE, distance / TILE_SIZE)
}

const RIGS: [Rig; 8] = [
    Rig::Tier(1),
    Rig::Tier(3),
    Rig::Tier(5),
    Rig::Tier(8),
    Rig::Tier(10),
    Rig::Tier(12),
    Rig::Tier(15),
    Rig::God,
];

#[test]
#[ignore = "report: prints the measured movement-feel table"]
fn feel_report() {
    let mut table = String::from(
        "| Rig | Top speed (tiles/s) | First move (ms) | To 50 % (s) | To 90 % (s) | \
         Cross 100 tiles (s) | Stop from top (tiles / s) | Reverse (s) | 100 ms tap (px) | \
         Flight top (tiles/s) | Flight stop (tiles) | Climb top (tiles/s) |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for rig in RIGS {
        let feel = measure(rig, 60);
        let (flight_speed, flight_stop) = flight(rig);
        let _ = writeln!(
            table,
            "| {} | {:.1} | {:.0} | {:.2} | {:.2} | {:.1} | {:.1} / {:.2} | {:.2} | {:.0} | {:.1} \
             | {:.1} | {:.1} |",
            rig.label(),
            feel.top_speed,
            feel.first_move_seconds * 1000.0,
            feel.response_seconds,
            feel.ramp_seconds,
            feel.cross_100_seconds,
            feel.stop_tiles,
            feel.stop_seconds,
            feel.reverse_seconds,
            feel.tap_pixels,
            flight_speed,
            flight_stop,
            climb(rig),
        );
    }
    println!("{table}");
}

#[test]
fn feel_scales_strongly_with_tier_and_stays_controllable() {
    let feels: Vec<Feel> = RIGS.iter().map(|rig| measure(*rig, 60)).collect();
    for pair in feels.windows(2) {
        assert!(pair[1].top_speed > pair[0].top_speed, "{pair:?}");
        assert!(
            pair[1].cross_100_seconds < pair[0].cross_100_seconds,
            "{pair:?}"
        );
    }
    let (stock, max_tier, god) = (feels[0], feels[6], feels[7]);
    assert!(max_tier.top_speed > stock.top_speed * 4.5, "{max_tier:?}");
    assert!(god.top_speed > max_tier.top_speed * 2.0, "{god:?}");
    for feel in &feels {
        // Every rig moves visibly within ~1/10 s of the stick...
        assert!(feel.first_move_seconds <= 0.1, "{feel:?}");
        // ...stops within a few tiles and well under half a second, and turns quickly...
        assert!(feel.stop_tiles <= 3.5, "{feel:?}");
        assert!(feel.stop_seconds <= 0.45, "{feel:?}");
        assert!(feel.reverse_seconds < 0.6, "{feel:?}");
        // ...and a short tap stays a precise nudge (under a tile), not a lurch.
        assert!(feel.tap_pixels < TILE_SIZE, "{feel:?}");
    }
    // Fast rigs ease in to their speed instead of snapping to it.
    assert!(
        god.ramp_seconds > stock.ramp_seconds * 2.0,
        "{god:?} vs {stock:?}"
    );
    assert!(god.response_seconds > stock.response_seconds, "{god:?}");
}

#[test]
fn god_mode_speed_does_not_depend_on_tick_rate() {
    let reference = measure(Rig::God, 60);
    for hz in [30, 120, 240] {
        let feel = measure(Rig::God, hz);
        let message = format!("{hz} Hz {feel:?} vs 60 Hz {reference:?}");
        assert!(
            (feel.top_speed - reference.top_speed).abs() <= reference.top_speed * 0.01,
            "{message}"
        );
        assert!(
            (feel.cross_100_seconds - reference.cross_100_seconds).abs() <= 1.0 / 30.0 + 0.01,
            "{message}"
        );
        assert!(
            (feel.stop_tiles - reference.stop_tiles).abs() <= 0.25,
            "{message}"
        );
    }
}
