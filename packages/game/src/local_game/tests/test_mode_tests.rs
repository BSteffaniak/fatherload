//! Test mode through `LocalGame::frame`: the setup dialog starts a test-mode world, and each
//! split-screen seat warps its own rig and flips the crew toggles from the pause menu.

use drillgame_simulation::{terrain::DeepStratum, worldgen::Band};

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

    // Seat 1 turns on invulnerability: shown at once, applied once the world resumes.
    choose(&mut game, 0, PauseOption::ToggleInvulnerable);
    let shown = game
        .shell
        .shown_test_mode(game.runtime.world().test_mode())
        .expect("test mode");
    assert!(shown.invulnerable);
    press_seat(&mut game, 0, pause_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    idle(&mut game, 3);
    assert!(
        game.runtime
            .world()
            .test_mode()
            .expect("test mode")
            .invulnerable
    );
    assert_eq!(game.shell.pending_test_mode, None);
}
