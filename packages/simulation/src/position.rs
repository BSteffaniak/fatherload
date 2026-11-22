//! World positions without limits (expansion plan §15 F7).
//!
//! Gameplay positions used to be plain `f32` world pixels, which resolve only 1/8 px at 2^20 px
//! (about 32,000 tiles from the origin). F7 splits a position in two:
//!
//! * a [`WorldPos`]: an exact, absolute position, an `i64` **sector** per axis
//!   ([`SECTOR_TILES`] tiles a side) plus an `f32` pixel offset inside it. It reaches
//!   ±2^63 sectors (far past 10^12 tiles) and is what crosses the API boundary: saves, netcode
//!   poses, the renderer's camera and anything comparing far-apart places;
//! * a [`Frame`]: the sector the simulation's **local pixel space** is centred on. Every
//!   gameplay `f32` position in the world (rigs, pilots, creatures, drops, projectiles, charges,
//!   boulders, people, tomb actors, ...) is a pixel offset from the frame's origin, so near the
//!   frame every system runs exactly the `f32` code it always ran, and collisions happen in that
//!   local frame. Tile coordinates stay absolute (`i32` [`TilePosition`], ±2.1 x 10^9 columns);
//!   only the conversions between local pixels and tiles go through the frame
//!   ([`Frame::col`], [`Frame::row`], [`Frame::tile_at`], [`Frame::rel_col`], [`Frame::rel_row`],
//!   [`Frame::tile_center`], ...).
//!
//! A world with a **floating** frame ([`Frame::floating`]) re-centres it on its anchor rig
//! (`WorldState::recentre_frame`, the last step of every tick) once that rig is more than
//! [`RECENTRE_SECTORS`] sector from the origin on an axis: every local position shifts by a
//! whole number of sectors (exact in `f32` for the anchor), so the anchor's local position
//! always stays inside ±[`SECTOR_PX`] where `f32` holds the simulation's 1/1024 px grid exactly.
//! A rig flying, colliding and landing at 10^9 tiles is bit-identical to the same flight near
//! the origin.
//!
//! A world with a **fixed** frame never moves it: its origin stays at sector `(0, 0)` and every
//! local pixel *is* the old world pixel, bit for bit. Worlds from generators V1-V10 use fixed
//! frames ([`crate::worldgen::GeneratorVersion::floating_frame`]), so their gameplay, saves and
//! state hashes are unchanged.
//!
//! **Migration helpers.** Code written against plain world pixels ports mechanically:
//! `(x / TILE_SIZE).floor() as i32` becomes `frame.col(x)`, `tile.x as f32` (an absolute
//! column read as pixels) becomes `frame.rel_col(tile.x)`, `(x, y)` handed to a pure function
//! of absolute place (environment fields, sky bands) becomes `frame.legacy_px(x, y)`. For a
//! fixed frame at the origin every helper is the exact expression it replaces.

use serde::{Deserialize, Serialize};

use crate::terrain::{TILE_SIZE, TilePosition};

/// Tiles per sector side. A power of two (512 = 16 terrain chunks) so a sector boundary is a
/// chunk boundary and a frame shift is an exact power-of-two multiple in `f32`.
pub const SECTOR_TILES: i64 = 512;

/// Pixels per sector side (`SECTOR_TILES * TILE_SIZE` = 2^14). Below this magnitude `f32`
/// holds every multiple of 1/1024 px (the simulation's sub-pixel grid) exactly.
pub const SECTOR_PX: f32 = 16_384.0;

/// [`SECTOR_PX`] as `f64`.
const SECTOR_PX_F64: f64 = 16_384.0;

/// How far (in sectors, per axis) a floating frame's anchor may drift from the frame origin
/// before the frame re-centres on it.
pub const RECENTRE_SECTORS: f32 = 1.0;

/// An absolute world position: sector coordinates plus a pixel offset inside the sector.
///
/// Normalized positions ([`Self::normalized`]) keep the offset in `[0, SECTOR_PX)`; every
/// constructor here returns a normalized position. Equality compares the representation, so
/// compare normalized positions.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WorldPos {
    /// Sector column.
    pub sector_x: i64,
    /// Sector row (down is positive, as everywhere in the world).
    pub sector_y: i64,
    /// Pixels right of the sector's left edge.
    pub x: f32,
    /// Pixels below the sector's top edge.
    pub y: f32,
}

/// Splits `value` pixels into whole sectors and the rest, exactly: `value = sectors *
/// SECTOR_PX + rest` with `rest` in `[0, SECTOR_PX)` (for finite input).
#[allow(
    clippy::cast_possible_truncation,
    reason = "the sector count of any finite f32 fits an i64"
)]
fn split_px(value: f32) -> (i64, f32) {
    if !value.is_finite() {
        return (0, 0.0);
    }
    let sectors = (value / SECTOR_PX).floor();
    // `sectors * SECTOR_PX` is a power-of-two multiple of an integer-valued float: exact, and
    // the difference is exact too (both operands are multiples of `value`'s ulp or coarser).
    let rest = value - sectors * SECTOR_PX;
    // Rounding at the top edge can land on SECTOR_PX itself.
    if rest >= SECTOR_PX {
        (sectors as i64 + 1, rest - SECTOR_PX)
    } else if rest < 0.0 {
        (sectors as i64 - 1, rest + SECTOR_PX)
    } else {
        (sectors as i64, rest)
    }
}

impl WorldPos {
    /// The position `(x, y)` pixels from the top-left corner of sector `(sector_x, sector_y)`,
    /// normalized.
    #[must_use]
    pub fn new(sector_x: i64, sector_y: i64, x: f32, y: f32) -> Self {
        Self {
            sector_x,
            sector_y,
            x,
            y,
        }
        .normalized()
    }

    /// An old-style absolute world pixel, exactly (any finite `f32` splits losslessly).
    #[must_use]
    pub fn from_legacy_px(x: f32, y: f32) -> Self {
        let (sector_x, x) = split_px(x);
        let (sector_y, y) = split_px(y);
        Self {
            sector_x,
            sector_y,
            x,
            y,
        }
    }

    /// The top-left corner of tile `(column, row)` (64-bit tile coordinates).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "a tile inside its sector is below 512"
    )]
    pub const fn from_tile(column: i64, row: i64) -> Self {
        Self {
            sector_x: column.div_euclid(SECTOR_TILES),
            sector_y: row.div_euclid(SECTOR_TILES),
            x: column.rem_euclid(SECTOR_TILES) as f32 * TILE_SIZE,
            y: row.rem_euclid(SECTOR_TILES) as f32 * TILE_SIZE,
        }
    }

    /// The centre of tile `(column, row)`.
    #[must_use]
    pub fn tile_center(column: i64, row: i64) -> Self {
        Self::from_tile(column, row).offset(TILE_SIZE * 0.5, TILE_SIZE * 0.5)
    }

    /// The same position with its offsets folded into `[0, SECTOR_PX)` (exact).
    #[must_use]
    pub fn normalized(self) -> Self {
        let (carry_x, x) = split_px(self.x);
        let (carry_y, y) = split_px(self.y);
        Self {
            sector_x: self.sector_x.saturating_add(carry_x),
            sector_y: self.sector_y.saturating_add(carry_y),
            x,
            y,
        }
    }

    /// This position moved by `(dx, dy)` pixels, normalized.
    #[must_use]
    pub fn offset(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
        .normalized()
    }

    /// `other - self` in pixels, as an `f64` pair (exact sector difference, then the offsets).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "sector differences beyond 2^53 are farther than anything compares"
    )]
    pub fn delta_f64(self, other: Self) -> (f64, f64) {
        let sectors_x = other.sector_x.saturating_sub(self.sector_x) as f64;
        let sectors_y = other.sector_y.saturating_sub(self.sector_y) as f64;
        (
            sectors_x.mul_add(SECTOR_PX_F64, f64::from(other.x) - f64::from(self.x)),
            sectors_y.mul_add(SECTOR_PX_F64, f64::from(other.y) - f64::from(self.y)),
        )
    }

    /// `other - self` in pixels, as an `f32` vector: exact to the sub-pixel grid for nearby
    /// positions however far from the origin both are.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "a pixel delta as f32")]
    pub fn delta(self, other: Self) -> (f32, f32) {
        let (dx, dy) = self.delta_f64(other);
        (dx as f32, dy as f32)
    }

    /// Distance to `other`, in pixels.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "a pixel distance as f32")]
    pub fn distance(self, other: Self) -> f32 {
        let (dx, dy) = self.delta_f64(other);
        dx.hypot(dy) as f32
    }

    /// The tile containing this position (64-bit tile coordinates).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the offset is inside its sector"
    )]
    pub fn tile(self) -> (i64, i64) {
        let normal = self.normalized();
        (
            normal
                .sector_x
                .saturating_mul(SECTOR_TILES)
                .saturating_add((normal.x / TILE_SIZE).floor() as i64),
            normal
                .sector_y
                .saturating_mul(SECTOR_TILES)
                .saturating_add((normal.y / TILE_SIZE).floor() as i64),
        )
    }

    /// The tile containing this position, when it fits the terrain's `i32` grid.
    #[must_use]
    pub fn tile_position(self) -> Option<TilePosition> {
        let (column, row) = self.tile();
        Some(TilePosition {
            x: i32::try_from(column).ok()?,
            y: i32::try_from(row).ok()?,
        })
    }

    /// The absolute position in pixels as `f64` (exact to about 2^-8 px out to 2^45 px).
    #[must_use]
    pub fn px_f64(self) -> (f64, f64) {
        Self::default().delta_f64(self)
    }

    /// The old-style absolute world pixel (lossy far from the origin: `f32` resolves 1/8 px at
    /// 2^20 px). For pure functions of place that only need tile-scale accuracy.
    #[must_use]
    pub fn to_legacy_px(self) -> (f32, f32) {
        Self::default().delta(self)
    }
}

/// The sector the world's local pixel space is centred on, and whether it follows play.
///
/// Every gameplay `f32` position in a world is relative to its frame (see the module docs).
/// The default frame is fixed at the origin: local pixels are the old world pixels.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct Frame {
    /// Sector column of the local origin.
    pub sector_x: i64,
    /// Sector row of the local origin.
    pub sector_y: i64,
    /// Whether the frame re-centres on its anchor (generator V11 worlds and tests); a fixed
    /// frame stays at the origin.
    pub floating: bool,
}

impl Frame {
    /// The fixed frame at the origin (V1-V10 worlds): local pixels are world pixels.
    pub const FIXED: Self = Self {
        sector_x: 0,
        sector_y: 0,
        floating: false,
    };

    /// A floating frame at the origin.
    pub const FLOATING: Self = Self {
        sector_x: 0,
        sector_y: 0,
        floating: true,
    };

    /// The frame a new world of `generator` starts with: floating at the origin when the
    /// generator asks for it ([`GeneratorVersion::floating_frame`]), else fixed.
    ///
    /// [`GeneratorVersion::floating_frame`]: crate::worldgen::GeneratorVersion::floating_frame
    #[must_use]
    pub const fn for_generator(generator: crate::worldgen::GeneratorVersion) -> Self {
        if generator.floating_frame() {
            Self::FLOATING
        } else {
            Self::FIXED
        }
    }

    /// A floating frame centred on sector `(sector_x, sector_y)`.
    #[must_use]
    pub const fn floating_at(sector_x: i64, sector_y: i64) -> Self {
        Self {
            sector_x,
            sector_y,
            floating: true,
        }
    }

    /// Whether this frame re-centres on play.
    #[must_use]
    pub const fn floating(self) -> bool {
        self.floating
    }

    /// Whether this is the origin frame (local pixels are world pixels).
    #[must_use]
    pub const fn at_origin(self) -> bool {
        self.sector_x == 0 && self.sector_y == 0
    }

    /// The tile column of the local origin.
    #[must_use]
    pub const fn origin_col(self) -> i64 {
        self.sector_x.saturating_mul(SECTOR_TILES)
    }

    /// The tile row of the local origin.
    #[must_use]
    pub const fn origin_row(self) -> i64 {
        self.sector_y.saturating_mul(SECTOR_TILES)
    }

    /// The tile column under local pixel `x` (`(x / TILE_SIZE).floor()` in world terms),
    /// saturated to the terrain's `i32` grid (a column past it is outside every world).
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "saturating float-to-int")]
    pub fn col(self, x: f32) -> i32 {
        saturate((x / TILE_SIZE).floor() as i64, self.origin_col())
    }

    /// The tile row under local pixel `y`, saturated like [`Self::col`].
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "saturating float-to-int")]
    pub fn row(self, y: f32) -> i32 {
        saturate((y / TILE_SIZE).floor() as i64, self.origin_row())
    }

    /// `(x / TILE_SIZE).ceil()` as an absolute column (visible-range ends), saturated.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "saturating float-to-int")]
    pub fn col_ceil(self, x: f32) -> i32 {
        saturate((x / TILE_SIZE).ceil() as i64, self.origin_col())
    }

    /// `(y / TILE_SIZE).ceil()` as an absolute row, saturated.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "saturating float-to-int")]
    pub fn row_ceil(self, y: f32) -> i32 {
        saturate((y / TILE_SIZE).ceil() as i64, self.origin_row())
    }

    /// The tile under local pixel `(x, y)`.
    #[must_use]
    pub fn tile_at(self, x: f32, y: f32) -> TilePosition {
        TilePosition {
            x: self.col(x),
            y: self.row(y),
        }
    }

    /// Absolute column `column` relative to the frame, in tiles, as `f32`: the drop-in for
    /// `column as f32` when the result is then scaled to pixels (`rel_col(c) * TILE_SIZE` is
    /// the local pixel of the column's left edge).
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "a column near the frame")]
    pub const fn rel_col(self, column: i32) -> f32 {
        (column as i64 - self.origin_col()) as f32
    }

    /// Absolute row `row` relative to the frame, in tiles, as `f32` (see [`Self::rel_col`]).
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "a row near the frame")]
    pub const fn rel_row(self, row: i32) -> f32 {
        (row as i64 - self.origin_row()) as f32
    }

    /// Local pixel of the left edge of column `column`.
    #[must_use]
    pub fn col_px(self, column: i32) -> f32 {
        self.rel_col(column) * TILE_SIZE
    }

    /// Local pixel of the top edge of row `row`.
    #[must_use]
    pub fn row_px(self, row: i32) -> f32 {
        self.rel_row(row) * TILE_SIZE
    }

    /// Local pixel of the top-left corner of `tile`.
    #[must_use]
    pub fn tile_origin(self, tile: TilePosition) -> (f32, f32) {
        (self.col_px(tile.x), self.row_px(tile.y))
    }

    /// Local pixel of the centre of `tile` (`(c + 0.5) * TILE_SIZE` in world terms).
    #[must_use]
    pub fn tile_center(self, tile: TilePosition) -> (f32, f32) {
        (
            (self.rel_col(tile.x) + 0.5) * TILE_SIZE,
            (self.rel_row(tile.y) + 0.5) * TILE_SIZE,
        )
    }

    /// Local pixel of the centre of tile `(column, row)` ([`Self::tile_center`] for a pair).
    #[must_use]
    pub fn centre_of(self, column: i32, row: i32) -> (f32, f32) {
        self.tile_center(TilePosition { x: column, y: row })
    }

    /// The absolute position of local pixel `(x, y)`.
    #[must_use]
    pub fn world(self, x: f32, y: f32) -> WorldPos {
        WorldPos {
            sector_x: self.sector_x,
            sector_y: self.sector_y,
            x,
            y,
        }
        .normalized()
    }

    /// The local pixel of absolute position `position` (exact when it is near the frame).
    #[must_use]
    pub fn local(self, position: WorldPos) -> (f32, f32) {
        self.world(0.0, 0.0).delta(position)
    }

    /// Absolute world pixels of local pixel `(x, y)` as `f64`.
    #[must_use]
    pub fn absolute_px(self, x: f32, y: f32) -> (f64, f64) {
        self.world(x, y).px_f64()
    }

    /// The old-style absolute world pixel of local pixel `(x, y)`, for pure functions of
    /// place that predate F7 (environment fields, sky bands, biome lookups). Exactly `(x, y)`
    /// in the origin frame; lossy far from the origin (fine at tile scale).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "an approximate world pixel"
    )]
    pub fn legacy_px(self, x: f32, y: f32) -> (f32, f32) {
        if self.at_origin() {
            return (x, y);
        }
        let (ax, ay) = self.absolute_px(x, y);
        (ax as f32, ay as f32)
    }

    /// The old-style absolute world pixel of a local `x` (see [`Self::legacy_px`]).
    #[must_use]
    pub fn legacy_x(self, x: f32) -> f32 {
        self.legacy_px(x, 0.0).0
    }

    /// The old-style absolute world pixel of a local `y` (see [`Self::legacy_px`]).
    #[must_use]
    pub fn legacy_y(self, y: f32) -> f32 {
        self.legacy_px(0.0, y).1
    }

    /// The local pixel of an old-style absolute world pixel (constants such as spawn points
    /// authored in world pixels). Exactly `(x, y)` in the origin frame.
    #[must_use]
    pub fn from_legacy_px(self, x: f32, y: f32) -> (f32, f32) {
        if self.at_origin() {
            return (x, y);
        }
        self.local(WorldPos::from_legacy_px(x, y))
    }

    /// Pixels to subtract from every local position when moving from this frame to `next`
    /// (whole sectors, so exact for positions near either origin).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "frame moves are a few sectors at a time"
    )]
    pub fn shift_to(self, next: Self) -> FrameShift {
        FrameShift {
            dx: next.sector_x.saturating_sub(self.sector_x) as f32 * SECTOR_PX,
            dy: next.sector_y.saturating_sub(self.sector_y) as f32 * SECTOR_PX,
        }
    }

    /// The frame a floating frame moves to so local pixel `(x, y)` (its anchor) is back near
    /// the origin, or `None` while the anchor is within [`RECENTRE_SECTORS`] on both axes (or
    /// the frame is fixed). Only the axis that drifted moves.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "sector counts of local pixels"
    )]
    pub fn recentred_on(self, x: f32, y: f32) -> Option<Self> {
        if !self.floating || !x.is_finite() || !y.is_finite() {
            return None;
        }
        let limit = RECENTRE_SECTORS * SECTOR_PX;
        let step = |value: f32| -> i64 {
            if value.abs() > limit {
                (value / SECTOR_PX).round() as i64
            } else {
                0
            }
        };
        let (step_x, step_y) = (step(x), step(y));
        if step_x == 0 && step_y == 0 {
            return None;
        }
        Some(Self {
            sector_x: self.sector_x.saturating_add(step_x),
            sector_y: self.sector_y.saturating_add(step_y),
            floating: true,
        })
    }
}

/// `local + origin`, saturated into `i32`.
fn saturate(local: i64, origin: i64) -> i32 {
    let value = local.saturating_add(origin);
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

/// A move of the local pixel space: subtract it from every local position
/// ([`Self::apply`]). Velocities, lengths and tile coordinates do not change.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameShift {
    /// Pixels subtracted from every local `x`.
    pub dx: f32,
    /// Pixels subtracted from every local `y`.
    pub dy: f32,
}

impl FrameShift {
    /// Whether this shift moves nothing.
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.dx == 0.0 && self.dy == 0.0
    }

    /// Shifts a local `x`.
    #[must_use]
    pub fn x(self, x: f32) -> f32 {
        x - self.dx
    }

    /// Shifts a local `y`.
    #[must_use]
    pub fn y(self, y: f32) -> f32 {
        y - self.dy
    }

    /// Shifts a local point in place.
    pub fn apply(self, x: &mut f32, y: &mut f32) {
        *x -= self.dx;
        *y -= self.dy;
    }

    /// Shifts a local point given as a pair.
    #[must_use]
    pub fn point(self, (x, y): (f32, f32)) -> (f32, f32) {
        (x - self.dx, y - self.dy)
    }
}

/// State holding local pixel positions, moved when the world's frame moves.
///
/// Every type that stores a local position implements it, so a frame move reaches every
/// rig, body, waypoint and anchor. New state with positions must implement it too (the
/// world's implementation destructures every field, so the compiler asks).
pub trait Reframe {
    /// Moves every local position this value holds by `shift`.
    fn reframe(&mut self, shift: FrameShift);
}

impl<T: Reframe> Reframe for Option<T> {
    fn reframe(&mut self, shift: FrameShift) {
        if let Some(value) = self {
            value.reframe(shift);
        }
    }
}

impl<T: Reframe> Reframe for Vec<T> {
    fn reframe(&mut self, shift: FrameShift) {
        for value in self {
            value.reframe(shift);
        }
    }
}

impl<K, T: Reframe> Reframe for std::collections::BTreeMap<K, T> {
    fn reframe(&mut self, shift: FrameShift) {
        for value in self.values_mut() {
            value.reframe(shift);
        }
    }
}

/// A local point stored as a pair.
impl Reframe for (f32, f32) {
    fn reframe(&mut self, shift: FrameShift) {
        *self = shift.point(*self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_pixels_round_trip_exactly() {
        for value in [
            0.0_f32,
            -0.5,
            1.0 / 1024.0,
            -1.0 / 1024.0,
            16_383.999,
            16_384.0,
            -16_384.0,
            1_048_576.125,
            -3_000_000.25,
            123_456_789.0,
            -9.0e15,
        ] {
            let position = WorldPos::from_legacy_px(value, -value);
            assert!(
                (0.0..SECTOR_PX).contains(&position.x),
                "{value}: {position:?}"
            );
            assert!(
                (0.0..SECTOR_PX).contains(&position.y),
                "{value}: {position:?}"
            );
            let (x, y) = position.px_f64();
            assert_eq!(x, f64::from(value), "{value}");
            assert_eq!(y, f64::from(-value), "{value}");
            assert_eq!(position.to_legacy_px(), (value, -value));
        }
    }

    #[test]
    fn origin_frame_helpers_are_the_old_expressions() {
        let frame = Frame::FIXED;
        for x in [-70.5_f32, -32.0, -0.001, 0.0, 31.999, 32.0, 5_000.25, 1.0e7] {
            #[allow(clippy::cast_possible_truncation)]
            let old = (x / TILE_SIZE).floor() as i32;
            assert_eq!(frame.col(x), old);
            assert_eq!(frame.row(x), old);
            assert_eq!(frame.legacy_px(x, x), (x, x));
            assert_eq!(frame.from_legacy_px(x, x), (x, x));
        }
        for column in [-131_072_i32, -1, 0, 7, 97, 131_071] {
            #[allow(clippy::cast_precision_loss)]
            let old = column as f32;
            assert_eq!(frame.rel_col(column), old);
            assert_eq!(frame.col_px(column), old * TILE_SIZE);
            assert_eq!(
                frame.tile_center(TilePosition {
                    x: column,
                    y: column
                }),
                ((old + 0.5) * TILE_SIZE, (old + 0.5) * TILE_SIZE)
            );
        }
    }

    /// Precision far from the origin: 10^6, 10^9 and 10^12 tiles out, sub-pixel steps on the
    /// simulation's grid stay exact and nearby deltas are exact.
    #[test]
    fn far_positions_keep_the_subpixel_grid() {
        let step = 1.0 / 1024.0;
        for tiles in [1.0e6_f64, 1.0e9, 1.0e12, -1.0e12] {
            #[allow(clippy::cast_possible_truncation)]
            let column = tiles as i64;
            let start = WorldPos::from_tile(column, column / 3);
            let mut moving = start;
            for _ in 0..10_000 {
                moving = moving.offset(step, -step);
            }
            let (dx, dy) = start.delta(moving);
            assert_eq!(dx, 10_000.0 * step, "{tiles}");
            assert_eq!(dy, -10_000.0 * step, "{tiles}");
            assert_eq!(moving.tile(), (column, column / 3 - 1), "{tiles}");
            // A frame on the far sector sees it as a small local position.
            let frame = Frame::floating_at(start.sector_x, start.sector_y);
            let (x, y) = frame.local(moving);
            assert!(x.abs() < SECTOR_PX && y.abs() < SECTOR_PX);
            assert_eq!(frame.world(x, y), moving);
            let expected =
                i32::try_from(column).unwrap_or(if column < 0 { i32::MIN } else { i32::MAX });
            assert_eq!(frame.tile_at(x, y).x, expected);
        }
    }

    #[test]
    fn recentring_moves_whole_sectors_and_only_the_drifted_axis() {
        let frame = Frame::FLOATING;
        assert_eq!(frame.recentred_on(16_000.0, -16_000.0), None);
        let next = frame.recentred_on(40_000.0, 100.0).expect("drifted");
        assert_eq!((next.sector_x, next.sector_y), (2, 0));
        let shift = frame.shift_to(next);
        assert_eq!(shift.x(40_000.0), 40_000.0 - 2.0 * SECTOR_PX);
        assert_eq!(shift.y(100.0), 100.0);
        assert_eq!(Frame::FIXED.recentred_on(1.0e9, 1.0e9), None);
        // The shifted anchor is the same absolute place.
        assert_eq!(
            frame.world(40_000.0, 100.0),
            next.world(shift.x(40_000.0), shift.y(100.0))
        );
    }

    #[test]
    fn far_tiles_saturate_outside_the_i32_grid() {
        let frame = Frame::floating_at(1 << 40, -(1 << 40));
        assert_eq!(frame.col(0.0), i32::MAX);
        assert_eq!(frame.row(0.0), i32::MIN);
        assert!(frame.world(0.0, 0.0).tile_position().is_none());
    }
}
