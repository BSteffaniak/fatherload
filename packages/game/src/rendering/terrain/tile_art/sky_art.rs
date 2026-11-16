//! Tile art for the sky lands (see `fatherload_simulation::sky`): floating islands, sky
//! continents and the Moon.
//!
//! An island reads as a lump of earth torn out of the ground and set adrift: a thick turf top
//! in its palette (meadow grass, blossom with pink flowers, frost-dusted, golden amber grass),
//! a band of soil, and a rock underside in the palette's own colour (slate blue, lilac, pale
//! frost, warm sandstone) with cyan sky crystal pockets. Roots and crystal shards hang below,
//! painted over the open sky so they dangle. Continents are steel-blue slab rock; the Moon is
//! pale grey regolith dust over dark cratered moon rock with lavender glints.
//!
//! The island content of V9 skies (WP-SKY): brass-bound sky caches, twig nests on the turf,
//! the aeronauts' white ruin stone and broken pillars, mossy springs (the renderer pours a
//! waterfall from them), leafy vines, glowing violet keystones of crumbling isles, updraft vent
//! grates and the hazard-striped pads of fuel buoys.

use fatherload_simulation::sky::{SkyBiome, SkyTile};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, TRANSPARENT, index_of};

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
/// The aeronauts' carved ruin stone.
const RUIN: Ramp = ramp([SLATE, FOG, MIST, BONE]);
/// Buoy pad steel.
const PAD: Ramp = ramp([NIGHT, STEEL_DARK, STEEL, STEEL_LIGHT]);
/// The aether core: near-black violet rock (V9 continents).
const CORE: Ramp = ramp([INK, NIGHT, PLUM, VIOLET]);
/// Aether crystal (and the glow of crumbling isles' keystones): violet to rose-white facets.
const AETHER: Ramp = ramp([PLUM, VIOLET, LAVENDER, PINK]);
/// Storm glass: deep blue glass.
const STORM: Ramp = ramp([NIGHT, DEEP_BLUE, OCEAN, SKY_LIGHT]);
/// Temple stone: weathered ivory blocks.
const TEMPLE: Ramp = ramp([STONE, STONE_LIGHT, MIST, BONE]);
/// Temple bronze (gates).
const BRONZE: Ramp = ramp([RUST, COPPER, AMBER, GOLD]);
/// Woven twigs.
const TWIGS: Ramp = ramp([SOIL_DARK, SOIL, LOAM, SAND]);

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
#[allow(clippy::match_same_arms, reason = "one arm per tile family")]
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
        SkyTile::Cache => GOLD,
        SkyTile::Nest => SAND,
        SkyTile::RuinBrick | SkyTile::RuinPillar => BONE,
        SkyTile::Spring => SKY_LIGHT,
        SkyTile::Vines => MOSS,
        SkyTile::Keystone => VIOLET,
        SkyTile::Vent => TEAL,
        SkyTile::BuoyPad => ORANGE,
        SkyTile::AetherCore => PLUM,
        SkyTile::AetherCrystal => LAVENDER,
        SkyTile::StormGlass => OCEAN,
        SkyTile::TempleBrick | SkyTile::TempleLever => STONE_LIGHT,
        SkyTile::TempleDoor | SkyTile::KeystoneDoor => COPPER,
        SkyTile::TempleKeystone => MINT,
        SkyTile::TempleCache => GOLD,
        SkyTile::HarpyNest => LOAM,
        SkyTile::HarpyEgg => CREAM,
    }
}

/// The glow of a glowing sky tile (the light map, halos), if it glows.
#[must_use]
pub const fn sky_glow(tile: SkyTile) -> Option<Color> {
    match tile {
        SkyTile::AetherCore => Some(VIOLET),
        SkyTile::AetherCrystal => Some(LAVENDER),
        SkyTile::StormGlass => Some(SKY_LIGHT),
        SkyTile::TempleKeystone | SkyTile::KeystoneDoor => Some(MINT),
        SkyTile::TempleCache => Some(GOLD),
        _ => None,
    }
}

use raylib::prelude::Color;

/// The pattern step of a body material at art pixel `(x, y)`.
fn body_pixel(tile: SkyTile, x: i32, y: i32) -> PaletteIndex {
    match tile {
        SkyTile::Turf(_)
        | SkyTile::Soil
        | SkyTile::Roots
        | SkyTile::Vines
        | SkyTile::Nest
        | SkyTile::Spring => SOIL_RAMP[dirt_level(x, y)],
        SkyTile::RuinBrick | SkyTile::RuinPillar => RUIN[brick(x, y)],
        SkyTile::Cache | SkyTile::Keystone | SkyTile::Vent => {
            rock_ramp(SkyBiome::Meadow)[cobble_level(x, y)]
        }
        SkyTile::BuoyPad => PAD[slab_level(x, y)],
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
        SkyTile::AetherCore => {
            // Dark core rock threaded with glowing aether veins.
            let vein = (noise(x, y, 7, 71) - 0.5).abs() < 0.035;
            if hash(x, y, 72).is_multiple_of(61) {
                index_of(BONE)
            } else if vein {
                AETHER[2 + usize::from(hash(x, y, 73).is_multiple_of(3))]
            } else {
                CORE[slab_level(x, y)]
            }
        }
        SkyTile::AetherCrystal => {
            if hash(x, y, 74).is_multiple_of(37) {
                index_of(BONE)
            } else {
                AETHER[slab_level(x, y)]
            }
        }
        SkyTile::StormGlass => {
            // Dark glass with a gold lightning seam zig-zagging through it.
            let seam = (x + ((y / 3) % 2) * 2 - y / 2).rem_euclid(13) == 0;
            if seam {
                index_of(if hash(x, y, 75).is_multiple_of(3) {
                    CREAM
                } else {
                    GOLD
                })
            } else if hash(x, y, 76).is_multiple_of(47) {
                index_of(ICE)
            } else {
                STORM[slab_level(x, y)]
            }
        }
        SkyTile::TempleBrick
        | SkyTile::TempleDoor
        | SkyTile::KeystoneDoor
        | SkyTile::TempleLever
        | SkyTile::TempleKeystone
        | SkyTile::TempleCache
        | SkyTile::HarpyNest
        | SkyTile::HarpyEgg => temple_pixel(x, y),
    }
}

/// Temple stone: courses of big ivory blocks with dark mortar.
fn temple_pixel(x: i32, y: i32) -> PaletteIndex {
    let course = y.div_euclid(8);
    let offset = if course % 2 == 0 { 0 } else { 6 };
    let mortar = y.rem_euclid(8) == 7 || (x + offset).rem_euclid(12) == 11;
    if mortar {
        TEMPLE[0]
    } else if y.rem_euclid(8) == 0 {
        TEMPLE[3]
    } else if noise(x, y, 5, 77) > 0.7 {
        TEMPLE[1]
    } else {
        TEMPLE[2]
    }
}

/// Paints a sky-land tile.
#[allow(clippy::too_many_lines, reason = "one arm per sky tile")]
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
        SkyTile::Vines => {
            paint_vines(canvas, seed, edges.up == Cell::Solid);
            return;
        }
        SkyTile::Nest => {
            paint_nest(canvas, seed);
            return;
        }
        SkyTile::RuinPillar => {
            paint_pillar(canvas, edges.up.is_open(), seed);
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
        SkyTile::TempleDoor | SkyTile::KeystoneDoor => {
            paint_gate(canvas, tile == SkyTile::KeystoneDoor, edges);
            return;
        }
        SkyTile::TempleLever => {
            paint_body(canvas, tile);
            paint_lever(canvas);
            return;
        }
        SkyTile::TempleKeystone => {
            paint_temple_keystone(canvas);
            return;
        }
        SkyTile::TempleCache => {
            paint_relic_chest(canvas);
            return;
        }
        SkyTile::HarpyNest => {
            paint_harpy_nest(canvas, seed, edges);
            return;
        }
        SkyTile::HarpyEgg => {
            paint_egg(canvas, seed);
            return;
        }
        _ => {}
    }
    paint_body(canvas, tile);
    let (lip, shade) = match tile {
        SkyTile::Turf(biome) => (turf_colors(biome)[0], SOIL_RAMP[0]),
        SkyTile::Soil | SkyTile::Roots | SkyTile::Vines | SkyTile::Nest | SkyTile::Spring => {
            (SOIL_RAMP[3], SOIL_RAMP[0])
        }
        SkyTile::Rock(biome) => (rock_ramp(biome)[3], rock_ramp(biome)[0]),
        SkyTile::Crystal => (CRYSTAL[3], CRYSTAL[0]),
        SkyTile::ContinentRock => (CONTINENT[3], CONTINENT[0]),
        SkyTile::Regolith => (REGOLITH[3], REGOLITH[0]),
        SkyTile::MoonRock => (MOON_ROCK[3], MOON_ROCK[0]),
        SkyTile::RuinBrick | SkyTile::RuinPillar => (RUIN[3], RUIN[0]),
        SkyTile::Cache | SkyTile::Keystone | SkyTile::Vent => (
            rock_ramp(SkyBiome::Meadow)[3],
            rock_ramp(SkyBiome::Meadow)[0],
        ),
        SkyTile::BuoyPad => (PAD[3], PAD[0]),
        SkyTile::AetherCore => (CORE[3], CORE[0]),
        SkyTile::AetherCrystal => (AETHER[3], AETHER[0]),
        SkyTile::StormGlass => (STORM[3], STORM[0]),
        SkyTile::TempleBrick
        | SkyTile::TempleDoor
        | SkyTile::KeystoneDoor
        | SkyTile::TempleLever
        | SkyTile::TempleKeystone
        | SkyTile::TempleCache
        | SkyTile::HarpyNest
        | SkyTile::HarpyEgg => (TEMPLE[3], TEMPLE[0]),
    };
    match tile {
        SkyTile::Turf(biome) => paint_turf(canvas, biome, edges.up.is_open()),
        SkyTile::Cache => paint_sky_chest(canvas),
        SkyTile::Keystone => paint_keystone(canvas, seed),
        SkyTile::Vent => paint_vent(canvas),
        SkyTile::BuoyPad => paint_pad(canvas, edges.up.is_open()),
        SkyTile::Spring => paint_spring(canvas, edges),
        SkyTile::RuinBrick => paint_glyph(canvas, seed),
        _ => {}
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

/// Fills the tile with `tile`'s body pattern.
fn paint_body(canvas: &mut TileCanvas, tile: SkyTile) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, body_pixel(tile, gx, gy));
        }
    }
}

/// A bronze temple gate: riveted vertical bars in a dark frame; a keystone gate has a glowing
/// keystone socket.
fn paint_gate(canvas: &mut TileCanvas, keystone: bool, edges: Edges) {
    let ink = index_of(INK);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let color = match px.rem_euclid(4) {
                0 => ink,
                1 => BRONZE[3],
                2 => BRONZE[2],
                _ => BRONZE[1],
            };
            canvas.set(px, py, color);
        }
        if py.rem_euclid(6) == 2 {
            for px in (1..TILE_ART).step_by(4) {
                canvas.set(px, py, index_of(CREAM));
            }
        }
    }
    if edges.up != Cell::Solid {
        for px in 0..TILE_ART {
            canvas.set(px, 0, ink);
            canvas.set(px, 1, BRONZE[0]);
        }
    }
    if keystone {
        // A diamond socket glowing mint.
        for dy in -4_i32..=4 {
            let half = 4 - dy.abs();
            for dx in -half..=half {
                let color = if dx.abs() == half {
                    INK
                } else if dy < 0 {
                    ICE
                } else {
                    MINT
                };
                canvas.set(8 + dx, 8 + dy, index_of(color));
            }
        }
    }
}

/// A lever set in temple stone: a dark slot and a handle with a red knob.
fn paint_lever(canvas: &mut TileCanvas) {
    let ink = index_of(INK);
    for py in 9..14 {
        for px in 5..11 {
            let color = if py == 9 || py == 13 {
                ink
            } else {
                index_of(SLATE_DARK)
            };
            canvas.set(px, py, color);
        }
    }
    for step in 0..7 {
        canvas.set(8 + step / 2, 11 - step, index_of(STEEL_DARK));
        canvas.set(9 + step / 2, 11 - step, index_of(STEEL_LIGHT));
    }
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1), (-1, 0), (0, -1)] {
        canvas.set(12 + dx, 3 + dy, index_of(RED));
    }
    canvas.set(12, 3, index_of(PINK));
}

/// A temple keystone on its plinth: a glowing wedge with a gold rim.
fn paint_temple_keystone(canvas: &mut TileCanvas) {
    let ink = index_of(INK);
    for py in 12..16 {
        for px in 2..14 {
            canvas.set(px, py, if py == 12 { TEMPLE[3] } else { TEMPLE[1] });
        }
    }
    // The wedge: wide at the top, narrow at the bottom.
    for py in 2..12 {
        let half = 6 - (py - 2) / 3;
        for px in 8 - half..8 + half {
            let edge = px == 8 - half || px == 8 + half - 1 || py == 2;
            let color = if edge {
                index_of(GOLD)
            } else if px < 8 {
                index_of(ICE)
            } else {
                index_of(MINT)
            };
            canvas.set(px, py, color);
        }
    }
    canvas.set(6, 5, index_of(BONE));
    for px in 1..15 {
        canvas.set(px, 11, if px % 3 == 0 { ink } else { TEMPLE[0] });
    }
}

/// A sealed relic chest: carved stone with a gold band and a mint seal.
fn paint_relic_chest(canvas: &mut TileCanvas) {
    let ink = index_of(INK);
    for py in 4..16 {
        for px in 1..15 {
            let edge = px == 1 || px == 14 || py == 4 || py == 15;
            let color = if edge {
                ink
            } else if py == 8 || py == 9 {
                index_of(GOLD)
            } else if py < 8 {
                TEMPLE[3]
            } else {
                TEMPLE[2]
            };
            canvas.set(px, py, color);
        }
    }
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        canvas.set(7 + dx, 8 + dy, index_of(MINT));
    }
    canvas.set(3, 6, index_of(BONE));
}

/// A woven harpy nest: crisscrossed twigs, ragged on top when open above.
fn paint_harpy_nest(canvas: &mut TileCanvas, seed: u32, edges: Edges) {
    let open_above = edges.up.is_open();
    let seed = (seed % 97) as i32;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            if open_above && py < 5 && (px + py + seed) % 5 != 0 {
                continue;
            }
            let weave =
                (px + py * 2 + seed).rem_euclid(5) < 2 || (px * 2 - py + seed).rem_euclid(7) == 0;
            let level = if weave {
                3
            } else {
                usize::from(hash(px, py, 79).is_multiple_of(3)) + 1
            };
            canvas.set(px, py, TWIGS[level]);
        }
    }
    for strand in 0..3 {
        let h = hash(seed, strand, 80);
        let x = 2 + (h % 12) as i32;
        canvas.set(x, 0, TWIGS[2]);
        canvas.set(x + 1, 1, TWIGS[1]);
    }
}

/// A speckled harpy egg sitting in twigs.
fn paint_egg(canvas: &mut TileCanvas, seed: u32) {
    let ink = index_of(INK);
    let seed = (seed % 89) as i32;
    for py in 2..15 {
        let dy = py - 8;
        let half = if dy < 0 {
            4 - (dy * dy) / 9
        } else {
            5 - (dy * dy) / 8
        };
        for px in 8 - half..=8 + half {
            let edge = px == 8 - half || px == 8 + half || py == 2 || py == 14;
            let color = if edge {
                ink
            } else if (px * 7 + py * 3 + seed).rem_euclid(11) == 0 {
                index_of(SKY_LIGHT)
            } else if px < 7 {
                index_of(BONE)
            } else {
                index_of(CREAM)
            };
            canvas.set(px, py, color);
        }
    }
    for px in 0..TILE_ART {
        if px % 3 != 1 {
            canvas.set(px, 14, TWIGS[2]);
        }
        canvas.set(px, 15, TWIGS[1]);
    }
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

/// Ruin stone: big squared blocks with thin mortar seams.
const fn brick(x: i32, y: i32) -> usize {
    let row = y.div_euclid(5);
    let offset = if row.rem_euclid(2) == 0 { 0 } else { 4 };
    if y.rem_euclid(5) == 4 || (x + offset).rem_euclid(8) == 7 {
        0
    } else if y.rem_euclid(5) == 0 {
        3
    } else if hash(x, y, 441).is_multiple_of(11) {
        1
    } else {
        2
    }
}

/// Stamps `rows` (a tiny legend) with its top-left at `(left, top)`.
fn stamp_rows(canvas: &mut TileCanvas, rows: &[&str], left: i32, top: i32) {
    for (row, line) in rows.iter().enumerate() {
        for (column, symbol) in line.chars().enumerate() {
            let color = match symbol {
                'k' => INK,
                'w' => RUST,
                'c' => COPPER,
                'y' => GOLD,
                'l' => CREAM,
                's' => SAND,
                'd' => LOAM,
                'o' => SOIL,
                'b' => BONE,
                'm' => MIST,
                'f' => FOG,
                'i' => ICE,
                'q' => SKY_LIGHT,
                'u' => SKY,
                'p' => PLUM,
                'v' => VIOLET,
                'a' => LAVENDER,
                'n' => PINK,
                'g' => LEAF,
                'e' => MOSS,
                'h' => LIME,
                't' => MINT,
                'x' => STEEL_DARK,
                'z' => AMBER,
                _ => continue,
            };
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_possible_wrap,
                reason = "tiny stamps"
            )]
            canvas.set(left + column as i32, top + row as i32, index_of(color));
        }
    }
}

/// A sky cache: a sky-blue strongbox bound in brass, set into the island rock.
fn paint_sky_chest(canvas: &mut TileCanvas) {
    const CHEST: [&str; 10] = [
        "..kkkkkkkkkkkk..",
        ".kyyyyyyyyyyyyk.",
        ".kquuuuyyuuuuqk.",
        ".kyyyyyyyyyyyyk.",
        ".kkkkkkllkkkkkk.",
        ".kquuuuyyuuuuqk.",
        ".kyuuuuuuuuuuyk.",
        ".kquuuuuuuuuuqk.",
        ".kyyyyyyyyyyyyk.",
        ".kkkkkkkkkkkkkk.",
    ];
    stamp_rows(canvas, &CHEST, 0, 4);
}

/// A twig nest sitting on the turf, two speckled eggs and a feather.
fn paint_nest(canvas: &mut TileCanvas, seed: u32) {
    const NEST: [&str; 8] = [
        "..........b.....",
        ".........bm.....",
        "....kkk.kmk.....",
        "...kbbmkllbk....",
        ".kkdbqbmlqbkdkk.",
        "kdsdkbbkkbbksdsk",
        "kddsdsdsddsdsddk",
        ".kkdddddddddkkk.",
    ];
    #[allow(clippy::cast_possible_wrap, reason = "a tiny shift")]
    let shift = (seed % 3) as i32 - 1;
    stamp_rows(canvas, &NEST, shift, 8);
}

/// A broken ruin pillar: a fluted white column, its top snapped off when nothing stands on it.
fn paint_pillar(canvas: &mut TileCanvas, broken_top: bool, seed: u32) {
    let ink = index_of(INK);
    for py in 0..TILE_ART {
        for px in 3..=12 {
            let color = if px == 3 || px == 12 {
                ink
            } else if px == 5 || px == 9 {
                RUIN[1]
            } else if px < 6 {
                RUIN[3]
            } else {
                RUIN[2]
            };
            canvas.set(px, py, color);
        }
    }
    if broken_top {
        // A jagged break across the top rows.
        for px in 3..=12 {
            #[allow(clippy::cast_possible_wrap, reason = "a small hash slice")]
            let bite = (hash(px, seed as i32, 447) % 5) as i32;
            for py in 0..bite {
                canvas.set(px, py, TRANSPARENT);
            }
            canvas.set(px, bite, ink);
        }
    } else {
        for px in 2..=13 {
            canvas.set(px, 0, ink);
            canvas.set(px, 1, RUIN[3]);
        }
    }
    // The pillar's base.
    for px in 2..=13 {
        canvas.set(px, TILE_ART - 2, RUIN[1]);
        canvas.set(px, TILE_ART - 1, ink);
    }
}

/// A gold glyph carved into some ruin blocks.
fn paint_glyph(canvas: &mut TileCanvas, seed: u32) {
    const GLYPH: [&str; 6] = ["..zz..", ".z..z.", "zz..zz", ".zyyz.", "..yy..", ".y..y."];
    if !seed.is_multiple_of(3) {
        return;
    }
    stamp_rows(canvas, &GLYPH, 5, 5);
}

/// The aether keystone: a violet crystal hexagon wired into the rock, humming with light.
fn paint_keystone(canvas: &mut TileCanvas, seed: u32) {
    const STONE: [&str; 12] = [
        ".....kkkkkk.....",
        "....kavvvvak....",
        "...kanavvvvak...",
        "..kannavvvvvak..",
        ".kvaannvvvvvpvk.",
        ".kvvaanavvvppvk.",
        ".kvvvaaavvpppvk.",
        ".kvvvvavvpppvvk.",
        "..kvvvvvppppvk..",
        "...kpvvppppvk...",
        "....kpppppvk....",
        ".....kkkkkk.....",
    ];
    stamp_rows(canvas, &STONE, 0, 2);
    // A sparkle that sits somewhere different on every keystone.
    #[allow(clippy::cast_possible_wrap, reason = "a small hash slice")]
    let sparkle = 5 + (seed % 5) as i32;
    canvas.set(sparkle, 6, AETHER[3]);
}

/// An updraft vent: a stone ring with a slotted grate and mint wisps breathing up.
fn paint_vent(canvas: &mut TileCanvas) {
    const VENT: [&str; 7] = [
        "..t....t....t...",
        "kkkkkkkkkkkkkkkk",
        "kffffffffffffffk",
        "kxkxkxkxkxkxkxkk",
        "kfkfkfkfkfkfkfkk",
        "kxkxkxkxkxkxkxkk",
        "kmmmmmmmmmmmmmmk",
    ];
    stamp_rows(canvas, &VENT, 0, 0);
}

/// A buoy pad: steel deck plates with a hazard stripe along the top.
fn paint_pad(canvas: &mut TileCanvas, open_above: bool) {
    if !open_above {
        return;
    }
    for px in 0..TILE_ART {
        let stripe = (px / 3).rem_euclid(2) == 0;
        let color = if stripe {
            index_of(AMBER)
        } else {
            index_of(INK)
        };
        canvas.set(px, 0, index_of(STEEL_LIGHT));
        canvas.set(px, 1, color);
        canvas.set(px, 2, color);
        canvas.set(px, 3, index_of(STEEL_DARK));
    }
    for px in [2, 7, 12] {
        canvas.set(px, 8, index_of(STEEL_LIGHT));
        canvas.set(px, 13, index_of(STEEL_LIGHT));
    }
}

/// A spring: a mossy stone lip with clear water welling out toward the open side.
fn paint_spring(canvas: &mut TileCanvas, edges: Edges) {
    const LIP: [&str; 9] = [
        "eegeegeegeegeege",
        "kkkkkkkkkkkkkkkk",
        "kmmffmmffmmffmmk",
        "kqqqiqqqqiqqqqqk",
        "kuqqqqiqquqqqiqk",
        "kuuqquqqquuqquuk",
        "kkkkkkkkkkkkkkkk",
        "kffmmffmmffmmffk",
        "kkkkkkkkkkkkkkkk",
    ];
    stamp_rows(canvas, &LIP, 0, 4);
    // Water spilling over the open rim.
    let spill = if edges.right.is_open() {
        Some(TILE_ART - 1)
    } else if edges.left.is_open() {
        Some(0)
    } else {
        None
    };
    if let Some(x) = spill {
        for py in 7..TILE_ART {
            canvas.set(x, py, index_of(SKY_LIGHT));
        }
    }
}

/// Leafy vines dangling under an island: green strands with leaves, longer than roots.
fn paint_vines(canvas: &mut TileCanvas, seed: u32, from_above: bool) {
    let (stem, leaf, light) = (index_of(MOSS), index_of(LEAF), index_of(LIME));
    let strands = 1 + seed % 3;
    for strand in 0..strands {
        #[allow(clippy::cast_possible_wrap, reason = "hash inputs")]
        let h = hash(seed as i32, strand as i32, 451);
        #[allow(clippy::cast_possible_wrap, reason = "small hash slices")]
        let mut x = 2 + (h % 12) as i32;
        #[allow(clippy::cast_possible_wrap, reason = "small hash slices")]
        let length = if from_above {
            TILE_ART
        } else {
            9 + (h >> 4) as i32 % 7
        };
        #[allow(clippy::cast_possible_wrap, reason = "small hash slices")]
        let phase = (h % 4) as i32;
        for py in 0..length {
            canvas.set(x, py, stem);
            if (py + phase).rem_euclid(4) == 0 {
                canvas.set(x - 1, py, leaf);
                canvas.set(x + 1, py, leaf);
                canvas.set(x + 1, py - 1, light);
            }
            if (h >> (py % 24)).trailing_zeros() >= 3 {
                x += if (h >> 5) & 1 == 0 { 1 } else { -1 };
                x = x.clamp(2, TILE_ART - 3);
            }
        }
        if h.is_multiple_of(3) {
            canvas.set(x, length.min(TILE_ART) - 1, index_of(PINK));
        }
    }
}
