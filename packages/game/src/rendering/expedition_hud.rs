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
    shell::expedition::{
        CodexRow, ExpeditionRow, ProgressRow, chapter_lines, codex_entries_of, codex_rows,
        expedition_rows, progress_rows,
    },
};

/// The colour a chapter is marked with.
fn chapter_color(numeral: &str) -> Color {
    match numeral {
        "I" => AMBER,
        "II" => COPPER,
        "III" => MINT,
        "IV" => SKY_LIGHT,
        "V" => GOLD,
        "VII" => SAND,
        "VIII" => CREAM,
        _ => LAVENDER,
    }
}

/// The expedition journal page.
#[allow(clippy::too_many_lines, reason = "one builder per part of the page")]
pub(super) fn draw_expedition_journal(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let state = ui.snapshot.expedition;
    let rows = expedition_rows(state);
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let heard = |kind: EchoKind| {
        ECHOES
            .iter()
            .take(usize::from(state.echoes))
            .filter(|echo| echo.kind == kind)
            .count()
    };
    let (radio, ghosts, builders) = (
        heard(EchoKind::Radio),
        heard(EchoKind::Ghost),
        heard(EchoKind::Builder),
    );
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
                if builders > 0 {
                    format!("{radio} radio, {ghosts} ghost, {builders} builder")
                } else {
                    format!("{radio} radio, {ghosts} ghost")
                },
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

/// The expedition codex page (Wave 9): the hired hand's diary and the builders' tablets and
/// room inscriptions, each page with its found entries as rows and a count of the rest.
pub(super) fn draw_expedition_codex(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    use fatherload_simulation::expedition::codex::CodexPage;
    let snapshot = ui.snapshot;
    let entries = codex_entries_of(snapshot.crust, snapshot.old_deep, snapshot.expedition);
    let rows = codex_rows(&entries);
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let mut sections = Vec::new();
    for page in CodexPage::ALL {
        let color = match page {
            CodexPage::Diary => SAND,
            CodexPage::Tablets => TEAL,
        };
        let mut items: Vec<SectionItem> = vec![SectionItem::text(page.subtitle().to_owned())];
        let mut missing = 0;
        for (index, entry) in entries.iter().enumerate() {
            if entry.page != page {
                continue;
            }
            if !entry.found {
                missing += 1;
                continue;
            }
            let row = rows.iter().position(|row| *row == CodexRow::Entry(index));
            items.push(SectionItem::item(ItemEntry {
                icon: Some(&menu_icons::BOOK),
                label: entry.title.to_owned(),
                detail: entry.place.to_owned(),
                value: match page {
                    CodexPage::Diary => "PAGE".to_owned(),
                    CodexPage::Tablets => "GLYPH".to_owned(),
                },
                value_color: color,
                coin: false,
                selected: row == Some(selected),
                disabled: false,
            }));
        }
        if missing > 0 {
            items.push(SectionItem::stat(
                "Still to find",
                format!(
                    "{missing} {}",
                    match page {
                        CodexPage::Diary => "pages",
                        CodexPage::Tablets => "inscriptions",
                    }
                ),
                FOG,
            ));
        }
        let found = entries
            .iter()
            .filter(|entry| entry.page == page && entry.found)
            .count();
        let total = entries.iter().filter(|entry| entry.page == page).count();
        sections.push(Section::new(
            format!("{} ({found}/{total})", page.title()),
            color,
            items,
        ));
    }
    sections.push(Section::new(
        "",
        FOG,
        vec![SectionItem::item(ItemEntry {
            icon: Some(&menu_icons::BACK),
            label: "Turn the page: Descent progress".to_owned(),
            detail: String::new(),
            value: String::new(),
            value_color: GOLD,
            coin: false,
            selected: rows.get(selected) == Some(&CodexRow::TurnPage),
            disabled: false,
        })],
    ));
    let footer = match rows.get(selected) {
        Some(CodexRow::Entry(index)) => entries.get(*index).map(|entry| entry.text.to_owned()),
        _ if rows.len() == 1 => Some(
            "Strongboxes in the Crust, tablet walls in the Old Deep and the glyphs over the \
             deeper anomaly rooms all keep words worth copying down."
                .to_owned(),
        ),
        _ => None,
    };
    UiLayout::screen(draw).modal_with_render_plan(
        "Codex",
        "The other voices of the deep: a hired hand and the builders.",
        &ModalContent::new(sections).footer(footer).prompts(&[
            hint(Prompt::Navigate, "read"),
            hint(Prompt::Tab, "pages"),
            hint(Prompt::Back, "close"),
        ]),
    );
}

/// The Descent progress page (Wave 10): the totals, a row per Descent (reached or not, its gate
/// won or not, its logs found) and the selected Descent's gate and chapter pinned under the
/// list. Rows by keys, pad, mouse and touch like the journal's.
#[allow(clippy::too_many_lines, reason = "one builder per part of the page")]
pub(super) fn draw_descent_progress(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    use fatherload_simulation::expedition::progress::{descent_rows, totals};
    let state = ui.snapshot.expedition;
    let rows = progress_rows(state);
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let descents = descent_rows(state);
    let (reached, beaten, read, logs) = totals(state);
    let mut sections = vec![Section::new(
        "The Descent so far",
        GOLD,
        vec![
            SectionItem::stat(
                "Descents reached",
                format!("{reached} of {}", descents.len()),
                CREAM,
            ),
            SectionItem::stat(
                "Gates won",
                format!("{beaten} of {}", descents.len()),
                ORANGE,
            ),
            SectionItem::stat("Logs found", format!("{read} of {logs}"), AMBER),
            SectionItem::stat(
                "The Heart",
                if state.cable_lit {
                    "awake: the cable is lit".to_owned()
                } else {
                    "asleep".to_owned()
                },
                if state.cable_lit { SKY_LIGHT } else { FOG },
            ),
        ],
    )];
    let mut items = Vec::new();
    for (index, row) in descents.iter().enumerate() {
        let numeral = row.descent.numeral();
        let color = chapter_color(numeral);
        let (label, detail) = if row.reached {
            (
                format!("{numeral}. {}", row.descent.name()),
                format!(
                    "Gate: {}, {}",
                    row.boss.name(),
                    if row.boss_beaten { "won" } else { "not yet" }
                ),
            )
        } else {
            (
                format!("{numeral}. ???"),
                "Not reached yet: dig deeper.".to_owned(),
            )
        };
        items.push(SectionItem::item(ItemEntry {
            icon: Some(if row.boss_beaten {
                &menu_icons::MEDAL_GOLD
            } else if row.reached {
                &menu_icons::FLAG
            } else {
                &menu_icons::MEDAL_LOCKED
            }),
            label,
            detail,
            value: format!("{}/{} logs", row.logs_read, row.logs_total),
            value_color: if row.chapter.is_some() { color } else { FOG },
            coin: false,
            selected: rows.get(selected) == Some(&ProgressRow::Descent(index)),
            disabled: !row.reached,
        }));
    }
    sections.push(Section::new("The Descents", SAND, items));
    sections.push(Section::new(
        "",
        FOG,
        vec![SectionItem::item(ItemEntry {
            icon: Some(&menu_icons::BACK),
            // The star charts come next on Deep Descent worlds (Wave 10 integration).
            label: if ui.snapshot.terrain.settings().generator.has_descents() {
                "Turn the page: the star charts"
            } else {
                "Turn the page: the map"
            }
            .to_owned(),
            detail: String::new(),
            value: String::new(),
            value_color: GOLD,
            coin: false,
            selected: rows.get(selected) == Some(&ProgressRow::TurnPage),
            disabled: false,
        })],
    ));
    let footer = match rows.get(selected) {
        Some(ProgressRow::Descent(index)) => descents.get(*index).map(|row| {
            if !row.reached {
                "Somewhere below. The Fathers' trail goes on down the cable.".to_owned()
            } else if state.cable_lit
                && row.descent == fatherload_simulation::descent::Descent::Heart
            {
                "The Heart is awake and the cable is lit to the Moon. Choose this row to read \
                 the ending again."
                    .to_owned()
            } else if let Some(chapter) = row.chapter {
                format!(
                    "Chapter {}: {chapter}. {}",
                    row.descent.numeral(),
                    fatherload_simulation::expedition::chapter(row.descent)
                        .map_or("", |chapter| chapter.revelation)
                )
            } else {
                format!(
                    "{} {} canisters still to find here.",
                    fatherload_simulation::expedition::progress::arrival_line(row.descent),
                    row.logs_total - row.logs_read
                )
            }
        }),
        _ => None,
    };
    UiLayout::screen(draw).modal_with_render_plan(
        "Descent Progress",
        "How far down the cable the crew has come.",
        &ModalContent::new(sections).footer(footer).prompts(&[
            hint(Prompt::Navigate, "read"),
            hint(Prompt::Tab, "pages"),
            hint(Prompt::Back, "close"),
        ]),
    );
}

/// The ending (Wave 10): the builders' cable as a line from the Heart at the foot of the page to
/// the Moon at its top, its light climbing a station per beat; the beat's place and words beside
/// it. Everything moves slowly, nothing flashes.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    reason = "page pixels are small integers; one block per part of the page"
)]
pub(super) fn draw_ending(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    use super::ui_kit::{self, PanelStyle, TextStyle};
    use crate::shell::expedition::{ENDING_PAGES, ending_page};
    let canvas = ui_kit::canvas();
    let page = ui.seat.selected_menu_item.min(ENDING_PAGES - 1);
    let (place, text) = ending_page(page);
    draw.draw_rectangle(0, 0, canvas.width, canvas.height, with_alpha(INK, 215));
    // The whole screen is one row for the pointer and touch: a tap anywhere turns the page.
    let screen = Rectangle::new(0.0, 0.0, canvas.width as f32, canvas.height as f32);
    super::pointer::record_dialog(screen);
    super::pointer::record_row(screen, screen, 0);
    let width = (canvas.width - 24).min(440);
    let height = (canvas.height - 24).min(270);
    let x = (canvas.width - width) / 2;
    let y = (canvas.height - height) / 2;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Accent,
    );
    let seconds = ui_kit::seconds();
    // A night sky over the top of the page: still stars, a few breathing slowly.
    for index in 0..34 {
        let sx = x + 8 + (index * 97 + index * index * 13) % (width - 16);
        let sy = y + 6 + (index * 61 + index * index * 7) % 30;
        let breathe = (seconds * 0.7 + index as f32).sin() * 0.5 + 0.5;
        let alpha = if index % 4 == 0 {
            (90.0 + breathe * 100.0) as u8
        } else {
            110
        };
        draw.draw_rectangle(sx, sy, 1, 1, with_alpha(BONE, alpha));
    }
    // The cable: the Heart at the foot, the Moon at the top, a station per page.
    let cable_x = x + 24;
    let top = y + 36;
    let bottom = y + height - 30;
    let stations = ENDING_PAGES as i32 - 1;
    let station_y = |beat: i32| bottom - (bottom - top) * beat / stations.max(1);
    draw.draw_rectangle(cable_x, top, 2, bottom - top, SLATE);
    let lit_to = station_y(page as i32);
    draw.draw_rectangle(
        cable_x - 1,
        lit_to,
        4,
        bottom - lit_to,
        with_alpha(GOLD, 60),
    );
    draw.draw_rectangle(cable_x, lit_to, 2, bottom - lit_to, CREAM);
    // A soft glint climbing the lit cable, slowly and forever (never a flash).
    let span = (bottom - lit_to).max(1);
    let glint = bottom - ((seconds * 30.0) as i32).rem_euclid(span);
    draw.draw_rectangle(cable_x - 1, glint - 3, 4, 6, with_alpha(BONE, 160));
    // Stations, each with its place name: lit up to the page, the page's own in gold.
    for beat in 0..=stations {
        let sy = station_y(beat);
        let lit = beat <= page as i32;
        let current = beat == page as i32;
        draw.draw_circle(
            cable_x + 1,
            sy,
            if current { 4.0 } else { 3.0 },
            if lit { GOLD } else { SLATE_DARK },
        );
        draw.draw_circle(cable_x + 1, sy, 1.0, if lit { CREAM } else { SLATE });
        let (name, _) = ending_page(beat as usize);
        let label = if beat == 0 { "The Star Drive" } else { name };
        ui_kit::text(
            draw,
            label,
            cable_x + 10,
            sy - TextStyle::Body.line_height() / 2,
            TextStyle::Body,
            if current {
                GOLD
            } else if lit {
                BONE
            } else {
                SLATE_LIGHT
            },
        );
    }
    // The Moon at the top (it wakes on the last page), the drive's glow at the foot.
    let moon = if page + 1 == ENDING_PAGES { BONE } else { FOG };
    draw.draw_circle(cable_x + 1, top - 14, 8.0, moon);
    draw.draw_circle(cable_x + 4, top - 16, 2.0, with_alpha(SLATE, 140));
    draw.draw_circle(cable_x - 2, top - 11, 1.5, with_alpha(SLATE, 120));
    let pulse = 0.5 + 0.5 * (seconds * 1.1).sin();
    draw.draw_circle(
        cable_x + 1,
        bottom + 10,
        7.0 + pulse * 1.5,
        with_alpha(GOLD, 110),
    );
    draw.draw_circle(cable_x + 1, bottom + 10, 3.0, CREAM);
    // The words, in the page's right half.
    let text_x = x + width * 2 / 5;
    let text_width = x + width - 14 - text_x;
    ui_kit::text(
        draw,
        "THE CABLE LIGHTS",
        text_x,
        y + 48,
        TextStyle::Body,
        GOLD,
    );
    ui_kit::text(draw, place, text_x, y + 62, TextStyle::Title, CREAM);
    let lines = ui_kit::wrap(text, text_width, TextStyle::Body);
    for (index, line) in lines.iter().take(10).enumerate() {
        ui_kit::text(
            draw,
            line,
            text_x,
            y + 86 + index as i32 * (TextStyle::Body.line_height() + 2),
            TextStyle::Body,
            BONE,
        );
    }
    let footer = if page + 1 == ENDING_PAGES {
        "The Descent is over. The game goes on."
    } else {
        "Confirm or tap: on up the cable."
    };
    let footer_lines = ui_kit::wrap(
        &format!("{}/{}  {footer}", page + 1, ENDING_PAGES),
        text_width,
        TextStyle::Body,
    );
    for (index, line) in footer_lines.iter().enumerate() {
        ui_kit::text(
            draw,
            line,
            text_x,
            y + height - 16 - (footer_lines.len() - 1 - index) as i32 * 12,
            TextStyle::Body,
            FOG,
        );
    }
}
