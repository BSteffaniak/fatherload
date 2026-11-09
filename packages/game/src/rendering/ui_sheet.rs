//! The UI reference sheet: every menu icon and interior prop at 1x and 3x, captured as the
//! `ui_sheet` scene so new UI art can be checked pixel by pixel.

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    pixel_art::{
        sprite::{self, SpriteDef},
        sprites::{interior, menu_icons},
    },
    ui_kit::{self, TextStyle},
};

/// Every menu icon, in sheet order.
const MENU_ICONS: &[&SpriteDef] = &[
    &menu_icons::HAMMER,
    &menu_icons::CONTRACT,
    &menu_icons::RECEIPT,
    &menu_icons::SORT,
    &menu_icons::BOOK,
    &menu_icons::LETTER,
    &menu_icons::BANK,
    &menu_icons::INSURANCE,
    &menu_icons::ROCKET,
    &menu_icons::GIFT,
    &menu_icons::DRONE,
    &menu_icons::SCRAP,
    &menu_icons::COLLAPSE,
    &menu_icons::HOUSE,
    &menu_icons::FLAG,
    &menu_icons::ANVIL,
    &menu_icons::DISK,
    &menu_icons::FOLDER,
    &menu_icons::PERSON,
    &menu_icons::READY,
    &menu_icons::WAITING,
    &menu_icons::OFFLINE,
    &menu_icons::CROWN,
    &menu_icons::SIGNAL,
    &menu_icons::KEY,
    &menu_icons::SPEAKER,
    &menu_icons::SCREEN,
    &menu_icons::PLAY,
    &menu_icons::DOOR,
    &menu_icons::BACK,
    &menu_icons::GLOBE,
    &menu_icons::SPLIT,
    &menu_icons::DICE,
    &menu_icons::GEAR,
];

/// Interior props, in sheet order.
const PROPS: &[&SpriteDef] = &[
    &interior::CRATE,
    &interior::BARREL,
    &interior::FUEL_PUMP,
    &interior::GEAR,
    &interior::PART_BOX,
    &interior::CAN,
    &interior::LAMP,
];

/// Draws the sheet on the UI canvas (call inside a [`ui_kit::UiCanvasScope`]).
pub(super) fn draw_ui_sheet(draw: &mut RaylibDrawHandle<'_>) {
    let canvas = ui_kit::canvas();
    draw.draw_rectangle_rec(canvas.rect(), NIGHT);
    ui_kit::text(draw, "MENU ICONS", 8, 6, TextStyle::Bold, SKY_LIGHT);
    let per_row = 12;
    for (index, icon) in MENU_ICONS.iter().enumerate() {
        let column = i32::try_from(index % per_row).unwrap_or(0);
        let row = i32::try_from(index / per_row).unwrap_or(0);
        let (x, y) = (8 + column * 52, 20 + row * 46);
        draw.draw_rectangle(x - 1, y - 1, 29, 29, SLATE_DARK);
        sprite::draw_sprite(
            draw,
            icon,
            0,
            Vector2::new(x as f32, y as f32),
            3.0,
            Color::WHITE,
        );
        ui_kit::icon(draw, icon, x + 32, y + 9);
        let label = icon.name.trim_start_matches("menu_");
        ui_kit::text(draw, label, x, y + 30, TextStyle::Body, FOG);
    }
    let props_y = 20 + 3 * 46 + 8;
    ui_kit::text(
        draw,
        "INTERIOR PROPS (3x)",
        8,
        props_y,
        TextStyle::Bold,
        SKY_LIGHT,
    );
    let mut x = 8;
    let mut y = props_y + 14;
    for prop in PROPS {
        let width = prop.width() * 3;
        if x + width > canvas.width - 8 {
            x = 8;
            y += 60;
        }
        sprite::draw_sprite(
            draw,
            prop,
            0,
            Vector2::new(x as f32, y as f32),
            3.0,
            Color::WHITE,
        );
        x += width + 8;
    }
}
