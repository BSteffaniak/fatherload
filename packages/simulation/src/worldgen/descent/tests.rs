//! Generator V11: the Deep Descent layout, its ragged transitions, the classic rules read
//! through host bands, the Hollow World cavern, landmarks and the pinned output.

use super::{landmarks::*, *};
use crate::{
    base::BuiltTile,
    cave::CaveTile,
    descent::{LandmarkKind, Stratum},
    terrain::{ChunkPosition, TileKind, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSize,
        strata::{self, band_at, band_layout_rows},
        streamed::{generated_chunk, generated_tile},
    },
};

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

#[test]
fn v11_is_selectable_but_not_the_default() {
    assert_eq!(GeneratorVersion::CURRENT, GeneratorVersion::V10);
    assert_eq!(GeneratorVersion::DEEP_DESCENT, GeneratorVersion::V11);
    assert!(GeneratorVersion::ALL.contains(&GeneratorVersion::V11));
    let settings = v11(1);
    let generator = settings.generator;
    assert!(generator.has_descents() && generator.is_streamed() && generator.is_deep());
    assert!(generator.has_sky() && generator.has_moon_content() && generator.has_tombs());
    assert_eq!(generator.sky_bands(), GeneratorVersion::V10.sky_bands());
    assert!(!GeneratorVersion::V10.has_descents());
    assert_eq!(generator.name(), "v11");
}

#[test]
fn the_layout_fills_about_thirteen_thousand_rows_in_order() {
    for seed in 0..60 {
        for size in WorldSize::ALL {
            let settings = WorldSettings { size, ..v11(seed) };
            let layout = layout(&settings);
            assert_eq!(layout.tops[0], SKY_ROWS);
            assert_eq!(layout.floor, floor_row(&settings));
            let rows = layout.floor - SKY_ROWS;
            assert!(
                (12_900..=13_700).contains(&rows),
                "seed {seed}: {rows} rows"
            );
            for stratum in Stratum::ALL {
                let (top, bottom) = layout.rows(stratum);
                let thickness = bottom - top + 1;
                assert!(
                    thickness >= MIN_STRATUM_ROWS,
                    "seed {seed} {stratum:?}: {thickness}"
                );
                // Room for both bounding transition zones.
                let index = stratum.index();
                let above = index.checked_sub(1).map_or(0, |boundary| {
                    let (wave, half) = boundary_shape(boundary);
                    wave + half
                });
                let below = if index < BOUNDARY_COUNT {
                    let (wave, half) = boundary_shape(index);
                    wave + half
                } else {
                    strata::BEDROCK_WAVE_ROWS
                };
                assert!(thickness > above + below + 20, "seed {seed} {stratum:?}");
            }
            if size == WorldSize::Standard {
                // Each Descent stays near its target share (seeds vary them about a tenth).
                for descent in Descent::ALL {
                    let (top, bottom) = layout.descent_rows(descent);
                    let thickness = bottom - top + 1;
                    let target = descent.target_thickness();
                    assert!(
                        (thickness - target).abs() <= target / 6,
                        "seed {seed} {descent:?}: {thickness} vs {target}"
                    );
                }
            }
        }
    }
}

#[test]
fn seeds_move_the_strata() {
    let tops: Vec<[i32; STRATUM_COUNT]> = (0..80).map(|seed| layout(&v11(seed)).tops).collect();
    for stratum in 2..STRATUM_COUNT {
        let min = tops.iter().map(|tops| tops[stratum]).min().unwrap_or(0);
        let max = tops.iter().map(|tops| tops[stratum]).max().unwrap_or(0);
        assert!(
            max - min >= 40,
            "stratum {stratum} only moves {}",
            max - min
        );
    }
}

#[test]
fn strata_keep_their_order_in_every_column() {
    let settings = v11(5);
    let layout = layout(&settings);
    for x in (-300..300).step_by(37) {
        let mut previous = 0;
        for y in SKY_ROWS + 1..layout.floor {
            let index = stratum_in(&settings, &layout, x, y).index();
            assert!(index + 1 >= previous, "x {x} y {y}");
            assert!(index <= previous + 1, "x {x} y {y}");
            previous = previous.max(index);
        }
        assert_eq!(previous, STRATUM_COUNT - 1);
    }
}

#[test]
fn transitions_are_ragged_over_many_rows() {
    let settings = v11(7);
    let layout = layout(&settings);
    for index in 1..BOUNDARY_COUNT {
        let (shallowest, deepest) = boundary_depth_range(&layout, index);
        let (upper, lower) = (Stratum::ALL[index], Stratum::ALL[index + 1]);
        let mixed_rows = (shallowest..=deepest)
            .filter(|y| {
                let mut seen = (false, false);
                for x in 0..320 {
                    match stratum_in(&settings, &layout, x, *y) {
                        stratum if stratum == upper => seen.0 = true,
                        stratum if stratum == lower => seen.1 = true,
                        _ => {}
                    }
                }
                seen.0 && seen.1
            })
            .count();
        assert!(
            mixed_rows >= 16,
            "boundary {index}: {mixed_rows} mixed rows"
        );
        let lensed = (0..320).any(|x| {
            let mut seen_lower = false;
            (shallowest..=deepest).any(|y| {
                let stratum = stratum_in(&settings, &layout, x, y);
                seen_lower |= stratum == lower;
                seen_lower && stratum == upper
            })
        });
        assert!(lensed, "boundary {index} never interleaves");
        // The boundary waves along x.
        let depths: Vec<i32> = (0..400)
            .map(|x| boundary_column_depth(&settings, &layout, index, x))
            .collect();
        let spread = depths.iter().max().unwrap_or(&0) - depths.iter().min().unwrap_or(&0);
        assert!(spread >= 6, "boundary {index} is flat");
    }
}

#[test]
fn classic_rules_read_the_host_bands() {
    let settings = v11(9);
    let layout = layout(&settings);
    for x in [-40, 3, 97, 411] {
        for y in (SKY_ROWS + 1..layout.floor).step_by(53) {
            let stratum = stratum_at(&settings, x, y);
            assert_eq!(band_at(&settings, x, y), stratum.host_band(), "x {x} y {y}");
        }
    }
    // Descents I and II are the classic bands, in order, and nominal rows map onto the
    // classic stack monotonically there.
    for band in Band::ALL {
        let stratum = Stratum::of_band(band);
        assert_eq!(band_layout_rows(&settings, band), layout.rows(stratum));
        assert_eq!(
            strata::nominal_row(&settings, layout.rows(stratum).0),
            strata::nominal_band_top(band)
        );
    }
    let end = layout.tops[Band::ALL.len()];
    let mut previous = strata::nominal_row(&settings, SKY_ROWS);
    for row in SKY_ROWS..end {
        let nominal = strata::nominal_row(&settings, row);
        assert!(nominal >= previous, "row {row}");
        previous = nominal;
        let scaled = strata::scaled_row(&settings, nominal);
        assert!(
            scaled <= row && strata::nominal_row(&settings, scaled) == nominal,
            "row {row}"
        );
    }
    assert_eq!(strata::scaled_row(&settings, WORLD_HEIGHT_TILES), end);
    // Every deeper row maps inside its stratum's host band's classic rows.
    for row in (end..layout.floor).step_by(17) {
        let stratum = layout.stratum_of_row(row);
        let host = stratum.host_band();
        let nominal = strata::nominal_row(&settings, row);
        let top = strata::nominal_band_top(host);
        assert!(
            nominal >= top && nominal < top + 30,
            "row {row} {stratum:?}: {nominal}"
        );
        let depth = strata::nominal_depth(&settings, f32::from(u16::try_from(row).unwrap_or(0)));
        assert!((depth - f32::from(u16::try_from(nominal).unwrap_or(0))).abs() < 1.0);
    }
    assert!(strata::scaled_span(&settings, 22) > 200);
}

#[test]
fn scaled_row_matches_a_search_of_nominal_rows() {
    for seed in [0, 1, 7, 0xBEEF, 0x00C0_FFEE, u64::MAX] {
        for size in [WorldSize::Standard, WorldSize::Large] {
            let settings = WorldSettings { size, ..v11(seed) };
            let layout = layout(&settings);
            for nominal in -3..WORLD_HEIGHT_TILES + 3 {
                assert_eq!(
                    scaled_row(&layout, nominal),
                    scaled_row_by_search(&layout, nominal),
                    "seed {seed}, {size:?}, nominal row {nominal}"
                );
            }
        }
    }
}

#[test]
fn older_worlds_name_the_strata_of_their_bands() {
    for settings in [
        WorldSettings::new(3),
        WorldSettings::bounded(3),
        WorldSettings::legacy(3),
    ] {
        for y in (0..settings.height_tiles()).step_by(11) {
            let band = band_at(&settings, 20, y);
            let stratum = stratum_at(&settings, 20, y);
            assert_eq!(stratum, Stratum::of_band(band));
            assert!(stratum.is_classic());
        }
    }
    assert_eq!(
        stratum_at(&v11(3), 20, floor_row(&v11(3)) - 2),
        Stratum::TheHeart
    );
}

#[test]
fn the_hollow_world_is_a_giant_open_cavern() {
    let settings = v11(11);
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::InnerSky);
    let mut open = 0;
    let mut total = 0;
    for x in (-200..200).step_by(5) {
        let (ceiling, floor) = hollow_rows(&settings, &layout, x);
        assert!(ceiling > top && floor < bottom, "x {x}");
        assert!(
            floor - ceiling > 280,
            "x {x}: only {} rows",
            floor - ceiling
        );
        for y in (ceiling..=floor).step_by(3) {
            total += 1;
            open += usize::from(hollow_open(&settings, &layout, x, y));
        }
    }
    assert!(open * 100 / total >= 65, "only {open}/{total} open");
    assert!(open < total, "no floating stones at all");
    // The generated terrain shows it: a column through the middle is mostly air.
    let x = 40;
    let (ceiling, floor) = hollow_rows(&settings, &layout, x);
    let air = (ceiling..=floor)
        .filter(|y| generated_tile(&settings, TilePosition { x, y: *y }).kind == TileKind::Air)
        .count();
    assert!(air * 100 / usize::try_from(floor - ceiling + 1).unwrap_or(1) >= 55);
    // No other world has one.
    assert!(!WorldSettings::new(11).generator.has_descents());
}

#[test]
fn every_descent_has_an_outpost_and_a_gate_chamber() {
    for seed in [1, 2, 3, 0xD1_11_6A_4E] {
        let settings = v11(seed);
        let layout = layout(&settings);
        for descent in Descent::ALL {
            let outpost = outpost(&settings, &layout, descent).expect("an outpost");
            let (top, bottom) = layout.descent_rows(descent);
            assert!(
                outpost.bounds.min.y > top && outpost.bounds.max.y < bottom,
                "{descent:?}"
            );
            assert!(
                outpost
                    .stratum
                    .def()
                    .landmarks
                    .contains(&LandmarkKind::Outpost)
            );
            // Off the spawn shaft, close enough to find.
            assert!(
                !(outpost.bounds.min.x..=outpost.bounds.max.x)
                    .contains(&crate::worldgen::SPAWN_TILE_X)
            );
            assert!((outpost.bounds.min.x - crate::worldgen::SPAWN_TILE_X).abs() < 200);
            let gate = gate_chamber(&settings, &layout, descent);
            assert_eq!(gate.stratum, descent.last_stratum());
            assert!(gate.bounds.min.y > top && gate.bounds.max.y < layout.floor);
            assert!(gate.bounds.max.y < bottom + 1, "{descent:?}");
        }
        // The stamps are in the generated terrain.
        let first = outpost(&settings, &layout, Descent::OldDeep).expect("an outpost");
        let corner = generated_tile(&settings, first.bounds.min).kind;
        assert_eq!(corner, TileKind::Built(BuiltTile::Plating), "seed {seed}");
        let inside = TilePosition {
            x: first.bounds.min.x + 5,
            y: first.bounds.min.y + 3,
        };
        assert_eq!(generated_tile(&settings, inside).kind, TileKind::Air);
        let gate = gate_chamber(&settings, &layout, Descent::LivingDark);
        assert_eq!(
            generated_tile(&settings, gate.bounds.max).kind,
            TileKind::Cave(CaveTile::RuinBrick)
        );
        // The Hollow World's outpost stands on the cavern's ground.
        let hollow = outpost(&settings, &layout, Descent::HollowWorld).expect("an outpost");
        assert_eq!(hollow.stratum, Stratum::InnerSky);
        // Most of its floor rests on rock (a cave may run under part of it: the plated floor
        // holds).
        let resting = (hollow.bounds.min.x..=hollow.bounds.max.x)
            .filter(|x| {
                let below = TilePosition {
                    x: *x,
                    y: hollow.bounds.max.y + 1,
                };
                generated_tile(&settings, below).kind != TileKind::Air
            })
            .count();
        assert!(
            resting * 2 > usize::try_from(OUTPOST_SIZE.0).unwrap_or(0),
            "seed {seed}: {resting} columns rest on rock"
        );
    }
}

#[test]
fn anomaly_rooms_appear_in_the_strange_layers_only() {
    let settings = v11(13);
    let layout = layout(&settings);
    let (top, bottom) = layout.descent_rows(Descent::StrangeLayers);
    let area = TileRect {
        min: TilePosition {
            x: -800,
            y: SKY_ROWS,
        },
        max: TilePosition {
            x: 800,
            y: layout.floor,
        },
    };
    let rooms: Vec<Landmark> = landmarks_with(&settings, &layout, &area)
        .into_iter()
        .filter(|landmark| landmark.kind == LandmarkKind::AnomalyRoom)
        .collect();
    assert!(rooms.len() >= 20, "only {} rooms", rooms.len());
    for room in &rooms {
        assert_eq!(room.descent, Descent::StrangeLayers);
        assert!(room.bounds.min.y >= top && room.bounds.max.y <= bottom);
        assert!(
            room.stratum
                .def()
                .landmarks
                .contains(&LandmarkKind::AnomalyRoom)
        );
    }
    let room = rooms[0];
    assert_eq!(
        generated_tile(&settings, room.bounds.min).kind,
        TileKind::Cave(CaveTile::Crystal)
    );
    // Every kind the registry names has a generator.
    for kind in LandmarkKind::ALL {
        let _ = landmark_generator(kind);
    }
    assert!(landmarks_in(&WorldSettings::new(13), &area).is_empty());
}

#[test]
fn deep_strata_use_their_registry_rock_and_ore() {
    let settings = v11(17);
    let layout = layout(&settings);
    let mut ore_by_descent = [0_usize; 8];
    let (mut listed, mut deep_ore) = (0_usize, 0_usize);
    for descent in Descent::ALL {
        for stratum in descent.strata() {
            let (top, _) = layout.rows(stratum);
            for chunk_x in -3..3 {
                for chunk_y in 0..4 {
                    let chunk = ChunkPosition {
                        x: chunk_x,
                        y: (top + 40).div_euclid(16) + chunk_y,
                    };
                    let origin = chunk.origin();
                    for (index, tile) in
                        generated_chunk(&settings, chunk).tiles().iter().enumerate()
                    {
                        let TileKind::Ore(mineral) = tile.kind else {
                            continue;
                        };
                        ore_by_descent[descent.index()] += 1;
                        let local = i32::try_from(index).unwrap_or(0);
                        let here = stratum_in(
                            &settings,
                            &layout,
                            origin.x + local % 16,
                            origin.y + local / 16,
                        );
                        if !here.is_classic() {
                            deep_ore += 1;
                            listed += usize::from(
                                here.def().ores.iter().any(|ore| ore.mineral == mineral),
                            );
                        }
                    }
                }
            }
        }
    }
    for (descent, count) in ore_by_descent.iter().enumerate() {
        assert!(
            *count > 20,
            "Descent {descent} holds only {count} ore tiles"
        );
    }
    // Veins, pods and motherlodes below Descent II follow the registry's ore curves (prefab
    // formations still deal their host band's ore).
    assert!(
        listed * 100 / deep_ore.max(1) >= 80,
        "{listed} of {deep_ore} deep ore listed"
    );
    // Each placeholder stratum's own rock.
    let fungal = layout.rows(Stratum::FungalForest);
    let rock: Vec<TileKind> = (fungal.0 + 30..fungal.0 + 60)
        .map(|y| generated_tile(&settings, TilePosition { x: 300, y }).kind)
        .collect();
    let def = Stratum::FungalForest.def();
    assert!(
        rock.iter()
            .any(|kind| *kind == def.soft || *kind == def.hard)
    );
}

#[test]
fn v11_generation_is_deterministic_and_cache_independent() {
    let settings = v11(21);
    let layout = layout(&settings);
    let chunks: Vec<ChunkPosition> = Descent::ALL
        .iter()
        .map(|descent| {
            let (top, bottom) = layout.descent_rows(*descent);
            ChunkPosition::of(TilePosition {
                x: 120,
                y: i32::midpoint(top, bottom),
            })
        })
        .collect();
    let first: Vec<Vec<u64>> = chunks
        .iter()
        .map(|chunk| {
            generated_chunk(&settings, *chunk)
                .tiles()
                .iter()
                .map(|tile| tile.code())
                .collect()
        })
        .collect();
    crate::worldgen::streamed::clear_caches();
    // Generate in reverse order: chunks never depend on what was generated first.
    let mut second: Vec<Vec<u64>> = chunks
        .iter()
        .rev()
        .map(|chunk| {
            generated_chunk(&settings, *chunk)
                .tiles()
                .iter()
                .map(|tile| tile.code())
                .collect()
        })
        .collect();
    second.reverse();
    assert_eq!(first, second);
}

/// The chunks the golden digest covers: the town's ground, the middle of every Descent, every
/// Descent's outpost and gate chamber, the Hollow World cavern and an anomaly room.
fn golden_chunks(settings: &WorldSettings) -> Vec<ChunkPosition> {
    let layout = layout(settings);
    let mut chunks = vec![ChunkPosition { x: 3, y: 0 }, ChunkPosition { x: 6, y: 1 }];
    for descent in Descent::ALL {
        let (top, bottom) = layout.descent_rows(descent);
        chunks.push(ChunkPosition::of(TilePosition {
            x: 50,
            y: i32::midpoint(top, bottom),
        }));
        if let Some(outpost) = outpost(settings, &layout, descent) {
            chunks.push(ChunkPosition::of(outpost.bounds.min));
        }
        chunks.push(ChunkPosition::of(
            gate_chamber(settings, &layout, descent).bounds.min,
        ));
    }
    let (ceiling, _) = hollow_rows(settings, &layout, 0);
    chunks.push(ChunkPosition::of(TilePosition {
        x: 0,
        y: ceiling + 100,
    }));
    let area = TileRect {
        min: TilePosition { x: -400, y: 0 },
        max: TilePosition {
            x: 400,
            y: layout.floor,
        },
    };
    if let Some(room) = landmarks_with(settings, &layout, &area)
        .into_iter()
        .find(|landmark| landmark.kind == LandmarkKind::AnomalyRoom)
    {
        chunks.push(ChunkPosition::of(room.bounds.min));
    }
    chunks.push(ChunkPosition::of(TilePosition {
        x: 9,
        y: layout.floor - 1,
    }));
    chunks
}

/// Pins V11's generated tiles. Change it only with a new generator version.
#[test]
fn v11_generation_is_pinned() {
    let settings = v11(crate::worldgen::DEFAULT_WORLD_SEED);
    let mut combined = 0_u64;
    for chunk in golden_chunks(&settings) {
        for (index, tile) in generated_chunk(&settings, chunk).tiles().iter().enumerate() {
            combined = combined
                .rotate_left(7)
                .wrapping_add(tile.code() ^ (index as u64).wrapping_mul(0x9E37));
        }
    }
    assert_eq!(
        combined, GOLDEN_V11_DIGEST,
        "V11 generation changed: {combined:#018x}"
    );
}

/// The digest [`v11_generation_is_pinned`] expects (Wave 7 integration: the stratum fluid
/// pockets, the outposts on the builders' cable and the ancient lifts' carve).
const GOLDEN_V11_DIGEST: u64 = 0x0c5f_b99d_3d82_b654;

#[test]
#[ignore = "debug: prints a V11 world's layout"]
fn print_layout() {
    let settings = v11(crate::worldgen::DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    for stratum in Stratum::ALL {
        let (top, bottom) = layout.rows(stratum);
        println!(
            "{:>4} {:<6} {:<26} {top:>6}..{bottom:>6} ({} rows) host {:?}",
            stratum.index(),
            stratum.descent().numeral(),
            stratum.name(),
            bottom - top + 1,
            stratum.host_band()
        );
    }
    let start = std::time::Instant::now();
    for y in 0..60 {
        let _ = generated_chunk(&settings, ChunkPosition { x: 4, y: y * 13 });
    }
    println!("60 chunks in {:?}", start.elapsed());
}

#[test]
#[ignore = "debug: times V10 and V11 chunk generation"]
fn time_chunks() {
    for generator in [GeneratorVersion::V10, GeneratorVersion::V11] {
        let settings = WorldSettings {
            generator,
            ..WorldSettings::new(crate::worldgen::DEFAULT_WORLD_SEED)
        };
        let start = std::time::Instant::now();
        for y in 1..50 {
            let _ = generated_chunk(&settings, ChunkPosition { x: 4, y });
        }
        println!("{generator:?}: 49 chunks in {:?}", start.elapsed());
    }
}
