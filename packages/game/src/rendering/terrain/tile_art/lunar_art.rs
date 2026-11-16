//! Tile art for the filled Moon (see `fatherload_simulation::moon`, WP-MOON).
//!
//! The Moon reads as a cold, quiet place lit hard from one side: bright pale-grey regolith with
//! dark pebbles and a sunlit rim, dark basalt maria, cobbled grey rock giving way to slabby
//! near-black deep rock along the underside, glassy plum-black basalt lining the lava tubes,
//! faceted blue-white ice, regolith freckled gold where helium-3 sits, violet-and-mint alien
//! alloy veins, and violet crystal trees whose blooms glow. Built things contrast with it: the
//! colony's clean steel panels, orange safety stripes, blue dome glass and a hazard-striped pad;
//! rusted, dented habitat panels; and the alien facility's huge near-black teal blocks carved
//! with glowing mint glyphs, shimmering veils, pylons that show their lighting order as dots, a
//! barred gate, orange power conduits, a violet seal and the iridescent shell around the core.
//!
//! The air inside the Moon (lava tubes, the facility's rooms, the colony's rooms) gets its own
//! back walls ([`paint_lunar_air`]) instead of the open sky.

use fatherload_simulation::{
    moon::LunarTile,
    terrain::{Terrain, TilePosition},
    worldgen::moon::{MoonLayout, layout},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, index_of};

use super::{
    Edges, Ramp, Strata, TILE_ART, TileCanvas, cobble_level, dirt_level, hash, noise, paint_edges,
    ramp, slab_level,
};

const REGOLITH: Ramp = ramp([SLATE_LIGHT, FOG, MIST, BONE]);
const MARE: Ramp = ramp([NIGHT, SLATE_DARK, SLATE, SLATE_LIGHT]);
const ROCK: Ramp = ramp([NIGHT, SLATE_DARK, SLATE, FOG]);
const DEEP: Ramp = ramp([INK, NIGHT, SLATE_DARK, SLATE]);
const BASALT: Ramp = ramp([INK, NIGHT, SLATE_DARK, PLUM]);
const ICE_RAMP: Ramp = ramp([OCEAN, SKY, ICE, BONE]);
const CRYSTAL: Ramp = ramp([PLUM, VIOLET, LAVENDER, PINK]);
const BLOOM: Ramp = ramp([VIOLET, PINK, ROSE, BONE]);
const HULL: Ramp = ramp([STEEL_DARK, STEEL, STEEL_LIGHT, BONE]);
const GLASS: Ramp = ramp([DEEP_BLUE, OCEAN, SKY, SKY_LIGHT]);
const RUSTY: Ramp = ramp([MAROON, RUST, COPPER, STEEL_LIGHT]);
const FACILITY: Ramp = ramp([INK, NIGHT, TEAL_DARK, SLATE_DARK]);
const GLYPH: Ramp = ramp([TEAL_DARK, TEAL, MINT, ICE]);
const SHELL: Ramp = ramp([INK, PLUM, VIOLET, LAVENDER]);
const CORE: Ramp = ramp([TEAL, MINT, ICE, BONE]);

/// Representative colour of a lunar tile (drill dust, debris, the map).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "one colour per tile, grouped by look"
)]
pub const fn lunar_color(tile: LunarTile) -> raylib::prelude::Color {
    match tile {
        LunarTile::Regolith => MIST,
        LunarTile::MareDust => SLATE,
        LunarTile::Rock | LunarTile::RelicCache => SLATE_DARK,
        LunarTile::DeepRock => NIGHT,
        LunarTile::Basalt => PLUM,
        LunarTile::Ice => ICE,
        LunarTile::Helium3 => GOLD,
        LunarTile::AlloyVein => LAVENDER,
        LunarTile::CrystalStalk => VIOLET,
        LunarTile::CrystalBloom => PINK,
        LunarTile::ColonyHull | LunarTile::ColonyPad => STEEL,
        LunarTile::ColonyGlass => SKY,
        LunarTile::HabitatPanel => RUST,
        LunarTile::FacilityWall | LunarTile::Gate => TEAL_DARK,
        LunarTile::FacilityGlyph | LunarTile::Veil | LunarTile::PylonLit => MINT,
        LunarTile::Pylon => TEAL,
        LunarTile::Conduit => ORANGE,
        LunarTile::CoreSeal | LunarTile::CoreShell => VIOLET,
        LunarTile::CoreHeart => BONE,
    }
}

/// The body pattern of a lunar tile at art pixel `(x, y)` (global coordinates).
#[allow(clippy::too_many_lines, reason = "one pattern per lunar material")]
fn body_pixel(tile: LunarTile, x: i32, y: i32) -> PaletteIndex {
    match tile {
        LunarTile::Regolith | LunarTile::Helium3 => {
            let h = hash(x, y, 701);
            if tile == LunarTile::Helium3 && h % 11 < 2 {
                return index_of(if h.is_multiple_of(3) { GOLD } else { AMBER });
            }
            // Fine dust: a speckle of pebbles and glints over a soft mottling.
            if h.is_multiple_of(29) {
                REGOLITH[0]
            } else if h.is_multiple_of(17) {
                REGOLITH[3]
            } else if noise(x, y, 3, 702) > 0.7 || noise(x, y, 11, 719) > 0.78 {
                REGOLITH[1]
            } else {
                REGOLITH[2]
            }
        }
        LunarTile::MareDust => {
            let h = hash(x, y, 703);
            if h.is_multiple_of(31) {
                MARE[3]
            } else if h.is_multiple_of(19) || noise(x, y, 3, 704) > 0.72 {
                MARE[1]
            } else {
                MARE[2]
            }
        }
        LunarTile::Rock | LunarTile::RelicCache => {
            if hash(x, y, 705).is_multiple_of(113) {
                index_of(LAVENDER)
            } else {
                ROCK[cobble_level(x, y)]
            }
        }
        LunarTile::DeepRock => DEEP[slab_level(x, y)],
        LunarTile::Basalt => {
            // Glassy flow bands with a cold sheen.
            let band = (y + (noise(x, y, 12, 706) * 5.0) as i32).rem_euclid(5);
            if hash(x, y, 707).is_multiple_of(41) {
                index_of(LAVENDER)
            } else if band == 0 {
                BASALT[3]
            } else {
                BASALT[usize::from(band % 2 == 0)]
            }
        }
        LunarTile::Ice => {
            if hash(x, y, 708).is_multiple_of(29) {
                index_of(BONE)
            } else {
                ICE_RAMP[slab_level(x, y).min(3)]
            }
        }
        LunarTile::AlloyVein => {
            let shimmer = noise(x, y, 3, 709);
            if shimmer > 0.66 {
                index_of(MINT)
            } else if shimmer > 0.45 {
                index_of(LAVENDER)
            } else if shimmer > 0.3 {
                index_of(VIOLET)
            } else {
                ROCK[cobble_level(x, y)]
            }
        }
        LunarTile::CrystalStalk => CRYSTAL[slab_level(x, y)],
        LunarTile::CrystalBloom => BLOOM[slab_level(x, y)],
        LunarTile::ColonyHull | LunarTile::ColonyPad => {
            let (px, py) = (x.rem_euclid(TILE_ART), y.rem_euclid(TILE_ART));
            if (px == 2 || px == 13) && (py == 2 || py == 13) {
                HULL[0]
            } else if py == 0 || px == 0 {
                HULL[3]
            } else if py == TILE_ART - 1 || px == TILE_ART - 1 {
                HULL[0]
            } else {
                HULL[2]
            }
        }
        LunarTile::ColonyGlass => {
            let streak = (x + y).rem_euclid(13);
            if streak < 2 {
                GLASS[3]
            } else if streak < 4 {
                GLASS[2]
            } else {
                GLASS[usize::from(noise(x, y, 6, 710) > 0.5)]
            }
        }
        LunarTile::HabitatPanel => {
            let h = hash(x, y, 711);
            let (px, py) = (x.rem_euclid(TILE_ART), y.rem_euclid(TILE_ART));
            if px == 0 || py == 0 {
                RUSTY[3]
            } else if h.is_multiple_of(7) {
                RUSTY[0]
            } else {
                RUSTY[1 + usize::from(noise(x, y, 4, 712) > 0.5)]
            }
        }
        LunarTile::FacilityWall
        | LunarTile::FacilityGlyph
        | LunarTile::Pylon
        | LunarTile::PylonLit
        | LunarTile::Gate
        | LunarTile::Conduit
        | LunarTile::CoreSeal => {
            // Huge ancient blocks, three tiles to a course.
            let course = y.div_euclid(TILE_ART * 2);
            let (bx, by) = (
                (x + course * TILE_ART * 3 / 2).rem_euclid(TILE_ART * 3),
                y.rem_euclid(TILE_ART * 2),
            );
            if bx == 0 || by == 0 {
                FACILITY[0]
            } else if bx == 1 || by == 1 {
                FACILITY[3]
            } else if hash(x, y, 713).is_multiple_of(23) {
                FACILITY[2]
            } else {
                FACILITY[1]
            }
        }
        LunarTile::CoreShell => {
            // Iridescent hexes.
            let level = cobble_level(x / 2, y / 2);
            SHELL[level]
        }
        LunarTile::CoreHeart => CORE[3 - slab_level(x, y).min(3)],
        LunarTile::Veil => index_of(MINT),
    }
}

/// Paints a lunar tile.
pub(super) fn paint(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: LunarTile,
    strata: &Strata,
    edges: Edges,
) {
    if tile == LunarTile::Veil {
        paint_veil(canvas);
        return;
    }
    let embedded = !edges.left.is_open() && !edges.right.is_open();
    if tile == LunarTile::CrystalBloom && (!embedded || edges.up.is_open()) {
        paint_bloom(canvas, hash(position.x, position.y, 714));
        return;
    }
    if tile == LunarTile::CrystalStalk && !embedded {
        paint_stalk(canvas);
        return;
    }
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, body_pixel(tile, gx, gy));
        }
    }
    match tile {
        LunarTile::FacilityGlyph => paint_glyph(canvas, hash(position.x, position.y, 715)),
        LunarTile::Pylon | LunarTile::PylonLit => {
            let dots = layout(terrain.settings()).and_then(|layout| pylon_dots(&layout, position));
            paint_pylon(canvas, tile == LunarTile::PylonLit, dots);
        }
        LunarTile::Gate => paint_gate(canvas),
        LunarTile::Conduit => paint_conduit(canvas),
        LunarTile::CoreSeal => paint_seal(canvas),
        LunarTile::RelicCache => paint_cache(canvas),
        LunarTile::ColonyPad if edges.up.is_open() => paint_pad_stripes(canvas),
        LunarTile::ColonyHull if position.y.rem_euclid(3) == 0 => {
            // An orange safety stripe along the hull.
            for px in 0..TILE_ART {
                canvas.set(px, 7, index_of(ORANGE));
                canvas.set(px, 8, index_of(AMBER));
            }
        }
        _ => {}
    }
    let (lip, shade) = match tile {
        LunarTile::Regolith | LunarTile::Helium3 => (REGOLITH[3], REGOLITH[0]),
        LunarTile::MareDust => (MARE[3], MARE[0]),
        LunarTile::Rock | LunarTile::RelicCache | LunarTile::AlloyVein => (ROCK[3], ROCK[0]),
        LunarTile::DeepRock => (DEEP[3], DEEP[0]),
        LunarTile::Basalt => (BASALT[3], BASALT[0]),
        LunarTile::Ice => (ICE_RAMP[3], ICE_RAMP[0]),
        LunarTile::CrystalStalk => (CRYSTAL[3], CRYSTAL[0]),
        LunarTile::CrystalBloom => (BLOOM[3], BLOOM[0]),
        LunarTile::ColonyHull | LunarTile::ColonyPad => (HULL[3], HULL[0]),
        LunarTile::ColonyGlass => (GLASS[3], GLASS[0]),
        LunarTile::HabitatPanel => (RUSTY[3], RUSTY[0]),
        LunarTile::CoreShell => (SHELL[3], SHELL[0]),
        LunarTile::CoreHeart => (CORE[3], CORE[0]),
        _ => (FACILITY[3], FACILITY[0]),
    };
    if matches!(tile, LunarTile::Regolith | LunarTile::Helium3) && edges.up.is_open() {
        // The sunlit rim of the dust.
        for px in 0..TILE_ART {
            canvas.set(px, 0, REGOLITH[3]);
            if hash(canvas.origin_x + px, 1, 716).is_multiple_of(2) {
                canvas.set(px, 1, REGOLITH[3]);
            }
        }
    }
    let square = matches!(
        tile,
        LunarTile::ColonyHull
            | LunarTile::ColonyGlass
            | LunarTile::ColonyPad
            | LunarTile::HabitatPanel
            | LunarTile::FacilityWall
            | LunarTile::FacilityGlyph
            | LunarTile::Gate
            | LunarTile::CoreSeal
    );
    paint_edges(canvas, strata, edges, lip, shade, square);
}

/// How many dots pylon `position` shows (its place in the lighting order, from 1).
fn pylon_dots(layout: &MoonLayout, position: TilePosition) -> Option<u8> {
    let plan = layout.facility;
    plan.pylons
        .iter()
        .position(|pylon| *pylon == position)
        .map(|index| plan.order[index] + 1)
}

fn paint_glyph(canvas: &mut TileCanvas, seed: u32) {
    // A carved angular glyph that glows.
    let strokes = 3 + seed % 3;
    for stroke in 0..strokes {
        let h = hash(seed as i32, stroke as i32, 717);
        let (mut x, mut y) = (3 + (h % 10) as i32, 3 + ((h >> 4) % 10) as i32);
        let (dx, dy) = match (h >> 8) % 4 {
            0 => (1, 0),
            1 => (0, 1),
            2 => (1, 1),
            _ => (1, -1),
        };
        for _ in 0..5 {
            if (1..TILE_ART - 1).contains(&x) && (1..TILE_ART - 1).contains(&y) {
                canvas.set(x, y, GLYPH[2]);
            }
            x += dx;
            y += dy;
        }
    }
}

fn paint_pylon(canvas: &mut TileCanvas, lit: bool, dots: Option<u8>) {
    let (edge, body, glow) = if lit {
        (GLYPH[1], GLYPH[2], GLYPH[3])
    } else {
        (index_of(SLATE), index_of(SLATE_DARK), GLYPH[2])
    };
    let ink = index_of(INK);
    for py in 0..TILE_ART {
        let half = 4 + py / 4;
        for px in 8 - half..8 + half {
            let color = if px == 8 - half || px == 8 + half - 1 {
                ink
            } else if px < 8 {
                edge
            } else {
                body
            };
            canvas.set(px, py, color);
        }
    }
    // The order marks: dots down the pylon's face.
    let count = i32::from(dots.unwrap_or(0));
    for dot in 0..count {
        let y = 3 + dot * 4;
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            canvas.set(7 + dx, y + dy, glow);
        }
        canvas.set(6, y, index_of(INK));
        canvas.set(9, y + 1, index_of(INK));
    }
    if lit {
        canvas.set(7, 0, GLYPH[3]);
        canvas.set(8, 0, GLYPH[3]);
    }
}

fn paint_gate(canvas: &mut TileCanvas) {
    for py in 0..TILE_ART {
        for px in [2, 3, 7, 8, 12, 13] {
            canvas.set(px, py, if px % 5 == 2 { GLYPH[1] } else { GLYPH[0] });
        }
        if py % 8 == 4 {
            for px in 0..TILE_ART {
                canvas.set(px, py, GLYPH[1]);
            }
        }
    }
}

fn paint_conduit(canvas: &mut TileCanvas) {
    let coil = [
        index_of(MAROON),
        index_of(ORANGE),
        index_of(AMBER),
        index_of(GOLD),
    ];
    for py in 1..TILE_ART - 1 {
        for px in 4..12 {
            let level = ((py + px / 3) % 4) as usize;
            canvas.set(px, py, coil[level]);
        }
        canvas.set(3, py, index_of(INK));
        canvas.set(12, py, index_of(INK));
    }
    for px in 5..11 {
        canvas.set(px, 7, index_of(CREAM));
    }
}

fn paint_seal(canvas: &mut TileCanvas) {
    let violet = [
        index_of(PLUM),
        index_of(VIOLET),
        index_of(LAVENDER),
        index_of(PINK),
    ];
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let d = (px - 8).abs() + (py - 8).abs();
            if d % 5 == 0 {
                canvas.set(px, py, violet[(d / 5).clamp(0, 3) as usize]);
            }
        }
    }
}

fn paint_cache(canvas: &mut TileCanvas) {
    let ink = index_of(INK);
    for py in 4..13 {
        for px in 3..13 {
            let edge = py == 4 || py == 12 || px == 3 || px == 12;
            canvas.set(px, py, if edge { ink } else { index_of(STONE_DARK) });
        }
    }
    for px in 5..11 {
        canvas.set(px, 8, index_of(GOLD));
    }
    canvas.set(7, 6, index_of(MINT));
    canvas.set(8, 6, index_of(MINT));
    canvas.set(7, 10, index_of(MINT));
    canvas.set(8, 10, index_of(MINT));
}

fn paint_pad_stripes(canvas: &mut TileCanvas) {
    for px in 0..TILE_ART {
        let stripe = (canvas.origin_x + px).rem_euclid(8) < 4;
        let color = if stripe {
            index_of(GOLD)
        } else {
            index_of(INK)
        };
        canvas.set(px, 0, color);
        canvas.set(px, 1, color);
        canvas.set(px, 2, HULL[0]);
    }
}

fn paint_veil(canvas: &mut TileCanvas) {
    // Shimmering vertical strands over the open doorway (mostly see-through).
    for px in 0..TILE_ART {
        let strand = (canvas.origin_x + px).rem_euclid(5);
        if strand > 1 {
            continue;
        }
        for py in 0..TILE_ART {
            if (canvas.origin_y + py + px * 3).rem_euclid(7) < 4 {
                canvas.set(px, py, if strand == 0 { GLYPH[3] } else { GLYPH[2] });
            }
        }
    }
}

fn paint_stalk(canvas: &mut TileCanvas) {
    // A faceted crystal trunk: a lit face, a shadowed face and a bright seam between.
    let ink = index_of(INK);
    for py in 0..TILE_ART {
        let wobble = ((canvas.origin_y + py) / 6).rem_euclid(2);
        let (left, right) = (4 + wobble, 12 + wobble);
        for px in left..right {
            let color = if px == left || px == right - 1 {
                ink
            } else if px == left + 3 {
                CRYSTAL[3]
            } else if px < left + 3 {
                CRYSTAL[2]
            } else {
                CRYSTAL[1]
            };
            canvas.set(px, py, color);
        }
        if (canvas.origin_y + py).rem_euclid(7) == 0 {
            // Growth rings.
            for px in left + 1..right - 1 {
                canvas.set(px, py, CRYSTAL[0]);
            }
        }
    }
}

fn paint_bloom(canvas: &mut TileCanvas, seed: u32) {
    // A glowing faceted bloom: a star of petals around a white heart.
    let ink = index_of(INK);
    let r = 6 + (seed % 2) as i32;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (dx, dy) = (px - 8, py - 8);
            let d = dx.abs() + dy.abs();
            let star = dx.abs().min(dy.abs()) <= 1;
            if d <= r || (star && d <= r + 2) {
                let edge = d == r || (star && d == r + 2);
                let color = if edge {
                    ink
                } else if d <= 1 {
                    BLOOM[3]
                } else if d <= 3 {
                    BLOOM[2]
                } else {
                    BLOOM[1]
                };
                canvas.set(px, py, color);
            }
        }
    }
}

/// Paints the back wall of an air tile inside the Moon (a lava tube, the facility's rooms, the
/// colony's rooms). Returns `false` for air that is open sky.
pub(super) fn paint_lunar_air(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
) -> bool {
    let Some(layout) = layout(terrain.settings()) else {
        return false;
    };
    let (x, y) = (position.x, position.y);
    let back: [PaletteIndex; 2] = if layout.colony_pressurized(x, y) {
        if y < layout.colony.ground_row - 5 && (x - layout.colony.center_x).abs() <= 16 {
            // Under the dome's glass: open to the stars.
            return true;
        }
        [index_of(STEEL_DARK), index_of(STEEL)]
    } else if layout
        .facility
        .rooms()
        .iter()
        .any(|room| room.contains(position))
        || layout.in_core_chamber(x, y)
    {
        // The facility's inner walls: dark ancient courses with faint teal seams.
        for py in 0..TILE_ART {
            for px in 0..TILE_ART {
                let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
                let course = gy.div_euclid(TILE_ART * 2);
                let bx = (gx + course * TILE_ART).rem_euclid(TILE_ART * 4);
                let by = gy.rem_euclid(TILE_ART * 2);
                let color = if bx == 0 || by == 0 {
                    index_of(TEAL_DARK)
                } else if hash(gx, gy, 721).is_multiple_of(61) {
                    index_of(SLATE_DARK)
                } else if bx == 1 || by == 1 {
                    index_of(NIGHT)
                } else {
                    index_of(INK)
                };
                canvas.set(px, py, color);
            }
        }
        if layout.in_core_chamber(x, y) {
            paint_core_light(canvas, &layout, x, y);
        }
        return true;
    } else if inside_body(&layout, x, y) {
        [index_of(NIGHT), index_of(INK)]
    } else {
        return false;
    };
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            if layout.colony_pressurized(x, y) {
                // Panelled walls, a warm strip light along the top of the room.
                let (lx, ly) = (gx.rem_euclid(TILE_ART * 2), gy.rem_euclid(TILE_ART * 5));
                let color = if ly == 2 && lx > 3 && lx < 28 {
                    index_of(CREAM)
                } else if lx == 0 || ly == 0 {
                    index_of(NIGHT)
                } else {
                    back[usize::from(lx == 1 || ly == 1)]
                };
                canvas.set(px, py, color);
                continue;
            }
            let level = usize::from(dirt_level(gx, gy) < 2);
            let speck = hash(gx, gy, 720).is_multiple_of(53);
            canvas.set(px, py, if speck { index_of(PLUM) } else { back[level] });
        }
    }
    true
}

/// The core's light on its chamber's walls: motes of teal near the heart.
fn paint_core_light(canvas: &mut TileCanvas, layout: &MoonLayout, x: i32, y: i32) {
    let (dx, dy) = (x - layout.core.x, y - layout.core.y);
    if dx * dx + dy * dy >= 40 {
        return;
    }
    for py in (0..TILE_ART).step_by(3) {
        for px in (0..TILE_ART).step_by(3) {
            if hash(canvas.origin_x + px, canvas.origin_y + py, 718).is_multiple_of(3) {
                canvas.set(px, py, GLYPH[0]);
            }
        }
    }
}

/// Whether `(x, y)` lies inside the Moon's body (under its surface, above its underside).
fn inside_body(layout: &MoonLayout, x: i32, y: i32) -> bool {
    layout
        .surface_row(x)
        .is_some_and(|surface| y > surface && y < layout.moon.raw_bottom(x, surface))
}

/// Whether air at `(x, y)` is enclosed by the Moon (a lava tube, a facility room, the colony's
/// rooms): the terrain art treats it like a cave, not the open sky.
#[must_use]
pub fn lunar_enclosed(terrain: &Terrain, x: i32, y: i32) -> bool {
    let settings = terrain.settings();
    if !settings.generator.has_moon_content() || y > 0 {
        return false;
    }
    let Some(layout) = layout(settings) else {
        return false;
    };
    layout.colony_pressurized(x, y)
        || layout.facility.temple_room.contains(TilePosition { x, y })
        || inside_body(&layout, x, y)
}
