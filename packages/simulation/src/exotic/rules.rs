//! The exotic worlds' rules as pure functions of the ground, a tile and world time: zones, the
//! rule at a tile, the hazards' exposure, and the schedules of everything that strikes (bolts,
//! hailstones, meteors, geysers, quakes). `world/exotic.rs` applies them; the renderer draws
//! their tells from the same functions, so what is shown is what will hit.
//!
//! Every schedule is hashed from the body's seed, a cell of the strip and a period index, so it
//! needs no saved state and plays alike at every tick rate.

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

use serde::{Deserialize, Serialize};

use super::{ExoticRule, ExoticStyle, gas_giant, locked, ring, roll, style_of_ground};
use crate::{
    planet::{BodyGround, PlanetGround},
    terrain::TilePosition,
};

/// Where a tile of an exotic world is.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Zone {
    /// On or over the surface (a gas giant: over its cloud tops, or on a city).
    Surface,
    /// The ring world's Skyring (its band of floating debris).
    Skyring,
    /// A stratum (a gas giant's band), by index (0-3).
    Stratum(u8),
    CoreChamber,
}

/// Rows under the surface that still count as the surface.
pub const SURFACE_ROWS: i32 = 2;

/// The zone of `tile` on `ground` (an exotic world).
#[must_use]
pub fn zone_at(ground: &PlanetGround, style: ExoticStyle, tile: TilePosition) -> Zone {
    if ground.in_core_chamber(tile.y) || tile.y >= ground.core_floor_row() {
        return Zone::CoreChamber;
    }
    if style == ExoticStyle::Ring
        && (ring::SKYRING_TOP - 8..=ring::SKYRING_BOTTOM + 4).contains(&tile.y)
    {
        return Zone::Skyring;
    }
    let column = i64::from(tile.x);
    if style == ExoticStyle::GasGiant {
        let top = gas_giant::cloud_top(ground, column);
        if tile.y < top + SURFACE_ROWS {
            return Zone::Surface;
        }
        return Zone::Stratum(
            u8::try_from(gas_giant::band_at(ground, column, tile.y)).unwrap_or(3),
        );
    }
    if tile.y < ground.surface_row(tile.x) + SURFACE_ROWS {
        return Zone::Surface;
    }
    Zone::Stratum(u8::try_from(ground.stratum_index(tile.x, tile.y)).unwrap_or(3))
}

/// The rule at `tile` (the stratum's, the surface's or the zone's).
#[must_use]
pub fn rule_at(
    ground: &PlanetGround,
    style: ExoticStyle,
    tile: TilePosition,
) -> Option<ExoticRule> {
    match (style, zone_at(ground, style, tile)) {
        (_, Zone::Stratum(index)) => style.rules().get(usize::from(index)).copied(),
        (_, Zone::Skyring) => Some(ExoticRule::Microgravity),
        (ExoticStyle::Locked, Zone::Surface) => Some(ExoticRule::SunAndShadow),
        (ExoticStyle::Ring, Zone::Surface) => Some(ExoticRule::MeteorShower),
        (ExoticStyle::GasGiant, Zone::Surface) => Some(ExoticRule::Buoyancy),
        (_, Zone::CoreChamber) => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Hazards.
// ---------------------------------------------------------------------------------------------

/// Hull a second the locked world's noon or night takes from a bare rig at full strength
/// (before the tier's scale).
pub const SUN_SHADOW_DAMAGE: f32 = 2.4;
/// Hull a second the Pressure Deep takes from a bare rig at its top (before the tier's
/// scale); it grows to [`PRESSURE_DEEPEST`] times as much at the band's foot and stays there
/// in the Diamond Rain.
pub const PRESSURE_DAMAGE: f32 = 1.5;
pub const PRESSURE_DEEPEST: f32 = 6.0;
/// The Dense Core's extra pull (standard gravities).
pub const DENSE_EXTRA_GRAVITY: f32 = 0.5;

/// What a tile's hazards do to a bare rig each second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Exposure {
    /// Heat: hull a second.
    pub heat: f32,
    /// Cold: hull a second.
    pub cold: f32,
    /// Pressure: hull a second (braced by high-gravity gear).
    pub pressure: f32,
    /// Extra gravity, standard gravities.
    pub heavy: f32,
}

impl Exposure {
    /// The planet hazards present.
    #[must_use]
    pub fn hazards(&self) -> Vec<crate::frontier::PlanetHazard> {
        use crate::frontier::PlanetHazard;
        let mut out = Vec::new();
        if self.heat > 0.0 {
            out.push(PlanetHazard::Heat);
        }
        if self.cold > 0.0 {
            out.push(PlanetHazard::Cold);
        }
        if self.pressure > 0.0 || self.heavy > 0.0 {
            out.push(PlanetHazard::HighGravity);
        }
        out
    }
}

/// How far into the crushing depth `tile` is (`0.0` above the Pressure Deep, `1.0` at its
/// foot and below).
#[must_use]
pub fn pressure_depth(ground: &PlanetGround, tile: TilePosition) -> f32 {
    let column = i64::from(tile.x);
    let top = gas_giant::cloud_top(ground, column);
    let depth = tile.y - top;
    let start = gas_giant::band_top_row(ground, 2);
    let end = gas_giant::band_top_row(ground, 3);
    if depth < start {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "a few thousand rows")]
    let share = (depth - start) as f32 / (end - start).max(1) as f32;
    share.clamp(0.0, 1.0)
}

/// The hazards a bare rig feels at `tile` (before the planet gear and the tier's scale).
#[must_use]
pub fn exposure_at(ground: &PlanetGround, tile: TilePosition) -> Exposure {
    let Some(style) = style_of_ground(ground) else {
        return Exposure::default();
    };
    let zone = zone_at(ground, style, tile);
    match (style, zone) {
        (ExoticStyle::GasGiant, Zone::Stratum(2..) | Zone::CoreChamber) => {
            let deep = pressure_depth(ground, tile);
            Exposure {
                pressure: PRESSURE_DAMAGE * (PRESSURE_DEEPEST - 1.0).mul_add(deep, 1.0),
                ..Exposure::default()
            }
        }
        (ExoticStyle::Locked, Zone::Surface | Zone::Stratum(0)) => {
            let warm = locked::warmth(ground, i64::from(tile.x));
            // Under the crust's rock the sun and the night bite half as hard.
            let shade = if zone == Zone::Surface { 1.0 } else { 0.5 };
            #[allow(clippy::cast_precision_loss, reason = "0..=1024")]
            let heat = ((warm - 640) as f32 / 384.0).clamp(0.0, 1.0);
            #[allow(clippy::cast_precision_loss, reason = "0..=1024")]
            let cold = ((384 - warm) as f32 / 384.0).clamp(0.0, 1.0);
            Exposure {
                heat: SUN_SHADOW_DAMAGE * heat * shade,
                cold: SUN_SHADOW_DAMAGE * cold * shade,
                ..Exposure::default()
            }
        }
        (ExoticStyle::Ring, Zone::Stratum(3) | Zone::CoreChamber) => Exposure {
            heavy: DENSE_EXTRA_GRAVITY,
            ..Exposure::default()
        },
        _ => Exposure::default(),
    }
}

// ---------------------------------------------------------------------------------------------
// Weather.
// ---------------------------------------------------------------------------------------------

/// How strong `style`'s storm is at world time `millis` on the body seeded `seed`
/// (`0.0..=1.0`, the body's own forecast: the badge and the rules agree).
#[must_use]
pub fn storm_strength(style: ExoticStyle, seed: u64, millis: u64) -> f32 {
    use crate::bodylife::weather::forecast;
    let table = crate::planet::hooks::hooks(style.theme()).weather_table();
    // The tables hold only kinds that blow without air: any atmosphere reads alike.
    let now = forecast(seed, table, crate::space::Atmosphere::Thin, millis, false);
    if now.kind == super::hooks::storm_kind(style) {
        now.intensity()
    } else {
        0.0
    }
}

/// The storm's rise: whether it is still calm at `before` and blowing at `now`.
#[must_use]
pub fn storm_rising(style: ExoticStyle, seed: u64, before: u64, now: u64) -> bool {
    storm_strength(style, seed, before) <= 0.0 && storm_strength(style, seed, now) > 0.0
}

// ---------------------------------------------------------------------------------------------
// Jet streams and drifts.
// ---------------------------------------------------------------------------------------------

/// Rows of one jet-stream lane.
pub const JET_LANE_ROWS: i32 = 24;
/// A jet stream's speed (pixels a second) in calm weather.
pub const JET_SPEED: f32 = 110.0;
/// How fast the Skyring's debris drifts a craft east (pixels a second).
pub const ORBIT_DRIFT: f32 = 45.0;
/// How fast the lodestone drags a rig toward the sunward meridian (pixels a second).
pub const LODESTONE_DRIFT: f32 = 55.0;
/// How hard the terminator gale pushes along the twilight's surface (pixels a second).
pub const GALE_SPEED: f32 = 90.0;

/// The jet stream's direction in the lane at `row` under the cloud tops `top` (+1 east).
#[must_use]
pub const fn jet_direction(row: i32, top: i32) -> f32 {
    if (row - top).div_euclid(JET_LANE_ROWS) % 2 == 0 {
        1.0
    } else {
        -1.0
    }
}

/// The sideways carry at `tile` (signed pixels a second): a jet stream, the Skyring's drift,
/// the lodestone's drag, the terminator gale.
#[must_use]
pub fn carry_at(ground: &PlanetGround, tile: TilePosition, storm: f32) -> Option<f32> {
    let style = style_of_ground(ground)?;
    match (style, zone_at(ground, style, tile)) {
        (ExoticStyle::GasGiant, Zone::Stratum(0)) => {
            let top = gas_giant::cloud_top(ground, i64::from(tile.x));
            Some(JET_SPEED * (1.0 + storm) * jet_direction(tile.y, top))
        }
        (ExoticStyle::Ring, Zone::Skyring) => Some(ORBIT_DRIFT),
        (ExoticStyle::Locked, Zone::Stratum(3)) => {
            // Toward column 0 the short way round.
            let width = i64::from(ground.strip_width());
            let delta = super::wrap_delta(i64::from(tile.x), 0, width);
            (delta != 0).then(|| -LODESTONE_DRIFT * delta.signum() as f32)
        }
        (ExoticStyle::Locked, Zone::Surface) if storm > 0.0 => {
            // From the day side toward the night, along the twilight ring.
            matches!(
                locked::side_of(ground, i64::from(tile.x)),
                locked::Side::Twilight
            )
            .then(|| {
                let width = i64::from(ground.strip_width());
                let delta = super::wrap_delta(i64::from(tile.x), 0, width);
                GALE_SPEED * storm * delta.signum() as f32
            })
        }
        _ => None,
    }
}

/// The share of the body's pull that holds a craft up at `tile` (the gas giant's buoyancy,
/// the Skyring's weak pull): `0.0` none.
#[must_use]
pub fn lift_share(ground: &PlanetGround, tile: TilePosition) -> f32 {
    let Some(style) = style_of_ground(ground) else {
        return 0.0;
    };
    match (style, zone_at(ground, style, tile)) {
        // The thick gas: a little at the cloud tops, more below.
        (ExoticStyle::GasGiant, Zone::Surface) => 0.3,
        (ExoticStyle::GasGiant, Zone::Stratum(0)) => 0.45,
        (ExoticStyle::GasGiant, _) => 0.55,
        (ExoticStyle::Ring, Zone::Skyring) => 2.0 / 3.0,
        _ => 0.0,
    }
}

/// Fastest fall (pixels a second) in the gas giant's thick gas.
pub const GAS_TERMINAL_FALL: f32 = 260.0;

// ---------------------------------------------------------------------------------------------
// Things that strike: bolts, hail, meteors.
// ---------------------------------------------------------------------------------------------

/// How long a bolt's column flickers before it strikes (milliseconds).
pub const BOLT_TELL_MILLIS: u64 = 1_500;
/// How long a bolt burns.
pub const BOLT_STRIKE_MILLIS: u64 = 300;
/// A bolt cell's width (columns), height (rows) and period (milliseconds).
pub const BOLT_CELL: (i64, i32) = (48, 60);
pub const BOLT_PERIOD_MILLIS: u64 = 5_000;
/// Hull a bolt takes from a rig in it (before the tier's scale).
pub const BOLT_DAMAGE: f32 = 16.0;

/// A bolt of lightning down a column of the Storm Belt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bolt {
    pub column: i32,
    pub top: i32,
    pub bottom: i32,
    /// World time its tell begins (it strikes [`BOLT_TELL_MILLIS`] later).
    pub tell_millis: u64,
}

impl Bolt {
    /// When it strikes.
    #[must_use]
    pub const fn strike_millis(&self) -> u64 {
        self.tell_millis + BOLT_TELL_MILLIS
    }

    /// Whether it is flickering at `millis`.
    #[must_use]
    pub const fn telling(&self, millis: u64) -> bool {
        millis >= self.tell_millis && millis < self.strike_millis()
    }

    /// Whether it is burning at `millis`.
    #[must_use]
    pub const fn striking(&self, millis: u64) -> bool {
        millis >= self.strike_millis() && millis < self.strike_millis() + BOLT_STRIKE_MILLIS
    }

    /// Whether `tile` is in it.
    #[must_use]
    pub fn covers(&self, tile: TilePosition, width: i32) -> bool {
        let dx = super::wrap_delta(i64::from(tile.x), i64::from(self.column), i64::from(width));
        dx.abs() <= 1 && (self.top..=self.bottom).contains(&tile.y)
    }
}

/// The bolt of cell `(cell_x, cell_y)` in period `period`, if one falls there (only in the
/// Storm Belt; twice as often in a superstorm).
#[must_use]
pub fn bolt_in(
    ground: &PlanetGround,
    cell_x: i64,
    cell_y: i32,
    period: u64,
    storm: f32,
) -> Option<Bolt> {
    let width = i64::from(ground.strip_width());
    let (cw, ch) = BOLT_CELL;
    let cells = (width / cw).max(1);
    let hash = roll(
        ground,
        100,
        cell_x.rem_euclid(cells) + i64::try_from(period).unwrap_or(0) * cells,
        i64::from(cell_y),
    );
    let chance = if storm > 0.0 { 60 } else { 30 };
    if hash % 100 >= chance {
        return None;
    }
    let column =
        cell_x * cw + i64::try_from((hash >> 8) % u64::try_from(cw).unwrap_or(1)).unwrap_or(0);
    let top = cell_y * ch;
    // Only in the Storm Belt.
    if gas_giant::band_at(ground, column, top + ch / 2) != 1 {
        return None;
    }
    let window = BOLT_PERIOD_MILLIS - BOLT_TELL_MILLIS - BOLT_STRIKE_MILLIS;
    Some(Bolt {
        column: i32::try_from(column.rem_euclid(width)).unwrap_or(0),
        top,
        bottom: top + ch - 1,
        tell_millis: period * BOLT_PERIOD_MILLIS + (hash >> 20) % window,
    })
}

/// The bolts round `tile` (within `reach` cells either way) whose tell or strike overlaps
/// `from..=to` (world milliseconds).
#[must_use]
pub fn bolts_near(
    ground: &PlanetGround,
    tile: TilePosition,
    reach: i64,
    from: u64,
    to: u64,
    storm: f32,
) -> Vec<Bolt> {
    let (cw, ch) = BOLT_CELL;
    let cell_x = i64::from(tile.x).div_euclid(cw);
    let cell_y = tile.y.div_euclid(ch);
    let first_period = from.saturating_sub(BOLT_PERIOD_MILLIS) / BOLT_PERIOD_MILLIS;
    let last_period = to / BOLT_PERIOD_MILLIS;
    let mut out = Vec::new();
    for period in first_period..=last_period {
        for cx in cell_x - reach..=cell_x + reach {
            for cy in cell_y - 1..=cell_y + 1 {
                if let Some(bolt) = bolt_in(ground, cx, cy, period, storm)
                    && bolt.tell_millis <= to
                    && bolt.strike_millis() + BOLT_STRIKE_MILLIS >= from
                {
                    out.push(bolt);
                }
            }
        }
    }
    out
}

/// What falls from the sky (a hailstone of diamond, a meteorite).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Faller {
    pub column: i32,
    /// The row it falls from (it lands on the first solid ground under it).
    pub from_row: i32,
    /// The lowest row it can land on (it is gone if nothing stops it).
    pub to_row: i32,
    /// When it lands (its shadow grows for [`FALL_TELL_MILLIS`] before).
    pub land_millis: u64,
}

/// How long a falling thing's shadow grows before it lands.
pub const FALL_TELL_MILLIS: u64 = 1_800;
/// Hull a hailstone or a meteorite takes from what it lands on (before the tier's scale), and
/// how far its blow reaches (tiles).
pub const FALL_DAMAGE: f32 = 14.0;
pub const FALL_RADIUS_TILES: f32 = 1.8;
/// A hail cell's size and period; a meteor cell's.
pub const HAIL_CELL: (i64, i32) = (24, 40);
pub const HAIL_PERIOD_MILLIS: u64 = 3_000;
pub const METEOR_CELL: i64 = 48;
pub const METEOR_PERIOD_MILLIS: u64 = 7_000;

/// The faller of `style`'s cell `(cell_x, cell_y)` in period `period`, if one falls.
#[must_use]
pub fn faller_in(
    ground: &PlanetGround,
    style: ExoticStyle,
    cell_x: i64,
    cell_y: i32,
    period: u64,
    storm: f32,
) -> Option<Faller> {
    let width = i64::from(ground.strip_width());
    let (cw, ch, period_millis, chance) = match style {
        ExoticStyle::GasGiant => (HAIL_CELL.0, HAIL_CELL.1, HAIL_PERIOD_MILLIS, 35_u64),
        ExoticStyle::Ring => (
            METEOR_CELL,
            0,
            METEOR_PERIOD_MILLIS,
            if storm > 0.0 { 75 } else { 25 },
        ),
        ExoticStyle::Locked => return None,
    };
    let cells = (width / cw).max(1);
    let hash = roll(
        ground,
        110,
        cell_x.rem_euclid(cells) + i64::try_from(period).unwrap_or(0) * cells,
        i64::from(cell_y),
    );
    if hash % 100 >= chance {
        return None;
    }
    let column = (cell_x * cw
        + i64::try_from((hash >> 8) % u64::try_from(cw).unwrap_or(1)).unwrap_or(0))
    .rem_euclid(width);
    let land_millis = period * period_millis
        + FALL_TELL_MILLIS
        + (hash >> 20) % (period_millis - FALL_TELL_MILLIS);
    let (from_row, to_row) = match style {
        ExoticStyle::GasGiant => {
            let top = cell_y * ch;
            // Only in the Diamond Rain.
            if gas_giant::band_at(ground, column, top + ch / 2) != 3 {
                return None;
            }
            (top, top + ch - 1)
        }
        _ => {
            // From the Skyring's foot down to the plains.
            let surface = ground.surface_row(i32::try_from(column).unwrap_or(0));
            (ring::SKYRING_BOTTOM + 1, surface + SURFACE_ROWS)
        }
    };
    Some(Faller {
        column: i32::try_from(column).unwrap_or(0),
        from_row,
        to_row,
        land_millis,
    })
}

/// The fallers round `tile` (within `reach` cells either way) landing in `from..=to`.
#[must_use]
pub fn fallers_near(
    ground: &PlanetGround,
    style: ExoticStyle,
    tile: TilePosition,
    reach: i64,
    from: u64,
    to: u64,
    storm: f32,
) -> Vec<Faller> {
    let (cw, ch, period_millis) = match style {
        ExoticStyle::GasGiant => (HAIL_CELL.0, HAIL_CELL.1, HAIL_PERIOD_MILLIS),
        ExoticStyle::Ring => (METEOR_CELL, 1, METEOR_PERIOD_MILLIS),
        ExoticStyle::Locked => return Vec::new(),
    };
    let cell_x = i64::from(tile.x).div_euclid(cw);
    let rows: Vec<i32> = if style == ExoticStyle::GasGiant {
        let cell_y = tile.y.div_euclid(ch);
        (cell_y - 1..=cell_y + 1).collect()
    } else {
        vec![0]
    };
    let first = from / period_millis;
    let last = to / period_millis;
    let mut out = Vec::new();
    for period in first..=last {
        for cx in cell_x - reach..=cell_x + reach {
            for cy in &rows {
                if let Some(faller) = faller_in(ground, style, cx, *cy, period, storm)
                    && faller.land_millis >= from
                    && faller.land_millis <= to
                {
                    out.push(faller);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Geysers and quakes (the locked world).
// ---------------------------------------------------------------------------------------------

/// A geyser's cycle (milliseconds), its hiss before it blows and how long it blows.
pub const GEYSER_PERIOD_MILLIS: u64 = 10_000;
pub const GEYSER_HISS_MILLIS: u64 = 2_000;
pub const GEYSER_BLOW_MILLIS: u64 = 2_500;
/// How high a plume reaches over its vent (rows), and how fast it throws a craft up.
pub const GEYSER_REACH_ROWS: i32 = 16;
pub const GEYSER_LIFT: f32 = 380.0;
/// Hull a second a plume's steam or nitrogen takes from a bare rig in it (before the tier).
pub const GEYSER_BITE: f32 = 3.0;

/// A geyser's phase at `millis`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeyserPhase {
    Quiet,
    Hissing,
    Blowing,
}

/// The phase of the geyser at `vent` at world time `millis`.
#[must_use]
pub fn geyser_phase(ground: &PlanetGround, vent: TilePosition, millis: u64) -> GeyserPhase {
    let offset = roll(ground, 120, i64::from(vent.x), i64::from(vent.y)) % GEYSER_PERIOD_MILLIS;
    let into = (millis + offset) % GEYSER_PERIOD_MILLIS;
    let blow_from = GEYSER_PERIOD_MILLIS - GEYSER_BLOW_MILLIS;
    if into >= blow_from {
        GeyserPhase::Blowing
    } else if into >= blow_from - GEYSER_HISS_MILLIS {
        GeyserPhase::Hissing
    } else {
        GeyserPhase::Quiet
    }
}

/// The vents (generated positions) within `reach` columns and rows of `tile` in the Geyser
/// Fields (whether each still stands is the terrain's say).
#[must_use]
pub fn vents_near(ground: &PlanetGround, tile: TilePosition, reach: i32) -> Vec<TilePosition> {
    let width = ground.strip_width();
    let cell_x = i64::from(tile.x).div_euclid(40);
    let cell_y = tile.y.div_euclid(30);
    let mut out = Vec::new();
    for cx in cell_x - 2..=cell_x + 2 {
        for cy in cell_y - 2..=cell_y + 2 {
            if let Some(vent) = locked::vent_in(ground, cx, cy) {
                let dx = super::wrap_delta(i64::from(vent.x), i64::from(tile.x), i64::from(width));
                if dx.abs() <= i64::from(reach)
                    && (vent.y - tile.y).abs() <= reach
                    && ground.stratum_index(vent.x, vent.y) == 1
                {
                    out.push(vent);
                }
            }
        }
    }
    out
}

/// A tidal quake's cycle (milliseconds) and its rumble before the shock.
pub const QUAKE_PERIOD_MILLIS: u64 = 40_000;
pub const QUAKE_RUMBLE_MILLIS: u64 = 5_000;

/// The quake's phase on `ground` at `millis`: `Some(true)` the shock's moment has passed in
/// `(before, now]`, `Some(false)` rumbling, `None` quiet.
#[must_use]
pub fn quake_at(ground: &PlanetGround, millis: u64) -> Option<bool> {
    let offset = roll(ground, 130, 0, 0) % QUAKE_PERIOD_MILLIS;
    let into = (millis + offset) % QUAKE_PERIOD_MILLIS;
    (into < QUAKE_RUMBLE_MILLIS).then_some(false)
}

/// Whether the shock of a quake (the end of its rumble) falls in `(before, now]`.
#[must_use]
pub fn quake_shock(ground: &PlanetGround, before: u64, now: u64) -> bool {
    let offset = roll(ground, 130, 0, 0) % QUAKE_PERIOD_MILLIS;
    let index = |millis: u64| {
        (i128::from(millis) + i128::from(offset) - i128::from(QUAKE_RUMBLE_MILLIS))
            .div_euclid(i128::from(QUAKE_PERIOD_MILLIS))
    };
    index(now) != index(before)
}

/// Whether a quake's rumble begins in `(before, now]`.
#[must_use]
pub fn quake_rumble_starts(ground: &PlanetGround, before: u64, now: u64) -> bool {
    let offset = roll(ground, 130, 0, 0) % QUAKE_PERIOD_MILLIS;
    (before + offset) / QUAKE_PERIOD_MILLIS != (now + offset) / QUAKE_PERIOD_MILLIS
}

// ---------------------------------------------------------------------------------------------
// Tiers.
// ---------------------------------------------------------------------------------------------

/// The enemy tier of an exotic world's surface (the stars open after the Heart: like the wild
/// worlds, they hit like the deep Descents); each stratum down adds one, up to
/// [`EXOTIC_MAX_TIER`].
pub const EXOTIC_BASE_TIER: u8 = 8;
pub const EXOTIC_MAX_TIER: u8 = 11;

/// The tier at `tile` on an exotic world.
#[must_use]
pub fn tier_at(ground: &PlanetGround, style: ExoticStyle, tile: TilePosition) -> u8 {
    let extra = match zone_at(ground, style, tile) {
        Zone::Surface | Zone::Skyring => 0,
        Zone::Stratum(index) => index.min(2) + 1,
        Zone::CoreChamber => 3,
    };
    EXOTIC_BASE_TIER.saturating_add(extra).min(EXOTIC_MAX_TIER)
}
