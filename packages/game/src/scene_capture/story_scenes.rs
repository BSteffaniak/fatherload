//! Screenshot scenes for story arcs (WP-STORY): a quest giver pitching a story on the street,
//! the journal with stories under way and told, and a story's climax moment.

use std::collections::BTreeSet;

use fatherload_simulation::{
    PlayerId, WorldState,
    entity::{EntityId, EntityKind, EntityState},
    people::{
        ArcId, ArcRecord, ArcRun, Culture, DayTime, SeedSource, SlotBinding, StorySeed, WorldFact,
        story::{
            Slot,
            run::{BindContext, bind_arc},
        },
    },
    poi::PoiJournal,
    terrain::TilePosition,
    village::{Village, VillageSetting},
    worldgen::FeatureKind,
};

use super::{
    Scene, SceneStage,
    people_scenes::{column_x, find_village, set_time, street_at},
    reveal,
};
use crate::{
    input::PlayerInput,
    shell::{JournalRow, ModalScreen, StoryMomentKind, journal_rows},
};

/// The NPC nearest seat 0's rig who has a story for its player, and the rig moved beside them.
fn approach_storyteller(stage: &mut SceneStage<'_>) -> Option<EntityId> {
    let world = stage.world();
    let player = world.player_ids().next()?;
    let rig_x = world.player(player)?.x;
    let mut found: Vec<(EntityId, f32)> = world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter_map(|entity| match entity.state {
            EntityState::Npc(npc) if npc.inside.is_none() => {
                Some((entity.id, npc.x_at(world.simulation_tick(), 0.0)))
            }
            _ => None,
        })
        .filter(|(id, _)| {
            matches!(
                world.arc_talk(player, *id),
                fatherload_simulation::world::ArcTalk::HasWork(_)
            )
        })
        .collect();
    #[allow(clippy::cast_possible_truncation, reason = "pixel distances")]
    found.sort_by_key(|(_, x)| (x - rig_x).abs() as i64);
    let (npc, x) = *found.first()?;
    stage.play_on(|world| {
        if let Some(rig) = world.player_mut(player) {
            rig.x = x;
        }
    });
    Some(npc)
}

fn press(stage: &mut SceneStage<'_>, input: PlayerInput) {
    stage.hold(1, input);
}

/// Binds `arc` as told by the `index`th resident of `found` for `player`, as play would.
fn bind_story(
    world: &WorldState,
    found: &Village,
    resident: usize,
    arc: ArcId,
    player: PlayerId,
) -> Option<ArcRun> {
    let template = arc.template()?;
    let giver = found.population.characters.get(resident)?;
    let settlement = fatherload_simulation::people::routine::Settlement::of_village(found);
    let facts = fatherload_simulation::village::village_facts(world.world_settings(), found);
    let bindings = template
        .slots
        .iter()
        .map(|slot| match slot {
            Slot::Absent(_) => SlotBinding::Absent {
                name: giver.absent.first().map_or_else(
                    || "Tobin".to_owned(),
                    |person| person.name.short().to_owned(),
                ),
                relation: giver
                    .absent
                    .first()
                    .map_or_else(|| "brother".to_owned(), |person| person.relation.clone()),
            },
            Slot::Neighbour(_) => SlotBinding::Neighbour {
                id: None,
                name: found
                    .population
                    .characters
                    .get(resident + 1)
                    .map_or_else(|| "Marla".to_owned(), |other| other.name.short().to_owned()),
                relation: "neighbour".to_owned(),
            },
            Slot::Place { kind, .. } => SlotBinding::Place(
                facts
                    .iter()
                    .find(|fact| fact.kind == *kind)
                    .copied()
                    .unwrap_or(WorldFact {
                        kind: *kind,
                        band: 2,
                        row: 60,
                        feature: None,
                    }),
            ),
        })
        .collect();
    let seed = StorySeed {
        arc,
        giver: giver.id,
        source: SeedSource::Role(giver.role),
        bindings,
        score: 0,
    };
    let journal = PoiJournal::default();
    let taken = BTreeSet::new();
    let ctx = BindContext {
        terrain: world.terrain(),
        origin: TilePosition {
            x: settlement.center_column(),
            y: settlement.ground_row - 1,
        },
        home: found.id,
        home_name: &found.name,
        journal: &journal,
        taken: &taken,
        owner: player,
        player_name: "Rook",
        tick: world.simulation_tick().get(),
        day: 3,
        hull_tenths: 1_000,
    };
    bind_arc(&seed, giver, &ctx)
}

fn frontier_village(found: &Village) -> bool {
    found.culture == Culture::Frontier && found.setting == VillageSetting::Surface
}

pub const STORY_SCENES: &[Scene] = &[
    Scene {
        name: "story_quest_giver",
        summary: "A villager pitching a story on the street: the pitch in their own voice, naming real places, and the choice to take it on",
        setup: |stage| {
            street_at(stage, frontier_village, DayTime::at(1, 10, 30), 0);
            let _ = approach_storyteller(stage);
            stage.settle(2);
            press(
                stage,
                PlayerInput {
                    interact: true,
                    ..PlayerInput::default()
                },
            );
            stage.settle(4);
            // The first row asks whether they need a hand.
            stage.seat(0).selected_menu_item = 0;
            press(
                stage,
                PlayerInput {
                    confirm: true,
                    ..PlayerInput::default()
                },
            );
            stage.settle(6);
        },
    },
    Scene {
        name: "story_journal",
        summary: "The journal: stories under way with where they stand and what to do next, stories already told, giving one up",
        setup: |stage| {
            street_at(stage, frontier_village, DayTime::at(2, 16, 0), 0);
            stage.play_on(|world| {
                let player = world.player_ids().next().expect("player");
                let found = find_village(world, frontier_village);
                let mut runs = Vec::new();
                for (resident, arc, stage_at) in [(0, 1, 1_u8), (1, 201, 0), (2, 601, 0)] {
                    if let Some(mut run) = bind_story(world, &found, resident, ArcId(arc), player) {
                        run.quest.stage = stage_at;
                        for progress in run.quest.objectives.iter_mut().take(usize::from(stage_at))
                        {
                            progress.done = true;
                        }
                        runs.push(run);
                    }
                }
                let told: Vec<ArcRecord> = [(3, 104, 0_u8), (4, 101, 1)]
                    .into_iter()
                    .filter_map(|(resident, arc, outcome)| {
                        bind_story(world, &found, resident, ArcId(arc), player).map(|run| {
                            ArcRecord {
                                cast: run.cast,
                                outcome,
                                day: 2,
                                abandoned: false,
                            }
                        })
                    })
                    .collect();
                if let Some(rig) = world.player_mut(player) {
                    rig.quests.arcs.active = runs;
                    for record in told {
                        rig.quests.arcs.record(record);
                    }
                }
            });
            stage.settle(2);
            let seat = stage.seat(0);
            seat.modal = Some(ModalScreen::Journal);
            seat.selected_menu_item = 0;
            seat.journal.focus = 0;
            stage.settle(3);
        },
    },
    Scene {
        name: "story_journal_told",
        summary: "The journal on a story already told: its ending and epilogue",
        setup: |stage| {
            street_at(stage, frontier_village, DayTime::at(2, 16, 0), 0);
            stage.play_on(|world| {
                let player = world.player_ids().next().expect("player");
                let found = find_village(world, frontier_village);
                if let Some(run) = bind_story(world, &found, 1, ArcId(5), player)
                    && let Some(rig) = world.player_mut(player)
                {
                    rig.quests.arcs.record(ArcRecord {
                        cast: run.cast,
                        outcome: 0,
                        day: 4,
                        abandoned: false,
                    });
                }
            });
            stage.settle(2);
            let rows = journal_rows(
                stage
                    .world()
                    .arc_log(stage.world().player_ids().next().expect("player")),
            );
            let told = rows
                .iter()
                .position(|row| matches!(row, JournalRow::Told(_)))
                .unwrap_or(0);
            let seat = stage.seat(0);
            seat.modal = Some(ModalScreen::Journal);
            seat.selected_menu_item = told;
            seat.journal.focus = told;
            stage.settle(3);
        },
    },
    Scene {
        name: "story_climax",
        summary: "A story's climax: watching the Moon all night from a sky island, the moment card telling what happened",
        setup: |stage| {
            let mut climax = None;
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let found = find_village(world, frontier_village);
                set_time(world, DayTime::at(2, 23, 40));
                let origin = TilePosition {
                    x: found.center_column(),
                    y: found.ground_row - 1,
                };
                let island =
                    world
                        .terrain()
                        .nearest_feature(origin, FeatureKind::SkyIsland, 3_000, |_| true);
                let (column, row) = island.map_or_else(
                    || (found.center_column(), found.ground_row - 1),
                    |island| {
                        let top = fatherload_simulation::worldgen::sky::land(
                            world.world_settings(),
                            island.id,
                        )
                        .and_then(|land| land.surface_row(island.center.x))
                        .unwrap_or(island.center.y);
                        (island.center.x, top)
                    },
                );
                if let Some(rig) = world.player_mut(player) {
                    rig.x = column_x(column);
                    #[allow(clippy::cast_precision_loss, reason = "tile rows")]
                    {
                        rig.y = (row as f32 - 0.5) * fatherload_simulation::terrain::TILE_SIZE;
                    }
                    rig.velocity_x = 0.0;
                    rig.velocity_y = 0.0;
                }
                reveal(world, player, column - 40..column + 40, row - 20..row + 16);
                world.stream_terrain_near_players();
                if let Some(mut run) = bind_story(world, &found, 0, ArcId(407), player) {
                    run.quest.stage = 1;
                    run.quest.ready = true;
                    for progress in &mut run.quest.objectives {
                        progress.done = true;
                    }
                    climax = Some((
                        run.template()
                            .map_or(String::new(), |template| template.title.to_owned()),
                        run.journal_text(),
                    ));
                    if let Some(rig) = world.player_mut(player) {
                        rig.quests.arcs.active = vec![run];
                    }
                }
            });
            stage.settle(20);
            if let Some((title, text)) = climax {
                stage
                    .game
                    .shell
                    .story_moment(0, StoryMomentKind::Climax, title, text);
            }
            stage.settle(40);
        },
    },
];
