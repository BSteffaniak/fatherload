//! The compact world form (settings + tile diffs + everything else) round-trips the whole world.

use super::determinism_tests::{scripted_inputs, scripted_world};
use crate::{PlayerId, SimulationTick, WorldState, terrain::Terrain};

/// A world after a long scripted run: drilling, bombs, scanning, infrastructure, trades.
fn played_world(ticks: u64) -> WorldState {
    let mut world = scripted_world();
    for tick in 0..ticks {
        let _output = world.simulate_tick(SimulationTick::new(tick), &scripted_inputs(tick));
    }
    world
}

#[test]
fn a_played_world_round_trips_through_its_compact_form() {
    let world = played_world(6_000);
    assert!(!world.infrastructure().is_empty(), "infrastructure placed");
    let diffs = world.terrain().generation_diffs().len();
    assert!(diffs > 50, "the run dug terrain: {diffs} diffs");

    let json = serde_json::to_string(&world).expect("json");
    let from_json: WorldState = serde_json::from_str(&json).expect("json restores");
    assert_eq!(
        from_json.simulation_state_hash(),
        world.simulation_state_hash()
    );

    let bytes = postcard::to_allocvec(&world).expect("postcard");
    let from_bytes: WorldState = postcard::from_bytes(&bytes).expect("postcard restores");
    assert_eq!(
        from_bytes.simulation_state_hash(),
        world.simulation_state_hash()
    );

    // The restored world keeps playing identically.
    let mut original = world;
    let mut restored = from_bytes;
    for tick in 6_000..6_600 {
        let inputs = scripted_inputs(tick);
        let a = original.simulate_tick(SimulationTick::new(tick), &inputs);
        let b = restored.simulate_tick(SimulationTick::new(tick), &inputs);
        assert_eq!(a, b);
    }
    assert_eq!(
        restored.simulation_state_hash(),
        original.simulation_state_hash()
    );

    // Redacted keyframes restore to the same client-visible state.
    let viewer = Some(PlayerId::new(2));
    let keyframe = original.redacted_for(viewer);
    let decoded: WorldState =
        postcard::from_bytes(&postcard::to_allocvec(&keyframe).expect("encode")).expect("decode");
    assert_eq!(
        decoded.visible_state_hash(viewer),
        original.visible_state_hash(viewer)
    );
}

#[test]
fn a_fresh_world_keyframe_carries_no_tiles() {
    let world = WorldState::new_game(PlayerId::new(1));
    assert!(world.terrain().generation_diffs().is_empty());
    let bytes = postcard::to_allocvec(world.terrain()).expect("encode");
    assert!(bytes.len() < 64, "{} bytes", bytes.len());
    let terrain: Terrain = postcard::from_bytes(&bytes).expect("decode");
    assert_eq!(terrain.digest(), world.terrain().digest());
}
