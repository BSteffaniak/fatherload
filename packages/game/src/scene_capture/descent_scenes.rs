//! Deep Descent scenes (Wave 7 F8, generator V11): one per Descent at a representative
//! depth, each in its own placeholder palette, with the depth ruler's stratum label and the
//! "entering" banner; plus the Hollow World cavern from inside and a Strange Layers anomaly
//! room.

use fatherload_simulation::{
    PlayerId, WorldState,
    descent::{Descent, LandmarkKind, Stratum},
    terrain::{Terrain, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{self, landmarks},
    },
};

use super::{Scene, SceneStage, carve, place_rig, reveal, sink_rig};
use crate::input::PlayerInput;

/// Seed of every Deep Descent scene world.
const SCENE_SEED: u64 = 0x0D3E_9DE5;
/// Column the scenes dig at.
const SCENE_X: i32 = 120;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// Swaps the staged world's terrain for the Deep Descent scene world.
fn deep_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
}

/// Fits the rig for the deep: a big tank, a strong hull and engine.
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 14;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(14);
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.engine_level = 12;
    }
}

/// A rig in a gallery dug across `stratum` a third of the way down it, the rock around it
/// explored: the stratum's palette, ores, hazards and caves on show.
fn gallery_in(world: &mut WorldState, player: PlayerId, stratum: Stratum) {
    deep_world(world);
    fit(world, player);
    let layout = descent::layout(&settings());
    let (top, bottom) = layout.rows(stratum);
    let row = top + (bottom - top) / 3;
    sink_rig(world, player, SCENE_X, row);
    carve(world, SCENE_X - 14..SCENE_X + 15, row - 1..row + 1);
    reveal(
        world,
        player,
        SCENE_X - 46..SCENE_X + 47,
        row - 18..row + 18,
    );
    world.stream_terrain_near_players();
}

fn descent_scene(stage: &mut SceneStage<'_>, stratum: Stratum) {
    stage.play(1, |world, seats| {
        gallery_in(world, seats[0].player, stratum);
    });
    // Long enough for the "entering" banner to slide in fully.
    stage.settle(20);
}

pub const DESCENT_SCENES: &[Scene] = &[
    Scene {
        name: "descent_1_crust",
        summary: "Descent I, the Crust: the Slate Shelves of a Deep Descent world, classic art",
        setup: |stage| descent_scene(stage, Stratum::SlateShelves),
    },
    Scene {
        name: "descent_2_old_deep",
        summary: "Descent II, the Old Deep: the Radioactive Hollow's green glow",
        setup: |stage| descent_scene(stage, Stratum::RadioactiveHollow),
    },
    Scene {
        name: "descent_3_living_dark",
        summary: "Descent III, the Living Dark: the Fungal Forest's violet rock (placeholder)",
        setup: |stage| descent_scene(stage, Stratum::FungalForest),
    },
    Scene {
        name: "descent_4_drowned_frozen",
        summary: "Descent IV, the Drowned and Frozen: the Brine Glaciers' pale ice (placeholder)",
        setup: |stage| descent_scene(stage, Stratum::BrineGlaciers),
    },
    Scene {
        name: "descent_5_hollow_world",
        summary: "Descent V, the Hollow World: hovering inside the giant Inner Sky cavern",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                deep_world(world);
                fit(world, player);
                let settings = settings();
                let layout = descent::layout(&settings);
                let (_, floor) = descent::hollow_rows(&settings, &layout, SCENE_X);
                let row = floor - 9;
                // An open spot (floating stones drift through the cavern).
                let x = (SCENE_X..SCENE_X + 200)
                    .find(|x| {
                        (row - 3..row + 3).all(|y| descent::hollow_open(&settings, &layout, *x, y))
                    })
                    .unwrap_or(SCENE_X);
                place_rig(world, player, x, row);
                reveal(world, player, x - 60..x + 61, row - 40..row + 40);
                world.stream_terrain_near_players();
            });
            for frame in 0..20 {
                stage.hold(
                    1,
                    PlayerInput {
                        thrust: frame % 3 != 2,
                        ..PlayerInput::default()
                    },
                );
            }
        },
    },
    Scene {
        name: "descent_6_strange_layers",
        summary: "Descent VI, the Strange Layers: Pixel Rot's neon rock beside an anomaly room",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                deep_world(world);
                fit(world, player);
                let settings = settings();
                let layout = descent::layout(&settings);
                let (top, bottom) = layout.descent_rows(Descent::StrangeLayers);
                let area = TileRect {
                    min: TilePosition { x: -600, y: top },
                    max: TilePosition { x: 600, y: bottom },
                };
                let room = landmarks::landmarks_with(&settings, &layout, &area)
                    .into_iter()
                    .filter(|landmark| landmark.kind == LandmarkKind::AnomalyRoom)
                    .min_by_key(|landmark| {
                        (
                            landmark.stratum != Stratum::PixelRot,
                            landmark.bounds.min.x.abs(),
                        )
                    });
                if let Some(room) = room {
                    let row = room.bounds.max.y - 1;
                    let x = room.bounds.min.x - 6;
                    sink_rig(world, player, x, row);
                    carve(world, x..room.bounds.min.x, row..row + 1);
                    reveal(
                        world,
                        player,
                        x - 30..room.bounds.max.x + 30,
                        row - 18..row + 16,
                    );
                    world.stream_terrain_near_players();
                } else {
                    gallery_in(world, player, Stratum::PixelRot);
                }
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "descent_7_ancients",
        summary: "Descent VII, the Ancients: the Buried Megacity's bronze stone (placeholder)",
        setup: |stage| descent_scene(stage, Stratum::BuriedMegacity),
    },
    Scene {
        name: "descent_8_heart",
        summary: "Descent VIII, the Heart: the Core Ocean's molten silver (placeholder)",
        setup: |stage| descent_scene(stage, Stratum::CoreOcean),
    },
    Scene {
        name: "descent_outpost",
        summary: "A Deep Descent outpost hall (placeholder) in the Fungal Forest, the rig at its door",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                deep_world(world);
                fit(world, player);
                let settings = settings();
                let layout = descent::layout(&settings);
                if let Some(outpost) = landmarks::outpost(&settings, &layout, Descent::LivingDark) {
                    let row = outpost.bounds.max.y - 1;
                    let x = outpost.bounds.min.x - 4;
                    sink_rig(world, player, x, row);
                    carve(world, x..outpost.bounds.min.x, row - 1..row + 1);
                    reveal(
                        world,
                        player,
                        x - 20..outpost.bounds.max.x + 20,
                        row - 16..row + 12,
                    );
                    world.stream_terrain_near_players();
                }
            });
            stage.settle(20);
        },
    },
];
