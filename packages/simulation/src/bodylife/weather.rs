//! Weather on a body (Wave 12 bodylife): a registry of weather kinds and each body's forecast.
//!
//! **The registry** ([`WEATHER_REGISTRY`]) is a list of per-track lists of [`WeatherKind`]s.
//! Every kind carries its own [`WeatherId`], drawn from its track's range ([`WeatherId`]'s
//! docs), so tracks append their own list without renumbering anybody's: rain, storms, snow
//! and auroras are this track's; sandstorms, ash fall, acid rain, spore drift and the rest are
//! the theme tracks'. A theme says how often each kind blows on its worlds with its weather
//! table ([`WeatherOdds`], [`crate::planet::hooks::PlanetThemeHooks::weather_table`]).
//!
//! **The forecast** ([`forecast`]) is a pure function of the body's seed, its weather table,
//! its air and the world time: time is cut into spells of [`SPELL_MILLIS`], each spell's kind
//! is drawn from the table by a hash of the seed and the spell, and a kind fades in and out
//! over [`RAMP_MILLIS`] where the spell before or after differs. Nothing is stored, saved or
//! replicated: every peer reads the same weather from the same tick, at every tick rate.
//!
//! **Effects** ([`WeatherNow`]) at full strength are the kind's, scaled by the spell's
//! intensity: wind (with gusts) that carries an airborne rig and pilot, visibility (the view's
//! fog; every creature's line-of-sight noticing range scales by it), light, a temperature change (the
//! climate reads it), hull and suit damage for whatever stands under the open sky, and air
//! the pilot cannot breathe.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "bounded clock, price and weather arithmetic: columns, permille and milliseconds"
)]

use serde::{Deserialize, Serialize};

use crate::space::Atmosphere;

/// A weather kind's id (saved in events, read by the renderer and audio).
///
/// Ranges by track (append only): bodylife `0..=63`, planets-a `64..=127`, planets-b
/// `128..=191`, aliens `192..=223`, stations `224..=239`, hostiles `240..=255`; later waves
/// from 256 up (Wave 13 planets-d: `320..=351`).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct WeatherId(pub u16);

impl WeatherId {
    /// Clear skies (no weather).
    pub const CLEAR: Self = Self(0);
    /// Rain and gales (bodylife).
    pub const STORM: Self = Self(1);
    /// Snowfall (bodylife).
    pub const SNOW: Self = Self(2);
    /// An aurora over the night side (bodylife).
    pub const AURORA: Self = Self(3);
}

/// What the renderer draws for a kind (append only; the renderer matches every variant).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum WeatherParticles {
    /// Nothing falls.
    #[default]
    None,
    /// Long streaks along the wind.
    Rain,
    /// Slow drifting flakes.
    Snow,
    /// Fine grit blown flat (sandstorms).
    Dust,
    /// Grey flakes falling slowly (ash fall).
    Ash,
    /// Floating motes (spore drift).
    Spores,
    /// Ribbons of light across the night sky (no particles near the ground).
    Aurora,
}

/// How a kind looks.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WeatherLook {
    pub particles: WeatherParticles,
    /// The particles' colour (sRGB).
    pub colour: [u8; 3],
    /// The fog's colour where visibility drops (sRGB).
    pub fog: [u8; 3],
    /// A glow over the sky (aurora ribbons), if any (sRGB).
    pub sky_glow: Option<[u8; 3]>,
}

/// One weather kind (see the module docs).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WeatherKind {
    pub id: WeatherId,
    pub name: &'static str,
    /// The track that registered it (sound names and art are prefixed by it).
    pub track: &'static str,
    /// It needs air to blow (it never comes to an airless body).
    pub needs_air: bool,
    /// It shows only on the night side (an aurora); elsewhere the spell is clear.
    pub night_only: bool,
    /// Sustained wind at full strength (px/s along the ground).
    pub wind: u16,
    /// Extra wind in gusts at full strength (px/s).
    pub gust: u16,
    /// Visibility at full strength (per mille of a clear day's).
    pub visibility_permille: u16,
    /// Light added (or taken, negative) at full strength (per mille).
    pub light_permille: i16,
    /// Temperature change at full strength (°C).
    pub temperature_c: i16,
    /// Hull damage per second to a rig under the open sky at full strength (thousandths).
    pub hull_damage_milli: u16,
    /// Suit damage per second to a pilot under the open sky at full strength (thousandths).
    pub suit_damage_milli: u16,
    /// The pilot cannot breathe the air while it blows (spores, ash, acid).
    pub fouls_air: bool,
    pub look: WeatherLook,
    /// The ambience the game plays while it blows (an audio cue name).
    pub sound: &'static str,
}

impl WeatherKind {
    /// Clear skies.
    pub const CLEAR: Self = Self {
        id: WeatherId::CLEAR,
        name: "Clear",
        track: "bodylife",
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
            colour: [0, 0, 0],
            fog: [0, 0, 0],
            sky_glow: None,
        },
        sound: "",
    };
}

/// The bodylife track's generic kinds: clear skies, storms, snow and auroras.
pub const BODYLIFE_WEATHER: &[WeatherKind] = &[
    WeatherKind::CLEAR,
    WeatherKind {
        id: WeatherId::STORM,
        name: "Storm",
        track: "bodylife",
        needs_air: true,
        night_only: false,
        wind: 150,
        gust: 110,
        visibility_permille: 420,
        light_permille: -260,
        temperature_c: -6,
        // Hail at the storm's height: a slow sting for a rig parked in the open.
        hull_damage_milli: 400,
        suit_damage_milli: 300,
        fouls_air: false,
        look: WeatherLook {
            particles: WeatherParticles::Rain,
            colour: [170, 196, 226],
            fog: [70, 82, 104],
            sky_glow: None,
        },
        sound: "bodylife_rain",
    },
    WeatherKind {
        id: WeatherId::SNOW,
        name: "Snowfall",
        track: "bodylife",
        needs_air: true,
        night_only: false,
        wind: 45,
        gust: 40,
        visibility_permille: 560,
        light_permille: -90,
        temperature_c: -28,
        hull_damage_milli: 0,
        suit_damage_milli: 0,
        fouls_air: false,
        look: WeatherLook {
            particles: WeatherParticles::Snow,
            colour: [240, 246, 255],
            fog: [196, 210, 228],
            sky_glow: None,
        },
        sound: "bodylife_wind",
    },
    WeatherKind {
        id: WeatherId::AURORA,
        name: "Aurora",
        track: "bodylife",
        needs_air: true,
        night_only: true,
        wind: 0,
        gust: 0,
        visibility_permille: 1_000,
        light_permille: 170,
        temperature_c: 0,
        hull_damage_milli: 0,
        suit_damage_milli: 0,
        fouls_air: false,
        look: WeatherLook {
            particles: WeatherParticles::Aurora,
            colour: [120, 255, 190],
            fog: [0, 0, 0],
            sky_glow: Some([110, 240, 170]),
        },
        sound: "bodylife_aurora",
    },
];

/// Every track's weather kinds (append a track's list at the end; ids come from the kinds).
pub const WEATHER_REGISTRY: &[&[WeatherKind]] = &[
    BODYLIFE_WEATHER,
    // Wave 12 planets-a: the downpour, squall and blizzard.
    crate::planet::wilds::hooks::WILDS_WEATHER,
    // Wave 12 planets-b: the sandstorm, ash fall and glitterfrost.
    crate::frontier::hooks::FRONTIER_WEATHER,
    // Wave 13 planets-c: spore drift, glowfall, the ion storm, the grey wind and ghostlight.
    crate::planet::outlands::hooks::OUTLAND_WEATHER,
    // Wave 13 planets-d: the superstorm, nitrogen snow, terminator gale and ring shower.
    crate::exotic::hooks::EXOTIC_WEATHER,
];

/// The kind with id `id` (an unknown id, from a newer build, reads as clear skies).
#[must_use]
pub const fn weather_kind(id: WeatherId) -> &'static WeatherKind {
    let mut list = 0;
    while list < WEATHER_REGISTRY.len() {
        let kinds = WEATHER_REGISTRY[list];
        let mut index = 0;
        while index < kinds.len() {
            if kinds[index].id.0 == id.0 {
                return &kinds[index];
            }
            index += 1;
        }
        list += 1;
    }
    &BODYLIFE_WEATHER[0]
}

/// Every registered kind, in registry order.
pub fn weather_kinds() -> impl Iterator<Item = &'static WeatherKind> {
    WEATHER_REGISTRY.iter().flat_map(|list| list.iter())
}

/// How often a kind blows on a theme's worlds: its weight against the table's others.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WeatherOdds {
    pub kind: WeatherId,
    pub weight: u16,
}

/// A table with clear skies at `clear` and the other kinds' weights.
#[must_use]
pub const fn odds(kind: WeatherId, weight: u16) -> WeatherOdds {
    WeatherOdds { kind, weight }
}

/// Length of one weather spell (milliseconds of world time).
pub const SPELL_MILLIS: u64 = 150_000;
/// A kind fades in (and out) over this long where the spell before (after) differs.
pub const RAMP_MILLIS: u64 = 20_000;
/// Period of a gust (milliseconds).
pub const GUST_MILLIS: u64 = 6_400;

/// The weather of one place at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherNow {
    pub kind: WeatherId,
    /// How strong it is (per mille: fades in and out at the spell's edges).
    pub intensity_permille: u16,
    /// The spell it belongs to (spell index since world time zero).
    pub spell: u64,
    /// Wind along the ground (px/s, positive toward rising columns), gusts included.
    pub wind: f32,
    /// Visibility (per mille of a clear day's).
    pub visibility_permille: u16,
    /// Light added or taken (per mille).
    pub light_permille: i16,
    /// Temperature change (°C).
    pub temperature_c: i16,
    /// Hull damage per second under the open sky.
    pub hull_damage: f32,
    /// Suit damage per second under the open sky.
    pub suit_damage: f32,
    /// The pilot cannot breathe while it blows.
    pub fouls_air: bool,
}

impl WeatherNow {
    /// Clear skies.
    pub const CLEAR: Self = Self {
        kind: WeatherId::CLEAR,
        intensity_permille: 0,
        spell: 0,
        wind: 0.0,
        visibility_permille: 1_000,
        light_permille: 0,
        temperature_c: 0,
        hull_damage: 0.0,
        suit_damage: 0.0,
        fouls_air: false,
    };

    /// The registered kind.
    #[must_use]
    pub const fn info(&self) -> &'static WeatherKind {
        weather_kind(self.kind)
    }

    /// Whether anything is blowing.
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.kind == WeatherId::CLEAR || self.intensity_permille == 0
    }

    /// A share of the intensity as a float (`0.0..=1.0`, presentation and physics).
    #[must_use]
    pub fn intensity(&self) -> f32 {
        f32::from(self.intensity_permille) / 1_000.0
    }
}

/// A splitmix64 step.
const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// How strongly wind pushes in `atmosphere` (per mille: thin air pushes less, thick more).
#[must_use]
pub const fn air_push_permille(atmosphere: Atmosphere) -> u32 {
    match atmosphere {
        Atmosphere::Vacuum => 0,
        Atmosphere::Thin => 550,
        Atmosphere::Breathable | Atmosphere::Spores | Atmosphere::Toxic => 1_000,
        Atmosphere::Thick => 1_400,
        Atmosphere::Scalding => 1_600,
    }
}

/// The kinds of `table` that can blow in `atmosphere`, with their weights.
fn usable(table: &[WeatherOdds], atmosphere: Atmosphere) -> Vec<WeatherOdds> {
    table
        .iter()
        .copied()
        .filter(|entry| {
            entry.weight > 0
                && (entry.kind == WeatherId::CLEAR
                    || !weather_kind(entry.kind).needs_air
                    || atmosphere != Atmosphere::Vacuum)
        })
        .collect()
}

/// The kind of spell `spell` on the body seeded `seed` (clear when the table is empty).
#[must_use]
pub fn spell_kind(
    seed: u64,
    table: &[WeatherOdds],
    atmosphere: Atmosphere,
    spell: u64,
) -> WeatherId {
    let usable = usable(table, atmosphere);
    let total: u64 = usable.iter().map(|entry| u64::from(entry.weight)).sum();
    if total == 0 {
        return WeatherId::CLEAR;
    }
    let mut roll = mix(mix(seed ^ 0x5745_4154_4845_5221) ^ spell) % total;
    for entry in &usable {
        let weight = u64::from(entry.weight);
        if roll < weight {
            return entry.kind;
        }
        roll -= weight;
    }
    WeatherId::CLEAR
}

/// The weather at world time `millis` on the body seeded `seed` with weather table `table`
/// and air `atmosphere`, at a place whose sky is dark (`night`: night-only kinds show).
#[must_use]
pub fn forecast(
    seed: u64,
    table: &[WeatherOdds],
    atmosphere: Atmosphere,
    millis: u64,
    night: bool,
) -> WeatherNow {
    let spell = millis / SPELL_MILLIS;
    let into = millis % SPELL_MILLIS;
    let kind = spell_kind(seed, table, atmosphere, spell);
    if kind == WeatherId::CLEAR {
        return WeatherNow {
            spell,
            ..WeatherNow::CLEAR
        };
    }
    let registered = weather_kind(kind);
    if registered.night_only && !night {
        return WeatherNow {
            spell,
            ..WeatherNow::CLEAR
        };
    }
    // Fade in where the spell before differs, out where the one after does.
    let mut intensity: u64 = 1_000;
    if spell == 0 || spell_kind(seed, table, atmosphere, spell - 1) != kind {
        intensity = intensity.min(into * 1_000 / RAMP_MILLIS);
    }
    if spell_kind(seed, table, atmosphere, spell + 1) != kind {
        intensity = intensity.min((SPELL_MILLIS - into) * 1_000 / RAMP_MILLIS);
    }
    #[allow(clippy::cast_possible_truncation, reason = "at most 1,000")]
    let intensity = intensity.min(1_000) as u16;
    let scale = |value: u16| -> u32 { u32::from(value) * u32::from(intensity) / 1_000 };
    // The wind blows one way for the spell; gusts swell and ebb as a triangle wave.
    let direction = if mix(seed ^ spell.wrapping_mul(0x9E37)) & 1 == 0 {
        1.0
    } else {
        -1.0
    };
    let phase = (millis + (mix(seed ^ spell) % GUST_MILLIS)) % GUST_MILLIS;
    let half = GUST_MILLIS / 2;
    let surge = if phase < half {
        phase * 1_000 / half
    } else {
        (GUST_MILLIS - phase) * 1_000 / half
    };
    let push = air_push_permille(atmosphere);
    let wind_milli = (u64::from(scale(registered.wind)) * 1_000
        + u64::from(scale(registered.gust)) * surge)
        * u64::from(push)
        / 1_000;
    #[allow(
        clippy::cast_precision_loss,
        reason = "a few hundred px/s in thousandths"
    )]
    let wind = direction * (wind_milli as f32 / 1_000.0);
    let lerp_down = |full: u16| -> u16 {
        // From a clear day's 1,000 toward `full`.
        let drop = u32::from(1_000_u16.saturating_sub(full)) * u32::from(intensity) / 1_000;
        #[allow(clippy::cast_possible_truncation, reason = "at most 1,000")]
        let value = 1_000 - drop as u16;
        value
    };
    let signed = |full: i16| -> i16 {
        #[allow(clippy::cast_possible_truncation, reason = "bounded by `full`")]
        let value = (i32::from(full) * i32::from(intensity) / 1_000) as i16;
        value
    };
    #[allow(clippy::cast_precision_loss, reason = "thousandths of a hull point")]
    let damage = |milli: u16| -> f32 { scale(milli) as f32 / 1_000.0 };
    WeatherNow {
        kind,
        intensity_permille: intensity,
        spell,
        wind: crate::quantize::quantize_simulation_scalar(wind),
        visibility_permille: lerp_down(registered.visibility_permille),
        light_permille: signed(registered.light_permille),
        temperature_c: signed(registered.temperature_c),
        hull_damage: damage(registered.hull_damage_milli),
        suit_damage: damage(registered.suit_damage_milli),
        fouls_air: registered.fouls_air && intensity >= 500,
    }
}

/// The world time of the next spell at or after `from` whose kind is `kind` (scenes, tests
/// and the HUD's outlook), looking at most `spells` spells ahead.
#[must_use]
pub fn next_spell_of(
    seed: u64,
    table: &[WeatherOdds],
    atmosphere: Atmosphere,
    kind: WeatherId,
    from: u64,
    spells: u64,
) -> Option<u64> {
    let first = from / SPELL_MILLIS;
    (first..first + spells)
        .find(|spell| spell_kind(seed, table, atmosphere, *spell) == kind)
        .map(|spell| (spell * SPELL_MILLIS).max(from))
}
