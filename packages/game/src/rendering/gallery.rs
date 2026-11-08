//! The art reference sheet: every palette colour, UI kit widget, icon and sprite on one canvas.
//! Captured as the `art_gallery` scene so contributors can check new art against the set.

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
        sprites::{icons, interior, rig},
    },
    ui_kit::{self, BarColors, ButtonState, ItemRow, PanelStyle, TextStyle},
};
use crate::render_snapshot::MINERALS;

/// Draws the sheet on the UI canvas (call inside a [`ui_kit::UiCanvasScope`]).
pub(super) fn draw_art_gallery(draw: &mut RaylibDrawHandle<'_>) {
    let canvas = ui_kit::canvas();
    draw.draw_rectangle_rec(canvas.rect(), NIGHT);
    ui_kit::text_centred(
        draw,
        "Art Reference",
        canvas.width / 2,
        6,
        TextStyle::Title,
        GOLD,
    );
    ui_kit::text_centred(
        draw,
        "Palette, UI kit, icons and sprites (see rendering/ART_GUIDE.md)",
        canvas.width / 2,
        26,
        TextStyle::Body,
        FOG,
    );
    draw_palette(draw, 8, 40);
    draw_widgets(draw, 8, 104);
    draw_sprites(draw, 330, 104);
}

fn draw_palette(draw: &mut RaylibDrawHandle<'_>, x: i32, y: i32) {
    const SWATCH: i32 = 11;
    const PER_ROW: usize = 11;
    const PITCH: i32 = 57;
    for (index, (name, color)) in palette::PALETTE.iter().enumerate() {
        let column = i32::try_from(index % PER_ROW).unwrap_or(0);
        let row = i32::try_from(index / PER_ROW).unwrap_or(0);
        let (sx, sy) = (x + column * PITCH, y + row * (SWATCH + 4));
        draw.draw_rectangle(sx, sy, SWATCH + 2, SWATCH + 2, INK);
        draw.draw_rectangle(sx + 1, sy + 1, SWATCH, SWATCH, *color);
        // Abbreviated so every label fits its column: "slate_dark" -> "slatedk".
        let short: String = name
            .replace("_dark", "dk")
            .replace("_light", "lt")
            .chars()
            .filter(|c| *c != '_')
            .take(7)
            .collect::<String>()
            .to_lowercase();
        ui_kit::text_shadowed(
            draw,
            &short,
            sx + SWATCH + 4,
            sy + 2,
            TextStyle::Body,
            FOG,
            None,
        );
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "a reference sheet lists every widget"
)]
fn draw_widgets(draw: &mut RaylibDrawHandle<'_>, x: i32, y: i32) {
    let styles = [
        ("Standard", PanelStyle::Standard),
        ("Accent", PanelStyle::Accent),
        ("Warning", PanelStyle::Warning),
    ];
    for (index, (label, style)) in styles.into_iter().enumerate() {
        let px = x + i32::try_from(index).unwrap_or(0) * 104;
        ui_kit::panel(draw, Rectangle::new(px as f32, y as f32, 98.0, 34.0), style);
        ui_kit::text(draw, label, px + 8, y + 7, TextStyle::Bold, BONE);
        ui_kit::text(draw, "panel", px + 8, y + 18, TextStyle::Body, FOG);
    }
    let y = y + 42;
    for (index, (label, state)) in [
        ("Idle", ButtonState::Idle),
        ("Selected", ButtonState::Selected),
        ("Disabled", ButtonState::Disabled),
    ]
    .into_iter()
    .enumerate()
    {
        let bx = x + 8 + i32::try_from(index).unwrap_or(0) * 104;
        ui_kit::button(
            draw,
            Rectangle::new(bx as f32, y as f32, 90.0, ui_kit::BUTTON_HEIGHT as f32),
            label,
            state,
        );
    }
    let y = y + 24;
    let tab_height = ui_kit::tabs(
        draw,
        Rectangle::new(x as f32, y as f32, 300.0, 40.0),
        &["Cargo", "Market", "Rig"],
        1,
    );
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, (y + tab_height - 1) as f32, 310.0, 50.0),
        PanelStyle::Accent,
    );
    let inner = y + tab_height + 6;
    ui_kit::stat_bar(
        draw,
        x + 8,
        inner,
        140,
        &icons::HULL,
        "HULL",
        72.0,
        100.0,
        BarColors::HULL,
        0.35,
    );
    ui_kit::stat_bar(
        draw,
        x + 8,
        inner + 12,
        140,
        &icons::FUEL,
        "FUEL",
        15.0,
        100.0,
        BarColors::FUEL,
        0.2,
    );
    ui_kit::stat_bar(
        draw,
        x + 8,
        inner + 24,
        140,
        &icons::CARGO,
        "CARGO",
        9.0,
        12.0,
        BarColors::CARGO,
        0.0,
    );
    ui_kit::badge(draw, x + 160, inner, &icons::FUEL, "LOW FUEL");
    ui_kit::badge(draw, x + 160, inner + 20, &icons::HEAT, "HEAT");
    let y = inner + 46;
    for (index, (state, price_color)) in [
        (ButtonState::Selected, BONE),
        (ButtonState::Idle, BONE),
        (ButtonState::Disabled, RED),
    ]
    .into_iter()
    .enumerate()
    {
        let row_y = y + i32::try_from(index).unwrap_or(0) * (ui_kit::ITEM_ROW_HEIGHT + 3);
        ui_kit::item_row(
            draw,
            Rectangle::new(
                (x + 8) as f32,
                row_y as f32,
                300.0,
                ui_kit::ITEM_ROW_HEIGHT as f32,
            ),
            &ItemRow {
                icon: Some(&rig::DRILL_DOWN),
                label: "Drill Bit",
                detail: "Tier 2 Carbide Drill",
                price: "420",
                price_color,
                coin: true,
                state,
            },
        );
    }
    let y = y + 3 * (ui_kit::ITEM_ROW_HEIGHT + 3) + 26;
    ui_kit::tooltip(
        draw,
        "Bolt: If it still rattles, upgrade the part making the noise.",
        x + 80,
        y + 6,
        170,
        ui_kit::canvas().rect(),
    );
}

fn draw_sprites(draw: &mut RaylibDrawHandle<'_>, x: i32, y: i32) {
    ui_kit::text(draw, "ICONS", x, y, TextStyle::Bold, SKY_LIGHT);
    let mut ix = x;
    let stat_icons: [&SpriteDef; 9] = [
        &icons::FUEL,
        &icons::HULL,
        &icons::CARGO,
        &icons::CREDITS,
        &icons::DEPTH,
        &icons::WARNING,
        &icons::HEAT,
        &icons::BOMB,
        &icons::SCANNER,
    ];
    for icon in stat_icons {
        ui_kit::icon(draw, icon, ix, y + 12);
        ix += 13;
    }
    let mut ix = x;
    for mineral in MINERALS {
        ui_kit::icon(draw, icons::mineral(mineral), ix, y + 26);
        ix += 13;
    }
    ui_kit::text(
        draw,
        "RIG (2x preview)",
        x,
        y + 44,
        TextStyle::Bold,
        SKY_LIGHT,
    );
    let preview = |draw: &mut RaylibDrawHandle<'_>, sprite: &SpriteDef, frame, px, py| {
        sprite::draw_sprite(
            draw,
            sprite,
            frame,
            Vector2::new(px as f32, py as f32),
            2.0,
            Color::WHITE,
        );
    };
    preview(draw, &rig::RIG, 0, x, y + 56);
    preview(draw, &rig::RIG, 1, x + 34, y + 56);
    preview(draw, &rig::RIG_DAMAGED, 0, x + 68, y + 56);
    preview(draw, &rig::DRILL_DOWN, 0, x + 102, y + 60);
    preview(draw, &rig::DRILL_SIDE, 0, x + 122, y + 60);
    preview(draw, &rig::FLAME, 0, x + 140, y + 60);
    ui_kit::text(draw, "INTERIOR", x, y + 90, TextStyle::Bold, SKY_LIGHT);
    let tiles: [&SpriteDef; 3] = [
        &interior::shop::WALL,
        &interior::shop::PLANK,
        &interior::shop::FLOOR,
    ];
    let mut tx = x;
    for tile in tiles {
        ui_kit::icon(draw, tile, tx, y + 102);
        tx += 20;
    }
    let keeper = &interior::shop::KEEPER;
    ui_kit::icon(draw, keeper, tx + 4, y + 102);
    let props: [&SpriteDef; 7] = [
        &interior::CRATE,
        &interior::BARREL,
        &interior::FUEL_PUMP,
        &interior::GEAR,
        &interior::PART_BOX,
        &interior::CAN,
        &interior::LAMP,
    ];
    let mut px = x;
    for prop in props {
        ui_kit::icon(draw, prop, px, y + 124);
        px += prop.width() + 6;
    }
    ui_kit::icon(draw, &interior::EXIT_SIGN, px, y + 124);
    let sample = "The quick brown fox digs 1234567890 m deep!";
    let lines = ui_kit::paragraph(draw, sample, x, y + 150, 280, TextStyle::Body, BONE);
    ui_kit::text(
        draw,
        "BOLD 0123456789 +-/%",
        x,
        y + 152 + lines,
        TextStyle::Bold,
        GOLD,
    );
    ui_kit::text(draw, "Title", x, y + 166 + lines, TextStyle::Title, CREAM);
}
