//! Side-view tomb tile sets (WP-TOMBS T28): masonry, pillars, back walls and trap housings in
//! each culture's look ([`TombCulture`]).
//!
//! * **Sandstone** (mesa tombs): sun-baked ashlars with carved eyes and staffs, glyph bands on
//!   the back wall, bronze sconces.
//! * **Catacomb** (deep strata, old quarters): small grey bone-and-mortar bricks, skull niches
//!   and stacked bones.
//! * **Sky temple** (sky continents): pale marble blocks with gold inlay bands and sun glyphs.
//! * **Lunar vault** (the Moon): bevelled alloy panels, rivets and teal light seams.
//!
//! A piece's masonry set is in its tile ([`BuiltTile::TombWall`]) inside the run vault; a trap
//! or the back wall borrows the set of the masonry around it, and an entrance on the map (always
//! set 0, so the V10 generator stays pinned) takes the culture of the site it belongs to.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "small art coordinates"
)]

use fatherload_simulation::{
    base::BuiltTile,
    terrain::{Terrain, TileKind, TilePosition},
    tomb::{culture::TombCulture, realize::TrapKind, side_view::dart_facing},
    worldgen::{features::TileRect, tomb_sites},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, index_of};

use super::{
    super::{Cell, Edges, Ramp, Strata, TILE_ART, TileCanvas, hash, paint_edges, ramp},
    STEEL_RAMP, TOMB_SANDSTONE, masonry, tomb_behind,
};

/// Catacomb masonry: grey stone and old bone.
const CATACOMB: Ramp = ramp([NIGHT, SLATE_DARK, STONE_DARK, STONE]);
/// Sky temple marble.
const MARBLE: Ramp = ramp([SLATE, FOG, MIST, BONE]);
/// Lunar vault alloy.
const LUNAR: Ramp = ramp([INK, STEEL_DARK, SLATE, STEEL_LIGHT]);

/// The masonry ramp of a side-view culture.
#[must_use]
pub const fn culture_ramp(culture: TombCulture) -> Ramp {
    match culture {
        TombCulture::Sandstone => TOMB_SANDSTONE,
        TombCulture::Catacomb => CATACOMB,
        TombCulture::SkyTemple => MARBLE,
        TombCulture::LunarVault => LUNAR,
    }
}

/// The masonry colours of a side-view culture, darkest to brightest (world art: blades,
/// crushers).
#[must_use]
pub const fn culture_colors(culture: TombCulture) -> [Color3; 4] {
    match culture {
        TombCulture::Sandstone => [SOIL, LOAM, CLAY, SAND],
        TombCulture::Catacomb => [NIGHT, SLATE_DARK, STONE_DARK, STONE],
        TombCulture::SkyTemple => [SLATE, FOG, MIST, BONE],
        TombCulture::LunarVault => [INK, STEEL_DARK, SLATE, STEEL_LIGHT],
    }
}

/// The back wall's two shades and its highlight in a culture.
const fn back_shades(culture: TombCulture) -> [Color3; 3] {
    match culture {
        TombCulture::Sandstone => [SOIL_DARK, SOIL, LOAM],
        TombCulture::Catacomb => [NIGHT, SLATE_DARK, STONE_DARK],
        TombCulture::SkyTemple => [SLATE_DARK, SLATE, FOG],
        TombCulture::LunarVault => [INK, NIGHT, STEEL_DARK],
    }
}

type Color3 = raylib::prelude::Color;

fn masonry_of(terrain: &Terrain, tile: TilePosition) -> Option<u8> {
    match terrain.tile(tile).map(|found| found.kind) {
        Some(TileKind::Built(BuiltTile::TombWall(set) | BuiltTile::TombPillar(set))) => Some(set),
        _ => None,
    }
}

/// The masonry set of the tomb around `position`: the nearest masonry beside it, then up
/// its column (a room's ceiling).
#[must_use]
pub fn masonry_near(terrain: &Terrain, position: TilePosition) -> Option<u8> {
    for (dx, dy) in [(0, 1), (0, -1), (-1, 0), (1, 0), (-1, 1), (1, 1)] {
        if let Some(set) = masonry_of(
            terrain,
            TilePosition {
                x: position.x + dx,
                y: position.y + dy,
            },
        ) {
            return Some(set);
        }
    }
    (1..16).find_map(|dy| {
        masonry_of(
            terrain,
            TilePosition {
                x: position.x,
                y: position.y - dy,
            },
        )
    })
}

/// The culture an entrance piece at `position` on the map shows: its site's.
fn entrance_culture(terrain: &Terrain, position: TilePosition) -> TombCulture {
    let rect = TileRect {
        min: position,
        max: position,
    };
    tomb_sites::sites_touching(terrain.settings(), &rect)
        .into_iter()
        .find(|site| tomb_sites::entrance_bounds(site).contains(position))
        .map_or(TombCulture::Sandstone, |site| {
            TombCulture::of_setting(site.setting)
        })
}

/// The side-view culture a piece of masonry set `set` at `position` is painted in (`None`:
/// a top-down crypt's stone, T29's).
fn culture_at(terrain: &Terrain, position: TilePosition, set: u8) -> Option<TombCulture> {
    let culture = TombCulture::of_masonry(set)?;
    if set == 0 && !tomb_sites::in_vault(terrain.settings(), position) {
        return Some(entrance_culture(terrain, position));
    }
    Some(culture)
}

/// Paints a side-view tomb piece in its culture. Returns `false` for pieces the plain built
/// art paints (crypt stone and hatches, sandstone doors, shrines, stairs, caches).
pub(in crate::rendering::terrain) fn paint_piece(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: BuiltTile,
    strata: &Strata,
    edges: Edges,
) -> bool {
    match tile {
        BuiltTile::TombWall(set) => {
            let Some(culture) = culture_at(terrain, position, set) else {
                return false;
            };
            paint_wall(canvas, culture, position);
            let colors = culture_ramp(culture);
            paint_edges(canvas, strata, edges, colors[3], colors[0], true);
            true
        }
        BuiltTile::TombPillar(set) => {
            let Some(culture) = culture_at(terrain, position, set) else {
                return false;
            };
            paint_pillar(canvas, strata, edges, culture, position);
            true
        }
        // A side-view entrance's slab door in the site's culture's stone (sandstone keeps the
        // plain art).
        BuiltTile::TombDoor { style: 0, sealed } => {
            let culture = entrance_culture(terrain, position);
            if culture == TombCulture::Sandstone {
                return false;
            }
            let colors = culture_ramp(culture);
            super::paint_tomb_door_in(canvas, colors, false, sealed, (position.x, position.y));
            paint_edges(canvas, strata, edges, colors[3], colors[0], true);
            true
        }
        BuiltTile::TombTrap { kind, hidden } => {
            let culture = masonry_near(terrain, position)
                .and_then(TombCulture::of_masonry)
                .unwrap_or_default();
            paint_trap(
                canvas,
                strata,
                edges,
                culture,
                position,
                TrapKind::from_index(kind),
                hidden,
            );
            true
        }
        _ => false,
    }
}

/// One masonry pixel of `culture` at art pixel `(x, y)`.
const fn wall_pixel(culture: TombCulture, x: i32, y: i32) -> PaletteIndex {
    let colors = culture_ramp(culture);
    match culture {
        TombCulture::Sandstone => masonry(x, y, 8, 12, colors, index_of(SOIL_DARK)),
        TombCulture::Catacomb => {
            // Small rough bricks with the odd bone end showing (old, dull bone: bright flecks
            // read as sparks at play scale).
            if hash(x.div_euclid(3), y.div_euclid(2), 401).is_multiple_of(613) {
                index_of(STONE_LIGHT)
            } else {
                masonry(x, y, 4, 7, colors, index_of(INK))
            }
        }
        TombCulture::SkyTemple => {
            // Gold inlay every other course.
            if y.rem_euclid(32) == 13 {
                index_of(GOLD)
            } else if y.rem_euclid(32) == 14 {
                index_of(AMBER)
            } else {
                masonry(x, y, 8, 16, colors, index_of(SLATE))
            }
        }
        TombCulture::LunarVault => {
            let (px, py) = (x.rem_euclid(16), y.rem_euclid(16));
            if px == 15 || py == 15 {
                colors[0]
            } else if px == 0 || py == 0 || ((px == 2 || px == 13) && (py == 2 || py == 13)) {
                // The lit bevel and the rivets.
                colors[3]
            } else if px == 14 || py == 14 || (x + 2 * y).rem_euclid(11) == 0 {
                colors[1]
            } else {
                colors[2]
            }
        }
    }
}

/// Masonry with the culture's carvings.
fn paint_wall(canvas: &mut TileCanvas, culture: TombCulture, position: TilePosition) {
    let colors = culture_ramp(culture);
    let roll = hash(position.x, position.y, 281);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
            let mut pixel = wall_pixel(culture, x, y);
            let inner = (4..=11).contains(&px) && (4..=11).contains(&py);
            match culture {
                TombCulture::Sandstone if inner && roll.is_multiple_of(7) => {
                    // An eye and a staff.
                    if ((5..=7).contains(&px) && (py == 5 || py == 7))
                        || (px == 5 && py == 6)
                        || (px == 9 && (5..=11).contains(&py))
                        || (py == 8 && (8..=10).contains(&px))
                    {
                        pixel = colors[0];
                    }
                }
                TombCulture::Catacomb if inner && roll.is_multiple_of(17) => {
                    // A skull niche.
                    let recess = (5..=10).contains(&px) && (5..=11).contains(&py);
                    let skull = (6..=9).contains(&px) && (6..=10).contains(&py);
                    let eye = (px == 7 || px == 8) && py == 8 && px != 0;
                    if eye || (skull && py == 10 && px % 2 == 0) {
                        pixel = index_of(INK);
                    } else if skull {
                        pixel = index_of(STONE_LIGHT);
                    } else if recess {
                        pixel = index_of(INK);
                    }
                }
                TombCulture::SkyTemple if inner && roll.is_multiple_of(8) => {
                    // A gold sun glyph.
                    let (dx, dy) = (px - 8, py - 8);
                    let ring = dx * dx + dy * dy;
                    if ring <= 4 {
                        pixel = index_of(GOLD);
                    } else if (dx == 0 || dy == 0 || dx == dy || dx == -dy) && ring <= 13 {
                        pixel = index_of(AMBER);
                    }
                }
                // A light seam across the panel.
                TombCulture::LunarVault
                    if roll.is_multiple_of(4) && (py == 7 || py == 8) && (2..=13).contains(&px) =>
                {
                    pixel = index_of(if py == 7 { MINT } else { TEAL });
                }
                _ => {}
            }
            canvas.set(px, py, pixel);
        }
    }
}

/// A carved pillar a pilot walks past, in the culture's stone.
fn paint_pillar(
    canvas: &mut TileCanvas,
    strata: &Strata,
    edges: Edges,
    culture: TombCulture,
    position: TilePosition,
) {
    let colors = culture_ramp(culture);
    let band = match culture {
        TombCulture::Sandstone => index_of(AMBER),
        TombCulture::Catacomb => index_of(BONE),
        TombCulture::SkyTemple => index_of(GOLD),
        TombCulture::LunarVault => index_of(MINT),
    };
    let ring_row = position.y.rem_euclid(3) == 0;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let pixel = if (4..=11).contains(&px) {
                if px == 4 || px == 11 {
                    index_of(INK)
                } else if ring_row && (py == 1 || py == 2) {
                    band
                } else if culture == TombCulture::SkyTemple && (px == 6 || px == 9) {
                    // Fluting.
                    colors[1]
                } else if px == 5 {
                    colors[3]
                } else if px >= 9 {
                    colors[1]
                } else {
                    colors[2]
                }
            } else {
                backdrop_or_behind(canvas, strata, edges, culture, px, py)
            };
            canvas.set(px, py, pixel);
        }
    }
}

/// What shows around a see-through piece inside a run: the culture's back wall (outside, the
/// plain backdrop).
fn backdrop_or_behind(
    canvas: &TileCanvas,
    strata: &Strata,
    edges: Edges,
    culture: TombCulture,
    px: i32,
    py: i32,
) -> PaletteIndex {
    if edges.up == Cell::Sky || edges.left == Cell::Sky || edges.right == Cell::Sky {
        return tomb_behind(canvas, strata, edges, px, py);
    }
    back_pixel(culture, canvas.origin_x + px, canvas.origin_y + py)
}

/// The back wall's base pattern: big dim blocks in the culture's dark shades.
const fn back_pixel(culture: TombCulture, x: i32, y: i32) -> PaletteIndex {
    let [dark, body, light] = back_shades(culture);
    let (course, length) = match culture {
        TombCulture::Sandstone => (12, 20),
        TombCulture::Catacomb => (6, 10),
        TombCulture::SkyTemple => (16, 24),
        TombCulture::LunarVault => (16, 16),
    };
    let row = y.div_euclid(course);
    let offset = if row % 2 == 0 { 0 } else { length / 2 };
    let column = (x + offset).rem_euclid(length);
    let row_y = y.rem_euclid(course);
    if row_y == course - 1 || column == length - 1 {
        index_of(dark)
    } else if row_y == 0 && hash((x + offset).div_euclid(length), row, 409).is_multiple_of(2) {
        index_of(light)
    } else if hash(x, y, 419).is_multiple_of(37) {
        index_of(dark)
    } else {
        index_of(body)
    }
}

/// The back wall of an open tile inside a run (the room's culture: the masonry above it).
/// Returns `false` outside the run vault.
pub(in crate::rendering::terrain) fn paint_backdrop(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    edges: Edges,
) -> bool {
    if !tomb_sites::in_vault(terrain.settings(), position) {
        return false;
    }
    let Some(culture) = masonry_near(terrain, position).and_then(TombCulture::of_masonry) else {
        return false;
    };
    let [dark, _, light] = back_shades(culture);
    // Murals: a band of the culture's motif along every third row.
    let mural = position.y.rem_euclid(3) == 0
        && !hash(position.x.div_euclid(3), position.y, 431).is_multiple_of(3);
    let sconce = position.x.rem_euclid(7) == 3 && position.y.rem_euclid(4) == 1;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
            let mut pixel = back_pixel(culture, x, y);
            if mural && (5..=10).contains(&py) {
                let motif = match culture {
                    // Walking figures in profile.
                    TombCulture::Sandstone => {
                        let step = x.rem_euclid(8);
                        (step == 2 && py >= 6)
                            || ((1..=3).contains(&step) && py == 6)
                            || (step == 2 && py == 5)
                    }
                    // Skull niches.
                    TombCulture::Catacomb => {
                        let step = x.rem_euclid(8);
                        (2..=5).contains(&step) && (6..=9).contains(&py)
                    }
                    // A gold meander.
                    TombCulture::SkyTemple => {
                        let step = x.rem_euclid(6);
                        py == 6
                            || py == 9
                            || (step == 0 && (6..=9).contains(&py))
                            || (step == 3 && (6..=8).contains(&py))
                    }
                    // Glowing panel lines.
                    TombCulture::LunarVault => py == 7,
                };
                if motif {
                    pixel = match culture {
                        TombCulture::Sandstone => index_of(LOAM),
                        TombCulture::Catacomb => {
                            let step = x.rem_euclid(8);
                            if (step == 3 || step == 4) && py == 7 {
                                index_of(INK)
                            } else {
                                index_of(STONE_DARK)
                            }
                        }
                        TombCulture::SkyTemple => index_of(AMBER),
                        TombCulture::LunarVault => index_of(TEAL_DARK),
                    };
                }
            }
            if sconce {
                // A wall sconce with a small flame (the run's torchlight).
                let bracket = (6..=9).contains(&px) && (9..=10).contains(&py);
                let flame = (7..=8).contains(&px) && (5..=8).contains(&py);
                if bracket {
                    pixel = index_of(if culture == TombCulture::LunarVault {
                        STEEL
                    } else {
                        COPPER
                    });
                } else if flame {
                    pixel = index_of(match (culture, py) {
                        (TombCulture::LunarVault, _) => MINT,
                        (_, 5 | 6) => CREAM,
                        _ => ORANGE,
                    });
                }
            }
            // Soft contact shadows under the ceiling and beside the walls.
            if (edges.up == Cell::Solid && py <= 2)
                || (edges.left == Cell::Solid && px <= 1)
                || (edges.right == Cell::Solid && px >= TILE_ART - 2)
            {
                pixel = index_of(dark);
            }
            let _ = light;
            canvas.set(px, py, pixel);
        }
    }
    true
}

/// A trap's tile: the housing set in the culture's masonry (the moving parts are drawn in the
/// world), the spikes, the crumbling slab, the false wall.
#[allow(clippy::too_many_lines, reason = "one housing per trap kind")]
fn paint_trap(
    canvas: &mut TileCanvas,
    strata: &Strata,
    edges: Edges,
    culture: TombCulture,
    position: TilePosition,
    kind: TrapKind,
    hidden: bool,
) {
    let colors = culture_ramp(culture);
    let metal = if culture == TombCulture::LunarVault {
        STEEL_RAMP
    } else {
        ramp([SOIL_DARK, RUST, COPPER, AMBER])
    };
    match kind {
        TrapKind::Spikes => {
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
                        backdrop_or_behind(canvas, strata, edges, culture, px, py)
                    };
                    canvas.set(px, py, pixel);
                }
            }
        }
        TrapKind::FalseWall => {
            // Masonry like any other; a faint hairline crack for sharp eyes.
            paint_wall(canvas, culture, position);
            for step in 0..6 {
                canvas.set(5 + step, 3 + step + step / 2, colors[1]);
            }
            paint_edges(canvas, strata, edges, colors[3], colors[0], true);
        }
        TrapKind::Crumble => {
            paint_wall(canvas, culture, position);
            if !hidden {
                // Cracks across the slab and a broken edge.
                for step in 0..TILE_ART {
                    let y = 4 + (step * 3 / 5) + i32::from(step % 4 == 0);
                    canvas.set(step, y, index_of(INK));
                    if step % 5 == 2 {
                        canvas.set(step, y + 1, index_of(INK));
                    }
                }
                for py in 8..TILE_ART {
                    canvas.set(4 + (py - 8) / 2, py, index_of(INK));
                    canvas.set(11 - (py - 8) / 3, py, colors[0]);
                }
            }
            paint_edges(canvas, strata, edges, colors[3], colors[0], true);
        }
        TrapKind::Blade => {
            // A bronze hub in the ceiling with the chain's first link.
            paint_wall(canvas, culture, position);
            for py in 0..TILE_ART {
                for px in 0..TILE_ART {
                    let (dx, dy) = (px - 8, py - 11);
                    if dx * dx + dy * dy <= 12 {
                        canvas.set(
                            px,
                            py,
                            if dx * dx + dy * dy <= 3 {
                                metal[3]
                            } else {
                                metal[1]
                            },
                        );
                    } else if (7..=8).contains(&px) && py >= 13 {
                        canvas.set(px, py, metal[2]);
                    }
                }
            }
            paint_edges(canvas, strata, edges, colors[3], colors[0], true);
        }
        TrapKind::Crusher => {
            // The housing's slot and the block's iron teeth waiting in it.
            paint_wall(canvas, culture, position);
            for py in 6..TILE_ART {
                for px in 1..TILE_ART - 1 {
                    let pixel = if px == 1 || px == TILE_ART - 2 {
                        index_of(INK)
                    } else if py >= 13 && (px % 3 == 1) {
                        STEEL_RAMP[3]
                    } else if py >= 12 {
                        STEEL_RAMP[1]
                    } else {
                        index_of(INK)
                    };
                    canvas.set(px, py, pixel);
                }
            }
            paint_edges(canvas, strata, edges, colors[3], colors[0], true);
        }
        TrapKind::DartWall => {
            // A carved face whose mouth is the dart's hole, on the side it fires.
            paint_wall(canvas, culture, position);
            let facing = dart_facing(hidden);
            for py in 4..=12_i32 {
                for px in 3..=12_i32 {
                    let face = (px - 8).abs() <= 4 && (py - 8).abs() <= 4;
                    if face {
                        canvas.set(px, py, colors[1]);
                    }
                }
            }
            for px in [5, 10] {
                canvas.set(px, 6, index_of(INK));
            }
            let mouth = if facing < 0 { 0..=6 } else { 9..=15 };
            for px in mouth {
                canvas.set(px, 9, index_of(INK));
                canvas.set(px, 10, index_of(INK));
            }
            paint_edges(canvas, strata, edges, colors[3], colors[0], true);
        }
        TrapKind::SandSluice => {
            // A bronze grate in the floor, dark sand behind its bars.
            for py in 0..TILE_ART {
                for px in 0..TILE_ART {
                    let pixel = if py <= 1 || py >= TILE_ART - 2 || px == 0 || px == TILE_ART - 1 {
                        metal[2]
                    } else if px % 3 == 0 {
                        metal[1]
                    } else if culture.floods_with_water() {
                        index_of(DEEP_BLUE)
                    } else {
                        index_of(SOIL)
                    };
                    canvas.set(px, py, pixel);
                }
            }
            paint_edges(canvas, strata, edges, metal[3], metal[0], true);
        }
        TrapKind::FogVent => {
            // A cracked vent grille breathing a sickly glow.
            paint_wall(canvas, culture, position);
            for py in 0..5 {
                for px in 3..=12 {
                    let pixel = if py == 0 || px == 3 || px == 12 {
                        index_of(INK)
                    } else if px % 2 == 0 {
                        index_of(MOSS)
                    } else {
                        index_of(LIME)
                    };
                    canvas.set(px, py, pixel);
                }
            }
            paint_edges(canvas, strata, edges, colors[3], colors[0], true);
        }
    }
}
