//! The Wave 14 palette, launch flags, debug views and time scale through `LocalGame::frame`.

use fatherload_simulation::{
    SimulationParams,
    descent::Descent,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::*;
use crate::{
    console::{parse::TpTarget, start_at::DEFAULT_TEST_SEED},
    debug_views::{DebugView, TimeScale},
};

fn deep_world(test_mode: bool) -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(DEFAULT_TEST_SEED)
        },
        SimulationParams::default(),
    );
    if test_mode {
        world.enable_test_mode();
    }
    world
}

fn backtick() -> PlayerInput {
    PlayerInput {
        console: true,
        ..PlayerInput::default()
    }
}

/// Types `line` into the open palette and presses Enter.
fn type_line(game: &mut LocalGame, line: &str) {
    for character in line.chars() {
        let _ = step(
            game,
            PlayerInput {
                text_input: Some(character),
                ..PlayerInput::default()
            },
        );
    }
    let _ = step(game, confirm_input());
    idle(game, 3);
}

fn row(game: &LocalGame) -> i32 {
    let world = game.world();
    world.player(PLAYER).expect("rig").tile_in(world.frame()).y
}

#[test]
fn the_palette_opens_only_in_a_test_mode_world() {
    let mut normal = playing(deep_world(false), 1, "console-normal");
    let _ = press(&mut normal, backtick());
    assert!(!normal.shell.console.open, "never in a normal world");
    assert!(!normal.shell.text_entry());

    let mut game = playing(deep_world(true), 1, "console-test");
    let _ = press(&mut game, backtick());
    assert!(game.shell.console.open);
    assert!(game.shell.text_entry(), "hotkeys rest while typing");
    assert!(
        !game.shell.gameplay_input_enabled(0),
        "the rig stands still while the palette has its keys"
    );
    let _ = press(&mut game, backtick());
    assert!(!game.shell.console.open);
    assert!(game.shell.gameplay_input_enabled(0));
    // The pause menu opens it too (a phone whose top row has no room for CMD), in test mode only.
    assert!(PauseOption::for_session(None, true).contains(&PauseOption::CommandPalette));
    assert!(!PauseOption::for_session(None, false).contains(&PauseOption::CommandPalette));
}

#[test]
fn typed_travel_moves_the_rig_through_the_test_mode_action() {
    let mut game = playing(deep_world(true), 1, "console-travel");
    let start = row(&game);
    let _ = press(&mut game, backtick());
    type_line(&mut game, "tp descent 3");
    let layout = game
        .world()
        .descent_strata_layout()
        .expect("a Deep Descent world");
    let (top, bottom) = layout.descent_rows(Descent::LivingDark);
    let now = row(&game);
    assert!((top..=bottom).contains(&now), "row {now} (from {start})");
    // `tp back` returns to where the rig was.
    type_line(&mut game, "tp back");
    assert!(
        (row(&game) - start).abs() <= 2,
        "back at {start}: {}",
        row(&game)
    );
    // A preview moves nothing; Enter on an empty line goes.
    type_line(&mut game, "tp descent 2 ?");
    assert!((row(&game) - start).abs() <= 2);
    let _ = step(&mut game, confirm_input());
    idle(&mut game, 3);
    let (top, bottom) = layout.descent_rows(Descent::OldDeep);
    assert!((top..=bottom).contains(&row(&game)));
}

#[test]
fn debug_views_switch_from_the_palette_and_never_show_outside_test_mode() {
    let mut game = playing(deep_world(true), 1, "console-views");
    for view in DebugView::ALL {
        assert!(
            !game.shell.debug_views.is_on(view),
            "{view:?} off by default"
        );
    }
    let _ = press(&mut game, backtick());
    type_line(&mut game, "view hitboxes");
    type_line(&mut game, "view fps on");
    assert!(game.shell.debug_views.shows(DebugView::HitBoxes, true));
    assert!(game.shell.debug_views.shows(DebugView::FrameStats, true));
    assert!(!game.shell.debug_views.shows(DebugView::HitBoxes, false));
    type_line(&mut game, "view off");
    for view in DebugView::ALL {
        assert!(!game.shell.debug_views.is_on(view));
    }
    // The free camera pans with the stick and leaves the rig where it is.
    type_line(&mut game, "view freecam");
    let _ = press(&mut game, backtick());
    let before = game.world().player(PLAYER).expect("rig").x;
    let screen = (SCREEN_WIDTH, SCREEN_HEIGHT);
    let camera_before = game.snapshot(screen).views[0].camera;
    for _ in 0..30 {
        let _ = step(
            &mut game,
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
        );
    }
    let after = game.world().player(PLAYER).expect("rig").x;
    assert!((after - before).abs() < 1.0, "the rig stayed put");
    let camera_after = game.snapshot(screen).views[0].camera;
    assert!(
        camera_after.x > camera_before.x + 100.0,
        "the camera panned"
    );
}

#[test]
fn time_scale_runs_whole_ticks_and_matches_a_normal_run() {
    let run = |scale: TimeScale, frames: usize| {
        let mut game = playing(deep_world(true), 1, "console-speed");
        game.shell.time_scale = scale;
        let mut ticks = 0;
        for _ in 0..frames {
            ticks += step(&mut game, PlayerInput::default()).ticks_run;
        }
        (
            ticks,
            game.world().simulation_tick(),
            game.world().simulation_state_hash(),
        )
    };
    let normal = run(TimeScale::Normal, 120);
    let double = run(TimeScale::Double, 60);
    let quad = run(TimeScale::Quadruple, 30);
    assert_eq!(normal, double, "x2 in half the frames is the same run");
    assert_eq!(normal, quad, "x4 in a quarter of the frames too");
    let (paused_ticks, ..) = run(TimeScale::Paused, 60);
    assert_eq!(paused_ticks, 0);
    let (half_ticks, ..) = run(TimeScale::Half, 120);
    assert_eq!(half_ticks * 2, normal.0);
    // A normal world ignores the scale.
    let mut game = playing(deep_world(false), 1, "console-speed-normal");
    game.shell.time_scale = TimeScale::Paused;
    let ticks: u32 = (0..60)
        .map(|_| step(&mut game, PlayerInput::default()).ticks_run)
        .sum();
    assert!(ticks > 0);
}

#[test]
fn start_at_launches_a_test_world_in_place() {
    let mut game = title_game("console-start-at");
    let _ = game.start_test_world(
        DEFAULT_TEST_SEED,
        Some(&TpTarget::Descent(Descent::HollowWorld)),
    );
    assert!(game.world().is_test_mode());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    idle(&mut game, 3);
    let layout = game
        .world()
        .descent_strata_layout()
        .expect("a Deep Descent world");
    let (top, bottom) = layout.descent_rows(Descent::HollowWorld);
    assert!((top..=bottom).contains(&row(&game)), "row {}", row(&game));
}

#[test]
fn start_at_reaches_a_random_ice_planet_and_a_gate_boss() {
    use fatherload_simulation::{
        place::{BodyId, Place},
        space::{Space, Theme},
    };

    let mut game = title_game("console-start-at-planet");
    let target = crate::console::parse::parse_start_at("planet:random:ice").expect("parses");
    let _ = game.start_test_world(DEFAULT_TEST_SEED, Some(&target));
    idle(&mut game, 5);
    let Place::OnBody(BodyId::Space(key)) = game.world().place_of(PLAYER) else {
        panic!("on a planet: {:?}", game.world().place_of(PLAYER));
    };
    let theme = Space::shared(DEFAULT_TEST_SEED.into())
        .sector(key.sector)
        .systems[usize::from(key.system)]
    .bodies[usize::from(key.body)]
    .theme;
    assert_eq!(theme, Theme::Ice);

    let mut game = title_game("console-start-at-boss");
    let target = crate::console::parse::parse_start_at("boss:colossus").expect("parses");
    let _ = game.start_test_world(DEFAULT_TEST_SEED, Some(&target));
    idle(&mut game, 5);
    let TpTarget::Boss(crate::console::names::BossName::Gate(boss)) = target else {
        panic!("a boss target");
    };
    let descent = Descent::ALL
        .into_iter()
        .find(|descent| descent.gate().is_some_and(|gate| gate.boss == boss))
        .expect("guards a Descent");
    let layout = game
        .world()
        .descent_strata_layout()
        .expect("a Deep Descent world");
    let (top, bottom) = layout.descent_rows(descent);
    assert!((top..=bottom).contains(&row(&game)), "row {}", row(&game));
}

/// `--start-at` reaches the index's every family of place (Wave 14 integration): a station,
/// a theme boss's lair, a faction's flagship and a village, each through the same `TravelTo`.
#[test]
fn start_at_reaches_a_station_a_lair_a_flagship_and_a_village() {
    use fatherload_simulation::place::{BodyId, Place};

    for (target, wanted) in [
        ("station:nearest", "space"),
        ("boss:sporemother", "body"),
        ("boss:pirates", "space"),
        ("village", "home"),
    ] {
        let mut game = title_game("console-start-at-families");
        let parsed = crate::console::parse::parse_start_at(target).expect("parses");
        let _ = game.start_test_world(DEFAULT_TEST_SEED, Some(&parsed));
        idle(&mut game, 5);
        let place = game.world().place_of(PLAYER);
        let ok = match wanted {
            "space" => place == Place::Space,
            "body" => matches!(place, Place::OnBody(BodyId::Space(_))),
            _ => place.is_home_store(),
        };
        assert!(ok, "--start-at {target}: {place:?}");
    }
}
