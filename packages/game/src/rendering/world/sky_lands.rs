//! The high sky of a V8 world (expansion plan F6): cloud seas below the island bands, wind and
//! updraft cues, the Moon looming overhead as a rig climbs toward it, and the altitude scale of
//! the sky's colour gradient.
//!
//! Islands, continents and the Moon themselves are terrain tiles (`terrain::tile_art::sky_art`);
//! this module draws what makes the sky readable around them:
//!
//! * **Cloud seas**: thick banks of cloud floating just under the low isles, inside the
//!   archipelago and at the floor and ceiling of the dead-air gap, drawn behind the terrain with
//!   a slow horizontal parallax so the islands seem to float over them.
//! * **Wind streaks**: thin white dashes drifting at the local wind's speed and direction
//!   ([`environment::wind_at`]), denser where it blows harder: the crosswinds of the dead-air
//!   gap read at a glance.
//! * **Updrafts**: a faint mint column of rising motes over every updraft
//!   ([`sky::updrafts_touching`]), so a pilot can spot the free ride up.
//! * **The Moon from below**: from the continents up, the Moon's underside hangs in the sky
//!   above its column, small and far at first and growing as the rig climbs (a perspective
//!   stand-in until its real tiles come into view).
//! * **Gradient scale** ([`gradient_y`], [`world_y_of_gradient`]): a V8 sky stretches the
//!   classic altitude colour bands over its own bands, so the islands float in daylight blue,
//!   the dead-air gap deepens toward navy, and black space with stars begins at orbit.
//!
//! Everything is a pure function of the world settings, the view and the clock, and is skipped
//! entirely on older worlds and below the island bands.

use raylib::prelude::*;

use fatherload_simulation::{
    environment,
    terrain::TILE_SIZE,
    worldgen::{
        TileRect, WorldSettings,
        sky::{
            self, ARCHIPELAGO_BOTTOM_ROWS, CONTINENTS_BOTTOM_ROWS, DEAD_AIR_BOTTOM_ROWS,
            LOW_ISLES_BOTTOM_ROWS, MOON_BOTTOM_ROWS, ORBIT_BOTTOM_ROWS,
        },
    },
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::terrain::tile_art::hash;

use super::{ART, snap};

/// World pixel y of `altitude_rows` rows above row 0.
#[allow(clippy::cast_precision_loss, reason = "band rows are small integers")]
fn y_of(altitude_rows: i32) -> f32 {
    -(altitude_rows as f32) * TILE_SIZE
}

/// Knots of the gradient scale: (real altitude in rows, gradient world y in pixels). Between
/// knots the scale is linear; below the first it is the identity; above the last it continues
/// at one gradient pixel per world pixel.
const fn gradient_knots() -> [(i32, f32); 7] {
    [
        (0, 0.0),
        (LOW_ISLES_BOTTOM_ROWS, -900.0),
        (ARCHIPELAGO_BOTTOM_ROWS, -1_800.0),
        (DEAD_AIR_BOTTOM_ROWS, -3_900.0),
        (CONTINENTS_BOTTOM_ROWS, -10_500.0),
        (ORBIT_BOTTOM_ROWS, -17_000.0),
        (MOON_BOTTOM_ROWS, -22_000.0),
    ]
}

/// The sky-gradient y (the classic altitude scale the sky's colour bands, stars and clouds are
/// authored in) of real world y `world_y`: the identity on worlds without a sky.
#[must_use]
pub fn gradient_y(settings: &WorldSettings, world_y: f32) -> f32 {
    if !settings.generator.has_sky() || world_y >= 0.0 {
        return world_y;
    }
    let knots = gradient_knots();
    for pair in knots.windows(2) {
        let (low, high) = (y_of(pair[0].0), y_of(pair[1].0));
        if world_y >= high {
            let t = (world_y - low) / (high - low);
            return (pair[1].1 - pair[0].1).mul_add(t, pair[0].1);
        }
    }
    let (last_rows, last_gradient) = knots[knots.len() - 1];
    last_gradient + (world_y - y_of(last_rows))
}

/// The real world y whose sky-gradient y is `gradient` (the inverse of [`gradient_y`]).
#[must_use]
pub fn world_y_of_gradient(settings: &WorldSettings, gradient: f32) -> f32 {
    if !settings.generator.has_sky() || gradient >= 0.0 {
        return gradient;
    }
    let knots = gradient_knots();
    for pair in knots.windows(2) {
        if gradient >= pair[1].1 {
            let t = (gradient - pair[0].1) / (pair[1].1 - pair[0].1);
            let (low, high) = (y_of(pair[0].0), y_of(pair[1].0));
            return (high - low).mul_add(t, low);
        }
    }
    let (last_rows, last_gradient) = knots[knots.len() - 1];
    y_of(last_rows) + (gradient - last_gradient)
}

/// Whether the view reaches into a V8 sky's island bands or above.
fn in_high_sky(settings: &WorldSettings, visible: Rectangle) -> bool {
    settings.generator.has_sky() && visible.y < y_of(LOW_ISLES_BOTTOM_ROWS - 120)
}

/// Everything of the high sky drawn behind the terrain (after the sky's gradient, stars and
/// sun): the Moon from below, cloud seas, updraft columns and wind streaks.
pub fn draw_sky_backdrop(
    draw: &mut impl RaylibDraw,
    settings: &WorldSettings,
    visible: Rectangle,
    seconds: f32,
) {
    if !in_high_sky(settings, visible) {
        return;
    }
    draw_moon_from_below(draw, settings, visible);
    draw_cloud_seas(draw, visible, seconds);
    draw_updrafts(draw, settings, visible, seconds);
    draw_wind(draw, settings, visible, seconds);
}

// ---------------------------------------------------------------------------------------------
// Cloud seas.

/// A cloud sea: the altitude of its top (rows), its thickness (rows), and its colours.
struct CloudSea {
    top_rows: i32,
    thickness: i32,
    light: Color,
    body: Color,
    shade: Color,
    salt: u32,
}

/// The cloud seas, low to high.
const fn cloud_seas() -> [CloudSea; 4] {
    [
        // Just under the low isles: the first sea a climbing rig breaks through.
        CloudSea {
            top_rows: LOW_ISLES_BOTTOM_ROWS - 10,
            thickness: 26,
            light: BONE,
            body: ICE,
            shade: SKY_LIGHT,
            salt: 501,
        },
        // Inside the archipelago: islands' waterfalls pour into it.
        CloudSea {
            top_rows: ARCHIPELAGO_BOTTOM_ROWS + 260,
            thickness: 40,
            light: BONE,
            body: MIST,
            shade: FOG,
            salt: 502,
        },
        // The dead-air gap's floor: the last soft cloud before the empty, windy dark.
        CloudSea {
            top_rows: DEAD_AIR_BOTTOM_ROWS - 40,
            thickness: 60,
            light: MIST,
            body: FOG,
            shade: SLATE_LIGHT,
            salt: 503,
        },
        // The continents ride on a sea of their own, high and cold.
        CloudSea {
            top_rows: CONTINENTS_BOTTOM_ROWS - 60,
            thickness: 44,
            light: ICE,
            body: FOG,
            shade: SLATE,
            salt: 504,
        },
    ]
}

/// Puffs along a cloud sea's top edge are this far apart (world pixels).
const PUFF_STEP: f32 = 96.0;
/// The seas drift sideways at this share of the camera's motion.
const SEA_PARALLAX: f32 = 0.55;

fn draw_cloud_seas(draw: &mut impl RaylibDraw, visible: Rectangle, seconds: f32) {
    let left = snap(visible.x) - ART * 2.0;
    let width = visible.width + ART * 6.0;
    for sea in cloud_seas() {
        let top = y_of(sea.top_rows);
        #[allow(clippy::cast_precision_loss, reason = "a few dozen rows")]
        let bottom = top + sea.thickness as f32 * TILE_SIZE;
        if bottom + PUFF_STEP < visible.y || top - PUFF_STEP > visible.y + visible.height {
            continue;
        }
        // The body: a dithered band between the puffed top and bottom edges.
        let body_top = top.max(visible.y - ART);
        let body_bottom = bottom.min(visible.y + visible.height + ART);
        if body_bottom > body_top {
            draw.draw_rectangle_rec(
                Rectangle::new(left, body_top, width, body_bottom - body_top),
                with_alpha(sea.body, 215),
            );
        }
        let origin = visible.x * (1.0 - SEA_PARALLAX) + seconds * 5.0;
        let first = ((visible.x - origin) / PUFF_STEP).floor() as i32 - 2;
        let last = ((visible.x + visible.width - origin) / PUFF_STEP).ceil() as i32 + 2;
        for index in first..=last {
            let h = hash(index, sea.top_rows, sea.salt);
            let cx = origin + index as f32 * PUFF_STEP + (h % 40) as f32;
            let radius = 18 + (h >> 8) as i32 % 18;
            // A bright puffed top...
            draw_puff(
                draw,
                cx,
                top + ART * 2.0,
                radius,
                (sea.light, sea.body),
                true,
            );
            // A smaller puff between, so the top rolls instead of standing in a row.
            let small = 8 + (h >> 12) as i32 % 10;
            draw_puff(
                draw,
                cx + PUFF_STEP * 0.5,
                top + ART * 2.0,
                small,
                (sea.light, sea.body),
                true,
            );
            // ...and a shaded, ragged underside.
            let under = 6 + (h >> 16) as i32 % 9;
            draw_puff(
                draw,
                cx + PUFF_STEP * 0.5,
                bottom,
                under,
                (sea.shade, sea.body),
                false,
            );
        }
    }
}

/// A half-disc cloud puff of `radius` art pixels centred on `(cx, base)`: bulging up (`up`)
/// from a top edge or down from a bottom edge; its rim in `colors.0`, its body `colors.1`.
fn draw_puff(
    draw: &mut impl RaylibDraw,
    cx: f32,
    base: f32,
    radius: i32,
    colors: (Color, Color),
    up: bool,
) {
    let cx = snap(cx);
    for row in 0..=radius {
        let half = (((radius * radius - row * row) as f32).sqrt()).round() as i32;
        let y = if up {
            base - (row as f32 + 1.0) * ART
        } else {
            base + row as f32 * ART
        };
        let color = if row >= radius - 1 || (up && row * 3 >= radius * 2) {
            colors.0
        } else {
            colors.1
        };
        draw.draw_rectangle_rec(
            Rectangle::new(cx - half as f32 * ART, y, (half * 2 + 1) as f32 * ART, ART),
            if up { color } else { with_alpha(color, 225) },
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Wind and updrafts.

/// Wind streak cells: (world pixels across, rows tall).
const STREAK_CELL: (f32, i32) = (150.0, 2);

/// Thin dashes drifting with the wind: one candidate per cell, more of them lit (and longer)
/// where the wind blows harder.
fn draw_wind(
    draw: &mut impl RaylibDraw,
    settings: &WorldSettings,
    visible: Rectangle,
    seconds: f32,
) {
    let first_row = (visible.y / TILE_SIZE).floor() as i32;
    let last_row = ((visible.y + visible.height) / TILE_SIZE).ceil() as i32;
    let mut row = first_row.div_euclid(STREAK_CELL.1) * STREAK_CELL.1;
    while row <= last_row {
        let column = ((visible.x + visible.width * 0.5) / TILE_SIZE).floor() as i32;
        #[allow(clippy::cast_precision_loss, reason = "rows are small integers")]
        let wind = environment::wind_at(settings, column, -(row as f32));
        let strength = (wind.abs() / 160.0).min(1.0);
        if strength > 0.06 {
            let offset = seconds * wind * 1.6;
            let first = ((visible.x - offset) / STREAK_CELL.0).floor() as i32 - 1;
            let last = ((visible.x + visible.width - offset) / STREAK_CELL.0).ceil() as i32;
            for cell in first..=last {
                let h = hash(cell, row, 611);
                if (h % 100) as f32 >= strength * 45.0 {
                    continue;
                }
                let x = offset + cell as f32 * STREAK_CELL.0 + (h >> 8) as f32 % STREAK_CELL.0;
                #[allow(clippy::cast_precision_loss, reason = "rows are small integers")]
                let y = (row as f32).mul_add(TILE_SIZE, ((h >> 16) % 24) as f32);
                let length = (4.0 + 30.0 * strength) * ART * (0.5 + ((h >> 24) % 8) as f32 / 8.0);
                let alpha = (60.0 + 120.0 * strength) as u8;
                draw.draw_rectangle_rec(
                    Rectangle::new(snap(x), snap(y), snap(length), ART),
                    with_alpha(BONE, alpha),
                );
                // A fainter tail behind the streak.
                let tail = if wind > 0.0 { -length * 0.6 } else { length };
                draw.draw_rectangle_rec(
                    Rectangle::new(snap(x + tail), snap(y), snap(length * 0.6), ART),
                    with_alpha(ICE, alpha / 3),
                );
            }
        }
        row += STREAK_CELL.1;
    }
}

/// Every updraft in view: a faint mint column with motes rising through it.
fn draw_updrafts(
    draw: &mut impl RaylibDraw,
    settings: &WorldSettings,
    visible: Rectangle,
    seconds: f32,
) {
    let rect = TileRect {
        min: fatherload_simulation::terrain::TilePosition {
            x: (visible.x / TILE_SIZE).floor() as i32 - 1,
            y: (visible.y / TILE_SIZE).floor() as i32 - 1,
        },
        max: fatherload_simulation::terrain::TilePosition {
            x: ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + 1,
            y: ((visible.y + visible.height) / TILE_SIZE).ceil() as i32 + 1,
        },
    };
    for updraft in sky::updrafts_touching(settings, &rect) {
        let left = updraft.rect.min.x as f32 * TILE_SIZE;
        let right = (updraft.rect.max.x + 1) as f32 * TILE_SIZE;
        let top = updraft.rect.min.y as f32 * TILE_SIZE;
        let bottom = (updraft.rect.max.y + 1) as f32 * TILE_SIZE;
        let fade = sky::UPDRAFT_TAPER_ROWS as f32 * TILE_SIZE;
        // The column: a soft tint, fading out over the taper at its top.
        let solid_top = (top + fade).max(visible.y);
        let solid_bottom = bottom.min(visible.y + visible.height);
        if solid_bottom > solid_top {
            draw.draw_rectangle_rec(
                Rectangle::new(left, solid_top, right - left, solid_bottom - solid_top),
                with_alpha(MINT, 26),
            );
        }
        for step in 0..4 {
            let band_top = top + fade * step as f32 / 4.0;
            draw.draw_rectangle_rec(
                Rectangle::new(left, band_top, right - left, fade / 4.0),
                with_alpha(MINT, (6 + step * 5) as u8),
            );
        }
        // Rising motes: each lane loops from the bottom to the top at its own speed.
        let lanes = ((right - left) / (ART * 4.0)) as i32;
        let height = bottom - top;
        for lane in 0..lanes.max(1) {
            let h = hash(updraft.rect.min.x, lane, 613);
            let speed = 90.0 + (h % 80) as f32 + updraft.strength as f32 * 0.05;
            for mote in 0..6 {
                let phase =
                    ((seconds * speed + (h >> 8) as f32 % height + mote as f32 * height / 6.0)
                        % height)
                        .abs();
                let y = bottom - phase;
                if y < visible.y || y > visible.y + visible.height {
                    continue;
                }
                let x =
                    left + (lane as f32 + 0.5) * ART * 4.0 + ((h >> (mote * 4)) % 3) as f32 * ART;
                let alpha = if y < top + fade { 70 } else { 150 };
                draw.draw_rectangle_rec(
                    Rectangle::new(snap(x), snap(y), ART, ART * 3.0),
                    with_alpha(ICE, alpha),
                );
                draw.draw_rectangle_rec(
                    Rectangle::new(snap(x), snap(y + ART * 3.0), ART, ART * 2.0),
                    with_alpha(MINT, alpha / 2),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The Moon from below.

/// Rows below the Moon from which its looming underside starts to show.
const MOON_LOOM_ROWS: i32 = MOON_BOTTOM_ROWS - CONTINENTS_BOTTOM_ROWS + 400;

/// How much of the clock's phased backdrop Moon (`sky::draw_moon`) still shows in `visible`:
/// 1 on older worlds and from the ground up to where the looming underside starts to show, then
/// fading to 0 over the same stretch the looming Moon fades in, so the two never show at once
/// at full strength.
pub fn backdrop_moon_share(settings: &WorldSettings, visible: Rectangle) -> f32 {
    if !settings.generator.has_sky() {
        return 1.0;
    }
    let distance = visible.y - y_of(MOON_BOTTOM_ROWS);
    #[allow(clippy::cast_precision_loss, reason = "band rows are small integers")]
    let reach = MOON_LOOM_ROWS as f32 * TILE_SIZE;
    (1.0 - (reach - distance) / (reach * 0.25)).clamp(0.0, 1.0)
}

/// The Moon's underside hanging above its column, drawn in perspective: far below it, it is a
/// small grey lens near the top of the view; it grows and drops toward its true place as the
/// view climbs, then gives way to its real tiles.
fn draw_moon_from_below(draw: &mut impl RaylibDraw, settings: &WorldSettings, visible: Rectangle) {
    let Some(moon) = sky::moon(settings) else {
        return;
    };
    let underside = y_of(MOON_BOTTOM_ROWS);
    let view_top = visible.y;
    let distance = view_top - underside;
    #[allow(clippy::cast_precision_loss, reason = "band rows are small integers")]
    let reach = MOON_LOOM_ROWS as f32 * TILE_SIZE;
    if distance <= visible.height * 0.15 || distance > reach {
        return;
    }
    // Perspective: an offset that is the true one up close and settles just inside the view's
    // top when far away; the same scale shrinks the Moon and pulls it toward the view's centre.
    let settle = visible.height * 0.4;
    let offset = distance / (1.0 + distance / settle);
    let scale = offset / distance;
    let center_x = visible.x + visible.width * 0.5;
    let moon_x = (moon.center_x as f32 + 0.5) * TILE_SIZE;
    let apparent_x = center_x + (moon_x - center_x) * scale;
    let apparent_bottom = view_top + visible.height * 0.22 * (1.0 - scale);
    let half_width = moon.half_width as f32 * TILE_SIZE * scale;
    let depth = moon.depth as f32 * TILE_SIZE * scale;
    if apparent_x + half_width < visible.x || apparent_x - half_width > visible.x + visible.width {
        return;
    }
    // Fade in from the far edge of its reach, out as the real tiles approach.
    let far = ((reach - distance) / (reach * 0.25)).clamp(0.0, 1.0);
    let near = ((distance - visible.height * 0.15) / (visible.height * 2.0)).clamp(0.0, 1.0);
    let alpha = (255.0 * far * near) as u8;
    if alpha == 0 {
        return;
    }
    let rows = (depth / ART).ceil().max(2.0) as i32;
    let top = apparent_bottom - depth;
    for row in 0..rows {
        let y = top + row as f32 * ART;
        if y + ART < visible.y || y > visible.y + visible.height {
            continue;
        }
        // The lens narrows toward its lowest point.
        let t = row as f32 / rows as f32;
        let half = half_width * (1.0 - t * t).max(0.0).sqrt();
        if half < ART {
            continue;
        }
        let color = if t > 0.9 { SLATE_DARK } else { SLATE };
        let left = snap(apparent_x - half).max(visible.x - ART);
        let right = snap(apparent_x + half).min(visible.x + visible.width + ART);
        draw.draw_rectangle_rec(
            Rectangle::new(left, snap(y), right - left, ART),
            with_alpha(color, alpha),
        );
        // Blotches of lighter and darker rock, anchored to the lens (craters seen from below).
        let span = right - left;
        for blotch in 0..4 {
            let h = hash(row / 3, blotch, 617);
            if !h.is_multiple_of(3) {
                continue;
            }
            let bx = left + (h >> 8) as f32 % span.max(1.0);
            let width = ART * (3 + (h >> 20) % 10) as f32 * (1.0 + scale * 6.0);
            let shade = if h & 0x40 == 0 {
                SLATE_LIGHT
            } else {
                SLATE_DARK
            };
            draw.draw_rectangle_rec(
                Rectangle::new(snap(bx), snap(y), snap(width.min(right - bx)), ART),
                with_alpha(shade, alpha),
            );
        }
        // A lit lower-right edge: the sun catching the underside's curve.
        draw.draw_rectangle_rec(
            Rectangle::new(right - ART * 2.0, snap(y), ART * 2.0, ART),
            with_alpha(FOG, alpha),
        );
    }
}
