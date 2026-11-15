//! Village scenes: surface villages of several cultures and landscapes, an underground
//! settlement, building interiors (a counter, a house conversation, the meeting hall), the
//! arrival banner, a signpost and the mine map with village markers.

use std::sync::Arc;

use fatherload_simulation::{
    PlayerId, WorldState,
    people::Culture,
    terrain::{MineralKind, TILE_SIZE},
    village::{self, BuildingKind, Specialty, Village, VillageSetting, placement},
};

use super::{Scene, SceneStage, reveal};
use crate::shell::{ModalScreen, VILLAGE_COUNTER_X, VillageInterior};

/// The first village (from site 1 outward, east and west) accepted by `wanted`.
fn find_village(world: &WorldState, wanted: impl Fn(&Village) -> bool) -> Arc<Village> {
    let settings = *world.world_settings();
    for site in (1..80).flat_map(|site| [site, -site]) {
        for id in [placement::surface_id(site), placement::underground_id(site)] {
            if let Some(found) = village::village(&settings, id)
                && wanted(&found)
            {
                return found;
            }
        }
    }
    panic!("no such village within eighty sites");
}

fn surface_of(culture: Culture) -> impl Fn(&Village) -> bool {
    move |found: &Village| found.culture == culture && found.setting == VillageSetting::Surface
}

/// Parks `player`'s rig on the village ground at `column`, with some credits and cargo.
fn park_in(world: &mut WorldState, player: PlayerId, found: &Village, column: i32) {
    if let Some(rig) = world.player_mut(player) {
        #[allow(clippy::cast_precision_loss, reason = "tile indices")]
        {
            rig.x = (column as f32 + 0.5) * TILE_SIZE;
            rig.y = (found.ground_row as f32 - 0.5) * TILE_SIZE;
        }
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.x_remainder = 0.0;
        rig.credits = 3_400;
        rig.fuel = rig.fuel_capacity * 0.4;
        rig.hull = rig.max_hull() * 0.65;
        rig.cargo_bay.add(MineralKind::Silver, 3);
        rig.cargo_bay.add(MineralKind::Gold, 2);
        rig.cargo_bay.add(MineralKind::Copper, 5);
        if let Specialty::MineralBuyer(mineral) = found.specialty {
            rig.cargo_bay.add(mineral, 2);
        }
    }
    let area = found.area();
    reveal(
        world,
        player,
        area.min.x - 20..area.max.x + 20,
        area.min.y - 4..area.max.y + 6,
    );
}

/// A street scene of the first surface village of `culture`, the rig at its plaza.
fn street(stage: &mut SceneStage<'_>, wanted: impl Fn(&Village) -> bool, offset: i32) {
    stage.play(1, |world, seats| {
        let found = find_village(world, wanted);
        let column = i32::midpoint(found.plaza.0, found.plaza.1) + offset;
        park_in(world, seats[0].player, &found, column);
        if let Some(rig) = world.player_mut(seats[0].player) {
            rig.villages.visited.insert(found.id);
        }
    });
    stage.settle(20);
}

/// Puts seat 0 inside building `building` of `found`, at `x` room units.
fn inside(stage: &mut SceneStage<'_>, found: &Village, building: usize, x: f32) {
    let seat = stage.seat(0);
    seat.village.interior = Some(VillageInterior {
        village: found.id,
        building: u8::try_from(building).expect("building index"),
    });
    seat.interior_x = x;
    seat.interior_facing = 1.0;
}

/// Enters the first village building of `kind` (in a village accepted by `wanted`).
fn interior(
    stage: &mut SceneStage<'_>,
    wanted: impl Fn(&Village) -> bool,
    kind: BuildingKind,
    modal: Option<ModalScreen>,
) -> (Arc<Village>, usize) {
    let mut chosen = None;
    stage.play(1, |world, seats| {
        let found = find_village(world, |found| {
            wanted(found) && found.buildings.iter().any(|building| building.kind == kind)
        });
        let building = found
            .buildings
            .iter()
            .position(|lot| lot.kind == kind)
            .expect("building");
        park_in(
            world,
            seats[0].player,
            &found,
            found.buildings[building].first_column + 2,
        );
        chosen = Some((found, building));
    });
    stage.settle(5);
    let (found, building) = chosen.expect("staged");
    let x = if found.buildings[building].keeper.is_some() {
        VILLAGE_COUNTER_X
    } else {
        150.0
    };
    inside(stage, &found, building, x);
    stage.seat(0).modal = modal;
    stage.seat(0).selected_menu_item = 0;
    (found, building)
}

pub const VILLAGE_SCENES: &[Scene] = &[
    Scene {
        name: "village_frontier",
        summary: "A Frontier village of timber and tin on the plains: plaza well, market awning, houses with chimneys, residents about",
        setup: |stage| street(stage, surface_of(Culture::Frontier), 0),
    },
    Scene {
        name: "village_highland",
        summary: "A Highland village of dry stone in the hills: statue on the plaza, slate roofs, a lookout tower",
        setup: |stage| street(stage, surface_of(Culture::Highland), 0),
    },
    Scene {
        name: "village_mesa",
        summary: "A Mesa town of adobe: flat roofs and vigas, painted doors, a fountain and cacti",
        setup: |stage| street(stage, surface_of(Culture::Mesa), 0),
    },
    Scene {
        name: "village_riverside",
        summary: "A Riverside village of green-roofed timber among the trees, with a totem on its plaza",
        setup: |stage| street(stage, surface_of(Culture::Riverside), 0),
    },
    Scene {
        name: "village_scrapper",
        summary: "A Scrapper camp of corrugated sheets: bonfire plaza, a mine head and crates",
        setup: |stage| street(stage, surface_of(Culture::Scrapper), 0),
    },
    Scene {
        name: "village_underground",
        summary: "A Deepfolk settlement in a carved hall deep in the strata: rock-cut fronts, lamps and crystals",
        setup: |stage| {
            street(
                stage,
                |found| found.setting == VillageSetting::Underground,
                0,
            );
        },
    },
    Scene {
        name: "village_shrine",
        summary: "A village shrine with its tiered roof, bell and prayer flags",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let found = find_village(world, |found| {
                    found.specialty == Specialty::Shrine && found.setting == VillageSetting::Surface
                });
                let shrine = found
                    .buildings
                    .iter()
                    .find(|lot| lot.kind == BuildingKind::Shrine)
                    .expect("shrine");
                park_in(world, seats[0].player, &found, shrine.first_column - 2);
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "village_signpost",
        summary: "A village edge: the signpost pointing back to town and on to the next village, the landing pad",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let found = find_village(world, |found| found.site == 1);
                let post = found
                    .signposts
                    .iter()
                    .find(|post| post.direction < 0)
                    .expect("west post");
                park_in(world, seats[0].player, &found, post.column + 3);
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "village_arrival_banner",
        summary: "Driving into a village for the first time: the discovery banner with its name, style and specialty",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let found = find_village(world, |found| found.site == -1);
                park_in(world, seats[0].player, &found, found.first_column + 4);
            });
            stage.settle(30);
        },
    },
    Scene {
        name: "village_trading_post",
        summary: "Inside a village trading post: the counter's deals (cargo at village prices, fuel) and the keeper",
        setup: |stage| {
            interior(
                stage,
                |found| found.setting == VillageSetting::Surface,
                BuildingKind::Market,
                Some(ModalScreen::VillageCounter),
            );
            stage.settle(4);
        },
    },
    Scene {
        name: "village_assay_office",
        summary: "An assay office paying well over the depot price for its one mineral",
        setup: |stage| {
            interior(
                stage,
                |found| matches!(found.specialty, Specialty::MineralBuyer(_)),
                BuildingKind::Assay,
                Some(ModalScreen::VillageCounter),
            );
            stage.settle(4);
        },
    },
    Scene {
        name: "village_house",
        summary: "Inside a village house: the family at home, the room dressed in the village's style",
        setup: |stage| {
            interior(
                stage,
                |found| found.culture == Culture::Highland,
                BuildingKind::House,
                None,
            );
            stage.settle(4);
        },
    },
    Scene {
        name: "village_talk",
        summary: "Talking to a villager: personality-driven lines from the dialogue system",
        setup: |stage| {
            let (found, building) = interior(
                stage,
                |found| found.culture == Culture::Mesa,
                BuildingKind::House,
                None,
            );
            let x = crate::shell::room_residents(&found, building)
                .first()
                .map_or(200.0, |(_, x)| *x);
            stage.seat(0).interior_x = x - 20.0;
            stage.hold(
                1,
                crate::input::PlayerInput {
                    interact: true,
                    ..crate::input::PlayerInput::default()
                },
            );
            stage.settle(4);
        },
    },
    Scene {
        name: "village_hall",
        summary: "The meeting hall's counter: prosperity, the village's trouble, its neighbours and the elder",
        setup: |stage| {
            interior(
                stage,
                |found| found.setting == VillageSetting::Surface,
                BuildingKind::Hall,
                Some(ModalScreen::VillageCounter),
            );
            stage.settle(4);
        },
    },
    Scene {
        name: "village_map",
        summary: "The mine map near two villages: village markers with names, a charted one beyond",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let found = find_village(world, |found| found.site == 1);
                park_in(world, player, &found, found.last_column + 30);
                let next = placement::surface_id(2);
                if let Some(rig) = world.player_mut(player) {
                    rig.villages.visited.insert(found.id);
                    rig.villages.charted.insert(next);
                }
            });
            stage.settle(5);
            stage.seat(0).modal = Some(ModalScreen::Map);
            stage.settle(2);
        },
    },
];
