//! Tile art for the distortion strata (Wave 9, Descent VI part A,
//! `fatherload_simulation::distortion`), and their back walls.
//!
//! * Mirror Strata: silvered glass in diagonal panes with a bright streak; real mirror ore is
//!   a cluster of silver-blue crystals with a dark shadow under each and a warm glint on its
//!   upper left; its reflection is the same cluster *flat*: no shadows, a cold cast and the glint
//!   on the upper right (and it breathes, see `rendering::world::distortion`). The back wall is a
//!   dim hall of mirrors: vertical panes with pale reflections of light.
//! * Gravity Shear: flow stone in streaked magenta bands, shear plates with rivets and a chevron
//!   (the renderer draws the field's flow over them). The back wall leans: diagonal strata
//!   lines, as if the whole place were tipped.
//! * Pixel Rot: data ore as square lime cells with a bright bit pattern; rotten blocks as flat,
//!   glitching squares of a few colours with a scan line; the Dead Screen's lit pixels glow.
//!   The back wall is a coarse grid of dark squares that loses detail deeper (its own pixels
//!   grow with the rot tier).

use fatherload_simulation::{
    descent::{Stratum, fields::cached_layout},
    distortion::{WarpedTile, rot},
    terrain::{Terrain, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{
    Cell, Edges, Ramp, Strata, TILE_ART, TileCanvas, hash, noise, paint_edges, ramp, slab_level,
};

const GLASS: Ramp = ramp([SLATE_DARK, SLATE, MIST, SKY_LIGHT]);
const SILVER: Ramp = ramp([STEEL_DARK, STEEL, STEEL_LIGHT, ICE]);
const FLOW: Ramp = ramp([NIGHT, PLUM, ROSE, PINK]);
const PLATE: Ramp = ramp([NIGHT, STEEL_DARK, STEEL, LAVENDER]);
const DATA: Ramp = ramp([INK, TEAL_DARK, TEAL, LIME]);
const ROT: Ramp = ramp([INK, PLUM, TEAL_DARK, PINK]);

/// Representative colour of a warped tile (drill dust, debris, the map).
#[must_use]
pub const fn warped_color(tile: WarpedTile) -> raylib::prelude::Color {
    match tile {
        WarpedTile::MirrorGlass => MIST,
        WarpedTile::MirrorOre | WarpedTile::MirrorGhost => ICE,
        WarpedTile::ShearStone => ROSE,
        WarpedTile::ShearPlate => STEEL,
        WarpedTile::DataOre => LIME,
        WarpedTile::RotBlock => PLUM,
        WarpedTile::ScreenPixel => MINT,
    }
}

/// The distortion stratum at `position` (`None` elsewhere and on older worlds).
#[must_use]
pub(super) fn distortion_stratum(terrain: &Terrain, position: TilePosition) -> Option<Stratum> {
    if !terrain.settings().generator.has_descents() {
        return None;
    }
    let stratum = terrain.stratum_at(position);
    fatherload_simulation::distortion::is_distortion(stratum).then_some(stratum)
}

/// One art pixel of `tile`'s body at absolute art pixel `(x, y)`.
fn body_pixel(tile: WarpedTile, x: i32, y: i32, rot_cell: i32) -> PaletteIndex {
    match tile {
        WarpedTile::MirrorGlass => {
            // Diagonal panes, a bright streak across each.
            let pane = (x + y).div_euclid(7);
            let along = (x + y).rem_euclid(7);
            let seam = (x - y).rem_euclid(9) == 0;
            if seam {
                GLASS[0]
            } else if along == 1 && hash(pane, 0, 901).is_multiple_of(2) {
                GLASS[3]
            } else if (x - y + pane).rem_euclid(5) == 0 {
                GLASS[2]
            } else {
                GLASS[1 + usize::from(noise(x, y, 6, 903) > 0.6)]
            }
        }
        WarpedTile::MirrorOre | WarpedTile::MirrorGhost => {
            // The host rock: dark silvered stone.
            SILVER[usize::from(slab_level(x, y) > 1)]
        }
        WarpedTile::ShearStone => {
            // Streaked bands leaning one way, flecks of light.
            let band = (x * 2 + y + (noise(x, y, 10, 905) * 4.0) as i32).rem_euclid(6);
            if hash(x, y, 906).is_multiple_of(47) {
                FLOW[3]
            } else {
                FLOW[match band {
                    0 => 0,
                    1 | 2 => 1,
                    _ => 2,
                }]
            }
        }
        WarpedTile::ShearPlate => {
            let (px, py) = (x.rem_euclid(16), y.rem_euclid(16));
            if px == 0 || py == 0 || px == 15 || py == 15 {
                PLATE[0]
            } else if (px == 2 || px == 13) && (py == 2 || py == 13) {
                PLATE[3]
            } else if py <= 1 || px <= 1 {
                PLATE[2]
            } else {
                PLATE[1 + usize::from(hash(x, y, 907).is_multiple_of(9))]
            }
        }
        WarpedTile::DataOre => {
            // Square cells of data, a lit bit here and there; the cell grid follows the rot.
            let cell = rot_cell.max(4);
            let (cx, cy) = (x.rem_euclid(cell), y.rem_euclid(cell));
            if cx == 0 || cy == 0 {
                DATA[0]
            } else if hash(x.div_euclid(2), y.div_euclid(2), 908).is_multiple_of(5) {
                DATA[3]
            } else if cx == 1 || cy == 1 {
                DATA[2]
            } else {
                DATA[1]
            }
        }
        WarpedTile::RotBlock => {
            // Flat squares of a few colours, a scan line, a dead column.
            let cell = rot_cell.max(4);
            let block = hash(x.div_euclid(cell), y.div_euclid(cell), 909);
            if y.rem_euclid(cell * 2) == 0 {
                ROT[0]
            } else {
                ROT[usize::try_from(block % 4).unwrap_or(0)]
            }
        }
        WarpedTile::ScreenPixel => {
            let (px, py) = (x.rem_euclid(16), y.rem_euclid(16));
            if px == 0 || py == 0 {
                index_of(TEAL)
            } else if px <= 2 && py <= 2 {
                index_of(BONE)
            } else {
                index_of(MINT)
            }
        }
    }
}

/// A cluster of mirror ore crystals: real (shadows, warm glint upper left) or a reflection
/// (flat, cold, its glint on the upper right).
fn paint_mirror_ore(canvas: &mut TileCanvas, real: bool, pick: u32) {
    // Three crystals, always placed the same so a pair matches at a glance.
    let crystals: [(i32, i32, i32); 3] = [(4, 9, 3), (9, 6, 4), (11, 11, 2)];
    let shift = i32::try_from(pick % 2).unwrap_or(0);
    for (cx, cy, size) in crystals {
        let cx = cx + shift;
        // A reflection's crystals are mirrored left to right.
        let cx = if real { cx } else { TILE_ART - 1 - cx };
        for dy in -size..=size {
            let half = size - dy.abs();
            for dx in -half..=half {
                let (x, y) = (cx + dx, cy + dy);
                let lit = if real { dx < 0 } else { dx > 0 };
                let index = if dy <= -size + 1 && dx.abs() <= 1 {
                    index_of(if real { BONE } else { ICE })
                } else if lit {
                    index_of(if real { ICE } else { SKY })
                } else {
                    index_of(if real { SKY } else { SKY_LIGHT })
                };
                canvas.set(x, y, index);
            }
        }
        if real {
            // A dark shadow under each real crystal.
            for dx in -1..=1 {
                canvas.set(cx + dx, cy + size + 1, index_of(INK));
            }
            canvas.set(cx - size, cy - 1, index_of(GOLD));
        } else {
            // A reflection catches cold light on the far side.
            canvas.set(cx + size, cy - 1, index_of(SKY_LIGHT));
        }
    }
}

/// A shear plate's chevron, pointing the way its rivets were set (a direction cue).
fn paint_chevron(canvas: &mut TileCanvas) {
    for step in 0..4 {
        canvas.set(6 + step, 6 + step, index_of(LAVENDER));
        canvas.set(6 + step, 9 - step, index_of(LAVENDER));
    }
}

/// Paints warped tile `tile` at `position`.
pub(super) fn paint(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: WarpedTile,
    strata: &Strata,
    edges: Edges,
) {
    let rot_cell = rot_cell_pixels(terrain, position);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, body_pixel(tile, gx, gy, rot_cell));
        }
    }
    let pick = hash(position.x, position.y, 920);
    match tile {
        WarpedTile::MirrorOre => paint_mirror_ore(canvas, true, pick),
        WarpedTile::MirrorGhost => paint_mirror_ore(canvas, false, pick),
        WarpedTile::ShearPlate => paint_chevron(canvas),
        _ => {}
    }
    let (lip, shade) = match tile {
        WarpedTile::MirrorGlass | WarpedTile::MirrorGhost => (GLASS[3], GLASS[0]),
        WarpedTile::MirrorOre => (SILVER[3], SILVER[0]),
        WarpedTile::ShearStone => (FLOW[3], FLOW[0]),
        WarpedTile::ShearPlate => (PLATE[3], PLATE[0]),
        WarpedTile::DataOre | WarpedTile::ScreenPixel => (DATA[3], DATA[0]),
        WarpedTile::RotBlock => (ROT[3], ROT[0]),
    };
    // A reflection has no rim shadow: it is flat.
    let shade = if tile == WarpedTile::MirrorGhost {
        lip
    } else {
        shade
    };
    let square = matches!(
        tile,
        WarpedTile::ShearPlate
            | WarpedTile::DataOre
            | WarpedTile::RotBlock
            | WarpedTile::ScreenPixel
    );
    paint_edges(canvas, strata, edges, lip, shade, square);
}

/// The rot grid's pixel size (art pixels) at `position`: one tile's art per block side in
/// Pixel Rot's tiers (4 above it), so data ore and rotten blocks coarsen with the rot.
fn rot_cell_pixels(terrain: &Terrain, position: TilePosition) -> i32 {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return 4;
    }
    let layout = cached_layout(settings);
    rot::tier_at(&layout, position.y).map_or(4, |tier| 4 + i32::from(tier) * 3)
}

/// One art pixel of `stratum`'s back wall at absolute art pixel `(x, y)`.
#[must_use]
fn back_pixel(stratum: Stratum, x: i32, y: i32, rot_cell: i32) -> PaletteIndex {
    match stratum {
        Stratum::MirrorStrata => {
            // A dim hall of mirrors: tall panes, pale reflections sliding down them.
            let pane = x.rem_euclid(19);
            let streak = (y + x.div_euclid(19) * 7).rem_euclid(29);
            if pane == 0 {
                index_of(INK)
            } else if pane == 1 {
                index_of(SLATE_DARK)
            } else if (pane == 4 || pane == 5) && streak < 9 {
                index_of(SLATE)
            } else if hash(x, y, 931).is_multiple_of(211) {
                index_of(MIST)
            } else if noise(x, y, 12, 933) + f32::from(bayer4(x, y)) / 40.0 > 0.66 {
                index_of(SLATE_DARK)
            } else {
                index_of(NIGHT)
            }
        }
        Stratum::GravityShear => {
            // Strata lines tipped on their side, as if the whole place leaned.
            let line = (x * 3 - y + (noise(x, y, 16, 941) * 6.0) as i32).rem_euclid(17);
            if line == 0 {
                index_of(PLUM)
            } else if line == 1 {
                index_of(INK)
            } else if hash(x, y, 943).is_multiple_of(173) {
                index_of(ROSE)
            } else if noise(x, y, 9, 945) > 0.68 {
                index_of(PLUM)
            } else {
                index_of(NIGHT)
            }
        }
        _ => {
            // Coarse dark squares; the squares grow with the rot.
            let cell = rot_cell.max(4);
            let block = hash(x.div_euclid(cell), y.div_euclid(cell), 951);
            if x.rem_euclid(cell) == 0 || y.rem_euclid(cell) == 0 {
                index_of(INK)
            } else if block.is_multiple_of(13) {
                index_of(TEAL_DARK)
            } else if block.is_multiple_of(29) {
                index_of(PLUM)
            } else {
                index_of(NIGHT)
            }
        }
    }
}

/// Paints an open tile of `stratum`: its back wall, shadowed under rock overhead and beside
/// rock walls (like every tunnel).
pub(super) fn paint_back(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    stratum: Stratum,
    (up, left, right): (Cell, Cell, Cell),
) {
    let shade = index_of(INK);
    let rot_cell = rot_cell_pixels(terrain, position);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let shadowed = (up == Cell::Solid && (py < 2 || (py < 4 && bayer4(gx, gy) < 6)))
                || (left == Cell::Solid && (px == 0 || (px == 1 && bayer4(gx, gy) < 8)))
                || (right == Cell::Solid
                    && (px == TILE_ART - 1 || (px == TILE_ART - 2 && bayer4(gx, gy) < 8)));
            let index = if shadowed {
                shade
            } else {
                back_pixel(stratum, gx, gy, rot_cell)
            };
            canvas.set(px, py, index);
        }
    }
}
