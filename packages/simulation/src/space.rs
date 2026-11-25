//! Seamless space (expansion plan §15 F12, Wave 11 space-gen): everything above the Moon,
//! generated deterministically from the world seed per **space sector**.
//!
//! This module is pure data plus queries: no rendering, no simulation state. A sector's
//! contents are a pure function of `(world seed, sector coordinates)` ([`generate_sector`]),
//! so every peer, every save and every frontend agrees on where every star, planet, moon,
//! belt, comet, nebula, derelict, station, black hole and wormhole is without anything being
//! saved or sent. [`Space`] wraps the generator with a bounded sector cache and the query API
//! the other space tracks read (systems and bodies near a point, a body by id, gravity sources,
//! the nearest station, the scanner, fog, time dilation, asteroids).
//!
//! # Units and coordinates
//!
//! Space positions are **absolute tiles** as `i64` pairs ([`SpaceTile`]), exact everywhere,
//! converted to the game's [`WorldPos`] (sector + `f32` offset) with [`SpaceTile::pos`] and
//! back with [`SpaceTile::of_pos`]. Never `f32` absolute coordinates: a far system sits 10^11
//! tiles out. y grows **down**, as in the world.
//!
//! A space sector is [`SPACE_SECTOR_TILES`] (2^22 = 4,194,304) tiles a side, which is
//! [`SPACE_SECTOR_WORLD_SECTORS`] (8,192) [`WorldPos`] sectors. It is sized for the Star Drive:
//! a star system fits in one quarter-cell of a sector (at most [`SYSTEM_MAX_EXTENT`] tiles from
//! its star), so **cruise** (a minute or two across a system, ~10^4 tiles/s at the top of its
//! ramp) stays inside a sector or two, and **warp** crosses sectors (the Archive's charts lie
//! 3-48 sectors out, `star_chart::CHART_RING`).
//!
//! # A sector
//!
//! A sector is split into 2 x 2 **cells** ([`CELL_TILES`] a side). One to three cells hold a
//! star system (never overlapping: a system stays inside its cell); the free cells hold the
//! interstellar things: a wormhole mouth, a black hole, drifting derelicts. Nebulae are bigger
//! than sectors and come from their own coarse grid ([`NEBULA_CELL_SECTORS`]); wormholes are
//! paired across regions of [`WORMHOLE_REGION_SECTORS`] sectors by an involution, so both
//! mouths of a pair are found from either side without any search.
//!
//! # Home
//!
//! The home system sits alone in [`HOME_SECTOR`] (`star_chart::HOME_SECTOR`): a yellow sun,
//! the home world ([`BodyKind::HomeWorld`], body 0) and the Moon ([`BodyKind::HomeMoon`],
//! body 1) straight "up" from it, then generated sister planets. The home world's ground and the
//! Moon stay what they are (their own places); in space they are bodies with a nominal radius,
//! gravity and position for the map, the scanner and the crossing. A craft leaving the top of
//! the home world's sky enters space at [`Space::home_departure`], just above the Moon.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    reason = "space generation mixes hashed bits, bounded integer ranges and f64 distances"
)]

mod asteroids;
mod generate;
mod home;
mod interstellar;
mod model;
mod query;
pub mod report;
#[cfg(test)]
mod tests;
mod trig;

use serde::{Deserialize, Serialize};

pub use asteroids::MAX_ASTEROIDS_PER_QUERY;
pub use asteroids::{asteroids_in_segment, belt_segments};
pub use generate::{
    ChartTarget, chart_target, generate_sector, resolve_chart, surface_columns, system_uid,
};
pub use home::{
    DEPARTURE_ABOVE_MOON, HOME_ORBIT, HOME_SUN_RADIUS, HOME_WORLD_BODY, HOME_WORLD_RADIUS,
    MOON_BODY, MOON_ORBIT, MOON_RADIUS,
};
pub use model::*;
pub use query::{
    BodyHit, CACHE_SECTORS, ContactKind, GravityKind, GravitySource, MAX_QUERY_RADIUS,
    MIN_PULL_MILLI_G, ScanContact, Space, StationHit,
};
pub use trig::{ONE as TRIG_ONE, TURN, cos_sin};

use crate::position::{SECTOR_TILES, WorldPos};

/// Tiles per space sector side (2^22).
pub const SPACE_SECTOR_TILES: i64 = 1 << 22;
/// [`WorldPos`] sectors per space sector side.
pub const SPACE_SECTOR_WORLD_SECTORS: i64 = SPACE_SECTOR_TILES / SECTOR_TILES;
/// Tiles per cell side (a sector is 2 x 2 cells; a system lives inside one).
pub const CELL_TILES: i64 = SPACE_SECTOR_TILES / 2;
/// Farthest anything of a system lies from its centre (its star or its stars' barycentre).
pub const SYSTEM_MAX_EXTENT: i64 = CELL_TILES / 2 - 48_000;
/// Smallest radius you can land on (§15 F12): moons and crater rocks 400-1,000 tiles, planets
/// 1,500-10,000. Anything smaller is an asteroid mined from flight.
pub const MIN_LANDABLE_RADIUS: u32 = 400;
/// Planet radii, tiles (inclusive).
pub const PLANET_RADIUS: (u32, u32) = (1_500, 10_000);
/// Moon and crater-rock radii, tiles (inclusive).
pub const SMALL_BODY_RADIUS: (u32, u32) = (400, 1_000);
/// The largest asteroid, tiles (one under the least landable radius).
pub const ASTEROID_MAX_RADIUS: u16 = 399;
/// Nebula grid cells, sectors a side.
pub const NEBULA_CELL_SECTORS: i32 = 4;
/// Wormhole regions, sectors a side (each region holds at most one mouth).
pub const WORMHOLE_REGION_SECTORS: i32 = 8;
/// The home system's sector.
pub const HOME_SECTOR: SpaceSector = SpaceSector {
    x: crate::star_chart::HOME_SECTOR.0,
    y: crate::star_chart::HOME_SECTOR.1,
};

/// Pins the space generator (Wave 11 space-gen).
///
/// The `StateHasher` digest of the 25 sectors within two of home on the default world seed.
/// Change it only on purpose, and say so: every space position moves with it.
pub const GOLDEN_SPACE_DIGEST: u64 = 0xf303_d220_59a6_d7db;

/// Whether worlds of `generator` have space (V11, the Deep Descent, and later only).
#[must_use]
pub const fn has_space(generator: crate::worldgen::GeneratorVersion) -> bool {
    generator.has_descents()
}

/// A space sector's coordinates (home at `(0, 0)`, x right, y down).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct SpaceSector {
    pub x: i32,
    pub y: i32,
}

impl SpaceSector {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The sector containing `tile`.
    #[must_use]
    pub const fn of_tile(tile: SpaceTile) -> Self {
        Self {
            x: tile.x.div_euclid(SPACE_SECTOR_TILES) as i32,
            y: tile.y.div_euclid(SPACE_SECTOR_TILES) as i32,
        }
    }

    /// The sector containing `pos`.
    #[must_use]
    pub fn of_pos(pos: WorldPos) -> Self {
        Self::of_tile(SpaceTile::of_pos(pos))
    }

    /// Its top-left tile.
    #[must_use]
    pub const fn origin(self) -> SpaceTile {
        SpaceTile {
            x: self.x as i64 * SPACE_SECTOR_TILES,
            y: self.y as i64 * SPACE_SECTOR_TILES,
        }
    }

    /// Its centre tile.
    #[must_use]
    pub const fn centre(self) -> SpaceTile {
        let origin = self.origin();
        SpaceTile {
            x: origin.x + SPACE_SECTOR_TILES / 2,
            y: origin.y + SPACE_SECTOR_TILES / 2,
        }
    }

    /// The centre of cell `cell` (0-3: left-top, right-top, left-bottom, right-bottom).
    #[must_use]
    pub const fn cell_centre(self, cell: u8) -> SpaceTile {
        let origin = self.origin();
        SpaceTile {
            x: origin.x + (cell & 1) as i64 * CELL_TILES + CELL_TILES / 2,
            y: origin.y + (cell >> 1) as i64 * CELL_TILES + CELL_TILES / 2,
        }
    }

    /// Chebyshev distance to `other`, sectors.
    #[must_use]
    pub const fn chebyshev(self, other: Self) -> i64 {
        let dx = (other.x as i64 - self.x as i64).abs();
        let dy = (other.y as i64 - self.y as i64).abs();
        if dx > dy { dx } else { dy }
    }

    /// Euclidean distance to `other`, sectors (the same bits on every platform: see
    /// [`SpaceTile::distance`]).
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        let dx = i128::from(other.x) - i128::from(self.x);
        let dy = i128::from(other.y) - i128::from(self.y);
        ((dx * dx + dy * dy) as f64).sqrt()
    }

    /// Squared distance from `tile` to this sector's square, tiles² (0 inside).
    #[must_use]
    pub fn distance2_to(self, tile: SpaceTile) -> i128 {
        let origin = self.origin();
        let clamp = |value: i64, low: i64| -> i128 {
            let high = low + SPACE_SECTOR_TILES;
            if value < low {
                (low - value) as i128
            } else if value > high {
                (value - high) as i128
            } else {
                0
            }
        };
        let dx = clamp(tile.x, origin.x);
        let dy = clamp(tile.y, origin.y);
        dx * dx + dy * dy
    }
}

/// An absolute space position in whole tiles (exact; see the module docs).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct SpaceTile {
    pub x: i64,
    pub y: i64,
}

impl SpaceTile {
    #[must_use]
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    /// The tile containing `pos`.
    #[must_use]
    pub fn of_pos(pos: WorldPos) -> Self {
        let (x, y) = pos.tile();
        Self { x, y }
    }

    /// This tile's top-left corner as a [`WorldPos`] (exact).
    #[must_use]
    pub const fn pos(self) -> WorldPos {
        WorldPos::from_tile(self.x, self.y)
    }

    /// This tile moved by `(dx, dy)` tiles.
    #[must_use]
    pub const fn offset(self, dx: i64, dy: i64) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }

    /// `other - self`, tiles.
    #[must_use]
    pub const fn delta(self, other: Self) -> (i64, i64) {
        (other.x - self.x, other.y - self.y)
    }

    /// Squared distance to `other`, tiles² (exact).
    #[must_use]
    pub const fn distance2(self, other: Self) -> i128 {
        let dx = (other.x - self.x) as i128;
        let dy = (other.y - self.y) as i128;
        dx * dx + dy * dy
    }

    /// Distance to `other`, tiles.
    ///
    /// The same bits on every platform: the exact squared distance rounded once to `f64`,
    /// then an IEEE square root (correctly rounded everywhere). `f64::hypot` comes from the
    /// platform's libm (macOS, glibc and the browser build's musl may differ in the last
    /// bit), so space never uses it.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        (self.distance2(other) as f64).sqrt()
    }

    /// Distance to `other`, tiles, rounded up (exact integers).
    #[must_use]
    pub const fn distance_ceil(self, other: Self) -> i64 {
        let d2 = self.distance2(other).unsigned_abs();
        let root = d2.isqrt();
        (if root * root == d2 { root } else { root + 1 }) as i64
    }

    /// Whether `other` lies within `radius` tiles (exact).
    #[must_use]
    pub const fn within(self, other: Self, radius: i64) -> bool {
        let r = radius as i128;
        self.distance2(other) <= r * r
    }
}
