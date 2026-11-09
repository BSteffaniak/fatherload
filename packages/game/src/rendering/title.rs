//! The title screen: a pixel-art cross-section of the frontier claim (sky, town, strata and a
//! rig drilling its shaft), the outlined logo, and the main menu as icon buttons.
//!
//! Everything is drawn on the UI canvas from palette colours and scales with the canvas, so the
//! scene fills 640x360 and the 480x270 minimum alike. Animation follows the shell clock, so
//! captures stay deterministic.

#![allow(
    clippy::cast_possible_wrap,
    reason = "hashed scatter offsets are reduced modulo small canvas sizes first"
)]

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    pixel_art::{
        palette,
        sprite::{self, SpriteDef},
        sprites::{menu_icons, rig},
    },
    prompts::{self, Hint, hint},
    screen::UiFrame,
    ui_kit::{self, ButtonState, ItemRow, PanelStyle, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    save::{latest_save_summary, saves_exist},
    shell::TitleOption,
};

/// A cheap deterministic hash for scattering stars, ore and pebbles.
const fn hash(x: i32, y: i32) -> u32 {
    let mut value = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    value ^= value >> 15;
    value = value.wrapping_mul(0xC2B2_AE3D);
    value ^ (value >> 13)
}

/// Strata from the surface down: `(share of the depth in percent, light, body, speck)`.
const STRATA: [(i32, Color, Color, Color); 6] = [
    (3, LIME, LEAF, MOSS),
    (13, CLAY, LOAM, SAND),
    (16, LOAM, SOIL, CLAY),
    (22, STONE_LIGHT, STONE, STONE_DARK),
    (24, STONE, STONE_DARK, SLATE),
    (100, RUST, MAROON, SOIL_DARK),
];

/// Ore glints scattered through the rock (colour pairs: body, glint).
const ORES: [(Color, Color); 6] = [
    (COPPER, SAND),
    (STEEL_LIGHT, BONE),
    (GOLD, CREAM),
    (LEAF, LIME),
    (SKY_LIGHT, ICE),
    (ROSE, PINK),
];

/// The icon for a title menu entry.
const fn option_icon(option: TitleOption) -> &'static SpriteDef {
    match option {
        TitleOption::Resume => &menu_icons::PLAY,
        TitleOption::NewGame => &rig::DRILL_DOWN,
        TitleOption::LocalMultiplayer => &menu_icons::SPLIT,
        TitleOption::HostOnline => &menu_icons::GLOBE,
        TitleOption::JoinOnline => &menu_icons::SIGNAL,
        TitleOption::LoadSlot => &menu_icons::FOLDER,
        TitleOption::Options => &menu_icons::GEAR,
        TitleOption::Exit => &menu_icons::DOOR,
    }
}

/// Draws the whole title screen (backdrop, logo, menu).
pub(super) fn draw_title(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let canvas = ui_kit::canvas();
    let (width, height) = (canvas.width, canvas.height);
    draw_backdrop(draw);
    let horizon = height * 3 / 8;
    let logo_bottom = draw_logo(draw, width, (horizon / 2 - 22).max(10));
    draw_menu(draw, ui, width, height, logo_bottom + 8);
}

/// The claim cross-section behind the title and online screens.
pub(super) fn draw_backdrop(draw: &mut RaylibDrawHandle<'_>) {
    let canvas = ui_kit::canvas();
    let (width, height) = (canvas.width, canvas.height);
    let seconds = ui_kit::seconds();
    let horizon = height * 3 / 8;
    draw_sky(draw, width, horizon, seconds);
    draw_ridges(draw, width, horizon);
    let shaft_x = width * 4 / 5;
    draw_strata(draw, width, height, horizon, shaft_x);
    draw_town(draw, width, horizon, shaft_x);
    draw_rig(draw, shaft_x, horizon, height, seconds);
}

fn draw_sky(draw: &mut impl RaylibDraw, width: i32, horizon: i32, seconds: f32) {
    // Dusk bands from deep blue to a warm horizon glow.
    let bands = [DEEP_BLUE, OCEAN, SKY, SKY_LIGHT];
    let band_height = (horizon / bands.len() as i32).max(1);
    for (index, color) in bands.into_iter().enumerate() {
        let y = i32::try_from(index).unwrap_or(0) * band_height;
        draw.draw_rectangle(0, y, width, band_height + 1, color);
        // Dithered seam into the next band.
        if index + 1 < bands.len() {
            let next = bands[index + 1];
            for x in (0..width).step_by(2) {
                let offset = i32::from(x % 4 == 0);
                draw.draw_rectangle(x + offset, y + band_height - 1, 1, 1, next);
            }
        }
    }
    draw.draw_rectangle(
        0,
        band_height * 4,
        width,
        horizon - band_height * 4 + 1,
        SKY_LIGHT,
    );
    // Stars in the top band, twinkling on the shell clock.
    let twinkle = (seconds * 2.0) as i32;
    for index in 0..48 {
        let roll = hash(index, 7);
        let x = (roll % width.max(1) as u32) as i32;
        let y = ((roll >> 12) % band_height.max(1) as u32) as i32;
        let bright = (roll >> 20).wrapping_add(twinkle as u32).is_multiple_of(5);
        draw.draw_rectangle(x, y, 1, 1, if bright { BONE } else { MIST });
    }
    // Drifting clouds.
    for (index, (base_y, length)) in [(horizon / 2, 26), (horizon * 2 / 3, 18), (horizon / 3, 14)]
        .into_iter()
        .enumerate()
    {
        let speed = 3.0 + index as f32 * 2.0;
        let span = width + length * 2;
        let x = ((seconds * speed) as i32 + i32::try_from(index).unwrap_or(0) * 211)
            .rem_euclid(span)
            - length;
        draw.draw_rectangle(x, base_y, length, 2, BONE);
        draw.draw_rectangle(x + 4, base_y - 2, length / 2, 2, BONE);
        draw.draw_rectangle(x + 2, base_y + 2, length - 4, 1, MIST);
    }
}

/// Far and near ridgelines above the horizon.
fn draw_ridges(draw: &mut impl RaylibDraw, width: i32, horizon: i32) {
    for (amplitude, base, frequency, color, cap) in [
        (10.0, 18, 0.021, SLATE_LIGHT, FOG),
        (6.0, 8, 0.043, SLATE, SLATE_LIGHT),
    ] {
        for x in 0..width {
            let wave = (x as f32 * frequency).sin() * amplitude
                + (x as f32 * frequency * 2.7).cos() * amplitude * 0.4;
            let top = horizon - base - wave.round() as i32;
            draw.draw_rectangle(x, top, 1, horizon - top, color);
            draw.draw_rectangle(x, top, 1, 1, cap);
        }
    }
}

fn draw_strata(draw: &mut impl RaylibDraw, width: i32, height: i32, horizon: i32, shaft_x: i32) {
    let mut y = horizon;
    let depth = height - horizon;
    for (layer, (share, light, body, speck)) in STRATA.into_iter().enumerate() {
        let bottom = (y + (depth * share / 100).max(3)).min(height);
        draw.draw_rectangle(0, y, width, bottom - y, body);
        // A wavy top edge in the lighter tone.
        for x in 0..width {
            let lift = i32::from(hash(x / 3, i32::try_from(layer).unwrap_or(0)).is_multiple_of(3));
            draw.draw_rectangle(x, y - lift, 1, 1 + lift, light);
        }
        // Pebbles.
        for index in 0..(width * (bottom - y) / 160).max(0) {
            let roll = hash(index, i32::try_from(layer).unwrap_or(0) + 11);
            let px = (roll % width.max(1) as u32) as i32;
            let py = y + 2 + ((roll >> 11) % (bottom - y - 2).max(1) as u32) as i32;
            draw.draw_rectangle(px, py, 2, 1, speck);
        }
        y = bottom;
        if y >= height {
            break;
        }
    }
    // Ore pockets in the rock below the topsoil.
    let rock_top = horizon + 20;
    for index in 0..(width / 14) {
        let roll = hash(index, 101);
        let x = (roll % width.max(1) as u32) as i32;
        let y = rock_top + ((roll >> 10) % (height - rock_top).max(1) as u32) as i32;
        let depth_band = (y - rock_top) * 6 / (height - rock_top).max(1);
        let (body, glint) = ORES[usize::try_from(depth_band.clamp(0, 5)).unwrap_or(0)];
        draw.draw_rectangle(x - 1, y - 1, 5, 4, INK);
        draw.draw_rectangle(x, y, 3, 2, body);
        draw.draw_rectangle(x, y, 1, 1, glint);
    }
    // Lava glow along the bottom edge.
    for x in 0..width {
        let lift = (hash(x / 4, 303) % 4) as i32;
        draw.draw_rectangle(x, height - 3 - lift, 1, 3 + lift, ORANGE);
        draw.draw_rectangle(x, height - 1 - lift / 2, 1, 1 + lift / 2, GOLD);
    }
    // The open shaft.
    let rig_bottom = horizon + (height - horizon) * 3 / 5 + 34;
    draw.draw_rectangle(shaft_x - 16, horizon, 32, rig_bottom - horizon, NIGHT);
    draw.draw_rectangle(shaft_x - 17, horizon, 1, rig_bottom - horizon, INK);
    draw.draw_rectangle(shaft_x + 16, horizon, 1, rig_bottom - horizon, INK);
    draw.draw_rectangle(shaft_x - 16, rig_bottom, 32, 1, INK);
    // Timber supports along the walls.
    for rung in (horizon + 10..rig_bottom - 4).step_by(14) {
        draw.draw_rectangle(shaft_x - 16, rung, 32, 2, SOIL_DARK);
        draw.draw_rectangle(shaft_x - 16, rung, 32, 1, LOAM);
        draw.draw_rectangle(shaft_x - 16, rung, 2, 6, LOAM);
        draw.draw_rectangle(shaft_x + 14, rung, 2, 6, LOAM);
    }
    // A lamp-lit side tunnel.
    let tunnel_y = horizon + (height - horizon) * 2 / 5;
    draw.draw_rectangle(shaft_x - 60, tunnel_y, 44, 12, NIGHT);
    draw.draw_rectangle(shaft_x - 60, tunnel_y - 1, 44, 1, INK);
    draw.draw_rectangle(shaft_x - 60, tunnel_y + 12, 44, 1, INK);
    draw.draw_rectangle(shaft_x - 61, tunnel_y, 1, 12, INK);
    draw.draw_rectangle(shaft_x - 40, tunnel_y + 1, 2, 2, GOLD);
    draw.draw_rectangle(
        shaft_x - 44,
        tunnel_y + 3,
        10,
        9,
        palette::with_alpha(GOLD, 40),
    );
}

/// Town buildings standing on the surface with lit windows.
fn draw_town(draw: &mut impl RaylibDraw, width: i32, horizon: i32, shaft_x: i32) {
    let buildings = [
        (width / 12, 30, 18, RED),
        (width / 12 + 34, 22, 14, LEAF),
        (width / 12 + 60, 40, 24, OCEAN),
        (width / 12 + 104, 26, 16, VIOLET),
        (width / 2 + 40, 32, 20, TEAL),
        (width / 2 + 76, 24, 13, AMBER),
    ];
    for (x, building_width, building_height, roof) in buildings {
        if x + building_width > shaft_x - 30 && x < shaft_x + 30 {
            continue;
        }
        let top = horizon - building_height;
        draw.draw_rectangle(x - 1, top - 4, building_width + 2, building_height + 4, INK);
        draw.draw_rectangle(x, top, building_width, building_height, SLATE_DARK);
        draw.draw_rectangle(x, top - 3, building_width, 3, roof);
        draw.draw_rectangle(x, top - 3, building_width, 1, BONE);
        for wx in (x + 3..x + building_width - 3).step_by(6) {
            for wy in (top + 3..horizon - 5).step_by(7) {
                let lit = !hash(wx, wy).is_multiple_of(3);
                draw.draw_rectangle(wx, wy, 3, 3, if lit { GOLD } else { NIGHT });
            }
        }
        draw.draw_rectangle(x + building_width / 2 - 2, horizon - 6, 4, 6, INK);
    }
    // Headframe over the shaft.
    let (left, right) = (shaft_x - 20, shaft_x + 19);
    let top = horizon - 34;
    for (x0, x1) in [(left, shaft_x - 1), (right, shaft_x)] {
        let steps = (top - horizon).abs();
        for step in 0..steps {
            let x = x0 + (x1 - x0) * step / steps.max(1);
            draw.draw_rectangle(x, horizon - step, 2, 1, INK);
            draw.draw_rectangle(x, horizon - step, 1, 1, RUST);
        }
    }
    draw.draw_rectangle(shaft_x - 6, top - 2, 12, 4, INK);
    draw.draw_rectangle(shaft_x - 5, top - 1, 10, 2, COPPER);
    draw.draw_rectangle(shaft_x, top + 2, 1, 32, MIST);
}

/// The rig drilling at the bottom of the shaft with sparks and dust.
fn draw_rig(
    draw: &mut RaylibDrawHandle<'_>,
    shaft_x: i32,
    horizon: i32,
    height: i32,
    seconds: f32,
) {
    let body = &rig::RIG;
    let drill = &rig::DRILL_DOWN;
    let scale = 2;
    let bob = ((seconds * 8.0) as i32) % 2;
    let rig_y = horizon + (height - horizon) * 3 / 5 + bob;
    let rig_x = shaft_x - body.width() * scale / 2;
    sprite::draw_sprite(
        draw,
        body,
        body.frame_at(seconds),
        Vector2::new(rig_x as f32, rig_y as f32),
        scale as f32,
        Color::WHITE,
    );
    let drill_x = shaft_x - drill.width() * scale / 2;
    let drill_y = rig_y + body.height() * scale - 2;
    sprite::draw_sprite(
        draw,
        drill,
        drill.frame_at(seconds),
        Vector2::new(drill_x as f32, drill_y as f32),
        scale as f32,
        Color::WHITE,
    );
    // Sparks and flying grit around the bit.
    let tick = (seconds * 12.0) as i32;
    let tip_y = drill_y + drill.height() * scale;
    for spark in 0..6 {
        let roll = hash(spark, tick);
        let dx = (roll % 15) as i32 - 7;
        let dy = ((roll >> 8) % 6) as i32;
        let color = if roll.is_multiple_of(3) { GOLD } else { CREAM };
        draw.draw_rectangle(shaft_x + dx, tip_y - dy, 1, 1, color);
    }
    for grit in 0..4 {
        let roll = hash(grit + 40, tick / 2);
        let dx = (roll % 13) as i32 - 6;
        draw.draw_rectangle(shaft_x + dx, tip_y + 1 + (roll >> 9) as i32 % 3, 2, 1, LOAM);
    }
}

/// The outlined, two-tone logo centred at the top; returns its bottom edge.
fn draw_logo(draw: &mut impl RaylibDraw, width: i32, top: i32) -> i32 {
    const TITLE: &str = "DRILLGAME";
    let canvas = ui_kit::canvas();
    let scale = if canvas.height >= 340 { 4 } else { 3 };
    let text_width = ui_kit::display_text_width(TITLE, scale);
    let x = (width - text_width) / 2;
    let glyph_height = 7 * scale;
    // Deep drop shadow, then a thick ink outline.
    for offset in 1..=scale {
        ui_kit::display_text(draw, TITLE, x + offset, top + offset + scale, scale, MAROON);
    }
    for (dx, dy) in [
        (-scale / 2 - 1, 0),
        (scale / 2 + 1, 0),
        (0, -scale / 2 - 1),
        (0, scale / 2 + 1),
        (-scale / 2 - 1, -scale / 2 - 1),
        (scale / 2 + 1, -scale / 2 - 1),
        (-scale / 2 - 1, scale / 2 + 1),
        (scale / 2 + 1, scale / 2 + 1),
    ] {
        ui_kit::display_text(draw, TITLE, x + dx, top + dy, scale, INK);
    }
    // Gold face with an amber lower half and a cream highlight row.
    ui_kit::display_text(draw, TITLE, x, top, scale, GOLD);
    ui_kit::begin_clip(Rectangle::new(
        (x - 4) as f32,
        (top + glyph_height / 2 + scale / 2) as f32,
        (text_width + 8) as f32,
        (glyph_height) as f32,
    ));
    ui_kit::display_text(draw, TITLE, x, top, scale, AMBER);
    ui_kit::end_clip();
    ui_kit::begin_clip(Rectangle::new(
        (x - 4) as f32,
        (top + scale) as f32,
        (text_width + 8) as f32,
        scale as f32,
    ));
    ui_kit::display_text(draw, TITLE, x, top, scale, CREAM);
    ui_kit::end_clip();
    let tagline = "A FRONTIER MINING RUN";
    let tagline_y = top + glyph_height + scale + 6;
    let tagline_width = ui_kit::text_width(tagline, TextStyle::Bold);
    let tag_x = (width - tagline_width) / 2;
    // Rule lines either side of the tagline.
    draw.draw_rectangle(tag_x - 34, tagline_y + 4, 28, 1, INK);
    draw.draw_rectangle(tag_x - 34, tagline_y + 3, 28, 1, CREAM);
    draw.draw_rectangle(tag_x + tagline_width + 6, tagline_y + 4, 28, 1, INK);
    draw.draw_rectangle(tag_x + tagline_width + 6, tagline_y + 3, 28, 1, CREAM);
    ui_kit::text(draw, tagline, tag_x, tagline_y, TextStyle::Bold, BONE);
    tagline_y + TextStyle::Bold.height()
}

/// The title screen's footer prompts.
const TITLE_HINTS: [Hint; 3] = [
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Confirm, "confirm"),
    hint(Prompt::Fullscreen, "fullscreen"),
];

fn draw_menu(draw: &mut impl RaylibDraw, ui: &UiFrame<'_, '_>, width: i32, height: i32, top: i32) {
    let options = TitleOption::all(saves_exist());
    let count = i32::try_from(options.len()).unwrap_or(0);
    let footer = 26;
    let row_gap = 2;
    let available = height - top - footer - 14;
    let row_height = ((available - (count - 1) * row_gap) / count.max(1)).clamp(15, 19);
    let panel_width = 196;
    let panel_height = count * row_height + (count - 1) * row_gap + 14;
    let panel_x = (width - panel_width) / 2;
    ui_kit::panel(
        draw,
        Rectangle::new(
            panel_x as f32,
            top as f32,
            panel_width as f32,
            panel_height as f32,
        ),
        PanelStyle::Accent,
    );
    for (index, option) in options.iter().enumerate() {
        let row = i32::try_from(index).unwrap_or(0);
        let y = top + 7 + row * (row_height + row_gap);
        let state = if index == ui.shell.selected_title_item {
            ButtonState::Selected
        } else {
            ButtonState::Idle
        };
        ui_kit::item_row(
            draw,
            Rectangle::new(
                (panel_x + 14) as f32,
                y as f32,
                (panel_width - 21) as f32,
                row_height as f32,
            ),
            &ItemRow {
                icon: Some(option_icon(*option)),
                label: option.label(),
                detail: "",
                price: "",
                price_color: BONE,
                coin: false,
                state,
            },
        );
    }
    // Footer: last save and controls.
    let mut footer_y = top + panel_height + 5;
    if options.contains(&TitleOption::Resume)
        && let Some(meta) = latest_save_summary()
    {
        let line = format!(
            "Last save: {}{}m deep | {} cr | {:.0} min played",
            if meta.test_mode { "TEST MODE | " } else { "" },
            meta.depth,
            meta.credits,
            (meta.play_seconds / 60.0).floor()
        );
        ui_kit::text_centred(draw, &line, width / 2, footer_y, TextStyle::Body, CREAM);
        footer_y += TextStyle::Body.line_height();
    }
    let device = prompts::menu_device();
    let bar = prompts::bar_width(&TITLE_HINTS, device);
    prompts::draw_bar(
        draw,
        width / 2 - bar / 2,
        footer_y.max(height - 26),
        &TITLE_HINTS,
        device,
        MIST,
    );
}
