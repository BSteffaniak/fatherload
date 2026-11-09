//! Regression scenarios for loose bombs, chains, persistence and replication.

use super::{WorldImport, bombs::MAX_ACTIVE_BOMBS, quests::QuestNotes};
use crate::{
    GameplayEventKind, PlacedBomb, PlayerCommand, SimulationTick, TickInputs, WorldState,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};
const LOCAL_PLAYER_ID: crate::PlayerId = crate::PlayerId::new(0);

fn arena() -> WorldState {
    let mut world = WorldState::from_import(&WorldImport::new_game(LOCAL_PLAYER_ID));
    for y in 8..=32 {
        for x in 8..=32 {
            let kind = if y == 32 {
                TileKind::Bedrock
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    world
}

fn bomb(id: u64, x: f32, y: f32, deadline: u64) -> PlacedBomb {
    PlacedBomb {
        id,
        x,
        y,
        previous_x: x,
        previous_y: y,
        detonation_tick: SimulationTick::new(deadline),
        timer_seconds: 100.0,
        ..PlacedBomb::default()
    }
}

fn advance(world: &mut WorldState, count: u64) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..count {
        let tick = world.simulation_tick().next();
        let mut output = crate::TickOutput {
            tick,
            ..crate::TickOutput::default()
        };
        world.advance_bombs(tick, &mut QuestNotes::default(), &mut output);
        events.extend(output.events.into_iter().map(|event| event.kind));
        world.set_simulation_tick(tick);
    }
    events
}

#[test]
fn unsupported_bombs_fall_bounce_settle_and_wake_when_support_is_blasted() {
    let mut world = arena();
    let x = 20.5 * TILE_SIZE;
    let y = 20.5 * TILE_SIZE;
    for column in 18..=22 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x: column, y: 23 }, TileKind::Stone);
    }
    world.bombs.push(bomb(0, x, y, 10_000));
    let events = advance(&mut world, 180);
    assert!(world.bombs[0].y > y + TILE_SIZE);
    assert!(world.bombs[0].y < 23.0 * TILE_SIZE);
    assert!(world.bombs[0].velocity_y.abs() < 1.0);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::BombBounced { .. }))
    );
    let resting_y = world.bombs[0].y;
    let _ = world.detonate_bomb_at(TilePosition { x: 20, y: 23 }, 2);
    advance(&mut world, 60);
    assert!(world.bombs[0].y > resting_y + TILE_SIZE);
}

#[test]
fn fast_bombs_cannot_tunnel_through_a_one_tile_wall_or_bedrock_floor() {
    let mut world = arena();
    for y in 8..32 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x: 24, y }, TileKind::Stone);
    }
    let mut moving = bomb(1, 22.0 * TILE_SIZE, 25.0 * TILE_SIZE, 10_000);
    moving.velocity_x = 10_000.0;
    moving.velocity_y = 10_000.0;
    world.bombs.push(moving);
    advance(&mut world, 240);
    assert!(world.bombs[0].x < 24.0 * TILE_SIZE);
    assert!(world.bombs[0].y + super::bombs::BOMB_RADIUS < 32.0 * TILE_SIZE);
}

#[test]
fn blast_kicks_outwards_and_shortens_fuses_without_instantly_detonating_neighbors() {
    let mut world = arena();
    let x = 20.5 * TILE_SIZE;
    let y = 20.5 * TILE_SIZE;
    world.bombs = vec![
        bomb(0, x, y, 1),
        bomb(1, x + TILE_SIZE, y, 600),
        bomb(2, x - TILE_SIZE, y, 600),
    ];
    let events = advance(&mut world, 1);
    assert_eq!(world.bombs.len(), 2);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, GameplayEventKind::BombDetonated { .. }))
            .count(),
        1
    );
    assert!(world.bombs[0].velocity_x > 0.0);
    assert!(world.bombs[1].velocity_x < 0.0);
    assert!(
        world
            .bombs
            .iter()
            .all(|bomb| bomb.chain_triggered && bomb.detonation_tick.get() < 60)
    );
    let events = advance(&mut world, 60);
    assert!(world.bombs.is_empty());
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, GameplayEventKind::BombDetonated { .. }))
            .count(),
        2
    );
}

#[test]
fn outer_blast_moves_but_does_not_trigger_and_never_extends_a_fuse() {
    let mut world = arena();
    let x = 20.5 * TILE_SIZE;
    let y = 20.5 * TILE_SIZE;
    world.bombs = vec![
        bomb(0, x, y, 1),
        bomb(1, 3.0f32.mul_add(TILE_SIZE, x), y, 600),
        bomb(2, x + TILE_SIZE, y, 2),
    ];
    advance(&mut world, 1);
    assert!(!world.bombs[0].chain_triggered);
    assert!(world.bombs[0].velocity_x > 0.0);
    assert_eq!(world.bombs[0].detonation_tick.get(), 600);
    assert_eq!(world.bombs[1].detonation_tick.get(), 2);
}

#[test]
fn rock_occludes_impulses_before_the_explosion_removes_it() {
    let mut world = arena();
    let y = 20.5 * TILE_SIZE;
    for row in 8..32 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x: 21, y: row }, TileKind::Stone);
    }
    world.bombs = vec![
        bomb(0, 20.5 * TILE_SIZE, y, 1),
        bomb(1, 22.5 * TILE_SIZE, y, 600),
    ];
    advance(&mut world, 1);
    assert!(world.bombs[0].velocity_x.abs() < f32::EPSILON);
    assert_eq!(world.bombs[0].detonation_tick.get(), 600);
    assert!(!world.bombs[0].chain_triggered);
}

#[test]
fn simultaneous_blasts_have_identical_results_regardless_of_storage_order() {
    let mut first = arena();
    let x = 20.5 * TILE_SIZE;
    let y = 20.5 * TILE_SIZE;
    first.bombs = vec![
        bomb(0, x, y, 1),
        bomb(1, x + TILE_SIZE, y, 1),
        bomb(2, 2.0f32.mul_add(TILE_SIZE, x), y, 600),
    ];
    let mut second = first.clone();
    second.bombs.reverse();
    let first_events = advance(&mut first, 120);
    let second_events = advance(&mut second, 120);
    assert_eq!(first_events, second_events);
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
}

#[test]
fn in_flight_save_and_live_replication_preserve_the_entire_bomb_state() {
    let mut original = arena();
    original
        .bombs
        .push(bomb(8, 20.5 * TILE_SIZE, 20.5 * TILE_SIZE, 600));
    original.bombs[0].velocity_x = 120.0;
    original.next_bomb_id = 9;
    advance(&mut original, 20);
    let json = serde_json::to_string(&original).expect("save");
    let mut restored: WorldState = serde_json::from_str(&json).expect("load");
    let mut replica = restored.clone();
    replica.bombs.clear();
    replica.next_bomb_id = 0;
    replica.apply_live_entities(original.live_entities());
    assert_eq!(
        original.simulation_state_hash(),
        replica.simulation_state_hash()
    );
    advance(&mut original, 120);
    advance(&mut restored, 120);
    advance(&mut replica, 120);
    assert_eq!(
        original.simulation_state_hash(),
        restored.simulation_state_hash()
    );
    assert_eq!(
        original.simulation_state_hash(),
        replica.simulation_state_hash()
    );
}

#[test]
fn a_large_coincident_pile_produces_bounded_finite_deterministic_cascades() {
    let mut world = arena();
    for id in 0..u64::try_from(MAX_ACTIVE_BOMBS).unwrap() {
        world.bombs.push(bomb(
            id,
            20.5 * TILE_SIZE,
            20.5 * TILE_SIZE,
            if id == 0 { 1 } else { 600 },
        ));
    }
    let mut replay = world.clone();
    let events = advance(&mut world, 200);
    assert_eq!(events, advance(&mut replay, 200));
    assert_eq!(
        world.simulation_state_hash(),
        replay.simulation_state_hash()
    );
    assert!(world.bombs.is_empty());
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, GameplayEventKind::BombDetonated { .. }))
            .count(),
        MAX_ACTIVE_BOMBS
    );
}

#[test]
fn cap_does_not_consume_inventory_and_bomb_identity_does_not_reuse_removed_ids() {
    let mut world = arena();
    world.player_mut(LOCAL_PLAYER_ID).unwrap().bombs = 10;
    world.next_bomb_id = 42;
    let inputs = TickInputs::default().act(LOCAL_PLAYER_ID, PlayerCommand::PlaceBomb);
    let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    assert_eq!(world.bombs[0].id, 42);
    world.bombs.clear();
    let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    assert_eq!(world.bombs[0].id, 43);
    while world.bombs.len() < MAX_ACTIVE_BOMBS {
        world.bombs.push(bomb(
            u64::try_from(world.bombs.len()).unwrap() + 100,
            20.5 * TILE_SIZE,
            20.5 * TILE_SIZE,
            10_000,
        ));
    }
    let before = world.player(LOCAL_PLAYER_ID).unwrap().bombs;
    let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    assert_eq!(world.player(LOCAL_PLAYER_ID).unwrap().bombs, before);
}
