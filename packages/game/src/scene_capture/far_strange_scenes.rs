//! The far Strange Layers' scenes (Wave 9 strange-b, Descent VI strata 4-6 and its gate): each
//! stratum's set piece with its creatures and its rule at work (a slow pool and a fast lane, a
//! time bubble and amber, the Echo Caves' dark before and after a ping, the Choir, a keyed
//! curtain opening to its light and a spectrum vault), and every rule of The Glitch's fight.

use fatherload_simulation::{
    PlayerId, WorldState,
    depth_progression::{ProgressionRules, RigSystem},
    enemy::EnemyKind,
    explosive::ExplosiveKind,
    far_strange::{GlitchRule, LampFilter, TimePocketKind},
    hazard::PlacedBomb,
    terrain::{Terrain, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{
            self,
            far_strange::{self as sites, FarStamp, FarStampKind},
            landmarks,
        },
    },
};

use super::{Scene, SceneStage, place_rig, reveal};

/// Seed of every far Strange Layers scene world (the Deep Descent scenes' seed).
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
}

/// Fits the rig for Descent VI (every gate system to the Gravity Anchor; the Phase Drill not
/// yet).
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 14;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(14);
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.engine_level = 14;
        rig.drill_strength = 15;
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.hazard_gear.rig_systems &= !(RigSystem::PhaseDrill.bit() | RigSystem::HeartKey.bit());
        rig.radiator_level = 15;
        rig.hull_level = 15;
        rig.hull = rig.max_hull();
        rig.credits = 400_000;
        rig.add_explosives(ExplosiveKind::TimeBubble, 3);
    }
}

/// The stamp of `kind` nearest the scene column.
fn stamp(kind: FarStampKind) -> Option<FarStamp> {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = sites::carve_rows(&layout);
    let area = TileRect {
        min: TilePosition { x: -700, y: top },
        max: TilePosition { x: 900, y: bottom },
    };
    sites::stamps_with(&settings, &layout, &area)
        .into_iter()
        .filter(|found| found.kind == kind)
        .min_by_key(|found| (found.bounds.center().x - SCENE_X).abs())
}

fn spawn(world: &mut WorldState, kind: EnemyKind, x: i32, y: i32) {
    let (px, py) = world.frame().tile_center(TilePosition { x, y });
    let _ = world.spawn_enemy(kind, px, py);
}

/// The rig at tile `(x, y)`, the land around it explored.
fn stage_at(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    deep_world(world);
    fit(world, player);
    place_rig(world, player, x, y);
    reveal(world, player, x - 56..x + 56, y - 34..y + 34);
    world.stream_terrain_near_players();
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
    // The landmarks' story cards are their own scene (the clock hall's): found already here.
    world.set_far_strange(fatherload_simulation::far_strange::FarStrange {
        found: fatherload_simulation::far_strange::FarLandmark::ALL
            .into_iter()
            .collect(),
        ..world.far_strange().clone()
    });
}

/// Whether tile `(x, y)` is open.
fn open(world: &WorldState, x: i32, y: i32) -> bool {
    world
        .terrain()
        .tile(TilePosition { x, y })
        .is_some_and(|tile| tile.kind.is_open())
}

/// The lowest open row of column `x` inside `bounds` (a floor to stand on).
fn floor_in(world: &WorldState, bounds: &TileRect, x: i32) -> i32 {
    // From the middle: up out of any rock, then down to the floor of that cavity.
    let mut y = bounds.center().y;
    while !open(world, x, y) && y > bounds.min.y {
        y -= 1;
    }
    while open(world, x, y + 1) && y < bounds.max.y {
        y += 1;
    }
    y
}

/// A slow pool (or a fast lane) of the Time Sludge, its creatures about: tick mites, an amber
/// sloth and a rewind newt.
fn time_pocket(stage: &mut SceneStage<'_>, kind: TimePocketKind) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(pocket) = stamp(FarStampKind::TimePocket(kind)) else {
            return;
        };
        let middle = pocket.bounds.center();
        stage_at(world, player, middle.x, middle.y);
        let floor = floor_in(world, &pocket.bounds, middle.x);
        place_rig(world, player, middle.x - 3, floor);
        if kind == TimePocketKind::Fast
            && let Some(rig) = world.player_mut(player)
        {
            rig.velocity_x = 260.0;
        }
        let x = middle.x;
        let floor_right = floor_in(world, &pocket.bounds, x + 5);
        spawn(world, EnemyKind::TickMite, x + 4, floor_right);
        spawn(world, EnemyKind::TickMite, x + 6, floor_right);
        spawn(
            world,
            EnemyKind::RewindNewt,
            x - 7,
            floor_in(world, &pocket.bounds, x - 7),
        );
        if kind == TimePocketKind::Slow {
            spawn(
                world,
                EnemyKind::AmberSloth,
                x + 8,
                floor_in(world, &pocket.bounds, x + 8),
            );
        }
    });
    stage.settle(260);
}

/// The Stopped Clock: its striped hall, its handless dial, and its story card.
fn clock_hall(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(hall) = sites::clock_hall(&settings, &layout) else {
            return;
        };
        stage_at(world, player, hall.center().x, hall.max.y - 2);
        // This scene shows the clock hall's story card.
        world.set_far_strange(fatherload_simulation::far_strange::FarStrange::default());
        reveal(
            world,
            player,
            hall.min.x - 10..hall.max.x + 10,
            hall.min.y - 10..hall.max.y + 10,
        );
    });
    stage.settle(260);
}

/// A time bubble slowing creatures in it, and a rig held in amber by a sloth's touch.
fn time_bubble(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(pocket) = stamp(FarStampKind::TimePocket(TimePocketKind::Slow)) else {
            return;
        };
        let middle = pocket.bounds.center();
        stage_at(world, player, middle.x, middle.y);
        let floor = floor_in(world, &pocket.bounds, middle.x);
        place_rig(world, player, middle.x - 4, floor);
        spawn(world, EnemyKind::TickMite, middle.x + 3, floor);
        spawn(world, EnemyKind::TickMite, middle.x + 4, floor);
        spawn(world, EnemyKind::RewindNewt, middle.x + 6, floor);
        spawn(world, EnemyKind::AmberSloth, middle.x - 2, floor);
    });
    stage.settle(220);
    stage.play_on(|world| {
        let player = world.player_ids().next().unwrap_or(PlayerId::new(0));
        let Some(rig) = world.player(player) else {
            return;
        };
        let (x, y) = (
            4.0f32.mul_add(fatherload_simulation::terrain::TILE_SIZE, rig.x),
            rig.y - 8.0,
        );
        world.insert_charge(PlacedBomb {
            id: 77_001,
            x,
            y,
            previous_x: x,
            previous_y: y,
            kind: ExplosiveKind::TimeBubble,
            detonation_tick: world.simulation_tick().next(),
            owner: Some(player),
            ..PlacedBomb::default()
        });
    });
    stage.settle(60);
}

/// An echo hollow in total darkness (before a ping, when `ping` is false; the ping's wave
/// lighting it, a shrieker screaming and stalkers coming, when true).
fn echo_hollow(stage: &mut SceneStage<'_>, ping: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(hollow) = stamp(FarStampKind::EchoHollow) else {
            return;
        };
        let middle = hollow.bounds.center();
        stage_at(world, player, middle.x, middle.y);
        let floor = floor_in(world, &hollow.bounds, middle.x - 3);
        place_rig(world, player, middle.x - 3, floor);
        spawn(
            world,
            EnemyKind::EchoStalker,
            middle.x + 6,
            floor_in(world, &hollow.bounds, middle.x + 6),
        );
        spawn(world, EnemyKind::Shrieker, middle.x + 2, middle.y - 3);
        spawn(world, EnemyKind::HushMoth, middle.x - 9, middle.y - 2);
    });
    stage.settle(240);
    if ping {
        stage.play_on(|world| {
            let player = world.player_ids().next().unwrap_or(PlayerId::new(0));
            let tile = world
                .player(player)
                .map_or(TilePosition { x: 0, y: 0 }, |rig| {
                    rig.tile_in(world.frame())
                });
            world.stage_sonar_ping(Some(player), tile, 30);
        });
        stage.settle(22);
    } else {
        stage.settle(20);
    }
}

/// The Choir: a great hall of echo quartz, a pillar ringing and lighting the hall.
fn choir(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(hall) = sites::choir(&settings, &layout) else {
            return;
        };
        let middle = hall.center();
        stage_at(world, player, middle.x, middle.y);
        let floor = floor_in(world, &hall, middle.x + 1);
        place_rig(world, player, middle.x + 1, floor);
        reveal(
            world,
            player,
            hall.min.x - 10..hall.max.x + 10,
            hall.min.y - 10..hall.max.y + 10,
        );
    });
    stage.settle(240);
    // A pillar rings beside the rig: the hall lights as the wave runs out.
    stage.play_on(|world| {
        let player = world.player_ids().next().unwrap_or(PlayerId::new(0));
        if let Some(tile) = world.player(player).map(|rig| rig.tile_in(world.frame())) {
            world.stage_sonar_ping(
                None,
                TilePosition {
                    x: tile.x - 2,
                    y: tile.y - 2,
                },
                26,
            );
        }
    });
    stage.settle(36);
}

/// A keyed curtain in the Prism Desert: the rig's red lamp opening the red wall, a scarab and
/// a splitter spire (the walls shut, under white light, when `lit` is false).
fn keyed_curtain(stage: &mut SceneStage<'_>, lit: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(curtain) = stamp(FarStampKind::KeyedCurtain) else {
            return;
        };
        let middle = curtain.bounds.center();
        stage_at(world, player, middle.x, middle.y);
        // Stand two tiles short of the first keyed wall (the walls are on the seed's side).
        let side = if curtain.seed.is_multiple_of(2) {
            1
        } else {
            -1
        };
        let half = (curtain.bounds.max.x - curtain.bounds.min.x + 1) / 2;
        let wall_x = middle.x + side * (half / 3);
        let x = wall_x - side * 2;
        place_rig(world, player, x, floor_in(world, &curtain.bounds, x));
        if lit {
            // The first wall's colour.
            let wall = (curtain.bounds.min.x..=curtain.bounds.max.x)
                .flat_map(|cx| {
                    (curtain.bounds.min.y..=curtain.bounds.max.y).map(move |cy| (cx, cy))
                })
                .find_map(|(cx, cy)| {
                    world
                        .terrain()
                        .tile(TilePosition { x: cx, y: cy })
                        .and_then(|tile| match tile.kind {
                            fatherload_simulation::terrain::TileKind::Cave(cave) => {
                                LampFilter::of_wall(cave)
                            }
                            _ => None,
                        })
                })
                .unwrap_or(LampFilter::Red);
            world.set_lamp_filter(player, wall);
        }
        spawn(
            world,
            EnemyKind::PrismScarab,
            x - side * 5,
            floor_in(world, &curtain.bounds, x - side * 5),
        );
        spawn(
            world,
            EnemyKind::SplitterSpire,
            x - side * 8,
            floor_in(world, &curtain.bounds, x - side * 8),
        );
    });
    stage.settle(260);
}

/// A spectrum vault: three nested keyed walls, the rig's blue lamp at the inner ring and a lens
/// wraith showing in its blue light.
fn spectrum_vault(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(vault) = stamp(FarStampKind::SpectrumVault) else {
            return;
        };
        let b = vault.bounds;
        stage_at(world, player, b.center().x, b.center().y);
        // Outside the vault, beside its red ring: dig a pocket to stand in.
        let x = b.min.x - 2;
        let y = b.max.y - 2;
        for dx in -2..=1 {
            for dy in -2..=0 {
                let _ = world.terrain_mut().set_kind(
                    TilePosition {
                        x: x + dx,
                        y: y + dy,
                    },
                    fatherload_simulation::terrain::TileKind::Air,
                );
            }
        }
        place_rig(world, player, x, y);
        world.set_lamp_filter(player, LampFilter::Red);
        // A lens wraith that shows in this rig's light.
        for attempt in 0..12_u8 {
            let (px, py) = world
                .frame()
                .tile_center(TilePosition { x: x - 3, y: y - 3 });
            let id = world.spawn_enemy(EnemyKind::LensWraith, px, py - f32::from(attempt));
            if WorldState::lens_wraith_colour(id.get()) == LampFilter::Red {
                break;
            }
            world.entities_mut().despawn(id);
        }
    });
    stage.settle(260);
}

/// The Rewrite Hall: The Glitch awake on `rule`, the rig on the hall's floor (mid-rewrite,
/// telegraphing the next rule, when `rewriting`).
fn glitch(stage: &mut SceneStage<'_>, rule: GlitchRule, rewriting: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(arena) = sites::glitch_arena(&settings, &layout) else {
            return;
        };
        let inside = sites::arena_inside(&arena);
        stage_at(world, player, inside.min.x + 16, inside.max.y);
        reveal(
            world,
            player,
            arena.min.x - 10..arena.max.x + 10,
            arena.min.y - 10..arena.max.y + 10,
        );
        world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
    });
    // It wakes on the first tick a rig is in its hall; past the entering banner, the rule.
    stage.settle(240);
    stage.play_on(|world| {
        world.stage_glitch_rule(rule, rewriting);
        // Bring The Glitch into view: to one of its own blink spots, clear of the ledges,
        // up and to the left of the rig.
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(arena) = sites::glitch_arena(&settings, &layout) else {
            return;
        };
        let inside = sites::arena_inside(&arena);
        let (x, y) = world.frame().tile_center(TilePosition {
            x: inside.min.x + 8,
            y: inside.max.y - 5,
        });
        if let Some(id) = world.far_strange().glitch.entity {
            world
                .entities_mut()
                .update(fatherload_simulation::entity::EntityId(id), |entity| {
                    entity.body.x = x;
                    entity.body.y = y;
                    entity.body.previous_x = x;
                    entity.body.previous_y = y;
                    entity.body.velocity_x = 0.0;
                    entity.body.velocity_y = 0.0;
                });
        }
    });
    stage.settle(if rewriting { 40 } else { 90 });
}

pub const FAR_STRANGE_SCENES: &[Scene] = &[
    Scene {
        name: "far_time_slow_pool",
        summary: "Descent VI, the Time Sludge: a slow pool (time crawls, honey-gold, motes hanging still) with tick mites, an amber sloth and a rewind newt",
        setup: |stage| time_pocket(stage, TimePocketKind::Slow),
    },
    Scene {
        name: "far_time_fast_lane",
        summary: "The Time Sludge's fast lane: time races, blue-white streaks; the rig shoots along it",
        setup: |stage| time_pocket(stage, TimePocketKind::Fast),
    },
    Scene {
        name: "far_clock_hall",
        summary: "The Stopped Clock: the builders' hall striped slow and fast under a handless dial, and its story card",
        setup: clock_hall,
    },
    Scene {
        name: "far_time_bubble",
        summary: "A time bubble slowing tick mites and a newt to a crawl; an amber sloth reaching for the rig",
        setup: time_bubble,
    },
    Scene {
        name: "far_echo_dark",
        summary: "The Echo Caves: total darkness, the rig's lamp a pinprick, the hunters unseen",
        setup: |stage| echo_hollow(stage, false),
    },
    Scene {
        name: "far_echo_ping",
        summary: "A sonar ping in the Echo Caves: the wave runs out and lights the cave for a moment, an echo stalker, a shrieker and a hush moth caught in it",
        setup: |stage| echo_hollow(stage, true),
    },
    Scene {
        name: "far_choir",
        summary: "The Choir: a great hall of echo quartz pillars, one ringing and lighting the hall",
        setup: choir,
    },
    Scene {
        name: "far_prism_curtain",
        summary: "The Prism Desert: colours split; a keyed curtain shut under white light, a prism scarab and a splitter spire",
        setup: |stage| keyed_curtain(stage, false),
    },
    Scene {
        name: "far_prism_lit",
        summary: "The rig's lamp filtered to the curtain's colour: the keyed wall opens in its light",
        setup: |stage| keyed_curtain(stage, true),
    },
    Scene {
        name: "far_spectrum_vault",
        summary: "A spectrum vault: red, green and blue walls nested round a relic cache; a lens wraith showing in the rig's red light",
        setup: spectrum_vault,
    },
    Scene {
        name: "far_glitch_baseline",
        summary: "Descent VI's gate: The Glitch awake in the Rewrite Hall under the world's own rules",
        setup: |stage| glitch(stage, GlitchRule::Baseline, false),
    },
    Scene {
        name: "far_glitch_rewriting",
        summary: "The Glitch rewriting its hall: the telegraph (the hall flickers, it cannot attack or be hurt, the next rule shown)",
        setup: |stage| glitch(stage, GlitchRule::Baseline, true),
    },
    Scene {
        name: "far_glitch_gravity_up",
        summary: "The Glitch's second rule: gravity points up",
        setup: |stage| glitch(stage, GlitchRule::GravityUp, false),
    },
    Scene {
        name: "far_glitch_mirrored",
        summary: "The Glitch's third rule: screen and controls mirrored; shards shed",
        setup: |stage| glitch(stage, GlitchRule::Mirrored, false),
    },
    Scene {
        name: "far_glitch_time_warp",
        summary: "The Glitch's fourth rule: time races in the hall",
        setup: |stage| glitch(stage, GlitchRule::TimeWarp, false),
    },
    Scene {
        name: "far_glitch_sideways",
        summary: "The Glitch's last rule: gravity sideways and the picture rotting into big pixels",
        setup: |stage| glitch(stage, GlitchRule::Sideways, false),
    },
];
