//! Wave 9 story: Cathedral Station and the Amber Court have resident people, who come out when
//! a rig arrives and tell their own memories of the Fathers in conversation.

use crate::{
    PlayerCommand, PlayerId, SimulationParams, TickInputs, WorldState,
    enemy::EnemyDifficulty,
    entity::{EntityId, EntityKind, EntityState, NpcState},
    expedition::{ExpeditionState, memories},
    people::{
        PeopleCommand, TalkTopic,
        deep_folk::{self, AMBER_COURT, CATHEDRAL_STATION, DeepSettlement},
        ledger::Speaker,
        routine,
    },
    position::WorldPos,
    summary::PlayerScopedCommandOutcome,
    village::VillageId,
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);
const PARTNER: PlayerId = PlayerId::new(1);

fn world(seed: u64) -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    };
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    // Below Descent I the crew holds the Deep Claim, and the rig every earlier gate's reward.
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.radiator_level = 15;
    rig.hull_level = 15;
    rig.hull = rig.max_hull();
    world
}

fn run(world: &mut WorldState, ticks: u32) {
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &TickInputs::default());
    }
}

/// Parks `player` in `place`'s hall, standing on its floor.
fn park(world: &mut WorldState, player: PlayerId, place: DeepSettlement) -> (i32, i32) {
    let settings = *world.terrain().settings();
    let (ground, first, last) = deep_folk::hall(&settings, place).expect("a hall");
    let column = i32::midpoint(first, last);
    world.teleport_rig_to(
        player,
        WorldPos::tile_center(i64::from(column), i64::from(ground - 1)),
    );
    (column, ground)
}

fn folk_of(world: &WorldState, id: VillageId) -> Vec<(EntityId, NpcState)> {
    world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter_map(|entity| match entity.state {
            EntityState::Npc(state) if state.character.village == id => Some((entity.id, state)),
            _ => None,
        })
        .collect()
}

/// Walks the rig up to `npc` and chats: the lines they said.
fn chat(world: &mut WorldState, npc: EntityId) -> Vec<String> {
    let (x, y) = world.npc_position(npc).expect("position");
    let frame = world.frame();
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(
            i64::from(frame.col(x)),
            i64::from(frame.row(y - crate::terrain::TILE_SIZE)),
        ),
    );
    let tick = world.simulation_tick();
    let output = world.simulate_tick(
        tick,
        &TickInputs::default().act(
            PLAYER,
            PlayerCommand::People(PeopleCommand::Talk {
                npc,
                topic: TalkTopic::Chat,
            }),
        ),
    );
    assert!(
        output
            .action_outcomes
            .first()
            .is_some_and(|outcome| outcome.outcome == PlayerScopedCommandOutcome::Applied),
        "{:?}",
        output.action_outcomes
    );
    world
        .people()
        .conversation(PLAYER)
        .expect("a conversation")
        .lines
        .iter()
        .filter(|line| line.speaker == Speaker::Them)
        .map(|line| line.text.clone())
        .collect()
}

#[test]
fn cathedral_station_and_the_amber_court_have_residents_who_remember_the_fathers() {
    let mut world = world(0x5EED_0009);
    // The crew has read the trail to the Hive's queen's carving, and on into Descent IV.
    world.set_expedition(ExpeditionState {
        found: 33,
        ..ExpeditionState::default()
    });
    for place in DeepSettlement::ALL {
        park(&mut world, PLAYER, place);
        run(&mut world, 40);
        let folk = folk_of(&world, place.id());
        assert_eq!(
            folk.len(),
            place.cast().len(),
            "{}'s people come out",
            place.name()
        );
        let settings = *world.terrain().settings();
        // Each resident tells a memory of their own, word for word, in conversation.
        for (npc, state) in folk {
            let character = routine::character(&settings, state.character).expect("who");
            let (remembered, speaker) =
                deep_folk::resident_speaker(&character).expect("a resident");
            let said = chat(&mut world, npc);
            let own: Vec<&str> = memories::lines_by(remembered, speaker, world.expedition())
                .map(|m| m.line)
                .collect();
            assert!(!own.is_empty(), "{speaker} has an open line");
            assert!(
                said.iter().any(|line| own.contains(&line.as_str())),
                "{} ({speaker}) said {said:?}",
                character.name.full()
            );
            assert!(
                said.iter()
                    .any(|line| Some(line.as_str()) == deep_folk::greeting(&character)),
                "{said:?}"
            );
            // Hive folk keep to their own words: no village small talk.
            if deep_folk::is_hive_folk(place.id()) {
                assert_eq!(said.len(), 2, "{said:?}");
            }
            // Out of the conversation before the next.
            let tick = world.simulation_tick();
            let _ = world.simulate_tick(
                tick,
                &TickInputs::default().act(PLAYER, PlayerCommand::People(PeopleCommand::Goodbye)),
            );
        }
        // Away from the hall, its people go home to the deep's quiet.
        park(
            &mut world,
            PLAYER,
            DeepSettlement::ALL[(place as usize + 1) % DeepSettlement::ALL.len()],
        );
        run(&mut world, 60);
        assert!(
            folk_of(&world, place.id()).is_empty(),
            "{} empties",
            place.name()
        );
    }
}

#[test]
fn the_amber_court_stays_shut_while_it_is_hostile() {
    let mut world = world(0x5EED_0010);
    world.living_dark.hive.standing = -100;
    park(&mut world, PLAYER, DeepSettlement::AmberCourt);
    run(&mut world, 40);
    assert!(folk_of(&world, AMBER_COURT).is_empty());
    // Cathedral Station is not the Hive's to close.
    park(&mut world, PLAYER, DeepSettlement::CathedralStation);
    run(&mut world, 40);
    assert!(!folk_of(&world, CATHEDRAL_STATION).is_empty());
}

#[test]
fn co_op_partners_meet_the_same_residents() {
    let mut host = world(0x5EED_0011);
    host.add_player(PARTNER);
    park(&mut host, PARTNER, DeepSettlement::CathedralStation);
    run(&mut host, 40);
    // The partner's arrival brings the station's people out for everyone, the same people.
    let folk = folk_of(&host, CATHEDRAL_STATION);
    assert_eq!(folk.len(), DeepSettlement::CathedralStation.cast().len());
    let json = serde_json::to_string(&host).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(folk_of(&loaded, CATHEDRAL_STATION), folk);
}

/// Wave 11 carryover: the Archive Post's and Heart's Edge's residents talk about their own
/// place (their own chatter, no generic village small talk) beside their memories.
#[test]
fn the_archive_post_and_hearts_edge_talk_about_their_own_place() {
    let mut world = world(0x5EED_0012);
    world.set_expedition(ExpeditionState {
        found: 80,
        ..ExpeditionState::default()
    });
    let settings = *world.terrain().settings();
    for place in [DeepSettlement::ArchivePost, DeepSettlement::HeartsEdge] {
        park(&mut world, PLAYER, place);
        run(&mut world, 40);
        let folk = folk_of(&world, place.id());
        assert_eq!(folk.len(), place.cast().len(), "{}", place.name());
        for (npc, state) in folk {
            let character = routine::character(&settings, state.character).expect("who");
            let cast = place.cast()[usize::from(character.id.index)];
            assert!(
                !cast.chatter.is_empty(),
                "{} has its own talk",
                cast.speaker
            );
            let said = chat(&mut world, npc);
            // Greeting, their own talk, then a memory of the Fathers: nothing generic.
            assert_eq!(said.len(), 3, "{}: {said:?}", cast.speaker);
            assert_eq!(said[0], cast.greeting);
            assert!(cast.chatter.contains(&said[1].as_str()), "{said:?}");
            let (remembered, speaker) =
                deep_folk::resident_speaker(&character).expect("a resident");
            assert!(
                memories::lines_by(remembered, speaker, world.expedition())
                    .any(|memory| memory.line == said[2]),
                "{said:?}"
            );
            let tick = world.simulation_tick();
            let _ = world.simulate_tick(
                tick,
                &TickInputs::default().act(PLAYER, PlayerCommand::People(PeopleCommand::Goodbye)),
            );
        }
    }
}

/// Wave 11 carryover: Heart's Edge, Descent VIII's outpost, is a settlement with residents who
/// remember the Fathers, and its people offer its own stories.
#[test]
fn hearts_edge_has_residents_who_remember_the_fathers_and_offer_its_stories() {
    let mut world = world(0x5EED_0013);
    let place = DeepSettlement::HeartsEdge;
    assert_eq!(
        memories::Settlement::of_place_name(place.name()),
        Some(memories::Settlement::HeartCamp),
        "registered by name"
    );
    // The hall is Descent VIII's logistics outpost.
    let settings = *world.terrain().settings();
    let (ground, _, _) = deep_folk::hall(&settings, place).expect("a hall");
    assert_eq!(
        usize::from(crate::depth_progression::descent_index_at_row(
            &settings, ground
        )),
        crate::descent::Descent::Heart.index()
    );
    park(&mut world, PLAYER, place);
    run(&mut world, 40);
    let folk = folk_of(&world, place.id());
    assert_eq!(folk.len(), 5, "Heart's Edge's people come out");
    let offers = folk
        .iter()
        .filter_map(|(npc, _)| match world.arc_talk(PLAYER, *npc) {
            super::ArcTalk::HasWork(arc) => arc.template(),
            _ => None,
        })
        .filter(|template| {
            template
                .triggers
                .contains(&crate::people::story::Trigger::Settlement(
                    memories::Settlement::HeartCamp,
                ))
        })
        .count();
    assert!(offers > 0, "its own stories are offered there");
    // Every resident has something of the Fathers to say from the first visit.
    for (npc, _) in folk {
        let said = chat(&mut world, npc);
        assert!(
            said.iter()
                .any(|line| memories::MEMORIES.iter().any(|memory| {
                    memory.settlement == memories::Settlement::HeartCamp && memory.line == *line
                })),
            "{said:?}"
        );
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(
            tick,
            &TickInputs::default().act(PLAYER, PlayerCommand::People(PeopleCommand::Goodbye)),
        );
    }
}
