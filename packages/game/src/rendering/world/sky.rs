//! The backdrop behind the world: an endless sky, a pixel sun, clouds and two layers of hills.
//!
//! The sky has no top. Its colour follows altitude in flat dithered bands, from the pale
//! horizon through day blues into night and the black of space, where stars come out (rigs may
//! fly up without limit, so the sky keeps going however high the camera is). Clouds drift in
//! three height bands: low puffs over the ground, big banks at mountain-peak height and thin
//! cirrus far up.
//!
//! The hills behind the ground are per biome (WP-SURFACE): plains hills, forested ridges,
//! dunes, mesas, snowy downs, a jagged mountain range and marsh, each a far and a near layer
//! scrolling at its own parallax rate. Near a biome boundary the two biomes' backdrops cross-
//! fade by the heightmap's own blend weight, so walking or flying between biomes is seamless.
//!
//! Every texture is rasterized once at startup ([`SkyArt`]) into small point-filtered images
//! and tiled across the view; only rectangles and those textures are drawn per frame.

use raylib::prelude::*;

use fatherload_simulation::worldgen::{
    WorldSettings,
    biome::{self, Biome},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::{
    pixel_art::{shade::bayer4, sprite::blank_image},
    terrain::tile_art::hash,
};

use super::{ART, GROUND_Y, snap, surface::SurfaceArt};

const BAND_TEXTURE_WIDTH: i32 = 64;
/// Art rows of dither between two sky bands.
const BAND_DITHER_ROWS: i32 = 12;
const FAR_WIDTH: i32 = 320;
const FAR_HEIGHT: i32 = 64;
const NEAR_WIDTH: i32 = 256;
const NEAR_HEIGHT: i32 = 44;
/// Art rows the near hills continue below their bottom line in their own colours, so ground
/// lower than the hills' anchor (dunes, lake basins) still has hills behind it.
const NEAR_SKIRT: i32 = 128;
const CLOUD_WIDTH: i32 = 56;
const CLOUD_HEIGHT: i32 = 18;
const CLOUD_VARIANTS: i32 = 3;

/// The sky's colour bands by altitude: each entry is the world y where a band's lower edge
/// meets the band below it, with the band's colour (top to bottom). Above the first edge is
/// [`SPACE`].
const SKY_BANDS: [(f32, Color); 6] = [
    (-20_000.0, NIGHT),
    (-12_000.0, DEEP_BLUE),
    (-6_500.0, OCEAN),
    (-3_000.0, SKY),
    (-500.0, SKY_LIGHT),
    (130.0, ICE),
];
/// The colour above the highest band: space.
const SPACE: Color = INK;
/// World y where the first stars appear, and where every star is out.
const STARS_FROM_Y: f32 = -7_000.0;
const STARS_FULL_Y: f32 = -16_000.0;
/// World y above which the moon rises into view, and where it is fully up.
const MOON_FROM_Y: f32 = -14_000.0;
const MOON_FULL_Y: f32 = -24_000.0;

/// A backdrop style: one per biome.
const BACKDROPS: [Biome; 7] = Biome::ALL;

/// The startup-rasterized backdrop textures.
pub struct SkyArt {
    /// Dither steps between consecutive sky bands (`SKY_BANDS.len()` of them, the first from
    /// space into the first band).
    bands: Vec<Option<Texture2D>>,
    /// Far and near hills of each biome, in [`BACKDROPS`] order.
    far: Vec<Option<Texture2D>>,
    near: Vec<Option<Texture2D>>,
    clouds: Option<Texture2D>,
    /// Flora sprites, water and weather art.
    pub(super) surface: SurfaceArt,
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
        let mut upper = SPACE;
        let mut bands = Vec::with_capacity(SKY_BANDS.len());
        for (_, lower) in SKY_BANDS {
            bands.push(texture(band_step_image(upper, lower)));
            upper = lower;
        }
        let far = BACKDROPS
            .iter()
            .map(|biome| texture(far_image(*biome)))
            .collect();
        let near = BACKDROPS
            .iter()
            .map(|biome| texture(near_image(*biome)))
            .collect();
        let clouds = texture(clouds_image());
        let surface = SurfaceArt::load(&mut texture);
        Self {
            bands,
            far,
            near,
            clouds,
            surface,
        }
    }
}

/// A `BAND_TEXTURE_WIDTH` x `BAND_DITHER_ROWS` ordered-dither step from `upper` to `lower`.
fn band_step_image(upper: Color, lower: Color) -> Image {
    let mut image = blank_image(BAND_TEXTURE_WIDTH, BAND_DITHER_ROWS);
    for y in 0..BAND_DITHER_ROWS {
        let mix = (y as f32 + 0.5) / BAND_DITHER_ROWS as f32;
        for x in 0..BAND_TEXTURE_WIDTH {
            let color = if mix * 16.0 > f32::from(bayer4(x, y)) + 0.5 {
                lower
            } else {
                upper
            };
            image.draw_pixel(x, y, color);
        }
    }
    image
}

// ---------------------------------------------------------------------------------------------
// Backdrop layers.

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

/// Paints a silhouette layer: `tops[x]` is the first painted row of column `x`, and `color`
/// picks each pixel from its column, row and depth below the top.
fn silhouette(
    width: i32,
    height: i32,
    tops: &[i32],
    color: impl Fn(i32, i32, i32) -> Color,
) -> Image {
    let mut image = blank_image(width, height);
    for (x, top) in (0..width).zip(tops.iter().copied()) {
        for y in top.max(0)..height {
            image.draw_pixel(x, y, color(x, y, y - top));
        }
    }
    image
}

/// Little conifers along a ridge (`body` with a lit left edge).
fn pines_along(image: &mut Image, tops: &[i32], step: usize, salt: u32, colors: (Color, Color)) {
    let width = tops.len() as i32;
    for x in (3..width - 3).step_by(step) {
        if !hash(x, 0, salt).is_multiple_of(3) {
            continue;
        }
        let base = tops[x as usize] + 1;
        let tall = 5 + (hash(x, 1, salt) % 4) as i32;
        for row in 0..tall {
            let half = (row * 2 / 3).min(2);
            for dx in -half..=half {
                let color = if dx == -half { colors.0 } else { colors.1 };
                image.draw_pixel(x + dx, base - tall + row, color);
            }
        }
    }
}

fn far_tops(biome: Biome) -> Vec<i32> {
    let w = FAR_WIDTH;
    (0..w)
        .map(|x| {
            let lift = match biome {
                Biome::Plains => ridge(
                    x,
                    w,
                    30.0,
                    &[(2.0, 12.0, 0.4), (5.0, 6.0, 1.3), (11.0, 2.5, 2.0)],
                ),
                Biome::Forest => ridge(
                    x,
                    w,
                    28.0,
                    &[(3.0, 8.0, 1.1), (7.0, 4.0, 0.3), (17.0, 1.5, 2.2)],
                ),
                Biome::Desert => ridge(
                    x,
                    w,
                    16.0,
                    &[(2.0, 5.0, 0.7), (5.0, 4.0, 2.1), (9.0, 1.0, 0.2)],
                ),
                Biome::Badlands => {
                    // Flat-topped buttes: a stepped profile.
                    let raw = ridge(x, w, 20.0, &[(3.0, 14.0, 0.9), (7.0, 6.0, 2.4)]);
                    (raw / 9) * 9 + 6
                }
                Biome::Tundra => ridge(x, w, 22.0, &[(2.0, 8.0, 1.9), (6.0, 3.0, 0.5)]),
                Biome::Mountains => {
                    // Jagged peaks: sharpened ridges.
                    let phase = x as f32 / w as f32 * std::f32::consts::TAU;
                    let a = (phase * 3.0 + 0.6).sin().abs();
                    let b = (phase * 7.0 + 1.7).sin().abs();
                    let c = (phase * 19.0 + 0.2).sin();
                    (a.mul_add(30.0, b * 14.0) + c * 2.0 + 14.0).round() as i32
                }
                Biome::Wetlands => ridge(x, w, 14.0, &[(2.0, 4.0, 0.2), (5.0, 2.0, 1.4)]),
            };
            FAR_HEIGHT - lift.clamp(2, FAR_HEIGHT - 1)
        })
        .collect()
}

fn far_image(biome: Biome) -> Image {
    let tops = far_tops(biome);
    silhouette(FAR_WIDTH, FAR_HEIGHT, &tops, |x, y, depth| {
        let top = y - depth;
        match biome {
            Biome::Plains | Biome::Forest | Biome::Wetlands => {
                if depth < 4 && top < 24 && biome == Biome::Plains {
                    // Snow on the high peaks.
                    if depth == 0 || !hash(x, y, 3).is_multiple_of(3) {
                        ICE
                    } else {
                        SKY_LIGHT
                    }
                } else if depth == 0 {
                    SKY_LIGHT
                } else if ((x + depth / 3) % 9 == 0 && depth > 3)
                    || (biome == Biome::Forest && depth < 3 && hash(x, 0, 5).is_multiple_of(2))
                {
                    // Shadowed folds (and, on forested ridges, a ragged treeline).
                    OCEAN
                } else {
                    SKY
                }
            }
            Biome::Desert => {
                if depth == 0 {
                    CREAM
                } else if depth > 6 && bayer4(x, y) < ((depth - 6) * 2).min(10) as u8 {
                    CLAY
                } else {
                    SAND
                }
            }
            Biome::Badlands => {
                if depth == 0 {
                    AMBER
                } else if (y + 2) % 9 < 2 {
                    RUST
                } else {
                    CLAY
                }
            }
            Biome::Tundra => {
                if depth == 0 {
                    BONE
                } else if depth > 5 && hash(x, y, 7).is_multiple_of(4) {
                    FOG
                } else {
                    MIST
                }
            }
            Biome::Mountains => {
                let snowline = 30;
                if top + depth < snowline && depth < 10 {
                    if depth == 0 || !hash(x, y, 9).is_multiple_of(4) {
                        BONE
                    } else {
                        ICE
                    }
                } else if depth == 0 {
                    SKY_LIGHT
                } else if (x * 2 + depth).rem_euclid(13) == 0 {
                    OCEAN
                } else {
                    SKY
                }
            }
        }
    })
}

fn near_tops(biome: Biome) -> Vec<i32> {
    let w = NEAR_WIDTH;
    (0..w)
        .map(|x| {
            let lift = match biome {
                Biome::Plains => ridge(
                    x,
                    w,
                    16.0,
                    &[(3.0, 6.0, 0.9), (7.0, 3.0, 0.2), (13.0, 1.5, 1.1)],
                ),
                Biome::Forest => ridge(
                    x,
                    w,
                    20.0,
                    &[(2.0, 7.0, 0.3), (5.0, 4.0, 2.0), (23.0, 1.2, 0.4)],
                ),
                Biome::Desert => {
                    // Dunes: steep lee faces, long windward slopes.
                    let phase = (x as f32 / w as f32 * 4.0).fract();
                    let dune = if phase < 0.7 {
                        phase / 0.7
                    } else {
                        (1.0 - phase) / 0.3
                    };
                    (dune.mul_add(14.0, 6.0) + (x as f32 * 0.11).sin() * 2.0).round() as i32
                }
                Biome::Badlands => {
                    let raw = ridge(x, w, 18.0, &[(2.0, 12.0, 2.2), (5.0, 7.0, 0.4)]);
                    (raw / 7) * 7 + 4
                }
                Biome::Tundra => ridge(x, w, 14.0, &[(3.0, 5.0, 1.4), (9.0, 2.0, 0.6)]),
                Biome::Mountains => {
                    let phase = x as f32 / w as f32 * std::f32::consts::TAU;
                    let a = (phase * 2.0 + 2.0).sin().abs();
                    let b = (phase * 5.0 + 0.4).sin().abs();
                    (a.mul_add(18.0, b * 10.0) + 10.0).round() as i32
                }
                Biome::Wetlands => ridge(x, w, 9.0, &[(4.0, 2.0, 0.5), (11.0, 1.0, 1.7)]),
            };
            NEAR_HEIGHT - lift.clamp(2, NEAR_HEIGHT - 1)
        })
        .collect()
}

fn near_image(biome: Biome) -> Image {
    let tops = near_tops(biome);
    let (edge, body, deep) = match biome {
        Biome::Plains => (LEAF, MOSS, PINE),
        Biome::Forest => (MOSS, PINE, PINE),
        Biome::Desert => (CREAM, SAND, CLAY),
        Biome::Badlands => (COPPER, RUST, MAROON),
        Biome::Tundra => (ICE, MIST, FOG),
        Biome::Mountains => (SLATE_LIGHT, SLATE, SLATE_DARK),
        Biome::Wetlands => (LEAF, MOSS, TEAL_DARK),
    };
    let mut image = silhouette(
        NEAR_WIDTH,
        NEAR_HEIGHT + NEAR_SKIRT,
        &tops,
        |x, y, depth| {
            if depth == 0 {
                edge
            } else if biome == Biome::Badlands && (y + 1) % 7 == 0 {
                MAROON
            } else if biome == Biome::Mountains && y - depth < 14 && depth < 4 {
                BONE
            } else if depth > 10 && bayer4(x, y) < ((depth - 10) * 2).min(16) as u8 {
                deep
            } else {
                body
            }
        },
    );
    match biome {
        Biome::Plains => pines_along(&mut image, &tops, 5, 51, (MOSS, PINE)),
        Biome::Forest => pines_along(&mut image, &tops, 3, 52, (MOSS, PINE)),
        Biome::Tundra => pines_along(&mut image, &tops, 7, 53, (FOG, SLATE)),
        Biome::Mountains => pines_along(&mut image, &tops, 4, 54, (SLATE, PINE)),
        Biome::Wetlands => {
            // Reeds poking up along the marsh.
            for x in (2..NEAR_WIDTH - 2).step_by(3) {
                if hash(x, 2, 55).is_multiple_of(2) {
                    let base = tops[x as usize];
                    let tall = 2 + (hash(x, 3, 55) % 4) as i32;
                    for row in 0..tall {
                        image.draw_pixel(x, base - row, if row == tall - 1 { LIME } else { MOSS });
                    }
                }
            }
        }
        Biome::Desert | Biome::Badlands => {}
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

// ---------------------------------------------------------------------------------------------
// Drawing.

/// Draws `texture` repeated horizontally across `visible`, scrolled for a layer at `parallax`
/// (0 = fixed to the screen, 1 = fixed to the world), bottom at world `bottom`, tinted `tint`.
fn draw_layer(
    draw: &mut impl RaylibDraw,
    texture: &Texture2D,
    visible: Rectangle,
    parallax: f32,
    bottom: f32,
    tint: Color,
) {
    let width = texture.width() as f32 * ART;
    let height = texture.height() as f32 * ART;
    if bottom < visible.y || bottom - height > visible.y + visible.height {
        return;
    }
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
            tint,
        );
    }
}

/// Draws everything behind the terrain: the sky for the view's altitude, stars, the sun,
/// clouds and the biome hills.
pub fn draw_sky(
    draw: &mut impl RaylibDraw,
    art: &SkyArt,
    settings: &WorldSettings,
    visible: Rectangle,
    seconds: f32,
) {
    if visible.y > GROUND_Y + 12.0 * fatherload_simulation::terrain::TILE_SIZE {
        // Deep underground: the terrain and the world's backdrop cover everything.
        return;
    }
    draw_sky_bands(draw, art, visible);
    draw_stars(draw, visible, seconds);
    draw_moon(draw, visible);
    draw_sun(draw, visible);
    if let Some(clouds) = &art.clouds {
        draw_clouds(draw, clouds, visible, seconds);
    }
    draw_hills(draw, art, settings, visible);
}

/// The flat colour bands of the sky with their dithered steps.
fn draw_sky_bands(draw: &mut impl RaylibDraw, art: &SkyArt, visible: Rectangle) {
    let left = snap(visible.x) - ART * 2.0;
    let width = visible.width + ART * 6.0;
    let (top, bottom) = (visible.y - ART, visible.y + visible.height + ART);
    let step = BAND_DITHER_ROWS as f32 * ART;
    let mut upper_edge = f32::NEG_INFINITY;
    let mut color = SPACE;
    for (index, (edge, lower)) in SKY_BANDS.iter().enumerate() {
        // Band `color` spans upper_edge..edge (minus the dither step at its bottom).
        let band_top = upper_edge.max(top);
        let band_bottom = (edge - step * 0.5).min(bottom);
        if band_bottom > band_top {
            draw.draw_rectangle_rec(
                Rectangle::new(left, band_top, width, band_bottom - band_top),
                color,
            );
        }
        let step_top = edge - step * 0.5;
        if step_top < bottom
            && step_top + step > top
            && let Some(Some(texture)) = art.bands.get(index)
        {
            let tile = BAND_TEXTURE_WIDTH as f32 * ART;
            let first = (left / tile).floor() as i32;
            let last = ((left + width) / tile).ceil() as i32;
            for repeat in first..=last {
                draw.draw_texture_pro(
                    texture,
                    Rectangle::new(0.0, 0.0, texture.width() as f32, texture.height() as f32),
                    Rectangle::new(repeat as f32 * tile, step_top, tile, step),
                    Vector2::zero(),
                    0.0,
                    Color::WHITE,
                );
            }
        }
        upper_edge = step_top + step;
        color = *lower;
    }
    if bottom > upper_edge {
        draw.draw_rectangle_rec(
            Rectangle::new(
                left,
                upper_edge.max(top),
                width,
                bottom - upper_edge.max(top),
            ),
            color,
        );
    }
}

/// How many stars are out at world y `y` (0 none, 1 all).
fn star_share(y: f32) -> f32 {
    ((STARS_FROM_Y - y) / (STARS_FROM_Y - STARS_FULL_Y)).clamp(0.0, 1.0)
}

/// Stars high in the sky: a field drifting at a tenth of the camera's speed, thinning out
/// toward the day sky and twinkling.
fn draw_stars(draw: &mut impl RaylibDraw, visible: Rectangle, seconds: f32) {
    const PARALLAX: f32 = 0.1;
    const CELL: f32 = 44.0;
    if star_share(visible.y + visible.height) <= 0.0 && star_share(visible.y) <= 0.0 {
        return;
    }
    let (origin_x, origin_y) = (visible.x * (1.0 - PARALLAX), visible.y * (1.0 - PARALLAX));
    let first_x = ((visible.x - origin_x) / CELL).floor() as i32;
    let last_x = ((visible.x + visible.width - origin_x) / CELL).ceil() as i32;
    let first_y = ((visible.y - origin_y) / CELL).floor() as i32;
    let last_y = ((visible.y + visible.height - origin_y) / CELL).ceil() as i32;
    let twinkle = (seconds * 2.0) as u32;
    for cy in first_y..=last_y {
        for cx in first_x..=last_x {
            let h = hash(cx, cy, 91);
            if h % 100 >= 34 {
                continue;
            }
            let x = origin_x + cx as f32 * CELL + ((h >> 8) % 40) as f32;
            let y = origin_y + cy as f32 * CELL + ((h >> 16) % 40) as f32;
            // Each star has its own altitude threshold, so they come out one by one.
            let share = star_share(y);
            if ((h >> 24) % 100) as f32 >= share * 100.0 {
                continue;
            }
            let (x, y) = (snap(x), snap(y));
            let bright = (h >> 5).is_multiple_of(9);
            let color = match (h >> 12) % 6 {
                0 => CREAM,
                1 => ICE,
                2 => LAVENDER,
                _ => BONE,
            };
            let flicker = (h.wrapping_add(twinkle.wrapping_mul(0x9E37))).is_multiple_of(7);
            let color = if flicker { MIST } else { color };
            draw.draw_rectangle_rec(Rectangle::new(x, y, ART, ART), color);
            if bright && !flicker {
                let glow = with_alpha(color, 120);
                for (dx, dy) in [(-1.0_f32, 0.0_f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    draw.draw_rectangle_rec(
                        Rectangle::new(dx.mul_add(ART, x), dy.mul_add(ART, y), ART, ART),
                        glow,
                    );
                }
            }
        }
    }
}

/// A big cratered moon that rises into view out in space, almost fixed to the screen.
fn draw_moon(draw: &mut impl RaylibDraw, visible: Rectangle) {
    let center_y = visible.y + visible.height * 0.5;
    let share = ((MOON_FROM_Y - center_y) / (MOON_FROM_Y - MOON_FULL_Y)).clamp(0.0, 1.0);
    if share <= 0.0 {
        return;
    }
    let radius = 26;
    let cx = snap(visible.x + visible.width * 0.24);
    // It rises from below the view's bottom edge as the camera climbs.
    let rest = visible.y + visible.height * 0.3;
    let cy = snap((1.0 - share).mul_add(visible.height * 0.9, rest));
    for row in -radius..=radius {
        let half = (((radius * radius - row * row) as f32).sqrt()).round() as i32;
        // Runs of one colour along the row draw as one rectangle (a few hundred quads per
        // frame instead of one per moon pixel: the browser build feels every draw call).
        let mut run: Option<(i32, Color)> = None;
        let flush = |draw: &mut dyn FnMut(Rectangle, Color), start: i32, end: i32, color| {
            draw(
                Rectangle::new(
                    (start as f32).mul_add(ART, cx),
                    (row as f32).mul_add(ART, cy),
                    (end - start) as f32 * ART,
                    ART,
                ),
                color,
            );
        };
        let mut paint = |rect: Rectangle, color: Color| draw.draw_rectangle_rec(rect, color);
        for column in -half..=half {
            // Lit from the sun's side (right), with a soft terminator and craters.
            let lit = column as f32 + row as f32 * 0.3 > -(radius as f32) * 0.35;
            let crater = [
                (-8, -6, 5),
                (7, 4, 6),
                (-2, 12, 4),
                (12, -10, 3),
                (-14, 8, 3),
            ]
            .iter()
            .any(|(x, y, r)| {
                let (dx, dy) = (column - x, row - y);
                dx * dx + dy * dy <= r * r
            });
            let rim = half - column.abs() <= 1 && column > 0;
            let color = if !lit {
                if bayer4(column, row) < 3 {
                    SLATE
                } else {
                    SLATE_DARK
                }
            } else if rim {
                BONE
            } else if crater {
                FOG
            } else if hash(column, row, 101).is_multiple_of(9) {
                MIST
            } else {
                ICE
            };
            match run {
                Some((_, run_color)) if run_color == color => {}
                Some((start, run_color)) => {
                    flush(&mut paint, start, column, run_color);
                    run = Some((column, color));
                }
                None => run = Some((column, color)),
            }
        }
        if let Some((start, run_color)) = run {
            flush(&mut paint, start, half + 1, run_color);
        }
    }
}

/// A pixel sun high in the sky, almost fixed to the screen.
fn draw_sun(draw: &mut impl RaylibDraw, visible: Rectangle) {
    let cx = snap(visible.x + visible.width * 0.8);
    let cy =
        snap(visible.y + visible.height * 0.18 + ((visible.y + 200.0) * 0.04).clamp(-40.0, 120.0));
    let rings = [(11, with_alpha(CREAM, 70)), (8, GOLD), (6, CREAM)];
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

/// One band of clouds: where it floats, how big its clouds are and how it moves.
struct CloudBand {
    /// World y of the band's lowest and highest cloud.
    low: f32,
    high: f32,
    /// Horizontal stretch and vertical size in art pixels per cloud pixel.
    stretch: f32,
    scale: f32,
    parallax: f32,
    drift: f32,
    alpha: u8,
    period: f32,
    count: i32,
    salt: u32,
}

const CLOUD_BANDS: [CloudBand; 3] = [
    // Thin cirrus far up.
    CloudBand {
        low: -7_200.0,
        high: -9_400.0,
        stretch: 4.0,
        scale: 0.6,
        parallax: 0.55,
        drift: 3.0,
        alpha: 130,
        period: 3_600.0,
        count: 6,
        salt: 73,
    },
    // Big banks at mountain-peak height.
    CloudBand {
        low: -1_500.0,
        high: -3_000.0,
        stretch: 2.2,
        scale: 1.8,
        parallax: 0.75,
        drift: 4.0,
        alpha: 225,
        period: 3_000.0,
        count: 5,
        salt: 72,
    },
    // Low puffs over the ground.
    CloudBand {
        low: -40.0,
        high: -380.0,
        stretch: 1.0,
        scale: 1.0,
        parallax: 0.3,
        drift: 6.0,
        alpha: 255,
        period: 2_400.0,
        count: 7,
        salt: 71,
    },
];

fn draw_clouds(draw: &mut impl RaylibDraw, clouds: &Texture2D, visible: Rectangle, seconds: f32) {
    for band in &CLOUD_BANDS {
        let height = CLOUD_HEIGHT as f32 * ART * band.scale;
        if band.low < visible.y - height || band.high > visible.y + visible.height + height {
            continue;
        }
        let width = CLOUD_WIDTH as f32 * ART * band.scale * band.stretch;
        let origin = visible.x * (1.0 - band.parallax) + seconds * band.drift;
        let first = ((visible.x - origin - width) / band.period).floor() as i32 - 1;
        let last = ((visible.x + visible.width - origin) / band.period).ceil() as i32;
        let span = (band.low - band.high).max(1.0) as u32;
        for repeat in first..=last {
            for index in 0..band.count {
                let h = hash(index, repeat.rem_euclid(5), band.salt);
                let x = origin
                    + repeat as f32 * band.period
                    + index as f32 * band.period / band.count as f32
                    + (h % 160) as f32;
                let y = band.low - ((h >> 8) % span) as f32;
                let variant = ((h >> 16) % CLOUD_VARIANTS as u32) as i32;
                draw.draw_texture_pro(
                    clouds,
                    Rectangle::new(
                        (variant * CLOUD_WIDTH) as f32,
                        0.0,
                        CLOUD_WIDTH as f32,
                        CLOUD_HEIGHT as f32,
                    ),
                    Rectangle::new(snap(x), snap(y), width, height),
                    Vector2::zero(),
                    0.0,
                    Color::new(255, 255, 255, band.alpha),
                );
            }
        }
    }
}

/// The far and near hills of the biome under the view's centre, cross-faded with the
/// neighbouring biome's near a boundary.
fn draw_hills(
    draw: &mut impl RaylibDraw,
    art: &SkyArt,
    settings: &WorldSettings,
    visible: Rectangle,
) {
    let center = ((visible.x + visible.width * 0.5) / fatherload_simulation::terrain::TILE_SIZE)
        .floor() as i32;
    let blend = biome::biome_blend(settings, center);
    let index = |biome: Biome| BACKDROPS.iter().position(|b| *b == biome).unwrap_or(0);
    let anchor = hills_bottom(settings, visible);
    let layers: [(&[Option<Texture2D>], f32, f32); 2] = [
        (&art.far, 0.18, anchor),
        (&art.near, 0.42, (NEAR_SKIRT as f32).mul_add(ART, anchor)),
    ];
    for (textures, parallax, bottom) in layers {
        if let Some(Some(primary)) = textures.get(index(blend.primary)) {
            draw_layer(draw, primary, visible, parallax, bottom, Color::WHITE);
        }
        if blend.mix > 0
            && blend.secondary != blend.primary
            && let Some(Some(secondary)) = textures.get(index(blend.secondary))
        {
            // `mix` is the neighbour's weight in 256ths (at most half, on the boundary).
            draw_layer(
                draw,
                secondary,
                visible,
                parallax,
                bottom,
                Color::new(255, 255, 255, blend.mix),
            );
        }
    }
}

/// World y of the bottom of the hills backdrop for a view: the classic ground line, or in a
/// biome world the ground averaged over a wide window around the view's centre (so the hills
/// stand behind raised and lowered ground alike, and drift smoothly while scrolling).
#[must_use]
pub fn hills_bottom(settings: &WorldSettings, visible: Rectangle) -> f32 {
    use fatherload_simulation::terrain::TILE_SIZE;
    const REACH: i32 = 72;
    const STEP: usize = 6;
    if !settings.generator.has_biomes() {
        return GROUND_Y + ART * 2.0;
    }
    let center = ((visible.x + visible.width * 0.5) / TILE_SIZE).floor() as i32;
    // Weighted towards the centre (a tent filter), so features entering the window ease in.
    let (mut sum, mut weights) = (0.0_f32, 0.0_f32);
    for x in (center - REACH..=center + REACH).step_by(STEP) {
        let weight = (REACH + 1 - (x - center).abs()) as f32;
        sum += biome::surface_row(settings, x) as f32 * weight;
        weights += weight;
    }
    snap((sum / weights.max(1.0)).mul_add(TILE_SIZE, ART * 2.0))
}
