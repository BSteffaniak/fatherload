use std::time::Instant;

use super::super::{
    ArtifactKind, ChunkPosition, MineralKind, Terrain, Tile, TileChange, TileKind, TilePosition,
};
use super::{FullTerrain, TerrainRestoreError, TerrainSnapshot, TileDiffs};
use crate::{
    poi::PoiTile,
    worldgen::{GeneratorVersion, WorldSettings, WorldSize},
};

/// The binary layout of a [`TerrainSnapshot`] with plain integers for the enums (postcard
/// encodes an enum variant as its index), to forge unknown variants.
#[derive(serde::Serialize)]
struct WireSettings {
    seed: u64,
    size: u32,
    richness: u32,
    hazards: u32,
    generator: u32,
}

#[derive(serde::Serialize)]
struct WireSnapshot {
    settings: WireSettings,
    width: i32,
    height: i32,
    digest: u64,
    diffs: Vec<(u32, Vec<u64>)>,
}

/// Digs tunnels, blasts craters, chips tiles and plants ore across the whole terrain.
fn excavate(terrain: &mut Terrain, salt: i32) {
    let (width, height) = (terrain.width(), terrain.height());
    for step in 0..400 {
        let x = (step * 37 + salt * 11).rem_euclid(width);
        let y = 6 + (step * 53 + salt * 7).rem_euclid(height - 6);
        match step % 4 {
            0 => {
                let _ = terrain.blast_radius(TilePosition { x, y }, 3);
            }
            1 => {
                for dx in 0..12 {
                    let _ = terrain.set_kind(TilePosition { x: x + dx, y }, TileKind::Air);
                }
            }
            2 => {
                let _ = terrain.chip(TilePosition { x, y });
            }
            _ => {
                let _ = terrain.set_kind(TilePosition { x, y }, TileKind::Ore(MineralKind::Gold));
            }
        }
    }
}

fn assert_same_tiles(restored: &Terrain, original: &Terrain) {
    assert_eq!(restored.width(), original.width());
    assert_eq!(restored.height(), original.height());
    assert_eq!(restored.settings(), original.settings());
    assert_eq!(restored.digest(), original.digest());
    assert!(restored.tiles() == original.tiles(), "every tile matches");
}

fn settings_cases() -> [WorldSettings; 3] {
    [
        WorldSettings::new(0x5EED),
        WorldSettings {
            size: WorldSize::Large,
            ..WorldSettings::new(0xB16)
        },
        WorldSettings::legacy(0x1E6A),
    ]
}

/// Stops compiling when a mineral or artifact gains a variant, so it is added to
/// [`Tile::from_code`] (and to the list below) instead of becoming an undecodable save tile.
const fn assert_payload_kinds_listed(mineral: MineralKind, artifact: ArtifactKind) {
    match mineral {
        MineralKind::Copper
        | MineralKind::Iron
        | MineralKind::Silver
        | MineralKind::Gold
        | MineralKind::Emerald
        | MineralKind::Ruby
        | MineralKind::Diamond
        | MineralKind::Platinum
        | MineralKind::Uranium
        | MineralKind::Mythril => {}
    }
    match artifact {
        ArtifactKind::Fossil
        | ArtifactKind::OldCircuit
        | ArtifactKind::BuriedIdol
        | ArtifactKind::StarCore
        | ArtifactKind::CrystalPrism
        | ArtifactKind::RadiantShell
        | ArtifactKind::PressureGlyph
        | ArtifactKind::ReactorBloom
        | ArtifactKind::MachineSigil
        | ArtifactKind::VoidPearl
        | ArtifactKind::MantleHeart => {}
    }
}

#[test]
fn every_tile_code_round_trips_and_unknown_codes_are_rejected() {
    assert_payload_kinds_listed(MineralKind::Copper, ArtifactKind::Fossil);
    let mut kinds = vec![
        TileKind::Air,
        TileKind::Dirt,
        TileKind::Clay,
        TileKind::Stone,
        TileKind::HardRock,
        TileKind::Foundation,
        TileKind::Lava,
        TileKind::Gas,
        TileKind::ExplosivePocket,
        TileKind::PressurePocket,
        TileKind::MagmaVent,
    ];
    kinds.extend(crate::worldgen::deposits::MINERALS.map(TileKind::Ore));
    kinds.extend(
        [
            ArtifactKind::Fossil,
            ArtifactKind::OldCircuit,
            ArtifactKind::BuriedIdol,
            ArtifactKind::StarCore,
            ArtifactKind::CrystalPrism,
            ArtifactKind::RadiantShell,
            ArtifactKind::PressureGlyph,
            ArtifactKind::ReactorBloom,
            ArtifactKind::MachineSigil,
            ArtifactKind::VoidPearl,
            ArtifactKind::MantleHeart,
        ]
        .map(TileKind::Artifact),
    );
    kinds.extend(
        [
            PoiTile::SupplyCrate,
            PoiTile::Wreckage,
            PoiTile::SalvageCore,
            PoiTile::VaultWall,
            PoiTile::VaultDoor,
            PoiTile::VaultSeal,
            PoiTile::VaultSealVented,
            PoiTile::VaultSwitch {
                order: 1,
                lit: false,
            },
            PoiTile::VaultSwitch {
                order: 3,
                lit: true,
            },
            PoiTile::VaultRelay { lit: false },
            PoiTile::VaultRelay { lit: true },
            PoiTile::VaultCache,
            PoiTile::FossilShale,
            PoiTile::GeodeShell,
            PoiTile::OutpostPlating,
            PoiTile::OutpostSalvage,
        ]
        .map(TileKind::Poi),
    );
    for kind in kinds {
        for durability in [0, 1, 7, 255] {
            let tile = Tile { kind, durability };
            assert_eq!(Tile::from_code(tile.code()), Some(tile));
        }
    }
    for code in [
        11 << 8,
        0x10A << 8,
        0x20B << 8,
        0x3FF << 8,
        0x400 << 8,
        u64::MAX,
    ] {
        assert_eq!(Tile::from_code(code), None, "code {code:#x}");
    }
}

#[test]
fn fresh_worlds_have_no_diffs_and_regenerate_exactly() {
    for settings in settings_cases() {
        let terrain = Terrain::generate(&settings);
        let snapshot = terrain.snapshot();
        assert!(snapshot.diffs.is_empty(), "{settings:?}");
        let restored = Terrain::from_snapshot(snapshot).expect("restores");
        assert_same_tiles(&restored, &terrain);
    }
}

#[test]
fn dug_worlds_round_trip_through_json_and_postcard() {
    for (salt, settings) in (1..).zip(settings_cases()) {
        let mut terrain = Terrain::generate(&settings);
        excavate(&mut terrain, salt);
        let snapshot = terrain.snapshot();
        assert!(snapshot.diffs.len() > 1_000, "the world was dug");
        assert_eq!(snapshot.diffs.len(), terrain.generation_diffs().len());

        let json = serde_json::to_string(&terrain).expect("json");
        let restored: Terrain = serde_json::from_str(&json).expect("json restores");
        assert_same_tiles(&restored, &terrain);

        let bytes = postcard::to_allocvec(&terrain).expect("postcard");
        let restored: Terrain = postcard::from_bytes(&bytes).expect("postcard restores");
        assert_same_tiles(&restored, &terrain);
        assert_eq!(restored.features(), terrain.features());
    }
}

#[test]
fn odd_sized_and_extended_terrain_round_trips() {
    // Legacy test and harness worlds of any size, and old saves extended downwards.
    let mut small = Terrain::new_seeded(12, 40, 3);
    excavate(&mut small, 9);
    small.ensure_depth(220);
    let restored: Terrain =
        postcard::from_bytes(&postcard::to_allocvec(&small).expect("encode")).expect("decode");
    assert_same_tiles(&restored, &small);

    // A short v1 world extended to full depth regains exactly the generated rows.
    let settings = WorldSettings::new(0xACE);
    let generated = Terrain::generate(&settings);
    let mut short = Terrain::try_from(FullTerrain {
        tiles: generated.tiles()[..usize::try_from(240 * 100).expect("small")].to_vec(),
        height: 100,
        ..FullTerrain::from(&generated)
    })
    .expect("valid full terrain");
    short.ensure_depth(settings.height_tiles());
    assert_same_tiles(&short, &generated);
    assert!(short.snapshot().diffs.is_empty());
}

#[test]
fn full_terrain_with_the_wrong_tile_count_or_size_is_rejected() {
    let generated = Terrain::new_seeded(20, 30, 5);
    let full = FullTerrain::from(&generated);
    let mut short = full.clone();
    short.tiles.pop();
    assert_eq!(
        Terrain::try_from(short).expect_err("short").to_string(),
        "terrain holds 599 tiles but its size needs 600"
    );
    let mut long = full.clone();
    long.tiles.push(long.tiles[0]);
    assert!(matches!(
        Terrain::try_from(long),
        Err(TerrainRestoreError::TileCountMismatch { .. })
    ));
    let mut empty = full.clone();
    empty.width = 0;
    assert!(matches!(
        Terrain::try_from(empty),
        Err(TerrainRestoreError::InvalidSize { .. })
    ));
    let restored = Terrain::try_from(full).expect("valid");
    assert_same_tiles(&restored, &generated);
}

#[test]
fn generation_diffs_apply_in_any_order_and_twice() {
    let settings = WorldSettings::new(0xD1FF);
    let mut terrain = Terrain::generate(&settings);
    excavate(&mut terrain, 4);
    let diffs = terrain.generation_diffs();
    assert!(
        diffs
            .windows(2)
            .all(|pair| (pair[0].position.y, pair[0].position.x)
                < (pair[1].position.y, pair[1].position.x)),
        "row-major order"
    );
    let (width, height) = (terrain.width(), terrain.height());

    let forward = Terrain::from_generation_diffs(&settings, width, height, &diffs).expect("ok");
    assert_same_tiles(&forward, &terrain);

    let mut reversed: Vec<TileChange> = diffs.iter().rev().copied().collect();
    let third = reversed.len() / 3;
    reversed.rotate_left(third);
    let shuffled = Terrain::from_generation_diffs(&settings, width, height, &reversed).expect("ok");
    assert_same_tiles(&shuffled, &terrain);

    let mut twice = forward;
    for change in &diffs {
        assert!(twice.apply_tile_change(change));
    }
    assert_same_tiles(&twice, &terrain);

    // Diffs as a set: building them from shuffled pairs gives the same sorted list.
    let from_pairs = TileDiffs::new(terrain.snapshot().diffs.iter().rev());
    assert_eq!(from_pairs, terrain.snapshot().diffs);
}

#[test]
fn chunk_diffs_restore_a_stale_chunk_from_generation() {
    let settings = WorldSettings::new(0xC4C4);
    let fresh = Terrain::generate(&settings);
    let mut dug = fresh.clone();
    excavate(&mut dug, 2);
    let mut replica = fresh.clone();
    // The replica also has a stale write of its own the authority overwrote since.
    let stray = TilePosition { x: 40, y: 40 };
    let _ = replica.set_kind(stray, TileKind::Lava);

    let mut refreshed_tiles = 0;
    for chunk_y in 0..=(dug.height() / 16) {
        for chunk_x in 0..=(dug.width() / 16) {
            let chunk = ChunkPosition {
                x: chunk_x,
                y: chunk_y,
            };
            let diffs = dug.chunk_diffs(chunk);
            assert!(diffs.len() <= 256);
            refreshed_tiles += diffs.len();
            let wire: TileDiffs =
                postcard::from_bytes(&postcard::to_allocvec(&diffs).expect("encode"))
                    .expect("decode");
            replica.apply_chunk_diffs(chunk, &wire);
            // Idempotent.
            replica.apply_chunk_diffs(chunk, &wire);
            assert_eq!(replica.chunk_tiles(chunk), dug.chunk_tiles(chunk));
        }
    }
    assert_eq!(refreshed_tiles, dug.generation_diffs().len());
    assert_same_tiles(&replica, &dug);
    assert!(fresh.chunk_diffs(ChunkPosition { x: 2, y: 2 }).is_empty());
}

#[test]
fn corrupted_or_foreign_snapshots_are_rejected_cleanly() {
    let mut terrain = Terrain::generate(&WorldSettings::new(0xBAD));
    excavate(&mut terrain, 5);
    let snapshot = terrain.snapshot();

    // A snapshot whose generator produced different base terrain (simulated by relabelling the
    // seed) fails the digest check.
    let foreign = TerrainSnapshot {
        settings: WorldSettings::new(0xBAD + 1),
        ..snapshot.clone()
    };
    assert!(matches!(
        Terrain::from_snapshot(foreign),
        Err(TerrainRestoreError::DigestMismatch {
            generator: GeneratorVersion::V2,
            ..
        })
    ));
    let relabelled = TerrainSnapshot {
        settings: WorldSettings::legacy(0xBAD),
        ..snapshot.clone()
    };
    let error = Terrain::from_snapshot(relabelled).expect_err("legacy base differs");
    assert!(error.to_string().contains("generator legacy"), "{error}");

    let out_of_bounds = TerrainSnapshot {
        diffs: TileDiffs::new([(
            u32::try_from(240 * 220).expect("small"),
            Tile {
                kind: TileKind::Air,
                durability: 0,
            },
        )]),
        ..snapshot
    };
    assert!(matches!(
        Terrain::from_snapshot(out_of_bounds),
        Err(TerrainRestoreError::DiffOutOfBounds { .. })
    ));
    for (width, height) in [(0, 10), (10, -1), (100_000, 10), (4_096, 4_096)] {
        assert!(matches!(
            Terrain::from_snapshot(TerrainSnapshot {
                width,
                height,
                ..snapshot.clone()
            }),
            Err(TerrainRestoreError::InvalidSize { .. })
        ));
    }

    // An unknown generator version (from a newer build) is a decode error, not a panic.
    let mut json = serde_json::to_value(&terrain).expect("json");
    json["settings"]["generator"] = serde_json::Value::String("V9".to_owned());
    let error = serde_json::from_value::<Terrain>(json).expect_err("unknown generator");
    assert!(error.to_string().contains("V9"), "{error}");

    // The same in the binary keyframe codec: postcard encodes the variant as its index.
    let known = WireSnapshot {
        settings: WireSettings {
            seed: 0xBAD,
            size: 0,
            richness: 1,
            hazards: 1,
            // `WorldSettings::new` uses the current generator (V2, variant index 2).
            generator: 2,
        },
        width: 240,
        height: 220,
        digest: Terrain::generate(&WorldSettings::new(0xBAD)).digest(),
        diffs: Vec::new(),
    };
    let bytes = postcard::to_allocvec(&known).expect("encode");
    assert!(
        postcard::from_bytes::<Terrain>(&bytes).is_ok(),
        "mirror matches"
    );
    let unknown = WireSnapshot {
        settings: WireSettings {
            generator: 7,
            ..known.settings
        },
        ..known
    };
    let bytes = postcard::to_allocvec(&unknown).expect("encode");
    assert!(postcard::from_bytes::<Terrain>(&bytes).is_err());

    // Garbage tile codes are rejected too.
    let mut json = serde_json::to_value(&terrain).expect("json");
    json["diffs"][0][1][0] = serde_json::Value::from(0xFFFF_FFFF_u64);
    let error = serde_json::from_value::<Terrain>(json).expect_err("bad tile code");
    assert!(error.to_string().contains("invalid tile code"), "{error}");
}

/// Points of interest live in their tiles: a looted crate and lit vault switches must survive
/// the compact save/keyframe form exactly.
#[test]
fn changed_poi_tiles_round_trip_through_snapshots() {
    let settings = WorldSettings::new(0x9017);
    let mut terrain = Terrain::generate(&settings);
    let mut changed = 0;
    let poi_tiles: Vec<(TilePosition, PoiTile)> = terrain
        .features()
        .pois()
        .flat_map(|feature| feature.bounds.positions())
        .filter_map(|position| match terrain.tile(position)?.kind {
            TileKind::Poi(tile) => Some((position, tile)),
            _ => None,
        })
        .collect();
    assert!(!poi_tiles.is_empty(), "a V2 world has points of interest");
    for (position, tile) in poi_tiles {
        let replacement = match tile {
            PoiTile::SupplyCrate | PoiTile::SalvageCore => TileKind::Air,
            PoiTile::VaultSwitch { order, .. } => {
                TileKind::Poi(PoiTile::VaultSwitch { order, lit: true })
            }
            PoiTile::VaultRelay { .. } => TileKind::Poi(PoiTile::VaultRelay { lit: true }),
            PoiTile::VaultSeal => TileKind::Poi(PoiTile::VaultSealVented),
            _ => continue,
        };
        if terrain.set_kind(position, replacement) {
            changed += 1;
        }
    }
    assert!(changed > 0, "the test changed some POI tiles");

    let restored = Terrain::from_snapshot(terrain.snapshot()).expect("restore");
    assert_same_tiles(&restored, &terrain);
    let bytes = postcard::to_allocvec(&terrain).expect("encode");
    let decoded: Terrain = postcard::from_bytes(&bytes).expect("decode");
    assert_same_tiles(&decoded, &terrain);
}

/// Generation timings behind the full-world (not per-chunk) regeneration decision.
#[test]
#[ignore = "timing report; run with --ignored --nocapture (in debug and --release)"]
fn generation_timing_report() {
    for settings in [
        WorldSettings::new(0x7111),
        WorldSettings {
            size: WorldSize::Large,
            ..WorldSettings::new(0x7112)
        },
        WorldSettings::legacy(0x7113),
    ] {
        let start = Instant::now();
        let world = crate::worldgen::generate(&settings);
        let generate = start.elapsed();
        let mut terrain = Terrain::generate(&settings);
        excavate(&mut terrain, 1);
        let bytes = postcard::to_allocvec(&terrain).expect("encode");
        let start = Instant::now();
        let _restored: Terrain = postcard::from_bytes(&bytes).expect("decode (cached base)");
        let cached = start.elapsed();
        println!(
            "{:?} {}x{}: generate {generate:?}, restore with cached base {cached:?}",
            settings.generator, world.width, world.height
        );
    }
}
