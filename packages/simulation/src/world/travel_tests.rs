//! The Wave 14 console's travel and stocking actions (`world/travel.rs`): test-mode only,
//! deterministic, landing where the resolver said.

use crate::{
    ControlState, PlayerCommand, PlayerId, PlayerScopedCommandOutcome, SimulationParams,
    TickInputs, TickOutput, WorldState,
    descent::{Descent, DescentBoss, Stratum},
    enemy::EnemyKind,
    place::{BodyId, Place},
    space::Theme,
    terrain::TILE_SIZE,
    test_mode::{
        TestModeAction,
        palette_stock::StockRequest,
        travel::{ArrivalTarget, Destination, Pick},
    },
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(1);
const SEED: u64 = 0x00C0_FFEE;

fn deep_world(test_mode: bool) -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(SEED)
        },
        SimulationParams::new(30),
    );
    if test_mode {
        world.enable_test_mode();
    }
    world
}

fn act(world: &mut WorldState, action: TestModeAction) -> TickOutput {
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(PLAYER, PlayerCommand::TestMode(action));
    world.simulate_tick(tick, &inputs)
}

fn idle(world: &mut WorldState, ticks: u32) {
    let inputs = TickInputs::held(PLAYER, ControlState::default());
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
    }
}

fn row_of(world: &WorldState) -> i32 {
    world.player(PLAYER).expect("rig").tile_in(world.frame()).y
}

#[test]
fn travel_and_stock_are_refused_in_a_normal_world() {
    let mut world = deep_world(false);
    let before = world.player(PLAYER).expect("rig").clone();
    for action in [
        TestModeAction::PaletteTravel(Destination::Descent(Descent::LivingDark)),
        TestModeAction::PaletteTravel(Destination::Depth(500)),
        TestModeAction::PaletteStock(StockRequest::GiveAll),
        TestModeAction::PaletteStock(StockRequest::Spawn(EnemyKind::ALL[0])),
    ] {
        let output = act(&mut world, action);
        assert_eq!(
            output.action_outcomes[0].outcome,
            PlayerScopedCommandOutcome::IgnoredUnavailable,
            "{action:?}"
        );
    }
    let after = world.player(PLAYER).expect("rig");
    assert_eq!(after.credits, before.credits);
    assert!((after.y - before.y).abs() < TILE_SIZE * 2.0, "never moved");
}

#[test]
fn every_descent_and_stratum_resolves_and_lands_in_its_rows() {
    let mut world = deep_world(true);
    let layout = world.travel_layout().expect("a Deep Descent world");
    for descent in Descent::ALL {
        let arrival = world
            .resolve_destination(PLAYER, Destination::Descent(descent))
            .expect("resolves");
        let ArrivalTarget::HomeRow { row } = arrival.target else {
            panic!("a home row");
        };
        let (top, bottom) = layout.descent_rows(descent);
        assert!((top..=bottom).contains(&row), "{descent:?}: {row}");
    }
    for stratum in Stratum::ALL {
        assert!(
            world
                .resolve_destination(PLAYER, Destination::Stratum(stratum))
                .is_some()
        );
    }
    for boss in DescentBoss::ALL {
        assert!(
            world
                .resolve_destination(PLAYER, Destination::Boss(boss))
                .is_some(),
            "{boss:?}"
        );
    }
    // Land in Descent V and stand there safely.
    let output = act(
        &mut world,
        TestModeAction::PaletteTravel(Destination::Descent(Descent::HollowWorld)),
    );
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::Applied
    );
    idle(&mut world, 30);
    let (top, bottom) = layout.descent_rows(Descent::HollowWorld);
    let row = row_of(&world);
    assert!((top..=bottom).contains(&row), "landed at row {row}");
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.hull >= rig.max_hull() * 0.99, "safe arrival");
}

#[test]
fn depth_travel_lands_that_many_metres_down() {
    let mut world = deep_world(true);
    let arrival = world
        .resolve_destination(PLAYER, Destination::Depth(4_200))
        .expect("resolves");
    let ArrivalTarget::HomeRow { row: wanted } = arrival.target else {
        panic!("a home row");
    };
    let _ = act(
        &mut world,
        TestModeAction::PaletteTravel(Destination::Depth(4_200)),
    );
    idle(&mut world, 10);
    let row = row_of(&world);
    assert!(
        (row - wanted).abs() <= 3,
        "row {row}, wanted about {wanted}"
    );
    // A depth past the world's floor lands as deep as travel goes, not nowhere.
    let output = act(
        &mut world,
        TestModeAction::PaletteTravel(Destination::Depth(90_000)),
    );
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::Applied
    );
    idle(&mut world, 10);
    let layout = world.travel_layout().expect("a Deep Descent world");
    let (top, bottom) = layout.descent_rows(Descent::ALL[Descent::ALL.len() - 1]);
    let row = row_of(&world);
    assert!((top..=bottom).contains(&row), "the deepest Descent: {row}");
}

#[test]
fn planets_resolve_by_theme_and_cycle_outward() {
    let world = deep_world(true);
    let nearest = world
        .resolve_destination(
            PLAYER,
            Destination::Planet {
                theme: Some(Theme::Ice),
                pick: Pick::NEAREST,
            },
        )
        .expect("an ice planet near home");
    let next = world
        .resolve_destination(
            PLAYER,
            Destination::Planet {
                theme: Some(Theme::Ice),
                pick: Pick::Nth(1),
            },
        )
        .expect("a second ice planet");
    assert_ne!(nearest.target, next.target);
    let previous = world
        .resolve_destination(
            PLAYER,
            Destination::Planet {
                theme: Some(Theme::Ice),
                pick: Pick::Nth(-1),
            },
        )
        .expect("wraps to the farthest");
    assert_ne!(previous.target, nearest.target);
    // Random picks are the caller's seed: the same seed, the same planet on every peer.
    let random = |seed| {
        world
            .resolve_destination(
                PLAYER,
                Destination::Planet {
                    theme: None,
                    pick: Pick::Random(seed),
                },
            )
            .expect("any planet")
    };
    assert_eq!(random(7), random(7));
    assert!(nearest.name.contains("ice"));
}

#[test]
fn travelling_to_a_planet_lands_there_and_a_spot_brings_the_rig_back() {
    let mut world = deep_world(true);
    let _ = act(
        &mut world,
        TestModeAction::PaletteStock(StockRequest::EndingDone),
    );
    let (home_place, home_pos) = world.rig_world_pos(PLAYER).expect("rig");
    let (column, row) = home_pos.tile();
    let _ = act(
        &mut world,
        TestModeAction::PaletteTravel(Destination::Planet {
            theme: Some(Theme::Jungle),
            pick: Pick::NEAREST,
        }),
    );
    idle(&mut world, 5);
    assert!(
        matches!(world.place_of(PLAYER), Place::OnBody(BodyId::Space(_))),
        "on the jungle world: {:?}",
        world.place_of(PLAYER)
    );
    let _ = act(
        &mut world,
        TestModeAction::PaletteTravel(Destination::Spot {
            place: home_place,
            column,
            row: row - 1,
        }),
    );
    idle(&mut world, 5);
    assert_eq!(world.place_of(PLAYER), Place::HOME);
    let (_, back) = world.rig_world_pos(PLAYER).expect("rig");
    assert!((back.tile().0 - column).abs() <= 1, "back at the column");
}

#[test]
fn stock_stubs_stock_and_spawn() {
    let mut world = deep_world(true);
    let rocket = crate::weapon::AmmoKind::Rocket.item();
    let before = world.player(PLAYER).expect("rig").cargo_bay.count(rocket);
    let _ = act(
        &mut world,
        TestModeAction::PaletteStock(StockRequest::Give {
            item: rocket,
            count: 99,
        }),
    );
    assert_eq!(
        world.player(PLAYER).expect("rig").cargo_bay.count(rocket),
        before + 99
    );
    let kind = EnemyKind::ALL[0];
    let count = |world: &WorldState| {
        world
            .entities()
            .of_kind(crate::entity::EntityKind::Enemy)
            .count()
    };
    let enemies = count(&world);
    let output = act(
        &mut world,
        TestModeAction::PaletteStock(StockRequest::Spawn(kind)),
    );
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(count(&world), enemies + 1);
}

#[test]
fn travel_is_deterministic() {
    let run = || {
        let mut world = deep_world(true);
        let _ = act(
            &mut world,
            TestModeAction::PaletteTravel(Destination::Stratum(Stratum::ALL[20])),
        );
        idle(&mut world, 20);
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
}
