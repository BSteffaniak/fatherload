//! Many places at once online (Part III F11): a client whose player warps to a space body
//! receives that body's ground (chunks, ops, entities and frame) and none of the home world's
//! while it is away, its player's place replicates with its per-tick state, its authority keeps
//! matching the host's visible state, and coming home refreshes the home chunks it skipped.

use std::time::Duration;

use fatherload_simulation::{
    ControlState, PlayerCommand, TestModeAction, WorldState,
    enemy::EnemyDifficulty,
    place::{BodyId, Place, SpaceBodyKey, StoreKey},
    worldgen::{GeneratorVersion, WorldSettings},
};

use crate::protocol::{self, ServerMessage};
use crate::world::{StoreChunk, to_sim_player};
use crate::{
    ClientConfig, ClientId, ClientPhase, ClientSession, HostConfig, HostSession, NetSimulation,
    NetTuning, PlayerId, SeatId, SessionParams,
};

const BODY: SpaceBodyKey = SpaceBodyKey::new(4, 1, 2, 0);

/// One host (with one local seat) and one directly connected client.
struct Pair {
    host: HostSession<WorldState>,
    client: ClientSession<WorldState>,
    id: ClientId,
    dt: Duration,
    host_hashes: std::collections::BTreeMap<u64, u64>,
}

impl Pair {
    fn new(world: WorldState, tuning: NetTuning, params: SessionParams) -> Self {
        let mut host = HostSession::new(
            world,
            HostConfig {
                params,
                tuning,
                ..HostConfig::default()
            },
        );
        assert!(host.adopt_local_player(SeatId(0), PlayerId(0)));
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
            host_hashes: std::collections::BTreeMap::new(),
        }
    }

    fn step(&mut self, controls: &ControlState, actions: Vec<PlayerCommand>) {
        let output = self.host.advance(self.dt);
        if let Some(me) = self.client.player() {
            let hash = NetSimulation::visible_state_hash(self.host.world(), Some(me));
            self.host_hashes
                .insert(NetSimulation::tick(self.host.world()).0, hash);
        }
        for (client, message) in output.outbound {
            if client == self.id {
                let bytes = protocol::encode(&message).expect("encode");
                let message: ServerMessage<WorldState> = protocol::decode(&bytes).expect("decode");
                self.client.handle_server_message(message);
            }
        }
        for message in self.client.advance(self.dt, controls, actions) {
            let bytes = protocol::encode(&message).expect("encode");
            self.host
                .handle_client_bytes(self.id, &bytes)
                .expect("host decodes");
        }
    }

    fn run(&mut self, ticks: u32, controls: &ControlState) {
        for _ in 0..ticks {
            self.step(controls, Vec::new());
        }
    }

    fn authority(&self) -> &WorldState {
        self.client.authority().expect("joined")
    }

    fn me(&self) -> PlayerId {
        self.client.player().expect("joined")
    }

    /// Whether the client's authority hashes like the host's view for its player did at the
    /// same tick (the host's hashes are recorded every step).
    fn hashes_agree(&self) -> bool {
        let tick = NetSimulation::tick(self.authority());
        let me = Some(self.me());
        let Some(host) = self.host_hashes.get(&tick.0) else {
            return false;
        };
        *host == NetSimulation::visible_state_hash(self.authority(), me)
    }
}

fn warp(place: Place) -> Vec<PlayerCommand> {
    vec![PlayerCommand::TestMode(TestModeAction::WarpToPlace(place))]
}

/// A test-mode streamed world (quick drills), peaceful.
fn world(rate: u32) -> WorldState {
    let first = fatherload_simulation::PlayerId::new(0);
    let mut world = WorldState::new_game_with_settings(
        first,
        &WorldSettings {
            generator: GeneratorVersion::V5,
            ..WorldSettings::new(0x0F11_0001)
        },
        fatherload_simulation::SimulationParams::new(rate),
    );
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.enable_test_mode();
    world
}

#[test]
fn a_client_away_on_a_body_gets_that_body_and_not_home() {
    let params = SessionParams::new(60);
    let mut pair = Pair::new(world(60), NetTuning::default(), params);
    let idle = ControlState::default();
    let dig = ControlState::new(0.0, false, true);
    pair.run(30, &idle);
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    let me = to_sim_player(pair.me());
    let body = Place::OnBody(BodyId::Space(BODY));
    let store = StoreKey::Body(BODY);

    // Warp out, land and dig into the body while the host's own rig digs at home.
    pair.step(&idle, warp(body));
    pair.run(20, &idle);
    assert_eq!(
        pair.host.world().place_of(me),
        body,
        "the host moved the rig"
    );
    assert_eq!(pair.authority().place_of(me), body, "the place replicated");
    pair.host.submit_local_input(PlayerId(0), dig, Vec::new());
    pair.run(240, &dig);
    pair.host.submit_local_input(PlayerId(0), idle, Vec::new());
    pair.run(30, &idle);

    let host = pair.host.world();
    let relevance = pair.host.relevance_for(pair.id).expect("client");
    assert_eq!(
        relevance.region,
        store.code(),
        "relevance follows the place"
    );
    let host_body = host.places().store(store).expect("the body's ground");
    assert!(
        host_body.terrain().edited_chunk_count() > 0,
        "the client dug"
    );
    let dug = host_body
        .terrain()
        .edited_chunks()
        .next()
        .expect("a dug chunk");
    assert!(host.chunk_relevant(&StoreChunk { store, chunk: dug }, &relevance));
    let home_dug = host.terrain().edited_chunks().next().expect("home was dug");
    assert!(
        !host.chunk_relevant(&StoreChunk::home(home_dug), &relevance),
        "home chunks are not the away client's"
    );
    // The client holds the body's ground exactly, and skipped the home world's digging.
    let client_body = pair.authority().places().store(store).expect("replicated");
    assert_eq!(client_body.terrain().digest(), host_body.terrain().digest());
    assert_ne!(
        pair.authority().terrain().digest(),
        host.terrain().digest(),
        "the home ops were withheld"
    );
    // Only the body's entities are the client's.
    let revisions = NetSimulation::entity_revisions(host, &relevance);
    assert!(revisions.iter().all(|(id, _)| {
        host.placed_entity_record(*id)
            .is_some_and(|(at, _)| at == store)
    }));
    assert!(pair.hashes_agree(), "the client's view matches the host's");

    // Home again: the skipped home chunks are refreshed.
    pair.step(&idle, warp(Place::HOME));
    pair.run(60, &idle);
    assert_eq!(pair.authority().place_of(me), Place::HOME);
    assert_eq!(
        pair.authority().terrain().digest(),
        pair.host.world().terrain().digest(),
        "home caught up"
    );
    assert!(pair.hashes_agree());
}

#[test]
fn a_finite_relevance_keeps_players_in_other_places_out() {
    let params = SessionParams::new(60);
    let tuning = NetTuning {
        relevance_radius: Some(40.0),
        ..NetTuning::default()
    };
    let mut pair = Pair::new(world(60), tuning, params);
    let idle = ControlState::default();
    pair.run(30, &idle);
    let host_player = pair.host.world().player_pose(PlayerId(0)).expect("pose");
    let relevance = pair.host.relevance_for(pair.id).expect("client");
    assert!(relevance.contains_pose(&host_player), "both at home");
    pair.step(&idle, warp(Place::Space));
    pair.run(20, &idle);
    let relevance = pair.host.relevance_for(pair.id).expect("client");
    assert_eq!(relevance.region, StoreKey::Space.code());
    let host_player = pair.host.world().player_pose(PlayerId(0)).expect("pose");
    assert!(
        !relevance.contains_pose(&host_player),
        "a rig at home is not near one in space, whatever the numbers"
    );
    let me = pair.host.world().player_pose(pair.me()).expect("pose");
    assert_eq!(me.region, StoreKey::Space.code());
    assert_eq!(
        pair.authority().place_of(to_sim_player(pair.me())),
        Place::Space
    );
}

/// Events are routed by place (Wave 13 QA): a player-less event raised by a body's pass is
/// tagged with that body's store, and only clients in that store (or a spectator) receive
/// it; a home-pass event and any event with a player attached reach everyone as before.
#[test]
fn a_bodys_player_less_events_reach_only_the_clients_there() {
    use crate::world::WorldEvent;
    use fatherload_simulation::{GameplayEvent, GameplayEventId, GameplayEventKind, TickInputs};
    let mut world = world(60);
    let away = fatherload_simulation::PlayerId::new(1);
    world.add_player(away);
    let body = Place::OnBody(BodyId::Space(BODY));
    let mut inputs = TickInputs::default();
    inputs.act_as(away, warp(body).remove(0));
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    let _ = output;
    let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    let _ = output;
    assert_eq!(world.place_of(away), body);
    // Every event of the body's own pass carries its store.
    let mut inputs = TickInputs::default();
    inputs.act_as(away, warp(Place::HOME).remove(0));
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    let moved = output
        .events
        .iter()
        .find(|event| matches!(event.kind, GameplayEventKind::PlaceChanged { .. }))
        .expect("the warp home is told");
    assert_eq!(moved.store, StoreKey::Home, "a move is its destination's");
    let mut inputs = TickInputs::default();
    inputs.act_as(away, warp(body).remove(0));
    let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    let event = |store| {
        WorldEvent::Gameplay(GameplayEvent {
            id: GameplayEventId {
                tick: world.simulation_tick(),
                ordinal: 0,
            },
            kind: GameplayEventKind::HazardChanged,
            store,
        })
    };
    let home_viewer = Some(PlayerId(0));
    let body_viewer = Some(PlayerId(1));
    let on_body = event(body.store());
    assert!(!NetSimulation::event_visible(
        &world,
        &on_body,
        None,
        home_viewer
    ));
    assert!(NetSimulation::event_visible(
        &world,
        &on_body,
        None,
        body_viewer
    ));
    assert!(NetSimulation::event_visible(&world, &on_body, None, None));
    assert!(NetSimulation::event_visible(
        &world,
        &on_body,
        Some(PlayerId(1)),
        home_viewer
    ));
    let at_home = event(StoreKey::Home);
    assert!(NetSimulation::event_visible(
        &world,
        &at_home,
        None,
        body_viewer
    ));
    assert!(NetSimulation::event_visible(
        &world,
        &at_home,
        None,
        home_viewer
    ));
}
