//! The expedition journal page (Wave 8 story): the Fathers' logs the crew has read, chapter by
//! chapter, with the selected entry pinned under the list and the trail's lead on top.

use raylib::prelude::*;

use fatherload_simulation::expedition::{ECHOES, EchoKind, LOGS, anomaly::AnomalyKind};

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
    shell::expedition::{ExpeditionRow, chapter_lines, expedition_rows},
};

/// The colour a chapter is marked with.
fn chapter_color(numeral: &str) -> Color {
    match numeral {
        "I" => AMBER,
        "II" => COPPER,
        _ => MINT,
    }
}

/// The expedition journal page.
#[allow(clippy::too_many_lines, reason = "one builder per part of the page")]
pub(super) fn draw_expedition_journal(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let state = ui.snapshot.expedition;
    let rows = expedition_rows(state);
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let ghosts = ECHOES
        .iter()
        .take(usize::from(state.echoes))
        .filter(|echo| echo.kind == EchoKind::Ghost)
        .count();
    let mut sections = vec![Section::new(
        "The trail",
        GOLD,
        vec![
            SectionItem::stat(
                "Logs read",
                format!("{} of {}", state.found, LOGS.len()),
                CREAM,
            ),
            SectionItem::stat(
                "Echoes heard",
                format!(
                    "{} radio, {ghosts} ghost",
                    usize::from(state.echoes) - ghosts
                ),
                SKY_LIGHT,
            ),
            SectionItem::stat(
                "Anomalies",
                format!(
                    "{} rooms, {} of {} kinds",
                    state.anomalies.len(),
                    state.anomaly_kind_count(),
                    AnomalyKind::ALL.len()
                ),
                LAVENDER,
            ),
            SectionItem::text(ui.snapshot.expedition_lead.clone()),
        ],
    )];
    for (numeral, title, read, total, revelation) in chapter_lines(state) {
        let color = chapter_color(numeral);
        let mut items: Vec<SectionItem> = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let ExpeditionRow::Log(log) = *row else {
                continue;
            };
            let entry = &LOGS[log];
            if entry.descent().numeral() != numeral {
                continue;
            }
            items.push(SectionItem::item(ItemEntry {
                icon: Some(&menu_icons::BOOK),
                label: format!("{}. {}", log + 1, entry.title),
                detail: format!(
                    "Day {} | {} | {}",
                    entry.day,
                    entry.author.name(),
                    entry.stratum.name()
                ),
                value: entry.site.name().to_uppercase(),
                value_color: color,
                coin: false,
                selected: index == selected,
                disabled: false,
            }));
        }
        if read < total {
            items.push(SectionItem::stat(
                if read == 0 {
                    "Not yet found"
                } else {
                    "Still below"
                },
                format!("{} canisters", total - read),
                FOG,
            ));
        }
        if let Some(revelation) = revelation {
            items.push(SectionItem::text(revelation.to_owned()));
        }
        sections.push(Section::new(
            format!("{numeral}. {title} ({read}/{total})"),
            color,
            items,
        ));
    }
    sections.push(Section::new(
        "",
        FOG,
        vec![SectionItem::item(ItemEntry {
            icon: Some(&menu_icons::BACK),
            label: "Turn the page: the map".to_owned(),
            detail: String::new(),
            value: String::new(),
            value_color: GOLD,
            coin: false,
            selected: rows.get(selected) == Some(&ExpeditionRow::TurnPage),
            disabled: false,
        })],
    ));
    // The selected entry's words, pinned under the list (its day and author are on its row).
    let footer = match rows.get(selected) {
        Some(ExpeditionRow::Log(log)) => LOGS.get(*log).map(|entry| entry.text.to_owned()),
        _ if state.found == 0 => Some(
            "The Fathers left a cipher canister at every camp, wreck and cairn beside the \
             builders' cable. Each opens only after the one before it."
                .to_owned(),
        ),
        _ => None,
    };
    UiLayout::screen(draw).modal_with_render_plan(
        "Expedition Logs",
        "The Fathers' trail down the builders' cable.",
        &ModalContent::new(sections).footer(footer).prompts(&[
            hint(Prompt::Navigate, "read"),
            hint(Prompt::Tab, "pages"),
            hint(Prompt::Back, "close"),
        ]),
    );
}
