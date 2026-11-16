//! Altitude physics (expansion plan F6): the per-location environment that replaces the
//! movement and physics constants in the sky of [`GeneratorVersion::V8`] worlds.
//!
//! [`environment_at`] answers, for any point of the world, an [`Environment`]:
//!
//! * **Air density** falls with altitude ([`air_density_profile`]): full up to
//!   [`FULL_AIR_ROWS`], then a long hyperbolic thinning, and none at all from the orbit band up.
//!   A rig's **thrust efficiency** ([`Environment::thrust_efficiency`]) is the air density plus
//!   what its **high-altitude intakes** recover of the rest ([`intake_recovery`], the
//!   Altitude Intake upgrade line). Efficiency scales thrust and lift, the climb-speed cap and
//!   fuel burn (thin air runs an engine rich). Thinner air also drags less: vertical and
//!   horizontal braking scale with density, so the orbit band drifts with heavy inertia, and
//!   falls through thin air run faster before the air thickens again. (Aether thrusters, a
//!   sky-crafted engine part, are reserved for Wave 4.)
//! * **Cold** builds up with altitude ([`cold_level`]), the reverse of band heat: each level
//!   the rig's de-icer ([`cold_cover`], the De-Icer upgrade line) does not cover burns
//!   [`COLD_DAMAGE_PER_EXCESS_LEVEL`] hull per second.
//! * **Wind layers** ([`wind_at`]): bands of moving air every [`WIND_LAYER_ROWS`] rows above
//!   [`WIND_FLOOR_ROWS`], deterministic per seed, blowing harder in the dead-air gap's
//!   crosswinds. An airborne rig moves relative to the air, so it coasts with the wind and
//!   riding a layer is a free ride sideways.
//! * **Updrafts** ([`crate::worldgen::sky::updraft_at`]): columns of rising air in the island
//!   and continent bands, a free ride up.
//! * **Gravity fields** ([`gravity_scale`]): normal on the ground and up through the island
//!   bands, [`CONTINENT_GRAVITY`] in the sky continent band, near zero ([`ORBIT_GRAVITY`]) in
//!   the orbit band, and [`MOON_GRAVITY`] on and around the Moon. Gravity applies to rigs,
//!   drops, charges (gravity bombs arc far on the Moon and float in orbit), boulders, enemies
//!   and, through [`Environment::gravity`], the pilot on foot (T15 wires its own movement to
//!   it).
//!
//! Everything is a pure function of the world settings and the position, using only basic
//! arithmetic (no transcendental functions), so hosts, predicting clients and every platform
//! agree bit for bit. Worlds of older generators get [`Environment::STANDARD`] everywhere, and
//! so does every point below [`STANDARD_BELOW_ROWS`] (the ground, the underground): ground
//! gameplay is unchanged bit for bit.
//!
//! [`GeneratorVersion::V8`]: crate::worldgen::GeneratorVersion::V8

use crate::{
    sky::SkyBand,
    terrain::TILE_SIZE,
    worldgen::{
        WorldSettings,
        noise::{NOISE_MAX, hash2, line_noise},
        sky::{
            ARCHIPELAGO_BOTTOM_ROWS, CONTINENTS_BOTTOM_ROWS, DEAD_AIR_BOTTOM_ROWS,
            MOON_BOTTOM_ROWS, ORBIT_BOTTOM_ROWS, jet_stream_at, moon, updraft_at, vent_updraft_at,
        },
    },
};

/// The air and gravity at one point of the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    /// Gravity relative to the ground's (1.0): multiply any body's gravity constant by it.
    pub gravity: f32,
    /// Air density relative to the ground's, `0.0..=1.0`.
    pub air_density: f32,
    /// Cold level, from 0 to [`MAX_COLD`].
    pub cold: u8,
    /// Velocity of the air (px/s, positive to the right): an airborne rig coasts toward it.
    pub wind_x: f32,
    /// Upward acceleration of an updraft (px/s²; 0 outside updrafts).
    pub updraft: f32,
}

impl Environment {
    /// The ground's environment: full air, normal gravity, no cold, wind or updrafts.
    pub const STANDARD: Self = Self {
        gravity: 1.0,
        air_density: 1.0,
        cold: 0,
        wind_x: 0.0,
        updraft: 0.0,
    };

    /// Whether this is exactly [`Self::STANDARD`] (movement then takes the classic path).
    #[must_use]
    #[allow(clippy::float_cmp, reason = "the standard values are exact constants")]
    pub fn is_standard(&self) -> bool {
        self.gravity == 1.0
            && self.air_density == 1.0
            && self.cold == 0
            && self.wind_x == 0.0
            && self.updraft == 0.0
    }

    /// A rig's thrust efficiency here with altitude intakes of `intake_level`: the air
    /// density plus the share of the missing air the intakes recover.
    #[must_use]
    pub fn thrust_efficiency(&self, intake_level: u8) -> f32 {
        (1.0 - self.air_density).mul_add(intake_recovery(intake_level), self.air_density)
    }

    /// Multiplier on a rig's climb-speed cap at thrust efficiency `efficiency`: less thrust
    /// climbs slower, thinner air drags less.
    #[must_use]
    pub fn climb_cap_factor(&self, efficiency: f32) -> f32 {
        efficiency.mul_add(0.7, 0.3) * (1.0 - self.air_density).mul_add(THIN_AIR_CLIMB_BONUS, 1.0)
    }

    /// Multiplier on fuel burn at thrust efficiency `efficiency` (thin air runs rich).
    #[must_use]
    pub fn burn_factor(efficiency: f32) -> f32 {
        (1.0 - efficiency).mul_add(THIN_AIR_BURN, 1.0)
    }

    /// Multiplier on the terminal fall speed (thin air drags less).
    #[must_use]
    pub fn fall_cap_factor(&self) -> f32 {
        (1.0 - self.air_density).mul_add(THIN_AIR_FALL_BONUS, 1.0)
    }

    /// The per-tick velocity retention of a drag whose standard retention is `standard`, in
    /// air of this density (first order in the drag: no drag at all in vacuum).
    #[must_use]
    pub fn drag_retention(&self, standard: f32) -> f32 {
        (1.0 - standard).mul_add(-self.air_density, 1.0)
    }

    /// The net downward acceleration (px/s²) of a body whose ground gravity is `gravity`.
    #[must_use]
    pub fn body_gravity(&self, gravity: f32) -> f32 {
        gravity.mul_add(self.gravity, -self.updraft)
    }
}

/// Air is full density up to this altitude (rows above row 0), above the tallest peaks.
pub const FULL_AIR_ROWS: f32 = 200.0;
/// Scale of the air's thinning: `FULL_AIR_ROWS + AIR_SCALE_ROWS` rows up, the air is half as
/// dense.
pub const AIR_SCALE_ROWS: f32 = 600.0;
/// Rows over which the air fades to vacuum, centred on the orbit band's floor.
pub const VACUUM_FADE_ROWS: f32 = 240.0;
/// How much faster a rig may climb in vacuum than in full air (at full efficiency).
pub const THIN_AIR_CLIMB_BONUS: f32 = 2.2;
/// Extra fuel burn per unit of lost thrust efficiency.
pub const THIN_AIR_BURN: f32 = 8.0;
/// How much faster a rig may fall in vacuum than in full air.
pub const THIN_AIR_FALL_BONUS: f32 = 2.0;
/// Below this altitude (rows above row 0) the environment is always standard.
pub const STANDARD_BELOW_ROWS: i32 = 120;

/// Top Altitude Intake tier.
pub const ALTITUDE_INTAKE_MAX_LEVEL: u8 = 6;
/// Share of the missing air each intake tier recovers.
pub const INTAKE_RECOVERY: [f32; 7] = [0.0, 0.1, 0.18, 0.26, 0.33, 0.4, 0.48];

/// Share of the missing air altitude intakes of `level` recover (0 without intakes).
#[must_use]
pub fn intake_recovery(level: u8) -> f32 {
    INTAKE_RECOVERY[usize::from(level.min(ALTITUDE_INTAKE_MAX_LEVEL))]
}

/// Top De-Icer tier.
pub const DE_ICER_MAX_LEVEL: u8 = 6;
/// The coldest cold level (the Moon).
pub const MAX_COLD: u8 = 10;
/// Cold levels each de-icer tier covers.
pub const COLD_COVER_PER_DE_ICER: u8 = 2;
/// Hull damage per second for each cold level the de-icer does not cover.
pub const COLD_DAMAGE_PER_EXCESS_LEVEL: f32 = 1.0;

/// Cold levels a de-icer of `level` covers (the insulation hook: suits, hull parts and sky
/// gear can add to it later).
#[must_use]
pub const fn cold_cover(de_icer: u8) -> u8 {
    de_icer.saturating_mul(COLD_COVER_PER_DE_ICER)
}

/// Cold levels at `cold` the de-icer does not cover.
#[must_use]
pub const fn cold_excess(cold: u8, de_icer: u8) -> u8 {
    cold.saturating_sub(cold_cover(de_icer))
}

/// Gravity in the sky continent band (slightly lighter than the ground).
pub const CONTINENT_GRAVITY: f32 = 0.85;
/// Gravity in the orbit band (near zero: drifting, inertia-heavy flight).
pub const ORBIT_GRAVITY: f32 = 0.04;
/// Gravity on and around the Moon.
pub const MOON_GRAVITY: f32 = 0.3;
/// Columns beyond the Moon's edge its gravity still reaches (fading out).
pub const MOON_HALO_COLUMNS: i32 = 240;
/// Rows under the Moon's lowest point its gravity reaches (fading out).
pub const MOON_HALO_ROWS: i32 = 80;

/// Wind blows only above this altitude (rows above row 0).
pub const WIND_FLOOR_ROWS: f32 = 160.0;
/// Height of one wind layer.
pub const WIND_LAYER_ROWS: i32 = 64;
const SALT_WIND: u64 = 0x5C1E_0030;
const SALT_GUST: u64 = 0x5C1E_0031;

/// Linear interpolation of `from..to` at `t` in `0..=1`.
fn lerp(from: f32, to: f32, t: f32) -> f32 {
    (to - from).mul_add(t.clamp(0.0, 1.0), from)
}

/// Air density at `altitude_rows` (rows above row 0) before the vacuum of orbit.
///
/// Full up to [`FULL_AIR_ROWS`], then `AIR_SCALE_ROWS / (AIR_SCALE_ROWS + rows above it)`. The
/// climb harness measures in this profile.
#[must_use]
pub fn air_density_profile(altitude_rows: f32) -> f32 {
    let above = altitude_rows - FULL_AIR_ROWS;
    if above <= 0.0 {
        return 1.0;
    }
    AIR_SCALE_ROWS / (AIR_SCALE_ROWS + above)
}

/// Air density at `altitude_rows` in a V8 sky: the profile, fading to vacuum across the orbit
/// band's floor.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "band rows are small integers")]
pub fn air_density(altitude_rows: f32) -> f32 {
    let orbit = ORBIT_BOTTOM_ROWS as f32;
    let fade =
        ((orbit + VACUUM_FADE_ROWS / 2.0 - altitude_rows) / VACUUM_FADE_ROWS).clamp(0.0, 1.0);
    air_density_profile(altitude_rows) * fade
}

/// Cold level at `altitude_rows`: none below the cloud archipelago, rising through it and the
/// dead-air gap, bitter in the continent band, and [`MAX_COLD`] in orbit and on the Moon.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "band rows are small integers and the level is clamped to 0..=10"
)]
pub fn cold_level(altitude_rows: f32) -> u8 {
    let (arch, gap, continents, orbit) = (
        ARCHIPELAGO_BOTTOM_ROWS as f32,
        DEAD_AIR_BOTTOM_ROWS as f32,
        CONTINENTS_BOTTOM_ROWS as f32,
        ORBIT_BOTTOM_ROWS as f32,
    );
    let level = if altitude_rows < arch {
        0.0
    } else if altitude_rows < gap {
        lerp(0.0, 4.0, (altitude_rows - arch) / (gap - arch))
    } else if altitude_rows < continents {
        lerp(4.0, 7.0, (altitude_rows - gap) / (continents - gap))
    } else if altitude_rows < orbit {
        lerp(
            7.0,
            8.0,
            (altitude_rows - continents) / (orbit - continents),
        )
    } else {
        lerp(9.0, 10.0, (altitude_rows - orbit) / 400.0)
    };
    (level.floor() as u8).min(MAX_COLD)
}

/// Gravity relative to the ground at column `x` (tile) and `altitude_rows`.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "band rows and columns are small integers"
)]
pub fn gravity_scale(settings: &WorldSettings, x: i32, altitude_rows: f32) -> f32 {
    let continents = CONTINENTS_BOTTOM_ROWS as f32;
    let orbit = ORBIT_BOTTOM_ROWS as f32;
    let mut gravity = if altitude_rows < continents - 120.0 {
        1.0
    } else if altitude_rows < continents {
        lerp(
            1.0,
            CONTINENT_GRAVITY,
            (altitude_rows - (continents - 120.0)) / 120.0,
        )
    } else if altitude_rows < orbit {
        CONTINENT_GRAVITY
    } else {
        lerp(
            CONTINENT_GRAVITY,
            ORBIT_GRAVITY,
            (altitude_rows - orbit) / 160.0,
        )
    };
    let moon_floor = (MOON_BOTTOM_ROWS - MOON_HALO_ROWS) as f32;
    if altitude_rows > moon_floor
        && let Some(moon) = moon(settings)
    {
        let outside = ((x - moon.center_x).abs() - moon.half_width).max(0);
        if outside < MOON_HALO_COLUMNS {
            let across = 1.0 - outside as f32 / MOON_HALO_COLUMNS as f32;
            let up = ((altitude_rows - moon_floor) / MOON_HALO_ROWS as f32).clamp(0.0, 1.0);
            gravity = gravity.max(MOON_GRAVITY * across * up);
        }
    }
    gravity
}

/// Peak wind speed (px/s) of the band at `altitude_rows`.
const fn band_wind(altitude_rows: i32) -> f32 {
    match SkyBand::at_altitude(altitude_rows) {
        SkyBand::Ground => 28.0,
        SkyBand::LowIsles => 52.0,
        SkyBand::Archipelago => 96.0,
        SkyBand::DeadAir => 210.0,
        SkyBand::Continents => 80.0,
        SkyBand::Orbit | SkyBand::Lunar => 0.0,
    }
}

/// The steady wind (px/s) of wind layer `layer`.
#[allow(clippy::cast_precision_loss, reason = "a hashed per-mille value")]
fn layer_wind(seed: u64, layer: i32) -> f32 {
    let roll = hash2(seed, SALT_WIND, layer, 0);
    let sign = if roll & 1 == 0 { 1.0 } else { -1.0 };
    let strength = ((roll >> 8) % 650 + 350) as f32 / 1000.0;
    sign * strength * band_wind(layer * WIND_LAYER_ROWS + WIND_LAYER_ROWS / 2)
}

/// The wind (air velocity, px/s) at column `x` (tile) and `altitude_rows`: layered, blended
/// between layers, with slow gusts along the layer, and calm below [`WIND_FLOOR_ROWS`].
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "layer indices are small integers"
)]
pub fn wind_at(settings: &WorldSettings, x: i32, altitude_rows: f32) -> f32 {
    if altitude_rows < WIND_FLOOR_ROWS {
        return 0.0;
    }
    let position = altitude_rows / WIND_LAYER_ROWS as f32;
    let layer = position.floor();
    let t = position - layer;
    let layer = layer as i32;
    let seed = settings.seed;
    let steady = lerp(layer_wind(seed, layer), layer_wind(seed, layer + 1), t);
    let gust = line_noise(
        seed,
        SALT_GUST.wrapping_add(u64::from(layer.unsigned_abs())),
        x,
        128,
    ) as f32
        / NOISE_MAX as f32;
    let ramp = ((altitude_rows - WIND_FLOOR_ROWS) / 120.0).clamp(0.0, 1.0);
    steady * gust.mul_add(0.3, 0.7) * ramp
}

#[cfg(test)]
thread_local! {
    /// Climb harness override: still air (standard gravity, the density profile, no wind,
    /// updrafts or vacuum) for the measurements of [`crate::climb`].
    static STILL_AIR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `body` with the still-air override on (tests: the climb harness).
#[cfg(test)]
pub(crate) fn with_still_air<T>(body: impl FnOnce() -> T) -> T {
    STILL_AIR.with(|still| still.set(true));
    let result = body();
    STILL_AIR.with(|still| still.set(false));
    result
}

/// The environment at world pixel `(x, y)` (see the module docs). [`Environment::STANDARD`]
/// on generators without a sky and below [`STANDARD_BELOW_ROWS`].
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "pixel positions map to tiles"
)]
pub fn environment_at(settings: &WorldSettings, x: f32, y: f32) -> Environment {
    if !settings.generator.has_sky() {
        return Environment::STANDARD;
    }
    let altitude = -y / TILE_SIZE;
    #[allow(clippy::cast_precision_loss, reason = "a small constant")]
    if altitude < STANDARD_BELOW_ROWS as f32 {
        return Environment::STANDARD;
    }
    let tile_x = (x / TILE_SIZE).floor() as i32;
    let tile_y = (y / TILE_SIZE).floor() as i32;
    #[cfg(test)]
    if STILL_AIR.with(std::cell::Cell::get) {
        return Environment {
            gravity: 1.0,
            air_density: air_density_profile(altitude),
            cold: cold_level(altitude),
            wind_x: 0.0,
            updraft: 0.0,
        };
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "updraft strengths are small integers"
    )]
    let mut updraft = updraft_at(settings, tile_x, tile_y) as f32;
    let mut wind_x = wind_at(settings, tile_x, altitude);
    if settings.generator.has_sky_isles() {
        // V9 (WP-SKY): island vents breathe updrafts and archipelago chains carry jet
        // streams (`worldgen::sky::winds`).
        #[allow(
            clippy::cast_precision_loss,
            reason = "vent strengths and stream speeds are small integers"
        )]
        {
            updraft = updraft.max(vent_updraft_at(settings, tile_x, tile_y) as f32);
            wind_x += jet_stream_at(settings, tile_x, tile_y) as f32;
        }
    }
    Environment {
        gravity: gravity_scale(settings, tile_x, altitude),
        air_density: air_density(altitude),
        cold: cold_level(altitude),
        wind_x,
        updraft,
    }
}

#[cfg(test)]
mod tests;
