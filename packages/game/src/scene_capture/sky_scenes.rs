//! Sky scenes (expansion plan F6, V8 worlds): a low isle cluster, a cloud archipelago, the
//! dead-air gap's crosswinds, a sky continent's edge, the Moon seen from below and up close,
//! and an updraft.

use fatherload_simulation::{
    PlayerId, WorldState, environment,
    terrain::TilePosition,
    worldgen::{
        TileRect,
        sky::{
            self, ARCHIPELAGO_BOTTOM_ROWS, CONTINENTS_BOTTOM_ROWS, DEAD_AIR_BOTTOM_ROWS,
            LOW_ISLES_BOTTOM_ROWS, MOON_BOTTOM_ROWS, ORBIT_BOTTOM_ROWS, SkyLand, SkyLandKind,
        },
    },
};

use super::{Scene, SceneStage, place_rig, reveal};
use crate::input::PlayerInput;

/// The rect of altitude band `bottom..top` (rows above row 0) over columns `first..=last`.
const fn band(first: i32, last: i32, bottom: i32, top: i32) -> TileRect {
    TileRect {
        min: TilePosition {
            x: first,
            y: sky::row_of(top),
        },
        max: TilePosition {
            x: last,
            y: sky::row_of(bottom),
        },
    }
}

/// Fits the seat's rig for the high sky: a big tank and engine, `intake` and `de_icer` tiers.
fn fit(world: &mut WorldState, player: PlayerId, intake: u8, de_icer: u8) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 14;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(14);
        rig.fuel = rig.fuel_capacity * 0.8;
        rig.engine_level = 14;
        rig.hazard_gear.altitude_intake = intake;
        rig.hazard_gear.de_icer = de_icer;
    }
}

/// Places the rig at `(x, row)` and reveals around it.
fn hover_at(world: &mut WorldState, player: PlayerId, x: i32, row: i32) {
    place_rig(world, player, x, row);
    reveal(world, player, x - 45..x + 45, row - 30..row + 30);
    world.stream_terrain_near_players();
}

/// Taps thrust so the rig hangs in the air for the capture (roughly hovering).
fn hang(stage: &mut SceneStage<'_>, frames: usize) {
    for frame in 0..frames {
        stage.hold(
            1,
            PlayerInput {
                thrust: frame % 3 != 2,
                ..PlayerInput::default()
            },
        );
    }
}

/// The land cell (lands sharing a feature-id region) with the most lands of `kind` in `area`,
/// nearest the town first.
fn best_group(world: &WorldState, area: &TileRect, kind: SkyLandKind) -> Vec<SkyLand> {
    let settings = *world.world_settings();
    let lands: Vec<SkyLand> = sky::lands_touching(&settings, area)
        .into_iter()
        .filter(|land| land.kind == kind)
        .collect();
    let region = |land: &SkyLand| land.id.0 >> 11;
    let mut best: Vec<SkyLand> = Vec::new();
    for land in &lands {
        let group: Vec<SkyLand> = lands
            .iter()
            .filter(|other| region(other) == region(land))
            .copied()
            .collect();
        if group.len() > best.len() {
            best = group;
        }
    }
    best
}

pub const SKY_SCENES: &[Scene] = &[
    Scene {
        name: "sky_low_isles",
        summary: "A cluster of low sky isles over the first cloud sea: turf tops, rock undersides, hanging roots and crystal",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player, 0, 0);
                let area = band(
                    -1_500,
                    1_500,
                    LOW_ISLES_BOTTOM_ROWS,
                    ARCHIPELAGO_BOTTOM_ROWS,
                );
                let group = best_group(world, &area, SkyLandKind::Isle);
                // Just off the cluster's westmost isle, level with its top.
                let (x, row) = group
                    .iter()
                    .min_by_key(|isle| isle.center_x)
                    .map_or((100, -300), |isle| {
                        (isle.center_x - isle.half_width - 4, isle.top_row - 2)
                    });
                hover_at(world, player, x, row);
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "sky_archipelago",
        summary: "A cloud archipelago: a chain of medium islands stepping across the sky, a cloud sea below",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player, 2, 2);
                let area = band(
                    -3_000,
                    3_000,
                    ARCHIPELAGO_BOTTOM_ROWS,
                    ARCHIPELAGO_BOTTOM_ROWS + 1_200,
                );
                let group = best_group(world, &area, SkyLandKind::ArchipelagoIsle);
                let (x, row) = group
                    .get(1)
                    .or_else(|| group.first())
                    .map_or((100, -900), |isle| (isle.center_x, isle.top_row - 5));
                hover_at(world, player, x, row);
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "sky_dead_air_gap",
        summary: "The dead-air gap: crosswind streaks in thin dark air, the sky gauge warning of thin air and cold",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player, 6, 2);
                let settings = *world.world_settings();
                // The windiest layer a third of the way up the gap.
                let start =
                    DEAD_AIR_BOTTOM_ROWS + (CONTINENTS_BOTTOM_ROWS - DEAD_AIR_BOTTOM_ROWS) / 3;
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    reason = "altitudes and wind speeds"
                )]
                let altitude = (start..start + 400)
                    .step_by(8)
                    .max_by_key(|rows| {
                        (environment::wind_at(&settings, 100, *rows as f32).abs() * 10.0) as i32
                    })
                    .unwrap_or(start);
                hover_at(world, player, 100, sky::row_of(altitude));
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "sky_continent_edge",
        summary: "The edge of a sky continent hundreds of tiles wide, riding a high cold cloud sea",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player, 6, 4);
                let settings = *world.world_settings();
                let area = band(-12_000, 12_000, CONTINENTS_BOTTOM_ROWS, ORBIT_BOTTOM_ROWS);
                let continent = sky::lands_touching(&settings, &area)
                    .into_iter()
                    .filter(|land| land.kind == SkyLandKind::Continent)
                    .min_by_key(|land| land.center_x.abs());
                let (x, row) = continent.map_or((100, -8_500), |land| {
                    let edge = land.center_x - land.half_width + 6;
                    (edge, land.surface_row(edge).unwrap_or(land.top_row) - 3)
                });
                hover_at(world, player, x, row);
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "sky_moon_from_below",
        summary: "In orbit under the Moon: its vast underside looming overhead, near-zero gravity, stars",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player, 6, 6);
                let settings = *world.world_settings();
                let x = sky::moon_column(&settings).unwrap_or(4_000) + 300;
                hover_at(world, player, x, sky::row_of(MOON_BOTTOM_ROWS - 1_200));
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "sky_moon_underside",
        summary: "Right under the Moon: its rocky underside hanging just overhead, moon crystal dangling",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player, 6, 6);
                let settings = *world.world_settings();
                let moon = sky::moon(&settings).expect("V8 has a moon");
                let x = moon.center_x + 200;
                let underside = (moon.top_row..moon.bounds().max.y)
                    .rev()
                    .find(|row| moon.tile_at(x, *row).is_some())
                    .unwrap_or(moon.top_row + moon.depth);
                hover_at(world, player, x, underside + 7);
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "sky_moon_surface",
        summary: "Standing on the Moon: grey regolith and craters under black space, low gravity",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player, 6, 6);
                let settings = *world.world_settings();
                let moon = sky::moon(&settings).expect("V8 has a moon");
                // A level stretch of regolith near the middle.
                let level = |x: i32| {
                    (x - 12..=x + 12)
                        .map(|column| moon.surface_row(column).unwrap_or(moon.top_row))
                        .fold((i32::MAX, i32::MIN), |(low, high), row| {
                            (low.min(row), high.max(row))
                        })
                };
                let x = (0..600)
                    .map(|step| moon.center_x - 300 + step * 2)
                    .find(|x| {
                        let (low, high) = level(*x);
                        high - low <= 1
                    })
                    .unwrap_or(moon.center_x);
                let surface = moon.surface_row(x).unwrap_or(moon.top_row);
                hover_at(world, player, x, surface - 1);
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "sky_cloud_sea",
        summary: "Skimming the first cloud sea just under the low isles: puffed tops drifting in parallax",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player, 0, 0);
                hover_at(world, player, 100, sky::row_of(LOW_ISLES_BOTTOM_ROWS - 4));
            });
            hang(stage, 12);
        },
    },
    Scene {
        name: "sky_updraft",
        summary: "An updraft column among the isles: mint motes rising, a rig riding it up without thrust",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player, 0, 0);
                let settings = *world.world_settings();
                let area = band(
                    -2_000,
                    2_000,
                    LOW_ISLES_BOTTOM_ROWS,
                    ARCHIPELAGO_BOTTOM_ROWS + 400,
                );
                // An updraft with an island nearby, nearest the town.
                let updraft = sky::updrafts_touching(&settings, &area)
                    .into_iter()
                    .filter(|updraft| {
                        !sky::lands_touching(&settings, &updraft.rect.expanded(14)).is_empty()
                            && sky::lands_touching(&settings, &updraft.rect).is_empty()
                    })
                    .min_by_key(|updraft| updraft.rect.min.x.abs());
                let (x, row) = updraft.map_or((100, -400), |updraft| {
                    (
                        i32::midpoint(updraft.rect.min.x, updraft.rect.max.x),
                        i32::midpoint(updraft.rect.min.y, updraft.rect.max.y),
                    )
                });
                hover_at(world, player, x, row);
            });
            stage.settle(16);
        },
    },
];
