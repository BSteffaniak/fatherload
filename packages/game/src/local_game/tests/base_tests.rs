//! Base building end to end through `LocalGame::frame`: a split-screen seat builds in build mode
//! while the other keeps driving, and a stranded rig refuels at an underground fuel station
//! through its module menu.

use fatherload_simulation::{
    InputSequence, PlayerCommand, PlayerTickInput, TickAction, TickInputs, WorldState,
    base::{BuildCommand, Buildable, BuiltTile, Facing, ModuleKind},
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{
    LocalGame, PlayerId, PlayerInput, confirm_input, idle, interact, player, playing, press_seat,
    split_world,
};
use crate::shell::ModalScreen;

const FIRST: PlayerId = PlayerId::new(1);
const SECOND: PlayerId = PlayerId::new(2);
/// The floor row of the test cavern (rigs stand on `FLOOR - 1`).
const FLOOR: i32 = 60;

/// Opens a stone-floored cavern (columns 14..=40) at `FLOOR` and parks `player` in it at column
/// `x`, with credits to spend and the world's deepest dig far down (every module unlocked).
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
        rig.y = (FLOOR as f32 - 0.5) * TILE_SIZE;
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.credits = 20_000;
    if let Some(fatherload_simulation::SectionPayload::Progression(mut progression)) =
        world.section(fatherload_simulation::SectionKey::Progression)
    {
        progression.deepest_tile_reached = 400;
        world.apply_section(
            fatherload_simulation::SectionKey::Progression,
            fatherload_simulation::SectionPayload::Progression(progression),
        );
    }
}

fn press_build(game: &mut LocalGame, seat: usize, input: PlayerInput) {
    press_seat(game, seat, input);
}

#[test]
fn a_split_screen_seat_builds_while_the_other_keeps_driving() {
    let mut world = split_world(&[None, None]);
    cavern(&mut world, SECOND, 24);
    let mut game = playing(world, 2, "base-build");
    idle(&mut game, 3);

    press_build(
        &mut game,
        1,
        PlayerInput {
            build_mode: true,
            ..PlayerInput::default()
        },
    );
    assert!(game.shell.seat(1).build.is_some(), "seat 2 is building");
    assert!(game.shell.seat(0).build.is_none());
    assert!(game.shell.gameplay_input_enabled(0), "seat 1 still drives");
    assert!(
        !game.shell.gameplay_input_enabled(1),
        "the builder's rig is parked"
    );

    // The palette opens on the claim beacon, cursor beside the rig: stake the claim.
    press_build(&mut game, 1, confirm_input());
    idle(&mut game, 2);
    let claim = game
        .world()
        .bases()
        .claim_at(TilePosition {
            x: 25,
            y: FLOOR - 1,
        })
        .copied()
        .expect("claim staked");
    assert_eq!(claim.owner, SECOND);

    // Next pieces up to the metal plating, two tiles further right, and place it.
    let plating = Buildable::PALETTE
        .iter()
        .position(|item| *item == Buildable::Tile(BuiltTile::Plating))
        .expect("plating in the palette");
    for _ in 0..plating {
        press_build(
            &mut game,
            1,
            PlayerInput {
                cycle_explosive: true,
                ..PlayerInput::default()
            },
        );
    }
    assert_eq!(
        game.shell.seat(1).build.map(|draft| draft.selected),
        Some(plating)
    );
    for _ in 0..2 {
        press_build(
            &mut game,
            1,
            PlayerInput {
                menu_right: true,
                ..PlayerInput::default()
            },
        );
    }
    press_build(&mut game, 1, confirm_input());
    idle(&mut game, 2);
    let spot = TilePosition {
        x: 27,
        y: FLOOR - 1,
    };
    assert_eq!(
        game.world().terrain().tile(spot).map(|tile| tile.kind),
        Some(TileKind::Built(BuiltTile::Plating))
    );
    // The view shows the ghost and the bar while building.
    let snapshot = game.snapshot((1280, 720));
    assert!(snapshot.views[1].build_preview.is_some());
    assert!(snapshot.views[0].build_preview.is_none());

    // Deconstructing gives the credits back; leaving build mode frees the rig.
    let credits = player(&game, SECOND).credits;
    press_build(
        &mut game,
        1,
        PlayerInput {
            build_remove: true,
            ..PlayerInput::default()
        },
    );
    idle(&mut game, 2);
    assert_eq!(
        game.world().terrain().tile(spot).map(|tile| tile.kind),
        Some(TileKind::Air)
    );
    assert!(player(&game, SECOND).credits > credits);
    press_build(
        &mut game,
        1,
        PlayerInput {
            build_mode: true,
            ..PlayerInput::default()
        },
    );
    assert!(game.shell.seat(1).build.is_none());
    assert!(game.shell.gameplay_input_enabled(1));
}

#[test]
fn a_refused_placement_buzzes_with_its_reason_and_sends_nothing() {
    let mut world = split_world(&[None]);
    cavern(&mut world, FIRST, 24);
    let mut game = playing(world, 1, "base-refused");
    idle(&mut game, 2);
    press_build(
        &mut game,
        0,
        PlayerInput {
            build_mode: true,
            ..PlayerInput::default()
        },
    );
    // Point the cursor into the rock floor.
    press_build(
        &mut game,
        0,
        PlayerInput {
            menu_down: true,
            ..PlayerInput::default()
        },
    );
    press_build(&mut game, 0, confirm_input());
    idle(&mut game, 2);
    assert!(game.world().bases().is_empty(), "nothing was built");
    assert!(
        game.shell.seat(0).message.contains("in the way"),
        "{}",
        game.shell.seat(0).message
    );
}

/// Runs `command` from `player` through one real tick.
fn build(world: &mut WorldState, player: PlayerId, command: BuildCommand) {
    let mut inputs = TickInputs::default();
    inputs.players.insert(
        player,
        PlayerTickInput {
            controls: None,
            actions: vec![TickAction {
                sequence: InputSequence::new(0),
                command: PlayerCommand::Build(command),
            }],
        },
    );
    let tick = world.simulation_tick();
    let _ = world.simulate_tick(tick, &inputs);
}

#[test]
fn a_stranded_rig_refuels_at_its_underground_station() {
    let mut world = split_world(&[None]);
    cavern(&mut world, FIRST, 24);
    for (item, x) in [
        (Buildable::Module(ModuleKind::Beacon), 23),
        (Buildable::Module(ModuleKind::FuelStation), 25),
    ] {
        build(
            &mut world,
            FIRST,
            BuildCommand::Place {
                item,
                at: TilePosition { x, y: FLOOR - 1 },
                facing: Facing::Right,
            },
        );
    }
    assert_eq!(world.bases().modules().count(), 2);
    let rig = world.player_mut(FIRST).expect("rig");
    rig.fuel = 0.0;
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        rig.x = 26.5 * TILE_SIZE;
    }
    let mut game = playing(world, 1, "base-refuel");
    idle(&mut game, 2);
    assert!(game.world().failure_state(FIRST).is_some(), "stranded");
    press_seat(&mut game, 0, interact());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::BaseModule));
    let snapshot = game.snapshot((1280, 720));
    let menu = snapshot.views[0].base_module.as_ref().expect("menu view");
    assert_eq!(menu.kind(), ModuleKind::FuelStation);
    assert!(menu.rows[0].blocked.is_none(), "refuel is available");
    press_seat(&mut game, 0, confirm_input());
    idle(&mut game, 2);
    assert!(player(&game, FIRST).fuel > 0.0, "refuelled at depth");
    assert!(game.world().failure_state(FIRST).is_none());
}
