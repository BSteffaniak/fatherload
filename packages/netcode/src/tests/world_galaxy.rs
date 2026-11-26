//! The integrated galaxy online (Wave 12 integration): while the host's rig stays home, a
//! client docked at Halcyon Dock fits planet gear and a cannon, warps down onto a volcanic
//! world (its native creatures and its body reading reach the client as the host has them),
//! then back up into free flight where a pirate encounter finds it (the ships replicate); the
//! client's view keeps matching the host's throughout.

use std::time::Duration;

use fatherload_simulation::{
    ControlState, PlayerCommand, TestModeAction, WorldState,
    enemy::EnemyDifficulty,
    entity::{EntityKind, EntityState},
    flight::{FlightCommand, FlightPlace, RigFlight, StationService, autopilot::home_dock},
    hostiles::{CombatCommand, Faction, ShipGearKind},
    place::{BodyId, Place, SpaceBodyKey, StoreKey},
    space::{Space, SpaceSector, Theme},
    station::{ShipUpgrade, SpaceGear, StationAction},
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

/// A Deep Descent world in test mode, hostiles on, whose new rigs join docked at Halcyon Dock
/// with the Heart's upgrades and credits.
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
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    world.enable_test_mode();
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    world.set_heart(heart);
    let dock = home_dock(&world).expect("Halcyon Dock");
    let park = world.space_park().expect("park");
    let mut template = world.player(first).expect("rig").clone();
    fatherload_simulation::economy::max_out_upgrades(&mut template);
    template.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
    template.hazard_gear.ship = fatherload_simulation::station::ShipGear::default();
    template.hazard_gear.arms = fatherload_simulation::hostiles::ShipGear::default();
    template.hull = template.max_hull();
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

/// A landable body near home whose space theme is `theme`.
fn body_of_theme(theme: Theme) -> SpaceBodyKey {
    let space = Space::shared(fatherload_simulation::space::SpaceSeed::from(SEED));
    for ring in 0..=6_i32 {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let sector = SpaceSector::new(x, y);
                for system in &space.sector(sector).systems {
                    for body in &system.bodies {
                        let id = SpaceBodyKey {
                            sector,
                            system: system.index,
                            body: body.index,
                        };
                        if body.theme == theme
                            && fatherload_simulation::place::planet_params_for(
                                fatherload_simulation::space::SpaceSeed::from(SEED),
                                id,
                            )
                            .is_some()
                        {
                            return id;
                        }
                    }
                }
            }
        }
    }
    panic!("no {theme:?} body near home");
}

fn enemies_of(world: &WorldState, store: StoreKey) -> Vec<fatherload_simulation::enemy::EnemyKind> {
    world.store_entities(store).map_or_default(|entities| {
        entities
            .of_kind(EntityKind::Enemy)
            .filter_map(|entity| match entity.state {
                EntityState::Enemy(state) => Some(state.kind),
                _ => None,
            })
            .collect()
    })
}

#[test]
fn a_client_fits_out_lands_on_a_volcanic_world_and_meets_pirates_while_the_host_stays_home() {
    let mut pair = Pair::new(world(), SessionParams::new(30));
    pair.run(30);
    assert_eq!(pair.client.phase(), ClientPhase::InGame);
    let me = to_sim_player(pair.me());
    let host_player = fatherload_simulation::PlayerId::new(0);
    // Docked at Halcyon: planet gear and a cannon.
    pair.step(vec![PlayerCommand::Flight(FlightCommand::Station(
        StationService::Hub(StationAction::Fit(ShipUpgrade::Gear(
            SpaceGear::HeatShielding,
        ))),
    ))]);
    pair.run(2);
    pair.step(vec![PlayerCommand::Combat(CombatCommand::BuyGear(
        ShipGearKind::Cannon,
    ))]);
    pair.run(20);
    let host = pair.host.world();
    assert_eq!(
        host.ship_gear(me).heat_shielding,
        1,
        "the host fitted the gear"
    );
    assert_eq!(host.player(me).expect("rig").hazard_gear.arms.cannon, 1);
    assert_eq!(pair.authority().ship_gear(me), host.ship_gear(me));
    assert!(pair.hashes_agree());
    // Down onto a volcanic world: the client's place, its creatures and its reading.
    let id = body_of_theme(Theme::Volcanic);
    let place = Place::OnBody(BodyId::Space(id));
    let store = StoreKey::Body(id);
    pair.step(vec![PlayerCommand::TestMode(TestModeAction::WarpToPlace(
        place,
    ))]);
    let mut natives = Vec::new();
    for _ in 0..60 {
        pair.run(30);
        natives = enemies_of(pair.host.world(), store);
        if !natives.is_empty() {
            break;
        }
    }
    assert_eq!(pair.host.world().place_of(me), place);
    assert_eq!(pair.authority().place_of(me), place, "the place replicated");
    assert_eq!(
        pair.host.world().place_of(host_player),
        Place::HOME,
        "the host stays home"
    );
    assert!(!natives.is_empty(), "the volcanic world's creatures came");
    assert_eq!(
        enemies_of(pair.authority(), store),
        natives,
        "the client holds the same creatures"
    );
    let reading = |world: &WorldState| {
        world
            .body_reading_of(me)
            .map(|reading| (reading.local_permille, reading.weather.kind))
    };
    assert_eq!(reading(pair.authority()), reading(pair.host.world()));
    assert!(pair.hashes_agree(), "the client's view matches the host's");
    // Up into free flight: pirates find the rig, and their ships reach the client.
    pair.step(vec![PlayerCommand::TestMode(TestModeAction::WarpToPlace(
        Place::Space,
    ))]);
    pair.run(10);
    pair.step(vec![PlayerCommand::TestMode(
        TestModeAction::SpawnHostiles(Faction::Pirates),
    )]);
    pair.run(20);
    let ships = pair.host.world().hostiles().ships.len();
    assert!(ships > 0, "pirates found the client's rig");
    assert_eq!(
        pair.authority().hostiles().ships.len(),
        ships,
        "and replicated"
    );
    assert!(pair.hashes_agree(), "the client's view matches the host's");
}
