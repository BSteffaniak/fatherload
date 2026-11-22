//! Progression to the Heart (expansion plan Part III, F10).
//!
//! The Descent layout the depth rules read, which upgrade ladder a world plays
//! ([`ProgressionRules`]), the deep tiers of the core upgrade lines, the eight rig systems
//! that gate the Descents, and the fuel, hull, cargo and price curves to about 13,000 rows.
//!
//! **Classic worlds never change.** Every generator up to V10 plays [`ProgressionRules::Classic`]:
//! the fifteen-tier ladders, the classic prices and no rig systems on sale. The Descent rules
//! ([`ProgressionRules::Descent`]) switch on for every generator after V10
//! ([`ProgressionRules::for_generator`] compares versions, so generator V11 gets them the moment
//! it exists) and for scenarios that ask for them before play
//! ([`crate::WorldState::set_progression_rules`]).
//!
//! **The layout is an adapter.** The depth rules here read a [`DescentLayout`]: the eight
//! Descents and their 41 strata as row ranges below the ground line. Until the stratum registry
//! (F8) lands, [`PlanLayout`] models expansion plan §14's row table; the registry implements the
//! trait and [`layout_for`] is the one place that picks it.

use serde::{Deserialize, Serialize};

use crate::worldgen::{GeneratorVersion, WorldSettings};

/// Which upgrade ladder and depth rules a world plays.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum ProgressionRules {
    /// The fifteen-tier ladders of every world up to generator V10 (unchanged).
    #[default]
    Classic,
    /// The Deep Descent: deep tiers per Descent, the rig systems and the Descent gates.
    Descent,
}

impl ProgressionRules {
    /// The rules a new world of `generator` plays: Descent for every generator after V10.
    #[must_use]
    pub fn for_generator(generator: GeneratorVersion) -> Self {
        if generator > GeneratorVersion::V10 {
            Self::Descent
        } else {
            Self::Classic
        }
    }

    /// Whether these are the Deep Descent rules.
    #[must_use]
    pub const fn is_descent(self) -> bool {
        matches!(self, Self::Descent)
    }
}

/// One Descent (chapter) of the dig: its index (0 = Descent I), name and row range below the
/// ground line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescentSpan {
    pub index: u8,
    pub name: &'static str,
    /// First row of the Descent (rows below the ground line).
    pub top: i32,
    /// First row below it.
    pub bottom: i32,
}

impl DescentSpan {
    /// The Descent's roman numeral ("I" to "VIII").
    #[must_use]
    pub const fn numeral(self) -> &'static str {
        numeral(self.index)
    }

    /// Rows from top to bottom.
    #[must_use]
    pub const fn rows(self) -> i32 {
        self.bottom - self.top
    }
}

/// One stratum of a Descent: its name, the Descent it belongs to and its row range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StratumSpan {
    pub descent: u8,
    pub name: &'static str,
    pub top: i32,
    pub bottom: i32,
}

impl StratumSpan {
    /// Rows from top to bottom.
    #[must_use]
    pub const fn rows(self) -> i32 {
        self.bottom - self.top
    }
}

/// The Descent and stratum row ranges the depth rules read (the F8 stratum registry implements
/// it; [`PlanLayout`] models the plan's table until then).
pub trait DescentLayout {
    /// The eight Descents, top to bottom, contiguous.
    fn descents(&self) -> &[DescentSpan];
    /// Every stratum, top to bottom, contiguous.
    fn strata(&self) -> Vec<StratumSpan>;

    /// The Descent `row` (rows below the ground line) lies in, clamped to the first and last.
    fn descent_at(&self, row: i32) -> u8 {
        let descents = self.descents();
        descents
            .iter()
            .find(|span| row < span.bottom)
            .or_else(|| descents.last())
            .map_or(0, |span| span.index)
    }

    /// The span of Descent `index`, if it exists.
    fn descent(&self, index: u8) -> Option<DescentSpan> {
        self.descents().get(usize::from(index)).copied()
    }

    /// Row of the ground line's floor: the bottom of the last Descent.
    fn floor_row(&self) -> i32 {
        self.descents().last().map_or(0, |span| span.bottom)
    }
}

/// Number of Descents.
pub const DESCENT_COUNT: usize = 8;
/// Number of strata across all Descents.
pub const STRATUM_COUNT: usize = 41;

/// Expansion plan §14's Descents: name, top row, bottom row and stratum names.
const PLAN: [(&str, i32, i32, &[&str]); DESCENT_COUNT] = [
    (
        "The Crust",
        0,
        1_100,
        &[
            "Topsoil",
            "Clay Beds",
            "Grey Stone",
            "Slate Shelves",
            "Basalt Columns",
        ],
    ),
    (
        "The Old Deep",
        1_100,
        2_700,
        &[
            "Crystal Faults",
            "Fossil Oceans",
            "Pressure Cathedrals",
            "Radioactive Hollow",
            "Ancient Machine Layer",
            "Void Geode Fields",
            "Mantle Storm Zone",
        ],
    ),
    (
        "The Living Dark",
        2_700,
        4_200,
        &[
            "Fungal Forest",
            "Root Labyrinth",
            "The Hive",
            "Flesh Caverns",
            "Glow Sea",
        ],
    ),
    (
        "The Drowned and Frozen",
        4_200,
        5_800,
        &[
            "Underground Ocean",
            "Brine Glaciers",
            "Geyser Fields",
            "Merrow City",
            "Pearl Abyss",
        ],
    ),
    (
        "The Hollow World",
        5_800,
        7_600,
        &[
            "Inner Sky",
            "Megafauna Jungle",
            "Upside-Down Isles",
            "The Sunforge",
        ],
    ),
    (
        "The Strange Layers",
        7_600,
        9_400,
        &[
            "Mirror Strata",
            "Gravity Shear",
            "Pixel Rot",
            "Time Sludge",
            "Echo Caves",
            "Prism Desert",
        ],
    ),
    (
        "The Ancients",
        9_400,
        11_400,
        &[
            "Buried Megacity",
            "Engine Halls",
            "Archive of Stars",
            "Sleeping Garrison",
            "The Seal Gate",
        ],
    ),
    (
        "The Heart",
        11_400,
        13_000,
        &[
            "Dark Matter Veins",
            "Singularity Fields",
            "Core Ocean",
            "The Heart",
        ],
    ),
];

const PLAN_DESCENTS: [DescentSpan; DESCENT_COUNT] = {
    let mut spans = [DescentSpan {
        index: 0,
        name: "",
        top: 0,
        bottom: 0,
    }; DESCENT_COUNT];
    let mut index = 0;
    while index < DESCENT_COUNT {
        let (name, top, bottom, _) = PLAN[index];
        spans[index] = DescentSpan {
            #[allow(clippy::cast_possible_truncation, reason = "eight Descents")]
            index: index as u8,
            name,
            top,
            bottom,
        };
        index += 1;
    }
    spans
};

/// The plan's §14 row table as a [`DescentLayout`] (a mean V11 world): each Descent's rows
/// shared evenly between its strata.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlanLayout;

impl DescentLayout for PlanLayout {
    fn descents(&self) -> &[DescentSpan] {
        &PLAN_DESCENTS
    }

    fn strata(&self) -> Vec<StratumSpan> {
        let mut strata = Vec::with_capacity(STRATUM_COUNT);
        for (index, (_, top, bottom, names)) in PLAN.iter().enumerate() {
            let count = i32::try_from(names.len()).unwrap_or(1).max(1);
            let rows = bottom - top;
            for (slot, name) in (0..).zip(names.iter()) {
                strata.push(StratumSpan {
                    descent: u8::try_from(index).unwrap_or(u8::MAX),
                    name,
                    top: top + rows * slot / count,
                    bottom: top + rows * (slot + 1) / count,
                });
            }
        }
        strata
    }
}

/// The Descent layout a world's depth rules read. The stratum registry (F8) replaces the plan
/// table here for V11 worlds; classic worlds never consult it.
#[must_use]
pub fn layout_for(_settings: &WorldSettings) -> &'static dyn DescentLayout {
    &PlanLayout
}

/// Roman numeral of Descent `index` (0 = "I").
#[must_use]
pub const fn numeral(index: u8) -> &'static str {
    match index {
        0 => "I",
        1 => "II",
        2 => "III",
        3 => "IV",
        4 => "V",
        5 => "VI",
        6 => "VII",
        _ => "VIII",
    }
}

// --- Rig systems ------------------------------------------------------------------------------

/// The eight rig systems of the Deep Descent.
///
/// One per Descent from II on, plus Lamp II and the Sonar Suite as companions: each is a
/// one-tier upgrade line on Descent worlds
/// ([`crate::economy::UpgradeKind::rig_system`]), stored as a bit in
/// [`crate::protection::HazardGear::rig_systems`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum RigSystem {
    /// Descent II's reward: survives the crushing depths of the Living Dark.
    PressureHull,
    /// Descent III's reward: spore and acid immunity, needed for the Drowned and Frozen.
    BioFilter,
    /// Descent III's companion: a brighter, wider headlamp for the Glow Sea and below.
    LampII,
    /// Descent IV's companion: echo mapping (the Echo Caves need it).
    SonarSuite,
    /// Descent IV's reward: frost- and pressure-proof plating for the Hollow World's way in.
    CryoHull,
    /// Descent V's reward: holds the rig to any surface for a while (the Strange Layers).
    GravityAnchor,
    /// Descent VI's reward: drills through builder alloy (the Ancients).
    PhaseDrill,
    /// Descent VII's reward: opens the Heart.
    HeartKey,
}

impl RigSystem {
    /// Every rig system, in shop order.
    pub const ALL: [Self; 8] = [
        Self::PressureHull,
        Self::BioFilter,
        Self::LampII,
        Self::SonarSuite,
        Self::CryoHull,
        Self::GravityAnchor,
        Self::PhaseDrill,
        Self::HeartKey,
    ];

    /// The system's bit in [`crate::protection::HazardGear::rig_systems`].
    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << (self as u8)
    }

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PressureHull => "Pressure Hull",
            Self::BioFilter => "Bio-Filter",
            Self::LampII => "Lamp II",
            Self::SonarSuite => "Sonar Suite",
            Self::CryoHull => "Cryo Hull",
            Self::GravityAnchor => "Gravity Anchor",
            Self::PhaseDrill => "Phase Drill",
            Self::HeartKey => "Heart Key",
        }
    }

    /// What the system does (its first effects; later waves deepen them).
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::PressureHull => {
                "Holds the crushing pressure past the Old Deep: the Living Dark and below"
            }
            Self::BioFilter => {
                "Spore and acid filters: halves gas damage and opens the Drowned and Frozen"
            }
            Self::LampII => "A second, wider headlamp for the darkest strata",
            Self::SonarSuite => "Echo mapping: scanner pings reach much further",
            Self::CryoHull => "Frost- and pressure-proof plating: the way into the Hollow World",
            Self::GravityAnchor => "Holds the rig to any surface: the way into the Strange Layers",
            Self::PhaseDrill => "Drills builder alloy: the way into the Ancients",
            Self::HeartKey => "The builders' key: opens the Heart",
        }
    }

    /// The Descent this system opens (entering it without the system hurts, see
    /// [`descent_gate`]), or `None` for the companion systems.
    #[must_use]
    pub const fn gates_descent(self) -> Option<u8> {
        match self {
            Self::PressureHull => Some(2),
            Self::BioFilter => Some(3),
            Self::CryoHull => Some(4),
            Self::GravityAnchor => Some(5),
            Self::PhaseDrill => Some(6),
            Self::HeartKey => Some(7),
            Self::LampII | Self::SonarSuite => None,
        }
    }

    /// The Descent whose last stratum puts this system on sale ("you felt the need before you
    /// can buy it"): the one above the Descent it opens.
    #[must_use]
    pub const fn sold_from_descent(self) -> u8 {
        match self {
            Self::PressureHull => 1,
            Self::BioFilter | Self::LampII => 2,
            Self::SonarSuite | Self::CryoHull => 3,
            Self::GravityAnchor => 4,
            Self::PhaseDrill => 5,
            Self::HeartKey => 6,
        }
    }

    /// Shop price in credits (sized against the deep income curve the descent harness
    /// measures).
    #[must_use]
    pub const fn price(self) -> u32 {
        match self {
            Self::PressureHull => 60_000,
            Self::BioFilter => 150_000,
            Self::LampII => 90_000,
            Self::SonarSuite => 220_000,
            Self::CryoHull => 320_000,
            Self::GravityAnchor => 600_000,
            Self::PhaseDrill => 1_100_000,
            Self::HeartKey => 1_900_000,
        }
    }

    /// Row (below the ground line) from which the shop sells this system on `layout`: the top
    /// of the last stratum of [`Self::sold_from_descent`].
    #[must_use]
    pub fn sold_from_row(self, layout: &dyn DescentLayout) -> i32 {
        let descent = self.sold_from_descent();
        layout
            .strata()
            .iter()
            .rev()
            .find(|stratum| stratum.descent == descent)
            .map_or(0, |stratum| stratum.top)
    }
}

/// A rig's fitted rig systems (bits of [`RigSystem::bit`]).
#[must_use]
pub const fn has_system(systems: u8, system: RigSystem) -> bool {
    systems & system.bit() != 0
}

/// The rig system needed to go into Descent `index` safely, if any.
#[must_use]
pub const fn descent_gate(index: u8) -> Option<RigSystem> {
    match index {
        2 => Some(RigSystem::PressureHull),
        3 => Some(RigSystem::BioFilter),
        4 => Some(RigSystem::CryoHull),
        5 => Some(RigSystem::GravityAnchor),
        6 => Some(RigSystem::PhaseDrill),
        7 => Some(RigSystem::HeartKey),
        _ => None,
    }
}

/// The first gate system `systems` lacks for Descent `index` and every Descent above it (a rig
/// that skipped the Pressure Hull is still crushed in Descent V).
#[must_use]
pub fn missing_gate(systems: u8, index: u8) -> Option<RigSystem> {
    (0..=index)
        .filter_map(descent_gate)
        .find(|system| !has_system(systems, *system))
}

/// Hull per second a rig loses in a Descent whose gate system it lacks (the gates' first,
/// stub effect: later waves turn each into its own mechanic).
pub const GATE_DAMAGE_PER_SECOND: f32 = 14.0;

/// Gas damage kept by a rig with the Bio-Filter (percent).
pub const BIO_FILTER_GAS_PERCENT: u32 = 50;
/// Extra scanner reach of the Sonar Suite, in tiles.
pub const SONAR_SUITE_SCAN_BONUS_TILES: i32 = 4;
/// Headlamp radius multiplier of Lamp II (the renderer reads it).
pub const LAMP_II_LIGHT_SCALE: f32 = 1.6;

// --- Deep tiers and curves --------------------------------------------------------------------

/// Top tier of the core lines on Descent worlds (the classic top is
/// [`crate::economy::MAX_UPGRADE_LEVEL`]).
pub const DESCENT_MAX_UPGRADE_LEVEL: u8 = 22;

/// The highest core tier the shop sells once someone has reached Descent `index`.
///
/// About two tiers per Descent on the drill, fuel tank, cargo bay, engine, hull and radiator,
/// so each chapter has its upgrades to save for.
#[must_use]
pub const fn core_tier_cap(index: u8) -> u8 {
    match index {
        0 => 6,
        1 => 10,
        2 => 13,
        3 => 15,
        4 => 17,
        5 => 19,
        6 => 21,
        _ => DESCENT_MAX_UPGRADE_LEVEL,
    }
}

/// The first Descent whose cap sells core tier `level` (the lock a shop shows).
#[must_use]
pub const fn descent_for_core_tier(level: u8) -> u8 {
    let mut index = 0;
    while index < 7 && core_tier_cap(index) < level {
        index += 1;
    }
    index
}

/// Price growth per tier past the classic top on Descent worlds (percent): gentler than the
/// classic ladders, because deep ore pays more ([`descent_value_percent`]).
pub const DEEP_TIER_GROWTH_PERCENT: u64 = 135;

/// Price of the tier after `level` on a core line on a Descent world: the classic price up to
/// the classic top tier, then [`DEEP_TIER_GROWTH_PERCENT`] more per tier.
#[must_use]
pub fn deep_tier_cost(classic_top_cost: u32, level: u8, classic_top: u8) -> u32 {
    let past = level.saturating_sub(classic_top.saturating_sub(1));
    let mut cost = u64::from(classic_top_cost);
    for _ in 0..past {
        cost = cost * DEEP_TIER_GROWTH_PERCENT / 100;
    }
    let step = if cost < 100_000 { 250 } else { 1_000 };
    u32::try_from(cost.div_ceil(step) * step).unwrap_or(u32::MAX)
}

/// What ore found in Descent `index` sells for, in percent of its classic value (the price
/// curve to the Heart: the stratum registry's ores take their values from it).
#[must_use]
pub const fn descent_value_percent(index: u8) -> u32 {
    match index {
        0 => 100,
        1 => 110,
        2 => 125,
        3 => 140,
        4 => 160,
        5 => 180,
        6 => 205,
        _ => 230,
    }
}

/// Outpost and beacon prices scale with depth: percent of the Descent I price in Descent
/// `index`.
#[must_use]
pub const fn descent_price_percent(index: u8) -> u32 {
    100 + 60 * index as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::{MAX_UPGRADE_LEVEL, UpgradeKind, upgrade_cost};

    #[test]
    fn plan_layout_has_eight_descents_and_41_strata_end_to_end() {
        let layout = PlanLayout;
        assert_eq!(layout.descents().len(), DESCENT_COUNT);
        let strata = layout.strata();
        assert_eq!(strata.len(), STRATUM_COUNT);
        assert_eq!(strata.first().map(|s| s.top), Some(0));
        assert_eq!(strata.last().map(|s| s.bottom), Some(13_000));
        for pair in strata.windows(2) {
            assert_eq!(
                pair[0].bottom, pair[1].top,
                "{} -> {}",
                pair[0].name, pair[1].name
            );
            assert!(pair[0].rows() > 150, "{} is too thin", pair[0].name);
        }
        assert_eq!(layout.descent_at(0), 0);
        assert_eq!(layout.descent_at(1_099), 0);
        assert_eq!(layout.descent_at(1_100), 1);
        assert_eq!(layout.descent_at(12_999), 7);
        assert_eq!(layout.descent_at(40_000), 7);
        assert_eq!(layout.floor_row(), 13_000);
    }

    #[test]
    fn rules_follow_the_generator() {
        for generator in [
            GeneratorVersion::Legacy,
            GeneratorVersion::V1,
            GeneratorVersion::V4,
            GeneratorVersion::V9,
            GeneratorVersion::V10,
        ] {
            assert_eq!(
                ProgressionRules::for_generator(generator),
                ProgressionRules::Classic
            );
        }
    }

    #[test]
    fn gates_cover_descents_three_to_eight_in_order() {
        assert_eq!(descent_gate(0), None);
        assert_eq!(descent_gate(1), None);
        let mut systems = 0_u8;
        for index in 2..8 {
            let gate = descent_gate(index).expect("gated");
            assert_eq!(gate.gates_descent(), Some(index));
            assert_eq!(missing_gate(systems, index), Some(gate));
            assert!(gate.sold_from_descent() < index);
            systems |= gate.bit();
            assert_eq!(missing_gate(systems, index), None);
        }
        // Skipping one keeps every deeper Descent shut.
        let skipped = systems & !RigSystem::CryoHull.bit();
        assert_eq!(missing_gate(skipped, 7), Some(RigSystem::CryoHull));
        assert_eq!(missing_gate(skipped, 3), None);
    }

    #[test]
    fn systems_go_on_sale_in_the_descent_before_their_gate() {
        let layout = PlanLayout;
        let mut last = 0;
        for system in RigSystem::ALL {
            let row = system.sold_from_row(&layout);
            assert!(row >= last, "{system:?}");
            last = row;
            let descent = layout.descent_at(row);
            assert_eq!(descent, system.sold_from_descent());
            if let Some(gate) = system.gates_descent() {
                let top = layout.descent(gate).expect("descent").top;
                assert!(row < top, "{system:?} on sale at {row}, gate at {top}");
            }
        }
        let mut prices: Vec<u32> = [
            RigSystem::PressureHull,
            RigSystem::BioFilter,
            RigSystem::CryoHull,
            RigSystem::GravityAnchor,
            RigSystem::PhaseDrill,
            RigSystem::HeartKey,
        ]
        .map(RigSystem::price)
        .to_vec();
        let sorted = prices.clone();
        prices.sort_unstable();
        assert_eq!(
            prices, sorted,
            "gate systems cost more the deeper they open"
        );
    }

    #[test]
    fn curves_to_the_heart_are_monotonic() {
        let mut last_cap = 0;
        for index in 0..8 {
            let cap = core_tier_cap(index);
            assert!(cap > last_cap);
            last_cap = cap;
            assert_eq!(descent_for_core_tier(cap), index);
        }
        assert_eq!(last_cap, DESCENT_MAX_UPGRADE_LEVEL);
        assert!(core_tier_cap(3) >= MAX_UPGRADE_LEVEL);
        for pair in (0..8).collect::<Vec<u8>>().windows(2) {
            assert!(descent_value_percent(pair[1]) > descent_value_percent(pair[0]));
            assert!(descent_price_percent(pair[1]) > descent_price_percent(pair[0]));
        }
        for kind in UpgradeKind::ALL
            .iter()
            .copied()
            .filter(|kind| kind.is_core())
        {
            let top = upgrade_cost(kind, MAX_UPGRADE_LEVEL - 1);
            let mut last = top;
            for level in MAX_UPGRADE_LEVEL..DESCENT_MAX_UPGRADE_LEVEL {
                let cost = deep_tier_cost(top, level, MAX_UPGRADE_LEVEL);
                assert!(cost > last, "{kind:?} tier {level}");
                last = cost;
            }
        }
    }
}
