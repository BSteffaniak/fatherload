//! The Fathers' expedition (expansion plan Part III §13, "following the builders' cable", and
//! §14's cross-Descent systems): the story backbone of the Deep Descent, plus the anomaly rooms.
//!
//! A generation ago six people from the town went down the builders' cable to find out what
//! hums: Amos Varga (lead driller), Ines Calloway (surveyor), Dov Halloran (radio), Ruth
//! "Bran" Brannigan (medic and cook), Teo Marsh (mechanic) and Sefa Rhee (naturalist). The town
//! called them the Fathers; Bran painted THE FATHERS' LOAD on their sled. Nobody came back up.
//!
//! * **Logs** ([`logs::LOGS`]): the entries they left in cipher canisters at their camps,
//!   wrecks and cairns, one site per log beside the cable ([`sites`]), each deeper than the
//!   one before. Ines keyed every canister to a word of the one before, so the trail is read
//!   **in order**: the crew's read logs are always a prefix (`found` logs, 1..=found). A rig
//!   that reaches a later canister first sees it sealed; it opens by itself the moment the one
//!   before is read ([`ExpeditionState::sealed`]).
//! * **Echoes** ([`logs::ECHOES`]): Dov's radio echoes (while his drop line reached the town)
//!   and ghost recordings (his recorder, left on the cable after the storm cut the line). Each
//!   plays once to the whole crew when a rig first reaches its depth.
//! * **Memories** ([`memories`]): stable-id lines for the people of Gravel Hollow, the Old Deep
//!   camp and the Hive, opening as the crew reads the trail.
//! * **Anomaly rooms** ([`anomaly`]): rare rooms at any depth where a deeper chapter's rule
//!   leaks through (gravity flipped, pixel rot, time stopped, mirrored; from Descent IV down
//!   also sideways, sunlit, weightless and bent), each with a small reward.
//! * **The codex** ([`codex`], Wave 9): the Crust's strongbox diary and the builders' tablets
//!   and inscriptions, as pages of the journal beside the numbered logs.
//!
//! The crew shares one [`ExpeditionState`] (`WorldState::expedition`, the shared
//! `SectionKey::Expedition` section, saved with the world and hashed only once something was
//! found), so co-op partners read the same journal. Only Deep Descent worlds (generator V11)
//! place sites, echoes and rooms; every older world keeps an empty state.

pub mod anomaly;
pub mod codex;
pub mod logs;
pub mod memories;
pub mod sites;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub use logs::{CHAPTERS, Chapter, ECHOES, Echo, EchoKind, ExpeditionLog, LOGS, chapter};

use crate::descent::Descent;

/// A member of the Fathers' expedition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Member {
    Amos,
    Ines,
    Dov,
    Bran,
    Teo,
    Sefa,
}

impl Member {
    pub const ALL: [Self; 6] = [
        Self::Amos,
        Self::Ines,
        Self::Dov,
        Self::Bran,
        Self::Teo,
        Self::Sefa,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Amos => "Amos Varga",
            Self::Ines => "Ines Calloway",
            Self::Dov => "Dov Halloran",
            Self::Bran => "Ruth Brannigan",
            Self::Teo => "Teo Marsh",
            Self::Sefa => "Sefa Rhee",
        }
    }

    /// What they did on the expedition.
    #[must_use]
    pub const fn role(self) -> &'static str {
        match self {
            Self::Amos => "lead driller",
            Self::Ines => "surveyor",
            Self::Dov => "radio operator",
            Self::Bran => "medic and cook",
            Self::Teo => "mechanic",
            Self::Sefa => "naturalist",
        }
    }

    /// Pitch of the member's voice in the ghost recordings' procedural cue (0 low .. 255 high).
    #[must_use]
    pub const fn voice(self) -> u8 {
        match self {
            Self::Amos => 40,
            Self::Dov => 70,
            Self::Sefa => 120,
            Self::Ines => 150,
            Self::Bran => 110,
            Self::Teo => 190,
        }
    }
}

/// What a log was left at.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SiteKind {
    /// A night's camp: a tent, a lantern, bedrolls and the canister on a crate.
    Camp,
    /// An abandoned rig: the expedition's, or an older one they found.
    Rig,
    /// A cairn with a flag and a chalk arrow pointing down.
    Marker,
    /// A grave (Wave 9): a mound under a cut headstone, the canister left at its foot. Dov
    /// Halloran's, in the inner sun's light.
    Grave,
}

impl SiteKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Camp => "camp",
            Self::Rig => "abandoned rig",
            Self::Marker => "cairn",
            Self::Grave => "grave",
        }
    }
}

/// Credits a canister holds for whoever reads it (the crew's stash), by its Descent.
#[must_use]
pub const fn log_bounty(descent: Descent) -> u32 {
    match descent {
        Descent::Crust => 80,
        Descent::OldDeep => 240,
        Descent::LivingDark => 520,
        Descent::DrownedAndFrozen => 900,
        Descent::HollowWorld => 1_400,
        Descent::StrangeLayers => 2_000,
        Descent::Ancients => 2_800,
        Descent::Heart => 3_600,
    }
}

/// Seconds between two echoes (so one card finishes before the next plays).
pub const ECHO_GAP_MILLIS: u64 = 12_000;

/// What the crew has found of the expedition (shared by everyone in the world).
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct ExpeditionState {
    /// Logs read: logs `1..=found` (always a prefix of [`LOGS`]).
    pub found: u8,
    /// Logs (1-based) whose canister a rig reached before the one before it was read: sealed
    /// until then, read at once after.
    pub sealed: BTreeSet<u8>,
    /// Echoes played: echoes `1..=echoes` of [`ECHOES`].
    pub echoes: u8,
    /// The simulation tick before which no further echo plays.
    pub next_echo_tick: u64,
    /// Anomaly rooms entered, by region.
    pub anomalies: BTreeSet<(i32, i32)>,
    /// Kinds of anomaly room entered ([`anomaly::AnomalyKind::bit`]).
    pub anomaly_kinds: u8,
}

impl ExpeditionState {
    /// Whether nothing was found yet (the state is then left out of the state hash, so every
    /// older world's hashes are unchanged).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The log entries read, in order.
    pub fn read_logs(&self) -> impl Iterator<Item = &'static ExpeditionLog> {
        LOGS.iter().take(usize::from(self.found))
    }

    /// The echoes heard, in order.
    pub fn heard_echoes(&self) -> impl Iterator<Item = &'static Echo> {
        ECHOES.iter().take(usize::from(self.echoes))
    }

    /// Whether log `number` (1-based) has been read.
    #[must_use]
    pub const fn has_read(&self, number: u8) -> bool {
        number >= 1 && number <= self.found
    }

    /// Logs read of `descent`'s chapter, and how many it has.
    #[must_use]
    pub fn chapter_progress(&self, descent: Descent) -> (usize, usize) {
        let total = LOGS.iter().filter(|log| log.descent() == descent).count();
        let read = self
            .read_logs()
            .filter(|log| log.descent() == descent)
            .count();
        (read, total)
    }

    /// Whether every log of `descent`'s chapter has been read (false for a chapter without
    /// logs).
    #[must_use]
    pub fn chapter_read(&self, descent: Descent) -> bool {
        let (read, total) = self.chapter_progress(descent);
        total > 0 && read == total
    }

    /// How many kinds of anomaly room the crew has entered.
    #[must_use]
    pub const fn anomaly_kind_count(&self) -> u32 {
        self.anomaly_kinds.count_ones()
    }
}

#[cfg(test)]
mod tests;
