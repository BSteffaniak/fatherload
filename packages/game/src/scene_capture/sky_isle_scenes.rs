//! Sky isle scenes (WP-SKY, T20, V9 worlds): a low isle cluster with a fuel buoy, an
//! archipelago with waterfalls, each kind of sky town, wind-riding a jet stream and a
//! collapsing island.

use fatherload_simulation::{
    PlayerId, WorldState,
    sky::{SkyTile, SkyTownKind},
    terrain::{TileKind, TilePosition},
    worldgen::{
        TileRect,
        sky::{
            self, ARCHIPELAGO_BOTTOM_ROWS, DEAD_AIR_BOTTOM_ROWS, LOW_ISLES_BOTTOM_ROWS, SkyLand,
        },
    },
};

use super::{Scene, SceneStage, place_rig, reveal};
use crate::input::PlayerInput;

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

/// A mid-game sky rig: a good tank and engine, some intakes.
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 8;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(8);
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.engine_level = 8;
        rig.hazard_gear.altitude_intake = 2;
        rig.hazard_gear.de_icer = 2;
        rig.credits = 2_400;
    }
}

fn hover_at(world: &mut WorldState, player: PlayerId, x: i32, row: i32) {
    place_rig(world, player, x, row);
    reveal(world, player, x - 60..x + 60, row - 40..row + 40);
    world.stream_terrain_near_players();
}

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

fn isles_near_town(world: &WorldState, bottom: i32, top: i32) -> Vec<SkyLand> {
    let settings = *world.world_settings();
    let mut lands = sky::lands_touching(&settings, &band(-4_000, 4_000, bottom, top));
    lands.sort_by_key(|land| land.center_x.abs());
    lands
}

/// Places the seat's rig standing in the sky town of `kind` nearest town, by its specialty.
fn visit_sky_town(world: &mut WorldState, player: PlayerId, kind: SkyTownKind) {
    fit(world, player);
    let settings = *world.world_settings();
    let site = (0..10)
        .flat_map(|bx| [bx, -bx - 1])
        .flat_map(|bx| (0..12).map(move |by| (bx, by)))
        .filter_map(|(bx, by)| sky::sky_town_site(&settings, bx, by))
        .find(|site| site.kind == kind);
    let Some(site) = site else {
        hover_at(world, player, 100, -900);
        return;
    };
    let town = fatherload_simulation::village::village(&settings, site.id);
    let column = town
        .and_then(|found| {
            found
                .buildings
                .iter()
                .find(|building| building.kind == found.specialty.building())
                .map(|building| building.first_column + building.width + 1)
        })
        .unwrap_or_else(|| site.center());
    hover_at(world, player, column, site.ground_row - 1);
    // Already visited: no arrival banner over the street.
    if let Some(rig) = world.player_mut(player) {
        rig.villages.visited.insert(site.id);
    }
}

pub const SKY_ISLE_SCENES: &[Scene] = &[
    Scene {
        name: "sky_isle_buoy",
        summary: "A low isle cluster with a fuel buoy riding over its pad, sky flora, a cache and skystone in the rock",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let lands = isles_near_town(world, LOW_ISLES_BOTTOM_ROWS, ARCHIPELAGO_BOTTOM_ROWS);
                let buoy = lands.iter().find_map(sky::isles::buoy_of);
                let (x, row) = buoy.map_or((100, -320), |buoy| (buoy.pad.x + 9, buoy.pad.y - 4));
                hover_at(world, player, x, row);
                if let Some(rig) = world.player_mut(player) {
                    rig.fuel = 30.0;
                }
            });
            hang(stage, 14);
        },
    },
    Scene {
        name: "sky_archipelago_waterfalls",
        summary: "A cloud archipelago chain with waterfalls pouring off the isles into the void, a nest and a ruin",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let lands = isles_near_town(
                    world,
                    ARCHIPELAGO_BOTTOM_ROWS,
                    ARCHIPELAGO_BOTTOM_ROWS + 1_400,
                );
                // The isle with a spring and the most neighbours with springs.
                let spring = lands
                    .iter()
                    .filter(|land| land.content().and_then(|c| c.spring).is_some())
                    .max_by_key(|land| {
                        lands
                            .iter()
                            .filter(|other| {
                                other.content().and_then(|c| c.spring).is_some()
                                    && (other.center_x - land.center_x).abs() < 60
                                    && (other.top_row - land.top_row).abs() < 30
                            })
                            .count()
                    });
                // Beside the falls, out in the open sky off the island's rim.
                let (x, row) = spring
                    .and_then(|land| {
                        let spring = land.content().and_then(|c| c.spring)?;
                        let side = if spring.x >= land.center_x { 1 } else { -1 };
                        Some((spring.x + side * 9, spring.y + 10))
                    })
                    .unwrap_or((100, -900));
                hover_at(world, player, x, row);
            });
            hang(stage, 14);
        },
    },
    Scene {
        name: "sky_town_airship_dock",
        summary: "An aeronaut sky town: canvas-and-brass buildings under striped envelopes, an airship moored at the dock",
        setup: |stage| {
            stage.play(1, |world, seats| {
                visit_sky_town(world, seats[0].player, SkyTownKind::AirshipDock);
            });
            stage.settle(16);
            stage.seat(0).village.banner = None;
        },
    },
    Scene {
        name: "sky_town_floating_market",
        summary: "A floating market: striped awnings and lanterns, smugglers' stalls on a town isle",
        setup: |stage| {
            stage.play(1, |world, seats| {
                visit_sky_town(world, seats[0].player, SkyTownKind::FloatingMarket);
            });
            stage.settle(16);
            stage.seat(0).village.banner = None;
        },
    },
    Scene {
        name: "sky_town_monastery",
        summary: "A sky monastery: white stone and blue tiles, a bell tower, prayer flags in the wind",
        setup: |stage| {
            stage.play(1, |world, seats| {
                visit_sky_town(world, seats[0].player, SkyTownKind::Monastery);
            });
            stage.settle(16);
            stage.seat(0).village.banner = None;
        },
    },
    Scene {
        name: "sky_town_cloud_farm",
        summary: "A cloud-farmer village: a vapour silo and cloud nets catching drifting puffs",
        setup: |stage| {
            stage.play(1, |world, seats| {
                visit_sky_town(world, seats[0].player, SkyTownKind::CloudFarm);
            });
            stage.settle(16);
            stage.seat(0).village.banner = None;
        },
    },
    Scene {
        name: "sky_wind_riding",
        summary: "Riding a jet stream along an archipelago chain: wind streaks racing, the rig carried between isles",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let settings = *world.world_settings();
                let stream = sky::jet_streams_touching(
                    &settings,
                    &band(-3_000, 3_000, ARCHIPELAGO_BOTTOM_ROWS, DEAD_AIR_BOTTOM_ROWS),
                )
                .into_iter()
                .filter(|stream| stream.rect.max.x - stream.rect.min.x > 80)
                .min_by_key(|stream| stream.rect.min.x.abs());
                let (x, row) = stream.map_or((100, -900), |stream| {
                    (
                        i32::midpoint(stream.rect.min.x, stream.rect.max.x),
                        i32::midpoint(stream.rect.min.y, stream.rect.max.y),
                    )
                });
                hover_at(world, player, x, row);
            });
            hang(stage, 24);
        },
    },
    Scene {
        name: "sky_isle_collapse",
        summary: "A crumbling isle falling apart after its aether keystone broke: rock tearing loose, dust raining",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                fit(world, player);
                let lands = isles_near_town(world, LOW_ISLES_BOTTOM_ROWS, ARCHIPELAGO_BOTTOM_ROWS);
                let crumbling = lands
                    .iter()
                    .filter(|land| land.half_width >= 8)
                    .find(|land| land.content().and_then(|c| c.keystone).is_some());
                let keystone = crumbling
                    .and_then(|land| land.content().and_then(|c| c.keystone))
                    .unwrap_or(TilePosition { x: 100, y: -320 });
                // Standing on the doomed isle's turf near its edge: the underside goes first.
                let (x, row) = crumbling.map_or((keystone.x, keystone.y - 6), |land| {
                    let x = land.center_x + land.half_width - 3;
                    (x, land.surface_row(x).unwrap_or(land.top_row) - 1)
                });
                hover_at(world, player, x, row);
                if world.terrain().tile(keystone).map(|tile| tile.kind)
                    == Some(TileKind::Sky(SkyTile::Keystone))
                {
                    let _ = world.terrain_mut().set_kind(keystone, TileKind::Air);
                }
            });
            stage.settle(40);
            // Hold the rig where it was so the camera stays on the island.
        },
    },
];
