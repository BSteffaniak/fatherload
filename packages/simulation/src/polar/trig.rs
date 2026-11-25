//! Deterministic `f64` trigonometry for the polar grid (expansion plan F12).
//!
//! The standard library's `sin`, `cos` and `atan2` call the platform's maths library, whose last
//! bits differ between targets (glibc, macOS, MSVC, emscripten). Polar conversions feed
//! gameplay (the crossing between space and a planet, local up and down), so they use only
//! basic IEEE arithmetic here: a Cody-Waite range reduction and Taylor polynomials, identical on
//! every platform and accurate to a few ulps (far below a sub-pixel at any radius).

use std::f64::consts::{FRAC_PI_2, FRAC_PI_6, PI, TAU};

/// `pi / 2` split in two for an exact range reduction (Cody-Waite): the high part has
/// trailing zero bits so `k * PIO2_HI` is exact for every `k` this module meets.
const PIO2_HI: f64 = 1.570_796_326_734_125_6;
const PIO2_LO: f64 = 6.077_100_506_506_192e-11;
/// `sqrt(3)`.
const SQRT_3: f64 = 1.732_050_807_568_877_2;
/// `tan(pi / 12)`: the reduced range of [`atan`].
const TAN_PI_12: f64 = 0.267_949_192_431_122_7;

/// Sine and cosine of `angle` (radians).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the quadrant count of a finite angle"
)]
pub fn sin_cos(angle: f64) -> (f64, f64) {
    if !angle.is_finite() {
        return (0.0, 1.0);
    }
    // Fold whole turns away first so the quadrant count stays small.
    let angle = (angle / TAU).round().mul_add(-TAU, angle);
    let quadrant = (angle / FRAC_PI_2).round();
    let reduced = quadrant.mul_add(-PIO2_LO, quadrant.mul_add(-PIO2_HI, angle));
    let (sin, cos) = (sin_poly(reduced), cos_poly(reduced));
    match (quadrant as i64).rem_euclid(4) {
        0 => (sin, cos),
        1 => (cos, -sin),
        2 => (-sin, -cos),
        _ => (-cos, sin),
    }
}

/// Sine of `x` for `|x| <= pi / 4` (Taylor to `x^19`).
fn sin_poly(x: f64) -> f64 {
    let x2 = x * x;
    let mut term = x;
    let mut sum = x;
    let mut n = 1.0;
    for _ in 0..9 {
        term *= -x2 / ((n + 1.0) * (n + 2.0));
        sum += term;
        n += 2.0;
    }
    sum
}

/// Cosine of `x` for `|x| <= pi / 4` (Taylor to `x^20`).
fn cos_poly(x: f64) -> f64 {
    let x2 = x * x;
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut n = 0.0;
    for _ in 0..10 {
        term *= -x2 / ((n + 1.0) * (n + 2.0));
        sum += term;
        n += 2.0;
    }
    sum
}

/// Arctangent of `t` (radians, `-pi/2..=pi/2`).
#[must_use]
pub fn atan(t: f64) -> f64 {
    if t.is_nan() {
        return 0.0;
    }
    let negative = t < 0.0;
    let mut a = t.abs();
    let mut offset = 0.0;
    // atan(a) = pi/2 - atan(1/a)
    let flip = a > 1.0;
    if flip {
        a = 1.0 / a;
    }
    if a > TAN_PI_12 {
        // atan(a) = pi/6 + atan((a sqrt3 - 1) / (sqrt3 + a))
        a = a.mul_add(SQRT_3, -1.0) / (SQRT_3 + a);
        offset = FRAC_PI_6;
    }
    // |a| <= tan(pi/12): the series converges fast (a^31 < 1e-17).
    let a2 = a * a;
    let mut power = a;
    let mut sum = a;
    let mut sign = -1.0;
    let mut denominator = 3.0;
    for _ in 0..15 {
        power *= a2;
        sum += sign * power / denominator;
        sign = -sign;
        denominator += 2.0;
    }
    let mut result = offset + sum;
    if flip {
        result = FRAC_PI_2 - result;
    }
    if negative { -result } else { result }
}

/// The angle of the vector `(x, y)` from the +x axis toward +y (`-pi..=pi`), like
/// `f64::atan2(y, x)`.
#[must_use]
pub fn atan2(y: f64, x: f64) -> f64 {
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }
    if x.abs() >= y.abs() {
        let base = atan(y / x);
        if x > 0.0 {
            base
        } else if y >= 0.0 {
            base + PI
        } else {
            base - PI
        }
    } else {
        let base = atan(x / y);
        if y > 0.0 {
            FRAC_PI_2 - base
        } else {
            -FRAC_PI_2 - base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_cos_match_the_platform_within_a_few_ulps() {
        for step in -4_050..4_050 {
            let angle = f64::from(step) * 0.012_345;
            let (sin, cos) = sin_cos(angle);
            assert!((sin - angle.sin()).abs() < 4e-15, "sin {angle}");
            assert!((cos - angle.cos()).abs() < 4e-15, "cos {angle}");
        }
        assert_eq!(sin_cos(0.0), (0.0, 1.0));
    }

    #[test]
    fn atan2_matches_the_platform_within_a_few_ulps() {
        for i in -40..=40 {
            for j in -40..=40 {
                let (y, x) = (f64::from(i) * 0.37, f64::from(j) * 0.91);
                let ours = atan2(y, x);
                if i == 0 && j == 0 {
                    assert!(ours.abs() < f64::EPSILON);
                    continue;
                }
                assert!((ours - y.atan2(x)).abs() < 4e-15, "atan2({y}, {x})");
            }
        }
    }
}
