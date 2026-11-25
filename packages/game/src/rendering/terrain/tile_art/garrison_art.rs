//! Descent VII part B's tiles and back walls (Wave 10 ancients-b: the Sleeping Garrison and
//! the Seal Gate).
//!
//! * War plate: olive armour plates on a riveted grid, a worn hazard chevron now and then.
//! * War bays: a dark recess behind a red lamp lens and shutter slats (the machines' doors).
//! * Hold pylons: a gunmetal column banded with amber light.
//! * Barricades: thick riveted plate with yellow-and-black edging.
//! * Seal stone: gilded ashlar, its joints picked out in gold.
//! * The seal door: great bronze-gold slabs with a vertical seam and engraved rings.
//! * The Heart Seal: deep crimson stone veined with white-hot light.
//! * Seal gold: dark rock with fat veins of gold.
//! * Collapse rubble: broken gilded blocks and dust.
//! * The cracked floor and vault: seal stone split by glowing cracks.
//! * Key altars: a gold plinth with a socket.
//! * Back walls: the Sleeping Garrison's dark olive bulkheads with stencilled numbers and
//!   pipes; the Seal Gate's warm umber stone with faint gilded glyph lines.

use fatherload_simulation::{
    cave::CaveTile,
    descent::Stratum,
    terrain::{Terrain, TilePosition},
    worldgen::descent::garrison as sites,
};

use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{Cell, Ramp, TILE_ART, TileCanvas, hash, noise, ramp};

/// The stratum at `position` if it is one of Descent VII part B's (`None` elsewhere or on
/// older worlds).
#[must_use]
pub(super) fn garrison_stratum(terrain: &Terrain, position: TilePosition) -> Option<Stratum> {
    if !terrain.settings().generator.has_descents() {
        return None;
    }
    let stratum = terrain.stratum_at(position);
    sites::is_garrison(stratum).then_some(stratum)
}

/// The material ramp of one of the tiles (edges and lips).
#[must_use]
pub const fn garrison_ramp(tile: CaveTile) -> Option<Ramp> {
    Some(match tile {
        CaveTile::WarPlate | CaveTile::WarBay | CaveTile::WarBarricade => {
            ramp([INK, PINE, MOSS, STONE_LIGHT])
        }
        CaveTile::HoldPylon => ramp([INK, STEEL_DARK, STEEL, AMBER]),
        CaveTile::SealStone
        | CaveTile::CrackedFloor
        | CaveTile::CrackedVault
        | CaveTile::CollapseRubble => ramp([SOIL_DARK, RUST, AMBER, GOLD]),
        CaveTile::SealDoor | CaveTile::KeyAltar => ramp([SOIL_DARK, COPPER, GOLD, CREAM]),
        CaveTile::HeartSeal => ramp([INK, MAROON, RED, CREAM]),
        CaveTile::SealGold => ramp([SOIL_DARK, SOIL, AMBER, GOLD]),
        _ => return None,
    })
}

/// Representative colour (drill dust, the minimap).
#[must_use]
#[allow(clippy::match_same_arms, reason = "one row per material")]
pub const fn garrison_color(tile: CaveTile) -> Option<Color> {
    Some(match tile {
        CaveTile::WarPlate | CaveTile::WarBarricade => MOSS,
        CaveTile::WarBay => RED,
        CaveTile::HoldPylon => AMBER,
        CaveTile::SealStone | CaveTile::CrackedFloor | CaveTile::CrackedVault => AMBER,
        CaveTile::CollapseRubble => RUST,
        CaveTile::SealDoor | CaveTile::KeyAltar | CaveTile::SealGold => GOLD,
        CaveTile::HeartSeal => RED,
        _ => return None,
    })
}

/// The glow of a lit tile (halos, the light overlay's tint).
#[must_use]
pub const fn garrison_glow(tile: CaveTile) -> Option<Color> {
    Some(match tile {
        CaveTile::WarBay => RED,
        CaveTile::HoldPylon => AMBER,
        CaveTile::KeyAltar | CaveTile::SealGold => GOLD,
        CaveTile::HeartSeal => PINK,
        _ => return None,
    })
}

/// One art pixel of `tile` at absolute art pixel `(x, y)`, if it is one of Descent VII part
/// B's.
#[must_use]
pub(super) fn garrison_pixel(tile: CaveTile, x: i32, y: i32) -> Option<PaletteIndex> {
    Some(match tile {
        CaveTile::WarPlate => war_plate_pixel(x, y),
        CaveTile::WarBay => war_bay_pixel(x, y),
        CaveTile::HoldPylon => pylon_pixel(x, y),
        CaveTile::WarBarricade => barricade_pixel(x, y),
        CaveTile::SealStone => seal_stone_pixel(x, y, false),
        CaveTile::CrackedFloor | CaveTile::CrackedVault => seal_stone_pixel(x, y, true),
        CaveTile::SealDoor => door_pixel(x, y),
        CaveTile::HeartSeal => heart_seal_pixel(x, y),
        CaveTile::SealGold => seal_gold_pixel(x, y),
        CaveTile::CollapseRubble => rubble_pixel(x, y),
        CaveTile::KeyAltar => altar_pixel(x, y),
        _ => return None,
    })
}

/// War plate: olive plates on a riveted 16-pixel grid, lit along their top, a worn hazard
/// chevron on some.
fn war_plate_pixel(x: i32, y: i32) -> PaletteIndex {
    let (gx, gy) = (x.rem_euclid(16), y.rem_euclid(16));
    if gx == 0 || gy == 0 {
        return index_of(INK);
    }
    if (gx == 2 || gx == 13) && (gy == 2 || gy == 13) {
        return index_of(STONE_LIGHT);
    }
    let plate = hash(x.div_euclid(16), y.div_euclid(16), 1_001);
    // Some plates carry a worn yellow-and-black chevron band.
    if plate.is_multiple_of(5) && (6..=9).contains(&gy) {
        let stripe = (gx + gy).rem_euclid(6) < 3;
        let worn = noise(x, y, 4, 1_003) > 0.72;
        if !worn {
            return index_of(if stripe { GOLD } else { INK });
        }
    }
    let grime = noise(x, y, 6, 1_005) + f32::from(bayer4(x, y)) / 50.0;
    if gy == 1 {
        index_of(LEAF)
    } else if gy >= 14 || gx >= 14 {
        index_of(PINE)
    } else if grime > 0.82 {
        index_of(SOIL)
    } else {
        index_of(MOSS)
    }
}

/// A war bay: a dark slatted recess behind a red lamp lens.
const fn war_bay_pixel(x: i32, y: i32) -> PaletteIndex {
    let (gx, gy) = (x.rem_euclid(16), y.rem_euclid(16));
    if gx == 0 || gx == 15 || gy == 0 || gy == 15 {
        return index_of(PINE);
    }
    if gx == 1 || gx == 14 || gy == 1 || gy == 14 {
        return index_of(MOSS);
    }
    // The red lens at the middle.
    let (dx, dy) = (gx - 8, gy - 8);
    let distance = dx * dx + dy * dy;
    if distance <= 4 {
        return index_of(if distance <= 1 { PINK } else { RED });
    }
    if distance <= 9 {
        return index_of(MAROON);
    }
    // Shutter slats.
    if gy.rem_euclid(3) == 0 {
        index_of(STEEL_DARK)
    } else {
        index_of(INK)
    }
}

/// A hold pylon: a gunmetal column banded with amber light, brighter at its core.
fn pylon_pixel(x: i32, y: i32) -> PaletteIndex {
    let gx = x.rem_euclid(16);
    let gy = y.rem_euclid(16);
    if gx <= 1 || gx >= 14 {
        return index_of(INK);
    }
    if gy.rem_euclid(8) == 3 {
        return index_of(if (6..=9).contains(&gx) { CREAM } else { AMBER });
    }
    if gy.rem_euclid(8) == 4 {
        return index_of(GOLD);
    }
    if gx <= 3 {
        index_of(STEEL)
    } else if gx >= 12 {
        index_of(STEEL_DARK)
    } else if (7..=8).contains(&gx) {
        index_of(STEEL_LIGHT)
    } else {
        index_of(STEEL)
    }
}

/// A barricade: thick riveted plate with a yellow-and-black edge.
const fn barricade_pixel(x: i32, y: i32) -> PaletteIndex {
    let (gx, gy) = (x.rem_euclid(16), y.rem_euclid(16));
    if gx == 0 || gy == 0 || gx == 15 || gy == 15 {
        return index_of(INK);
    }
    if gx <= 2 || gx >= 13 || gy <= 2 || gy >= 13 {
        return index_of(if (gx + gy).rem_euclid(4) < 2 {
            GOLD
        } else {
            INK
        });
    }
    if (gx == 4 || gx == 11) && (gy == 4 || gy == 11) {
        return index_of(STONE_LIGHT);
    }
    if gy == 3 {
        index_of(LEAF)
    } else if gy >= 11 {
        index_of(PINE)
    } else {
        index_of(MOSS)
    }
}

/// Seal stone: gilded ashlar blocks, gold joints; `cracked`: split by glowing cracks.
fn seal_stone_pixel(x: i32, y: i32, cracked: bool) -> PaletteIndex {
    let row = y.div_euclid(8);
    let shift = if row.rem_euclid(2) == 0 { 0 } else { 8 };
    let (bx, by) = ((x + shift).rem_euclid(16), y.rem_euclid(8));
    if cracked {
        // A jagged crack wanders through each block, lit from inside.
        let block = hash((x + shift).div_euclid(16), row, 1_011);
        let line = 3 + i32::try_from(block % 9).unwrap_or(0);
        let wander = (bx / 3 + i32::try_from((block >> 4) % 3).unwrap_or(0)).rem_euclid(3) - 1;
        if (by - (line % 8) - wander).abs() == 0 || (bx == line && by > 1) {
            return index_of(if hash(x, y, 1_013).is_multiple_of(3) {
                CREAM
            } else {
                ORANGE
            });
        }
    }
    if bx == 0 || by == 0 {
        return index_of(AMBER);
    }
    let wear = noise(x, y, 5, 1_015) + f32::from(bayer4(x, y)) / 40.0;
    if by == 1 {
        index_of(RUST)
    } else if by == 7 || bx == 15 {
        index_of(NIGHT)
    } else if wear > 0.85 {
        index_of(RUST)
    } else if wear > 0.45 {
        index_of(SOIL)
    } else {
        index_of(SOIL_DARK)
    }
}

/// The seal door: bronze-gold slabs, a seam down the middle, engraved rings.
fn door_pixel(x: i32, y: i32) -> PaletteIndex {
    let (gx, gy) = (x.rem_euclid(32), y.rem_euclid(32));
    if gx == 0 || gx == 31 {
        return index_of(SOIL_DARK);
    }
    if gx == 15 || gx == 16 {
        return index_of(INK);
    }
    let (dx, dy) = ((gx % 16) - 8, gy - 16);
    let ring = dx * dx + dy * dy;
    if (36..=44).contains(&ring) || (100..=112).contains(&ring) {
        return index_of(CREAM);
    }
    let sheen = noise(x, y, 8, 1_021) + f32::from(bayer4(x, y)) / 40.0;
    if gy.rem_euclid(16) == 0 {
        index_of(COPPER)
    } else if sheen > 0.7 {
        index_of(GOLD)
    } else {
        index_of(AMBER)
    }
}

/// The Heart Seal: deep crimson stone veined with white-hot light.
fn heart_seal_pixel(x: i32, y: i32) -> PaletteIndex {
    let vein = noise(x, y, 7, 1_031);
    let crack = (vein - 0.5).abs() < 0.035;
    if crack {
        return index_of(if hash(x, y, 1_033).is_multiple_of(2) {
            CREAM
        } else {
            PINK
        });
    }
    if (vein - 0.5).abs() < 0.08 {
        return index_of(RED);
    }
    let depth = noise(x, y, 4, 1_035) + f32::from(bayer4(x, y)) / 45.0;
    if depth > 0.7 {
        index_of(MAROON)
    } else {
        index_of(PLUM)
    }
}

/// Seal gold: dark rock with fat veins of gold and glints.
fn seal_gold_pixel(x: i32, y: i32) -> PaletteIndex {
    if hash(x, y, 1_041).is_multiple_of(41) {
        return index_of(CREAM);
    }
    let vein = noise(x, y, 6, 1_043);
    if vein > 0.66 {
        index_of(GOLD)
    } else if vein > 0.56 {
        index_of(AMBER)
    } else if noise(x, y, 3, 1_045) > 0.6 {
        index_of(SOIL)
    } else {
        index_of(SOIL_DARK)
    }
}

/// Collapse rubble: broken gilded blocks and grey dust.
const fn rubble_pixel(x: i32, y: i32) -> PaletteIndex {
    let chunk = hash(x.div_euclid(5), y.div_euclid(4), 1_051);
    let (cx, cy) = (x.rem_euclid(5), y.rem_euclid(4));
    if cx == 0 || cy == 0 {
        return index_of(if chunk.is_multiple_of(3) {
            STONE_DARK
        } else {
            SOIL_DARK
        });
    }
    match chunk % 5 {
        0 => index_of(GOLD),
        1 | 2 => index_of(RUST),
        3 => index_of(STONE),
        _ => index_of(if cy == 1 { AMBER } else { SOIL }),
    }
}

/// A key altar: a gold plinth with a dark socket.
fn altar_pixel(x: i32, y: i32) -> PaletteIndex {
    let (gx, gy) = (x.rem_euclid(16), y.rem_euclid(16));
    if gx == 0 || gx == 15 {
        return index_of(SOIL_DARK);
    }
    if gy <= 1 {
        return index_of(CREAM);
    }
    if (5..=10).contains(&gx) && (4..=9).contains(&gy) {
        return index_of(if gy == 4 || gx == 5 { INK } else { NIGHT });
    }
    if gx <= 2 {
        index_of(GOLD)
    } else if gx >= 13 {
        index_of(COPPER)
    } else {
        index_of(AMBER)
    }
}

/// One art pixel of `stratum`'s back wall at `(x, y)`.
#[allow(clippy::single_match_else, reason = "one arm per stratum")]
fn back_pixel(stratum: Stratum, x: i32, y: i32) -> PaletteIndex {
    match stratum {
        Stratum::SleepingGarrison => {
            // Dark olive bulkheads, a pipe run, stencilled bay numbers.
            let (gx, gy) = (x.rem_euclid(32), y.rem_euclid(24));
            if gx == 0 || gy == 0 {
                return index_of(INK);
            }
            if gy == 6 || gy == 7 {
                return index_of(if gy == 6 { STEEL_DARK } else { NIGHT });
            }
            let panel = hash(x.div_euclid(32), y.div_euclid(24), 1_061);
            if panel.is_multiple_of(4) && (12..=18).contains(&gy) && (10..=20).contains(&gx) {
                // A stencilled numeral block.
                let on = (gx + gy * 3 + i32::try_from(panel % 7).unwrap_or(0)).rem_euclid(4) == 0;
                if on {
                    return index_of(MOSS);
                }
            }
            if gx == 1 || gy == 1 {
                index_of(PINE)
            } else if hash(x, y, 1_063).is_multiple_of(97) {
                index_of(SLATE_DARK)
            } else {
                index_of(NIGHT)
            }
        }
        _ => {
            // Warm umber stone with faint gilded glyph lines.
            let glyph = hash(x.div_euclid(12), y.div_euclid(12), 1_071);
            let (gx, gy) = (x.rem_euclid(12), y.rem_euclid(12));
            if glyph.is_multiple_of(6)
                && ((gx == 3 && gy > 2 && gy < 10) || (gy == 6 && gx > 2 && gx < 9))
            {
                return index_of(RUST);
            }
            if noise(x, y, 10, 1_073) + f32::from(bayer4(x, y)) / 50.0 > 0.86 {
                index_of(SOIL)
            } else {
                index_of(SOIL_DARK)
            }
        }
    }
}

/// The Gatekeeper's hall's back wall: dark bronze with gold pilasters and a frieze.
const fn hall_pixel(x: i32, y: i32) -> PaletteIndex {
    let (gx, gy) = (x.rem_euclid(128), y.rem_euclid(64));
    if gx < 10 {
        // A fluted pilaster.
        return index_of(match gx {
            0 | 9 => NIGHT,
            1 | 8 => RUST,
            4 | 5 => GOLD,
            _ => AMBER,
        });
    }
    if gy < 3 {
        return index_of(if gy == 1 { AMBER } else { NIGHT });
    }
    if gy == 6 && (gx + y / 64).rem_euclid(6) < 3 {
        return index_of(RUST);
    }
    if hash(x, y, 1_081).is_multiple_of(61) {
        index_of(SOIL)
    } else {
        index_of(INK)
    }
}

/// Paints the open ground at `position` in one of the strata: its back wall (the Gatekeeper's
/// hall its own), shadowed under rock.
pub(super) fn paint_back(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    stratum: Stratum,
    up: Cell,
    left: Cell,
    right: Cell,
) {
    let settings = terrain.settings();
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let in_hall = stratum == Stratum::TheSealGate
        && sites::colossus_hall(settings, &layout)
            .is_some_and(|hall| sites::hall_inside(&hall).expanded(1).contains(position));
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
            } else if in_hall {
                hall_pixel(gx, gy)
            } else {
                back_pixel(stratum, gx, gy)
            };
            canvas.set(px, py, index);
        }
    }
}
