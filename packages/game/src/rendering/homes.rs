//! The home screens (WP-HOME): the land office, surveying, the site board, contracts, the
//! deed and the home's own menus. The shell builds each screen's rows
//! ([`crate::shell::homes::HomeView`]); this draws them as one dialog, so what is listed is
//! exactly what confirm does.

#![allow(
    clippy::cast_precision_loss,
    reason = "progress bars scale small integers"
)]

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    pixel_art::sprites::menu_icons,
    prompts::hint,
    screen::UiFrame,
};
use crate::{
    input::prompts::Prompt,
    shell::homes::{HomeAction, HomeScreen},
};

/// Draws the seat's open home screen.
#[allow(clippy::too_many_lines, reason = "the dialog's sections in order")]
pub(super) fn draw_home_modal(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let view = &ui.seat.home.view;
    let selected = ui.seat.selected_menu_item;
    let mut sections = Vec::new();
    if !view.status.is_empty() {
        let items = view
            .status
            .iter()
            .map(|status| {
                status.meter.map_or_else(
                    || SectionItem::stat(status.label.clone(), status.value.clone(), GOLD),
                    |share| {
                        SectionItem::meter(
                            format!("{}: {}", status.label, status.value),
                            share.clamp(0.0, 1.0) * 100.0,
                            100.0,
                            LEAF,
                            LEAF,
                        )
                    },
                )
            })
            .collect();
        sections.push(Section::new("Status", SKY_LIGHT, items));
    }
    let mut footer = None;
    let rows: Vec<SectionItem> = view
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            if index == selected {
                footer = row
                    .refusal
                    .clone()
                    .or_else(|| (!row.detail.is_empty()).then(|| row.detail.clone()));
            }
            let icon = match row.action {
                HomeAction::Command(_) | HomeAction::Sign => &menu_icons::CONTRACT,
                HomeAction::Open(HomeScreen::Catalogue | HomeScreen::DecorPick) => {
                    &menu_icons::BOOK
                }
                HomeAction::Open(HomeScreen::Contract(_)) => &menu_icons::HAMMER,
                HomeAction::Contractor(_) => &menu_icons::PERSON,
                HomeAction::ToggleSupply => &menu_icons::ANVIL,
                HomeAction::GoInside | HomeAction::Leave | HomeAction::Open(_) => {
                    &menu_icons::HOUSE
                }
                HomeAction::Info => &menu_icons::LETTER,
            };
            let label = if row.chosen {
                format!("> {}", row.label)
            } else {
                row.label.clone()
            };
            SectionItem::item(ItemEntry {
                icon: Some(icon),
                label,
                detail: row.detail.clone(),
                value: row.value.clone(),
                value_color: if !row.enabled {
                    FOG
                } else if row.chosen {
                    LIME
                } else {
                    GOLD
                },
                coin: false,
                selected: index == selected,
                disabled: !row.enabled,
            })
        })
        .collect();
    if !rows.is_empty() {
        sections.push(Section::new(view.rows_title.clone(), GOLD, rows));
    }
    if let Some(quote) = &view.quote {
        sections.push(Section::new(
            "",
            BONE,
            vec![SectionItem::text(quote.clone())],
        ));
    }
    let contract = matches!(ui.seat.home.screen, Some(HomeScreen::Contract(_)));
    let hints = if contract {
        vec![
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Adjust, "supply blocks"),
            hint(Prompt::Confirm, ""),
            hint(Prompt::Back, "back"),
        ]
    } else {
        vec![
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Confirm, ""),
            hint(Prompt::Back, "back"),
        ]
    };
    UiLayout::screen(draw).modal_with_render_plan(
        &view.title,
        &view.subtitle,
        &ModalContent::new(sections).footer(footer).prompts(&hints),
    );
}
