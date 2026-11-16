//! The Moon's living light (WP-MOON): crystal blooms, glyphs, lit pylons, veils and conduits
//! throw soft pulsing halos, and the core blazes in its chamber. Only explored tiles glow.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "small art-pixel math"
)]

use fatherload_simulation::{
    moon::LunarTile,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    world::DiscoveredTiles,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::terrain::tile_art::hash;

/// The halo colour of a glowing lunar tile.
const fn glow_color(tile: LunarTile) -> Option<Color> {
    match tile {
        LunarTile::CrystalBloom => Some(PINK),
        LunarTile::FacilityGlyph | LunarTile::PylonLit | LunarTile::Veil => Some(MINT),
        LunarTile::Conduit => Some(ORANGE),
        LunarTile::CoreSeal => Some(VIOLET),
        LunarTile::CoreHeart => Some(ICE),
        LunarTile::Ice => Some(SKY_LIGHT),
        _ => None,
    }
}

/// Draws the halos of glowing lunar tiles in `visible` (world pixels).
pub(super) fn draw_lunar_glows(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    explored: Option<&DiscoveredTiles>,
    visible: Rectangle,
    seconds: f32,
) {
    if !terrain.settings().generator.has_moon_content() || visible.y > 0.0 {
        return;
    }
    let first = TilePosition {
        x: (visible.x / TILE_SIZE).floor() as i32 - 1,
        y: (visible.y / TILE_SIZE).floor() as i32 - 1,
    };
    let last = TilePosition {
        x: ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + 1,
        y: ((visible.y + visible.height) / TILE_SIZE).ceil() as i32 + 1,
    };
    for y in first.y..=last.y {
        for x in first.x..=last.x {
            let position = TilePosition { x, y };
            let Some(TileKind::Lunar(tile)) = terrain.tile(position).map(|t| t.kind) else {
                continue;
            };
            let Some(color) = glow_color(tile) else {
                continue;
            };
            if tile != LunarTile::CoreHeart
                && !explored.is_none_or(|tiles| tiles.contains(position))
            {
                continue;
            }
            let (cx, cy) = ((x as f32 + 0.5) * TILE_SIZE, (y as f32 + 0.5) * TILE_SIZE);
            let phase = (hash(x, y, 731) % 628) as f32 / 100.0;
            let pulse = (seconds * 1.4 + phase).sin().mul_add(0.5, 0.5);
            let (radius, alpha) = match tile {
                LunarTile::CoreHeart => (TILE_SIZE * 2.6, 26.0 + pulse * 22.0),
                LunarTile::Ice => (TILE_SIZE * 0.6, 4.0 + pulse * 6.0),
                LunarTile::CrystalBloom | LunarTile::PylonLit => {
                    (TILE_SIZE * 1.1, 14.0 + pulse * 14.0)
                }
                _ => (TILE_SIZE * 0.8, 10.0 + pulse * 10.0),
            };
            draw.draw_circle(cx as i32, cy as i32, radius, with_alpha(color, alpha as u8));
            draw.draw_circle(
                cx as i32,
                cy as i32,
                radius * 0.6,
                with_alpha(color, (alpha * 1.5) as u8),
            );
        }
    }
}

/// Tranquility Station's furniture and lights (drawn over its tiles): blinking pad beacons,
/// the mast's warning light, the counter with its screen, potted plants under the dome and a
/// light in the hab module's window.
#[allow(clippy::too_many_lines, reason = "one block per prop")]
pub(super) fn draw_colony_props(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    visible: Rectangle,
    seconds: f32,
) {
    if !terrain.settings().generator.has_moon_content() || visible.y > 0.0 {
        return;
    }
    let Some(layout) = fatherload_simulation::worldgen::moon::layout(terrain.settings()) else {
        return;
    };
    let area = layout.colony_area();
    let (left, top) = (area.min.x as f32 * TILE_SIZE, area.min.y as f32 * TILE_SIZE);
    let (right, bottom) = (
        (area.max.x + 1) as f32 * TILE_SIZE,
        (area.max.y + 1) as f32 * TILE_SIZE,
    );
    if right < visible.x
        || left > visible.x + visible.width
        || bottom < visible.y
        || top > visible.y + visible.height
    {
        return;
    }
    let g = layout.colony.ground_row as f32 * TILE_SIZE;
    let cx = layout.colony.center_x as f32 * TILE_SIZE;
    let px = |tiles: f32| cx + tiles * TILE_SIZE;
    let rect =
        |draw: &mut dyn FnMut(Rectangle, Color), x: f32, y: f32, w: f32, h: f32, c: Color| {
            draw(Rectangle::new(x, y, w, h), c);
        };
    let mut fill = |r: Rectangle, c: Color| draw.draw_rectangle_rec(r, c);
    // Pad beacons: amber, alternating.
    let (pad_first, pad_last, _) = layout.pad();
    for (index, column) in [pad_first, pad_last].into_iter().enumerate() {
        let on = ((seconds * 1.5) as i32 + index as i32) % 2 == 0;
        let x = column as f32 * TILE_SIZE + TILE_SIZE * 0.5;
        rect(&mut fill, x - 4.0, g - TILE_SIZE - 6.0, 8.0, 6.0, INK);
        rect(
            &mut fill,
            x - 3.0,
            g - TILE_SIZE - 5.0,
            6.0,
            4.0,
            if on { GOLD } else { MAROON },
        );
        if on {
            rect(
                &mut fill,
                x - 10.0,
                g - TILE_SIZE - 12.0,
                20.0,
                16.0,
                with_alpha(AMBER, 40),
            );
        }
    }
    // The mast's warning light.
    let mast = px(35.5);
    let blink = (seconds * 0.9).fract() < 0.25;
    rect(
        &mut fill,
        mast - 4.0,
        g - 22.0 * TILE_SIZE - 8.0,
        8.0,
        8.0,
        INK,
    );
    rect(
        &mut fill,
        mast - 3.0,
        g - 22.0 * TILE_SIZE - 7.0,
        6.0,
        6.0,
        if blink { RED } else { MAROON },
    );
    if blink {
        rect(
            &mut fill,
            mast - 14.0,
            g - 22.0 * TILE_SIZE - 18.0,
            28.0,
            28.0,
            with_alpha(RED, 50),
        );
    }
    // The counter, its screen flickering mint.
    let desk = px(3.0);
    rect(&mut fill, desk, g - 22.0, 70.0, 22.0, INK);
    rect(&mut fill, desk + 2.0, g - 20.0, 66.0, 18.0, STEEL);
    rect(&mut fill, desk + 2.0, g - 20.0, 66.0, 3.0, STEEL_LIGHT);
    rect(&mut fill, desk + 40.0, g - 40.0, 24.0, 18.0, INK);
    let glow = if (seconds * 3.0).fract() < 0.9 {
        MINT
    } else {
        TEAL
    };
    rect(&mut fill, desk + 42.0, g - 38.0, 20.0, 14.0, glow);
    rect(&mut fill, desk + 44.0, g - 34.0, 12.0, 2.0, TEAL_DARK);
    rect(&mut fill, desk + 44.0, g - 30.0, 8.0, 2.0, TEAL_DARK);
    // Potted plants: green in a grey world.
    for tiles in [-9.0, -4.0, 10.0] {
        let x = px(tiles);
        rect(&mut fill, x, g - 14.0, 18.0, 14.0, INK);
        rect(&mut fill, x + 2.0, g - 12.0, 14.0, 12.0, RUST);
        for (dx, dy, w, h) in [
            (2.0, -30.0, 6.0, 18.0),
            (8.0, -38.0, 6.0, 26.0),
            (13.0, -28.0, 5.0, 16.0),
        ] {
            rect(&mut fill, x + dx, g + dy, w, h, LEAF);
            rect(&mut fill, x + dx, g + dy, w, 3.0, LIME);
        }
    }
    // A warm light in the hab module's window.
    rect(
        &mut fill,
        px(29.0),
        g - 4.0 * TILE_SIZE,
        TILE_SIZE,
        TILE_SIZE * 2.0,
        with_alpha(GOLD, 70),
    );
}

/// How far a glowing lunar tile lights the dark (tiles; 0: it doesn't).
pub(super) const fn lunar_light(tile: LunarTile) -> f32 {
    tile.glow() as f32 / 255.0 * 4.0
}
