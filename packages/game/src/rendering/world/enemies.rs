//! Enemies and their shots in world space (WP-ENEMIES), plus the hit flash on struck rigs.
//! Presentation only: every pose and animation derives from entity state and the clocks.
//!
//! Each kind has one animated sprite (`sprites::enemies`); the mode picks frames (a mimic stays
//! a boulder while dormant, a spitter puffs up for its wind-up, a burrower opens its jaws to
//! lunge). Tells are drawn on top: a blinking "!" and a growing glow during wind-ups, sparks
//! while stunned, flames while burning, a shield bubble on guardians, and a health bar once an
//! enemy is hurt or hunting. Burrowers swimming through rock show only cracks and dust. Enemies
//! in tiles the view's player has not discovered stay hidden in the fog.

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles, PlayerId,
    enemy::{EnemyKind, EnemyMode, EnemyShotState, EnemyState, Locomotion, ShotKind},
    entity::{Entity, EntityState},
    terrain::{TILE_SIZE, Terrain, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::{
    sprite::{self, SpriteDef},
    sprites::enemies as art,
};

use super::{ART, Pen, fx::disc, snap};

/// The sprite of an enemy kind (exhaustive: a new kind needs art).
#[must_use]
pub const fn enemy_sprite(kind: EnemyKind) -> &'static SpriteDef {
    match kind {
        EnemyKind::CaveBat => &art::CAVE_BAT,
        EnemyKind::Burrower => &art::BURROWER,
        EnemyKind::AcidSpitter => &art::ACID_SPITTER,
        EnemyKind::RockMimic => &art::ROCK_MIMIC,
        EnemyKind::Shellback => &art::SHELLBACK,
        EnemyKind::MagmaWisp => &art::MAGMA_WISP,
        EnemyKind::BroodNest => &art::BROOD_NEST,
        EnemyKind::BroodLord => &art::BROOD_LORD,
        EnemyKind::MiningDrone => &art::MINING_DRONE,
        EnemyKind::SentryTurret => &art::SENTRY_TURRET,
        EnemyKind::RogueExcavator => &art::ROGUE_EXCAVATOR,
        EnemyKind::AncientGuardian => &art::ANCIENT_GUARDIAN,
    }
}

/// The sprite of an enemy shot (exhaustive: a new shot needs art).
#[must_use]
pub const fn shot_sprite(shot: ShotKind) -> &'static SpriteDef {
    match shot {
        ShotKind::Acid => &art::SHOT_ACID,
        ShotKind::Fireball => &art::SHOT_FIREBALL,
        ShotKind::Bolt => &art::SHOT_BOLT,
    }
}

/// What one view needs to draw enemies.
pub(super) struct EnemyScene<'a> {
    pub entities: &'a [Entity],
    pub terrain: &'a Terrain,
    pub explored: Option<&'a DiscoveredTiles>,
    pub visible: Rectangle,
    /// Presentation fraction between the last two fixed ticks.
    pub alpha: f32,
    /// The simulation tick of the snapshot (hurt flashes, stun timers).
    pub world_tick: u64,
    /// Shell ticks (60 per second) for animation.
    pub ticks: u64,
}

impl EnemyScene<'_> {
    #[allow(clippy::cast_precision_loss, reason = "animation clock")]
    fn seconds(&self) -> f32 {
        self.ticks as f32 / 60.0
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "world pixels map to tiles for the fog test"
    )]
    fn hidden(&self, x: f32, y: f32) -> bool {
        let tile = TilePosition {
            x: (x / TILE_SIZE).floor() as i32,
            y: (y / TILE_SIZE).floor() as i32,
        };
        // Open sky (above the local ground, mountains included) is never fogged.
        tile.y >= self.terrain.surface_row(tile.x).min(0)
            && self
                .explored
                .is_some_and(|explored| !explored.contains(tile))
    }

    fn on_screen(&self, x: f32, y: f32, margin: f32) -> bool {
        x > self.visible.x - margin
            && x < self.visible.x + self.visible.width + margin
            && y > self.visible.y - margin
            && y < self.visible.y + self.visible.height + margin
    }
}

/// Draws every visible enemy and shot.
pub(super) fn draw_enemies(draw: &mut impl RaylibDraw, scene: &EnemyScene<'_>) {
    for entity in scene.entities {
        let (x, y) = entity.body.lerp_position(scene.alpha);
        if !scene.on_screen(x, y, TILE_SIZE * 2.0) || scene.hidden(x, y) {
            continue;
        }
        match &entity.state {
            EntityState::Enemy(state) => draw_enemy(draw, scene, entity, state, (x, y)),
            EntityState::EnemyShot(shot) => draw_shot(draw, scene, *shot, (x, y)),
            _ => {}
        }
    }
}

/// The frame showing `state` (mode-driven poses, otherwise the loop).
#[allow(clippy::cast_precision_loss, reason = "per-enemy animation phase")]
fn enemy_frame(entity: &Entity, state: &EnemyState, seconds: f32) -> usize {
    let sprite = enemy_sprite(state.kind);
    let phase = seconds + (state.seed % 97) as f32 * 0.013;
    let moving = entity.body.velocity_x.abs() > 4.0;
    let attacking = matches!(state.mode, EnemyMode::Windup | EnemyMode::Strike);
    match state.kind {
        EnemyKind::Burrower | EnemyKind::BroodLord => usize::from(attacking),
        EnemyKind::AcidSpitter => {
            if state.mode == EnemyMode::Windup {
                2
            } else if moving {
                ((phase * 6.0) as usize) % 2
            } else {
                0
            }
        }
        EnemyKind::RockMimic => match state.mode {
            EnemyMode::Dormant => 0,
            EnemyMode::Windup | EnemyMode::Strike => 2,
            _ => 1,
        },
        EnemyKind::SentryTurret => usize::from(
            state.mode == EnemyMode::Windup && ((phase * 12.0) as usize).is_multiple_of(2),
        ),
        EnemyKind::Shellback | EnemyKind::RogueExcavator => {
            if moving || attacking {
                sprite.frame_at(phase)
            } else {
                0
            }
        }
        EnemyKind::MiningDrone if state.mode == EnemyMode::Stunned => 0,
        EnemyKind::CaveBat
        | EnemyKind::MagmaWisp
        | EnemyKind::BroodNest
        | EnemyKind::MiningDrone
        | EnemyKind::AncientGuardian => sprite.frame_at(phase),
    }
}

/// Whether a kind's art faces right and should mirror to face left.
const fn faces(kind: EnemyKind) -> bool {
    matches!(
        kind,
        EnemyKind::Burrower
            | EnemyKind::AcidSpitter
            | EnemyKind::Shellback
            | EnemyKind::SentryTurret
            | EnemyKind::RogueExcavator
    )
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    reason = "presentation math on small bounded values; one pass per enemy"
)]
fn draw_enemy(
    draw: &mut impl RaylibDraw,
    scene: &EnemyScene<'_>,
    entity: &Entity,
    state: &EnemyState,
    (x, y): (f32, f32),
) {
    let seconds = scene.seconds();
    let stats = state.kind.stats();
    let sprite = enemy_sprite(state.kind);
    let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
    let grounded = matches!(
        stats.locomotion,
        Locomotion::Walking | Locomotion::Stationary
    );
    // Walkers stand on their collision box's floor; flyers and burrowers centre on it.
    let left = x - width * 0.5;
    let top = if grounded {
        y + stats.radius - height + ART
    } else {
        y - height * 0.5
    };
    let world_tick = scene.world_tick;
    let stunned = state.mode == EnemyMode::Stunned;
    // A burrower inside rock is only cracks and dust.
    if stats.locomotion == Locomotion::Burrowing && in_rock(scene.terrain, x, y) {
        draw_burrow_trace(draw, state, x, y, seconds);
        return;
    }
    let hurt = state.hurt_until.get() > world_tick;
    let tint = if hurt {
        PINK
    } else if stunned && (seconds * 10.0) as i32 % 2 == 0 {
        SKY_LIGHT
    } else {
        Color::WHITE
    };
    let shake = if state.mode == EnemyMode::Windup && (seconds * 30.0) as i32 % 2 == 0 {
        ART
    } else {
        0.0
    };
    // Glows behind the body.
    match state.kind {
        EnemyKind::MagmaWisp => {
            let pulse = ((seconds * 5.0).sin() * 1.5) as i32;
            disc(draw, x, y, 9 + pulse, with_alpha(ORANGE, 40));
            disc(draw, x, y, 6, with_alpha(AMBER, 50));
        }
        EnemyKind::AncientGuardian => {
            disc(draw, x, y - ART * 2.0, 8, with_alpha(MINT, 30));
        }
        EnemyKind::BroodNest => {
            disc(draw, x, y + ART * 4.0, 7, with_alpha(LIME, 25));
        }
        _ => {}
    }
    sprite::draw_sprite_flipped(
        draw,
        sprite,
        enemy_frame(entity, state, seconds),
        Vector2::new(snap(left + shake), snap(top)),
        ART,
        faces(state.kind) && state.facing < 0,
        tint,
    );
    let pen = Pen::at(left, top);
    let art_width = sprite.width();
    // Wind-up tells.
    if state.mode == EnemyMode::Windup {
        if (seconds * 8.0) as i32 % 2 == 0 {
            let cx = art_width / 2 - 1;
            pen.rect(draw, cx, -8, 3, 6, INK);
            pen.rect(draw, cx + 1, -7, 1, 3, RED);
            pen.rect(draw, cx, -1, 3, 2, INK);
            pen.px(draw, cx + 1, -1, RED);
        }
        let charge = (seconds * 6.0).fract();
        let glow = match state.kind {
            EnemyKind::AcidSpitter | EnemyKind::BroodLord => Some(LIME),
            EnemyKind::MagmaWisp => Some(AMBER),
            EnemyKind::SentryTurret | EnemyKind::AncientGuardian | EnemyKind::MiningDrone => {
                Some(GOLD)
            }
            EnemyKind::RogueExcavator | EnemyKind::Shellback | EnemyKind::RockMimic => Some(RED),
            _ => None,
        };
        if let Some(glow) = glow {
            let side = if faces(state.kind) {
                f32::from(state.facing)
            } else {
                0.0
            };
            let (gx, gy) = (side.mul_add(stats.radius, x), y - ART);
            disc(
                draw,
                gx,
                gy,
                2 + (charge * 3.0) as i32,
                with_alpha(glow, 110),
            );
            disc(draw, gx, gy, 1, with_alpha(CREAM, 200));
        }
    }
    // A shield bubble while it holds.
    if state.shield > 0.0 {
        let shimmer = 90 + ((seconds * 4.0).sin() * 30.0) as i32;
        let radius = (stats.radius / ART) as i32 + 3;
        ring(
            draw,
            x,
            y,
            radius,
            with_alpha(MINT, u8::try_from(shimmer.clamp(0, 255)).unwrap_or(90)),
        );
    }
    if stunned {
        for spark in 0..3 {
            let angle = seconds * 9.0 + spark as f32 * 2.1;
            let (sx, sy) = (
                angle.cos().mul_add(stats.radius, x),
                angle.sin().mul_add(stats.radius * 0.6, y - stats.radius),
            );
            disc(draw, sx, sy, 1, with_alpha(SKY_LIGHT, 220));
        }
    }
    if state.burning_until.get() > world_tick {
        for flame in 0..3 {
            let flicker = ((seconds * 14.0) as i32 + flame * 3) % 5;
            pen.rect(
                draw,
                art_width / 4 + flame * art_width / 4 - 1,
                -2 - flicker,
                2,
                3,
                with_alpha(if flame % 2 == 0 { ORANGE } else { AMBER }, 200),
            );
        }
    }
    // Health once it is hurt or hunting (never on a sleeping mimic).
    let hurt_at_all = state.health < state.max_health - 0.5;
    if state.mode != EnemyMode::Dormant && (hurt_at_all || state.mode.hostile() || stats.elite) {
        let bar = if stats.elite {
            24
        } else {
            (art_width - 2).max(8)
        };
        let bar_x = art_width / 2 - bar / 2;
        let bar_y = -3;
        pen.rect(draw, bar_x - 1, bar_y - 1, bar + 2, 3, INK);
        let filled = ((bar as f32) * state.health_fraction()).round() as i32;
        pen.rect(
            draw,
            bar_x,
            bar_y,
            filled,
            1,
            if stats.elite { GOLD } else { RED },
        );
        if state.shield > 0.0 && stats.shield > 0.0 {
            let max_shield = stats.shield * fatherload_simulation::enemy::tier_scale(state.tier);
            let shield =
                ((bar as f32) * (state.shield / max_shield).clamp(0.0, 1.0)).round() as i32;
            pen.rect(draw, bar_x, bar_y - 2, shield, 1, MINT);
        }
    }
}

/// Whether world pixel `(x, y)` is inside solid rock.
#[allow(clippy::cast_possible_truncation, reason = "world pixels map to tiles")]
fn in_rock(terrain: &Terrain, x: f32, y: f32) -> bool {
    terrain.is_solid_at(TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    })
}

/// A burrower under the rock: a shifting crack, trickling dust, and glinting eyes when it
/// gathers itself to lunge.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "presentation math on small bounded values"
)]
fn draw_burrow_trace(draw: &mut impl RaylibDraw, state: &EnemyState, x: f32, y: f32, seconds: f32) {
    // A churning pocket in the rock: a dark hollow ringed by a ragged crack, rubble tumbling
    // through it, and the worm's eyes glinting when it hunts.
    let churn = (seconds * 10.0) as i32;
    disc(draw, x, y, 6, with_alpha(INK, 170));
    disc(draw, x, y, 4, with_alpha(SOIL_DARK, 200));
    let pen = Pen::at(x - 8.0 * ART, y - 6.0 * ART);
    for step in 0..16 {
        let wobble = (step * 7 + churn) % 3 - 1;
        let top = i32::from(step % 4 == 1);
        pen.px(draw, step, top + wobble.max(0), with_alpha(INK, 200));
        pen.px(draw, step, 11 - top - wobble.min(0), with_alpha(INK, 200));
    }
    for grit in 0..6 {
        let spin = (churn + grit * 5) % 12;
        let (gx, gy) = (3 + (grit * 3 + spin) % 10, 2 + (spin + grit * 2) % 8);
        pen.px(
            draw,
            gx,
            gy,
            with_alpha(if grit % 2 == 0 { SAND } else { CLAY }, 220),
        );
    }
    let hunting = matches!(
        state.mode,
        EnemyMode::Windup | EnemyMode::Chase | EnemyMode::Strike
    );
    if hunting && (seconds * 4.0) as i32 % 3 != 0 {
        let side = if state.facing < 0 { 4 } else { 9 };
        pen.rect(draw, side, 5, 1, 1, GOLD);
        pen.rect(draw, side + 2, 5, 1, 1, GOLD);
    }
}

/// A one-pixel ring of radius `radius` art pixels.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "small pixel circle"
)]
fn ring(draw: &mut impl RaylibDraw, x: f32, y: f32, radius: i32, color: Color) {
    let (cx, cy) = (snap(x), snap(y));
    let steps = radius * 8;
    for step in 0..steps {
        let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
        let px = (angle.cos() * radius as f32).round();
        let py = (angle.sin() * radius as f32).round();
        draw.draw_rectangle_rec(
            Rectangle::new(px.mul_add(ART, cx), py.mul_add(ART, cy), ART, ART),
            color,
        );
    }
}

#[allow(clippy::cast_precision_loss, reason = "sprite sizes are small")]
fn draw_shot(
    draw: &mut impl RaylibDraw,
    scene: &EnemyScene<'_>,
    shot: EnemyShotState,
    (x, y): (f32, f32),
) {
    let sprite = shot_sprite(shot.kind);
    let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
    let glow = match shot.kind {
        ShotKind::Acid => LIME,
        ShotKind::Fireball => ORANGE,
        ShotKind::Bolt => MINT,
    };
    disc(draw, x, y, 4, with_alpha(glow, 50));
    sprite::draw_sprite(
        draw,
        sprite,
        sprite.frame_at(scene.seconds()),
        Vector2::new(snap(x - width * 0.5), snap(y - height * 0.5)),
        ART,
        Color::WHITE,
    );
}

/// The red hit flash on rigs inside their post-hit invulnerability window.
#[allow(clippy::cast_possible_truncation, reason = "a blink phase")]
pub(super) fn draw_rig_hits(
    draw: &mut impl RaylibDraw,
    players: &[crate::render_snapshot::PlayerPresentation],
    guards: &[(PlayerId, f32)],
    ticks: u64,
) {
    for (player_id, seconds) in guards {
        if *seconds <= 0.0 || (ticks / 4) % 2 == 1 {
            continue;
        }
        if let Some(player) = players.iter().find(|player| player.player_id == *player_id) {
            let strength = (seconds / 0.8).clamp(0.0, 1.0);
            let alpha = 80.0f32.mul_add(strength, 110.0) as u8;
            ring(draw, player.x, player.y, 11, with_alpha(RED, alpha));
            disc(draw, player.x, player.y, 9, with_alpha(RED, alpha / 3));
        }
    }
}
