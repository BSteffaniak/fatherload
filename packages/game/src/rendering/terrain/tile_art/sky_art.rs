//! Tile art for the sky lands (see `fatherload_simulation::sky`): floating islands, sky
//! continents and the Moon.
//!
//! An island reads as a lump of earth torn out of the ground and set adrift: a thick turf top
//! in its palette (meadow grass, blossom with pink flowers, frost-dusted, golden amber grass),
//! a band of soil, and a rock underside in the palette's own colour (slate blue, lilac, pale
//! frost, warm sandstone) with cyan sky crystal pockets. Roots and crystal shards hang below,
//! painted over the open sky so they dangle. Continents are steel-blue slab rock; the Moon is
//! pale grey regolith dust over dark cratered moon rock with lavender glints.

use fatherload_simulation::sky::{SkyBiome, SkyTile};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, index_of};

use super::{
    Cell, Edges, Ramp, Strata, TILE_ART, TileCanvas, cobble_level, dirt_level, hash, noise,
    paint_edges, ramp, slab_level,
};

/// Soil under the turf.
const SOIL_RAMP: Ramp = ramp([SOIL_DARK, SOIL, LOAM, CLAY]);
/// Continent rock: steel-blue slabs.
const CONTINENT: Ramp = ramp([NIGHT, STEEL_DARK, STEEL, STEEL_LIGHT]);
/// Moon dust.
const REGOLITH: Ramp = ramp([SLATE_LIGHT, FOG, MIST, BONE]);
/// Moon rock.
const MOON_ROCK: Ramp = ramp([NIGHT, SLATE_DARK, SLATE, FOG]);
/// Sky crystal.
const CRYSTAL: Ramp = ramp([TEAL_DARK, TEAL, MINT, ICE]);

/// The rock underside of an island in `biome`'s palette.
pub const fn rock_ramp(biome: SkyBiome) -> Ramp {
    match biome {
        SkyBiome::Meadow => ramp([SLATE_DARK, SLATE, SLATE_LIGHT, FOG]),
        SkyBiome::Blossom => ramp([PLUM, VIOLET, LAVENDER, PINK]),
        SkyBiome::Frost => ramp([SLATE, FOG, MIST, ICE]),
        SkyBiome::Amber => ramp([MAROON, RUST, COPPER, SAND]),
    }
}

/// Turf colours of `biome`: (tips, blades, roots, flower).
pub const fn turf_colors(biome: SkyBiome) -> [PaletteIndex; 4] {
    let colors = match biome {
        SkyBiome::Meadow => [LIME, LEAF, MOSS, CREAM],
        SkyBiome::Blossom => [LIME, LEAF, MOSS, PINK],
        SkyBiome::Frost => [BONE, ICE, MIST, SKY_LIGHT],
        SkyBiome::Amber => [CREAM, AMBER, LOAM, GOLD],
    };
    [
        index_of(colors[0]),
        index_of(colors[1]),
        index_of(colors[2]),
        index_of(colors[3]),
    ]
}

/// Representative colour of a sky tile (drill dust, debris, the map).
pub const fn sky_color(tile: SkyTile) -> Color {
    match tile {
        SkyTile::Turf(SkyBiome::Meadow | SkyBiome::Blossom) => LEAF,
        SkyTile::Turf(SkyBiome::Frost) => ICE,
        SkyTile::Turf(SkyBiome::Amber) => AMBER,
        SkyTile::Soil | SkyTile::Roots => LOAM,
        SkyTile::Rock(SkyBiome::Meadow) => SLATE_LIGHT,
        SkyTile::Rock(SkyBiome::Blossom) => LAVENDER,
        SkyTile::Rock(SkyBiome::Amber) => CLAY,
        SkyTile::Crystal => MINT,
        SkyTile::ContinentRock => STEEL,
        SkyTile::Rock(SkyBiome::Frost) | SkyTile::Regolith => MIST,
        SkyTile::MoonRock => SLATE,
    }
}

use raylib::prelude::Color;

/// The pattern step of a body material at art pixel `(x, y)`.
fn body_pixel(tile: SkyTile, x: i32, y: i32) -> PaletteIndex {
    match tile {
        SkyTile::Turf(_) | SkyTile::Soil | SkyTile::Roots => SOIL_RAMP[dirt_level(x, y)],
        SkyTile::Rock(biome) => {
            // Layered, slightly tilted strata over cobbles: rock torn from a cliff.
            let level = cobble_level(x, y);
            let stripe = (y + (noise(x, y, 11, 61) * 4.0) as i32).rem_euclid(7) == 0;
            rock_ramp(biome)[if stripe {
                level.saturating_sub(1)
            } else {
                level
            }]
        }
        SkyTile::Crystal => {
            // Big faceted crystal faces with the odd sparkle.
            if hash(x, y, 63).is_multiple_of(53) {
                index_of(BONE)
            } else {
                CRYSTAL[slab_level(x, y)]
            }
        }
        SkyTile::ContinentRock => CONTINENT[slab_level(x, y)],
        SkyTile::Regolith => {
            let h = hash(x, y, 65);
            if h.is_multiple_of(41) {
                REGOLITH[0]
            } else if h.is_multiple_of(29) {
                REGOLITH[3]
            } else if noise(x, y, 6, 66) > 0.62 {
                REGOLITH[1]
            } else {
                REGOLITH[2]
            }
        }
        SkyTile::MoonRock => {
            if hash(x, y, 67).is_multiple_of(97) {
                index_of(LAVENDER)
            } else {
                MOON_ROCK[cobble_level(x, y)]
            }
        }
    }
}

/// Paints a sky-land tile.
pub(super) fn paint(canvas: &mut TileCanvas, tile: SkyTile, strata: &Strata, edges: Edges) {
    let seed = hash(canvas.origin_x, canvas.origin_y, 431);
    // Hanging pieces (roots, a crystal shard under the rock) dangle over open sky.
    let dangling = edges.left.is_open() && edges.right.is_open();
    let hanging = dangling && edges.down.is_open();
    match tile {
        SkyTile::Roots => {
            paint_roots(canvas, seed, edges.up == Cell::Solid);
            return;
        }
        SkyTile::Crystal if hanging => {
            paint_shard(canvas, seed);
            return;
        }
        SkyTile::Crystal if dangling => {
            paint_shaft(canvas);
            return;
        }
        _ => {}
    }
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, body_pixel(tile, gx, gy));
        }
    }
    let (lip, shade) = match tile {
        SkyTile::Turf(biome) => (turf_colors(biome)[0], SOIL_RAMP[0]),
        SkyTile::Soil | SkyTile::Roots => (SOIL_RAMP[3], SOIL_RAMP[0]),
        SkyTile::Rock(biome) => (rock_ramp(biome)[3], rock_ramp(biome)[0]),
        SkyTile::Crystal => (CRYSTAL[3], CRYSTAL[0]),
        SkyTile::ContinentRock => (CONTINENT[3], CONTINENT[0]),
        SkyTile::Regolith => (REGOLITH[3], REGOLITH[0]),
        SkyTile::MoonRock => (MOON_ROCK[3], MOON_ROCK[0]),
    };
    if let SkyTile::Turf(biome) = tile {
        paint_turf(canvas, biome, edges.up.is_open());
    }
    if tile == SkyTile::Regolith && edges.up.is_open() {
        // A bright dusty rim catching the sun.
        for px in 0..TILE_ART {
            canvas.set(px, 0, REGOLITH[3]);
            if hash(canvas.origin_x + px, 1, 68).is_multiple_of(3) {
                canvas.set(px, 1, REGOLITH[3]);
            }
        }
    }
    paint_edges(canvas, strata, edges, lip, shade, false);
}

/// A turf top: thick grass (snow, golden grass) over the soil, with flowers on blossom isles.
fn paint_turf(canvas: &mut TileCanvas, biome: SkyBiome, open_above: bool) {
    let [tips, blades, roots, flower] = turf_colors(biome);
    let depth = if open_above { 11 } else { 6 };
    for px in 0..TILE_ART {
        let gx = canvas.origin_x + px;
        let h = hash(gx, canvas.origin_y, 433);
        let reach = depth - (h % 3) as i32;
        for py in 0..reach.max(1) {
            let color = if py == 0 && open_above {
                tips
            } else if py + 1 >= reach {
                roots
            } else {
                blades
            };
            canvas.set(px, py, color);
        }
        if open_above && biome == SkyBiome::Blossom && h.is_multiple_of(7) {
            canvas.set(px, 1, flower);
        }
        if open_above && biome == SkyBiome::Meadow && h.is_multiple_of(19) {
            canvas.set(px, 1, flower);
        }
    }
}

/// Roots hanging under an island: a few twisting strands, thickest at the top.
fn paint_roots(canvas: &mut TileCanvas, seed: u32, from_above: bool) {
    let (dark, light) = (index_of(SOIL), index_of(LOAM));
    let strands = 2 + seed % 3;
    for strand in 0..strands {
        let h = hash(seed as i32, strand as i32, 437);
        let mut x = 2 + (h % 12) as i32;
        let length = if from_above {
            TILE_ART
        } else {
            8 + (h >> 4) as i32 % 8
        };
        for py in 0..length {
            canvas.set(x, py, dark);
            if py < 5 {
                canvas.set(x + 1, py, light);
            }
            if (h >> (py % 24)).trailing_zeros() >= 2 {
                x += if (h >> 3) & 1 == 0 { 1 } else { -1 };
                x = x.clamp(1, TILE_ART - 2);
            }
        }
        // A rootlet tip.
        canvas.set(x, length.min(TILE_ART) - 1, index_of(CLAY));
    }
}

/// The upper part of a hanging crystal: a faceted column (its tip is [`paint_shard`]).
fn paint_shaft(canvas: &mut TileCanvas) {
    let ink = index_of(INK);
    let center = 8;
    for py in 0..TILE_ART {
        for px in center - 6..=center + 6 {
            let edge = px == center - 6 || px == center + 6;
            let color = if edge {
                ink
            } else if px < center - 1 {
                CRYSTAL[3]
            } else if px <= center + 1 {
                CRYSTAL[2]
            } else {
                CRYSTAL[1]
            };
            canvas.set(px, py, color);
        }
    }
}

/// A sky-crystal shard hanging point-down.
fn paint_shard(canvas: &mut TileCanvas, seed: u32) {
    let center = 8 + (seed % 2) as i32 - 1;
    let ink = index_of(INK);
    for py in 0..TILE_ART {
        let half = ((TILE_ART - py) * 5 / 16).max(0);
        for px in center - half - 1..=center + half + 1 {
            let edge = px == center - half - 1 || px == center + half + 1;
            let color = if edge {
                ink
            } else if px < center {
                CRYSTAL[3]
            } else if px == center {
                CRYSTAL[2]
            } else {
                CRYSTAL[1]
            };
            canvas.set(px, py, color);
        }
    }
}
