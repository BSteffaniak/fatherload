//! Top-down crypt scenes (WP-TOMBS T29, V10 worlds): each tile set (catacomb, sandstone, sky
//! vault, lunar vault), combat with a skeleton pack, spectres drifting through the walls, the
//! traps (spikes, flame jets, arrow plates, a boulder chute, cracked floors and pits), a dodge
//! roll, the crypt lord's hall, the room map, split screen with one seat in a crypt from
//! above and the other in the side view outside, and the phone touch layout.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops,
    reason = "scene layouts mix small tile indices and world pixels"
)]

use fatherload_simulation::{
    EnemyKind, PlayerId, WorldState,
    enemy::EnemyMode,
    entity::EntityState,
    pilot::PilotGear,
    terrain::{TILE_SIZE, TilePosition},
    tomb::{
        Relic, RoomKind, SiteSetting, SiteStyle, TombSite,
        crypt::{CryptTrap, CryptTrapKind},
    },
    worldgen::{features::TileRect, tomb_sites},
};

use super::{Scene, SceneStage, reveal};
use crate::input::{
    devices::InputDevice,
    touch::{TouchControl, TouchView},
};

/// A top-down crypt site of `setting` (ground, then the sky continents and the Moon).
fn crypt_site(world: &WorldState, setting: SiteSetting) -> Option<TombSite> {
    let settings = world.terrain().settings();
    let mut rects = vec![
        TileRect {
            min: TilePosition {
                x: -12_000,
                y: -100,
            },
            max: TilePosition { x: 12_000, y: 900 },
        },
        TileRect {
            min: TilePosition {
                x: -24_000,
                y: -16_400,
            },
            max: TilePosition {
                x: 24_000,
                y: -9_000,
            },
        },
    ];
    if let Some(moon) = fatherload_simulation::worldgen::sky::moon(settings) {
        rects.push(moon.bounds());
    }
    rects.into_iter().find_map(|rect| {
        tomb_sites::sites_touching(settings, &rect)
            .into_iter()
            .find(|site| site.style == SiteStyle::TopDown && site.setting == setting)
    })
}

/// A well kitted pilot with a rifle.
fn kit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.pilot_gear = PilotGear {
            jetpack: 1,
            suit: 2,
            headlamp: 2,
            ..PilotGear::default()
        };
        let _ = rig
            .pilot_arsenal
            .receive(fatherload_simulation::WeaponKind::Rifle);
        rig.pilot_arsenal.equipped = Some(fatherload_simulation::WeaponKind::Rifle);
        rig.credits = 3_100;
    }
}

/// Starts a crypt run of `site` for `party` (no enemies on the floor unless the scene adds
/// them, so the art reads).
fn enter(world: &mut WorldState, party: &[PlayerId], site: &TombSite) {
    for player in party {
        kit(world, *player);
        let _ = world.place_pilot(*player, 0.0, 0.0);
    }
    let _ = world.scenario_enter_tomb(site, party);
    world.stream_terrain_near_players();
}

/// Clears the floor's creatures (scenes add their own).
fn clear_creatures(world: &mut WorldState) {
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Peaceful);
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
}

/// Reveals the whole floor of `player`'s run.
fn reveal_floor(world: &mut WorldState, player: PlayerId) {
    let Some(run) = world.tomb_run_of(player).cloned() else {
        return;
    };
    for room in &run.rooms {
        reveal(
            world,
            player,
            room.interior.min.x - 1..room.interior.max.x + 2,
            room.interior.min.y - 1..room.interior.max.y + 2,
        );
    }
}

/// Reveals the rooms up to (and including) index `upto` of the run's rooms in path order.
fn reveal_rooms(world: &mut WorldState, player: PlayerId, rooms: &[usize]) {
    let Some(run) = world.tomb_run_of(player).cloned() else {
        return;
    };
    for index in rooms {
        if let Some(room) = run.rooms.get(*index) {
            reveal(
                world,
                player,
                room.interior.min.x - 1..room.interior.max.x + 2,
                room.interior.min.y - 1..room.interior.max.y + 2,
            );
        }
    }
}

/// The index of the first room of `kind`.
fn room_of(world: &WorldState, player: PlayerId, kind: RoomKind) -> Option<usize> {
    world
        .tomb_run_of(player)
        .and_then(|run| run.rooms.iter().position(|room| room.kind == kind))
}

/// Puts `player`'s pilot at tile offset `(dx, dy)` from room `room`'s top-left interior tile.
fn stand(world: &mut WorldState, player: PlayerId, room: usize, (dx, dy): (i32, i32)) {
    let Some(run) = world.tomb_run_of(player).cloned() else {
        return;
    };
    let Some(room) = run.rooms.get(room) else {
        return;
    };
    let x = (room.interior.min.x + dx) as f32 * TILE_SIZE + 16.0;
    let feet = (room.interior.min.y + dy + 1) as f32 * TILE_SIZE - 4.0;
    if let Some((id, _, _)) = world.pilot_of(player) {
        world.edit_pilot(id, |body, state| {
            body.x = x;
            body.y = feet;
            body.previous_x = x;
            body.previous_y = feet;
            state.heading_x = 0;
            state.heading_y = 127;
        });
    }
}

/// The pilot aims at `(x, y)` (world pixels): the arm and gun follow.
fn aim_at(world: &mut WorldState, player: PlayerId, (tx, ty): (f32, f32)) {
    if let Some((id, body, state)) = world.pilot_of(player) {
        let (mx, my) = state.middle(body);
        let (dx, dy) = (tx - mx, ty - my);
        let length = dx.hypot(dy).max(1.0);
        world.edit_pilot(id, |_, state| {
            state.aim_x = (dx / length * 127.0) as i8;
            state.aim_y = (dy / length * 127.0) as i8;
            state.heading_x = state.aim_x;
            state.heading_y = state.aim_y;
        });
    }
}

/// A room's interior tile `(dx, dy)` from its top-left, as world pixels (the tile's centre).
fn room_point(
    world: &WorldState,
    player: PlayerId,
    room: usize,
    (dx, dy): (i32, i32),
) -> (f32, f32) {
    world
        .tomb_run_of(player)
        .and_then(|run| run.rooms.get(room).copied())
        .map_or((0.0, 0.0), |room| {
            (
                (room.interior.min.x + dx) as f32 * TILE_SIZE + 16.0,
                (room.interior.min.y + dy) as f32 * TILE_SIZE + 16.0,
            )
        })
}

/// A tile-set showcase: the run's ambush room with its pillars, the floor revealed.
fn tileset(stage: &mut SceneStage<'_>, setting: SiteSetting) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        if let Some(site) = crypt_site(world, setting) {
            enter(world, &[player], &site);
            clear_creatures(world);
            reveal_floor(world, player);
            let room = room_of(world, player, RoomKind::Shrine).unwrap_or(0);
            stand(world, player, room, (3, 6));
            world.scenario_edit_tomb_member(player, |member| {
                member.relics = vec![Relic::Quickstep];
                member.unbanked = 240;
            });
        }
    });
    stage.settle(6);
}

pub const CRYPT_SCENES: &[Scene] = &[
    Scene {
        name: "crypt_tileset_catacomb",
        summary: "A catacomb crypt (deep strata) from above: bone-niched walls in 3/4 view, flagstones, the shrine, the room map",
        setup: |stage| tileset(stage, SiteSetting::Deep),
    },
    Scene {
        name: "crypt_tileset_sandstone",
        summary: "A sandstone crypt under a mesa from above",
        setup: |stage| tileset(stage, SiteSetting::Mesa),
    },
    Scene {
        name: "crypt_tileset_sky_vault",
        summary: "A sky vault crypt on a sky continent: pale marble and gold trim",
        setup: |stage| tileset(stage, SiteSetting::SkyContinent),
    },
    Scene {
        name: "crypt_tileset_lunar_vault",
        summary: "A lunar vault crypt on the Moon: black stone and cold light",
        setup: |stage| tileset(stage, SiteSetting::Moon),
    },
    Scene {
        name: "crypt_skeleton_pack",
        summary: "Combat with a skeleton pack in an ambush room: four skeletons flanking, one rattling its blade before a lunge, the pilot firing",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = crypt_site(world, SiteSetting::Deep) {
                    enter(world, &[player], &site);
                    clear_creatures(world);
                    let room = room_of(world, player, RoomKind::Ambush).unwrap_or(0);
                    reveal_rooms(world, player, &[room]);
                    stand(world, player, room, (5, 4));
                    for (dx, dy) in [(1, 1), (9, 1), (2, 7), (9, 6)] {
                        let (x, y) = room_point(world, player, room, (dx, dy));
                        let _ = world.spawn_enemy(EnemyKind::CryptSkeleton, x, y);
                    }
                    let target = room_point(world, player, room, (9, 1));
                    aim_at(world, player, target);
                }
            });
            stage.settle(24);
            stage.play_on(|world| {
                // One skeleton winding up its lunge.
                let ids: Vec<_> = world
                    .enemies()
                    .filter(|(_, state)| state.kind == EnemyKind::CryptSkeleton)
                    .map(|(entity, _)| entity.id)
                    .collect();
                if let Some(id) = ids.first() {
                    let until = fatherload_simulation::SimulationTick::new(
                        world.simulation_tick().get() + 30,
                    );
                    world.entities_mut().update(*id, |entity| {
                        if let EntityState::Enemy(state) = &mut entity.state {
                            state.mode = EnemyMode::Windup;
                            state.mode_until = until;
                        }
                    });
                }
            });
        },
    },
    Scene {
        name: "crypt_spectres",
        summary: "Spectres drifting through crypt walls toward the pilot: faint inside the masonry, solid in the room",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = crypt_site(world, SiteSetting::Mesa) {
                    enter(world, &[player], &site);
                    clear_creatures(world);
                    let room = room_of(world, player, RoomKind::Entry).unwrap_or(0);
                    reveal_floor(world, player);
                    stand(world, player, room, (5, 4));
                    // One in the north wall, one in the west wall, one in the room.
                    for (dx, dy) in [(4, -1), (-1, 5), (8, 6)] {
                        let (x, y) = room_point(world, player, room, (dx, dy));
                        let spectre = world.spawn_enemy(EnemyKind::Spectre, x, y);
                        world.entities_mut().update(spectre, |entity| {
                            entity.body.x = x;
                            entity.body.y = y;
                        });
                    }
                }
            });
            stage.settle(4);
        },
    },
    Scene {
        name: "crypt_traps",
        summary: "Crypt traps: a rippling spike field (warning and up), flame jets taking turns, arrow plates with their wall slots, a boulder rolling from its chute, cracked floor and a pit",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = crypt_site(world, SiteSetting::Deep) {
                    enter(world, &[player], &site);
                    clear_creatures(world);
                    let room = room_of(world, player, RoomKind::Shrine).unwrap_or(0);
                    reveal_floor(world, player);
                    let Some(run) = world.tomb_run_of(player).cloned() else {
                        return;
                    };
                    let interior = run.rooms[room].interior;
                    let (x0, y0) = (interior.min.x, interior.min.y);
                    let at = |dx: i32, dy: i32| TilePosition {
                        x: x0 + dx,
                        y: y0 + dy,
                    };
                    let trap = |at: TilePosition, kind, dir: (i8, i8), phase: u16| CryptTrap {
                        at,
                        kind,
                        dir,
                        phase,
                        hidden: false,
                        room: u8::try_from(room).unwrap_or(0),
                    };
                    let mut traps = Vec::new();
                    // Phases picked for the moment of the shot (a few ticks into the floor):
                    // the first spike column up, the next rattling; the west vents burning, the
                    // east ones glowing; the chute just let its boulder go.
                    for dy in 0..3 {
                        for (dx, phase) in [(0, 1_700), (1, 1_300), (2, 900), (3, 1_750)] {
                            traps.push(trap(at(dx, dy), CryptTrapKind::Spikes, (0, 0), phase));
                        }
                    }
                    for dy in 5..9 {
                        traps.push(trap(at(1, dy), CryptTrapKind::FlameJet, (0, 0), 1_950));
                        traps.push(trap(at(3, dy), CryptTrapKind::FlameJet, (0, 0), 1_550));
                    }
                    for dy in [1, 3] {
                        traps.push(trap(at(7, dy), CryptTrapKind::ArrowPlate, (1, 0), 0));
                    }
                    traps.push(trap(at(10, 0), CryptTrapKind::BoulderChute, (0, 1), 2_900));
                    for (dx, dy) in [(7, 6), (8, 6), (9, 7), (7, 8), (9, 8)] {
                        traps.push(trap(at(dx, dy), CryptTrapKind::CrackedFloor, (0, 0), 0));
                    }
                    for (dx, dy) in [(8, 7), (8, 8)] {
                        traps.push(trap(at(dx, dy), CryptTrapKind::Pit, (0, 0), 0));
                    }
                    // Spikes up and the first flame line burning when the shot is taken.
                    world.scenario_crypt_traps(player, traps, 0);
                    stand(world, player, room, (5, 6));
                    let target = room_point(world, player, room, (10, 4));
                    aim_at(world, player, target);
                }
            });
            stage.settle(9);
        },
    },
    Scene {
        name: "crypt_dodge_roll",
        summary: "A dodge roll through a hex bolt: the tumbling pilot and its trail",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = crypt_site(world, SiteSetting::Mesa) {
                    enter(world, &[player], &site);
                    clear_creatures(world);
                    let room = room_of(world, player, RoomKind::Entry).unwrap_or(0);
                    reveal_floor(world, player);
                    stand(world, player, room, (2, 4));
                    let (x, y) = room_point(world, player, room, (9, 4));
                    let _ = world.spawn_enemy(EnemyKind::Cultist, x, y);
                }
            });
            let roll = crate::input::PlayerInput {
                horizontal: 1.0,
                thrust: true,
                jump: true,
                ..crate::input::PlayerInput::default()
            };
            stage.hold(6, roll);
        },
    },
    Scene {
        name: "crypt_lord",
        summary: "The crypt lord's hall on the last floor: the lord in its second phase winding up a hex ring, summoned skeletons, the boss bar under the run banner",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = crypt_site(world, SiteSetting::Deep) {
                    enter(world, &[player], &site);
                    world.scenario_tomb_floor(player, site.floors() - 1);
                    clear_creatures(world);
                    let room = room_of(world, player, RoomKind::Boss).unwrap_or(0);
                    reveal_floor(world, player);
                    stand(world, player, room, (8, 14));
                }
            });
            stage.settle(20);
            stage.play_on(|world| {
                let Some(player) = world.player_ids().next() else {
                    return;
                };
                if let Some(room) = room_of(world, player, RoomKind::Boss) {
                    for (dx, dy) in [(8, 13), (15, 12)] {
                        let (x, y) = room_point(world, player, room, (dx, dy));
                        let _ = world.spawn_enemy(EnemyKind::CryptSkeleton, x, y);
                    }
                    let lord = room_point(world, player, room, (13, 10));
                    let ids: Vec<_> = world
                        .enemies()
                        .filter(|(_, state)| state.kind == EnemyKind::CryptLord)
                        .map(|(entity, _)| entity.id)
                        .collect();
                    for id in ids {
                        world.entities_mut().update(id, |entity| {
                            entity.body.x = lord.0;
                            entity.body.y = lord.1;
                        });
                    }
                }
            });
            stage.play_on(|world| {
                let ids: Vec<_> = world
                    .enemies()
                    .filter(|(_, state)| state.kind == EnemyKind::CryptLord)
                    .map(|(entity, _)| entity.id)
                    .collect();
                let until =
                    fatherload_simulation::SimulationTick::new(world.simulation_tick().get() + 40);
                for id in ids {
                    world.entities_mut().update(id, |entity| {
                        if let EntityState::Enemy(state) = &mut entity.state {
                            state.health = state.max_health * 0.55;
                            state.phase = 2;
                            state.pattern = 2;
                            state.mode = EnemyMode::Windup;
                            state.mode_until = until;
                        }
                    });
                }
            });
            stage.settle(12);
        },
    },
    Scene {
        name: "crypt_minimap",
        summary: "The crypt's room map: the rooms walked so far coloured by kind, doorways, the party, the room the pilot is in",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = crypt_site(world, SiteSetting::Deep) {
                    enter(world, &[player], &site);
                    clear_creatures(world);
                    let rooms: Vec<usize> = (0..6).collect();
                    reveal_rooms(world, player, &rooms);
                    let shrine = room_of(world, player, RoomKind::Shrine).unwrap_or(0);
                    reveal_rooms(world, player, &[shrine]);
                    stand(world, player, shrine, (3, 3));
                }
            });
            stage.settle(4);
        },
    },
    Scene {
        name: "crypt_split_screen",
        summary: "Split screen: seat one crawls a crypt from above while seat two flies its rig in the side view outside",
        setup: |stage| {
            stage.play(2, |world, seats| {
                let (a, b) = (seats[0].player, seats[1].player);
                if let Some(site) = crypt_site(world, SiteSetting::Deep) {
                    enter(world, &[a], &site);
                    let room = room_of(world, a, RoomKind::Ambush).unwrap_or(0);
                    reveal_rooms(world, a, &[0, room]);
                    stand(world, a, room, (5, 4));
                    if let Some(rig) = world.player_mut(b) {
                        rig.x = 40.0 * TILE_SIZE;
                        rig.y = 2.0 * TILE_SIZE;
                    }
                }
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "crypt_touch",
        summary: "A crypt on a phone: the left stick walks eight ways, the right thumb aims and fires, the ROLL button, USE at a lever, the room map top left",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = crypt_site(world, SiteSetting::Mesa) {
                    enter(world, &[player], &site);
                    clear_creatures(world);
                    reveal_floor(world, player);
                    let room = room_of(world, player, RoomKind::Puzzle).unwrap_or(0);
                    // Beside the lever.
                    let lever = world.tomb_run_of(player).and_then(|run| {
                        let room = run.rooms.get(room)?;
                        (room.interior.min.y..=room.interior.max.y)
                            .flat_map(|y| {
                                (room.interior.min.x..=room.interior.max.x)
                                    .map(move |x| TilePosition { x, y })
                            })
                            .find(|tile| {
                                matches!(
                                    world.terrain().tile(*tile).map(|found| found.kind),
                                    Some(fatherload_simulation::terrain::TileKind::Built(
                                        fatherload_simulation::base::BuiltTile::Lever { .. }
                                    ))
                                )
                            })
                            .map(|tile| {
                                (tile.x - room.interior.min.x, tile.y - room.interior.min.y)
                            })
                    });
                    let (dx, dy) =
                        lever.map_or((5, 4), |(dx, dy)| (dx + if dx > 5 { -1 } else { 1 }, dy));
                    stand(world, player, room, (dx, dy));
                }
            });
            stage.settle(4);
            stage.use_devices(&[InputDevice::Touch]);
            stage.touch_view = TouchView {
                stick: Some(((150.0, 280.0), (180.0, 260.0))),
                held: vec![TouchControl::Fire],
                aim: Some((TouchControl::Fire, (0.7, -0.4))),
                ..TouchView::default()
            };
        },
    },
];
