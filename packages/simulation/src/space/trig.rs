//! Integer trigonometry for placement: every peer (native, WASM) places a planet on the same
//! tile, which `f64::sin` does not promise.

/// Angle units per full turn (angles are `u32`, taken modulo [`TURN`]).
pub const TURN: u32 = 1 << 16;
const QUARTER: i64 = 1 << 14;
/// π/2 in Q32.
const HALF_PI_Q32: i128 = 6_746_518_852;
const ONE_Q32: i128 = 1 << 32;

/// Fixed-point scale of [`cos_sin`] (Q30).
pub const ONE: i64 = 1 << 30;

/// sin of `f` quarter-turn units (0..=2^14 is 0..=π/2), Q30.
const fn sin_quarter(f: i64) -> i64 {
    let x = f as i128 * HALF_PI_Q32 / QUARTER as i128;
    let x2 = x * x / ONE_Q32;
    // Taylor to x^13: error below 1e-9 on [0, π/2].
    let mut term = x;
    let mut sum = x;
    let mut n = 1_i128;
    let mut sign = -1_i128;
    while n < 13 {
        term = term * x2 / ONE_Q32 / ((n + 1) * (n + 2));
        sum += sign * term;
        sign = -sign;
        n += 2;
    }
    let value = (sum + (1 << 1)) >> 2;
    if value > ONE as i128 {
        ONE
    } else {
        value as i64
    }
}

/// `(cos, sin)` of `angle` ([`TURN`] per turn), scaled by [`ONE`] (2^30). y grows down, so angle
/// `TURN / 4` points down and `3 * TURN / 4` points up.
#[must_use]
pub const fn cos_sin(angle: u32) -> (i64, i64) {
    let a = (angle % TURN) as i64;
    let quadrant = a / QUARTER;
    let f = a % QUARTER;
    let s = sin_quarter(f);
    let c = sin_quarter(QUARTER - f);
    match quadrant {
        0 => (c, s),
        1 => (-s, c),
        2 => (-c, -s),
        _ => (s, -c),
    }
}

/// The offset `radius` tiles out at `angle`.
#[must_use]
pub const fn polar(radius: i64, angle: u32) -> (i64, i64) {
    let (c, s) = cos_sin(angle);
    (
        ((radius as i128 * c as i128 + (1 << 29)) >> 30) as i64,
        ((radius as i128 * s as i128 + (1 << 29)) >> 30) as i64,
    )
}

/// The angle of `(dx, dy)` ([`TURN`] per turn), approximate (`f64`): only ever used to pick
/// which segments to enumerate, with padding, never to place anything.
#[must_use]
pub fn angle_of(dx: i64, dy: i64) -> f64 {
    let turns = (dy as f64).atan2(dx as f64) / std::f64::consts::TAU;
    turns.rem_euclid(1.0) * f64::from(TURN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_trig_matches_float_trig() {
        for angle in (0..TURN).step_by(97) {
            let (c, s) = cos_sin(angle);
            let radians = f64::from(angle) / f64::from(TURN) * std::f64::consts::TAU;
            assert!(
                (c as f64 / ONE as f64 - radians.cos()).abs() < 1e-8,
                "{angle}"
            );
            assert!(
                (s as f64 / ONE as f64 - radians.sin()).abs() < 1e-8,
                "{angle}"
            );
        }
        assert_eq!(cos_sin(0), (ONE, 0));
        assert_eq!(cos_sin(TURN / 4), (0, ONE));
        assert_eq!(polar(1_000, 3 * TURN / 4), (0, -1_000));
    }

    #[test]
    fn angle_of_inverts_polar() {
        for angle in (0..TURN).step_by(1_013) {
            let (dx, dy) = polar(1_000_000, angle);
            let back = angle_of(dx, dy);
            let diff = (back - f64::from(angle)).abs();
            assert!(diff < 2.0 || diff > f64::from(TURN) - 2.0, "{angle} {back}");
        }
    }
}
