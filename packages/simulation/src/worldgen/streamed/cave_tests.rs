//! Tests of cave ecosystems (Caves 2.0, V6): determinism, seamless chunks, every kind appears,
//! reachable interiors, fair loot and clearances.

use std::sync::Arc;

use super::*;
use crate::{
    cave::{CaveKind, CaveTile, cache_loot},
    terrain::{TileKind, tile_hardness},
    worldgen::{GeneratorVersion, WorldSettings},
};

/// Seeds the cave suite samples (`FATHERLOAD_WORLDGEN_SEEDS` raises the count).
fn sample_seeds(default: u64) -> impl Iterator<Item = u64> {
    let count = std::env::var("FATHERLOAD_WORLDGEN_SEEDS")
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(default);
    (0..count).map(|index| index.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xCA7E_0006)
}

fn window(settings: &WorldSettings, half_width: i32) -> TileRect {
    TileRect {
        min: TilePosition {
            x: -half_width,
            y: 0,
        },
        max: TilePosition {
            x: half_width,
            y: settings.height_tiles() - 1,
        },
    }
}

fn biomes(settings: &WorldSettings, area: &TileRect) -> Vec<Arc<CaveRegion>> {
    caves_touching(settings, area)
        .into_iter()
        .filter(|region| region.biome.is_some())
        .collect()
}

#[test]
fn every_cave_kind_appears_on_every_seed() {
    for seed in sample_seeds(3) {
        let settings = v6(seed);
        let found: std::collections::BTreeSet<CaveKind> = features_in(
            &settings,
            &window(&settings, 2_500),
            Some(&[FeatureKind::Cave]),
        )
        .iter()
        .filter_map(|feature| feature.detail.cave())
        .collect();
        for kind in CaveKind::ALL {
            assert!(found.contains(&kind), "seed {seed:#x}: no {kind:?}");
        }
    }
}

#[test]
fn caves_are_frequent_but_not_everywhere() {
    for seed in sample_seeds(3) {
        let settings = v6(seed);
        let caves = biomes(&settings, &window(&settings, 500)).len();
        // About 13 region rows by 10-11 region columns per 1,000 columns.
        assert!(
            (25..=110).contains(&caves),
            "seed {seed:#x}: {caves} caves per 1,000 columns"
        );
    }
}

#[test]
fn cave_interiors_are_open_connected_and_reachable() {
    for seed in sample_seeds(4) {
        let settings = v6(seed);
        for region in biomes(&settings, &window(&settings, 700)) {
            let biome = region.biome.as_ref().expect("biome");
            let feature = biome.feature;
            let interior: Vec<TilePosition> = biome
                .cells()
                .filter(|(_, cell)| matches!(cell, BiomeCell::Open | BiomeCell::Water))
                .map(|(position, _)| position)
                .collect();
            assert_eq!(
                interior.len(),
                usize::try_from(feature.size).expect("size"),
                "{:?} {:?}",
                biome.kind,
                feature.center
            );
            // One 4-connected area.
            let set: std::collections::BTreeSet<TilePosition> = interior.iter().copied().collect();
            let mut seen = std::collections::BTreeSet::from([interior[0]]);
            let mut queue = std::collections::VecDeque::from([interior[0]]);
            while let Some(at) = queue.pop_front() {
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let next = TilePosition {
                        x: at.x + dx,
                        y: at.y + dy,
                    };
                    if set.contains(&next) && seen.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
            assert_eq!(
                seen.len(),
                set.len(),
                "{:?} at {:?} is split",
                biome.kind,
                feature.center
            );
            // The centre is inside, and every interior tile generates open.
            assert!(set.contains(&feature.center));
            for position in &interior {
                assert_eq!(
                    generated_tile(&settings, *position).kind,
                    TileKind::Air,
                    "{:?} at {position:?}",
                    biome.kind
                );
            }
            // Nothing in the footprint is undrillable or harder than the cave's band drill.
            let strength = feature.band.required_drill_strength();
            for (position, cell) in biome.cells() {
                if cell == BiomeCell::Outside {
                    continue;
                }
                let tile = generated_tile(&settings, position).kind;
                assert!(!crate::terrain::is_indestructible(tile), "{position:?}");
                if let TileKind::Cave(_) = tile {
                    let local = crate::worldgen::strata::band_at(&settings, position.x, position.y)
                        .required_drill_strength();
                    assert!(
                        tile_hardness(tile) <= strength.max(local),
                        "{tile:?} in {:?} at {position:?}",
                        feature.band
                    );
                }
            }
        }
    }
}

#[test]
fn caves_keep_clear_of_spawn_the_starter_field_and_points_of_interest() {
    for seed in sample_seeds(4) {
        let settings = v6(seed);
        for region in biomes(&settings, &window(&settings, 900)) {
            let biome = region.biome.as_ref().expect("biome");
            let bounds = biome.bounds();
            if let Some(poi) = pois_touching(&settings, &bounds.expanded(3)).first() {
                panic!(
                    "{:?} at {bounds:?} touches {:?}",
                    biome.kind,
                    poi.poi_kind()
                );
            }
            for (position, cell) in biome.cells() {
                if matches!(cell, BiomeCell::Open | BiomeCell::Water) {
                    assert!(
                        crate::worldgen::caves::cave_allowed(position.x, position.y),
                        "{position:?}"
                    );
                    assert!(!regions::STARTER_AREA.contains(position), "{position:?}");
                }
            }
        }
    }
}

#[test]
fn cave_loot_is_balanced() {
    let mut caves = 0_u32;
    let mut with_loot = 0_u32;
    for seed in sample_seeds(3) {
        let settings = v6(seed);
        for region in biomes(&settings, &window(&settings, 900)) {
            let biome = region.biome.as_ref().expect("biome");
            caves += 1;
            let (mut caches, mut relics, mut ore_value, mut cache_value) = (0, 0, 0_u32, 0_u32);
            for (position, cell) in biome.cells() {
                let BiomeCell::Solid(kind) = cell else {
                    continue;
                };
                match kind {
                    TileKind::Cave(tile) if tile.is_cache() => {
                        let band =
                            crate::worldgen::strata::band_at(&settings, position.x, position.y);
                        let loot = cache_loot(settings.seed, position, band, tile);
                        assert!(loot.credits > 0 && !loot.items.is_empty());
                        cache_value += loot.value();
                        caches += 1;
                        if tile == CaveTile::RelicCache {
                            relics += 1;
                        }
                    }
                    TileKind::Ore(mineral) => ore_value += mineral.value(),
                    _ => {}
                }
            }
            assert!(caches <= 1, "{:?} holds {caches} caches", biome.kind);
            if biome.kind == CaveKind::AncientRuins {
                assert_eq!(relics, 1, "ruins hold one relic cache");
            } else {
                assert_eq!(relics, 0);
            }
            // A cave is a top-up, never a jackpot.
            assert!(
                cache_value + ore_value <= 9_000,
                "{:?} in {:?}: {cache_value} + {ore_value}",
                biome.kind,
                biome.feature.band
            );
            if cache_value + ore_value > 0 {
                with_loot += 1;
            }
        }
    }
    // Most caves are worth exploring.
    assert!(
        with_loot * 100 >= caves * 60,
        "{with_loot} of {caves} caves hold loot"
    );
}

#[test]
fn water_lives_only_in_flooded_caves_and_lies_still() {
    for seed in sample_seeds(3) {
        let settings = v6(seed);
        for region in biomes(&settings, &window(&settings, 900)) {
            let biome = region.biome.as_ref().expect("biome");
            let water: Vec<TilePosition> = biome
                .cells()
                .filter(|(_, cell)| *cell == BiomeCell::Water)
                .map(|(position, _)| position)
                .collect();
            if biome.kind != CaveKind::FloodedCave {
                assert!(water.is_empty(), "{:?} holds water", biome.kind);
                continue;
            }
            assert!(!water.is_empty());
            for position in &water {
                // Still water: never air under water.
                let below = biome.cell(position.x, position.y + 1);
                assert_ne!(below, BiomeCell::Open, "air under water at {position:?}");
                assert!(cave_at(&settings, *position).is_some_and(|sample| sample.water));
            }
        }
    }
}

#[test]
fn v6_generation_is_order_independent_and_seamless() {
    let settings = v6(0x0CA7_E5EA);
    let chunks: Vec<ChunkPosition> = (-6..6)
        .flat_map(|x| (4..24).map(move |y| ChunkPosition { x, y }))
        .collect();
    clear_caches();
    let forward: Vec<Vec<Tile>> = chunks
        .iter()
        .map(|chunk| generated_chunk(&settings, *chunk).tiles().to_vec())
        .collect();
    clear_caches();
    let mut backward: Vec<Vec<Tile>> = chunks
        .iter()
        .rev()
        .map(|chunk| generated_chunk(&settings, *chunk).tiles().to_vec())
        .collect();
    backward.reverse();
    assert_eq!(forward, backward);
    // Two cave-layer windows of different extents agree everywhere they overlap.
    let ctx = base::Ctx::new(&settings);
    let small = TileRect {
        min: TilePosition { x: -61, y: 70 },
        max: TilePosition { x: 33, y: 230 },
    };
    let mut a = window::Window::new(&ctx, &small);
    let mut b = window::Window::new(&ctx, &small.expanded(57));
    for position in small.positions() {
        assert_eq!(
            a.kind(position.x, position.y),
            b.kind(position.x, position.y)
        );
    }
}

#[test]
fn older_generators_have_no_caves() {
    let settings = WorldSettings {
        generator: GeneratorVersion::V5,
        ..WorldSettings::new(0x0CA7)
    };
    assert!(
        features_in(
            &settings,
            &window(&settings, 600),
            Some(&[FeatureKind::Cave])
        )
        .is_empty()
    );
    for x in (-600..600).step_by(7) {
        for y in (0..settings.height_tiles()).step_by(5) {
            assert!(cave_at(&settings, TilePosition { x, y }).is_none());
        }
    }
}

#[test]
fn cave_features_resolve_by_id() {
    let settings = v6(0x1D6);
    for found in features_in(
        &settings,
        &window(&settings, 400),
        Some(&[FeatureKind::Cave]),
    ) {
        assert_eq!(feature(&settings, found.id), Some(found));
        let sample = cave_at(&settings, found.center).expect("centre inside its cave");
        assert_eq!(sample.feature, found.id);
        assert!(sample.interior);
        assert_eq!(Some(sample.kind), found.detail.cave());
    }
}

#[test]
fn golden_v6_chunk_digests_are_pinned() {
    // Pinned like V5: once V6 worlds are in saves, changing what V6 generates must be a new
    // generator version (the wave 2 integrator collapses wave 2's generator changes first).
    let settings = v6(crate::worldgen::DEFAULT_WORLD_SEED);
    let mut combined = 0_u64;
    // Chunks spread over the town's region plus the chunks holding the first caves' centres.
    let mut chunks = vec![
        ChunkPosition { x: 0, y: 6 },
        ChunkPosition { x: -20, y: 31 },
        ChunkPosition { x: 3_000, y: 12 },
    ];
    chunks.extend(
        features_in(
            &settings,
            &window(&settings, 600),
            Some(&[FeatureKind::Cave]),
        )
        .iter()
        .take(8)
        .map(|feature| ChunkPosition::of(feature.center)),
    );
    assert!(chunks.len() > 8, "the window holds caves");
    for chunk in chunks {
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        for tile in generated_chunk(&settings, chunk).tiles() {
            for byte in tile.code().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01B3);
            }
        }
        combined = combined.rotate_left(7) ^ hash;
    }
    assert_eq!(
        combined, GOLDEN_V6_DIGEST,
        "V6 generation changed: {combined:#018x}"
    );
}

const GOLDEN_V6_DIGEST: u64 = 0x5731_7de3_5f46_e4c6;

fn v6(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V6,
        ..WorldSettings::new(seed)
    }
}

fn glyph(kind: TileKind, water: bool) -> char {
    match kind {
        TileKind::Air if water => '~',
        TileKind::Air => ' ',
        TileKind::Dirt | TileKind::Clay => ':',
        TileKind::Stone | TileKind::HardRock => '#',
        TileKind::Lava => 'L',
        TileKind::Gas => 'g',
        TileKind::Ore(_) => 'o',
        TileKind::Artifact(_) => 'a',
        TileKind::Bedrock => '=',
        TileKind::Poi(_) => 'P',
        TileKind::Cave(tile) => match tile {
            CaveTile::Crystal => '*',
            CaveTile::MushroomStem => '|',
            CaveTile::MushroomCap => 'n',
            CaveTile::GlowCap => ',',
            CaveTile::Mycelium => 'm',
            CaveTile::Obsidian => 'O',
            CaveTile::RuinBrick => 'B',
            CaveTile::RuinPillar => 'I',
            CaveTile::RuinGlyph => 'G',
            CaveTile::Stalactite => 'v',
            CaveTile::TreasureCache => '$',
            CaveTile::RelicCache => '&',
            CaveTile::FossilRib => ')',
            CaveTile::GlowAlgae => '%',
            CaveTile::Silt => '_',
        },
        _ => '!',
    }
}

#[test]
#[ignore = "prints caves for review"]
fn print_caves() {
    let settings = v6(crate::worldgen::DEFAULT_WORLD_SEED);
    let area = TileRect {
        min: TilePosition { x: -1_500, y: 0 },
        max: TilePosition {
            x: 1_500,
            y: settings.height_tiles() - 1,
        },
    };
    let mut shown = std::collections::BTreeSet::new();
    for region in caves_touching(&settings, &area) {
        let Some(biome) = region.biome.as_ref() else {
            continue;
        };
        if !shown.insert(biome.kind) {
            continue;
        }
        let bounds = biome.bounds().expanded(2);
        println!(
            "{} at {:?} band {:?} size {}",
            biome.kind.name(),
            biome.feature.center,
            biome.feature.band,
            biome.feature.size
        );
        for y in bounds.min.y..=bounds.max.y {
            let line: String = (bounds.min.x..=bounds.max.x)
                .map(|x| {
                    let position = TilePosition { x, y };
                    let tile = generated_tile(&settings, position);
                    let water = cave_at(&settings, position).is_some_and(|s| s.water);
                    glyph(tile.kind, water)
                })
                .collect();
            println!("{line}");
        }
    }
}
