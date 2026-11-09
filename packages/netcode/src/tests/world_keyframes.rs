//! Compact keyframes of the real world: the terrain travels as generation settings plus the
//! tiles that differ from generation, and the client regenerates the rest (plan §4.6).

use std::time::Duration;

use drillgame_simulation::{
    PlayerId as SimPlayerId, WorldState,
    terrain::{ChunkPosition, FullTerrain, Terrain, TilePosition},
    worldgen::{WorldSettings, WorldSize},
};

use super::support::assert_converged;
use crate::harness::{Harness, HarnessConfig};
use crate::protocol::{self, ServerMessage};
use crate::{
    ClientConfig, ClientSession, LinkConditions, NetSimulation, SessionEpoch, SessionParams, Tick,
};

/// Blasts craters and cuts tunnels across the whole terrain.
fn excavate(terrain: &mut Terrain) {
    let (width, height) = (terrain.width(), terrain.height());
    for step in 0..(width * height / 60) {
        let x = (step * 37).rem_euclid(width);
        let y = 6 + (step * 53).rem_euclid(height - 6);
        let _ = terrain.blast_radius(TilePosition { x, y }, 2);
        let _ = terrain.chip(TilePosition { x: x + 3, y });
    }
}

/// Encoded size of a world's keyframe, and what it was when the terrain carried every tile.
fn keyframe_sizes(world: &WorldState) -> (usize, usize) {
    let keyframe = postcard::to_allocvec(world).expect("encode").len();
    let compact = postcard::to_allocvec(world.terrain())
        .expect("encode")
        .len();
    let full = postcard::to_allocvec(&FullTerrain::from(world.terrain()))
        .expect("encode")
        .len();
    (keyframe, keyframe - compact + full)
}

#[test]
fn heavily_dug_world_converges_on_epoch_change_join_and_reconnect() {
    let link = LinkConditions::new(20, 3, 0.5);
    let mut harness =
        Harness::<WorldState>::new(HarnessConfig::new(SessionParams::new(60), 2, link));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());

    // Load a heavily dug copy of the host's world: every client gets a compact keyframe.
    let mut dug = harness.world().clone();
    excavate(dug.terrain_mut());
    let diffs = dug.terrain().generation_diffs().len();
    assert!(diffs > 1_000, "heavily dug: {diffs} diffs");
    harness.host.new_epoch(dug);
    harness.run_for(Duration::from_secs(3));

    // Join in progress: the welcome keyframe carries the dug terrain.
    let late = harness.add_client(None);
    harness.run_for(Duration::from_secs(3));
    assert!(harness.clients[late].session.player().is_some());

    // Reconnect: the dropped client's player comes back with a fresh keyframe.
    let token = harness.clients[0]
        .session
        .reconnect_token()
        .expect("reconnect token");
    harness.disconnect(0);
    harness.run_for(Duration::from_secs(1));
    let back = harness.add_client(Some(token));
    harness.run_for(Duration::from_secs(3));
    harness.settle(Duration::from_secs(1));
    assert!(harness.clients[back].session.player().is_some());

    assert_converged(&harness);
    for index in [1, late, back] {
        let client = &harness.clients[index];
        assert!(
            client.hash_compares > 0,
            "client {index} compared its authority against the host"
        );
        assert_eq!(client.hash_mismatches, 0, "client {index}");
    }
}

#[test]
fn test_mode_world_converges_with_toggles_warps_and_late_joiners() {
    use drillgame_simulation::{
        InputSequence, PlayerCommand, PlayerTickInput, TestModeAction, TickAction, TickInputs,
        terrain::TILE_SIZE, worldgen::Band,
    };

    let link = LinkConditions::new(40, 5, 1.0);
    let mut harness =
        Harness::<WorldState>::new(HarnessConfig::new(SessionParams::new(60), 2, link));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());

    // The host switches to a test-mode copy of its world: toggles on, one rig warped to the grey stone (the harness world is shallow).
    let mut test = harness.world().clone();
    test.enable_test_mode();
    let warped = test.player_ids().next().expect("a rig");
    let actions = [
        TestModeAction::SetInvulnerable(true),
        TestModeAction::SetInfiniteFuel(true),
        TestModeAction::WarpToBand(Band::Stone),
    ];
    let mut inputs = TickInputs::default();
    inputs.players.insert(
        warped,
        PlayerTickInput {
            controls: None,
            actions: actions
                .iter()
                .zip(0..)
                .map(|(action, sequence)| TickAction {
                    sequence: InputSequence::new(sequence),
                    command: PlayerCommand::TestMode(*action),
                })
                .collect(),
        },
    );
    let tick = test.simulation_tick();
    let _ = test.simulate_tick(tick, &inputs);
    let rig = test.player(warped).expect("warped rig");
    assert_eq!(
        test.terrain().band_at(rig.tile_position(TILE_SIZE)),
        Band::Stone
    );
    harness.host.new_epoch(test);
    harness.run_for(Duration::from_secs(3));

    // A late joiner gets the test-mode section and a maxed rig from the template.
    let late = harness.add_client(None);
    harness.run_for(Duration::from_secs(3));
    harness.settle(Duration::from_secs(1));
    assert!(harness.clients[late].session.player().is_some());

    assert_converged(&harness);
    let host_mode = harness.world().test_mode();
    assert!(host_mode.is_some_and(|mode| mode.invulnerable && mode.infinite_fuel));
    for client in &harness.clients {
        let authority = client.session.authority().expect("in game");
        assert_eq!(authority.test_mode(), host_mode);
        let me = crate::world::to_sim_player(client.session.player().expect("joined"));
        assert_eq!(
            authority.player(me).expect("own rig").drill_mount_level,
            2,
            "every rig starts maxed in a test-mode world"
        );
    }
}

#[test]
fn keyframes_carry_only_tiles_that_differ_from_generation() {
    let mut report = Vec::new();
    for (label, settings) in [
        ("standard", WorldSettings::new(0x5EED)),
        (
            "large",
            WorldSettings {
                size: WorldSize::Large,
                ..WorldSettings::new(0x5EED)
            },
        ),
    ] {
        let fresh = WorldState::new_game_with_settings(
            SimPlayerId::new(1),
            &settings,
            drillgame_simulation::SimulationParams::default(),
        );
        let mut dug = fresh.clone();
        excavate(dug.terrain_mut());
        for (state, world) in [("fresh", &fresh), ("heavily dug", &dug)] {
            let (after, before) = keyframe_sizes(world);
            report.push(format!(
                "keyframe {label} {state}: {before} -> {after} bytes ({} tile diffs)",
                world.terrain().generation_diffs().len()
            ));
            assert!(after * 4 < before, "{label} {state}: {after} vs {before}");
            let decoded: WorldState = postcard::from_bytes(
                &postcard::to_allocvec(&world.keyframe_for(None)).expect("encode"),
            )
            .expect("decode");
            assert_eq!(
                decoded.visible_state_hash(None),
                world.visible_state_hash(None)
            );
        }

        // A chunk refresh carries the chunk's diffs, not its 256 tiles.
        let chunk = ChunkPosition { x: 3, y: 4 };
        let diffs = postcard::to_allocvec(&dug.terrain_chunk(chunk))
            .expect("encode")
            .len();
        let tiles = postcard::to_allocvec(&dug.terrain().chunk_tiles(chunk))
            .expect("encode")
            .len();
        report.push(format!(
            "chunk refresh {label}: {tiles} -> {diffs} bytes (dug), {} bytes (untouched)",
            postcard::to_allocvec(&fresh.terrain_chunk(chunk))
                .expect("encode")
                .len()
        ));
        assert!(diffs < tiles);
    }
    println!("{}", report.join("\n"));
}

#[test]
fn a_keyframe_from_an_unknown_generator_is_a_codec_error() {
    let world = WorldState::new_game(SimPlayerId::new(1));
    let message: ServerMessage<WorldState> = ServerMessage::Keyframe {
        epoch: SessionEpoch(1),
        tick: Tick(0),
        world: Box::new(world.clone()),
    };
    let mut bytes = protocol::encode(&message).expect("encode");
    let terrain = postcard::to_allocvec(world.terrain()).expect("encode terrain");
    let start = bytes
        .windows(terrain.len())
        .position(|window| window == terrain.as_slice())
        .expect("the keyframe holds the terrain snapshot");
    // The snapshot starts with the settings: the seed varint, then one byte per enum
    // (size, richness, hazards, generator).
    let seed = postcard::to_allocvec(&world.world_settings().seed)
        .expect("encode seed")
        .len();
    let generator = start + seed + 3;
    let original = bytes[generator];
    bytes[generator] = 9;

    let mut client: ClientSession<WorldState> = ClientSession::new(ClientConfig::default());
    assert!(client.handle_server_bytes(&bytes).is_err());
    assert!(client.authority().is_none());

    // The untouched message decodes.
    bytes[generator] = original;
    assert!(protocol::decode::<ServerMessage<WorldState>>(&bytes).is_ok());
}
