//! The destination index online (Wave 14 W14-B): a client travels by the index (Descent V, the
//! next planet, a bookmark, back) while the host's two split-screen seats travel elsewhere;
//! the host's world decides, every player moves only their own rig, the client's authority
//! keeps matching the host's view, and the travel log replicates. A normal world refuses the
//! same orders.

use std::time::Duration;

use fatherload_simulation::{
    ControlState, PlayerCommand, TestModeAction, WorldState,
    descent::Descent,
    destination::{DestinationFilter, DestinationKind, Selector, TravelRequest},
    enemy::EnemyDifficulty,
    place::{BodyId, Place},
    worldgen::{GeneratorVersion, WorldSettings},
};

use crate::protocol::{self, ServerMessage};
use crate::world::to_sim_player;
use crate::{
    ClientConfig, ClientId, ClientPhase, ClientSession, HostConfig, HostSession, NetSimulation,
    NetTuning, PlayerId, SeatId, SessionParams,
};

struct Trio {
    host: HostSession<WorldState>,
    client: ClientSession<WorldState>,
    id: ClientId,
    dt: Duration,
    host_hashes: std::collections::BTreeMap<u64, u64>,
}

impl Trio {
    fn new(world: WorldState, params: SessionParams) -> Self {
        let mut host = HostSession::new(
            world,
            HostConfig {
                params,
                tuning: NetTuning::default(),
                ..HostConfig::default()
            },
        );
        assert!(host.adopt_local_player(SeatId(0), PlayerId(0)));
        let _second = host.add_local_player(SeatId(1));
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

    fn step(&mut self, actions: Vec<PlayerCommand>) {
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
        for message in self
            .client
            .advance(self.dt, &ControlState::default(), actions)
        {
            let bytes = protocol::encode(&message).expect("encode");
            self.host
                .handle_client_bytes(self.id, &bytes)
                .expect("host decodes");
        }
    }

    fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.step(Vec::new());
        }
    }

    fn seat(&mut self, seat: u32, action: TestModeAction) {
        self.host.submit_local_input(
            PlayerId(seat),
            ControlState::default(),
            vec![PlayerCommand::TestMode(action)],
        );
    }

    fn me(&self) -> PlayerId {
        self.client.player().expect("joined")
    }

    fn authority(&self) -> &WorldState {
        self.client.authority().expect("joined")
    }

    fn hashes_agree(&self) -> bool {
        let tick = NetSimulation::tick(self.authority());
        self.host_hashes.get(&tick.0).is_some_and(|host| {
            *host == NetSimulation::visible_state_hash(self.authority(), Some(self.me()))
        })
    }
}

fn travel(request: TravelRequest) -> Vec<PlayerCommand> {
    vec![PlayerCommand::TestMode(TestModeAction::TravelTo(request))]
}

fn world(test_mode: bool) -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        fatherload_simulation::PlayerId::new(0),
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(0x00C0_FFEE)
        },
        fatherload_simulation::SimulationParams::new(30),
    );
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    if test_mode {
        world.enable_test_mode();
    }
    world
}

#[test]
fn travel_online_and_in_split_screen_moves_each_players_own_rig() {
    let mut trio = Trio::new(world(true), SessionParams::new(30));
    trio.run(30);
    assert_eq!(trio.client.phase(), ClientPhase::InGame);
    let me = to_sim_player(trio.me());
    let seat0 = fatherload_simulation::PlayerId::new(0);

    // The client goes to Descent V; seat 0 bookmarks home and goes to the nearest planet.
    trio.seat(0, TestModeAction::BookmarkHere);
    trio.step(travel(TravelRequest::descent(Descent::HollowWorld)));
    trio.run(4);
    trio.seat(
        0,
        TestModeAction::TravelTo(TravelRequest::new(DestinationKind::Planet, Selector::Next)),
    );
    trio.run(40);
    let host = trio.host.world();
    let deep = host.travel_here(me).expect("client rig");
    let fifth = host
        .destinations(me, DestinationKind::Descent, DestinationFilter::Any)
        .into_iter()
        .find(|found| found.info.descent == Some(Descent::HollowWorld))
        .expect("Descent V");
    assert!(
        deep.y > 5_000,
        "the client is deep down ({deep:?}, {fifth:?})"
    );
    assert!(
        matches!(host.place_of(seat0), Place::OnBody(BodyId::Space(_))),
        "seat 0 is on a planet"
    );
    let seat1 = host
        .player_ids()
        .find(|id| *id != seat0 && *id != me)
        .expect("seat 1");
    assert_eq!(host.place_of(seat1), Place::HOME, "seat 1 stayed home");
    assert_eq!(host.player_travel(seat0).bookmarks.len(), 1);
    assert_eq!(host.player_travel(me).recent.len(), 1);
    // The client's authority follows: its own place and the replicated log.
    assert_eq!(
        trio.authority().travel_here(me).map(|id| id.y),
        Some(deep.y)
    );
    assert_eq!(trio.authority().travel_log(), host.travel_log());
    assert!(trio.hashes_agree(), "the client's view matches the host's");

    // The client goes back home.
    trio.step(vec![PlayerCommand::TestMode(TestModeAction::TravelBack)]);
    trio.run(40);
    let back = trio.host.world().travel_here(me).expect("client rig");
    assert!(back.y < 100, "back home ({back:?})");
    assert!(trio.hashes_agree());
}

#[test]
fn a_normal_world_refuses_travel_online() {
    let mut trio = Trio::new(world(false), SessionParams::new(30));
    trio.run(30);
    let me = to_sim_player(trio.me());
    let before = trio.host.world().travel_here(me).expect("rig");
    trio.step(travel(TravelRequest::descent(Descent::Heart)));
    trio.run(30);
    let after = trio.host.world().travel_here(me).expect("rig");
    assert!(
        (after.y - before.y).abs() < 4,
        "no travel in a normal world"
    );
    assert!(trio.host.world().travel_log().is_empty());
}

/// The god-mode toolkit end to end online (Wave 14 integration): the client stocks its rig and
/// travels to a theme boss's lair on its world while the host's seats go to a station and a
/// faction's flagship; each player moves only their own rig, the host decides, the client's
/// view keeps matching.
#[test]
fn a_stocked_client_and_the_hosts_seats_travel_to_three_different_places() {
    let mut trio = Trio::new(world(true), SessionParams::new(30));
    trio.run(30);
    assert_eq!(trio.client.phase(), ClientPhase::InGame);
    let me = to_sim_player(trio.me());
    let seat0 = fatherload_simulation::PlayerId::new(0);
    let seat1 = trio
        .host
        .world()
        .player_ids()
        .find(|id| *id != seat0 && *id != me)
        .expect("seat 1");

    trio.step(vec![PlayerCommand::TestMode(TestModeAction::StockAll)]);
    trio.run(4);
    assert!(
        trio.host.world().test_stock_gaps(me).is_empty(),
        "the client's rig is stocked"
    );
    let lair = trio
        .host
        .world()
        .destinations(me, DestinationKind::ThemeBossLair, DestinationFilter::Any)
        .into_iter()
        .next()
        .expect("a theme boss's lair");
    let Place::OnBody(lair_body) = lair.id.place else {
        panic!("a lair is on a body: {lair:?}");
    };
    trio.step(travel(TravelRequest::exact(lair.id)));
    trio.seat(
        0,
        TestModeAction::TravelTo(TravelRequest::nearest(DestinationKind::Station)),
    );
    trio.seat(
        1,
        TestModeAction::TravelTo(TravelRequest::nearest(DestinationKind::FactionBossLair)),
    );
    trio.run(40);
    let host = trio.host.world();
    assert_eq!(
        host.place_of(me),
        Place::OnBody(lair_body),
        "the client at the lair"
    );
    assert_eq!(host.place_of(seat0), Place::Space, "seat 0 by a station");
    assert_eq!(host.place_of(seat1), Place::Space, "seat 1 at a flagship");
    assert_ne!(
        host.travel_here(seat0).map(|id| (id.x, id.y)),
        host.travel_here(seat1).map(|id| (id.x, id.y)),
        "two different places in space"
    );
    assert_eq!(trio.authority().place_of(me), Place::OnBody(lair_body));
    assert!(trio.hashes_agree(), "the client's view matches the host's");
}
