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
    bytes[start + seed + 3] = 9;

    let mut client: ClientSession<WorldState> = ClientSession::new(ClientConfig::default());
    assert!(client.handle_server_bytes(&bytes).is_err());
    assert!(client.authority().is_none());

    // The untouched message decodes.
    bytes[start + seed + 3] = 1;
    assert!(protocol::decode::<ServerMessage<WorldState>>(&bytes).is_ok());
}
