//! The home system: a yellow sun, the home world and the Moon, then the generator's own
//! sister planets outward. Fixed in [`HOME_SECTOR`] at the sector's centre.

use super::{
    Atmosphere, BodyKind, HOME_SECTOR, SpaceBody, SpaceBodyId, SpaceTile, StarSystem, Station,
    StationId, StationKind, TURN, Theme,
    generate::{SystemPlan, generate_system, surface_columns},
    trig::polar,
};
use crate::{star_chart::StarClass, worldgen::noise::hash2};

/// The home sun's radius, tiles.
pub const HOME_SUN_RADIUS: u32 = 24_000;
/// The home world's orbit around the sun, tiles (straight "up" from the sun).
pub const HOME_ORBIT: u32 = 260_000;
/// The home world's nominal radius in space, tiles. Its ground is the existing (flat,
/// wrapping, streamed) world; this radius is for the map, gravity from space and the crossing.
pub const HOME_WORLD_RADIUS: u32 = 10_000;
/// The Moon's distance from the home world's centre, tiles (straight up from it).
pub const MOON_ORBIT: u32 = 36_000;
/// The Moon's nominal radius in space, tiles (its ground is the existing Moon).
pub const MOON_RADIUS: u32 = 640;
/// Tiles above the Moon's top where a craft leaving the home world's sky enters space.
pub const DEPARTURE_ABOVE_MOON: u32 = 256;
/// Depth of the home world's Descent, rows (its real ground; see [`HOME_WORLD_RADIUS`]).
pub const HOME_DESCENT_ROWS: u16 = 13_000;

const SALT_HOME_GROUND: u64 = 0x5BAC_E900;

/// Halcyon Dock, the home system's station, from the home world's centre (tiles).
///
/// Wave 11 integration: flight's first dock, a short hop up and across from the departure
/// point, outside the home world's and the Moon's reach, so the straight line there never dips
/// back into the home sky.
pub const HALCYON_DOCK_OFFSET: (i64, i64) = (3_000, -42_000);

/// The home world's id.
pub const HOME_WORLD_BODY: SpaceBodyId = SpaceBodyId {
    sector: HOME_SECTOR,
    system: 0,
    body: 0,
};
/// The Moon's id.
pub const MOON_BODY: SpaceBodyId = SpaceBodyId {
    sector: HOME_SECTOR,
    system: 0,
    body: 1,
};

const UP: u32 = 3 * TURN / 4;

/// The home system (system 0 of [`HOME_SECTOR`]).
pub(super) fn home_system(seed: u64) -> StarSystem {
    generate_system(
        seed,
        HOME_SECTOR,
        0,
        &SystemPlan {
            anchor: HOME_SECTOR.centre(),
            jitter: false,
            needs_planets: false,
            star: Some((StarClass::Yellow, HOME_SUN_RADIUS)),
            reserved: Some(HOME_ORBIT + home_extent()),
            home: true,
        },
    )
}

/// How far the home world's own things reach (the Moon and a margin).
const fn home_extent() -> u32 {
    MOON_ORBIT + MOON_RADIUS + 4_000
}

/// Pushes the home world (body 0) and the Moon (body 1), relative to the sun.
pub(super) fn push_home_bodies(seed: u64, system: &mut StarSystem) {
    let (x, y) = polar(i64::from(HOME_ORBIT), UP);
    let home = SpaceTile::new(x, y);
    system.bodies.push(SpaceBody {
        index: 0,
        kind: BodyKind::HomeWorld,
        parent: None,
        theme: Theme::Home,
        centre: home,
        radius: HOME_WORLD_RADIUS,
        orbit_radius: HOME_ORBIT,
        orbit_angle: UP,
        extent: home_extent(),
        gravity_milli_g: 1_000,
        day_seconds: 1_440,
        atmosphere: Atmosphere::Breathable,
        temperature_c: 15,
        surface_columns: surface_columns(HOME_WORLD_RADIUS),
        rows: HOME_WORLD_RADIUS,
        strata_rows: HOME_DESCENT_ROWS,
        ring: None,
        ground_seed: hash2(seed, SALT_HOME_GROUND, 0, 0),
    });
    let (dx, dy) = polar(i64::from(MOON_ORBIT), UP);
    system.bodies.push(SpaceBody {
        index: 1,
        kind: BodyKind::HomeMoon,
        parent: Some(0),
        theme: Theme::Moon,
        centre: home.offset(dx, dy),
        radius: MOON_RADIUS,
        orbit_radius: MOON_ORBIT,
        orbit_angle: UP,
        extent: MOON_RADIUS + 1_000,
        gravity_milli_g: 170,
        day_seconds: 0,
        atmosphere: Atmosphere::Vacuum,
        temperature_c: -120,
        surface_columns: surface_columns(MOON_RADIUS),
        rows: MOON_RADIUS,
        strata_rows: 240,
        ring: None,
        ground_seed: hash2(seed, SALT_HOME_GROUND, 1, 0),
    });
    system.stations.push(Station {
        id: StationId {
            sector: HOME_SECTOR,
            system: 0,
            index: 0,
        },
        kind: StationKind::TradePost,
        centre: home.offset(HALCYON_DOCK_OFFSET.0, HALCYON_DOCK_OFFSET.1),
        size: 120,
        planet: 1,
    });
}

/// Where a craft leaving the top of the home world's sky enters space: just above the Moon,
/// straight up from it (away from the home world and its sun).
#[must_use]
pub fn home_departure(system: &StarSystem) -> SpaceTile {
    let moon = &system.bodies[usize::from(MOON_BODY.body)];
    moon.centre
        .offset(0, -i64::from(MOON_RADIUS + DEPARTURE_ABOVE_MOON))
}
