//! World tests of space flight (Wave 11): leaving home over the Moon with the Star Drive (and
//! not without it), the lean limit lifted only in space, coming home, warp (timing, the
//! fuel, interruptions), docking and a station's services, life support's warnings and the
//! emergency beacon, determinism across tick rates, saves and replication.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WorldState,
    attitude::{RigAttitude, tilt_limit},
    enemy::EnemyDifficulty,
    field::trig::wrap_angle,
    flight::{
        FlightCommand, FlightEvent, FlightPlace, RigFlight, StarDrive, StationService, WarpRefusal,
        nav::{self, NavTarget},
        physics::{WARP_ARRIVAL, WARP_SPOOL_MILLIS, warp_duration_millis},
        space::{self, BodyKind, SpaceQuery, SpaceSystem},
    },
    position::WorldPos,
    test_mode::TestModeAction,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::SectionKey;

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x5_7A2D;

fn put_rig(world: &mut WorldState, column: i32, row: i32) {
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(column), i64::from(row)),
    );
    world.stream_terrain_near_players();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn tick_with(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let output = world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
    output.events.into_iter().map(|event| event.kind).collect()
}

fn act(world: &mut WorldState, command: PlayerCommand) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::held(PLAYER, ControlState::default()).act(PLAYER, command);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    output.events.into_iter().map(|event| event.kind).collect()
}

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
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hull = rig.max_hull();
    rig.fuel = rig.fuel_capacity;
    rig.credits = 50_000;
    world
}

fn grant_star_drive(world: &mut WorldState) {
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    heart.star_drive.insert(PLAYER);
    world.set_heart(heart);
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

fn flight_events(events: &[GameplayEventKind]) -> Vec<(FlightEvent, u32)> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Flight { what, detail, .. } => Some((*what, *detail)),
            _ => None,
        })
        .collect()
}

fn flight(world: &WorldState) -> RigFlight {
    *world.rig_flight(PLAYER).expect("rig")
}

/// A point in deep space in the home galactic sector's corner, far from every body.
/// Open space between the home star and its nearest neighbour (the space generator's):
/// within the top nav suite's ping of the neighbour, and far from every surface.
fn deep_space() -> WorldPos {
    use std::sync::OnceLock;
    static DEEP: OnceLock<WorldPos> = OnceLock::new();
    *DEEP.get_or_init(|| {
        let galaxy = space::GeneratedSpace::new(crate::space::SpaceSeed::from(settings().seed));
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
                return at;
            }
        }
        panic!("no open space between home and its neighbour");
    })
}

#[test]
fn a_rig_with_the_star_drive_flies_past_the_moon_into_space_and_one_without_does_not() {
    for drive in [false, true] {
        let mut world = world_at(60);
        if drive {
            grant_star_drive(&mut world);
        }
        let line = world.space_line_row().expect("space line");
        let column = crate::worldgen::sky::moon_column(&settings()).expect("moon") + 600;
        put_rig(&mut world, column, line + 6);
        let up = ControlState::new(0.0, true, false);
        let mut events = Vec::new();
        for _ in 0..240 {
            events.extend(tick_with(&mut world, up));
        }
        let flights = flight_events(&events);
        if drive {
            assert!(world.in_space(PLAYER), "into space");
            assert!(flights.contains(&(FlightEvent::EnteredSpace, 0)));
            let flight = flight(&world);
            assert_eq!(flight.exit_column, column);
            assert!(flight.air > 0.0, "a full tank of air");
            // Wave 12 systems: the rig lives in free flight's store, its store position its
            // exact space position.
            assert_eq!(world.store_of(PLAYER), crate::place::StoreKey::Space);
            let (_, at) = world.rig_world_pos(PLAYER).expect("rig");
            assert!(at.span(flight.pos) < 2.0, "{}", at.span(flight.pos));
            // No loading screen: the rig keeps the climb's speed, away from the home world.
            let home = space::home_centre();
            assert!(flight.pos.distance(home) > space::tiles(space::HOME_APPROACH_TILES));
        } else {
            assert!(!world.in_space(PLAYER), "no Star Drive, no space");
            assert!(flights.is_empty());
        }
    }
}

#[test]
fn the_test_mode_grant_fits_the_star_drive_on_deep_descent_worlds_only() {
    let mut world = world_at(60);
    world.enable_test_mode();
    let events = act(
        &mut world,
        PlayerCommand::TestMode(TestModeAction::GrantStarDrive),
    );
    let _ = events;
    assert!(world.space_unlocked());
    assert!(world.has_star_drive(PLAYER));
    let mut classic = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(SEED),
        SimulationParams::new(60),
    );
    classic.enable_test_mode();
    let _ = act(
        &mut classic,
        PlayerCommand::TestMode(TestModeAction::GrantStarDrive),
    );
    assert!(!classic.space_unlocked(), "older generators have no space");
    assert!(classic.space_line_row().is_none());
}

#[test]
fn the_lean_limit_holds_at_home_and_lifts_only_in_space() {
    let mut world = world_at(60);
    grant_star_drive(&mut world);
    // Low over the town, hovering and leaning hard right for two seconds.
    put_rig(&mut world, 40, -30);
    let mut leaned: f32 = 0.0;
    for tick in 0..120 {
        let thrust = tick % 3 != 0;
        let _ = tick_with(
            &mut world,
            ControlState::new(0.0, thrust, false).with_rotation(1.0),
        );
        leaned = leaned.max(world.player(PLAYER).expect("rig").attitude.orientation);
    }
    let gear = world.player(PLAYER).expect("rig").hazard_gear.attitude;
    assert!(
        leaned <= tilt_limit(gear.tilt_rating) + 0.02,
        "the home world's limit: {leaned}"
    );
    // In space the same thrusters turn the rig all the way round.
    launch_to(&mut world, deep_space(), 0.0);
    let mut turned = 0.0;
    for _ in 0..240 {
        let before = world.player(PLAYER).expect("rig").attitude.orientation;
        let _ = tick_with(&mut world, ControlState::default().with_rotation(1.0));
        turned += wrap_angle(world.player(PLAYER).expect("rig").attitude.orientation - before);
    }
    assert!(
        turned > std::f32::consts::TAU,
        "a full turn and more: {turned}"
    );
    // Released, it keeps spinning (no drag in vacuum).
    let spin = world.player(PLAYER).expect("rig").attitude.angular_velocity;
    for _ in 0..120 {
        let _ = tick_with(&mut world, ControlState::default());
    }
    let after = world.player(PLAYER).expect("rig").attitude.angular_velocity;
    assert!((after - spin).abs() < 1e-3, "{after} vs {spin}");
}

#[test]
fn flying_back_into_the_home_world_hands_the_rig_to_its_sky() {
    let mut world = world_at(60);
    grant_star_drive(&mut world);
    let home = space::home_centre();
    // Just outside the approach sphere east of home, belly toward it (its up axis pointing
    // east, away from home), falling in sideways on the chart.
    let start = home.offset(space::tiles(space::HOME_APPROACH_TILES + 20.0), 0.0);
    launch_to(&mut world, start, std::f32::consts::FRAC_PI_2);
    world.player_mut(PLAYER).expect("rig").flight.velocity_x = -400.0;
    let mut events = Vec::new();
    for _ in 0..240 {
        events.extend(tick_with(&mut world, ControlState::default()));
        if !world.in_space(PLAYER) {
            break;
        }
    }
    assert!(!world.in_space(PLAYER), "home");
    assert!(flight_events(&events).contains(&(FlightEvent::ReturnedHome, 0)));
    let line = world.space_line_row().expect("line");
    let frame = world.frame();
    let rig = world.player(PLAYER).expect("rig");
    // Wave 12 systems: under the point of the home world it approached (a quarter turn east
    // of the departure's column, the Moon's), not the column it left from.
    let moon_column = crate::worldgen::sky::moon_column(&settings()).expect("moon");
    let east = moon_column + 15_708;
    assert!(
        (frame.col(rig.x) - east).abs() <= 4,
        "under the point it approached: {} vs {east}",
        frame.col(rig.x)
    );
    assert!(frame.row(rig.y) > line, "under the space line");
    assert!(rig.velocity_y > 0.0, "still falling in");
    // Coming in belly first is level in the home sky (the frames turn, the rig does not).
    assert!(rig.attitude.orientation.abs() < 0.01, "{:?}", rig.attitude);
}

fn system_east(world: &WorldState) -> SpaceSystem {
    let space = world.space();
    (2..12)
        .find_map(|gx| space.system_at((gx, 0)))
        .expect("a system to the east")
}

#[test]
fn a_warp_spools_crosses_to_another_star_in_its_time_and_costs_fuel() {
    for hz in [30, 60] {
        let mut world = world_at(hz);
        grant_star_drive(&mut world);
        let target = system_east(&world);
        let start = deep_space();
        launch_to(&mut world, start, nav::bearing(start, target.star));
        let _ = act(
            &mut world,
            PlayerCommand::Flight(FlightCommand::SetWaypoint(Some(NavTarget::System(
                target.system,
            )))),
        );
        let fuel_before = world.player(PLAYER).expect("rig").fuel;
        let events = act(&mut world, PlayerCommand::Flight(FlightCommand::Warp));
        assert!(flight_events(&events).contains(&(FlightEvent::WarpSpooling, 0)));
        let started = world.millis_at(world.simulation_tick());
        let mut engaged_at = None;
        let mut arrived_at = None;
        for _ in 0..(hz * 60) {
            let new = tick_with(&mut world, ControlState::default());
            let flights = flight_events(&new);
            let now = world.millis_at(world.simulation_tick());
            if flights.contains(&(FlightEvent::WarpEngaged, 0)) {
                engaged_at = Some(now);
            }
            if flights.contains(&(FlightEvent::WarpArrived, 0)) {
                arrived_at = Some(now);
                break;
            }
        }
        let engaged = engaged_at.expect("the warp left");
        let arrived = arrived_at.expect("the warp arrived");
        assert!(engaged - started >= WARP_SPOOL_MILLIS);
        let expected = u64::from(warp_duration_millis(f64::from(
            start.distance(super::flight::warp_arrival(start, target.star)),
        )));
        let tick = 1_000 / u64::from(hz) + 1;
        assert!(
            arrived.abs_diff(engaged + expected) <= tick,
            "{hz} Hz: {arrived} vs {}",
            engaged + expected
        );
        let flight = flight(&world);
        let distance = flight.pos.distance(target.star);
        assert!(
            (distance - WARP_ARRIVAL).abs() < WARP_ARRIVAL * 0.01,
            "dropped out at the system's edge: {distance}"
        );
        assert!(
            matches!(flight.drive, StarDrive::Cruise { .. }),
            "{flight:?}"
        );
        assert!(world.player(PLAYER).expect("rig").fuel < fuel_before - 50.0);
        assert!(world.known_systems().contains(&target.system));
    }
}

#[test]
fn a_warp_is_refused_off_its_bearing_and_broken_by_a_gravity_well() {
    let mut world = world_at(60);
    grant_star_drive(&mut world);
    let target = system_east(&world);
    let start = deep_space();
    let _ = act(
        &mut world,
        PlayerCommand::Flight(FlightCommand::SetWaypoint(Some(NavTarget::System(
            target.system,
        )))),
    );
    launch_to(&mut world, start, nav::bearing(start, target.star) + 0.5);
    let events = act(&mut world, PlayerCommand::Flight(FlightCommand::Warp));
    assert!(
        flight_events(&events)
            .contains(&(FlightEvent::WarpRefused, WarpRefusal::NotAligned.code()))
    );
    // Lined up so the jump passes straight through the home star.
    let home_star = space::home_star();
    let (dx, dy) = target.star.delta_f64(home_star);
    let length = dx.hypot(dy);
    let back = space::tiles(300_000.0);
    let start = space::advance(
        home_star,
        dx / length * f64::from(back),
        dy / length * f64::from(back),
    );
    launch_to(&mut world, start, nav::bearing(start, target.star));
    let events = act(&mut world, PlayerCommand::Flight(FlightCommand::Warp));
    assert!(flight_events(&events).contains(&(FlightEvent::WarpSpooling, 0)));
    let mut interrupted = false;
    for _ in 0..(60 * 40) {
        let events = tick_with(&mut world, ControlState::default());
        if flight_events(&events).contains(&(FlightEvent::WarpInterrupted, 1)) {
            interrupted = true;
            break;
        }
    }
    assert!(interrupted, "pulled out by the home star's well");
    let flight = flight(&world);
    assert!(!matches!(flight.drive, StarDrive::Warp(_)), "{flight:?}");
    // Pulled out within half the warp's clearance of the star's surface.
    #[allow(clippy::cast_precision_loss, reason = "a star's radius")]
    let reach = space::tiles(crate::space::HOME_SUN_RADIUS as f32 + 5_000.0);
    assert!(flight.pos.distance(home_star) < reach);
}

#[test]
fn docking_needs_the_port_slow_and_aligned_and_opens_the_station() {
    let mut world = world_at(60);
    grant_star_drive(&mut world);
    let station = world
        .space()
        .bodies_of(&world.space().system_at((0, 0)).expect("home"))
        .into_iter()
        .find(|body| body.kind == BodyKind::Station)
        .expect("station");
    let port = station.port().expect("port");
    // Misaligned: refused.
    launch_to(
        &mut world,
        port.offset(10.0, 0.0),
        station.dock_orientation() + 0.8,
    );
    let events = act(&mut world, PlayerCommand::Flight(FlightCommand::Use));
    assert!(!flight_events(&events).contains(&(FlightEvent::Docked, 0)));
    // Aligned, slow, in reach: docked.
    launch_to(
        &mut world,
        port.offset(10.0, 0.0),
        station.dock_orientation(),
    );
    world.player_mut(PLAYER).expect("rig").flight.air = 5.0;
    let events = act(&mut world, PlayerCommand::Flight(FlightCommand::Use));
    assert!(flight_events(&events).contains(&(FlightEvent::Docked, 0)));
    let docked = flight(&world);
    assert_eq!(docked.place, FlightPlace::Docked(station.body));
    assert!(docked.air > 100.0, "air refilled");
    // The station's stub: fuel and repairs for credits.
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.fuel = rig.fuel_capacity * 0.5;
    rig.hull = rig.max_hull() * 0.5;
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
    assert!((rig.fuel - rig.fuel_capacity).abs() < 1.0);
    assert!((rig.hull - rig.max_hull()).abs() < 1.0);
    assert!(rig.credits < credits);
    // Docked rigs hold still whatever the sticks do.
    for _ in 0..30 {
        let _ = tick_with(
            &mut world,
            ControlState::new(1.0, true, false).with_rotation(1.0),
        );
    }
    assert_eq!(flight(&world).pos, port);
    let events = act(&mut world, PlayerCommand::Flight(FlightCommand::Use));
    assert!(flight_events(&events).contains(&(FlightEvent::Undocked, 0)));
    assert!(!matches!(flight(&world).place, FlightPlace::Docked(_)));
}

#[test]
fn life_support_warns_early_and_the_beacon_tows_the_rig_home_for_a_fee() {
    let mut world = world_at(60);
    grant_star_drive(&mut world);
    launch_to(&mut world, deep_space(), 0.0);
    let capacity = world.air_capacity_of(PLAYER);
    let mut warned = Vec::new();
    for share in [0.55, 0.49, 0.24, 0.09] {
        world.player_mut(PLAYER).expect("rig").flight.air = capacity * share;
        for (what, detail) in flight_events(&tick_with(&mut world, ControlState::default())) {
            if what == FlightEvent::AirLow {
                warned.push(detail);
            }
        }
    }
    assert_eq!(
        warned,
        vec![50, 25, 10],
        "warned at half, a quarter and a tenth"
    );
    world.player_mut(PLAYER).expect("rig").flight.air = 0.4;
    let credits = world.player(PLAYER).expect("rig").credits;
    let mut events = Vec::new();
    for _ in 0..60 {
        events.extend(tick_with(&mut world, ControlState::default()));
    }
    let fee = crate::flight::beacon_fee(credits);
    assert!(flight_events(&events).contains(&(FlightEvent::BeaconReturn, fee)));
    assert!(!world.in_space(PLAYER), "towed home");
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(rig.credits, credits - fee);
    assert!(rig.hull > 0.0 && rig.fuel > 0.0);
    // The beacon also answers on demand, and a stranded rig with nothing can still call it.
    launch_to(&mut world, deep_space(), 0.0);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.fuel = 0.0;
    rig.credits = 0;
    let events = act(&mut world, PlayerCommand::Flight(FlightCommand::Beacon));
    assert!(flight_events(&events).contains(&(FlightEvent::BeaconReturn, 0)));
    assert!(!world.in_space(PLAYER));
}

#[test]
fn a_scanner_ping_finds_systems_in_reach() {
    let mut world = world_at(60);
    grant_star_drive(&mut world);
    launch_to(&mut world, deep_space(), 0.0);
    let before = world.known_systems().len();
    let events = act(&mut world, PlayerCommand::UseScanner);
    assert!(
        flight_events(&events)
            .iter()
            .any(|(what, _)| *what == FlightEvent::ScannerPing)
    );
    assert!(
        world.known_systems().len() > before,
        "the nav suite's ping found stars"
    );
    let ping = world
        .flight_shared()
        .pings
        .get(&PLAYER)
        .copied()
        .expect("ping");
    assert!(ping.range >= nav::ping_range(3, true));
}

/// A scripted flight: spin up, thrust, cruise, brake.
fn scripted(hz: u32) -> (WorldPos, RigAttitude, WorldState) {
    let mut world = world_at(hz);
    grant_star_drive(&mut world);
    launch_to(&mut world, deep_space(), 0.0);
    let ticks = |seconds: f32| -> u32 {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "test durations"
        )]
        let count = (seconds * hz as f32).round() as u32;
        count
    };
    for _ in 0..ticks(0.5) {
        let _ = tick_with(&mut world, ControlState::default().with_rotation(1.0));
    }
    for _ in 0..ticks(0.5) {
        let _ = tick_with(&mut world, ControlState::default().with_rotation(-1.0));
    }
    for _ in 0..ticks(2.0) {
        let _ = tick_with(&mut world, ControlState::new(0.4, true, false));
    }
    let _ = act(
        &mut world,
        PlayerCommand::Flight(FlightCommand::ToggleCruise),
    );
    for _ in 0..ticks(3.0) {
        let _ = tick_with(&mut world, ControlState::new(0.0, true, false));
    }
    let flight = flight(&world);
    let attitude = world.player(PLAYER).expect("rig").attitude;
    (flight.pos, attitude, world)
}

#[test]
fn flight_is_deterministic_and_agrees_across_tick_rates() {
    let (reference, attitude, world) = scripted(60);
    let (again, _, twin) = scripted(60);
    assert_eq!(reference, again);
    assert_eq!(world.simulation_state_hash(), twin.simulation_state_hash());
    let travelled = deep_space().distance(reference);
    assert!(travelled > space::tiles(100.0), "it flew: {travelled}");
    for hz in [30, 120] {
        let (other, other_attitude, _) = scripted(hz);
        let gap = other.distance(reference);
        assert!(
            gap < travelled * 0.03,
            "{hz} Hz lands within 3 %: {gap} of {travelled}"
        );
        assert!(
            wrap_angle(other_attitude.orientation - attitude.orientation).abs() < 0.05,
            "{hz} Hz: {other_attitude:?} vs {attitude:?}"
        );
    }
}

#[test]
fn flight_state_saves_and_replicates() {
    assert!(SectionKey::SHARED.contains(&SectionKey::Flight));
    let (_, _, mut world) = scripted(60);
    let _ = act(&mut world, PlayerCommand::UseScanner);
    let _ = act(
        &mut world,
        PlayerCommand::Flight(FlightCommand::SetWaypoint(Some(NavTarget::Body(
            space::HOME_BODY,
        )))),
    );
    let _ = act(
        &mut world,
        PlayerCommand::Flight(FlightCommand::ShareWaypoint),
    );
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    assert_eq!(loaded.rig_flight(PLAYER), world.rig_flight(PLAYER));
    assert_eq!(
        loaded.flight_shared().crew_waypoint,
        Some(NavTarget::Body(space::HOME_BODY))
    );
    // A replica built from the keyframe and the per-tick state agrees.
    let mut replica = world.redacted_for(Some(PlayerId::new(9)));
    let state = world.player_tick_state(PLAYER).expect("tick state");
    replica.apply_player_tick_state(PLAYER, state);
    let payload = world.section(SectionKey::Flight).expect("section");
    replica.apply_section(SectionKey::Flight, payload);
    assert_eq!(replica.rig_flight(PLAYER), world.rig_flight(PLAYER));
    assert_eq!(replica.flight_shared(), world.flight_shared());
}

/// A pilot bot that flies only with the player's controls (rotation thrusters, the main,
/// retro and side thrusters, cruise and the use button): from the launch point over the home
/// world to the home system's station, then docks. It proves the trip is flyable by hand
/// with no assists, and how long it takes.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one bot: steer, cruise, brake, line up, dock"
)]
fn a_bot_flies_from_home_to_the_station_and_docks_by_hand() {
    use crate::flight::physics::{axes, cruise_limit};
    let hz = 60_u32;
    let mut world = world_at(hz);
    grant_star_drive(&mut world);
    // No assists fitted: the bot has only the thrusters.
    world.player_mut(PLAYER).expect("rig").hazard_gear = crate::protection::HazardGear {
        attitude: crate::attitude::AttitudeGear::default(),
        flight: crate::flight::FlightGear::default(),
        ..crate::protection::HazardGear::MAXED
    };
    launch_to(&mut world, space::home_launch(), 0.0);
    let station = world
        .space()
        .bodies_of(&world.space().system_at((0, 0)).expect("home"))
        .into_iter()
        .find(|body| body.kind == BodyKind::Station)
        .expect("station");
    let port = station.port().expect("port");
    let dock_heading = station.dock_orientation();
    // The approach starts on the port's axis, outside it.
    let (up, _) = axes(dock_heading);
    let lineup = port.offset(-up.0 * space::tiles(25.0), -up.1 * space::tiles(25.0));
    let fuel_before = world.player(PLAYER).expect("rig").fuel;
    let mut cruised = false;
    let mut docked_at = None;
    for tick in 0..(hz * 600) {
        let flight = flight(&world);
        if matches!(flight.place, FlightPlace::Docked(_)) {
            docked_at = Some(tick);
            break;
        }
        let attitude = world.player(PLAYER).expect("rig").attitude;
        let to_lineup = flight.pos.distance(lineup);
        let to_port = flight.pos.distance(port);
        let final_leg = to_lineup < space::tiles(4.0) || to_port < space::tiles(25.0);
        let target = if final_leg { port } else { lineup };
        let distance = flight.pos.distance(target);
        let (dx, dy) = flight.pos.delta(target);
        let (dir_x, dir_y) = (dx / distance.max(1.0), dy / distance.max(1.0));
        // Heading: at the target far out, the port's axis close in.
        let heading = if to_port < space::tiles(60.0) {
            dock_heading
        } else {
            nav::bearing(flight.pos, target)
        };
        let error = wrap_angle(heading - attitude.orientation);
        let rotate = 2.5f32.mul_add(error, -0.9 * attitude.angular_velocity);
        // Cruise far out, aligned; it drops by itself in the station's approach sphere.
        let cruising = matches!(flight.drive, StarDrive::Cruise { .. });
        if !cruising
            && distance > space::tiles(2_000.0)
            && error.abs() < 0.05
            && attitude.angular_velocity.abs() < 0.05
        {
            let _ = act(
                &mut world,
                PlayerCommand::Flight(FlightCommand::ToggleCruise),
            );
            cruised = true;
            continue;
        }
        if final_leg && to_port < space::tiles(2.5) && flight.speed() < 60.0 && error.abs() < 0.2 {
            let _ = act(&mut world, PlayerCommand::Flight(FlightCommand::Use));
            continue;
        }
        let controls = if cruising {
            // The throttle open while the cruise's ramp allows it.
            let speed = flight.speed();
            let limit = cruise_limit(distance);
            ControlState::new(0.0, speed < limit * 0.9, speed > limit * 1.1).with_rotation(rotate)
        } else {
            // Impulse: a velocity toward the target that brakes in time, in the craft's axes.
            let speed_wanted = if final_leg {
                (distance * 0.6).min(55.0)
            } else {
                (distance * 0.8).min(1_200.0)
            };
            let (ex, ey) = (
                dir_x.mul_add(speed_wanted, -flight.velocity_x),
                dir_y.mul_add(speed_wanted, -flight.velocity_y),
            );
            let (up, right) = axes(attitude.orientation);
            let forward = ex.mul_add(up.0, ey * up.1);
            let strafe = ex.mul_add(right.0, ey * right.1);
            ControlState::new(
                (strafe / 40.0).clamp(-1.0, 1.0),
                forward > 8.0,
                forward < -8.0,
            )
            .with_rotation(rotate)
        };
        let _ = tick_with(&mut world, controls);
    }
    let seconds = docked_at.map(|tick| tick / hz);
    let fuel_used = fuel_before - world.player(PLAYER).expect("rig").fuel;
    assert!(cruised, "the bot cruised");
    assert!(
        seconds.is_some_and(|seconds| (20..=240).contains(&seconds)),
        "docked by hand in a sensible time: {seconds:?} s, {:?}",
        flight(&world)
    );
    assert!(fuel_used > 0.0, "the trip burned fuel");
}
