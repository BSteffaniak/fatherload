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
    /// Read the first log of the Fathers' expedition (Wave 8 story).
    TrailFound,
    /// Read every expedition log of the Crust.
    ShapedWorld,
    /// Read every expedition log of the Old Deep.
    BuildersCellar,
    /// Read every expedition log of the Living Dark.
    GrowsInTheDark,
    /// Heard one of Dov Halloran's ghost recordings.
    GhostVoices,
    /// Stepped into an anomaly room.
    FirstAnomaly,
    /// Stepped into every kind of anomaly room.
    EveryAnomaly,
    /// Reach Descent II, the Old Deep. (Wave 10 story).
    ReachedOldDeep,
    /// Reach Descent III, the Living Dark. (Wave 10 story).
    ReachedLivingDark,
    /// Reach Descent IV, the Drowned and Frozen. (Wave 10 story).
    ReachedDrowned,
    /// Reach Descent V, the Hollow World. (Wave 10 story).
    ReachedHollow,
    /// Reach Descent VI, the Strange Layers. (Wave 10 story).
    ReachedStrange,
    /// Reach Descent VII, the Ancients. (Wave 10 story).
    ReachedAncients,
    /// Reach Descent VIII, the Heart. (Wave 10 story).
    ReachedHeart,
    /// Climb out of the Sinkhole with its core: the Crust's gate. (Wave 10 story).
    BeatSinkhole,
    /// Beat the Star Core: the Old Deep's gate. (Wave 10 story).
    BeatStarCore,
    /// Beat the Mycelial Mind: the Living Dark's gate. (Wave 10 story).
    BeatMind,
    /// Beat the Leviathan: the Drowned and Frozen's gate. (Wave 10 story).
    BeatLeviathan,
    /// Beat the Sun Serpent: the Hollow World's gate. (Wave 10 story).
    BeatSerpent,
    /// Beat the Glitch: the Strange Layers' gate. (Wave 10 story).
    BeatGlitch,
    /// Beat the Gatekeeper Colossus: the Ancients' gate. (Wave 10 story).
    BeatColossus,
    /// Read every expedition log in the Drowned and Frozen. (Wave 10 story).
    PeopleBelow,
    /// Read every expedition log in the Hollow World. (Wave 10 story).
    HollowWorldRead,
    /// Read every expedition log in the Strange Layers. (Wave 10 story).
    WearsThin,
    /// Read every expedition log in the Ancients. (Wave 10 story).
    BuildersCity,
    /// Read every expedition log in the Heart. (Wave 10 story).
    StarDriveChapter,
    /// Read all eighty logs of the Fathers' expedition. (Wave 10 story).
    WholeTrail,
    /// Wake the Heart and light the cable to the Moon. (Wave 10 story).
    LookUp,
    /// Mine a space material from a belt rock. (Wave 13 endgame).
    SpaceMiner,
    /// Gather comet dust from a comet's tail. (Wave 13 endgame).
    CometChaser,
    /// Land on a world beyond the Moon. (Wave 13 endgame).
    FirstWorld,
    /// Land on ten different worlds. (Wave 13 endgame).
    TenWorlds,
    /// Land on every kind of world: all twelve. (Wave 13 endgame).
    EveryTheme,
    /// Meet one of the friendly species. (Wave 13 endgame).
    FirstContactMade,
    /// Speak a species' tongue fluently. (Wave 13 endgame).
    FluentTongue,
    /// Speak every species' tongue fluently. (Wave 13 endgame).
    EveryTongue,
    /// Beat the Gilded Maw, king of the Rustwake Corsairs. (Wave 13 endgame).
    PirateKingBeaten,
    /// Beat the Swarm Queen of the Bloom. (Wave 13 endgame).
    SwarmQueenBeaten,
    /// Beat the Arbiter, the Concord's capital ship. (Wave 13 endgame).
    ArbiterBeaten,
    /// Beat every boss on the hunt list. (Wave 13 endgame).
    HuntComplete,
    /// Fit a ship line at tier 5. (Wave 13 endgame).
    TopShipLine,
    /// Fit the Starwright Hull and Engine: the endgame ship. (Wave 13 endgame).
    StarwrightShip,
    /// Fit the Heartline Drive. (Wave 13 endgame).
    HeartlineDrive,
    /// Finish the Halcyon Gate with your crew. (Wave 13 endgame).
    GateBuilt,
    /// Own a finished home on an alien world. (Wave 13 endgame).
    AlienHome,
    /// Raise every wing of an estate on an alien world. (Wave 13 endgame).
    FullEstate,
    /// See and do everything: 100% completion. (Wave 13 endgame).
    Completionist,
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
    /// The galaxy (Wave 13 endgame).
    Space,
}

impl MilestoneGroup {
    pub const ALL: [Self; 5] = [
        Self::Upgrades,
        Self::Depths,
        Self::Discoveries,
        Self::Sky,
        Self::Space,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Upgrades => "Upgrades",
            Self::Depths => "Depths",
            Self::Discoveries => "Discoveries",
            Self::Sky => "Sky",
            Self::Space => "Space",
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

/// Whether `player` has earned the Island Hopper milestone ([`Milestone::IslandCapableTank`])
/// in a sky with `bands`.
///
/// The rig must really reach the low isles ([`island_capable`]) and its kit must be what the
/// sky was planned to reach them with ([`SkyBands::calibration`]). V8 skies were calibrated
/// on the first fuel curve; on today's bigger tanks a stock rig already climbs into them, so
/// [`island_capable`] alone handed the milestone out on a V8 world's first tick. Against the
/// world's own table it still takes the first tank upgrades there, as it does in V9 skies
/// (whose calibration is today's table).
#[must_use]
pub fn island_hopper(player: &Player, bands: &SkyBands) -> bool {
    island_capable(player, bands)
        && crate::climb::reach_in(
            bands.calibration,
            player.fuel_tank_level,
            player.engine_level,
            player.hazard_gear.altitude_intake,
        ) >= bands.island_reach_rows()
}

/// Rows over the cloud archipelago's floor a rig must climb to reach the sky towns of its
/// lowest town row (a town isle stands at least 70 rows into its cell and at most about 140).
pub const SKY_TOWN_REACH_ROWS: i32 = 150;

/// Whether `player`'s rig climbs to the lowest sky towns of a sky with `bands` on a full tank
/// (the story place [`crate::quest::StoryPlace::SkyTown`] binds those).
#[must_use]
pub fn sky_town_capable(player: &Player, bands: &SkyBands) -> bool {
    climb_reach(player, bands) >= bands.archipelago + SKY_TOWN_REACH_ROWS
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

/// The share of its tank by which a rig's fuel must fall short of crossing the dead-air gap
/// from where it is for free lift to push it at full strength ([`free_lift_share`]).
pub const FREE_LIFT_FUEL_MARGIN: f32 = 0.04;

/// The fuel at and above which free lift lets go of `player`'s rig at `altitude_rows`.
///
/// That is the climb table's estimate of what it would take to climb from here to
/// the continents' floor (`None` where free lift is never rationed: below the low isles,
/// from the continents up, and for rigs that cross the gap anyway).
///
/// The estimate spreads the rig's full tank evenly over its [`climb_reach`] from the climb
/// harness's start. Fuel per row only grows with altitude (thinner air), so the real climb
/// from here costs at least this much: a rig held under the limit never holds enough to cross.
#[must_use]
pub fn free_lift_fuel_limit(player: &Player, altitude_rows: i32, bands: &SkyBands) -> Option<f32> {
    if !(bands.low_isles..bands.dead_air_reach_rows()).contains(&altitude_rows)
        || crosses_dead_air(player, bands)
    {
        return None;
    }
    // The climb harness starts at rest 100 rows up (`world::climb_tests::START_ALTITUDE_ROWS`).
    let start = 100;
    let reach = climb_reach(player, bands);
    #[allow(
        clippy::cast_precision_loss,
        reason = "altitudes are a few thousand rows"
    )]
    let limit = player.fuel_capacity * (bands.dead_air_reach_rows() - altitude_rows) as f32
        / (reach - start).max(1) as f32;
    Some(limit)
}

/// The share of an updraft's or vent's push that lifts `player`'s rig at `altitude_rows` in a
/// sky with `bands`.
///
/// The Act II gate ([`crosses_dead_air`]) is a full-tank climb from the ground in still air.
/// Riding updrafts and vents up the archipelago costs no fuel, so a near-crossing rig (tier
/// 10-12 tanks with top intakes) could ride to the gap's floor with a near-full tank and cross
/// on it. So free lift lets go of a rig that cannot cross the gap as its tank nears what it
/// would take to climb across from where it is ([`free_lift_fuel_limit`]): all of the push
/// while it is [`FREE_LIFT_FUEL_MARGIN`] of its tank short or more, none at the limit (a
/// laden tank is too heavy for the air). A free ride can therefore never leave such a rig
/// holding enough to cross. Rigs far short of the gap (every rig of the island bands' own
/// tiers) ride as before, and so do rigs that cross anyway and everything outside the island
/// bands.
#[must_use]
pub fn free_lift_share(player: &Player, altitude_rows: i32, bands: &SkyBands) -> f32 {
    let Some(limit) = free_lift_fuel_limit(player, altitude_rows, bands) else {
        return 1.0;
    };
    let margin = player.fuel_capacity.max(1.0) * FREE_LIFT_FUEL_MARGIN;
    ((limit - player.fuel) / margin).clamp(0.0, 1.0)
}

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
    /// Expedition logs the crew has read (Wave 8 story; shared by the crew).
    pub expedition_logs: u8,
    /// Expedition echoes the crew has heard.
    pub expedition_echoes: u8,
    /// Kinds of anomaly room the crew has entered (bits of
    /// [`crate::expedition::anomaly::AnomalyKind::bit`]).
    pub anomaly_kinds: u8,
    /// Descents the crew has reached (Wave 10; bits of
    /// [`crate::expedition::progress::descent_bit`]).
    pub descents: u8,
    /// Gate bosses the crew has beaten (bits of [`crate::expedition::progress::boss_bit`]).
    pub bosses: u16,
    /// The Heart woke and the cable lit (the ending).
    pub cable_lit: bool,
    /// The player's travels in space (Wave 13 endgame; default off a world without space).
    pub space: SpaceFacts,
}

/// What the world knows of a player's travels in space (Wave 13 endgame achievements,
/// gathered by `WorldState::space_facts`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one fact per achievement, read by its own arm"
)]
pub struct SpaceFacts {
    /// Space material units mined, and of those comet dust.
    pub mined: u32,
    pub comet_dust: u32,
    /// Worlds landed on, and the kinds of world among them.
    pub worlds: u16,
    pub themes: u8,
    /// Species met, species whose tongue the player speaks fluently, and species in all.
    pub species_met: u8,
    pub tongues: u8,
    pub species_total: u8,
    /// Faction bosses beaten by the crew (bit per [`crate::hostiles::Faction::code`]).
    pub faction_bosses: u8,
    /// Every boss on the hunt list beaten.
    pub hunt_done: bool,
    /// A ship line at tier 5; the Starwright ship; the Heartline Drive.
    pub line_maxed: bool,
    pub starwright: bool,
    pub heartline: bool,
    /// The crew's Halcyon Gate finished.
    pub gate_open: bool,
    /// A finished home on an alien world, and an estate with every wing.
    pub alien_home: bool,
    pub estate_full: bool,
    /// Everything else done (the completion screen's 100 %).
    pub complete: bool,
}

/// Whether a crew with `facts` has read every expedition log of `descent`'s chapter.
fn chapter_done(facts: &MilestoneFacts, descent: crate::descent::Descent) -> bool {
    let state = crate::expedition::ExpeditionState {
        found: facts.expedition_logs,
        ..crate::expedition::ExpeditionState::default()
    };
    state.chapter_read(descent)
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
    pub const ALL: [Self; 66] = [
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
        Self::TrailFound,
        Self::ShapedWorld,
        Self::BuildersCellar,
        Self::GrowsInTheDark,
        Self::GhostVoices,
        Self::FirstAnomaly,
        Self::EveryAnomaly,
        Self::ReachedOldDeep,
        Self::ReachedLivingDark,
        Self::ReachedDrowned,
        Self::ReachedHollow,
        Self::ReachedStrange,
        Self::ReachedAncients,
        Self::ReachedHeart,
        Self::BeatSinkhole,
        Self::BeatStarCore,
        Self::BeatMind,
        Self::BeatLeviathan,
        Self::BeatSerpent,
        Self::BeatGlitch,
        Self::BeatColossus,
        Self::PeopleBelow,
        Self::HollowWorldRead,
        Self::WearsThin,
        Self::BuildersCity,
        Self::StarDriveChapter,
        Self::WholeTrail,
        Self::LookUp,
        Self::SpaceMiner,
        Self::CometChaser,
        Self::FirstWorld,
        Self::TenWorlds,
        Self::EveryTheme,
        Self::FirstContactMade,
        Self::FluentTongue,
        Self::EveryTongue,
        Self::PirateKingBeaten,
        Self::SwarmQueenBeaten,
        Self::ArbiterBeaten,
        Self::HuntComplete,
        Self::TopShipLine,
        Self::StarwrightShip,
        Self::HeartlineDrive,
        Self::GateBuilt,
        Self::AlienHome,
        Self::FullEstate,
        Self::Completionist,
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
            Self::TrailFound => "The Fathers' Trail",
            Self::ShapedWorld => "The Shaped World",
            Self::BuildersCellar => "The Builders' Cellar",
            Self::GrowsInTheDark => "What Grows in the Dark",
            Self::GhostVoices => "Voices on the Cable",
            Self::FirstAnomaly => "Something Is Wrong Here",
            Self::EveryAnomaly => "Cracks in Reality",
            Self::ReachedOldDeep => "Past the Lid",
            Self::ReachedLivingDark => "Where It Grows",
            Self::ReachedDrowned => "Under the Sea",
            Self::ReachedHollow => "A Sun Inside",
            Self::ReachedStrange => "Thin Places",
            Self::ReachedAncients => "Swept Streets",
            Self::ReachedHeart => "Heartward",
            Self::BeatSinkhole => "Out of the Sinkhole",
            Self::BeatStarCore => "Core Breaker",
            Self::BeatMind => "Clear-Headed",
            Self::BeatLeviathan => "Leviathan's Bane",
            Self::BeatSerpent => "Sun Chaser",
            Self::BeatGlitch => "Bug Fixed",
            Self::BeatColossus => "Colossus Toppled",
            Self::PeopleBelow => "The People Below",
            Self::HollowWorldRead => "The Hollow World",
            Self::WearsThin => "Where the World Wears Thin",
            Self::BuildersCity => "The Builders' City",
            Self::StarDriveChapter => "The Star Drive",
            Self::WholeTrail => "The Whole Trail",
            Self::LookUp => "Look Up",
            Self::SpaceMiner => "Star Miner",
            Self::CometChaser => "Comet Chaser",
            Self::FirstWorld => "Planetfall",
            Self::TenWorlds => "Wanderer",
            Self::EveryTheme => "Atlas of Worlds",
            Self::FirstContactMade => "Hello, Neighbour",
            Self::FluentTongue => "Fluent",
            Self::EveryTongue => "Many Tongues",
            Self::PirateKingBeaten => "Crown Breaker",
            Self::SwarmQueenBeaten => "Queenfall",
            Self::ArbiterBeaten => "Overruled",
            Self::HuntComplete => "The Hunt Is Over",
            Self::TopShipLine => "Shipwright",
            Self::StarwrightShip => "Starwright",
            Self::HeartlineDrive => "Heartline",
            Self::GateBuilt => "Gatebuilder",
            Self::AlienHome => "Home Away From Home",
            Self::FullEstate => "Lord of the Manor",
            Self::Completionist => "Fatherload",
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
            Self::TrailFound => "Read the first log of the Fathers' expedition.",
            Self::ShapedWorld => "Read every expedition log in the Crust.",
            Self::BuildersCellar => "Read every expedition log in the Old Deep.",
            Self::GrowsInTheDark => "Read every expedition log in the Living Dark.",
            Self::GhostVoices => "Hear one of Dov Halloran's ghost recordings.",
            Self::FirstAnomaly => "Step into an anomaly room.",
            Self::EveryAnomaly => "Step into all four kinds of anomaly room.",
            Self::ReachedOldDeep => "Reach Descent II, the Old Deep.",
            Self::ReachedLivingDark => "Reach Descent III, the Living Dark.",
            Self::ReachedDrowned => "Reach Descent IV, the Drowned and Frozen.",
            Self::ReachedHollow => "Reach Descent V, the Hollow World.",
            Self::ReachedStrange => "Reach Descent VI, the Strange Layers.",
            Self::ReachedAncients => "Reach Descent VII, the Ancients.",
            Self::ReachedHeart => "Reach Descent VIII, the Heart.",
            Self::BeatSinkhole => "Climb out of the Sinkhole with its core: the Crust's gate.",
            Self::BeatStarCore => "Beat the Star Core: the Old Deep's gate.",
            Self::BeatMind => "Beat the Mycelial Mind: the Living Dark's gate.",
            Self::BeatLeviathan => "Beat the Leviathan: the Drowned and Frozen's gate.",
            Self::BeatSerpent => "Beat the Sun Serpent: the Hollow World's gate.",
            Self::BeatGlitch => "Beat the Glitch: the Strange Layers' gate.",
            Self::BeatColossus => "Beat the Gatekeeper Colossus: the Ancients' gate.",
            Self::PeopleBelow => "Read every expedition log in the Drowned and Frozen.",
            Self::HollowWorldRead => "Read every expedition log in the Hollow World.",
            Self::WearsThin => "Read every expedition log in the Strange Layers.",
            Self::BuildersCity => "Read every expedition log in the Ancients.",
            Self::StarDriveChapter => "Read every expedition log in the Heart.",
            Self::WholeTrail => "Read all eighty logs of the Fathers' expedition.",
            Self::LookUp => "Wake the Heart and light the cable to the Moon.",
            Self::SpaceMiner => "Mine a space material from a belt rock.",
            Self::CometChaser => "Gather comet dust from a comet's tail.",
            Self::FirstWorld => "Land on a world beyond the Moon.",
            Self::TenWorlds => "Land on ten different worlds.",
            Self::EveryTheme => "Land on every kind of world: all twelve.",
            Self::FirstContactMade => "Meet one of the friendly species.",
            Self::FluentTongue => "Speak a species' tongue fluently.",
            Self::EveryTongue => "Speak every species' tongue fluently.",
            Self::PirateKingBeaten => "Beat the Gilded Maw, king of the Rustwake Corsairs.",
            Self::SwarmQueenBeaten => "Beat the Swarm Queen of the Bloom.",
            Self::ArbiterBeaten => "Beat the Arbiter, the Concord's capital ship.",
            Self::HuntComplete => "Beat every boss on the hunt list.",
            Self::TopShipLine => "Fit a ship line at tier 5.",
            Self::StarwrightShip => "Fit the Starwright Hull and Engine: the endgame ship.",
            Self::HeartlineDrive => "Fit the Heartline Drive.",
            Self::GateBuilt => "Finish the Halcyon Gate with your crew.",
            Self::AlienHome => "Own a finished home on an alien world.",
            Self::FullEstate => "Raise every wing of an estate on an alien world.",
            Self::Completionist => "See and do everything: 100% completion.",
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
            | Self::ReachedMantle
            | Self::ReachedOldDeep
            | Self::ReachedLivingDark
            | Self::ReachedDrowned
            | Self::ReachedHollow
            | Self::ReachedStrange
            | Self::ReachedAncients
            | Self::ReachedHeart
            | Self::BeatSinkhole
            | Self::BeatStarCore
            | Self::BeatMind
            | Self::BeatLeviathan
            | Self::BeatSerpent
            | Self::BeatGlitch
            | Self::BeatColossus
            | Self::LookUp => MilestoneGroup::Depths,
            Self::FirstArtifact
            | Self::FirstCave
            | Self::FirstVillage
            | Self::FirstKill
            | Self::FirstClaim
            | Self::TrailFound
            | Self::ShapedWorld
            | Self::BuildersCellar
            | Self::GrowsInTheDark
            | Self::GhostVoices
            | Self::FirstAnomaly
            | Self::EveryAnomaly
            | Self::PeopleBelow
            | Self::HollowWorldRead
            | Self::WearsThin
            | Self::BuildersCity
            | Self::StarDriveChapter
            | Self::WholeTrail => MilestoneGroup::Discoveries,
            Self::HighFlyer | Self::IslandCapableTank | Self::ActTwoUnlocked => MilestoneGroup::Sky,
            Self::SpaceMiner
            | Self::CometChaser
            | Self::FirstWorld
            | Self::TenWorlds
            | Self::EveryTheme
            | Self::FirstContactMade
            | Self::FluentTongue
            | Self::EveryTongue
            | Self::PirateKingBeaten
            | Self::SwarmQueenBeaten
            | Self::ArbiterBeaten
            | Self::HuntComplete
            | Self::TopShipLine
            | Self::StarwrightShip
            | Self::HeartlineDrive
            | Self::GateBuilt
            | Self::AlienHome
            | Self::FullEstate
            | Self::Completionist => MilestoneGroup::Space,
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
            | Self::HighFlyer
            | Self::TrailFound
            | Self::FirstAnomaly
            | Self::SpaceMiner
            | Self::FirstWorld
            | Self::FirstContactMade => Medal::Bronze,
            Self::DrillTier5
            | Self::EngineTier5
            | Self::FirstDeepLayer
            | Self::IslandCapableTank
            | Self::ShapedWorld
            | Self::GhostVoices
            | Self::ReachedOldDeep
            | Self::ReachedLivingDark
            | Self::ReachedDrowned
            | Self::BeatSinkhole
            | Self::CometChaser
            | Self::TenWorlds
            | Self::FluentTongue
            | Self::TopShipLine
            | Self::AlienHome => Medal::Silver,
            Self::DrillTier10
            | Self::EngineTier10
            | Self::MaxedLine
            | Self::ReachedMachineLayer
            | Self::ReachedMantle
            | Self::BuildersCellar
            | Self::GrowsInTheDark
            | Self::EveryAnomaly
            | Self::ReachedHollow
            | Self::ReachedStrange
            | Self::ReachedAncients
            | Self::ReachedHeart
            | Self::BeatStarCore
            | Self::BeatMind
            | Self::BeatLeviathan
            | Self::BeatSerpent
            | Self::BeatGlitch
            | Self::PeopleBelow
            | Self::HollowWorldRead
            | Self::WearsThin
            | Self::BuildersCity
            | Self::StarDriveChapter
            | Self::EveryTheme
            | Self::EveryTongue
            | Self::PirateKingBeaten
            | Self::SwarmQueenBeaten
            | Self::ArbiterBeaten
            | Self::StarwrightShip
            | Self::GateBuilt
            | Self::FullEstate => Medal::Gold,
            Self::FullyLoaded
            | Self::ActTwoUnlocked
            | Self::BeatColossus
            | Self::WholeTrail
            | Self::LookUp
            | Self::HuntComplete
            | Self::HeartlineDrive
            | Self::Completionist => Medal::Platinum,
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
            Self::TrailFound => &[CosmeticItem::Paint(PaintColor::Khaki)],
            Self::ShapedWorld => &[CosmeticItem::Paint(PaintColor::Ochre)],
            Self::BuildersCellar => &[CosmeticItem::Paint(PaintColor::Bronze)],
            Self::GrowsInTheDark => &[CosmeticItem::Paint(PaintColor::Lumen)],
            Self::GhostVoices => &[CosmeticItem::Paint(PaintColor::Phantom)],
            Self::FirstAnomaly => &[CosmeticItem::Paint(PaintColor::Anomaly)],
            Self::EveryAnomaly => &[CosmeticItem::Paint(PaintColor::Glitch)],
            Self::ReachedOldDeep => &[CosmeticItem::Paint(PaintColor::Quartz)],
            Self::ReachedLivingDark => &[CosmeticItem::Paint(PaintColor::Spore)],
            Self::ReachedDrowned => &[CosmeticItem::Paint(PaintColor::Brine)],
            Self::ReachedHollow => &[CosmeticItem::Paint(PaintColor::InnerSun)],
            Self::ReachedStrange => &[CosmeticItem::Paint(PaintColor::Prism)],
            Self::ReachedAncients => &[CosmeticItem::Paint(PaintColor::Alloy)],
            Self::ReachedHeart => &[CosmeticItem::Paint(PaintColor::Heartlight)],
            Self::BeatSinkhole => &[CosmeticItem::Paint(PaintColor::Rubble)],
            Self::BeatStarCore => &[CosmeticItem::Paint(PaintColor::Corona)],
            Self::BeatMind => &[CosmeticItem::Paint(PaintColor::Mycelium)],
            Self::BeatLeviathan => &[CosmeticItem::Paint(PaintColor::DeepScale)],
            Self::BeatSerpent => &[CosmeticItem::Paint(PaintColor::SerpentGold)],
            Self::BeatGlitch => &[CosmeticItem::Paint(PaintColor::DeadPixel)],
            Self::BeatColossus => &[CosmeticItem::Paint(PaintColor::GateGold)],
            Self::PeopleBelow => &[CosmeticItem::Paint(PaintColor::Nacre)],
            Self::HollowWorldRead => &[CosmeticItem::Paint(PaintColor::Jungle)],
            Self::WearsThin => &[CosmeticItem::Paint(PaintColor::Mirrorglass)],
            Self::BuildersCity => &[CosmeticItem::Paint(PaintColor::Lamplight)],
            Self::StarDriveChapter => &[CosmeticItem::Paint(PaintColor::DriveWhite)],
            Self::WholeTrail => &[CosmeticItem::Paint(PaintColor::FathersLoad)],
            Self::LookUp => &[CosmeticItem::Paint(PaintColor::Starward)],
            Self::SpaceMiner => &[CosmeticItem::Paint(PaintColor::Meteorite)],
            Self::CometChaser => &[CosmeticItem::Flame(FlameColor::Comet)],
            Self::FirstWorld => &[CosmeticItem::Decal(Decal::Planet)],
            Self::TenWorlds => &[CosmeticItem::Paint(PaintColor::Nebula)],
            Self::EveryTheme => &[CosmeticItem::Paint(PaintColor::Atlas)],
            Self::FirstContactMade => &[CosmeticItem::Decal(Decal::Glyph)],
            Self::FluentTongue => &[CosmeticItem::Paint(PaintColor::Xeno)],
            Self::EveryTongue => &[CosmeticItem::Paint(PaintColor::Babel)],
            Self::PirateKingBeaten => &[CosmeticItem::Paint(PaintColor::GildedMaw)],
            Self::SwarmQueenBeaten => &[CosmeticItem::Paint(PaintColor::Chitin)],
            Self::ArbiterBeaten => &[CosmeticItem::Paint(PaintColor::Concord)],
            Self::HuntComplete => &[CosmeticItem::Paint(PaintColor::Huntsman)],
            Self::TopShipLine => &[CosmeticItem::Paint(PaintColor::Shipwright)],
            Self::StarwrightShip => &[CosmeticItem::Paint(PaintColor::Starwright)],
            Self::HeartlineDrive => &[CosmeticItem::Flame(FlameColor::Heartline)],
            Self::GateBuilt => &[CosmeticItem::Paint(PaintColor::Gatestone)],
            Self::AlienHome => &[CosmeticItem::Paint(PaintColor::Hearthstone)],
            Self::FullEstate => &[CosmeticItem::Paint(PaintColor::Manor)],
            Self::Completionist => &[CosmeticItem::Pattern(HullPattern::Constellation)],
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
    #[allow(clippy::too_many_lines, reason = "one arm per milestone")]
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
                .is_some_and(|bands| island_hopper(player, &bands)),
            Self::ActTwoUnlocked => facts
                .sky_bands
                .is_some_and(|bands| crosses_dead_air(player, &bands)),
            Self::TrailFound => facts.expedition_logs >= 1,
            Self::ShapedWorld => chapter_done(facts, crate::descent::Descent::Crust),
            Self::BuildersCellar => chapter_done(facts, crate::descent::Descent::OldDeep),
            Self::GrowsInTheDark => chapter_done(facts, crate::descent::Descent::LivingDark),
            Self::GhostVoices => crate::expedition::ECHOES
                .iter()
                .take(usize::from(facts.expedition_echoes))
                .any(|echo| echo.kind == crate::expedition::EchoKind::Ghost),
            Self::FirstAnomaly => facts.anomaly_kinds != 0,
            // Wave 8's four (the deeper kinds of Wave 9 fill the codex instead).
            Self::EveryAnomaly => crate::expedition::anomaly::AnomalyKind::SHALLOW
                .iter()
                .all(|kind| facts.anomaly_kinds & kind.bit() != 0),
            // Wave 10 story: the Descents reached, the gate bosses beaten, the deeper chapters,
            // the whole trail and the ending (all crew-wide, from the shared journal).
            Self::ReachedOldDeep => {
                facts.descents
                    & crate::expedition::progress::descent_bit(crate::descent::Descent::OldDeep)
                    != 0
            }
            Self::ReachedLivingDark => {
                facts.descents
                    & crate::expedition::progress::descent_bit(crate::descent::Descent::LivingDark)
                    != 0
            }
            Self::ReachedDrowned => {
                facts.descents
                    & crate::expedition::progress::descent_bit(
                        crate::descent::Descent::DrownedAndFrozen,
                    )
                    != 0
            }
            Self::ReachedHollow => {
                facts.descents
                    & crate::expedition::progress::descent_bit(crate::descent::Descent::HollowWorld)
                    != 0
            }
            Self::ReachedStrange => {
                facts.descents
                    & crate::expedition::progress::descent_bit(
                        crate::descent::Descent::StrangeLayers,
                    )
                    != 0
            }
            Self::ReachedAncients => {
                facts.descents
                    & crate::expedition::progress::descent_bit(crate::descent::Descent::Ancients)
                    != 0
            }
            Self::ReachedHeart => {
                facts.descents
                    & crate::expedition::progress::descent_bit(crate::descent::Descent::Heart)
                    != 0
            }
            Self::BeatSinkhole => {
                facts.bosses
                    & crate::expedition::progress::boss_bit(
                        crate::descent::DescentBoss::TheSinkhole,
                    )
                    != 0
            }
            Self::BeatStarCore => {
                facts.bosses
                    & crate::expedition::progress::boss_bit(crate::descent::DescentBoss::StarCore)
                    != 0
            }
            Self::BeatMind => {
                facts.bosses
                    & crate::expedition::progress::boss_bit(
                        crate::descent::DescentBoss::MycelialMind,
                    )
                    != 0
            }
            Self::BeatLeviathan => {
                facts.bosses
                    & crate::expedition::progress::boss_bit(crate::descent::DescentBoss::Leviathan)
                    != 0
            }
            Self::BeatSerpent => {
                facts.bosses
                    & crate::expedition::progress::boss_bit(crate::descent::DescentBoss::SunSerpent)
                    != 0
            }
            Self::BeatGlitch => {
                facts.bosses
                    & crate::expedition::progress::boss_bit(crate::descent::DescentBoss::TheGlitch)
                    != 0
            }
            Self::BeatColossus => {
                facts.bosses
                    & crate::expedition::progress::boss_bit(
                        crate::descent::DescentBoss::GatekeeperColossus,
                    )
                    != 0
            }
            Self::PeopleBelow => chapter_done(facts, crate::descent::Descent::DrownedAndFrozen),
            Self::HollowWorldRead => chapter_done(facts, crate::descent::Descent::HollowWorld),
            Self::WearsThin => chapter_done(facts, crate::descent::Descent::StrangeLayers),
            Self::BuildersCity => chapter_done(facts, crate::descent::Descent::Ancients),
            Self::StarDriveChapter => chapter_done(facts, crate::descent::Descent::Heart),
            Self::WholeTrail => usize::from(facts.expedition_logs) >= crate::expedition::LOGS.len(),
            Self::LookUp => facts.cable_lit,
            Self::SpaceMiner => facts.space.mined > 0,
            Self::CometChaser => facts.space.comet_dust > 0,
            Self::FirstWorld => facts.space.worlds >= 1,
            Self::TenWorlds => facts.space.worlds >= 10,
            Self::EveryTheme => {
                usize::from(facts.space.themes) >= crate::space::Theme::PLANET.len()
            }
            Self::FirstContactMade => facts.space.species_met >= 1,
            Self::FluentTongue => facts.space.tongues >= 1,
            Self::EveryTongue => {
                facts.space.species_total > 0 && facts.space.tongues >= facts.space.species_total
            }
            Self::PirateKingBeaten => facts.space.faction_bosses & 1 != 0,
            Self::SwarmQueenBeaten => facts.space.faction_bosses & 2 != 0,
            Self::ArbiterBeaten => facts.space.faction_bosses & 4 != 0,
            Self::HuntComplete => facts.space.hunt_done,
            Self::TopShipLine => facts.space.line_maxed,
            Self::StarwrightShip => facts.space.starwright,
            Self::HeartlineDrive => facts.space.heartline,
            Self::GateBuilt => facts.space.gate_open,
            Self::AlienHome => facts.space.alien_home,
            Self::FullEstate => facts.space.estate_full,
            Self::Completionist => facts.space.complete,
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
    fn island_hopper_is_earned_against_the_worlds_own_table() {
        // A stock rig of today's fuel curve really climbs into V8's low isles, but the
        // milestone waits for the kit V8's sky was planned to reach them with.
        let v8 = crate::worldgen::sky::V8_SKY_BANDS;
        let facts = MilestoneFacts {
            sky_bands: Some(v8),
            ..MilestoneFacts::default()
        };
        let mut player = rig();
        assert!(island_capable(&player, &v8));
        assert!(!island_hopper(&player, &v8));
        assert!(
            !Milestone::newly_met(&player, &facts).contains(&Milestone::IslandCapableTank),
            "a stock rig earns nothing on its first tick"
        );
        // Upgrading earns it, at the tier V8's own table opens the isles at.
        let earned_at = (1..=crate::economy::MAX_UPGRADE_LEVEL)
            .find(|tank| {
                player.fuel_tank_level = *tank;
                island_hopper(&player, &v8)
            })
            .expect("some tank earns it");
        assert!(earned_at > 1, "earned at tank {earned_at}");
        assert!(Milestone::IslandCapableTank.met(&player, &facts));
        // In V9's sky (calibrated on today's table) it is exactly the real climb.
        let v9 = crate::worldgen::sky::V9_SKY_BANDS;
        for tank in 1..=crate::economy::MAX_UPGRADE_LEVEL {
            player.fuel_tank_level = tank;
            assert_eq!(island_hopper(&player, &v9), island_capable(&player, &v9));
        }
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
