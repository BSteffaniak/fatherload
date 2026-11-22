//! Tile art for Descent II, the Old Deep (Wave 8, `fatherload_simulation::old_deep`).
//!
//! Each stratum has its own materials: singing blue crystal with white facet glints and clear
//! prisms that throw a rainbow edge (Crystal Faults); banded tan shale, honey amber with dark
//! inclusions and ivory bones (Fossil Oceans); carved lavender ashlar, iron jet grates and the
//! lift's red and green valve wheels (Pressure Cathedrals); near-black rock split by lime glow
//! veins, mint isotope crystals and a pulsing pink relic (Radioactive Hollow); riveted bronze
//! casings, chevroned belts, hazard-striped pistons and amber-glyph tablets (Ancient Machine
//! Layer); violet void glass and pink blooms (Void Geode Fields); porous cinder with ember
//! specks and glowing vents (Mantle Storm Zone); and the Star Core's obsidian-and-gold wall,
//! cyan pylons and the white-gold core itself. A latent gem paints as the plain rock it hides
//! in: the refracted beams reveal it live (`rendering::world::old_deep`).

use fatherload_simulation::{
    old_deep::DeepTile,
    terrain::{Terrain, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, index_of};

use super::{
    Edges, Ramp, Strata, TILE_ART, TileCanvas, base_pixel, cobble_level, hash, noise, paint_edges,
    ramp, slab_level,
};

const CRYSTAL: Ramp = ramp([DEEP_BLUE, OCEAN, SKY, ICE]);
const PRISM: Ramp = ramp([OCEAN, SKY_LIGHT, ICE, BONE]);
const SHALE: Ramp = ramp([SOIL_DARK, SOIL, LOAM, SAND]);
const AMBER_RAMP: Ramp = ramp([RUST, ORANGE, AMBER, GOLD]);
const BONE_RAMP: Ramp = ramp([STONE_DARK, FOG, CREAM, BONE]);
const NAVE: Ramp = ramp([NIGHT, PLUM, VIOLET, LAVENDER]);
const IRON: Ramp = ramp([INK, STEEL_DARK, STEEL, STEEL_LIGHT]);
const GLOW: Ramp = ramp([INK, PINE, MOSS, LIME]);
const ISOTOPE: Ramp = ramp([PINE, LEAF, LIME, MINT]);
const RELIC: Ramp = ramp([PLUM, ROSE, PINK, MINT]);
const BRONZE: Ramp = ramp([SOIL_DARK, RUST, COPPER, AMBER]);
const BELT: Ramp = ramp([INK, NIGHT, SLATE_DARK, SLATE]);
const VOID: Ramp = ramp([INK, PLUM, VIOLET, PINK]);
const BLOOM: Ramp = ramp([VIOLET, ROSE, PINK, BONE]);
const CINDER: Ramp = ramp([INK, NIGHT, MAROON, RUST]);
const CORE_WALL: Ramp = ramp([INK, NIGHT, PLUM, GOLD]);
const PYLON: Ramp = ramp([TEAL_DARK, TEAL, MINT, ICE]);
const CORE: Ramp = ramp([AMBER, GOLD, CREAM, BONE]);
const SPENT: Ramp = ramp([INK, NIGHT, SLATE_DARK, STONE_DARK]);
const CRATE: Ramp = ramp([SOIL_DARK, SOIL, LOAM, CREAM]);

/// Representative colour of an Old Deep tile (drill dust, debris, the map).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "one colour per tile, grouped by look"
)]
pub const fn deep_color(tile: DeepTile) -> raylib::prelude::Color {
    match tile {
        DeepTile::ResonantCrystal => SKY,
        DeepTile::Prism => ICE,
        DeepTile::LatentGem => STONE,
        DeepTile::Amber => AMBER,
        DeepTile::FossilBone => BONE,
        DeepTile::Shale => LOAM,
        DeepTile::JetVent => STEEL,
        DeepTile::LiftValve => RED,
        DeepTile::LiftValveOpen => LIME,
        DeepTile::NaveStone => VIOLET,
        DeepTile::GlowRock => MOSS,
        DeepTile::IsotopeCluster => LIME,
        DeepTile::MutationRelic => PINK,
        DeepTile::ConveyorLeft | DeepTile::ConveyorRight => SLATE_DARK,
        DeepTile::PistonHousing => STEEL,
        DeepTile::MachineCasing => COPPER,
        DeepTile::BuilderTablet => AMBER,
        DeepTile::GeodeShell => VIOLET,
        DeepTile::VoidBloom => PINK,
        DeepTile::StormVent => ORANGE,
        DeepTile::CinderRock => MAROON,
        DeepTile::CoreWall => PLUM,
        DeepTile::CorePylon => MINT,
        DeepTile::FirstCoreShielded | DeepTile::FirstCore => CREAM,
        DeepTile::FirstCoreSpent => SLATE_DARK,
        DeepTile::ExpeditionLog => LOAM,
    }
}

/// The body pattern of `tile` at art pixel `(x, y)` (global coordinates).
#[allow(clippy::too_many_lines, reason = "one pattern per Old Deep material")]
fn body_pixel(tile: DeepTile, strata: &Strata, x: i32, y: i32) -> PaletteIndex {
    match tile {
        DeepTile::ResonantCrystal => {
            // Long diagonal facets with bright edges and rare white glints.
            let facet = (x + y * 2 + (noise(x, y, 6, 801) * 6.0) as i32).rem_euclid(7);
            if hash(x, y, 802).is_multiple_of(37) {
                index_of(BONE)
            } else if facet == 0 {
                CRYSTAL[3]
            } else if facet < 3 {
                CRYSTAL[2]
            } else if facet < 5 {
                CRYSTAL[1]
            } else {
                CRYSTAL[0]
            }
        }
        DeepTile::Prism => PRISM[slab_level(x, y).min(3)],
        DeepTile::LatentGem | DeepTile::Shale | DeepTile::FossilBone => {
            if tile == DeepTile::LatentGem {
                return base_pixel(
                    fatherload_simulation::terrain::TileKind::Stone,
                    strata,
                    fatherload_simulation::terrain::TileKind::Stone,
                    x,
                    y,
                );
            }
            // Thin sedimentary bands, wavering, with fossil flecks.
            let band = (y + (noise(x, y, 14, 803) * 3.0) as i32).rem_euclid(4);
            if hash(x, y, 804).is_multiple_of(53) {
                index_of(CREAM)
            } else {
                SHALE[match band {
                    0 => 3,
                    2 => 1,
                    _ => 2,
                }]
            }
        }
        DeepTile::Amber => {
            // Honey glass: a soft gradient, darker specks (inclusions), bright highlights.
            let h = hash(x, y, 805);
            if h.is_multiple_of(41) {
                index_of(SOIL_DARK)
            } else if noise(x, y, 5, 806) > 0.72 {
                AMBER_RAMP[3]
            } else if noise(x, y, 9, 807) > 0.45 {
                AMBER_RAMP[2]
            } else {
                AMBER_RAMP[1]
            }
        }
        DeepTile::JetVent | DeepTile::PistonHousing => IRON[slab_level(x, y).min(3)],
        DeepTile::LiftValve | DeepTile::LiftValveOpen | DeepTile::NaveStone => {
            // Carved ashlar: courses of long blocks, mortar lines, a worn sheen.
            let course = y.div_euclid(8);
            let offset = if course.rem_euclid(2) == 0 { 0 } else { 6 };
            let seam_x = (x + offset).rem_euclid(12) == 0;
            let seam_y = y.rem_euclid(8) == 0;
            if seam_x || seam_y {
                NAVE[0]
            } else if y.rem_euclid(8) == 1 {
                NAVE[3]
            } else if hash(x, y, 808).is_multiple_of(23) {
                NAVE[1]
            } else {
                NAVE[2]
            }
        }
        DeepTile::GlowRock => {
            // Near-black rock split by glowing veins.
            let vein = noise(x, y, 7, 809);
            if (0.48..0.53).contains(&vein) {
                GLOW[3]
            } else if (0.44..0.57).contains(&vein) {
                GLOW[2]
            } else {
                GLOW[cobble_level(x, y).min(1)]
            }
        }
        DeepTile::IsotopeCluster => {
            if hash(x, y, 810).is_multiple_of(5) {
                ISOTOPE[3]
            } else {
                GLOW[cobble_level(x, y).min(1)]
            }
        }
        DeepTile::MutationRelic => RELIC[((noise(x, y, 4, 811) * 3.9) as usize).min(3)],
        DeepTile::ConveyorLeft | DeepTile::ConveyorRight => BELT[slab_level(x, y).min(2)],
        DeepTile::MachineCasing | DeepTile::BuilderTablet => {
            // Riveted plates.
            let (px, py) = (x.rem_euclid(16), y.rem_euclid(16));
            if px == 0 || py == 0 {
                BRONZE[0]
            } else if (px == 2 || px == 13) && (py == 2 || py == 13) {
                BRONZE[3]
            } else if py == 1 || px == 1 {
                BRONZE[2]
            } else if hash(x, y, 812).is_multiple_of(19) {
                BRONZE[0]
            } else {
                BRONZE[1]
            }
        }
        DeepTile::GeodeShell => {
            let facet = (x * 3 - y * 2 + (noise(x, y, 5, 813) * 8.0) as i32).rem_euclid(9);
            if hash(x, y, 814).is_multiple_of(29) {
                index_of(PINK)
            } else {
                VOID[match facet {
                    0 => 3,
                    1..=3 => 2,
                    4..=6 => 1,
                    _ => 0,
                }]
            }
        }
        DeepTile::VoidBloom => BLOOM[((noise(x, y, 3, 815) * 3.9) as usize).min(3)],
        DeepTile::StormVent | DeepTile::CinderRock => {
            let h = hash(x, y, 816);
            if h.is_multiple_of(31) {
                index_of(ORANGE)
            } else if h.is_multiple_of(7) {
                CINDER[0]
            } else {
                CINDER[cobble_level(x, y).clamp(1, 3)]
            }
        }
        DeepTile::CoreWall => {
            // Obsidian blocks with a gold inlay line every course.
            let (px, py) = (x.rem_euclid(16), y.rem_euclid(16));
            if py == 7 && px.rem_euclid(4) != 0 {
                CORE_WALL[3]
            } else if px == 0 || py == 0 {
                CORE_WALL[0]
            } else {
                CORE_WALL[1 + usize::from(hash(x, y, 817).is_multiple_of(9))]
            }
        }
        DeepTile::CorePylon => PYLON[slab_level(x, y).min(1)],
        DeepTile::FirstCoreShielded | DeepTile::FirstCore => CORE[1],
        DeepTile::FirstCoreSpent => SPENT[slab_level(x, y).min(3)],
        DeepTile::ExpeditionLog => CRATE[slab_level(x, y).min(2)],
    }
}

fn paint_bone(canvas: &mut TileCanvas, pick: u32) {
    // A vertebra or a rib, ivory with a shadowed edge.
    let ink = index_of(INK);
    if pick.is_multiple_of(2) {
        for py in 4..12_i32 {
            for px in 2..14_i32 {
                let inside = (px - 8).abs() * 2 + (py - 8).abs() * 3 < 14;
                if inside {
                    canvas.set(px, py, BONE_RAMP[if py < 7 { 3 } else { 2 }]);
                }
            }
        }
        for px in 6..10 {
            canvas.set(px, 8, BONE_RAMP[0]);
        }
        canvas.set(2, 8, ink);
        canvas.set(13, 8, ink);
    } else {
        for step in 0..12 {
            let (px, py) = (2 + step, 12 - step * 3 / 4);
            canvas.set(px, py, BONE_RAMP[3]);
            canvas.set(px, py + 1, BONE_RAMP[2]);
            canvas.set(px, py + 2, BONE_RAMP[1]);
        }
    }
}

fn paint_prism(canvas: &mut TileCanvas) {
    // A clear triangular prism standing in the rock, a rainbow along its right face.
    let rainbow = [RED, ORANGE, GOLD, LIME, SKY, VIOLET];
    for py in 2..15 {
        let half = (py - 1) / 2;
        for px in 8 - half..=8 + half {
            let edge = px == 8 - half || px == 8 + half || py == 14;
            canvas.set(
                px,
                py,
                if edge {
                    PRISM[3]
                } else {
                    PRISM[1 + (px + py) as usize % 2]
                },
            );
        }
        let band = usize::try_from((py - 2) / 2).unwrap_or(0).min(5);
        canvas.set(9 + half, py, index_of(rainbow[band]));
    }
    canvas.set(8, 2, index_of(BONE));
}

fn paint_valve(canvas: &mut TileCanvas, open: bool) {
    // A spoked valve wheel on a pipe stub.
    let (rim, hub) = if open { (LIME, MINT) } else { (RED, ORANGE) };
    let ink = index_of(INK);
    for py in 0..16 {
        canvas.set(7, py, IRON[2]);
        canvas.set(8, py, IRON[1]);
    }
    for py in 2..14 {
        for px in 2..14 {
            let d = (px - 7) * (px - 7) + (py - 7) * (py - 7);
            if (20..=30).contains(&d) {
                canvas.set(px, py, index_of(rim));
            } else if d > 30 && d <= 38 {
                canvas.set(px, py, ink);
            }
        }
    }
    for i in 3..12 {
        canvas.set(i, 7, index_of(rim));
        canvas.set(7, i, index_of(rim));
    }
    canvas.set(7, 7, index_of(hub));
    canvas.set(8, 7, index_of(hub));
}

fn paint_vent(canvas: &mut TileCanvas) {
    // An iron grate over a dark throat; steam stains on top.
    for px in 2..14 {
        canvas.set(px, 0, IRON[0]);
        canvas.set(px, 1, if px % 3 == 0 { IRON[3] } else { index_of(INK) });
        canvas.set(
            px,
            2,
            if px % 3 == 0 {
                IRON[3]
            } else {
                index_of(NIGHT)
            },
        );
        canvas.set(px, 3, IRON[1]);
    }
}

fn paint_belt(canvas: &mut TileCanvas, left: bool) {
    // Rollers under a dark belt, amber chevrons pointing the way it runs.
    for px in 0..16 {
        canvas.set(px, 0, BELT[3]);
        canvas.set(px, 1, BELT[1]);
        canvas.set(px, 4, BELT[0]);
    }
    for start in [1, 9] {
        for row in 0..2 {
            let tip = if left { start + row } else { start + 5 - row };
            canvas.set(tip, 2 + row, index_of(AMBER));
            canvas.set(tip, 3 - row, index_of(AMBER));
        }
    }
    for roller in [3, 8, 13] {
        for py in 6..10 {
            canvas.set(roller, py, IRON[3]);
            canvas.set(roller + 1, py, IRON[1]);
        }
    }
}

fn paint_piston(canvas: &mut TileCanvas) {
    // Hazard stripes round the housing's mouth (the ram is drawn live).
    for px in 0..16_i32 {
        let stripe = px.rem_euclid(6) < 3;
        canvas.set(px, 12, index_of(if stripe { GOLD } else { INK }));
        canvas.set(px, 13, index_of(if stripe { INK } else { GOLD }));
        canvas.set(px, 15, IRON[0]);
    }
    for px in 5..11 {
        canvas.set(px, 14, IRON[3]);
    }
}

fn paint_tablet(canvas: &mut TileCanvas, pick: u32) {
    // A dark slab with glowing builder glyph lines.
    for py in 2..14 {
        for px in 3..13 {
            canvas.set(px, py, index_of(NIGHT));
        }
    }
    for line in 0..4 {
        let py = 4 + line * 3;
        let mut px = 4;
        let mut bits = pick.rotate_left(line as u32 * 7);
        while px < 12 {
            let run = 1 + (bits % 3) as i32;
            for dx in 0..run {
                canvas.set(px + dx, py, index_of(AMBER));
            }
            px += run + 1;
            bits /= 3;
        }
    }
}

fn paint_relic(canvas: &mut TileCanvas) {
    // A pulsing organic relic: a pink bulb with mint eyes.
    for py in 3..14 {
        for px in 3..13 {
            let d = (px - 8) * (px - 8) * 2 + (py - 8) * (py - 8) * 3;
            if d < 60 {
                canvas.set(
                    px,
                    py,
                    RELIC[if d < 18 {
                        3
                    } else if d < 40 {
                        2
                    } else {
                        1
                    }],
                );
            }
        }
    }
    canvas.set(6, 7, index_of(MINT));
    canvas.set(10, 7, index_of(MINT));
    canvas.set(8, 10, index_of(INK));
}

fn paint_isotopes(canvas: &mut TileCanvas, pick: u32) {
    // Three glowing crystal spears out of the rock.
    for (i, base) in [3, 7, 11].into_iter().enumerate() {
        let height = 5 + ((pick >> (i * 3)) % 5) as i32;
        for step in 0..height {
            let py = 14 - step;
            canvas.set(base, py, ISOTOPE[2]);
            canvas.set(
                base + 1,
                py,
                if step == height - 1 {
                    ISOTOPE[3]
                } else {
                    ISOTOPE[1]
                },
            );
        }
    }
}

fn paint_bloom(canvas: &mut TileCanvas) {
    for (dx, dy) in [(0, -4), (3, -1), (2, 3), (-2, 3), (-3, -1)] {
        for t in 0..=3 {
            let (px, py) = (8 + dx * t / 3, 8 + dy * t / 3);
            canvas.set(px, py, BLOOM[1 + usize::try_from(t).unwrap_or(0).min(2)]);
        }
    }
    canvas.set(8, 8, index_of(BONE));
}

fn paint_storm_vent(canvas: &mut TileCanvas) {
    // A cinder cone with a glowing throat.
    for py in 0..6 {
        for px in 5 - py / 2..=10 + py / 2 {
            let throat = (7..=8).contains(&px) && py < 4;
            canvas.set(
                px,
                py,
                if throat {
                    index_of(if py < 2 { GOLD } else { ORANGE })
                } else {
                    CINDER[3]
                },
            );
        }
    }
}

fn paint_pylon(canvas: &mut TileCanvas) {
    // A cyan pylon: a tapering column with a bright energy seam and a cap.
    for py in 0..16 {
        let half = 2 + (16 - py) / 8;
        for px in 8 - half..8 + half {
            canvas.set(px, py, PYLON[if px == 8 - half { 3 } else { 1 }]);
        }
        canvas.set(8, py, index_of(if py % 4 == 0 { BONE } else { ICE }));
    }
    for px in 4..12 {
        canvas.set(px, 0, PYLON[3]);
        canvas.set(px, 15, index_of(INK));
    }
}

fn paint_core(canvas: &mut TileCanvas, shielded: bool, spent: bool) {
    // The core: a white-gold sphere (a dark husk once spent); a cyan shield ring around it.
    for py in 0..16 {
        for px in 0..16 {
            let d = (px * 2 - 15) * (px * 2 - 15) + (py * 2 - 15) * (py * 2 - 15);
            if d <= 120 {
                let ramp = if spent { &SPENT } else { &CORE };
                let level = if d < 25 {
                    3
                } else if d < 60 {
                    2
                } else {
                    usize::from(d < 95)
                };
                canvas.set(px, py, ramp[level]);
            } else if shielded && (180..=230).contains(&d) {
                canvas.set(px, py, PYLON[2]);
            }
        }
    }
    if !spent {
        canvas.set(6, 5, index_of(BONE));
        canvas.set(5, 6, index_of(BONE));
    }
}

fn paint_crate(canvas: &mut TileCanvas) {
    // The Fathers' cache: a strapped crate with their mark (a pick over a downward arrow).
    let ink = index_of(INK);
    for px in 1..15 {
        canvas.set(px, 1, ink);
        canvas.set(px, 14, ink);
    }
    for py in 1..15 {
        canvas.set(1, py, ink);
        canvas.set(14, py, ink);
    }
    for px in 2..14 {
        canvas.set(px, 4, CRATE[0]);
        canvas.set(px, 11, CRATE[0]);
    }
    for py in 5..11 {
        canvas.set(8, py, index_of(GOLD));
    }
    for d in 0..3 {
        canvas.set(8 - d, 10 - d, index_of(GOLD));
        canvas.set(8 + d, 10 - d, index_of(GOLD));
    }
    for px in 5..12 {
        canvas.set(px, 6, index_of(STEEL_LIGHT));
    }
}

pub(super) fn paint(
    canvas: &mut TileCanvas,
    _terrain: &Terrain,
    position: TilePosition,
    tile: DeepTile,
    strata: &Strata,
    edges: Edges,
) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, body_pixel(tile, strata, gx, gy));
        }
    }
    let pick = hash(position.x, position.y, 820);
    match tile {
        DeepTile::FossilBone => paint_bone(canvas, pick),
        DeepTile::Prism => paint_prism(canvas),
        DeepTile::LiftValve | DeepTile::LiftValveOpen => {
            paint_valve(canvas, tile == DeepTile::LiftValveOpen);
        }
        DeepTile::JetVent => paint_vent(canvas),
        DeepTile::ConveyorLeft | DeepTile::ConveyorRight => {
            paint_belt(canvas, tile == DeepTile::ConveyorLeft);
        }
        DeepTile::PistonHousing => paint_piston(canvas),
        DeepTile::BuilderTablet => paint_tablet(canvas, pick),
        DeepTile::MutationRelic => paint_relic(canvas),
        DeepTile::IsotopeCluster => paint_isotopes(canvas, pick),
        DeepTile::VoidBloom => paint_bloom(canvas),
        DeepTile::StormVent => paint_storm_vent(canvas),
        DeepTile::CorePylon => paint_pylon(canvas),
        DeepTile::FirstCoreShielded | DeepTile::FirstCore | DeepTile::FirstCoreSpent => {
            paint_core(
                canvas,
                tile == DeepTile::FirstCoreShielded,
                tile == DeepTile::FirstCoreSpent,
            );
        }
        DeepTile::ExpeditionLog => paint_crate(canvas),
        _ => {}
    }
    let (lip, shade) = match tile {
        DeepTile::ResonantCrystal => (CRYSTAL[3], CRYSTAL[0]),
        DeepTile::Prism => (PRISM[3], PRISM[0]),
        DeepTile::Shale | DeepTile::FossilBone | DeepTile::LatentGem => (SHALE[3], SHALE[0]),
        DeepTile::Amber => (AMBER_RAMP[3], AMBER_RAMP[0]),
        DeepTile::NaveStone | DeepTile::LiftValve | DeepTile::LiftValveOpen => (NAVE[3], NAVE[0]),
        DeepTile::GlowRock | DeepTile::IsotopeCluster => (GLOW[3], GLOW[0]),
        DeepTile::MachineCasing | DeepTile::BuilderTablet => (BRONZE[3], BRONZE[0]),
        DeepTile::GeodeShell | DeepTile::VoidBloom => (VOID[3], VOID[0]),
        DeepTile::CinderRock | DeepTile::StormVent => (CINDER[3], CINDER[0]),
        DeepTile::CoreWall => (CORE_WALL[3], CORE_WALL[0]),
        _ => (IRON[3], IRON[0]),
    };
    let square = matches!(
        tile,
        DeepTile::NaveStone
            | DeepTile::MachineCasing
            | DeepTile::BuilderTablet
            | DeepTile::ConveyorLeft
            | DeepTile::ConveyorRight
            | DeepTile::PistonHousing
            | DeepTile::CoreWall
            | DeepTile::JetVent
            | DeepTile::LiftValve
            | DeepTile::LiftValveOpen
            | DeepTile::ExpeditionLog
    );
    let skip_edges = matches!(
        tile,
        DeepTile::FirstCore
            | DeepTile::FirstCoreShielded
            | DeepTile::FirstCoreSpent
            | DeepTile::CorePylon
    );
    if !skip_edges {
        paint_edges(canvas, strata, edges, lip, shade, square);
    }
}
