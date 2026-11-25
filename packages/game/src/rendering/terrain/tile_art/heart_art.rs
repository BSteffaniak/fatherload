//! Tile art for Descent VIII, the Heart (Wave 10, `fatherload_simulation::heart`), and its
//! back walls.
//!
//! * Dark Matter Veins: violet shale in thin layers, black null stone in slabs, and dark
//!   matter as a pocket of starry void with a violet rim (it looks like a hole in the world).
//! * Singularity Fields: banded event shale curving round the wells, glassy horizon rock, pink
//!   shards of event glass, and the singularity's point (the renderer draws the black hole).
//! * Core Ocean: rusty core basalt in columns, dark iron crust, core metal nuggets, and the
//!   magnetic rails: a steel beam with a glowing teal magnet strip on a see-through truss, the
//!   switch's amber arrow lamp, the booster's lit chevrons, and the pylons' riveted columns.
//! * The Heart: warm faceted star rock, pale drive alloy with gold seams, white-gold star shards,
//!   the chamber's dark golden shell with circuit lines, the seal's teal keyhole sigil, and the
//!   builders' braided cable.

use fatherload_simulation::{
    descent::Stratum,
    heart::CoreTile,
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
    worley,
};

const SHALE: Ramp = ramp([NIGHT, PLUM, VIOLET, LAVENDER]);
const NULL: Ramp = ramp([INK, NIGHT, SLATE_DARK, PLUM]);
const VOID: Ramp = ramp([INK, NIGHT, VIOLET, LAVENDER]);
const EVENT: Ramp = ramp([INK, DEEP_BLUE, SLATE_DARK, PLUM]);
const HORIZON: Ramp = ramp([INK, NIGHT, DEEP_BLUE, SKY]);
const GLASS: Ramp = ramp([PLUM, ROSE, PINK, BONE]);
const BASALT: Ramp = ramp([MAROON, RUST, COPPER, AMBER]);
const IRON: Ramp = ramp([INK, STEEL_DARK, RUST, COPPER]);
const STEEL_RAMP: Ramp = ramp([INK, STEEL_DARK, STEEL, STEEL_LIGHT]);
const PLATE: Ramp = ramp([STEEL_DARK, STEEL, STEEL_LIGHT, CREAM]);
const STAR: Ramp = ramp([MAROON, RUST, COPPER, AMBER]);
const ALLOY: Ramp = ramp([STEEL_DARK, STEEL, STEEL_LIGHT, BONE]);
const SHELL: Ramp = ramp([MAROON, COPPER, AMBER, GOLD]);
const CABLE: Ramp = ramp([STEEL_DARK, STEEL, GOLD, CREAM]);

/// Representative colour of a Heart tile (drill dust, debris, the map).
#[must_use]
pub const fn core_color(tile: CoreTile) -> raylib::prelude::Color {
    match tile {
        CoreTile::VeinShale => PLUM,
        CoreTile::NullStone | CoreTile::Singularity => NIGHT,
        CoreTile::DarkMatter => VIOLET,
        CoreTile::EventShale | CoreTile::HorizonRock => DEEP_BLUE,
        CoreTile::EventGlass => PINK,
        CoreTile::CoreBasalt => RUST,
        CoreTile::IronCrust => STEEL_DARK,
        CoreTile::CoreMetal | CoreTile::DriveAlloy => STEEL_LIGHT,
        CoreTile::MagRail | CoreTile::MagSwitch | CoreTile::MagBooster => TEAL,
        CoreTile::RailPylon | CoreTile::TerminalPlate => STEEL,
        CoreTile::StarRock => AMBER,
        CoreTile::StarShard | CoreTile::StarCore => CREAM,
        CoreTile::HeartShell => COPPER,
        CoreTile::HeartSeal => MINT,
        CoreTile::Cable => GOLD,
    }
}

/// The light a Heart tile gives (halos, the light overlay's tint).
#[must_use]
pub const fn core_glow(tile: CoreTile) -> Option<raylib::prelude::Color> {
    match tile {
        CoreTile::DarkMatter => Some(LAVENDER),
        CoreTile::EventGlass => Some(PINK),
        CoreTile::MagRail | CoreTile::MagSwitch => Some(TEAL),
        CoreTile::MagBooster | CoreTile::HeartSeal => Some(MINT),
        CoreTile::StarShard | CoreTile::StarCore | CoreTile::Cable => Some(GOLD),
        CoreTile::CoreMetal => Some(STEEL_LIGHT),
        _ => None,
    }
}

/// The Heart stratum at `position` (`None` elsewhere and on older worlds).
#[must_use]
pub(super) fn heart_stratum(terrain: &Terrain, position: TilePosition) -> Option<Stratum> {
    if !terrain.settings().generator.has_descents() {
        return None;
    }
    let stratum = terrain.stratum_at(position);
    fatherload_simulation::heart::is_heart(stratum).then_some(stratum)
}

/// A rail's beam over a see-through truss (`None`: see-through).
const fn rail_pixel(tile: CoreTile, px: i32, py: i32, x: i32) -> Option<PaletteIndex> {
    match py {
        0 => Some(STEEL_RAMP[3]),
        1 | 2 => Some(STEEL_RAMP[2]),
        3 => Some(match tile {
            // The magnet strip: teal, pulsing dots along it (static art; lit by the glow).
            CoreTile::MagBooster => index_of(if x.rem_euclid(4) < 2 { LIME } else { MINT }),
            _ => index_of(if x.rem_euclid(6) == 0 { MINT } else { TEAL }),
        }),
        4 => Some(STEEL_RAMP[1]),
        5 => Some(STEEL_RAMP[0]),
        _ => {
            // The truss: two rails of a lattice and diagonals between.
            let row = py - 6;
            let diagonal = (px + row).rem_euclid(8) == 0 || (px - row).rem_euclid(8) == 0;
            if py == TILE_ART - 1 || (diagonal && py < TILE_ART - 1) {
                Some(STEEL_RAMP[1])
            } else {
                None
            }
        }
    }
}

/// One art pixel of `tile`'s body at absolute art pixel `(x, y)` (tile pixel `(px, py)`).
#[allow(clippy::too_many_lines, reason = "one arm per material")]
fn body_pixel(tile: CoreTile, x: i32, y: i32, px: i32, py: i32) -> Option<PaletteIndex> {
    Some(match tile {
        CoreTile::VeinShale => {
            // Layers of shale, broad and wavy, cracked here and there; a violet fleck.
            let wave = (noise(x, y, 18, 1001) * 9.0) as i32;
            let band = (y + wave).div_euclid(5);
            let crack = (y + wave).rem_euclid(5) == 0
                && hash(x.div_euclid(4), band, 1003).is_multiple_of(2);
            if hash(x, y, 1002).is_multiple_of(71) {
                SHALE[3]
            } else if crack {
                SHALE[0]
            } else if hash(band, x.div_euclid(9), 1004).is_multiple_of(3) {
                SHALE[2]
            } else {
                SHALE[1]
            }
        }
        CoreTile::NullStone => NULL[slab_level(x, y)],
        CoreTile::DarkMatter => {
            // Shards of dark matter growing UP (it falls up): violet crystals with lavender
            // edges round a starry void, on black stone.
            let shard = |sx: i32, base: i32, height: i32| {
                let rise = base - py;
                rise >= 0 && rise < height && (px - sx).abs() <= (height - rise) / 3
            };
            let in_shard = shard(4, 14, 11) || shard(10, 15, 13) || shard(13, 13, 7);
            if in_shard {
                if hash(x, y, 1004).is_multiple_of(19) {
                    index_of(BONE)
                } else if (px + py).rem_euclid(5) == 0 || hash(x, y, 1003).is_multiple_of(5) {
                    index_of(LAVENDER)
                } else {
                    index_of(VIOLET)
                }
            } else if hash(x, y, 1005).is_multiple_of(29) {
                index_of(PLUM)
            } else {
                VOID[0]
            }
        }
        CoreTile::EventShale => {
            // Bands curving as if pulled toward something below and to the side.
            let curve = (x * x / 64 + y * 3 + (noise(x, y, 12, 1006) * 4.0) as i32).rem_euclid(9);
            if hash(x, y, 1007).is_multiple_of(97) {
                EVENT[3]
            } else {
                EVENT[match curve {
                    0 => 0,
                    1 | 2 => 2,
                    _ => 1,
                }]
            }
        }
        CoreTile::HorizonRock => {
            let (near, second, dx, _) = worley(x, y, 9, 1008, false);
            if second - near < 1.2 {
                HORIZON[0]
            } else if dx < 0 && near < 2.0 {
                HORIZON[3]
            } else {
                HORIZON[1 + usize::from(near < 3.0)]
            }
        }
        CoreTile::EventGlass => {
            // Pink shards, each with a bright edge.
            let shard = (x * 2 - y).div_euclid(5);
            let along = (x * 2 - y).rem_euclid(5);
            if along == 0 {
                GLASS[0]
            } else if along == 1 && hash(shard, 0, 1009).is_multiple_of(2) {
                GLASS[3]
            } else {
                GLASS[1 + usize::from((x + y).rem_euclid(3) == 0)]
            }
        }
        CoreTile::Singularity => {
            let (cx, cy) = (px - 7, py - 7);
            if cx * cx + cy * cy <= 9 {
                index_of(INK)
            } else {
                return None;
            }
        }
        CoreTile::CoreBasalt => {
            // Rusty columns: vertical seams, a horizontal crack now and then.
            let column = x.div_euclid(6);
            let seam = x.rem_euclid(6) == 0;
            let crack = (y + column * 5).rem_euclid(13) == 0;
            if seam || crack {
                BASALT[0]
            } else if x.rem_euclid(6) == 1 {
                BASALT[2]
            } else {
                BASALT[1 + usize::from(hash(column, y.div_euclid(3), 1010).is_multiple_of(5))]
            }
        }
        CoreTile::IronCrust => {
            let level = slab_level(x, y);
            if hash(x, y, 1011).is_multiple_of(29) {
                IRON[3]
            } else {
                IRON[level.min(2)]
            }
        }
        CoreTile::CoreMetal => {
            // Basalt with bright metal nuggets.
            let (near, _, _, _) = worley(x, y, 6, 1012, true);
            if near < 1.5 {
                index_of(STEEL_LIGHT)
            } else if near < 2.5 {
                index_of(STEEL)
            } else if near < 3.0 {
                index_of(INK)
            } else {
                BASALT[1]
            }
        }
        CoreTile::MagRail | CoreTile::MagSwitch | CoreTile::MagBooster => {
            return rail_pixel(tile, px, py, x);
        }
        CoreTile::RailPylon => {
            // A riveted column, see-through either side.
            if !(3..=12).contains(&px) {
                return None;
            }
            if px == 3 || px == 12 {
                STEEL_RAMP[0]
            } else if (px == 5 || px == 10) && y.rem_euclid(8) == 2 {
                STEEL_RAMP[3]
            } else if px == 4 {
                STEEL_RAMP[2]
            } else {
                STEEL_RAMP[1]
            }
        }
        CoreTile::TerminalPlate => {
            let (qx, qy) = (x.rem_euclid(16), y.rem_euclid(8));
            if qy == 0 || qx == 0 {
                PLATE[0]
            } else if qy == 1 {
                PLATE[3]
            } else {
                PLATE[1 + usize::from(qx == 1)]
            }
        }
        CoreTile::StarRock => {
            let (near, second, dx, dy) = worley(x, y, 8, 1013, false);
            if second - near < 1.0 {
                STAR[0]
            } else if dx + dy < 0 {
                STAR[2 + usize::from(hash(x, y, 1014).is_multiple_of(19))]
            } else {
                STAR[1]
            }
        }
        CoreTile::DriveAlloy => {
            // Pale plates with gold seams in a builder pattern.
            let (qx, qy) = (x.rem_euclid(12), y.rem_euclid(12));
            if qx == 0 || qy == 0 {
                index_of(
                    if (x.div_euclid(12) + y.div_euclid(12)).rem_euclid(3) == 0 {
                        GOLD
                    } else {
                        STEEL_DARK
                    },
                )
            } else {
                ALLOY[1 + usize::from(qx + qy < 6) + usize::from(qx + qy < 3)]
            }
        }
        CoreTile::StarShard => {
            // Star rock with a white-gold crystal cluster.
            let (cx, cy) = (px - 8, py - 8);
            let spike = cx.abs() + cy.abs() <= 5 && (cx == 0 || cy == 0 || cx.abs() == cy.abs());
            if spike {
                index_of(if cx.abs() + cy.abs() <= 1 {
                    BONE
                } else {
                    CREAM
                })
            } else if cx.abs() + cy.abs() <= 6 && hash(x, y, 1015).is_multiple_of(2) {
                index_of(GOLD)
            } else {
                STAR[1]
            }
        }
        CoreTile::HeartShell => {
            // Dark gold plating with circuit lines.
            let trace_x = x.rem_euclid(10) == 3 && y.rem_euclid(20) < 14;
            let trace_y = y.rem_euclid(10) == 6 && x.rem_euclid(20) < 12;
            if trace_x || trace_y {
                SHELL[3]
            } else if x.rem_euclid(10) == 0 || y.rem_euclid(10) == 0 {
                SHELL[0]
            } else {
                SHELL[1 + usize::from(bayer4(x, y) < 3)]
            }
        }
        CoreTile::HeartSeal => {
            // The keyhole sigil in teal on the shell.
            let (cx, cy) = (px - 8, py - 7);
            let ring = (cx * cx + cy * cy - 20).abs() <= 4;
            let stem = cx.abs() <= 1 && (0..=5).contains(&cy);
            if ring || stem {
                index_of(MINT)
            } else if cx * cx + cy * cy <= 9 {
                index_of(TEAL)
            } else {
                SHELL[1]
            }
        }
        CoreTile::StarCore => index_of(if bayer4(x, y) < 8 { CREAM } else { BONE }),
        CoreTile::Cable => {
            // Braided strands twisting down; see-through either side of the braid.
            if !(4..=11).contains(&px) {
                return None;
            }
            let strand = (px + y.div_euclid(2)).rem_euclid(4);
            if px == 4 || px == 11 {
                CABLE[0]
            } else if strand == 0 {
                CABLE[3]
            } else if strand == 1 {
                CABLE[2]
            } else {
                CABLE[1]
            }
        }
    })
}

/// Which way a ramp of rail climbs through `position`: `Some(1)` rising to the right,
/// `Some(-1)` falling to the right, `None` level.
fn ramp_slope(terrain: &Terrain, position: TilePosition) -> Option<i32> {
    let rail = |dx: i32, dy: i32| {
        terrain
            .tile(TilePosition {
                x: position.x + dx,
                y: position.y + dy,
            })
            .is_some_and(|tile| matches!(tile.kind, fatherload_simulation::terrain::TileKind::Core(core) if core.is_mag_rail()))
    };
    let level = rail(-1, 0) || rail(1, 0);
    if level {
        return None;
    }
    if rail(1, -1) || rail(-1, 1) {
        Some(1)
    } else if rail(1, 1) || rail(-1, -1) {
        Some(-1)
    } else {
        None
    }
}

/// Paints a ramp's slanted beam (see [`ramp_slope`]) over the stratum's back wall.
fn paint_ramp(canvas: &mut TileCanvas, stratum: Stratum, tile: CoreTile, slope: i32) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            // The beam's top edge runs corner to corner.
            let edge = if slope > 0 { TILE_ART - 1 - px } else { px };
            let depth = py - edge;
            let index = match depth {
                0 => Some(STEEL_RAMP[3]),
                1 => Some(STEEL_RAMP[2]),
                2 => Some(match tile {
                    CoreTile::MagBooster => index_of(LIME),
                    _ => index_of(TEAL),
                }),
                3 => Some(STEEL_RAMP[1]),
                4 => Some(STEEL_RAMP[0]),
                _ => None,
            };
            canvas.set(px, py, index.unwrap_or_else(|| back_pixel(stratum, gx, gy)));
        }
    }
}

/// Paints Heart tile `tile` at `position`.
pub(super) fn paint(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: CoreTile,
    strata: &Strata,
    edges: Edges,
) {
    let stratum = heart_stratum(terrain, position).unwrap_or(Stratum::DarkMatterVeins);
    if tile.is_mag_rail()
        && let Some(slope) = ramp_slope(terrain, position)
    {
        paint_ramp(canvas, stratum, tile, slope);
        return;
    }
    let mut open = false;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let index = body_pixel(tile, gx, gy, px, py).unwrap_or_else(|| {
                open = true;
                // See-through parts show the stratum's back wall.
                back_pixel(stratum, gx, gy)
            });
            canvas.set(px, py, index);
        }
    }
    if open || tile == CoreTile::Singularity {
        // Rails, pylons, the cable and the singularity have no rock rim.
        return;
    }
    let (lip, shade) = match tile {
        CoreTile::VeinShale => (SHALE[3], SHALE[0]),
        CoreTile::NullStone | CoreTile::DarkMatter => (VOID[2], VOID[0]),
        CoreTile::EventShale => (EVENT[3], EVENT[0]),
        CoreTile::HorizonRock => (HORIZON[3], HORIZON[0]),
        CoreTile::EventGlass => (GLASS[3], GLASS[0]),
        CoreTile::CoreBasalt | CoreTile::CoreMetal => (BASALT[3], BASALT[0]),
        CoreTile::IronCrust => (IRON[3], IRON[0]),
        CoreTile::TerminalPlate | CoreTile::DriveAlloy => (PLATE[3], PLATE[0]),
        CoreTile::StarRock | CoreTile::StarShard | CoreTile::StarCore => (STAR[3], STAR[0]),
        CoreTile::HeartShell | CoreTile::HeartSeal => (SHELL[3], SHELL[0]),
        _ => (STEEL_RAMP[3], STEEL_RAMP[0]),
    };
    let square = matches!(
        tile,
        CoreTile::TerminalPlate | CoreTile::DriveAlloy | CoreTile::HeartShell | CoreTile::HeartSeal
    );
    paint_edges(canvas, strata, edges, lip, shade, square);
}

/// One art pixel of `stratum`'s back wall at absolute art pixel `(x, y)`.
#[must_use]
fn back_pixel(stratum: Stratum, x: i32, y: i32) -> PaletteIndex {
    match stratum {
        Stratum::DarkMatterVeins => {
            // Deep violet shale in the dark, grit motes drifting the wrong way.
            if hash(x, y, 1031).is_multiple_of(173) {
                index_of(LAVENDER)
            } else if (y * 2 + (noise(x, y, 18, 1032) * 6.0) as i32).rem_euclid(11) == 0
                || noise(x, y, 10, 1033) + f32::from(bayer4(x, y)) / 40.0 > 0.68
            {
                index_of(PLUM)
            } else {
                index_of(NIGHT)
            }
        }
        Stratum::SingularityFields => {
            // Near black, faint stars, faint curved bands.
            if hash(x, y, 1041).is_multiple_of(151) {
                index_of(BONE)
            } else if (x * x / 96 + y * 2).rem_euclid(17) == 0 {
                index_of(DEEP_BLUE)
            } else if noise(x, y, 12, 1042) + f32::from(bayer4(x, y)) / 40.0 > 0.72 {
                index_of(NIGHT)
            } else {
                index_of(INK)
            }
        }
        Stratum::CoreOcean => {
            // Hot dark basalt columns lit faintly from below by the metal.
            let column = x.rem_euclid(18) == 0;
            let warm = noise(x, y, 22, 1051) + f32::from(bayer4(x, y)) / 50.0;
            if (column && y.rem_euclid(40) < 30) || warm > 0.82 {
                index_of(MAROON)
            } else if hash(x, y, 1052).is_multiple_of(331) {
                index_of(RUST)
            } else {
                index_of(NIGHT)
            }
        }
        _ => {
            // The Heart: dark walls in a faint hex lattice of the builders, lit warm.
            let (hx, hy) = (x.rem_euclid(16), y.rem_euclid(18));
            let edge = hy == 0 || (hx == 0 && hy < 9) || (hx == 8 && hy >= 9);
            if edge {
                index_of(MAROON)
            } else if hash(x, y, 1061).is_multiple_of(173) {
                index_of(AMBER)
            } else if noise(x, y, 14, 1062) + f32::from(bayer4(x, y)) / 40.0 > 0.72 {
                index_of(SOIL_DARK)
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
    stratum: Stratum,
    (up, left, right): (Cell, Cell, Cell),
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
