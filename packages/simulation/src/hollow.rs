//! Descent V, the Hollow World (Wave 9, expansion plan §14): the planet is hollow. A rig breaks
//! through a rock sky into a giant open cavern with its own inner sun and its own day.
//!
//! Pure data and math; the tick lives in `world/hollow.rs`, the creatures' behaviour and the
//! Sun Serpent's fight in `world/enemies/hollow.rs`, the generated places in
//! [`crate::worldgen::descent::hollow`]. Classic worlds never see any of it: every tile here is
//! generated only by V11 and every rule reads only those tiles or the V11 layout.
//!
//! | Stratum | Signature rule (introduced gently, then pushed) | Counter-play |
//! |---|---|---|
//! | Inner Sky | the **breakthrough**: an open cavern hundreds of rows tall under a rock sky, lit by the inner sun; **wind** bands push a flying rig ([`wind_at`]), stronger and gustier deeper | fly with the wind, land on floating stones to rest |
//! | Megafauna Jungle | giant beasts: friendly **crestbacks** the pilot can **ride** ([`MOUNT_MOVE_PCT`]), towering skynecks, **fangrunner** packs that hunt at night; **tar** floors grip a rig; the **Sunfolk** village | ride on foot, travel by day, drill clear of the tar |
//! | Upside-Down Isles | **local inverted gravity**: isles hang from the cavern's ceiling and under each one gravity points up ([`fields`]); flip the rig with its thrusters to land on them feet first | turn the rig before touching down ([`ROOF_LANDING_DEGREES`]) |
//! | The Sunforge | **overexposure**: in the inner sun's light the screen blooms and a rig heats ([`EXPOSURE_GAIN_PER_SECOND`]); the shade of floating stones cools it, so routes hop from shadow to shadow | read the light, rest in shade, cross by night |
//! | Gate | the **Sun Serpent** orbits the inner sun in its Sun Well: break its four sun scales, then strike its head; flares from the sun are shaded by the well's pillars | shoot the glowing scales, hide behind a pillar when the sun flares |
//!
//! The inner sun's day ([`inner_sun_at`]) is a pure function of world time, so every peer and
//! every tick rate sees the same dawn: herbivores graze by day and sleep at night, fangrunners
//! hunt at night, the Sunforge's light (and heat) falls at night.
//!
//! The Sun Serpent's reward is the **Gravity Anchor** ([`ANCHOR_MILLIS`], [`ANCHOR_COOLDOWN_MILLIS`]):
//! the rig's own down (turned by the player with the rotation thrusters) becomes gravity for a
//! while, so it walks on walls and ceilings.

pub mod fields;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    PlayerId,
    descent::Stratum,
    ids::SimulationTick,
    item::{BlockKind, ItemKind},
    terrain::{ArtifactKind, MineralKind, StrategicResourceKind},
};

// ---------------------------------------------------------------------------------------------
// Tiles.
// ---------------------------------------------------------------------------------------------

/// Kind-code range of [`crate::terrain::TileKind::Hollow`] tiles in
/// [`crate::terrain::Tile::code`] (`0x900 | HollowTile::code`).
pub const HOLLOW_TILE_CODE_BASE: u64 = 0x900;

/// A tile of the Hollow World's strata ([`crate::terrain::TileKind::Hollow`], V11 worlds only).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum HollowTile {
    // --- Inner Sky ---
    /// Pale, wind-scoured stone of the floating stones and the rock sky.
    SkyStone,
    /// Amber nodules in the floating stones (the Inner Sky's layer resource, Sky Amber).
    SkyAmber,
    /// A plank of the Fathers' winch platform at the Breach.
    BreachTimber,
    // --- Megafauna Jungle ---
    /// Dark jungle loam: the jungle's rock.
    Loam,
    /// Ferns over loam: the floor of a jungle hall.
    Fern,
    /// The trunk of a giant tree.
    Trunk,
    /// A giant tree's leafy crown.
    Canopy,
    /// A tar pit's tar: it grips a rig that drives on it.
    Tar,
    /// A megafauna bone in the loam (the jungle's layer resource, Giant Hide's bones).
    GiantBone,
    // --- Upside-Down Isles ---
    /// The rock of a hanging isle.
    IsleRock,
    /// Grass growing on an isle's underside (upward, toward its upside-down gravity).
    IsleGrass,
    /// Lodestone in the isles (the layer resource): faintly blue, magnetic.
    Lodestone,
    /// The stalk an isle hangs from.
    IsleStalk,
    // --- The Sunforge ---
    /// Bright, sun-baked rock.
    SunRock,
    /// Sun glass (the Sunforge's layer resource): glassy and glowing.
    SunGlass,
    /// Hot slag over the forge's lava.
    Slag,
    // --- The Sun Well (Descent V gate) ---
    /// The Sun Well's wall: nothing breaks it.
    WellWall,
    /// One of the Sun Well's pillars: shade from the inner sun's flares; nothing breaks it.
    ShadePillar,
    /// The inner sun's body in the middle of the Sun Well: nothing breaks it.
    InnerSun,
    // --- The Sunfolk ---
    /// A carved totem of the jungle tribes.
    Totem,
}

impl HollowTile {
    /// Every Hollow World tile, in code order.
    pub const ALL: [Self; 20] = [
        Self::SkyStone,
        Self::SkyAmber,
        Self::BreachTimber,
        Self::Loam,
        Self::Fern,
        Self::Trunk,
        Self::Canopy,
        Self::Tar,
        Self::GiantBone,
        Self::IsleRock,
        Self::IsleGrass,
        Self::Lodestone,
        Self::IsleStalk,
        Self::SunRock,
        Self::SunGlass,
        Self::Slag,
        Self::WellWall,
        Self::ShadePillar,
        Self::InnerSun,
        Self::Totem,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SkyStone => "sky stone",
            Self::SkyAmber => "sky amber",
            Self::BreachTimber => "winch timber",
            Self::Loam => "jungle loam",
            Self::Fern => "ferns",
            Self::Trunk => "giant trunk",
            Self::Canopy => "canopy",
            Self::Tar => "tar",
            Self::GiantBone => "giant bone",
            Self::IsleRock => "isle rock",
            Self::IsleGrass => "isle grass",
            Self::Lodestone => "lodestone",
            Self::IsleStalk => "isle stalk",
            Self::SunRock => "sun rock",
            Self::SunGlass => "sun glass",
            Self::Slag => "slag",
            Self::WellWall => "sun well wall",
            Self::ShadePillar => "shade pillar",
            Self::InnerSun => "the inner sun",
            Self::Totem => "tribal totem",
        }
    }

    /// Stable code (part of [`crate::terrain::Tile::code`], in the `0x900` family). Never
    /// renumber: saves and digests use it.
    #[must_use]
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

    /// Whether no drill, blast or pick ever breaks it (the Sun Well and the inner sun).
    #[must_use]
    pub const fn indestructible(self) -> bool {
        matches!(self, Self::WellWall | Self::ShadePillar | Self::InnerSun)
    }

    /// Drill strength needed to dig it (the Hollow World wants the deep drills).
    #[must_use]
    pub const fn hardness(self) -> u8 {
        if self.indestructible() {
            return crate::terrain::UNBREAKABLE_HARDNESS;
        }
        match self {
            Self::Fern | Self::Canopy | Self::Tar | Self::IsleGrass | Self::BreachTimber => 2,
            Self::Loam | Self::Trunk | Self::Totem | Self::GiantBone | Self::Slag => 3,
            Self::SkyStone | Self::IsleRock | Self::IsleStalk | Self::SkyAmber => 4,
            _ => 5,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "one row per toughness")]
    pub const fn durability(self) -> u8 {
        if self.indestructible() {
            return u8::MAX;
        }
        match self {
            Self::Fern | Self::Canopy | Self::IsleGrass => 3,
            Self::Tar | Self::BreachTimber => 4,
            Self::Loam | Self::Trunk | Self::Totem => 6,
            Self::GiantBone | Self::Slag => 7,
            Self::SkyStone | Self::IsleRock | Self::IsleStalk => 9,
            Self::SkyAmber | Self::Lodestone => 10,
            Self::SunRock => 11,
            Self::SunGlass => 12,
            _ => 10,
        }
    }

    /// The item a broken tile yields: the layer resources and blocks.
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "grouped by stratum")]
    pub const fn item(self) -> Option<ItemKind> {
        Some(match self {
            Self::SkyAmber => ItemKind::Material(StrategicResourceKind::AetherCrystal),
            Self::GiantBone => ItemKind::Artifact(ArtifactKind::Fossil),
            Self::Lodestone => ItemKind::Material(StrategicResourceKind::VoidGlass),
            Self::SunGlass => ItemKind::Material(StrategicResourceKind::StormGlass),
            Self::SkyStone | Self::IsleRock | Self::IsleStalk | Self::SunRock => {
                ItemKind::Block(BlockKind::HardRock)
            }
            Self::Loam => ItemKind::Block(BlockKind::Dirt),
            _ => return None,
        })
    }

    /// Light it gives off (0-255, the renderer's light map).
    #[must_use]
    pub const fn glow(self) -> u8 {
        match self {
            Self::InnerSun => 255,
            Self::SunGlass => 190,
            Self::SkyAmber | Self::Lodestone => 120,
            Self::Slag => 110,
            Self::Totem => 60,
            _ => 0,
        }
    }

    /// Whether it is plain rock to the deposit and hazard passes (ore sits in it like in stone).
    #[must_use]
    pub const fn plain_rock(self) -> bool {
        matches!(
            self,
            Self::SkyStone | Self::Loam | Self::IsleRock | Self::SunRock
        )
    }

    /// Whether sunlight passes it (shade comes only from solid rock).
    #[must_use]
    pub const fn lets_light_through(self) -> bool {
        matches!(self, Self::Canopy | Self::SunGlass)
    }
}

// ---------------------------------------------------------------------------------------------
// The inner sun's day.
// ---------------------------------------------------------------------------------------------

/// One full inner day: dawn, day, dusk, night.
pub const INNER_DAY_MILLIS: u64 = 16 * 60_000;
/// The dawn's share of the day (the sun swells).
pub const INNER_DAWN_MILLIS: u64 = 2 * 60_000;
/// Full day.
pub const INNER_NOON_MILLIS: u64 = 7 * 60_000;
/// The dusk (the sun dims).
pub const INNER_DUSK_MILLIS: u64 = 2 * 60_000;
/// The inner night's light, per mille of the day's (never pitch black: the sun glows on).
pub const INNER_NIGHT_LIGHT: u16 = 260;

/// The inner sun's phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum InnerPhase {
    Dawn,
    Day,
    Dusk,
    Night,
}

impl InnerPhase {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dawn => "inner dawn",
            Self::Day => "inner day",
            Self::Dusk => "inner dusk",
            Self::Night => "inner night",
        }
    }

    /// Whether the sun is up (dawn, day or dusk).
    #[must_use]
    pub const fn sun_up(self) -> bool {
        !matches!(self, Self::Night)
    }
}

/// The inner sun at an instant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InnerSun {
    pub phase: InnerPhase,
    /// How bright it shines, per mille ([`INNER_NIGHT_LIGHT`] at night, 1000 at noon).
    pub light: u16,
    /// How far through the whole day it is, per mille (presentation: the sun's swell).
    pub day: u16,
    /// The light's slant, per mille of a row per row (-400..400 over the day): shadows lean
    /// one way in the morning and the other in the afternoon.
    pub slant: i16,
}

/// The inner sun at `millis` of world time (the world clock's `world_millis`; a pure
/// function, so every peer and tick rate agrees).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "per mille values of bounded ranges"
)]
pub const fn inner_sun_at(millis: u64) -> InnerSun {
    let t = millis % INNER_DAY_MILLIS;
    let night = INNER_NIGHT_LIGHT as u64;
    let day = (t * 1_000 / INNER_DAY_MILLIS) as u16;
    let lit = INNER_DAWN_MILLIS + INNER_NOON_MILLIS + INNER_DUSK_MILLIS;
    let slant = if t < lit {
        ((t * 800 / lit) as i64 - 400) as i16
    } else {
        0
    };
    if t < INNER_DAWN_MILLIS {
        let light = night + (1_000 - night) * t / INNER_DAWN_MILLIS;
        InnerSun {
            phase: InnerPhase::Dawn,
            light: light as u16,
            day,
            slant,
        }
    } else if t < INNER_DAWN_MILLIS + INNER_NOON_MILLIS {
        InnerSun {
            phase: InnerPhase::Day,
            light: 1_000,
            day,
            slant,
        }
    } else if t < lit {
        let into = t - INNER_DAWN_MILLIS - INNER_NOON_MILLIS;
        let light = 1_000 - (1_000 - night) * into / INNER_DUSK_MILLIS;
        InnerSun {
            phase: InnerPhase::Dusk,
            light: light as u16,
            day,
            slant,
        }
    } else {
        InnerSun {
            phase: InnerPhase::Night,
            light: INNER_NIGHT_LIGHT,
            day,
            slant,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The Inner Sky's wind.
// ---------------------------------------------------------------------------------------------

/// Rows of one wind band (bands alternate direction by seed).
pub const WIND_BAND_ROWS: i32 = 26;
/// One gust cycle of a band (it builds, holds and lulls).
pub const WIND_GUST_MILLIS: u64 = 14_000;
/// A band's calm speed at the Inner Sky's top and the extra its gusts add at its bottom (px/s).
pub const WIND_BASE_SPEED: f32 = 40.0;
pub const WIND_GUST_SPEED: f32 = 150.0;
/// How quickly an airborne rig's sideways speed eases toward the wind (per second).
pub const WIND_GRIP: f32 = 0.9;

const SALT_WIND: u64 = 0x5744_0901;

/// The wind across row `row` of the Inner Sky at `millis`, px/s (positive blows right).
///
/// `along` is per mille of how deep in the stratum the row is: calm near the rock sky, gusty
/// deeper (introduced gently, then pushed).
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "small per mille values")]
pub fn wind_at(seed: u64, row: i32, along: u16, millis: u64) -> f32 {
    let band = row.div_euclid(WIND_BAND_ROWS);
    let roll = crate::worldgen::noise::hash2(seed, SALT_WIND, band, 0);
    let direction = if roll.is_multiple_of(2) { 1.0 } else { -1.0 };
    // Each band gusts on its own phase: build, hold, lull (a triangle-ish wave).
    let phase = (millis + (roll >> 8) % WIND_GUST_MILLIS) % WIND_GUST_MILLIS;
    let half = WIND_GUST_MILLIS / 2;
    let gust = if phase < half {
        phase as f32 / half as f32
    } else {
        (WIND_GUST_MILLIS - phase) as f32 / half as f32
    };
    let depth = f32::from(along.min(1_000)) / 1_000.0;
    // A calm sliver between bands so a rig can slip from one stream to the next.
    let into = row.rem_euclid(WIND_BAND_ROWS);
    let edge = into.min(WIND_BAND_ROWS - 1 - into);
    let calm = if edge < 2 { 0.35 } else { 1.0 };
    direction * calm * (WIND_GUST_SPEED * depth).mul_add(gust, WIND_BASE_SPEED * depth.max(0.25))
}

// ---------------------------------------------------------------------------------------------
// The Sunforge's light.
// ---------------------------------------------------------------------------------------------

/// Rows a point scans toward the inner sun (below) for shade.
pub const SHADE_SCAN_ROWS: i32 = 36;
/// Exposure a rig gains a second in full noon sunlight at the Sunforge's top (per mille of the
/// meter; doubled by its bottom).
pub const EXPOSURE_GAIN_PER_SECOND: u32 = 55;
/// Exposure a rig sheds a second in shade (or out of the light).
pub const EXPOSURE_COOL_PER_SECOND: u32 = 140;
/// Above this exposure (per mille) a rig's hull scorches.
pub const EXPOSURE_SCORCH_AT: u16 = 1_000;
/// Hull a second a fully exposed rig loses (scaled by the depth's creature scale: about 1 % of
/// a Sunforge-tier hull a second, so a long crossing in the light really hurts).
pub const EXPOSURE_DAMAGE_PER_SECOND: f32 = 4.5;
/// How long an exposure step is (world time; tick-rate safe).
pub const EXPOSURE_STEP_MILLIS: u64 = 250;

// ---------------------------------------------------------------------------------------------
// The Upside-Down Isles.
// ---------------------------------------------------------------------------------------------

/// A rig touching down on an isle leaning more than this from its gravity's level lands on
/// its roof: it takes [`ROOF_LANDING_DAMAGE`] and must be turned before it can drive off.
pub const ROOF_LANDING_DEGREES: f32 = 75.0;
/// Hull a roof landing costs (scaled by the depth's creature scale).
pub const ROOF_LANDING_DAMAGE: f32 = 6.0;

// ---------------------------------------------------------------------------------------------
// The jungle.
// ---------------------------------------------------------------------------------------------

/// A pilot within this many pixels of a crestback's body may climb onto it.
pub const MOUNT_REACH: f32 = 30.0;
/// A riding pilot's walk and run speed, percent more.
pub const MOUNT_MOVE_PCT: i16 = 85;
/// A riding pilot's jump, percent more.
pub const MOUNT_JUMP_PCT: i16 = 35;
/// How high the pilot sits above the mount's feet, pixels (presentation).
pub const MOUNT_SADDLE_PIXELS: f32 = 28.0;
/// Tar's grip: a grounded rig's sideways speed decays at this rate a second (`exp(-k t)`:
/// about a twelfth is left after a second of driving on tar).
pub const TAR_GRIP_PER_SECOND: f32 = 2.5;

/// The jungle tribes' village.
pub const SUNFOLK_NAME: &str = "Sunfolk Kraal";

// ---------------------------------------------------------------------------------------------
// The Gravity Anchor.
// ---------------------------------------------------------------------------------------------

/// How long the anchor holds.
pub const ANCHOR_MILLIS: u64 = 12_000;
/// How long the anchor needs between holds (from the moment it lets go).
pub const ANCHOR_COOLDOWN_MILLIS: u64 = 8_000;

/// Which way the anchor pulls, in quarter turns clockwise from straight down (0 down, 1 left,
/// 2 up, 3 right on screen): the rig's own down, snapped to the nearest quarter when engaged.
#[must_use]
pub fn anchor_quarter(orientation: f32) -> u8 {
    let quarter = std::f32::consts::FRAC_PI_2;
    let turns = (orientation / quarter).round();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a quarter count, wrapped to 0..4"
    )]
    let wrapped = (turns as i32).rem_euclid(4) as u8;
    wrapped
}

/// The gravity direction (unit vector, +y down) of anchor quarter `quarter`.
#[must_use]
pub const fn anchor_gravity(quarter: u8) -> (f32, f32) {
    match quarter % 4 {
        1 => (-1.0, 0.0),
        2 => (0.0, -1.0),
        3 => (1.0, 0.0),
        _ => (0.0, 1.0),
    }
}

/// A rig's Gravity Anchor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Anchor {
    /// The quarter it pulls toward while it holds ([`anchor_quarter`]).
    #[serde(default)]
    pub quarter: u8,
    /// It holds until this tick (`None`: not holding).
    #[serde(default)]
    pub until: Option<SimulationTick>,
    /// It may hold again from this tick.
    #[serde(default)]
    pub ready: SimulationTick,
}

// ---------------------------------------------------------------------------------------------
// The Sun Serpent.
// ---------------------------------------------------------------------------------------------

/// Body coils behind the serpent's head.
pub const SERPENT_COILS: usize = 12;
/// The coils that carry sun scales (they glow; break them to expose the head).
pub const SERPENT_SCALES: [usize; 4] = [2, 5, 8, 11];
/// Pixels between coils along the serpent's trail.
pub const SERPENT_COIL_SPACING: f32 = 26.0;
/// The serpent's orbit radius round the inner sun, tiles (it breathes between the two).
pub const SERPENT_ORBIT_TILES: (f32, f32) = (12.0, 18.0);
/// How long one orbit breath takes (in to out and back).
pub const SERPENT_BREATH_MILLIS: u64 = 16_000;
/// Its orbit speed, radians a second (it speeds up each cycle).
pub const SERPENT_ORBIT_SPEED: f32 = 0.42;
/// Time between lunges, the lunge's tell and its length.
pub const SERPENT_LUNGE_MILLIS: u64 = 11_000;
pub const SERPENT_TELL_MILLIS: u64 = 1_300;
pub const SERPENT_DASH_MILLIS: u64 = 1_600;
/// Time between solar flares, the flare's warning and how long it burns.
pub const FLARE_EVERY_MILLIS: u64 = 19_000;
pub const FLARE_WARNING_MILLIS: u64 = 2_800;
pub const FLARE_BURN_MILLIS: u64 = 2_200;
/// Hull a second a flare burns a rig in the inner sun's line of sight (scaled by tier).
pub const FLARE_DAMAGE_PER_SECOND: f32 = 2.6;
/// How long the head stays exposed (fallen out of orbit) once every scale is broken.
pub const SERPENT_EXPOSED_MILLIS: u64 = 15_000;
/// With no rig in the well this long, the serpent sinks into the sun and heals.
pub const SERPENT_SLEEP_MILLIS: u64 = 20_000;
/// Radius of the Sun Well, tiles.
pub const WELL_RADIUS: i32 = 25;
/// Radius of the inner sun's body in the well, tiles.
pub const SUN_RADIUS: i32 = 4;
/// Distance of the shade pillars from the well's centre, tiles.
pub const PILLAR_DISTANCE: i32 = 16;

/// What the serpent is doing.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub enum SerpentPhase {
    /// Circling the inner sun.
    #[default]
    Orbit,
    /// The head glows: a lunge is coming at the marked point.
    Tell,
    /// Dashing at the marked point (then back to its orbit).
    Lunge,
    /// Every scale broken: it has fallen out of its orbit, its head bare.
    Exposed,
}

/// The Sun Serpent's fight.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct SerpentState {
    /// Beaten: the gate is open and the Gravity Anchor handed out.
    #[serde(default)]
    pub beaten: bool,
    /// The head's entity while it is awake.
    #[serde(default)]
    pub head: Option<u64>,
    /// Its coils, head to tail ([`SERPENT_COILS`]).
    #[serde(default)]
    pub coils: Vec<u64>,
    /// Which scales are still whole ([`SERPENT_SCALES`] order).
    #[serde(default)]
    pub scales: [bool; 4],
    #[serde(default)]
    pub phase: SerpentPhase,
    /// When the phase ends.
    #[serde(default)]
    pub phase_until: SimulationTick,
    /// The orbit's angle at [`Self::orbit_millis`], 1/65536 turns: the head's path is a pure
    /// function of world time from there.
    #[serde(default)]
    pub angle: u32,
    /// World time the orbit is measured from.
    #[serde(default)]
    pub orbit_millis: u64,
    /// The lunge's target, local pixels (set at the tell).
    #[serde(default)]
    pub mark: (i32, i32),
    /// World time the last lunge started.
    #[serde(default)]
    pub lunge_millis: Option<u64>,
    /// World time it fell from its orbit (every scale broken), while it lies exposed.
    #[serde(default)]
    pub exposed_millis: Option<u64>,
    /// How many times its scales have all broken (each time it moves faster).
    #[serde(default)]
    pub cycles: u8,
    #[serde(default)]
    pub next_lunge: SimulationTick,
    #[serde(default)]
    pub next_flare: SimulationTick,
    /// The flare's warning started at, and it burns from, these ticks (`None`: no flare).
    #[serde(default)]
    pub flare_warned: Option<SimulationTick>,
    #[serde(default)]
    pub flare_burns: Option<SimulationTick>,
    /// The last tick a rig was in the well.
    #[serde(default)]
    pub last_visited: SimulationTick,
}

impl SerpentState {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the serpent is awake.
    #[must_use]
    pub const fn awake(&self) -> bool {
        self.head.is_some()
    }

    /// Whether any sun scale is still whole (the head is armoured).
    #[must_use]
    pub fn shielded(&self) -> bool {
        self.phase != SerpentPhase::Exposed
    }

    /// Whether the inner sun is burning a flare now.
    #[must_use]
    pub const fn flaring(&self) -> bool {
        self.flare_burns.is_some()
    }

    /// Moves the lunge's mark (local pixels) with a frame move.
    pub fn reframe_mark(&mut self, shift: crate::position::FrameShift) {
        if shift.is_zero() {
            return;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "local pixels"
        )]
        {
            self.mark = (
                (self.mark.0 as f32 - shift.dx).round() as i32,
                (self.mark.1 as f32 - shift.dy).round() as i32,
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Events, commands and refusals.
// ---------------------------------------------------------------------------------------------

/// Something of the Hollow World happened (netcode codes `540 + code`).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum HollowEvent {
    /// A rig broke through the rock sky into the Hollow World for the first time (the reveal).
    Breakthrough,
    /// The inner sun rose (dawn began).
    InnerDawn,
    /// The inner sun set (night began).
    InnerNight,
    /// A pilot climbed onto a crestback.
    Mounted,
    /// A pilot got down (or was thrown off).
    Dismounted,
    /// A rig touched down feet first on an upside-down isle.
    LandedUpsideDown,
    /// A rig landed on its roof.
    RoofLanding,
    /// A rig's exposure reached the scorch line.
    Overexposed,
    /// The Gravity Anchor took hold.
    AnchorEngaged,
    /// The Gravity Anchor let go.
    AnchorReleased,
    /// The Sun Serpent woke.
    SerpentAwakened,
    /// A sun scale broke.
    ScaleBroken,
    /// Every scale broken: the serpent fell from its orbit.
    SerpentExposed,
    /// Its scales blazed back.
    ScalesReignited,
    /// The serpent's head glows: a lunge is coming.
    SerpentTell,
    /// The inner sun is about to flare: get into a pillar's shade.
    FlareWarning,
    /// The Sun Serpent fell (and the crew's rigs got the Gravity Anchor).
    SerpentDefeated,
    /// A gust bladder burst into a gust.
    GustBurst,
    /// A lode mite clung to a rig.
    MiteLatched,
    /// A flarefly burst into heat on a rig.
    FlareflyBurst,
}

impl HollowEvent {
    /// Offset of the event's netcode code from 540.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

/// A Hollow World command (sent as `LogisticsCommand::Hollow`).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum HollowAction {
    /// Engage the Gravity Anchor: the rig's own down becomes gravity for a while.
    EngageAnchor,
    /// Let the anchor go early.
    ReleaseAnchor,
    /// The anchor button: engage when ready, let go while holding.
    ToggleAnchor,
}

/// Why a Hollow World command is refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HollowRefusal {
    /// The rig has no Gravity Anchor (the Sun Serpent's reward).
    NoAnchor,
    /// The anchor is cooling down.
    Recharging,
    /// The anchor is not holding.
    NotHolding,
    /// The anchor works from the rig, not on foot.
    OnFoot,
}

impl HollowRefusal {
    /// The refusal in words.
    #[must_use]
    pub fn message(self) -> String {
        match self {
            Self::NoAnchor => "No Gravity Anchor aboard: the Sun Serpent guards it.",
            Self::Recharging => "The Gravity Anchor is recharging.",
            Self::NotHolding => "The Gravity Anchor is not holding.",
            Self::OnFoot => "The Gravity Anchor works from the rig.",
        }
        .to_owned()
    }
}

// ---------------------------------------------------------------------------------------------
// The world state.
// ---------------------------------------------------------------------------------------------

/// The Hollow World's shared state (saved, replicated as the appended `Hollow` section, hashed
/// once anything happened).
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Hollow {
    /// Players who have broken through into the Hollow World and the tick they did (the reveal
    /// plays once each; the view eases back in from it).
    #[serde(default)]
    pub revealed: BTreeMap<PlayerId, SimulationTick>,
    /// Each rig's exposure to the inner sun, per mille ([`EXPOSURE_SCORCH_AT`] scorches).
    #[serde(default)]
    pub exposure: BTreeMap<PlayerId, u16>,
    /// Each rig's Gravity Anchor.
    #[serde(default)]
    pub anchors: BTreeMap<PlayerId, Anchor>,
    /// The crestback each riding pilot sits on (entity ids).
    #[serde(default)]
    pub mounts: BTreeMap<PlayerId, u64>,
    /// Rigs that were resting on ground last tick, and whether they lay on their roof (the
    /// landing check).
    #[serde(default)]
    pub grounded: BTreeMap<PlayerId, bool>,
    /// Feet-first isle landings so far (the first one is cheered).
    #[serde(default)]
    pub isle_landings: u32,
    /// The inner sun's phase last tick (dawn and night are announced on the change).
    #[serde(default)]
    pub sun_phase: Option<InnerPhase>,
    #[serde(default)]
    pub serpent: SerpentState,
}

impl Hollow {
    /// Whether nothing has happened in the Hollow World (left out of the state hash then, so
    /// every older world's hash is unchanged).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.revealed.is_empty()
            && self.exposure.is_empty()
            && self.anchors.is_empty()
            && self.mounts.is_empty()
            && self.grounded.is_empty()
            && self.isle_landings == 0
            && self.sun_phase.is_none()
            && self.serpent.is_default()
    }

    /// `player`'s exposure, per mille.
    #[must_use]
    pub fn exposure_of(&self, player: PlayerId) -> u16 {
        self.exposure.get(&player).copied().unwrap_or(0)
    }

    /// Whether `player`'s Gravity Anchor holds at `now`.
    #[must_use]
    pub fn anchored(&self, player: PlayerId, now: SimulationTick) -> Option<u8> {
        let anchor = self.anchors.get(&player)?;
        anchor
            .until
            .is_some_and(|until| now < until)
            .then_some(anchor.quarter)
    }

    /// Forgets a leaving player.
    pub fn forget(&mut self, player: PlayerId) {
        self.revealed.remove(&player);
        self.exposure.remove(&player);
        self.anchors.remove(&player);
        self.mounts.remove(&player);
        self.grounded.remove(&player);
    }
}

// ---------------------------------------------------------------------------------------------
// The descent probe's view.
// ---------------------------------------------------------------------------------------------

/// The authored mechanics and set pieces a Hollow World stratum adds beyond its registry entry
/// (the descent probe counts them as content).
#[must_use]
pub const fn authored_content(stratum: Stratum) -> (u8, u8) {
    match stratum {
        // Inner Sky: wind, the reveal, the inner day and night, gust bladders' shoves; the
        // Breach and the sky-stone archipelago. Isles: inverted gravity, roof landings,
        // clinging mites, the free flip; the isles and the isle sea. Sunforge: exposure, shade,
        // sun-powered drakes, flareflies' heat; the forge sky and the Sun Well.
        Stratum::InnerSky | Stratum::UpsideDownIsles | Stratum::TheSunforge => (4, 2),
        // Riding, tar, the packs' night hunts; the Sunfolk village, giant trees, jungle halls.
        Stratum::MegafaunaJungle => (3, 3),
        _ => (0, 0),
    }
}

/// Share of `stratum` whose gravity the isles turn, per mille (Wave 9 integration: the descent
/// probe's turned-gravity share, which the registry's fields leave to `hollow::fields`).
#[must_use]
pub const fn turned_gravity_share_permille(stratum: Stratum) -> u16 {
    match stratum {
        Stratum::UpsideDownIsles => 350,
        _ => 0,
    }
}

/// The named new things of a Hollow World stratum (the descent probe's novelty markers beyond
/// the registry's).
#[must_use]
pub const fn authored_markers(stratum: Stratum) -> &'static [&'static str] {
    match stratum {
        Stratum::InnerSky => &[
            "the breakthrough: the inner sun",
            "wind bands",
            "the inner day and night",
            "the Fathers' Breach",
        ],
        Stratum::MegafaunaJungle => &[
            "riding a crestback",
            "tar floors",
            "the Sunfolk village",
            "giant trees",
        ],
        Stratum::UpsideDownIsles => &["inverted gravity under the isles", "landing upside down"],
        Stratum::TheSunforge => &["overexposure and shade-hopping", "the Sun Well"],
        _ => &[],
    }
}

/// The minerals the Hollow World's own tiles stand in for in reports (the layer resources).
#[must_use]
pub const fn layer_mineral(stratum: Stratum) -> Option<MineralKind> {
    match stratum {
        Stratum::InnerSky => Some(MineralKind::Diamond),
        Stratum::MegafaunaJungle => Some(MineralKind::Emerald),
        Stratum::UpsideDownIsles => Some(MineralKind::Mythril),
        Stratum::TheSunforge => Some(MineralKind::Ruby),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn tiles_round_trip_their_codes() {
        for (index, tile) in HollowTile::ALL.iter().enumerate() {
            assert_eq!(tile.code(), index as u64);
            assert_eq!(HollowTile::from_code(tile.code()), Some(*tile));
        }
        assert_eq!(HollowTile::from_code(HollowTile::ALL.len() as u64), None);
    }

    #[test]
    fn the_inner_day_turns_on_world_time_alone() {
        assert_eq!(inner_sun_at(0).phase, InnerPhase::Dawn);
        assert_eq!(inner_sun_at(0).light, INNER_NIGHT_LIGHT);
        let noon = inner_sun_at(INNER_DAWN_MILLIS + 1_000);
        assert_eq!(noon.phase, InnerPhase::Day);
        assert_eq!(noon.light, 1_000);
        let dusk = inner_sun_at(INNER_DAWN_MILLIS + INNER_NOON_MILLIS + INNER_DUSK_MILLIS / 2);
        assert_eq!(dusk.phase, InnerPhase::Dusk);
        assert!(dusk.light > INNER_NIGHT_LIGHT && dusk.light < 1_000);
        let night = inner_sun_at(INNER_DAY_MILLIS - 1);
        assert_eq!(night.phase, InnerPhase::Night);
        assert_eq!(night.light, INNER_NIGHT_LIGHT);
        // Periodic, and monotone through the dawn.
        assert_eq!(inner_sun_at(INNER_DAY_MILLIS * 7 + 5), inner_sun_at(5));
        let mut last = 0;
        for step in 0..=20 {
            let light = inner_sun_at(INNER_DAWN_MILLIS * step / 20).light;
            assert!(light >= last);
            last = light;
        }
    }

    #[test]
    fn wind_bands_blow_both_ways_and_gust_harder_deeper() {
        let seed = 7;
        let directions: BTreeSet<bool> = (0..20)
            .map(|band| wind_at(seed, band * WIND_BAND_ROWS + 10, 500, 0) > 0.0)
            .collect();
        assert_eq!(directions.len(), 2, "bands blow both ways");
        let shallow = (0..28)
            .map(|step| wind_at(seed, 10, 50, step * 500).abs())
            .fold(0.0_f32, f32::max);
        let deep = (0..28)
            .map(|step| wind_at(seed, 10, 950, step * 500).abs())
            .fold(0.0_f32, f32::max);
        assert!(deep > shallow * 2.0, "{deep} vs {shallow}");
        assert!(deep <= WIND_BASE_SPEED + WIND_GUST_SPEED + 1.0);
    }

    #[test]
    fn the_anchor_snaps_the_rig_down_to_a_quarter() {
        use std::f32::consts::{FRAC_PI_2, PI};
        assert_eq!(anchor_quarter(0.1), 0);
        assert_eq!(anchor_quarter(FRAC_PI_2 - 0.2), 1);
        assert_eq!(anchor_quarter(PI), 2);
        assert_eq!(anchor_quarter(-FRAC_PI_2), 3);
        assert_eq!(anchor_gravity(2), (0.0, -1.0));
        assert_eq!(anchor_gravity(1), (-1.0, 0.0));
    }
}
