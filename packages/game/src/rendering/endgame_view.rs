//! The endgame on screen (Wave 13 endgame): the Works counter's bay and the Records wall inside
//! a docked station, the Halcyon Gate in space, an estate's wings on alien ground and the
//! Starwright ship's plating in flight.
//!
//! * **The Works**: crates of the eight space materials along the deck (each stencilled with
//!   its icon) and the station's own centrepiece: at Halcyon Dock the Halcyon Gate's model,
//!   one ring segment lit per part delivered, the lens and keystone last; at a great yard the
//!   Starwright hull on its cradle, framed ribs until each part is fitted; at an alien-run
//!   station a grown vat with a pod per alien tech, one ring per mark; elsewhere the
//!   outfitter's paint booth with its swatches.
//! * **The Records**: a banner per boss of the hunt list (gold and lit when beaten, dark with
//!   a red mark when not) and the completion dial.
//!
//! Nothing flashes: lights hold steady or breathe slowly.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::while_float,
    reason = "screen layout mixes whole pixels and float positions; art walks its pixels"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    endgame::{
        StarwrightPart,
        hunt::Quarry,
        materials::SPACE_MATERIALS,
        works::{GATE_PARTS, OUTFITTER, builds_starwright, has_outfitter},
    },
    hostiles::Faction,
    station::{AlienTech, StationProfile},
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    pixel_art::{sprite::draw_sprite, sprites::icons},
    rig_art::paint_ramp,
    ui_kit::{self, TextStyle},
};

/// An art-pixel rectangle.
fn block(draw: &mut impl RaylibDraw, x: f32, y: f32, w: f32, h: f32, colour: Color) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    draw.draw_rectangle(
        x.floor() as i32,
        y.floor() as i32,
        w.ceil() as i32,
        h.ceil() as i32,
        colour,
    );
}

/// A slow breath between 0 and 1 (never a flash: a period of several seconds).
fn breath(seconds: f32, period: f32) -> f32 {
    (seconds * std::f32::consts::TAU / period)
        .sin()
        .mul_add(0.5, 0.5)
}

// --- The Works ------------------------------------------------------------------------------

/// The Works counter's bay (see the module docs).
pub fn draw_works(
    draw: &mut impl RaylibDraw,
    stage: Rectangle,
    left: f32,
    floor_y: f32,
    profile: &StationProfile,
    world: &WorldState,
    player_id: PlayerId,
    seconds: f32,
) {
    let width = (stage.width - 24.0).clamp(110.0, 260.0);
    // The centrepiece first, the crates in front of it.
    if profile.halcyon {
        // Sized to the stage (a split-screen sheet is short).
        let room = (floor_y - stage.y - 56.0) * 0.5;
        let radius = (width * 0.22).clamp(30.0, 50.0).min(room.max(18.0));
        draw_gate_model(
            draw,
            left + width * 0.5,
            floor_y - radius - 46.0,
            radius,
            world,
            seconds,
        );
    } else if builds_starwright(profile) {
        draw_starwright_cradle(draw, left, floor_y, width, world, player_id, seconds);
    } else if profile.alien_tech {
        draw_tech_vat(draw, left + width * 0.5, floor_y, world, player_id, seconds);
    } else if has_outfitter(profile) {
        draw_paint_booth(draw, left, floor_y, width, seconds);
    }
    draw_material_crates(draw, left, floor_y, width, world, player_id);
}

/// Crates of every space material along the deck, each stencilled with its icon; a crate the
/// bay holds some of wears a lit tag.
fn draw_material_crates(
    draw: &mut impl RaylibDraw,
    left: f32,
    floor_y: f32,
    width: f32,
    world: &WorldState,
    player_id: PlayerId,
) {
    let count = SPACE_MATERIALS.len() as f32;
    let step = (width / count).clamp(14.0, 26.0);
    let crate_w = step - 3.0;
    let bay = world.player(player_id).map(|player| &player.cargo_bay);
    for (index, mineral) in SPACE_MATERIALS.iter().enumerate() {
        let x = (left + index as f32 * step).floor();
        let h = 16.0 + (index % 3) as f32 * 2.0;
        let y = floor_y - h;
        block(draw, x, y, crate_w, h, LOAM);
        block(draw, x, y, crate_w, 2.0, CLAY);
        block(draw, x, y + h - 2.0, crate_w, 2.0, SOIL_DARK);
        block(draw, x + crate_w - 2.0, y, 2.0, h, SOIL);
        let icon = icons::mineral(*mineral);
        draw_sprite(
            draw,
            icon,
            0,
            Vector2::new(x + (crate_w - 9.0) * 0.5, y + 4.0),
            1.0,
            Color::WHITE,
        );
        let held = bay.is_some_and(|bay| bay.mineral(*mineral) > 0);
        block(
            draw,
            x + 1.0,
            y - 3.0,
            4.0,
            3.0,
            if held { MINT } else { SLATE_DARK },
        );
    }
}

/// The Halcyon Gate's model on a stand: six ring segments, the lens in the middle and the
/// keystone on top; each delivered part lit.
fn draw_gate_model(
    draw: &mut impl RaylibDraw,
    cx: f32,
    cy: f32,
    radius: f32,
    world: &WorldState,
    seconds: f32,
) {
    let endgame = world.endgame();
    // The stand: a post from the ring's foot to a plinth on the deck.
    block(draw, cx - 3.0, cy + radius - 2.0, 6.0, 40.0, STEEL_DARK);
    block(draw, cx - 2.0, cy + radius - 2.0, 1.0, 40.0, STEEL);
    block(draw, cx - 20.0, cy + radius + 36.0, 40.0, 8.0, STEEL);
    block(draw, cx - 20.0, cy + radius + 36.0, 40.0, 1.0, STEEL_LIGHT);
    // A placard: parts delivered.
    let label = format!("{}/8", endgame.gate_parts_done());
    ui_kit::text_shadowed(
        draw,
        &label,
        (cx - 8.0) as i32,
        (cy + radius + 37.0) as i32,
        TextStyle::Body,
        INK,
        None,
    );
    draw_gate_ring(
        draw,
        cx,
        cy,
        radius,
        |part| endgame.gate_part(part),
        seconds,
    );
}

/// The gate's ring at `(cx, cy)`: `delivered(part)` lights segment `part` (0-5), the lens (6)
/// and the keystone (7). Shared by the Works' model and the gate in space.
pub fn draw_gate_ring(
    draw: &mut impl RaylibDraw,
    cx: f32,
    cy: f32,
    radius: f32,
    delivered: impl Fn(u8) -> bool,
    seconds: f32,
) {
    let open = (0..GATE_PARTS).all(&delivered);
    let thickness = (radius * 0.16).max(3.0);
    let steps = (radius * 7.0) as i32;
    for step in 0..steps {
        let t = step as f32 / steps as f32;
        let angle = t * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let segment = ((t * 6.0) as u8).min(5);
        let gap = (t * 6.0).fract();
        if !(0.04..=0.96).contains(&gap) {
            continue;
        }
        let lit = delivered(segment);
        let (outer, inner) = if lit {
            (GOLD, STEEL_LIGHT)
        } else {
            (SLATE, SLATE_DARK)
        };
        for layer in 0..thickness as i32 {
            let r = radius - layer as f32;
            let colour = if layer == 0 { outer } else { inner };
            block(
                draw,
                angle.cos().mul_add(r, cx),
                angle.sin().mul_add(r, cy),
                1.0,
                1.0,
                colour,
            );
        }
        // Rivets on a lit segment, the outline of a missing one dotted.
        if !lit && step % 4 == 0 {
            block(
                draw,
                angle.cos().mul_add(radius + 2.0, cx),
                angle.sin().mul_add(radius + 2.0, cy),
                2.0,
                2.0,
                AMBER,
            );
        }
    }
    // The lens: a soft disc, breathing once the gate is open.
    let lens_r = radius * 0.55;
    if delivered(6) {
        let glow = if open { breath(seconds, 4.0) } else { 0.2 };
        let core = Color::new(
            (120.0 + 80.0 * glow) as u8,
            (190.0 + 40.0 * glow) as u8,
            255,
            (110.0 + 90.0 * glow) as u8,
        );
        disc(draw, cx, cy, lens_r, core);
        disc(draw, cx, cy, lens_r * 0.45, ICE.alpha(0.7));
    } else {
        ring(draw, cx, cy, lens_r, SLATE);
    }
    // The keystone at the top.
    let key = if delivered(GATE_PARTS - 1) {
        CREAM
    } else {
        SLATE_DARK
    };
    block(draw, cx - 5.0, cy - radius - 6.0, 10.0, 8.0, key);
    block(draw, cx - 3.0, cy - radius - 8.0, 6.0, 2.0, key);
    if delivered(GATE_PARTS - 1) {
        block(draw, cx - 1.0, cy - radius - 4.0, 2.0, 3.0, GOLD);
    }
}

fn disc(draw: &mut impl RaylibDraw, cx: f32, cy: f32, radius: f32, colour: Color) {
    let r = radius.max(1.0);
    let top = (cy - r).floor() as i32;
    let bottom = (cy + r).ceil() as i32;
    for y in top..bottom {
        let dy = y as f32 + 0.5 - cy;
        let half = (r * r - dy * dy).max(0.0).sqrt();
        block(draw, cx - half, y as f32, half * 2.0, 1.0, colour);
    }
}

fn ring(draw: &mut impl RaylibDraw, cx: f32, cy: f32, radius: f32, colour: Color) {
    let steps = (radius * 6.0).max(12.0) as i32;
    for step in 0..steps {
        let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
        block(
            draw,
            angle.cos().mul_add(radius, cx),
            angle.sin().mul_add(radius, cy),
            1.0,
            1.0,
            colour,
        );
    }
}

/// The Starwright hull on its cradle: ribs only until the hull is fitted, white-gold plating
/// after; the engine's long bell glows once the engine is fitted.
fn draw_starwright_cradle(
    draw: &mut impl RaylibDraw,
    left: f32,
    floor_y: f32,
    width: f32,
    world: &WorldState,
    player_id: PlayerId,
    seconds: f32,
) {
    let career = world.career(player_id);
    let hull = career.has_starwright(StarwrightPart::Hull);
    let engine = career.has_starwright(StarwrightPart::Engine);
    let x = left + 10.0;
    let w = (width - 30.0).max(80.0);
    let h = 30.0;
    let y = floor_y - h - 44.0;
    // The cradle's arms.
    for arm in [x + w * 0.25, x + w * 0.7] {
        block(draw, arm, y + h, 4.0, 44.0 - 18.0, STEEL_DARK);
        block(draw, arm - 6.0, y + h - 2.0, 16.0, 3.0, STEEL);
    }
    if hull {
        // A long white-gold hull: highlit top, cream flanks, a sand belly, a gold keel band,
        // panel seams, a swept dorsal and ventral fin aft and a dark canopy forward, the nose
        // a pointed taper.
        let nose = 28.0;
        for row in 0..h as i32 {
            let r = row as f32;
            let off = (r - h * 0.5).abs() / (h * 0.5);
            let reach = w - nose * off;
            let colour = if row < 2 {
                BONE
            } else if r < h * 0.45 {
                CREAM
            } else if r < h - 6.0 {
                SAND
            } else if r < h - 3.0 {
                GOLD
            } else {
                AMBER
            };
            block(draw, x, y + r, reach, 1.0, colour);
        }
        // Panel seams.
        let mut seam = x + 14.0;
        while seam < x + w - nose - 4.0 {
            block(draw, seam, y + 2.0, 1.0, h - 8.0, lit_sand());
            seam += 18.0;
        }
        // A gold stripe down the flank.
        block(draw, x + 4.0, y + h * 0.42, w - nose - 6.0, 2.0, GOLD);
        // Fins aft.
        for step in 0..12 {
            let s = step as f32;
            block(
                draw,
                x + 6.0 + s,
                y - 12.0 + s,
                20.0 - s,
                1.0,
                if step < 2 { CREAM } else { GOLD },
            );
            block(draw, x + 6.0 + s, y + h + 11.0 - s, 20.0 - s, 1.0, AMBER);
        }
        // The canopy.
        block(draw, x + w - nose - 18.0, y + 5.0, 16.0, 7.0, DEEP_BLUE);
        block(draw, x + w - nose - 17.0, y + 6.0, 6.0, 1.0, SKY_LIGHT);
        block(draw, x + w - nose - 18.0, y + 12.0, 16.0, 1.0, GOLD);
    } else {
        // Ribs on the jig.
        let mut rib = x + 6.0;
        while rib < x + w - 16.0 {
            block(draw, rib, y + 2.0, 2.0, h - 4.0, STEEL_LIGHT);
            rib += 9.0;
        }
        block(draw, x + 4.0, y + 2.0, w - 20.0, 2.0, STEEL);
        block(draw, x + 4.0, y + h - 4.0, w - 20.0, 2.0, STEEL);
    }
    // The engine bell aft (left).
    let bell = if engine { GOLD } else { STEEL_DARK };
    block(draw, x - 10.0, y + 6.0, 12.0, h - 12.0, bell);
    block(
        draw,
        x - 12.0,
        y + 4.0,
        4.0,
        h - 8.0,
        if engine { CREAM } else { STEEL },
    );
    if engine {
        let glow = breath(seconds, 3.0);
        block(
            draw,
            x - 20.0,
            y + 9.0,
            8.0,
            h - 18.0,
            ORANGE.alpha(0.35 + 0.35 * glow),
        );
    }
    // A welder's steady arc over the work, until both parts are in.
    if !(hull && engine) {
        let wx = x + w * (0.45 + 0.1 * (seconds * 0.15).sin());
        block(draw, wx, y - 22.0, 2.0, 20.0, STEEL_DARK);
        block(draw, wx - 2.0, y - 3.0, 6.0, 3.0, SKY_LIGHT.alpha(0.8));
    }
}

/// A darker sand for panel seams.
const fn lit_sand() -> Color {
    Color::new(0xb0, 0x92, 0x62, 255)
}

/// An alien-run station's grown vat: a pod per alien tech, ringed once per mark.
fn draw_tech_vat(
    draw: &mut impl RaylibDraw,
    cx: f32,
    floor_y: f32,
    world: &WorldState,
    player_id: PlayerId,
    seconds: f32,
) {
    let top = floor_y - 120.0;
    // Glass walls, the liquid with a lighter surface, the grown rims top and bottom.
    block(draw, cx - 40.0, top, 80.0, 100.0, NIGHT.alpha(0.55));
    block(draw, cx - 40.0, top + 14.0, 80.0, 82.0, TEAL.alpha(0.45));
    block(draw, cx - 40.0, top + 14.0, 80.0, 2.0, MINT.alpha(0.8));
    block(draw, cx - 41.0, top, 1.0, 100.0, ICE.alpha(0.7));
    block(draw, cx + 40.0, top, 1.0, 100.0, ICE.alpha(0.7));
    block(draw, cx - 34.0, top + 4.0, 2.0, 88.0, ICE.alpha(0.35));
    block(draw, cx - 44.0, top - 6.0, 88.0, 7.0, VIOLET);
    block(draw, cx - 44.0, top - 6.0, 88.0, 2.0, LAVENDER);
    block(draw, cx - 44.0, floor_y - 24.0, 88.0, 7.0, VIOLET);
    // Slow bubbles.
    for index in 0..6 {
        let rise = (seconds * 6.0 + index as f32 * 17.0) % 80.0;
        let bx = cx - 30.0 + index as f32 * 11.0;
        block(draw, bx, floor_y - 28.0 - rise, 2.0, 2.0, ICE.alpha(0.6));
    }
    for (index, tech) in AlienTech::ALL.into_iter().enumerate() {
        let px = cx - 27.0 + index as f32 * 18.0;
        let py = top + 30.0 + (index % 2) as f32 * 18.0;
        let mark = world.alien_tech_mark(player_id, tech);
        let lit = mark > 0;
        let glow = breath(seconds + index as f32, 5.0);
        let body = if lit {
            Color::new(180, (140.0 + 60.0 * glow) as u8, 255, 255)
        } else {
            PLUM
        };
        disc(draw, px, py, 6.0, body);
        disc(
            draw,
            px - 2.0,
            py - 2.0,
            2.0,
            if lit { BONE } else { VIOLET },
        );
        for ring_index in 0..mark {
            ring(draw, px, py, 8.0 + f32::from(ring_index) * 2.5, LAVENDER);
        }
        // A stalk down to the vat's floor.
        block(
            draw,
            px - 0.5,
            py + 6.0,
            1.0,
            floor_y - 24.0 - py - 6.0,
            MINT.alpha(0.5),
        );
    }
}

/// The outfitter's paint booth: a spray arm over a turntable and the space paints' swatches.
fn draw_paint_booth(draw: &mut impl RaylibDraw, left: f32, floor_y: f32, width: f32, seconds: f32) {
    let cx = left + width * 0.45;
    block(draw, cx - 34.0, floor_y - 30.0, 68.0, 4.0, STEEL);
    block(draw, cx - 30.0, floor_y - 26.0, 60.0, 3.0, STEEL_DARK);
    // A rig's silhouette on the turntable, in the first space paint.
    let ramp = paint_ramp(fatherload_simulation::cosmetics::PaintColor::Aurora);
    block(draw, cx - 18.0, floor_y - 46.0, 36.0, 14.0, ramp.body);
    block(draw, cx - 18.0, floor_y - 46.0, 36.0, 3.0, ramp.light);
    block(draw, cx - 6.0, floor_y - 54.0, 14.0, 8.0, ramp.shade);
    let arm = cx + 22.0 * (seconds * 0.4).sin();
    block(draw, arm, floor_y - 96.0, 3.0, 36.0, STEEL_LIGHT);
    block(draw, arm - 3.0, floor_y - 60.0, 9.0, 5.0, STEEL);
    // Swatches.
    for (index, (item, _)) in OUTFITTER.iter().enumerate() {
        let x = left + 4.0 + index as f32 * 16.0;
        let y = floor_y - 120.0;
        let colour = match item {
            fatherload_simulation::cosmetics::CosmeticItem::Paint(paint) => paint_ramp(*paint).body,
            _ => SKY_LIGHT,
        };
        block(draw, x, y, 12.0, 16.0, INK);
        block(draw, x + 1.0, y + 1.0, 10.0, 14.0, colour);
    }
}

// --- The Records ----------------------------------------------------------------------------

/// The Records wall: a banner per boss of the hunt list and the completion dial.
pub fn draw_records(
    draw: &mut impl RaylibDraw,
    stage: Rectangle,
    left: f32,
    floor_y: f32,
    world: &WorldState,
    player_id: PlayerId,
    seconds: f32,
) {
    let hunt = world.hunt();
    let banner_w = 34.0;
    let gap = 8.0;
    // Below the stage's top on a short split-screen sheet.
    let top = (floor_y - 150.0).max(stage.y + 8.0);
    for (index, (quarry, beaten, _)) in hunt.iter().enumerate().take(6) {
        let x = left + index as f32 * (banner_w + gap);
        if x + banner_w > stage.x + stage.width - 4.0 {
            break;
        }
        draw_banner(draw, x, top, banner_w, *quarry, *beaten, seconds);
    }
    // The completion dial.
    let completion = world.completion(player_id);
    let per_mille = completion.per_mille();
    let cx = left + 40.0;
    let cy = floor_y - 46.0;
    let radius = 28.0;
    let steps = 180;
    for step in 0..steps {
        let t = step as f32 / steps as f32;
        let angle = t * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let lit = t * 1_000.0 < per_mille as f32;
        for layer in 0..5 {
            let r = radius - layer as f32;
            block(
                draw,
                angle.cos().mul_add(r, cx),
                angle.sin().mul_add(r, cy),
                1.0,
                1.0,
                if lit { GOLD } else { SLATE_DARK },
            );
        }
    }
    let label = format!("{}%", per_mille / 10);
    let w = ui_kit::text_width(&label, TextStyle::Title) as f32;
    ui_kit::text_shadowed(
        draw,
        &label,
        (cx - w * 0.5) as i32,
        (cy - 7.0) as i32,
        TextStyle::Title,
        CREAM,
        Some(INK),
    );
    ui_kit::text_shadowed(
        draw,
        "COMPLETE",
        (cx + radius + 10.0) as i32,
        (cy - 10.0) as i32,
        TextStyle::Bold,
        SKY_LIGHT,
        Some(INK),
    );
    // A pip per category, lit when full.
    for (index, category) in completion.categories.iter().enumerate() {
        let x = cx + radius + 10.0 + index as f32 * 7.0;
        let colour = if category.complete() {
            MINT
        } else if category.done > 0 {
            AMBER
        } else {
            SLATE
        };
        block(draw, x, cy + 4.0, 5.0, 5.0, colour);
    }
}

/// One boss's banner: a cloth with its emblem, gold and lit when beaten.
fn draw_banner(
    draw: &mut impl RaylibDraw,
    x: f32,
    top: f32,
    w: f32,
    quarry: Quarry,
    beaten: bool,
    seconds: f32,
) {
    let h = 52.0;
    let (cloth, trim) = match quarry {
        Quarry::Faction(Faction::Pirates) => (MAROON, GOLD),
        Quarry::Faction(Faction::Swarm) => (PINE, LIME),
        Quarry::Faction(Faction::Machine) => (SLATE, RED),
        Quarry::Theme(_) => (PLUM, LAVENDER),
    };
    let (cloth, trim) = if beaten {
        (cloth, trim)
    } else {
        (NIGHT, SLATE)
    };
    block(draw, x - 2.0, top - 3.0, w + 4.0, 3.0, STEEL_DARK);
    block(draw, x, top, w, h, cloth);
    block(draw, x, top, 2.0, h, trim);
    block(draw, x + w - 2.0, top, 2.0, h, trim);
    // A swallowtail.
    for step in 0..8 {
        let s = step as f32;
        block(draw, x, top + h + s, w * 0.5 - s * 2.0, 1.0, cloth);
        block(
            draw,
            x + w * 0.5 + s * 2.0,
            top + h + s,
            w * 0.5 - s * 2.0,
            1.0,
            cloth,
        );
    }
    let cx = x + w * 0.5;
    let cy = top + 22.0;
    let ink = if beaten { trim } else { SLATE_DARK };
    match quarry {
        Quarry::Faction(Faction::Pirates) => {
            // A gilded maw: a crown over a jaw of teeth.
            for tooth in 0..5 {
                block(
                    draw,
                    cx - 10.0 + tooth as f32 * 4.0,
                    cy - 9.0,
                    2.0,
                    4.0,
                    ink,
                );
            }
            block(draw, cx - 10.0, cy - 5.0, 20.0, 3.0, ink);
            block(draw, cx - 9.0, cy, 18.0, 8.0, ink);
            for tooth in 0..6 {
                block(draw, cx - 8.0 + tooth as f32 * 3.0, cy, 2.0, 3.0, cloth);
                block(
                    draw,
                    cx - 7.0 + tooth as f32 * 3.0,
                    cy + 5.0,
                    2.0,
                    3.0,
                    cloth,
                );
            }
        }
        Quarry::Faction(Faction::Swarm) => {
            // The queen: a chitin body with four wings.
            disc(draw, cx, cy, 5.0, ink);
            disc(draw, cx, cy + 8.0, 3.5, ink);
            for (dx, dy) in [(-9.0, -5.0), (5.0, -5.0), (-9.0, 2.0), (5.0, 2.0)] {
                block(draw, cx + dx, cy + dy, 4.0, 3.0, ink);
            }
        }
        Quarry::Faction(Faction::Machine) => {
            // The Arbiter's prow: a wedge with a red eye.
            for row in 0..14 {
                let half = row as f32 * 0.8;
                block(draw, cx - half, cy - 7.0 + row as f32, half * 2.0, 1.0, ink);
            }
            block(
                draw,
                cx - 1.0,
                cy + 1.0,
                2.0,
                2.0,
                if beaten { BONE } else { RED },
            );
        }
        Quarry::Theme(_) => {
            disc(draw, cx, cy, 7.0, ink);
            block(draw, cx - 4.0, cy - 2.0, 3.0, 3.0, cloth);
            block(draw, cx + 1.0, cy - 2.0, 3.0, 3.0, cloth);
        }
    }
    if beaten {
        // A slow gleam on the trim.
        let glow = breath(seconds + x * 0.01, 6.0);
        block(
            draw,
            x + 3.0,
            top + 2.0,
            w - 6.0,
            1.0,
            CREAM.alpha(0.3 + 0.4 * glow),
        );
    } else {
        // Still at large: a red mark.
        ui_kit::text_shadowed(
            draw,
            "?",
            (cx - 2.0) as i32,
            (top + h - 16.0) as i32,
            TextStyle::Bold,
            RED,
            None,
        );
    }
}
