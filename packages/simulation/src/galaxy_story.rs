//! The galaxy story (expansion plan §16 "Story beyond the Moon", Wave 13 story): the builders'
//! trail through the stars, and endless exploration after it.
//!
//! The builders left by the Star Drive, and the Archive's star charts are their roads. Once
//! the Heart wakes and space opens, the crew follows the road from the Archive's first chart
//! ([`plan::anchor_of`]) through ten chapters ([`chapters::CHAPTERS`]): a waymark, the peoples
//! of the near stars, a foundry on a machine world, a quiet world's archive, the Concord's
//! captain, the singing spires, the nursery beacon (the revelation: the town's people are the
//! builders' children, the Load, and the Fathers were named for those who stayed to raise
//! them), the lamp-key, the long dark, and the Harbour far from home, where the Warden waits
//! and the crew chooses ([`finale`]). The credits roll and the game keeps going: every system
//! holds procedural leads ([`discovery`]) that keep giving reasons to visit.
//!
//! # A quest engine for space
//!
//! * A chapter is a list of [`chapters::Goal`]s done **in order** by anyone in the crew (go to
//!   a system, scan, land, find a relic, hold a site, meet a species, finish a species'
//!   chapter, beat a boss, reach the Harbour, calm the Warden, choose). Targets come from the
//!   [`plan::StoryPlan`], a pure function of the world seed and the anchor (saved once, at the
//!   story's start), so every peer agrees where the road goes.
//! * Progress is the crew's, per world: one [`GalaxyStory`] (the world field `galaxy_story`,
//!   the shared section `SectionKey::GalaxyStory`), saved and replicated, so co-op partners
//!   share one journal, one waypoint and one road, and players anywhere (each split-screen seat
//!   in its own place) all count.
//! * The **journal** keeps each chapter's page (intro, goals, outro), the space logs found
//!   ([`journal::SPACE_LOGS`]), the builders' fragments and the discovery log; the **nav
//!   waypoint** for the current goal is [`crate::WorldState::galaxy_waypoint`], and
//!   [`GalaxyCommand::SetCourse`] makes it the player's own (the warp's target).
//! * The **archivist** remarks on space finds ([`journal::REMARKS`], raised as
//!   [`GalaxyEvent::ArchivistRemark`]).
//!
//! The rules live in `world/galaxy_story.rs`.

pub mod chapters;
pub mod discovery;
pub mod finale;
pub mod journal;
pub mod plan;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    ids::PlayerId,
    space::{SpaceBodyId, SystemRef},
};

pub use chapters::{CHAPTER_COUNT, CHAPTERS, ChapterDef, Goal, GoalDef, chapter};
pub use discovery::{Lead, LeadKind, LeadStep, leads_of};
pub use finale::FinaleChoice;

/// Most leads the discovery log keeps.
pub const MAX_LEADS: usize = 1_024;
/// Most systems the crew's visited list keeps.
pub const MAX_VISITED: usize = 4_096;
/// Most worlds the crew's landed list keeps.
pub const MAX_LANDED: usize = 4_096;
/// How often the world-level story checks run (ms of world time).
pub const CHECK_MILLIS: u64 = 250;
/// A relic is found from this many columns either side of it.
pub const RELIC_REACH_COLUMNS: i32 = 5;
/// A relic is found from this many rows above it down to this many below its row.
pub const RELIC_REACH_ROWS: (i32, i32) = (7, 3);
/// A hold counts while someone is within this many columns of the relic.
pub const HOLD_REACH_COLUMNS: i32 = 14;
/// The archivist does not remark more often than this (ms).
pub const REMARK_GAP_MILLIS: u64 = 8_000;

/// One lead of the discovery log: a system's lead picked up, and how far it is.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LeadRecord {
    pub system: SystemRef,
    /// The lead's index in its system ([`discovery::leads_of`]).
    pub index: u8,
    /// The step under way (`steps.len()` once done).
    pub step: u8,
    pub done: bool,
    /// World time it was picked up.
    pub found_millis: u64,
    /// The rig the step's ambush is on (a distress call's survive step).
    #[serde(default)]
    pub ambushed: Option<PlayerId>,
}

/// What one player did on the road (co-op stats).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CrewStats {
    /// Story goals this player finished.
    pub goals: u16,
    /// Leads this player finished.
    pub leads: u16,
    /// Systems this player was first into.
    pub systems: u16,
}

/// The crew's galaxy story (a world field, the shared `GalaxyStory` section).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct GalaxyStory {
    /// The story began (space is open).
    pub begun: bool,
    /// The system the road starts from (the crew's first chart's, fixed at the start).
    pub anchor: Option<SystemRef>,
    /// The chapter under way (`CHAPTER_COUNT` once the road is walked).
    pub chapter: u8,
    /// The goal under way in it.
    pub step: u8,
    /// World time each chapter was finished.
    pub chapter_millis: Vec<u64>,
    /// How long the current hold has been kept (ms).
    pub held_millis: u32,
    /// World time a crew member was last seen at the hold (for its accumulation).
    pub held_seen: Option<u64>,
    /// The hold's raid has landed.
    pub raid_landed: bool,
    /// Space logs found ([`journal::SPACE_LOGS`] indices).
    pub logs: BTreeSet<u8>,
    /// Builders' fragments found ([`journal::FRAGMENTS`] indices).
    pub fragments: BTreeSet<u8>,
    /// The Warden: the rig it was called on (while a fight is on).
    pub warden_on: Option<PlayerId>,
    /// The Warden is calm (beaten).
    pub warden_calmed: bool,
    /// The finale's choice.
    pub choice: Option<FinaleChoice>,
    /// World time the choice was made (the ending plays from it).
    pub finale_millis: Option<u64>,
    /// The discovery log, in the order found.
    pub leads: Vec<LeadRecord>,
    /// Systems the crew has been in.
    pub visited: BTreeSet<SystemRef>,
    /// Worlds the crew has set down on.
    pub landed: BTreeSet<SpaceBodyId>,
    /// Each player's part.
    #[serde(with = "crate::pair_list")]
    pub crew: BTreeMap<PlayerId, CrewStats>,
    /// The archivist's remarks so far (their turns rotate).
    pub remarks: u32,
    /// World time of the last remark.
    pub last_remark_millis: u64,
    /// World time of the next world-level check.
    pub next_check_millis: u64,
}

impl GalaxyStory {
    /// Whether nothing ever happened (the state hash skips it, so older worlds hash as before).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the road is walked (every chapter done).
    #[must_use]
    pub const fn finished(&self) -> bool {
        self.chapter >= CHAPTER_COUNT
    }

    /// How many chapters are done.
    #[must_use]
    pub const fn chapters_done(&self) -> u8 {
        if self.chapter > CHAPTER_COUNT {
            CHAPTER_COUNT
        } else {
            self.chapter
        }
    }

    /// The goal under way, if the story is under way.
    #[must_use]
    pub fn current_goal(&self) -> Option<&'static GoalDef> {
        if !self.begun {
            return None;
        }
        chapter(self.chapter)?.goals.get(usize::from(self.step))
    }

    /// The lead record of `system`'s lead `index`.
    #[must_use]
    pub fn lead(&self, system: SystemRef, index: u8) -> Option<&LeadRecord> {
        self.leads
            .iter()
            .find(|record| record.system == system && record.index == index)
    }

    /// Forgets a player who left (their stats stay with the world: nothing to do).
    pub const fn forget(&mut self, _player: PlayerId) {}

    /// The crew's stats (leads by kind are [`Self::stats_of_seed`]'s: they need the seed).
    #[must_use]
    pub fn stats(&self) -> GalaxyStats {
        let count = |len: usize| u32::try_from(len).unwrap_or(u32::MAX);
        GalaxyStats {
            chapters_done: self.chapters_done(),
            goals_done: if self.begun {
                goals_before(self.chapter, self.step)
            } else {
                0
            },
            systems_visited: count(self.visited.len()),
            worlds_landed: count(self.landed.len()),
            leads_found: count(self.leads.len()),
            leads_done: count(self.leads.iter().filter(|record| record.done).count()),
            leads_by_kind: [0; 5],
            logs: count(self.logs.len()),
            fragments: count(self.fragments.len()),
        }
    }

    /// The lead `record` names in the world seeded `seed`.
    #[must_use]
    pub fn lead_of_seed(seed: u64, record: &LeadRecord) -> Option<Lead> {
        leads_of(seed, record.system)
            .into_iter()
            .find(|lead| lead.index == record.index)
    }

    /// The crew's stats in the world seeded `seed` (leads by kind need the seed).
    #[must_use]
    pub fn stats_of_seed(&self, seed: u64) -> GalaxyStats {
        let mut stats = self.stats();
        let mut by_kind = [0_u32; 5];
        for record in self.leads.iter().filter(|record| record.done) {
            if let Some(lead) = Self::lead_of_seed(seed, record) {
                by_kind[lead.kind.index()] += 1;
            }
        }
        stats.leads_by_kind = by_kind;
        stats
    }
}

/// Goals done before goal `step` of chapter `chapter`.
#[must_use]
pub fn goals_before(chapter: u8, step: u8) -> u32 {
    let earlier: usize = CHAPTERS
        .iter()
        .take(usize::from(chapter))
        .map(|chapter| chapter.goals.len())
        .sum();
    u32::try_from(earlier).unwrap_or(u32::MAX) + u32::from(step)
}

/// Every goal of the story.
#[must_use]
pub fn goal_count() -> u32 {
    goals_before(CHAPTER_COUNT, 0)
}

/// The crew's completion stats (the discovery log's summary).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GalaxyStats {
    pub chapters_done: u8,
    pub goals_done: u32,
    pub systems_visited: u32,
    pub worlds_landed: u32,
    pub leads_found: u32,
    pub leads_done: u32,
    /// Leads done by [`LeadKind::index`].
    pub leads_by_kind: [u32; 5],
    pub logs: u32,
    pub fragments: u32,
}

/// A galaxy story action ([`crate::PlayerCommand::Galaxy`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum GalaxyCommand {
    /// Make the current goal's target the player's own nav waypoint (the warp's target).
    SetCourse,
    /// Make lead `index` of the discovery log the player's waypoint.
    SetLeadCourse { lead: u16 },
    /// Ask the archivist about the road.
    AskArchivist,
    /// The finale's choice (at the Harbour's gate, the Warden calm).
    Choose(FinaleChoice),
}

/// Why a galaxy action was refused (the [`GalaxyEvent::Refused`] detail).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum GalaxyRefusal {
    /// The story has not begun (space is not open).
    NotBegun,
    /// Nothing to set a course to.
    NoTarget,
    /// The choice is not open yet (the Warden, the gate) or was made.
    NotYet,
    /// Not at the Harbour's gate.
    NotAtGate,
}

impl GalaxyRefusal {
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::NotBegun => 0,
            Self::NoTarget => 1,
            Self::NotYet => 2,
            Self::NotAtGate => 3,
        }
    }

    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::NotBegun),
            1 => Some(Self::NoTarget),
            2 => Some(Self::NotYet),
            3 => Some(Self::NotAtGate),
            _ => None,
        }
    }

    /// What the HUD says.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotBegun => "The road opens once the Heart wakes and the sky is open.",
            Self::NoTarget => "Nothing to set a course to.",
            Self::NotYet => "Not yet: the Warden still holds the gate.",
            Self::NotAtGate => "Bring your rig to the Harbour's gate to choose.",
        }
    }
}

/// What happened on the road (netcode `1040 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum GalaxyEvent {
    /// The story began (`detail`: 0).
    Begun,
    /// A chapter began (`detail`: the chapter).
    ChapterBegun,
    /// A goal was done (`detail`: `chapter * 16 + step`).
    GoalDone,
    /// A chapter was finished (`detail`: the chapter).
    ChapterDone,
    /// A space log was found (`detail`: its index).
    LogFound,
    /// A chapter's relic was found (`detail`: the chapter).
    RelicFound,
    /// A hold began: raiders are coming (`detail`: the faction's code).
    HoldBegun,
    /// The Warden came out (`detail`: 0).
    WardenAppeared,
    /// The Warden is calm (`detail`: 0).
    WardenCalmed,
    /// The finale's choice was made (`detail`: [`FinaleChoice::code`]).
    Chosen,
    /// A lead was picked up (`detail`: its index in the discovery log).
    LeadFound,
    /// A lead's step was done (`detail`: its index in the discovery log).
    LeadStep,
    /// A lead was finished (`detail`: its index in the discovery log).
    LeadDone,
    /// The archivist remarked (`detail`: [`journal::REMARKS`] index).
    ArchivistRemark,
    /// The course was set (`detail`: 0 story, 1 lead).
    CourseSet,
    /// Refused (`detail`: a [`GalaxyRefusal`] code).
    Refused,
    /// A builders' fragment was found (`detail`: its index).
    FragmentFound,
}

impl GalaxyEvent {
    pub const ALL: [Self; 17] = [
        Self::Begun,
        Self::ChapterBegun,
        Self::GoalDone,
        Self::ChapterDone,
        Self::LogFound,
        Self::RelicFound,
        Self::HoldBegun,
        Self::WardenAppeared,
        Self::WardenCalmed,
        Self::Chosen,
        Self::LeadFound,
        Self::LeadStep,
        Self::LeadDone,
        Self::ArchivistRemark,
        Self::CourseSet,
        Self::Refused,
        Self::FragmentFound,
    ];

    /// Its code (netcode adds 1040; at most 19).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Begun => 0,
            Self::ChapterBegun => 1,
            Self::GoalDone => 2,
            Self::ChapterDone => 3,
            Self::LogFound => 4,
            Self::RelicFound => 5,
            Self::HoldBegun => 6,
            Self::WardenAppeared => 7,
            Self::WardenCalmed => 8,
            Self::Chosen => 9,
            Self::LeadFound => 10,
            Self::LeadStep => 11,
            Self::LeadDone => 12,
            Self::ArchivistRemark => 13,
            Self::CourseSet => 14,
            Self::Refused => 15,
            Self::FragmentFound => 16,
        }
    }
}

/// Fills a chapter text's placeholders: `{system}`, `{world}`, `{theme}`, `{harbour}`.
#[must_use]
pub fn fill(text: &str, words: &StopWords) -> String {
    text.replace("{system}", &words.system)
        .replace("{world}", &words.world)
        .replace("{theme}", &words.theme)
        .replace("{harbour}", &words.harbour)
}

/// The names a chapter's words are filled with.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StopWords {
    pub system: String,
    pub world: String,
    pub theme: String,
    pub harbour: String,
}

/// The names of chapter `chapter`'s stop in `plan` (the world seeded `seed`).
#[must_use]
pub fn stop_words(seed: u64, plan: &plan::StoryPlan, chapter: u8) -> StopWords {
    let space = crate::space::Space::shared(seed);
    let harbour = space
        .system(plan.harbour.system)
        .map_or_else(|| "the Harbour's star".to_owned(), |system| system.name);
    let Some(stop) = plan.stop(chapter) else {
        return StopWords {
            harbour,
            ..StopWords::default()
        };
    };
    let system = space.system(stop.system);
    StopWords {
        system: system.as_ref().map_or_else(
            || "an uncharted star".to_owned(),
            |found| found.name.clone(),
        ),
        world: stop
            .body
            .and_then(|body| system.as_ref().map(|found| found.body_name(body.body)))
            .unwrap_or_else(|| "its world".to_owned()),
        // A noun phrase: "machine world", "crystal world", "gas giant".
        theme: stop.theme.map_or_else(
            || "strange world".to_owned(),
            |theme| {
                let name = theme.name();
                if name.ends_with("world") || name.ends_with("giant") {
                    name.to_owned()
                } else {
                    format!("{name} world")
                }
            },
        ),
        harbour,
    }
}

#[cfg(test)]
mod tests;
