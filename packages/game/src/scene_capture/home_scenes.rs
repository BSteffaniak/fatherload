//! Contractor-built home scenes (WP-HOME): the land office's plot list and the survey screen,
//! the site board with a builder's request, each construction stage with the crew at work,
//! a finished cabin and a manor at dusk, decorated rooms with trophy cases, the garage bay with
//! the rig parked in it, decorating, a bedroom at night, and a co-op visit in split-screen.

use fatherload_simulation::{
    PlayerId, TILE_SIZE, WorldState,
    economy::SurfaceZone,
    home::{
        Blueprint, BuildStage, Construction, Expansion, Furniture, HomeJob, PlotId, PlotSetting,
        PlotSite, PlotSize, RoomKind, SiteEvent, SiteEventKind, decor::PlacedDecor, plot,
    },
    item::{BlockKind, ItemKind},
    terrain::{ArtifactKind, MineralKind},
};

use super::{Scene, SceneStage, park_at, put_seat_inside, reveal};
use crate::shell::homes::{HOME_DOOR_X, HomeInterior, HomeScreen};

/// World time at day slice `slice` (0..96) of day `day`.
fn time_at(world: &WorldState, day: u64, slice: f32) -> u64 {
    let length = world.day_length().millis();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "a slice of a day in milliseconds"
    )]
    let into_day = (f64::from(slice) * length as f64 / 96.0) as u64;
    day * length + into_day
}

/// The listed site nearest the town matching `wanted`.
fn site_where(world: &WorldState, wanted: impl Fn(&PlotSite) -> bool) -> PlotSite {
    let settings = *world.world_settings();
    let mut sites = plot::listed_sites_between(&settings, -6_000, 6_000);
    sites.sort_by_key(|site| (site.middle().abs(), site.left));
    sites
        .iter()
        .copied()
        .find(|site| wanted(site))
        .or_else(|| sites.first().copied())
        .expect("listed sites")
}

#[allow(clippy::cast_precision_loss, reason = "tile indices")]
fn column_px(column: i32) -> f32 {
    (column as f32 + 0.5) * TILE_SIZE
}

/// Parks `player`'s rig on the plot's ground `offset` columns right of its middle.
fn park_on(world: &mut WorldState, player: PlayerId, site: &PlotSite, offset: i32) {
    if let Some(rig) = world.player_mut(player) {
        rig.x = column_px(site.middle() + offset);
        #[allow(clippy::cast_precision_loss, reason = "tile indices")]
        {
            rig.y = (site.ground_row as f32 - 0.5) * TILE_SIZE;
        }
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
    reveal(
        world,
        player,
        site.left - 30..site.right() + 30,
        site.ground_row - 24..site.ground_row + 6,
    );
}

/// The pilot out on foot at `column` on the plot's ground.
fn pilot_at(world: &mut WorldState, player: PlayerId, site: &PlotSite, column: i32) {
    #[allow(clippy::cast_precision_loss, reason = "tile indices")]
    let feet = site.ground_row as f32 * TILE_SIZE;
    let _ = world.place_pilot(player, column_px(column), feet);
}

/// Stakes `site` for `player`, with a finished `blueprint` home when given.
fn home_on(
    world: &mut WorldState,
    player: PlayerId,
    site: PlotSite,
    blueprint: Option<Blueprint>,
) -> PlotId {
    world.scenario_home(player, site, blueprint)
}

/// Puts a contract for `blueprint` on `plot`, `share` (0..1) of the way through `stage`.
fn under_way(
    world: &mut WorldState,
    player: PlayerId,
    plot: PlotId,
    blueprint: Blueprint,
    stage: BuildStage,
    share: f32,
) {
    let now = world.world_millis();
    let offer = world
        .contractors_for(plot, player)
        .into_iter()
        .next()
        .expect("a contractor");
    let size = world
        .homes()
        .plot(plot)
        .map_or(PlotSize::Medium, |plot| plot.site.size);
    let quote = offer.profile.quote(HomeJob::Build(blueprint).job(size));
    let total = u64::from(quote.tenth_days) * world.day_length().millis() / 10;
    let next = BuildStage::ALL
        .iter()
        .copied()
        .find(|later| *later > stage)
        .unwrap_or(BuildStage::Done);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "per mille"
    )]
    let progress = stage.start() + ((next.start() - stage.start()) as f32 * share) as u32;
    let worked = total * u64::from(progress) / 1_000;
    world.scenario_edit_plot(plot, |found| {
        found.construction = Some(Construction {
            job: HomeJob::Build(blueprint),
            contractor: offer.profile,
            guild: offer.character.is_none(),
            quote,
            price_paid: quote.price,
            supplied_blocks: 0,
            started_millis: now.saturating_sub(worked),
            total_millis: total.max(1),
            rushed_millis: 0,
            held_millis: 0,
            hold_since: None,
            planned: Vec::new(),
            events: Vec::new(),
            announced: BuildStage::Survey,
            bonus_quality: 0,
        });
    });
}

/// A construction scene: a medium village-edge plot at 10:00 with `blueprint` at `stage`, the
/// pilot on foot watching and the crew at work.
fn construction_scene(stage: &mut SceneStage<'_>, blueprint: Blueprint, at: BuildStage) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let site = site_where(world, |site| {
            matches!(site.setting, PlotSetting::Village(_)) && site.size >= PlotSize::Medium
        });
        let site = if site.size >= blueprint.min_plot() {
            site
        } else {
            site_where(world, |site| site.size >= blueprint.min_plot())
        };
        world.set_world_time_millis(time_at(world, 2, 40.0));
        let plot = home_on(world, player, site, None);
        under_way(world, player, plot, blueprint, at, 0.55);
        park_on(world, player, &site, site.width() / 2 + 4);
        pilot_at(world, player, &site, site.right() + 1);
        world.stream_terrain_near_players();
    });
    // The crew arrives on the next refresh and spreads along the house.
    stage.settle(90);
    stage.seat(0).village.banner = None;
    stage.settle(1);
}

/// A finished `blueprint` on a site matching `wanted`, at day slice `slice`, the pilot outside.
fn finished_scene(
    stage: &mut SceneStage<'_>,
    blueprint: Blueprint,
    wanted: fn(&PlotSite) -> bool,
    slice: f32,
) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let site = site_where(world, wanted);
        world.set_world_time_millis(time_at(world, 3, slice));
        let plot = home_on(world, player, site, Some(blueprint));
        world.scenario_edit_plot(plot, |found| {
            // A lamp in every room: the windows glow.
            if let Some(home) = found.home.as_mut() {
                for room in &mut home.rooms {
                    room.decor.pieces.entry(8 + 7).or_insert(PlacedDecor {
                        furniture: Furniture::WallLamp,
                        display: None,
                    });
                }
            }
        });
        park_on(world, player, &site, site.width() / 2 + 6);
        let door = world.homes().plot(plot).map_or_else(
            || site.middle(),
            fatherload_simulation::home::Plot::door_column,
        );
        pilot_at(world, player, &site, door - 3);
        world.stream_terrain_near_players();
    });
    stage.settle(30);
    stage.seat(0).village.banner = None;
    stage.settle(1);
}

/// Inside `plot`'s room `room` at room units `x`, the pilot at the door outside.
fn inside(stage: &mut SceneStage<'_>, seat: usize, plot: PlotId, room: usize, x: f32) {
    let seat_ui = stage.seat(seat);
    seat_ui.home.interior = Some(HomeInterior {
        plot,
        room,
        x,
        facing: 1.0,
        stride: 0.0,
    });
    seat_ui.modal = None;
    seat_ui.village.banner = None;
}

/// A manor on a large site with its gallery dressed: trophy cases showing a star core and a
/// diamond, a banner, a rug, plants and the chandelier lit. Returns the plot.
fn trophy_manor(world: &mut WorldState, player: PlayerId) -> (PlotId, PlotSite) {
    let site = site_where(world, |site| site.size >= PlotSize::Large);
    let plot = home_on(world, player, site, Some(Blueprint::Manor));
    world.scenario_edit_plot(plot, |found| {
        let Some(home) = found.home.as_mut() else {
            return;
        };
        if let Some(gallery) = home
            .rooms
            .iter_mut()
            .find(|room| room.kind == RoomKind::Gallery)
        {
            let pieces = &mut gallery.decor.pieces;
            pieces.insert(
                2,
                PlacedDecor {
                    furniture: Furniture::TrophyCase,
                    display: Some(ItemKind::Artifact(ArtifactKind::StarCore)),
                },
            );
            pieces.insert(
                5,
                PlacedDecor {
                    furniture: Furniture::TrophyCase,
                    display: Some(ItemKind::Mineral(MineralKind::Diamond)),
                },
            );
            pieces.insert(
                3,
                PlacedDecor {
                    furniture: Furniture::TrophyCase,
                    display: Some(ItemKind::Artifact(ArtifactKind::VoidPearl)),
                },
            );
            pieces.insert(
                0,
                PlacedDecor {
                    furniture: Furniture::Plant,
                    display: None,
                },
            );
            pieces.insert(
                7,
                PlacedDecor {
                    furniture: Furniture::Armchair,
                    display: None,
                },
            );
            pieces.insert(
                8 + 3,
                PlacedDecor {
                    furniture: Furniture::Banner,
                    display: None,
                },
            );
            pieces.insert(
                8,
                PlacedDecor {
                    furniture: Furniture::Painting,
                    display: None,
                },
            );
            pieces.insert(
                8 + 7,
                PlacedDecor {
                    furniture: Furniture::MountedPick,
                    display: None,
                },
            );
            gallery.decor.rugs.insert(
                2,
                PlacedDecor {
                    furniture: Furniture::Rug,
                    display: None,
                },
            );
            gallery.decor.wallpaper = fatherload_simulation::home::Wallpaper::Midnight;
            gallery.decor.flooring = fatherload_simulation::home::Flooring::Parquet;
        }
    });
    (plot, site)
}

pub const HOME_SCENES: &[Scene] = &[
    Scene {
        name: "home_sites_for_sale",
        summary: "East of town by day: the land office's plots marked out with survey pegs and FOR SALE boards showing their price",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let site = site_where(world, |site| {
                    site.setting == PlotSetting::Town && site.left > 0
                });
                world.set_world_time_millis(time_at(world, 2, 42.0));
                park_on(world, player, &site, 0);
                world.stream_terrain_near_players();
            });
            stage.settle(6);
        },
    },
    Scene {
        name: "home_land_office",
        summary: "The town HQ's land office: plots for sale around town and the villages with their sizes, settings and prices, the catalogue row",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_at(world, seats[0].player, SurfaceZone::Headquarters);
                if let Some(rig) = world.player_mut(seats[0].player) {
                    rig.credits = 9_000;
                }
            });
            put_seat_inside(stage, 0, SurfaceZone::Headquarters, None);
            stage.seat(0).open_home_screen(HomeScreen::LandOffice, None);
            stage.settle(4);
        },
    },
    Scene {
        name: "home_survey",
        summary: "Surveying a spot of open meadow far from town: the four plot sizes priced by distance and levelling",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let settings = *world.world_settings();
                let column = (900..6_000)
                    .step_by(31)
                    .find(|column| {
                        let row = fatherload_simulation::worldgen::surface_row(&settings, *column);
                        if let Some(rig) = world.player_mut(player) {
                            rig.x = column_px(*column);
                            #[allow(clippy::cast_precision_loss, reason = "tile indices")]
                            {
                                rig.y = (row as f32 - 0.5) * TILE_SIZE;
                            }
                        }
                        world.survey_plot(player, PlotSize::Large).is_ok()
                    })
                    .unwrap_or(900);
                let row = fatherload_simulation::worldgen::surface_row(&settings, column);
                let site = PlotSite {
                    key: fatherload_simulation::home::SiteKey::Surveyed,
                    size: PlotSize::Small,
                    setting: PlotSetting::Wilds,
                    left: column,
                    ground_row: row,
                    price: 0,
                };
                park_on(world, player, &site, 0);
                if let Some(rig) = world.player_mut(player) {
                    rig.credits = 6_000;
                }
                world.set_world_time_millis(time_at(world, 2, 44.0));
                world.stream_terrain_near_players();
            });
            stage.settle(2);
            stage.seat(0).open_home_screen(HomeScreen::Survey, None);
            stage.settle(4);
        },
    },
    Scene {
        name: "home_site_board",
        summary: "The site board of a house going up: the walls stage at 60%, the contractor short of clay blocks with bring or pay rows, the rush row and the deed",
        setup: |stage| {
            construction_scene(stage, Blueprint::House, BuildStage::Walls);
            let plot = stage.world().homes().plots().next().map(|plot| plot.id);
            if let Some(plot) = plot {
                stage.play_on(|world| {
                    let now = world.world_millis();
                    world.scenario_edit_plot(plot, |found| {
                        if let Some(work) = found.construction.as_mut() {
                            work.events.push(SiteEvent {
                                kind: SiteEventKind::Shortage {
                                    block: BlockKind::Clay,
                                    count: 12,
                                },
                                stage: BuildStage::Walls,
                                at_millis: now,
                                answered: false,
                            });
                            work.hold_since = Some(now);
                            work.announced = BuildStage::Walls;
                        }
                    });
                });
                stage.seat(0).open_home_screen(HomeScreen::Site, Some(plot));
            }
            stage.settle(4);
        },
    },
    Scene {
        name: "home_contract",
        summary: "Choosing a builder for a cottage: the village's contractors with their style, time, price and quality, the supply-blocks toggle and the contract to sign",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let site = site_where(world, |site| {
                    matches!(site.setting, PlotSetting::Village(_))
                });
                world.set_world_time_millis(time_at(world, 2, 40.0));
                let _ = home_on(world, player, site, None);
                if let Some(rig) = world.player_mut(player) {
                    rig.credits = 12_000;
                    rig.block_hopper.add(ItemKind::Block(BlockKind::Dirt), 30);
                }
                park_on(world, player, &site, 0);
                pilot_at(world, player, &site, site.middle() + 2);
                world.stream_terrain_near_players();
            });
            stage.settle(2);
            let plot = stage.world().homes().plots().next().map(|plot| plot.id);
            stage.seat(0).open_home_screen(HomeScreen::Site, plot);
            stage.seat(0).open_home_screen(
                HomeScreen::Contract(HomeJob::Build(Blueprint::Cottage)),
                plot,
            );
            stage.seat(0).home.draft.supply = true;
            stage.settle(4);
        },
    },
    Scene {
        name: "home_stage_survey",
        summary: "Construction, day one: survey stakes, string lines and the surveyor's tripod on a village-edge plot, the contractor on site",
        setup: |stage| construction_scene(stage, Blueprint::House, BuildStage::Survey),
    },
    Scene {
        name: "home_stage_foundation",
        summary: "The foundation poured across the footprint, lumber and brick pallets stacked, the mixer turning, workers in hard hats",
        setup: |stage| construction_scene(stage, Blueprint::House, BuildStage::Foundation),
    },
    Scene {
        name: "home_stage_frame",
        summary: "Scaffolding up and the timber frame rising, the crew hammering",
        setup: |stage| construction_scene(stage, Blueprint::House, BuildStage::Frame),
    },
    Scene {
        name: "home_stage_walls",
        summary: "Brick walls halfway up inside the scaffold and the frame",
        setup: |stage| construction_scene(stage, Blueprint::House, BuildStage::Walls),
    },
    Scene {
        name: "home_stage_roof",
        summary: "The walls done and the slate roof going on, a crane swinging tiles up",
        setup: |stage| construction_scene(stage, Blueprint::House, BuildStage::Roof),
    },
    Scene {
        name: "home_stage_finishing",
        summary: "Finishing touches: the house whole but unlit, a ladder against it, paint pots and a fresh coat going on",
        setup: |stage| construction_scene(stage, Blueprint::House, BuildStage::Finishing),
    },
    Scene {
        name: "home_cabin_dusk",
        summary: "A finished log cabin at dusk on the edge of town: a gable roof, chimney smoke, its window glowing, the fence and flowers, the pilot home",
        setup: |stage| {
            finished_scene(
                stage,
                Blueprint::Cabin,
                |site| site.setting == PlotSetting::Town,
                75.5,
            );
        },
    },
    Scene {
        name: "home_manor_dusk",
        summary: "A stone manor on a large scenic plot at dusk: towers, dormers, a columned porch, a garage door, every window lit, trees and a hedge",
        setup: |stage| {
            finished_scene(
                stage,
                Blueprint::Manor,
                |site| site.size >= PlotSize::Large,
                76.0,
            );
        },
    },
    Scene {
        name: "home_interior_trophies",
        summary: "Inside the manor's gallery: three trophy cases showing a star core, a void pearl and a diamond, a banner and a mounted pick, midnight wallpaper, parquet and the chandelier",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                world.set_world_time_millis(time_at(world, 2, 52.0));
                let (_, site) = trophy_manor(world, player);
                park_on(world, player, &site, site.width() / 2 + 6);
                let door = world.homes().plots().next().map_or_else(
                    || site.middle(),
                    fatherload_simulation::home::Plot::door_column,
                );
                pilot_at(world, player, &site, door);
            });
            stage.settle(2);
            let found = stage.world().homes().plots().next().map(|plot| {
                let gallery = plot.home.as_ref().and_then(|home| {
                    home.rooms
                        .iter()
                        .position(|room| room.kind == RoomKind::Gallery)
                });
                (plot.id, gallery.unwrap_or(0))
            });
            if let Some((plot, room)) = found {
                inside(stage, 0, plot, room, 300.0);
            }
            stage.settle(3);
        },
    },
    Scene {
        name: "home_garage_bay",
        summary: "A workshop home's garage bay: the rig parked inside under the roll-up door getting patched up, the tool board and the oil stain",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                world.set_world_time_millis(time_at(world, 2, 46.0));
                let site = site_where(world, |site| site.size >= PlotSize::Medium);
                let plot = home_on(world, player, site, Some(Blueprint::WorkshopHome));
                let (first, _) = world
                    .homes()
                    .plot(plot)
                    .and_then(fatherload_simulation::home::Plot::garage_columns)
                    .unwrap_or((site.left, site.left + 4));
                park_on(world, player, &site, first + 1 - site.middle());
                if let Some(rig) = world.player_mut(player) {
                    rig.hull = rig.max_hull() * 0.4;
                }
                let door = world.homes().plot(plot).map_or_else(
                    || site.middle(),
                    fatherload_simulation::home::Plot::door_column,
                );
                pilot_at(world, player, &site, door);
            });
            stage.settle(2);
            let plot = stage.world().homes().plots().next().map(|plot| plot.id);
            if let Some(plot) = plot {
                inside(stage, 0, plot, 0, 470.0);
            }
            stage.settle(3);
        },
    },
    Scene {
        name: "home_decorating",
        summary: "Decorating a cottage's sitting room: choosing a spot for a new bookshelf, its ghost glowing where it would stand",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                world.set_world_time_millis(time_at(world, 2, 50.0));
                let site = site_where(world, |site| site.setting == PlotSetting::Town);
                let plot = home_on(world, player, site, Some(Blueprint::Cottage));
                world.grant_furniture(player, Furniture::Bookshelf, 1);
                world.grant_furniture(player, Furniture::Rug, 2);
                world.grant_furniture(player, Furniture::Lantern, 1);
                park_on(world, player, &site, site.width() / 2 + 3);
                let door = world.homes().plot(plot).map_or_else(
                    || site.middle(),
                    fatherload_simulation::home::Plot::door_column,
                );
                pilot_at(world, player, &site, door);
            });
            stage.settle(2);
            let plot = stage.world().homes().plots().next().map(|plot| plot.id);
            if let Some(plot) = plot {
                inside(stage, 0, plot, 0, 200.0);
                stage
                    .seat(0)
                    .open_home_screen(HomeScreen::Decor, Some(plot));
                stage
                    .seat(0)
                    .open_home_screen(HomeScreen::DecorSpot(Furniture::Bookshelf), Some(plot));
                stage.settle(2);
                stage.seat(0).selected_menu_item = 2;
            }
            stage.settle(2);
        },
    },
    Scene {
        name: "home_bedroom_night",
        summary: "A house's bedroom at night: the starry sky and the Moon in the windows, the wall lamp's warm pool, the pilot by the bed",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                world.set_world_time_millis(time_at(world, 2, 88.0));
                let site = site_where(world, |site| site.size >= PlotSize::Medium);
                let plot = home_on(world, player, site, Some(Blueprint::House));
                park_on(world, player, &site, site.width() / 2 + 3);
                let door = world.homes().plot(plot).map_or_else(
                    || site.middle(),
                    fatherload_simulation::home::Plot::door_column,
                );
                pilot_at(world, player, &site, door);
            });
            stage.settle(2);
            let found = stage.world().homes().plots().next().map(|plot| {
                let room = plot.home.as_ref().and_then(|home| {
                    home.rooms
                        .iter()
                        .position(|room| room.kind == RoomKind::Bedroom)
                });
                (plot.id, room.unwrap_or(0))
            });
            if let Some((plot, room)) = found {
                inside(stage, 0, plot, room, 230.0);
            }
            stage.settle(3);
        },
    },
    Scene {
        name: "home_basement_lift",
        summary: "A cabin's basement: the hatch down into the cellar and the private lift cage to the base below, pipes along the ceiling",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                world.set_world_time_millis(time_at(world, 2, 50.0));
                let site = site_where(world, |site| site.setting == PlotSetting::Town);
                let plot = home_on(world, player, site, Some(Blueprint::Cabin));
                world.scenario_expand(plot, Expansion::Basement);
                world.scenario_expand(plot, Expansion::Lift);
                park_on(world, player, &site, site.width() / 2 + 3);
                let door = world.homes().plot(plot).map_or_else(
                    || site.middle(),
                    fatherload_simulation::home::Plot::door_column,
                );
                pilot_at(world, player, &site, door);
            });
            stage.settle(2);
            let found = stage.world().homes().plots().next().map(|plot| {
                let room = plot
                    .home
                    .as_ref()
                    .and_then(|home| home.rooms.iter().position(|room| room.floor < 0));
                (plot.id, room.unwrap_or(0))
            });
            if let Some((plot, room)) = found {
                inside(stage, 0, plot, room, 300.0);
            }
            stage.settle(3);
        },
    },
    Scene {
        name: "home_split_visit",
        summary: "Split-screen co-op: player 1 inside the cottage's sitting room, player 2 outside on the plot by the front door",
        setup: |stage| {
            stage.play(2, |world, seats| {
                world.set_world_time_millis(time_at(world, 2, 60.0));
                let site = site_where(world, |site| site.setting == PlotSetting::Town);
                let plot = home_on(world, seats[0].player, site, Some(Blueprint::Cottage));
                let door = world.homes().plot(plot).map_or_else(
                    || site.middle(),
                    fatherload_simulation::home::Plot::door_column,
                );
                park_on(world, seats[0].player, &site, site.width() / 2 + 2);
                park_on(world, seats[1].player, &site, -site.width() / 2 - 3);
                pilot_at(world, seats[0].player, &site, door);
                pilot_at(world, seats[1].player, &site, door + 2);
            });
            stage.settle(2);
            let plot = stage.world().homes().plots().next().map(|plot| plot.id);
            if let Some(plot) = plot {
                inside(stage, 0, plot, 0, HOME_DOOR_X + 120.0);
            }
            stage.settle(3);
        },
    },
];
