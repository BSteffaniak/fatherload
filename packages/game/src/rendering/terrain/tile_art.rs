//! Procedural pixel art for terrain tiles.
//!
//! Every tile is 16x16 art pixels painted into a buffer of palette indices. Materials (dirt,
//! clay, cobbled stone, faceted hard rock, concrete foundations) are patterns sampled in *global*
//! art-pixel coordinates, so a material flows seamlessly across tiles instead of showing a grid.
//! Each depth band (and each deep stratum) recolours the materials through its own ramps. Ores,
//! artifacts and hazards add stamps or features on top of their host rock, and the faces where
//! solid ground meets dug-out air get an ink outline, a lit lip and rounded corners, while the air
//! behind tunnels shows a dark back wall with soft contact shadows.
//!
//! Everything here is a pure function of the terrain, so tiles can be repainted one at a time.

#![allow(
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "procedural art mixes small hashed integers and short coordinate names"
)]

use drillgame_simulation::{
    terrain::{DeepStratum, Terrain, TileKind, TilePosition},
    worldgen::{Band, HazardKind},
};
use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::{
    shade::{PaletteIndex, TRANSPARENT, bayer4, index_of},
    sprite::SpriteDef,
    sprites::terrain::{VOLATILE_CRYSTAL, artifact_stamp, ore_stamp_in},
};

mod poi_art;

pub use poi_art::poi_color;

/// Art pixels per tile edge (a tile is 32 world pixels at 2 world pixels per art pixel).
pub const TILE_ART: i32 = 16;
const TILE_ART_USIZE: usize = 16;
/// Deepest tile row that is always open sky.
pub const SKY_MAX_TILE_Y: i32 = 4;

/// Four steps of one material: deepest shade, shade, body, highlight.
type Ramp = [PaletteIndex; 4];

const fn ramp(colors: [Color; 4]) -> Ramp {
    [
        index_of(colors[0]),
        index_of(colors[1]),
        index_of(colors[2]),
        index_of(colors[3]),
    ]
}

/// How one depth band colours its materials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Strata {
    dirt: Ramp,
    clay: Ramp,
    stone: Ramp,
    hard: Ramp,
    /// Back wall behind dug tunnels: body and shadow.
    back: [PaletteIndex; 2],
    /// Rare glinting specks in the rock (crystals, fossils, glowing bits).
    accent: Option<PaletteIndex>,
}

const DIRT: Ramp = ramp([SOIL_DARK, SOIL, LOAM, CLAY]);
const CLAY_RAMP: Ramp = ramp([SOIL_DARK, SOIL, RUST, CLAY]);
const GREY_STONE: Ramp = ramp([NIGHT, STONE_DARK, STONE, STONE_LIGHT]);
const SLATE_STONE: Ramp = ramp([NIGHT, SLATE_DARK, SLATE, SLATE_LIGHT]);
const DARK_ROCK: Ramp = ramp([INK, NIGHT, SLATE_DARK, SLATE]);
const BASALT: Ramp = ramp([INK, NIGHT, STONE_DARK, STONE]);
const FOUNDATION: Ramp = ramp([STONE_DARK, STONE, STONE_LIGHT, MIST]);
/// The bedrock floor: near-black obsidian slabs with violet facets.
pub const BEDROCK: Ramp = ramp([INK, NIGHT, SLATE_DARK, PLUM]);

const TOPSOIL_BAND: Strata = Strata {
    dirt: DIRT,
    clay: CLAY_RAMP,
    stone: GREY_STONE,
    hard: DARK_ROCK,
    back: [index_of(SOIL_DARK), index_of(NIGHT)],
    accent: None,
};
const CLAY_BAND: Strata = Strata {
    back: [index_of(SOIL_DARK), index_of(NIGHT)],
    ..TOPSOIL_BAND
};
const GREY_BAND: Strata = Strata {
    back: [index_of(STONE_DARK), index_of(NIGHT)],
    ..TOPSOIL_BAND
};
const SLATE_BAND: Strata = Strata {
    stone: SLATE_STONE,
    hard: BASALT,
    back: [index_of(SLATE_DARK), index_of(NIGHT)],
    accent: Some(index_of(SKY_LIGHT)),
    ..TOPSOIL_BAND
};
const BASALT_BAND: Strata = Strata {
    stone: GREY_STONE,
    hard: BASALT,
    back: [index_of(NIGHT), index_of(INK)],
    accent: Some(index_of(RED)),
    ..TOPSOIL_BAND
};

const fn deep_strata(stratum: DeepStratum) -> Strata {
    match stratum {
        DeepStratum::CrystalFaults => Strata {
            stone: ramp([NIGHT, DEEP_BLUE, OCEAN, STEEL]),
            hard: ramp([INK, NIGHT, DEEP_BLUE, OCEAN]),
            back: [index_of(DEEP_BLUE), index_of(NIGHT)],
            accent: Some(index_of(ICE)),
            ..TOPSOIL_BAND
        },
        DeepStratum::FossilOceans => Strata {
            stone: ramp([SOIL_DARK, LOAM, CLAY, SAND]),
            hard: ramp([NIGHT, SOIL_DARK, SOIL, LOAM]),
            back: [index_of(SOIL_DARK), index_of(NIGHT)],
            accent: Some(index_of(BONE)),
            ..TOPSOIL_BAND
        },
        DeepStratum::PressureCathedrals => Strata {
            stone: ramp([NIGHT, SLATE_DARK, STEEL_DARK, STEEL]),
            hard: ramp([INK, NIGHT, SLATE_DARK, STEEL_DARK]),
            back: [index_of(NIGHT), index_of(INK)],
            accent: Some(index_of(SKY_LIGHT)),
            ..TOPSOIL_BAND
        },
        DeepStratum::RadioactiveHollow => Strata {
            stone: ramp([NIGHT, PINE, MOSS, LEAF]),
            hard: ramp([INK, NIGHT, PINE, MOSS]),
            back: [index_of(PINE), index_of(NIGHT)],
            accent: Some(index_of(LIME)),
            ..TOPSOIL_BAND
        },
        DeepStratum::AncientMachineLayer => Strata {
            stone: ramp([NIGHT, STEEL_DARK, STEEL, STEEL_LIGHT]),
            hard: ramp([INK, SLATE_DARK, RUST, COPPER]),
            back: [index_of(SLATE_DARK), index_of(NIGHT)],
            accent: Some(index_of(COPPER)),
            ..TOPSOIL_BAND
        },
        DeepStratum::VoidGeodeFields => Strata {
            stone: ramp([INK, PLUM, VIOLET, LAVENDER]),
            hard: ramp([INK, NIGHT, PLUM, VIOLET]),
            back: [index_of(NIGHT), index_of(INK)],
            accent: Some(index_of(PINK)),
            ..TOPSOIL_BAND
        },
        DeepStratum::MantleStormZone => Strata {
            stone: ramp([INK, MAROON, RUST, COPPER]),
            hard: ramp([INK, NIGHT, MAROON, RUST]),
            back: [index_of(MAROON), index_of(NIGHT)],
            accent: Some(index_of(ORANGE)),
            ..TOPSOIL_BAND
        },
    }
}

/// The palettes of `band` (the world's band at a tile comes from [`Terrain::band_at`], so the
/// palettes follow the world's wavy band boundaries).
#[must_use]
pub const fn strata_for(band: Band) -> Strata {
    match band {
        Band::Topsoil => TOPSOIL_BAND,
        Band::Clay => CLAY_BAND,
        Band::Stone => GREY_BAND,
        Band::Slate => SLATE_BAND,
        Band::Basalt => BASALT_BAND,
        Band::Deep(stratum) => deep_strata(stratum),
    }
}

/// The plain rock an ore, artifact or pocket in `band` is embedded in.
const fn host_material(band: Band) -> TileKind {
    band.soft_material()
}

/// Representative colour of a tile kind in `band` (drill dust, debris, map previews).
#[must_use]
pub fn material_color(kind: TileKind, band: Band) -> Color {
    let strata = strata_for(band);
    let index = match kind {
        TileKind::Dirt => strata.dirt[2],
        TileKind::Clay => strata.clay[2],
        TileKind::Stone => strata.stone[2],
        TileKind::HardRock | TileKind::MagmaVent => strata.hard[2],
        TileKind::Foundation => FOUNDATION[2],
        TileKind::Bedrock => BEDROCK[1],
        TileKind::Lava => index_of(ORANGE),
        TileKind::Gas => index_of(LEAF),
        TileKind::Air => strata.back[0],
        TileKind::Poi(tile) => return poi_color(tile, band),
        TileKind::Ore(_)
        | TileKind::Artifact(_)
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket => return material_color(host_material(band), band),
    };
    crate::rendering::pixel_art::shade::color_at(index)
}

/// Integer hash of a lattice point (well mixed, deterministic).
#[must_use]
pub const fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    h
}

fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Smooth value noise in `0..1` with lattice spacing `cell` art pixels.
fn noise(x: i32, y: i32, cell: i32, seed: u32) -> f32 {
    let (cx, cy) = (x.div_euclid(cell), y.div_euclid(cell));
    let fx = (x.rem_euclid(cell) as f32 + 0.5) / cell as f32;
    let fy = (y.rem_euclid(cell) as f32 + 0.5) / cell as f32;
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = unit(hash(cx, cy, seed));
    let b = unit(hash(cx + 1, cy, seed));
    let c = unit(hash(cx, cy + 1, seed));
    let d = unit(hash(cx + 1, cy + 1, seed));
    let top = (b - a).mul_add(sx, a);
    let bottom = (d - c).mul_add(sx, c);
    (bottom - top).mul_add(sy, top)
}

/// Ordered-dither offset in `-0.5..0.5`.
fn dither(x: i32, y: i32) -> f32 {
    f32::from(bayer4(x, y)) / 16.0 - 0.47
}

/// Soil: a lit body with darker clumps, bright grains and dark specks.
fn dirt_level(x: i32, y: i32) -> usize {
    if hash(x, y, 3).is_multiple_of(41) {
        return 0;
    }
    let value = noise(x, y, 5, 1).mul_add(0.65, noise(x, y, 2, 2) * 0.35) + dither(x, y) * 0.16;
    if value < 0.36 {
        1
    } else if value < 0.8 {
        2
    } else {
        3
    }
}

/// Clay: a dark body crossed by wavy, broken reddish strata.
fn clay_level(x: i32, y: i32) -> usize {
    if hash(x, y, 4).is_multiple_of(47) {
        return 0;
    }
    let wave = (noise(x, y, 13, 5) * 6.0) as i32;
    let broken = noise(x, y, 7, 6) + dither(x, y) * 0.1;
    match (y + wave).rem_euclid(10) {
        0 if broken > 0.6 => 3,
        0..=1 if broken > 0.35 => 2,
        6 if broken > 0.55 => 0,
        _ => {
            if noise(x, y, 4, 7) + dither(x, y) * 0.1 > 0.8 {
                2
            } else {
                1
            }
        }
    }
}

/// Worley cells: (squared distance to nearest point, to second nearest, offset to nearest).
fn worley(x: i32, y: i32, cell: i32, seed: u32, manhattan: bool) -> (f32, f32, i32, i32) {
    let (cx, cy) = (x.div_euclid(cell), y.div_euclid(cell));
    let mut best = (f32::MAX, f32::MAX, 0, 0);
    for oy in -1..=1 {
        for ox in -1..=1 {
            let (gx, gy) = (cx + ox, cy + oy);
            let h = hash(gx, gy, seed);
            let px = gx * cell + (h % cell as u32) as i32;
            let py = gy * cell + ((h >> 8) % cell as u32) as i32;
            let (dx, dy) = (x - px, y - py);
            let distance = if manhattan {
                (dx.abs() + dy.abs()) as f32
            } else {
                ((dx * dx + dy * dy) as f32).sqrt()
            };
            if distance < best.0 {
                best = (distance, best.0, dx, dy);
            } else if distance < best.1 {
                best.1 = distance;
            }
        }
    }
    best
}

/// Stone: rounded cobbles with dark seams, lit from the top-left.
fn cobble_level(x: i32, y: i32) -> usize {
    let (near, second, dx, dy) = worley(x, y, 7, 6, false);
    if second - near < 1.1 {
        return 0;
    }
    let tilt = dx + dy;
    if tilt <= -4 {
        3
    } else if tilt >= 3 || hash(x, y, 7).is_multiple_of(13) {
        1
    } else {
        2
    }
}

/// Hard rock: big angular slabs with sharp facets.
fn slab_level(x: i32, y: i32) -> usize {
    let (near, second, dx, dy) = worley(x, y, 10, 8, true);
    if second - near < 1.5 {
        return 0;
    }
    if dx < -2 && dy < 0 {
        3
    } else if dy > 1 || hash(x, y, 9).is_multiple_of(11) {
        1
    } else {
        2
    }
}

/// Foundation: offset concrete blocks.
const fn brick_level(x: i32, y: i32) -> usize {
    let row = y.div_euclid(4);
    let offset = if row % 2 == 0 { 0 } else { 4 };
    let column = (x + offset).rem_euclid(8);
    let row_y = y.rem_euclid(4);
    if row_y == 3 || column == 7 {
        0
    } else if row_y == 0 {
        3
    } else if column == 6 {
        1
    } else {
        2
    }
}

fn base_pixel(kind: TileKind, strata: &Strata, host: TileKind, x: i32, y: i32) -> PaletteIndex {
    let accent = |index: PaletteIndex| match strata.accent {
        Some(accent) if hash(x, y, 31).is_multiple_of(89) => accent,
        _ => index,
    };
    match kind {
        TileKind::Dirt => strata.dirt[dirt_level(x, y)],
        TileKind::Clay => strata.clay[clay_level(x, y)],
        TileKind::Stone => accent(strata.stone[cobble_level(x, y)]),
        TileKind::HardRock | TileKind::MagmaVent => accent(strata.hard[slab_level(x, y)]),
        TileKind::Foundation => FOUNDATION[brick_level(x, y)],
        TileKind::Bedrock => bedrock_pixel(x, y),
        TileKind::Lava => lava_pixel(x, y),
        TileKind::Air => backdrop_pixel(strata, x, y),
        TileKind::Ore(_)
        | TileKind::Artifact(_)
        | TileKind::Gas
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket
        | TileKind::Poi(_) => base_pixel(host, strata, host, x, y),
    }
}

/// Bedrock: big diamond-faceted obsidian slabs split by black seams, their upper-left faces
/// catching a violet sheen, with rare lavender glints. Anchored to the world grid, so the floor
/// reads as one unbroken mass.
fn bedrock_pixel(x: i32, y: i32) -> PaletteIndex {
    let (near, second, dx, dy) = worley(x, y, 11, 43, true);
    if second - near < 1.5 {
        return BEDROCK[0];
    }
    if hash(x, y, 47).is_multiple_of(97) {
        return index_of(LAVENDER);
    }
    if dx + dy <= -5 {
        BEDROCK[3]
    } else if dx + dy <= -1 {
        BEDROCK[2]
    } else {
        BEDROCK[1]
    }
}

/// Molten rock: hot cores in a darker crust.
fn lava_pixel(x: i32, y: i32) -> PaletteIndex {
    let value = noise(x, y, 4, 21).mul_add(0.6, noise(x, y, 2, 22) * 0.4) + dither(x, y) * 0.14;
    index_of(if value < 0.22 {
        MAROON
    } else if value < 0.4 {
        RED
    } else if value < 0.64 {
        ORANGE
    } else if value < 0.84 {
        AMBER
    } else {
        GOLD
    })
}

/// The back wall behind a dug tunnel.
fn backdrop_pixel(strata: &Strata, x: i32, y: i32) -> PaletteIndex {
    let value = noise(x, y, 6, 11).mul_add(0.7, noise(x, y, 2, 12) * 0.3) + dither(x, y) * 0.2;
    if value > 0.66 || hash(x, y, 13).is_multiple_of(29) {
        strata.back[1]
    } else {
        strata.back[0]
    }
}

/// What fills a neighbouring cell, for edges and shadows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Cell {
    Solid,
    /// Air below the surface (a dug tunnel or cave).
    Cave,
    /// Open sky above the ground line.
    Sky,
}

impl Cell {
    const fn is_open(self) -> bool {
        !matches!(self, Self::Solid)
    }
}

fn cell_at(terrain: &Terrain, x: i32, y: i32) -> Cell {
    if y < 0 {
        return Cell::Sky;
    }
    match terrain.tile(TilePosition { x, y }) {
        Some(tile) if tile.kind == TileKind::Air => {
            if y <= SKY_MAX_TILE_Y {
                Cell::Sky
            } else {
                Cell::Cave
            }
        }
        // Solid ground, or past the world's edges (bedrock).
        _ => Cell::Solid,
    }
}

/// A 16x16 tile being painted.
struct TileCanvas {
    pixels: [PaletteIndex; TILE_ART_USIZE * TILE_ART_USIZE],
    origin_x: i32,
    origin_y: i32,
}

impl TileCanvas {
    const fn get(&self, x: i32, y: i32) -> PaletteIndex {
        self.pixels[(y * TILE_ART + x) as usize]
    }

    fn set(&mut self, x: i32, y: i32, index: PaletteIndex) {
        if (0..TILE_ART).contains(&x) && (0..TILE_ART).contains(&y) {
            self.pixels[(y * TILE_ART + x) as usize] = index;
        }
    }

    /// Paints `sprite`'s first frame with its top-left at tile pixel `(x, y)`.
    fn stamp(&mut self, sprite: &SpriteDef, x: i32, y: i32) {
        let Some(rows) = sprite.frames.first() else {
            return;
        };
        for (row_index, row) in rows.iter().enumerate() {
            for (column, symbol) in row.chars().enumerate() {
                let Some(color) = sprite.color_of(symbol) else {
                    continue;
                };
                if color.a == 0 {
                    continue;
                }
                self.set(
                    x + column as i32,
                    y + row_index as i32,
                    index_of_runtime(color),
                );
            }
        }
    }
}

fn index_of_runtime(color: Color) -> PaletteIndex {
    PALETTE
        .iter()
        .position(|(_, entry)| entry.r == color.r && entry.g == color.g && entry.b == color.b)
        .and_then(|index| PaletteIndex::try_from(index).ok())
        .unwrap_or(0)
}

/// Paints tile `position` into `out` (a row-major index buffer `stride` pixels wide) with its
/// top-left at `(origin_x, origin_y)`.
pub fn paint_tile(
    terrain: &Terrain,
    position: TilePosition,
    out: &mut [PaletteIndex],
    stride: usize,
    origin: (usize, usize),
) {
    let canvas = render_tile(terrain, position);
    for row in 0..TILE_ART_USIZE {
        let start = (origin.1 + row) * stride + origin.0;
        if let Some(target) = out.get_mut(start..start + TILE_ART_USIZE) {
            target
                .copy_from_slice(&canvas.pixels[row * TILE_ART_USIZE..(row + 1) * TILE_ART_USIZE]);
        }
    }
}

fn render_tile(terrain: &Terrain, position: TilePosition) -> TileCanvas {
    let kind = terrain
        .tile(position)
        .map_or(TileKind::Air, |tile| tile.kind);
    let band = terrain.band_at(position);
    let (strata, host) = (strata_for(band), host_material(band));
    let mut canvas = TileCanvas {
        pixels: [TRANSPARENT; TILE_ART_USIZE * TILE_ART_USIZE],
        origin_x: position.x * TILE_ART,
        origin_y: position.y * TILE_ART,
    };
    let (x, y) = (position.x, position.y);
    let up = cell_at(terrain, x, y - 1);
    let down = cell_at(terrain, x, y + 1);
    let left = cell_at(terrain, x - 1, y);
    let right = cell_at(terrain, x + 1, y);

    if kind == TileKind::Air {
        if y <= SKY_MAX_TILE_Y {
            paint_grass_blades(&mut canvas, terrain, position, down);
        } else {
            paint_cave(&mut canvas, &strata, up, left, right);
            if poi_art::inside_vault(terrain, position) {
                paint_vault_room(&mut canvas, &strata, up, left, right);
            }
            poi_art::paint_geode_lining(&mut canvas, terrain, position, band);
        }
        return canvas;
    }
    if let TileKind::Poi(tile) = kind {
        poi_art::paint(
            &mut canvas,
            terrain,
            position,
            tile,
            &strata,
            Edges {
                up,
                down,
                left,
                right,
            },
        );
        return canvas;
    }

    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, base_pixel(kind, &strata, host, gx, gy));
        }
    }
    let seed = hash(x, y, 101);
    paint_stamps(&mut canvas, kind, band, seed);
    paint_telltale(&mut canvas, terrain, position, kind, seed);
    let material = match kind {
        TileKind::Ore(_)
        | TileKind::Artifact(_)
        | TileKind::Gas
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket => host,
        other => other,
    };
    let lip = match material {
        TileKind::Dirt => strata.dirt[3],
        TileKind::Clay => strata.clay[3],
        TileKind::Stone => strata.stone[3],
        TileKind::Foundation => FOUNDATION[3],
        TileKind::Bedrock => BEDROCK[3],
        TileKind::Lava => index_of(CREAM),
        _ => strata.hard[3],
    };
    let shade = match material {
        TileKind::Dirt => strata.dirt[0],
        TileKind::Clay => strata.clay[0],
        TileKind::Stone => strata.stone[0],
        TileKind::Foundation => FOUNDATION[0],
        TileKind::Bedrock => BEDROCK[0],
        TileKind::Lava => index_of(RED),
        _ => strata.hard[0],
    };
    if up == Cell::Sky && kind != TileKind::Foundation {
        paint_grass_top(&mut canvas);
    }
    paint_edges(
        &mut canvas,
        &strata,
        Edges {
            up,
            down,
            left,
            right,
        },
        lip,
        shade,
        kind == TileKind::Foundation,
    );
    canvas
}

/// The features a tile kind adds over its host rock: pebbles, ore, artifacts and pockets.
fn paint_stamps(canvas: &mut TileCanvas, kind: TileKind, band: Band, seed: u32) {
    match kind {
        TileKind::Dirt => paint_pebbles(canvas, seed),
        TileKind::Ore(mineral) => {
            let (stamp, count) = ore_stamp_in(mineral, band);
            paint_clusters(canvas, stamp, count, seed);
        }
        TileKind::Artifact(artifact) => {
            let stamp = artifact_stamp(artifact);
            let jitter_x = (seed % 3) as i32 - 1;
            let jitter_y = ((seed >> 4) % 3) as i32 - 1;
            canvas.stamp(
                stamp,
                (TILE_ART - stamp.width()) / 2 + jitter_x,
                (TILE_ART - stamp.height()) / 2 + jitter_y,
            );
        }
        TileKind::Gas => paint_pocket(canvas, seed, [PINE, MOSS, LEAF, LIME]),
        TileKind::PressurePocket => {
            paint_pocket(canvas, seed, [DEEP_BLUE, OCEAN, SKY, ICE]);
            paint_fractures(canvas, seed);
        }
        TileKind::ExplosivePocket => {
            paint_fractures(canvas, seed);
            paint_clusters(canvas, &VOLATILE_CRYSTAL, 3, seed);
        }
        TileKind::MagmaVent => paint_fissure(canvas, seed),
        _ => {}
    }
}

/// How far (Chebyshev tiles) a hazard pocket stains the rock around it.
pub const TELLTALE_RADIUS: i32 = 2;

/// The closest hazard tile within [`TELLTALE_RADIUS`] of `position` and its distance.
fn nearby_hazard(terrain: &Terrain, position: TilePosition) -> Option<(TileKind, i32)> {
    let mut best: Option<(TileKind, i32)> = None;
    for dy in -TELLTALE_RADIUS..=TELLTALE_RADIUS {
        for dx in -TELLTALE_RADIUS..=TELLTALE_RADIUS {
            let distance = dx.abs().max(dy.abs());
            if distance == 0 || best.is_some_and(|(_, best)| best <= distance) {
                continue;
            }
            let Some(tile) = terrain.tile(TilePosition {
                x: position.x + dx,
                y: position.y + dy,
            }) else {
                continue;
            };
            if HazardKind::of_tile(tile.kind).is_some() {
                best = Some((tile.kind, distance));
            }
        }
    }
    best
}

/// Tell-tale signs on rock next to a hazard pocket, so a careful driller can read the danger
/// before breaking in: sickly green seepage near gas, glowing hot cracks near lava and magma,
/// damp blue stains near pressure pockets and yellow crystal grit near explosive pockets. The
/// stain is denser right beside the pocket and fades one tile further out.
fn paint_telltale(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    kind: TileKind,
    seed: u32,
) {
    let rock = matches!(
        kind,
        TileKind::Dirt
            | TileKind::Clay
            | TileKind::Stone
            | TileKind::HardRock
            | TileKind::Ore(_)
            | TileKind::Artifact(_)
    );
    if !rock {
        return;
    }
    let Some((hazard, distance)) = nearby_hazard(terrain, position) else {
        return;
    };
    let [dark, light] = match hazard {
        TileKind::Gas => [MOSS, LIME],
        TileKind::Lava | TileKind::MagmaVent => [RED, ORANGE],
        TileKind::PressurePocket => [OCEAN, SKY],
        _ => [RUST, AMBER],
    }
    .map(index_of);
    let density = if distance <= 1 { 13 } else { 5 };
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let roll = hash(gx, gy, 211) % 100;
            if roll < density {
                canvas.set(px, py, if roll.is_multiple_of(3) { light } else { dark });
            }
        }
    }
    if distance <= 1 {
        // A hairline crack in the hazard's colour.
        let h = hash(seed as i32, 7, 223);
        let (mut x, mut y) = (2 + (h % 12) as i32, 1 + ((h >> 4) % 4) as i32);
        let step = if h & 1 == 0 { 1 } else { -1 };
        for length in 0..9 {
            canvas.set(x, y, dark);
            if length % 3 == 1 {
                canvas.set(x + step, y, light);
            }
            y += 1;
            if (h >> (length + 8)) & 1 == 1 {
                x += step;
            }
        }
    }
}

/// One or two small stones in the soil.
fn paint_pebbles(canvas: &mut TileCanvas, seed: u32) {
    for index in 0..=seed % 2 {
        let h = hash(seed as i32, index as i32, 17);
        let x = 2 + (h % 11) as i32;
        let y = 3 + ((h >> 5) % 10) as i32;
        let (light, mid, dark) = (index_of(STONE_LIGHT), index_of(STONE), index_of(STONE_DARK));
        canvas.set(x, y, light);
        canvas.set(x + 1, y, mid);
        canvas.set(x, y + 1, mid);
        canvas.set(x + 1, y + 1, dark);
        if h & (1 << 20) != 0 {
            canvas.set(x + 2, y + 1, dark);
            canvas.set(x + 2, y, mid);
        }
    }
}

/// `count` ore clusters spread over the tile's quadrants.
fn paint_clusters(canvas: &mut TileCanvas, stamp: &SpriteDef, count: u32, seed: u32) {
    const ANCHORS: [(i32, i32); 4] = [(1, 1), (8, 2), (2, 8), (9, 8)];
    let rotation = (seed % 4) as usize;
    for index in 0..count as usize {
        let (ax, ay) = ANCHORS[(index + rotation) % ANCHORS.len()];
        let h = hash(seed as i32, index as i32, 19);
        let x = ax + (h % 3) as i32 - 1;
        let y = ay + ((h >> 3) % 2) as i32;
        let x = x.clamp(0, TILE_ART - stamp.width());
        let y = y.clamp(0, TILE_ART - stamp.height());
        canvas.stamp(stamp, x, y);
    }
}

/// A rounded cavity filled with bubbles (gas) or pressurised water (pressure pocket).
fn paint_pocket(canvas: &mut TileCanvas, seed: u32, colors: [Color; 4]) {
    let [deep, body, bubble, glint] = colors.map(index_of);
    let ink = index_of(INK);
    let (cx, cy) = (7.5 + (seed % 3) as f32 * 0.5 - 0.5, 7.5);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let wobble = noise(canvas.origin_x + px, canvas.origin_y + py, 3, seed % 97) * 1.6;
            let distance = (px as f32 - cx).hypot(py as f32 - cy);
            let radius = 6.2 + wobble - 0.8;
            if distance < radius - 1.0 {
                let lit = (px as f32 - cx) + (py as f32 - cy) < -4.0;
                canvas.set(px, py, if lit { body } else { deep });
            } else if distance < radius {
                canvas.set(px, py, ink);
            }
        }
    }
    // Bubbles: a few rings with a glint.
    for index in 0..3 {
        let h = hash(seed as i32, index, 23);
        let bx = 5 + (h % 6) as i32;
        let by = 5 + ((h >> 4) % 6) as i32;
        let size = 1 + (h >> 9) as i32 % 2;
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            canvas.set(bx + dx * size, by + dy * size, bubble);
        }
        canvas.set(bx - size + 1, by - size + 1, glint);
    }
}

/// Bright stress fractures radiating across the tile.
fn paint_fractures(canvas: &mut TileCanvas, seed: u32) {
    let ink = index_of(INK);
    for index in 0..2 {
        let h = hash(seed as i32, index, 29);
        let (mut x, mut y) = (3 + (h % 10) as i32, 2 + ((h >> 4) % 3) as i32);
        let step = if h & 1 == 0 { 1 } else { -1 };
        for length in 0..11 {
            canvas.set(x, y, ink);
            y += 1;
            if (h >> (length + 6)) & 1 == 1 {
                x += step;
            }
        }
    }
}

/// A glowing magma seam rising through the vent rock.
fn paint_fissure(canvas: &mut TileCanvas, seed: u32) {
    let colors = [GOLD, ORANGE, RED, MAROON].map(index_of);
    for py in 0..TILE_ART {
        let sway = ((hash(seed as i32, py / 3, 37) % 3) as i32) - 1;
        let centre = 8 + sway;
        let width = if py < 3 { 3 } else { 2 };
        for px in 0..TILE_ART {
            let distance = (px - centre).abs();
            if distance <= width + 1 {
                let level = (distance + i32::from(py > 12)).min(3);
                if distance <= width || hash(px, py, seed).is_multiple_of(2) {
                    canvas.set(px, py, colors[level as usize]);
                }
            }
        }
    }
}

/// Grass on ground that meets the sky.
fn paint_grass_top(canvas: &mut TileCanvas) {
    let (lime, leaf, moss) = (index_of(LIME), index_of(LEAF), index_of(MOSS));
    for px in 0..TILE_ART {
        let gx = canvas.origin_x + px;
        let h = hash(gx, 5, 41);
        canvas.set(px, 0, if h.is_multiple_of(3) { lime } else { leaf });
        canvas.set(px, 1, if h.is_multiple_of(5) { lime } else { leaf });
        canvas.set(px, 2, moss);
        if h.is_multiple_of(3) {
            canvas.set(px, 3, moss);
        }
        if h.is_multiple_of(7) {
            canvas.set(px, 4, moss);
        }
    }
}

/// Tufts of grass standing on the ground in the sky row above it.
fn paint_grass_blades(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    down: Cell,
) {
    let below = terrain.tile(TilePosition {
        x: position.x,
        y: position.y + 1,
    });
    if down != Cell::Solid || below.is_some_and(|tile| tile.kind == TileKind::Foundation) {
        return;
    }
    let (lime, leaf, moss) = (index_of(LIME), index_of(LEAF), index_of(MOSS));
    for px in 0..TILE_ART {
        let h = hash(canvas.origin_x + px, 4, 43);
        match h % 9 {
            0 => {
                canvas.set(px, TILE_ART - 1, leaf);
                canvas.set(px, TILE_ART - 2, lime);
            }
            1 | 2 => canvas.set(px, TILE_ART - 1, leaf),
            3 => {
                canvas.set(px, TILE_ART - 1, moss);
                canvas.set(px, TILE_ART - 2, leaf);
                canvas.set(px, TILE_ART - 3, leaf);
            }
            _ => {}
        }
    }
}

/// Air inside a vault: tiled panels instead of rock, with the same contact shadows.
fn paint_vault_room(canvas: &mut TileCanvas, strata: &Strata, up: Cell, left: Cell, right: Cell) {
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
                poi_art::vault_floor_pixel(strata, gx, gy)
            };
            canvas.set(px, py, index);
        }
    }
}

/// Air underground: the back wall with contact shadows under ceilings and along walls.
fn paint_cave(canvas: &mut TileCanvas, strata: &Strata, up: Cell, left: Cell, right: Cell) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let mut index = backdrop_pixel(strata, gx, gy);
            let shadowed = (up == Cell::Solid && (py < 2 || (py < 4 && bayer4(gx, gy) < 6)))
                || (left == Cell::Solid && (px == 0 || (px == 1 && bayer4(gx, gy) < 8)))
                || (right == Cell::Solid
                    && (px == TILE_ART - 1 || (px == TILE_ART - 2 && bayer4(gx, gy) < 8)));
            if shadowed {
                index = strata.back[1];
            }
            canvas.set(px, py, index);
        }
    }
}

#[derive(Clone, Copy)]
struct Edges {
    up: Cell,
    down: Cell,
    left: Cell,
    right: Cell,
}

/// Outlines the faces that meet open air, lights the top/left lips, shades the bottom/right and
/// rounds corners where two faces meet.
fn paint_edges(
    canvas: &mut TileCanvas,
    strata: &Strata,
    edges: Edges,
    lip: PaletteIndex,
    shade: PaletteIndex,
    square: bool,
) {
    let ink = index_of(INK);
    let last = TILE_ART - 1;
    let sky_up = edges.up == Cell::Sky;
    if edges.up == Cell::Cave {
        for px in 0..TILE_ART {
            canvas.set(px, 0, ink);
            canvas.set(px, 1, lip);
        }
    }
    if edges.down.is_open() {
        for px in 0..TILE_ART {
            canvas.set(px, last, ink);
            if canvas.get(px, last - 1) != ink {
                canvas.set(px, last - 1, shade);
            }
        }
    }
    if edges.left.is_open() {
        for py in 0..TILE_ART {
            canvas.set(0, py, ink);
            if py > 0 && py < last && !(sky_up && py < 3) {
                canvas.set(1, py, lip);
            }
        }
    }
    if edges.right.is_open() {
        for py in 0..TILE_ART {
            canvas.set(last, py, ink);
            if py > 0 && py < last && !(sky_up && py < 3) {
                canvas.set(last - 1, py, shade);
            }
        }
    }
    if edges.up == Cell::Cave && edges.left.is_open() {
        canvas.set(1, 1, lip);
    }
    if square {
        return;
    }
    // Rounded corners: the outer pixels become whatever is beyond the corner.
    let open_pixel = |canvas: &TileCanvas, cell: Cell, x: i32, y: i32| match cell {
        Cell::Sky => TRANSPARENT,
        _ => backdrop_pixel(strata, canvas.origin_x + x, canvas.origin_y + y),
    };
    let corners = [
        (edges.up, edges.left, (0, 0), (1, 0), (0, 1), (1, 1)),
        (
            edges.up,
            edges.right,
            (last, 0),
            (last - 1, 0),
            (last, 1),
            (last - 1, 1),
        ),
        (
            edges.down,
            edges.left,
            (0, last),
            (1, last),
            (0, last - 1),
            (1, last - 1),
        ),
        (
            edges.down,
            edges.right,
            (last, last),
            (last - 1, last),
            (last, last - 1),
            (last - 1, last - 1),
        ),
    ];
    for (vertical, horizontal, corner, beside, below, inner) in corners {
        if !(vertical.is_open() && horizontal.is_open()) {
            continue;
        }
        let beyond = if vertical == Cell::Sky {
            Cell::Sky
        } else {
            horizontal
        };
        for (x, y) in [corner, beside, below] {
            let index = open_pixel(canvas, beyond, x, y);
            canvas.set(x, y, index);
        }
        canvas.set(inner.0, inner.1, ink);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::pixel_art::shade::color_at;

    #[test]
    fn every_band_uses_palette_indices() {
        for band in Band::ALL {
            let strata = strata_for(band);
            for index in strata
                .dirt
                .iter()
                .chain(&strata.clay)
                .chain(&strata.stone)
                .chain(&strata.hard)
                .chain(&strata.back)
            {
                assert!(usize::from(*index) < PALETTE.len());
            }
        }
    }

    #[test]
    fn tiles_paint_only_palette_colours_and_sky_stays_clear() {
        let terrain = Terrain::new_seeded(64, 120, 7);
        let mut buffer = vec![0; 16 * 16];
        for y in 0..120 {
            for x in 0..64 {
                let position = TilePosition { x, y };
                paint_tile(&terrain, position, &mut buffer, 16, (0, 0));
                for index in &buffer {
                    assert!(
                        *index == TRANSPARENT || usize::from(*index) < PALETTE.len(),
                        "{position:?}"
                    );
                }
                let kind = terrain.tile(position).map(|tile| tile.kind);
                if y < SKY_MAX_TILE_Y && kind == Some(TileKind::Air) {
                    assert!(buffer.iter().all(|index| *index == TRANSPARENT));
                }
                if y > SKY_MAX_TILE_Y {
                    assert!(
                        buffer.iter().all(|index| *index != TRANSPARENT),
                        "{position:?} has holes"
                    );
                }
            }
        }
        assert_eq!(color_at(TRANSPARENT).a, 0);
    }

    #[test]
    fn dug_faces_get_an_ink_outline() {
        let mut terrain = Terrain::new_seeded(16, 40, 3);
        let tunnel = TilePosition { x: 8, y: 20 };
        terrain.set_kind(tunnel, TileKind::Air);
        terrain.set_kind(TilePosition { x: 8, y: 19 }, TileKind::Stone);
        let mut buffer = vec![0; 16 * 16];
        paint_tile(
            &terrain,
            TilePosition { x: 8, y: 19 },
            &mut buffer,
            16,
            (0, 0),
        );
        // The bottom row of the tile above the tunnel is the outline (corners aside).
        assert!(
            buffer[15 * 16 + 2..15 * 16 + 14]
                .iter()
                .all(|index| *index == index_of(INK))
        );
    }
}
