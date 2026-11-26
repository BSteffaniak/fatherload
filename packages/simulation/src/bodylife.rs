//! Living bodies (Wave 12 bodylife, expansion plan §16 WP-SPACE): every landable body's own
//! clock and day length, weather, climate, air, gravity and economy.
//!
//! A [`BodyLife`] gathers what makes a body a place of its own, as a pure function of the
//! world seed and the body (the space generator's [`SpaceBody`] and its system), so nothing of
//! it is stored, saved or replicated, and every peer reads the same:
//!
//! * [`clock`]: the body's day (tidally locked bodies keep an eternal day and night side), the
//!   local hour and daylight at every column, the star's place in the sky;
//! * [`weather`]: the weather registry (theme tracks register their kinds) and the body's
//!   forecast, with wind, visibility, light, cold and hazards;
//! * [`climate`]: temperature from the star, the air and the depth, pressure and breathable
//!   air, which replace the home world's band heat and altitude cold on a body;
//! * [`market`]: the body's price model and its settlements' local markets (credits stay
//!   universal);
//! * gravity: the body's own, the one number its ground's field zone, flight's approach and
//!   the HUD share ([`BodyLife::gravity_milli`]).
//!
//! The world rules that read it live in `world/bodylife.rs` ([`crate::WorldState::body_life`]).

pub mod climate;
pub mod clock;
pub mod market;
#[cfg(test)]
mod tests;
pub mod weather;

use crate::{
    planet::{PlanetGround, PlanetParams, PlanetThemeHooks, hooks::LockedSides},
    space::{Atmosphere, SpaceBody, SpaceBodyId, SpaceSector},
};

pub use climate::{BodyClimate, ClimateReading};
pub use clock::BodyClock;
pub use market::{BodyPrices, LocalMarket};
pub use weather::{WeatherId, WeatherKind, WeatherNow, forecast, weather_kind};

/// Day length (seconds) of a planet world that has no space body behind it (the polar
/// track's own planet worlds, tests and tools).
pub const LONE_PLANET_DAY_SECONDS: u32 = 1_200;

/// A body's life (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyLife {
    /// The body (`None` for a lone planet world with no space behind it).
    pub id: Option<SpaceBodyId>,
    /// Its ground's params (seed, radius, gravity, theme).
    pub params: PlanetParams,
    pub clock: BodyClock,
    pub climate: BodyClimate,
    pub prices: BodyPrices,
    /// Its seed for weather and markets (the ground seed).
    pub seed: u64,
}

/// The weather, climate and light at one place of a body at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyReading {
    /// Local time (thousandths of a day from midnight).
    pub local_permille: u32,
    /// Local hours and minutes.
    pub hours_minutes: (u8, u8),
    /// Daylight with the weather's change (per mille), before lamps.
    pub light_permille: u16,
    /// The sun's elevation (sine).
    pub elevation: f64,
    pub weather: WeatherNow,
    pub climate: ClimateReading,
    /// The place is under the open sky (weather reaches it).
    pub exposed: bool,
}

/// The mean a tidally locked body of a theme with [`LockedSides`] holds its terminator to
/// (the body's own `mean_c` anywhere else).
fn locked_mean_c(hooks: &dyn PlanetThemeHooks, tidally_locked: bool, mean_c: i16) -> i16 {
    match hooks.locked_sides() {
        Some(LockedSides {
            terminator_c: (low, high),
            ..
        }) if tidally_locked => mean_c.clamp(low, high),
        _ => mean_c,
    }
}

/// The least side swing of a tidally locked body of a theme with [`LockedSides`] (0 else).
fn locked_swing_c(hooks: &dyn PlanetThemeHooks, tidally_locked: bool) -> i16 {
    hooks
        .locked_sides()
        .filter(|_| tidally_locked)
        .map_or(0, |sides| sides.swing_c)
}

/// Galactic sectors (Chebyshev) from home to `sector`.
#[must_use]
pub fn sectors_from_home(sector: SpaceSector) -> u32 {
    let home = crate::space::HOME_SECTOR;
    let dx = (i64::from(sector.x) - i64::from(home.x)).unsigned_abs();
    let dy = (i64::from(sector.y) - i64::from(home.y)).unsigned_abs();
    u32::try_from(dx.max(dy)).unwrap_or(u32::MAX)
}

impl BodyLife {
    /// The life of space body `body` (its id `id`) whose ground is `params`.
    #[must_use]
    pub fn of_space_body(id: SpaceBodyId, body: &SpaceBody, params: PlanetParams) -> Self {
        let ground = PlanetGround::new(params);
        let hooks = ground.hooks();
        let depth_rows = ground.core_chamber_top().max(1);
        Self {
            id: Some(id),
            params,
            clock: BodyClock::new(
                body.day_seconds,
                ground.grid().surface_columns(),
                params.seed,
            ),
            climate: BodyClimate {
                mean_c: locked_mean_c(hooks, body.tidally_locked(), body.temperature_c),
                atmosphere: body.atmosphere,
                gravity_milli: params.gravity_milli,
                depth_rows,
                tidally_locked: body.tidally_locked(),
                core_bias_c: hooks.climate().core_bias_c,
                side_swing_c: locked_swing_c(hooks, body.tidally_locked()),
            },
            prices: BodyPrices::new(
                params.seed,
                &crate::planet::hooks::common_minerals(params.theme),
                sectors_from_home(id.sector),
            ),
            seed: params.seed,
        }
    }

    /// The life of a lone planet world (no space body: the theme's typical numbers).
    #[must_use]
    pub fn of_planet(params: PlanetParams) -> Self {
        let ground = PlanetGround::new(params);
        let hooks = ground.hooks();
        let (mean_c, atmosphere) = lone_climate(params);
        // A theme built on a locked body's two sides keeps them on a lone world too.
        let locked = hooks.locked_sides().is_some();
        Self {
            id: None,
            params,
            clock: BodyClock::new(
                if locked { 0 } else { LONE_PLANET_DAY_SECONDS },
                ground.grid().surface_columns(),
                params.seed,
            ),
            climate: BodyClimate {
                mean_c: locked_mean_c(hooks, locked, mean_c),
                atmosphere,
                gravity_milli: params.gravity_milli,
                depth_rows: ground.core_chamber_top().max(1),
                tidally_locked: locked,
                core_bias_c: hooks.climate().core_bias_c,
                side_swing_c: locked_swing_c(hooks, locked),
            },
            prices: BodyPrices::new(
                params.seed,
                &crate::planet::hooks::common_minerals(params.theme),
                0,
            ),
            seed: params.seed,
        }
    }

    /// The theme's hooks.
    #[must_use]
    pub fn hooks(&self) -> &'static dyn PlanetThemeHooks {
        crate::planet::hooks(self.params.theme)
    }

    /// The body's gravity (milli-g, within the ground's playable range): the one number its
    /// ground's field zone, flight's approach and the HUD agree on.
    #[must_use]
    pub fn gravity_milli(&self) -> u16 {
        effective_gravity_milli(self.params.gravity_milli)
    }

    /// Daylight on its night side (per mille): starlight on bare rock, a dim glow under air.
    #[must_use]
    pub const fn night_light(&self) -> u16 {
        if matches!(self.climate.atmosphere, Atmosphere::Vacuum) {
            clock::AIRLESS_NIGHT_LIGHT
        } else {
            clock::AIR_NIGHT_LIGHT
        }
    }

    /// The weather at strip `column` at world time `millis`.
    #[must_use]
    pub fn weather(&self, millis: u64, column: i64) -> WeatherNow {
        self.weather_with(millis, column, None)
    }

    /// [`Self::weather`], or `forced` blowing at full strength all the time (the test-mode
    /// weather switch, Wave 14): the same forecast over a table of that one kind, so it still
    /// needs air and a night-only kind still shows only at night.
    #[must_use]
    pub fn weather_with(&self, millis: u64, column: i64, forced: Option<WeatherId>) -> WeatherNow {
        let night = self.clock.daylight(millis, column, 0) < 300;
        let only = forced.map(|kind| [weather::odds(kind, 1)]);
        let table = only
            .as_ref()
            .map_or_else(|| self.hooks().weather_table(), |only| &only[..]);
        forecast(self.seed, table, self.climate.atmosphere, millis, night)
    }

    /// Everything at strip tile `(column, row)` at world time `millis`, `surface` being the
    /// ground's top row there and `exposed` whether the open sky is over it.
    #[must_use]
    pub fn reading(
        &self,
        millis: u64,
        column: i64,
        row: i32,
        surface: i32,
        exposed: bool,
    ) -> BodyReading {
        self.reading_with(millis, column, row, surface, exposed, None)
    }

    /// [`Self::reading`] under `forced` weather (see [`Self::weather_with`]).
    #[must_use]
    pub fn reading_with(
        &self,
        millis: u64,
        column: i64,
        row: i32,
        surface: i32,
        exposed: bool,
        forced: Option<WeatherId>,
    ) -> BodyReading {
        let elevation = self.clock.elevation(millis, column);
        let weather = self.weather_with(millis, column, forced);
        let reach = if exposed { weather } else { WeatherNow::CLEAR };
        let daylight = i32::from(self.clock.daylight(millis, column, self.night_light()));
        // Weather darkens the day; an aurora lights the night.
        let light = (daylight + i32::from(weather.light_permille)).clamp(60, 1_000);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped"
        )]
        let light = light as u16;
        let depth = row - surface;
        let climate = self.climate.reading(
            depth,
            elevation,
            i32::from(weather.temperature_c),
            weather.fouls_air,
        );
        BodyReading {
            local_permille: self.clock.local_permille(millis, column),
            hours_minutes: self.clock.hours_minutes(millis, column),
            light_permille: light,
            elevation,
            weather: reach,
            climate,
            exposed,
        }
    }

    /// A settlement's local market at strip `column` (its id `settlement`) at world time
    /// `millis`: its boards turn over at local midnight.
    #[must_use]
    pub fn market(&self, settlement: u64, millis: u64, column: i64) -> LocalMarket {
        LocalMarket::new(
            self.prices.clone(),
            settlement ^ self.seed,
            self.clock.local_day(millis, column),
        )
    }
}

/// A gravity (milli-g) brought into the range a body's ground plays with
/// ([`crate::planet::MIN_PLANET_GRAVITY`]..=[`crate::planet::MAX_PLANET_GRAVITY`]).
#[must_use]
pub fn effective_gravity_milli(milli: u16) -> u16 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the planet gravity range in thousandths"
    )]
    let (low, high) = (
        (crate::planet::MIN_PLANET_GRAVITY * 1_000.0).round() as u16,
        (crate::planet::MAX_PLANET_GRAVITY * 1_000.0).round() as u16,
    );
    milli.clamp(low, high)
}

/// The typical climate of a theme's world (a lone planet world with no space body).
const fn lone_climate(params: PlanetParams) -> (i16, Atmosphere) {
    use crate::planet::PlanetThemeId;
    match params.theme {
        PlanetThemeId::ROCKY => (18, Atmosphere::Breathable),
        PlanetThemeId::ICE => (-55, Atmosphere::Thin),
        _ => (-20, Atmosphere::Vacuum),
    }
}
