//! Story UI (WP-STORY): the journal page and the story moment card (a new story, its climax,
//! its end) that slides in across the seat's view.

use fatherload_simulation::people::{ArcKind, ArcLog};
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
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    shell::{JournalRow, STORY_MOMENT_TICKS, StoryMoment, StoryMomentKind, journal_rows},
};

/// The colour a kind of story is marked with in the journal.
const fn kind_color(kind: ArcKind) -> Color {
    match kind {
        ArcKind::Frontier => GOLD,
        ArcKind::Village => MINT,
        ArcKind::Sky => SKY_LIGHT,
        ArcKind::Mystery => VIOLET,
        ArcKind::Home => ORANGE,
        ArcKind::Night => MIST,
        ArcKind::Pilot => LIME,
        ArcKind::Crew => PINK,
        ArcKind::Tomb => AMBER,
    }
}

/// Wrapped paragraph lines as section text items.
fn paragraph(text: &str) -> Vec<SectionItem> {
    vec![SectionItem::text(text.to_owned())]
}

/// The journal page: stories under way, stories told, the story in focus.
#[allow(
    clippy::too_many_lines,
    reason = "one row builder per kind of journal row"
)]
pub(super) fn draw_journal(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let log: &ArcLog = &ui.player.quests.arcs;
    let rows = journal_rows(Some(log));
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let mut running = Vec::new();
    let mut told = Vec::new();
    let mut actions = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let picked = index == selected;
        match *row {
            JournalRow::Running(at) => {
                let Some(run) = log.active.get(at) else {
                    continue;
                };
                let Some(template) = run.template() else {
                    continue;
                };
                running.push(SectionItem::item(ItemEntry {
                    icon: Some(&menu_icons::BOOK),
                    label: template.title.to_owned(),
                    detail: format!(
                        "{} | {} of {}",
                        template.kind.name(),
                        run.cast.giver_name,
                        run.cast.home
                    ),
                    value: if run.ready() {
                        "TELL IT".to_owned()
                    } else {
                        format!("{}/{}", run.step() + 1, template.stages.len())
                    },
                    value_color: if run.ready() {
                        LIME
                    } else {
                        kind_color(template.kind)
                    },
                    coin: false,
                    selected: picked,
                    disabled: false,
                }));
            }
            JournalRow::Told(at) => {
                let Some(record) = log.history.get(at) else {
                    continue;
                };
                let Some(template) = record.cast.template() else {
                    continue;
                };
                told.push(SectionItem::item(ItemEntry {
                    icon: Some(if record.abandoned {
                        &menu_icons::GHOST
                    } else {
                        &menu_icons::READY
                    }),
                    label: template.title.to_owned(),
                    detail: format!("{}, day {}", record.cast.giver_name, record.day),
                    value: if record.abandoned {
                        "LET GO".to_owned()
                    } else {
                        "TOLD".to_owned()
                    },
                    value_color: if record.abandoned {
                        FOG
                    } else {
                        kind_color(template.kind)
                    },
                    coin: false,
                    selected: picked,
                    disabled: false,
                }));
            }
            JournalRow::GiveUp => {
                let armed = ui.seat.journal.abandon_armed.is_some();
                actions.push(SectionItem::item(ItemEntry {
                    icon: Some(&menu_icons::BACK),
                    label: if armed {
                        "Really give it up?".to_owned()
                    } else {
                        "Give up this story".to_owned()
                    },
                    detail: "The giver will remember".to_owned(),
                    value: String::new(),
                    value_color: RED,
                    coin: false,
                    selected: picked,
                    disabled: false,
                }));
            }
            JournalRow::TurnPage => actions.push(SectionItem::item(ItemEntry {
                icon: Some(&menu_icons::MEDAL_GOLD),
                label: "Turn the page: milestones".to_owned(),
                detail: String::new(),
                value: String::new(),
                value_color: GOLD,
                coin: false,
                selected: picked,
                disabled: false,
            })),
        }
    }
    let mut sections = Vec::new();
    if running.is_empty() {
        running = paragraph(
            "No stories under way. People in villages, camps and the town have things on their \
             minds: talk to them and ask if they need a hand.",
        );
    }
    sections.push(Section::new("Stories under way", GOLD, running));
    sections.extend(focus_section(ui, log, &rows));
    if !told.is_empty() {
        sections.push(Section::new("Stories told", SKY_LIGHT, told));
    }
    sections.push(Section::new("", FOG, actions));
    UiLayout::screen(draw).modal_with_render_plan(
        "Journal",
        "The stories you've taken on, and how they ended.",
        &ModalContent::new(sections).prompts(&[
            hint(Prompt::Navigate, "read"),
            hint(Prompt::Tab, "pages"),
            hint(Prompt::Back, "close"),
        ]),
    );
}

/// The story in focus: where it stands, what to do, how it ended.
fn focus_section(ui: &UiFrame<'_, '_>, log: &ArcLog, rows: &[JournalRow]) -> Option<Section> {
    match rows.get(ui.seat.journal.focus)? {
        JournalRow::Running(at) => {
            let run = log.active.get(*at)?;
            let template = run.template()?;
            let mut items = Vec::new();
            let past = run.story_so_far();
            if let Some(last) = past.last() {
                // Only the beat just behind you: the journal reads forward.
                items.push(SectionItem::text(format!("Before: {last}")));
            }
            items.push(SectionItem::text(run.journal_text()));
            for (line, done) in run.objective_lines(Some(ui.player), ui.snapshot.tick_rate) {
                items.push(SectionItem::text(if done {
                    format!("[x] {line}")
                } else {
                    format!("[ ] {line}")
                }));
            }
            Some(Section::new(
                template.title,
                kind_color(template.kind),
                items,
            ))
        }
        JournalRow::Told(at) => {
            let record = log.history.get(*at)?;
            let template = record.cast.template()?;
            let mut items = vec![SectionItem::text(format!(
                "How it ended: {}.",
                record.ending().map_or("let go", |ending| ending.label)
            ))];
            items.push(SectionItem::text(
                if record.abandoned && record.ending().is_none() {
                    format!(
                        "You let this one go. {} hasn't forgotten.",
                        record.cast.giver_name
                    )
                } else {
                    record.epilogue()
                },
            ));
            Some(Section::new(
                template.title,
                kind_color(template.kind),
                items,
            ))
        }
        JournalRow::GiveUp | JournalRow::TurnPage => None,
    }
}

/// The story moment card: a heading, the story's title and its words, sliding in from the top.
pub(super) fn draw_story_moment(draw: &mut RaylibDrawHandle<'_>, moment: &StoryMoment, now: u64) {
    if !moment.showing(now) {
        return;
    }
    let age = now.saturating_sub(moment.tick);
    let canvas = ui_kit::canvas();
    let width = (canvas.width - 24).min(380);
    let (heading, color, style) = match moment.kind {
        StoryMomentKind::Begun => ("A NEW STORY", GOLD, PanelStyle::Accent),
        StoryMomentKind::Climax => ("THE STORY TURNS", VIOLET, PanelStyle::Accent),
        StoryMomentKind::Ended => ("STORY'S END", LIME, PanelStyle::Accent),
        StoryMomentKind::Lost => ("A STORY LET GO", FOG, PanelStyle::Standard),
        StoryMomentKind::ExpeditionLog => ("EXPEDITION LOG", AMBER, PanelStyle::Accent),
        StoryMomentKind::RadioEcho => ("A RADIO ECHO", SKY_LIGHT, PanelStyle::Standard),
        StoryMomentKind::GhostRecording => ("A GHOST RECORDING", MIST, PanelStyle::Standard),
        StoryMomentKind::Anomaly => ("AN ANOMALY", LAVENDER, PanelStyle::Accent),
        StoryMomentKind::Remembered => ("THEY REMEMBER THE FATHERS", SAND, PanelStyle::Standard),
    };
    let mut lines = ui_kit::wrap(&moment.text, width - 22, TextStyle::Body);
    lines.truncate(match moment.kind {
        StoryMomentKind::ExpeditionLog
        | StoryMomentKind::RadioEcho
        | StoryMomentKind::GhostRecording
        | StoryMomentKind::Remembered => 8,
        _ => 6,
    });
    let heading_y = 6;
    let title_y = heading_y + TextStyle::Body.line_height() + 2;
    let text_y = title_y + TextStyle::Title.line_height() + 4;
    let height =
        text_y + 6 + i32::try_from(lines.len()).unwrap_or(0) * (TextStyle::Body.line_height() + 1);
    // Slides down, holds, then lifts away.
    let enter = i32::try_from(16_u64.saturating_sub(age)).unwrap_or(0) * 5;
    let leave = i32::try_from(age.saturating_sub(STORY_MOMENT_TICKS - 16)).unwrap_or(0) * 6;
    let x = (canvas.width - width) / 2;
    let y = 40 - enter - leave;
    // A parchment page behind the panel, a bookmark ribbon on its edge.
    draw.draw_rectangle(x + 4, y + 4, width, height, INK);
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        style,
    );
    draw.draw_rectangle(x + width - 18, y - 4, 7, 14, color);
    draw.draw_rectangle(x + width - 18, y + 10, 3, 3, color);
    draw.draw_rectangle(x + width - 14, y + 10, 3, 3, color);
    if moment.kind == StoryMomentKind::Climax {
        // Motes drifting up from the page.
        for index in 0..6_u64 {
            let drift = i32::try_from((age + index * 37) % 90).unwrap_or(0);
            let mx = x
                + 14
                + i32::try_from(index * 61 % u64::try_from(width - 28).unwrap_or(1)).unwrap_or(0);
            let my = y + height - 6 - drift / 2;
            if my > y + 4 {
                draw.draw_rectangle(mx, my, 1, 1, if index % 2 == 0 { CREAM } else { color });
            }
        }
    }
    super::expedition_card::decorate(draw, moment.kind, (x, y, width, height), age, color);
    let centre = x + width / 2;
    ui_kit::text_centred(draw, heading, centre, y + heading_y, TextStyle::Body, color);
    ui_kit::text_centred(
        draw,
        &ui_kit::truncate(&moment.title, width - 24, TextStyle::Title),
        centre,
        y + title_y,
        TextStyle::Title,
        CREAM,
    );
    for (index, line) in lines.iter().enumerate() {
        ui_kit::text(
            draw,
            line,
            x + 11,
            y + text_y + i32::try_from(index).unwrap_or(0) * (TextStyle::Body.line_height() + 1),
            TextStyle::Body,
            BONE,
        );
    }
}
