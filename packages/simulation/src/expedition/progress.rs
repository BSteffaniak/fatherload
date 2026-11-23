//! The crew's progress through the Descent (Wave 10).
//!
//! The Descents reached, the gate bosses beaten and the expedition logs found per Descent, as
//! the journal's "Descent progress" page shows it, plus the story events the tick raises when
//! they change.
//!
//! Everything here is read from [`super::ExpeditionState`] (saved, hashed once non-empty and
//! replicated in the shared `Expedition` section), so every seat and co-op partner sees the
//! same page.

use serde::{Deserialize, Serialize};

use super::{ExpeditionState, LOGS};
use crate::descent::{Descent, DescentBoss};

/// What the story tick reports (`GameplayEventKind::Story`, netcode codes 700-719).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum StoryEvent {
    /// The crew reached a Descent for the first time.
    DescentReached(Descent),
    /// The crew beat a Descent's gate boss.
    BossBeaten(DescentBoss),
    /// The Heart woke and the cable to the Moon lit: the ending
    /// ([`super::ending`]).
    CableLit,
}

impl StoryEvent {
    /// Offset of the event in the story's netcode range (700 + code).
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::DescentReached(_) => 0,
            Self::BossBeaten(_) => 1,
            Self::CableLit => 2,
        }
    }
}

/// What the card says when the crew first reaches `descent`: the chapter's promise, never its
/// spoiler.
#[must_use]
pub const fn arrival_line(descent: Descent) -> &'static str {
    match descent {
        Descent::Crust => {
            "Six people went down a cable nobody can scratch, and never came back up. Follow it."
        }
        Descent::OldDeep => {
            "Below the lid: sleeping machines, singing glass, and a lamp the town called the \
             bottom."
        }
        Descent::LivingDark => {
            "Life that makes its own light. Somewhere below, a hive remembers the Lamp-Bearers."
        }
        Descent::DrownedAndFrozen => {
            "A sea under the rock, ice older than the town, and people who sing."
        }
        Descent::HollowWorld => "The rock gives way to sky. The planet is hollow, and lit.",
        Descent::StrangeLayers => {
            "Near the builders' machine the world wears thin. Trust the cable, not your eyes."
        }
        Descent::Ancients => {
            "Streets: lamps on poles, trams on rails, and a voice that has waited an age to say \
             hello."
        }
        Descent::Heart => {
            "The hum is loud enough to feel in your teeth. At the bottom of everything, \
             something sleeps."
        }
    }
}

/// The bit of `boss` in [`ExpeditionState::bosses`].
#[must_use]
pub const fn boss_bit(boss: DescentBoss) -> u16 {
    1 << (boss as u16)
}

/// The bit of `descent` in [`ExpeditionState::descents`].
#[must_use]
pub const fn descent_bit(descent: Descent) -> u8 {
    1 << (descent as u8)
}

/// One Descent's line on the progress page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescentRow {
    pub descent: Descent,
    pub reached: bool,
    /// The Descent's gate boss or set piece.
    pub boss: DescentBoss,
    pub boss_beaten: bool,
    /// Expedition logs read in this Descent, and how many it holds.
    pub logs_read: usize,
    pub logs_total: usize,
    /// The chapter's title once every log of it is read.
    pub chapter: Option<&'static str>,
}

/// The gate boss of `descent` (every Descent ends at one).
#[must_use]
pub const fn gate_boss(descent: Descent) -> DescentBoss {
    DescentBoss::ALL[descent as usize]
}

/// The progress page's rows for a crew with `state`, top to bottom.
#[must_use]
pub fn descent_rows(state: &ExpeditionState) -> Vec<DescentRow> {
    Descent::ALL
        .into_iter()
        .map(|descent| {
            let (logs_read, logs_total) = state.chapter_progress(descent);
            let boss = gate_boss(descent);
            DescentRow {
                descent,
                reached: state.reached(descent),
                boss,
                boss_beaten: state.beat(boss),
                logs_read,
                logs_total,
                chapter: super::chapter(descent)
                    .filter(|_| logs_total > 0 && logs_read == logs_total)
                    .map(|chapter| chapter.title),
            }
        })
        .collect()
}

/// The page's totals: `(descents reached, bosses beaten, logs read, logs in all)`.
#[must_use]
pub fn totals(state: &ExpeditionState) -> (u32, u32, usize, usize) {
    (
        state.descents.count_ones(),
        (state.bosses & 0x00FF).count_ones(),
        usize::from(state.found).min(LOGS.len()),
        LOGS.len(),
    )
}
