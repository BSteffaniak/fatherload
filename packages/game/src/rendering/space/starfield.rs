#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "cell indices and pixel art"
)]

//! The deep sky: three parallax layers of stars (cells hashed from the world seed, so every
//! peer sees the same sky), the galaxy's band with its haze, and a few far galaxies. Stars at
//! infinity do not scale with the zoom; they slide a little as the craft travels (more for the
//! near layer) and stretch into streaks at Star Drive speeds. Twinkles are slow and faint.

use raylib::prelude::*;

use super::{
    Projection, SpaceArt, additive, draw_centred, fade, hash64, unit,
    view::{FlightMotion, SpaceRenderView},
};
use crate::rendering::pixel_art::palette;

/// One parallax layer.
struct Layer {
    /// Layer points per world pixel of travel.
    factor: f64,
    /// Cell side (layer points).
    cell: f64,
    /// Chance of a star per cell (0..=1).
    density: f32,
    /// Brightness range.
    bright: (f32, f32),
    /// Chance a star is a big (cross) one.
    big: f32,
}

const LAYERS: [Layer; 3] = [
    Layer {
        factor: 0.000_25,
        cell: 15.0,
        density: 0.5,
        bright: (0.3, 0.7),
        big: 0.0,
    },
    Layer {
        factor: 0.001,
        cell: 34.0,
        density: 0.55,
        bright: (0.5, 0.95),
        big: 0.06,
    },
    Layer {
        factor: 0.004,
        cell: 90.0,
        density: 0.5,
        bright: (0.75, 1.0),
        big: 0.3,
    },
];

/// Star tints, weighted toward white.
const STAR_TINTS: [Color; 8] = [
    palette::BONE,
    palette::BONE,
    palette::MIST,
    palette::ICE,
    palette::SKY_LIGHT,
    palette::CREAM,
    palette::GOLD,
    palette::PINK,
];

/// The galaxy's band: its direction in layer space and the spacing of its repeats.
const BAND_ANGLE: f64 = 0.42;
const BAND_SPACING: f64 = 1_700.0;
const BAND_WIDTH: f64 = 230.0;

/// How close `(x, y)` (deep-layer points) is to the galaxy's band (1 on its spine).
fn band_weight(x: f64, y: f64) -> f32 {
    let (sin, cos) = BAND_ANGLE.sin_cos();
    let across = (-sin * x + cos * y).rem_euclid(BAND_SPACING);
    let distance = across.min(BAND_SPACING - across);
    (-(distance / BAND_WIDTH).powi(2)).exp() as f32
}

/// The streak every layer's stars draw this frame: screen direction (unit) and length per
/// unit of layer factor, or `None` when the craft is not under the Star Drive.
fn streak(view: &SpaceRenderView<'_>, projection: &Projection) -> Option<((f32, f32), f32)> {
    let motion_scale = view
        .effects
        .scale(fatherload_simulation::field::ScreenEffect::Wobble)
        .max(0.25);
    let (heading, length) = match view.motion {
        FlightMotion::Coasting => return None,
        FlightMotion::Cruise { speed, heading } => (heading, (speed / 400.0).min(1.0) * 18.0),
        FlightMotion::Warp { charge, heading } => (heading, 30.0 + charge.clamp(0.0, 1.0) * 420.0),
    };
    let angle = projection.screen_angle(heading);
    // Stars stream backwards past the craft.
    Some(((-angle.cos(), -angle.sin()), length * motion_scale))
}

/// Draws the deep sky over the view.
pub fn draw_sky(
    draw: &mut impl RaylibDraw,
    art: &SpaceArt,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
) {
    let (abs_x, abs_y) = projection.camera.px_f64();
    let reach = f64::from(projection.reach()) + 8.0;
    let seed = view.sky_seed as i64;
    let streaks = streak(view, projection);
    draw_haze(art, view, projection, (abs_x, abs_y), reach);
    for (index, layer) in LAYERS.iter().enumerate() {
        let (ox, oy) = (abs_x * layer.factor, abs_y * layer.factor);
        let (first_x, last_x) = (
            ((ox - reach) / layer.cell).floor() as i64,
            ((ox + reach) / layer.cell).floor() as i64,
        );
        let (first_y, last_y) = (
            ((oy - reach) / layer.cell).floor() as i64,
            ((oy + reach) / layer.cell).floor() as i64,
        );
        let depth = (index + 1) as f32 / LAYERS.len() as f32;
        for cy in first_y..=last_y {
            for cx in first_x..=last_x {
                let h = hash64(&[seed, index as i64, cx, cy]);
                let (px, py) = (
                    (cx as f64 + f64::from(unit(h, 0))) * layer.cell,
                    (cy as f64 + f64::from(unit(h, 16))) * layer.cell,
                );
                let boost = if index == 0 { band_weight(px, py) } else { 0.0 };
                if unit(h, 32) > layer.density + boost * 0.9 {
                    continue;
                }
                let (lx, ly) = ((px - ox) as f32, (py - oy) as f32);
                if lx.abs() > reach as f32 || ly.abs() > reach as f32 {
                    continue;
                }
                let (sx, sy) = projection.turn((lx, ly));
                let point = (projection.pivot.0 + sx, projection.pivot.1 + sy);
                if !projection.visible(point, 4.0) {
                    continue;
                }
                let tint = STAR_TINTS[(h >> 48) as usize % STAR_TINTS.len()];
                let twinkle =
                    0.88 + 0.12 * (view.seconds * (0.6 + unit(h, 40) * 1.4) + unit(h, 24) * 6.3).sin();
                let bright = (layer.bright.0
                    + (layer.bright.1 - layer.bright.0) * unit(h, 52))
                    * twinkle;
                let colour = fade(tint, bright);
                let x = point.0.round();
                let y = point.1.round();
                if let Some(((dx, dy), length)) = streaks {
                    let length = length * depth * depth;
                    if length > 1.5 {
                        draw.draw_line_ex(
                            Vector2::new(x, y),
                            Vector2::new(x + dx * length, y + dy * length),
                            1.0 + depth,
                            colour,
                        );
                        continue;
                    }
                }
                let big = unit(h, 8) < layer.big;
                if big {
                    draw.draw_rectangle_rec(Rectangle::new(x - 1.0, y - 1.0, 3.0, 3.0), fade(tint, bright * 0.45));
                    draw.draw_rectangle_rec(Rectangle::new(x - 2.0, y, 5.0, 1.0), fade(tint, bright * 0.6));
                    draw.draw_rectangle_rec(Rectangle::new(x, y - 2.0, 1.0, 5.0), fade(tint, bright * 0.6));
                    draw.draw_rectangle_rec(Rectangle::new(x, y, 1.0, 1.0), palette::BONE);
                } else if index == 2 {
                    draw.draw_rectangle_rec(Rectangle::new(x, y, 2.0, 2.0), colour);
                } else {
                    draw.draw_rectangle_rec(Rectangle::new(x, y, 1.0, 1.0), colour);
                }
            }
        }
    }
}

/// The galaxy's haze along its band, and far galaxies: big faint cloud quads on the deepest
/// layer (cheap: a dozen textured quads).
fn draw_haze(
    art: &SpaceArt,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
    (abs_x, abs_y): (f64, f64),
    reach: f64,
) {
    const CELL: f64 = 300.0;
    let factor = LAYERS[0].factor;
    let (ox, oy) = (abs_x * factor, abs_y * factor);
    let seed = view.sky_seed as i64;
    let margin = reach + CELL * 1.5;
    additive(|| {
        for cy in ((oy - margin) / CELL).floor() as i64..=((oy + margin) / CELL).floor() as i64 {
            for cx in ((ox - margin) / CELL).floor() as i64..=((ox + margin) / CELL).floor() as i64
            {
                let h = hash64(&[seed, 99, cx, cy]);
                let (px, py) = (
                    (cx as f64 + f64::from(unit(h, 0))) * CELL,
                    (cy as f64 + f64::from(unit(h, 16))) * CELL,
                );
                let (lx, ly) = ((px - ox) as f32, (py - oy) as f32);
                let (sx, sy) = projection.turn((lx, ly));
                let point = (projection.pivot.0 + sx, projection.pivot.1 + sy);
                let weight = band_weight(px, py);
                if weight > 0.2 {
                    if let Some(cloud) = art.clouds[(h >> 40) as usize % 3].as_ref() {
                        let size = 380.0 + unit(h, 32) * 260.0;
                        let tint = if (h >> 50) & 1 == 0 {
                            palette::LAVENDER
                        } else {
                            palette::SKY
                        };
                        draw_centred(
                            cloud,
                            point,
                            (size, size * 0.6),
                            BAND_ANGLE.to_degrees() as f32 - projection.roll.to_degrees(),
                            fade(tint, 0.13 * weight),
                        );
                    }
                } else if unit(h, 48) < 0.06 && projection.visible(point, 20.0) {
                    draw_galaxy(art, point, h, projection.roll);
                }
            }
        }
    });
}

/// A small far galaxy: a tilted glow with a bright core.
fn draw_galaxy(art: &SpaceArt, point: (f32, f32), h: u64, roll: f32) {
    let Some(glow) = art.glow.as_ref() else {
        return;
    };
    let size = 10.0 + unit(h, 20) * 16.0;
    let turn = unit(h, 4) * 180.0 - roll.to_degrees();
    let tint = if (h >> 60) & 1 == 0 {
        palette::LAVENDER
    } else {
        palette::PINK
    };
    draw_centred(glow, point, (size * 2.2, size * 0.7), turn, fade(tint, 0.45));
    draw_centred(glow, point, (size * 0.8, size * 0.4), turn, fade(palette::CREAM, 0.5));
}

/// A soft round glow in five hard steps (a pixel-art halo): white, alpha falling outward.
#[must_use]
pub fn glow_pixels(size: i32) -> Vec<u8> {
    let mut pixels = vec![0_u8; (size * size * 4) as usize];
    let centre = size as f32 * 0.5;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - centre).hypot(y as f32 + 0.5 - centre) / centre).min(1.0);
            let soft = (1.0 - d).powf(2.2);
            let alpha = (soft * 5.0).ceil() / 5.0;
            let at = ((y * size + x) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[255, 255, 255, (alpha * 255.0) as u8]);
        }
    }
    pixels
}

/// A cloud: a few octaves of value noise under a round falloff, in four alpha steps with an
/// ordered dither (chunky pixel-art fog once scaled up).
#[must_use]
pub fn cloud_pixels(size: i32, seed: u32) -> Vec<u8> {
    let noise = |x: f32, y: f32, cell: f32, salt: u32| -> f32 {
        let (gx, gy) = ((x / cell).floor(), (y / cell).floor());
        let (fx, fy) = (x / cell - gx, y / cell - gy);
        let corner = |dx: f32, dy: f32| -> f32 {
            let h = hash64(&[
                i64::from(seed),
                i64::from(salt),
                (gx + dx) as i64,
                (gy + dy) as i64,
            ]);
            unit(h, 0)
        };
        let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        let top = corner(0.0, 0.0) + (corner(1.0, 0.0) - corner(0.0, 0.0)) * sx;
        let bottom = corner(0.0, 1.0) + (corner(1.0, 1.0) - corner(0.0, 1.0)) * sx;
        top + (bottom - top) * sy
    };
    let mut pixels = vec![0_u8; (size * size * 4) as usize];
    let centre = size as f32 * 0.5;
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32, y as f32);
            let value = noise(fx, fy, 24.0, 1) * 0.5
                + noise(fx, fy, 12.0, 2) * 0.3
                + noise(fx, fy, 6.0, 3) * 0.2;
            let d = (fx + 0.5 - centre).hypot(fy + 0.5 - centre) / centre;
            let falloff = (1.0 - d * d).max(0.0);
            let dither = ((x + y * 2) % 4) as f32 * 0.06 - 0.09;
            let level = ((value * falloff * 1.6 + dither) * 4.0).floor().clamp(0.0, 4.0) / 4.0;
            let at = ((y * size + x) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[255, 255, 255, (level * 255.0) as u8]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glows_fall_off_in_steps_and_clouds_fade_at_the_edges() {
        let glow = glow_pixels(32);
        let alpha = |pixels: &[u8], size: i32, x: i32, y: i32| pixels[((y * size + x) * 4 + 3) as usize];
        assert!(alpha(&glow, 32, 16, 16) > 200);
        assert_eq!(alpha(&glow, 32, 0, 0), 0);
        let steps: std::collections::BTreeSet<u8> = glow.chunks(4).map(|px| px[3]).collect();
        assert!(steps.len() <= 6, "{steps:?}");
        let cloud = cloud_pixels(64, 7);
        assert_eq!(alpha(&cloud, 64, 0, 0), 0);
        assert_eq!(cloud, cloud_pixels(64, 7));
    }

    #[test]
    fn the_band_is_brightest_on_its_spine() {
        assert!((band_weight(0.0, 0.0) - 1.0).abs() < 1e-6);
        assert!(band_weight(0.0, 2_000.0) < 0.01);
    }
}
