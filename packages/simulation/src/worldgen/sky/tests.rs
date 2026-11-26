use super::*;
use crate::{
    terrain::{ChunkPosition, Terrain, Tile},
    worldgen::{GeneratorVersion, streamed},
};

fn v8(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V8,
        ..WorldSettings::new(seed)
    }
}

fn rect(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> TileRect {
    TileRect {
        min: TilePosition { x: min_x, y: min_y },
        max: TilePosition { x: max_x, y: max_y },
    }
}

/// The rect of a band's island rows over columns `first..=last`.
fn band_rect(first: i32, last: i32, bottom_rows: i32, top_rows: i32) -> TileRect {
    rect(first, row_of(top_rows), last, row_of(bottom_rows))
}

#[test]
#[ignore = "prints the band rows"]
fn print_bands() {
    println!(
        "low {LOW_ISLES_BOTTOM_ROWS} arch {ARCHIPELAGO_BOTTOM_ROWS} gap {DEAD_AIR_BOTTOM_ROWS} \
         continents {CONTINENTS_BOTTOM_ROWS} orbit {ORBIT_BOTTOM_ROWS} moon {MOON_BOTTOM_ROWS} \
         ceiling {SKY_CEILING_ROWS}"
    );
}

#[test]
fn v9_is_current_and_only_v8_and_v9_have_a_sky() {
    // V10 (tombs, WP-TOMBS) is current and keeps V9's sky exactly.
    assert_eq!(GeneratorVersion::CURRENT, GeneratorVersion::V10);
    for generator in GeneratorVersion::ALL {
        assert_eq!(
            generator.has_sky(),
            matches!(
                generator,
                GeneratorVersion::V8
                    | GeneratorVersion::V9
                    | GeneratorVersion::V10
                    | GeneratorVersion::V11
            )
        );
        assert_eq!(
            generator.has_moon_content(),
            matches!(
                generator,
                GeneratorVersion::V9 | GeneratorVersion::V10 | GeneratorVersion::V11
            )
        );
    }
    let v7 = WorldSettings {
        generator: GeneratorVersion::V7,
        ..WorldSettings::new(3)
    };
    assert!(moon(&v7).is_none());
    assert_eq!(
        lands_touching(&v7, &rect(-5_000, -SKY_CEILING_ROWS, 5_000, 0)),
        []
    );
    assert_eq!(
        updrafts_touching(&v7, &rect(-5_000, -SKY_CEILING_ROWS, 5_000, 0)),
        []
    );
    assert_eq!(streamed::top_row(&v8(3)), -SKY_CEILING_ROWS);
    assert_eq!(SKY_CEILING_ROWS % CHUNK_SIZE_TILES, 0);
}

#[test]
fn bands_climb_in_order_with_room_for_their_lands() {
    let bands = [
        LOW_ISLES_BOTTOM_ROWS,
        ARCHIPELAGO_BOTTOM_ROWS,
        DEAD_AIR_BOTTOM_ROWS,
        CONTINENTS_BOTTOM_ROWS,
        ORBIT_BOTTOM_ROWS,
        MOON_BOTTOM_ROWS,
    ];
    for pair in bands.windows(2) {
        assert!(pair[0] < pair[1], "{bands:?}");
    }
    // Above the tallest peak (V7 mountains stay inside the headroom).
    const {
        assert!(LOW_ISLES_BOTTOM_ROWS > crate::worldgen::biome::SKY_HEADROOM_ROWS + 60);
        assert!(SKY_CEILING_ROWS > MOON_BOTTOM_ROWS + MOON_DEPTH_ROWS + 300);
    }
}

#[test]
fn lands_are_deterministic_and_stay_in_their_bands() {
    for seed in [1_u64, 77, 0xD1_11_6A_4E] {
        let settings = v8(seed);
        let area = rect(-3_000, -SKY_CEILING_ROWS, 3_000, 0);
        let lands = lands_touching(&settings, &area);
        assert_eq!(lands, lands_touching(&settings, &area), "seed {seed}");
        let (mut isles, mut archipelago, mut continents, mut moons) = (0, 0, 0, 0);
        for land in &lands {
            let top = -land.bounds().min.y;
            let bottom = -land.bounds().max.y;
            match land.kind {
                SkyLandKind::Isle => {
                    isles += 1;
                    assert!(bottom >= LOW_ISLES_BOTTOM_ROWS, "{land:?}");
                    assert!(top < ARCHIPELAGO_BOTTOM_ROWS, "{land:?}");
                }
                SkyLandKind::ArchipelagoIsle | SkyLandKind::TownIsle => {
                    archipelago += 1;
                    assert!(bottom >= ARCHIPELAGO_BOTTOM_ROWS, "{land:?}");
                    assert!(top < DEAD_AIR_BOTTOM_ROWS, "{land:?}");
                }
                SkyLandKind::Continent => {
                    continents += 1;
                    assert!(bottom >= CONTINENTS_BOTTOM_ROWS, "{land:?}");
                    assert!(top < ORBIT_BOTTOM_ROWS, "{land:?}");
                    assert!(land.half_width * 2 >= 300, "hundreds of tiles wide");
                }
                SkyLandKind::Moon => {
                    moons += 1;
                    assert!(bottom >= MOON_BOTTOM_ROWS - 20, "{land:?}");
                }
            }
            assert_eq!(land_by_id(&settings, land.id), Some(*land), "lookup by id");
        }
        assert!(isles >= 15, "seed {seed}: {isles} low isles");
        assert!(
            archipelago >= 60,
            "seed {seed}: {archipelago} archipelago isles"
        );
        assert!(continents >= 1, "seed {seed}: {continents} continents");
        assert!(moons <= 1);
    }
}

/// [`super::land`] (named apart from the loop variables).
fn land_by_id(settings: &WorldSettings, id: crate::worldgen::FeatureId) -> Option<SkyLand> {
    super::land(settings, id)
}

#[test]
fn nothing_hangs_in_the_dead_air_gap_or_orbit() {
    for seed in [2_u64, 99] {
        let settings = v8(seed);
        let gap = band_rect(
            -8_000,
            8_000,
            DEAD_AIR_BOTTOM_ROWS,
            CONTINENTS_BOTTOM_ROWS - 1,
        );
        assert!(lands_touching(&settings, &gap).is_empty(), "seed {seed}");
        assert!(updrafts_touching(&settings, &gap).is_empty(), "seed {seed}");
        let orbit = band_rect(-8_000, 8_000, ORBIT_BOTTOM_ROWS, MOON_BOTTOM_ROWS - 30);
        assert!(lands_touching(&settings, &orbit).is_empty(), "seed {seed}");
    }
}

#[test]
fn the_moon_is_one_giant_landmass_a_few_thousand_tiles_from_town() {
    for seed in 0..40_u64 {
        let settings = v8(seed);
        let moon = moon(&settings).expect("a moon");
        let width = moon.half_width * 2 + 1;
        assert!((2_000..=2_901).contains(&width), "seed {seed}: {width}");
        let distance = (moon.center_x - TOWN_CENTER_COLUMN).abs();
        assert!(
            (3_200..=4_800).contains(&distance),
            "seed {seed}: {distance}"
        );
        assert_eq!(moon_column(&settings), Some(moon.center_x));
        // Its underside's lowest point is at the band floor; its top is regolith.
        let lowest = (moon.center_x - 20..=moon.center_x + 20)
            .filter_map(|x| {
                (moon.top_row..=moon.bounds().max.y)
                    .rev()
                    .find(|y| moon.tile_at(x, *y).is_some())
            })
            .max()
            .expect("an underside");
        assert!(
            (-lowest - MOON_BOTTOM_ROWS).abs() <= 12,
            "seed {seed}: {}",
            -lowest
        );
        let top = moon.surface_row(moon.center_x).expect("covers its centre");
        assert_eq!(
            moon.tile_at(moon.center_x, top),
            Some(TileKind::Sky(SkyTile::Regolith))
        );
        assert_eq!(
            moon.tile_at(moon.center_x, top + 40),
            Some(TileKind::Sky(SkyTile::MoonRock))
        );
        assert_eq!(moon.tile_at(moon.center_x, top - 1), None);
    }
}

#[test]
fn islands_have_turf_soil_and_a_rock_underside_with_things_hanging_below() {
    let settings = v8(5);
    let area = band_rect(-4_000, 4_000, LOW_ISLES_BOTTOM_ROWS, DEAD_AIR_BOTTOM_ROWS);
    let lands = lands_touching(&settings, &area);
    let (mut roots, mut crystal, mut rock) = (0, 0, 0);
    for land in lands
        .iter()
        .filter(|land| matches!(land.kind, SkyLandKind::Isle | SkyLandKind::ArchipelagoIsle))
    {
        let top = land.surface_row(land.center_x).expect("covers its centre");
        assert_eq!(
            land.tile_at(land.center_x, top),
            Some(TileKind::Sky(SkyTile::Turf(land.biome))),
            "{land:?}"
        );
        assert_eq!(
            land.tile_at(land.center_x, top + 1),
            Some(TileKind::Sky(SkyTile::Soil))
        );
        for position in land.bounds().positions() {
            match land.tile_at(position.x, position.y) {
                Some(TileKind::Sky(SkyTile::Roots)) => roots += 1,
                Some(TileKind::Sky(SkyTile::Crystal)) => crystal += 1,
                Some(TileKind::Sky(SkyTile::Rock(_))) => rock += 1,
                _ => {}
            }
            assert!(land.bounds().contains(position));
        }
        // Nothing outside the bounds.
        let bounds = land.bounds();
        for x in bounds.min.x - 2..=bounds.max.x + 2 {
            for y in [bounds.min.y - 1, bounds.max.y + 1] {
                assert_eq!(land.tile_at(x, y), None, "{land:?} at {x},{y}");
            }
        }
    }
    assert!(
        rock > 1_000 && roots > 50 && crystal > 20,
        "{rock} {roots} {crystal}"
    );
}

#[test]
fn sky_chunks_are_seamless_and_empty_sky_is_free() {
    let settings = v8(11);
    let terrain = Terrain::generate(&settings);
    // Every land tile reads the same from the chunk cache as from the land itself, across
    // chunk borders.
    let area = band_rect(
        -600,
        600,
        LOW_ISLES_BOTTOM_ROWS,
        ARCHIPELAGO_BOTTOM_ROWS + 400,
    );
    let lands = lands_touching(&settings, &area);
    assert_ne!(lands, []);
    let mut checked = 0;
    for land in lands.iter().take(12) {
        for position in land.bounds().expanded(1).positions() {
            let expected = tile_at(&settings, position).unwrap_or(TileKind::Air);
            assert_eq!(
                terrain.tile(position).map(|tile| tile.kind),
                Some(expected),
                "{position:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 2_000);
    // A chunk in the dead-air gap is the shared empty chunk.
    let gap_chunk = ChunkPosition::of(TilePosition {
        x: 40,
        y: row_of(DEAD_AIR_BOTTOM_ROWS + 900),
    });
    let first = streamed::generated_chunk(&settings, gap_chunk);
    let second = streamed::generated_chunk(&settings, ChunkPosition { x: 9, ..gap_chunk });
    assert!(
        std::sync::Arc::ptr_eq(&first, &second),
        "one shared chunk of air"
    );
    assert!(first.tiles().iter().all(|tile| *tile
        == Tile {
            kind: TileKind::Air,
            durability: 0
        }));
}

#[test]
fn v8_ground_is_exactly_v7_ground() {
    let v7 = WorldSettings {
        generator: GeneratorVersion::V7,
        ..WorldSettings::new(0x5EED)
    };
    let v8 = v8(0x5EED);
    let first_row = crate::worldgen::biome::SKY_HEADROOM_ROWS.div_euclid(CHUNK_SIZE_TILES);
    for cx in [-300, -40, -1, 0, 6, 7, 60, 250] {
        for cy in -first_row..20 {
            let chunk = ChunkPosition { x: cx, y: cy };
            assert_eq!(
                streamed::generated_chunk(&v7, chunk).tiles(),
                streamed::generated_chunk(&v8, chunk).tiles(),
                "{chunk:?}"
            );
        }
    }
    for x in [-4_000, -97, 0, 115, 3_000] {
        assert_eq!(
            crate::worldgen::surface_row(&v7, x),
            crate::worldgen::surface_row(&v8, x)
        );
    }
}

#[test]
fn sky_lands_are_regional_features() {
    let settings = v8(21);
    let town = TilePosition {
        x: TOWN_CENTER_COLUMN,
        y: 0,
    };
    let island =
        streamed::nearest_feature(&settings, town, FeatureKind::SkyIsland, 4_000, |_| true)
            .expect("an island near town");
    assert_ne!(island.detail, FeatureDetail::None);
    assert_eq!(streamed::feature(&settings, island.id), Some(island));
    let moon_feature =
        streamed::nearest_feature(&settings, town, FeatureKind::Moon, 8_000, |_| true)
            .expect("the moon");
    assert_eq!(Some(moon_feature.center.x), moon_column(&settings));
    let continent =
        streamed::nearest_feature(&settings, town, FeatureKind::SkyContinent, 12_000, |_| true)
            .expect("a continent");
    assert!(continent.bounds.width() >= 300);
    // Area queries by kind.
    let area = band_rect(
        -2_000,
        2_000,
        LOW_ISLES_BOTTOM_ROWS,
        ARCHIPELAGO_BOTTOM_ROWS,
    );
    let isles = streamed::features_in(&settings, &area, Some(&[FeatureKind::SkyIsland]));
    assert_ne!(isles, []);
    assert!(
        isles
            .iter()
            .all(|feature| feature.kind == FeatureKind::SkyIsland)
    );
}

#[test]
fn updrafts_lift_in_the_island_bands_and_taper_at_the_top() {
    let settings = v8(8);
    let area = band_rect(-3_000, 3_000, UPDRAFT_FLOOR_ROWS, ORBIT_BOTTOM_ROWS);
    let updrafts = updrafts_touching(&settings, &area);
    assert!(updrafts.len() > 40, "{}", updrafts.len());
    for updraft in &updrafts {
        let bottom = -updraft.rect.max.y;
        let band = SkyBand::at_altitude(bottom);
        assert!(
            matches!(
                band,
                SkyBand::Ground | SkyBand::LowIsles | SkyBand::Archipelago | SkyBand::Continents
            ),
            "{updraft:?}"
        );
        let x = updraft.rect.min.x;
        assert_eq!(
            updraft_at(&settings, x, updraft.rect.max.y),
            updraft.strength
        );
        assert!(updraft_at(&settings, x, updraft.rect.min.y) < updraft.strength / 4);
        assert_eq!(updraft_at(&settings, x, updraft.rect.min.y - 1), 0);
        assert!(updraft.strength > 440, "lifts a rig without thrust");
    }
}

#[test]
fn chunk_digests_of_v8_skies_are_pinned() {
    // V8 = V7 + the sky. Pinned once it ships: changing what V8 generates (including the climb
    // table the bands come from) breaks V8 saves, so ship it as a new generator version.
    let mut combined = 0_u64;
    for seed in [1_u64, 0xD1_11_6A_4E] {
        let settings = v8(seed);
        let moon = moon(&settings).expect("a moon");
        let chunks = [
            // Ground, low isles, archipelago, continents and the Moon's top and underside.
            (0, 0),
            (
                5,
                row_of(LOW_ISLES_BOTTOM_ROWS + 150).div_euclid(CHUNK_SIZE_TILES),
            ),
            (
                -9,
                row_of(ARCHIPELAGO_BOTTOM_ROWS + 500).div_euclid(CHUNK_SIZE_TILES),
            ),
            (
                30,
                row_of(CONTINENTS_BOTTOM_ROWS + 300).div_euclid(CHUNK_SIZE_TILES),
            ),
            (
                moon.center_x.div_euclid(CHUNK_SIZE_TILES),
                moon.top_row.div_euclid(CHUNK_SIZE_TILES),
            ),
            (
                moon.center_x.div_euclid(CHUNK_SIZE_TILES),
                row_of(MOON_BOTTOM_ROWS + 4).div_euclid(CHUNK_SIZE_TILES),
            ),
        ];
        for (x, y) in chunks {
            for (index, tile) in streamed::generated_chunk(&settings, ChunkPosition { x, y })
                .tiles()
                .iter()
                .enumerate()
            {
                combined = combined
                    .rotate_left(7)
                    .wrapping_add(tile.code() ^ (index as u64).wrapping_mul(0x9E37));
            }
        }
        let lands = lands_touching(&settings, &rect(-6_000, -SKY_CEILING_ROWS, 6_000, 0));
        for land in lands {
            combined = combined.rotate_left(5)
                ^ (land.id.0
                    ^ u64::from(land.center_x.unsigned_abs())
                    ^ u64::from(land.top_row.unsigned_abs()));
        }
    }
    assert_eq!(
        combined, GOLDEN_V8_DIGEST,
        "V8 generation changed: {combined:#018x}"
    );
}

const GOLDEN_V8_DIGEST: u64 = 0x4d09_93ad_a72c_3523;
