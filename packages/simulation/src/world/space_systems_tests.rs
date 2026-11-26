//! World tests of space's things (Wave 12 systems): a star's radiance and flare, a black
//! hole's well, dilation and rescue, a wormhole's transit, nebula fog, mining a belt rock from
//! flight (drill and beam) until it is mined out, comet dust, the Moon's handover, coming home
//! where the rig approaches, the parking hold, determinism across tick rates, saves and the
//! shared section.

use std::sync::OnceLock;

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WorldState,
    attitude::RigAttitude,
    enemy::EnemyDifficulty,
    flight::{
        FlightPlace, RIG_CONTACT, RigFlight, StarDrive,
        physics::up_angle,
        space::{self, BodyKind, GeneratedSpace, SpaceQuery, tiles},
        systems::{BodyExtra, SystemsEvent},
    },
    input::WeaponControl,
    position::WorldPos,
    space::{Space, SpaceSector},
    terrain::TILE_SIZE,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::SectionKey;

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x5_7A2D;

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
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    heart.star_drive.insert(PLAYER);
    world.set_heart(heart);
    world
}

/// The rig put straight into space at `pos`, moving `velocity`, turned `orientation`.
fn launch_to(world: &mut WorldState, pos: WorldPos, velocity: (f32, f32), orientation: f32) {
    let park = world.space_park().expect("park");
    let capacity = world.air_capacity_of(PLAYER);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.flight = RigFlight {
        place: FlightPlace::Space,
        pos,
        velocity_x: velocity.0,
        velocity_y: velocity.1,
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

fn tick_with(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let output = world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
    output.events.into_iter().map(|event| event.kind).collect()
}

fn run(world: &mut WorldState, seconds: f32, controls: ControlState) -> Vec<(SystemsEvent, u32)> {
    let hz = world.timing().rate.hz();

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "a test's ticks"
    )]
    let ticks = (seconds * hz as f32).round() as u32;
    let mut seen = Vec::new();
    for _ in 0..ticks {
        for event in tick_with(world, controls) {
            if let GameplayEventKind::SpaceSystems { what, detail, .. } = event {
                seen.push((what, detail));
            }
        }
    }
    seen
}

fn thrusting() -> ControlState {
    let mut controls = ControlState::default();
    controls.thrust = true;
    controls
}

fn flight(world: &WorldState) -> RigFlight {
    *world.rig_flight(PLAYER).expect("rig")
}

fn galaxy() -> &'static Space {
    static SPACE: OnceLock<GeneratedSpace> = OnceLock::new();
    SPACE
        .get_or_init(|| GeneratedSpace::new(crate::space::SpaceSeed::from(SEED)))
        .space()
}

/// Every sector within `reach` of home, nearest rings first.
fn sectors(reach: i32) -> Vec<SpaceSector> {
    let mut all = Vec::new();
    for ring in 0..=reach {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) == ring {
                    all.push(SpaceSector::new(x, y));
                }
            }
        }
    }
    all
}

/// The flight body of the first `kind` found near home (the flight bodies of its sector).
fn first_body(kind: BodyKind, reach: i32) -> space::FlightBody {
    let generated = GeneratedSpace::new(crate::space::SpaceSeed::from(SEED));
    for sector in sectors(reach) {
        let centre = crate::space::SpaceTile {
            x: sector.origin().x + crate::space::SPACE_SECTOR_TILES / 2,
            y: sector.origin().y + crate::space::SPACE_SECTOR_TILES / 2,
        };
        let pos = centre.pos();
        if let Some(found) = generated.bodies_near(pos).into_iter().find(|body| {
            body.kind == kind
                && space::galaxy_sector_of(body.centre) == space::galaxy_sector_of(pos)
        }) {
            return found;
        }
    }
    panic!("no {kind:?} within {reach} sectors of home");
}

#[test]
fn a_black_holes_well_dilates_time_warns_three_times_and_its_horizon_is_a_fair_rescue() {
    let hole = first_body(BodyKind::BlackHole, 12);
    let BodyExtra::Hole(facts) = hole.extra else {
        unreachable!()
    };
    let mut world = world_at(30);
    // Just inside the lensing: the well, slow time, a real pull (never a turn).
    let inside = hole.centre.offset(facts.lensing * 0.9, 0.0);
    launch_to(&mut world, inside, (0.0, 0.0), 0.4);
    let air = flight(&world).air;
    let events = run(&mut world, 5.0, ControlState::default());
    assert!(
        events
            .iter()
            .any(|(what, _)| *what == SystemsEvent::HoleWell),
        "{events:?}"
    );
    let scale = world.time_scale_at(flight(&world).pos);
    assert!(scale < 1.0, "{scale}");
    let after = flight(&world);
    assert!(after.lag_seconds > 0.0, "the rig's clock fell behind");
    assert!(
        air - after.air < 5.0 - 1e-3,
        "air drains on the rig's own clock"
    );
    assert!(
        after.pos.span(hole.centre) < facts.lensing * 0.9,
        "pulled in"
    );
    assert!((world.player(PLAYER).expect("rig").attitude.orientation - 0.4).abs() < 1e-3);
    // At the strain line the thrusters still climb out (the warning comes in time).
    let thrust = world.rig_thrust(PLAYER);
    let no_return = crate::flight::systems::no_return_radius(&hole, thrust);
    let strain = no_return / crate::flight::systems::HOLE_STRAIN_SHARE.sqrt() * 0.97;
    launch_to(
        &mut world,
        hole.centre.offset(strain, 0.0),
        (0.0, 0.0),
        up_angle(1.0, 0.0),
    );
    let events = run(&mut world, 1.0, thrusting());
    assert!(
        events
            .iter()
            .any(|(what, _)| *what == SystemsEvent::HoleStrain),
        "{events:?}"
    );
    run(&mut world, 30.0, thrusting());
    let escaping = flight(&world);
    assert!(
        escaping.pos.span(hole.centre) > strain * 1.03,
        "climbing out"
    );
    assert!(escaping.velocity_x > 0.0, "moving away");
    // Past no return: the warning, then the horizon's tug rescue home, the hold lost.
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.add_cargo(crate::terrain::MineralKind::Gold);
    let credits = rig.credits;
    launch_to(
        &mut world,
        hole.centre.offset(no_return * 0.9, 0.0),
        (0.0, 0.0),
        0.0,
    );
    let events = run(&mut world, 200.0, ControlState::default());
    assert!(
        events
            .iter()
            .any(|(what, _)| *what == SystemsEvent::HoleNoReturn),
        "{events:?}"
    );
    let rescued = events
        .iter()
        .find(|(what, _)| *what == SystemsEvent::HoleRescued)
        .expect("rescued");
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(rig.flight.place, FlightPlace::Home);
    assert_eq!(rig.cargo_used(), 0, "the hold went into the hole");
    assert_eq!(credits - rig.credits, rescued.1);
    assert!(rig.hull > 0.0);
}

#[test]
fn a_wormhole_carries_the_rig_to_its_pair_and_the_crew_learns_the_route() {
    let mouth = first_body(BodyKind::Wormhole, 12);
    let BodyExtra::Mouth(facts) = mouth.extra else {
        unreachable!()
    };
    let mut world = world_at(30);
    let start = mouth.centre.offset(mouth.radius + tiles(40.0), 0.0);
    launch_to(&mut world, start, (-1_000.0, 0.0), up_angle(-1.0, 0.0));
    let events = run(&mut world, 2.0, ControlState::default());
    assert!(
        events.contains(&(SystemsEvent::WormholeEntered, 1)),
        "{events:?}"
    );
    assert!(matches!(flight(&world).drive, StarDrive::Transit(_)));
    assert_eq!(
        world
            .space_systems()
            .wormhole_from(facts.sector)
            .map(|(_, sector)| sector),
        Some(facts.exit_sector)
    );
    let events = run(&mut world, 4.0, ControlState::default());
    assert!(
        events
            .iter()
            .any(|(what, _)| *what == SystemsEvent::WormholeExited)
    );
    let after = flight(&world);
    assert_eq!(after.drive, StarDrive::Off);
    // In open space, or still in the far mouth's sphere (moving out of it).
    assert!(
        matches!(after.place, FlightPlace::Space | FlightPlace::Approach(_)),
        "{:?}",
        after.place
    );
    let out = after.pos.span(facts.exit);
    assert!(
        out > facts.exit_radius && out < facts.exit_radius + tiles(400.0),
        "out of the far mouth: {out}"
    );
    assert_eq!(space::galaxy_sector_of(after.pos), facts.exit_sector);
    // It never falls straight back in.
    run(&mut world, 3.0, ControlState::default());
    assert_eq!(flight(&world).drive, StarDrive::Off);
}

#[test]
fn a_nebula_fogs_the_scanner_and_says_so() {
    let mut nebula = None;
    'search: for sector in sectors(12) {
        let found = galaxy().sector(sector);
        for cloud in found.nebulae.iter().chain(
            found
                .systems
                .iter()
                .filter_map(|system| system.nebula.as_ref()),
        ) {
            if cloud.fog >= 400 {
                nebula = Some(*cloud);
                break 'search;
            }
        }
    }
    let nebula = nebula.expect("a nebula near home");
    let mut world = world_at(30);
    launch_to(&mut world, nebula.centre.pos(), (0.0, 0.0), 0.0);
    let events = run(&mut world, 0.5, ControlState::default());
    assert!(
        events
            .iter()
            .any(|(what, fog)| *what == SystemsEvent::NebulaEntered && *fog >= 400),
        "{events:?}"
    );
    assert!(world.nebula_cover(flight(&world).pos) >= 400);
    let inputs = TickInputs::held(PLAYER, ControlState::default())
        .act(PLAYER, crate::PlayerCommand::UseScanner);
    world.simulate_tick(world.simulation_tick(), &inputs);
    let ping = world.flight_shared().pings[&PLAYER];
    assert!(ping.fogged);
    assert!(ping.range < crate::flight::nav::ping_range(3, false) * 0.5);
}

/// A belt rock of at least 20 tiles near home and its body.
fn belt_rock() -> space::FlightBody {
    static ROCK: OnceLock<space::FlightBody> = OnceLock::new();
    *ROCK.get_or_init(|| {
        let generated = GeneratedSpace::new(crate::space::SpaceSeed::from(SEED));
        let empty = crate::flight::systems::SystemsShared::default();
        for sector in sectors(4) {
            for system in &galaxy().sector(sector).systems {
                for belt in &system.belts {
                    let mid = i64::from(belt.inner / 2 + belt.outer / 2);
                    let at = system.centre.offset(mid, 0).pos();
                    if let Some(rock) = generated
                        .rocks_near(at, 6_000, &empty)
                        .into_iter()
                        .find(|rock| rock.radius >= tiles(20.0))
                    {
                        return rock;
                    }
                }
            }
        }
        panic!("no belt rock near home");
    })
}

#[test]
fn a_rig_mines_a_belt_rock_from_flight_until_it_crumbles() {
    let rock = belt_rock();
    let BodyExtra::Rock(facts) = rock.extra else {
        unreachable!()
    };
    let mut world = world_at(30);
    // Resting with its underside on the rock: the drill bites.
    let up = (0.0_f32, -1.0_f32);
    let pos = rock.centre.offset(0.0, -(rock.radius + RIG_CONTACT + 1.0));
    launch_to(&mut world, pos, (0.0, 0.0), up_angle(up.0, up.1));
    let events = run(&mut world, 3.0, ControlState::default());
    let mined = events
        .iter()
        .filter(|(what, _)| *what == SystemsEvent::OreMined)
        .count();
    assert!(mined >= 2, "{events:?}");
    let mineral = crate::flight::systems::asteroid_mineral(facts.ore);
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.cargo_bay.mineral(mineral) >= 2);
    let taken = world.space_systems().mined_from(facts.id);
    assert_eq!(usize::from(taken), mined);
    // The beam from ten tiles off, nose to the rock, trigger held, mines it out.
    let mut record = world.space_systems().clone();
    record.mined.insert(facts.id, facts.yield_units - 1);
    world.set_space_systems(record);
    let off = rock.centre.offset(rock.radius + tiles(10.0), 0.0);
    launch_to(&mut world, off, (0.0, 0.0), up_angle(-1.0, 0.0));
    let fire = ControlState::default().with_weapon(WeaponControl {
        fire: true,
        ..WeaponControl::default()
    });
    let events = run(&mut world, 2.0, fire);
    assert!(
        events
            .iter()
            .any(|(what, _)| *what == SystemsEvent::AsteroidDepleted),
        "{events:?}"
    );
    assert!(
        !world
            .flight_bodies_near(rock.centre, 0.0)
            .iter()
            .any(|body| body.extra == rock.extra),
        "a mined-out rock is gone"
    );
    // The record survives a save and reaches a replica through its section.
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.space_systems(), world.space_systems());
    assert!(SectionKey::SHARED.contains(&SectionKey::SpaceSystems));
    let mut replica = world_at(30);
    let payload = world.section(SectionKey::SpaceSystems).expect("section");
    replica.apply_section(SectionKey::SpaceSystems, payload);
    assert_eq!(replica.space_systems(), world.space_systems());
    let _ = TILE_SIZE;
}

#[test]
fn flying_through_a_comets_tail_gathers_dust() {
    let comet = first_body(BodyKind::Comet, 6);
    let BodyExtra::Comet(facts) = comet.extra else {
        unreachable!()
    };
    let mut world = world_at(30);
    let (dx, dy) = comet.centre.delta(facts.tail_end);
    let middle = comet.centre.offset(dx * 0.5, dy * 0.5);
    launch_to(&mut world, middle, (0.0, 0.0), 0.0);
    let events = run(&mut world, 6.0, ControlState::default());
    let dust = events
        .iter()
        .filter(|(what, _)| *what == SystemsEvent::CometDust)
        .count();
    assert!(dust >= 2, "{events:?}");
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.cargo_bay.mineral(crate::flight::systems::COMET_MINERAL) >= 2);
}

#[test]
fn a_stars_radiance_and_flare_heat_the_hull_with_a_warning_first() {
    // A flaring star near home and a point at 1.5 heat radii from it, on the far side from
    // its worlds' plane (the open side).
    let mut found = None;
    'search: for sector in sectors(6) {
        let generated = GeneratedSpace::new(crate::space::SpaceSeed::from(SEED));
        for system in generated.systems_near(
            crate::space::SpaceTile {
                x: sector.origin().x + 1,
                y: sector.origin().y + 1,
            }
            .pos(),
            0,
        ) {
            for star in generated.bodies_of(&system) {
                if let BodyExtra::Star(facts) = star.extra
                    && facts.flare_period_s > 0
                    && star.body.index == 0
                {
                    found = Some((star, facts));
                    break 'search;
                }
            }
        }
    }
    let (star, facts) = found.expect("a flaring star near home");
    let mut world = world_at(30);
    let hz = u64::from(world.timing().rate.hz());
    // The clock just before a flare's warning.
    let period = u64::from(facts.flare_period_s) * 1_000;
    let start = u64::from(facts.flare_phase_s) * 1_000 % period + period * 3;
    let warn_at = start - crate::flight::systems::FLARE_WARNING_MILLIS - 1_000;
    world.set_simulation_tick(crate::SimulationTick::new(warn_at * hz / 1_000));
    // Unshielded, where the radiance is a little over the hull's cooling.
    world
        .player_mut(PLAYER)
        .expect("rig")
        .hazard_gear
        .flight
        .reentry_shield = 0;
    let light = (f32::from(facts.light) / 1_000.0).clamp(0.5, 2.0);
    let radii = (crate::flight::systems::RADIANT_HEAT * light / 0.35).sqrt();
    let pos = star.centre.offset(0.0, facts.heat_radius * radii);
    launch_to(&mut world, pos, (0.0, 0.0), 0.0);
    let events = run(&mut world, 3.0, ControlState::default());
    assert!(
        events
            .iter()
            .any(|(what, _)| *what == SystemsEvent::StarHeat),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|(what, _)| *what == SystemsEvent::FlareWarning),
        "{events:?}"
    );
    let before = flight(&world).heat;
    let hull = world.player(PLAYER).expect("rig").hull;
    let events = run(&mut world, 18.0, ControlState::default());
    assert!(
        events.iter().any(|(what, _)| matches!(
            what,
            SystemsEvent::FlareStruck | SystemsEvent::FlareSheltered
        )),
        "{events:?}"
    );
    if events
        .iter()
        .any(|(what, _)| *what == SystemsEvent::FlareStruck)
    {
        assert!(world.player(PLAYER).expect("rig").hull < hull || flight(&world).heat > before);
    }
}

#[test]
fn coming_down_to_the_moon_lands_on_it_and_a_fly_by_does_not() {
    let mut world = world_at(30);
    let moon = space::moon_centre();
    let radius = tiles(640.0);
    // A fly-by beside the Moon, inside its approach sphere but well over it.
    launch_to(
        &mut world,
        moon.offset(radius + tiles(110.0), 0.0),
        (0.0, -900.0),
        0.0,
    );
    run(&mut world, 1.0, ControlState::default());
    assert!(flight(&world).away(), "no snap home on a fly-by");
    // Coming down onto it from above: over the Moon in the home sky.
    launch_to(
        &mut world,
        moon.offset(tiles(200.0), -(radius + tiles(80.0))),
        (0.0, 500.0),
        0.0,
    );
    let events = run(&mut world, 6.0, ControlState::default());
    assert!(
        events
            .iter()
            .any(|(what, _)| *what == SystemsEvent::MoonApproach),
        "{events:?}"
    );
    let settings = settings();
    let sky_moon = crate::worldgen::sky::moon(&settings).expect("moon");
    let rig = world.player(PLAYER).expect("rig");
    assert!(!rig.flight.away());
    let tile = world.frame().tile_at(rig.x, rig.y);
    assert!(
        (tile.x - sky_moon.center_x).abs() <= sky_moon.half_width,
        "over the Moon: {tile:?}"
    );
    assert!(tile.x > sky_moon.center_x, "on the side it came down");
    assert!(
        tile.y < sky_moon.top_row && tile.y > sky_moon.top_row - 60,
        "{tile:?}"
    );
}

#[test]
fn coming_home_lands_under_the_point_the_rig_approached() {
    let mut world = world_at(30);
    let home = space::home_centre();
    let angle = 0.3_f32;
    let reach = tiles(space::HOME_APPROACH_TILES + 20.0);
    let (sin, cos) = crate::field::trig::sin_cos(angle);
    launch_to(
        &mut world,
        home.offset(sin * reach, -cos * reach),
        (-sin * 900.0, cos * 900.0),
        angle,
    );
    run(&mut world, 2.0, ControlState::default());
    let rig = world.player(PLAYER).expect("rig");
    assert!(!rig.flight.away(), "home");
    let column = world.frame().tile_at(rig.x, rig.y).x;
    let moon_column = crate::worldgen::sky::moon_column(&settings()).expect("moon");
    let expected = moon_column + 3_000;
    assert!((column - expected).abs() <= 3, "{column} vs {expected}");
    // Level on arrival: home's up there became the sky's up.
    assert!(rig.attitude.orientation.abs() < 0.05);
}

#[test]
fn an_idle_rig_at_the_departure_point_holds_station_and_never_drifts_home() {
    for hz in [30, 60] {
        let mut world = world_at(hz);
        let start = space::home_launch();
        launch_to(&mut world, start, (0.0, 0.0), 0.3);
        let events = run(&mut world, 90.0, ControlState::default());
        assert!(
            events.contains(&(SystemsEvent::ParkingHold, 0)),
            "{events:?}"
        );
        let after = flight(&world);
        assert!(after.away(), "still in space");
        assert!(
            after.pos.span(start) < TILE_SIZE,
            "{}",
            after.pos.span(start)
        );
        assert!((world.player(PLAYER).expect("rig").attitude.orientation - 0.3).abs() < 1e-3);
    }
}

#[test]
fn space_systems_agree_at_every_tick_rate_and_twice_over() {
    let mouth = first_body(BodyKind::Wormhole, 12);
    let rock = belt_rock();
    let play = |hz: u32| {
        let mut world = world_at(hz);
        let pos = rock.centre.offset(0.0, -(rock.radius + RIG_CONTACT + 1.0));
        launch_to(&mut world, pos, (0.0, 0.0), 0.0);
        run(&mut world, 2.0, ControlState::default());
        let start = mouth.centre.offset(mouth.radius + tiles(30.0), 0.0);
        launch_to(&mut world, start, (-900.0, 0.0), up_angle(-1.0, 0.0));
        run(&mut world, 6.0, ControlState::default());
        world
    };
    let a = play(30);
    let b = play(30);
    assert_eq!(a.simulation_state_hash(), b.simulation_state_hash());
    let c = play(60);
    let (fa, fc) = (flight(&a), flight(&c));
    assert_eq!(a.space_systems().wormholes, c.space_systems().wormholes);
    assert_eq!(a.space_systems().mined, c.space_systems().mined);
    assert!(fa.pos.span(fc.pos) < tiles(2.0), "{}", fa.pos.span(fc.pos));
}
