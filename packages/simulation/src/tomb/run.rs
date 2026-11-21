//! Live runs, site counters and permanent unlocks: the world's [`Tombs`] (WP-TOMBS).
//!
//! One shared world field (serde default: older saves load with none), replicated as the
//! `SectionKey::Tombs` on-change section to everyone and hashed only once anything happened,
//! so worlds without tombs keep their hashes.
//!
//! * A [`TombRun`] is one party's descent: the site, the site's run counter value, the run slot
//!   it is realized in, the floor, the [`RunPhase`], the members ([`RunMember`]: relics, relic
//!   offers between floors, banked and unbanked loot, downed state), the floor's rooms (for
//!   ambushes and the HUD) and the boss.
//! * [`Tombs::site_runs`] counts the runs started at each site (the floor seed's run counter:
//!   every run is fresh); [`Tombs::unsealed`] lists broken seals.
//! * [`TombUnlocks`] are per player and permanent: the relic codex, tomb maps (sites entered),
//!   cleared sites, guardians defeated (trophies) and lore pages.
//! * [`RunSummary`] is the last run's outcome per player, for the death / summary screen.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    ids::{PlayerId, SimulationTick},
    terrain::TilePosition,
};

use super::{
    SiteId, SiteStyle, TombSite,
    realize::{RealizedRoom, SpawnSlot},
    relic::{Relic, RelicEffects, effects_of},
};

/// What a run is doing.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum RunPhase {
    /// On a floor.
    #[default]
    Exploring,
    /// The stairs were taken: members pick relics, then the next floor is built.
    Picking,
}

/// How a run ended.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum RunOutcome {
    /// Down every floor and out past the guardian: everything banked, the site cleared.
    Finished,
    /// Out through the way back on the first floor: everything banked.
    Retreated,
    /// Every member downed: unbanked loot lost.
    Wiped,
    /// The member left the session mid-run: unbanked loot lost.
    Abandoned,
}

impl RunOutcome {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Finished => "Tomb Cleared",
            Self::Retreated => "Made It Out",
            Self::Wiped => "Party Lost",
            Self::Abandoned => "Run Abandoned",
        }
    }

    /// Whether unbanked loot is kept.
    #[must_use]
    pub const fn banks(self) -> bool {
        matches!(self, Self::Finished | Self::Retreated)
    }
}

/// One member of a run's party.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunMember {
    pub player: PlayerId,
    /// Relics picked this run.
    pub relics: Vec<Relic>,
    /// Relics on offer while the run is picking (empty once picked).
    pub offers: Vec<Relic>,
    /// Picked (or passed) this time.
    pub picked: bool,
    /// Loot value (credits) found and not yet banked: lost on a wipe.
    pub unbanked: u32,
    /// Loot value banked at shrines this run (paid out at once).
    pub banked: u32,
    /// Downed since this tick (`None`: standing).
    pub downed: Option<SimulationTick>,
    /// A crewmate has been standing beside this downed member since this tick.
    pub revive_since: Option<SimulationTick>,
    /// Caches opened this run.
    pub caches: u32,
    /// The lore page found this run, if any.
    #[serde(default)]
    pub lore: Option<u8>,
}

impl RunMember {
    #[must_use]
    pub const fn new(player: PlayerId) -> Self {
        Self {
            player,
            relics: Vec::new(),
            offers: Vec::new(),
            picked: false,
            unbanked: 0,
            banked: 0,
            downed: None,
            revive_since: None,
            caches: 0,
            lore: None,
        }
    }

    /// The member's relic effects (synergies included).
    #[must_use]
    pub fn effects(&self) -> RelicEffects {
        effects_of(&self.relics)
    }

    #[must_use]
    pub const fn is_downed(&self) -> bool {
        self.downed.is_some()
    }
}

/// One party's run through a site.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TombRun {
    pub id: u32,
    pub site: TombSite,
    /// The site's run counter value for this run (the floor seed).
    pub run: u32,
    /// The run slot the floors are realized in ([`crate::worldgen::tomb_sites::slot_origin`]).
    pub slot: u8,
    /// 0-based floor.
    pub floor: u8,
    pub phase: RunPhase,
    pub members: Vec<RunMember>,
    /// The floor's rooms (absolute interiors).
    pub rooms: Vec<RealizedRoom>,
    /// The floor's spawn slots (absolute tiles).
    pub spawns: Vec<SpawnSlot>,
    /// Rooms whose ambush has sprung (by index into [`Self::rooms`]).
    pub sprung: Vec<bool>,
    /// Where the party stands on arrival (absolute feet tile).
    pub start: TilePosition,
    /// The guardian (entity id), once spawned.
    pub boss: Option<u64>,
    /// The guardian is down (or there was none to fight).
    pub boss_defeated: bool,
    /// The shrine the party last rested at (absolute tile), on this floor.
    pub checkpoint: Option<TilePosition>,
    pub started: SimulationTick,
    /// When the relic picks began (they time out).
    #[serde(default)]
    pub picking_since: Option<SimulationTick>,
    /// The way out was taken this tick: the run ends in the tombs pass.
    #[serde(default)]
    pub leaving: bool,
    /// T28: the floor's crumbling and fallen slabs (side view; cleared with each floor).
    #[serde(default)]
    pub crumbling: Vec<super::traps::CrumblingSlab>,
    /// T28: the guardian's phase so far (0 until it is hurt into its second; the phase event
    /// fires as it rises).
    #[serde(default)]
    pub boss_phase: u8,
    /// A crypt floor's trap overlays and their state (T29; empty on side-view runs).
    #[serde(default)]
    pub crypt: super::crypt::CryptState,
}

impl TombRun {
    #[must_use]
    pub const fn style(&self) -> SiteStyle {
        self.site.style
    }

    /// Whether the party is on the last floor.
    #[must_use]
    pub const fn on_last_floor(&self) -> bool {
        self.floor + 1 >= self.site.floors()
    }

    #[must_use]
    pub fn member(&self, player: PlayerId) -> Option<&RunMember> {
        self.members.iter().find(|member| member.player == player)
    }

    pub fn member_mut(&mut self, player: PlayerId) -> Option<&mut RunMember> {
        self.members
            .iter_mut()
            .find(|member| member.player == player)
    }

    /// Whether every member is downed.
    #[must_use]
    pub fn wiped(&self) -> bool {
        !self.members.is_empty() && self.members.iter().all(RunMember::is_downed)
    }

    /// Total unbanked loot of the party.
    #[must_use]
    pub fn unbanked(&self) -> u32 {
        self.members.iter().map(|member| member.unbanked).sum()
    }
}

/// Permanent tomb unlocks of one player.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TombUnlocks {
    /// Relics ever picked (the relic codex).
    pub codex: BTreeSet<Relic>,
    /// Sites entered (tomb maps: their markers stay on the map).
    pub maps: BTreeSet<SiteId>,
    /// Sites finished.
    pub cleared: BTreeSet<SiteId>,
    /// Guardians defeated (trophies for homes).
    pub trophies: u32,
    /// Lore pages found (indices into [`super::LORE_PAGES`]).
    pub lore: BTreeSet<u8>,
    /// Runs started and finished.
    pub runs: u32,
    pub finished: u32,
    /// Most loot banked in one run.
    pub best_haul: u32,
}

/// The last run's outcome for one player (the death / summary screen).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunSummary {
    /// Increases with every summary, so the game can tell a new one from the last.
    pub serial: u32,
    pub site: SiteId,
    pub style: SiteStyle,
    pub tier: u8,
    pub outcome: RunOutcome,
    /// Floors cleared (stairs taken).
    pub floors: u8,
    pub floors_total: u8,
    pub banked: u32,
    pub lost: u32,
    pub relics: Vec<Relic>,
    /// Run length in ticks.
    pub ticks: u64,
    /// Lore page found on the way out, if any.
    pub lore: Option<u8>,
}

/// Every run, site counter, seal and unlock (see the module docs).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Tombs {
    #[serde(default)]
    pub runs: BTreeMap<u32, TombRun>,
    #[serde(default)]
    pub next_run: u32,
    #[serde(default)]
    pub site_runs: BTreeMap<SiteId, u32>,
    #[serde(default)]
    pub unsealed: BTreeSet<SiteId>,
    #[serde(default)]
    pub unlocks: BTreeMap<PlayerId, TombUnlocks>,
    #[serde(default)]
    pub summaries: BTreeMap<PlayerId, RunSummary>,
    #[serde(default)]
    pub next_summary: u32,
}

impl Tombs {
    /// Whether nothing has happened yet (the world hash leaves it out).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The run `player` is in, if any.
    #[must_use]
    pub fn run_of(&self, player: PlayerId) -> Option<&TombRun> {
        self.runs
            .values()
            .find(|run| run.members.iter().any(|member| member.player == player))
    }

    /// The id of the run `player` is in.
    #[must_use]
    pub fn run_id_of(&self, player: PlayerId) -> Option<u32> {
        self.run_of(player).map(|run| run.id)
    }

    /// `player`'s member record in their run.
    #[must_use]
    pub fn member_of(&self, player: PlayerId) -> Option<&RunMember> {
        self.run_of(player).and_then(|run| run.member(player))
    }

    /// `player`'s relic effects (none outside a run).
    #[must_use]
    pub fn effects_of(&self, player: PlayerId) -> RelicEffects {
        self.member_of(player)
            .map(RunMember::effects)
            .unwrap_or_default()
    }

    /// `player`'s permanent unlocks.
    #[must_use]
    pub fn unlocks_of(&self, player: PlayerId) -> Option<&TombUnlocks> {
        self.unlocks.get(&player)
    }

    /// Whether `site`'s seal is broken.
    #[must_use]
    pub fn is_unsealed(&self, site: SiteId) -> bool {
        self.unsealed.contains(&site)
    }

    /// The lowest run slot no live run uses.
    #[must_use]
    pub fn free_slot(&self) -> Option<u8> {
        (0..crate::worldgen::tomb_sites::SLOTS)
            .find(|slot| self.runs.values().all(|run| run.slot != *slot))
    }
}
