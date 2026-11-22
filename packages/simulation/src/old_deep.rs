//! Descent II, the Old Deep (Wave 8, expansion plan §14): the seven deep strata of a Deep
//! Descent world (generator V11) with their signature rules, materials, set pieces and the
//! Star Core gate.
//!
//! Classic worlds never see any of it: every tile here is generated only by V11
//! ([`sites`]), and every rule in `world/old_deep.rs` reads only those tiles or the V11 layout,
//! so V1-V10 worlds keep their digests, hashes and behaviour.
//!
//! * **Crystal Faults:** resonant crystal shatters in chains ([`resonance_reach`]) and prisms
//!   split the headlamp into beams ([`refraction`]) that light latent gems hidden in the rock.
//! * **Fossil Oceans:** gas tides ([`gas_tide`]) breathe out of gas pockets, amber holds a rig
//!   fast, and fossil skeletons must be dug out bone by bone ([`bone_exposed`]).
//! * **Pressure Cathedrals:** grand chambers with timed pressure jets ([`jet_active`]) and the
//!   Cathedral Lift set piece (three valves restore an ancient lift).
//! * **Radioactive Hollow:** a dose meter ([`Dose`]) that fills near glow rock, drains away from
//!   it, and costs fuel, sight and finally hull; mutation relics ([`Mutation`]).
//! * **Ancient Machine Layer:** machine halls that wake as a rig comes close: conveyors carry
//!   rigs, pistons slam ([`piston_extension`]); builder tablets tell the first of the story.
//! * **Void Geode Fields:** geodes are low-gravity pockets whose weak pull points out to their
//!   shell ([`fields`]), so rigs float and turn to land on the walls.
//! * **Mantle Storm Zone:** heat storms on a timer ([`storm_phase`]) scorch rigs in open caves
//!   and make storm vents pour molten metal.
//! * **The gate:** the Star Core, the "first core", a boss set piece (pylons, flares, sparks,
//!   no countdown) whose defeat grants the Pressure Hull and reveals that the core is not the
//!   bottom.

pub mod fields;
pub mod refraction;
pub mod sites;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    PlayerId,
    ids::SimulationTick,
    item::{BlockKind, ItemKind},
    terrain::{ArtifactKind, StrategicResourceKind, TilePosition},
};

// ---------------------------------------------------------------------------------------------
// Tiles.
// ---------------------------------------------------------------------------------------------

/// Kind-code range of [`crate::terrain::TileKind::Deep`] tiles in
/// [`crate::terrain::Tile::code`] (`0xD00 | DeepTile::code`).
pub const DEEP_TILE_CODE_BASE: u64 = 0xD00;

/// A tile of the Old Deep's strata ([`crate::terrain::TileKind::Deep`], V11 worlds only).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum DeepTile {
    // --- Crystal Faults ---
    /// Singing blue crystal along a fault: drilling it rings the crystal around it.
    ResonantCrystal,
    /// A clear prism: it splits the headlamp's light into beams.
    Prism,
    /// A gem hidden in the rock: it looks like plain stone until a refracted beam lights it.
    LatentGem,
    // --- Fossil Oceans ---
    /// Ancient amber: sticky, it holds a rig that touches it.
    Amber,
    /// A bone of a fossil skeleton: dig around it before you take it, or it crumbles.
    FossilBone,
    /// Soft fossil shale around the skeletons and basins.
    Shale,
    // --- Pressure Cathedrals ---
    /// A pressure jet's mouth: it blasts the air above it on a cycle.
    JetVent,
    /// A closed valve of the Cathedral Lift.
    LiftValve,
    /// An opened valve of the Cathedral Lift.
    LiftValveOpen,
    /// The carved stone of the cathedral pillars and ledges.
    NaveStone,
    // --- Radioactive Hollow ---
    /// Rock that glows green: it raises the dose of anyone near it.
    GlowRock,
    /// A cluster of glow isotopes (the layer resource).
    IsotopeCluster,
    /// A relic that changes the rig that breaks it for a while.
    MutationRelic,
    // --- Ancient Machine Layer ---
    /// A conveyor belt running left.
    ConveyorLeft,
    /// A conveyor belt running right.
    ConveyorRight,
    /// A piston's housing in a machine hall's ceiling: its ram slams down once awake.
    PistonHousing,
    /// The builders' machine casing.
    MachineCasing,
    /// A builder tablet: drill it to read it.
    BuilderTablet,
    // --- Void Geode Fields ---
    /// The void-glass shell of a geode.
    GeodeShell,
    /// A violet bloom on a geode's inner wall.
    VoidBloom,
    // --- Mantle Storm Zone ---
    /// A storm vent: it pours molten metal while a heat storm rages.
    StormVent,
    /// Hot cinder rock.
    CinderRock,
    // --- The Star Core (Descent II gate) ---
    /// The Star Core arena's wall: nothing breaks it.
    CoreWall,
    /// A pylon feeding the Star Core's shield.
    CorePylon,
    /// The Star Core behind its shield: nothing breaks it.
    FirstCoreShielded,
    /// The Star Core with its shield down: drill it to strike it.
    FirstCore,
    /// The spent Star Core after the fight.
    FirstCoreSpent,
    // --- Story ---
    /// A cache of the Fathers' expedition: drill it to read their log.
    ExpeditionLog,
}

impl DeepTile {
    /// Every Old Deep tile, in code order.
    pub const ALL: [Self; 28] = [
        Self::ResonantCrystal,
        Self::Prism,
        Self::LatentGem,
        Self::Amber,
        Self::FossilBone,
        Self::Shale,
        Self::JetVent,
        Self::LiftValve,
        Self::LiftValveOpen,
        Self::NaveStone,
        Self::GlowRock,
        Self::IsotopeCluster,
        Self::MutationRelic,
        Self::ConveyorLeft,
        Self::ConveyorRight,
        Self::PistonHousing,
        Self::MachineCasing,
        Self::BuilderTablet,
        Self::GeodeShell,
        Self::VoidBloom,
        Self::StormVent,
        Self::CinderRock,
        Self::CoreWall,
        Self::CorePylon,
        Self::FirstCoreShielded,
        Self::FirstCore,
        Self::FirstCoreSpent,
        Self::ExpeditionLog,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ResonantCrystal => "resonant crystal",
            Self::Prism => "prism",
            Self::LatentGem => "latent gem",
            Self::Amber => "amber",
            Self::FossilBone => "fossil bone",
            Self::Shale => "fossil shale",
            Self::JetVent => "pressure jet",
            Self::LiftValve => "lift valve",
            Self::LiftValveOpen => "open lift valve",
            Self::NaveStone => "nave stone",
            Self::GlowRock => "glow rock",
            Self::IsotopeCluster => "glow isotopes",
            Self::MutationRelic => "mutation relic",
            Self::ConveyorLeft | Self::ConveyorRight => "conveyor",
            Self::PistonHousing => "piston",
            Self::MachineCasing => "machine casing",
            Self::BuilderTablet => "builder tablet",
            Self::GeodeShell => "void glass",
            Self::VoidBloom => "void bloom",
            Self::StormVent => "storm vent",
            Self::CinderRock => "cinder rock",
            Self::CoreWall => "core wall",
            Self::CorePylon => "core pylon",
            Self::FirstCoreShielded => "the shielded Star Core",
            Self::FirstCore => "the Star Core",
            Self::FirstCoreSpent => "the spent Star Core",
            Self::ExpeditionLog => "expedition cache",
        }
    }

    /// Stable code (part of [`crate::terrain::Tile::code`], in the `0xD00` family). Never
    /// renumber: saves and digests use it.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "28 tiles")]
    pub const fn code(self) -> u64 {
        self as u64
    }

    /// The tile with code `code`, if any.
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        if code < Self::ALL.len() as u64 {
            #[allow(clippy::cast_possible_truncation, reason = "checked above")]
            Some(Self::ALL[code as usize])
        } else {
            None
        }
    }

    /// Whether no drill, blast or pick ever breaks it (the arena, the shielded and spent core,
    /// the lift valves: the world changes them by rule).
    #[must_use]
    pub const fn indestructible(self) -> bool {
        matches!(
            self,
            Self::CoreWall
                | Self::FirstCoreShielded
                | Self::FirstCoreSpent
                | Self::LiftValve
                | Self::LiftValveOpen
                | Self::JetVent
        )
    }

    /// Drill strength needed to dig it (Descent II wants the deep drills).
    #[must_use]
    pub const fn hardness(self) -> u8 {
        if self.indestructible() {
            return crate::terrain::UNBREAKABLE_HARDNESS;
        }
        match self {
            Self::Shale | Self::Amber => 2,
            Self::ResonantCrystal | Self::FossilBone | Self::VoidBloom | Self::ExpeditionLog => 3,
            Self::Prism
            | Self::LatentGem
            | Self::NaveStone
            | Self::GlowRock
            | Self::IsotopeCluster
            | Self::MutationRelic
            | Self::ConveyorLeft
            | Self::ConveyorRight
            | Self::BuilderTablet
            | Self::GeodeShell
            | Self::CinderRock
            | Self::StormVent
            | Self::CorePylon
            | Self::FirstCore => 4,
            _ => 5,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    #[allow(
        clippy::match_same_arms,
        reason = "one row per toughness, the rest as hard rock"
    )]
    pub const fn durability(self) -> u8 {
        if self.indestructible() {
            return u8::MAX;
        }
        match self {
            Self::Shale => 3,
            Self::Amber | Self::ResonantCrystal | Self::VoidBloom => 5,
            Self::FossilBone | Self::ExpeditionLog | Self::BuilderTablet => 6,
            Self::Prism | Self::LatentGem | Self::IsotopeCluster | Self::MutationRelic => 8,
            Self::GlowRock | Self::CinderRock | Self::GeodeShell | Self::NaveStone => 10,
            Self::ConveyorLeft | Self::ConveyorRight | Self::StormVent => 12,
            Self::PistonHousing | Self::MachineCasing => 14,
            Self::CorePylon => 18,
            Self::FirstCore => 24,
            _ => 10,
        }
    }

    /// The item a broken tile yields: the layer resources, gems, bones and blocks.
    #[must_use]
    #[allow(
        clippy::match_same_arms,
        reason = "grouped by the stratum each tile belongs to"
    )]
    pub const fn item(self) -> Option<ItemKind> {
        Some(match self {
            Self::ResonantCrystal | Self::Prism => {
                ItemKind::Material(StrategicResourceKind::CrystalLens)
            }
            Self::LatentGem => ItemKind::Mineral(crate::terrain::MineralKind::Diamond),
            Self::Amber => ItemKind::Material(StrategicResourceKind::RadiantFossil),
            Self::FossilBone => ItemKind::Artifact(ArtifactKind::Fossil),
            Self::IsotopeCluster => ItemKind::Material(StrategicResourceKind::CoreShard),
            Self::MachineCasing | Self::PistonHousing => {
                ItemKind::Material(StrategicResourceKind::MachineRelic)
            }
            Self::GeodeShell | Self::VoidBloom => {
                ItemKind::Material(StrategicResourceKind::VoidGlass)
            }
            Self::StormVent => ItemKind::Material(StrategicResourceKind::CoreShard),
            Self::Shale => ItemKind::Block(BlockKind::Stone),
            Self::NaveStone | Self::GlowRock | Self::CinderRock => {
                ItemKind::Block(BlockKind::HardRock)
            }
            _ => return None,
        })
    }

    /// Light it gives off (0-255, the renderer's light map).
    #[must_use]
    pub const fn glow(self) -> u8 {
        match self {
            Self::FirstCore | Self::FirstCoreShielded => 255,
            Self::GlowRock | Self::IsotopeCluster | Self::CorePylon | Self::StormVent => 200,
            Self::ResonantCrystal
            | Self::Prism
            | Self::MutationRelic
            | Self::VoidBloom
            | Self::LiftValveOpen => 150,
            Self::Amber | Self::BuilderTablet | Self::ExpeditionLog | Self::FirstCoreSpent => 90,
            Self::LiftValve | Self::JetVent | Self::CinderRock => 60,
            _ => 0,
        }
    }

    /// Whether the refraction beams pass through it (glass and crystal).
    #[must_use]
    pub const fn translucent(self) -> bool {
        matches!(
            self,
            Self::ResonantCrystal | Self::Prism | Self::GeodeShell | Self::Amber
        )
    }

    /// Whether it is a moving machine part (conveyor or piston).
    #[must_use]
    pub const fn is_machine(self) -> bool {
        matches!(
            self,
            Self::ConveyorLeft | Self::ConveyorRight | Self::PistonHousing
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Timing rules (pure functions of world time, so every tick rate agrees).
// ---------------------------------------------------------------------------------------------

/// Milliseconds of world time at the start of tick `tick` at `hz`.
#[must_use]
pub const fn world_millis(tick: SimulationTick, hz: u32) -> u64 {
    tick.get().saturating_mul(1_000) / if hz == 0 { 1 } else { hz as u64 }
}

/// Whether a whole second of world time ends during tick `tick` (1 Hz rules: dose, storms).
#[must_use]
pub const fn second_boundary(tick: SimulationTick, hz: u32) -> bool {
    let now = world_millis(tick, hz);
    let next = world_millis(tick.next(), hz);
    now / 1_000 != next / 1_000
}

/// The gas tide's cycle: rise, high, fall, low.
pub const GAS_TIDE_MILLIS: u64 = 90_000;
/// The share of the cycle the tide stands high (gas breathes out of the pockets).
pub const GAS_TIDE_HIGH: (u64, u64) = (30_000, 55_000);

/// The gas tide's height `0..=4` rows at `millis` of world time (0 low, 4 high).
#[must_use]
pub const fn gas_tide(millis: u64) -> u8 {
    let phase = millis % GAS_TIDE_MILLIS;
    let (high_from, high_to) = GAS_TIDE_HIGH;
    if phase < high_from {
        // Rising over the first 30 s.
        #[allow(clippy::cast_possible_truncation, reason = "0..=4")]
        let rows = (phase * 5 / high_from) as u8;
        if rows > 4 { 4 } else { rows }
    } else if phase < high_to {
        4
    } else {
        // Falling over the rest.
        let left = GAS_TIDE_MILLIS - phase;
        #[allow(clippy::cast_possible_truncation, reason = "0..=4")]
        let rows = (left * 5 / (GAS_TIDE_MILLIS - high_to)) as u8;
        if rows > 4 { 4 } else { rows }
    }
}

/// Whether the gas tide stands high (the pockets breathe gas) at `millis`.
#[must_use]
pub const fn gas_tide_high(millis: u64) -> bool {
    let phase = millis % GAS_TIDE_MILLIS;
    phase >= GAS_TIDE_HIGH.0 && phase < GAS_TIDE_HIGH.1
}

/// A pressure jet's period and blast.
pub const JET_PERIOD_MILLIS: u64 = 4_200;
pub const JET_ACTIVE_MILLIS: u64 = 1_500;
/// Rows a jet reaches above its vent.
pub const JET_REACH_ROWS: i32 = 12;
/// Upward acceleration of a jet on a rig, px/s².
pub const JET_ACCELERATION: f32 = 1_400.0;
/// The fastest a jet throws a rig upward, px/s.
pub const JET_TOP_SPEED: f32 = 520.0;

/// The jet's phase offset at a vent (neighbouring vents fire out of step, so a climber can
/// time them).
#[must_use]
pub const fn jet_offset(vent: TilePosition) -> u64 {
    let column = vent.x.rem_euclid(5) as u64;
    column * 840
}

/// Whether the jet at `vent` blasts at `millis`.
#[must_use]
pub const fn jet_active(vent: TilePosition, millis: u64) -> bool {
    (millis + jet_offset(vent)) % JET_PERIOD_MILLIS < JET_ACTIVE_MILLIS
}

/// Whether the jet at `vent` is about to fire (the tell: hiss and steam, 700 ms before).
#[must_use]
pub const fn jet_warning(vent: TilePosition, millis: u64) -> bool {
    let phase = (millis + jet_offset(vent)) % JET_PERIOD_MILLIS;
    phase >= JET_PERIOD_MILLIS - 700
}

/// A piston's cycle: rest, slam, hold, retract.
pub const PISTON_PERIOD_MILLIS: u64 = 3_600;
/// The longest a ram reaches below its housing, rows.
pub const PISTON_REACH_ROWS: i32 = 4;
/// Damage of a ram that slams onto a rig.
pub const PISTON_DAMAGE: f32 = 14.0;

/// How far (rows, `0..=PISTON_REACH_ROWS`) the ram under `housing` reaches at `millis`.
#[must_use]
pub fn piston_extension(housing: TilePosition, millis: u64) -> f32 {
    let offset = (housing.x.rem_euclid(3) as u64) * 1_200;
    let phase = (millis + offset) % PISTON_PERIOD_MILLIS;
    #[allow(clippy::cast_precision_loss, reason = "millis within one cycle")]
    let reach = match phase {
        // Resting up in the housing.
        0..1_500 => 0.0,
        // The slam: fast.
        1_500..1_750 => (phase - 1_500) as f32 / 250.0,
        // Held down.
        1_750..2_600 => 1.0,
        // Grinding back up.
        _ => 1.0 - (phase - 2_600) as f32 / 1_000.0,
    };
    #[allow(clippy::cast_precision_loss, reason = "a few rows")]
    let rows = PISTON_REACH_ROWS as f32;
    reach.clamp(0.0, 1.0) * rows
}

/// Conveyor belt speed, px/s.
pub const CONVEYOR_SPEED: f32 = 150.0;
/// Columns either side of a machine hall a rig wakes it from.
pub const MACHINE_WAKE_TILES: i32 = 8;

/// The heat storm's cycle.
pub const STORM_PERIOD_MILLIS: u64 = 150_000;
/// When the warning starts and the storm breaks (within the cycle).
pub const STORM_WARNING_AT: u64 = 105_000;
pub const STORM_BREAKS_AT: u64 = 120_000;

/// The phases of the Mantle Storm Zone's weather.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum StormPhase {
    /// The safe window.
    Calm,
    /// The tell: the air shimmers and the music swells (15 s).
    Gathering,
    /// The heat storm: open caves scorch and storm vents pour (30 s).
    Raging,
}

/// The storm's phase at `millis` of world time.
#[must_use]
pub const fn storm_phase(millis: u64) -> StormPhase {
    let phase = millis % STORM_PERIOD_MILLIS;
    if phase >= STORM_BREAKS_AT {
        StormPhase::Raging
    } else if phase >= STORM_WARNING_AT {
        StormPhase::Gathering
    } else {
        StormPhase::Calm
    }
}

/// Which storm (counting from the world's start) `millis` falls in.
#[must_use]
pub const fn storm_index(millis: u64) -> u64 {
    millis / STORM_PERIOD_MILLIS
}

/// Seconds left in the current phase at `millis` (the HUD's countdown to the next window).
#[must_use]
pub const fn storm_seconds_left(millis: u64) -> u64 {
    let phase = millis % STORM_PERIOD_MILLIS;
    let end = if phase >= STORM_BREAKS_AT {
        STORM_PERIOD_MILLIS
    } else if phase >= STORM_WARNING_AT {
        STORM_BREAKS_AT
    } else {
        STORM_WARNING_AT
    };
    (end - phase).div_ceil(1_000)
}

/// Hull a second a rig in open cave air loses while the storm rages.
pub const STORM_DAMAGE_PER_SECOND: f32 = 9.0;
/// Open tiles in the 7x7 box around a rig at which it counts as exposed to the storm (a
/// rig dug into a tight tunnel shelters).
pub const STORM_EXPOSED_OPEN_TILES: usize = 20;
/// Molten metal cells a storm vent pours each storm.
pub const STORM_VENT_POURS: u8 = 5;

// ---------------------------------------------------------------------------------------------
// Resonance and fossils.
// ---------------------------------------------------------------------------------------------

/// How far (tiles, Chebyshev) a resonance chain runs from a drilled crystal for a drill of
/// `drill_strength`: fast drills ring the fault wide, slow ones barely at all.
#[must_use]
pub const fn resonance_reach(drill_strength: u8) -> i32 {
    match drill_strength {
        0..=3 => 1,
        4 => 2,
        5 => 3,
        _ => 4,
    }
}

/// The most crystals one chain shatters.
pub const RESONANCE_MAX_SHATTER: usize = 14;
/// Shattered crystals at which a chain without a Resonance Drill shakes a boulder loose.
pub const RESONANCE_CAVE_IN_AT: usize = 5;

/// Whether a fossil bone is free enough to lift out whole: at most one of its four sides
/// still touches rock (other bones of its skeleton do not count).
#[must_use]
pub const fn bone_exposed(solid_sides: usize) -> bool {
    solid_sides <= 1
}

/// Credits a complete, unbroken skeleton earns its excavator (the museum's bounty), per bone.
pub const SKELETON_BOUNTY_PER_BONE: u32 = 180;

/// Rig speed (px/s) amber lets through while a rig touches it.
pub const AMBER_SPEED_CAP: f32 = 34.0;

// ---------------------------------------------------------------------------------------------
// The dose meter and mutations.
// ---------------------------------------------------------------------------------------------

/// The dose meter's full scale.
pub const DOSE_MAX: u16 = 1_000;
/// Dose a second anywhere in the Radioactive Hollow.
pub const DOSE_HOLLOW_PER_SECOND: u16 = 4;
/// Extra dose a second for each glow rock (isotopes count double) within
/// [`DOSE_GLOW_RADIUS`] tiles, up to [`DOSE_GLOW_CAP`].
pub const DOSE_PER_GLOW: u16 = 2;
pub const DOSE_GLOW_RADIUS: i32 = 3;
pub const DOSE_GLOW_CAP: u16 = 18;
/// Dose that drains a second outside the hollow (and twice that near a lit outpost).
pub const DOSE_DECAY_PER_SECOND: u16 = 6;
/// The dose levels at which the meter warns and the effects step up.
pub const DOSE_LEVELS: [u16; 3] = [400, 700, 1_000];
/// Hull a second at a full meter.
pub const DOSE_HULL_PER_SECOND: f32 = 3.0;

/// A rig's dose meter.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Dose {
    /// `0..=DOSE_MAX`.
    pub level: u16,
    /// The highest [`DOSE_LEVELS`] step warned about since the meter last fell below it.
    pub warned: u8,
}

impl Dose {
    /// The effect step `0..=3` of the meter.
    #[must_use]
    pub fn step(self) -> u8 {
        let step = DOSE_LEVELS
            .iter()
            .filter(|&&level| self.level >= level)
            .count();
        u8::try_from(step).unwrap_or(3)
    }

    /// Extra fuel a second the dose costs, as a share of the burn (sick crews fly badly).
    #[must_use]
    pub fn fuel_penalty(self) -> f32 {
        match self.step() {
            0 => 0.0,
            1 => 0.2,
            _ => 0.45,
        }
    }

    /// Whether the scanner is blinded by the dose.
    #[must_use]
    pub fn scanner_blinded(self) -> bool {
        self.step() >= 2
    }

    /// The dose as a share of the meter, `0..=1` (the HUD and the screen tint).
    #[must_use]
    pub fn fraction(self) -> f32 {
        f32::from(self.level.min(DOSE_MAX)) / f32::from(DOSE_MAX)
    }
}

/// The share of dose a rig's gear keeps out, in percent: each gas scrubber tier seals 15 %
/// (up to 60 %), the Pressure Hull's lining 30 %, a Lead Skin mutation 75 %; the rest stacks
/// multiplicatively.
#[must_use]
pub fn dose_shield_percent(scrubber: u8, pressure_hull: bool, lead_skin: bool) -> u32 {
    let mut keep = 100 - u32::from(scrubber.min(4)) * 15;
    if pressure_hull {
        keep = keep * 70 / 100;
    }
    if lead_skin {
        keep = keep * 25 / 100;
    }
    100 - keep
}

/// What a mutation relic does to the rig that breaks it, for [`MUTATION_MILLIS`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Mutation {
    /// Glowing skin: the headlamp reaches further and lights latent gems on its own.
    Lumen,
    /// Lead skin: the dose meter barely moves.
    LeadSkin,
    /// Regrowing plates: the hull mends itself.
    Regrowth,
    /// A strange metabolism: the tank slowly refills.
    FuelGland,
}

impl Mutation {
    pub const ALL: [Self; 4] = [Self::Lumen, Self::LeadSkin, Self::Regrowth, Self::FuelGland];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lumen => "Lumen Skin",
            Self::LeadSkin => "Lead Skin",
            Self::Regrowth => "Regrowth",
            Self::FuelGland => "Fuel Gland",
        }
    }

    /// The mutation a relic at `relic` grants (a fixed pick of its place).
    #[must_use]
    pub const fn of_relic(seed: u64, relic: TilePosition) -> Self {
        let roll = crate::worldgen::noise::hash2(seed, 0x0D_4E1A, relic.x, relic.y);
        Self::ALL[(roll % 4) as usize]
    }
}

/// How long a mutation lasts.
pub const MUTATION_MILLIS: u64 = 300_000;
/// Dose a mutation relic gives as it breaks.
pub const MUTATION_DOSE: u16 = 150;
/// Hull a second Regrowth mends.
pub const REGROWTH_PER_SECOND: f32 = 1.5;
/// Fuel a second a Fuel Gland makes.
pub const FUEL_GLAND_PER_SECOND: f32 = 0.6;

/// A mutation a rig carries.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActiveMutation {
    pub mutation: Mutation,
    pub until: SimulationTick,
}

// ---------------------------------------------------------------------------------------------
// The Cathedral Lift and the Star Core.
// ---------------------------------------------------------------------------------------------

/// How long a rig holds a valve to open it.
pub const VALVE_HOLD_MILLIS: u64 = 3_000;
/// How close (px from the valve's centre) a rig must stay to hold it.
pub const VALVE_REACH_PX: f32 = 30.0;

/// The Cathedral Lift set piece's progress.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CathedralLift {
    /// Valves opened (bit per valve, [`sites::CathedralSite::valves`] order).
    pub opened: u8,
    /// Valves being held: valve index to the holder and the tick they started.
    pub holds: BTreeMap<u8, (PlayerId, SimulationTick)>,
    /// Whether the lift runs.
    pub restored: bool,
}

/// The Star Core fight's phases.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum CorePhase {
    /// Asleep until a rig enters the arena.
    #[default]
    Dormant,
    /// Shielded: its pylons feed the shield; it throws flares and sparks.
    Shielded,
    /// Its pylons are down: the core lies open for a while.
    Exposed,
    /// Beaten: the core is spent and the way below revealed.
    Beaten,
}

/// Strikes on the open core that beat it.
pub const CORE_STRIKES: u8 = 3;
/// How long the core lies open once its pylons fall.
pub const CORE_EXPOSED_MILLIS: u64 = 16_000;
/// Flares: one volley every this long while the core is awake (faster as it weakens).
pub const CORE_FLARE_MILLIS: u64 = 2_600;
/// A spark (a core wisp) every this long while shielded, up to [`CORE_MAX_SPARKS`] alive.
pub const CORE_SPARK_MILLIS: u64 = 11_000;
pub const CORE_MAX_SPARKS: usize = 3;
/// How long the arena must stand empty before an unfinished fight resets.
pub const CORE_RESET_MILLIS: u64 = 10_000;
/// Damage of one core flare.
pub const CORE_FLARE_DAMAGE: f32 = 9.0;

/// The Star Core fight.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct StarCoreFight {
    pub phase: CorePhase,
    /// Strikes left before the core is spent.
    pub strikes_left: u8,
    /// When the open core closes again.
    pub exposed_until: SimulationTick,
    pub next_flare: SimulationTick,
    pub next_spark: SimulationTick,
    /// When the arena last held a rig.
    pub last_seen: SimulationTick,
    /// Whoever landed the last strike.
    pub beaten_by: Option<PlayerId>,
}

/// The Glowmother, Descent II's mini-boss (Radioactive Hollow).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MiniBoss {
    /// The entity while she is awake.
    pub entity: Option<u64>,
    pub defeated: bool,
}

// ---------------------------------------------------------------------------------------------
// World state.
// ---------------------------------------------------------------------------------------------

/// A skeleton's excavation.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkeletonDig {
    pub freed: u8,
    pub broken: u8,
    pub completed: bool,
}

/// The Old Deep's world state (the `OldDeep` replication section). Empty, unhashed and unsent
/// in every world nobody has touched it in (every classic world).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OldDeep {
    #[serde(default)]
    pub doses: BTreeMap<PlayerId, Dose>,
    #[serde(default)]
    pub mutations: BTreeMap<PlayerId, ActiveMutation>,
    /// Machine halls awake (by region).
    #[serde(default)]
    pub woken_halls: BTreeSet<(i32, i32)>,
    /// Skeleton excavations (by region).
    #[serde(default)]
    pub skeletons: BTreeMap<(i32, i32), SkeletonDig>,
    #[serde(default)]
    pub cathedral: CathedralLift,
    #[serde(default)]
    pub core: StarCoreFight,
    #[serde(default)]
    pub glowmother: MiniBoss,
    /// Story logs read (crew-wide).
    #[serde(default)]
    pub logs_read: BTreeSet<u8>,
    /// Storm vents' pours this storm: the storm's index and each vent's count.
    #[serde(default)]
    pub pours: (u64, BTreeMap<(i32, i32), u8>),
    /// Gas pockets that breathed this tide: the tide's index and the pockets.
    #[serde(default)]
    pub breathed: (u64, BTreeSet<(i32, i32)>),
    /// Rigs stuck in amber (for the one warning per touch).
    #[serde(default)]
    pub stuck: BTreeSet<PlayerId>,
}

impl OldDeep {
    /// Whether nothing has happened yet (left out of the state hash).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Drops `player`'s per-rig state (they left the world).
    pub fn forget(&mut self, player: PlayerId) {
        self.doses.remove(&player);
        self.mutations.remove(&player);
        self.stuck.remove(&player);
        self.cathedral
            .holds
            .retain(|_, (holder, _)| *holder != player);
    }

    /// `player`'s dose meter.
    #[must_use]
    pub fn dose(&self, player: PlayerId) -> Dose {
        self.doses.get(&player).copied().unwrap_or_default()
    }

    /// `player`'s mutation, if one is active at `now`.
    #[must_use]
    pub fn mutation(&self, player: PlayerId, now: SimulationTick) -> Option<Mutation> {
        self.mutations
            .get(&player)
            .filter(|active| active.until > now)
            .map(|active| active.mutation)
    }
}

/// What the Old Deep authored in `stratum` beyond its registry entry, for the descent harness.
///
/// `balance::descent::RegistryStrata` counts its signature mechanics and its set pieces (each
/// stratum's Fathers' camp; the Cathedral Lift, the Glowmother and the Star Core).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "one row per stratum, with what it holds"
)]
pub const fn authored_content(stratum: crate::descent::Stratum) -> (u8, u8) {
    use crate::descent::Stratum;
    match stratum {
        // Resonance chains and refraction.
        Stratum::CrystalFaults => (2, 1),
        // Gas tides, amber and skeleton digs.
        Stratum::FossilOceans => (3, 1),
        // Pressure jets; the Cathedral Lift.
        Stratum::PressureCathedrals => (1, 2),
        // The dose meter and mutations; the Glowmother.
        Stratum::RadioactiveHollow => (2, 2),
        // Conveyors and pistons (and the builder tablets).
        Stratum::AncientMachineLayer => (2, 1),
        // Geode gravity.
        Stratum::VoidGeodeFields => (1, 1),
        // Heat storms and storm vents; the Star Core.
        Stratum::MantleStormZone => (2, 2),
        _ => (0, 0),
    }
}

/// The story logs of the Old Deep: the Fathers' expedition (one per stratum, top to bottom),
/// then the builder tablets, then the Star Core's revelation.
pub const LOG_EXPEDITION_FIRST: u8 = 0;
pub const LOG_TABLET_FIRST: u8 = 7;
pub const LOG_TABLETS: u8 = 4;
pub const LOG_CORE_REVEAL: u8 = 11;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_codes_round_trip() {
        for tile in DeepTile::ALL {
            assert_eq!(DeepTile::from_code(tile.code()), Some(tile));
        }
        assert_eq!(DeepTile::from_code(200), None);
    }

    #[test]
    fn cycles_agree_across_tick_rates() {
        for hz in [20, 30, 60, 120] {
            let ticks = u64::from(hz) * 300;
            let mut seconds = 0;
            for tick in 0..ticks {
                if second_boundary(SimulationTick::new(tick), hz) {
                    seconds += 1;
                }
            }
            assert_eq!(seconds, 300, "{hz} Hz");
        }
        assert_eq!(world_millis(SimulationTick::new(60), 60), 1_000);
        assert_eq!(world_millis(SimulationTick::new(30), 30), 1_000);
    }

    #[test]
    fn the_gas_tide_rises_stands_and_falls() {
        assert_eq!(gas_tide(0), 0);
        assert_eq!(gas_tide(40_000), 4);
        assert!(gas_tide_high(40_000));
        assert!(!gas_tide_high(10_000));
        assert!(gas_tide(80_000) < 4);
    }

    #[test]
    fn storms_warn_before_they_break() {
        assert_eq!(storm_phase(0), StormPhase::Calm);
        assert_eq!(storm_phase(110_000), StormPhase::Gathering);
        assert_eq!(storm_phase(125_000), StormPhase::Raging);
        assert_eq!(storm_phase(150_000), StormPhase::Calm);
        assert_eq!(storm_seconds_left(100_000), 5);
    }

    #[test]
    fn pistons_slam_and_retract() {
        let housing = TilePosition { x: 0, y: 0 };
        assert!(piston_extension(housing, 0) < 0.01);
        assert!((piston_extension(housing, 2_000) - 4.0).abs() < 0.01);
        assert!(piston_extension(housing, 3_000) < 4.0);
    }

    #[test]
    fn dose_steps_and_shields() {
        let dose = Dose {
            level: 750,
            warned: 0,
        };
        assert_eq!(dose.step(), 2);
        assert!(dose.scanner_blinded());
        assert_eq!(dose_shield_percent(0, false, false), 0);
        assert_eq!(dose_shield_percent(4, true, false), 72);
        assert!(dose_shield_percent(0, false, true) >= 75);
    }
}
