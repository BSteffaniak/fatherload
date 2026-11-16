//! Moon scenes (WP-MOON, V9 worlds): the filled Moon from below, the landing at sunrise on the
//! colony's pad, a crater field and a lava tube, the colony and its counter, the crystal
//! forest, the alien facility's puzzle hall and the Warden in its arena, the core, and a lunar
//! uplink.

use fatherload_simulation::{
    EnemyKind, PlayerCommand, PlayerId, TickInputs, WorldState,
    base::{BuildCommand, Buildable, Facing, ModuleKind},
    pilot::PilotGear,
    terrain::{StrategicResourceKind, TILE_SIZE, TileKind, TilePosition},
    worldgen::moon::MoonLayout,
};

use super::{Scene, SceneStage, reveal};
use crate::{input::PlayerInput, shell::ModalScreen};

/// Runs one real tick of `command` from `player`.
fn act(world: &mut WorldState, player: PlayerId, command: PlayerCommand) {
    let command_copy = command.clone();
    let inputs = TickInputs {
        players: std::iter::once((
            player,
            fatherload_simulation::input::PlayerTickInput {
                controls: None,
                actions: vec![fatherload_simulation::TickAction {
                    sequence: fatherload_simulation::InputSequence::new(0),
                    command,
                }],
            },
        ))
        .collect(),
    };
    if let PlayerCommand::Build(build) = &command_copy
        && let Err(refusal) = world.check_build(player, build)
    {
        eprintln!("moon scene build refused: {build:?}: {}", refusal.message());
    }
    let tick = world.simulation_tick();
    let _ = world.simulate_tick(tick, &inputs);
}

/// The world's filled Moon.
fn layout(world: &WorldState) -> Option<MoonLayout> {
    world.moon_layout()
}

/// Fits the seat's rig for the Moon: every line it needs, plus a top tank and engine.
fn moon_rig(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 15;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(15);
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.engine_level = 15;
        rig.hazard_gear.altitude_intake = 6;
        rig.hazard_gear.de_icer = 6;
        rig.hazard_gear.pressurized_hull = fatherload_simulation::moon::VACUUM_SEAL_TIER;
        rig.hazard_gear.life_support = 2;
        // Been here before (the landing scene clears it for the set piece).
        rig.lunar.landed = true;
        rig.pilot_gear = PilotGear {
            jetpack: 2,
            suit: 2,
        };
    }
}

#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn px(tile: i32) -> f32 {
    (tile as f32 + 0.5) * TILE_SIZE
}

/// The rig resting on the floor whose top row is `floor`, at column `x`, its surroundings
/// revealed and streamed.
#[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
fn rest_at(world: &mut WorldState, player: PlayerId, x: i32, floor: i32) {
    if let Some(rig) = world.player_mut(player) {
        rig.x = px(x);
        rig.y = (floor as f32).mul_add(TILE_SIZE, -11.0);
        rig.x_remainder = 0.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
    reveal(world, player, x - 60..x + 60, floor - 40..floor + 30);
    world.stream_terrain_near_players();
}

/// The pilot out at `(x, feet row)`.
#[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
fn pilot_at(world: &mut WorldState, player: PlayerId, x: i32, floor: i32) {
    let _ = world.place_pilot(player, px(x), (floor as f32).mul_add(TILE_SIZE, -0.25));
    reveal(world, player, x - 60..x + 60, floor - 40..floor + 30);
}

/// Morning on the Moon: the sun low over the rim.
const fn morning(world: &mut WorldState) {
    let length = world.day_length().millis();
    world.set_world_time_millis(length + length / 4);
}

fn hang(stage: &mut SceneStage<'_>, frames: usize) {
    for frame in 0..frames {
        stage.hold(
            1,
            PlayerInput {
                thrust: frame % 4 != 3,
                ..PlayerInput::default()
            },
        );
    }
}

pub const MOON_SCENES: &[Scene] = &[
    Scene {
        name: "moon_from_below",
        summary: "Under the filled Moon: its deep-rock underside overhead, crystal stalks and glowing blooms hanging into orbit",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                moon_rig(world, player);
                let Some(layout) = layout(world) else {
                    return;
                };
                let moon = layout.moon;
                let x = moon.center_x + 140;
                let bottom = moon.raw_bottom(x, moon.top_row);
                if let Some(rig) = world.player_mut(player) {
                    rig.x = px(x);
                    rig.y = px(bottom + 7);
                    rig.velocity_x = 0.0;
                    rig.velocity_y = 0.0;
                }
                reveal(world, player, x - 60..x + 60, bottom - 30..bottom + 30);
                world.stream_terrain_near_players();
                morning(world);
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "moon_landing_sunrise",
        summary: "The landing: touching down on Tranquility Station's pad as the sun breaks over the Moon's rim (the set piece and its banner)",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                moon_rig(world, player);
                let Some(layout) = layout(world) else {
                    return;
                };
                let (first, last, row) = layout.pad();
                rest_at(world, player, i32::midpoint(first, last), row);
                if let Some(rig) = world.player_mut(player) {
                    rig.y -= 6.0;
                    rig.lunar.landed = false;
                }
                morning(world);
            });
            stage.settle(90);
        },
    },
    Scene {
        name: "moon_craters_and_tube",
        summary: "A lava tube under the regolith: a basalt-lined tunnel lit by the rig, a skylight open to space, craters above",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                moon_rig(world, player);
                let Some(layout) = layout(world) else {
                    return;
                };
                let tube = layout
                    .tubes
                    .iter()
                    .find(|tube| tube.placed())
                    .copied()
                    .unwrap_or(layout.tubes[0]);
                // Hovering in the skylight, halfway down: the cratered surface above, the tube
                // opening below.
                let x = tube.first + 1;
                let surface = layout.surface_row(x).unwrap_or(layout.moon.top_row);
                let middle = surface + 5;
                if let Some(rig) = world.player_mut(player) {
                    rig.x = px(x);
                    rig.y = px(middle + 2);
                    rig.velocity_x = 0.0;
                    rig.velocity_y = 0.0;
                }
                reveal(
                    world,
                    player,
                    x - 70..x + 70,
                    surface - 30..tube.center_row(x) + 20,
                );
                world.stream_terrain_near_players();
                morning(world);
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "moon_colony",
        summary: "Tranquility Station: the landing pad, the glass dome, the hab module and the comms mast, the pilot walking into the dome",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                moon_rig(world, player);
                let Some(layout) = layout(world) else {
                    return;
                };
                let (first, last, row) = layout.pad();
                rest_at(world, player, i32::midpoint(first, last), row);
                if let Some(rig) = world.player_mut(player) {
                    rig.lunar.landed = true;
                }
                pilot_at(
                    world,
                    player,
                    layout.colony.center_x - 6,
                    layout.colony.ground_row,
                );
                morning(world);
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "moon_colony_counter",
        summary: "The colony's counter: fuel, repairs, melting moon ice, selling helium-3, the reactor, alloy drill teeth and the Moon weapons",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                moon_rig(world, player);
                let Some(layout) = layout(world) else {
                    return;
                };
                let (first, last, row) = layout.pad();
                rest_at(world, player, i32::midpoint(first, last), row);
                if let Some(rig) = world.player_mut(player) {
                    rig.lunar.landed = true;
                    rig.credits = 31_500;
                    rig.cargo_bay.add(StrategicResourceKind::Helium3, 12);
                    rig.cargo_bay.add(StrategicResourceKind::MoonIce, 4);
                    rig.cargo_bay.add(StrategicResourceKind::AlienAlloy, 7);
                }
            });
            stage.settle(4);
            let seat = stage.seat(0);
            seat.modal = Some(ModalScreen::Colony);
            seat.selected_menu_item = 4;
            stage.settle(6);
        },
    },
    Scene {
        name: "moon_crystal_forest",
        summary: "An alien crystal forest on the Moon's top: violet trees with glowing blooms, crystal mites swarming",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                moon_rig(world, player);
                let Some(layout) = layout(world) else {
                    return;
                };
                let forest = layout
                    .forests
                    .iter()
                    .find(|forest| forest.last > forest.first)
                    .copied()
                    .unwrap_or(layout.forests[0]);
                let x = i32::midpoint(forest.first, forest.last);
                let ground = layout.surface_row(x).unwrap_or(layout.moon.top_row);
                if let Some(rig) = world.player_mut(player) {
                    rig.x = px(x);
                    rig.y = px(ground - 6);
                    rig.velocity_y = 0.0;
                }
                reveal(world, player, x - 60..x + 60, ground - 30..ground + 20);
                world.stream_terrain_near_players();
                for dx in [6, 7, 9] {
                    let _ = world.spawn_enemy(EnemyKind::CrystalMite, px(x + dx), px(ground - 5));
                }
                morning(world);
            });
            hang(stage, 10);
        },
    },
    Scene {
        name: "moon_facility_puzzle",
        summary: "The alien facility's puzzle hall on foot: three pylons marked with dots, one lit, the sealed gate ahead",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                moon_rig(world, player);
                let Some(layout) = layout(world) else {
                    return;
                };
                let plan = layout.facility;
                let floor = plan.hall.max.y + 1;
                // The rig parked by the temple; the pilot inside, beside the first pylon.
                rest_at(
                    world,
                    player,
                    plan.temple.center_x - plan.direction * 16,
                    plan.temple.ground_row,
                );
                let first = plan.order.iter().position(|place| *place == 0).unwrap_or(0);
                let _ = world.strike_facility(player, plan.pylons[first]);
                let x = plan.pylons[1].x - plan.direction * 3;
                pilot_at(world, player, x, floor);
            });
            stage.settle(16);
        },
    },
    Scene {
        name: "moon_warden",
        summary: "The Warden's arena: the boss guardian awake and shielded, its power conduits glowing, the pilot facing it",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                moon_rig(world, player);
                let Some(layout) = layout(world) else {
                    return;
                };
                let plan = layout.facility;
                rest_at(
                    world,
                    player,
                    plan.temple.center_x - plan.direction * 16,
                    plan.temple.ground_row,
                );
                let floor = plan.arena.max.y + 1;
                let x = plan.arena.min.x + 6;
                pilot_at(world, player, x, floor);
            });
            stage.settle(40);
        },
    },
    Scene {
        name: "moon_core",
        summary: "The heart of the Moon: the strange core floating in its chamber inside the iridescent shell, the pilot beside it",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                moon_rig(world, player);
                let Some(layout) = layout(world) else {
                    return;
                };
                let plan = layout.facility;
                rest_at(
                    world,
                    player,
                    plan.temple.center_x - plan.direction * 16,
                    plan.temple.ground_row,
                );
                let core = layout.core;
                let floor = (core.y..core.y + 14)
                    .find(|row| {
                        world.terrain().is_solid_at(TilePosition {
                            x: core.x - 6,
                            y: *row,
                        })
                    })
                    .unwrap_or(core.y + 10);
                pilot_at(world, player, core.x - 6, floor);
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "moon_uplink",
        summary: "A lunar uplink on a Moon claim: the ring gate humming between its pylons, ready to beam the rig home",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                moon_rig(world, player);
                let Some(layout) = layout(world) else {
                    return;
                };
                // Level ground beside the colony, away from the centre.
                let side = (layout.colony.center_x - layout.moon.center_x).signum();
                let x = layout.colony.center_x
                    + side * (fatherload_simulation::worldgen::moon::COLONY_HALF_WIDTH + 40);
                let ground = layout.surface_row(x).unwrap_or(layout.moon.top_row);
                for dx in -12..=12 {
                    for dy in 1..=8 {
                        let _ = world.terrain_mut().set_kind(
                            TilePosition {
                                x: x + dx,
                                y: ground - dy,
                            },
                            TileKind::Air,
                        );
                    }
                    let _ = world.terrain_mut().set_kind(
                        TilePosition {
                            x: x + dx,
                            y: ground,
                        },
                        TileKind::Lunar(fatherload_simulation::moon::LunarTile::Regolith),
                    );
                }
                rest_at(world, player, x, ground);
                if let Some(rig) = world.player_mut(player) {
                    rig.lunar.landed = true;
                    rig.credits = 100_000;
                }
                for (item, dx) in [
                    (ModuleKind::Beacon, -1),
                    (ModuleKind::Generator, -4),
                    (ModuleKind::LunarUplink, 3),
                ] {
                    let command = BuildCommand::Place {
                        item: Buildable::Module(item),
                        at: TilePosition {
                            x: x + dx,
                            y: ground - 1,
                        },
                        facing: Facing::Right,
                    };
                    act(world, player, PlayerCommand::Build(command));
                }
                // Parked on the pad, ready to beam.
                rest_at(world, player, x + 4, ground);
                morning(world);
            });
            stage.settle(30);
        },
    },
];
