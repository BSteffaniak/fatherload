//! The exotic worlds on the planet theme framework: the gas giant, locked and ring themes'
//! [`PlanetThemeHooks`].
//!
//! Each world builds its own chunks ([`ThemeWorld`], reached through
//! [`PlanetThemeHooks::world`]); the hooks give the rest of the galaxy what it reads off any
//! theme: the weather table (each world's weather is a registered bodylife kind,
//! [`EXOTIC_WEATHER`], ids 320-324 of the planets-d range), the climate, the hazard tags the
//! stations' planet gear counters, the settlement site (the aliens' town), the creatures and
//! the music. Each world places its own secret, so the framework's default vault stays out.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::too_long_first_doc_paragraph,
    clippy::missing_const_for_fn,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::collapsible_match,
    clippy::too_many_lines,
    reason = "generation and rules arithmetic over bounded columns, rows and milliseconds; \
              doc paragraphs that read as one thought"
)]

use crate::{
    bodylife::weather::{
        WeatherId, WeatherKind as BodyWeatherKind, WeatherLook, WeatherOdds, WeatherParticles, odds,
    },
    enemy::EnemyKind,
    planet::{
        PlanetGround, PlanetThemeId,
        hooks::{CreatureSpawn, MusicBed, PlanetThemeHooks, SecretSite, ThemeClimate},
        world::ThemeWorld,
    },
    station::hazard::PlanetHazard as GearHazard,
};

use super::ExoticStyle;

/// The gas giant's superstorm (planets-d weather range `320..=351`): the jet streams blow
/// twice as hard and the Storm Belt's bolts come twice as often.
pub const SUPERSTORM: WeatherId = WeatherId(320);
/// The locked world's nitrogen snow (it falls only where it is dark: the night side).
pub const NITROGEN_SNOW: WeatherId = WeatherId(321);
/// The locked world's terminator gale: hot air rushing to the night side along the twilight.
pub const TERMINATOR_GALE: WeatherId = WeatherId(322);
/// The ring world's ring shower: meteors fall three times as often, bright streaks over the
/// sky.
pub const RING_SHOWER: WeatherId = WeatherId(323);

/// A weather kind of this track. The world rules give the gameplay (the jet streams, the
/// bolts, the meteors, the gale's push), so the kinds carry only their look (and the snow's
/// chill, which the climate reads).
#[allow(clippy::too_many_arguments, reason = "one weather kind's look")]
const fn kind(
    id: WeatherId,
    name: &'static str,
    night_only: bool,
    particles: WeatherParticles,
    colour: [u8; 3],
    fog: [u8; 3],
    visibility_permille: u16,
    light_permille: i16,
    temperature_c: i16,
) -> BodyWeatherKind {
    BodyWeatherKind {
        id,
        name,
        track: "planets_d",
        needs_air: false,
        night_only,
        wind: 0,
        gust: 0,
        visibility_permille,
        light_permille,
        temperature_c,
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

/// The exotic worlds' weather kinds (registered in
/// [`crate::bodylife::weather::WEATHER_REGISTRY`]).
pub const EXOTIC_WEATHER: &[BodyWeatherKind] = &[
    kind(
        SUPERSTORM,
        "Superstorm",
        false,
        WeatherParticles::Rain,
        [236, 206, 160],
        [150, 104, 70],
        620,
        -200,
        0,
    ),
    kind(
        NITROGEN_SNOW,
        "Nitrogen snow",
        true,
        WeatherParticles::Snow,
        [214, 230, 255],
        [120, 140, 190],
        560,
        -60,
        -20,
    ),
    kind(
        TERMINATOR_GALE,
        "Terminator gale",
        false,
        WeatherParticles::Dust,
        [210, 160, 120],
        [150, 110, 90],
        700,
        -80,
        0,
    ),
    kind(
        RING_SHOWER,
        "Ring shower",
        false,
        WeatherParticles::None,
        [255, 236, 200],
        [120, 130, 180],
        1_000,
        40,
        0,
    ),
];

const GAS_WEATHER: [WeatherOdds; 2] = [odds(WeatherId::CLEAR, 3), odds(SUPERSTORM, 1)];
const LOCKED_WEATHER: [WeatherOdds; 3] = [
    odds(WeatherId::CLEAR, 3),
    odds(NITROGEN_SNOW, 2),
    odds(TERMINATOR_GALE, 1),
];
const RING_WEATHER: [WeatherOdds; 2] = [odds(WeatherId::CLEAR, 3), odds(RING_SHOWER, 1)];

/// The storm of `style` (the kind whose spells its rules feel).
#[must_use]
pub const fn storm_kind(style: ExoticStyle) -> WeatherId {
    match style {
        ExoticStyle::GasGiant => SUPERSTORM,
        ExoticStyle::Locked => TERMINATOR_GALE,
        ExoticStyle::Ring => RING_SHOWER,
    }
}

const fn spawn(kind: EnemyKind, weight: u16, min_depth: i32, max_depth: i32) -> CreatureSpawn {
    CreatureSpawn {
        creature: kind as u16,
        weight,
        min_depth,
        max_depth,
        night_only: false,
    }
}

const GAS_SPAWNS: [CreatureSpawn; 4] = [
    spawn(EnemyKind::DriftRay, 4, -40, 300),
    spawn(EnemyKind::Gasbag, 2, -30, 800),
    spawn(EnemyKind::StormEel, 3, 300, 1_800),
    spawn(EnemyKind::VaporStinger, 3, -20, 500),
];
const LOCKED_SPAWNS: [CreatureSpawn; 4] = [
    spawn(EnemyKind::DuskGrazer, 3, -4, 2),
    spawn(EnemyKind::Sunbasker, 3, -2, 2),
    spawn(EnemyKind::ScorchMite, 3, -6, 600),
    spawn(EnemyKind::FrostStalker, 3, -2, 900),
];
const RING_SPAWNS: [CreatureSpawn; 4] = [
    spawn(EnemyKind::Ringhopper, 4, -130, 2),
    spawn(EnemyKind::Shardback, 2, -4, 2),
    spawn(EnemyKind::TetherDrone, 3, 300, 1_500),
    spawn(EnemyKind::VoidMite, 3, -130, 600),
];

/// The hooks of one exotic world.
#[derive(Clone, Copy)]
pub struct ExoticHooks {
    style: ExoticStyle,
    weather: &'static [WeatherOdds],
    spawns: &'static [CreatureSpawn],
    core_bias_c: i16,
    tags: &'static [(GearHazard, u8)],
    music: MusicBed,
}

impl std::fmt::Debug for ExoticHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExoticHooks({:?})", self.style)
    }
}

/// The gas giant's hooks.
pub static GAS_GIANT_HOOKS: ExoticHooks = ExoticHooks {
    style: ExoticStyle::GasGiant,
    weather: &GAS_WEATHER,
    spawns: &GAS_SPAWNS,
    core_bias_c: 120,
    // The giant's pull and the deep's pressure; the cloud tops' cold.
    tags: &[(GearHazard::HighGravity, 2), (GearHazard::Cold, 1)],
    music: MusicBed {
        name: "pd_music_gas_giant",
        tempo: 66,
        brightness: 7,
    },
};

/// The locked world's hooks.
pub static LOCKED_HOOKS: ExoticHooks = ExoticHooks {
    style: ExoticStyle::Locked,
    weather: &LOCKED_WEATHER,
    spawns: &LOCKED_SPAWNS,
    core_bias_c: 30,
    tags: &[(GearHazard::Heat, 2), (GearHazard::Cold, 2)],
    music: MusicBed {
        name: "pd_music_locked",
        tempo: 58,
        brightness: 4,
    },
};

/// The ring world's hooks.
pub static RING_HOOKS: ExoticHooks = ExoticHooks {
    style: ExoticStyle::Ring,
    weather: &RING_WEATHER,
    spawns: &RING_SPAWNS,
    core_bias_c: 20,
    tags: &[(GearHazard::HighGravity, 1)],
    music: MusicBed {
        name: "pd_music_ring",
        tempo: 74,
        brightness: 8,
    },
};

impl PlanetThemeHooks for ExoticHooks {
    fn id(&self) -> PlanetThemeId {
        self.style.theme()
    }

    fn world(&self) -> Option<&'static dyn ThemeWorld> {
        Some(self.style.world())
    }

    /// The world lays its own marker and cache ([`super::secret_tile`]).
    fn secret(&self, _ground: &PlanetGround) -> Option<SecretSite> {
        None
    }

    fn creature_spawns(&self) -> &'static [CreatureSpawn] {
        self.spawns
    }

    fn weather_table(&self) -> &'static [WeatherOdds] {
        self.weather
    }

    fn settlement_sites(&self, ground: &PlanetGround, count: usize) -> Vec<i32> {
        let mut sites: Vec<i32> = self
            .style
            .world()
            .settlement_site(ground)
            .map(|site| site.centre())
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
        self.style
            .world()
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
