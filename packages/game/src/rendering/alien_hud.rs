//! The friendly aliens' talk panel (Wave 12 aliens track): the resident's portrait, how much of
//! their tongue the seat's player understands, what they say (each word in the species' glyphs
//! until the player has learned it), and the panel's rows.
//!
//! Speech is rendered per seat from the world's conversation lines through
//! [`fatherload_simulation::alien::speech_tokens`] with the seat's own player's understanding:
//! the same line reads as glyphs to a stranger and as plain words to a fluent speaker.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "UI layout in small integer art pixels"
)]

use fatherload_simulation::{
    alien::{
        AlienSpecies, SpeechToken,
        language::{FULL_UNDERSTANDING, Fluency},
        quest::chapter,
        speech_tokens,
    },
    people::{Character, ledger::Speaker},
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    alien_art,
    layout::{self, ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    people_art::{self, PersonPen},
    pixel_art::sprites::menu_icons,
    prompts::hint,
    screen::UiFrame,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    shell::aliens::{AlienPage, AlienRow, ChapterRow, alien_rows, resident, row_text},
};

const HELP: &[super::prompts::Hint] = &[
    hint(Prompt::Navigate, "choose"),
    hint(Prompt::Confirm, "say"),
    hint(Prompt::Back, "back"),
];

/// The colour a species writes in.
#[must_use]
pub const fn ink(species: AlienSpecies) -> Color {
    match species {
        AlienSpecies::Vessi => SKY_LIGHT,
        AlienSpecies::Tokkari => AMBER,
        AlienSpecies::Siphra => MINT,
    }
}

/// Lines of speech the card shows (the newest).
const SHOWN_LINES: usize = 3;

/// One word laid out: a word in the font, or a run of glyphs.
enum Laid<'a> {
    Word(&'a str),
    Glyphs(&'a [u8], &'a str),
}

/// The width (art pixels) of a laid-out token.
fn width_of(token: &Laid<'_>) -> i32 {
    match token {
        Laid::Word(word) => ui_kit::text_width(word, TextStyle::Body),
        Laid::Glyphs(glyphs, trailing) => {
            alien_art::glyphs_width(glyphs.len(), 1.0) as i32
                + ui_kit::text_width(trailing, TextStyle::Body)
        }
    }
}

/// Wraps `tokens` to `width` art pixels: lines of laid tokens.
fn wrap(tokens: &[SpeechToken], width: i32) -> Vec<Vec<Laid<'_>>> {
    let space = ui_kit::text_width(" ", TextStyle::Body).max(3);
    let mut lines: Vec<Vec<Laid<'_>>> = vec![Vec::new()];
    let mut used = 0;
    for token in tokens {
        let laid = match token {
            SpeechToken::Word(word) => Laid::Word(word.as_str()),
            SpeechToken::Glyphs { glyphs, trailing } => Laid::Glyphs(glyphs, trailing.as_str()),
        };
        let w = width_of(&laid);
        if used > 0 && used + space + w > width {
            lines.push(Vec::new());
            used = 0;
        }
        if used > 0 {
            used += space;
        }
        used += w;
        if let Some(line) = lines.last_mut() {
            line.push(laid);
        }
    }
    lines
}

/// Draws laid lines from `(x, y)`; returns the height used.
fn draw_lines(
    draw: &mut impl RaylibDraw,
    lines: &[Vec<Laid<'_>>],
    x: i32,
    y: i32,
    species: AlienSpecies,
    colour: Color,
) -> i32 {
    let space = ui_kit::text_width(" ", TextStyle::Body).max(3);
    let line_height = TextStyle::Body.line_height().max(9);
    for (row, line) in lines.iter().enumerate() {
        let mut cursor = x;
        let top = y + row as i32 * line_height;
        for laid in line {
            match laid {
                Laid::Word(word) => {
                    ui_kit::text(draw, word, cursor, top, TextStyle::Body, colour);
                }
                Laid::Glyphs(glyphs, trailing) => {
                    for (index, glyph) in glyphs.iter().enumerate() {
                        alien_art::draw_glyph(
                            draw,
                            (cursor + index as i32 * 6) as f32,
                            (top + 1) as f32,
                            1.0,
                            species,
                            *glyph,
                            ink(species),
                        );
                    }
                    let after = cursor + alien_art::glyphs_width(glyphs.len(), 1.0) as i32;
                    ui_kit::text(draw, trailing, after, top, TextStyle::Body, colour);
                }
            }
            cursor += width_of(laid) + space;
        }
    }
    lines.len() as i32 * line_height
}

/// Draws seat `ui.seat`'s alien conversation.
#[allow(clippy::too_many_lines, reason = "the panel's sections in one place")]
pub(super) fn draw_alien_talk(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let Some(talk) = &ui.seat.village.talk else {
        return;
    };
    let Some((found, slot)) = resident(talk.speaker) else {
        return;
    };
    let Some(character) = found.population.characters.get(slot).cloned() else {
        return;
    };
    let species = found.species;
    let world = ui.snapshot.world;
    let player = ui.view.player_id;
    let understanding = world.understanding(player, species);
    let fluency = Fluency::of(understanding);
    let page = ui.seat.alien.page;
    let rows = alien_rows(world, Some(player), talk.speaker, page);
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let items: Vec<SectionItem> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let (label, detail) = row_text(world, Some(player), &found, *row);
            let icon = match row {
                AlienRow::Chat => &menu_icons::PERSON,
                AlienRow::Trade
                | AlienRow::Buy(_)
                | AlienRow::Sell(..)
                | AlienRow::Gift
                | AlienRow::GiftItem(_) => &menu_icons::GIFT,
                AlienRow::Lesson | AlienRow::Chapter(ChapterRow::Ask) => &menu_icons::BOOK,
                AlienRow::Chapter(_) => &menu_icons::CONTRACT,
                AlienRow::Homes
                | AlienRow::Plot(_)
                | AlienRow::Rest(_)
                | AlienRow::Expand(_)
                | AlienRow::Neighbours(_) => &menu_icons::HOUSE,
                AlienRow::Goodbye | AlienRow::Back => &menu_icons::BACK,
            };
            SectionItem::item(ItemEntry {
                icon: Some(icon),
                label,
                detail: String::new(),
                value: detail,
                value_color: BONE,
                coin: false,
                selected: index == selected,
                disabled: false,
            })
        })
        .collect();
    let log = world.aliens().log(player, species);
    let mut tongue = vec![
        SectionItem::meter(
            "Understanding",
            f32::from(understanding),
            f32::from(FULL_UNDERSTANDING),
            ink(species),
            ink(species),
        ),
        SectionItem::stat("Their tongue", fluency.name(), ink(species)),
    ];
    if log.stones > 0 {
        tongue.push(SectionItem::stat(
            "Lexicon stones read",
            log.stones.to_string(),
            GOLD,
        ));
    }
    let mut sections = vec![Section::new(
        format!("{} ({})", species.def().name, species.def().endonym),
        ink(species),
        tongue,
    )];
    if let Some(active) = log.active
        && let Some(def) = chapter(species, active.chapter)
    {
        let state = if active.passenger.is_some() {
            "a traveller rides with you"
        } else if active.reached {
            "done: tell the elder"
        } else {
            "under way"
        };
        let mut chapter_items = vec![SectionItem::stat(def.title, state, GOLD)];
        // The goal in plain words (the player's own notes), whatever they read of the elder.
        if let Some((_, objective)) = chapter_objective(world, player, species) {
            chapter_items.push(SectionItem::text(objective));
        }
        sections.push(Section::new("Their chapter", GOLD, chapter_items));
    }
    let title = match page {
        AlienPage::Talk => "Say",
        AlienPage::Trade => "Trade",
        AlienPage::Homes => "Homes for sale",
        AlienPage::Gift => "A gift",
    };
    sections.push(Section::new(title, LIME, items));
    // Docked like a counter: the resident, and what they say, beside the panel.
    layout::set_modal_placement(layout::ModalPlacement::Counter);
    UiLayout::screen(draw).modal_with_render_plan(
        &talk.name,
        &talk.about,
        &ModalContent::new(sections).prompts(HELP),
    );
    layout::set_modal_placement(layout::ModalPlacement::Centred);
    let portrait_bottom = draw_portrait(draw, &character, species);
    draw_speech_card(
        draw,
        ui,
        &character,
        species,
        understanding,
        portrait_bottom,
    );
}

/// The card of what the resident said, above the panel (or below it when there is no room).
fn draw_speech_card(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    character: &Character,
    species: AlienSpecies,
    understanding: u16,
    below: Option<i32>,
) {
    let Some(panel) = layout::peek_last_dialog_rect() else {
        return;
    };
    let Some(talk) = &ui.seat.village.talk else {
        return;
    };
    let canvas = ui_kit::canvas();
    // Beside the docked panel when there is room, else across the top of the view.
    let right = (panel.x + panel.width) as i32 + 8;
    let beside = canvas.width - right - 8 >= 150;
    let width = if beside {
        canvas.width - right - 8
    } else {
        canvas.width - 16
    };
    let inner = width - 16;
    let world = ui.snapshot.world;
    let player = ui.view.player_id;
    let mut said: Vec<(Speaker, String)> = world
        .people()
        .conversation(player)
        .filter(|conversation| conversation.npc == talk.npc)
        .map(|conversation| {
            let skip = conversation.lines.len().saturating_sub(SHOWN_LINES);
            conversation
                .lines
                .iter()
                .skip(skip)
                .map(|line| (line.speaker, line.text.clone()))
                .collect()
        })
        .unwrap_or_default();
    // The elder's request while a chapter is offered.
    let log = world.aliens().log(player, species);
    if let Some(def) = log.offered.and_then(|index| chapter(species, index)) {
        said.push((Speaker::Them, def.ask.to_owned()));
    } else if let Some(def) = log
        .active
        .and_then(|active| chapter(species, active.chapter))
        && resident(talk.speaker)
            .is_some_and(|(_, slot)| slot == fatherload_simulation::alien::town::ELDER)
    {
        said.push((Speaker::Them, def.nudge.to_owned()));
    }
    if said.is_empty() {
        said.push((
            Speaker::Note,
            format!("{} turns toward you...", character.name.short()),
        ));
    }
    let name = character.name.short().to_owned();
    let mut blocks: Vec<(Color, Vec<SpeechToken>)> = Vec::new();
    for (speaker, text) in &said {
        match speaker {
            Speaker::Them => blocks.push((CREAM, speech_tokens(text, species, understanding))),
            Speaker::You => blocks.push((SKY_LIGHT, plain(&format!("You: {text}")))),
            Speaker::Note => blocks.push((MIST, plain(&format!("> {text}")))),
        }
    }
    let laid: Vec<(Color, Vec<Vec<Laid<'_>>>)> = blocks
        .iter()
        .map(|(colour, tokens)| (*colour, wrap(tokens, inner - 4)))
        .collect();
    let line_height = TextStyle::Body.line_height().max(9);
    let lines: i32 = laid.iter().map(|(_, lines)| lines.len() as i32).sum();
    let height = 22 + lines * line_height + laid.len() as i32 * 3;
    let (x, y) = if beside {
        (right, below.map_or(panel.y as i32, |bottom| bottom + 6))
    } else {
        (8, 4)
    };
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Accent,
    );
    ui_kit::text(draw, &name, x + 8, y + 6, TextStyle::Bold, ink(species));
    let readable = if understanding >= FULL_UNDERSTANDING {
        "every word".to_owned()
    } else {
        format!("you follow {}%", understanding / 10)
    };
    let width_readable = ui_kit::text_width(&readable, TextStyle::Body);
    ui_kit::text(
        draw,
        &readable,
        x + width - 8 - width_readable,
        y + 6,
        TextStyle::Body,
        MIST,
    );
    let mut top = y + 18;
    for (colour, lines) in &laid {
        top += draw_lines(draw, lines, x + 8, top, species, *colour) + 3;
    }
}

/// Plain words (the player's own lines and notes are never in their tongue).
fn plain(text: &str) -> Vec<SpeechToken> {
    text.split_whitespace()
        .map(|word| SpeechToken::Word(word.to_owned()))
        .collect()
}

/// The resident's portrait beside the panel; returns the card's bottom (`None` if there was
/// no room for it).
fn draw_portrait(
    draw: &mut RaylibDrawHandle<'_>,
    character: &Character,
    species: AlienSpecies,
) -> Option<i32> {
    let canvas = ui_kit::canvas();
    let panel = layout::peek_last_dialog_rect()?;
    let scale = if canvas.height >= 300 { 3 } else { 2 };
    let art = people_art::PORTRAIT_SIZE * scale;
    let card_w = art + 16;
    let card_h = art + 30;
    let free_left = (panel.x + panel.width) as i32 + 8;
    if canvas.width - free_left - 8 < 150 || card_h + 90 > canvas.height {
        return None;
    }
    let (x, y) = (free_left, (panel.y as i32).max(8));
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, card_w as f32, card_h as f32),
        PanelStyle::Accent,
    );
    ui_kit::text(
        draw,
        species.def().name,
        x + 8,
        y + 6,
        TextStyle::Bold,
        ink(species),
    );
    let well = Rectangle::new(
        (x + 6) as f32,
        (y + 18) as f32,
        (art + 4) as f32,
        (art + 4) as f32,
    );
    ui_kit::well(draw, well);
    let seconds = ui_kit::seconds();
    people_art::draw_portrait(
        draw,
        PersonPen {
            x: well.x + 2.0,
            y: well.y + 2.0,
            pixel: scale as f32,
        },
        character,
        ((seconds * 0.8) as i32) % 3 != 0,
        seconds,
    );
    Some(y + card_h)
}

/// What `player`'s running chapter of `species` asks, as their own notes in plain words (the
/// elder's request reads only as well as the player knows the tongue, so the goal is written
/// down plainly): `(title, objective)`.
#[must_use]
pub fn chapter_objective(
    world: &fatherload_simulation::WorldState,
    player: fatherload_simulation::PlayerId,
    species: AlienSpecies,
) -> Option<(&'static str, String)> {
    use fatherload_simulation::alien::{quest::ChapterGoal, town::town_by_id};
    let active = world.aliens().log(player, species).active?;
    let def = chapter(species, active.chapter)?;
    let town = town_by_id(active.town).map_or_else(|| "the town".to_owned(), |t| t.name.clone());
    let have = |mineral: fatherload_simulation::terrain::MineralKind| {
        world
            .player(player)
            .map_or(0, |rig| rig.cargo_bay.count(mineral))
    };
    let line = if active.reached {
        format!("Done: tell the elder of {town}")
    } else {
        match def.goal {
            ChapterGoal::Deliver { mineral, count } => format!(
                "Bring {count} {} to the elder of {town} ({}/{count})",
                mineral.name(),
                have(mineral).min(count)
            ),
            ChapterGoal::ReachDepth { rows } => format!("Dig {rows} rows down under {town}"),
            ChapterGoal::ReachFarSide => {
                format!("Stand on the far side of the world from {town}")
            }
            ChapterGoal::Escort => {
                let traveller = active
                    .passenger
                    .and_then(|id| {
                        town_by_id(id.village)?
                            .population
                            .characters
                            .get(usize::from(id.index))
                            .map(|character| character.name.short().to_owned())
                    })
                    .unwrap_or_else(|| "the traveller".to_owned());
                let to = active.destination.and_then(town_by_id).map_or_else(
                    || "the far side of the world".to_owned(),
                    |t| t.name.clone(),
                );
                format!("Carry {traveller} to {to}")
            }
            ChapterGoal::Raise {
                mineral,
                count,
                credits,
            } => format!(
                "Bring {count} {} ({}/{count}) and {} cr to the elder of {town}",
                mineral.name(),
                have(mineral).min(count),
                crate::credits_text::grouped(credits)
            ),
            ChapterGoal::Trail => format!("Listen to the elder of {town}"),
        }
    };
    Some((def.title, line))
}

/// The seat's running alien chapters (Wave 12), under the quest tracker: each title in the
/// species' ink and its goal in plain words.
pub(super) fn draw_chapter_tracker(
    draw: &mut RaylibDrawHandle<'_>,
    world: &fatherload_simulation::WorldState,
    player: fatherload_simulation::PlayerId,
    (x, y, width): (i32, i32, i32),
) {
    let mut top = y;
    for species in AlienSpecies::ALL {
        let Some((title, line)) = chapter_objective(world, player, species) else {
            continue;
        };
        let inner = width - 12;
        let mut lines = ui_kit::wrap(&line, inner, TextStyle::Body);
        lines.truncate(2);
        let height = 18 + i32::try_from(lines.len()).unwrap_or(1) * TextStyle::Body.line_height();
        ui_kit::panel(
            draw,
            Rectangle::new(x as f32, top as f32, width as f32, height as f32),
            PanelStyle::Standard,
        );
        ui_kit::icon(draw, &menu_icons::CONTRACT, x + 4, top + 4);
        ui_kit::text(
            draw,
            &ui_kit::truncate(title, inner - 14, TextStyle::Bold),
            x + 18,
            top + 5,
            TextStyle::Bold,
            ink(species),
        );
        for (index, text) in lines.iter().enumerate() {
            ui_kit::text(
                draw,
                text,
                x + 6,
                top + 16 + i32::try_from(index).unwrap_or(0) * TextStyle::Body.line_height(),
                TextStyle::Body,
                MIST,
            );
        }
        top += height + 2;
    }
}
