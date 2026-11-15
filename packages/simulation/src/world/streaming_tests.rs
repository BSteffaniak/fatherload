//! Streamed (V5) worlds in play: travel far from spawn without precision drift, chunks loading
//! and unloading around rigs with bounded memory, and edits that survive unloading.

use crate::{
    ControlState, PlayerId, SimulationParams, TickInputs, WorldState,
    terrain::{STREAM_KEEP_RADIUS_CHUNKS, TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::{WorldSettings, streamed::STREAMED_COLUMN_LIMIT},
};

const PLAYER: PlayerId = PlayerId::new(1);
/// Row of the corridor floor the rig drives along.
const FLOOR_ROW: i32 = 40;

fn streamed_world() -> WorldState {
    let world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(0x0005_7EA3),
        SimulationParams::new(60),
    );
    assert!(world.terrain().is_streamed());
    world
}

/// Carves a corridor three rows tall from `start` for `length` columns over a dirt floor, with a
/// dirt wall at its end, and parks the rig at its start.
fn corridor(world: &mut WorldState, start: i32, length: i32) {
    for x in start - 2..=start + length {
        for y in FLOOR_ROW - 3..=FLOOR_ROW {
            let kind = if y == FLOOR_ROW || x == start - 2 || x == start + length {
                TileKind::Dirt
            } else {
                TileKind::Air
            };
            assert!(world.terrain_mut().set_kind(TilePosition { x, y }, kind));
        }
    }
    let rig = world.player_mut(PLAYER).expect("rig");
    #[allow(
        clippy::cast_precision_loss,
        reason = "tile coordinates within the world"
    )]
    {
        rig.x = (start as f32).mul_add(TILE_SIZE, TILE_SIZE * 0.5);
        rig.y = ((FLOOR_ROW - 1) as f32).mul_add(TILE_SIZE, TILE_SIZE * 0.5);
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.x_remainder = 0.0;
    rig.fuel = 5_000.0;
    rig.fuel_capacity = 5_000.0;
}

fn run(world: &mut WorldState, ticks: u32, horizontal: f32) {
    let inputs = TickInputs::held(PLAYER, ControlState::new(horizontal, false, false));
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
    }
}

/// Drives the rig along a corridor at `start`: how far it got in a second, where it rests
/// against the end wall (relative to the corridor start) and whether it then stays put.
fn drive(world: &mut WorldState, start: i32) -> (f32, f32) {
    corridor(world, start, 40);
    run(world, 30, 0.0);
    #[allow(
        clippy::cast_precision_loss,
        reason = "tile coordinates within the world"
    )]
    let origin = start as f32 * TILE_SIZE;
    let before = world.player(PLAYER).expect("rig").x;
    run(world, 60, 1.0);
    let moved = world.player(PLAYER).expect("rig").x - before;
    run(world, 600, 1.0);
    run(world, 120, 0.0);
    let rest = world.player(PLAYER).expect("rig").x;
    run(world, 300, 0.0);
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        (rig.x - rest).abs() < f32::EPSILON,
        "the rig creeps at column {start}"
    );
    assert_eq!(rig.tile_position(TILE_SIZE).y, FLOOR_ROW - 1);
    (moved, rest - origin)
}

#[test]
fn travel_fifty_thousand_tiles_each_way_without_drift() {
    let mut world = streamed_world();
    let (near_moved, near_rest) = drive(&mut world, 1_000);
    assert!(near_moved > TILE_SIZE, "the rig drives ({near_moved} px)");
    for start in [50_000, -50_040] {
        let (moved, rest) = drive(&mut world, start);
        // f32 world pixels only resolve 1/8 px out here, but movement keeps the rest of the
        // exact position in the rig's remainder: it drives exactly as far as near spawn (to
        // within the f32 spacing of the stored position) and stops at the same offset.
        assert!(
            (moved - near_moved).abs() <= 0.25,
            "at column {start} the rig drove {moved} px, near spawn {near_moved} px"
        );
        assert!(
            (rest - near_rest).abs() <= 0.25,
            "at column {start} the rig rests at {rest} px, near spawn {near_rest} px"
        );
        let rig = world.player(PLAYER).expect("rig");
        #[allow(clippy::cast_possible_truncation, reason = "in-world tile coordinates")]
        let column = (rig.x / TILE_SIZE).floor() as i32;
        assert_eq!(rig.tile_position(TILE_SIZE).x, column);
    }
}

#[test]
fn chunks_stream_around_rigs_with_bounded_memory_and_edits_survive_unloading() {
    let mut world = streamed_world();
    let far = TilePosition { x: 20_000, y: 300 };
    let (keep_x, keep_y) = STREAM_KEEP_RADIUS_CHUNKS;
    let bound = usize::try_from((2 * keep_x + 1) * (2 * keep_y + 1)).expect("small");
    let mut most_loaded = 0;
    // Hop a long way in steps; every stop streams the chunks around the rig in.
    for step in 0..12 {
        let x = -60_000 + step * 10_000;
        corridor(&mut world, x, 8);
        run(&mut world, 2, 0.0);
        most_loaded = most_loaded.max(world.terrain().loaded_chunk_count());
        let rig = world.player(PLAYER).expect("rig").tile_position(TILE_SIZE);
        let chunk = crate::terrain::ChunkPosition::of(rig);
        assert!(
            world
                .terrain()
                .loaded_chunks()
                .any(|loaded| loaded == chunk)
        );
    }
    assert!(
        most_loaded <= bound,
        "{most_loaded} chunks loaded (bound {bound})"
    );
    // Dig far away, leave, come back: the hole is still there although its chunk unloaded.
    let dug = if world.terrain().tile(far).map(|tile| tile.kind) == Some(TileKind::Air) {
        TileKind::Dirt
    } else {
        TileKind::Air
    };
    assert!(world.terrain_mut().set_kind(far, dug));
    corridor(&mut world, -1_000, 8);
    run(&mut world, 2, 0.0);
    let far_chunk = crate::terrain::ChunkPosition::of(far);
    assert!(
        world
            .terrain()
            .loaded_chunks()
            .all(|chunk| chunk != far_chunk)
    );
    assert!(
        world
            .terrain()
            .edited_chunks()
            .any(|chunk| chunk == far_chunk)
    );
    assert_eq!(world.terrain().tile(far).map(|tile| tile.kind), Some(dug));
    // Which chunks are loaded never shows in the state hash.
    let mut unloaded = world.clone();
    unloaded.terrain_mut().stream_around(&[]);
    assert_eq!(unloaded.terrain().loaded_chunk_count(), 0);
    assert_eq!(
        unloaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
}

#[test]
fn streamed_terrain_snapshots_hold_only_edits_and_reject_other_generators() {
    let settings = WorldSettings::new(0x0005_5A9E);
    let mut terrain = Terrain::generate(&settings);
    for (x, y) in [
        (-50_000, 200),
        (3, 60),
        (77_777, 500),
        (STREAMED_COLUMN_LIMIT - 1, 10),
    ] {
        assert!(terrain.set_kind(TilePosition { x, y }, TileKind::Air));
        assert!(terrain.set_kind(TilePosition { x: x - 1, y }, TileKind::Stone) || x < 0);
    }
    assert!(!terrain.set_kind(
        TilePosition {
            x: STREAMED_COLUMN_LIMIT,
            y: 10
        },
        TileKind::Air
    ));
    let snapshot = terrain.snapshot();
    assert!(snapshot.diffs.is_empty());
    assert!(snapshot.chunks.len() >= 3);
    let restored = Terrain::from_snapshot(snapshot.clone()).expect("restores");
    assert_eq!(restored.digest(), terrain.digest());
    assert_eq!(restored.generation_diffs(), terrain.generation_diffs());
    // A different generator build (simulated by another seed) fails the probe.
    let foreign = crate::terrain::TerrainSnapshot {
        settings: WorldSettings::new(0x0005_5A9F),
        ..snapshot
    };
    assert!(Terrain::from_snapshot(foreign).is_err());
    // Writing a tile back to its generated value drops the edit.
    let position = TilePosition { x: 3, y: 60 };
    let generated = Terrain::generate(&settings).tile(position).expect("tile");
    terrain.set_tile_from_network(position, generated.kind, generated.durability);
    assert!(
        terrain
            .generation_diffs()
            .iter()
            .all(|change| change.position != position)
    );
    // Round trip through the binary keyframe codec too.
    let bytes = postcard::to_allocvec(&terrain).expect("encode");
    let decoded: Terrain = postcard::from_bytes(&bytes).expect("decode");
    assert_eq!(decoded.digest(), terrain.digest());
    assert!(
        bytes.len() < 4_096,
        "keyframe terrain is {} bytes",
        bytes.len()
    );
}

/// Flies a rig continuously 50,000 tiles out each way (two tiles a tick, a fast god-mode
/// pace), streaming chunks every tick: prints the mean, 99th-percentile and worst tick times
/// and checks memory stays bounded the whole way. Run in release:
/// `cargo test --release -p fatherload_simulation long_flight -- --ignored --nocapture`.
#[test]
#[ignore = "timing helper: tens of thousands of ticks; run in release"]
fn long_flight_keeps_tick_time_and_memory_bounded() {
    let mut world = streamed_world();
    let (keep_x, keep_y) = STREAM_KEEP_RADIUS_CHUNKS;
    let bound = usize::try_from((2 * keep_x + 1) * (2 * keep_y + 1)).expect("small");
    let step = 2.0 * TILE_SIZE;
    let ticks = 25_000_usize;
    let start_x = world.player(PLAYER).expect("rig").x;
    // Through the sky, then through solid rock deep down (more chunk rows and every deep
    // region layer in the load window).
    for (direction, row) in [(1.0_f32, 2.5_f32), (-1.0, 2.5), (1.0, 400.5), (-1.0, 400.5)] {
        let mut times = Vec::with_capacity(ticks);
        let mut most_loaded = 0;
        for index in 0..ticks {
            {
                let rig = world.player_mut(PLAYER).expect("rig");
                #[allow(clippy::cast_precision_loss, reason = "tick counts are small")]
                let offset = direction * step * (index as f32);
                rig.x = start_x + offset;
                rig.x_remainder = 0.0;
                rig.y = row * TILE_SIZE;
                rig.velocity_x = 0.0;
                rig.velocity_y = 0.0;
                rig.fuel = rig.fuel_capacity;
                rig.hull = rig.max_hull();
            }
            let tick = world.simulation_tick();
            let begin = std::time::Instant::now();
            let _ = world.simulate_tick(tick, &TickInputs::default());
            times.push(begin.elapsed());
            most_loaded = most_loaded.max(world.terrain().loaded_chunk_count());
        }
        times.sort_unstable();
        let total: std::time::Duration = times.iter().sum();
        println!(
            "flight {direction:+} at row {row}: {ticks} ticks over {} tiles: mean {:?}, p99 {:?}, worst {:?}, \
             most chunks loaded {most_loaded}",
            ticks * 2,
            total / u32::try_from(ticks).expect("small"),
            times[ticks * 99 / 100],
            times[ticks - 1],
        );
        assert!(most_loaded <= bound, "{most_loaded} chunks (bound {bound})");
    }
    // What the trip costs a save: the terrain holds no edits, but the crew's discovery map
    // remembers every chunk they revealed.
    let json = serde_json::to_string(&world).expect("serialize");
    println!(
        "world JSON after 200,000 tiles of flight: {} bytes ({} edited chunks)",
        json.len(),
        world.terrain().edited_chunk_count()
    );
}
