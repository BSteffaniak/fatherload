//! Test mode through `LocalGame::frame`: the setup dialog starts a test-mode world, and each
//! split-screen seat warps its own rig and flips the crew toggles from the pause menu.

use fatherload_simulation::{terrain::DeepStratum, worldgen::Band};

use super::*;
use crate::shell::WARP_BANDS;

fn pause_input() -> PlayerInput {
    PlayerInput {
        pause: true,
        ..PlayerInput::default()
    }
}

fn up() -> PlayerInput {
    PlayerInput {
        menu_up: true,
        ..PlayerInput::default()
    }
}

/// Seat `seat` pauses, highlights `option` and confirms it.
fn choose(game: &mut LocalGame, seat: usize, option: PauseOption) {
    press_seat(game, seat, pause_input());
    assert_eq!(game.shell.run_mode, RunMode::Paused);
    let options = PauseOption::for_session(None, game.runtime.world().is_test_mode());
    let target = options
        .iter()
        .position(|entry| *entry == option)
        .expect("pause lists the option");
    for _ in 0..options.len() {
        press_seat(game, seat, up());
    }
    for _ in 0..target {
        press_seat(game, seat, down());
    }
    press_seat(game, seat, confirm_input());
}

fn band_of(game: &LocalGame, player: PlayerId) -> Band {
    let world = game.runtime.world();
    let rig = world.player(player).expect("rig");
    world.terrain().band_at(rig.tile_position(TILE_SIZE))
}

#[test]
fn the_setup_dialog_starts_a_test_mode_world() {
    let mut game = title_game("test-mode-setup");
    let _ = press(&mut game, confirm_input());
    assert_eq!(game.shell.modal, Some(ModalScreen::WorldSetup));
    let test_row = crate::shell::world_setup_start_row() - 1;
    for _ in 0..test_row {
        let _ = press(&mut game, down());
    }
    let _ = press(&mut game, confirm_input());
    assert!(game.shell.world_setup.as_ref().expect("dialog").test_mode);
    let _ = press(&mut game, down());
    let _ = press(&mut game, confirm_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    let world = game.runtime.world();
    assert!(world.is_test_mode());
    assert_eq!(world.player(PLAYER).expect("rig").drill_mount_level, 2);
}

#[test]
fn each_seat_warps_its_own_rig_and_toggles_apply_on_resume() {
    let mut world = split_world(&[None, None]);
    world.enable_test_mode();
    let second = PlayerId::new(2);
    let mut game = playing(world, 2, "test-mode-split");
    idle(&mut game, 5);
    let toggles = game.runtime.world().test_mode().expect("test mode");
    assert!(
        toggles.invulnerable && toggles.infinite_fuel,
        "test mode starts invulnerable with infinite fuel"
    );
    let first_band = band_of(&game, PLAYER);

    // Seat 2 warps to the Fossil Oceans.
    choose(&mut game, 1, PauseOption::WarpToLayer);
    assert_eq!(game.shell.modal, Some(ModalScreen::WarpToLayer));
    let target = Band::Deep(DeepStratum::FossilOceans);
    let row = WARP_BANDS
        .iter()
        .position(|band| *band == target)
        .expect("listed");
    for _ in 0..row {
        press_seat(&mut game, 1, down());
    }
    press_seat(&mut game, 1, confirm_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    idle(&mut game, 10);
    assert_eq!(band_of(&game, second), target);
    assert_eq!(band_of(&game, PLAYER), first_band, "seat 1 stays put");

    // Seat 1 turns invulnerability off: shown at once, applied once the world resumes.
    choose(&mut game, 0, PauseOption::ToggleInvulnerable);
    let shown = game
        .shell
        .shown_test_mode(game.runtime.world().test_mode())
        .expect("test mode");
    assert!(!shown.invulnerable);
    press_seat(&mut game, 0, pause_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    idle(&mut game, 3);
    assert!(
        !game
            .runtime
            .world()
            .test_mode()
            .expect("test mode")
            .invulnerable
    );
    assert_eq!(game.shell.pending_test_mode, None);
}

/// Seat `seat` steps the open toolkit dialog to `row` and confirms it.
fn use_toolkit_row(game: &mut LocalGame, seat: usize, row: crate::shell::test_toolkit::ToolkitRow) {
    let descents = game
        .runtime
        .world()
        .world_settings()
        .generator
        .has_descents();
    let rows = crate::shell::test_toolkit::ToolkitRow::rows(descents);
    let target = rows.iter().position(|entry| *entry == row).expect("listed");
    for _ in 0..rows.len() {
        press_seat(game, seat, up());
    }
    for _ in 0..target {
        press_seat(game, seat, down());
    }
    press_seat(game, seat, confirm_input());
}

#[test]
fn a_seat_uses_the_toolkit_on_its_own_rig() {
    use crate::shell::test_toolkit::ToolkitRow;
    use fatherload_simulation::{enemy::EnemyDifficulty, test_mode::StockItem};
    let mut world = split_world(&[None, None]);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    world.enable_test_mode();
    let second = PlayerId::new(2);
    let mut game = playing(world, 2, "test-toolkit-split");
    idle(&mut game, 5);
    assert_eq!(
        game.runtime.world().test_stock_gaps(second),
        [] as [String; 0]
    );

    // Seat 2 opens the toolkit from the pause menu, turns infinite supplies off and spawns a
    // creature beside its own rig.
    choose(&mut game, 1, PauseOption::TestToolkit);
    assert_eq!(game.shell.modal, Some(ModalScreen::TestToolkit));
    use_toolkit_row(&mut game, 1, ToolkitRow::InfiniteSupplies);
    use_toolkit_row(&mut game, 1, ToolkitRow::SpawnCreature);
    press_seat(&mut game, 1, back_input());
    assert_eq!(game.shell.modal, None);
    press_seat(&mut game, 1, pause_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    idle(&mut game, 3);
    let world = game.runtime.world();
    assert!(!world.test_mode().expect("test mode").infinite_supplies);
    let near = |player: PlayerId| {
        let rig = world.player(player).expect("rig");
        world
            .enemies()
            .filter(|(entity, _)| {
                let (dx, dy) = (entity.body.x - rig.x, entity.body.y - rig.y);
                dx.mul_add(dx, dy * dy) <= (6.0 * TILE_SIZE) * (6.0 * TILE_SIZE)
            })
            .count()
    };
    assert!(near(second) >= 1, "spawned beside seat 2's rig");
    assert_eq!(world.stock_count(second, StockItem::Bombs), 99);
}
