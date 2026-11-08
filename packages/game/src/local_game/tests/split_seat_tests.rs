//! Adversarial split-screen sequences through `LocalGame::frame`: simultaneous presses, pause
//! races, unplugged controllers, seat count changes and online hosts with local seats inside
//! buildings.

use super::*;
use crate::input::devices::{KeyboardLayout, PadFamily};

fn pause_input() -> PlayerInput {
    PlayerInput {
        pause: true,
        ..PlayerInput::default()
    }
}

fn right() -> PlayerInput {
    PlayerInput {
        horizontal: 1.0,
        ..PlayerInput::default()
    }
}

const PAD: InputDevice = InputDevice::Gamepad {
    index: 0,
    family: PadFamily::Xbox,
};

/// Seat 1 on the WASD keys, seat 2 on controller 0 (connected or not).
fn pad_frame(inputs: &[PlayerInput], pad_connected: bool) -> FrameInput {
    FrameInput {
        primary: inputs.first().copied().unwrap_or_default(),
        other_seats: inputs.iter().skip(1).copied().collect(),
        devices: vec![InputDevice::Keyboard(KeyboardLayout::Wasd), PAD],
        gamepads: if pad_connected {
            BTreeMap::from([(0, "Xbox Wireless Controller".to_owned())])
        } else {
            BTreeMap::new()
        },
        ..FrameInput::default()
    }
}

/// Seat `seat` moves the pause selection to `option` and confirms it.
fn choose_pause_option(game: &mut LocalGame, seat: usize, option: PauseOption) {
    let options = PauseOption::for_role(game.runtime.online_role());
    let target = options
        .iter()
        .position(|entry| *entry == option)
        .expect("pause lists the option");
    for _ in 0..options.len() {
        press_seat(
            game,
            seat,
            PlayerInput {
                menu_up: true,
                ..PlayerInput::default()
            },
        );
    }
    for _ in 0..target {
        press_seat(game, seat, down());
    }
    assert_eq!(game.shell.selected_pause_item, target);
    press_seat(game, seat, confirm_input());
}

#[test]
fn two_seats_using_the_same_door_on_the_same_frame_each_get_their_own_room() {
    let mut world = split_world(&[Some(SurfaceZone::Shop), Some(SurfaceZone::Shop)]);
    let second = PlayerId::new(2);
    for player in [PLAYER, second] {
        world.player_mut(player).expect("rig").credits = 10_000;
    }
    let mut game = playing(world, 2, "qa-same-door");
    idle(&mut game, 5);
    let _report = step_seats(&mut game, &[interact(), interact()]);
    idle(&mut game, 1);
    for seat in 0..2 {
        assert_eq!(game.shell.seat(seat).interior_zone, Some(SurfaceZone::Shop));
    }
    walk_seats_to_counters(&mut game, &[0, 1]);
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Shop));
    assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Shop));

    // Player 1 picks the cargo bay, player 2 the drill; both buy on the same frames.
    press_seat(&mut game, 0, down());
    press_seat(&mut game, 0, down());
    let (one, two) = (player(&game, PLAYER), player(&game, second));
    let _report = step_seats(&mut game, &[confirm_input(), confirm_input()]);
    idle(&mut game, 1);
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::ShopConfirm));
    assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::ShopConfirm));
    let _report = step_seats(&mut game, &[confirm_input(), confirm_input()]);
    idle(&mut game, 3);
    let (one_after, two_after) = (player(&game, PLAYER), player(&game, second));
    assert_eq!(one_after.cargo_bay_level, one.cargo_bay_level + 1);
    assert_eq!(one_after.drill_strength, one.drill_strength);
    assert_eq!(two_after.drill_strength, two.drill_strength + 1);
    assert_eq!(two_after.cargo_bay_level, two.cargo_bay_level);
    for seat in 0..2 {
        let state = game.shell.seat(seat);
        assert!(state.message.starts_with("Bought"), "{}", state.message);
        assert!(!state.has_pending_service());
    }
    assert!(
        !game.shell.world_should_tick(),
        "both inside: the world rests"
    );
}

#[test]
fn pressing_pause_and_use_on_the_same_frame_does_not_depend_on_seat_order() {
    // Seat 1 pauses while seat 2 walks in, and the other way round: in both cases the pause
    // menu opens for the seat that pressed it and the other seat's press still lands.
    for pauser in [0, 1] {
        let walker = 1 - pauser;
        let mut spots = [None, None];
        spots[walker] = Some(SurfaceZone::Shop);
        let mut game = playing(split_world(&spots), 2, "qa-pause-race");
        idle(&mut game, 5);
        let mut inputs = [PlayerInput::default(); 2];
        inputs[pauser] = pause_input();
        inputs[walker] = interact();
        let _report = step_seats(&mut game, &inputs);
        idle(&mut game, 1);
        assert_eq!(game.shell.run_mode, RunMode::Paused, "pauser {pauser}");
        assert_eq!(game.shell.menu_seat, pauser);
        assert_eq!(
            game.shell.seat(walker).interior_zone,
            Some(SurfaceZone::Shop),
            "pauser {pauser}: the other seat's door press is not lost"
        );
    }
}

#[test]
fn a_counter_left_open_under_the_pause_menu_survives_resume_and_save_but_not_load() {
    let path = temp_save("qa-pause-counter");
    let mut game = LocalGame::new(SettingsFile::default(), SaveTarget::File(path.clone()));
    game.runtime = Runtime::local(split_world(&[None, Some(SurfaceZone::Shop)]), 2);
    game.shell.enter_world("test", 0.0);
    idle(&mut game, 5);
    press_seat(&mut game, 1, interact());
    walk_seats_to_counters(&mut game, &[1]);
    press_seat(&mut game, 1, down());
    assert_eq!(game.shell.seat(1).selected_menu_item, 1);

    press_seat(&mut game, 0, pause_input());
    assert_eq!(game.shell.run_mode, RunMode::Paused);
    // Player 2's keys do nothing to their counter (or the pause menu) while paused.
    press_seat(&mut game, 1, down());
    press_seat(&mut game, 1, confirm_input());
    assert_eq!(game.shell.seat(1).selected_menu_item, 1);
    assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Shop));
    assert_eq!(game.shell.selected_pause_item, 0);
    assert!(!game.shell.seat(1).has_pending_service());

    // Save from the pause menu, close the slots and resume: the counter is still open.
    choose_pause_option(&mut game, 0, PauseOption::Save);
    assert_eq!(game.shell.modal, Some(ModalScreen::SaveSlots));
    press_seat(&mut game, 0, confirm_input());
    assert!(
        game.shell.message.starts_with("Saved slot"),
        "{}",
        game.shell.message
    );
    press_seat(&mut game, 0, back_input());
    press_seat(&mut game, 0, back_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Shop));
    assert_eq!(game.shell.seat(1).selected_menu_item, 1);
    assert!(step(&mut game, PlayerInput::default()).ticks_run > 0);

    // Pause only opens once the counter is closed (as playing alone); loading from the pause
    // menu then puts everyone outdoors.
    press_seat(&mut game, 1, pause_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    press_seat(&mut game, 1, back_input());
    assert!(game.shell.seat(1).inside());
    press_seat(&mut game, 1, pause_input());
    assert_eq!(game.shell.menu_seat, 1);
    choose_pause_option(&mut game, 1, PauseOption::Load);
    assert_eq!(game.shell.modal, Some(ModalScreen::LoadSlots));
    press_seat(&mut game, 1, confirm_input());
    let _ignored = std::fs::remove_file(&path);
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    assert_eq!(game.shell.menu_seat, 0);
    for seat in 0..2 {
        let state = game.shell.seat(seat);
        assert!(!state.inside() && state.modal.is_none(), "seat {seat}");
    }
}

#[test]
fn the_pause_menu_passes_to_player_one_when_its_controller_is_unplugged() {
    let mut game = playing(split_world(&[None, None]), 2, "qa-pause-unplug");
    for _ in 0..5 {
        let _report = game.frame(&pad_frame(&[], true), DT);
    }
    let _report = game.frame(
        &pad_frame(&[PlayerInput::default(), pause_input()], true),
        DT,
    );
    assert_eq!(game.shell.run_mode, RunMode::Paused);
    assert_eq!(game.shell.menu_seat, 1);
    // Player 1 cannot take the menu from a connected controller.
    let _report = game.frame(&pad_frame(&[back_input()], true), DT);
    assert_eq!(game.shell.run_mode, RunMode::Paused);

    // The controller is unplugged: player 1 takes over and can resume.
    let _report = game.frame(&pad_frame(&[], false), DT);
    assert_eq!(game.shell.menu_seat, 0, "player 1 drives the menu now");
    let _report = game.frame(&pad_frame(&[back_input()], false), DT);
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    assert!(game.frame(&pad_frame(&[], false), DT).ticks_run > 0);
}

#[test]
fn a_seat_whose_controller_is_unplugged_inside_a_building_holds_nobody_up() {
    let mut game = playing(
        split_world(&[None, Some(SurfaceZone::Shop)]),
        2,
        "qa-unplug-room",
    );
    for _ in 0..5 {
        let _report = game.frame(&pad_frame(&[], true), DT);
    }
    let _report = game.frame(&pad_frame(&[PlayerInput::default(), interact()], true), DT);
    assert!(game.shell.seat(1).inside());
    let start = player(&game, PLAYER).x;
    for _ in 0..60 {
        let report = game.frame(&pad_frame(&[right()], false), DT);
        assert!(report.ticks_run > 0);
    }
    assert!(player(&game, PLAYER).x > start + TILE_SIZE);
    assert!(game.shell.seat(1).inside(), "player 2 waits inside");
    // Plugged back in, player 2 walks out again.
    let _report = game.frame(
        &pad_frame(&[PlayerInput::default(), back_input()], true),
        DT,
    );
    assert!(!game.shell.seat(1).inside());
}

#[test]
fn a_stranded_seat_calls_rescue_while_another_seat_is_inside() {
    let mut world = split_world(&[Some(SurfaceZone::Shop), None]);
    let second = PlayerId::new(2);
    world.player_mut(second).expect("rig").fuel = 0.0;
    let mut game = playing(world, 2, "qa-rescue-inside");
    idle(&mut game, 3);
    assert!(game.world().failure_state(second).is_some());
    press_seat(&mut game, 0, interact());
    assert!(game.shell.seat(0).inside());
    press_seat(&mut game, 1, interact());
    idle(&mut game, 3);
    assert_eq!(game.world().rescue_count(second), 1);
    assert!(game.world().failure_state(second).is_none());
    assert_eq!(game.shell.seat(1).message, "Rescue crew dispatched.");
    assert!(game.shell.seat(0).inside(), "player 1 is still shopping");
    assert!(!game.shell.seat(0).message.starts_with("Rescue"));
}

#[test]
fn options_opened_by_player_two_are_driven_by_player_two() {
    let mut game = playing(split_world(&[None, None]), 2, "qa-options-seat");
    idle(&mut game, 3);
    press_seat(&mut game, 1, pause_input());
    choose_pause_option(&mut game, 1, PauseOption::Options);
    assert_eq!(game.shell.modal, Some(ModalScreen::Options));
    press_seat(&mut game, 0, down());
    assert_eq!(
        game.shell.selected_menu_item, 0,
        "player 1 does not drive it"
    );
    press_seat(&mut game, 1, down());
    assert_eq!(game.shell.selected_menu_item, 1);
    press_seat(&mut game, 0, back_input());
    assert_eq!(game.shell.modal, Some(ModalScreen::Options));
    press_seat(&mut game, 1, back_input());
    assert_eq!(game.shell.modal, None);
    assert_eq!(game.shell.run_mode, RunMode::Paused);
    press_seat(&mut game, 1, back_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
}

#[test]
fn ending_an_online_session_from_pause_with_seats_inside_resets_everyone() {
    let mut game = playing(
        split_world(&[Some(SurfaceZone::Shop), Some(SurfaceZone::Depot)]),
        2,
        "qa-end-session",
    );
    game.set_host_bind("127.0.0.1:0".parse().expect("addr"));
    game.handle_request(
        ShellRequest::OpenOnline,
        &mut Vec::new(),
        &mut FrameReport::default(),
    );
    assert_eq!(game.runtime.online_role(), Some(OnlineRole::Host));
    idle(&mut game, 3);
    let _report = step_seats(&mut game, &[interact(), interact()]);
    idle(&mut game, 1);
    walk_seats_to_counters(&mut game, &[0]);
    assert!(game.shell.seat(0).modal.is_some() && game.shell.seat(1).inside());

    press_seat(&mut game, 1, pause_input());
    choose_pause_option(&mut game, 1, PauseOption::EndSession);
    assert_eq!(game.shell.run_mode, RunMode::Title);
    assert_eq!(game.runtime.online_role(), None);
    assert_eq!(game.seats().len(), 1);
    assert_eq!(game.shell.seats.len(), 1);
    assert_eq!(game.shell.menu_seat, 0);
    assert!(!game.shell.seat(0).inside() && game.shell.seat(0).modal.is_none());
    assert!(!game.shell.world_should_tick());

    // A new game from the title starts clean.
    press_seat(&mut game, 0, confirm_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    assert!(!game.shell.seat(0).inside());
}

#[test]
fn spamming_use_and_back_at_the_door_never_stalls_the_others() {
    let mut game = playing(
        split_world(&[None, Some(SurfaceZone::Shop)]),
        2,
        "qa-door-spam",
    );
    idle(&mut game, 5);
    let start = player(&game, PLAYER).x;
    let second_start = player(&game, PlayerId::new(2)).x;
    for frame in 0..120 {
        let spam = match frame % 3 {
            0 => interact(),
            1 => back_input(),
            _ => PlayerInput {
                interact: true,
                back: true,
                ..PlayerInput::default()
            },
        };
        let report = step_seats(&mut game, &[right(), spam]);
        assert!(report.ticks_run > 0, "frame {frame}");
        assert_eq!(game.shell.run_mode, RunMode::Playing, "frame {frame}");
        let state = game.shell.seat(1);
        assert!(
            state.modal.is_none(),
            "frame {frame}: no counter opened at the door"
        );
    }
    assert!(player(&game, PLAYER).x > TILE_SIZE.mul_add(4.0, start));
    assert!(
        (player(&game, PlayerId::new(2)).x - second_start).abs() < 1.0,
        "player 2's rig never drove off while flicking in and out"
    );
}

#[test]
fn player_one_alone_inside_in_split_screen_keeps_the_world_running() {
    let mut game = playing(
        split_world(&[Some(SurfaceZone::Shop), None]),
        2,
        "qa-p1-inside",
    );
    idle(&mut game, 5);
    press_seat(&mut game, 0, interact());
    assert!(game.shell.seat(0).inside());
    let one = player(&game, PLAYER).x;
    let two = player(&game, PlayerId::new(2)).x;
    for _ in 0..40 {
        let report = step_seats(&mut game, &[right(), right()]);
        assert!(report.ticks_run > 0);
    }
    assert!(player(&game, PlayerId::new(2)).x > two + TILE_SIZE);
    assert!(
        (player(&game, PLAYER).x - one).abs() < 1.0,
        "player 1's rig waits"
    );
    assert!(
        game.shell.seat(0).interior_x > 82.0,
        "player 1 walked inside instead"
    );
}

#[test]
fn losing_and_regaining_a_seat_while_it_is_inside_leaves_no_stale_room() {
    let mut game = playing(
        split_world(&[None, Some(SurfaceZone::Shop)]),
        2,
        "qa-seat-drop",
    );
    idle(&mut game, 5);
    press_seat(&mut game, 1, interact());
    walk_seats_to_counters(&mut game, &[1]);
    assert!(game.shell.seat(1).modal.is_some());

    let _report = game.stage_local_world(1, |_, _| {});
    idle(&mut game, 1);
    assert_eq!(game.shell.seats.len(), 1);
    assert!(game.shell.world_should_tick());
    let _report = game.stage_local_world(2, |_, _| {});
    idle(&mut game, 1);
    assert_eq!(game.shell.seats.len(), 2);
    let state = game.shell.seat(1);
    assert!(!state.inside() && state.modal.is_none() && !state.has_pending_service());

    // A seat added mid-game by a controller also starts outdoors.
    game.shell.seat_mut(1).interior_zone = Some(SurfaceZone::Shop);
    assert_eq!(game.add_seat_for_controller(), Some(2));
    idle(&mut game, 1);
    assert_eq!(game.shell.seats.len(), 3);
    assert!(!game.shell.seat(2).inside());
    assert!(game.shell.world_should_tick(), "the new seat is outdoors");
}

#[test]
fn a_purchase_pending_when_the_pause_menu_opens_still_resolves() {
    let mut world = split_world(&[None, Some(SurfaceZone::Shop)]);
    let second = PlayerId::new(2);
    world.player_mut(second).expect("rig").credits = 10_000;
    let mut game = playing(world, 2, "qa-pending-pause");
    idle(&mut game, 5);
    press_seat(&mut game, 1, interact());
    walk_seats_to_counters(&mut game, &[1]);
    press_seat(&mut game, 1, confirm_input());
    assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::ShopConfirm));
    let before = player(&game, second).drill_strength;
    // Player 2 confirms the purchase on the frame player 1 pauses.
    let _report = step_seats(&mut game, &[pause_input(), confirm_input()]);
    assert_eq!(game.shell.run_mode, RunMode::Paused);
    idle(&mut game, 3);
    assert_eq!(player(&game, second).drill_strength, before + 1);
    assert!(!game.shell.seat(1).has_pending_service());
    assert!(game.shell.seat(1).message.starts_with("Bought"));
    // And the pause holds from then on.
    for _ in 0..5 {
        assert_eq!(step(&mut game, PlayerInput::default()).ticks_run, 0);
    }
}

#[test]
fn an_online_host_with_two_local_seats_keeps_serving_a_remote_client() {
    let (mut pair, client_player) = host_and_join_over_real_quic();
    pair.host.handle_request(
        ShellRequest::AddLocalSeat,
        &mut Vec::new(),
        &mut FrameReport::default(),
    );
    assert_eq!(pair.host.seats().len(), 2);
    pair.frame(PlayerInput::default(), PlayerInput::default());
    assert_eq!(pair.host.shell.seats.len(), 2);
    pair.host.shell.enter_interior(1, SurfaceZone::Shop);

    // The remote client keeps driving while the host's second player is inside.
    let start_x = pair.host.world().player(client_player).expect("rig").x;
    pair.holding_until("the client's movement on the host", right(), |pair| {
        pair.host.world().player(client_player).expect("rig").x > start_x + TILE_SIZE
    });
    // Both local players inside: an online host still never pauses.
    pair.host.shell.enter_interior(0, SurfaceZone::Depot);
    let tick = pair.host.world().simulation_tick();
    pair.until("host ticks with every local seat inside", |pair| {
        pair.host.world().simulation_tick().get() > tick.get() + 10
    });
    assert!(pair.host.shell.seat(0).inside() && pair.host.shell.seat(1).inside());
    assert_eq!(pair.client.shell.seats.len(), 1);
    assert!(!pair.client.shell.seat(0).inside());
    let mut requests = Vec::new();
    pair.host.handle_request(
        ShellRequest::LeaveOnline,
        &mut requests,
        &mut FrameReport::default(),
    );
    assert_eq!(pair.host.shell.seats.len(), 1);
    assert!(!pair.host.shell.seat(0).inside());
}
