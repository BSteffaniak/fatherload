//! Property tests of the polar grid: round trips, wrap, halving neighbours, tile shapes and the
//! F12 table.

use super::*;
use crate::rng::DeterministicRngState;

/// Radii covering moons, planets and the edges of the range.
const RADII: [u32; 10] = [
    400, 401, 555, 777, 1_000, 1_500, 2_345, 5_000, 7_777, 10_000,
];

fn rng(seed: u64) -> DeterministicRngState {
    DeterministicRngState::new(seed)
}

fn unit(rng: &mut DeterministicRngState) -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "53 random bits")]
    let value = (rng.next_u64() >> 11) as f64 / (1_u64 << 53) as f64;
    value
}

#[test]
fn surface_columns_are_whole_sectors_and_nearly_square() {
    for radius in RADII {
        let grid = PolarGrid::new(radius);
        assert_eq!(
            i64::from(grid.surface_columns()) % SECTOR_TILES,
            0,
            "R={radius}"
        );
        assert_eq!(
            grid.lap_sectors() * SECTOR_TILES,
            i64::from(grid.surface_columns())
        );
        let aspect = grid.tile_aspect(0);
        assert!((0.9..=1.12).contains(&aspect), "R={radius}: {aspect}");
    }
}

#[test]
fn every_ring_keeps_its_tiles_between_point_seven_and_one_point_four_square() {
    for radius in [400, 450, 600, 999, 1_500, 3_000, 5_000, 10_000] {
        let grid = PolarGrid::new(radius);
        assert!(grid.halvings() >= 3, "R={radius} halves toward the core");
        for row in 0..=grid.bottom_row() {
            let aspect = grid.tile_aspect(row);
            assert!(
                (0.70..=1.42).contains(&aspect),
                "R={radius} row {row}: aspect {aspect}"
            );
        }
        // The rings are in order, each half the one above.
        let mut previous = grid.surface_columns();
        for start in grid.halving_rows() {
            assert_eq!(grid.columns_at(*start), previous / 2);
            assert_eq!(grid.columns_at(start - 1), previous);
            previous /= 2;
        }
        assert_eq!(grid.columns_at(grid.bottom_row()), previous);
        assert_eq!(
            grid.columns_at(-40),
            grid.surface_columns(),
            "hills are ring 0"
        );
    }
}

#[test]
fn a_moon_of_radius_400_has_the_expected_rings() {
    let grid = PolarGrid::new(400);
    assert_eq!(grid.surface_columns(), 2_560);
    assert_eq!(grid.lap_sectors(), 5);
    assert_eq!(grid.core_radius(), 33);
    assert_eq!(grid.bottom_row(), 366);
    assert_eq!(grid.halvings(), 4);
    // Each boundary sits where the ring's tiles would get narrower than 1/sqrt 2.
    for (k, start) in grid.halving_rows().iter().enumerate() {
        let above = grid.tile_aspect(start - 1) / 2.0;
        let would_be =
            TAU * grid.radius_at(f64::from(*start) + 0.5) / f64::from(grid.surface_columns() >> k);
        assert!(would_be < MIN_TILE_ASPECT, "ring {k} halves at {start}");
        assert!(above * 2.0 >= MIN_TILE_ASPECT);
    }
    assert_eq!(grid.columns_at(grid.bottom_row()), 160);
}

#[test]
fn the_f12_table_bow_and_width_difference() {
    // | Radius | Tile bow | Width difference | Horizon drop 20 tiles away |
    let table = [
        (400, 0.01, 0.25, 16.0),
        (1_500, 0.003, 0.07, 4.0),
        (5_000, 0.001, 0.02, 1.3),
    ];
    for (radius, bow, width, drop) in table {
        let grid = PolarGrid::new(radius);
        // Bow of a surface tile, measured on a square tile (F12's 32 / (8 R) px).
        let measured_bow = grid.tile_bow_px(0) / grid.tile_aspect(0).powi(2);
        assert!(
            (measured_bow / (32.0 / (8.0 * f64::from(radius))) - 1.0).abs() < 0.01,
            "R={radius} bow {measured_bow}"
        );
        assert!(
            (measured_bow - bow).abs() <= bow * 0.5,
            "R={radius}: bow {measured_bow} px vs the table's {bow}"
        );
        let difference = grid.tile_width_difference(0) * 100.0;
        assert!(
            (difference - 100.0 / f64::from(radius)).abs() < 1e-12,
            "R={radius}"
        );
        assert!(
            (difference - width).abs() <= width * 0.15,
            "R={radius}: {difference} % vs {width} %"
        );
        let measured_drop = grid.horizon_drop_px(20.0);
        assert!(
            (measured_drop - drop).abs() <= drop * 0.08,
            "R={radius}: drop {measured_drop} px vs {drop}"
        );
    }
}

#[test]
fn strip_and_cartesian_round_trip() {
    for (index, radius) in RADII.into_iter().enumerate() {
        let grid = PolarGrid::new(radius);
        let width = f64::from(grid.surface_columns());
        let mut rng = rng(0x9017 + index as u64);
        for _ in 0..2_000 {
            let u = unit(&mut rng) * width;
            let v = unit(&mut rng).mul_add(f64::from(grid.bottom_row() + 40), -40.0);
            let (x, y) = grid.to_cartesian(u, v);
            let (u2, v2) = grid.from_cartesian(x, y);
            // Compare on the circle (u near the seam may come back as u - width).
            let du = (u2 - u).rem_euclid(width);
            let du = du.min(width - du);
            assert!(du < 1e-9, "R={radius} u {u} -> {u2}");
            assert!((v2 - v).abs() < 1e-9, "R={radius} v {v} -> {v2}");
            // The distance from the centre is the ring's radius.
            assert!((x.hypot(y) - grid.radius_at(v)).abs() < 1e-9);
        }
    }
}

#[test]
fn tiles_and_offsets_round_trip_through_the_strip() {
    for (index, radius) in RADII.into_iter().enumerate() {
        let grid = PolarGrid::new(radius);
        let mut rng = rng(0x7113 + index as u64);
        for _ in 0..2_000 {
            #[allow(clippy::cast_possible_truncation, reason = "a test row")]
            let row = (unit(&mut rng) * f64::from(grid.bottom_row() + 1)) as i32;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a test column"
            )]
            let column =
                (unit(&mut rng) * f64::from(grid.columns_at(row))) as u32 % grid.columns_at(row);
            let tile = PolarTile { column, row };
            let (fx, fy) = (unit(&mut rng) * 0.999, unit(&mut rng) * 0.999);
            let (u, v) = grid.tile_point(tile, fx, fy);
            let (back, bx, by) = grid.point_tile(u, v);
            assert_eq!(back, tile, "R={radius}");
            assert!((bx - fx).abs() < 1e-9 && (by - fy).abs() < 1e-9);
            // And through the body-centred frame.
            let (x, y) = grid.to_cartesian(u, v);
            let (u2, v2) = grid.from_cartesian(x, y);
            let (again, _, _) = grid.point_tile(u2, v2);
            assert_eq!(again, tile, "R={radius} via Cartesian");
            // Every strip column the tile spans maps back to it.
            let first = grid.first_strip_column(tile);
            for cell in first..first + grid.span_at(row) {
                assert_eq!(grid.tile_of(i64::from(cell), row), tile);
            }
        }
    }
}

#[test]
fn columns_wrap_around_the_body() {
    let grid = PolarGrid::new(400);
    let width = i64::from(grid.surface_columns());
    for column in [
        -1,
        0,
        1,
        width - 1,
        width,
        width + 1,
        -width,
        7 * width + 13,
    ] {
        let wrapped = grid.wrap_column(column);
        assert!(i64::from(wrapped) < width);
        assert_eq!((column - i64::from(wrapped)).rem_euclid(width), 0);
        assert_eq!(
            grid.tile_of(column, 10),
            grid.tile_of(i64::from(wrapped), 10)
        );
    }
    // A lap of the strip is a full turn of the body.
    let (x0, y0) = grid.to_cartesian(0.25, 3.0);
    let (x1, y1) = grid.to_cartesian(0.25 + f64::from(grid.surface_columns()), 3.0);
    assert!((x0 - x1).abs() < 1e-9 && (y0 - y1).abs() < 1e-9);
    // Stepping right off the last column of any ring lands on its first.
    for row in [0, grid.bottom_row()] {
        let last = PolarTile {
            column: grid.columns_at(row) - 1,
            row,
        };
        assert_eq!(
            grid.neighbours(last, PolarStep::Right),
            vec![PolarTile { column: 0, row }]
        );
        assert_eq!(
            grid.neighbours(PolarTile { column: 0, row }, PolarStep::Left),
            vec![last]
        );
    }
}

#[test]
fn neighbours_across_a_halving_boundary_agree_both_ways() {
    for radius in [400, 1_500, 5_000] {
        let grid = PolarGrid::new(radius);
        for start in grid.halving_rows() {
            let above = start - 1;
            for column in 0..grid.columns_at(above) {
                let tile = PolarTile { column, row: above };
                let below = grid.neighbours(tile, PolarStep::Down);
                assert_eq!(below.len(), 1);
                assert_eq!(below[0].column, column / 2);
                // Stepping back up from the inner tile reaches this tile among its two.
                let up = grid.neighbours(below[0], PolarStep::Up);
                assert_eq!(up.len(), 2, "R={radius} boundary {start}");
                assert!(up.contains(&tile));
                // The inner tile spans exactly the strip columns of the two above it.
                let first = grid.first_strip_column(below[0]);
                let cells: Vec<u32> = (first..first + grid.span_at(*start)).collect();
                let mut expected = Vec::new();
                for outer in &up {
                    let first = grid.first_strip_column(*outer);
                    expected.extend(first..first + grid.span_at(above));
                }
                assert_eq!(cells, expected);
            }
        }
        // Inside a ring, up and down are one-to-one.
        let row = grid.halving_rows()[0] + 2;
        let tile = PolarTile { column: 3, row };
        assert_eq!(
            grid.neighbours(tile, PolarStep::Down),
            vec![PolarTile {
                column: 3,
                row: row + 1
            }]
        );
        assert_eq!(
            grid.neighbours(tile, PolarStep::Up),
            vec![PolarTile {
                column: 3,
                row: row - 1
            }]
        );
        // Nothing below the last row: the core.
        assert!(
            grid.neighbours(
                PolarTile {
                    column: 0,
                    row: grid.bottom_row()
                },
                PolarStep::Down
            )
            .is_empty()
        );
    }
}

#[test]
fn local_up_points_away_from_the_centre_everywhere() {
    let grid = PolarGrid::new(1_000);
    let width = f64::from(grid.surface_columns());
    for step in 0..64 {
        let u = width * f64::from(step) / 64.0;
        let frame = grid.local_frame(u);
        let (x, y) = grid.to_cartesian(u, 0.0);
        let r = x.hypot(y);
        assert!((frame.up.0 - x / r).abs() < 1e-12 && (frame.up.1 - y / r).abs() < 1e-12);
        assert!((frame.down.0 + frame.up.0).abs() < 1e-15);
        // The tangent is perpendicular to up and points toward increasing columns.
        assert!(
            frame
                .tangent
                .0
                .mul_add(frame.up.0, frame.tangent.1 * frame.up.1)
                .abs()
                < 1e-12
        );
        let (xn, yn) = grid.to_cartesian(u + 0.01, 0.0);
        assert!(frame.tangent.0.mul_add(xn - x, frame.tangent.1 * (yn - y)) > 0.0);
    }
    // The top of the body: up is -y (the screen's up), the tangent +x.
    let top = grid.local_frame(0.0);
    assert!((top.up.1 + 1.0).abs() < 1e-15 && top.tangent.0 > 0.999_999);
    assert!(top.angle.abs() < 1e-15);
    // A quarter turn round: up is +x.
    let side = grid.local_frame(width / 4.0);
    assert!((side.up.0 - 1.0).abs() < 1e-12);
}

#[test]
fn world_positions_in_space_round_trip_far_from_the_origin() {
    let grid = PolarGrid::new(2_345);
    for centre in [
        WorldPos::default(),
        WorldPos::new(1_000_000_000, -7, 100.5, 3.25),
        WorldPos::new(-3_000_000_000_000, 2_000_000_000_000, 16_000.0, 12.0),
    ] {
        let mut rng = rng(0xACE);
        for _ in 0..500 {
            let u = unit(&mut rng) * f64::from(grid.surface_columns());
            let v = unit(&mut rng).mul_add(f64::from(grid.bottom_row()), -100.0);
            let position = grid.to_world(centre, u, v);
            let (u2, v2) = grid.from_world(centre, position);
            let width = f64::from(grid.surface_columns());
            let du = (u2 - u).rem_euclid(width);
            // f32 offsets inside a sector resolve about 1/1000 px: the arc along the ring
            // (not the column count, which grows toward the core) is well under 1e-4 tiles.
            let arc = du.min(width - du) * TAU * grid.radius_at(v) / width;
            assert!(arc < 1e-4, "{centre:?} u {u} -> {u2}");
            assert!((v2 - v).abs() < 1e-4, "{centre:?} v {v} -> {v2}");
        }
    }
}

#[test]
fn geometry_is_a_pure_function_of_the_radius() {
    for radius in RADII {
        assert_eq!(PolarGrid::new(radius), PolarGrid::new(radius));
    }
    // Bit-identical conversions (the deterministic trig, no platform maths).
    let grid = PolarGrid::new(777);
    let (x, y) = grid.to_cartesian(1_234.567, 89.25);
    assert_eq!((x.to_bits(), y.to_bits()), {
        let (x, y) = PolarGrid::new(777).to_cartesian(1_234.567, 89.25);
        (x.to_bits(), y.to_bits())
    });
    // Clamped at the edges of the range.
    assert_eq!(PolarGrid::new(1).radius(), MIN_GRID_RADIUS);
    assert_eq!(PolarGrid::new(u32::MAX).radius(), MAX_RADIUS);
}
