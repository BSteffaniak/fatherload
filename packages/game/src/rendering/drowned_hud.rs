//! Descent IV, the Drowned and Frozen, on screen (Wave 9): one small panel per seat with what
//! the stratum is doing to its rig right now, and the screen effects its rules add.
//!
//! * the **ballast** gauge while the rig is a submarine (trim up, neutral, down) and the
//!   current pushing it;
//! * the **hull frost** meter (Brine Glaciers, and while it thaws): blue to white, with "find a
//!   thermal vent" when it bites;
//! * the **pressure** meter (Pearl Abyss) against the seals' rating: green, amber past three
//!   quarters, red past the seals;
//! * the **Leviathan**'s health, phase and what it is doing, while it fights near.
//!
//! [`drowned_screen`] adds the abyss's look through the field-effect stack (so the
//! accessibility setting and the photosensitivity limits apply): the deeper the pressure, the
//! darker and greyer the picture and the tighter the vignette; and a cold blue grade as frost
//! rimes the hull. Levels change slowly; nothing flashes.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "UI layout mixes small integer art pixels and float rectangles"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    descent::Stratum,
    drowned::{FROST_MAX, LeviathanMove, LeviathanPhase},
    enemy::EnemyKind,
    entity::EntityState,
    field::{FieldSample, ScreenEffect},
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "HUD code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, PanelStyle, TextStyle};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// The seat's screen effects with the Drowned and Frozen added: the abyss darkens and greys
/// with its pressure, frost gives a cold grade.
#[must_use]
pub fn drowned_screen(
    world: &WorldState,
    player_id: PlayerId,
    mut fields: FieldSample,
) -> FieldSample {
    if !world.terrain().settings().generator.has_descents() {
        return fields;
    }
    if let Some((pressure, _)) = world.abyss_pressure_for(player_id) {
        // From the trench's lip (about 280 per mille) to its crushing floor.
        let deep = ((f32::from(pressure) - 250.0) / 750.0).clamp(0.0, 1.0);
        let desaturate = ScreenEffect::Desaturate.index();
        let vignette = ScreenEffect::Vignette.index();
        fields.screen[desaturate] = fields.screen[desaturate].max(deep.mul_add(0.65, 0.1));
        fields.screen[vignette] = fields.screen[vignette].max(deep.mul_add(0.55, 0.1));
        fields.light *= deep.mul_add(-0.55, 1.0);
    }
    let frost = f32::from(world.rig_water(player_id).frost_permille()) / f32::from(FROST_MAX);
    if frost > 0.0 {
        let grade = ScreenEffect::ColourGrade.index();
        let vignette = ScreenEffect::Vignette.index();
        fields.screen[grade] = fields.screen[grade].max(frost * 0.35);
        fields.screen[vignette] = fields.screen[vignette].max(frost * 0.25);
        let cold = [0.75, 0.9, 1.0];
        for (channel, target) in fields.grade_rgb.iter_mut().zip(cold) {
            *channel = (target - *channel).mul_add(frost * 0.7, *channel);
        }
    }
    fields
}

/// One line of the panel.
struct Line {
    text: String,
    color: Color,
}

/// A gauge on the panel: a fill (0..1, or -1..1 centred for the ballast) and its colour.
enum Gauge {
    Fill(f32, Color),
    Centred(f32, Color),
}

/// Draws the seat's Drowned and Frozen panel at the right of its view, from `top` down.
#[allow(clippy::too_many_lines, reason = "one block per rule shown")]
pub(super) fn draw_drowned_hud(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    top: i32,
) {
    let terrain = snapshot.terrain;
    if !terrain.settings().generator.has_descents() || view.pilot.is_some() {
        return;
    }
    let frame = crate::view_frame::frame();
    let rig_tile = frame.tile_at(view.player.x, view.player.y);
    let stratum = terrain.stratum_at(rig_tile);
    let Some(rig) = snapshot
        .players
        .iter()
        .find(|player| player.player_id == view.player_id)
    else {
        return;
    };
    let water = rig.water;
    let mut title: Option<(&str, Color)> = None;
    let mut lines: Vec<Line> = Vec::new();
    let mut gauges: Vec<Gauge> = Vec::new();

    // The Leviathan's fight takes the panel while it is near.
    let leviathan = snapshot
        .enemies
        .iter()
        .find_map(|entity| match &entity.state {
            EntityState::Enemy(state) if state.kind == EnemyKind::Leviathan => {
                Some((entity, *state))
            }
            _ => None,
        });
    if let Some((entity, state)) = leviathan
        && (entity.body.x - view.player.x).hypot(entity.body.y - view.player.y)
            < 50.0 * fatherload_simulation::terrain::TILE_SIZE
    {
        let lev = &snapshot.drowned.leviathan;
        title = Some(("THE LEVIATHAN", GOLD));
        gauges.push(Gauge::Fill(state.health_fraction(), RED));
        let phase = match lev.phase {
            LeviathanPhase::Circling => ("Circling", SKY_LIGHT),
            LeviathanPhase::Maelstrom => ("The Maelstrom", MINT),
            LeviathanPhase::Thrashing => ("Thrashing", ORANGE),
        };
        lines.push(Line {
            text: phase.0.to_owned(),
            color: phase.1,
        });
        let (text, color) = match lev.action {
            LeviathanMove::ChargeTell { .. } => ("IT ROARS - get out of its line!", RED),
            LeviathanMove::Charge { .. } => ("Charging!", ORANGE),
            LeviathanMove::Maw | LeviathanMove::Spit => ("Its maw is open: strike it!", GOLD),
            LeviathanMove::Sink { .. } => ("It sinks to the floor...", MINT),
            LeviathanMove::Circle | LeviathanMove::Rise => ("Keep your distance", FOG),
        };
        lines.push(Line {
            text: text.to_owned(),
            color,
        });
    }
    if title.is_none() {
        if let Some((pressure, capacity)) = rig.pressure {
            title = Some(("PRESSURE", LAVENDER));
            let fill = f32::from(pressure) / 1_000.0;
            let color = if pressure > capacity {
                RED
            } else if u32::from(pressure) * 4 > u32::from(capacity) * 3 {
                GOLD
            } else {
                LEAF
            };
            gauges.push(Gauge::Fill(fill, color));
            lines.push(Line {
                text: if pressure > capacity {
                    format!("{pressure} / {capacity}: SEALS FAILING, rise!")
                } else {
                    format!("{pressure} / {capacity} the seals hold")
                },
                color,
            });
        }
        let frost = water.frost_permille();
        if frost > 0 || stratum == Stratum::BrineGlaciers {
            title.get_or_insert(("HULL FROST", ICE));
            let fill = f32::from(frost) / f32::from(FROST_MAX);
            gauges.push(Gauge::Fill(fill, if fill > 0.6 { BONE } else { SKY_LIGHT }));
            lines.push(Line {
                text: if frost >= 600 {
                    format!("Frost {}%: find a thermal vent", frost / 10)
                } else if frost > 0 {
                    format!("Frost {}%: the rig slows", frost / 10)
                } else {
                    "No frost. Vents are warm.".to_owned()
                },
                color: if frost >= 600 { BONE } else { SKY_LIGHT },
            });
        }
        if rig.submerged {
            title.get_or_insert(("SUBMARINE", MINT));
            let trim = f32::from(water.ballast) / 1_000.0;
            gauges.push(Gauge::Centred(trim, MINT));
            lines.push(Line {
                text: if water.ballast > 150 {
                    "Ballast blown: rising".to_owned()
                } else if water.ballast < -150 {
                    "Ballast flooded: sinking".to_owned()
                } else {
                    "Trimmed: hovering".to_owned()
                },
                color: MINT,
            });
        }
    }
    let Some((heading, heading_color)) = title else {
        return;
    };
    let width = 200;
    let height = 18 + gauges.len() as i32 * 7 + lines.len() as i32 * 11;
    let x = (rect.x + rect.width) as i32 - 16 - width;
    if x <= rect.x as i32 + 120 {
        return;
    }
    // Below the Old Deep's panel's place (they never show at once: different Descents).
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, top as f32, width as f32, height as f32),
        PanelStyle::Standard,
    );
    let inner = x + ui_kit::PANEL_PADDING;
    ui_kit::text(
        draw,
        heading,
        inner,
        top + 5,
        TextStyle::Bold,
        heading_color,
    );
    let mut y = top + 17;
    let bar_width = width - ui_kit::PANEL_PADDING * 2;
    for gauge in gauges {
        draw.draw_rectangle(inner, y, bar_width, 5, INK);
        match gauge {
            Gauge::Fill(fill, color) => {
                draw.draw_rectangle(
                    inner + 1,
                    y + 1,
                    ((bar_width - 2) as f32 * fill.clamp(0.0, 1.0)) as i32,
                    3,
                    color,
                );
            }
            Gauge::Centred(trim, color) => {
                let mid = inner + bar_width / 2;
                let reach = ((bar_width / 2 - 1) as f32 * trim.abs().min(1.0)) as i32;
                let left = if trim < 0.0 { mid - reach } else { mid };
                draw.draw_rectangle(left, y + 1, reach.max(1), 3, color);
                draw.draw_rectangle(mid, y, 1, 5, FOG);
            }
        }
        y += 7;
    }
    for line in lines {
        let text = ui_kit::truncate(&line.text, width - 10, TextStyle::Body);
        ui_kit::text(draw, &text, inner, y, TextStyle::Body, line.color);
        y += 11;
    }
}
