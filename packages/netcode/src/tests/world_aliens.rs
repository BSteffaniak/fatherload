//! The friendly aliens online (Wave 12 aliens track): a client warps to a species' first home
//! and lands in its town; the residents reach it as NPC entities of the body's store; it talks
//! to one (a people command on a body) and the words it learned replicate in the shared
//! `Aliens` section; a lesson and a trade go through the host; its view keeps matching the
//! host's.

use std::time::Duration;

use fatherload_simulation::{
    ControlState, PlayerCommand, TestModeAction, WorldState,
    alien::{AlienCommand, AlienSpecies, atlas::first_home, town},
    enemy::EnemyDifficulty,
    entity::{EntityKind, EntityState},
    people::{PeopleCommand, TalkTopic},
    place::{BodyId, Place, StoreKey},
    worldgen::{GeneratorVersion, WorldSettings},
};

use crate::protocol::{self, ServerMessage};
use crate::world::to_sim_player;
use crate::{
    ClientConfig, ClientId, ClientPhase, ClientSession, HostConfig, HostSession, NetSimulation,
    NetTuning, PlayerId, SeatId, SessionParams,
};

const SEED: u64 = 0xA11E_0013;

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

    fn step(&mut self, actions: Vec<PlayerCommand>) {
        let controls = ControlState::default();
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
        for message in self.client.advance(self.dt, &controls, actions) {
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

    fn authority(&self) -> &WorldState {
        self.client.authority().expect("joined")
    }

    fn me(&self) -> PlayerId {
        self.client.player().expect("joined")
    }

    fn hashes_agree(&self) -> bool {
        let tick = NetSimulation::tick(self.authority());
        let me = Some(self.me());
        let Some(host) = self.host_hashes.get(&tick.0) else {
            return false;
        };
        *host == NetSimulation::visible_state_hash(self.authority(), me)
    }
}

/// A Deep Descent test-mode world, peaceful.
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
fn a_client_lands_in_an_alien_town_talks_learns_and_trades_online() {
    let params = SessionParams::new(30);
    let mut pair = Pair::new(world(30), params);
    pair.run(30);
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    let me = to_sim_player(pair.me());
    let species = AlienSpecies::Tokkari;
    let id = first_home(SEED, species).expect("a first home");
    let place = Place::OnBody(BodyId::Space(id));
    let store = StoreKey::Body(id);
    // The test-mode warp to a settled body lands over its first town's square.
    pair.step(vec![PlayerCommand::TestMode(TestModeAction::WarpToPlace(
        place,
    ))]);
    pair.run(60);
    assert_eq!(pair.host.world().place_of(me), place);
    assert_eq!(pair.authority().place_of(me), place, "the place replicated");
    let host = pair.host.world();
    let met = host.aliens().log(me, species);
    assert!(met.met(), "first contact on the host");
    assert_eq!(
        pair.authority().aliens().log(me, species),
        met,
        "and on the client: the section replicated"
    );
    // The residents reach the client as the body's NPC entities.
    let residents = |world: &WorldState| -> Vec<fatherload_simulation::entity::EntityId> {
        world
            .store_entities(store)
            .map_or_default(|entities| {
                entities
                    .of_kind(EntityKind::Npc)
                    .filter(|entity| {
                        matches!(entity.state, EntityState::Npc(state) if town::is_alien_town(state.character.village))
                    })
                    .map(|entity| entity.id)
                    .collect()
            })
    };
    let on_host = residents(pair.host.world());
    assert!(on_host.len() >= 4, "residents about on the host");
    let on_client = residents(pair.authority());
    assert_eq!(on_client, on_host, "the client holds the same residents");
    // Talk to the nearest one in reach (the client's own world says who that is).
    let mut talked = false;
    for _ in 0..200 {
        if let Some(npc) = pair.authority().npcs_in_reach(me).first().copied() {
            let before = pair.host.world().understanding(me, species);
            pair.step(vec![PlayerCommand::People(PeopleCommand::Talk {
                npc,
                topic: TalkTopic::Chat,
            })]);
            pair.run(20);
            let after = pair.host.world().understanding(me, species);
            if after > before {
                talked = true;
                break;
            }
        }
        pair.run(15);
    }
    assert!(talked, "a chat on a body taught words through the host");
    assert_eq!(
        pair.authority().understanding(me, species),
        pair.host.world().understanding(me, species),
        "the client reads with the host's understanding"
    );
    // A purchase from the trader, if they are in reach.
    let trader = town::towns_of(
        &fatherload_simulation::place::planet_params_for(
            fatherload_simulation::space::SpaceSeed::from(SEED),
            id,
        )
        .expect("a planet"),
    )[0]
    .population
    .characters[town::TRADER]
        .id;
    let trader_npc = pair.authority().npcs_in_reach(me).into_iter().find(|npc| {
        pair.authority()
            .npc(*npc)
            .is_some_and(|(_, state)| state.character == trader)
    });
    if let Some(npc) = trader_npc {
        pair.step(vec![PlayerCommand::Alien(AlienCommand::Buy {
            npc,
            good: fatherload_simulation::alien::AlienGood::HullPlates,
        })]);
        pair.run(20);
    }
    pair.run(10);
    assert_eq!(pair.authority().aliens(), pair.host.world().aliens());
    assert!(pair.hashes_agree(), "the client's view matches the host's");
}
