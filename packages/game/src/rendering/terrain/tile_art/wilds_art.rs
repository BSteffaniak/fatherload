//! Tile art for the wild worlds (Wave 12 planets-a, `fatherload_simulation::planet::wilds`):
//! jungle, ocean archipelago and glacier planets, their open air and their back walls.
//!
//! * Jungle: dark loam flecked with moss, root soil threaded with pale rootlets, twisted
//!   living roots, ridged trunks, clumped canopy leaves with sky between them, grey-green
//!   spore stone furred with moss, mycelium threads, pink spotted puffcaps on the cave floors,
//!   heartwood in red-brown growth rings, golden amber with a fossil inside, lifesap glowing
//!   lime in the wood, the Elder Tree's lichened bark and its golden seed. Vines hang in the
//!   open air under the canopy.
//! * Ocean: pale sand with shell flecks, layered limestone with fossil spirals, pink branching
//!   coral and teal brain coral, dark blue silt stone, the ruins' cut blocks and their glowing
//!   glyphs and relics, black hexagonal trench basalt, sulphur-crusted vent rock, the smokers'
//!   glowing chimneys, pearl shells. The sea is painted into its tiles (the polar renderer has no
//!   live fluid pass): a bright surface line, deep blues below, kelp swaying up from the
//!   seabed.
//! * Glacier: pale glacier ice in long strata lines, clear deep blue ice with fractures, sea
//!   ice plates, feathery rime, frozen gravel, clear frost crystals, hanging icicles, the
//!   titan's ivory bone and its red crystal heart.

#![allow(
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::manual_range_contains,
    reason = "presentation code: art math reads top to bottom"
)]

use fatherload_simulation::{
    fluid::{FLUID_FULL, FluidKind},
    planet::{
        BodyGround, PlanetGround, PlanetThemeId,
        wilds::{WildTile, jungle, ocean},
    },
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

const LOAM: Ramp = ramp([INK, SOIL_DARK, SOIL, MOSS]);
const ROOT_SOIL: Ramp = ramp([SOIL_DARK, SOIL, CLAY, SAND]);
const LIVING_ROOT: Ramp = ramp([SOIL_DARK, SOIL, CLAY, MOSS]);
const BARK: Ramp = ramp([INK, SOIL_DARK, SOIL, CLAY]);
const LEAVES: Ramp = ramp([PINE, MOSS, LEAF, LIME]);
const SPORE: Ramp = ramp([NIGHT, SLATE_DARK, TEAL_DARK, MOSS]);
const HEARTWOOD: Ramp = ramp([MAROON, RUST, COPPER, AMBER]);
const AMBER_RAMP: Ramp = ramp([RUST, ORANGE, AMBER, GOLD]);
const SAP: Ramp = ramp([MOSS, LEAF, LIME, CREAM]);
const ELDER: Ramp = ramp([INK, SOIL_DARK, SOIL, FOG]);
const SAND_RAMP: Ramp = ramp([CLAY, SAND, CREAM, BONE]);
const LIMESTONE: Ramp = ramp([CLAY, SAND, CREAM, BONE]);
const CORAL: Ramp = ramp([MAROON, ROSE, PINK, CREAM]);
const BRAIN: Ramp = ramp([TEAL_DARK, TEAL, MINT, BONE]);
const SILT: Ramp = ramp([INK, NIGHT, DEEP_BLUE, SLATE_DARK]);
const RUIN: Ramp = ramp([SLATE_DARK, STONE_DARK, STONE, STONE_LIGHT]);
const BASALT: Ramp = ramp([INK, NIGHT, SLATE_DARK, STEEL_DARK]);
const VENT: Ramp = ramp([NIGHT, SLATE_DARK, AMBER, GOLD]);
const WATER: Ramp = ramp([NIGHT, DEEP_BLUE, OCEAN, SKY]);
const GLACIER: Ramp = ramp([SKY, SKY_LIGHT, ICE, BONE]);
const BLUE_ICE: Ramp = ramp([DEEP_BLUE, OCEAN, SKY, ICE]);
const SEA_ICE: Ramp = ramp([OCEAN, SKY_LIGHT, ICE, BONE]);
const PERMAFROST: Ramp = ramp([NIGHT, SLATE_DARK, SLATE, FOG]);
const BONE_RAMP: Ramp = ramp([FOG, MIST, CREAM, BONE]);
const HEART: Ramp = ramp([MAROON, RED, ROSE, PINK]);

/// Representative colour of a wild tile (drill dust, debris, the map).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "one colour per tile, grouped by look"
)]
pub const fn wild_color(tile: WildTile) -> raylib::prelude::Color {
    match tile {
        WildTile::JungleLoam | WildTile::RootSoil => SOIL,
        WildTile::LivingRoot | WildTile::Trunk => CLAY,
        WildTile::Canopy => LEAF,
        WildTile::SporeStone | WildTile::Mycel => TEAL_DARK,
        WildTile::Puffcap => PINK,
        WildTile::Heartwood => RUST,
        WildTile::AmberResin => AMBER,
        WildTile::Lifesap => LIME,
        WildTile::ElderBark => SOIL_DARK,
        WildTile::ElderSeed => GOLD,
        WildTile::SeaSand => SAND,
        WildTile::Limestone => STONE_LIGHT,
        WildTile::Coral => ROSE,
        WildTile::BrainCoral => TEAL,
        WildTile::SiltStone => DEEP_BLUE,
        WildTile::RuinStone => STONE,
        WildTile::RuinGlyph => MINT,
        WildTile::RuinRelic => GOLD,
        WildTile::TrenchBasalt => NIGHT,
        WildTile::VentRock => AMBER,
        WildTile::Smoker => ORANGE,
        WildTile::Pearlshell => CREAM,
        WildTile::GlacierIce | WildTile::SeaIce => ICE,
        WildTile::BlueIce => OCEAN,
        WildTile::Rime => BONE,
        WildTile::Permafrost => SLATE,
        WildTile::FrostCrystal => SKY_LIGHT,
        WildTile::Icicle => ICE,
        WildTile::TitanBone => BONE,
        WildTile::TitanHeart => RED,
        WildTile::Cache => STEEL_LIGHT,
    }
}

/// The wild world `terrain` is the ground of, if any.
fn wild_planet(terrain: &Terrain) -> Option<&PlanetGround> {
    terrain.planet_ground().filter(|planet| {
        matches!(
            planet.params().theme,
            PlanetThemeId::JUNGLE | PlanetThemeId::OCEAN | PlanetThemeId::GLACIER
        )
    })
}

/// The back wall of a wild world's stratum at absolute art pixel `(x, y)`.
fn back_pixel(theme: PlanetThemeId, stratum: usize, x: i32, y: i32) -> PaletteIndex {
    let value = noise(x, y, 6, 2_011).mul_add(0.7, noise(x, y, 2, 2_012) * 0.3)
        + f32::from(bayer4(x, y)) / 60.0;
    let (body, dark, fleck) = match (theme, stratum) {
        (PlanetThemeId::JUNGLE, 0) => (SOIL_DARK, INK, SOIL),
        (PlanetThemeId::JUNGLE, 1) => (NIGHT, INK, TEAL_DARK),
        (PlanetThemeId::JUNGLE, _) => (MAROON, INK, RUST),
        (PlanetThemeId::OCEAN, 0) => (STONE_DARK, NIGHT, STONE),
        (PlanetThemeId::OCEAN, 1) => (NIGHT, INK, DEEP_BLUE),
        (PlanetThemeId::OCEAN, _) => (INK, INK, SLATE_DARK),
        (_, 0) => (OCEAN, DEEP_BLUE, SKY),
        (_, 1) => (DEEP_BLUE, NIGHT, OCEAN),
        _ => (NIGHT, INK, SLATE_DARK),
    };
    if hash(x, y, 2_013).is_multiple_of(53) {
        index_of(fleck)
    } else if value > 0.68 {
        index_of(dark)
    } else {
        index_of(body)
    }
}

/// A [`Strata`] whose back wall is the wild stratum's (rounded corners, cave shadows).
fn wild_strata(strata: &Strata, theme: PlanetThemeId, stratum: usize) -> Strata {
    let mut wild = *strata;
    wild.back = [back_pixel(theme, stratum, 0, 0), index_of(INK)];
    wild
}

/// The wild stratum at `position` (0-2), for back walls.
fn stratum_at(planet: &PlanetGround, position: TilePosition) -> usize {
    planet.stratum_index(position.x, position.y).min(2)
}

/// Paints open air (and the sea) on a wild world: vines, kelp and water under the sky, the
/// strata's back walls underground. `false` anywhere else (the generic art paints it).
pub(super) fn paint_wilds_air(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    up: Cell,
    left: Cell,
    right: Cell,
) -> bool {
    let Some(planet) = wild_planet(terrain) else {
        return false;
    };
    let theme = planet.params().theme;
    let surface = planet.surface_row(position.x);
    let sky = position.y < surface;
    let fluid = terrain.fluid_at(position);
    // Under the sky: transparent, with the jungle's vines and the sea's kelp.
    if sky || (theme == PlanetThemeId::OCEAN && position.y < surface + 1 && fluid.is_some()) {
        for py in 0..TILE_ART {
            for px in 0..TILE_ART {
                canvas.set(px, py, TRANSPARENT);
            }
        }
        if theme == PlanetThemeId::JUNGLE
            && jungle::vine_at(planet, i64::from(position.x), position.y)
        {
            paint_vine(canvas, position);
        }
        if let Some(cell) = fluid {
            if theme == PlanetThemeId::OCEAN && ocean::sea_depth(planet, i64::from(position.x)) > 3
            {
                paint_kelp(canvas, terrain, position);
            }
            paint_water(canvas, terrain, position, cell.kind, cell.level);
        } else if theme == PlanetThemeId::JUNGLE
            && !jungle::vine_at(planet, i64::from(position.x), position.y)
        {
            // Grass blades on the jungle floor (the generic pass draws them on loam).
            return false;
        }
        return true;
    }
    // Underground: the stratum's own back wall, shadowed under ceilings and by walls.
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
    if let Some(cell) = fluid {
        paint_water(canvas, terrain, position, cell.kind, cell.level);
    }
    true
}

/// A vine strand down the middle of the tile: a twisting stem with leaves.
fn paint_vine(canvas: &mut TileCanvas, position: TilePosition) {
    let offset = (hash(position.x, 0, 2_021) % 8) as i32 + 4;
    for py in 0..TILE_ART {
        let gy = canvas.origin_y + py;
        let sway = ((gy / 5).rem_euclid(3)) - 1;
        let x = offset + sway;
        canvas.set(x, py, index_of(PINE));
        canvas.set(x + 1, py, index_of(MOSS));
        if hash(position.x, gy, 2_022).is_multiple_of(5) {
            let side = if gy.rem_euclid(2) == 0 { -2 } else { 2 };
            canvas.set(x + side, py, index_of(LEAF));
            canvas.set(x + side + side.signum(), py, index_of(LIME));
        }
    }
}

/// Kelp swaying up through the water from the seabed.
fn paint_kelp(canvas: &mut TileCanvas, terrain: &Terrain, position: TilePosition) {
    let Some(planet) = terrain.planet_ground() else {
        return;
    };
    let floor = planet.surface_row(position.x);
    let height = 2 + (hash(position.x, 1, 2_031) % 6) as i32;
    if !hash(position.x, 0, 2_030).is_multiple_of(3) || position.y < floor - height {
        return;
    }
    let base = (hash(position.x, 2, 2_032) % 10) as i32 + 3;
    for py in 0..TILE_ART {
        let gy = canvas.origin_y + py;
        let sway = ((gy / 4).rem_euclid(4) - 1).clamp(-1, 1);
        canvas.set(base + sway, py, index_of(PINE));
        if gy.rem_euclid(6) == 0 {
            canvas.set(base + sway + 1, py, index_of(MOSS));
        }
    }
}

/// The fluid filling the tile from the bottom to its level: a lit surface line where open air
/// is above it, deeper blues below.
fn paint_water(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    kind: FluidKind,
    level: u8,
) {
    let filled = (i32::from(level) * TILE_ART / i32::from(FLUID_FULL)).clamp(0, TILE_ART);
    let above = terrain.fluid_at(TilePosition {
        x: position.x,
        y: position.y + -1,
    });
    let top = TILE_ART - filled;
    let ramp = match kind {
        FluidKind::Water => WATER,
        FluidKind::Brine => ramp([DEEP_BLUE, TEAL_DARK, TEAL, MINT]),
        FluidKind::Acid => ramp([PINE, MOSS, LEAF, LIME]),
        FluidKind::LiquidMetal => ramp([STEEL_DARK, STEEL, STEEL_LIGHT, BONE]),
    };
    for py in top..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let surface = above.is_none() && py == top;
            let index = if surface {
                if (gx / 3 + (gy / 2)).rem_euclid(4) == 0 {
                    index_of(BONE)
                } else {
                    ramp[3]
                }
            } else if above.is_none() && py == top + 1 {
                ramp[3]
            } else {
                // Deeper is darker; a shimmer of light lines in the body.
                let depth = (position.y.max(0) * TILE_ART + py) as f32;
                let shade = noise(gx / 3, gy, 7, 2_041) * 0.12
                    + f32::from(bayer4(gx, gy)) / 120.0
                    + (depth / 480.0).min(0.85);
                if hash(gx, gy, 2_042).is_multiple_of(71) {
                    ramp[3]
                } else if shade < 0.35 {
                    ramp[2]
                } else if shade < 1.0 {
                    ramp[1]
                } else {
                    ramp[0]
                }
            };
            canvas.set(px, py, index);
        }
    }
}

/// The body pattern of `tile` at art pixel `(x, y)` (absolute), local `(px, py)`; `None` is
/// see-through (the back wall, or the sky).
fn body_pixel(tile: WildTile, x: i32, y: i32, px: i32, py: i32) -> Option<PaletteIndex> {
    let n = |cell: i32, seed: u32| noise(x, y, cell, seed);
    let pick = |ramp: Ramp, value: f32| {
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
    let dither = f32::from(bayer4(x, y)) / 40.0;
    Some(match tile {
        WildTile::JungleLoam => {
            // Dark, rich loam: soft clumps, a few pale grains and the odd green sprout.
            let value = n(5, 2_102) * 0.6 + n(2, 2_103) * 0.25 + dither * 0.5;
            if hash(x, y, 2_101).is_multiple_of(41) {
                index_of(MOSS)
            } else if hash(x, y, 2_104).is_multiple_of(37) {
                LOAM[3]
            } else if value > 0.62 {
                LOAM[2]
            } else {
                LOAM[1]
            }
        }
        WildTile::RootSoil => {
            let root = (x + (n(9, 2_103) * 6.0) as i32).rem_euclid(9) == 0 && n(5, 2_104) > 0.45;
            if root {
                ROOT_SOIL[3]
            } else if n(4, 2_105) + dither * 0.4 > 0.7 {
                ROOT_SOIL[1]
            } else {
                ROOT_SOIL[0]
            }
        }
        WildTile::LivingRoot => {
            // Twisted fibres winding along the root, a moss streak on top.
            let twist = ((x + y / 2) + (n(6, 2_106) * 5.0) as i32).rem_euclid(5);
            if py < 2 && n(4, 2_107) > 0.5 {
                index_of(MOSS)
            } else {
                LIVING_ROOT[match twist {
                    0 => 0,
                    1 | 4 => 1,
                    2 => 3,
                    _ => 2,
                }]
            }
        }
        WildTile::Trunk => {
            let ridge = (x + (n(12, 2_108) * 3.0) as i32).rem_euclid(5);
            if n(7, 2_109) > 0.78 {
                index_of(MOSS)
            } else {
                BARK[match ridge {
                    0 => 0,
                    1 => 1,
                    2 | 3 => 2,
                    _ => 3,
                }]
            }
        }
        WildTile::Canopy => {
            // Clumps of leaves with sky in the gaps.
            let (near, second, dx, dy) = worley(x, y, 5, 2_110, false);
            if second - near < 2.0 && hash(x, y, 2_111).is_multiple_of(3) {
                return None;
            }
            if dx + dy < -2 {
                LEAVES[3]
            } else if dx + dy < 1 {
                LEAVES[2]
            } else if near < 9.0 {
                LEAVES[1]
            } else {
                LEAVES[0]
            }
        }
        WildTile::SporeStone => {
            if hash(x, y, 2_112).is_multiple_of(17) {
                index_of(MINT)
            } else {
                pick(SPORE, n(5, 2_113) * 0.8 + dither * 0.5)
            }
        }
        WildTile::Mycel => {
            let thread = (x - y + (n(8, 2_114) * 7.0) as i32).rem_euclid(7) == 0
                || (x + y + (n(9, 2_115) * 5.0) as i32).rem_euclid(11) == 0;
            if thread {
                index_of(BONE)
            } else {
                pick(SPORE, n(4, 2_116) * 0.7)
            }
        }
        WildTile::Puffcap => {
            // A round spotted cap on a pale stem.
            let (cx, cy) = (px - 8, py - 7);
            if (6..16).contains(&py) && (6..10).contains(&px) && py > 9 {
                index_of(if px == 6 { FOG } else { BONE })
            } else if cx * cx * 2 + cy * cy * 5 <= 120 && py <= 9 {
                if (cx + 3) * (cx + 3) + (cy + 2) * (cy + 2) <= 2
                    || (cx - 3) * (cx - 3) + (cy + 1) * (cy + 1) <= 1
                {
                    index_of(CREAM)
                } else if cy < -2 {
                    index_of(PINK)
                } else {
                    index_of(ROSE)
                }
            } else {
                return None;
            }
        }
        WildTile::Heartwood => {
            let (near, _, _, _) = worley(x, y, 14, 2_117, false);
            let ring = (near.sqrt() * 1.3 + n(5, 2_118) * 2.0) as i32;
            HEARTWOOD[[0, 1, 2, 1][ring.rem_euclid(4) as usize]]
        }
        WildTile::AmberResin => {
            // Amber glowing round a dark insect fossil.
            let (cx, cy) = (px - 8, py - 8);
            if (cy == 0 && cx.abs() <= 3)
                || (cy.abs() <= 1 && cx == 1)
                || (cx.abs() == 2 && cy == -1)
            {
                index_of(SOIL_DARK)
            } else {
                pick(
                    AMBER_RAMP,
                    0.35 + n(4, 2_119) * 0.45 + if (px + py) < 8 { 0.2 } else { 0.0 },
                )
            }
        }
        WildTile::Lifesap => {
            let (near, _, _, _) = worley(x, y, 6, 2_120, false);
            if near < 4.0 {
                SAP[3]
            } else if near < 10.0 {
                SAP[2]
            } else {
                SAP[1]
            }
        }
        WildTile::ElderBark => {
            let ridge = (x + (n(16, 2_121) * 4.0) as i32).rem_euclid(7);
            if n(9, 2_122) > 0.8 {
                index_of(MOSS)
            } else if hash(x, y, 2_123).is_multiple_of(23) {
                index_of(FOG)
            } else {
                ELDER[match ridge {
                    0 => 0,
                    1 | 2 => 1,
                    3..=5 => 2,
                    _ => 3,
                }]
            }
        }
        WildTile::ElderSeed => {
            let (cx, cy) = (px - 8, py - 9);
            if cx * cx * 3 + cy * cy * 2 <= 60 {
                if cx < -1 && cy < -1 {
                    index_of(CREAM)
                } else {
                    index_of(if cx + cy > 3 { AMBER } else { GOLD })
                }
            } else if py > 12 {
                index_of(if (px + py).rem_euclid(2) == 0 {
                    SOIL
                } else {
                    CLAY
                })
            } else {
                return None;
            }
        }
        WildTile::SeaSand => {
            // Pale sand in soft ripples, a pink shell fleck here and there.
            let ripple = (y + (n(12, 2_126) * 4.0) as i32 + x / 6).rem_euclid(5) == 0;
            if hash(x, y, 2_124).is_multiple_of(53) {
                index_of(ROSE)
            } else if ripple && n(6, 2_127) > 0.4 {
                SAND_RAMP[1]
            } else if hash(x, y, 2_125).is_multiple_of(11) {
                SAND_RAMP[3]
            } else {
                SAND_RAMP[2]
            }
        }
        WildTile::Limestone => {
            // Warm layered limestone with the odd fossil shell spiral.
            let layer = (y + (n(14, 2_126) * 3.0) as i32).rem_euclid(6) == 0;
            let (near, _, _, _) = worley(x, y, 11, 2_127, false);
            if (4.0..6.5).contains(&near) && hash(x / 4, y / 4, 2_128).is_multiple_of(5) {
                LIMESTONE[0]
            } else if layer {
                LIMESTONE[1]
            } else if n(5, 2_129) + dither * 0.3 > 0.72 {
                LIMESTONE[3]
            } else {
                LIMESTONE[2]
            }
        }
        WildTile::Coral => {
            // Branching coral: a trunk and forking arms reaching up.
            let trunk = (px - 7).abs() <= 1 && py > 6;
            let arm_l = (px - (7 - (12 - py) / 2)).abs() <= 1 && (2..12).contains(&py);
            let arm_r = (px - (8 + (12 - py) / 2)).abs() <= 1 && (3..12).contains(&py);
            let twig = (px - 3).abs() <= 0 && (1..6).contains(&py);
            if trunk || arm_l || arm_r || twig || py >= 14 {
                CORAL[if py < 4 {
                    3
                } else if (px + py).rem_euclid(3) == 0 {
                    1
                } else {
                    2
                }]
            } else {
                return None;
            }
        }
        WildTile::BrainCoral => {
            let (cx, cy) = (px - 8, py - 9);
            if cx * cx + cy * cy * 2 > 80 && py < 14 {
                return None;
            }
            let groove = (x + (n(5, 2_130) * 6.0) as i32 + y / 2).rem_euclid(4) == 0;
            BRAIN[if groove {
                0
            } else if cy < -3 {
                3
            } else {
                2
            }]
        }
        WildTile::SiltStone => {
            let layer = (y + (n(20, 2_131) * 4.0) as i32).rem_euclid(4) == 0;
            if layer {
                SILT[0]
            } else {
                pick(SILT, 0.3 + n(6, 2_132) * 0.5 + dither * 0.3)
            }
        }
        WildTile::RuinStone | WildTile::RuinGlyph => {
            // Cut blocks in courses, mortar lines, a green bloom of algae.
            let course = y.rem_euclid(8) == 0;
            let joint = (x + if y.div_euclid(8).rem_euclid(2) == 0 {
                0
            } else {
                6
            })
            .rem_euclid(12)
                == 0;
            if tile == WildTile::RuinGlyph && (3..13).contains(&px) && (3..13).contains(&py) {
                // A glowing glyph: a ring with a cross.
                let (cx, cy) = (px - 8, py - 8);
                let ring = (cx * cx + cy * cy - 16).abs() <= 4;
                let cross = cx == 0 || cy == 0;
                if ring || (cross && cx.abs() + cy.abs() <= 3) {
                    return Some(index_of(if ring { MINT } else { TEAL }));
                }
            }
            if course || joint {
                RUIN[0]
            } else if n(7, 2_133) > 0.72 {
                index_of(TEAL_DARK)
            } else {
                pick(RUIN, 0.4 + n(4, 2_134) * 0.5)
            }
        }
        WildTile::RuinRelic => {
            // A golden tablet on a stone plinth.
            if py >= 12 {
                RUIN[if py == 12 { 3 } else { 1 }]
            } else if (3..13).contains(&px) && py >= 2 {
                if (px == 3 || px == 12 || py == 2) && py < 12 {
                    index_of(AMBER)
                } else if (py - 2).rem_euclid(3) == 0 && (5..11).contains(&px) {
                    index_of(MINT)
                } else {
                    index_of(GOLD)
                }
            } else {
                return None;
            }
        }
        WildTile::TrenchBasalt => {
            let (near, second, _, _) = worley(x, y, 7, 2_135, false);
            if second - near < 2.5 {
                BASALT[0]
            } else {
                pick(BASALT, 0.35 + n(5, 2_136) * 0.5)
            }
        }
        WildTile::VentRock => {
            if n(4, 2_137) > 0.68 {
                pick(VENT, 0.6 + n(2, 2_138) * 0.4)
            } else {
                pick(BASALT, 0.3 + n(5, 2_139) * 0.6)
            }
        }
        WildTile::Smoker => {
            // A chimney with glowing cracks.
            let crack =
                (y + (n(6, 2_140) * 8.0) as i32).rem_euclid(7) == 0 && (3..13).contains(&px);
            if crack {
                index_of(if bayer4(x, y) < 8 { ORANGE } else { AMBER })
            } else if px < 3 || px > 12 {
                BASALT[0]
            } else {
                pick(BASALT, 0.4 + n(4, 2_141) * 0.5)
            }
        }
        WildTile::Pearlshell => {
            let (cx, cy) = (px - 8, py - 10);
            if cx * cx + cy * cy * 3 > 70 {
                return None;
            }
            if (cx - 1) * (cx - 1) + (cy + 1) * (cy + 1) <= 4 {
                index_of(if cx < 1 && cy < -1 { BONE } else { CREAM })
            } else if (cx + 8).rem_euclid(3) == 0 {
                index_of(ROSE)
            } else {
                index_of(PINK)
            }
        }
        WildTile::GlacierIce => {
            // Long, faint annual layers in pale ice; a glint now and then.
            let line = (y + (n(24, 2_142) * 4.0) as i32).rem_euclid(9) == 0 && n(10, 2_145) > 0.3;
            if hash(x, y, 2_143).is_multiple_of(97) {
                index_of(BONE)
            } else if line || n(12, 2_144) + dither * 0.3 > 0.82 {
                GLACIER[1]
            } else {
                GLACIER[2]
            }
        }
        WildTile::BlueIce | WildTile::FrostCrystal => {
            let (near, second, dx, dy) = worley(x, y, 9, 2_145, false);
            if tile == WildTile::FrostCrystal && near < 12.0 {
                // Clear crystal points.
                return Some(index_of(if dx + dy < 0 { BONE } else { SKY_LIGHT }));
            }
            if second - near < 1.5 {
                BLUE_ICE[3]
            } else if dx + dy < -3 {
                BLUE_ICE[2]
            } else {
                pick(BLUE_ICE, 0.2 + n(6, 2_146) * 0.5)
            }
        }
        WildTile::SeaIce => {
            let (near, second, _, _) = worley(x, y, 12, 2_147, true);
            if second - near < 1.5 {
                SEA_ICE[0]
            } else {
                pick(SEA_ICE, 0.4 + n(5, 2_148) * 0.5)
            }
        }
        WildTile::Rime => {
            // Feathery frost over the gap it closed.
            if (x + y).rem_euclid(3) == 0 || (x - y).rem_euclid(4) == 0 {
                index_of(if hash(x, y, 2_149).is_multiple_of(3) {
                    ICE
                } else {
                    BONE
                })
            } else {
                return None;
            }
        }
        WildTile::Permafrost => {
            if hash(x, y, 2_150).is_multiple_of(13) {
                index_of(ICE)
            } else {
                let (near, _, _, _) = worley(x, y, 4, 2_151, false);
                PERMAFROST[if near < 2.0 {
                    3
                } else if near < 5.0 {
                    2
                } else {
                    1
                }]
            }
        }
        WildTile::Icicle => {
            // Hanging from the top edge, tapering to a point.
            let half = (TILE_ART - py) / 3;
            if (px - 8).abs() <= half {
                index_of(match px.cmp(&8) {
                    std::cmp::Ordering::Less => BONE,
                    std::cmp::Ordering::Equal => ICE,
                    std::cmp::Ordering::Greater => SKY_LIGHT,
                })
            } else {
                return None;
            }
        }
        WildTile::TitanBone => {
            if hash(x, y, 2_152).is_multiple_of(9) {
                BONE_RAMP[0]
            } else {
                pick(BONE_RAMP, 0.45 + n(5, 2_153) * 0.5)
            }
        }
        WildTile::TitanHeart => {
            let (cx, cy) = (px - 8, py - 8);
            if cx * cx + cy * cy > 40 {
                return Some(pick(BLUE_ICE, 0.4 + n(4, 2_154) * 0.4));
            }
            HEART[if cx + cy < -3 {
                3
            } else if cx + cy < 1 {
                2
            } else {
                1
            }]
        }
        WildTile::Cache => {
            if px < 2 || px > 13 || py < 4 || py > 14 {
                return None;
            }
            if (px - 8).abs() + (py - 9).abs() <= 2 {
                index_of(GOLD)
            } else if py == 4 || px == 2 || px == 13 || py == 14 {
                index_of(STEEL_DARK)
            } else {
                index_of(STEEL)
            }
        }
    })
}

/// The rock ore sits in at `position` on a wild world (`None` elsewhere).
#[must_use]
pub(super) fn host_rock(terrain: &Terrain, position: TilePosition) -> Option<TileKind> {
    let planet = wild_planet(terrain)?;
    let theme = fatherload_simulation::planet::theme(planet.params().theme);
    if position.y < planet.surface_row(position.x) + theme.crust_rows {
        return Some(theme.crust);
    }
    Some(theme.strata[stratum_at(planet, position)].rock)
}

/// One art pixel of a wild rock (ore's host): its body, solid throughout.
#[must_use]
pub(super) fn rock_pixel(tile: WildTile, x: i32, y: i32) -> PaletteIndex {
    body_pixel(tile, x, y, x.rem_euclid(TILE_ART), y.rem_euclid(TILE_ART))
        .unwrap_or_else(|| index_of(wild_color(tile)))
}

/// Paints wild tile `tile` at `position`.
pub(super) fn paint(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: WildTile,
    strata: &Strata,
    edges: Edges,
) {
    let planet = terrain.planet_ground();
    let theme = planet.map_or(PlanetThemeId::JUNGLE, |planet| planet.params().theme);
    let stratum = planet.map_or(0, |planet| stratum_at(planet, position));
    let sky = planet.is_some_and(|planet| position.y < planet.surface_row(position.x))
        || edges.up == Cell::Sky;
    let strata = wild_strata(strata, theme, stratum);
    let mut open = false;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let index = body_pixel(tile, gx, gy, px, py).unwrap_or_else(|| {
                // Coral and shells buried in the shelf are fossils: limestone round them.
                if !sky
                    && matches!(
                        tile,
                        WildTile::Coral | WildTile::BrainCoral | WildTile::Pearlshell
                    )
                    && terrain
                        .fluid_at(TilePosition {
                            x: position.x,
                            y: position.y - 1,
                        })
                        .is_none()
                {
                    return rock_pixel(WildTile::Limestone, gx, gy);
                }
                open = true;
                // See-through parts show the sky, the sea or the stratum's back wall.
                if sky {
                    TRANSPARENT
                } else {
                    back_pixel(theme, stratum, gx, gy)
                }
            });
            canvas.set(px, py, index);
        }
    }
    // Coral, shells and relics stand in the sea: the water shows round them.
    if open
        && let Some(cell) = terrain.fluid_at(TilePosition {
            x: position.x,
            y: position.y - 1,
        })
    {
        let _ = cell;
        let water = index_of(OCEAN);
        for py in 0..TILE_ART {
            for px in 0..TILE_ART {
                if canvas.get(px, py) == TRANSPARENT {
                    canvas.set(px, py, water);
                }
            }
        }
    }
    if open {
        return;
    }
    let (lip, shade) = match tile {
        WildTile::JungleLoam | WildTile::RootSoil => (LOAM[3], LOAM[0]),
        WildTile::LivingRoot | WildTile::Trunk | WildTile::ElderBark => (BARK[3], BARK[0]),
        WildTile::SporeStone | WildTile::Mycel => (SPORE[3], SPORE[0]),
        WildTile::Heartwood | WildTile::AmberResin | WildTile::Lifesap => {
            (HEARTWOOD[3], HEARTWOOD[0])
        }
        WildTile::SeaSand | WildTile::Limestone => (SAND_RAMP[3], SAND_RAMP[0]),
        WildTile::SiltStone => (SILT[3], SILT[0]),
        WildTile::RuinStone | WildTile::RuinGlyph => (RUIN[3], RUIN[0]),
        WildTile::TrenchBasalt | WildTile::VentRock | WildTile::Smoker => (BASALT[3], BASALT[0]),
        WildTile::GlacierIce | WildTile::SeaIce => (GLACIER[3], GLACIER[0]),
        WildTile::BlueIce | WildTile::FrostCrystal => (BLUE_ICE[3], BLUE_ICE[0]),
        WildTile::Permafrost => (PERMAFROST[3], PERMAFROST[0]),
        WildTile::TitanBone | WildTile::TitanHeart => (BONE_RAMP[3], BONE_RAMP[0]),
        _ => (LEAVES[3], LEAVES[0]),
    };
    let square = matches!(tile, WildTile::RuinStone | WildTile::RuinGlyph);
    paint_edges(canvas, &strata, edges, lip, shade, square);
    // Snow on the glacier's open tops.
    if edges.up == Cell::Sky
        && matches!(
            tile,
            WildTile::GlacierIce | WildTile::BlueIce | WildTile::SeaIce | WildTile::Permafrost
        )
    {
        for px in 0..TILE_ART {
            let drift = (hash(canvas.origin_x + px, 0, 2_160) % 3) as i32;
            for py in 0..=drift {
                canvas.set(px, py, index_of(if py == 0 { BONE } else { ICE }));
            }
        }
    }
}

/// Whether grass grows on `tile` where it meets the sky (the jungle's soils only).
#[must_use]
pub const fn grows_grass(tile: WildTile) -> bool {
    matches!(tile, WildTile::JungleLoam | WildTile::RootSoil)
}

/// The jungle's grass colours.
#[must_use]
pub fn grass(terrain: &Terrain) -> Option<(PaletteIndex, PaletteIndex, PaletteIndex)> {
    wild_planet(terrain)
        .filter(|planet| planet.params().theme == PlanetThemeId::JUNGLE)
        .map(|_| (index_of(LIME), index_of(LEAF), index_of(PINE)))
}
