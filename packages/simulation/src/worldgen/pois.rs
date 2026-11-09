//! Points of interest: prefab rooms stamped into a region grid ([`GeneratorVersion::V2`] and
//! later).
//!
//! Every band is split into columns of about [`REGION_WIDTH`] tiles; each region of each band
//! is one candidate slot. A band first places its guaranteed kinds ([`guaranteed_kinds`]) in
//! shuffled regions, then fills some of the remaining regions at random with kinds whose depth
//! window ([`PoiKind::band_window`]) includes the band. Regions left empty become reserved
//! [`FeatureKind::PrefabSlot`]s for later systems (story sites, quests).
//!
//! A prefab only lands where it fits: inside the world, its centre in the band, clear of the
//! spawn shaft and the town, [`POI_SPACING`] tiles from every other POI, on mostly solid ground
//! (never floating in a cavern), never over the town's foundations and with no hazard within
//! [`HAZARD_CLEARANCE`] tiles, so a POI never sits in a hazard pocket; the only hazards near a
//! POI are a formation's own guardians. POIs are placed before deposits, which then flow
//! around them (and count their ore toward each band's budget).
//!
//! [`GeneratorVersion::V2`]: super::GeneratorVersion::V2

use crate::{
    poi::{LORE, LoreId, PoiKind, PoiTile, PoiVariant},
    terrain::{TileKind, TilePosition},
};

use super::{
    Canvas, SPAWN_TILE_X,
    caves::{CRUST_ROWS, SPAWN_SHAFT_HALF_WIDTH, SPAWN_SHAFT_ROWS},
    deposits,
    features::{FeatureDetail, FeatureKind, HazardKind, TileRect},
    hazards::hazard_allowed,
    noise::RegionRng,
    prefabs::{Cell, Prefab, PrefabRole, prefabs_of},
    strata::{Band, nominal_band_top},
    town_columns,
};

/// Approximate width of one placement region.
pub const REGION_WIDTH: i32 = 48;
/// Smallest gap, in tiles, between two POIs.
pub const POI_SPACING: i32 = 6;
/// No hazard may lie within this many tiles of a POI's bounds.
pub const HAZARD_CLEARANCE: i32 = 2;
/// Columns either side of the spawn column no POI may touch above
/// [`SPAWN_CLEAR_ROWS`], and no vault may touch at any depth.
pub const SPAWN_CLEAR_HALF_WIDTH: i32 = SPAWN_SHAFT_HALF_WIDTH + 4;
/// Depth of the spawn clearance.
pub const SPAWN_CLEAR_ROWS: i32 = SPAWN_SHAFT_ROWS + 12;
/// Columns either side of the spawn column a vault (undiggable walls) may never touch.
pub const VAULT_SPAWN_CLEAR_HALF_WIDTH: i32 = 4;
/// Shallowest row any POI may reach.
pub const POI_MIN_ROW: i32 = CRUST_ROWS;
/// Chance (per mille) that a region left after the guaranteed kinds gets a random POI.
const RANDOM_FILL_PER_MILLE: u32 = 250;

/// Number of derelict logs (fits a `u16`).
#[allow(clippy::cast_possible_truncation, reason = "a dozen logs")]
const LORE_COUNT: u16 = LORE.len() as u16;

const SALT_POI: u64 = 0x9017_1000;
const SALT_SPOT: u64 = 0x9017_1001;
const SALT_FILL: u64 = 0x9017_1002;

/// Kinds every band always tries to place (each in its own region).
#[must_use]
pub const fn guaranteed_kinds(band: Band) -> &'static [PoiKind] {
    use PoiKind::{DerelictRig, Formation, RivalClaim, SupplyCache, Vault};
    match band.index() {
        0 => &[],
        1 => &[SupplyCache],
        2 => &[SupplyCache, DerelictRig],
        3 => &[SupplyCache, Formation],
        4 => &[SupplyCache, DerelictRig, RivalClaim],
        5 | 7 | 9 => &[Vault, Formation, SupplyCache],
        6 => &[Vault, DerelictRig, SupplyCache],
        8 => &[Vault, DerelictRig, RivalClaim],
        10 => &[Vault, DerelictRig, Formation],
        _ => &[SupplyCache, Vault, Formation],
    }
}

/// Relative weight of each kind (in [`PoiKind::ALL`] order) for random fills in `band`.
const fn fill_weights(band: Band) -> [u32; 5] {
    let mut weights = [6, 3, 1, 3, 2];
    let mut index = 0;
    while index < weights.len() {
        if !PoiKind::ALL[index].fits_band(band) {
            weights[index] = 0;
        }
        index += 1;
    }
    weights
}

/// The hazard that guards a formation in `band`.
#[must_use]
pub const fn guardian_hazard(band: Band) -> TileKind {
    match band {
        Band::Deep(stratum) => stratum.primary_hazard(),
        Band::Basalt => TileKind::MagmaVent,
        _ => TileKind::Gas,
    }
}

/// Rows `band` nominally spans (first, last).
const fn band_rows(canvas: &Canvas<'_>, band: Band) -> (i32, i32) {
    let top = nominal_band_top(band);
    let bottom = if band.index() + 1 < Band::ALL.len() {
        nominal_band_top(Band::from_index(band.index() + 1)) - 1
    } else {
        canvas.height - 1
    };
    (top, bottom)
}

/// Whether `rect` keeps clear of spawn and the town.
#[must_use]
pub const fn clear_of_spawn_and_town(rect: &TileRect, kind: PoiKind) -> bool {
    let (town_first, town_last) = town_columns();
    let near_town = rect.min.y < SPAWN_CLEAR_ROWS
        && rect.max.x >= town_first - 6
        && rect.min.x <= town_last + 6;
    let near_spawn = rect.min.y < SPAWN_CLEAR_ROWS
        && rect.max.x >= SPAWN_TILE_X - SPAWN_CLEAR_HALF_WIDTH
        && rect.min.x <= SPAWN_TILE_X + SPAWN_CLEAR_HALF_WIDTH;
    let vault_over_shaft = matches!(kind, PoiKind::Vault)
        && rect.max.x >= SPAWN_TILE_X - VAULT_SPAWN_CLEAR_HALF_WIDTH
        && rect.min.x <= SPAWN_TILE_X + VAULT_SPAWN_CLEAR_HALF_WIDTH;
    !near_town && !near_spawn && !vault_over_shaft
}

/// One placement of a prefab.
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

    /// The prefab cell stamped at world `(x, y)`.
    fn cell_at(&self, x: i32, y: i32) -> Cell {
        let local_x = x - self.origin.x;
        let local_y = y - self.origin.y;
        let column = if self.flipped {
            self.prefab.width() - 1 - local_x
        } else {
            local_x
        };
        self.prefab.cell(column, local_y)
    }
}

/// Places every POI of a V2+ world and reserves the empty regions as prefab slots.
pub(crate) fn place(canvas: &mut Canvas<'_>) {
    let seed = canvas.seed();
    let regions = (canvas.width / REGION_WIDTH).max(1);
    let region_width = canvas.width / regions;
    let mut placed: Vec<TileRect> = Vec::new();
    let mut derelicts: u16 = 0;
    let lore_offset = u16::try_from(seed % u64::from(LORE_COUNT)).unwrap_or(0);
    for band in Band::ALL {
        let band_index = i32::try_from(band.index()).unwrap_or(0);
        let mut rng = RegionRng::new(seed, SALT_POI, band_index, 0);
        let mut order: Vec<i32> = (0..regions).collect();
        for index in (1..order.len()).rev() {
            let other =
                usize::try_from(rng.range(0, i32::try_from(index).unwrap_or(0))).unwrap_or(0);
            order.swap(index, other);
        }
        let mut pending: Vec<PoiKind> = guaranteed_kinds(band).to_vec();
        for region in order {
            let x_range = (region * region_width, (region + 1) * region_width - 1);
            let mut fill_rng = RegionRng::new(seed, SALT_FILL, band_index, region);
            let forced = !pending.is_empty();
            let kind = if forced {
                Some(pending.remove(0))
            } else if fill_rng.chance(RANDOM_FILL_PER_MILLE) {
                fill_rng
                    .pick(&fill_weights(band))
                    .map(|index| PoiKind::ALL[index])
            } else {
                None
            };
            let spot = kind.and_then(|kind| {
                let mut spot_rng = RegionRng::new(seed, SALT_SPOT, band_index, region);
                find_spot(canvas, band, kind, x_range, &placed, &mut spot_rng)
            });
            match (kind, spot) {
                (Some(_), Some(spot)) => {
                    let mut stamp_rng = RegionRng::new(seed, SALT_SPOT ^ 0xFF, band_index, region);
                    let variant = match spot.prefab.role {
                        PrefabRole::SupplyCache => PoiVariant::SupplyCache,
                        PrefabRole::DerelictRig => {
                            let log = LoreId((derelicts + lore_offset) % LORE_COUNT);
                            derelicts += 1;
                            PoiVariant::DerelictRig { log }
                        }
                        PrefabRole::Vault(puzzle) => PoiVariant::Vault(puzzle),
                        PrefabRole::Formation(style) => PoiVariant::Formation(style),
                        PrefabRole::RivalClaim => PoiVariant::RivalClaim,
                    };
                    let size = stamp(canvas, band, &spot, &mut stamp_rng);
                    let bounds = spot.bounds();
                    placed.push(bounds);
                    canvas.features.push(
                        FeatureKind::PointOfInterest,
                        FeatureDetail::Poi(variant),
                        band,
                        bounds.center(),
                        bounds,
                        size,
                    );
                }
                (kind, None) => {
                    if forced && let Some(kind) = kind {
                        // Try the guaranteed kind again in the next region.
                        pending.insert(0, kind);
                    }
                    reserve_slot(canvas, band, x_range, &placed);
                }
                (None, Some(_)) => {}
            }
        }
    }
}

/// Picks a prefab of `kind` and a spot for it in the region's columns `x_range` of `band`.
fn find_spot(
    canvas: &Canvas<'_>,
    band: Band,
    kind: PoiKind,
    x_range: (i32, i32),
    placed: &[TileRect],
    rng: &mut RegionRng,
) -> Option<Spot> {
    let candidates: Vec<&'static Prefab> = prefabs_of(kind)
        .filter(|prefab| match prefab.role {
            PrefabRole::Formation(style) => style == crate::poi::FormationStyle::for_band(band),
            _ => true,
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let (top, bottom) = band_rows(canvas, band);
    for _ in 0..40 {
        let prefab = candidates[usize::try_from(
            rng.range(0, i32::try_from(candidates.len()).unwrap_or(1) - 1),
        )
        .unwrap_or(0)];
        let (width, height) = (prefab.width(), prefab.height());
        let x = rng.range(x_range.0 + 1, x_range.1 - width - 1);
        let y_low = (top - height / 3).max(POI_MIN_ROW);
        let y_high = (bottom - height * 2 / 3).min(canvas.height - height - 1);
        if y_high < y_low {
            continue;
        }
        let y = rng.range(y_low, y_high);
        let spot = Spot {
            prefab,
            origin: TilePosition { x, y },
            flipped: rng.chance(500),
        };
        if fits(canvas, band, kind, &spot, placed) {
            return Some(spot);
        }
    }
    None
}

/// Whether `spot` is a legal placement.
fn fits(canvas: &Canvas<'_>, band: Band, kind: PoiKind, spot: &Spot, placed: &[TileRect]) -> bool {
    let bounds = spot.bounds();
    if bounds.min.x < 2
        || bounds.max.x > canvas.width - 3
        || bounds.min.y < POI_MIN_ROW
        || bounds.max.y > canvas.height - 2
    {
        return false;
    }
    let center = bounds.center();
    if canvas.band(center.x, center.y) != band || !clear_of_spawn_and_town(&bounds, kind) {
        return false;
    }
    if placed
        .iter()
        .any(|other| other.expanded(POI_SPACING).intersects(&bounds))
    {
        return false;
    }
    if bounds
        .expanded(HAZARD_CLEARANCE)
        .positions()
        .any(|position| {
            canvas
                .get(position.x, position.y)
                .is_some_and(|kind| HazardKind::of_tile(kind).is_some())
        })
    {
        return false;
    }
    let mut open = 0;
    let mut total = 0;
    for position in bounds.positions() {
        match canvas.get(position.x, position.y) {
            Some(TileKind::Foundation) | None => return false,
            Some(TileKind::Air) => open += 1,
            _ => {}
        }
        total += 1;
    }
    // Never float in a cavern: at most a third of the footprint may already be open.
    open * 3 <= total
}

/// Reserves an untouched spot in the region for later prefabs (when it is clear of spawn and
/// the other POIs).
fn reserve_slot(canvas: &mut Canvas<'_>, band: Band, x_range: (i32, i32), placed: &[TileRect]) {
    let (top, bottom) = band_rows(canvas, band);
    let center = TilePosition {
        x: i32::midpoint(x_range.0, x_range.1),
        y: i32::midpoint(top, bottom).clamp(POI_MIN_ROW + 3, canvas.height - 5),
    };
    let bounds = TileRect::around(center, 5, 3);
    if !clear_of_spawn_and_town(&bounds, PoiKind::SupplyCache)
        || placed
            .iter()
            .any(|other| other.expanded(POI_SPACING).intersects(&bounds))
    {
        return;
    }
    canvas.features.push(
        FeatureKind::PrefabSlot,
        FeatureDetail::None,
        canvas.band(center.x, center.y),
        center,
        bounds,
        0,
    );
}

/// Stamps `spot` into the canvas. Returns how many tiles it changed.
fn stamp(canvas: &mut Canvas<'_>, band: Band, spot: &Spot, rng: &mut RegionRng) -> u32 {
    let bounds = spot.bounds();
    // Shuffle which switch slot is which order (1..=3).
    let mut orders = [1_u8, 2, 3];
    for index in (1..orders.len()).rev() {
        let other = usize::try_from(rng.range(0, i32::try_from(index).unwrap_or(0))).unwrap_or(0);
        orders.swap(index, other);
    }
    let matrix = match spot.prefab.role {
        PrefabRole::Formation(crate::poi::FormationStyle::Geode) => PoiTile::GeodeShell,
        _ => PoiTile::FossilShale,
    };
    let mut changed = 0;
    for position in bounds.positions() {
        let (x, y) = (position.x, position.y);
        let host = canvas.band(x, y).soft_material();
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
            Cell::Ore => formation_ore(band, y, rng).unwrap_or(TileKind::Poi(matrix)),
            Cell::OreOrMatrix => {
                if rng.chance(500) {
                    formation_ore(band, y, rng).unwrap_or(TileKind::Poi(matrix))
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
        canvas.set(x, y, kind);
        changed += 1;
    }
    changed
}

/// Rich ore for a formation: the band's two most valuable minerals.
fn formation_ore(band: Band, y: i32, rng: &mut RegionRng) -> Option<TileKind> {
    let weights = deposits::mineral_weights(band, y);
    let mut best: Vec<usize> = (0..weights.len())
        .filter(|index| weights[*index] > 0)
        .collect();
    best.sort_by_key(|index| core::cmp::Reverse(*index));
    best.truncate(2);
    if best.is_empty() {
        return None;
    }
    let pick = best
        [usize::try_from(rng.range(0, i32::try_from(best.len()).unwrap_or(1) - 1)).unwrap_or(0)];
    Some(TileKind::Ore(deposits::MINERALS[pick]))
}
