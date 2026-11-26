//! A body's climate (Wave 12 bodylife): temperature, pressure and air by place and depth.
//!
//! On a body these replace the home world's band heat and altitude cold:
//!
//! * **Temperature** starts from the body's mean surface temperature (the space generator's:
//!   its star's heat at its distance plus its theme and air), swings between day and night by
//!   how much air holds the heat ([`day_swing_c`]: bare rock bakes and freezes, a thick sky
//!   barely moves; a tidally locked body's sides swing furthest), adds the weather's
//!   change, and underground settles toward the mean within [`SKIN_ROWS`] then warms toward the
//!   core ([`BodyClimate::core_c`]) along the body's depth.
//! * **Heat and cold** for a rig follow from it: every [`HEAT_STEP_C`] over [`HEAT_START_C`] is
//!   a heat level (the radiator and heat shielding cover them as at home), every
//!   [`COLD_STEP_C`] under [`COLD_START_C`] a cold level (the de-icer covers them).
//! * **Pressure** (kPa) is the air's at the surface, scaled by gravity, and rises underground
//!   in a body with air. It is a reading (the HUD's), not a hazard of its own.
//! * **Air**: a pilot breathes only a breathable atmosphere, and not while the weather fouls
//!   it ([`super::weather::WeatherNow::fouls_air`]); the rig's life support reads the same.

use crate::{
    environment::MAX_COLD,
    protection::{HEAT_UNITS_PER_LEVEL, MAX_BAND_HEAT},
    space::Atmosphere,
};

/// Underground the day and night swing fades out over this many rows.
pub const SKIN_ROWS: i32 = 24;
/// Temperature (°C) from which the heat starts to count.
pub const HEAT_START_C: i32 = 70;
/// °C per heat level past [`HEAT_START_C`].
pub const HEAT_STEP_C: i32 = 45;
/// Temperature (°C) under which the cold starts to count.
pub const COLD_START_C: i32 = -40;
/// °C per cold level under [`COLD_START_C`].
pub const COLD_STEP_C: i32 = 22;

/// What a body's climate is made of (the space body's numbers, see the module docs).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BodyClimate {
    /// Mean surface temperature (°C).
    pub mean_c: i16,
    pub atmosphere: Atmosphere,
    /// Surface gravity (milli-g).
    pub gravity_milli: u16,
    /// Rows from the surface to the core chamber.
    pub depth_rows: i32,
    /// Whether the body is tidally locked (its sides swing further).
    pub tidally_locked: bool,
    /// The theme's own heat underground (°C added at the core; themes tune it).
    pub core_bias_c: i16,
}

/// How far day and night swing from the mean in `atmosphere` (°C, half the full swing).
#[must_use]
pub const fn day_swing_c(atmosphere: Atmosphere) -> i32 {
    match atmosphere {
        Atmosphere::Vacuum => 85,
        Atmosphere::Thin => 38,
        Atmosphere::Breathable | Atmosphere::Spores => 11,
        Atmosphere::Toxic => 14,
        Atmosphere::Thick => 5,
        Atmosphere::Scalding => 4,
    }
}

/// Surface pressure of `atmosphere` at one standard gravity (kPa).
#[must_use]
pub const fn surface_pressure_kpa(atmosphere: Atmosphere) -> u32 {
    match atmosphere {
        Atmosphere::Vacuum => 0,
        Atmosphere::Thin => 28,
        Atmosphere::Breathable => 101,
        Atmosphere::Spores => 112,
        Atmosphere::Toxic => 160,
        Atmosphere::Thick => 480,
        Atmosphere::Scalding => 920,
    }
}

/// A reading of the climate at one place.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClimateReading {
    pub temperature_c: i32,
    pub pressure_kpa: u32,
    /// A pilot breathes here without the suit's air.
    pub breathable: bool,
    /// Heat units (radiator and shielding cover them, [`HEAT_UNITS_PER_LEVEL`] a level).
    pub heat_units: u8,
    /// Cold levels (the de-icer covers them).
    pub cold: u8,
}

impl BodyClimate {
    /// The temperature at the core chamber (°C): the mean plus a body's inner heat (bigger
    /// and heavier bodies keep more of it), plus the theme's own.
    #[must_use]
    pub fn core_c(&self) -> i32 {
        let depth = self.depth_rows.max(1);
        let inner = 120 + depth / 6 + i32::from(self.gravity_milli) / 8;
        i32::from(self.mean_c) + inner.min(900) + i32::from(self.core_bias_c)
    }

    /// The temperature `depth` rows below the surface (negative: above it, in the sky) where
    /// the sun stands at `elevation` (its sine, see [`super::clock::BodyClock::elevation`]),
    /// with the weather's change `weather_c`.
    #[must_use]
    pub fn temperature_at(&self, depth: i32, elevation: f64, weather_c: i32) -> i32 {
        let mut swing = day_swing_c(self.atmosphere);
        if self.tidally_locked {
            swing = swing * 3 / 2;
        }
        // The swing in whole °C (elevation is -1..=1).
        #[allow(clippy::cast_possible_truncation, reason = "at most a few hundred °C")]
        let day = (f64::from(swing) * elevation.clamp(-1.0, 1.0)).round() as i32;
        let mean = i32::from(self.mean_c);
        if depth <= 0 {
            return mean + day + weather_c;
        }
        // The swing and the weather fade out through the skin of the ground...
        let skin = (SKIN_ROWS - depth).max(0);
        let surface = mean + (day + weather_c) * skin / SKIN_ROWS;
        // ...and the inner heat rises toward the core (quadratic: cool for a long way, then
        // the deep heat comes on).
        let depth_rows = self.depth_rows.max(1);
        let along = i64::from(depth.min(depth_rows));
        let span = i64::from(depth_rows);
        let rise = i64::from(self.core_c() - mean) * along * along / (span * span);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "bounded by the core's heat"
        )]
        let rise = rise as i32;
        surface + rise
    }

    /// Pressure `depth` rows below the surface (kPa).
    #[must_use]
    pub fn pressure_at(&self, depth: i32) -> u32 {
        let base = surface_pressure_kpa(self.atmosphere) * u32::from(self.gravity_milli) / 1_000;
        if base == 0 {
            return 0;
        }
        let depth = depth.clamp(0, self.depth_rows.max(0)).cast_unsigned();
        base + base * depth / 400
    }

    /// Heat units at `temperature_c` (see the module docs).
    #[must_use]
    pub fn heat_units(temperature_c: i32) -> u8 {
        if temperature_c <= HEAT_START_C {
            return 0;
        }
        let over = temperature_c - HEAT_START_C;
        let units = over * i32::from(HEAT_UNITS_PER_LEVEL) / HEAT_STEP_C;
        let cap = i32::from(MAX_BAND_HEAT) * i32::from(HEAT_UNITS_PER_LEVEL);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "capped"
        )]
        let units = units.clamp(0, cap) as u8;
        units
    }

    /// Cold levels at `temperature_c`.
    #[must_use]
    pub fn cold_levels(temperature_c: i32) -> u8 {
        if temperature_c >= COLD_START_C {
            return 0;
        }
        let under = COLD_START_C - temperature_c;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "capped"
        )]
        let levels = (1 + under / COLD_STEP_C).clamp(0, i32::from(MAX_COLD)) as u8;
        levels
    }

    /// The whole reading `depth` rows down under a sun at `elevation` with the weather's
    /// temperature change and whether it fouls the air.
    #[must_use]
    pub fn reading(
        &self,
        depth: i32,
        elevation: f64,
        weather_c: i32,
        fouled: bool,
    ) -> ClimateReading {
        let temperature_c = self.temperature_at(depth, elevation, weather_c);
        ClimateReading {
            temperature_c,
            pressure_kpa: self.pressure_at(depth),
            breathable: self.atmosphere.breathable() && !fouled,
            heat_units: Self::heat_units(temperature_c),
            cold: Self::cold_levels(temperature_c),
        }
    }
}
