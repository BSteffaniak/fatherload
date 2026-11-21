//! Drawing pilots on foot in the world (Part II F5): the pilot sprite for its pose
//! (`rendering::pilot_art`), jetpack flames, bubbles under water, the hand pick's cracks on the
//! tile it chips, small health and air gauges when they are not full, and the parked rig's
//! hazard lights and headlamp while its pilot is out.

use raylib::prelude::*;

use fatherload_simulation::{
    pilot::{PilotPose, STRIKE_GUARD_SECONDS, TetherKind},
    terrain::TILE_SIZE,
};

use crate::render_snapshot::{PilotPresentation, PlayerPresentation};
use crate::rendering::pilot_art::{PilotArms, PilotFrame, PilotLook, paint_pilot};
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
#[allow(
    clippy::too_many_lines,
    reason = "the pilot and its gear effects in draw order"
)]
pub(super) fn draw_pilot(draw: &mut impl RaylibDraw, pilot: &PilotPresentation, ticks: u64) {
    let look = PilotLook::new(pilot.cosmetics, pilot.state.gear);
    let state = &pilot.state;
    let swinging = pilot.pick.is_some() || state.pick_cooldown > 0.0 || state.swing > 0.0;
    let facing = if state.facing < 0 { -1.0 } else { 1.0 };
    let length = f32::from(state.aim_x)
        .hypot(f32::from(state.aim_y))
        .max(1.0);
    // A pick swung at a creature draws like a melee arm.
    let weapon = state.weapon.or_else(|| {
        (state.swing > 0.0 && pilot.pick.is_none())
            .then_some(fatherload_simulation::WeaponKind::HandPick)
    });
    let frame = PilotFrame {
        pose: pilot.pose,
        seconds: state.anim,
        swinging,
        arms: PilotArms {
            weapon,
            aim: (
                f32::from(state.aim_x) / length * facing,
                f32::from(state.aim_y) / length,
            ),
            swing: (state.swing / fatherload_simulation::world::PILOT_SWING_SHOW_SECONDS)
                .clamp(0.0, 1.0),
        },
        low_gravity: pilot.low_gravity,
        downed: pilot.downed,
    };
    let flip = state.facing < 0 && pilot.pose != PilotPose::Climb;
    let feet = Vector2::new(snap(pilot.x), snap(pilot.y));
    let pen = Pen {
        x: feet.x,
        y: feet.y,
    };
    let column = |x: i32| if flip { -x - 1 } else { x };
    let tint = (pilot.state.hurt > STRIKE_GUARD_SECONDS * 0.2 && (ticks / 3).is_multiple_of(2))
        .then_some(BONE);
    let canvas = paint_pilot(&look, frame);
    if let Some(rope) = state.tether {
        // The rope from the glove to its anchor: a taut line with a hook (grapple) or the
        // rig's winch drum at the top.
        let (hand_x, hand_y) = canvas.hand;
        let hand_x = if flip { -hand_x - 1 } else { hand_x };
        let from = (
            (hand_x as f32 + 0.5).mul_add(ART, feet.x),
            (hand_y as f32 + 0.5).mul_add(ART, feet.y),
        );
        draw_rope(draw, from, (rope.anchor_x, rope.anchor_y), rope.kind);
    }
    canvas.draw(draw, feet, ART, flip, tint);
    if pilot.pose == PilotPose::Hover {
        // A steady, short blue-white plume holding altitude.
        let flicker = (ticks / 3 % 2) as i32;
        pen.rect(
            draw,
            column(-8).min(column(-6)),
            -6,
            3,
            5 + flicker,
            with_alpha(SKY_LIGHT, 90),
        );
        pen.rect(draw, column(-7), -6, 1, 3 + flicker, CREAM);
        pen.rect(draw, column(-7), -3 + flicker, 1, 2, SKY_LIGHT);
    }
    if pilot.pose == PilotPose::Glide {
        // Wind streaks trailing behind the wing.
        for streak in 0..3_i32 {
            let age = ((ticks as i32) + streak * 7) % 18;
            pen.rect(
                draw,
                column(-12 - age).min(column(-9 - age)),
                -30 + streak * 3,
                4,
                1,
                with_alpha(BONE, (150 - age * 8).max(0) as u8),
            );
        }
    }
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

/// A rope from `from` to `anchor` (world pixels): a taut, slightly kinked line in rope brown
/// with a lighter twist, a three-pronged hook at a grapple's end or a winch drum at the rig.
fn draw_rope(draw: &mut impl RaylibDraw, from: (f32, f32), anchor: (f32, f32), kind: TetherKind) {
    let (dx, dy) = (anchor.0 - from.0, anchor.1 - from.1);
    let length = dx.hypot(dy).max(1.0);
    let steps = (length / ART).ceil() as i32;
    for step in 0..=steps {
        let t = step as f32 / steps.max(1) as f32;
        let x = dx.mul_add(t, from.0);
        let y = dy.mul_add(t, from.1);
        let color = if step % 3 == 0 { SAND } else { LOAM };
        draw.draw_rectangle_rec(Rectangle::new(snap(x), snap(y), ART, ART), color);
    }
    let pen = Pen::at(snap(anchor.0), snap(anchor.1));
    match kind {
        TetherKind::Grapple => {
            pen.rect(draw, -2, -1, 5, 1, STEEL_LIGHT);
            pen.rect(draw, -2, -2, 1, 1, STEEL);
            pen.rect(draw, 2, -2, 1, 1, STEEL);
            pen.rect(draw, 0, -3, 1, 2, STEEL_LIGHT);
            pen.px(draw, 0, 0, INK);
        }
        TetherKind::Winch => {
            pen.rect(draw, -2, -1, 5, 3, STEEL_DARK);
            pen.rect(draw, -1, 0, 3, 1, STEEL_LIGHT);
            pen.px(draw, 0, 2, AMBER);
        }
    }
}
