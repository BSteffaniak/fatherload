//! Deterministic trigonometry for the simulation (expansion plan F9).
//!
//! Platform `sin`, `cos` and `atan2` come from each target's maths library and may differ in
//! the last bits between a host, a predicting client and the browser build. Everything here
//! uses only basic arithmetic, fused multiply-adds and rounding, which IEEE 754 specifies
//! exactly, so every platform computes the same bits.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// High part of pi / 2 (the nearest `f32`).
const PIO2_HI: f32 = FRAC_PI_2;
/// The rest of pi / 2 (`pi / 2 - PIO2_HI`).
const PIO2_LO: f32 = -4.371_139e-8;

/// Sine and cosine of `angle` (radians), each within about 2e-7 of the true value.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the quadrant count of a bounded simulation angle"
)]
pub fn sin_cos(angle: f32) -> (f32, f32) {
    if !angle.is_finite() {
        return (0.0, 1.0);
    }
    let quadrant = (angle / FRAC_PI_2).round();
    let r = quadrant.mul_add(-PIO2_LO, quadrant.mul_add(-PIO2_HI, angle));
    let r2 = r * r;
    // Taylor series on [-pi/4, pi/4]: the next terms are below 3e-7 there.
    let sin = (r2 * r).mul_add(
        r2.mul_add(
            r2.mul_add(r2.mul_add(1.0 / 362_880.0, -1.0 / 5_040.0), 1.0 / 120.0),
            -1.0 / 6.0,
        ),
        r,
    );
    let cos = r2.mul_add(
        r2.mul_add(
            r2.mul_add(r2.mul_add(1.0 / 40_320.0, -1.0 / 720.0), 1.0 / 24.0),
            -0.5,
        ),
        1.0,
    );
    match (quadrant as i64).rem_euclid(4) {
        0 => (sin, cos),
        1 => (cos, -sin),
        2 => (-sin, -cos),
        _ => (-cos, sin),
    }
}

/// Arctangent of `z` for `|z| <= 1`, within about 1e-5 radians.
fn atan_unit(z: f32) -> f32 {
    let z2 = z * z;
    z * z2.mul_add(
        z2.mul_add(
            z2.mul_add(
                z2.mul_add(z2.mul_add(-0.011_721_2, 0.052_653_32), -0.116_432_87),
                0.193_543_46,
            ),
            -0.332_623_47,
        ),
        0.999_977_26,
    )
}

/// The angle of the vector `(x, y)` from the +x axis toward +y (radians in `-pi..=pi`), like
/// `f32::atan2(y, x)` within about 1e-5 radians. `(0, 0)` answers 0.
#[must_use]
pub fn atan2(y: f32, x: f32) -> f32 {
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }
    let (ax, ay) = (x.abs(), y.abs());
    let base = if ax >= ay {
        atan_unit(ay / ax)
    } else {
        FRAC_PI_2 - atan_unit(ax / ay)
    };
    let quadrant = if x < 0.0 { PI - base } else { base };
    if y < 0.0 { -quadrant } else { quadrant }
}

/// `angle` wrapped into `-pi..pi`.
#[must_use]
pub fn wrap_angle(angle: f32) -> f32 {
    if !angle.is_finite() {
        return 0.0;
    }
    if (-PI..PI).contains(&angle) {
        return angle;
    }
    let turns = ((angle + PI) / TAU).floor();
    let wrapped = turns.mul_add(-TAU, angle);
    if wrapped >= PI {
        wrapped - TAU
    } else if wrapped < -PI {
        wrapped + TAU
    } else {
        wrapped
    }
}

/// `e^-x` for `x >= 0`, within about 1e-7 relative error: halved until small, a Taylor
/// series, then squared back up.
#[must_use]
pub fn exp_neg(x: f32) -> f32 {
    if !x.is_finite() || x <= 0.0 {
        return if x > 0.0 { 0.0 } else { 1.0 };
    }
    if x > 80.0 {
        return 0.0;
    }
    let mut reduced = x;
    let mut squarings = 0_u32;
    while reduced > 0.125 {
        reduced *= 0.5;
        squarings += 1;
    }
    let y = -reduced;
    let mut value = y.mul_add(
        y.mul_add(
            y.mul_add(
                y.mul_add(y.mul_add(1.0 / 120.0, 1.0 / 24.0), 1.0 / 6.0),
                0.5,
            ),
            1.0,
        ),
        1.0,
    );
    for _ in 0..squarings {
        value *= value;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_cos_track_the_platform_functions_everywhere() {
        let mut angle = -20.0_f32;
        while angle < 20.0 {
            let (sin, cos) = sin_cos(angle);
            assert!((sin - angle.sin()).abs() < 1e-6, "sin {angle}");
            assert!((cos - angle.cos()).abs() < 1e-6, "cos {angle}");
            angle += 0.013;
        }
        assert_eq!(sin_cos(0.0), (0.0, 1.0), "zero is exact (the level rig)");
    }

    #[test]
    fn atan2_tracks_the_platform_function_in_every_quadrant() {
        for step in 0..720 {
            let angle = f32::from(i16::try_from(step).unwrap_or(0)).to_radians() * 0.5 - PI;
            let (y, x) = (angle.sin() * 3.0, angle.cos() * 3.0);
            assert!((atan2(y, x) - y.atan2(x)).abs() < 3e-5, "{angle}");
        }
        assert!(atan2(0.0, 0.0).abs() < f32::EPSILON);
        assert!(atan2(1.0, 0.0) > 0.0 && atan2(-1.0, 0.0) < 0.0);
    }

    #[test]
    fn wrap_and_exp_behave() {
        assert!(
            (wrap_angle(3.0 * PI) - -PI).abs() < 1e-5 || (wrap_angle(3.0 * PI) - PI).abs() < 1e-5
        );
        assert!((wrap_angle(-7.0) - (-7.0 + TAU)).abs() < 1e-5);
        assert!((wrap_angle(0.5) - 0.5).abs() < f32::EPSILON);
        for x in [0.0_f32, 0.01, 0.1, 0.7, 2.0, 9.0] {
            assert!(
                (exp_neg(x) - (-x).exp()).abs() < 1e-6 * (-x).exp().max(1e-3),
                "{x}"
            );
        }
    }
}
