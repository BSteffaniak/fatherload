//! Wave 6 content scenes: a sky town's land office and its isle plots, a cloud cottage on an
//! isle, the Moon's colonists at work, new furniture and tomb trophies in a home, a longhouse
//! at dusk, and a tomb-bound story under way (at the tomb door, and in the journal).

use fatherload_simulation::{
    PlayerId, TILE_SIZE, WorldState,
    home::{
        Blueprint, Furniture, PlotId, PlotSite, RoomKind,
        decor::{Flooring, PlacedDecor, Wallpaper},
        plot,
    },
    item::ItemKind,
    people::{ArcId, Culture, DayTime, colony},
    terrain::{ArtifactKind, TilePosition},
    village::{self, BuildingKind, Village, VillageSetting},
};

use super::{
    Scene, SceneStage,
    people_scenes::{column_x, find_village, set_time},
    reveal,
    story_scenes::bind_story,
};
use crate::shell::{
    ModalScreen,
    homes::{HomeInterior, HomeScreen},
};

/// Parks `player`'s rig at rest standing on `ground_row` at `column`, the area revealed.
#[allow(clippy::cast_precision_loss, reason = "tile rows")]
fn park(world: &mut WorldState, player: PlayerId, column: i32, ground_row: i32) {
    if let Some(rig) = world.player_mut(player) {
        rig.x = column_x(column);
        rig.y = (ground_row as f32 - 0.5) * TILE_SIZE;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.x_remainder = 0.0;
        rig.credits = 24_000;
        rig.fuel_tank_level = 12;
        rig.engine_level = 12;
    }
    reveal(
        world,
        player,
        column - 70..column + 70,
        ground_row - 40..ground_row + 20,
    );
    world.stream_terrain_near_players();
}

/// The pilot out on foot at `column`, standing on `ground_row`.
#[allow(clippy::cast_precision_loss, reason = "tile rows")]
fn pilot_at(world: &mut WorldState, player: PlayerId, column: i32, ground_row: i32) {
    let _ = world.place_pilot(player, column_x(column), ground_row as f32 * TILE_SIZE);
}

/// The first sky town near the spawn with isle plots and its own folk, with its plots.
fn sky_town(world: &WorldState) -> Option<(std::sync::Arc<Village>, Vec<PlotSite>)> {
    let settings = *world.world_settings();
    fatherload_simulation::worldgen::sky::towns::sky_towns_between(&settings, -12_000, 12_000)
        .into_iter()
        .filter(|town| {
            fatherload_simulation::worldgen::sky::continent::sky_city(&settings, town.id).is_none()
        })
        .filter_map(|town| {
            let sites = plot::sky_sites_of(&settings, town.id);
            let found = village::village(&settings, town.id)?;
            (!sites.is_empty()).then_some((town, found, sites))
        })
        .min_by_key(|(town, _, _)| (town.block.1, town.center().abs()))
        .map(|(_, found, sites)| (found, sites))
}

/// The column of `found`'s hall door.
fn hall_door(found: &Village) -> i32 {
    found
        .buildings
        .iter()
        .find(|lot| lot.kind == BuildingKind::Hall)
        .map_or_else(
            || found.center_column(),
            |lot| lot.first_column + lot.width / 2,
        )
}

/// Inside `plot`'s room `room` at room units `x`, no panel open.
fn inside(stage: &mut SceneStage<'_>, plot: PlotId, room: usize, x: f32) {
    let seat = stage.seat(0);
    seat.home.interior = Some(HomeInterior {
        plot,
        room,
        x,
        facing: 1.0,
        stride: 0.0,
    });
    seat.modal = None;
    seat.village.banner = None;
}

/// Places `pieces` (cell, furniture, display) in room `room` of `plot`.
fn furnish(
    world: &mut WorldState,
    plot: PlotId,
    room: RoomKind,
    pieces: &[(u8, Furniture, Option<ItemKind>)],
    look: (Wallpaper, Flooring),
) {
    world.scenario_edit_plot(plot, |found| {
        let Some(home) = found.home.as_mut() else {
            return;
        };
        let Some(room) = home
            .rooms
            .iter_mut()
            .find(|candidate| candidate.kind == room)
        else {
            return;
        };
        room.decor.pieces.clear();
        room.decor.rugs.clear();
        for (cell, furniture, display) in pieces {
            let placed = PlacedDecor {
                furniture: *furniture,
                display: *display,
            };
            if furniture.underlay() {
                room.decor.rugs.insert(*cell, placed);
            } else {
                room.decor.pieces.insert(*cell, placed);
            }
        }
        room.decor.wallpaper = look.0;
        room.decor.flooring = look.1;
    });
}

/// The listed ground site nearest the town that fits `blueprint`.
fn ground_site(world: &WorldState, blueprint: Blueprint) -> PlotSite {
    let settings = *world.world_settings();
    let mut sites = plot::listed_sites_between(&settings, -6_000, 6_000);
    sites.sort_by_key(|site| (site.middle().abs(), site.left));
    sites
        .into_iter()
        .find(|site| site.size >= blueprint.min_plot() && site.width() >= blueprint.footprint().0)
        .expect("a listed site")
}

/// A furnished home of `blueprint` on a ground site, the pilot at its door, inside `room`.
fn furnished_room(
    stage: &mut SceneStage<'_>,
    blueprint: Blueprint,
    room: RoomKind,
    pieces: &'static [(u8, Furniture, Option<ItemKind>)],
    look: (Wallpaper, Flooring),
    time: DayTime,
) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        set_time(world, time);
        let site = ground_site(world, blueprint);
        let plot = world.scenario_home(player, site, Some(blueprint));
        furnish(world, plot, room, pieces, look);
        park(world, player, site.right() + 4, site.ground_row);
        let door = world.homes().plot(plot).map_or(
            site.middle(),
            fatherload_simulation::home::Plot::door_column,
        );
        pilot_at(world, player, door, site.ground_row);
    });
    stage.settle(2);
    let found = stage.world().homes().plots().next().map(|plot| {
        let index = plot.home.as_ref().and_then(|home| {
            home.rooms
                .iter()
                .position(|candidate| candidate.kind == room)
        });
        (plot.id, index.unwrap_or(0))
    });
    if let Some((plot, index)) = found {
        inside(stage, plot, index, 560.0);
    }
    stage.settle(3);
}

const CULTURE_PIECES: &[(u8, Furniture, Option<ItemKind>)] = &[
    (0, Furniture::Loom, None),
    (2, Furniture::RockingChair, None),
    (3, Furniture::ClayOven, None),
    (4, Furniture::FishTank, None),
    (5, Furniture::JunkSculpture, None),
    (6, Furniture::Gramophone, None),
    (2, Furniture::Rug, None),
    (8 + 1, Furniture::StarChart, None),
    (8 + 4, Furniture::Barometer, None),
    (8 + 6, Furniture::Painting, None),
    (16 + 1, Furniture::GlowcapLamp, None),
    (16 + 3, Furniture::ModelAirship, None),
    (16 + 6, Furniture::WindChime, None),
];

const TROPHY_PIECES: &[(u8, Furniture, Option<ItemKind>)] = &[
    (
        0,
        Furniture::RelicPlinth,
        Some(ItemKind::Artifact(ArtifactKind::StarCore)),
    ),
    (1, Furniture::MoonGlobe, None),
    (2, Furniture::HydroponicRack, None),
    (4, Furniture::CloudHammock, None),
    (7, Furniture::Moonflower, None),
    (8 + 1, Furniture::GuardianMask, None),
    (8 + 3, Furniture::GuardianMask, None),
    (8 + 6, Furniture::StarChart, None),
    (16 + 2, Furniture::Lantern, None),
    (16 + 5, Furniture::StarMobile, None),
];

fn frontier_village(found: &Village) -> bool {
    found.culture == Culture::Frontier && found.setting == VillageSetting::Surface
}

/// Binds the tomb arc `arc` for seat 0's player from the first frontier village and makes it
/// their running story at beat `beat`; returns the tomb door it was bound to.
fn tomb_story(world: &mut WorldState, arc: u16, beat: u8) -> Option<TilePosition> {
    let player = world.player_ids().next()?;
    let found = find_village(world, frontier_village);
    let mut run = bind_story(world, &found, 0, ArcId(arc), player)?;
    run.quest.stage = beat;
    for progress in run.quest.objectives.iter_mut().take(usize::from(beat)) {
        progress.done = true;
    }
    let door = run.quest.targets.first().map(|target| target.center);
    if let Some(rig) = world.player_mut(player) {
        rig.quests.arcs.active.push(run);
    }
    door
}

pub const WAVE6_SCENES: &[Scene] = &[
    Scene {
        name: "w6_sky_land_office",
        summary: "A sky town's hall is its land office: plots on the small isles beside the town, sized and priced for the clouds, and a catalogue with hammocks and barometers",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                set_time(world, DayTime::at(2, 11, 0));
                if let Some((found, _)) = sky_town(world) {
                    park(world, player, hall_door(&found), found.ground_row);
                }
            });
            stage.settle(30);
            stage.seat(0).village.banner = None;
            stage.seat(0).open_home_screen(HomeScreen::LandOffice, None);
            stage.settle(4);
        },
    },
    Scene {
        name: "w6_sky_isle_plots",
        summary: "A cloud cottage built on a sky town's isle plot at midday, the town's isle beside it",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                set_time(world, DayTime::at(2, 12, 0));
                let Some((found, sites)) = sky_town(world) else {
                    return;
                };
                let built = sites[sites.len() - 1];
                let _ = world.scenario_home(player, built, Some(Blueprint::CloudCottage));
                let column = if built.middle() < found.center_column() {
                    built.right() + 10
                } else {
                    built.left - 10
                };
                park(world, player, column, built.ground_row);
                if let Some(rig) = world.player_mut(player) {
                    rig.y -= 4.0 * TILE_SIZE;
                }
            });
            stage.settle(150);
            stage.seat(0).village.banner = None;
            stage.settle(1);
        },
    },
    Scene {
        name: "w6_moon_colonists",
        summary: "Tranquility Station by day: colonists in jumpsuits at work under the dome and by the hab, the rig on the pad",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                set_time(world, DayTime::at(2, 10, 30));
                let settings = *world.world_settings();
                let Some(settlement) = colony::colony_settlement_id(&settings).and_then(|id| {
                    fatherload_simulation::people::routine::settlement(&settings, id)
                }) else {
                    return;
                };
                if let Some(rig) = world.player_mut(player) {
                    rig.hazard_gear.pressurized_hull =
                        fatherload_simulation::moon::VACUUM_SEAL_TIER;
                    rig.hazard_gear.life_support = 2;
                    rig.lunar.landed = true;
                }
                park(
                    world,
                    player,
                    settlement.center_column() - 9,
                    settlement.ground_row,
                );
            });
            stage.settle(150);
            stage.seat(0).village.banner = None;
            stage.settle(1);
        },
    },
    Scene {
        name: "w6_home_culture_furniture",
        summary: "A longhouse's great hall furnished with every culture's signature piece: a Highland loom, a Frontier rocking chair, a Mesa clay oven, a Riverside fish tank, a Scrapper sculpture, Deepfolk glowcaps, an Aeronaut model airship and Cloudborn chimes",
        setup: |stage| {
            furnished_room(
                stage,
                Blueprint::Longhouse,
                RoomKind::Hall,
                CULTURE_PIECES,
                (Wallpaper::Timber, Flooring::Planks),
                DayTime::at(2, 17, 0),
            );
        },
    },
    Scene {
        name: "w6_home_trophies",
        summary: "A study of trophies and far places: a relic plinth showing a star core, two guardian's masks won in tombs, a moon globe, a hydroponic rack, a cloud hammock, a moonflower and a star chart",
        setup: |stage| {
            furnished_room(
                stage,
                Blueprint::House,
                RoomKind::Study,
                TROPHY_PIECES,
                (Wallpaper::Midnight, Flooring::Parquet),
                DayTime::at(2, 17, 30),
            );
        },
    },
    Scene {
        name: "w6_longhouse_dusk",
        summary: "A new plan: a turf-roofed longhouse at dusk, its windows lit, on a village-edge plot",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                set_time(world, DayTime::at(3, 18, 10));
                let site = ground_site(world, Blueprint::Longhouse);
                let plot = world.scenario_home(player, site, Some(Blueprint::Longhouse));
                world.scenario_edit_plot(plot, |found| {
                    if let Some(home) = found.home.as_mut() {
                        for room in &mut home.rooms {
                            room.decor.pieces.entry(8 + 7).or_insert(PlacedDecor {
                                furniture: Furniture::WallLamp,
                                display: None,
                            });
                        }
                    }
                });
                park(world, player, site.right() + 6, site.ground_row);
            });
            stage.settle(20);
            stage.seat(0).village.banner = None;
            stage.settle(1);
        },
    },
    Scene {
        name: "w6_story_tomb_step",
        summary: "A tomb-bound story under way: the pilot walking up to a sealed tomb's door to see the glow a prospector swore by, the story's marker on the door",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                set_time(world, DayTime::at(2, 17, 30));
                if let Some(door) = tomb_story(world, 905, 1) {
                    park(world, player, door.x + 7, door.y + 1);
                    pilot_at(world, player, door.x + 3, door.y + 1);
                }
            });
            stage.settle(20);
            stage.seat(0).village.banner = None;
            stage.settle(1);
        },
    },
    Scene {
        name: "w6_story_tomb_journal",
        summary: "The journal with tomb stories under way: reading a seal's last line, a lamp for a crypt's builders and a relative's sketchbook from the ruins",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let found = find_village(world, frontier_village);
                set_time(world, DayTime::at(2, 16, 0));
                park(
                    world,
                    player,
                    i32::midpoint(found.plaza.0, found.plaza.1),
                    found.ground_row,
                );
                let _ = tomb_story(world, 901, 1);
                let _ = tomb_story(world, 902, 0);
                let _ = tomb_story(world, 908, 2);
            });
            stage.settle(2);
            let seat = stage.seat(0);
            seat.village.banner = None;
            seat.modal = Some(ModalScreen::Journal);
            seat.selected_menu_item = 0;
            seat.journal.focus = 0;
            stage.settle(3);
        },
    },
];
