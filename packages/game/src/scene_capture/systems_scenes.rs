//! Space's things (Wave 12 systems): a star's flare and a blue star's glare, a black hole's
//! well with the time dilation readout, a wormhole's tunnel, a nebula's fog, mining a belt rock
//! from flight, a comet's tail, coming down to the Moon from space, and the parking hold at
//! the departure point. Each is staged on the flight scenes' world from what the space
//! generator placed near home.

#![allow(
    clippy::missing_const_for_fn,
    reason = "scene staging: plain arithmetic on positions"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    flight::{
        BodyKind, FlightBody, GeneratedSpace, SpaceQuery, StarDrive, TransitRun, WarpRun,
        nav::NavTarget,
        physics::up_angle,
        space::{self, galaxy_sector_of, tiles},
        systems::{self, BodyExtra, StarFacts},
    },
    space::{SPACE_SECTOR_TILES, SpaceSector, SpaceTile},
    star_chart::StarClass,
};

use super::{
    Scene, SceneStage,
    flight_scenes::{SCENE_SEED, launch, set_waypoint, space_world},
};

/// Sectors within `reach` of home, nearest rings first.
fn sectors(reach: i32) -> impl Iterator<Item = SpaceSector> {
    (0..=reach).flat_map(move |ring| {
        (-ring..=ring).flat_map(move |y| {
            (-ring..=ring)
                .filter(move |x| x.abs().max(y.abs()) == ring)
                .map(move |x| SpaceSector::new(x, y))
        })
    })
}

/// The first flight body near home `pick` accepts.
fn find(pick: impl Fn(&FlightBody) -> bool) -> Option<FlightBody> {
    let galaxy = GeneratedSpace::new(SCENE_SEED);
    sectors(16).find_map(|sector| {
        let middle = SpaceTile {
            x: sector.origin().x + SPACE_SECTOR_TILES / 2,
            y: sector.origin().y + SPACE_SECTOR_TILES / 2,
        }
        .pos();
        galaxy
            .bodies_near(middle)
            .into_iter()
            .find(|body| galaxy_sector_of(body.centre) == galaxy_sector_of(middle) && pick(body))
    })
}

fn player(seats: &[crate::runtime::LocalSeat]) -> PlayerId {
    seats[0].player
}

/// A flaring star, the clock part way into one of its flares, the rig in the open at a
/// distance where the radiance is just over the hull's cooling.
fn star_flare(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        let Some(star) = find(|body| {
            matches!(body.extra, BodyExtra::Star(facts) if facts.flare_period_s > 0)
                && body.body.index == 0
                && body.class != StarClass::Blue
        }) else {
            return;
        };
        let BodyExtra::Star(facts) = star.extra else {
            return;
        };
        let hz = u64::from(world.tick_rate_hz());
        let period = u64::from(facts.flare_period_s) * 1_000;
        let start = u64::from(facts.flare_phase_s) * 1_000 % period + period * 2;
        world.set_simulation_tick(fatherload_simulation::SimulationTick::new(
            (start + 2_200) * hz / 1_000,
        ));
        let at = star
            .centre
            .offset(facts.heat_radius * 1.9, -facts.heat_radius * 0.4);
        let facing = up_angle(-1.0, 0.25);
        launch(
            world,
            player(seats),
            at,
            facing,
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
        if let Some(rig) = world.player_mut(player(seats)) {
            rig.hazard_gear.flight.reentry_shield = 1;
        }
    });
    stage.settle(14);
}

/// A blue star's glare close in, its flare forecast on the HUD.
fn blue_glare(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        let Some(star) = find(|body| body.kind == BodyKind::Star && body.class == StarClass::Blue)
        else {
            return;
        };
        let heat = match star.extra {
            BodyExtra::Star(StarFacts { heat_radius, .. }) => heat_radius,
            _ => star.radius * 3.0,
        };
        let at = star.centre.offset(-heat * 2.6, heat * 0.8);
        launch(
            world,
            player(seats),
            at,
            up_angle(1.0, -0.3),
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
    });
    stage.settle(10);
}

/// In a black hole's well, past the strain line: the warning, the no-return ring, the
/// dilation readout and a clock already minutes behind, burning away.
fn black_hole(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        let Some(hole) = find(|body| body.kind == BodyKind::BlackHole) else {
            return;
        };
        let thrust = fatherload_simulation::flight::SPACE_THRUST * 2.0;
        let no_return = systems::no_return_radius(&hole, thrust);
        let at = hole.centre.offset(no_return * 1.25, -no_return * 0.2);
        let (dx, dy) = hole.centre.delta(at);
        launch(
            world,
            player(seats),
            at,
            up_angle(dx, dy),
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
        if let Some(rig) = world.player_mut(player(seats)) {
            rig.flight.lag_seconds = 134.0;
        }
    });
    let input = crate::input::PlayerInput {
        thrust: true,
        ..crate::input::PlayerInput::default()
    };
    stage.hold(16, input);
}

/// Mid-transit through a wormhole: the tunnel and its progress.
fn wormhole(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        let Some(mouth) = find(|body| body.kind == BodyKind::Wormhole) else {
            return;
        };
        let BodyExtra::Mouth(facts) = mouth.extra else {
            return;
        };
        let (arrival, (hx, hy)) = systems::transit_exit(&facts, (-600.0, 0.0));
        let now = world.millis_at(world.simulation_tick());
        let speed = systems::TRANSIT_EXIT_SPEED;
        launch(
            world,
            player(seats),
            mouth.centre,
            up_angle(-1.0, 0.0),
            (0.0, 0.0),
            StarDrive::Transit(TransitRun {
                run: WarpRun {
                    origin: mouth.centre,
                    arrival,
                    target: fatherload_simulation::flight::SystemRef {
                        id: u64::MAX,
                        sector: facts.exit_sector,
                    },
                    start: now.saturating_sub(1_300),
                    duration_millis: systems::TRANSIT_MILLIS,
                },
                heading_x: hx / speed,
                heading_y: hy / speed,
            }),
            true,
        );
    });
    stage.settle(4);
}

/// Deep in a nebula: the tint, the fogged sensors, the waypoint and a crewmate's markers
/// faded.
fn nebula(stage: &mut SceneStage<'_>) {
    stage.play(2, |world, seats| {
        space_world(world);
        let galaxy = GeneratedSpace::new(SCENE_SEED);
        let found = sectors(14).find_map(|sector| {
            let found = galaxy.space().sector(sector);
            found
                .nebulae
                .iter()
                .chain(
                    found
                        .systems
                        .iter()
                        .filter_map(|system| system.nebula.as_ref()),
                )
                .find(|cloud| cloud.fog >= 500)
                .copied()
        });
        let Some(cloud) = found else {
            return;
        };
        let at = cloud.centre.offset(cloud.radius / 5, 0).pos();
        launch(
            world,
            seats[0].player,
            at,
            0.6,
            (90.0, -40.0),
            StarDrive::Off,
            true,
        );
        launch(
            world,
            seats[1].player,
            at.offset(tiles(9_000.0), tiles(-4_000.0)),
            0.0,
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
        set_waypoint(
            world,
            seats[0].player,
            NavTarget::System(space::HOME_SYSTEM),
        );
    });
    stage.settle(8);
}

/// The first big belt rock near home.
fn belt_rock(world: &WorldState) -> Option<FlightBody> {
    let galaxy = GeneratedSpace::new(SCENE_SEED);
    let record = world.space_systems().clone();
    sectors(4).find_map(|sector| {
        galaxy
            .space()
            .sector(sector)
            .systems
            .iter()
            .find_map(|system| {
                system.belts.iter().find_map(|belt| {
                    let mid = i64::from(belt.inner / 2 + belt.outer / 2);
                    galaxy
                        .rocks_near(system.centre.offset(mid, 0).pos(), 8_000, &record)
                        .into_iter()
                        .find(|rock| rock.radius >= tiles(24.0) && rock.radius <= tiles(60.0))
                })
            })
    })
}

/// Resting on a belt rock with the drill to it: sparks, the gauge and ore in the bay.
fn mining(stage: &mut SceneStage<'_>, beam: bool) {
    stage.play(1, |world, seats| {
        space_world(world);
        let Some(rock) = belt_rock(world) else {
            return;
        };
        if beam {
            let at = rock.centre.offset(rock.radius + tiles(8.0), tiles(-3.0));
            let (dx, dy) = at.delta(rock.centre);
            launch(
                world,
                player(seats),
                at,
                up_angle(dx, dy),
                (0.0, 0.0),
                StarDrive::Off,
                true,
            );
        } else {
            let at = rock.centre.offset(
                0.0,
                -(rock.radius + fatherload_simulation::flight::RIG_CONTACT + 1.0),
            );
            launch(
                world,
                player(seats),
                at,
                0.0,
                (0.0, 0.0),
                StarDrive::Off,
                true,
            );
        }
    });
    if beam {
        let input = crate::input::PlayerInput {
            fire: true,
            ..crate::input::PlayerInput::default()
        };
        stage.hold(40, input);
    } else {
        stage.settle(40);
    }
}

/// Gliding through a comet's tail, gathering dust.
fn comet(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        let Some(comet) = find(|body| body.kind == BodyKind::Comet) else {
            return;
        };
        let BodyExtra::Comet(facts) = comet.extra else {
            return;
        };
        let (dx, dy) = comet.centre.delta(facts.tail_end);
        let at = comet.centre.offset(dx * 0.04, dy * 0.04);
        launch(
            world,
            player(seats),
            at,
            up_angle(-dx, -dy),
            (dx * 0.0002, dy * 0.0002),
            StarDrive::Off,
            true,
        );
    });
    stage.settle(70);
}

/// Coming down to the Moon from space: just over it (`arrived` false), or a moment later in
/// the home sky over the Moon's ground.
fn moon(stage: &mut SceneStage<'_>, arrived: bool) {
    stage.play(1, |world, seats| {
        space_world(world);
        let moon = space::moon_centre();
        let radius = tiles(640.0);
        let at = moon.offset(
            tiles(180.0),
            -(radius + tiles(if arrived { 62.0 } else { 140.0 })),
        );
        launch(
            world,
            player(seats),
            at,
            0.15,
            (0.0, 160.0),
            StarDrive::Off,
            true,
        );
    });
    stage.settle(if arrived { 60 } else { 8 });
}

/// Idle at the departure point: the parking hold keeps the rig where it is.
fn parking_hold(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        launch(
            world,
            player(seats),
            space::home_launch(),
            -0.5,
            (4.0, 6.0),
            StarDrive::Off,
            false,
        );
    });
    stage.settle(60);
}

pub const SYSTEMS_SCENES: &[Scene] = &[
    Scene {
        name: "systems_star_flare",
        summary: "A flaring star: the amber wash of its flare, the hull heating, the flare banner over the flight HUD",
        setup: star_flare,
    },
    Scene {
        name: "systems_blue_glare",
        summary: "Close to a blue star: its glare washes the view (capped, no flashing), the flare forecast",
        setup: blue_glare,
    },
    Scene {
        name: "systems_black_hole_dilation",
        summary: "In a black hole's well past the strain line, burning away: the warning, the no-return ring, TIME x0.xx and the clock behind",
        setup: black_hole,
    },
    Scene {
        name: "systems_wormhole_transit",
        summary: "Mid-transit through a wormhole: the tunnel's rings and the transit's progress",
        setup: wormhole,
    },
    Scene {
        name: "systems_nebula_fog",
        summary: "Deep in a nebula: its tint, SENSORS FOGGED, the waypoint and a crewmate's markers faded",
        setup: nebula,
    },
    Scene {
        name: "systems_mining_asteroid",
        summary: "Resting on a belt rock with the drill to it: sparks, the drilling gauge and the units left",
        setup: |stage| mining(stage, false),
    },
    Scene {
        name: "systems_mining_beam",
        summary: "The Mining Beam from the nose to a belt rock: the beam in the ore's colour and the gauge",
        setup: |stage| mining(stage, true),
    },
    Scene {
        name: "systems_comet_tail",
        summary: "In a comet's tail near its nucleus: dust streaming past and the comet dust gauge",
        setup: comet,
    },
    Scene {
        name: "systems_moon_approach",
        summary: "Coming down to the Moon from space: the round Moon under the rig, inside its approach sphere",
        setup: |stage| moon(stage, false),
    },
    Scene {
        name: "systems_moon_arrival",
        summary: "A moment later: handed to the home sky over the point of the Moon it came down to",
        setup: |stage| moon(stage, true),
    },
    Scene {
        name: "systems_parking_hold",
        summary: "Idle at the departure point over the Moon: the parking hold keeps the rig there (no drift home)",
        setup: parking_hold,
    },
];
