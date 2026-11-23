//! The Hollow World in the world view (Wave 9, Descent V): its skies and its sun.
//!
//! * **The hollow backdrop** ([`draw_hollow_backdrop`]): behind the terrain of the three great
//!   open caverns (whose open tiles paint transparent, `tile_art::in_open_hollow`). The Inner
//!   Sky is a real sky under the rock: azure overhead, warm haze toward the far ground, the
//!   **inner sun** hanging low over the horizon (it swells by day and sinks to an ember at
//!   night, `hollow::inner_sun_at`), far floating isles drifting in parallax layers and, at
//!   night, the far side's lights. The isle sea is cooler, crowded with far hanging isles; the
//!   forge sky blazes amber, brightest toward the sun below.
//! * **The Sun Well** ([`draw_sun_well`]): the inner sun's swelling glow while a flare is
//!   coming (it rises smoothly over the warning, never blinks), its rays while it burns with
//!   the shade pillars' shadows cast long and dark (that is where to be), and the Sun
//!   Serpent's body drawn as one gold band through its coils.
//!
//! The inner day's light on the scene itself is the seat's post-process light
//! (`render_snapshot`: the view's fields take the inner sun's light), so every layer darkens
//! together at night.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::suboptimal_flops,
    clippy::while_float,
    clippy::single_match_else,
    clippy::too_many_lines,
    reason = "presentation maps world pixels and tiles between small integers and floats"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    descent::Stratum,
    enemy::EnemyKind,
    entity::{Entity, EntityState},
    hollow::{Hollow, InnerSun, SerpentPhase, inner_sun_at},
    terrain::{TILE_SIZE, Terrain, TilePosition},
    worldgen::descent::{self as layout_rules, hollow as places},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::fx::disc;

/// World pixels per sky band.
const BAND: f32 = 8.0;

/// What the backdrop needs.
pub struct HollowScene<'a> {
    pub terrain: &'a Terrain,
    pub hollow: &'a Hollow,
    pub enemies: &'a [Entity],
    pub visible: Rectangle,
    pub world_millis: u64,
    pub seconds: f32,
    pub effects_strength: f32,
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color::new(
        lerp(a.r, b.r),
        lerp(a.g, b.g),
        lerp(a.b, b.b),
        lerp(a.a, b.a),
    )
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::new(r, g, b, 255)
}

/// Which open cavern a view centred on `tile` looks into, and that cavern's rows at its column.
fn cavern_at(terrain: &Terrain, tile: TilePosition) -> Option<(Stratum, i32, i32)> {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return None;
    }
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let (first, last) = layout.descent_rows(fatherload_simulation::descent::Descent::HollowWorld);
    if tile.y < first - 60 || tile.y > last + 60 {
        return None;
    }
    // The nearest cavern by rows (a view straddling two strata takes the one at its middle).
    let candidates = [
        (
            Stratum::InnerSky,
            layout_rules::hollow_rows(settings, &layout, tile.x),
        ),
        (
            Stratum::UpsideDownIsles,
            places::isle_sea_rows(settings, &layout, tile.x),
        ),
        (
            Stratum::TheSunforge,
            places::forge_rows(settings, &layout, tile.x),
        ),
    ];
    candidates
        .into_iter()
        .map(|(stratum, (top, bottom))| {
            let distance = if tile.y < top {
                top - tile.y
            } else if tile.y > bottom {
                tile.y - bottom
            } else {
                0
            };
            (distance, stratum, top, bottom)
        })
        .filter(|(distance, ..)| *distance < 60)
        .min_by_key(|(distance, ..)| *distance)
        .map(|(_, stratum, top, bottom)| (stratum, top, bottom))
}

/// The sky's colour at `share` (0 at the cavern's roof, 1 at its floor) under `sun`.
fn sky_colour(stratum: Stratum, share: f32, sun: InnerSun) -> Color {
    let day = (f32::from(sun.light) - 260.0) / 740.0;
    let (day_top, day_mid, day_low, night_top, night_low) = match stratum {
        Stratum::TheSunforge => (
            rgb(214, 112, 40),
            rgb(246, 176, 82),
            rgb(255, 238, 190),
            rgb(92, 34, 22),
            rgb(176, 84, 40),
        ),
        Stratum::UpsideDownIsles => (
            rgb(64, 96, 168),
            rgb(118, 164, 222),
            rgb(196, 214, 236),
            rgb(14, 18, 44),
            rgb(46, 40, 78),
        ),
        _ => (
            rgb(58, 104, 178),
            rgb(108, 168, 228),
            rgb(255, 214, 154),
            rgb(12, 16, 42),
            rgb(78, 44, 64),
        ),
    };
    let lit = if share < 0.55 {
        mix(day_top, day_mid, share / 0.55)
    } else {
        mix(day_mid, day_low, (share - 0.55) / 0.45)
    };
    let dark = mix(night_top, night_low, share);
    mix(dark, lit, day)
}

/// Draws the hollow backdrop behind the terrain (see the module docs).
pub(super) fn draw_hollow_backdrop(draw: &mut impl RaylibDraw, scene: &HollowScene<'_>) {
    let frame = crate::view_frame::frame();
    let visible = scene.visible;
    let centre = frame.tile_at(
        visible.width.mul_add(0.5, visible.x),
        visible.height.mul_add(0.5, visible.y),
    );
    let Some((stratum, top, bottom)) = cavern_at(scene.terrain, centre) else {
        return;
    };
    let sun = inner_sun_at(scene.world_millis);
    let (_, top_px) = frame.tile_center(TilePosition {
        x: centre.x,
        y: top,
    });
    let (_, bottom_px) = frame.tile_center(TilePosition {
        x: centre.x,
        y: bottom,
    });
    let height = (bottom_px - top_px).max(TILE_SIZE);
    // Sky bands, dithered at each band's edge so the gradient stays pixel art.
    let mut y = (visible.y / BAND).floor() * BAND;
    while y < visible.y + visible.height {
        let share = (y - top_px) / height;
        let colour = sky_colour(stratum, share, sun);
        draw.draw_rectangle_rec(Rectangle::new(visible.x, y, visible.width, BAND), colour);
        y += BAND;
    }
    // Night: the far side's lights, steady (a slow shimmer, never a blink).
    let night = 1.0 - (f32::from(sun.light) - 260.0) / 740.0;
    if night > 0.2 && stratum != Stratum::TheSunforge {
        let alpha = (night * 200.0) as u8;
        let cell = 48.0;
        let first_x = (visible.x / cell).floor() as i32;
        let first_y = (visible.y / cell).floor() as i32;
        for cy in first_y..=first_y + (visible.height / cell) as i32 + 1 {
            for cx in first_x..=first_x + (visible.width / cell) as i32 + 1 {
                let h = crate::rendering::terrain::tile_art::hash(cx, cy, 971);
                if !h.is_multiple_of(7) {
                    continue;
                }
                let (px, py) = (
                    (cx as f32).mul_add(cell, (h % 40) as f32),
                    (cy as f32).mul_add(cell, ((h >> 8) % 40) as f32),
                );
                if (py - top_px) / height > 0.7 {
                    continue;
                }
                let shimmer = 0.75 + 0.25 * (scene.seconds * 0.6 + (h % 13) as f32).sin();
                let tint = if h.is_multiple_of(3) { AMBER } else { CREAM };
                draw.draw_rectangle_rec(
                    Rectangle::new(px, py, 2.0, 2.0),
                    with_alpha(tint, (f32::from(alpha) * shimmer) as u8),
                );
            }
        }
    }
    draw_inner_sun(draw, scene, stratum, (top_px, bottom_px), sun);
    draw_far_isles(draw, scene, stratum, (top_px, bottom_px), sun);
}

/// The inner sun: low over the far ground of the Inner Sky (it drifts slowly with travel),
/// a great glow under the forge sky.
fn draw_inner_sun(
    draw: &mut impl RaylibDraw,
    scene: &HollowScene<'_>,
    stratum: Stratum,
    (top_px, bottom_px): (f32, f32),
    sun: InnerSun,
) {
    let visible = scene.visible;
    let light = f32::from(sun.light) / 1_000.0;
    let centre_x = visible.width.mul_add(0.5, visible.x);
    match stratum {
        Stratum::TheSunforge => {
            // The sun below: a wash of light rising from the forge's floor.
            let x = ((centre_x * 0.000_8).sin() * TILE_SIZE).mul_add(4.0, centre_x);
            let y = bottom_px + TILE_SIZE * 6.0;
            for (radius, alpha) in [(150, 26_u8), (110, 34), (70, 46), (40, 60)] {
                disc(
                    draw,
                    x,
                    y,
                    radius,
                    with_alpha(CREAM, (f32::from(alpha) * light) as u8),
                );
            }
        }
        _ => {
            // Far away: it drifts only slowly across the view as the rig travels.
            let x = (visible.width * 0.22).mul_add((centre_x * 0.000_6).sin(), centre_x);
            let horizon = bottom_px - (bottom_px - top_px) * 0.18;
            let rise = (1.0 - light) * TILE_SIZE * 3.0;
            let y = horizon + rise;
            let core = if sun.phase.sun_up() { CREAM } else { ORANGE };
            let body = if sun.phase.sun_up() { GOLD } else { RUST };
            let size = 30.0f32.mul_add(light, 26.0) as i32;
            disc(
                draw,
                x,
                y,
                size * 3,
                with_alpha(AMBER, (22.0 * light) as u8 + 6),
            );
            disc(
                draw,
                x,
                y,
                size * 2,
                with_alpha(GOLD, (40.0 * light) as u8 + 10),
            );
            disc(draw, x, y, size, body);
            disc(draw, x, y, size * 3 / 4, core);
            if sun.phase.sun_up() {
                disc(
                    draw,
                    x - size as f32 * 0.6,
                    y - size as f32 * 0.6,
                    size / 4,
                    BONE,
                );
            }
        }
    }
}

/// Far floating isles in two parallax layers (hanging ones in the isle sea).
fn draw_far_isles(
    draw: &mut impl RaylibDraw,
    scene: &HollowScene<'_>,
    stratum: Stratum,
    (top_px, bottom_px): (f32, f32),
    sun: InnerSun,
) {
    if stratum == Stratum::TheSunforge {
        return;
    }
    let visible = scene.visible;
    let light = (f32::from(sun.light) - 260.0) / 740.0;
    let height = bottom_px - top_px;
    for (layer, parallax, tint, spacing) in [
        (0_u32, 0.25_f32, rgb(142, 170, 210), 560.0_f32),
        (1, 0.5, rgb(98, 124, 168), 420.0),
    ] {
        let tint = mix(rgb(30, 30, 60), tint, light.clamp(0.2, 1.0));
        let offset = visible.x * parallax;
        let first = ((visible.x - offset) / spacing).floor() as i32 - 1;
        let count = (visible.width / spacing) as i32 + 3;
        for index in first..first + count {
            let h = crate::rendering::terrain::tile_art::hash(index, layer as i32, 977 + layer);
            let width = 90.0 + (h % 120) as f32;
            let depth = 0.25 + ((h >> 8) % 50) as f32 / 100.0;
            let x = (index as f32).mul_add(spacing, offset) + ((h >> 16) % 160) as f32;
            let y = top_px + height * depth;
            let hanging = stratum == Stratum::UpsideDownIsles;
            // A flat face and a tapering point (the point up for hanging isles).
            let face_h = 10.0 + (h % 8) as f32;
            let point = 30.0 + ((h >> 4) % 40) as f32;
            let (face_y, tip_y) = if hanging {
                (y, y - point)
            } else {
                (y, y + point)
            };
            draw.draw_rectangle_rec(
                Rectangle::new(x, face_y - face_h * 0.5, width, face_h),
                tint,
            );
            let steps = 6;
            for step in 0..steps {
                let t0 = step as f32 / steps as f32;
                let t1 = (step + 1) as f32 / steps as f32;
                let w = width * (1.0 - t0 * 0.9);
                let (y0, y1) = (
                    (tip_y - face_y).mul_add(t0, face_y),
                    (tip_y - face_y).mul_add(t1, face_y),
                );
                draw.draw_rectangle_rec(
                    Rectangle::new(x + (width - w) * 0.5, y0.min(y1), w, (y1 - y0).abs() + 1.0),
                    tint,
                );
            }
            if hanging {
                // A thread of stalk up to the roof.
                draw.draw_rectangle_rec(
                    Rectangle::new(x + width * 0.5 - 2.0, top_px, 4.0, tip_y - top_px),
                    with_alpha(tint, 160),
                );
            }
        }
    }
}

/// The Sun Well's flare and the serpent's body (see the module docs).
pub(super) fn draw_sun_well(draw: &mut impl RaylibDraw, scene: &HollowScene<'_>) {
    let settings = scene.terrain.settings();
    if !settings.generator.has_descents() {
        return;
    }
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let Some(centre) = places::sun_well(settings, &layout) else {
        return;
    };
    let frame = crate::view_frame::frame();
    let (cx, cy) = frame.tile_center(centre);
    let reach = places::sun_well_rect(centre).width() as f32 * TILE_SIZE;
    if cx < scene.visible.x - reach
        || cx > scene.visible.x + scene.visible.width + reach
        || cy < scene.visible.y - reach
        || cy > scene.visible.y + scene.visible.height + reach
    {
        return;
    }
    let serpent = &scene.hollow.serpent;
    // The sun's own glow, always.
    let pulse = (scene.seconds * 0.8).sin() * 2.0;
    disc(draw, cx, cy, 70 + pulse as i32, with_alpha(GOLD, 24));
    disc(draw, cx, cy, 48, with_alpha(CREAM, 30));
    // A flare coming: the glow swells smoothly over the warning.
    if let (Some(_), None) = (serpent.flare_warned, serpent.flare_burns) {
        let swell = ((scene.seconds * 1.2).sin() * 0.5 + 0.5) * scene.effects_strength.max(0.3);
        disc(
            draw,
            cx,
            cy,
            110,
            with_alpha(ORANGE, (30.0 + 40.0 * swell) as u8),
        );
        disc(
            draw,
            cx,
            cy,
            80,
            with_alpha(GOLD, (40.0 + 50.0 * swell) as u8),
        );
    }
    if serpent.flaring() {
        // The burn: light everywhere the sun sees, the pillars' shadows long and dark.
        disc(draw, cx, cy, 150, with_alpha(CREAM, 50));
        for pillar in places::shade_pillars(centre) {
            let (px, py) = frame.tile_center(pillar);
            let (dx, dy) = (px - cx, py - cy);
            let length = dx.hypot(dy).max(1.0);
            let (nx, ny) = (dx / length, dy / length);
            let (sx, sy) = (-ny * TILE_SIZE * 1.6, nx * TILE_SIZE * 1.6);
            let far = TILE_SIZE * 14.0;
            let a = Vector2::new(px + sx, py + sy);
            let b = Vector2::new(px - sx, py - sy);
            let c = Vector2::new(
                far.mul_add(nx, px) - sx * 2.0,
                far.mul_add(ny, py) - sy * 2.0,
            );
            let d = Vector2::new(
                far.mul_add(nx, px) + sx * 2.0,
                far.mul_add(ny, py) + sy * 2.0,
            );
            let shadow = with_alpha(NIGHT, 150);
            draw.draw_triangle(a, c, b, shadow);
            draw.draw_triangle(a, d, c, shadow);
            draw.draw_triangle(b, c, a, shadow);
            draw.draw_triangle(a, c, d, shadow);
        }
    }
    // The serpent's body: a gold band through its head and coils, behind their sprites.
    let mut parts: Vec<(f32, f32)> = Vec::new();
    if let Some(head) = serpent.head {
        for id in std::iter::once(head).chain(serpent.coils.iter().copied()) {
            if let Some(entity) = scene.enemies.iter().find(|entity| entity.id.get() == id)
                && matches!(entity.state, EntityState::Enemy(state)
                    if matches!(state.kind, EnemyKind::SunSerpent | EnemyKind::SerpentCoil))
            {
                parts.push((entity.body.x, entity.body.y));
            }
        }
    }
    let exposed = serpent.phase == SerpentPhase::Exposed;
    for pair in parts.windows(2) {
        let (a, b) = (
            Vector2::new(pair[0].0, pair[0].1),
            Vector2::new(pair[1].0, pair[1].1),
        );
        draw.draw_line_ex(a, b, 30.0, INK);
        draw.draw_line_ex(a, b, 24.0, if exposed { RUST } else { AMBER });
        draw.draw_line_ex(a, b, 10.0, if exposed { MAROON } else { GOLD });
    }
}
