//! The Mirror Strata's mirror halls: where they are and what they look like.
//!
//! A **mirror hall** is a rectangle of the stratum with a vertical axis down its middle. Inside
//! it the screen and the controls mirror (the field, [`super::fields`]); its rock is perfectly
//! symmetric about the axis (the generator draws it from the distance to the axis), so a hall
//! reads as a reflection of itself, and its frame is dotted with silvered glass in the rock
//! around it (the boundary's telegraph before the shimmer on screen).
//!
//! Halls are rolled one per [`MIRROR_CELL`] at most: a quarter of the cells near the stratum's
//! top, narrow ones, and most of them near its bottom, wide and tall ("introduced gently, then
//! pushed"). One fixed hall per world, the **Looking-Glass Hall** ([`looking_glass_hall`]), is
//! the stratum's landmark. Parallax Station (the Descent VI outpost) and the builders' cable
//! keep clear of every hall.
//!
//! **Mirror ore** sits in the halls' walls in pairs at mirrored columns: one real
//! ([`WarpedTile::MirrorOre`]) and one reflection ([`WarpedTile::MirrorGhost`]). Which side is
//! real is rolled per pair. The tells are the renderer's: a reflection has no shadow, a cooler
//! cast, and it breathes with a faint shimmer.

use crate::{
    descent::Stratum,
    terrain::{TileKind, TilePosition},
    worldgen::{
        SPAWN_TILE_X, TileRect, WorldSettings,
        descent::{DescentLayout, landmarks, variant},
        noise::{NOISE_MAX, hash2, value_noise},
    },
};

use super::WarpedTile;

/// The cells mirror halls are rolled per, `(width, height)` in tiles.
pub const MIRROR_CELL: (i32, i32) = (64, 40);
/// Tiles over which a hall's mirror fades in from its edge.
pub const MIRROR_FEATHER_TILES: f32 = 3.0;
/// `(width, height)` of the Looking-Glass Hall.
pub const LOOKING_GLASS_SIZE: (i32, i32) = (60, 26);
/// Columns either side of the outposts' column no hall comes within (Parallax Station, its
/// settlement hall and the builders' cable stay plain).
pub const STATION_CLEARANCE: i32 = 96;

const SALT_CELL: u64 = 0x3171_0001;
const SALT_SHAPE: u64 = 0x3171_0002;
const SALT_ROCK: u64 = 0x3171_0003;
const SALT_ORE: u64 = 0x3171_0004;
const SALT_HALL: u64 = 0x3171_0005;
const SALT_FRAME: u64 = 0x3171_0006;

/// One mirror hall.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MirrorZone {
    /// Every tile it mirrors (and the generator draws symmetric).
    pub bounds: TileRect,
    /// How deep in the stratum it is, per mille (deeper halls are wider and taller).
    pub along: u16,
    /// Per-hall variation.
    pub seed: u64,
    /// Whether this is the Looking-Glass Hall.
    pub landmark: bool,
}

impl MirrorZone {
    /// `min.x + max.x`: column `x` reflects to `axis2 - x`.
    #[must_use]
    pub const fn axis2(&self) -> i32 {
        self.bounds.min.x + self.bounds.max.x
    }

    /// The column `x` reflects to.
    #[must_use]
    pub const fn reflect_column(&self, x: i32) -> i32 {
        self.axis2() - x
    }

    /// The axis in fractional tile columns (`x = axis` is the mirror line).
    #[must_use]
    pub fn axis(&self) -> f32 {
        #[allow(clippy::cast_precision_loss, reason = "tile columns of a world fit")]
        let axis = (self.axis2() + 1) as f32 * 0.5;
        axis
    }

    /// How far (tiles) column `x`'s centre is from the axis.
    #[must_use]
    pub const fn axis_distance2(&self, x: i32) -> i32 {
        (2 * x - self.axis2()).abs()
    }

    /// The key a hall's twin cooldown is filed under.
    #[must_use]
    pub const fn key(&self) -> (i32, i32) {
        (self.bounds.min.x, self.bounds.min.y)
    }

    /// How strongly the hall mirrors at fractional tile `(column, row)`: 0 outside, 1 deeper
    /// inside than [`MIRROR_FEATHER_TILES`] from its side edges (the curtains), a smoothstep
    /// across the feather. Its floor and ceiling are rock, so the rows inside it mirror fully:
    /// a rig on the floor never sees a half-mirrored double image.
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
        let inside = (column - left).min(right - column);
        let t = (inside / MIRROR_FEATHER_TILES).clamp(0.0, 1.0);
        t * t * 2.0f32.mul_add(-t, 3.0)
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

/// Whether a hall over `bounds` keeps clear of the outposts' column (Parallax Station, the
/// cable).
const fn clear_of_station(settings: &WorldSettings, bounds: &TileRect) -> bool {
    let column = landmarks::outpost_column(settings);
    bounds.max.x < column - STATION_CLEARANCE || bounds.min.x > column + STATION_CLEARANCE
}

/// The Looking-Glass Hall: the Mirror Strata's landmark hall, in the lower middle of the
/// stratum on the side away from the outposts (one per Deep Descent world).
#[must_use]
pub const fn looking_glass_hall(
    settings: &WorldSettings,
    layout: &DescentLayout,
) -> Option<MirrorZone> {
    if !super::has_distortion(settings) {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::MirrorStrata);
    let roll = hash2(settings.seed, SALT_HALL, 0, 0);
    let side = if landmarks::outpost_column(settings) >= SPAWN_TILE_X {
        -1
    } else {
        1
    };
    let centre = SPAWN_TILE_X + side * roll_range(roll, 0, 60, 110);
    let middle = top + (bottom - top) * 3 / 5 + roll_range(roll, 12, -10, 10);
    Some(MirrorZone {
        bounds: rect(
            centre - LOOKING_GLASS_SIZE.0 / 2,
            middle - LOOKING_GLASS_SIZE.1 / 2,
            LOOKING_GLASS_SIZE,
        ),
        along: 600,
        seed: hash2(settings.seed, SALT_SHAPE, -1, 0),
        landmark: true,
    })
}

/// The hall rolled in cell `(cx, cy)`, if any.
fn cell_zone(
    settings: &WorldSettings,
    layout: &DescentLayout,
    hall: Option<&MirrorZone>,
    cx: i32,
    cy: i32,
) -> Option<MirrorZone> {
    let (width, height) = MIRROR_CELL;
    let centre_row = cy * height + height / 2;
    if layout.stratum_of_row(centre_row) != Stratum::MirrorStrata {
        return None;
    }
    let along = u32::from(layout.along(centre_row));
    let roll = hash2(settings.seed, SALT_CELL, cx, cy);
    // "Twin Galleries" holds a few more halls.
    let base = if variant(settings, Stratum::MirrorStrata) == 1 {
        320
    } else {
        250
    };
    let chance = u64::from(base + 450 * along / 1_000);
    if roll % 1_000 >= chance {
        return None;
    }
    #[allow(clippy::cast_possible_wrap, reason = "per mille")]
    let deep = along as i32;
    let w = roll_range(roll, 10, 16 + deep / 60, 22 + deep / 40).min(width - 8);
    let h = roll_range(roll, 20, 12 + deep / 90, 16 + deep / 70).min(height - 6);
    let x = cx * width + roll_range(roll, 30, 3, (width - w - 3).max(3));
    let y = cy * height + roll_range(roll, 40, 3, (height - h - 3).max(3));
    let bounds = rect(x, y, (w, h));
    let (top, bottom) = layout.rows(Stratum::MirrorStrata);
    if bounds.min.y < top + 12 || bounds.max.y > bottom - 12 || !clear_of_station(settings, &bounds)
    {
        return None;
    }
    if hall.is_some_and(|hall| hall.bounds.expanded(6).intersects(&bounds)) {
        return None;
    }
    // Clear of the Descent landmarks (outposts, gate chambers, anomaly rooms).
    if !landmarks::landmarks_with(settings, layout, &bounds.expanded(4)).is_empty() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, reason = "per mille")]
    Some(MirrorZone {
        bounds,
        along: along as u16,
        seed: hash2(settings.seed, SALT_SHAPE, cx, cy),
        landmark: false,
    })
}

/// Every mirror hall whose bounds overlap `area` (the Looking-Glass Hall first). Empty for
/// worlds without Descents.
#[must_use]
pub fn zones_touching(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> Vec<MirrorZone> {
    if !super::has_distortion(settings) {
        return Vec::new();
    }
    let (top, bottom) = layout.rows(Stratum::MirrorStrata);
    if area.max.y < top || area.min.y > bottom {
        return Vec::new();
    }
    let hall = looking_glass_hall(settings, layout);
    let mut found: Vec<MirrorZone> = hall
        .iter()
        .copied()
        .filter(|zone| zone.bounds.intersects(area))
        .collect();
    let (width, height) = MIRROR_CELL;
    for cy in area.min.y.div_euclid(height)..=area.max.y.div_euclid(height) {
        for cx in area.min.x.div_euclid(width)..=area.max.x.div_euclid(width) {
            if let Some(zone) = cell_zone(settings, layout, hall.as_ref(), cx, cy)
                && zone.bounds.intersects(area)
            {
                found.push(zone);
            }
        }
    }
    found
}

/// The mirror hall holding tile `(x, y)`, if any.
#[must_use]
pub fn zone_at(
    settings: &WorldSettings,
    layout: &DescentLayout,
    x: i32,
    y: i32,
) -> Option<MirrorZone> {
    let at = TilePosition { x, y };
    zones_touching(settings, layout, &TileRect { min: at, max: at })
        .into_iter()
        .next()
}

/// What a hall holds at a tile (drawn from the distance to the axis, so the hall is symmetric
/// to the tile).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HallCell {
    /// The hall's shell (rock, glass lining and ore pairs where it borders the open hall).
    Shell,
    /// A ledge standing out from the walls (rock).
    Ledge,
    /// The Looking-Glass Hall's mirror: a glass pillar on the axis.
    Mirror,
    Open,
}

fn hall_cell(zone: &MirrorZone, x: i32, y: i32) -> HallCell {
    let bounds = zone.bounds;
    let d2 = zone.axis_distance2(x);
    let width = bounds.max.x - bounds.min.x + 1;
    let height = bounds.height();
    let row = y - bounds.min.y;
    let edge = row.min(height - 1 - row);
    // Rounded top corners, square bottom ones (the floor runs wall to wall).
    let from_wall = width - 1 - d2;
    let corner = row < 4 && from_wall < 4 - row;
    if edge < 2 || d2 > width - 4 || corner {
        return HallCell::Shell;
    }
    if zone.landmark && d2 <= 1 && row >= 5 && row < height - 7 {
        return HallCell::Mirror;
    }
    // Ledges every six rows, standing out from both walls (mirrored), a gap in the middle.
    let floor_gap = height - 1 - row;
    if row >= 4 && floor_gap > 3 && (row - 4).rem_euclid(6) == 0 {
        let folded_row = row / 6;
        let reach = 3 + i32::try_from(hash2(zone.seed, SALT_SHAPE, folded_row, 0) % 7).unwrap_or(0);
        let wall_side = from_wall - 2;
        if wall_side >= 0 && wall_side < reach * 2 {
            return HallCell::Ledge;
        }
    }
    // A few alcoves of rock near the walls from the folded noise.
    let noise = value_noise(zone.seed, SALT_SHAPE, d2 / 2, y, 6, 4);
    let threshold = if zone.landmark { 56_000 } else { 50_000 };
    if from_wall < 8 && noise > threshold {
        return HallCell::Ledge;
    }
    HallCell::Open
}

fn hall_open(zone: &MirrorZone, x: i32, y: i32) -> bool {
    hall_cell(zone, x, y) == HallCell::Open
}

/// The tile `zone` puts at `(x, y)` over `base`, or `None` to leave it: symmetric rock, the
/// open hall, its glass lining, mirror ore pairs, the frame's glass dots.
#[must_use]
pub fn zone_tile(
    settings: &WorldSettings,
    zone: &MirrorZone,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if !super::carvable(base) {
        return None;
    }
    let bounds = zone.bounds;
    // The frame: glass dots in the rock just outside the hall, every third tile.
    if !bounds.contains(at) {
        let frame = bounds.expanded(1);
        if frame.contains(at) && base_is_rock(base) && (x + y).rem_euclid(3) == 0 {
            return Some(TileKind::Warped(WarpedTile::MirrorGlass));
        }
        return None;
    }
    let cell = hall_cell(zone, x, y);
    if cell == HallCell::Mirror {
        return Some(TileKind::Warped(WarpedTile::MirrorGlass));
    }
    if cell == HallCell::Open {
        // The Looking-Glass Hall's strongbox and its reflection, on the floor either side.
        if zone.landmark {
            let floor = bounds.max.y - 2;
            let quarter = bounds.width() / 4;
            let real_left = hash2(settings.seed, SALT_HALL, 1, 0).is_multiple_of(2);
            let left = bounds.min.x + quarter;
            let right = zone.reflect_column(left);
            if y == floor && (x == left || x == right) {
                let real = (x == left) == real_left;
                return Some(if real {
                    TileKind::Cave(crate::cave::CaveTile::TreasureCache)
                } else {
                    TileKind::Warped(WarpedTile::MirrorGhost)
                });
            }
        }
        return Some(TileKind::Air);
    }
    let folded = zone.axis_distance2(x) / 2;
    // A rock tile beside the open hall: glass lining and ore pairs.
    let beside = [(0, -1), (1, 0), (0, 1), (-1, 0)]
        .into_iter()
        .any(|(dx, dy)| {
            let (nx, ny) = (x + dx, y + dy);
            bounds.contains(TilePosition { x: nx, y: ny }) && hall_open(zone, nx, ny)
        });
    if beside {
        let pair = hash2(zone.seed, SALT_ORE, folded, y);
        let ore_odds = if variant(settings, Stratum::MirrorStrata) == 2 {
            5
        } else {
            8
        };
        if folded >= 2 && pair.is_multiple_of(ore_odds) {
            let left_real = (pair >> 20).is_multiple_of(2);
            let real = (2 * x < zone.axis2()) == left_real;
            return Some(TileKind::Warped(if real {
                WarpedTile::MirrorOre
            } else {
                WarpedTile::MirrorGhost
            }));
        }
        let glass_odds = if variant(settings, Stratum::MirrorStrata) == 0 {
            2
        } else {
            3
        };
        if hash2(zone.seed, SALT_FRAME, folded, y).is_multiple_of(glass_odds) {
            return Some(TileKind::Warped(WarpedTile::MirrorGlass));
        }
    }
    // Symmetric rock: soft and hard by the folded noise.
    let mass = value_noise(zone.seed, SALT_ROCK, folded, y, 6, 4);
    Some(if mass > NOISE_MAX * 2 / 3 {
        TileKind::HardRock
    } else {
        TileKind::Stone
    })
}

const fn base_is_rock(kind: TileKind) -> bool {
    super::is_rock(kind)
}
