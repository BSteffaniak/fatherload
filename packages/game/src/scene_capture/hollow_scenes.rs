//! The Hollow World's scenes (Wave 9, Descent V of a Deep Descent world): the breakthrough's
//! reveal, every stratum with its creatures by day and by night, the Fathers' Breach, the
//! Sunfolk village, a pilot riding a crestback, a rig flipped to land on an isle's underside,
//! the Sunforge's glare and shade, the Gravity Anchor on a wall, and the Sun Serpent's fight
//! with the inner sun's flare.

#![allow(
    clippy::suboptimal_flops,
    clippy::missing_const_for_fn,
    reason = "scene staging: plain arithmetic on tile positions"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    depth_progression::{ProgressionRules, RigSystem},
    descent::Stratum,
    enemy::EnemyKind,
    hollow::{INNER_DAWN_MILLIS, INNER_DAY_MILLIS, INNER_NOON_MILLIS},
    terrain::{Terrain, TileKind, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{self, hollow as places, landmarks},
    },
};

use super::{Scene, SceneStage, place_rig, reveal};

/// Seed of every Hollow World scene world (the Deep Descent scenes' seed).
const SCENE_SEED: u64 = 0x0D3E_9DE5;
/// Column the scenes look around.
const SCENE_X: i32 = 120;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

fn deep_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
    // Creatures about (a peaceful world has none, and yields the serpent's gate unfought).
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
    world.set_progression_rules(ProgressionRules::Descent);
    world.install_logistics_sites(&landmarks::logistics_sites(&settings()), false);
    world.grant_deep_claim();
}

/// A Descent V rig: every protection and every earlier gate system, the Gravity Anchor only
/// when `anchor`.
fn fit(world: &mut WorldState, player: PlayerId, anchor: bool) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 16;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(16);
        rig.fuel = rig.fuel_capacity * 0.75;
        rig.engine_level = 14;
        rig.drill_strength = 15;
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        if !anchor {
            rig.hazard_gear.rig_systems &= !RigSystem::GravityAnchor.bit();
        }
        rig.radiator_level = 22;
        rig.hull_level = 16;
        rig.hull = rig.max_hull();
        rig.credits = 80_000;
    }
}

/// World time of the inner `phase`: by day (`true`) mid-morning of the inner day, else deep in
/// the inner night (on the surface clock's own day as well).
const fn inner_time(day: bool) -> u64 {
    let base = INNER_DAY_MILLIS * 40;
    if day {
        base + INNER_DAWN_MILLIS + INNER_NOON_MILLIS / 3
    } else {
        base + INNER_DAY_MILLIS - 60_000
    }
}

fn spawn(world: &mut WorldState, kind: EnemyKind, x: i32, y: i32) {
    let (px, py) = world.frame().tile_center(TilePosition { x, y });
    let _ = world.spawn_enemy(kind, px, py);
}

/// The rig at tile `(x, y)`, the land round it explored, the reveal already played (unless
/// `fresh`), at inner day or night.
fn stage_at(world: &mut WorldState, player: PlayerId, (x, y): (i32, i32), day: bool, fresh: bool) {
    deep_world(world);
    fit(world, player, false);
    world.set_world_time_millis(inner_time(day));
    place_rig(world, player, x, y);
    world.stream_terrain_near_players();
    reveal(world, player, x - 64..x + 64, y - 40..y + 40);
    if !fresh {
        world.mark_hollow_revealed(player);
    }
}

/// The first open row of the Inner Sky's cavern under column `x` plus `below`.
fn sky_row(x: i32, below: i32) -> i32 {
    let settings = settings();
    let layout = descent::layout(&settings);
    descent::hollow_rows(&settings, &layout, x).0 + below
}

/// A clear column of the Inner Sky near the scene column (no floating stone in the rows the
/// scene shows).
fn open_sky_column(rows: std::ops::Range<i32>) -> i32 {
    let settings = settings();
    let layout = descent::layout(&settings);
    (0..200)
        .map(|step| {
            SCENE_X
                + if step % 2 == 0 {
                    step / 2
                } else {
                    -(step / 2) - 1
                }
        })
        .find(|x| {
            rows.clone().all(|dy| {
                let y = sky_row(*x, dy);
                descent::hollow_open(&settings, &layout, *x, y)
                    && !places::sky_stone(&settings, &layout, *x, y)
            })
        })
        .unwrap_or(SCENE_X)
}

/// The breakthrough: the rig has just broken through the rock sky; the light floods in.
fn breakthrough(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let x = open_sky_column(0..8);
        let y = sky_row(x, 2);
        stage_at(world, player, (x, y), true, true);
        if let Some(rig) = world.player_mut(player) {
            rig.velocity_y = 120.0;
        }
    });
    stage.settle(26);
}

/// The Inner Sky with its creatures: sky mantas on the wind, a stoop roc, gust bladders.
fn inner_sky(stage: &mut SceneStage<'_>, day: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let x = open_sky_column(30..60);
        let y = sky_row(x, 46);
        stage_at(world, player, (x, y), day, false);
        if day {
            spawn(world, EnemyKind::SkyManta, x - 12, y - 6);
            spawn(world, EnemyKind::SkyManta, x + 16, y + 4);
            spawn(world, EnemyKind::StoopRoc, x + 8, y - 10);
            spawn(world, EnemyKind::GustBladder, x - 6, y + 5);
        } else {
            spawn(world, EnemyKind::SkyManta, x + 10, y - 3);
            spawn(world, EnemyKind::GustBladder, x - 9, y + 2);
        }
    });
    stage.settle(220);
}

/// The Fathers' Breach: their winch platform hanging under the rock sky.
fn breach(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(rect) = places::breach(&settings, &layout) else {
            return;
        };
        let centre = rect.center();
        stage_at(world, player, (centre.x, rect.max.y + 6), true, false);
    });
    stage.settle(200);
}

/// A jungle hall: crestbacks grazing, a skyneck, a fangrunner pack (by day) or the pack hunting
/// (by night).
fn jungle(stage: &mut SceneStage<'_>, day: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let (x, floor) = jungle_floor();
        stage_at(world, player, (x, floor - 2), day, false);
        spawn(world, EnemyKind::Crestback, x - 9, floor - 1);
        spawn(world, EnemyKind::Crestback, x + 7, floor - 1);
        spawn(world, EnemyKind::Skyneck, x + 16, floor - 2);
        spawn(world, EnemyKind::Fangrunner, x - 18, floor - 1);
        spawn(world, EnemyKind::Fangrunner, x - 20, floor - 1);
    });
    stage.settle(180);
}

/// A jungle hall's floor near the scene column: `(column, floor row)` (the first solid row under
/// a wide stretch of open hall).
fn jungle_floor() -> (i32, i32) {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.rows(Stratum::MegafaunaJungle);
    for y in (top + 30)..(bottom - 30) {
        for step in 0..120 {
            let x = SCENE_X + if step % 2 == 0 { step } else { -step };
            let open = |cx: i32, cy: i32| places::jungle_open(&settings, &layout, cx, cy);
            if (-14..=14).all(|dx| open(x + dx, y - 1) && open(x + dx, y - 6) && !open(x + dx, y)) {
                return (x, y);
            }
        }
    }
    (SCENE_X, i32::midpoint(top, bottom))
}

/// The Sunfolk village: the jungle tribes' kraal in its valley, its people about their day.
fn sunfolk_village(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some((site, center)) =
            fatherload_simulation::worldgen::streamed::sunfolk_hall(&settings())
        else {
            return;
        };
        stage_at(world, player, (center, site.floor_row() - 1), true, false);
        reveal(
            world,
            player,
            center - 70..center + 70,
            site.top - 20..site.floor_row() + 4,
        );
    });
    stage.settle(260);
}

/// A pilot riding a crestback across a jungle hall.
fn ride(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let (x, floor) = jungle_floor();
        stage_at(world, player, (x + 6, floor - 1), true, false);
        let (px, feet) = world.frame().tile_center(TilePosition {
            x: x - 2,
            y: floor - 1,
        });
        let feet = feet + fatherload_simulation::TILE_SIZE * 0.5;
        let _ = world.place_pilot(player, px, feet - 0.25);
        spawn(world, EnemyKind::Crestback, x - 2, floor - 1);
        spawn(world, EnemyKind::Crestback, x + 12, floor - 1);
        spawn(world, EnemyKind::Skyneck, x - 16, floor - 2);
    });
    stage.settle(4);
    stage.play_on(|world| {
        let Some(player) = world.player_ids().next() else {
            return;
        };
        let mut output = fatherload_simulation::TickOutput::default();
        let _ = world.scene_pilot_use(player, &mut output);
    });
    // Ride to the right.
    stage.hold(
        40,
        crate::input::PlayerInput {
            horizontal: 1.0,
            ..crate::input::PlayerInput::default()
        },
    );
    stage.settle(4);
}

/// An isle hanging in the isle sea near the scene column.
fn isle() -> Option<places::Isle> {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.rows(Stratum::UpsideDownIsles);
    places::isles_touching(
        &settings,
        &layout,
        &TileRect {
            min: TilePosition {
                x: SCENE_X - 400,
                y: top,
            },
            max: TilePosition {
                x: SCENE_X + 400,
                y: bottom,
            },
        },
    )
    .into_iter()
    .filter(|isle| isle.bounds.width() >= 16)
    .min_by_key(|isle| (isle.bounds.center().x - SCENE_X).abs())
}

/// The isle sea: the rig flipped under an isle, about to land on its underside (or landed).
fn isles(stage: &mut SceneStage<'_>, landed: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(isle) = isle() else {
            stage_at(world, player, (SCENE_X, 7_000), true, false);
            return;
        };
        let centre = isle.bounds.min.x + isle.bounds.width() / 2;
        let below = if landed { 1 } else { 7 };
        stage_at(
            world,
            player,
            (centre, isle.underside() + below),
            true,
            false,
        );
        if let Some(rig) = world.player_mut(player) {
            // Turned half way round by its thrusters, feet to the isle overhead.
            rig.attitude.orientation = std::f32::consts::PI;
            rig.velocity_y = if landed { 0.0 } else { -90.0 };
        }
        spawn(world, EnemyKind::Hangclaw, centre + 5, isle.underside() + 1);
        spawn(world, EnemyKind::LodeMite, centre - 9, isle.underside() + 6);
        spawn(
            world,
            EnemyKind::LodeMite,
            centre - 10,
            isle.underside() + 7,
        );
    });
    stage.settle(if landed { 90 } else { 18 });
}

/// A clear spot of the forge sky near the scene column with a floating stone below it (shade)
/// or none (glare).
fn forge_spot(shaded: bool) -> (i32, i32) {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.rows(Stratum::TheSunforge);
    for y in (top + 40)..(bottom - 80) {
        for step in 0..160 {
            let x = SCENE_X + if step % 2 == 0 { step } else { -step };
            if !places::forge_open(&settings, &layout, x, y) {
                continue;
            }
            let stone_below = (2..12).any(|dy| !places::forge_open(&settings, &layout, x, y + dy));
            let clear_below = (1..40).all(|dy| places::forge_open(&settings, &layout, x, y + dy));
            if (shaded && stone_below) || (!shaded && clear_below) {
                return (x, y);
            }
        }
    }
    (SCENE_X, i32::midpoint(top, bottom))
}

/// The Sunforge: the rig in the glare (exposure blooming the screen) with cinder drakes,
/// flareflies and a shade stalker in a stone's shadow.
fn sunforge(stage: &mut SceneStage<'_>, shaded: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let (x, y) = forge_spot(shaded);
        stage_at(world, player, (x, y), true, false);
        if !shaded {
            world.set_hollow_exposure(player, 900);
        }
        spawn(world, EnemyKind::Flarefly, x + 6, y - 2);
        spawn(world, EnemyKind::Flarefly, x + 8, y);
        spawn(world, EnemyKind::CinderDrake, x - 10, y + 2);
        spawn(world, EnemyKind::ShadeStalker, x + 14, y + 3);
    });
    stage.settle(120);
}

/// The Gravity Anchor: the rig turned feet to a wall, the anchor holding it there.
fn gravity_anchor(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let (x, floor) = jungle_floor();
        stage_at(world, player, (x, floor - 3), true, false);
        fit(world, player, true);
        // A wall of loam to the rig's left to walk up.
        for y in floor - 12..floor {
            for dx in -6..=-2 {
                let _ = world.terrain_mut().set_kind(
                    TilePosition { x: x + dx, y },
                    TileKind::Hollow(fatherload_simulation::hollow::HollowTile::Loam),
                );
            }
        }
        place_rig(world, player, x - 1, floor - 5);
        if let Some(rig) = world.player_mut(player) {
            rig.attitude.orientation = std::f32::consts::FRAC_PI_2;
        }
    });
    stage.play_on(|world| {
        let Some(player) = world.player_ids().next() else {
            return;
        };
        let _ = world.scene_engage_anchor(player);
    });
    stage.settle(60);
}

/// The Sun Serpent's fight in the Sun Well: it orbits the inner sun, its sun scales blazing;
/// with `flare`, the inner sun burns and the pillars' shadows show where to hide.
fn sun_serpent(stage: &mut SceneStage<'_>, flare: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(centre) = places::sun_well(&settings, &layout) else {
            return;
        };
        let pillar = places::shade_pillars(centre)[if flare { 0 } else { 2 }];
        stage_at(world, player, (pillar.x - 3, pillar.y - 3), true, false);
        reveal(
            world,
            player,
            centre.x - 30..centre.x + 30,
            centre.y - 30..centre.y + 30,
        );
    });
    // It wakes on the first tick a rig is in the well, then orbits.
    stage.settle(if flare { 470 } else { 160 });
}

pub const HOLLOW_SCENES: &[Scene] = &[
    Scene {
        name: "hollow_breakthrough",
        summary: "Descent V, the breakthrough: the rig breaks through the rock sky into the Hollow World; the light floods in, the inner sun hangs low, the banner and the sting",
        setup: breakthrough,
    },
    Scene {
        name: "hollow_inner_sky",
        summary: "The Inner Sky by day: a giant cavern under a rock sky, floating stones, the inner sun, sky mantas on the wind, a stoop roc and gust bladders",
        setup: |stage| inner_sky(stage, true),
    },
    Scene {
        name: "hollow_inner_night",
        summary: "The Inner Sky at inner night: the sun sunk to an ember, the far side's lights, a manta adrift",
        setup: |stage| inner_sky(stage, false),
    },
    Scene {
        name: "hollow_breach",
        summary: "The Fathers' Breach: their winch platform hanging under the Inner Sky's rock sky",
        setup: breach,
    },
    Scene {
        name: "hollow_jungle",
        summary: "The Megafauna Jungle: a jungle hall of giant trees, crestbacks grazing, a skyneck and a fangrunner pack",
        setup: |stage| jungle(stage, true),
    },
    Scene {
        name: "hollow_jungle_night",
        summary: "The jungle at inner night: the crestbacks asleep, the fangrunners hunting",
        setup: |stage| jungle(stage, false),
    },
    Scene {
        name: "hollow_sunfolk_village",
        summary: "The Sunfolk Kraal: the jungle tribes' village in its valley, its people about their day",
        setup: sunfolk_village,
    },
    Scene {
        name: "hollow_ride",
        summary: "Riding a crestback: the pilot in its saddle, running across a jungle hall",
        setup: ride,
    },
    Scene {
        name: "hollow_isles",
        summary: "The Upside-Down Isles: the rig turned by its thrusters, falling up toward an isle's underside where gravity points up; a hangclaw and lode mites",
        setup: |stage| isles(stage, false),
    },
    Scene {
        name: "hollow_upside_down_landing",
        summary: "Landed upside down: the rig standing on an isle's underside, its grass hanging down",
        setup: |stage| isles(stage, true),
    },
    Scene {
        name: "hollow_sunforge",
        summary: "The Sunforge in the glare: the screen blooms with exposure; flareflies, a cinder drake and a shade stalker",
        setup: |stage| sunforge(stage, false),
    },
    Scene {
        name: "hollow_sunforge_shade",
        summary: "The Sunforge's shade: the rig resting in a floating stone's shadow, cooling",
        setup: |stage| sunforge(stage, true),
    },
    Scene {
        name: "hollow_gravity_anchor",
        summary: "The Gravity Anchor: the rig turned feet to a wall, the anchor holding it there",
        setup: gravity_anchor,
    },
    Scene {
        name: "hollow_sun_serpent",
        summary: "The Sun Serpent: the great serpent orbiting the inner sun in the Sun Well, its four sun scales blazing",
        setup: |stage| sun_serpent(stage, false),
    },
    Scene {
        name: "hollow_serpent_flare",
        summary: "The inner sun flares: rays across the Sun Well, the rig sheltering in a pillar's long shadow",
        setup: |stage| sun_serpent(stage, true),
    },
];
