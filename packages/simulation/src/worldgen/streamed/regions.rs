//! Region-grid placement for streamed worlds.
//!
//! Each placement layer splits the plane into a fixed grid of regions. A region's output (its
//! worm tunnels, chamber, point of interest, hazard clusters, ore candidates, ...) is computed
//! from a per-region RNG plus queries of the layers below it, never from neighbouring outputs
//! of its own layer, so it is the same whichever chunk asks for it. Outputs reach at most a
//! fixed distance past their region (the `*_REACH` constants), which bounds the regions a
//! chunk has to consult. Outputs are memoized in process-wide caches.
//!
//! Layer order (each may read the ones before it):
//! 1. base rock ([`super::base`]) and points of interest (base rock only);
//! 2. worm tunnels and chambers (pure geometry), giving the cave layer
//!    ([`super::window::Window`]), which never opens inside a point of interest;
//! 3. hazard clusters and large caverns (the cave layer);
//! 4. ore and artifact candidates (pure; applied to whatever plain rock is left).

use std::sync::Arc;

use crate::{
    poi::{LoreId, PoiKind, PoiTile, PoiVariant},
    terrain::{ArtifactKind, DeepStratum, MineralKind, TileKind, TilePosition},
    worldgen::{
        SPAWN_TILE_X, caves,
        deposits::{self, MINERALS, mineral_fits},
        features::{FeatureDetail, FeatureKind, HazardKind, TileRect, WorldFeature},
        hazards::{self, MIN_CLUSTER_TILES, hazard_allowed},
        noise::{NOISE_MAX, RegionRng, hash2, value_noise},
        pois::{
            self, DEEP_RANDOM_FILL_PER_MILLE, LORE_COUNT, POI_MIN_ROW, clear_of_spawn_and_town,
            guaranteed_kinds, guardian_hazard,
        },
        prefabs::{Cell, Prefab, PrefabRole, prefabs_of},
        strata::{Band, SKY_ROWS},
    },
};

use super::{
    base::{Ctx, Tier, is_plain_rock},
    cache::MemoCache,
    index::{Layer, feature_id},
    window::Window,
};

pub use crate::worldgen::caves::{CHAMBER_REGION, WORM_REGION};

/// Furthest a worm tunnel reaches from its region (its length plus a bulge).
pub const WORM_REACH: i32 = 72;
/// Furthest a chamber reaches from its region.
pub const CHAMBER_REACH: i32 = 18;
/// Width of one point-of-interest region (rows come from the band tiers).
pub const POI_REGION_WIDTH: i32 = 48;
/// Columns a point of interest keeps from its region's sides (so neighbouring regions' POIs
/// are always more than [`pois::POI_SPACING`] apart).
const POI_SIDE_MARGIN: i32 = 4;
/// Rows a point of interest keeps from its tier's top and bottom.
const POI_TIER_MARGIN: i32 = 3;
/// Regions in one rotation of a tier's guaranteed kinds beyond the kinds themselves.
const POI_FILL_SLOTS: usize = 3;
pub use crate::worldgen::hazards::REGION as HAZARD_REGION;
/// Furthest a hazard cluster grows from its region.
pub const HAZARD_REACH: i32 = 14;
/// Region of large-cavern detection.
pub const CAVERN_REGION: (i32, i32) = (64, 32);
/// Lattice step of large-cavern tracing.
const CAVERN_SAMPLE_STEP: i32 = 2;
pub use crate::worldgen::deposits::{CLUSTER_REGION, MOTHERLODE_REGION};
/// Furthest a cluster pod reaches from its region.
pub const CLUSTER_REACH: i32 = 4;
/// Furthest a motherlode reaches from its region.
pub const MOTHERLODE_REACH: i32 = 1;
/// Region of artifact beds.
pub const BED_REGION: (i32, i32) = (80, 30);
/// Furthest an artifact bed reaches from its region.
pub const BED_REACH: i32 = 4;
/// Chance (per mille) that a bed region holds a bed.
const BED_CHANCE: u32 = 800;
/// Width of the regions band transitions are indexed per.
pub const TRANSITION_REGION_WIDTH: i32 = 256;

/// Starter-field candidate value relative to the guaranteed value (percent): candidates over
/// rock that turns out not to be plain still leave the field above its guarantee.
const STARTER_CANDIDATE_PERCENT: u64 = 260;

const V5_SALT: u64 = 0x0005_0000_0000_0000;

/// The region indices whose cells cover `min..=max` along one axis.
pub const fn span(min: i32, max: i32, size: i32) -> std::ops::RangeInclusive<i32> {
    min.div_euclid(size)..=max.div_euclid(size)
}

/// The rows of region row `ry` clamped to the world, or `None` when it lies outside.
fn region_rows(ctx: &Ctx, ry: i32, size: i32) -> Option<(i32, i32)> {
    let top = ry.checked_mul(size)?;
    let bottom = top.checked_add(size - 1)?;
    (bottom >= 0 && top < ctx.height).then_some((top.max(0), bottom.min(ctx.height - 1)))
}

const fn region_rect(rx: i32, ry: i32, size: (i32, i32)) -> TileRect {
    TileRect {
        min: TilePosition {
            x: rx * size.0,
            y: ry * size.1,
        },
        max: TilePosition {
            x: rx * size.0 + size.0 - 1,
            y: ry * size.1 + size.1 - 1,
        },
    }
}

// ---------------------------------------------------------------------------------------------
// Worm tunnels

#[derive(Debug, Default)]
pub struct WormRegion {
    /// `(x, y, radius)` discs carved along the region's tunnels, in order.
    pub discs: Vec<(i32, i32, i32)>,
    pub bounds: Option<TileRect>,
}

static WORMS: MemoCache<WormRegion> = MemoCache::new(8_192);

pub fn worm_region(ctx: &Ctx, rx: i32, ry: i32) -> Arc<WormRegion> {
    WORMS.get_or_compute((ctx.settings, rx, ry), || compute_worms(ctx, rx, ry))
}

fn compute_worms(ctx: &Ctx, rx: i32, ry: i32) -> WormRegion {
    let mut region = WormRegion::default();
    if region_rows(ctx, ry, WORM_REGION.1).is_none() {
        return region;
    }
    let mut rng = RegionRng::new(ctx.seed(), caves::SALT_WORM, rx, ry);
    let center_band = ctx.band(
        rx * WORM_REGION.0 + WORM_REGION.0 / 2,
        (ry * WORM_REGION.1 + WORM_REGION.1 / 2).min(ctx.height - 1),
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
        let start_y =
            (ry * WORM_REGION.1 + rng.range(0, WORM_REGION.1 - 1)).max(caves::CRUST_ROWS + 2);
        let deep = matches!(center_band, Band::Deep(_) | Band::Basalt);
        worm(ctx, &mut region.discs, &mut rng, (start_x, start_y), deep);
    }
    region.bounds = region.discs.iter().fold(None, |bounds, &(x, y, radius)| {
        let disc = TileRect::around(TilePosition { x, y }, radius, radius);
        Some(bounds.map_or(disc, |bounds: TileRect| TileRect {
            min: TilePosition {
                x: bounds.min.x.min(disc.min.x),
                y: bounds.min.y.min(disc.min.y),
            },
            max: TilePosition {
                x: bounds.max.x.max(disc.max.x),
                y: bounds.max.y.max(disc.max.y),
            },
        }))
    });
    region
}

/// One worm tunnel (the V4 walk, bounded only by the world's rows).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "heading indices are bounded by the table length"
)]
fn worm(
    ctx: &Ctx,
    discs: &mut Vec<(i32, i32, i32)>,
    rng: &mut RegionRng,
    start: (i32, i32),
    deep: bool,
) {
    const SIDEWAYS: [usize; 6] = [0, 1, 15, 7, 8, 9];
    let mut heading = if rng.chance(750) {
        SIDEWAYS[rng.range(0, 5) as usize]
    } else {
        rng.range(0, 15) as usize
    };
    let (mut x, mut y) = (
        i64::from(start.0) * 256 + 128,
        i64::from(start.1) * 256 + 128,
    );
    let length = rng.range(24, if deep { 70 } else { 56 });
    let mut bulge = 0;
    for _ in 0..length {
        if deep && bulge == 0 && rng.chance(20) {
            bulge = rng.range(3, 7);
        }
        let radius = 1 + i32::from(bulge > 0);
        bulge = (bulge - 1).max(0);
        let (tile_x, tile_y) = ((x >> 8) as i32, (y >> 8) as i32);
        discs.push((tile_x, tile_y, radius));
        if rng.chance(230) {
            heading = if rng.chance(500) {
                (heading + 1) % 16
            } else {
                (heading + 15) % 16
            };
        }
        if matches!(heading, 4 | 12) && rng.chance(300) {
            heading = if rng.chance(500) { 3 } else { 13 };
        }
        let (dx, dy) = caves::HEADINGS[heading];
        x += i64::from(dx);
        y += i64::from(dy);
        if ((y >> 8) as i32) < caves::CRUST_ROWS + 1 {
            heading = if dx >= 0 { 1 } else { 7 };
        }
        if ((y >> 8) as i32) >= ctx.height - 1 {
            break;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Chambers

#[derive(Clone, Copy, Debug)]
pub struct Chamber {
    pub x: i32,
    pub y: i32,
    pub half_width: i32,
    pub half_height: i32,
}

#[derive(Debug, Default)]
pub struct ChamberRegion {
    pub chamber: Option<Chamber>,
    pub feature: Option<WorldFeature>,
}

static CHAMBERS: MemoCache<ChamberRegion> = MemoCache::new(4_096);

pub fn chamber_region(ctx: &Ctx, rx: i32, ry: i32) -> Arc<ChamberRegion> {
    CHAMBERS.get_or_compute((ctx.settings, rx, ry), || compute_chamber(ctx, rx, ry))
}

/// Whether `(x, y)` lies inside `chamber`'s ragged ellipse.
pub fn chamber_tile(ctx: &Ctx, chamber: &Chamber, x: i32, y: i32) -> bool {
    let (dx, dy) = (x - chamber.x, y - chamber.y);
    let (hw, hh) = (
        i64::from(chamber.half_width),
        i64::from(chamber.half_height),
    );
    let r2 = hw * hw * hh * hh;
    let d = i64::from(dx * dx) * hh * hh + i64::from(dy * dy) * hw * hw;
    let edge = i64::from(value_noise(
        ctx.seed(),
        caves::SALT_CHAMBER_EDGE,
        x,
        y,
        3,
        3,
    ));
    d <= r2 * (49_152 + edge / 2) / 65_536
}

fn compute_chamber(ctx: &Ctx, rx: i32, ry: i32) -> ChamberRegion {
    let mut region = ChamberRegion::default();
    if region_rows(ctx, ry, CHAMBER_REGION.1).is_none() {
        return region;
    }
    let mut rng = RegionRng::new(ctx.seed(), caves::SALT_CHAMBER, rx, ry);
    let cx = rx * CHAMBER_REGION.0 + rng.range(10, CHAMBER_REGION.0 - 10);
    let cy = ry * CHAMBER_REGION.1 + rng.range(6, CHAMBER_REGION.1 - 6);
    if cy >= ctx.floor_row(cx) - 6 {
        return region;
    }
    let band = ctx.band(cx, cy);
    let Band::Deep(stratum) = band else {
        return region;
    };
    let per_mille = match stratum {
        DeepStratum::PressureCathedrals => 700,
        DeepStratum::VoidGeodeFields | DeepStratum::FossilOceans => 450,
        DeepStratum::AncientMachineLayer => 250,
        _ => 330,
    };
    if !rng.chance(per_mille) {
        return region;
    }
    let (half_width, half_height) = match stratum {
        DeepStratum::PressureCathedrals => (rng.range(6, 10), rng.range(6, 9)),
        DeepStratum::FossilOceans => (rng.range(10, 15), rng.range(4, 6)),
        _ => (rng.range(6, 12), rng.range(4, 7)),
    };
    let chamber = Chamber {
        x: cx,
        y: cy,
        half_width,
        half_height,
    };
    let mut size = 0_u32;
    for dy in -half_height - 1..=half_height + 1 {
        for dx in -half_width - 1..=half_width + 1 {
            if chamber_tile(ctx, &chamber, cx + dx, cy + dy) {
                size += 1;
            }
        }
    }
    let center = TilePosition { x: cx, y: cy };
    region.chamber = Some(chamber);
    region.feature = Some(WorldFeature {
        id: feature_id(Layer::Chamber, rx, ry, 0),
        kind: FeatureKind::Chamber,
        detail: FeatureDetail::None,
        band,
        center,
        bounds: TileRect::around(center, half_width + 1, half_height + 1),
        size,
    });
    region
}

// ---------------------------------------------------------------------------------------------
// Points of interest

#[derive(Debug)]
pub struct PlacedPoi {
    /// Tiles the prefab writes, `(x, y, kind)`.
    pub cells: Vec<(i32, i32, TileKind)>,
    pub feature: WorldFeature,
}

#[derive(Debug, Default)]
pub struct PoiRegion {
    pub poi: Option<PlacedPoi>,
}

static POIS: MemoCache<PoiRegion> = MemoCache::new(8_192);

/// The point-of-interest region in column `rx` of tier `tier` (an index into [`Ctx::tiers`]).
pub fn poi_region(ctx: &Ctx, rx: i32, tier: usize) -> Arc<PoiRegion> {
    let Some(spec) = ctx.tiers.get(tier).copied() else {
        return Arc::new(PoiRegion::default());
    };
    POIS.get_or_compute((ctx.settings, rx, spec.key), || compute_poi(ctx, rx, &spec))
}

/// Every point-of-interest region whose rows and columns overlap `rect`.
pub fn poi_regions_in(ctx: &Ctx, rect: &TileRect) -> Vec<Arc<PoiRegion>> {
    let mut regions = Vec::new();
    for (index, tier) in ctx.tiers.iter().enumerate() {
        if tier.bottom < rect.min.y || tier.top > rect.max.y {
            continue;
        }
        for rx in span(rect.min.x, rect.max.x, POI_REGION_WIDTH) {
            regions.push(poi_region(ctx, rx, index));
        }
    }
    regions
}

/// The kinds a tier deals out in rotation: its share of the band's guaranteed kinds, plus a
/// supply cache on every further tier of a deep band below the topsoil.
fn tier_kinds(tier: &Tier) -> Vec<PoiKind> {
    let mut kinds: Vec<PoiKind> = guaranteed_kinds(tier.band)
        .iter()
        .enumerate()
        .filter(|(index, _)| index % tier.count == tier.ordinal)
        .map(|(_, kind)| *kind)
        .collect();
    if tier.ordinal > 0 && tier.band != Band::Topsoil {
        kinds.push(PoiKind::SupplyCache);
    }
    kinds
}

#[derive(Clone, Copy)]
struct Spot {
    prefab: &'static Prefab,
    origin: TilePosition,
    flipped: bool,
}

impl Spot {
    const fn bounds(&self) -> TileRect {
        TileRect {
            min: self.origin,
            max: TilePosition {
                x: self.origin.x + self.prefab.width() - 1,
                y: self.origin.y + self.prefab.height() - 1,
            },
        }
    }

    fn cell_at(&self, x: i32, y: i32) -> Cell {
        let local_x = x - self.origin.x;
        let column = if self.flipped {
            self.prefab.width() - 1 - local_x
        } else {
            local_x
        };
        self.prefab.cell(column, y - self.origin.y)
    }
}

fn compute_poi(ctx: &Ctx, rx: i32, tier: &Tier) -> PoiRegion {
    let seed = ctx.seed();
    let kinds = tier_kinds(tier);
    let period = kinds.len() + POI_FILL_SLOTS;
    let phase = hash2(seed, pois::SALT_POI ^ V5_SALT, tier.key, 0) % period as u64;
    let slot = usize::try_from(
        (i64::from(rx) + i64::try_from(phase).unwrap_or(0))
            .rem_euclid(i64::try_from(period).unwrap_or(1)),
    )
    .unwrap_or(0);
    let mut fill_rng = RegionRng::new(seed, pois::SALT_FILL ^ V5_SALT, tier.key, rx);
    let kind = if slot < kinds.len() {
        Some(kinds[slot])
    } else if fill_rng.chance(DEEP_RANDOM_FILL_PER_MILLE) {
        fill_rng
            .pick(&pois::fill_weights(tier.band))
            .map(|index| PoiKind::ALL[index])
    } else {
        None
    };
    let Some(kind) = kind else {
        return PoiRegion::default();
    };
    let mut spot_rng = RegionRng::new(seed, pois::SALT_SPOT ^ V5_SALT, tier.key, rx);
    let Some(placement) = find_spot(ctx, tier, kind, rx, &mut spot_rng) else {
        return PoiRegion::default();
    };
    let mut stamp_rng = RegionRng::new(seed, (pois::SALT_SPOT ^ 0xFF) ^ V5_SALT, tier.key, rx);
    let variant = match placement.prefab.role {
        PrefabRole::SupplyCache => PoiVariant::SupplyCache,
        PrefabRole::DerelictRig => PoiVariant::DerelictRig {
            log: LoreId(
                u16::try_from(hash2(seed, 0x0D_E7E1, tier.key, rx) % u64::from(LORE_COUNT))
                    .unwrap_or(0),
            ),
        },
        PrefabRole::Vault(puzzle) => PoiVariant::Vault(puzzle),
        PrefabRole::Formation(style) => PoiVariant::Formation(style),
        PrefabRole::RivalClaim => PoiVariant::RivalClaim,
    };
    let cells = stamp(ctx, tier.band, &placement, &mut stamp_rng);
    let bounds = placement.bounds();
    PoiRegion {
        poi: Some(PlacedPoi {
            feature: WorldFeature {
                id: feature_id(Layer::Poi, rx, tier.key, 0),
                kind: FeatureKind::PointOfInterest,
                detail: FeatureDetail::Poi(variant),
                band: tier.band,
                center: bounds.center(),
                bounds,
                size: u32::try_from(cells.len()).unwrap_or(u32::MAX),
            },
            cells,
        }),
    }
}

fn find_spot(ctx: &Ctx, tier: &Tier, kind: PoiKind, rx: i32, rng: &mut RegionRng) -> Option<Spot> {
    let candidates: Vec<&'static Prefab> = prefabs_of(kind)
        .filter(|prefab| match prefab.role {
            PrefabRole::Formation(style) => {
                style == crate::poi::FormationStyle::for_band(tier.band)
            }
            _ => true,
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let left = rx * POI_REGION_WIDTH;
    for _ in 0..40 {
        let prefab = candidates[usize::try_from(
            rng.range(0, i32::try_from(candidates.len()).unwrap_or(1) - 1),
        )
        .unwrap_or(0)];
        let (width, height) = (prefab.width(), prefab.height());
        let x = rng.range(
            left + POI_SIDE_MARGIN,
            left + POI_REGION_WIDTH - 1 - width - POI_SIDE_MARGIN,
        );
        let y_low = (tier.top + POI_TIER_MARGIN).max(POI_MIN_ROW);
        let y_high = (tier.bottom - height - POI_TIER_MARGIN).min(ctx.height - height - 2);
        if y_high < y_low {
            continue;
        }
        let y = rng.range(y_low, y_high);
        let spot = Spot {
            prefab,
            origin: TilePosition { x, y },
            flipped: rng.chance(500),
        };
        if fits(ctx, tier.band, kind, &spot) {
            return Some(spot);
        }
    }
    None
}

/// Whether `spot` is a legal placement: its centre in the band, clear of spawn and the town,
/// and over plain rock (no sky, foundations or bedrock). Caves and hazards keep clear of
/// points of interest on their own, so this only reads the base rock.
fn fits(ctx: &Ctx, band: Band, kind: PoiKind, spot: &Spot) -> bool {
    let bounds = spot.bounds();
    if bounds.min.y < POI_MIN_ROW || bounds.max.y > ctx.height - 2 {
        return false;
    }
    let center = bounds.center();
    if ctx.band(center.x, center.y) != band || !clear_of_spawn_and_town(&bounds, kind) {
        return false;
    }
    bounds.positions().all(|position| {
        ctx.base_kind(position.x, position.y)
            .is_some_and(is_plain_rock)
    })
}

fn stamp(ctx: &Ctx, band: Band, spot: &Spot, rng: &mut RegionRng) -> Vec<(i32, i32, TileKind)> {
    let bounds = spot.bounds();
    let mut orders = [1_u8, 2, 3];
    for index in (1..orders.len()).rev() {
        let other = usize::try_from(rng.range(0, i32::try_from(index).unwrap_or(0))).unwrap_or(0);
        orders.swap(index, other);
    }
    let matrix = match spot.prefab.role {
        PrefabRole::Formation(crate::poi::FormationStyle::Geode) => PoiTile::GeodeShell,
        _ => PoiTile::FossilShale,
    };
    let mut cells = Vec::new();
    for position in bounds.positions() {
        let (x, y) = (position.x, position.y);
        let nominal_y = ctx.nominal_row(y);
        let host = ctx.band(x, y).soft_material();
        let kind = match spot.cell_at(x, y) {
            Cell::Keep => continue,
            Cell::Air => TileKind::Air,
            Cell::HostRock => host,
            Cell::Fixture(tile) => TileKind::Poi(tile),
            Cell::CrateOrAir => {
                if rng.chance(500) {
                    TileKind::Poi(PoiTile::SupplyCrate)
                } else {
                    TileKind::Air
                }
            }
            Cell::WreckOrAir => {
                if rng.chance(500) {
                    TileKind::Poi(PoiTile::Wreckage)
                } else {
                    TileKind::Air
                }
            }
            Cell::Switch(index) => TileKind::Poi(PoiTile::VaultSwitch {
                order: orders[usize::from(index.min(2))],
                lit: false,
            }),
            Cell::Artifact => {
                deposits::pick_artifact(band, rng).map_or(TileKind::Poi(matrix), TileKind::Artifact)
            }
            Cell::Ore => pois::formation_ore(band, nominal_y, rng).unwrap_or(TileKind::Poi(matrix)),
            Cell::OreOrMatrix => {
                if rng.chance(500) {
                    pois::formation_ore(band, nominal_y, rng).unwrap_or(TileKind::Poi(matrix))
                } else {
                    TileKind::Poi(matrix)
                }
            }
            Cell::Hazard => {
                if hazard_allowed(x, y) {
                    guardian_hazard(band)
                } else {
                    host
                }
            }
        };
        cells.push((x, y, kind));
    }
    cells
}

// ---------------------------------------------------------------------------------------------
// Hazard clusters

#[derive(Debug)]
pub struct HazardCluster {
    pub kind: HazardKind,
    pub tiles: Vec<(i32, i32)>,
    pub feature: WorldFeature,
}

#[derive(Debug, Default)]
pub struct HazardRegion {
    pub clusters: Vec<HazardCluster>,
}

static HAZARDS: MemoCache<HazardRegion> = MemoCache::new(8_192);

pub fn hazard_region(ctx: &Ctx, rx: i32, ry: i32) -> Arc<HazardRegion> {
    HAZARDS.get_or_compute((ctx.settings, rx, ry), || compute_hazards(ctx, rx, ry))
}

fn compute_hazards(ctx: &Ctx, rx: i32, ry: i32) -> HazardRegion {
    let mut region = HazardRegion::default();
    if region_rows(ctx, ry, HAZARD_REGION.1).is_none() {
        return region;
    }
    let seed = ctx.seed();
    let intensity = ctx.settings.hazards.percent();
    let mut rng = RegionRng::new(seed, hazards::SALT_CLUSTER, rx, ry);
    let rect = region_rect(rx, ry, HAZARD_REGION);
    let origin = (rect.min.x, rect.min.y);
    let band = ctx.band(
        origin.0 + HAZARD_REGION.0 / 2,
        (origin.1 + HAZARD_REGION.1 / 2).min(ctx.height - 1),
    );
    let (density, _) = hazards::band_rules(band);
    let area = u32::try_from(HAZARD_REGION.0 * HAZARD_REGION.1).unwrap_or_default();
    let mut expected = density * area * intensity / 1000;
    if expected == 0 {
        return region;
    }
    let mut window = Window::new(ctx, &rect.expanded(HAZARD_REACH));
    let mut taken: Vec<(i32, i32)> = Vec::new();
    let mut local = 0_u16;
    while expected > 0 {
        if expected >= 1000 || rng.chance(expected) {
            let x = origin.0 + rng.range(0, HAZARD_REGION.0 - 1);
            let y = origin.1 + rng.range(0, HAZARD_REGION.1 - 1);
            if let Some(mut cluster) = grow_cluster(ctx, &mut window, &mut rng, &taken, x, y) {
                cluster.feature.id = feature_id(Layer::Hazard, rx, ry, local);
                local += 1;
                taken.extend(cluster.tiles.iter().copied());
                region.clusters.push(cluster);
            }
        }
        expected = expected.saturating_sub(1000);
    }
    region
}

/// Whether a hazard may sit at `(x, y)`: plain cave-layer rock, away from spawn, the town and
/// every point of interest.
fn hazard_spot(window: &mut Window<'_>, taken: &[(i32, i32)], x: i32, y: i32) -> bool {
    hazard_allowed(x, y)
        && !taken.contains(&(x, y))
        && window.is_plain_rock(x, y)
        && !window.near_poi(x, y, pois::HAZARD_CLEARANCE)
}

/// One cluster grown from `(x, y)` (the V4 blob walk).
fn grow_cluster(
    ctx: &Ctx,
    window: &mut Window<'_>,
    rng: &mut RegionRng,
    taken: &[(i32, i32)],
    x: i32,
    y: i32,
) -> Option<HazardCluster> {
    if !hazard_spot(window, taken, x, y) {
        return None;
    }
    let band = window.band(x, y);
    let mut weights = ctx.hazard_weights(band, x, y);
    let nominal = ctx.nominal_row(y);
    for (weight, kind) in weights.iter_mut().zip(HazardKind::ALL) {
        if nominal < hazards::family_min_row(kind) {
            *weight = 0;
        }
    }
    let kind = HazardKind::ALL[rng.pick(&weights)?];
    let target = match kind {
        HazardKind::Lava => rng.range(5, 12),
        HazardKind::Gas => rng.range(4, 9),
        HazardKind::MagmaVent => rng.range(3, 5),
        HazardKind::Pressure | HazardKind::Explosive => rng.range(3, 7),
    };
    let target = usize::try_from(target).unwrap_or(MIN_CLUSTER_TILES);
    let mut tiles = vec![(x, y)];
    let mut attempts = 0;
    while tiles.len() < target && attempts < target * 8 {
        attempts += 1;
        let from =
            tiles[usize::try_from(rng.range(0, i32::try_from(tiles.len()).unwrap_or(1) - 1))
                .unwrap_or_default()];
        let step = match kind {
            HazardKind::Lava => [(1, 0), (-1, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]
                [usize::try_from(rng.range(0, 5)).unwrap_or_default()],
            HazardKind::MagmaVent => [(0, -1), (0, 1), (0, -1), (1, 0), (-1, 0)]
                [usize::try_from(rng.range(0, 4)).unwrap_or_default()],
            _ => [(1, 0), (-1, 0), (0, 1), (0, -1)]
                [usize::try_from(rng.range(0, 3)).unwrap_or_default()],
        };
        let next = (from.0 + step.0, from.1 + step.1);
        if !tiles.contains(&next) && hazard_spot(window, taken, next.0, next.1) {
            tiles.push(next);
        }
    }
    if tiles.len() < MIN_CLUSTER_TILES {
        return None;
    }
    let (mut min, mut max) = ((x, y), (x, y));
    for (tx, ty) in &tiles {
        min = (min.0.min(*tx), min.1.min(*ty));
        max = (max.0.max(*tx), max.1.max(*ty));
    }
    Some(HazardCluster {
        kind,
        feature: WorldFeature {
            id: feature_id(Layer::Hazard, 0, 0, 0),
            kind: FeatureKind::HazardCluster,
            detail: FeatureDetail::Hazard(kind),
            band,
            center: TilePosition { x, y },
            bounds: TileRect {
                min: TilePosition { x: min.0, y: min.1 },
                max: TilePosition { x: max.0, y: max.1 },
            },
            size: u32::try_from(tiles.len()).unwrap_or(u32::MAX),
        },
        tiles,
    })
}

// ---------------------------------------------------------------------------------------------
// Large caverns

#[derive(Debug, Default)]
pub struct CavernRegion {
    pub feature: Option<WorldFeature>,
}

static CAVERNS: MemoCache<CavernRegion> = MemoCache::new(4_096);

pub fn cavern_region(ctx: &Ctx, rx: i32, ry: i32) -> Arc<CavernRegion> {
    CAVERNS.get_or_compute((ctx.settings, rx, ry), || compute_cavern(ctx, rx, ry))
}

/// The largest connected open area of the region's cave layer (counted inside the region), if
/// it is at least [`caves::LARGE_CAVERN_TILES`] tiles.
fn compute_cavern(ctx: &Ctx, rx: i32, ry: i32) -> CavernRegion {
    let Some((top, bottom)) = region_rows(ctx, ry, CAVERN_REGION.1) else {
        return CavernRegion::default();
    };
    let top = top.max(caves::CRUST_ROWS);
    if top > bottom {
        return CavernRegion::default();
    }
    let rect = TileRect {
        min: TilePosition {
            x: rx * CAVERN_REGION.0,
            y: top,
        },
        max: TilePosition {
            x: rx * CAVERN_REGION.0 + CAVERN_REGION.0 - 1,
            y: bottom,
        },
    };
    let mut window = Window::new(ctx, &rect);
    // The open area is traced on a lattice of every other tile each way (a quarter of the
    // tiles; caverns are far wider than two tiles), each sample standing for four tiles.
    let step = CAVERN_SAMPLE_STEP;
    let columns = (rect.width() + step - 1) / step;
    let rows = (rect.height() + step - 1) / step;
    let cells = usize::try_from(columns * rows).unwrap_or(0);
    let mut seen = vec![false; cells];
    let index = |cx: i32, cy: i32| usize::try_from(cy * columns + cx).unwrap_or(0);
    let tile = |cx: i32, cy: i32| (rect.min.x + cx * step, rect.min.y + cy * step);
    let mut best: Option<(usize, i64, i64, TileRect)> = None;
    for start_y in 0..rows {
        for start_x in 0..columns {
            let (x, y) = tile(start_x, start_y);
            if seen[index(start_x, start_y)] || !window.is_roughly_open(x, y) {
                continue;
            }
            seen[index(start_x, start_y)] = true;
            let mut queue = vec![(start_x, start_y)];
            let mut next = 0;
            let (mut sum_x, mut sum_y) = (0_i64, 0_i64);
            let mut bounds = TileRect {
                min: TilePosition { x, y },
                max: TilePosition { x, y },
            };
            while next < queue.len() {
                let (cx, cy) = queue[next];
                next += 1;
                let (x, y) = tile(cx, cy);
                sum_x += i64::from(x);
                sum_y += i64::from(y);
                bounds.min.x = bounds.min.x.min(x);
                bounds.min.y = bounds.min.y.min(y);
                bounds.max.x = bounds.max.x.max(x);
                bounds.max.y = bounds.max.y.max(y);
                for (nx, ny) in [(cx + 1, cy), (cx - 1, cy), (cx, cy + 1), (cx, cy - 1)] {
                    if nx < 0 || ny < 0 || nx >= columns || ny >= rows || seen[index(nx, ny)] {
                        continue;
                    }
                    let (tx, ty) = tile(nx, ny);
                    if window.is_roughly_open(tx, ty) {
                        seen[index(nx, ny)] = true;
                        queue.push((nx, ny));
                    }
                }
            }
            if best.as_ref().is_none_or(|(count, ..)| queue.len() > *count) {
                best = Some((queue.len(), sum_x, sum_y, bounds));
            }
        }
    }
    let Some((samples, sum_x, sum_y, bounds)) = best else {
        return CavernRegion::default();
    };
    let count = samples * usize::try_from(step * step).unwrap_or(1);
    if count < caves::LARGE_CAVERN_TILES {
        return CavernRegion::default();
    }
    let count_i64 = i64::try_from(samples).unwrap_or(1);
    let center = TilePosition {
        x: i32::try_from(sum_x / count_i64).unwrap_or(bounds.min.x),
        y: i32::try_from(sum_y / count_i64).unwrap_or(bounds.min.y),
    };
    CavernRegion {
        feature: Some(WorldFeature {
            id: feature_id(Layer::Cavern, rx, ry, 0),
            kind: FeatureKind::LargeCavern,
            detail: FeatureDetail::None,
            band: ctx.band(center.x, center.y),
            center,
            bounds,
            size: u32::try_from(count).unwrap_or(u32::MAX),
        }),
    }
}

// ---------------------------------------------------------------------------------------------
// Ore and artifact candidates (applied where plain rock is left)

/// Candidate tiles of one deposit layer region, and its feature (if it is notable).
#[derive(Debug)]
pub struct Candidates<K> {
    pub tiles: Vec<(i32, i32, K)>,
    pub feature: Option<WorldFeature>,
}

impl<K> Default for Candidates<K> {
    fn default() -> Self {
        Self {
            tiles: Vec::new(),
            feature: None,
        }
    }
}

static MOTHERLODES: MemoCache<Candidates<MineralKind>> = MemoCache::new(4_096);
static CLUSTERS: MemoCache<Candidates<MineralKind>> = MemoCache::new(16_384);
static BEDS: MemoCache<Candidates<ArtifactKind>> = MemoCache::new(4_096);
static STARTER: MemoCache<Candidates<MineralKind>> = MemoCache::new(16);

/// Whether deep ore comes in fewer, chunkier deposits (rich worlds, as in V3/V4).
const fn chunky_deep_ore(ctx: &Ctx) -> bool {
    matches!(
        ctx.settings.richness,
        crate::worldgen::ResourceRichness::Rich
    )
}

pub fn motherlode_region(ctx: &Ctx, rx: i32, ry: i32) -> Arc<Candidates<MineralKind>> {
    MOTHERLODES.get_or_compute((ctx.settings, rx, ry), || compute_motherlode(ctx, rx, ry))
}

fn compute_motherlode(ctx: &Ctx, rx: i32, ry: i32) -> Candidates<MineralKind> {
    let mut region = Candidates::default();
    if region_rows(ctx, ry, MOTHERLODE_REGION.1).is_none() {
        return region;
    }
    let rich = ctx.settings.richness.percent();
    let mut rng = RegionRng::new(ctx.seed(), deposits::SALT_MOTHERLODE, rx, ry);
    let x = rx * MOTHERLODE_REGION.0 + rng.range(6, MOTHERLODE_REGION.0 - 6);
    let y = ry * MOTHERLODE_REGION.1 + rng.range(5, MOTHERLODE_REGION.1 - 5);
    if ctx.nominal_row(y) < 34 || y >= ctx.floor_row(x) - 4 {
        return region;
    }
    if !rng.chance(260 * rich / 100) {
        return region;
    }
    let band = ctx.band(x, y);
    let weights = ctx.mineral_weights(band, x, y);
    let Some(mineral) = MINERALS
        .iter()
        .zip(weights)
        .filter(|(_, weight)| *weight >= 8)
        .map(|(mineral, _)| *mineral)
        .next_back()
    else {
        return region;
    };
    let (half_width, half_height) = (rng.range(3, 5), rng.range(2, 4));
    for dy in -half_height..=half_height {
        for dx in -half_width..=half_width {
            let inside = dx * dx * half_height * half_height + dy * dy * half_width * half_width
                <= half_width * half_width * half_height * half_height;
            let (tx, ty) = (x + dx, y + dy);
            if inside && rng.chance(820) && mineral_fits(ctx.band(tx, ty), mineral) {
                region.tiles.push((tx, ty, mineral));
            }
        }
    }
    if region.tiles.len() >= 6 {
        let center = TilePosition { x, y };
        region.feature = Some(WorldFeature {
            id: feature_id(Layer::Motherlode, rx, ry, 0),
            kind: FeatureKind::Motherlode,
            detail: FeatureDetail::Mineral(mineral),
            band,
            center,
            bounds: TileRect::around(center, half_width, half_height),
            size: u32::try_from(region.tiles.len()).unwrap_or(u32::MAX),
        });
    }
    region
}

pub fn cluster_region(ctx: &Ctx, rx: i32, ry: i32) -> Arc<Candidates<MineralKind>> {
    CLUSTERS.get_or_compute((ctx.settings, rx, ry), || compute_clusters(ctx, rx, ry))
}

fn compute_clusters(ctx: &Ctx, rx: i32, ry: i32) -> Candidates<MineralKind> {
    let mut region = Candidates::default();
    if region_rows(ctx, ry, CLUSTER_REGION.1).is_none() {
        return region;
    }
    let mut rng = RegionRng::new(ctx.seed(), deposits::SALT_CLUSTER, rx, ry);
    let origin = (rx * CLUSTER_REGION.0, ry * CLUSTER_REGION.1);
    let centre = (
        origin.0 + CLUSTER_REGION.0 / 2,
        (origin.1 + CLUSTER_REGION.1 / 2).min(ctx.height - 1),
    );
    let band = ctx.band(centre.0, centre.1);
    // The world's richness (V11's registry strata scale it by their ore density, Wave 8).
    let rich = u32::try_from(ctx.ore_richness(centre.0, centre.1)).unwrap_or(0);
    let mut expected = deposits::cluster_density(band) * rich / 100;
    if chunky_deep_ore(ctx) && band.deep_stratum().is_some() {
        expected = expected * 3 / 4;
    }
    while expected > 0 {
        if expected >= 1000 || rng.chance(expected) {
            let x = origin.0 + rng.range(0, CLUSTER_REGION.0 - 1);
            let y = origin.1 + rng.range(0, CLUSTER_REGION.1 - 1);
            cluster(ctx, &mut rng, x, y, None, &mut region.tiles);
        }
        expected = expected.saturating_sub(1000);
    }
    region
}

/// One cluster pod's candidate tiles (the V4 pod shapes).
fn cluster(
    ctx: &Ctx,
    rng: &mut RegionRng,
    x: i32,
    y: i32,
    mineral: Option<MineralKind>,
    tiles: &mut Vec<(i32, i32, MineralKind)>,
) {
    if y <= SKY_ROWS || y >= ctx.height {
        return;
    }
    let band = ctx.band(x, y);
    let mineral = match mineral {
        Some(mineral) => mineral,
        None => match rng.pick(&ctx.mineral_weights(band, x, y)) {
            Some(index) => MINERALS[index],
            None => return,
        },
    };
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
    let (radius_x, radius_y) = if chunky_deep_ore(ctx) && band.deep_stratum().is_some() {
        (radius_x.max(2), radius_y.max(1) + i32::from(radius_x <= 1))
    } else {
        (radius_x, radius_y)
    };
    let edge_salt = rng.next_u64();
    let (rx2, ry2) = (radius_x * radius_x, (radius_y * radius_y).max(1));
    for dy in -radius_y..=radius_y {
        for dx in -radius_x..=radius_x {
            let (tx, ty) = (x + dx, y + dy);
            let distance = dx * dx * ry2 + dy * dy * rx2;
            let ragged = value_noise(ctx.seed(), edge_salt, tx, ty, 2, 2);
            if distance * NOISE_MAX > rx2 * ry2 * (NOISE_MAX / 2 + ragged * 3 / 4) {
                continue;
            }
            if ty > SKY_ROWS && ty < ctx.height && mineral_fits(ctx.band(tx, ty), mineral) {
                tiles.push((tx, ty, mineral));
            }
        }
    }
}

pub fn bed_region(ctx: &Ctx, rx: i32, ry: i32) -> Arc<Candidates<ArtifactKind>> {
    BEDS.get_or_compute((ctx.settings, rx, ry), || compute_bed(ctx, rx, ry))
}

fn compute_bed(ctx: &Ctx, rx: i32, ry: i32) -> Candidates<ArtifactKind> {
    let mut region = Candidates::default();
    if region_rows(ctx, ry, BED_REGION.1).is_none() {
        return region;
    }
    let mut rng = RegionRng::new(ctx.seed(), deposits::SALT_ARTIFACT_BED ^ V5_SALT, rx, ry);
    if !rng.chance(BED_CHANCE) {
        return region;
    }
    let x = rx * BED_REGION.0 + rng.range(BED_REACH, BED_REGION.0 - 1 - BED_REACH);
    let y = ry * BED_REGION.1 + rng.range(2, BED_REGION.1 - 3);
    if y >= ctx.height - 3 || y >= ctx.floor_row(x) - 2 {
        return region;
    }
    let band = ctx.band(x, y);
    if band == Band::Topsoil || !ctx.base_kind(x, y).is_some_and(is_plain_rock) {
        return region;
    }
    if (x - SPAWN_TILE_X).abs() < 12 && y < 30 {
        return region;
    }
    let count = rng.range(3, 5);
    let mut kind_seen = None;
    for _ in 0..count * 4 {
        if region.tiles.len() >= usize::try_from(count).unwrap_or_default() {
            break;
        }
        let (tx, ty) = (x + rng.range(-3, 3), y + rng.range(-2, 2));
        if ctx.band(tx, ty) != band || region.tiles.iter().any(|t| (t.0, t.1) == (tx, ty)) {
            continue;
        }
        let Some(artifact) = deposits::pick_artifact(band, &mut rng) else {
            break;
        };
        kind_seen.get_or_insert(artifact);
        region.tiles.push((tx, ty, artifact));
    }
    if region.tiles.len() >= 2
        && let Some(artifact) = kind_seen
    {
        let center = TilePosition { x, y };
        region.feature = Some(WorldFeature {
            id: feature_id(Layer::Bed, rx, ry, 0),
            kind: FeatureKind::ArtifactBed,
            detail: FeatureDetail::Artifact(artifact),
            band,
            center,
            bounds: TileRect::around(center, 3, 2),
            size: u32::try_from(region.tiles.len()).unwrap_or(u32::MAX),
        });
    }
    region
}

/// The starter field's copper and iron candidates near spawn: enough pods that what lands on
/// plain rock always pays for the first upgrades.
pub fn starter_field(ctx: &Ctx) -> Arc<Candidates<MineralKind>> {
    STARTER.get_or_compute((ctx.settings, 0, 0), || {
        let mut region = Candidates::default();
        let mut rng = RegionRng::new(ctx.seed(), deposits::SALT_STARTER ^ V5_SALT, 0, 0);
        let target = deposits::STARTER_FIELD_VALUE * STARTER_CANDIDATE_PERCENT / 100;
        let mut value = 0_u64;
        let mut attempts = 0;
        while value < target && attempts < 400 {
            attempts += 1;
            let x = SPAWN_TILE_X
                + rng.range(
                    -deposits::STARTER_FIELD_HALF_WIDTH,
                    deposits::STARTER_FIELD_HALF_WIDTH,
                );
            let y = rng.range(SKY_ROWS + 2, deposits::STARTER_FIELD_LAST_ROW);
            let mineral = if rng.chance(700) || y < 9 {
                MineralKind::Copper
            } else {
                MineralKind::Iron
            };
            let before = region.tiles.len();
            cluster(ctx, &mut rng, x, y, Some(mineral), &mut region.tiles);
            // Only count tiles inside the field that no earlier pod claimed.
            let fresh: Vec<(i32, i32, MineralKind)> = region.tiles.drain(before..).collect();
            for tile in fresh {
                if region.tiles.iter().any(|t| (t.0, t.1) == (tile.0, tile.1)) {
                    continue;
                }
                if tile.1 <= deposits::STARTER_FIELD_LAST_ROW
                    && (tile.0 - SPAWN_TILE_X).abs() <= deposits::STARTER_FIELD_HALF_WIDTH
                {
                    value += u64::from(tile.2.value());
                }
                region.tiles.push(tile);
            }
        }
        region
    })
}

/// Columns and rows the starter field's candidates can touch.
pub const STARTER_AREA: TileRect = TileRect {
    min: TilePosition {
        x: SPAWN_TILE_X - deposits::STARTER_FIELD_HALF_WIDTH - 4,
        y: 0,
    },
    max: TilePosition {
        x: SPAWN_TILE_X + deposits::STARTER_FIELD_HALF_WIDTH + 4,
        y: deposits::STARTER_FIELD_LAST_ROW + 4,
    },
};

/// Whether a stray artifact sits at `(x, y)` (when that tile is still plain rock), and which.
pub fn stray_artifact(ctx: &Ctx, band: Band, x: i32, y: i32) -> Option<ArtifactKind> {
    let rich = ctx.settings.richness.percent();
    if !crate::worldgen::noise::chance(
        ctx.seed(),
        deposits::SALT_ARTIFACT_STRAY,
        x,
        y,
        2 * rich / 100 + 1,
    ) {
        return None;
    }
    let mut rng = RegionRng::new(ctx.seed(), deposits::SALT_ARTIFACT_STRAY ^ 0xFF, x, y);
    deposits::pick_artifact(band, &mut rng)
}

/// Forgets every memoized region (tests).
#[cfg(test)]
pub fn clear_caches() {
    WORMS.clear();
    CHAMBERS.clear();
    POIS.clear();
    HAZARDS.clear();
    CAVERNS.clear();
    MOTHERLODES.clear();
    CLUSTERS.clear();
    BEDS.clear();
    STARTER.clear();
}
