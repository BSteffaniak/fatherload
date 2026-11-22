//! The Deep Descent framework (expansion plan Part III, F8): the eight **Descents** (chapters of
//! the dig) and their 41 **strata**, as a data-driven registry.
//!
//! Every stratum is one [`StratumDef`] in [`registry::STRATA`]: its Descent, name and signature
//! rule, its share of its Descent's rows, the classic band it borrows rules from while it is a
//! placeholder ([`StratumDef::host_band`]), its rock, palette, ores and their depth curves, its
//! layer resource, creature table, hazards, field-effect profile ([`FieldProfile`], plain data
//! the field-effects track interprets), music and ambience cues, landmark generators, seed
//! variants and the gate at the end of its Descent.
//!
//! Generator [`GeneratorVersion::V11`](crate::worldgen::GeneratorVersion::V11) lays the strata
//! out ([`crate::worldgen::descent`]); older worlds map their twelve classic bands onto the
//! twelve strata of Descents I and II ([`Stratum::of_band`]), so every world can answer "which
//! stratum is this" and every HUD can name it.
//!
//! The registry is the seam later waves fill: Wave 8 replaces the placeholder values of
//! Descents I to III, Wave 9 those of IV to VI and Wave 10 those of VII and VIII, without
//! touching the generator's layout code.

pub mod fields;
pub mod registry;

use serde::{Deserialize, Serialize};

use crate::{
    enemy::EnemyKind,
    terrain::{DeepStratum, MineralKind, StrategicResourceKind, TileKind},
    worldgen::{Band, HazardKind},
};

pub use registry::STRATA;

/// One chapter of the dig.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Descent {
    Crust,
    OldDeep,
    LivingDark,
    DrownedAndFrozen,
    HollowWorld,
    StrangeLayers,
    Ancients,
    Heart,
}

impl Descent {
    /// Every Descent, top to bottom.
    pub const ALL: [Self; 8] = [
        Self::Crust,
        Self::OldDeep,
        Self::LivingDark,
        Self::DrownedAndFrozen,
        Self::HollowWorld,
        Self::StrangeLayers,
        Self::Ancients,
        Self::Heart,
    ];

    /// Position in [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn from_index(index: usize) -> Self {
        if index < Self::ALL.len() {
            Self::ALL[index]
        } else {
            Self::Heart
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Crust => "The Crust",
            Self::OldDeep => "The Old Deep",
            Self::LivingDark => "The Living Dark",
            Self::DrownedAndFrozen => "The Drowned and Frozen",
            Self::HollowWorld => "The Hollow World",
            Self::StrangeLayers => "The Strange Layers",
            Self::Ancients => "The Ancients",
            Self::Heart => "The Heart",
        }
    }

    /// The chapter number as players see it ("I" to "VIII").
    #[must_use]
    pub const fn numeral(self) -> &'static str {
        match self {
            Self::Crust => "I",
            Self::OldDeep => "II",
            Self::LivingDark => "III",
            Self::DrownedAndFrozen => "IV",
            Self::HollowWorld => "V",
            Self::StrangeLayers => "VI",
            Self::Ancients => "VII",
            Self::Heart => "VIII",
        }
    }

    /// Target rows of a mean V11 world, `(first, end)` measured from the ground line
    /// (expansion plan §14; seeds vary each Descent's thickness by up to a tenth).
    #[must_use]
    pub const fn target_rows(self) -> (i32, i32) {
        match self {
            Self::Crust => (0, 1_100),
            Self::OldDeep => (1_100, 2_700),
            Self::LivingDark => (2_700, 4_200),
            Self::DrownedAndFrozen => (4_200, 5_800),
            Self::HollowWorld => (5_800, 7_600),
            Self::StrangeLayers => (7_600, 9_400),
            Self::Ancients => (9_400, 11_400),
            Self::Heart => (11_400, 13_000),
        }
    }

    /// Target rows of the Descent in a mean world (`end - first` of [`Self::target_rows`]).
    #[must_use]
    pub const fn target_thickness(self) -> i32 {
        let (first, end) = self.target_rows();
        end - first
    }

    /// Cumulative play hours a focused player reaches the end of the Descent in (the balance
    /// harness's target).
    #[must_use]
    pub const fn target_hours(self) -> (u16, u16) {
        match self {
            Self::Crust => (0, 7),
            Self::OldDeep => (7, 16),
            Self::LivingDark => (16, 26),
            Self::DrownedAndFrozen => (26, 36),
            Self::HollowWorld => (36, 48),
            Self::StrangeLayers => (48, 60),
            Self::Ancients => (60, 75),
            Self::Heart => (75, 90),
        }
    }

    /// The story beat the chapter reveals (the Fathers' expedition thread).
    #[must_use]
    pub const fn story_beat(self) -> &'static str {
        match self {
            Self::Crust => "The world was shaped.",
            Self::OldDeep => "The builders were here long ago.",
            Self::LivingDark => "Life adapted to the dark.",
            Self::DrownedAndFrozen => "There are whole civilisations below.",
            Self::HollowWorld => "The planet is hollow, with an inner sun.",
            Self::StrangeLayers => "Reality thins near the builders' machine.",
            Self::Ancients => "The builders' city and their archive.",
            Self::Heart => "The core is a sleeping Star Drive.",
        }
    }

    /// The Descent's strata, top to bottom.
    pub fn strata(self) -> impl Iterator<Item = Stratum> {
        Stratum::ALL
            .into_iter()
            .filter(move |stratum| stratum.descent() == self)
    }

    /// The Descent's first stratum.
    #[must_use]
    pub const fn first_stratum(self) -> Stratum {
        let mut index = 0;
        while index < Stratum::ALL.len() {
            if Stratum::ALL[index].def().descent as usize == self as usize {
                return Stratum::ALL[index];
            }
            index += 1;
        }
        Stratum::TheHeart
    }

    /// The Descent's last stratum (the one holding its gate).
    #[must_use]
    pub const fn last_stratum(self) -> Stratum {
        let mut index = Stratum::ALL.len();
        while index > 0 {
            index -= 1;
            if Stratum::ALL[index].def().descent as usize == self as usize {
                return Stratum::ALL[index];
            }
        }
        Stratum::Topsoil
    }

    /// The gate that ends the Descent (held by its last stratum).
    #[must_use]
    pub const fn gate(self) -> Option<&'static GateDef> {
        self.last_stratum().def().gate.as_ref()
    }
}

/// One of the 41 strata, top to bottom.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[repr(u8)]
pub enum Stratum {
    // Descent I: The Crust.
    Topsoil,
    ClayBeds,
    GreyStone,
    SlateShelves,
    BasaltColumns,
    // Descent II: The Old Deep.
    CrystalFaults,
    FossilOceans,
    PressureCathedrals,
    RadioactiveHollow,
    AncientMachineLayer,
    VoidGeodeFields,
    MantleStormZone,
    // Descent III: The Living Dark.
    FungalForest,
    RootLabyrinth,
    TheHive,
    FleshCaverns,
    GlowSea,
    // Descent IV: The Drowned and Frozen.
    UndergroundOcean,
    BrineGlaciers,
    GeyserFields,
    MerrowCity,
    PearlAbyss,
    // Descent V: The Hollow World.
    InnerSky,
    MegafaunaJungle,
    UpsideDownIsles,
    TheSunforge,
    // Descent VI: The Strange Layers.
    MirrorStrata,
    GravityShear,
    PixelRot,
    TimeSludge,
    EchoCaves,
    PrismDesert,
    // Descent VII: The Ancients.
    BuriedMegacity,
    EngineHalls,
    ArchiveOfStars,
    SleepingGarrison,
    TheSealGate,
    // Descent VIII: The Heart.
    DarkMatterVeins,
    SingularityFields,
    CoreOcean,
    TheHeart,
}

/// Number of strata.
pub const STRATUM_COUNT: usize = 41;

impl Stratum {
    /// Every stratum, top to bottom.
    pub const ALL: [Self; STRATUM_COUNT] = [
        Self::Topsoil,
        Self::ClayBeds,
        Self::GreyStone,
        Self::SlateShelves,
        Self::BasaltColumns,
        Self::CrystalFaults,
        Self::FossilOceans,
        Self::PressureCathedrals,
        Self::RadioactiveHollow,
        Self::AncientMachineLayer,
        Self::VoidGeodeFields,
        Self::MantleStormZone,
        Self::FungalForest,
        Self::RootLabyrinth,
        Self::TheHive,
        Self::FleshCaverns,
        Self::GlowSea,
        Self::UndergroundOcean,
        Self::BrineGlaciers,
        Self::GeyserFields,
        Self::MerrowCity,
        Self::PearlAbyss,
        Self::InnerSky,
        Self::MegafaunaJungle,
        Self::UpsideDownIsles,
        Self::TheSunforge,
        Self::MirrorStrata,
        Self::GravityShear,
        Self::PixelRot,
        Self::TimeSludge,
        Self::EchoCaves,
        Self::PrismDesert,
        Self::BuriedMegacity,
        Self::EngineHalls,
        Self::ArchiveOfStars,
        Self::SleepingGarrison,
        Self::TheSealGate,
        Self::DarkMatterVeins,
        Self::SingularityFields,
        Self::CoreOcean,
        Self::TheHeart,
    ];

    /// Position in [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn from_index(index: usize) -> Self {
        if index < Self::ALL.len() {
            Self::ALL[index]
        } else {
            Self::TheHeart
        }
    }

    /// The stratum's registry entry.
    #[must_use]
    pub const fn def(self) -> &'static StratumDef {
        &STRATA[self as usize]
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.def().name
    }

    #[must_use]
    pub const fn descent(self) -> Descent {
        self.def().descent
    }

    /// The classic band whose rules the stratum borrows while it is a placeholder.
    #[must_use]
    pub const fn host_band(self) -> Band {
        self.def().host_band
    }

    /// The stratum of Descents I and II that stands for classic band `band` (every world
    /// before V11 maps its bands this way).
    #[must_use]
    pub const fn of_band(band: Band) -> Self {
        match band {
            Band::Topsoil => Self::Topsoil,
            Band::Clay => Self::ClayBeds,
            Band::Stone => Self::GreyStone,
            Band::Slate => Self::SlateShelves,
            Band::Basalt => Self::BasaltColumns,
            Band::Deep(DeepStratum::CrystalFaults) => Self::CrystalFaults,
            Band::Deep(DeepStratum::FossilOceans) => Self::FossilOceans,
            Band::Deep(DeepStratum::PressureCathedrals) => Self::PressureCathedrals,
            Band::Deep(DeepStratum::RadioactiveHollow) => Self::RadioactiveHollow,
            Band::Deep(DeepStratum::AncientMachineLayer) => Self::AncientMachineLayer,
            Band::Deep(DeepStratum::VoidGeodeFields) => Self::VoidGeodeFields,
            Band::Deep(DeepStratum::MantleStormZone) => Self::MantleStormZone,
        }
    }

    /// Whether the stratum is one of the twelve classic bands (Descents I and II), which keep
    /// the classic rules and art.
    #[must_use]
    pub const fn is_classic(self) -> bool {
        (self as usize) < Band::ALL.len()
    }

    /// Whether generator V11 reads the stratum's own registry rules (ores, ore density and
    /// hazards) rather than its classic band's curves: every stratum below Descent II, and the
    /// authored Crust (Wave 8). The Old Deep keeps the classic curves until its Wave authors it.
    #[must_use]
    pub const fn uses_registry_rules(self) -> bool {
        !self.is_classic() || matches!(self.def().descent, Descent::Crust)
    }

    /// The enemy tier (health, damage and loot scale) of the stratum's creatures: the classic
    /// band index in Descents I and II, the deepest classic tier below them.
    #[must_use]
    pub const fn enemy_tier(self) -> u8 {
        if self.is_classic() {
            self as u8
        } else {
            Self::MantleStormZone as u8
        }
    }
}

/// An sRGB colour (plain data: the renderer maps it onto its own palette).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// How a stratum's rock looks.
///
/// The body colours of its soft and hard rock, the back wall of dug tunnels and an optional
/// accent for glints. The renderer builds its shade ramps from
/// these (Descents I and II keep their classic hand-tuned art).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StratumPalette {
    pub rock: Rgb,
    pub hard: Rgb,
    pub back: Rgb,
    pub accent: Option<Rgb>,
}

/// A mineral's depth curve inside one stratum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OreCurve {
    pub mineral: MineralKind,
    /// Where through the stratum (per mille of its rows, top to bottom) the mineral peaks.
    pub peak_per_mille: u16,
    /// How far either side of the peak (per mille of the rows) the mineral fades out over.
    pub spread_per_mille: u16,
    /// Relative weight at the peak.
    pub weight: u32,
}

impl OreCurve {
    /// The curve's weight `along` per mille of the way through the stratum (a tenth of the
    /// peak weight still shows at the edges, so a stratum's ores never vanish entirely).
    #[must_use]
    pub const fn weight_at(&self, along: u16) -> u32 {
        let distance = along.abs_diff(self.peak_per_mille) as u32;
        let spread = if self.spread_per_mille == 0 {
            1
        } else {
            self.spread_per_mille as u32
        };
        let share = if distance >= spread {
            0
        } else {
            (spread - distance) * 100 / spread
        };
        self.weight * (10 + share * 9 / 10) / 100
    }
}

/// The crafting resource a stratum is known for. Placeholder strata name a resource and the
/// existing strategic resource that stands in for it until its own item exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayerResource {
    pub name: &'static str,
    pub stands_in: Option<StrategicResourceKind>,
}

/// A whole-screen effect a stratum's field profile asks for (the post-process pipeline of the
/// field-effects track draws them).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ScreenEffect {
    Pixelate,
    PaletteReduce,
    Exposure,
    Bloom,
    Desaturate,
    ColourGrade,
    ChromaticAberration,
    Mirror,
    Wobble,
    Lensing,
    Scanlines,
    Vignette,
    Fog,
    Blur,
}

impl ScreenEffect {
    pub const ALL: [Self; 14] = [
        Self::Pixelate,
        Self::PaletteReduce,
        Self::Exposure,
        Self::Bloom,
        Self::Desaturate,
        Self::ColourGrade,
        Self::ChromaticAberration,
        Self::Mirror,
        Self::Wobble,
        Self::Lensing,
        Self::Scanlines,
        Self::Vignette,
        Self::Fog,
        Self::Blur,
    ];
}

/// One screen effect at an intensity (per mille of full).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScreenEffectLevel {
    pub effect: ScreenEffect,
    pub intensity_permille: u16,
}

/// A stratum's field-effect profile.
///
/// Plain integer data (deterministic and hashable) the field-effects track (F9) interprets for rigs, pilots, creatures and drops, and for the
/// screen. [`FieldProfile::NEUTRAL`] changes nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldProfile {
    /// Gravity's direction and strength in thousandths of standard gravity (`(0, 1000)` is
    /// standard, straight down; screen y grows downwards).
    pub gravity_milli: (i32, i32),
    /// Speed of time, per mille (1000 is normal).
    pub time_scale_permille: u16,
    /// Ground friction, per mille of standard.
    pub friction_permille: u16,
    /// Air (or fluid) drag, per mille of standard.
    pub drag_permille: u16,
    /// Ambient light, per mille of the standard underground light.
    pub light_permille: u16,
    /// Whether horizontal controls are mirrored.
    pub mirror_controls: bool,
    /// The screen-effect stack, in drawing order.
    pub screen: &'static [ScreenEffectLevel],
    /// Rigs may turn all the way round anywhere in the stratum (a 360-degree zone; Wave 7
    /// integration). Off in every other stratum, where the home world's lean limit holds.
    pub free_rotation: bool,
    /// Regions of the stratum where the field departs further (gravity turns, mirroring).
    pub regions: FieldRegions,
    /// The screen effects grow from half strength at the stratum's top to full at its bottom
    /// (Pixel Rot: the deeper, the lower the resolution).
    pub deepening: bool,
}

/// Where inside a stratum its field departs further.
///
/// Region cells ([`FIELD_REGION_TILES`]): a seeded share of which hold the departure, feathered over
/// [`FIELD_REGION_FEATHER_TILES`] at their edges (Wave 7 integration; read by
/// [`fields::stratum_field_source`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldRegions {
    /// The field is the same all through the stratum.
    None,
    /// In `per_mille` of the cells gravity turns clockwise by one of `turns` (degrees: 90, 180
    /// or 270; the Gravity Shear's walls and ceilings, the Upside-Down Isles).
    GravityTurns {
        per_mille: u16,
        turns: &'static [u16],
    },
    /// In `per_mille` of the cells the controls and the screen mirror (the Mirror Strata).
    Mirrored { per_mille: u16 },
}

/// Size (columns, rows) of a field region cell.
pub const FIELD_REGION_TILES: (i32, i32) = (48, 32);
/// Tiles over which a region cell's departure fades in from its edges.
pub const FIELD_REGION_FEATHER_TILES: f32 = 6.0;

impl FieldProfile {
    /// Standard gravity, time, friction, drag and light, no mirroring and no screen effects.
    pub const NEUTRAL: Self = Self {
        gravity_milli: (0, 1_000),
        time_scale_permille: 1_000,
        friction_permille: 1_000,
        drag_permille: 1_000,
        light_permille: 1_000,
        mirror_controls: false,
        screen: &[],
        free_rotation: false,
        regions: FieldRegions::None,
        deepening: false,
    };

    /// Whether the profile changes nothing.
    #[must_use]
    pub const fn is_neutral(&self) -> bool {
        self.gravity_milli.0 == 0
            && self.gravity_milli.1 == 1_000
            && self.time_scale_permille == 1_000
            && self.friction_permille == 1_000
            && self.drag_permille == 1_000
            && self.light_permille == 1_000
            && !self.mirror_controls
            && self.screen.is_empty()
            && !self.free_rotation
            && matches!(self.regions, FieldRegions::None)
    }
}

impl Default for FieldProfile {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// A Descent's music theme (the audio director picks the piece; one per Descent until each
/// stratum gets its own).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum MusicCue {
    Crust,
    OldDeep,
    LivingDark,
    DrownedAndFrozen,
    HollowWorld,
    StrangeLayers,
    Ancients,
    Heart,
    // --- The authored Crust's own pieces (Wave 8), appended ----------------------------------
    /// Topsoil: a plucked folk line over creaking roots.
    Topsoil,
    /// Clay Beds: a dripping marimba and the gurgle of water behind the clay.
    ClayBeds,
    /// Grey Stone: a reedy work song over the old works' far clanks.
    GreyStone,
    /// Slate Shelves: a glassy, slow pad and ringing drips.
    SlateShelves,
    /// Basalt Columns: a low drone and the crackle of heat.
    BasaltColumns,
    // --- The Living Dark's own pieces (Wave 8), appended ------------------------------------
    /// Fungal Forest: soft plucked bells over a breathing pad, spores in the air.
    FungalForest,
    /// Root Labyrinth: a creaking, tightening ostinato of low strings.
    RootLabyrinth,
    /// The Hive: a buzzing drone and a busy marimba of the comb.
    Hive,
    /// Flesh Caverns: a heartbeat that drives the tempo, wet throbbing bass.
    FleshCaverns,
    /// Glow Sea: near silence, slow glass chimes in a deep reverb.
    GlowSea,
    /// The Mycelial Mind's fight.
    MycelialMind,
}

/// A stratum's ambience bed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum AmbienceCue {
    /// Dripping water and settling rock (the classic underground bed).
    Drips,
    /// Wind through open space.
    Wind,
    /// Crackling heat and distant lava.
    Lava,
    /// Machinery humming and clanking.
    Machines,
    /// Water all around: currents, bubbles, far whale song.
    Water,
    /// Creaking ice.
    Ice,
    /// Wet, living sounds: a heartbeat, breathing walls, chittering.
    Living,
    /// Hums and glitches of a thinning reality.
    Strange,
    /// Near silence.
    Silence,
    // --- The authored Crust's beds (Wave 8), appended -----------------------------------------
    /// Roots creaking and the scratching of burrowers.
    Burrows,
    /// Water moving behind clay: gurgles and seeps.
    Aquifer,
    /// The old mine works: timber groaning, far rails, a dripping lamp.
    MineWorks,
    /// Glassy drips off stalactites and shelves ticking as they settle.
    SlateDrip,
    /// The lava tubes' hot breath and crackle.
    LavaTube,
    // --- The Living Dark (Wave 8), appended -------------------------------------------------
    /// Puffing spores, soft fungal creaks.
    Spores,
    /// Creaking roots that tighten.
    Roots,
    /// The hive's hum and chitter.
    Hive,
    /// A heartbeat and gurgling acid.
    Heartbeat,
    /// The deep dark: far bubbles, a whale-like moan.
    Abyss,
}

/// What kind of landmark a stratum holds; every kind has a generator
/// ([`crate::worldgen::descent::landmark_generator`]).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LandmarkKind {
    /// The Descent's deep outpost (a placeholder hall until the logistics track turns it into
    /// a claimable base with fuel, repair, a shop and a respawn point). One per Descent.
    Outpost,
    /// The set-piece chamber at the end of a Descent where its gate stands.
    GateChamber,
    /// The Hollow World's great open cavern with floating stones.
    HollowCavern,
    /// A rare room where reality misbehaves (gravity, pixel rot, time, mirrors).
    AnomalyRoom,
    /// Hand-authored prefab points of interest (caches, derelicts, vaults, cabins).
    Prefabs,
    /// Cave ecosystems (grottos, geodes, lava tubes, flooded caves, ruins).
    CaveEcosystems,
    /// Villages and underground settlements (Gravel Hollow).
    Settlements,
    /// Tomb and crypt sites.
    Tombs,
    /// The Living Dark's set pieces (Wave 8): cap groves, heartroots, the Hive's galleries and
    /// the Amber Court, gullets, abyssal halls and the Mind's hollow
    /// ([`crate::worldgen::descent::living`]).
    LivingDark,
}

impl LandmarkKind {
    pub const ALL: [Self; 9] = [
        Self::Outpost,
        Self::GateChamber,
        Self::HollowCavern,
        Self::AnomalyRoom,
        Self::Prefabs,
        Self::CaveEcosystems,
        Self::Settlements,
        Self::Tombs,
        Self::LivingDark,
    ];
}

/// A key item, rig system or story key a gate hands out or asks for.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum DescentKey {
    DeepClaim,
    DrillII,
    PressureHull,
    BioFilter,
    LampII,
    SonarSuite,
    CryoHull,
    GravityAnchor,
    PhaseDrill,
    HeartKey,
    StarDrive,
}

impl DescentKey {
    pub const ALL: [Self; 11] = [
        Self::DeepClaim,
        Self::DrillII,
        Self::PressureHull,
        Self::BioFilter,
        Self::LampII,
        Self::SonarSuite,
        Self::CryoHull,
        Self::GravityAnchor,
        Self::PhaseDrill,
        Self::HeartKey,
        Self::StarDrive,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DeepClaim => "Deep Claim",
            Self::DrillII => "Drill II",
            Self::PressureHull => "Pressure Hull",
            Self::BioFilter => "Bio-Filter",
            Self::LampII => "Lamp II",
            Self::SonarSuite => "Sonar Suite",
            Self::CryoHull => "Cryo Hull",
            Self::GravityAnchor => "Gravity Anchor",
            Self::PhaseDrill => "Phase Drill",
            Self::HeartKey => "Heart Key",
            Self::StarDrive => "Star Drive",
        }
    }
}

/// The boss or set piece guarding a gate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum DescentBoss {
    TheSinkhole,
    StarCore,
    MycelialMind,
    Leviathan,
    SunSerpent,
    TheGlitch,
    GatekeeperColossus,
    TheHeart,
}

impl DescentBoss {
    pub const ALL: [Self; 8] = [
        Self::TheSinkhole,
        Self::StarCore,
        Self::MycelialMind,
        Self::Leviathan,
        Self::SunSerpent,
        Self::TheGlitch,
        Self::GatekeeperColossus,
        Self::TheHeart,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TheSinkhole => "The Sinkhole",
            Self::StarCore => "The Star Core",
            Self::MycelialMind => "The Mycelial Mind",
            Self::Leviathan => "The Leviathan",
            Self::SunSerpent => "The Sun Serpent",
            Self::TheGlitch => "The Glitch",
            Self::GatekeeperColossus => "The Gatekeeper Colossus",
            Self::TheHeart => "The Heart",
        }
    }
}

/// One thing a gate asks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateRequirement {
    /// A drill of at least this tier.
    DrillTier(u8),
    /// A key item or rig system.
    Key(DescentKey),
    /// The gate's boss or set piece beaten.
    Boss(DescentBoss),
    /// The keys of every earlier Descent (the Seal Gate).
    EveryEarlierKey,
}

/// The gate at the end of a Descent: its set piece, what passing it needs and what it gives.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GateDef {
    pub boss: DescentBoss,
    pub requires: &'static [GateRequirement],
    pub rewards: &'static [DescentKey],
}

/// What a gate check knows about the crew trying to pass (the logistics and progression track
/// fills this from the world; see [`gate_passable`]).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GateProgress {
    /// The best drill tier among the crew.
    pub drill_tier: u8,
    /// Keys held, one bit per [`DescentKey`] (by declaration order).
    pub keys: u32,
    /// Bosses beaten, one bit per [`DescentBoss`].
    pub bosses: u32,
}

impl GateProgress {
    #[must_use]
    pub const fn has_key(&self, key: DescentKey) -> bool {
        self.keys & (1 << key as u32) != 0
    }

    #[must_use]
    pub const fn beat(&self, boss: DescentBoss) -> bool {
        self.bosses & (1 << boss as u32) != 0
    }
}

/// Whether `progress` meets every requirement of `gate`.
///
/// **Seam:** nothing in the simulation blocks the way down on this yet; while the gates are
/// placeholders every Descent is open (the logistics and progression track, F10, decides
/// where to enforce it, e.g. a sealed gate chamber floor).
#[must_use]
pub fn gate_passable(gate: &GateDef, progress: &GateProgress) -> bool {
    gate.requires.iter().all(|requirement| match requirement {
        GateRequirement::DrillTier(tier) => progress.drill_tier >= *tier,
        GateRequirement::Key(key) => progress.has_key(*key),
        GateRequirement::Boss(boss) => progress.beat(*boss),
        GateRequirement::EveryEarlierKey => Descent::ALL
            .iter()
            .filter_map(|descent| descent.gate())
            .take_while(|earlier| earlier.boss != gate.boss)
            .flat_map(|earlier| earlier.rewards.iter())
            .all(|key| progress.has_key(*key)),
    })
}

/// One stratum's definition (see the module docs).
#[derive(Clone, Copy, Debug)]
pub struct StratumDef {
    pub stratum: Stratum,
    pub descent: Descent,
    pub name: &'static str,
    /// The one rule that changes how the stratum plays (the design note for its Wave).
    pub signature: &'static str,
    /// The stratum's share of its Descent's rows, per mille (a Descent's shares sum to 1000).
    pub share_per_mille: u16,
    /// The classic band whose rules (hazard families, heat, drill gates, POI kinds) the stratum
    /// borrows while it is a placeholder.
    pub host_band: Band,
    /// Common rock (routes, the host of ore and pockets).
    pub soft: TileKind,
    /// Tougher rock (shelves and lumps).
    pub hard: TileKind,
    pub palette: StratumPalette,
    /// Ores and their depth curves through the stratum.
    pub ores: &'static [OreCurve],
    pub layer_resource: LayerResource,
    /// Creatures that roam the stratum, with spawn weights (existing kinds stand in).
    pub creatures: &'static [(EnemyKind, u32)],
    /// Hazard families and their weights.
    pub hazards: &'static [(HazardKind, u32)],
    pub field: FieldProfile,
    pub music: MusicCue,
    pub ambience: AmbienceCue,
    pub landmarks: &'static [LandmarkKind],
    /// The stratum's seed variants (a world picks one, [`crate::worldgen::descent::variant`]).
    pub variants: &'static [&'static str],
    /// The gate that ends the Descent (only on a Descent's last stratum).
    pub gate: Option<GateDef>,
    /// How much ore the stratum's veins and clusters carry, per mille of the classic density
    /// (Wave 8; 1000 by default). Read by generator V11 where the stratum's own ores apply
    /// ([`Stratum::uses_registry_rules`]) and by the descent probe.
    pub ore_density_permille: u16,
    /// The fluid bodies generator V11 places in the stratum's chunks
    /// ([`crate::fluid::fluid_pockets_in_chunk`], Wave 7 integration): sealed aquifers and
    /// cave pools of water, brine, acid or liquid metal. Empty in most strata.
    pub fluids: &'static [crate::fluid::FluidPocketRule],
}

impl StratumDef {
    /// The ore weights `along` per mille of the way through the stratum, in
    /// [`crate::worldgen::deposits::MINERALS`] order.
    #[must_use]
    pub fn ore_weights(&self, along: u16) -> [u32; 10] {
        let mut weights = [0; 10];
        for curve in self.ores {
            if let Some(slot) = crate::worldgen::deposits::MINERALS
                .iter()
                .position(|mineral| *mineral == curve.mineral)
            {
                weights[slot] += curve.weight_at(along);
            }
        }
        weights
    }

    /// The hazard weights in [`HazardKind::ALL`] order.
    #[must_use]
    pub fn hazard_weights(&self) -> [u32; 5] {
        let mut weights = [0; 5];
        for (kind, weight) in self.hazards {
            if let Some(slot) = HazardKind::ALL.iter().position(|each| each == kind) {
                weights[slot] += weight;
            }
        }
        weights
    }
}

#[cfg(test)]
mod tests;
