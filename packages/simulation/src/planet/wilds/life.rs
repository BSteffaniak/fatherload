//! What it is like to be on a wild world (Wave 12 planets-a).
//!
//! Each theme's hazards (tagged for the stations track's planet gear), its strata's signature
//! rules, its creatures and where they live, its weather and its day. Pure data and pure
//! functions of the body and the world time; `world/wilds.rs` plays them.

use super::{PlanetGround, PlanetParams, roll};
use crate::planet::PlanetThemeId;
use crate::{depth_progression::RigSystem, enemy::EnemyKind};

/// A planet hazard, tagged so planet gear can counter it (§16 "planet-specific gear: heat,
/// cold, toxic air, high gravity").
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlanetHazard {
    /// Scalding heat (black smokers): the hull burns.
    Heat,
    /// Cold that drinks fuel (the heaters run) and, in cold water, three times as fast.
    Cold,
    /// Air thick with spores: the hull's seals foul.
    ToxicAir,
    /// A heavy world (1.4 g and up): engines labour (the gravity itself; gear lightens it).
    HighGravity,
    /// Crushing water deep in the Trench.
    Pressure,
}

impl PlanetHazard {
    /// Every hazard.
    pub const ALL: [Self; 5] = [
        Self::Heat,
        Self::Cold,
        Self::ToxicAir,
        Self::HighGravity,
        Self::Pressure,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Heat => "heat",
            Self::Cold => "cold",
            Self::ToxicAir => "toxic air",
            Self::HighGravity => "high gravity",
            Self::Pressure => "pressure",
        }
    }

    /// The rig system from the Descent that already counters it (`None`: only planet gear
    /// will). TODO-SEAM w12-stations: the planet gear line counters every hazard too
    /// (`WorldState::planet_gear_counters`).
    #[must_use]
    pub const fn descent_counter(self) -> Option<RigSystem> {
        match self {
            Self::Cold => Some(RigSystem::CryoHull),
            Self::ToxicAir => Some(RigSystem::BioFilter),
            Self::Pressure => Some(RigSystem::PressureHull),
            Self::Heat | Self::HighGravity => None,
        }
    }
}

/// The gravity from which a world counts as heavy (milli-g).
pub const HIGH_GRAVITY_MILLI: u16 = 1_400;

/// A stratum's signature rule (the one rule that changes how you play there).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignatureRule {
    /// Living roots snag the tracks of a rig rubbing against them.
    GraspingRoots,
    /// A drilled puffcap bursts into a spore cloud.
    SporeBursts,
    /// A drilled lifesap node mends the hull.
    Lifesap,
    /// The rock is full of sea-water pockets: a shaft floods.
    Waterlogged,
    /// Flooded caves round sealed halls of air.
    SealedHalls,
    /// Black smokers scald what comes close; their ore is rich.
    BlackSmokers,
    /// Ice is slick, and tunnels refreeze at night (and thaw by day).
    SlickIce,
    /// The water bites three times as cold.
    ColdWater,
    /// Icicles fall on what passes under them.
    Icicles,
}

impl SignatureRule {
    /// One line for the HUD and the docs.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::GraspingRoots => "Living roots snag your tracks: drill them out of your way",
            Self::SporeBursts => "Puffcaps burst into spore clouds when drilled",
            Self::Lifesap => "Glowing lifesap mends the hull when drilled",
            Self::Waterlogged => "Waterlogged rock: pockets of sea flood your shaft",
            Self::SealedHalls => "Flooded caves round sealed halls of air",
            Self::BlackSmokers => "Black smokers scald what comes close; rich ore round them",
            Self::SlickIce => "Slick ice; tunnels refreeze at night",
            Self::ColdWater => "The water here bites three times as cold",
            Self::Icicles => "Icicles fall on what passes under them",
        }
    }
}

/// A wild stratum's play rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WildStratum {
    pub signature: SignatureRule,
    /// Hazards in this stratum (on top of the theme's surface hazards where they reach).
    pub hazards: &'static [PlanetHazard],
}

/// Where a creature lives on a wild world.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Habitat {
    /// On the open surface (a floor under the sky).
    Surface,
    /// Hanging under the canopy or a ledge (air with solid ground right over it).
    Hanging,
    /// In the sea (or any deep water).
    Water,
    /// In the caves of stratum `n` (open ground with a floor).
    Caves(u8),
    /// Flying in the caves of stratum `n`.
    CaveAir(u8),
}

/// One of a theme's native creatures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WildCreature {
    pub kind: EnemyKind,
    pub habitat: Habitat,
    /// Spawn weight among the creatures whose habitat fits.
    pub weight: u32,
    /// How many come together (min, max).
    pub pack: (u8, u8),
    /// Neutral fauna: never attacks first (fights back when struck).
    pub neutral: bool,
}

/// A theme's weather.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum WeatherKind {
    /// Warm, heavy rain in the canopy (the jungle).
    Downpour,
    /// Wind and spray off the sea (the archipelago): it pushes craft over the waves.
    Squall,
    /// Snow and wind (the glacier): it pushes craft and doubles the cold.
    Blizzard,
}

impl WeatherKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Downpour => "downpour",
            Self::Squall => "squall",
            Self::Blizzard => "blizzard",
        }
    }

    /// Wind at full strength, pixels per second (a push across the surface).
    #[must_use]
    pub const fn wind(self) -> f32 {
        match self {
            Self::Downpour => 0.0,
            Self::Squall => 70.0,
            Self::Blizzard => 95.0,
        }
    }
}

/// The weather on a body at a moment: its kind and how strong it is (0: clear, 1000: full).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Weather {
    pub kind: WeatherKind,
    pub intensity: u16,
}

impl Weather {
    /// The intensity as a fraction.
    #[must_use]
    pub fn strength(&self) -> f32 {
        f32::from(self.intensity) / 1_000.0
    }
}

/// A wild theme's life (see the module docs).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WildTheme {
    pub theme: PlanetThemeId,
    /// Hazards on the open surface.
    pub surface_hazards: &'static [PlanetHazard],
    pub strata: [WildStratum; 3],
    pub creatures: &'static [WildCreature],
    pub weather: WeatherKind,
    /// The music bed's name (the game's patch).
    pub music: &'static str,
}

const JUNGLE_CREATURES: [WildCreature; 4] = [
    WildCreature {
        kind: EnemyKind::Trundlehorn,
        habitat: Habitat::Surface,
        weight: 5,
        pack: (1, 3),
        neutral: true,
    },
    WildCreature {
        kind: EnemyKind::Vinestalker,
        habitat: Habitat::Hanging,
        weight: 3,
        pack: (1, 1),
        neutral: false,
    },
    WildCreature {
        kind: EnemyKind::Mossprowler,
        habitat: Habitat::Surface,
        weight: 3,
        pack: (1, 2),
        neutral: false,
    },
    WildCreature {
        kind: EnemyKind::Sporewing,
        habitat: Habitat::CaveAir(1),
        weight: 5,
        pack: (2, 4),
        neutral: false,
    },
];

const OCEAN_CREATURES: [WildCreature; 4] = [
    WildCreature {
        kind: EnemyKind::Shellstrider,
        habitat: Habitat::Water,
        weight: 3,
        pack: (1, 1),
        neutral: true,
    },
    WildCreature {
        kind: EnemyKind::Glimmerfin,
        habitat: Habitat::Water,
        weight: 6,
        pack: (3, 6),
        neutral: true,
    },
    WildCreature {
        kind: EnemyKind::Sawjaw,
        habitat: Habitat::Water,
        weight: 3,
        pack: (1, 2),
        neutral: false,
    },
    WildCreature {
        kind: EnemyKind::Tidesnapper,
        habitat: Habitat::Surface,
        weight: 4,
        pack: (1, 3),
        neutral: false,
    },
];

const GLACIER_CREATURES: [WildCreature; 4] = [
    WildCreature {
        kind: EnemyKind::Woolhorn,
        habitat: Habitat::Surface,
        weight: 5,
        pack: (2, 3),
        neutral: true,
    },
    WildCreature {
        kind: EnemyKind::Rimeskipper,
        habitat: Habitat::Surface,
        weight: 4,
        pack: (2, 4),
        neutral: true,
    },
    WildCreature {
        kind: EnemyKind::Rimewolf,
        habitat: Habitat::Surface,
        weight: 3,
        pack: (2, 4),
        neutral: false,
    },
    WildCreature {
        kind: EnemyKind::Crevassewyrm,
        habitat: Habitat::Caves(2),
        weight: 3,
        pack: (1, 1),
        neutral: false,
    },
];

/// The wild themes' life, by theme.
pub const WILD_THEMES: [WildTheme; 3] = [
    WildTheme {
        theme: PlanetThemeId::JUNGLE,
        surface_hazards: &[],
        strata: [
            WildStratum {
                signature: SignatureRule::GraspingRoots,
                hazards: &[],
            },
            WildStratum {
                signature: SignatureRule::SporeBursts,
                hazards: &[PlanetHazard::ToxicAir],
            },
            WildStratum {
                signature: SignatureRule::Lifesap,
                hazards: &[],
            },
        ],
        creatures: &JUNGLE_CREATURES,
        weather: WeatherKind::Downpour,
        music: "planet_jungle",
    },
    WildTheme {
        theme: PlanetThemeId::OCEAN,
        surface_hazards: &[],
        strata: [
            WildStratum {
                signature: SignatureRule::Waterlogged,
                hazards: &[],
            },
            WildStratum {
                signature: SignatureRule::SealedHalls,
                hazards: &[PlanetHazard::Pressure],
            },
            WildStratum {
                signature: SignatureRule::BlackSmokers,
                hazards: &[PlanetHazard::Heat, PlanetHazard::Pressure],
            },
        ],
        creatures: &OCEAN_CREATURES,
        weather: WeatherKind::Squall,
        music: "planet_ocean",
    },
    WildTheme {
        theme: PlanetThemeId::GLACIER,
        surface_hazards: &[PlanetHazard::Cold],
        strata: [
            WildStratum {
                signature: SignatureRule::SlickIce,
                hazards: &[PlanetHazard::Cold],
            },
            WildStratum {
                signature: SignatureRule::ColdWater,
                hazards: &[PlanetHazard::Cold],
            },
            WildStratum {
                signature: SignatureRule::Icicles,
                hazards: &[PlanetHazard::Cold],
            },
        ],
        creatures: &GLACIER_CREATURES,
        weather: WeatherKind::Blizzard,
        music: "planet_glacier",
    },
];

/// The life of wild theme `theme` (`None` for other themes).
#[must_use]
pub fn wild_theme(theme: PlanetThemeId) -> Option<&'static WildTheme> {
    WILD_THEMES.iter().find(|wild| wild.theme == theme)
}

/// Every hazard a body of `params` has anywhere (its theme's, and high gravity on a heavy one).
#[must_use]
pub fn body_hazards(params: &PlanetParams) -> Vec<PlanetHazard> {
    let mut hazards: Vec<PlanetHazard> = Vec::new();
    if let Some(wild) = wild_theme(params.theme) {
        for hazard in wild.surface_hazards.iter().chain(
            wild.strata
                .iter()
                .flat_map(|stratum| stratum.hazards.iter()),
        ) {
            if !hazards.contains(hazard) {
                hazards.push(*hazard);
            }
        }
    }
    if params.gravity_milli >= HIGH_GRAVITY_MILLI {
        hazards.push(PlanetHazard::HighGravity);
    }
    hazards.sort();
    hazards
}

/// Seconds a weather cycle lasts.
pub const WEATHER_CYCLE_SECONDS: u64 = 150;
/// One weather cycle in three brings a storm (on average).
const STORM_ONE_IN: u64 = 3;

/// The weather on the body of `params` at world time `millis`.
///
/// A pure function of the body and the time: every peer agrees. TODO-SEAM w12-bodylife: the
/// per-body weather system replaces this schedule; the theme's kind ([`WildTheme::weather`])
/// is what it should pick.
#[must_use]
pub fn weather_at(params: &PlanetParams, millis: u64) -> Option<Weather> {
    let wild = wild_theme(params.theme)?;
    let cycle_millis = WEATHER_CYCLE_SECONDS * 1_000;
    let cycle = millis / cycle_millis;
    let ground_roll =
        super::super::hash3(params.seed, 0x3EA7, i64::try_from(cycle).unwrap_or(0), 0);
    if !ground_roll.is_multiple_of(STORM_ONE_IN) {
        return Some(Weather {
            kind: wild.weather,
            intensity: 0,
        });
    }
    // Up over the first fifth, full in the middle, down over the last fifth.
    let into = millis % cycle_millis;
    let ramp = cycle_millis / 5;
    let level = if into < ramp {
        into * 1_000 / ramp
    } else if into > cycle_millis - ramp {
        (cycle_millis - into) * 1_000 / ramp
    } else {
        1_000
    };
    Some(Weather {
        kind: wild.weather,
        intensity: u16::try_from(level).unwrap_or(1_000),
    })
}

/// The body's day length, seconds. TODO-SEAM w12-bodylife: the space body's `day_seconds`
/// through the per-body clock; until then a length from the body's seed (8-40 minutes, the
/// space generator's range).
#[must_use]
pub const fn day_seconds(params: &PlanetParams) -> u64 {
    480 + super::super::hash3(params.seed, 0xDA7, 0, 0) % 1_920
}

/// Whether it is night at the surface of the body of `params` at world time `millis` (the
/// second half of each day).
#[must_use]
pub const fn is_night(params: &PlanetParams, millis: u64) -> bool {
    let day = day_seconds(params) * 1_000;
    millis % day >= day / 2
}

/// The hazards in force at strip tile `(column, row)` of `ground` (without weather or water).
#[must_use]
pub fn hazards_at(ground: &PlanetGround, column: i32, row: i32) -> &'static [PlanetHazard] {
    let Some(wild) = wild_theme(ground.params().theme) else {
        return &[];
    };
    if row < crate::planet::BodyGround::surface_row(ground, column) {
        return wild.surface_hazards;
    }
    if row >= ground.core_chamber_top() {
        return &[];
    }
    wild.strata[ground.stratum_index(column, row).min(2)].hazards
}

/// The signature rule of the stratum at `(column, row)`, if it is a wild stratum.
#[must_use]
pub fn signature_at(ground: &PlanetGround, column: i32, row: i32) -> Option<SignatureRule> {
    let wild = wild_theme(ground.params().theme)?;
    if row < crate::planet::BodyGround::surface_row(ground, column)
        || row >= ground.core_chamber_top()
    {
        return None;
    }
    Some(wild.strata[ground.stratum_index(column, row).min(2)].signature)
}

/// A deterministic roll for life on `ground` (spawns, refreezing).
#[must_use]
pub const fn life_roll(ground: &PlanetGround, purpose: u64, a: i64, b: i64) -> u64 {
    roll(ground, 0x1000 + purpose, a, b)
}

/// Something a wild world did (netcode codes 900-919).
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum WildsEvent {
    /// Living roots snag a rig's tracks.
    RootsSnag,
    /// A puffcap burst into a spore cloud.
    SporeBurst,
    /// Lifesap mended a hull.
    LifesapMended,
    /// A black smoker's heat burns a hull.
    Scalded,
    /// The Trench's water creaks against a hull.
    PressureCreak,
    /// The cold bites (fuel drains for the heaters).
    ColdBite,
    /// An icicle cracked over a rig (it falls next).
    IcicleCrack,
    /// Tunnels refroze round a rig.
    Refroze,
    /// A landmark's relic was dug out.
    LandmarkRelic,
    /// A small body's secret cache was dug out.
    SecretCache,
    /// A storm is rising.
    StormRising,
    /// Rime (a snow bridge) gave way under a rig.
    RimeBroke,
}

impl WildsEvent {
    /// Every event, in code order.
    pub const ALL: [Self; 12] = [
        Self::RootsSnag,
        Self::SporeBurst,
        Self::LifesapMended,
        Self::Scalded,
        Self::PressureCreak,
        Self::ColdBite,
        Self::IcicleCrack,
        Self::Refroze,
        Self::LandmarkRelic,
        Self::SecretCache,
        Self::StormRising,
        Self::RimeBroke,
    ];

    /// Offset of the event's netcode code from 900.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// The event with offset `code`.
    #[must_use]
    pub const fn from_code(code: u16) -> Option<Self> {
        if (code as usize) < Self::ALL.len() {
            Some(Self::ALL[code as usize])
        } else {
            None
        }
    }
}
