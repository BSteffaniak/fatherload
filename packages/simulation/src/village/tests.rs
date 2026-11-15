//! Village placement, layout, terrain and lookup tests.

use std::collections::BTreeSet;

use super::*;
use crate::{
    terrain::{TileKind, tile_hardness},
    worldgen::{GeneratorVersion, streamed::generated_tile},
};

fn v6(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V6,
        ..WorldSettings::new(seed)
    }
}

/// Seeds the suite samples (`FATHERLOAD_WORLDGEN_SEEDS` raises the count).
fn seeds(default: u64) -> impl Iterator<Item = u64> {
    let count = std::env::var("FATHERLOAD_WORLDGEN_SEEDS")
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(default);
    (0..count).map(|index| index.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x7111_A6E5)
}

/// Every village of sites `-reach..=reach` (surface and underground).
fn sample(settings: &WorldSettings, reach: i32) -> Vec<Arc<Village>> {
    let mut found = Vec::new();
    for site in -reach..=reach {
        if site == 0 {
            continue;
        }
        found.extend(village(settings, placement::surface_id(site)));
        found.extend(village(settings, placement::underground_id(site)));
    }
    found
}

#[test]
fn older_generators_have_no_villages() {
    for generator in [GeneratorVersion::V4, GeneratorVersion::V5] {
        let settings = WorldSettings {
            generator,
            ..WorldSettings::new(9)
        };
        assert!(village(&settings, placement::surface_id(1)).is_none());
        assert!(villages_between(&settings, -5_000, 5_000).is_empty());
        let site = placement::surface_site(9, 1).expect("site");
        assert_eq!(
            crate::worldgen::surface_row(&settings, site.center),
            crate::worldgen::streamed::raw_surface_row(&settings, site.center)
        );
    }
}

#[test]
fn villages_are_deterministic() {
    let settings = v6(0xD0E5);
    let first = sample(&settings, 4);
    crate::worldgen::streamed::clear_caches();
    let again = sample(&settings, 4);
    assert_eq!(first.len(), again.len());
    for (a, b) in first.iter().zip(&again) {
        assert_eq!(**a, **b);
    }
}

#[test]
fn every_village_is_laid_out_inside_its_span_with_a_service_and_a_specialty() {
    for seed in seeds(12) {
        let settings = v6(seed);
        for found in sample(&settings, 6) {
            let label = format!("seed {seed:#x} {}", found.describe());
            let span = found.first_column..=found.last_column;
            assert!(
                span.contains(&found.plaza.0) && span.contains(&found.plaza.1),
                "{label}"
            );
            if let Some((first, last)) = found.landing_pad {
                assert!(span.contains(&first) && span.contains(&last), "{label}");
            }
            let mut previous_end = found.first_column - 1;
            for building in &found.buildings {
                assert!(building.first_column > previous_end, "{label}: overlap");
                assert!(span.contains(&building.last_column()), "{label}");
                let plaza = found.plaza.0..=found.plaza.1;
                assert!(
                    !plaza.contains(&building.first_column)
                        && !plaza.contains(&building.last_column()),
                    "{label}: building on the plaza"
                );
                previous_end = building.last_column();
            }
            let specialty = found
                .buildings
                .iter()
                .position(|building| building.kind == found.specialty.building())
                .unwrap_or_else(|| panic!("{label}: no specialty building"));
            assert!(
                !found.offers(specialty).is_empty(),
                "{label}: specialty sells nothing"
            );
            let market = found
                .buildings
                .iter()
                .position(|building| building.kind == BuildingKind::Market)
                .unwrap_or_else(|| panic!("{label}: no market"));
            assert!(!found.offers(market).is_empty(), "{label}");
            // Every village (inn or not) can point you at something nearby.
            assert!(
                (0..found.buildings.len()).any(|building| found
                    .offers(building)
                    .iter()
                    .any(|offer| offer.deal == crate::village::VillageDeal::Rumour)),
                "{label}: nobody trades rumours"
            );
            let count = found.population.characters.len();
            assert!((4..=12).contains(&count), "{label}: {count} residents");
            let housed: BTreeSet<u16> = found
                .buildings
                .iter()
                .flat_map(|building| building.residents.iter().copied())
                .collect();
            assert_eq!(housed.len(), count, "{label}: someone is homeless");
            for building in found
                .buildings
                .iter()
                .filter(|b| !b.kind.keeper_roles().is_empty())
            {
                assert!(
                    building.keeper.is_some(),
                    "{label}: {:?} unstaffed",
                    building.kind
                );
            }
            assert!(!found.name.is_empty());
        }
    }
}

#[test]
fn villages_vary_in_culture_size_and_specialty() {
    let mut cultures = BTreeSet::new();
    let mut sizes = BTreeSet::new();
    let mut specialties = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut total = 0;
    for seed in seeds(6) {
        for found in sample(&v6(seed), 10) {
            cultures.insert(found.culture);
            sizes.insert(found.size);
            specialties.insert(found.specialty.name());
            names.insert(found.name.clone());
            total += 1;
        }
    }
    assert_eq!(cultures.len(), 6, "{cultures:?}");
    assert_eq!(sizes.len(), 3);
    assert_eq!(specialties.len(), 7, "{specialties:?}");
    assert!(
        names.len() * 10 >= total * 8,
        "{} names for {total}",
        names.len()
    );
}

#[test]
fn surface_villages_stand_on_flat_foundations_under_open_sky() {
    for seed in seeds(4) {
        let settings = v6(seed);
        for site in [-2, -1, 1, 2] {
            let found = village(&settings, placement::surface_id(site)).expect("village");
            let ground = found.ground_row;
            for x in found.first_column..=found.last_column {
                assert_eq!(crate::worldgen::surface_row(&settings, x), ground, "x {x}");
                for y in (ground - 8).max(0)..ground {
                    assert_eq!(
                        generated_tile(&settings, TilePosition { x, y }).kind,
                        TileKind::Air,
                        "seed {seed:#x} ({x}, {y})"
                    );
                }
                let below = generated_tile(&settings, TilePosition { x, y: ground }).kind;
                assert!(
                    tile_hardness(below) > 0 || below == TileKind::Foundation,
                    "{below:?}"
                );
                for y in ground..ground + 8 {
                    let kind = generated_tile(&settings, TilePosition { x, y }).kind;
                    assert!(
                        crate::worldgen::HazardKind::of_tile(kind).is_none(),
                        "hazard under the village at ({x}, {y})"
                    );
                }
            }
            for building in &found.buildings {
                for x in building.first_column..=building.last_column() {
                    for y in ground..ground + 2 {
                        assert_eq!(
                            generated_tile(&settings, TilePosition { x, y }).kind,
                            TileKind::Foundation
                        );
                    }
                }
            }
            // The ground eases back into the heightmap beyond the span.
            let outside = found.last_column + placement::FLATTEN_FADE_COLUMNS + 1;
            assert_eq!(
                crate::worldgen::surface_row(&settings, outside),
                crate::worldgen::streamed::raw_surface_row(&settings, outside)
            );
        }
    }
}

#[test]
fn underground_settlements_sit_in_a_carved_hall_clear_of_sites() {
    let mut halls = 0;
    for seed in seeds(8) {
        let settings = v6(seed);
        for site in -8..=8 {
            let Some(found) = village(&settings, placement::underground_id(site)) else {
                continue;
            };
            halls += 1;
            assert_eq!(found.setting, VillageSetting::Underground);
            assert_eq!(found.culture, crate::people::Culture::Deepfolk);
            let area = found.area();
            for x in area.min.x..=area.max.x {
                for y in area.min.y..found.ground_row {
                    assert_eq!(
                        generated_tile(&settings, TilePosition { x, y }).kind,
                        TileKind::Air,
                        "seed {seed:#x} hall air at ({x}, {y})"
                    );
                }
                assert_eq!(
                    generated_tile(
                        &settings,
                        TilePosition {
                            x,
                            y: found.ground_row
                        }
                    )
                    .kind,
                    TileKind::Foundation
                );
            }
            let rect = area.expanded(3);
            for position in rect.positions() {
                let kind = generated_tile(&settings, position).kind;
                assert!(
                    crate::worldgen::HazardKind::of_tile(kind).is_none(),
                    "{position:?}"
                );
                assert!(!matches!(kind, TileKind::Poi(_)), "{position:?}");
            }
        }
    }
    assert!(halls > 0);
}

#[test]
fn the_feature_index_finds_villages_anywhere() {
    let settings = v6(0x5EED);
    let terrain = crate::terrain::Terrain::generate(&settings);
    for from_x in [-40_000, -3_000, 0, 97, 2_500, 60_000] {
        let from = TilePosition { x: from_x, y: 5 };
        let feature = terrain
            .nearest_feature(from, FeatureKind::Village, 2_000, |_| true)
            .unwrap_or_else(|| panic!("no village near {from_x}"));
        let id = feature.village().expect("village detail");
        let found = village(&settings, id).expect("resolves");
        assert!((found.center_column() - from_x).abs() <= 1_000, "{from_x}");
        let nearest = nearest_village(&settings, from, 2_000, |_| true).expect("nearest");
        assert_eq!(nearest.id, found.id);
        assert_eq!(terrain.feature(feature.id), Some(feature));
    }
    let listed = villages_between(&settings, -2_000, 2_000);
    let featured = terrain.features_in(
        &crate::worldgen::TileRect {
            min: TilePosition { x: -2_000, y: 0 },
            max: TilePosition {
                x: 2_000,
                y: terrain.height() - 1,
            },
        },
        Some(&[FeatureKind::Village]),
    );
    assert_eq!(listed.len(), featured.len());
    assert!(listed.len() >= 6, "{}", listed.len());
}

#[test]
fn doors_and_areas_resolve_from_positions() {
    let settings = v6(77);
    let found = village(&settings, placement::surface_id(-1)).expect("village");
    let building = found
        .buildings
        .iter()
        .position(VillageBuilding::enterable)
        .expect("enterable");
    let lot = &found.buildings[building];
    #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
    let (x, y) = (
        (lot.first_column as f32 + 1.5) * TILE_SIZE,
        (found.ground_row as f32 - 0.5) * TILE_SIZE,
    );
    let (at, index) = building_at(&settings, x, y).expect("door");
    assert_eq!((at.id, index), (found.id, building));
    assert!(building_at(&settings, x, 6.0f32.mul_add(-TILE_SIZE, y)).is_none());
    assert!(village_at(&settings, TilePosition { x: 97, y: 4 }).is_none());
    let inside = TilePosition {
        x: found.center_column(),
        y: found.ground_row - 3,
    };
    assert_eq!(village_at(&settings, inside).map(|v| v.id), Some(found.id));
}

#[test]
fn signposts_point_to_the_neighbours_and_home() {
    let settings = v6(12);
    let first = village(&settings, placement::surface_id(1)).expect("village");
    let west = first
        .signposts
        .iter()
        .find(|post| post.direction < 0)
        .expect("west post");
    assert_eq!(west.target, SignTarget::Town);
    let east = first
        .signposts
        .iter()
        .find(|post| post.direction > 0)
        .expect("east post");
    assert_eq!(east.target, SignTarget::Village(placement::surface_id(2)));
    assert!((300..=800).contains(&east.distance), "{}", east.distance);
}

#[test]
fn trail_milestones_point_at_the_nearest_place_along_the_surface() {
    for seed in seeds(4) {
        let settings = v6(seed);
        let (town_first, town_last) = crate::surface::town_span();
        let mut posts = 0;
        for x in -3_000..=3_000 {
            let Some(post) = milestone_at(&settings, x) else {
                continue;
            };
            posts += 1;
            assert_eq!(post.column, x);
            assert!(
                placement::surface_site_at(settings.seed, x, 0).is_none(),
                "seed {seed:#x}: milestone inside a village at {x}"
            );
            assert!(!(town_first..=town_last).contains(&x));
            // It points the right way, at the nearest place, and says how far.
            let (target_first, target_last) = match post.target {
                SignTarget::Town => (town_first, town_last),
                SignTarget::Village(id) => {
                    let found = village(&settings, id).expect("target village");
                    assert_eq!(found.setting, VillageSetting::Surface);
                    (found.first_column, found.last_column)
                }
            };
            let edge = if post.direction > 0 {
                target_first
            } else {
                target_last
            };
            assert_eq!(
                (edge - x) * post.direction,
                post.distance,
                "seed {seed:#x} x {x}"
            );
            assert!(post.distance > 0 && post.distance <= 450, "{post:?}");
        }
        // Roughly one every MILESTONE_SPACING columns, minus the villages and town.
        assert!(
            (40..=76).contains(&posts),
            "seed {seed:#x}: {posts} milestones"
        );
    }
    let v5 = WorldSettings {
        generator: GeneratorVersion::V5,
        ..WorldSettings::new(1)
    };
    assert_eq!(milestone_at(&v5, 1_075), None);
}

#[test]
fn village_facts_and_story_seeds_bind() {
    let settings = v6(0xFAC7);
    let found = village(&settings, placement::surface_id(2)).expect("village");
    let facts = village_facts(&settings, &found);
    assert!(!facts.is_empty());
    let seeds = found.story_seeds(&facts, &[], 0);
    assert!(!seeds.is_empty());
}

/// Chunk generation cost with and without villages, and the cost of building a village
/// (`cargo test --release -p fatherload_simulation village_timing_report -- --ignored
/// --nocapture`).
#[test]
#[ignore = "timing report"]
fn village_timing_report() {
    use crate::{terrain::ChunkPosition, worldgen::streamed::generated_chunk};
    for generator in [GeneratorVersion::V5, GeneratorVersion::V6] {
        let settings = WorldSettings {
            generator,
            ..WorldSettings::new(0x71AE)
        };
        crate::worldgen::streamed::clear_caches();
        let start = std::time::Instant::now();
        // The surface and crust chunks across two villages' worth of columns.
        for x in 20..60 {
            for y in 0..4 {
                let _ = generated_chunk(&settings, ChunkPosition { x, y });
            }
        }
        println!("{generator:?}: 160 surface chunks in {:?}", start.elapsed());
    }
    let settings = v6(0x71AE);
    let start = std::time::Instant::now();
    let built = sample(&settings, 20).len();
    println!("{built} villages built in {:?}", start.elapsed());
}
