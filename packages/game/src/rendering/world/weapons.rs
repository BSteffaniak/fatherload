//! Hardpoint weapons in the world: the gun on each rig's roof turned toward its aim, projectiles
//! in flight, plasma beams, muzzle flashes and railgun tracers, practice targets and each view's
//! aim reticle. Presentation only: everything derives from the snapshot and the shell clock.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "effect radii and alphas from bounded animation phases"
)]

use raylib::prelude::*;

use fatherload_simulation::WeaponKind;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprites::weapons as art;
use crate::{
    effects::ShotFlash,
    render_snapshot::{BeamView, PlayerPresentation, ProjectileView, TargetView},
    rendering::pixel_art::sprite::{draw_sprite_rotated, draw_sprite_rotated_flipped},
    weapon_aim::Reticle,
};

use super::{ART, fx::disc, snap};

/// The world point of a rig's hardpoint pivot: on the dome's roof (`rig_art::HARDPOINT`),
/// mirrored with the rig's facing.
#[must_use]
pub fn hardpoint(player: &PlayerPresentation, facing_left: bool) -> (f32, f32) {
    let (column, row) = crate::rendering::rig_art::HARDPOINT;
    let width = crate::rendering::rig_art::BODY_WIDTH as f32 * ART;
    let height = crate::rendering::rig_art::BODY_HEIGHT as f32 * ART;
    let left = player.x - width * 0.5;
    let top = player.y + super::rig::FOOT_OFFSET - height;
    let column = if facing_left {
        crate::rendering::rig_art::BODY_WIDTH - 1 - column
    } else {
        column
    };
    (
        (column as f32 + 0.5).mul_add(ART, left),
        (row as f32 + 1.0).mul_add(ART, top),
    )
}

/// Tier accent stripes on a weapon: one notch of gold per upgrade tier above Mk I.
fn draw_tier_notches(draw: &mut impl RaylibDraw, pivot: (f32, f32), aim: (f32, f32), mk: u8) {
    for notch in 0..mk {
        let along = f32::from(notch).mul_add(2.0, 2.0) * ART;
        let x = aim.0.mul_add(along, pivot.0);
        let y = aim.1.mul_add(along, pivot.1);
        draw.draw_rectangle_rec(Rectangle::new(snap(x), snap(y), ART, ART), GOLD);
    }
}

/// The mounted weapon on a rig, turned toward its aim (recoiling a touch while firing).
pub fn draw_mounted_weapon(
    draw: &mut impl RaylibDraw,
    player: &PlayerPresentation,
    facing_left: bool,
    ticks: u64,
) {
    let Some(pose) = player.weapon else {
        return;
    };
    let sprite = art::mounted(pose.mounted.kind);
    let pivot = hardpoint(player, facing_left);
    let (aim_x, aim_y) = pose.aim;
    let recoil = if pose.firing && (ticks / 2).is_multiple_of(2) {
        ART
    } else {
        0.0
    };
    let reach = (sprite.width() as f32).mul_add(0.5, -1.0) * ART - recoil;
    let center = Vector2::new(aim_x.mul_add(reach, pivot.0), aim_y.mul_add(reach, pivot.1));
    let degrees = aim_y.atan2(aim_x).to_degrees();
    // A turret ring under the gun.
    disc(draw, pivot.0, pivot.1 + ART, 2, STEEL_DARK);
    disc(draw, pivot.0, pivot.1 + ART, 1, STEEL);
    draw_sprite_rotated_flipped(draw, sprite, 0, center, ART, degrees, aim_x < 0.0);
    draw_tier_notches(draw, pivot, pose.aim, pose.mounted.mk);
}

/// Projectiles in flight, interpolated between ticks, with their trails.
#[allow(clippy::too_many_lines, reason = "one trail per weapon, in one pass")]
pub fn draw_projectiles(
    draw: &mut impl RaylibDraw,
    projectiles: &[ProjectileView],
    players: &[PlayerPresentation],
    alpha: f32,
    seconds: f32,
) {
    for projectile in projectiles {
        let x = (projectile.x - projectile.previous_x).mul_add(alpha, projectile.previous_x);
        let y = (projectile.y - projectile.previous_y).mul_add(alpha, projectile.previous_y);
        let speed = projectile.velocity_x.hypot(projectile.velocity_y).max(1.0);
        let (dx, dy) = (projectile.velocity_x / speed, projectile.velocity_y / speed);
        match projectile.weapon {
            WeaponKind::RocketLauncher => {
                // Exhaust flame and a smoke trail.
                for puff in 1..=6_u8 {
                    let back = f32::from(puff) * 4.0 * ART;
                    let wobble = (seconds * 30.0 + f32::from(puff)).sin() * ART;
                    let px = dx.mul_add(-back, x) - dy * wobble;
                    let py = dy.mul_add(-back, y) + dx * wobble;
                    let color = match puff {
                        1 => CREAM,
                        2 => GOLD,
                        3 => ORANGE,
                        _ => FOG,
                    };
                    let alpha = 230_u8.saturating_sub(puff * 30);
                    disc(
                        draw,
                        px,
                        py,
                        i32::from(puff / 2 + 1),
                        with_alpha(color, alpha),
                    );
                }
            }
            WeaponKind::RivetGun
            | WeaponKind::ScatterBlaster
            | WeaponKind::Sidearm
            | WeaponKind::Rifle
            | WeaponKind::Shotgun => {
                for step in 1..=4_u8 {
                    let back = f32::from(step) * 2.0 * ART;
                    draw.draw_rectangle_rec(
                        Rectangle::new(
                            snap(dx.mul_add(-back, x)),
                            snap(dy.mul_add(-back, y)),
                            ART,
                            ART,
                        ),
                        with_alpha(if step == 1 { GOLD } else { AMBER }, 220 / step),
                    );
                }
            }
            WeaponKind::Harpoon => {
                // The line paying out from the shooter's hardpoint, sagging a little.
                let anchor = players
                    .iter()
                    .find(|player| player.player_id == projectile.owner)
                    .map_or_else(
                        || (dx.mul_add(-60.0, x), dy.mul_add(-60.0, y)),
                        |player| {
                            (
                                player.x,
                                player.y - fatherload_simulation::weapon::MUZZLE_RISE,
                            )
                        },
                    );
                let length = (x - anchor.0).hypot(y - anchor.1);
                let steps = (length / (ART * 2.0)).ceil().max(1.0) as i32;
                for step in 0..steps {
                    let t = step as f32 / steps as f32;
                    let sag = (t * std::f32::consts::PI).sin() * length * 0.04;
                    draw.draw_rectangle_rec(
                        Rectangle::new(
                            snap((x - anchor.0).mul_add(t, anchor.0)),
                            snap((y - anchor.1).mul_add(t, anchor.1) + sag),
                            ART,
                            ART,
                        ),
                        with_alpha(LOAM, 230),
                    );
                }
            }
            WeaponKind::FlakCannon => {
                // A short fuse spark trail.
                for step in 1..=3_u8 {
                    let back = f32::from(step) * 3.0 * ART;
                    disc(
                        draw,
                        dx.mul_add(-back, x),
                        dy.mul_add(-back, y),
                        1,
                        with_alpha(if step == 1 { GOLD } else { FOG }, 200 / step),
                    );
                }
            }
            WeaponKind::LowGMortar => {
                // A faint orange spark trail along the lob.
                for puff in 1..=4_u8 {
                    let back = f32::from(puff) * 3.0 * ART;
                    draw.draw_rectangle_rec(
                        Rectangle::new(
                            snap(dx.mul_add(-back, x)),
                            snap(dy.mul_add(-back, y)),
                            ART,
                            ART,
                        ),
                        with_alpha(ORANGE, 200 - puff * 40),
                    );
                }
            }
            WeaponKind::PlasmaCutter
            | WeaponKind::Railgun
            | WeaponKind::ArcRifle
            | WeaponKind::GravityGun
            | WeaponKind::TetherHarpoon
            | WeaponKind::LunarCutter
            | WeaponKind::VoidLance
            | WeaponKind::Wrench
            | WeaponKind::HandPick => {}
        }
        if let Some(sprite) = art::projectile(projectile.weapon) {
            draw_sprite_rotated(
                draw,
                sprite,
                0,
                Vector2::new(snap(x), snap(y)),
                ART,
                dy.atan2(dx).to_degrees(),
            );
        }
    }
}

/// Plasma cutter beams: a hot core in a violet glow, crackling, with sparks where it bites.
pub fn draw_beams(draw: &mut impl RaylibDraw, beams: &[BeamView], seconds: f32) {
    for beam in beams {
        let (fx, fy) = beam.from;
        let (tx, ty) = beam.to;
        let length = (tx - fx).hypot(ty - fy).max(1.0);
        let (nx, ny) = (-(ty - fy) / length, (tx - fx) / length);
        let jitter = |t: f32| ((seconds * 47.0 + t * 13.0).sin() * 0.8) * ART;
        let steps = (length / (ART * 3.0)).ceil().max(1.0) as i32;
        let mut previous = Vector2::new(fx, fy);
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let wobble = if step == steps { 0.0 } else { jitter(t) };
            let point = Vector2::new(
                (tx - fx).mul_add(t, fx) + nx * wobble,
                (ty - fy).mul_add(t, fy) + ny * wobble,
            );
            draw.draw_line_ex(previous, point, ART * 4.0, with_alpha(VIOLET, 90));
            draw.draw_line_ex(previous, point, ART * 2.0, with_alpha(LAVENDER, 200));
            draw.draw_line_ex(previous, point, ART, PINK);
            previous = point;
        }
        let pulse = (seconds * 20.0).sin().abs();
        disc(
            draw,
            tx,
            ty,
            3 + (pulse * 2.0) as i32,
            with_alpha(LAVENDER, 150),
        );
        disc(draw, tx, ty, 2, CREAM);
        for spark in 0..4_u8 {
            let phase = (seconds * 6.0 + f32::from(spark) * 0.25).fract();
            let angle = f32::from(spark).mul_add(1.9, seconds * 5.0);
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap((angle.cos() * phase).mul_add(10.0 * ART, tx)),
                    snap((angle.sin() * phase).mul_add(10.0 * ART, ty)),
                    ART,
                    ART,
                ),
                with_alpha(PINK, (230.0 * (1.0 - phase)) as u8),
            );
        }
        disc(
            draw,
            fx,
            fy,
            2 + (pulse * 1.5) as i32,
            with_alpha(LAVENDER, 170),
        );
    }
}

/// A jagged electric bolt from `from` to `to` (arc rifle links and lightning): a glow, a bright
/// core and a fork or two, re-jagged every few frames.
fn draw_bolt(
    draw: &mut impl RaylibDraw,
    from: (f32, f32),
    to: (f32, f32),
    fade: f32,
    lightning: bool,
    seed: u32,
) {
    let length = (to.0 - from.0).hypot(to.1 - from.1).max(1.0);
    let (nx, ny) = (-(to.1 - from.1) / length, (to.0 - from.0) / length);
    let steps = ((length / (ART * 6.0)).ceil() as i32).clamp(2, 40);
    let amplitude = if lightning { 7.0 } else { 4.0 } * ART;
    let (glow, core) = if lightning {
        (GOLD, CREAM)
    } else {
        (SKY_LIGHT, ICE)
    };
    let width = if lightning { 3.0 } else { 2.0 };
    let mut previous = Vector2::new(from.0, from.1);
    let mut points = Vec::with_capacity(steps as usize);
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let h = (seed
            .wrapping_mul(2_654_435_761)
            .wrapping_add(step as u32 * 40_503))
            >> 8;
        let offset = if step == steps {
            0.0
        } else {
            ((h % 200) as f32 / 100.0 - 1.0) * amplitude
        };
        let point = Vector2::new(
            (to.0 - from.0).mul_add(t, from.0) + nx * offset,
            (to.1 - from.1).mul_add(t, from.1) + ny * offset,
        );
        draw.draw_line_ex(
            previous,
            point,
            ART * width * 2.5,
            with_alpha(glow, (90.0 * fade) as u8),
        );
        draw.draw_line_ex(
            previous,
            point,
            ART * width,
            with_alpha(core, (255.0 * fade) as u8),
        );
        points.push(point);
        previous = point;
    }
    // A fork off the middle.
    if let Some(middle) = points.get(points.len() / 2) {
        let fork = Vector2::new(
            nx.mul_add(amplitude * 2.0, middle.x) + (to.0 - from.0) * 0.08,
            ny.mul_add(amplitude * 2.0, middle.y) + (to.1 - from.1) * 0.08,
        );
        draw.draw_line_ex(*middle, fork, ART, with_alpha(core, (200.0 * fade) as u8));
    }
    disc(draw, to.0, to.1, 3, with_alpha(glow, (160.0 * fade) as u8));
    disc(draw, to.0, to.1, 1, with_alpha(core, (255.0 * fade) as u8));
}

/// The tether harpoon's line from the rig's hardpoint to its hook, and the gravity gun's
/// swirling field at its hold point while it is held (T21).
pub fn draw_sky_weapon_lines(
    draw: &mut impl RaylibDraw,
    players: &[PlayerPresentation],
    facing_left: impl Fn(&PlayerPresentation) -> bool,
    seconds: f32,
) {
    for player in players {
        let Some(pose) = player.weapon else {
            continue;
        };
        let pivot = hardpoint(player, facing_left(player));
        if let Some((hx, hy)) = pose.tether {
            let length = (hx - pivot.0).hypot(hy - pivot.1).max(1.0);
            let steps = (length / (ART * 2.0)).ceil().max(1.0) as i32;
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        snap((hx - pivot.0).mul_add(t, pivot.0)),
                        snap((hy - pivot.1).mul_add(t, pivot.1)),
                        ART,
                        ART,
                    ),
                    if step % 4 == 0 { CREAM } else { LOAM },
                );
            }
            // The hook's barbs in the rock.
            disc(draw, hx, hy, 2, STEEL_LIGHT);
            disc(draw, hx, hy, 1, INK);
        }
        if pose.mounted.kind == WeaponKind::GravityGun && pose.firing {
            let reach = fatherload_simulation::weapon::MUZZLE_REACH
                + fatherload_simulation::world::HOLD_REACH;
            let (cx, cy) = (
                pose.aim.0.mul_add(reach, player.x),
                pose.aim
                    .1
                    .mul_add(reach, player.y - fatherload_simulation::weapon::MUZZLE_RISE),
            );
            let pulse = (seconds * 6.0).sin().mul_add(0.5, 0.5);
            disc(draw, cx, cy, 14, with_alpha(VIOLET, 40));
            disc(
                draw,
                cx,
                cy,
                9 + (pulse * 3.0) as i32,
                with_alpha(LAVENDER, 60),
            );
            for mote in 0..8_u8 {
                let angle = f32::from(mote).mul_add(0.785, seconds * 5.0);
                let radius = (1.0 - (seconds * 1.5 + f32::from(mote) * 0.125).fract()) * 18.0 * ART;
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        snap(angle.cos().mul_add(radius, cx)),
                        snap(angle.sin().mul_add(radius, cy)),
                        ART,
                        ART,
                    ),
                    with_alpha(PINK, 200),
                );
            }
            // The beam from the muzzle to the hold point.
            draw.draw_line_ex(
                Vector2::new(pivot.0, pivot.1),
                Vector2::new(cx, cy),
                ART * 2.0,
                with_alpha(LAVENDER, 90),
            );
        }
    }
}

/// Muzzle flashes, and the railgun's lingering tracer.
#[allow(clippy::match_same_arms, reason = "one flash colour per weapon")]
pub fn draw_shots(draw: &mut impl RaylibDraw, shots: &[ShotFlash]) {
    for shot in shots {
        let t = (shot.age / shot.lifetime()).clamp(0.0, 1.0);
        let fade = 1.0 - t;
        if shot.bolt {
            // Seeded by the absolute place, so a frame move never re-jags a bolt (F7).
            let seed = (crate::view_frame::phase_x(shot.x) as i32).unsigned_abs()
                ^ ((crate::view_frame::phase_y(shot.end_y) as i32).unsigned_abs() << 7)
                ^ ((shot.age * 30.0) as u32);
            draw_bolt(
                draw,
                (shot.x, shot.y),
                (shot.end_x, shot.end_y),
                fade,
                shot.lightning,
                seed,
            );
            continue;
        }
        let length = (shot.end_x - shot.x).hypot(shot.end_y - shot.y).max(1.0);
        let (dx, dy) = (
            (shot.end_x - shot.x) / length,
            (shot.end_y - shot.y) / length,
        );
        if matches!(shot.weapon, WeaponKind::Railgun | WeaponKind::VoidLance) {
            let from = Vector2::new(shot.x, shot.y);
            let to = Vector2::new(shot.end_x, shot.end_y);
            draw.draw_line_ex(
                from,
                to,
                ART * 5.0 * fade,
                with_alpha(TEAL, (120.0 * fade) as u8),
            );
            draw.draw_line_ex(from, to, ART * 2.0, with_alpha(MINT, (230.0 * fade) as u8));
            draw.draw_line_ex(from, to, ART, with_alpha(BONE, (255.0 * fade) as u8));
            // Rings spiralling along the slug's path.
            let rings = (length / (TILE_RING_SPACING * ART)) as i32;
            for ring in 0..rings {
                let along = ring as f32 * TILE_RING_SPACING * ART;
                let x = dx.mul_add(along, shot.x);
                let y = dy.mul_add(along, shot.y);
                let swirl = ((ring as f32).mul_add(1.3, shot.age * 30.0)).sin() * 3.0 * ART * fade;
                draw.draw_rectangle_rec(
                    Rectangle::new(snap(x - dy * swirl), snap(y + dx * swirl), ART, ART),
                    with_alpha(MINT, (200.0 * fade) as u8),
                );
            }
        }
        if t > 0.6 && !matches!(shot.weapon, WeaponKind::Railgun | WeaponKind::VoidLance) {
            continue;
        }
        let (core, glow, size) = match shot.weapon {
            WeaponKind::RivetGun | WeaponKind::Rifle => (CREAM, AMBER, 2),
            WeaponKind::ScatterBlaster => (CREAM, ORANGE, 4),
            WeaponKind::PlasmaCutter => (PINK, VIOLET, 2),
            WeaponKind::RocketLauncher => (GOLD, ORANGE, 4),
            WeaponKind::Harpoon => (BONE, STEEL_LIGHT, 1),
            WeaponKind::Railgun => (BONE, MINT, 3),
            WeaponKind::ArcRifle => (ICE, SKY_LIGHT, 3),
            WeaponKind::GravityGun => (PINK, VIOLET, 3),
            WeaponKind::TetherHarpoon => (BONE, STEEL_LIGHT, 1),
            WeaponKind::FlakCannon => (GOLD, ORANGE, 4),
            WeaponKind::LunarCutter => (ICE, MINT, 3),
            WeaponKind::LowGMortar => (AMBER, ORANGE, 3),
            WeaponKind::VoidLance => (PINK, VIOLET, 4),
            WeaponKind::Sidearm | WeaponKind::Wrench | WeaponKind::HandPick => (CREAM, GOLD, 1),
            WeaponKind::Shotgun => (CREAM, ORANGE, 3),
        };
        let reach = ART * 3.0;
        let (cx, cy) = (dx.mul_add(reach, shot.x), dy.mul_add(reach, shot.y));
        disc(
            draw,
            cx,
            cy,
            size + 1,
            with_alpha(glow, (200.0 * fade) as u8),
        );
        disc(draw, cx, cy, size, with_alpha(core, (255.0 * fade) as u8));
        // Two flash spikes along the shot.
        for spike in 1..=3_u8 {
            let along = f32::from(spike).mul_add(ART * 2.0, reach);
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap(dx.mul_add(along, shot.x)),
                    snap(dy.mul_add(along, shot.y)),
                    ART,
                    ART,
                ),
                with_alpha(core, (220.0 * fade) as u8 / spike),
            );
        }
    }
}

/// Art pixels between railgun tracer rings.
const TILE_RING_SPACING: f32 = 7.0;

/// Practice targets: the bullseye on its post, with a health bar once hit.
pub fn draw_targets(draw: &mut impl RaylibDraw, targets: &[TargetView], seconds: f32) {
    for target in targets {
        let sprite = &art::TARGET;
        // The bullseye's centre is the body: the sprite's top 11 rows are the disc.
        let left = target.x - (sprite.width() as f32 * ART) * 0.5;
        let top = target.y - 5.5 * ART;
        let wobble = if target.health < 1.0 {
            ((seconds * 9.0).sin() * (1.0 - target.health)).round() * ART * 0.5
        } else {
            0.0
        };
        crate::rendering::pixel_art::sprite::draw_sprite(
            draw,
            sprite,
            0,
            Vector2::new(snap(left + wobble), snap(top)),
            ART,
            Color::WHITE,
        );
        if target.health < 1.0 {
            let width = 12.0 * ART;
            let bar_y = top - 3.0 * ART;
            draw.draw_rectangle_rec(
                Rectangle::new(snap(target.x - width * 0.5), snap(bar_y), width, ART * 2.0),
                INK,
            );
            let fill = (width - ART * 2.0) * target.health;
            let color = if target.health > 0.5 {
                LIME
            } else if target.health > 0.25 {
                AMBER
            } else {
                RED
            };
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap(target.x - width * 0.5 + ART),
                    snap(bar_y + ART * 0.5),
                    fill.max(ART),
                    ART,
                ),
                color,
            );
        }
    }
}

/// A view's aim reticle: a crosshair at the pointer or along the stick; auto-aim draws
/// brackets that close in and lock onto the chosen target.
pub fn draw_reticle(draw: &mut impl RaylibDraw, reticle: &Reticle, accent: Color, seconds: f32) {
    let (x, y) = (snap(reticle.x), snap(reticle.y));
    if reticle.locked {
        let breathe = (seconds * 6.0).sin().abs() * ART;
        let gap = 7.0f32.mul_add(ART, breathe);
        let arm = 3.0 * ART;
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let (cx, cy) = (x + sx * gap, y + sy * gap);
            let shadow = with_alpha(INK, 160);
            for (color, offset) in [(shadow, ART), (accent, 0.0)] {
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        if sx < 0.0 { cx } else { cx - arm } + offset,
                        cy + offset,
                        arm,
                        ART,
                    ),
                    color,
                );
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        cx + offset,
                        if sy < 0.0 { cy } else { cy - arm } + offset,
                        ART,
                        arm,
                    ),
                    color,
                );
            }
        }
        disc(draw, x, y, 1, with_alpha(accent, 220));
        return;
    }
    let color = if reticle.auto {
        with_alpha(accent, 150)
    } else {
        accent
    };
    let gap = 3.0 * ART;
    let arm = 3.0 * ART;
    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let (start_x, start_y) = (x + dx * gap, y + dy * gap);
        let (w, h) = if dx == 0.0 { (ART, arm) } else { (arm, ART) };
        let rect_x = if dx < 0.0 { start_x - arm } else { start_x };
        let rect_y = if dy < 0.0 { start_y - arm } else { start_y };
        draw.draw_rectangle_rec(
            Rectangle::new(rect_x + ART, rect_y + ART, w, h),
            with_alpha(INK, 150),
        );
        draw.draw_rectangle_rec(Rectangle::new(rect_x, rect_y, w, h), color);
    }
    draw.draw_rectangle_rec(Rectangle::new(x, y, ART, ART), color);
}
