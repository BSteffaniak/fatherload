//! Fairness suite for points of interest: counts per band, spacing, clearance from spawn, the
//! town and hazards, and that every vault's lock can be reached and worked.

use std::collections::VecDeque;

use super::{
    pois::{HAZARD_CLEARANCE, POI_SPACING, VAULT_SPAWN_CLEAR_HALF_WIDTH, clear_of_spawn_and_town},
    *,
};
use crate::{
    poi::{PoiKind, PoiTile, PoiVariant, VaultPuzzle},
    terrain::{TileKind, TilePosition},
};

fn sample_seeds() -> impl Iterator<Item = u64> {
    let count = std::env::var("DRILLGAME_WORLDGEN_SEEDS")
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(32);
    (0..count).map(|index| index.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x9015)
}

fn tile_at(world: &GeneratedWorld, x: i32, y: i32) -> Option<TileKind> {
    (x >= 0 && y >= 0 && x < world.width && y < world.height)
        .then(|| world.tiles[usize::try_from(y * world.width + x).unwrap_or(0)].kind)
}

/// Tiles a rig can eventually get through: open air or anything a drill digs (never hazards,
/// foundations or vault fixtures).
fn passable(world: &GeneratedWorld, x: i32, y: i32) -> bool {
    match tile_at(world, x, y) {
        None | Some(TileKind::Foundation) => false,
        Some(TileKind::Poi(tile)) => tile.hardness() != crate::poi::UNDRILLABLE,
        Some(kind) => HazardKind::of_tile(kind).is_none(),
    }
}

/// Every tile reachable from spawn through passable tiles.
fn reachable_from_spawn(world: &GeneratedWorld) -> Vec<bool> {
    let mut seen = vec![false; world.tiles.len()];
    let index = |x: i32, y: i32| usize::try_from(y * world.width + x).unwrap_or(0);
    let start = (SPAWN_TILE_X, strata::SKY_ROWS - 1);
    seen[index(start.0, start.1)] = true;
    let mut queue = VecDeque::from([start]);
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= world.width || ny >= world.height {
                continue;
            }
            if !seen[index(nx, ny)] && passable(world, nx, ny) {
                seen[index(nx, ny)] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    seen
}

#[test]
fn poi_counts_per_band_stay_in_bounds() {
    for seed in sample_seeds() {
        for size in WorldSize::ALL {
            let settings = WorldSettings {
                size,
                ..WorldSettings::new(seed)
            };
            let world = generate(&settings);
            let scale = usize::try_from(settings.width_tiles() / 120).unwrap_or(2);
            let total = world.features.pois().count();
            assert!(
                (6 * scale + 12..=24 * scale).contains(&total),
                "seed {seed} {size:?}: {total} POIs"
            );
            for band in Band::ALL.into_iter().skip(1) {
                let count = world.features.pois_in_band(band).count();
                assert!(
                    (1..=3 * scale).contains(&count),
                    "seed {seed} {size:?} {band:?}: {count} POIs"
                );
                for poi in world.features.pois_in_band(band) {
                    let kind = poi.poi_kind().expect("a POI");
                    assert!(kind.fits_band(band), "seed {seed}: {kind:?} in {band:?}");
                }
            }
            // Caches show up early: every band from the clay to the slate has one.
            for band in [Band::Clay, Band::Stone, Band::Slate] {
                assert!(
                    world
                        .features
                        .pois_in_band(band)
                        .any(|poi| poi.poi_kind() == Some(PoiKind::SupplyCache)),
                    "seed {seed} {size:?}: no cache in {band:?}"
                );
            }
            for kind in PoiKind::ALL {
                assert!(
                    world.features.pois_of(kind).count() >= 2,
                    "seed {seed} {size:?}: fewer than two {kind:?}"
                );
            }
            assert!(world.features.pois_of(PoiKind::Vault).count() >= 4);
        }
    }
}

#[test]
fn v1_worlds_have_no_points_of_interest() {
    let world = generate(&WorldSettings {
        generator: GeneratorVersion::V1,
        ..WorldSettings::new(3)
    });
    assert_eq!(world.features.pois().count(), 0);
    assert!(
        !world
            .tiles
            .iter()
            .any(|tile| matches!(tile.kind, TileKind::Poi(_)))
    );
}

#[test]
fn pois_are_spaced_and_clear_of_spawn_the_town_and_hazards() {
    for seed in sample_seeds() {
        let world = generate(&WorldSettings::new(seed));
        let pois: Vec<_> = world.features.pois().copied().collect();
        for (index, poi) in pois.iter().enumerate() {
            let kind = poi.poi_kind().expect("a POI");
            assert!(
                clear_of_spawn_and_town(&poi.bounds, kind),
                "seed {seed}: {kind:?} at {:?} near spawn or town",
                poi.bounds
            );
            for other in &pois[index + 1..] {
                assert!(
                    !poi.bounds
                        .expanded(POI_SPACING - 1)
                        .intersects(&other.bounds),
                    "seed {seed}: POIs {:?} and {:?} crowd each other",
                    poi.bounds,
                    other.bounds
                );
            }
            // No hazard near a POI except a formation's guardians (its band's style).
            let guardian = pois::guardian_hazard(poi.band);
            for position in poi.bounds.expanded(HAZARD_CLEARANCE).positions() {
                let Some(found) = tile_at(&world, position.x, position.y) else {
                    continue;
                };
                if HazardKind::of_tile(found).is_none() {
                    continue;
                }
                assert!(
                    kind == PoiKind::Formation && found == guardian,
                    "seed {seed}: {found:?} at {position:?} by {kind:?} {:?}",
                    poi.bounds
                );
            }
        }
        // Nothing undiggable ever sits straight below spawn.
        for y in strata::SKY_ROWS..world.height {
            for x in SPAWN_TILE_X - VAULT_SPAWN_CLEAR_HALF_WIDTH
                ..=SPAWN_TILE_X + VAULT_SPAWN_CLEAR_HALF_WIDTH
            {
                if let Some(TileKind::Poi(tile)) = tile_at(&world, x, y) {
                    assert!(
                        !tile.is_vault_fixture(),
                        "seed {seed}: {tile:?} at ({x}, {y})"
                    );
                }
            }
        }
    }
}

#[test]
fn every_vault_lock_can_be_reached_and_worked() {
    for seed in sample_seeds() {
        let world = generate(&WorldSettings::new(seed));
        let reachable = reachable_from_spawn(&world);
        let reached = |x: i32, y: i32| {
            x >= 0
                && y >= 0
                && x < world.width
                && y < world.height
                && reachable[usize::try_from(y * world.width + x).unwrap_or(0)]
        };
        for vault in world.features.pois_of(PoiKind::Vault) {
            let Some(PoiVariant::Vault(puzzle)) = vault.poi() else {
                panic!("vault detail");
            };
            let tiles: Vec<(TilePosition, PoiTile)> = vault
                .bounds
                .positions()
                .filter_map(|position| match tile_at(&world, position.x, position.y) {
                    Some(TileKind::Poi(tile)) => Some((position, tile)),
                    _ => None,
                })
                .collect();
            let count = |wanted: fn(PoiTile) -> bool| {
                tiles.iter().filter(|(_, tile)| wanted(*tile)).count()
            };
            assert!(
                count(|tile| tile == PoiTile::VaultCache) >= 1,
                "seed {seed}"
            );
            // Every door can be walked up to from outside once it opens.
            for (door, _) in tiles.iter().filter(|(_, tile)| *tile == PoiTile::VaultDoor) {
                assert!(
                    reached(door.x - 1, door.y) || reached(door.x + 1, door.y),
                    "seed {seed}: door {door:?} of {puzzle:?} sealed off"
                );
            }
            match puzzle {
                VaultPuzzle::Sequence | VaultPuzzle::Relay => {
                    let lock: Vec<TilePosition> = tiles
                        .iter()
                        .filter(|(_, tile)| {
                            matches!(
                                tile,
                                PoiTile::VaultSwitch { .. } | PoiTile::VaultRelay { .. }
                            )
                        })
                        .map(|(position, _)| *position)
                        .collect();
                    assert_eq!(
                        lock.len(),
                        if puzzle == VaultPuzzle::Sequence {
                            3
                        } else {
                            2
                        },
                        "seed {seed}"
                    );
                    if puzzle == VaultPuzzle::Sequence {
                        let mut orders: Vec<u8> = tiles
                            .iter()
                            .filter_map(|(_, tile)| match tile {
                                PoiTile::VaultSwitch { order, .. } => Some(*order),
                                _ => None,
                            })
                            .collect();
                        orders.sort_unstable();
                        assert_eq!(orders, [1, 2, 3], "seed {seed}");
                    }
                    // Drilled from above: the tile over each switch or relay is reachable.
                    for switch in lock {
                        assert!(
                            reached(switch.x, switch.y - 1),
                            "seed {seed}: {puzzle:?} switch {switch:?} unreachable"
                        );
                    }
                }
                VaultPuzzle::Seals => {
                    let seals: Vec<TilePosition> = tiles
                        .iter()
                        .filter(|(_, tile)| *tile == PoiTile::VaultSeal)
                        .map(|(position, _)| *position)
                        .collect();
                    assert!(seals.len() >= 2, "seed {seed}");
                    // A bomb dropped from a reachable tile next to each seal catches it.
                    for seal in seals {
                        assert!(
                            [(1, 0), (-1, 0), (0, 1), (0, -1)]
                                .iter()
                                .any(|(dx, dy)| reached(seal.x + dx, seal.y + dy)),
                            "seed {seed}: seal {seal:?} out of reach"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn feature_queries_find_pois_by_kind_band_region_and_proximity() {
    let world = generate(&WorldSettings::new(9));
    let features = &world.features;
    let vault = features
        .pois_of(PoiKind::Vault)
        .next()
        .copied()
        .expect("a vault");
    assert_eq!(
        features.poi_at(vault.center).map(|poi| poi.id),
        Some(vault.id)
    );
    assert_eq!(
        features
            .nearest_poi(vault.center, Some(PoiKind::Vault))
            .map(|poi| poi.id),
        Some(vault.id)
    );
    assert!(features.nearest_poi(vault.center, None).is_some());
    assert!(
        features
            .pois_in_band(vault.band)
            .any(|poi| poi.id == vault.id)
    );
    assert!(
        features
            .in_region(vault.bounds)
            .any(|poi| poi.id == vault.id)
    );
    assert!(
        features
            .pois()
            .all(|poi| poi.kind == FeatureKind::PointOfInterest)
    );
}
