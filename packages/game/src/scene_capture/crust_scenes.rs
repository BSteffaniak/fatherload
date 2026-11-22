//! The authored Crust (Wave 8, generator V11's Descent I): every stratum in its own look with
//! its signature on show, the lost miner's cabin, an aquifer flooding the tunnel that breached
//! it, the old mine works with a cart ride and a timber set coming down, Gravel Hollow and its
//! people, the slate shelves' stalactite galleries, a lava tube's heat, the Crust's nine
//! creatures, and the Sinkhole (its core, and the collapse climbing after the escaping rig).

use fatherload_simulation::{
    PlayerId, WorldState,
    crust::{self, CrustTile},
    depth_progression::ProgressionRules,
    descent::Stratum,
    enemy::EnemyKind,
    fluid::{FLUID_FULL, FluidKind},
    terrain::{Terrain, TileKind, TilePosition},
    worldgen::{
        GeneratorVersion, WorldSettings,
        descent::{self, crust as crust_gen, landmarks},
    },
};

use super::{Scene, SceneStage, carve, place_rig, reveal, tile_centre};
use crate::input::PlayerInput;

/// Seed of every Crust scene world.
const SCENE_SEED: u64 = 0x0C2B_57A1;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// Swaps the staged world's terrain for the Crust scene world (Descent rules, its outposts).
fn crust_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
    world.set_progression_rules(ProgressionRules::Descent);
    world.install_logistics_sites(&landmarks::logistics_sites(&settings()), false);
    world.stream_terrain_near_players();
}

/// A Descent I rig: a mid tank, the drill and hull a Crust rig would have.
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.drill_strength = 5;
        rig.engine_level = 5;
        rig.radiator_level = 3;
        rig.hull_level = 5;
        rig.hull = rig.max_hull();
        rig.fuel = rig.fuel_capacity * 0.8;
    }
}

/// Places the rig at `(x, y)`, the rock around it explored, chunks loaded.
fn stand(world: &mut WorldState, player: PlayerId, (x, y): (i32, i32), reach: (i32, i32)) {
    place_rig(world, player, x, y);
    reveal(
        world,
        player,
        x - reach.0..x + reach.0,
        y - reach.1..y + reach.1,
    );
    world.stream_terrain_near_players();
}

/// The first spot from `(x0, row)` outwards whose tile is open with ground under it.
fn open_spot(world: &WorldState, x0: i32, rows: std::ops::Range<i32>) -> (i32, i32) {
    let first = rows.start;
    for row in rows {
        for step in 0..300 {
            let x = x0
                + if step % 2 == 0 {
                    step / 2
                } else {
                    -(step / 2) - 1
                };
            let open = |dx: i32, dy: i32| {
                world
                    .terrain()
                    .tile(TilePosition {
                        x: x + dx,
                        y: row + dy,
                    })
                    .is_some_and(|tile| tile.kind == TileKind::Air)
            };
            let floor = world.terrain().is_solid_at(TilePosition { x, y: row + 1 });
            if open(0, 0) && open(0, -1) && open(-1, 0) && open(1, 0) && floor {
                return (x, row);
            }
        }
    }
    (x0, first)
}

/// The rows of `stratum` in the scene world.
const fn rows(stratum: Stratum) -> (i32, i32) {
    descent::layout(&settings()).rows(stratum)
}

/// A rig in a 40 x 10 chamber dug a third of the way down `stratum` (its rock, ores and
/// features around it), with `creatures` about.
fn stratum_scene(stage: &mut SceneStage<'_>, stratum: Stratum, creatures: &'static [EnemyKind]) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        crust_world(world);
        fit(world, player);
        let (top, bottom) = rows(stratum);
        let row = top + (bottom - top) / 3;
        let x = 40;
        carve(world, x - 6..x + 7, row - 2..row + 1);
        stand(world, player, (x, row), (46, 18));
        for (index, kind) in (0_i32..).zip(creatures) {
            let (cx, cy) = open_spot(world, x + 10 + index * 6, row - 8..row + 8);
            let _ = world.spawn_enemy(*kind, tile_centre(cx), tile_centre(cy));
        }
    });
    // Past the "entering" banner.
    stage.settle(250);
}

/// The old mine works of the scene world nearest its middle.
fn works() -> crust_gen::MineWorks {
    let (top, bottom) = rows(Stratum::GreyStone);
    (0..20)
        .flat_map(|ry| (-3..3).map(move |rx| (rx, ry)))
        .find_map(|(rx, ry)| crust_gen::mine_works(SCENE_SEED, top, bottom, rx, ry))
        .expect("the scene world has old works")
}

pub const CRUST_SCENES: &[Scene] = &[
    Scene {
        name: "crust_topsoil",
        summary: "The Topsoil: rain-soaked mud, root mats under the grass, burrows and a burrow mole",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let (top, _) = rows(Stratum::Topsoil);
                let row = top + 22;
                carve(world, 54..70, row - 2..row + 1);
                stand(world, player, (60, row), (46, 26));
                let (mx, my) = open_spot(world, 72, row - 4..row + 10);
                let _ = world.spawn_enemy(EnemyKind::BurrowMole, tile_centre(mx), tile_centre(my));
                let (lx, ly) = open_spot(world, 52, row - 2..row + 6);
                let _ = world.spawn_enemy(EnemyKind::MudLeech, tile_centre(lx), tile_centre(ly));
            });
            stage.settle(250);
        },
    },
    Scene {
        name: "crust_cabin",
        summary: "The lost miner's cabin: log walls buried in the Topsoil, his strongbox and the old entrance shaft of rubble",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let cabin = crust::cabin(&settings()).expect("a cabin");
                stand(world, player, (cabin.min.x + 3, cabin.max.y - 1), (30, 20));
            });
            stage.settle(250);
        },
    },
    Scene {
        name: "crust_clay_beds",
        summary: "The Clay Beds: aquifers sealed in the clay (damp stains give them away), clay pots and potter wasps",
        setup: |stage| {
            stratum_scene(
                stage,
                Stratum::ClayBeds,
                &[EnemyKind::PotterWasp, EnemyKind::PotterWasp],
            );
        },
    },
    Scene {
        name: "crust_aquifer_flood",
        summary: "An aquifer breached: water floods the tunnel the drill opened into it",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let (top, bottom) = rows(Stratum::ClayBeds);
                let row = i32::midpoint(top, bottom);
                let x0 = 30;
                for y in row - 8..row + 4 {
                    for x in x0 - 4..x0 + 26 {
                        let _ = world
                            .terrain_mut()
                            .set_kind(TilePosition { x, y }, TileKind::Clay);
                    }
                }
                carve(world, x0 - 2..x0 + 7, row - 1..row + 1);
                for y in row - 5..=row {
                    for x in x0 + 8..=x0 + 18 {
                        let _ = world.terrain_mut().set_tile_from_network(
                            TilePosition { x, y },
                            TileKind::Fluid(FluidKind::Water),
                            FLUID_FULL,
                        );
                    }
                }
                stand(world, player, (x0 + 5, row), (30, 14));
            });
            // Drill east into the pocket, then back off while it floods.
            stage.hold(
                150,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
            stage.hold(
                40,
                PlayerInput {
                    horizontal: -1.0,
                    thrust: true,
                    ..PlayerInput::default()
                },
            );
            stage.settle(30);
        },
    },
    Scene {
        name: "crust_grey_stone",
        summary: "The Grey Stone's old mine works: a timbered gallery on a rail line, lanterns, a stope with a strongbox, mine bats",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let works = works();
                stand(
                    world,
                    player,
                    (works.stope.0 + 2, works.rail_row - 1),
                    (40, 16),
                );
                for offset in [14, 16] {
                    let _ = world.spawn_enemy(
                        EnemyKind::MineBat,
                        tile_centre(works.first + offset),
                        tile_centre(works.rail_row - 3),
                    );
                }
                let _ = world.spawn_enemy(
                    EnemyKind::TimberBeetle,
                    tile_centre(works.first + 10),
                    tile_centre(works.rail_row - 1),
                );
            });
            stage.settle(250);
        },
    },
    Scene {
        name: "crust_cart_ride",
        summary: "A cart ride: the rig hooked onto the old works' rails, rolling fast down the gallery",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let works = works();
                stand(
                    world,
                    player,
                    (works.stope.1 + 2, works.rail_row - 1),
                    (60, 16),
                );
            });
            stage.settle(170);
            stage.hold(
                90,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "crust_support_collapse",
        summary: "A timber set undercut: the beam gives and the roof comes down into the gallery",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let works = works();
                let x = works.last - 8;
                stand(world, player, (x, works.rail_row - 1), (30, 14));
                // The rig's mate broke the floor under the set a moment ago.
                let _ = world.terrain_mut().set_kind(
                    TilePosition {
                        x: x - 4,
                        y: works.rail_row,
                    },
                    TileKind::Air,
                );
            });
            // Drill down through the rail bed under the set, then sit back.
            stage.settle(80);
            stage.hold(
                60,
                PlayerInput {
                    drill_down: true,
                    ..PlayerInput::default()
                },
            );
            stage.settle(66);
        },
    },
    Scene {
        name: "crust_gravel_hollow",
        summary: "Gravel Hollow: the Grey Stone's miners' settlement beside the Crust's outpost, its people about their day",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let (center, top) = crust::gravel_hollow(&settings()).expect("Gravel Hollow");
                let floor = top + fatherload_simulation::village::placement::HALL_AIR_ROWS;
                stand(world, player, (center, floor - 1), (50, 18));
            });
            stage.settle(250);
        },
    },
    Scene {
        name: "crust_slate_shelves",
        summary: "The Slate Shelves: slick shelves banding the rock, a stalactite gallery, a slate skink and a shelf crab",
        setup: |stage| {
            stratum_scene(
                stage,
                Stratum::SlateShelves,
                &[EnemyKind::SlateSkink, EnemyKind::ShelfCrab],
            );
        },
    },
    Scene {
        name: "crust_lava_tube",
        summary: "The Basalt Columns: inside a lava tube, its glass glowing, basalt split into columns, a tube crawler and cinder moths",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let settings = settings();
                let (top, bottom) = rows(Stratum::BasaltColumns);
                let (x, y) = (top..bottom)
                    .flat_map(|y| (0..600).map(move |x| (x, y)))
                    .find(|(x, y)| {
                        // Well inside a long tube, on its floor, so it runs off both sides of
                        // the view.
                        (-24..=24).step_by(6).all(|dx| {
                            (-1..=1).any(|dy| crust::lava_tube_at(&settings, x + dx, y + dy))
                        }) && crust::lava_tube_at(&settings, *x, *y)
                            && !crust::lava_tube_at(&settings, *x, y + 1)
                    })
                    .unwrap_or((40, top + 20));
                stand(world, player, (x, y), (46, 16));
                let _ =
                    world.spawn_enemy(EnemyKind::TubeCrawler, tile_centre(x + 9), tile_centre(y));
                for dx in [5, 13] {
                    let _ = world.spawn_enemy(
                        EnemyKind::CinderMoth,
                        tile_centre(x + dx),
                        tile_centre(y - 1),
                    );
                }
            });
            stage.settle(250);
        },
    },
    Scene {
        name: "crust_creatures",
        summary: "The Crust's nine creatures in a row: mole, leech, wasp, bat, beetle, skink, crab, crawler, moth",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let (top, _) = rows(Stratum::GreyStone);
                let row = top + 40;
                for y in row - 6..row + 3 {
                    for x in 10..70 {
                        let _ = world.terrain_mut().set_kind(
                            TilePosition { x, y },
                            if y > row {
                                TileKind::Stone
                            } else {
                                TileKind::Air
                            },
                        );
                    }
                }
                stand(world, player, (30, row), (40, 12));
                let kinds = [
                    EnemyKind::BurrowMole,
                    EnemyKind::MudLeech,
                    EnemyKind::PotterWasp,
                    EnemyKind::MineBat,
                    EnemyKind::TimberBeetle,
                    EnemyKind::SlateSkink,
                    EnemyKind::ShelfCrab,
                    EnemyKind::TubeCrawler,
                    EnemyKind::CinderMoth,
                ];
                for (index, kind) in (0_i32..).zip(kinds) {
                    // Three tiles apart either side of the rig.
                    let x = 16 + index * 3 + if index >= 4 { 4 } else { 0 };
                    let y = if matches!(
                        kind.stats().locomotion,
                        fatherload_simulation::enemy::Locomotion::Flying
                    ) {
                        row - 3
                    } else {
                        row
                    };
                    let _ = world.spawn_enemy(kind, tile_centre(x), tile_centre(y));
                }
            });
            stage.settle(4);
        },
    },
    Scene {
        name: "crust_sinkhole",
        summary: "The Sinkhole: the gate chamber at the end of the Crust, its humming core in the floor and the Fathers' strongbox",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let site = crust::sinkhole_site(&settings()).expect("a sinkhole");
                stand(world, player, (site.core.x - 6, site.core.y), (40, 24));
                reveal(
                    world,
                    player,
                    site.zone.min.x..site.zone.max.x,
                    site.lip_row..site.chamber.max.y + 2,
                );
            });
            stage.settle(250);
        },
    },
    Scene {
        name: "crust_sinkhole_escape",
        summary: "The Sinkhole letting go: the core out, the collapse boiling up the funnel after the climbing rig",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                crust_world(world);
                fit(world, player);
                let site = crust::sinkhole_site(&settings()).expect("a sinkhole");
                stand(world, player, (site.core.x - 1, site.core.y), (40, 30));
                reveal(
                    world,
                    player,
                    site.zone.min.x..site.zone.max.x,
                    site.lip_row..site.chamber.max.y + 2,
                );
                let _ = world
                    .terrain()
                    .tile(site.core)
                    .is_some_and(|tile| tile.kind == TileKind::Crust(CrustTile::SinkholeCore));
            });
            // Drill the core out, sit out most of the rumble, then climb hard: the collapse
            // starts boiling up the funnel a few rows under the rig.
            stage.hold(
                120,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
            stage.hold(125, PlayerInput::default());
            stage.hold(
                55,
                PlayerInput {
                    thrust: true,
                    ..PlayerInput::default()
                },
            );
        },
    },
];
