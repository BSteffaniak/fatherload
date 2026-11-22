//! WP-PILOT scenes: the pilot firing each personal arm (sidearm, rifle, shotgun, the wrench),
//! gliding over the meadow, swinging on a grapple, a lever-door ruin, a crawlway cave, the
//! rig's winch lowering its pilot down a shaft, the rig vault (hop out, pull the lever, fly
//! through), and the town gunsmith's rack.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops,
    reason = "scene layouts mix small tile indices and world pixels"
)]

use fatherload_simulation::{
    EnemyKind, PlayerId, WeaponKind, WorldState,
    economy::SurfaceZone,
    pilot::{PilotGear, PilotMode},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    weapon::AmmoKind,
    worldgen::pilot_spaces::{PilotSpace, PilotSpaceKind},
};

use super::{Scene, SceneStage, WASD, carve, put_seat_inside, reveal, tile_centre};
use crate::{input::PlayerInput, shell::ModalScreen};

/// The hall's floor row (the top of the stone floor) for the arms scenes.
const HALL_FLOOR: i32 = 42;
/// The meadow's ground row (a flat strip west of town).
const GROUND: i32 = 5;

pub const PILOT_CONTENT_SCENES: &[Scene] = &[
    Scene {
        name: "pilot_fires_sidearm",
        summary: "On foot in a cave hall, the pilot's sidearm popping rounds at a burrower",
        setup: |stage| arms_range(stage, WeaponKind::Sidearm, EnemyKind::Burrower, 6),
    },
    Scene {
        name: "pilot_fires_rifle",
        summary: "The pilot's rifle: a long, bright streak across the hall at an acid spitter",
        setup: |stage| arms_range(stage, WeaponKind::Rifle, EnemyKind::AcidSpitter, 10),
    },
    Scene {
        name: "pilot_fires_shotgun",
        summary: "The pilot's shotgun: a close fan of pellets into a cave bat swarm",
        setup: |stage| arms_range(stage, WeaponKind::Shotgun, EnemyKind::CaveBat, 4),
    },
    Scene {
        name: "pilot_swings_wrench",
        summary: "The wrench swung in an arc at an acid spitter right in front of the pilot",
        setup: |stage| arms_range(stage, WeaponKind::Wrench, EnemyKind::AcidSpitter, 1),
    },
    Scene {
        name: "pilot_gliding",
        summary: "Gliding down over the meadow under the canvas wing, wind streaking past",
        setup: gliding,
    },
    Scene {
        name: "pilot_grappling",
        summary: "A grapple hook bitten into a cave roof, the pilot reeling up the rope",
        setup: grappling,
    },
    Scene {
        name: "pilot_lever_ruin",
        summary: "A sealed ruin on foot: the lever thrown, the first gate open, the keycard chest and vault beyond",
        setup: lever_ruin,
    },
    Scene {
        name: "pilot_crawlway_cave",
        summary: "Crawling into a crawlway cave through its vent grates, the rig parked outside the hardpan",
        setup: crawlway_cave,
    },
    Scene {
        name: "pilot_winch",
        summary: "The rig's winch lowering its pilot down a narrow shaft",
        setup: winch,
    },
    Scene {
        name: "pilot_rig_vault",
        summary: "A rig vault: the pilot up the crawlway has thrown the lever and the gate is open for the rig",
        setup: rig_vault,
    },
    Scene {
        name: "pilot_gunsmith",
        summary: "The town gunsmith's rack at the Explosives Shack: personal arms, ammo and pilot gear tiers",
        setup: gunsmith,
    },
];

/// Fills a box of stone, then carves a hall in it (floor at [`HALL_FLOOR`]).
fn hall(world: &mut WorldState, player: PlayerId, xs: std::ops::Range<i32>, height: i32) {
    for y in HALL_FLOOR - height - 3..HALL_FLOOR + 3 {
        for x in xs.start - 3..xs.end + 3 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Stone);
        }
    }
    carve(world, xs.clone(), HALL_FLOOR - height..HALL_FLOOR);
    reveal(
        world,
        player,
        xs.start - 4..xs.end + 4,
        HALL_FLOOR - height - 4..HALL_FLOOR + 4,
    );
}

/// `player`'s rig resting on floor row `floor` at column `x`.
fn rest_rig(world: &mut WorldState, player: PlayerId, x: i32, floor: i32) {
    if let Some(rig) = world.player_mut(player) {
        rig.x = tile_centre(x);
        rig.y = floor as f32 * TILE_SIZE - 11.0;
        rig.x_remainder = 0.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
}

/// Puts `player`'s pilot out with `gear`, its feet at world `(x, feet)`.
fn pilot_out(world: &mut WorldState, player: PlayerId, gear: PilotGear, x: f32, feet: f32) {
    if let Some(rig) = world.player_mut(player) {
        rig.pilot_gear = gear;
    }
    let _ = world.place_pilot(player, x, feet - 0.25);
}

/// The pilot holding `weapon` in a cave hall, firing right at a creature `distance` tiles
/// away.
fn arms_range(stage: &mut SceneStage<'_>, weapon: WeaponKind, enemy: EnemyKind, distance: i32) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        world.set_enemy_difficulty(fatherload_simulation::EnemyDifficulty::Normal);
        hall(world, player, 90..122, 6);
        rest_rig(world, player, 93, HALL_FLOOR);
        if let Some(rig) = world.player_mut(player) {
            let _ = rig.pilot_arsenal.receive(weapon);
            rig.pilot_arsenal.equipped = Some(weapon);
            let _ = rig.pilot_arsenal.add_ammo(AmmoKind::Cartridge, 40);
            let _ = rig.pilot_arsenal.add_ammo(AmmoKind::Shell, 24);
        }
        pilot_out(
            world,
            player,
            PilotGear {
                suit: 1,
                armor: 1,
                headlamp: 2,
                ..PilotGear::default()
            },
            tile_centre(100),
            HALL_FLOOR as f32 * TILE_SIZE,
        );
        let x = tile_centre(100 + distance);
        let y = if enemy == EnemyKind::CaveBat {
            (HALL_FLOOR - 2) as f32 * TILE_SIZE
        } else {
            HALL_FLOOR as f32 * TILE_SIZE - enemy.stats().radius - 1.0
        };
        let _ = world.spawn_enemy(enemy, x, y);
        if enemy == EnemyKind::CaveBat {
            let _ = world.spawn_enemy(enemy, x + 20.0, y - 18.0);
            let _ = world.spawn_enemy(enemy, x + 8.0, y + 14.0);
        }
    });
    stage.settle(3);
    let frames = match weapon {
        WeaponKind::Rifle => 39,
        WeaponKind::Shotgun => 50,
        WeaponKind::Wrench => 31,
        _ => 21,
    };
    stage.hold(
        frames,
        PlayerInput {
            fire: true,
            aim_x: 1.0,
            aim_y: if weapon == WeaponKind::Shotgun {
                -0.15
            } else {
                0.0
            },
            ..PlayerInput::default()
        },
    );
}

/// Flattens columns 2..60 west of town to a dirt meadow on the classic ground line.
fn meadow(world: &mut WorldState) {
    for x in 2..60 {
        for y in -14..GROUND {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
        for y in GROUND..GROUND + 3 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Dirt);
        }
    }
}

fn gliding(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        meadow(world);
        reveal(world, player, 0..60, -14..GROUND + 6);
        rest_rig(world, player, 40, GROUND);
        pilot_out(
            world,
            player,
            PilotGear {
                suit: 1,
                glider: 2,
                ..PilotGear::default()
            },
            tile_centre(12),
            (GROUND - 9) as f32 * TILE_SIZE,
        );
        if let Some((pilot, _, _)) = world.pilot_of(player) {
            world.edit_pilot(pilot, |body, state| {
                state.grounded = false;
                body.velocity_y = 60.0;
            });
        }
    });
    stage.hold(
        28,
        PlayerInput {
            horizontal: 1.0,
            jump: true,
            ..PlayerInput::default()
        },
    );
}

fn grappling(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        hall(world, player, 90..122, 9);
        rest_rig(world, player, 93, HALL_FLOOR);
        pilot_out(
            world,
            player,
            PilotGear {
                suit: 2,
                grapple: 2,
                boots: 1,
                ..PilotGear::default()
            },
            tile_centre(104),
            HALL_FLOOR as f32 * TILE_SIZE,
        );
    });
    stage.settle(3);
    // Fire the hook up and forward, then reel in while swinging.
    stage.hold(
        1,
        PlayerInput {
            rope: true,
            aim_x: 0.55,
            aim_y: -0.83,
            ..PlayerInput::default()
        },
    );
    stage.hold(
        26,
        PlayerInput {
            thrust: true,
            horizontal: 0.6,
            ..PlayerInput::default()
        },
    );
}

/// Stamps `space` into solid stone (its `#` cells keep the stone), revealed for `player`.
fn stamp_space(world: &mut WorldState, player: PlayerId, space: PilotSpace) {
    let bounds = space.bounds();
    for y in bounds.min.y - 4..=bounds.max.y + 4 {
        for x in bounds.min.x - 6..=bounds.max.x + 6 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Stone);
        }
    }
    let _ = world.stamp_tiles(&space.cells());
    reveal(
        world,
        player,
        bounds.min.x - 7..bounds.max.x + 8,
        bounds.min.y - 5..bounds.max.y + 6,
    );
}

fn lever_ruin(stage: &mut SceneStage<'_>) {
    let space = PilotSpace {
        kind: PilotSpaceKind::Ruin,
        origin: TilePosition { x: 96, y: 36 },
        mirrored: false,
        card: 0,
    };
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stamp_space(world, player, space);
        // The rig dug in up to the entrance grate.
        let entrance = space.entrance();
        carve(
            world,
            entrance.x - 5..entrance.x,
            entrance.y - 1..entrance.y + 1,
        );
        rest_rig(world, player, entrance.x - 3, entrance.y + 1);
        if let Some(rig) = world.player_mut(player) {
            let _ = rig.pilot_arsenal.receive(WeaponKind::Sidearm);
        }
        pilot_out(
            world,
            player,
            PilotGear {
                suit: 1,
                headlamp: 2,
                grapple: 1,
                ..PilotGear::default()
            },
            tile_centre(entrance.x + 1),
            (entrance.y + 1) as f32 * TILE_SIZE,
        );
    });
    stage.settle(4);
    // Pull the lever, then walk into the open gateway.
    stage.hold(
        1,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    );
    stage.settle(8);
    stage.hold(
        92,
        PlayerInput {
            horizontal: 1.0,
            ..PlayerInput::default()
        },
    );
}

fn crawlway_cave(stage: &mut SceneStage<'_>) {
    let space = PilotSpace {
        kind: PilotSpaceKind::CrawlwayCave,
        origin: TilePosition { x: 100, y: 38 },
        mirrored: false,
        card: 0,
    };
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stamp_space(world, player, space);
        let entrance = space.entrance();
        carve(
            world,
            entrance.x - 6..entrance.x,
            entrance.y - 1..entrance.y + 1,
        );
        rest_rig(world, player, entrance.x - 4, entrance.y + 1);
        pilot_out(
            world,
            player,
            PilotGear {
                suit: 1,
                headlamp: 3,
                ..PilotGear::default()
            },
            tile_centre(entrance.x + 2),
            (entrance.y + 1) as f32 * TILE_SIZE,
        );
        if let Some((pilot, _, _)) = world.pilot_of(player) {
            world.edit_pilot(pilot, |_, state| state.crouching = true);
        }
    });
    stage.settle(3);
    stage.hold(
        24,
        PlayerInput {
            horizontal: 1.0,
            drill_down: true,
            ..PlayerInput::default()
        },
    );
}

fn winch(stage: &mut SceneStage<'_>) {
    let mut who = None;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        who = Some(player);
        hall(world, player, 92..112, 4);
        // A shaft below the hall floor, one tile wide, beside the rig, in solid stone.
        for y in HALL_FLOOR..HALL_FLOOR + 18 {
            for x in 94..110 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Stone);
            }
        }
        carve(world, 101..102, HALL_FLOOR..HALL_FLOOR + 14);
        reveal(world, player, 96..108, HALL_FLOOR..HALL_FLOOR + 16);
        rest_rig(world, player, 100, HALL_FLOOR);
        pilot_out(
            world,
            player,
            PilotGear {
                suit: 1,
                headlamp: 2,
                ..PilotGear::default()
            },
            tile_centre(100) + 18.0,
            HALL_FLOOR as f32 * TILE_SIZE - 26.0,
        );
    });
    stage.settle(2);
    // Clip onto the winch beside the rig, step over the shaft and pay out.
    stage.hold(
        1,
        PlayerInput {
            rope: true,
            ..PlayerInput::default()
        },
    );
    stage.hold(
        6,
        PlayerInput {
            horizontal: 0.5,
            ..PlayerInput::default()
        },
    );
    stage.play_on(|world| {
        if let Some(player) = who
            && let Some((pilot, _, _)) = world.pilot_of(player)
        {
            // Through the frame, so the `--frame-shift` check stages the same place.
            let x = world.frame().centre_of(101, 0).0;
            world.edit_pilot(pilot, |body, _| {
                body.x = x;
                body.previous_x = body.x;
            });
        }
    });
    stage.hold(
        24,
        PlayerInput {
            drill_down: true,
            ..PlayerInput::default()
        },
    );
}

fn rig_vault(stage: &mut SceneStage<'_>) {
    let space = PilotSpace {
        kind: PilotSpaceKind::RigVault,
        origin: TilePosition { x: 102, y: 36 },
        mirrored: false,
        card: 0,
    };
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stamp_space(world, player, space);
        let origin = space.origin;
        // The rig dug up to the gate; a short side shaft to the pocket's grate.
        carve(world, origin.x - 6..origin.x, origin.y + 2..origin.y + 7);
        rest_rig(world, player, origin.x - 3, origin.y + 7);
        pilot_out(
            world,
            player,
            PilotGear {
                suit: 1,
                headlamp: 1,
                ..PilotGear::default()
            },
            tile_centre(origin.x + 2),
            (origin.y + 3) as f32 * TILE_SIZE,
        );
        if let Some((pilot, _, _)) = world.pilot_of(player) {
            world.edit_pilot(pilot, |_, state| state.mode = PilotMode::Free);
        }
    });
    stage.settle(4);
    stage.hold(
        1,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    );
    stage.settle(16);
}

fn gunsmith(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 9_800;
            let _ = rig.pilot_arsenal.receive(WeaponKind::Sidearm);
            let _ = rig.pilot_arsenal.add_ammo(AmmoKind::Cartridge, 12);
            rig.pilot_gear.jetpack = 1;
            rig.pilot_gear.grapple = 1;
        }
    });
    stage.settle(5);
    put_seat_inside(
        stage,
        0,
        SurfaceZone::Explosives,
        Some(ModalScreen::Explosives),
    );
    stage.seat(0).selected_menu_item = crate::shell::arms::ARMS_FIRST_ROW + 12;
    stage.use_devices(&[WASD]);
}
