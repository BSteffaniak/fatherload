//! People in the world (WP-PEOPLE): population around rigs, clock-driven routines, determinism,
//! talking (variety, reach, goodwill), trades, gifts, persistence and replication.

use std::{collections::BTreeSet, sync::Arc};

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, PlayerScopedCommandOutcome,
    SectionKey, SimulationParams, SimulationTick, TickInputs, TickOutput, WorldState,
    crew::CrewMember,
    entity::{EntityDelta, EntityId, EntityKind, EntityState},
    item::ItemKind,
    people::{
        DayTime, PeopleCommand, Standing, TalkTopic, TickClock,
        clock::DEFAULT_DAY_LENGTH_MINUTES,
        ledger::Speaker,
        offers::{self, GiftReaction, PersonDeal},
        routine::{Activity, TOWN_VILLAGE},
    },
    terrain::TILE_SIZE,
    village::{self, Village, VillageDeal, placement},
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(1);
const SEED: u64 = 0x5EED_0017;

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

/// Puts `player`'s rig on the ground at world pixel column `x` of `found`.
fn park_at(world: &mut WorldState, player: PlayerId, found: &Village, x: f32) {
    let rig = world.player_mut(player).expect("rig");
    rig.x = x;
    #[allow(clippy::cast_precision_loss, reason = "small rows")]
    {
        rig.y = (found.ground_row as f32 - 0.5) * TILE_SIZE;
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.x_remainder = 0.0;
}

#[allow(clippy::cast_precision_loss, reason = "small columns")]
fn column_x(column: i32) -> f32 {
    (column as f32 + 0.5) * TILE_SIZE
}

fn step(world: &mut WorldState, inputs: &TickInputs) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, inputs)
}

fn idle() -> TickInputs {
    TickInputs::held(PLAYER, ControlState::new(0.0, false, false))
}

fn act(world: &mut WorldState, command: PlayerCommand) -> TickOutput {
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(PLAYER, command);
    world.simulate_tick(tick, &inputs)
}

fn run(world: &mut WorldState, ticks: u64) {
    for _ in 0..ticks {
        let _ = step(world, &idle());
    }
}

/// Jumps the clock to `time` (the world clock is derived from the tick).
fn set_time(world: &mut WorldState, time: DayTime) {
    let tick = TickClock::tick_at(time, world.tick_rate_hz(), DEFAULT_DAY_LENGTH_MINUTES);
    world.set_simulation_tick(SimulationTick::new(tick));
}

fn npcs_of(
    world: &WorldState,
    village: village::VillageId,
) -> Vec<(EntityId, crate::entity::NpcState)> {
    world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter_map(|entity| match entity.state {
            EntityState::Npc(state) if state.character.village == village => {
                Some((entity.id, state))
            }
            _ => None,
        })
        .collect()
}

fn outcome(output: &TickOutput) -> PlayerScopedCommandOutcome {
    output.action_outcomes.first().expect("one action").outcome
}

/// A world with the rig parked on the first village's plaza at 10:00 on day 1, residents
/// spawned and walking.
fn village_world() -> (WorldState, Arc<Village>) {
    let mut world = world_at(60);
    let found = first_village(&world);
    set_time(&mut world, DayTime::at(1, 10, 0));
    let plaza = i32::midpoint(found.plaza.0, found.plaza.1);
    park_at(&mut world, PLAYER, &found, column_x(plaza));
    run(&mut world, 40);
    (world, found)
}

/// An outdoor person in `found` and the rig moved next to them.
fn meet_someone(world: &mut WorldState, found: &Village) -> EntityId {
    let (id, _) = npcs_of(world, found.id)
        .into_iter()
        .find(|(_, state)| state.inside.is_none())
        .expect("someone outdoors");
    approach(world, found, id);
    id
}

fn approach(world: &mut WorldState, found: &Village, id: EntityId) {
    let (x, _) = world.npc_position(id).expect("npc");
    park_at(world, PLAYER, found, x);
}

#[test]
fn residents_appear_near_rigs_and_leave_when_nobody_is_near() {
    let (mut world, found) = village_world();
    let present = npcs_of(&world, found.id);
    assert_eq!(present.len(), found.population.characters.len());
    let characters: BTreeSet<_> = present.iter().map(|(_, state)| state.character).collect();
    assert_eq!(characters.len(), present.len(), "everyone exactly once");
    // Fly far away: the village empties out.
    park_at(
        &mut world,
        PLAYER,
        &found,
        column_x(found.last_column + 400),
    );
    run(&mut world, 40);
    assert!(npcs_of(&world, found.id).is_empty());
    // Town folk walk the town street on worlds with villages.
    let town = world.terrain().surface_row(97);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.x = column_x(110);
    #[allow(clippy::cast_precision_loss, reason = "small rows")]
    {
        rig.y = (town as f32 - 0.5) * TILE_SIZE;
    }
    run(&mut world, 40);
    assert_eq!(
        npcs_of(&world, TOWN_VILLAGE).len(),
        crate::people::routine::TOWN_FOLK
    );
}

#[test]
fn routines_follow_the_clock() {
    let mut world = world_at(60);
    let found = first_village(&world);
    let plaza = i32::midpoint(found.plaza.0, found.plaza.1);
    set_time(&mut world, DayTime::at(1, 2, 30));
    park_at(&mut world, PLAYER, &found, column_x(plaza));
    run(&mut world, 40);
    let night = world.people_about();
    assert!(!night.is_empty());
    for (character, activity, inside) in &night {
        assert_eq!(*activity, Activity::Sleeping, "{character:?}");
        assert!(inside.is_some(), "{character:?} sleeps indoors");
    }
    // Morning comes: they leave home on their own, walking.
    set_time(&mut world, DayTime::at(1, 10, 30));
    run(&mut world, 60 * 40);
    let day = world.people_about();
    assert!(
        day.iter()
            .all(|(_, activity, _)| *activity == Activity::Working),
        "{day:?}"
    );
    let outdoors = day.iter().filter(|(_, _, inside)| inside.is_none()).count();
    assert!(outdoors > 0, "someone works outdoors: {day:?}");
    // Evening at the inn (for a village with one) or home.
    set_time(&mut world, DayTime::at(1, 20, 45));
    run(&mut world, 60 * 60);
    let evening = world.people_about();
    assert!(
        evening
            .iter()
            .all(|(_, activity, _)| matches!(activity, Activity::AtInn | Activity::Sleeping)),
        "{evening:?}"
    );
}

#[test]
fn people_walk_deterministically_and_rest_without_churn() {
    let run_world = || {
        let (mut world, _) = village_world();
        run(&mut world, 1_800);
        world
    };
    let a = run_world();
    let b = run_world();
    assert_eq!(a.simulation_state_hash(), b.simulation_state_hash());
    let positions = |world: &WorldState| -> Vec<(u64, u32)> {
        world
            .entities()
            .of_kind(EntityKind::Npc)
            .filter_map(|entity| match entity.state {
                EntityState::Npc(state) => Some((
                    entity.id.get(),
                    state.x_at(world.simulation_tick(), 0.0).to_bits(),
                )),
                _ => None,
            })
            .collect()
    };
    assert_eq!(positions(&a), positions(&b));
    // Somebody moved during those 30 seconds.
    let (start, _) = village_world();
    assert_ne!(positions(&start), positions(&a));
    // Walks are legs: a person's entity changes when a leg starts or ends, not every tick, so
    // walking people cost little on the wire.
    let (mut world, found) = village_world();
    let revisions = |world: &WorldState| -> std::collections::BTreeMap<EntityId, u64> {
        world
            .entities()
            .of_kind(EntityKind::Npc)
            .map(|entity| (entity.id, entity.revision))
            .collect()
    };
    let mut last = revisions(&world);
    let mut changes = 0;
    for _ in 0..1_800 {
        let _ = step(&mut world, &idle());
        let now = revisions(&world);
        changes += now
            .iter()
            .filter(|(id, revision)| last.get(id).is_some_and(|old| old != *revision))
            .count();
        last = now;
    }
    let people = found.population.characters.len();
    assert!(
        changes < people * 30,
        "{changes} entity changes for {people} people in 30 s"
    );
}

#[test]
fn talking_needs_reach_varies_and_builds_goodwill() {
    let (mut world, found) = village_world();
    let npc = meet_someone(&mut world, &found);
    // Too far away: nothing.
    let (x, _) = world.npc_position(npc).expect("npc");
    park_at(&mut world, PLAYER, &found, TILE_SIZE.mul_add(12.0, x));
    let far = act(
        &mut world,
        PlayerCommand::People(PeopleCommand::Talk {
            npc,
            topic: TalkTopic::Chat,
        }),
    );
    assert_eq!(
        outcome(&far),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
    approach(&mut world, &found, npc);
    let first = act(
        &mut world,
        PlayerCommand::People(PeopleCommand::Talk {
            npc,
            topic: TalkTopic::Chat,
        }),
    );
    assert_eq!(outcome(&first), PlayerScopedCommandOutcome::Applied);
    assert!(first.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::PersonTalked {
            first_meeting: true,
            ..
        }
    )));
    let character = world.people().conversation(PLAYER).expect("open").character;
    // They stop and listen.
    let held = npcs_of(&world, found.id)
        .into_iter()
        .find(|(id, _)| *id == npc)
        .expect("npc")
        .1;
    assert_eq!(held.talking_to, Some(PLAYER));
    let mut lines = Vec::new();
    for day in 2..8 {
        set_time(&mut world, DayTime::at(day, 10, 0));
        run(&mut world, 2);
        for topic in [TalkTopic::Chat, TalkTopic::Life, TalkTopic::Rumours] {
            let Some(id) = world.npc_of(character) else {
                continue;
            };
            approach(&mut world, &found, id);
            let output = act(
                &mut world,
                PlayerCommand::People(PeopleCommand::Talk { npc: id, topic }),
            );
            if outcome(&output) != PlayerScopedCommandOutcome::Applied {
                continue;
            }
            let conversation = world.people().conversation(PLAYER).expect("open");
            // This exchange's lines: everything after the player's own line.
            let start = conversation
                .lines
                .iter()
                .rposition(|line| line.speaker == Speaker::You)
                .map_or(0, |index| index + 1);
            lines.extend(
                conversation.lines[start..]
                    .iter()
                    .filter(|line| line.speaker == Speaker::Them)
                    .map(|line| line.text.clone()),
            );
        }
    }
    let distinct: BTreeSet<&String> = lines.iter().collect();
    assert!(lines.len() >= 12, "{lines:#?}");
    assert!(
        distinct.len() * 10 >= lines.len() * 6,
        "too repetitive: {lines:#?}"
    );
    for line in &lines {
        assert!(!line.contains(['{', '}']), "{line}");
    }
    assert!(world.people().affinity(character, PLAYER) >= 10);
    assert!(matches!(
        world.people().standing(character, PLAYER),
        Standing::Acquainted | Standing::Friendly | Standing::Close
    ));
    // Goodbye frees them.
    let bye = act(&mut world, PlayerCommand::People(PeopleCommand::Goodbye));
    assert_eq!(outcome(&bye), PlayerScopedCommandOutcome::Applied);
    assert!(world.people().conversation(PLAYER).is_none());
}

#[test]
#[allow(clippy::too_many_lines, reason = "one scenario end to end")]
fn trades_follow_standing_and_charge_credits() {
    let (mut world, found) = village_world();
    world.player_mut(PLAYER).expect("rig").credits = 50_000;
    // Somebody who sells a tonic, a patch-up or gossip to strangers.
    let (npc, row, offer) = npcs_of(&world, found.id)
        .into_iter()
        .filter(|(_, state)| state.inside.is_none())
        .find_map(|(id, state)| {
            let character =
                crate::people::routine::character(world.world_settings(), state.character)?;
            offers::offers_of(&character)
                .into_iter()
                .enumerate()
                .find(|(_, offer)| {
                    offer.needs == Standing::Stranger
                        && matches!(offer.deal, PersonDeal::Goods(VillageDeal::Blessing(_)))
                })
                .map(|(row, offer)| (id, row, offer))
        })
        .unwrap_or_else(|| {
            // Fall back to anyone's first offer, after making friends.
            let (id, state) = npcs_of(&world, found.id)
                .into_iter()
                .find(|(_, state)| state.inside.is_none())
                .expect("someone outdoors");
            let character =
                crate::people::routine::character(world.world_settings(), state.character)
                    .expect("character");
            (id, 0, offers::offers_of(&character)[0])
        });
    approach(&mut world, &found, npc);
    let character = npcs_of(&world, found.id)
        .into_iter()
        .find(|(id, _)| *id == npc)
        .expect("npc")
        .1
        .character;
    if !offers::standing_allows(Standing::Stranger, offer.needs) {
        let early = act(
            &mut world,
            PlayerCommand::People(PeopleCommand::Trade {
                npc,
                offer: u8::try_from(row).expect("row"),
            }),
        );
        assert_eq!(
            outcome(&early),
            PlayerScopedCommandOutcome::IgnoredUnavailable
        );
        world
            .people
            .characters
            .entry(character)
            .or_default()
            .remember(
                PLAYER,
                1,
                crate::people::MemoryFact::FoundPerson {
                    name: "Tobin".into(),
                    relation: "brother".into(),
                },
            );
        world
            .people
            .characters
            .entry(character)
            .or_default()
            .hear_about(PLAYER, 40);
    }
    let before = world.player(PLAYER).expect("rig").credits;
    let output = act(
        &mut world,
        PlayerCommand::People(PeopleCommand::Trade {
            npc,
            offer: u8::try_from(row).expect("row"),
        }),
    );
    if outcome(&output) == PlayerScopedCommandOutcome::Applied {
        assert!(
            output
                .events
                .iter()
                .any(|event| matches!(event.kind, GameplayEventKind::PersonTraded { .. }))
        );
        let after = world.player(PLAYER).expect("rig").credits;
        if offer.price > 0 {
            assert!(after < before, "{offer:?}");
        }
        if offer.once {
            let again = act(
                &mut world,
                PlayerCommand::People(PeopleCommand::Trade {
                    npc,
                    offer: u8::try_from(row).expect("row"),
                }),
            );
            assert_eq!(
                outcome(&again),
                PlayerScopedCommandOutcome::IgnoredUnavailable
            );
        }
    } else {
        // Only map deals can come up empty (nothing unknown nearby).
        assert!(
            matches!(offer.deal, PersonDeal::MapSecret | PersonDeal::Rumour),
            "{offer:?}"
        );
    }
}

#[test]
fn gifts_land_by_taste_and_come_out_of_the_bay() {
    let (mut world, found) = village_world();
    let npc = meet_someone(&mut world, &found);
    let state = npcs_of(&world, found.id)
        .into_iter()
        .find(|(id, _)| *id == npc)
        .expect("npc")
        .1;
    let character =
        crate::people::routine::character(world.world_settings(), state.character).expect("who");
    let favourite = ItemKind::Mineral(offers::favourite_mineral(&character));
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(favourite, 2);
    let output = act(
        &mut world,
        PlayerCommand::People(PeopleCommand::Gift {
            npc,
            item: favourite,
        }),
    );
    assert_eq!(outcome(&output), PlayerScopedCommandOutcome::Applied);
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::PersonGifted {
            reaction: GiftReaction::Loved,
            ..
        }
    )));
    assert_eq!(
        world
            .player(PLAYER)
            .expect("rig")
            .cargo_bay
            .count(favourite),
        1
    );
    assert!(world.people().affinity(state.character, PLAYER) >= 8);
    // Nothing left to give: refused.
    let _ = world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .take_all(favourite);
    let empty = act(
        &mut world,
        PlayerCommand::People(PeopleCommand::Gift {
            npc,
            item: favourite,
        }),
    );
    assert_eq!(
        outcome(&empty),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
}

#[test]
fn affinity_persists_through_saves_and_parking() {
    let (mut world, found) = village_world();
    let npc = meet_someone(&mut world, &found);
    let _ = act(
        &mut world,
        PlayerCommand::People(PeopleCommand::Talk {
            npc,
            topic: TalkTopic::Chat,
        }),
    );
    let character = world.people().conversation(PLAYER).expect("open").character;
    let affinity = world.people().affinity(character, PLAYER);
    assert!(affinity > 0);
    // A save round trip keeps every memory.
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.people(), world.people());
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    // Old saves without people load with nobody remembering anything.
    let mut old: serde_json::Value = serde_json::to_value(&world).expect("json");
    old.as_object_mut().expect("object").remove("people");
    let old: WorldState = serde_json::from_value(old).expect("old save");
    assert!(old.people().is_empty());
    // A second player parks and returns under the same id: still remembered.
    let friend = PlayerId::new(2);
    world.add_player(friend);
    world.set_crew_member(
        friend,
        CrewMember {
            profile: None,
            name: "Alex".into(),
            seat: None,
        },
    );
    let (npc_x, _) = world.npc_position(npc).expect("npc");
    park_at(&mut world, friend, &found, npc_x);
    let tick = world.simulation_tick();
    let output = world.simulate_tick(
        tick,
        &TickInputs::default().act(
            friend,
            PlayerCommand::People(PeopleCommand::Talk {
                npc,
                topic: TalkTopic::Chat,
            }),
        ),
    );
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::Applied
    );
    let conversation = world.people().conversation(friend).expect("open");
    assert!(
        conversation
            .lines
            .iter()
            .all(|line| !line.text.contains(crate::world::DEFAULT_PLAYER_NAME)),
        "they call Alex by name"
    );
    let remembered = world.people().affinity(character, friend);
    assert!(remembered > 0);
    assert!(world.park_player(friend));
    assert!(world.unpark_player(friend));
    assert_eq!(world.people().affinity(character, friend), remembered);
}

#[test]
fn people_and_conversations_replicate() {
    let (mut authority, found) = village_world();
    let mut replica = authority.redacted_for(Some(PLAYER));
    let npc = meet_someone(&mut authority, &found);
    let _ = act(
        &mut authority,
        PlayerCommand::People(PeopleCommand::Talk {
            npc,
            topic: TalkTopic::Chat,
        }),
    );
    run(&mut authority, 90);
    let wire = |value: &crate::SectionPayload| -> crate::SectionPayload {
        let bytes = postcard::to_stdvec(value).expect("encode");
        postcard::from_bytes(&bytes).expect("decode")
    };
    let section = authority
        .section(SectionKey::People)
        .expect("people section");
    replica.apply_section(SectionKey::People, wire(&section));
    let held: BTreeSet<EntityId> = replica.entities().iter().map(|entity| entity.id).collect();
    let current = authority.entity_revisions(None, None);
    let updates = current
        .iter()
        .filter_map(|(id, _)| authority.entity_record(*id))
        .collect();
    let despawned = held
        .into_iter()
        .filter(|id| authority.entities().get(*id).is_none())
        .collect();
    let bytes = postcard::to_stdvec(&EntityDelta { updates, despawned }).expect("encode");
    replica.apply_entity_delta(postcard::from_bytes(&bytes).expect("decode"));
    assert_eq!(replica.people(), authority.people());
    assert_eq!(
        replica.people().conversation(PLAYER),
        authority.people().conversation(PLAYER)
    );
    let people = |world: &WorldState| -> Vec<crate::entity::Entity> {
        world.entities().of_kind(EntityKind::Npc).cloned().collect()
    };
    assert_eq!(people(&replica), people(&authority));
    // Interpolation: positions between two ticks lie on the walking leg, a step apart.
    let now = authority.simulation_tick();
    let before = SimulationTick::new(now.get() - 1);
    assert!(people(&authority).iter().all(|entity| match entity.state {
        EntityState::Npc(state) => {
            let (a, b) = (state.x_at(before, 0.0), state.x_at(before, 1.0));
            (b - state.x_at(now, 0.0)).abs() < 0.01 && (a - b).abs() <= TILE_SIZE
        }
        _ => true,
    }));
}

#[test]
fn people_never_take_damage_or_count_as_targets() {
    let (world, found) = village_world();
    for (id, _) in npcs_of(&world, found.id) {
        let entity = world.entities().get(id).expect("npc");
        assert!(entity.state.health().is_none());
        assert!(!entity.kind().feels_blasts());
    }
    assert!(world.enemy_safe_zone(crate::terrain::TilePosition {
        x: found.center_column(),
        y: found.ground_row - 1,
    }));
}

#[test]
fn walking_pace_is_tick_rate_safe() {
    let measure = |hz: u32| {
        let mut world = world_at(hz);
        let found = first_village(&world);
        set_time(&mut world, DayTime::at(1, 2, 30));
        let plaza = i32::midpoint(found.plaza.0, found.plaza.1);
        park_at(&mut world, PLAYER, &found, column_x(plaza));
        run(&mut world, u64::from(hz));
        set_time(&mut world, DayTime::at(1, 10, 30));
        run(&mut world, u64::from(hz) * 20);
        world.people_about()
    };
    let slow = measure(30);
    let fast = measure(60);
    let activities = |about: &[(crate::people::CharacterId, Activity, Option<u8>)]| {
        about
            .iter()
            .map(|(character, activity, _)| (*character, *activity))
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(activities(&slow), activities(&fast));
}
