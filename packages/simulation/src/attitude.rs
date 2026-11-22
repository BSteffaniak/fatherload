//! Craft rotation (expansion plan F9, §18 decision 1): a rig's orientation, angular velocity
//! and rotation thrusters.
//!
//! The **player** rotates the rig with its rotation thrusters ([`crate::ControlState`]'s
//! rotate axis); nothing tilts it automatically. Rotation has momentum: the thrusters spin
//! the rig up against a light angular drag, so a released rig keeps turning and slows down.
//!
//! * **Limits.** On the home world a flying rig may lean [`BASE_TILT_LIMIT`] (about 35
//!   degrees) either way from "level" (the local gravity's frame), more with the tilt rating
//!   upgrade hook ([`tilt_limit`]). Where a field allows it ([`crate::field::FieldSample::
//!   rotation_unlimited`]: gravity shear, upside-down rooms, weightlessness, space later) it
//!   turns all the way round. A rig carried out of such a field beyond its limit eases back
//!   inside it.
//! * **Settling.** A rig resting on the ground or drilling settles level (to its gravity
//!   frame) and ignores the rotation thrusters while it rests (where rotation is limited).
//!   On the home world a settled rig is exactly level and still again, so movement returns
//!   to the classic path bit for bit.
//! * **Assists** (upgrades, off by default even when fitted): the **gyro stabiliser** brakes
//!   spin whenever the thrusters are idle; **horizon hold** eases the rig back to level
//!   while the thrusters are idle in flight.
//! * **Thrust is craft-relative** (see the world's movement): the main thruster pushes
//!   along the rig's own up axis, so leaning and thrusting flies sideways.
//!
//! The step is integrated in fixed internal steps of at most [`ATTITUDE_STEP_SECONDS`]
//! whatever the tick rate (and scaled by the field's time scale), using only basic
//! arithmetic, so it is tick-rate safe and identical on every platform.

use serde::{Deserialize, Serialize};

use crate::field::trig::wrap_angle;

/// A rig's orientation and spin (per tick, replicated with its pose).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RigAttitude {
    /// Orientation in radians, clockwise on screen from upright (`-pi..pi`).
    pub orientation: f32,
    /// Spin in radians per second, clockwise positive.
    pub angular_velocity: f32,
}

impl RigAttitude {
    /// Upright and still.
    pub const LEVEL: Self = Self {
        orientation: 0.0,
        angular_velocity: 0.0,
    };

    /// Whether the rig is exactly upright and not spinning (movement then takes the classic
    /// path).
    #[must_use]
    #[allow(
        clippy::float_cmp,
        reason = "level is an exact state the settle snaps to"
    )]
    pub fn is_level(&self) -> bool {
        self.orientation == 0.0 && self.angular_velocity == 0.0
    }
}

/// Assists the seat has switched on (they work only when fitted, see [`AttitudeGear`]).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct AttitudeAssists {
    pub gyro: bool,
    pub horizon: bool,
}

impl AttitudeAssists {
    /// As bits (wire form in [`crate::ControlState`]).
    #[must_use]
    pub const fn bits(self) -> u8 {
        (self.gyro as u8) | ((self.horizon as u8) << 1)
    }

    /// From [`Self::bits`].
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self {
            gyro: bits & 1 != 0,
            horizon: bits & 2 != 0,
        }
    }
}

/// The rotation hardware fitted to a rig (private loadout, in
/// [`crate::protection::HazardGear`]). **Upgrade hook:** nothing sells these yet; the
/// progression track (F10) adds them to the shop. Test mode fits them all.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct AttitudeGear {
    /// Tilt rating: each tier adds [`TILT_LIMIT_PER_TIER`] to the home-world lean limit.
    #[serde(default)]
    pub tilt_rating: u8,
    /// Gyro stabiliser fitted (0 or 1).
    #[serde(default)]
    pub gyro_stabiliser: u8,
    /// Horizon hold fitted (0 or 1).
    #[serde(default)]
    pub horizon_hold: u8,
}

impl AttitudeGear {
    /// Everything fitted (test mode).
    pub const MAXED: Self = Self {
        tilt_rating: TILT_RATING_MAX_LEVEL,
        gyro_stabiliser: 1,
        horizon_hold: 1,
    };
}

/// Lean limit either way while flying where rotation is limited (radians, 35 degrees).
pub const BASE_TILT_LIMIT: f32 = 35.0 * std::f32::consts::PI / 180.0;
/// Extra lean per tilt rating tier (radians, 10 degrees).
pub const TILT_LIMIT_PER_TIER: f32 = 10.0 * std::f32::consts::PI / 180.0;
/// Top tilt rating tier (75 degrees).
pub const TILT_RATING_MAX_LEVEL: u8 = 4;
/// Angular acceleration of the rotation thrusters at full input (rad/s²).
pub const ROTATION_THRUST: f32 = 10.0;
/// Angular drag (per second): top spin is `ROTATION_THRUST / ANGULAR_DRAG` (about 190
/// degrees a second).
pub const ANGULAR_DRAG: f32 = 3.0;
/// Angular drag with the gyro stabiliser braking (thrusters idle).
pub const GYRO_DRAG: f32 = 12.0;
/// Horizon hold's spring (rad/s² per radian of lean) and its damping (per second).
pub const HORIZON_STIFFNESS: f32 = 30.0;
pub const HORIZON_DAMPING: f32 = 10.0;
/// How fast a resting rig settles level (per second, exponential).
pub const SETTLE_RATE: f32 = 9.0;
/// A settling rig always turns level at least this fast (rad/s).
pub const SETTLE_MIN_SPEED: f32 = 0.5;
/// A settling rig snaps exactly level within this much (radians, about 0.1 degree).
pub const SETTLE_SNAP: f32 = 0.002;
/// Where rotation is unlimited, a resting rig settles only when it leans less than this.
pub const SETTLE_WINDOW: f32 = BASE_TILT_LIMIT;
/// How fast a rig carried beyond its limit eases back inside (rad/s).
pub const LIMIT_RETURN_SPEED: f32 = 2.5;
/// How much leaning while thrusting steers sideways, as stick per unit of the lean's sine
/// (35 degrees steers about 0.8 of full stick).
pub const TILT_STEER_GAIN: f32 = 1.4;
/// An idle rig spinning slower than this (rad/s, about one degree a second) stops.
pub const SPIN_REST: f32 = 0.02;
/// Longest internal integration step (240 Hz).
pub const ATTITUDE_STEP_SECONDS: f32 = 1.0 / 240.0;

/// Steps per radian (and per radian a second) attitudes are quantized to after every tick.
pub const ANGLE_STEPS_PER_RADIAN: f32 = 65_536.0;

/// `value` on the attitude grid ([`ANGLE_STEPS_PER_RADIAN`]).
#[must_use]
pub fn quantize_angle(value: f32) -> f32 {
    (value * ANGLE_STEPS_PER_RADIAN).round() / ANGLE_STEPS_PER_RADIAN
}

/// The lean limit (radians) of a rig with tilt rating `tilt_rating` where rotation is
/// limited.
#[must_use]
pub fn tilt_limit(tilt_rating: u8) -> f32 {
    f32::from(tilt_rating.min(TILT_RATING_MAX_LEVEL)).mul_add(TILT_LIMIT_PER_TIER, BASE_TILT_LIMIT)
}

/// What one attitude step needs to know.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttitudeContext {
    /// The orientation that counts as level here (radians): the gravity frame's angle, or
    /// the rig's own orientation where it is weightless.
    pub level: f32,
    /// Lean limit either way from level, `None` where rotation is unlimited.
    pub limit: Option<f32>,
    /// Resting on the ground (along local gravity) or drilling.
    pub resting: bool,
    /// Rotation thruster input, `-1..=1` (clockwise positive).
    pub input: f32,
    /// Fitted assists the seat has switched on.
    pub gyro: bool,
    pub horizon: bool,
    /// Seconds to advance (the tick, scaled by the field's time scale).
    pub seconds: f32,
}

/// Advances `attitude` by one tick of `context` (before collision: the caller refuses a
/// rotation that would push the rig into rock).
#[must_use]
pub fn step_attitude(attitude: RigAttitude, context: &AttitudeContext) -> RigAttitude {
    if context.seconds <= 0.0 {
        return attitude;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a bounded handful of internal steps"
    )]
    let steps = ((context.seconds / ATTITUDE_STEP_SECONDS).ceil() as u32).clamp(1, 64);
    #[allow(clippy::cast_precision_loss, reason = "a handful of steps")]
    let dt = context.seconds / steps as f32;
    let mut lean = wrap_angle(attitude.orientation - context.level);
    let mut spin = attitude.angular_velocity;
    let input = context.input.clamp(-1.0, 1.0);
    let settle = context.resting && (context.limit.is_some() || lean.abs() <= SETTLE_WINDOW);
    for _ in 0..steps {
        if settle {
            spin = 0.0;
            // Exponential, with a floor speed so it always arrives.
            let step = (lean.abs() * SETTLE_RATE).max(SETTLE_MIN_SPEED) * dt;
            lean = if lean > 0.0 {
                (lean - step).max(0.0)
            } else {
                (lean + step).min(0.0)
            };
            if lean.abs() < SETTLE_SNAP {
                lean = 0.0;
            }
            continue;
        }
        let idle = input == 0.0;
        let mut acceleration = input * ROTATION_THRUST;
        let mut drag = ANGULAR_DRAG;
        if idle && context.gyro {
            drag = GYRO_DRAG;
        }
        if idle && context.horizon && !context.resting {
            acceleration -= HORIZON_STIFFNESS * lean;
            drag = drag.max(HORIZON_DAMPING);
        }
        // Semi-implicit Euler with the drag applied implicitly (stable at any step).
        spin = acceleration.mul_add(dt, spin) / drag.mul_add(dt, 1.0);
        if idle && spin.abs() < SPIN_REST {
            spin = 0.0;
        }
        lean = wrap_angle(spin.mul_add(dt, lean));
        if let Some(limit) = context.limit {
            if lean > limit + 1e-6 || lean < -limit - 1e-6 {
                // Carried in beyond the limit (or leaning past it): ease back inside.
                let target = lean.clamp(-limit, limit);
                let step = LIMIT_RETURN_SPEED * dt;
                lean = if lean > target {
                    (lean - step).max(target)
                } else {
                    (lean + step).min(target)
                };
                if (lean > 0.0 && spin > 0.0) || (lean < 0.0 && spin < 0.0) {
                    spin = 0.0;
                }
            }
        }
        if context.horizon && idle && lean.abs() < SETTLE_SNAP && spin == 0.0 {
            lean = 0.0;
        }
    }
    let orientation = if lean == 0.0 {
        context.level
    } else {
        wrap_angle(context.level + lean)
    };
    RigAttitude {
        orientation: quantize_angle(orientation),
        angular_velocity: quantize_angle(spin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flying(input: f32, seconds: f32) -> AttitudeContext {
        AttitudeContext {
            level: 0.0,
            limit: Some(tilt_limit(0)),
            resting: false,
            input,
            gyro: false,
            horizon: false,
            seconds,
        }
    }

    fn run(
        mut attitude: RigAttitude,
        context: AttitudeContext,
        hz: u16,
        seconds: f32,
    ) -> RigAttitude {
        let ticks = (f32::from(hz) * seconds).round() as u32;
        let context = AttitudeContext {
            seconds: 1.0 / f32::from(hz),
            ..context
        };
        for _ in 0..ticks {
            attitude = step_attitude(attitude, &context);
        }
        attitude
    }

    #[test]
    fn a_level_rig_with_idle_thrusters_stays_exactly_level() {
        let after = run(RigAttitude::LEVEL, flying(0.0, 0.0), 60, 2.0);
        assert!(after.is_level());
    }

    #[test]
    fn rotation_thrusters_lean_the_rig_only_to_its_limit_on_the_home_world() {
        let leaning = run(RigAttitude::LEVEL, flying(1.0, 0.0), 60, 2.0);
        let limit = tilt_limit(0);
        assert!((leaning.orientation - limit).abs() < 0.01, "{leaning:?}");
        let other = run(RigAttitude::LEVEL, flying(-1.0, 0.0), 60, 2.0);
        assert!((other.orientation + limit).abs() < 0.01, "{other:?}");
        let rated = AttitudeContext {
            limit: Some(tilt_limit(TILT_RATING_MAX_LEVEL)),
            ..flying(1.0, 0.0)
        };
        let far = run(RigAttitude::LEVEL, rated, 60, 3.0);
        assert!(
            far.orientation > limit + 0.6,
            "the tilt rating leans further: {far:?}"
        );
    }

    #[test]
    fn rotation_is_unlimited_where_the_field_allows_it() {
        let context = AttitudeContext {
            limit: None,
            ..flying(1.0, 0.0)
        };
        let mut attitude = RigAttitude::LEVEL;
        let mut turned = 0.0;
        for _ in 0..240 {
            let next = step_attitude(
                attitude,
                &AttitudeContext {
                    seconds: 1.0 / 60.0,
                    ..context
                },
            );
            turned += wrap_angle(next.orientation - attitude.orientation);
            attitude = next;
        }
        assert!(
            turned > std::f32::consts::TAU,
            "a full turn and more: {turned}"
        );
    }

    #[test]
    fn a_resting_rig_settles_exactly_level_and_ignores_the_thrusters() {
        let leaning = RigAttitude {
            orientation: 0.5,
            angular_velocity: 1.0,
        };
        let resting = AttitudeContext {
            resting: true,
            ..flying(1.0, 0.0)
        };
        let after = run(leaning, resting, 60, 1.5);
        assert!(after.is_level(), "{after:?}");
    }

    #[test]
    fn spin_carries_on_after_release_and_the_gyro_kills_it() {
        let spinning = RigAttitude {
            orientation: 0.0,
            angular_velocity: 2.0,
        };
        let free = AttitudeContext {
            limit: None,
            ..flying(0.0, 0.0)
        };
        let coasting = run(spinning, free, 60, 0.25);
        assert!(coasting.angular_velocity > 0.5, "momentum: {coasting:?}");
        let gyro = run(spinning, AttitudeContext { gyro: true, ..free }, 60, 1.0);
        assert!(gyro.angular_velocity.abs() < f32::EPSILON, "{gyro:?}");
    }

    #[test]
    fn horizon_hold_levels_an_idle_rig_in_flight() {
        let leaning = RigAttitude {
            orientation: 0.5,
            angular_velocity: 0.0,
        };
        let held = run(
            leaning,
            AttitudeContext {
                horizon: true,
                ..flying(0.0, 0.0)
            },
            60,
            2.0,
        );
        assert!(held.is_level(), "{held:?}");
        let unheld = run(leaning, flying(0.0, 0.0), 60, 2.0);
        assert!(
            unheld.orientation > 0.4,
            "no automatic tilt without the assist: {unheld:?}"
        );
    }

    #[test]
    fn attitude_agrees_across_tick_rates() {
        let reference = run(
            RigAttitude::LEVEL,
            AttitudeContext {
                limit: None,
                ..flying(0.6, 0.0)
            },
            60,
            0.8,
        );
        for hz in [20, 30, 120, 240] {
            let other = run(
                RigAttitude::LEVEL,
                AttitudeContext {
                    limit: None,
                    ..flying(0.6, 0.0)
                },
                hz,
                0.8,
            );
            assert!(
                (other.orientation - reference.orientation).abs() < 2e-3,
                "{hz} Hz: {other:?} vs {reference:?}"
            );
        }
    }
}
