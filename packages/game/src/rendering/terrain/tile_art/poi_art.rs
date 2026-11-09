//! Tile art for point-of-interest fixtures (see `fatherload_simulation::poi`).
//!
//! Built things read as built: vault walls are dressed blocks of dark alloy with brass inlay
//! (the inlay glows in the stratum's accent colour), wreckage is dented, weathered teal-grey
//! hull plating with rust, outposts are corrugated steel with a teal company stripe. Natural
//! formations take their band's palette: fossil shale is laminated stratum stone with shell and
//! bone imprints, geode shells are faceted crystal in a per-stratum ramp. Objects (crates,
//! consoles, doors, seals, switches, relays, the vault cache, lockers) are 16x16 stamps from
//! `sprites::poi` over one of those bases.

use fatherload_simulation::{
    poi::{FormationStyle, PoiKind, PoiTile, PoiVariant},
    terrain::{DeepStratum, Terrain, TileKind, TilePosition},
    worldgen::Band,
};
use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::{
    shade::{PaletteIndex, index_of},
    sprite::SpriteDef,
    sprites::poi as art,
};

use super::{
    Edges, Ramp, Strata, TILE_ART, TileCanvas, backdrop_pixel, hash, noise, paint_edges, ramp,
    slab_level,
};

/// The dark alloy of vault walls (deepest shade to highlight).
const VAULT_ALLOY: Ramp = ramp([INK, SLATE_DARK, STEEL_DARK, STEEL]);
/// Hull plating of wrecked rigs.
const HULL_STEEL: Ramp = ramp([STEEL_DARK, STEEL, STEEL_LIGHT, MIST]);
/// Corrugated outpost sheet.
const SHEET_STEEL: Ramp = ramp([SLATE_DARK, STEEL_DARK, STEEL, STEEL_LIGHT]);

/// The crystal a geode grows in `band`.
const fn crystal_ramp(band: Band) -> Ramp {
    match band {
        Band::Deep(DeepStratum::CrystalFaults) => ramp([DEEP_BLUE, SKY, SKY_LIGHT, ICE]),
        Band::Deep(DeepStratum::FossilOceans) => ramp([LOAM, CLAY, SAND, CREAM]),
        Band::Deep(DeepStratum::PressureCathedrals) => ramp([OCEAN, STEEL, SKY_LIGHT, ICE]),
        Band::Deep(DeepStratum::RadioactiveHollow) => ramp([PINE, LEAF, LIME, CREAM]),
        Band::Deep(DeepStratum::AncientMachineLayer) => ramp([RUST, COPPER, AMBER, GOLD]),
        Band::Deep(DeepStratum::MantleStormZone) => ramp([MAROON, RED, ORANGE, GOLD]),
        Band::Deep(DeepStratum::VoidGeodeFields) | Band::Basalt => {
            ramp([PLUM, VIOLET, LAVENDER, PINK])
        }
        _ => ramp([TEAL_DARK, TEAL, MINT, ICE]),
    }
}

/// Representative colour of a POI tile (drill dust, the minimap, world previews).
#[must_use]
pub const fn poi_color(tile: PoiTile, band: Band) -> Color {
    match tile {
        PoiTile::SupplyCrate => SAND,
        PoiTile::Wreckage => TEAL_DARK,
        PoiTile::SalvageCore => MINT,
        PoiTile::VaultWall | PoiTile::VaultSealVented => SLATE,
        PoiTile::VaultDoor | PoiTile::VaultSwitch { lit: false, .. } => GOLD,
        PoiTile::VaultSeal => RED,
        PoiTile::VaultSwitch { lit: true, .. } => LIME,
        PoiTile::VaultRelay { lit: false } => COPPER,
        PoiTile::VaultRelay { lit: true } => CREAM,
        PoiTile::VaultCache => LAVENDER,
        PoiTile::FossilShale => BONE,
        PoiTile::GeodeShell => match band {
            Band::Deep(DeepStratum::CrystalFaults) => SKY_LIGHT,
            Band::Deep(DeepStratum::MantleStormZone) => ORANGE,
            Band::Deep(DeepStratum::RadioactiveHollow) => LIME,
            Band::Deep(DeepStratum::AncientMachineLayer) => AMBER,
            _ => LAVENDER,
        },
        PoiTile::OutpostPlating => STEEL,
        PoiTile::OutpostSalvage => TEAL,
    }
}

/// Vault wall: big offset blocks of dark alloy, bevelled, some carrying a brass rivet or a
/// glyph glowing in the stratum's accent colour.
fn vault_wall_pixel(strata: &Strata, x: i32, y: i32) -> PaletteIndex {
    let row = y.div_euclid(8);
    let shifted = x + if row % 2 == 0 { 0 } else { 4 };
    let (column, bx, by) = (
        shifted.div_euclid(8),
        shifted.rem_euclid(8),
        y.rem_euclid(8),
    );
    if bx == 7 || by == 7 {
        return VAULT_ALLOY[0];
    }
    if by == 0 || bx == 0 {
        return VAULT_ALLOY[3];
    }
    if by == 6 || bx == 6 {
        return VAULT_ALLOY[1];
    }
    let block = hash(column, row, 61);
    let inlay = strata.accent.unwrap_or(index_of(GOLD));
    match block % 5 {
        // A brass rivet.
        0 if (bx, by) == (3, 3) => index_of(GOLD),
        0 if (bx, by) == (4, 4) => index_of(RUST),
        // A glyph: a glowing dot in a brass cross.
        1 if (bx, by) == (3, 3) => inlay,
        1 if (bx == 3 && (by == 1 || by == 5)) || (by == 3 && (bx == 1 || bx == 5)) => {
            index_of(AMBER)
        }
        _ => {
            if hash(x, y, 63).is_multiple_of(19) {
                VAULT_ALLOY[1]
            } else {
                VAULT_ALLOY[2]
            }
        }
    }
}

/// The back wall inside a vault: dark tiled panels with a faint brass grid.
pub(super) fn vault_floor_pixel(strata: &Strata, x: i32, y: i32) -> PaletteIndex {
    let (px, py) = (x.rem_euclid(16), y.rem_euclid(16));
    if px == 0 || py == 0 {
        return if (px + py) % 4 == 0 {
            index_of(RUST)
        } else {
            index_of(SLATE_DARK)
        };
    }
    if (px, py) == (8, 8) {
        return strata.accent.unwrap_or(index_of(GOLD));
    }
    if hash(x, y, 67).is_multiple_of(17) {
        index_of(INK)
    } else {
        index_of(NIGHT)
    }
}

/// Wreckage: weathered teal-grey hull panels with dark seams and rivets, flaking paint, rust
/// blooms and soot. No warm paint, so a wreck never reads as lava or a hot vent.
fn wreck_pixel(x: i32, y: i32) -> PaletteIndex {
    let (px, py) = (x.rem_euclid(10), y.rem_euclid(8));
    if px == 0 || py == 0 {
        return if (px == 0 && py == 4) || (py == 0 && px == 5) {
            HULL_STEEL[2]
        } else {
            index_of(SLATE_DARK)
        };
    }
    let soot = noise(x, y, 6, 79);
    let rust = noise(x, y, 4, 73);
    if soot > 0.66 {
        return index_of(if hash(x, y, 81).is_multiple_of(3) {
            STONE_DARK
        } else {
            NIGHT
        });
    }
    if rust > 0.66 {
        // Rust streaks: mostly deep rust, a few fresher copper flecks.
        return index_of(if hash(x, y, 77).is_multiple_of(5) {
            COPPER
        } else {
            RUST
        });
    }
    // Bare steel where the paint has flaked off.
    if noise(x, y, 3, 85) > 0.7 {
        return HULL_STEEL[usize::from(py < 4)];
    }
    // Faded teal company paint on each panel, lit at the top-left and shaded at the
    // bottom-right.
    index_of(if py == 1 || px == 1 {
        STEEL_LIGHT
    } else if py == 7 || px == 9 {
        TEAL_DARK
    } else if py >= 4 {
        STEEL_DARK
    } else if noise(x, y, 5, 87) > 0.45 {
        TEAL
    } else {
        STEEL
    })
}

/// Fossil shale: pale laminations (the same sandy bed in every band) over the band's darkest
/// stone, flecked with bone.
fn shale_pixel(strata: &Strata, x: i32, y: i32) -> PaletteIndex {
    let warp = (noise(x, y, 9, 81) * 4.0) as i32;
    match (y + warp).rem_euclid(5) {
        _ if hash(x, y, 83).is_multiple_of(29) => index_of(BONE),
        0 => strata.stone[0],
        1 => index_of(SAND),
        4 => index_of(SOIL),
        _ => index_of(LOAM),
    }
}

/// Geode shell: crystal facets in the band's crystal ramp with bright glints.
fn geode_pixel(band: Band, x: i32, y: i32) -> PaletteIndex {
    let crystal = crystal_ramp(band);
    if hash(x, y, 91).is_multiple_of(37) {
        return index_of(BONE);
    }
    crystal[slab_level(x + 3, y + 5)]
}

/// Corrugated outpost sheet with rivet rows, rust streaks and a teal company stripe.
fn plating_pixel(x: i32, y: i32, local_y: i32) -> PaletteIndex {
    if local_y == 1 || local_y == 14 {
        return if x.rem_euclid(4) == 1 {
            SHEET_STEEL[3]
        } else {
            SHEET_STEEL[0]
        };
    }
    if local_y == 6 || local_y == 7 {
        return index_of(if local_y == 6 { TEAL } else { TEAL_DARK });
    }
    let streak = noise(x, y.div_euclid(3), 2, 95) > 0.78 && noise(x, y, 5, 97) > 0.45;
    if streak {
        return index_of(RUST);
    }
    SHEET_STEEL[[1, 2, 3, 2][x.rem_euclid(4) as usize]]
}

fn fill(canvas: &mut TileCanvas, pixel: impl Fn(i32, i32, i32, i32) -> PaletteIndex) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, pixel(gx, gy, px, py));
        }
    }
}

/// Crystals lining the hollow of a geode: on an open tile inside a geode, every side that
/// touches the shell grows a row of faceted points in the band's crystal ramp (dark at the
/// root, a bright glint at the tip), so the hollow reads as a crystal-lined cavity rather than
/// a plain cave.
pub(super) fn paint_geode_lining(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    band: Band,
) {
    if !terrain.settings().generator.has_points_of_interest() {
        return;
    }
    let in_geode = terrain.features().poi_at(position).is_some_and(|poi| {
        // Only the hollow: the ring of tiles just inside the bounds is shell or outside rock.
        poi.poi() == Some(PoiVariant::Formation(FormationStyle::Geode))
            && poi.bounds.expanded(-2).contains(position)
    });
    if !in_geode {
        return;
    }
    let crystal = crystal_ramp(band);
    let shell = |dx: i32, dy: i32| {
        terrain
            .tile(TilePosition {
                x: position.x + dx,
                y: position.y + dy,
            })
            .is_some_and(|tile| {
                matches!(
                    tile.kind,
                    TileKind::Poi(PoiTile::GeodeShell) | TileKind::Ore(_)
                )
            })
    };
    // (neighbour offset, side index) for floor, ceiling, left and right walls.
    for (dx, dy, side) in [(0, 1, 0_u32), (0, -1, 1), (-1, 0, 2), (1, 0, 3)] {
        if !shell(dx, dy) {
            continue;
        }
        let salt = 151 + 2 * side;
        let seed = u64::from(hash(position.x, position.y, salt))
            | (u64::from(hash(position.x, position.y, salt + 1)) << 32);
        // Three or four points per side, at hashed offsets along it.
        for point in 0..4_u64 {
            if point == 3 && (seed >> 60) % 2 == 0 {
                continue;
            }
            let along = 2 + ((seed >> (point * 12)) % 12) as i32;
            let length = 5 + ((seed >> (point * 12 + 6)) % 6) as i32;
            for depth in 0..length {
                // A faceted point: five pixels wide at the root narrowing to one at the tip.
                let half = (length - 1 - depth) * 5 / (2 * length);
                for offset in -half..=half {
                    let shade = if depth >= length - 2 && offset == 0 {
                        crystal[3]
                    } else if offset == -half && half > 0 {
                        crystal[0]
                    } else if offset < 0 {
                        crystal[2]
                    } else if offset == 0 {
                        crystal[3]
                    } else {
                        crystal[1]
                    };
                    let (px, py) = match side {
                        0 => (along + offset, TILE_ART - 1 - depth),
                        1 => (along + offset, depth),
                        2 => (depth, along + offset),
                        _ => (TILE_ART - 1 - depth, along + offset),
                    };
                    canvas.set(px, py, shade);
                }
            }
        }
    }
}

/// Whether the open tile at `position` lies inside a vault (its rooms and antechambers).
pub(super) fn inside_vault(terrain: &Terrain, position: TilePosition) -> bool {
    terrain.settings().generator.has_points_of_interest()
        && terrain.features().poi_at(position).is_some_and(|poi| {
            poi.poi_kind() == Some(PoiKind::Vault) && poi.bounds.expanded(-1).contains(position)
        })
}

/// A switch plate sprite for an order and state.
const fn switch_sprite(order: u8, lit: bool) -> &'static SpriteDef {
    match (order, lit) {
        (0 | 1, false) => &art::SWITCH_1,
        (2, false) => &art::SWITCH_2,
        (_, false) => &art::SWITCH_3,
        (0 | 1, true) => &art::SWITCH_1_LIT,
        (2, true) => &art::SWITCH_2_LIT,
        (_, true) => &art::SWITCH_3_LIT,
    }
}

/// Paints one POI tile.
pub(super) fn paint(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: PoiTile,
    strata: &Strata,
    edges: Edges,
) {
    let band = terrain.band_at(position);
    let seed = hash(position.x, position.y, 107);
    // Base layer, and the lip/shade for faces that meet open air (`None`: the stamp carries its
    // own frame).
    let faces: Option<(PaletteIndex, PaletteIndex, bool)> = match tile {
        PoiTile::VaultWall
        | PoiTile::VaultSeal
        | PoiTile::VaultSealVented
        | PoiTile::VaultSwitch { .. }
        | PoiTile::VaultRelay { .. } => {
            fill(canvas, |x, y, _, _| vault_wall_pixel(strata, x, y));
            Some((VAULT_ALLOY[3], VAULT_ALLOY[0], true))
        }
        PoiTile::Wreckage => {
            fill(canvas, |x, y, _, _| wreck_pixel(x, y));
            Some((index_of(STEEL_LIGHT), index_of(SLATE_DARK), false))
        }
        PoiTile::FossilShale => {
            fill(canvas, |x, y, _, _| shale_pixel(strata, x, y));
            Some((index_of(BONE), strata.stone[0], false))
        }
        PoiTile::GeodeShell => {
            fill(canvas, |x, y, _, _| geode_pixel(band, x, y));
            let crystal = crystal_ramp(band);
            Some((crystal[3], crystal[0], false))
        }
        PoiTile::OutpostPlating => {
            fill(canvas, |x, y, _, local_y| plating_pixel(x, y, local_y));
            Some((SHEET_STEEL[3], SHEET_STEEL[0], true))
        }
        PoiTile::VaultCache => {
            fill(canvas, |x, y, _, _| backdrop_pixel(strata, x, y));
            None
        }
        PoiTile::SupplyCrate
        | PoiTile::SalvageCore
        | PoiTile::VaultDoor
        | PoiTile::OutpostSalvage => None,
    };
    let stamp: Option<&SpriteDef> = match tile {
        PoiTile::SupplyCrate => Some(&art::CRATE),
        PoiTile::SalvageCore => Some(&art::SALVAGE_CORE),
        PoiTile::VaultDoor => Some(&art::VAULT_DOOR),
        PoiTile::VaultSeal => Some(&art::VAULT_SEAL),
        PoiTile::VaultSealVented => Some(&art::VAULT_SEAL_VENTED),
        PoiTile::VaultSwitch { order, lit } => Some(switch_sprite(order, lit)),
        PoiTile::VaultRelay { lit } => Some(if lit { &art::RELAY_LIT } else { &art::RELAY }),
        PoiTile::VaultCache => Some(&art::VAULT_CACHE),
        PoiTile::OutpostSalvage => Some(&art::LOCKER),
        PoiTile::FossilShale => {
            // Every shale tile carries an imprint.
            Some(if seed.is_multiple_of(3) {
                &art::BONE_IMPRINT
            } else {
                &art::SHELL_IMPRINT
            })
        }
        _ => None,
    };
    if tile.is_vault_fixture() {
        paint_vault_trim(canvas, terrain, position);
    }
    if let Some((lip, shade, square)) = faces {
        paint_edges(canvas, strata, edges, lip, shade, square);
    }
    // Fixtures go on top of the frame so a plate or valve is never cut by the outline.
    if let Some(stamp) = stamp {
        let (x, y) = if stamp.width() >= TILE_ART {
            (0, 0)
        } else {
            (
                2 + (seed >> 4) as i32 % (TILE_ART - stamp.width() - 3),
                2 + (seed >> 8) as i32 % (TILE_ART - stamp.height() - 3),
            )
        };
        canvas.stamp(stamp, x, y);
    }
}

/// Whether the tile at `(x, y)` is part of a vault's shell.
fn is_vault_shell(terrain: &Terrain, x: i32, y: i32) -> bool {
    terrain
        .tile(TilePosition { x, y })
        .is_some_and(|tile| matches!(tile.kind, TileKind::Poi(poi) if poi.is_vault_fixture() || poi == PoiTile::VaultDoor))
}

/// A brass band just inside every face of a vault's shell that meets something else, so the
/// whole structure reads as one framed, built thing.
fn paint_vault_trim(canvas: &mut TileCanvas, terrain: &Terrain, position: TilePosition) {
    let (x, y) = (position.x, position.y);
    let (gold, amber) = (index_of(GOLD), index_of(AMBER));
    let last = TILE_ART - 1;
    if !is_vault_shell(terrain, x, y - 1) {
        for px in 0..TILE_ART {
            canvas.set(px, 1, gold);
        }
    }
    if !is_vault_shell(terrain, x, y + 1) {
        for px in 0..TILE_ART {
            canvas.set(px, last - 1, amber);
        }
    }
    if !is_vault_shell(terrain, x - 1, y) {
        for py in 0..TILE_ART {
            canvas.set(1, py, gold);
        }
    }
    if !is_vault_shell(terrain, x + 1, y) {
        for py in 0..TILE_ART {
            canvas.set(last - 1, py, amber);
        }
    }
}
