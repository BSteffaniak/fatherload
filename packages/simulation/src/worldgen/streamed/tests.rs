//! Tests of streamed (V5) generation: determinism, seamless chunk borders, order independence,
//! distant chunks, fairness and the regional feature index.

use super::{
    window::{POI_CAVE_SHIELD, Window},
    *,
};
use crate::{
    terrain::{MineralKind, TileKind, tile_hardness},
    worldgen::{
        GeneratorVersion, HazardIntensity, ResourceRichness, SPAWN_TILE_X,
        caves::{SPAWN_SHAFT_HALF_WIDTH, SPAWN_SHAFT_ROWS},
        deposits::{STARTER_FIELD_VALUE, band_ore_target_per_ten_tiles, starter_field_value},
        features::HazardKind,
        hazards::hazard_allowed,
        pois::{HAZARD_CLEARANCE, POI_SPACING, clear_of_spawn_and_town},
        strata::{Band, SKY_ROWS},
        town_columns,
    },
};

fn v5(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V5,
        ..WorldSettings::new(seed)
    }
}

/// Seeds the streamed suite samples (`FATHERLOAD_WORLDGEN_SEEDS` raises the count).
fn sample_seeds(default: u64) -> impl Iterator<Item = u64> {
    let count = std::env::var("FATHERLOAD_WORLDGEN_SEEDS")
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(default);
    (0..count).map(|index| index.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5EED_0005)
}

fn chunk_digest(tiles: &[Tile]) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for tile in tiles {
        for byte in tile.code().to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01B3);
        }
    }
    hash
}

fn rect(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> TileRect {
    TileRect {
        min: TilePosition { x: min_x, y: min_y },
        max: TilePosition { x: max_x, y: max_y },
    }
}

/// Chunks spread over the town, both sides, the deep strata and far away.
fn sample_chunks() -> Vec<ChunkPosition> {
    let mut chunks = Vec::new();
    for (x, y) in [
        (0, 0),
        (6, 1),
        (-1, 2),
        (-7, 5),
        (14, 9),
        (3, 20),
        (-12, 33),
        (8, 52),
        (3_125, 4),
        (-3_126, 30),
    ] {
        chunks.push(ChunkPosition { x, y });
    }
    chunks
}

#[test]
fn chunks_do_not_depend_on_generation_order_or_caches() {
    let settings = v5(0x0DE7);
    clear_caches();
    let forward: Vec<u64> = sample_chunks()
        .into_iter()
        .map(|chunk| chunk_digest(generated_chunk(&settings, chunk).tiles()))
        .collect();
    clear_caches();
    let mut backward: Vec<u64> = sample_chunks()
        .into_iter()
        .rev()
        .map(|chunk| chunk_digest(generated_chunk(&settings, chunk).tiles()))
        .collect();
    backward.reverse();
    assert_eq!(forward, backward);
}

#[test]
fn golden_v5_chunk_digests_are_pinned() {
    // Pinned to the V5 generator: any change to streamed generation must be a new generator
    // version, or existing V5 worlds would load different terrain (their saves would then fail
    // the generator probe instead).
    let settings = v5(crate::worldgen::DEFAULT_WORLD_SEED);
    let digests: Vec<u64> = sample_chunks()
        .into_iter()
        .map(|chunk| chunk_digest(generated_chunk(&settings, chunk).tiles()))
        .collect();
    let combined = digests
        .iter()
        .fold(0_u64, |sum, digest| sum.rotate_left(7) ^ digest);
    assert_eq!(
        combined, GOLDEN_V5_DIGEST,
        "V5 generation changed: {combined:#018x}"
    );
}

const GOLDEN_V5_DIGEST: u64 = 0x10b1_0bcd_e70c_184a;

#[test]
fn overlapping_windows_agree_on_every_shared_tile() {
    // The cave layer is what region placement reads: two windows of different extents must see
    // the same tiles wherever they overlap, or chunk borders would show seams.
    for seed in sample_seeds(3) {
        let ctx = Ctx::new(&v5(seed));
        let small = rect(-37, 120, 21, 190);
        let large = rect(-90, 80, 70, 260);
        let mut a = Window::new(&ctx, &small);
        let mut b = Window::new(&ctx, &large);
        for position in small.positions() {
            assert_eq!(
                a.kind(position.x, position.y),
                b.kind(position.x, position.y),
                "seed {seed} at {position:?}"
            );
        }
    }
}

#[test]
fn chunk_borders_are_seamless() {
    // A window over a chunk border: every tile of the assembled chunks matches the cave layer
    // or a later layer's write, never a different cave layer (no half-carved tunnels).
    let settings = v5(0x5EA);
    let ctx = Ctx::new(&settings);
    let area = rect(-48, 96, 47, 191);
    let mut window = Window::new(&ctx, &area);
    for position in area.positions() {
        let tile = generated_tile(&settings, position).kind;
        let cave = window.kind(position.x, position.y);
        if cave == TileKind::Air {
            // Only points of interest (and their air cells) and hazards may fill a cave tile.
            assert!(
                matches!(tile, TileKind::Air | TileKind::Poi(_))
                    || HazardKind::of_tile(tile).is_none()
                    || tile == TileKind::Air,
                "{position:?}: cave filled with {tile:?}"
            );
        }
    }
}

#[test]
fn distant_chunks_generate_like_near_ones() {
    let settings = v5(0xFA4);
    let ctx = Ctx::new(&settings);
    for x in [
        -50_000,
        50_000,
        STREAMED_COLUMN_LIMIT - 1,
        -STREAMED_COLUMN_LIMIT,
    ] {
        let floor = ctx.floor_row(x);
        assert!(floor < settings.height_tiles());
        let column: Vec<TileKind> = (0..settings.height_tiles())
            .map(|y| generated_tile(&settings, TilePosition { x, y }).kind)
            .collect();
        assert_eq!(column[0], TileKind::Air, "sky at {x}");
        assert!(
            column[usize::try_from(floor).expect("row")..]
                .iter()
                .all(|kind| *kind == TileKind::Bedrock),
            "bedrock floor at {x}"
        );
        assert!(column.iter().any(|kind| matches!(kind, TileKind::Ore(_))));
        let again = generated_tile(&settings, TilePosition { x, y: 300 });
        assert_eq!(again.kind, column[300]);
    }
}

#[test]
fn the_town_stays_flat_and_its_foundations_fixed() {
    for seed in sample_seeds(6) {
        let settings = v5(seed);
        for x in TOWN_FLAT_FIRST_COLUMN..=TOWN_FLAT_LAST_COLUMN {
            assert_eq!(surface_row(&settings, x), SKY_ROWS);
            for y in 0..SKY_ROWS {
                assert_eq!(
                    generated_tile(&settings, TilePosition { x, y }).kind,
                    TileKind::Air
                );
            }
        }
        for x in -2_000..2_000 {
            let row = surface_row(&settings, x);
            assert!((SKY_ROWS..=SKY_ROWS + MAX_SURFACE_DIP_ROWS).contains(&row));
        }
        let (first, last) = town_columns();
        for x in first..=last {
            for y in SKY_ROWS..SKY_ROWS + 3 {
                let kind = generated_tile(&settings, TilePosition { x, y }).kind;
                assert_eq!(
                    kind == TileKind::Foundation,
                    crate::surface::building_foundation_at(x, y),
                    "seed {seed} ({x}, {y}) {kind:?}"
                );
            }
        }
        // The spawn shaft is solid ground: no caves and no hazards.
        for y in SKY_ROWS..SPAWN_SHAFT_ROWS {
            for x in SPAWN_TILE_X - SPAWN_SHAFT_HALF_WIDTH..=SPAWN_TILE_X + SPAWN_SHAFT_HALF_WIDTH {
                let kind = generated_tile(&settings, TilePosition { x, y }).kind;
                assert_ne!(kind, TileKind::Air, "seed {seed} shaft open at ({x}, {y})");
                assert!(HazardKind::of_tile(kind).is_none());
            }
        }
    }
}

#[test]
fn the_starter_field_always_pays_for_the_first_upgrades() {
    for richness in ResourceRichness::ALL {
        for seed in sample_seeds(8) {
            let settings = WorldSettings {
                richness,
                ..v5(seed)
            };
            let value = starter_field_value(|x, y| {
                Some(generated_tile(&settings, TilePosition { x, y }).kind)
            });
            assert!(
                value >= STARTER_FIELD_VALUE,
                "seed {seed} {richness:?}: starter field worth {value}"
            );
        }
    }
}

#[test]
fn hazards_cluster_and_keep_clear_of_spawn_and_points_of_interest() {
    for seed in sample_seeds(3) {
        let settings = v5(seed);
        let area = rect(-200, 0, 300, settings.height_tiles() - 1);
        let features = features_in(&settings, &area, None);
        let pois: Vec<&WorldFeature> = features
            .iter()
            .filter(|feature| feature.kind == FeatureKind::PointOfInterest)
            .collect();
        let clusters: Vec<&WorldFeature> = features
            .iter()
            .filter(|feature| feature.kind == FeatureKind::HazardCluster)
            .collect();
        assert!(!clusters.is_empty() && !pois.is_empty(), "seed {seed}");
        for cluster in &clusters {
            assert!(cluster.size >= 3);
            assert!(hazard_allowed(cluster.center.x, cluster.center.y));
            assert!(
                pois.iter().all(|poi| !poi
                    .bounds
                    .expanded(HAZARD_CLEARANCE)
                    .contains(cluster.center)),
                "seed {seed}: hazard {:?} inside a POI's clearance",
                cluster.center
            );
        }
        // Every hazard tile in the window respects the spawn and town rules.
        for x in -40..240 {
            for y in 0..80 {
                let kind = generated_tile(&settings, TilePosition { x, y }).kind;
                let fixture = pois
                    .iter()
                    .any(|poi| poi.bounds.contains(TilePosition { x, y }));
                if HazardKind::of_tile(kind).is_some() && !fixture {
                    assert!(hazard_allowed(x, y), "seed {seed}: hazard at ({x}, {y})");
                }
            }
        }
    }
}

#[test]
fn points_of_interest_are_spaced_clear_of_spawn_and_never_hollowed_by_caves() {
    for seed in sample_seeds(3) {
        let settings = v5(seed);
        let ctx = Ctx::new(&settings);
        let area = rect(-400, 0, 400, settings.height_tiles() - 1);
        let pois = features_in(&settings, &area, Some(&[FeatureKind::PointOfInterest]));
        assert!(pois.len() > 20, "seed {seed}: {} POIs", pois.len());
        for (index, poi) in pois.iter().enumerate() {
            let kind = poi.poi_kind().expect("poi");
            assert!(clear_of_spawn_and_town(&poi.bounds, kind));
            for other in &pois[index + 1..] {
                assert!(
                    !poi.bounds.expanded(POI_SPACING).intersects(&other.bounds),
                    "seed {seed}: {:?} and {:?} too close",
                    poi.bounds,
                    other.bounds
                );
            }
            let mut window = Window::new(&ctx, &poi.bounds.expanded(POI_CAVE_SHIELD));
            for position in poi.bounds.positions() {
                assert_ne!(
                    window.kind(position.x, position.y),
                    TileKind::Air,
                    "seed {seed}: a cave opens inside {:?}",
                    poi.bounds
                );
            }
        }
    }
}

#[test]
fn every_guaranteed_poi_kind_recurs_in_every_band() {
    // A band's guaranteed kinds rotate through its regions, so wherever a crew goes, each one
    // is within a few regions.
    let settings = v5(0x6A2);
    let ctx = Ctx::new(&settings);
    for start in [-30_000, 0, 41_000] {
        let area = rect(start, 0, start + 1_600, settings.height_tiles() - 1);
        let pois = features_in(&settings, &area, Some(&[FeatureKind::PointOfInterest]));
        for tier in ctx.tiers.iter().filter(|tier| tier.ordinal == 0) {
            for kind in crate::worldgen::pois::guaranteed_kinds(tier.band) {
                assert!(
                    pois.iter()
                        .any(|poi| poi.band == tier.band && poi.poi_kind() == Some(*kind)),
                    "{:?} has no {kind:?} near column {start}",
                    tier.band
                );
            }
        }
    }
}

#[test]
fn feature_ids_name_their_region() {
    let settings = v5(0x1D5);
    let area = rect(-300, 0, 300, settings.height_tiles() - 1);
    let features = features_in(&settings, &area, None);
    for kind in FeatureKind::ALL {
        // V5 has no cave ecosystems or villages (V6 does: `cave_tests`, `village::tests`).
        if matches!(
            kind,
            FeatureKind::PrefabSlot
                | FeatureKind::Cave
                | FeatureKind::Village
                | FeatureKind::SkyIsland
                | FeatureKind::SkyContinent
                | FeatureKind::Moon
        ) {
            continue;
        }
        assert!(
            features.iter().any(|feature| feature.kind == kind),
            "no {kind:?} in the window"
        );
    }
    let mut ids: Vec<FeatureId> = features.iter().map(|feature| feature.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), features.len(), "ids are unique");
    for feature in &features {
        assert_eq!(super::feature(&settings, feature.id), Some(*feature));
    }
    assert_eq!(super::feature(&settings, FeatureId(3)), None);
}

#[test]
fn nearest_feature_queries_reach_anywhere() {
    let settings = v5(0x7EA);
    for from in [
        TilePosition { x: 97, y: 40 },
        TilePosition { x: 50_000, y: 300 },
        TilePosition { x: -50_000, y: 600 },
    ] {
        for kind in [
            FeatureKind::PointOfInterest,
            FeatureKind::Chamber,
            FeatureKind::Motherlode,
            FeatureKind::ArtifactBed,
        ] {
            let found = nearest_feature(&settings, from, kind, 2_000, |_| true)
                .unwrap_or_else(|| panic!("no {kind:?} near {from:?}"));
            assert_eq!(found.kind, kind);
            // Nothing in a full window around the hit is nearer.
            let dx = i64::from((found.center.x - from.x).abs());
            let dy = i64::from((found.center.y - from.y).abs());
            let reach = i32::try_from((dx * dx + dy * dy).isqrt()).expect("small") + 1;
            let window = rect(
                from.x - reach,
                0,
                from.x + reach,
                settings.height_tiles() - 1,
            );
            for other in features_in(&settings, &window, Some(&[kind])) {
                let odx = i64::from(other.center.x - from.x);
                let ody = i64::from(other.center.y - from.y);
                assert!(
                    odx * odx + ody * ody >= dx * dx + dy * dy,
                    "{kind:?}: {:?} is nearer than {:?}",
                    other.center,
                    found.center
                );
            }
        }
    }
}

#[test]
fn ore_never_needs_a_stronger_drill_than_its_band_and_bands_hold_their_ore() {
    let mut values = [0_u64; 12];
    let mut tiles = [0_u64; 12];
    for seed in sample_seeds(2) {
        let settings = v5(seed);
        let ctx = Ctx::new(&settings);
        for x in (-480..480).step_by(8) {
            for y in SKY_ROWS..ctx.floor_row(x) {
                let kind = generated_tile(&settings, TilePosition { x, y }).kind;
                let band = ctx.band(x, y);
                tiles[band.index()] += 1;
                if let TileKind::Ore(mineral) = kind {
                    assert!(
                        tile_hardness(kind) <= band.required_drill_strength(),
                        "seed {seed}: {mineral:?} in {band:?} at ({x}, {y})"
                    );
                    values[band.index()] += u64::from(mineral.value());
                }
            }
        }
    }
    for band in Band::ALL {
        let per_ten = values[band.index()] * 10 / tiles[band.index()].max(1);
        let target = band_ore_target_per_ten_tiles(band);
        assert!(
            per_ten * 2 >= target && per_ten <= target * 3,
            "{band:?}: {per_ten} per 10 tiles vs target {target}"
        );
    }
}

#[test]
fn hazard_intensity_scales_streamed_hazards() {
    let count = |hazards: HazardIntensity| {
        sample_seeds(2)
            .map(|seed| {
                let settings = WorldSettings {
                    hazards,
                    ..v5(seed)
                };
                features_in(
                    &settings,
                    &rect(-300, 0, 300, settings.height_tiles() - 1),
                    Some(&[FeatureKind::HazardCluster]),
                )
                .len()
            })
            .sum::<usize>()
    };
    let (calm, standard, brutal) = (
        count(HazardIntensity::Calm),
        count(HazardIntensity::Standard),
        count(HazardIntensity::Brutal),
    );
    assert!(
        calm < standard && standard < brutal,
        "{calm} {standard} {brutal}"
    );
}

#[test]
#[ignore = "calibration helper: prints chunk generation and feature query timings"]
fn print_v5_timings() {
    let settings = v5(7);
    clear_caches();
    let start = std::time::Instant::now();
    let mut count = 0;
    for y in 0..chunk_rows(&settings) {
        for x in -10..10 {
            let _ = generated_chunk(&settings, ChunkPosition { x, y });
            count += 1;
        }
    }
    let cold = start.elapsed();
    println!(
        "cold: {count} chunks (20 columns x full depth) in {cold:?} = {:?}/chunk",
        cold / count
    );
    // A rig flying sideways: one new chunk column of the load window (7 chunks) per 16 tiles.
    let start = std::time::Instant::now();
    let mut columns = 0;
    for x in 1_000..1_100 {
        for y in 10..17 {
            let _ = generated_chunk(&settings, ChunkPosition { x, y });
        }
        columns += 1;
    }
    let walk = start.elapsed();
    println!(
        "flight: {columns} new chunk columns (7 chunks each) in {walk:?} = {:?}/column",
        walk / columns
    );
    for kind in FeatureKind::ALL {
        let start = std::time::Instant::now();
        let found = features_in(
            &settings,
            &rect(5_000, 0, 5_576, settings.height_tiles() - 1),
            Some(&[kind]),
        );
        println!(
            "features_in 577 columns {kind:?}: {} in {:?}",
            found.len(),
            start.elapsed()
        );
    }
    let start = std::time::Instant::now();
    let found = nearest_feature(
        &settings,
        TilePosition { x: 20_000, y: 400 },
        FeatureKind::PointOfInterest,
        2_000,
        |feature| feature.poi_kind() == Some(crate::poi::PoiKind::Vault),
    );
    println!(
        "nearest vault: {:?} in {:?}",
        found.map(|f| f.center),
        start.elapsed()
    );
    let _ = MineralKind::Copper;
}

fn glyph(kind: TileKind) -> char {
    match kind {
        TileKind::Air => ' ',
        TileKind::Dirt => '.',
        TileKind::Clay => ',',
        TileKind::Stone => ':',
        TileKind::HardRock => '#',
        TileKind::Foundation => 'F',
        TileKind::Bedrock => '=',
        TileKind::Lava => 'L',
        TileKind::Gas => 'g',
        TileKind::ExplosivePocket => 'x',
        TileKind::PressurePocket => 'p',
        TileKind::MagmaVent => 'v',
        TileKind::Ore(_) => 'o',
        TileKind::Artifact(_) => 'A',
        TileKind::Poi(_) => 'P',
        TileKind::Cave(_) => 'c',
        TileKind::Built(_) => 'B',
        TileKind::Surface(_) => 's',
        TileKind::Sky(_) => 'k',
        TileKind::Lunar(_) => 'l',
        TileKind::Fluid(_) => '~',
        TileKind::Crust(_) => 'r',
        TileKind::Deep(_) => 'd',
        TileKind::Hollow(_) => 'h',
        TileKind::Warped(_) => 'w',
    }
}

#[test]
#[ignore = "calibration helper: prints a strip of a V5 world"]
fn print_v5_strip() {
    let settings = v5(7);
    let world = generate_window(&settings, -100, 200);
    for y in 0..world.height {
        let row: String = (0..world.width)
            .map(|x| glyph(world.tiles[usize::try_from(y * world.width + x).unwrap_or(0)].kind))
            .collect();
        println!("{y:>4} {row}");
    }
}
