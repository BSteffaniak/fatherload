//! Depth logistics end to end through `LocalGame::frame` (Part III F10): the use button at a
//! deep outpost opens its counter, a confirmed row goes through the host's tick, the
//! inventory's row opens the dialog anywhere, and a split-screen seat rides a lift while the
//! other keeps driving.

use fatherload_simulation::{
    WorldState,
    logistics::{LogisticsCommand, OutpostService, sites::LogisticsSites},
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{
    LocalGame, PlayerId, PlayerInput, confirm_input, idle, interact, player, playing, press_seat,
    split_world,
};
use crate::shell::ModalScreen;

const FIRST: PlayerId = PlayerId::new(1);
const SECOND: PlayerId = PlayerId::new(2);
const OUTPOST: i32 = 30;
const FLOOR: i32 = 70;
const LIFT: i32 = OUTPOST + 6;

/// A stone slab with an outpost on `FLOOR` and an ancient lift from row 40 down beside it.
fn outpost_world(world: &mut WorldState) {
    for y in 36..FLOOR + 4 {
        for x in OUTPOST - 10..LIFT + 6 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Stone);
        }
    }
    let mut sites = LogisticsSites::default();
    sites.place_outpost(OUTPOST, FLOOR, 1, "Test Camp");
    sites.place_ancient_lift(LIFT, 40, FLOOR - 1, 15);
    world.install_logistics_sites(&sites, true);
}

fn park(world: &mut WorldState, id: PlayerId, x: i32, row: i32) {
    let rig = world.player_mut(id).expect("rig");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        rig.x = (x as f32 + 0.5) * TILE_SIZE;
        rig.y = (row as f32 + 0.5) * TILE_SIZE;
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.credits = 40_000;
}

fn select(game: &mut LocalGame, seat: usize, command: LogisticsCommand) {
    let row = game
        .shell
        .seat(seat)
        .logistics
        .rows
        .iter()
        .position(|row| row.command == command)
        .expect("row offered");
    game.shell.seat_mut(seat).selected_menu_item = row;
}

#[test]
fn the_outpost_counter_opens_with_use_and_refuels_through_the_tick() {
    let mut world = split_world(&[None]);
    outpost_world(&mut world);
    park(&mut world, FIRST, OUTPOST, FLOOR - 1);
    world.player_mut(FIRST).expect("rig").fuel = 50.0;
    let mut game = playing(world, 1, "logistics-outpost");
    idle(&mut game, 3);
    press_seat(&mut game, 0, interact());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Logistics));
    let outpost = *game
        .world()
        .logistics()
        .outposts
        .keys()
        .next()
        .expect("outpost");
    assert!(game.world().logistics().outposts[&outpost].reached);
    select(
        &mut game,
        0,
        LogisticsCommand::Outpost {
            outpost,
            service: OutpostService::Refuel,
        },
    );
    press_seat(&mut game, 0, confirm_input());
    idle(&mut game, 3);
    let rig = player(&game, FIRST);
    assert!(
        (rig.fuel - rig.fuel_capacity).abs() < 1.0,
        "refuelled: {}",
        rig.fuel
    );
    assert!(rig.credits < 40_000);
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Logistics));
}

#[test]
fn the_inventory_row_opens_the_dialog_anywhere() {
    let mut world = split_world(&[None]);
    outpost_world(&mut world);
    park(&mut world, FIRST, OUTPOST - 6, FLOOR - 1);
    let mut game = playing(world, 1, "logistics-inventory");
    idle(&mut game, 2);
    press_seat(
        &mut game,
        0,
        PlayerInput {
            inventory: true,
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Inventory));
    let last = crate::shell::hopper::hopper_rows(&player(&game, FIRST)).len() + 1;
    game.shell.seat_mut(0).selected_menu_item = last;
    press_seat(&mut game, 0, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Logistics));
    assert!(
        game.shell
            .seat(0)
            .logistics
            .rows
            .iter()
            .any(|row| row.command == LogisticsCommand::PlaceBeacon)
    );
}

#[test]
fn a_split_screen_seat_rides_the_lift_while_the_other_drives() {
    let mut world = split_world(&[None, None]);
    outpost_world(&mut world);
    park(&mut world, SECOND, LIFT, FLOOR - 1);
    let mut game = playing(world, 2, "logistics-split");
    idle(&mut game, 3);
    // Restore from the bottom station, then ride up.
    press_seat(&mut game, 1, interact());
    assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Logistics));
    let shaft = game
        .world()
        .logistics()
        .lift_on_column(LIFT)
        .expect("lift")
        .id;
    select(&mut game, 1, LogisticsCommand::RestoreLift { shaft });
    press_seat(&mut game, 1, confirm_input());
    idle(&mut game, 2);
    assert!(game.world().logistics().lifts[&shaft].restored);
    select(
        &mut game,
        1,
        LogisticsCommand::RideLift { shaft, to_row: 40 },
    );
    press_seat(&mut game, 1, confirm_input());
    assert_eq!(
        game.shell.seat(1).modal,
        None,
        "the dialog closes for the ride"
    );
    assert!(game.world().logistics().is_riding(SECOND));
    assert!(game.shell.gameplay_input_enabled(0), "seat 1 still drives");
    idle(&mut game, 120);
    assert!(!game.world().logistics().is_riding(SECOND));
    assert_eq!(
        player(&game, SECOND).tile_position(TILE_SIZE),
        TilePosition { x: LIFT, y: 40 }
    );
}
