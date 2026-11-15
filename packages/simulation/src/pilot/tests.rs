//! The pilot body on hand-made grids: walking, running, jumping (coyote time, buffering,
//! variable height), gaps, crawlways, ladders, ledges, swimming, the jetpack, fall damage and
//! tick-rate independence.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    clippy::while_float,
    reason = "test fixtures convert small tile, tick and pixel numbers freely"
)]

use std::collections::BTreeSet;

use super::*;

/// A tile grid: `#` solid, `H` ladder, `~` water, `.` open; row 0 is the top line. Outside the
/// grid is solid.
struct Grid {
    solid: BTreeSet<(i32, i32)>,
    ladders: BTreeSet<(i32, i32)>,
    water: BTreeSet<(i32, i32)>,
    width: i32,
    height: i32,
}

impl Grid {
    fn new(rows: &[&str]) -> Self {
        let mut grid = Self {
            solid: BTreeSet::new(),
            ladders: BTreeSet::new(),
            water: BTreeSet::new(),
            width: rows.iter().map(|row| row.len()).max().unwrap_or(0) as i32,
            height: rows.len() as i32,
        };
        for (y, row) in rows.iter().enumerate() {
            for (x, cell) in row.chars().enumerate() {
                let at = (x as i32, y as i32);
                match cell {
                    '#' => {
                        grid.solid.insert(at);
                    }
                    'H' => {
                        grid.ladders.insert(at);
                    }
                    '~' => {
                        grid.water.insert(at);
                    }
                    _ => {}
                }
            }
        }
        grid
    }
}

impl PilotTerrain for Grid {
    fn blocks(&self, tile: TilePosition) -> bool {
        tile.x < 0
            || tile.x >= self.width
            || tile.y >= self.height
            || (tile.y >= 0 && self.solid.contains(&(tile.x, tile.y)))
    }

    fn climbable(&self, tile: TilePosition) -> bool {
        self.ladders.contains(&(tile.x, tile.y))
    }

    fn water(&self, tile: TilePosition) -> bool {
        self.water.contains(&(tile.x, tile.y))
    }

    fn platform_top(&self, _: f32, _: f32, _: f32, _: f32) -> Option<f32> {
        None
    }
}

fn pilot_on(tile_x: f32, floor_row: i32) -> (Body, PilotState) {
    let body = Body::at(tile_x * TILE_SIZE, floor_row as f32 * TILE_SIZE - FLUSH);
    let mut state = PilotState::new(PlayerId::new(1), PilotGear::default(), 1);
    state.grounded = true;
    (body, state)
}

fn hold(horizontal: f32) -> PilotControls {
    PilotControls {
        horizontal,
        ..PilotControls::default()
    }
}

fn run_for(
    grid: &Grid,
    body: &mut Body,
    state: &mut PilotState,
    controls: PilotControls,
    seconds: f32,
    hz: u32,
) -> PilotStepReport {
    let dt = 1.0 / hz as f32;
    let ticks = (seconds * hz as f32).round() as u32;
    let mut total = PilotStepReport::default();
    for _ in 0..ticks {
        let report = step_pilot(body, state, grid, controls, dt);
        total.jumped |= report.jumped;
        total.landed |= report.landed;
        total.grabbed_ledge |= report.grabbed_ledge;
        total.climbed_ledge |= report.climbed_ledge;
        total.jet_started |= report.jet_started;
        total.jet_empty |= report.jet_empty;
        total.splashed |= report.splashed;
        total.landing_speed = total.landing_speed.max(report.landing_speed);
    }
    total
}

const FLOOR: &[&str] = &[
    "..............................",
    "..............................",
    "..............................",
    "..............................",
    "..............................",
    "..............................",
    "##############################",
];

#[test]
fn walks_then_breaks_into_a_run() {
    let grid = Grid::new(FLOOR);
    let (mut body, mut state) = pilot_on(3.0, 6);
    run_for(&grid, &mut body, &mut state, hold(1.0), 0.3, 60);
    assert!(
        (body.velocity_x - WALK_SPEED).abs() < 1.0,
        "walking: {}",
        body.velocity_x
    );
    run_for(&grid, &mut body, &mut state, hold(1.0), 0.6, 60);
    assert!(
        (body.velocity_x - RUN_SPEED).abs() < 1.0,
        "running: {}",
        body.velocity_x
    );
    assert!(state.grounded);
    assert_eq!(state.pose(&body), PilotPose::Run);
    run_for(&grid, &mut body, &mut state, hold(0.0), 0.3, 60);
    assert!(body.velocity_x.abs() < f32::EPSILON, "stops quickly");
    assert_eq!(state.pose(&body), PilotPose::Idle);
}

#[test]
fn jumps_about_two_and_a_half_tiles_and_lands() {
    let grid = Grid::new(FLOOR);
    let (mut body, mut state) = pilot_on(5.0, 6);
    let start = body.y;
    let jump = PilotControls {
        jump: true,
        ..PilotControls::default()
    };
    let mut highest = start;
    let mut jumped = false;
    for _ in 0..90 {
        let report = step_pilot(&mut body, &mut state, &grid, jump, 1.0 / 60.0);
        jumped |= report.jumped;
        highest = highest.min(body.y);
    }
    assert!(jumped);
    let height = (start - highest) / TILE_SIZE;
    assert!((2.2..=2.7).contains(&height), "jump height {height}");
    assert!(state.grounded, "back on the floor");
    assert!((body.y - start).abs() < 0.5);
}

#[test]
fn releasing_jump_early_makes_a_short_hop() {
    let grid = Grid::new(FLOOR);
    let (mut body, mut state) = pilot_on(5.0, 6);
    let start = body.y;
    let jump = PilotControls {
        jump: true,
        ..PilotControls::default()
    };
    step_pilot(&mut body, &mut state, &grid, jump, 1.0 / 60.0);
    step_pilot(&mut body, &mut state, &grid, jump, 1.0 / 60.0);
    let mut highest = start;
    for _ in 0..60 {
        step_pilot(&mut body, &mut state, &grid, hold(0.0), 1.0 / 60.0);
        highest = highest.min(body.y);
    }
    assert!((start - highest) / TILE_SIZE < 1.0, "a tap is a hop");
}

#[test]
fn coyote_time_allows_a_jump_just_after_the_ledge() {
    let grid = Grid::new(&[
        "..............................",
        "..............................",
        "..............................",
        "..............................",
        "..............................",
        "..............................",
        "##########....................",
        "##########....................",
        "##############################",
    ]);
    let (mut body, mut state) = pilot_on(8.0, 6);
    // Walk off the edge at column 10.
    while body.x - PILOT_HALF_WIDTH < 10.0 * TILE_SIZE + 2.0 {
        step_pilot(&mut body, &mut state, &grid, hold(1.0), 1.0 / 240.0);
    }
    assert!(!state.grounded, "past the edge");
    let report = step_pilot(
        &mut body,
        &mut state,
        &grid,
        PilotControls {
            horizontal: 1.0,
            jump: true,
            ..PilotControls::default()
        },
        1.0 / 60.0,
    );
    assert!(report.jumped, "coyote jump");
}

#[test]
fn a_buffered_jump_fires_on_landing() {
    let grid = Grid::new(FLOOR);
    let (mut body, mut state) = pilot_on(5.0, 6);
    body.y -= TILE_SIZE * 0.6;
    state.grounded = false;
    body.velocity_y = 300.0;
    let jump = PilotControls {
        jump: true,
        ..PilotControls::default()
    };
    let mut jumped = false;
    for _ in 0..6 {
        jumped |= step_pilot(&mut body, &mut state, &grid, jump, 1.0 / 60.0).jumped;
    }
    assert!(jumped, "pressed just before landing");
}

#[test]
fn slips_through_a_one_tile_shaft() {
    let grid = Grid::new(&[
        "..........",
        "..........",
        "..........",
        "####.#####",
        "####.#####",
        "####.#####",
        "####.#####",
        "####.#####",
        "##########",
    ]);
    let (mut body, mut state) = pilot_on(4.5, 3);
    body.y -= 2.0;
    run_for(&grid, &mut body, &mut state, hold(0.0), 1.5, 60);
    assert!(state.grounded);
    assert!(
        (body.y - (8.0 * TILE_SIZE - FLUSH)).abs() < 0.5,
        "at the bottom of the shaft: {}",
        body.y
    );
}

#[test]
fn crawls_through_a_one_tile_high_crawlway() {
    let grid = Grid::new(&[
        "....................",
        "....................",
        "....#########.......",
        "....................",
        "####################",
    ]);
    let (mut body, mut state) = pilot_on(1.5, 4);
    let stand = hold(1.0);
    run_for(&grid, &mut body, &mut state, stand, 1.0, 60);
    assert!(
        body.x < 4.0 * TILE_SIZE,
        "standing, the crawlway stops the pilot: {}",
        body.x / TILE_SIZE
    );
    let crawl = PilotControls {
        horizontal: 1.0,
        down: true,
        ..PilotControls::default()
    };
    run_for(&grid, &mut body, &mut state, crawl, 5.0, 60);
    assert!(
        body.x > 13.5 * TILE_SIZE,
        "crawled through: {}",
        body.x / TILE_SIZE
    );
    assert!(state.crouching);
    run_for(&grid, &mut body, &mut state, hold(0.0), 0.2, 60);
    assert!(!state.crouching, "stands up in the open");
}

#[test]
fn stays_crouched_under_a_low_ceiling() {
    let grid = Grid::new(&["..........", "##########", "..........", "##########"]);
    let (mut body, mut state) = pilot_on(4.5, 3);
    state.crouching = true;
    run_for(&grid, &mut body, &mut state, hold(0.0), 0.2, 60);
    assert!(state.crouching, "no room to stand");
    assert_eq!(state.pose(&body), PilotPose::Crouch);
}

#[test]
fn climbs_a_ladder_and_steps_off_the_top() {
    let grid = Grid::new(&[
        "..........",
        "..........",
        "....H#####",
        "....H#####",
        "....H#####",
        "....H.....",
        "##########",
    ]);
    let (mut body, mut state) = pilot_on(4.5, 6);
    let up = PilotControls {
        up: true,
        ..PilotControls::default()
    };
    run_for(&grid, &mut body, &mut state, up, 0.3, 60);
    assert_eq!(state.mode, PilotMode::Climb);
    assert_eq!(state.pose(&body), PilotPose::Climb);
    run_for(&grid, &mut body, &mut state, up, 1.6, 60);
    run_for(
        &grid,
        &mut body,
        &mut state,
        PilotControls {
            horizontal: 1.0,
            up: true,
            ..PilotControls::default()
        },
        0.6,
        60,
    );
    run_for(&grid, &mut body, &mut state, hold(1.0), 0.5, 60);
    assert!(
        (body.y - (2.0 * TILE_SIZE - FLUSH)).abs() < 1.0,
        "standing on the top: {}",
        body.y / TILE_SIZE
    );
    assert!(body.x > 5.0 * TILE_SIZE);
}

#[test]
fn grabs_a_ledge_and_pulls_up() {
    let grid = Grid::new(&[
        "............",
        "............",
        "............",
        "......######",
        "......######",
        "......######",
        "############",
    ]);
    let (mut body, mut state) = pilot_on(4.0, 6);
    // Jump toward the three-tile wall: too high to clear, a ledge to catch.
    let report = run_for(
        &grid,
        &mut body,
        &mut state,
        PilotControls {
            horizontal: 1.0,
            jump: true,
            ..PilotControls::default()
        },
        0.8,
        60,
    );
    assert!(report.jumped);
    assert!(report.grabbed_ledge, "caught the ledge");
    // Let go of jump, then press it again to pull up.
    run_for(&grid, &mut body, &mut state, hold(1.0), 0.1, 60);
    assert_eq!(state.mode, PilotMode::Ledge);
    assert_eq!(state.pose(&body), PilotPose::Hang);
    let report = run_for(
        &grid,
        &mut body,
        &mut state,
        PilotControls {
            horizontal: 1.0,
            jump: true,
            ..PilotControls::default()
        },
        0.4,
        60,
    );
    assert!(report.climbed_ledge);
    assert!(
        (body.y - (3.0 * TILE_SIZE - FLUSH)).abs() < 1.0,
        "on top: {}",
        body.y / TILE_SIZE
    );
    assert!(body.x > 6.0 * TILE_SIZE);
}

#[test]
fn falls_hurt_only_past_five_tiles() {
    let mut rows = vec![".........."; 20];
    rows.push("##########");
    let grid = Grid::new(&rows);
    for (drop_tiles, hurts) in [(4.0, false), (10.0, true)] {
        let (mut body, mut state) = pilot_on(5.0, 20);
        body.y -= drop_tiles * TILE_SIZE;
        state.grounded = false;
        let report = run_for(&grid, &mut body, &mut state, hold(0.0), 2.0, 60);
        assert!(report.landed);
        assert_eq!(
            fall_damage(report.landing_speed) > 0.0,
            hurts,
            "{drop_tiles} tiles: {}",
            report.landing_speed
        );
    }
    assert!(
        fall_damage(MAX_FALL_SPEED) >= BASE_HEALTH,
        "a long fall kills"
    );
}

#[test]
fn swims_up_and_climbs_out_of_water() {
    let grid = Grid::new(&[
        "..........",
        "..........",
        "..........",
        "#~~~~~~###",
        "#~~~~~~###",
        "#~~~~~~###",
        "##########",
    ]);
    let (mut body, mut state) = pilot_on(3.0, 3);
    body.y += TILE_SIZE * 2.5;
    state.grounded = false;
    let report = run_for(&grid, &mut body, &mut state, hold(0.0), 0.2, 60);
    assert!(report.splashed);
    assert_eq!(state.mode, PilotMode::Swim);
    assert_eq!(state.pose(&body), PilotPose::Swim);
    let up = PilotControls {
        horizontal: 1.0,
        jump: true,
        ..PilotControls::default()
    };
    run_for(&grid, &mut body, &mut state, up, 3.0, 60);
    assert!(
        body.x > 7.0 * TILE_SIZE,
        "on the bank: {}",
        body.x / TILE_SIZE
    );
    assert!(
        state.grounded,
        "standing on the bank: {}",
        body.y / TILE_SIZE
    );
    assert_ne!(state.mode, PilotMode::Swim);
}

#[test]
fn the_jetpack_burns_a_few_seconds_and_recharges_only_on_the_ground() {
    let mut rows = vec!["......"; 40];
    rows.push("######");
    let grid = Grid::new(&rows);
    let (mut body, _) = pilot_on(3.0, 40);
    let gear = PilotGear {
        jetpack: 1,
        suit: 0,
    };
    let mut state = PilotState {
        grounded: true,
        ..PilotState::new(PlayerId::new(1), gear, 1)
    };
    let start = body.y;
    let jump = PilotControls {
        jump: true,
        ..PilotControls::default()
    };
    let mut highest = start;
    let mut report = PilotStepReport::default();
    for _ in 0..114 {
        let step = step_pilot(&mut body, &mut state, &grid, jump, 1.0 / 60.0);
        report.jet_started |= step.jet_started;
        report.jet_empty |= step.jet_empty;
        highest = highest.min(body.y);
    }
    assert!(report.jet_started && report.jet_empty);
    assert!(state.jetpack_fuel <= 0.0);
    assert!(!state.grounded, "still coming down");
    assert!(
        (start - highest) / TILE_SIZE > 6.0,
        "flew well above a jump: {}",
        (start - highest) / TILE_SIZE
    );
    // No recharge in the air.
    run_for(&grid, &mut body, &mut state, hold(0.0), 0.3, 60);
    assert!(state.jetpack_fuel <= 0.0);
    run_for(&grid, &mut body, &mut state, hold(0.0), 4.0, 60);
    assert!(state.grounded);
    assert!(
        (state.jetpack_fuel - gear.jetpack_seconds()).abs() < 0.01,
        "full again"
    );
    // Without a jetpack, holding jump is just a jump.
    let mut plain = PilotState {
        grounded: true,
        ..PilotState::new(PlayerId::new(1), PilotGear::default(), 1)
    };
    let mut plain_body = Body::at(3.0 * TILE_SIZE, 40.0 * TILE_SIZE - FLUSH);
    let report = run_for(&grid, &mut plain_body, &mut plain, jump, 1.0, 60);
    assert!(!report.jet_started);
}

/// The same held input from the same start traces the same path at 30, 60, 120 and 240 Hz.
#[test]
fn movement_is_tick_rate_independent() {
    let grid = Grid::new(&[
        "........................................",
        "........................................",
        "........................................",
        "..............###.......................",
        "..............###.......................",
        "........######...#######................",
        "########################################",
    ]);
    let script = [
        (0.5, hold(1.0)),
        (
            0.3,
            PilotControls {
                horizontal: 1.0,
                jump: true,
                ..PilotControls::default()
            },
        ),
        (0.9, hold(1.0)),
        (0.4, hold(-0.6)),
        (0.6, hold(0.0)),
    ];
    let mut results = Vec::new();
    for hz in [30, 60, 120, 240] {
        let (mut body, mut state) = pilot_on(2.0, 6);
        for (seconds, controls) in script {
            run_for(&grid, &mut body, &mut state, controls, seconds, hz);
        }
        results.push((hz, body.x, body.y, state.mode, state.grounded));
    }
    let (_, x, y, mode, grounded) = results[0];
    for (hz, other_x, other_y, other_mode, other_grounded) in &results[1..] {
        assert!(
            (x - other_x).abs() < 0.01 && (y - other_y).abs() < 0.01,
            "{hz} Hz drifted: ({other_x}, {other_y}) vs ({x}, {y})"
        );
        assert_eq!(mode, *other_mode);
        assert_eq!(grounded, *other_grounded);
    }
}

#[test]
fn pick_breaks_only_soft_ground() {
    assert!(pick_seconds(TileKind::Dirt).is_some());
    assert!(pick_seconds(TileKind::Clay).unwrap() > pick_seconds(TileKind::Dirt).unwrap());
    assert!(pick_seconds(TileKind::Surface(SurfaceMaterial::Sand)).is_some());
    assert!(pick_seconds(TileKind::Stone).is_none());
    assert!(pick_seconds(TileKind::HardRock).is_none());
    assert!(pick_seconds(TileKind::Bedrock).is_none());
    assert!(pilot_passable(TileKind::Built(BuiltTile::VentGrate)));
    assert!(!pilot_passable(TileKind::Built(BuiltTile::Plank)));
}

#[test]
fn gear_tiers_raise_health_air_and_pack() {
    let bare = PilotGear::default();
    let suited = PilotGear {
        jetpack: 2,
        suit: 2,
    };
    assert!(suited.max_health() > bare.max_health());
    assert!(suited.max_oxygen() > bare.max_oxygen());
    assert!(suited.pack_capacity() > bare.pack_capacity());
    assert!(bare.jetpack_seconds() <= 0.0);
    assert!((suited.jetpack_seconds() - 3.0).abs() < f32::EPSILON);
    assert_eq!(PilotGearKind::Jetpack.price(MAX_JETPACK_TIER), None);
    assert!(PilotGearKind::Suit.price(0).is_some());
}
