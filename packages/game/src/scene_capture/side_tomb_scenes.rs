//! Side-view tomb scenes (WP-TOMBS T28, V10 worlds): each culture's tile set (sandstone,
//! catacomb, sky temple, lunar vault), the trap halls (blades, crushers, darts, a collapsing
//! bridge, rising sand, cursed fog), the trap-sight lantern at work, an ambush, both guardians
//! and a shrine.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "scene layouts mix small tile indices and world pixels"
)]

use fatherload_simulation::{
    EnemyKind, PlayerId, WorldState,
    base::BuiltTile,
    pilot::PilotGear,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    tomb::{
        Relic, RoomKind, SiteSetting, SiteStyle, TombSite,
        floor::plan_floor,
        realize::{RealizedFloor, TrapKind, realizer_for},
        traps::{
            CrusherStage, FogStage, SandStage, crusher_state, fog_state, sand_state, world_millis,
        },
    },
    worldgen::{features::TileRect, tomb_sites},
};

use super::{Scene, SceneStage, reveal};

/// The first side-view site in `setting` (the ground, the sky continents, then the Moon).
fn side_site(world: &WorldState, setting: SiteSetting) -> Option<TombSite> {
    let settings = world.terrain().settings();
    let rects = [
        TileRect {
            min: TilePosition { x: -9_000, y: -100 },
            max: TilePosition { x: 9_000, y: 900 },
        },
        TileRect {
            min: TilePosition {
                x: -60_000,
                y: -18_000,
            },
            max: TilePosition {
                x: 60_000,
                y: -8_000,
            },
        },
    ];
    let mut sites: Vec<TombSite> = Vec::new();
    for rect in &rects {
        if matches!(setting, SiteSetting::Moon) {
            break;
        }
        sites.extend(tomb_sites::sites_touching(settings, rect));
        if sites
            .iter()
            .any(|site| site.setting == setting && site.style == SiteStyle::SideView)
        {
            break;
        }
    }
    if setting == SiteSetting::Moon
        && let Some(moon) = fatherload_simulation::worldgen::sky::moon(settings)
    {
        sites.extend(tomb_sites::sites_touching(settings, &moon.bounds()));
    }
    sites
        .into_iter()
        .find(|site| site.setting == setting && site.style == SiteStyle::SideView)
}

/// The first run counter whose floor `floor` satisfies `wanted`.
fn run_with(site: &TombSite, floor: u8, wanted: impl Fn(&RealizedFloor) -> bool) -> u32 {
    (0..400)
        .find(|run| {
            wanted(&realizer_for(SiteStyle::SideView).realize(&plan_floor(site, *run, floor)))
        })
        .unwrap_or(0)
}

fn has_trap(floor: &RealizedFloor, kind: TrapKind) -> bool {
    floor.cells.iter().any(|(_, cell)| {
        matches!(cell, TileKind::Built(BuiltTile::TombTrap { kind: found, .. }) if *found == kind.index())
    })
}

/// A well-kitted pilot (jetpack, suit, rifle).
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
        rig.credits = 2_600;
    }
}

/// Starts run `run` of `site` for `party`.
fn enter(world: &mut WorldState, party: &[PlayerId], site: &TombSite, run: u32) {
    for player in party {
        kit(world, *player);
    }
    let (x, feet) = WorldState::outside_spot(site, 0);
    let _ = world.place_pilot(party[0], x, feet);
    world.scenario_next_tomb_run(site.id, run);
    let _ = world.scenario_enter_tomb(site, party);
    world.stream_terrain_near_players();
}

/// The pilot's feet on tile `tile` (absolute).
fn stand(world: &mut WorldState, player: PlayerId, tile: TilePosition) {
    let x = (tile.x as f32 + 0.5) * TILE_SIZE;
    let feet = ((tile.y + 1) as f32).mul_add(TILE_SIZE, -0.25);
    if let Some((id, _, _)) = world.pilot_of(player) {
        world.edit_pilot(id, |body, _| {
            body.x = x;
            body.y = feet;
            body.previous_x = x;
            body.previous_y = feet;
            body.velocity_x = 0.0;
            body.velocity_y = 0.0;
        });
    }
}

/// The absolute tile of the first trap of `kind` on the run's floor.
fn trap_tile(world: &WorldState, player: PlayerId, kind: TrapKind) -> Option<TilePosition> {
    let run = world.tomb_run_of(player)?;
    let origin = tomb_sites::slot_origin(run.slot);
    let floor = WorldState::tomb_floor_of(run);
    floor.cells.iter().find_map(|(tile, cell)| {
        matches!(cell, TileKind::Built(BuiltTile::TombTrap { kind: found, .. }) if *found == kind.index())
            .then_some(TilePosition {
                x: tile.x + origin.x,
                y: tile.y + origin.y,
            })
    })
}

/// The room (absolute interior) holding `tile`.
fn room_at(world: &WorldState, player: PlayerId, tile: TilePosition) -> Option<TileRect> {
    world
        .tomb_run_of(player)?
        .rooms
        .iter()
        .find(|room| room.interior.expanded(1).contains(tile))
        .map(|room| room.interior)
}

/// The first room of `kind` (absolute interior).
fn room_of(world: &WorldState, player: PlayerId, kind: RoomKind) -> Option<TileRect> {
    world
        .tomb_run_of(player)?
        .rooms
        .iter()
        .find(|room| room.kind == kind)
        .map(|room| room.interior)
}

/// The row the run's doorways stand on (absolute: the open row above the floor).
fn doorway_row(world: &WorldState, player: PlayerId) -> i32 {
    world.tomb_run_of(player).map_or(0, |run| {
        tomb_sites::slot_origin(run.slot).y + fatherload_simulation::tomb::side_view::FLOOR_ROW - 1
    })
}

/// Reveals the whole slot of the player's run.
fn reveal_run(world: &mut WorldState, player: PlayerId) {
    let Some(run) = world.tomb_run_of(player) else {
        return;
    };
    let origin = tomb_sites::slot_origin(run.slot);
    reveal(
        world,
        player,
        origin.x..origin.x + tomb_sites::SLOT_SIZE.0,
        origin.y..origin.y + tomb_sites::SLOT_SIZE.1,
    );
}

/// The player in the scene's run (the first seat's).
fn runner(world: &WorldState) -> PlayerId {
    world
        .tombs()
        .runs
        .values()
        .next()
        .and_then(|run| run.members.first())
        .map_or(PlayerId::new(0), |member| member.player)
}

/// Ticks until `ready` holds for the world (at most `limit` frames).
fn wait_until(stage: &mut SceneStage<'_>, limit: usize, ready: impl Fn(&WorldState) -> bool) {
    for _ in 0..limit {
        if ready(stage.world()) {
            return;
        }
        stage.settle(1);
    }
}

const fn millis(world: &WorldState) -> u64 {
    world_millis(world.simulation_tick().get(), world.tick_rate_hz())
}

/// A culture scene: a run of the setting's first side-view site on a floor with a trap hall of
/// `kind`, the pilot standing in that hall.
fn culture_scene(
    stage: &mut SceneStage<'_>,
    setting: SiteSetting,
    kind: TrapKind,
    relics: Vec<Relic>,
) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(site) = side_site(world, setting) else {
            return;
        };
        let run = run_with(&site, 0, |floor| has_trap(floor, kind));
        enter(world, &[player], &site, run);
        world.scenario_edit_tomb_member(player, |member| {
            member.relics = relics;
            member.unbanked = 260;
        });
        if let Some(trap) = trap_tile(world, player, kind)
            && let Some(room) = room_at(world, player, trap)
        {
            stand(
                world,
                player,
                TilePosition {
                    x: room.min.x - 1,
                    y: doorway_row(world, player),
                },
            );
        }
        reveal_run(world, player);
    });
    stage.settle(10);
}

/// The pilot at the door of the first side-view site in `setting`, everything around revealed.
fn entrance_scene(stage: &mut SceneStage<'_>, setting: SiteSetting) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(site) = side_site(world, setting) else {
            return;
        };
        kit(world, player);
        let (x, feet) = WorldState::outside_spot(&site, 0);
        if let Some(rig) = world.player_mut(player) {
            let outward = if site.mirrored { 1.0 } else { -1.0 };
            rig.x = outward * TILE_SIZE * 4.0 + x;
            rig.y = feet - 11.5;
            rig.velocity_x = 0.0;
            rig.velocity_y = 0.0;
            rig.x_remainder = 0.0;
            rig.lunar.landed = setting == SiteSetting::Moon;
        }
        let _ = world.place_pilot(player, x, feet);
        reveal(
            world,
            player,
            site.door.x - 50..site.door.x + 50,
            site.door.y - 30..site.door.y + 20,
        );
        world.stream_terrain_near_players();
    });
    stage.settle(8);
}

pub const SIDE_TOMB_SCENES: &[Scene] = &[
    Scene {
        name: "side_tomb_entrance_lunar",
        summary: "A side-view tomb's entrance on the Moon, painted as a lunar vault (alloy panels, teal seams)",
        setup: |stage| entrance_scene(stage, SiteSetting::Moon),
    },
    Scene {
        name: "side_tomb_entrance_catacomb",
        summary: "A side-view tomb's entrance in the deep strata, painted as a catacomb",
        setup: |stage| entrance_scene(stage, SiteSetting::Deep),
    },
    Scene {
        name: "side_tomb_sandstone",
        summary: "A sandstone tomb (mesa): a blade gallery in sun-baked ashlars, glyph bands and sconces on the back wall",
        setup: |stage| {
            culture_scene(
                stage,
                SiteSetting::Mesa,
                TrapKind::Blade,
                vec![Relic::SpringHeels],
            );
            wait_until(stage, 200, |world| {
                world.tomb_run_of(runner(world)).is_some()
                    && (millis(world) % fatherload_simulation::tomb::traps::BLADE_PERIOD_MILLIS)
                        < 120
            });
        },
    },
    Scene {
        name: "side_tomb_catacomb",
        summary: "A catacomb (deep strata): grey bone-and-mortar bricks, skull niches and a cursed corridor",
        setup: |stage| {
            culture_scene(
                stage,
                SiteSetting::Deep,
                TrapKind::FogVent,
                vec![Relic::BoneCharm],
            );
            wait_until(stage, 300, |world| {
                trap_tile(world, runner(world), TrapKind::FogVent)
                    .is_some_and(|tile| fog_state(tile, millis(world)).0 == FogStage::Burst)
            });
        },
    },
    Scene {
        name: "side_tomb_sky_temple",
        summary: "A sky temple vault (sky continent): pale marble with gold inlay, a crumbling bridge over spikes",
        setup: |stage| {
            culture_scene(
                stage,
                SiteSetting::SkyContinent,
                TrapKind::Crumble,
                vec![Relic::GliderBoost],
            );
        },
    },
    Scene {
        name: "side_tomb_lunar",
        summary: "A lunar vault (the Moon): alloy panels with teal light seams, a crusher run",
        setup: |stage| {
            culture_scene(
                stage,
                SiteSetting::Moon,
                TrapKind::Crusher,
                vec![Relic::Quickstep],
            );
            wait_until(stage, 300, |world| {
                trap_tile(world, runner(world), TrapKind::Crusher)
                    .is_some_and(|tile| crusher_state(tile, millis(world)).0 == CrusherStage::Down)
            });
        },
    },
    Scene {
        name: "side_tomb_crushers_shaking",
        summary: "A crusher shaking and shedding dust before it slams (the tell)",
        setup: |stage| {
            culture_scene(stage, SiteSetting::Mesa, TrapKind::Crusher, vec![]);
            wait_until(stage, 300, |world| {
                trap_tile(world, runner(world), TrapKind::Crusher).is_some_and(|tile| {
                    crusher_state(tile, millis(world)).0 == CrusherStage::Shaking
                })
            });
        },
    },
    Scene {
        name: "side_tomb_darts",
        summary: "Spike pits under a low ceiling, a dart thrower's eye glowing and a dart in flight at jump height",
        setup: |stage| {
            culture_scene(
                stage,
                SiteSetting::OldQuarter,
                TrapKind::DartWall,
                vec![Relic::TrapSightLantern],
            );
            wait_until(stage, 200, |world| {
                trap_tile(world, runner(world), TrapKind::DartWall).is_some_and(|tile| {
                    fatherload_simulation::tomb::traps::dart_state(tile, millis(world))
                        .1
                        .is_some_and(|flown| flown > 100.0 && flown < 220.0)
                })
            });
        },
    },
    Scene {
        name: "side_tomb_sand",
        summary: "A sand basin filling from its sluice grates, the pilot safe on the edge",
        setup: |stage| {
            culture_scene(
                stage,
                SiteSetting::Mesa,
                TrapKind::SandSluice,
                vec![Relic::Sandwalker],
            );
            wait_until(stage, 1_000, |world| {
                let player = runner(world);
                trap_tile(world, player, TrapKind::SandSluice)
                    .and_then(|tile| {
                        room_at(
                            world,
                            player,
                            TilePosition {
                                x: tile.x,
                                y: tile.y - 1,
                            },
                        )
                    })
                    .is_some_and(|room| {
                        let (stage, level) = sand_state(room, millis(world));
                        stage == SandStage::Rising && level > 1.2
                    })
            });
        },
    },
    Scene {
        name: "side_tomb_water",
        summary: "A lunar vault's flood basin: the same sluice trap pouring water",
        setup: |stage| {
            culture_scene(stage, SiteSetting::Moon, TrapKind::SandSluice, vec![]);
            wait_until(stage, 1_000, |world| {
                let player = runner(world);
                trap_tile(world, player, TrapKind::SandSluice)
                    .and_then(|tile| {
                        room_at(
                            world,
                            player,
                            TilePosition {
                                x: tile.x,
                                y: tile.y - 1,
                            },
                        )
                    })
                    .is_some_and(|room| sand_state(room, millis(world)).0 == SandStage::Full)
            });
        },
    },
    Scene {
        name: "side_tomb_trap_sight",
        summary: "The Trap-Sight Lantern: hidden spikes and a false-wall secret glowing, each trap's reach outlined",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let Some(site) = side_site(world, SiteSetting::Mesa) else {
                    return;
                };
                let run = run_with(&site, 0, |floor| {
                    floor.cells.iter().any(|(_, cell)| {
                        matches!(cell, TileKind::Built(BuiltTile::TombTrap { kind, hidden: true })
                            if *kind == TrapKind::FalseWall.index() || *kind == TrapKind::Spikes.index())
                    }) && has_trap(floor, TrapKind::DartWall)
                });
                enter(world, &[player], &site, run);
                world.scenario_edit_tomb_member(player, |member| {
                    member.relics = vec![Relic::TrapSightLantern, Relic::Sandwalker];
                });
                // Beside the first hidden trap (a pit's spikes or a false wall).
                let hidden = world.tomb_run_of(player).and_then(|run| {
                    let origin = tomb_sites::slot_origin(run.slot);
                    WorldState::tomb_floor_of(run).cells.iter().find_map(|(tile, cell)| {
                        matches!(
                            cell,
                            TileKind::Built(BuiltTile::TombTrap { kind, hidden: true })
                                if *kind != TrapKind::DartWall.index()
                        )
                        .then_some(TilePosition {
                            x: tile.x + origin.x,
                            y: tile.y + origin.y,
                        })
                    })
                });
                if let Some(trap) = hidden {
                    stand(world, player, TilePosition { x: trap.x - 3, y: doorway_row(world, player) });
                }
                reveal_run(world, player);
            });
            stage.settle(12);
        },
    },
    Scene {
        name: "side_tomb_ambush",
        summary: "An ambush springing: mummies lurching in, crypt bats dropping, a skeletal archer drawing on a ledge",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let Some(site) = side_site(world, SiteSetting::Mesa) else {
                    return;
                };
                enter(world, &[player], &site, 0);
                if let Some(room) = room_of(world, player, RoomKind::Ambush) {
                    stand(
                        world,
                        player,
                        TilePosition {
                            x: room.min.x + 3,
                            y: room.max.y,
                        },
                    );
                    let row = |dy: i32| (room.max.y - dy) as f32 * TILE_SIZE;
                    let column = |dx: i32| (room.min.x + dx) as f32 * TILE_SIZE;
                    let _ = world.spawn_enemy(EnemyKind::Mummy, column(8), row(0));
                    let _ = world.spawn_enemy(EnemyKind::Mummy, column(11), row(0));
                    let _ = world.spawn_enemy(EnemyKind::CryptBat, column(6), row(5));
                    let _ = world.spawn_enemy(EnemyKind::CryptBat, column(9), row(4));
                    let _ = world.spawn_enemy(EnemyKind::SkeletalArcher, column(12), row(3));
                    let _ = world.spawn_enemy(EnemyKind::AnimatedStatue, column(1), row(0));
                }
                world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
                reveal_run(world, player);
            });
            stage.settle(36);
        },
    },
    Scene {
        name: "side_tomb_colossus",
        summary: "The Sunwrought Colossus in its crypt: crouched to leap, rubble flying, mummies raised (phase 2)",
        setup: |stage| boss_scene(stage, EnemyKind::SunColossus),
    },
    Scene {
        name: "side_tomb_matriarch",
        summary: "The Hollow Matriarch over her crypt: hexes flying, crypt bats about her (phase 2)",
        setup: |stage| boss_scene(stage, EnemyKind::HollowMatriarch),
    },
    Scene {
        name: "side_tomb_shrine",
        summary: "A shrine checkpoint between pillars, its lamp lit",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let Some(site) = side_site(world, SiteSetting::Deep) else {
                    return;
                };
                enter(world, &[player], &site, 2);
                world.scenario_edit_tomb_member(player, |member| {
                    member.relics = vec![Relic::SecondWind, Relic::VampiricPick];
                    member.unbanked = 540;
                });
                if let Some(room) = room_of(world, player, RoomKind::Shrine) {
                    stand(
                        world,
                        player,
                        TilePosition {
                            x: room.min.x + 2,
                            y: room.max.y,
                        },
                    );
                }
                reveal_run(world, player);
            });
            stage.settle(4);
            stage.play_on(|world| {
                let player = runner(world);
                let mut inputs = fatherload_simulation::TickInputs::default();
                inputs.act_as(
                    player,
                    fatherload_simulation::PlayerCommand::Pilot(
                        fatherload_simulation::pilot::PilotAction::Use,
                    ),
                );
                let _ = world.simulate_tick(world.simulation_tick(), &inputs);
            });
            stage.settle(6);
        },
    },
];

/// A guardian scene: the last floor of a sandstone tomb, the guardian awake and hurt into its
/// second phase, the pilot at the crypt's edge.
fn boss_scene(stage: &mut SceneStage<'_>, kind: EnemyKind) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(site) = side_site(world, SiteSetting::Mesa) else {
            return;
        };
        enter(world, &[player], &site, 0);
        world.scenario_tomb_floor(player, site.floors() - 1);
        world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
        if let Some(room) = room_of(world, player, RoomKind::Boss) {
            stand(
                world,
                player,
                TilePosition {
                    x: room.min.x + 3,
                    y: room.max.y,
                },
            );
            if let Some(boss) = world.scenario_tomb_guardian(player, kind) {
                world.scenario_hurt_enemy_to(boss, 0.5);
            }
        }
        reveal_run(world, player);
    });
    stage.settle(70);
}
