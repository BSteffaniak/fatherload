//! Friendly alien species, their towns, languages, trade, quests and homes (Part III §16
//! WP-SPACE "Aliens: friendly species", Wave 12 aliens track).
//!
//! Three species live out among the stars, each with its own body plan, culture, voice,
//! building style and tastes ([`species`]):
//!
//! * the **Vessi** ([`AlienSpecies::Vessi`]): tall, four-legged, slow singers of the ice
//!   worlds, who keep time in songs and live in spiral snow-vaults lit from inside;
//! * the **Tokkari** ([`AlienSpecies::Tokkari`]): stout six-limbed burrowing smiths of the rocky
//!   worlds, who keep their debts in knotted cords and build terraced mound-halls of fused
//!   stone round glowing forges;
//! * the **Siphra** ([`AlienSpecies::Siphra`]): drifting, glass-bodied memory keepers of the
//!   airless moons, who feed on starlight, hang their homes from crystal pylons, and remember
//!   the builders' ships passing.
//!
//! Everything here is a pure function of the world seed and space ids, so every peer agrees
//! without anything being saved or sent but what players did:
//!
//! * **Where they live** ([`atlas`]): every body's folk come from the space body (its theme,
//!   its kind, a hashed roll) and are written into its [`crate::planet::PlanetParams::folk`],
//!   so the planet generator flattens the towns' ground and buries their lexicon stones. Each
//!   species has a guaranteed **first home** near the home system, so first contact comes
//!   early in the galaxy.
//! * **Towns** ([`town`]): sites spread round the body's ring, each a span of level ground with
//!   the species' own buildings (dwellings, a trade hall, the elder's house, the lexicon house,
//!   a landmark, workshops and homes for sale) and its residents, generated with the people
//!   engine ([`crate::people`]) in the species' culture. A town is a
//!   [`crate::people::routine::Settlement`], so residents keep schedules, walk to work, talk,
//!   take gifts and remember players like every villager.
//! * **Languages** ([`language`]): what a player understands of a species grows with talking,
//!   trading, lessons at the lexicon house, lexicon stones dug from under the towns and
//!   finished quests; speech renders as glyphs until the words are learned.
//! * **Trade** ([`trade`]): each species buys the ores it values and sells its own goods.
//! * **Quests** ([`quest`]): a story arc of chapters per species (first contact, a fetch, an
//!   exploration, an escort, a raising, the builders' trail) ending in a lead for Wave 13's
//!   galaxy story.
//! * **State** ([`state`]): per player understanding, contact, quests and homes; the shared
//!   [`crate::world::SectionKey::Aliens`] section.
//!
//! The world's rules are in `world/aliens.rs`.

pub mod atlas;
pub mod language;
pub mod quest;
pub mod species;
pub mod state;
pub mod talk;
#[cfg(test)]
mod tests;
pub mod town;
pub mod trade;

use serde::{Deserialize, Serialize};

pub use atlas::{first_home, folk_of_body};
pub use language::{Fluency, SpeechToken, speech_tokens};
pub use species::{SPECIES, SpeciesDef, species};
pub use state::{AlienCommand, AlienEvent, AlienRefusal, AlienState};
pub use town::{AlienBuildingKind, AlienTown, is_alien_town, town_by_id, towns_of};
pub use trade::AlienGood;

/// Kind-code range of [`crate::terrain::TileKind::Alien`] tiles in
/// [`crate::terrain::Tile::code`] (`0x2800 | AlienTile::code`): the aliens track's range.
pub const ALIEN_TILE_CODE_BASE: u64 = 0x2800;

/// A friendly species (append-only; the index is saved and replicated).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum AlienSpecies {
    /// The ice-world singers.
    #[default]
    Vessi,
    /// The rocky-world smiths.
    Tokkari,
    /// The airless-moon memory keepers.
    Siphra,
}

impl AlienSpecies {
    /// Every species, in index order.
    pub const ALL: [Self; 3] = [Self::Vessi, Self::Tokkari, Self::Siphra];

    /// Its index (what [`crate::planet::PlanetParams::folk`] stores, minus one).
    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::Vessi => 0,
            Self::Tokkari => 1,
            Self::Siphra => 2,
        }
    }

    /// The species of index `index`.
    #[must_use]
    pub const fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::Vessi),
            1 => Some(Self::Tokkari),
            2 => Some(Self::Siphra),
            _ => None,
        }
    }

    /// The species a planet's `folk` byte names (`0`: nobody).
    #[must_use]
    pub const fn from_folk(folk: u8) -> Option<Self> {
        if folk == 0 {
            None
        } else {
            Self::from_index(folk - 1)
        }
    }

    /// The `folk` byte for this species.
    #[must_use]
    pub const fn folk(self) -> u8 {
        self.index() + 1
    }

    /// Its definition.
    #[must_use]
    pub const fn def(self) -> &'static SpeciesDef {
        species(self)
    }

    /// The people engine's culture of its folk (their names).
    #[must_use]
    pub const fn culture(self) -> crate::people::Culture {
        match self {
            Self::Vessi => crate::people::Culture::Vessi,
            Self::Tokkari => crate::people::Culture::Tokkari,
            Self::Siphra => crate::people::Culture::Siphra,
        }
    }

    /// The species whose folk carry `culture`, if it is an alien one.
    #[must_use]
    pub const fn of_culture(culture: crate::people::Culture) -> Option<Self> {
        match culture {
            crate::people::Culture::Vessi => Some(Self::Vessi),
            crate::people::Culture::Tokkari => Some(Self::Tokkari),
            crate::people::Culture::Siphra => Some(Self::Siphra),
            _ => None,
        }
    }
}

/// A tile of an alien town (the aliens track's [`crate::terrain::TileKind::Alien`] family):
/// each species' paving under its towns, and the lexicon stones buried under them.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum AlienTile {
    /// Packed blue ice slabs (the Vessi's streets).
    VessiPaving,
    /// Fused mound stone (the Tokkari's streets).
    TokkariPaving,
    /// Pale starlit glass (the Siphra's streets).
    SiphraPaving,
    /// A buried tablet of a species' writing: drilled out, it teaches its words.
    LexiconStone(AlienSpecies),
}

impl AlienTile {
    /// Every tile, in code order.
    pub const ALL: [Self; 6] = [
        Self::VessiPaving,
        Self::TokkariPaving,
        Self::SiphraPaving,
        Self::LexiconStone(AlienSpecies::Vessi),
        Self::LexiconStone(AlienSpecies::Tokkari),
        Self::LexiconStone(AlienSpecies::Siphra),
    ];

    /// The paving of `species`' towns.
    #[must_use]
    pub const fn paving(species: AlienSpecies) -> Self {
        match species {
            AlienSpecies::Vessi => Self::VessiPaving,
            AlienSpecies::Tokkari => Self::TokkariPaving,
            AlienSpecies::Siphra => Self::SiphraPaving,
        }
    }

    /// Its code within the family.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::VessiPaving => 0,
            Self::TokkariPaving => 1,
            Self::SiphraPaving => 2,
            Self::LexiconStone(species) => 3 + species.index() as u64,
        }
    }

    /// The tile of code `code`.
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::VessiPaving),
            1 => Some(Self::TokkariPaving),
            2 => Some(Self::SiphraPaving),
            3 => Some(Self::LexiconStone(AlienSpecies::Vessi)),
            4 => Some(Self::LexiconStone(AlienSpecies::Tokkari)),
            5 => match AlienSpecies::from_index(2) {
                Some(species) => Some(Self::LexiconStone(species)),
                None => None,
            },
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::VessiPaving => "vault ice",
            Self::TokkariPaving => "mound stone",
            Self::SiphraPaving => "starglass",
            Self::LexiconStone(_) => "lexicon stone",
        }
    }

    /// Drill strength needed.
    #[must_use]
    pub const fn hardness(self) -> u8 {
        match self {
            Self::VessiPaving | Self::SiphraPaving => 2,
            Self::TokkariPaving => 3,
            // A tablet any drill opens: the find is for everyone.
            Self::LexiconStone(_) => 1,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    pub const fn durability(self) -> u8 {
        match self {
            Self::VessiPaving => 4,
            Self::SiphraPaving => 5,
            Self::TokkariPaving => 7,
            Self::LexiconStone(_) => 3,
        }
    }

    /// Seconds per chip at the base drill.
    #[must_use]
    pub const fn base_chip_seconds(self) -> f32 {
        match self {
            Self::TokkariPaving => 0.18,
            Self::VessiPaving | Self::SiphraPaving => 0.14,
            Self::LexiconStone(_) => 0.12,
        }
    }

    /// Whether it is a lexicon stone, and whose.
    #[must_use]
    pub const fn lexicon(self) -> Option<AlienSpecies> {
        match self {
            Self::LexiconStone(species) => Some(species),
            _ => None,
        }
    }
}
