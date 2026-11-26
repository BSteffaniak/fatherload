//! The outland worlds on the planet theme framework (Wave 13 planets-c): the fungal, machine
//! and dead themes' [`PlanetThemeHooks`] and their weather.
//!
//! Each world generates whole chunks of its own ([`ThemeWorld`]); its hooks give the galaxy the
//! rest: its weather table (its own kinds, registered in the bodylife registry in the
//! planets-c range [`OUTLAND_WEATHER_IDS`]), its climate, hazard tags (the stations' planet gear
//! counters them), settlement site (the Tokkari build on the fungal and machine worlds, the
//! Siphra on the dead ones), creatures and music. Each places its own secret, so the
//! framework's default vault stays out of its ground.
//!
//! Unlike the Wave 12 storms, these kinds carry their own effects (the bodylife weather rules
//! apply them under the open sky): spore drift fouls the air and stings a pilot's suit, an ion
//! storm crackles over a parked rig's hull, the grey wind pushes craft and hides the plain.

use crate::{
    bodylife::weather::{
        WeatherId, WeatherKind as BodyWeatherKind, WeatherLook, WeatherOdds, WeatherParticles, odds,
    },
    enemy::EnemyKind,
    station::hazard::PlanetHazard as GearHazard,
};

use super::super::{
    PlanetGround, PlanetThemeId,
    hooks::{CreatureSpawn, MusicBed, PlanetThemeHooks, SecretSite, ThemeClimate, ThemeHazards},
    world::ThemeWorld,
};

/// The planets-c weather ids (`1000..=1019`, clear of every Wave 12 range).
pub const OUTLAND_WEATHER_IDS: std::ops::RangeInclusive<u16> = 1_000..=1_019;
/// The fungal world's spore drift: clouds of spores that foul the air.
pub const SPORE_DRIFT: WeatherId = WeatherId(1_000);
/// The fungal night's glowfall: luminous spores drifting down (night only).
pub const GLOWFALL: WeatherId = WeatherId(1_001);
/// The machine world's ion storm: static crackling over the plates.
pub const ION_STORM: WeatherId = WeatherId(1_002);
/// The dead world's grey wind: ash on a hard wind.
pub const GREY_WIND: WeatherId = WeatherId(1_003);
/// The dead night's ghostlight: pale lights over the plains (night only).
pub const GHOSTLIGHT: WeatherId = WeatherId(1_004);

/// The outland worlds' weather kinds (registered in
/// [`crate::bodylife::weather::WEATHER_REGISTRY`]).
pub const OUTLAND_WEATHER: &[BodyWeatherKind] = &[
    BodyWeatherKind {
        id: SPORE_DRIFT,
        name: "Spore Drift",
        track: "planets_c",
        needs_air: false,
        night_only: false,
        wind: 30,
        gust: 30,
        visibility_permille: 430,
        light_permille: -150,
        temperature_c: 0,
        hull_damage_milli: 0,
        suit_damage_milli: 250,
        fouls_air: true,
        look: WeatherLook {
            particles: WeatherParticles::Spores,
            colour: [214, 176, 236],
            fog: [120, 86, 140],
            sky_glow: None,
        },
        sound: "pc_spore_drift",
    },
    BodyWeatherKind {
        id: GLOWFALL,
        name: "Glowfall",
        track: "planets_c",
        needs_air: false,
        night_only: true,
        wind: 0,
        gust: 10,
        visibility_permille: 1_000,
        light_permille: 200,
        temperature_c: 0,
        hull_damage_milli: 0,
        suit_damage_milli: 0,
        fouls_air: false,
        look: WeatherLook {
            particles: WeatherParticles::Spores,
            colour: [120, 255, 214],
            fog: [0, 0, 0],
            sky_glow: Some([90, 200, 190]),
        },
        sound: "pc_glowfall",
    },
    BodyWeatherKind {
        id: ION_STORM,
        name: "Ion Storm",
        track: "planets_c",
        needs_air: false,
        night_only: false,
        wind: 0,
        gust: 0,
        visibility_permille: 560,
        light_permille: -120,
        temperature_c: 0,
        // Static crackles over a rig parked on the open plates.
        hull_damage_milli: 350,
        suit_damage_milli: 300,
        fouls_air: false,
        look: WeatherLook {
            particles: WeatherParticles::Dust,
            colour: [150, 210, 255],
            fog: [52, 70, 96],
            sky_glow: Some([90, 140, 220]),
        },
        sound: "pc_ion_storm",
    },
    BodyWeatherKind {
        id: GREY_WIND,
        name: "Grey Wind",
        track: "planets_c",
        needs_air: true,
        night_only: false,
        wind: 120,
        gust: 90,
        visibility_permille: 360,
        light_permille: -200,
        temperature_c: -8,
        hull_damage_milli: 0,
        suit_damage_milli: 0,
        fouls_air: true,
        look: WeatherLook {
            particles: WeatherParticles::Ash,
            colour: [190, 186, 180],
            fog: [96, 92, 90],
            sky_glow: None,
        },
        sound: "pc_grey_wind",
    },
    BodyWeatherKind {
        id: GHOSTLIGHT,
        name: "Ghostlight",
        track: "planets_c",
        needs_air: false,
        night_only: true,
        wind: 0,
        gust: 0,
        visibility_permille: 1_000,
        light_permille: 120,
        temperature_c: -4,
        hull_damage_milli: 0,
        suit_damage_milli: 0,
        fouls_air: false,
        look: WeatherLook {
            particles: WeatherParticles::Aurora,
            colour: [190, 255, 220],
            fog: [0, 0, 0],
            sky_glow: Some([150, 230, 200]),
        },
        sound: "pc_ghostlight",
    },
];

const FUNGAL_WEATHER: [WeatherOdds; 3] = [
    odds(WeatherId::CLEAR, 6),
    odds(SPORE_DRIFT, 3),
    odds(GLOWFALL, 3),
];
const MACHINE_WEATHER: [WeatherOdds; 2] = [odds(WeatherId::CLEAR, 2), odds(ION_STORM, 1)];
const DEAD_WEATHER: [WeatherOdds; 3] = [
    odds(WeatherId::CLEAR, 6),
    odds(GREY_WIND, 3),
    odds(GHOSTLIGHT, 3),
];

const fn spawn(kind: EnemyKind, weight: u16, min_depth: i32, max_depth: i32) -> CreatureSpawn {
    CreatureSpawn {
        creature: kind as u16,
        weight,
        min_depth,
        max_depth,
        night_only: false,
    }
}

const FUNGAL_SPAWNS: [CreatureSpawn; 4] = [
    spawn(EnemyKind::Capgrazer, 5, -4, 2),
    spawn(EnemyKind::Lanternfly, 5, 260, 900),
    spawn(EnemyKind::Sporeling, 4, -2, 2),
    spawn(EnemyKind::Rotmaw, 4, 520, 2_500),
];
const MACHINE_SPAWNS: [CreatureSpawn; 4] = [
    spawn(EnemyKind::Scuttlebot, 5, -2, 2),
    spawn(EnemyKind::WardDrone, 3, -6, 2),
    spawn(EnemyKind::Cogspider, 4, 240, 900),
    spawn(EnemyKind::ArcMite, 4, 520, 2_500),
];
const DEAD_SPAWNS: [CreatureSpawn; 4] = [
    spawn(EnemyKind::Ashrunner, 5, -2, 2),
    spawn(EnemyKind::Bonepicker, 4, -2, 2),
    spawn(EnemyKind::Gravewarden, 3, 240, 900),
    spawn(EnemyKind::Shade, 4, 520, 2_500),
];

/// The hooks of one outland world.
#[derive(Clone, Copy)]
pub struct OutlandHooks {
    theme: PlanetThemeId,
    world: &'static dyn ThemeWorld,
    weather: &'static [WeatherOdds],
    spawns: &'static [CreatureSpawn],
    core_bias_c: i16,
    corrosive_air_milli: u16,
    tags: &'static [(GearHazard, u8)],
    music: MusicBed,
}

impl std::fmt::Debug for OutlandHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OutlandHooks({:?})", self.theme)
    }
}

/// The fungal world's hooks.
pub static FUNGAL_HOOKS: OutlandHooks = OutlandHooks {
    theme: PlanetThemeId::FUNGAL,
    world: &super::fungal::FUNGAL_WORLD,
    weather: &FUNGAL_WEATHER,
    spawns: &FUNGAL_SPAWNS,
    core_bias_c: 20,
    // Spore-laden air stings a pilot's suit.
    corrosive_air_milli: 80,
    // The Sporeheart Deep's air (and the spore drift) fouls the hull.
    tags: &[(GearHazard::ToxicAir, 1)],
    music: MusicBed {
        name: "loop_planet_fungal",
        tempo: 66,
        brightness: 6,
    },
};

/// The machine world's hooks.
pub static MACHINE_HOOKS: OutlandHooks = OutlandHooks {
    theme: PlanetThemeId::MACHINE,
    world: &super::machine::MACHINE_WORLD,
    weather: &MACHINE_WEATHER,
    spawns: &MACHINE_SPAWNS,
    // The core reactor still runs.
    core_bias_c: 140,
    corrosive_air_milli: 0,
    tags: &[(GearHazard::Heat, 1)],
    music: MusicBed {
        name: "loop_planet_machine",
        tempo: 96,
        brightness: 4,
    },
};

/// The dead world's hooks.
pub static DEAD_HOOKS: OutlandHooks = OutlandHooks {
    theme: PlanetThemeId::DEAD,
    world: &super::dead::DEAD_WORLD,
    weather: &DEAD_WEATHER,
    spawns: &DEAD_SPAWNS,
    core_bias_c: -30,
    corrosive_air_milli: 0,
    // The ghost lights' chill.
    tags: &[(GearHazard::Cold, 1)],
    music: MusicBed {
        name: "loop_planet_dead",
        tempo: 52,
        brightness: 2,
    },
};

impl PlanetThemeHooks for OutlandHooks {
    fn id(&self) -> PlanetThemeId {
        self.theme
    }

    fn world(&self) -> Option<&'static dyn ThemeWorld> {
        Some(self.world)
    }

    /// The outland world lays its own marker and cache ([`super::secret_tile`]).
    fn secret(&self, _ground: &PlanetGround) -> Option<SecretSite> {
        None
    }

    fn creature_spawns(&self) -> &'static [CreatureSpawn] {
        self.spawns
    }

    fn weather_table(&self) -> &'static [WeatherOdds] {
        self.weather
    }

    /// The theme's settlement site first, then the flattest stretches.
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

    fn hazards(&self) -> ThemeHazards {
        ThemeHazards {
            corrosive_air_milli: self.corrosive_air_milli,
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
