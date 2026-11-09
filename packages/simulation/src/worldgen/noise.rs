//! Integer-only deterministic hashing and coherent noise for world generation.
//!
//! Everything here is plain integer arithmetic (wrapping multiplies, shifts and fixed-point
//! interpolation), so a seed produces bit-identical worlds on every platform. Noise values are
//! in `0..=NOISE_MAX`.

#![allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    reason = "lattice coordinates are hashed as raw bits and fixed-point values are bounded"
)]

/// Largest noise value (16-bit fixed point, `NOISE_MAX` is "1.0").
pub const NOISE_MAX: i32 = 0xFFFF;

/// Well-mixed 64-bit hash of a lattice point under `seed` and a per-purpose `salt`.
#[must_use]
pub const fn hash2(seed: u64, salt: u64, x: i32, y: i32) -> u64 {
    let mut z = seed
        ^ salt.wrapping_mul(0xA076_1D64_78BD_642F)
        ^ (x as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// [`hash2`] reduced to `0..=NOISE_MAX`.
#[must_use]
pub const fn lattice(seed: u64, salt: u64, x: i32, y: i32) -> i32 {
    (hash2(seed, salt, x, y) >> 48) as i32
}

/// Whether a hashed lattice point passes a `per_mille` chance.
#[must_use]
pub const fn chance(seed: u64, salt: u64, x: i32, y: i32, per_mille: u32) -> bool {
    (hash2(seed, salt, x, y) % 1000) < per_mille as u64
}

/// Smoothstep of a fixed-point `t` in `0..=65536`.
const fn smooth(t: i64) -> i64 {
    // t^2 * (3 - 2t), all in 16.16 fixed point.
    (((t * t) >> 16) * (3 * 65_536 - 2 * t)) >> 16
}

/// Fixed-point position of `v` inside a lattice cell of `cell` units: (cell index, 0..65536).
const fn cell_coord(v: i32, cell: i32) -> (i32, i64) {
    let index = v.div_euclid(cell);
    let offset = v.rem_euclid(cell) as i64;
    // Sample cell centres so neighbouring cells blend symmetrically.
    (index, (offset * 2 + 1) * 32_768 / cell as i64)
}

/// Smooth 2D value noise with lattice spacing `cell_x` by `cell_y` tiles.
#[must_use]
#[allow(
    clippy::many_single_char_names,
    reason = "bilinear interpolation reads best with corner names"
)]
pub const fn value_noise(seed: u64, salt: u64, x: i32, y: i32, cell_x: i32, cell_y: i32) -> i32 {
    let (ix, fx) = cell_coord(x, cell_x);
    let (iy, fy) = cell_coord(y, cell_y);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let a = lattice(seed, salt, ix, iy) as i64;
    let b = lattice(seed, salt, ix + 1, iy) as i64;
    let c = lattice(seed, salt, ix, iy + 1) as i64;
    let d = lattice(seed, salt, ix + 1, iy + 1) as i64;
    let top = a + (((b - a) * sx) >> 16);
    let bottom = c + (((d - c) * sx) >> 16);
    (top + (((bottom - top) * sy) >> 16)) as i32
}

/// Smooth 1D value noise with lattice spacing `cell` (a wavy line along x).
#[must_use]
pub const fn line_noise(seed: u64, salt: u64, x: i32, cell: i32) -> i32 {
    let (ix, fx) = cell_coord(x, cell);
    let s = smooth(fx);
    let a = lattice(seed, salt, ix, 0) as i64;
    let b = lattice(seed, salt, ix + 1, 0) as i64;
    (a + (((b - a) * s) >> 16)) as i32
}

/// Fractal (octave-summed) value noise: each octave halves the cell size and the amplitude.
///
/// The result is renormalised to `0..=NOISE_MAX`, then stretched so its spread is close to a
/// single octave's (summed octaves bunch towards the middle).
#[must_use]
pub const fn fractal_noise(
    seed: u64,
    salt: u64,
    x: i32,
    y: i32,
    cell_x: i32,
    cell_y: i32,
    octaves: u32,
) -> i32 {
    let mut total: i64 = 0;
    let mut weight_sum: i64 = 0;
    let mut weight: i64 = 256;
    let (mut cx, mut cy) = (cell_x, cell_y);
    let mut octave = 0;
    while octave < octaves {
        total += value_noise(seed, salt.wrapping_add(octave as u64 * 0x51), x, y, cx, cy) as i64
            * weight;
        weight_sum += weight;
        weight /= 2;
        cx = if cx > 2 { cx / 2 } else { 1 };
        cy = if cy > 2 { cy / 2 } else { 1 };
        octave += 1;
    }
    let mean = total / weight_sum;
    let stretched = 32_768 + (mean - 32_768) * 3 / 2;
    if stretched < 0 {
        0
    } else if stretched > NOISE_MAX as i64 {
        NOISE_MAX
    } else {
        stretched as i32
    }
}

/// Scales a noise value (`0..=NOISE_MAX`) to `-amplitude..=amplitude` around its middle,
/// rounding to the nearest whole step.
#[must_use]
pub const fn centred(value: i32, amplitude: i32) -> i32 {
    let offset = (value - NOISE_MAX / 2) * amplitude * 2;
    if offset >= 0 {
        (offset + NOISE_MAX / 2) / NOISE_MAX
    } else {
        -((-offset + NOISE_MAX / 2) / NOISE_MAX)
    }
}

/// Ridge transform: `NOISE_MAX` where `value` crosses the middle, falling to 0 at the extremes.
/// Thresholding a ridge near the top gives thin wandering lines (veins, seams).
#[must_use]
pub const fn ridge(value: i32) -> i32 {
    NOISE_MAX - (2 * value - NOISE_MAX).abs()
}

/// Small deterministic RNG for region-based placement (seeded per region and purpose).
#[derive(Clone, Copy, Debug)]
pub struct RegionRng(u64);

impl RegionRng {
    #[must_use]
    pub const fn new(seed: u64, salt: u64, x: i32, y: i32) -> Self {
        Self(hash2(seed, salt, x, y) | 1)
    }

    pub const fn next_u64(&mut self) -> u64 {
        // splitmix64 step.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform integer in `low..=high` (`low` when the range is empty).
    pub const fn range(&mut self, low: i32, high: i32) -> i32 {
        if high <= low {
            return low;
        }
        let span = (high - low + 1) as u64;
        low + (self.next_u64() % span) as i32
    }

    /// Whether a `per_mille` chance passes.
    pub const fn chance(&mut self, per_mille: u32) -> bool {
        self.next_u64() % 1000 < per_mille as u64
    }

    /// Index picked with probability proportional to `weights` (`None` if they sum to 0).
    pub fn pick(&mut self, weights: &[u32]) -> Option<usize> {
        let total: u64 = weights.iter().map(|weight| u64::from(*weight)).sum();
        if total == 0 {
            return None;
        }
        let mut roll = self.next_u64() % total;
        for (index, weight) in weights.iter().enumerate() {
            let weight = u64::from(*weight);
            if roll < weight {
                return Some(index);
            }
            roll -= weight;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_stays_in_range_and_is_smooth() {
        let mut previous = value_noise(7, 1, 0, 0, 8, 8);
        for x in 1..400 {
            let value = value_noise(7, 1, x, 0, 8, 8);
            assert!((0..=NOISE_MAX).contains(&value));
            assert!((value - previous).abs() < NOISE_MAX / 4, "jump at {x}");
            previous = value;
            let fractal = fractal_noise(7, 2, x, -x, 16, 9, 3);
            assert!((0..=NOISE_MAX).contains(&fractal));
        }
    }

    #[test]
    fn noise_is_a_pure_function() {
        assert_eq!(
            fractal_noise(99, 3, -40, 71, 20, 12, 3),
            fractal_noise(99, 3, -40, 71, 20, 12, 3)
        );
        assert_ne!(lattice(1, 0, 0, 0), lattice(2, 0, 0, 0));
    }

    #[test]
    fn region_rng_ranges_are_inclusive() {
        let mut rng = RegionRng::new(5, 9, 1, 2);
        let mut seen = [false; 4];
        for _ in 0..200 {
            let value = rng.range(2, 5);
            assert!((2..=5).contains(&value));
            seen[(value - 2) as usize] = true;
        }
        assert!(seen.iter().all(|seen| *seen));
    }
}
