//! Hazards: clustered pockets placed by band rules and scaled by the hazard intensity.
//!
//! Hazards always come in clusters of at least [`MIN_CLUSTER_TILES`] tiles (never an isolated
//! random tile), and never near spawn or under the town. The renderer marks the rock around a
//! pocket with a tell-tale stain (see the game's tile art), so a careful player can read danger
//! before drilling into it.

use crate::terrain::{DeepStratum, TilePosition};

use super::{
    Canvas, SPAWN_TILE_X,
    features::{FeatureDetail, FeatureKind, HazardKind, TileRect},
    noise::RegionRng,
    strata::Band,
    town_columns,
};

/// Smallest hazard cluster.
pub const MIN_CLUSTER_TILES: usize = 3;
/// Shallowest row a hazard may sit in.
pub const HAZARD_MIN_ROW: i32 = 24;
/// Shallowest hazard row under the town (and a few columns either side).
pub const TOWN_HAZARD_MIN_ROW: i32 = 32;
/// No hazard lies within this many tiles (Chebyshev) of the spawn point.
pub const SPAWN_HAZARD_CLEARANCE: i32 = 22;

const SALT_CLUSTER: u64 = 0x4A2A_0001;
const REGION: (i32, i32) = (24, 16);

/// Whether a hazard may be placed at `(x, y)`.
#[must_use]
pub const fn hazard_allowed(x: i32, y: i32) -> bool {
    if y < HAZARD_MIN_ROW {
        return false;
    }
    let (town_first, town_last) = town_columns();
    if y < TOWN_HAZARD_MIN_ROW && x >= town_first - 6 && x <= town_last + 6 {
        return false;
    }
    let dx = (x - SPAWN_TILE_X).abs();
    let dy = (y - super::strata::SKY_ROWS).abs();
    !(dx <= SPAWN_HAZARD_CLEARANCE && dy <= SPAWN_HAZARD_CLEARANCE)
}

/// Clusters per 10 000 tiles of a band, and the relative weight of each hazard family
/// (in [`HazardKind::ALL`] order: gas, lava, magma vent, pressure, explosive).
const fn band_rules(band: Band) -> (u32, [u32; 5]) {
    match band {
        Band::Topsoil => (0, [0; 5]),
        Band::Clay => (8, [1, 0, 0, 0, 0]),
        Band::Stone => (16, [5, 0, 0, 2, 3]),
        Band::Slate => (22, [4, 2, 1, 2, 3]),
        Band::Basalt => (26, [2, 4, 3, 2, 2]),
        Band::Deep(stratum) => {
            let mut weights = [1, 2, 1, 1, 1];
            let primary = match stratum {
                DeepStratum::FossilOceans => 0,
                DeepStratum::MantleStormZone => 1,
                DeepStratum::RadioactiveHollow => 2,
                DeepStratum::PressureCathedrals | DeepStratum::VoidGeodeFields => 3,
                DeepStratum::CrystalFaults | DeepStratum::AncientMachineLayer => 4,
            };
            weights[primary] = 8;
            (30, weights)
        }
    }
}

/// Depth gates per family (the shallowest row each may appear in).
const fn family_min_row(kind: HazardKind) -> i32 {
    match kind {
        HazardKind::Gas => HAZARD_MIN_ROW,
        HazardKind::Explosive => 29,
        HazardKind::Pressure => 35,
        HazardKind::Lava => 48,
        HazardKind::MagmaVent => 55,
    }
}

/// Places every hazard cluster.
pub(crate) fn place(canvas: &mut Canvas<'_>) {
    let seed = canvas.seed();
    let intensity = canvas.settings.hazards.percent();
    let regions_x = (canvas.width + REGION.0 - 1) / REGION.0;
    let regions_y = (canvas.height + REGION.1 - 1) / REGION.1;
    for ry in 0..regions_y {
        for rx in 0..regions_x {
            let mut rng = RegionRng::new(seed, SALT_CLUSTER, rx, ry);
            let origin = (rx * REGION.0, ry * REGION.1);
            let band = canvas.band(origin.0 + REGION.0 / 2, origin.1 + REGION.1 / 2);
            let (density, _) = band_rules(band);
            // Expected clusters in this region, in thousandths.
            let area = u32::try_from(REGION.0 * REGION.1).unwrap_or_default();
            let mut expected = density * area * intensity / 1000;
            while expected > 0 {
                if expected >= 1000 || rng.chance(expected) {
                    let x = origin.0 + rng.range(0, REGION.0 - 1);
                    let y = origin.1 + rng.range(0, REGION.1 - 1);
                    place_cluster(canvas, &mut rng, x, y);
                }
                expected = expected.saturating_sub(1000);
            }
        }
    }
}

fn place_cluster(canvas: &mut Canvas<'_>, rng: &mut RegionRng, x: i32, y: i32) {
    if !hazard_allowed(x, y) || !canvas.is_plain_rock(x, y) {
        return;
    }
    let band = canvas.band(x, y);
    let (_, mut weights) = band_rules(band);
    for (weight, kind) in weights.iter_mut().zip(HazardKind::ALL) {
        if y < family_min_row(kind) {
            *weight = 0;
        }
    }
    let Some(family) = rng.pick(&weights) else {
        return;
    };
    let kind = HazardKind::ALL[family];
    let target = match kind {
        HazardKind::Lava => rng.range(5, 12),
        HazardKind::Gas => rng.range(4, 9),
        HazardKind::MagmaVent => rng.range(3, 5),
        HazardKind::Pressure | HazardKind::Explosive => rng.range(3, 7),
    };
    let target = usize::try_from(target).unwrap_or(MIN_CLUSTER_TILES);
    // Grow a blob from the centre into neighbouring plain rock.
    let mut tiles = vec![(x, y)];
    let mut attempts = 0;
    while tiles.len() < target && attempts < target * 8 {
        attempts += 1;
        let from =
            tiles[usize::try_from(rng.range(0, i32::try_from(tiles.len()).unwrap_or(1) - 1))
                .unwrap_or_default()];
        // Lava pools spread sideways, vents climb, others grow evenly.
        let step = match kind {
            HazardKind::Lava => [(1, 0), (-1, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]
                [usize::try_from(rng.range(0, 5)).unwrap_or_default()],
            HazardKind::MagmaVent => [(0, -1), (0, 1), (0, -1), (1, 0), (-1, 0)]
                [usize::try_from(rng.range(0, 4)).unwrap_or_default()],
            _ => [(1, 0), (-1, 0), (0, 1), (0, -1)]
                [usize::try_from(rng.range(0, 3)).unwrap_or_default()],
        };
        let next = (from.0 + step.0, from.1 + step.1);
        if !tiles.contains(&next)
            && hazard_allowed(next.0, next.1)
            && canvas.is_plain_rock(next.0, next.1)
        {
            tiles.push(next);
        }
    }
    if tiles.len() < MIN_CLUSTER_TILES {
        return;
    }
    let tile = kind.tile();
    let (mut min, mut max) = ((x, y), (x, y));
    for (tx, ty) in &tiles {
        canvas.set(*tx, *ty, tile);
        min = (min.0.min(*tx), min.1.min(*ty));
        max = (max.0.max(*tx), max.1.max(*ty));
    }
    canvas.features.push(
        FeatureKind::HazardCluster,
        FeatureDetail::Hazard(kind),
        band,
        TilePosition { x, y },
        TileRect {
            min: TilePosition { x: min.0, y: min.1 },
            max: TilePosition { x: max.0, y: max.1 },
        },
        u32::try_from(tiles.len()).unwrap_or(u32::MAX),
    );
}
