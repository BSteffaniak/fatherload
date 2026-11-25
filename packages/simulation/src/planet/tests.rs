//! Planet ground generation: determinism, wrap, rings, strata, caves and the core chamber.

use super::*;
use crate::terrain::{Terrain, is_indestructible};

fn moon() -> PlanetGround {
    PlanetGround::new(PlanetParams::new(0x05EE_D400, 400, PlanetThemeId::BARREN))
}

#[test]
fn generation_is_a_pure_function_of_the_params() {
    let a = moon();
    let b = moon();
    for (column, row) in [(0, 0), (17, 40), (2_559, 200), (1_000, 360), (-5, 3)] {
        assert_eq!(a.generated_tile(column, row), b.generated_tile(column, row));
    }
    // The chunk cache never changes a result.
    let chunk = ChunkPosition { x: 3, y: 5 };
    assert_eq!(
        a.chunk_tiles(chunk),
        a.generated_chunk(chunk).tiles().to_vec()
    );
    // Another seed is another moon.
    let other = PlanetGround::new(PlanetParams::new(0x0BAD, 400, PlanetThemeId::BARREN));
    let differs = (0..2_560).any(|column| a.surface_row(column) != other.surface_row(column));
    assert!(differs);
}

#[test]
fn the_strip_wraps_seamlessly() {
    for ground in [
        moon(),
        PlanetGround::new(PlanetParams::new(7, 1_500, PlanetThemeId::ROCKY)),
    ] {
        let width = ground.strip_width();
        for row in [-10, 0, 5, 60, ground.bottom_row() - 20] {
            for column in [0, 1, 37, width - 1] {
                assert_eq!(
                    ground.generated_tile(column, row),
                    ground.generated_tile(column + width, row)
                );
                assert_eq!(
                    ground.generated_tile(column, row),
                    ground.generated_tile(column - 3 * width, row)
                );
            }
        }
        // Hills join across the seam: no cliff between the last column and the first.
        let seam = (ground.surface_row(width - 1) - ground.surface_row(0)).abs();
        assert!(seam <= 2, "seam step {seam}");
    }
}

#[test]
fn the_surface_has_hills_and_open_sky_above() {
    let ground = moon();
    let rows: Vec<i32> = (0..ground.strip_width())
        .map(|column| ground.surface_row(column))
        .collect();
    let (high, low) = (
        *rows.iter().min().expect("rows"),
        *rows.iter().max().expect("rows"),
    );
    assert!(low - high >= 3, "hills {high}..{low}");
    assert!(-high <= ground.max_hill_rows());
    assert!(ground.strip_top() < high);
    for column in (0..ground.strip_width()).step_by(97) {
        let surface = ground.surface_row(column);
        assert!(ground.generated_tile(column, surface - 1).kind.is_open());
        assert!(!ground.generated_tile(column, surface).kind.is_open());
        assert_eq!(
            ground.generated_tile(column, surface).kind,
            ground.theme().crust,
            "the crust tops every column"
        );
    }
}

#[test]
fn every_cell_of_a_ring_tile_generates_alike() {
    let ground = moon();
    let grid = *ground.grid();
    for start in grid.halving_rows() {
        let row = start + 1;
        if row >= ground.core_chamber_top() {
            continue;
        }
        let span = grid.span_at(row);
        assert!(span >= 2);
        for tile_column in 0..grid.columns_at(row).min(200) {
            let tile = crate::polar::PolarTile {
                column: tile_column,
                row,
            };
            let first = grid.first_strip_column(tile);
            let expected = ground.generated_tile(i32::try_from(first).expect("col"), row);
            for cell in first..first + span {
                assert_eq!(
                    ground.generated_tile(i32::try_from(cell).expect("col"), row),
                    expected,
                    "row {row} tile {tile_column}"
                );
            }
        }
    }
}

#[test]
fn strata_ores_caves_and_the_core_chamber() {
    let ground = moon();
    let theme = ground.theme();
    let width = ground.strip_width();
    let mut ores = 0;
    let mut open_underground = 0;
    let mut strata_seen = std::collections::BTreeSet::new();
    for column in (0..width).step_by(5) {
        let surface = ground.surface_row(column);
        for row in (surface + theme.crust_rows..ground.core_chamber_top()).step_by(3) {
            let tile = ground.generated_tile(column, row);
            if matches!(tile.kind, TileKind::Ore(_)) {
                ores += 1;
            }
            if tile.kind.is_open() {
                open_underground += 1;
            }
            strata_seen.insert(ground.stratum_index(column, row));
        }
    }
    assert!(ores > 100, "ores {ores}");
    assert!(open_underground > 20, "caves {open_underground}");
    assert_eq!(strata_seen.len(), theme.strata.len());
    // The core chamber is open all the way round, over an indestructible core floor.
    for column in (0..width).step_by(11) {
        let top = ground.core_chamber_top();
        let floor = ground.core_floor_row();
        let open = (top..floor)
            .filter(|row| ground.generated_tile(column, *row).kind.is_open())
            .count();
        assert!(open >= 6, "column {column}: {open} open chamber rows");
        for row in floor..=ground.bottom_row() {
            assert!(is_indestructible(ground.generated_tile(column, row).kind));
        }
    }
    assert_eq!(
        ground.layer_name(0, ground.core_chamber_top()),
        "Core Chamber"
    );
    assert_eq!(ground.layer_name(0, ground.strip_top()), theme.name);
}

#[test]
fn every_theme_generates_and_unknown_ids_fall_back() {
    for (index, theme) in PLANET_THEMES.iter().enumerate() {
        assert_eq!(usize::from(theme.id.0), index, "ids are indexes");
        let ground = PlanetGround::new(PlanetParams::new(3, 600, theme.id));
        let surface = ground.surface_row(10);
        assert_eq!(ground.generated_tile(10, surface).kind, theme.crust);
        let shares: u32 = theme
            .strata
            .iter()
            .map(|s| u32::from(s.share_permille))
            .sum();
        assert_eq!(shares, 1_000, "{}", theme.name);
    }
    assert_eq!(super::theme(PlanetThemeId(999)).id, PlanetThemeId::BARREN);
}

#[test]
fn params_clamp_and_gravity_scales_with_size() {
    let small = PlanetParams::new(1, 10, PlanetThemeId::BARREN);
    assert_eq!(small.radius, crate::polar::MIN_LANDABLE_RADIUS);
    let big = PlanetParams::new(1, 10_000, PlanetThemeId::ROCKY);
    assert!(big.gravity() > small.gravity());
    assert!(small.gravity() >= MIN_PLANET_GRAVITY);
    let weird = PlanetParams {
        gravity_milli: 0,
        ..small
    };
    assert!((weird.gravity() - MIN_PLANET_GRAVITY).abs() < f32::EPSILON);
}

#[test]
fn a_planet_terrain_reads_its_ground_and_wraps() {
    let params = PlanetParams::new(0x05EE_D400, 400, PlanetThemeId::BARREN);
    let ground = PlanetGround::new(params);
    let terrain = Terrain::planet(params);
    let width = ground.strip_width();
    for (column, row) in [(0, 0), (5, 30), (width - 1, 100), (1_234, 300)] {
        let position = crate::terrain::TilePosition { x: column, y: row };
        assert_eq!(
            terrain.tile(position),
            Some(ground.generated_tile(column, row))
        );
        let lapped = crate::terrain::TilePosition {
            x: column + width,
            y: row,
        };
        assert_eq!(terrain.tile(lapped), terrain.tile(position));
    }
    assert_eq!(terrain.min_y(), ground.strip_top());
    assert_eq!(terrain.height(), ground.bottom_row() + 1);
    assert!(terrain.frame().floating());
    assert_eq!(terrain.surface_row(width + 3), ground.surface_row(3));
    assert!(
        terrain
            .caves_touching(&crate::worldgen::TileRect {
                min: crate::terrain::TilePosition { x: 0, y: 0 },
                max: crate::terrain::TilePosition { x: 100, y: 100 },
            })
            .is_empty()
    );
}
