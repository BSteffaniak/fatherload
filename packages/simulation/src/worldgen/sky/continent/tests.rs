use std::collections::{BTreeSet, VecDeque};

use super::*;
use crate::{
    terrain::ChunkPosition,
    worldgen::{
        GeneratorVersion,
        sky::{SkyLandKind, V9_SKY_BANDS, lands_touching, row_of},
        streamed,
    },
};

fn v9(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V9,
        ..WorldSettings::new(seed)
    }
}

fn band() -> TileRect {
    TileRect {
        min: TilePosition {
            x: -8_000,
            y: row_of(V9_SKY_BANDS.orbit),
        },
        max: TilePosition {
            x: 8_000,
            y: row_of(V9_SKY_BANDS.continents),
        },
    }
}

fn continents(settings: &WorldSettings) -> Vec<(SkyLand, ContinentPlan)> {
    continents_touching(settings, &band())
}

#[test]
fn only_v9_fills_continents_and_keeps_v8_placement() {
    // V10 (tombs, WP-TOMBS) is current and keeps V9's sky exactly.
    assert_eq!(GeneratorVersion::CURRENT, GeneratorVersion::V10);
    for generator in GeneratorVersion::ALL {
        assert_eq!(
            generator.has_sky_content(),
            matches!(
                generator,
                GeneratorVersion::V9 | GeneratorVersion::V10 | GeneratorVersion::V11
            )
        );
    }
    for seed in [3_u64, 0x00C0_FFEE] {
        let nine = v9(seed);
        let eight = WorldSettings {
            generator: GeneratorVersion::V8,
            ..nine
        };
        assert_eq!(continents(&eight), []);
        assert!(!continents(&nine).is_empty(), "seed {seed}");
        // Every continent is placed in the same columns, as wide and as deep; only its
        // altitude (V9's own, taller continent band) and its content differ. (The islands are
        // furnished by V9 too, see `worldgen::sky::isle_tests`, and the Moon by the Moon
        // generator, see `worldgen::sky::moon`.)
        let area = TileRect {
            min: TilePosition {
                x: -6_000,
                y: -V9_SKY_BANDS.ceiling_rows(),
            },
            max: TilePosition { x: 6_000, y: 0 },
        };
        let only_continents = |settings: &WorldSettings| {
            lands_touching(settings, &area)
                .into_iter()
                .filter(|land| land.kind == SkyLandKind::Continent)
                .collect::<Vec<_>>()
        };
        let (a, b) = (only_continents(&eight), only_continents(&nine));
        assert_eq!(a.len(), b.len());
        for (old, new) in a.iter().zip(&b) {
            assert_eq!(
                (old.id, old.center_x, old.half_width, old.depth),
                (new.id, new.center_x, new.half_width, new.depth)
            );
        }
        // The ground generates the same tiles.
        for chunk in [ChunkPosition { x: 0, y: 0 }, ChunkPosition { x: -3, y: 2 }] {
            assert_eq!(
                streamed::generated_chunk(&eight, chunk).tiles(),
                streamed::generated_chunk(&nine, chunk).tiles(),
                "{chunk:?}"
            );
        }
    }
}

#[test]
fn every_continent_has_a_city_a_temple_a_lake_peaks_nests_and_a_core() {
    for seed in [1_u64, 77, 0xD1_11_6A_4E, 0x5EED] {
        let settings = v9(seed);
        let found = continents(&settings);
        assert!(!found.is_empty(), "seed {seed}");
        for (land, plan) in &found {
            let city = plan.city.expect("a city");
            let temple = plan.temple.expect("a temple");
            let lake = plan.lake.expect("a lake");
            assert!(plan.peaks.iter().flatten().count() >= 1);
            assert!(plan.nests.iter().flatten().count() >= 1);
            let (west, east) = (
                land.center_x - land.half_width,
                land.center_x + land.half_width,
            );
            assert!(city.first > west && city.last < east && city.last - city.first >= 80);
            let rect = temple.rect();
            assert!(rect.min.x > west && rect.max.x < east);
            // Nothing overlaps: the temple's plain, the city and the lake are apart.
            assert!(rect.max.x < city.first - 4 || rect.min.x > city.last + 4);
            assert!(
                lake.center + lake.half_width < city.first
                    || lake.center - lake.half_width > city.last
            );
            // The street is flat foundation; buildings stand inside it, apart.
            for x in city.first..=city.last {
                assert_eq!(land.surface_row(x), Some(city.street_row), "x {x}");
                assert_eq!(land.tile_at(x, city.street_row), Some(TileKind::Foundation));
            }
            // The core is indestructible aether, wrapped in crystal.
            let core = plan.core;
            assert_eq!(
                land.tile_at(core.center_x, core.center_y),
                Some(TileKind::Sky(SkyTile::AetherCore))
            );
            assert!(crate::terrain::is_indestructible(TileKind::Sky(
                SkyTile::AetherCore
            )));
        }
    }
}

#[test]
fn bodies_hold_caves_storm_glass_and_aether_crystal() {
    let settings = v9(0xD1_11_6A_4E);
    let (mut caves, mut glass, mut aether, mut eggs, mut water) = (0, 0, 0, 0, 0);
    for (land, plan) in continents(&settings) {
        let bounds = land.bounds();
        for x in (bounds.min.x..=bounds.max.x).step_by(2) {
            let Some(top) = land.surface_row(x) else {
                continue;
            };
            for y in bounds.min.y..=bounds.max.y {
                match land.tile_at(x, y) {
                    Some(TileKind::Sky(SkyTile::StormGlass)) => glass += 1,
                    Some(TileKind::Sky(SkyTile::AetherCrystal)) => aether += 1,
                    Some(TileKind::Sky(SkyTile::HarpyEgg)) => eggs += 1,
                    None if y > top + 8 && y < land.top_row + land.depth / 2 => caves += 1,
                    _ => {}
                }
                if plan.water(&land, x, y) {
                    assert_eq!(land.tile_at(x, y), None, "water stands in open tiles");
                    water += 1;
                }
            }
        }
        for x in [
            plan.nests[0].expect("nest").x - 1,
            plan.nests[0].expect("nest").x + 1,
        ] {
            let floor = plan.nests[0].expect("nest").y;
            assert_eq!(
                land.tile_at(x, floor - 1),
                Some(TileKind::Sky(SkyTile::HarpyEgg))
            );
        }
    }
    assert!(caves > 200, "caves {caves}");
    assert!(glass > 20, "storm glass {glass}");
    assert!(aether > 40, "aether crystal {aether}");
    assert!(eggs > 0 && water > 30, "eggs {eggs} water {water}");
    assert_eq!(
        crate::item::ItemKind::from_tile(TileKind::Sky(SkyTile::StormGlass)),
        Some(crate::item::ItemKind::Material(
            crate::terrain::StrategicResourceKind::StormGlass
        ))
    );
}

/// The open cells a pilot can reach from `start` (a cell is where its feet rest on the tile
/// below): walks, two-row jumps from support, falls. Doors in `opened` count as open.
fn pilot_reach(
    land: &SkyLand,
    start: TilePosition,
    opened: &BTreeSet<TilePosition>,
) -> BTreeSet<TilePosition> {
    let solid = |tile: TilePosition| {
        if opened.contains(&tile) {
            return false;
        }
        match land.tile_at(tile.x, tile.y) {
            None | Some(TileKind::Air | TileKind::Built(BuiltTile::VentGrate)) => false,
            Some(_) => true,
        }
    };
    let mut seen = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(cell) = queue.pop_front() {
        let supported = solid(TilePosition {
            x: cell.x,
            y: cell.y + 1,
        });
        for (dx, dy) in [
            (1, 0),
            (-1, 0),
            (0, -1),
            (0, -2),
            (1, -1),
            (-1, -1),
            (1, -2),
            (-1, -2),
            (0, 1),
            (1, 1),
            (-1, 1),
        ] {
            let next = TilePosition {
                x: cell.x + dx,
                y: cell.y + dy,
            };
            if seen.contains(&next) || solid(next) || (dy < 0 && !supported) {
                continue;
            }
            if dy == -2
                && solid(TilePosition {
                    x: cell.x,
                    y: cell.y - 1,
                })
            {
                continue;
            }
            if (next.x - start.x).abs() > 80 || (next.y - start.y).abs() > 40 {
                continue;
            }
            seen.insert(next);
            queue.push_back(next);
        }
    }
    seen
}

fn touches(reach: &BTreeSet<TilePosition>, tile: TilePosition) -> bool {
    (-1..=1).any(|dx| {
        (-1..=1).any(|dy| {
            reach.contains(&TilePosition {
                x: tile.x + dx,
                y: tile.y + dy,
            })
        })
    })
}

#[test]
fn temples_open_in_order_on_foot_and_keep_rigs_out() {
    for seed in [1_u64, 0xD1_11_6A_4E] {
        let settings = v9(seed);
        for (land, temple) in temples_touching(&settings, &band()) {
            let entrance = temple.entrance();
            assert_eq!(
                land.tile_at(entrance.x, entrance.y),
                Some(TileKind::Built(BuiltTile::VentGrate))
            );
            // Outside the grate, on the plain's ground.
            let side = if temple.mirrored { 1 } else { -1 };
            let start = TilePosition {
                x: entrance.x + side * 4,
                y: entrance.y,
            };
            assert_eq!(land.tile_at(start.x, start.y), None);
            assert!(
                land.tile_at(start.x, start.y + 1).is_some(),
                "ground outside"
            );
            let lever = temple.tiles_of('L')[0];
            let keystone = temple.tiles_of('k')[0];
            let gates: BTreeSet<TilePosition> = temple.tiles_of('D').into_iter().collect();
            let vault: BTreeSet<TilePosition> = temple.tiles_of('K').into_iter().collect();
            let chest = temple.tiles_of('C')[0];
            let closed = pilot_reach(&land, start, &BTreeSet::new());
            assert!(touches(&closed, lever), "the lever is reachable");
            assert!(!touches(&closed, keystone), "the gate guards the keystone");
            let gate_open = pilot_reach(&land, start, &gates);
            assert!(touches(&gate_open, keystone));
            assert!(!touches(&gate_open, chest), "the vault stays shut");
            let all: BTreeSet<TilePosition> = gates.union(&vault).copied().collect();
            assert!(touches(&pilot_reach(&land, start, &all), chest));
            // Temple stone is indestructible: a rig can neither drill in nor squeeze past the
            // grate.
            for tile in temple.tiles_of('#').into_iter().take(40) {
                let kind = land.tile_at(tile.x, tile.y).expect("stone");
                assert!(crate::terrain::is_indestructible(kind));
            }
            assert!(!crate::pilot::pilot_passable(TileKind::Sky(
                SkyTile::TempleDoor
            )));
        }
    }
}

#[test]
fn sky_cities_are_found_by_id_and_by_place() {
    let settings = v9(0x5EED);
    let cities = sky_cities_between(&settings, -8_000, 8_000);
    assert_ne!(cities, []);
    for city in &cities {
        assert!(is_sky_city(city.id));
        assert!(!crate::people::camps::is_camp(city.id));
        assert_eq!(sky_city(&settings, city.id).as_ref(), Some(city));
        assert!(
            city.buildings.len() >= 6,
            "{} buildings",
            city.buildings.len()
        );
        let kinds: BTreeSet<_> = city.buildings.iter().map(|b| b.kind).collect();
        assert!(
            kinds.contains(&SkyBuildingKind::Monastery) && kinds.contains(&SkyBuildingKind::Hangar)
        );
        for pair in city.buildings.windows(2) {
            assert!(pair[0].first_column + pair[0].width < pair[1].first_column);
        }
        for building in &city.buildings {
            assert!(building.first_column >= city.first_column);
            assert!(building.first_column + building.width <= city.last_column);
            assert!(
                building.first_column + building.width <= city.plaza.0
                    || building.first_column >= city.plaza.1
            );
        }
        let middle = TilePosition {
            x: i32::midpoint(city.first_column, city.last_column),
            y: city.street_row - 1,
        };
        assert_eq!(sky_city_at(&settings, middle, 0).as_ref(), Some(city));
        assert!(!lake_water_at(&settings, middle));
    }
    assert!(sky_city(&v9(0x5EED), crate::village::VillageId(4)).is_none());
}

#[test]
fn chunk_digests_of_v9_continents_are_pinned() {
    let mut combined = 0_u64;
    for seed in [1_u64, 0xD1_11_6A_4E] {
        let settings = v9(seed);
        for (land, plan) in continents(&settings).into_iter().take(2) {
            let temple = plan.temple.expect("temple");
            let city = plan.city.expect("city");
            let chunks = [
                ChunkPosition::of(TilePosition {
                    x: land.center_x,
                    y: land.top_row,
                }),
                ChunkPosition::of(temple.world((20, 14))),
                ChunkPosition::of(TilePosition {
                    x: city.first,
                    y: city.street_row,
                }),
                ChunkPosition::of(TilePosition {
                    x: plan.core.center_x,
                    y: plan.core.center_y,
                }),
            ];
            for chunk in chunks {
                for (index, tile) in streamed::generated_chunk(&settings, chunk)
                    .tiles()
                    .iter()
                    .enumerate()
                {
                    combined = combined
                        .rotate_left(7)
                        .wrapping_add(tile.code() ^ (index as u64).wrapping_mul(0x9E37));
                }
            }
        }
    }
    assert_eq!(
        combined, GOLDEN_V9_CONTINENT_DIGEST,
        "V9 continent generation changed: {combined:#018x}"
    );
}

// Re-pinned at the wave 4 integration: the continent tiles moved to codes 0x26-0x30 (after the
// sky isles' tiles) when T20 and T21 were folded into one V9.
const GOLDEN_V9_CONTINENT_DIGEST: u64 = 0x00b0_081e_60ca_29bf;

#[test]
fn every_nest_sits_on_its_summit_with_both_eggs() {
    // A nest without its eggs from the start would leave its nester enraged before anyone
    // touched it (`nest_has_eggs` in `world/enemies/sky.rs` reads the eggs at x +- 1).
    for seed in [1_u64, 3, 77, 0xD1_11_6A_4E, 0x5EED, 0x00C0_FFEE, 42, 9_001] {
        let settings = v9(seed);
        for (land, plan) in continents(&settings) {
            assert!(plan.nests.iter().flatten().count() >= 1, "seed {seed}");
            for nest in plan.nests.iter().flatten() {
                assert_eq!(
                    land.surface_row(nest.x),
                    Some(nest.y),
                    "seed {seed} {nest:?}"
                );
                for x in [nest.x - 1, nest.x + 1] {
                    assert_eq!(
                        land.tile_at(x, nest.y - 1),
                        Some(TileKind::Sky(SkyTile::HarpyEgg)),
                        "seed {seed} {nest:?}"
                    );
                }
            }
        }
    }
}
