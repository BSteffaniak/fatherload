//! Terrain op relevance: with a finite relevance radius, ops in far chunks are withheld and the
//! chunks are refreshed whole once the client comes near, so its terrain converges.

use std::time::Duration;

use drillgame_simulation::{
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
    let sim_digger = drillgame_simulation::PlayerId::new(0);
    let mut world = WorldState::new_game_with_params(
        sim_digger,
        drillgame_simulation::SimulationParams::new(params.tick_rate_hz),
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
