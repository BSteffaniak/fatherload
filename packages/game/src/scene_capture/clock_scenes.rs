//! World clock scenes (Part II F4): dawn, noon, golden hour and dusk over the surface, the town
//! and a village at night, a rig's headlights on a night in the hills, the Moon overhead, the
//! crew going to bed, and the wave 3 tracks together: a village at dusk with its people walking
//! home, lamps coming on, the pilot out beside the rig and the Moon hanging over its column.

use fatherload_simulation::{
    PlayerId, WorldState,
    clock::{DayLength, MoonPhase, WorldClock},
    economy::SurfaceZone,
    village::{Village, VillageSetting},
    worldgen::biome::Biome,
};

use super::{
    Scene, SceneStage,
    base_scenes::{act_at, homestead_claim},
    park_at, reveal,
    surface_scenes::{find_biome, hover_over},
    village_scenes::find_village,
};
use crate::input::PlayerInput;

/// World time of slice `slice` (0..96) on the first day from day 1 whose Moon is `wanted` (or
/// day 1), for the world's seed.
fn time_of(world: &WorldState, slice: f32, wanted: &[MoonPhase]) -> u64 {
    let length = world.day_length();
    let seed = world.terrain().seed();
    let day = (0..8)
        .find(|day| {
            let clock = WorldClock::from_world_millis(day * length.millis(), length, seed);
            wanted.is_empty() || wanted.contains(&clock.moon_phase())
        })
        .unwrap_or(0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "a slice of a day in milliseconds"
    )]
    let into_day = (f64::from(slice) * length.millis() as f64 / 96.0) as u64;
    day * length.millis() + into_day
}

const BRIGHT_MOON: &[MoonPhase] = &[MoonPhase::WaxingGibbous, MoonPhase::Full];

/// Hovers over the first stretch of `biome` at day slice `slice`.
fn biome_at(stage: &mut SceneStage<'_>, biome: Biome, slice: f32, height: i32) {
    stage.play(1, |world, seats| {
        let x = find_biome(world, biome, 60);
        hover_over(world, seats[0].player, x, height);
        world.stream_terrain_near_players();
        let time = time_of(world, slice, BRIGHT_MOON);
        world.set_world_time_millis(time);
    });
    hang(stage, 12);
}

fn hang(stage: &mut SceneStage<'_>, frames: usize) {
    stage.hold(
        frames,
        PlayerInput {
            thrust: true,
            ..PlayerInput::default()
        },
    );
}

/// The town at day slice `slice`, the rig at the depot.
fn town_at(stage: &mut SceneStage<'_>, slice: f32) {
    stage.play(1, |world, seats| {
        park_at(world, seats[0].player, SurfaceZone::Depot);
        let time = time_of(world, slice, BRIGHT_MOON);
        world.set_world_time_millis(time);
    });
    stage.settle(12);
}

fn park_in_village(world: &mut WorldState, player: PlayerId, found: &Village) {
    let column = i32::midpoint(found.plaza.0, found.plaza.1) + 3;
    if let Some(rig) = world.player_mut(player) {
        #[allow(clippy::cast_precision_loss, reason = "tile indices")]
        {
            rig.x = (column as f32 + 0.5) * fatherload_simulation::TILE_SIZE;
            rig.y = (found.ground_row as f32 - 0.5) * fatherload_simulation::TILE_SIZE;
        }
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.villages.visited.insert(found.id);
    }
    let area = found.area();
    reveal(
        world,
        player,
        area.min.x - 20..area.max.x + 20,
        area.min.y - 4..area.max.y + 6,
    );
}

/// The wave 3 tracks in one view: a lamp-lit surface village (the one nearest the Moon's
/// column, so the Moon hangs in the sky) at dusk, its people walking home, windows and lamps
/// coming on, and seat 0's pilot out on foot beside the parked rig.
fn dusk_village_with_pilot(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let moon = world.moon_anchor().column;
        let settings = *world.world_settings();
        let found = (1..80)
            .flat_map(|site| [site, -site])
            .filter_map(|site| {
                fatherload_simulation::village::village(
                    &settings,
                    fatherload_simulation::village::placement::surface_id(site),
                )
            })
            .filter(|found| {
                found.setting == VillageSetting::Surface
                    && found
                        .decor
                        .iter()
                        .any(|decor| decor.kind == fatherload_simulation::village::DecorKind::Lamp)
            })
            .min_by_key(|found| (i32::midpoint(found.plaza.0, found.plaza.1) - moon).abs())
            .unwrap_or_else(|| {
                find_village(world, |found: &Village| {
                    found.setting == VillageSetting::Surface
                })
            });
        park_in_village(world, player, &found);
        world.stream_terrain_near_players();
        let time = time_of(world, 75.5, BRIGHT_MOON);
        world.set_world_time_millis(time);
        // The pilot climbs out and stands a couple of tiles toward the plaza.
        if let Some(rig) = world.player(player) {
            let (x, y) = (rig.x, rig.y);
            let feet = (y / fatherload_simulation::TILE_SIZE).floor().mul_add(
                fatherload_simulation::TILE_SIZE,
                fatherload_simulation::TILE_SIZE,
            );
            let _ = world.place_pilot(
                player,
                fatherload_simulation::TILE_SIZE.mul_add(-1.6, x),
                feet - 0.25,
            );
        }
    });
    // People spawn on the next refresh and walk their evening legs.
    stage.settle(150);
    stage.seat(0).village.banner = None;
    stage.settle(1);
}

pub const CLOCK_SCENES: &[Scene] = &[
    Scene {
        name: "clock_dawn",
        summary: "Dawn over the mountains: gold and pink at the horizon, the sun rising on the left behind the ridges, rim-lit slopes, the last stars",
        setup: |stage| biome_at(stage, Biome::Mountains, 20.5, 4),
    },
    Scene {
        name: "clock_noon",
        summary: "Noon in town: deep blue overhead, the sun at the top of the sky",
        setup: |stage| town_at(stage, 48.0),
    },
    Scene {
        name: "clock_golden_hour",
        summary: "Golden hour in the forest: a warm wash, a gold horizon, the sun low on the right, cream clouds",
        setup: |stage| biome_at(stage, Biome::Forest, 69.5, 3),
    },
    Scene {
        name: "clock_dusk",
        summary: "Dusk on the plains: rose and violet over the horizon, the hills darkening, the first stars",
        setup: |stage| biome_at(stage, Biome::Plains, 75.5, 3),
    },
    Scene {
        name: "night_town",
        summary: "Night in town: a starry sky, the town's windows glowing, the rig's headlights",
        setup: |stage| town_at(stage, 86.0),
    },
    Scene {
        name: "night_headlights",
        summary: "A night drive across the forest hills: the rig's headlight beam and lamp pool cutting the dark, stars wheeling overhead",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let x = find_biome(world, Biome::Forest, 60);
                hover_over(world, seats[0].player, x, 1);
                world.stream_terrain_near_players();
                let time = time_of(world, 4.0, BRIGHT_MOON);
                world.set_world_time_millis(time);
            });
            stage.hold(
                24,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "night_village",
        summary: "A village at night: lit windows and street lamps, residents about, the surface dark around it",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let found = find_village(world, |found: &Village| {
                    found.setting == VillageSetting::Surface
                        && found.decor.iter().any(|decor| {
                            decor.kind == fatherload_simulation::village::DecorKind::Lamp
                        })
                });
                park_in_village(world, seats[0].player, &found);
                world.stream_terrain_near_players();
                let time = time_of(world, 84.0, BRIGHT_MOON);
                world.set_world_time_millis(time);
            });
            stage.settle(20);
            stage.seat(0).village.banner = None;
            stage.settle(1);
        },
    },
    Scene {
        name: "night_moon",
        summary: "Under the Moon's column at night: the full Moon hanging huge and still overhead",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let column = world.moon_anchor().column;
                hover_over(world, seats[0].player, column, 8);
                world.stream_terrain_near_players();
                let time = time_of(world, 2.0, &[MoonPhase::Full]);
                world.set_world_time_millis(time);
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "night_sleep_vote",
        summary: "Split-screen at night by a homestead: player 1 in bed waiting on the vote, player 2 still out under the stars with the lamps lit",
        setup: |stage| {
            stage.play(2, |world, seats| {
                let (home, column, row) = homestead_claim(world, seats[0].player);
                let time = time_of(world, 82.0, BRIGHT_MOON);
                world.set_world_time_millis(time);
                if let Some(home) = home {
                    act_at(
                        world,
                        seats[0].player,
                        (column, row),
                        fatherload_simulation::base::BuildCommand::Use {
                            structure: home,
                            service: fatherload_simulation::base::ModuleService::Sleep,
                        },
                    );
                }
                if let Some(rig) = world.player_mut(seats[1].player) {
                    #[allow(clippy::cast_precision_loss, reason = "tile indices")]
                    {
                        rig.x = (column as f32 + 9.5) * fatherload_simulation::TILE_SIZE;
                        rig.y = (row as f32 + 0.5) * fatherload_simulation::TILE_SIZE;
                    }
                    rig.velocity_x = 0.0;
                    rig.velocity_y = 0.0;
                }
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "wave3_dusk_village",
        summary: "Wave 3 together: a village at dusk with its people heading home, windows and lamps coming on, the pilot on foot beside the parked rig, and the Moon hanging still over its column",
        setup: dusk_village_with_pilot,
    },
    Scene {
        name: "clock_short_day_hud",
        summary: "The sun dial and day count in the HUD on a 24-minute day, late afternoon of day 3",
        setup: |stage| {
            stage.play(1, |world, seats| {
                world.set_day_length(DayLength::Short);
                park_at(world, seats[0].player, SurfaceZone::Shop);
                world.set_world_time_millis(
                    DayLength::Short.millis() * 2 + DayLength::Short.millis() * 64 / 96,
                );
            });
            stage.settle(8);
        },
    },
];
