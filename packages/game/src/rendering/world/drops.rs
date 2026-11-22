//! Item drops (WP-DROPS): small bobbing pixel-art items, distinct per kind (each mineral's ore
//! nugget or gem, each artifact's relic, a little cube per building block), valuables glinting
//! and glowing, stack counts, the magnet's pull, pickups flying into the rig and the "full" tag
//! over a rig whose bay or hopper turned a drop away.
//!
//! Presentation only: drops come straight from the snapshot's entity store (looked up for the
//! visible rectangle through its spatial index), animations from the shell clock.

use raylib::prelude::*;

use fatherload_simulation::{
    PlayerId,
    entity::{EntityState, EntityStore},
    item::{ItemKind, Stowage},
    terrain::TILE_SIZE,
    world::drops::DROP_COLLECT_RADIUS,
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::{
    sprite::SpriteDef,
    sprites::{icons, terrain as terrain_art},
};

use super::{ART, Pen, WorldScene, draw_world_sprite, fx::disc, snap};

/// The sprite a drop of `item` shows.
#[must_use]
pub const fn drop_sprite(item: ItemKind) -> &'static SpriteDef {
    match item {
        ItemKind::Mineral(mineral) => icons::mineral(mineral),
        ItemKind::Artifact(artifact) => terrain_art::artifact_stamp(artifact),
        ItemKind::Block(block) => icons::block(block),
        ItemKind::Layer(stratum) => {
            crate::rendering::pixel_art::sprites::deep_gear::layer_resource(stratum)
        }
        ItemKind::Material(_)
        | ItemKind::Module(_)
        | ItemKind::Weapon(_)
        | ItemKind::Ammo(_)
        | ItemKind::Explosive(_)
        | ItemKind::Cosmetic(_)
        | ItemKind::Quest(_) => &icons::CARGO,
    }
}

/// How a drop of `item` shines: the glow colour, and how strongly (0: a plain block, no glow).
fn shine(item: ItemKind) -> (Color, u8) {
    match item {
        ItemKind::Block(_) => (BONE, 0),
        ItemKind::Artifact(_) => (PINK, 90),
        ItemKind::Mineral(mineral) => {
            let value = mineral.value();
            let color = match mineral {
                fatherload_simulation::terrain::MineralKind::Copper => COPPER,
                fatherload_simulation::terrain::MineralKind::Iron => STONE_LIGHT,
                fatherload_simulation::terrain::MineralKind::Silver => ICE,
                fatherload_simulation::terrain::MineralKind::Gold => GOLD,
                fatherload_simulation::terrain::MineralKind::Emerald => LEAF,
                fatherload_simulation::terrain::MineralKind::Ruby => ROSE,
                fatherload_simulation::terrain::MineralKind::Diamond => SKY_LIGHT,
                fatherload_simulation::terrain::MineralKind::Platinum => MIST,
                fatherload_simulation::terrain::MineralKind::Uranium => LIME,
                fatherload_simulation::terrain::MineralKind::Mythril => LAVENDER,
                fatherload_simulation::terrain::MineralKind::Skystone => SKY,
                fatherload_simulation::terrain::MineralKind::CloudSilver => BONE,
            };
            // Cheap ore barely glows; jackpots shine.
            let strength = u8::try_from((value / 6).clamp(18, 80)).unwrap_or(80);
            (color, strength)
        }
        // Layer resources glow in their stratum's accent colour (Wave 8).
        ItemKind::Layer(stratum) => {
            let palette = stratum.def().palette;
            let accent = palette.accent.unwrap_or(palette.rock);
            (Color::new(accent.0, accent.1, accent.2, 255), 60)
        }
        _ => (CREAM, 40),
    }
}

/// Where rig `player` is drawn this frame, if it is in the scene.
fn rig_position(scene: &WorldScene<'_>, player: PlayerId) -> Option<(f32, f32)> {
    scene
        .players
        .iter()
        .find(|rig| rig.player_id == player)
        .map(|rig| (rig.x, rig.y))
}

/// A small per-drop phase so neighbouring drops do not bob in lockstep.
#[allow(clippy::cast_precision_loss, reason = "small hashed phase")]
fn phase(id: u64) -> f32 {
    (id.wrapping_mul(0x9e37_79b9) % 628) as f32 / 100.0
}

/// Every drop in the visible rectangle.
pub(super) fn draw_drops(draw: &mut impl RaylibDraw, scene: &WorldScene<'_>) {
    let seconds = scene.seconds();
    let margin = TILE_SIZE;
    let visible = scene.visible;
    let ids = scene.entities.ids_in_rect(
        visible.x - margin,
        visible.y - margin,
        visible.x + visible.width + margin,
        visible.y + visible.height + margin,
    );
    for id in ids {
        let Some(entity) = scene.entities.get(id) else {
            continue;
        };
        let EntityState::Drop(drop) = entity.state else {
            continue;
        };
        let (x, y) = entity.body.lerp_position(scene.bomb_alpha);
        let resting = entity.body.asleep || entity.body.rest_ticks > 0;
        if let Some(puller) = drop.pulled_by
            && let Some((rx, ry)) = rig_position(scene, puller)
        {
            draw_magnet_pull(draw, (x, y), (rx, ry), seconds + phase(id.get()));
            // A cold shimmer around a drop in the magnet's grip.
            disc(draw, x, y - ART * 3.0, 6, with_alpha(ICE, 70));
        }
        draw_drop(
            draw,
            drop.stack.kind,
            drop.stack.count,
            (x, y),
            DropLook {
                resting,
                blocked: drop.blocked,
                seconds: seconds + phase(id.get()),
            },
        );
    }
}

/// How one drop is drawn this frame.
#[derive(Clone, Copy)]
struct DropLook {
    /// On the ground (it bobs and casts a shadow) rather than flying.
    resting: bool,
    /// Turned away by a full bay or hopper (it blinks a red mark).
    blocked: bool,
    /// Animation clock, phase-shifted per drop.
    seconds: f32,
}

/// One drop centred on world `(x, y)` (its body's centre).
fn draw_drop(
    draw: &mut impl RaylibDraw,
    item: ItemKind,
    count: u32,
    (x, y): (f32, f32),
    look: DropLook,
) {
    let sprite = drop_sprite(item);
    let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
    // Resting drops float a little above the ground and bob; flying ones do not.
    let bob = if look.resting {
        (look.seconds * 3.2).sin().mul_add(ART, -ART * 2.0)
    } else {
        0.0
    };
    let left = x - width * 0.5;
    let top = y + 5.0 - height + bob;
    let (glow, strength) = shine(item);
    if look.resting {
        // A soft shadow on the ground, shrinking as the drop bobs up.
        let shadow = Pen::at(x - ART * 3.0, y + 4.0);
        let span = if bob < -ART * 2.5 { 4 } else { 6 };
        shadow.rect(draw, (6 - span) / 2, 0, span, 1, with_alpha(INK, 90));
    }
    if strength > 0 {
        let pulse = (look.seconds * 2.4).sin().mul_add(0.3, 0.7);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "alpha in 0..=255"
        )]
        let alpha = (f32::from(strength) * pulse) as u8;
        disc(
            draw,
            x,
            top + height * 0.5,
            (sprite.width() / 2 + 2).max(4),
            with_alpha(glow, alpha / 2),
        );
        disc(
            draw,
            x,
            top + height * 0.5,
            (sprite.width() / 2).max(3),
            with_alpha(glow, alpha),
        );
    }
    draw_world_sprite(draw, sprite, 0, left, top, false);
    let pen = Pen::at(left, top);
    if strength >= 40 {
        // A glint walks around the gem now and then.
        let cycle = (look.seconds * 1.3).rem_euclid(3.0);
        if cycle < 0.35 {
            let (gx, gy) = if cycle < 0.17 { (1, 1) } else { (2, 0) };
            pen.px(draw, gx, gy + 1, BONE);
            pen.px(draw, gx + 1, gy, with_alpha(BONE, 180));
            pen.px(draw, gx - 1, gy, with_alpha(BONE, 180));
            pen.px(draw, gx, gy - 1, with_alpha(BONE, 180));
        }
    }
    if count > 1 {
        draw_count(draw, count, left + width - ART, top + height - ART * 3.0);
    }
    if look.blocked && (look.seconds * 4.0) as i32 % 2 == 0 {
        // A red "no room" cross over the drop.
        let mark = Pen::at(x - ART * 2.0, top - ART * 6.0);
        for step in 0..5 {
            mark.px(draw, step, step, RED);
            mark.px(draw, 4 - step, step, RED);
        }
    }
}

/// The magnet's pull: a dotted shimmer from the drop to the rig, crawling inward.
fn draw_magnet_pull(draw: &mut impl RaylibDraw, from: (f32, f32), to: (f32, f32), seconds: f32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let distance = dx.hypot(dy);
    if distance < ART * 4.0 {
        return;
    }
    let steps = (distance / (ART * 4.0)) as i32;
    let crawl = (seconds * 6.0).rem_euclid(1.0);
    for step in 0..steps {
        let t = (step as f32 + crawl) / steps as f32;
        let alpha = t.mul_add(120.0, 110.0).min(230.0) as u8;
        let color = if step % 2 == 0 { SKY_LIGHT } else { ICE };
        draw.draw_rectangle_rec(
            Rectangle::new(
                snap(dx.mul_add(t, from.0)),
                snap(dy.mul_add(t, from.1)),
                ART,
                ART,
            ),
            with_alpha(color, alpha),
        );
    }
}

/// A tiny pixel count ("x12") with its right edge at world `x`, top at `y`.
fn draw_count(draw: &mut impl RaylibDraw, count: u32, x: f32, y: f32) {
    let text = if count > 999 {
        "999+".to_owned()
    } else {
        count.to_string()
    };
    // 3x5 digits, one art pixel apart, plus a 3x3 "x".
    let width = i32::try_from(text.len()).unwrap_or(4) * 4 + 4;
    let pen = Pen::at(x - width as f32 * ART, y);
    pen.rect(draw, -1, -1, width + 1, 7, with_alpha(INK, 200));
    for step in 0..3 {
        pen.px(draw, step, 1 + step, CREAM);
        pen.px(draw, 2 - step, 1 + step, CREAM);
    }
    for (index, digit) in text.chars().enumerate() {
        let origin = 4 + i32::try_from(index).unwrap_or(0) * 4;
        draw_digit(draw, pen, origin, digit);
    }
}

/// 3x5 pixel glyphs for the stack counter.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b011, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

fn draw_digit(draw: &mut impl RaylibDraw, pen: Pen, origin: i32, digit: char) {
    // Digits, or the "+" of "999+".
    let rows = digit
        .to_digit(10)
        .map_or([0b000, 0b010, 0b111, 0b010, 0b000], |value| {
            DIGITS[value as usize]
        });
    for (row, bits) in rows.iter().enumerate() {
        for column in 0..3 {
            if bits & (0b100 >> column) != 0 {
                pen.px(
                    draw,
                    origin + column,
                    i32::try_from(row).unwrap_or(0),
                    CREAM,
                );
            }
        }
    }
}

/// Collected drops flying into their rigs (the item is already aboard): the sprite shrinks
/// along an eased path into the rig's hatch with a sparkle trail.
pub(super) fn draw_pickup_flights(draw: &mut impl RaylibDraw, scene: &WorldScene<'_>) {
    for flight in scene.pickups {
        let Some((rx, ry)) = rig_position(scene, flight.player) else {
            continue;
        };
        let t = flight.progress();
        // A little hop on the way in.
        let arc = (t * std::f32::consts::PI).sin() * -TILE_SIZE * 0.35;
        let x = (rx - flight.from_x).mul_add(t, flight.from_x);
        let y = (ry - flight.from_y).mul_add(t, flight.from_y) + arc;
        let sprite = drop_sprite(flight.item);
        let (glow, strength) = shine(flight.item);
        if t < 0.8 {
            let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
            draw_world_sprite(draw, sprite, 0, x - width * 0.5, y - height * 0.5, false);
        } else {
            // Swallowed: a last pip.
            disc(draw, x, y, 2, with_alpha(glow, 220));
        }
        if strength > 0 {
            for trail in 1..=3_u8 {
                let back = (t - f32::from(trail) * 0.08).max(0.0);
                let tx = (rx - flight.from_x).mul_add(back, flight.from_x);
                let ty = (ry - flight.from_y).mul_add(back, flight.from_y)
                    + (back * std::f32::consts::PI).sin() * -TILE_SIZE * 0.35;
                draw.draw_rectangle_rec(
                    Rectangle::new(snap(tx), snap(ty), ART, ART),
                    with_alpha(glow, 200 - trail * 50),
                );
            }
        }
    }
}

/// "BAY FULL" / "HOPPER FULL" over the view's rig while a drop it cannot take sits in reach.
pub(super) fn draw_full_tag(draw: &mut impl RaylibDraw, scene: &WorldScene<'_>) {
    let Some(local) = scene.local else {
        return;
    };
    let Some(stowage) = turned_away(scene.entities, local.x, local.y) else {
        return;
    };
    let text = match stowage {
        Stowage::CargoBay => "BAY FULL",
        Stowage::BlockHopper => "HOPPER FULL",
    };
    let width = super::text_art_width(text);
    let pen = Pen::at(local.x, local.y - TILE_SIZE * 1.6);
    pen.rect(
        draw,
        -width / 2 - 3,
        -2,
        width + 6,
        12,
        with_alpha(INK, 210),
    );
    // The underline pulses so the tag reads as an alert without hiding the text.
    let pulse = if (scene.seconds() * 3.0) as i32 % 2 == 0 {
        RED
    } else {
        MAROON
    };
    pen.rect(draw, -width / 2 - 3, 9, width + 6, 1, pulse);
    pen.text(
        draw,
        text,
        -width / 2,
        -1,
        if matches!(stowage, Stowage::CargoBay) {
            GOLD
        } else {
            SAND
        },
    );
}

/// Which container turned a drop away near `(x, y)`, if any drop in pickup reach is marked
/// blocked (the cargo bay first).
fn turned_away(entities: &EntityStore, x: f32, y: f32) -> Option<Stowage> {
    let mut found = None;
    for id in entities.ids_in_radius(x, y, DROP_COLLECT_RADIUS + TILE_SIZE * 0.25) {
        if let Some(entity) = entities.get(id)
            && let EntityState::Drop(drop) = entity.state
            && drop.blocked
        {
            let stowage = drop.stack.kind.stowage();
            if stowage == Stowage::CargoBay {
                return Some(stowage);
            }
            found = Some(stowage);
        }
    }
    found
}
