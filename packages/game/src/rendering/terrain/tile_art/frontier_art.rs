//! Tile art for the frontier worlds (Wave 12 planets-b, `fatherload_simulation::frontier`):
//! the dune, volcanic and crystal planets, and their tunnels' back walls.
//!
//! * The dune world: rippled dune sand, packed sand in thin beds, banded sandstone in reds and
//!   golds, the ruins' weathered bricks and cracked floor slabs, a buried cache's gilded urn,
//!   desert glass in green facets, salt in white cubes, glazed ringed wormstone, palms and the
//!   glowing sun glyphs.
//! * The volcanic world: soft grey ash with embers, porous pumice, basalt in hexagonal
//!   columns, red vesicular scoria, glossy obsidian with a violet sheen, ropey tube crust,
//!   magma rock split by glowing cracks, a magma heart's gold core, cinders and sulphur.
//! * The crystal world: lilac quartz soil, shard stone with embedded splinters, big faceted
//!   amethyst crystal, ice-blue spires, prismatic stone in rainbow bands, glass prisms, the sun
//!   lens, receptors dark and lit, the vault's teal seal, agate-banded geode shells, amethyst
//!   geode hearts and the deep-blue heavy lattice.

#![allow(
    clippy::match_same_arms,
    clippy::if_same_then_else,
    clippy::comparison_chain,
    clippy::bool_to_int_with_if,
    reason = "pixel art reads best one shade per case"
)]

use fatherload_simulation::{
    frontier::{
        FrontierStyle, FrontierTile,
        rules::{Zone, zone_at},
        style_of_ground,
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

// The dune world.
const DUNE: Ramp = ramp([LOAM, CLAY, SAND, CREAM]);
const PACKED: Ramp = ramp([SOIL, LOAM, CLAY, SAND]);
const SANDSTONE: Ramp = ramp([MAROON, RUST, CLAY, SAND]);
const BRICK: Ramp = ramp([SOIL_DARK, LOAM, SAND, CREAM]);
const GLASS: Ramp = ramp([TEAL_DARK, TEAL, MINT, ICE]);
const SALT: Ramp = ramp([FOG, MIST, BONE, PINK]);
const WORM: Ramp = ramp([SOIL_DARK, SOIL, LOAM, AMBER]);
const PALM: Ramp = ramp([SOIL_DARK, SOIL, LOAM, CLAY]);
const FROND: Ramp = ramp([PINE, MOSS, LEAF, LIME]);
const SUN: Ramp = ramp([RUST, AMBER, GOLD, CREAM]);
// The volcanic world.
const ASH: Ramp = ramp([STONE_DARK, STONE, STONE_LIGHT, FOG]);
const PUMICE: Ramp = ramp([STONE_DARK, STONE, STONE_LIGHT, MIST]);
const BASALT: Ramp = ramp([NIGHT, STONE_DARK, STONE, STONE_LIGHT]);
const SCORIA: Ramp = ramp([SOIL_DARK, MAROON, RUST, COPPER]);
const OBSIDIAN: Ramp = ramp([INK, NIGHT, PLUM, LAVENDER]);
const TUBE: Ramp = ramp([INK, NIGHT, MAROON, RUST]);
const MAGMA: Ramp = ramp([NIGHT, MAROON, RED, ORANGE]);
const HEAT: Ramp = ramp([RED, ORANGE, AMBER, GOLD]);
const SULPHUR: Ramp = ramp([LOAM, AMBER, GOLD, CREAM]);
// The crystal world.
const QUARTZ: Ramp = ramp([SLATE, SLATE_LIGHT, MIST, BONE]);
const SHARD: Ramp = ramp([NIGHT, SLATE_DARK, SLATE, SLATE_LIGHT]);
const AMETHYST: Ramp = ramp([PLUM, VIOLET, LAVENDER, BONE]);
const SPIRE: Ramp = ramp([OCEAN, SKY, SKY_LIGHT, ICE]);
const PRISM_GLASS: Ramp = ramp([SLATE_DARK, SKY_LIGHT, ICE, BONE]);
const SEAL: Ramp = ramp([TEAL_DARK, TEAL, MINT, ICE]);
const AGATE: Ramp = ramp([STONE_DARK, STONE, ROSE, PINK]);
const LATTICE: Ramp = ramp([INK, DEEP_BLUE, OCEAN, SKY]);
const RAINBOW: [Color; 6] = [RED, ORANGE, GOLD, LEAF, SKY, VIOLET];

type Color = raylib::prelude::Color;

/// Representative colour of a frontier tile (drill dust, debris, the map).
#[must_use]
pub const fn frontier_color(tile: FrontierTile) -> Color {
    match tile {
        FrontierTile::DuneSand | FrontierTile::RuinFloor => SAND,
        FrontierTile::PackedSand | FrontierTile::RuinBrick => CLAY,
        FrontierTile::BandedSandstone => RUST,
        FrontierTile::RuinCache | FrontierTile::SunGlyph | FrontierTile::SulphurCrust => GOLD,
        FrontierTile::DesertGlass => MINT,
        FrontierTile::SaltBed => BONE,
        FrontierTile::Wormstone | FrontierTile::PalmTrunk => LOAM,
        FrontierTile::PalmFrond => LEAF,
        FrontierTile::Ash => SLATE_LIGHT,
        FrontierTile::Pumice => STONE_LIGHT,
        FrontierTile::Basalt => STONE_DARK,
        FrontierTile::Scoria | FrontierTile::TubeCrust => MAROON,
        FrontierTile::Obsidian => NIGHT,
        FrontierTile::MagmaRock | FrontierTile::Cinder => RED,
        FrontierTile::MagmaHeart => AMBER,
        FrontierTile::QuartzSoil => MIST,
        FrontierTile::ShardStone => SLATE,
        FrontierTile::Crystal | FrontierTile::GeodeCore => LAVENDER,
        FrontierTile::SpireCrystal => SKY_LIGHT,
        FrontierTile::PrismaticStone => VIOLET,
        FrontierTile::PrismCw | FrontierTile::PrismCcw => ICE,
        FrontierTile::SunLens => CREAM,
        FrontierTile::Receptor => STEEL_DARK,
        FrontierTile::ReceptorLit | FrontierTile::VaultSeal => MINT,
        FrontierTile::GeodeShell => STONE,
        FrontierTile::DenseLattice => DEEP_BLUE,
    }
}

/// The light a frontier tile gives (halos, the light overlay's tint).
#[must_use]
#[allow(dead_code, reason = "the frontier world renderer's halos")]
pub const fn frontier_glow(tile: FrontierTile) -> Option<Color> {
    match tile {
        FrontierTile::SunGlyph | FrontierTile::SunLens => Some(GOLD),
        FrontierTile::MagmaRock | FrontierTile::Cinder => Some(ORANGE),
        FrontierTile::MagmaHeart => Some(AMBER),
        FrontierTile::Crystal | FrontierTile::GeodeCore => Some(LAVENDER),
        FrontierTile::SpireCrystal | FrontierTile::PrismCw | FrontierTile::PrismCcw => {
            Some(SKY_LIGHT)
        }
        FrontierTile::ReceptorLit | FrontierTile::VaultSeal => Some(MINT),
        FrontierTile::DesertGlass => Some(TEAL),
        _ => None,
    }
}

/// The frontier world and zone at `position` (`None` on any other ground).
#[must_use]
pub(super) fn frontier_place(
    terrain: &Terrain,
    position: TilePosition,
) -> Option<(FrontierStyle, Zone)> {
    let ground = terrain.planet_ground()?;
    let style = style_of_ground(ground)?;
    Some((style, zone_at(ground, position)))
}

/// The stratum rock ore and finds sit in at `position` of a frontier world.
#[must_use]
pub(super) fn host(style: FrontierStyle, zone: Zone) -> TileKind {
    let index = match zone {
        Zone::Surface => 0,
        Zone::Stratum(index) => usize::from(index.min(3)),
        Zone::CoreChamber => 3,
    };
    let strata = match style {
        FrontierStyle::Desert => &fatherload_simulation::frontier::desert::STRATA,
        FrontierStyle::Volcanic => &fatherload_simulation::frontier::volcanic::STRATA,
        FrontierStyle::Crystal => &fatherload_simulation::frontier::crystal::STRATA,
    };
    strata[index].rock
}

/// The back wall's body and shadow behind a frontier world's tunnels.
#[must_use]
pub(super) const fn back_pair(style: FrontierStyle, zone: Zone) -> [PaletteIndex; 2] {
    let deep = matches!(zone, Zone::Stratum(2 | 3) | Zone::CoreChamber);
    match (style, deep) {
        (FrontierStyle::Desert, false) => [index_of(SOIL), index_of(SOIL_DARK)],
        (FrontierStyle::Desert, true) => [index_of(SLATE_DARK), index_of(NIGHT)],
        (FrontierStyle::Volcanic, false) => [index_of(STONE_DARK), index_of(NIGHT)],
        (FrontierStyle::Volcanic, true) => [index_of(MAROON), index_of(INK)],
        (FrontierStyle::Crystal, false) => [index_of(SLATE_DARK), index_of(NIGHT)],
        (FrontierStyle::Crystal, true) => [index_of(PLUM), index_of(INK)],
    }
}

/// The highlight and the shade of a frontier tile's lips.
#[must_use]
pub(super) const fn lip_and_shade(tile: FrontierTile) -> (PaletteIndex, PaletteIndex) {
    let ramp = body_ramp(tile);
    (ramp[3], ramp[0])
}

const fn body_ramp(tile: FrontierTile) -> Ramp {
    match tile {
        FrontierTile::DuneSand | FrontierTile::RuinFloor => DUNE,
        FrontierTile::PackedSand => PACKED,
        FrontierTile::BandedSandstone => SANDSTONE,
        FrontierTile::RuinBrick | FrontierTile::RuinCache => BRICK,
        FrontierTile::DesertGlass => GLASS,
        FrontierTile::SaltBed => SALT,
        FrontierTile::Wormstone => WORM,
        FrontierTile::PalmTrunk => PALM,
        FrontierTile::PalmFrond => FROND,
        FrontierTile::SunGlyph | FrontierTile::SunLens => SUN,
        FrontierTile::Ash => ASH,
        FrontierTile::Pumice => PUMICE,
        FrontierTile::Basalt => BASALT,
        FrontierTile::Scoria | FrontierTile::Cinder => SCORIA,
        FrontierTile::Obsidian => OBSIDIAN,
        FrontierTile::TubeCrust => TUBE,
        FrontierTile::MagmaRock | FrontierTile::MagmaHeart => MAGMA,
        FrontierTile::SulphurCrust => SULPHUR,
        FrontierTile::QuartzSoil => QUARTZ,
        FrontierTile::ShardStone => SHARD,
        FrontierTile::Crystal | FrontierTile::GeodeCore => AMETHYST,
        FrontierTile::SpireCrystal => SPIRE,
        FrontierTile::PrismaticStone => SHARD,
        FrontierTile::PrismCw | FrontierTile::PrismCcw => PRISM_GLASS,
        FrontierTile::Receptor | FrontierTile::ReceptorLit | FrontierTile::VaultSeal => SEAL,
        FrontierTile::GeodeShell => AGATE,
        FrontierTile::DenseLattice => LATTICE,
    }
}

/// Whether the tile has see-through parts (palms; the sky or the back wall shows).
const fn see_through(tile: FrontierTile) -> bool {
    matches!(tile, FrontierTile::PalmTrunk | FrontierTile::PalmFrond)
}

fn dither(x: i32, y: i32) -> f32 {
    f32::from(bayer4(x, y)) / 40.0
}

/// One art pixel of `tile`'s body at absolute art pixel `(x, y)` (tile pixel `(px, py)`);
/// `None` is see-through.
#[allow(clippy::too_many_lines, reason = "one arm per material")]
#[allow(
    clippy::cast_possible_truncation,
    reason = "noise offsets are a few pixels"
)]
fn body(tile: FrontierTile, x: i32, y: i32, px: i32, py: i32) -> Option<PaletteIndex> {
    Some(match tile {
        FrontierTile::DuneSand => {
            // Wind ripples: long shallow diagonals, sparkles of quartz.
            let wave = (noise(x, y, 24, 2001) * 5.0) as i32;
            let ripple = (y * 3 + x / 2 + wave).rem_euclid(9);
            if hash(x, y, 2002).is_multiple_of(97) {
                DUNE[3]
            } else if ripple == 0 {
                DUNE[1]
            } else if ripple == 1 || noise(x, y, 6, 2003) + dither(x, y) > 0.86 {
                DUNE[3]
            } else {
                DUNE[2]
            }
        }
        FrontierTile::PackedSand => {
            // Thin wind-laid beds and a few pebbles.
            let wave = (noise(x, y, 20, 2011) * 4.0) as i32;
            let bed = (y + wave).div_euclid(3);
            if hash(x, y, 2012).is_multiple_of(61) {
                PACKED[0]
            } else if (y + wave).rem_euclid(3) == 0 {
                PACKED[1]
            } else if hash(bed, x.div_euclid(11), 2013).is_multiple_of(3) {
                PACKED[3]
            } else {
                PACKED[2]
            }
        }
        FrontierTile::BandedSandstone => {
            // Bands of red, rust and gold, gently folded.
            let fold = (noise(x, y, 28, 2021) * 7.0) as i32;
            let band = (y + fold).div_euclid(2);
            match hash(band, 0, 2022) % 5 {
                0 => SANDSTONE[1],
                1 => SANDSTONE[3],
                2 => index_of(AMBER),
                3 if hash(x, y, 2023).is_multiple_of(5) => SANDSTONE[0],
                _ => SANDSTONE[2],
            }
        }
        FrontierTile::RuinBrick => {
            // Weathered bricks: offset courses, mortar, sand-scoured corners.
            let course = y.div_euclid(4);
            let offset = if course.rem_euclid(2) == 0 { 0 } else { 4 };
            let column = (x + offset).rem_euclid(8);
            let row = y.rem_euclid(4);
            if row == 3 || column == 7 {
                BRICK[0]
            } else if hash(x.div_euclid(8), course, 2031).is_multiple_of(7)
                && noise(x, y, 4, 2032) > 0.55
            {
                BRICK[1]
            } else if row == 0 || column == 0 {
                BRICK[3]
            } else {
                BRICK[2]
            }
        }
        FrontierTile::RuinFloor => {
            // A floor slab with a zigzag crack: it will give.
            let crack = (px + (py / 2) * 3).rem_euclid(7) == 0 && (5..12).contains(&py);
            if py == 0 {
                BRICK[3]
            } else if py == TILE_ART - 1 || px.rem_euclid(8) == 7 {
                BRICK[0]
            } else if crack {
                index_of(INK)
            } else if py < 3 {
                DUNE[3]
            } else {
                BRICK[2]
            }
        }
        FrontierTile::RuinCache => {
            // A gilded urn bedded in sand.
            let (cx, cy) = (px - 8, py - 9);
            let body = cx * cx * 4 + cy * cy * 5 <= 150 && py >= 4;
            let neck = (4..=6).contains(&py) && cx.abs() <= 2;
            let lip = py == 3 && cx.abs() <= 3;
            if body || neck || lip {
                if py == 9 || py == 10 {
                    index_of(GOLD)
                } else if cx < -2 && cy < 1 {
                    index_of(AMBER)
                } else if cx > 3 || cy > 3 {
                    index_of(RUST)
                } else {
                    index_of(COPPER)
                }
            } else {
                DUNE[if ((x + y) * 3).rem_euclid(9) == 0 {
                    1
                } else {
                    2
                }]
            }
        }
        FrontierTile::DesertGlass => {
            // Green glass in fractured facets, bright fracture lines.
            let (near, second, dx, dy) = worley(x, y, 6, 2041, false);
            if second - near < 0.9 {
                GLASS[3]
            } else if dx + dy < -2 {
                GLASS[2]
            } else if dx + dy > 2 {
                GLASS[0]
            } else {
                GLASS[1]
            }
        }
        FrontierTile::SaltBed => {
            // White salt in little cubes, a pink blush here and there.
            let (gx, gy) = (x.rem_euclid(5), y.rem_euclid(5));
            if gx == 4 || gy == 4 {
                SALT[0]
            } else if gx == 0 && gy == 0 {
                SALT[2]
            } else if noise(x, y, 9, 2051) + dither(x, y) > 0.8 {
                SALT[3]
            } else {
                SALT[1]
            }
        }
        FrontierTile::Wormstone => {
            // Sand glazed by the wyrms: concentric rings with a glossy streak.
            let (cx, cy) = (x.div_euclid(24) * 24 + 12, y.div_euclid(24) * 24 + 12);
            let (dx, dy) = (x - cx, y - cy);
            let ring = ((dx * dx + dy * dy) as f32).sqrt() as i32 / 2;
            if ring.rem_euclid(3) == 0 {
                WORM[1]
            } else if (x - y).rem_euclid(13) == 0 && hash(x, y, 2061).is_multiple_of(2) {
                WORM[3]
            } else if ring.rem_euclid(3) == 1 {
                WORM[2]
            } else {
                WORM[0]
            }
        }
        FrontierTile::PalmTrunk => {
            // A ringed trunk, six pixels wide.
            if !(5..=10).contains(&px) {
                return None;
            }
            let ring = (py + if px < 8 { 0 } else { 1 }).rem_euclid(4);
            if px == 5 || px == 10 {
                PALM[0]
            } else if ring == 0 {
                PALM[1]
            } else if px == 6 {
                PALM[3]
            } else {
                PALM[2]
            }
        }
        FrontierTile::PalmFrond => {
            // Drooping fronds: arcs of leaves, sky between them.
            let arc = |reach: i32| (px - 8).abs() * (px - 8).abs() / reach;
            let leaf_a = py == 4 + arc(10) || py == 5 + arc(10);
            let leaf_b = py == 8 + arc(16);
            let spine = (6..=9).contains(&px) && py <= 6;
            if !(leaf_a || leaf_b || spine) || py > 13 {
                return None;
            }
            if spine {
                PALM[2]
            } else if (px + py).rem_euclid(3) == 0 {
                FROND[3]
            } else if py > 8 {
                FROND[1]
            } else {
                FROND[2]
            }
        }
        FrontierTile::SunGlyph => {
            // A sun carved in sandstone, glowing: a disc and rays.
            let (dx, dy) = (px - 8, py - 8);
            let r2 = dx * dx + dy * dy;
            let ray = (dx == 0 || dy == 0 || dx.abs() == dy.abs()) && (16..=42).contains(&r2);
            if r2 <= 6 {
                SUN[3]
            } else if r2 <= 12 {
                SUN[2]
            } else if ray {
                SUN[1]
            } else {
                SANDSTONE[(x + y).rem_euclid(5).min(3) as usize]
            }
        }
        FrontierTile::Ash => {
            // Soft grey drifts, now and then an ember.
            if hash(x, y, 2101).is_multiple_of(113) {
                index_of(ORANGE)
            } else {
                // Soft drifts laid down by the wind: fine grain in gentle bands.
                let n = noise(x, y, 3, 2102) * 0.6
                    + noise(x, y + x / 3, 9, 2103) * 0.4
                    + dither(x, y) * 0.6;
                if n > 0.95 {
                    ASH[3]
                } else if n > 0.7 {
                    ASH[2]
                } else if n > 0.35 {
                    ASH[1]
                } else {
                    ASH[0]
                }
            }
        }
        FrontierTile::Pumice => {
            // Porous and pale: little dark pores.
            let (near, _, dx, dy) = worley(x, y, 4, 2111, false);
            if near < 1.1 && hash(x.div_euclid(4), y.div_euclid(4), 2112).is_multiple_of(2) {
                PUMICE[0]
            } else if dx + dy < -1 {
                PUMICE[3]
            } else if near > 2.4 {
                PUMICE[2]
            } else {
                PUMICE[1]
            }
        }
        FrontierTile::Basalt => {
            // Columns: vertical joints every few pixels, the column tops catching the light.
            let column = (x + (y.div_euclid(12) * 3)).rem_euclid(6);
            if column == 0 {
                BASALT[0]
            } else if y.rem_euclid(12) == 0 {
                BASALT[3]
            } else if column == 1 {
                BASALT[3]
            } else if column >= 4 {
                BASALT[1]
            } else {
                BASALT[2]
            }
        }
        FrontierTile::Scoria => {
            // Red rock full of vesicles.
            let (near, _, dx, dy) = worley(x, y, 5, 2121, false);
            if near < 1.3 {
                SCORIA[0]
            } else if dx + dy < -2 {
                SCORIA[3]
            } else if hash(x, y, 2122).is_multiple_of(3) {
                SCORIA[1]
            } else {
                SCORIA[2]
            }
        }
        FrontierTile::Obsidian => {
            // Glossy black glass: conchoidal ripples and a violet sheen down the diagonal.
            let (near, second, dx, _) = worley(x, y, 9, 2131, false);
            let sheen = (x + y).rem_euclid(14);
            if second - near < 1.0 {
                OBSIDIAN[1]
            } else if sheen == 0 || (sheen == 1 && dx < 0) {
                OBSIDIAN[3]
            } else if sheen <= 3 {
                OBSIDIAN[2]
            } else {
                OBSIDIAN[0]
            }
        }
        FrontierTile::TubeCrust => {
            // Ropey glassy lava, ember glints.
            let rope = (y * 2 + (noise(x, y, 8, 2141) * 6.0) as i32).rem_euclid(5);
            if hash(x, y, 2142).is_multiple_of(89) {
                index_of(ORANGE)
            } else if rope == 0 {
                TUBE[0]
            } else if rope == 1 {
                TUBE[3]
            } else if rope == 2 {
                TUBE[2]
            } else {
                TUBE[1]
            }
        }
        FrontierTile::MagmaRock => {
            // Dark crust split by glowing cracks.
            let (near, second, _, _) = worley(x, y, 7, 2151, false);
            let edge = second - near;
            if edge < 0.8 {
                HEAT[2]
            } else if edge < 1.6 {
                HEAT[0]
            } else if noise(x, y, 6, 2152) + dither(x, y) > 0.85 {
                MAGMA[1]
            } else {
                MAGMA[0]
            }
        }
        FrontierTile::MagmaHeart => {
            // A heart of molten gold in a ring of magma.
            let (dx, dy) = (px - 8, py - 8);
            let r2 = dx * dx + dy * dy;
            if r2 <= 5 {
                HEAT[3]
            } else if r2 <= 16 {
                HEAT[2]
            } else if r2 <= 30 {
                HEAT[1]
            } else if r2 <= 42 {
                MAGMA[2]
            } else {
                MAGMA[0]
            }
        }
        FrontierTile::Cinder => {
            // A cooled bomb: lumpy dark rock, red seams still hot.
            let (near, second, _, _) = worley(x, y, 5, 2161, false);
            if second - near < 0.7 {
                index_of(RED)
            } else if near < 1.5 {
                SCORIA[1]
            } else {
                BASALT[1]
            }
        }
        FrontierTile::SulphurCrust => {
            // Yellow sulphur crystals crusting grey rock.
            let n = noise(x, y, 4, 2171) + dither(x, y);
            if n > 0.9 {
                SULPHUR[3]
            } else if n > 0.6 {
                SULPHUR[2]
            } else if n > 0.4 {
                SULPHUR[1]
            } else {
                ASH[1]
            }
        }
        FrontierTile::QuartzSoil => {
            // Pale lilac soil glittering with quartz.
            if hash(x, y, 2201).is_multiple_of(53) {
                QUARTZ[3]
            } else if hash(x, y, 2202).is_multiple_of(71) {
                index_of(LAVENDER)
            } else {
                let n = noise(x, y, 6, 2203) + dither(x, y);
                if n > 0.85 {
                    QUARTZ[2]
                } else if n > 0.4 {
                    QUARTZ[1]
                } else {
                    QUARTZ[0]
                }
            }
        }
        FrontierTile::ShardStone => {
            // Violet-grey stone with splinters of crystal set in it.
            let shard = (x * 2 - y).rem_euclid(17) < 2
                && y.rem_euclid(6) < 3
                && hash(x.div_euclid(6), y.div_euclid(6), 2211).is_multiple_of(3);
            if shard {
                if (x + y).rem_euclid(2) == 0 {
                    index_of(LAVENDER)
                } else {
                    index_of(PINK)
                }
            } else {
                let (near, second, dx, dy) = worley(x, y, 9, 2212, true);
                if second - near < 1.4 {
                    SHARD[0]
                } else if dx < -2 && dy < 0 {
                    SHARD[3]
                } else if dy > 1 {
                    SHARD[1]
                } else {
                    SHARD[2]
                }
            }
        }
        FrontierTile::Crystal => {
            // Big amethyst facets, bright edges and a glint.
            let (near, second, dx, dy) = worley(x, y, 7, 2221, false);
            if hash(x, y, 2222).is_multiple_of(67) {
                AMETHYST[3]
            } else if second - near < 1.0 {
                AMETHYST[3]
            } else if dx - dy < -2 {
                AMETHYST[2]
            } else if dx - dy > 3 {
                AMETHYST[0]
            } else {
                AMETHYST[1]
            }
        }
        FrontierTile::GeodeCore => {
            // A cluster of amethyst points radiating from the bottom.
            let (dx, dy) = (px - 8, 15 - py);
            let angle_band = if dy == 0 {
                0
            } else {
                (dx * 6 / dy.max(1)).abs()
            };
            let point = dy > 0 && dy <= 13 - dx.abs() / 2 && angle_band.rem_euclid(3) != 2;
            if point {
                if dx < 0 {
                    AMETHYST[3]
                } else if dx == 0 || dx == 1 {
                    AMETHYST[2]
                } else {
                    AMETHYST[1]
                }
            } else {
                AGATE[if (px + py).rem_euclid(4) == 0 { 2 } else { 0 }]
            }
        }
        FrontierTile::SpireCrystal => {
            // Ice-blue pillar facets running up.
            let facet = (x + (y / 6) % 2).rem_euclid(5);
            if facet == 0 {
                SPIRE[3]
            } else if facet == 4 {
                SPIRE[0]
            } else if hash(x, y, 2231).is_multiple_of(41) {
                index_of(BONE)
            } else if facet == 1 {
                SPIRE[2]
            } else {
                SPIRE[1]
            }
        }
        FrontierTile::PrismaticStone => {
            // Dark stone banded in every colour, the bands slanting.
            let slant = (y + x / 3 + (noise(x, y, 20, 2241) * 5.0) as i32).div_euclid(2);
            let colour = RAINBOW[slant.rem_euclid(6) as usize];
            if hash(slant, x.div_euclid(16), 2242).is_multiple_of(5) {
                index_of(colour)
            } else if slant.rem_euclid(2) == 0 {
                SHARD[1]
            } else {
                SHARD[0]
            }
        }
        FrontierTile::PrismCw | FrontierTile::PrismCcw => {
            // A glass prism: a triangle of clear glass split along the diagonal it turns light
            // round, a rainbow along its edge.
            let diagonal = if tile == FrontierTile::PrismCcw {
                px + py - (TILE_ART - 1)
            } else {
                px - py
            };
            if diagonal.abs() <= 1 {
                index_of(RAINBOW[((px + py) / 3).rem_euclid(6) as usize])
            } else if diagonal < 0 {
                if (px + py).rem_euclid(5) == 0 {
                    PRISM_GLASS[3]
                } else {
                    PRISM_GLASS[2]
                }
            } else {
                SHARD[1]
            }
        }
        FrontierTile::SunLens => {
            // A round lens in a gold rim.
            let (dx, dy) = (px * 2 - 15, py * 2 - 15);
            let r2 = dx * dx + dy * dy;
            if r2 <= 64 {
                if dx < -2 && dy < -2 {
                    index_of(BONE)
                } else {
                    index_of(CREAM)
                }
            } else if r2 <= 150 {
                SUN[2]
            } else if r2 <= 200 {
                SUN[0]
            } else {
                SHARD[1]
            }
        }
        FrontierTile::Receptor | FrontierTile::ReceptorLit => {
            // A socket of steel holding a crystal eye: dark, or blazing when lit.
            let (dx, dy) = (px * 2 - 15, py * 2 - 15);
            let r2 = dx * dx + dy * dy;
            let lit = tile == FrontierTile::ReceptorLit;
            if r2 <= 36 {
                index_of(if lit { BONE } else { TEAL_DARK })
            } else if r2 <= 90 {
                index_of(if lit { MINT } else { NIGHT })
            } else if r2 <= 150 {
                index_of(if (px + py).rem_euclid(3) == 0 {
                    STEEL_LIGHT
                } else {
                    STEEL
                })
            } else {
                index_of(STEEL_DARK)
            }
        }
        FrontierTile::VaultSeal => {
            // A smooth teal crystal wall in a hex weave.
            let (hx, hy) = (x.rem_euclid(8), y.rem_euclid(10));
            let edge = hy == 0 || (hx == 0 && hy < 5) || (hx == 4 && hy >= 5);
            if edge {
                SEAL[0]
            } else if (x - y).rem_euclid(9) == 0 {
                SEAL[3]
            } else if hy < 3 {
                SEAL[2]
            } else {
                SEAL[1]
            }
        }
        FrontierTile::GeodeShell => {
            // Rough grey rind with agate bands.
            let band = ((x - 4).pow(2) + (y * 2).pow(2)).rem_euclid(23);
            if band < 2 {
                AGATE[2]
            } else if band == 2 {
                AGATE[3]
            } else {
                let (near, second, _, _) = worley(x, y, 6, 2251, true);
                if second - near < 1.2 {
                    AGATE[0]
                } else {
                    AGATE[1]
                }
            }
        }
        FrontierTile::DenseLattice => {
            // A deep blue lattice: crossing struts with bright nodes.
            let (gx, gy) = (x.rem_euclid(8), y.rem_euclid(8));
            let strut = (gx + gy).rem_euclid(8) == 0 || (gx - gy).rem_euclid(8) == 0;
            if gx == 0 && gy == 0 {
                LATTICE[3]
            } else if strut {
                LATTICE[2]
            } else if noise(x, y, 7, 2261) + dither(x, y) > 0.8 {
                LATTICE[1]
            } else {
                LATTICE[0]
            }
        }
    })
}

/// One art pixel of a frontier tile's body (ore and finds sit in it as their host rock).
#[must_use]
pub(super) fn base_pixel(tile: FrontierTile, x: i32, y: i32) -> PaletteIndex {
    body(tile, x, y, x.rem_euclid(TILE_ART), y.rem_euclid(TILE_ART))
        .unwrap_or_else(|| body_ramp(tile)[1])
}

/// Paints frontier tile `tile` at `position`.
pub(super) fn paint(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    tile: FrontierTile,
    strata: &Strata,
    edges: Edges,
) {
    let _ = (terrain, position);
    let mut open = false;
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let index = body(tile, gx, gy, px, py).unwrap_or_else(|| {
                open = true;
                // See-through parts show the sky over the ground, else the back wall.
                if edges.up == Cell::Sky || edges.left == Cell::Sky || edges.right == Cell::Sky {
                    TRANSPARENT
                } else {
                    super::backdrop_pixel(strata, gx, gy)
                }
            });
            canvas.set(px, py, index);
        }
    }
    if open || see_through(tile) {
        return;
    }
    let (lip, shade) = lip_and_shade(tile);
    let square = matches!(
        tile,
        FrontierTile::RuinBrick
            | FrontierTile::RuinFloor
            | FrontierTile::VaultSeal
            | FrontierTile::Receptor
            | FrontierTile::ReceptorLit
            | FrontierTile::PrismCw
            | FrontierTile::PrismCcw
    );
    paint_edges(canvas, strata, edges, lip, shade, square);
}

/// Paints an open tile of a frontier world's underground: its stratum's back wall, shadowed
/// under rock overhead and beside rock walls (like every tunnel).
pub(super) fn paint_back(
    canvas: &mut TileCanvas,
    style: FrontierStyle,
    zone: Zone,
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
                back_pixel(style, zone, gx, gy)
            };
            canvas.set(px, py, index);
        }
    }
}

/// One art pixel of a frontier world's back wall.
#[allow(
    clippy::cast_possible_truncation,
    reason = "noise offsets are a few pixels"
)]
fn back_pixel(style: FrontierStyle, zone: Zone, x: i32, y: i32) -> PaletteIndex {
    let [body, dark] = back_pair(style, zone);
    match style {
        FrontierStyle::Desert => {
            // Sand-coloured strata lines in the dark.
            let wave = (noise(x, y, 30, 2301) * 6.0) as i32;
            if (y + wave).rem_euclid(9) == 0 {
                body
            } else if noise(x, y, 12, 2302) + dither(x, y) > 0.78 {
                body
            } else {
                dark
            }
        }
        FrontierStyle::Volcanic => {
            // Black rock with faint ember veins.
            let (near, second, _, _) = worley(x, y, 14, 2311, false);
            if second - near < 0.8
                && hash(x.div_euclid(14), y.div_euclid(14), 2312).is_multiple_of(2)
            {
                index_of(MAROON)
            } else if noise(x, y, 10, 2313) + dither(x, y) > 0.8 {
                body
            } else {
                dark
            }
        }
        FrontierStyle::Crystal => {
            // A violet dark with crystal sparkles and faint lattice lines.
            if hash(x, y, 2321).is_multiple_of(139) {
                index_of(LAVENDER)
            } else if (x + y * 2).rem_euclid(23) == 0 {
                body
            } else if noise(x, y, 12, 2322) + dither(x, y) > 0.82 {
                body
            } else {
                dark
            }
        }
    }
}
