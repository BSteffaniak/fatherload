//! The expedition codex (Wave 9): the other voices of the deep, beside the Fathers' numbered
//! journal.
//!
//! Two pages the expedition journal shows after its numbered logs, so nothing the crew reads
//! is lost to a card that came and went:
//!
//! * **A hired hand's diary** ([`CodexPage::Diary`]): the Crust's strongbox entries
//!   ([`crate::crust::CRUST_LOGS`]), written by a hand hired on with the Fathers' survey in
//!   their own voice. Read as the crew opens the strongboxes, in any order
//!   ([`crate::crust::CrustState::logs`]).
//! * **Builder tablets** ([`CodexPage::Tablets`]): the Old Deep's tablets
//!   ([`BUILDER_TABLETS`], read from [`crate::old_deep::OldDeep::logs_read`]), the Star
//!   Core's revelation, and the inscriptions over the deeper anomaly rooms' glyphs
//!   ([`super::anomaly::AnomalyKind::inscription`], copied when the crew first enters a room
//!   of that kind).
//!
//! Everything is read from state the world already saves and shares with every client (the
//! `Crust`, `OldDeep` and `Expedition` sections), so the codex needs no state of its own and
//! every seat, partner and reloaded save shows the same pages.

use crate::{crust::CrustState, old_deep::OldDeep};

use super::{ExpeditionState, anomaly::AnomalyKind};

/// A page of the codex.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodexPage {
    /// The Crust's strongbox diary.
    Diary,
    /// The builders' own words: tablets, the core's revelation and room inscriptions.
    Tablets,
}

impl CodexPage {
    pub const ALL: [Self; 2] = [Self::Diary, Self::Tablets];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Diary => "A Hired Hand's Diary",
            Self::Tablets => "Builder Tablets",
        }
    }

    /// The line under the page's title.
    #[must_use]
    pub const fn subtitle(self) -> &'static str {
        match self {
            Self::Diary => "Strongbox pages left by a hand on the Fathers' survey, Descent I.",
            Self::Tablets => "The builders' own words, cut in stone and over the rooms' glyphs.",
        }
    }
}

/// One entry of the codex.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodexEntry {
    pub page: CodexPage,
    pub title: &'static str,
    /// Where it was found (a stratum, a tablet wall, a room).
    pub place: &'static str,
    pub text: &'static str,
    /// Whether the crew has found it (unfound entries show only as a count).
    pub found: bool,
}

/// The Old Deep's builder tablets, by `old_deep::LOG_TABLET_FIRST + index`: `(title, text)`.
pub const BUILDER_TABLETS: [(&str, &str); 4] = [
    (
        "The anchor",
        "WE SET THE ANCHOR IN THE FIRST FIRE AND HUNG THE CABLE FROM IT.",
    ),
    (
        "The dock",
        "THE STONE ABOVE IS A DOCK. THE HEART BELOW IS AN ENGINE. BETWEEN THEM, THE CABLE.",
    ),
    (
        "The lock",
        "WHEN THE FIRST CORE WAKES, DO NOT STOP. IT IS ONLY THE LOCK.",
    ),
    (
        "The sleepers",
        "WE WENT DOWN TO SLEEP WITH THE HEART. WAKE US GENTLY.",
    ),
];

/// The Star Core's revelation (`old_deep::LOG_CORE_REVEAL`): `(title, text)`.
pub const CORE_REVELATION: (&str, &str) = (
    "The first core was a lock",
    "The Star Core goes dark. Under it the shaft runs on, down past the light, ringed in \
     builder metal. It was never the bottom. The Pressure Hull seals around your rig.",
);

/// Where each strongbox of the diary is found, by [`crate::crust::CRUST_LOGS`] index.
const DIARY_PLACES: [&str; 6] = [
    "Topsoil, the lost miner's cabin",
    "Clay Beds",
    "Grey Stone, the old mine works",
    "Slate Shelves",
    "Basalt Columns, the lava tubes",
    "The Sinkhole",
];

/// The diary page for a crew whose Crust state is `crust`, in the diary's own order.
#[must_use]
pub fn diary(crust: &CrustState) -> Vec<CodexEntry> {
    crate::crust::CRUST_LOGS
        .iter()
        .enumerate()
        .map(|(index, (title, text))| CodexEntry {
            page: CodexPage::Diary,
            title,
            place: DIARY_PLACES[index.min(DIARY_PLACES.len() - 1)],
            text,
            found: u8::try_from(index).is_ok_and(|index| crust.logs.contains(&index)),
        })
        .collect()
}

/// The tablets page, in the order a dig meets its entries.
///
/// The four tablets and the core's revelation are read from the Old Deep state `deep`, the
/// deeper rooms' inscriptions from the journal `expedition`.
#[must_use]
pub fn tablets(deep: &OldDeep, expedition: &ExpeditionState) -> Vec<CodexEntry> {
    use crate::old_deep::{LOG_CORE_REVEAL, LOG_TABLET_FIRST};
    let mut entries: Vec<CodexEntry> = BUILDER_TABLETS
        .iter()
        .enumerate()
        .map(|(index, (title, text))| CodexEntry {
            page: CodexPage::Tablets,
            title,
            place: "Ancient Machine Layer, a tablet wall",
            text,
            found: u8::try_from(index)
                .is_ok_and(|index| deep.logs_read.contains(&(LOG_TABLET_FIRST + index))),
        })
        .collect();
    entries.push(CodexEntry {
        page: CodexPage::Tablets,
        title: CORE_REVELATION.0,
        place: "Mantle Storm Zone, the Star Core",
        text: CORE_REVELATION.1,
        found: deep.logs_read.contains(&LOG_CORE_REVEAL),
    });
    entries.extend(AnomalyKind::ALL.iter().filter_map(|kind| {
        Some(CodexEntry {
            page: CodexPage::Tablets,
            title: kind.name(),
            place: "Over an anomaly room's glyph",
            text: kind.inscription()?,
            found: expedition.anomaly_kinds & kind.bit() != 0,
        })
    }));
    entries
}

/// Every entry of `page`.
#[must_use]
pub fn page(
    page: CodexPage,
    crust: &CrustState,
    deep: &OldDeep,
    expedition: &ExpeditionState,
) -> Vec<CodexEntry> {
    match page {
        CodexPage::Diary => diary(crust),
        CodexPage::Tablets => tablets(deep, expedition),
    }
}

impl crate::WorldState {
    /// The codex's `page` as the crew has found it (Wave 9; see [`super::codex`]).
    #[must_use]
    pub fn expedition_codex(&self, page: CodexPage) -> Vec<CodexEntry> {
        self::page(page, self.crust(), self.old_deep(), self.expedition())
    }
}
