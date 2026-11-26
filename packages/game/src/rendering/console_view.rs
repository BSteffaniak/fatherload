//! The test-mode command palette and debug views on screen (Wave 14 console): the palette's
//! panel along the bottom of the screen, the FPS and tick-time overlay, and the world-space
//! debug views (hit and contact boxes, field zones, the chunk and region grid).

use raylib::prelude::*;

use fatherload_simulation::{
    entity::EntityKind,
    position::SECTOR_TILES,
    terrain::{CHUNK_SIZE_TILES, TILE_SIZE},
    world::AUTHORITATIVE_PLAYER_RADIUS,
};

use super::{
    pixel_art::palette::{self, with_alpha},
    ui_kit::{self, TextStyle},
};
use crate::{
    console::MAX_SHOWN_COMPLETIONS,
    debug_views::{DebugView, DebugViews},
    render_snapshot::{RenderSnapshot, ViewSnapshot},
    shell::ShellState,
};

/// Lines of output the panel shows.
const SHOWN_LINES: usize = 10;

/// Lines of output the panel shows on touch (it sits above the phone's keyboard).
const SHOWN_LINES_TOUCH: usize = 6;

/// Draws the palette along the bottom of the screen (UI canvas; the top on touch).
#[allow(
    clippy::too_many_lines,
    reason = "the panel top to bottom: title, output, completions, the typed line"
)]
pub fn draw_console(draw: &mut RaylibDrawHandle<'_>, shell: &ShellState) {
    let console = &shell.console;
    if !console.open {
        return;
    }
    let canvas = ui_kit::canvas();
    let line_height = TextStyle::Body.line_height();
    let margin = 6;
    let width = (canvas.width - margin * 2).min(520);
    let left = (canvas.width - width) / 2;
    // Long lines wrap (continuations indented); the newest rows that fit are shown, each in
    // its line's colour: typed commands, previews and travels, replies.
    let mut wrapped: Vec<(String, Color)> = Vec::new();
    for line in &console.lines {
        let body = line.trim_start();
        let indent = i32::try_from(line.len() - body.len()).unwrap_or(0) * 3;
        let color = if line.starts_with("> ") {
            palette::MIST
        } else if body.starts_with('?') || body.starts_with("->") {
            palette::GOLD
        } else {
            palette::MINT
        };
        for (part, row) in ui_kit::wrap(body, width - 20 - indent - 12, TextStyle::Body)
            .into_iter()
            .enumerate()
        {
            let pad = if part == 0 { indent } else { indent + 12 };
            wrapped.push((format!("{}{row}", " ".repeat((pad / 3) as usize)), color));
        }
    }
    let touch = shell.prompt_device(console.seat).is_touch();
    let shown_lines = if touch {
        SHOWN_LINES_TOUCH
    } else {
        SHOWN_LINES
    };
    let shown: Vec<&(String, Color)> = wrapped
        .iter()
        .skip(wrapped.len().saturating_sub(shown_lines))
        .collect();
    let completions: Vec<&String> = console
        .completions
        .iter()
        .take(MAX_SHOWN_COMPLETIONS)
        .collect();
    let rows = i32::try_from(shown.len() + usize::from(!completions.is_empty())).unwrap_or(0);
    let height = 18 + line_height * rows + 18;
    // On touch the panel sits at the top: a phone's keyboard comes up over the bottom half of
    // the screen, and the touch menu's < > BACK buttons sit in the bottom corner.
    let top = if touch {
        margin
    } else {
        canvas.height - height - margin
    };
    ui_kit::panel(
        draw,
        Rectangle::new(left as f32, top as f32, width as f32, height as f32),
        ui_kit::PanelStyle::Accent,
    );
    let title = console.acting.map_or_else(
        || "COMMAND".to_owned(),
        |seat| format!("COMMAND: SEAT {}", seat + 1),
    );
    ui_kit::text(
        draw,
        &title,
        left + 8,
        top + 5,
        TextStyle::Bold,
        palette::GOLD,
    );
    let hint = if touch {
        "> completes  < history  Enter runs  Back closes"
    } else {
        "Tab completes  Up/Down history  Enter runs  Esc closes"
    };
    let hint_width = ui_kit::text_width(hint, TextStyle::Body);
    if hint_width + 70 < width {
        ui_kit::text(
            draw,
            hint,
            left + width - 8 - hint_width,
            top + 5,
            TextStyle::Body,
            palette::FOG,
        );
    }
    ui_kit::divider(draw, left + 6, top + 15, width - 12);
    let text_width = width - 16;
    let mut y = top + 19;
    for (line, color) in shown {
        let row = ui_kit::truncate(line, text_width, TextStyle::Body);
        ui_kit::text(draw, &row, left + 8, y, TextStyle::Body, *color);
        y += line_height;
    }
    if !completions.is_empty() {
        let words: Vec<String> = completions
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let word = line.rsplit(' ').next().unwrap_or(line);
                if console.completion_at == Some(index) {
                    format!("[{word}]")
                } else {
                    word.to_owned()
                }
            })
            .collect();
        let more = console.completions.len().saturating_sub(completions.len());
        let row = if more > 0 {
            format!("{}  +{more}", words.join("  "))
        } else {
            words.join("  ")
        };
        let row = ui_kit::truncate(&row, text_width, TextStyle::Body);
        ui_kit::text(draw, &row, left + 8, y, TextStyle::Body, palette::LAVENDER);
    }
    // The line being typed, in a well, with a blinking caret.
    let well_top = top + height - 16;
    ui_kit::well(
        draw,
        Rectangle::new(
            (left + 5) as f32,
            well_top as f32,
            (width - 10) as f32,
            12.0,
        ),
    );
    let typed = format!("> {}", console.text);
    let typed = ui_kit::truncate(&typed, text_width - 6, TextStyle::Body);
    ui_kit::text(
        draw,
        &typed,
        left + 8,
        well_top + 2,
        TextStyle::Body,
        palette::BONE,
    );
    if (ui_kit::seconds() * 2.0) as i32 % 2 == 0 {
        let caret = left + 9 + ui_kit::text_width(&typed, TextStyle::Body);
        draw.draw_rectangle(caret, well_top + 2, 1, 8, palette::GOLD);
    }
}

/// The FPS and tick-time readout (top centre), while that view is on.
pub fn draw_frame_stats(draw: &mut RaylibDrawHandle<'_>, shell: &ShellState, test_mode: bool) {
    if !shell.debug_views.shows(DebugView::FrameStats, test_mode) {
        return;
    }
    let stats = shell.frame_stats;
    let speed = if shell.time_scale == crate::debug_views::TimeScale::Normal {
        String::new()
    } else {
        format!("  x{}", shell.time_scale.word())
    };
    let text = format!(
        "{:.0} FPS  {:.2} ms ticks ({}){speed}",
        stats.fps, stats.tick_ms, stats.ticks
    );
    let canvas = ui_kit::canvas();
    let width = ui_kit::text_width(&text, TextStyle::Body) + 12;
    let left = (canvas.width - width) / 2;
    // Under the HUD's alert row.
    let top = 82;
    ui_kit::panel(
        draw,
        Rectangle::new(left as f32, top as f32, width as f32, 15.0),
        ui_kit::PanelStyle::Standard,
    );
    let color = if stats.fps < 50.0 {
        palette::AMBER
    } else {
        palette::MINT
    };
    ui_kit::text(draw, &text, left + 6, top + 4, TextStyle::Body, color);
}

/// The time scale's badge while it is not real time (top centre, under the FPS readout).
pub fn draw_time_scale(draw: &mut RaylibDrawHandle<'_>, shell: &ShellState, test_mode: bool) {
    if !test_mode || shell.time_scale == crate::debug_views::TimeScale::Normal {
        return;
    }
    let text = if shell.time_scale == crate::debug_views::TimeScale::Paused {
        "PAUSED (speed 1 resumes)".to_owned()
    } else {
        format!("SPEED x{}", shell.time_scale.word())
    };
    let canvas = ui_kit::canvas();
    let width = ui_kit::text_width(&text, TextStyle::Bold) + 12;
    let left = (canvas.width - width) / 2;
    ui_kit::panel(
        draw,
        Rectangle::new(left as f32, 99.0, width as f32, 15.0),
        ui_kit::PanelStyle::Warning,
    );
    ui_kit::text(draw, &text, left + 6, 103, TextStyle::Bold, palette::GOLD);
}

/// The world-space debug views over a seat's world (inside its 2D camera; `visible` is the
/// world rectangle on screen).
pub fn draw_world_debug(
    draw: &mut impl RaylibDraw,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    views: DebugViews,
    visible: Rectangle,
) {
    let test_mode = snapshot.world.is_test_mode();
    let views = views.effective(test_mode);
    let line = (1.0 / view.world_scale.max(0.05)).max(1.0);
    if views.is_on(DebugView::ChunkGrid) {
        draw_grid(draw, visible, line);
    }
    if views.is_on(DebugView::FieldZones) {
        // Zones are in absolute tiles; the view draws in the world's frame.
        let frame = crate::view_frame::frame();
        for (_, zone) in snapshot.world.field_zones().iter() {
            let rect = Rectangle::new(
                frame.col_px(zone.min_x),
                frame.row_px(zone.min_y),
                view_x(zone.max_x - zone.min_x + 1),
                view_x(zone.max_y - zone.min_y + 1),
            );
            draw.draw_rectangle_rec(rect, with_alpha(palette::VIOLET, 40));
            draw.draw_rectangle_lines_ex(rect, line * 2.0, palette::LAVENDER);
            let feather = f32::from(zone.feather_tiles) * TILE_SIZE;
            if feather > 0.0 {
                draw.draw_rectangle_lines_ex(
                    Rectangle::new(
                        rect.x + feather,
                        rect.y + feather,
                        (rect.width - feather * 2.0).max(0.0),
                        (rect.height - feather * 2.0).max(0.0),
                    ),
                    line,
                    with_alpha(palette::LAVENDER, 140),
                );
            }
            let name = zone
                .preset
                .map_or_else(|| "zone".to_owned(), |preset| preset.title().to_owned());
            draw.draw_text(
                &name,
                (rect.x + 3.0) as i32,
                (rect.y + 3.0) as i32,
                (10.0 * line) as i32,
                palette::BONE,
            );
        }
    }
    if views.is_on(DebugView::HitBoxes) {
        for player in &snapshot.players {
            let r = AUTHORITATIVE_PLAYER_RADIUS;
            draw.draw_rectangle_lines_ex(
                Rectangle::new(player.x - r, player.y - r, r * 2.0, r * 2.0),
                line,
                palette::LIME,
            );
            draw.draw_circle_lines(player.x as i32, player.y as i32, r, palette::LIME);
        }
        for entity in snapshot.entities.iter() {
            let Some(area) = entity.state.hit_area() else {
                if matches!(
                    entity.kind(),
                    EntityKind::Drop | EntityKind::Critter | EntityKind::Npc
                ) {
                    draw.draw_rectangle_lines_ex(
                        Rectangle::new(entity.body.x - 3.0, entity.body.y - 3.0, 6.0, 6.0),
                        line,
                        palette::SKY,
                    );
                }
                continue;
            };
            let (x, y) = (entity.body.x, entity.body.y);
            draw.draw_circle_lines(x as i32, y as i32, area.radius, palette::RED);
            if area.half.0 > 0.0 || area.half.1 > 0.0 {
                let cy = y + area.centre_dy;
                draw.draw_rectangle_lines_ex(
                    Rectangle::new(
                        x - area.half.0,
                        cy - area.half.1,
                        area.half.0 * 2.0,
                        area.half.1 * 2.0,
                    ),
                    line,
                    palette::ORANGE,
                );
            }
        }
    }
}

/// `tiles` tiles in pixels.
fn view_x(tiles: i32) -> f32 {
    tiles as f32 * TILE_SIZE
}

/// Chunk lines (thin) and region (sector) lines (thick) over the visible rectangle, labelled
/// with their chunk coordinates.
fn draw_grid(draw: &mut impl RaylibDraw, visible: Rectangle, line: f32) {
    let chunk_px = CHUNK_SIZE_TILES as f32 * TILE_SIZE;
    let frame = crate::view_frame::frame();
    let region_tiles = SECTOR_TILES;
    let first_col = (visible.x / chunk_px).floor() as i32;
    let last_col = ((visible.x + visible.width) / chunk_px).ceil() as i32;
    let first_row = (visible.y / chunk_px).floor() as i32;
    let last_row = ((visible.y + visible.height) / chunk_px).ceil() as i32;
    // Too many lines when zoomed far out: chunks give way to regions alone.
    let chunks = (last_col - first_col) < 160 && (last_row - first_row) < 160;
    for col in first_col..=last_col {
        let x = col as f32 * chunk_px;
        let absolute = i64::from(col) * i64::from(CHUNK_SIZE_TILES) + frame.origin_col();
        let region = absolute.rem_euclid(region_tiles) == 0;
        if region || chunks {
            draw.draw_line_ex(
                Vector2::new(x, visible.y),
                Vector2::new(x, visible.y + visible.height),
                if region { line * 2.5 } else { line },
                if region {
                    with_alpha(palette::GOLD, 200)
                } else {
                    with_alpha(palette::ICE, 90)
                },
            );
        }
    }
    for row in first_row..=last_row {
        let y = row as f32 * chunk_px;
        let absolute = i64::from(row) * i64::from(CHUNK_SIZE_TILES) + frame.origin_row();
        let region = absolute.rem_euclid(region_tiles) == 0;
        if region || chunks {
            draw.draw_line_ex(
                Vector2::new(visible.x, y),
                Vector2::new(visible.x + visible.width, y),
                if region { line * 2.5 } else { line },
                if region {
                    with_alpha(palette::GOLD, 200)
                } else {
                    with_alpha(palette::ICE, 90)
                },
            );
        }
    }
    // Labels (absolute chunk coordinates) only while few enough to read and cheap to draw.
    if (last_col - first_col) <= 24 && (last_row - first_row) <= 16 {
        let chunk = i64::from(CHUNK_SIZE_TILES);
        let (origin_col, origin_row) = (
            frame.origin_col().div_euclid(chunk),
            frame.origin_row().div_euclid(chunk),
        );
        for col in first_col..=last_col {
            for row in first_row..=last_row {
                draw.draw_text(
                    &format!(
                        "{},{}",
                        i64::from(col) + origin_col,
                        i64::from(row) + origin_row
                    ),
                    (col as f32 * chunk_px + 2.0) as i32,
                    (row as f32 * chunk_px + 2.0) as i32,
                    (8.0 * line) as i32,
                    with_alpha(palette::ICE, 150),
                );
            }
        }
    }
}
