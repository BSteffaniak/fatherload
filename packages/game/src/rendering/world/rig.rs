//! Drawing rigs in the world: the modular rig sprite (`rendering::rig_art`) posed from the
//! snapshot (rolling tracks, the drill on the nose, below or on the roof, thruster flames while
//! climbing or hovering, drilling shake, beacon, scorch marks), exhaust and damage smoke, track
//! dust, and a name tag with fuel/hull pips over other players' rigs.

use std::collections::BTreeMap;

use raylib::prelude::*;

use fatherload_simulation::{
    DrillDirection, PlayerId, RigCosmetics, cosmetics::RoofAttachment, terrain::Terrain,
};

use crate::render_snapshot::{DangerLevel, PlayerPresentation};
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::rig_art::{
    self, BODY_HEIGHT, BODY_WIDTH, DrillPose, RigLook, RigPose, Thrust,
};

use super::{ART, Pen, snap, text_art_width};

/// World pixels from the rig's centre down to the bottom of its tracks.
pub(super) const FOOT_OFFSET: f32 = 12.0;

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

    pub(super) fn facing_left(&self, player: &PlayerPresentation) -> bool {
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

/// A player's accent colour, matching their hull paint (name tags, HUD title, minimap marker):
/// their garage paint job's primary colour, or their default.
#[must_use]
pub fn player_accent(player_id: PlayerId, cosmetics: Option<RigCosmetics>) -> Color {
    rig_art::player_accent(player_id, cosmetics)
}

fn is_airborne(terrain: &Terrain, player: &PlayerPresentation) -> bool {
    let below = crate::view_frame::frame().tile_at(player.x, player.y + FOOT_OFFSET + 3.0);
    !terrain.is_solid_at(below)
}

/// Draws one rig: the modular rig sprite (flames, drill, hull, parts) for its pose, then
/// exhaust, damage smoke and track dust.
pub fn draw_rig(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    player: &PlayerPresentation,
    facings: &RigFacings,
    ticks: u64,
) {
    let look = RigLook::from_presentation(player);
    let (width, height) = (BODY_WIDTH as f32 * ART, BODY_HEIGHT as f32 * ART);
    let drilling = player.active_drill.map(|drill| drill.direction);
    let left_x = snap(player.x - width * 0.5);
    let top = snap(player.y + FOOT_OFFSET - height);
    let facing_left = facings.facing_left(player);
    // Drilling shakes the rig by one art pixel.
    let shake = i32::from(drilling.is_some() && (ticks / 2).is_multiple_of(2));
    let rolling = player.velocity_x.abs() > 5.0 && drilling.is_none();
    let track_frame = if rolling {
        (crate::view_frame::phase_x(player.x) / 6.0).rem_euclid(2.0) as usize
    } else {
        0
    };
    let airborne = is_airborne(terrain, player);
    let climbing = player.velocity_y < -20.0;
    let hovering = airborne && !climbing && player.velocity_y < 70.0;
    // Beacon: slow blink, fast and red when the hull is critical.
    let critical = player.hull < player.max_hull * 0.15;
    let blink_rate = if critical { 8 } else { 40 };
    let beacon = ((ticks / blink_rate) % 2 == 1).then_some(if critical { MAROON } else { GOLD });
    let pose = RigPose {
        drill: match drilling {
            Some(DrillDirection::Down) => DrillPose::Down,
            Some(DrillDirection::Up) => DrillPose::Up,
            other => DrillPose::Nose {
                spinning: other.is_some(),
            },
        },
        thrust: if climbing {
            Thrust::Climb
        } else if hovering {
            Thrust::Hover
        } else {
            Thrust::Off
        },
        track_frame,
        shake: if drilling.is_some_and(DrillDirection::is_sideways) {
            shake
        } else {
            0
        },
        spin: (ticks / 3) as usize,
        ticks,
        beacon,
        damaged: player.hull < player.max_hull * 0.3,
        gimbal_live: airborne
            && drilling.is_some_and(DrillDirection::is_sideways)
            && (ticks / 4).is_multiple_of(2),
    };
    let body_y = match drilling {
        Some(DrillDirection::Down) => top + shake as f32 * ART,
        Some(DrillDirection::Up) => top - shake as f32 * ART,
        _ => top,
    };
    rig_art::paint_rig(&look, &pose).draw(draw, Vector2::new(left_x, body_y), ART, facing_left);
    super::weapons::draw_mounted_weapon(draw, player, facing_left, ticks);
    let pen = Pen::at(left_x, body_y);
    draw_exhaust(draw, player, &look, pen, facing_left, ticks);
    // Dust kicked up behind the tracks.
    let column = |x: i32| if facing_left { 15 - x } else { x };
    if rolling && !airborne {
        let tail = -class_overhang(&look);
        for grain in 0..3_u64 {
            let age = (ticks + grain * 5) % 15;
            let step = (age / 3) as i32;
            let x = column(tail) + if facing_left { 1 + step } else { -1 - step };
            let y = 11 - (age / 4) as i32;
            pen.px(draw, x, y, with_alpha(SAND, (200 - age * 12) as u8));
        }
    }
}

/// How far thruster pods stick out behind the tail (dust starts behind them).
const fn class_overhang(look: &RigLook) -> i32 {
    match look.tiers.engine {
        0..=2 => 0,
        3..=5 => 3,
        6..=9 => 4,
        10..=12 => 5,
        _ => 6,
    }
}

/// Exhaust puffs from the stack while running, thick smoke and sparks when damaged.
fn draw_exhaust(
    draw: &mut impl RaylibDraw,
    player: &PlayerPresentation,
    look: &RigLook,
    pen: Pen,
    left: bool,
    ticks: u64,
) {
    let column = |x: i32| if left { 15 - x } else { x };
    let (stack_x, stack_y) = look.exhaust();
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
        let x = column(stack_x) + if left { drift } else { -drift } - size / 2;
        let color = if damaged {
            if age < period / 3 { STONE } else { STONE_DARK }
        } else {
            MIST
        };
        let alpha = (220 - age * 200 / period) as u8;
        pen.rect(
            draw,
            x,
            stack_y - rise - size,
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

/// Art pixels the name tag rises to clear tall cargo holds and roof attachments.
const fn tag_lift(player: &PlayerPresentation) -> i32 {
    let cargo = match player.tiers.cargo {
        0..=5 => 0,
        6..=9 => 1,
        10..=12 => 2,
        _ => 3,
    };
    let roof = match player.cosmetics.roof {
        RoofAttachment::None => 0,
        RoofAttachment::Siren | RoofAttachment::LightBar => 1,
        RoofAttachment::Dish | RoofAttachment::Trophy => 3,
        RoofAttachment::Antenna | RoofAttachment::Halo => 5,
    };
    cargo + roof
}

/// A name tag with fuel and hull pips floating over another player's rig.
pub fn draw_remote_tag(draw: &mut impl RaylibDraw, player: &PlayerPresentation, ticks: u64) {
    let label = format!("P{}", player.player_id.get());
    let text_width = text_art_width(&label);
    let width = text_width.max(14) + 6;
    let pen = Pen::at(
        player.x - width as f32 * ART * 0.5,
        player.y - 18.0 - 17.0 * ART - tag_lift(player) as f32 * ART,
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
        rig_art::paint_accent(player.cosmetics.primary),
    );
    let bar_width = width - 4;
    for (row, (value, color)) in [
        (
            player.fuel_fraction(),
            if player.fuel_low() { ORANGE } else { AMBER },
        ),
        (
            player.hull_fraction(),
            if player.hull_low() { RED } else { LEAF },
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
            fuel_capacity: 1.0,
            hull: 1.0,
            max_hull: 1.0,
            scanner_level: 0,
            active_drill: None,
            tiers: crate::render_snapshot::RigTiers::STOCK,
            cosmetics: RigCosmetics::default_for(id),
            weapon: None,
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

    #[test]
    fn each_player_has_one_paint_and_accent_everywhere() {
        let distinct = (1..=6_u64)
            .map(|id| player_accent(PlayerId::new(id), None))
            .map(|color| (color.r, color.g, color.b))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(distinct.len(), 6, "players 1-6 get distinct default paints");
        assert_eq!(
            player_accent(PlayerId::new(7), None),
            player_accent(PlayerId::new(1), None),
            "default paints cycle"
        );
        let chosen = RigCosmetics {
            primary: fatherload_simulation::cosmetics::PaintColor::Violet,
            ..RigCosmetics::default()
        };
        let accent = player_accent(PlayerId::new(1), Some(chosen));
        let mut presented = rig(PlayerId::new(1), 0.0);
        presented.cosmetics = chosen;
        assert_eq!(
            rig_art::paint_accent(RigLook::from_presentation(&presented).cosmetics.primary),
            accent,
            "the chosen paint is the accent everywhere"
        );
    }
}
