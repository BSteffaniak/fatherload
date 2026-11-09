//! Side quests (plan §8): data-driven definitions, the reusable objective primitives they are
//! built from, world binding, quest givers with reputation, blueprint fragments, and each
//! player's quest log.
//!
//! - **Definitions** ([`QuestDef`]) are static data tables in [`content`]: the quest giver,
//!   tier, prerequisites, the targets to bind in the current world, a sequence of stages whose
//!   objectives are [`Objective`] primitives, optional [`Twist`]s, rewards and authored text.
//!   The content suite (`content_tests`) checks every reference, the unlock graph, rewards per
//!   tier, solo paths for co-op quests, a "no filler" lint and binding on many seeds.
//! - **Binding** ([`binding`]): accepting a quest binds its [`TargetSpec`]s to real features
//!   and points of interest through the world's [`crate::worldgen::FeatureIndex`]
//!   (deterministically: same world, player and quest, same places), and lays out marked spots
//!   for multi-spot objectives.
//! - **State**: a player's [`QuestLog`] (active solo quests, completions, failures, reputation
//!   per giver, blueprint fragments, the HQ board ledger) lives in their private loadout; quests
//!   accepted in crew mode are shared in [`CrewQuests`] (the shared `Quests` section) so every
//!   rig sees their markers. Everything is saved with the world and hashed.
//! - **Rules** (`world/quests.rs`): accept, turn in and abandon are town services at the
//!   giver's counter; objectives advance inside the fixed tick.
//!
//! Durations are authored in milliseconds and converted with the session tick rate, so quests
//! behave the same at any tick rate.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    economy::SurfaceZone,
    ids::PlayerId,
    infrastructure::InfrastructureKind,
    player::Player,
    poi::{PoiKind, RigFitting},
    rig::RigPartKind,
    terrain::{ArtifactKind, MineralKind, StrategicResourceKind, TilePosition},
    worldgen::{FeatureId, FeatureKind, TileRect},
};

pub mod binding;
pub mod content;
#[cfg(test)]
mod content_tests;
pub mod describe;

pub use binding::{BoardOffer, bind_quest, board_offers};
pub use content::{BOARD_TEMPLATES, QUESTS};

/// Most quests (solo and crew together) one player may have active at once.
pub const MAX_ACTIVE_QUESTS: usize = 5;

/// A rig at or above this row counts as back on the surface.
pub const SURFACE_ROW: i32 = 6;

/// Stable id of a quest definition (saved in quest logs; never reused).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct QuestId(pub u16);

impl QuestId {
    /// The definition this id names, among the authored quests and board templates.
    #[must_use]
    pub fn def(self) -> Option<&'static QuestDef> {
        QUESTS
            .iter()
            .chain(BOARD_TEMPLATES.iter())
            .find(|def| def.id == self)
    }
}

/// Who hands out a quest: the keeper of a town building (and the HQ quest board).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum QuestGiver {
    /// Kade, the depot's ore buyer: deliveries, markets, careful hauling.
    Kade,
    /// Mara, the salvage yard boss: derelicts, logs, recoveries.
    Mara,
    /// Nix, the explosives shack's demolition artist: bombs, sequences, controlled chaos.
    Nix,
    /// Iona, the repair garage mechanic: hazards, endurance, field tests.
    Iona,
    /// Pip, the fuel station kid: speed runs, relays, couriering.
    Pip,
    /// Director Vale's rotating HQ quest board (procedural parameters, authored twists).
    Vale,
}

impl QuestGiver {
    pub const ALL: [Self; 6] = [
        Self::Kade,
        Self::Mara,
        Self::Nix,
        Self::Iona,
        Self::Pip,
        Self::Vale,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Kade => "Kade",
            Self::Mara => "Mara",
            Self::Nix => "Nix",
            Self::Iona => "Iona",
            Self::Pip => "Pip",
            Self::Vale => "Director Vale",
        }
    }

    /// Where the giver works.
    #[must_use]
    pub const fn zone(self) -> SurfaceZone {
        match self {
            Self::Kade => SurfaceZone::Depot,
            Self::Mara => SurfaceZone::Salvage,
            Self::Nix => SurfaceZone::Explosives,
            Self::Iona => SurfaceZone::Repair,
            Self::Pip => SurfaceZone::Fuel,
            Self::Vale => SurfaceZone::Headquarters,
        }
    }

    /// The giver working at `zone`, if any.
    #[must_use]
    pub const fn at(zone: SurfaceZone) -> Option<Self> {
        match zone {
            SurfaceZone::Depot => Some(Self::Kade),
            SurfaceZone::Salvage => Some(Self::Mara),
            SurfaceZone::Explosives => Some(Self::Nix),
            SurfaceZone::Repair => Some(Self::Iona),
            SurfaceZone::Fuel => Some(Self::Pip),
            SurfaceZone::Headquarters => Some(Self::Vale),
            SurfaceZone::Shop | SurfaceZone::Bank => None,
        }
    }

    /// One line of personality for the quest desk.
    #[must_use]
    pub const fn personality(self) -> &'static str {
        match self {
            Self::Kade => "Counts every gram twice and trusts nobody's scales but his own.",
            Self::Mara => "Collects other people's bad luck and sells it back as good luck.",
            Self::Nix => "Believes most problems are just rock that hasn't been asked loudly.",
            Self::Iona => "Fixes rigs by day, stress-tests them by night. Keeps notes on you.",
            Self::Pip => "Fastest courier in town, according to Pip. Timing everything.",
            Self::Vale => "Posts the jobs HQ can't be seen asking for. Pays on time.",
        }
    }

    /// What reputation with this giver earns, per rank.
    #[must_use]
    pub const fn perk(self) -> &'static str {
        match self {
            Self::Kade => "+2% on cargo sales per rank",
            Self::Mara => "deeper salvage contracts and her best leads",
            Self::Nix => "4% back on charges per rank",
            Self::Iona => "4% back on repairs per rank",
            Self::Pip => "4% back on fuel per rank",
            Self::Vale => "richer board postings and HQ's trust",
        }
    }

    /// Percent of a service price this giver hands back at `rank` (or adds to a sale, for
    /// Kade).
    #[must_use]
    pub const fn rebate_percent(self, rank: u8) -> u32 {
        let rank = if rank > 4 { 4 } else { rank } as u32;
        match self {
            Self::Kade => rank * 2,
            Self::Nix | Self::Iona | Self::Pip => rank * 4,
            Self::Mara | Self::Vale => 0,
        }
    }
}

/// Reputation needed for each rank (rank `i` needs `REPUTATION_RANKS[i]` points).
pub const REPUTATION_RANKS: [u32; 5] = [0, 10, 25, 50, 90];

/// The rank `points` of reputation earn (0..=4).
#[must_use]
pub fn reputation_rank(points: u32) -> u8 {
    let rank = REPUTATION_RANKS
        .iter()
        .rposition(|needed| points >= *needed)
        .unwrap_or(0);
    u8::try_from(rank).unwrap_or(4)
}

/// Display name of a reputation rank.
#[must_use]
pub const fn rank_name(rank: u8) -> &'static str {
    match rank {
        0 => "Stranger",
        1 => "Regular",
        2 => "Trusted",
        3 => "Partner",
        _ => "Family",
    }
}

/// How deep and demanding a quest is; rewards are bounded per tier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum QuestTier {
    /// Clay to slate: learning the earth.
    Shallow,
    /// Slate to the first deep strata.
    Mid,
    /// The deep strata (later batches).
    Deep,
}

impl QuestTier {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Shallow => "Shallow",
            Self::Mid => "Mid",
            Self::Deep => "Deep",
        }
    }

    /// Upper bounds on one quest's rewards, kept inside the economy curve: upgrades cost
    /// 85-135 x level squared, so a shallow quest pays about one early upgrade and a mid quest
    /// a mid-game one.
    #[must_use]
    pub const fn reward_bounds(self) -> RewardBounds {
        match self {
            Self::Shallow => RewardBounds {
                credits: 450,
                materials: 1,
                fragments: 1,
                fitting: false,
                bombs: 4,
                reputation: 12,
            },
            Self::Mid => RewardBounds {
                credits: 1_300,
                materials: 3,
                fragments: 1,
                fitting: true,
                bombs: 6,
                reputation: 15,
            },
            Self::Deep => RewardBounds {
                credits: 3_200,
                materials: 6,
                fragments: 2,
                fitting: true,
                bombs: 8,
                reputation: 20,
            },
        }
    }
}

/// Per-tier reward ceilings (see [`QuestTier::reward_bounds`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RewardBounds {
    pub credits: u32,
    pub materials: u32,
    pub fragments: u8,
    pub fitting: bool,
    pub bombs: u32,
    pub reputation: u32,
}

/// An upgrade line a prerequisite can ask about.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpgradeStat {
    Drill,
    Engine,
    Hull,
    FuelTank,
    CargoBay,
    Radiator,
    Scanner,
}

impl UpgradeStat {
    #[must_use]
    pub const fn level(self, player: &Player) -> u8 {
        match self {
            Self::Drill => player.drill_strength,
            Self::Engine => player.engine_level,
            Self::Hull => player.hull_level,
            Self::FuelTank => player.fuel_tank_level,
            Self::CargoBay => player.cargo_bay_level,
            Self::Radiator => player.radiator_level,
            Self::Scanner => player.scanner_level,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Drill => "drill",
            Self::Engine => "engine",
            Self::Hull => "hull",
            Self::FuelTank => "fuel tank",
            Self::CargoBay => "cargo bay",
            Self::Radiator => "radiator",
            Self::Scanner => "scanner",
        }
    }
}

/// What a player must have done before a quest is offered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Prerequisite {
    /// The player has been this deep (tile row).
    Depth(i32),
    /// The player has visited a point of interest of this kind.
    Visited(PoiKind),
    /// The player has recovered at least this many derelict logs.
    Logs(u8),
    /// The player has at least this much reputation with a giver.
    Reputation(QuestGiver, u32),
    /// The player completed another quest.
    Quest(QuestId),
    /// An upgrade line at least at this level.
    Upgrade(UpgradeStat, u8),
}

impl Prerequisite {
    /// Whether `player` meets it.
    #[must_use]
    pub fn met(self, player: &Player) -> bool {
        let log = &player.quests;
        match self {
            Self::Depth(row) => {
                log.deepest_row
                    .max(player.tile_position(crate::TILE_SIZE).y)
                    >= row
            }
            Self::Visited(kind) => log.visited_kinds.contains(&kind),
            Self::Logs(count) => player.journal.logs.len() >= usize::from(count),
            Self::Reputation(giver, points) => log.reputation(giver) >= points,
            Self::Quest(quest) => log.completed.contains(&quest),
            Self::Upgrade(stat, level) => stat.level(player) >= level,
        }
    }

    /// A short "needs" line for locked quests.
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::Depth(row) => format!("reach {row}m"),
            Self::Visited(kind) => format!("visit a {}", kind.name().to_lowercase()),
            Self::Logs(count) => format!("recover {count} derelict log(s)"),
            Self::Reputation(giver, points) => {
                format!("{} reputation {points}", giver.name())
            }
            Self::Quest(quest) => format!(
                "finish \"{}\"",
                quest.def().map_or("an earlier job", |def| def.title)
            ),
            Self::Upgrade(stat, level) => format!("{} level {level}", stat.name()),
        }
    }
}

/// A thing a quest asks you to deliver.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum QuestItem {
    Mineral(MineralKind),
    Artifact(ArtifactKind),
    Material(StrategicResourceKind),
}

impl QuestItem {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mineral(mineral) => mineral.name(),
            Self::Artifact(artifact) => artifact.name(),
            Self::Material(material) => material.name(),
        }
    }

    /// How many `player` carries.
    #[must_use]
    pub fn held(self, player: &Player) -> u32 {
        match self {
            Self::Mineral(mineral) => player.cargo.get(&mineral).copied(),
            Self::Artifact(artifact) => player.artifacts.get(&artifact).copied(),
            Self::Material(material) => player.materials.get(&material).copied(),
        }
        .unwrap_or(0)
    }

    /// Takes up to `count` from `player`; returns how many were taken.
    pub fn take(self, player: &mut Player, count: u32) -> u32 {
        fn take_from<K: Ord>(map: &mut BTreeMap<K, u32>, key: &K, count: u32) -> u32 {
            let Some(held) = map.get_mut(key) else {
                return 0;
            };
            let taken = (*held).min(count);
            *held -= taken;
            if *held == 0 {
                map.remove(key);
            }
            taken
        }
        match self {
            Self::Mineral(mineral) => take_from(&mut player.cargo, &mineral, count),
            Self::Artifact(artifact) => take_from(&mut player.artifacts, &artifact, count),
            Self::Material(material) => take_from(&mut player.materials, &material, count),
        }
    }
}

/// Bands by [`crate::worldgen::Band::index`], inclusive: 1 clay, 2 grey stone, 3 slate,
/// 4 basalt, 5 Crystal Faults, 6 Fossil Oceans, ...
pub type BandRange = (u8, u8);

/// What a quest target binds to when the quest is accepted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetSpec {
    /// A point of interest of `kind` centred in `bands`; `unopened` asks for one whose loot or
    /// lock is still intact. Sites the player has not visited are preferred, then the nearest.
    Poi {
        kind: PoiKind,
        bands: BandRange,
        unopened: bool,
    },
    /// A generated feature (cavern, chamber, motherlode, artifact bed, hazard cluster, ...)
    /// centred in `bands`; the nearest is preferred.
    Feature { kind: FeatureKind, bands: BandRange },
}

impl TargetSpec {
    #[must_use]
    pub const fn bands(self) -> BandRange {
        match self {
            Self::Poi { bands, .. } | Self::Feature { bands, .. } => bands,
        }
    }

    /// What players call the place.
    #[must_use]
    pub const fn noun(self) -> &'static str {
        match self {
            Self::Poi { kind, .. } => match kind {
                PoiKind::SupplyCache => "supply cache",
                PoiKind::DerelictRig => "derelict rig",
                PoiKind::Vault => "vault",
                PoiKind::Formation => "formation",
                PoiKind::RivalClaim => "rival claim",
            },
            Self::Feature { kind, .. } => match kind {
                FeatureKind::LargeCavern => "cavern",
                FeatureKind::Chamber => "deep chamber",
                FeatureKind::Motherlode => "motherlode",
                FeatureKind::ArtifactBed => "artifact bed",
                FeatureKind::HazardCluster => "hazard pocket",
                FeatureKind::StratumTransition => "stratum seam",
                FeatureKind::PrefabSlot => "quiet hollow",
                FeatureKind::PointOfInterest => "site",
            },
        }
    }
}

/// A reusable objective primitive. `target` fields index [`QuestDef::targets`].
///
/// Co-op primitives (`Simultaneous`, `Relay`, `HoldAndClimb`, `Escort`, `SplitDelivery`) carry
/// both their crew parameters and a solo variant, so every co-op quest can be finished alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Objective {
    /// Hand `count` of an item in at turn-in (only in the last stage).
    Deliver { item: QuestItem, count: u32 },
    /// Bring a rig to the target.
    Reach { target: u8 },
    /// Bring a rig this deep (tile row).
    ReachDepth { row: i32 },
    /// Drill `count` tiles in and around the target.
    Dig { target: u8, count: u32 },
    /// Pulse a scanner near the target.
    Scan { target: u8 },
    /// Open the target: a vault's lock, or drill a crate, salvage core or locker inside it.
    OpenPoi { target: u8 },
    /// Spend `seconds` in and around the target (a hazard zone).
    Survive { target: u8, seconds: u16 },
    /// Pick up a quest parcel at the target (it rides with the owner's rig).
    PickUp { target: u8, parcel: &'static str },
    /// Get the owner's rig back to the surface.
    ReturnToSurface,
    /// Have infrastructure of `kind` standing at the target.
    Build {
        target: u8,
        kind: InfrastructureKind,
    },
    /// Detonate bombs at `spots` marked spots around the target, in order (a wrong spot
    /// resets the sequence).
    Detonate { target: u8, spots: u8 },
    /// Drill the salvage core of the target derelict (recovering its log).
    RecoverLog { target: u8 },
    /// Co-op: rigs on all `spots` marked spots around the target within `window_ms` of each
    /// other (solo: one rig within `solo_window_ms`).
    Simultaneous {
        target: u8,
        spots: u8,
        window_ms: u32,
        solo_window_ms: u32,
    },
    /// Co-op: carry a marker from the giver's door to the target. A carrier can take it only
    /// `leg_rows` rows before it must be handed to another rig (solo: it drops and cools for
    /// `solo_cooldown_ms` before the same rig can take it on).
    Relay {
        target: u8,
        leg_rows: u16,
        solo_cooldown_ms: u32,
    },
    /// Co-op: the goal counts only while the switch is held: a rig on it, infrastructure
    /// anchored next to it, or (solo) its latch still counting down `solo_latch_ms` after the
    /// rig left.
    HoldAndClimb {
        switch: u8,
        goal: u8,
        solo_latch_ms: u32,
    },
    /// Co-op: a slow haul crawls from the target to the surface through open tunnel, one tile
    /// every `step_ms` while a rig is near (every `crew_step_ms` with two rigs pushing).
    Escort {
        target: u8,
        step_ms: u32,
        crew_step_ms: u32,
    },
    /// Co-op: drop `count` of an item at each of two buildings within `window_ms` of each
    /// other (solo: within `solo_window_ms`). A drop that waits too long spoils.
    SplitDelivery {
        item: QuestItem,
        count: u32,
        zones: [SurfaceZone; 2],
        window_ms: u32,
        solo_window_ms: u32,
    },
}

impl Objective {
    /// Whether this is a co-op primitive.
    #[must_use]
    pub const fn is_coop(self) -> bool {
        matches!(
            self,
            Self::Simultaneous { .. }
                | Self::Relay { .. }
                | Self::HoldAndClimb { .. }
                | Self::Escort { .. }
                | Self::SplitDelivery { .. }
        )
    }

    /// Whether this is a basic primitive (the "filler" lint wants more than these).
    #[must_use]
    pub const fn is_trivial(self) -> bool {
        matches!(
            self,
            Self::Deliver { .. } | Self::Reach { .. } | Self::ReachDepth { .. }
        )
    }

    /// Whether a crew-mode co-op primitive has a solo variant at least as forgiving.
    #[must_use]
    pub const fn has_solo_path(self) -> bool {
        match self {
            Self::Simultaneous {
                window_ms,
                solo_window_ms,
                ..
            }
            | Self::SplitDelivery {
                window_ms,
                solo_window_ms,
                ..
            } => solo_window_ms > window_ms,
            Self::Relay {
                solo_cooldown_ms, ..
            } => solo_cooldown_ms > 0,
            Self::HoldAndClimb { solo_latch_ms, .. } => solo_latch_ms > 0,
            Self::Escort {
                step_ms,
                crew_step_ms,
                ..
            } => step_ms >= crew_step_ms && step_ms > 0,
            _ => true,
        }
    }

    /// Targets this objective refers to.
    #[must_use]
    pub fn targets(self) -> Vec<u8> {
        match self {
            Self::Reach { target }
            | Self::Dig { target, .. }
            | Self::Scan { target }
            | Self::OpenPoi { target }
            | Self::Survive { target, .. }
            | Self::PickUp { target, .. }
            | Self::Build { target, .. }
            | Self::Detonate { target, .. }
            | Self::RecoverLog { target }
            | Self::Simultaneous { target, .. }
            | Self::Relay { target, .. }
            | Self::Escort { target, .. } => vec![target],
            Self::HoldAndClimb { switch, goal, .. } => vec![switch, goal],
            Self::Deliver { .. }
            | Self::ReachDepth { .. }
            | Self::ReturnToSurface
            | Self::SplitDelivery { .. } => Vec::new(),
        }
    }
}

/// A modifier that makes a quest harder in an interesting way.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Twist {
    /// Finish (be ready to turn in) within this many seconds of accepting.
    TimeLimit { seconds: u16 },
    /// The owner's rig may take at most this much hull damage (while carrying a parcel, or
    /// for the whole quest if it has none).
    Fragile { max_damage: u16 },
    /// Using the scanner fails the quest.
    NoScanner,
    /// Placing a bomb fails the quest.
    NoBombs,
    /// Once below the crust, surfacing before the job is done fails the quest.
    StayDown,
}

impl Twist {
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::TimeLimit { seconds } => format!("Timed: {}:{:02}", seconds / 60, seconds % 60),
            Self::Fragile { max_damage } => format!("Fragile: take at most {max_damage} damage"),
            Self::NoScanner => "Blind run: no scanner".to_owned(),
            Self::NoBombs => "Quiet job: no bombs".to_owned(),
            Self::StayDown => "Stay down: no surfacing until done".to_owned(),
        }
    }
}

/// A blueprint assembled from fragments found in vaults and earned from quests.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum BlueprintId {
    /// Hauler Engine schematics (rig part).
    HaulerFrame,
    /// Cargo Balloon schematics (rig part).
    BalloonRigging,
    /// Pressure Hull schematics (rig part).
    PressureWeave,
    /// A resonance dampener (future utility part).
    DampenerCoil,
    /// A tow winch for hauling stranded crewmates (future co-op part).
    TowWinch,
    /// A deep outpost refinery module (future base module).
    RefineryFrame,
}

/// What finishing a blueprint unlocks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlueprintUnlock {
    /// An existing rig part joins the crew's parts.
    Part(RigPartKind),
    /// A recipe for a part or base module that later systems build (recorded now).
    Recipe(&'static str),
}

impl BlueprintId {
    pub const ALL: [Self; 6] = [
        Self::HaulerFrame,
        Self::BalloonRigging,
        Self::PressureWeave,
        Self::DampenerCoil,
        Self::TowWinch,
        Self::RefineryFrame,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HaulerFrame => "Hauler Frame",
            Self::BalloonRigging => "Balloon Rigging",
            Self::PressureWeave => "Pressure Weave",
            Self::DampenerCoil => "Dampener Coil",
            Self::TowWinch => "Tow Winch",
            Self::RefineryFrame => "Refinery Frame",
        }
    }

    /// Fragments needed to complete it.
    #[must_use]
    pub const fn fragments_needed(self) -> u8 {
        match self {
            Self::HaulerFrame | Self::BalloonRigging | Self::TowWinch => 3,
            Self::PressureWeave | Self::DampenerCoil => 4,
            Self::RefineryFrame => 5,
        }
    }

    #[must_use]
    pub const fn unlock(self) -> BlueprintUnlock {
        match self {
            Self::HaulerFrame => BlueprintUnlock::Part(RigPartKind::HaulerEngine),
            Self::BalloonRigging => BlueprintUnlock::Part(RigPartKind::CargoBalloon),
            Self::PressureWeave => BlueprintUnlock::Part(RigPartKind::PressureHull),
            Self::DampenerCoil => BlueprintUnlock::Recipe("resonance dampener utility part"),
            Self::TowWinch => BlueprintUnlock::Recipe("tow winch co-op part"),
            Self::RefineryFrame => BlueprintUnlock::Recipe("outpost refinery module"),
        }
    }

    /// Blueprints vault caches hand fragments of, in order of preference.
    pub const VAULT_POOL: [Self; 4] = [
        Self::DampenerCoil,
        Self::PressureWeave,
        Self::RefineryFrame,
        Self::TowWinch,
    ];
}

/// What finishing a quest pays. Bounded per tier ([`QuestTier::reward_bounds`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuestReward {
    pub credits: u32,
    pub materials: &'static [(StrategicResourceKind, u32)],
    pub fitting: Option<RigFitting>,
    pub fragment: Option<(BlueprintId, u8)>,
    pub bombs: u32,
    pub kit: Option<InfrastructureKind>,
    pub reputation: u32,
}

impl QuestReward {
    /// Credits and reputation only.
    #[must_use]
    pub const fn credits(credits: u32, reputation: u32) -> Self {
        Self {
            credits,
            materials: &[],
            fitting: None,
            fragment: None,
            bombs: 0,
            kit: None,
            reputation,
        }
    }

    #[must_use]
    pub const fn materials(mut self, materials: &'static [(StrategicResourceKind, u32)]) -> Self {
        self.materials = materials;
        self
    }

    #[must_use]
    pub const fn fitting(mut self, fitting: RigFitting) -> Self {
        self.fitting = Some(fitting);
        self
    }

    #[must_use]
    pub const fn fragment(mut self, blueprint: BlueprintId) -> Self {
        self.fragment = Some((blueprint, 1));
        self
    }

    #[must_use]
    pub const fn bombs(mut self, bombs: u32) -> Self {
        self.bombs = bombs;
        self
    }

    #[must_use]
    pub const fn kit(mut self, kit: InfrastructureKind) -> Self {
        self.kit = Some(kit);
        self
    }

    /// A short "+300 cr, 1 Crystal Lens, blueprint fragment" summary.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("+{} cr", self.credits)];
        for (material, count) in self.materials {
            parts.push(format!("{count} {}", material.name()));
        }
        if let Some(fitting) = self.fitting {
            parts.push(fitting.name().to_owned());
        }
        if let Some((blueprint, count)) = self.fragment {
            parts.push(format!("{count} {} fragment", blueprint.name()));
        }
        if self.bombs > 0 {
            parts.push(format!("{} bombs", self.bombs));
        }
        if let Some(kit) = self.kit {
            parts.push(format!("{} kit", kit.name()));
        }
        parts.join(", ")
    }

    /// Total material units.
    #[must_use]
    pub fn material_count(&self) -> u32 {
        self.materials.iter().map(|(_, count)| *count).sum()
    }
}

/// The giver's authored lines.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuestText {
    /// The pitch at the counter.
    pub offer: &'static str,
    /// What the giver says while it is in progress (quest log).
    pub progress: &'static str,
    /// The thanks on turn-in (completion toast).
    pub complete: &'static str,
}

/// One quest, authored as data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuestDef {
    pub id: QuestId,
    pub title: &'static str,
    pub giver: QuestGiver,
    pub tier: QuestTier,
    /// Why it is fun (authoring note; every quest states one).
    pub hook: &'static str,
    pub prerequisites: &'static [Prerequisite],
    pub targets: &'static [TargetSpec],
    /// Stages run in order; every objective of a stage must be done to move on.
    pub stages: &'static [&'static [Objective]],
    /// Twists that always apply. Board templates instead pick one per posting.
    pub twists: &'static [Twist],
    pub reward: QuestReward,
    pub text: QuestText,
    /// Board templates: posted again and again with fresh places and twists.
    pub repeatable: bool,
}

impl QuestDef {
    /// Every objective, stage by stage, flattened (the order of
    /// [`ActiveQuest::objectives`]).
    pub fn objectives(&self) -> impl Iterator<Item = Objective> + '_ {
        self.stages.iter().flat_map(|stage| stage.iter().copied())
    }

    /// Index of the first objective of `stage` in [`Self::objectives`].
    #[must_use]
    pub fn stage_start(&self, stage: usize) -> usize {
        self.stages
            .iter()
            .take(stage)
            .map(|stage| stage.len())
            .sum()
    }

    /// Whether any objective is a co-op primitive.
    #[must_use]
    pub fn is_coop(&self) -> bool {
        self.objectives().any(Objective::is_coop)
    }

    /// Whether every prerequisite is met by `player` (ignores completion and active state).
    #[must_use]
    pub fn unlocked_for(&self, player: &Player) -> bool {
        self.prerequisites
            .iter()
            .all(|prerequisite| prerequisite.met(player))
    }

    /// The twists that apply to an accepted quest (`variant` picks a board twist).
    #[must_use]
    pub fn active_twists(&self, variant: u8) -> Vec<Twist> {
        if self.repeatable {
            self.twists
                .get(usize::from(variant))
                .copied()
                .into_iter()
                .collect()
        } else {
            self.twists.to_vec()
        }
    }
}

/// A quest target bound to the current world.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct QuestTarget {
    pub feature: Option<FeatureId>,
    pub center: TilePosition,
    pub bounds: TileRect,
}

/// Progress of one objective. Primitives use the fields they need; see `world/quests.rs`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ObjectiveProgress {
    #[serde(default)]
    pub done: bool,
    /// Tiles dug, bombs in sequence, survive ticks.
    #[serde(default)]
    pub count: u64,
    /// Marked spots laid out at binding (multi-spot primitives).
    #[serde(default)]
    pub spots: Vec<TilePosition>,
    /// Per spot (or per delivery zone): tick + 1 it was last touched (0: never).
    #[serde(default)]
    pub marks: Vec<u64>,
    /// Per delivery zone: units dropped.
    #[serde(default)]
    pub counts: Vec<u32>,
    /// A moving thing: the escorted haul, or the relay marker while it rests.
    #[serde(default)]
    pub position: Option<TilePosition>,
    /// Who carries the relay marker.
    #[serde(default)]
    pub carrier: Option<PlayerId>,
    /// Who carried the relay marker last.
    #[serde(default)]
    pub last_carrier: Option<PlayerId>,
    /// A deadline tick: the escort's next step, the relay cooldown, the switch latch.
    #[serde(default)]
    pub timer: u64,
    /// The row where the relay's current leg started.
    #[serde(default)]
    pub anchor: i32,
}

/// One accepted quest.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent progress flags of one quest"
)]
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ActiveQuest {
    pub quest: QuestId,
    pub owner: PlayerId,
    /// Accepted in crew mode (shared with every rig) rather than solo.
    #[serde(default)]
    pub crew: bool,
    /// The board posting's twist pick.
    #[serde(default)]
    pub variant: u8,
    pub accepted_tick: u64,
    /// Tick the time limit runs out, if timed.
    #[serde(default)]
    pub deadline: Option<u64>,
    /// Current stage.
    #[serde(default)]
    pub stage: u8,
    pub targets: Vec<QuestTarget>,
    /// Progress of every objective, in [`QuestDef::objectives`] order.
    pub objectives: Vec<ObjectiveProgress>,
    /// Hull damage (tenths) the fragile twist counts.
    #[serde(default)]
    pub damage_taken: u32,
    /// The owner's hull (tenths) last tick, for counting damage.
    #[serde(default)]
    pub last_hull: u32,
    /// The owner carries the quest parcel.
    #[serde(default)]
    pub carrying: bool,
    /// The owner has been below the crust since accepting (stay-down twist).
    #[serde(default)]
    pub descended: bool,
    /// Every stage is done: turn it in at the giver.
    #[serde(default)]
    pub ready: bool,
    /// Other rigs that helped a crew quest along.
    #[serde(default)]
    pub helpers: BTreeSet<PlayerId>,
}

impl ActiveQuest {
    #[must_use]
    pub fn def(&self) -> Option<&'static QuestDef> {
        self.quest.def()
    }
}

/// One player's quests: active solo quests, history, reputation and blueprint fragments.
///
/// Lives in the player's private loadout (replicated only to its owner, saved and hashed with
/// the player).
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct QuestLog {
    /// Quests accepted solo (crew quests live in [`CrewQuests`]).
    #[serde(default)]
    pub active: Vec<ActiveQuest>,
    #[serde(default)]
    pub completed: BTreeSet<QuestId>,
    /// Times each quest failed (failed quests can be taken again).
    #[serde(default)]
    pub failures: BTreeMap<QuestId, u16>,
    #[serde(default)]
    pub reputation: BTreeMap<QuestGiver, u32>,
    /// Fragments collected toward unfinished blueprints.
    #[serde(default)]
    pub fragments: BTreeMap<BlueprintId, u8>,
    /// Finished blueprints.
    #[serde(default)]
    pub blueprints: BTreeSet<BlueprintId>,
    /// Deepest row this player has reached.
    #[serde(default)]
    pub deepest_row: i32,
    /// Kinds of points of interest this player has been inside.
    #[serde(default)]
    pub visited_kinds: BTreeSet<PoiKind>,
    /// The board day `board_taken` refers to.
    #[serde(default)]
    pub board_day: u32,
    /// Board postings of `board_day` this player already took.
    #[serde(default)]
    pub board_taken: BTreeSet<u8>,
    /// Board jobs finished.
    #[serde(default)]
    pub board_completed: u32,
}

impl QuestLog {
    #[must_use]
    pub fn reputation(&self, giver: QuestGiver) -> u32 {
        self.reputation.get(&giver).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn rank(&self, giver: QuestGiver) -> u8 {
        reputation_rank(self.reputation(giver))
    }

    /// Adds one blueprint fragment; returns the blueprint if this completed it.
    pub fn add_fragment(&mut self, blueprint: BlueprintId) -> Option<BlueprintId> {
        if self.blueprints.contains(&blueprint) {
            return None;
        }
        let count = self.fragments.entry(blueprint).or_default();
        *count = count.saturating_add(1);
        if *count >= blueprint.fragments_needed() {
            self.fragments.remove(&blueprint);
            self.blueprints.insert(blueprint);
            Some(blueprint)
        } else {
            None
        }
    }

    /// Fragments held toward `blueprint` (its full count once finished).
    #[must_use]
    pub fn fragments_of(&self, blueprint: BlueprintId) -> u8 {
        if self.blueprints.contains(&blueprint) {
            blueprint.fragments_needed()
        } else {
            self.fragments.get(&blueprint).copied().unwrap_or(0)
        }
    }

    /// The first vault-pool blueprint this player has not finished.
    #[must_use]
    pub fn next_vault_blueprint(&self, pick: u64) -> Option<BlueprintId> {
        let open: Vec<BlueprintId> = BlueprintId::VAULT_POOL
            .into_iter()
            .filter(|blueprint| !self.blueprints.contains(blueprint))
            .collect();
        if open.is_empty() {
            return None;
        }
        // Keep collecting the most advanced one, so fragments add up.
        open.iter()
            .copied()
            .max_by_key(|blueprint| (self.fragments_of(*blueprint), u64::MAX - pick))
            .filter(|blueprint| self.fragments_of(*blueprint) > 0)
            .or_else(|| {
                open.get(usize::try_from(pick % open.len() as u64).unwrap_or(0))
                    .copied()
            })
    }
}

/// Quests accepted in crew mode: shared with every player (the `Quests` section), so the whole
/// crew sees and works their markers.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct CrewQuests {
    #[serde(default)]
    pub active: Vec<ActiveQuest>,
}

/// Why a quest failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum QuestFailure {
    OutOfTime,
    TooMuchDamage,
    UsedScanner,
    UsedBombs,
    Surfaced,
    Rescued,
}

impl QuestFailure {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::OutOfTime => "out of time",
            Self::TooMuchDamage => "the cargo took too much damage",
            Self::UsedScanner => "you used the scanner",
            Self::UsedBombs => "you set off a bomb",
            Self::Surfaced => "you surfaced too early",
            Self::Rescued => "the rescue crew lost the parcel",
        }
    }
}
