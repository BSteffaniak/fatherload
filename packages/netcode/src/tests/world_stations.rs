//! Stations online (Wave 12): a client docked at Halcyon Dock fits a ship line, fills its hold
//! and takes a mission with ordinary actions; the host applies them, the client predicts
//! them, the shared station section and the client's own loadout reach it, and its view keeps
//! matching the host's for its player.

use std::time::Duration;

use fatherload_simulation::{
    ControlState, PlayerCommand, WorldState,
    enemy::EnemyDifficulty,
    flight::{FlightCommand, FlightPlace, RigFlight, StationService, autopilot::home_dock},
    station::{Good, ShipLine, ShipUpgrade, StationAction},
    worldgen::{GeneratorVersion, WorldSettings},
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

/// A V11 world after the ending whose new rigs join docked at Halcyon Dock with credits.
fn world() -> WorldState {
    let first = fatherload_simulation::PlayerId::new(0);
    let mut world = WorldState::new_game_with_settings(
        first,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(0x00C0_FFEE)
        },
        fatherload_simulation::SimulationParams::new(60),
    );
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    world.set_heart(heart);
    let dock = home_dock(&world).expect("Halcyon Dock");
    let park = world.space_park().expect("park");
    let mut template = world.player(first).expect("rig").clone();
    template.flight = RigFlight {
        place: FlightPlace::Docked(dock.body),
        pos: dock.port().expect("port"),
        air: 600.0,
        ..RigFlight::default()
    };
    template.x = park.0;
    template.y = park.1;
    template.credits = fatherload_simulation::endgame::economy::space_credits(50_000);
    world.set_player_template(template);
    world
}

fn hub(action: StationAction) -> Vec<PlayerCommand> {
    vec![PlayerCommand::Flight(FlightCommand::Station(
        StationService::Hub(action),
    ))]
}

#[test]
fn a_client_docked_at_a_station_trades_fits_and_takes_a_mission_online() {
    let mut pair = Pair::new(world(), SessionParams::new(60));
    pair.run(30);
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    let me = to_sim_player(pair.me());
    assert!(
        pair.host.world().docked_station(me).is_some(),
        "the client's rig joined docked"
    );
    pair.step(hub(StationAction::Fit(ShipUpgrade::Line(ShipLine::Hold))));
    pair.run(2);
    pair.step(hub(StationAction::BuyGood(Good::MedKits, 3)));
    pair.run(2);
    pair.step(hub(StationAction::Accept(0)));
    pair.run(40);
    let host = pair.host.world();
    assert_eq!(host.ship_gear(me).hold, 1, "the host fitted the hold");
    assert_eq!(
        host.station_ledger()
            .hold(me)
            .map_or(0, |hold| hold.count(Good::MedKits)),
        3
    );
    assert_eq!(host.station_ledger().missions_of(me).len(), 1);
    // The client holds the same: its loadout, the shared section.
    let client = pair.authority();
    assert_eq!(client.ship_gear(me), host.ship_gear(me));
    assert_eq!(client.station_ledger(), host.station_ledger());
    assert_eq!(
        client.player(me).map(|rig| rig.credits),
        host.player(me).map(|rig| rig.credits)
    );
    assert!(pair.hashes_agree(), "the client's view matches the host's");
}
