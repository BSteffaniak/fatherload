//! Deposits: ore and artifacts.
//!
//! Every mineral has a depth curve (where it appears and where it peaks), sharpened by the deep
//! stratum it sits in. Ore comes in three styles: veins that follow noise ridges, region-placed
//! clusters, and a few rare motherlodes. Artifacts lie in small beds (one or two per band) and as
//! rare strays, always of a kind that fits their stratum. After placement every band is topped
//! up to its ore budget and a starter field near spawn is guaranteed, so no world is starved.

use crate::terrain::{ArtifactKind, DeepStratum, MineralKind, TileKind, TilePosition};

use super::{
    Canvas, SPAWN_TILE_X,
    features::{FeatureDetail, FeatureKind, TileRect},
    noise::{NOISE_MAX, RegionRng, centred, chance, fractal_noise, hash2, ridge, value_noise},
    strata::{Band, SKY_ROWS, nominal_band_top},
};

const SALT_VEIN: u64 = 0x0E0E_0001;
const SALT_VEIN_MASK: u64 = 0x0E0E_0002;
const SALT_VEIN_MINERAL: u64 = 0x0E0E_0003;
const SALT_VEIN_WARP: u64 = 0x0E0E_0004;
const SALT_CLUSTER: u64 = 0x0E0E_0100;
const SALT_MOTHERLODE: u64 = 0x0E0E_0200;
const SALT_TOP_UP: u64 = 0x0E0E_0300;
const SALT_STARTER: u64 = 0x0E0E_0400;
const SALT_ARTIFACT_BED: u64 = 0x0E0E_0500;
const SALT_ARTIFACT_STRAY: u64 = 0x0E0E_0501;
const SALT_PREFAB: u64 = 0x0E0E_0600;

/// Every mineral, cheapest first.
pub const MINERALS: [MineralKind; 10] = [
    MineralKind::Copper,
    MineralKind::Iron,
    MineralKind::Silver,
    MineralKind::Gold,
    MineralKind::Emerald,
    MineralKind::Ruby,
    MineralKind::Diamond,
    MineralKind::Platinum,
    MineralKind::Uranium,
    MineralKind::Mythril,
];

/// Depth curve of a mineral: (first row, peak row, last row, peak weight).
const fn depth_curve(mineral: MineralKind) -> (i32, i32, i32, u32) {
    match mineral {
        MineralKind::Copper => (SKY_ROWS, 9, 42, 100),
        MineralKind::Iron => (9, 22, 58, 80),
        MineralKind::Silver => (16, 32, 76, 60),
        MineralKind::Gold => (26, 48, 104, 46),
        MineralKind::Emerald => (42, 66, 136, 34),
        MineralKind::Ruby => (50, 80, 152, 26),
        MineralKind::Diamond => (72, 110, 220, 20),
        MineralKind::Platinum => (96, 130, 206, 16),
        MineralKind::Uranium => (128, 162, 220, 12),
        MineralKind::Mythril => (140, 190, 220, 9),
    }
}

/// The minerals a deep stratum is known for, with the weight bonus each gets there.
///
/// The deepest strata lean on the most valuable minerals so ore value keeps rising with depth
/// (the reward for braving the worst hazards).
const fn stratum_signatures(stratum: DeepStratum) -> &'static [(MineralKind, u32)] {
    match stratum {
        DeepStratum::CrystalFaults => &[(MineralKind::Diamond, 24)],
        DeepStratum::FossilOceans => &[(MineralKind::Platinum, 24)],
        DeepStratum::PressureCathedrals | DeepStratum::AncientMachineLayer => {
            &[(MineralKind::Mythril, 24)]
        }
        DeepStratum::RadioactiveHollow => &[(MineralKind::Uranium, 24)],
        DeepStratum::VoidGeodeFields => &[(MineralKind::Diamond, 12), (MineralKind::Mythril, 30)],
        DeepStratum::MantleStormZone => &[
            (MineralKind::Mythril, 36),
            (MineralKind::Uranium, 10),
            (MineralKind::Ruby, 6),
        ],
    }
}

/// Relative weight of each mineral (in [`MINERALS`] order) at row `y` of `band`. Minerals too
/// hard for the band's required drill never appear, so ore never blocks a band.
#[must_use]
pub fn mineral_weights(band: Band, y: i32) -> [u32; 10] {
    let mut weights = [0; 10];
    for (weight, mineral) in weights.iter_mut().zip(MINERALS) {
        let (first, peak, last, top) = depth_curve(mineral);
        if y < first || y > last {
            continue;
        }
        let (distance, span) = if y <= peak {
            (peak - y, (peak - first).max(1))
        } else {
            (y - peak, (last - peak).max(1))
        };
        let share = u32::try_from((span - distance).max(0) * 100 / span).unwrap_or_default();
        *weight = top * (10 + share * 9 / 10) / 100;
        if crate::terrain::tile_hardness(TileKind::Ore(mineral)) > band.required_drill_strength() {
            *weight = 0;
        }
    }
    if let Band::Deep(stratum) = band {
        let signatures = stratum_signatures(stratum);
        let mut index = 0;
        while index < signatures.len() {
            let (mineral, bonus) = signatures[index];
            let slot = mineral as usize;
            weights[slot] = weights[slot] * 3 + bonus;
            index += 1;
        }
    }
    weights
}

/// A mineral picked from the depth curves by a hash.
fn mineral_from_hash(band: Band, y: i32, hash: u64) -> Option<MineralKind> {
    let weights = mineral_weights(band, y);
    let total: u64 = weights.iter().map(|weight| u64::from(*weight)).sum();
    if total == 0 {
        return None;
    }
    let mut roll = hash % total;
    for (weight, mineral) in weights.iter().zip(MINERALS) {
        if roll < u64::from(*weight) {
            return Some(mineral);
        }
        roll -= u64::from(*weight);
    }
    None
}

/// Places ore and artifacts, then tops up budgets.
pub(crate) fn place(canvas: &mut Canvas<'_>) {
    place_motherlodes(canvas);
    place_clusters(canvas);
    place_veins(canvas);
    top_up_bands(canvas);
    guarantee_starter_field(canvas);
    place_artifacts(canvas);
}

/// Ore richness scaled by the world setting (percent).
const fn richness(canvas: &Canvas<'_>) -> u32 {
    canvas.settings.richness.percent()
}

/// Veins: thin lines along the ridges of two noise fields, broken into segments by a mask and
/// coloured per stretch so a vein reads as one mineral.
fn place_veins(canvas: &mut Canvas<'_>) {
    let seed = canvas.seed();
    let rich = i32::try_from(richness(canvas)).unwrap_or(100);
    // Richer worlds keep more of each vein.
    let mask_threshold = NOISE_MAX * (100 - rich * 45 / 100) / 100;
    for family in 0..2_u64 {
        let (cell_x, cell_y) = if family == 0 { (26, 13) } else { (13, 22) };
        for y in SKY_ROWS + 1..canvas.height {
            for x in 0..canvas.width {
                if !canvas.is_plain_rock(x, y) {
                    continue;
                }
                // A little domain warp makes the veins wander instead of running ruler-straight.
                let warp_x = x + centred(value_noise(seed, SALT_VEIN_WARP, x, y, 5, 5), 3);
                let warp_y = y + centred(value_noise(seed, SALT_VEIN_WARP + 1, x, y, 5, 5), 2);
                let line = ridge(fractal_noise(
                    seed,
                    SALT_VEIN + family,
                    warp_x,
                    warp_y,
                    cell_x,
                    cell_y,
                    2,
                ));
                if line < NOISE_MAX - 4_200 {
                    continue;
                }
                let mask = value_noise(seed, SALT_VEIN_MASK + family, x, y, 18, 10);
                if mask < mask_threshold {
                    continue;
                }
                let band = canvas.band(x, y);
                let stretch = hash2(
                    seed,
                    SALT_VEIN_MINERAL + family,
                    x.div_euclid(9),
                    y.div_euclid(6),
                );
                if let Some(mineral) = mineral_from_hash(band, y, stretch) {
                    canvas.set(x, y, TileKind::Ore(mineral));
                }
            }
        }
    }
}

const CLUSTER_REGION: (i32, i32) = (14, 10);

/// Clusters per region (thousandths) by band, before richness.
const fn cluster_density(band: Band) -> u32 {
    match band {
        Band::Deep(DeepStratum::VoidGeodeFields) => 2_500,
        Band::Deep(DeepStratum::MantleStormZone) => 2_400,
        Band::Topsoil | Band::Deep(_) => 1_500,
        Band::Clay => 1_900,
        Band::Stone | Band::Slate | Band::Basalt => 2_000,
    }
}

/// Clusters: blobs of one mineral scattered by region.
fn place_clusters(canvas: &mut Canvas<'_>) {
    let seed = canvas.seed();
    let rich = richness(canvas);
    let regions_x = (canvas.width + CLUSTER_REGION.0 - 1) / CLUSTER_REGION.0;
    let regions_y = (canvas.height + CLUSTER_REGION.1 - 1) / CLUSTER_REGION.1;
    for ry in 0..regions_y {
        for rx in 0..regions_x {
            let mut rng = RegionRng::new(seed, SALT_CLUSTER, rx, ry);
            let origin = (rx * CLUSTER_REGION.0, ry * CLUSTER_REGION.1);
            let band = canvas.band(
                origin.0 + CLUSTER_REGION.0 / 2,
                origin.1 + CLUSTER_REGION.1 / 2,
            );
            let mut expected = cluster_density(band) * rich / 100;
            while expected > 0 {
                if expected >= 1000 || rng.chance(expected) {
                    let x = origin.0 + rng.range(0, CLUSTER_REGION.0 - 1);
                    let y = origin.1 + rng.range(0, CLUSTER_REGION.1 - 1);
                    cluster(canvas, &mut rng, x, y, None);
                }
                expected = expected.saturating_sub(1000);
            }
        }
    }
}

/// One cluster at `(x, y)`; returns the ore tiles placed.
fn cluster(
    canvas: &mut Canvas<'_>,
    rng: &mut RegionRng,
    x: i32,
    y: i32,
    mineral: Option<MineralKind>,
) -> u32 {
    if y <= SKY_ROWS {
        return 0;
    }
    let band = canvas.band(x, y);
    let mineral = match mineral {
        Some(mineral) => mineral,
        None => match rng.pick(&mineral_weights(band, y)) {
            Some(index) => MINERALS[index],
            None => return 0,
        },
    };
    // Cheap ore comes in bigger pods, precious ore in small nuggets. Pods are slightly wide
    // ellipses with ragged (noise-displaced) edges.
    let (radius_x, radius_y) = match mineral {
        MineralKind::Copper | MineralKind::Iron | MineralKind::Silver => {
            (rng.range(2, 3), rng.range(1, 2))
        }
        MineralKind::Gold | MineralKind::Emerald | MineralKind::Ruby => {
            (rng.range(1, 3), rng.range(1, 2))
        }
        MineralKind::Uranium | MineralKind::Mythril => (rng.range(1, 2), 1),
        _ => (rng.range(2, 3), rng.range(1, 2)),
    };
    let edge_salt = rng.next_u64();
    let mut placed = 0;
    let (rx2, ry2) = (radius_x * radius_x, (radius_y * radius_y).max(1));
    for dy in -radius_y..=radius_y {
        for dx in -radius_x..=radius_x {
            let (tx, ty) = (x + dx, y + dy);
            let distance = dx * dx * ry2 + dy * dy * rx2;
            let ragged = value_noise(canvas.seed(), edge_salt, tx, ty, 2, 2);
            if distance * NOISE_MAX > rx2 * ry2 * (NOISE_MAX / 2 + ragged * 3 / 4) {
                continue;
            }
            if ty > SKY_ROWS
                && canvas.is_plain_rock(tx, ty)
                && mineral_fits(canvas.band(tx, ty), mineral)
            {
                canvas.set(tx, ty, TileKind::Ore(mineral));
                placed += 1;
            }
        }
    }
    placed
}

/// Whether `mineral` may sit in `band` without blocking it (hardness within the band's drill).
const fn mineral_fits(band: Band, mineral: MineralKind) -> bool {
    crate::terrain::tile_hardness(TileKind::Ore(mineral)) <= band.required_drill_strength()
}

const MOTHERLODE_REGION: (i32, i32) = (60, 40);

/// Motherlodes: rare, dense deposits of the most valuable mineral at their depth.
fn place_motherlodes(canvas: &mut Canvas<'_>) {
    let seed = canvas.seed();
    let rich = richness(canvas);
    let regions_x = (canvas.width + MOTHERLODE_REGION.0 - 1) / MOTHERLODE_REGION.0;
    let regions_y = (canvas.height + MOTHERLODE_REGION.1 - 1) / MOTHERLODE_REGION.1;
    for ry in 0..regions_y {
        for rx in 0..regions_x {
            let mut rng = RegionRng::new(seed, SALT_MOTHERLODE, rx, ry);
            let x = rx * MOTHERLODE_REGION.0 + rng.range(6, MOTHERLODE_REGION.0 - 6);
            let y = ry * MOTHERLODE_REGION.1 + rng.range(5, MOTHERLODE_REGION.1 - 5);
            if y < 34 || x >= canvas.width - 4 || y >= canvas.height - 4 {
                continue;
            }
            if !rng.chance(260 * rich / 100) {
                continue;
            }
            let band = canvas.band(x, y);
            let weights = mineral_weights(band, y);
            // The most valuable mineral with a real presence here.
            let Some(mineral) = MINERALS
                .iter()
                .zip(weights)
                .filter(|(_, weight)| *weight >= 8)
                .map(|(mineral, _)| *mineral)
                .next_back()
            else {
                continue;
            };
            let (half_width, half_height) = (rng.range(3, 5), rng.range(2, 4));
            let mut placed = 0_u32;
            for dy in -half_height..=half_height {
                for dx in -half_width..=half_width {
                    let inside = dx * dx * half_height * half_height
                        + dy * dy * half_width * half_width
                        <= half_width * half_width * half_height * half_height;
                    let (tx, ty) = (x + dx, y + dy);
                    if inside
                        && rng.chance(820)
                        && canvas.is_plain_rock(tx, ty)
                        && mineral_fits(canvas.band(tx, ty), mineral)
                    {
                        canvas.set(tx, ty, TileKind::Ore(mineral));
                        placed += 1;
                    }
                }
            }
            if placed >= 6 {
                let center = TilePosition { x, y };
                canvas.features.push(
                    FeatureKind::Motherlode,
                    FeatureDetail::Mineral(mineral),
                    band,
                    center,
                    TileRect::around(center, half_width, half_height),
                    placed,
                );
            }
        }
    }
}

/// Ore value and tile count per band of a canvas (in [`Band::ALL`] order).
fn band_ore_values(canvas: &Canvas<'_>) -> ([u64; 12], [u64; 12]) {
    let mut values = [0_u64; 12];
    let mut tiles = [0_u64; 12];
    for y in SKY_ROWS..canvas.height {
        for x in 0..canvas.width {
            let band = canvas.band(x, y).index();
            tiles[band] += 1;
            if let Some(TileKind::Ore(mineral)) = canvas.get(x, y) {
                values[band] += u64::from(mineral.value());
            }
        }
    }
    (values, tiles)
}

/// Target mean ore value per ten tiles of a band at standard richness (the fairness suite and
/// the report check worlds against it).
#[must_use]
pub const fn band_ore_target_per_ten_tiles(band: Band) -> u64 {
    match band {
        Band::Topsoil => 13,
        Band::Clay => 22,
        Band::Stone => 40,
        Band::Slate => 75,
        Band::Basalt => 125,
        Band::Deep(DeepStratum::CrystalFaults) => 215,
        Band::Deep(DeepStratum::FossilOceans) => 260,
        Band::Deep(DeepStratum::PressureCathedrals) => 340,
        Band::Deep(DeepStratum::RadioactiveHollow) => 320,
        Band::Deep(DeepStratum::AncientMachineLayer) => 420,
        Band::Deep(DeepStratum::VoidGeodeFields) => 450,
        Band::Deep(DeepStratum::MantleStormZone) => 490,
    }
}

/// Share (percent) of a band's target that its ore is always topped up to.
pub const BAND_BUDGET_PERCENT: u64 = 70;

/// Adds clusters to any band below its ore budget.
fn top_up_bands(canvas: &mut Canvas<'_>) {
    let seed = canvas.seed();
    let rich = u64::from(richness(canvas));
    let (mut values, tiles) = band_ore_values(canvas);
    for band in Band::ALL {
        let budget =
            band_ore_target_per_ten_tiles(band) * tiles[band.index()] * rich * BAND_BUDGET_PERCENT
                / 100_000;
        let mut rng = RegionRng::new(
            seed,
            SALT_TOP_UP,
            i32::try_from(band.index()).unwrap_or(0),
            0,
        );
        let top = nominal_band_top(band) - 4;
        let mut attempts = 0;
        while values[band.index()] < budget && attempts < 4_000 {
            attempts += 1;
            let x = rng.range(0, canvas.width - 1);
            let y = rng
                .range(top, top + 26)
                .clamp(SKY_ROWS + 1, canvas.height - 1);
            if canvas.band(x, y) != band {
                continue;
            }
            let before = band_value_near(canvas, x, y);
            cluster(canvas, &mut rng, x, y, None);
            let after = band_value_near(canvas, x, y);
            values[band.index()] += after.saturating_sub(before);
        }
    }
}

/// Ore value within a cluster's reach of `(x, y)`.
fn band_value_near(canvas: &Canvas<'_>, x: i32, y: i32) -> u64 {
    let mut value = 0;
    for dy in -3..=3 {
        for dx in -3..=3 {
            if let Some(TileKind::Ore(mineral)) = canvas.get(x + dx, y + dy) {
                value += u64::from(mineral.value());
            }
        }
    }
    value
}

/// Half-width of the starter field around the spawn column.
pub const STARTER_FIELD_HALF_WIDTH: i32 = 45;
/// Deepest row of the starter field (still topsoil in every world).
pub const STARTER_FIELD_LAST_ROW: i32 = 12;
/// Ore value the starter field always holds, whatever the richness: enough for the first
/// drill, fuel and cargo upgrades with a margin for unlucky routes.
pub const STARTER_FIELD_VALUE: u64 = 1_500;

/// Value of the drill-1 ore (copper and iron) in the starter field.
#[must_use]
pub fn starter_field_value(kinds: impl Fn(i32, i32) -> Option<TileKind>) -> u64 {
    let mut value = 0;
    for y in SKY_ROWS..=STARTER_FIELD_LAST_ROW {
        for x in SPAWN_TILE_X - STARTER_FIELD_HALF_WIDTH..=SPAWN_TILE_X + STARTER_FIELD_HALF_WIDTH {
            if let Some(TileKind::Ore(mineral @ (MineralKind::Copper | MineralKind::Iron))) =
                kinds(x, y)
            {
                value += u64::from(mineral.value());
            }
        }
    }
    value
}

/// Adds copper and iron lumps near spawn until the starter field is worth enough.
fn guarantee_starter_field(canvas: &mut Canvas<'_>) {
    let mut rng = RegionRng::new(canvas.seed(), SALT_STARTER, 0, 0);
    let mut attempts = 0;
    while starter_field_value(|x, y| canvas.get(x, y)) < STARTER_FIELD_VALUE && attempts < 400 {
        attempts += 1;
        let x = SPAWN_TILE_X + rng.range(-STARTER_FIELD_HALF_WIDTH, STARTER_FIELD_HALF_WIDTH);
        let y = rng.range(SKY_ROWS + 2, STARTER_FIELD_LAST_ROW);
        let mineral = if rng.chance(700) || y < 9 {
            MineralKind::Copper
        } else {
            MineralKind::Iron
        };
        cluster(canvas, &mut rng, x, y, Some(mineral));
    }
}

/// The artifact kinds that fit a band, with relative weights.
const fn band_artifacts(band: Band) -> &'static [(ArtifactKind, u32)] {
    match band {
        Band::Topsoil => &[],
        Band::Clay => &[(ArtifactKind::Fossil, 1)],
        Band::Stone => &[(ArtifactKind::Fossil, 3), (ArtifactKind::OldCircuit, 2)],
        Band::Slate => &[(ArtifactKind::OldCircuit, 3), (ArtifactKind::BuriedIdol, 2)],
        Band::Basalt => &[(ArtifactKind::BuriedIdol, 2), (ArtifactKind::StarCore, 3)],
        Band::Deep(stratum) => match stratum {
            DeepStratum::CrystalFaults => &[(ArtifactKind::CrystalPrism, 1)],
            DeepStratum::FossilOceans => &[(ArtifactKind::RadiantShell, 1)],
            DeepStratum::PressureCathedrals => &[(ArtifactKind::PressureGlyph, 1)],
            DeepStratum::RadioactiveHollow => &[(ArtifactKind::ReactorBloom, 1)],
            DeepStratum::AncientMachineLayer => &[(ArtifactKind::MachineSigil, 1)],
            DeepStratum::VoidGeodeFields => &[(ArtifactKind::VoidPearl, 1)],
            DeepStratum::MantleStormZone => &[(ArtifactKind::MantleHeart, 1)],
        },
    }
}

pub(crate) fn pick_artifact(band: Band, rng: &mut RegionRng) -> Option<ArtifactKind> {
    let table = band_artifacts(band);
    let weights: Vec<u32> = table.iter().map(|(_, weight)| *weight).collect();
    rng.pick(&weights).map(|index| table[index].0)
}

/// Artifact beds (a few artifacts close together, one or two per band) and rare strays.
fn place_artifacts(canvas: &mut Canvas<'_>) {
    let seed = canvas.seed();
    let width_factor = canvas.width / 120;
    for band in Band::ALL.into_iter().skip(1) {
        let mut rng = RegionRng::new(
            seed,
            SALT_ARTIFACT_BED,
            i32::try_from(band.index()).unwrap_or(0),
            0,
        );
        // Two or three beds per band in a standard world, three or four in a large one.
        let beds = width_factor + i32::from(rng.chance(500));
        let top = nominal_band_top(band);
        let mut placed_beds = 0;
        let mut attempts = 0;
        while placed_beds < beds && attempts < 200 {
            attempts += 1;
            let x = rng.range(4, canvas.width - 5);
            let y = rng.range(top + 2, top + 16).min(canvas.height - 3);
            if canvas.band(x, y) != band || !canvas.is_plain_rock(x, y) {
                continue;
            }
            if (x - SPAWN_TILE_X).abs() < 12 && y < 30 {
                continue;
            }
            let count = rng.range(3, 5);
            let mut placed = 0_u32;
            let mut kind_seen = None;
            for _ in 0..count * 4 {
                if placed >= u32::try_from(count).unwrap_or_default() {
                    break;
                }
                let (tx, ty) = (x + rng.range(-3, 3), y + rng.range(-2, 2));
                if canvas.band(tx, ty) != band || !canvas.is_plain_rock(tx, ty) {
                    continue;
                }
                let Some(artifact) = pick_artifact(band, &mut rng) else {
                    break;
                };
                kind_seen.get_or_insert(artifact);
                canvas.set(tx, ty, TileKind::Artifact(artifact));
                placed += 1;
            }
            if placed >= 2
                && let Some(artifact) = kind_seen
            {
                placed_beds += 1;
                let center = TilePosition { x, y };
                canvas.features.push(
                    FeatureKind::ArtifactBed,
                    FeatureDetail::Artifact(artifact),
                    band,
                    center,
                    TileRect::around(center, 3, 2),
                    placed,
                );
            }
        }
    }
    // Strays: rare single artifacts anywhere below the clay line.
    let rich = richness(canvas);
    for y in 20..canvas.height {
        for x in 0..canvas.width {
            if !canvas.is_plain_rock(x, y)
                || !chance(seed, SALT_ARTIFACT_STRAY, x, y, 2 * rich / 100 + 1)
            {
                continue;
            }
            let band = canvas.band(x, y);
            let mut rng = RegionRng::new(seed, SALT_ARTIFACT_STRAY ^ 0xFF, x, y);
            if let Some(artifact) = pick_artifact(band, &mut rng) {
                canvas.set(x, y, TileKind::Artifact(artifact));
            }
        }
    }
}

/// Reserves a few untouched spots per band for future prefab rooms.
pub(crate) fn reserve_prefab_slots(canvas: &mut Canvas<'_>) {
    let seed = canvas.seed();
    let segments = (canvas.width / 120).max(1);
    let segment_width = canvas.width / segments;
    for band in Band::ALL.into_iter().skip(1) {
        let top = nominal_band_top(band);
        for segment in 0..segments {
            let mut rng = RegionRng::new(
                seed,
                SALT_PREFAB,
                i32::try_from(band.index()).unwrap_or(0),
                segment,
            );
            let x = segment * segment_width + rng.range(10, segment_width - 10);
            let y = (top + rng.range(5, 12)).min(canvas.height - 5);
            if (x - SPAWN_TILE_X).abs() < 14 && y < 30 {
                continue;
            }
            let center = TilePosition { x, y };
            canvas.features.push(
                FeatureKind::PrefabSlot,
                FeatureDetail::None,
                canvas.band(x, y),
                center,
                TileRect::around(center, 5, 3),
                0,
            );
        }
    }
}
