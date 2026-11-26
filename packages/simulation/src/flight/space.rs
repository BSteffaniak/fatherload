//! The space the flight model flies through: star systems, their bodies and stations, and
//! the gravity they pull with.
//!
//! **Wired to the space generator (Wave 11 integration).** [`GeneratedSpace`] reads the real
//! galaxy ([`crate::space::Space`], one shared cache per world seed) and shows flight the part
//! it needs as [`FlightBody`]s: every star, every body (the home world and the Moon, planets,
//! moons and crater rocks) and every station of the systems near the rig. [`SpaceQuery`] is the
//! whole interface flight reads.
//!
//! * Everything is a pure function of the world seed (the generator's) and the Archive's star
//!   charts resolve to the generator's systems ([`GeneratedSpace::chart_system`]).
//! * Positions are space pixels ([`WorldPos`], 32 to a tile), the generator's own frame. A
//!   galactic sector is the generator's space sector ([`GALAXY_SECTOR_TILES`]); the home system
//!   is sector (0, 0).
//! * Orbits are static (a body's position never moves), so a body list is valid for a tick.
//! * A [`BodyRef`]'s index is the star (0, a twin's second sun 1), the generator's body index
//!   plus [`FIRST_BODY_INDEX`], or a station's index plus [`STATION_INDEX`]
//!   ([`GeneratedSpace::space_body`] maps it back to the generator's [`SpaceBodyId`]).

use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use crate::{
    position::{SECTOR_PX, SECTOR_TILES, WorldPos},
    space::{Space, SpaceBodyId, SpaceSector, StarSystem},
    star_chart::{StarChart, StarClass},
    terrain::TILE_SIZE,
};

use super::{
    BodyRef,
    systems::{BodyExtra, CometFacts, CometKey, HoleFacts, MouthFacts, StarFacts},
};

/// Tiles across a galactic sector (the space generator's sector).
pub const GALAXY_SECTOR_TILES: i64 = crate::space::SPACE_SECTOR_TILES;
/// Position sectors ([`SECTOR_TILES`]) across a galactic sector.
pub const GALAXY_SECTOR_POSITION_SECTORS: i64 = GALAXY_SECTOR_TILES / SECTOR_TILES;
/// The home system's id (charted systems' ids are never zero).
pub const HOME_SYSTEM_ID: u64 = 0;
/// The home system.
pub const HOME_SYSTEM: SystemRef = SystemRef {
    id: HOME_SYSTEM_ID,
    sector: (0, 0),
};
/// The first [`BodyRef::index`] of a system's bodies (below it: its suns).
pub const FIRST_BODY_INDEX: u8 = 2;
/// The home world's body index.
pub const HOME_INDEX: u8 = FIRST_BODY_INDEX + crate::space::HOME_WORLD_BODY.body;
/// The Moon's body index (seen from space; its ground is the home world's sky).
pub const MOON_INDEX: u8 = FIRST_BODY_INDEX + crate::space::MOON_BODY.body;
/// The Moon (Wave 12 systems: coming down to it lands on it).
pub const MOON_BODY_REF: BodyRef = BodyRef {
    system: HOME_SYSTEM,
    index: MOON_INDEX,
};
/// The body index of a black hole, a wormhole mouth, a belt's rock and a comet (Wave 12
/// systems; their [`BodyRef::system`] carries their own id and sector).
pub const HOLE_INDEX: u8 = 250;
pub const MOUTH_INDEX: u8 = 251;
pub const ROCK_INDEX: u8 = 252;
/// A system's first comet's body index (the rest follow, up to [`HOLE_INDEX`]).
pub const COMET_INDEX: u8 = 230;
/// The home world.
pub const HOME_BODY: BodyRef = BodyRef {
    system: HOME_SYSTEM,
    index: HOME_INDEX,
};
/// A system's first station's body index (the rest follow).
pub const STATION_INDEX: u8 = 200;
/// The home world's radius as seen from space (tiles), and its approach sphere's: flying into
/// it hands the rig back to the home sky.
pub const HOME_RADIUS_TILES: f32 = 10_000.0;
pub const HOME_APPROACH_TILES: f32 = 11_500.0;
/// The Moon's approach sphere over its surface (tiles): flying into it hands the rig back to
/// the home sky over the Moon (the departure point is just outside it).
pub const MOON_APPROACH_TILES: f32 = 150.0;
/// Gravity a body of 1 g pulls with in flight (px/s²): the rig's own on the home world, so a
/// planet's pull in flight matches its ground's.
pub const FLIGHT_G_PX: f32 = crate::world::AUTHORITATIVE_RIG_GRAVITY;

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
    /// Wave 12 systems: a black hole (its radius the event horizon).
    BlackHole,
    /// Wave 12 systems: a wormhole's mouth (fly in to cross to its pair).
    Wormhole,
    /// Wave 12 systems: a belt's rock (solid, mineable from flight).
    Asteroid,
    /// Wave 12 systems: a comet's nucleus (its tail holds dust).
    Comet,
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
    /// What else it does (Wave 12 systems: stars' heat and flares, black holes, wormhole
    /// mouths, rocks and comets).
    pub extra: super::systems::BodyExtra,
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

    /// Sensor fog at `pos`, per mille (nebulae; 0 where the space has none).
    fn fog_at(&self, pos: WorldPos) -> u16 {
        let _ = pos;
        0
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

const SALT_PORT: u64 = 0xF117_0003;
const SALT_MOUTH: u64 = 0xF117_0004;

/// The generated galaxy as flight sees it (see the module docs).
#[derive(Clone, Debug)]
pub struct GeneratedSpace {
    space: Arc<Space>,
}

/// The home system's fixed layout (the same for every seed: no jitter, fixed orbits).
struct HomeLayout {
    star: WorldPos,
    home: WorldPos,
    moon: WorldPos,
    departure: WorldPos,
    dock: WorldPos,
}

fn home_layout() -> &'static HomeLayout {
    static LAYOUT: OnceLock<HomeLayout> = OnceLock::new();
    LAYOUT.get_or_init(|| {
        let space = Space::new(0);
        let system = space.home_system();
        let body = |index: u8| {
            system
                .bodies
                .get(usize::from(index))
                .map_or_else(|| system.centre.pos(), |body| body.centre.pos())
        };
        HomeLayout {
            star: system.centre.pos(),
            home: body(crate::space::HOME_WORLD_BODY.body),
            moon: body(crate::space::MOON_BODY.body),
            departure: space.home_departure(),
            dock: system
                .stations
                .first()
                .map_or_else(|| system.centre.pos(), |station| station.centre.pos()),
        }
    })
}

/// The home star's position.
#[must_use]
pub fn home_star() -> WorldPos {
    home_layout().star
}

/// The home world's centre as seen from space.
#[must_use]
pub fn home_centre() -> WorldPos {
    home_layout().home
}

/// The Moon's centre as seen from space.
#[must_use]
pub fn moon_centre() -> WorldPos {
    home_layout().moon
}

/// Where a rig leaving the home world appears: the space generator's departure point, just
/// above the Moon, straight "up" from the home world.
#[must_use]
pub fn home_launch() -> WorldPos {
    home_layout().departure
}

/// Halcyon Dock, the home system's station.
#[must_use]
pub fn home_dock() -> WorldPos {
    home_layout().dock
}

/// A star's flight figures by class: (surface gravity px/s², corona heat).
const fn star_shape(class: StarClass) -> (f32, f32) {
    match class {
        StarClass::Yellow => (520.0, 1.6),
        StarClass::Red => (380.0, 1.0),
        StarClass::Blue => (700.0, 2.6),
        StarClass::White => (900.0, 2.0),
        StarClass::Binary => (620.0, 2.0),
        StarClass::Nebula => (480.0, 1.4),
    }
}

/// A uniform share in `0..1` from 16 hash bits.
#[allow(clippy::cast_precision_loss, reason = "16 masked bits")]
const fn share(roll: u64) -> f32 {
    (roll & 0xFFFF) as f32 / 65_536.0
}

impl GeneratedSpace {
    /// The galaxy of the world seeded `seed` (the shared cache).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            space: Space::shared(seed),
        }
    }

    /// The space generator itself (nebulae, black holes, belts and the rest the renderer and
    /// the nav HUD read).
    #[must_use]
    pub fn space(&self) -> &Space {
        &self.space
    }

    /// Flight's reference to the generator's system `system` of `sector`.
    #[must_use]
    pub fn reference(sector: SpaceSector, system: &StarSystem) -> SystemRef {
        SystemRef {
            id: if system.home {
                HOME_SYSTEM_ID
            } else {
                system.id
            },
            sector: (i64::from(sector.x), i64::from(sector.y)),
        }
    }

    /// The generator's sector of flight's galactic sector `sector` (`None` past the
    /// generator's `i32` range).
    fn space_sector(sector: (i64, i64)) -> Option<SpaceSector> {
        Some(SpaceSector::new(
            i32::try_from(sector.0).ok()?,
            i32::try_from(sector.1).ok()?,
        ))
    }

    /// The generator's system `system` refers to, with its index in its sector.
    #[must_use]
    pub fn star_system(&self, system: SystemRef) -> Option<StarSystem> {
        let sector = Self::space_sector(system.sector)?;
        self.space
            .sector(sector)
            .systems
            .iter()
            .find(|found| Self::reference(sector, found) == system)
            .cloned()
    }

    /// The system a star chart leads to (the generator resolves it, legacy charts too).
    #[must_use]
    pub fn chart_system(&self, chart: &StarChart) -> SystemRef {
        let resolved = self.space.chart_system(chart);
        self.space.system(resolved).map_or_else(
            || SystemRef {
                id: chart.system,
                sector: (i64::from(resolved.sector.x), i64::from(resolved.sector.y)),
            },
            |system| Self::reference(resolved.sector, &system),
        )
    }

    /// The generator's id of a flight body (`None` for suns and stations).
    #[must_use]
    pub fn space_body(&self, body: BodyRef) -> Option<SpaceBodyId> {
        if !(FIRST_BODY_INDEX..STATION_INDEX).contains(&body.index) {
            return None;
        }
        let sector = Self::space_sector(body.system.sector)?;
        let system = self.star_system(body.system)?;
        let index = body.index - FIRST_BODY_INDEX;
        (usize::from(index) < system.bodies.len()).then_some(SpaceBodyId {
            sector,
            system: system.index,
            body: index,
        })
    }

    /// Flight's reference to the generator's body `id`.
    #[must_use]
    pub fn body_ref(&self, id: SpaceBodyId) -> Option<BodyRef> {
        let system = self.space.system(id.system_ref())?;
        let index = FIRST_BODY_INDEX.checked_add(id.body)?;
        (index < STATION_INDEX).then_some(BodyRef {
            system: Self::reference(id.sector, &system),
            index,
        })
    }

    fn summary(sector: SpaceSector, system: &StarSystem) -> SpaceSystem {
        SpaceSystem {
            system: Self::reference(sector, system),
            star: system.centre.pos(),
            class: system.class,
            worlds: u8::try_from(system.planet_count()).unwrap_or(u8::MAX),
        }
    }
}

impl SpaceQuery for GeneratedSpace {
    fn fog_at(&self, pos: WorldPos) -> u16 {
        self.space.fog_at(pos)
    }

    fn system_at(&self, sector: (i64, i64)) -> Option<SpaceSystem> {
        let coords = Self::space_sector(sector)?;
        let found = self.space.sector(coords);
        found
            .systems
            .first()
            .map(|system| Self::summary(coords, system))
    }

    fn systems_near(&self, pos: WorldPos, reach: i64) -> Vec<SpaceSystem> {
        let (sx, sy) = galaxy_sector_of(pos);
        let mut found = Vec::new();
        for gy in sy.saturating_sub(reach)..=sy.saturating_add(reach) {
            for gx in sx.saturating_sub(reach)..=sx.saturating_add(reach) {
                let Some(coords) = Self::space_sector((gx, gy)) else {
                    continue;
                };
                let sector = self.space.sector(coords);
                found.extend(
                    sector
                        .systems
                        .iter()
                        .map(|system| Self::summary(coords, system)),
                );
            }
        }
        found
    }

    fn system(&self, system: SystemRef) -> Option<SpaceSystem> {
        let sector = Self::space_sector(system.sector)?;
        self.star_system(system)
            .map(|found| Self::summary(sector, &found))
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::too_many_lines,
        reason = "radii of at most a few hundred thousand tiles; suns, bodies and stations in one place"
    )]
    fn bodies_of(&self, system: &SpaceSystem) -> Vec<FlightBody> {
        let Some(found) = self.star_system(system.system) else {
            return Vec::new();
        };
        let mut bodies = Vec::with_capacity(found.stars.len() + found.bodies.len() + 2);
        let (star_gravity, corona) = star_shape(found.class);
        for (index, star) in found
            .stars
            .iter()
            .take(usize::from(FIRST_BODY_INDEX))
            .enumerate()
        {
            let radius = tiles(star.radius as f32);
            bodies.push(FlightBody {
                body: BodyRef {
                    system: system.system,
                    index: u8::try_from(index).unwrap_or(0),
                },
                kind: BodyKind::Star,
                centre: star.centre.pos(),
                radius,
                surface_gravity: star_gravity,
                atmosphere: radius * 0.6,
                approach: radius * 1.6,
                corona_heat: corona,
                port_angle: 0.0,
                class: found.class,
                extra: BodyExtra::Star(StarFacts {
                    heat_radius: tiles(star.heat_radius as f32),
                    light: star.light,
                    flare_period_s: star.flare_period_s,
                    flare_phase_s: star.flare_phase_s,
                }),
            });
        }
        for body in &found.bodies {
            let Some(index) = FIRST_BODY_INDEX
                .checked_add(body.index)
                .filter(|index| *index < STATION_INDEX)
            else {
                continue;
            };
            let radius = tiles(body.radius as f32);
            let (kind, atmosphere, approach) = match body.kind {
                crate::space::BodyKind::HomeWorld => (
                    BodyKind::Home,
                    tiles(HOME_APPROACH_TILES - HOME_RADIUS_TILES),
                    tiles(HOME_APPROACH_TILES),
                ),
                crate::space::BodyKind::HomeMoon => {
                    (BodyKind::Home, 0.0, radius + tiles(MOON_APPROACH_TILES))
                }
                crate::space::BodyKind::Planet
                | crate::space::BodyKind::Moon
                | crate::space::BodyKind::CraterRock => {
                    let air = if matches!(body.atmosphere, crate::space::Atmosphere::Vacuum) {
                        0.0
                    } else {
                        (radius * 0.35).max(tiles(200.0))
                    };
                    (
                        BodyKind::Planet,
                        air,
                        radius + air.max(tiles(300.0)) + tiles(200.0),
                    )
                }
            };
            bodies.push(FlightBody {
                body: BodyRef {
                    system: system.system,
                    index,
                },
                kind,
                centre: body.centre.pos(),
                radius,
                surface_gravity: body.gravity_g() * FLIGHT_G_PX,
                atmosphere,
                approach,
                corona_heat: 0.0,
                port_angle: 0.0,
                class: found.class,
                extra: BodyExtra::None,
            });
        }
        for station in &found.stations {
            let Some(index) = STATION_INDEX.checked_add(station.id.index) else {
                continue;
            };
            let roll = hash64(
                found.id,
                SALT_PORT,
                i64::from(station.id.index),
                i64::from(station.planet),
            );
            // Halcyon Dock's port faces the departure point (down toward the Moon), so a rig
            // leaving home flies straight in.
            let port_angle = if found.home && station.id.index == 0 {
                std::f32::consts::PI
            } else {
                share(roll) * std::f32::consts::TAU
            };
            bodies.push(FlightBody {
                body: BodyRef {
                    system: system.system,
                    index,
                },
                kind: BodyKind::Station,
                centre: station.centre.pos(),
                radius: tiles(5.0),
                surface_gravity: 0.0,
                atmosphere: 0.0,
                approach: tiles(60.0),
                corona_heat: 0.0,
                port_angle,
                class: found.class,
                extra: BodyExtra::None,
            });
        }
        // Comets (Wave 12 systems): a solid nucleus and a tail to gather dust in.
        for (index, comet) in found.comets.iter().enumerate() {
            let Some(slot) = u8::try_from(index)
                .ok()
                .and_then(|index| COMET_INDEX.checked_add(index))
                .filter(|slot| *slot < HOLE_INDEX)
            else {
                continue;
            };
            bodies.push(FlightBody {
                body: BodyRef {
                    system: system.system,
                    index: slot,
                },
                kind: BodyKind::Comet,
                centre: comet.centre.pos(),
                radius: tiles(f32::from(comet.nucleus_radius)),
                surface_gravity: 0.0,
                atmosphere: 0.0,
                approach: 0.0,
                corona_heat: 0.0,
                port_angle: 0.0,
                class: found.class,
                extra: BodyExtra::Comet(CometFacts {
                    key: CometKey {
                        system: system.system,
                        index: slot - COMET_INDEX,
                    },
                    tail_end: comet.tail_end.pos(),
                    tail_width: tiles(comet.tail_width as f32),
                }),
            });
        }
        bodies
    }

    fn bodies_near(&self, pos: WorldPos) -> Vec<FlightBody> {
        let mut bodies: Vec<FlightBody> = self
            .systems_near(pos, 1)
            .iter()
            .flat_map(|system| self.bodies_of(system))
            .collect();
        bodies.extend(self.interstellar_near(pos));
        bodies
    }
}

impl GeneratedSpace {
    /// The black holes and wormhole mouths of the 3 x 3 galactic sectors round `pos` as flight
    /// bodies (Wave 12 systems).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "radii of a few tens of thousands of tiles"
    )]
    pub fn interstellar_near(&self, pos: WorldPos) -> Vec<FlightBody> {
        let (sx, sy) = galaxy_sector_of(pos);
        let mut bodies = Vec::new();
        for gy in sy.saturating_sub(1)..=sy.saturating_add(1) {
            for gx in sx.saturating_sub(1)..=sx.saturating_add(1) {
                let Some(coords) = Self::space_sector((gx, gy)) else {
                    continue;
                };
                let sector = self.space.sector(coords);
                for hole in &sector.black_holes {
                    let radius = tiles(hole.horizon as f32);
                    let lensing = tiles(hole.lensing_radius as f32);
                    bodies.push(FlightBody {
                        body: BodyRef {
                            system: SystemRef {
                                id: hole.id,
                                sector: (gx, gy),
                            },
                            index: HOLE_INDEX,
                        },
                        kind: BodyKind::BlackHole,
                        centre: hole.centre.pos(),
                        radius,
                        surface_gravity: hole.gravity_milli_g as f32 / 1_000.0 * FLIGHT_G_PX,
                        atmosphere: 0.0,
                        approach: lensing,
                        corona_heat: 0.0,
                        port_angle: 0.0,
                        class: StarClass::default(),
                        extra: BodyExtra::Hole(HoleFacts {
                            lensing,
                            time_dilation: f32::from(hole.time_dilation) / 1_000.0,
                        }),
                    });
                }
                for mouth in &sector.wormholes {
                    let radius = tiles(mouth.radius as f32);
                    let exit_radius = self
                        .space
                        .sector(mouth.exit_sector)
                        .wormholes
                        .first()
                        .map_or(radius, |far| tiles(far.radius as f32));
                    bodies.push(FlightBody {
                        body: BodyRef {
                            system: SystemRef {
                                id: hash64(0, SALT_MOUTH, gx, gy),
                                sector: (gx, gy),
                            },
                            index: MOUTH_INDEX,
                        },
                        kind: BodyKind::Wormhole,
                        centre: mouth.centre.pos(),
                        radius,
                        surface_gravity: 0.0,
                        atmosphere: 0.0,
                        approach: radius * 4.0,
                        corona_heat: 0.0,
                        port_angle: 0.0,
                        class: StarClass::default(),
                        extra: BodyExtra::Mouth(MouthFacts {
                            exit: mouth.exit.pos(),
                            exit_radius,
                            sector: (gx, gy),
                            exit_sector: (
                                i64::from(mouth.exit_sector.x),
                                i64::from(mouth.exit_sector.y),
                            ),
                        }),
                    });
                }
            }
        }
        bodies
    }

    /// The belt rocks within `reach_tiles` of `pos` that the crew has not mined out, as flight
    /// bodies (Wave 12 systems).
    #[must_use]
    pub fn rocks_near(
        &self,
        pos: WorldPos,
        reach_tiles: i64,
        mined: &super::systems::SystemsShared,
    ) -> Vec<FlightBody> {
        self.space
            .asteroids_near(pos, reach_tiles)
            .iter()
            .filter_map(|rock| super::systems::rock_body(rock, mined.mined_from(rock.id)))
            .collect()
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
        let distance = crate::field::trig::hypot(dx, dy);
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
        // Rocks and comets do not pace the cruise (they bump it out instead; Wave 12).
        if matches!(body.kind, BodyKind::Asteroid | BodyKind::Comet) {
            continue;
        }
        let gap = pos.span(body.centre) - body.radius;
        if gap < best.0 {
            best = (gap, Some(index));
        }
    }
    best
}
