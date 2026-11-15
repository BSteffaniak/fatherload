//! Drawing pilots on foot in the world (Part II F5): the pilot sprite for its pose
//! (`rendering::pilot_art`), jetpack flames, bubbles under water, the hand pick's cracks on the
//! tile it chips, small health and air gauges when they are not full, and the parked rig's
//! hazard lights and headlamp while its pilot is out.

use raylib::prelude::*;

use fatherload_simulation::{
    pilot::{PilotPose, STRIKE_GUARD_SECONDS},
    terrain::TILE_SIZE,
};

use crate::render_snapshot::{PilotPresentation, PlayerPresentation};
use crate::rendering::pilot_art::{PilotFrame, PilotLook, paint_pilot};
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{ART, Pen, rig::RigFacings, snap};

/// Draws every pilot in view.
pub(super) fn draw_pilots(
    draw: &mut impl RaylibDraw,
    pilots: &[PilotPresentation],
    visible: Rectangle,
    ticks: u64,
) {
    for pilot in pilots {
        let margin = TILE_SIZE * 2.0;
        if pilot.x < visible.x - margin
            || pilot.x > visible.x + visible.width + margin
            || pilot.y < visible.y - margin
            || pilot.y > visible.y + visible.height + margin * 2.0
        {
            continue;
        }
        draw_pilot(draw, pilot, ticks);
    }
}

/// Draws one pilot: flames behind, the sprite, then bubbles and gauges.
pub(super) fn draw_pilot(draw: &mut impl RaylibDraw, pilot: &PilotPresentation, ticks: u64) {
    let look = PilotLook::new(pilot.cosmetics, pilot.state.gear);
    let swinging = pilot.pick.is_some() || pilot.state.pick_cooldown > 0.0;
    let frame = PilotFrame {
        pose: pilot.pose,
        seconds: pilot.state.anim,
        swinging,
    };
    let flip = pilot.state.facing < 0 && pilot.pose != PilotPose::Climb;
    let feet = Vector2::new(snap(pilot.x), snap(pilot.y));
    let pen = Pen {
        x: feet.x,
        y: feet.y,
    };
    let column = |x: i32| if flip { -x - 1 } else { x };
    let tint = (pilot.state.hurt > STRIKE_GUARD_SECONDS * 0.2 && (ticks / 3).is_multiple_of(2))
        .then_some(BONE);
    paint_pilot(&look, frame).draw(draw, feet, ART, flip, tint);
    if pilot.pose == PilotPose::Jetpack {
        // A flickering plume from the pack's nozzles, over the boots: white-hot at the nozzle,
        // through gold and orange to a red tip, with a soft glow around it.
        let flicker = (ticks / 2 % 3) as i32;
        let length = 8 + flicker;
        let glow = with_alpha(GOLD, 70);
        pen.rect(draw, column(-8).min(column(-4)), -6, 5, length - 2, glow);
        for dy in 0..length {
            let color = match dy {
                0 => CREAM,
                1 | 2 => GOLD,
                _ if dy == length - 1 => with_alpha(RED, 200),
                _ => ORANGE,
            };
            let width = if dy < 3 {
                3
            } else if dy < length - 2 {
                2
            } else {
                1
            };
            let left = -7 + (3 - width + 1) / 2;
            pen.rect(
                draw,
                column(left).min(column(left + width - 1)),
                -6 + dy,
                width,
                1,
                color,
            );
        }
    }
    if pilot.pose == PilotPose::Swim {
        for bubble in 0..3_u64 {
            let age = (ticks + bubble * 13) % 40;
            let x = column(2) + ((age / 7 + bubble) % 2) as i32;
            pen.px(
                draw,
                x,
                -22 - (age / 3) as i32,
                with_alpha(ICE, (220 - age * 5) as u8),
            );
        }
    }
    draw_gauges(draw, pilot, pen);
}

/// Small bars over the helmet: health (red) when hurt, air (cyan) when using it up.
fn draw_gauges(draw: &mut impl RaylibDraw, pilot: &PilotPresentation, pen: Pen) {
    let state = &pilot.state;
    let mut row = -30;
    let mut bar = |draw: &mut dyn FnMut(i32, i32, i32, Color), share: f32, fill: Color| {
        let width = 12;
        let filled = (share.clamp(0.0, 1.0) * width as f32).round() as i32;
        draw(-6, row, width + 2, INK);
        draw(-5, row, filled, fill);
        row -= 2;
    };
    let mut rects: Vec<(i32, i32, i32, Color)> = Vec::new();
    {
        let mut push = |x: i32, y: i32, w: i32, color: Color| rects.push((x, y, w, color));
        if state.health < state.max_health {
            bar(&mut push, state.health / state.max_health.max(1.0), RED);
        }
        if state.oxygen < state.max_oxygen {
            let low = state.oxygen < state.max_oxygen * 0.25;
            bar(
                &mut push,
                state.oxygen / state.max_oxygen.max(1.0),
                if low { ROSE } else { SKY_LIGHT },
            );
        }
        if state.jetpack_capacity > 0.0 && state.jetpack_fuel < state.jetpack_capacity {
            bar(&mut push, state.jetpack_fuel / state.jetpack_capacity, GOLD);
        }
    }
    for (x, y, w, color) in rects {
        pen.rect(draw, x, y, w, 1, color);
    }
}

/// Cracks on the tile the hand pick is chipping, growing with its progress.
pub(super) fn draw_pick_cracks(draw: &mut impl RaylibDraw, pilots: &[PilotPresentation]) {
    for pilot in pilots {
        let Some((tile, progress)) = pilot.pick else {
            continue;
        };
        let pen = Pen::at(tile.x as f32 * TILE_SIZE, tile.y as f32 * TILE_SIZE);
        let stages = (progress * 4.0).ceil() as i32;
        let cracks: [&[(i32, i32)]; 4] = [
            &[(7, 6), (8, 7), (8, 8)],
            &[(6, 5), (9, 9), (10, 10), (5, 4)],
            &[(4, 9), (11, 5), (12, 4), (3, 10)],
            &[(2, 3), (13, 12), (6, 12), (11, 2)],
        ];
        for stage in cracks.iter().take(stages.clamp(0, 4) as usize) {
            for &(x, y) in *stage {
                pen.px(draw, x, y, with_alpha(INK, 200));
            }
        }
    }
}

/// A parked rig (its pilot out on foot): hazard lights blinking on both sides and its lamp left
/// on, a soft cone ahead.
pub(super) fn draw_parked_rig(
    draw: &mut impl RaylibDraw,
    player: &PlayerPresentation,
    facings: &RigFacings,
    ticks: u64,
) {
    let left = facings.facing_left(player);
    let ahead: f32 = if left { -1.0 } else { 1.0 };
    // The lamp's cone, three translucent bands fading out.
    for band in 0..3 {
        let reach = TILE_SIZE * (1.2 + band as f32 * 0.9);
        let spread = 6.0 + band as f32 * 10.0;
        let alpha = 46 - band * 12;
        let nose = ahead.mul_add(TILE_SIZE * 0.45, player.x);
        let apex = Vector2::new(nose, player.y - 2.0);
        let upper = Vector2::new(ahead.mul_add(reach, nose), player.y - spread);
        let lower = Vector2::new(ahead.mul_add(reach, nose), player.y + spread);
        // Only the counter-clockwise winding fills; send both so either facing draws once.
        let color = with_alpha(CREAM, alpha as u8);
        draw.draw_triangle(apex, upper, lower, color);
        draw.draw_triangle(apex, lower, upper, color);
    }
    if (ticks / 20).is_multiple_of(2) {
        let pen = Pen::at(player.x - 9.0 * ART, player.y - 2.0 * ART);
        pen.rect(draw, 0, 0, 1, 2, AMBER);
        pen.rect(draw, 17, 0, 1, 2, AMBER);
        pen.px(draw, 0, -1, with_alpha(GOLD, 120));
        pen.px(draw, 17, -1, with_alpha(GOLD, 120));
    }
}
