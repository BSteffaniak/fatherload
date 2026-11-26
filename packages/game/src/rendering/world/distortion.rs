//! The distortion strata in the world view (Wave 9, Descent VI part A).
//!
//! * **Mirror halls** ([`draw_mirror_halls`]): a hall's boundary is a shimmering silver curtain
//!   (thin bright strips at its side edges, sparkles drifting up them) and its axis a faint line
//!   of light, so a rig sees a hall coming before the screen flips. Reflections of mirror ore
//!   breathe: a cold sheen swells and fades on them every couple of seconds (real ore never
//!   does), the tell a player learns.
//! * **Shear halls and the Spire** ([`draw_shear_flow`]): flow lines streaming the way gravity
//!   pulls inside every turned field in view, so a rig reads which way is down before it falls.
//! * **Creature tells** ([`draw_creature_tell`]): a twin's cold shimmer, a gazer's open eye
//!   glow, a lodestar's contracting rings of pull, a glitchling's ghost where it will land, a
//!   bitworm's square segments trailing behind it, a dead pixel's fringe.
//!
//! Every animation is slow (under one cycle a second) and low contrast, and scales with the
//! effects setting where it is decoration.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "presentation maps world pixels and tiles between small integers and floats"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    descent::fields::cached_layout,
    distortion::{WarpedTile, mirror, shear},
    enemy::{EnemyKind, EnemyMode, EnemyState},
    entity::Entity,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::TileRect,
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{ART, fx::disc};

/// The tiles (inclusive) covering `visible`, a tile of margin.
fn tile_span(visible: Rectangle) -> TileRect {
    let frame = crate::view_frame::frame();
    TileRect {
        min: TilePosition {
            x: frame.col(visible.x) - 1,
            y: frame.row(visible.y) - 1,
        },
        max: TilePosition {
            x: frame.col(visible.x + visible.width) + 1,
            y: frame.row(visible.y + visible.height) + 1,
        },
    }
}

/// Draws the distortion strata's world overlays in view.
pub(super) fn draw_distortion_world(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    visible: Rectangle,
    seconds: f32,
    strength: f32,
) {
    let settings = terrain.settings();
    if !fatherload_simulation::distortion::has_distortion(settings) {
        return;
    }
    let layout = cached_layout(settings);
    let span = tile_span(visible);
    let (first, last) = fatherload_simulation::distortion::distortion_rows(&layout);
    if span.max.y < first - 8 || span.min.y > last + 8 {
        return;
    }
    draw_mirror_halls(draw, terrain, &layout, &span, seconds, strength);
    draw_shear_flow(draw, terrain, &layout, &span, seconds, strength);
}

/// The mirror halls' curtains and axes, and the reflections' breathing.
fn draw_mirror_halls(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    layout: &fatherload_simulation::worldgen::descent::DescentLayout,
    span: &TileRect,
    seconds: f32,
    strength: f32,
) {
    let frame = crate::view_frame::frame();
    for zone in mirror::zones_touching(terrain.settings(), layout, span) {
        let bounds = zone.bounds;
        let top = frame.row_px(bounds.min.y.max(span.min.y));
        let bottom = frame.row_px(bounds.max.y.min(span.max.y) + 1);
        let height = (bottom - top).max(0.0);
        // The curtains: a strip at each side edge, a little inside the frame's glass dots.
        for edge_column in [bounds.min.x, bounds.max.x + 1] {
            let x = frame.col_px(edge_column);
            let alpha = 60.0f32.mul_add(strength, 40.0) as u8;
            draw.draw_rectangle_rec(
                Rectangle::new(x - ART * 3.0, top, ART * 6.0, height),
                with_alpha(SKY_LIGHT, alpha / 3),
            );
            draw.draw_rectangle_rec(
                Rectangle::new(x - ART, top, ART * 2.0, height),
                with_alpha(ICE, alpha),
            );
            // Sparkles drifting up the curtain.
            let sparkles = (height / (TILE_SIZE * 2.0)) as i32 + 1;
            for index in 0..sparkles {
                let phase =
                    (seconds * 0.35 + index as f32 * 0.37 + edge_column as f32 * 0.11).fract();
                let y = bottom - phase * height;
                let fade = (1.0 - (phase - 0.5).abs() * 2.0).max(0.0);
                disc(draw, x, y, 2, with_alpha(BONE, (170.0 * fade) as u8));
            }
        }
        // The axis: a faint line of light down the middle.
        let axis = f32::midpoint(frame.col_px(bounds.min.x), frame.col_px(bounds.max.x + 1));
        let pulse = (seconds * 0.6).sin().mul_add(0.5, 0.5);
        draw.draw_rectangle_rec(
            Rectangle::new(axis - ART, top, ART * 2.0, height),
            with_alpha(SKY_LIGHT, (14.0 + 22.0 * pulse * strength) as u8),
        );
        // Reflections breathe; real ore never does.
        for y in bounds.min.y.max(span.min.y)..=bounds.max.y.min(span.max.y) {
            for x in bounds.min.x.max(span.min.x)..=bounds.max.x.min(span.max.x) {
                let at = TilePosition { x, y };
                if terrain.tile(at).map(|tile| tile.kind)
                    != Some(TileKind::Warped(WarpedTile::MirrorGhost))
                {
                    continue;
                }
                let breath = (seconds * 1.4 + (x + y) as f32 * 0.3)
                    .sin()
                    .mul_add(0.5, 0.5);
                draw.draw_rectangle_rec(
                    Rectangle::new(frame.col_px(x), frame.row_px(y), TILE_SIZE, TILE_SIZE),
                    with_alpha(SKY, (breath * 70.0) as u8),
                );
            }
        }
    }
}

/// Flow lines in every turned field in view: short streaks sliding the way gravity pulls.
fn draw_shear_flow(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    layout: &fatherload_simulation::worldgen::descent::DescentLayout,
    span: &TileRect,
    seconds: f32,
    strength: f32,
) {
    let settings = terrain.settings();
    let frame = crate::view_frame::frame();
    let mut fields: Vec<(TileRect, Option<(i32, i32)>)> =
        shear::regions_touching(settings, layout, span)
            .into_iter()
            .map(|region| (region.bounds, Some(region.down())))
            .collect();
    if let Some(spire) = shear::spire(settings, layout).filter(|spire| spire.intersects(span)) {
        fields.push((spire, None));
    }
    let alpha = 90.0f32.mul_add(strength, 90.0) as u8;
    for (bounds, down) in fields {
        // One streak per 3x3 tiles, each sliding along its own track.
        for ty in (bounds.min.y.max(span.min.y)..=bounds.max.y.min(span.max.y)).step_by(3) {
            for tx in (bounds.min.x.max(span.min.x)..=bounds.max.x.min(span.max.x)).step_by(3) {
                let at = TilePosition { x: tx, y: ty };
                if terrain.tile(at).is_some_and(|tile| !tile.kind.is_open()) {
                    continue;
                }
                let (dx, dy) = down.unwrap_or_else(|| {
                    let (x, y) = shear::spire_down(&bounds, tx as f32 + 0.5, ty as f32 + 0.5);
                    (x as i32, y as i32)
                });
                let phase = (seconds * 0.8 + ((tx * 7 + ty * 13) % 17) as f32 / 17.0).fract();
                let travel = phase * TILE_SIZE * 3.0;
                let (cx, cy) = frame.centre_of(tx, ty);
                let (x, y) = (
                    (dx as f32).mul_add(travel - TILE_SIZE * 1.5, cx),
                    (dy as f32).mul_add(travel - TILE_SIZE * 1.5, cy),
                );
                let fade = (1.0 - (phase - 0.5).abs() * 2.0).max(0.0);
                let color = with_alpha(PINK, (f32::from(alpha) * fade) as u8);
                // A streak a tile long along the flow (two art pixels wide), with a brighter
                // head.
                for step in 0..8 {
                    let back = step as f32 * ART * 2.0;
                    let (wide_x, wide_y) = (ART * 2.0, ART * 2.0);
                    draw.draw_rectangle_rec(
                        Rectangle::new(
                            (dx as f32).mul_add(-back, x),
                            (dy as f32).mul_add(-back, y),
                            wide_x,
                            wide_y,
                        ),
                        if step == 0 {
                            with_alpha(CREAM, (f32::from(alpha) * fade) as u8)
                        } else {
                            color
                        },
                    );
                }
            }
        }
    }
}

/// The extra tells of a distortion creature, drawn behind its sprite.
pub(super) fn draw_creature_tell(
    draw: &mut impl RaylibDraw,
    entity: &Entity,
    state: &EnemyState,
    (x, y): (f32, f32),
    seconds: f32,
) {
    match state.kind {
        EnemyKind::MirrorTwin => {
            // A cold shimmer round the reflection.
            let pulse = ((seconds * 1.2).sin() * 2.0) as i32;
            disc(draw, x, y, 12 + pulse, with_alpha(SKY_LIGHT, 26));
            disc(draw, x, y, 8, with_alpha(ICE, 22));
        }
        EnemyKind::Gazer => {
            if state.mode.hostile() {
                let open = state.volley != 0;
                disc(
                    draw,
                    x,
                    y - ART * 9.0,
                    if open { 8 } else { 5 },
                    with_alpha(if open { PINK } else { LAVENDER }, 46),
                );
            }
        }
        EnemyKind::Lodestar => {
            // Rings of pull contracting toward it.
            disc(draw, x, y, 9, with_alpha(PINK, 30));
            for ring in 0..3 {
                let phase = (seconds * 0.6 + ring as f32 / 3.0).fract();
                let radius = (1.0 - phase) * 7.0 * TILE_SIZE / ART;
                draw_ring(
                    draw,
                    x,
                    y,
                    radius as i32,
                    with_alpha(LAVENDER, (60.0 * phase) as u8),
                );
            }
        }
        EnemyKind::Glitchling => {
            if state.mode == EnemyMode::Windup
                && let Some((tx, ty)) = state.waypoint
            {
                // The ghost where it will land, flickering slowly.
                let (gx, gy) = crate::view_frame::frame().centre_of(tx, ty);
                let on = (seconds * 4.0) as i32 % 2 == 0;
                let alpha = if on { 120 } else { 60 };
                draw.draw_rectangle_rec(
                    Rectangle::new(gx - ART * 6.0, gy - ART * 6.0, ART * 12.0, ART * 12.0),
                    with_alpha(LIME, alpha / 3),
                );
                draw_ring(draw, gx, gy, 7, with_alpha(PINK, alpha));
            }
            disc(draw, x, y, 6, with_alpha(LIME, 26));
        }
        EnemyKind::Bitworm => {
            // Square segments trailing behind the head, against its heading.
            let (vx, vy) = (entity.body.velocity_x, entity.body.velocity_y);
            let speed = vx.hypot(vy).max(1.0);
            let (bx, by) = (-vx / speed, -vy / speed);
            for segment in 1..=4 {
                let back = segment as f32 * ART * 6.0;
                let size = ART * (10 - segment) as f32;
                let (sx, sy) = (bx.mul_add(back, x), by.mul_add(back, y));
                draw.draw_rectangle_rec(
                    Rectangle::new(sx - size * 0.5, sy - size * 0.5, size, size),
                    INK,
                );
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        sx - size * 0.5 + ART,
                        sy - size * 0.5 + ART,
                        size - ART * 2.0,
                        size - ART * 2.0,
                    ),
                    if segment % 2 == 0 { TEAL } else { TEAL_DARK },
                );
            }
        }
        EnemyKind::DeadPixel => {
            let pulse = ((seconds * 1.5).sin() * 2.0) as i32;
            disc(draw, x, y, 11 + pulse, with_alpha(PLUM, 50));
        }
        _ => {}
    }
}

/// A one-art-pixel ring of `radius` art pixels.
fn draw_ring(draw: &mut impl RaylibDraw, x: f32, y: f32, radius: i32, color: Color) {
    if radius <= 0 {
        return;
    }
    let steps = (radius * 6).max(8);
    for step in 0..steps {
        let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
        let (px, py) = (
            (angle.cos() * radius as f32).round(),
            (angle.sin() * radius as f32).round(),
        );
        draw.draw_rectangle_rec(
            Rectangle::new(px.mul_add(ART, x), py.mul_add(ART, y), ART, ART),
            color,
        );
    }
}
