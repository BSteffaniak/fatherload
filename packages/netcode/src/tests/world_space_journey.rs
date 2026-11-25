//! The Wave 11 journey online (integration): a client's rig flies up past the Moon, warps to a
//! charted system of the space generator, lands on a small moon's round ground, drives and
//! digs there, takes off, warps home, docks at Halcyon Dock and flies home — while the host's
//! own rig digs at home, in another place the whole time. The client's bot reads only the
//! client's own world; at every leg the client's view matches the host's for its player.

use std::time::Duration;

use fatherload_simulation::{
    ControlState, GameplayEventKind, PlayerCommand, TestModeAction, WorldState,
    enemy::EnemyDifficulty,
    flight::{
        FlightEvent,
        autopilot::{Leg, SpaceBot, chart_with_a_small_moon, home_dock},
        space,
    },
    place::{BodyId, Place, StoreKey},
    worldgen::{GeneratorVersion, WorldSettings},
};

use crate::protocol::{self, ServerMessage};
use crate::world::{WorldEvent, to_sim_player};
use crate::{
    ClientConfig, ClientId, ClientPhase, ClientSession, HostConfig, HostSession, NetSimulation,
    NetTuning, PlayerId, SeatId, SessionParams,
};

const SEED: u64 = 0x5EED_0011;

struct Pair {
    host: HostSession<WorldState>,
    client: ClientSession<WorldState>,
    id: ClientId,
    dt: Duration,
    host_hashes: std::collections::BTreeMap<u64, u64>,
}

impl Pair {
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

    /// One step; returns the gameplay events the host ran.
    fn step(
        &mut self,
        controls: &ControlState,
        actions: Vec<PlayerCommand>,
    ) -> Vec<GameplayEventKind> {
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
        output
            .events
            .into_iter()
            .filter_map(|event| match event.payload {
                WorldEvent::Gameplay(event) => Some(event.kind),
                WorldEvent::ActionResolved { .. } => None,
            })
            .collect()
    }

    fn authority(&self) -> &WorldState {
        self.client.authority().expect("joined")
    }

    fn me(&self) -> PlayerId {
        self.client.player().expect("joined")
    }

    fn hashes_agree(&self) -> bool {
        let tick = NetSimulation::tick(self.authority());
        let me = Some(self.me());
        self.host_hashes
            .get(&tick.0)
            .is_some_and(|host| *host == NetSimulation::visible_state_hash(self.authority(), me))
    }

    /// Runs the client's bot until `legs` legs are done (the host's rig digs at home with
    /// `host_controls`); returns the client player's flight events.
    fn fly(&mut self, bot: &mut SpaceBot, legs: usize) -> Vec<FlightEvent> {
        let me = to_sim_player(self.me());
        let mut seen = Vec::new();
        while bot.finished.len() < legs {
            let (controls, command) = bot.next(self.authority(), me);
            let events = self.step(&controls, command.into_iter().collect());
            for event in &events {
                if let GameplayEventKind::Flight {
                    what, player_id, ..
                } = event
                    && *player_id == me
                {
                    seen.push(*what);
                }
            }
            let mine: Vec<GameplayEventKind> = events
                .into_iter()
                .filter(|event| match event {
                    GameplayEventKind::Flight { player_id, .. } => *player_id == me,
                    GameplayEventKind::Planet { player_id, .. } => *player_id == Some(me),
                    _ => false,
                })
                .collect();
            bot.observe(&mine);
        }
        seen
    }
}

fn world(rate: u32) -> WorldState {
    let first = fatherload_simulation::PlayerId::new(0);
    let mut world = WorldState::new_game_with_settings(
        first,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(SEED)
        },
        fatherload_simulation::SimulationParams::new(rate),
    );
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.enable_test_mode();
    world
}

#[test]
#[allow(clippy::too_many_lines, reason = "the journey's legs read in order")]
fn a_client_flies_to_a_round_moon_and_home_while_the_host_digs_at_home() {
    let rate = 30;
    let params = SessionParams::new(rate);
    let mut base = world(rate);
    let (chart, system, body) = chart_with_a_small_moon(SEED).expect("a charted small moon");
    base.record_star_chart(chart);
    let mut pair = Pair::new(base, params);
    let idle = ControlState::default();
    for _ in 0..30 {
        pair.step(&idle, Vec::new());
    }
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    let me = to_sim_player(pair.me());
    // Debug grant: the Star Drive; then onto the Moon, under the space line.
    pair.step(
        &idle,
        vec![
            PlayerCommand::TestMode(TestModeAction::GrantStarDrive),
            PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::OnBody(BodyId::Moon))),
        ],
    );
    for _ in 0..10 {
        pair.step(&idle, Vec::new());
    }
    assert!(pair.authority().space_access(me).may_leave());
    // The host's own rig digs at home the whole time.
    let dig = ControlState::new(0.0, false, true);
    pair.host.submit_local_input(PlayerId(0), dig, Vec::new());
    let host_player = fatherload_simulation::PlayerId::new(0);
    let galaxy = pair.authority().space();
    let ground = pair
        .authority()
        .body_ground(body.body)
        .expect("round ground");
    let width = i64::from(ground.params.grid().surface_columns());
    let station = home_dock(pair.authority()).expect("Halcyon Dock");
    let mut bot = SpaceBot::new([
        Leg::Leave,
        Leg::Warp(system),
        Leg::Land(body),
        Leg::Drive(width / 4),
        Leg::Dig(8),
        Leg::TakeOff,
        Leg::Warp(space::HOME_SYSTEM),
        Leg::Dock(station),
        Leg::Undock,
        Leg::Home,
    ]);
    // Up into space.
    assert!(pair.fly(&mut bot, 1).contains(&FlightEvent::EnteredSpace));
    assert_eq!(pair.host.world().place_of(me), Place::Space);
    assert_eq!(pair.authority().place_of(me), Place::Space);
    assert!(pair.hashes_agree(), "in space");
    // To the charted system and down onto the moon.
    assert!(pair.fly(&mut bot, 2).contains(&FlightEvent::WarpArrived));
    pair.fly(&mut bot, 3);
    let id = galaxy.space_body(body.body).expect("a generated body");
    let store = StoreKey::Body(id);
    let place = Place::OnBody(BodyId::Space(id));
    assert_eq!(pair.host.world().place_of(me), place);
    assert_eq!(pair.authority().place_of(me), place);
    assert_eq!(pair.host.world().place_of(host_player), Place::HOME);
    let relevance = pair.host.relevance_for(pair.id).expect("client");
    assert_eq!(
        relevance.region,
        store.code(),
        "relevance follows the place"
    );
    assert!(pair.hashes_agree(), "landed");
    // Drive and dig on the round ground; the client holds it exactly.
    pair.fly(&mut bot, 5);
    for _ in 0..10 {
        pair.step(&idle, Vec::new());
    }
    let host_body = pair
        .host
        .world()
        .places()
        .store(store)
        .expect("the body's ground");
    let client_body = pair.authority().places().store(store).expect("replicated");
    assert!(
        host_body.terrain().edited_chunk_count() > 0,
        "the client dug the moon"
    );
    assert_eq!(client_body.terrain().digest(), host_body.terrain().digest());
    assert!(
        client_body.planet().is_some(),
        "a round body's strip on the client too"
    );
    assert!(pair.hashes_agree(), "dug in");
    // The host dug at home meanwhile, in its own place.
    assert!(pair.host.world().terrain().edited_chunks().next().is_some());
    // Off the moon, home by warp, docked, undocked and back in the home sky.
    assert!(pair.fly(&mut bot, 6).contains(&FlightEvent::TookOff));
    assert!(pair.fly(&mut bot, 7).contains(&FlightEvent::WarpArrived));
    let docking = pair.fly(&mut bot, 9);
    assert!(docking.contains(&FlightEvent::Docked));
    assert!(pair.fly(&mut bot, 10).contains(&FlightEvent::ReturnedHome));
    pair.host.submit_local_input(PlayerId(0), idle, Vec::new());
    for _ in 0..30 {
        pair.step(&idle, Vec::new());
    }
    assert_eq!(pair.authority().place_of(me), Place::HOME);
    assert_eq!(
        pair.authority().terrain().digest(),
        pair.host.world().terrain().digest(),
        "home caught up"
    );
    assert!(pair.hashes_agree(), "home again");
}
