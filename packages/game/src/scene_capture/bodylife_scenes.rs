//! Living bodies' scenes (Wave 12 bodylife): a body's own day and night on its round ground
//! (the terminator sweeping across it), a tidally locked world's eternal day and night sides,
//! a storm, snowfall and an aurora, each on a real body of the scene world's space with the
//! body badge (its hour, climate, air, gravity and weather) in the clock's place.

use std::sync::Arc;

use fatherload_simulation::{
    PlayerId, WorldState,
    bodylife::{
        BodyLife, WeatherId,
        weather::{SPELL_MILLIS, next_spell_of},
    },
    flight::{FlightBody, SpaceQuery, StarDrive},
    place::SpaceBodyKey,
    space::{Atmosphere, Space, SpaceSector},
};

use super::{
    Scene, SceneStage,
    flight_scenes::{SCENE_SEED, launch, set_down, space_world},
};

/// The first landable body of the scene world near home (by rings of sectors) that `wanted`
/// picks, as flight sees it, with its life.
fn find_body(
    world: &WorldState,
    wanted: fn(&BodyLife, Atmosphere) -> bool,
) -> Option<(FlightBody, Arc<BodyLife>)> {
    let space = Space::shared(fatherload_simulation::space::SpaceSeed::from(SCENE_SEED));
    let flight = world.space();
    for ring in 0..6_i32 {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let sector = space.sector(SpaceSector::new(x, y));
                for system in &sector.systems {
                    for body in &system.bodies {
                        let key = SpaceBodyKey {
                            sector: sector.coords,
                            system: system.index,
                            body: body.index,
                        };
                        let Some(params) = fatherload_simulation::place::planet_params_for(
                            fatherload_simulation::space::SpaceSeed::from(SCENE_SEED),
                            key,
                        ) else {
                            continue;
                        };
                        let Some(life) = fatherload_simulation::world::body_life_of(
                            fatherload_simulation::space::SpaceSeed::from(SCENE_SEED),
                            Some(key),
                            params,
                        ) else {
                            continue;
                        };
                        // Out of any nebula's clouds, so the night sky reads as night.
                        let fogged = system.nebula.is_some()
                            || space
                                .sectors_near(body.centre, 400_000)
                                .iter()
                                .flat_map(|near| near.nebulae.iter())
                                .any(|nebula| {
                                    body.centre.distance(nebula.centre)
                                        < f64::from(
                                            i32::try_from(nebula.radius).unwrap_or(i32::MAX),
                                        ) * 1.8
                                });
                        if fogged || !wanted(&life, body.atmosphere) {
                            continue;
                        }
                        let Some(found) = flight.body_ref(key).and_then(|at| flight.body(at))
                        else {
                            continue;
                        };
                        return Some((found, life));
                    }
                }
            }
        }
    }
    None
}

/// The seat's rig set down on the body `wanted` picks, at the body time `when` gives and the
/// strip column `column` gives for it.
fn stand_on(
    stage: &mut SceneStage<'_>,
    wanted: fn(&BodyLife, Atmosphere) -> bool,
    when: fn(&BodyLife) -> u64,
    column: fn(&BodyLife, u64) -> i64,
) {
    stage.play(1, |world, seats| {
        space_world(world);
        world.enable_test_mode();
        let Some((body, life)) = find_body(world, wanted) else {
            return;
        };
        let player: PlayerId = seats[0].player;
        let time = when(&life);
        world.set_body_millis(time);
        launch(
            world,
            player,
            body.centre,
            0.0,
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
        let at = i32::try_from(column(&life, time)).unwrap_or(0);
        set_down(world, player, body, at, 1);
    });
    stage.settle(40);
}

/// The middle of the first spell of `kind` from ten minutes in (or ten minutes in).
fn spell_of(life: &BodyLife, kind: WeatherId) -> u64 {
    let start = 600_000;
    next_spell_of(
        life.seed,
        life.hooks().weather_table(),
        life.climate.atmosphere,
        kind,
        start,
        2_000,
    )
    .map_or(start, |at| at - at % SPELL_MILLIS + SPELL_MILLIS / 2)
}

fn has_weather(life: &BodyLife, kind: WeatherId) -> bool {
    life.hooks()
        .weather_table()
        .iter()
        .any(|odds| odds.kind == kind)
}

fn turning_with_air(life: &BodyLife, air: Atmosphere) -> bool {
    !life.clock.tidally_locked() && air != Atmosphere::Vacuum
}

const fn locked(life: &BodyLife, _: Atmosphere) -> bool {
    life.clock.tidally_locked()
}

fn stormy(life: &BodyLife, air: Atmosphere) -> bool {
    air != Atmosphere::Vacuum && has_weather(life, WeatherId::STORM)
}

fn snowy(life: &BodyLife, air: Atmosphere) -> bool {
    air != Atmosphere::Vacuum && has_weather(life, WeatherId::SNOW)
}

fn auroral(life: &BodyLife, air: Atmosphere) -> bool {
    !life.clock.tidally_locked()
        && air != Atmosphere::Vacuum
        && has_weather(life, WeatherId::AURORA)
}

/// Ten minutes in.
const fn ten_minutes(_: &BodyLife) -> u64 {
    600_000
}

pub const BODYLIFE_SCENES: &[Scene] = &[
    Scene {
        name: "bodylife_terminator",
        summary: "Dusk on a turning world: the terminator across its round ground, day on one side and night on the other, the body badge with its local hour",
        setup: |stage| {
            stand_on(stage, turning_with_air, ten_minutes, |life, time| {
                life.clock.column_at(time, 755)
            });
        },
    },
    Scene {
        name: "bodylife_night",
        summary: "Midnight on a turning world: the ground in starlight, the badge's night hour and the cold",
        setup: |stage| {
            stand_on(stage, turning_with_air, ten_minutes, |life, time| {
                life.clock.column_at(time, 0)
            });
        },
    },
    Scene {
        name: "bodylife_locked_day",
        summary: "A tidally locked world's day side: its star overhead forever (column 0), the badge says DAY SIDE",
        setup: |stage| stand_on(stage, locked, ten_minutes, |_, _| 24),
    },
    Scene {
        name: "bodylife_locked_night",
        summary: "The same tidally locked world's night side, half a lap round: eternal night and deep cold",
        setup: |stage| {
            stand_on(stage, locked, ten_minutes, |life, _| {
                i64::from(life.clock.columns / 2) + 24
            });
        },
    },
    Scene {
        name: "bodylife_locked_terminator",
        summary: "A tidally locked world's terminator, a quarter lap round: the line between the eternal day and night",
        setup: |stage| {
            stand_on(stage, locked, ten_minutes, |life, _| {
                i64::from(life.clock.columns / 4)
            });
        },
    },
    Scene {
        name: "bodylife_storm",
        summary: "A storm by day on a world with air: rain driven along the wind, the gloom and fog closing in, the badge's STORM and wind",
        setup: |stage| {
            stand_on(
                stage,
                stormy,
                |life| spell_of(life, WeatherId::STORM),
                |life, time| life.clock.column_at(time, 560),
            );
        },
    },
    Scene {
        name: "bodylife_snow",
        summary: "Snowfall on a cold world: flakes drifting with the wind, the white fog, the cold on the badge",
        setup: |stage| {
            stand_on(
                stage,
                snowy,
                |life| spell_of(life, WeatherId::SNOW),
                |life, time| life.clock.column_at(time, 540),
            );
        },
    },
    Scene {
        name: "bodylife_aurora",
        summary: "An aurora over a world's night side: green and violet curtains hanging over the round horizon",
        setup: |stage| {
            stand_on(
                stage,
                auroral,
                |life| spell_of(life, WeatherId::AURORA),
                |life, time| life.clock.column_at(time, 20),
            );
        },
    },
];
