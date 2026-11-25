#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    reason = "screen geometry"
)]

//! The Star Drive's speed, seen: cruise draws thin lines streaming past the craft, warp
//! draws long streaks, a blue tunnel at the view's edges and a glow ahead. The starfield's
//! own stars stretch into streaks too ([`super::starfield`]). Nothing flashes: warp brightens
//! only as its charge rises (the flight track ramps it over seconds), and the screen effects
//! setting thins all of it (the motion effects are the ones it cuts most).

use raylib::prelude::*;

use fatherload_simulation::field::ScreenEffect;

use super::{
    Projection, additive, fade, hash64, unit,
    view::{FlightMotion, SpaceRenderView},
};
use crate::rendering::pixel_art::palette;

/// How much of the motion effects the setting lets through (never quite none: the lines are
/// how the player reads speed).
const fn motion_share(view: &SpaceRenderView<'_>) -> f32 {
    view.effects.scale(ScreenEffect::Wobble).max(0.2)
}

/// Draws cruise lines or warp streaks over the view.
pub fn draw_motion(
    draw: &mut impl RaylibDraw,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
) {
    let share = motion_share(view);
    match view.motion {
        FlightMotion::Coasting => {}
        FlightMotion::Cruise { speed, heading } => {
            let intensity = (speed / 600.0).clamp(0.15, 1.0);
            lines(draw, view, projection, heading, intensity * share, false);
        }
        FlightMotion::Warp { charge, heading } => {
            let charge = charge.clamp(0.0, 1.0);
            tunnel(draw, projection, charge * share);
            lines(
                draw,
                view,
                projection,
                heading,
                (0.4 + 0.6 * charge) * share,
                true,
            );
        }
    }
}

/// Lines parallel to the heading, streaming backwards past the craft.
fn lines(
    draw: &mut impl RaylibDraw,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
    heading: f32,
    intensity: f32,
    warp: bool,
) {
    let angle = projection.screen_angle(heading);
    let (dx, dy) = (angle.cos(), angle.sin());
    let (nx, ny) = (-dy, dx);
    let reach = projection.reach();
    let count = if warp { 70 } else { 28 };
    let shown = (count as f32 * intensity).ceil() as usize;
    additive(|| {
        for index in 0..shown {
            let h = hash64(&[0x5eed, i64::try_from(index).unwrap_or(0)]);
            let rate = if warp {
                0.9 + unit(h, 8) * 0.8
            } else {
                0.35 + unit(h, 8) * 0.4
            };
            let along = (1.0 - (unit(h, 0) + view.seconds * rate).fract()) * 2.0 * reach - reach;
            let across = (unit(h, 16) * 2.0 - 1.0) * reach;
            if across.abs() < 34.0 && along.abs() < 60.0 {
                continue;
            }
            let length = if warp {
                (180.0 + unit(h, 32) * 420.0) * intensity
            } else {
                (30.0 + unit(h, 32) * 90.0) * intensity
            };
            let start = Vector2::new(
                projection.pivot.0 + dx * along + nx * across,
                projection.pivot.1 + dy * along + ny * across,
            );
            let end = Vector2::new(start.x - dx * length, start.y - dy * length);
            let colour = if warp && index % 3 == 0 {
                palette::BONE
            } else if warp {
                palette::SKY_LIGHT
            } else {
                palette::MIST
            };
            let alpha = if warp { 0.55 } else { 0.28 } * intensity.min(1.0);
            draw.draw_line_ex(
                start,
                end,
                if warp && index % 5 == 0 { 2.0 } else { 1.0 },
                fade(colour, alpha),
            );
        }
    });
}

/// The warp tunnel: blue at the view's edges.
fn tunnel(draw: &mut impl RaylibDraw, projection: &Projection, level: f32) {
    if level <= 0.0 {
        return;
    }
    let bounds = projection.bounds;
    let edge = (bounds.width.min(bounds.height) * 0.22).round() as i32;
    let tint = fade(palette::SKY, 0.32 * level);
    let clear = fade(palette::SKY, 0.0);
    let (x, y, w, h) = (
        bounds.x as i32,
        bounds.y as i32,
        bounds.width as i32,
        bounds.height as i32,
    );
    draw.draw_rectangle_gradient_v(x, y, w, edge, tint, clear);
    draw.draw_rectangle_gradient_v(x, y + h - edge, w, edge, clear, tint);
    draw.draw_rectangle_gradient_h(x, y, edge, h, tint, clear);
    draw.draw_rectangle_gradient_h(x + w - edge, y, edge, h, clear, tint);
}
