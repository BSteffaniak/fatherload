//! Tile art for cave ecosystems (see `fatherload_simulation::cave`).
//!
//! Each cave kind has its own look: crystal geodes glitter in a per-band crystal ramp, fungal
//! grottos are plum mycelium with pale stalks, rose caps and teal glowcaps, lava tubes are
//! glossy obsidian, ancient ruins are dressed sandstone with teal glyphs, flooded caves hold
//! deep blue water over silt and glowing algae. The air inside a cave gets the cave's own back
//! wall (crystal sparkle, fungal haze, ruin brickwork, warm tube walls) so a cave reads as a
//! place even before its lights are seen. Partial tiles (stalks, caps, glowcaps, stalactites,
//! pillars, caches) paint that back wall first and their object over it.

use fatherload_simulation::{
    cave::{CaveKind, CaveTile},
    terrain::{DeepStratum, Terrain, TileKind, TilePosition},
    worldgen::Band,
};
use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{
    Cell, Edges, Ramp, Strata, TILE_ART, TileCanvas, backdrop_pixel, hash, noise, paint_edges,
    ramp, slab_level,
};

/// Glossy volcanic glass.
const OBSIDIAN: Ramp = ramp([INK, NIGHT, PLUM, LAVENDER]);
/// Weathered sandstone of the ancient builders.
const SANDSTONE: Ramp = ramp([SOIL_DARK, LOAM, CLAY, SAND]);
/// Fibrous fungal soil.
const MYCELIUM: Ramp = ramp([NIGHT, PLUM, VIOLET, LAVENDER]);
/// Mushroom stalks.
const STALK: Ramp = ramp([LOAM, SAND, CREAM, BONE]);
/// Mushroom caps.
const CAP: Ramp = ramp([MAROON, ROSE, PINK, CREAM]);
/// Lake bed silt.
const SILT: Ramp = ramp([SOIL_DARK, SOIL, LOAM, SAND]);
/// Cave water, deep to surface.
const WATER: Ramp = ramp([NIGHT, DEEP_BLUE, OCEAN, SKY]);

/// The crystal a cave grows in `band`.
const fn crystal_ramp(band: Band) -> Ramp {
    match band {
        Band::Deep(DeepStratum::CrystalFaults | DeepStratum::PressureCathedrals) => {
            ramp([DEEP_BLUE, SKY, SKY_LIGHT, ICE])
        }
        Band::Deep(DeepStratum::RadioactiveHollow) => ramp([PINE, LEAF, LIME, CREAM]),
        Band::Deep(DeepStratum::AncientMachineLayer) => ramp([RUST, COPPER, AMBER, GOLD]),
        Band::Deep(DeepStratum::MantleStormZone) => ramp([MAROON, RED, ORANGE, GOLD]),
        Band::Deep(DeepStratum::VoidGeodeFields) | Band::Basalt => {
            ramp([PLUM, VIOLET, LAVENDER, PINK])
        }
        Band::Slate => ramp([DEEP_BLUE, VIOLET, LAVENDER, ICE]),
        _ => ramp([TEAL_DARK, TEAL, MINT, ICE]),
    }
}

/// The glow colour of a lit cave tile in `band` (halos, the light overlay's tint).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "the far Strange Layers' tiles keep rows of their own"
)]
pub const fn cave_glow(tile: CaveTile, band: Band) -> Option<Color> {
    match tile {
        CaveTile::Crystal => Some(match band {
            Band::Deep(DeepStratum::RadioactiveHollow) => LIME,
            Band::Deep(DeepStratum::AncientMachineLayer | DeepStratum::MantleStormZone) => AMBER,
            Band::Deep(DeepStratum::VoidGeodeFields) | Band::Basalt => LAVENDER,
            Band::Deep(DeepStratum::CrystalFaults | DeepStratum::PressureCathedrals) => SKY_LIGHT,
            _ => MINT,
        }),
        CaveTile::GlowCap => Some(MINT),
        CaveTile::RuinGlyph => Some(TEAL),
        CaveTile::GlowAlgae | CaveTile::SporePod | CaveTile::AcidGland => Some(LIME),
        CaveTile::TreasureCache | CaveTile::RelicCache => Some(GOLD),
        CaveTile::JellyCell => Some(CREAM),
        // The far Strange Layers (Wave 9): coloured glass and chronoglass glow.
        CaveTile::PrismRed => Some(RED),
        CaveTile::PrismGreen => Some(LIME),
        CaveTile::PrismBlue => Some(SKY),
        CaveTile::Chronoglass => Some(GOLD),
        _ => match super::garrison_art::garrison_glow(tile) {
            Some(glow) => Some(glow),
            None => super::drowned_art::drowned_glow(tile),
        },
    }
}

/// Representative colour of a cave tile (drill dust, the minimap, world previews).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "the Living Dark's tiles keep rows of their own"
)]
pub const fn cave_color(tile: CaveTile, band: Band) -> Color {
    match tile {
        CaveTile::Crystal => match cave_glow(tile, band) {
            Some(color) => color,
            None => MINT,
        },
        CaveTile::MushroomStem => SAND,
        CaveTile::MushroomCap => ROSE,
        CaveTile::GlowCap => MINT,
        CaveTile::Mycelium => VIOLET,
        CaveTile::Obsidian => PLUM,
        CaveTile::RuinBrick | CaveTile::RuinPillar => CLAY,
        CaveTile::RuinGlyph => TEAL,
        CaveTile::Stalactite => STONE,
        CaveTile::TreasureCache => GOLD,
        CaveTile::RelicCache => AMBER,
        CaveTile::FossilRib => BONE,
        CaveTile::GlowAlgae => LIME,
        CaveTile::Silt => LOAM,
        CaveTile::Root => LOAM,
        CaveTile::CharredRoot => NIGHT,
        CaveTile::Flesh => ROSE,
        CaveTile::Comb => AMBER,
        CaveTile::JellyCell => CREAM,
        CaveTile::SporePod => VIOLET,
        CaveTile::AcidGland => MAROON,
        // The far Strange Layers (Wave 9).
        CaveTile::EchoQuartz => SKY_LIGHT,
        CaveTile::PrismRed => RED,
        CaveTile::PrismGreen => LIME,
        CaveTile::PrismBlue => SKY,
        CaveTile::BuilderAlloy => STEEL_DARK,
        CaveTile::Chronoglass => AMBER,
        // The Drowned and Frozen (Wave 9) and Descent VII part B (Wave 10).
        _ => match super::drowned_art::drowned_color(tile) {
            Some(color) => color,
            None => match super::garrison_art::garrison_color(tile) {
                Some(color) => color,
                None => STONE,
            },
        },
    }
}

/// Map colour of a cave kind (markers, the worldgen preview).
#[must_use]
pub const fn cave_kind_color(kind: CaveKind) -> Color {
    match kind {
        CaveKind::GrandCavern => STONE_LIGHT,
        CaveKind::CrystalGeode => ICE,
        CaveKind::FloodedCave => SKY,
        CaveKind::LavaTube => ORANGE,
        CaveKind::FungalGrotto => PINK,
        CaveKind::WormBurrows => BONE,
        CaveKind::AncientRuins => SAND,
        CaveKind::Chasm => LAVENDER,
        CaveKind::CaveNetwork => MINT,
    }
}

/// The back wall of an open tile inside a cave of `kind`.
fn cave_back_pixel(
    kind: Option<CaveKind>,
    strata: &Strata,
    band: Band,
    x: i32,
    y: i32,
) -> PaletteIndex {
    let base = backdrop_pixel(strata, x, y);
    match kind {
        Some(CaveKind::CrystalGeode) => {
            if hash(x, y, 401).is_multiple_of(71) {
                crystal_ramp(band)[1 + (hash(x, y, 403) % 3) as usize]
            } else {
                base
            }
        }
        Some(CaveKind::FungalGrotto) => {
            let haze = noise(x, y, 7, 405);
            if hash(x, y, 407).is_multiple_of(113) {
                index_of(MINT)
            } else if haze > 0.62 {
                index_of(PLUM)
            } else if haze > 0.3 {
                index_of(NIGHT)
            } else {
                index_of(INK)
            }
        }
        Some(CaveKind::LavaTube) => {
            if hash(x, y, 409).is_multiple_of(97) {
                index_of(RUST)
            } else if noise(x, y, 9, 411) > 0.74 {
                index_of(MAROON)
            } else {
                index_of(NIGHT)
            }
        }
        Some(CaveKind::AncientRuins) => {
            let row = y.div_euclid(6);
            let shifted = x + if row % 2 == 0 { 0 } else { 6 };
            if y.rem_euclid(6) == 5 || shifted.rem_euclid(12) == 11 {
                index_of(INK)
            } else if noise(x, y, 5, 413) > 0.7 {
                index_of(SOIL)
            } else {
                index_of(SOIL_DARK)
            }
        }
        Some(CaveKind::WormBurrows) => {
            // Faint rings left by the worm's segments.
            if (x + y / 2).rem_euclid(9) == 0 && bayer4(x, y) < 10 {
                strata.back[1]
            } else {
                base
            }
        }
        _ => base,
    }
}

/// Water in a flooded cave: dark depths, lighter shafts, a bright rippling surface.
fn water_pixel(x: i32, y: i32, depth_px: i32) -> PaletteIndex {
    if depth_px == 0 {
        return index_of(if hash(x, 0, 415).is_multiple_of(4) {
            ICE
        } else {
            SKY_LIGHT
        });
    }
    if depth_px == 1 {
        return WATER[3];
    }
    let shaft = noise(x, y, 9, 417) + f32::from(bayer4(x, y)) / 40.0;
    if hash(x, y, 419).is_multiple_of(97) {
        return index_of(SKY_LIGHT);
    }
    if depth_px < 6 && shaft > 0.55 {
        WATER[3]
    } else if shaft > 0.6 {
        WATER[2]
    } else {
        WATER[1]
    }
}

/// Paints an open cave tile: its cave's back wall, water where the cave is flooded. Returns
/// whether it painted (the tile is inside a cave).
pub(super) fn paint_cave_air(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    strata: &Strata,
    band: Band,
    up: Cell,
) -> bool {
    let Some(sample) = terrain.cave_at(position) else {
        return false;
    };
    let water = terrain.water_at(position);
    let surface = water
        && !terrain.water_at(TilePosition {
            x: position.x,
            y: position.y - 1,
        });
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let mut index = if up == Cell::Solid && (py < 2 || (py < 4 && bayer4(gx, gy) < 6)) {
                strata.back[1]
            } else {
                cave_back_pixel(Some(sample.kind), strata, band, gx, gy)
            };
            if water {
                let depth = if surface { py - 3 } else { py + TILE_ART };
                if depth >= 0 {
                    index = water_pixel(gx, gy, depth);
                }
            }
            canvas.set(px, py, index);
        }
    }
    true
}

/// The back wall a partial cave tile at `position` stands in front of.
fn paint_back(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    strata: &Strata,
    band: Band,
) {
    let kind = terrain.cave_kind_at(position);
    // Decor under water (a sunken cache, a cap in a lake) stands in water when water is above
    // it or on both sides.
    let water_near = |dx: i32, dy: i32| {
        terrain.water_at(TilePosition {
            x: position.x + dx,
            y: position.y + dy,
        })
    };
    let water = water_near(0, -1) || (water_near(-1, 0) && water_near(1, 0));
    // Outside a cave ecosystem, the Living Dark's own wall stands behind (Wave 8), and the
    // Drowned and Frozen's (Wave 9).
    let living = kind
        .is_none()
        .then(|| super::living_art::living_stratum(terrain, position))
        .flatten();
    let drowned = kind
        .is_none()
        .then(|| super::drowned_art::drowned_stratum(terrain, position))
        .flatten();
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let index = if water {
                water_pixel(gx, gy, py + TILE_ART)
            } else if let Some(stratum) = living {
                super::living_art::back_pixel(stratum, gx, gy)
            } else if let Some(stratum) = drowned {
                super::drowned_art::back_pixel(stratum, gx, gy)
            } else {
                cave_back_pixel(kind, strata, band, gx, gy)
            };
            canvas.set(px, py, index);
        }
    }
}

/// Paints the cave tile `tile` at `position`.
#[allow(clippy::too_many_lines, reason = "one arm per cave tile family")]
pub(super) fn paint(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: CaveTile,
    strata: &Strata,
    band: Band,
    edges: Edges,
) {
    let seed = hash(position.x, position.y, 421);
    // The Drowned and Frozen's tiles have their own art (Wave 9).
    if super::drowned_art::is_drowned_tile(tile) {
        super::drowned_art::paint(canvas, terrain, position, tile, strata, edges);
        return;
    }
    match tile {
        CaveTile::MushroomStem => {
            paint_back(canvas, terrain, position, strata, band);
            paint_stalk(canvas, seed);
            return;
        }
        CaveTile::MushroomCap => {
            paint_back(canvas, terrain, position, strata, band);
            paint_cap(canvas, terrain, position, seed);
            return;
        }
        CaveTile::GlowCap => {
            paint_back(canvas, terrain, position, strata, band);
            paint_glowcaps(canvas, seed);
            return;
        }
        CaveTile::Stalactite => {
            paint_back(canvas, terrain, position, strata, band);
            paint_stalactite(canvas, strata, seed);
            return;
        }
        CaveTile::RuinPillar => {
            paint_back(canvas, terrain, position, strata, band);
            paint_pillar(canvas, terrain, position);
            return;
        }
        CaveTile::TreasureCache => {
            paint_back(canvas, terrain, position, strata, band);
            paint_chest(canvas);
            return;
        }
        CaveTile::RelicCache => {
            paint_back(canvas, terrain, position, strata, band);
            paint_relic(canvas);
            return;
        }
        CaveTile::SporePod => {
            paint_back(canvas, terrain, position, strata, band);
            paint_spore_pod(canvas, seed);
            return;
        }
        _ => {}
    }
    // Descent VII part B's tiles (Wave 10): war plate, bays, pylons, barricades, seal stone,
    // the door, the Heart Seal, gold, rubble, altars.
    if let Some(material) = super::garrison_art::garrison_ramp(tile) {
        for py in 0..TILE_ART {
            for px in 0..TILE_ART {
                let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
                if let Some(index) = super::garrison_art::garrison_pixel(tile, x, y) {
                    canvas.set(px, py, index);
                }
            }
        }
        paint_edges(canvas, strata, edges, material[3], material[0], true);
        return;
    }
    // The far Strange Layers' tiles (Wave 9): quartz, keyed glass, alloy, chronoglass.
    if let Some(material) = super::far_art::far_ramp(tile) {
        for py in 0..TILE_ART {
            for px in 0..TILE_ART {
                let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
                if let Some(index) = super::far_art::far_pixel(tile, x, y) {
                    canvas.set(px, py, index);
                }
            }
        }
        paint_edges(
            canvas,
            strata,
            edges,
            material[3],
            material[0],
            tile.is_keyed_wall(),
        );
        return;
    }
    let crystal = crystal_ramp(band);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
            let index = match tile {
                CaveTile::Crystal => crystal_pixel(crystal, x, y),
                CaveTile::Mycelium => mycelium_pixel(x, y),
                CaveTile::Obsidian => obsidian_pixel(x, y),
                CaveTile::RuinBrick | CaveTile::RuinGlyph => sandstone_pixel(x, y),
                CaveTile::FossilRib => fossil_rib_pixel(strata, x, y),
                CaveTile::GlowAlgae | CaveTile::Silt => silt_pixel(x, y),
                CaveTile::Root => root_pixel(x, y),
                CaveTile::CharredRoot => charred_pixel(x, y),
                CaveTile::Flesh | CaveTile::AcidGland => flesh_pixel(x, y),
                CaveTile::Comb => comb_pixel(x, y, false),
                CaveTile::JellyCell => comb_pixel(x, y, true),
                _ => strata.stone[2],
            };
            canvas.set(px, py, index);
        }
    }
    match tile {
        CaveTile::RuinGlyph => paint_glyph(canvas, seed),
        CaveTile::GlowAlgae => paint_algae(canvas, seed),
        CaveTile::AcidGland => paint_gland(canvas, seed),
        _ => {}
    }
    let material: Ramp = match tile {
        CaveTile::Crystal => crystal,
        CaveTile::Mycelium => MYCELIUM,
        CaveTile::Obsidian => OBSIDIAN,
        CaveTile::RuinBrick | CaveTile::RuinGlyph => SANDSTONE,
        CaveTile::GlowAlgae | CaveTile::Silt => SILT,
        CaveTile::Root => ramp([SOIL_DARK, SOIL, LOAM, SAND]),
        CaveTile::CharredRoot => ramp([INK, NIGHT, STONE_DARK, STONE]),
        CaveTile::Flesh | CaveTile::AcidGland => ramp([MAROON, RED, ROSE, PINK]),
        CaveTile::Comb | CaveTile::JellyCell => ramp([SOIL_DARK, AMBER, GOLD, CREAM]),
        _ => strata.stone,
    };
    paint_edges(canvas, strata, edges, material[3], material[0], false);
}

// --- The Living Dark (Wave 8) ----------------------------------------------------------------

/// One family of root strands at art pixel `(x, y)`: where a strand lies there, how far across
/// it the pixel is (-1 at its lit top edge to 1 at its shaded underside). Strands run along a
/// slope, bend with slow noise and swell and thin along their length.
fn strand(x: i32, y: i32, slope: f32, pitch: f32, seed: u32) -> Option<f32> {
    let bend = noise(x, y, 16, seed).mul_add(10.0, noise(x, y, 6, seed + 1) * 3.0);
    let along = (x as f32).mul_add(slope, y as f32) + bend;
    let cell = (along / pitch).floor();
    let local = along - cell * pitch;
    let half = noise(x, cell as i32 * 37, 9, seed + 2).mul_add(2.4, 1.8);
    let across = local - pitch * 0.5;
    (across.abs() < half).then(|| across / half)
}

/// Living root: a tangle of round, sinuous strands, each lit on top and shaded under with an
/// ink rim, crossing one another over dark soil; pale rootlet hairs and moss in the gaps.
fn root_pixel(x: i32, y: i32) -> PaletteIndex {
    let shade = |t: f32| {
        if t < -0.75 {
            index_of(SAND)
        } else if t < -0.25 {
            index_of(CLAY)
        } else if t < 0.45 {
            index_of(LOAM)
        } else if t < 0.8 {
            index_of(SOIL)
        } else {
            index_of(INK)
        }
    };
    // The nearer strands (gently sloping) over the farther ones (steeper, darker).
    if let Some(t) = strand(x, y, 0.35, 11.0, 511) {
        return shade(t);
    }
    if let Some(t) = strand(x, y, -1.4, 13.0, 521) {
        return if t > 0.6 {
            index_of(INK)
        } else if t < -0.3 {
            index_of(LOAM)
        } else {
            index_of(SOIL)
        };
    }
    if hash(x, y, 513).is_multiple_of(23) {
        return index_of(MOSS);
    }
    if hash(x, y, 514).is_multiple_of(17) {
        return index_of(STONE_LIGHT);
    }
    if noise(x, y, 5, 515) > 0.6 {
        index_of(SOIL_DARK)
    } else {
        index_of(NIGHT)
    }
}

/// Charred root: black charcoal crazed with grey ash and a few dull embers.
fn charred_pixel(x: i32, y: i32) -> PaletteIndex {
    if hash(x, y, 521).is_multiple_of(41) {
        return index_of(RUST);
    }
    let (near, second, _, _) = super::worley(x, y, 5, 523, false);
    if second - near < 0.9 {
        return index_of(STONE_DARK);
    }
    if noise(x, y, 4, 525) > 0.7 {
        index_of(NIGHT)
    } else {
        index_of(INK)
    }
}

/// Flesh: swollen lobes of dark red meat, each lit on its upper left and wet with a glint,
/// sunk between maroon folds threaded with darker veins.
fn flesh_pixel(x: i32, y: i32) -> PaletteIndex {
    let (near, second, dx, dy) = super::worley(x, y, 8, 531, false);
    let fold = second - near;
    if fold < 0.7 {
        return index_of(NIGHT);
    }
    if fold < 1.6 {
        return index_of(MAROON);
    }
    // A vein crossing the lobe.
    let vein = (noise(x, y, 9, 537) * 14.0) as i32;
    if (x + y / 2 + vein).rem_euclid(13) == 0 && noise(x, y, 5, 539) > 0.45 {
        return index_of(PLUM);
    }
    let lit = dx + dy;
    if lit <= -5 && hash(x, y, 533).is_multiple_of(5) {
        // A wet glint on the swell.
        return index_of(PINK);
    }
    if lit <= -3 {
        index_of(ROSE)
    } else if lit >= 4 {
        index_of(MAROON)
    } else if noise(x, y, 4, 535) + f32::from(bayer4(x, y)) / 50.0 > 0.62 {
        index_of(ROSE)
    } else {
        index_of(RED)
    }
}

/// Honeycomb: hexagonal wax cells, amber walls round dark honeyed hollows.
const fn comb_pixel(x: i32, y: i32, filled: bool) -> PaletteIndex {
    // A hex lattice: rows of cells 6 wide and 5 tall, alternate rows offset by half a cell.
    let row = y.div_euclid(5);
    let shift = if row.rem_euclid(2) == 0 { 0 } else { 3 };
    let cx = (x + shift).rem_euclid(6);
    let cy = y.rem_euclid(5);
    let wall = cy == 0 || cx == 0 || (cy == 1 && (cx == 1 || cx == 5));
    if wall {
        return if cy == 0 && cx > 1 && cx < 5 {
            index_of(GOLD)
        } else {
            index_of(AMBER)
        };
    }
    if filled {
        // Royal jelly glowing in the cell.
        return if cy == 2 && cx == 2 {
            index_of(BONE)
        } else if cy <= 2 {
            index_of(CREAM)
        } else {
            index_of(GOLD)
        };
    }
    if cy == 2 && cx == 2 && hash(x, y, 541).is_multiple_of(3) {
        return index_of(ORANGE);
    }
    if cy >= 3 {
        index_of(SOIL)
    } else {
        index_of(SOIL_DARK)
    }
}

/// A ripe spore pod hanging from the ceiling: a violet bulb with lime glowing pores.
fn paint_spore_pod(canvas: &mut TileCanvas, seed: u32) {
    let sway = (seed % 3) as i32 - 1;
    // The stalk.
    for py in 0..4 {
        canvas.set(7 + sway * (py / 3), py, index_of(SAND));
        canvas.set(8 + sway * (py / 3), py, index_of(LOAM));
    }
    // The bulb.
    let (cx, cy) = (8 + sway, 9);
    for py in 3..TILE_ART {
        for px in 0..TILE_ART {
            let dx = px - cx;
            let dy = (py - cy) * 6 / 5;
            let d = dx * dx + dy * dy;
            if d > 30 {
                continue;
            }
            let index = if d > 24 {
                index_of(INK)
            } else if dx + dy < -4 {
                index_of(LAVENDER)
            } else if dx + dy > 4 {
                index_of(PLUM)
            } else {
                index_of(VIOLET)
            };
            canvas.set(px, py, index);
        }
    }
    for (px, py) in [
        (cx - 2, cy - 2),
        (cx + 2, cy),
        (cx - 1, cy + 2),
        (cx + 1, cy - 3),
    ] {
        canvas.set(px, py, index_of(LIME));
    }
    canvas.set(cx - 2, cy - 3, index_of(PINK));
}

/// An acid gland: a puckered maroon sphincter in the flesh, oozing lime acid.
fn paint_gland(canvas: &mut TileCanvas, seed: u32) {
    let (cx, cy) = (8, 7);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let dx = px - cx;
            let dy = py - cy;
            let d = dx * dx + dy * dy;
            let index = if d <= 2 {
                index_of(LIME)
            } else if d <= 6 {
                index_of(INK)
            } else if d <= 20 && (dx * 3 + dy * 2).rem_euclid(5) == 0 {
                index_of(MAROON)
            } else if d <= 20 {
                index_of(RED)
            } else {
                continue;
            };
            canvas.set(px, py, index);
        }
    }
    // A drip of acid running down.
    let drip = 2 + (seed % 4) as i32;
    for py in cy + 2..cy + 2 + drip {
        canvas.set(cx, py, index_of(LIME));
    }
    canvas.set(cx, cy + 2 + drip, index_of(LEAF));
}

/// Crystal: sharp diagonal facets with bright cleavage lines and glints.
fn crystal_pixel(crystal: Ramp, x: i32, y: i32) -> PaletteIndex {
    let (near, second, dx, dy) = super::worley(x, y, 6, 431, true);
    if second - near < 1.2 {
        return crystal[3];
    }
    if hash(x, y, 433).is_multiple_of(37) {
        return index_of(CREAM);
    }
    let facet = dx - dy;
    if facet <= -3 {
        crystal[2]
    } else if facet >= 3 {
        crystal[0]
    } else {
        crystal[1]
    }
}

/// Mycelium: plum soil threaded with pale fibres and glowing teal specks.
fn mycelium_pixel(x: i32, y: i32) -> PaletteIndex {
    if hash(x, y, 441).is_multiple_of(31) {
        return index_of(MINT);
    }
    let fibre = (noise(x, y, 5, 443) * 8.0) as i32;
    if (x + fibre).rem_euclid(7) == 0 && noise(x, y, 3, 445) > 0.4 {
        return index_of(BONE);
    }
    let body = noise(x, y, 4, 447) + f32::from(bayer4(x, y)) / 60.0;
    if body > 0.68 {
        MYCELIUM[3]
    } else if body > 0.42 {
        MYCELIUM[2]
    } else if body > 0.22 {
        MYCELIUM[1]
    } else {
        MYCELIUM[0]
    }
}

/// Obsidian: black glass with long violet sheens and sharp conchoidal ridges.
fn obsidian_pixel(x: i32, y: i32) -> PaletteIndex {
    let sheen = ((x * 2 + y) as f32 / 7.0 + noise(x, y, 8, 451) * 3.0).sin();
    if hash(x, y, 453).is_multiple_of(61) {
        return OBSIDIAN[3];
    }
    match slab_level(x, y) {
        0 => OBSIDIAN[0],
        _ if sheen > 0.86 => OBSIDIAN[2],
        3 => OBSIDIAN[2],
        _ => OBSIDIAN[1],
    }
}

/// Sandstone: big dressed blocks with dark mortar, a lit top edge and weathering pits.
fn sandstone_pixel(x: i32, y: i32) -> PaletteIndex {
    let row = y.div_euclid(8);
    let shifted = x + if row % 2 == 0 { 0 } else { 8 };
    let (bx, by) = (shifted.rem_euclid(16), y.rem_euclid(8));
    if by == 7 || bx == 15 {
        return index_of(NIGHT);
    }
    if by == 0 {
        return SANDSTONE[3];
    }
    if by == 6 || bx == 14 {
        return SANDSTONE[0];
    }
    if hash(x, y, 461).is_multiple_of(17) || noise(x, y, 3, 463) > 0.8 {
        return SANDSTONE[1];
    }
    SANDSTONE[2]
}

/// Band stone crossed by a pale curved rib of old bone.
fn fossil_rib_pixel(strata: &Strata, x: i32, y: i32) -> PaletteIndex {
    let lx = x.rem_euclid(TILE_ART) - 8;
    let ly = y.rem_euclid(TILE_ART);
    // A rib arcing across the tile.
    let arc = (lx * lx) / 9 + 3;
    if (ly - arc).abs() <= 1 {
        return index_of(if ly == arc - 1 { CREAM } else { BONE });
    }
    if ly == arc + 2 {
        return index_of(SOIL_DARK);
    }
    strata.stone[super::cobble_level(x, y)]
}

/// Silt: fine rippled layers.
fn silt_pixel(x: i32, y: i32) -> PaletteIndex {
    let wave = (noise(x, y, 11, 471) * 4.0) as i32;
    match (y + wave).rem_euclid(4) {
        _ if hash(x, y, 473).is_multiple_of(29) => SILT[3],
        0 => SILT[2],
        3 => SILT[0],
        _ => SILT[1],
    }
}

/// Glowing algae strands on silt.
fn paint_algae(canvas: &mut TileCanvas, seed: u32) {
    for strand in 0..4 {
        let h = hash(seed as i32, strand, 475);
        let x = 1 + (h % 14) as i32;
        let height = 3 + ((h >> 6) % 6) as i32;
        for step in 0..height {
            let sway = i32::from((step + (h >> 12) as i32 % 2) % 3 == 0);
            let color = if step == height - 1 {
                CREAM
            } else if step % 2 == 0 {
                LIME
            } else {
                LEAF
            };
            canvas.set(x + sway, step, index_of(color));
        }
    }
}

/// A carved glyph glowing teal in a sandstone block.
fn paint_glyph(canvas: &mut TileCanvas, seed: u32) {
    const GLYPHS: [[u8; 5]; 4] = [
        [0b01110, 0b10001, 0b10101, 0b10001, 0b01110],
        [0b00100, 0b01110, 0b10101, 0b00100, 0b00100],
        [0b10001, 0b01010, 0b00100, 0b01010, 0b10001],
        [0b11111, 0b10000, 0b11110, 0b00001, 0b11111],
    ];
    let glyph = GLYPHS[(seed % 4) as usize];
    let (ox, oy) = (5, 1);
    for (row, bits) in glyph.iter().enumerate() {
        for column in 0..5 {
            if bits & (1 << (4 - column)) != 0 {
                let (x, y) = (ox + column, oy + row as i32);
                canvas.set(x, y, index_of(MINT));
                canvas.set(x + 1, y + 1, index_of(TEAL_DARK));
            }
        }
    }
}

/// A mushroom stalk: a pale, fibrous column.
fn paint_stalk(canvas: &mut TileCanvas, seed: u32) {
    let sway = (seed % 3) as i32 - 1;
    for py in 0..TILE_ART {
        let left = 5 + if py > 8 { sway } else { 0 };
        for px in left..left + 6 {
            let edge = px - left;
            let index = match edge {
                0 => STALK[0],
                5 => STALK[1],
                1 => STALK[3],
                _ => {
                    if (px + py / 3).rem_euclid(4) == 0 {
                        STALK[1]
                    } else {
                        STALK[2]
                    }
                }
            };
            canvas.set(px, py, index);
        }
    }
}

/// A mushroom cap: a rose dome with cream spots and dark gills below; its ends round off where
/// the cap ends.
fn paint_cap(canvas: &mut TileCanvas, terrain: &Terrain, position: TilePosition, seed: u32) {
    let is_cap = |dx: i32| {
        terrain
            .tile(TilePosition {
                x: position.x + dx,
                y: position.y,
            })
            .is_some_and(|tile| tile.kind == TileKind::Cave(CaveTile::MushroomCap))
    };
    let (left_end, right_end) = (!is_cap(-1), !is_cap(1));
    for py in 0..12 {
        for px in 0..TILE_ART {
            // Round the outer ends of the cap.
            let inset = |end: bool, distance: i32| {
                end && distance < (5 - py).max(0) + i32::from(py > 9) * 2
            };
            if inset(left_end, px) || inset(right_end, TILE_ART - 1 - px) {
                continue;
            }
            let index = if py == 0 {
                CAP[3]
            } else if py >= 10 {
                if px % 2 == 0 {
                    CAP[0]
                } else {
                    index_of(STONE_DARK)
                }
            } else if py == 9 {
                CAP[1]
            } else {
                let spot = hash(position.x * 16 + px / 3, py / 3, 481 + seed % 2).is_multiple_of(7);
                if spot && py > 1 && py < 8 {
                    index_of(CREAM)
                } else if py < 3 {
                    CAP[2]
                } else {
                    CAP[1]
                }
            };
            canvas.set(px, py, index);
        }
    }
}

/// Small glowing mushrooms on a strip of mycelium.
fn paint_glowcaps(canvas: &mut TileCanvas, seed: u32) {
    for py in 12..TILE_ART {
        for px in 0..TILE_ART {
            let (x, y) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, mycelium_pixel(x, y));
        }
    }
    for cap in 0..3 {
        let h = hash(seed as i32, cap, 483);
        let x = 2 + (h % 11) as i32;
        let height = 2 + ((h >> 5) % 5) as i32;
        let top = 12 - height;
        for y in top..12 {
            canvas.set(x, y, index_of(BONE));
        }
        let width = 1 + ((h >> 9) % 2) as i32;
        for dx in -width..=width {
            canvas.set(x + dx, top - 1, index_of(MINT));
            canvas.set(x + dx, top, index_of(TEAL));
        }
        canvas.set(x, top - 2, index_of(ICE));
    }
}

/// A rock spike hanging from the ceiling, in the band's stone.
fn paint_stalactite(canvas: &mut TileCanvas, strata: &Strata, seed: u32) {
    let tip = 8 + (seed % 4) as i32;
    let length = 13 + ((seed >> 4) % 3) as i32;
    for py in 0..length {
        let half = ((length - py) * 6) / length;
        for px in (tip - half)..(tip + half) {
            let index = if px == tip - half {
                index_of(INK)
            } else if px - (tip - half) == 1 {
                strata.stone[3]
            } else if px >= tip + half - 2 {
                strata.stone[0]
            } else if (py + px) % 5 == 0 {
                strata.stone[1]
            } else {
                strata.stone[2]
            };
            canvas.set(px, py, index);
        }
    }
    // Drip at the tip.
    canvas.set(tip - 1, length, index_of(SKY_LIGHT));
}

/// A fluted sandstone pillar; the capital where nothing stands above, the base on the floor.
fn paint_pillar(canvas: &mut TileCanvas, terrain: &Terrain, position: TilePosition) {
    let pillar_at = |dy: i32| {
        terrain
            .tile(TilePosition {
                x: position.x,
                y: position.y + dy,
            })
            .is_some_and(|tile| tile.kind == TileKind::Cave(CaveTile::RuinPillar))
    };
    let (top, bottom) = (!pillar_at(-1), !pillar_at(1));
    for py in 0..TILE_ART {
        let wide = (top && py < 3) || (bottom && py > 12);
        let (left, right) = if wide { (1, 14) } else { (3, 12) };
        for px in left..=right {
            // A broken, jagged top.
            if top && py == 0 && hash(position.x, px, 491).is_multiple_of(3) {
                continue;
            }
            let index = if px == left {
                SANDSTONE[3]
            } else if px == right {
                SANDSTONE[0]
            } else if !wide && (px - left) % 3 == 0 {
                SANDSTONE[1]
            } else if wide && (py == 0 || py == TILE_ART - 1) {
                index_of(NIGHT)
            } else {
                SANDSTONE[2]
            };
            canvas.set(px, py, index);
        }
    }
}

/// A banded wooden treasure chest on the floor.
fn paint_chest(canvas: &mut TileCanvas) {
    const CHEST: [&str; 9] = [
        "..kkkkkkkkkkkk..",
        ".kwwwwwwwwwwwwk.",
        ".kgggggggggggg k",
        ".kwwwwwyywwwwwk.",
        ".kkkkkkyykkkkkk.",
        ".kwwwwwwwwwwwwk.",
        ".kgwwwwwwwwwwgk.",
        ".kwwwwwwwwwwwwk.",
        ".kkkkkkkkkkkkkk.",
    ];
    stamp(canvas, &CHEST, 7);
}

/// An ancient stone reliquary with gold trim and a teal seal.
fn paint_relic(canvas: &mut TileCanvas) {
    const RELIC: [&str; 11] = [
        "....kkkkkkkk....",
        "...kyyyyyyyyk...",
        "..kssssssssssk..",
        ".kyyyyyyyyyyyyk.",
        ".kssssmmmsssssk.",
        ".ksssmttmmssssk.",
        ".kssssmmmsssssk.",
        ".ksssssssssssdk.",
        ".kyyyyyyyyyyyyk.",
        "kddddddddddddddk",
        "kkkkkkkkkkkkkkkk",
    ];
    stamp(canvas, &RELIC, 5);
}

fn stamp(canvas: &mut TileCanvas, rows: &[&str], top: i32) {
    for (row, line) in rows.iter().enumerate() {
        for (column, symbol) in line.chars().enumerate() {
            let color = match symbol {
                'k' => INK,
                'w' => RUST,
                'g' => COPPER,
                'y' => GOLD,
                's' => SAND,
                'd' => LOAM,
                'm' => TEAL,
                't' => ICE,
                _ => continue,
            };
            canvas.set(column as i32, top + row as i32, index_of(color));
        }
    }
}
