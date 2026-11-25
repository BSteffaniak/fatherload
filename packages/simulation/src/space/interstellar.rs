//! Between the stars: nebulae (their own coarse grid), wormholes (paired across regions) and
//! black holes (in a sector's free cells).

use super::{
    BlackHole, CELL_TILES, HOME_SECTOR, NEBULA_CELL_SECTORS, Nebula, SPACE_SECTOR_TILES,
    SYSTEM_MAX_EXTENT, SpaceSector, SpaceTile, WORMHOLE_REGION_SECTORS, WormholeId, WormholeMouth,
    generate::cell_layout,
};
use crate::worldgen::noise::{RegionRng, hash2};

const SALT_NEBULA: u64 = 0x5BAC_EA00;
const SALT_NEBULA_ID: u64 = 0x5BAC_EA01;
const SALT_WORM_FLIP: u64 = 0x5BAC_EB00;
const SALT_WORM_PAIR: u64 = 0x5BAC_EB01;
const SALT_WORM_SECTOR: u64 = 0x5BAC_EB02;
const SALT_WORM_PLACE: u64 = 0x5BAC_EB03;
const SALT_BLACK_HOLE: u64 = 0x5BAC_EC00;

/// Nebula colours (presentation; `[r, g, b]`).
const NEBULA_COLOURS: [[u8; 3]; 6] = [
    [190, 110, 255],
    [255, 110, 170],
    [100, 190, 255],
    [120, 255, 190],
    [255, 170, 90],
    [230, 90, 110],
];

/// Per mille of nebula cells holding a nebula.
const NEBULA_CHANCE: u32 = 300;
/// Per mille of wormhole pairs that exist.
const WORMHOLE_PAIR_CHANCE: u64 = 450;
/// Per mille of sectors (with a free cell, far enough from home) holding a black hole.
const BLACK_HOLE_CHANCE: u32 = 70;
/// No black hole within this many sectors of home (Chebyshev).
const BLACK_HOLE_HOME_CLEARANCE: i64 = 3;

// ---------------------------------------------------------------------------------------------
// Nebulae.
// ---------------------------------------------------------------------------------------------

/// The nebula of nebula cell `(cx, cy)`, if it has one.
pub(super) fn nebula_in_cell(seed: u64, cx: i32, cy: i32) -> Option<Nebula> {
    let mut rng = RegionRng::new(seed, SALT_NEBULA, cx, cy);
    if !rng.chance(NEBULA_CHANCE) {
        return None;
    }
    let span = i64::from(NEBULA_CELL_SECTORS) * SPACE_SECTOR_TILES;
    let origin = SpaceTile::new(i64::from(cx) * span, i64::from(cy) * span);
    let centre = origin.offset(
        (rng.next_u64() % span as u64) as i64,
        (rng.next_u64() % span as u64) as i64,
    );
    let radius = SPACE_SECTOR_TILES * i64::from(rng.range(800, 2_400)) / 1_000;
    // Home stays clear.
    if HOME_SECTOR
        .centre()
        .within(centre, radius + SYSTEM_MAX_EXTENT)
    {
        return None;
    }
    Some(Nebula {
        id: hash2(seed, SALT_NEBULA_ID, cx, cy) | 1,
        centre,
        radius,
        colour: NEBULA_COLOURS[rng.range(0, NEBULA_COLOURS.len() as i32 - 1) as usize],
        fog: rng.range(250, 700) as u16,
    })
}

/// Every interstellar nebula overlapping `sector`'s square.
pub(super) fn nebulae_overlapping(seed: u64, sector: SpaceSector) -> Vec<Nebula> {
    let cx = sector.x.div_euclid(NEBULA_CELL_SECTORS);
    let cy = sector.y.div_euclid(NEBULA_CELL_SECTORS);
    let mut out = Vec::new();
    for y in cy - 1..=cy + 1 {
        for x in cx - 1..=cx + 1 {
            if let Some(nebula) = nebula_in_cell(seed, x, y) {
                let r = i128::from(nebula.radius);
                if sector.distance2_to(nebula.centre) < r * r {
                    out.push(nebula);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Wormholes.
// ---------------------------------------------------------------------------------------------

/// The region bits a pair flips (bits 2-4: partners lie 4, 8 or 16 regions apart per axis).
const FLIP_BITS: i32 = 0b1_1100;
const FLIPS: [i32; 4] = [0, 4, 8, 16];

/// The wormhole region holding `sector`.
pub(super) const fn region_of(sector: SpaceSector) -> (i32, i32) {
    (
        sector.x.div_euclid(WORMHOLE_REGION_SECTORS),
        sector.y.div_euclid(WORMHOLE_REGION_SECTORS),
    )
}

/// The region paired with `region`. An involution: `partner(partner(r)) == r`, never `r`,
/// because the flips are drawn from the bits they leave unchanged.
pub(super) const fn partner(seed: u64, region: (i32, i32)) -> (i32, i32) {
    let key = (region.0 & !FLIP_BITS, region.1 & !FLIP_BITS);
    let roll = hash2(seed, SALT_WORM_FLIP, key.0, key.1);
    let mut fx = FLIPS[(roll & 3) as usize];
    let fy = FLIPS[((roll >> 2) & 3) as usize];
    if fx == 0 && fy == 0 {
        fx = 8;
    }
    (region.0 ^ fx, region.1 ^ fy)
}

/// The pair `region` belongs to and whether it exists.
fn pair_of(seed: u64, region: (i32, i32)) -> Option<WormholeId> {
    let other = partner(seed, region);
    let (a, b) = if region <= other {
        (region, other)
    } else {
        (other, region)
    };
    (hash2(seed, SALT_WORM_PAIR, a.0, a.1) % 1_000 < WORMHOLE_PAIR_CHANCE)
        .then_some(WormholeId { a, b })
}

/// The sector of `region`'s mouth (never the home sector).
pub(super) fn mouth_sector(seed: u64, region: (i32, i32)) -> SpaceSector {
    let roll = hash2(seed, SALT_WORM_SECTOR, region.0, region.1);
    let size = WORMHOLE_REGION_SECTORS;
    let sector = SpaceSector::new(
        region.0 * size + (roll % size as u64) as i32,
        region.1 * size + ((roll >> 8) % size as u64) as i32,
    );
    if sector == HOME_SECTOR {
        SpaceSector::new(sector.x + 3, sector.y)
    } else {
        sector
    }
}

/// The centre and radius of `region`'s mouth: in the first free cell of its sector.
fn mouth_place(seed: u64, region: (i32, i32)) -> (SpaceSector, SpaceTile, u32) {
    let sector = mouth_sector(seed, region);
    let layout = cell_layout(seed, sector);
    let cell = layout.free_cells().first().copied().unwrap_or(0);
    let mut rng = RegionRng::new(seed, SALT_WORM_PLACE, region.0, region.1);
    let reach = (CELL_TILES / 2 - 100_000) as i32;
    let centre = sector.cell_centre(cell).offset(
        i64::from(rng.range(-reach, reach)),
        i64::from(rng.range(-reach, reach)),
    );
    (sector, centre, rng.range(300, 600) as u32)
}

/// The wormhole mouth in `sector`, if one opens there.
pub(super) fn wormhole_in(seed: u64, sector: SpaceSector) -> Option<WormholeMouth> {
    let region = region_of(sector);
    if mouth_sector(seed, region) != sector {
        return None;
    }
    let id = pair_of(seed, region)?;
    let (_, centre, radius) = mouth_place(seed, region);
    let (exit_sector, exit, _) = mouth_place(seed, partner(seed, region));
    Some(WormholeMouth {
        id,
        centre,
        radius,
        exit,
        exit_sector,
    })
}

// ---------------------------------------------------------------------------------------------
// Black holes.
// ---------------------------------------------------------------------------------------------

/// A black hole in one of `free` cells of `sector`, and which cell.
pub(super) fn black_hole_in(
    seed: u64,
    sector: SpaceSector,
    free: &[u8],
) -> Option<(u8, BlackHole)> {
    if free.is_empty() || sector.chebyshev(HOME_SECTOR) < BLACK_HOLE_HOME_CLEARANCE {
        return None;
    }
    let mut rng = RegionRng::new(seed, SALT_BLACK_HOLE, sector.x, sector.y);
    if !rng.chance(BLACK_HOLE_CHANCE) {
        return None;
    }
    let cell = free[rng.range(0, free.len() as i32 - 1) as usize];
    let horizon = rng.range(600, 1_800) as u32;
    let lensing_radius = horizon * rng.range(8, 14) as u32;
    let reach = (CELL_TILES / 2 - i64::from(lensing_radius) - 40_000) as i32;
    let centre = sector.cell_centre(cell).offset(
        i64::from(rng.range(-reach, reach)),
        i64::from(rng.range(-reach, reach)),
    );
    Some((
        cell,
        BlackHole {
            id: rng.next_u64() | 1,
            centre,
            horizon,
            lensing_radius,
            time_dilation: rng.range(150, 400) as u16,
            gravity_milli_g: rng.range(30_000, 60_000) as u32,
        },
    ))
}
