//! The daytime backdrop above the ground line: a dithered sky gradient, a pixel sun, drifting
//! clouds and two layers of hills, each scrolling at its own parallax rate.
//!
//! The gradient, hill silhouettes and cloud shapes are rasterized once at startup into small
//! point-filtered textures ([`SkyArt`]) and tiled across the view.

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::{
    pixel_art::{shade::bayer4, sprite::blank_image},
    terrain::tile_art::hash,
};

use super::{ART, GROUND_Y, snap};

/// World y of the top of the sky gradient (above it is solid `DEEP_BLUE`).
const SKY_TOP: f32 = -800.0;
const SKY_TEXTURE_WIDTH: i32 = 64;
const FAR_HILLS_WIDTH: i32 = 320;
const FAR_HILLS_HEIGHT: i32 = 60;
const NEAR_HILLS_WIDTH: i32 = 256;
const NEAR_HILLS_HEIGHT: i32 = 40;
const CLOUD_WIDTH: i32 = 56;
const CLOUD_HEIGHT: i32 = 18;
const CLOUD_VARIANTS: i32 = 3;

/// Gradient stops from the top of the sky (0) to the horizon (1).
const SKY_STOPS: [(f32, Color); 5] = [
    (0.0, DEEP_BLUE),
    (0.3, OCEAN),
    (0.58, SKY),
    (0.84, SKY_LIGHT),
    (0.985, ICE),
];

/// The startup-rasterized sky textures.
pub struct SkyArt {
    gradient: Option<Texture2D>,
    far_hills: Option<Texture2D>,
    near_hills: Option<Texture2D>,
    clouds: Option<Texture2D>,
}

impl SkyArt {
    pub fn load(raylib: &mut RaylibHandle, thread: &RaylibThread) -> Self {
        let mut texture = |image: Image| {
            let texture = raylib.load_texture_from_image(thread, &image).ok();
            if let Some(texture) = &texture {
                texture.set_texture_filter(thread, TextureFilter::TEXTURE_FILTER_POINT);
            }
            texture
        };
        Self {
            gradient: texture(gradient_image()),
            far_hills: texture(far_hills_image()),
            near_hills: texture(near_hills_image()),
            clouds: texture(clouds_image()),
        }
    }
}

fn art_height_of_sky() -> i32 {
    ((GROUND_Y - SKY_TOP) / ART) as i32
}

/// Rows of dither between two sky bands.
const DITHER_ROWS: f32 = 5.0;

/// The sky colour at gradient position `t` for art pixel `(x, y)` of a gradient `height` rows
/// tall: flat bands with a short dithered step into the next colour.
fn sky_color(t: f32, height: f32, x: i32, y: i32) -> Color {
    let mut index = 0;
    while index + 1 < SKY_STOPS.len() && t > SKY_STOPS[index + 1].0 {
        index += 1;
    }
    if index + 1 >= SKY_STOPS.len() {
        return SKY_STOPS[index].1;
    }
    let from = SKY_STOPS[index].1;
    let (end, to) = SKY_STOPS[index + 1];
    let rows_left = (end - t) * height;
    let mix = (1.0 - rows_left / DITHER_ROWS).clamp(0.0, 1.0);
    if mix * 16.0 > f32::from(bayer4(x, y)) + 0.5 {
        to
    } else {
        from
    }
}

fn gradient_image() -> Image {
    let height = art_height_of_sky();
    let mut image = blank_image(SKY_TEXTURE_WIDTH, height);
    for y in 0..height {
        let t = (y as f32 + 0.5) / height as f32;
        for x in 0..SKY_TEXTURE_WIDTH {
            image.draw_pixel(x, y, sky_color(t, height as f32, x, y));
        }
    }
    image
}

/// A periodic ridge height (art pixels above the image bottom) at column `x` of `width`.
fn ridge(x: i32, width: i32, base: f32, waves: &[(f32, f32, f32)]) -> i32 {
    let phase = x as f32 / width as f32 * std::f32::consts::TAU;
    let height = waves
        .iter()
        .fold(base, |sum, (frequency, amplitude, offset)| {
            (phase * frequency + offset).sin().mul_add(*amplitude, sum)
        });
    height.round() as i32
}

fn far_hills_image() -> Image {
    let mut image = blank_image(FAR_HILLS_WIDTH, FAR_HILLS_HEIGHT);
    for x in 0..FAR_HILLS_WIDTH {
        let top = FAR_HILLS_HEIGHT
            - ridge(
                x,
                FAR_HILLS_WIDTH,
                30.0,
                &[(2.0, 12.0, 0.4), (5.0, 6.0, 1.3), (11.0, 2.5, 2.0)],
            );
        for y in top.max(0)..FAR_HILLS_HEIGHT {
            let depth = y - top;
            let color = if depth < 4 && top < 22 {
                // Snow on the high peaks.
                if depth == 0 || !hash(x, y, 3).is_multiple_of(3) {
                    ICE
                } else {
                    SKY_LIGHT
                }
            } else if depth == 0 {
                SKY_LIGHT
            } else if (x + depth / 3) % 9 == 0 && depth > 3 {
                OCEAN
            } else {
                SKY
            };
            image.draw_pixel(x, y, color);
        }
    }
    image
}

fn near_hills_image() -> Image {
    let mut image = blank_image(NEAR_HILLS_WIDTH, NEAR_HILLS_HEIGHT);
    let tops: Vec<i32> = (0..NEAR_HILLS_WIDTH)
        .map(|x| {
            NEAR_HILLS_HEIGHT
                - ridge(
                    x,
                    NEAR_HILLS_WIDTH,
                    16.0,
                    &[(3.0, 6.0, 0.9), (7.0, 3.0, 0.2), (13.0, 1.5, 1.1)],
                )
        })
        .collect();
    for (x, top) in (0..NEAR_HILLS_WIDTH).zip(tops.iter().copied()) {
        for y in top.max(0)..NEAR_HILLS_HEIGHT {
            let depth = y - top;
            let color = if depth == 0 {
                LEAF
            } else if depth > 10 && bayer4(x, y) < ((depth - 10) * 2).min(16) as u8 {
                PINE
            } else {
                MOSS
            };
            image.draw_pixel(x, y, color);
        }
    }
    // Little pines along the ridge.
    for x in (3..NEAR_HILLS_WIDTH - 3).step_by(5) {
        if !hash(x, 0, 51).is_multiple_of(3) {
            continue;
        }
        let base = tops[x as usize] + 1;
        let tall = 5 + (hash(x, 1, 51) % 4) as i32;
        for row in 0..tall {
            let half = (row * 2 / 3).min(2);
            for dx in -half..=half {
                let color = if dx == -half { MOSS } else { PINE };
                image.draw_pixel(x + dx, base - tall + row, color);
            }
        }
    }
    image
}

fn clouds_image() -> Image {
    let mut image = blank_image(CLOUD_WIDTH * CLOUD_VARIANTS, CLOUD_HEIGHT);
    for variant in 0..CLOUD_VARIANTS {
        let left = variant * CLOUD_WIDTH;
        let puffs = 4 + variant;
        let mut discs = Vec::new();
        for puff in 0..puffs {
            let h = hash(variant, puff, 61);
            let span = CLOUD_WIDTH - 20;
            let cx = 10 + span * puff / (puffs - 1).max(1) + (h % 5) as i32 - 2;
            let radius =
                5 + ((h >> 4) % 4) as i32 + if puff > 0 && puff < puffs - 1 { 2 } else { 0 };
            let cy = CLOUD_HEIGHT - 4 - radius / 2;
            discs.push((cx, cy, radius));
        }
        for y in 0..CLOUD_HEIGHT {
            for x in 0..CLOUD_WIDTH {
                let inside = |px: i32, py: i32| {
                    py < CLOUD_HEIGHT - 3
                        && discs.iter().any(|(cx, cy, r)| {
                            let (dx, dy) = (px - cx, py - cy);
                            dx * dx + dy * dy <= r * r
                        })
                };
                if !inside(x, y) {
                    continue;
                }
                let color = if !inside(x, y + 2) {
                    MIST
                } else if !inside(x - 1, y - 2) {
                    ICE
                } else {
                    BONE
                };
                image.draw_pixel(left + x, y, color);
            }
        }
    }
    image
}

/// Draws `texture` repeated horizontally across `visible`, scrolled for a layer at `parallax`
/// (0 = fixed to the screen, 1 = fixed to the world), bottom at world `bottom`.
fn draw_layer(
    draw: &mut impl RaylibDraw,
    texture: &Texture2D,
    visible: Rectangle,
    parallax: f32,
    bottom: f32,
) {
    let width = texture.width() as f32 * ART;
    let height = texture.height() as f32 * ART;
    let origin = snap(visible.x * (1.0 - parallax));
    let first = ((visible.x - origin) / width).floor() as i32;
    let last = ((visible.x + visible.width - origin) / width).ceil() as i32;
    for repeat in first..=last {
        let x = origin + repeat as f32 * width;
        draw.draw_texture_pro(
            texture,
            Rectangle::new(0.0, 0.0, texture.width() as f32, texture.height() as f32),
            Rectangle::new(x, bottom - height, width, height),
            Vector2::zero(),
            0.0,
            Color::WHITE,
        );
    }
}

/// Draws everything behind the town: sky, sun, clouds and hills.
pub fn draw_sky(draw: &mut impl RaylibDraw, art: &SkyArt, visible: Rectangle, seconds: f32) {
    if visible.y > GROUND_Y {
        return;
    }
    let left = snap(visible.x) - ART;
    let width = visible.width + ART * 4.0;
    // Above the gradient: deep blue.
    if visible.y < SKY_TOP {
        draw.draw_rectangle_rec(
            Rectangle::new(left, visible.y - ART, width, SKY_TOP - visible.y + ART),
            DEEP_BLUE,
        );
    }
    if let Some(gradient) = &art.gradient {
        let tile = gradient.width() as f32 * ART;
        let first = (left / tile).floor() as i32;
        let last = ((left + width) / tile).ceil() as i32;
        for repeat in first..=last {
            draw.draw_texture_pro(
                gradient,
                Rectangle::new(0.0, 0.0, gradient.width() as f32, gradient.height() as f32),
                Rectangle::new(
                    repeat as f32 * tile,
                    SKY_TOP,
                    tile,
                    gradient.height() as f32 * ART,
                ),
                Vector2::zero(),
                0.0,
                Color::WHITE,
            );
        }
    }
    draw_sun(draw, visible);
    if let Some(clouds) = &art.clouds {
        draw_clouds(draw, clouds, visible, seconds);
    }
    if let Some(far) = &art.far_hills {
        draw_layer(draw, far, visible, 0.18, GROUND_Y + ART * 2.0);
    }
    if let Some(near) = &art.near_hills {
        draw_layer(draw, near, visible, 0.42, GROUND_Y + ART * 2.0);
    }
}

/// A pixel sun high in the sky, almost fixed to the screen.
fn draw_sun(draw: &mut impl RaylibDraw, visible: Rectangle) {
    let cx = snap(visible.x + visible.width * 0.8);
    let cy = snap(-260.0 + (visible.y + 200.0) * 0.85);
    let rings = [(11, palette_glow(CREAM)), (8, GOLD), (6, CREAM)];
    for (radius, color) in rings {
        for row in -radius..=radius {
            let half = (((radius * radius - row * row) as f32).sqrt()).round() as i32;
            draw.draw_rectangle_rec(
                Rectangle::new(
                    cx - half as f32 * ART,
                    cy + row as f32 * ART,
                    (half * 2 + 1) as f32 * ART,
                    ART,
                ),
                color,
            );
        }
    }
}

const fn palette_glow(color: Color) -> Color {
    crate::rendering::pixel_art::palette::with_alpha(color, 70)
}

fn draw_clouds(draw: &mut impl RaylibDraw, clouds: &Texture2D, visible: Rectangle, seconds: f32) {
    const PERIOD: f32 = 2_400.0;
    const COUNT: i32 = 7;
    let parallax = 0.3;
    let drift = seconds * 6.0;
    let origin = visible.x * (1.0 - parallax) + drift;
    let first = ((visible.x - origin) / PERIOD).floor() as i32 - 1;
    let last = ((visible.x + visible.width - origin) / PERIOD).ceil() as i32;
    for repeat in first..=last {
        for index in 0..COUNT {
            let h = hash(index, 0, 71);
            let x = origin
                + repeat as f32 * PERIOD
                + index as f32 * PERIOD / COUNT as f32
                + (h % 160) as f32;
            let y = -40.0 - ((h >> 8) % 340) as f32;
            let variant = ((h >> 16) % CLOUD_VARIANTS as u32) as i32;
            draw.draw_texture_pro(
                clouds,
                Rectangle::new(
                    (variant * CLOUD_WIDTH) as f32,
                    0.0,
                    CLOUD_WIDTH as f32,
                    CLOUD_HEIGHT as f32,
                ),
                Rectangle::new(
                    snap(x),
                    snap(y),
                    CLOUD_WIDTH as f32 * ART,
                    CLOUD_HEIGHT as f32 * ART,
                ),
                Vector2::zero(),
                0.0,
                Color::WHITE,
            );
        }
    }
}
