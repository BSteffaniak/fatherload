//! Terrain op relevance: with a finite relevance radius, ops in far chunks are withheld and the
//! chunks are refreshed whole once the client comes near, so its terrain converges.

use std::time::Duration;

use fatherload_simulation::{
    ControlState, WorldState,
    terrain::{ChunkPosition, TILE_SIZE},
};

use crate::protocol::{self, ServerMessage};
use crate::toy::{ToyControls, ToySim};
use crate::{
    ClientConfig, ClientId, ClientPhase, ClientSession, HostConfig, HostSession, NetSimulation,
    NetTuning, PlayerId, SeatId, SessionParams,
};

/// One host and one directly connected client; every message crosses the codec.
struct Pair<S: NetSimulation> {
    host: HostSession<S>,
    client: ClientSession<S>,
    id: ClientId,
    dt: Duration,
}

impl<S: NetSimulation> Pair<S> {
    /// Builds the host, lets `setup` add local seats, then connects the client.
    fn new(
        world: S,
        tuning: NetTuning,
        params: SessionParams,
        setup: impl FnOnce(&mut HostSession<S>),
    ) -> Self {
        let mut host = HostSession::new(
            world,
            HostConfig {
                params,
                tuning,
                ..HostConfig::default()
            },
        );
        setup(&mut host);
        let mut client = ClientSession::new(ClientConfig::default());
        let id = ClientId(1);
        host.client_connected(id);
        for message in client.take_outbound() {
            host.handle_client_message(id, message);
        }
        Self {
            host,
            client,
            id,
            dt: params.tick_duration(),
        }
    }

    fn step(&mut self, controls: &S::Controls) {
        let output = self.host.advance(self.dt);
        for (client, message) in output.outbound {
            if client == self.id {
                let bytes = protocol::encode(&message).expect("encode");
                let message: ServerMessage<S> = protocol::decode(&bytes).expect("decode");
                self.client.handle_server_message(message);
            }
        }
        for message in self.client.advance(self.dt, controls, Vec::new()) {
            let bytes = protocol::encode(&message).expect("encode");
            self.host
                .handle_client_bytes(self.id, &bytes)
                .expect("host decodes");
        }
    }

    fn authority(&self) -> &S {
        self.client.authority().expect("joined")
    }

    fn me(&self) -> PlayerId {
        self.client.player().expect("joined")
    }

    fn chunk_matches(&self, chunk: &S::Chunk) -> bool
    where
        S::ChunkSnapshot: PartialEq,
    {
        self.host.world().chunk_snapshot(chunk) == self.authority().chunk_snapshot(chunk)
    }

    fn relevance(&self) -> crate::Relevance {
        self.host.relevance_for(self.id).expect("client")
    }

    fn set_radius(&mut self, radius: Option<f32>) {
        let mut tuning = *self.client.tuning();
        tuning.relevance_radius = radius;
        self.host.set_client_tuning(self.id, tuning);
    }
}

fn finite_tuning(radius: f32) -> NetTuning {
    NetTuning {
        relevance_radius: Some(radius),
        ..NetTuning::default()
    }
}

#[test]
fn toy_client_converges_on_far_terrain_when_it_moves_close() {
    let params = SessionParams::new(60);
    // Four player columns, eight tiles each; the client spawns in the last one.
    let world = ToySim::new(&params, 4, 32);
    let mut pair = Pair::new(world, finite_tuning(6.0), params, |host| {
        for seat in 0..3 {
            let _ = host.add_local_player(SeatId(seat));
        }
    });
    let digger = PlayerId(0);
    let neutral = ToyControls::default();
    for _ in 0..30 {
        pair.step(&neutral);
    }
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    assert_eq!(pair.me(), PlayerId(3));

    // The digger digs straight down in column 0 for a while.
    let dig = ToyControls {
        move_x: 0,
        move_y: 1,
        drill: true,
    };
    pair.host.submit_local_input(digger, dig, Vec::new());
    for _ in 0..120 {
        pair.step(&neutral);
    }
    pair.host
        .submit_local_input(digger, ToyControls::default(), Vec::new());
    let dug: Vec<(i32, i32)> = (0..4)
        .map(|y| (0, y))
        .filter(|chunk| !pair.chunk_matches(chunk))
        .collect();
    assert!(
        !dug.is_empty(),
        "the far client skipped the digger's terrain ops"
    );

    // Walk left until every dug chunk is relevant, then the terrain matches the host.
    let walk = ToyControls {
        move_x: -1,
        move_y: 0,
        drill: false,
    };
    let mut steps = 0;
    while dug
        .iter()
        .any(|chunk| !pair.host.world().chunk_relevant(chunk, &pair.relevance()))
    {
        pair.step(&walk);
        steps += 1;
        assert!(steps < 60 * 20, "the client reached the dug column");
    }
    for _ in 0..10 {
        pair.step(&neutral);
    }
    for chunk in &dug {
        assert!(pair.chunk_matches(chunk), "chunk {chunk:?} refreshed");
    }
}

#[test]
fn world_client_converges_on_far_terrain_when_it_moves_close() {
    let params = SessionParams::new(60);
    let sim_digger = fatherload_simulation::PlayerId::new(0);
    let mut world = WorldState::new_game_with_params(
        sim_digger,
        fatherload_simulation::SimulationParams::new(params.tick_rate_hz),
    );
    // The digger works 22 tiles left of where new rigs spawn (clear of building foundations).
    let spawn_x = 62.5 * TILE_SIZE;
    let mut template = world.player_template().clone();
    template.x = spawn_x;
    world.set_player_template(template);
    world.player_mut(sim_digger).expect("digger").x = 40.5 * TILE_SIZE;
    let radius_tiles = 8.0;
    let digger = PlayerId(0);
    let mut pair = Pair::new(world, finite_tuning(radius_tiles), params, |host| {
        assert!(host.adopt_local_player(SeatId(0), digger));
    });
    let idle = ControlState::default();
    for _ in 0..30 {
        pair.step(&idle);
    }
    assert_eq!(pair.client.phase(), ClientPhase::InGame);

    pair.host
        .submit_local_input(digger, ControlState::new(0.0, false, true), Vec::new());
    for _ in 0..60 * 6 {
        pair.step(&idle);
    }
    pair.host.submit_local_input(digger, idle, Vec::new());
    let dig_chunk = ChunkPosition { x: 2, y: 0 };
    assert!(
        !pair
            .host
            .world()
            .chunk_relevant(&dig_chunk, &pair.relevance()),
        "the dug chunk starts out of relevance"
    );
    assert!(
        !pair.chunk_matches(&dig_chunk),
        "the far client skipped the digger's terrain ops"
    );

    let walk = ControlState::new(-1.0, false, false);
    let mut steps = 0;
    while !pair
        .host
        .world()
        .chunk_relevant(&dig_chunk, &pair.relevance())
    {
        pair.step(&walk);
        steps += 1;
        assert!(steps < 60 * 30, "the client walked towards the digger");
    }
    for _ in 0..10 {
        pair.step(&idle);
    }
    assert!(
        pair.chunk_matches(&dig_chunk),
        "the approached chunk was refreshed"
    );

    // Back to the default infinite radius: every withheld chunk is refreshed.
    pair.set_radius(None);
    for _ in 0..10 {
        pair.step(&idle);
    }
    assert_eq!(
        pair.authority().terrain().digest(),
        pair.host.world().terrain().digest(),
        "terrain fully converged"
    );
}

#[test]
fn world_entities_spawn_change_and_despawn_per_client_relevance() {
    use fatherload_simulation::{
        item::{ItemKind, ItemStack},
        terrain::MineralKind,
    };
    let params = SessionParams::new(60);
    let sim_host = fatherload_simulation::PlayerId::new(0);
    let mut world = WorldState::new_game_with_params(
        sim_host,
        fatherload_simulation::SimulationParams::new(params.tick_rate_hz),
    );
    let spawn_x = 62.5 * TILE_SIZE;
    let mut template = world.player_template().clone();
    template.x = spawn_x;
    world.set_player_template(template);
    world.player_mut(sim_host).expect("host rig").x = 20.5 * TILE_SIZE;
    let gold = ItemStack::new(ItemKind::Mineral(MineralKind::Gold), 2);
    // Two drops falling near the joining client, one short-lived; one far away by the host.
    // All of them out of the rigs' magnet reach (WP-DROPS), but inside the client's relevance.
    let near = world.spawn_item_drop(
        5.0f32.mul_add(TILE_SIZE, spawn_x),
        2.0 * TILE_SIZE,
        (30.0, 0.0),
        gold,
        None,
    );
    let fleeting = world.spawn_item_drop(
        5.0f32.mul_add(-TILE_SIZE, spawn_x),
        2.0 * TILE_SIZE,
        (0.0, 0.0),
        gold,
        Some(2_000),
    );
    let far = world.spawn_item_drop(25.5 * TILE_SIZE, 2.0 * TILE_SIZE, (0.0, 0.0), gold, None);
    let mut pair = Pair::new(world, finite_tuning(8.0), params, |host| {
        assert!(host.adopt_local_player(SeatId(0), PlayerId(0)));
    });
    let idle = ControlState::default();
    for _ in 0..60 {
        pair.step(&idle);
    }
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    let held = |pair: &Pair<WorldState>| {
        pair.authority()
            .entities()
            .iter()
            .map(|entity| entity.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        held(&pair),
        vec![near, fleeting],
        "only nearby entities, the far one despawned"
    );
    for _ in 0..60 {
        pair.step(&idle);
    }
    // The near drop landed and fell asleep; the client holds exactly the host's record.
    let host_near = pair
        .host
        .world()
        .entities()
        .get(near)
        .cloned()
        .expect("near");
    assert!(host_near.body.asleep, "the drop came to rest");
    assert_eq!(pair.authority().entities().get(near), Some(&host_near));
    // The short-lived drop expires on the host and the despawn reaches the client.
    for _ in 0..60 {
        pair.step(&idle);
    }
    assert!(!pair.host.world().entities().contains(fleeting));
    assert_eq!(held(&pair), vec![near]);
    // With unlimited relevance the client converges on the whole store.
    pair.set_radius(None);
    for _ in 0..5 {
        pair.step(&idle);
    }
    assert_eq!(held(&pair), vec![near, far]);
    assert_eq!(pair.authority().entities(), pair.host.world().entities());
}

/// R17, no flight ceiling: a client's rig ten thousand tiles above a biome world's surface
/// keeps climbing under its own thrust, its prediction agrees with the host, nothing streams
/// for the empty sky, and the terrain stays converged.
#[test]
fn rig_far_above_the_surface_flies_online() {
    let params = SessionParams::new(60);
    let sim_host = fatherload_simulation::PlayerId::new(0);
    let mut world = WorldState::new_game_with_params(
        sim_host,
        fatherload_simulation::SimulationParams::new(params.tick_rate_hz),
    );
    assert!(world.world_settings().generator.has_biomes());
    // New rigs (the client's) spawn ten thousand tiles up.
    let altitude = 10_000.0 * TILE_SIZE;
    let mut template = world.player_template().clone();
    template.y = -altitude;
    world.set_player_template(template);
    let host_player = PlayerId(0);
    let mut pair = Pair::new(world, finite_tuning(12.0), params, |host| {
        assert!(host.adopt_local_player(SeatId(0), host_player));
    });
    let idle = ControlState::default();
    for _ in 0..30 {
        pair.step(&idle);
    }
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    let me = crate::world::to_sim_player(pair.me());
    let start = pair.host.world().player(me).expect("client rig").y;
    assert!(
        start < -altitude + 13.0 * TILE_SIZE,
        "spawned up high: {start}"
    );

    let climb = ControlState::new(0.3, true, false);
    for _ in 0..60 * 3 {
        pair.step(&climb);
    }
    for _ in 0..20 {
        pair.step(&idle);
    }
    let host_rig = pair.host.world().player(me).expect("host copy").clone();
    assert!(
        host_rig.y < start - 10.0 * TILE_SIZE,
        "the rig climbed past where any ceiling stood: {start} -> {}",
        host_rig.y
    );
    let authority = pair.authority().player(me).expect("client authority");
    assert!(
        (authority.y - host_rig.y).abs() < 0.01 && (authority.x - host_rig.x).abs() < 0.01,
        "the client agrees on its rig high in the sky"
    );
    let predicted = pair
        .client
        .predicted()
        .and_then(|world| world.player(me))
        .expect("prediction");
    assert!(
        (predicted.y - host_rig.y).abs() < TILE_SIZE,
        "prediction tracks the authority: {} vs {}",
        predicted.y,
        host_rig.y
    );
    assert_eq!(
        pair.authority().terrain().digest(),
        pair.host.world().terrain().digest(),
        "terrain converged"
    );
}

/// An endless (V5) world: a digger far out (thousands of tiles from spawn) works on chunks the
/// client never generated; the client only learns the diffs of chunks in its relevance, its
/// keyframe never carried generated terrain, and once relevance is lifted the far chunks'
/// diffs stream in and the terrain converges.
#[test]
fn streamed_world_streams_far_chunk_diffs_to_clients() {
    let params = SessionParams::new(60);
    let sim_digger = fatherload_simulation::PlayerId::new(0);
    let mut world = WorldState::new_game_with_params(
        sim_digger,
        fatherload_simulation::SimulationParams::new(params.tick_rate_hz),
    );
    assert!(world.terrain().is_streamed());
    let far_column: i16 = -3_000;
    {
        let digger = world.player_mut(sim_digger).expect("digger");
        digger.x = (f32::from(far_column) + 0.5) * TILE_SIZE;
        digger.y = 4.5 * TILE_SIZE;
        digger.velocity_x = 0.0;
        digger.velocity_y = 0.0;
    }
    let keyframe_bytes = postcard::to_allocvec(world.terrain())
        .expect("encode")
        .len();
    assert!(
        keyframe_bytes < 256,
        "an untouched endless world's terrain keyframe is {keyframe_bytes} bytes"
    );
    let digger = PlayerId(0);
    let mut pair = Pair::new(world, finite_tuning(8.0), params, |host| {
        assert!(host.adopt_local_player(SeatId(0), digger));
    });
    let idle = ControlState::default();
    for _ in 0..30 {
        pair.step(&idle);
    }
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    pair.host
        .submit_local_input(digger, ControlState::new(0.0, false, true), Vec::new());
    for _ in 0..60 * 6 {
        pair.step(&idle);
    }
    pair.host.submit_local_input(digger, idle, Vec::new());
    for _ in 0..10 {
        pair.step(&idle);
    }
    let dug: Vec<ChunkPosition> = pair.host.world().terrain().edited_chunks().collect();
    assert!(
        dug.iter()
            .any(|chunk| chunk.x * 16 < i32::from(far_column) + 32),
        "the digger dug far out: {dug:?}"
    );
    assert!(
        dug.iter().any(|chunk| !pair.chunk_matches(chunk)),
        "the client skipped the far chunks' diffs"
    );
    pair.set_radius(None);
    for _ in 0..10 {
        pair.step(&idle);
    }
    for chunk in &dug {
        assert!(pair.chunk_matches(chunk), "chunk {chunk:?} streamed in");
    }
    assert_eq!(
        pair.authority().terrain().digest(),
        pair.host.world().terrain().digest(),
        "terrain fully converged"
    );
}
