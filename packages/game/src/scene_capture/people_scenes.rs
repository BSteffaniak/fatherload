//! Screenshot scenes for people in the world (WP-PEOPLE): a busy plaza at lunch, the inn of an
//! evening, the talk panel, a trade list, a gift list, and the folk of several cultures and the
//! town walking their streets.

use std::sync::Arc;

use fatherload_simulation::{
    PlayerId, SimulationTick, WorldState,
    entity::{EntityKind, EntityState},
    people::{
        Character, Culture, DayTime, TickClock, clock::DEFAULT_DAY_LENGTH_MINUTES, offers, routine,
    },
    terrain::{MineralKind, TILE_SIZE},
    village::{self, BuildingKind, Village, VillageSetting, placement},
};

use super::{Scene, SceneStage, reveal};
use crate::shell::{ModalScreen, VillageInterior};

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

/// Jumps the world clock to `time` (the people clock is derived from the tick).
const fn set_time(world: &mut WorldState, time: DayTime) {
    let tick = TickClock::tick_at(time, world.tick_rate_hz(), DEFAULT_DAY_LENGTH_MINUTES);
    world.set_simulation_tick(SimulationTick::new(tick));
}

#[allow(clippy::cast_precision_loss, reason = "tile indices")]
fn park(world: &mut WorldState, player: PlayerId, x: f32, ground_row: i32) {
    if let Some(rig) = world.player_mut(player) {
        rig.x = x;
        rig.y = (ground_row as f32 - 0.5) * TILE_SIZE;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.x_remainder = 0.0;
        rig.credits = 4_200;
        rig.cargo_bay.add(MineralKind::Silver, 3);
        rig.cargo_bay.add(MineralKind::Gold, 2);
        rig.cargo_bay.add(MineralKind::Ruby, 1);
    }
}

#[allow(clippy::cast_precision_loss, reason = "tile indices")]
fn column_x(column: i32) -> f32 {
    (column as f32 + 0.5) * TILE_SIZE
}

/// A street scene in the first village `wanted` accepts, at `time`, the rig `offset` columns
/// from the plaza's middle.
fn street_at(
    stage: &mut SceneStage<'_>,
    wanted: impl Fn(&Village) -> bool,
    time: DayTime,
    offset: i32,
) {
    stage.play(1, |world, seats| {
        let found = find_village(world, wanted);
        set_time(world, time);
        let column = i32::midpoint(found.plaza.0, found.plaza.1) + offset;
        park(world, seats[0].player, column_x(column), found.ground_row);
        if let Some(rig) = world.player_mut(seats[0].player) {
            rig.villages.visited.insert(found.id);
        }
        let area = found.area();
        reveal(
            world,
            seats[0].player,
            area.min.x - 20..area.max.x + 20,
            area.min.y - 4..area.max.y + 6,
        );
        world.stream_terrain_near_players();
    });
    // Population refreshes every half second; then let them walk a little.
    stage.settle(150);
    stage.seat(0).village.banner = None;
    stage.settle(1);
}

/// The person outdoors nearest seat 0's rig that `wanted` accepts (anyone, if nobody), and the
/// rig moved beside them.
fn approach_someone(
    stage: &mut SceneStage<'_>,
    wanted: fn(&Character) -> bool,
) -> Option<fatherload_simulation::entity::EntityId> {
    let world = stage.world();
    let settings = *world.world_settings();
    let player = world.player_ids().next()?;
    let rig_x = world.player(player)?.x;
    let outdoors: Vec<(fatherload_simulation::entity::EntityId, f32, bool)> = world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter_map(|entity| match entity.state {
            EntityState::Npc(npc) if npc.inside.is_none() => {
                let fits = routine::character(&settings, npc.character)
                    .is_some_and(|character| wanted(&character));
                Some((entity.id, npc.x_at(world.simulation_tick(), 0.0), fits))
            }
            _ => None,
        })
        .collect();
    #[allow(clippy::cast_possible_truncation, reason = "pixel distances")]
    let (npc, x, _) = outdoors
        .iter()
        .copied()
        .min_by_key(|(_, x, fits)| (!*fits, (x - rig_x).abs() as i64))?;
    let mut moved = None;
    stage.play_on(|world| {
        if let Some(rig) = world.player_mut(player) {
            rig.x = x;
            moved = Some(());
        }
    });
    moved.map(|()| npc)
}

/// Opens a conversation with the person nearest seat 0's rig (that `wanted` accepts) by
/// pressing the talk key.
fn open_talk(stage: &mut SceneStage<'_>, wanted: fn(&Character) -> bool) {
    let _ = approach_someone(stage, wanted);
    stage.settle(2);
    stage.hold(
        1,
        crate::input::PlayerInput {
            interact: true,
            ..crate::input::PlayerInput::default()
        },
    );
    stage.settle(4);
}

pub const PEOPLE_SCENES: &[Scene] = &[
    Scene {
        name: "people_plaza_day",
        summary: "A village plaza at lunchtime: residents walking in from work, standing about the well, each a procedural figure in their culture's cloth",
        setup: |stage| {
            street_at(
                stage,
                |found| {
                    found.culture == Culture::Frontier && found.setting == VillageSetting::Surface
                },
                DayTime::at(1, 12, 20),
                0,
            );
        },
    },
    Scene {
        name: "people_inn_evening",
        summary: "The inn on an evening: the innkeeper behind the bar and the regulars in from their day",
        setup: |stage| {
            let mut chosen = None;
            stage.play(1, |world, seats| {
                let found = find_village(world, |found| {
                    found.setting == VillageSetting::Surface
                        && found.population.characters.len() >= 7
                        && found
                            .buildings
                            .iter()
                            .any(|lot| lot.kind == BuildingKind::Inn)
                });
                set_time(world, DayTime::at(1, 20, 40));
                let inn = found
                    .buildings
                    .iter()
                    .position(|lot| lot.kind == BuildingKind::Inn)
                    .expect("inn");
                park(
                    world,
                    seats[0].player,
                    column_x(found.buildings[inn].first_column + 2),
                    found.ground_row,
                );
                chosen = Some((found, inn));
            });
            stage.settle(90);
            let (found, inn) = chosen.expect("staged");
            let seat = stage.seat(0);
            seat.village.banner = None;
            seat.village.interior = Some(VillageInterior {
                village: found.id,
                building: u8::try_from(inn).expect("index"),
            });
            seat.interior_x = 110.0;
            seat.interior_facing = 1.0;
            stage.settle(3);
        },
    },
    Scene {
        name: "people_dialogue",
        summary: "Talking to a villager on the street: their portrait, the conversation from the dialogue system, how they see you and what you can say",
        setup: |stage| {
            street_at(
                stage,
                |found| {
                    found.culture == Culture::Highland && found.setting == VillageSetting::Surface
                },
                DayTime::at(1, 10, 30),
                0,
            );
            open_talk(stage, |_| true);
            // Carry on: ask about their life.
            stage.hold(
                1,
                crate::input::PlayerInput {
                    menu_down: true,
                    ..crate::input::PlayerInput::default()
                },
            );
            stage.hold(
                1,
                crate::input::PlayerInput {
                    menu_down: true,
                    ..crate::input::PlayerInput::default()
                },
            );
            stage.hold(
                1,
                crate::input::PlayerInput {
                    confirm: true,
                    ..crate::input::PlayerInput::default()
                },
            );
            stage.settle(4);
        },
    },
    Scene {
        name: "people_trade",
        summary: "A person's trade list: their one-of-a-kind offer, prices by standing, rows that need a better friendship",
        setup: |stage| {
            street_at(
                stage,
                |found| found.culture == Culture::Mesa && found.setting == VillageSetting::Surface,
                DayTime::at(1, 12, 30),
                0,
            );
            // Someone with a one-of-a-kind offer that wants a friendship first.
            open_talk(stage, |character| {
                offers::offers_of(character)
                    .iter()
                    .any(|offer| offer.needs != fatherload_simulation::people::Standing::Stranger)
                    && offers::offers_of(character).iter().any(|offer| offer.once)
            });
            stage.seat(0).modal = Some(ModalScreen::PersonTrade);
            stage.seat(0).selected_menu_item = 0;
            stage.settle(3);
        },
    },
    Scene {
        name: "people_gift",
        summary: "Choosing a gift from the cargo bay for a villager",
        setup: |stage| {
            street_at(
                stage,
                |found| {
                    found.culture == Culture::Riverside && found.setting == VillageSetting::Surface
                },
                DayTime::at(1, 10, 30),
                0,
            );
            open_talk(stage, |_| true);
            stage.seat(0).modal = Some(ModalScreen::PersonGift);
            stage.seat(0).selected_menu_item = 0;
            stage.settle(3);
        },
    },
    Scene {
        name: "people_scrapper_street",
        summary: "Scrapper camp folk at work: patched cloth, goggles and rust tones",
        setup: |stage| {
            street_at(
                stage,
                |found| {
                    found.culture == Culture::Scrapper && found.setting == VillageSetting::Surface
                },
                DayTime::at(1, 15, 0),
                -6,
            );
        },
    },
    Scene {
        name: "people_mesa_street",
        summary: "Mesa townsfolk after work: ponchos with bright bands, wide hats",
        setup: |stage| {
            street_at(
                stage,
                |found| found.culture == Culture::Mesa && found.setting == VillageSetting::Surface,
                DayTime::at(1, 18, 0),
                4,
            );
        },
    },
    Scene {
        name: "people_highland_morning",
        summary: "Highland villagers up early by their doors: tartan, wool scarves",
        setup: |stage| {
            street_at(
                stage,
                |found| {
                    found.culture == Culture::Highland && found.setting == VillageSetting::Surface
                },
                DayTime::at(1, 6, 50),
                0,
            );
        },
    },
    Scene {
        name: "people_deepfolk_hall",
        summary: "An underground settlement's Deepfolk in their lamplit hall: hooded, pale from lamplight",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let found =
                    find_village(world, |found| found.setting == VillageSetting::Underground);
                set_time(world, DayTime::at(1, 12, 30));
                let column = i32::midpoint(found.plaza.0, found.plaza.1);
                park(world, seats[0].player, column_x(column), found.ground_row);
                let area = found.area();
                reveal(
                    world,
                    seats[0].player,
                    area.min.x - 4..area.max.x + 4,
                    area.min.y - 2..area.max.y + 2,
                );
            });
            stage.settle(150);
            stage.seat(0).village.banner = None;
            stage.settle(1);
        },
    },
    Scene {
        name: "people_town_folk",
        summary: "Town folk walking the town's street between the service buildings",
        setup: |stage| {
            stage.play(1, |world, seats| {
                set_time(world, DayTime::at(1, 12, 40));
                let ground = world.terrain().surface_row(110);
                park(world, seats[0].player, column_x(112), ground);
            });
            stage.settle(150);
        },
    },
];
