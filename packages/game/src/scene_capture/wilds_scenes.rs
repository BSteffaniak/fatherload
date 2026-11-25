//! The wild worlds' scenes (Wave 12 planets-a): a jungle, an ocean archipelago and a glacier
//! world of the real galaxy (the nearest of each to home on the flight scenes' seed), seen
//! from space, on their surface, at their landmark, in two of their strata and with their
//! creatures about. Every scene stands on the body's round ground (its store, the polar
//! renderer), the camera locked to the craft.

use fatherload_simulation::{
    WorldState,
    enemy::EnemyKind,
    flight::{BodyRef, FlightBody, FlightPlace, SpaceQuery, StarDrive, nav, space},
    place::{BodyId, Place, StoreKey},
    planet::{BodyGround, PlanetGround, PlanetThemeId, world_of},
    space::{BodyKind, HOME_SECTOR, SpaceSector, Theme, generate_sector},
    terrain::TilePosition,
};

use super::{
    Scene, SceneStage,
    flight_scenes::{SCENE_SEED, launch, set_down, space_world},
};

/// The nearest planet of `theme` to home on the scenes' seed.
fn wild_body(world: &WorldState, theme: Theme) -> Option<FlightBody> {
    let galaxy = world.space();
    for ring in 0..6_i32 {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let sector = generate_sector(
                    SCENE_SEED,
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
        PlanetThemeId::OCEAN => Theme::OceanArchipelago,
        PlanetThemeId::GLACIER => Theme::Ice,
        _ => Theme::Jungle,
    }
}

/// Where on the strip a scene stands.
#[derive(Clone, Copy)]
enum Spot {
    /// The settlement site.
    Settlement,
    /// Beside the landmark.
    Landmark,
    /// In an open cave of stratum `n`.
    Stratum(usize),
    /// Open ground away from the settlement, where the creatures roam.
    Wilds,
}

/// The strip tile (column, floor row) of `spot` on `planet`.
fn spot_tile(planet: &PlanetGround, spot: Spot) -> (i32, i32) {
    let world = world_of(planet.params().theme).expect("a wild world");
    match spot {
        Spot::Settlement => {
            let site = world.settlement_site(planet).expect("a site");
            (site.centre(), site.floor)
        }
        Spot::Landmark => {
            let site = world.landmark_site(planet).expect("a landmark");
            let column = site.centre() - 14;
            (column, planet.surface_row(column))
        }
        Spot::Wilds => {
            let site = world.settlement_site(planet).expect("a site");
            (site.centre() + 90..site.centre() + 2_000)
                .map(|column| (column, planet.surface_row(column)))
                .find(|&(column, floor)| {
                    (-4..=4).all(|dx| planet.surface_row(column + dx) == floor)
                        && (1..=4).all(|up| {
                            (-2..=2).all(|dx| {
                                planet
                                    .generated_tile(column + dx, floor - up)
                                    .kind
                                    .is_open()
                            })
                        })
                        && !planet.generated_tile(column, floor).kind.is_open()
                })
                .unwrap_or_else(|| (site.centre(), site.floor))
        }
        Spot::Stratum(stratum) => {
            let top = planet.core_chamber_top();
            let rows = match stratum {
                1 => top / 3 + 12..top * 2 / 3 - 12,
                _ => top * 2 / 3 + 12..top - 12,
            };
            (0..planet.strip_width())
                .step_by(7)
                .flat_map(|column| rows.clone().map(move |row| (column, row)))
                .find(|&(column, row)| {
                    let open = |dx: i32, dy: i32| {
                        planet.generated_tile(column + dx, row + dy).kind.is_open()
                    };
                    // A roomy floor: open over a solid floor, open round it.
                    !open(0, 1) && (-3..=3).all(|dx| (-4..=0).all(|dy| open(dx, dy)))
                })
                .map_or((0, rows.start), |(column, row)| (column, row + 1))
        }
    }
}

/// Lands on the nearest `theme` world at `spot`; `creatures` are spawned round the rig.
fn on_wild_world(
    stage: &mut SceneStage<'_>,
    theme: PlanetThemeId,
    spot: Spot,
    creatures: &'static [(EnemyKind, i32, i32)],
) {
    stage.play(1, |world, seats| {
        space_world(world);
        world.enable_test_mode();
        let Some(body) = wild_body(world, space_theme(theme)) else {
            return;
        };
        let Some(ground) = world.body_ground(body.body) else {
            return;
        };
        let planet = PlanetGround::new(ground.params);
        let (column, floor) = spot_tile(&planet, spot);
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
        let store = StoreKey::Body(ground.id);
        let _ = world.with_store_ground(store, |world| {
            for (kind, dx, dy) in creatures {
                let (x, y) = world.frame().centre_of(column + dx, floor + dy);
                let _ = world.spawn_enemy(*kind, x, y);
            }
            let _ = TilePosition {
                x: column,
                y: floor,
            };
        });
        let _ = (Place::OnBody(BodyId::Space(ground.id)), seats);
    });
    stage.settle(90);
}

/// Coming down over the nearest `theme` world from space.
fn approach(stage: &mut SceneStage<'_>, theme: PlanetThemeId) {
    stage.play(1, |world, seats| {
        space_world(world);
        let Some(body) = wild_body(world, space_theme(theme)) else {
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

const JUNGLE_HERD: &[(EnemyKind, i32, i32)] = &[
    (EnemyKind::Trundlehorn, 7, -2),
    (EnemyKind::Mossprowler, -6, -1),
    (EnemyKind::Vinestalker, 3, -9),
];
const SPORE_CAVE: &[(EnemyKind, i32, i32)] =
    &[(EnemyKind::Sporewing, 3, -3), (EnemyKind::Sporewing, 4, -4)];
const OCEAN_LIFE: &[(EnemyKind, i32, i32)] = &[
    (EnemyKind::Tidesnapper, 5, -1),
    (EnemyKind::Tidesnapper, -5, -1),
];
const GLACIER_HERD: &[(EnemyKind, i32, i32)] = &[
    (EnemyKind::Woolhorn, 7, -2),
    (EnemyKind::Rimeskipper, -4, -1),
    (EnemyKind::Rimewolf, -8, -1),
];
const PERMAFROST: &[(EnemyKind, i32, i32)] = &[(EnemyKind::Crevassewyrm, 4, -1)];

pub const WILDS_SCENES: &[Scene] = &[
    Scene {
        name: "wild_jungle_approach",
        summary: "A jungle world from space: green under its glow, the craft coming down nose up",
        setup: |stage| approach(stage, PlanetThemeId::JUNGLE),
    },
    Scene {
        name: "wild_jungle_surface",
        summary: "On a jungle world at its settlement clearing: giant trees, canopy and vines round the level ground",
        setup: |stage| on_wild_world(stage, PlanetThemeId::JUNGLE, Spot::Settlement, &[]),
    },
    Scene {
        name: "wild_jungle_creatures",
        summary: "Jungle fauna about the clearing: a trundlehorn grazing, a mossprowler lying flat, a vinestalker hanging in the canopy",
        setup: |stage| on_wild_world(stage, PlanetThemeId::JUNGLE, Spot::Wilds, JUNGLE_HERD),
    },
    Scene {
        name: "wild_jungle_landmark",
        summary: "The Elder Tree: an eight-wide trunk on its rise, its crown, roots and the door to the hollow at its foot",
        setup: |stage| on_wild_world(stage, PlanetThemeId::JUNGLE, Spot::Landmark, &[]),
    },
    Scene {
        name: "wild_jungle_spore_caves",
        summary: "The Spore Caves: spore stone and mycelium, puffcaps on the cave floor, sporewings",
        setup: |stage| on_wild_world(stage, PlanetThemeId::JUNGLE, Spot::Stratum(1), SPORE_CAVE),
    },
    Scene {
        name: "wild_jungle_heartwood",
        summary: "The Heartwood Deep: red growth rings, amber with fossils, glowing lifesap",
        setup: |stage| on_wild_world(stage, PlanetThemeId::JUNGLE, Spot::Stratum(2), &[]),
    },
    Scene {
        name: "wild_ocean_approach",
        summary: "An ocean archipelago from space: blue sea, scattered islands, the craft coming down",
        setup: |stage| approach(stage, PlanetThemeId::OCEAN),
    },
    Scene {
        name: "wild_ocean_surface",
        summary: "On the settlement island of an ocean world: beach sand, the sea to the horizon",
        setup: |stage| on_wild_world(stage, PlanetThemeId::OCEAN, Spot::Settlement, OCEAN_LIFE),
    },
    Scene {
        name: "wild_ocean_landmark",
        summary: "The Drowned Spire rising out of the open sea, its dry room and flooded base",
        setup: |stage| on_wild_world(stage, PlanetThemeId::OCEAN, Spot::Landmark, &[]),
    },
    Scene {
        name: "wild_ocean_ruins",
        summary: "The Drowned Ruins: silt stone, flooded caves, a sealed hall's glyph walls",
        setup: |stage| on_wild_world(stage, PlanetThemeId::OCEAN, Spot::Stratum(1), &[]),
    },
    Scene {
        name: "wild_ocean_trench",
        summary: "The Trench: black basalt, flooded caves and black smokers",
        setup: |stage| on_wild_world(stage, PlanetThemeId::OCEAN, Spot::Stratum(2), &[]),
    },
    Scene {
        name: "wild_glacier_approach",
        summary: "A glacier world from space: white and pale blue, the craft coming down",
        setup: |stage| approach(stage, PlanetThemeId::GLACIER),
    },
    Scene {
        name: "wild_glacier_surface",
        summary: "On a glacier world: terraced ice under snow, a woolhorn herd, a rimewolf",
        setup: |stage| on_wild_world(stage, PlanetThemeId::GLACIER, Spot::Wilds, GLACIER_HERD),
    },
    Scene {
        name: "wild_glacier_landmark",
        summary: "The Frozen Titan: its tusk breaking the glacier, its bones in clear ice",
        setup: |stage| on_wild_world(stage, PlanetThemeId::GLACIER, Spot::Landmark, &[]),
    },
    Scene {
        name: "wild_glacier_sea",
        summary: "The Under-Ice Sea: sea ice round a lens of cold water",
        setup: |stage| on_wild_world(stage, PlanetThemeId::GLACIER, Spot::Stratum(1), &[]),
    },
    Scene {
        name: "wild_glacier_permafrost",
        summary: "The Permafrost Caves: blue-lined ice caves, icicles, a crevassewyrm",
        setup: |stage| on_wild_world(stage, PlanetThemeId::GLACIER, Spot::Stratum(2), PERMAFROST),
    },
];
