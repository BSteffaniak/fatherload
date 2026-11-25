//! Descent VII part B's scenes (Wave 10 ancients-b): a garrison fort's waves against the
//! build kit's defences (the bays' tell, a wave held off by turrets, barricades and traps),
//! the Grand Bastion, the Seal Gate's sockets (some keys missing, then every key lighting as
//! it opens), a tremor's falling stones, and every stage of the Gatekeeper Colossus with its
//! hall collapsing between them.

use fatherload_simulation::{
    PlayerId, WorldState,
    depth_progression::{ProgressionRules, RigSystem},
    enemy::EnemyKind,
    garrison::{ColossusStage, DefenceKind, GarrisonLandmark},
    terrain::{Terrain, TilePosition},
    weapon::WeaponKind,
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{
            self,
            garrison::{self as sites, Fort},
            landmarks,
        },
    },
};

use super::{Scene, SceneStage, place_rig, reveal};

/// Seed of every Descent VII part B scene world (the Deep Descent scenes' seed).
const SCENE_SEED: u64 = 0x0D3E_9DE5;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// Swaps the staged world's terrain for the scene world, with the Descent rules.
fn deep_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
    world.set_progression_rules(ProgressionRules::Descent);
    world.install_logistics_sites(&landmarks::logistics_sites(&settings()), false);
    world.grant_deep_claim();
}

/// Fits the rig for Descent VII (every gate system through the Phase Drill; no Heart Key).
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 15;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(15);
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.engine_level = 15;
        rig.drill_strength = 15;
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.hazard_gear.rig_systems &= !RigSystem::HeartKey.bit();
        rig.radiator_level = 15;
        rig.hull_level = 15;
        rig.hull = rig.max_hull();
        rig.credits = 900_000;
        rig.weapon_rack.battery_level = fatherload_simulation::weapon::MAX_BATTERY_LEVEL;
        let _ = rig.receive_weapon(WeaponKind::Railgun);
        let _ = rig.mount_weapon(Some(WeaponKind::Railgun));
    }
}

/// The rig at tile `(x, y)`, the land around it explored, every landmark's card already read.
fn stage_at(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    deep_world(world);
    fit(world, player);
    place_rig(world, player, x, y);
    reveal(world, player, x - 60..x + 60, y - 36..y + 36);
    world.stream_terrain_near_players();
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
    world.set_garrison(fatherload_simulation::garrison::Garrison {
        found: GarrisonLandmark::ALL.into_iter().collect(),
        ..world.garrison().clone()
    });
}

/// The barracks fort nearest the spawn column (or the Bastion).
fn fort(bastion: bool) -> Option<Fort> {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.rows(fatherload_simulation::descent::Stratum::SleepingGarrison);
    let area = TileRect {
        min: TilePosition { x: -700, y: top },
        max: TilePosition { x: 900, y: bottom },
    };
    sites::forts_with(&settings, &layout, &area)
        .into_iter()
        .filter(|fort| (fort.kind == fatherload_simulation::garrison::FortKind::Bastion) == bastion)
        .min_by_key(|fort| (fort.bounds.center().x - 97).abs())
}

/// A garrison fort: the defences built round the pylon, wave two telegraphed (`telegraph`) or
/// out and held off.
fn garrison_defence(stage: &mut SceneStage<'_>, telegraph: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(fort) = fort(false) else {
            return;
        };
        let pylon = fort.pylon();
        stage_at(world, player, pylon.x - 4, pylon.y);
        // The defences: two turrets either side of the pylon, a barricade line at each gate,
        // shock traps in front of them.
        for (kind, dx) in [
            (DefenceKind::Turret, -3),
            (DefenceKind::Turret, 5),
            (DefenceKind::ShockTrap, -9),
            (DefenceKind::ShockTrap, 11),
            (DefenceKind::Barricade, -12),
            (DefenceKind::Barricade, 14),
        ] {
            world.stage_defence(
                player,
                kind,
                TilePosition {
                    x: pylon.x + dx,
                    y: pylon.y,
                },
            );
        }
        world.stage_defence(
            player,
            DefenceKind::Barricade,
            TilePosition {
                x: pylon.x - 12,
                y: pylon.y - 1,
            },
        );
        world.stage_defence(
            player,
            DefenceKind::Barricade,
            TilePosition {
                x: pylon.x + 14,
                y: pylon.y - 1,
            },
        );
    });
    // Past the entering banner and the fort's first stirring.
    stage.settle(200);
    stage.play_on(|world| {
        let Some(fort) = fort(false) else {
            return;
        };
        world.stage_fort_wave(fort.pylon(), 2);
    });
    // The tell runs five seconds; then the wave is out and the defences at work.
    stage.settle(if telegraph { 90 } else { 300 + 420 });
}

/// The Grand Bastion, its garrison stirring (the build window).
fn grand_bastion(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(fort) = fort(true) else {
            return;
        };
        let pylon = fort.pylon();
        stage_at(world, player, pylon.x - 6, pylon.y);
    });
    stage.settle(90);
}

/// The Seal Court at the door: the sockets lit for the keys the rig carries (`missing` keys
/// dark), or every key lighting as the door opens (`opening`).
fn seal_gate(stage: &mut SceneStage<'_>, missing: &'static [RigSystem], opening: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(hall) = sites::colossus_hall(&settings, &layout) else {
            return;
        };
        let door = sites::seal_door(&settings, &hall);
        let side = sites::door_side(&settings);
        let x = if side < 0 {
            door.min.x - 5
        } else {
            door.max.x + 5
        };
        stage_at(world, player, x, sites::upper_floor_row(&hall) - 1);
        if let Some(rig) = world.player_mut(player) {
            for system in missing {
                rig.hazard_gear.rig_systems &= !system.bit();
            }
        }
        let _ = (side, EnemyKind::SealWarden);
    });
    stage.settle(if opening { 300 } else { 240 });
}

/// A tremor in the Seal Gate: dust pouring over the stones about to fall round the rig, a
/// seal warden and a gilded crawler about.
fn tremor(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let (top, bottom) = layout.rows(fatherload_simulation::descent::Stratum::TheSealGate);
        let (x, y) = (60, top + (bottom - top) / 3);
        stage_at(world, player, x, y);
        // A low gilded gallery round the rig.
        super::carve(world, x - 14..x + 14, y - 3..y + 1);
        let (wx, wy) = world
            .frame()
            .tile_center(TilePosition { x: x + 7, y: y - 2 });
        let _ = world.spawn_enemy(EnemyKind::SealWarden, wx, wy);
        let (cx, cy) = world.frame().tile_center(TilePosition { x: x - 8, y });
        let _ = world.spawn_enemy(EnemyKind::GildedCrawler, cx, cy);
    });
    // Past the entering banner, the tremor: its dust gathers over the stones.
    stage.settle(220);
    stage.play_on(|world| {
        let mut garrison = world.garrison().clone();
        garrison.next_tremor = world.simulation_tick().next();
        world.set_garrison(garrison);
    });
    stage.settle(30);
}

/// The Gatekeeper's hall with the Colossus awake on `stage_of`, the rig on the door-side
/// floor; `collapsing` shows the collapse's tell, `fist` a fist raised over the rig.
fn colossus(stage: &mut SceneStage<'_>, stage_of: ColossusStage, collapsing: bool, fist: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(hall) = sites::colossus_hall(&settings, &layout) else {
            return;
        };
        let side = sites::door_side(&settings);
        let x = if side < 0 {
            hall.min.x + 9
        } else {
            hall.max.x - 9
        };
        stage_at(world, player, x, sites::upper_floor_row(&hall) - 1);
        reveal(
            world,
            player,
            hall.min.x - 8..hall.max.x + 8,
            hall.min.y - 8..hall.max.y + 8,
        );
    });
    // It wakes on the first tick a rig is in its hall; past the entering banner, the stage.
    stage.settle(240);
    stage.play_on(|world| {
        world.stage_colossus(stage_of, collapsing);
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(hall) = sites::colossus_hall(&settings, &layout) else {
            return;
        };
        // Bring the rig within sight of it: a working distance on its near side.
        let post = sites::colossus_post(&settings, &hall, stage_of);
        let side = sites::door_side(&settings);
        // Behind it, so the HUD's corner panels leave it in view.
        let x = post.x - side * 7;
        let y = match stage_of {
            ColossusStage::Guard => sites::upper_floor_row(&hall) - 1,
            ColossusStage::Fallen => sites::crypt_floor_row(&hall) - 4,
            ColossusStage::Unbound => sites::upper_floor_row(&hall) - 4,
        };
        let players: Vec<PlayerId> = world.players().keys().copied().collect();
        let frame = world.frame();
        for player in players {
            if let Some(rig) = world.player_mut(player) {
                rig.x = (frame.rel_col(x) + 0.5) * fatherload_simulation::terrain::TILE_SIZE;
                rig.y = (frame.rel_row(y) + 0.5) * fatherload_simulation::terrain::TILE_SIZE;
                rig.velocity_x = 0.0;
                rig.velocity_y = 0.0;
            }
        }
        if fist {
            world.stage_colossus_fist(
                u8::from(side < 0),
                TilePosition {
                    x,
                    y: sites::upper_floor_row(&hall) - 1,
                },
            );
        }
    });
    stage.settle(if collapsing { 50 } else { 24 });
}

pub const GARRISON_SCENES: &[Scene] = &[
    Scene {
        name: "ab_garrison_telegraph",
        summary: "Descent VII, the Sleeping Garrison: a fort's second wave telegraphed (its bays pulsing red) while turrets, barricades and shock traps stand ready round the hold pylon",
        setup: |stage| garrison_defence(stage, true),
    },
    Scene {
        name: "ab_garrison_defence",
        summary: "Garrison defence in progress: war sentries, lancer drones and a siege walker out of the bays, the turrets firing, traps arcing, a barricade battered",
        setup: |stage| garrison_defence(stage, false),
    },
    Scene {
        name: "ab_grand_bastion",
        summary: "The Grand Bastion stirring: racks of sleeping machines, the hold pylon's ring of light, the build window counting down",
        setup: grand_bastion,
    },
    Scene {
        name: "ab_seal_gate_sockets",
        summary: "The Seal Gate: six sockets on the door, lit for the keys the rig carries; the Phase Drill's socket dark and the HUD naming it",
        setup: |stage| seal_gate(stage, &[RigSystem::PhaseDrill], false),
    },
    Scene {
        name: "ab_seal_gate_opening",
        summary: "Every key of every Descent: the Seal Gate's sockets lit one by one and the great door sinking into the floor",
        setup: |stage| seal_gate(stage, &[], true),
    },
    Scene {
        name: "ab_seal_tremor",
        summary: "The Seal Gate's tremor: dust pouring over the stones about to fall round the rig, a seal warden and a gilded crawler",
        setup: tremor,
    },
    Scene {
        name: "ab_colossus_guard",
        summary: "The Gatekeeper Colossus, stage one (the Guard): towering at the hall's far end over the cracked floor",
        setup: |stage| colossus(stage, ColossusStage::Guard, false, false),
    },
    Scene {
        name: "ab_colossus_slam",
        summary: "Stage one: a fist raised over its red mark before it slams",
        setup: |stage| colossus(stage, ColossusStage::Guard, false, true),
    },
    Scene {
        name: "ab_colossus_collapsing",
        summary: "The hall's first collapse telegraphed: the Colossus roars, the cracks glow, dust pours (it cannot attack or be hurt)",
        setup: |stage| colossus(stage, ColossusStage::Guard, true, false),
    },
    Scene {
        name: "ab_colossus_fallen",
        summary: "Stage two (the Fall): the floor gone, the Colossus in the crypt, rubble ramps at its ends, a fist sweeping",
        setup: |stage| colossus(stage, ColossusStage::Fallen, false, true),
    },
    Scene {
        name: "ab_colossus_unbound",
        summary: "Stage three (Unbound): the vault down to rubble platforms, the legs gone, the bare core blazing as it hovers",
        setup: |stage| colossus(stage, ColossusStage::Unbound, false, false),
    },
];
