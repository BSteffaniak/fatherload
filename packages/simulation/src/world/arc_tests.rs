//! Story arcs in the world (WP-STORY): asking, taking on, running beats through the tick
//! (bot-driven where the beat is about going somewhere, talking, watching or walking), contacts,
//! climaxes, endings and their effects, giving up, saves, replication and determinism.

use std::sync::Arc;

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, PlayerScopedCommandOutcome,
    SectionKey, SimulationParams, SimulationTick, TickInputs, TickOutput, WorldState,
    entity::{EntityId, EntityKind, EntityState},
    people::{
        ArcId, ArcRun, DayTime, MemoryFact, PeopleCommand, TalkTopic, TickClock,
        clock::DEFAULT_DAY_LENGTH_MINUTES, story::OutcomeWhen,
    },
    quest::{Objective, QuestTarget},
    terrain::{TILE_SIZE, TilePosition},
    village::{self, Village, placement},
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::ArcTalk;

const PLAYER: PlayerId = PlayerId::new(1);
const SEED: u64 = 0x5EED_0026;

fn world_at(hz: u32) -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V7,
        ..WorldSettings::new(SEED)
    };
    WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(hz))
}

fn first_village(world: &WorldState) -> Arc<Village> {
    village::village(world.world_settings(), placement::surface_id(1)).expect("first village")
}

#[allow(clippy::cast_precision_loss, reason = "small columns and rows")]
fn tile_px(tile: TilePosition) -> (f32, f32) {
    (
        (tile.x as f32 + 0.5) * TILE_SIZE,
        (tile.y as f32 + 0.5) * TILE_SIZE,
    )
}

/// Puts `player`'s rig at a tile, at rest.
fn put_rig(world: &mut WorldState, player: PlayerId, tile: TilePosition) {
    let (x, y) = tile_px(tile);
    let rig = world.player_mut(player).expect("rig");
    rig.x = x;
    rig.y = y;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.x_remainder = 0.0;
}

fn step(world: &mut WorldState) -> TickOutput {
    let tick = world.simulation_tick();
    let inputs = TickInputs::held(PLAYER, ControlState::new(0.0, false, false));
    world.simulate_tick(tick, &inputs)
}

fn run(world: &mut WorldState, ticks: u64) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(step(world).events.into_iter().map(|event| event.kind));
    }
    events
}

fn act(world: &mut WorldState, command: PlayerCommand) -> TickOutput {
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(PLAYER, command);
    world.simulate_tick(tick, &inputs)
}

fn people(world: &mut WorldState, command: PeopleCommand) -> TickOutput {
    act(world, PlayerCommand::People(command))
}

fn outcome(output: &TickOutput) -> PlayerScopedCommandOutcome {
    output.action_outcomes.first().expect("one action").outcome
}

fn set_time(world: &mut WorldState, time: DayTime) {
    let tick = TickClock::tick_at(time, world.tick_rate_hz(), DEFAULT_DAY_LENGTH_MINUTES);
    world.set_simulation_tick(SimulationTick::new(tick));
}

/// The rig on the first village's plaza at 10:00 on day 1, residents out and about.
fn village_world() -> (WorldState, Arc<Village>) {
    let mut world = world_at(30);
    let found = first_village(&world);
    set_time(&mut world, DayTime::at(1, 10, 0));
    let plaza = i32::midpoint(found.plaza.0, found.plaza.1);
    put_rig(
        &mut world,
        PLAYER,
        TilePosition {
            x: plaza,
            y: found.ground_row - 1,
        },
    );
    let _ = run(&mut world, 20);
    (world, found)
}

fn npcs(world: &WorldState) -> Vec<(EntityId, crate::entity::NpcState)> {
    world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter_map(|entity| match entity.state {
            EntityState::Npc(state) => Some((entity.id, state)),
            _ => None,
        })
        .collect()
}

/// Moves the rig next to `npc` (outdoors) or to the door of the building they are in.
fn go_to(world: &mut WorldState, npc: EntityId) {
    let (x, y) = world.npc_position(npc).expect("npc");
    #[allow(clippy::cast_possible_truncation, reason = "world pixels fit tiles")]
    let tile = TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32 - 1,
    };
    put_rig(world, PLAYER, tile);
}

/// The first outdoor person in `found` with an arc for the player, matching `want`.
fn giver_with(
    world: &WorldState,
    found: &Village,
    want: impl Fn(ArcId) -> bool,
) -> Option<(EntityId, ArcId)> {
    npcs(world)
        .into_iter()
        .filter(|(_, state)| state.character.village == found.id && state.inside.is_none())
        .find_map(|(id, _)| match world.arc_talk(PLAYER, id) {
            ArcTalk::HasWork(arc) if want(arc) => Some((id, arc)),
            _ => None,
        })
}

/// A player who could take on anything (every prerequisite met).
fn make_veteran(world: &mut WorldState) {
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.fuel_tank_level = 15;
    rig.engine_level = 15;
    rig.drill_strength = 15;
    rig.hazard_gear.altitude_intake = crate::climb::INTAKE_TIERS[3];
    rig.quests.deepest_row = 10_000;
}

fn running(world: &WorldState) -> Vec<ArcRun> {
    world.arc_log(PLAYER).expect("log").active.clone()
}

#[test]
fn a_villager_pitches_an_arc_and_the_player_takes_it_on() {
    let (mut world, found) = village_world();
    let (npc, arc) = giver_with(&world, &found, |_| true).expect("someone has a story");
    go_to(&mut world, npc);
    // Asking: the pitch, bound to the world, kept as the open offer.
    let asked = people(&mut world, PeopleCommand::AskWork { npc });
    assert_eq!(outcome(&asked), PlayerScopedCommandOutcome::Applied);
    assert!(asked.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::ArcOffered { arc: offered, .. } if offered == arc
    )));
    let offer = world
        .arc_log(PLAYER)
        .and_then(|log| log.offer.clone())
        .expect("offer");
    assert_eq!(offer.arc(), arc);
    assert_eq!(world.arc_talk(PLAYER, npc), ArcTalk::Pitched(arc));
    let conversation = world.people().conversation(PLAYER).expect("talking");
    assert!(conversation.lines.len() >= 3, "{:#?}", conversation.lines);
    assert!(
        conversation
            .lines
            .iter()
            .all(|line| !line.text.contains(['{', '}', '[', ']'])),
        "{:#?}",
        conversation.lines
    );
    // Taking it on.
    let accepted = people(&mut world, PeopleCommand::AcceptArc { npc });
    assert_eq!(outcome(&accepted), PlayerScopedCommandOutcome::Applied);
    assert!(accepted.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::ArcAccepted { arc: taken, .. } if taken == arc
    )));
    let log = world.arc_log(PLAYER).expect("log");
    assert!(log.offer.is_none());
    assert_eq!(log.active.len(), 1);
    assert_eq!(world.arc_talk(PLAYER, npc), ArcTalk::Running(arc));
    assert!(!world.arc_markers(PLAYER).is_empty() || log.active[0].quest.targets.is_empty());
    // Asking again: how it's going.
    let nudged = people(&mut world, PeopleCommand::AskWork { npc });
    assert_eq!(outcome(&nudged), PlayerScopedCommandOutcome::Applied);
    // Accepting again does nothing (no offer on the table).
    let again = people(&mut world, PeopleCommand::AcceptArc { npc });
    assert_eq!(
        outcome(&again),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
}

#[test]
fn asking_needs_reach() {
    let (mut world, found) = village_world();
    let (npc, _) = giver_with(&world, &found, |_| true).expect("someone has a story");
    put_rig(
        &mut world,
        PLAYER,
        TilePosition {
            x: found.last_column + 60,
            y: found.ground_row - 1,
        },
    );
    let asked = people(&mut world, PeopleCommand::AskWork { npc });
    assert_eq!(
        outcome(&asked),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
}

/// Drives the current beat of the running arc `arc` where a bot can: going somewhere (rig or
/// pilot), talking to the contact, keeping watch at night. Other primitives (digging, bombing,
/// scanning, building) are the quest engine's own, tested in `quest_tests`; here their
/// progress is marked done. Returns false once the arc is ready.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per kind of beat the bot can drive"
)]
fn drive_beat(world: &mut WorldState, arc: ArcId, events: &mut Vec<GameplayEventKind>) -> bool {
    let Some(run) = running(world).into_iter().find(|run| run.arc() == arc) else {
        return false;
    };
    if run.ready() {
        return false;
    }
    // Back in the rig between beats (a pilot left on foot would keep people away from it).
    let _ = world.take_pilot_for_parking(PLAYER);
    // The bot never runs dry or breaks down (hazards are other tests' business).
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.hull = rig.max_hull();
        rig.fuel = rig.fuel_capacity;
    }
    let def = run.quest.quest.def().expect("def");
    let stage = usize::from(run.quest.stage);
    let start = def.stage_start(stage);
    let target = |index: u8| -> QuestTarget { run.quest.targets[usize::from(index)] };
    for (offset, objective) in def.stages[stage].iter().enumerate() {
        if run.quest.objectives[start + offset].done {
            continue;
        }
        match *objective {
            Objective::Reach { target: index } | Objective::Survive { target: index, .. } => {
                put_rig(world, PLAYER, target(index).center);
            }
            Objective::ReturnToSurface => {
                let x = target(0).center.x;
                let surface = crate::quest::surface_row_at(world.terrain(), x);
                put_rig(world, PLAYER, TilePosition { x, y: surface - 2 });
            }
            Objective::Vigil { target: index, .. } => {
                if !world.is_night() {
                    set_time(world, DayTime::at(world.people_clock().day, 22, 0));
                }
                put_rig(world, PLAYER, target(index).center);
            }
            Objective::Dawn { target: index } => {
                // Late in the night, at the place, until the sky greys.
                if run.quest.objectives[start + offset].timer == 0 {
                    set_time(world, DayTime::at(world.people_clock().day + 1, 4, 24));
                }
                put_rig(world, PLAYER, target(index).center);
            }
            Objective::OnFoot { target: index } | Objective::HandDeliver { target: index, .. } => {
                let center = target(index).center;
                let bounds = target(index).bounds;
                // A door inside the target (a village's market, say), else the first one.
                let door = village::village_at(world.world_settings(), center).and_then(|found| {
                    let doors: Vec<TilePosition> = found
                        .buildings
                        .iter()
                        .map(|lot| TilePosition {
                            x: lot.first_column + lot.width / 2,
                            y: found.ground_row - 1,
                        })
                        .collect();
                    doors
                        .iter()
                        .find(|door| bounds.contains(**door))
                        .or_else(|| doors.first())
                        .copied()
                });
                let tile = door.unwrap_or(center);
                put_rig(
                    world,
                    PLAYER,
                    TilePosition {
                        x: tile.x + 2,
                        ..tile
                    },
                );
                let (x, y) = tile_px(tile);
                let _ = world.place_pilot(PLAYER, x, TILE_SIZE.mul_add(0.5, y));
            }
            Objective::Speak { .. } => {
                if let Some(contact) = &run.cast.contact {
                    // Walk over to them (bringing their settlement out) and say hello.
                    if let Some(settlement) = crate::people::routine::settlement(
                        world.world_settings(),
                        contact.id.village,
                    ) {
                        put_rig(
                            world,
                            PLAYER,
                            TilePosition {
                                x: settlement.center_column(),
                                y: settlement.ground_row - 1,
                            },
                        );
                        events.extend(run_ticks(world, 30));
                    }
                    if let Some(npc) = world.npc_of(contact.id) {
                        go_to(world, npc);
                        let out = people(
                            world,
                            PeopleCommand::Talk {
                                npc,
                                topic: TalkTopic::Chat,
                            },
                        );
                        events.extend(out.events.into_iter().map(|event| event.kind));
                    }
                } else {
                    put_rig(
                        world,
                        PLAYER,
                        target(def.stages[stage][offset].targets()[0]).center,
                    );
                }
            }
            Objective::Deliver { item, count } => {
                world
                    .player_mut(PLAYER)
                    .expect("rig")
                    .cargo_bay
                    .add(item.item(), count);
            }
            _ => {
                if let Some(player) = world.player_mut(PLAYER)
                    && let Some(run) = player
                        .quests
                        .arcs
                        .active
                        .iter_mut()
                        .find(|run| run.arc() == arc)
                {
                    run.quest.objectives[start + offset].done = true;
                }
            }
        }
    }
    true
}

fn run_ticks(world: &mut WorldState, ticks: u64) -> Vec<GameplayEventKind> {
    run(world, ticks)
}

/// Plays `arc` (already running) to its climax, beat by beat; returns the events seen.
fn play_to_climax(world: &mut WorldState, arc: ArcId) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..40 {
        if !drive_beat(world, arc, &mut events) {
            break;
        }
        // Long beats (a vigil, holding out) take their time.
        for _ in 0..80 {
            let seen = run(world, 10);
            let moved = seen.iter().any(|event| {
                matches!(
                    event,
                    GameplayEventKind::ArcStepDone { .. } | GameplayEventKind::ArcClimax { .. }
                )
            });
            events.extend(seen);
            if moved {
                break;
            }
            let _ = drive_beat(world, arc, &mut events);
        }
    }
    events
}

#[test]
fn arcs_run_through_their_beats_to_a_climax_and_an_ending() {
    let (mut world, found) = village_world();
    make_veteran(&mut world);
    let (npc, arc) = giver_with(&world, &found, |_| true).expect("someone has a story");
    let giver = world.npc(npc).expect("npc").1.character;
    go_to(&mut world, npc);
    let _ = people(&mut world, PeopleCommand::AskWork { npc });
    let _ = people(&mut world, PeopleCommand::AcceptArc { npc });
    let template = arc.template().expect("template");
    let events = play_to_climax(&mut world, arc);
    let steps = events
        .iter()
        .filter(|event| matches!(event, GameplayEventKind::ArcStepDone { arc: a, .. } if *a == arc))
        .count();
    assert_eq!(
        steps,
        template.stages.len() - 1,
        "{}: {events:?}",
        template.title
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::ArcClimax { arc: a, .. } if *a == arc)),
        "{}: no climax",
        template.title
    );
    let ready = running(&world)
        .into_iter()
        .find(|run| run.arc() == arc)
        .expect("running");
    assert!(ready.ready());
    assert_ne!(ready.journal_text(), "");
    // Back to the giver: the endings are on offer.
    let home =
        crate::people::routine::settlement(world.world_settings(), giver.village).expect("home");
    put_rig(
        &mut world,
        PLAYER,
        TilePosition {
            x: home.center_column(),
            y: home.ground_row - 1,
        },
    );
    let _ = run(&mut world, 30);
    let npc = world.npc_of(giver).expect("giver about");
    go_to(&mut world, npc);
    let ArcTalk::Ready { endings, .. } = world.arc_talk(PLAYER, npc) else {
        panic!("{}: not ready to tell", template.title);
    };
    assert_eq!(endings.len(), template.turn_in_choices().len());
    // Deliveries are handed over at turn-in: carry them.
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
    let affinity = world.people().affinity(giver, PLAYER);
    let (index, _) = endings[0];
    let told = people(
        &mut world,
        PeopleCommand::TurnInArc {
            npc,
            outcome: index,
        },
    );
    assert_eq!(
        outcome(&told),
        PlayerScopedCommandOutcome::Applied,
        "{}",
        template.title
    );
    assert!(told.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::ArcCompleted { arc: a, .. } if a == arc
    )));
    assert!(world.player(PLAYER).expect("rig").credits > credits, "paid");
    assert!(world.people().affinity(giver, PLAYER) > affinity, "warmer");
    let log = world.arc_log(PLAYER).expect("log");
    assert_eq!(log.active, []);
    assert!(log.finished_with(giver, arc));
    let record = log.history.last().expect("history");
    assert!(!record.abandoned);
    assert_ne!(record.epilogue(), "");
    assert_eq!(
        template.outcomes[usize::from(record.outcome)].when,
        OutcomeWhen::Completed
    );
    // The giver remembers it.
    let memory = world.people().character(giver).memory(PLAYER);
    assert!(
        memory
            .events
            .iter()
            .any(|event| matches!(event.fact, MemoryFact::ArcCompleted { arc: a, .. } if a == arc))
    );
    // And doesn't offer the same story again.
    assert_ne!(world.arc_talk(PLAYER, npc), ArcTalk::HasWork(arc));
}

#[test]
fn every_arc_plays_through_on_the_tick() {
    // Each arc, bound from the first village's elder, run beat by beat to its climax.
    let (mut world, found) = village_world();
    make_veteran(&mut world);
    let elder = found.population.characters[0].clone();
    let settlement =
        crate::people::routine::settlement(world.world_settings(), found.id).expect("settlement");
    for template in crate::people::story::ARC_TEMPLATES.iter() {
        let facts = village::village_facts(world.world_settings(), &found);
        let mut rng_bindings = Vec::new();
        let ok = template.slots.iter().all(|slot| {
            let binding = match slot {
                crate::people::story::Slot::Absent(_) => crate::people::SlotBinding::Absent {
                    name: "Tobin".into(),
                    relation: "brother".into(),
                },
                crate::people::story::Slot::Neighbour(_) => crate::people::SlotBinding::Neighbour {
                    id: None,
                    name: "Marla".into(),
                    relation: "neighbour".into(),
                },
                crate::people::story::Slot::Place { kind, .. } => {
                    let fact = facts
                        .iter()
                        .find(|fact| fact.kind == *kind)
                        .copied()
                        .unwrap_or(crate::people::WorldFact {
                            kind: *kind,
                            band: 2,
                            row: 60,
                            feature: None,
                        });
                    crate::people::SlotBinding::Place(fact)
                }
            };
            rng_bindings.push(binding);
            true
        });
        assert!(ok);
        let seed = crate::people::StorySeed {
            arc: template.id,
            giver: elder.id,
            source: crate::people::SeedSource::Role(elder.role),
            bindings: rng_bindings,
            score: 0,
        };
        let run = world
            .bind_offer(PLAYER, &settlement, &elder, &seed)
            .unwrap_or_else(|| panic!("{} binds", template.title));
        world.player_mut(PLAYER).expect("rig").quests.arcs.active = vec![run];
        let events = play_to_climax(&mut world, template.id);
        assert!(
            events.iter().any(
                |event| matches!(event, GameplayEventKind::ArcClimax { arc, .. } if *arc == template.id)
            ),
            "{} never reached its climax: {events:?}",
            template.title
        );
        world
            .player_mut(PLAYER)
            .expect("rig")
            .quests
            .arcs
            .active
            .clear();
    }
}

/// The Wave 6 arcs (ids 901 and up): bound to tombs, crypts, ruins, the night market,
/// homesteads, sky towns and the Moon's colony.
fn wave6_arcs() -> impl Iterator<Item = &'static crate::people::ArcTemplate> {
    crate::people::story::ARC_TEMPLATES
        .iter()
        .filter(|template| template.id.0 >= 901)
}

/// Binds `template` from `found`'s elder with stand-in slots and makes it the player's only
/// running arc.
fn start_arc(world: &mut WorldState, found: &Village, template: &crate::people::ArcTemplate) {
    let elder = found.population.characters[0].clone();
    let settlement =
        crate::people::routine::settlement(world.world_settings(), found.id).expect("settlement");
    let bindings = template
        .slots
        .iter()
        .map(|slot| match slot {
            crate::people::story::Slot::Absent(_) => crate::people::SlotBinding::Absent {
                name: "Tobin".into(),
                relation: "brother".into(),
            },
            crate::people::story::Slot::Neighbour(_) => crate::people::SlotBinding::Neighbour {
                id: None,
                name: "Marla".into(),
                relation: "neighbour".into(),
            },
            crate::people::story::Slot::Place { kind, .. } => {
                crate::people::SlotBinding::Place(crate::people::WorldFact {
                    kind: *kind,
                    band: 2,
                    row: 60,
                    feature: None,
                })
            }
        })
        .collect();
    let seed = crate::people::StorySeed {
        arc: template.id,
        giver: elder.id,
        source: crate::people::SeedSource::Role(elder.role),
        bindings,
        score: 0,
    };
    let run = world
        .bind_offer(PLAYER, &settlement, &elder, &seed)
        .unwrap_or_else(|| panic!("{} binds", template.title));
    world.player_mut(PLAYER).expect("rig").quests.arcs.active = vec![run];
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one pass over every new arc with its places"
)]
fn every_wave6_arc_binds_to_its_real_places_and_plays_through_on_a_v10_world() {
    // A V10 world has tombs and crypts, sealed ruins, sky towns and a filled Moon with its
    // colony: every new arc binds to the real thing here (not a stand-in) and a bot plays it
    // to its climax on the tick.
    let settings = WorldSettings::new(SEED);
    assert!(settings.generator.has_tombs() && settings.generator.has_moon_content());
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(30));
    let found = first_village(&world);
    set_time(&mut world, DayTime::at(1, 10, 0));
    put_rig(
        &mut world,
        PLAYER,
        TilePosition {
            x: i32::midpoint(found.plaza.0, found.plaza.1),
            y: found.ground_row - 1,
        },
    );
    let _ = run(&mut world, 20);
    make_veteran(&mut world);
    let colony = crate::people::colony::colony_settlement_id(&settings).expect("a colony");
    let mut real = std::collections::BTreeMap::<&str, usize>::new();
    for template in wave6_arcs() {
        start_arc(&mut world, &found, template);
        let running = running(&world).remove(0);
        for (spec, target) in template.targets.iter().zip(&running.quest.targets) {
            let crate::quest::TargetSpec::Story(place) = spec else {
                continue;
            };
            let bound_real = match place {
                crate::quest::StoryPlace::Tomb | crate::quest::StoryPlace::Crypt => {
                    crate::worldgen::streamed::tomb_sites::site_with_door(&settings, target.center)
                        .is_some_and(|site| {
                            *place == crate::quest::StoryPlace::Tomb
                                || site.style == crate::tomb::SiteStyle::TopDown
                        })
                }
                crate::quest::StoryPlace::Ruin => {
                    crate::worldgen::streamed::pilot_spaces::spaces_touching(
                        &settings,
                        &target.bounds,
                    )
                    .iter()
                    .any(|space| {
                        space.kind == crate::worldgen::streamed::pilot_spaces::PilotSpaceKind::Ruin
                            && space.entrance() == target.center
                    })
                }
                crate::quest::StoryPlace::SkyTown => village::village_at(&settings, target.center)
                    .is_some_and(|town| town.setting.sky_town().is_some()),
                crate::quest::StoryPlace::Homestead => crate::home::plot::listed_sites_between(
                    &settings,
                    target.center.x - 300,
                    target.center.x + 300,
                )
                .iter()
                .any(|site| {
                    site.left == target.bounds.min.x && site.right() == target.bounds.max.x
                }),
                crate::quest::StoryPlace::NightMarket => {
                    let (x, y) = tile_px(target.center);
                    village::building_at(&settings, x, y).is_some_and(|(town, lot)| {
                        town.buildings[lot].kind == village::BuildingKind::Market
                    })
                }
                crate::quest::StoryPlace::Moon => running
                    .cast
                    .contact
                    .as_ref()
                    .is_none_or(|contact| contact.id.village == colony),
                _ => true,
            };
            assert!(
                bound_real,
                "{}: {place:?} stood in on a V10 world",
                template.title
            );
            *real.entry(place.noun()).or_default() += 1;
        }
        // Speak beats at the Moon find a colonist, in a sky town one of its folk.
        if let Some(contact) = &running.cast.contact
            && template.targets.iter().all(|spec| {
                *spec == crate::quest::TargetSpec::Story(crate::quest::StoryPlace::Moon)
            })
        {
            assert_eq!(contact.id.village, colony, "{}", template.title);
        }
        let events = play_to_climax(&mut world, template.id);
        assert!(
            events.iter().any(
                |event| matches!(event, GameplayEventKind::ArcClimax { arc, .. } if *arc == template.id)
            ),
            "{} never reached its climax: {events:?}",
            template.title
        );
        world
            .player_mut(PLAYER)
            .expect("rig")
            .quests
            .arcs
            .active
            .clear();
    }
    for place in [
        "tomb",
        "crypt",
        "ruin",
        "night market",
        "homestead",
        "sky town",
        "Moon",
    ] {
        assert!(
            real.get(place).copied().unwrap_or(0) > 0,
            "no arc bound a {place}"
        );
    }
}

#[test]
fn a_tomb_story_ending_breaks_the_seal_and_pays_furniture() {
    // "The Tale on the Seal" ends by singing the seal its song: the tomb it was bound to opens
    // (WP-TOMBS's story step) and a star chart goes to the player's furniture stock.
    let settings = WorldSettings::new(SEED);
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(30));
    let found = first_village(&world);
    set_time(&mut world, DayTime::at(1, 10, 0));
    make_veteran(&mut world);
    let template = ArcId(901).template().expect("arc 901");
    start_arc(&mut world, &found, template);
    let door = running(&world)[0].quest.targets[0].center;
    let site = crate::worldgen::streamed::tomb_sites::site_with_door(&settings, door)
        .expect("bound to a real tomb");
    let _ = play_to_climax(&mut world, template.id);
    let run_now = running(&world).remove(0);
    assert!(run_now.ready());
    let giver = run_now.cast.giver;
    let home = crate::people::routine::settlement(&settings, giver.village).expect("home");
    put_rig(
        &mut world,
        PLAYER,
        TilePosition {
            x: home.center_column(),
            y: home.ground_row - 1,
        },
    );
    let _ = run(&mut world, 30);
    let npc = world.npc_of(giver).expect("giver about");
    go_to(&mut world, npc);
    let charts = world
        .homes()
        .furniture_count(PLAYER, crate::home::Furniture::StarChart);
    assert!(!world.tombs.unsealed.contains(&site.id));
    let told = people(&mut world, PeopleCommand::TurnInArc { npc, outcome: 0 });
    assert_eq!(outcome(&told), PlayerScopedCommandOutcome::Applied);
    assert!(world.tombs.unsealed.contains(&site.id), "the seal opened");
    assert!(
        told.events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::TombSeal { opened: true, .. }))
    );
    assert_eq!(
        world
            .homes()
            .furniture_count(PLAYER, crate::home::Furniture::StarChart),
        charts + 1
    );
}

#[test]
fn giving_up_is_remembered_and_journalled() {
    let (mut world, found) = village_world();
    let (npc, arc) = giver_with(&world, &found, |_| true).expect("someone has a story");
    let giver = world.npc(npc).expect("npc").1.character;
    go_to(&mut world, npc);
    let _ = people(&mut world, PeopleCommand::AskWork { npc });
    let _ = people(&mut world, PeopleCommand::AcceptArc { npc });
    let gave_up = people(&mut world, PeopleCommand::AbandonArc { arc });
    assert_eq!(outcome(&gave_up), PlayerScopedCommandOutcome::Applied);
    assert!(gave_up.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::ArcAbandoned { arc: a, .. } if a == arc
    )));
    let log = world.arc_log(PLAYER).expect("log");
    assert_eq!(log.active, []);
    assert!(log.history.last().is_some_and(|record| record.abandoned));
    assert!(
        !log.finished_with(giver, arc),
        "a given-up story can be taken again"
    );
    let memory = world.people().character(giver).memory(PLAYER);
    assert!(
        memory
            .events
            .iter()
            .any(|event| matches!(event.fact, MemoryFact::ArcAbandoned { arc: a } if a == arc))
    );
    let again = people(&mut world, PeopleCommand::AbandonArc { arc });
    assert_eq!(
        outcome(&again),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
}

/// Removes every quest log's `arcs` field (a save from before stories).
fn strip_arcs(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if map.contains_key("board_completed") {
                map.remove("arcs");
            }
            for child in map.values_mut() {
                strip_arcs(child);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                strip_arcs(child);
            }
        }
        _ => {}
    }
}

#[test]
fn arcs_save_load_and_replicate() {
    let (mut world, found) = village_world();
    let (npc, _) = giver_with(&world, &found, |_| true).expect("someone has a story");
    go_to(&mut world, npc);
    let mut replica = world.redacted_for(Some(PLAYER));
    let _ = people(&mut world, PeopleCommand::AskWork { npc });
    let _ = people(&mut world, PeopleCommand::AcceptArc { npc });
    let _ = run(&mut world, 5);
    // Saves keep the arc log.
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.arc_log(PLAYER), world.arc_log(PLAYER));
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    // Saves from before stories load with an empty log.
    let mut old: serde_json::Value = serde_json::to_value(&world).expect("json");
    strip_arcs(&mut old);
    let old: WorldState = serde_json::from_value(old).expect("old save");
    assert!(
        old.arc_log(PLAYER)
            .is_some_and(crate::people::ArcLog::is_empty)
    );
    // The loadout (private) carries the log to its owner over the wire.
    let wire = |value: &crate::SectionPayload| -> crate::SectionPayload {
        let bytes = postcard::to_stdvec(value).expect("encode");
        postcard::from_bytes(&bytes).expect("decode")
    };
    for key in [SectionKey::Loadout(PLAYER), SectionKey::People] {
        let section = world.section(key).expect("section");
        replica.apply_section(key, wire(&section));
    }
    assert_eq!(replica.arc_log(PLAYER), world.arc_log(PLAYER));
    assert_eq!(replica.people(), world.people());
    // Another player never sees it.
    let other = world.redacted_for(Some(PlayerId::new(9)));
    assert!(
        other
            .arc_log(PLAYER)
            .is_none_or(crate::people::ArcLog::is_empty)
    );
}

#[test]
fn arcs_are_deterministic() {
    let play = || {
        let (mut world, found) = village_world();
        let (npc, arc) = giver_with(&world, &found, |_| true).expect("someone has a story");
        go_to(&mut world, npc);
        let _ = people(&mut world, PeopleCommand::AskWork { npc });
        let _ = people(&mut world, PeopleCommand::AcceptArc { npc });
        let _ = play_to_climax(&mut world, arc);
        world.simulation_state_hash()
    };
    assert_eq!(play(), play());
}

#[test]
fn two_seats_keep_their_own_stories() {
    let (mut world, found) = village_world();
    let second = PlayerId::new(2);
    world.add_player(second);
    let mut givers = npcs(&world)
        .into_iter()
        .filter(|(_, state)| state.character.village == found.id && state.inside.is_none())
        .filter_map(|(id, _)| match world.arc_talk(PLAYER, id) {
            ArcTalk::HasWork(arc) => Some((id, arc)),
            _ => None,
        });
    let (first_npc, _) = givers.next().expect("one giver");
    let (second_npc, _) = givers.next().expect("another giver");
    go_to(&mut world, first_npc);
    let _ = people(&mut world, PeopleCommand::AskWork { npc: first_npc });
    let _ = people(&mut world, PeopleCommand::AcceptArc { npc: first_npc });
    let (x, y) = world.npc_position(second_npc).expect("npc");
    {
        let rig = world.player_mut(second).expect("second rig");
        rig.x = x;
        rig.y = y - TILE_SIZE;
    }
    for command in [
        PeopleCommand::AskWork { npc: second_npc },
        PeopleCommand::AcceptArc { npc: second_npc },
    ] {
        let tick = world.simulation_tick();
        let output = world.simulate_tick(
            tick,
            &TickInputs::default().act(second, PlayerCommand::People(command)),
        );
        assert_eq!(
            output.action_outcomes[0].outcome,
            PlayerScopedCommandOutcome::Applied
        );
    }
    let mine = world.arc_log(PLAYER).expect("mine").active.clone();
    let theirs = world.arc_log(second).expect("theirs").active.clone();
    assert_eq!(mine.len(), 1);
    assert_eq!(theirs.len(), 1);
    assert_ne!(mine[0].cast.giver, theirs[0].cast.giver);
    // A player can't turn in someone else's story.
    let tick = world.simulation_tick();
    let output = world.simulate_tick(
        tick,
        &TickInputs::default().act(
            second,
            PlayerCommand::People(PeopleCommand::AbandonArc { arc: mine[0].arc() }),
        ),
    );
    if mine[0].arc() != theirs[0].arc() {
        assert_eq!(
            output.action_outcomes[0].outcome,
            PlayerScopedCommandOutcome::IgnoredUnavailable
        );
    }
    assert_eq!(world.arc_log(PLAYER).expect("mine").active.len(), 1);
}

#[test]
fn story_endings_leave_a_mark_on_villages_and_relationships() {
    let (mut world, found) = village_world();
    // A village's fortunes: the prosperity stories gave it prices its counters.
    let base = world.village_now(found.id).expect("village").prosperity;
    assert_eq!(base, found.prosperity);
    world.people.prosperity.insert(found.id, 2);
    let lifted = world.village_now(found.id).expect("village");
    assert_eq!(lifted.prosperity, (base + 2).min(5));
    let market = found
        .buildings
        .iter()
        .position(|lot| lot.kind == village::BuildingKind::Market);
    if let Some(market) = market
        && lifted.prosperity != base
    {
        assert_ne!(found.offers(market), lifted.offers(market), "prices move");
    }
    world.people.prosperity.insert(found.id, -9);
    assert_eq!(world.village_now(found.id).expect("village").prosperity, 1);
    // A changed relationship: word of the player now passes between the pair on its new terms.
    let settlement =
        crate::people::routine::settlement(world.world_settings(), found.id).expect("settlement");
    let people = &settlement.population.characters;
    let (giver, other) = (people[0].id, people[1].id);
    let hear = |world: &mut WorldState, bond| {
        world.people.characters.clear();
        world.people.bonds.clear();
        if let Some(bond) = bond {
            world.people.bonds.insert((giver, other), bond);
        }
        let before = world.people.affinity(giver, PLAYER);
        world.add_goodwill(PLAYER, giver, 24);
        world.spread_word(PLAYER, &settlement, giver, before);
        world.people.affinity(other, PLAYER)
    };
    assert!(
        hear(
            &mut world,
            Some(Some(crate::people::RelationshipKind::Rival))
        ) < 0
    );
    assert!(
        hear(
            &mut world,
            Some(Some(crate::people::RelationshipKind::Friend))
        ) > 0
    );
    assert_eq!(hear(&mut world, Some(None)), 0, "a relationship that ended");
}
