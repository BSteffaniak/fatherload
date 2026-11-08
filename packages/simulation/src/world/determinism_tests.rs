//! Long-run determinism: identical seeds and input recordings must produce identical worlds.

use crate::{
    InputSequence, PlayerCommand, PlayerId, SequencedPlayerCommand, SimulationStateHash,
    SimulationTick, TerrainRevisionTracker, TickInputs, TickOutput, WorldState, player::Player,
    world::NEW_WORLD_SPAWN,
};

const FIRST_PLAYER: PlayerId = PlayerId::new(1);
const SECOND_PLAYER: PlayerId = PlayerId::new(2);
const RUN_TICKS: u64 = 10_000;
const HASH_EVERY_TICKS: u64 = 250;

pub(super) fn scripted_world() -> WorldState {
    let mut world = WorldState::new_game(FIRST_PLAYER);
    let mut second = Player::new(NEW_WORLD_SPAWN.0 + 96.0, NEW_WORLD_SPAWN.1);
    second.credits = 5_000;
    second.bombs = 40;
    second.signal_relay_kits = 4;
    second.pump_station_kits = 4;
    world.insert_player(SECOND_PLAYER, second);
    let first = world.player_mut(FIRST_PLAYER).expect("first player exists");
    first.credits = 5_000;
    first.bombs = 40;
    first.survey_drone_kits = 4;
    first.tunnel_support_kits = 4;
    world
}

/// Deterministic bot script covering movement, drilling, bombs, scanning, infrastructure,
/// economy transactions, and rescue for two rigs.
pub(super) fn scripted_commands(tick: u64, sequence: &mut u32) -> Vec<SequencedPlayerCommand> {
    let mut commands = Vec::new();
    for (index, player_id) in [FIRST_PLAYER, SECOND_PLAYER].into_iter().enumerate() {
        let phase = (tick + index as u64 * 137) % 600;
        let (horizontal, thrust, drill_down) = match phase {
            0..200 => (0.0, false, true),
            200..320 => (1.0, false, false),
            320..420 => (-0.6, true, false),
            420..520 => (-1.0, false, true),
            _ => (0.25, true, false),
        };
        let mut push = |command: PlayerCommand| {
            *sequence = sequence.wrapping_add(1);
            commands.push(SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(*sequence),
                target_tick: SimulationTick::new(tick),
                command,
            });
        };
        push(PlayerCommand::Movement {
            horizontal,
            thrust,
            drill_down,
        });
        let offset = tick + index as u64 * 13;
        if offset.is_multiple_of(97) {
            push(PlayerCommand::PlaceBomb);
        }
        if offset.is_multiple_of(131) {
            push(PlayerCommand::UseScanner);
        }
        if offset.is_multiple_of(409) {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "slot index is bounded to the six infrastructure kinds"
            )]
            push(PlayerCommand::PlaceInfrastructure {
                slot: (offset % 6) as u8,
            });
        }
        if offset.is_multiple_of(503) {
            push(PlayerCommand::SellCargo);
        }
        if offset.is_multiple_of(701) {
            push(PlayerCommand::Refuel);
            push(PlayerCommand::Repair);
        }
        if offset.is_multiple_of(997) {
            push(PlayerCommand::Rescue);
        }
        if offset.is_multiple_of(1_201) {
            push(PlayerCommand::BuyUpgrade { index: 0 });
        }
    }
    commands
}

struct RunRecord {
    checkpoints: Vec<(u64, SimulationStateHash)>,
    outputs: Vec<TickOutput>,
    final_world: WorldState,
}

fn run(mut world: WorldState, start_tick: u64, end_tick: u64) -> RunRecord {
    let mut revisions = TerrainRevisionTracker::default();
    let mut sequence = u32::try_from(start_tick)
        .unwrap_or(u32::MAX)
        .wrapping_mul(16);
    let mut checkpoints = Vec::new();
    let mut outputs = Vec::new();
    for tick in start_tick..end_tick {
        let inputs = TickInputs::from_commands(&scripted_commands(tick, &mut sequence));
        outputs.push(world.simulate_tick(SimulationTick::new(tick), &inputs, &mut revisions));
        if (tick + 1).is_multiple_of(HASH_EVERY_TICKS) {
            checkpoints.push((tick + 1, world.simulation_state_hash()));
        }
    }
    RunRecord {
        checkpoints,
        outputs,
        final_world: world,
    }
}

#[test]
fn ten_thousand_tick_scripted_run_is_deterministic() {
    let initial_hash = scripted_world().simulation_state_hash();
    let first = run(scripted_world(), 0, RUN_TICKS);
    let second = run(scripted_world(), 0, RUN_TICKS);

    assert_eq!(first.checkpoints, second.checkpoints);
    assert_eq!(first.outputs, second.outputs);
    assert_eq!(
        first.final_world.simulation_state_hash(),
        second.final_world.simulation_state_hash()
    );

    // The script must actually exercise the simulation, not idle.
    assert_ne!(first.final_world.simulation_state_hash(), initial_hash);
    let event_count: usize = first.outputs.iter().map(|output| output.events.len()).sum();
    let changed_tiles: usize = first
        .outputs
        .iter()
        .map(|output| output.terrain_changed_tiles.len())
        .sum();
    assert!(
        event_count > 100,
        "scripted run produced {event_count} events"
    );
    assert!(changed_tiles > 0, "scripted run never changed terrain");
    assert!(!first.final_world.service_transactions().is_empty());
}

#[test]
fn cloned_world_replays_identically_from_a_mid_run_snapshot() {
    let midpoint = RUN_TICKS / 2;
    let prefix = run(scripted_world(), 0, midpoint);
    let snapshot = prefix.final_world.clone();
    assert_eq!(
        snapshot.simulation_state_hash(),
        prefix.final_world.simulation_state_hash()
    );

    let continued = run(prefix.final_world, midpoint, midpoint + 2_000);
    let replayed = run(snapshot, midpoint, midpoint + 2_000);

    assert_eq!(continued.checkpoints, replayed.checkpoints);
    assert_eq!(continued.outputs, replayed.outputs);
}

#[test]
fn scripted_run_is_deterministic_at_other_tick_rates() {
    for hz in [30_u32, 120] {
        let world = || {
            let mut world = scripted_world();
            let mut fresh =
                WorldState::new_game_with_params(FIRST_PLAYER, crate::SimulationParams::new(hz));
            for (player_id, player) in world.players() {
                fresh.insert_player(*player_id, player.clone());
            }
            world = fresh;
            world
        };
        let first = run(world(), 0, 2_000);
        let second = run(world(), 0, 2_000);
        assert_eq!(first.checkpoints, second.checkpoints, "{hz} Hz");
        assert_eq!(first.outputs, second.outputs, "{hz} Hz");
        assert_eq!(first.final_world.tick_rate_hz(), hz);
    }
}
