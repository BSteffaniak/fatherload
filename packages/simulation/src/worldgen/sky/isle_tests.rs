//! V9 sky content (WP-SKY, T20): furnished islands, town isles, sky towns, jet streams and
//! vents.

#![allow(
    clippy::cast_precision_loss,
    reason = "small tile coordinates in tests"
)]

use std::collections::BTreeSet;

use super::*;
use crate::{
    terrain::{ChunkPosition, MineralKind, Tile},
    village::{self, VillageSetting, placement},
    worldgen::{GeneratorVersion, streamed},
};

fn v9(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V9,
        ..WorldSettings::new(seed)
    }
}

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

/// V8's bare island placement (no furnishings, no town cells) in a sky with `bands`.
fn bare_lands(seed: u64, bands: &SkyBands, area: &TileRect) -> Vec<SkyLand> {
    let mut found = Vec::new();
    for ry in band_cells(bands, ID_LOW, LOW_ISLE_CELL.1, area) {
        for rx in cells(area.min.x, area.max.x, LOW_ISLE_CELL.0) {
            found.extend(low_isle_cell(seed, bands, rx, ry));
        }
    }
    for ry in band_cells(bands, ID_ARCHIPELAGO, ARCHIPELAGO_CELL.1, area) {
        for rx in cells(area.min.x, area.max.x, ARCHIPELAGO_CELL.0) {
            found.extend(archipelago_cell(seed, bands, rx, ry));
        }
    }
    found.retain(|land| land.bounds().intersects(area));
    found.sort_by_key(|land| land.id);
    found
}

/// The island bands of a V9 sky over columns `first..=last`.
fn island_bands(first: i32, last: i32) -> TileRect {
    rect(
        first,
        row_of(V9_SKY_BANDS.dead_air),
        last,
        row_of(V9_SKY_BANDS.low_isles),
    )
}

#[test]
fn v9_is_current_and_only_v9_furnishes_the_isles() {
    assert_eq!(GeneratorVersion::CURRENT, GeneratorVersion::V9);
    for generator in GeneratorVersion::ALL {
        assert_eq!(generator.has_sky_isles(), generator == GeneratorVersion::V9);
        assert_eq!(
            generator.has_sky(),
            matches!(generator, GeneratorVersion::V8 | GeneratorVersion::V9)
        );
    }
    let area = island_bands(-3_000, 3_000);
    assert!(
        lands_touching(&v8(5), &area)
            .iter()
            .all(|land| land.content().is_none() && land.kind != SkyLandKind::TownIsle),
        "V8 islands stay bare"
    );
    assert!(sky_towns_touching(&v8(5), &area).is_empty());
    assert!(jet_streams_touching(&v8(5), &area).is_empty());
}

#[test]
fn v9_keeps_the_bare_placement_outside_town_cells() {
    // V9 furnishes V8's island placement (in its own, taller bands) and swaps one cell per
    // block for a town cell; everything else is where the bare generator puts it.
    for seed in [3_u64, 0xD1_11_6A_4E] {
        let area = island_bands(-4_000, 4_000);
        let old: Vec<SkyLand> = bare_lands(seed, &V9_SKY_BANDS, &area);
        let new: Vec<SkyLand> = lands_touching(&v9(seed), &area)
            .into_iter()
            .filter(|land| land.bounds().intersects(&area))
            .filter(|land| {
                matches!(
                    land.kind,
                    SkyLandKind::Isle | SkyLandKind::ArchipelagoIsle | SkyLandKind::TownIsle
                )
            })
            .collect();
        let town_cells: BTreeSet<(i32, i32)> = new
            .iter()
            .filter(|land| land.kind == SkyLandKind::TownIsle)
            .filter_map(|land| {
                let (_, rx, ry, _) = streamed::index::decode(land.id)?;
                Some((rx, ry))
            })
            .collect();
        assert!(!town_cells.is_empty(), "seed {seed} has sky towns");
        let in_town_cell = |land: &SkyLand| {
            streamed::index::decode(land.id)
                .is_some_and(|(_, rx, ry, _)| town_cells.contains(&(rx, ry)))
        };
        let shape = |land: &SkyLand| {
            (
                land.id,
                land.kind,
                land.biome,
                land.center_x,
                land.top_row,
                land.half_width,
                land.depth,
            )
        };
        let old: Vec<_> = old.iter().filter(|l| !in_town_cell(l)).map(shape).collect();
        let new: Vec<_> = new.iter().filter(|l| !in_town_cell(l)).map(shape).collect();
        assert_eq!(old, new, "seed {seed}");
    }
}

#[test]
fn v9_islands_are_furnished_with_everything_the_plan_lists() {
    let settings = v9(11);
    let lands = lands_touching(&settings, &island_bands(-8_000, 8_000));
    let count = |pick: &dyn Fn(&IsleContent) -> bool, kind: SkyLandKind| {
        lands
            .iter()
            .filter(|land| land.kind == kind && land.content().is_some_and(pick))
            .count()
    };
    for kind in [SkyLandKind::Isle, SkyLandKind::ArchipelagoIsle] {
        assert!(count(&|c| c.cache.is_some(), kind) > 5, "{kind:?} caches");
        assert!(count(&|c| c.spring.is_some(), kind) > 3, "{kind:?} springs");
        assert!(count(&|c| c.vent.is_some(), kind) > 3, "{kind:?} vents");
        assert!(
            count(&|c| c.keystone.is_some(), kind) > 2,
            "{kind:?} crumbling"
        );
    }
    assert!(count(&|c| c.nest.is_some(), SkyLandKind::ArchipelagoIsle) > 3);
    assert!(count(&|c| c.ruin.is_some(), SkyLandKind::ArchipelagoIsle) > 3);
    let buoys: Vec<&SkyLand> = lands
        .iter()
        .filter(|land| isles::buoy_of(land).is_some())
        .collect();
    assert!(buoys.len() > 5, "buoys on the low isles");
    for land in &buoys {
        assert_eq!(land.kind, SkyLandKind::Isle, "buoys moor to low isles only");
        let content = land.content().expect("furnished");
        assert!(content.keystone.is_none(), "a buoy isle never crumbles");
        let buoy = isles::buoy_of(land).expect("buoy");
        for dx in -1..=1 {
            assert_eq!(
                land.tile_at(buoy.pad.x + dx, buoy.pad.y),
                Some(TileKind::Sky(SkyTile::BuoyPad)),
                "{land:?}"
            );
        }
        // The pad and a column either side are flat (a rig parks level).
        for dx in -2..=2 {
            assert_eq!(land.surface_row(buoy.pad.x + dx), Some(buoy.pad.y));
        }
    }
    for land in lands
        .iter()
        .filter(|land| land.kind == SkyLandKind::TownIsle)
    {
        let content = land.content().expect("furnished");
        assert!(content.keystone.is_none() && content.cache.is_none());
    }
}

#[test]
fn furnished_tiles_sit_where_the_content_says_and_ore_grows_in_the_rock() {
    let settings = v9(23);
    let lands = lands_touching(&settings, &island_bands(-6_000, 6_000));
    let mut ores: BTreeSet<MineralKind> = BTreeSet::new();
    for land in &lands {
        let content = *land.content().expect("V9 islands are furnished");
        let at = |position: TilePosition| land.tile_at(position.x, position.y);
        if let Some(cache) = content.cache {
            assert_eq!(at(cache), Some(TileKind::Sky(SkyTile::Cache)));
        }
        if let Some(keystone) = content.keystone {
            assert_eq!(at(keystone), Some(TileKind::Sky(SkyTile::Keystone)));
        }
        if let Some(spring) = content.spring {
            assert_eq!(at(spring), Some(TileKind::Sky(SkyTile::Spring)));
        }
        if let Some(vent) = content.vent {
            assert_eq!(at(vent), Some(TileKind::Sky(SkyTile::Vent)));
        }
        if let Some(nest) = content.nest {
            assert_eq!(at(nest), Some(TileKind::Sky(SkyTile::Nest)));
        }
        if let Some(first) = content.ruin {
            let top = land.surface_row(first).expect("on the island");
            assert_eq!(
                land.tile_at(first, top - 1),
                Some(TileKind::Sky(SkyTile::RuinPillar))
            );
        }
        for position in land.bounds().positions() {
            if let Some(TileKind::Ore(mineral)) = land.tile_at(position.x, position.y) {
                ores.insert(mineral);
            }
        }
    }
    assert!(ores.contains(&MineralKind::Skystone), "{ores:?}");
    assert!(ores.contains(&MineralKind::CloudSilver), "{ores:?}");
    assert!(
        ores.iter()
            .all(|ore| matches!(ore, MineralKind::Skystone | MineralKind::CloudSilver)),
        "only sky ore grows in the sky: {ores:?}"
    );
}

#[test]
fn v9_chunks_are_seamless_and_deterministic() {
    let settings = v9(41);
    let site = sky_town_site(&settings, 0, 1).expect("a town");
    let land = land(&settings, site.isle).expect("the town isle");
    let step = CHUNK_SIZE_TILES;
    let bounds = land.bounds();
    for chunk_x in bounds.min.x.div_euclid(step) - 1..=bounds.max.x.div_euclid(step) + 1 {
        for chunk_y in bounds.min.y.div_euclid(step) - 1..=bounds.max.y.div_euclid(step) + 1 {
            let chunk = ChunkPosition {
                x: chunk_x,
                y: chunk_y,
            };
            let tiles: Vec<Tile> = streamed::generated_chunk(&settings, chunk).tiles().to_vec();
            let again: Vec<Tile> = streamed::generated_chunk(&settings, chunk).tiles().to_vec();
            assert_eq!(tiles, again);
            // Every non-stamped tile matches the single-tile query.
            for (index, tile) in tiles.iter().enumerate() {
                let index = i32::try_from(index).expect("small");
                let position = TilePosition {
                    x: chunk_x * step + index % step,
                    y: chunk_y * step + index / step,
                };
                if tile.kind == TileKind::Foundation {
                    continue;
                }
                assert_eq!(
                    tile_at(&settings, position).unwrap_or(TileKind::Air),
                    tile.kind,
                    "{position:?}"
                );
            }
        }
    }
}

#[test]
fn every_block_has_one_sky_town_on_a_flat_town_isle() {
    for seed in [1_u64, 9, 0xBEEF] {
        let settings = v9(seed);
        let mut ids = BTreeSet::new();
        let mut kinds = BTreeSet::new();
        for bx in -4..=4 {
            for by in 0..towns::TOWN_BLOCK.1 * 5 {
                let Some(site) = sky_town_site(&settings, bx, by) else {
                    continue;
                };
                assert!(ids.insert(site.id), "unique ids");
                assert_eq!(placement::decode_sky_town(site.id), Some((bx, by)));
                kinds.insert(site.kind);
                let isle = land(&settings, site.isle).expect("isle");
                assert_eq!(isle.kind, SkyLandKind::TownIsle);
                assert_eq!(
                    SkyBand::at_row_in(site.ground_row, &V9_SKY_BANDS),
                    SkyBand::Archipelago
                );
                for x in site.first..=site.last {
                    assert_eq!(isle.surface_row(x), Some(site.ground_row), "flat at {x}");
                }
                // The town isle stands clear of the cell's other islands.
                let lands = lands_touching(&settings, &isle.bounds().expanded(4));
                for other in lands.iter().filter(|other| other.id != isle.id) {
                    assert!(
                        !other.bounds().intersects(&isle.bounds()),
                        "{other:?} overlaps {isle:?}"
                    );
                }
                // The village is built there, in a sky culture.
                let town = village::village(&settings, site.id).expect("a sky town");
                assert_eq!(town.setting, VillageSetting::Sky(site.kind));
                assert!(town.culture.is_sky());
                assert_eq!(town.ground_row, site.ground_row);
                assert!(
                    town.buildings
                        .iter()
                        .any(|building| building.kind == town.specialty.building()),
                    "its specialty building"
                );
                assert!(town.buildings.iter().all(|building| {
                    building.first_column >= site.first && building.last_column() <= site.last
                }));
                let middle = TilePosition {
                    x: site.center(),
                    y: site.ground_row - 2,
                };
                assert_eq!(
                    village::village_at(&settings, middle).map(|found| found.id),
                    Some(site.id)
                );
                assert!(streamed::in_village_area(&settings, middle, 0));
            }
        }
        assert!(ids.len() > 20, "seed {seed}: {}", ids.len());
        assert!(kinds.len() >= 3, "seed {seed}: {kinds:?}");
    }
}

#[test]
fn sky_content_sits_in_its_tank_tier_band() {
    // Reachability per tank tier band (climb table, still air, no intakes): every buoy hangs in
    // the low isles a tier 4 tank and engine climb to, and some within a tier 3 tank's reach;
    // every sky town stands below the dead-air gap, within what a rig without intakes can
    // climb to, and the lowest row of towns is open to mid tiers (tank 7, engine 8).
    let reach =
        |tank, engine, intake| crate::climb::reach_in(V9_SKY_BANDS.climb, tank, engine, intake);
    for seed in [3_u64, 0x5EED, 0x00C0_FFEE] {
        let settings = v9(seed);
        let mut buoys = Vec::new();
        for land in lands_touching(&settings, &island_bands(-6_000, 6_000)) {
            if let Some(buoy) = isles::buoy_of(&land) {
                assert_eq!(land.kind, SkyLandKind::Isle, "buoys moor to low isles");
                buoys.push(-buoy.pad.y);
            }
        }
        assert!(buoys.len() >= 6, "seed {seed}: {} buoys", buoys.len());
        for altitude in &buoys {
            assert!(
                (V9_SKY_BANDS.low_isles..V9_SKY_BANDS.archipelago).contains(altitude),
                "seed {seed}: a buoy at {altitude} rows"
            );
            assert!(
                *altitude + 4 <= reach(4, 4, 0),
                "seed {seed}: a buoy at {altitude} rows is out of a tier 4 rig's reach"
            );
        }
        assert!(
            buoys.iter().any(|altitude| *altitude + 4 <= reach(3, 4, 0)),
            "seed {seed}: no buoy within a tier 3 tank's reach"
        );
        let mut lowest_row_towns = 0;
        for bx in -6..=6 {
            for by in 0..towns::TOWN_BLOCK.1 * 6 {
                let Some(site) = sky_town_site(&settings, bx, by) else {
                    continue;
                };
                let altitude = -site.ground_row;
                assert!(
                    altitude + towns::SKY_TOWN_AIR_ROWS < V9_SKY_BANDS.dead_air,
                    "seed {seed}: a sky town in the dead-air gap ({altitude} rows)"
                );
                assert!(
                    altitude + 4 <= reach(15, 15, 0),
                    "seed {seed}: a sky town at {altitude} rows needs intakes"
                );
                if by == 0 {
                    lowest_row_towns += 1;
                    assert!(
                        altitude + 4 <= reach(7, 8, 0),
                        "seed {seed}: the lowest towns ({altitude} rows) are out of mid tiers"
                    );
                }
            }
        }
        assert!(lowest_row_towns > 6, "seed {seed}: {lowest_row_towns}");
    }
}

#[test]
fn sky_towns_are_regional_village_features() {
    let settings = v9(17);
    let site = sky_town_site(&settings, 1, 2).expect("a town");
    let from = TilePosition {
        x: site.center() + 30,
        y: site.ground_row - 10,
    };
    let terrain = crate::terrain::Terrain::generate(&settings);
    let nearest = terrain
        .nearest_feature(from, crate::worldgen::FeatureKind::Village, 2_000, |_| true)
        .expect("a village");
    assert_eq!(nearest.village(), Some(site.id));
    assert_eq!(terrain.feature(nearest.id), Some(nearest));
    let near = village::sky_towns_in(&settings, &site.area());
    assert!(near.iter().any(|found| found.id == site.id));
    // Column queries (ground villages, camps, trails) leave sky towns out.
    let between = village::villages_between(&settings, site.center(), site.center());
    assert!(between.iter().all(|found| found.id != site.id));
}

#[test]
fn jet_streams_and_vents_blow_where_they_should() {
    let settings = v9(29);
    let area = island_bands(-3_000, 3_000);
    let streams = jet_streams_touching(&settings, &area);
    assert!(streams.len() > 10);
    for stream in &streams {
        let middle = TilePosition {
            x: i32::midpoint(stream.rect.min.x, stream.rect.max.x),
            y: i32::midpoint(stream.rect.min.y, stream.rect.max.y),
        };
        assert_eq!(
            jet_stream_at(&settings, middle.x, middle.y),
            stream.velocity,
            "{stream:?}"
        );
        assert_eq!(jet_stream_at(&settings, middle.x, stream.rect.min.y - 1), 0);
        // The ribbon flies over the chain's islands, never through their turf.
        for land in lands_touching(&settings, &stream.rect) {
            assert!(
                land.surface_row(middle.x)
                    .is_none_or(|top| top > stream.rect.max.y),
                "{land:?}"
            );
        }
    }
    let vents: Vec<TilePosition> = lands_touching(&settings, &area)
        .iter()
        .filter_map(|land| land.content().and_then(|content| content.vent))
        .collect();
    assert!(!vents.is_empty());
    for vent in vents {
        assert!(vent_updraft_at(&settings, vent.x, vent.y - 3) >= winds::VENT_STRENGTH);
        assert_eq!(vent_updraft_at(&settings, vent.x + 3, vent.y - 3), 0);
        let env = crate::environment::environment_at(
            &settings,
            (vent.x as f32 + 0.5) * crate::terrain::TILE_SIZE,
            (vent.y as f32 - 3.0) * crate::terrain::TILE_SIZE,
        );
        assert!(env.updraft >= winds::VENT_STRENGTH as f32);
    }
    assert_eq!(vent_updraft_at(&v8(29), 0, -400), 0);
}

#[test]
fn chunk_digests_of_v9_skies_are_pinned() {
    // V9 = V8 + the furnished isles and sky towns. Pinned once it ships.
    let mut combined = 0_u64;
    for seed in [1_u64, 0xD1_11_6A_4E] {
        let settings = v9(seed);
        let town = sky_town_site(&settings, 0, 2).expect("a town");
        let chunks = [
            (
                5,
                row_of(V9_SKY_BANDS.low_isles + 150).div_euclid(CHUNK_SIZE_TILES),
            ),
            (
                -9,
                row_of(V9_SKY_BANDS.archipelago + 500).div_euclid(CHUNK_SIZE_TILES),
            ),
            (
                town.center().div_euclid(CHUNK_SIZE_TILES),
                town.ground_row.div_euclid(CHUNK_SIZE_TILES),
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
        for land in lands_touching(&settings, &island_bands(-4_000, 4_000)) {
            let content = land.content().copied().unwrap_or_default();
            combined = combined.rotate_left(5)
                ^ (land.id.0
                    ^ u64::from(land.center_x.unsigned_abs())
                    ^ u64::from(content.keystone.is_some())
                    ^ (u64::from(content.buoy.is_some()) << 1));
        }
    }
    assert_eq!(
        combined, GOLDEN_V9_DIGEST,
        "V9 generation changed: {combined:#018x}"
    );
}

const GOLDEN_V9_DIGEST: u64 = 0xcf20_9927_7491_267f;
