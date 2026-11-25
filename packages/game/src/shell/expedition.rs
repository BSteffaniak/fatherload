//! The Fathers' expedition in the shell (Wave 8 story): the expedition journal page and the
//! words for the cards and messages its events raise.
//!
//! The journal ([`super::ModalScreen::ExpeditionLog`]) follows the tomb codex in the map's page
//! cycle on Deep Descent worlds. Its rows are the logs the crew has read, in order (selecting
//! one shows its full entry under the list), then "Turn the page" (back to the map), so it works
//! by keys, pad, mouse and touch alike: a tap selects a row and confirms it, and confirming a
//! log row only shows it. The crew shares one journal, so every seat reads the same pages.

use fatherload_simulation::{
    WorldState,
    expedition::{
        CHAPTERS, ECHOES, EchoKind, ExpeditionState, LOGS,
        anomaly::AnomalyKind,
        chapter,
        codex::{CodexEntry, CodexPage},
    },
};

use super::{ModalScreen, SeatUi, SoundCue, StoryMomentKind};
use crate::input::PlayerInput;

/// One row of the expedition journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpeditionRow {
    /// A log read (index into [`LOGS`]).
    Log(usize),
    /// Back to the map.
    TurnPage,
}

/// The journal's rows for `state`.
#[must_use]
pub fn expedition_rows(state: &ExpeditionState) -> Vec<ExpeditionRow> {
    let mut rows: Vec<ExpeditionRow> = (0..usize::from(state.found).min(LOGS.len()))
        .map(ExpeditionRow::Log)
        .collect();
    rows.push(ExpeditionRow::TurnPage);
    rows
}

/// Whether the expedition journal is in the page cycle (Deep Descent worlds).
#[must_use]
pub const fn has_expedition_page(world: &WorldState) -> bool {
    world.terrain().settings().generator.has_descents()
}

/// The full entry of log `index` as the journal and the card show it.
#[must_use]
pub fn log_entry(index: usize) -> Option<(String, String)> {
    let log = LOGS.get(index)?;
    Some((
        format!("{}. {}", index + 1, log.title),
        format!(
            "Day {}, {} ({}): {}",
            log.day,
            log.author.name(),
            log.author.role(),
            log.text
        ),
    ))
}

/// The trail's lead: where the next canister lies, in words.
#[must_use]
pub fn next_lead(world: &WorldState) -> String {
    let state = world.expedition();
    let Some(next) = world.next_log_site() else {
        return if usize::from(state.found) >= LOGS.len() {
            "The trail goes on below. The canisters this deep are still to be found.".to_owned()
        } else {
            "No trail here: the Fathers went down a Deep Descent world's cable.".to_owned()
        };
    };
    let side =
        if fatherload_simulation::expedition::sites::trail_side(world.terrain().settings()) > 0 {
            "east"
        } else {
            "west"
        };
    let ground =
        fatherload_simulation::descent::fields::cached_layout(world.terrain().settings()).tops[0];
    format!(
        "Next: log {} at a {} in the {}, {} of the builders' cable, about {} rows down.",
        next.log + 1,
        next.kind.name(),
        next.stratum.name(),
        side,
        (next.floor_row() - ground).max(0)
    )
}

/// The card and message a read log raises: `(kind, title, text, message)`.
#[must_use]
pub fn log_found_words(log: u8) -> Option<(StoryMomentKind, String, String, String)> {
    let index = usize::from(log.checked_sub(1)?);
    let (title, text) = log_entry(index)?;
    let entry = &LOGS[index];
    let last_of_chapter = LOGS
        .get(index + 1)
        .is_none_or(|next| next.descent() != entry.descent());
    let message = chapter(entry.descent())
        .filter(|_| last_of_chapter)
        .map_or_else(
            || {
                format!(
                    "+{} credits from the canister. The journal (map pages) keeps it.",
                    fatherload_simulation::expedition::log_bounty(entry.descent())
                )
            },
            |chapter| {
                format!(
                    "Chapter {} complete: {}. Read it in the expedition journal.",
                    entry.descent().numeral(),
                    chapter.title
                )
            },
        );
    Some((StoryMomentKind::ExpeditionLog, title, text, message))
}

/// The card an echo raises: `(kind, title, text)`.
#[must_use]
pub fn echo_words(echo: u8) -> Option<(StoryMomentKind, String, String)> {
    let entry = ECHOES.get(usize::from(echo.checked_sub(1)?))?;
    let kind = match entry.kind {
        EchoKind::Radio => StoryMomentKind::RadioEcho,
        EchoKind::Ghost => StoryMomentKind::GhostRecording,
        EchoKind::Builder => StoryMomentKind::BuilderEcho,
    };
    Some((
        kind,
        entry.title.to_owned(),
        format!("{}: {}", entry.voice(), entry.text),
    ))
}

/// The card an anomaly room raises: `(kind, title, text)`.
#[must_use]
pub fn anomaly_words(kind: AnomalyKind) -> (StoryMomentKind, String, String) {
    // The deeper rooms carry a builder inscription over their glyph (Wave 9): the card quotes
    // it, and the codex keeps it.
    let text = kind.inscription().map_or_else(
        || kind.hint().to_owned(),
        |inscription| {
            format!(
                "{} Over the glyph: {inscription} (Copied into the codex.)",
                kind.hint()
            )
        },
    );
    (StoryMomentKind::Anomaly, kind.name().to_owned(), text)
}

/// The message for a canister reached before its turn, when the crew has read logs
/// `1..=found` (so the one to find next is `found + 1`, which may be several logs back).
#[must_use]
pub fn sealed_words(log: u8, found: u8) -> String {
    format!(
        "A sealed canister: log {log}. Ines keyed it to the entry before. Find log {} first.",
        found.saturating_add(1).min(log.saturating_sub(1)).max(1)
    )
}

/// The chapter lines the journal shows for `state`: `(numeral, title, read, total,
/// revelation once complete)`.
#[must_use]
pub fn chapter_lines(
    state: &ExpeditionState,
) -> Vec<(
    &'static str,
    &'static str,
    usize,
    usize,
    Option<&'static str>,
)> {
    CHAPTERS
        .iter()
        .map(|chapter| {
            let (read, total) = state.chapter_progress(chapter.descent);
            (
                chapter.descent.numeral(),
                chapter.title,
                read,
                total,
                (total > 0 && read == total).then_some(chapter.revelation),
            )
        })
        .collect()
}

impl SeatUi {
    /// The expedition journal page.
    pub(super) fn handle_expedition_log(
        &mut self,
        input: PlayerInput,
        world: &WorldState,
        cues: &mut Vec<SoundCue>,
    ) {
        if input.backs_out() {
            self.modal = None;
            cues.push(SoundCue::UiBack);
            return;
        }
        let rows = expedition_rows(world.expedition());
        self.move_selection(input, rows.len().saturating_sub(1));
        self.selected_menu_item = self.selected_menu_item.min(rows.len().saturating_sub(1));
        if input.tab_previous {
            self.open_modal(ModalScreen::TombCodex);
            return;
        }
        if input.tab_next {
            self.open_modal(ModalScreen::ExpeditionCodex);
            return;
        }
        if !input.confirm {
            return;
        }
        match rows.get(self.selected_menu_item) {
            Some(ExpeditionRow::Log(_)) => cues.push(SoundCue::UiMove),
            Some(ExpeditionRow::TurnPage) | None => {
                self.open_modal(ModalScreen::ExpeditionCodex);
            }
        }
    }

    /// The expedition codex page (Wave 9): the diary and the builders' words.
    pub(super) fn handle_expedition_codex(
        &mut self,
        input: PlayerInput,
        world: &WorldState,
        cues: &mut Vec<SoundCue>,
    ) {
        if input.backs_out() {
            self.modal = None;
            cues.push(SoundCue::UiBack);
            return;
        }
        let rows = codex_rows(&codex_entries(world));
        self.move_selection(input, rows.len().saturating_sub(1));
        self.selected_menu_item = self.selected_menu_item.min(rows.len().saturating_sub(1));
        if input.tab_previous {
            self.open_modal(ModalScreen::ExpeditionLog);
            return;
        }
        // The star charts come next (Wave 10), then the map.
        let next = ModalScreen::StarCharts;
        if input.tab_next {
            self.open_modal(next);
            return;
        }
        if !input.confirm {
            return;
        }
        match rows.get(self.selected_menu_item) {
            Some(CodexRow::Entry(..)) => cues.push(SoundCue::UiMove),
            Some(CodexRow::TurnPage) | None => self.open_modal(next),
        }
    }
}

/// One row of the expedition codex.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexRow {
    /// A found entry (index into [`codex_entries`]).
    Entry(usize),
    /// Back to the map.
    TurnPage,
}

/// Every codex entry of `world`, found or not: the diary's, then the tablets'.
#[must_use]
pub fn codex_entries(world: &WorldState) -> Vec<CodexEntry> {
    codex_entries_of(world.crust(), world.old_deep(), world.expedition())
}

/// [`codex_entries`] from the replicated states it reads (the renderer's snapshot holds them).
#[must_use]
pub fn codex_entries_of(
    crust: &fatherload_simulation::crust::CrustState,
    deep: &fatherload_simulation::old_deep::OldDeep,
    expedition: &ExpeditionState,
) -> Vec<CodexEntry> {
    CodexPage::ALL
        .iter()
        .flat_map(|page| {
            fatherload_simulation::expedition::codex::page(*page, crust, deep, expedition)
        })
        .collect()
}

/// The codex's rows for `entries`: every found entry in page order, then "Turn the page".
#[must_use]
pub fn codex_rows(entries: &[CodexEntry]) -> Vec<CodexRow> {
    let mut rows: Vec<CodexRow> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.found)
        .map(|(index, _)| CodexRow::Entry(index))
        .collect();
    rows.push(CodexRow::TurnPage);
    rows
}
