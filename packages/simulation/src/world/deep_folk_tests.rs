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
        park(&mut world, PLAYER, DeepSettlement::ALL[1 - place as usize]);
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
