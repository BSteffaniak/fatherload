//! The destination index (Wave 14 W14-B): every kind resolves on two seeds, the selectors
//! cycle and are deterministic, travel arrives safely everywhere, counts nothing, goes back,
//! and its bookmarks and back-stack are saved and replicated.

use crate::{
    GameplayEventKind, PlayerCommand, PlayerId, PlayerScopedCommandOutcome, SimulationParams,
    TestModeAction, TickInputs, WorldState,
    descent::Descent,
    destination::{
        ArrivalOptions, DestinationFilter, DestinationId, DestinationKind, Selector, TravelEvent,
        TravelRequest,
    },
    place::{BodyId, Place},
    space::Theme,
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);
const HZ: u32 = 30;
const SEEDS: [u64; 2] = [0x00C0_FFEE, 0x5EED_0014];

/// The kinds every Deep Descent world has candidates of without anything recorded.
const GENERATED: [DestinationKind; 32] = [
    DestinationKind::Descent,
    DestinationKind::Stratum,
    DestinationKind::Depth,
    DestinationKind::HomeTown,
    DestinationKind::Village,
    DestinationKind::SkyIsland,
    DestinationKind::Tomb,
    DestinationKind::Cave,
    DestinationKind::Vault,
    DestinationKind::AnomalyRoom,
    DestinationKind::Landmark,
    DestinationKind::Settlement,
    DestinationKind::Outpost,
    DestinationKind::StoryLog,
    DestinationKind::Camp,
    DestinationKind::BossArena,
    DestinationKind::GateSetPiece,
    DestinationKind::Heart,
    DestinationKind::Moon,
    DestinationKind::System,
    DestinationKind::Planet,
    DestinationKind::SmallBody,
    DestinationKind::Station,
    DestinationKind::Belt,
    DestinationKind::Comet,
    DestinationKind::Nebula,
    DestinationKind::BlackHole,
    DestinationKind::Wormhole,
    DestinationKind::ArchiveSystem,
    DestinationKind::ThemeBossLair,
    DestinationKind::AlienTown,
    DestinationKind::BuilderOutpost,
];

/// A Deep Descent test-mode world with space open.
fn test_world(seed: u64) -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(seed)
        },
        SimulationParams::new(HZ),
    );
    world.enable_test_mode();
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    heart.star_drive.insert(PLAYER);
    world.set_heart(heart);
    world
}

fn act_as(
    world: &mut WorldState,
    player: PlayerId,
    action: TestModeAction,
) -> Vec<GameplayEventKind> {
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(player, PlayerCommand::TestMode(action));
    world
        .simulate_tick(tick, &inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn act(world: &mut WorldState, action: TestModeAction) -> Vec<GameplayEventKind> {
    act_as(world, PLAYER, action)
}

fn travel(world: &mut WorldState, request: TravelRequest) -> Vec<GameplayEventKind> {
    act(world, TestModeAction::TravelTo(request))
}

fn arrived(events: &[GameplayEventKind]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            GameplayEventKind::Travel {
                what: TravelEvent::Arrived | TravelEvent::WentBack,
                ..
            }
        )
    })
}

fn run(world: &mut WorldState, ticks: u32) {
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &TickInputs::default());
    }
}

#[test]
fn every_kind_resolves_on_two_seeds_with_a_card() {
    for seed in SEEDS {
        let mut world = test_world(seed);
        world.add_player(OTHER);
        for kind in GENERATED {
            let found = world.destinations(PLAYER, kind, DestinationFilter::Any);
            assert!(!found.is_empty(), "seed {seed:x}: no {kind:?}");
            for destination in &found {
                assert_eq!(destination.id.kind, kind);
                assert!(!destination.name.is_empty(), "{kind:?} has a name");
                assert!(
                    destination.info.distance.is_some(),
                    "{kind:?} has a distance"
                );
            }
            // Ordered outward from the player.
            let distances: Vec<u64> = found
                .iter()
                .map(|destination| destination.info.distance.unwrap_or(0))
                .collect();
            let first_elsewhere = found.iter().position(|destination| {
                destination.id.place.store() != world.place_of(PLAYER).store()
            });
            let here = first_elsewhere.unwrap_or(found.len());
            assert!(
                distances[..here].is_sorted(),
                "seed {seed:x}: {kind:?} nearest first: {distances:?}"
            );
        }
        // Every Descent, and Descent V by filter, names its boss and its Descent.
        let descents = world.destinations(PLAYER, DestinationKind::Descent, DestinationFilter::Any);
        assert_eq!(descents.len(), 8);
        let fifth = world
            .resolve_travel(PLAYER, &TravelRequest::descent(Descent::HollowWorld))
            .expect("Descent V")
            .0;
        assert_eq!(fifth.info.descent, Some(Descent::HollowWorld));
        assert!(fifth.info.depth_m.is_some_and(|depth| depth > 4_000));
        // Every stratum by name, and every planet theme.
        let strata = world.destinations(PLAYER, DestinationKind::Stratum, DestinationFilter::Any);
        assert_eq!(strata.len(), crate::descent::Stratum::ALL.len());
        for theme in Theme::PLANET {
            let planet = world.resolve_travel(
                PLAYER,
                &TravelRequest::nearest(DestinationKind::Planet)
                    .filtered(DestinationFilter::Theme(theme)),
            );
            assert!(
                planet.is_some_and(|(found, _)| found.info.theme == Some(theme)),
                "seed {seed:x}: a {theme:?} planet near home"
            );
        }
        // An exact depth, in metres.
        let depth = world
            .resolve_travel(PLAYER, &TravelRequest::depth_metres(4_200))
            .expect("4200 m")
            .0;
        assert_eq!(depth.info.depth_m, Some(4_200));
        // The other player.
        assert_eq!(
            world
                .destinations(PLAYER, DestinationKind::Player, DestinationFilter::Any)
                .len(),
            1
        );
        // A search by name.
        let found = world.search_destinations(PLAYER, "descent 5", 4);
        assert!(
            found
                .iter()
                .any(|destination| destination.info.descent == Some(Descent::HollowWorld)),
            "search finds Descent V"
        );
        let leviathan = world.search_destinations(PLAYER, "boss leviathan", 4);
        assert_eq!(leviathan.len(), 1, "one Leviathan");
    }
}

#[test]
fn selectors_cycle_outward_and_are_deterministic() {
    let mut one = test_world(SEEDS[0]);
    let mut two = test_world(SEEDS[0]);
    for kind in [
        DestinationKind::Planet,
        DestinationKind::Village,
        DestinationKind::Stratum,
    ] {
        let list = one.destinations(PLAYER, kind, DestinationFilter::Any);
        assert_eq!(list, two.destinations(PLAYER, kind, DestinationFilter::Any));
        // Random: the same seed picks the same candidate on both worlds.
        for seed in 0..6 {
            let request = TravelRequest::new(kind, Selector::Random(seed));
            assert_eq!(
                one.resolve_travel(PLAYER, &request).map(|found| found.0.id),
                two.resolve_travel(PLAYER, &request).map(|found| found.0.id),
            );
        }
        let picks: std::collections::BTreeSet<DestinationId> = (0..12)
            .filter_map(|seed| {
                one.resolve_travel(PLAYER, &TravelRequest::new(kind, Selector::Random(seed)))
                    .map(|found| found.0.id)
            })
            .collect();
        assert!(picks.len() > 1, "{kind:?}: random varies with its seed");
    }
    // Next walks outward through the list made where the cycle began; previous walks back.
    let start = one.travel_here(PLAYER).expect("here");
    let list = one.destinations(PLAYER, DestinationKind::Village, DestinationFilter::Any);
    let next = TravelRequest::new(DestinationKind::Village, Selector::Next);
    let mut visited = Vec::new();
    for _ in 0..3 {
        assert!(arrived(&travel(&mut one, next)));
        assert!(arrived(&travel(&mut two, next)));
        let cursor = one.player_travel(PLAYER).cursor.expect("a cycle");
        assert_eq!(cursor.origin.x, start.x);
        visited.push(list[usize::from(cursor.index)].id);
        assert_eq!(one.travel_here(PLAYER), two.travel_here(PLAYER));
    }
    assert_eq!(visited.len(), 3);
    assert!(visited.windows(2).all(|pair| pair[0] != pair[1]));
    let previous = TravelRequest::new(DestinationKind::Village, Selector::Previous);
    assert!(arrived(&travel(&mut one, previous)));
    let cursor = one.player_travel(PLAYER).cursor.expect("a cycle");
    assert_eq!(list[usize::from(cursor.index)].id, visited[1]);
    // The next planet, twice: two different worlds.
    let planet = TravelRequest::new(DestinationKind::Planet, Selector::Next);
    assert!(arrived(&travel(&mut one, planet)));
    let first = one.place_of(PLAYER);
    assert!(arrived(&travel(&mut one, planet)));
    let second = one.place_of(PLAYER);
    assert!(matches!(
        first,
        Place::OnBody(BodyId::Space(_)) | Place::Space
    ));
    assert_ne!(first, second);
    assert_eq!(one.simulation_state_hash(), {
        let _ = travel(&mut two, previous);
        let _ = travel(&mut two, planet);
        let _ = travel(&mut two, planet);
        two.simulation_state_hash()
    });
}

/// Travels to the nearest of `kind`, then runs 10 s with the hull able to break: the rig must
/// keep its hull and stay where it arrived.
fn survives(world: &mut WorldState, request: TravelRequest) {
    let events = travel(world, request);
    assert!(arrived(&events), "{request:?}: arrived ({events:?})");
    let place = world.place_of(PLAYER);
    let start = world.travel_here(PLAYER).expect("here");
    for tick in 0..10 * HZ {
        run(world, 1);
        let rig = world.player(PLAYER).expect("rig");
        assert!(
            rig.hull > 0.0,
            "{request:?} at {place:?}: hull gone after {tick} ticks"
        );
    }
    assert_eq!(world.place_of(PLAYER), place, "{request:?}: still there");
    if place != Place::Space {
        let end = world.travel_here(PLAYER).expect("here");
        let moved = crate::destination::isqrt(crate::destination::distance2(
            (start.x, start.y),
            (end.x, end.y),
        ));
        assert!(moved < 40, "{request:?}: drifted {moved} tiles");
    }
}

#[test]
fn travel_arrives_safely_at_every_kind_with_the_hull_able_to_break() {
    let mut world = test_world(SEEDS[0]);
    world.add_player(OTHER);
    let _ = act(&mut world, TestModeAction::SetInvulnerable(false));
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hull = rig.max_hull();
    for kind in GENERATED {
        survives(&mut world, TravelRequest::nearest(kind));
    }
    // The other player, a bookmark and the back-stack.
    survives(&mut world, TravelRequest::nearest(DestinationKind::Player));
    let _ = act(&mut world, TestModeAction::BookmarkHere);
    survives(
        &mut world,
        TravelRequest::nearest(DestinationKind::Bookmark),
    );
    survives(&mut world, TravelRequest::nearest(DestinationKind::Recent));
    // In the water and in the arena.
    survives(
        &mut world,
        TravelRequest::nearest(DestinationKind::BossArena).arriving(ArrivalOptions {
            in_water: false,
            in_arena: true,
        }),
    );
}

#[test]
fn travel_counts_no_records_and_goes_back() {
    let mut world = test_world(SEEDS[1]);
    let start = world.travel_here(PLAYER).expect("here");
    let records = world.player(PLAYER).expect("rig").milestones.clone();
    let progression = serde_json::to_value(world.progression()).expect("progression");
    assert!(arrived(&travel(
        &mut world,
        TravelRequest::descent(Descent::Heart)
    )));
    let deep = world.travel_here(PLAYER).expect("deep");
    assert!(deep.y > start.y + 10_000, "down in the Heart's Descent");
    run(&mut world, HZ);
    assert_eq!(world.player(PLAYER).expect("rig").milestones, records);
    assert_eq!(
        serde_json::to_value(world.progression()).expect("progression"),
        progression
    );
    // Off to a planet, then back twice: the Heart's Descent, then home.
    assert!(arrived(&travel(
        &mut world,
        TravelRequest::nearest(DestinationKind::Planet)
    )));
    assert!(matches!(
        world.place_of(PLAYER),
        Place::OnBody(BodyId::Space(_))
    ));
    assert!(arrived(&act(&mut world, TestModeAction::TravelBack)));
    let back = world.travel_here(PLAYER).expect("back");
    assert_eq!(back.place, Place::HOME);
    assert!((back.y - deep.y).abs() <= 2 && (back.x - deep.x).abs() <= 48);
    assert!(arrived(&act(&mut world, TestModeAction::TravelBack)));
    let home = world.travel_here(PLAYER).expect("home");
    assert!((home.y - start.y).abs() <= 2 && (home.x - start.x).abs() <= 48);
    // Nothing left to go back to.
    let events = act(&mut world, TestModeAction::TravelBack);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::Travel {
            what: TravelEvent::NotFound,
            ..
        }
    )));
}

#[test]
fn travel_is_test_mode_only() {
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(SEEDS[0])
        },
        SimulationParams::new(HZ),
    );
    let before = world.travel_here(PLAYER);
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(
        PLAYER,
        PlayerCommand::TestMode(TestModeAction::TravelTo(TravelRequest::descent(
            Descent::Heart,
        ))),
    );
    let output = world.simulate_tick(tick, &inputs);
    assert!(
        output
            .action_outcomes
            .iter()
            .all(|outcome| outcome.outcome == PlayerScopedCommandOutcome::IgnoredUnavailable)
    );
    assert!(!arrived(
        &output
            .events
            .iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>()
    ));
    assert!(world.travel_log().is_empty());
    let after = world.travel_here(PLAYER);
    assert_eq!(before.map(|id| id.y), after.map(|id| id.y));
    assert!(
        world
            .stage_travel(PLAYER, &TravelRequest::descent(Descent::Heart))
            .is_none()
    );
}

#[test]
fn bookmarks_and_the_back_stack_are_saved_and_replicated() {
    let mut world = test_world(SEEDS[0]);
    world.add_player(OTHER);
    let _ = act(&mut world, TestModeAction::BookmarkHere);
    assert!(arrived(&travel(
        &mut world,
        TravelRequest::nearest(DestinationKind::Outpost)
    )));
    let _ = act_as(&mut world, OTHER, TestModeAction::BookmarkHere);
    let log = world.travel_log().clone();
    assert_eq!(log.of(PLAYER).bookmarks.len(), 1);
    assert_eq!(log.of(PLAYER).recent.len(), 1);
    assert_eq!(log.of(OTHER).bookmarks.len(), 1);
    // Saved: the world round-trips through its save encoding.
    let saved = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&saved).expect("load");
    assert_eq!(loaded.travel_log(), &log);
    // An older save without the field loads with an empty log.
    let mut value: serde_json::Value = serde_json::from_str(&saved).expect("json");
    value.as_object_mut().expect("object").remove("travel");
    let old: WorldState = serde_json::from_value(value).expect("old save");
    assert!(old.travel_log().is_empty());
    // Replicated: the shared section carries it to a replica.
    let key = crate::world::SectionKey::Travel;
    assert!(crate::world::SectionKey::SHARED.contains(&key));
    let payload = world.section(key).expect("section");
    let mut replica = test_world(SEEDS[0]);
    replica.apply_section(key, payload);
    assert_eq!(replica.travel_log(), &log);
    // A leaving player's records go with them.
    world.remove_player(OTHER);
    assert!(world.travel_log().of(OTHER).bookmarks.is_empty());
    // Removing a bookmark.
    let _ = act(&mut world, TestModeAction::RemoveBookmark(0));
    assert!(world.player_travel(PLAYER).bookmarks.is_empty());
}

#[test]
fn random_with_one_seed_picks_afresh_after_each_travel() {
    let mut world = test_world(SEEDS[1]);
    let request = TravelRequest::new(DestinationKind::Stratum, Selector::Random(0));
    let mut picks = std::collections::BTreeSet::new();
    for _ in 0..6 {
        let preview = world.resolve_travel(PLAYER, &request).expect("a stratum").0;
        assert!(arrived(&travel(&mut world, request)));
        // The travel went where its preview said (the stratum it stands in).
        let here = world.travel_here(PLAYER).expect("here");
        assert!(
            (here.y - preview.id.y).abs() <= 24,
            "previewed {preview:?}, arrived {here:?}"
        );
        picks.insert(preview.id);
    }
    assert!(picks.len() > 2, "a fixed seed still wanders: {picks:?}");
}
