//! Landing is a skill (expansion plan F12): touching down on a round body tilted slides,
//! scrapes or tips the craft; on the ground it rests on its tracks the way physics puts it.
//!
//! On a planet the player turns the craft freely with its rotation thrusters and **nothing
//! turns it for them** in flight: gravity pulls toward the body's centre ("down" in the strip)
//! but never rotates the craft. Only contact does:
//!
//! * **Touchdown** ([`touchdown`]): coming down onto the ground, the craft's lean from the
//!   ground under it decides the landing. Within the safe angle ([`safe_angle`], wider with
//!   landing legs) it is clean; past it the craft **slides** the way it leans and, harder or
//!   more tilted, **scrapes** its hull; past the tip angle ([`tip_angle`]) it **tips over** onto
//!   its side and takes a heavier knock.
//! * **Resting** ([`rest_target`]): on its tracks the craft settles onto the ground's slope
//!   (level on flat ground, leaning on a slope, [`slope_angle`]) as contact pushes it, at the
//!   attitude model's settle rate: a smooth roll, never a camera snap. Tipped past the tip
//!   angle it lies on its side (or its roof) instead, and the player rights it by holding the
//!   rotation thrusters toward upright ([`righting`]).
//!
//! Everything is a pure function of its inputs (basic arithmetic), so hosts and predicting
//! clients agree at every tick rate.

use serde::{Deserialize, Serialize};

use crate::field::trig::wrap_angle;

/// Degrees to radians.
const DEG: f32 = std::f32::consts::PI / 180.0;

/// Lean from the ground (radians) a stock rig touches down cleanly within (12 degrees).
pub const BASE_SAFE_ANGLE: f32 = 12.0 * DEG;
/// Extra safe lean per landing legs tier (radians, 12 degrees).
pub const LEGS_SAFE_ANGLE_PER_TIER: f32 = 12.0 * DEG;
/// Lean from the ground (radians) past which a stock rig tips over on touchdown, and past
/// which a resting rig falls onto its side (50 degrees).
pub const BASE_TIP_ANGLE: f32 = 50.0 * DEG;
/// Extra tip angle per landing legs tier (radians, 10 degrees).
pub const LEGS_TIP_ANGLE_PER_TIER: f32 = 10.0 * DEG;
/// Top landing legs tier.
pub const LANDING_LEGS_MAX_LEVEL: u8 = 2;
/// The steepest lean (radians) a rig resting on its tracks takes from the ground's slope.
pub const MAX_SLOPE_LEAN: f32 = 25.0 * DEG;
/// How far apart (px) the ground is sampled under the tracks to find its slope (just outside
/// the tracks: a craft leans where a track hangs over a step).
pub const SLOPE_SPAN_PX: f32 = 28.0;
/// Sideways speed (px/s) a slide gives per unit of fall speed per radian of excess lean.
pub const SLIDE_GAIN: f32 = 1.6;
/// Hull a scrape takes per (px/s of fall speed x radian of excess lean).
pub const SCRAPE_DAMAGE: f32 = 0.06;
/// A slide that costs at least this much hull counts as a scrape.
pub const SCRAPE_THRESHOLD: f32 = 0.5;
/// Hull a tip-over takes, plus [`SCRAPE_DAMAGE`] on the excess past the safe angle.
pub const TIP_DAMAGE: f32 = 6.0;
/// Slowest fall (px/s) that counts as a touchdown: a craft resting on the ground meets it a
/// tick's worth of gravity at a time, and one set down slower than this simply rests.
pub const TOUCHDOWN_MIN_FALL_SPEED: f32 = 25.0;
/// Least fall speed (px/s) a touchdown counts at (a rig set down from a hover still lands).
pub const MIN_TOUCHDOWN_SPEED: f32 = 40.0;

/// The safe touchdown lean (radians) with landing legs tier `legs`.
#[must_use]
pub fn safe_angle(legs: u8) -> f32 {
    f32::from(legs.min(LANDING_LEGS_MAX_LEVEL)).mul_add(LEGS_SAFE_ANGLE_PER_TIER, BASE_SAFE_ANGLE)
}

/// The tip angle (radians) with landing legs tier `legs`.
#[must_use]
pub fn tip_angle(legs: u8) -> f32 {
    f32::from(legs.min(LANDING_LEGS_MAX_LEVEL)).mul_add(LEGS_TIP_ANGLE_PER_TIER, BASE_TIP_ANGLE)
}

/// How a touchdown went (also the [`PlanetEvent`] it fires).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Touchdown {
    /// Within the safe angle.
    Clean,
    /// Leaning too far: the craft slides the way it leans.
    Slid,
    /// Sliding hard enough to scrape the hull.
    Scraped,
    /// Past the tip angle: the craft tips over onto its side.
    Tipped,
}

/// What a touchdown does to the craft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchdownEffect {
    pub outcome: Touchdown,
    /// Hull damage (before the rig's mitigation).
    pub hull_damage: f32,
    /// Sideways speed (px/s, strip `+x` is clockwise) added along the ground.
    pub slide: f32,
}

/// The touchdown of a craft leaning `lean` (radians, clockwise positive) from the ground's
/// slope, coming down at `fall_speed` px/s, with landing legs tier `legs`.
#[must_use]
pub fn touchdown(lean: f32, fall_speed: f32, legs: u8) -> TouchdownEffect {
    let lean = wrap_angle(lean);
    let tilt = lean.abs();
    let safe = safe_angle(legs);
    let speed = fall_speed.max(MIN_TOUCHDOWN_SPEED);
    if tilt <= safe {
        return TouchdownEffect {
            outcome: Touchdown::Clean,
            hull_damage: 0.0,
            slide: 0.0,
        };
    }
    let excess = tilt - safe;
    let direction = if lean > 0.0 { 1.0 } else { -1.0 };
    if tilt > tip_angle(legs) {
        return TouchdownEffect {
            outcome: Touchdown::Tipped,
            hull_damage: (SCRAPE_DAMAGE * speed).mul_add(excess, TIP_DAMAGE),
            slide: direction * SLIDE_GAIN * speed * excess * 0.5,
        };
    }
    let damage = SCRAPE_DAMAGE * speed * excess;
    TouchdownEffect {
        outcome: if damage >= SCRAPE_THRESHOLD {
            Touchdown::Scraped
        } else {
            Touchdown::Slid
        },
        hull_damage: damage,
        slide: direction * SLIDE_GAIN * speed * excess,
    }
}

/// The ground's slope under a craft (radians, clockwise positive).
///
/// The ground rising to the right tilts the craft counter-clockwise, so a rise to the right
/// is negative. From the ground's top `left_top` and `right_top` (px, `+y` down) sampled [`SLOPE_SPAN_PX`] apart,
/// capped at [`MAX_SLOPE_LEAN`].
#[must_use]
pub fn slope_angle(left_top: f32, right_top: f32) -> f32 {
    // A rise to the right (right_top < left_top) leans the craft back: counter-clockwise.
    let rise = left_top - right_top;
    let angle = crate::field::trig::atan2(-rise, SLOPE_SPAN_PX);
    angle.clamp(-MAX_SLOPE_LEAN, MAX_SLOPE_LEAN)
}

/// Where a resting craft settles (the target orientation, radians).
///
/// The ground's `slope` while it leans less than the tip angle from it, else onto its side (a quarter turn from the slope, the way it leans) or, past
/// three quarters, its roof.
#[must_use]
pub fn rest_target(orientation: f32, slope: f32, legs: u8) -> f32 {
    let lean = wrap_angle(orientation - slope);
    let tilt = lean.abs();
    if tilt <= tip_angle(legs) {
        return slope;
    }
    let side: f32 = if lean > 0.0 { 1.0 } else { -1.0 };
    if tilt > 0.75 * std::f32::consts::PI {
        return wrap_angle(slope + std::f32::consts::PI);
    }
    wrap_angle(side.mul_add(std::f32::consts::FRAC_PI_2, slope))
}

/// Whether a resting craft is tipped over (it leans past the tip angle from the slope).
#[must_use]
pub fn is_tipped(orientation: f32, slope: f32, legs: u8) -> bool {
    wrap_angle(orientation - slope).abs() > tip_angle(legs)
}

/// Whether the player is righting a tipped craft: holding the rotation thrusters (`input`,
/// clockwise positive) toward upright.
#[must_use]
pub fn righting(orientation: f32, slope: f32, legs: u8, input: f32) -> bool {
    if !is_tipped(orientation, slope, legs) || input == 0.0 {
        return false;
    }
    let lean = wrap_angle(orientation - slope);
    // Leaning clockwise (positive), turning counter-clockwise (negative input) rights it.
    (lean > 0.0) != (input > 0.0)
}

/// Something of a round body's ground happened (netcode codes 780-799).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PlanetEvent {
    /// A clean touchdown.
    Landed,
    /// A touchdown that slid.
    Slid,
    /// A touchdown that scraped the hull.
    HullScraped,
    /// A touchdown that tipped the craft over.
    Tipped,
    /// A tipped craft rolled back onto its tracks.
    Righted,
}

impl PlanetEvent {
    /// Offset of the event's netcode code from 780.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// The event a touchdown fires.
    #[must_use]
    pub const fn of(outcome: Touchdown) -> Self {
        match outcome {
            Touchdown::Clean => Self::Landed,
            Touchdown::Slid => Self::Slid,
            Touchdown::Scraped => Self::HullScraped,
            Touchdown::Tipped => Self::Tipped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_touchdown_is_clean_at_any_speed() {
        for speed in [0.0, 100.0, 300.0] {
            let effect = touchdown(0.0, speed, 0);
            assert_eq!(effect.outcome, Touchdown::Clean);
            assert!(effect.hull_damage.abs() < f32::EPSILON && effect.slide.abs() < f32::EPSILON);
        }
        assert_eq!(touchdown(11.0 * DEG, 200.0, 0).outcome, Touchdown::Clean);
    }

    #[test]
    fn leaning_touchdowns_slide_the_way_they_lean_then_scrape_then_tip() {
        let slid = touchdown(16.0 * DEG, 60.0, 0);
        assert_eq!(slid.outcome, Touchdown::Slid);
        assert!(slid.slide > 0.0);
        let left = touchdown(-16.0 * DEG, 60.0, 0);
        assert!(left.slide < 0.0, "leaning left slides left");
        let scraped = touchdown(35.0 * DEG, 200.0, 0);
        assert_eq!(scraped.outcome, Touchdown::Scraped);
        assert!(scraped.hull_damage > slid.hull_damage);
        let tipped = touchdown(70.0 * DEG, 120.0, 0);
        assert_eq!(tipped.outcome, Touchdown::Tipped);
        assert!(tipped.hull_damage > TIP_DAMAGE);
    }

    #[test]
    fn landing_legs_widen_the_safe_and_tip_angles() {
        assert_eq!(touchdown(20.0 * DEG, 150.0, 0).outcome, Touchdown::Scraped);
        assert_eq!(touchdown(20.0 * DEG, 150.0, 1).outcome, Touchdown::Clean);
        assert_eq!(touchdown(55.0 * DEG, 150.0, 0).outcome, Touchdown::Tipped);
        assert_ne!(touchdown(55.0 * DEG, 150.0, 2).outcome, Touchdown::Tipped);
        assert!(safe_angle(9) <= safe_angle(LANDING_LEGS_MAX_LEVEL) + f32::EPSILON);
    }

    #[test]
    fn a_resting_craft_settles_onto_the_slope_or_its_side() {
        let slope = 10.0 * DEG;
        assert!((rest_target(30.0 * DEG, slope, 0) - slope).abs() < 1e-6);
        assert!((rest_target(-20.0 * DEG, 0.0, 0)).abs() < 1e-6);
        let side = rest_target(80.0 * DEG, 0.0, 0);
        assert!((side - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        let other = rest_target(-80.0 * DEG, 0.0, 0);
        assert!((other + std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        let roof = rest_target(170.0 * DEG, 0.0, 0);
        assert!((roof.abs() - std::f32::consts::PI).abs() < 1e-5);
        assert!(is_tipped(80.0 * DEG, 0.0, 0) && !is_tipped(30.0 * DEG, 0.0, 0));
        assert!(righting(80.0 * DEG, 0.0, 0, -1.0));
        assert!(!righting(80.0 * DEG, 0.0, 0, 1.0));
        assert!(!righting(30.0 * DEG, 0.0, 0, -1.0));
    }

    #[test]
    fn slopes_lean_the_craft_and_are_capped() {
        assert!(slope_angle(100.0, 100.0).abs() < 1e-6);
        // Ground rising to the right: the craft leans back (counter-clockwise).
        assert!(slope_angle(100.0, 80.0) < 0.0);
        assert!(slope_angle(80.0, 100.0) > 0.0);
        assert!((slope_angle(0.0, 500.0) - MAX_SLOPE_LEAN).abs() < 1e-6);
    }
}
