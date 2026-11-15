//! God mode (test mode) feel: top speed well above the top engine tier, instant drilling, and
//! the no-clip flight toggle.

use crate::{
    ControlState, PlayerCommand, PlayerId, PlayerScopedCommandOutcome, TickInputs, TickOutput,
    WorldState,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    test_mode::TestModeAction,
};

use super::{
    NO_CLIP_SPEED,
    movement::{authoritative_player_collides, engine_top_speed_tiles},
};

const PLAYER: PlayerId = PlayerId::new(1);

fn test_world() -> WorldState {
    let mut world = WorldState::new_game(PLAYER);
    world.enable_test_mode();
    world
}

fn act(world: &mut WorldState, action: TestModeAction) -> TickOutput {
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(PLAYER, PlayerCommand::TestMode(action));
    world.simulate_tick(tick, &inputs)
}

fn hold(world: &mut WorldState, controls: ControlState, ticks: u32) -> Vec<TickOutput> {
    let inputs = TickInputs::held(PLAYER, controls);
    (0..ticks)
        .map(|_| {
            let tick = world.simulation_tick();
            world.simulate_tick(tick, &inputs)
        })
        .collect()
}

/// Fills a block of tiles around `(column, row)` with `kind`.
fn fill(world: &mut WorldState, column: i32, row: i32, radius: i32, kind: TileKind) {
    for y in row - radius..=row + radius {
        for x in column - radius..=column + radius {
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
}

/// Puts the rig at the centre of tile `(column, row)`, at rest.
fn place(world: &mut WorldState, column: i32, row: i32) {
    let rig = world.player_mut(PLAYER).expect("player");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    let (x, y) = (column as f32, row as f32);
    rig.x = TILE_SIZE.mul_add(x, TILE_SIZE * 0.5);
    rig.y = TILE_SIZE.mul_add(y, TILE_SIZE * 0.5);
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

#[test]
fn god_mode_drives_well_above_the_top_engine_tier() {
    let top_tier = engine_top_speed_tiles(15) * TILE_SIZE;
    let speed = |god: bool| {
        let mut world = WorldState::new_game(PLAYER);
        if god {
            world.enable_test_mode();
        }
        world.player_mut(PLAYER).expect("player").engine_level = 15;
        // An open corridor on a dirt floor.
        let spawn = world
            .player(PLAYER)
            .expect("player")
            .tile_position(TILE_SIZE);
        for x in 2..world.terrain().width() - 2 {
            for y in spawn.y - 3..=spawn.y + 1 {
                let kind = if y > spawn.y {
                    TileKind::Dirt
                } else {
                    TileKind::Air
                };
                let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
            }
        }
        world.player_mut(PLAYER).expect("player").x = 8.0 * TILE_SIZE;
        let _ = hold(&mut world, ControlState::new(1.0, false, false), 90);
        world.player(PLAYER).expect("player").velocity_x
    };
    let max_tier = speed(false);
    let god = speed(true);
    assert!(
        max_tier > top_tier * 0.95,
        "max tier {max_tier} vs {top_tier}"
    );
    assert!(god > max_tier * 2.0, "god {god} vs max tier {max_tier}");
}

#[test]
fn god_mode_drills_a_tile_the_tick_the_drill_touches_it() {
    let mut world = test_world();
    let spawn = world
        .player(PLAYER)
        .expect("player")
        .tile_position(TILE_SIZE);
    // Rest the rig on hard rock.
    fill(&mut world, spawn.x, spawn.y + 2, 1, TileKind::HardRock);
    fill(&mut world, spawn.x, spawn.y, 0, TileKind::Air);
    let _ = hold(&mut world, ControlState::default(), 60);
    let below = TilePosition {
        x: world
            .player(PLAYER)
            .expect("player")
            .tile_position(TILE_SIZE)
            .x,
        y: world
            .player(PLAYER)
            .expect("player")
            .tile_position(TILE_SIZE)
            .y
            + 1,
    };
    assert_eq!(
        world.terrain().tile(below).map(|tile| tile.kind),
        Some(TileKind::HardRock)
    );
    let outputs = hold(&mut world, ControlState::new(0.0, false, true), 1);
    assert!(
        outputs[0]
            .terrain_ops
            .iter()
            .any(|change| change.position == below && change.tile.kind == TileKind::Air),
        "the hard rock under the rig falls on the first tick"
    );
}

#[test]
fn no_clip_flies_through_rock_without_drilling_or_damage() {
    let mut world = test_world();
    let _ = act(&mut world, TestModeAction::SetInvulnerable(false));
    let output = act(&mut world, TestModeAction::SetNoClip(true));
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::Applied
    );
    assert!(world.no_clip_active());
    fill(&mut world, 60, 40, 12, TileKind::Stone);
    place(&mut world, 60, 40);
    let hull = world.player(PLAYER).expect("player").hull;
    let start_y = world.player(PLAYER).expect("player").y;
    let outputs = hold(&mut world, ControlState::new(0.0, false, true), 60);
    let rig = world.player(PLAYER).expect("player");
    assert!(
        rig.y > 4.0f32.mul_add(TILE_SIZE, start_y),
        "flew down {}",
        rig.y - start_y
    );
    assert!(rig.velocity_y <= NO_CLIP_SPEED);
    assert!((rig.hull - hull).abs() < f32::EPSILON);
    assert!(outputs.iter().all(|output| output.terrain_ops.is_empty()));
    assert!(world.active_drill(PLAYER).is_none());
    // Thrust flies back up; sideways works too.
    let _ = hold(&mut world, ControlState::new(-1.0, true, false), 90);
    let rig = world.player(PLAYER).expect("player");
    assert!(rig.y < 4.0f32.mul_add(TILE_SIZE, start_y));
    assert!(rig.x < TILE_SIZE * 58.0);
}

#[test]
fn turning_no_clip_off_frees_a_rig_left_inside_rock() {
    let mut world = test_world();
    let _ = act(&mut world, TestModeAction::SetNoClip(true));
    fill(&mut world, 60, 40, 8, TileKind::Stone);
    place(&mut world, 60, 40);
    let rig = world.player(PLAYER).expect("player");
    assert!(authoritative_player_collides(world.terrain(), rig.x, rig.y));
    let _ = act(&mut world, TestModeAction::SetNoClip(false));
    assert!(!world.no_clip_active());
    let rig = world.player(PLAYER).expect("player");
    assert!(!authoritative_player_collides(
        world.terrain(),
        rig.x,
        rig.y
    ));
    // Next to open air, the rig steps into it instead of carving.
    fill(&mut world, 60, 40, 8, TileKind::Stone);
    fill(&mut world, 61, 38, 0, TileKind::Air);
    place(&mut world, 60, 40);
    let _ = act(&mut world, TestModeAction::SetNoClip(true));
    let _ = act(&mut world, TestModeAction::SetNoClip(false));
    let rig = world.player(PLAYER).expect("player");
    assert_eq!(rig.tile_position(TILE_SIZE), TilePosition { x: 61, y: 38 });
    assert_eq!(
        world
            .terrain()
            .tile(TilePosition { x: 60, y: 40 })
            .map(|tile| tile.kind),
        Some(TileKind::Stone)
    );
}

#[test]
fn no_clip_is_ignored_outside_test_mode_and_survives_serde() {
    let mut world = WorldState::new_game(PLAYER);
    let output = act(&mut world, TestModeAction::SetNoClip(true));
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
    assert!(!world.no_clip_active());

    let mut world = test_world();
    let _ = act(&mut world, TestModeAction::SetNoClip(true));
    let state = world.test_mode().expect("test mode");
    let json = serde_json::to_string(&state).expect("encode");
    assert_eq!(
        serde_json::from_str::<crate::TestModeState>(&json).expect("decode"),
        state
    );
    // Saves from before the toggle existed load with it off.
    let old: crate::TestModeState =
        serde_json::from_str(r#"{"invulnerable":true,"infinite_fuel":false}"#).expect("decode");
    assert!(!old.no_clip);
}
