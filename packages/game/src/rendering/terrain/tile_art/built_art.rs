//! Tile art for solid building pieces (`fatherload_simulation::base::BuiltTile`).
//!
//! Built pieces read as made, not dug: every wall is coursed masonry in its block's colours
//! (rammed earth in banded layers, fired brick, dressed stone, big granite ashlars), plating is
//! riveted steel panels, windows are steel-framed panes of night-blue glass with a glint, and
//! plank floors are a timber deck on a cross-braced trestle. Neighbouring pieces join without
//! seams; faces that meet open air get the usual ink outline and lit lip.

use fatherload_simulation::{base::BuiltTile, item::BlockKind};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, TRANSPARENT, index_of};

use super::{
    Cell, Edges, Ramp, Strata, TILE_ART, TileCanvas, backdrop_pixel, hash, paint_edges, ramp,
};

const EARTH: Ramp = ramp([SOIL_DARK, SOIL, LOAM, SAND]);
const BRICK: Ramp = ramp([MAROON, RUST, COPPER, CLAY]);
const DRESSED: Ramp = ramp([STONE_DARK, STONE, STONE_LIGHT, MIST]);
const GRANITE: Ramp = ramp([NIGHT, SLATE_DARK, SLATE, SLATE_LIGHT]);
const STEEL_RAMP: Ramp = ramp([SLATE_DARK, STEEL_DARK, STEEL, STEEL_LIGHT]);
const TIMBER: Ramp = ramp([SOIL_DARK, SOIL, RUST, AMBER]);
/// Ancient ruin masonry: cold blue-green stone (WP-PILOT ruins).
const RUIN: Ramp = ramp([NIGHT, SLATE_DARK, TEAL_DARK, TEAL]);
/// Old bronze for ruin mechanisms.
const BRONZE: Ramp = ramp([SOIL_DARK, RUST, COPPER, AMBER]);
/// Side-view tomb masonry: sun-baked sandstone (WP-TOMBS).
const TOMB_SANDSTONE: Ramp = ramp([SOIL, LOAM, CLAY, SAND]);
/// Top-down crypt masonry: cold violet-grey crypt stone (WP-TOMBS).
const CRYPT_STONE: Ramp = ramp([NIGHT, SLATE_DARK, PLUM, SLATE_LIGHT]);

/// The masonry ramp of a tomb style index (0 tomb sandstone, 1 crypt stone).
#[must_use]
pub const fn tomb_ramp(style: u8) -> Ramp {
    if style == 1 {
        CRYPT_STONE
    } else {
        TOMB_SANDSTONE
    }
}

/// The colour of a ruin gate channel (lintel gems, lever knobs, plate insets).
#[must_use]
pub const fn channel_color(channel: u8) -> raylib::prelude::Color {
    match channel % 4 {
        0 => AMBER,
        1 => ROSE,
        2 => SKY,
        _ => LIME,
    }
}

/// The colour of a keycard (doors, chests and the card itself).
#[must_use]
pub const fn keycard_color(color: u8) -> raylib::prelude::Color {
    match color % 4 {
        0 => RED,
        1 => SKY,
        2 => LEAF,
        _ => GOLD,
    }
}

/// The darkest to brightest shades of a built tile (lip and shade of its open faces).
#[must_use]
pub const fn built_ramp(tile: BuiltTile) -> Ramp {
    match tile {
        BuiltTile::Wall(BlockKind::Dirt) => EARTH,
        BuiltTile::Wall(BlockKind::Clay) => BRICK,
        BuiltTile::Wall(BlockKind::Stone) => DRESSED,
        BuiltTile::Wall(BlockKind::HardRock) => GRANITE,
        BuiltTile::Plating
        | BuiltTile::Glass
        | BuiltTile::VentGrate
        | BuiltTile::Gate
        | BuiltTile::KeycardDoor(_) => STEEL_RAMP,
        BuiltTile::Plank | BuiltTile::Chest { .. } | BuiltTile::Palisade => TIMBER,
        BuiltTile::RuinWall | BuiltTile::GateLintel(_) | BuiltTile::Lever { .. } => RUIN,
        BuiltTile::PressurePlate { .. } => BRONZE,
        BuiltTile::TombWall(style)
        | BuiltTile::TombPillar(style)
        | BuiltTile::TombDoor { style, .. } => tomb_ramp(style),
        BuiltTile::TombShrine { .. }
        | BuiltTile::TombStairs
        | BuiltTile::TombExit
        | BuiltTile::TombCache { .. }
        | BuiltTile::TombTrap { .. } => TOMB_SANDSTONE,
    }
}

/// Coursed masonry: `course` rows tall, `length` wide, alternate courses offset by half, a
/// mortar line of `mortar` between, a lit top edge and a shaded bottom edge per block.
const fn masonry(
    x: i32,
    y: i32,
    course: i32,
    length: i32,
    colors: Ramp,
    mortar: PaletteIndex,
) -> PaletteIndex {
    let row = y.div_euclid(course);
    let offset = if row % 2 == 0 { 0 } else { length / 2 };
    let column = (x + offset).rem_euclid(length);
    let row_y = y.rem_euclid(course);
    let block = (x + offset).div_euclid(length);
    if row_y == course - 1 || column == length - 1 {
        return mortar;
    }
    let tone = hash(block, row, 211) % 5;
    if row_y == 0 {
        colors[3]
    } else if column == length - 2 || row_y == course - 2 {
        colors[0]
    } else if tone == 0 {
        colors[1]
    } else if hash(x, y, 223).is_multiple_of(23) {
        colors[3]
    } else {
        colors[2]
    }
}

/// Rammed earth: wavy horizontal layers with pebbles.
const fn earth(x: i32, y: i32) -> PaletteIndex {
    let wave = (hash(x.div_euclid(5), 0, 233) % 2) as i32;
    let layer = (y + wave).rem_euclid(5);
    if hash(x, y, 239).is_multiple_of(31) {
        return EARTH[3];
    }
    match layer {
        0 => EARTH[3],
        1 | 2 => EARTH[2],
        3 => EARTH[1],
        _ => EARTH[0],
    }
}

/// Riveted plating: 8x8 panels with rivets in the corners and a brushed sheen.
const fn plating(x: i32, y: i32) -> PaletteIndex {
    let (px, py) = (x.rem_euclid(8), y.rem_euclid(8));
    if px == 7 || py == 7 {
        return STEEL_RAMP[0];
    }
    if (px == 1 || px == 5) && (py == 1 || py == 5) {
        return STEEL_RAMP[3];
    }
    if py == 0 || px == 0 {
        return STEEL_RAMP[3];
    }
    if (x + y).rem_euclid(9) == 0 {
        STEEL_RAMP[3]
    } else if px >= 5 || py >= 5 {
        STEEL_RAMP[1]
    } else {
        STEEL_RAMP[2]
    }
}

/// Paints built tile `tile` at world tile `(x, y)`.
pub(super) fn paint(
    canvas: &mut TileCanvas,
    tile: BuiltTile,
    strata: &Strata,
    edges: Edges,
    position: (i32, i32),
) {
    let (origin_x, origin_y) = (canvas.origin_x, canvas.origin_y);
    let colors = built_ramp(tile);
    match tile {
        BuiltTile::Wall(block) => {
            for py in 0..TILE_ART {
                for px in 0..TILE_ART {
                    let (x, y) = (origin_x + px, origin_y + py);
                    let pixel = match block {
                        BlockKind::Dirt => earth(x, y),
                        BlockKind::Clay => masonry(x, y, 4, 8, BRICK, index_of(SAND)),
                        BlockKind::Stone => masonry(x, y, 6, 10, DRESSED, index_of(SLATE_DARK)),
                        BlockKind::HardRock => masonry(x, y, 8, 16, GRANITE, index_of(INK)),
                    };
                    canvas.set(px, py, pixel);
                }
            }
        }
        BuiltTile::Plating => {
            for py in 0..TILE_ART {
                for px in 0..TILE_ART {
                    canvas.set(px, py, plating(origin_x + px, origin_y + py));
                }
            }
        }
        BuiltTile::Glass => paint_glass(canvas, position),
        BuiltTile::VentGrate => paint_vent(canvas, position),
        BuiltTile::Plank => {
            paint_plank(canvas, strata, edges);
            return;
        }
        BuiltTile::RuinWall => paint_ruin_wall(canvas, position),
        BuiltTile::GateLintel(channel) => {
            paint_ruin_wall(canvas, position);
            paint_lintel(canvas, channel);
        }
        BuiltTile::Lever { channel, on } => {
            paint_ruin_wall(canvas, position);
            paint_lever(canvas, channel, on);
        }
        BuiltTile::PressurePlate { channel, down } => {
            paint_ruin_wall(canvas, position);
            paint_plate(canvas, channel, down);
        }
        BuiltTile::Gate => paint_gate(canvas, strata, edges),
        BuiltTile::KeycardDoor(color) => paint_keycard_door(canvas, color, position),
        BuiltTile::Chest { open, card } => {
            paint_chest(canvas, strata, edges, open, card);
        }
        BuiltTile::Palisade => {
            paint_palisade(canvas, strata, edges, position);
            return;
        }
        BuiltTile::TombWall(style) => paint_tomb_wall(canvas, style, position),
        BuiltTile::TombPillar(style) => {
            paint_tomb_pillar(canvas, strata, edges, style);
            return;
        }
        BuiltTile::TombDoor { style, sealed } => {
            paint_tomb_door(canvas, style, sealed, position);
        }
        BuiltTile::TombShrine { lit } => {
            paint_tomb_shrine(canvas, strata, edges, lit);
            return;
        }
        BuiltTile::TombStairs => paint_tomb_stairs(canvas),
        BuiltTile::TombExit => paint_tomb_exit(canvas),
        BuiltTile::TombCache { open } => {
            paint_tomb_cache(canvas, strata, edges, open);
            return;
        }
        BuiltTile::TombTrap { hidden, .. } => {
            paint_tomb_trap(canvas, strata, edges, hidden);
            return;
        }
    }
    paint_edges(canvas, strata, edges, colors[3], colors[0], true);
}

/// A crawlway's vent grate (Part II F5): a riveted steel frame around slanted louvres with
/// dark gaps, a rust streak per tile and a warm hazard tab on top, so pilots read it as "a way
/// through" and rig drivers as a wall.
fn paint_vent(canvas: &mut TileCanvas, (tile_x, tile_y): (i32, i32)) {
    let last = TILE_ART - 1;
    let rust = 3 + (hash(tile_x, tile_y, 257) % (TILE_ART as u32 - 6)) as i32;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let outer = px == 0 || py == 0 || px == last || py == last;
            let frame = px <= 1 || py <= 1 || px >= last - 1 || py >= last - 1;
            let rivet = (px == 2 || px == last - 2) && (py == 2 || py == last - 2);
            let pixel = if outer {
                index_of(INK)
            } else if rivet {
                STEEL_RAMP[3]
            } else if frame {
                if py <= 1 {
                    if (px / 2) % 2 == 0 {
                        index_of(AMBER)
                    } else {
                        index_of(INK)
                    }
                } else {
                    STEEL_RAMP[2]
                }
            } else {
                // Louvres: a lit slat, its shaded underside, then a gap into the dark.
                match (py - 2).rem_euclid(4) {
                    0 => STEEL_RAMP[3],
                    1 => STEEL_RAMP[1],
                    _ if px == rust && py > 3 => index_of(RUST),
                    _ => index_of(NIGHT),
                }
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// A steel-framed window pane: deep blue glass, a diagonal glint and a mullion cross.
fn paint_glass(canvas: &mut TileCanvas, (tile_x, tile_y): (i32, i32)) {
    let last = TILE_ART - 1;
    let glint = (hash(tile_x, tile_y, 241) % 6) as i32;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let frame = px <= 1 || py <= 1 || px >= last - 1 || py >= last - 1;
            let mullion = px == 7 || py == 7;
            let pixel = if frame {
                if px == 0 || py == 0 || px == last || py == last {
                    index_of(INK)
                } else {
                    STEEL_RAMP[2]
                }
            } else if mullion {
                STEEL_RAMP[1]
            } else {
                let diagonal = (px + py + glint).rem_euclid(13);
                if diagonal == 0 || diagonal == 1 {
                    index_of(ICE)
                } else if diagonal == 2 {
                    index_of(SKY)
                } else if py < 7 {
                    index_of(OCEAN)
                } else {
                    index_of(DEEP_BLUE)
                }
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// A palisade (WP-NIGHT): three sharpened logs side by side, bark-dark with a lit edge, bound
/// by a rope lashing; the top piece of a run is cut to points, and the slits between logs
/// show whatever is behind.
fn paint_palisade(
    canvas: &mut TileCanvas,
    strata: &Strata,
    edges: Edges,
    (tile_x, tile_y): (i32, i32),
) {
    let pointed = edges.up.is_open();
    let behind = |canvas: &TileCanvas, px: i32, py: i32| {
        if edges.up == Cell::Sky || edges.left == Cell::Sky || edges.right == Cell::Sky {
            TRANSPARENT
        } else {
            backdrop_pixel(strata, canvas.origin_x + px, canvas.origin_y + py)
        }
    };
    let lash = 9 + (hash(tile_x, tile_y, 263) % 3) as i32;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            // Three logs five pixels wide with a one-pixel slit after each.
            let log = px.div_euclid(5).min(2);
            let across = px - log * 5;
            let slit = px == 15 || across == 4 && log < 2;
            // Each log's sharpened point: a triangle over its top five rows.
            let height = (hash(tile_x * 3 + log, tile_y, 269) % 2) as i32;
            let tip = if pointed {
                let middle = 2 - (across - 2).abs();
                5 - middle * 2 - height
            } else {
                0
            };
            let pixel = if slit || py < tip {
                behind(canvas, px, py)
            } else if across == 0 || across == 3 || py == tip && pointed {
                index_of(INK)
            } else if py == lash || py == lash + 1 {
                // The rope lashing.
                if (px + py) % 2 == 0 {
                    index_of(SAND)
                } else {
                    index_of(CLAY)
                }
            } else if across == 1 {
                TIMBER[3]
            } else if hash(px + canvas.origin_x, py + canvas.origin_y, 271).is_multiple_of(7) {
                TIMBER[0]
            } else {
                TIMBER[1]
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// A timber deck on a cross-braced trestle; the gaps show whatever is behind.
fn paint_plank(canvas: &mut TileCanvas, strata: &Strata, edges: Edges) {
    let last = TILE_ART - 1;
    let behind = |canvas: &TileCanvas, px: i32, py: i32| {
        if edges.up == Cell::Sky {
            TRANSPARENT
        } else {
            backdrop_pixel(strata, canvas.origin_x + px, canvas.origin_y + py)
        }
    };
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let pixel = if py == 0 {
                index_of(INK)
            } else if py == 1 {
                TIMBER[3]
            } else if py <= 4 {
                // Boards, with butt joints staggered per row.
                let joint = (px + py * 5 + canvas.origin_x).rem_euclid(9) == 0;
                if joint {
                    TIMBER[0]
                } else if py == 4 {
                    TIMBER[1]
                } else {
                    TIMBER[2]
                }
            } else if py == 5 {
                index_of(INK)
            } else if px <= 1 || px >= last - 1 {
                // Posts.
                if px == 0 || px == last {
                    index_of(INK)
                } else {
                    TIMBER[1]
                }
            } else if (px - (py - 5)).abs() <= 0 || (px - (last - (py - 5))).abs() <= 0 {
                TIMBER[1]
            } else {
                behind(canvas, px, py)
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// Ancient coursed stone with moss specks and, now and then, a faintly glowing glyph.
fn paint_ruin_wall(canvas: &mut TileCanvas, (tile_x, tile_y): (i32, i32)) {
    let glyph = hash(tile_x, tile_y, 271).is_multiple_of(13);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
            let mut pixel = if hash(x, y, 277).is_multiple_of(23) {
                index_of(MOSS)
            } else {
                masonry(x, y, 6, 11, RUIN, index_of(NIGHT))
            };
            if glyph && (5..=10).contains(&px) && (4..=11).contains(&py) {
                // A little ring-and-bar glyph carved into the block.
                let ring = ((px == 5 || px == 10) && (5..=8).contains(&py))
                    || ((py == 4 || py == 9) && (6..=9).contains(&px));
                let bar = px == 7 && py >= 9;
                if ring || bar {
                    pixel = index_of(MINT);
                }
            }
            canvas.set(px, py, pixel);
        }
    }
}

/// A gate lintel: a heavy dark keystone with the channel's gem set in it.
fn paint_lintel(canvas: &mut TileCanvas, channel: u8) {
    let gem = index_of(channel_color(channel));
    for py in 3..TILE_ART {
        for px in 2..=13 {
            let pixel = if px == 2 || px == 13 || py == TILE_ART - 1 {
                index_of(INK)
            } else if (6..=9).contains(&px) && (6..=9).contains(&py) {
                if px == 6 && py == 6 {
                    index_of(CREAM)
                } else {
                    gem
                }
            } else if py == 3 {
                RUIN[3]
            } else {
                RUIN[1]
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// A lever in a wall slot: up when off, thrown down when on; the knob shows its channel.
fn paint_lever(canvas: &mut TileCanvas, channel: u8, on: bool) {
    for py in 3..=12 {
        for px in 5..=10 {
            let edge = px == 5 || px == 10 || py == 3 || py == 12;
            canvas.set(px, py, if edge { index_of(INK) } else { index_of(NIGHT) });
        }
    }
    let (dx, dy): (i32, i32) = if on { (3, 5) } else { (3, -5) };
    for step in 0..=5 {
        let px = 7 + dx * step / 5;
        let py = 8 + dy * step / 5;
        canvas.set(px, py.clamp(0, TILE_ART - 1), BRONZE[3]);
        canvas.set(px + 1, py.clamp(0, TILE_ART - 1), BRONZE[1]);
    }
    let (kx, ky) = (7 + dx, 8 + dy);
    for (ox, oy) in [(0, 0), (1, 0), (0, 1), (1, 1), (-1, 0), (0, -1)] {
        canvas.set(
            (kx + ox).clamp(0, TILE_ART - 1),
            (ky + oy).clamp(0, TILE_ART - 1),
            index_of(channel_color(channel)),
        );
    }
    canvas.set(kx, ky.clamp(0, TILE_ART - 1), index_of(CREAM));
    canvas.set(7, 8, index_of(INK));
    canvas.set(8, 8, BRONZE[0]);
}

/// A pressure plate: a bronze slab set in the floor, raised until weight presses it flush.
fn paint_plate(canvas: &mut TileCanvas, channel: u8, down: bool) {
    let top = if down { 2 } else { 0 };
    for py in top..=top + 3 {
        for px in 1..TILE_ART - 1 {
            let pixel = if py == top {
                if px == 1 || px == TILE_ART - 2 {
                    index_of(INK)
                } else {
                    BRONZE[3]
                }
            } else if py == top + 3 {
                index_of(INK)
            } else if (6..=9).contains(&px) {
                index_of(channel_color(channel))
            } else {
                BRONZE[2]
            };
            canvas.set(px, py, pixel);
        }
    }
    if down {
        // A lit groove around the pressed plate.
        for px in 1..TILE_ART - 1 {
            canvas.set(px, 1, index_of(channel_color(channel)));
        }
    }
}

/// Gate bars: three iron bars with a cross brace, the dark gateway behind.
fn paint_gate(canvas: &mut TileCanvas, strata: &Strata, edges: Edges) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let bar = matches!(px, 2 | 3 | 7 | 8 | 12 | 13);
            let brace = (7..=8).contains(&py) || (1..=2).contains(&py);
            let pixel = if bar {
                if matches!(px, 2 | 7 | 12) {
                    STEEL_RAMP[3]
                } else {
                    STEEL_RAMP[1]
                }
            } else if brace {
                if py == 1 || py == 7 {
                    STEEL_RAMP[2]
                } else {
                    index_of(INK)
                }
            } else if edges.up == Cell::Sky {
                TRANSPARENT
            } else if (px + py) % 3 == 0 {
                index_of(NIGHT)
            } else {
                backdrop_pixel(strata, canvas.origin_x + px, canvas.origin_y + py)
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// A keycard door: a steel slab with a reader whose light shows the card it wants.
fn paint_keycard_door(canvas: &mut TileCanvas, color: u8, (_, tile_y): (i32, i32)) {
    let light = index_of(keycard_color(color));
    let reader = tile_y.rem_euclid(3) == 1;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let pixel = if reader && (10..=13).contains(&px) && (5..=10).contains(&py) {
                if px == 10 || px == 13 || py == 5 || py == 10 {
                    index_of(INK)
                } else if py == 6 {
                    light
                } else {
                    index_of(NIGHT)
                }
            } else if (px <= 1 || px >= TILE_ART - 2) && py % 4 == 1 {
                light
            } else if px == 7 || px == 8 {
                STEEL_RAMP[0]
            } else {
                plating(canvas.origin_x + px, canvas.origin_y + py)
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// A treasure chest standing on the floor: banded timber and a gold lock (a keycard chest
/// shows its card's colour on the lock); open, the lid stands up and the inside is dark.
fn paint_chest(
    canvas: &mut TileCanvas,
    strata: &Strata,
    edges: Edges,
    open: bool,
    card: Option<u8>,
) {
    let last = TILE_ART - 1;
    let lock = index_of(card.map_or(GOLD, keycard_color));
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let behind = if edges.up == Cell::Sky {
                TRANSPARENT
            } else {
                backdrop_pixel(strata, canvas.origin_x + px, canvas.origin_y + py)
            };
            let top = if open { 6 } else { 5 };
            let pixel = if px < 1 || px > last - 1 {
                behind
            } else if open && py < top {
                // The lid, tipped up behind the box.
                if py < 2 || px < 2 || px > last - 2 {
                    behind
                } else if py == 2 || px == 2 || px == last - 2 {
                    index_of(INK)
                } else {
                    TIMBER[1]
                }
            } else if py < top {
                behind
            } else if py == top || px == 1 || px == last - 1 || py == last {
                index_of(INK)
            } else if open && py <= top + 2 {
                index_of(NIGHT)
            } else if (7..=8).contains(&px) && (top + 3..=top + 5).contains(&py) {
                lock
            } else if py == top + 3 || px == 4 || px == last - 4 {
                // Iron bands.
                STEEL_RAMP[2]
            } else if py == top + 1 {
                TIMBER[3]
            } else {
                TIMBER[2]
            };
            canvas.set(px, py, pixel);
        }
    }
    if !open {
        // A glint on the lock.
        canvas.set(7, 8, index_of(CREAM));
    }
}

// --- Tomb pieces (WP-TOMBS) ------------------------------------------------------------------

/// What shows behind a see-through tomb piece.
fn tomb_behind(
    canvas: &TileCanvas,
    strata: &Strata,
    edges: Edges,
    px: i32,
    py: i32,
) -> PaletteIndex {
    if edges.up == Cell::Sky || edges.left == Cell::Sky || edges.right == Cell::Sky {
        TRANSPARENT
    } else {
        backdrop_pixel(strata, canvas.origin_x + px, canvas.origin_y + py)
    }
}

/// Tomb masonry: big sandstone ashlars with carved glyphs (side-view tombs), or dark crypt
/// stone with the odd skull niche (top-down crypts).
fn paint_tomb_wall(canvas: &mut TileCanvas, style: u8, (tile_x, tile_y): (i32, i32)) {
    let colors = tomb_ramp(style);
    let glyph = hash(tile_x, tile_y, 281).is_multiple_of(if style == 1 { 11 } else { 7 });
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
            let mut pixel = if style == 1 {
                masonry(x, y, 5, 9, colors, index_of(INK))
            } else {
                masonry(x, y, 8, 12, colors, index_of(SOIL_DARK))
            };
            if glyph && (4..=11).contains(&px) && (4..=11).contains(&py) {
                let carved = if style == 1 {
                    // A skull niche: two eyes and a jaw line.
                    ((px == 6 || px == 9) && (6..=7).contains(&py))
                        || (py == 10 && (6..=9).contains(&px))
                        || ((px == 5 || px == 10) && (5..=9).contains(&py))
                } else {
                    // An eye and a staff.
                    ((5..=7).contains(&px) && (py == 5 || py == 7))
                        || (px == 5 && py == 6)
                        || (px == 9 && (5..=11).contains(&py))
                        || (py == 8 && (8..=10).contains(&px))
                };
                if carved {
                    pixel = colors[0];
                }
            }
            canvas.set(px, py, pixel);
        }
    }
}

/// A carved pillar a pilot walks past (drawn in front of the backdrop).
fn paint_tomb_pillar(canvas: &mut TileCanvas, strata: &Strata, edges: Edges, style: u8) {
    let colors = tomb_ramp(style);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let pixel = if (4..=11).contains(&px) {
                if px == 4 || px == 11 {
                    index_of(INK)
                } else if px == 5 || py % 6 == 0 {
                    colors[3]
                } else if px >= 9 {
                    colors[1]
                } else {
                    colors[2]
                }
            } else {
                tomb_behind(canvas, strata, edges, px, py)
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// A tomb's entrance door: a carved slab (side view) or a floor hatch with a ring (top down);
/// sealed, a red-and-gold seal band crosses it; open, it shows the dark way in.
fn paint_tomb_door(canvas: &mut TileCanvas, style: u8, sealed: bool, (_, tile_y): (i32, i32)) {
    let colors = tomb_ramp(style);
    let last = TILE_ART - 1;
    let hatch = style == 1;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let frame = px <= 1 || px >= last - 1 || (hatch && (py <= 1 || py >= last - 1));
            let pixel = if frame {
                if px == 0 || px == last || (hatch && (py == 0 || py == last)) {
                    index_of(INK)
                } else {
                    colors[3]
                }
            } else if !sealed {
                if hatch && (py - 2).rem_euclid(4) == 0 {
                    // Steps going down into the crypt.
                    colors[2]
                } else {
                    index_of(INK)
                }
            } else if hatch && (5..=10).contains(&px) && (5..=10).contains(&py) {
                // The hatch's iron ring.
                if px == 5 || px == 10 || py == 5 || py == 10 {
                    STEEL_RAMP[3]
                } else {
                    colors[1]
                }
            } else if !hatch
                && tile_y.rem_euclid(2) == 0
                && (6..=9).contains(&px)
                && (4..=9).contains(&py)
            {
                // The builders' sigil in gold on the upper slab.
                if px == 6 || px == 9 || py == 4 || py == 9 {
                    index_of(GOLD)
                } else {
                    index_of(AMBER)
                }
            } else if (py + px).rem_euclid(5) == 0 {
                colors[1]
            } else {
                colors[2]
            };
            canvas.set(px, py, pixel);
        }
    }
    if sealed {
        // The seal band.
        let band = if hatch { 2..=3 } else { 11..=12 };
        for py in band {
            for px in 1..last {
                canvas.set(
                    px,
                    py,
                    if px % 3 == 0 {
                        index_of(GOLD)
                    } else {
                        index_of(RED)
                    },
                );
            }
        }
    }
}

/// A shrine lamp on a carved plinth; lit, the bowl burns warm.
fn paint_tomb_shrine(canvas: &mut TileCanvas, strata: &Strata, edges: Edges, lit: bool) {
    let last = TILE_ART - 1;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let plinth = (3..=12).contains(&px) && py >= 8;
            let bowl = (2..=13).contains(&px) && (5..=7).contains(&py);
            let flame = lit && (5..=10).contains(&px) && py <= 4 && (px - 7).abs() + (4 - py) <= 4;
            let pixel = if plinth {
                if px == 3 || px == 12 || py == last {
                    index_of(INK)
                } else if py == 8 {
                    TOMB_SANDSTONE[3]
                } else if (6..=9).contains(&px) && (10..=13).contains(&py) {
                    index_of(if lit { GOLD } else { TEAL })
                } else {
                    TOMB_SANDSTONE[2]
                }
            } else if bowl {
                if py == 7 || px == 2 || px == 13 {
                    index_of(INK)
                } else {
                    index_of(COPPER)
                }
            } else if flame {
                if (px - 7).abs() + (4 - py) <= 2 {
                    index_of(CREAM)
                } else {
                    index_of(ORANGE)
                }
            } else {
                tomb_behind(canvas, strata, edges, px, py)
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// The stairs down: a gilded archway with steps sinking into the dark.
fn paint_tomb_stairs(canvas: &mut TileCanvas) {
    let last = TILE_ART - 1;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let arch = px <= 1 || px >= last - 1 || py <= 1;
            let pixel = if arch {
                if px == 0 || px == last || py == 0 {
                    index_of(INK)
                } else {
                    index_of(GOLD)
                }
            } else {
                // Steps: each lower one further in, darker.
                let step = (py - 2) / 3;
                let edge = (py - 2) % 3 == 0 && px >= 2 + step;
                if edge {
                    TOMB_SANDSTONE[3 - usize::try_from(step.min(3)).unwrap_or(0)]
                } else {
                    index_of(INK)
                }
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// The way out: a doorway with pale daylight beyond and a gold arrow pointing up.
fn paint_tomb_exit(canvas: &mut TileCanvas) {
    let last = TILE_ART - 1;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let frame = px <= 1 || px >= last - 1 || py <= 1;
            let arrow = ((px == 7 || px == 8) && (4..=12).contains(&py))
                || (py - 4 == (7 - px).abs() && (4..=7).contains(&py));
            let pixel = if frame {
                if px == 0 || px == last || py == 0 {
                    index_of(INK)
                } else {
                    TOMB_SANDSTONE[3]
                }
            } else if arrow {
                index_of(GOLD)
            } else if py > 10 {
                index_of(SKY)
            } else {
                index_of(SKY_LIGHT)
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// A cache of run loot: a gilded urn; opened, its lid is gone and it is dark inside.
fn paint_tomb_cache(canvas: &mut TileCanvas, strata: &Strata, edges: Edges, open: bool) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let width = match py {
                4..=5 => 3,
                6..=12 => 5,
                13..=15 => 4,
                _ => -1,
            };
            let left = 7 - width;
            let right = 8 + width;
            let inside = width >= 0 && (left..=right).contains(&px);
            let pixel = if inside {
                let rim = px == left || px == right || py == 15;
                if rim || (open && py <= 6) {
                    index_of(INK)
                } else if py == 9 || py == 10 {
                    index_of(if open { STONE } else { TEAL })
                } else if px <= 6 {
                    index_of(if open { STONE_LIGHT } else { CREAM })
                } else {
                    index_of(if open { STONE } else { GOLD })
                }
            } else if !open && (5..=10).contains(&px) && (2..=3).contains(&py) {
                // The lid.
                if py == 2 {
                    index_of(GOLD)
                } else {
                    index_of(AMBER)
                }
            } else {
                tomb_behind(canvas, strata, edges, px, py)
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// Floor spikes standing out of the dark; a hidden trap shows only a few faint tips (trap
/// sight draws its glow over it in the world).
fn paint_tomb_trap(canvas: &mut TileCanvas, strata: &Strata, edges: Edges, hidden: bool) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let column = px.rem_euclid(4);
            let height = if hidden { 3 } else { 9 };
            let tip = TILE_ART - height + (column - 1).abs() * 2;
            let spike = column != 3 && py >= tip;
            let pixel = if spike {
                if hidden {
                    index_of(SLATE_DARK)
                } else if column == 1 {
                    STEEL_RAMP[3]
                } else {
                    STEEL_RAMP[1]
                }
            } else if py >= TILE_ART - 2 {
                index_of(INK)
            } else {
                tomb_behind(canvas, strata, edges, px, py)
            };
            canvas.set(px, py, pixel);
        }
    }
}
