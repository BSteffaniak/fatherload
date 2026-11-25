//! The frontier worlds on the planet theme framework (Wave 12 integration): the dune, volcanic
//! and crystal themes' [`PlanetThemeHooks`].
//!
//! The frontier generator keeps its own surface shaper and feature pass (they need the cave
//! list, so `PlanetGround` calls them by [`super::style_of`]); the hooks give the rest of the
//! galaxy what it reads off any theme: the weather table (each world's storm is a registered
//! bodylife weather kind, [`FRONTIER_WEATHER`], so the body badge, the forecast and the
//! frontier's own storm are one spell), the climate (the volcanic world runs hot below), the
//! hazard tags (the stations' planet gear counters them), the settlement site (the aliens build
//! their town on it), the creatures and the music. Each frontier body places its own secret
//! ([`super::secret`]), so the framework's default vault stays out of its ground.

use crate::{
    bodylife::weather::{
        WeatherId, WeatherKind as BodyWeatherKind, WeatherLook, WeatherOdds, WeatherParticles, odds,
    },
    planet::{
        PlanetGround, PlanetThemeId,
        hooks::{CreatureSpawn, MusicBed, PlanetThemeHooks, SecretSite, ThemeClimate},
    },
    station::hazard::PlanetHazard as GearHazard,
};

use super::FrontierStyle;

/// The dune world's sandstorm (planets-b weather range `128..=191`).
pub const SANDSTORM: WeatherId = WeatherId(128);
/// The volcanic world's ash fall.
pub const ASH_FALL: WeatherId = WeatherId(129);
/// The crystal world's glitterfrost.
pub const GLITTERFROST: WeatherId = WeatherId(130);

/// A frontier storm as a bodylife weather kind: it names the spell on the body badge. The
/// frontier draws its own storm over the view and applies its own wind and bite
/// ([`super::rules::weather_at`]), so the kind adds no particles, fog, wind or damage (they
/// would count twice). It blows on airless bodies too, so the forecast is the same pure
/// function of the seed whatever air the space generator rolled.
const fn storm(id: WeatherId, name: &'static str, colour: [u8; 3]) -> BodyWeatherKind {
    BodyWeatherKind {
        id,
        name,
        track: "planets_b",
        needs_air: false,
        night_only: false,
        wind: 0,
        gust: 0,
        visibility_permille: 1_000,
        light_permille: 0,
        temperature_c: 0,
        hull_damage_milli: 0,
        suit_damage_milli: 0,
        fouls_air: false,
        look: WeatherLook {
            particles: WeatherParticles::None,
            colour,
            fog: colour,
            sky_glow: None,
        },
        sound: "",
    }
}

/// The frontier worlds' weather kinds (registered in
/// [`crate::bodylife::weather::WEATHER_REGISTRY`]).
pub const FRONTIER_WEATHER: &[BodyWeatherKind] = &[
    storm(SANDSTORM, "Sandstorm", [214, 176, 112]),
    storm(ASH_FALL, "Ash fall", [96, 90, 92]),
    storm(GLITTERFROST, "Glitterfrost", [200, 226, 255]),
];

/// The storm kind of `style`.
#[must_use]
pub const fn storm_kind(style: FrontierStyle) -> WeatherId {
    match style {
        FrontierStyle::Desert => SANDSTORM,
        FrontierStyle::Volcanic => ASH_FALL,
        FrontierStyle::Crystal => GLITTERFROST,
    }
}

/// About one spell in four is a storm (the frontier's own schedule ran a storm of a minute or
/// so every four).
const fn table(storm: WeatherId) -> [WeatherOdds; 2] {
    [odds(WeatherId::CLEAR, 3), odds(storm, 1)]
}

const DUNE_WEATHER: [WeatherOdds; 2] = table(SANDSTORM);
const VOLCANIC_WEATHER: [WeatherOdds; 2] = table(ASH_FALL);
const CRYSTAL_WEATHER: [WeatherOdds; 2] = table(GLITTERFROST);

/// A creature for the framework's spawn table (the frontier spawner,
/// `world/enemies/frontier.rs`, places them by zone through the enemy body rules).
const fn spawn(
    kind: crate::enemy::EnemyKind,
    weight: u16,
    min_depth: i32,
    max_depth: i32,
) -> CreatureSpawn {
    CreatureSpawn {
        creature: kind as u16,
        weight,
        min_depth,
        max_depth,
        night_only: false,
    }
}

const DUNE_SPAWNS: [CreatureSpawn; 4] = {
    use crate::enemy::EnemyKind::{DuneSkipper, DuneWyrm, GlassScorpion, MirageMoth};
    [
        spawn(DuneSkipper, 4, -2, 2),
        spawn(MirageMoth, 2, -6, 0),
        spawn(DuneWyrm, 2, 2, 900),
        spawn(GlassScorpion, 3, 60, 2_500),
    ]
};
const VOLCANIC_SPAWNS: [CreatureSpawn; 4] = {
    use crate::enemy::EnemyKind::{BasaltTortoise, CinderBat, EmberStrider, MagmaSlug};
    [
        spawn(BasaltTortoise, 3, -2, 2),
        spawn(EmberStrider, 3, -2, 2),
        spawn(CinderBat, 3, 0, 1_200),
        spawn(MagmaSlug, 3, 40, 2_500),
    ]
};
const CRYSTAL_SPAWNS: [CreatureSpawn; 4] = {
    use crate::enemy::EnemyKind::{ChimeDrifter, GeodeMimic, GlintMoth, ShardCrab};
    [
        spawn(GlintMoth, 5, -6, 2),
        spawn(ShardCrab, 3, -2, 2_500),
        spawn(ChimeDrifter, 2, 60, 1_200),
        spawn(GeodeMimic, 3, 400, 2_500),
    ]
};

/// The hooks of one frontier world.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontierHooks {
    style: FrontierStyle,
    weather: &'static [WeatherOdds],
    spawns: &'static [CreatureSpawn],
    core_bias_c: i16,
    tags: &'static [(GearHazard, u8)],
    music: MusicBed,
}

/// The dune world's hooks.
pub static DUNE_HOOKS: FrontierHooks = FrontierHooks {
    style: FrontierStyle::Desert,
    weather: &DUNE_WEATHER,
    spawns: &DUNE_SPAWNS,
    core_bias_c: 40,
    tags: &[(GearHazard::Heat, 1)],
    music: MusicBed {
        name: "pb_loop_dune_world",
        tempo: 88,
        brightness: 7,
    },
};

/// The volcanic world's hooks.
pub static VOLCANIC_HOOKS: FrontierHooks = FrontierHooks {
    style: FrontierStyle::Volcanic,
    weather: &VOLCANIC_WEATHER,
    spawns: &VOLCANIC_SPAWNS,
    core_bias_c: 160,
    tags: &[(GearHazard::Heat, 2), (GearHazard::ToxicAir, 1)],
    music: MusicBed {
        name: "pb_loop_volcanic_world",
        tempo: 96,
        brightness: 3,
    },
};

/// The crystal world's hooks.
pub static CRYSTAL_HOOKS: FrontierHooks = FrontierHooks {
    style: FrontierStyle::Crystal,
    weather: &CRYSTAL_WEATHER,
    spawns: &CRYSTAL_SPAWNS,
    core_bias_c: -20,
    tags: &[(GearHazard::Cold, 1), (GearHazard::HighGravity, 1)],
    music: MusicBed {
        name: "pb_loop_crystal_world",
        tempo: 66,
        brightness: 9,
    },
};

impl PlanetThemeHooks for FrontierHooks {
    fn id(&self) -> PlanetThemeId {
        self.style.theme()
    }

    fn surface_rows(&self, ground: &PlanetGround, column: i64) -> i32 {
        super::generate::surface_height(ground, self.style, column)
    }

    /// The frontier body lays its own cache, ember or geode ([`super::secret`]).
    fn secret(&self, _ground: &PlanetGround) -> Option<SecretSite> {
        None
    }

    fn creature_spawns(&self) -> &'static [CreatureSpawn] {
        self.spawns
    }

    fn weather_table(&self) -> &'static [WeatherOdds] {
        self.weather
    }

    /// The frontier's settlement site first (an oasis rim, a basalt plateau, a crystal-free
    /// meadow), then the flattest stretches.
    fn settlement_sites(&self, ground: &PlanetGround, count: usize) -> Vec<i32> {
        let mut sites: Vec<i32> = super::settlement_site(ground)
            .map(|site| site.column + site.width / 2)
            .into_iter()
            .collect();
        if count > sites.len() {
            sites.extend(
                crate::planet::hooks::flattest_sites(ground, count)
                    .into_iter()
                    .skip(1)
                    .take(count - sites.len()),
            );
        }
        sites.truncate(count);
        sites
    }

    fn settlement_site(&self, ground: &PlanetGround) -> Option<(i32, i32)> {
        super::settlement_site(ground).map(|site| (site.column + site.width / 2, site.width))
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
