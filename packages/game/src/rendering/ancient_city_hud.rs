//! The builders' buried city on screen (Wave 10 ancients-a): per seat,
//!
//! * a small panel with what the stratum is doing to the rig: the Megacity's alarm (and the
//!   ride: the next stop of the tram or lift it is aboard, or a piston's stroke), the Engine
//!   Halls' beat as a pulsing metronome, the Archive's star charts read;
//! * the seat's archivist's words, in a speech box at the bottom of its view while it talks.

use fatherload_simulation::{
    ancient_city::{
        ALARM_FULL, ALARM_WATCH, archivist,
        movers::{CityMover, MoverKind},
    },
    descent::Stratum,
    worldgen::descent::ancient_city as sites,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::pixel_art::sprites::ancient_city as art;
use super::ui_kit::{self, PanelStyle, TextStyle};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// One line of the panel.
struct Line {
    text: String,
    color: Color,
}

/// World milliseconds of the snapshot's tick.
fn snapshot_millis(snapshot: &RenderSnapshot<'_>) -> u64 {
    snapshot.tick.saturating_mul(1_000) / u64::from(snapshot.tick_rate.hz().max(1))
}

/// The ride's line for the panel: where the car, cab or head is going and when.
fn ride_line(snapshot: &RenderSnapshot<'_>, view: &ViewSnapshot<'_>) -> Option<Line> {
    let ride = snapshot.ancient_city.rides.get(&view.player_id)?;
    let settings = snapshot.terrain.settings();
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let mover = sites::mover_at(settings, &layout, ride.mover)?;
    let millis = snapshot_millis(snapshot);
    #[allow(clippy::cast_precision_loss, reason = "a few seconds")]
    let seconds = |ms: u64| ms as f32 / 1_000.0;
    Some(match (&mover, mover.track_at(millis)) {
        (CityMover::Tram { .. }, Some(state)) => Line {
            text: if state.dwelling.is_some() {
                format!(
                    "TRAM: at a stop, leaving in {:.0} s",
                    seconds(state.until_millis)
                )
            } else {
                format!("TRAM: next stop in {:.0} s", seconds(state.until_millis))
            },
            color: MINT,
        },
        (CityMover::Lift { .. }, Some(state)) => Line {
            text: if state.dwelling.is_some() {
                format!("LIFT: doors open, {:.0} s", seconds(state.until_millis))
            } else {
                "LIFT: between floors".to_owned()
            },
            color: GOLD,
        },
        _ => Line {
            text: match ride.mover.kind {
                MoverKind::Piston => "ON A PISTON: thrust to jump off".to_owned(),
                _ => "RIDING".to_owned(),
            },
            color: AMBER,
        },
    })
}

/// Draws the seat's buried city panel at the right of its view, from `top` down.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    reason = "a few lines of text; one block per stratum"
)]
pub(super) fn draw_city_hud(
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
    let city = snapshot.ancient_city;
    let millis = snapshot_millis(snapshot);
    let mut lines: Vec<Line> = Vec::new();
    let mut metronome: Option<u64> = None;
    let (heading, heading_color) = match stratum {
        Stratum::BuriedMegacity => {
            let alarm = city
                .alarms
                .get(&view.player_id)
                .map_or(0, |alarm| alarm.level_at(millis));
            let hunted = city
                .alarms
                .get(&view.player_id)
                .is_some_and(|alarm| alarm.hunting(millis));
            if hunted {
                lines.push(Line {
                    text: "HUNTED: the city's drones are coming".to_owned(),
                    color: RED,
                });
            } else if alarm > 0 {
                lines.push(Line {
                    text: format!("City alarm {}%", alarm * 100 / ALARM_FULL),
                    color: if alarm >= ALARM_WATCH { ORANGE } else { GOLD },
                });
            } else {
                lines.push(Line {
                    text: "The city is calm: ride, don't cut".to_owned(),
                    color: FOG,
                });
            }
            ("BURIED MEGACITY", AMBER)
        }
        Stratum::EngineHalls => {
            let along = fatherload_simulation::descent::fields::cached_layout(terrain.settings())
                .along(rig_tile.y);
            let beat = fatherload_simulation::ancient_city::engine_beat_millis(along);
            metronome = Some(beat);
            lines.push(Line {
                text: format!(
                    "The engine's beat: {} a minute",
                    fatherload_simulation::ancient_city::engine_bpm(beat)
                ),
                color: ORANGE,
            });
            ("ENGINE HALLS", ORANGE)
        }
        Stratum::ArchiveOfStars => {
            lines.push(Line {
                text: format!("Star charts read: {}", city.charts.len()),
                color: GOLD,
            });
            lines.push(Line {
                text: "Vault locks: plates, levers, light".to_owned(),
                color: FOG,
            });
            ("ARCHIVE OF STARS", SKY_LIGHT)
        }
        _ => return,
    };
    if let Some(line) = ride_line(snapshot, view) {
        lines.insert(0, line);
    }
    let width = 236;
    let height = 18 + lines.len() as i32 * 11 + if metronome.is_some() { 10 } else { 0 };
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
    for line in &lines {
        ui_kit::text(draw, &line.text, inner, y, TextStyle::Body, line.color);
        y += 11;
    }
    if let Some(beat) = metronome {
        // A metronome bar: a light sweeping across on the beat (steady, never a flash).
        let phase = (millis % beat.max(1)) as f32 / beat.max(1) as f32;
        let bar = width - ui_kit::PANEL_PADDING * 2;
        draw.draw_rectangle(inner, y + 2, bar, 3, with_alpha(NIGHT, 220));
        let pos = (phase * (bar - 8) as f32) as i32;
        draw.draw_rectangle(inner + pos, y + 1, 8, 5, ORANGE);
    }
}

/// Draws the seat's archivist's words while it talks: a speech box at the bottom of its view
/// with its portrait.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "screen layout in whole pixels"
)]
pub(super) fn draw_archivist_line(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
) {
    let Some(drone) = snapshot.ancient_city.archivists.get(&view.player_id) else {
        return;
    };
    let Some((id, tick)) = drone.line else {
        return;
    };
    let millis = snapshot_millis(snapshot);
    let said = tick.get().saturating_mul(1_000) / u64::from(snapshot.tick_rate.hz().max(1));
    if millis.saturating_sub(said) >= archivist::LINE_SHOW_MILLIS {
        return;
    }
    let Some(text) = archivist::line_text(id) else {
        return;
    };
    let width = (rect.width as i32 - 40).min(520);
    if width < 200 {
        return;
    }
    let lines = ui_kit::wrap(text, width - 70, TextStyle::Body);
    let height = 22 + lines.len() as i32 * 11;
    let x = rect.x as i32 + (rect.width as i32 - width) / 2;
    let y = (rect.y + rect.height) as i32 - height - 70;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Accent,
    );
    super::pixel_art::sprite::draw_sprite(
        draw,
        &art::ARCHIVIST,
        2,
        Vector2::new(x as f32 + 10.0, y as f32 + 8.0),
        4.0,
        Color::WHITE,
    );
    ui_kit::text(
        draw,
        archivist::ARCHIVIST_NAME,
        x + 62,
        y + 5,
        TextStyle::Bold,
        MINT,
    );
    let mut line_y = y + 18;
    for line in lines {
        ui_kit::text(draw, &line, x + 62, line_y, TextStyle::Body, CREAM);
        line_y += 11;
    }
}

/// The star charts page (Wave 10): the charts read as rows (touch, keys, pad and mouse select
/// them) and, where the view has room beside the dialog, a picture of the charted sky: home at
/// its heart, every charted star at its sector in its colour, the selected one ringed with a
/// line of light to it.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "screen layout in whole pixels"
)]
pub(super) fn draw_star_charts(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &super::screen::UiFrame<'_, '_>,
) {
    use super::layout::{ItemEntry, ModalContent, Section, SectionItem, UiLayout};
    use crate::{
        input::prompts::Prompt,
        shell::star_charts::{StarChartRow, chart_words, star_chart_rows},
    };
    let charts = &ui.snapshot.ancient_city.charts;
    let rows = star_chart_rows(charts);
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let mut items: Vec<SectionItem> = Vec::new();
    if charts.is_empty() {
        items.push(SectionItem::text(
            "No stars charted yet. The Archive of Stars keeps them on plinths behind its vault \
             locks, and three in the Orrery.",
        ));
    }
    for (index, chart) in charts.iter().enumerate() {
        let [r, g, b] = chart.class.colour();
        items.push(SectionItem::item(ItemEntry {
            icon: Some(&super::pixel_art::sprites::menu_icons::SIGNAL),
            label: chart.name.clone(),
            detail: format!("{}, {} sectors", chart.class.name(), chart.distance()),
            value: format!("{} W", chart.worlds),
            value_color: Color::new(r, g, b, 255),
            coin: false,
            selected: rows.get(selected) == Some(&StarChartRow::Chart(index)),
            disabled: false,
        }));
    }
    let sections = vec![
        Section::new(format!("Charted stars ({})", charts.len()), GOLD, items),
        Section::new(
            "",
            FOG,
            vec![SectionItem::item(ItemEntry {
                icon: Some(&super::pixel_art::sprites::menu_icons::BACK),
                label: "Turn the page: the map".to_owned(),
                detail: String::new(),
                value: String::new(),
                value_color: GOLD,
                coin: false,
                selected: rows.get(selected) == Some(&StarChartRow::TurnPage),
                disabled: false,
            })],
        ),
    ];
    let footer = match rows.get(selected) {
        Some(StarChartRow::Chart(index)) => charts.get(*index).map(chart_words),
        _ => Some(
            "The first coordinates of the sky beyond the Moon: the builders came down from \
             these stars."
                .to_owned(),
        ),
    };
    UiLayout::screen(draw).modal_with_render_plan(
        "Star Charts",
        "The sky the builders came from, a star at a time.",
        &ModalContent::new(sections)
            .footer(footer)
            .compact()
            .prompts(&[
                super::prompts::hint(Prompt::Navigate, "read"),
                super::prompts::hint(Prompt::Tab, "pages"),
                super::prompts::hint(Prompt::Back, "close"),
            ]),
    );
    // The picture of the sky beside the dialog, where there is room.
    let view = super::screen::view_rect(ui.view);
    let room = (view.width - 300.0) * 0.5 - 24.0;
    if room < 150.0 {
        return;
    }
    let size = room.min(view.height - 120.0).min(300.0);
    if size < 150.0 {
        return;
    }
    let x = view.x + 12.0;
    let y = view.y + (view.height - size) * 0.5;
    let picked = match rows.get(selected) {
        Some(StarChartRow::Chart(index)) => Some(*index),
        _ => None,
    };
    draw_sky(draw, Rectangle::new(x, y, size, size), charts, picked);
}

/// The charted sky in `rect` (see [`draw_star_charts`]).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "screen layout in whole pixels"
)]
fn draw_sky(
    draw: &mut RaylibDrawHandle<'_>,
    rect: Rectangle,
    charts: &[fatherload_simulation::star_chart::StarChart],
    picked: Option<usize>,
) {
    use fatherload_simulation::star_chart::CHART_RING;
    ui_kit::panel(draw, rect, PanelStyle::Standard);
    let inner = Rectangle::new(
        rect.x + 6.0,
        rect.y + 6.0,
        rect.width - 12.0,
        rect.height - 12.0,
    );
    draw.draw_rectangle_rec(inner, NIGHT);
    // Far stars, fixed (no twinkle: the page is calm).
    for index in 0..90_u32 {
        let h = index.wrapping_mul(0x9E37_79B9) ^ 0x5A5A;
        let sx = inner.x + (h % 997) as f32 / 997.0 * inner.width;
        let sy = inner.y + ((h >> 10) % 991) as f32 / 991.0 * inner.height;
        let colour = if h.is_multiple_of(7) {
            SLATE_LIGHT
        } else {
            SLATE
        };
        draw.draw_rectangle(sx as i32, sy as i32, 1, 1, colour);
    }
    let (cx, cy) = (inner.x + inner.width * 0.5, inner.y + inner.height * 0.5);
    let scale = (inner.width.min(inner.height) * 0.5 - 10.0) / CHART_RING.1 as f32;
    // Rings every dozen sectors.
    for ring in [12, 24, 36, 48] {
        draw.draw_circle_lines(
            cx as i32,
            cy as i32,
            ring as f32 * scale,
            with_alpha(SLATE_DARK, 200),
        );
    }
    // Home: the sun the planet circles.
    draw.draw_circle(cx as i32, cy as i32, 4.0, GOLD);
    draw.draw_circle(cx as i32, cy as i32, 2.0, CREAM);
    ui_kit::text(
        draw,
        "HOME",
        cx as i32 + 6,
        cy as i32 - 4,
        TextStyle::Body,
        GOLD,
    );
    for (index, chart) in charts.iter().enumerate() {
        let px = (chart.sector.0 as f32).mul_add(scale, cx);
        let py = (-chart.sector.1 as f32).mul_add(scale, cy);
        let [r, g, b] = chart.class.colour();
        let colour = Color::new(r, g, b, 255);
        if picked == Some(index) {
            draw.draw_line_ex(
                Vector2::new(cx, cy),
                Vector2::new(px, py),
                1.0,
                with_alpha(GOLD, 150),
            );
            draw.draw_circle_lines(px as i32, py as i32, 7.0, CREAM);
            ui_kit::text(
                draw,
                &chart.name,
                px as i32 + 9,
                py as i32 - 4,
                TextStyle::Body,
                CREAM,
            );
        }
        draw.draw_circle(
            px as i32,
            py as i32,
            2.0 + f32::from(chart.worlds.min(4)) * 0.5,
            colour,
        );
    }
    ui_kit::text(
        draw,
        "THE CHARTED SKY",
        inner.x as i32 + 4,
        inner.y as i32 + 3,
        TextStyle::Bold,
        SKY_LIGHT,
    );
}
