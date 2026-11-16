//! Sky continent scenes (Wave 4 T21, V9 worlds): a continent vista, a sky city, a temple's
//! shrine and halls, every sky enemy family, an airship battle and the Act II weapons (gravity
//! gun, arc rifle, tether harpoon).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops,
    clippy::manual_midpoint,
    clippy::or_fun_call,
    reason = "scene arithmetic on small tile and pixel numbers"
)]

use fatherload_simulation::{
    AmmoKind, EnemyKind, PlayerId, WeaponKind, WorldState,
    item::{BlockKind, ItemKind, ItemStack},
    terrain::{MineralKind, StrategicResourceKind, TILE_SIZE},
    weapon::MAX_BATTERY_LEVEL,
    worldgen::{
        TileRect,
        sky::{
            self, SkyLand,
            continent::{self, ContinentPlan},
        },
    },
};

use super::{Scene, SceneStage, reveal};
use crate::input::PlayerInput;

/// The bands of the scene world's own sky (each sky generator has its own table).
fn bands(world: &WorldState) -> fatherload_simulation::worldgen::sky::SkyBands {
    world
        .world_settings()
        .generator
        .sky_bands()
        .unwrap_or(fatherload_simulation::worldgen::sky::V8_SKY_BANDS)
}

fn pixel(tile: i32) -> f32 {
    (tile as f32 + 0.5) * TILE_SIZE
}

/// The V9 continent nearest town in the stage's world, with its plan.
fn nearest_continent(world: &WorldState) -> Option<(SkyLand, ContinentPlan)> {
    let settings = *world.world_settings();
    let band = TileRect {
        min: fatherload_simulation::terrain::TilePosition {
            x: -12_000,
            y: sky::row_of(bands(world).orbit),
        },
        max: fatherload_simulation::terrain::TilePosition {
            x: 12_000,
            y: sky::row_of(bands(world).continents),
        },
    };
    continent::continents_touching(&settings, &band)
        .into_iter()
        .min_by_key(|(land, _)| land.center_x.abs())
}

/// Fits the seat's rig for the high sky (big tank, engine, intakes, de-icer).
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 14;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(14);
        rig.fuel = rig.fuel_capacity * 0.8;
        rig.engine_level = 14;
        rig.hazard_gear.altitude_intake = 6;
        rig.hazard_gear.de_icer = 6;
        rig.hull = rig.max_hull();
    }
}

/// Puts the rig at world pixel `(x, y)` at rest and reveals around it.
fn put_rig(world: &mut WorldState, player: PlayerId, (x, y): (f32, f32)) {
    if let Some(rig) = world.player_mut(player) {
        rig.x = x;
        rig.y = y;
        rig.x_remainder = 0.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
    let (tx, ty) = ((x / TILE_SIZE) as i32, (y / TILE_SIZE) as i32);
    reveal(world, player, tx - 50..tx + 50, ty - 32..ty + 32);
    world.stream_terrain_near_players();
}

/// The rig standing on surface row `row` at column `x`.
fn stand_rig(world: &mut WorldState, player: PlayerId, x: i32, row: i32) {
    put_rig(world, player, (pixel(x), row as f32 * TILE_SIZE - 11.0));
}

/// Arms the rig with `weapon` (mounted, Mk II, a full big capacitor).
fn arm(world: &mut WorldState, player: PlayerId, weapon: WeaponKind) {
    if let Some(rig) = world.player_mut(player) {
        let _ = rig.receive_weapon(weapon);
        rig.weapon_rack.owned.insert(weapon, 1);
        let _ = rig.mount_weapon(Some(weapon));
        rig.weapon_rack.battery_level = MAX_BATTERY_LEVEL;
        rig.weapon.energy = rig.battery_capacity();
        rig.cargo_bay.add(AmmoKind::Rocket.item(), 6);
    }
}

/// Open air high over the continent's city (out of its safe zone), offset `dx` tiles.
fn hover_spot(plan: &ContinentPlan, dx: i32, up: i32) -> (f32, f32) {
    plan.city.map_or((pixel(100), pixel(-8_600)), |city| {
        (
            pixel(i32::midpoint(city.first, city.last) + dx),
            pixel(city.street_row - up),
        )
    })
}

/// Taps thrust so the rig hangs roughly still for `frames` frames (aiming `aim`, firing if
/// `fire`).
fn hang(stage: &mut SceneStage<'_>, frames: usize, aim: (f32, f32), fire: bool) {
    let pinned = stage
        .world()
        .player_ids()
        .next()
        .and_then(|id| stage.world().player(id).map(|rig| (id, rig.x, rig.y)));
    for _ in 0..frames {
        // Pin the rig in place: the scene is about what flies around it.
        if let Some((id, x, y)) = pinned {
            stage.play_on(|world| {
                if let Some(rig) = world.player_mut(id) {
                    rig.x = x;
                    rig.y = y;
                    rig.velocity_x = 0.0;
                    rig.velocity_y = 0.0;
                }
            });
        }
        stage.hold(
            1,
            PlayerInput {
                thrust: false,
                aim_x: aim.0,
                aim_y: aim.1,
                fire,
                ..PlayerInput::default()
            },
        );
    }
}

fn aim(aim_x: f32, aim_y: f32, fire: bool) -> PlayerInput {
    PlayerInput {
        aim_x,
        aim_y,
        fire,
        ..PlayerInput::default()
    }
}

pub const SKY_CONTINENT_SCENES: &[Scene] = &[
    Scene {
        name: "sky_continent_vista",
        summary: "A sky continent: snow-capped peaks veined with storm glass, a harpy nest on the summit, a lake in its basin",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((land, plan)) = nearest_continent(world) else {
                    return;
                };
                let peak = plan
                    .peaks
                    .iter()
                    .flatten()
                    .max_by_key(|peak| peak.height)
                    .copied();
                let (x, row) = peak.map_or((land.center_x, land.top_row - 12), |peak| {
                    (peak.center - 12, land.top_row - peak.height + 4)
                });
                put_rig(world, player, (pixel(x), pixel(row)));
            });
            hang(stage, 14, (1.0, 0.0), false);
        },
    },
    Scene {
        name: "sky_continent_lake",
        summary: "A sky continent's lake: open water in a basin on the high plains, caves and the glowing aether core below",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((land, plan)) = nearest_continent(world) else {
                    return;
                };
                let lake = plan.lake.map_or(land.center_x, |lake| lake.center);
                stand_rig(
                    world,
                    player,
                    lake - plan.lake.map_or(0, |l| l.half_width) - 4,
                    land.top_row,
                );
                let (tx, ty) = (lake, land.top_row);
                reveal(world, player, tx - 60..tx + 60, ty - 30..ty + 80);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "sky_continent_core",
        summary: "Inside a sky continent: winding caves, aether crystal glowing around the indestructible aether core",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                let core = plan.core;
                // Hollow a pocket beside the core for the rig.
                let (x, y) = (core.center_x - core.half_width - 4, core.center_y);
                for dy in -2..=1 {
                    for dx in -3..=3 {
                        let _ = world.terrain_mut().set_kind(
                            fatherload_simulation::terrain::TilePosition {
                                x: x + dx,
                                y: y + dy,
                            },
                            fatherload_simulation::terrain::TileKind::Air,
                        );
                    }
                }
                put_rig(world, player, (pixel(x), pixel(y)));
            });
            stage.settle(6);
        },
    },
    Scene {
        name: "sky_city",
        summary: "A sky city on a continent: aeronaut towers, a domed hall, the cloud market, the monastery, hangars and sky folk",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                if let Some(city) = plan.city {
                    stand_rig(
                        world,
                        player,
                        (city.first + city.last) / 2 - 3,
                        city.street_row,
                    );
                }
            });
            stage.settle(150);
        },
    },
    Scene {
        name: "sky_temple_shrine",
        summary: "A sky temple's shrine on a continent plain: the pilot crawls in through the vent grate the rig cannot pass",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                let Some(temple) = plan.temple else {
                    return;
                };
                let entrance = temple.entrance();
                let side = if temple.mirrored { 1 } else { -1 };
                let ground = temple.top + continent::TEMPLE_GROUND_ROW;
                stand_rig(world, player, entrance.x + side * 5, ground);
                let _ =
                    world.place_pilot(player, pixel(entrance.x + side), ground as f32 * TILE_SIZE);
            });
            stage.hold(
                20,
                PlayerInput {
                    drill_down: true,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "sky_temple_halls",
        summary: "Inside a sky temple: the lever hall, the bronze gate, the keystone on its plinth beyond and a sentry on guard",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                let Some(temple) = plan.temple else {
                    return;
                };
                let entrance = temple.entrance();
                let side = if temple.mirrored { 1 } else { -1 };
                let ground = temple.top + continent::TEMPLE_GROUND_ROW;
                stand_rig(world, player, entrance.x + side * 5, ground);
                let spot = temple.world((15, 15));
                let rect = temple.rect();
                reveal(
                    world,
                    player,
                    rect.min.x - 4..rect.max.x + 4,
                    rect.min.y - 4..rect.max.y + 4,
                );
                let _ = world.place_pilot(player, pixel(spot.x), (spot.y + 1) as f32 * TILE_SIZE);
                let guard = temple.guard_posts()[0];
                let _ = world.spawn_enemy(
                    EnemyKind::SentryTurret,
                    pixel(guard.x),
                    pixel(guard.y) + 4.0,
                );
            });
            stage.settle(8);
        },
    },
    Scene {
        name: "sky_temple_vault",
        summary: "A sky temple's vault opened with the keystone: relic chests glowing behind the bronze keystone gate",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                let Some(temple) = plan.temple else {
                    return;
                };
                let ground = temple.top + continent::TEMPLE_GROUND_ROW;
                let entrance = temple.entrance();
                let side = if temple.mirrored { 1 } else { -1 };
                stand_rig(world, player, entrance.x + side * 5, ground);
                let rect = temple.rect();
                reveal(
                    world,
                    player,
                    rect.min.x - 4..rect.max.x + 4,
                    rect.min.y - 4..rect.max.y + 4,
                );
                let mut output = fatherload_simulation::TickOutput::default();
                let lever = temple.tiles_of('L')[0];
                let _ = world.work_temple_fitting(
                    player,
                    lever,
                    fatherload_simulation::sky::SkyTile::TempleLever,
                    &mut output,
                );
                if let Some(rig) = world.player_mut(player) {
                    rig.pilot_pack
                        .add(fatherload_simulation::item::TEMPLE_KEYSTONE, 1);
                }
                let gate = temple.tiles_of('K')[0];
                let _ = world.work_temple_fitting(
                    player,
                    gate,
                    fatherload_simulation::sky::SkyTile::KeystoneDoor,
                    &mut output,
                );
                let spot = temple.world((44, 20));
                let _ = world.place_pilot(player, pixel(spot.x), (spot.y + 1) as f32 * TILE_SIZE);
            });
            stage.settle(8);
        },
    },
    Scene {
        name: "sky_enemy_rays",
        summary: "Sky rays: a school of blue mantas over the continent, one hanging above the rig, one diving through it",
        setup: |stage| {
            let mut at = (0.0, 0.0);
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                at = hover_spot(&plan, 0, 32);
                put_rig(world, player, at);
                for (dx, dy) in [(-5.0, -4.0), (4.0, -5.0), (7.0, 1.0)] {
                    let _ = world.spawn_enemy(
                        EnemyKind::SkyRay,
                        at.0 + dx * TILE_SIZE,
                        at.1 + dy * TILE_SIZE,
                    );
                }
            });
            hang(stage, 70, (1.0, 0.0), false);
        },
    },
    Scene {
        name: "sky_enemy_wyverns",
        summary: "Wyverns: rust-red sky drakes circling to dive, one spitting fire from afar",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                let at = hover_spot(&plan, 10, 32);
                put_rig(world, player, at);
                for (dx, dy) in [(-7.0, -4.0), (9.0, -2.0)] {
                    let _ = world.spawn_enemy(
                        EnemyKind::Wyvern,
                        at.0 + dx * TILE_SIZE,
                        at.1 + dy * TILE_SIZE,
                    );
                }
            });
            hang(stage, 60, (1.0, 0.0), false);
        },
    },
    Scene {
        name: "sky_enemy_storm",
        summary: "A storm elemental's lightning arc jolting the rig: capacitor drained, weapon jammed",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                arm(world, player, WeaponKind::RivetGun);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                let at = hover_spot(&plan, -10, 32);
                put_rig(world, player, at);
                let _ = world.spawn_enemy(
                    EnemyKind::StormElemental,
                    at.0 - 6.0 * TILE_SIZE,
                    at.1 - 3.0 * TILE_SIZE,
                );
                let _ = world.spawn_enemy(
                    EnemyKind::StormElemental,
                    at.0 + 7.0 * TILE_SIZE,
                    at.1 - 1.0 * TILE_SIZE,
                );
            });
            // Hang until the first arc has just struck.
            for _ in 0..120 {
                hang(stage, 1, (1.0, 0.0), false);
                let struck = stage
                    .world()
                    .player_ids()
                    .next()
                    .and_then(|id| stage.world().player(id))
                    .is_some_and(|rig| rig.weapon.energy < 1.0);
                if struck {
                    break;
                }
            }
            hang(stage, 3, (1.0, 0.0), false);
        },
    },
    Scene {
        name: "sky_enemy_harpies",
        summary: "A harpy nester rising from its nest of eggs on a summit as the pilot comes too close",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((land, plan)) = nearest_continent(world) else {
                    return;
                };
                let nest = plan.nests[0].unwrap_or(fatherload_simulation::terrain::TilePosition {
                    x: land.center_x,
                    y: land.top_row,
                });
                put_rig(world, player, (pixel(nest.x - 7), pixel(nest.y - 4)));
                let _ = world.spawn_enemy(EnemyKind::HarpyNester, pixel(nest.x), pixel(nest.y - 3));
            });
            hang(stage, 40, (1.0, 0.0), false);
        },
    },
    Scene {
        name: "sky_enemy_pirates",
        summary: "Sky pirates: an airship with brass turrets drops a jetpack boarding party onto the pilot on foot",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let Some((land, plan)) = nearest_continent(world) else {
                    return;
                };
                let Some(temple) = plan.temple else {
                    return;
                };
                let ground = temple.top + continent::TEMPLE_GROUND_ROW;
                // The temple's side away from the city (whose safe zone would calm the pirates).
                let city_east = plan.city.is_some_and(|city| city.first > temple.left);
                let side = if city_east { 1 } else { -1 };
                let x = if city_east {
                    temple.left - 3
                } else {
                    temple.left + continent::TEMPLE_WIDTH + 2
                };
                stand_rig(world, player, x + side * 2, ground);
                let _ = world.place_pilot(player, pixel(x - side), ground as f32 * TILE_SIZE);
                let _ = land;
                let _hull = world.spawn_airship(pixel(x - side * 6), pixel(ground - 8));
            });
            stage.settle(150);
        },
    },
    Scene {
        name: "sky_airship_battle",
        summary: "An airship battle: the rig's flak cannon bursting shrapnel around a pirate airship, its turrets firing back",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                arm(world, player, WeaponKind::FlakCannon);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                let at = hover_spot(&plan, -20, 32);
                put_rig(world, player, at);
                let _ = world.spawn_airship(at.0 + 9.0 * TILE_SIZE, at.1 - 4.0 * TILE_SIZE);
            });
            for frame in 0..70 {
                let target = stage.world().enemies().find_map(|(entity, state)| {
                    (state.kind == EnemyKind::PirateAirship)
                        .then_some((entity.body.x, entity.body.y))
                });
                let rig = stage
                    .world()
                    .player_ids()
                    .next()
                    .and_then(|id| stage.world().player(id))
                    .map(|rig| (rig.x, rig.y));
                let aim_at = match (target, rig) {
                    (Some(t), Some(r)) => {
                        let (dx, dy) = (t.0 - r.0, t.1 - r.1);
                        let length = dx.hypot(dy).max(1.0);
                        (dx / length, dy / length)
                    }
                    _ => (1.0, -0.4),
                };
                hang(stage, 1, aim_at, frame % 20 < 14);
            }
        },
    },
    Scene {
        name: "sky_gravity_gun",
        summary: "The gravity gun: a violet field holding a sky ray helpless while loot and a ripped-out block swirl in",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                arm(world, player, WeaponKind::GravityGun);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                let Some(city) = plan.city else {
                    return;
                };
                // On the city's edge (calm), facing out over the plains.
                let x = city.last + 2;
                stand_rig(world, player, x, city.street_row);
                let (rx, ry) = (pixel(x), city.street_row as f32 * TILE_SIZE - 11.0);
                for (index, item) in [
                    ItemKind::Mineral(MineralKind::Gold),
                    ItemKind::Material(StrategicResourceKind::StormGlass),
                    ItemKind::Block(BlockKind::Stone),
                ]
                .into_iter()
                .enumerate()
                {
                    let _ = world.spawn_item_drop(
                        rx + (4.0 + index as f32 * 1.5) * TILE_SIZE,
                        ry - 6.0 - index as f32 * 10.0,
                        (0.0, 0.0),
                        ItemStack::new(item, 1),
                        None,
                    );
                }
                let _ = world.spawn_enemy(
                    EnemyKind::SkyRay,
                    rx + 5.0 * TILE_SIZE,
                    ry - 2.5 * TILE_SIZE,
                );
            });
            stage.settle(4);
            stage.hold(16, aim(1.0, -0.35, true));
        },
    },
    Scene {
        name: "sky_arc_rifle",
        summary: "The arc rifle: lightning leaping from a sky ray to a wyvern to another ray",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                arm(world, player, WeaponKind::ArcRifle);
                let Some((_, plan)) = nearest_continent(world) else {
                    return;
                };
                let at = hover_spot(&plan, 25, 32);
                put_rig(world, player, at);
                let _ = world.spawn_enemy(EnemyKind::SkyRay, at.0 + 4.0 * TILE_SIZE, at.1 - 14.0);
                let _ = world.spawn_enemy(
                    EnemyKind::Wyvern,
                    at.0 + 7.5 * TILE_SIZE,
                    at.1 - 2.5 * TILE_SIZE,
                );
                let _ = world.spawn_enemy(
                    EnemyKind::SkyRay,
                    at.0 + 9.0 * TILE_SIZE,
                    at.1 + 1.5 * TILE_SIZE,
                );
            });
            hang(stage, 3, (1.0, 0.0), false);
            hang(stage, 2, (1.0, 0.0), true);
        },
    },
    Scene {
        name: "sky_tether_swing",
        summary: "The tether harpoon: the rig hooked into a continent's underside, swinging on its line",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                arm(world, player, WeaponKind::TetherHarpoon);
                let Some((land, plan)) = nearest_continent(world) else {
                    return;
                };
                let x = plan.city.map_or(land.center_x, |city| city.first + 10);
                let under = land.underside_row(x).unwrap_or(land.top_row + land.depth);
                put_rig(world, player, (pixel(x), pixel(under + 7)));
                if let Some(rig) = world.player_mut(player) {
                    rig.velocity_x = 260.0;
                }
            });
            stage.hold(1, aim(-0.3, -1.0, true));
            stage.hold(40, aim(-0.3, -1.0, true));
        },
    },
];
