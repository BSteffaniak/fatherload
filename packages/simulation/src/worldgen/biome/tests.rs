//! Tests of the surface biomes and heightmap: determinism, continuity, flat town and sites,
//! biome fairness over many seeds, traversability and chunk-seamless biome ground.

use super::*;
use crate::{
    terrain::{ChunkPosition, TilePosition},
    worldgen::{GeneratorVersion, streamed},
};

fn v7(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V7,
        ..WorldSettings::new(seed)
    }
}

/// Seeds sampled (`FATHERLOAD_WORLDGEN_SEEDS` raises the count).
fn sample_seeds(default: u64) -> impl Iterator<Item = u64> {
    let count = std::env::var("FATHERLOAD_WORLDGEN_SEEDS")
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(default);
    (0..count).map(|index| index.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5EED_0006)
}

#[test]
fn v7_is_the_current_generator_and_has_biomes() {
    assert_eq!(GeneratorVersion::CURRENT, GeneratorVersion::V7);
    assert!(GeneratorVersion::V7.has_biomes());
    assert!(GeneratorVersion::V7.is_streamed());
    assert!(!GeneratorVersion::V5.has_biomes());
    assert_eq!(streamed::top_row(&v7(1)), -SKY_HEADROOM_ROWS);
}

#[test]
fn the_heightmap_is_deterministic_and_cache_independent() {
    for seed in sample_seeds(4) {
        let settings = v7(seed);
        let cached: Vec<i32> = (-3_000..3_000)
            .step_by(7)
            .map(|x| surface_row(&settings, x))
            .collect();
        let fresh: Vec<i32> = (-3_000..3_000)
            .step_by(7)
            .map(|x| compute_column(seed, x).surface_row)
            .collect();
        assert_eq!(cached, fresh, "seed {seed}");
        // Reverse order through the cache agrees too.
        let mut reverse: Vec<i32> = (-3_000..3_000)
            .step_by(7)
            .rev()
            .map(|x| surface_row(&settings, x))
            .collect();
        reverse.reverse();
        assert_eq!(cached, reverse);
    }
}

#[test]
fn the_heightmap_stays_in_bounds_and_has_no_impassable_walls() {
    for seed in sample_seeds(8) {
        let settings = v7(seed);
        let mut previous = surface_row(&settings, -12_001);
        for x in -12_000..12_000 {
            let row = surface_row(&settings, x);
            assert!(
                (SKY_ROWS - MAX_ELEVATION_ROWS..=SKY_ROWS + MAX_DIP_ROWS).contains(&row),
                "seed {seed} column {x}: row {row}"
            );
            // Cliffs exist, but no single step is taller than a short climb.
            assert!(
                (row - previous).abs() <= 8,
                "seed {seed} column {x}: step {previous} -> {row}"
            );
            previous = row;
        }
    }
}

#[test]
fn the_town_stays_flat_at_the_classic_surface() {
    for seed in sample_seeds(16) {
        let settings = v7(seed);
        for x in streamed::TOWN_FLAT_FIRST_COLUMN..=streamed::TOWN_FLAT_LAST_COLUMN {
            assert_eq!(
                surface_row(&settings, x),
                SKY_ROWS,
                "seed {seed} column {x}"
            );
            assert!(!mountain_cave(&settings, x, SKY_ROWS - 4));
        }
        assert_eq!(
            biome_at(&settings, crate::worldgen::SPAWN_TILE_X),
            Biome::Plains
        );
        assert!(flat_sites_in(&settings, -100, 340).is_empty());
    }
}

#[test]
fn flat_sites_are_flat_spaced_and_findable() {
    for seed in sample_seeds(8) {
        let settings = v7(seed);
        let sites = flat_sites_in(&settings, -20_000, 20_000);
        assert!(sites.len() > 50, "seed {seed}: {} sites", sites.len());
        for pair in sites.windows(2) {
            let gap = pair[1].center - pair[0].center;
            // The village ladder: 400-800 apart, wider across the town.
            let across_town = pair[0].center < 0 && pair[1].center > 0;
            assert!(
                (400..=800).contains(&gap) || (across_town && gap <= 1_100),
                "seed {seed}: gap {gap}"
            );
        }
        for site in &sites {
            for x in site.first_column..=site.last_column {
                assert_eq!(
                    surface_row(&settings, x),
                    site.row,
                    "seed {seed} site {site:?}"
                );
                assert!(column(&settings, x).flat);
            }
            assert_eq!(flat_site_at(&settings, site.center), Some(*site));
            assert_eq!(
                nearest_flat_site(&settings, site.center + 3, 1_000).map(|s| s.center),
                Some(site.center)
            );
            assert!(site.row <= LAKE_ROW, "site {site:?} under water");
            // The sites are the villages' sites: each village stands on its site's flat ground
            // and takes its look from the site's biome.
            let village = crate::village::placement::surface_site_at(seed, site.center, 0)
                .expect("a village on every flat site");
            assert_eq!(
                (village.first, village.last),
                (site.first_column, site.last_column)
            );
            assert_eq!(
                streamed::village_ground_row(&settings, site.center),
                site.row
            );
            let landscape = crate::village::layout::site_landscape(&settings, site.center);
            let expected: &[crate::village::layout::Landscape] = match biome_at(&settings, site.center) {
                Biome::Plains => &[crate::village::layout::Landscape::Plains],
                Biome::Forest | Biome::Wetlands => &[crate::village::layout::Landscape::Forest],
                Biome::Mountains | Biome::Tundra => &[crate::village::layout::Landscape::Hills],
                Biome::Desert => &[crate::village::layout::Landscape::Mesa],
                Biome::Badlands => &[
                    crate::village::layout::Landscape::Mesa,
                    crate::village::layout::Landscape::Scrapland,
                ],
            };
            assert!(expected.contains(&landscape), "site {site:?}: {landscape:?}");
        }
    }
    assert!(flat_sites_in(&WorldSettings::bounded(1), -1_000, 1_000).is_empty());
}

#[test]
fn every_biome_appears_near_spawn_on_every_seed() {
    // Fairness: within 16,000 columns of the town every biome shows up, and no biome hogs
    // the surface.
    for seed in sample_seeds(24) {
        let settings = v7(seed);
        let mut counts = std::collections::BTreeMap::new();
        for x in (-8_000..8_000).step_by(4) {
            *counts.entry(biome_at(&settings, x)).or_insert(0_u32) += 1;
        }
        for biome in Biome::ALL {
            let count = counts.get(&biome).copied().unwrap_or(0);
            assert!(count > 0, "seed {seed}: no {}", biome.name());
        }
        let total: u32 = counts.values().sum();
        for (biome, count) in &counts {
            assert!(
                count * 100 / total <= 40,
                "seed {seed}: {} covers {}%",
                biome.name(),
                count * 100 / total
            );
        }
    }
}

#[test]
fn mountains_are_tall_and_lakes_hold_water() {
    for seed in sample_seeds(12) {
        let settings = v7(seed);
        let highest = (-12_000..12_000)
            .map(|x| surface_row(&settings, x))
            .min()
            .unwrap_or(SKY_ROWS);
        assert!(
            highest <= SKY_ROWS - 40,
            "seed {seed}: highest ground row {highest}"
        );
        let lake_columns = (-12_000..12_000)
            .filter(|x| lake_row(&settings, *x).is_some())
            .count();
        assert!(
            lake_columns > 20,
            "seed {seed}: {lake_columns} lake columns"
        );
    }
}

#[test]
fn biome_ground_is_seamless_across_chunk_borders() {
    // Every chunk is generated alone; a tile reads the same from its own chunk and from a
    // window spanning chunk borders, here over mountains and lakes above row 0.
    let settings = v7(0x51DE);
    let mountain = (-12_000..12_000)
        .min_by_key(|x| surface_row(&settings, *x))
        .unwrap_or(0);
    for x in mountain - 40..mountain + 40 {
        let column = column(&settings, x);
        for y in column.surface_row - 3..column.surface_row + 6 {
            let position = TilePosition { x, y };
            let alone = streamed::generated_chunk(&settings, ChunkPosition::of(position))
                .tile(ChunkPosition::local_index(position));
            assert_eq!(alone, streamed::generated_tile(&settings, position));
            if y < column.surface_row {
                assert_eq!(alone.kind, TileKind::Air, "{position:?}");
            } else if y == column.surface_row && !mountain_cave(&settings, x, y) {
                assert_ne!(alone.kind, TileKind::Air, "{position:?}");
            }
        }
    }
}

#[test]
fn surface_materials_follow_the_biome() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in sample_seeds(6) {
        let settings = v7(seed);
        for x in (-8_000..8_000).step_by(3) {
            let column = column(&settings, x);
            if let Some(TileKind::Surface(material)) = ground_kind(&settings, x, column.surface_row)
            {
                seen.insert(material);
                let ok = match material {
                    SurfaceMaterial::Sand => {
                        matches!(column.biome, Biome::Desert | Biome::Badlands)
                    }
                    SurfaceMaterial::Snow => column.biome.is_cold(),
                    SurfaceMaterial::Mud => column.biome == Biome::Wetlands,
                    SurfaceMaterial::Granite | SurfaceMaterial::RedRock => {
                        matches!(column.biome, Biome::Mountains | Biome::Badlands)
                    }
                    SurfaceMaterial::Sandstone => column.biome.is_dry(),
                };
                assert!(
                    ok,
                    "seed {seed} column {x}: {material:?} in {:?}",
                    column.biome
                );
            }
        }
    }
    for material in [
        SurfaceMaterial::Sand,
        SurfaceMaterial::Snow,
        SurfaceMaterial::Mud,
        SurfaceMaterial::Granite,
        SurfaceMaterial::RedRock,
    ] {
        assert!(
            seen.contains(&material),
            "{material:?} never on the surface"
        );
    }
}

#[test]
fn flora_grows_by_biome() {
    let settings = v7(0xF10A);
    let mut trees = 0;
    let mut cacti = 0;
    for x in -8_000..8_000 {
        let Some(spot) = flora_at(&settings, x) else {
            continue;
        };
        let biome = biome_at(&settings, x);
        match spot.flora {
            Flora::Cactus => {
                cacti += 1;
                assert!(biome.is_dry(), "cactus in {biome:?}");
            }
            Flora::Oak | Flora::Pine | Flora::SnowPine => {
                trees += 1;
                assert!(!biome.is_dry(), "tree in {biome:?}");
                assert!(!column(&settings, x).flat, "tree on flat ground at {x}");
            }
            _ => {}
        }
    }
    assert!(trees > 200 && cacti > 10, "{trees} trees, {cacti} cacti");
    assert!(flora_at(&WorldSettings::bounded(1), 40).is_none());
}

#[test]
#[ignore = "calibration helper: prints the skyline of a V7 world"]
fn print_v7_skyline() {
    let seed = std::env::var("FATHERLOAD_SEED")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(7);
    let settings = v7(seed);
    let step = 12;
    let columns: Vec<i32> = (-4_000..4_000).step_by(step).collect();
    for row in (SKY_ROWS - MAX_ELEVATION_ROWS..=SKY_ROWS + MAX_DIP_ROWS).step_by(3) {
        let line: String = columns
            .iter()
            .map(|x| {
                let column = column(&settings, *x);
                if row >= column.surface_row {
                    column.biome.name().chars().next().unwrap_or('?')
                } else if lake_row(&settings, *x).is_some_and(|lake| row >= lake) {
                    '~'
                } else {
                    ' '
                }
            })
            .collect();
        println!("{row:>4} {line}");
    }
}
