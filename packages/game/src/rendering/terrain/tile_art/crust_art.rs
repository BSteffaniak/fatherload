//! Tile art for the authored Crust (Wave 8, `fatherload_simulation::crust`).
//!
//! Root mats are the Topsoil's dirt shot through with pale, knotted roots; mine timber is
//! adzed planks with iron straps and bolt heads; a rail bed carries a polished steel rail on
//! tarred sleepers over ballast; slate shelves are blue-grey sheets with a wet sheen on their
//! top face; basalt splits into hexagonal columns with lit facets; lava tubes are lined with
//! ropy black glass that still glows red in its cracks. The lost miner's cabin is round logs,
//! strongboxes are iron-banded chests with a brass lock, the Sinkhole's core is a humming teal
//! stone, collapses leave broken rubble and Gravel Hollow's street is packed gravel.
//!
//! Open tiles get Crust-only backdrops too: the posts and lanterns of a timber set behind a
//! mine gallery, and the warm, cracked walls of a lava tube. Clay and dirt beside an aquifer
//! show a damp stain (the tell that water is behind it).

use fatherload_simulation::{
    crust::{self, CrustTile},
    terrain::{Terrain, TileKind, TilePosition},
};
use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{
    Edges, Ramp, Strata, TILE_ART, TileCanvas, backdrop_pixel, hash, noise, paint_edges, ramp,
};

/// Adzed mine timber.
const TIMBER: Ramp = ramp([SOIL_DARK, SOIL, LOAM, SAND]);
/// Cabin logs (warm, oiled pine gone dark with age).
const LOG: Ramp = ramp([SOIL_DARK, RUST, COPPER, SAND]);
/// Rain-soaked mud.
const MUD: Ramp = ramp([INK, NIGHT, SOIL_DARK, SOIL]);
/// Slate sheets.
const SLATE_SHEET: Ramp = ramp([NIGHT, SLATE_DARK, SLATE, SLATE_LIGHT]);
/// Basalt columns.
const BASALT_COLUMN: Ramp = ramp([INK, NIGHT, STONE_DARK, STONE]);
/// Tube glass.
const TUBE: Ramp = ramp([INK, NIGHT, MAROON, RUST]);
/// Rubble.
const RUBBLE: Ramp = ramp([NIGHT, STONE_DARK, STONE, STONE_LIGHT]);
/// Gravel.
const GRAVEL: Ramp = ramp([SOIL_DARK, STONE_DARK, STONE, STONE_LIGHT]);

/// Representative colour of a Crust tile (drill dust, the minimap, world previews).
#[must_use]
pub const fn crust_color(tile: CrustTile) -> Color {
    match tile {
        CrustTile::RootMat => LOAM,
        CrustTile::Timber | CrustTile::CabinLog => SOIL,
        CrustTile::Rail => STEEL,
        CrustTile::Slate => SLATE,
        CrustTile::BasaltColumn => STONE_DARK,
        CrustTile::TubeGlass => MAROON,
        CrustTile::Strongbox => COPPER,
        CrustTile::SinkholeCore => MINT,
        CrustTile::Rubble => STONE,
        CrustTile::Gravel => STONE_LIGHT,
    }
}

/// The glow of a lit Crust tile (halos, the light overlay's tint).
#[must_use]
pub const fn crust_glow(tile: CrustTile) -> Option<Color> {
    match tile {
        CrustTile::SinkholeCore => Some(MINT),
        CrustTile::TubeGlass => Some(ORANGE),
        _ => None,
    }
}

fn kind_at(terrain: &Terrain, x: i32, y: i32) -> Option<TileKind> {
    terrain.tile(TilePosition { x, y }).map(|tile| tile.kind)
}

/// Paints the Crust tile `tile` at `position`.
pub(super) fn paint(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: CrustTile,
    strata: &Strata,
    edges: Edges,
) {
    let seed = hash(position.x, position.y, 811);
    match tile {
        CrustTile::Strongbox => {
            paint_back(canvas, strata);
            paint_strongbox(canvas);
            return;
        }
        CrustTile::SinkholeCore => {
            paint_back(canvas, strata);
            paint_core(canvas, seed);
            return;
        }
        _ => {}
    }
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
            let index = match tile {
                CrustTile::RootMat => root_pixel(strata, x, y),
                CrustTile::Timber => timber_pixel(x, y, px, py),
                CrustTile::Rail => rail_bed_pixel(strata, x, y, py),
                CrustTile::Slate => slate_pixel(x, y),
                CrustTile::BasaltColumn => basalt_pixel(x, y),
                CrustTile::TubeGlass => tube_pixel(x, y),
                CrustTile::CabinLog => log_pixel(x, y, py),
                CrustTile::Rubble => rubble_pixel(x, y),
                CrustTile::Gravel => gravel_pixel(x, y),
                CrustTile::Strongbox | CrustTile::SinkholeCore => strata.stone[2],
            };
            canvas.set(px, py, index);
        }
    }
    match tile {
        CrustTile::Timber => paint_straps(canvas, terrain, position),
        CrustTile::Rail => paint_rail(canvas, terrain, position),
        CrustTile::Slate if edges.up.is_open() => paint_sheen(canvas, seed),
        _ => {}
    }
    let material: Ramp = match tile {
        CrustTile::RootMat => strata.dirt,
        CrustTile::Timber => TIMBER,
        CrustTile::CabinLog => LOG,
        CrustTile::Slate => SLATE_SHEET,
        CrustTile::BasaltColumn => BASALT_COLUMN,
        CrustTile::TubeGlass => TUBE,
        CrustTile::Rubble => RUBBLE,
        CrustTile::Gravel => GRAVEL,
        CrustTile::Rail | CrustTile::Strongbox | CrustTile::SinkholeCore => strata.stone,
    };
    // Timber and logs are dressed: square corners.
    let square = matches!(
        tile,
        CrustTile::Timber | CrustTile::CabinLog | CrustTile::Rail
    );
    paint_edges(canvas, strata, edges, material[3], material[0], square);
}

fn paint_back(canvas: &mut TileCanvas, strata: &Strata) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, backdrop_pixel(strata, gx, gy));
        }
    }
}

/// Dirt shot through with pale knotted roots.
fn root_pixel(strata: &Strata, x: i32, y: i32) -> PaletteIndex {
    // Two families of wavy strands, one steep and one shallow.
    let wobble = (noise(x, y, 9, 821) * 10.0) as i32;
    let steep = (x * 3 + y + wobble).rem_euclid(17);
    let shallow = (x + y * 3 - wobble).rem_euclid(23);
    if steep == 0 || shallow == 0 {
        return index_of(if hash(x, y, 823).is_multiple_of(5) {
            BONE
        } else {
            SAND
        });
    }
    if steep == 1 || shallow == 1 {
        return index_of(LOAM);
    }
    let body = noise(x, y, 4, 825) + f32::from(bayer4(x, y)) / 50.0;
    if body > 0.72 {
        strata.dirt[3]
    } else if body > 0.45 {
        strata.dirt[2]
    } else if body > 0.2 {
        strata.dirt[1]
    } else {
        strata.dirt[0]
    }
}

/// Adzed planks running along the beam, with dark grain and knots.
fn timber_pixel(x: i32, y: i32, px: i32, py: i32) -> PaletteIndex {
    let plank = py.div_euclid(5);
    if py.rem_euclid(5) == 4 {
        return TIMBER[0];
    }
    let grain = noise(x * 3, y + plank * 31, 7, 831);
    if hash(x / 3, y, 833).is_multiple_of(41) {
        return TIMBER[0];
    }
    if py.rem_euclid(5) == 0 {
        return TIMBER[3];
    }
    if grain > 0.7 || (px + plank * 5).rem_euclid(11) == 0 {
        TIMBER[1]
    } else {
        TIMBER[2]
    }
}

/// Iron straps and bolt heads where a beam meets its neighbours (the ends of a set).
fn paint_straps(canvas: &mut TileCanvas, terrain: &Terrain, position: TilePosition) {
    let timber = |dx: i32| {
        matches!(
            kind_at(terrain, position.x + dx, position.y),
            Some(TileKind::Crust(CrustTile::Timber))
        )
    };
    for (open, column) in [(!timber(-1), 2), (!timber(1), TILE_ART - 4)] {
        if !open {
            continue;
        }
        for py in 1..TILE_ART - 1 {
            canvas.set(column, py, index_of(STEEL_DARK));
            canvas.set(column + 1, py, index_of(STEEL));
        }
        for py in [3, 11] {
            canvas.set(column, py, index_of(STEEL_LIGHT));
        }
    }
}

/// A rail bed: ballast and rock under the sleepers.
fn rail_bed_pixel(strata: &Strata, x: i32, y: i32, py: i32) -> PaletteIndex {
    if py < 6 {
        // Ballast between the sleepers.
        return if hash(x, y, 841).is_multiple_of(3) {
            GRAVEL[3]
        } else if hash(x, y, 843).is_multiple_of(2) {
            GRAVEL[2]
        } else {
            GRAVEL[1]
        };
    }
    let body = noise(x, y, 5, 845) + f32::from(bayer4(x, y)) / 50.0;
    if body > 0.66 {
        strata.stone[2]
    } else if body > 0.33 {
        strata.stone[1]
    } else {
        strata.stone[0]
    }
}

/// The rail and its sleepers on top of a rail bed; a ramp's rail climbs to the next bed.
fn paint_rail(canvas: &mut TileCanvas, terrain: &Terrain, position: TilePosition) {
    let rail = |dx: i32, dy: i32| {
        matches!(
            kind_at(terrain, position.x + dx, position.y + dy),
            Some(TileKind::Crust(CrustTile::Rail))
        )
    };
    // Sleepers every eight art pixels (anchored to the world grid).
    for px in 0..TILE_ART {
        let gx = canvas.origin_x + px;
        if gx.rem_euclid(8) < 3 {
            for py in 2..5 {
                canvas.set(
                    px,
                    py,
                    if py == 2 {
                        index_of(LOAM)
                    } else {
                        index_of(SOIL)
                    },
                );
            }
        }
    }
    // The rail: level, or sloping up to a higher neighbour / down to a lower one.
    let rises_left = rail(-1, -1) && !rail(-1, 0);
    let rises_right = rail(1, -1) && !rail(1, 0);
    for px in 0..TILE_ART {
        let lift = if rises_right && px >= TILE_ART / 2 {
            (px - TILE_ART / 2) * 2
        } else if rises_left && px < TILE_ART / 2 {
            (TILE_ART / 2 - px) * 2
        } else {
            0
        };
        let top = 0 - lift;
        if top >= 0 {
            canvas.set(px, top, index_of(STEEL_LIGHT));
            canvas.set(px, top + 1, index_of(STEEL_DARK));
        }
    }
}

/// Slate: thin blue-grey sheets with dark partings and rare silvery flecks.
fn slate_pixel(x: i32, y: i32) -> PaletteIndex {
    let offset = (noise(x, y / 3, 13, 851) * 3.0) as i32;
    let layer = (y + offset).rem_euclid(4);
    if layer == 0 {
        return SLATE_SHEET[0];
    }
    if hash(x, y, 853).is_multiple_of(79) {
        return index_of(SKY_LIGHT);
    }
    let body = noise(x, y, 11, 855) + f32::from(bayer4(x, y)) / 40.0;
    if layer == 1 && body > 0.5 {
        SLATE_SHEET[3]
    } else if body > 0.45 {
        SLATE_SHEET[2]
    } else {
        SLATE_SHEET[1]
    }
}

/// A wet sheen along the slick top face of a slate shelf.
fn paint_sheen(canvas: &mut TileCanvas, seed: u32) {
    for px in 1..TILE_ART - 1 {
        let gx = canvas.origin_x + px;
        if (gx + (seed % 7) as i32).rem_euclid(9) < 5 {
            canvas.set(px, 2, index_of(SKY_LIGHT));
        }
        if (gx * 3).rem_euclid(13) == 0 {
            canvas.set(px, 3, index_of(ICE));
        }
    }
}

/// Basalt: hexagonal columns split by black joints, their tops catching the light.
fn basalt_pixel(x: i32, y: i32) -> PaletteIndex {
    let width = 9;
    let column = x.div_euclid(width);
    let shift = if column.rem_euclid(2) == 0 { 0 } else { 6 };
    let (cx, cy) = (x.rem_euclid(width), (y + shift).rem_euclid(12));
    if cx == 0 {
        return BASALT_COLUMN[0];
    }
    if cy == 0 {
        return BASALT_COLUMN[0];
    }
    if cy == 1 && cx > 1 && cx < width - 1 {
        return BASALT_COLUMN[3];
    }
    if cx == 1 {
        return BASALT_COLUMN[3];
    }
    if cx == width - 1 {
        return BASALT_COLUMN[1];
    }
    if hash(x, y, 861).is_multiple_of(97) {
        return index_of(RED);
    }
    if noise(x, y, 4, 863) > 0.6 {
        BASALT_COLUMN[2]
    } else {
        BASALT_COLUMN[1]
    }
}

/// Tube glass: ropy black glass with lines of red heat in its cracks.
fn tube_pixel(x: i32, y: i32) -> PaletteIndex {
    let rope = ((x as f32) / 3.0 + noise(x, y, 6, 871) * 4.0 + (y as f32) / 5.0).sin();
    if hash(x, y, 873).is_multiple_of(53) {
        return index_of(ORANGE);
    }
    if rope > 0.92 {
        TUBE[3]
    } else if rope > 0.6 {
        TUBE[2]
    } else if rope < -0.85 {
        TUBE[0]
    } else {
        TUBE[1]
    }
}

/// Round logs stacked flat, each with a lit top, grain and a dark gap under it.
fn log_pixel(x: i32, y: i32, py: i32) -> PaletteIndex {
    let band = py.rem_euclid(8);
    match band {
        7 => LOG[0],
        0 => LOG[3],
        1 | 2 => {
            if noise(x * 2, y, 6, 881) > 0.7 {
                LOG[1]
            } else {
                LOG[3]
            }
        }
        _ => {
            if (x + band * 3).rem_euclid(13) == 0 || noise(x * 3, y, 5, 883) > 0.75 {
                LOG[1]
            } else {
                LOG[2]
            }
        }
    }
}

/// Rubble: broken chunks with dark gaps between them.
fn rubble_pixel(x: i32, y: i32) -> PaletteIndex {
    let (near, second, dx, dy) = super::worley(x, y, 5, 891, false);
    if second - near < 1.2 {
        return RUBBLE[0];
    }
    if dx + dy <= -2 {
        RUBBLE[3]
    } else if dx + dy <= 1 {
        RUBBLE[2]
    } else {
        RUBBLE[1]
    }
}

/// Packed gravel: pebbles of three greys.
const fn gravel_pixel(x: i32, y: i32) -> PaletteIndex {
    match hash(x / 2, y / 2, 895) % 7 {
        0 => GRAVEL[3],
        1 | 2 => GRAVEL[2],
        3 => GRAVEL[0],
        _ => GRAVEL[1],
    }
}

/// An iron-banded strongbox with a brass lock and rivets.
fn paint_strongbox(canvas: &mut TileCanvas) {
    const BOX: [&str; 10] = [
        "..kkkkkkkkkkkk..",
        ".kssmmmmmmmmssk.",
        ".ksmmmmmmmmmmsk.",
        ".kddddddddddddk.",
        ".kssmmmyymmmssk.",
        ".ksmmmmyymmmmsk.",
        ".kssmmmmmmmmssk.",
        ".ksmmmmmmmmmmsk.",
        ".krssssssssssrk.",
        "..kkkkkkkkkkkk..",
    ];
    stamp(canvas, &BOX, 6);
}

/// The humming core stone: a faceted teal disc with a bright heart, set in dark rock.
fn paint_core(canvas: &mut TileCanvas, seed: u32) {
    const CORE: [&str; 14] = [
        ".....kkkkkk.....",
        "...kkttttttkk...",
        "..kttmmmmmmttk..",
        ".kttmmiiiimmttk.",
        ".ktmmiiwwiimmtk.",
        "kttmiiwwwwiimttk",
        "ktmmiwwwwwwimmtk",
        "ktmmiwwwwwwimmtk",
        "kttmiiwwwwiimttk",
        ".ktmmiiwwiimmtk.",
        ".kttmmiiiimmttk.",
        "..kttmmmmmmttk..",
        "...kkttttttkk...",
        ".....kkkkkk.....",
    ];
    stamp(canvas, &CORE, 1);
    // A few runes scratched into the rim.
    for step in 0..3 {
        let px = 4 + ((seed >> (step * 3)) % 8) as i32;
        canvas.set(px, 2 + step * 5, index_of(CREAM));
    }
}

fn stamp(canvas: &mut TileCanvas, rows: &[&str], top: i32) {
    for (row, line) in rows.iter().enumerate() {
        for (column, symbol) in line.chars().enumerate() {
            let color = match symbol {
                'k' => INK,
                's' => STEEL_DARK,
                'm' => RUST,
                'd' => STEEL,
                'y' => GOLD,
                'r' => STEEL_LIGHT,
                't' => TEAL_DARK,
                'i' => MINT,
                'w' => ICE,
                _ => continue,
            };
            canvas.set(
                i32::try_from(column).unwrap_or(0),
                top + i32::try_from(row).unwrap_or(0),
                index_of(color),
            );
        }
    }
}

/// Paints an open tile with a Crust backdrop: the posts of a timber set behind a mine gallery
/// (and the lantern hung from its beam), or the cracked, glowing walls of a lava tube. Returns
/// whether it painted.
#[allow(
    clippy::too_many_lines,
    reason = "one backdrop per Crust place: cabin and camp, lava tube, mine gallery"
)]
pub(super) fn paint_crust_air(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    strata: &Strata,
) -> bool {
    let settings = terrain.settings();
    if !crust::crust_rules(settings) {
        return false;
    }
    // Inside the lost miner's cabin or the Fathers' camp: a plank wall and a lamp.
    let indoors = crust::cabin(settings).is_some_and(|cabin| cabin.contains(position))
        || crust::sinkhole_site(settings).is_some_and(|site| site.camp.contains(position));
    if indoors {
        for py in 0..TILE_ART {
            for px in 0..TILE_ART {
                let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
                let board = gx.rem_euclid(6);
                let index = if board == 0 {
                    index_of(NIGHT)
                } else if board == 1 {
                    index_of(RUST)
                } else if noise(gx * 3, gy, 7, 907) > 0.7
                    || (gy + gx.div_euclid(6) * 5).rem_euclid(23) == 0
                {
                    index_of(SOIL_DARK)
                } else {
                    index_of(SOIL)
                };
                canvas.set(px, py, index);
            }
        }
        let under_roof = matches!(
            kind_at(terrain, position.x, position.y - 1),
            Some(TileKind::Crust(CrustTile::CabinLog))
        );
        if under_roof && position.x.rem_euclid(5) == 2 {
            for (dx, dy, color) in [
                (0, 2, STEEL_DARK),
                (-1, 3, AMBER),
                (0, 3, CREAM),
                (1, 3, AMBER),
                (-1, 4, AMBER),
                (0, 4, GOLD),
                (1, 4, AMBER),
                (0, 5, STEEL_DARK),
            ] {
                canvas.set(8 + dx, dy, index_of(color));
            }
        }
        return true;
    }
    if crust::lava_tube_at(settings, position.x, position.y) {
        for py in 0..TILE_ART {
            for px in 0..TILE_ART {
                let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
                let crack = (noise(gx, gy, 7, 901) * 9.0) as i32;
                let index = if (gx + crack).rem_euclid(19) == 0 && bayer4(gx, gy) < 9 {
                    index_of(if hash(gx, gy, 903).is_multiple_of(3) {
                        ORANGE
                    } else {
                        RED
                    })
                } else if noise(gx, gy, 9, 905) > 0.7 {
                    index_of(MAROON)
                } else {
                    index_of(NIGHT)
                };
                canvas.set(px, py, index);
            }
        }
        return true;
    }
    // A mine gallery: a timber beam within three rows above and the rail within three below.
    let beam_row = (1..=3).find(|dy| {
        matches!(
            kind_at(terrain, position.x, position.y - dy),
            Some(TileKind::Crust(CrustTile::Timber))
        )
    });
    let rail_row = (1..=3).find(|dy| {
        matches!(
            kind_at(terrain, position.x, position.y + dy),
            Some(TileKind::Crust(CrustTile::Rail))
        )
    });
    let (Some(beam_row), Some(_)) = (beam_row, rail_row) else {
        return false;
    };
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, backdrop_pixel(strata, gx, gy));
        }
    }
    // A set's posts stand at the beam's ends; every set has one post pair per four columns.
    if position.x.rem_euclid(4) == 0 {
        for py in 0..TILE_ART {
            for px in 6..10 {
                let index = if px == 6 {
                    TIMBER[3]
                } else if px == 9 {
                    TIMBER[0]
                } else if (canvas.origin_y + py).rem_euclid(9) == 0 {
                    TIMBER[1]
                } else {
                    TIMBER[2]
                };
                canvas.set(px, py, index);
            }
        }
    }
    // A lantern hangs under every other set's beam.
    if beam_row == 1 && position.x.rem_euclid(8) == 2 {
        for py in 0..4 {
            canvas.set(8, py, index_of(STEEL_DARK));
        }
        for (dx, dy, color) in [
            (-1, 4, STEEL_DARK),
            (0, 4, STEEL),
            (1, 4, STEEL_DARK),
            (-1, 5, AMBER),
            (0, 5, CREAM),
            (1, 5, AMBER),
            (-1, 6, AMBER),
            (0, 6, GOLD),
            (1, 6, AMBER),
            (0, 7, STEEL_DARK),
        ] {
            canvas.set(8 + dx, dy, index_of(color));
        }
    }
    true
}

/// Whether tile `position` (clay or dirt) sits beside an aquifer: water within two tiles.
pub(super) fn beside_aquifer(terrain: &Terrain, position: TilePosition) -> bool {
    if !crust::crust_rules(terrain.settings()) {
        return false;
    }
    (-2..=2).any(|dy| {
        (-2..=2).any(|dx| {
            matches!(
                kind_at(terrain, position.x + dx, position.y + dy),
                Some(TileKind::Fluid(
                    fatherload_simulation::fluid::FluidKind::Water
                ))
            )
        })
    })
}

/// Damp stains on rock beside an aquifer.
pub(super) fn paint_seep(canvas: &mut TileCanvas, seed: u32) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let damp = noise(gx, gy, 5, 911 + seed % 3);
            if damp > 0.62 {
                let index = canvas.get(px, py);
                canvas.set(px, py, crate::rendering::pixel_art::shade::darker(index));
            }
            if damp > 0.8 && bayer4(gx, gy) < 2 {
                canvas.set(px, py, index_of(SKY));
            }
        }
    }
}

/// Rain-soaked mud (the Topsoil of a Deep Descent world): dark, clumpy and wet, with glints
/// of standing water, and a sheen of puddle on its top face.
pub(super) fn paint_mud(canvas: &mut TileCanvas, strata: &Strata, edges: Edges) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
            let clump = noise(x, y, 5, 921) + f32::from(bayer4(x, y)) / 45.0;
            // Streaks of standing water catch the light along the clumps' tops.
            let streak = (y + (noise(x, y, 9, 925) * 6.0) as i32).rem_euclid(5) == 0
                && noise(x, y, 4, 927) > 0.55;
            let index = if hash(x / 2, y, 923).is_multiple_of(41) {
                index_of(SKY_LIGHT)
            } else if streak {
                index_of(OCEAN)
            } else if clump > 0.74 {
                MUD[3]
            } else if clump > 0.5 {
                MUD[2]
            } else if clump > 0.24 {
                MUD[1]
            } else {
                MUD[0]
            };
            canvas.set(px, py, index);
        }
    }
    if edges.up.is_open() {
        for px in 1..TILE_ART - 1 {
            let gx = canvas.origin_x + px;
            canvas.set(
                px,
                2,
                if gx.rem_euclid(7) < 4 {
                    index_of(SKY)
                } else {
                    MUD[3]
                },
            );
            if gx.rem_euclid(11) == 3 {
                canvas.set(px, 3, index_of(SKY_LIGHT));
            }
        }
    }
    paint_edges(canvas, strata, edges, MUD[3], MUD[0], false);
}
