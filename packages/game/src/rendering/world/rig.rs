//! Drawing rigs: the body with rolling tracks, the drill (nose drill while driving, the spinning
//! bit when drilling down or sideways), the thruster flame, exhaust and damage smoke, and a name
//! tag with fuel/hull pips over other players' rigs.

use std::collections::BTreeMap;

use raylib::prelude::*;

use drillgame_simulation::{
    DrillDirection, PlayerId,
    terrain::{TILE_SIZE, Terrain, TilePosition},
};

use crate::render_snapshot::{DangerLevel, PlayerPresentation};
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::{sprite::SpriteDef, sprites::rig};

use super::{ART, Pen, draw_world_sprite, snap, text_art_width};

/// World pixels from the rig's centre down to the bottom of its tracks.
const FOOT_OFFSET: f32 = 12.0;

/// The way each rig last faced, so a stopped rig keeps facing where it was going. Updated once
/// per frame from the snapshot, before any view draws.
#[derive(Clone, Debug, Default)]
pub struct RigFacings {
    left: BTreeMap<PlayerId, bool>,
}

impl RigFacings {
    /// Records the facing of every rig that is drilling sideways or moving horizontally.
    pub fn update(&mut self, players: &[PlayerPresentation]) {
        self.left
            .retain(|id, _| players.iter().any(|player| player.player_id == *id));
        for player in players {
            if let Some(left) = wanted_facing_left(player) {
                self.left.insert(player.player_id, left);
            }
        }
    }

    /// Forgets every rig (new world).
    pub fn clear(&mut self) {
        self.left.clear();
    }

    fn facing_left(&self, player: &PlayerPresentation) -> bool {
        wanted_facing_left(player)
            .or_else(|| self.left.get(&player.player_id).copied())
            .unwrap_or(false)
    }
}

/// The facing a rig's drill or motion asks for, if any.
fn wanted_facing_left(player: &PlayerPresentation) -> Option<bool> {
    match player.active_drill.map(|drill| drill.direction) {
        Some(DrillDirection::Left) => Some(true),
        Some(DrillDirection::Right) => Some(false),
        _ if player.velocity_x < -8.0 => Some(true),
        _ if player.velocity_x > 8.0 => Some(false),
        _ => None,
    }
}

/// The hull paint of a rig.
fn body_sprite(player: &PlayerPresentation, is_local: bool) -> &'static SpriteDef {
    if is_local {
        if player.hull < player.max_hull * 0.3 {
            &rig::RIG_DAMAGED
        } else {
            &rig::RIG
        }
    } else {
        let index = (player.player_id.get() as usize) % rig::REMOTE_BODIES.len();
        rig::REMOTE_BODIES[index]
    }
}

/// The accent colour of a remote rig's paint (name tags).
const fn remote_color(player: &PlayerPresentation) -> Color {
    const COLORS: [Color; 5] = [MINT, PINK, LIME, LAVENDER, SKY_LIGHT];
    COLORS[(player.player_id.get() as usize) % COLORS.len()]
}

fn is_airborne(terrain: &Terrain, player: &PlayerPresentation) -> bool {
    let below = TilePosition {
        x: (player.x / TILE_SIZE).floor() as i32,
        y: ((player.y + FOOT_OFFSET + 3.0) / TILE_SIZE).floor() as i32,
    };
    !terrain.is_solid_at(below)
}

/// Where a rig is drawn this frame.
#[derive(Clone, Copy)]
struct Placement {
    left_x: f32,
    top: f32,
    width: f32,
    height: f32,
    facing_left: bool,
    /// One art pixel of drilling shake (or 0).
    shake: f32,
    /// Fast animation step (drill spin, flame flicker).
    spin: usize,
}

/// Draws one rig with its drill, flame and damage effects.
pub fn draw_rig(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    player: &PlayerPresentation,
    facings: &RigFacings,
    is_local: bool,
    ticks: u64,
) {
    let body = body_sprite(player, is_local);
    let (width, height) = (body.width() as f32 * ART, body.height() as f32 * ART);
    let drilling = player.active_drill.map(|drill| drill.direction);
    let place = Placement {
        left_x: snap(player.x - width * 0.5),
        top: snap(player.y + FOOT_OFFSET - height),
        width,
        height,
        facing_left: facings.facing_left(player),
        // Drilling shakes the rig by one art pixel.
        shake: if drilling.is_some() && (ticks / 2).is_multiple_of(2) {
            ART
        } else {
            0.0
        },
        spin: (ticks / 3) as usize,
    };
    let rolling = player.velocity_x.abs() > 5.0 && drilling.is_none();
    let track_frame = if rolling {
        (player.x / 6.0).rem_euclid(2.0) as usize
    } else {
        0
    };
    let airborne = is_airborne(terrain, player);
    draw_flame(draw, player, place, airborne);
    draw_drill(draw, player, place, drilling);
    let body_y = if drilling == Some(DrillDirection::Down) {
        place.top + place.shake
    } else {
        place.top
    };
    draw_world_sprite(
        draw,
        body,
        track_frame,
        place.left_x,
        body_y,
        place.facing_left,
    );
    let pen = Pen::at(place.left_x, body_y);
    let left = place.facing_left;
    let column = |x: i32| if left { 15 - x } else { x };
    // Beacon: slow blink, fast and red when the hull is critical.
    let critical = player.hull < player.max_hull * 0.15;
    let blink_rate = if critical { 8 } else { 40 };
    if (ticks / blink_rate) % 2 == 1 {
        pen.px(draw, column(2), 0, if critical { MAROON } else { GOLD });
    }
    if player.hull_level > 2 {
        // Armour plating across the lower hull.
        pen.rect(draw, 1, 9, 14, 1, STEEL_LIGHT);
    }
    if player.radiator_level > 1 {
        for fin in 0..i32::from(player.radiator_level.min(4)) {
            pen.px(draw, column(0) + if left { 1 } else { -1 }, 5 + fin, ORANGE);
        }
    }
    draw_exhaust(draw, player, pen, left, ticks);
    // Dust kicked up behind the tracks.
    if rolling && !airborne {
        for grain in 0..3_u64 {
            let age = (ticks + grain * 5) % 15;
            let step = (age / 3) as i32;
            let x = column(0) + if left { 1 + step } else { -1 - step };
            let y = 11 - (age / 4) as i32;
            pen.px(draw, x, y, with_alpha(SAND, (200 - age * 12) as u8));
        }
    }
}

/// The thruster flame: long while climbing, a short hover flame while holding altitude.
fn draw_flame(
    draw: &mut impl RaylibDraw,
    player: &PlayerPresentation,
    place: Placement,
    airborne: bool,
) {
    let climbing = player.velocity_y < -20.0;
    let hovering = airborne && !climbing && player.velocity_y < 70.0;
    if !(climbing || hovering) {
        return;
    }
    let sprite = if climbing {
        &rig::FLAME
    } else {
        &rig::FLAME_SMALL
    };
    let offset = if place.facing_left { ART } else { -ART };
    let x = player.x - sprite.width() as f32 * ART * 0.5 + offset;
    draw_world_sprite(
        draw,
        sprite,
        place.spin % sprite.frames.len(),
        x,
        place.top + place.height - ART,
        false,
    );
}

/// The drill: the spinning bit below the rig when drilling down, otherwise the nose drill at
/// the front (spinning while drilling sideways).
fn draw_drill(
    draw: &mut impl RaylibDraw,
    player: &PlayerPresentation,
    place: Placement,
    drilling: Option<DrillDirection>,
) {
    if drilling == Some(DrillDirection::Down) {
        let drill = &rig::DRILL_DOWN;
        draw_world_sprite(
            draw,
            drill,
            place.spin % drill.frames.len(),
            player.x - drill.width() as f32 * ART * 0.5,
            place.top + place.height - ART + place.shake,
            false,
        );
        return;
    }
    let drill = &rig::DRILL_SIDE;
    let frame = if drilling.is_some() {
        place.spin % drill.frames.len()
    } else {
        0
    };
    let drill_width = drill.width() as f32 * ART;
    let x = if place.facing_left {
        place.left_x - drill_width + ART - place.shake
    } else {
        place.left_x + place.width - ART + place.shake
    };
    draw_world_sprite(
        draw,
        drill,
        frame,
        x,
        place.top + place.height - drill.height() as f32 * ART - ART,
        place.facing_left,
    );
}

/// Exhaust puffs from the stack while running, thick smoke and sparks when damaged.
fn draw_exhaust(
    draw: &mut impl RaylibDraw,
    player: &PlayerPresentation,
    pen: Pen,
    left: bool,
    ticks: u64,
) {
    let column = |x: i32| if left { 15 - x } else { x };
    let damaged = player.hull < player.max_hull * 0.35;
    let puffs = if damaged { 3 } else { 1 };
    let period = if damaged { 36 } else { 90 };
    for puff in 0..puffs {
        let age = (ticks + puff * period / puffs) % period;
        if !damaged && age > 30 {
            continue;
        }
        let rise = (age / 4) as i32;
        let drift = (age / 7) as i32;
        let size = if damaged {
            2 + (age * 2 / period) as i32
        } else {
            1
        };
        let x = column(2) + if left { drift } else { -drift } - size / 2;
        let color = if damaged {
            if age < period / 3 { STONE } else { STONE_DARK }
        } else {
            MIST
        };
        let alpha = (220 - age * 200 / period) as u8;
        pen.rect(
            draw,
            x,
            1 - rise - size,
            size,
            size,
            with_alpha(color, alpha),
        );
    }
    if damaged && (ticks / 5).is_multiple_of(7) {
        pen.px(draw, column(9), 6, GOLD);
        pen.px(draw, column(10), 5, CREAM);
    }
}

/// A name tag with fuel and hull pips floating over another player's rig.
pub fn draw_remote_tag(draw: &mut impl RaylibDraw, player: &PlayerPresentation, ticks: u64) {
    let label = format!("P{}", player.player_id.get());
    let text_width = text_art_width(&label);
    let width = text_width.max(14) + 6;
    let pen = Pen::at(
        player.x - width as f32 * ART * 0.5,
        player.y - 18.0 - 17.0 * ART,
    );
    let danger = player.danger_level();
    let border = match danger {
        DangerLevel::Nominal => INK,
        DangerLevel::Warning => AMBER,
        DangerLevel::Critical if (ticks / 10).is_multiple_of(2) => RED,
        DangerLevel::Critical => MAROON,
    };
    pen.rect(draw, 0, 0, width, 15, border);
    pen.rect(draw, 1, 1, width - 2, 13, with_alpha(NIGHT, 220));
    pen.text(
        draw,
        &label,
        (width - text_width) / 2,
        1,
        remote_color(player),
    );
    let bar_width = width - 4;
    for (row, (value, color)) in [
        (
            player.fuel / 100.0,
            if player.fuel < 30.0 { ORANGE } else { AMBER },
        ),
        (
            player.hull / player.max_hull.max(1.0),
            if player.hull < 30.0 { RED } else { LEAF },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 10 + row as i32 * 2;
        pen.rect(draw, 2, y, bar_width, 1, SLATE_DARK);
        let filled = (value.clamp(0.0, 1.0) * bar_width as f32).round() as i32;
        pen.rect(draw, 2, y, filled, 1, color);
    }
    // Pointer down to the rig.
    pen.rect(draw, width / 2 - 1, 15, 3, 1, border);
    pen.px(draw, width / 2, 16, border);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig(id: PlayerId, velocity_x: f32) -> PlayerPresentation {
        PlayerPresentation {
            player_id: id,
            x: 0.0,
            y: 0.0,
            velocity_x,
            velocity_y: 0.0,
            fuel: 1.0,
            hull: 1.0,
            max_hull: 1.0,
            scanner_level: 0,
            radiator_level: 0,
            hull_level: 0,
            active_drill: None,
        }
    }

    #[test]
    fn a_stopped_rig_keeps_its_last_facing_until_it_leaves() {
        let id = PlayerId::new(3);
        let mut facings = RigFacings::default();
        assert!(!facings.facing_left(&rig(id, 0.0)));
        facings.update(&[rig(id, -40.0)]);
        assert!(facings.facing_left(&rig(id, 0.0)));
        facings.update(&[rig(id, 0.0)]);
        assert!(facings.facing_left(&rig(id, 0.0)));
        assert!(!facings.facing_left(&rig(id, 40.0)));
        facings.update(&[]);
        assert!(!facings.facing_left(&rig(id, 0.0)));
    }
}
