//! The builders' buried city's tiles and back walls (Wave 10 ancients-a, Descent VII strata
//! 1-3, `fatherload_simulation::ancient_city`).
//!
//! * Buried Megacity: city stone in big pale blocks with incised seams; tower glass as dark
//!   teal panes, some lit warm from inside; tram rails (a steel rail on dark sleepers) and
//!   platforms with a yellow edge stripe; hanging street lamps; alloy ore (copper and teal
//!   flecks); lift rails; the archivist's core (a pedestal with a cyan eye). The back wall is a
//!   night city: far facades with lit windows; inside a tower, panelled walls.
//! * Engine Halls: riveted dark steel with rust; belts with chevrons; piston housings in
//!   hazard stripes; engine core ore glowing orange; copper pipes. The back wall is dark
//!   plating with vertical pipes and gauges.
//! * The Archive of Stars: shelving full of book spines; deep blue archive stone with tiny
//!   stars; glowing star glass; star chart plinths (a gold star disc on a pedestal, dim once
//!   read). The back wall is a starry blue with shelf silhouettes.

use fatherload_simulation::{
    ancient_city::CityTile,
    descent::Stratum,
    terrain::{Terrain, TilePosition},
    worldgen::descent::ancient_city::{self as sites, CityStampKind},
};

use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{Cell, Edges, Ramp, Strata, TILE_ART, TileCanvas, hash, noise, paint_edges, ramp};

const STONE_RAMP: Ramp = ramp([STONE_DARK, CLAY, SAND, CREAM]);
const GLASS_RAMP: Ramp = ramp([NIGHT, TEAL_DARK, TEAL, MINT]);
const STEEL_RAMP: Ramp = ramp([INK, STEEL_DARK, STEEL, STEEL_LIGHT]);
const HAZARD_RAMP: Ramp = ramp([INK, AMBER, GOLD, CREAM]);
const ARCHIVE_RAMP: Ramp = ramp([NIGHT, DEEP_BLUE, OCEAN, SKY_LIGHT]);
const SHELF_RAMP: Ramp = ramp([SOIL_DARK, SOIL, LOAM, AMBER]);
const STAR_RAMP: Ramp = ramp([OCEAN, SKY, SKY_LIGHT, ICE]);

/// The buried city's stratum at `position` (`None` elsewhere or on older worlds).
#[must_use]
pub(super) fn city_stratum(terrain: &Terrain, position: TilePosition) -> Option<Stratum> {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return None;
    }
    let stratum = terrain.stratum_at(position);
    sites::is_city(stratum).then_some(stratum)
}

/// Representative colour of a city tile (drill dust, debris, the map).
#[must_use]
pub const fn city_color(tile: CityTile) -> Color {
    match tile {
        CityTile::CityStone => SAND,
        CityTile::TowerGlass => TEAL,
        CityTile::TramRail | CityTile::LiftRail | CityTile::EngineSteel => STEEL,
        CityTile::Platform | CityTile::PistonHousing | CityTile::ChartPlinth => GOLD,
        CityTile::StreetLamp => AMBER,
        CityTile::AlloyOre | CityTile::Pipe => COPPER,
        CityTile::ArchivistCore => MINT,
        CityTile::ConveyorLeft | CityTile::ConveyorRight => STEEL_DARK,
        CityTile::EngineCoreOre => ORANGE,
        CityTile::ArchiveShelf => LOAM,
        CityTile::ArchiveStone => DEEP_BLUE,
        CityTile::StarGlass => SKY_LIGHT,
        CityTile::ChartPlinthRead => STONE,
    }
}

/// The lip and shade a city tile's open edges take.
const fn edge_colours(tile: CityTile) -> (Ramp, bool) {
    match tile {
        CityTile::CityStone | CityTile::StreetLamp | CityTile::Platform => (STONE_RAMP, true),
        CityTile::TowerGlass => (GLASS_RAMP, true),
        CityTile::TramRail
        | CityTile::LiftRail
        | CityTile::EngineSteel
        | CityTile::ConveyorLeft
        | CityTile::ConveyorRight
        | CityTile::Pipe => (STEEL_RAMP, true),
        CityTile::PistonHousing => (HAZARD_RAMP, true),
        CityTile::ArchiveShelf => (SHELF_RAMP, true),
        CityTile::ArchiveStone | CityTile::ChartPlinth | CityTile::ChartPlinthRead => {
            (ARCHIVE_RAMP, true)
        }
        CityTile::StarGlass => (STAR_RAMP, false),
        CityTile::AlloyOre | CityTile::EngineCoreOre | CityTile::ArchivistCore => {
            (STEEL_RAMP, false)
        }
    }
}

/// One art pixel of `tile` at absolute art pixel `(x, y)` (`(lx, ly)` within the tile).
#[allow(clippy::too_many_lines, reason = "one arm per tile")]
fn body_pixel(tile: CityTile, x: i32, y: i32, lx: i32, ly: i32) -> PaletteIndex {
    match tile {
        CityTile::CityStone => {
            // Big blocks (16 x 8), offset every row, incised seams and a worn top edge.
            let row = y.div_euclid(8);
            let shift = if row.rem_euclid(2) == 0 { 0 } else { 8 };
            let (bx, by) = ((x + shift).rem_euclid(16), y.rem_euclid(8));
            if bx == 0 || by == 0 {
                STONE_RAMP[0]
            } else if by == 1 {
                STONE_RAMP[3]
            } else if hash(x, y, 1_101).is_multiple_of(41) {
                STONE_RAMP[0]
            } else if noise(x, y, 6, 1_103) + f32::from(bayer4(x, y)) / 40.0 > 0.7 {
                STONE_RAMP[1]
            } else {
                STONE_RAMP[2]
            }
        }
        CityTile::TowerGlass => {
            // Window panes in a dark frame; about a third of the windows are lit.
            let window = hash(x.div_euclid(8), y.div_euclid(8), 1_105);
            let (wx, wy) = (x.rem_euclid(8), y.rem_euclid(8));
            if wx == 0 || wy == 0 {
                index_of(INK)
            } else if window.is_multiple_of(3) {
                // Lit: warm, a figure-less glow, brighter in the middle.
                if (2..=5).contains(&wx) && (2..=5).contains(&wy) {
                    index_of(CREAM)
                } else {
                    index_of(AMBER)
                }
            } else if wx + wy == 4 {
                GLASS_RAMP[3]
            } else if wy >= 6 {
                GLASS_RAMP[0]
            } else {
                GLASS_RAMP[1]
            }
        }
        CityTile::TramRail => {
            // The top four rows: the steel rail; under it sleepers on gravel.
            if ly <= 1 {
                STEEL_RAMP[3]
            } else if ly <= 3 {
                STEEL_RAMP[2]
            } else if ly == 4 {
                STEEL_RAMP[0]
            } else if x.rem_euclid(8) < 4 && ly <= 8 {
                index_of(SOIL)
            } else if hash(x, y, 1_107).is_multiple_of(3) {
                index_of(STONE)
            } else {
                index_of(STONE_DARK)
            }
        }
        CityTile::Platform => {
            // Paving with a yellow safety stripe along its top edge.
            if ly <= 1 {
                index_of(GOLD)
            } else if ly == 2 {
                if x.rem_euclid(6) < 3 {
                    index_of(INK)
                } else {
                    index_of(GOLD)
                }
            } else if lx == 0 || ly == 9 {
                index_of(STONE_DARK)
            } else if hash(x, y, 1_109).is_multiple_of(19) {
                index_of(STONE_LIGHT)
            } else {
                index_of(STONE)
            }
        }
        CityTile::StreetLamp => {
            // Ceiling stone with a lamp hanging from a bracket: a warm globe.
            let (dx, dy) = (lx - 8, ly - 10);
            if ly <= 3 {
                STONE_RAMP[2 - usize::from(ly == 3)]
            } else if (7..=8).contains(&lx) && ly <= 6 {
                index_of(STEEL_DARK)
            } else if dx * dx + dy * dy <= 10 {
                if dx * dx + dy * dy <= 3 {
                    index_of(CREAM)
                } else {
                    index_of(GOLD)
                }
            } else if dx * dx + dy * dy <= 18 && ly >= 7 {
                index_of(AMBER)
            } else {
                index_of(NIGHT)
            }
        }
        CityTile::AlloyOre => {
            // Dark host rock with copper and teal flecks along a seam.
            let seam = noise(x, y, 5, 1_111) + f32::from(bayer4(x, y)) / 50.0;
            if seam > 0.8 {
                index_of(if hash(x, y, 1_113).is_multiple_of(3) {
                    TEAL
                } else {
                    COPPER
                })
            } else if seam > 0.7 {
                index_of(RUST)
            } else if noise(x, y, 9, 1_115) > 0.55 {
                index_of(STONE_DARK)
            } else {
                index_of(SLATE_DARK)
            }
        }
        CityTile::LiftRail => {
            // A guide rail: two steel uprights with rungs.
            if lx == 6 || lx == 9 {
                STEEL_RAMP[2]
            } else if (lx == 7 || lx == 8) && ly.rem_euclid(4) == 0 {
                STEEL_RAMP[3]
            } else if lx == 5 || lx == 10 {
                STEEL_RAMP[0]
            } else {
                index_of(NIGHT)
            }
        }
        CityTile::ArchivistCore => {
            // A pedestal with a cyan eye glowing in its housing.
            let (dx, dy) = (lx - 8, ly - 6);
            let r = dx * dx + dy * dy;
            if r <= 4 {
                index_of(if r <= 1 { ICE } else { MINT })
            } else if r <= 12 {
                index_of(TEAL)
            } else if r <= 20 {
                STEEL_RAMP[0]
            } else if ly >= 12 {
                STEEL_RAMP[usize::from(lx > 2 && lx < 13) + 1]
            } else {
                STEEL_RAMP[1]
            }
        }
        CityTile::EngineSteel => {
            // Riveted plates (16 x 16) with rust bleeding from the seams.
            let (px, py) = (x.rem_euclid(16), y.rem_euclid(16));
            if px == 0 || py == 0 {
                STEEL_RAMP[0]
            } else if (px == 2 || px == 13) && (py == 2 || py == 13) {
                STEEL_RAMP[3]
            } else if py >= 14 && hash(x, 0, 1_117).is_multiple_of(3) {
                index_of(RUST)
            } else if py <= 1 {
                STEEL_RAMP[2]
            } else {
                STEEL_RAMP[1]
            }
        }
        CityTile::ConveyorLeft | CityTile::ConveyorRight => {
            // A dark belt with chevrons pointing its way, rollers under it.
            let dir = if tile == CityTile::ConveyorLeft {
                -1
            } else {
                1
            };
            if ly <= 1 {
                index_of(STEEL_DARK)
            } else if ly <= 6 {
                let cx = (x * dir).rem_euclid(8);
                if cx == ly - 1 || cx == 7 - (ly - 1) {
                    index_of(GOLD)
                } else {
                    index_of(INK)
                }
            } else if ly <= 9 {
                let roller = x.rem_euclid(8);
                if roller == 3 || roller == 4 {
                    STEEL_RAMP[3]
                } else {
                    STEEL_RAMP[1]
                }
            } else {
                STEEL_RAMP[usize::from(lx.rem_euclid(5) != 0)]
            }
        }
        CityTile::PistonHousing => {
            // Hazard stripes on a heavy housing.
            if ly <= 1 || ly >= 14 {
                STEEL_RAMP[2]
            } else if (x + y).rem_euclid(8) < 4 {
                HAZARD_RAMP[2]
            } else {
                HAZARD_RAMP[0]
            }
        }
        CityTile::EngineCoreOre => {
            // Dark rock with glowing orange cores.
            let core = hash(x.div_euclid(6), y.div_euclid(6), 1_119);
            let (cx, cy) = (x.rem_euclid(6) - 3, y.rem_euclid(6) - 3);
            if core.is_multiple_of(2) && cx * cx + cy * cy <= 2 {
                index_of(if cx * cx + cy * cy == 0 {
                    CREAM
                } else {
                    ORANGE
                })
            } else if core.is_multiple_of(2) && cx * cx + cy * cy <= 5 {
                index_of(RUST)
            } else if noise(x, y, 8, 1_121) > 0.6 {
                index_of(STEEL_DARK)
            } else {
                index_of(NIGHT)
            }
        }
        CityTile::Pipe => {
            // A horizontal copper pipe with a flange, in dark rock.
            if (4..=11).contains(&ly) {
                if lx.rem_euclid(16) == 0 || lx == 1 {
                    index_of(RUST)
                } else if ly <= 5 {
                    index_of(GOLD)
                } else if ly >= 10 {
                    index_of(RUST)
                } else {
                    index_of(COPPER)
                }
            } else if noise(x, y, 7, 1_123) > 0.55 {
                index_of(STONE_DARK)
            } else {
                index_of(SLATE_DARK)
            }
        }
        CityTile::ArchiveShelf => {
            // Shelves of book spines in a dark wooden frame.
            let shelf = ly.rem_euclid(8);
            if shelf == 0 || lx == 0 || lx == 15 {
                SHELF_RAMP[0]
            } else if shelf == 7 {
                SHELF_RAMP[1]
            } else {
                let spine = hash(x.div_euclid(2), y.div_euclid(8), 1_125);
                let colour = match spine % 7 {
                    0 => MAROON,
                    1 => DEEP_BLUE,
                    2 => PINE,
                    3 => RUST,
                    4 => PLUM,
                    5 => CREAM,
                    _ => SOIL,
                };
                if shelf == 1 && spine.is_multiple_of(5) {
                    SHELF_RAMP[0]
                } else if x.rem_euclid(2) == 0 && shelf == 3 {
                    index_of(GOLD)
                } else {
                    index_of(colour)
                }
            }
        }
        CityTile::ArchiveStone => {
            // Deep blue blocks with tiny stars set in them.
            let row = y.div_euclid(8);
            let shift = if row.rem_euclid(2) == 0 { 0 } else { 6 };
            let (bx, by) = ((x + shift).rem_euclid(12), y.rem_euclid(8));
            if bx == 0 || by == 0 {
                ARCHIVE_RAMP[0]
            } else if hash(x, y, 1_127).is_multiple_of(53) {
                index_of(CREAM)
            } else if by == 1 {
                ARCHIVE_RAMP[2]
            } else {
                ARCHIVE_RAMP[1]
            }
        }
        CityTile::StarGlass => {
            // Glowing pale crystal facets with a four-pointed sparkle.
            let facet = (x - y).rem_euclid(9);
            let sparkle = hash(x.div_euclid(8), y.div_euclid(8), 1_129);
            let (sx, sy) = (x.rem_euclid(8) - 4, y.rem_euclid(8) - 4);
            if sparkle.is_multiple_of(3)
                && ((sx == 0 && sy.abs() <= 2) || (sy == 0 && sx.abs() <= 2))
            {
                index_of(BONE)
            } else if facet == 0 {
                STAR_RAMP[0]
            } else if facet <= 3 {
                STAR_RAMP[3]
            } else if facet <= 6 {
                STAR_RAMP[2]
            } else {
                STAR_RAMP[1]
            }
        }
        CityTile::ChartPlinth | CityTile::ChartPlinthRead => {
            // A pedestal; on it a disc of light with a star.
            let lit = tile == CityTile::ChartPlinth;
            let (dx, dy) = (lx - 8, ly - 4);
            let r = dx * dx + dy * dy * 3;
            if ly <= 7 && r <= 22 {
                if dx == 0 && dy.abs() <= 2 || dy == 0 && dx.abs() <= 3 {
                    index_of(if lit { BONE } else { STONE_LIGHT })
                } else {
                    index_of(if lit { GOLD } else { STONE })
                }
            } else if ly >= 8 && (4..=11).contains(&lx) {
                if lx == 4 || ly == 15 {
                    ARCHIVE_RAMP[0]
                } else if lx == 5 {
                    ARCHIVE_RAMP[2]
                } else {
                    ARCHIVE_RAMP[1]
                }
            } else {
                index_of(NIGHT)
            }
        }
    }
}

/// Paints city tile `tile` at `position`.
pub(super) fn paint(canvas: &mut TileCanvas, tile: CityTile, strata: &Strata, edges: Edges) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, body_pixel(tile, gx, gy, px, py));
        }
    }
    // Lamps, plinths, cores and pipes keep their shapes; the rest take the usual rims.
    if matches!(
        tile,
        CityTile::StreetLamp
            | CityTile::ArchivistCore
            | CityTile::ChartPlinth
            | CityTile::ChartPlinthRead
            | CityTile::LiftRail
    ) {
        return;
    }
    let (rim, square) = edge_colours(tile);
    paint_edges(canvas, strata, edges, rim[3], rim[0], square);
}

/// What set piece the open ground at `position` belongs to (its back wall), if any.
fn stamp_at(terrain: &Terrain, position: TilePosition) -> Option<CityStampKind> {
    let settings = terrain.settings();
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    sites::stamps_with(
        settings,
        &layout,
        &fatherload_simulation::worldgen::TileRect {
            min: position,
            max: position,
        },
    )
    .into_iter()
    .find(|stamp| stamp.bounds.contains(position))
    .map(|stamp| stamp.kind)
}

/// One art pixel of a back wall at `(x, y)`.
#[allow(clippy::too_many_lines, reason = "one arm per wall")]
fn back_pixel(stratum: Stratum, stamp: Option<CityStampKind>, x: i32, y: i32) -> PaletteIndex {
    match (stratum, stamp) {
        (_, Some(CityStampKind::Tower | CityStampKind::LanternLibrary)) => {
            // Inside a tower: panelled walls, a wainscot, warm light.
            let (px, py) = (x.rem_euclid(24), y.rem_euclid(48));
            if px == 0 {
                index_of(SOIL_DARK)
            } else if py >= 36 {
                if py == 36 {
                    index_of(AMBER)
                } else {
                    index_of(SOIL)
                }
            } else if px == 1 || py == 0 {
                index_of(LOAM)
            } else if noise(x, y, 10, 1_131) > 0.7 {
                index_of(SOIL)
            } else {
                index_of(SOIL_DARK)
            }
        }
        (_, Some(CityStampKind::Avenue)) => {
            // A street at night: far facades of the city, lit windows, a dark sky of rock.
            let building = x.div_euclid(40);
            let roof = 8 + (hash(building, 0, 1_133) % 40) as i32;
            let level = y.rem_euclid(112);
            if level < roof {
                return if hash(x, y, 1_135).is_multiple_of(97) {
                    index_of(SLATE)
                } else {
                    index_of(INK)
                };
            }
            let (wx, wy) = (x.rem_euclid(6), level.rem_euclid(8));
            let lit = hash(x.div_euclid(6), level.div_euclid(8) + building * 31, 1_137);
            if x.rem_euclid(40) == 0 {
                index_of(INK)
            } else if (2..=4).contains(&wx) && (2..=5).contains(&wy) {
                if lit.is_multiple_of(4) {
                    index_of(AMBER)
                } else if lit.is_multiple_of(7) {
                    index_of(TEAL_DARK)
                } else {
                    index_of(SLATE_DARK)
                }
            } else {
                index_of(NIGHT)
            }
        }
        (
            _,
            Some(
                CityStampKind::EngineHall
                | CityStampKind::PistonShaft
                | CityStampKind::CrusherLine
                | CityStampKind::GreatEngine,
            ),
        ) => {
            // The halls' plating: big dark panels, vertical pipes, a gauge now and then.
            let (px, py) = (x.rem_euclid(32), y.rem_euclid(32));
            let pipe = x.rem_euclid(48);
            if pipe == 10 || pipe == 11 {
                index_of(if pipe == 10 { COPPER } else { RUST })
            } else if px == 0 || py == 0 {
                index_of(INK)
            } else if (px - 16).abs() <= 2
                && (py - 12).abs() <= 2
                && hash(x.div_euclid(32), y.div_euclid(32), 1_139).is_multiple_of(5)
            {
                index_of(if px == 16 && py <= 12 { RED } else { CREAM })
            } else if py <= 1 {
                index_of(STEEL_DARK)
            } else {
                index_of(NIGHT)
            }
        }
        (_, Some(CityStampKind::Stacks | CityStampKind::Orrery | CityStampKind::Vault(_))) => {
            // The Archive's halls: a starry dome of deep blue.
            if hash(x, y, 1_141).is_multiple_of(61) {
                index_of(if hash(x, y, 1_143).is_multiple_of(3) {
                    CREAM
                } else {
                    SKY_LIGHT
                })
            } else if noise(x, y, 14, 1_145) > 0.7 {
                index_of(DEEP_BLUE)
            } else {
                index_of(NIGHT)
            }
        }
        (Stratum::BuriedMegacity, _) => {
            // Buried rubble of the city: dark stone with brick courses showing through.
            let course = y.rem_euclid(6) == 0 && noise(x, y, 12, 1_147) > 0.5;
            if course {
                index_of(SOIL_DARK)
            } else if noise(x, y, 9, 1_149) > 0.65 {
                index_of(SLATE_DARK)
            } else {
                index_of(INK)
            }
        }
        (Stratum::EngineHalls, _) => {
            // Oily dark rock with old cable runs.
            if y.rem_euclid(37) == 5 {
                index_of(STEEL_DARK)
            } else if noise(x, y, 10, 1_151) > 0.66 {
                index_of(RUST)
            } else {
                index_of(NIGHT)
            }
        }
        _ => {
            // The Archive's rock: deep blue-black, a far star.
            if hash(x, y, 1_153).is_multiple_of(173) {
                index_of(SKY_LIGHT)
            } else if noise(x, y, 11, 1_155) > 0.62 {
                index_of(DEEP_BLUE)
            } else {
                index_of(NIGHT)
            }
        }
    }
}

/// Paints the open ground at `position` in a city stratum: its back wall, shadowed under rock.
pub(super) fn paint_back(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    stratum: Stratum,
    up: Cell,
    left: Cell,
    right: Cell,
) {
    let stamp = stamp_at(terrain, position);
    let shade = index_of(INK);
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
                back_pixel(stratum, stamp, gx, gy)
            };
            canvas.set(px, py, index);
        }
    }
}
