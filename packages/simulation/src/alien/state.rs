//! What players did with the species (Wave 12 aliens track).
//!
//! The world's [`AlienState`] is saved with the world and replicated as the shared
//! [`crate::world::SectionKey::Aliens`] section; the [`AlienCommand`]s and [`AlienEvent`]s are
//! the rules' in `world/aliens.rs`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    entity::EntityId,
    home::Home,
    ids::PlayerId,
    people::{CharacterId, VillageId},
    place::SpaceBodyKey,
    space::SystemRef,
    terrain::MineralKind,
};

use super::{AlienSpecies, language::Understanding, trade::AlienGood};

/// An action with the species ([`crate::PlayerCommand::Alien`]). Each names the resident it is
/// done with (the actor must reach them, as for every people command) or the home it is done
/// at.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AlienCommand {
    /// Sell `count` of `mineral` from the bay to the town's trader `npc`.
    Sell {
        npc: EntityId,
        mineral: MineralKind,
        count: u32,
    },
    /// Buy `good` from the town's trader `npc`.
    Buy { npc: EntityId, good: AlienGood },
    /// A lesson in the tongue from the town's lexicon keeper `npc` (one a day per species).
    Lesson { npc: EntityId },
    /// Ask the elder `npc` what the town needs (the next chapter of the species' arc).
    AskChapter { npc: EntityId },
    /// Take on the chapter the elder `npc` asked for.
    AcceptChapter { npc: EntityId },
    /// Tell the elder `npc` the chapter is done (hands over what it asked for).
    TurnInChapter { npc: EntityId },
    /// Give up the running chapter of `species` (anywhere).
    AbandonChapter { species: AlienSpecies },
    /// Buy home plot `plot` (its building index) from the town's builder `npc`; the builder
    /// raises a home on it in the species' style.
    BuyHome { npc: EntityId, plot: u8 },
    /// Rest at your own home in town `town` (plot `plot`): air topped up, hull patched.
    RestAtHome { town: VillageId, plot: u8 },
}

/// What happened with the species (netcode `940 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum AlienEvent {
    /// The player met the species for the first time (`detail`: the town's id).
    FirstContact,
    /// Their understanding reached a new level (`detail`: the [`super::Fluency`] index).
    FluencyRose,
    /// They learned words (`detail`: understanding gained).
    Learned,
    /// They sold ore (`detail`: credits paid).
    Sold,
    /// They bought a good (`detail`: credits paid).
    Bought,
    /// A lesson at the lexicon house (`detail`: credits paid).
    Lesson,
    /// A lexicon stone dug out.
    LexiconFound,
    /// The elder asked for a chapter (`detail`: the chapter).
    ChapterOffered,
    /// The chapter was taken on (`detail`: the chapter).
    ChapterAccepted,
    /// The chapter's goal was reached (`detail`: the chapter).
    ChapterReached,
    /// The chapter was finished (`detail`: the chapter).
    ChapterCompleted,
    /// A traveller climbed aboard (`detail`: the chapter).
    PassengerAboard,
    /// The traveller arrived (`detail`: the chapter).
    PassengerDelivered,
    /// A town's monument was raised (`detail`: the town's id).
    MonumentRaised,
    /// A home plot was bought (`detail`: credits paid).
    HomeBought,
    /// A home was finished (`detail`: the plot).
    HomeBuilt,
    /// The builders' trail: a lead charted (`detail`: the lead's index).
    TrailLead,
    /// Refused (`detail`: an [`AlienRefusal`] code).
    Refused,
    /// Rested at home.
    Rested,
}

impl AlienEvent {
    /// Every event, in code order.
    pub const ALL: [Self; 19] = [
        Self::FirstContact,
        Self::FluencyRose,
        Self::Learned,
        Self::Sold,
        Self::Bought,
        Self::Lesson,
        Self::LexiconFound,
        Self::ChapterOffered,
        Self::ChapterAccepted,
        Self::ChapterReached,
        Self::ChapterCompleted,
        Self::PassengerAboard,
        Self::PassengerDelivered,
        Self::MonumentRaised,
        Self::HomeBought,
        Self::HomeBuilt,
        Self::TrailLead,
        Self::Refused,
        Self::Rested,
    ];

    /// Its code (netcode adds 940).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::FirstContact => 0,
            Self::FluencyRose => 1,
            Self::Learned => 2,
            Self::Sold => 3,
            Self::Bought => 4,
            Self::Lesson => 5,
            Self::LexiconFound => 6,
            Self::ChapterOffered => 7,
            Self::ChapterAccepted => 8,
            Self::ChapterReached => 9,
            Self::ChapterCompleted => 10,
            Self::PassengerAboard => 11,
            Self::PassengerDelivered => 12,
            Self::MonumentRaised => 13,
            Self::HomeBought => 14,
            Self::HomeBuilt => 15,
            Self::TrailLead => 16,
            Self::Refused => 17,
            Self::Rested => 18,
        }
    }
}

/// Why an alien action was refused (the [`AlienEvent::Refused`] detail).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum AlienRefusal {
    /// The person is not in reach, or is not one of the species.
    NotInReach,
    /// They do not keep that trade (only the trader trades, the keeper teaches, ...).
    NotTheirTrade,
    /// They do not buy that ore.
    DoNotBuy,
    /// The bay holds less than that.
    NotEnoughCargo,
    /// Not enough credits.
    NotEnoughCredits,
    /// Already had today's lesson.
    LessonToday,
    /// The elder waits until the player understands more.
    NeedsUnderstanding,
    /// A chapter is already under way (or none was asked for).
    ChapterState,
    /// The chapter's goal is not reached yet.
    NotDone,
    /// The whole arc is told.
    ArcFinished,
    /// The plot is not for sale (taken, or not a plot).
    PlotTaken,
    /// No home of the player's there.
    NoHome,
    /// The home is still being built.
    StillBuilding,
}

impl AlienRefusal {
    /// Its code.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::NotInReach => 0,
            Self::NotTheirTrade => 1,
            Self::DoNotBuy => 2,
            Self::NotEnoughCargo => 3,
            Self::NotEnoughCredits => 4,
            Self::LessonToday => 5,
            Self::NeedsUnderstanding => 6,
            Self::ChapterState => 7,
            Self::NotDone => 8,
            Self::ArcFinished => 9,
            Self::PlotTaken => 10,
            Self::NoHome => 11,
            Self::StillBuilding => 12,
        }
    }

    /// The refusal of code `code`.
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        Some(match code {
            0 => Self::NotInReach,
            1 => Self::NotTheirTrade,
            2 => Self::DoNotBuy,
            3 => Self::NotEnoughCargo,
            4 => Self::NotEnoughCredits,
            5 => Self::LessonToday,
            6 => Self::NeedsUnderstanding,
            7 => Self::ChapterState,
            8 => Self::NotDone,
            9 => Self::ArcFinished,
            10 => Self::PlotTaken,
            11 => Self::NoHome,
            12 => Self::StillBuilding,
            _ => return None,
        })
    }

    /// What the HUD says.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotInReach => "They are out of reach.",
            Self::NotTheirTrade => "That is not their trade: find the right keeper.",
            Self::DoNotBuy => "They have no use for that ore.",
            Self::NotEnoughCargo => "The bay does not hold enough of it.",
            Self::NotEnoughCredits => "Not enough credits.",
            Self::LessonToday => "One lesson a day: come back tomorrow.",
            Self::NeedsUnderstanding => {
                "The elder waits until you understand more of their tongue."
            }
            Self::ChapterState => "Finish what you promised first.",
            Self::NotDone => "It is not done yet.",
            Self::ArcFinished => "They have told you everything.",
            Self::PlotTaken => "That plot is not for sale.",
            Self::NoHome => "You have no home here.",
            Self::StillBuilding => "Your home is still being built.",
        }
    }
}

/// A chapter under way.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActiveChapter {
    pub chapter: u8,
    /// The town whose elder gave it.
    pub town: VillageId,
    /// The body the town is on.
    pub body: SpaceBodyKey,
    /// Whether its goal is reached (explore and escort chapters; fetch and build are checked
    /// at the elder).
    #[serde(default)]
    pub reached: bool,
    /// The traveller riding along (escort chapters).
    #[serde(default)]
    pub passenger: Option<CharacterId>,
    /// Where the traveller goes (escort chapters: the next town; `None` with a passenger: the
    /// far side of the world).
    #[serde(default)]
    pub destination: Option<VillageId>,
}

/// One player's dealings with one species.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpeciesLog {
    /// What they understand of the tongue (per mille).
    #[serde(default)]
    pub understanding: Understanding,
    /// World time of first contact (`None`: never met).
    #[serde(default)]
    pub met_millis: Option<u64>,
    /// The day the residents in [`Self::chatted`] were talked to.
    #[serde(default)]
    pub chat_day: u32,
    /// Residents talked to on [`Self::chat_day`] (each teaches once a day).
    #[serde(default)]
    pub chatted: BTreeSet<CharacterId>,
    /// The day of the last trade that taught words, per town.
    #[serde(default, with = "crate::pair_list")]
    pub traded_day: BTreeMap<VillageId, u32>,
    /// The day of the last lesson.
    #[serde(default)]
    pub lesson_day: Option<u32>,
    /// Lexicon stones dug out.
    #[serde(default)]
    pub stones: u16,
    /// Chapters finished (the next one is offered).
    #[serde(default)]
    pub chapters_done: u8,
    /// The chapter the elder asked for and the player has not yet taken on.
    #[serde(default)]
    pub offered: Option<u8>,
    /// The chapter under way.
    #[serde(default)]
    pub active: Option<ActiveChapter>,
}

impl SpeciesLog {
    /// Whether the player has met them.
    #[must_use]
    pub const fn met(&self) -> bool {
        self.met_millis.is_some()
    }
}

/// One player's dealings with every species.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlienLog {
    #[serde(default, with = "crate::pair_list")]
    pub species: BTreeMap<AlienSpecies, SpeciesLog>,
}

impl AlienLog {
    /// The log of `species` (empty if never met).
    #[must_use]
    pub fn of(&self, species: AlienSpecies) -> SpeciesLog {
        self.species.get(&species).cloned().unwrap_or_default()
    }
}

/// A home in an alien town.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlienHome {
    pub owner: PlayerId,
    pub town: VillageId,
    pub body: SpaceBodyKey,
    /// The plot's building index in the town.
    pub plot: u8,
    pub species: AlienSpecies,
    /// The builder (the town's builder).
    pub builder: CharacterId,
    /// Credits paid.
    pub paid: u32,
    pub bought_millis: u64,
    /// World time the builder hands it over.
    pub ready_millis: u64,
    /// The finished home (rooms, decor, storage), once handed over.
    #[serde(default)]
    pub home: Option<Home>,
}

impl AlienHome {
    /// How far the build is (per mille) at world time `now`.
    #[must_use]
    pub fn progress_per_mille(&self, now: u64) -> u32 {
        if self.home.is_some() || now >= self.ready_millis {
            return 1_000;
        }
        let span = self.ready_millis.saturating_sub(self.bought_millis).max(1);
        let done = now.saturating_sub(self.bought_millis);
        u32::try_from(done * 1_000 / span).unwrap_or(1_000)
    }
}

/// A lead on the builders' trail, given by a species' elder (the hook for Wave 13's galaxy
/// story).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrailLead {
    pub species: AlienSpecies,
    /// The system the builders went toward.
    pub system: SystemRef,
    /// Who heard it.
    pub player: PlayerId,
    /// World time it was told.
    pub told_millis: u64,
}

/// Everything players did with the species (one shared section).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlienState {
    /// Each player's understanding, contact and chapters.
    #[serde(default, with = "crate::pair_list")]
    pub players: BTreeMap<PlayerId, AlienLog>,
    /// Homes bought in alien towns.
    #[serde(default)]
    pub homes: Vec<AlienHome>,
    /// Town monuments raised (by town id).
    #[serde(default)]
    pub monuments: BTreeSet<VillageId>,
    /// The builders' trail leads given.
    #[serde(default)]
    pub trail_leads: Vec<TrailLead>,
}

impl AlienState {
    /// Whether nothing ever happened (the state hash skips it).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
            && self.homes.is_empty()
            && self.monuments.is_empty()
            && self.trail_leads.is_empty()
    }

    /// `player`'s log of `species`.
    #[must_use]
    pub fn log(&self, player: PlayerId, species: AlienSpecies) -> SpeciesLog {
        self.players
            .get(&player)
            .map(|log| log.of(species))
            .unwrap_or_default()
    }

    /// `player`'s understanding of `species`' tongue.
    #[must_use]
    pub fn understanding(&self, player: PlayerId, species: AlienSpecies) -> Understanding {
        self.log(player, species).understanding
    }

    /// The mutable log of `player` and `species`.
    pub fn log_mut(&mut self, player: PlayerId, species: AlienSpecies) -> &mut SpeciesLog {
        self.players
            .entry(player)
            .or_default()
            .species
            .entry(species)
            .or_default()
    }

    /// The home on plot `plot` of town `town`, if one was bought.
    #[must_use]
    pub fn home_at(&self, town: VillageId, plot: u8) -> Option<&AlienHome> {
        self.homes
            .iter()
            .find(|home| home.town == town && home.plot == plot)
    }

    /// `player`'s homes.
    pub fn homes_of(&self, player: PlayerId) -> impl Iterator<Item = &AlienHome> + '_ {
        self.homes.iter().filter(move |home| home.owner == player)
    }

    /// Forgets a player who left the world for good (their homes stay: deeds outlive visits).
    pub fn forget(&mut self, player: PlayerId) {
        self.players.remove(&player);
    }
}
