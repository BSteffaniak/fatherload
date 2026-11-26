//! Unit tests of the flight model (pure functions): rotation in vacuum, gravity that never
//! turns a rig, the cruise and warp timings, re-entry heat, landing, the placeholder galaxy.

use super::{
    BodyRef, FlightControls, FlightEvent, FlightPlace, RigFlight, StarDrive, WarpRun,
    nav::{self, NavTarget},
    physics::{
        CRUISE_MAX_SPEED, FlightInput, FlightOutcome, FlightRig, WARP_MAX_MILLIS, step_flight,
        warp_duration_millis, warp_position,
    },
    space::{
        self, BodyKind, FlightBody, GALAXY_SECTOR_TILES, GeneratedSpace, HOME_SYSTEM, SpaceQuery,
        SystemRef, tiles,
    },
};
use crate::{
    attitude::RigAttitude, field::trig::wrap_angle, position::WorldPos, star_chart::StarClass,
};

const SYSTEM: SystemRef = SystemRef {
    id: 7,
    sector: (3, 0),
};

fn rig() -> FlightRig {
    FlightRig {
        engine_power: 1.0,
        cargo_ratio: 0.0,
        shield: 0,
        auto_land: false,
        fuel: 10_000.0,
        now_millis: 0,
    }
}

fn planet(index: u8, centre: WorldPos, radius_tiles: f32, air_tiles: f32) -> FlightBody {
    let radius = tiles(radius_tiles);
    FlightBody {
        body: BodyRef {
            system: SYSTEM,
            index,
        },
        kind: BodyKind::Planet,
        centre,
        radius,
        surface_gravity: 300.0,
        atmosphere: tiles(air_tiles),
        approach: radius + tiles(air_tiles.max(300.0)) + tiles(200.0),
        corona_heat: 0.0,
        port_angle: 0.0,
        class: StarClass::Yellow,
        extra: crate::flight::systems::BodyExtra::None,
    }
}

fn free(pos: WorldPos) -> RigFlight {
    RigFlight {
        place: FlightPlace::Space,
        pos,
        air: 600.0,
        ..RigFlight::default()
    }
}

/// Runs `seconds` of flight at `hz` with `input`, returning every outcome.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test durations"
)]
fn fly(
    flight: &mut RigFlight,
    attitude: &mut RigAttitude,
    input: FlightInput,
    bodies: &[FlightBody],
    hz: u16,
    seconds: f32,
) -> Vec<FlightOutcome> {
    let ticks = (seconds * f32::from(hz)).round() as u32;
    (0..ticks)
        .map(|_| {
            step_flight(
                flight,
                attitude,
                &input,
                &rig(),
                bodies,
                1.0 / f32::from(hz),
            )
        })
        .collect()
}

#[test]
fn spin_keeps_going_in_vacuum_until_the_player_counter_thrusts() {
    let mut flight = free(WorldPos::default());
    let mut attitude = RigAttitude::LEVEL;
    let spin_up = FlightInput {
        rotate: 1.0,
        ..FlightInput::default()
    };
    fly(&mut flight, &mut attitude, spin_up, &[], 60, 0.25);
    let spin = attitude.angular_velocity;
    assert!(spin > 1.0, "the thrusters spun it up: {attitude:?}");
    // Released: no drag in vacuum, the spin is conserved (angular momentum), for ten seconds.
    let mut turned = 0.0;
    for _ in 0..600 {
        let before = attitude.orientation;
        fly(
            &mut flight,
            &mut attitude,
            FlightInput::default(),
            &[],
            60,
            1.0 / 60.0,
        );
        turned += wrap_angle(attitude.orientation - before);
        assert!(
            (attitude.angular_velocity - spin).abs() < 1e-4,
            "spin conserved: {attitude:?} vs {spin}"
        );
    }
    assert!(
        turned > std::f32::consts::TAU * 2.0,
        "full turns: {turned} rad"
    );
    // Counter-thrust stops it.
    let counter = FlightInput {
        rotate: -1.0,
        ..FlightInput::default()
    };
    let mut ticks = 0;
    while attitude.angular_velocity > 0.0 && ticks < 600 {
        fly(&mut flight, &mut attitude, counter, &[], 60, 1.0 / 60.0);
        ticks += 1;
    }
    assert!(
        ticks < 120,
        "counter-thrust brakes the spin in under two seconds"
    );
}

#[test]
fn spin_tops_out_and_a_laden_bay_spins_up_slower() {
    let spin_after = |cargo_ratio: f32, seconds: f32| {
        let mut flight = free(WorldPos::default());
        let mut attitude = RigAttitude::LEVEL;
        let input = FlightInput {
            rotate: 1.0,
            ..FlightInput::default()
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "test ticks"
        )]
        for _ in 0..(seconds * 60.0) as u32 {
            step_flight(
                &mut flight,
                &mut attitude,
                &input,
                &FlightRig {
                    cargo_ratio,
                    ..rig()
                },
                &[],
                1.0 / 60.0,
            );
        }
        attitude.angular_velocity
    };
    let empty = spin_after(0.0, 0.2);
    let laden = spin_after(1.0, 0.2);
    assert!(
        laden < empty * 0.6,
        "a full bay spins up slower: {laden} vs {empty}"
    );
    let top = spin_after(0.0, 5.0);
    assert!(
        (top - super::MAX_SPACE_SPIN).abs() < 0.01,
        "the thrusters top out: {top}"
    );
}

#[test]
fn rotation_burns_fuel() {
    let mut flight = free(WorldPos::default());
    let mut attitude = RigAttitude::LEVEL;
    let outcomes = fly(
        &mut flight,
        &mut attitude,
        FlightInput {
            rotate: 1.0,
            ..FlightInput::default()
        },
        &[],
        60,
        1.0,
    );
    let burned: f32 = outcomes.iter().map(|outcome| outcome.fuel_burned).sum();
    assert!(
        super::SPACE_FUEL_PER_SECOND
            .mul_add(-super::ROTATION_FUEL_SHARE, burned)
            .abs()
            < 0.01,
        "{burned}"
    );
}

#[test]
fn gravity_pulls_but_never_turns_the_rig() {
    let centre = WorldPos::default();
    let world = planet(1, centre, 1_000.0, 0.0);
    // Tilted 0.9 rad, 400 tiles over the surface, idle.
    let mut flight = free(centre.offset(0.0, -tiles(1_400.0)));
    let tilt = crate::attitude::quantize_angle(0.9);
    let mut attitude = RigAttitude {
        orientation: tilt,
        angular_velocity: 0.0,
    };
    fly(
        &mut flight,
        &mut attitude,
        FlightInput::default(),
        &[world],
        60,
        3.0,
    );
    assert!(
        flight.velocity_y > 100.0,
        "falling toward the world: {flight:?}"
    );
    #[allow(clippy::float_cmp, reason = "exactly unturned")]
    {
        assert_eq!(attitude.orientation, tilt, "gravity never rotates the rig");
        assert_eq!(attitude.angular_velocity, 0.0);
    }
    // Sideways of the world, the same.
    let mut side = free(centre.offset(tiles(1_400.0), 0.0));
    let mut level = RigAttitude::LEVEL;
    fly(
        &mut side,
        &mut level,
        FlightInput::default(),
        &[world],
        60,
        3.0,
    );
    assert!(side.velocity_x < -100.0, "pulled sideways: {side:?}");
    assert!(level.is_level(), "still upright on screen: {level:?}");
}

#[test]
fn gravity_blends_between_two_bodies() {
    let a = planet(1, WorldPos::default(), 1_000.0, 0.0);
    let b = planet(
        2,
        WorldPos::default().offset(tiles(10_000.0), 0.0),
        1_000.0,
        0.0,
    );
    let middle = WorldPos::default().offset(tiles(5_000.0), 0.0);
    let ((gx, _), _) = space::gravity_at(&[a, b], middle);
    assert!(gx.abs() < 1e-3, "balanced halfway: {gx}");
    let nearer_b = WorldPos::default().offset(tiles(7_000.0), 0.0);
    let ((gx, _), strongest) = space::gravity_at(&[a, b], nearer_b);
    assert!(gx > 0.0, "pulled toward the nearer body");
    assert_eq!(strongest, Some(1));
}

#[test]
fn thrust_is_craft_relative_and_screen_relative_is_an_option() {
    // Turned a quarter clockwise: forward pushes right on the chart.
    let mut flight = free(WorldPos::default());
    let mut attitude = RigAttitude {
        orientation: std::f32::consts::FRAC_PI_2,
        angular_velocity: 0.0,
    };
    let forward = FlightInput {
        forward: 1.0,
        ..FlightInput::default()
    };
    fly(&mut flight, &mut attitude, forward, &[], 60, 0.5);
    assert!(
        flight.velocity_x > 100.0 && flight.velocity_y.abs() < 1.0,
        "{flight:?}"
    );
    // The screen-relative scheme pushes up the chart whatever the rig's turn.
    let mut chart = free(WorldPos::default());
    let mut turned = attitude;
    let screen = FlightInput {
        controls: FlightControls {
            screen_relative: true,
            auto_land: false,
        },
        ..forward
    };
    fly(&mut chart, &mut turned, screen, &[], 60, 0.5);
    assert!(
        chart.velocity_y < -100.0 && chart.velocity_x.abs() < 1.0,
        "{chart:?}"
    );
}

/// Cruises from just outside `a`'s approach sphere toward `b` until the drive drops out,
/// returning the seconds it took.
fn cruise_time(gap_tiles: f32, hz: u16) -> f32 {
    let a = planet(1, WorldPos::default(), 2_000.0, 600.0);
    let b = planet(
        2,
        WorldPos::default().offset(0.0, -tiles(gap_tiles)),
        2_000.0,
        600.0,
    );
    let bodies = [a, b];
    let mut flight = free(WorldPos::default().offset(0.0, -(a.approach + tiles(10.0))));
    flight.drive = StarDrive::Cruise { speed: 0.0 };
    let mut attitude = RigAttitude::LEVEL;
    let input = FlightInput {
        forward: 1.0,
        ..FlightInput::default()
    };
    let mut seconds = 0.0;
    while flight.drive != StarDrive::Off && seconds < 600.0 {
        step_flight(
            &mut flight,
            &mut attitude,
            &input,
            &rig(),
            &bodies,
            1.0 / f32::from(hz),
        );
        seconds += 1.0 / f32::from(hz);
    }
    assert!(
        matches!(flight.place, FlightPlace::Approach(body) if body == b.body),
        "dropped out at the far world: {flight:?}"
    );
    seconds
}

#[test]
fn crossing_a_star_system_on_cruise_takes_a_minute_or_two() {
    for gap in [120_000.0, 250_000.0] {
        let seconds = cruise_time(gap, 60);
        assert!(
            (60.0..=120.0).contains(&seconds),
            "{gap} tiles took {seconds} s"
        );
    }
    // Tick-rate safe.
    let reference = cruise_time(120_000.0, 60);
    for hz in [30, 120] {
        let other = cruise_time(120_000.0, hz);
        assert!(
            (other - reference).abs() < 1.0,
            "{hz} Hz: {other} vs {reference}"
        );
    }
}

#[test]
fn cruise_slows_near_bodies_and_never_passes_its_top_speed() {
    assert!((super::physics::cruise_limit(f32::MAX) - CRUISE_MAX_SPEED).abs() < 1.0);
    assert!(
        super::physics::cruise_limit(tiles(1_000.0))
            < super::physics::cruise_limit(tiles(50_000.0))
    );
}

#[test]
fn a_warp_really_crosses_the_distance_in_its_time() {
    let origin = WorldPos::default();
    let arrival = space::advance(origin, 40.0 * 1_048_576.0 * 32.0, 0.0);
    let distance = origin.distance(arrival);
    let duration = warp_duration_millis(f64::from(distance));
    assert!(duration <= WARP_MAX_MILLIS);
    assert_eq!(warp_duration_millis(0.0), super::physics::WARP_BASE_MILLIS);
    let run = WarpRun {
        origin,
        arrival,
        target: SYSTEM,
        start: 1_000,
        duration_millis: duration,
    };
    let (start, _, done) = warp_position(&run, 1_000);
    assert_eq!(start, origin);
    assert!(!done);
    let (middle, (vx, _), _) = warp_position(&run, 1_000 + u64::from(duration) / 2);
    let (dx, _) = origin.delta_f64(middle);
    assert!(
        (dx / f64::from(distance) - 0.5).abs() < 1e-6,
        "halfway at half time"
    );
    assert!(vx > CRUISE_MAX_SPEED * 10.0, "far faster than cruise: {vx}");
    let (end, _, done) = warp_position(&run, 1_000 + u64::from(duration));
    assert!(done);
    assert!(end.distance(arrival) < 1.0, "{end:?} vs {arrival:?}");
    // Eased: every step forward, never back.
    let mut last = 0.0;
    for step in 0..=100 {
        let (pos, _, _) = warp_position(&run, 1_000 + u64::from(duration) * step / 100);
        let (dx, _) = origin.delta_f64(pos);
        assert!(dx >= last);
        last = dx;
    }
}

#[test]
fn a_steep_fast_dive_burns_the_hull_and_a_shallow_glide_does_not() {
    let world = planet(1, WorldPos::default(), 1_500.0, 600.0);
    let dive = |vx: f32, vy: f32, shield: u8| {
        let mut flight = free(WorldPos::default().offset(0.0, -(world.radius + tiles(250.0))));
        flight.velocity_x = vx;
        flight.velocity_y = vy;
        let mut attitude = RigAttitude::LEVEL;
        let mut damage = 0.0;
        let mut peak: f32 = 0.0;
        for _ in 0..120 {
            let outcome = step_flight(
                &mut flight,
                &mut attitude,
                &FlightInput::default(),
                &FlightRig { shield, ..rig() },
                &[world],
                1.0 / 60.0,
            );
            damage += outcome.hull_damage;
            peak = peak.max(flight.heat);
            // Hold the speed (a dive under power).
            flight.velocity_x = vx;
            flight.velocity_y = vy;
        }
        (damage, peak)
    };
    let (steep, steep_heat) = dive(0.0, 2_600.0, 0);
    assert!(steep_heat > super::HEAT_BURN, "{steep_heat}");
    assert!(steep > 5.0, "a steep fast dive burns: {steep}");
    let (shallow, shallow_heat) = dive(1_300.0, 60.0, 0);
    assert!(shallow_heat < super::HEAT_WARNING, "{shallow_heat}");
    assert!(shallow.abs() < f32::EPSILON, "{shallow}");
    let (shielded, _) = dive(0.0, 2_600.0, super::REENTRY_SHIELD_MAX);
    assert!(shielded < steep * 0.25, "the shield: {shielded} vs {steep}");
}

fn drop_onto(
    world: &FlightBody,
    speed: f32,
    tilt: f32,
) -> (RigFlight, RigAttitude, Vec<FlightOutcome>) {
    let mut flight = free(WorldPos::default().offset(0.0, -(world.radius + 20.0)));
    flight.velocity_y = speed;
    let mut attitude = RigAttitude {
        orientation: tilt,
        angular_velocity: 0.0,
    };
    let outcomes = fly(
        &mut flight,
        &mut attitude,
        FlightInput::default(),
        &[*world],
        60,
        0.2,
    );
    (flight, attitude, outcomes)
}

#[test]
fn a_slow_upright_touchdown_lands_and_a_tilted_one_scrapes_and_tips() {
    let world = planet(1, WorldPos::default(), 800.0, 0.0);
    let (flight, _, outcomes) = drop_onto(&world, 200.0, 0.1);
    assert_eq!(flight.place, FlightPlace::Landed(world.body));
    assert!(outcomes.iter().all(|outcome| outcome.hull_damage == 0.0));
    assert!(outcomes.iter().any(|outcome| {
        outcome
            .events
            .iter()
            .any(|(event, _)| *event == FlightEvent::Landed)
    }));
    let (tipped, attitude, outcomes) = drop_onto(&world, 200.0, 1.2);
    assert!(
        !matches!(tipped.place, FlightPlace::Landed(_)),
        "{tipped:?}"
    );
    let damage: f32 = outcomes.iter().map(|outcome| outcome.hull_damage).sum();
    assert!(damage > 0.0, "the hull scraped");
    assert!(attitude.angular_velocity > 0.5, "tipped over: {attitude:?}");
    let (crashed, _, outcomes) = drop_onto(&world, 900.0, 0.0);
    assert!(!matches!(crashed.place, FlightPlace::Landed(_)));
    let damage: f32 = outcomes.iter().map(|outcome| outcome.hull_damage).sum();
    assert!(damage > 40.0, "a fast touchdown crashes: {damage}");
}

#[test]
fn a_landed_rig_rests_then_takes_off_on_its_thrusters() {
    let world = planet(1, WorldPos::default(), 800.0, 0.0);
    let (mut flight, mut attitude, _) = drop_onto(&world, 200.0, 0.2);
    assert_eq!(flight.place, FlightPlace::Landed(world.body));
    // Resting: it settles level on flat ground and does not sink or slide.
    let rest = flight.pos;
    fly(
        &mut flight,
        &mut attitude,
        FlightInput::default(),
        &[world],
        60,
        2.0,
    );
    assert!(flight.pos.distance(rest) < 1.0);
    assert!(attitude.orientation.abs() < 0.01, "{attitude:?}");
    let outcomes = fly(
        &mut flight,
        &mut attitude,
        FlightInput {
            forward: 1.0,
            ..FlightInput::default()
        },
        &[world],
        60,
        1.0,
    );
    assert!(
        matches!(flight.place, FlightPlace::Approach(_)),
        "{flight:?}"
    );
    assert!(outcomes.iter().any(|outcome| {
        outcome
            .events
            .iter()
            .any(|(event, _)| *event == FlightEvent::TookOff)
    }));
}

#[test]
fn auto_land_holds_the_last_metres_only_when_fitted_and_on() {
    let world = planet(1, WorldPos::default(), 800.0, 0.0);
    let land = |fitted: bool, on: bool| {
        let mut flight = free(WorldPos::default().offset(0.0, -(world.radius + tiles(9.5))));
        flight.velocity_y = 350.0;
        let mut attitude = RigAttitude {
            orientation: 0.3,
            angular_velocity: 0.0,
        };
        let input = FlightInput {
            controls: FlightControls {
                auto_land: on,
                screen_relative: false,
            },
            ..FlightInput::default()
        };
        let mut damage = 0.0;
        for _ in 0..240 {
            damage += step_flight(
                &mut flight,
                &mut attitude,
                &input,
                &FlightRig {
                    auto_land: fitted,
                    ..rig()
                },
                &[world],
                1.0 / 60.0,
            )
            .hull_damage;
        }
        (flight.place, damage, attitude)
    };
    let (place, damage, attitude) = land(true, true);
    assert_eq!(place, FlightPlace::Landed(world.body));
    assert!(damage.abs() < f32::EPSILON, "a soft touchdown: {damage}");
    assert!(attitude.orientation.abs() < 0.01, "levelled: {attitude:?}");
    for (fitted, on) in [(false, true), (true, false)] {
        let (_, damage, _) = land(fitted, on);
        assert!(
            damage > 0.0,
            "a hard landing without the assist: {fitted} {on}"
        );
    }
}

#[test]
fn assists_and_flight_switches_are_off_by_default() {
    let controls = crate::ControlState::default();
    assert_eq!(controls.flight(), FlightControls::default());
    assert!(!controls.assists().gyro && !controls.assists().horizon);
    let gear = crate::protection::HazardGear::default();
    assert_eq!(gear.flight, super::FlightGear::default());
    assert_eq!(gear.attitude.gyro_stabiliser, 0);
    assert_eq!(gear.attitude.horizon_hold, 0);
    let bits = FlightControls {
        auto_land: true,
        screen_relative: true,
    };
    assert_eq!(FlightControls::from_bits(bits.bits()), bits);
    assert_eq!(controls.with_flight(bits).flight(), bits);
}

#[test]
fn the_generated_galaxy_is_deterministic_and_honours_the_charts() {
    let space = GeneratedSpace::new(0xF00D);
    let again = GeneratedSpace::new(0xF00D);
    let home = space.system_at((0, 0)).expect("home");
    assert_eq!(home.system, HOME_SYSTEM);
    let bodies = space.bodies_of(&home);
    assert!(
        bodies
            .iter()
            .any(|body| body.body == space::HOME_BODY && body.kind == BodyKind::Home)
    );
    assert!(bodies.iter().any(|body| body.kind == BodyKind::Station));
    // The home world and the Moon are the space generator's, and their ids map both ways.
    assert_eq!(
        space.space_body(space::HOME_BODY),
        Some(crate::space::HOME_WORLD_BODY)
    );
    assert_eq!(
        space
            .body_ref(crate::space::MOON_BODY)
            .map(|body| body.index),
        Some(space::MOON_INDEX)
    );
    // A chart names a real system of the generator, with its class and worlds.
    let target = crate::star_chart::chart_at_site(
        0xF00D,
        40,
        9_900,
        500,
        crate::ids::SimulationTick::new(0),
    );
    let charted_ref = space.chart_system(&target);
    let charted = space.system(charted_ref).expect("charted");
    assert_eq!(charted.class, target.class);
    assert_eq!(usize::from(charted.worlds), usize::from(target.worlds));
    let charted_bodies = space.bodies_of(&charted);
    for body in &charted_bodies {
        if body.landable() {
            assert!(
                body.radius >= tiles(400.0),
                "landable bodies are 400+ tiles"
            );
            // Every landable body has round ground to land on.
            let id = space.space_body(body.body).expect("a generated body");
            assert!(crate::place::planet_params_for(0xF00D, id).is_some());
            assert_eq!(space.body_ref(id), Some(body.body));
        }
    }
    for sector in [(1, 1), (-4, 9), (30, -2)] {
        assert_eq!(space.system_at(sector), again.system_at(sector));
    }
    // Every system's star lies in its own sector, and resolves from its reference.
    for gx in -3..3 {
        for gy in -3..3 {
            for system in space.systems_near(space::galaxy_sector_origin((gx, gy)), 0) {
                assert_eq!(system.system.sector, (gx, gy));
                assert_eq!(space.system(system.system), Some(system));
            }
        }
    }
    let launch = space::home_launch();
    let home_world = bodies
        .iter()
        .find(|body| body.body == space::HOME_BODY)
        .expect("home world");
    assert!(launch.distance(home_world.centre) > home_world.approach);
    for body in &bodies {
        if body.kind == BodyKind::Home {
            assert!(
                launch.distance(body.centre) > body.approach,
                "the departure point is outside every home body's approach sphere"
            );
        }
    }
    assert_eq!(
        NavTarget::System(charted.system).position(&space),
        Some(charted.star)
    );
    let _ = GALAXY_SECTOR_TILES;
}

#[test]
fn pings_reach_further_with_the_nav_suite_and_less_in_a_nebula() {
    assert!(nav::ping_range(3, false) > nav::ping_range(0, false) * 10.0);
    assert!((nav::ping_range(1, true) - nav::ping_range(1, false) * nav::NEBULA_FOG).abs() < 1.0);
    assert_eq!(nav::format_eta(80.0), "1m 20s");
    assert_eq!(nav::format_eta(42.0), "42s");
    let near = nav::eta_seconds(f64::from(tiles(120_000.0)), false);
    assert!((40.0..150.0).contains(&near), "{near}");
}
