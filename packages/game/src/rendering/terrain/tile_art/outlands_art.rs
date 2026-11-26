//! Tile art for the outland worlds (Wave 13 planets-c, `fatherload_simulation::planet::outlands`):
//! the fungal, machine and dead planets, their open air and their back walls.
//!
//! * Fungal: violet spore loam with mould flecks, the white felt of the mycelium mat threaded
//!   with lilac hyphae, ribbed pale stalks, caps of rose flesh with cream spots (the Mother
//!   Cap's spots glow teal), soft gill fans, round puffballs, ribbed gillstone, glowcaps (a
//!   luminous mint dome on a pale stem), lumen moss specks, dense plum sporeheart with spore
//!   pockets, spore vents (a crusted mouth glowing at its heart) and gold truffles.
//! * Machine: riveted steel plate with panel seams, packed scrap of mixed metal, belts with
//!   rollers and chevrons pointing their way, girder pylons, beacon lamps, cogwork in gear
//!   teeth, the gears' toothed rims round a hub, cable conduit in bundles, reactor casing
//!   hazard-striped, relays dark and lit, the power seal's glowing bars, vault walls and the
//!   reactor heart.
//! * Dead: grey loose ash with embers long cold, compacted ash stone in thin beds, petrified
//!   stone in coursed blocks with lichen, petrified glass, urns, bone-packed ossuary stone,
//!   tomb brick, the tomb seal's carved door, a sarcophagus, black grave rock and pale
//!   soulstone, the throne relic.
//! * Every theme: the builders' dressed stone and glowing glyphs, caches, the lair seals (lit
//!   while their boss waits, cold once spent) and the reward vaults' doors.

#![allow(
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::manual_range_contains,
    clippy::match_same_arms,
    clippy::if_same_then_else,
    reason = "presentation code: art math reads top to bottom"
)]

use fatherload_simulation::{
    planet::{BodyGround, PlanetGround, PlanetThemeId, outlands::OutlandTile},
    terrain::{Terrain, TileKind, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, TRANSPARENT, bayer4, index_of};

use super::{
    Cell, Edges, Ramp, Strata, TILE_ART, TileCanvas, hash, noise, paint_edges, ramp, worley,
};

type Color = raylib::prelude::Color;

// The fungal world.
const SPORE_LOAM: Ramp = ramp([NIGHT, PLUM, VIOLET, LAVENDER]);
const MAT: Ramp = ramp([PLUM, LAVENDER, MIST, BONE]);
const HYPHAE: Ramp = ramp([PLUM, VIOLET, LAVENDER, PINK]);
const STALK: Ramp = ramp([STONE, FOG, BONE, CREAM]);
const CAP: Ramp = ramp([MAROON, RED, ROSE, PINK]);
const GILL: Ramp = ramp([PLUM, ROSE, PINK, CREAM]);
const GILLSTONE: Ramp = ramp([NIGHT, SLATE_DARK, SLATE, LAVENDER]);
const GLOW: Ramp = ramp([TEAL_DARK, TEAL, MINT, ICE]);
const SPOREHEART: Ramp = ramp([INK, NIGHT, PLUM, VIOLET]);
const TRUFFLE: Ramp = ramp([SOIL_DARK, RUST, AMBER, GOLD]);
// The machine world.
const PLATE: Ramp = ramp([STEEL_DARK, STEEL, STEEL_LIGHT, MIST]);
const SCRAP: Ramp = ramp([SOIL_DARK, RUST, STEEL_DARK, STEEL]);
const BELT: Ramp = ramp([INK, NIGHT, STEEL_DARK, AMBER]);
const COG: Ramp = ramp([NIGHT, STEEL_DARK, COPPER, AMBER]);
const CONDUIT: Ramp = ramp([INK, NIGHT, SLATE_DARK, TEAL_DARK]);
const CASING: Ramp = ramp([NIGHT, STEEL_DARK, STEEL, GOLD]);
const LAMP: Ramp = ramp([OCEAN, SKY, SKY_LIGHT, ICE]);
// The dead world.
const ASH: Ramp = ramp([STONE_DARK, STONE, STONE_LIGHT, FOG]);
const ASH_STONE: Ramp = ramp([SLATE_DARK, STONE_DARK, STONE, STONE_LIGHT]);
const PETRIFIED: Ramp = ramp([NIGHT, SLATE_DARK, SLATE, SLATE_LIGHT]);
const OSSUARY: Ramp = ramp([SOIL_DARK, STONE, FOG, BONE]);
const TOMB: Ramp = ramp([INK, SOIL_DARK, LOAM, CLAY]);
const GRAVE: Ramp = ramp([INK, INK, NIGHT, SLATE_DARK]);
const SOUL: Ramp = ramp([TEAL_DARK, MINT, ICE, BONE]);
// The builders.
const BUILDER: Ramp = ramp([NIGHT, SLATE_DARK, STEEL, CREAM]);

/// Representative colour of an outland tile (drill dust, debris, the map).
#[must_use]
pub const fn outland_color(tile: OutlandTile) -> Color {
    match tile {
        OutlandTile::SporeLoam => VIOLET,
        OutlandTile::MyceliumMat => MIST,
        OutlandTile::Hyphae => LAVENDER,
        OutlandTile::Stalk => BONE,
        OutlandTile::CapFlesh => ROSE,
        OutlandTile::Gills => PINK,
        OutlandTile::Puffball => CREAM,
        OutlandTile::Gillstone => SLATE,
        OutlandTile::Glowcap | OutlandTile::MotherCap => MINT,
        OutlandTile::Lumenmoss => TEAL,
        OutlandTile::Sporeheart => PLUM,
        OutlandTile::SporeVent => VIOLET,
        OutlandTile::Truffle | OutlandTile::SporeCrown => GOLD,
        OutlandTile::Scrapfill => RUST,
        OutlandTile::HullPlate | OutlandTile::PlateSeam => STEEL,
        OutlandTile::ConveyorWest | OutlandTile::ConveyorEast => STEEL_DARK,
        OutlandTile::Pylon => STEEL_DARK,
        OutlandTile::Beacon => SKY_LIGHT,
        OutlandTile::Cogwork | OutlandTile::Gear => COPPER,
        OutlandTile::Conduit => SLATE_DARK,
        OutlandTile::ReactorCasing => STEEL,
        OutlandTile::RelayOff => STEEL_DARK,
        OutlandTile::RelayOn | OutlandTile::PowerSeal => SKY,
        OutlandTile::VaultWall => NIGHT,
        OutlandTile::ReactorHeart => SKY_LIGHT,
        OutlandTile::AshDrift => STONE_LIGHT,
        OutlandTile::AshStone => STONE,
        OutlandTile::Petrified | OutlandTile::PetrifiedGlass => SLATE,
        OutlandTile::Urn => CLAY,
        OutlandTile::Ossuary => FOG,
        OutlandTile::TombBrick | OutlandTile::TombSeal => LOAM,
        OutlandTile::Sarcophagus => GOLD,
        OutlandTile::GraveRock => NIGHT,
        OutlandTile::Soulstone => MINT,
        OutlandTile::ThroneRelic => ICE,
        OutlandTile::Cache | OutlandTile::BuilderCache => STEEL_LIGHT,
        OutlandTile::BuilderStone => SLATE_LIGHT,
        OutlandTile::BuilderGlyph => GOLD,
        OutlandTile::BossSeal => RED,
        OutlandTile::BossSealSpent => STONE_DARK,
        OutlandTile::VaultDoor => AMBER,
    }
}

/// The outland world `terrain` is the ground of, if any.
#[must_use]
pub fn outland_planet(terrain: &Terrain) -> Option<&PlanetGround> {
    terrain.planet_ground().filter(|planet| {
        matches!(
            planet.params().theme,
            PlanetThemeId::FUNGAL | PlanetThemeId::MACHINE | PlanetThemeId::DEAD
        )
    })
}

/// The stratum at `position` (0-2), for back walls.
fn stratum_at(planet: &PlanetGround, position: TilePosition) -> usize {
    planet.stratum_index(position.x, position.y).min(2)
}

/// The back wall of an outland stratum at absolute art pixel `(x, y)`.
fn back_pixel(theme: PlanetThemeId, stratum: usize, x: i32, y: i32) -> PaletteIndex {
    let value = noise(x, y, 6, 3_011).mul_add(0.7, noise(x, y, 2, 3_012) * 0.3)
        + f32::from(bayer4(x, y)) / 60.0;
    match (theme, stratum) {
        (PlanetThemeId::FUNGAL, 0) => {
            // Behind the mat: dark felt with pale threads.
            if (x + (noise(x, y, 8, 3_013) * 5.0) as i32).rem_euclid(11) == 0 {
                index_of(SLATE)
            } else if value > 0.66 {
                index_of(NIGHT)
            } else {
                index_of(SLATE_DARK)
            }
        }
        (PlanetThemeId::FUNGAL, 1) => {
            // Grotto walls: deep violet with lumen specks.
            if hash(x, y, 3_014).is_multiple_of(61) {
                index_of(TEAL)
            } else if value > 0.66 {
                index_of(INK)
            } else {
                index_of(NIGHT)
            }
        }
        (PlanetThemeId::FUNGAL, _) => {
            if hash(x, y, 3_015).is_multiple_of(47) {
                index_of(PLUM)
            } else if value > 0.6 {
                index_of(INK)
            } else {
                index_of(NIGHT)
            }
        }
        (PlanetThemeId::MACHINE, 0) => {
            // Bulkhead panels behind the plates.
            let (px, py) = (x.rem_euclid(16), y.rem_euclid(16));
            if px == 0 || py == 0 {
                index_of(INK)
            } else if (px == 2 || px == 13) && (py == 2 || py == 13) {
                index_of(STEEL_DARK)
            } else if value > 0.7 {
                index_of(INK)
            } else {
                index_of(NIGHT)
            }
        }
        (PlanetThemeId::MACHINE, 1) => {
            // The works' dark, with copper pipe runs.
            if y.rem_euclid(12) == 5 || (x.rem_euclid(23) == 0 && y.rem_euclid(12) > 5) {
                index_of(SOIL_DARK)
            } else if value > 0.7 {
                index_of(INK)
            } else {
                index_of(NIGHT)
            }
        }
        (PlanetThemeId::MACHINE, _) => {
            // Cable trays faintly lit.
            if x.rem_euclid(6) == 0 && hash(x / 6, y / 24, 3_016).is_multiple_of(2) {
                index_of(TEAL_DARK)
            } else if value > 0.7 {
                index_of(INK)
            } else {
                index_of(NIGHT)
            }
        }
        (_, 0) => {
            if value > 0.66 {
                index_of(STONE_DARK)
            } else {
                index_of(SLATE_DARK)
            }
        }
        (_, 1) => {
            // The Necropolis: bones in the dark.
            if hash(x, y, 3_017).is_multiple_of(71) {
                index_of(FOG)
            } else if value > 0.66 {
                index_of(INK)
            } else {
                index_of(SOIL_DARK)
            }
        }
        _ => {
            if hash(x, y, 3_018).is_multiple_of(97) {
                index_of(TEAL_DARK)
            } else {
                index_of(INK)
            }
        }
    }
}

/// A [`Strata`] whose back wall is the outland stratum's (rounded corners, cave shadows).
fn outland_strata(strata: &Strata, theme: PlanetThemeId, stratum: usize) -> Strata {
    let mut out = *strata;
    out.back = [back_pixel(theme, stratum, 0, 0), index_of(INK)];
    out
}

/// Paints open air on an outland world: transparent under the sky, the strata's back walls
/// underground. `false` anywhere else.
pub(super) fn paint_outlands_air(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    up: Cell,
    left: Cell,
    right: Cell,
) -> bool {
    let Some(planet) = outland_planet(terrain) else {
        return false;
    };
    let theme = planet.params().theme;
    if position.y < planet.surface_row(position.x) {
        // A petrified tower's rooms keep a dark back wall of their old plaster and blocks.
        let room = theme == PlanetThemeId::DEAD
            && fatherload_simulation::planet::outlands::dead::in_tower(
                planet,
                i64::from(position.x),
                position.y,
            );
        for py in 0..TILE_ART {
            for px in 0..TILE_ART {
                let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
                let index = if room {
                    let course = gy.div_euclid(5);
                    let offset = if course.rem_euclid(2) == 0 { 0 } else { 5 };
                    if (gx + offset).rem_euclid(10) == 9 || gy.rem_euclid(5) == 4 {
                        index_of(INK)
                    } else if up == Cell::Solid && py < 3 {
                        index_of(INK)
                    } else if noise(gx, gy, 5, 3_021) > 0.7 {
                        index_of(NIGHT)
                    } else {
                        index_of(SLATE_DARK)
                    }
                } else {
                    TRANSPARENT
                };
                canvas.set(px, py, index);
            }
        }
        return true;
    }
    let stratum = stratum_at(planet, position);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let shadowed = (up == Cell::Solid && (py < 2 || (py < 4 && bayer4(gx, gy) < 6)))
                || (left == Cell::Solid && (px == 0 || (px == 1 && bayer4(gx, gy) < 8)))
                || (right == Cell::Solid
                    && (px == TILE_ART - 1 || (px == TILE_ART - 2 && bayer4(gx, gy) < 8)));
            let index = if shadowed {
                index_of(INK)
            } else {
                back_pixel(theme, stratum, gx, gy)
            };
            canvas.set(px, py, index);
        }
    }
    true
}

/// The rock ore sits in at `position` on an outland world (`None` elsewhere).
#[must_use]
pub(super) fn host_rock(terrain: &Terrain, position: TilePosition) -> Option<TileKind> {
    let planet = outland_planet(terrain)?;
    let theme = fatherload_simulation::planet::theme(planet.params().theme);
    if position.y < planet.surface_row(position.x) + theme.crust_rows {
        return Some(theme.crust);
    }
    Some(theme.strata[stratum_at(planet, position)].rock)
}

/// The tile's main ramp (its lips and shade).
const fn body_ramp(tile: OutlandTile) -> Ramp {
    match tile {
        OutlandTile::SporeLoam => SPORE_LOAM,
        OutlandTile::MyceliumMat => MAT,
        OutlandTile::Hyphae => HYPHAE,
        OutlandTile::Stalk => STALK,
        OutlandTile::CapFlesh | OutlandTile::MotherCap => CAP,
        OutlandTile::Gills | OutlandTile::Puffball => GILL,
        OutlandTile::Gillstone | OutlandTile::Lumenmoss => GILLSTONE,
        OutlandTile::Glowcap => GLOW,
        OutlandTile::Sporeheart | OutlandTile::SporeVent => SPOREHEART,
        OutlandTile::Truffle | OutlandTile::SporeCrown => TRUFFLE,
        OutlandTile::HullPlate | OutlandTile::PlateSeam => PLATE,
        OutlandTile::Scrapfill => SCRAP,
        OutlandTile::ConveyorWest | OutlandTile::ConveyorEast => BELT,
        OutlandTile::Pylon => PLATE,
        OutlandTile::Beacon | OutlandTile::RelayOn | OutlandTile::PowerSeal => LAMP,
        OutlandTile::Cogwork | OutlandTile::Gear => COG,
        OutlandTile::Conduit => CONDUIT,
        OutlandTile::ReactorCasing | OutlandTile::ReactorHeart => CASING,
        OutlandTile::RelayOff | OutlandTile::VaultWall => CONDUIT,
        OutlandTile::AshDrift => ASH,
        OutlandTile::AshStone => ASH_STONE,
        OutlandTile::Petrified | OutlandTile::PetrifiedGlass => PETRIFIED,
        OutlandTile::Urn | OutlandTile::TombBrick | OutlandTile::TombSeal => TOMB,
        OutlandTile::Ossuary => OSSUARY,
        OutlandTile::Sarcophagus => TRUFFLE,
        OutlandTile::GraveRock => GRAVE,
        OutlandTile::Soulstone | OutlandTile::ThroneRelic => SOUL,
        OutlandTile::Cache
        | OutlandTile::BuilderCache
        | OutlandTile::BuilderStone
        | OutlandTile::BuilderGlyph
        | OutlandTile::BossSeal
        | OutlandTile::BossSealSpent
        | OutlandTile::VaultDoor => BUILDER,
    }
}

/// Whether a tile has see-through parts (caps' rims, stalks, puffballs, glowcaps, pylons).
const fn see_through(tile: OutlandTile) -> bool {
    matches!(
        tile,
        OutlandTile::Puffball | OutlandTile::Glowcap | OutlandTile::Pylon | OutlandTile::Gills
    )
}

/// One art pixel of `tile`'s body at absolute art pixel `(x, y)` (tile pixel `(px, py)`);
/// `None` is see-through.
fn body_pixel(tile: OutlandTile, x: i32, y: i32, px: i32, py: i32) -> Option<PaletteIndex> {
    let n = |cell: i32, seed: u32| noise(x, y, cell, seed);
    let dither = f32::from(bayer4(x, y)) / 40.0;
    let level = |ramp: Ramp, value: f32| {
        ramp[if value < 0.3 {
            0
        } else if value < 0.55 {
            1
        } else if value < 0.8 {
            2
        } else {
            3
        }]
    };
    Some(match tile {
        // --- Fungal -----------------------------------------------------------------------
        OutlandTile::SporeLoam => {
            // Violet loam in soft clumps, flecks of pale mould and the odd teal spore.
            let value = n(5, 3_102) * 0.6 + n(2, 3_103) * 0.25 + dither * 0.5;
            if hash(x, y, 3_101).is_multiple_of(43) {
                index_of(MIST)
            } else if hash(x, y, 3_104).is_multiple_of(71) {
                index_of(TEAL)
            } else if value > 0.66 {
                SPORE_LOAM[2]
            } else if value < 0.3 {
                SPORE_LOAM[0]
            } else {
                SPORE_LOAM[1]
            }
        }
        OutlandTile::MyceliumMat => {
            // A pale felt: bright wavy threads crossing over lilac shadows, the odd knot.
            let wave = (n(9, 3_111) * 6.0) as i32;
            let thread = (x + y + wave).rem_euclid(5) == 0 || (x - y + wave).rem_euclid(7) == 0;
            let shadow = n(8, 3_113) * 0.8 + n(3, 3_114) * 0.2 + dither * 0.15;
            if hash(x, y, 3_112).is_multiple_of(61) {
                HYPHAE[2]
            } else if thread {
                MAT[3]
            } else if shadow > 0.74 {
                MAT[1]
            } else {
                MAT[2]
            }
        }
        OutlandTile::Hyphae => {
            // The mat knotted tight with lilac cords: thick twisting strands over pale felt.
            let wave = (n(10, 3_121) * 7.0) as i32;
            let cord = (x * 2 + y + wave).rem_euclid(7);
            if cord == 0 {
                HYPHAE[1]
            } else if cord == 1 {
                HYPHAE[2]
            } else if cord == 2 && hash(x, y, 3_122).is_multiple_of(3) {
                HYPHAE[3]
            } else if n(5, 3_123) > 0.7 {
                MAT[1]
            } else {
                MAT[2]
            }
        }
        OutlandTile::Stalk => {
            // A pale stalk with long ribs and a faint scale pattern.
            let rib = px.rem_euclid(4) == 0;
            if rib {
                STALK[1]
            } else if px.rem_euclid(4) == 1 {
                STALK[3]
            } else if hash(x / 2, y / 3, 3_131).is_multiple_of(9) {
                STALK[0]
            } else {
                STALK[2]
            }
        }
        OutlandTile::CapFlesh | OutlandTile::MotherCap => {
            // Rose flesh, darker in the hollows, cream spots (the Mother Cap's glow teal).
            let (near, _, dx, dy) = worley(x, y, 9, 3_141, false);
            let spot = near < 3.2 && hash(x - dx, y - dy, 3_142).is_multiple_of(2);
            if spot {
                if tile == OutlandTile::MotherCap {
                    if near < 1.5 {
                        index_of(ICE)
                    } else {
                        index_of(MINT)
                    }
                } else if near < 1.5 {
                    index_of(CREAM)
                } else {
                    CAP[3]
                }
            } else {
                // Smooth flesh: broad soft light and shade, a fine grain.
                let value = n(14, 3_143) * 0.8 + n(4, 3_144) * 0.2 + dither * 0.12;
                if value > 0.72 {
                    CAP[3]
                } else if value < 0.32 {
                    CAP[1]
                } else {
                    CAP[2]
                }
            }
        }
        OutlandTile::Gills => {
            // Thin radiating gill fans with dark gaps.
            if px.rem_euclid(3) == 2 {
                return None;
            }
            if py < 4 {
                GILL[2]
            } else if py < 10 {
                GILL[1]
            } else if py < 14 {
                GILL[0]
            } else {
                return None;
            }
        }
        OutlandTile::Puffball => {
            // A round cream puffball with warts, sitting on the ground.
            let (cx, cy) = (px - 8, py - 9);
            let inside = cx * cx + cy * cy * 2 <= 46;
            if !inside {
                return None;
            }
            if hash(x, y, 3_151).is_multiple_of(9) {
                GILL[1]
            } else if cx < -1 && cy < -1 {
                index_of(BONE)
            } else if cx > 3 || cy > 3 {
                GILL[2]
            } else {
                index_of(CREAM)
            }
        }
        OutlandTile::Gillstone => {
            // Ribbed grey-violet rock: curving ribs like the underside of a cap.
            let curve = (n(14, 3_161) * 8.0) as i32;
            let rib = (x + curve).rem_euclid(4);
            if hash(x, y, 3_162).is_multiple_of(89) {
                index_of(TEAL)
            } else if rib == 0 {
                GILLSTONE[0]
            } else if rib == 1 {
                GILLSTONE[3]
            } else if n(5, 3_163) > 0.6 {
                GILLSTONE[1]
            } else {
                GILLSTONE[2]
            }
        }
        OutlandTile::Glowcap => {
            // A glowing mint dome on a pale stem, bright at its crown.
            let (cx, cy) = (px - 8, py - 6);
            let dome = cy <= 1 && cx * cx + cy * cy * 3 <= 38;
            let stem = (cy > 1) && (cx.abs() <= 1) && py <= 14;
            if dome {
                if cy < -2 && cx.abs() < 3 {
                    GLOW[3]
                } else if cy == 1 {
                    GLOW[0]
                } else if hash(x, y, 3_171).is_multiple_of(5) {
                    index_of(ICE)
                } else {
                    GLOW[2]
                }
            } else if stem {
                if cx == -1 { STALK[3] } else { STALK[2] }
            } else {
                return None;
            }
        }
        OutlandTile::Lumenmoss => {
            // Gillstone with patches of glowing moss.
            let moss = n(4, 3_181) > 0.55;
            if moss && hash(x, y, 3_182).is_multiple_of(2) {
                if hash(x, y, 3_183).is_multiple_of(4) {
                    GLOW[3]
                } else {
                    GLOW[1]
                }
            } else {
                GILLSTONE[if n(5, 3_184) > 0.5 { 2 } else { 1 }]
            }
        }
        OutlandTile::Sporeheart => {
            // Dense plum rock with pockets of violet spores.
            let (near, second, _, _) = worley(x, y, 6, 3_191, false);
            if near < 2.0 && hash(x.div_euclid(6), y.div_euclid(6), 3_192).is_multiple_of(2) {
                if near < 0.8 {
                    index_of(LAVENDER)
                } else {
                    SPOREHEART[3]
                }
            } else if second - near < 1.0 {
                SPOREHEART[0]
            } else {
                level(SPOREHEART, n(5, 3_193) * 0.6 + dither * 0.5 + 0.2)
            }
        }
        OutlandTile::SporeVent => {
            // A crusted ring round a dark throat glowing violet deep down.
            let (cx, cy) = (px - 8, py - 8);
            let distance = cx * cx + cy * cy;
            if distance <= 6 {
                if distance <= 2 {
                    index_of(PINK)
                } else {
                    index_of(LAVENDER)
                }
            } else if distance <= 16 {
                index_of(INK)
            } else if distance <= 40 {
                if hash(x, y, 3_201).is_multiple_of(3) {
                    MAT[1]
                } else {
                    SPOREHEART[2]
                }
            } else {
                SPOREHEART[1]
            }
        }
        OutlandTile::Truffle | OutlandTile::SporeCrown => {
            // A gold lump in the rock (the crown: a gold ring of caps with a mint jewel).
            let (cx, cy) = (px - 8, py - 8);
            let lump = cx * cx * 3 + cy * cy * 4 <= 120;
            if tile == OutlandTile::SporeCrown {
                let ring = (5..=7).contains(&py) && cx.abs() <= 6;
                let points = py < 5 && py > 1 && cx.rem_euclid(3) == 0 && cx.abs() <= 6;
                let jewel = py == 6 && cx.abs() <= 1;
                if jewel {
                    return Some(index_of(MINT));
                }
                if ring || points {
                    return Some(if py == 5 { TRUFFLE[3] } else { TRUFFLE[2] });
                }
                return Some(SPOREHEART[1]);
            }
            if lump {
                if hash(x, y, 3_211).is_multiple_of(4) {
                    TRUFFLE[1]
                } else if cx < 0 && cy < 0 {
                    TRUFFLE[3]
                } else {
                    TRUFFLE[2]
                }
            } else {
                SPOREHEART[1]
            }
        }
        // --- Machine ------------------------------------------------------------------------
        OutlandTile::HullPlate => {
            // Riveted plate: a seam round each 8x8 panel, rivets at its corners, a soft sheen.
            let (qx, qy) = (x.rem_euclid(8), y.rem_euclid(8));
            if qx == 7 || qy == 7 {
                PLATE[0]
            } else if (qx == 1 || qx == 5) && (qy == 1 || qy == 5) {
                PLATE[3]
            } else if qx + qy < 4 {
                PLATE[2]
            } else if hash(x.div_euclid(8), y.div_euclid(8), 3_301).is_multiple_of(17)
                && n(3, 3_302) > 0.62
            {
                // A rust stain spreading on the odd panel.
                index_of(RUST)
            } else {
                PLATE[1]
            }
        }
        OutlandTile::PlateSeam => {
            // A welded seam: a bead of weld down the middle of the plate.
            if px == 7 || px == 8 {
                if py.rem_euclid(3) == 0 {
                    index_of(COPPER)
                } else {
                    index_of(AMBER)
                }
            } else if px == 6 || px == 9 {
                PLATE[0]
            } else {
                PLATE[1 + usize::from(py.rem_euclid(8) < 2)]
            }
        }
        OutlandTile::Scrapfill => {
            // Packed scrap: angular metal chunks, each one shade, lit on their upper edge,
            // rust between them and the odd bright bolt.
            let (near, second, dx, dy) = worley(x, y, 6, 3_311, true);
            let chunk = hash(x - dx, y - dy, 3_312) % 3;
            if second - near < 1.2 {
                SCRAP[0]
            } else if hash(x, y, 3_313).is_multiple_of(97) {
                index_of(STEEL_LIGHT)
            } else if dy < -1 {
                SCRAP[(chunk as usize + 1).min(3)]
            } else {
                SCRAP[(chunk as usize).min(2)]
            }
        }
        OutlandTile::ConveyorWest | OutlandTile::ConveyorEast => {
            // A belt over rollers: chevrons along its top pointing its way, rollers below.
            let east = tile == OutlandTile::ConveyorEast;
            if py <= 5 {
                let along = if east { px - py } else { px + py };
                if py == 0 {
                    BELT[2]
                } else if along.rem_euclid(6) < 2 {
                    BELT[3]
                } else {
                    BELT[1]
                }
            } else if py <= 7 {
                BELT[0]
            } else {
                let (cx, cy) = ((px % 8) - 4, py - 11);
                if cx * cx + cy * cy <= 6 {
                    if cx * cx + cy * cy <= 1 {
                        PLATE[3]
                    } else {
                        PLATE[1]
                    }
                } else {
                    PLATE[0]
                }
            }
        }
        OutlandTile::Pylon => {
            // Girders: two rails and their cross-bracing (open between).
            let rail = px <= 1 || px >= 14;
            let brace =
                (px - py.rem_euclid(16)).abs() <= 0 || (px + py.rem_euclid(16) - 15).abs() <= 0;
            if rail {
                if px == 0 || px == 15 {
                    PLATE[0]
                } else {
                    PLATE[2]
                }
            } else if brace {
                PLATE[1]
            } else {
                return None;
            }
        }
        OutlandTile::Beacon => {
            // A beacon lamp in a steel cage.
            let (cx, cy) = (px - 8, py - 8);
            let d = cx * cx + cy * cy;
            if d <= 9 {
                if d <= 2 { index_of(ICE) } else { LAMP[2] }
            } else if d <= 30 {
                if (cx + cy).rem_euclid(3) == 0 {
                    PLATE[0]
                } else {
                    LAMP[1]
                }
            } else {
                PLATE[if py < 8 { 2 } else { 1 }]
            }
        }
        OutlandTile::Cogwork => {
            // Packed cogwork: interlocking toothed wheels.
            let (cell_x, cell_y) = (x.div_euclid(10), y.div_euclid(10));
            let (cx, cy) = (x.rem_euclid(10) - 5, y.rem_euclid(10) - 5);
            let d = cx * cx + cy * cy;
            let tooth = (cx.abs() == 4 && cy.abs() <= 1) || (cy.abs() == 4 && cx.abs() <= 1);
            let _ = (cell_x, cell_y);
            if d <= 2 {
                COG[3]
            } else if d <= 10 {
                COG[2]
            } else if tooth || d <= 14 {
                COG[1]
            } else {
                COG[0]
            }
        }
        OutlandTile::Gear => {
            // One great gear's rim: heavy copper teeth over a dark hub band.
            let t = (x + y).rem_euclid(6);
            if t < 2 {
                COG[3]
            } else if t < 4 {
                COG[2]
            } else if (x - y).rem_euclid(6) == 0 {
                COG[0]
            } else {
                COG[1]
            }
        }
        OutlandTile::Conduit => {
            // Bundled cables in horizontal runs, clamps every so often.
            let run = y.div_euclid(3);
            let clamp = (x + run * 5).rem_euclid(16) == 0;
            if clamp {
                PLATE[2]
            } else if y.rem_euclid(3) == 0 {
                CONDUIT[0]
            } else if hash(run, x.div_euclid(16), 3_331).is_multiple_of(3) {
                CONDUIT[3]
            } else {
                CONDUIT[2]
            }
        }
        OutlandTile::ReactorCasing => {
            // Heavy casing with hazard stripes along its panels.
            let stripe = py >= 12 && (px + py).rem_euclid(6) < 3;
            if stripe {
                index_of(GOLD)
            } else if py == 11 || py == 15 {
                CASING[0]
            } else if px.rem_euclid(8) == 0 {
                CASING[1]
            } else {
                level(CASING, n(6, 3_341) * 0.4 + 0.45 + dither * 0.3).min(CASING[2])
            }
        }
        OutlandTile::RelayOff | OutlandTile::RelayOn => {
            // A relay: a socket of dark casing with a big lens, dark or lit.
            let lit = tile == OutlandTile::RelayOn;
            let (cx, cy) = (px - 8, py - 8);
            let d = cx * cx + cy * cy;
            if d <= 14 {
                if lit {
                    if d <= 3 { index_of(ICE) } else { LAMP[2] }
                } else if d <= 3 {
                    index_of(SLATE)
                } else {
                    index_of(STEEL_DARK)
                }
            } else if d <= 24 {
                index_of(if lit { SKY } else { INK })
            } else if px == 0 || py == 0 || px == 15 || py == 15 {
                CASING[0]
            } else {
                CASING[1]
            }
        }
        OutlandTile::PowerSeal => {
            // Glowing bars of a power seal between dark frames.
            if px.rem_euclid(4) == 0 {
                CONDUIT[0]
            } else if (py + x / 4).rem_euclid(8) < 2 {
                index_of(ICE)
            } else {
                LAMP[1]
            }
        }
        OutlandTile::VaultWall => {
            // Black armoured blocks.
            let (qx, qy) = (x.rem_euclid(16), y.rem_euclid(8));
            if qx == 0 || qy == 0 {
                index_of(INK)
            } else if qy == 1 {
                index_of(SLATE_DARK)
            } else {
                index_of(NIGHT)
            }
        }
        OutlandTile::ReactorHeart => {
            // The reactor heart: a blue-white core in a gold cage.
            let (cx, cy) = (px - 8, py - 8);
            let d = cx * cx + cy * cy;
            if d <= 8 {
                if d <= 2 {
                    index_of(BONE)
                } else {
                    index_of(ICE)
                }
            } else if d <= 20 {
                LAMP[1]
            } else if (cx.abs() == cy.abs()) || cx == 0 || cy == 0 {
                index_of(GOLD)
            } else {
                CASING[0]
            }
        }
        // --- Dead -----------------------------------------------------------------------------
        OutlandTile::AshDrift => {
            // Soft grey ash in ripples, a few dark cinders.
            let wave = (n(20, 3_401) * 4.0) as i32;
            let ripple = (y * 2 + x / 3 + wave).rem_euclid(7);
            if hash(x, y, 3_402).is_multiple_of(59) {
                index_of(NIGHT)
            } else if ripple == 0 {
                ASH[1]
            } else if ripple == 1 || n(5, 3_403) + dither > 0.9 {
                ASH[3]
            } else {
                ASH[2]
            }
        }
        OutlandTile::AshStone => {
            // Compacted ash in thin beds.
            let wave = (n(24, 3_411) * 3.0) as i32;
            let bed = (y + wave).div_euclid(4);
            if (y + wave).rem_euclid(4) == 0 && n(6, 3_413) > 0.35 {
                ASH_STONE[1]
            } else if hash(bed, x.div_euclid(13), 3_412).is_multiple_of(4) {
                ASH_STONE[3]
            } else {
                ASH_STONE[2]
            }
        }
        OutlandTile::Petrified => {
            // Coursed blocks of petrified stone, lichen at their edges.
            let course = y.div_euclid(5);
            let offset = if course.rem_euclid(2) == 0 { 0 } else { 5 };
            let (qx, qy) = ((x + offset).rem_euclid(10), y.rem_euclid(5));
            if qx == 9 || qy == 4 {
                PETRIFIED[0]
            } else if hash(x, y, 3_421).is_multiple_of(29) && (qx < 2 || qy == 3) {
                index_of(TEAL_DARK)
            } else if qy == 0 || qx == 0 {
                PETRIFIED[3]
            } else if n(4, 3_422) > 0.62 {
                PETRIFIED[1]
            } else {
                PETRIFIED[2]
            }
        }
        OutlandTile::PetrifiedGlass => {
            // A window turned to dull stone-glass with cracks.
            if px <= 1 || px >= 14 || py <= 1 || py >= 14 {
                PETRIFIED[2]
            } else if (px - py).rem_euclid(9) == 0 || (px + py).rem_euclid(11) == 0 {
                index_of(INK)
            } else if py < 6 {
                index_of(SLATE_LIGHT)
            } else {
                index_of(SLATE)
            }
        }
        OutlandTile::Urn => {
            // A clay urn with a dark band.
            let (cx, cy) = (px - 8, py - 9);
            let body = cx * cx * 4 + cy * cy * 5 <= 150 && py >= 4;
            let neck = (2..=4).contains(&py) && cx.abs() <= 2;
            if body || neck {
                if py == 8 || py == 9 {
                    TOMB[0]
                } else if cx < -2 {
                    TOMB[3]
                } else {
                    TOMB[2]
                }
            } else {
                return None;
            }
        }
        OutlandTile::Ossuary => {
            // Stone packed with old bones: pale knuckles and long bones in the dark.
            let (near, second, dx, dy) = worley(x, y, 5, 3_431, false);
            let long = (x + y * 2).rem_euclid(13) == 0 && n(6, 3_432) > 0.5;
            if long {
                OSSUARY[3]
            } else if near < 1.6 && hash((x + dx) / 5, (y + dy) / 5, 3_433).is_multiple_of(3) {
                OSSUARY[3]
            } else if second - near < 1.0 {
                OSSUARY[0]
            } else {
                OSSUARY[1]
            }
        }
        OutlandTile::TombBrick => {
            let course = y.div_euclid(4);
            let offset = if course.rem_euclid(2) == 0 { 0 } else { 3 };
            let (qx, qy) = ((x + offset).rem_euclid(6), y.rem_euclid(4));
            if qx == 5 || qy == 3 {
                TOMB[0]
            } else if qy == 0 {
                TOMB[3]
            } else {
                TOMB[2]
            }
        }
        OutlandTile::TombSeal => {
            // A carved stone door: a frame, a disc with a glyph.
            let (cx, cy) = (px - 8, py - 8);
            let d = cx * cx + cy * cy;
            if px <= 1 || px >= 14 {
                TOMB[0]
            } else if d <= 20 && d >= 12 {
                index_of(GOLD)
            } else if d <= 4 && (cx == 0 || cy == 0) {
                index_of(AMBER)
            } else {
                TOMB[1 + usize::from(py < 8)]
            }
        }
        OutlandTile::Sarcophagus => {
            // A gilded lid with a carved face.
            let edge = px == 0 || px == 15 || py == 0 || py == 15;
            if edge {
                TRUFFLE[0]
            } else if (5..=10).contains(&px) && (3..=6).contains(&py) {
                if py == 5 && (px == 6 || px == 9) {
                    index_of(INK)
                } else {
                    TRUFFLE[3]
                }
            } else if py.rem_euclid(4) == 0 {
                TRUFFLE[1]
            } else {
                TRUFFLE[2]
            }
        }
        OutlandTile::GraveRock => {
            // Black rock with fine silver fractures.
            let (near, second, _, _) = worley(x, y, 8, 3_441, false);
            if second - near < 0.7 {
                index_of(SLATE_DARK)
            } else if hash(x, y, 3_442).is_multiple_of(113) {
                index_of(SLATE)
            } else {
                level(GRAVE, n(6, 3_443) * 0.6 + dither * 0.5 + 0.2)
            }
        }
        OutlandTile::Soulstone | OutlandTile::ThroneRelic => {
            // Pale crystal glowing from inside (the relic: a carved crown of it).
            let (near, second, dx, dy) = worley(x, y, 5, 3_451, false);
            if tile == OutlandTile::ThroneRelic {
                let (cx, cy) = (px - 8, py - 8);
                if cx * cx + cy * cy <= 30 {
                    return Some(if cy < 0 && cx.abs() < 3 {
                        SOUL[3]
                    } else {
                        SOUL[2]
                    });
                }
                return Some(GRAVE[2]);
            }
            if second - near < 1.0 {
                SOUL[0]
            } else if dx + dy < 0 {
                SOUL[3]
            } else {
                SOUL[1 + usize::from(near < 2.0)]
            }
        }
        // --- The builders and the lairs ------------------------------------------------------
        OutlandTile::BuilderStone => {
            // Dressed stone in tall even blocks, a fine gold inlay line.
            let (qx, qy) = (x.rem_euclid(8), y.rem_euclid(12));
            if qx == 7 || qy == 11 {
                BUILDER[0]
            } else if qy == 5 {
                index_of(AMBER)
            } else if qx == 0 {
                BUILDER[3]
            } else {
                BUILDER[2]
            }
        }
        OutlandTile::BuilderGlyph => {
            // A builders' glyph: a ring and a spoked eye, glowing gold.
            let (cx, cy) = (px - 8, py - 8);
            let d = cx * cx + cy * cy;
            if (30..=44).contains(&d) || (d <= 4) || (cx == 0 && cy.abs() <= 5) {
                index_of(if d <= 4 { CREAM } else { GOLD })
            } else if (cy == 0 && cx.abs() <= 5) || (cx.abs() == cy.abs() && d <= 18) {
                index_of(AMBER)
            } else {
                BUILDER[1]
            }
        }
        OutlandTile::Cache | OutlandTile::BuilderCache => {
            // A steel strongbox with a glowing latch.
            if px <= 1 || px >= 14 || py <= 3 || py >= 14 {
                BUILDER[0]
            } else if (7..=8).contains(&px) && (7..=9).contains(&py) {
                index_of(if tile == OutlandTile::Cache {
                    MINT
                } else {
                    GOLD
                })
            } else if py == 4 {
                index_of(STEEL_LIGHT)
            } else {
                index_of(STEEL)
            }
        }
        OutlandTile::BossSeal | OutlandTile::BossSealSpent => {
            // A great seal on the lair's floor: a burning eye (cold once its boss fell).
            let spent = tile == OutlandTile::BossSealSpent;
            let (cx, cy) = (px - 8, py - 8);
            let d = cx * cx + cy * cy;
            if d <= 6 {
                index_of(if spent { SLATE_DARK } else { CREAM })
            } else if d <= 20 {
                index_of(if spent { STONE_DARK } else { RED })
            } else if d <= 36 {
                index_of(if spent { SLATE } else { ORANGE })
            } else {
                BUILDER[1]
            }
        }
        OutlandTile::VaultDoor => {
            // A vault door: heavy bars in a gold frame.
            if px <= 1 || px >= 14 {
                index_of(GOLD)
            } else if px.rem_euclid(4) == 2 {
                index_of(AMBER)
            } else {
                BUILDER[0]
            }
        }
    })
}

/// One art pixel of an outland rock (ore's host): its body, solid throughout.
#[must_use]
pub(super) fn rock_pixel(tile: OutlandTile, x: i32, y: i32) -> PaletteIndex {
    body_pixel(tile, x, y, x.rem_euclid(TILE_ART), y.rem_euclid(TILE_ART))
        .unwrap_or_else(|| body_ramp(tile)[1])
}

/// Paints outland tile `tile` at `position`.
pub(super) fn paint(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: OutlandTile,
    strata: &Strata,
    edges: Edges,
) {
    let planet = terrain.planet_ground();
    let theme = planet.map_or(PlanetThemeId::FUNGAL, |planet| planet.params().theme);
    let stratum = planet.map_or(0, |planet| stratum_at(planet, position));
    let sky = planet.is_some_and(|planet| position.y < planet.surface_row(position.x))
        || edges.up == Cell::Sky;
    let strata = outland_strata(strata, theme, stratum);
    let mut open = false;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let index = body_pixel(tile, gx, gy, px, py).unwrap_or_else(|| {
                open = true;
                if sky {
                    TRANSPARENT
                } else {
                    back_pixel(theme, stratum, gx, gy)
                }
            });
            canvas.set(px, py, index);
        }
    }
    if open || see_through(tile) {
        return;
    }
    let ramp = body_ramp(tile);
    let square = matches!(
        tile,
        OutlandTile::HullPlate
            | OutlandTile::PlateSeam
            | OutlandTile::ConveyorWest
            | OutlandTile::ConveyorEast
            | OutlandTile::ReactorCasing
            | OutlandTile::VaultWall
            | OutlandTile::PowerSeal
            | OutlandTile::RelayOff
            | OutlandTile::RelayOn
            | OutlandTile::Petrified
            | OutlandTile::TombBrick
            | OutlandTile::TombSeal
            | OutlandTile::BuilderStone
            | OutlandTile::VaultDoor
    );
    paint_edges(canvas, &strata, edges, ramp[3], ramp[0], square);
    // Mould on the fungal crust's open tops; ash dusting the dead world's.
    if edges.up == Cell::Sky {
        let (top, under) = match tile {
            OutlandTile::SporeLoam | OutlandTile::MyceliumMat => (MIST, LAVENDER),
            OutlandTile::AshDrift | OutlandTile::AshStone => (FOG, STONE_LIGHT),
            _ => return,
        };
        for px in 0..TILE_ART {
            let drift = (hash(canvas.origin_x + px, 0, 3_501) % 3) as i32;
            for py in 0..=drift {
                canvas.set(px, py, index_of(if py == 0 { top } else { under }));
            }
        }
    }
}

/// The light an outland tile gives (the light overlay's tint), if it glows.
#[must_use]
#[allow(dead_code, reason = "the round ground's halos")]
pub const fn outland_glow(tile: OutlandTile) -> Option<Color> {
    match tile {
        OutlandTile::Glowcap | OutlandTile::MotherCap | OutlandTile::Lumenmoss => Some(MINT),
        OutlandTile::Beacon | OutlandTile::RelayOn | OutlandTile::PowerSeal => Some(SKY_LIGHT),
        OutlandTile::ReactorHeart => Some(ICE),
        OutlandTile::BuilderGlyph | OutlandTile::VaultDoor | OutlandTile::Truffle => Some(GOLD),
        OutlandTile::BossSeal => Some(RED),
        OutlandTile::Soulstone | OutlandTile::ThroneRelic => Some(MINT),
        OutlandTile::SporeVent | OutlandTile::SporeCrown => Some(LAVENDER),
        _ => None,
    }
}
