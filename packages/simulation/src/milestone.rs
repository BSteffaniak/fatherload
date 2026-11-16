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
//! (tank, engine and altitude intake that can cross the dead-air gap). Both read the measured
//! climb table of the current fuel curve ([`crate::climb::reach`]) against the bands of the
//! world's own sky ([`MilestoneFacts::sky_bands`], from
//! [`crate::worldgen::GeneratorVersion::sky_bands`]): [`island_capable`] is a climb that tops
//! [`SkyBands::island_reach_rows`] (a little into the low isles' band), and
//! [`crosses_dead_air`] one that tops [`SkyBands::dead_air_reach_rows`] (the floor of the sky
//! continents, above the whole dead-air gap). So the gates always say what the rig can really
//! do in this world: V8 skies were calibrated on the smaller first fuel curve, so bigger tanks
//! open their bands earlier than V8 planned. They are only awarded on worlds with a sky.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    cosmetics::{
        CosmeticItem, Decal, DrillStyle, FlameColor, HullPattern, PaintColor, RearAttachment,
        RoofAttachment,
    },
    economy::{MAX_UPGRADE_LEVEL, UpgradeKind, current_level},
    player::Player,
    worldgen::{Band, sky::SkyBands},
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
    /// Fitted a fuel tank that can reach the low sky isles ([`island_capable`]) of the
    /// world's sky.
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

/// Altitude above the local ground (tiles) that earns [`Milestone::HighFlyer`].
///
/// Just past a stock rig's straight climb on a full tank (about 400 tiles, the balance report's
/// climb table), so it needs the first tank or engine upgrade.
pub const HIGH_FLYER_ALTITUDE_TILES: i32 = 420;

/// How high `player`'s rig climbs on a full tank in still air (T16's measured climb table,
/// conservatively rounded down between its engine and intake tiers).
#[must_use]
pub fn climb_reach(player: &Player, bands: &SkyBands) -> i32 {
    crate::climb::reach_in(
        bands.climb,
        player.fuel_tank_level,
        player.engine_level,
        player.hazard_gear.altitude_intake,
    )
}

/// Whether `player`'s rig climbs into the low sky isles of a sky with `bands` on a full tank.
#[must_use]
pub fn island_capable(player: &Player, bands: &SkyBands) -> bool {
    climb_reach(player, bands) >= bands.island_reach_rows()
}

/// Whether `player`'s tank, engine and altitude intake carry the rig over the dead-air gap of a
/// sky with `bands` to the sky continents and the Moon: the Act I -> II unlock.
#[must_use]
pub fn crosses_dead_air(player: &Player, bands: &SkyBands) -> bool {
    climb_reach(player, bands) >= bands.dead_air_reach_rows()
}

/// The most fuel `player`'s rig may be filled to while it is `altitude_rows` above row 0.
///
/// The honest-gate rule for refuelling in the sky (WP-SKY rest stops): village counters,
/// people's trades and base fuel stations all read it.
///
/// Sky towns hang high in the cloud archipelago, just under the dead-air gap: a full tank
/// bought there would carry rigs that cannot cross on a full tank from the ground (tier 10-12
/// tanks with intakes) over the gap, and [`crosses_dead_air`] would stop being the Act II gate.
/// So in the island bands of the world's sky `bands` (from [`SkyBands::low_isles`] up to the
/// continents' floor) a rig that cannot cross the gap is only topped up to what a full-tank
/// climb from the ground would still hold here, estimated from the climb table as the share of
/// the rig's [`climb_reach`] still above it. Fuel per row only grows with altitude (thinner
/// air), so this never leaves a rig better off than the climb the gate measured
/// (`world::sky_isle_tests::sky_refuels_never_open_the_dead_air_gap`); every such rig may still
/// fill [`SKY_REFUEL_FLOOR_PERCENT`] of its tank. Rigs that already cross the gap, and anything
/// below the low isles or above the continents' floor, fill right up.
#[must_use]
pub fn sky_refuel_level(player: &Player, altitude_rows: i32, bands: Option<&SkyBands>) -> f32 {
    let capacity = player.fuel_capacity;
    let Some(bands) = bands else {
        return capacity;
    };
    if !(bands.low_isles..bands.dead_air_reach_rows()).contains(&altitude_rows)
        || crosses_dead_air(player, bands)
    {
        return capacity;
    }
    // The climb harness starts at rest 100 rows up (`world::climb_tests::START_ALTITUDE_ROWS`).
    let start = 100;
    let reach = climb_reach(player, bands);
    #[allow(
        clippy::cast_precision_loss,
        reason = "altitudes are a few thousand rows"
    )]
    let share = (reach - altitude_rows).max(0) as f32 / (reach - start).max(1) as f32;
    // A rig that rode updrafts or jet streams higher than its tank could lift it still gets a
    // little: enough to hop to a nearby isle or ease down, far short of the gap.
    let floor = capacity * SKY_REFUEL_FLOOR_PERCENT / 100.0;
    (capacity * share.clamp(0.0, 1.0)).max(floor).floor()
}

/// The share of its tank (percent) a rig that cannot cross the dead-air gap may always be
/// filled to in the sky ([`sky_refuel_level`]).
pub const SKY_REFUEL_FLOOR_PERCENT: f32 = 25.0;

/// What the world knows about a player this tick beyond the [`Player`] itself.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MilestoneFacts {
    /// The deepest band the player's rig has been in (by [`Band::index`]), if underground ever.
    pub deepest_band: Option<usize>,
    /// The player owns at least one base claim.
    pub owns_claim: bool,
    /// The bands of the world's sky (`None` without one: the sky gates count only with a sky,
    /// against its own bands).
    pub sky_bands: Option<SkyBands>,
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
            Self::HighFlyer => "Fly 420 tiles above the ground below you.",
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
            Self::IslandCapableTank => facts
                .sky_bands
                .is_some_and(|bands| island_capable(player, &bands)),
            Self::ActTwoUnlocked => facts
                .sky_bands
                .is_some_and(|bands| crosses_dead_air(player, &bands)),
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
        // A sky calibrated on the current fuel curve (the next generator's bands).
        let bands = crate::worldgen::sky::NEXT_SKY_BANDS;
        let mut player = rig();
        assert!(
            !island_capable(&player, &bands),
            "a stock rig stays under the isles"
        );
        player.fuel_tank_level = 2;
        assert!(
            island_capable(&player, &bands),
            "the first tank reaches the low isles"
        );
        player.fuel_tank_level = 15;
        player.engine_level = 15;
        assert!(
            !crosses_dead_air(&player, &bands),
            "the best tank and engine alone die in the dead air"
        );
        player.hazard_gear.altitude_intake = 4;
        assert!(crosses_dead_air(&player, &bands), "intakes carry it over");
        let sky = MilestoneFacts {
            sky_bands: Some(bands),
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
    fn v8_gates_read_the_v8_bands() {
        // V8 skies were calibrated on the smaller first fuel curve: the gates compare what the
        // rig really climbs with the world's own bands, so bigger tanks open them earlier.
        let v8 = crate::worldgen::sky::V8_SKY_BANDS;
        let mut player = rig();
        assert!(
            island_capable(&player, &v8),
            "a stock tank now reaches V8's isles"
        );
        player.fuel_tank_level = 15;
        player.engine_level = 15;
        assert!(
            !crosses_dead_air(&player, &v8),
            "without intakes nothing crosses V8's dead air either"
        );
        player.fuel_tank_level = 10;
        player.hazard_gear.altitude_intake = 2;
        assert!(crosses_dead_air(&player, &v8), "but far less kit does");
        let v8_facts = MilestoneFacts {
            sky_bands: Some(v8),
            ..MilestoneFacts::default()
        };
        assert!(Milestone::ActTwoUnlocked.met(&player, &v8_facts));
        let next_facts = MilestoneFacts {
            sky_bands: Some(crate::worldgen::sky::NEXT_SKY_BANDS),
            ..MilestoneFacts::default()
        };
        assert!(
            !Milestone::ActTwoUnlocked.met(&player, &next_facts),
            "the same rig is far from the next generator's continents"
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
