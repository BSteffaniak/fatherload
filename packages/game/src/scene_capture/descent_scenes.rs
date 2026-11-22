//! Deep Descent scenes (Wave 7 F8, generator V11): one per Descent at a representative
//! depth, each in its own placeholder palette, with the depth ruler's stratum label and the
//! "entering" banner; plus the Hollow World cavern from inside and a Strange Layers anomaly
//! room. The Wave 7 integration scenes show the seams working together in a V11 world: the
//! strata's field effects (the Gravity Shear's turned gravity with the rig standing on a wall,
//! the Sunforge's glare, a Mirror Strata cell), the Underground Ocean's generated water
//! pockets and the builders' cable of ancient lifts beside an outpost.

use fatherload_simulation::{
    PlayerId, TILE_SIZE, WorldState,
    depth_progression::ProgressionRules,
    descent::{Descent, LandmarkKind, Stratum},
    field::{self, FieldSample},
    position::Frame,
    terrain::{Terrain, TileKind, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{self, landmarks},
    },
};

use super::{Scene, SceneStage, carve, place_rig, reveal, sink_rig, tile_centre};
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

/// Swaps the staged world's terrain for the Deep Descent scene world, with its depth
/// logistics (the outposts and the builders' cable the generator carved) and the Descent rules.
fn deep_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
    world.set_progression_rules(ProgressionRules::Descent);
    world.install_logistics_sites(&landmarks::logistics_sites(&settings()), false);
}

/// The field sample at the middle of tile `(x, y)` of the scene world (the origin frame the
/// scenes are staged in).
fn field_at(x: i32, y: i32) -> FieldSample {
    let (px, py) = Frame::default().tile_center(TilePosition { x, y });
    field::sample_in(
        &field::FieldZones::default(),
        &settings(),
        Frame::default(),
        px,
        py,
    )
}

/// The middle of the first region cell (scanning out from the scene column, row by row through
/// the middle of `stratum`) whose sample satisfies `wanted` all through a 12-tile box.
fn region_with(stratum: Stratum, wanted: impl Fn(&FieldSample) -> bool) -> Option<(i32, i32)> {
    let layout = descent::layout(&settings());
    let (top, bottom) = layout.rows(stratum);
    let (width, height) = fatherload_simulation::descent::FIELD_REGION_TILES;
    (top + height..bottom - height)
        .step_by(usize::try_from(height).unwrap_or(32))
        .flat_map(|row| {
            (0..40).flat_map(move |step| {
                let offset = (step + 1) / 2 * width * if step % 2 == 0 { 1 } else { -1 };
                let x = (SCENE_X + offset).div_euclid(width) * width + width / 2;
                let y = row.div_euclid(height) * height + height / 2;
                [(x, y)]
            })
        })
        .find(|(x, y)| (-6..=6).all(|dy| (-6..=6).all(|dx| wanted(&field_at(x + dx, y + dy)))))
}

/// A rig in a 30 x 15 chamber carved around `(x, y)`, the rock around it explored.
fn chamber_at(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    deep_world(world);
    fit(world, player);
    carve(world, x - 15..x + 15, y - 7..y + 8);
    place_rig(world, player, x, y);
    reveal(world, player, x - 50..x + 50, y - 30..y + 30);
    world.stream_terrain_near_players();
}

/// Fits the rig for the deep: a big tank, a strong hull and engine.
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 14;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(14);
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.engine_level = 12;
        // Every Descent gate's rig system and every protection, a top radiator and hull, so
        // neither the depths nor the placeholder strata's borrowed heat wreck it mid-scene.
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.radiator_level = 15;
        rig.hull_level = 15;
        rig.hull = rig.max_hull();
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
        name: "descent_explored",
        summary: "A Deep Descent world dug to the bottom: a shaft to the bedrock floor and a \
                  gallery across every Descent, the rig at the foot of the shaft (the browser \
                  memory fixture: export with --export-saves, time with scripts/perf-web.cjs)",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                deep_world(world);
                fit(world, player);
                let settings = settings();
                let layout = descent::layout(&settings);
                let floor = descent::floor_row(&settings);
                for descent in Descent::ALL {
                    let (top, bottom) = layout.descent_rows(descent);
                    let row = i32::midpoint(top, bottom);
                    carve(world, SCENE_X - 120..SCENE_X + 120, row..row + 3);
                }
                let bottom = floor - 3;
                sink_rig(world, player, SCENE_X, bottom);
                reveal(
                    world,
                    player,
                    SCENE_X - 40..SCENE_X + 41,
                    bottom - 20..bottom + 4,
                );
                world.stream_terrain_near_players();
            });
            stage.settle(20);
        },
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
    Scene {
        name: "descent_gravity_shear",
        summary: "Wave 7 integration: a Gravity Shear region cell of a V11 world turns gravity \
                  to pull right; the rig stands on the chamber's right wall, the camera rolled \
                  with it, the stratum's wobble on screen",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let (x, y) = region_with(Stratum::GravityShear, |sample| sample.gravity_x > 0.95)
                    .unwrap_or((SCENE_X, 8_000));
                chamber_at(world, player, x, y);
                if let Some(rig) = world.player_mut(player) {
                    // Against the right wall (column x + 15 is rock), turned to stand on it.
                    rig.x = 0.5f32.mul_add(-TILE_SIZE, tile_centre(x + 15)) - 11.0;
                    rig.attitude.orientation = -std::f32::consts::FRAC_PI_2;
                }
            });
            // Past the "entering" banner, so the field shows.
            stage.settle(260);
        },
    },
    Scene {
        name: "descent_upside_down_isles",
        summary: "Wave 7 integration: an Upside-Down Isles cell flips gravity; the rig, turned \
                  all the way round with its thrusters, rests on the chamber's ceiling",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let (x, y) =
                    region_with(Stratum::UpsideDownIsles, |sample| sample.gravity_y < -0.95)
                        .unwrap_or((SCENE_X, 7_000));
                chamber_at(world, player, x, y);
                if let Some(rig) = world.player_mut(player) {
                    rig.y = tile_centre(y - 6);
                    rig.attitude.orientation = std::f32::consts::PI;
                }
            });
            // Past the "entering" banner, so the field shows.
            stage.settle(260);
        },
    },
    Scene {
        name: "descent_mirror_cell",
        summary: "Wave 7 integration: a Mirror Strata cell mirrors the screen and the controls \
                  (the HUD stays readable)",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let (x, y) = region_with(Stratum::MirrorStrata, FieldSample::mirrored)
                    .unwrap_or((SCENE_X, 7_900));
                chamber_at(world, player, x, y + 6);
            });
            // Past the "entering" banner, so the field shows.
            stage.settle(260);
        },
    },
    Scene {
        name: "descent_sunforge_glare",
        summary: "Wave 7 integration: the Sunforge's field over-exposes and blooms the screen \
                  near the Hollow World's inner sun",
        setup: |stage| {
            stage.play(1, |world, seats| {
                gallery_in(world, seats[0].player, Stratum::TheSunforge);
            });
            // Past the "entering" banner, so the glare shows.
            stage.settle(260);
        },
    },
    Scene {
        name: "descent_underground_ocean",
        summary: "Wave 7 integration: the Underground Ocean's generated water pockets (the \
                  fluids generator per stratum) around a gallery, one breached and flooding",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                deep_world(world);
                fit(world, player);
                let layout = descent::layout(&settings());
                let (top, bottom) = layout.rows(Stratum::UndergroundOcean);
                // The densest water near the scene column, a third of the way down.
                let row = top + (bottom - top) / 3;
                let wet = |x: i32, y: i32| {
                    matches!(
                        world
                            .terrain()
                            .tile(TilePosition { x, y })
                            .map(|tile| tile.kind),
                        Some(TileKind::Fluid(_))
                    )
                };
                let (x, y) = (row - 40..row + 40)
                    .step_by(4)
                    .flat_map(|y| {
                        (SCENE_X - 200..SCENE_X + 200)
                            .step_by(4)
                            .map(move |x| (x, y))
                    })
                    .max_by_key(|(x, y)| {
                        (-24..=24)
                            .step_by(2)
                            .flat_map(|dy| (-40..=40).step_by(2).map(move |dx| (dx, dy)))
                            .filter(|(dx, dy)| wet(x + dx, y + dy))
                            .count()
                    })
                    .unwrap_or((SCENE_X, row));
                carve(world, x - 18..x + 18, y..y + 2);
                // Any pocket the gallery cut into floods it.
                world.wake_fluids(TileRect {
                    min: TilePosition {
                        x: x - 20,
                        y: y - 4,
                    },
                    max: TilePosition {
                        x: x + 20,
                        y: y + 4,
                    },
                });
                place_rig(world, player, x, y + 1);
                reveal(world, player, x - 60..x + 60, y - 34..y + 34);
                world.stream_terrain_near_players();
            });
            // Past the "entering" banner, so the field shows.
            stage.settle(260);
        },
    },
    Scene {
        name: "descent_builders_cable",
        summary: "Wave 7 integration: the builders' cable in a V11 world, the ancient lift's \
                  rail and station beside the Old Deep's outpost hall, the rig at the station",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                deep_world(world);
                fit(world, player);
                let settings = settings();
                let column = landmarks::lift_column(&settings);
                let sites = landmarks::logistics_sites(&settings);
                let station = sites
                    .outposts
                    .get(1)
                    .map_or(1_500, |outpost| outpost.floor_row - 1);
                place_rig(world, player, column, station);
                reveal(
                    world,
                    player,
                    column - 50..column + 50,
                    station - 26..station + 16,
                );
                world.stream_terrain_near_players();
            });
            stage.settle(20);
        },
    },
];
