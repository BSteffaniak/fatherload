//! Polar tile geometry for round bodies (expansion plan §15 F12, Wave 11 polar track).
//!
//! A planet's tiles are laid out in **rings**: a tile's row is its depth below the body's datum
//! surface and its column its angle around the body. Each tile is a ring segment (a curved top
//! and bottom edge, sides pointing at the centre), so the ground is a true circle and "down" is
//! always toward the core.
//!
//! * **The unrolled strip.** Gameplay runs on the planet unrolled into a flat strip that wraps
//!   around: strip column `u` (in units of the *surface* ring's columns, [`PolarGrid::
//!   surface_columns`]) is the angle `2 pi u / W0` clockwise from the body's top, strip row `v`
//!   is the depth below the datum radius `R` (`r = R - v`). Rows above the datum (negative `v`)
//!   are hills and then open sky. This is the same kind of world as the home planet, so the
//!   terrain, digging, caves, fluids and drops run unchanged in strip coordinates.
//! * **Columns halve toward the core.** A ring of `W` columns has tiles `2 pi r / W` wide; as
//!   `r` shrinks the ring's column count halves at fixed rows ([`PolarGrid::halving_rows`])
//!   whenever its tiles would get narrower than [`MIN_TILE_ASPECT`] (`1/sqrt 2`) of a square,
//!   so every tile stays between about 0.7x and 1.4x square all the way down to the **core
//!   chamber** ([`PolarGrid::core_radius`]). In the strip, a tile of ring `k` (after `k`
//!   halvings) spans `2^k` strip columns ([`PolarGrid::span_at`]).
//! * **Exact wrap.** The surface ring has a whole number of world sectors' worth of columns
//!   ([`crate::position::SECTOR_TILES`] = 512 per sector), so going once around is a whole
//!   number of sectors: the world's floating frame wraps by relabelling its sector, and every
//!   local `f32` position stays bit-exact.
//! * **Body-centred Cartesian frame.** Tiles, `+x` right, `+y` down (as everywhere in the
//!   world), origin at the body's centre; the strip's column 0 points straight up (`-y`) and
//!   columns increase clockwise. [`PolarGrid::to_cartesian`] / [`PolarGrid::from_cartesian`]
//!   convert exactly (deterministic trigonometry, [`trig`]), [`PolarGrid::to_world`] /
//!   [`PolarGrid::from_world`] go to and from [`WorldPos`] in space around a body centre, and
//!   [`PolarGrid::local_frame`] gives local up, down and the tangent anywhere.
//!
//! Everything here is a pure function of the radius (basic IEEE arithmetic only), so every
//! peer and platform agrees.

#[cfg(test)]
mod tests;
pub mod trig;

use std::f64::consts::{FRAC_1_SQRT_2, TAU};

use crate::position::{SECTOR_PX, SECTOR_TILES, WorldPos};

/// The smallest radius (tiles) a body can be landed on (F12: moons and craters 400-1,000,
/// planets 1,500-10,000; smaller rocks are asteroids mined from flight).
pub const MIN_LANDABLE_RADIUS: u32 = 400;
/// The largest radius (tiles) the polar grid supports.
pub const MAX_RADIUS: u32 = 1 << 20;
/// The smallest radius the geometry itself accepts (test grids; bodies use
/// [`MIN_LANDABLE_RADIUS`]).
pub const MIN_GRID_RADIUS: u32 = 96;
/// A ring halves its columns where its tiles would get narrower than this many tile heights.
pub const MIN_TILE_ASPECT: f64 = FRAC_1_SQRT_2;
/// Most halvings a grid has (its surface columns are a multiple of 512 = 2^9).
pub const MAX_HALVINGS: usize = 9;

/// Pixels per tile as `f64`.
const TILE_PX: f64 = 32.0;

/// A tile of a ring: its column within the ring (`0..columns_at(row)`) and its row.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PolarTile {
    pub column: u32,
    pub row: i32,
}

/// Which way to step from a tile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolarStep {
    /// Clockwise (increasing column), wrapping.
    Right,
    /// Counter-clockwise (decreasing column), wrapping.
    Left,
    /// Outward (toward the surface, row - 1).
    Up,
    /// Inward (toward the core, row + 1).
    Down,
}

/// Local directions at a point of a body, unit vectors in the body-centred frame (`+y` down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalFrame {
    /// Away from the centre.
    pub up: (f64, f64),
    /// Toward the centre (where gravity pulls).
    pub down: (f64, f64),
    /// Along the ground, clockwise (increasing strip column): "right" for someone standing
    /// there.
    pub tangent: (f64, f64),
    /// The angle of local up from the body's top, clockwise (radians, `0..2 pi`): how far a
    /// craft's orientation reads from the screen's up when it stands level there.
    pub angle: f64,
}

/// The polar tile grid of one body (see the module docs).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PolarGrid {
    radius: u32,
    surface_columns: u32,
    core_radius: u32,
    /// First row of ring `k + 1` (after `k + 1` halvings), for `k < halvings`.
    halving_rows: [i32; MAX_HALVINGS],
    halvings: u8,
}

impl PolarGrid {
    /// The grid of a body `radius_tiles` tiles from centre to datum surface (clamped to
    /// [`MIN_GRID_RADIUS`]..=[`MAX_RADIUS`]).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bounded radii and ring counts"
    )]
    pub fn new(radius_tiles: u32) -> Self {
        let radius = radius_tiles.clamp(MIN_GRID_RADIUS, MAX_RADIUS);
        let r = f64::from(radius);
        // A whole number of sectors of columns, as close to square tiles as possible.
        // SECTOR_TILES (512) as f64.
        let sector = 512.0_f64;
        debug_assert_eq!(SECTOR_TILES, 512);
        let sectors = (TAU * r / sector).round().max(1.0);
        let surface_columns = (sectors * sector) as u32;
        let core_radius = (radius / 12).clamp(24, 640);
        let mut halving_rows = [i32::MAX; MAX_HALVINGS];
        let mut halvings = 0_u8;
        let mut columns = f64::from(surface_columns);
        // Walk the rows down to the core, halving whenever a ring's tiles get too narrow.
        let mut row = 0_i32;
        let bottom = i32::try_from(radius - core_radius).unwrap_or(i32::MAX);
        while row < bottom && usize::from(halvings) < MAX_HALVINGS {
            let mid = r - f64::from(row) - 0.5;
            let width = TAU * mid / columns;
            if width < MIN_TILE_ASPECT && surface_columns.is_multiple_of(1 << (halvings + 1)) {
                halving_rows[usize::from(halvings)] = row;
                halvings += 1;
                columns *= 0.5;
                continue;
            }
            row += 1;
        }
        Self {
            radius,
            surface_columns,
            core_radius,
            halving_rows,
            halvings,
        }
    }

    /// Radius of the datum surface (tiles): the top edge of row 0.
    #[must_use]
    pub const fn radius(&self) -> u32 {
        self.radius
    }

    /// Columns of the surface ring (`W0`, a multiple of 512): the strip's width.
    #[must_use]
    pub const fn surface_columns(&self) -> u32 {
        self.surface_columns
    }

    /// The surface ring's width in world sectors (one lap of the strip).
    #[must_use]
    #[allow(clippy::cast_possible_wrap, reason = "a few thousand sectors at most")]
    pub const fn lap_sectors(&self) -> i64 {
        self.surface_columns as i64 / SECTOR_TILES
    }

    /// Radius of the core (tiles): the bottom edge of the strip's last row.
    #[must_use]
    pub const fn core_radius(&self) -> u32 {
        self.core_radius
    }

    /// The strip's last row (its bottom edge is the core).
    #[must_use]
    #[allow(clippy::cast_possible_wrap, reason = "radii are bounded")]
    pub const fn bottom_row(&self) -> i32 {
        (self.radius - self.core_radius) as i32 - 1
    }

    /// How many times the columns halve between the surface and the core.
    #[must_use]
    pub const fn halvings(&self) -> u8 {
        self.halvings
    }

    /// The first row of each ring after the surface ring (a halving boundary each).
    #[must_use]
    pub fn halving_rows(&self) -> &[i32] {
        &self.halving_rows[..usize::from(self.halvings)]
    }

    /// The ring (number of halvings above it) row `row` is in: 0 at and above the surface.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "at most nine halvings")]
    pub fn ring_of(&self, row: i32) -> u8 {
        self.halving_rows()
            .iter()
            .filter(|start| row >= **start)
            .count() as u8
    }

    /// Columns in row `row`'s ring.
    #[must_use]
    pub fn columns_at(&self, row: i32) -> u32 {
        self.surface_columns >> self.ring_of(row)
    }

    /// Strip columns one tile of row `row` spans (`2^ring`).
    #[must_use]
    pub fn span_at(&self, row: i32) -> u32 {
        1 << self.ring_of(row)
    }

    /// Strip column `column` folded into `0..surface_columns` (the wrap).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "rem_euclid of a positive width"
    )]
    pub const fn wrap_column(&self, column: i64) -> u32 {
        column.rem_euclid(self.surface_columns as i64) as u32
    }

    /// The ring tile holding strip tile `(column, row)` (any column: it wraps).
    #[must_use]
    pub fn tile_of(&self, column: i64, row: i32) -> PolarTile {
        PolarTile {
            column: self.wrap_column(column) >> self.ring_of(row),
            row,
        }
    }

    /// The first strip column of `tile` (`0..surface_columns`); it spans
    /// [`Self::span_at`] columns from there.
    #[must_use]
    pub fn first_strip_column(&self, tile: PolarTile) -> u32 {
        (tile.column % self.columns_at(tile.row)) << self.ring_of(tile.row)
    }

    /// The tiles next to `tile` one `step` away. Left and right wrap round the ring; up and
    /// down cross a halving boundary where there is one: one tile of the inner ring sits under
    /// two of the outer ring, so stepping up from it reaches both (left one first). Empty
    /// below the strip's last row.
    #[must_use]
    pub fn neighbours(&self, tile: PolarTile, step: PolarStep) -> Vec<PolarTile> {
        let columns = self.columns_at(tile.row);
        let column = tile.column % columns;
        match step {
            PolarStep::Right => vec![PolarTile {
                column: (column + 1) % columns,
                row: tile.row,
            }],
            PolarStep::Left => vec![PolarTile {
                column: (column + columns - 1) % columns,
                row: tile.row,
            }],
            PolarStep::Down => {
                if tile.row >= self.bottom_row() {
                    return Vec::new();
                }
                let below = tile.row + 1;
                let shift = self.ring_of(below) - self.ring_of(tile.row);
                vec![PolarTile {
                    column: column >> shift,
                    row: below,
                }]
            }
            PolarStep::Up => {
                let above = tile.row - 1;
                let shift = self.ring_of(tile.row) - self.ring_of(above);
                (0..1_u32 << shift)
                    .map(|part| PolarTile {
                        column: (column << shift) + part,
                        row: above,
                    })
                    .collect()
            }
        }
    }

    /// Radius (tiles) of strip row coordinate `v` (`R - v`; the top edge of row `v` when
    /// whole).
    #[must_use]
    pub fn radius_at(&self, v: f64) -> f64 {
        f64::from(self.radius) - v
    }

    /// Width of row `row`'s tiles at their middle, in tile heights (`2 pi r / columns`).
    #[must_use]
    pub fn tile_aspect(&self, row: i32) -> f64 {
        let mid = self.radius_at(f64::from(row) + 0.5);
        TAU * mid / f64::from(self.columns_at(row))
    }

    /// How far (px, at [`crate::TILE_SIZE`] px per tile) the top edge of a row-`row` tile bows above
    /// the chord between its corners: the sagitta `r (1 - cos(theta / 2))`.
    #[must_use]
    pub fn tile_bow_px(&self, row: i32) -> f64 {
        let top = self.radius_at(f64::from(row));
        let half = TAU / f64::from(self.columns_at(row)) * 0.5;
        let (_, cos) = trig::sin_cos(half);
        top * (1.0 - cos) * TILE_PX
    }

    /// How much narrower a row-`row` tile's bottom edge is than its top, as a share of the
    /// top (`1 / r_top`).
    #[must_use]
    pub fn tile_width_difference(&self, row: i32) -> f64 {
        1.0 / self.radius_at(f64::from(row))
    }

    /// How far (px) the ground drops below a standing viewer's horizontal at `tiles` tiles
    /// along it: `R (1 - cos(tiles / R))`.
    #[must_use]
    pub fn horizon_drop_px(&self, tiles: f64) -> f64 {
        let r = f64::from(self.radius);
        let (_, cos) = trig::sin_cos(tiles / r);
        r * (1.0 - cos) * TILE_PX
    }

    /// The angle (radians, clockwise from the body's top, `0..2 pi` for a column in
    /// `0..surface_columns`) of strip column coordinate `u`.
    #[must_use]
    pub fn angle_of(&self, u: f64) -> f64 {
        u * TAU / f64::from(self.surface_columns)
    }

    /// The body-centred position (tiles, `+y` down) of strip point `(u, v)`: column
    /// coordinate `u` (surface columns, fractional), row coordinate `v` (rows below the datum,
    /// fractional; the top edge of row `v` when whole).
    #[must_use]
    pub fn to_cartesian(&self, u: f64, v: f64) -> (f64, f64) {
        let r = self.radius_at(v);
        let (sin, cos) = trig::sin_cos(self.angle_of(u));
        (r * sin, -r * cos)
    }

    /// The strip point `(u, v)` of body-centred position `(x, y)` (tiles): `u` in
    /// `0..surface_columns`, `v` the depth below the datum (negative above it). The centre
    /// itself reads as column 0 at the bottom of the core.
    #[must_use]
    pub fn from_cartesian(&self, x: f64, y: f64) -> (f64, f64) {
        let r = x.hypot(y);
        // Angle clockwise from up (-y): atan2(x, -y).
        let mut angle = trig::atan2(x, -y);
        if angle < 0.0 {
            angle += TAU;
        }
        let mut u = angle / TAU * f64::from(self.surface_columns);
        if u >= f64::from(self.surface_columns) {
            u -= f64::from(self.surface_columns);
        }
        (u, f64::from(self.radius) - r)
    }

    /// The strip point of ring tile `tile` with sub-tile offset `(fx, fy)` (`0..1` across the
    /// tile left to right, top to bottom; a tile of an inner ring spans several strip
    /// columns).
    #[must_use]
    pub fn tile_point(&self, tile: PolarTile, fx: f64, fy: f64) -> (f64, f64) {
        let span = f64::from(self.span_at(tile.row));
        let first = f64::from(self.first_strip_column(tile));
        (fx.mul_add(span, first), f64::from(tile.row) + fy)
    }

    /// The ring tile and sub-tile offset of strip point `(u, v)` (`u` wraps): the inverse of
    /// [`Self::tile_point`].
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "floor of a bounded tile coordinate"
    )]
    pub fn point_tile(&self, u: f64, v: f64) -> (PolarTile, f64, f64) {
        let width = f64::from(self.surface_columns);
        let u = u.rem_euclid(width);
        let row = v.floor() as i32;
        let span = f64::from(self.span_at(row));
        let column = (u / span).floor();
        let fx = column.mul_add(-span, u) / span;
        let tile = PolarTile {
            #[allow(clippy::cast_sign_loss, reason = "a wrapped, non-negative column")]
            column: (column as u32) % self.columns_at(row),
            row,
        };
        (tile, fx, v - f64::from(row))
    }

    /// Local up, down and the tangent (clockwise) at strip column coordinate `u` (they
    /// depend only on the angle).
    #[must_use]
    pub fn local_frame(&self, u: f64) -> LocalFrame {
        let angle = self.angle_of(u).rem_euclid(TAU);
        let (sin, cos) = trig::sin_cos(angle);
        LocalFrame {
            up: (sin, -cos),
            down: (-sin, cos),
            tangent: (cos, sin),
            angle,
        }
    }

    /// The absolute position in space of strip point `(u, v)` of a body centred on
    /// `centre`. Exact to the sub-pixel at any distance from the origin (the offset from the
    /// centre is added in `f64` with whole sectors split off).
    #[must_use]
    pub fn to_world(&self, centre: WorldPos, u: f64, v: f64) -> WorldPos {
        let (x, y) = self.to_cartesian(u, v);
        offset_world(centre, x * TILE_PX, y * TILE_PX)
    }

    /// The strip point `(u, v)` of absolute position `position` around a body centred on
    /// `centre` (see [`Self::from_cartesian`]).
    #[must_use]
    pub fn from_world(&self, centre: WorldPos, position: WorldPos) -> (f64, f64) {
        let (dx, dy) = centre.delta_f64(position);
        self.from_cartesian(dx / TILE_PX, dy / TILE_PX)
    }
}

/// `position` moved by `(dx, dy)` pixels given as `f64`, splitting whole sectors off exactly
/// (an `f32` offset would lose sub-pixels past 2^14 px).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "sector counts and an in-sector offset"
)]
pub fn offset_world(position: WorldPos, dx: f64, dy: f64) -> WorldPos {
    let sector = f64::from(SECTOR_PX);
    let x = f64::from(position.x) + dx;
    let y = f64::from(position.y) + dy;
    let (sectors_x, sectors_y) = ((x / sector).floor(), (y / sector).floor());
    WorldPos::new(
        position.sector_x.saturating_add(sectors_x as i64),
        position.sector_y.saturating_add(sectors_y as i64),
        sectors_x.mul_add(-sector, x) as f32,
        sectors_y.mul_add(-sector, y) as f32,
    )
}
