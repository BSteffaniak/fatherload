//! The Gravity Shear's shear halls and the Shear Spire.
//!
//! A **shear hall** is a rectangle of the stratum where gravity is turned: 90 or 270 degrees
//! (it pulls toward a wall) near the stratum's top, 180 (toward the ceiling) from a third of the
//! way down, any of them near the bottom, where halls also come more often and bigger. Inside
//! one the rig turns itself with its rotation thrusters (the camera follows the rig, never on
//! its own) and walks the wall the field calls the floor. The generator opens the hall, lays
//! **shear plates** along its turned floor with shelves sticking out from it every few tiles,
//! hides an ore pocket in the turned floor's far end, and dots its frame with **flow stone**.
//!
//! The **Shear Spire** ([`spire`], the stratum's landmark) is a hollow tower where down is the
//! nearest wall ([`spire_down`]): walk up its sides by turning the rig at each corner, and the
//! cache waits on the central plinth's top.

use crate::{
    cave::CaveTile,
    descent::Stratum,
    terrain::{MineralKind, TileKind, TilePosition},
    worldgen::{
        SPAWN_TILE_X, TileRect, WorldSettings,
        descent::{DescentLayout, landmarks, variant},
        noise::hash2,
    },
};

use super::WarpedTile;

/// The cells shear halls are rolled per, `(width, height)` in tiles.
pub const SHEAR_CELL: (i32, i32) = (48, 36);
/// Tiles over which a hall's turned gravity fades in from its edge.
pub const SHEAR_FEATHER_TILES: f32 = 4.0;
/// `(width, height)` of the Shear Spire.
pub const SPIRE_SIZE: (i32, i32) = (36, 48);
/// Half size of the Spire's central plinth `(columns, rows)`.
pub const SPIRE_PLINTH_HALF: (i32, i32) = (3, 4);

const SALT_CELL: u64 = 0x5EA2_0001;
const SALT_SHAPE: u64 = 0x5EA2_0002;
const SALT_SPIRE: u64 = 0x5EA2_0003;
const SALT_DECOR: u64 = 0x5EA2_0004;

/// One shear hall.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShearRegion {
    /// Every tile whose gravity is turned.
    pub bounds: TileRect,
    /// Degrees gravity is turned clockwise on screen from straight down: 90 (pulls left), 180
    /// (up) or 270 (right).
    pub turn: u16,
    /// How deep in the stratum it is, per mille.
    pub along: u16,
    pub seed: u64,
}

/// The unit gravity of a turn (`(x, y)`, +y down): 90 pulls left, 180 up, 270 right.
#[must_use]
pub const fn turned(turn: u16) -> (i32, i32) {
    match turn % 360 {
        90 => (-1, 0),
        180 => (0, -1),
        270 => (1, 0),
        _ => (0, 1),
    }
}

impl ShearRegion {
    /// The hall's unit gravity.
    #[must_use]
    pub const fn down(&self) -> (i32, i32) {
        turned(self.turn)
    }

    /// How strongly the hall's gravity holds at fractional tile `(column, row)`.
    #[must_use]
    pub fn weight(&self, column: f32, row: f32) -> f32 {
        #[allow(
            clippy::cast_precision_loss,
            reason = "tile coordinates of a world fit"
        )]
        let (left, right, top, bottom) = (
            self.bounds.min.x as f32,
            (self.bounds.max.x + 1) as f32,
            self.bounds.min.y as f32,
            (self.bounds.max.y + 1) as f32,
        );
        if column < left || column >= right || row < top || row >= bottom {
            return 0.0;
        }
        let inside = (column - left)
            .min(right - column)
            .min(row - top)
            .min(bottom - row);
        let t = (inside / SHEAR_FEATHER_TILES).clamp(0.0, 1.0);
        t * t * 2.0f32.mul_add(-t, 3.0)
    }

    /// Distance (tiles) of tile `(x, y)` from the hall's turned floor (0 on the floor row or
    /// column), and its position along the floor.
    const fn floor_frame(&self, x: i32, y: i32) -> (i32, i32) {
        let b = self.bounds;
        match self.down() {
            (-1, 0) => (x - b.min.x, y - b.min.y),
            (1, 0) => (b.max.x - x, y - b.min.y),
            (0, -1) => (y - b.min.y, x - b.min.x),
            _ => (b.max.y - y, x - b.min.x),
        }
    }

    /// The length of the hall's turned floor (tiles) and its depth across.
    const fn floor_span(&self) -> (i32, i32) {
        let b = self.bounds;
        let (w, h) = (b.max.x - b.min.x + 1, b.max.y - b.min.y + 1);
        if self.down().0 != 0 { (h, w) } else { (w, h) }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "small bounded rolls"
)]
const fn roll_range(roll: u64, shift: u32, low: i32, high: i32) -> i32 {
    if high <= low {
        return low;
    }
    low + ((roll >> shift) % ((high - low + 1) as u64)) as i32
}

const fn rect(x: i32, y: i32, (width, height): (i32, i32)) -> TileRect {
    TileRect {
        min: TilePosition { x, y },
        max: TilePosition {
            x: x + width - 1,
            y: y + height - 1,
        },
    }
}

/// The Shear Spire: a hollow tower in the lower half of the Gravity Shear, on the spawn side
/// away from the outposts (one per Deep Descent world).
#[must_use]
pub const fn spire(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !super::has_distortion(settings) {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::GravityShear);
    let roll = hash2(settings.seed, SALT_SPIRE, 0, 0);
    let side = if landmarks::outpost_column(settings) >= SPAWN_TILE_X {
        -1
    } else {
        1
    };
    let centre = SPAWN_TILE_X + side * roll_range(roll, 0, 40, 100);
    let middle = top + (bottom - top) * 3 / 5 + roll_range(roll, 12, -12, 12);
    Some(rect(
        centre - SPIRE_SIZE.0 / 2,
        middle - SPIRE_SIZE.1 / 2,
        SPIRE_SIZE,
    ))
}

/// Down inside the Spire at fractional tile `(column, row)`: toward the nearest wall, as a unit
/// vector (`(x, y)`, +y down).
#[must_use]
pub fn spire_down(spire: &TileRect, column: f32, row: f32) -> (f32, f32) {
    #[allow(
        clippy::cast_precision_loss,
        reason = "tile coordinates of a world fit"
    )]
    let (cx, cy, hw, hh) = (
        (spire.min.x + spire.max.x + 1) as f32 * 0.5,
        (spire.min.y + spire.max.y + 1) as f32 * 0.5,
        (spire.max.x - spire.min.x + 1) as f32 * 0.5,
        (spire.max.y - spire.min.y + 1) as f32 * 0.5,
    );
    let (dx, dy) = (column - cx, row - cy);
    // Compare distances to the walls: the nearer wall is down.
    let to_side = hw - dx.abs();
    let to_end = hh - dy.abs();
    if to_side < to_end {
        (dx.signum(), 0.0)
    } else {
        (0.0, if dy >= 0.0 { 1.0 } else { -1.0 })
    }
}

/// The weight of the Spire's own gravity at `(column, row)` (smoothstep over the feather at its
/// outer edge).
#[must_use]
pub fn spire_weight(spire: &TileRect, column: f32, row: f32) -> f32 {
    let region = ShearRegion {
        bounds: *spire,
        turn: 0,
        along: 0,
        seed: 0,
    };
    region.weight(column, row)
}

/// The hall rolled in cell `(cx, cy)`, if any.
fn cell_region(
    settings: &WorldSettings,
    layout: &DescentLayout,
    spire: Option<&TileRect>,
    cx: i32,
    cy: i32,
) -> Option<ShearRegion> {
    let (width, height) = SHEAR_CELL;
    let centre_row = cy * height + height / 2;
    if layout.stratum_of_row(centre_row) != Stratum::GravityShear {
        return None;
    }
    let along = u32::from(layout.along(centre_row));
    let roll = hash2(settings.seed, SALT_CELL, cx, cy);
    // "Shear Lines" turns a few more cells.
    let base = if variant(settings, Stratum::GravityShear) == 1 {
        360
    } else {
        300
    };
    if roll % 1_000 >= u64::from(base + 400 * along / 1_000) {
        return None;
    }
    // Walls first; ceilings from a third of the way down; anything near the bottom.
    let pick = (roll >> 50) % 6;
    let turn = if along < 340 {
        if pick.is_multiple_of(2) { 90 } else { 270 }
    } else if along < 700 {
        match pick {
            0 | 1 => 90,
            2 | 3 => 270,
            _ => 180,
        }
    } else {
        match pick {
            0 => 90,
            1 => 270,
            _ => 180,
        }
    };
    #[allow(clippy::cast_possible_wrap, reason = "per mille")]
    let deep = along as i32;
    let w = roll_range(roll, 10, 18 + deep / 80, 24 + deep / 60).min(width - 6);
    let h = roll_range(roll, 20, 14 + deep / 100, 20 + deep / 80).min(height - 6);
    let x = cx * width + roll_range(roll, 30, 3, (width - w - 3).max(3));
    let y = cy * height + roll_range(roll, 40, 3, (height - h - 3).max(3));
    let bounds = rect(x, y, (w, h));
    let (top, bottom) = layout.rows(Stratum::GravityShear);
    if bounds.min.y < top + 12 || bounds.max.y > bottom - 12 {
        return None;
    }
    if spire.is_some_and(|spire| spire.expanded(6).intersects(&bounds)) {
        return None;
    }
    let column = landmarks::outpost_column(settings);
    if bounds.max.x >= column - 24 && bounds.min.x <= column + 24 {
        return None;
    }
    // Clear of the Descent landmarks (outposts, gate chambers, anomaly rooms).
    if !landmarks::landmarks_with(settings, layout, &bounds.expanded(4)).is_empty() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, reason = "per mille")]
    Some(ShearRegion {
        bounds,
        turn,
        along: along as u16,
        seed: hash2(settings.seed, SALT_SHAPE, cx, cy),
    })
}

/// Every shear hall whose bounds overlap `area`. Empty for worlds without Descents.
#[must_use]
pub fn regions_touching(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> Vec<ShearRegion> {
    if !super::has_distortion(settings) {
        return Vec::new();
    }
    let (top, bottom) = layout.rows(Stratum::GravityShear);
    if area.max.y < top || area.min.y > bottom {
        return Vec::new();
    }
    let spire = spire(settings, layout);
    let (width, height) = SHEAR_CELL;
    let mut found = Vec::new();
    for cy in area.min.y.div_euclid(height)..=area.max.y.div_euclid(height) {
        for cx in area.min.x.div_euclid(width)..=area.max.x.div_euclid(width) {
            if let Some(region) = cell_region(settings, layout, spire.as_ref(), cx, cy)
                && region.bounds.intersects(area)
            {
                found.push(region);
            }
        }
    }
    found
}

/// The tile `region` puts at `(x, y)` over `base`, or `None` to leave it: the open hall, its
/// turned floor of shear plates with shelves, the ore pocket at the floor's far end and the
/// frame's flow stone.
#[must_use]
pub fn region_tile(region: &ShearRegion, x: i32, y: i32, base: TileKind) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if !super::carvable(base) {
        return None;
    }
    let bounds = region.bounds;
    if !bounds.contains(at) {
        // The frame: flow stone in the rock just outside, every other tile.
        if bounds.expanded(1).contains(at) && super::is_rock(base) && (x + y).rem_euclid(2) == 0 {
            return Some(TileKind::Warped(WarpedTile::ShearStone));
        }
        return None;
    }
    let (depth, along) = region.floor_frame(x, y);
    let (length, across) = region.floor_span();
    // A rock shell two tiles thick all round, so the hall is a room.
    let edge = (x - bounds.min.x)
        .min(bounds.max.x - x)
        .min(y - bounds.min.y)
        .min(bounds.max.y - y);
    if edge < 2 {
        // Doorways: the turned floor's two ends open through the shell at floor height (the
        // way in from plain gravity, the way out the far side).
        let door = (2..=4).contains(&depth) && (along < 2 || along >= length - 2);
        return Some(if door { TileKind::Air } else { base });
    }
    if depth == 2 {
        // The turned floor: plates, with an ore pocket under its far end.
        return Some(TileKind::Warped(WarpedTile::ShearPlate));
    }
    // Shelves: short plates standing off the floor every seven tiles along it.
    if depth > 2 && depth <= 4 && (along - 3).rem_euclid(7) == 0 && along > 3 && along < length - 4
    {
        return Some(TileKind::Warped(WarpedTile::ShearPlate));
    }
    // The prize: an ore pocket set in the far end of the turned floor's shell, reached by
    // walking the floor.
    if depth >= across - 4 && along >= length - 6 && along < length - 3 {
        let ore = if region.along > 600 {
            MineralKind::Mythril
        } else {
            MineralKind::Platinum
        };
        return Some(TileKind::Ore(ore));
    }
    if depth >= across - 3 && hash2(region.seed, SALT_DECOR, x, y).is_multiple_of(5) {
        return Some(TileKind::Warped(WarpedTile::ShearStone));
    }
    Some(TileKind::Air)
}

/// The tile the Spire puts at `(x, y)` over `base`, or `None` outside it.
///
/// A shell of flow stone, plates every few tiles along its inner walls (they are all floors),
/// the central plinth with its cache, and doorways at the bottom corners.
#[must_use]
pub fn spire_tile(spire: &TileRect, x: i32, y: i32, base: TileKind) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if !spire.contains(at) || !super::carvable(base) {
        return None;
    }
    let (left, right, top, bottom) = (
        x - spire.min.x,
        spire.max.x - x,
        y - spire.min.y,
        spire.max.y - y,
    );
    let edge = left.min(right).min(top).min(bottom);
    if edge < 2 {
        // Doorways low in both sides: three rows above the floor.
        let door = (left < 2 || right < 2) && (2..=4).contains(&bottom);
        return Some(if door {
            TileKind::Air
        } else {
            TileKind::Warped(WarpedTile::ShearStone)
        });
    }
    let centre = spire.center();
    let (px, py) = SPIRE_PLINTH_HALF;
    if (x - centre.x).abs() <= px && (y - centre.y).abs() <= py {
        // The plinth: flow stone with the cache on its top face (the top is a floor: down
        // points at the ceiling up there).
        if y == centre.y - py && (x - centre.x).abs() <= 1 {
            return Some(TileKind::Cave(CaveTile::TreasureCache));
        }
        if y == centre.y && (x - centre.x).abs() <= 1 {
            return Some(TileKind::Ore(MineralKind::Mythril));
        }
        return Some(TileKind::Warped(WarpedTile::ShearStone));
    }
    // Shelves along every inner wall: each wall is a floor here.
    let along_wall = if edge == left.min(right) { y } else { x };
    if edge <= 3 && along_wall.rem_euclid(6) == 0 && edge == 2 {
        return Some(TileKind::Warped(WarpedTile::ShearPlate));
    }
    Some(TileKind::Air)
}
