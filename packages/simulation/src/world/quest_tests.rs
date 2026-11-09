//! Quest gameplay through the real tick: accepting and turning in at the counters, solo and
//! crew objectives, twists, rewards, reputation, the HQ board, replication, saves and
//! determinism.
//!
//! Rigs are placed directly (a player without controls in a tick's inputs is not moved), so
//! each test reads as "be here, do that".

use crate::{
    GameplayEventKind, PlayerCommand, PlayerId, SectionKey, SimulationParams, TILE_SIZE,
    TickInputs, TickOutput, WorldState,
    quest::{
        ActiveQuest, Objective, QuestFailure, QuestGiver, QuestId, QuestItem, SURFACE_ROW,
        binding::giver_door,
    },
    service::SessionServiceRequest,
    terrain::{MineralKind, TileKind, TilePosition},
    worldgen::WorldSettings,
};

const LEAD: PlayerId = PlayerId::new(1);
const WING: PlayerId = PlayerId::new(2);

const SCALES: QuestId = QuestId(101);
const GLASS_JAW: QuestId = QuestId(103);
const TWO_COUNTERS: QuestId = QuestId(104);
const HEAVY_HAUL: QuestId = QuestId(105);
const DEMOLITION: QuestId = QuestId(301);
const TWIN_FUSES: QuestId = QuestId(305);
const DEAD_MANS_SWITCH: QuestId = QuestId(306);
const RELAY_RACE: QuestId = QuestId(503);
const SYNCHRONIZED_FUELING: QuestId = QuestId(505);

fn solo_world() -> WorldState {
    WorldState::new_game_with_settings(LEAD, &WorldSettings::new(3), SimulationParams::default())
}

fn crew_world() -> WorldState {
    let mut world = solo_world();
    world.add_player(WING);
    world
}

fn step(world: &mut WorldState, inputs: &TickInputs) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, inputs)
}

/// Runs `ticks` idle ticks, collecting events.
fn idle(world: &mut WorldState, ticks: u64) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(
            step(world, &TickInputs::default())
                .events
                .iter()
                .map(|event| event.kind),
        );
    }
    events
}

fn seconds(world: &WorldState, seconds: u64) -> u64 {
    world.timing().ticks_for_millis(seconds * 1_000)
}

/// Puts a rig at rest inside `tile` (cleared), sturdy and fuelled.
#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn place(world: &mut WorldState, player: PlayerId, tile: TilePosition) {
    if tile.y > SURFACE_ROW && world.terrain().is_solid_at(tile) {
        world.terrain_mut().set_kind(tile, TileKind::Air);
    }
    let rig = world.player_mut(player).expect("rig");
    rig.x = (tile.x as f32 + 0.5) * TILE_SIZE;
    rig.y = (tile.y as f32 + 0.5) * TILE_SIZE;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.fuel = rig.fuel_capacity;
    if rig.hull < 100.0 {
        rig.hull = 100.0;
    }
}

fn at_counter(world: &mut WorldState, player: PlayerId, giver: QuestGiver) {
    place(world, player, giver_door(giver));
}

fn service(world: &mut WorldState, player: PlayerId, request: SessionServiceRequest) -> TickOutput {
    step(
        world,
        &TickInputs::default().act(player, PlayerCommand::Service(request)),
    )
}

fn applied(output: &TickOutput) -> bool {
    output
        .action_outcomes
        .iter()
        .all(|outcome| outcome.outcome == crate::summary::PlayerScopedCommandOutcome::Applied)
}

fn accept(world: &mut WorldState, player: PlayerId, quest: QuestId, crew: bool) -> TickOutput {
    let giver = quest.def().expect("quest").giver;
    at_counter(world, player, giver);
    let output = service(
        world,
        player,
        SessionServiceRequest::AcceptQuest { quest, crew },
    );
    assert!(applied(&output), "{quest:?} was not accepted");
    assert!(output.events.iter().any(|event| event.kind
        == GameplayEventKind::QuestAccepted {
            player_id: player,
            quest
        }));
    output
}

fn turn_in(world: &mut WorldState, player: PlayerId, quest: QuestId) -> TickOutput {
    let giver = quest.def().expect("quest").giver;
    at_counter(world, player, giver);
    service(world, player, SessionServiceRequest::TurnInQuest { quest })
}

/// The quest `owner` has going (solo or crew).
fn active(world: &WorldState, owner: PlayerId, quest: QuestId) -> Option<ActiveQuest> {
    world
        .visible_quests(owner)
        .into_iter()
        .find(|active| active.quest == quest && active.owner == owner)
        .cloned()
}

/// Progress of the quest's objective number `index`.
fn spots(world: &WorldState, owner: PlayerId, quest: QuestId, index: usize) -> Vec<TilePosition> {
    active(world, owner, quest).expect("active").objectives[index]
        .spots
        .clone()
}

fn target(world: &WorldState, owner: PlayerId, quest: QuestId, index: usize) -> TilePosition {
    active(world, owner, quest).expect("active").targets[index].center
}

fn unlock(world: &mut WorldState, player: PlayerId, quest: QuestId) {
    let def = quest.def().expect("quest");
    let terrain = world.terrain().clone();
    let rig = world.player_mut(player).expect("rig");
    for prerequisite in def.prerequisites {
        let scaled = match *prerequisite {
            crate::quest::Prerequisite::Depth(row) => terrain.scaled_row(row),
            _ => 0,
        };
        match *prerequisite {
            crate::quest::Prerequisite::Depth(_) => rig.quests.deepest_row = scaled,
            crate::quest::Prerequisite::Quest(other) => {
                rig.quests.completed.insert(other);
            }
            crate::quest::Prerequisite::Reputation(giver, points) => {
                rig.quests.reputation.insert(giver, points);
            }
            crate::quest::Prerequisite::Upgrade(_, level) => rig.scanner_level = level,
            crate::quest::Prerequisite::Logs(count) => {
                rig.journal
                    .logs
                    .extend((0..u16::from(count)).map(crate::poi::LoreId));
            }
            crate::quest::Prerequisite::Visited(kind) => {
                rig.quests.visited_kinds.insert(kind);
            }
        }
    }
}

/// Places a bomb at `spot` and waits for it to go off.
fn bomb_at(world: &mut WorldState, player: PlayerId, spot: TilePosition) -> Vec<GameplayEventKind> {
    place(world, player, spot);
    world.player_mut(player).expect("rig").bombs += 1;
    world.player_mut(player).expect("rig").hull = 10_000.0;
    let mut events: Vec<GameplayEventKind> = step(
        world,
        &TickInputs::default().act(player, PlayerCommand::PlaceBomb),
    )
    .events
    .iter()
    .map(|event| event.kind)
    .collect();
    let fuse = seconds(world, 4);
    events.extend(idle(world, fuse));
    events
}

#[test]
fn quests_are_accepted_and_turned_in_only_at_their_givers_counter() {
    let mut world = solo_world();
    at_counter(&mut world, LEAD, QuestGiver::Pip);
    let refused = service(
        &mut world,
        LEAD,
        SessionServiceRequest::AcceptQuest {
            quest: SCALES,
            crew: false,
        },
    );
    assert!(!applied(&refused), "Kade's quest is not on Pip's desk");
    accept(&mut world, LEAD, SCALES, false);
    assert!(active(&world, LEAD, SCALES).is_some());
    // Not ready without the iron.
    assert!(!applied(&turn_in(&mut world, LEAD, SCALES)));
    world
        .player_mut(LEAD)
        .expect("rig")
        .cargo
        .insert(MineralKind::Iron, 6);
    let events = idle(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::QuestReady {
        player_id: LEAD,
        quest: SCALES
    }));
    let credits = world.player(LEAD).expect("rig").credits;
    let output = turn_in(&mut world, LEAD, SCALES);
    assert!(applied(&output));
    assert!(output.events.iter().any(|event| event.kind
        == GameplayEventKind::QuestCompleted {
            player_id: LEAD,
            quest: SCALES
        }));
    let rig = world.player(LEAD).expect("rig");
    assert_eq!(rig.credits, credits + 170);
    assert_eq!(rig.cargo.get(&MineralKind::Iron), Some(&1));
    assert_eq!(rig.quests.reputation(QuestGiver::Kade), 6);
    assert!(rig.quests.completed.contains(&SCALES));
    assert!(active(&world, LEAD, SCALES).is_none());
    // A finished quest is not offered again.
    assert!(
        !world
            .quest_offers(LEAD, QuestGiver::Kade)
            .iter()
            .any(|offer| offer.quest == SCALES)
    );
}

#[test]
fn a_time_limit_fails_the_quest_when_it_runs_out() {
    let mut world = solo_world();
    accept(&mut world, LEAD, SCALES, false);
    let limit = seconds(&world, 420);
    let events = idle(&mut world, limit + 2);
    assert!(events.contains(&GameplayEventKind::QuestFailed {
        player_id: LEAD,
        quest: SCALES,
        reason: QuestFailure::OutOfTime
    }));
    assert!(active(&world, LEAD, SCALES).is_none());
    let rig = world.player(LEAD).expect("rig");
    assert_eq!(rig.quests.failures.get(&SCALES), Some(&1));
    // Failed quests can be taken again.
    accept(&mut world, LEAD, SCALES, false);
}

#[test]
fn detonations_must_follow_the_marked_order() {
    let mut world = solo_world();
    accept(&mut world, LEAD, DEMOLITION, false);
    let marks = spots(&world, LEAD, DEMOLITION, 0);
    assert_eq!(marks.len(), 2);
    // Out of order: the second mark first does nothing.
    let _ = bomb_at(&mut world, LEAD, marks[1]);
    assert_eq!(
        active(&world, LEAD, DEMOLITION).expect("active").objectives[0].count,
        0
    );
    let _ = bomb_at(&mut world, LEAD, marks[0]);
    assert_eq!(
        active(&world, LEAD, DEMOLITION).expect("active").objectives[0].count,
        1
    );
    let events = bomb_at(&mut world, LEAD, marks[1]);
    assert!(events.contains(&GameplayEventKind::QuestReady {
        player_id: LEAD,
        quest: DEMOLITION
    }));
    let bombs = world.player(LEAD).expect("rig").bombs;
    assert!(applied(&turn_in(&mut world, LEAD, DEMOLITION)));
    assert_eq!(world.player(LEAD).expect("rig").bombs, bombs + 3);
}

#[test]
fn fragile_parcels_fail_when_the_rig_takes_too_much_damage() {
    let mut world = solo_world();
    unlock(&mut world, LEAD, GLASS_JAW);
    accept(&mut world, LEAD, GLASS_JAW, false);
    // Damage before the pickup does not count.
    world.player_mut(LEAD).expect("rig").hull = 60.0;
    let _ = idle(&mut world, 1);
    let cache = target(&world, LEAD, GLASS_JAW, 0);
    place(&mut world, LEAD, cache);
    world.player_mut(LEAD).expect("rig").hull = 100.0;
    let _ = idle(&mut world, 1);
    let quest = active(&world, LEAD, GLASS_JAW).expect("active");
    assert!(quest.carrying);
    assert_eq!(quest.damage_taken, 0);
    world.player_mut(LEAD).expect("rig").hull = 80.0;
    let events = idle(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::QuestFailed {
        player_id: LEAD,
        quest: GLASS_JAW,
        reason: QuestFailure::TooMuchDamage
    }));
}

#[test]
fn a_careful_courier_brings_the_parcel_home() {
    let mut world = solo_world();
    unlock(&mut world, LEAD, GLASS_JAW);
    accept(&mut world, LEAD, GLASS_JAW, false);
    let cache = target(&world, LEAD, GLASS_JAW, 0);
    place(&mut world, LEAD, cache);
    let _ = idle(&mut world, 1);
    world.player_mut(LEAD).expect("rig").hull = 95.0;
    let _ = idle(&mut world, 1);
    at_counter(&mut world, LEAD, QuestGiver::Kade);
    let events = idle(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::QuestReady {
        player_id: LEAD,
        quest: GLASS_JAW
    }));
    assert!(applied(&turn_in(&mut world, LEAD, GLASS_JAW)));
}

#[test]
fn crew_rigs_light_simultaneous_marks_together_and_share_the_reward() {
    let mut world = crew_world();
    unlock(&mut world, LEAD, TWIN_FUSES);
    accept(&mut world, LEAD, TWIN_FUSES, true);
    let quest = active(&world, LEAD, TWIN_FUSES).expect("active");
    assert!(quest.crew);
    assert!(
        world
            .crew_quests()
            .active
            .iter()
            .any(|q| q.quest == TWIN_FUSES)
    );
    assert!(world.player(LEAD).expect("rig").quests.active.is_empty());
    // The wing sees the crew quest too.
    assert!(
        world
            .visible_quests(WING)
            .iter()
            .any(|q| q.quest == TWIN_FUSES)
    );
    let fuses = spots(&world, LEAD, TWIN_FUSES, 0);
    // One rig at a time is too slow for a crew window.
    place(&mut world, LEAD, fuses[0]);
    let _ = idle(&mut world, 1);
    place(&mut world, LEAD, TilePosition { x: 20, y: 4 });
    let late = seconds(&world, 4);
    let _ = idle(&mut world, late);
    place(&mut world, LEAD, fuses[1]);
    let _ = idle(&mut world, 1);
    assert_eq!(active(&world, LEAD, TWIN_FUSES).expect("active").stage, 0);
    // Both rigs together.
    place(&mut world, LEAD, fuses[0]);
    place(&mut world, WING, fuses[1]);
    let events = idle(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::QuestStageDone {
        player_id: LEAD,
        quest: TWIN_FUSES
    }));
    let marks = spots(&world, LEAD, TWIN_FUSES, 1);
    let _ = bomb_at(&mut world, WING, marks[0]);
    let events = bomb_at(&mut world, WING, marks[1]);
    assert!(events.contains(&GameplayEventKind::QuestReady {
        player_id: LEAD,
        quest: TWIN_FUSES
    }));
    let wing_credits = world.player(WING).expect("wing").credits;
    assert!(applied(&turn_in(&mut world, LEAD, TWIN_FUSES)));
    assert_eq!(
        world.player(WING).expect("wing").credits,
        wing_credits + 680 / 4
    );
    assert!(world.crew_quests().active.is_empty());
}

#[test]
fn a_solo_pilot_gets_the_long_window_for_simultaneous_marks() {
    let mut world = solo_world();
    unlock(&mut world, LEAD, TWIN_FUSES);
    // Crew mode needs a crew: alone, the request falls back to the solo variant.
    accept(&mut world, LEAD, TWIN_FUSES, true);
    let quest = active(&world, LEAD, TWIN_FUSES).expect("active");
    assert!(!quest.crew);
    let fuses = spots(&world, LEAD, TWIN_FUSES, 0);
    place(&mut world, LEAD, fuses[0]);
    let _ = idle(&mut world, 1);
    place(&mut world, LEAD, TilePosition { x: 20, y: 4 });
    let wait = seconds(&world, 10);
    let _ = idle(&mut world, wait);
    place(&mut world, LEAD, fuses[1]);
    let _ = idle(&mut world, 1);
    assert_eq!(active(&world, LEAD, TWIN_FUSES).expect("active").stage, 1);
}

#[test]
fn hold_and_climb_needs_the_switch_held_or_latched() {
    // Crew: the wing holds the switch while the lead reaches the formation.
    let mut world = crew_world();
    unlock(&mut world, LEAD, DEAD_MANS_SWITCH);
    accept(&mut world, LEAD, DEAD_MANS_SWITCH, true);
    let switch = spots(&world, LEAD, DEAD_MANS_SWITCH, 0)[0];
    let goal = target(&world, LEAD, DEAD_MANS_SWITCH, 1);
    place(&mut world, LEAD, goal);
    let _ = idle(&mut world, 2);
    assert!(
        !active(&world, LEAD, DEAD_MANS_SWITCH)
            .expect("active")
            .ready
    );
    place(&mut world, WING, switch);
    let events = idle(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::QuestReady {
        player_id: LEAD,
        quest: DEAD_MANS_SWITCH
    }));

    // Solo: the switch latches for half a minute after the rig leaves it.
    let mut world = solo_world();
    unlock(&mut world, LEAD, DEAD_MANS_SWITCH);
    accept(&mut world, LEAD, DEAD_MANS_SWITCH, false);
    let switch = spots(&world, LEAD, DEAD_MANS_SWITCH, 0)[0];
    let goal = target(&world, LEAD, DEAD_MANS_SWITCH, 1);
    place(&mut world, LEAD, switch);
    let _ = idle(&mut world, 1);
    let latch = seconds(&world, 31);
    place(&mut world, LEAD, TilePosition { x: 20, y: 4 });
    let _ = idle(&mut world, latch);
    place(&mut world, LEAD, goal);
    let _ = idle(&mut world, 2);
    assert!(
        !active(&world, LEAD, DEAD_MANS_SWITCH)
            .expect("active")
            .ready,
        "latch expired"
    );
    place(&mut world, LEAD, switch);
    let _ = idle(&mut world, 1);
    place(&mut world, LEAD, goal);
    let events = idle(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::QuestReady {
        player_id: LEAD,
        quest: DEAD_MANS_SWITCH
    }));
}

#[test]
fn the_relay_marker_passes_rig_to_rig_or_cools_for_a_solo_pilot() {
    // Crew hand-off.
    let mut world = crew_world();
    unlock(&mut world, LEAD, RELAY_RACE);
    accept(&mut world, LEAD, RELAY_RACE, true);
    let door = giver_door(QuestGiver::Pip);
    let goal = target(&world, LEAD, RELAY_RACE, 0);
    let _ = idle(&mut world, 1);
    let quest = active(&world, LEAD, RELAY_RACE).expect("active");
    assert_eq!(
        quest.objectives[0].carrier,
        Some(LEAD),
        "the lead picks it up at the door"
    );
    // One row past the relay's leg (authored in classic rows, stretched in deep worlds).
    let leg_rows = RELAY_RACE
        .def()
        .expect("quest")
        .objectives()
        .find_map(|objective| match objective {
            crate::quest::Objective::Relay { leg_rows, .. } => Some(i32::from(leg_rows)),
            _ => None,
        })
        .expect("a relay");
    let hot = TilePosition {
        x: door.x,
        y: door.y + crate::worldgen::strata::scaled_span(world.terrain().settings(), leg_rows) + 1,
    };
    place(&mut world, LEAD, hot);
    let _ = idle(&mut world, 1);
    assert_eq!(
        active(&world, LEAD, RELAY_RACE).expect("active").objectives[0].carrier,
        None
    );
    place(&mut world, WING, hot);
    let _ = idle(&mut world, 1);
    assert_eq!(
        active(&world, LEAD, RELAY_RACE).expect("active").objectives[0].carrier,
        Some(WING),
        "a crewmate takes the dropped marker at once"
    );
    place(&mut world, WING, goal);
    let events = idle(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::QuestReady {
        player_id: LEAD,
        quest: RELAY_RACE
    }));

    // Solo: the dropped marker cools before the same rig can take it on.
    let mut world = solo_world();
    unlock(&mut world, LEAD, RELAY_RACE);
    accept(&mut world, LEAD, RELAY_RACE, false);
    let _ = idle(&mut world, 1);
    place(&mut world, LEAD, hot);
    let _ = idle(&mut world, 2);
    assert_eq!(
        active(&world, LEAD, RELAY_RACE).expect("active").objectives[0].carrier,
        None
    );
    let _ = idle(&mut world, 5);
    assert_eq!(
        active(&world, LEAD, RELAY_RACE).expect("active").objectives[0].carrier,
        None
    );
    let cooldown = seconds(&world, 8);
    let _ = idle(&mut world, cooldown);
    assert_eq!(
        active(&world, LEAD, RELAY_RACE).expect("active").objectives[0].carrier,
        Some(LEAD)
    );
}

#[test]
fn two_rigs_cannot_pass_the_relay_marker_back_and_forth_every_tick() {
    let mut world = crew_world();
    unlock(&mut world, LEAD, RELAY_RACE);
    accept(&mut world, LEAD, RELAY_RACE, true);
    let door = giver_door(QuestGiver::Pip);
    let carrier =
        |world: &WorldState| active(world, LEAD, RELAY_RACE).expect("active").objectives[0].carrier;
    let _ = idle(&mut world, 1);
    assert_eq!(carrier(&world), Some(LEAD));
    place(
        &mut world,
        WING,
        TilePosition {
            x: door.x + 1,
            y: door.y,
        },
    );
    let _ = idle(&mut world, 1);
    assert_eq!(carrier(&world), Some(WING), "handed to the wing");
    let _ = idle(&mut world, 5);
    assert_eq!(
        carrier(&world),
        Some(WING),
        "it doesn't bounce straight back to the lead"
    );
    // Once the wing has carried a few rows, the lead can take the next leg.
    let below = door.y + 7;
    place(
        &mut world,
        WING,
        TilePosition {
            x: door.x,
            y: below,
        },
    );
    place(
        &mut world,
        LEAD,
        TilePosition {
            x: door.x + 1,
            y: below,
        },
    );
    let _ = idle(&mut world, 1);
    assert_eq!(carrier(&world), Some(LEAD));
}

#[test]
fn a_two_rig_crew_can_work_a_three_spot_job_with_one_runner() {
    let mut world = crew_world();
    unlock(&mut world, LEAD, SYNCHRONIZED_FUELING);
    accept(&mut world, LEAD, SYNCHRONIZED_FUELING, true);
    assert!(
        active(&world, LEAD, SYNCHRONIZED_FUELING)
            .expect("active")
            .crew
    );
    let valves = spots(&world, LEAD, SYNCHRONIZED_FUELING, 0);
    assert_eq!(valves.len(), 3);
    place(&mut world, LEAD, valves[0]);
    place(&mut world, WING, valves[1]);
    let _ = idle(&mut world, 1);
    // The wing runs on to the third valve: slower than the three-rig window, but two rigs get
    // a stretched one.
    place(&mut world, WING, TilePosition { x: 20, y: 4 });
    let run = seconds(&world, 8);
    let _ = idle(&mut world, run);
    place(&mut world, WING, valves[2]);
    let events = idle(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::QuestReady {
        player_id: LEAD,
        quest: SYNCHRONIZED_FUELING
    }));
}

/// Opens a straight shaft from `from` up to the sky.
fn open_shaft(world: &mut WorldState, from: TilePosition) {
    for y in 0..=from.y {
        world
            .terrain_mut()
            .set_kind(TilePosition { x: from.x, y }, TileKind::Air);
    }
}

/// Walks rigs beside the haul until it surfaces; returns the ticks it took.
fn escort_home(world: &mut WorldState, rigs: &[PlayerId]) -> u64 {
    for tick in 0..200_000 {
        let Some(quest) = active(world, LEAD, HEAVY_HAUL) else {
            panic!("the escort ended early");
        };
        if quest.ready {
            return tick;
        }
        let haul = quest.objectives[0].position.expect("haul");
        for (index, rig) in rigs.iter().enumerate() {
            let offset = i32::try_from(index).unwrap_or(0) + 1;
            place(
                world,
                *rig,
                TilePosition {
                    x: haul.x,
                    y: haul.y + offset.min(2),
                },
            );
        }
        let _ = idle(world, 1);
    }
    panic!("the haul never surfaced");
}

#[test]
fn an_escorted_haul_climbs_open_tunnel_and_two_rigs_push_it_faster() {
    let mut solo = solo_world();
    unlock(&mut solo, LEAD, HEAVY_HAUL);
    accept(&mut solo, LEAD, HEAVY_HAUL, false);
    let start = active(&solo, LEAD, HEAVY_HAUL).expect("active").objectives[0]
        .position
        .expect("haul");
    open_shaft(&mut solo, start);
    let solo_ticks = escort_home(&mut solo, &[LEAD]);
    // A crew quest pushed by two rigs.
    let mut crew_world = crew_world();
    unlock(&mut crew_world, LEAD, HEAVY_HAUL);
    accept(&mut crew_world, LEAD, HEAVY_HAUL, true);
    let start = active(&crew_world, LEAD, HEAVY_HAUL)
        .expect("active")
        .objectives[0]
        .position
        .expect("haul");
    open_shaft(&mut crew_world, start);
    let crew_ticks = escort_home(&mut crew_world, &[LEAD, WING]);
    assert!(
        crew_ticks * 3 < solo_ticks * 2,
        "crew {crew_ticks} vs solo {solo_ticks}"
    );
    assert!(applied(&turn_in(&mut crew_world, LEAD, HEAVY_HAUL)));
    let rig = crew_world.player(LEAD).expect("rig");
    assert_eq!(
        rig.quests
            .fragments_of(crate::quest::BlueprintId::HaulerFrame),
        1
    );
}

#[test]
fn split_deliveries_must_land_close_together() {
    let silver = |world: &mut WorldState, player: PlayerId| {
        world
            .player_mut(player)
            .expect("rig")
            .cargo
            .insert(MineralKind::Silver, 2);
    };
    // Crew: both halves at once.
    let mut world = crew_world();
    unlock(&mut world, LEAD, TWO_COUNTERS);
    accept(&mut world, LEAD, TWO_COUNTERS, true);
    silver(&mut world, LEAD);
    silver(&mut world, WING);
    place(&mut world, LEAD, giver_door(QuestGiver::Kade));
    place(&mut world, WING, giver_door(QuestGiver::Mara));
    let events = idle(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::QuestReady {
        player_id: LEAD,
        quest: TWO_COUNTERS
    }));
    assert!(
        !world
            .player(WING)
            .expect("wing")
            .cargo
            .contains_key(&MineralKind::Silver)
    );

    // Solo: a half that waits past the window spoils.
    let mut world = solo_world();
    unlock(&mut world, LEAD, TWO_COUNTERS);
    accept(&mut world, LEAD, TWO_COUNTERS, false);
    silver(&mut world, LEAD);
    let _ = idle(&mut world, 1);
    let quest = active(&world, LEAD, TWO_COUNTERS).expect("active");
    assert_eq!(quest.objectives[0].counts, vec![2, 0]);
    place(&mut world, LEAD, TilePosition { x: 20, y: 4 });
    let window = seconds(&world, 121);
    let _ = idle(&mut world, window);
    assert_eq!(
        active(&world, LEAD, TWO_COUNTERS)
            .expect("active")
            .objectives[0]
            .counts,
        vec![0, 0]
    );
}

#[test]
fn the_hq_board_posts_daily_jobs_each_taken_once() {
    let mut world = solo_world();
    let row = world.terrain().scaled_row(30);
    world.player_mut(LEAD).expect("rig").quests.deepest_row = row;
    world
        .player_mut(LEAD)
        .expect("rig")
        .quests
        .completed
        .insert(DEMOLITION);
    let offers = world.quest_offers(LEAD, QuestGiver::Vale);
    assert_eq!(offers.len(), 3);
    let posting = offers[0].posting.expect("posting");
    let _ = accept(&mut world, LEAD, posting.quest, false);
    let active = active(&world, LEAD, posting.quest).expect("active");
    assert_eq!(active.variant, posting.variant);
    let offers = world.quest_offers(LEAD, QuestGiver::Vale);
    assert_eq!(offers.len(), 2, "a taken posting leaves the board");
    let output = service(
        &mut world,
        LEAD,
        SessionServiceRequest::AbandonQuest {
            quest: posting.quest,
        },
    );
    assert!(applied(&output));
    let again = service(
        &mut world,
        LEAD,
        SessionServiceRequest::AcceptQuest {
            quest: posting.quest,
            crew: false,
        },
    );
    assert!(!applied(&again), "each posting can be taken once a day");
}

#[test]
fn reputation_earns_rebates_at_the_givers_counter() {
    let mut world = solo_world();
    at_counter(&mut world, LEAD, QuestGiver::Pip);
    let rig = world.player_mut(LEAD).expect("rig");
    rig.fuel = 0.0;
    rig.credits = 1_000;
    rig.quests.reputation.insert(QuestGiver::Pip, 25);
    let _ = step(
        &mut world,
        &TickInputs::default().act(LEAD, PlayerCommand::Refuel),
    );
    let rig = world.player(LEAD).expect("rig");
    let spent = 1_000 - rig.credits;
    // Rank 2 hands back 8%: a full tank (100 fuel at 3 credits) costs 276 instead of 300.
    let full = 100 * crate::economy::REFUEL_UNIT_COST;
    assert_eq!(spent, full - full * 8 / 100);
}

#[test]
fn deliveries_and_parcels_need_their_items_at_turn_in() {
    let mut world = solo_world();
    accept(&mut world, LEAD, SCALES, false);
    let def = SCALES.def().expect("def");
    assert!(matches!(
        def.stages[0][0],
        Objective::Deliver {
            item: QuestItem::Mineral(MineralKind::Iron),
            count: 5
        }
    ));
    world
        .player_mut(LEAD)
        .expect("rig")
        .cargo
        .insert(MineralKind::Iron, 5);
    let _ = idle(&mut world, 1);
    // Selling the iron before turning in leaves nothing to hand over.
    world.player_mut(LEAD).expect("rig").cargo.clear();
    assert!(!applied(&turn_in(&mut world, LEAD, SCALES)));
}

/// A crew world mid-way through a crew quest and a solo quest.
fn mid_quest_world() -> WorldState {
    let mut world = crew_world();
    unlock(&mut world, LEAD, TWIN_FUSES);
    accept(&mut world, LEAD, TWIN_FUSES, true);
    accept(&mut world, WING, DEMOLITION, false);
    let fuses = spots(&world, LEAD, TWIN_FUSES, 0);
    place(&mut world, LEAD, fuses[0]);
    let _ = idle(&mut world, 3);
    world
}

#[test]
fn quest_state_replicates_privately_and_publicly() {
    let authority = mid_quest_world();
    let mut replica = WorldState::new_game_with_settings(
        LEAD,
        &WorldSettings::new(3),
        SimulationParams::default(),
    );
    for key in authority.section_keys() {
        if let Some(section) = authority.section(key) {
            replica.apply_section(key, section);
        }
    }
    for key in [
        SectionKey::Quests,
        SectionKey::Loadout(LEAD),
        SectionKey::Loadout(WING),
    ] {
        assert_eq!(
            replica.section_hash(key),
            authority.section_hash(key),
            "{key:?}"
        );
    }
    assert_eq!(replica.crew_quests(), authority.crew_quests());
    // The wing's solo quest is private; the crew quest is shared.
    let seen_by_lead = authority.redacted_for(Some(LEAD));
    assert!(
        seen_by_lead
            .player(WING)
            .expect("wing")
            .quests
            .active
            .is_empty()
    );
    assert_eq!(seen_by_lead.crew_quests().active.len(), 1);
    assert_eq!(
        seen_by_lead.visible_state_hash(Some(LEAD)),
        authority.visible_state_hash(Some(LEAD))
    );
}

#[test]
fn quests_survive_a_save_round_trip_and_finish_identically() {
    let mut original = mid_quest_world();
    let saved = serde_json::to_string(&original).expect("save");
    let mut restored: WorldState = serde_json::from_str(&saved).expect("load");
    assert_eq!(
        restored.simulation_state_hash(),
        original.simulation_state_hash()
    );
    for world in [&mut original, &mut restored] {
        let fuses = spots(world, LEAD, TWIN_FUSES, 0);
        place(world, LEAD, fuses[0]);
        place(world, WING, fuses[1]);
        let _ = idle(world, 2);
        assert_eq!(active(world, LEAD, TWIN_FUSES).expect("active").stage, 1);
    }
    assert_eq!(
        restored.simulation_state_hash(),
        original.simulation_state_hash()
    );
}

#[test]
fn saves_from_before_quests_load_with_empty_quest_logs() {
    let world = solo_world();
    let mut value = serde_json::to_value(&world).expect("save");
    let players = value["players"].as_object_mut().expect("players");
    for player in players.values_mut() {
        player.as_object_mut().expect("player").remove("quests");
    }
    value["progression"]
        .as_object_mut()
        .expect("progression")
        .remove("crew_quests");
    let old: WorldState = serde_json::from_value(value).expect("old save");
    assert!(old.player(LEAD).expect("rig").quests.active.is_empty());
    assert!(old.crew_quests().active.is_empty());
}

#[test]
fn quests_play_out_deterministically() {
    let run = || {
        let mut world = mid_quest_world();
        let marks = spots(&world, WING, DEMOLITION, 0);
        let _ = bomb_at(&mut world, WING, marks[0]);
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
}
