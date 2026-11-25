//! Space combat on screen (Wave 12 hostiles): the factions' ships, their shots and wrecks,
//! every telegraphed attack's warning, and the combat HUD (the deflector and missile rack, the
//! encounter's banner, a pirate demand, a boss's health bar and its warnings, the escape jump's
//! spool, and arrows to ships and salvage off screen).
//!
//! Ships are drawn from the atlas (`sprites::hostiles`, nose up) turned to their headings in
//! the seat's craft-locked view, at true scale: their hit circles cover exactly what is drawn.
//! Warnings brighten steadily toward the moment they fire and never flash (photosensitivity
//! safe); they scale with the screen effects setting only in glow, never in readability.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "screen layout mixes whole pixels and float positions"
)]

use fatherload_simulation::{
    hostiles::{
        Attack, COMBAT_STEP_HZ, EncounterState, Faction, HostileShip, ShipClass, ShotKind,
        Telegraph,
        ai::RigView,
        boss::{self, Unleash},
        combat::{from_ship_frame, nose},
    },
    position::WorldPos,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    pixel_art::{
        sprite::{self, SpriteDef},
        sprites::hostiles as art,
    },
    space_view::SpaceFrame,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// The view's zoom in a fight (a share of one to one): ships and the rig at true scale, the
/// fight around them in view.
pub const COMBAT_ZOOM: f32 = 0.55;
/// Hostiles this near (px) pull the view in to [`COMBAT_ZOOM`]; it eases in from
/// [`COMBAT_ZOOM_REACH`] + 1,200 px.
pub const COMBAT_ZOOM_REACH: f32 = 2_200.0;

/// How far into a fight the view is (0 none, 1 fully), from the nearest hostile.
#[must_use]
pub fn combat_share(nearest: Option<f32>) -> f32 {
    nearest.map_or(0.0, |distance| {
        ((COMBAT_ZOOM_REACH + 1_200.0 - distance) / 1_200.0).clamp(0.0, 1.0)
    })
}

/// The sprite of a ship class.
#[must_use]
pub const fn ship_sprite(class: ShipClass) -> &'static SpriteDef {
    match class {
        ShipClass::Cutter => &art::CUTTER,
        ShipClass::Marauder => &art::MARAUDER,
        ShipClass::PirateKing => &art::PIRATE_KING,
        ShipClass::Mite => &art::MITE,
        ShipClass::Stinger => &art::STINGER,
        ShipClass::SwarmQueen => &art::SWARM_QUEEN,
        ShipClass::Drone => &art::DRONE,
        ShipClass::Lancer => &art::LANCER,
        ShipClass::Arbiter => &art::ARBITER,
    }
}

/// The sprite of a raid trooper (Wave 12 ground raids), `None` for other creatures.
#[must_use]
pub const fn trooper_sprite(
    kind: fatherload_simulation::enemy::EnemyKind,
) -> Option<&'static SpriteDef> {
    use fatherload_simulation::enemy::EnemyKind;
    match kind {
        EnemyKind::RaidCorsair => Some(&art::RAID_CORSAIR),
        EnemyKind::SwarmSkitter => Some(&art::SWARM_SKITTER),
        EnemyKind::MachineTrooper => Some(&art::MACHINE_TROOPER),
        _ => None,
    }
}

/// The combat step of the snapshot (for telegraph progress and hit flashes).
const fn step_of(snapshot: &RenderSnapshot<'_>) -> u64 {
    snapshot.space.millis * COMBAT_STEP_HZ / 1_000
}

/// The sprite frame a ship shows: engines flicker; a boss shows its wind-up frame while it
/// warns; machines light their eyes and emitters while charging.
fn ship_frame(ship: &HostileShip, ticks: u64) -> usize {
    let flicker = usize::from((ticks / 6 + u64::from(ship.id)) % 2 == 1);
    match (ship.class, ship.telegraph.map(|telegraph| telegraph.attack)) {
        (ShipClass::PirateKing, Some(Attack::Broadside))
        | (ShipClass::SwarmQueen, Some(Attack::SporeRing | Attack::Brood)) => 2,
        (ShipClass::Drone | ShipClass::Lancer | ShipClass::Arbiter, Some(_)) => 1,
        (ShipClass::Drone | ShipClass::Lancer | ShipClass::Arbiter, None) => 0,
        _ => flicker,
    }
}

/// Draws an atlas sprite centred at `centre`, turned `degrees` and tinted.
fn draw_turned(
    draw: &mut RaylibDrawHandle<'_>,
    sprite: &SpriteDef,
    frame: usize,
    centre: Vector2,
    scale: f32,
    degrees: f32,
    tint: Color,
) {
    sprite::draw_sprite_rotated_tinted(draw, sprite, frame, centre, scale, degrees, tint);
}

/// Everything hostile near the seat's rig, in the space view (physical pixels): wrecks, ships
/// with their warnings, shots.
#[allow(
    clippy::too_many_lines,
    reason = "wrecks, ships and shots in drawing order"
)]
pub(super) fn draw_hostiles(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    ticks: u64,
) {
    let hostiles = snapshot.space.hostiles;
    if hostiles.ships.is_empty() && hostiles.shots.is_empty() && hostiles.wrecks.is_empty() {
        return;
    }
    let step = step_of(snapshot);
    let origin = frame.origin;
    let near = |pos: WorldPos, reach: f32| {
        let at = frame.project(pos);
        let margin = reach * frame.scale + 8.0;
        (at.x + margin >= frame.rect.x
            && at.x - margin <= frame.rect.x + frame.rect.width
            && at.y + margin >= frame.rect.y
            && at.y - margin <= frame.rect.y + frame.rect.height)
            .then_some(at)
    };
    // Wrecks, beneath everything.
    for wreck in &hostiles.wrecks {
        let Some(at) = near(wreck.pos, 200.0) else {
            continue;
        };
        let turn = (wreck.id as f32 * 1.7 + ticks as f32 * 0.004) - frame.roll;
        let art_scale = frame.scale * 2.0;
        if wreck.owner.is_some() {
            // A rig's own cargo pod: a small crate with its owner's light.
            let size = 10.0 * art_scale;
            draw.draw_rectangle_pro(
                Rectangle::new(at.x, at.y, size, size),
                Vector2::new(size * 0.5, size * 0.5),
                turn.to_degrees(),
                LOAM,
            );
            draw.draw_rectangle_pro(
                Rectangle::new(at.x, at.y, size * 0.6, size * 0.6),
                Vector2::new(size * 0.3, size * 0.3),
                turn.to_degrees(),
                SAND,
            );
        } else {
            draw_turned(
                draw,
                ship_sprite(wreck.class),
                0,
                at,
                art_scale,
                turn.to_degrees(),
                Color::new(120, 110, 120, 235),
            );
        }
        // The salvage glint: a slow gold pulse (a boss's tech glows brighter).
        let pulse = ((ticks as f32 * 0.05 + wreck.id as f32).sin() * 0.5 + 0.5) * 0.6 + 0.4;
        let tech = wreck.tech.is_some();
        let radius = if tech { 26.0 } else { 9.0 } * frame.scale * 2.0;
        draw.draw_ring(
            at,
            radius,
            radius + 2.0,
            0.0,
            360.0,
            32,
            with_alpha(if tech { GOLD } else { AMBER }, (pulse * 200.0) as u8),
        );
    }
    let rig_view = RigView {
        player: view.player_id,
        pos: origin,
        vx: view.player.flight.velocity_x,
        vy: view.player.flight.velocity_y,
    };
    // Ships, the warnings under them.
    for ship in &hostiles.ships {
        let reach = ship.class.reach();
        if let Some(telegraph) = ship.telegraph {
            draw_warning(draw, frame, ship, &telegraph, &rig_view, snapshot, step);
        }
        let Some(at) = near(ship.pos, reach) else {
            continue;
        };
        let degrees = (ship.angle - frame.roll).to_degrees();
        let art_scale = frame.scale * 2.0;
        draw_turned(
            draw,
            ship_sprite(ship.class),
            ship_frame(ship, ticks),
            at,
            art_scale,
            degrees,
            Color::WHITE,
        );
        // A shield taking a hit shimmers round the hull for a moment.
        let since = step.saturating_sub(ship.hit_at);
        if ship.shield > 0.0 && since < 20 && ship.hit_at > 0 {
            let alpha = (1.0 - since as f32 / 20.0) * 160.0;
            draw.draw_ring(
                at,
                reach * frame.scale,
                reach * frame.scale + 2.0,
                0.0,
                360.0,
                48,
                with_alpha(SKY_LIGHT, alpha as u8),
            );
        } else if since < 10 && ship.hit_at > 0 {
            // A hull hit: sparks off it.
            for spark in 0..5_u64 {
                let roll = fatherload_simulation::flight::space::hash64(
                    u64::from(ship.id),
                    spark,
                    ship.hit_at as i64,
                    0,
                );
                let sx = ((roll & 0xFF) as f32 / 255.0 - 0.5) * reach * 1.2 * frame.scale;
                let sy = (((roll >> 8) & 0xFF) as f32 / 255.0 - 0.5) * reach * 1.2 * frame.scale;
                let size = (2.0 * frame.scale * 2.0).max(1.0);
                draw.draw_rectangle_v(
                    Vector2::new(at.x + sx, at.y + sy),
                    Vector2::new(size, size),
                    if spark % 2 == 0 { ORANGE } else { CREAM },
                );
            }
        }
        // A small hull bar over a hurt ship (bosses have theirs on the HUD).
        if !ship.class.is_boss() && ship.hull_share() < 0.999 {
            let width = (ship.class.stats().size.0 * frame.scale).max(16.0);
            let top = at.y - reach * frame.scale - 6.0;
            draw.draw_rectangle(
                (at.x - width * 0.5) as i32,
                top as i32,
                width as i32,
                3,
                INK,
            );
            draw.draw_rectangle(
                (at.x - width * 0.5) as i32,
                top as i32,
                (width * ship.hull_share()) as i32,
                3,
                faction_colour(ship.class.faction()),
            );
        }
    }
    // Shots on top.
    for shot in &hostiles.shots {
        let Some(at) = near(shot.pos, 80.0) else {
            continue;
        };
        draw_shot(draw, frame, shot.kind, at, (shot.vx, shot.vy), ticks);
    }
}

/// A faction's colour (bars, markers, banners).
#[must_use]
pub const fn faction_colour(faction: Faction) -> Color {
    match faction {
        Faction::Pirates => ORANGE,
        Faction::Swarm => LIME,
        Faction::Machine => MINT,
    }
}

/// One shot, drawn along its motion.
fn draw_shot(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    kind: ShotKind,
    at: Vector2,
    (vx, vy): (f32, f32),
    ticks: u64,
) {
    let s = frame.scale;
    let speed = vx.hypot(vy).max(1.0);
    let (tx, ty) = frame.turn(vx / speed, vy / speed);
    let tail = |length: f32| Vector2::new(at.x - tx * length * s, at.y - ty * length * s);
    match kind {
        ShotKind::Cannon => {
            draw.draw_line_ex(at, tail(18.0), (4.0 * s).max(2.0), with_alpha(GOLD, 160));
            draw.draw_line_ex(at, tail(12.0), (2.0 * s).max(1.0), CREAM);
        }
        ShotKind::Turret => {
            draw.draw_line_ex(at, tail(14.0), (3.0 * s).max(1.5), with_alpha(SKY, 170));
            draw.draw_line_ex(at, tail(9.0), (1.5 * s).max(1.0), ICE);
        }
        ShotKind::Missile => {
            let burn = ((ticks % 4) as f32).mul_add(2.0, 10.0);
            draw.draw_line_ex(tail(8.0), tail(8.0 + burn), (4.0 * s).max(2.0), ORANGE);
            draw.draw_line_ex(at, tail(10.0), (4.0 * s).max(2.0), STEEL_LIGHT);
            draw.draw_circle_v(at, (2.0 * s).max(1.0), RED);
        }
        ShotKind::Bolt => {
            draw.draw_line_ex(at, tail(16.0), (4.0 * s).max(2.0), with_alpha(RED, 170));
            draw.draw_line_ex(at, tail(10.0), (2.0 * s).max(1.0), AMBER);
        }
        ShotKind::Broadside => {
            draw.draw_circle_v(at, 10.0 * s, with_alpha(ORANGE, 70));
            draw.draw_circle_v(at, 6.0 * s, ORANGE);
            draw.draw_circle_v(at, 3.0 * s, CREAM);
        }
        ShotKind::Spit => {
            draw.draw_circle_v(at, 8.0 * s, with_alpha(LIME, 70));
            draw.draw_circle_v(at, 6.0 * s, LEAF);
            draw.draw_circle_v(Vector2::new(at.x - 2.0 * s, at.y - 2.0 * s), 2.0 * s, LIME);
        }
        ShotKind::Spore => {
            draw.draw_circle_v(at, 13.0 * s, with_alpha(PINK, 55));
            draw.draw_circle_v(at, 9.0 * s, ROSE);
            draw.draw_circle_v(Vector2::new(at.x - 3.0 * s, at.y - 3.0 * s), 3.0 * s, PINK);
        }
        ShotKind::Pulse => {
            draw.draw_circle_v(at, 7.0 * s, with_alpha(MINT, 70));
            draw.draw_circle_v(at, 4.0 * s, MINT);
            draw.draw_circle_v(at, 2.0 * s, BONE);
        }
        ShotKind::Lance => {
            draw.draw_line_ex(at, tail(70.0), (8.0 * s).max(3.0), with_alpha(MINT, 110));
            draw.draw_line_ex(at, tail(64.0), (3.0 * s).max(1.5), BONE);
        }
    }
}

/// A telegraphed attack's warning: the line a lance or a ram will take, the mark a lunge
/// goes for, the flank a broadside comes off with its shells' paths, the spore ring with its
/// gap, the turrets and bays glowing. It brightens steadily until it fires (never a flash).
#[allow(clippy::too_many_lines, reason = "one arm per attack")]
fn draw_warning(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    ship: &HostileShip,
    telegraph: &Telegraph,
    rig: &RigView,
    snapshot: &RenderSnapshot<'_>,
    step: u64,
) {
    let span = telegraph.fire_at.saturating_sub(telegraph.since).max(1);
    let share = (step.saturating_sub(telegraph.since) as f32 / span as f32).clamp(0.0, 1.0);
    let alpha = |base: f32| (base * (0.35 + 0.65 * share)).min(255.0) as u8;
    let at = frame.project(ship.pos);
    let s = frame.scale;
    let line = |draw: &mut RaylibDrawHandle<'_>, from: (f32, f32), angle: f32, length: f32| {
        let start = frame.project(ship.pos.offset(from.0, from.1));
        let (nx, ny) = nose(angle);
        let (tx, ty) = frame.turn(nx, ny);
        // Dashes marching outward along the line.
        let dash = 26.0;
        let phase = (snapshot.space.millis as f32 * 0.08) % (dash * 2.0);
        let dashes = (length / (dash * 2.0)).ceil() as u32;
        for index in 0..dashes {
            let along = (index as f32).mul_add(dash * 2.0, phase);
            if along >= length {
                break;
            }
            let a = Vector2::new(start.x + tx * along * s, start.y + ty * along * s);
            let b_len = (along + dash).min(length);
            let b = Vector2::new(start.x + tx * b_len * s, start.y + ty * b_len * s);
            draw.draw_line_ex(a, b, (5.0 * s).max(2.0), with_alpha(RED, alpha(200.0)));
        }
        let end = Vector2::new(start.x + tx * length * s, start.y + ty * length * s);
        draw.draw_line_ex(
            start,
            end,
            (1.5 * s).max(1.0),
            with_alpha(ORANGE, alpha(150.0)),
        );
    };
    match telegraph.attack {
        Attack::Lance => {
            if let Unleash::Shots(volleys) =
                boss::unleash(ship, telegraph, rig, snapshot.space.seed)
                && ship.class == ShipClass::Arbiter
            {
                for volley in volleys {
                    line(draw, volley.from, volley.angle, 1_600.0);
                }
            } else {
                line(
                    draw,
                    from_ship_frame(ship.angle, 0.0, -44.0),
                    telegraph.angle,
                    1_500.0,
                );
            }
        }
        Attack::Ram => {
            let prow = from_ship_frame(ship.angle, 0.0, -140.0);
            line(draw, prow, ship.angle, 1_300.0);
        }
        Attack::Lunge => {
            let mark = frame.project(telegraph.mark);
            let (dx, dy) = ship.pos.delta(telegraph.mark);
            line(
                draw,
                (0.0, 0.0),
                fatherload_simulation::hostiles::combat::bearing(dx, dy),
                dx.hypot(dy),
            );
            let radius = 40.0 * s;
            draw.draw_ring(
                mark,
                radius,
                radius + 3.0,
                0.0,
                360.0,
                32,
                with_alpha(RED, alpha(220.0)),
            );
            draw.draw_line_ex(
                Vector2::new(mark.x - radius, mark.y),
                Vector2::new(mark.x + radius, mark.y),
                2.0,
                with_alpha(RED, alpha(180.0)),
            );
            draw.draw_line_ex(
                Vector2::new(mark.x, mark.y - radius),
                Vector2::new(mark.x, mark.y + radius),
                2.0,
                with_alpha(RED, alpha(180.0)),
            );
        }
        Attack::Broadside | Attack::SporeRing | Attack::Volley => {
            // Where every shell, spore or pulse will go: short paths off the hull.
            if let Unleash::Shots(volleys) =
                boss::unleash(ship, telegraph, rig, snapshot.space.seed)
            {
                let colour = match telegraph.attack {
                    Attack::Broadside => ORANGE,
                    Attack::SporeRing => PINK,
                    _ => MINT,
                };
                for volley in volleys {
                    let start = frame.project(ship.pos.offset(volley.from.0, volley.from.1));
                    let (nx, ny) = nose(volley.angle);
                    let (tx, ty) = frame.turn(nx, ny);
                    let reach = 90.0 + 160.0 * share;
                    let end = Vector2::new(start.x + tx * reach * s, start.y + ty * reach * s);
                    draw.draw_line_ex(
                        start,
                        end,
                        (3.0 * s).max(1.5),
                        with_alpha(colour, alpha(170.0)),
                    );
                    draw.draw_circle_v(start, 6.0 * s, with_alpha(colour, alpha(220.0)));
                }
            }
        }
        Attack::Escorts | Attack::Brood | Attack::Launch => {
            let radius = ship.class.reach() * s * (1.0 + 0.15 * share);
            draw.draw_ring(
                at,
                radius,
                radius + 4.0,
                0.0,
                360.0,
                48,
                with_alpha(faction_colour(ship.class.faction()), alpha(170.0)),
            );
        }
    }
}

/// The combat HUD over a seat's space view (UI pixels): the deflector and rack, the
/// encounter's banner and a pirate demand, a boss's bar and warning, the escape jump's spool
/// and arrows to ships off screen. The banners stack in the view's lower third; `top` is where
/// the flight HUD's top panels end (the arrows' lanes start there), returned as it came.
#[allow(clippy::too_many_lines, reason = "the combat HUD's parts in order")]
pub(super) fn draw_hostile_hud(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    frame: &SpaceFrame,
    rect: Rectangle,
    under_meters: f32,
    top: f32,
) -> f32 {
    let hostiles = snapshot.space.hostiles;
    let player = view.player;
    let step = step_of(snapshot);
    let gear = player.hazard_gear.arms;
    let combat = hostiles
        .rigs
        .get(&view.player_id)
        .copied()
        .unwrap_or_default();
    // The fight's banners stack below the rig (the view's lower third), clear of the ships
    // ahead of it; the arrows keep the lanes under the flight HUD's top panels.
    let lanes_top = top;
    let mut top = rect.y + rect.height * 0.7;
    // The deflector and the missile rack, under the meters.
    let capacity = fatherload_simulation::hostiles::shield_capacity(gear);
    let rack = fatherload_simulation::hostiles::missile_capacity(
        gear,
        hostiles.has_tech(fatherload_simulation::hostiles::BossTech::LanceMissiles),
    );
    let panel = Rectangle::new(
        rect.x + 4.0,
        under_meters,
        150.0,
        if rack > 0 { 26.0 } else { 16.0 },
    );
    ui_kit::panel(draw, panel, PanelStyle::Standard);
    let shield = if hostiles.rigs.contains_key(&view.player_id) {
        combat.shield
    } else {
        capacity
    };
    ui_kit::text(
        draw,
        "SHIELD",
        panel.x as i32 + 5,
        panel.y as i32 + 4,
        TextStyle::Bold,
        MIST,
    );
    ui_kit::bar(
        draw,
        Rectangle::new(panel.x + 46.0, panel.y + 4.0, panel.width - 51.0, 8.0),
        shield / capacity.max(1.0),
        ui_kit::BarColors::COOL,
        0.0,
    );
    if rack > 0 {
        ui_kit::text(
            draw,
            "MSL",
            panel.x as i32 + 5,
            panel.y as i32 + 14,
            TextStyle::Bold,
            MIST,
        );
        let missiles = if hostiles.rigs.contains_key(&view.player_id) {
            combat.missiles
        } else {
            rack
        };
        for index in 0..rack {
            let x = panel.x + 46.0 + f32::from(index) * 9.0;
            let colour = if index < missiles { RED } else { SLATE };
            draw.draw_rectangle(x as i32, panel.y as i32 + 15, 6, 7, INK);
            draw.draw_rectangle(x as i32 + 1, panel.y as i32 + 16, 4, 5, colour);
        }
    }
    let narrow = rect.width < 420.0;
    let centre_x = rect.x + rect.width * 0.5;
    let banner = |draw: &mut RaylibDrawHandle<'_>, text: &str, colour: Color, top: f32| -> f32 {
        let width = ui_kit::text_width(text, TextStyle::Bold) + 12;
        let plate = Rectangle::new(centre_x - width as f32 * 0.5, top, width as f32, 15.0);
        ui_kit::panel(draw, plate, PanelStyle::Standard);
        ui_kit::text_centred(
            draw,
            text,
            centre_x as i32,
            plate.y as i32 + 4,
            TextStyle::Bold,
            colour,
        );
        plate.y + plate.height + 3.0
    };
    // The boss's bar and its warning.
    if let Some(boss) = hostiles
        .boss()
        .filter(|boss| boss.pos.distance(player.flight.pos) < 6_000.0)
    {
        let stats = boss.class.stats();
        let width = (rect.width * 0.5).clamp(160.0, 360.0);
        let plate = Rectangle::new(centre_x - width * 0.5, top, width, 26.0);
        ui_kit::panel(draw, plate, PanelStyle::Warning);
        let phase = if boss.phase > 0 { "  ENRAGED" } else { "" };
        ui_kit::text_centred(
            draw,
            &format!("{}{phase}", boss.class.name().to_uppercase()),
            centre_x as i32,
            plate.y as i32 + 4,
            TextStyle::Bold,
            GOLD,
        );
        let bar = Rectangle::new(plate.x + 6.0, plate.y + 15.0, plate.width - 12.0, 7.0);
        ui_kit::bar(draw, bar, boss.hull_share(), ui_kit::BarColors::DANGER, 0.0);
        if stats.shield > 0.0 && boss.shield > 0.0 {
            draw.draw_rectangle(
                bar.x as i32 + 1,
                bar.y as i32 + 1,
                ((bar.width - 2.0) * boss.shield / stats.shield) as i32,
                2,
                SKY_LIGHT,
            );
        }
        top = plate.y + plate.height + 3.0;
        if let Some(telegraph) = boss.telegraph {
            top = banner(draw, telegraph.attack.warning(), ORANGE, top);
        }
    }
    // The encounter: a demand, or the fight.
    if let Some(encounter) = hostiles.engaged(view.player_id) {
        let count = hostiles.ships_of(encounter.id).count();
        match encounter.state {
            EncounterState::Demand { credits, until } => {
                let seconds = until.saturating_sub(step) as f32 / COMBAT_STEP_HZ as f32;
                let text = if narrow {
                    format!("CORSAIRS: PAY {credits} CR? {seconds:.0}s")
                } else {
                    format!(
                        "THE CORSAIRS HAIL: \"{credits} credits or we open fire\"  {seconds:.0}s"
                    )
                };
                top = banner(draw, &text, GOLD, top);
            }
            EncounterState::Closing if encounter.faction == Faction::Pirates => {
                top = banner(draw, "CORSAIRS CLOSING IN", ORANGE, top);
            }
            EncounterState::Paid => {
                top = banner(draw, "PAID OFF: THE CORSAIRS LEAVE", MIST, top);
            }
            EncounterState::Closing | EncounterState::Fighting => {
                if hostiles.boss().is_none() {
                    let text = format!(
                        "{}: {count} {}",
                        encounter.faction.name().to_uppercase(),
                        if count == 1 { "SHIP" } else { "SHIPS" }
                    );
                    top = banner(draw, &text, faction_colour(encounter.faction), top);
                }
            }
        }
    }
    // The escape jump's spool.
    if let Some(since) = combat.escape {
        let share = (step.saturating_sub(since) as f32
            / fatherload_simulation::hostiles::ESCAPE_SPOOL_STEPS as f32)
            .clamp(0.0, 1.0);
        let width = 150.0;
        let plate = Rectangle::new(centre_x - width * 0.5, top, width, 20.0);
        ui_kit::panel(draw, plate, PanelStyle::Accent);
        ui_kit::text_centred(
            draw,
            "ESCAPE JUMP",
            centre_x as i32,
            plate.y as i32 + 3,
            TextStyle::Bold,
            ICE,
        );
        ui_kit::bar(
            draw,
            Rectangle::new(plate.x + 6.0, plate.y + 12.0, plate.width - 12.0, 5.0),
            share,
            ui_kit::BarColors::COOL,
            0.0,
        );
    }
    // Arrows to ships and salvage out of view.
    let lanes = Rectangle::new(
        rect.x + 6.0,
        lanes_top + 4.0,
        rect.width - 12.0,
        rect.height - (lanes_top - rect.y) - 30.0,
    );
    let ui_per_physical = rect.width / frame.rect.width.max(1.0);
    let mut ships: Vec<&HostileShip> = hostiles
        .ships
        .iter()
        .filter(|ship| ship.pos.distance(player.flight.pos) < 5_000.0)
        .collect();
    ships.sort_by(|a, b| {
        a.pos
            .distance(player.flight.pos)
            .total_cmp(&b.pos.distance(player.flight.pos))
    });
    for ship in ships.into_iter().take(6) {
        let at = frame.project(ship.pos);
        let inside = at.x > frame.rect.x
            && at.x < frame.rect.x + frame.rect.width
            && at.y > frame.rect.y
            && at.y < frame.rect.y + frame.rect.height;
        if !inside {
            super::space_view::edge_marker(
                draw,
                rect,
                lanes,
                frame,
                player.flight.pos,
                ship.pos,
                short_name(ship.class),
                faction_colour(ship.class.faction()),
                ui_per_physical,
            );
        }
    }
    for wreck in hostiles
        .wrecks
        .iter()
        .filter(|wreck| wreck.owner.is_none_or(|owner| owner == view.player_id))
        .filter(|wreck| wreck.pos.distance(player.flight.pos) < 4_000.0)
        .take(3)
    {
        let at = frame.project(wreck.pos);
        let inside = at.x > frame.rect.x
            && at.x < frame.rect.x + frame.rect.width
            && at.y > frame.rect.y
            && at.y < frame.rect.y + frame.rect.height;
        if !inside {
            let name = if wreck.owner.is_some() {
                "YOUR POD"
            } else {
                "SALVAGE"
            };
            super::space_view::edge_marker(
                draw,
                rect,
                lanes,
                frame,
                player.flight.pos,
                wreck.pos,
                name,
                GOLD,
                ui_per_physical,
            );
        }
    }
    lanes_top
}

/// A ship's short name for its arrow.
const fn short_name(class: ShipClass) -> &'static str {
    match class {
        ShipClass::Cutter => "CUTTER",
        ShipClass::Marauder => "MARAUDER",
        ShipClass::PirateKing => "GILDED MAW",
        ShipClass::Mite => "MITE",
        ShipClass::Stinger => "STINGER",
        ShipClass::SwarmQueen => "QUEEN",
        ShipClass::Drone => "DRONE",
        ShipClass::Lancer => "LANCER",
        ShipClass::Arbiter => "ARBITER",
    }
}

/// The combat buttons' hints for a seat in a fight (the space HUD's prompt bar).
#[must_use]
pub fn combat_hints(view: &ViewSnapshot<'_>) -> Vec<super::prompts::Hint> {
    use super::prompts::hint;
    use crate::input::prompts::Prompt;
    let mut hints = Vec::new();
    if !view.space_combat.engaged {
        return hints;
    }
    if view.space_combat.demand.is_some() {
        hints.push(hint(Prompt::Use, "pay them off"));
    }
    hints.push(hint(Prompt::FireWeapon, "fire"));
    if view.player.hazard_gear.arms.missiles > 0 {
        hints.push(hint(Prompt::CycleWeapon, "missile"));
    }
    hints.push(hint(Prompt::Bomb, "escape jump"));
    hints
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether art pixel `(x, y)` of `sprite`'s first frame is part of the body (not outline
    /// ink or empty).
    fn body(sprite: &SpriteDef, x: i32, y: i32) -> bool {
        let Some(row) = usize::try_from(y)
            .ok()
            .and_then(|y| sprite.frames[0].get(y))
        else {
            return false;
        };
        let Some(symbol) = usize::try_from(x).ok().and_then(|x| row.chars().nth(x)) else {
            return false;
        };
        symbol != 'o' && sprite.color_of(symbol).is_some_and(|colour| colour.a > 0)
    }

    #[test]
    fn every_ships_hit_circles_match_its_drawn_body() {
        for class in ShipClass::ALL {
            let sprite = ship_sprite(class);
            let (w, h) = (sprite.width(), sprite.height());
            let size = class.stats().size;
            assert_eq!(
                (w as f32 * 2.0, h as f32 * 2.0),
                size,
                "{class:?}: the sprite is the drawn size (two space px an art px)"
            );
            // Every circle's middle sits on the body.
            for circle in class.hits() {
                let x = (circle.x / 2.0 + w as f32 * 0.5).floor() as i32;
                let y = (circle.y / 2.0 + h as f32 * 0.5).floor() as i32;
                assert!(body(sprite, x, y), "{class:?}: {circle:?} off the body");
            }
            // The circles cover most of the body and little else.
            let (mut covered, mut total, mut outside) = (0, 0, 0);
            for y in 0..h {
                for x in 0..w {
                    let px = (x as f32 + 0.5 - w as f32 * 0.5) * 2.0;
                    let py = (y as f32 + 0.5 - h as f32 * 0.5) * 2.0;
                    let inside = class
                        .hits()
                        .iter()
                        .any(|circle| (px - circle.x).hypot(py - circle.y) <= circle.r);
                    if body(sprite, x, y) {
                        total += 1;
                        covered += usize::from(inside);
                    } else if inside {
                        outside += 1;
                    }
                }
            }
            let share = covered as f32 / total.max(1) as f32;
            assert!(share > 0.8, "{class:?}: {share} of the body is hit area");
            assert!(
                (outside as f32) < total as f32 * 0.05,
                "{class:?}: the hit area spills off the body ({outside} px)"
            );
        }
    }

    #[test]
    fn the_raid_troopers_have_art_and_others_do_not() {
        use fatherload_simulation::enemy::EnemyKind;
        for kind in [
            EnemyKind::RaidCorsair,
            EnemyKind::SwarmSkitter,
            EnemyKind::MachineTrooper,
        ] {
            assert!(trooper_sprite(kind).is_some());
        }
        assert!(trooper_sprite(EnemyKind::CaveBat).is_none());
    }

    #[test]
    fn the_fight_zoom_eases_in() {
        assert!(combat_share(None).abs() < f32::EPSILON);
        assert!(combat_share(Some(10_000.0)).abs() < f32::EPSILON);
        assert!((combat_share(Some(500.0)) - 1.0).abs() < f32::EPSILON);
        let mid = combat_share(Some(COMBAT_ZOOM_REACH + 600.0));
        assert!(mid > 0.0 && mid < 1.0);
    }
}
