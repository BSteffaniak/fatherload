//! The wild worlds on the planet theme framework (Wave 12 integration): the jungle, ocean
//! archipelago and glacier themes' [`PlanetThemeHooks`].
//!
//! The wild worlds generate whole chunks of their own ([`ThemeWorld`], reached through
//! [`PlanetThemeHooks::world`]); their hooks give the rest of the galaxy what it reads off any
//! theme: their weather table (each world's storm is a registered bodylife weather kind,
//! [`WILDS_WEATHER`], so the body badge, the forecast and the theme's own storm are one
//! spell), their climate, their hazard tags (the stations' planet gear counters them), their
//! settlement site (the aliens build their town on it), their creatures and their music. Each
//! wild world places its own secret (a cairn over a cache), so the framework's default vault
//! stays out of its ground.

use crate::{
    bodylife::weather::{
        WeatherId, WeatherKind as BodyWeatherKind, WeatherLook, WeatherOdds, WeatherParticles, odds,
    },
    enemy::EnemyKind,
    station::hazard::PlanetHazard as GearHazard,
};

use super::super::{
    PlanetGround, PlanetThemeId,
    hooks::{CreatureSpawn, MusicBed, PlanetThemeHooks, SecretSite, ThemeClimate},
    world::ThemeWorld,
};

/// The jungle's downpour (planets-a weather range `64..=127`).
pub const DOWNPOUR: WeatherId = WeatherId(64);
/// The archipelago's squall.
pub const SQUALL: WeatherId = WeatherId(65);
/// The glacier's blizzard.
pub const BLIZZARD: WeatherId = WeatherId(66);

/// A wild world's storm as a bodylife weather kind: it names the spell, darkens and fogs the
/// view and draws its rain or snow. The theme's own rules give its push and its bite
/// ([`super::life::weather_at`]), so the kind carries no wind or damage of its own (they
/// would count twice). It blows on airless bodies too: a wild world's forecast is then the
/// same pure function of its seed whatever air the space generator rolled.
const fn storm(
    id: WeatherId,
    name: &'static str,
    particles: WeatherParticles,
    colour: [u8; 3],
    fog: [u8; 3],
    visibility_permille: u16,
    light_permille: i16,
) -> BodyWeatherKind {
    BodyWeatherKind {
        id,
        name,
        track: "planets_a",
        needs_air: false,
        night_only: false,
        wind: 0,
        gust: 0,
        visibility_permille,
        light_permille,
        temperature_c: 0,
        hull_damage_milli: 0,
        suit_damage_milli: 0,
        fouls_air: false,
        look: WeatherLook {
            particles,
            colour,
            fog,
            sky_glow: None,
        },
        sound: "",
    }
}

/// The wild worlds' weather kinds (registered in [`crate::bodylife::weather::WEATHER_REGISTRY`]).
pub const WILDS_WEATHER: &[BodyWeatherKind] = &[
    storm(
        DOWNPOUR,
        "Downpour",
        WeatherParticles::Rain,
        [150, 196, 176],
        [52, 78, 62],
        640,
        -220,
    ),
    storm(
        SQUALL,
        "Squall",
        WeatherParticles::Rain,
        [186, 214, 236],
        [74, 96, 118],
        600,
        -160,
    ),
    storm(
        BLIZZARD,
        "Blizzard",
        WeatherParticles::Snow,
        [244, 248, 255],
        [206, 218, 232],
        380,
        -120,
    ),
];

/// One storm spell in three, as the wild worlds' own schedule had it.
const fn table(storm: WeatherId) -> [WeatherOdds; 2] {
    [odds(WeatherId::CLEAR, 2), odds(storm, 1)]
}

const JUNGLE_WEATHER: [WeatherOdds; 2] = table(DOWNPOUR);
const OCEAN_WEATHER: [WeatherOdds; 2] = table(SQUALL);
const GLACIER_WEATHER: [WeatherOdds; 2] = table(BLIZZARD);

/// A creature id for the framework's spawn table: its [`EnemyKind`] code (the wild worlds'
/// own spawner, `world/enemies/wilds.rs`, places them by the enemy body rules).
const fn spawn(kind: EnemyKind, weight: u16, min_depth: i32, max_depth: i32) -> CreatureSpawn {
    CreatureSpawn {
        creature: kind as u16,
        weight,
        min_depth,
        max_depth,
        night_only: false,
    }
}

const JUNGLE_SPAWNS: [CreatureSpawn; 4] = [
    spawn(EnemyKind::Trundlehorn, 3, -4, 2),
    spawn(EnemyKind::Vinestalker, 2, -30, 0),
    spawn(EnemyKind::Mossprowler, 3, -2, 2),
    spawn(EnemyKind::Sporewing, 4, 200, 900),
];
const OCEAN_SPAWNS: [CreatureSpawn; 4] = [
    spawn(EnemyKind::Shellstrider, 2, -2, 40),
    spawn(EnemyKind::Glimmerfin, 4, 0, 40),
    spawn(EnemyKind::Sawjaw, 3, 0, 2_500),
    spawn(EnemyKind::Tidesnapper, 3, -2, 2),
];
const GLACIER_SPAWNS: [CreatureSpawn; 4] = [
    spawn(EnemyKind::Woolhorn, 3, -4, 2),
    spawn(EnemyKind::Rimeskipper, 4, -2, 2),
    spawn(EnemyKind::Rimewolf, 3, -2, 2),
    spawn(EnemyKind::Crevassewyrm, 2, 4, 900),
];

/// The hooks of one wild world.
#[derive(Clone, Copy)]
pub struct WildHooks {
    theme: PlanetThemeId,
    world: &'static dyn ThemeWorld,
    weather: &'static [WeatherOdds],
    spawns: &'static [CreatureSpawn],
    core_bias_c: i16,
    tags: &'static [(GearHazard, u8)],
    music: MusicBed,
}

impl std::fmt::Debug for WildHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WildHooks({:?})", self.theme)
    }
}

/// The jungle's hooks.
pub static JUNGLE_HOOKS: WildHooks = WildHooks {
    theme: PlanetThemeId::JUNGLE,
    world: &super::jungle::JUNGLE_WORLD,
    weather: &JUNGLE_WEATHER,
    spawns: &JUNGLE_SPAWNS,
    core_bias_c: 30,
    // The Spore Caves foul the air.
    tags: &[(GearHazard::ToxicAir, 1)],
    music: MusicBed {
        name: "loop_planet_jungle",
        tempo: 84,
        brightness: 7,
    },
};

/// The ocean archipelago's hooks.
pub static OCEAN_HOOKS: WildHooks = WildHooks {
    theme: PlanetThemeId::OCEAN,
    world: &super::ocean::OCEAN_WORLD,
    weather: &OCEAN_WEATHER,
    spawns: &OCEAN_SPAWNS,
    core_bias_c: 60,
    // The Trench's black smokers scald.
    tags: &[(GearHazard::Heat, 1)],
    music: MusicBed {
        name: "loop_planet_ocean",
        tempo: 70,
        brightness: 6,
    },
};

/// The glacier's hooks.
pub static GLACIER_HOOKS: WildHooks = WildHooks {
    theme: PlanetThemeId::GLACIER,
    world: &super::glacier::GLACIER_WORLD,
    weather: &GLACIER_WEATHER,
    spawns: &GLACIER_SPAWNS,
    core_bias_c: -40,
    tags: &[(GearHazard::Cold, 2)],
    music: MusicBed {
        name: "loop_planet_glacier",
        tempo: 62,
        brightness: 8,
    },
};

impl PlanetThemeHooks for WildHooks {
    fn id(&self) -> PlanetThemeId {
        self.theme
    }

    fn world(&self) -> Option<&'static dyn ThemeWorld> {
        Some(self.world)
    }

    /// The wild world lays its own cairn and cache ([`super::secret_tile`]).
    fn secret(&self, _ground: &PlanetGround) -> Option<SecretSite> {
        None
    }

    fn creature_spawns(&self) -> &'static [CreatureSpawn] {
        self.spawns
    }

    fn weather_table(&self) -> &'static [WeatherOdds] {
        self.weather
    }

    /// The theme's settlement site first (level, dry, clear), then the flattest stretches.
    fn settlement_sites(&self, ground: &PlanetGround, count: usize) -> Vec<i32> {
        let mut sites: Vec<i32> = self
            .world
            .settlement_site(ground)
            .map(|site| site.centre())
            .into_iter()
            .collect();
        if count > sites.len() {
            sites.extend(
                super::super::hooks::flattest_sites(ground, count)
                    .into_iter()
                    .skip(1)
                    .take(count - sites.len()),
            );
        }
        sites.truncate(count);
        sites
    }

    fn settlement_site(&self, ground: &PlanetGround) -> Option<(i32, i32)> {
        self.world
            .settlement_site(ground)
            .map(|site| (site.centre(), site.width))
    }

    fn climate(&self) -> ThemeClimate {
        ThemeClimate {
            core_bias_c: self.core_bias_c,
        }
    }

    fn hazard_tags(&self) -> &'static [(GearHazard, u8)] {
        self.tags
    }

    fn rules_own_hazards(&self) -> bool {
        true
    }

    fn music_bed(&self) -> MusicBed {
        self.music
    }
}
