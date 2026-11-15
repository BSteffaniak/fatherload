//! The paint bay end to end: a split-screen seat opens the garage from the repair counter, edits
//! a draft with live preview, applies it through the fixed tick, and every view sees the paint.

use fatherload_simulation::{RigCosmetics, cosmetics::PaintColor, economy::SurfaceZone};

use super::{
    LocalGame, PlayerId, PlayerInput, back_input, confirm_input, down, idle, interact, player,
    playing, press_seat, split_world, step_seats, walk_seats_to_counters,
};
use crate::shell::{GARAGE_COUNTER_ROW, GarageRow, ModalScreen};

const SECOND: PlayerId = PlayerId::new(2);

fn right() -> PlayerInput {
    PlayerInput {
        menu_right: true,
        ..PlayerInput::default()
    }
}

/// A two-seat game with seat 0 at the repair counter and seat 1 outdoors.
fn at_repair_counter(name: &str) -> LocalGame {
    let world = split_world(&[Some(SurfaceZone::Repair), None]);
    let mut game = playing(world, 2, name);
    idle(&mut game, 5);
    let _report = step_seats(&mut game, &[interact(), PlayerInput::default()]);
    idle(&mut game, 1);
    walk_seats_to_counters(&mut game, &[0]);
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Repair));
    game
}

fn open_garage(game: &mut LocalGame) {
    for _ in 0..GARAGE_COUNTER_ROW {
        press_seat(game, 0, down());
    }
    press_seat(game, 0, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Garage));
}

fn select_row(game: &mut LocalGame, row: GarageRow) {
    let target = GarageRow::ALL
        .iter()
        .position(|candidate| *candidate == row)
        .expect("row");
    while game.shell.seat(0).selected_menu_item != target {
        let up = game.shell.seat(0).selected_menu_item > target;
        press_seat(
            game,
            0,
            PlayerInput {
                menu_up: up,
                menu_down: !up,
                ..PlayerInput::default()
            },
        );
    }
}

#[test]
fn a_seat_repaints_its_rig_and_every_view_sees_it() {
    let mut game = at_repair_counter("garage-apply");
    let first = PlayerId::new(1);
    let before = RigCosmetics::default_for(first);
    open_garage(&mut game);
    // Hull paint one step right: a live preview only, nothing applied yet.
    press_seat(&mut game, 0, right());
    let draft = game.shell.seat(0).garage_preview().expect("draft");
    assert_eq!(draft.primary, before.primary.cycled(1));
    assert_eq!(player(&game, first).cosmetics, None);

    select_row(&mut game, GarageRow::Apply);
    press_seat(&mut game, 0, confirm_input());
    idle(&mut game, 4);
    assert_eq!(player(&game, first).cosmetics, Some(draft));
    assert_eq!(
        player(&game, SECOND).cosmetics,
        None,
        "only seat 0 repainted"
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Garage));

    // Both views draw seat 0's rig (and accent) in the new paint.
    let snapshot = game.snapshot((1280, 720));
    let presented = snapshot
        .players
        .iter()
        .find(|rig| rig.player_id == first)
        .expect("rig");
    assert_eq!(presented.cosmetics, draft);

    // Back returns to the repair counter on the paint bay row.
    press_seat(&mut game, 0, back_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Repair));
    assert_eq!(game.shell.seat(0).selected_menu_item, GARAGE_COUNTER_ROW);
}

#[test]
fn backing_out_discards_the_draft_and_locked_items_are_refused() {
    let mut game = at_repair_counter("garage-locked");
    let first = PlayerId::new(1);
    open_garage(&mut game);
    // Stealth paint is the last colour: one step left of the first.
    while game
        .shell
        .seat(0)
        .garage_preview()
        .is_some_and(|draft| draft.primary != PaintColor::Stealth)
    {
        press_seat(
            &mut game,
            0,
            PlayerInput {
                menu_left: true,
                ..PlayerInput::default()
            },
        );
    }
    select_row(&mut game, GarageRow::Apply);
    press_seat(&mut game, 0, confirm_input());
    idle(&mut game, 4);
    assert_eq!(player(&game, first).cosmetics, None, "locked paint refused");
    assert!(game.shell.seat(0).message.contains("locked"));

    press_seat(&mut game, 0, back_input());
    assert_eq!(game.shell.seat(0).garage, None, "the draft is discarded");
    assert_eq!(player(&game, first).cosmetics, None);
}

#[test]
fn crew_colours_apply_as_the_default_look() {
    let mut game = at_repair_counter("garage-default");
    let first = PlayerId::new(1);
    open_garage(&mut game);
    press_seat(&mut game, 0, right());
    select_row(&mut game, GarageRow::Apply);
    press_seat(&mut game, 0, confirm_input());
    idle(&mut game, 4);
    assert!(player(&game, first).cosmetics.is_some());
    select_row(&mut game, GarageRow::Default);
    press_seat(&mut game, 0, confirm_input());
    select_row(&mut game, GarageRow::Apply);
    press_seat(&mut game, 0, confirm_input());
    idle(&mut game, 4);
    assert_eq!(player(&game, first).cosmetics, None, "back on crew colours");
}
