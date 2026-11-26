//! World tests of space stations as hubs (Wave 12 stations): docking opens every service and
//! each works, ship upgrades apply in flight, alien tech at alien-run stations and salvage at
//! derelicts, planet gear counters a world's hazard tags, a bot completes the bounty board's
//! missions with the player's controls, and the station state saves and replicates.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WorldState,
    attitude::RigAttitude,
    enemy::EnemyDifficulty,
    event::TickOutput,
    flight::{
        FlightCommand, FlightPlace, RigFlight, StationService,
        autopilot::{Leg, SpaceBot, home_dock, steer, strip_tile},
        nav::{self, NavTarget},
        space::{self, BodyKind, FlightBody, SpaceQuery},
    },
    item::ItemKind,
    place::{BodyId, Place, StoreKey},
    position::WorldPos,
    space::{HOME_SECTOR, SpaceBodyId, SpaceSector},
    station::{
        AlienTech, Good, MissionKind, PlanetHazard, ShipLine, ShipUpgrade, SpaceGear,
        StationAction, StationEvent, StationId, StationOwner, StationRefusal,
        hazard::hazards_of,
        profile::{profile_of, station_ref},
        shipyard::GEAR_MAX,
    },
    terrain::{MineralKind, TileKind, TilePosition},
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::SectionKey;

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x00C0_FFEE;
const HALCYON: StationId = StationId {
    sector: HOME_SECTOR,
    system: 0,
    index: 0,
};

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    heart.star_drive.insert(PLAYER);
    world.set_heart(heart);
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hull = rig.max_hull();
    rig.fuel = rig.fuel_capacity;
    rig.credits = 0;
    world
}

fn tick(
    world: &mut WorldState,
    controls: ControlState,
    command: Option<PlayerCommand>,
) -> TickOutput {
    let mut inputs = TickInputs::held(PLAYER, controls);
    if let Some(command) = command {
        inputs = inputs.act(PLAYER, command);
    }
    world.simulate_tick(world.simulation_tick(), &inputs)
}

fn act(world: &mut WorldState, command: PlayerCommand) -> Vec<GameplayEventKind> {
    tick(world, ControlState::default(), Some(command))
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn hub(world: &mut WorldState, action: StationAction) -> Vec<(StationEvent, u32)> {
    station_events(&act(
        world,
        PlayerCommand::Flight(FlightCommand::Station(StationService::Hub(action))),
    ))
}

fn station_events(events: &[GameplayEventKind]) -> Vec<(StationEvent, u32)> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Station { what, detail, .. } => Some((*what, *detail)),
            _ => None,
        })
        .collect()
}

fn refused(events: &[(StationEvent, u32)], refusal: StationRefusal) -> bool {
    events.contains(&(StationEvent::Refused, refusal.code()))
}

/// The rig put straight into space at `pos`, still, turned `orientation`.
fn launch_to(world: &mut WorldState, pos: WorldPos, orientation: f32) {
    let park = world.space_park().expect("park");
    let capacity = world.air_capacity_of(PLAYER);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.flight = RigFlight {
        place: FlightPlace::Space,
        pos,
        air: capacity,
        exit_column: 12,
        ..RigFlight::default()
    };
    rig.attitude = RigAttitude {
        orientation,
        angular_velocity: 0.0,
    };
    rig.x = park.0;
    rig.y = park.1;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

/// A station as flight sees it.
fn station_body(world: &WorldState, id: StationId) -> FlightBody {
    let galaxy = world.space();
    let body = station_ref(&galaxy, id).expect("station ref");
    galaxy.body(body).expect("station body")
}

/// Docks at `station` the way a player does: in reach of the port, aligned, slow, use.
fn dock_at(world: &mut WorldState, station: StationId) {
    let body = station_body(world, station);
    let port = body.port().expect("port");
    launch_to(world, port.offset(10.0, 0.0), body.dock_orientation());
    let _ = act(world, PlayerCommand::Flight(FlightCommand::Use));
    assert_eq!(
        world.rig_flight(PLAYER).expect("rig").place,
        FlightPlace::Docked(body.body),
        "docked at {station:?}"
    );
}

/// Every station within `reach` sectors of home.
fn stations_near(world: &WorldState, reach: i32) -> Vec<crate::station::StationProfile> {
    let galaxy = world.space();
    let space = galaxy.space();
    let mut out = Vec::new();
    for y in -reach..=reach {
        for x in -reach..=reach {
            let sector = space.sector(SpaceSector::new(x, y));
            for system in &sector.systems {
                for station in &system.stations {
                    out.push(profile_of(space.seed(), system, station));
                }
            }
        }
    }
    out
}

#[test]
#[allow(clippy::too_many_lines, reason = "every service in turn")]
fn docking_opens_the_hub_and_every_service_works() {
    let mut world = world_at(30);
    dock_at(&mut world, HALCYON);
    let profile = world.docked_station(PLAYER).expect("docked at a station");
    assert_eq!(profile.name, "Halcyon Dock");
    // Shipyard: refused without credits, then two tiers of the engine line, then the yard's
    // limit.
    assert!(refused(
        &hub(
            &mut world,
            StationAction::Fit(ShipUpgrade::Line(ShipLine::Engine))
        ),
        StationRefusal::Credits
    ));
    world.player_mut(PLAYER).expect("rig").credits = 100_000;
    for tier in 1..=2 {
        let events = hub(
            &mut world,
            StationAction::Fit(ShipUpgrade::Line(ShipLine::Engine)),
        );
        assert!(
            events.contains(&(StationEvent::ShipUpgraded, tier)),
            "{events:?}"
        );
    }
    assert_eq!(world.ship_gear(PLAYER).engine, 2);
    let spent = ShipLine::Engine.price(1) + ShipLine::Engine.price(2);
    assert_eq!(world.player(PLAYER).expect("rig").credits, 100_000 - spent);
    assert!(refused(
        &hub(
            &mut world,
            StationAction::Fit(ShipUpgrade::Line(ShipLine::Engine))
        ),
        StationRefusal::Maxed
    ));
    // Planet gear to tier 1 here; alien tech is not sold by humans.
    let events = hub(
        &mut world,
        StationAction::Fit(ShipUpgrade::Gear(SpaceGear::HeatShielding)),
    );
    assert!(events.contains(&(StationEvent::GearFitted, 1)));
    assert!(refused(
        &hub(
            &mut world,
            StationAction::Fit(ShipUpgrade::Gear(SpaceGear::HeatShielding))
        ),
        StationRefusal::Maxed
    ));
    assert!(refused(
        &hub(
            &mut world,
            StationAction::Fit(ShipUpgrade::Alien(AlienTech::XenoLens))
        ),
        StationRefusal::NotHere
    ));
    // Market: buy into the hold (no more than it carries), sell back under the price.
    let credits = world.player(PLAYER).expect("rig").credits;
    let quote = world
        .station_quote(&profile, Good::Hydroponics)
        .expect("Halcyon trades greens");
    let events = hub(&mut world, StationAction::BuyGood(Good::Hydroponics, 100));
    assert!(
        events.contains(&(StationEvent::GoodsBought, quote.buy * 8)),
        "{events:?}"
    );
    assert_eq!(
        world
            .station_ledger()
            .hold(PLAYER)
            .expect("hold")
            .count(Good::Hydroponics),
        world.hold_capacity_of(PLAYER)
    );
    assert!(refused(
        &hub(&mut world, StationAction::BuyGood(Good::WaterIce, 1)),
        StationRefusal::HoldFull
    ));
    let events = hub(&mut world, StationAction::SellGood(Good::Hydroponics, 8));
    assert!(
        events
            .iter()
            .any(|(what, _)| *what == StationEvent::GoodsSold)
    );
    assert!(
        world.station_ledger().hold(PLAYER).is_none(),
        "the hold is empty"
    );
    assert!(
        world.player(PLAYER).expect("rig").credits < credits,
        "the spread"
    );
    assert!(refused(
        &hub(&mut world, StationAction::BuyGood(Good::XenoSilk, 1)),
        StationRefusal::Nothing
    ));
    // Ore from the cargo bay at the station's rate.
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(ItemKind::Mineral(MineralKind::Iron), 10);
    let events = hub(&mut world, StationAction::SellOre);
    assert!(events.contains(&(StationEvent::OreSold, 10 * MineralKind::Iron.value())));
    assert_eq!(
        world
            .player(PLAYER)
            .expect("rig")
            .cargo_bay
            .mineral(MineralKind::Iron),
        0
    );
    // The board: take a mission, the slot rolls its next; abandon it.
    let board = world.station_board(HALCYON);
    assert_eq!(board.len(), 3);
    let first = board[0].expect("a mission in slot 0");
    let events = hub(&mut world, StationAction::Accept(0));
    assert!(events.contains(&(StationEvent::MissionAccepted, first.reward)));
    assert_eq!(world.station_ledger().missions_of(PLAYER), &[first]);
    assert_ne!(
        world.station_board(HALCYON)[0],
        Some(first),
        "a fresh mission"
    );
    let _ = hub(&mut world, StationAction::Abandon(0));
    assert!(world.station_ledger().missions_of(PLAYER).is_empty());
    // Nothing to turn in yet.
    assert!(refused(
        &hub(&mut world, StationAction::TurnIn),
        StationRefusal::Nothing
    ));
    // The bar: a round for a patron with a rumour charts the world and sets a waypoint.
    let patrons = world.station_patrons(HALCYON);
    let (index, patron) = patrons
        .iter()
        .enumerate()
        .find(|(_, patron)| patron.rumour.is_some())
        .expect("a patron with a rumour");
    let rumour = patron.rumour.clone().expect("rumour");
    let events = hub(
        &mut world,
        StationAction::BuyRound(u8::try_from(index).expect("index")),
    );
    assert!(
        events
            .iter()
            .any(|(what, _)| *what == StationEvent::RumourHeard)
    );
    let target = world.space().body_ref(rumour.body).expect("body");
    assert_eq!(world.waypoint_of(PLAYER), Some(NavTarget::Body(target)));
    assert!(world.known_systems().contains(&target.system));
    // Refuel and repair at the station's prices.
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.fuel = rig.fuel_capacity - 10.0;
    rig.hull = rig.max_hull() - 10.0;
    let credits = rig.credits;
    let _ = act(
        &mut world,
        PlayerCommand::Flight(FlightCommand::Station(StationService::Refuel)),
    );
    let _ = act(
        &mut world,
        PlayerCommand::Flight(FlightCommand::Station(StationService::Repair)),
    );
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(
        rig.credits,
        credits - 10 * profile.fuel_price - 10 * profile.repair_price
    );
    // Undocked, the hub is closed.
    let _ = act(&mut world, PlayerCommand::Flight(FlightCommand::Use));
    assert!(refused(
        &hub(&mut world, StationAction::SellOre),
        StationRefusal::NotDocked
    ));
}

/// Open space between the home star and its nearest neighbour, far from every surface, and
/// the neighbour.
fn deep_space(world: &WorldState) -> (WorldPos, space::SpaceSystem) {
    let galaxy = world.space();
    let home = space::home_star();
    let neighbour = galaxy
        .systems_near(home, 1)
        .into_iter()
        .filter(|system| !system.system.is_home())
        .min_by(|a, b| home.distance(a.star).total_cmp(&home.distance(b.star)))
        .expect("a neighbouring star");
    let (dx, dy) = neighbour.star.delta_f64(home);
    let length = dx.hypot(dy);
    for step in 0..200_u16 {
        let reach = f64::from(space::tiles(
            4_000.0f32.mul_add(-f32::from(step), 900_000.0),
        ));
        let at = space::advance(neighbour.star, dx / length * reach, dy / length * reach);
        let bodies = galaxy.bodies_near(at);
        let (gap, _) = space::nearest_surface(&bodies, at);
        if gap > space::tiles(30_000.0) {
            return (at, neighbour);
        }
    }
    panic!("no open space between home and its neighbour");
}

#[test]
fn ship_lines_apply_in_flight() {
    let fitted = |line: ShipLine, tiers: u8| {
        let mut world = world_at(30);
        for _ in 0..tiers {
            world
                .player_mut(PLAYER)
                .expect("rig")
                .hazard_gear
                .ship
                .fit(ShipUpgrade::Line(line));
        }
        world
    };
    // The engine: more speed from a second of full thrust in open space.
    let speed_after_thrust = |tiers: u8| {
        let mut world = fitted(ShipLine::Engine, tiers);
        let (at, _) = deep_space(&world);
        launch_to(&mut world, at, 0.0);
        for _ in 0..30 {
            let _ = tick(&mut world, ControlState::new(0.0, true, false), None);
        }
        world.rig_flight(PLAYER).expect("rig").speed()
    };
    let (stock, tuned) = (speed_after_thrust(0), speed_after_thrust(3));
    assert!(tuned > stock * 1.25, "engine: {stock} -> {tuned}");
    // The hull: a star's surface burns less.
    let burn = |tiers: u8| {
        let mut world = fitted(ShipLine::Hull, tiers);
        let galaxy = world.space();
        let star = galaxy
            .bodies_of(&galaxy.system(space::HOME_SYSTEM).expect("home"))
            .into_iter()
            .find(|body| body.kind == BodyKind::Star)
            .expect("the sun");
        launch_to(&mut world, star.centre.offset(0.0, -star.radius - 4.0), 0.0);
        let before = world.player(PLAYER).expect("rig").hull;
        for _ in 0..15 {
            let _ = tick(&mut world, ControlState::default(), None);
        }
        before - world.player(PLAYER).expect("rig").hull
    };
    let (bare, plated) = (burn(0), burn(5));
    assert!(
        bare > 0.0 && plated < bare * 0.6,
        "hull: {bare} -> {plated}"
    );
    // Life support: more air.
    let air = |tiers: u8| fitted(ShipLine::LifeSupport, tiers).air_capacity_of(PLAYER);
    assert!((air(2) - air(0) - 480.0).abs() < 1e-3);
    // The hold.
    assert_eq!(fitted(ShipLine::Hold, 2).hold_capacity_of(PLAYER), 32);
    // Star Drive range: a warp to the same star costs less fuel.
    let warp_fuel = |tiers: u8| {
        let mut world = fitted(ShipLine::DriveRange, tiers);
        let (at, neighbour) = deep_space(&world);
        launch_to(&mut world, at, nav::bearing(at, neighbour.star));
        let _ = act(
            &mut world,
            PlayerCommand::Flight(FlightCommand::SetWaypoint(Some(NavTarget::System(
                neighbour.system,
            )))),
        );
        let before = world.player(PLAYER).expect("rig").fuel;
        let _ = act(&mut world, PlayerCommand::Flight(FlightCommand::Warp));
        for _ in 0..70 {
            let _ = tick(&mut world, ControlState::default(), None);
        }
        assert!(matches!(
            world.rig_flight(PLAYER).expect("rig").drive,
            crate::flight::StarDrive::Warp(_)
        ));
        before - world.player(PLAYER).expect("rig").fuel
    };
    let (short, long) = (warp_fuel(0), warp_fuel(4));
    assert!((long - short / 2.0).abs() < 1.0, "range: {short} -> {long}");
}

#[test]
fn alien_stations_sell_alien_tech_and_derelicts_are_stripped_once() {
    let mut world = world_at(30);
    world.player_mut(PLAYER).expect("rig").credits = 100_000;
    let stations = stations_near(&world, 3);
    let alien = stations
        .iter()
        .find(|station| matches!(station.owner, StationOwner::Alien(_)))
        .expect("an alien-run station");
    let derelict = stations
        .iter()
        .find(|station| station.owner == StationOwner::Derelict)
        .expect("a derelict");
    // Alien tech: the xeno lens doubles the ping.
    dock_at(&mut world, alien.id);
    let events = hub(
        &mut world,
        StationAction::Fit(ShipUpgrade::Alien(AlienTech::XenoLens)),
    );
    assert!(
        events.contains(&(StationEvent::AlienTechFitted, 1)),
        "{events:?}"
    );
    assert!(world.ship_gear(PLAYER).has(AlienTech::XenoLens));
    let (at, _) = deep_space(&world);
    launch_to(&mut world, at, 0.0);
    let _ = act(&mut world, PlayerCommand::UseScanner);
    let ping = world.flight_shared().pings[&PLAYER];
    let tier = world
        .player(PLAYER)
        .expect("rig")
        .hazard_gear
        .flight
        .nav_suite;
    assert!(
        2.0f32
            .mul_add(-nav::ping_range(tier, ping.fogged), ping.range)
            .abs()
            < 1.0
    );
    // A derelict: no counter, salvage once.
    dock_at(&mut world, derelict.id);
    let events = station_events(&act(
        &mut world,
        PlayerCommand::Flight(FlightCommand::Station(StationService::Refuel)),
    ));
    assert!(refused(&events, StationRefusal::NotHere));
    assert!(refused(
        &hub(&mut world, StationAction::BuyGood(Good::WaterIce, 1)),
        StationRefusal::NotHere
    ));
    let credits = world.player(PLAYER).expect("rig").credits;
    let events = hub(&mut world, StationAction::Salvage);
    let found = events
        .iter()
        .find(|(what, _)| *what == StationEvent::Salvaged)
        .expect("salvage")
        .1;
    assert_eq!(world.player(PLAYER).expect("rig").credits, credits + found);
    let hold = world.station_ledger().hold(PLAYER).expect("salvage aboard");
    assert_eq!(hold.count(Good::SalvagedAlloy), 3);
    assert_eq!(hold.count(Good::BuilderRelic), 1);
    assert!(refused(
        &hub(&mut world, StationAction::Salvage),
        StationRefusal::Stripped
    ));
}

/// A landable world near home with `hazard` on it.
fn world_with(world: &WorldState, hazard: PlanetHazard) -> Option<(SpaceBodyId, u8)> {
    let galaxy = world.space();
    let space = galaxy.space();
    for y in -3..=3 {
        for x in -3..=3 {
            let sector = space.sector(SpaceSector::new(x, y));
            for system in &sector.systems {
                for body in &system.bodies {
                    let id = SpaceBodyId {
                        sector: sector.coords,
                        system: system.index,
                        body: body.index,
                    };
                    let Some(severity) = hazards_of(body)
                        .into_iter()
                        .find(|(found, _)| *found == hazard)
                        .map(|(_, severity)| severity)
                    else {
                        continue;
                    };
                    if crate::place::planet_params_for(SEED, id).is_some()
                        && galaxy.body_ref(id).is_some()
                    {
                        return Some((id, severity));
                    }
                }
            }
        }
    }
    None
}

/// What a world's hazards take over five seconds on its ground, with `gear` tiers of the
/// counter fitted: (hull, fuel, air), and the events.
fn exposure_on(
    body: SpaceBodyId,
    counter: SpaceGear,
    gear: u8,
) -> ((f32, f32, f32), Vec<(StationEvent, u32)>) {
    let mut world = world_at(30);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        for _ in 0..gear {
            rig.hazard_gear.ship.fit(ShipUpgrade::Gear(counter));
        }
    }
    let place = Place::OnBody(BodyId::Space(body));
    assert!(matches!(
        world.warp_to_place(PLAYER, place),
        crate::summary::PlayerScopedCommandOutcome::Applied
    ));
    let target = world.space().body_ref(body).expect("ref");
    let capacity = world.air_capacity_of(PLAYER);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.flight.place = FlightPlace::Landed(target);
    rig.flight.air = capacity;
    // Settle onto the ground first.
    let mut events = Vec::new();
    for _ in 0..60 {
        events.extend(station_events(
            &tick(&mut world, ControlState::default(), None)
                .events
                .into_iter()
                .map(|event| event.kind)
                .collect::<Vec<_>>(),
        ));
    }
    let rig = world.player(PLAYER).expect("rig");
    let before = (rig.hull, rig.fuel, rig.flight.air);
    for _ in 0..150 {
        let _ = tick(&mut world, ControlState::default(), None);
    }
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(world.store_of(PLAYER), StoreKey::Body(body));
    (
        (
            before.0 - rig.hull,
            before.1 - rig.fuel,
            before.2 - rig.flight.air,
        ),
        events,
    )
}

#[test]
fn planet_gear_counters_the_hazard_tags() {
    let world = world_at(30);
    let mut checked = 0;
    for hazard in [
        PlanetHazard::Heat,
        PlanetHazard::Cold,
        PlanetHazard::ToxicAir,
    ] {
        let Some((body, severity)) = world_with(&world, hazard) else {
            continue;
        };
        let counter = hazard.counter();
        let (bare, events) = exposure_on(body, counter, 0);
        assert!(
            events.contains(&(StationEvent::HazardBiting, hazard.code()))
                || hazards_of(&world.space().space().body(body).expect("body"))
                    .first()
                    .is_some_and(|(first, _)| *first != hazard),
            "{hazard:?}: the warning"
        );
        let (geared, _) = exposure_on(body, counter, GEAR_MAX);
        let pick = |drain: (f32, f32, f32)| match hazard {
            PlanetHazard::Heat => drain.0,
            PlanetHazard::Cold => drain.1,
            _ => drain.2,
        };
        let expected = match hazard {
            PlanetHazard::Heat | PlanetHazard::Cold => 0.0,
            // Airless toxic worlds still drain the tank one second a second.
            _ => 5.0,
        };
        assert!(
            pick(bare) > pick(geared) + 1.0 && (pick(geared) - expected).abs() < 0.6,
            "{hazard:?} (severity {severity}): bare {bare:?}, geared {geared:?}"
        );
        checked += 1;
    }
    assert!(checked >= 2, "at least two hazards found near home");
}

/// Missions on Halcyon's starter board the bot flies: (scan body, mine body, delivery target).
fn starter_targets(world: &WorldState) -> (SpaceBodyId, SpaceBodyId, StationId) {
    let board = world.station_board(HALCYON);
    let scan = match board[0].map(|mission| mission.kind) {
        Some(MissionKind::Scan { body }) => body,
        other => panic!("slot 0: {other:?}"),
    };
    let mine = match board[1].map(|mission| mission.kind) {
        Some(MissionKind::Mine { body, .. }) => body,
        other => panic!("slot 1: {other:?}"),
    };
    let to = match board[2].map(|mission| mission.kind) {
        Some(MissionKind::Deliver { to, .. }) => to,
        other => panic!("slot 2: {other:?}"),
    };
    (scan, mine, to)
}

/// Runs the bot until it finished `legs` legs; returns the station events.
fn fly(world: &mut WorldState, bot: &mut SpaceBot, legs: usize) -> Vec<(StationEvent, u32)> {
    let mut seen = Vec::new();
    while bot.finished.len() < legs {
        let (controls, command) = bot.next(world, PLAYER);
        let output = tick(world, controls, command);
        let events: Vec<_> = output.events.iter().map(|event| event.kind).collect();
        seen.extend(station_events(&events));
        bot.observe(&events);
    }
    seen
}

#[test]
#[allow(clippy::too_many_lines, reason = "the bot's whole round of the board")]
fn a_bot_completes_the_starter_board_with_the_players_controls() {
    let mut world = world_at(30);
    world.enable_test_mode();
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hazard_gear.attitude = crate::attitude::AttitudeGear::default();
        rig.hazard_gear.flight = crate::flight::FlightGear::default();
        rig.credits = 0;
    }
    let (scan, mine, to) = starter_targets(&world);
    dock_at(&mut world, HALCYON);
    for slot in 0..3 {
        let events = hub(&mut world, StationAction::Accept(slot));
        assert!(
            events
                .iter()
                .any(|(what, _)| *what == StationEvent::MissionAccepted),
            "slot {slot}: {events:?}"
        );
    }
    let rewards: u32 = world
        .station_ledger()
        .missions_of(PLAYER)
        .iter()
        .map(|mission| mission.reward)
        .sum();
    let galaxy = world.space();
    let scan_body = galaxy
        .body(galaxy.body_ref(scan).expect("scan ref"))
        .expect("scan body");
    let mine_body = galaxy
        .body(galaxy.body_ref(mine).expect("mine ref"))
        .expect("mine body");
    let target = station_body(&world, to);
    // 1. Undock and fly to the scan target's neighbourhood, then ping.
    let mut bot = SpaceBot::new([Leg::Undock]);
    let _ = fly(&mut world, &mut bot, 1);
    let hz = world.timing().rate.hz();
    let mut seen = Vec::new();
    let mut pinged = false;
    for _ in 0..hz * 600 {
        let rig = world.player(PLAYER).expect("rig");
        let gap = rig.flight.pos.distance(scan_body.centre) - scan_body.radius;
        if gap < space::tiles(5_000.0) {
            let output = tick(
                &mut world,
                ControlState::default(),
                Some(PlayerCommand::UseScanner),
            );
            seen.extend(station_events(
                &output
                    .events
                    .iter()
                    .map(|event| event.kind)
                    .collect::<Vec<_>>(),
            ));
            pinged = true;
            break;
        }
        let (controls, command) = steer(&world, PLAYER, scan_body.centre, 1_200.0, None);
        let _ = tick(&mut world, controls, command);
    }
    assert!(pinged, "reached the scan target");
    assert!(
        seen.iter()
            .any(|(what, _)| *what == StationEvent::MissionReady),
        "the scan mission is done: {seen:?}"
    );
    // 2. Land on the quota's world (the nav waypoint first) and dig a seam of ore.
    let mut bot = SpaceBot::new([
        Leg::Waypoint(NavTarget::Body(mine_body.body)),
        Leg::Land(mine_body),
    ]);
    let _ = fly(&mut world, &mut bot, 2);
    assert_eq!(world.store_of(PLAYER), StoreKey::Body(mine));
    let (column, row) = strip_tile(&world, PLAYER).expect("on the strip");
    // A rich seam right under the rig (the world's own ore is sparse near the surface).
    let _ = world.with_store_ground(StoreKey::Body(mine), |ground| {
        for depth in 1..=20 {
            let position = TilePosition {
                x: i32::try_from(column).expect("column"),
                y: i32::try_from(row + depth).expect("row"),
            };
            ground
                .terrain_mut()
                .set_kind(position, TileKind::Ore(MineralKind::Silver));
        }
    });
    let mut bot = SpaceBot::new([Leg::Dig(18)]);
    let dug = fly(&mut world, &mut bot, 1);
    // Let the last drops come in.
    for _ in 0..hz * 2 {
        let output = tick(&mut world, ControlState::default(), None);
        seen.extend(station_events(
            &output
                .events
                .iter()
                .map(|event| event.kind)
                .collect::<Vec<_>>(),
        ));
    }
    seen.extend(dug);
    let quota = world
        .station_ledger()
        .missions_of(PLAYER)
        .iter()
        .find(|mission| matches!(mission.kind, MissionKind::Mine { .. }))
        .copied()
        .expect("the quota");
    assert!(quota.ready(), "the quota is dug: {quota:?}");
    // 3. Take off and carry the delivery to its station; turn in there: every reward paid.
    let mut bot = SpaceBot::new([Leg::TakeOff, Leg::Dock(target)]);
    let _ = fly(&mut world, &mut bot, 2);
    assert!(matches!(
        world.rig_flight(PLAYER).expect("rig").place,
        FlightPlace::Docked(_)
    ));
    let credits = world.player(PLAYER).expect("rig").credits;
    let events = hub(&mut world, StationAction::TurnIn);
    assert!(
        events.contains(&(StationEvent::MissionPaid, rewards)),
        "{events:?}"
    );
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        credits + rewards
    );
    assert!(world.station_ledger().missions_of(PLAYER).is_empty());
    assert_eq!(world.station_ledger().completed[&PLAYER], 3);
    assert!(world.station_ledger().scanned.contains(&scan));
    let _ = home_dock(&world);
}

#[test]
fn station_state_saves_and_replicates() {
    assert!(SectionKey::SHARED.contains(&SectionKey::Stations));
    let mut world = world_at(30);
    world.player_mut(PLAYER).expect("rig").credits = 100_000;
    dock_at(&mut world, HALCYON);
    let _ = hub(
        &mut world,
        StationAction::Fit(ShipUpgrade::Line(ShipLine::Hold)),
    );
    let _ = hub(&mut world, StationAction::BuyGood(Good::MedKits, 3));
    let _ = hub(&mut world, StationAction::Accept(1));
    assert!(!world.station_ledger().is_empty());
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    assert_eq!(loaded.station_ledger(), world.station_ledger());
    assert_eq!(loaded.ship_gear(PLAYER), world.ship_gear(PLAYER));
    let bytes = postcard::to_allocvec(&world).expect("postcard");
    let decoded: WorldState = postcard::from_bytes(&bytes).expect("postcard load");
    assert_eq!(decoded.station_ledger(), world.station_ledger());
    // A replica gets the shared section and the player's own loadout.
    let mut replica = world.redacted_for(Some(PlayerId::new(9)));
    let payload = world.section(SectionKey::Stations).expect("section");
    replica.apply_section(SectionKey::Stations, payload);
    assert_eq!(replica.station_ledger(), world.station_ledger());
    let mut own = world.redacted_for(Some(PLAYER));
    let payload = world.section(SectionKey::Loadout(PLAYER)).expect("loadout");
    own.apply_section(SectionKey::Loadout(PLAYER), payload);
    assert_eq!(own.ship_gear(PLAYER).hold, 1);
    // The same actions give the same world on another run.
    let mut again = world_at(30);
    again.player_mut(PLAYER).expect("rig").credits = 100_000;
    dock_at(&mut again, HALCYON);
    let _ = hub(
        &mut again,
        StationAction::Fit(ShipUpgrade::Line(ShipLine::Hold)),
    );
    let _ = hub(&mut again, StationAction::BuyGood(Good::MedKits, 3));
    let _ = hub(&mut again, StationAction::Accept(1));
    assert_eq!(again.simulation_state_hash(), world.simulation_state_hash());
}
