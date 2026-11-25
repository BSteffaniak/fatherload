//! The frontier worlds' rules as pure functions.
//!
//! The world pass in `world/frontier.rs` and the renderer read the same answers: where a tile
//! is (surface or stratum), its heat and other hazards, the weather, the volcanoes' eruptions
//! and their lava bombs, and the light wells' beams.
//!
//! Every timing is a pure function of the planet's seed and world time in milliseconds, so a
//! host, its clients and a replay agree at any tick rate, and nothing needs saving.

use serde::{Deserialize, Serialize};

use super::{
    FrontierStyle, FrontierTile, PlanetHazard, StratumRule,
    generate::{LightWell, cone_at, volcano_cones},
};
use crate::{
    planet::{BodyGround, PlanetGround, hash3},
    terrain::{TileKind, TilePosition},
};

/// Where a tile of a frontier world is: on (or over) the surface, or in a stratum.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Zone {
    Surface,
    /// A stratum, by index (0-3).
    Stratum(u8),
    CoreChamber,
}

/// Rows below the surface that still count as the surface (the dunes' and ash's top).
pub const SURFACE_ROWS: i32 = 2;

/// The zone of `tile` on frontier world `ground`.
#[must_use]
pub fn zone_at(ground: &PlanetGround, tile: TilePosition) -> Zone {
    if ground.in_core_chamber(tile.y) || tile.y >= ground.core_floor_row() {
        return Zone::CoreChamber;
    }
    if tile.y < ground.surface_row(tile.x) + SURFACE_ROWS {
        return Zone::Surface;
    }
    Zone::Stratum(u8::try_from(ground.stratum_index(tile.x, tile.y)).unwrap_or(3))
}

/// The signature rule in force at `tile`, if it is in a stratum.
#[must_use]
pub fn rule_at(
    ground: &PlanetGround,
    style: FrontierStyle,
    tile: TilePosition,
) -> Option<StratumRule> {
    match zone_at(ground, tile) {
        Zone::Stratum(index) => style.rules().get(usize::from(index)).copied(),
        Zone::Surface | Zone::CoreChamber => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Heat.
// ---------------------------------------------------------------------------------------------

/// Heat units of each zone (surface, strata 0-3) per style ([`crate::protection`]'s units:
/// three to a heat level). The Magma Chambers run hotter than the hottest band at home, so
/// even the best classic cooling leaves a burn the stations track's heat gear covers.
const fn zone_heat(style: FrontierStyle) -> [u8; 5] {
    match style {
        FrontierStyle::Desert => [6, 3, 3, 6, 15],
        FrontierStyle::Volcanic => [9, 6, 12, 21, 42],
        FrontierStyle::Crystal => [0, 0, 0, 0, 0],
    }
}

/// Extra heat units inside a lava tube.
pub const TUBE_HEAT_UNITS: u8 = 3;

/// Heat units at `tile` of a frontier world (`None` on any other ground): what the rig's heat
/// damage, a pilot's exposure and the HEAT warning read (`crust::heat_units` asks here
/// first).
#[must_use]
pub fn heat_units(ground: &PlanetGround, tile: TilePosition) -> Option<u8> {
    let style = super::style_of_ground(ground)?;
    let heat = zone_heat(style);
    Some(match zone_at(ground, tile) {
        Zone::Surface => heat[0],
        Zone::Stratum(index) => {
            let base = heat[usize::from(index.min(3)) + 1];
            let tube = style == FrontierStyle::Volcanic
                && index == 1
                && super::generate::tube_way(ground, i64::from(tile.x), tile.y).is_some();
            if tube {
                base.saturating_add(TUBE_HEAT_UNITS)
            } else {
                base
            }
        }
        Zone::CoreChamber => heat[4],
    })
}

// ---------------------------------------------------------------------------------------------
// The other hazards.
// ---------------------------------------------------------------------------------------------

/// Hull a second toxic air eats from an unprotected rig.
pub const TOXIC_DAMAGE_PER_SECOND: f32 = 3.0;
/// Hull a second the cold frosts off an unprotected rig (four times it in a glitterfrost).
pub const COLD_DAMAGE_PER_SECOND: f32 = 1.0;
/// The heavy lattice's extra pull, standard gravities, on an unprotected rig.
pub const HEAVY_EXTRA_GRAVITY: f32 = 0.5;

/// What a frontier world's air and ground do to a rig at a tile, before its planet gear.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Exposure {
    /// Toxic air: hull a second.
    pub toxic: f32,
    /// Cold: hull a second.
    pub cold: f32,
    /// Extra gravity, standard gravities.
    pub heavy: f32,
}

impl Exposure {
    /// The hazards present.
    #[must_use]
    pub fn hazards(&self) -> Vec<PlanetHazard> {
        let mut out = Vec::new();
        if self.toxic > 0.0 {
            out.push(PlanetHazard::ToxicAir);
        }
        if self.cold > 0.0 {
            out.push(PlanetHazard::Cold);
        }
        if self.heavy > 0.0 {
            out.push(PlanetHazard::HighGravity);
        }
        out
    }
}

/// The exposure at `tile` of frontier world `ground` in `weather`.
#[must_use]
pub fn exposure_at(ground: &PlanetGround, tile: TilePosition, weather: Weather) -> Exposure {
    let Some(style) = super::style_of_ground(ground) else {
        return Exposure::default();
    };
    let zone = zone_at(ground, tile);
    let storm = weather.phase == WeatherPhase::Storm;
    match (style, zone) {
        (FrontierStyle::Volcanic, Zone::Surface) => Exposure {
            toxic: TOXIC_DAMAGE_PER_SECOND * if storm { 2.0 } else { 1.0 },
            ..Exposure::default()
        },
        (FrontierStyle::Volcanic, Zone::Stratum(0)) => Exposure {
            toxic: TOXIC_DAMAGE_PER_SECOND,
            ..Exposure::default()
        },
        (FrontierStyle::Crystal, Zone::Surface) => Exposure {
            cold: COLD_DAMAGE_PER_SECOND * if storm { 4.0 } else { 1.0 },
            ..Exposure::default()
        },
        (FrontierStyle::Crystal, Zone::Stratum(3) | Zone::CoreChamber) => Exposure {
            heavy: HEAVY_EXTRA_GRAVITY,
            ..Exposure::default()
        },
        _ => Exposure::default(),
    }
}

/// The share of a hazard that gets through planet gear of `tier` (0-6): all of it bare,
/// none at the top tier.
#[must_use]
pub fn unprotected_share(tier: u8) -> f32 {
    let tier = tier.min(super::PLANET_GEAR_MAX_TIER);
    f32::from(super::PLANET_GEAR_MAX_TIER - tier) / f32::from(super::PLANET_GEAR_MAX_TIER)
}

// ---------------------------------------------------------------------------------------------
// Weather.
// ---------------------------------------------------------------------------------------------

/// A frontier weather's phase.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum WeatherPhase {
    #[default]
    Calm,
    /// The telegraph: the storm is coming (a darkening horizon, the wind picking up sand).
    Rising,
    /// The storm: a sandstorm (wind on the surface), an ash fall (toxic air doubles), a
    /// glitterfrost (bitter cold on the surface).
    Storm,
}

/// A frontier world's weather at a moment.
///
/// TODO-SEAM w12-bodylife: the bodylife track owns each body's weather and clock; until its
/// per-body weather lands, a frontier world's weather is this pure schedule of its seed and
/// world time ([`weather_at`]), and the effects read only this value.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Weather {
    pub phase: WeatherPhase,
    /// Which way the wind blows (+1 east, -1 west).
    pub wind: i8,
    /// How hard, per mille (it ramps in and out over a few seconds).
    pub strength_permille: u16,
    /// Milliseconds until the phase changes.
    pub remaining_millis: u64,
}

/// The weather's name for each style's storm.
#[must_use]
pub const fn storm_name(style: FrontierStyle) -> &'static str {
    match style {
        FrontierStyle::Desert => "sandstorm",
        FrontierStyle::Volcanic => "ash fall",
        FrontierStyle::Crystal => "glitterfrost",
    }
}

/// One weather cycle's length per style, milliseconds.
const fn cycle_millis(style: FrontierStyle) -> u64 {
    match style {
        FrontierStyle::Desert => 240_000,
        FrontierStyle::Volcanic => 200_000,
        FrontierStyle::Crystal => 260_000,
    }
}

/// How long the telegraph runs before a storm.
pub const RISING_MILLIS: u64 = 12_000;
/// How long a storm's strength takes to ramp in (and out).
const RAMP_MILLIS: u64 = 4_000;

const P_WEATHER: u64 = 80;

/// The weather of a frontier world of `style` and `seed` at world time `millis`.
#[must_use]
pub fn weather_at(style: FrontierStyle, seed: u64, millis: u64) -> Weather {
    let cycle = cycle_millis(style);
    let index = millis / cycle;
    let into = millis % cycle;
    let roll = hash3(seed, P_WEATHER, i64::try_from(index).unwrap_or(0), 0);
    let start = 50_000 + roll % 60_000;
    let length = 45_000 + (roll >> 20) % 25_000;
    let wind = if (roll >> 40).is_multiple_of(2) {
        1
    } else {
        -1
    };
    let rising_from = start - RISING_MILLIS;
    let end = start + length;
    if into < rising_from || into >= end {
        let remaining = if into < rising_from {
            rising_from - into
        } else {
            cycle - into + 50_000 - RISING_MILLIS
        };
        return Weather {
            phase: WeatherPhase::Calm,
            wind,
            strength_permille: 0,
            remaining_millis: remaining,
        };
    }
    if into < start {
        return Weather {
            phase: WeatherPhase::Rising,
            wind,
            strength_permille: 0,
            remaining_millis: start - into,
        };
    }
    let ramp_in = (into - start).min(RAMP_MILLIS);
    let ramp_out = (end - into).min(RAMP_MILLIS);
    let strength = ramp_in.min(ramp_out) * 1_000 / RAMP_MILLIS;
    Weather {
        phase: WeatherPhase::Storm,
        wind,
        strength_permille: u16::try_from(strength).unwrap_or(1_000),
        remaining_millis: end - into,
    }
}

/// The hazard a style's storm adds on its surface, if any.
#[must_use]
pub const fn weather_hazard(style: FrontierStyle) -> Option<PlanetHazard> {
    match style {
        FrontierStyle::Desert => None,
        FrontierStyle::Volcanic => Some(PlanetHazard::ToxicAir),
        FrontierStyle::Crystal => Some(PlanetHazard::Cold),
    }
}

/// A sandstorm's push on the surface at full strength, pixels per second squared.
pub const SANDSTORM_ACCEL: f32 = 200.0;
/// The fastest a sandstorm carries a rig or pilot sideways, pixels per second.
pub const SANDSTORM_TOP_SPEED: f32 = 150.0;

// ---------------------------------------------------------------------------------------------
// Eruptions.
// ---------------------------------------------------------------------------------------------

/// A volcano's phase.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum EruptionPhase {
    Quiet,
    /// The telegraph: the cone rumbles and smokes; bombs follow.
    Rumbling,
    /// Lava bombs fly.
    Erupting,
}

/// How long a cone rumbles before it erupts.
pub const RUMBLE_MILLIS: u64 = 6_000;
/// How long an eruption throws bombs.
pub const ERUPTION_MILLIS: u64 = 9_000;
/// Milliseconds between one cone's bombs.
pub const BOMB_PERIOD_MILLIS: u64 = 450;
/// A bomb's flight: its shadow shows on the ground this long before it lands.
pub const BOMB_FLIGHT_MILLIS: u64 = 1_600;
/// Hull a bomb takes from a rig it lands on (before the rig's mitigation).
pub const BOMB_DAMAGE: f32 = 14.0;
/// How far from its landing a bomb hurts, tiles.
pub const BOMB_RADIUS_TILES: f32 = 1.6;

const P_ERUPTION: u64 = 81;
const P_BOMB: u64 = 82;

/// One cone's eruption cycle: its length and where in it the cone starts at world time 0.
const fn eruption_cycle(seed: u64, cone: i64) -> (u64, u64) {
    let roll = hash3(seed, P_ERUPTION, cone, 0);
    let length = 120_000 + roll % 60_000;
    (length, (roll >> 24) % length)
}

/// Cone `cone`'s phase at world time `millis`, and the world time its eruption (current or
/// next) starts.
#[must_use]
pub const fn eruption_at(seed: u64, cone: i64, millis: u64) -> (EruptionPhase, u64) {
    let (length, offset) = eruption_cycle(seed, cone);
    let shifted = millis + offset;
    let into = shifted % length;
    let erupt_from = length - ERUPTION_MILLIS;
    let start = shifted - into + erupt_from;
    let start = start.saturating_sub(offset);
    let phase = if into >= erupt_from {
        EruptionPhase::Erupting
    } else if into >= erupt_from - RUMBLE_MILLIS {
        EruptionPhase::Rumbling
    } else {
        EruptionPhase::Quiet
    };
    (phase, start)
}

/// A lava bomb: thrown at `launch` (world milliseconds), it lands at `land` on `column`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct LavaBomb {
    pub cone: i64,
    pub column: i64,
    pub launch: u64,
    pub land: u64,
}

/// The bombs cone `cone` of `ground` throws in the eruption that starts at `start`.
fn eruption_bombs(ground: &PlanetGround, cone: i64, start: u64) -> impl Iterator<Item = LavaBomb> {
    let seed = ground.params().seed;
    let found = cone_at(ground, cone);
    let reach = found.radius + 28;
    let width = i64::from(ground.grid().surface_columns());
    (0..ERUPTION_MILLIS / BOMB_PERIOD_MILLIS).map(move |k| {
        let roll = hash3(
            seed,
            P_BOMB,
            cone,
            i64::try_from(start / 1_000 + k).unwrap_or(0),
        );
        let offset =
            i64::try_from(roll % u64::try_from(2 * reach + 1).unwrap_or(1)).unwrap_or(0) - reach;
        let launch = start + k * BOMB_PERIOD_MILLIS;
        LavaBomb {
            cone,
            column: (found.centre + offset).rem_euclid(width),
            launch,
            land: launch + BOMB_FLIGHT_MILLIS,
        }
    })
}

/// The bombs of cone `cone` that land in `(after, until]` (world milliseconds).
#[must_use]
pub fn bombs_landing(ground: &PlanetGround, cone: i64, after: u64, until: u64) -> Vec<LavaBomb> {
    let seed = ground.params().seed;
    let mut found = Vec::new();
    // The eruption under way at `until` (minus a flight) and the one before it.
    for probe in [
        until.saturating_sub(BOMB_FLIGHT_MILLIS),
        after.saturating_sub(BOMB_FLIGHT_MILLIS),
    ] {
        let (_, start) = eruption_at(seed, cone, probe);
        for bomb in eruption_bombs(ground, cone, start) {
            if bomb.land > after && bomb.land <= until && !found.contains(&bomb) {
                found.push(bomb);
            }
        }
    }
    found.sort();
    found
}

/// The bombs of cone `cone` in the air at world time `now` (thrown, not yet landed): the
/// renderer draws each one's arc and its shadow where it will land.
#[must_use]
pub fn bombs_in_flight(ground: &PlanetGround, cone: i64, now: u64) -> Vec<LavaBomb> {
    let seed = ground.params().seed;
    let (_, start) = eruption_at(seed, cone, now.saturating_sub(BOMB_FLIGHT_MILLIS));
    eruption_bombs(ground, cone, start)
        .filter(|bomb| bomb.launch <= now && bomb.land > now)
        .collect()
}

/// The cones of `ground` within `reach` columns of `column`.
#[must_use]
pub fn cones_near(ground: &PlanetGround, column: i64, reach: i64) -> Vec<i64> {
    let width = i64::from(ground.grid().surface_columns());
    volcano_cones(ground)
        .into_iter()
        .filter(|cone| {
            let mut dx = (column - cone.centre).rem_euclid(width);
            if dx > width / 2 {
                dx = width - dx;
            }
            dx <= reach + cone.radius
        })
        .map(|cone| cone.index)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Light beams.
// ---------------------------------------------------------------------------------------------

/// A traced beam: the tiles it lights, in order, and whether it reached its receptor.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Beam {
    pub path: Vec<TilePosition>,
    pub lit: bool,
}

/// The longest a beam runs, tiles.
pub const BEAM_MAX_TILES: usize = 96;

/// Turns a direction clockwise on screen (rows grow downward): down to west, west to up.
#[must_use]
pub const fn turn_cw((dx, dy): (i32, i32)) -> (i32, i32) {
    (-dy, dx)
}

/// Turns a direction counter-clockwise on screen: down to east, east to up.
#[must_use]
pub const fn turn_ccw((dx, dy): (i32, i32)) -> (i32, i32) {
    (dy, -dx)
}

/// The beam light well `well` sends down from its lens through ground `kind_at` reads: it
/// runs through open tiles, turns at prisms and stops at anything else; it is lit when it
/// reaches a receptor.
#[must_use]
pub fn trace_beam(well: &LightWell, kind_at: impl Fn(TilePosition) -> Option<TileKind>) -> Beam {
    let mut beam = Beam::default();
    let mut at = TilePosition {
        x: well.column,
        y: well.lens_row,
    };
    let mut dir = (0, 1);
    for _ in 0..BEAM_MAX_TILES {
        at = TilePosition {
            x: at.x + dir.0,
            y: at.y + dir.1,
        };
        let Some(kind) = kind_at(at) else {
            break;
        };
        match kind {
            TileKind::Air | TileKind::Fluid(_) => beam.path.push(at),
            TileKind::Frontier(FrontierTile::PrismCw) => {
                beam.path.push(at);
                dir = turn_cw(dir);
            }
            TileKind::Frontier(FrontierTile::PrismCcw) => {
                beam.path.push(at);
                dir = turn_ccw(dir);
            }
            TileKind::Frontier(FrontierTile::Receptor | FrontierTile::ReceptorLit) => {
                beam.path.push(at);
                beam.lit = true;
                break;
            }
            _ => break,
        }
    }
    beam
}
