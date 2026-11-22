//! The Drowned and Frozen's tiles and back walls (Wave 9, Descent IV).
//!
//! * The Underground Ocean: barnacled, silt-packed seabed rock; kelp fronds; glowing coral.
//!   Its back wall is a deep blue-green gloom with drifting motes and a faint caustic dance.
//! * Brine Glaciers: pale salty brine ice with trapped bubbles, deep blue pack ice with long
//!   fractures, glassy rime; warm thermal vents glowing amber in the ice; builder figures
//!   frozen in clear blocks. Its back wall is cold blue with frost feathers.
//! * Geyser Fields: layered, crusty cream-and-ochre sinter; geyser throats, wet and dark round
//!   a glowing orange core. Its back wall is warm brown haze with mineral terraces.
//! * Merrow City: pale pitted reef limestone; its back wall a sea-green with sunken carving.
//! * Pearl Abyss: black lumpy pillow basalt and giant clams. Its back wall is near-black
//!   indigo with the odd far glint.

use fatherload_simulation::{
    cave::CaveTile,
    descent::Stratum,
    terrain::{Terrain, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{
    Cell, Edges, Ramp, Strata, TILE_ART, TileCanvas, hash, noise, paint_edges, ramp, worley,
};

const SEABED: Ramp = ramp([NIGHT, SLATE_DARK, TEAL_DARK, TEAL]);
const BRINE_ICE: Ramp = ramp([SKY, SKY_LIGHT, ICE, BONE]);
const PACK_ICE: Ramp = ramp([DEEP_BLUE, OCEAN, SKY, SKY_LIGHT]);
const SINTER: Ramp = ramp([LOAM, SAND, CREAM, BONE]);
const BASALT: Ramp = ramp([INK, NIGHT, SLATE_DARK, SLATE]);
const REEF: Ramp = ramp([TEAL_DARK, FOG, MIST, BONE]);
const CORAL: Ramp = ramp([MAROON, ROSE, PINK, CREAM]);

/// Whether `tile` is one of the Drowned and Frozen's (this module paints it).
#[must_use]
pub(super) const fn is_drowned_tile(tile: CaveTile) -> bool {
    matches!(
        tile,
        CaveTile::Seabed
            | CaveTile::Kelp
            | CaveTile::Coral
            | CaveTile::BrineIce
            | CaveTile::PackIce
            | CaveTile::Rime
            | CaveTile::ThermalVent
            | CaveTile::FrozenFigure
            | CaveTile::Sinter
            | CaveTile::GeyserVent
            | CaveTile::PillowBasalt
            | CaveTile::PearlClam
            | CaveTile::MerrowStone
    )
}

/// Representative colour (drill dust, the minimap).
#[must_use]
pub const fn drowned_color(tile: CaveTile) -> Option<raylib::prelude::Color> {
    Some(match tile {
        CaveTile::Seabed => TEAL_DARK,
        CaveTile::Kelp => MOSS,
        CaveTile::Coral => ROSE,
        CaveTile::BrineIce | CaveTile::Rime => ICE,
        CaveTile::PackIce => SKY,
        CaveTile::ThermalVent => AMBER,
        CaveTile::FrozenFigure => SKY_LIGHT,
        CaveTile::Sinter => CREAM,
        CaveTile::GeyserVent => ORANGE,
        CaveTile::PillowBasalt => SLATE_DARK,
        CaveTile::PearlClam | CaveTile::MerrowStone => MIST,
        _ => return None,
    })
}

/// The glow of a lit Drowned tile.
#[must_use]
pub const fn drowned_glow(tile: CaveTile) -> Option<raylib::prelude::Color> {
    match tile {
        CaveTile::Coral => Some(PINK),
        CaveTile::ThermalVent | CaveTile::GeyserVent => Some(AMBER),
        CaveTile::FrozenFigure => Some(ICE),
        CaveTile::PearlClam => Some(BONE),
        _ => None,
    }
}

/// The Drowned stratum at `position` (`None` elsewhere and on older worlds).
#[must_use]
pub(super) fn drowned_stratum(terrain: &Terrain, position: TilePosition) -> Option<Stratum> {
    if !terrain.settings().generator.has_descents() {
        return None;
    }
    let stratum = terrain.stratum_at(position);
    fatherload_simulation::drowned::is_drowned(stratum).then_some(stratum)
}

/// One art pixel of `stratum`'s back wall at absolute art pixel `(x, y)`.
#[must_use]
pub(super) fn back_pixel(stratum: Stratum, x: i32, y: i32) -> PaletteIndex {
    match stratum {
        Stratum::UndergroundOcean => {
            // Blue-green gloom, motes, a caustic net of light rippling high on the wall.
            let (near, second, _, _) = worley(x, y, 11, 901, false);
            if hash(x, y, 903).is_multiple_of(181) {
                index_of(MINT)
            } else if second - near < 0.6 && noise(x, y, 20, 905) > 0.55 {
                index_of(TEAL)
            } else if noise(x, y, 9, 907) + f32::from(bayer4(x, y)) / 40.0 > 0.6 {
                index_of(TEAL_DARK)
            } else {
                index_of(DEEP_BLUE)
            }
        }
        Stratum::BrineGlaciers => {
            // Cold blue with feathered frost.
            let feather = (x + y * 2 + (noise(x, y, 8, 911) * 6.0) as i32).rem_euclid(11) == 0
                && noise(x, y, 6, 913) > 0.45;
            if hash(x, y, 915).is_multiple_of(149) {
                index_of(ICE)
            } else if feather {
                index_of(SKY)
            } else if noise(x, y, 10, 917) + f32::from(bayer4(x, y)) / 40.0 > 0.62 {
                index_of(OCEAN)
            } else {
                index_of(DEEP_BLUE)
            }
        }
        Stratum::GeyserFields => {
            // Warm haze over mineral terraces.
            let terrace = (y + (noise(x, y, 18, 921) * 5.0) as i32).rem_euclid(7);
            if terrace == 0 {
                index_of(LOAM)
            } else if terrace == 1 && bayer4(x, y) < 8 {
                index_of(SOIL)
            } else if hash(x, y, 923).is_multiple_of(173) {
                index_of(AMBER)
            } else {
                index_of(SOIL_DARK)
            }
        }
        Stratum::MerrowCity => {
            // Sea green with soft, sunken carving (scallops of old merrow masonry).
            let row = y.div_euclid(8);
            let shift = if row.rem_euclid(2) == 0 { 0 } else { 5 };
            let arc = (x + shift).rem_euclid(10);
            let cy = y.rem_euclid(8);
            if cy == 0 || (cy == 1 && (arc == 0 || arc == 9)) {
                index_of(TEAL_DARK)
            } else if hash(x, y, 931).is_multiple_of(97) {
                index_of(MINT)
            } else if noise(x, y, 9, 933) > 0.62 {
                index_of(TEAL)
            } else {
                index_of(NIGHT)
            }
        }
        _ => {
            // The abyss: near-black indigo, a far glint.
            if hash(x, y, 941).is_multiple_of(211) {
                index_of(LAVENDER)
            } else if noise(x, y, 14, 943) + f32::from(bayer4(x, y)) / 50.0 > 0.72 {
                index_of(SLATE_DARK)
            } else {
                index_of(INK)
            }
        }
    }
}

/// Paints an open tile of `stratum`: its back wall, shadowed under rock and beside walls.
pub(super) fn paint_back(
    canvas: &mut TileCanvas,
    stratum: Stratum,
    up: Cell,
    left: Cell,
    right: Cell,
) {
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
                back_pixel(stratum, gx, gy)
            };
            canvas.set(px, py, index);
        }
    }
}

fn seabed_pixel(x: i32, y: i32) -> PaletteIndex {
    // Packed silt in layers, barnacle clusters (pale rings) and dark pores.
    let (near, _, _, _) = worley(x, y, 5, 951, false);
    if near < 0.25 && hash(x / 5, y / 5, 953).is_multiple_of(3) {
        return index_of(MIST);
    }
    if near < 0.5 && hash(x / 5, y / 5, 953).is_multiple_of(3) {
        return index_of(STONE_LIGHT);
    }
    let layer = (y + (noise(x, y, 12, 955) * 4.0) as i32).rem_euclid(6);
    if hash(x, y, 957).is_multiple_of(37) {
        SEABED[0]
    } else if layer == 0 {
        SEABED[1]
    } else if noise(x, y, 6, 959) > 0.6 {
        SEABED[3]
    } else {
        SEABED[2]
    }
}

fn brine_ice_pixel(x: i32, y: i32) -> PaletteIndex {
    // Salty white-blue ice with trapped bubbles and a wet sheen.
    if hash(x, y, 961).is_multiple_of(29) {
        return index_of(BONE);
    }
    let (near, _, _, _) = worley(x, y, 4, 963, false);
    if near < 0.18 {
        return BRINE_ICE[1];
    }
    if (x - y).rem_euclid(13) == 0 && noise(x, y, 8, 965) > 0.5 {
        return BRINE_ICE[3];
    }
    if noise(x, y, 7, 967) > 0.58 {
        BRINE_ICE[2]
    } else {
        BRINE_ICE[1]
    }
}

fn pack_ice_pixel(x: i32, y: i32) -> PaletteIndex {
    // Deep blue, long pale fracture lines, dark depths.
    let fracture = (x * 2 - y + (noise(x, y, 14, 971) * 9.0) as i32).rem_euclid(19);
    if fracture == 0 {
        return PACK_ICE[3];
    }
    if fracture == 1 {
        return PACK_ICE[0];
    }
    if noise(x, y, 9, 973) + f32::from(bayer4(x, y)) / 40.0 > 0.62 {
        PACK_ICE[2]
    } else {
        PACK_ICE[1]
    }
}

const fn rime_pixel(x: i32, y: i32) -> PaletteIndex {
    // Thin, glassy, feathery ice: the tunnel's back wall shows through as blue.
    let feather = (x + (y * 3) / 2).rem_euclid(5) == 0 || (x - (y * 3) / 2).rem_euclid(7) == 0;
    if feather {
        index_of(BONE)
    } else if bayer4(x, y) < 6 {
        index_of(ICE)
    } else {
        index_of(SKY_LIGHT)
    }
}

fn sinter_pixel(x: i32, y: i32) -> PaletteIndex {
    // Crusty layered mineral: cream and ochre bands, pitted.
    let band = (y + (noise(x, y, 10, 981) * 3.0) as i32).rem_euclid(5);
    if hash(x, y, 983).is_multiple_of(23) {
        return SINTER[0];
    }
    match band {
        0 => index_of(AMBER),
        1 => SINTER[1],
        2 | 3 => SINTER[2],
        _ => {
            if noise(x, y, 5, 985) > 0.55 {
                SINTER[3]
            } else {
                SINTER[2]
            }
        }
    }
}

fn basalt_pixel(x: i32, y: i32) -> PaletteIndex {
    // Black pillows: rounded lumps with a dull rim of light on top.
    let (near, second, _, _) = worley(x, y, 6, 991, false);
    if second - near < 0.18 {
        return BASALT[0];
    }
    if near < 0.32 && second - near > 0.5 {
        return BASALT[3];
    }
    if near < 0.6 { BASALT[2] } else { BASALT[1] }
}

fn reef_pixel(x: i32, y: i32) -> PaletteIndex {
    // Pale pitted reef limestone with sea-green stains.
    let (near, _, _, _) = worley(x, y, 4, 1_001, false);
    if near < 0.2 {
        return REEF[0];
    }
    if hash(x, y, 1_003).is_multiple_of(31) {
        return index_of(TEAL);
    }
    if noise(x, y, 8, 1_005) > 0.6 {
        REEF[3]
    } else {
        REEF[2]
    }
}

fn coral_pixel(x: i32, y: i32) -> PaletteIndex {
    // Branching coral: rosy branches with cream tips over a deep red gloom.
    let branch = (x + (noise(x, y, 6, 1_011) * 6.0) as i32).rem_euclid(5) <= 1
        || (y + (noise(x, y, 7, 1_013) * 6.0) as i32).rem_euclid(6) == 0;
    if hash(x, y, 1_015).is_multiple_of(19) {
        return CORAL[3];
    }
    if branch {
        if noise(x, y, 4, 1_017) > 0.55 {
            CORAL[2]
        } else {
            CORAL[1]
        }
    } else {
        CORAL[0]
    }
}

/// Water behind a decoration living in the sea (kelp, clams).
fn sea_pixel(x: i32, y: i32) -> PaletteIndex {
    if noise(x, y, 9, 1_021) + f32::from(bayer4(x, y)) / 40.0 > 0.62 {
        index_of(OCEAN)
    } else {
        index_of(DEEP_BLUE)
    }
}

/// Paints a Drowned and Frozen tile.
pub(super) fn paint(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: CaveTile,
    strata: &Strata,
    edges: Edges,
) {
    let seed = hash(position.x, position.y, 1_031);
    let back = drowned_stratum(terrain, position);
    let backdrop = |canvas: &mut TileCanvas, sea: bool| {
        for py in 0..TILE_ART {
            for px in 0..TILE_ART {
                let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
                let index = if sea {
                    sea_pixel(gx, gy)
                } else {
                    back.map_or(strata.back[0], |stratum| back_pixel(stratum, gx, gy))
                };
                canvas.set(px, py, index);
            }
        }
    };
    match tile {
        CaveTile::Kelp => {
            backdrop(canvas, true);
            paint_kelp(canvas, seed);
            return;
        }
        CaveTile::PearlClam => {
            backdrop(canvas, true);
            paint_clam(canvas);
            return;
        }
        _ => {}
    }
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
            let index = match tile {
                CaveTile::Seabed => seabed_pixel(x, y),
                CaveTile::Coral => coral_pixel(x, y),
                CaveTile::BrineIce | CaveTile::FrozenFigure | CaveTile::ThermalVent => {
                    brine_ice_pixel(x, y)
                }
                CaveTile::PackIce => pack_ice_pixel(x, y),
                CaveTile::Rime => rime_pixel(x, y),
                CaveTile::Sinter | CaveTile::GeyserVent => sinter_pixel(x, y),
                CaveTile::PillowBasalt => basalt_pixel(x, y),
                _ => reef_pixel(x, y),
            };
            canvas.set(px, py, index);
        }
    }
    match tile {
        CaveTile::FrozenFigure => paint_figure(canvas),
        CaveTile::ThermalVent => paint_vent(canvas, seed),
        CaveTile::GeyserVent => paint_throat(canvas),
        _ => {}
    }
    let material: Ramp = match tile {
        CaveTile::Seabed => SEABED,
        CaveTile::Coral => CORAL,
        CaveTile::BrineIce | CaveTile::FrozenFigure | CaveTile::ThermalVent | CaveTile::Rime => {
            BRINE_ICE
        }
        CaveTile::PackIce => PACK_ICE,
        CaveTile::Sinter | CaveTile::GeyserVent => SINTER,
        CaveTile::PillowBasalt => BASALT,
        _ => REEF,
    };
    paint_edges(canvas, strata, edges, material[3], material[0], false);
}

/// Kelp: two or three swaying fronds of green rising through the water, gas bladders on them.
fn paint_kelp(canvas: &mut TileCanvas, seed: u32) {
    let fronds = 2 + (seed % 2) as i32;
    for index in 0..fronds {
        let base = 3 + index * 5 + (seed >> (index * 3) & 1) as i32;
        for py in 0..TILE_ART {
            let sway = ((py + index * 3) as f32 * 0.5)
                .sin()
                .mul_add(1.5, 0.0)
                .round() as i32;
            let x = base + sway;
            canvas.set(x, py, index_of(MOSS));
            canvas.set(x + 1, py, index_of(LEAF));
            if (py + index * 4).rem_euclid(6) == 0 {
                canvas.set(x + 2, py, index_of(LIME));
                canvas.set(x - 1, py, index_of(PINE));
            }
        }
    }
}

/// A giant clam half sunk in the silt: a ribbed, pale shell with a dark lip (the world layer
/// draws it gaping, its pearl gleaming, on its beat).
fn paint_clam(canvas: &mut TileCanvas) {
    for py in 6..TILE_ART {
        for px in 1..TILE_ART - 1 {
            let dx = (px - 7).abs();
            let top = 6 + dx * dx / 10;
            if py < top {
                continue;
            }
            let rib = px.rem_euclid(3) == 0;
            let index = if py == top {
                index_of(INK)
            } else if py >= 13 {
                index_of(STONE_DARK)
            } else if py == 10 {
                index_of(PLUM)
            } else if rib {
                index_of(FOG)
            } else {
                index_of(MIST)
            };
            canvas.set(px, py, index);
        }
    }
}

/// A builder's figure in clear ice: a tall, dark silhouette with a raised hand, a pale halo.
fn paint_figure(canvas: &mut TileCanvas) {
    const FIGURE: [&str; 16] = [
        "................",
        "........oo......",
        ".......oSSo..o..",
        ".......oSSo.oo..",
        "........oo.oo...",
        "......oooooo....",
        ".....oSSSSSo....",
        ".....oSSSSSo....",
        ".....oSSSSSo....",
        ".....oSSSSSo....",
        "......oSSSo.....",
        "......oS.So.....",
        "......oS.So.....",
        "......oS.So.....",
        ".....ooo.ooo....",
        "................",
    ];
    for (py, row) in FIGURE.iter().enumerate() {
        for (px, symbol) in row.chars().enumerate() {
            let index = match symbol {
                'o' => index_of(SLATE_DARK),
                'S' => index_of(SLATE),
                _ => continue,
            };
            #[allow(
                clippy::cast_possible_wrap,
                clippy::cast_possible_truncation,
                reason = "16"
            )]
            canvas.set(px as i32, py as i32, index);
        }
    }
}

/// A thermal vent: a dark slot in the ice, its warm amber core glowing, frost melted round it.
fn paint_vent(canvas: &mut TileCanvas, seed: u32) {
    for py in 4..TILE_ART {
        for px in 5_i32..11 {
            let core = (px - 7).abs() + (py - 11).abs() < 3;
            let index = if core {
                index_of(if (seed + px as u32).is_multiple_of(3) {
                    GOLD
                } else {
                    AMBER
                })
            } else if (px - 7).abs() < 2 {
                index_of(ORANGE)
            } else {
                index_of(RUST)
            };
            canvas.set(px, py, index);
        }
    }
    for px in 3..13 {
        canvas.set(px, 3, index_of(SOIL));
    }
}

/// A geyser's throat: a wet, dark mouth ringed with crust, a glowing orange core deep in it.
fn paint_throat(canvas: &mut TileCanvas) {
    for py in 0..TILE_ART {
        for px in 2_i32..14 {
            let dx = (px - 7).abs();
            if dx > 5 - py / 6 {
                continue;
            }
            let index = if py > 9 && dx < 2 {
                index_of(ORANGE)
            } else if py > 6 && dx < 3 {
                index_of(RUST)
            } else if dx >= 4 - py / 6 {
                index_of(LOAM)
            } else {
                index_of(SOIL_DARK)
            };
            canvas.set(px, py, index);
        }
    }
}
