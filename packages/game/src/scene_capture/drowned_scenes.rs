//! The Drowned and Frozen's scenes (Wave 9, Descent IV of a Deep Descent world): every stratum
//! with its creatures and its signature rule at work (the submarine in a basin's gyre and its
//! sonar ping, the eel-dragon, the drowned builder sub; ice galleries, a frosted rig, the
//! frozen figures and a thermal vent; a geyser erupting and Old Faithful; Merrow City and its
//! moon pool; the Pearl Abyss's pressure darkening the screen) and the Leviathan's fight (its
//! roar before a charge, the maelstrom).

use fatherload_simulation::{
    PlayerId, WorldState,
    depth_progression::{ProgressionRules, RigSystem},
    drowned::{GeyserPhase, LeviathanMove, RigWater, geyser_phase},
    enemy::{EnemyDifficulty, EnemyKind},
    position::WorldPos,
    terrain::{Terrain, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{
            self,
            drowned::{self, DrownedStamp, DrownedStampKind},
            landmarks,
        },
    },
};

use super::{Scene, SceneStage, reveal};

/// Seed of every Drowned and Frozen scene world (the Deep Descent scenes' seed).
const SCENE_SEED: u64 = 0x0D3E_9DE5;
/// Column the scenes look around.
const SCENE_X: i32 = 120;
/// Scene frames a second (the stage's fixed frame).
const FPS: u64 = 60;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// Swaps the staged world's terrain for the scene world, with the Descent rules and the gates
/// above Descent IV already won.
fn deep_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
    world.set_progression_rules(ProgressionRules::Descent);
    world.install_logistics_sites(&landmarks::logistics_sites(&settings()), false);
    world.grant_deep_claim();
}

/// Fits the rig for Descent IV: every protection and the gate systems up to the Bio-Filter,
/// not the Leviathan's rewards (so the frost and the pressure show).
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 14;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(14);
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.engine_level = 12;
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.hazard_gear.rig_systems &= !(RigSystem::SonarSuite.bit() | RigSystem::CryoHull.bit());
        rig.hazard_gear.pressure_rating = 3;
        rig.radiator_level = 15;
        rig.hull_level = 15;
        rig.hull = rig.max_hull();
        rig.credits = 60_000;
    }
}

/// The stamp of `kind` nearest the scene column (`None` if this world has none in reach).
fn stamp(kind: DrownedStampKind) -> Option<DrownedStamp> {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) =
        layout.descent_rows(fatherload_simulation::descent::Descent::DrownedAndFrozen);
    let area = TileRect {
        min: TilePosition { x: -700, y: top },
        max: TilePosition { x: 900, y: bottom },
    };
    drowned::stamps_with(&settings, &layout, &area)
        .into_iter()
        .filter(|found| found.kind == kind)
        .min_by_key(|found| (found.bounds.center().x - SCENE_X).abs() + found.bounds.min.y / 8)
}

fn spawn(world: &mut WorldState, kind: EnemyKind, x: i32, y: i32) {
    let (px, py) = world.frame().tile_center(TilePosition { x, y });
    let _ = world.spawn_enemy(kind, px, py);
}

/// Puts the rig at the centre of tile `(x, y)` without carving it (it may be under water).
fn put_rig(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    world.teleport_rig_to(player, WorldPos::tile_center(i64::from(x), i64::from(y)));
    if let Some(rig) = world.player_mut(player) {
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
}

/// The rig at tile `(x, y)`, the land round it explored and streamed in.
fn stage_at(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    deep_world(world);
    fit(world, player);
    put_rig(world, player, x, y);
    reveal(world, player, x - 60..x + 60, y - 36..y + 36);
    world.stream_terrain_near_players();
}

/// The Underground Ocean: the submarine hovering in a basin's gyre among kelp, a reef shark
/// circling and an ink squid (a sonar ping just sent when `sonar`).
fn ocean_basin(stage: &mut SceneStage<'_>, sonar: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(basin) = stamp(DrownedStampKind::Basin) else {
            return;
        };
        let middle = basin.bounds.center();
        stage_at(world, player, middle.x - 4, middle.y);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        spawn(world, EnemyKind::ReefShark, middle.x + 6, middle.y - 2);
        spawn(world, EnemyKind::InkSquid, middle.x - 10, middle.y + 2);
        spawn(world, EnemyKind::LureFish, middle.x + 2, middle.y + 4);
    });
    stage.settle(150);
    if sonar {
        stage.play_on(|world| {
            let player = world.players().keys().next().copied();
            if let Some(player) = player {
                let mut output = fatherload_simulation::TickOutput::default();
                let _ = world.sonar_ping(player, &mut output);
            }
        });
        stage.settle(40);
    } else {
        stage.settle(90);
    }
}

/// The eel-dragon awake in its grotto, crackling before a pulse.
fn eel_dragon(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(grotto) = drowned::eel_grotto(&settings, &layout) else {
            return;
        };
        let middle = grotto.center();
        stage_at(world, player, middle.x - 12, middle.y + 3);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
    });
    // It wakes as the rig comes near; its first pulse's tell comes a few seconds on.
    stage.settle(5);
    let wait = {
        let world = stage.world();
        let eel = world.drowned().eel;
        let now = world.simulation_tick().get();
        usize::try_from(eel.next_pulse.get().saturating_sub(now) + 10).unwrap_or(0)
    };
    stage.settle(wait.min(600));
}

/// The drowned builder sub on the seabed, its hatch facing the cable (the story card).
fn drowned_sub(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(basin) = drowned::drowned_sub(&settings, &layout) else {
            return;
        };
        let hull = drowned::wreck_hull(&basin);
        stage_at(world, player, hull.max.x + 3, hull.max.y - 1);
    });
    stage.settle(60);
    stage.play_on(|world| {
        let settings = *world.terrain().settings();
        let layout = descent::layout(&settings);
        if let Some(basin) = drowned::drowned_sub(&settings, &layout) {
            let hull = drowned::wreck_hull(&basin);
            let player = world.players().keys().next().copied();
            if let Some(player) = player {
                put_rig(world, player, hull.max.x - 2, hull.max.y - 1);
            }
        }
    });
    stage.settle(30);
}

/// Brine Glaciers: an ice gallery, the rig frosted white, frost mites and a tusk seal.
fn glaciers(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(gallery) = stamp(DrownedStampKind::IceGallery) else {
            return;
        };
        let middle = gallery.bounds.center();
        let floor = middle.y + gallery.bounds.height() / 4;
        stage_at(world, player, middle.x - 6, floor);
        world.set_rig_water(
            player,
            RigWater {
                ballast: 0,
                frost_micro: 720_000,
            },
        );
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        spawn(world, EnemyKind::FrostMite, middle.x + 3, middle.y - 2);
        spawn(world, EnemyKind::TuskSeal, middle.x + 8, floor);
    });
    stage.settle(160);
}

/// The builder figures frozen in clear ice round the cable (the story card).
fn frozen_figures(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(gallery) = drowned::frozen_figures(&settings, &layout) else {
            return;
        };
        stage_at(world, player, gallery.min.x + 8, gallery.max.y - 4);
    });
    stage.settle(90);
}

/// A thermal vent's grotto: warm amber light, the frosted rig thawing.
fn vent_grotto(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(grotto) = stamp(DrownedStampKind::VentGrotto) else {
            return;
        };
        let middle = grotto.bounds.center();
        stage_at(world, player, middle.x - 2, middle.y);
        world.set_rig_water(
            player,
            RigWater {
                ballast: 0,
                frost_micro: 500_000,
            },
        );
    });
    stage.settle(70);
}

/// A geyser chimney erupting: the steam column throwing the rig up the shaft, a scald beetle
/// riding it, a sinter snail on the floor.
fn geyser(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(chimney) = stamp(DrownedStampKind::Chimney) else {
            return;
        };
        let (vent_row, first, _) = chimney.vents().unwrap_or((chimney.bounds.max.y, 0, 0));
        let cx = first + 1;
        stage_at(world, player, cx, vent_row - 5);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        spawn(world, EnemyKind::ScaldBeetle, cx - 3, vent_row - 1);
        spawn(world, EnemyKind::SinterSnail, cx + 3, vent_row - 1);
    });
    // Until the vent blows (its beat is world time), then a moment into the eruption.
    let wait = {
        let world = stage.world();
        stamp(DrownedStampKind::Chimney).map_or(0, |chimney| {
            let (_, first, _) = chimney.vents().unwrap_or((0, 0, 0));
            let seed = world.terrain().settings().seed;
            let now = world.millis_at(world.simulation_tick());
            (0..20_000_u64)
                .step_by(16)
                .find(|ahead| {
                    geyser_phase(seed, first + 1, false, now + ahead) == GeyserPhase::Erupting
                })
                .map_or(0, |ahead| usize::try_from(ahead * FPS / 1_000).unwrap_or(0))
        })
    };
    stage.settle(wait + 18);
}

/// Old Faithful: the great geyser's shaft from its throat chamber, mid-eruption.
fn old_faithful(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(geyser) = drowned::old_faithful(&settings, &layout) else {
            return;
        };
        stage_at(world, player, geyser.min.x + 3, geyser.max.y - 2);
    });
    let wait = {
        let world = stage.world();
        let settings = settings();
        let layout = descent::layout(&settings);
        drowned::old_faithful(&settings, &layout).map_or(0, |geyser| {
            let column = geyser.min.x + geyser.width() / 2;
            let seed = world.terrain().settings().seed;
            let now = world.millis_at(world.simulation_tick());
            (0..20_000_u64)
                .step_by(16)
                .find(|ahead| {
                    geyser_phase(seed, column, true, now + ahead) == GeyserPhase::Erupting
                })
                .map_or(0, |ahead| usize::try_from(ahead * FPS / 1_000).unwrap_or(0))
        })
    };
    stage.settle(wait + 24);
}

/// Merrow City under its bubble: the merrow's hall and the outpost, the sea beyond the glass
/// (the arrival card when `card`).
fn merrow_city(stage: &mut SceneStage<'_>, card: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(site) = drowned::merrow_city(&settings()) else {
            return;
        };
        stage_at(world, player, site.hall_center + 6, site.floor - 1);
        reveal(
            world,
            player,
            site.dome.min.x - 12..site.dome.max.x + 12,
            site.dome.min.y - 12..site.floor + 14,
        );
    });
    stage.settle(if card { 40 } else { 300 });
}

/// The submarine surfacing into Merrow City through a moon pool from the harbour below.
fn moon_pool(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(site) = drowned::merrow_city(&settings()) else {
            return;
        };
        let pool = site.moon_pools()[0];
        stage_at(world, player, pool + 1, site.floor + 4);
        if let Some(rig) = world.player_mut(player) {
            rig.velocity_y = -120.0;
        }
        world.set_rig_water(
            player,
            RigWater {
                ballast: 900,
                frost_micro: 0,
            },
        );
    });
    stage.settle(50);
}

/// The Pearl Abyss: a pearl bed of gaping clams in black water, a lure angler's light and a
/// gulper eel; the pressure darkening and greying the screen.
fn pearl_abyss(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(bed) = stamp(DrownedStampKind::PearlBed) else {
            return;
        };
        let middle = bed.bounds.center();
        stage_at(world, player, middle.x - 6, middle.y + 1);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        spawn(world, EnemyKind::LureAngler, middle.x + 7, middle.y - 1);
        spawn(world, EnemyKind::GulperEel, middle.x - 12, middle.y - 3);
    });
    stage.settle(160);
}

/// Descent IV's gate: the Leviathan awake in its flooded arena (its roar and the charge's red
/// line when `roar`, its open maw in the maelstrom when `maw`).
fn leviathan(stage: &mut SceneStage<'_>, roar: bool, maw: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(arena) = drowned::leviathan_arena(&settings, &layout) else {
            return;
        };
        let middle = arena.center();
        stage_at(
            world,
            player,
            middle.x - 14,
            drowned::arena_floor_row(&arena) - 5,
        );
        reveal(
            world,
            player,
            arena.min.x - 6..arena.max.x + 6,
            arena.min.y - 6..arena.max.y + 6,
        );
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
    });
    stage.settle(90);
    if roar || maw {
        stage.play_on(move |world| {
            let mut drowned = world.drowned().clone();
            let now = world.simulation_tick();
            if maw {
                drowned.leviathan.phase = fatherload_simulation::drowned::LeviathanPhase::Maelstrom;
                drowned.leviathan.action = LeviathanMove::Maw;
            } else {
                let player = world.players().values().next().map(|rig| (rig.x, rig.y));
                let settings = *world.terrain().settings();
                let layout = descent::layout(&settings);
                if let (Some((px, py)), Some(arena)) =
                    (player, drowned::leviathan_arena(&settings, &layout))
                {
                    let centre = world.frame().tile_center(arena.center());
                    #[allow(clippy::cast_possible_truncation, reason = "pixel offsets")]
                    let target = ((px - centre.0) as i32, (py - centre.1) as i32);
                    let at = world.leviathan_pose().map_or((0, 0), |((x, y), _)| {
                        #[allow(clippy::cast_possible_truncation, reason = "pixel offsets")]
                        let offset = ((x - centre.0) as i32, (y - centre.1) as i32);
                        offset
                    });
                    drowned.leviathan.action = LeviathanMove::ChargeTell { at, target };
                }
            }
            drowned.leviathan.since = now;
            world.set_drowned(drowned);
        });
        stage.settle(if maw { 40 } else { 20 });
    } else {
        stage.settle(60);
    }
}

pub const DROWNED_SCENES: &[Scene] = &[
    Scene {
        name: "drowned_ocean_basin",
        summary: "Descent IV, the Underground Ocean: the submarine hovering in a basin's gyre among kelp and coral, a reef shark circling, an ink squid",
        setup: |stage| ocean_basin(stage, false),
    },
    Scene {
        name: "drowned_sonar",
        summary: "The submarine's sonar ping: a ring sweeping the water, lighting the rock and echoing off the creatures",
        setup: |stage| ocean_basin(stage, true),
    },
    Scene {
        name: "drowned_eel_dragon",
        summary: "The Underground Ocean's mini-boss: the eel-dragon in its grotto, crackling before its electric pulse",
        setup: eel_dragon,
    },
    Scene {
        name: "drowned_builder_sub",
        summary: "The drowned builder sub on the seabed, its hatch facing the cable, and the story it tells",
        setup: drowned_sub,
    },
    Scene {
        name: "drowned_glaciers",
        summary: "Brine Glaciers: an ice gallery of pack ice and slick brine ice, the rig white with hull frost, frost mites and a tusk seal",
        setup: glaciers,
    },
    Scene {
        name: "drowned_frozen_figures",
        summary: "The builders' figures frozen in clear ice round the cable",
        setup: frozen_figures,
    },
    Scene {
        name: "drowned_thermal_vent",
        summary: "A thermal vent's grotto in the ice: warm light, the frosted rig thawing",
        setup: vent_grotto,
    },
    Scene {
        name: "drowned_geyser",
        summary: "Geyser Fields: a chimney erupting, the steam column throwing the rig up the shaft; a scald beetle and a sinter snail",
        setup: geyser,
    },
    Scene {
        name: "drowned_old_faithful",
        summary: "Old Faithful: the great geyser's 60-row shaft from its throat chamber, mid-eruption",
        setup: old_faithful,
    },
    Scene {
        name: "drowned_merrow_city",
        summary: "Merrow City: the fish-folk's settlement and the Descent IV outpost under their glass bubble, the sea and its fish beyond",
        setup: |stage| merrow_city(stage, false),
    },
    Scene {
        name: "drowned_merrow_arrival",
        summary: "Arriving in Merrow City: the merrow remember the Fathers, and Sefa",
        setup: |stage| merrow_city(stage, true),
    },
    Scene {
        name: "drowned_moon_pool",
        summary: "The submarine surfacing into Merrow City through a moon pool from the harbour under the streets",
        setup: moon_pool,
    },
    Scene {
        name: "drowned_pearl_abyss",
        summary: "The Pearl Abyss: a pearl bed of gaping clams, a lure angler's light and a gulper eel; the pressure darkening and greying the screen",
        setup: pearl_abyss,
    },
    Scene {
        name: "drowned_leviathan",
        summary: "Descent IV's gate: the Leviathan circling its flooded arena, fought from the submarine",
        setup: |stage| leviathan(stage, false, false),
    },
    Scene {
        name: "drowned_leviathan_roar",
        summary: "The Leviathan's telegraph: its eyes flare, it roars, and the charge's path shows in red",
        setup: |stage| leviathan(stage, true, false),
    },
    Scene {
        name: "drowned_leviathan_maelstrom",
        summary: "The Leviathan's maelstrom: sunk to the floor, its maw agape, the whirlpool pulling the submarine in",
        setup: |stage| leviathan(stage, false, true),
    },
];
