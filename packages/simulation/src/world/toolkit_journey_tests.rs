//! The god-mode toolkit end to end (Wave 14 integration, expansion plan §19): a bot stocks its
//! rig and checks every registered item is there, then travels through the destination index
//! to each Descent, every boss arena (the gate bosses', the Heart, all twelve theme bosses'
//! lairs and the three faction flagships), each settlement kind, the nearest, next and a
//! random planet, a station, and back down the back-stack, through `simulate_tick` exactly as
//! the Atlas, the palette and `--start-at` send it. None of it reaches a normal world.

use crate::{
    GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TestModeAction, TickInputs,
    WorldState,
    descent::Descent,
    destination::{
        Destination, DestinationFilter, DestinationKind, Selector, TravelEvent, TravelRequest,
    },
    endgame::hunt::THEME_BOSSES,
    hostiles::{Faction, encounter::LAIR_REACH_TILES},
    place::{BodyId, Place},
    space::SpaceTile,
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x00C0_FFEE;

/// A fresh test-mode Deep Descent world: nothing granted by hand, the toolkit stocks it.
fn test_world() -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(SEED)
        },
        SimulationParams::new(30),
    );
    world.enable_test_mode();
    world
}

fn act(world: &mut WorldState, action: TestModeAction) -> Vec<GameplayEventKind> {
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(PLAYER, PlayerCommand::TestMode(action));
    world
        .simulate_tick(tick, &inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
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

/// Travels with `request` and checks the rig stands where the preview (`resolve_travel`) said:
/// in its place, and on the home world within a pocket's reach of its tile.
fn go(world: &mut WorldState, request: TravelRequest, what: &str) -> Destination {
    let (preview, _) = world
        .resolve_travel(PLAYER, &request)
        .unwrap_or_else(|| panic!("{what}: the index answers"));
    let events = act(world, TestModeAction::TravelTo(request));
    assert!(arrived(&events), "{what}: arrived ({events:?})");
    let place = world.place_of(PLAYER);
    match preview.id.place {
        Place::OnBody(BodyId::Home | BodyId::Moon) => {
            assert!(place.is_home_store(), "{what}: at home, not {place:?}");
            let rig = world.player(PLAYER).expect("rig");
            let at = rig.tile_in(world.frame());
            let (x, y) = (
                preview.inside.map_or(preview.id.x, |inside| inside.0),
                preview.id.y,
            );
            assert!(
                (i64::from(at.x) - x).abs() <= 200 && (i64::from(at.y) - y).abs() <= 32,
                "{what}: arrived at {at:?} for ({x}, {y})"
            );
        }
        Place::OnBody(body) => {
            assert_eq!(place, Place::OnBody(body), "{what}");
        }
        Place::Space => {
            assert_eq!(place, Place::Space, "{what}");
            let rig = world.player(PLAYER).expect("rig");
            let target = SpaceTile::new(preview.id.x, preview.id.y);
            assert!(
                SpaceTile::of_pos(rig.flight.pos).distance(target) < 4.0,
                "{what}: stopped at the spot"
            );
        }
    }
    preview
}

fn candidates(world: &WorldState, kind: DestinationKind) -> Vec<Destination> {
    world.destinations(PLAYER, kind, DestinationFilter::Any)
}

#[test]
#[allow(clippy::too_many_lines, reason = "the whole journey, stop by stop")]
fn a_stocked_rig_travels_to_every_kind_of_place_and_back() {
    let mut world = test_world();
    // 1. Stocked: every registered consumable, gear line, gate reward and space line.
    let _ = act(&mut world, TestModeAction::StockAll);
    let gaps = world.test_stock_gaps(PLAYER);
    assert!(
        gaps.is_empty(),
        "every registered item is present: {gaps:?}"
    );

    // 2. Each Descent, at its top.
    let layout = world.descent_strata_layout().expect("a Deep Descent world");
    for descent in Descent::ALL {
        let _ = go(&mut world, TravelRequest::descent(descent), descent.name());
        let row = world.player(PLAYER).expect("rig").tile_in(world.frame()).y;
        let (top, bottom) = layout.descent_rows(descent);
        // (The Crust's top is the ground itself: its arrival stands on the surface.)
        assert!(
            (top - 12..=bottom).contains(&row),
            "{}: row {row} in {top}..={bottom}",
            descent.name()
        );
    }

    // 3. Every boss arena: the gate bosses' (and the mini-bosses'), at their doors and inside
    // one; the Heart's chamber.
    let arenas = candidates(&world, DestinationKind::BossArena);
    assert!(arenas.len() >= 7, "the gate arenas: {}", arenas.len());
    for arena in &arenas {
        let _ = go(&mut world, TravelRequest::exact(arena.id), &arena.name);
    }
    let _ = go(
        &mut world,
        TravelRequest::nearest(DestinationKind::Heart),
        "the Heart",
    );

    // ... all twelve theme bosses' lairs, each on its own world.
    let lairs = candidates(&world, DestinationKind::ThemeBossLair);
    for boss in THEME_BOSSES {
        let lair = lairs
            .iter()
            .find(|lair| lair.info.boss.as_deref() == Some(boss.name))
            .unwrap_or_else(|| panic!("{} has a lair to travel to", boss.name));
        let found = go(&mut world, TravelRequest::exact(lair.id), boss.name);
        assert!(matches!(found.id.place, Place::OnBody(BodyId::Space(_))));
    }
    assert_eq!(
        THEME_BOSSES.len(),
        12,
        "all twelve planet themes have a boss"
    );

    // ... and the three faction flagships, inside their lairs' reach.
    let flagships = candidates(&world, DestinationKind::FactionBossLair);
    assert_eq!(flagships.len(), Faction::ALL.len());
    for faction in Faction::ALL {
        let flagship = flagships
            .iter()
            .find(|found| found.info.residents.iter().any(|who| who == faction.name()))
            .expect("a flagship");
        let _ = go(
            &mut world,
            TravelRequest::exact(flagship.id),
            &flagship.name,
        );
        let lair = world.boss_lair(faction).expect("its lair");
        let rig = world.player(PLAYER).expect("rig");
        let tiles = rig.flight.pos.distance(lair.pos) / crate::TILE_SIZE;
        assert!(tiles < LAIR_REACH_TILES, "{}: {tiles}", faction.name());
    }

    // 4. Each settlement kind.
    for kind in [
        DestinationKind::HomeTown,
        DestinationKind::Village,
        DestinationKind::SkyIsland,
        DestinationKind::Settlement,
        DestinationKind::Outpost,
        DestinationKind::Camp,
        DestinationKind::AlienTown,
        DestinationKind::BuilderOutpost,
    ] {
        let _ = go(&mut world, TravelRequest::nearest(kind), kind.name());
    }

    // 5. The nearest, the next and a random planet; a station.
    let nearest = go(
        &mut world,
        TravelRequest::nearest(DestinationKind::Planet),
        "the nearest planet",
    );
    let next = go(
        &mut world,
        TravelRequest::new(DestinationKind::Planet, Selector::Next),
        "the next planet",
    );
    let random = go(
        &mut world,
        TravelRequest::new(DestinationKind::Planet, Selector::Random(0x5EED)),
        "a random planet",
    );
    for found in [&nearest, &next, &random] {
        assert!(matches!(found.id.place, Place::OnBody(BodyId::Space(_))));
    }
    let before_station = world.travel_here(PLAYER).expect("here");
    let _ = go(
        &mut world,
        TravelRequest::nearest(DestinationKind::Station),
        "a station",
    );

    // 6. And back: the back-stack returns to the random planet.
    let events = act(&mut world, TestModeAction::TravelBack);
    assert!(arrived(&events), "back");
    assert_eq!(world.place_of(PLAYER), before_station.place);
    // Still stocked after the whole journey (infinite supplies keep it so).
    assert_eq!(world.test_stock_gaps(PLAYER), [] as [String; 0]);
}

#[test]
fn none_of_the_toolkit_reaches_a_normal_world() {
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(SEED)
        },
        SimulationParams::new(30),
    );
    let start = world.player(PLAYER).expect("rig").tile_in(world.frame());
    let place = world.place_of(PLAYER);
    for action in [
        TestModeAction::StockAll,
        TestModeAction::Restock,
        TestModeAction::SetEndingDone(true),
        TestModeAction::SetGateCleared {
            descent: 3,
            cleared: true,
        },
        TestModeAction::TravelTo(TravelRequest::descent(Descent::HollowWorld)),
        TestModeAction::TravelTo(TravelRequest::nearest(DestinationKind::Planet)),
        TestModeAction::TravelBack,
        TestModeAction::BookmarkHere,
        TestModeAction::SpawnCreature(crate::EnemyKind::ALL[0]),
    ] {
        let events = act(&mut world, action);
        assert!(
            !events.iter().any(|event| matches!(
                event,
                GameplayEventKind::Travel { .. } | GameplayEventKind::TestStock { .. }
            )),
            "{action:?} did something in a normal world: {events:?}"
        );
    }
    assert_eq!(world.place_of(PLAYER), place);
    let now = world.player(PLAYER).expect("rig").tile_in(world.frame());
    assert!(
        (now.y - start.y).abs() < 4,
        "the rig stayed: {start:?} -> {now:?}"
    );
    assert_eq!(world.player_travel(PLAYER).bookmarks, []);
    assert!(
        !world.heart().space_unlocked,
        "the ending was not marked done"
    );
}
