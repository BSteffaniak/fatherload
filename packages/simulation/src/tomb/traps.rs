//! Side-view tomb traps: timing and hazard shapes (WP-TOMBS, T28).
//!
//! Every moving trap is a **pure function of the world clock** (milliseconds of simulated time,
//! [`world_millis`]) and of the trap's tile: no per-trap state, no entities. The simulation
//! (`world/tomb_traps.rs`) and the renderer evaluate the same functions, so a blade is drawn
//! exactly where it hurts, every peer agrees, and the timing is the same at any tick rate. Only
//! basic float arithmetic is used (no `sin`), so desktop and browser builds agree bit for bit.
//!
//! Each trap telegraphs before it hurts and leaves a safe window every cycle:
//!
//! | Trap | Tile | Cycle | Tell | Hazard |
//! |---|---|---|---|---|
//! | [`TrapKind::Blade`] | pivot in a ceiling | [`BLADE_PERIOD_MILLIS`] | always visible, swings | the blade's disc at the end of its arm |
//! | [`TrapKind::Crusher`] | housing in a ceiling | [`CRUSHER_PERIOD_MILLIS`] | shakes and sheds dust for [`CRUSHER_SHAKE_MILLIS`] | the block from the housing down to its reach |
//! | [`TrapKind::DartWall`] | thrower in a side wall | [`DART_PERIOD_MILLIS`] | its eye glows for [`DART_CHARGE_MILLIS`] | the dart flying across the room |
//! | [`TrapKind::SandSluice`] | grate in a sunken floor | [`SAND_PERIOD_MILLIS`] | a trickle for [`SAND_TELL_MILLIS`] | the room filling to [`SAND_MAX_ROWS`] rows |
//! | [`TrapKind::FogVent`] | vent in a floor | [`FOG_PERIOD_MILLIS`] | wisps gather for [`FOG_GATHER_MILLIS`] | a cloud of [`FOG_RADIUS`] |
//! | [`TrapKind::Crumble`] | the floor itself | stepped on | cracks and shakes for [`CRUMBLE_MILLIS`] | falls away, sets again after [`REFORM_MILLIS`] |
//!
//! Damage rates are per second of contact before relics (the Trap-Sight Lantern, Sandwalker
//! sandals) and the enemy difficulty.

#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    reason = "tile coordinates and milliseconds are small; trap shapes are plain arithmetic"
)]

use crate::{
    position::Frame,
    terrain::{TILE_SIZE, TilePosition},
    worldgen::{features::TileRect, noise::hash2},
};

pub use super::realize::TrapKind;

const SALT_PHASE: u64 = 0x70B5_7A95_0028;

/// Milliseconds of simulated time at `tick` (tick-rate safe: the same wall time gives the
/// same value at any rate).
#[must_use]
pub const fn world_millis(tick: u64, hz: u32) -> u64 {
    if hz == 0 {
        return 0;
    }
    tick.saturating_mul(1_000) / hz as u64
}

/// `sin(2 pi turns)` from basic arithmetic only (a Taylor series on the folded quarter wave;
/// error under 1e-5), so every platform agrees.
#[must_use]
pub fn sin_turns(turns: f32) -> f32 {
    let mut u = turns - turns.floor();
    if u > 0.5 {
        u -= 1.0;
    }
    // x in [-pi, pi]; fold to [-pi/2, pi/2].
    let mut x = u * std::f32::consts::TAU;
    if x > std::f32::consts::FRAC_PI_2 {
        x = std::f32::consts::PI - x;
    } else if x < -std::f32::consts::FRAC_PI_2 {
        x = -std::f32::consts::PI - x;
    }
    let x2 = x * x;
    x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0))))
}

/// `cos(angle)` for small angles (|angle| below about 1.3 rad), basic arithmetic only.
#[must_use]
pub fn cos_small(angle: f32) -> f32 {
    let x2 = angle * angle;
    1.0 - x2 / 2.0 * (1.0 - x2 / 12.0 * (1.0 - x2 / 30.0 * (1.0 - x2 / 56.0)))
}

/// `sin(angle)` for small angles, basic arithmetic only.
#[must_use]
pub fn sin_small(angle: f32) -> f32 {
    let x2 = angle * angle;
    angle * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0))))
}

/// Where in its cycle a trap at `tile` is: a per-tile offset so a hall's traps fall out of step.
const fn hashed_phase(tile: TilePosition, period: u64) -> u64 {
    hash2(0, SALT_PHASE, tile.x, tile.y) % period
}

/// Milliseconds into a cycle of `period` for a trap whose offset is `phase`.
const fn into_cycle(millis: u64, phase: u64, period: u64) -> u64 {
    (millis + phase) % period
}

/// An axis-aligned box in world pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Bounds {
    #[must_use]
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.left < other.right
            && other.left < self.right
            && self.top < other.bottom
            && other.top < self.bottom
    }

    /// Whether a disc at `(x, y)` of `radius` touches the box.
    #[must_use]
    pub fn touches_disc(&self, (x, y): (f32, f32), radius: f32) -> bool {
        let nx = x.clamp(self.left, self.right);
        let ny = y.clamp(self.top, self.bottom);
        let (dx, dy) = (x - nx, y - ny);
        dx * dx + dy * dy <= radius * radius
    }
}

// --- Swinging blades -----------------------------------------------------------------------

/// A blade's full swing (there and back), milliseconds.
pub const BLADE_PERIOD_MILLIS: u64 = 2_600;
/// The arm from the pivot to the blade's centre, pixels.
pub const BLADE_LENGTH: f32 = TILE_SIZE * 3.8;
/// The widest swing either side of straight down, radians (about 60 degrees).
pub const BLADE_SWING: f32 = 1.05;
/// The blade's cutting disc, pixels.
pub const BLADE_RADIUS: f32 = 13.0;
/// Damage per second of contact.
pub const BLADE_DPS: f32 = 150.0;
/// Rows of open room below a blade's pivot the realizer leaves (the arm reaches pilot height).
pub const BLADE_ROOM_ROWS: i32 = 5;

/// Where a blade hangs from: the bottom middle of its pivot tile.
#[must_use]
pub fn blade_pivot(frame: Frame, tile: TilePosition) -> (f32, f32) {
    (
        (frame.rel_col(tile.x) + 0.5) * TILE_SIZE,
        frame.rel_row(tile.y + 1) * TILE_SIZE,
    )
}

/// The blade's angle from straight down at `millis` (radians, positive swings right).
#[must_use]
pub fn blade_angle(tile: TilePosition, millis: u64) -> f32 {
    let at = into_cycle(
        millis,
        hashed_phase(tile, BLADE_PERIOD_MILLIS),
        BLADE_PERIOD_MILLIS,
    );
    BLADE_SWING * sin_turns(at as f32 / BLADE_PERIOD_MILLIS as f32)
}

/// The blade's centre at `millis`.
#[must_use]
pub fn blade_tip(frame: Frame, tile: TilePosition, millis: u64) -> (f32, f32) {
    let (px, py) = blade_pivot(frame, tile);
    let angle = blade_angle(tile, millis);
    (
        px + BLADE_LENGTH * sin_small(angle),
        py + BLADE_LENGTH * cos_small(angle),
    )
}

// --- Crushers ------------------------------------------------------------------------------

/// A crusher's cycle, milliseconds.
pub const CRUSHER_PERIOD_MILLIS: u64 = 3_400;
/// It rests up for this long...
pub const CRUSHER_REST_MILLIS: u64 = 1_500;
/// ...shakes (the tell) for this long...
pub const CRUSHER_SHAKE_MILLIS: u64 = 700;
/// ...slams down in this long...
pub const CRUSHER_SLAM_MILLIS: u64 = 150;
/// ...holds down this long, then winds back up over the rest of the cycle.
pub const CRUSHER_HOLD_MILLIS: u64 = 550;
/// Stagger between neighbouring crushers (by column), milliseconds: a hall's crushers fall in
/// a wave the pilot can read.
pub const CRUSHER_STAGGER_MILLIS: u64 = 480;
/// Damage per second of contact with the block.
pub const CRUSHER_DPS: f32 = 140.0;
/// The most rows a crusher's block reaches.
pub const CRUSHER_MAX_REACH: i32 = 8;

/// What a crusher is doing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrusherStage {
    Resting,
    /// The tell: about to fall.
    Shaking,
    Slamming,
    Down,
    Rising,
}

/// A crusher's stage and how far its block is down (0 up in its housing, 1 on the floor).
#[must_use]
pub fn crusher_state(tile: TilePosition, millis: u64) -> (CrusherStage, f32) {
    let phase = (tile.x.rem_euclid(16) as u64) * CRUSHER_STAGGER_MILLIS;
    let at = into_cycle(millis, phase, CRUSHER_PERIOD_MILLIS);
    let shake = CRUSHER_REST_MILLIS;
    let slam = shake + CRUSHER_SHAKE_MILLIS;
    let down = slam + CRUSHER_SLAM_MILLIS;
    let rise = down + CRUSHER_HOLD_MILLIS;
    if at < shake {
        (CrusherStage::Resting, 0.0)
    } else if at < slam {
        (CrusherStage::Shaking, 0.0)
    } else if at < down {
        (
            CrusherStage::Slamming,
            (at - slam) as f32 / CRUSHER_SLAM_MILLIS as f32,
        )
    } else if at < rise {
        (CrusherStage::Down, 1.0)
    } else {
        let span = (CRUSHER_PERIOD_MILLIS - rise) as f32;
        (CrusherStage::Rising, 1.0 - (at - rise) as f32 / span)
    }
}

/// The crusher block's box at extension `extension` for a housing at `tile` whose block falls
/// `reach` rows.
#[must_use]
pub fn crusher_block(
    frame: Frame,
    tile: TilePosition,
    reach: i32,
    extension: f32,
) -> Option<Bounds> {
    if extension <= 0.0 || reach <= 0 {
        return None;
    }
    let left = frame.rel_col(tile.x) * TILE_SIZE + 2.0;
    let top = frame.rel_row(tile.y + 1) * TILE_SIZE;
    let bottom = top + reach as f32 * TILE_SIZE * extension.min(1.0);
    Some(Bounds::new(left, top, left + TILE_SIZE - 4.0, bottom))
}

// --- Dart walls ----------------------------------------------------------------------------

/// A dart thrower's cycle, milliseconds.
pub const DART_PERIOD_MILLIS: u64 = 2_200;
/// Its eye glows this long before the dart flies.
pub const DART_CHARGE_MILLIS: u64 = 750;
/// A dart flies this fast, pixels per second.
pub const DART_SPEED: f32 = 380.0;
/// Half the dart's size, pixels (it is long and thin).
pub const DART_HALF: (f32, f32) = (8.0, 3.0);
/// Damage per second of contact.
pub const DART_DPS: f32 = 170.0;
/// The farthest a dart flies, tiles.
pub const DART_MAX_REACH: i32 = 16;

/// A dart thrower's state at `millis`: how charged its eye is (0..1, 0 while a dart flies) and
/// how far the current dart has flown from the thrower's face (pixels), if one is flying.
#[must_use]
pub fn dart_state(tile: TilePosition, millis: u64) -> (f32, Option<f32>) {
    let at = into_cycle(
        millis,
        hashed_phase(tile, DART_PERIOD_MILLIS),
        DART_PERIOD_MILLIS,
    );
    let launch = DART_PERIOD_MILLIS - DART_CHARGE_MILLIS;
    if at >= launch {
        ((at - launch) as f32 / DART_CHARGE_MILLIS as f32, None)
    } else {
        (0.0, Some(at as f32 / 1_000.0 * DART_SPEED))
    }
}

/// The flying dart's centre for a thrower at `tile` facing `facing` (-1 left, 1 right), whose
/// darts fly `reach` tiles before they hit a wall.
#[must_use]
pub fn dart_position(
    frame: Frame,
    tile: TilePosition,
    facing: i8,
    reach: i32,
    millis: u64,
) -> Option<(f32, f32)> {
    let (_, flown) = dart_state(tile, millis);
    let flown = flown?;
    if flown > reach as f32 * TILE_SIZE {
        return None;
    }
    let face = if facing < 0 {
        frame.rel_col(tile.x) * TILE_SIZE
    } else {
        frame.rel_col(tile.x + 1) * TILE_SIZE
    };
    Some((
        face + f32::from(facing) * flown,
        (frame.rel_row(tile.y) + 0.5) * TILE_SIZE,
    ))
}

/// The dart's box.
#[must_use]
pub fn dart_bounds((x, y): (f32, f32)) -> Bounds {
    Bounds::new(
        x - DART_HALF.0,
        y - DART_HALF.1,
        x + DART_HALF.0,
        y + DART_HALF.1,
    )
}

// --- Rising sand ---------------------------------------------------------------------------

/// A sluice room's cycle, milliseconds.
pub const SAND_PERIOD_MILLIS: u64 = 15_000;
/// Dry floor for this long (the trickle starts [`SAND_TELL_MILLIS`] before it ends)...
pub const SAND_DRY_MILLIS: u64 = 5_000;
/// The trickle before the pour.
pub const SAND_TELL_MILLIS: u64 = 1_800;
/// ...rising for this long...
pub const SAND_RISE_MILLIS: u64 = 3_500;
/// ...full for this long...
pub const SAND_FULL_MILLIS: u64 = 2_500;
/// ...draining for this long, then dry to the end of the cycle.
pub const SAND_DRAIN_MILLIS: u64 = 2_500;
/// Rows of sand (or water) at full, measured from the room's floor.
pub const SAND_MAX_ROWS: f32 = 2.1;
/// A pilot whose feet are this far under the surface is hurt (pixels).
pub const SAND_WADE_PIXELS: f32 = 8.0;
/// Damage per second while under.
pub const SAND_DPS: f32 = 30.0;

/// What a sluice room is doing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandStage {
    Dry,
    /// The tell: a trickle from the grates.
    Trickling,
    Rising,
    Full,
    Draining,
}

/// A sluice room's stage and level (rows above its floor) at `millis`; the room's interior
/// sets the phase, so every grate of a room pours together.
#[must_use]
pub fn sand_state(room: TileRect, millis: u64) -> (SandStage, f32) {
    let at = into_cycle(
        millis,
        hashed_phase(room.min, SAND_PERIOD_MILLIS),
        SAND_PERIOD_MILLIS,
    );
    let rise = SAND_DRY_MILLIS;
    let full = rise + SAND_RISE_MILLIS;
    let drain = full + SAND_FULL_MILLIS;
    let dry = drain + SAND_DRAIN_MILLIS;
    if at < rise - SAND_TELL_MILLIS {
        (SandStage::Dry, 0.0)
    } else if at < rise {
        (SandStage::Trickling, 0.0)
    } else if at < full {
        (
            SandStage::Rising,
            SAND_MAX_ROWS * (at - rise) as f32 / SAND_RISE_MILLIS as f32,
        )
    } else if at < drain {
        (SandStage::Full, SAND_MAX_ROWS)
    } else if at < dry {
        (
            SandStage::Draining,
            SAND_MAX_ROWS * (1.0 - (at - drain) as f32 / SAND_DRAIN_MILLIS as f32),
        )
    } else {
        (SandStage::Dry, 0.0)
    }
}

/// The surface of a sluice room's sand at `level` rows (world pixel row).
#[must_use]
pub fn sand_surface(frame: Frame, room: TileRect, level: f32) -> f32 {
    frame.rel_row(room.max.y + 1) * TILE_SIZE - level * TILE_SIZE
}

/// Milliseconds until a sluice room's sand next starts to rise (0 while it is up).
#[must_use]
pub const fn sand_clear_for(room: TileRect, millis: u64) -> u64 {
    let at = into_cycle(
        millis,
        hashed_phase(room.min, SAND_PERIOD_MILLIS),
        SAND_PERIOD_MILLIS,
    );
    let drained = SAND_DRY_MILLIS + SAND_RISE_MILLIS + SAND_FULL_MILLIS + SAND_DRAIN_MILLIS;
    if at < SAND_DRY_MILLIS {
        SAND_DRY_MILLIS - at
    } else if at >= drained {
        SAND_PERIOD_MILLIS - at + SAND_DRY_MILLIS
    } else {
        0
    }
}

// --- Cursed fog ----------------------------------------------------------------------------

/// A vent's cycle, milliseconds.
pub const FOG_PERIOD_MILLIS: u64 = 4_200;
/// Calm for this long...
pub const FOG_CALM_MILLIS: u64 = 2_000;
/// ...wisps gather (the tell) for this long...
pub const FOG_GATHER_MILLIS: u64 = 900;
/// ...the cloud bursts and hangs for this long, then fades.
pub const FOG_BURST_MILLIS: u64 = 1_100;
/// Stagger between neighbouring vents (by column), milliseconds.
pub const FOG_STAGGER_MILLIS: u64 = 700;
/// The cloud's radius, pixels (vents five apart leave a pilot room between two clouds).
pub const FOG_RADIUS: f32 = TILE_SIZE * 1.8;
/// Damage per second inside the cloud.
pub const FOG_DPS: f32 = 40.0;

/// What a vent is doing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FogStage {
    Calm,
    Gathering,
    Burst,
    Fading,
}

/// A vent's stage and its cloud's size (0..1 of [`FOG_RADIUS`]) at `millis`.
#[must_use]
pub fn fog_state(tile: TilePosition, millis: u64) -> (FogStage, f32) {
    let phase = (tile.x.rem_euclid(12) as u64) * FOG_STAGGER_MILLIS;
    let at = into_cycle(millis, phase, FOG_PERIOD_MILLIS);
    let gather = FOG_CALM_MILLIS;
    let burst = gather + FOG_GATHER_MILLIS;
    let fade = burst + FOG_BURST_MILLIS;
    if at < gather {
        (FogStage::Calm, 0.0)
    } else if at < burst {
        (
            FogStage::Gathering,
            0.25 * (at - gather) as f32 / FOG_GATHER_MILLIS as f32,
        )
    } else if at < fade {
        // The cloud swells to full in a fifth of a second.
        (FogStage::Burst, ((at - burst) as f32 / 200.0).min(1.0))
    } else {
        (
            FogStage::Fading,
            1.0 - (at - fade) as f32 / (FOG_PERIOD_MILLIS - fade) as f32,
        )
    }
}

/// The middle of a vent's cloud.
#[must_use]
pub fn fog_centre(frame: Frame, tile: TilePosition) -> (f32, f32) {
    (
        (frame.rel_col(tile.x) + 0.5) * TILE_SIZE,
        frame.rel_row(tile.y) * TILE_SIZE - TILE_SIZE * 1.2,
    )
}

/// Whether a vent's cloud hurts at `millis` (the burst only: gathering wisps and the fade are
/// the tells).
#[must_use]
pub fn fog_hurts(tile: TilePosition, millis: u64) -> Option<f32> {
    match fog_state(tile, millis) {
        (FogStage::Burst, size) => Some(FOG_RADIUS * size),
        _ => None,
    }
}

// --- Crumbling floors ----------------------------------------------------------------------

/// A crumbling slab falls this long after a pilot steps on it.
pub const CRUMBLE_MILLIS: u64 = 450;
/// A fallen slab sets again after this long (when nobody stands in its place).
pub const REFORM_MILLIS: u64 = 5_000;

/// One crumbling (or fallen) slab of a run's floor, kept in [`super::run::TombRun::crumbling`].
#[derive(
    Clone, Copy, Debug, serde::Deserialize, Eq, Ord, PartialEq, PartialOrd, serde::Serialize,
)]
pub struct CrumblingSlab {
    /// The slab's tile (absolute).
    pub tile: TilePosition,
    /// When a pilot first stepped on it (world milliseconds).
    pub stepped: u64,
    /// When it fell (world milliseconds), once it has.
    pub fell: Option<u64>,
    /// It was hidden (it sets again hidden).
    pub hidden: bool,
}

impl CrumblingSlab {
    /// How far it is through its crumble (0..1) at `millis`, before it falls.
    #[must_use]
    pub fn shake(&self, millis: u64) -> f32 {
        if self.fell.is_some() {
            return 1.0;
        }
        (millis.saturating_sub(self.stepped) as f32 / CRUMBLE_MILLIS as f32).min(1.0)
    }
}

/// Whether the trap kind moves on the clock (its hazard is not its own tile).
#[must_use]
pub const fn timed(kind: TrapKind) -> bool {
    matches!(
        kind,
        TrapKind::Blade
            | TrapKind::Crusher
            | TrapKind::DartWall
            | TrapKind::SandSluice
            | TrapKind::FogVent
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_series_match_the_library_closely() {
        for step in 0..200 {
            let turns = step as f32 / 97.0 - 0.7;
            let expected = (turns * std::f32::consts::TAU).sin();
            assert!((sin_turns(turns) - expected).abs() < 1e-4, "{turns}");
        }
        for step in -20..=20 {
            let angle = step as f32 * 0.06;
            assert!((cos_small(angle) - angle.cos()).abs() < 1e-4);
            assert!((sin_small(angle) - angle.sin()).abs() < 1e-4);
        }
    }

    #[test]
    fn timings_are_tick_rate_safe() {
        // The same wall time reads the same at 30, 60 and 120 Hz.
        for seconds in [0_u64, 3, 17, 451] {
            let a = world_millis(seconds * 30, 30);
            let b = world_millis(seconds * 60, 60);
            let c = world_millis(seconds * 120, 120);
            assert_eq!(a, b);
            assert_eq!(b, c);
        }
    }

    #[test]
    fn every_moving_trap_leaves_a_safe_window_and_telegraphs() {
        let tile = TilePosition { x: -14_990, y: 200 };
        let mut crusher_safe = 0;
        let mut crusher_shake_before_slam = true;
        let mut last = CrusherStage::Resting;
        for millis in (0..CRUSHER_PERIOD_MILLIS * 2).step_by(10) {
            let (stage, extension) = crusher_state(tile, millis);
            if extension <= 0.0 {
                crusher_safe += 1;
            }
            if stage == CrusherStage::Slamming && last == CrusherStage::Resting {
                crusher_shake_before_slam = false;
            }
            last = stage;
        }
        assert!(crusher_shake_before_slam, "a crusher always shakes first");
        assert!(crusher_safe > 300, "{crusher_safe}");
        // A blade swings low only for part of its cycle.
        let low = (0..BLADE_PERIOD_MILLIS)
            .step_by(10)
            .filter(|millis| blade_angle(tile, *millis).abs() < 0.35)
            .count();
        assert!(low > 20 && low < 200, "{low}");
        // Darts glow before they fly.
        let (charge, flying) = dart_state(tile, 0);
        assert!(charge > 0.0 || flying.is_some());
        // The sand is down most of the time.
        let room = TileRect {
            min: tile,
            max: TilePosition {
                x: tile.x + 10,
                y: tile.y + 6,
            },
        };
        let dry = (0..SAND_PERIOD_MILLIS)
            .step_by(50)
            .filter(|millis| sand_state(room, *millis).1 <= 0.0)
            .count();
        assert!(dry * 50 >= 6_000, "{dry}");
        let hurting = (0..FOG_PERIOD_MILLIS)
            .step_by(10)
            .filter(|millis| fog_hurts(tile, *millis).is_some())
            .count();
        assert!(hurting > 50 && hurting < 200, "{hurting}");
    }

    #[test]
    fn a_blade_clears_a_standing_pilot_at_the_top_of_its_swing() {
        // The pivot sits in the ceiling five rows above the floor: straight down the blade is
        // at pilot height, at the end of its swing it is above a standing pilot's head.
        let pivot = TilePosition { x: 10, y: 0 };
        let floor = (1 + BLADE_ROOM_ROWS) as f32 * TILE_SIZE;
        let body_at = |x: f32| Bounds::new(x - 12.5, floor - 50.0, x + 12.5, floor);
        let (px, _) = blade_pivot(Frame::FIXED, pivot);
        let low = (px, blade_pivot(Frame::FIXED, pivot).1 + BLADE_LENGTH);
        assert!(body_at(px).touches_disc(low, BLADE_RADIUS));
        let high = (
            px + BLADE_LENGTH * sin_small(BLADE_SWING),
            blade_pivot(Frame::FIXED, pivot).1 + BLADE_LENGTH * cos_small(BLADE_SWING),
        );
        assert!(!body_at(high.0).touches_disc(high, BLADE_RADIUS));
    }
}
