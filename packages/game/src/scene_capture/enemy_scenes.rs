//! Enemy scenes (WP-ENEMIES): every family in its habitat band, their tells, a bomb fight with
//! loot, an EMP against machines, a split-screen crew fight and a lineup of every kind.

use fatherload_simulation::{
    EnemyKind, ExplosiveKind, PlacedBomb, PlayerId, SimulationTick, WorldState,
    enemy::EnemyMode,
    entity::{EntityId, EntityState},
    terrain::{DeepStratum, TILE_SIZE, TileKind, TilePosition},
    worldgen::Band,
};

use super::{Scene, SceneStage, carve, place_rig, reveal};
use crate::input::PlayerInput;

/// Column the habitat caverns are dug around (east of the town).
const CAVE_X: i32 = 420;

#[allow(clippy::cast_precision_loss, reason = "small tile indices")]
fn tile(index: i32) -> f32 {
    index as f32 * TILE_SIZE
}

/// World pixel y of a body of `kind` standing on the floor whose top is row `floor`.
#[allow(clippy::cast_precision_loss, reason = "small tile indices")]
fn on_floor(kind: EnemyKind, floor: i32) -> f32 {
    (floor as f32).mul_add(TILE_SIZE, -kind.stats().radius) - 1.0
}

/// The first row of `band` in column `x`, plus `into` rows.
fn band_row(world: &WorldState, band: Band, x: i32, into: i32) -> i32 {
    let terrain = world.terrain();
    (0..terrain.height())
        .find(|y| terrain.band_at(TilePosition { x, y: *y }) == band)
        .map_or(60, |row| row + into)
}

/// A cavern `width` x `height` tiles centred on column `x`, its floor at row `floor`: the band's
/// own rock around it with a few dips in the roof, everything explored by `player`.
fn cavern(world: &mut WorldState, player: PlayerId, x: i32, floor: i32, width: i32, height: i32) {
    let left = x - width / 2;
    for column in left - 3..left + width + 3 {
        for row in floor - height - 3..floor + 3 {
            let position = TilePosition { x: column, y: row };
            if world
                .terrain()
                .tile(position)
                .is_some_and(|t| t.kind == TileKind::Air)
            {
                let _ = world.terrain_mut().set_kind(position, TileKind::Stone);
            }
        }
    }
    for column in left..left + width {
        // A roof with the odd hanging lump.
        let roof = floor - height
            + i32::from(column.rem_euclid(9) == 4)
            + i32::from(column.rem_euclid(13) == 6);
        carve(world, column..column + 1, roof..floor);
    }
    reveal(
        world,
        player,
        left - 8..left + width + 8,
        floor - height - 8..floor + 6,
    );
}

fn spawn(world: &mut WorldState, kind: EnemyKind, x: f32, y: f32) -> EntityId {
    world.spawn_enemy(kind, x, y)
}

/// Sets an enemy moving (a burrower bursting out of the wall).
fn launch(world: &mut WorldState, id: EntityId, velocity_x: f32, velocity_y: f32) {
    world.entities_mut().update(id, |entity| {
        entity.body.velocity_x = velocity_x;
        entity.body.velocity_y = velocity_y;
    });
}

fn set_mode(world: &mut WorldState, id: EntityId, mode: EnemyMode, facing: i8) {
    world.entities_mut().update(id, |entity| {
        if let EntityState::Enemy(state) = &mut entity.state {
            state.mode = mode;
            state.facing = facing;
            state.mode_until = SimulationTick::new(u64::MAX / 2);
            state.attack_ready = SimulationTick::new(u64::MAX / 2);
        }
    });
}

/// A habitat portrait: a cavern in `band`, the rig at its west end, `arrange` places the
/// enemies (given the floor row), then `frames` frames of life.
fn habitat(
    stage: &mut SceneStage<'_>,
    band: Band,
    frames: usize,
    arrange: impl FnOnce(&mut WorldState, PlayerId, i32) + 'static,
) {
    stage.play(1, move |world, seats| {
        let player = seats[0].player;
        let floor = band_row(world, band, CAVE_X, 10);
        cavern(world, player, CAVE_X, floor, 38, 9);
        place_rig(world, player, CAVE_X - 7, floor - 1);
        arrange(world, player, floor);
    });
    stage.settle(frames);
}

fn clay_bats(stage: &mut SceneStage<'_>) {
    habitat(stage, Band::Clay, 14, |world, _, floor| {
        for (dx, dy) in [(-2, -6), (0, -7), (1, -5), (3, -6), (5, -7)] {
            let _ = spawn(
                world,
                EnemyKind::CaveBat,
                tile(CAVE_X + dx),
                tile(floor + dy),
            );
        }
        // A burrower digging its tunnel up through the clay below the floor.
        let _ = spawn(
            world,
            EnemyKind::Burrower,
            tile(CAVE_X - 6),
            tile(floor + 2),
        );
        // And one bursting out of the wall.
        let lunging = spawn(
            world,
            EnemyKind::Burrower,
            tile(CAVE_X + 8),
            tile(floor - 2),
        );
        set_mode(world, lunging, EnemyMode::Strike, -1);
        launch(world, lunging, -150.0, -40.0);
    });
}

fn stone_spitters(stage: &mut SceneStage<'_>) {
    habitat(stage, Band::Stone, 12, |world, _, floor| {
        let spitter = spawn(
            world,
            EnemyKind::AcidSpitter,
            tile(CAVE_X - 4),
            on_floor(EnemyKind::AcidSpitter, floor),
        );
        set_mode(world, spitter, EnemyMode::Windup, -1);
        let _ = spawn(
            world,
            EnemyKind::AcidSpitter,
            tile(CAVE_X + 6),
            on_floor(EnemyKind::AcidSpitter, floor),
        );
        let _ = spawn(
            world,
            EnemyKind::RockMimic,
            tile(CAVE_X + 1),
            on_floor(EnemyKind::RockMimic, floor),
        );
        let awake = spawn(
            world,
            EnemyKind::RockMimic,
            tile(CAVE_X + 12),
            on_floor(EnemyKind::RockMimic, floor),
        );
        set_mode(world, awake, EnemyMode::Windup, -1);
    });
}

fn slate_brood(stage: &mut SceneStage<'_>) {
    habitat(stage, Band::Slate, 40, |world, _, floor| {
        let _ = spawn(
            world,
            EnemyKind::BroodNest,
            tile(CAVE_X + 6),
            on_floor(EnemyKind::BroodNest, floor),
        );
        let shellback = spawn(
            world,
            EnemyKind::Shellback,
            tile(CAVE_X - 3),
            on_floor(EnemyKind::Shellback, floor),
        );
        set_mode(world, shellback, EnemyMode::Windup, -1);
        for (dx, dy) in [(5, -6), (8, -5)] {
            let _ = spawn(
                world,
                EnemyKind::CaveBat,
                tile(CAVE_X + dx),
                tile(floor + dy),
            );
        }
    });
}

fn machine_layer(stage: &mut SceneStage<'_>) {
    habitat(
        stage,
        Band::Deep(DeepStratum::AncientMachineLayer),
        20,
        |world, _, floor| {
            let turret = spawn(
                world,
                EnemyKind::SentryTurret,
                tile(CAVE_X + 4),
                on_floor(EnemyKind::SentryTurret, floor),
            );
            set_mode(world, turret, EnemyMode::Windup, -1);
            let _ = spawn(
                world,
                EnemyKind::SentryTurret,
                tile(CAVE_X + 14),
                on_floor(EnemyKind::SentryTurret, floor),
            );
            let _ = spawn(
                world,
                EnemyKind::RogueExcavator,
                tile(CAVE_X - 4),
                on_floor(EnemyKind::RogueExcavator, floor),
            );
            for (dx, dy) in [(0, -6), (8, -5)] {
                let _ = spawn(
                    world,
                    EnemyKind::MiningDrone,
                    tile(CAVE_X + dx),
                    tile(floor + dy),
                );
            }
        },
    );
}

fn guardian_lair(stage: &mut SceneStage<'_>) {
    habitat(
        stage,
        Band::Deep(DeepStratum::AncientMachineLayer),
        30,
        |world, _, floor| {
            let _ = spawn(
                world,
                EnemyKind::AncientGuardian,
                tile(CAVE_X + 4),
                tile(floor - 4),
            );
            for (dx, dy) in [(-1, -6), (9, -6)] {
                let _ = spawn(
                    world,
                    EnemyKind::MiningDrone,
                    tile(CAVE_X + dx),
                    tile(floor + dy),
                );
            }
        },
    );
}

fn fossil_lord(stage: &mut SceneStage<'_>) {
    habitat(
        stage,
        Band::Deep(DeepStratum::FossilOceans),
        20,
        |world, _, floor| {
            let lord = spawn(
                world,
                EnemyKind::BroodLord,
                tile(CAVE_X + 5),
                on_floor(EnemyKind::BroodLord, floor),
            );
            set_mode(world, lord, EnemyMode::Windup, -1);
            for (dx, dy) in [(1, -7), (3, -6), (10, -7)] {
                let _ = spawn(
                    world,
                    EnemyKind::CaveBat,
                    tile(CAVE_X + dx),
                    tile(floor + dy),
                );
            }
        },
    );
}

fn mantle_wisps(stage: &mut SceneStage<'_>) {
    habitat(
        stage,
        Band::Deep(DeepStratum::MantleStormZone),
        30,
        |world, _, floor| {
            for column in CAVE_X + 2..CAVE_X + 9 {
                let _ = world.terrain_mut().set_kind(
                    TilePosition {
                        x: column,
                        y: floor,
                    },
                    TileKind::Lava,
                );
            }
            for (dx, dy) in [(0, -5), (5, -6), (10, -4)] {
                let _ = spawn(
                    world,
                    EnemyKind::MagmaWisp,
                    tile(CAVE_X + dx),
                    tile(floor + dy),
                );
            }
            let _ = spawn(
                world,
                EnemyKind::RogueExcavator,
                tile(CAVE_X + 14),
                on_floor(EnemyKind::RogueExcavator, floor),
            );
        },
    );
}

/// Bats biting a rig (the red hit flash of its invulnerability window) while a spitter's acid
/// arcs in.
fn rig_bitten(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let floor = band_row(world, Band::Stone, CAVE_X, 10);
        cavern(world, player, CAVE_X, floor, 38, 9);
        place_rig(world, player, CAVE_X - 6, floor - 1);
        for (dx, dy) in [(-4, -2), (-7, -3), (-5, -4)] {
            let _ = spawn(
                world,
                EnemyKind::CaveBat,
                tile(CAVE_X + dx),
                tile(floor + dy),
            );
        }
        let _ = spawn(
            world,
            EnemyKind::AcidSpitter,
            tile(CAVE_X + 2),
            on_floor(EnemyKind::AcidSpitter, floor),
        );
    });
    stage.settle(58);
}

/// A bomb among a pack of bats and a spitter, the moment it blows.
fn bomb_fight(stage: &mut SceneStage<'_>, late: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let floor = band_row(world, Band::Stone, CAVE_X, 10);
        cavern(world, player, CAVE_X, floor, 38, 9);
        place_rig(world, player, CAVE_X - 10, floor - 1);
        for (dx, dy) in [(-5, -3), (-4, -4), (-6, -2), (-3, -3)] {
            let _ = spawn(
                world,
                EnemyKind::CaveBat,
                tile(CAVE_X + dx),
                tile(floor + dy),
            );
        }
        let _ = spawn(
            world,
            EnemyKind::AcidSpitter,
            tile(CAVE_X - 5),
            on_floor(EnemyKind::AcidSpitter, floor),
        );
        let (x, y) = (tile(CAVE_X - 5), tile(floor) - 7.0);
        let now = world.simulation_tick().get();
        world.insert_charge(PlacedBomb {
            id: 1,
            x,
            y,
            previous_x: x,
            previous_y: y,
            detonation_tick: SimulationTick::new(now + 40),
            timer_seconds: 0.6,
            kind: ExplosiveKind::Sticky,
            owner: Some(player),
            stuck: true,
            ..PlacedBomb::default()
        });
    });
    stage.settle(if late { 75 } else { 42 });
}

/// An EMP among drones and a turret: the machines drop, sparking.
fn emp_machines(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let floor = band_row(
            world,
            Band::Deep(DeepStratum::AncientMachineLayer),
            CAVE_X,
            10,
        );
        cavern(world, player, CAVE_X, floor, 38, 9);
        place_rig(world, player, CAVE_X - 12, floor - 1);
        for (dx, dy) in [(-5, -5), (-2, -6), (1, -4)] {
            let _ = spawn(
                world,
                EnemyKind::MiningDrone,
                tile(CAVE_X + dx),
                tile(floor + dy),
            );
        }
        let _ = spawn(
            world,
            EnemyKind::SentryTurret,
            tile(CAVE_X - 1),
            on_floor(EnemyKind::SentryTurret, floor),
        );
        let (x, y) = (tile(CAVE_X - 3), tile(floor - 4));
        let now = world.simulation_tick().get();
        world.insert_charge(PlacedBomb {
            id: 1,
            x,
            y,
            previous_x: x,
            previous_y: y,
            detonation_tick: SimulationTick::new(now + 6),
            kind: ExplosiveKind::Emp,
            owner: Some(player),
            ..PlacedBomb::default()
        });
    });
    stage.settle(30);
}

/// Two seats fighting a brood in split-screen: one rig bitten (hit flash), one dropping a bomb.
fn split_screen_fight(stage: &mut SceneStage<'_>) {
    stage.play(2, |world, seats| {
        let floor = band_row(world, Band::Slate, CAVE_X, 10);
        cavern(world, seats[0].player, CAVE_X, floor, 40, 9);
        reveal(
            world,
            seats[1].player,
            CAVE_X - 28..CAVE_X + 28,
            floor - 18..floor + 6,
        );
        place_rig(world, seats[0].player, CAVE_X - 6, floor - 1);
        place_rig(world, seats[1].player, CAVE_X + 6, floor - 1);
        for (dx, dy) in [(-5, -4), (-7, -3), (-4, -5)] {
            let _ = spawn(
                world,
                EnemyKind::CaveBat,
                tile(CAVE_X + dx),
                tile(floor + dy),
            );
        }
        let _ = spawn(
            world,
            EnemyKind::BroodNest,
            tile(CAVE_X + 12),
            on_floor(EnemyKind::BroodNest, floor),
        );
        let _ = spawn(
            world,
            EnemyKind::Shellback,
            tile(CAVE_X + 9),
            on_floor(EnemyKind::Shellback, floor),
        );
    });
    stage.hold_seats(
        40,
        &[
            PlayerInput::default(),
            PlayerInput {
                horizontal: -0.6,
                ..PlayerInput::default()
            },
        ],
    );
}

/// Sets an enemy hunting `player` (its next attack ready now).
fn hunt(world: &mut WorldState, id: EntityId, player: PlayerId) {
    let now = world.simulation_tick();
    world.entities_mut().update(id, |entity| {
        if let EntityState::Enemy(state) = &mut entity.state {
            state.mode = EnemyMode::Chase;
            state.target = Some(player);
            state.last_seen = now;
            state.attack_ready = now;
        }
    });
}

/// Enemies pressing in on the rig stop at contact: bats, a drone and a woken mimic dropped
/// right on top of the rig are pushed out to touching (they still bite), never drawn inside it.
fn contact(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let floor = band_row(world, Band::Stone, CAVE_X, 10);
        cavern(world, player, CAVE_X, floor, 30, 8);
        place_rig(world, player, CAVE_X, floor - 1);
        let Some((x, y)) = world.player(player).map(|rig| (rig.x, rig.y)) else {
            return;
        };
        let pressing = [
            (EnemyKind::CaveBat, 4.0, -3.0),
            (EnemyKind::CaveBat, -6.0, -8.0),
            (EnemyKind::CaveBat, 1.0, -12.0),
            (EnemyKind::MiningDrone, -9.0, 2.0),
        ];
        for (kind, dx, dy) in pressing {
            let id = spawn(world, kind, x + dx, y + dy);
            hunt(world, id, player);
        }
        let mimic = spawn(
            world,
            EnemyKind::RockMimic,
            x + 14.0,
            on_floor(EnemyKind::RockMimic, floor),
        );
        hunt(world, mimic, player);
    });
    stage.settle(8);
}

/// Rock stops every enemy the way it stops the rig: a bat pack and a drone hunting the rig from
/// behind a stone wall pile up against it, and a burrower digs a real tunnel through the soft
/// clay toward it instead of ghosting through.
fn blocked_by_rock(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let floor = band_row(world, Band::Clay, CAVE_X, 10);
        cavern(world, player, CAVE_X, floor, 34, 9);
        // A sealed stone wall down the middle of the cavern.
        for row in floor - 10..floor {
            for column in CAVE_X..CAVE_X + 2 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x: column, y: row }, TileKind::Stone);
            }
        }
        // A clay bank west of the rig with a burrower lurking in a pocket at its far side.
        for row in floor - 5..floor {
            for column in CAVE_X - 16..CAVE_X - 8 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x: column, y: row }, TileKind::Clay);
            }
        }
        carve(world, CAVE_X - 15..CAVE_X - 14, floor - 2..floor - 1);
        place_rig(world, player, CAVE_X - 3, floor - 1);
        for (dx, dy) in [(4, -3), (5, -5), (3, -6), (6, -2), (4, -7)] {
            let id = spawn(
                world,
                EnemyKind::CaveBat,
                tile(CAVE_X + dx),
                tile(floor + dy),
            );
            hunt(world, id, player);
        }
        let drone = spawn(
            world,
            EnemyKind::MiningDrone,
            tile(CAVE_X + 6),
            tile(floor - 4),
        );
        hunt(world, drone, player);
        let burrower = spawn(
            world,
            EnemyKind::Burrower,
            TILE_SIZE.mul_add(0.5, tile(CAVE_X - 15)),
            TILE_SIZE.mul_add(0.5, tile(floor - 2)),
        );
        hunt(world, burrower, player);
    });
    stage.settle(70);
}

/// Every kind side by side in the town's safe shaft (they idle there, nobody is hunted).
fn lineup(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let (left, floor) = (96, 26);
        for column in left - 2..left + 30 {
            for row in floor - 9..floor + 3 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x: column, y: row }, TileKind::Stone);
            }
        }
        carve(world, left..left + 28, floor - 8..floor);
        reveal(world, player, left - 6..left + 34, floor - 12..floor + 6);
        place_rig(world, player, left + 14, floor - 8);
        let walkers = [
            (EnemyKind::AcidSpitter, 4),
            (EnemyKind::RockMimic, 7),
            (EnemyKind::Shellback, 10),
            (EnemyKind::BroodNest, 13),
            (EnemyKind::SentryTurret, 16),
            (EnemyKind::RogueExcavator, 19),
            (EnemyKind::BroodLord, 24),
        ];
        for (kind, dx) in walkers {
            let _ = spawn(world, kind, tile(left + dx), on_floor(kind, floor));
        }
        let flyers = [
            (EnemyKind::CaveBat, 4),
            (EnemyKind::MiningDrone, 7),
            (EnemyKind::MagmaWisp, 10),
            (EnemyKind::Burrower, 13),
            (EnemyKind::AncientGuardian, 18),
        ];
        for (kind, dx) in flyers {
            let _ = spawn(world, kind, tile(left + dx), tile(floor - 5));
        }
    });
    stage.settle(6);
}

pub const ENEMY_SCENES: &[Scene] = &[
    Scene {
        name: "enemies_lineup",
        summary: "Every enemy kind side by side: creatures and machines",
        setup: lineup,
    },
    Scene {
        name: "enemies_clay_bats",
        summary: "Clay caves: a bat swarm, a burrower dug in under the floor and one bursting out",
        setup: clay_bats,
    },
    Scene {
        name: "enemies_stone_spitters",
        summary: "Stone caverns: acid spitters (one puffing up), a sleeping rock mimic and a woken one",
        setup: stone_spitters,
    },
    Scene {
        name: "enemies_slate_brood",
        summary: "Slate: a brood nest hatching bats while a shellback winds up a charge",
        setup: slate_brood,
    },
    Scene {
        name: "enemies_machine_layer",
        summary: "The Ancient Machine Layer: sentry turrets charging, a rogue excavator, mining drones",
        setup: machine_layer,
    },
    Scene {
        name: "enemies_guardian_lair",
        summary: "An ancient guardian's lair: the shielded elite with its drone escort",
        setup: guardian_lair,
    },
    Scene {
        name: "enemies_fossil_lord",
        summary: "A brood lord roaring in a fossil-ocean lair cavern",
        setup: fossil_lord,
    },
    Scene {
        name: "enemies_mantle_wisps",
        summary: "The mantle: magma wisps over lava and a rogue excavator",
        setup: mantle_wisps,
    },
    Scene {
        name: "enemies_fight_bomb",
        summary: "A fight: a sticky bomb among a bat pack and a spitter, the moment it blows",
        setup: |stage| bomb_fight(stage, false),
    },
    Scene {
        name: "enemies_fight_bitten",
        summary: "Bats biting the rig (red hit flash, knockback) while acid arcs in from a spitter",
        setup: rig_bitten,
    },
    Scene {
        name: "enemies_fight_after",
        summary: "Just after the blast: the survivors scatter and the spitter turns on the rig",
        setup: |stage| bomb_fight(stage, true),
    },
    Scene {
        name: "enemies_contact",
        summary: "Bats, a drone and a mimic pressing on the rig stop at contact, never inside it",
        setup: contact,
    },
    Scene {
        name: "enemies_blocked_by_rock",
        summary: "Bats and a drone held back by a stone wall; a burrower digs a real tunnel to the rig",
        setup: blocked_by_rock,
    },
    Scene {
        name: "enemies_emp",
        summary: "An EMP knocking out drones and a turret (stunned, sparking, falling)",
        setup: emp_machines,
    },
    Scene {
        name: "enemies_split_screen",
        summary: "Split-screen crew fight: bats on one rig, a nest and shellback by the other",
        setup: split_screen_fight,
    },
];
