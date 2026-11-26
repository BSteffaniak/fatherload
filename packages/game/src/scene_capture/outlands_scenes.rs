//! The outland worlds' scenes (Wave 13 planets-c): a fungal, a machine and a dead world of the
//! real galaxy (the nearest of each to home on the flight scenes' seed), seen from space, on
//! their surface among their creatures, at their landmark, in their strata (a grotto, a spore
//! vent, the gear works, a power vault, a tomb, the ghost lights), at a builders' outpost and
//! in each theme boss's lair. Every scene stands on the body's round ground (its store, the
//! polar renderer), the camera locked to the craft.

use fatherload_simulation::{
    WorldState,
    enemy::EnemyKind,
    flight::{BodyRef, FlightBody, FlightPlace, SpaceQuery, StarDrive, nav, space},
    place::StoreKey,
    planet::{
        BodyGround, PlanetGround, PlanetThemeId,
        outlands::{boss_lair, builder_outposts, dead, fungal, life, machine},
        world_of,
    },
    space::{BodyKind, HOME_SECTOR, SpaceSector, Theme, generate_sector},
};

use super::{
    Scene, SceneStage,
    flight_scenes::{SCENE_SEED, launch, set_down, space_world},
};

/// The nearest planet of `theme` to home on the scenes' seed (big enough for a lair).
fn outland_body(world: &WorldState, theme: Theme) -> Option<FlightBody> {
    let galaxy = world.space();
    for ring in 0..8_i32 {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let sector = generate_sector(
                    fatherload_simulation::space::SpaceSeed::from(SCENE_SEED),
                    SpaceSector::new(HOME_SECTOR.x + x, HOME_SECTOR.y + y),
                );
                for system in &sector.systems {
                    for body in &system.bodies {
                        if body.theme != theme
                            || body.kind != BodyKind::Planet
                            || body.radius > 5_000
                        {
                            continue;
                        }
                        let id = fatherload_simulation::space::SpaceBodyId {
                            sector: sector.coords,
                            system: system.index,
                            body: body.index,
                        };
                        let found = galaxy
                            .body_ref(id)
                            .and_then(|reference: BodyRef| galaxy.body(reference));
                        if let Some(found) = found
                            && world.body_ground(found.body).is_some()
                        {
                            return Some(found);
                        }
                    }
                }
            }
        }
    }
    None
}

const fn space_theme(theme: PlanetThemeId) -> Theme {
    match theme {
        PlanetThemeId::MACHINE => Theme::MachineWorld,
        PlanetThemeId::DEAD => Theme::DeadWorld,
        _ => Theme::Fungal,
    }
}

/// Where on the strip a scene stands.
#[derive(Clone, Copy)]
enum Spot {
    /// The settlement site.
    Settlement,
    /// Beside the landmark.
    Landmark,
    /// Open ground away from the settlement, where the creatures roam.
    Wilds,
    /// In an open cave of stratum `n`.
    Stratum(usize),
    /// Beside a spore vent's chimney (the fungal world).
    Vent,
    /// In a power vault's control room (the machine world).
    Vault,
    /// Beside a tomb (the dead world).
    Tomb,
    /// In a petrified city's street (the dead world).
    City,
    /// Beside the builders' outpost.
    Outpost,
    /// In the theme boss's lair.
    Lair,
}

/// A roomy open floor in `rows` (open over a solid floor, open round it), at its floor row.
fn open_floor(planet: &PlanetGround, rows: std::ops::Range<i32>) -> Option<(i32, i32)> {
    (0..planet.strip_width())
        .step_by(7)
        .flat_map(|column| rows.clone().map(move |row| (column, row)))
        .find(|&(column, row)| {
            let open =
                |dx: i32, dy: i32| planet.generated_tile(column + dx, row + dy).kind.is_open();
            !open(0, 1) && (-3..=3).all(|dx| (-4..=0).all(|dy| open(dx, dy)))
        })
        .map(|(column, row)| (column, row + 1))
}

/// The strip tile (column, floor row) of `spot` on `planet`.
#[allow(clippy::too_many_lines, reason = "one arm per spot")]
fn spot_tile(planet: &PlanetGround, spot: Spot) -> (i32, i32) {
    let world = world_of(planet.params().theme).expect("an outland world");
    let site = world.settlement_site(planet).expect("a site");
    let fallback = (site.centre(), site.floor);
    let width = i64::from(planet.strip_width());
    let top = planet.core_chamber_top();
    match spot {
        Spot::Settlement => fallback,
        Spot::Landmark => {
            // Hovering off to one side, high enough to frame the whole structure.
            let site = world.landmark_site(planet).expect("a landmark");
            (site.centre() - 12, planet.surface_row(site.centre()) - 24)
        }
        Spot::Wilds => (site.centre() + 90..site.centre() + 2_000)
            .map(|column| (column, planet.surface_row(column)))
            .find(|&(column, floor)| {
                (-4..=4).all(|dx| planet.surface_row(column + dx) == floor)
                    && (1..=6).all(|up| {
                        (-2..=2).all(|dx| {
                            planet
                                .generated_tile(column + dx, floor - up)
                                .kind
                                .is_open()
                        })
                    })
                    && !planet.generated_tile(column, floor).kind.is_open()
            })
            .unwrap_or(fallback),
        Spot::Stratum(stratum) => {
            let rows = match stratum {
                // Just under the crust (the mat's top), right under the settlement.
                0 => return (site.centre(), site.floor + 14),
                1 => top / 3 + 12..top * 2 / 3 - 12,
                _ => top * 2 / 3 + 12..top - 12,
            };
            open_floor(planet, rows).unwrap_or(fallback)
        }
        Spot::Vent => {
            let deep = top * 5 / 6;
            (0..width)
                .step_by(13)
                .find_map(|column| fungal::vent_in_cell(planet, column, deep))
                .map_or(fallback, |vent| {
                    (i32::try_from(vent.column).unwrap_or(0) - 3, vent.row - 2)
                })
        }
        Spot::Vault => {
            let shell = top * 5 / 6;
            (0..width / machine::VAULT_CELL.0)
                .flat_map(|cell| [shell - 30, shell, shell + 30].map(|row| (cell, row)))
                .find_map(|(cell, row)| {
                    machine::vault_in_cell(planet, cell * machine::VAULT_CELL.0, row)
                })
                .map_or(fallback, |vault| {
                    (
                        i32::try_from(vault.column + 5 * vault.span).unwrap_or(0),
                        vault.row,
                    )
                })
        }
        Spot::Tomb => {
            let necropolis = top / 2;
            (0..width)
                .step_by(9)
                .flat_map(|column| (necropolis - 30..necropolis + 30).map(move |row| (column, row)))
                .find_map(|(column, row)| dead::tomb_at(planet, column, row))
                .map_or(fallback, |tomb| {
                    let column = i32::try_from(tomb.column - 4).unwrap_or(0);
                    (column, tomb.row + dead::TOMB_BOX.1)
                })
        }
        Spot::City => (0..width / dead::CITY_CELL)
            .find_map(|cell| dead::city_in_cell(planet, cell))
            .map_or(fallback, |city| {
                let column = i32::try_from(city.first + city.width / 2).unwrap_or(0);
                (column, planet.surface_row(column))
            }),
        Spot::Outpost => builder_outposts(planet)
            .first()
            .map_or(fallback, |outpost| {
                let column = outpost.site.centre() - 8;
                (column, planet.surface_row(column))
            }),
        Spot::Lair => boss_lair(planet).map_or(fallback, |lair| {
            let column = i32::try_from((lair.centre - 14).rem_euclid(width)).unwrap_or(0);
            (column, lair.floor)
        }),
    }
}

/// Lands on the nearest `theme` world at `spot` at world time `millis`; `creatures` are
/// spawned round the rig (a theme boss's spawn is its lair's middle).
fn on_outland(
    stage: &mut SceneStage<'_>,
    theme: PlanetThemeId,
    spot: Spot,
    millis: u64,
    creatures: &'static [(EnemyKind, i32, i32)],
) {
    stage.play(1, |world, seats| {
        space_world(world);
        world.enable_test_mode();
        let Some(body) = outland_body(world, space_theme(theme)) else {
            return;
        };
        let Some(ground) = world.body_ground(body.body) else {
            return;
        };
        let planet = PlanetGround::new(ground.params);
        let (column, floor) = spot_tile(&planet, spot);
        // Mid-morning at the spot (the scenes' light), the vent scene mid-blast, the gear
        // scene with the works turning: the rhythms repeat, so each is found a whole number
        // of their cycles after the morning.
        let store = StoreKey::Body(ground.id);
        let morning = world.body_life(store).map_or(millis, |life| {
            life.clock.millis_when(millis, i64::from(column), 420)
        });
        let millis = match spot {
            Spot::Vent => vent_blast_time(&planet, column + 3, morning).unwrap_or(morning),
            _ if millis == GEARS_TURNING_MILLIS => {
                let cycle = life::GEAR_CYCLE_MILLIS;
                morning - morning % cycle + GEARS_TURNING_MILLIS % cycle
            }
            _ => morning,
        };
        world.set_body_millis(millis);
        launch(
            world,
            seats[0].player,
            body.centre,
            0.0,
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
        set_down(
            world,
            seats[0].player,
            body,
            column,
            planet.surface_row(column) - floor + 1,
        );

        let _ = world.with_store_ground(store, |world| {
            // Under the settlement the scene digs itself a gallery to stand in.
            if matches!(spot, Spot::Stratum(0)) {
                for dy in -3..0 {
                    for dx in -9..=9 {
                        let _ = world.terrain_mut().set_kind(
                            fatherload_simulation::terrain::TilePosition {
                                x: column + dx,
                                y: floor + dy,
                            },
                            fatherload_simulation::terrain::TileKind::Air,
                        );
                    }
                }
            }
            for (kind, dx, dy) in creatures {
                let (x, y) = world.frame().centre_of(column + dx, floor + dy);
                let _ = world.spawn_enemy(*kind, x, y);
            }
        });
    });
    // Over a landmark the craft is caught a moment after it was set high (still falling).
    stage.settle(if matches!(spot, Spot::Landmark) {
        3
    } else {
        90
    });
}

/// Coming down over the nearest `theme` world from space.
fn approach(stage: &mut SceneStage<'_>, theme: PlanetThemeId) {
    stage.play(1, |world, seats| {
        space_world(world);
        let Some(body) = outland_body(world, space_theme(theme)) else {
            return;
        };
        let altitude = body.radius + space::tiles(220.0);
        let pos = body.centre.offset(altitude * 0.6, -altitude * 0.8);
        launch(
            world,
            seats[0].player,
            pos,
            nav::bearing(body.centre, pos) + 0.3,
            (-40.0, 60.0),
            StarDrive::Off,
            true,
        );
        if let Some(rig) = world.player_mut(seats[0].player) {
            rig.flight.place = FlightPlace::Approach(body.body);
        }
    });
    stage.settle(10);
}

/// A world time a little way into a blast of the vent at strip column `column`, from `after`.
fn vent_blast_time(planet: &PlanetGround, column: i32, after: u64) -> Option<u64> {
    let column = i64::from(column);
    (after..after + life::VENT_CYCLE_MILLIS)
        .step_by(100)
        .find(|millis| {
            life::vent_phase(planet, column, *millis) == life::VentPhase::Blasting
                && life::vent_phase(planet, column, millis.saturating_sub(600))
                    == life::VentPhase::Blasting
        })
}

/// A world time when the gear works turn.
const GEARS_TURNING_MILLIS: u64 = 162_400;
/// The scenes' usual world time.
const START: u64 = 120_000;
/// The vent scene's fallback time (it finds its vent's blast).
const VENT_BLAST_MILLIS: u64 = START;

const FUNGAL_LIFE: &[(EnemyKind, i32, i32)] = &[
    (EnemyKind::Capgrazer, 7, -2),
    (EnemyKind::Sporeling, -5, -1),
    (EnemyKind::Sporeling, -7, -1),
];
const GROTTO_LIFE: &[(EnemyKind, i32, i32)] = &[
    (EnemyKind::Lanternfly, 3, -3),
    (EnemyKind::Lanternfly, -2, -4),
];
const ROTMAW: &[(EnemyKind, i32, i32)] = &[(EnemyKind::Rotmaw, 5, -1)];
const MACHINE_LIFE: &[(EnemyKind, i32, i32)] = &[
    (EnemyKind::Scuttlebot, 6, -1),
    (EnemyKind::WardDrone, -4, -6),
];
const GEAR_LIFE: &[(EnemyKind, i32, i32)] = &[(EnemyKind::Cogspider, 4, -1)];
const ARC_MITES: &[(EnemyKind, i32, i32)] =
    &[(EnemyKind::ArcMite, 3, -2), (EnemyKind::ArcMite, 5, -3)];
const DEAD_LIFE: &[(EnemyKind, i32, i32)] = &[
    (EnemyKind::Ashrunner, 7, -1),
    (EnemyKind::Bonepicker, -6, -1),
];
const TOMB_LIFE: &[(EnemyKind, i32, i32)] = &[(EnemyKind::Gravewarden, 3, -1)];
const SHADES: &[(EnemyKind, i32, i32)] = &[(EnemyKind::Shade, 4, -3)];
const SPOREMOTHER: &[(EnemyKind, i32, i32)] = &[(EnemyKind::Sporemother, 8, -1)];
const REACTOR_WARDEN: &[(EnemyKind, i32, i32)] = &[(EnemyKind::ReactorWarden, 6, -3)];
const ASHEN_SOVEREIGN: &[(EnemyKind, i32, i32)] = &[(EnemyKind::AshenSovereign, 6, -1)];

pub const OUTLANDS_SCENES: &[Scene] = &[
    Scene {
        name: "outland_fungal_approach",
        summary: "A fungal world from space: violet under a rose glow, the craft coming down",
        setup: |stage| approach(stage, PlanetThemeId::FUNGAL),
    },
    Scene {
        name: "outland_fungal_surface",
        summary: "On a fungal world: puffball mounds under giant mushrooms, a capgrazer and sporelings",
        setup: |stage| {
            on_outland(
                stage,
                PlanetThemeId::FUNGAL,
                Spot::Wilds,
                START,
                FUNGAL_LIFE,
            );
        },
    },
    Scene {
        name: "outland_fungal_settlement",
        summary: "A fungal world's settlement clearing: level ground ringed by giant mushrooms",
        setup: |stage| on_outland(stage, PlanetThemeId::FUNGAL, Spot::Settlement, START, &[]),
    },
    Scene {
        name: "outland_fungal_landmark",
        summary: "The Mother Cap: a stalk eight wide under a glowing dome, the door at its foot",
        setup: |stage| on_outland(stage, PlanetThemeId::FUNGAL, Spot::Landmark, START, &[]),
    },
    Scene {
        name: "outland_fungal_mat",
        summary: "The Mycelium Mat: white felt and lilac hyphae, the mat that grows back",
        setup: |stage| on_outland(stage, PlanetThemeId::FUNGAL, Spot::Stratum(0), START, &[]),
    },
    Scene {
        name: "outland_fungal_grottos",
        summary: "The Glowcap Grottos: ribbed gillstone, glowcaps and lumen moss, lanternflies",
        setup: |stage| {
            on_outland(
                stage,
                PlanetThemeId::FUNGAL,
                Spot::Stratum(1),
                START,
                GROTTO_LIFE,
            );
        },
    },
    Scene {
        name: "outland_fungal_vent",
        summary: "The Sporeheart Deep: a spore vent blasting up its chimney, a rotmaw in the rock",
        setup: |stage| {
            on_outland(
                stage,
                PlanetThemeId::FUNGAL,
                Spot::Vent,
                VENT_BLAST_MILLIS,
                ROTMAW,
            );
        },
    },
    Scene {
        name: "outland_fungal_boss",
        summary: "The Sporemother in her hollow under the Mother Cap, the seal glowing on the floor",
        setup: |stage| on_outland(stage, PlanetThemeId::FUNGAL, Spot::Lair, START, SPOREMOTHER),
    },
    Scene {
        name: "outland_machine_approach",
        summary: "A machine world from space: steel grey, the craft coming down",
        setup: |stage| approach(stage, PlanetThemeId::MACHINE),
    },
    Scene {
        name: "outland_machine_surface",
        summary: "On a machine world: riveted plate terraces, pylons with beacons, a scuttlebot and a ward drone",
        setup: |stage| {
            on_outland(
                stage,
                PlanetThemeId::MACHINE,
                Spot::Wilds,
                START,
                MACHINE_LIFE,
            );
        },
    },
    Scene {
        name: "outland_machine_landmark",
        summary: "The Reactor Spire: a tower of hazard-striped casing with lit windows and a beacon crown",
        setup: |stage| on_outland(stage, PlanetThemeId::MACHINE, Spot::Landmark, START, &[]),
    },
    Scene {
        name: "outland_machine_gears",
        summary: "The Gear Works turning: a gear room's great gears throwing sparks, a cogspider",
        setup: |stage| {
            on_outland(
                stage,
                PlanetThemeId::MACHINE,
                Spot::Stratum(1),
                GEARS_TURNING_MILLIS,
                GEAR_LIFE,
            );
        },
    },
    Scene {
        name: "outland_machine_vault",
        summary: "The Reactor Shell: a power vault's control room, three relays in its roof over the seal",
        setup: |stage| on_outland(stage, PlanetThemeId::MACHINE, Spot::Vault, START, ARC_MITES),
    },
    Scene {
        name: "outland_machine_boss",
        summary: "The Reactor Warden over the core reactor's hall, its reward vault in the wall",
        setup: |stage| {
            on_outland(
                stage,
                PlanetThemeId::MACHINE,
                Spot::Lair,
                START,
                REACTOR_WARDEN,
            );
        },
    },
    Scene {
        name: "outland_dead_approach",
        summary: "A dead world from space: ash grey under a faint glow, the craft coming down",
        setup: |stage| approach(stage, PlanetThemeId::DEAD),
    },
    Scene {
        name: "outland_dead_surface",
        summary: "On a dead world: ash plains, an ashrunner and a bonepicker",
        setup: |stage| on_outland(stage, PlanetThemeId::DEAD, Spot::Wilds, START, DEAD_LIFE),
    },
    Scene {
        name: "outland_dead_city",
        summary: "A petrified city: towers of stone with dead windows, storeys to fly up through",
        setup: |stage| on_outland(stage, PlanetThemeId::DEAD, Spot::City, START, &[]),
    },
    Scene {
        name: "outland_dead_landmark",
        summary: "The Hollow Throne: a stepped ziggurat of petrified stone, a throne on its crown",
        setup: |stage| on_outland(stage, PlanetThemeId::DEAD, Spot::Landmark, START, &[]),
    },
    Scene {
        name: "outland_dead_tomb",
        summary: "The Necropolis: a sealed tomb of brick in ossuary stone, a gravewarden",
        setup: |stage| on_outland(stage, PlanetThemeId::DEAD, Spot::Tomb, START, TOMB_LIFE),
    },
    Scene {
        name: "outland_dead_mantle",
        summary: "The Grave Mantle: black grave rock and soulstone, ghost lights drifting, a shade",
        setup: |stage| on_outland(stage, PlanetThemeId::DEAD, Spot::Stratum(2), START, SHADES),
    },
    Scene {
        name: "outland_dead_boss",
        summary: "The Ashen Sovereign in the tomb beneath the Hollow Throne",
        setup: |stage| {
            on_outland(
                stage,
                PlanetThemeId::DEAD,
                Spot::Lair,
                START,
                ASHEN_SOVEREIGN,
            );
        },
    },
    Scene {
        name: "outland_machine_outpost",
        summary: "A builders' outpost on a machine world: a glyph-marked obelisk over its cache",
        setup: |stage| on_outland(stage, PlanetThemeId::MACHINE, Spot::Outpost, START, &[]),
    },
];

/// The gear scene's world time stays in phase with the rule (a turning).
#[cfg(test)]
mod tests {
    #[test]
    fn the_gear_scene_shows_the_gears_turning() {
        use fatherload_simulation::planet::outlands::life::{GearPhase, gear_phase};
        assert_eq!(gear_phase(super::GEARS_TURNING_MILLIS), GearPhase::Turning);
    }
}
