//! Descent II, the Old Deep, on screen (Wave 8): one small panel per seat with what the
//! stratum's rule is doing to its rig right now, and the screen effects the rules add.
//!
//! * the dose meter (Radioactive Hollow, and while it drains): a bar in green, amber and red
//!   with its warning steps, and the active mutation with its time left;
//! * the heat storm's clock (Mantle Storm Zone): the safe window, the gathering and the storm,
//!   with "dig in" while it rages;
//! * the gas tide (Fossil Oceans): low, rising, high or ebbing;
//! * the Cathedral Lift's valves (in the nave) and the Star Core fight (in its arena): phase,
//!   pylons standing and strikes left.
//!
//! [`deep_screen`] tints each seat's screen through the same effect stack the field effects
//! use (so the accessibility setting and the photosensitivity limits apply): a green grade and
//! vignette that deepen with the dose, a heat haze while a storm rages around an exposed rig.
//! Nothing flashes; levels change slowly.

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
    field::{FieldSample, ScreenEffect},
    old_deep::{self, CorePhase, DeepTile, StormPhase},
    terrain::TileKind,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "HUD code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, PanelStyle, TextStyle};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// The seat's screen effects with the Old Deep's rules added: the dose's green grade and
/// vignette, and a heat haze in a raging storm.
#[must_use]
pub fn deep_screen(
    world: &WorldState,
    player_id: PlayerId,
    mut fields: FieldSample,
) -> FieldSample {
    if !world.terrain().settings().generator.has_descents() {
        return fields;
    }
    let dose = world.old_deep().dose(player_id).fraction();
    if dose > 0.0 {
        let grade = ScreenEffect::ColourGrade.index();
        let vignette = ScreenEffect::Vignette.index();
        fields.screen[grade] = fields.screen[grade].max(dose.mul_add(0.22, 0.08));
        fields.screen[vignette] = fields.screen[vignette].max(dose * 0.35);
        let green = [0.55, 1.0, 0.35];
        for (channel, target) in fields.grade_rgb.iter_mut().zip(green) {
            *channel = (target - *channel).mul_add(dose.min(1.0) * 0.6, *channel);
        }
    }
    let Some(rig) = world.player(player_id) else {
        return fields;
    };
    let tile = rig.tile_in(world.frame());
    if world.terrain().stratum_at(tile) == Stratum::MantleStormZone {
        let millis = old_deep::world_millis(world.simulation_tick(), world.tick_rate_hz());
        let level = match old_deep::storm_phase(millis) {
            StormPhase::Calm => 0.0,
            StormPhase::Gathering => 0.12,
            StormPhase::Raging => 0.32,
        };
        if level > 0.0 {
            let wobble = ScreenEffect::Wobble.index();
            let exposure = ScreenEffect::Exposure.index();
            fields.screen[wobble] = fields.screen[wobble].max(level);
            fields.screen[exposure] = fields.screen[exposure].max(level * 0.5);
        }
    }
    fields
}

/// One line of the panel.
struct Line {
    text: String,
    color: Color,
}

/// Draws the seat's Old Deep panel at the right of its view, from `top` down.
#[allow(clippy::too_many_lines, reason = "one block per rule shown")]
pub(super) fn draw_old_deep_hud(
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
    let deep = snapshot.old_deep;
    let millis = old_deep::world_millis(
        fatherload_simulation::SimulationTick::new(snapshot.tick),
        snapshot.tick_rate.hz(),
    );
    let mut title: Option<(&str, Color)> = None;
    let mut lines: Vec<Line> = Vec::new();
    let mut bar: Option<(f32, Color)> = None;

    let dose = deep.dose(view.player_id);
    if dose.level > 0 || stratum == Stratum::RadioactiveHollow {
        title = Some(("DOSE", LIME));
        let color = match dose.step() {
            0 => LIME,
            1 => GOLD,
            _ => RED,
        };
        bar = Some((dose.fraction(), color));
        lines.push(Line {
            text: match dose.step() {
                0 => format!("{} mSv", dose.level),
                1 => format!("{} mSv - sick: fuel burn up", dose.level),
                2 => format!("{} mSv - scanner blind", dose.level),
                _ => format!("{} mSv - LETHAL, climb", dose.level),
            },
            color,
        });
    }
    let now = fatherload_simulation::SimulationTick::new(snapshot.tick);
    if let Some(active) = deep
        .mutations
        .get(&view.player_id)
        .filter(|m| m.until > now)
    {
        let seconds = active.until.get().saturating_sub(snapshot.tick)
            / u64::from(snapshot.tick_rate.hz().max(1));
        lines.push(Line {
            text: format!(
                "{} {}:{:02}",
                active.mutation.name(),
                seconds / 60,
                seconds % 60
            ),
            color: PINK,
        });
    }
    match stratum {
        Stratum::MantleStormZone => {
            let left = old_deep::storm_seconds_left(millis);
            let (text, color) = match old_deep::storm_phase(millis) {
                StormPhase::Calm => (format!("Heat storm in {left} s"), FOG),
                StormPhase::Gathering => (format!("STORM GATHERING {left} s: dig in"), GOLD),
                StormPhase::Raging => (format!("HEAT STORM {left} s: stay in rock"), ORANGE),
            };
            title.get_or_insert(("WEATHER", ORANGE));
            lines.push(Line { text, color });
        }
        Stratum::FossilOceans => {
            let rows = old_deep::gas_tide(millis);
            let high = old_deep::gas_tide_high(millis);
            let rising = (millis % old_deep::GAS_TIDE_MILLIS) < old_deep::GAS_TIDE_HIGH.0;
            let text = if high {
                "Gas tide HIGH: pockets breathe".to_owned()
            } else if rising {
                format!("Gas tide rising ({rows}/4)")
            } else {
                format!("Gas tide ebbing ({rows}/4)")
            };
            title.get_or_insert(("GAS TIDE", LEAF));
            lines.push(Line {
                text,
                color: if high { LIME } else { MOSS },
            });
        }
        _ => {}
    }
    if let Some(site) = snapshot.deep_sites.cathedral.as_ref()
        && site.bounds.contains(rig_tile)
        && !deep.cathedral.restored
    {
        title.get_or_insert(("CATHEDRAL LIFT", LAVENDER));
        lines.push(Line {
            text: format!(
                "Valves {}/3 - hold one: jets calm",
                deep.cathedral.opened.count_ones()
            ),
            color: LAVENDER,
        });
    }
    if let Some(arena) = snapshot.deep_sites.arena.as_ref()
        && (arena.inside(rig_tile)
            || matches!(deep.core.phase, CorePhase::Shielded | CorePhase::Exposed))
        && (rig_tile.y - arena.floor()).abs() < 30
    {
        let pylons = arena
            .pylons
            .iter()
            .filter(|&&pylon| {
                terrain.tile(pylon).map(|t| t.kind) == Some(TileKind::Deep(DeepTile::CorePylon))
            })
            .count();
        title = Some(("THE STAR CORE", GOLD));
        let (text, color) = match deep.core.phase {
            CorePhase::Dormant => ("Asleep".to_owned(), FOG),
            CorePhase::Shielded => (format!("Shielded: break {pylons} pylons"), MINT),
            CorePhase::Exposed => ("OPEN - drill the core!".to_owned(), GOLD),
            CorePhase::Beaten => ("Spent. The way below is open.".to_owned(), CREAM),
        };
        lines.push(Line { text, color });
        if matches!(deep.core.phase, CorePhase::Shielded | CorePhase::Exposed) {
            lines.push(Line {
                text: format!("Strikes left: {}", deep.core.strikes_left),
                color: CREAM,
            });
        }
    }
    let Some((heading, heading_color)) = title else {
        return;
    };
    let width = 200;
    let bar_height = if bar.is_some() { 7 } else { 0 };
    let height = 18 + bar_height + lines.len() as i32 * 11;
    let x = (rect.x + rect.width) as i32 - 16 - width;
    if x <= rect.x as i32 + 120 {
        return;
    }
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
    if let Some((fill, color)) = bar {
        let bar_width = width - ui_kit::PANEL_PADDING * 2;
        draw.draw_rectangle(inner, y, bar_width, 5, INK);
        draw.draw_rectangle(
            inner + 1,
            y + 1,
            ((bar_width - 2) as f32 * fill.min(1.0)) as i32,
            3,
            color,
        );
        for level in old_deep::DOSE_LEVELS {
            let tick_x = inner
                + (bar_width as f32 * f32::from(level) / f32::from(old_deep::DOSE_MAX)) as i32;
            draw.draw_rectangle(tick_x.min(inner + bar_width - 1), y, 1, 5, FOG);
        }
        y += bar_height;
    }
    for line in lines {
        let text = ui_kit::truncate(&line.text, width - 10, TextStyle::Body);
        ui_kit::text(draw, &text, inner, y, TextStyle::Body, line.color);
        y += 11;
    }
}
