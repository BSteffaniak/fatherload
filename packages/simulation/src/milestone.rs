//! Milestones (WP-FEEL part 2): first-time achievements per player, their medals and the
//! cosmetic unlocks they award, plus the per-player records shown on the progression screen.
//!
//! # Rules
//!
//! Every [`Milestone`] is decided from state only ([`Milestone::met`] over the player and a
//! few [`MilestoneFacts`] the world gathers each tick), so it can never be missed by a dropped
//! event and old saves earn what they already did. The world checks each player every tick
//! (`WorldState::advance_milestones`, `world/milestones.rs`):
//!
//! - newly met milestones are recorded in the player's [`MilestoneLog`] with the tick they were
//!   earned, their reward cosmetics are unlocked ([`Milestone::rewards`]) and a
//!   [`crate::GameplayEventKind::MilestoneEarned`] event fires (the game's toast, fanfare and
//!   banner);
//! - the first check of a player whose log was never primed (a fresh rig, or a save from before
//!   milestones) records whatever is already met **silently** (unlocks granted, no events), so
//!   loading an old save does not bury the screen in toasts;
//! - test-mode worlds do not count milestones (like depth records).
//!
//! # Sky gates
//!
//! Two milestones belong to Act II: an **island-capable tank** and the **Act I -> II unlock**
//! (tank, engine and altitude intake that can cross the dead-air gap). Both read T16's measured
//! climb table ([`crate::climb::reach`]) against the V8 sky's bands: [`island_capable`] is a
//! climb that tops [`ISLAND_REACH_ROWS`] (a little into the low isles' band), and
//! [`crosses_dead_air`] one that tops [`DEAD_AIR_REACH_ROWS`] (the floor of the sky continents,
//! above the whole dead-air gap). Rebalancing the bands or the climb table moves the gates with
//! them. They are only awarded on worlds with a sky ([`MilestoneFacts::has_sky`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    cosmetics::{
        CosmeticItem, Decal, DrillStyle, FlameColor, HullPattern, PaintColor, RearAttachment,
        RoofAttachment,
    },
    economy::{MAX_UPGRADE_LEVEL, UpgradeKind, current_level},
    player::Player,
    worldgen::Band,
};

/// A first-time achievement.
///
/// Serialized by name: new milestones may be added anywhere; never rename one.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Milestone {
    /// Bought any upgrade.
    FirstUpgrade,
    /// Fitted a tier 5 drill (the plasma drill).
    DrillTier5,
    /// Fitted a tier 10 drill.
    DrillTier10,
    /// Fitted a tier 5 engine.
    EngineTier5,
    /// Fitted a tier 10 engine.
    EngineTier10,
    /// Maxed any core upgrade line (drill, tank, bay, engine, hull, radiator).
    MaxedLine,
    /// Maxed every core upgrade line.
    FullyLoaded,
    /// Drilled down into the Grey Stone.
    ReachedStone,
    /// Drilled down into the first deep stratum (the Crystal Faults).
    FirstDeepLayer,
    /// Drilled down into the Ancient Machine Layer.
    ReachedMachineLayer,
    /// Drilled down into the Mantle Storm Zone, the deepest stratum.
    ReachedMantle,
    /// Carried an artifact.
    FirstArtifact,
    /// Found a cave ecosystem.
    FirstCave,
    /// Entered a village.
    FirstVillage,
    /// Defeated an enemy.
    FirstKill,
    /// Staked a base claim.
    FirstClaim,
    /// Flew [`HIGH_FLYER_ALTITUDE_TILES`] above the local ground.
    HighFlyer,
    /// Fitted a fuel tank that can reach the low sky isles ([`island_capable`]).
    IslandCapableTank,
    /// The Act I -> II unlock: tank and intake can cross the dead-air gap
    /// ([`crosses_dead_air`]).
    ActTwoUnlocked,
}

/// A milestone's medal on the trophy shelf.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Medal {
    Bronze,
    Silver,
    Gold,
    /// The Act I -> II unlock and the fully loaded rig.
    Platinum,
}

impl Medal {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bronze => "Bronze",
            Self::Silver => "Silver",
            Self::Gold => "Gold",
            Self::Platinum => "Platinum",
        }
    }
}

/// What a milestone is about (progression screen grouping).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MilestoneGroup {
    Upgrades,
    Depths,
    Discoveries,
    Sky,
}

impl MilestoneGroup {
    pub const ALL: [Self; 4] = [Self::Upgrades, Self::Depths, Self::Discoveries, Self::Sky];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Upgrades => "Upgrades",
            Self::Depths => "Depths",
            Self::Discoveries => "Discoveries",
            Self::Sky => "Sky",
        }
    }
}

/// Altitude above the local ground (tiles) that earns [`Milestone::HighFlyer`]: just past a
/// stock tank's straight climb, so it needs the first tank upgrade.
pub const HIGH_FLYER_ALTITUDE_TILES: i32 = 120;

/// Altitude (rows above row 0) a full-tank climb must top to count as reaching the low sky
/// isles: 40 rows into their band, where the lowest isles' turf is.
pub const ISLAND_REACH_ROWS: i32 = crate::worldgen::sky::LOW_ISLES_BOTTOM_ROWS + 40;
/// Altitude a full-tank climb must top to cross the dead-air gap: the sky continents' floor.
pub const DEAD_AIR_REACH_ROWS: i32 = crate::worldgen::sky::CONTINENTS_BOTTOM_ROWS;

/// How high `player`'s rig climbs on a full tank in still air (T16's measured climb table,
/// conservatively rounded down between its engine and intake tiers).
#[must_use]
pub fn climb_reach(player: &Player) -> i32 {
    crate::climb::reach(
        player.fuel_tank_level,
        player.engine_level,
        player.hazard_gear.altitude_intake,
    )
}

/// Whether `player`'s rig climbs into the low sky isles on a full tank.
#[must_use]
pub fn island_capable(player: &Player) -> bool {
    climb_reach(player) >= ISLAND_REACH_ROWS
}

/// Whether `player`'s tank, engine and altitude intake carry the rig over the dead-air gap to
/// the sky continents and the Moon: the Act I -> II unlock.
#[must_use]
pub fn crosses_dead_air(player: &Player) -> bool {
    climb_reach(player) >= DEAD_AIR_REACH_ROWS
}

/// What the world knows about a player this tick beyond the [`Player`] itself.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MilestoneFacts {
    /// The deepest band the player's rig has been in (by [`Band::index`]), if underground ever.
    pub deepest_band: Option<usize>,
    /// The player owns at least one base claim.
    pub owns_claim: bool,
    /// The world has a sky to reach (generator V8+): the sky gates count only there.
    pub has_sky: bool,
}

/// The core upgrade lines (the ones that reach [`MAX_UPGRADE_LEVEL`]).
pub const CORE_LINES: [UpgradeKind; 6] = [
    UpgradeKind::Drill,
    UpgradeKind::FuelTank,
    UpgradeKind::CargoBay,
    UpgradeKind::Engine,
    UpgradeKind::Hull,
    UpgradeKind::Radiator,
];

impl Milestone {
    /// Every milestone, in progression-screen order.
    pub const ALL: [Self; 19] = [
        Self::FirstUpgrade,
        Self::DrillTier5,
        Self::EngineTier5,
        Self::DrillTier10,
        Self::EngineTier10,
        Self::MaxedLine,
        Self::FullyLoaded,
        Self::ReachedStone,
        Self::FirstDeepLayer,
        Self::ReachedMachineLayer,
        Self::ReachedMantle,
        Self::FirstArtifact,
        Self::FirstCave,
        Self::FirstVillage,
        Self::FirstKill,
        Self::FirstClaim,
        Self::HighFlyer,
        Self::IslandCapableTank,
        Self::ActTwoUnlocked,
    ];

    /// Display name.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::FirstUpgrade => "Fresh Iron",
            Self::DrillTier5 => "Plasma Bite",
            Self::DrillTier10 => "Rock Eater",
            Self::EngineTier5 => "Afterburn",
            Self::EngineTier10 => "Ion Rider",
            Self::MaxedLine => "Top of the Line",
            Self::FullyLoaded => "Fully Loaded",
            Self::ReachedStone => "Into the Grey",
            Self::FirstDeepLayer => "First Deep Layer",
            Self::ReachedMachineLayer => "The Old Machines",
            Self::ReachedMantle => "Mantle Diver",
            Self::FirstArtifact => "Relic Hunter",
            Self::FirstCave => "Spelunker",
            Self::FirstVillage => "Trail Blazer",
            Self::FirstKill => "Pest Control",
            Self::FirstClaim => "Stake a Claim",
            Self::HighFlyer => "High Flyer",
            Self::IslandCapableTank => "Island Hopper",
            Self::ActTwoUnlocked => "The Sky Opens",
        }
    }

    /// How it is earned.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::FirstUpgrade => "Buy your first upgrade at the shop.",
            Self::DrillTier5 => "Fit a tier 5 drill.",
            Self::DrillTier10 => "Fit a tier 10 drill.",
            Self::EngineTier5 => "Fit a tier 5 engine.",
            Self::EngineTier10 => "Fit a tier 10 engine.",
            Self::MaxedLine => "Max out any core upgrade line.",
            Self::FullyLoaded => "Max out drill, tank, bay, engine, hull and radiator.",
            Self::ReachedStone => "Drill down into the Grey Stone.",
            Self::FirstDeepLayer => "Drill down into the Crystal Faults, the first deep layer.",
            Self::ReachedMachineLayer => "Drill down into the Ancient Machine Layer.",
            Self::ReachedMantle => "Drill down into the Mantle Storm Zone.",
            Self::FirstArtifact => "Haul an artifact in your cargo bay.",
            Self::FirstCave => "Find a cave ecosystem.",
            Self::FirstVillage => "Roll into a village.",
            Self::FirstKill => "Defeat an enemy.",
            Self::FirstClaim => "Stake a base claim with a claim beacon.",
            Self::HighFlyer => "Fly 120 tiles above the ground below you.",
            Self::IslandCapableTank => "Fit a fuel tank that can reach the low sky isles.",
            Self::ActTwoUnlocked => {
                "Fit a tank and intake that can cross the dead-air gap to the sky lands."
            }
        }
    }

    #[must_use]
    pub const fn group(self) -> MilestoneGroup {
        match self {
            Self::FirstUpgrade
            | Self::DrillTier5
            | Self::DrillTier10
            | Self::EngineTier5
            | Self::EngineTier10
            | Self::MaxedLine
            | Self::FullyLoaded => MilestoneGroup::Upgrades,
            Self::ReachedStone
            | Self::FirstDeepLayer
            | Self::ReachedMachineLayer
            | Self::ReachedMantle => MilestoneGroup::Depths,
            Self::FirstArtifact
            | Self::FirstCave
            | Self::FirstVillage
            | Self::FirstKill
            | Self::FirstClaim => MilestoneGroup::Discoveries,
            Self::HighFlyer | Self::IslandCapableTank | Self::ActTwoUnlocked => MilestoneGroup::Sky,
        }
    }

    #[must_use]
    pub const fn medal(self) -> Medal {
        match self {
            Self::FirstUpgrade
            | Self::ReachedStone
            | Self::FirstArtifact
            | Self::FirstCave
            | Self::FirstVillage
            | Self::FirstKill
            | Self::FirstClaim
            | Self::HighFlyer => Medal::Bronze,
            Self::DrillTier5
            | Self::EngineTier5
            | Self::FirstDeepLayer
            | Self::IslandCapableTank => Medal::Silver,
            Self::DrillTier10
            | Self::EngineTier10
            | Self::MaxedLine
            | Self::ReachedMachineLayer
            | Self::ReachedMantle => Medal::Gold,
            Self::FullyLoaded | Self::ActTwoUnlocked => Medal::Platinum,
        }
    }

    /// The cosmetics this milestone unlocks (each is unlocked by exactly one milestone).
    #[must_use]
    pub const fn rewards(self) -> &'static [CosmeticItem] {
        match self {
            Self::FirstUpgrade => &[CosmeticItem::Decal(Decal::Wrench)],
            Self::DrillTier5 => &[CosmeticItem::Drill(DrillStyle::Corkscrew)],
            Self::DrillTier10 => &[CosmeticItem::Pattern(HullPattern::Tiger)],
            Self::EngineTier5 => &[CosmeticItem::Flame(FlameColor::Ember)],
            Self::EngineTier10 => &[CosmeticItem::Flame(FlameColor::Plasma)],
            Self::MaxedLine => &[CosmeticItem::Paint(PaintColor::Chrome)],
            Self::FullyLoaded => &[CosmeticItem::Pattern(HullPattern::Starfield)],
            Self::ReachedStone => &[CosmeticItem::Paint(PaintColor::Jade)],
            Self::FirstDeepLayer => &[CosmeticItem::Roof(RoofAttachment::Trophy)],
            Self::ReachedMachineLayer => &[CosmeticItem::Paint(PaintColor::Abyss)],
            Self::ReachedMantle => &[CosmeticItem::Paint(PaintColor::Magma)],
            Self::FirstArtifact => &[CosmeticItem::Decal(Decal::Gem)],
            Self::FirstCave => &[CosmeticItem::Paint(PaintColor::Glowcap)],
            Self::FirstVillage => &[CosmeticItem::Rear(RearAttachment::Lantern)],
            Self::FirstKill => &[CosmeticItem::Decal(Decal::Fang)],
            Self::FirstClaim => &[CosmeticItem::Decal(Decal::Flag)],
            Self::HighFlyer => &[CosmeticItem::Decal(Decal::Wings)],
            Self::IslandCapableTank => &[CosmeticItem::Flame(FlameColor::Aether)],
            Self::ActTwoUnlocked => &[CosmeticItem::Decal(Decal::Moon)],
        }
    }

    /// The milestone that unlocks `item`, if it is a milestone reward.
    #[must_use]
    pub fn reward_for(item: CosmeticItem) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|milestone| milestone.rewards().contains(&item))
    }

    /// Whether `player` (with the world's `facts` about them) has met this milestone.
    #[must_use]
    pub fn met(self, player: &Player, facts: &MilestoneFacts) -> bool {
        let reached = |band: Band| {
            facts
                .deepest_band
                .is_some_and(|deepest| deepest >= band.index())
        };
        match self {
            Self::FirstUpgrade => {
                player.milestones.records.upgrades_bought > 0
                    || UpgradeKind::ALL
                        .into_iter()
                        .any(|kind| current_level(player, kind) > kind.starting_level())
            }
            Self::DrillTier5 => player.drill_strength >= 5,
            Self::DrillTier10 => player.drill_strength >= 10,
            Self::EngineTier5 => player.engine_level >= 5,
            Self::EngineTier10 => player.engine_level >= 10,
            Self::MaxedLine => CORE_LINES
                .into_iter()
                .any(|kind| current_level(player, kind) >= MAX_UPGRADE_LEVEL),
            Self::FullyLoaded => CORE_LINES
                .into_iter()
                .all(|kind| current_level(player, kind) >= MAX_UPGRADE_LEVEL),
            Self::ReachedStone => reached(Band::Stone),
            Self::FirstDeepLayer => reached(Band::Deep(crate::terrain::DeepStratum::CrystalFaults)),
            Self::ReachedMachineLayer => {
                reached(Band::Deep(crate::terrain::DeepStratum::AncientMachineLayer))
            }
            Self::ReachedMantle => {
                reached(Band::Deep(crate::terrain::DeepStratum::MantleStormZone))
            }
            Self::FirstArtifact => {
                player.milestones.records.artifacts_found > 0
                    || player.cargo_bay.artifacts().next().is_some()
            }
            Self::FirstCave => !player.journal.caves.is_empty(),
            Self::FirstVillage => !player.villages.visited.is_empty(),
            Self::FirstKill => player.milestones.records.enemies_defeated > 0,
            Self::FirstClaim => facts.owns_claim,
            Self::HighFlyer => {
                player.milestones.records.highest_altitude >= HIGH_FLYER_ALTITUDE_TILES
            }
            Self::IslandCapableTank => facts.has_sky && island_capable(player),
            Self::ActTwoUnlocked => facts.has_sky && crosses_dead_air(player),
        }
    }

    /// Every milestone `player` has met but not yet earned, in [`Self::ALL`] order.
    #[must_use]
    pub fn newly_met(player: &Player, facts: &MilestoneFacts) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|milestone| !player.milestones.has(*milestone))
            .filter(|milestone| milestone.met(player, facts))
            .collect()
    }
}

/// A player's personal bests, shown on the progression screen (and read by milestones).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct MilestoneRecords {
    /// The deepest tile row the rig has reached (world rows; 0 is the classic surface).
    pub deepest_row: i32,
    /// The deepest band reached (by [`Band::index`]), once underground.
    pub deepest_band: Option<u8>,
    /// The most tiles the rig has flown above the ground below it.
    pub highest_altitude: i32,
    /// Enemies this player defeated.
    pub enemies_defeated: u32,
    /// Artifacts this player picked up.
    pub artifacts_found: u32,
    /// Upgrades this player bought at the shop.
    pub upgrades_bought: u32,
}

/// The milestones a player earned (private: replicated to the owner in the loadout, saved, and
/// parked with the rig).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct MilestoneLog {
    /// Earned milestones and the simulation tick each was earned on.
    pub earned: BTreeMap<Milestone, u64>,
    pub records: MilestoneRecords,
    /// Whether the first check ran (see the module docs: it records silently).
    pub primed: bool,
}

impl MilestoneLog {
    #[must_use]
    pub fn has(&self, milestone: Milestone) -> bool {
        self.earned.contains_key(&milestone)
    }

    /// Earned milestones.
    #[must_use]
    pub fn count(&self) -> usize {
        self.earned.len()
    }
}

impl Player {
    /// Records `milestone` as earned on `tick` and unlocks its reward cosmetics. Returns `false`
    /// (and changes nothing) if it was already earned. Call it only from inside the fixed tick.
    pub fn earn_milestone(&mut self, milestone: Milestone, tick: u64) -> bool {
        if self.milestones.has(milestone) {
            return false;
        }
        self.milestones.earned.insert(milestone, tick);
        for item in milestone.rewards() {
            self.unlock_cosmetic(*item);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn rig() -> Player {
        Player::new(0.0, 0.0)
    }

    #[test]
    fn a_fresh_rig_has_met_nothing() {
        assert!(Milestone::newly_met(&rig(), &MilestoneFacts::default()).is_empty());
    }

    #[test]
    fn every_reward_is_a_locked_item_awarded_once() {
        let mut seen = BTreeSet::new();
        for milestone in Milestone::ALL {
            assert!(
                !milestone.rewards().is_empty(),
                "{milestone:?} rewards nothing"
            );
            for item in milestone.rewards() {
                assert!(!item.is_starter(), "{item:?} is a starter item");
                assert!(seen.insert(*item), "{item:?} is awarded twice");
                assert_eq!(Milestone::reward_for(*item), Some(milestone));
                assert!(!item.is_vendor_stock(), "{item:?} is sold by vendors");
            }
        }
    }

    #[test]
    fn every_milestone_is_listed_once() {
        let unique: BTreeSet<_> = Milestone::ALL.into_iter().collect();
        assert_eq!(unique.len(), Milestone::ALL.len());
        for group in MilestoneGroup::ALL {
            assert!(
                Milestone::ALL
                    .iter()
                    .any(|milestone| milestone.group() == group)
            );
        }
    }

    #[test]
    fn tier_milestones_follow_the_fitted_tiers() {
        let mut player = rig();
        player.drill_strength = 5;
        let met = Milestone::newly_met(&player, &MilestoneFacts::default());
        assert_eq!(met, vec![Milestone::FirstUpgrade, Milestone::DrillTier5]);
        player.drill_strength = 15;
        player.engine_level = 10;
        let met = Milestone::newly_met(&player, &MilestoneFacts::default());
        assert!(met.contains(&Milestone::MaxedLine));
        assert!(met.contains(&Milestone::EngineTier10));
        assert!(!met.contains(&Milestone::FullyLoaded));
        for kind in CORE_LINES {
            while current_level(&player, kind) < MAX_UPGRADE_LEVEL {
                match kind {
                    UpgradeKind::Drill => player.drill_strength += 1,
                    UpgradeKind::FuelTank => player.fuel_tank_level += 1,
                    UpgradeKind::CargoBay => player.cargo_bay_level += 1,
                    UpgradeKind::Engine => player.engine_level += 1,
                    UpgradeKind::Hull => player.hull_level += 1,
                    _ => player.radiator_level += 1,
                }
            }
        }
        assert!(Milestone::FullyLoaded.met(&player, &MilestoneFacts::default()));
    }

    #[test]
    fn depth_milestones_follow_the_deepest_band() {
        let player = rig();
        let facts = |band: Band| MilestoneFacts {
            deepest_band: Some(band.index()),
            ..MilestoneFacts::default()
        };
        assert!(!Milestone::ReachedStone.met(&player, &facts(Band::Clay)));
        assert!(Milestone::ReachedStone.met(&player, &facts(Band::Stone)));
        let crystal = facts(Band::Deep(crate::terrain::DeepStratum::CrystalFaults));
        assert!(Milestone::FirstDeepLayer.met(&player, &crystal));
        assert!(!Milestone::ReachedMantle.met(&player, &crystal));
        assert!(Milestone::ReachedMantle.met(
            &player,
            &facts(Band::Deep(crate::terrain::DeepStratum::MantleStormZone))
        ));
    }

    #[test]
    fn the_sky_gates_need_the_tank_and_the_intake() {
        let mut player = rig();
        assert!(
            !island_capable(&player),
            "a stock rig stays under the isles"
        );
        player.fuel_tank_level = 2;
        assert!(
            island_capable(&player),
            "the first tank reaches the low isles"
        );
        player.fuel_tank_level = 15;
        player.engine_level = 15;
        assert!(
            !crosses_dead_air(&player),
            "the best tank and engine alone die in the dead air"
        );
        player.hazard_gear.altitude_intake = 4;
        assert!(crosses_dead_air(&player), "intakes carry it over");
        let sky = MilestoneFacts {
            has_sky: true,
            ..MilestoneFacts::default()
        };
        let met = Milestone::newly_met(&player, &sky);
        assert!(met.contains(&Milestone::IslandCapableTank));
        assert!(met.contains(&Milestone::ActTwoUnlocked));
        let ground = Milestone::newly_met(&player, &MilestoneFacts::default());
        assert!(
            !ground.contains(&Milestone::ActTwoUnlocked),
            "worlds without a sky award no sky gates"
        );
    }

    #[test]
    fn earning_unlocks_the_rewards_once() {
        let mut player = rig();
        assert!(player.earn_milestone(Milestone::FirstDeepLayer, 42));
        assert!(!player.earn_milestone(Milestone::FirstDeepLayer, 43));
        assert_eq!(
            player.milestones.earned.get(&Milestone::FirstDeepLayer),
            Some(&42)
        );
        assert!(
            player
                .cosmetic_unlocks
                .has(CosmeticItem::Roof(RoofAttachment::Trophy))
        );
        assert!(
            !Milestone::newly_met(&player, &MilestoneFacts::default())
                .contains(&Milestone::FirstDeepLayer)
        );
    }

    #[test]
    fn logs_round_trip_and_old_logs_load() {
        let mut player = rig();
        player.earn_milestone(Milestone::HighFlyer, 7);
        player.milestones.records.deepest_row = 321;
        player.milestones.primed = true;
        let json = serde_json::to_string(&player.milestones).unwrap();
        assert_eq!(
            serde_json::from_str::<MilestoneLog>(&json).unwrap(),
            player.milestones
        );
        let empty: MilestoneLog = serde_json::from_str("{}").unwrap();
        assert!(!empty.primed && empty.earned.is_empty());
    }
}
