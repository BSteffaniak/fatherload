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
//! **Time of day** (Part II F4, keyframes in `world/daylight.rs`): the low sky bands follow the
//! clock through dawn pinks and oranges, a deep noon blue, golden hour and dusk purples into a
//! starry night; glow bands sit over the horizon at dawn and dusk; the sun arcs from the left
//! horizon over the top of the sky to the right one; the stars come out at night and wheel
//! slowly round a pole above the view; clouds and hills take the light's tint, and the hills
//! catch a rim of low sun on their sunward slopes. The physical Moon (WP-MOON) hangs still
//! above its column ([`fatherload_simulation::clock::MoonAnchor`]): a huge backdrop body,
//! lit by the sun in its phase, that grows as the camera climbs toward it.
//!
//! Every texture is rasterized once at startup ([`SkyArt`]) into small point-filtered images
//! and tiled across the view; only rectangles and those textures are drawn per frame.

use raylib::prelude::*;

use fatherload_simulation::{
    clock::{MoonAnchor, WorldClock},
    worldgen::{
        WorldSettings,
        biome::{self, Biome},
    },
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

use super::{
    ART, GROUND_Y,
    daylight::{DayLook, Mix, RimSide},
    snap,
    surface::SurfaceArt,
};

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
/// World pixels above the hills' ground line of the tops of the three horizon glow bands.
const GLOW_TOPS: [f32; 3] = [230.0, 400.0, 620.0];
/// Columns either side of the Moon's column over which it slides from the edge of the sky to
/// straight overhead.
const MOON_SWING_TILES: f32 = 6_000.0;
/// The Moon's radius in art pixels seen from the ground far from its column (it looms larger
/// overhead, see [`draw_moon`]).
const MOON_RADIUS: f32 = 22.0;

/// A backdrop style: one per biome.
const BACKDROPS: [Biome; 7] = Biome::ALL;

/// The startup-rasterized backdrop textures.
pub struct SkyArt {
    /// The dither step between two sky bands as a white mask (white = the upper band), tinted
    /// with the band colours of the moment.
    band_mask: Option<Texture2D>,
    /// Far and near hills of each biome, in [`BACKDROPS`] order.
    far: Vec<Option<Texture2D>>,
    near: Vec<Option<Texture2D>>,
    /// The sunward rims of each biome's far and near hills as white masks: `[lit from the left,
    /// lit from the right]`.
    far_rims: Vec<[Option<Texture2D>; 2]>,
    near_rims: Vec<[Option<Texture2D>; 2]>,
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
        let band_mask = texture(band_mask_image());
        let far = BACKDROPS
            .iter()
            .map(|biome| texture(far_image(*biome)))
            .collect();
        let near = BACKDROPS
            .iter()
            .map(|biome| texture(near_image(*biome)))
            .collect();
        let far_rims = BACKDROPS
            .iter()
            .map(|biome| {
                let tops = far_tops(*biome);
                [
                    texture(rim_image(&tops, FAR_HEIGHT, RimSide::Left)),
                    texture(rim_image(&tops, FAR_HEIGHT, RimSide::Right)),
                ]
            })
            .collect();
        let near_rims = BACKDROPS
            .iter()
            .map(|biome| {
                let tops = near_tops(*biome);
                [
                    texture(rim_image(&tops, NEAR_HEIGHT + NEAR_SKIRT, RimSide::Left)),
                    texture(rim_image(&tops, NEAR_HEIGHT + NEAR_SKIRT, RimSide::Right)),
                ]
            })
            .collect();
        let clouds = texture(clouds_image());
        let surface = SurfaceArt::load(&mut texture);
        Self {
            band_mask,
            far,
            near,
            far_rims,
            near_rims,
            clouds,
            surface,
        }
    }
}

/// A `BAND_TEXTURE_WIDTH` x `BAND_DITHER_ROWS` ordered-dither step as a mask: white where the
/// upper band shows, clear where the lower one does.
fn band_mask_image() -> Image {
    let mut image = blank_image(BAND_TEXTURE_WIDTH, BAND_DITHER_ROWS);
    for y in 0..BAND_DITHER_ROWS {
        let mix = (y as f32 + 0.5) / BAND_DITHER_ROWS as f32;
        for x in 0..BAND_TEXTURE_WIDTH {
            if mix * 16.0 <= f32::from(bayer4(x, y)) + 0.5 {
                image.draw_pixel(x, y, Color::WHITE);
            }
        }
    }
    image
}

/// The rim a low sun lights along a ridge whose top row at column `x` is `tops[x]`: the top
/// pixels of slopes facing the sun, and the faces of steps up toward it (white mask).
fn rim_image(tops: &[i32], height: i32, side: RimSide) -> Image {
    let width = tops.len() as i32;
    let mut image = blank_image(width, height);
    for x in 0..width {
        let top = tops[x as usize];
        let toward = match side {
            RimSide::Left => tops[(x - 1).rem_euclid(width) as usize],
            RimSide::Right => tops[(x + 1).rem_euclid(width) as usize],
        };
        if top > toward {
            // The ground falls away from the sun here: in shade.
            continue;
        }
        // The top pixel, plus the face of a step up toward the sun.
        let face = (toward - top).clamp(1, 6);
        for y in top..(top + face).min(height) {
            image.draw_pixel(x, y.max(0), Color::WHITE);
        }
    }
    image
}

// ---------------------------------------------------------------------------------------------
// Backdrop layers.// ---------------------------------------------------------------------------------------------
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

/// The time of day the sky draws: the clock, its keyframed look and where the Moon hangs.
pub struct SkyTime {
    pub clock: WorldClock,
    pub look: DayLook,
    pub moon: MoonAnchor,
}

/// Draws everything behind the terrain: the sky for the view's altitude and the time of day,
/// stars, the horizon glow, the Moon, the sun, clouds and the biome hills.
pub fn draw_sky(
    draw: &mut impl RaylibDraw,
    art: &SkyArt,
    settings: &WorldSettings,
    visible: Rectangle,
    (seconds, time): (f32, &SkyTime),
) {
    if visible.y > GROUND_Y + 12.0 * fatherload_simulation::terrain::TILE_SIZE {
        // Deep underground: the terrain and the world's backdrop cover everything.
        return;
    }
    let horizon = hills_bottom(settings, visible);
    draw_sky_bands(draw, art, settings, visible, &time.look);
    draw_stars(draw, settings, visible, seconds, time);
    draw_horizon_glow(draw, art, visible, horizon, &time.look);
    draw_moon(draw, settings, visible, time);
    draw_sun(draw, visible, horizon, time.clock);
    if let Some(clouds) = &art.clouds {
        draw_clouds(draw, clouds, settings, visible, seconds, time.look.clouds);
    }
    draw_hills(draw, art, settings, visible, &time.look);
}

/// The colour of sky band `index` (top to bottom, [`SKY_BANDS`] order) now: the five low bands
/// follow the time of day; the highest band stays night.
fn band_color(index: usize, look: &DayLook) -> Mix {
    index.checked_sub(1).map_or_else(
        || Mix::solid(SKY_BANDS[0].1),
        |low| look.bands[low.min(look.bands.len() - 1)],
    )
}

/// The flat colour bands of the sky with their dithered steps (band edges on the classic
/// altitude scale, stretched over a V8 sky by [`super::sky_lands::world_y_of_gradient`]).
fn draw_sky_bands(
    draw: &mut impl RaylibDraw,
    art: &SkyArt,
    settings: &WorldSettings,
    visible: Rectangle,
    look: &DayLook,
) {
    let left = snap(visible.x) - ART * 2.0;
    let width = visible.width + ART * 6.0;
    let (top, bottom) = (visible.y - ART, visible.y + visible.height + ART);
    let step = BAND_DITHER_ROWS as f32 * ART;
    let mut upper_edge = f32::NEG_INFINITY;
    let mut color = Mix::solid(SPACE);
    for (index, (edge, _)) in SKY_BANDS.iter().enumerate() {
        let lower = band_color(index, look);
        let edge = &super::sky_lands::world_y_of_gradient(settings, *edge);
        // Band `color` spans upper_edge..edge (minus the dither step at its bottom).
        let band_top = upper_edge.max(top);
        let band_bottom = (edge - step * 0.5).min(bottom);
        if band_bottom > band_top {
            color.fill(
                draw,
                Rectangle::new(left, band_top, width, band_bottom - band_top),
            );
        }
        let step_top = edge - step * 0.5;
        if step_top < bottom && step_top + step > top {
            lower.fill(draw, Rectangle::new(left, step_top, width, step));
            if let Some(texture) = &art.band_mask {
                let tile = BAND_TEXTURE_WIDTH as f32 * ART;
                let first = (left / tile).floor() as i32;
                let last = ((left + width) / tile).ceil() as i32;
                let source =
                    Rectangle::new(0.0, 0.0, texture.width() as f32, texture.height() as f32);
                for repeat in first..=last {
                    color.stamp(
                        draw,
                        texture,
                        source,
                        Rectangle::new(repeat as f32 * tile, step_top, tile, step),
                    );
                }
            }
        }
        upper_edge = step_top + step;
        color = lower;
    }
    if bottom > upper_edge {
        color.fill(
            draw,
            Rectangle::new(
                left,
                upper_edge.max(top),
                width,
                bottom - upper_edge.max(top),
            ),
        );
    }
}

/// Glow bands over the horizon at dawn and dusk (and a faint haze at noon and night), each
/// with a dithered top edge, anchored on the hills' ground line `horizon`.
fn draw_horizon_glow(
    draw: &mut impl RaylibDraw,
    art: &SkyArt,
    visible: Rectangle,
    horizon: f32,
    look: &DayLook,
) {
    let left = snap(visible.x) - ART * 2.0;
    let width = visible.width + ART * 6.0;
    let step = BAND_DITHER_ROWS as f32 * ART;
    let bottom = horizon + NEAR_SKIRT as f32 * ART;
    for (index, (from, to)) in look.glow.iter().enumerate() {
        let band_top = snap(horizon - GLOW_TOPS[index]);
        if band_top > visible.y + visible.height || bottom < visible.y {
            continue;
        }
        for wash in [from, to] {
            if wash.alpha == 0 {
                continue;
            }
            let tint = with_alpha(wash.color, wash.alpha);
            let body_top = band_top + step;
            draw.draw_rectangle_rec(
                Rectangle::new(left, body_top, width, (bottom - body_top).max(0.0)),
                tint,
            );
            if let Some(texture) = &art.band_mask {
                // The mask is white for the upper part: flipped, it fades the glow out upward.
                let tile = BAND_TEXTURE_WIDTH as f32 * ART;
                let first = (left / tile).floor() as i32;
                let last = ((left + width) / tile).ceil() as i32;
                let source = Rectangle::new(
                    0.0,
                    texture.height() as f32,
                    texture.width() as f32,
                    -(texture.height() as f32),
                );
                for repeat in first..=last {
                    draw.draw_texture_pro(
                        texture,
                        source,
                        Rectangle::new(repeat as f32 * tile, band_top, tile, step),
                        Vector2::zero(),
                        0.0,
                        tint,
                    );
                }
            }
        }
    }
}

/// How many stars are out at world y `y` (0 none, 1 all).
fn star_share(y: f32) -> f32 {
    ((STARS_FROM_Y - y) / (STARS_FROM_Y - STARS_FULL_Y)).clamp(0.0, 1.0)
}

/// Stars: a field drifting at a tenth of the camera's speed, out high up (toward space) and
/// everywhere at night, wheeling slowly round a pole above the view (a full turn a day) and
/// twinkling. Altitude is read on the classic scale, stretched over a V8 sky.
fn draw_stars(
    draw: &mut impl RaylibDraw,
    settings: &WorldSettings,
    visible: Rectangle,
    seconds: f32,
    time: &SkyTime,
) {
    const PARALLAX: f32 = 0.1;
    const CELL: f32 = 44.0;
    let star_share = |y: f32| star_share(super::sky_lands::gradient_y(settings, y));
    let night = time.look.stars;
    if night <= 0.0 && star_share(visible.y + visible.height) <= 0.0 && star_share(visible.y) <= 0.0
    {
        return;
    }
    // The pole hangs above the middle of the view; the field turns about it.
    let pole = Vector2::new(
        visible.x + visible.width * 0.5,
        visible.y - visible.height * 0.6,
    );
    let angle = time.clock.day_fraction() * std::f32::consts::TAU;
    let (sin, cos) = angle.sin_cos();
    let rotate = |x: f32, y: f32, sin: f32| {
        let (dx, dy) = (x - pole.x, y - pole.y);
        (pole.x + dx * cos - dy * sin, pole.y + dx * sin + dy * cos)
    };
    let (origin_x, origin_y) = (visible.x * (1.0 - PARALLAX), visible.y * (1.0 - PARALLAX));
    // The view's corners turned back into the unrotated field bound the cells to visit.
    let corners = [
        (visible.x, visible.y),
        (visible.x + visible.width, visible.y),
        (visible.x, visible.y + visible.height),
        (visible.x + visible.width, visible.y + visible.height),
    ]
    .map(|(x, y)| rotate(x, y, -sin));
    let (min_x, max_x, min_y, max_y) = corners.iter().fold(
        (f32::MAX, f32::MIN, f32::MAX, f32::MIN),
        |(a, b, c, d), (x, y)| (a.min(*x), b.max(*x), c.min(*y), d.max(*y)),
    );
    let first_x = ((min_x - origin_x) / CELL).floor() as i32;
    let last_x = ((max_x - origin_x) / CELL).ceil() as i32;
    let first_y = ((min_y - origin_y) / CELL).floor() as i32;
    let last_y = ((max_y - origin_y) / CELL).ceil() as i32;
    let twinkle = (seconds * 2.0) as u32;
    // The Moon is solid: no star shows through its disc (it is drawn later, translucent by day).
    let (moon_x, moon_y, moon_radius) = moon_disc(visible, time.moon);
    let moon_reach = (moon_radius as f32 + 1.0) * ART;
    // A backdrop Moon faded out on a V8 world (the real Moon took over) hides no stars.
    let moon_solid = super::sky_lands::backdrop_moon_share(settings, visible) > 0.5;
    let behind_moon = |x: f32, y: f32| {
        if !moon_solid {
            return false;
        }
        let (dx, dy) = (x - moon_x, y - moon_y);
        dx.mul_add(dx, dy * dy) <= moon_reach * moon_reach
    };
    for cy in first_y..=last_y {
        for cx in first_x..=last_x {
            let h = hash(cx, cy, 91);
            if h % 100 >= 34 {
                continue;
            }
            let field_x = origin_x + cx as f32 * CELL + ((h >> 8) % 40) as f32;
            let field_y = origin_y + cy as f32 * CELL + ((h >> 16) % 40) as f32;
            let (x, y) = rotate(field_x, field_y, sin);
            if x < visible.x - ART
                || x > visible.x + visible.width
                || y < visible.y - ART
                || y > visible.y + visible.height
            {
                continue;
            }
            // Each star has its own threshold, so they come out one by one (with altitude
            // toward space, and with the night).
            let share = star_share(y).max(night);
            if ((h >> 24) % 100) as f32 >= share * 100.0 {
                continue;
            }
            let (x, y) = (snap(x), snap(y));
            if behind_moon(x, y) {
                continue;
            }
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

/// The Moon (WP-MOON): a huge body hanging still above its column, drawn as a backdrop. Seen
/// from far off it rides low toward its side of the sky; under it, it is straight overhead,
/// and it grows as the camera climbs toward it. The sun lights it in its phase (waxing lit on
/// the right, toward the sunset); by day it is a pale ghost, at night it glows, with the dark
/// part faintly earthlit. Colour runs draw as single rectangles (the browser feels every quad).
#[allow(
    clippy::too_many_lines,
    reason = "placement, phase and the run-merged disc read best together"
)]
/// Where the Moon's disc sits in a view: centre (world pixels) and radius (art pixels). Seen
/// from far off it rides low toward its side of the sky; under it, it is straight overhead, and
/// it grows as the camera climbs toward it.
fn moon_disc(visible: Rectangle, moon: MoonAnchor) -> (f32, f32, i32) {
    use fatherload_simulation::terrain::TILE_SIZE;
    let center_x = visible.x + visible.width * 0.5;
    let center_y = visible.y + visible.height * 0.5;
    let moon_x = (moon.column as f32 + 0.5) * TILE_SIZE;
    let swing = ((moon_x - center_x) / (MOON_SWING_TILES * TILE_SIZE)).clamp(-1.0, 1.0);
    // Altitude toward the Moon (0 on the ground, 1 at its underside).
    let moon_y = moon.row as f32 * TILE_SIZE;
    let climb = ((GROUND_Y - center_y) / (GROUND_Y - moon_y).max(1.0)).clamp(0.0, 1.0);
    let grow = climb.sqrt().mul_add(5.0, 1.0);
    let overhead = swing.abs().mul_add(-0.75, 1.75);
    let radius = (MOON_RADIUS * overhead * grow).round() as i32;
    let cx = snap(visible.width.mul_add(swing * 0.42, center_x));
    let lift = swing.abs().mul_add(0.25, 0.2) * (1.0 - climb);
    let cy =
        snap(visible.height.mul_add(lift, visible.y) + radius as f32 * ART * (0.4 - climb * 0.4));
    (cx, cy, radius)
}

fn draw_moon(
    draw: &mut impl RaylibDraw,
    settings: &WorldSettings,
    visible: Rectangle,
    time: &SkyTime,
) {
    let center_y = super::sky_lands::gradient_y(settings, visible.y + visible.height * 0.5);
    // On a V8 world the real Moon's looming underside takes over from the continents up.
    let share = super::sky_lands::backdrop_moon_share(settings, visible);
    if share <= 0.0 {
        return;
    }
    let (cx, cy, radius) = moon_disc(visible, time.moon);
    let r = radius as f32 * ART;
    if cx + r < visible.x
        || cx - r > visible.x + visible.width
        || cy - r > visible.y + visible.height
    {
        return;
    }
    // Only the rows inside the view are walked (the disc is huge up close).
    let first_row = (((visible.y - cy) / ART).floor() as i32).max(-radius);
    let last_row = (((visible.y + visible.height - cy) / ART).ceil() as i32).min(radius);
    let night = time.look.stars.max(star_share(center_y));
    let lit_alpha = (night.mul_add(175.0, 80.0) * share).round() as u8;
    let dark_alpha = (night * 200.0 * share).round() as u8;
    let cycle = time.clock.moon_cycle();
    let terminator = (cycle * std::f32::consts::TAU).cos();
    let waxing = cycle < 0.5;
    let craters = [
        (-0.3, -0.25, 0.18),
        (0.25, 0.15, 0.22),
        (-0.05, 0.42, 0.14),
        (0.42, -0.35, 0.1),
        (-0.48, 0.28, 0.11),
        (0.05, -0.05, 0.08),
    ];
    for row in first_row..=last_row {
        let v = row as f32 / radius as f32;
        let half_width = (1.0 - v * v).max(0.0).sqrt();
        let half = (half_width * radius as f32).round() as i32;
        let mut run: Option<(i32, Option<Color>)> = None;
        let flush =
            |draw: &mut dyn FnMut(Rectangle, Color), start: i32, end: i32, color: Option<Color>| {
                if let Some(color) = color {
                    draw(
                        Rectangle::new(
                            (start as f32).mul_add(ART, cx),
                            (row as f32).mul_add(ART, cy),
                            (end - start) as f32 * ART,
                            ART,
                        ),
                        color,
                    );
                }
            };
        let mut paint = |rect: Rectangle, color: Color| draw.draw_rectangle_rec(rect, color);
        for column in -half..=half {
            let u = column as f32 / radius as f32;
            let lit = if waxing {
                u > terminator * half_width
            } else {
                u < -terminator * half_width
            };
            let crater = craters.iter().any(|(x, y, size)| {
                let (dx, dy) = (u - x, v - y);
                dx * dx + dy * dy <= size * size
            });
            let rim = half - column.abs() <= 1;
            let color = if lit {
                let base = if crater {
                    FOG
                } else if rim {
                    BONE
                } else if hash(column, row, 101).is_multiple_of(9) {
                    MIST
                } else {
                    ICE
                };
                Some(with_alpha(base, lit_alpha))
            } else if dark_alpha > 0 {
                let base = if crater || bayer4(column, row) < 3 {
                    SLATE_DARK
                } else {
                    NIGHT
                };
                Some(with_alpha(base, dark_alpha))
            } else {
                None
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

/// The sun on its arc (presentation of [`WorldClock::sun_angle`]): rising on the left horizon,
/// over the top of the sky at noon, setting on the right; big and orange near the horizon,
/// small and white-gold high up. It rises from behind the hills (`horizon` is their ground
/// line), and keeps its arc inside the view when the camera is high above the ground.
fn draw_sun(draw: &mut impl RaylibDraw, visible: Rectangle, horizon: f32, clock: WorldClock) {
    let angle = clock.sun_angle();
    let height = angle.sin();
    if height < -0.05 {
        return;
    }
    let base = (horizon - 24.0).min(visible.y + visible.height * 1.02);
    let zenith = visible.height.mul_add(0.16, visible.y);
    let cx = snap(visible.width.mul_add(0.5 - 0.42 * angle.cos(), visible.x));
    let cy = snap((base - zenith).mul_add(-height.max(0.0), base));
    let low = (1.0 - height * 4.0).clamp(0.0, 1.0);
    if low > 0.0 {
        // The low sun's glare: wide soft discs (still glowing over the ridge it sets behind).
        for (radius, color, alpha) in [(46, ORANGE, 26.0), (32, AMBER, 34.0), (22, GOLD, 40.0)] {
            let alpha = (alpha * low).round() as u8;
            for row in -radius..=radius {
                let half = (((radius * radius - row * row) as f32).sqrt()).round() as i32;
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        cx - half as f32 * ART,
                        cy + row as f32 * ART,
                        (half * 2 + 1) as f32 * ART,
                        ART,
                    ),
                    with_alpha(color, alpha),
                );
            }
        }
    }
    let rings: [(i32, Color); 3] = if low > 0.5 {
        [(14, with_alpha(ORANGE, 80)), (10, ORANGE), (8, AMBER)]
    } else if low > 0.0 {
        [(12, with_alpha(AMBER, 75)), (9, AMBER), (7, GOLD)]
    } else {
        [(11, with_alpha(CREAM, 70)), (8, GOLD), (6, CREAM)]
    };
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
#[derive(Clone, Copy)]
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

fn draw_clouds(
    draw: &mut impl RaylibDraw,
    clouds: &Texture2D,
    settings: &WorldSettings,
    visible: Rectangle,
    seconds: f32,
    tint: Mix,
) {
    for band in &CLOUD_BANDS {
        // Bands sit on the classic altitude scale, stretched over a V8 sky.
        let band = &CloudBand {
            low: super::sky_lands::world_y_of_gradient(settings, band.low),
            high: super::sky_lands::world_y_of_gradient(settings, band.high),
            ..*band
        };
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
                let source = Rectangle::new(
                    (variant * CLOUD_WIDTH) as f32,
                    0.0,
                    CLOUD_WIDTH as f32,
                    CLOUD_HEIGHT as f32,
                );
                let dest = Rectangle::new(snap(x), snap(y), width, height);
                // Lit by the sun's colour of the moment (a crossfade between keyframes).
                draw.draw_texture_pro(
                    clouds,
                    source,
                    dest,
                    Vector2::zero(),
                    0.0,
                    with_alpha(tint.base, band.alpha),
                );
                if tint.t > 0 && tint.over != tint.base {
                    let alpha = (u16::from(band.alpha) * u16::from(tint.t) / 255) as u8;
                    draw.draw_texture_pro(
                        clouds,
                        source,
                        dest,
                        Vector2::zero(),
                        0.0,
                        with_alpha(tint.over, alpha),
                    );
                }
            }
        }
    }
}

/// The far and near hills of the biome under the view's centre, cross-faded with the
/// neighbouring biome's near a boundary, tinted by the light of the moment, with a rim of low
/// sun along their sunward slopes at dawn and dusk.
fn draw_hills(
    draw: &mut impl RaylibDraw,
    art: &SkyArt,
    settings: &WorldSettings,
    visible: Rectangle,
    look: &DayLook,
) {
    let center = ((visible.x + visible.width * 0.5) / fatherload_simulation::terrain::TILE_SIZE)
        .floor() as i32;
    let blend = biome::biome_blend(settings, center);
    let index = |biome: Biome| BACKDROPS.iter().position(|b| *b == biome).unwrap_or(0);
    let anchor = hills_bottom(settings, visible);
    let rim_slot = match look.rim_side {
        RimSide::Left => 0,
        RimSide::Right => 1,
    };
    let layers: [HillLayer<'_>; 2] = [
        (&art.far, &art.far_rims, 0.18, anchor),
        (
            &art.near,
            &art.near_rims,
            0.42,
            (NEAR_SKIRT as f32).mul_add(ART, anchor),
        ),
    ];
    let paint = |draw: &mut dyn FnMut(&Texture2D, Color),
                 textures: &[Option<Texture2D>],
                 rims: &[[Option<Texture2D>; 2]],
                 biome: Biome,
                 weight: u8| {
        if let Some(Some(texture)) = textures.get(index(biome)) {
            draw(texture, with_alpha(look.hills.base, weight));
            if look.hills.t > 0 && look.hills.over != look.hills.base {
                let alpha = (u16::from(weight) * u16::from(look.hills.t) / 255) as u8;
                draw(texture, with_alpha(look.hills.over, alpha));
            }
        }
        if look.rim.alpha > 0
            && let Some(Some(rim)) = rims.get(index(biome)).map(|pair| &pair[rim_slot])
        {
            let alpha = (u16::from(weight) * u16::from(look.rim.alpha) / 255) as u8;
            draw(rim, with_alpha(look.rim.color, alpha));
        }
    };
    for (textures, rims, parallax, bottom) in layers {
        let mut layer = |texture: &Texture2D, tint: Color| {
            draw_layer(draw, texture, visible, parallax, bottom, tint);
        };
        paint(&mut layer, textures, rims, blend.primary, 255);
        if blend.mix > 0 && blend.secondary != blend.primary {
            // `mix` is the neighbour's weight in 256ths (at most half, on the boundary).
            paint(&mut layer, textures, rims, blend.secondary, blend.mix);
        }
    }
}

/// One backdrop layer: its biome textures, their rim masks, its parallax and its bottom.
type HillLayer<'a> = (
    &'a [Option<Texture2D>],
    &'a [[Option<Texture2D>; 2]],
    f32,
    f32,
);

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
