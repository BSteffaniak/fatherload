//! Pixel Rot: the deeper, the lower the resolution.
//!
//! The stratum is cut into [`ROT_TIERS`] bands of rows, each a **rot tier**. Every tier is
//! coarser than the last: on screen, bigger pixels, fewer colours and more glitching
//! ([`screen_levels`], read by the field source); in the rock, the generator quantizes the
//! stratum into blocks [`block_size`] tiles on a side from the second tier down (the top tier
//! keeps the rock above it, with a scatter of single rotten pixels), so caves become blocky
//! voids and veins become squares. A thin band of rotten blocks marks where the resolution
//! first drops (a "scanline" in the rock).
//!
//! **Data ore** comes in whole blocks on the grid. A rig that mines the last tile of a data
//! block recovers a **data core** (extra layer resources and mythril, `world/distortion.rs`):
//! the grid is a pure function of the layout ([`block_at`]), so the tick tells a whole block
//! from the terrain alone.
//!
//! The **Dead Screen** ([`dead_screen`]) is a builder picture in giant pixels, the stratum's
//! landmark.

use crate::{
    descent::Stratum,
    terrain::{TileKind, TilePosition},
    worldgen::{
        SPAWN_TILE_X, TileRect, WorldSettings,
        descent::{DescentLayout, landmarks, variant},
        noise::{hash2, value_noise},
    },
};

use super::WarpedTile;

/// Rot tiers in the stratum.
pub const ROT_TIERS: usize = 5;
/// Rock block size (tiles) per tier (the top tier keeps its rock).
pub const BLOCK_SIZES: [i32; ROT_TIERS] = [1, 2, 3, 4, 5];
/// Pixelation (percent of the post-process's) per tier.
pub const PIXELATE: [u8; ROT_TIERS] = [18, 34, 52, 70, 88];
/// Palette collapse per tier.
pub const PALETTE: [u8; ROT_TIERS] = [22, 38, 56, 74, 90];
/// Sprite glitching per tier.
pub const GLITCH: [u8; ROT_TIERS] = [6, 16, 28, 42, 56];
/// Rows over which the screen eases from one tier into the next.
pub const TIER_BLEND_ROWS: f32 = 6.0;
/// `(width, height)` of the Dead Screen.
pub const DEAD_SCREEN_SIZE: (i32, i32) = (50, 30);

const SALT_VOID: u64 = 0xB17_0001;
const SALT_DATA: u64 = 0xB17_0002;
const SALT_ROT: u64 = 0xB17_0003;
const SALT_SCREEN: u64 = 0xB17_0004;

/// The rot tier of a row `along` per mille through the stratum.
#[must_use]
pub const fn tier_of_along(along: u16) -> u8 {
    let tier = along as usize * ROT_TIERS / 1_000;
    #[allow(clippy::cast_possible_truncation, reason = "below five")]
    let tier = if tier >= ROT_TIERS {
        ROT_TIERS - 1
    } else {
        tier
    } as u8;
    tier
}

/// The block size (tiles) of tier `tier`.
#[must_use]
pub const fn block_size(tier: u8) -> i32 {
    let index = tier as usize;
    if index < ROT_TIERS {
        BLOCK_SIZES[index]
    } else {
        BLOCK_SIZES[ROT_TIERS - 1]
    }
}

/// The first row of each tier's band.
#[must_use]
pub const fn tier_tops(layout: &DescentLayout) -> [i32; ROT_TIERS] {
    let (top, bottom) = layout.rows(Stratum::PixelRot);
    let span = bottom - top + 1;
    let mut tops = [0; ROT_TIERS];
    let mut index = 0;
    while index < ROT_TIERS {
        #[allow(
            clippy::cast_possible_wrap,
            clippy::cast_possible_truncation,
            reason = "five"
        )]
        let (k, tiers) = (index as i32, ROT_TIERS as i32);
        tops[index] = top + span * k / tiers;
        index += 1;
    }
    tops
}

/// The rot tier at `row` (`None` outside Pixel Rot's layout rows).
#[must_use]
pub const fn tier_at(layout: &DescentLayout, row: i32) -> Option<u8> {
    let (top, bottom) = layout.rows(Stratum::PixelRot);
    if row < top || row > bottom {
        return None;
    }
    let tops = tier_tops(layout);
    let mut tier = ROT_TIERS - 1;
    while tier > 0 && row < tops[tier] {
        tier -= 1;
    }
    #[allow(clippy::cast_possible_truncation, reason = "below five")]
    Some(tier as u8)
}

/// One block of the rot grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RotCell {
    pub tier: u8,
    /// Block coordinates in its tier's grid.
    pub bx: i32,
    pub by: i32,
    /// Its tiles (clipped to its tier's band).
    pub bounds: TileRect,
}

/// The block of the rot grid holding tile `(x, y)` (`None` outside Pixel Rot's rows). Blocks
/// are aligned to their tier band's first row.
#[must_use]
pub const fn block_at(layout: &DescentLayout, x: i32, y: i32) -> Option<RotCell> {
    let Some(tier) = tier_at(layout, y) else {
        return None;
    };
    let tops = tier_tops(layout);
    let band_top = tops[tier as usize];
    let band_end = if (tier as usize) + 1 < ROT_TIERS {
        tops[tier as usize + 1] - 1
    } else {
        layout.rows(Stratum::PixelRot).1
    };
    let size = block_size(tier);
    let bx = x.div_euclid(size);
    let by = (y - band_top).div_euclid(size);
    let min_y = band_top + by * size;
    let max_y = min_y + size - 1;
    Some(RotCell {
        tier,
        bx,
        by,
        bounds: TileRect {
            min: TilePosition {
                x: bx * size,
                y: min_y,
            },
            max: TilePosition {
                x: bx * size + size - 1,
                y: if max_y > band_end { band_end } else { max_y },
            },
        },
    })
}

/// What a block of the rot grid is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellFill {
    /// Leave the rock as generated (the top tier).
    Keep,
    /// A blocky void.
    Void,
    /// Plain rock.
    Rock,
    /// A rotten block.
    Rot,
    /// Data ore (a whole block pays a data core).
    Data,
}

/// The fill of `cell` in `settings`.
#[must_use]
pub fn cell_fill(settings: &WorldSettings, cell: &RotCell) -> CellFill {
    let seed = settings.seed;
    let salt = |base: u64| base ^ (u64::from(cell.tier) << 40);
    let roll = hash2(seed, salt(SALT_DATA), cell.bx, cell.by);
    // "Glitch Fields" rots more blocks, "Low Res" has more data.
    let pick = variant(settings, Stratum::PixelRot);
    let data_odds = if pick == 0 { 17 } else { 23 };
    let rot_odds = if pick == 1 { 5 } else { 8 };
    if cell.tier == 0 {
        // The top tier: the rock above, with single rotten pixels and data specks.
        return if roll.is_multiple_of(29) {
            CellFill::Data
        } else if roll.is_multiple_of(43) {
            CellFill::Rot
        } else {
            CellFill::Keep
        };
    }
    // Blocky voids from coarse noise sampled per block (bigger voids deeper).
    let void = value_noise(seed, salt(SALT_VOID), cell.bx, cell.by, 5, 4);
    let threshold = 47_500 - i32::from(cell.tier) * 900;
    if void > threshold {
        return CellFill::Void;
    }
    if roll.is_multiple_of(data_odds) {
        CellFill::Data
    } else if (roll >> 16).is_multiple_of(rot_odds) {
        CellFill::Rot
    } else {
        CellFill::Rock
    }
}

/// Whether `cell` is a data ore block.
#[must_use]
pub fn is_data_block(settings: &WorldSettings, cell: &RotCell) -> bool {
    cell_fill(settings, cell) == CellFill::Data
}

/// The Dead Screen: a builder picture in giant pixels, three fifths down Pixel Rot on the
/// outposts' far side (one per Deep Descent world).
#[must_use]
pub const fn dead_screen(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !super::has_distortion(settings) {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::PixelRot);
    let roll = hash2(settings.seed, SALT_SCREEN, 0, 0);
    let side = if landmarks::outpost_column(settings) >= SPAWN_TILE_X {
        -1
    } else {
        1
    };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "small"
    )]
    let centre = SPAWN_TILE_X + side * (50 + (roll % 60) as i32);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "small"
    )]
    let middle = top + (bottom - top) / 2 + ((roll >> 12) % 21) as i32 - 10;
    Some(TileRect {
        min: TilePosition {
            x: centre - DEAD_SCREEN_SIZE.0 / 2,
            y: middle - DEAD_SCREEN_SIZE.1 / 2,
        },
        max: TilePosition {
            x: centre - DEAD_SCREEN_SIZE.0 / 2 + DEAD_SCREEN_SIZE.0 - 1,
            y: middle - DEAD_SCREEN_SIZE.1 / 2 + DEAD_SCREEN_SIZE.1 - 1,
        },
    })
}

/// The Dead Screen's picture (one character per 2x2 block of tiles; `#` a lit pixel, `.` the
/// screen's dark, ` ` open air in front of it): the cable from the hanging stone to a sleeping
/// star, and a builder glyph.
const PICTURE: [&str; 11] = [
    "......##.............",
    ".....####.......#.#..",
    "......##........###..",
    ".......#.........#...",
    ".......#.............",
    ".......#.......##.##.",
    ".......#........###..",
    "......###......##.##.",
    ".....#####...........",
    "......###............",
    ".......#.............",
];

/// The tile the Dead Screen puts at `(x, y)` over `base`, or `None` outside it.
///
/// A frame of rotten blocks, the screen (data-dark rock) with its lit pixels, and an open
/// gallery in front of it along the floor.
#[must_use]
pub fn dead_screen_tile(screen: &TileRect, x: i32, y: i32, base: TileKind) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if !screen.contains(at) || !super::carvable(base) {
        return None;
    }
    let (left, right, top, bottom) = (
        x - screen.min.x,
        screen.max.x - x,
        y - screen.min.y,
        screen.max.y - y,
    );
    let edge = left.min(right).min(top).min(bottom);
    if edge < 2 {
        let door = (left < 2 || right < 2) && (2..=4).contains(&bottom);
        return Some(if door {
            TileKind::Air
        } else {
            TileKind::Warped(WarpedTile::RotBlock)
        });
    }
    // The gallery: the lower third is open, so the picture is seen from below.
    let gallery_top = screen.max.y - 9;
    if y >= gallery_top {
        if bottom == 2 {
            return Some(TileKind::Warped(WarpedTile::RotBlock));
        }
        return Some(TileKind::Air);
    }
    // The picture, centred, two tiles a pixel.
    let width = i32::try_from(PICTURE[0].len()).unwrap_or(0) * 2;
    let ox = screen.min.x + (screen.width() - width) / 2;
    let oy = screen.min.y + 2;
    let (px, py) = ((x - ox).div_euclid(2), (y - oy).div_euclid(2));
    let lit = usize::try_from(py)
        .ok()
        .and_then(|row| PICTURE.get(row))
        .and_then(|line| {
            usize::try_from(px)
                .ok()
                .and_then(|col| line.as_bytes().get(col))
        })
        .is_some_and(|c| *c == b'#');
    Some(if lit {
        TileKind::Warped(WarpedTile::ScreenPixel)
    } else {
        TileKind::HardRock
    })
}

/// The tile the rot grid puts at `(x, y)` over `base`, or `None` to leave it.
#[must_use]
pub fn rot_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    if !super::carvable(base) {
        return None;
    }
    let cell = block_at(layout, x, y)?;
    let fill = cell_fill(settings, &cell);
    // The scanline: where the resolution first drops, a band of rotten blocks with gaps.
    let tops = tier_tops(layout);
    if (y == tops[1] || y == tops[1] + 1) && super::is_rock(base) {
        let gap = hash2(settings.seed, SALT_ROT, x.div_euclid(3), 0).is_multiple_of(3);
        if !gap {
            return Some(TileKind::Warped(WarpedTile::RotBlock));
        }
    }
    match fill {
        CellFill::Keep => None,
        CellFill::Void => Some(TileKind::Air),
        CellFill::Rock => {
            // Keep the strata's own ore where a block stays rock; the rest is block-flat rock.
            if matches!(base, TileKind::Ore(_)) {
                return None;
            }
            let hard = hash2(settings.seed, SALT_ROT, cell.bx, cell.by).is_multiple_of(4);
            Some(if hard {
                TileKind::HardRock
            } else {
                TileKind::Stone
            })
        }
        CellFill::Rot => Some(TileKind::Warped(WarpedTile::RotBlock)),
        CellFill::Data => Some(TileKind::Warped(WarpedTile::DataOre)),
    }
}

/// The screen levels (pixelate, palette, glitch; percent) at fractional `row`, easing across
/// [`TIER_BLEND_ROWS`] at each tier boundary.
#[must_use]
pub fn screen_levels(layout: &DescentLayout, row: f32) -> (f32, f32, f32) {
    let tops = tier_tops(layout);
    #[allow(clippy::cast_possible_truncation, reason = "tile rows fit i32")]
    let whole = row.floor() as i32;
    let tier = usize::from(tier_at(layout, whole).unwrap_or(0));
    let level = |table: &[u8; ROT_TIERS], index: usize| f32::from(table[index.min(ROT_TIERS - 1)]);
    let mut levels = (
        level(&PIXELATE, tier),
        level(&PALETTE, tier),
        level(&GLITCH, tier),
    );
    // Ease up from the tier above over the first rows of this one.
    if tier > 0 {
        #[allow(clippy::cast_precision_loss, reason = "rows are small")]
        let into = row - tops[tier] as f32;
        if into < TIER_BLEND_ROWS {
            let t = (into / TIER_BLEND_ROWS).clamp(0.0, 1.0);
            let mix = |above: f32, here: f32| (here - above).mul_add(t, above);
            levels = (
                mix(level(&PIXELATE, tier - 1), levels.0),
                mix(level(&PALETTE, tier - 1), levels.1),
                mix(level(&GLITCH, tier - 1), levels.2),
            );
        }
    }
    levels
}
