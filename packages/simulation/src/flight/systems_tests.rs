//! Pure tests of space's things (Wave 12 systems): flare cycles, radiance, shelter, glare,
//! black hole warnings, wormhole exits, fog, drill contact, the beam, comet tails, the
//! parking hold, the Moon's handover and the crew's record.

#![allow(
    clippy::float_cmp,
    reason = "exact zeros and ones are the answers here"
)]

use crate::{
    attitude::RigAttitude,
    flight::{
        BodyRef, FlightPlace, RigFlight, StarDrive, SystemRef,
        physics::{
            FlightInput, FlightRig, MARK_HOLDING, MOON_LAND_HEIGHT, PARK_HOLD_SPEED, axes,
            step_flight, up_angle,
        },
        space::{self, BodyKind, FlightBody, MOON_BODY_REF, tiles},
        systems::{
            self, BodyExtra, CometFacts, CometKey, FLARE_BURST_MILLIS, FLARE_WARNING_MILLIS,
            FlarePhase, HoleFacts, HoleWarning, KnownWormhole, MouthFacts, RockFacts, StarFacts,
            SystemsShared,
        },
    },
    position::WorldPos,
    space::AsteroidId,
    star_chart::StarClass,
    terrain::TILE_SIZE,
};

const SYSTEM: SystemRef = SystemRef {
    id: 7,
    sector: (3, 4),
};

fn body(kind: BodyKind, centre: WorldPos, radius_tiles: f32, extra: BodyExtra) -> FlightBody {
    FlightBody {
        body: BodyRef {
            system: SYSTEM,
            index: 0,
        },
        kind,
        centre,
        radius: tiles(radius_tiles),
        surface_gravity: 0.0,
        atmosphere: 0.0,
        approach: 0.0,
        corona_heat: 0.0,
        port_angle: 0.0,
        class: StarClass::Yellow,
        extra,
    }
}

fn star(centre: WorldPos, period: u32, phase: u32) -> FlightBody {
    body(
        BodyKind::Star,
        centre,
        20_000.0,
        BodyExtra::Star(StarFacts {
            heat_radius: tiles(50_000.0),
            light: 1_000,
            flare_period_s: period,
            flare_phase_s: phase,
        }),
    )
}

fn origin() -> WorldPos {
    WorldPos::tile_center(1_000_000, -2_000_000)
}

fn rig(now: u64) -> FlightRig {
    FlightRig {
        engine_power: 1.0,
        cargo_ratio: 0.0,
        shield: 0,
        auto_land: false,
        fuel: 10_000.0,
        now_millis: now,
    }
}

#[test]
fn a_flare_is_telegraphed_burns_eased_and_never_comes_from_a_white_dwarf() {
    let facts = StarFacts {
        heat_radius: 1.0,
        light: 1_000,
        flare_period_s: 600,
        flare_phase_s: 100,
    };
    // A flare starts at t = 100 s, then every 600 s.
    let start = 100_000;
    assert!(matches!(
        systems::flare_at(&facts, start - FLARE_WARNING_MILLIS - 1),
        FlarePhase::Quiet { .. }
    ));
    assert_eq!(
        systems::flare_at(&facts, start - 4_000),
        FlarePhase::Warning { in_millis: 4_000 }
    );
    let FlarePhase::Burst { strength, .. } = systems::flare_at(&facts, start + 100) else {
        panic!("bursting");
    };
    assert!(strength > 0.0 && strength < 0.2, "eased in: {strength}");
    let peak = systems::flare_at(&facts, start + FLARE_BURST_MILLIS / 2).strength();
    assert!((peak - 1.0).abs() < 1e-6, "peak {peak}");
    assert!(matches!(
        systems::flare_at(&facts, start + FLARE_BURST_MILLIS),
        FlarePhase::Quiet { .. }
    ));
    // The cycle repeats exactly.
    assert_eq!(
        systems::flare_at(&facts, start + 600_000 + 2_000),
        systems::flare_at(&facts, start + 2_000)
    );
    let white = StarFacts {
        flare_period_s: 0,
        ..facts
    };
    for now in [0, start, start + 1_000, 9_999_999] {
        assert_eq!(systems::flare_at(&white, now).strength(), 0.0);
    }
}

#[test]
fn radiance_burns_inside_the_heat_radius_and_lets_a_rig_hang_at_twice_it() {
    let sun = star(origin(), 0, 0);
    let BodyExtra::Star(facts) = sun.extra else {
        unreachable!()
    };
    let at = |radii: f32| origin().offset(facts.heat_radius * radii, 0.0);
    let inside = systems::radiant_heat(&[sun], at(0.8));
    let edge = systems::radiant_heat(&[sun], at(1.0));
    let twice = systems::radiant_heat(&[sun], at(2.0));
    assert!(inside > edge && edge > twice);
    assert!(edge > crate::flight::HEAT_COOL_PER_SECOND * 3.0, "{edge}");
    assert!(twice < crate::flight::HEAT_COOL_PER_SECOND, "{twice}");
    assert_eq!(systems::radiant_heat(&[sun], at(10.0)), 0.0);
    // A blue star's light runs hotter than the home sun's, a red dwarf's cooler.
    let tinted = |light: u16| {
        let mut tinted = sun;
        tinted.extra = BodyExtra::Star(StarFacts { light, ..facts });
        systems::radiant_heat(&[tinted], at(1.0))
    };
    assert!(tinted(2_200) > edge && tinted(450) < edge);
}

#[test]
fn a_body_between_the_star_and_the_rig_shelters_it_from_a_flare() {
    let sun = star(origin(), 600, 0);
    let BodyExtra::Star(facts) = sun.extra else {
        unreachable!()
    };
    let planet_at = origin().offset(facts.heat_radius * 1.5, 0.0);
    let planet = body(BodyKind::Planet, planet_at, 3_000.0, BodyExtra::None);
    let bodies = [sun, planet];
    let behind = planet_at.offset(tiles(3_500.0), 0.0);
    let open = planet_at.offset(tiles(3_500.0), tiles(9_000.0));
    assert!(systems::shadowed(&bodies, origin(), behind, 0));
    assert!(!systems::shadowed(&bodies, origin(), open, 0));
    // At the flare's peak the open rig burns, the sheltered one does not.
    let peak = FLARE_BURST_MILLIS / 2;
    let (hit, near) = systems::flare_heat(&bodies, open, peak);
    let (sheltered, near_behind) = systems::flare_heat(&bodies, behind, peak);
    assert!(hit > 1.0, "{hit}");
    assert_eq!(sheltered, 0.0);
    assert!(!near.expect("in reach").sheltered);
    assert!(near_behind.expect("in reach").sheltered);
    // Out of reach, no forecast at all.
    let far = origin().offset(facts.heat_radius * 5.0, 0.0);
    assert!(systems::flare_heat(&bodies, far, peak).1.is_none());
}

#[test]
fn glare_is_strongest_near_bright_stars_and_never_over_one() {
    let sun = star(origin(), 0, 0);
    let mut blue = sun;
    let BodyExtra::Star(facts) = sun.extra else {
        unreachable!()
    };
    blue.extra = BodyExtra::Star(StarFacts {
        light: 2_200,
        ..facts
    });
    let at = origin().offset(sun.radius * 9.0, 0.0);
    let home_glare = systems::star_glare(&[sun], at, 0);
    let blue_glare = systems::star_glare(&[blue], at, 0);
    assert!(blue_glare > home_glare && home_glare > 0.0);
    for distance in [1.0, 1.5, 3.0] {
        assert!(
            systems::star_glare(&[blue], origin().offset(sun.radius * distance, 0.0), 0) <= 1.0
        );
    }
    assert_eq!(
        systems::star_glare(&[sun], origin().offset(sun.radius * 100.0, 0.0), 0),
        0.0
    );
}

fn hole() -> FlightBody {
    let mut hole = body(
        BodyKind::BlackHole,
        origin(),
        1_000.0,
        BodyExtra::Hole(HoleFacts {
            lensing: tiles(12_000.0),
            time_dilation: 0.25,
        }),
    );
    hole.surface_gravity = 40.0 * space::FLIGHT_G_PX;
    hole.approach = tiles(12_000.0);
    hole
}

#[test]
fn a_black_holes_warnings_deepen_toward_the_point_of_no_return() {
    let hole = hole();
    let thrust = crate::flight::SPACE_THRUST;
    let no_return = systems::no_return_radius(&hole, thrust);
    assert!(no_return > hole.radius * 2.0 && no_return < tiles(12_000.0));
    let level = |distance: f32| {
        systems::hole_warning(&[hole], origin().offset(distance, 0.0), thrust)
            .map(|(level, _)| level)
    };
    assert_eq!(level(tiles(12_500.0)), None);
    assert_eq!(level(tiles(11_500.0)), Some(HoleWarning::Well));
    assert_eq!(level(no_return * 1.2), Some(HoleWarning::Strain));
    assert_eq!(level(no_return * 0.95), Some(HoleWarning::NoReturn));
    // At the strain line full thrust still climbs out: the warning comes in time.
    let strain = no_return / systems::HOLE_STRAIN_SHARE.sqrt();
    assert!(hole.gravity_at(strain * 0.99) < thrust);
}

#[test]
fn crossing_the_horizon_or_a_mouth_is_reported_and_stops_the_step() {
    let hole = hole();
    let mut flight = RigFlight {
        place: FlightPlace::Space,
        pos: origin().offset(hole.radius + 3.0, 0.0),
        velocity_x: -900.0,
        ..RigFlight::default()
    };
    let mut attitude = RigAttitude::LEVEL;
    let outcome = step_flight(
        &mut flight,
        &mut attitude,
        &FlightInput::default(),
        &rig(0),
        &[hole],
        1.0 / 30.0,
    );
    assert_eq!(outcome.swallowed, Some(0));
    let mouth = body(
        BodyKind::Wormhole,
        origin(),
        400.0,
        BodyExtra::Mouth(MouthFacts {
            exit: origin().offset(tiles(9.0e7), 0.0),
            exit_radius: tiles(500.0),
            sector: (0, 0),
            exit_sector: (21, 0),
        }),
    );
    let mut flight = RigFlight {
        place: FlightPlace::Space,
        pos: origin().offset(mouth.radius + 10.0, 0.0),
        velocity_x: -900.0,
        ..RigFlight::default()
    };
    let outcome = step_flight(
        &mut flight,
        &mut attitude,
        &FlightInput::default(),
        &rig(0),
        &[mouth],
        1.0 / 30.0,
    );
    assert_eq!(outcome.mouth, Some(0));
    // The exit is past the far mouth's rim along the heading, moving on.
    let BodyExtra::Mouth(facts) = mouth.extra else {
        unreachable!()
    };
    let (exit, (vx, vy)) = systems::transit_exit(&facts, (-900.0, 0.0));
    let out = facts.exit.span(exit);
    assert!(out > facts.exit_radius + tiles(200.0), "{out}");
    assert!(vx < 0.0 && vy.abs() < 1e-3);
    assert!(facts.exit.delta(exit).0 < 0.0, "out along the heading");
}

#[test]
fn fog_cuts_the_scanner_and_fades_markers() {
    assert_eq!(systems::fog_range_share(0), 1.0);
    assert!((systems::fog_range_share(1_000) - 0.3).abs() < 1e-6);
    assert!(systems::marker_visibility(500) < 1.0 && systems::marker_visibility(500) > 0.15);
    assert!((systems::marker_visibility(2_000) - 0.15).abs() < 1e-6);
}

fn rock(centre: WorldPos, radius: f32) -> FlightBody {
    let mut rock = body(
        BodyKind::Asteroid,
        centre,
        radius,
        BodyExtra::Rock(RockFacts {
            id: AsteroidId::default(),
            ore: crate::space::AsteroidOre::Iridium,
            yield_units: 9,
        }),
    );
    rock.body.index = space::ROCK_INDEX;
    rock
}

#[test]
fn the_drill_bites_only_with_the_underside_on_the_rock_and_the_beam_from_the_nose() {
    let rock = rock(origin(), 40.0);
    let surface = rock.radius + crate::flight::RIG_CONTACT;
    // Resting on top of the rock, upright: the rock is under the rig.
    let on_top = origin().offset(0.0, -surface);
    assert_eq!(
        systems::drill_contact(&[rock], on_top, 0.0, crate::flight::RIG_CONTACT),
        Some(0)
    );
    // Upside down on it, the nose is on the rock: no bite.
    assert_eq!(
        systems::drill_contact(
            &[rock],
            on_top,
            std::f32::consts::PI,
            crate::flight::RIG_CONTACT
        ),
        None
    );
    // A tile off the surface: no bite either.
    assert_eq!(
        systems::drill_contact(
            &[rock],
            on_top.offset(0.0, -TILE_SIZE),
            0.0,
            crate::flight::RIG_CONTACT
        ),
        None
    );
    // The beam: ten tiles off, nose toward the rock hits; turned away it does not.
    let off = origin().offset(rock.radius + tiles(10.0), 0.0);
    let facing = up_angle(-1.0, 0.0);
    assert_eq!(systems::beam_target(&[rock], off, facing), Some(0));
    assert_eq!(systems::beam_target(&[rock], off, facing + 1.2), None);
    let far = origin().offset(rock.radius + tiles(20.0), 0.0);
    assert_eq!(systems::beam_target(&[rock], far, facing), None);
    // A higher drill tier is faster.
    assert!(systems::contact_mine_seconds(6) < systems::contact_mine_seconds(1));
    // Mined out, a rock is gone from flight.
    let generated = crate::space::Asteroid {
        id: AsteroidId::default(),
        centre: crate::space::SpaceTile { x: 0, y: 0 },
        radius: 12,
        ore: crate::space::AsteroidOre::Ice,
        yield_units: 4,
    };
    assert!(systems::rock_body(&generated, 3).is_some());
    assert!(systems::rock_body(&generated, 4).is_none());
    let (up, _) = axes(0.0);
    assert!(up.1 < 0.0);
}

#[test]
fn rocks_are_solid_and_a_hard_bump_dents_the_hull() {
    let rock = rock(origin(), 40.0);
    let start = origin().offset(0.0, -(rock.radius + crate::flight::RIG_CONTACT + 20.0));
    let mut attitude = RigAttitude::LEVEL;
    let mut gentle = RigFlight {
        place: FlightPlace::Space,
        pos: start,
        velocity_y: 120.0,
        ..RigFlight::default()
    };
    let mut hard = RigFlight {
        velocity_y: 900.0,
        ..gentle
    };
    let mut damage = (0.0, 0.0);
    for _ in 0..30 {
        damage.0 += step_flight(
            &mut gentle,
            &mut attitude,
            &FlightInput::default(),
            &rig(0),
            &[rock],
            1.0 / 30.0,
        )
        .rock_damage;
        damage.1 += step_flight(
            &mut hard,
            &mut attitude,
            &FlightInput::default(),
            &rig(0),
            &[rock],
            1.0 / 30.0,
        )
        .rock_damage;
    }
    assert_eq!(damage.0, 0.0);
    assert!(damage.1 > 5.0, "{damage:?}");
    for flight in [gentle, hard] {
        assert!(flight.pos.span(origin()) >= rock.radius + crate::flight::RIG_CONTACT - 0.5);
    }
}

#[test]
fn a_comets_tail_widens_away_from_the_nucleus() {
    let comet = body(
        BodyKind::Comet,
        origin(),
        40.0,
        BodyExtra::Comet(CometFacts {
            key: CometKey::default(),
            tail_end: origin().offset(tiles(20_000.0), 0.0),
            tail_width: tiles(3_000.0),
        }),
    );
    let bodies = [comet];
    assert_eq!(
        systems::in_comet_tail(&bodies, origin().offset(tiles(15_000.0), tiles(1_000.0))),
        Some(0)
    );
    assert_eq!(
        systems::in_comet_tail(&bodies, origin().offset(tiles(2_000.0), tiles(1_000.0))),
        None
    );
    assert_eq!(
        systems::in_comet_tail(&bodies, origin().offset(tiles(-500.0), 0.0)),
        None
    );
}

#[test]
fn an_idle_rig_holds_station_where_gravity_is_weak_and_any_thrust_lets_go() {
    // A planet far below pulls a little (under the hold's limit).
    let mut planet = body(BodyKind::Planet, origin(), 3_000.0, BodyExtra::None);
    planet.surface_gravity = 300.0;
    let start = origin().offset(0.0, -tiles(7_000.0));
    let mut flight = RigFlight {
        place: FlightPlace::Space,
        pos: start,
        ..RigFlight::default()
    };
    let mut attitude = RigAttitude {
        orientation: 0.7,
        angular_velocity: 0.0,
    };
    for _ in 0..(30 * 60) {
        let outcome = step_flight(
            &mut flight,
            &mut attitude,
            &FlightInput::default(),
            &rig(0),
            &[planet],
            1.0 / 30.0,
        );
        assert!(outcome.holding);
    }
    assert!(flight.pos.span(start) < 1.0, "{}", flight.pos.span(start));
    assert!(flight.marks & MARK_HOLDING != 0);
    assert!(
        (attitude.orientation - 0.7).abs() < 1e-3,
        "the hold never turns the rig"
    );
    // Thrust lets go of it; coasting fast it does not take hold again.
    let thrust = FlightInput {
        forward: 1.0,
        ..FlightInput::default()
    };
    let outcome = step_flight(&mut flight, &mut attitude, &thrust, &rig(0), &[planet], 0.5);
    assert!(!outcome.holding);
    assert!(flight.speed() > PARK_HOLD_SPEED * 2.0);
    let outcome = step_flight(
        &mut flight,
        &mut attitude,
        &FlightInput::default(),
        &rig(0),
        &[planet],
        1.0 / 30.0,
    );
    assert!(!outcome.holding);
    // Close in (strong gravity) the hold never takes the rig: it falls to land.
    let mut low = RigFlight {
        place: FlightPlace::Space,
        pos: origin().offset(0.0, -(planet.radius + tiles(100.0))),
        ..RigFlight::default()
    };
    let outcome = step_flight(
        &mut low,
        &mut attitude,
        &FlightInput::default(),
        &rig(0),
        &[planet],
        1.0 / 30.0,
    );
    assert!(!outcome.holding);
}

#[test]
fn the_moons_sphere_zooms_in_and_only_coming_down_to_it_hands_the_rig_over() {
    let moon_centre = space::moon_centre();
    let mut moon = body(BodyKind::Home, moon_centre, 640.0, BodyExtra::None);
    moon.body = MOON_BODY_REF;
    moon.approach = moon.radius + tiles(space::MOON_APPROACH_TILES);
    let mut attitude = RigAttitude::LEVEL;
    // Skimming past inside its sphere but high over it: approach, no handover.
    let mut skim = RigFlight {
        place: FlightPlace::Space,
        pos: moon_centre.offset(moon.radius + tiles(120.0), 0.0),
        velocity_y: -600.0,
        ..RigFlight::default()
    };
    let outcome = step_flight(
        &mut skim,
        &mut attitude,
        &FlightInput::default(),
        &rig(0),
        &[moon],
        1.0 / 30.0,
    );
    assert!(!outcome.reached_moon && !outcome.reached_home);
    assert_eq!(skim.place, FlightPlace::Approach(MOON_BODY_REF));
    // Coming down to it: handed over.
    let mut down = RigFlight {
        place: FlightPlace::Space,
        pos: moon_centre.offset(0.0, -(moon.radius + MOON_LAND_HEIGHT - 4.0)),
        velocity_y: 200.0,
        ..RigFlight::default()
    };
    let outcome = step_flight(
        &mut down,
        &mut attitude,
        &FlightInput::default(),
        &rig(0),
        &[moon],
        1.0 / 30.0,
    );
    assert!(outcome.reached_moon && !outcome.reached_home);
}

#[test]
fn the_crews_record_knows_a_wormhole_from_either_end() {
    let mut shared = SystemsShared::default();
    assert!(shared.is_empty());
    let link = KnownWormhole {
        mouth: origin(),
        sector: (1, 2),
        exit: origin().offset(tiles(1.0e8), 0.0),
        exit_sector: (25, 2),
    };
    assert!(shared.found_wormhole(link));
    assert!(!shared.found_wormhole(KnownWormhole {
        mouth: link.exit,
        sector: link.exit_sector,
        exit: link.mouth,
        exit_sector: link.sector,
    }));
    assert_eq!(shared.wormhole_from((1, 2)), Some((link.exit, (25, 2))));
    assert_eq!(shared.wormhole_from((25, 2)), Some((link.mouth, (1, 2))));
    assert_eq!(shared.wormhole_from((9, 9)), None);
    assert!(!shared.is_empty());
    let drive = StarDrive::Off;
    assert!(!drive.engaged());
}
