//! The distortion strata's scenes (Wave 9, Descent VI part A of a Deep Descent world): a
//! mirror hall with the rig's twin (at full and reduced effects), the Looking-Glass Hall and its
//! mirror ore pairs, a gazer's gaze, a shear hall walked on its wall and one on its ceiling, the
//! Shear Spire, Pixel Rot near its top and at its bottom (full and reduced), a rotted view, the
//! Dead Screen and Parallax Station.

use fatherload_simulation::{
    PlayerId, WorldState,
    depth_progression::ProgressionRules,
    distortion::{mirror, rot, shear},
    enemy::{EnemyDifficulty, EnemyKind},
    terrain::{Terrain, TileKind, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{self, landmarks},
    },
};

use super::{Scene, SceneStage, place_rig, reveal};
use crate::screen_fx::EffectsIntensity;

/// Seed of every distortion scene world (the Deep Descent scenes' seed).
const SCENE_SEED: u64 = 0x0D3E_9DE5;
/// Column the scenes look around.
const SCENE_X: i32 = 120;

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
    // The places' story cards would cover the views.
    world.mark_distortion_landmarks_found();
}

/// Fits the rig for the deep (every protection and gate system).
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        fatherload_simulation::economy::max_out_upgrades(rig);
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.hull = rig.max_hull();
        rig.credits = 60_000;
    }
}

/// Spawns `kind` at tile `(x, y)` (through the world's frame).
fn spawn(world: &mut WorldState, kind: EnemyKind, x: i32, y: i32) {
    let (px, py) = world.frame().tile_center(TilePosition { x, y });
    let _ = world.spawn_enemy(kind, px, py);
}

/// The rig at tile `(x, y)`, the land around it explored.
fn stage_at(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    deep_world(world);
    fit(world, player);
    place_rig(world, player, x, y);
    reveal(world, player, x - 60..x + 60, y - 36..y + 36);
    world.stream_terrain_near_players();
}

/// The mirror hall nearest the scene column, deep in the stratum (wide and tall).
fn deep_hall() -> Option<mirror::MirrorZone> {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.rows(fatherload_simulation::descent::Stratum::MirrorStrata);
    let area = TileRect {
        min: TilePosition {
            x: -700,
            y: top + (bottom - top) / 2,
        },
        max: TilePosition { x: 900, y: bottom },
    };
    mirror::zones_touching(&settings, &layout, &area)
        .into_iter()
        .filter(|zone| !zone.landmark && zone.bounds.width() >= 26)
        .min_by_key(|zone| (zone.bounds.center().x - SCENE_X).abs())
}

/// A mirror hall: the rig on its floor left of the axis, its twin stepping out of the glass on
/// the right, a glassback and a gazer about the hall (the screen mirrored inside).
fn mirror_hall(stage: &mut SceneStage<'_>, effects: EffectsIntensity) {
    stage.shell().effects = effects;
    let Some(hall) = deep_hall() else {
        return;
    };
    let floor = hall.bounds.max.y - 2;
    let axis = hall.axis2() / 2;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stage_at(world, player, axis - 4, floor);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        spawn(world, EnemyKind::Glassback, axis - 10, floor);
        spawn(world, EnemyKind::Gazer, axis + 10, floor);
        reveal(
            world,
            player,
            hall.bounds.min.x - 20..hall.bounds.max.x + 20,
            hall.bounds.min.y - 10..hall.bounds.max.y + 10,
        );
    });
    stage.settle(420);
}

/// The Looking-Glass Hall: its glass mirror on the axis, the strongbox and its reflection on
/// the floor, mirror ore pairs in the walls.
fn looking_glass_hall(stage: &mut SceneStage<'_>) {
    let settings = settings();
    let layout = descent::layout(&settings);
    let Some(hall) = mirror::looking_glass_hall(&settings, &layout) else {
        return;
    };
    let floor = hall.bounds.max.y - 2;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stage_at(world, player, hall.bounds.min.x + 8, floor);
        world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
        // A way in from the side through the frame.
        for x in hall.bounds.min.x - 4..hall.bounds.min.x + 3 {
            for y in floor - 1..=floor {
                world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Air);
            }
        }
        reveal(
            world,
            player,
            hall.bounds.min.x - 20..hall.bounds.max.x + 20,
            hall.bounds.min.y - 10..hall.bounds.max.y + 10,
        );
    });
    stage.settle(420);
}

/// A gazer's gaze: the rig in plain rock, held by a gazer's eye (its screen mirrors).
fn gazer(stage: &mut SceneStage<'_>) {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.rows(fatherload_simulation::descent::Stratum::MirrorStrata);
    let x = landmarks::outpost_column(&settings) + 70;
    let y = top + (bottom - top) / 4;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stage_at(world, player, x - 5, y);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        for column in x - 12..=x + 12 {
            for row in y - 5..=y + 1 {
                world
                    .terrain_mut()
                    .set_kind(TilePosition { x: column, y: row }, TileKind::Air);
            }
            world.terrain_mut().set_kind(
                TilePosition {
                    x: column,
                    y: y + 2,
                },
                TileKind::Stone,
            );
        }
        spawn(world, EnemyKind::Gazer, x + 6, y + 1);
        spawn(world, EnemyKind::Glassback, x + 2, y + 1);
    });
    stage.settle(400);
}

/// A shear hall turned `turn` degrees: the rig on its turned floor, flow lines streaming, the
/// stratum's three creatures about.
fn shear_hall(stage: &mut SceneStage<'_>, turn: u16) {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.rows(fatherload_simulation::descent::Stratum::GravityShear);
    let area = TileRect {
        min: TilePosition { x: -900, y: top },
        max: TilePosition { x: 900, y: bottom },
    };
    let Some(region) = shear::regions_touching(&settings, &layout, &area)
        .into_iter()
        .filter(|region| region.turn == turn)
        .min_by_key(|region| (region.bounds.center().x - SCENE_X).abs())
    else {
        return;
    };
    let b = region.bounds;
    let centre = b.center();
    let (dx, dy) = region.down();
    // On the turned floor: three tiles in from the floor side.
    let spot = (
        centre.x + dx * (b.width() / 2 - 4),
        centre.y + dy * (b.height() / 2 - 4),
    );
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stage_at(world, player, spot.0, spot.1);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        if let Some(rig) = world.player_mut(player) {
            rig.attitude.orientation = f32::from(turn).to_radians();
        }
        spawn(
            world,
            EnemyKind::Lodestar,
            centre.x - dx * 3,
            centre.y - dy * 3,
        );
        spawn(
            world,
            EnemyKind::Tumbler,
            centre.x + dy * 5,
            centre.y - dx * 5,
        );
        spawn(
            world,
            EnemyKind::ShearMantis,
            centre.x - dy * 4,
            centre.y + dx * 4,
        );
        reveal(
            world,
            player,
            b.min.x - 24..b.max.x + 24,
            b.min.y - 16..b.max.y + 16,
        );
    });
    stage.settle(400);
}

/// The Shear Spire: the rig on its right wall, flow lines pointing at every wall.
fn shear_spire(stage: &mut SceneStage<'_>) {
    let settings = settings();
    let layout = descent::layout(&settings);
    let Some(spire) = shear::spire(&settings, &layout) else {
        return;
    };
    let centre = spire.center();
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stage_at(world, player, spire.max.x - 4, centre.y + 6);
        world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
        if let Some(rig) = world.player_mut(player) {
            rig.attitude.orientation = -std::f32::consts::FRAC_PI_2;
        }
        reveal(
            world,
            player,
            spire.min.x - 10..spire.max.x + 10,
            spire.min.y - 10..spire.max.y + 10,
        );
    });
    stage.settle(420);
}

/// Pixel Rot at tier `tier`: the rig in a blocky void, a glitchling about to land, a bitworm and
/// a dead pixel.
fn pixel_rot(stage: &mut SceneStage<'_>, tier: usize, effects: EffectsIntensity) {
    stage.shell().effects = effects;
    let settings = settings();
    let layout = descent::layout(&settings);
    let tops = rot::tier_tops(&layout);
    let x = SCENE_X + 40;
    let y = tops[tier.min(rot::ROT_TIERS - 1)] + 14;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stage_at(world, player, x, y);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        // A void to stand in (on the grid), the rot all round it.
        for column in x - 10..=x + 10 {
            for row in y - 6..=y {
                world
                    .terrain_mut()
                    .set_kind(TilePosition { x: column, y: row }, TileKind::Air);
            }
        }
        spawn(world, EnemyKind::Glitchling, x - 6, y - 4);
        spawn(world, EnemyKind::Bitworm, x + 7, y - 5);
        spawn(world, EnemyKind::DeadPixel, x + 4, y - 2);
    });
    stage.settle(400);
}

/// The Dead Screen: the builder picture in giant pixels, seen from its gallery.
fn dead_screen(stage: &mut SceneStage<'_>) {
    let settings = settings();
    let layout = descent::layout(&settings);
    let Some(screen) = rot::dead_screen(&settings, &layout) else {
        return;
    };
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stage_at(world, player, screen.center().x - 6, screen.max.y - 3);
        world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
        reveal(
            world,
            player,
            screen.min.x - 10..screen.max.x + 10,
            screen.min.y - 10..screen.max.y + 10,
        );
    });
    stage.settle(420);
}

/// Parallax Station: the Descent VI outpost, its hall and the cable beside it.
fn parallax_station(stage: &mut SceneStage<'_>) {
    let settings = settings();
    let layout = descent::layout(&settings);
    let Some(hall) = landmarks::outpost(
        &settings,
        &layout,
        fatherload_simulation::descent::Descent::StrangeLayers,
    ) else {
        return;
    };
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        // Between the outpost's hall and the research station's.
        let outpost = hall.bounds.center().x;
        let station = fatherload_simulation::distortion::parallax_station(&settings)
            .map_or(outpost, |(center, _)| center);
        let x = outpost.midpoint(station);
        stage_at(world, player, x, hall.bounds.max.y - 1);
        world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
        reveal(
            world,
            player,
            x - 70..x + 70,
            hall.bounds.max.y - 30..hall.bounds.max.y + 20,
        );
    });
    stage.settle(420);
}

pub(super) const DISTORTION_SCENES: &[Scene] = &[
    Scene {
        name: "distortion_mirror_hall",
        summary: "The Mirror Strata: a mirror hall flips the screen and the controls; the rig's twin steps out of the glass; a glassback and a gazer",
        setup: |stage| mirror_hall(stage, EffectsIntensity::Full),
    },
    Scene {
        name: "distortion_mirror_hall_reduced",
        summary: "The same mirror hall with screen effects reduced (the mirror stays whole, the shimmer softer)",
        setup: |stage| mirror_hall(stage, EffectsIntensity::Reduced),
    },
    Scene {
        name: "distortion_looking_glass_hall",
        summary: "The Looking-Glass Hall: a glass mirror on the axis, a strongbox and its reflection, mirror ore pairs in the walls",
        setup: looking_glass_hall,
    },
    Scene {
        name: "distortion_gazer",
        summary: "A gazer's gaze: while its eye holds the rig, the rig's controls and screen mirror",
        setup: gazer,
    },
    Scene {
        name: "distortion_shear_wall",
        summary: "The Gravity Shear: a shear hall turned 90 degrees, the rig on its wall, flow lines, a lodestar, a tumbler and a mantis",
        setup: |stage| shear_hall(stage, 90),
    },
    Scene {
        name: "distortion_shear_ceiling",
        summary: "A shear hall turned 180 degrees: the rig walks its ceiling",
        setup: |stage| shear_hall(stage, 180),
    },
    Scene {
        name: "distortion_shear_spire",
        summary: "The Shear Spire: a hollow tower where down is the nearest wall",
        setup: shear_spire,
    },
    Scene {
        name: "distortion_pixel_rot_shallow",
        summary: "Pixel Rot near its top: the first coarse pixels and a duller palette; a glitchling, a bitworm, a dead pixel",
        setup: |stage| pixel_rot(stage, 1, EffectsIntensity::Full),
    },
    Scene {
        name: "distortion_pixel_rot_deep",
        summary: "Pixel Rot at its bottom: huge pixels, a few colours, glitching bands, rock in big blocks",
        setup: |stage| pixel_rot(stage, 4, EffectsIntensity::Full),
    },
    Scene {
        name: "distortion_pixel_rot_deep_reduced",
        summary: "Pixel Rot at its bottom with screen effects reduced",
        setup: |stage| pixel_rot(stage, 4, EffectsIntensity::Reduced),
    },
    Scene {
        name: "distortion_dead_screen",
        summary: "The Dead Screen: a builder picture in giant pixels, the cable from the hanging stone to a sleeping star",
        setup: dead_screen,
    },
    Scene {
        name: "distortion_parallax_station",
        summary: "Parallax Station: the Descent VI outpost in the Mirror Strata",
        setup: parallax_station,
    },
];
