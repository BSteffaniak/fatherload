//! The eased drive model: how fast a rig moves along one axis, how it spools up to that speed
//! and how hard it brakes, per engine tier (WP-FEEL).
//!
//! Every engine tier raises the rig's top speed: the first four upgrades by about two thirds of
//! a tile a second each (never slower than the pre-retune engine curve), every later one by
//! about 9 % (about five times stock at the top tier), and test mode ("god mode") more than doubles the top tier again. Fast rigs do
//! not snap to speed: acceleration starts near the stock rig's (so a tap still nudges the rig a
//! precise few pixels), spools up as the rig gathers speed (ease-in) and tapers off just below
//! top speed (ease-out). Braking grows with speed (a drag term plus a constant stop term), and
//! steering against the motion brakes harder still, so a fast rig stops and turns in a few tiles
//! instead of skating.
//!
//! The model is integrated at a fixed internal step of at most [`DRIVE_STEP_SECONDS`]
//! regardless of the session tick rate, so speeds and distances agree across tick rates (for
//! rates that divide 240 Hz the velocities are bit-identical at tick boundaries). Only basic
//! arithmetic is used, so every platform computes the same bits.

use crate::{terrain::TILE_SIZE, tick::TickRate};

/// Ground top speed (tiles per second) of a stock rig with an empty bay at each engine tier
/// (index 0 is tier 1), at full stick.
///
/// Every tier is at least as fast as the same tier was before the WP-FEEL retune (stock thrust
/// times `engine_power`, capped at 2.4x), so no existing rig gets slower: the early tiers keep
/// their big steps and the later ones, which used to hit the cap at tier 7, keep climbing.
pub const ENGINE_TOP_SPEED_TILES: [f32; 15] = [
    2.25, 2.9, 3.55, 4.2, 4.8, 5.25, 5.7, 6.2, 6.8, 7.4, 8.05, 8.8, 9.6, 10.45, 11.4,
];
/// Test mode ("god mode") multiplies the rig's horizontal top speed by this.
pub const GOD_MODE_SPEED_MULTIPLIER: f32 = 2.2;
/// Test mode multiplies the rig's thrust and its climb-speed cap by this.
pub const GOD_MODE_THRUST_MULTIPLIER: f32 = 1.5;
/// Airborne top speed relative to the ground top speed.
pub const AIR_TOP_SPEED_FACTOR: f32 = 0.85;
/// Longest internal integration step of the drive model (240 Hz).
pub const DRIVE_STEP_SECONDS: f32 = 1.0 / 240.0;
/// Internal steps per second of the drive model.
const DRIVE_STEPS_PER_SECOND: u32 = 240;

/// Top speed (px/s) below which the drive behaves like the original snappy stock rig.
const SLOW_TOP_SPEED: f32 = 2.25 * TILE_SIZE;
/// Top speed (px/s) at which the drive reaches its smoothest, most eased curve (god mode).
const FAST_TOP_SPEED: f32 = 25.0 * TILE_SIZE;
/// Fraction of top speed over which the spool-up eases in.
const SPOOL_SPAN: f32 = 0.5;
/// Fraction of top speed below the target over which acceleration tapers off.
const TAPER_SPAN: f32 = 0.2;
/// Peak acceleration relative to `top / ramp_seconds`.
const ACCELERATION_SCALE: f32 = 1.6;

/// Ground top speed (tiles per second) of an engine at `level`, at full stick, before rig
/// parts, cargo load and god mode.
#[must_use]
pub fn engine_top_speed_tiles(level: u8) -> f32 {
    let index = usize::from(level.max(1) - 1).min(ENGINE_TOP_SPEED_TILES.len() - 1);
    ENGINE_TOP_SPEED_TILES[index]
}

/// The drive tuning for a rig whose full-stick top speed is `top` px/s in this medium.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriveTuning {
    /// Top speed at full stick (px/s).
    pub top: f32,
    /// Peak acceleration (px/s²), reached at [`SPOOL_SPAN`] of top speed.
    pub peak_acceleration: f32,
    /// Acceleration from rest as a fraction of the peak (lower = more eased-in).
    pub ease_floor: f32,
    /// Braking proportional to speed while coasting (per second).
    pub coast_rate: f32,
    /// Constant braking while coasting (px/s²), so a rig comes to a clean stop.
    pub stop_deceleration: f32,
    /// Braking proportional to speed while steering against the motion (per second).
    pub turn_rate: f32,
}

impl DriveTuning {
    /// Tuning for a rig with full-stick top speed `top` (px/s), on the ground or in the air.
    #[must_use]
    pub fn new(top: f32, grounded: bool) -> Self {
        let top = top.max(1.0);
        let fast = ((top - SLOW_TOP_SPEED) / (FAST_TOP_SPEED - SLOW_TOP_SPEED)).clamp(0.0, 1.0);
        let lerp = |slow: f32, quick: f32| (quick - slow).mul_add(fast, slow);
        // Seconds to reach about 90 % of top speed, and to stop from top speed.
        let (ramp, stop) = if grounded {
            (lerp(0.11, 0.6), lerp(0.12, 0.36))
        } else {
            (lerp(0.14, 0.7), lerp(0.16, 0.48))
        };
        let coast_rate = 2.2 / stop;
        Self {
            top,
            peak_acceleration: ACCELERATION_SCALE * top / ramp,
            ease_floor: lerp(0.9, 0.4),
            coast_rate,
            stop_deceleration: 0.35 * top / stop,
            turn_rate: coast_rate * 1.5,
        }
    }

    /// This tuning in thin air (a V8 sky, see [`crate::environment`]): the air's braking
    /// (coasting drag and the stop term) scales with the air `density`, and the thrusters'
    /// push (acceleration and steering brake) with the rig's thrust `efficiency`. In vacuum a
    /// coasting rig drifts on without slowing.
    #[must_use]
    pub fn in_thin_air(self, density: f32, efficiency: f32) -> Self {
        Self {
            peak_acceleration: self.peak_acceleration * efficiency,
            coast_rate: self.coast_rate * density,
            stop_deceleration: self.stop_deceleration * density,
            turn_rate: self.turn_rate * efficiency,
            ..self
        }
    }

    /// Acceleration (px/s²) toward a target speed `target` at current speed `speed` (both
    /// non-negative, `speed < target`).
    fn acceleration(&self, speed: f32, target: f32) -> f32 {
        let s = (speed / (self.top * SPOOL_SPAN)).clamp(0.0, 1.0);
        let spool =
            (1.0 - self.ease_floor).mul_add(s * s * 2.0f32.mul_add(-s, 3.0), self.ease_floor);
        let taper = ((target - speed) / (self.top * TAPER_SPAN)).clamp(0.0, 1.0);
        self.peak_acceleration * spool * taper
    }

    /// Advances velocity `velocity` by `seconds` with stick `input` (`-1..=1`).
    #[must_use]
    pub fn step(&self, velocity: f32, input: f32, seconds: f32) -> f32 {
        let input = input.clamp(-1.0, 1.0);
        let speed = velocity.abs();
        if input.abs() <= f32::EPSILON {
            // Coasting: drag plus a constant stop term, never past zero.
            let brake = self.coast_rate.mul_add(speed, self.stop_deceleration) * seconds;
            return velocity.signum() * (speed - brake).max(0.0);
        }
        let direction = input.signum();
        if velocity * direction < 0.0 {
            // Steering against the motion: brake hard, then the drive takes over from rest.
            let brake = self.peak_acceleration.mul_add(
                self.ease_floor,
                self.turn_rate.mul_add(speed, self.stop_deceleration),
            ) * seconds;
            return velocity.signum() * (speed - brake).max(0.0);
        }
        let target = input.abs() * self.top;
        if speed < target {
            let next = self.acceleration(speed, target).mul_add(seconds, speed);
            direction * next.min(target)
        } else {
            // Faster than the stick asks for (eased off, or flung by a blast): coast down to it.
            let excess = speed - target;
            let brake = self.coast_rate.mul_add(excess, self.stop_deceleration) * seconds;
            direction * (speed - brake.min(excess))
        }
    }

    /// This tuning with its coasting and stop braking scaled by `scale` (a field's ground
    /// friction or air drag, see [`crate::field`]): 0 never slows a coasting rig.
    #[must_use]
    pub fn with_braking(self, scale: f32) -> Self {
        Self {
            coast_rate: self.coast_rate * scale,
            stop_deceleration: self.stop_deceleration * scale,
            ..self
        }
    }

    /// Advances `velocity` by `seconds` (any length: a time-scaled tick) with `input` held,
    /// in internal steps of at most [`DRIVE_STEP_SECONDS`].
    #[must_use]
    pub fn step_seconds(&self, velocity: f32, input: f32, seconds: f32) -> f32 {
        if seconds <= 0.0 {
            return velocity;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a bounded handful of internal steps"
        )]
        let steps = ((seconds / DRIVE_STEP_SECONDS).ceil() as u32).clamp(1, 256);
        #[allow(clippy::cast_precision_loss, reason = "a handful of internal steps")]
        let step = seconds / steps as f32;
        (0..steps).fold(velocity, |velocity, _| self.step(velocity, input, step))
    }

    /// Advances `velocity` over one tick of `rate` with `input` held, in fixed internal steps.
    #[must_use]
    pub fn step_tick(&self, velocity: f32, input: f32, rate: TickRate) -> f32 {
        let steps = drive_steps_per_tick(rate);
        #[allow(
            clippy::cast_precision_loss,
            reason = "a handful of internal steps per tick"
        )]
        let seconds = rate.delta_seconds() / steps as f32;
        (0..steps).fold(velocity, |velocity, _| self.step(velocity, input, seconds))
    }
}

/// Internal drive steps per tick at `rate` (so each step is at most [`DRIVE_STEP_SECONDS`]).
#[must_use]
pub const fn drive_steps_per_tick(rate: TickRate) -> u32 {
    DRIVE_STEPS_PER_SECOND.div_ceil(rate.hz())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_tier_is_faster_and_the_top_is_about_five_times_stock() {
        for pair in ENGINE_TOP_SPEED_TILES.windows(2) {
            assert!(pair[1] > pair[0] * 1.08, "{pair:?}");
        }
        assert!(engine_top_speed_tiles(15) >= engine_top_speed_tiles(1) * 4.5);
        assert!((engine_top_speed_tiles(0) - engine_top_speed_tiles(1)).abs() < f32::EPSILON);
        assert!((engine_top_speed_tiles(200) - engine_top_speed_tiles(15)).abs() < f32::EPSILON);
    }

    #[test]
    fn no_engine_tier_is_slower_than_before_the_retune() {
        use crate::player::{ENGINE_HANDLING_CAP, engine_power};
        use crate::world::movement::{
            AUTHORITATIVE_GROUND_DRAG_RATE_PER_SECOND, AUTHORITATIVE_HORIZONTAL_ACCELERATION,
        };
        // The old model settled at (ground acceleration / ground drag) x capped engine power.
        let stock = AUTHORITATIVE_HORIZONTAL_ACCELERATION * 1.35
            / AUTHORITATIVE_GROUND_DRAG_RATE_PER_SECOND
            / TILE_SIZE;
        for level in 1..=15 {
            let old = stock * engine_power(level).min(ENGINE_HANDLING_CAP);
            assert!(
                engine_top_speed_tiles(level) >= old * 0.99,
                "tier {level}: {} < {old}",
                engine_top_speed_tiles(level)
            );
        }
    }

    #[test]
    fn drive_eases_in_then_settles_at_top_speed_without_overshoot() {
        let rate = TickRate::new(60);
        let tuning = DriveTuning::new(FAST_TOP_SPEED, true);
        let mut velocity = 0.0;
        let mut previous_gain = 0.0;
        let mut eased_in = false;
        for _ in 0..240 {
            let next = tuning.step_tick(velocity, 1.0, rate);
            let gain = next - velocity;
            if velocity > 0.0 && gain > previous_gain * 1.01 {
                eased_in = true;
            }
            previous_gain = gain;
            velocity = next;
            assert!(velocity <= tuning.top);
        }
        assert!(eased_in, "acceleration should build as the rig spools up");
        assert!(velocity > tuning.top * 0.99);
    }

    #[test]
    #[allow(
        clippy::while_float,
        reason = "runs until the drive brings the rig to rest"
    )]
    fn coasting_and_turning_stop_the_rig_and_never_reverse_it() {
        let rate = TickRate::new(60);
        let tuning = DriveTuning::new(FAST_TOP_SPEED, true);
        let mut coast_speed = tuning.top;
        let mut turn_speed = tuning.top;
        let (mut coast_ticks, mut turn_ticks) = (0, 0);
        while coast_speed > 0.0 {
            coast_speed = tuning.step_tick(coast_speed, 0.0, rate);
            coast_ticks += 1;
        }
        while turn_speed > 0.0 {
            turn_speed = tuning.step_tick(turn_speed, -1.0, rate);
            turn_ticks += 1;
        }
        assert!(coast_speed.abs() < f32::EPSILON);
        assert!(turn_ticks < coast_ticks, "{turn_ticks} vs {coast_ticks}");
        assert!(
            coast_ticks < 40,
            "a god-mode rig stops within ~2/3 s: {coast_ticks}"
        );
    }

    #[test]
    fn velocities_agree_across_tick_rates_that_divide_the_internal_step() {
        let tuning = DriveTuning::new(engine_top_speed_tiles(15) * TILE_SIZE, false);
        let run = |hz: u32| {
            let rate = TickRate::new(hz);
            let mut velocity = 0.0;
            for _ in 0..hz / 2 {
                velocity = tuning.step_tick(velocity, 1.0, rate);
            }
            velocity
        };
        let reference = run(60);
        for hz in [30, 120, 240] {
            assert!((run(hz) - reference).abs() <= reference * 1e-4, "{hz} Hz");
        }
    }
}
