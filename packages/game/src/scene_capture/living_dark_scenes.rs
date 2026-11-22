//! The Living Dark's scenes (Wave 8, Descent III of a Deep Descent world): every stratum's set
//! piece with its creatures, its signature rule at work (a bounce off a mushroom cap, a spore
//! cloud blurring the screen, roots closing a tunnel, the acid tide in a gullet, the Glow
//! Sea's creature-lit darkness with and without Lamp II), the Hive's two faces (the Amber
//! Court and its counter, a Swarm gallery and the brood matriarch) and the Mycelial Mind's
//! fight.

use fatherload_simulation::{
    PlayerId, WorldState,
    depth_progression::{ProgressionRules, RigSystem},
    enemy::EnemyKind,
    terrain::{Terrain, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{
            self, landmarks,
            living::{self, LivingStamp, LivingStampKind},
        },
    },
};

use super::{Scene, SceneStage, place_rig, reveal};
use crate::shell::ModalScreen;

/// Seed of every Living Dark scene world (the Deep Descent scenes' seed).
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
}

/// Fits the rig for the deep (every protection and gate system but the Mind's rewards, so the
/// spores and the dark show), or with them when `rewards`.
fn fit(world: &mut WorldState, player: PlayerId, rewards: bool) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 14;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(14);
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.engine_level = 12;
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        if !rewards {
            rig.hazard_gear.rig_systems &= !(RigSystem::BioFilter.bit() | RigSystem::LampII.bit());
        }
        rig.radiator_level = 15;
        rig.hull_level = 15;
        rig.hull = rig.max_hull();
        rig.credits = 60_000;
    }
}

/// The stamp of `kind` nearest the scene column.
fn stamp(kind: LivingStampKind) -> Option<LivingStamp> {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.descent_rows(fatherload_simulation::descent::Descent::LivingDark);
    let area = TileRect {
        min: TilePosition { x: -700, y: top },
        max: TilePosition { x: 900, y: bottom },
    };
    living::stamps_with(&settings, &layout, &area)
        .into_iter()
        .filter(|found| found.kind == kind)
        .min_by_key(|found| (found.bounds.center().x - SCENE_X).abs() + found.bounds.min.y / 8)
}

/// Spawns `kind` at tile `(x, y)` (through the world's frame, so it works after the frame has
/// followed the rig too).
fn spawn(world: &mut WorldState, kind: EnemyKind, x: i32, y: i32) {
    let (px, py) = world.frame().tile_center(TilePosition { x, y });
    let _ = world.spawn_enemy(kind, px, py);
}

/// The rig at tile `(x, y)` of a set piece, the land around it explored.
fn stage_at(world: &mut WorldState, player: PlayerId, x: i32, y: i32, rewards: bool) {
    deep_world(world);
    fit(world, player, rewards);
    place_rig(world, player, x, y);
    reveal(world, player, x - 56..x + 56, y - 34..y + 34);
    world.stream_terrain_near_players();
}

/// A cap grove: the rig bouncing off a giant mushroom's cap, a spore puffer, a cap crawler and
/// glow gnats about the grove.
fn fungal_grove(stage: &mut SceneStage<'_>, spores: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(grove) = stamp(LivingStampKind::CapGrove) else {
            stage_at(world, player, SCENE_X, 3_000, false);
            return;
        };
        let middle = grove.bounds.center();
        stage_at(world, player, middle.x, middle.y - 2, false);
        if let Some(rig) = world.player_mut(player) {
            rig.velocity_y = -320.0;
        }
        let floor = grove.bounds.max.y - grove.bounds.height() / 5;
        spawn(world, EnemyKind::SporePuffer, middle.x - 8, floor - 1);
        spawn(world, EnemyKind::CapCrawler, middle.x + 7, floor - 1);
        spawn(world, EnemyKind::GlowGnat, middle.x + 3, middle.y - 4);
        if spores {
            // On the grove's floor where the rig comes down, and over the puffer.
            world.release_spore_cloud(TilePosition {
                x: middle.x,
                y: floor - 1,
            });
            world.release_spore_cloud(TilePosition {
                x: middle.x - 8,
                y: floor - 2,
            });
        }
    });
    // Past the "entering" banner, so the place and its effects show.
    stage.settle(250);
}

/// The Root Labyrinth: a heartroot chamber, the tunnel dug into it closing with new roots, a
/// root snapper waking and a thorn wisp.
fn root_labyrinth(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(heart) = stamp(LivingStampKind::Heartroot) else {
            stage_at(world, player, SCENE_X, 3_400, false);
            return;
        };
        let middle = heart.bounds.center();
        let floor = heart.bounds.max.y - 3;
        let x = heart.bounds.min.x;
        stage_at(world, player, x, floor, false);
        // The tunnel the rig came in by, its far end already grown shut and new roots
        // reaching into the rest.
        let tunnel_y = floor;
        let start = heart.bounds.min.x - 14;
        world.dig_living_tiles(TileRect {
            min: TilePosition {
                x: start,
                y: tunnel_y - 1,
            },
            max: TilePosition {
                x: heart.bounds.min.x + 2,
                y: tunnel_y,
            },
        });
        let root = fatherload_simulation::terrain::TileKind::Cave(
            fatherload_simulation::cave::CaveTile::Root,
        );
        for column in start..start + 10 {
            // Shut near the far end, thinning toward the rig.
            let grown = column < start + 5 || (column - start) % 2 == 0;
            if grown {
                world.terrain_mut().set_kind(
                    TilePosition {
                        x: column,
                        y: tunnel_y - i32::from(column >= start + 5),
                    },
                    root,
                );
                if column < start + 5 {
                    world.terrain_mut().set_kind(
                        TilePosition {
                            x: column,
                            y: tunnel_y - 1,
                        },
                        root,
                    );
                }
            }
        }
        spawn(world, EnemyKind::RootSnapper, middle.x + 5, floor);
        spawn(world, EnemyKind::ThornWisp, middle.x - 3, middle.y - 4);
        spawn(world, EnemyKind::BarkBeetle, middle.x + 8, middle.y - 2);
        reveal(
            world,
            player,
            start - 10..heart.bounds.max.x + 30,
            floor - 30..floor + 20,
        );
    });
    stage.settle(250);
}

/// The Amber Court: the rig in the friendly hall among the Court's drones (with the counter
/// open when `counter`).
fn hive_court(stage: &mut SceneStage<'_>, counter: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(hall) = living::court_hall(&settings, &layout) else {
            return;
        };
        let floor = hall.max.y - 2;
        let middle = hall.center();
        stage_at(world, player, middle.x - 4, floor, false);
        for (dx, dy) in [(-12, -3), (-7, -5), (5, -2), (10, -4)] {
            spawn(world, EnemyKind::HiveDrone, middle.x + dx, floor + dy);
        }
    });
    if counter {
        let seat = stage.seat(0);
        seat.modal = Some(ModalScreen::Logistics);
        seat.selected_menu_item = 1;
        stage.settle(30);
    } else {
        stage.settle(250);
    }
}

/// The Swarm: a brood gallery's soldiers and a lancer on the rig.
fn hive_swarm(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(gallery) = stamp(LivingStampKind::BroodGallery) else {
            return;
        };
        let middle = gallery.bounds.center();
        let floor = gallery.bounds.max.y - 2;
        stage_at(world, player, middle.x - 6, floor, false);
    });
    // Past the banner, then the Swarm comes for the rig.
    stage.settle(250);
    let Some(gallery) = stamp(LivingStampKind::BroodGallery) else {
        return;
    };
    let middle = gallery.bounds.center();
    let floor = gallery.bounds.max.y - 2;
    stage.play_on(|world| {
        for (dx, dy) in [(-2, -2), (0, -3), (2, -1), (-1, -4)] {
            spawn(world, EnemyKind::HiveSoldier, middle.x + dx, floor + dy);
        }
        spawn(world, EnemyKind::AcidLancer, middle.x + 5, floor);
    });
    stage.settle(8);
}

/// The brood matriarch in her gallery, breeding soldiers.
fn hive_matriarch(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(gallery) = living::matriarch_gallery(&settings, &layout) else {
            return;
        };
        let floor = gallery.max.y - 2;
        stage_at(world, player, gallery.center().x - 9, floor, false);
    });
    stage.settle(280);
}

/// A Flesh Caverns gullet at high tide, its glands pumping acid, a polyp and a maw worm.
fn flesh_gullet(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(gullet) = stamp(LivingStampKind::Gullet) else {
            return;
        };
        let middle = gullet.bounds.center();
        stage_at(world, player, middle.x - 3, middle.y - 2, false);
        // High water: the world's clock just past the tide's rise.
        let hz = u64::from(world.tick_rate_hz());
        let high = (fatherload_simulation::living_dark::TIDE_RISE_MILLIS + 2_000) * hz / 1_000;
        world.set_simulation_tick(fatherload_simulation::SimulationTick::new(high));
        world.flood_gullets_near(middle, 1_000);
        spawn(world, EnemyKind::BilePolyp, middle.x + 5, middle.y - 3);
        spawn(world, EnemyKind::Leechling, middle.x + 2, middle.y);
        spawn(world, EnemyKind::Leechling, middle.x - 6, middle.y + 1);
    });
    stage.settle(250);
}

/// The Glow Sea's dark: jellies, lure fish and an angler's bait the only lights (with Lamp II
/// when `lamp`).
fn glow_sea(stage: &mut SceneStage<'_>, lamp: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(hall) = stamp(LivingStampKind::AbyssHall) else {
            return;
        };
        let middle = hall.bounds.center();
        stage_at(world, player, middle.x - 6, middle.y, lamp);
        // The creatures hang in the open hall round the rig.
        let open = |world: &WorldState, x: i32, y: i32| {
            (-1..=1).all(|dy| {
                (-1..=1).all(|dx| {
                    !world.terrain().is_solid_at(TilePosition {
                        x: x + dx,
                        y: y + dy,
                    })
                })
            })
        };
        let picks = [
            (EnemyKind::GlowJelly, 6, -1),
            (EnemyKind::GlowJelly, -8, -2),
            (EnemyKind::LureFish, 3, 0),
            (EnemyKind::LureFish, -4, -1),
            (EnemyKind::Angler, 11, -1),
        ];
        let rig_x = middle.x - 6;
        // Where the rig comes to rest: the floor under its drop.
        let ground = (middle.y..middle.y + 40)
            .find(|row| {
                world.terrain().is_solid_at(TilePosition {
                    x: rig_x,
                    y: row + 1,
                })
            })
            .unwrap_or(middle.y);
        let middle = TilePosition {
            x: middle.x,
            y: ground - 1,
        };
        for (kind, dx, dy) in picks {
            let spot = (0..6)
                .flat_map(|lift| [(dx, dy - lift), (dx + 1, dy - lift), (dx - 1, dy - lift)])
                .find(|(x, y)| open(world, rig_x + x, middle.y + y));
            if let Some((x, y)) = spot {
                spawn(world, kind, rig_x + x, middle.y + y);
            }
        }
    });
    stage.settle(260);
}

/// The Mycelial Mind awake in its hollow: its nerve nodes, a spore storm and a possessed thrall
/// (exposed, its nodes dead, when `exposed`).
fn mycelial_mind(stage: &mut SceneStage<'_>, exposed: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(hollow) = living::mind_hollow(&settings, &layout) else {
            return;
        };
        let middle = hollow.center();
        stage_at(world, player, middle.x - 8, hollow.max.y - 6, false);
        reveal(
            world,
            player,
            hollow.min.x - 10..hollow.max.x + 10,
            hollow.min.y - 10..hollow.max.y + 10,
        );
    });
    // The Mind wakes on the first tick a rig is in its hollow, then storms and possesses.
    stage.settle(30);
    if exposed {
        stage.play_on(|world| {
            let nodes = world.living_dark().mind.nodes.clone();
            let mut output = fatherload_simulation::TickOutput::default();
            for node in nodes {
                let _ = world.damage_enemy(
                    fatherload_simulation::entity::EntityId(node),
                    fatherload_simulation::enemy::Damage::new(
                        100_000.0,
                        fatherload_simulation::enemy::DamageKind::Explosive,
                        fatherload_simulation::enemy::DamageSource::World,
                    ),
                    &mut output,
                );
            }
        });
    }
    stage.settle(400);
}

pub const LIVING_DARK_SCENES: &[Scene] = &[
    Scene {
        name: "living_fungal_grove",
        summary: "Descent III, the Fungal Forest: a cap grove of giant mushrooms, the rig bouncing off a cap, a spore puffer, a cap crawler and glow gnats",
        setup: |stage| fungal_grove(stage, false),
    },
    Scene {
        name: "living_spore_cloud",
        summary: "The Fungal Forest's spore cloud: the screen blurs and shifts sickly green-violet (the Bio-Filter sees through it)",
        setup: |stage| fungal_grove(stage, true),
    },
    Scene {
        name: "living_root_labyrinth",
        summary: "The Root Labyrinth: a heartroot chamber; roots grow back over the tunnel the rig dug in; a root snapper, a thorn wisp and a bark beetle",
        setup: root_labyrinth,
    },
    Scene {
        name: "living_hive_court",
        summary: "The Hive's friendly face: the Amber Court's comb hall, its royal larder of jelly cells and the Court's drones",
        setup: |stage| hive_court(stage, false),
    },
    Scene {
        name: "living_hive_court_counter",
        summary: "The Amber Court's counter: royal jelly, the Court's errands, the truce and amends, the crew's standing",
        setup: |stage| hive_court(stage, true),
    },
    Scene {
        name: "living_hive_swarm",
        summary: "The Hive's hostile face: Swarm soldiers and a lancer attack in a brood gallery",
        setup: hive_swarm,
    },
    Scene {
        name: "living_hive_matriarch",
        summary: "The Hive's mini-boss: the brood matriarch in her gallery of jelly cells, breeding soldiers",
        setup: hive_matriarch,
    },
    Scene {
        name: "living_flesh_gullet",
        summary: "The Flesh Caverns: a gullet at high tide, acid pumped by its glands; a bile polyp and leechlings in the pulsing flesh",
        setup: flesh_gullet,
    },
    Scene {
        name: "living_glow_sea",
        summary: "The Glow Sea: near-total darkness lit only by glow jellies, lure fish and an angler's bait; the rig lamp a candle",
        setup: |stage| glow_sea(stage, false),
    },
    Scene {
        name: "living_glow_sea_lamp_ii",
        summary: "The Glow Sea with Lamp II: the rig's wider beam pushes the dark back",
        setup: |stage| glow_sea(stage, true),
    },
    Scene {
        name: "living_mycelial_mind",
        summary: "Descent III's gate: the Mycelial Mind awake in its overgrown hollow, its nerve nodes shielding it, spore storms and a possessed thrall",
        setup: |stage| mycelial_mind(stage, false),
    },
    Scene {
        name: "living_mind_exposed",
        summary: "The Mycelial Mind exposed: its nerve nodes dead, it can be hurt until they grow back",
        setup: |stage| mycelial_mind(stage, true),
    },
];
