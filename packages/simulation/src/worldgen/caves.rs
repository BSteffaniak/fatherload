//! Caves: smoothed noise caverns, worm tunnels and big deep chambers.
//!
//! Caves never break the surface crust, never open under the town's foundations and never open
//! in the shaft below the spawn point. Large connected cave systems and chambers are recorded in
//! the feature index.

use std::collections::VecDeque;

use crate::terrain::{DeepStratum, TileKind, TilePosition};

use super::{
    Canvas, SPAWN_TILE_X,
    features::{FeatureDetail, FeatureKind, TileRect},
    noise::{RegionRng, fractal_noise, value_noise},
    strata::Band,
    town_columns,
};

/// Shallowest row any cave may open in.
pub const CRUST_ROWS: i32 = 12;
/// Shallowest cave row under the town.
pub const TOWN_CRUST_ROWS: i32 = 17;
/// Half-width of the cave-free shaft under spawn, and how deep it reaches.
pub const SPAWN_SHAFT_HALF_WIDTH: i32 = 10;
pub const SPAWN_SHAFT_ROWS: i32 = 28;
/// Smallest connected open area recorded as a [`FeatureKind::LargeCavern`].
pub const LARGE_CAVERN_TILES: usize = 90;

const SALT_CAVERN: u64 = 0xCA7E_0001;
const SALT_CAVERN_DETAIL: u64 = 0xCA7E_0002;
const SALT_WORM: u64 = 0xCA7E_0100;
const SALT_CHAMBER: u64 = 0xCA7E_0200;
const SALT_CHAMBER_EDGE: u64 = 0xCA7E_0201;

/// Whether caves may open at `(x, y)`.
#[must_use]
pub const fn cave_allowed(x: i32, y: i32) -> bool {
    if y < CRUST_ROWS {
        return false;
    }
    let (town_first, town_last) = town_columns();
    if y < TOWN_CRUST_ROWS && x >= town_first - 3 && x <= town_last + 3 {
        return false;
    }
    !(y < SPAWN_SHAFT_ROWS && (x - SPAWN_TILE_X).abs() <= SPAWN_SHAFT_HALF_WIDTH)
}

/// Share (per mille) of a band that the noise caverns open.
const fn cavern_share(band: Band) -> u32 {
    match band {
        Band::Topsoil => 20,
        Band::Clay => 40,
        Band::Stone => 60,
        Band::Slate => 75,
        Band::Basalt => 85,
        Band::Deep(DeepStratum::CrystalFaults | DeepStratum::RadioactiveHollow) => 100,
        Band::Deep(DeepStratum::FossilOceans | DeepStratum::VoidGeodeFields) => 125,
        Band::Deep(DeepStratum::PressureCathedrals) => 115,
        Band::Deep(DeepStratum::AncientMachineLayer) => 80,
        Band::Deep(DeepStratum::MantleStormZone) => 105,
    }
}

/// Cavern noise cell size per band: wide flat galleries shallow, taller voids deep. Deep (V4)
/// worlds stretch every cell by a quarter, so their thicker bands get roomier cave systems
/// rather than just more of the same small pockets.
const fn cavern_cells(band: Band, deep_world: bool) -> (i32, i32) {
    let (cell_x, cell_y) = classic_cavern_cells(band);
    if deep_world {
        (cell_x * 5 / 4, cell_y * 5 / 4)
    } else {
        (cell_x, cell_y)
    }
}

const fn classic_cavern_cells(band: Band) -> (i32, i32) {
    match band {
        Band::Topsoil | Band::Clay => (14, 6),
        Band::Stone | Band::Slate => (18, 8),
        Band::Basalt => (16, 9),
        Band::Deep(DeepStratum::FossilOceans) => (26, 8),
        Band::Deep(DeepStratum::PressureCathedrals) => (12, 16),
        Band::Deep(DeepStratum::AncientMachineLayer) => (22, 6),
        Band::Deep(_) => (16, 11),
    }
}

/// Carves every cave into `canvas` and indexes the big ones.
pub(crate) fn carve(canvas: &mut Canvas<'_>) {
    let mut open = cavern_mask(canvas);
    carve_worms(canvas, &mut open);
    carve_chambers(canvas, &mut open);
    for y in 0..canvas.height {
        for x in 0..canvas.width {
            if let Some(index) = canvas.index(x, y)
                && open[index]
                && cave_allowed(x, y)
                && !canvas.is_fixed(x, y)
            {
                canvas.kinds[index] = TileKind::Air;
            }
        }
    }
    index_caverns(canvas);
}

/// Thresholded fractal noise smoothed by a few cellular-automaton passes.
fn cavern_mask(canvas: &Canvas<'_>) -> Vec<bool> {
    let seed = canvas.seed();
    let deep_world = canvas.settings.generator.is_deep();
    let mut open = vec![false; canvas.kinds.len()];
    for y in CRUST_ROWS..canvas.height {
        for x in 0..canvas.width {
            let band = canvas.band(x, y);
            let (cell_x, cell_y) = cavern_cells(band, deep_world);
            let base = fractal_noise(seed, SALT_CAVERN, x, y, cell_x, cell_y, 3);
            // A little fine detail so edges are not perfectly smooth blobs.
            let detail = value_noise(seed, SALT_CAVERN_DETAIL, x, y, 3, 3);
            let value = base + (detail - 32_768) / 10;
            if let Some(index) = canvas.index(x, y) {
                open[index] = value > super::contrast_threshold(cavern_share(band));
            }
        }
    }
    for _ in 0..2 {
        open = smooth(canvas, &open);
    }
    open
}

/// One cellular-automaton pass: a cell opens with 5+ open neighbours and stays open with 4+.
fn smooth(canvas: &Canvas<'_>, open: &[bool]) -> Vec<bool> {
    let mut next = open.to_vec();
    for y in CRUST_ROWS..canvas.height {
        for x in 0..canvas.width {
            let mut neighbours = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx, dy) != (0, 0)
                        && canvas
                            .index(x + dx, y + dy)
                            .is_some_and(|index| open[index])
                    {
                        neighbours += 1;
                    }
                }
            }
            if let Some(index) = canvas.index(x, y) {
                next[index] = if open[index] {
                    neighbours >= 4
                } else {
                    neighbours >= 5
                };
            }
        }
    }
    next
}

/// Unit vectors (8.8 fixed point) for 16 headings, starting east and turning clockwise
/// (screen y grows downwards).
const HEADINGS: [(i32, i32); 16] = [
    (256, 0),
    (237, 98),
    (181, 181),
    (98, 237),
    (0, 256),
    (-98, 237),
    (-181, 181),
    (-237, 98),
    (-256, 0),
    (-237, -98),
    (-181, -181),
    (-98, -237),
    (0, -256),
    (98, -237),
    (181, -181),
    (237, -98),
];

/// Region size of worm placement.
const WORM_REGION: (i32, i32) = (40, 26);

/// Worm tunnels: a few per region, wandering mostly sideways.
fn carve_worms(canvas: &Canvas<'_>, open: &mut [bool]) {
    let seed = canvas.seed();
    let regions_x = (canvas.width + WORM_REGION.0 - 1) / WORM_REGION.0;
    let regions_y = (canvas.height + WORM_REGION.1 - 1) / WORM_REGION.1;
    for ry in 0..regions_y {
        for rx in 0..regions_x {
            let mut rng = RegionRng::new(seed, SALT_WORM, rx, ry);
            let center_band = canvas.band(
                rx * WORM_REGION.0 + WORM_REGION.0 / 2,
                ry * WORM_REGION.1 + WORM_REGION.1 / 2,
            );
            let (single, double) = match center_band {
                Band::Topsoil => (0, 0),
                Band::Clay => (250, 0),
                Band::Stone | Band::Slate => (380, 0),
                Band::Basalt => (420, 40),
                Band::Deep(_) => (450, 80),
            };
            let count = u32::from(rng.chance(single)) + u32::from(rng.chance(double));
            for _ in 0..count {
                let start_x = rx * WORM_REGION.0 + rng.range(0, WORM_REGION.0 - 1);
                let start_y = (ry * WORM_REGION.1 + rng.range(0, WORM_REGION.1 - 1))
                    .max(super::caves::CRUST_ROWS + 2);
                let deep = matches!(center_band, Band::Deep(_) | Band::Basalt);
                worm(canvas, open, &mut rng, (start_x, start_y), deep);
            }
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "heading indices are bounded by the table length"
)]
fn worm(
    canvas: &Canvas<'_>,
    open: &mut [bool],
    rng: &mut RegionRng,
    start: (i32, i32),
    deep: bool,
) {
    // Mostly sideways headings; occasionally steep.
    const SIDEWAYS: [usize; 6] = [0, 1, 15, 7, 8, 9];
    let mut heading = if rng.chance(750) {
        SIDEWAYS[rng.range(0, 5) as usize]
    } else {
        rng.range(0, 15) as usize
    };
    let (mut x, mut y) = (start.0 * 256 + 128, start.1 * 256 + 128);
    let length = rng.range(24, if deep { 70 } else { 56 });
    let base_radius = 1;
    let mut bulge = 0;
    for _ in 0..length {
        if deep && bulge == 0 && rng.chance(20) {
            bulge = rng.range(3, 7);
        }
        let radius = base_radius + i32::from(bulge > 0);
        bulge = (bulge - 1).max(0);
        carve_disc(canvas, open, x >> 8, y >> 8, radius);
        if rng.chance(230) {
            heading = if rng.chance(500) {
                (heading + 1) % 16
            } else {
                (heading + 15) % 16
            };
        }
        // Steer back towards horizontal when a worm dives too steeply.
        if matches!(heading, 4 | 12) && rng.chance(300) {
            heading = if rng.chance(500) { 3 } else { 13 };
        }
        let (dx, dy) = HEADINGS[heading];
        x += dx;
        y += dy;
        if (y >> 8) < CRUST_ROWS + 1 {
            heading = if dx >= 0 { 1 } else { 7 };
        }
        if (x >> 8) < 0 || (x >> 8) >= canvas.width || (y >> 8) >= canvas.height - 1 {
            break;
        }
    }
}

fn carve_disc(canvas: &Canvas<'_>, open: &mut [bool], cx: i32, cy: i32, radius: i32) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= radius * radius + radius / 2
                && let Some(index) = canvas.index(cx + dx, cy + dy)
            {
                open[index] = true;
            }
        }
    }
}

/// Region size of chamber placement.
const CHAMBER_REGION: (i32, i32) = (56, 34);

/// Big elliptical chambers with ragged walls, deep strata only.
fn carve_chambers(canvas: &mut Canvas<'_>, open: &mut [bool]) {
    let seed = canvas.seed();
    let regions_x = (canvas.width + CHAMBER_REGION.0 - 1) / CHAMBER_REGION.0;
    let regions_y = (canvas.height + CHAMBER_REGION.1 - 1) / CHAMBER_REGION.1;
    let mut placed = Vec::new();
    for ry in 0..regions_y {
        for rx in 0..regions_x {
            let mut rng = RegionRng::new(seed, SALT_CHAMBER, rx, ry);
            let cx = rx * CHAMBER_REGION.0 + rng.range(10, CHAMBER_REGION.0 - 10);
            let cy = ry * CHAMBER_REGION.1 + rng.range(6, CHAMBER_REGION.1 - 6);
            if cx >= canvas.width - 8 || cx < 0 || cy >= canvas.floor_row(cx) - 6 {
                continue;
            }
            let band = canvas.band(cx, cy);
            let Band::Deep(stratum) = band else {
                continue;
            };
            let per_mille = match stratum {
                DeepStratum::PressureCathedrals => 700,
                DeepStratum::VoidGeodeFields | DeepStratum::FossilOceans => 450,
                DeepStratum::AncientMachineLayer => 250,
                _ => 330,
            };
            if !rng.chance(per_mille) {
                continue;
            }
            let (half_width, half_height) = match stratum {
                DeepStratum::PressureCathedrals => (rng.range(6, 10), rng.range(6, 9)),
                DeepStratum::FossilOceans => (rng.range(10, 15), rng.range(4, 6)),
                _ => (rng.range(6, 12), rng.range(4, 7)),
            };
            let mut size = 0_u32;
            for dy in -half_height - 1..=half_height + 1 {
                for dx in -half_width - 1..=half_width + 1 {
                    let (x, y) = (cx + dx, cy + dy);
                    // Inside the ellipse, with a noisy margin of about one tile.
                    let r2 =
                        i64::from(half_width * half_width) * i64::from(half_height * half_height);
                    let d = i64::from(dx * dx) * i64::from(half_height * half_height)
                        + i64::from(dy * dy) * i64::from(half_width * half_width);
                    let edge = i64::from(value_noise(seed, SALT_CHAMBER_EDGE, x, y, 3, 3));
                    let limit = r2 * (49_152 + edge / 2) / 65_536;
                    if d <= limit
                        && let Some(index) = canvas.index(x, y)
                    {
                        open[index] = true;
                        size += 1;
                    }
                }
            }
            placed.push((cx, cy, half_width, half_height, band, size));
        }
    }
    for (cx, cy, half_width, half_height, band, size) in placed {
        let center = TilePosition { x: cx, y: cy };
        canvas.features.push(
            FeatureKind::Chamber,
            FeatureDetail::None,
            band,
            center,
            TileRect::around(center, half_width + 1, half_height + 1),
            size,
        );
    }
}

/// Records every large connected open area below the crust.
fn index_caverns(canvas: &mut Canvas<'_>) {
    let mut seen = vec![false; canvas.kinds.len()];
    let mut found = Vec::new();
    for y in CRUST_ROWS..canvas.height {
        for x in 0..canvas.width {
            let Some(start) = canvas.index(x, y) else {
                continue;
            };
            if seen[start] || canvas.kinds[start] != TileKind::Air {
                continue;
            }
            seen[start] = true;
            let mut queue = VecDeque::from([(x, y)]);
            let (mut min, mut max) = ((x, y), (x, y));
            let (mut sum_x, mut sum_y, mut count) = (0_i64, 0_i64, 0_i64);
            while let Some((cx, cy)) = queue.pop_front() {
                count += 1;
                sum_x += i64::from(cx);
                sum_y += i64::from(cy);
                min = (min.0.min(cx), min.1.min(cy));
                max = (max.0.max(cx), max.1.max(cy));
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (cx + dx, cy + dy);
                    if ny < CRUST_ROWS {
                        continue;
                    }
                    if let Some(next) = canvas.index(nx, ny)
                        && !seen[next]
                        && canvas.kinds[next] == TileKind::Air
                    {
                        seen[next] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }
            if usize::try_from(count).unwrap_or_default() >= LARGE_CAVERN_TILES {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "averages of in-bounds coordinates fit in i32"
                )]
                let centroid = TilePosition {
                    x: (sum_x / count) as i32,
                    y: (sum_y / count) as i32,
                };
                found.push((
                    centroid,
                    TileRect {
                        min: TilePosition { x: min.0, y: min.1 },
                        max: TilePosition { x: max.0, y: max.1 },
                    },
                    u32::try_from(count).unwrap_or(u32::MAX),
                ));
            }
        }
    }
    for (center, bounds, size) in found {
        let band = canvas.band(center.x, center.y);
        canvas.features.push(
            FeatureKind::LargeCavern,
            FeatureDetail::None,
            band,
            center,
            bounds,
            size,
        );
    }
}
