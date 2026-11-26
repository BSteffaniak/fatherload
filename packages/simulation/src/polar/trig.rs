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

/// `sqrt(x² + y²)` with plain IEEE arithmetic.
///
/// The platform's `hypot` is a maths-library call whose last bits differ between targets
/// (Wave 12 systems). Exact for every finite input a
/// space distance meets (no overflow below about 1e150).
#[must_use]
#[allow(
    clippy::suboptimal_flops,
    clippy::imprecise_flops,
    reason = "plain IEEE arithmetic: the same bits on every peer"
)]
pub fn hypot(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

/// The natural logarithm of `x > 0` (0 for anything else) with plain IEEE arithmetic.
///
/// The exponent from the bits, then `2 atanh((m - 1) / (m + 1))` on the mantissa in `[sqrt(1/2),
/// sqrt(2))` (Wave 12 systems: the nav HUD's ETA).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "IEEE exponent bits"
)]
pub fn ln(x: f64) -> f64 {
    if !(x.is_finite() && x > 0.0) {
        return 0.0;
    }
    let mut value = x;
    let mut exponent = 0_i32;
    // Subnormals: scale into the normal range first.
    if value < f64::MIN_POSITIVE {
        value *= 2f64.powi(64);
        exponent -= 64;
    }
    let bits = value.to_bits();
    exponent += ((bits >> 52) & 0x7FF) as i32 - 1_023;
    let mut mantissa = f64::from_bits((bits & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000);
    if mantissa > std::f64::consts::SQRT_2 {
        mantissa *= 0.5;
        exponent += 1;
    }
    let t = (mantissa - 1.0) / (mantissa + 1.0);
    let t2 = t * t;
    // atanh series: t + t³/3 + t⁵/5 + ... (|t| < 0.172: 12 terms reach 1e-17).
    let mut sum = 0.0_f64;
    let mut term = t;
    let mut k = 1.0_f64;
    for _ in 0..12 {
        sum += term / k;
        term *= t2;
        k += 2.0;
    }
    f64::from(exponent).mul_add(std::f64::consts::LN_2, 2.0 * sum)
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

    #[test]
    #[allow(clippy::while_float, reason = "a sweep of values")]
    fn hypot_and_ln_track_the_platform_functions() {
        let mut x = 1e-6_f64;
        while x < 1e14 {
            let ours = ln(x);
            assert!(
                (ours - x.ln()).abs() < 1e-13 * x.ln().abs().max(1.0),
                "ln {x}"
            );
            let other = x.mul_add(0.37, 3.0);
            let h = hypot(x, other);
            let platform = x.hypot(other);
            assert!((h - platform).abs() <= platform * 4e-16, "hypot {x}");
            x *= 1.37;
        }
        assert!(ln(0.0).abs() < f64::EPSILON && ln(-1.0).abs() < f64::EPSILON);
    }
}
