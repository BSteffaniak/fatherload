//! The galaxy story online (Wave 13 story): the host's rig stays home while a client warps to
//! the road's first system, scans its world and finds the waymark; the story's section and its
//! events reach the client as the host has them (one road for the crew), the client sets
//! course with a replicated command, and the views match throughout.

use std::time::Duration;

use fatherload_simulation::{
    ControlState, GameplayEventKind, PlayerCommand, TestModeAction, WorldState,
    galaxy_story::{GalaxyCommand, GalaxyEvent, Goal},
    worldgen::{GeneratorVersion, WorldSettings},
};

use crate::protocol::{self, ServerMessage};
use crate::world::to_sim_player;
use crate::{
    ClientConfig, ClientId, ClientPhase, ClientSession, HostConfig, HostSession, NetSimulation,
    NetTuning, PlayerId, SeatId, SessionParams,
};

const SEED: u64 = 0x00C0_FFEE;

struct Pair {
    host: HostSession<WorldState>,
    client: ClientSession<WorldState>,
    id: ClientId,
    dt: Duration,
    host_hashes: std::collections::BTreeMap<u64, u64>,
    host_events: Vec<GameplayEventKind>,
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
            host_events: Vec::new(),
        }
    }

    fn step(&mut self, actions: Vec<PlayerCommand>) {
        let output = self.host.advance(self.dt);
        self.host_events.extend(
            output
                .events
                .iter()
                .filter_map(|event| match &event.payload {
                    crate::world::WorldEvent::Gameplay(gameplay) => Some(gameplay.kind),
                    crate::world::WorldEvent::ActionResolved { .. } => None,
                }),
        );
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

    fn act(&mut self, command: PlayerCommand) {
        self.step(vec![command]);
        self.run(40);
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

    fn goal(&self) -> Option<Goal> {
        self.host
            .world()
            .galaxy_story()
            .current_goal()
            .map(|goal| goal.goal)
    }
}

/// A Deep Descent world in test mode whose crew woke the Heart (space open; new rigs get the
/// Star Drive's upgrades).
fn world() -> WorldState {
    let first = fatherload_simulation::PlayerId::new(0);
    let mut world = WorldState::new_game_with_settings(
        first,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(SEED)
        },
        fatherload_simulation::SimulationParams::new(30),
    );
    world.enable_test_mode();
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    world.set_heart(heart);
    let mut template = world.player(first).expect("rig").clone();
    fatherload_simulation::economy::max_out_upgrades(&mut template);
    template.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
    template.hull = template.max_hull();
    world.set_player_template(template);
    world
}

#[test]
fn a_client_walks_the_first_road_while_the_host_stays_home_and_both_share_it() {
    let mut pair = Pair::new(world(), SessionParams::new(30));
    pair.run(30);
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    let me = to_sim_player(pair.me());
    // The story began for the crew: the client holds the same road.
    assert!(pair.host.world().galaxy_story().begun);
    assert_eq!(
        pair.authority().galaxy_story(),
        pair.host.world().galaxy_story()
    );
    assert_eq!(pair.goal(), Some(Goal::ReachSystem));
    // Set course: a replicated command makes the story's target the client's waypoint.
    pair.act(PlayerCommand::Galaxy(GalaxyCommand::SetCourse));
    assert_eq!(
        pair.host.world().flight_shared().waypoint_of(me),
        pair.host.world().galaxy_waypoint()
    );
    // The client warps to the first system and scans its world; the host stays home.
    pair.act(PlayerCommand::TestMode(TestModeAction::WarpToStory));
    pair.run(20);
    assert_eq!(
        pair.goal(),
        Some(Goal::Scan),
        "the client reached the system"
    );
    pair.act(PlayerCommand::UseScanner);
    assert_eq!(pair.goal(), Some(Goal::FindRelic), "the scan counts");
    // Down onto the world at the waymark.
    pair.act(PlayerCommand::TestMode(TestModeAction::WarpToStory));
    pair.run(40);
    let story = pair.host.world().galaxy_story().clone();
    assert!(story.chapter >= 1, "the waymark found: chapter one closed");
    assert!(story.logs.contains(&0));
    assert_eq!(
        pair.host
            .world()
            .place_of(fatherload_simulation::PlayerId::new(0)),
        fatherload_simulation::place::Place::HOME,
        "the host never left home"
    );
    // The section reached the client: one road, one journal.
    pair.run(20);
    assert_eq!(
        pair.authority().galaxy_story(),
        pair.host.world().galaxy_story()
    );
    assert!(pair.hashes_agree(), "the client's view matches the host's");
    // The road's events were raised (the client's own run of the rules raises them too).
    assert!(pair.host_events.iter().any(|event| matches!(
        event,
        GameplayEventKind::Galaxy {
            what: GalaxyEvent::ChapterDone,
            ..
        }
    )));
}
