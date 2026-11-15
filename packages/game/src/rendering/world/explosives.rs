//! Explosive charges other than the plain bomb, their blasts, and the throw aim preview.
//! Presentation only: every animation derives from charge state and the tick clock.

use raylib::prelude::*;

use fatherload_simulation::{
    BlastDirection, ExplosiveKind, PlacedBomb,
    explosive::{BlastShape, FLARE_LIGHT_RADIUS},
    terrain::TILE_SIZE,
};

use crate::effects::{EXPLOSION_SECONDS, Explosion};
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::{sprite::SpriteDef, sprites::explosives as art};

use super::{ART, fx::disc, snap};

/// The world sprite of a charge of `kind` (`None`: the plain bomb, drawn by `fx::draw_bomb`).
#[must_use]
pub const fn charge_sprite(kind: ExplosiveKind) -> Option<&'static SpriteDef> {
    Some(match kind {
        ExplosiveKind::Bomb => return None,
        ExplosiveKind::Sticky => &art::STICKY,
        ExplosiveKind::Cluster => &art::CLUSTER,
        ExplosiveKind::Bomblet => &art::BOMBLET,
        ExplosiveKind::C4 => &art::C4,
        ExplosiveKind::Shaped => &art::SHAPED,
        ExplosiveKind::Mining => &art::MINING,
        ExplosiveKind::Incendiary => &art::INCENDIARY,
        ExplosiveKind::Emp => &art::EMP,
        ExplosiveKind::Flare => &art::FLARE,
        ExplosiveKind::Smoke => &art::SMOKE,
        ExplosiveKind::Scare => &art::SCARE,
    })
}

/// Rotation (degrees) that turns a sprite drawn "standing on the floor" so its base faces the
/// surface in `direction`.
const fn surface_degrees(direction: BlastDirection) -> f32 {
    match direction {
        BlastDirection::Down => 0.0,
        BlastDirection::Left => 90.0,
        BlastDirection::Up => 180.0,
        BlastDirection::Right => 270.0,
    }
}

/// A charge in flight, stuck to a wall, waiting for its detonator, or burning.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "animation phases from bounded clock values"
)]
pub fn draw_charge(draw: &mut impl RaylibDraw, bomb: &PlacedBomb, seconds: f32, alpha: f32) {
    let Some(sprite) = charge_sprite(bomb.kind) else {
        return;
    };
    let x = (bomb.x - bomb.previous_x).mul_add(alpha, bomb.previous_x);
    let y = (bomb.y - bomb.previous_y).mul_add(alpha, bomb.previous_y);
    if bomb.burning() {
        draw_burning(draw, bomb, x, y, seconds);
    }
    let degrees = if bomb.stuck {
        surface_degrees(bomb.direction)
    } else {
        (bomb.angle.to_degrees() / 15.0).round() * 15.0
    };
    crate::rendering::pixel_art::sprite::draw_sprite_rotated(
        draw,
        sprite,
        0,
        Vector2::new(snap(x), snap(y)),
        ART,
        degrees,
    );
    // Status lights: a waiting C4 blinks slowly; anything on a fuse blinks faster as it runs out.
    if bomb.awaiting_remote() {
        if (seconds * 2.0) as i32 % 2 == 0 {
            disc(draw, x, y - 2.0 * ART, 2, with_alpha(RED, 200));
        }
    } else if bomb.kind.has_audible_fuse() && !bomb.burning() {
        let urgent = bomb.chain_triggered || bomb.timer_seconds < 0.8;
        let rate = if urgent { 20.0 } else { 7.0 };
        if (seconds * rate) as i32 % 2 == 0 {
            disc(
                draw,
                x,
                y,
                5,
                with_alpha(if urgent { ORANGE } else { RED }, 70),
            );
        }
    }
    if !bomb.stuck && bomb.velocity_x.abs() + bomb.velocity_y.abs() > 120.0 {
        for step in 1..=3_u8 {
            let t = f32::from(step) * 0.008;
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap(bomb.velocity_x.mul_add(-t, x)),
                    snap(bomb.velocity_y.mul_add(-t, y)),
                    ART,
                    ART,
                ),
                with_alpha(FOG, 100 / step),
            );
        }
    }
}

/// A lit flare's light and flame, or an incendiary's spreading fire.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "animation phases from bounded clock values"
)]
fn draw_burning(draw: &mut impl RaylibDraw, bomb: &PlacedBomb, x: f32, y: f32, seconds: f32) {
    let flicker = ((seconds * 23.0).sin() + (seconds * 37.0).sin()) * 0.5;
    match bomb.kind {
        ExplosiveKind::Flare => {
            let reach = FLARE_LIGHT_RADIUS / ART;
            for (fraction, alpha) in [(1.0, 22_u8), (0.72, 30), (0.45, 40), (0.22, 70)] {
                let radius = (reach * fraction + flicker * 2.0) as i32;
                disc(draw, x, y, radius, with_alpha(AMBER, alpha));
            }
            let tip_y = y - 4.0 * ART;
            disc(
                draw,
                x,
                tip_y,
                3 + (flicker.abs() * 2.0) as i32,
                with_alpha(ORANGE, 220),
            );
            disc(draw, x, tip_y, 2, CREAM);
            // Sparks spitting off the tip.
            for spark in 0..4_u8 {
                let phase = (seconds * 3.0 + f32::from(spark) * 0.25).fract();
                let angle = f32::from(spark).mul_add(1.7, seconds * 0.9);
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        snap((angle.cos() * phase).mul_add(14.0, x)),
                        snap((phase * phase).mul_add(10.0, (-phase).mul_add(12.0, tip_y))),
                        ART,
                        ART,
                    ),
                    with_alpha(GOLD, (230.0 * (1.0 - phase)) as u8),
                );
            }
        }
        ExplosiveKind::Incendiary => {
            let ring = f32::from(bomb.stage) * TILE_SIZE;
            let tongues = 6 + usize::from(bomb.stage) * 4;
            for index in 0..tongues {
                let angle = index as f32 / tongues as f32 * std::f32::consts::TAU + seconds * 0.6;
                let lick = ((seconds * 9.0 + index as f32 * 1.3).sin() * 0.5 + 0.5) * 6.0;
                let px = angle.cos().mul_add(ring.max(10.0), x);
                let py = angle.sin().mul_add(ring.max(10.0), y);
                disc(draw, px, py - lick, 4, with_alpha(RED, 150));
                disc(draw, px, py - lick - 3.0, 3, with_alpha(ORANGE, 200));
                disc(draw, px, py - lick - 6.0, 1, with_alpha(GOLD, 230));
            }
            disc(draw, x, y, 10, with_alpha(ORANGE, 90));
        }
        ExplosiveKind::Smoke => draw_smoke_screen(draw, bomb, x, y, seconds),
        _ => {}
    }
}

/// A smoke screen: overlapping grey puffs that billow out to the screen's radius and drift,
/// thick enough to hide a rig inside.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "animation phases from bounded clock values"
)]
pub fn draw_smoke_screen(
    draw: &mut impl RaylibDraw,
    bomb: &PlacedBomb,
    x: f32,
    y: f32,
    seconds: f32,
) {
    let radius = fatherload_simulation::explosive::smoke_radius(bomb.stage);
    if radius <= 0.0 {
        return;
    }
    // Two rings of drifting puffs plus a dense core.
    for (ring, count, reach, size, alpha) in [
        (0_u32, 9_u32, 0.72_f32, 0.42_f32, 150_u8),
        (1, 6, 0.38, 0.5, 175),
        (2, 3, 0.1, 0.55, 190),
    ] {
        for index in 0..count {
            let seed = (bomb.id as f32).mul_add(1.37, (ring * 31 + index) as f32);
            let angle = (index as f32 / count as f32).mul_add(
                std::f32::consts::TAU,
                seconds * 0.12 * (ring as f32 + 1.0) + seed,
            );
            let wobble = (seconds * 0.7 + seed).sin() * 0.08;
            let px = angle.cos().mul_add(radius * (reach + wobble), x);
            let py = angle.sin().mul_add(radius * (reach + wobble) * 0.8, y)
                - (seconds * 0.4 + seed).sin().abs() * 4.0;
            let puff = (radius * size) as i32;
            let shade = if (index + ring) % 2 == 0 { FOG } else { MIST };
            disc(draw, px, py + 3.0, puff, with_alpha(SLATE_LIGHT, alpha / 3));
            disc(draw, px, py, puff, with_alpha(shade, alpha));
        }
    }
}

/// How big a blast of `kind` looks relative to the plain bomb's.
const fn blast_scale(kind: ExplosiveKind) -> f32 {
    match kind.blast() {
        BlastShape::Diamond(radius) | BlastShape::LootSafe(radius) => match radius {
            0 | 1 => 0.6,
            2 => 1.0,
            3 => 1.35,
            _ => 1.8,
        },
        BlastShape::Tunnel { .. } => 0.8,
        BlastShape::None => 0.4,
    }
}

/// A blast of anything but the plain bomb (that one is `fx::draw_explosion`).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "effect radii and alphas from bounded animation phases"
)]
pub fn draw_blast(draw: &mut impl RaylibDraw, explosion: &Explosion) {
    let t = (explosion.age / EXPLOSION_SECONDS).clamp(0.0, 1.0);
    let (x, y) = (explosion.x, explosion.y);
    if draw_pulse(draw, explosion, t) {
        return;
    }
    let scale = blast_scale(explosion.kind);
    if explosion.kind == ExplosiveKind::Shaped {
        draw_jet(draw, explosion, t);
    }
    if t < 0.12 {
        disc(draw, x, y, (30.0 * scale) as i32, with_alpha(CREAM, 120));
        disc(draw, x, y, (18.0 * scale) as i32, CREAM);
        return;
    }
    let dusty = explosion.kind == ExplosiveKind::Mining;
    let smoke_alpha = (200.0 * (1.0 - t)) as u8;
    let puffs = if dusty { 10 } else { 6 };
    for puff in 0..puffs {
        let angle = puff as f32 / puffs as f32 * std::f32::consts::TAU + 0.4;
        let reach = t.mul_add(26.0, 10.0) * scale;
        let px = (angle.cos() * reach * ART).mul_add(0.8, x);
        let py = (angle.sin() * reach * ART).mul_add(0.6, y) - t * 30.0;
        let color = match (dusty, puff % 2) {
            (true, 0) => LOAM,
            (true, _) => SAND,
            (false, 0) => STONE,
            (false, _) => STONE_DARK,
        };
        disc(
            draw,
            px,
            py,
            ((6.0 + t * 6.0) * scale.sqrt()) as i32,
            with_alpha(color, smoke_alpha),
        );
    }
    if t < 0.7 {
        let heat = 1.0 - t / 0.7;
        let radius = (20.0f32.mul_add(1.0 - heat, 8.0)) * scale;
        let (outer, middle) = if explosion.kind == ExplosiveKind::Incendiary {
            (MAROON, RED)
        } else {
            (RED, ORANGE)
        };
        disc(
            draw,
            x,
            y,
            radius as i32,
            with_alpha(outer, (220.0 * heat) as u8),
        );
        disc(
            draw,
            x,
            y,
            (radius * 0.75) as i32,
            with_alpha(middle, (240.0 * heat) as u8),
        );
        disc(
            draw,
            x,
            y,
            (radius * 0.45) as i32,
            with_alpha(GOLD, (255.0 * heat) as u8),
        );
        if heat > 0.6 {
            disc(draw, x, y, (radius * 0.2) as i32, CREAM);
        }
    }
}

/// The blasts that are not fireballs: an EMP's racing ring and a flare's flash. Returns
/// whether `explosion` was one of them.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "effect radii and alphas from bounded animation phases"
)]
fn draw_pulse(draw: &mut impl RaylibDraw, explosion: &Explosion, t: f32) -> bool {
    let (x, y) = (explosion.x, explosion.y);
    match explosion.kind {
        ExplosiveKind::Emp => {
            // A crackling ring racing outwards.
            let radius = t * 5.0 * TILE_SIZE;
            let alpha = (230.0 * (1.0 - t)) as u8;
            let segments = 48;
            for index in 0..segments {
                if (index + (t * 40.0) as i32) % 5 == 0 {
                    continue;
                }
                let angle = index as f32 / segments as f32 * std::f32::consts::TAU;
                let jitter = if index % 3 == 0 { ART } else { 0.0 };
                let px = angle.cos().mul_add(radius + jitter, x);
                let py = angle.sin().mul_add(radius + jitter, y);
                let color = if index % 2 == 0 { MINT } else { ICE };
                draw.draw_rectangle_rec(
                    Rectangle::new(snap(px), snap(py), ART * 2.0, ART * 2.0),
                    with_alpha(color, alpha),
                );
            }
            if t < 0.2 {
                disc(draw, x, y, 10, with_alpha(ICE, 160));
            }
            true
        }
        ExplosiveKind::Smoke => {
            // A puff of grey bursting from the canister (the screen itself is the burning
            // charge).
            let radius = (t * 2.0).min(1.0) * TILE_SIZE * 2.0;
            disc(
                draw,
                x,
                y,
                radius as i32,
                with_alpha(MIST, (160.0 * (1.0 - t)) as u8),
            );
            true
        }
        ExplosiveKind::Scare => {
            // A white flash and twin shock rings, with "bang" streaks flying off.
            if t < 0.15 {
                disc(draw, x, y, 16, with_alpha(CREAM, 230));
            }
            for (delay, color) in [(0.0, CREAM), (0.12, GOLD)] {
                let local = ((t - delay) / (1.0 - delay)).clamp(0.0, 1.0);
                if local <= 0.0 {
                    continue;
                }
                let radius = local * 4.0 * TILE_SIZE;
                let alpha = (220.0 * (1.0 - local)) as u8;
                let segments = 40;
                for index in 0..segments {
                    let angle = index as f32 / segments as f32 * std::f32::consts::TAU;
                    draw.draw_rectangle_rec(
                        Rectangle::new(
                            snap(angle.cos().mul_add(radius, x)),
                            snap(angle.sin().mul_add(radius, y)),
                            ART * 2.0,
                            ART * 2.0,
                        ),
                        with_alpha(color, alpha),
                    );
                }
            }
            for streak in 0..8_u8 {
                let angle = f32::from(streak) * std::f32::consts::FRAC_PI_4 + 0.3;
                let near = t.mul_add(30.0, 10.0);
                let far = near + 10.0;
                let alpha = (240.0 * (1.0 - t)) as u8;
                draw.draw_line_ex(
                    Vector2::new(angle.cos().mul_add(near, x), angle.sin().mul_add(near, y)),
                    Vector2::new(angle.cos().mul_add(far, x), angle.sin().mul_add(far, y)),
                    ART,
                    with_alpha(GOLD, alpha),
                );
            }
            true
        }
        ExplosiveKind::Flare => {
            if t < 0.3 {
                disc(
                    draw,
                    x,
                    y,
                    (14.0 * (1.0 - t / 0.3)) as i32 + 2,
                    with_alpha(CREAM, 200),
                );
            }
            true
        }
        _ => false,
    }
}

/// A shaped charge's cutting jet: a white-hot streak down the new tunnel, fading from the tip.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "effect radii and alphas from bounded animation phases"
)]
fn draw_jet(draw: &mut impl RaylibDraw, explosion: &Explosion, t: f32) {
    let BlastShape::Tunnel { length, .. } = ExplosiveKind::Shaped.blast() else {
        return;
    };
    let (dx, dy) = explosion.direction.step();
    let reach = (t / 0.25).min(1.0) * length as f32 * TILE_SIZE;
    let fade = 1.0 - t;
    let steps = (reach / (ART * 4.0)) as i32;
    for step in 0..=steps {
        let along = step as f32 * ART * 4.0;
        let px = (dx as f32).mul_add(along, explosion.x);
        let py = (dy as f32).mul_add(along, explosion.y);
        let taper = 1.0 - along / (length as f32 * TILE_SIZE);
        let width = (taper * 9.0 * fade) as i32 + 1;
        disc(
            draw,
            px,
            py,
            width + 3,
            with_alpha(ORANGE, (140.0 * fade) as u8),
        );
        disc(draw, px, py, width, with_alpha(GOLD, (220.0 * fade) as u8));
        if fade > 0.5 {
            disc(draw, px, py, (width / 2).max(1), CREAM);
        }
    }
}

/// The throw aim preview: a dotted arc along the path the charge will take, ending in a
/// landing marker (a bracket where a sticky charge will cling).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "dot spacing from short preview paths"
)]
pub fn draw_aim_preview(
    draw: &mut impl RaylibDraw,
    path: &[(f32, f32)],
    sticks: bool,
    seconds: f32,
) {
    if path.len() < 2 {
        return;
    }
    // Dots march along the arc so it reads as "this way".
    let march = (seconds * 12.0) as usize;
    for (index, (x, y)) in path.iter().enumerate().skip(2) {
        if !(index + march).is_multiple_of(3) {
            continue;
        }
        let fade = 1.0 - index as f32 / path.len() as f32;
        draw.draw_rectangle_rec(
            Rectangle::new(
                snap(*x) - ART * 0.5,
                snap(*y) - ART * 0.5,
                ART * 2.0,
                ART * 2.0,
            ),
            with_alpha(INK, (120.0 * fade + 60.0) as u8),
        );
        draw.draw_rectangle_rec(
            Rectangle::new(snap(*x), snap(*y), ART, ART),
            with_alpha(CREAM, (160.0 * fade + 80.0) as u8),
        );
    }
    let (end_x, end_y) = path[path.len() - 1];
    let pulse = ((seconds * 6.0).sin() * 0.5 + 0.5) * 2.0;
    let size = 5.0 + pulse;
    let color = if sticks { LIME } else { AMBER };
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let cx = snap((sx * size).mul_add(ART, end_x));
        let cy = snap((sy * size).mul_add(ART, end_y));
        draw.draw_rectangle_rec(Rectangle::new(cx - ART, cy, ART * 3.0, ART), color);
        draw.draw_rectangle_rec(Rectangle::new(cx, cy - ART, ART, ART * 3.0), color);
    }
}
