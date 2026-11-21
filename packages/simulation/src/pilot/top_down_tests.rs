//! The top-down mode on hand-made grids: eight-way walking with no gravity, no wall clipping
//! at any speed or angle, corner sliding, the roll's distance and i-frames, relic boons and
//! tick-rate independence.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::while_float,
    reason = "test fixtures convert small tile, tick and pixel numbers freely"
)]

use std::collections::BTreeSet;

use super::*;
use crate::{
    ids::PlayerId,
    pilot::{PilotGear, step_pilot},
    terrain::{TILE_SIZE, TilePosition},
};

/// `#` solid, anything else open; outside the grid is solid.
struct Grid {
    solid: BTreeSet<(i32, i32)>,
    width: i32,
    height: i32,
}

impl Grid {
    fn new(rows: &[&str]) -> Self {
        let mut solid = BTreeSet::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, cell) in row.chars().enumerate() {
                if cell == '#' {
                    solid.insert((x as i32, y as i32));
                }
            }
        }
        Self {
            solid,
            width: rows.iter().map(|row| row.len()).max().unwrap_or(0) as i32,
            height: rows.len() as i32,
        }
    }
}

impl PilotTerrain for Grid {
    fn blocks(&self, tile: TilePosition) -> bool {
        tile.x < 0
            || tile.y < 0
            || tile.x >= self.width
            || tile.y >= self.height
            || self.solid.contains(&(tile.x, tile.y))
    }

    fn climbable(&self, _: TilePosition) -> bool {
        false
    }

    fn water(&self, _: TilePosition) -> bool {
        false
    }

    fn platform_top(&self, _: f32, _: f32, _: f32, _: f32) -> Option<f32> {
        None
    }
}

const ROOM: [&str; 9] = [
    "###############",
    "#.............#",
    "#.............#",
    "#....#........#",
    "#.............#",
    "#.............#",
    "#.........##..#",
    "#.............#",
    "###############",
];

/// A top-down pilot with its feet at the bottom of tile `(tx, ty)`, centred.
fn pilot_at(tx: i32, ty: i32) -> (Body, PilotState) {
    let mut state = PilotState::new(PlayerId::new(1), PilotGear::default(), 1);
    state.view = PilotView::TopDown;
    let x = (tx as f32 + 0.5) * TILE_SIZE;
    let feet = (ty as f32 + 1.0).mul_add(TILE_SIZE, -3.0);
    (Body::at(x, feet), state)
}

fn controls(horizontal: f32, up: bool, down: bool, jump: bool) -> PilotControls {
    PilotControls {
        horizontal,
        up,
        down,
        jump,
        anchor: None,
    }
}

fn run(
    grid: &Grid,
    body: &mut Body,
    state: &mut PilotState,
    held: PilotControls,
    ticks: u32,
    hz: u32,
) -> PilotStepReport {
    let mut any = PilotStepReport::default();
    for _ in 0..ticks {
        let report = step_pilot(body, state, grid, held, 1.0 / hz as f32);
        any.rolled |= report.rolled;
    }
    any
}

fn assert_clear(grid: &Grid, body: &Body) {
    assert!(
        !footprint_blocked(grid, body.x, body.y),
        "footprint inside a wall at ({}, {})",
        body.x,
        body.y
    );
}

#[test]
fn walks_eight_ways_without_gravity() {
    let grid = Grid::new(&ROOM);
    let (mut body, mut state) = pilot_at(7, 4);
    let start = (body.x, body.y);
    // Standing still: no fall.
    run(
        &grid,
        &mut body,
        &mut state,
        controls(0.0, false, false, false),
        60,
        60,
    );
    assert_eq!((body.x, body.y), start);
    // North.
    run(
        &grid,
        &mut body,
        &mut state,
        controls(0.0, true, false, false),
        20,
        60,
    );
    assert!(body.y < start.1 - 40.0 && (body.x - start.0).abs() < 0.01);
    // A diagonal is no faster than a straight walk.
    let (mut diagonal, mut diagonal_state) = pilot_at(7, 4);
    run(
        &grid,
        &mut diagonal,
        &mut diagonal_state,
        controls(1.0, false, true, false),
        12,
        60,
    );
    let moved = (diagonal.x - start.0).hypot(diagonal.y - start.1);
    assert!(moved <= TOP_DOWN_SPEED.mul_add(0.2, 0.5), "{moved}");
    assert!(diagonal.x > start.0 && diagonal.y > start.1);
    assert!(diagonal_state.heading_x > 0 && diagonal_state.heading_y > 0);
}

#[test]
fn never_clips_into_walls_at_any_angle_or_speed() {
    let grid = Grid::new(&ROOM);
    for (horizontal, up, down) in [
        (1.0, false, false),
        (-1.0, false, false),
        (0.0, true, false),
        (0.0, false, true),
        (1.0, true, false),
        (-1.0, false, true),
        (0.6, false, true),
        (-0.4, true, false),
    ] {
        for roll in [false, true] {
            let (mut body, mut state) = pilot_at(7, 4);
            for tick in 0..240 {
                let jump = roll && tick % 30 == 0;
                let _ = step_pilot(
                    &mut body,
                    &mut state,
                    &grid,
                    controls(horizontal, up, down, jump),
                    1.0 / 60.0,
                );
                assert_clear(&grid, &body);
            }
            // It ended against a wall, not past it.
            assert!(body.x > TILE_SIZE && body.x < 14.0f32.mul_add(TILE_SIZE, 0.5));
            assert!(body.y > TILE_SIZE && body.y <= 8.0f32.mul_add(TILE_SIZE, 0.5));
        }
    }
}

#[test]
fn walking_diagonally_into_a_wall_slides_along_it() {
    let grid = Grid::new(&ROOM);
    let (mut body, mut state) = pilot_at(2, 1);
    let start = body.x;
    // Up and right: blocked north at once, keeps sliding east.
    run(
        &grid,
        &mut body,
        &mut state,
        controls(1.0, true, false, false),
        45,
        60,
    );
    assert!(body.x > 2.5f32.mul_add(TILE_SIZE, start), "{}", body.x);
    assert_clear(&grid, &body);
}

#[test]
fn walking_straight_at_a_clipped_corner_nudges_round_it() {
    // A pillar at (5, 3); walk east along row 2 with the footprint dipping 6 px into row 3.
    let grid = Grid::new(&ROOM);
    let (mut body, mut state) = pilot_at(2, 2);
    body.y = 3.0f32.mul_add(TILE_SIZE, 6.0);
    run(
        &grid,
        &mut body,
        &mut state,
        controls(1.0, false, false, false),
        60,
        60,
    );
    assert!(
        body.x > 7.0 * TILE_SIZE,
        "stuck on the corner at {}",
        body.x
    );
    assert!(
        body.y <= 3.0f32.mul_add(TILE_SIZE, 0.01),
        "nudged up: {}",
        body.y
    );
    assert_clear(&grid, &body);
    // Squarely blocked (the footprint well inside the pillar's rows): no nudge.
    let (mut body, mut state) = pilot_at(2, 3);
    run(
        &grid,
        &mut body,
        &mut state,
        controls(1.0, false, false, false),
        60,
        60,
    );
    assert!(body.x < 5.0 * TILE_SIZE, "{}", body.x);
}

#[test]
fn a_roll_dashes_with_i_frames_then_cools_down() {
    let grid = Grid::new(&ROOM);
    let (mut body, mut state) = pilot_at(2, 4);
    let start = body.x;
    let report = run(
        &grid,
        &mut body,
        &mut state,
        controls(1.0, false, false, true),
        1,
        60,
    );
    assert!(report.rolled);
    assert!(state.roll > 0.0 && state.roll_guarded());
    run(
        &grid,
        &mut body,
        &mut state,
        controls(1.0, false, false, true),
        30,
        60,
    );
    assert_eq!(state.roll, 0.0);
    assert!(!state.roll_guarded());
    let rolled = body.x - start;
    // A roll covers more than four tiles plus a little walking afterwards.
    assert!(rolled > 4.0 * TILE_SIZE, "{rolled}");
    // Holding the button does not roll again; a fresh press inside the cooldown does not
    // either.
    let report = run(
        &grid,
        &mut body,
        &mut state,
        controls(-1.0, false, false, true),
        2,
        60,
    );
    assert!(!report.rolled);
    let report = run(
        &grid,
        &mut body,
        &mut state,
        controls(-1.0, false, false, false),
        1,
        60,
    );
    assert!(!report.rolled);
    // After the cooldown: a roll west.
    run(
        &grid,
        &mut body,
        &mut state,
        controls(0.0, false, false, false),
        40,
        60,
    );
    let report = run(
        &grid,
        &mut body,
        &mut state,
        controls(0.0, false, false, true),
        1,
        60,
    );
    assert!(report.rolled);
    assert!(
        state.roll_x < 0,
        "rolls along the heading: {}",
        state.roll_x
    );
}

#[test]
fn the_rolls_last_moment_is_not_guarded() {
    let grid = Grid::new(&ROOM);
    let (mut body, mut state) = pilot_at(2, 4);
    let _ = step_pilot(
        &mut body,
        &mut state,
        &grid,
        controls(1.0, false, false, true),
        1.0 / 240.0,
    );
    let mut guarded = 0;
    let mut open = 0;
    while state.roll > 0.0 {
        if state.roll_guarded() {
            guarded += 1;
        } else {
            open += 1;
        }
        let _ = step_pilot(
            &mut body,
            &mut state,
            &grid,
            controls(1.0, false, false, true),
            1.0 / 240.0,
        );
    }
    assert!(guarded > 50, "{guarded}");
    assert!((10..=20).contains(&open), "{open}");
}

#[test]
fn relics_lengthen_and_speed_up_the_roll() {
    let grid = Grid::new(&[
        "#".repeat(40).as_str(),
        &format!("#{}#", ".".repeat(38)),
        "#".repeat(40).as_str(),
    ]);
    let distance = |extra_jumps: u8, glide_pct: i16| {
        let (mut body, mut state) = pilot_at(2, 1);
        state.boons.extra_jumps = extra_jumps;
        state.boons.glide_pct = glide_pct;
        let start = body.x;
        let _ = step_pilot(
            &mut body,
            &mut state,
            &grid,
            controls(1.0, false, false, true),
            1.0 / 60.0,
        );
        while state.roll > 0.0 {
            let _ = step_pilot(
                &mut body,
                &mut state,
                &grid,
                controls(0.0, false, false, true),
                1.0 / 60.0,
            );
        }
        body.x - start
    };
    let plain = distance(0, 0);
    assert!(distance(1, 0) > plain * 1.2, "Spring Heels rolls longer");
    assert!(distance(0, 30) > plain * 1.2, "Glider Boost dashes further");
}

#[test]
fn the_mode_is_tick_rate_safe() {
    let grid = Grid::new(&ROOM);
    let trace = |hz: u32| {
        let (mut body, mut state) = pilot_at(3, 3);
        let seconds = 3;
        for tick in 0..hz * seconds {
            let t = tick as f32 / hz as f32;
            let held = controls(
                if t < 1.5 { 1.0 } else { -0.7 },
                (0.8..1.9).contains(&t),
                t >= 2.2,
                (0.5..0.6).contains(&t) || (2.4..2.5).contains(&t),
            );
            let _ = step_pilot(&mut body, &mut state, &grid, held, 1.0 / hz as f32);
        }
        (body.x, body.y, state.roll_cooldown)
    };
    let base = trace(60);
    for hz in [30, 120] {
        let other = trace(hz);
        assert!(
            (other.0 - base.0).abs() < 0.5,
            "{hz} Hz: {other:?} vs {base:?}"
        );
        assert!(
            (other.1 - base.1).abs() < 0.5,
            "{hz} Hz: {other:?} vs {base:?}"
        );
    }
}
