//! The deep settlements' own stories (Wave 10 story) played through the tick on a Deep Descent
//! world: each settlement's residents offer its arcs (and no one else's), and a bot takes each
//! on, plays its beats at the real places (the Fathers' sites, the seams of the settlement's
//! own stratum, its people as contacts), turns it in and is paid.

use crate::{
    GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs, TickOutput,
    WorldState,
    enemy::EnemyDifficulty,
    entity::{EntityId, EntityKind, EntityState},
    expedition::memories::Settlement as Remembering,
    people::{
        ArcId, ArcKind, ArcRun, PeopleCommand, TalkTopic,
        deep_folk::{AMBER_COURT, ARCHIVE_POST, CATHEDRAL_STATION, HEARTS_EDGE},
        routine::{self, Settlement},
        story::{ARC_TEMPLATES, SeedSource, StorySeed, Trigger},
    },
    position::WorldPos,
    quest::{Objective, QuestTarget},
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    village::{self, VillageId, placement::underground_id},
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::ArcTalk;

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x5EED_1400;

fn world() -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(SEED)
    };
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.radiator_level = 15;
    rig.hull_level = 15;
    rig.hull = rig.max_hull();
    rig.fuel_tank_level = 15;
    rig.engine_level = 15;
    rig.drill_strength = 15;
    rig.quests.deepest_row = 100_000;
    world
}

/// The deep settlements with people in a V11 world, and what each remembers.
fn deep_settlements() -> [(VillageId, Remembering); 8] {
    [
        (underground_id(0), Remembering::GravelHollow),
        (CATHEDRAL_STATION, Remembering::OldDeepCamp),
        (AMBER_COURT, Remembering::TheHive),
        (
            underground_id(crate::drowned::MERROW_SITE),
            Remembering::MerrowCity,
        ),
        (
            underground_id(crate::worldgen::streamed::villages::SUNFOLK_SITE),
            Remembering::JungleTribes,
        ),
        (
            underground_id(crate::distortion::PARALLAX_SITE),
            Remembering::PrismRelay,
        ),
        // Descent VII's outpost (Wave 10 ancients-a), its stories the story track's.
        (ARCHIVE_POST, Remembering::ArchivePost),
        // Descent VIII's outpost (Wave 11 carryover: Heart's Edge has residents now).
        (HEARTS_EDGE, Remembering::HeartCamp),
    ]
}

/// The arcs `place`'s people tell.
fn arcs_of(place: Remembering) -> Vec<ArcId> {
    ARC_TEMPLATES
        .iter()
        .filter(|arc| arc.triggers.contains(&Trigger::Settlement(place)))
        .map(|arc| arc.id)
        .collect()
}

fn run(world: &mut WorldState, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        events.extend(
            world
                .simulate_tick(tick, &TickInputs::default())
                .events
                .into_iter()
                .map(|event| event.kind),
        );
    }
    events
}

fn people(world: &mut WorldState, command: PeopleCommand) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(
        tick,
        &TickInputs::default().act(PLAYER, PlayerCommand::People(command)),
    )
}

fn put(world: &mut WorldState, tile: TilePosition) {
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(tile.x), i64::from(tile.y)),
    );
}

/// Parks the rig on `settlement`'s floor and lets its people come out.
fn park(world: &mut WorldState, settlement: &Settlement) {
    put(
        world,
        TilePosition {
            x: settlement.center_column(),
            y: settlement.ground_row - 1,
        },
    );
    let _ = run(world, 40);
}

fn folk_of(world: &WorldState, id: VillageId) -> Vec<EntityId> {
    world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter_map(|entity| match entity.state {
            EntityState::Npc(state) if state.character.village == id => Some(entity.id),
            _ => None,
        })
        .collect()
}

/// Moves the rig up beside `npc`.
fn go_to(world: &mut WorldState, npc: EntityId) {
    let (x, y) = world.npc_position(npc).expect("npc");
    let frame = world.frame();
    put(
        world,
        TilePosition {
            x: frame.col(x),
            y: frame.row(y - TILE_SIZE),
        },
    );
}

fn running(world: &WorldState, arc: ArcId) -> Option<ArcRun> {
    world
        .arc_log(PLAYER)
        .and_then(|log| log.running(arc))
        .cloned()
}

fn mark_done(world: &mut WorldState, arc: ArcId, index: usize) {
    if let Some(run) = world.player_mut(PLAYER).and_then(|player| {
        player
            .quests
            .arcs
            .active
            .iter_mut()
            .find(|run| run.arc() == arc)
    }) {
        run.quest.objectives[index].done = true;
    }
}

/// The bot digs its way to `target`: the place is opened up to air, then the rig parked in it.
fn dig_to(world: &mut WorldState, target: QuestTarget) {
    for y in target.bounds.min.y..=target.bounds.max.y {
        for x in target.bounds.min.x..=target.bounds.max.x {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
    }
    put(world, target.center);
}

/// Plays the current beat of `arc`: false once it is ready to turn in (or gone).
fn drive_beat(world: &mut WorldState, arc: ArcId) -> bool {
    let Some(run) = running(world, arc) else {
        return false;
    };
    if run.ready() {
        return false;
    }
    let _ = world.take_pilot_for_parking(PLAYER);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.hull = rig.max_hull();
        rig.fuel = rig.fuel_capacity;
    }
    let def = run.quest.quest.def().expect("def");
    let stage = usize::from(run.quest.stage);
    let start = def.stage_start(stage);
    let target = |index: u8| run.quest.targets[usize::from(index)];
    for (offset, objective) in def.stages[stage].iter().enumerate() {
        if run.quest.objectives[start + offset].done {
            continue;
        }
        match *objective {
            Objective::Reach { target: index } | Objective::Survive { target: index, .. } => {
                dig_to(world, target(index));
            }
            Objective::ReturnToSurface => {
                let x = target(0).center.x;
                let surface = crate::quest::surface_row_at(world.terrain(), x);
                put(world, TilePosition { x, y: surface - 2 });
            }
            Objective::OnFoot { target: index } | Objective::HandDeliver { target: index, .. } => {
                let goal = target(index);
                // A door inside the place (a village's), else the place's own middle.
                let door =
                    village::village_at(world.world_settings(), goal.center).and_then(|found| {
                        found
                            .buildings
                            .iter()
                            .map(|lot| TilePosition {
                                x: lot.first_column + lot.width / 2,
                                y: found.ground_row - 1,
                            })
                            .find(|door| goal.bounds.contains(*door))
                    });
                let tile = door.unwrap_or(goal.center);
                put(
                    world,
                    TilePosition {
                        x: tile.x + 2,
                        ..tile
                    },
                );
                let (x, y) = world.frame().tile_center(tile);
                let _ = world.place_pilot(PLAYER, x, TILE_SIZE.mul_add(0.5, y));
            }
            Objective::Speak { .. } => {
                let contact = run.cast.contact.as_ref().expect("a contact");
                let home = routine::settlement(world.world_settings(), contact.id.village)
                    .expect("the contact's settlement");
                park(world, &home);
                let npc = world.npc_of(contact.id).expect("the contact is about");
                go_to(world, npc);
                let _ = people(
                    world,
                    PeopleCommand::Talk {
                        npc,
                        topic: TalkTopic::Chat,
                    },
                );
            }
            Objective::Deliver { item, count } => {
                world
                    .player_mut(PLAYER)
                    .expect("rig")
                    .cargo_bay
                    .add(item.item(), count);
            }
            // Scanning, digging and pick-ups are the quest engine's own tests' business.
            _ => mark_done(world, arc, start + offset),
        }
    }
    true
}

/// Plays `arc` to its climax; returns whether it got there.
fn play_to_climax(world: &mut WorldState, arc: ArcId) -> bool {
    let mut climax = false;
    for _ in 0..30 {
        if !drive_beat(world, arc) {
            break;
        }
        for _ in 0..80 {
            let seen = run(world, 10);
            climax |= seen.iter().any(
                |event| matches!(event, GameplayEventKind::ArcClimax { arc: a, .. } if *a == arc),
            );
            if seen.iter().any(|event| {
                matches!(
                    event,
                    GameplayEventKind::ArcStepDone { .. } | GameplayEventKind::ArcClimax { .. }
                )
            }) {
                break;
            }
            let _ = drive_beat(world, arc);
        }
    }
    // A Speak beat's climax lands on the talk's own tick.
    climax || running(world, arc).is_some_and(|run| run.ready())
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one pass over every settlement and its stories"
)]
fn every_deep_settlement_tells_its_own_stories_and_a_bot_sees_them_through() {
    let mut world = world();
    let settings = *world.world_settings();
    for (id, place) in deep_settlements() {
        let settlement = routine::settlement(&settings, id)
            .unwrap_or_else(|| panic!("{} has people", place.name()));
        assert_eq!(
            Remembering::of_place_name(&settlement.population.village_name),
            Some(place)
        );
        let wanted = arcs_of(place);
        assert!(
            (2..=3).contains(&wanted.len()),
            "{}: {} arcs",
            place.name(),
            wanted.len()
        );
        let mut finished: Vec<ArcId> = Vec::new();
        for _ in 0..8 {
            park(&mut world, &settlement);
            let offers: Vec<(EntityId, ArcId)> = folk_of(&world, id)
                .into_iter()
                .filter_map(|npc| match world.arc_talk(PLAYER, npc) {
                    ArcTalk::HasWork(arc) => Some((npc, arc)),
                    _ => None,
                })
                .collect();
            // Here the settlement's own stories come first (while any is left untold, every
            // resident with work offers one of them), never another settlement's.
            for (_, arc) in &offers {
                let template = arc.template().expect("template");
                if template.kind == ArcKind::Deep {
                    assert!(wanted.contains(arc), "{}: {}", place.name(), template.title);
                } else {
                    assert!(
                        wanted.iter().all(|own| finished.contains(own)),
                        "{}: {} offered before its own stories were told",
                        place.name(),
                        template.title
                    );
                }
            }
            let Some((npc, arc)) = offers
                .into_iter()
                .find(|(_, arc)| arc.template().is_some_and(|t| t.kind == ArcKind::Deep))
            else {
                break;
            };
            let template = arc.template().expect("template");
            let giver = world.npc(npc).expect("npc").1.character;
            go_to(&mut world, npc);
            let _ = people(&mut world, PeopleCommand::AskWork { npc });
            let _ = people(&mut world, PeopleCommand::AcceptArc { npc });
            assert!(
                running(&world, arc).is_some(),
                "{} taken on",
                template.title
            );
            assert!(
                play_to_climax(&mut world, arc),
                "{} never reached its climax",
                template.title
            );
            // Back in the rig and back to the giver to tell it.
            let _ = world.take_pilot_for_parking(PLAYER);
            park(&mut world, &settlement);
            let npc = world.npc_of(giver).expect("the giver is about");
            go_to(&mut world, npc);
            let ArcTalk::Ready { endings, .. } = world.arc_talk(PLAYER, npc) else {
                panic!("{}: not ready to tell", template.title);
            };
            for objective in template.stages.last().copied().unwrap_or(&[]) {
                if let Objective::Deliver { item, count } = objective {
                    world
                        .player_mut(PLAYER)
                        .expect("rig")
                        .cargo_bay
                        .add(item.item(), *count);
                }
            }
            let credits = world.player(PLAYER).expect("rig").credits;
            let told = people(
                &mut world,
                PeopleCommand::TurnInArc {
                    npc,
                    outcome: endings[0].0,
                },
            );
            assert_eq!(
                told.action_outcomes.first().map(|outcome| outcome.outcome),
                Some(PlayerScopedCommandOutcome::Applied),
                "{}",
                template.title
            );
            assert!(told.events.iter().any(|event| matches!(
                event.kind,
                GameplayEventKind::ArcCompleted { arc: a, .. } if a == arc
            )));
            assert!(
                world.player(PLAYER).expect("rig").credits >= credits + template.reward.credits,
                "{}: paid",
                template.title
            );
            finished.push(arc);
        }
        finished.sort_unstable();
        let mut wanted = wanted;
        wanted.sort_unstable();
        assert_eq!(finished, wanted, "{}: every story told", place.name());
    }
}

#[test]
fn deep_places_bind_deterministically_inside_the_settlements_descent() {
    let settings = WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(SEED)
    };
    let world = world();
    for (id, _) in deep_settlements() {
        let settlement = routine::settlement(&settings, id).expect("people");
        let origin = TilePosition {
            x: settlement.center_column(),
            y: settlement.ground_row - 1,
        };
        let here = crate::depth_progression::descent_index_at_row(&settings, origin.y);
        let giver = settlement.population.characters[0].clone();
        for template in ARC_TEMPLATES.iter().filter(|arc| arc.kind == ArcKind::Deep) {
            let seed = StorySeed {
                arc: template.id,
                giver: giver.id,
                source: SeedSource::Role(giver.role),
                bindings: Vec::new(),
                score: 0,
            };
            let run = world
                .bind_offer(PLAYER, &settlement, &giver, &seed)
                .expect("binds");
            assert_eq!(
                Some(&run),
                world
                    .bind_offer(PLAYER, &settlement, &giver, &seed)
                    .as_ref(),
                "deterministic"
            );
            for (spec, target) in template.targets.iter().zip(&run.quest.targets) {
                if *spec == crate::quest::TargetSpec::Story(crate::quest::StoryPlace::DeepSeam) {
                    assert_eq!(
                        crate::depth_progression::descent_index_at_row(&settings, target.center.y),
                        here,
                        "{}: the seam lies in the settlement's own Descent",
                        template.title
                    );
                }
            }
        }
    }
}
