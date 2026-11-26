//! The god-mode toolkit online (Wave 14 stock): a client in the host's test-mode world sets
//! its own rig's stock, restocks it and spawns a creature beside it (the host's world decides,
//! the host's rig is untouched, the client's view keeps matching); the same tools sent to a
//! normal world are refused and change nothing.

use std::time::Duration;

use fatherload_simulation::{
    ControlState, PlayerCommand, TestModeAction, WorldState,
    enemy::{EnemyDifficulty, EnemyKind},
    entity::EntityKind,
    test_mode::StockItem,
};

use crate::protocol::{self, ServerMessage};
use crate::world::to_sim_player;
use crate::{
    ClientConfig, ClientId, ClientPhase, ClientSession, HostConfig, HostSession, NetSimulation,
    NetTuning, PlayerId, SeatId, SessionParams,
};

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

    fn act(&mut self, action: TestModeAction) {
        self.step(vec![PlayerCommand::TestMode(action)]);
        self.run(20);
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
}

fn world(test_mode: bool) -> WorldState {
    let mut world = WorldState::new_game_with_params(
        fatherload_simulation::PlayerId::new(0),
        fatherload_simulation::SimulationParams::new(30),
    );
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    if test_mode {
        world.enable_test_mode();
    }
    world
}

fn enemies_near(world: &WorldState, player: fatherload_simulation::PlayerId) -> usize {
    let rig = world.player(player).expect("rig");
    world
        .entities()
        .of_kind(EntityKind::Enemy)
        .filter(|entity| {
            let (dx, dy) = (entity.body.x - rig.x, entity.body.y - rig.y);
            dx.mul_add(dx, dy * dy) <= 400.0 * 400.0
        })
        .count()
}

#[test]
fn a_client_stocks_and_spawns_on_its_own_rig_in_the_hosts_test_world() {
    let mut pair = Pair::new(world(true), SessionParams::new(30));
    pair.run(30);
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    let me = to_sim_player(pair.me());
    let host_rig = fatherload_simulation::PlayerId::new(0);
    // The joiner arrived fully stocked.
    assert_eq!(pair.host.world().test_stock_gaps(me), [] as [String; 0]);
    assert_eq!(pair.authority().stock_count(me, StockItem::Bombs), 99);

    pair.act(TestModeAction::SetInfiniteSupplies(false));
    pair.act(TestModeAction::Give(StockItem::Bombs, 2));
    assert_eq!(pair.host.world().stock_count(me, StockItem::Bombs), 2);
    assert_eq!(pair.authority().stock_count(me, StockItem::Bombs), 2);
    assert_eq!(
        pair.host.world().stock_count(host_rig, StockItem::Bombs),
        99,
        "the host's rig is its own"
    );
    pair.act(TestModeAction::Restock);
    assert_eq!(pair.authority().stock_count(me, StockItem::Bombs), 99);

    let before = enemies_near(pair.host.world(), me);
    pair.act(TestModeAction::SpawnCreature(EnemyKind::ALL[0]));
    assert_eq!(enemies_near(pair.host.world(), me), before + 1);
    pair.act(TestModeAction::ClearNearbyEnemies);
    assert_eq!(enemies_near(pair.host.world(), me), 0);
    pair.run(10);
    assert!(pair.hashes_agree(), "the client's view matches the host's");
}

#[test]
fn the_toolkit_sent_to_a_normal_world_is_refused() {
    let mut pair = Pair::new(world(false), SessionParams::new(30));
    pair.run(30);
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    let me = to_sim_player(pair.me());
    let bombs = pair.host.world().player(me).expect("rig").bombs;
    for action in [
        TestModeAction::StockAll,
        TestModeAction::Give(StockItem::Bombs, 50),
        TestModeAction::SpawnCreature(EnemyKind::ALL[0]),
        TestModeAction::SetTimeOfDay { minutes: 600 },
    ] {
        pair.act(action);
    }
    let host = pair.host.world();
    assert!(host.test_mode().is_none());
    assert_eq!(host.player(me).expect("rig").bombs, bombs);
    assert!(!host.test_stock_gaps(me).is_empty(), "nothing was stocked");
    assert_eq!(enemies_near(host, me), 0);
    assert!(!host.clock().is_held());
    pair.run(10);
    assert!(pair.hashes_agree());
}
