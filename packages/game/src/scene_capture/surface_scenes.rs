//! Surface scenes (WP-SURFACE): each biome, a mountain range, a mountain cave, a lake, a
//! biome border, and the sky high up and out in space.

use fatherload_simulation::{
    PlayerId, WorldState,
    terrain::{TILE_SIZE, TilePosition},
    worldgen::biome::{self, Biome},
};

use super::{Scene, SceneStage, place_rig, reveal};
use crate::input::PlayerInput;

/// The middle of the first run of at least `width` columns of `biome` (searching outward from
/// the town, east first), away from flat sites.
fn find_biome(world: &WorldState, biome: Biome, width: i32) -> i32 {
    let settings = *world.world_settings();
    let matches = |x: i32| {
        (x - width / 2..=x + width / 2)
            .step_by(4)
            .all(|column| biome::biome_at(&settings, column) == biome)
            && !biome::column(&settings, x).flat
    };
    for distance in (400..40_000).step_by(16) {
        for x in [distance, -distance] {
            if matches(x) {
                return x;
            }
        }
    }
    400
}

/// The highest column within `reach` of `x`.
fn peak_near(world: &WorldState, x: i32, reach: i32) -> i32 {
    (x - reach..=x + reach)
        .min_by_key(|column| world.terrain().surface_row(*column))
        .unwrap_or(x)
}

/// Hovers the seat's rig `height` rows above column `x`'s ground and reveals the area, so the
/// surface reads without fog.
fn hover_over(world: &mut WorldState, player: PlayerId, x: i32, height: i32) {
    let row = world.terrain().surface_row(x) - height;
    place_rig(world, player, x, row);
    reveal(world, player, x - 40..x + 40, row - 30..row + height + 25);
}

/// Holds thrust and a little sideways input so the rig hangs in the air for the capture.
fn hang(stage: &mut SceneStage<'_>, frames: usize) {
    stage.hold(
        frames,
        PlayerInput {
            thrust: true,
            ..PlayerInput::default()
        },
    );
}

fn biome_scene(stage: &mut SceneStage<'_>, biome: Biome, height: i32) {
    stage.play(1, |world, seats| {
        let x = find_biome(world, biome, 60);
        hover_over(world, seats[0].player, x, height);
        world.stream_terrain_near_players();
    });
    hang(stage, 12);
}

pub const SURFACE_SCENES: &[Scene] = &[
    Scene {
        name: "biome_plains",
        summary: "Plains: gentle grassy swells, flowers, bushes and the odd oak, drifting seeds",
        setup: |stage| biome_scene(stage, Biome::Plains, 3),
    },
    Scene {
        name: "biome_forest",
        summary: "Forest: rolling hills under oaks and pines, forested backdrop ridges, falling leaves",
        setup: |stage| biome_scene(stage, Biome::Forest, 3),
    },
    Scene {
        name: "biome_desert",
        summary: "Desert: sand dunes over sandstone, cacti and dead brush, dune backdrop, blowing dust",
        setup: |stage| biome_scene(stage, Biome::Desert, 3),
    },
    Scene {
        name: "biome_badlands",
        summary: "Badlands: terraced red-rock mesas with cliffs, butte backdrop, red dust",
        setup: |stage| biome_scene(stage, Biome::Badlands, 4),
    },
    Scene {
        name: "biome_tundra",
        summary: "Tundra: snow over frozen dirt, snowy pines and boulders, snowfall",
        setup: |stage| biome_scene(stage, Biome::Tundra, 3),
    },
    Scene {
        name: "biome_wetlands",
        summary: "Wetlands: lakes in mud basins with lily pads, reeds on the banks, marsh backdrop, rain",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let settings = *world.world_settings();
                let x = find_biome(world, Biome::Wetlands, 60);
                // Centre on the nearest lake.
                let lake = (x - 300..x + 300)
                    .filter(|column| biome::lake_row(&settings, *column).is_some())
                    .min_by_key(|column| (column - x).abs())
                    .unwrap_or(x);
                hover_over(world, seats[0].player, lake, 4);
                world.stream_terrain_near_players();
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "biome_mountains",
        summary: "Mountains: a granite peak with snow cap, ledges and cliffs, pines below the treeline, high-altitude snow",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let x = find_biome(world, Biome::Mountains, 90);
                let peak = peak_near(world, x, 120);
                hover_over(world, seats[0].player, peak + 9, 4);
                world.stream_terrain_near_players();
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "mountain_range",
        summary: "A mountain range from its foothills: slopes climbing far above the town's ground line, the mountain backdrop behind",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let x = find_biome(world, Biome::Mountains, 90);
                let peak = peak_near(world, x, 120);
                let settings = *world.world_settings();
                // Stand off the foothills on the lower side, looking up the slope.
                let foot = (peak - 160..peak)
                    .rev()
                    .find(|column| {
                        biome::surface_row(&settings, *column)
                            > biome::surface_row(&settings, peak) + 34
                    })
                    .unwrap_or(peak - 40);
                hover_over(world, seats[0].player, foot, 14);
                world.stream_terrain_near_players();
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "mountain_cave",
        summary: "Inside a mountain: a cave tunnel through the granite with mountain ore in its walls",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let settings = *world.world_settings();
                let x = find_biome(world, Biome::Mountains, 90);
                let mut spot = None;
                'search: for column in x - 200..x + 200 {
                    let surface = biome::surface_row(&settings, column);
                    for row in surface + 6..fatherload_simulation::worldgen::strata::SKY_ROWS - 4 {
                        if biome::mountain_cave(&settings, column, row)
                            && biome::mountain_cave(&settings, column, row - 1)
                            && !biome::mountain_cave(&settings, column, row + 1)
                        {
                            spot = Some((column, row - 1));
                            break 'search;
                        }
                    }
                }
                let (column, row) = spot.unwrap_or((x, biome::surface_row(&settings, x) + 6));
                let player = seats[0].player;
                place_rig(world, player, column, row);
                reveal(world, player, column - 30..column + 30, row - 20..row + 20);
                world.stream_terrain_near_players();
            });
            stage.settle(12);
        },
    },
    Scene {
        name: "biome_border",
        summary: "Where two biomes meet: heights, ground, flora and the hills backdrop blending across the boundary",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let settings = *world.world_settings();
                let x = (500..20_000)
                    .find(|column| {
                        let blend = biome::biome_blend(&settings, *column);
                        blend.mix >= 120
                            && blend.primary != blend.secondary
                            && !blend.primary.is_cold()
                            && blend.secondary != Biome::Mountains
                    })
                    .unwrap_or(600);
                hover_over(world, seats[0].player, x, 4);
                world.stream_terrain_near_players();
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "sky_high_altitude",
        summary: "Three hundred tiles above the ground: the deep blue upper sky, cirrus, the first stars",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(rig) = world.player_mut(player) {
                    rig.y = -300.0 * TILE_SIZE;
                }
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "sky_space",
        summary: "Ten thousand tiles up: no ceiling, just stars in the black, the HUD showing altitude",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(rig) = world.player_mut(player) {
                    rig.y = -10_000.0 * TILE_SIZE;
                    rig.fuel = rig.fuel_capacity;
                }
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "sky_mountain_clouds",
        summary: "Flying past a peak through the cloud banks at mountain height",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let x = find_biome(world, Biome::Mountains, 90);
                let peak = peak_near(world, x, 120);
                let player = seats[0].player;
                let row = world.terrain().surface_row(peak) - 6;
                place_rig(world, player, peak - 14, row);
                reveal(
                    world,
                    player,
                    peak - 50..peak + 30,
                    row..row + 60,
                );
                let _ = TilePosition { x: peak, y: row };
                world.stream_terrain_near_players();
            });
            hang(stage, 12);
        },
    },
];
