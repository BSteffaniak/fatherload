//! The space data model: ids, sectors, systems, stars, bodies and the interstellar things.
//!
//! Everything is integers (tiles, per mille, milli-g, seconds, °C) so a sector's bytes are
//! the same on every platform ([`super::tests`] pins them). Positions are [`SpaceTile`]s.

use serde::{Deserialize, Serialize};

use super::{SpaceSector, SpaceTile};
use crate::star_chart::StarClass;

// ---------------------------------------------------------------------------------------------
// Ids.
// ---------------------------------------------------------------------------------------------

/// A star system: its sector and its index in the sector (0-2).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct SystemRef {
    pub sector: SpaceSector,
    pub system: u8,
}

/// A body you can land on, in a generated system: `(sector, system index, body index)`.
///
/// TODO-SEAM (w11-regions): this is the payload of `place::BodyId`'s space variant; the
/// integrator unifies the two. The home world is `(HOME_SECTOR, 0, 0)` and the Moon
/// `(HOME_SECTOR, 0, 1)` here ([`super::home::HOME_WORLD_BODY`], [`super::home::MOON_BODY`]),
/// which `BodyId::Home` and `BodyId::Moon` map to.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct SpaceBodyId {
    pub sector: SpaceSector,
    pub system: u8,
    pub body: u8,
}

impl SpaceBodyId {
    /// The system it circles.
    #[must_use]
    pub const fn system_ref(self) -> SystemRef {
        SystemRef {
            sector: self.sector,
            system: self.system,
        }
    }
}

/// One asteroid of a belt: its belt, the belt's angular segment and its index there.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct AsteroidId {
    pub sector: SpaceSector,
    pub system: u8,
    pub belt: u8,
    pub segment: u32,
    pub index: u8,
}

/// A station (Wave 12 makes them hubs): its system and its index there.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct StationId {
    pub sector: SpaceSector,
    pub system: u8,
    pub index: u8,
}

/// A wormhole pair: the two regions its mouths lie in, the lesser first.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct WormholeId {
    pub a: (i32, i32),
    pub b: (i32, i32),
}

// ---------------------------------------------------------------------------------------------
// Tags and physical parameters.
// ---------------------------------------------------------------------------------------------

/// A body's theme (§16). Its ground content comes in Waves 12-13; space stores the tag and
/// the physical parameters.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum Theme {
    Jungle,
    OceanArchipelago,
    Ice,
    DesertDunes,
    Volcanic,
    Crystal,
    Fungal,
    MachineWorld,
    /// A gas giant with floating cities.
    GasGiant,
    /// Eternal day on one side, night on the other.
    TidallyLocked,
    RingWorld,
    /// A dead world with ruins.
    DeadWorld,
    /// Bare cratered rock: small moons and crater rocks (one secret each).
    #[default]
    Barren,
    /// The home world (its own ground, unchanged).
    Home,
    /// The Moon (its own ground, unchanged).
    Moon,
}

impl Theme {
    /// The twelve planet themes of §16.
    pub const PLANET: [Self; 12] = [
        Self::Jungle,
        Self::OceanArchipelago,
        Self::Ice,
        Self::DesertDunes,
        Self::Volcanic,
        Self::Crystal,
        Self::Fungal,
        Self::MachineWorld,
        Self::GasGiant,
        Self::TidallyLocked,
        Self::RingWorld,
        Self::DeadWorld,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Jungle => "jungle",
            Self::OceanArchipelago => "ocean archipelago",
            Self::Ice => "ice",
            Self::DesertDunes => "desert dunes",
            Self::Volcanic => "volcanic",
            Self::Crystal => "crystal",
            Self::Fungal => "fungal",
            Self::MachineWorld => "machine world",
            Self::GasGiant => "gas giant",
            Self::TidallyLocked => "tidally locked",
            Self::RingWorld => "ring world",
            Self::DeadWorld => "dead world",
            Self::Barren => "barren rock",
            Self::Home => "home",
            Self::Moon => "the Moon",
        }
    }
}

/// What a body is.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum BodyKind {
    /// Circles the star; 1,500-10,000 tiles.
    #[default]
    Planet,
    /// Circles a planet; 400-1,000 tiles.
    Moon,
    /// A crater rock circling the star; 400-1,000 tiles.
    CraterRock,
    /// The home world (its ground is the existing world).
    HomeWorld,
    /// The Moon (its ground is the existing Moon).
    HomeMoon,
}

impl BodyKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Planet => "planet",
            Self::Moon => "moon",
            Self::CraterRock => "crater rock",
            Self::HomeWorld => "home world",
            Self::HomeMoon => "the Moon",
        }
    }
}

/// A body's air.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum Atmosphere {
    #[default]
    Vacuum,
    Thin,
    Breathable,
    Thick,
    Toxic,
    /// Fungal spore air.
    Spores,
    /// Hot enough to cook a hull.
    Scalding,
}

impl Atmosphere {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Vacuum => "vacuum",
            Self::Thin => "thin",
            Self::Breathable => "breathable",
            Self::Thick => "thick",
            Self::Toxic => "toxic",
            Self::Spores => "spores",
            Self::Scalding => "scalding",
        }
    }

    /// Whether a pilot breathes it without life support.
    #[must_use]
    pub const fn breathable(self) -> bool {
        matches!(self, Self::Breathable)
    }

    /// Whether it glows from space (the renderer's atmosphere rim).
    #[must_use]
    pub const fn visible(self) -> bool {
        !matches!(self, Self::Vacuum)
    }
}

/// A ring around a body (ring worlds, some gas giants).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Ring {
    /// Inner and outer radius, tiles from the body's centre.
    pub inner: u32,
    pub outer: u32,
}

/// A landable body (planet, moon, crater rock, the home world, the Moon).
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct SpaceBody {
    /// Index in its system's [`StarSystem::bodies`].
    pub index: u8,
    pub kind: BodyKind,
    /// The planet a moon circles (an index into the same list).
    pub parent: Option<u8>,
    pub theme: Theme,
    pub centre: SpaceTile,
    /// Radius to the surface, tiles (at least [`super::MIN_LANDABLE_RADIUS`]).
    pub radius: u32,
    /// Orbit radius around its star (system centre) or parent, tiles.
    pub orbit_radius: u32,
    /// Orbit angle ([`super::TURN`] per turn; static: orbits do not move).
    pub orbit_angle: u32,
    /// How far its own things reach (moons, rings, the gap kept clear), tiles from its centre.
    pub extent: u32,
    /// Surface gravity, milli-g (1,000 = the home world's).
    pub gravity_milli_g: u16,
    /// Day length, seconds (0 = tidally locked: eternal day and night sides).
    pub day_seconds: u32,
    pub atmosphere: Atmosphere,
    /// Mean surface temperature, °C.
    pub temperature_c: i16,
    /// Columns around the surface ring (`≈ 2πR`, a multiple of 32: one terrain chunk). The
    /// polar track halves them toward the core.
    pub surface_columns: u32,
    /// Rows from the surface to the centre (= the radius).
    pub rows: u32,
    /// Depth of the authored strata (§16: 800-2,500 rows); below them, plain rock to the core.
    pub strata_rows: u16,
    pub ring: Option<Ring>,
    /// Seed for its ground generator (Wave 12; folds in the world seed and the id).
    pub ground_seed: u64,
}

impl SpaceBody {
    /// Whether it is the home world or the Moon (whose ground already exists).
    #[must_use]
    pub const fn is_home(&self) -> bool {
        matches!(self.kind, BodyKind::HomeWorld | BodyKind::HomeMoon)
    }

    /// Surface gravity in g.
    #[must_use]
    pub fn gravity_g(&self) -> f32 {
        f32::from(self.gravity_milli_g) / 1_000.0
    }

    /// Whether it is tidally locked.
    #[must_use]
    pub const fn tidally_locked(&self) -> bool {
        self.day_seconds == 0
    }
}

/// One star.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Star {
    pub centre: SpaceTile,
    /// Radius, tiles (can't land).
    pub radius: u32,
    /// Within this the heat burns any hull, tiles from the centre.
    pub heat_radius: u32,
    /// Surface temperature, kelvin (drives the planets' climate).
    pub temperature_k: u32,
    /// Light, per mille of the home sun's at the same distance.
    pub light: u16,
    /// Seconds between flares (0 = never flares).
    pub flare_period_s: u32,
    /// Phase of the flares, seconds (a flare starts when `time % period == phase`).
    pub flare_phase_s: u32,
    /// Surface gravity, milli-g.
    pub gravity_milli_g: u32,
}

/// An asteroid belt: an annulus around the system centre. Its asteroids are generated per
/// angular segment on demand ([`super::asteroids_in_segment`]).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct AsteroidBelt {
    pub index: u8,
    pub inner: u32,
    pub outer: u32,
    /// Asteroids per segment, 1-24.
    pub density: u8,
    /// The ore its asteroids favour.
    pub ore: AsteroidOre,
    /// Angular segments around the ring.
    pub segments: u32,
}

/// What an asteroid gives when mined from flight (Wave 12 maps these to items).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum AsteroidOre {
    #[default]
    Iron,
    Nickel,
    Ice,
    Cobalt,
    Platinum,
    Iridium,
    /// A rare dark glass found only out here.
    Voidglass,
}

impl AsteroidOre {
    pub const ALL: [Self; 7] = [
        Self::Iron,
        Self::Nickel,
        Self::Ice,
        Self::Cobalt,
        Self::Platinum,
        Self::Iridium,
        Self::Voidglass,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Iron => "iron",
            Self::Nickel => "nickel",
            Self::Ice => "ice",
            Self::Cobalt => "cobalt",
            Self::Platinum => "platinum",
            Self::Iridium => "iridium",
            Self::Voidglass => "voidglass",
        }
    }
}

/// One asteroid.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Asteroid {
    pub id: AsteroidId,
    pub centre: SpaceTile,
    /// Radius, tiles (below [`super::MIN_LANDABLE_RADIUS`]).
    pub radius: u16,
    pub ore: AsteroidOre,
    /// Units it yields when mined out.
    pub yield_units: u16,
}

/// A comet: an icy nucleus with a tail pointing away from its star.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Comet {
    pub centre: SpaceTile,
    pub nucleus_radius: u16,
    /// The tail's far end (straight away from the star).
    pub tail_end: SpaceTile,
    /// The tail's width at its end, tiles.
    pub tail_width: u32,
}

/// What a derelict was.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum DerelictKind {
    /// A gutted cargo hauler.
    #[default]
    Freighter,
    /// A tumbling survey probe.
    Probe,
    /// A builders' hull, older than anything.
    BuilderHulk,
    /// A field of wreckage from some old fight.
    Wreckfield,
}

impl DerelictKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Freighter => "derelict freighter",
            Self::Probe => "drifting probe",
            Self::BuilderHulk => "builders' hulk",
            Self::Wreckfield => "wreck field",
        }
    }
}

/// A derelict.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Derelict {
    pub kind: DerelictKind,
    pub centre: SpaceTile,
    /// Its size, tiles across.
    pub size: u16,
    /// Seeds its loot and layout (Wave 12).
    pub seed: u64,
}

/// What a station is for (placeholders: Wave 12 makes them hubs).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum StationKind {
    #[default]
    TradePost,
    Shipyard,
    Outpost,
    Relay,
}

impl StationKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TradePost => "trade post",
            Self::Shipyard => "shipyard",
            Self::Outpost => "outpost",
            Self::Relay => "relay",
        }
    }
}

/// A station, parked at a planet's trailing or leading point (60° along its orbit).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Station {
    pub id: StationId,
    pub kind: StationKind,
    pub centre: SpaceTile,
    /// Its size, tiles across.
    pub size: u16,
    /// The planet it keeps station with.
    pub planet: u8,
}

/// A nebula: a disc of coloured gas that fogs sensors.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Nebula {
    /// Its stable id (its nebula grid cell, or its system's id for a star's own nebula).
    pub id: u64,
    pub centre: SpaceTile,
    pub radius: i64,
    /// `[r, g, b]`.
    pub colour: [u8; 3],
    /// Sensor fog at the centre, per mille (falls to 0 at the rim).
    pub fog: u16,
}

impl Nebula {
    /// Fog at `at`, per mille (`fog * (1 - (d/r)²)`, 0 outside).
    #[must_use]
    pub fn fog_at(&self, at: SpaceTile) -> u16 {
        let d2 = self.centre.distance2(at);
        let r2 = (self.radius as i128) * (self.radius as i128);
        if d2 >= r2 || r2 == 0 {
            return 0;
        }
        let share = ((r2 - d2) * 1_000 / r2) as u32;
        (u32::from(self.fog) * share / 1_000) as u16
    }
}

/// A black hole: lensing and time dilation (in a free cell, never in a system).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct BlackHole {
    pub id: u64,
    pub centre: SpaceTile,
    /// Event horizon radius, tiles (no return).
    pub horizon: u32,
    /// Lensing reaches this far, tiles (the screen effect's radius).
    pub lensing_radius: u32,
    /// Time scale at the horizon, per mille (time runs this fast there; 1,000 at the lensing
    /// rim).
    pub time_dilation: u16,
    /// Gravity at the horizon, milli-g.
    pub gravity_milli_g: u32,
}

impl BlackHole {
    /// Time scale at `at` (1.0 outside the lensing radius, [`Self::time_dilation`] / 1,000
    /// at the horizon, eased in between).
    #[must_use]
    pub fn time_scale_at(&self, at: SpaceTile) -> f32 {
        let distance = self.centre.distance(at);
        let horizon = f64::from(self.horizon);
        let lens = f64::from(self.lensing_radius);
        if distance >= lens {
            return 1.0;
        }
        let low = f64::from(self.time_dilation) / 1_000.0;
        if distance <= horizon {
            return low as f32;
        }
        let depth = (lens - distance) / (lens - horizon);
        // Plain IEEE arithmetic (no `mul_add`): the same bits on every peer.
        #[allow(clippy::suboptimal_flops, reason = "bit-identical on every platform")]
        let scale = 1.0 - (1.0 - low) * depth * depth;
        scale as f32
    }
}

/// One mouth of a wormhole pair.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct WormholeMouth {
    pub id: WormholeId,
    pub centre: SpaceTile,
    /// Mouth radius, tiles.
    pub radius: u32,
    /// The far mouth's centre (fly in here, come out there).
    pub exit: SpaceTile,
    /// The far mouth's sector.
    pub exit_sector: SpaceSector,
}

/// A star system.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct StarSystem {
    pub index: u8,
    /// Its stable id (what a star chart names; never zero).
    pub id: u64,
    pub name: String,
    pub class: StarClass,
    /// The star's centre (or the twin suns' barycentre).
    pub centre: SpaceTile,
    /// One star, or two for [`StarClass::Binary`].
    pub stars: Vec<Star>,
    /// Planets and crater rocks outward, each planet followed by its moons.
    pub bodies: Vec<SpaceBody>,
    pub belts: Vec<AsteroidBelt>,
    pub comets: Vec<Comet>,
    pub stations: Vec<Station>,
    pub derelicts: Vec<Derelict>,
    /// The star's own nebula ([`StarClass::Nebula`]).
    pub nebula: Option<Nebula>,
    /// Farthest anything of it reaches, tiles from [`Self::centre`].
    pub extent: u32,
    /// Whether it is the home system.
    pub home: bool,
}

impl StarSystem {
    /// Its planets (not moons or crater rocks).
    pub fn planets(&self) -> impl Iterator<Item = &SpaceBody> {
        self.bodies
            .iter()
            .filter(|body| matches!(body.kind, BodyKind::Planet | BodyKind::HomeWorld))
    }

    /// How many planets circle it.
    #[must_use]
    pub fn planet_count(&self) -> usize {
        self.planets().count()
    }

    /// A body's name: the system's name and a numeral for planets, a letter for moons.
    #[must_use]
    pub fn body_name(&self, index: u8) -> String {
        let Some(body) = self.bodies.get(usize::from(index)) else {
            return String::new();
        };
        match body.kind {
            BodyKind::HomeWorld => "Home".to_string(),
            BodyKind::HomeMoon => "the Moon".to_string(),
            BodyKind::Planet | BodyKind::CraterRock => {
                let ordinal = self
                    .bodies
                    .iter()
                    .take(usize::from(index) + 1)
                    .filter(|other| other.parent.is_none())
                    .count();
                format!("{} {}", self.name, roman(ordinal))
            }
            BodyKind::Moon => {
                let parent = body.parent.unwrap_or(0);
                let letter = self
                    .bodies
                    .iter()
                    .take(usize::from(index))
                    .filter(|other| other.parent == Some(parent))
                    .count();
                format!(
                    "{} {}",
                    self.body_name(parent),
                    char::from(b'b' + (letter as u8).min(24))
                )
            }
        }
    }
}

fn roman(mut value: usize) -> String {
    const TABLE: [(usize, &str); 9] = [
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
        (0, ""),
        (0, ""),
        (0, ""),
    ];
    let mut out = String::new();
    for (step, numeral) in TABLE {
        if step == 0 {
            break;
        }
        while value >= step {
            out.push_str(numeral);
            value -= step;
        }
    }
    out
}

/// One generated space sector.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Sector {
    pub coords: SpaceSector,
    /// 1-3 systems (home: just the home system).
    pub systems: Vec<StarSystem>,
    pub black_holes: Vec<BlackHole>,
    /// Wormhole mouths in this sector.
    pub wormholes: Vec<WormholeMouth>,
    /// Interstellar nebulae overlapping this sector (each also listed by its neighbours).
    pub nebulae: Vec<Nebula>,
    /// Derelicts adrift between the stars (in free cells).
    pub derelicts: Vec<Derelict>,
}

impl Sector {
    /// System `index`.
    #[must_use]
    pub fn system(&self, index: u8) -> Option<&StarSystem> {
        self.systems.get(usize::from(index))
    }
}
