//! Deep supplies lying in the world (Wave 10 polish): a burning Flare Buoy (a red buoy whose
//! warm light pushes the Pearl Abyss's dark back) and Beast Bait (a haunch with its scent
//! curling up). Drawn over the dark, so a flare reads as light. Presentation only: everything
//! derives from the replicated `Logistics::supply_effects`; the glow breathes slowly (never a
//! strobe) and fades out as the supply is spent.

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use fatherload_simulation::{
    logistics::Logistics,
    terrain::TILE_SIZE,
    workshop::{DeepSupply, FLARE_REACH_TILES, SupplyLure},
};

use super::{ART, fx::disc};

/// What the supplies' drawing reads.
pub(super) struct SupplyScene<'a> {
    pub logistics: &'a Logistics,
    pub visible: Rectangle,
    pub world_tick: u64,
    pub seconds: f32,
    /// The effects setting's strength (0 = off): scales the glow's breathing.
    pub effects: f32,
}

/// How much of `lure`'s life is left at `tick` (`0..=1`), for its fade.
#[allow(clippy::cast_precision_loss, reason = "tick spans of a minute")]
fn life_left(lure: &SupplyLure, tick: u64) -> f32 {
    let span = lure.until.get().saturating_sub(lure.since.get()).max(1);
    let left = lure.until.get().saturating_sub(tick);
    (left as f32 / span as f32).clamp(0.0, 1.0)
}

/// Every flare and bait in view.
pub(super) fn draw_supply_lures(draw: &mut impl RaylibDraw, scene: &SupplyScene<'_>) {
    let effects = &scene.logistics.supply_effects;
    if effects.lures.is_empty() {
        return;
    }
    let frame = crate::view_frame::frame();
    let margin = FLARE_REACH_TILES * TILE_SIZE;
    for lure in &effects.lures {
        let (x, y) = frame.tile_center(lure.at);
        if x < scene.visible.x - margin
            || x > scene.visible.x + scene.visible.width + margin
            || y < scene.visible.y - margin
            || y > scene.visible.y + scene.visible.height + margin
        {
            continue;
        }
        // The last fifth of its life it gutters out.
        let fade = (life_left(lure, scene.world_tick) * 5.0).min(1.0);
        match lure.supply {
            DeepSupply::BeastBait => draw_bait(draw, x, y, fade, scene.seconds),
            _ => draw_flare(draw, x, y, fade, scene),
        }
    }
}

/// A flare buoy: the warm pool of its light, then the buoy bobbing with its lamp.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "alphas"
)]
fn draw_flare(draw: &mut impl RaylibDraw, x: f32, y: f32, fade: f32, scene: &SupplyScene<'_>) {
    let breathe = (scene.seconds * 1.4)
        .sin()
        .mul_add(0.12 * scene.effects, 1.0);
    let reach = FLARE_REACH_TILES * TILE_SIZE * breathe;
    let warm = Color::new(255, 176, 96, (90.0 * fade) as u8);
    draw.draw_circle_gradient(x as i32, y as i32, reach, warm, Color::new(255, 140, 70, 0));
    draw.draw_circle_gradient(
        x as i32,
        y as i32,
        reach * 0.35,
        Color::new(255, 220, 160, (110.0 * fade) as u8),
        Color::new(255, 190, 120, 0),
    );
    // The buoy bobs a pixel or two on the swell.
    let bob = (scene.seconds * 1.8).sin() * ART;
    let top = y - ART * 7.0 + bob;
    // Hull: an outlined red float with a white band and a dark keel, a mast to the lamp.
    let body = |dx: f32, dy: f32, w: f32, h: f32, color: Color| {
        (
            Rectangle::new(x + dx * ART, top + dy * ART, w * ART, h * ART),
            color,
        )
    };
    for (rect, color) in [
        body(-4.0, 4.0, 8.0, 8.0, INK),
        body(-3.0, 5.0, 6.0, 6.0, RED),
        body(-3.0, 5.0, 2.0, 5.0, ROSE),
        body(-3.0, 7.0, 6.0, 2.0, CREAM),
        body(-2.0, 11.0, 4.0, 2.0, INK),
        body(-1.0, 1.0, 2.0, 4.0, SLATE),
        body(-2.0, 0.0, 4.0, 1.0, INK),
    ] {
        draw.draw_rectangle_rec(rect, color);
    }
    // The lamp on its mast.
    disc(draw, x, top, 3, with_alpha(GOLD, (220.0 * fade) as u8));
    disc(draw, x, top, 1, with_alpha(CREAM, (255.0 * fade) as u8));
}

/// Beast bait: a haunch on the bone, its scent curling up in slow green wisps.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "alphas"
)]
fn draw_bait(draw: &mut impl RaylibDraw, x: f32, y: f32, fade: f32, seconds: f32) {
    let base = y + TILE_SIZE * 0.5 - ART * 6.0;
    let mut px = |dx: f32, dy: f32, w: f32, h: f32, color: Color| {
        draw_rect(draw, x + dx * ART, base + dy * ART, w * ART, h * ART, color);
    };
    // Outline, the haunch (marbled), the bone and its knuckles, a shadow under it.
    px(-7.0, -1.0, 11.0, 7.0, INK);
    px(-6.0, 0.0, 9.0, 5.0, RED);
    px(-5.0, 0.0, 6.0, 1.0, PINK);
    px(-4.0, 2.0, 2.0, 1.0, MAROON);
    px(-1.0, 3.0, 2.0, 1.0, MAROON);
    px(3.0, 1.0, 4.0, 3.0, INK);
    px(3.0, 2.0, 4.0, 1.0, CREAM);
    px(6.0, 0.0, 2.0, 2.0, CREAM);
    px(6.0, 3.0, 2.0, 2.0, CREAM);
    px(-7.0, 6.0, 15.0, 1.0, with_alpha(INK, 120));
    for wisp in 0..3_u8 {
        let phase = seconds.mul_add(0.6, f32::from(wisp) / 3.0).fract();
        let rise = phase * TILE_SIZE * 1.5;
        let sway = (phase * 6.0 + f32::from(wisp)).sin() * ART * 2.0;
        let alpha = ((1.0 - phase) * 120.0 * fade) as u8;
        disc(
            draw,
            x - ART + sway + f32::from(wisp) * ART * 2.0,
            base - ART * 3.0 - rise,
            2,
            with_alpha(LIME, alpha),
        );
    }
}

fn draw_rect(draw: &mut impl RaylibDraw, x: f32, y: f32, w: f32, h: f32, color: Color) {
    draw.draw_rectangle_rec(Rectangle::new(x, y, w, h), color);
}
