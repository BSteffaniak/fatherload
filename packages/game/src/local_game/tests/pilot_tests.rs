//! The pilot on foot end to end through `LocalGame::frame` (Part II F5): the use button climbs
//! out and boards again, the seat's controls and camera follow the pilot, pushing up at a door
//! walks into the building (and walking out through it never bounces straight back in), and a
//! split-screen seat walks while the other drives.

use fatherload_simulation::{
    WorldState,
    economy::SurfaceZone,
    surface::SURFACE_BUILDINGS,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{
    LocalGame, PLAYER, PlayerId, PlayerInput, idle, interact, playing, press, press_seat,
    split_world, step, step_seats,
};

/// The floor row of the test cavern.
const FLOOR: i32 = 60;

/// A stone-floored cavern (columns 14..=40) at `FLOOR` with `player`'s rig resting at column
/// `x`: no building, door or module anywhere near, so the use button means the rig.
fn cavern(world: &mut WorldState, player: PlayerId, x: i32) {
    for y in FLOOR - 7..=FLOOR {
        for column in 14..=40 {
            let kind = if y == FLOOR {
                TileKind::Stone
            } else {
                TileKind::Air
            };
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x: column, y }, kind);
        }
    }
    let rig = world.player_mut(player).expect("rig");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        rig.x = (x as f32 + 0.5) * TILE_SIZE;
        rig.y = (FLOOR as f32).mul_add(TILE_SIZE, -11.0);
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn hold(game: &mut LocalGame, frames: usize, input: PlayerInput) {
    for _ in 0..frames {
        let _report = step(game, input);
    }
}

fn walk(horizontal: f32) -> PlayerInput {
    PlayerInput {
        horizontal,
        ..PlayerInput::default()
    }
}

fn pilot_x(game: &LocalGame, player: PlayerId) -> f32 {
    game.world().pilot_of(player).expect("on foot").1.x
}

#[test]
fn the_use_button_climbs_out_and_boards_the_rig_again() {
    let mut world = super::super::new_world();
    cavern(&mut world, PLAYER, 24);
    let mut game = playing(world, 1, "pilot-exit");
    idle(&mut game, 10);
    press(&mut game, interact());
    assert!(game.world().is_on_foot(PLAYER), "climbed out");
    let rig_x = game.world().player(PLAYER).expect("rig").x;

    // The seat's controls now walk the pilot; the rig stays parked.
    let start = pilot_x(&game, PLAYER);
    hold(&mut game, 40, walk(1.0));
    assert!(
        pilot_x(&game, PLAYER) > start + TILE_SIZE,
        "the pilot walked"
    );
    let rig = game.world().player(PLAYER).expect("rig");
    assert!((rig.x - rig_x).abs() < 0.01, "the rig stayed parked");
    // The camera follows the pilot.
    let snapshot = game.snapshot((1280, 720));
    let view = snapshot.primary_view().expect("view");
    let (visible_width, _) = view.visible_world_size();
    let centre = visible_width.mul_add(0.5, view.camera.x);
    assert!(
        (centre - pilot_x(&game, PLAYER)).abs() < TILE_SIZE,
        "camera on the pilot"
    );
    assert!(view.pilot.is_some(), "the HUD shows the pilot's gauges");

    // Too far to board: the button says so and nothing happens.
    press(&mut game, interact());
    assert!(game.world().is_on_foot(PLAYER));
    // Walk back beside the rig and board.
    for _ in 0..240 {
        if game.world().can_board_rig(PLAYER) {
            break;
        }
        let _report = step(&mut game, walk(-1.0));
    }
    assert!(game.world().can_board_rig(PLAYER), "back beside the rig");
    press(&mut game, interact());
    assert!(!game.world().is_on_foot(PLAYER), "boarded");
}

#[test]
fn the_jump_button_jumps_on_foot() {
    let mut world = super::super::new_world();
    cavern(&mut world, PLAYER, 24);
    let mut game = playing(world, 1, "pilot-jump");
    idle(&mut game, 10);
    press(&mut game, interact());
    idle(&mut game, 20);
    let ground = game.world().pilot_of(PLAYER).expect("pilot").1.y;
    let jump = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    hold(&mut game, 12, jump);
    let air = game.world().pilot_of(PLAYER).expect("pilot").1.y;
    assert!(air < ground - TILE_SIZE, "jumped: {ground} -> {air}");
}

#[test]
fn pushing_up_at_a_door_walks_in_and_walking_out_does_not_bounce_back() {
    let depot = SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == SurfaceZone::Depot)
        .copied()
        .expect("depot");
    let mut world = super::super::new_world();
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    let door_x = (depot.tile_x as f32 + 2.5) * TILE_SIZE;
    // The rig parked beside the town, the pilot standing at the depot's door.
    let rig_ground = world.player(PLAYER).expect("rig").y;
    let _ = world.place_pilot(PLAYER, door_x, rig_ground + 11.0);
    let mut game = playing(world, 1, "pilot-door");
    idle(&mut game, 10);
    assert!(game.world().is_on_foot(PLAYER));
    assert_eq!(
        game.world().player_surface_zone(PLAYER),
        Some(SurfaceZone::Depot)
    );
    let up = PlayerInput {
        thrust: true,
        ..PlayerInput::default()
    };
    let _report = step(&mut game, up);
    assert_eq!(
        game.shell.seat(0).interior_zone,
        Some(SurfaceZone::Depot),
        "walked in"
    );
    // Walk out through the door on the left.
    hold(&mut game, 60, walk(-1.0));
    assert_eq!(game.shell.seat(0).interior_zone, None, "walked out");
    // Still at the door, holding up does not walk straight back in.
    hold(&mut game, 5, up);
    assert_eq!(game.shell.seat(0).interior_zone, None, "the door latch");
}

#[test]
fn a_split_screen_seat_walks_while_the_other_drives() {
    let first = PlayerId::new(1);
    let second = PlayerId::new(2);
    let mut world = split_world(&[None, None]);
    cavern(&mut world, first, 20);
    let mut game = playing(world, 2, "pilot-split");
    idle(&mut game, 10);
    press_seat(&mut game, 0, interact());
    assert!(game.world().is_on_foot(first));
    assert!(!game.world().is_on_foot(second));
    let start = pilot_x(&game, first);
    let second_start = game.world().player(second).expect("rig").x;
    for _ in 0..30 {
        let _report = step_seats(&mut game, &[walk(1.0), walk(-1.0)]);
    }
    assert!(pilot_x(&game, first) > start, "seat 1's pilot walked");
    assert!(
        game.world().player(second).expect("rig").x < second_start,
        "seat 2's rig drove"
    );
    let snapshot = game.snapshot((1280, 720));
    assert!(snapshot.views[0].pilot.is_some());
    assert!(snapshot.views[1].pilot.is_none());
}

#[test]
fn pressing_use_twice_away_from_the_rig_radios_for_a_pickup() {
    let mut world = super::super::new_world();
    cavern(&mut world, PLAYER, 24);
    world.player_mut(PLAYER).expect("rig").credits = 500;
    let mut game = playing(world, 1, "pilot-pickup");
    idle(&mut game, 10);
    press(&mut game, interact());
    hold(&mut game, 60, walk(1.0));
    assert!(!game.world().can_board_rig(PLAYER), "walked away");
    // The first press explains and arms the radio; the second calls the pickup.
    press(&mut game, interact());
    assert!(game.world().is_on_foot(PLAYER));
    press(&mut game, interact());
    idle(&mut game, 2);
    assert!(!game.world().is_on_foot(PLAYER), "back aboard");
    assert_eq!(
        game.world().player(PLAYER).expect("rig").credits,
        500 - fatherload_simulation::pilot::PILOT_RECALL_FEE
    );
}
