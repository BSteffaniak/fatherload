//! The space the flight model flies through: star systems, their bodies and stations, and
//! the gravity they pull with.
//!
//! **TODO-SEAM(w11-space-gen):** this is a **placeholder**: the space generator
//! (`crate::space`) owns the real galaxy. [`SpaceQuery`] is the whole interface flight reads,
//! so the integrator implements it on the generator and drops [`PlaceholderSpace`]. What the
//! placeholder promises (and the generator should keep):
//!
//! * Everything is a pure function of the world seed, the galactic sector and the Archive's
//!   star charts ([`crate::star_chart`]): a chart's system sits in its sector (y "up" the
//!   galactic plane, so space y is its negative) with its class and world count.
//! * Positions are space pixels ([`WorldPos`], 32 to a tile). A galactic sector is
//!   [`GALAXY_SECTOR_TILES`] across; the home system is sector (0, 0) and the home world
//!   ([`HOME_BODY`]) sits [`HOME_ORBIT_TILES`] from its star.
//! * Orbits are static (a body's position never moves), so a body list is valid for a tick.
//! * Every landable body is at least 400 tiles in radius (expansion plan F12).

use serde::{Deserialize, Serialize};

use crate::{
    position::{SECTOR_PX, SECTOR_TILES, WorldPos},
    star_chart::{StarChart, StarClass},
    terrain::TILE_SIZE,
};

use super::BodyRef;

/// Tiles across a galactic sector.
pub const GALAXY_SECTOR_TILES: i64 = 1 << 20;
/// Position sectors ([`SECTOR_TILES`]) across a galactic sector.
pub const GALAXY_SECTOR_POSITION_SECTORS: i64 = GALAXY_SECTOR_TILES / SECTOR_TILES;
/// The home system's id ([`crate::star_chart`] ids are never zero).
pub const HOME_SYSTEM_ID: u64 = 0;
/// The home system.
pub const HOME_SYSTEM: SystemRef = SystemRef {
    id: HOME_SYSTEM_ID,
    sector: (0, 0),
};
/// The home world's body index.
pub const HOME_INDEX: u8 = 1;
/// The home world.
pub const HOME_BODY: BodyRef = BodyRef {
    system: HOME_SYSTEM,
    index: HOME_INDEX,
};
/// A system's station's body index.
pub const STATION_INDEX: u8 = 200;
/// Tiles from the home star to the home world.
pub const HOME_ORBIT_TILES: i64 = 60_000;
/// The home world's radius as seen from space (tiles), and its approach sphere's.
pub const HOME_RADIUS_TILES: f32 = 6_000.0;
pub const HOME_APPROACH_TILES: f32 = 8_500.0;
/// Tiles over the home world's approach sphere a rig leaving it appears at.
pub const HOME_LAUNCH_CLEARANCE_TILES: f32 = 300.0;
/// Most worlds a placeholder system has.
pub const MAX_WORLDS: u8 = 6;

/// A star system's reference: its id and galactic sector.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct SystemRef {
    /// Stable id ([`HOME_SYSTEM_ID`] for home, a chart's id for a charted system).
    pub id: u64,
    /// Galactic sector (space axes: y down).
    pub sector: (i64, i64),
}

impl SystemRef {
    /// Whether this is the home system.
    #[must_use]
    pub const fn is_home(self) -> bool {
        self.id == HOME_SYSTEM_ID
    }
}

/// What a body is.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum BodyKind {
    Star,
    Planet,
    /// The home world (handed back to its ground and sky on approach).
    Home,
    Station,
}

/// One body, as flight sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightBody {
    pub body: BodyRef,
    pub kind: BodyKind,
    pub centre: WorldPos,
    /// Surface radius (px).
    pub radius: f32,
    /// Gravity at the surface (px/s²; 0 for stations). It falls with the square of distance.
    pub surface_gravity: f32,
    /// Thickness of the air (or a star's corona) over the surface (px; 0 airless).
    pub atmosphere: f32,
    /// The approach sphere's radius (px): inside, the view zooms in.
    pub approach: f32,
    /// Heat a second at the surface regardless of speed (stars' coronas; 0 otherwise).
    pub corona_heat: f32,
    /// Stations: the angle the port faces out from the centre (radians, clockwise from up).
    pub port_angle: f32,
    /// A star's class (also the class of the system a body belongs to).
    pub class: StarClass,
}

impl FlightBody {
    /// Whether a rig can set down on it.
    #[must_use]
    pub const fn landable(&self) -> bool {
        matches!(self.kind, BodyKind::Planet)
    }

    /// Whether its air is breathable (a world with an atmosphere).
    #[must_use]
    pub fn breathable(&self) -> bool {
        self.landable() && self.atmosphere > 0.0
    }

    /// Gravity at `distance` px from the centre (px/s²).
    #[must_use]
    pub fn gravity_at(&self, distance: f32) -> f32 {
        if self.surface_gravity <= 0.0 {
            return 0.0;
        }
        let r = distance.max(self.radius).max(1.0);
        let ratio = self.radius / r;
        self.surface_gravity * ratio * ratio
    }

    /// The station's port (where a docked rig sits), `None` for other bodies.
    #[must_use]
    pub fn port(&self) -> Option<WorldPos> {
        if self.kind != BodyKind::Station {
            return None;
        }
        let (sin, cos) = crate::field::trig::sin_cos(self.port_angle);
        let reach = 2.0f32.mul_add(TILE_SIZE, self.radius);
        Some(self.centre.offset(sin * reach, -cos * reach))
    }

    /// The orientation a rig docks at: its up axis pointing into the port.
    #[must_use]
    pub fn dock_orientation(&self) -> f32 {
        crate::field::trig::wrap_angle(self.port_angle + std::f32::consts::PI)
    }
}

/// One star system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpaceSystem {
    pub system: SystemRef,
    pub star: WorldPos,
    pub class: StarClass,
    /// Worlds in orbit (the home system's count includes the home world).
    pub worlds: u8,
}

/// What flight reads of space (the seam the space generator implements).
pub trait SpaceQuery {
    /// The system in galactic sector `sector`, if any.
    fn system_at(&self, sector: (i64, i64)) -> Option<SpaceSystem>;

    /// Every body of `system` (star first, then worlds, then the station).
    fn bodies_of(&self, system: &SpaceSystem) -> Vec<FlightBody>;

    /// Every system within `reach` galactic sectors of `pos`'s sector (a square), in sector
    /// order.
    fn systems_near(&self, pos: WorldPos, reach: i64) -> Vec<SpaceSystem> {
        let (sx, sy) = galaxy_sector_of(pos);
        let mut found = Vec::new();
        for gy in sy.saturating_sub(reach)..=sy.saturating_add(reach) {
            for gx in sx.saturating_sub(reach)..=sx.saturating_add(reach) {
                if let Some(system) = self.system_at((gx, gy)) {
                    found.push(system);
                }
            }
        }
        found
    }

    /// Every body near `pos` (the systems of the 3x3 galactic sectors around it).
    fn bodies_near(&self, pos: WorldPos) -> Vec<FlightBody> {
        self.systems_near(pos, 1)
            .iter()
            .flat_map(|system| self.bodies_of(system))
            .collect()
    }

    /// The system `system` refers to, if it exists.
    fn system(&self, system: SystemRef) -> Option<SpaceSystem> {
        self.system_at(system.sector)
            .filter(|found| found.system.id == system.id)
    }

    /// One body.
    fn body(&self, body: BodyRef) -> Option<FlightBody> {
        let system = self.system(body.system)?;
        self.bodies_of(&system)
            .into_iter()
            .find(|found| found.body == body)
    }
}

/// The galactic sector containing `pos`.
#[must_use]
pub const fn galaxy_sector_of(pos: WorldPos) -> (i64, i64) {
    (
        pos.sector_x.div_euclid(GALAXY_SECTOR_POSITION_SECTORS),
        pos.sector_y.div_euclid(GALAXY_SECTOR_POSITION_SECTORS),
    )
}

/// The top-left corner of galactic sector `sector`.
#[must_use]
pub const fn galaxy_sector_origin(sector: (i64, i64)) -> WorldPos {
    WorldPos {
        sector_x: sector.0.saturating_mul(GALAXY_SECTOR_POSITION_SECTORS),
        sector_y: sector.1.saturating_mul(GALAXY_SECTOR_POSITION_SECTORS),
        x: 0.0,
        y: 0.0,
    }
}

/// `pos` moved by `(dx, dy)` px given as `f64` (exact whole sectors, then the rest): for moves
/// far longer than a sector (warp).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "whole sectors of a finite move fit an i64; the rest is under a sector"
)]
pub fn advance(pos: WorldPos, dx: f64, dy: f64) -> WorldPos {
    let sector = f64::from(SECTOR_PX);
    let split = |delta: f64| -> (i64, f32) {
        if !delta.is_finite() {
            return (0, 0.0);
        }
        let whole = (delta / sector).floor();
        let rest = whole.mul_add(-sector, delta);
        (whole as i64, rest as f32)
    };
    let (sx, rx) = split(dx);
    let (sy, ry) = split(dy);
    WorldPos {
        sector_x: pos.sector_x.saturating_add(sx),
        sector_y: pos.sector_y.saturating_add(sy),
        x: pos.x + rx,
        y: pos.y + ry,
    }
    .normalized()
}

/// `tiles` as px.
#[must_use]
pub fn tiles(tiles: f32) -> f32 {
    tiles * TILE_SIZE
}

/// The tile `(column, row)` (64-bit) as a space position.
#[must_use]
pub const fn at_tile(column: i64, row: i64) -> WorldPos {
    WorldPos::from_tile(column, row)
}

const SALT_SYSTEM: u64 = 0xF117_0001;
const SALT_BODY: u64 = 0xF117_0002;

/// A 64-bit hash of a seed, a salt and two 64-bit coordinates.
#[must_use]
pub const fn hash64(seed: u64, salt: u64, x: i64, y: i64) -> u64 {
    let mut z = seed
        ^ salt.wrapping_mul(0xA076_1D64_78BD_642F)
        ^ x.cast_unsigned().wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ y.cast_unsigned()
            .rotate_left(29)
            .wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The placeholder galaxy (see the module docs): home, the Archive's charted systems, and
/// procedural systems in about two sectors of three elsewhere.
#[derive(Clone, Copy, Debug)]
pub struct PlaceholderSpace<'a> {
    pub seed: u64,
    pub charts: &'a [StarChart],
}

impl<'a> PlaceholderSpace<'a> {
    #[must_use]
    pub const fn new(seed: u64, charts: &'a [StarChart]) -> Self {
        Self { seed, charts }
    }

    /// The galactic sector (space axes) of a chart (its sector's y is "up").
    #[must_use]
    pub fn chart_sector(chart: &StarChart) -> (i64, i64) {
        (i64::from(chart.sector.0), -i64::from(chart.sector.1))
    }
}

/// The home star's position.
#[must_use]
pub const fn home_star() -> WorldPos {
    at_tile(GALAXY_SECTOR_TILES / 2, GALAXY_SECTOR_TILES / 2)
}

/// The home world's centre as seen from space.
#[must_use]
pub const fn home_centre() -> WorldPos {
    at_tile(
        GALAXY_SECTOR_TILES / 2,
        GALAXY_SECTOR_TILES / 2 - HOME_ORBIT_TILES,
    )
}

/// Where a rig leaving the home world appears (straight "up" from it).
#[must_use]
pub fn home_launch() -> WorldPos {
    home_centre().offset(
        0.0,
        -tiles(HOME_APPROACH_TILES + HOME_LAUNCH_CLEARANCE_TILES),
    )
}

/// A star's look and size by class: (radius tiles, surface gravity px/s², corona heat).
const fn star_shape(class: StarClass) -> (f32, f32, f32) {
    match class {
        StarClass::Yellow => (3_500.0, 520.0, 1.6),
        StarClass::Red => (2_200.0, 380.0, 1.0),
        StarClass::Blue => (6_000.0, 700.0, 2.6),
        StarClass::White => (900.0, 900.0, 2.0),
        StarClass::Binary => (4_200.0, 620.0, 2.0),
        StarClass::Nebula => (3_000.0, 480.0, 1.4),
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "masked hash bits"
)]
const fn class_from(roll: u64) -> StarClass {
    StarClass::ALL[(roll % 6) as usize]
}

/// A uniform share in `0..1` from 16 hash bits.
#[allow(clippy::cast_precision_loss, reason = "16 masked bits")]
const fn share(roll: u64) -> f32 {
    (roll & 0xFFFF) as f32 / 65_536.0
}

impl SpaceQuery for PlaceholderSpace<'_> {
    fn system_at(&self, sector: (i64, i64)) -> Option<SpaceSystem> {
        if sector == (0, 0) {
            return Some(SpaceSystem {
                system: HOME_SYSTEM,
                star: home_star(),
                class: StarClass::Yellow,
                worlds: 4,
            });
        }
        let origin = galaxy_sector_origin(sector);
        #[allow(clippy::cast_precision_loss, reason = "a power of two")]
        let (middle, jitter) = (
            tiles(GALAXY_SECTOR_TILES as f32 * 0.5),
            tiles(GALAXY_SECTOR_TILES as f32 * 0.25),
        );
        let roll = hash64(self.seed, SALT_SYSTEM, sector.0, sector.1);
        // A charted system wins its sector (the first chart read for it).
        if let Some(chart) = self
            .charts
            .iter()
            .find(|chart| Self::chart_sector(chart) == sector)
        {
            let dx = (share(roll) - 0.5).mul_add(jitter, middle);
            let dy = (share(roll >> 16) - 0.5).mul_add(jitter, middle);
            return Some(SpaceSystem {
                system: SystemRef {
                    id: chart.system,
                    sector,
                },
                star: advance(origin, f64::from(dx), f64::from(dy)),
                class: chart.class,
                worlds: chart.worlds.clamp(1, MAX_WORLDS),
            });
        }
        if roll.is_multiple_of(3) {
            return None;
        }
        let dx = (share(roll >> 8) - 0.5).mul_add(jitter, middle);
        let dy = (share(roll >> 24) - 0.5).mul_add(jitter, middle);
        #[allow(clippy::cast_possible_truncation, reason = "masked")]
        let worlds = 1 + ((roll >> 40) % u64::from(MAX_WORLDS)) as u8;
        Some(SpaceSystem {
            system: SystemRef {
                id: roll | 1,
                sector,
            },
            star: advance(origin, f64::from(dx), f64::from(dy)),
            class: class_from(roll >> 48),
            worlds,
        })
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the star, the worlds and the station in one place"
    )]
    fn bodies_of(&self, system: &SpaceSystem) -> Vec<FlightBody> {
        let mut bodies = Vec::with_capacity(usize::from(system.worlds) + 2);
        let (star_radius, star_gravity, corona) = star_shape(system.class);
        let star_radius = tiles(star_radius);
        bodies.push(FlightBody {
            body: BodyRef {
                system: system.system,
                index: 0,
            },
            kind: BodyKind::Star,
            centre: system.star,
            radius: star_radius,
            surface_gravity: star_gravity,
            atmosphere: star_radius * 0.6,
            approach: star_radius * 1.6,
            corona_heat: corona,
            port_angle: 0.0,
            class: system.class,
        });
        let mut first_world = system.star;
        for index in 1..=system.worlds {
            let roll = hash64(self.seed ^ system.system.id, SALT_BODY, i64::from(index), 0);
            let (centre, radius, gravity, air) = if system.system.is_home() && index == HOME_INDEX {
                (
                    home_centre(),
                    tiles(HOME_RADIUS_TILES),
                    50.0,
                    tiles(HOME_APPROACH_TILES - HOME_RADIUS_TILES),
                )
            } else {
                let orbit =
                    tiles(f32::from(index).mul_add(24_000.0, 8_000.0) + share(roll) * 8_000.0);
                let angle = share(roll >> 16) * std::f32::consts::TAU;
                let (sin, cos) = crate::field::trig::sin_cos(angle);
                let radius = tiles(share(roll >> 32).mul_add(2_600.0, 400.0));
                let air = if (roll >> 48).is_multiple_of(2) {
                    (radius * 0.35).max(tiles(200.0))
                } else {
                    0.0
                };
                (
                    system.star.offset(sin * orbit, -cos * orbit),
                    radius,
                    share(roll >> 24).mul_add(260.0, 180.0),
                    air,
                )
            };
            if index == 1 {
                first_world = centre;
            }
            let kind = if system.system.is_home() && index == HOME_INDEX {
                BodyKind::Home
            } else {
                BodyKind::Planet
            };
            let approach = if kind == BodyKind::Home {
                tiles(HOME_APPROACH_TILES)
            } else {
                radius + air.max(tiles(300.0)) + tiles(200.0)
            };
            bodies.push(FlightBody {
                body: BodyRef {
                    system: system.system,
                    index,
                },
                kind,
                centre,
                radius,
                surface_gravity: gravity,
                atmosphere: air,
                approach,
                corona_heat: 0.0,
                port_angle: 0.0,
                class: system.class,
            });
        }
        // One station, near the first world (near home: a short hop from the launch point).
        let roll = hash64(self.seed ^ system.system.id, SALT_BODY, 0, 1);
        let station = if system.system.is_home() {
            home_centre().offset(tiles(10_500.0), -tiles(4_000.0))
        } else {
            let angle = share(roll) * std::f32::consts::TAU;
            let (sin, cos) = crate::field::trig::sin_cos(angle);
            let reach = tiles(6_000.0);
            first_world.offset(sin * reach, -cos * reach)
        };
        bodies.push(FlightBody {
            body: BodyRef {
                system: system.system,
                index: STATION_INDEX,
            },
            kind: BodyKind::Station,
            centre: station,
            radius: tiles(5.0),
            surface_gravity: 0.0,
            atmosphere: 0.0,
            approach: tiles(60.0),
            corona_heat: 0.0,
            port_angle: share(roll >> 16) * std::f32::consts::TAU,
            class: system.class,
        });
        bodies
    }
}

/// Gravity at `pos` from `bodies` (px/s², summed: it blends between nearby bodies) and the
/// index of the body pulling hardest (the local "down").
#[must_use]
pub fn gravity_at(bodies: &[FlightBody], pos: WorldPos) -> ((f32, f32), Option<usize>) {
    let mut total = (0.0_f32, 0.0_f32);
    let mut strongest: Option<(usize, f32)> = None;
    for (index, body) in bodies.iter().enumerate() {
        if body.surface_gravity <= 0.0 {
            continue;
        }
        let (dx, dy) = pos.delta(body.centre);
        let distance = dx.hypot(dy);
        let pull = body.gravity_at(distance);
        if pull <= 0.0 || distance <= 0.0 {
            continue;
        }
        total.0 = (dx / distance).mul_add(pull, total.0);
        total.1 = (dy / distance).mul_add(pull, total.1);
        if strongest.is_none_or(|(_, best)| pull > best) {
            strongest = Some((index, pull));
        }
    }
    (total, strongest.map(|(index, _)| index))
}

/// The distance from `pos` to the nearest surface among `bodies` (px; infinite with none),
/// and that body's index.
#[must_use]
pub fn nearest_surface(bodies: &[FlightBody], pos: WorldPos) -> (f32, Option<usize>) {
    let mut best = (f32::INFINITY, None);
    for (index, body) in bodies.iter().enumerate() {
        let gap = pos.distance(body.centre) - body.radius;
        if gap < best.0 {
            best = (gap, Some(index));
        }
    }
    best
}
