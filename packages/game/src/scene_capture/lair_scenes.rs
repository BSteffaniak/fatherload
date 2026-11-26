//! The Wave 12 worlds' theme bosses in their lairs (Wave 14 finish): the nearest jungle,
//! ocean, glacier, dune, volcanic and crystal planet to home on the flight scenes' seed, the
//! rig on its lair's floor with the boss awake across the hall (the seal glowing between
//! them, the lamps along the walls), and the Tidecaller's dry well down the Drowned Spire.
//! Every scene stands on the body's round ground (its store, the polar renderer), the camera
//! locked to the craft.

use fatherload_simulation::{
    WorldState,
    enemy::EnemyKind,
    flight::{BodyRef, FlightBody, SpaceQuery, StarDrive},
    place::StoreKey,
    planet::{BodyGround, PlanetGround, PlanetThemeId, lairs::lair_of},
    space::{BodyKind, HOME_SECTOR, SpaceSector, Theme, generate_sector},
};

use super::{
    Scene, SceneStage,
    flight_scenes::{launch, set_down, space_world},
};

/// The nearest planet of `theme` to home on the scenes' seed with a lair.
fn lair_body(world: &WorldState, theme: Theme) -> Option<FlightBody> {
    let galaxy = world.space();
    for ring in 0..8_i32 {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let sector = generate_sector(
                    world.world_seed(),
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
        PlanetThemeId::JUNGLE => Theme::Jungle,
        PlanetThemeId::OCEAN => Theme::OceanArchipelago,
        PlanetThemeId::GLACIER => Theme::Ice,
        fatherload_simulation::frontier::DUNE_WORLD => Theme::DesertDunes,
        fatherload_simulation::frontier::VOLCANIC_WORLD => Theme::Volcanic,
        _ => Theme::Crystal,
    }
}

/// Where the rig stands.
#[derive(Clone, Copy)]
enum Spot {
    /// On the hall's floor west of the seal, facing the boss.
    Hall,
    /// Halfway down the way in (the Tidecaller's dry well).
    Well,
}

/// Lands on the nearest `theme` world in its lair and wakes `boss` across the hall.
fn in_lair(stage: &mut SceneStage<'_>, theme: PlanetThemeId, spot: Spot, boss: Option<EnemyKind>) {
    stage.play(1, |world, seats| {
        space_world(world);
        world.enable_test_mode();
        let Some(body) = lair_body(world, space_theme(theme)) else {
            return;
        };
        let Some(ground) = world.body_ground(body.body) else {
            return;
        };
        let planet = PlanetGround::new(ground.params);
        let Some(lair) = lair_of(&planet) else {
            return;
        };
        let width = i64::from(planet.strip_width());
        let (column, floor) = match spot {
            Spot::Hall => (
                i32::try_from((lair.centre - 5).rem_euclid(width)).unwrap_or(0),
                lair.floor,
            ),
            Spot::Well => (
                i32::try_from(lair.shaft.rem_euclid(width)).unwrap_or(0),
                i32::midpoint(lair.shaft_top, lair.top),
            ),
        };
        let store = StoreKey::Body(ground.id);
        let morning = world
            .body_life(store)
            .map_or(0, |life| life.clock.millis_when(0, i64::from(column), 420));
        world.set_body_millis(morning);
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
        if let Some(kind) = boss {
            let _ = world.with_store_ground(store, |world| {
                // Close enough to stay clear of the body panel in a 960-point view, just past
                // the seal (review: at 13 tiles the panel hid its head).
                let (x, y) = world.frame().centre_of(column + 7, lair.floor - 2);
                let _ = world.spawn_enemy(kind, x, y);
            });
        }
    });
    stage.settle(60);
    // The road's and the archivist's toasts belong to other scenes.
    stage.seat(0).message.clear();
}

pub const LAIR_SCENES: &[Scene] = &[
    Scene {
        name: "lair_jungle_boss",
        summary: "The Bramble King in the root hall under the Elder Tree, the seal glowing between",
        setup: |stage| {
            in_lair(
                stage,
                PlanetThemeId::JUNGLE,
                Spot::Hall,
                Some(EnemyKind::BrambleKing),
            );
        },
    },
    Scene {
        name: "lair_ocean_boss",
        summary: "The Tidecaller in the dry hall under the Drowned Spire, its pearl lantern lit",
        setup: |stage| {
            in_lair(
                stage,
                PlanetThemeId::OCEAN,
                Spot::Hall,
                Some(EnemyKind::Tidecaller),
            );
        },
    },
    Scene {
        name: "lair_ocean_well",
        summary: "The dry well down the Drowned Spire, the sea outside its walls",
        setup: |stage| in_lair(stage, PlanetThemeId::OCEAN, Spot::Well, None),
    },
    Scene {
        name: "lair_glacier_boss",
        summary: "The Hoarfrost Wyrm in the ice cave under the Frozen Titan",
        setup: |stage| {
            in_lair(
                stage,
                PlanetThemeId::GLACIER,
                Spot::Hall,
                Some(EnemyKind::HoarfrostWyrm),
            );
        },
    },
    Scene {
        name: "lair_dunes_boss",
        summary: "The Dune Tyrant in the buried hall under the Sleeping Sentinel",
        setup: |stage| {
            in_lair(
                stage,
                fatherload_simulation::frontier::DUNE_WORLD,
                Spot::Hall,
                Some(EnemyKind::DuneTyrant),
            );
        },
    },
    Scene {
        name: "lair_volcanic_boss",
        summary: "The Magma Drake in the magma hall under the Caldera",
        setup: |stage| {
            in_lair(
                stage,
                fatherload_simulation::frontier::VOLCANIC_WORLD,
                Spot::Hall,
                Some(EnemyKind::MagmaDrake),
            );
        },
    },
    Scene {
        name: "lair_crystal_boss",
        summary: "The Geode Colossus in the geode hall under the Choir Spire",
        setup: |stage| {
            in_lair(
                stage,
                fatherload_simulation::frontier::CRYSTAL_WORLD,
                Spot::Hall,
                Some(EnemyKind::GeodeColossus),
            );
        },
    },
];
