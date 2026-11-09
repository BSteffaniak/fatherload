//! Generator fairness suite: properties every seed must satisfy, plus golden hashes pinned to
//! the generator version.

use std::collections::VecDeque;

use super::{
    caves::cave_allowed,
    deposits::{BAND_BUDGET_PERCENT, STARTER_FIELD_VALUE, band_ore_target_per_ten_tiles},
    hazards::{SPAWN_HAZARD_CLEARANCE, TOWN_HAZARD_MIN_ROW},
    report::WorldStats,
    strata::SKY_ROWS,
    *,
};
use crate::terrain::{Terrain, TileKind, TilePosition, tile_hardness};

/// Seeds the property suite checks.
///
/// Generation is a few milliseconds per world in release and roughly ten times that in debug,
/// so by default the suite samples a spread of 32 seeds. Set `DRILLGAME_WORLDGEN_SEEDS` (e.g.
/// 300, with `--release`) for a deep run over many more.
fn sample_seeds() -> impl Iterator<Item = u64> {
    let count = std::env::var("DRILLGAME_WORLDGEN_SEEDS")
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(32);
    (0..count).map(|index| index.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5EED)
}

/// Standard settings pinned to [`GeneratorVersion::V1`].
const fn v1(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V1,
        ..WorldSettings::new(seed)
    }
}

/// Standard settings pinned to [`GeneratorVersion::V2`].
const fn v2(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V2,
        ..WorldSettings::new(seed)
    }
}

fn tile_at(world: &GeneratedWorld, x: i32, y: i32) -> Option<TileKind> {
    (x >= 0 && y >= 0 && x < world.width && y < world.height)
        .then(|| world.tiles[usize::try_from(y * world.width + x).unwrap_or(0)].kind)
}

fn digest(world: &GeneratedWorld) -> u64 {
    // FNV-1a over every tile code: order-dependent and platform independent.
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for tile in &world.tiles {
        for byte in tile.code().to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01B3);
        }
    }
    hash
}

#[test]
fn same_settings_generate_identical_worlds() {
    for seed in [1, 0xD111_6A4E, u64::MAX] {
        let settings = WorldSettings::new(seed);
        let first = generate(&settings);
        let second = generate(&settings);
        assert_eq!(digest(&first), digest(&second));
        assert_eq!(first.features, second.features);
    }
    let a = generate(&WorldSettings::new(1));
    let b = generate(&WorldSettings::new(2));
    assert_ne!(digest(&a), digest(&b), "different seeds must differ");
}

/// Golden digests of a few worlds for [`GeneratorVersion::V1`]. A change here means existing
/// generated worlds would change: bump the generator version instead of editing these.
#[test]
fn golden_world_digests_are_pinned_to_the_generator_version() {
    let cases = [
        (v1(DEFAULT_WORLD_SEED), 0xacad_3db4_cf17_715d),
        (v1(1), 0xe24f_c0a8_4dfb_6f08),
        (
            WorldSettings {
                size: WorldSize::Large,
                richness: ResourceRichness::Rich,
                hazards: HazardIntensity::Brutal,
                ..v1(0x00C0_FFEE)
            },
            0xea3e_f91a_6ab2_2d0c,
        ),
        (
            WorldSettings {
                richness: ResourceRichness::Lean,
                hazards: HazardIntensity::Calm,
                ..v1(77)
            },
            0x2c63_e545_6d83_9051,
        ),
    ];
    let actual: Vec<u64> = cases
        .iter()
        .map(|(settings, _)| digest(&generate(settings)))
        .collect();
    let expected: Vec<u64> = cases.iter().map(|(_, digest)| *digest).collect();
    assert_eq!(
        actual, expected,
        "generator v1 output changed; actual digests: {actual:#x?}"
    );
}

/// Golden digests of a few worlds for [`GeneratorVersion::V2`] (V1 plus points of interest).
#[test]
fn golden_v2_world_digests_are_pinned() {
    let cases = [
        (v2(DEFAULT_WORLD_SEED), 0xc3cb_282d_a845_6bca),
        (v2(1), 0x527b_dadf_83a0_c355),
        (
            WorldSettings {
                size: WorldSize::Large,
                richness: ResourceRichness::Rich,
                hazards: HazardIntensity::Brutal,
                ..v2(0x00C0_FFEE)
            },
            0xbde8_0bc6_70c9_8897,
        ),
    ];
    assert!(
        cases
            .iter()
            .all(|(settings, _)| settings.generator == GeneratorVersion::V2)
    );
    let actual: Vec<u64> = cases
        .iter()
        .map(|(settings, _)| digest(&generate(settings)))
        .collect();
    let expected: Vec<u64> = cases.iter().map(|(_, digest)| *digest).collect();
    assert_eq!(
        actual, expected,
        "generator v2 output changed; actual digests: {actual:#x?}"
    );
}

/// Golden digests of a few worlds for [`GeneratorVersion::V3`] (V2 with per-seed band
/// thicknesses and chunkier deep ore on rich worlds).
#[test]
fn golden_v3_world_digests_are_pinned() {
    let cases = [
        (
            WorldSettings::new(DEFAULT_WORLD_SEED),
            0x3b31_8f48_890b_8c15,
        ),
        (WorldSettings::new(1), 0xbee4_e60d_9d0e_2076),
        (
            WorldSettings {
                size: WorldSize::Large,
                richness: ResourceRichness::Rich,
                hazards: HazardIntensity::Brutal,
                ..WorldSettings::new(0x00C0_FFEE)
            },
            0x2138_ac79_7d36_7881,
        ),
    ];
    assert!(
        cases
            .iter()
            .all(|(settings, _)| settings.generator == GeneratorVersion::V3)
    );
    let actual: Vec<u64> = cases
        .iter()
        .map(|(settings, _)| digest(&generate(settings)))
        .collect();
    let expected: Vec<u64> = cases.iter().map(|(_, digest)| *digest).collect();
    assert_eq!(
        actual, expected,
        "generator v3 output changed; actual digests: {actual:#x?}"
    );
}

#[test]
fn legacy_settings_reproduce_the_original_generator() {
    let settings = WorldSettings::legacy(0xD1_11_6A_4E);
    let world = generate(&settings);
    let legacy = Terrain::new_seeded(world.width, world.height, settings.seed);
    assert_eq!(world.tiles.as_slice(), legacy.tiles());
    assert!(world.features.is_empty());
}

#[test]
fn worlds_have_the_configured_size() {
    let standard = generate(&WorldSettings::new(5));
    assert_eq!((standard.width, standard.height), (240, WORLD_HEIGHT_TILES));
    let large = generate(&WorldSettings {
        size: WorldSize::Large,
        ..WorldSettings::new(5)
    });
    assert_eq!((large.width, large.height), (360, WORLD_HEIGHT_TILES));
}

#[test]
fn the_surface_and_town_stay_fixed() {
    for seed in sample_seeds().take(8) {
        let world = generate(&WorldSettings::new(seed));
        for x in 0..world.width {
            for y in 0..SKY_ROWS {
                assert_eq!(tile_at(&world, x, y), Some(TileKind::Air));
            }
            let foundation = crate::surface::building_foundation_at(x, SKY_ROWS);
            assert_eq!(
                tile_at(&world, x, SKY_ROWS) == Some(TileKind::Foundation),
                foundation,
                "seed {seed} column {x}"
            );
            // The crust: no cave breaks the first rows of ground.
            for y in SKY_ROWS..caves::CRUST_ROWS {
                assert_ne!(tile_at(&world, x, y), Some(TileKind::Air), "seed {seed}");
            }
        }
    }
}

#[test]
fn caves_never_open_under_spawn_or_the_town() {
    for seed in sample_seeds() {
        let world = generate(&WorldSettings::new(seed));
        for y in SKY_ROWS..world.height {
            for x in 0..world.width {
                if !cave_allowed(x, y) {
                    assert_ne!(
                        tile_at(&world, x, y),
                        Some(TileKind::Air),
                        "seed {seed} opened ({x}, {y})"
                    );
                }
            }
        }
        // Straight below spawn the ground is solid for the whole shaft.
        for y in SKY_ROWS..caves::SPAWN_SHAFT_ROWS {
            assert_ne!(tile_at(&world, SPAWN_TILE_X, y), Some(TileKind::Air));
        }
    }
}

#[test]
fn no_hazard_sits_near_spawn_or_under_the_town() {
    let (town_first, town_last) = town_columns();
    for seed in sample_seeds() {
        let settings = WorldSettings {
            hazards: HazardIntensity::Brutal,
            ..WorldSettings::new(seed)
        };
        let world = generate(&settings);
        for y in 0..world.height {
            for x in 0..world.width {
                let Some(kind) = tile_at(&world, x, y) else {
                    continue;
                };
                if HazardKind::of_tile(kind).is_none() {
                    continue;
                }
                assert!(
                    y >= hazards::HAZARD_MIN_ROW,
                    "seed {seed} hazard at ({x}, {y})"
                );
                assert!(
                    (x - SPAWN_TILE_X).abs() > SPAWN_HAZARD_CLEARANCE
                        || y - SKY_ROWS > SPAWN_HAZARD_CLEARANCE,
                    "seed {seed} hazard near spawn at ({x}, {y})"
                );
                if (town_first..=town_last).contains(&x) {
                    assert!(y >= TOWN_HAZARD_MIN_ROW, "seed {seed} hazard under town");
                }
            }
        }
    }
}

#[test]
fn hazards_come_in_clusters_not_single_tiles() {
    for seed in sample_seeds().take(16) {
        let world = generate(&WorldSettings::new(seed));
        for y in 0..world.height {
            for x in 0..world.width {
                let Some(kind) = tile_at(&world, x, y) else {
                    continue;
                };
                if HazardKind::of_tile(kind).is_none() {
                    continue;
                }
                let neighbours = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .filter(|(dx, dy)| tile_at(&world, x + dx, y + dy) == Some(kind))
                    .count();
                assert!(
                    neighbours > 0,
                    "seed {seed} isolated {kind:?} at ({x}, {y})"
                );
            }
        }
        let clusters = world.features.count(FeatureKind::HazardCluster);
        assert!(
            clusters >= 20,
            "seed {seed}: only {clusters} hazard clusters"
        );
    }
}

#[test]
fn hazard_intensity_scales_hazards() {
    let count = |hazards: HazardIntensity| -> usize {
        sample_seeds()
            .take(6)
            .map(|seed| {
                let world = generate(&WorldSettings {
                    hazards,
                    ..WorldSettings::new(seed)
                });
                world.features.count(FeatureKind::HazardCluster)
            })
            .sum()
    };
    let (calm, standard, brutal) = (
        count(HazardIntensity::Calm),
        count(HazardIntensity::Standard),
        count(HazardIntensity::Brutal),
    );
    assert!(
        calm * 3 / 2 < standard,
        "calm {calm} vs standard {standard}"
    );
    assert!(
        standard * 5 / 4 < brutal,
        "standard {standard} vs brutal {brutal}"
    );
}

#[test]
fn the_starter_field_always_pays_for_the_first_upgrades() {
    for richness in ResourceRichness::ALL {
        for seed in sample_seeds().take(16) {
            let settings = WorldSettings {
                richness,
                ..WorldSettings::new(seed)
            };
            let world = generate(&settings);
            let stats = WorldStats::measure(&settings, &world);
            assert!(
                stats.starter_field_value >= STARTER_FIELD_VALUE,
                "seed {seed} {richness:?}: starter field worth {}",
                stats.starter_field_value
            );
        }
    }
}

#[test]
fn ore_value_per_band_stays_in_its_target_range() {
    for richness in ResourceRichness::ALL {
        for seed in sample_seeds().take(16) {
            let settings = WorldSettings {
                richness,
                ..WorldSettings::new(seed)
            };
            let world = generate(&settings);
            let stats = WorldStats::measure(&settings, &world);
            for band in Band::ALL {
                let band_stats = stats.bands[band.index()];
                let per_ten = band_stats.ore_value * 10 / band_stats.tiles.max(1);
                let target =
                    band_ore_target_per_ten_tiles(band) * u64::from(richness.percent()) / 100;
                // Topped up to the budget (a cluster's worth of slack for rounding), and never
                // more than about twice the target.
                let floor = target * BAND_BUDGET_PERCENT / 100 * 9 / 10;
                assert!(
                    per_ten >= floor,
                    "seed {seed} {richness:?} {band:?}: {per_ten} per 10 tiles < {floor}"
                );
                assert!(
                    per_ten <= target * 5 / 2 + 10,
                    "seed {seed} {richness:?} {band:?}: {per_ten} per 10 tiles > {target} x2.5"
                );
            }
        }
    }
}

/// Rich V3 worlds keep their deep ore in whole deposits instead of one-to-three-tile specks.
#[test]
fn rich_deep_strata_hold_few_ore_specks() {
    let specks = |generator: GeneratorVersion| -> u64 {
        sample_seeds()
            .take(6)
            .map(|seed| {
                let settings = WorldSettings {
                    richness: ResourceRichness::Rich,
                    generator,
                    ..WorldSettings::new(seed)
                };
                let world = generate(&settings);
                WorldStats::measure(&settings, &world)
                    .bands
                    .iter()
                    .skip(Band::Deep(crate::terrain::DeepStratum::CrystalFaults).index())
                    .map(|band| band.ore_specks)
                    .sum::<u64>()
            })
            .sum()
    };
    let (v2, v3) = (specks(GeneratorVersion::V2), specks(GeneratorVersion::V3));
    assert!(v3 * 10 < v2, "v3 {v3} specks vs v2 {v2}");
}

#[test]
fn richness_scales_ore() {
    let total = |richness: ResourceRichness| -> u64 {
        sample_seeds()
            .take(6)
            .map(|seed| {
                let settings = WorldSettings {
                    richness,
                    ..WorldSettings::new(seed)
                };
                let world = generate(&settings);
                WorldStats::measure(&settings, &world)
                    .bands
                    .iter()
                    .map(|band| band.ore_value)
                    .sum::<u64>()
            })
            .sum()
    };
    let (lean, standard, rich) = (
        total(ResourceRichness::Lean),
        total(ResourceRichness::Standard),
        total(ResourceRichness::Rich),
    );
    assert!(
        lean * 10 < standard * 9,
        "lean {lean} vs standard {standard}"
    );
    assert!(
        standard * 11 < rich * 10,
        "standard {standard} vs rich {rich}"
    );
}

/// Ore only ever appears where the band's own drill can dig it.
#[test]
fn ore_never_needs_a_stronger_drill_than_its_band() {
    for seed in sample_seeds().take(16) {
        let settings = WorldSettings::new(seed);
        let world = generate(&settings);
        for y in SKY_ROWS..world.height {
            for x in 0..world.width {
                if let Some(kind @ TileKind::Ore(_)) = tile_at(&world, x, y) {
                    let band = band_at(&settings, x, y);
                    assert!(
                        tile_hardness(kind) <= band.required_drill_strength(),
                        "seed {seed}: {kind:?} in {band:?} at ({x}, {y})"
                    );
                }
            }
        }
    }
}

/// Every band can be crossed with the drill that band requires: a path of diggable or open
/// tiles (never through hazards, foundations or rock harder than the band's drill) leads from
/// the spawn point to the bottom row.
#[test]
fn every_band_is_reachable_with_its_own_drill() {
    for seed in sample_seeds() {
        let settings = WorldSettings::new(seed);
        let world = generate(&settings);
        let passable = |x: i32, y: i32| -> bool {
            let Some(kind) = tile_at(&world, x, y) else {
                return false;
            };
            if kind == TileKind::Foundation || HazardKind::of_tile(kind).is_some() {
                return false;
            }
            kind == TileKind::Air
                || tile_hardness(kind) <= band_at(&settings, x, y).required_drill_strength()
        };
        let mut seen = vec![false; world.tiles.len()];
        let start = (SPAWN_TILE_X, SKY_ROWS - 1);
        let mut queue = VecDeque::from([start]);
        seen[usize::try_from(start.1 * world.width + start.0).unwrap_or(0)] = true;
        let mut deepest_per_band = [false; 12];
        while let Some((x, y)) = queue.pop_front() {
            deepest_per_band[band_at(&settings, x, y).index()] = true;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= world.width || ny >= world.height {
                    continue;
                }
                let index = usize::try_from(ny * world.width + nx).unwrap_or(0);
                if !seen[index] && passable(nx, ny) {
                    seen[index] = true;
                    queue.push_back((nx, ny));
                }
            }
        }
        let bottom = world.height - 1;
        let reached_bottom =
            (0..world.width).any(|x| seen[usize::try_from(bottom * world.width + x).unwrap_or(0)]);
        assert!(reached_bottom, "seed {seed}: the bottom is sealed off");
        for band in Band::ALL {
            assert!(
                deepest_per_band[band.index()],
                "seed {seed}: {band:?} unreachable"
            );
        }
    }
}

/// The first band is diggable with the starting drill all the way from spawn: no clay lump
/// walls the starter field off.
#[test]
fn the_starting_drill_reaches_the_clay_line_from_spawn() {
    for seed in sample_seeds() {
        let settings = WorldSettings::new(seed);
        let world = generate(&settings);
        let mut y = SKY_ROWS;
        while band_at(&settings, SPAWN_TILE_X, y) == Band::Topsoil {
            let kind = tile_at(&world, SPAWN_TILE_X, y).unwrap_or(TileKind::Air);
            assert!(tile_hardness(kind) <= 1, "seed {seed}: {kind:?} at row {y}");
            y += 1;
        }
    }
}

#[test]
fn artifacts_exist_in_every_stratum_below_the_topsoil() {
    for seed in sample_seeds().take(16) {
        let settings = WorldSettings::new(seed);
        let world = generate(&settings);
        let stats = WorldStats::measure(&settings, &world);
        for band in Band::ALL.into_iter().skip(1) {
            assert!(
                stats.bands[band.index()].artifacts >= 2,
                "seed {seed}: {band:?} has {} artifacts",
                stats.bands[band.index()].artifacts
            );
        }
        assert!(
            world
                .features
                .in_band(Band::Deep(crate::terrain::DeepStratum::MantleStormZone))
                .any(|feature| feature.kind == FeatureKind::ArtifactBed)
                || stats.bands[11].artifacts >= 2
        );
    }
}

#[test]
fn the_feature_index_counts_stay_in_bounds() {
    for seed in sample_seeds() {
        for size in WorldSize::ALL {
            let settings = WorldSettings {
                size,
                ..WorldSettings::new(seed)
            };
            let world = generate(&settings);
            let features = &world.features;
            let scale = usize::try_from(settings.width_tiles() / 120).unwrap_or(2);
            let bounds = [
                (FeatureKind::LargeCavern, 4 * scale, 30 * scale),
                (FeatureKind::Chamber, 0, 10 * scale),
                (FeatureKind::Motherlode, 0, 8 * scale),
                (FeatureKind::ArtifactBed, 10 * scale, 24 * scale),
                (FeatureKind::HazardCluster, 30 * scale, 120 * scale),
                (FeatureKind::StratumTransition, 11, 11),
                (FeatureKind::PrefabSlot, 4 * scale, 16 * scale),
                (FeatureKind::PointOfInterest, 6 * scale + 12, 24 * scale),
            ];
            for (kind, low, high) in bounds {
                let count = features.count(kind);
                assert!(
                    (low..=high).contains(&count),
                    "seed {seed} {size:?}: {count} {kind:?} not in {low}..={high}"
                );
            }
            // Ids are positions and every feature lies inside the world.
            for (index, feature) in features.all().iter().enumerate() {
                assert_eq!(feature.id.0 as usize, index);
                assert!(feature.center.x >= 0 && feature.center.x < world.width);
                assert!(feature.center.y >= 0 && feature.center.y < world.height);
            }
        }
    }
}

#[test]
fn feature_queries_filter_by_kind_depth_and_region() {
    let settings = WorldSettings::new(9);
    let world = generate(&settings);
    let features = &world.features;
    let deep: Vec<_> = features.in_depth(100..=219).collect();
    assert!(deep.iter().all(|feature| feature.center.y >= 100));
    let region = TileRect {
        min: TilePosition { x: 0, y: 0 },
        max: TilePosition { x: 119, y: 219 },
    };
    assert!(
        features
            .in_region(region)
            .all(|feature| feature.bounds.min.x <= 119)
    );
    let transition = features
        .of_kind(FeatureKind::StratumTransition)
        .find(|feature| {
            matches!(
                feature.detail,
                FeatureDetail::Transition {
                    upper: Band::Basalt,
                    ..
                }
            )
        })
        .expect("the basalt transition exists");
    let (shallowest, deepest) = strata::boundary_depth_range(&settings, Band::Basalt.index());
    assert!((shallowest..=deepest).contains(&transition.center.y));
    assert_eq!(
        features.nearest(transition.center, FeatureKind::StratumTransition),
        Some(transition)
    );
    for feature in features.of_kind(FeatureKind::Motherlode) {
        assert!(matches!(feature.detail, FeatureDetail::Mineral(_)));
        assert_eq!(features.get(feature.id), Some(feature));
    }
}

#[test]
fn seeds_round_trip_through_text() {
    for seed in [0, 1, 0xD111_6A4E, 0xFFFF_FFFF_FFFF_FFFF, 0x1234_5678_9ABC] {
        assert_eq!(parse_seed(&format_seed(seed)), Some(seed));
    }
    assert_eq!(format_seed(0xD111_6A4E), "D111-6A4E");
    assert_eq!(parse_seed("  d111 6a4e "), Some(0xD111_6A4E));
    assert_eq!(parse_seed("0xFF"), Some(255));
    assert_eq!(parse_seed(""), None);
    assert_eq!(parse_seed("deep rocks"), parse_seed("deep rocks"));
    assert_ne!(parse_seed("deep rocks"), parse_seed("deep rock"));
}

#[test]
#[ignore = "calibration helper: prints fractal noise percentiles"]
fn print_noise_percentiles() {
    let mut values = Vec::new();
    for seed in 0..8 {
        for y in 0..200 {
            for x in 0..300 {
                values.push(noise::fractal_noise(seed, 77, x, y, 17, 9, 3));
            }
        }
    }
    values.sort_unstable();
    for share in [20, 50, 100, 150, 200, 300, 400, 500, 700] {
        let index = values.len() * (1000 - share) / 1000;
        println!("({share}, {}),", values[index]);
    }
}

#[test]
fn a_seed_builds_the_same_world_at_every_tick_rate() {
    use crate::{PlayerId, SimulationParams, WorldState};
    let settings = WorldSettings {
        size: WorldSize::Large,
        ..WorldSettings::new(0xFEED)
    };
    let build = |hz| {
        WorldState::new_game_with_settings(PlayerId::new(1), &settings, SimulationParams::new(hz))
    };
    let reference = build(60);
    for hz in [30, 120, 240] {
        let world = build(hz);
        assert_eq!(
            world.terrain().digest(),
            reference.terrain().digest(),
            "{hz} Hz"
        );
        assert_eq!(world.world_settings(), reference.world_settings());
        assert_eq!(world.world_features(), reference.world_features());
    }
}

#[test]
fn replicas_receive_the_world_settings_in_the_session_section() {
    use crate::{PlayerId, SectionKey, SimulationParams, WorldState};
    let settings = WorldSettings {
        hazards: HazardIntensity::Calm,
        ..WorldSettings::new(42)
    };
    let authority = WorldState::new_game_with_settings(
        PlayerId::new(1),
        &settings,
        SimulationParams::default(),
    );
    let mut replica = WorldState::new_game(PlayerId::new(1));
    assert_ne!(
        replica.section_hash(SectionKey::Session),
        authority.section_hash(SectionKey::Session)
    );
    let payload = authority
        .section(SectionKey::Session)
        .expect("session section");
    replica.apply_section(SectionKey::Session, payload);
    assert_eq!(*replica.world_settings(), settings);
    assert_eq!(
        replica.section_hash(SectionKey::Session),
        authority.section_hash(SectionKey::Session)
    );
}

#[test]
fn the_state_hash_covers_the_world_settings() {
    use crate::{PlayerId, SimulationParams, WorldState};
    let plain = WorldState::new_game_with_settings(
        PlayerId::new(1),
        &WorldSettings::new(5),
        SimulationParams::default(),
    );
    let mut relabelled = plain.clone();
    relabelled.terrain_mut().set_settings(WorldSettings {
        richness: ResourceRichness::Rich,
        ..WorldSettings::new(5)
    });
    assert_ne!(
        plain.simulation_state_hash(),
        relabelled.simulation_state_hash()
    );
}

#[test]
fn loaded_terrain_recomputes_the_same_feature_index() {
    let settings = WorldSettings::new(31);
    let terrain = Terrain::generate(&settings);
    let json = serde_json::to_string(&terrain).expect("serialize");
    let loaded: Terrain = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(loaded.settings(), &settings);
    assert_eq!(loaded.digest(), terrain.digest());
    assert_eq!(loaded.features(), terrain.features());
    assert!(!loaded.features().is_empty());
}
