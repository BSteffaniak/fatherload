//! The god-mode toolkit (Wave 14, W14-A): stocking, infinite supplies, the stock audit, the
//! progress switches and the enemy tools.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, PlayerScopedCommandOutcome,
    SimulationParams, TickInputs, TickOutput, WorldState,
    bodylife::WeatherId,
    descent::DescentBoss,
    enemy::{EnemyDifficulty, EnemyKind},
    entity::EntityKind,
    expedition::{codex::CodexPage, progress::StoryEvent},
    place::StoreKey,
    planet::{PlanetParams, PlanetThemeId},
    terrain::TILE_SIZE,
    test_mode::{STOCK_REVISION, StockItem, TestModeAction, TestModeState, TestStockEvent},
    workshop::DeepSupply,
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(1);
const GUEST: PlayerId = PlayerId::new(2);

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

/// A Deep Descent test-mode world (the richest stock: gates, charts, space).
fn deep_test_world() -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &v11(0x57_0C4E), SimulationParams::default());
    world.enable_test_mode();
    world
}

fn classic_test_world() -> WorldState {
    let mut world = WorldState::new_game(PLAYER);
    world.enable_test_mode();
    world
}

fn act(world: &mut WorldState, player: PlayerId, action: TestModeAction) -> TickOutput {
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(player, PlayerCommand::TestMode(action));
    world.simulate_tick(tick, &inputs)
}

fn outcome(output: &TickOutput) -> PlayerScopedCommandOutcome {
    output.action_outcomes[0].outcome
}

fn idle(world: &mut WorldState, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        let inputs = world
            .players()
            .keys()
            .fold(TickInputs::default(), |inputs, id| {
                inputs.hold_as(*id, ControlState::default())
            });
        events.extend(
            world
                .simulate_tick(tick, &inputs)
                .events
                .into_iter()
                .map(|event| event.kind),
        );
    }
    events
}

fn enemies_near(world: &WorldState, player: PlayerId, tiles: f32) -> usize {
    let rig = world.player(player).expect("rig");
    let reach = tiles * TILE_SIZE;
    world
        .enemies()
        .filter(|(entity, _)| {
            let (dx, dy) = (entity.body.x - rig.x, entity.body.y - rig.y);
            dx.mul_add(dx, dy * dy) <= reach * reach
        })
        .count()
}

// --- Stocking ----------------------------------------------------------------------------------

#[test]
fn a_deep_test_world_stocks_every_registered_item_for_every_rig() {
    let mut world = deep_test_world();
    world.add_player(GUEST);
    for player in [PLAYER, GUEST] {
        let gaps = world.test_stock_gaps(player);
        assert!(gaps.is_empty(), "{player:?} lacks {gaps:#?}");
    }
    // The registries the audit walks are really stocked (a sample of each kind).
    assert_eq!(world.stock_count(PLAYER, StockItem::Bombs), 99);
    assert_eq!(
        world.stock_count(PLAYER, StockItem::Supply(DeepSupply::LampFilter)),
        u32::from(crate::workshop::MAX_SUPPLY_STACK)
    );
    assert!(world.space_unlocked() && world.has_star_drive(GUEST));
    assert!(world.crust().deep_claim);
    assert!(
        world.star_charts().len() >= 3,
        "the Orrery's plinths and the Archive's vaults are charted"
    );
    // The gates are not cleared by stocking (beyond the Sinkhole's Deep Claim), the ending is
    // not done and the story is fresh.
    assert!(world.gate_cleared(0));
    assert!((1..8).all(|descent| !world.gate_cleared(descent)));
    assert!(!world.ending_done());
    assert_eq!(world.expedition().found, 0);
    let state = world.test_mode().expect("test mode");
    assert!(state.infinite_supplies && !state.story_unlocked && !state.ai_frozen);
    assert_eq!(state.stock_revision, STOCK_REVISION);
}

#[test]
fn a_classic_test_world_stocks_everything_it_has() {
    let world = classic_test_world();
    let gaps = world.test_stock_gaps(PLAYER);
    assert!(gaps.is_empty(), "{gaps:#?}");
    // No Descents, no space: no charts, no Star Drive, nothing to clear.
    assert!(world.star_charts().is_empty());
    assert!(!world.space_unlocked());
}

#[test]
fn every_stocked_item_is_stocked_to_its_stack() {
    let world = deep_test_world();
    for item in StockItem::all() {
        assert_eq!(
            world.stock_count(PLAYER, item),
            world.stock_limit(PLAYER, item),
            "{item:?}"
        );
    }
}

#[test]
fn infinite_supplies_refill_what_is_spent_and_restock_refills_once() {
    let mut world = classic_test_world();
    // A spent stack is back by the end of the tick.
    let output = act(
        &mut world,
        PLAYER,
        TestModeAction::Give(StockItem::Bombs, 3),
    );
    assert_eq!(outcome(&output), PlayerScopedCommandOutcome::Applied);
    assert_eq!(world.stock_count(PLAYER, StockItem::Bombs), 99);
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::SetInfiniteSupplies(false),
    );
    assert!(!world.test_mode().expect("mode").infinite_supplies);
    let flare = StockItem::Explosive(crate::explosive::ExplosiveKind::Flare);
    let _ = act(&mut world, PLAYER, TestModeAction::Give(flare, 2));
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::Give(StockItem::Bombs, 0),
    );
    idle(&mut world, 5);
    assert_eq!(world.stock_count(PLAYER, flare), 2, "no refill while off");
    assert_eq!(world.stock_count(PLAYER, StockItem::Bombs), 0);
    let output = act(&mut world, PLAYER, TestModeAction::Restock);
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::TestStock {
            what: TestStockEvent::Restocked,
            ..
        }
    )));
    assert_eq!(world.stock_count(PLAYER, flare), flare.stack());
    assert_eq!(world.stock_count(PLAYER, StockItem::Bombs), 99);
    // Spending a bomb for real stays spent with the toggle off.
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(PLAYER, PlayerCommand::PlaceBomb);
    let placed = world.simulate_tick(tick, &inputs);
    if placed.action_outcomes[0].outcome == PlayerScopedCommandOutcome::Applied {
        assert_eq!(world.stock_count(PLAYER, StockItem::Bombs), 98);
    }
}

#[test]
fn give_sets_one_rigs_count_capped_at_its_stack() {
    let mut world = classic_test_world();
    world.add_player(GUEST);
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::SetInfiniteSupplies(false),
    );
    let ballast = StockItem::Supply(DeepSupply::BallastTank);
    let _ = act(&mut world, PLAYER, TestModeAction::Give(ballast, 4));
    assert_eq!(world.stock_count(PLAYER, ballast), 4);
    assert_eq!(
        world.stock_count(GUEST, ballast),
        ballast.stack(),
        "per rig"
    );
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::Give(StockItem::Bombs, 5_000),
    );
    assert_eq!(world.stock_count(PLAYER, StockItem::Bombs), 99);
    let charges = StockItem::DiveCharges;
    let _ = act(&mut world, PLAYER, TestModeAction::Give(charges, 1));
    assert_eq!(world.stock_count(PLAYER, charges), 1);
}

#[test]
fn stock_all_refits_a_rig_that_lost_its_gear() {
    let mut world = deep_test_world();
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.pilot_gear = crate::pilot::PilotGear::default();
        rig.hazard_gear.ship = crate::station::ShipGear::default();
        rig.cosmetic_unlocks = crate::cosmetics::CosmeticUnlocks::default();
    }
    assert!(!world.test_stock_gaps(PLAYER).is_empty());
    let output = act(&mut world, PLAYER, TestModeAction::StockAll);
    assert_eq!(outcome(&output), PlayerScopedCommandOutcome::Applied);
    let gaps = world.test_stock_gaps(PLAYER);
    assert!(gaps.is_empty(), "{gaps:#?}");
}

#[test]
fn an_older_test_mode_save_upgrades_cleanly_and_is_stocked_on_its_first_tick() {
    // A test-mode save from before Wave 14: only the first three toggles, nothing stocked.
    let mut world = WorldState::new_game(PLAYER);
    world.test_mode = Some(TestModeState {
        invulnerable: true,
        infinite_fuel: true,
        no_clip: false,
        ..TestModeState::default()
    });
    let mut json: serde_json::Value = serde_json::to_value(&world).expect("save");
    json["test_mode"] = serde_json::json!({ "invulnerable": true, "infinite_fuel": true });
    let mut loaded: WorldState = serde_json::from_value(json).expect("an old save loads");
    let state = loaded.test_mode().expect("still a test-mode world");
    assert!(state.infinite_supplies, "infinite supplies default on");
    assert_eq!(state.stock_revision, 0);
    assert!(!loaded.test_stock_gaps(PLAYER).is_empty());
    idle(&mut loaded, 1);
    let gaps = loaded.test_stock_gaps(PLAYER);
    assert!(gaps.is_empty(), "{gaps:#?}");
    assert_eq!(
        loaded.test_mode().expect("mode").stock_revision,
        STOCK_REVISION
    );
}

#[test]
fn every_toolkit_action_is_refused_in_a_normal_world() {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &v11(7), SimulationParams::default());
    let before = world.player(PLAYER).expect("rig").clone();
    let actions = [
        TestModeAction::SetInfiniteSupplies(true),
        TestModeAction::Restock,
        TestModeAction::StockAll,
        TestModeAction::Give(StockItem::Bombs, 50),
        TestModeAction::SetStoryUnlocked(true),
        TestModeAction::SetGateCleared {
            descent: 3,
            cleared: true,
        },
        TestModeAction::SetEndingDone(true),
        TestModeAction::SetTimeOfDay { minutes: 600 },
        TestModeAction::SetDayLock(true),
        TestModeAction::SetWeather(Some(WeatherId::STORM)),
        TestModeAction::SpawnCreature(EnemyKind::ALL[0]),
        TestModeAction::ClearNearbyEnemies,
        TestModeAction::SetAiFrozen(true),
    ];
    let clock = world.clock_state().clone();
    for action in actions {
        let output = act(&mut world, PLAYER, action);
        assert_eq!(
            outcome(&output),
            PlayerScopedCommandOutcome::IgnoredUnavailable,
            "{action:?}"
        );
    }
    assert!(world.test_mode().is_none());
    let after = world.player(PLAYER).expect("rig");
    assert_eq!(after.bombs, before.bombs);
    assert_eq!(after.hazard_gear, before.hazard_gear);
    assert!(!world.ending_done() && !world.gate_cleared(3));
    assert_eq!(world.expedition().found, 0);
    assert_eq!(world.clock_state().held_millis, clock.held_millis);
    assert_eq!(world.entities().count_of(EntityKind::Enemy), 0);
}

// --- Progress switches -------------------------------------------------------------------------

#[test]
fn gate_switches_clear_and_restore_each_descent_gate() {
    let mut world = deep_test_world();
    for descent in 1..7_u8 {
        let output = act(
            &mut world,
            PLAYER,
            TestModeAction::SetGateCleared {
                descent,
                cleared: true,
            },
        );
        assert_eq!(outcome(&output), PlayerScopedCommandOutcome::Applied);
        assert!(world.gate_cleared(descent), "gate {descent}");
        // The expedition's own pass reports the boss beaten in the same tick.
        let events: Vec<GameplayEventKind> = output.events.iter().map(|event| event.kind).collect();
        let boss = DescentBoss::ALL[usize::from(descent)];
        assert!(
            events.iter().any(|event| matches!(
                event,
                GameplayEventKind::Story {
                    what: StoryEvent::BossBeaten(beaten),
                    ..
                } if *beaten == boss
            )),
            "{boss:?} reported: {events:?}"
        );
        let _ = act(
            &mut world,
            PLAYER,
            TestModeAction::SetGateCleared {
                descent,
                cleared: false,
            },
        );
        assert!(!world.gate_cleared(descent), "gate {descent} restored");
        assert!(!world.expedition().beat(boss));
    }
    // The Sinkhole's gate is the Deep Claim.
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::SetGateCleared {
            descent: 0,
            cleared: false,
        },
    );
    assert!(!world.crust().deep_claim && !world.gate_cleared(0));
    // No such Descent.
    let output = act(
        &mut world,
        PLAYER,
        TestModeAction::SetGateCleared {
            descent: 9,
            cleared: true,
        },
    );
    assert_eq!(
        outcome(&output),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
}

#[test]
fn the_ending_switch_wakes_the_heart_without_playing_the_ending_and_resets_it() {
    let mut world = deep_test_world();
    world.add_player(GUEST);
    let output = act(&mut world, PLAYER, TestModeAction::SetEndingDone(true));
    assert_eq!(outcome(&output), PlayerScopedCommandOutcome::Applied);
    assert!(world.ending_done() && world.gate_cleared(7));
    assert!(world.space_unlocked() && world.has_star_drive(GUEST));
    let mut events: Vec<GameplayEventKind> = output.events.iter().map(|event| event.kind).collect();
    events.extend(idle(&mut world, 2));
    assert!(
        world
            .ending_elapsed()
            .is_none_or(|elapsed| elapsed > crate::heart::ENDING_SHIELD_MILLIS),
        "no ending sequence plays"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            GameplayEventKind::Story {
                what: StoryEvent::CableLit,
                ..
            }
        )),
        "the cable's moment is not replayed"
    );
    let _ = act(&mut world, PLAYER, TestModeAction::SetEndingDone(false));
    assert!(!world.ending_done() && !world.gate_cleared(7));
    assert!(world.ending_elapsed().is_none());
    assert!(
        world.space_unlocked(),
        "a stocked test world keeps space open"
    );
    // Classic worlds have no Heart.
    let mut classic = classic_test_world();
    let output = act(&mut classic, PLAYER, TestModeAction::SetEndingDone(true));
    assert_eq!(
        outcome(&output),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
}

#[test]
fn unlocking_the_story_fills_the_codex_and_switching_it_off_starts_it_fresh() {
    let mut world = deep_test_world();
    for page in CodexPage::ALL {
        assert!(
            world
                .expedition_codex(page)
                .iter()
                .all(|entry| !entry.found)
        );
    }
    let output = act(&mut world, PLAYER, TestModeAction::SetStoryUnlocked(true));
    assert_eq!(outcome(&output), PlayerScopedCommandOutcome::Applied);
    assert!(world.test_mode().expect("mode").story_unlocked);
    for page in CodexPage::ALL {
        let entries = world.expedition_codex(page);
        assert!(!entries.is_empty());
        assert!(entries.iter().all(|entry| entry.found), "{page:?}");
    }
    assert!(world.expedition().trail_complete());
    let unlocks = world.tombs().unlocks.get(&PLAYER).expect("tomb unlocks");
    assert_eq!(unlocks.codex.len(), crate::tomb::relic::Relic::ALL.len());
    // A rig joining later reads the same codex.
    world.add_player(GUEST);
    assert!(world.tombs().unlocks.contains_key(&GUEST));
    let _ = act(&mut world, PLAYER, TestModeAction::SetStoryUnlocked(false));
    assert!(!world.test_mode().expect("mode").story_unlocked);
    for page in CodexPage::ALL {
        assert!(
            world
                .expedition_codex(page)
                .iter()
                .all(|entry| !entry.found)
        );
    }
    assert_eq!(world.expedition().found, 0);
}

#[test]
fn time_of_day_moves_the_clock_and_the_lock_holds_it() {
    let mut world = classic_test_world();
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::SetTimeOfDay { minutes: 22 * 60 },
    );
    assert_eq!(world.clock().hours_minutes(), (22, 0));
    assert!(world.is_night());
    let world_millis = world.clock().world_millis();
    let _ = act(&mut world, PLAYER, TestModeAction::SetDayLock(true));
    assert!(world.clock().is_held());
    idle(&mut world, 600);
    assert_eq!(world.clock().hours_minutes(), (22, 0), "held");
    assert!(
        world.clock().world_millis() > world_millis,
        "world time runs on"
    );
    // A held clock jumps straight to a new time.
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::SetTimeOfDay { minutes: 12 * 60 },
    );
    assert_eq!(world.clock().hours_minutes(), (12, 0));
    assert!(!world.is_night());
    // Unlocked, it runs on from the held time.
    let _ = act(&mut world, PLAYER, TestModeAction::SetDayLock(false));
    assert!(!world.clock().is_held());
    assert_eq!(world.clock().hours_minutes(), (12, 0));
    let output = act(
        &mut world,
        PLAYER,
        TestModeAction::SetTimeOfDay { minutes: 1_440 },
    );
    assert_eq!(
        outcome(&output),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
}

#[test]
fn time_of_day_lands_on_the_same_minute_at_every_tick_rate() {
    for hz in [30, 60, 120] {
        let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(hz));
        world.enable_test_mode();
        idle(&mut world, hz / 3);
        let _ = act(
            &mut world,
            PLAYER,
            TestModeAction::SetTimeOfDay {
                minutes: 6 * 60 + 30,
            },
        );
        assert_eq!(world.clock().hours_minutes(), (6, 30), "{hz} Hz");
    }
}

#[test]
fn forced_weather_blows_over_a_body_and_clears_again() {
    let params = PlanetParams::new(0x00B0_D1E5, 640, PlanetThemeId::ROCKY);
    let mut world = WorldState::new_planet_world(PLAYER, params, SimulationParams::new(60));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.enable_test_mode();
    // Hover over the open surface.
    let surface = world.terrain().surface_row(200);
    let (x, y) = world.frame().centre_of(200, surface - 12);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.x = x;
        rig.y = y;
    }
    world.stream_terrain_near_players();
    let life = world.body_life(StoreKey::Home).expect("a lone planet");
    let _ = life;
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::SetWeather(Some(WeatherId::STORM)),
    );
    assert_eq!(world.forced_weather(), Some(WeatherId::STORM));
    let rig = world.player(PLAYER).expect("rig");
    let weather = world.body_weather_at(rig.x, rig.y);
    assert_eq!(weather.kind, WeatherId::STORM);
    assert_eq!(weather.intensity_permille, 1_000);
    let _ = act(&mut world, PLAYER, TestModeAction::SetWeather(None));
    assert_eq!(world.forced_weather(), None);
}

// --- Enemy tools -------------------------------------------------------------------------------

#[test]
fn a_spawned_creature_appears_beside_the_rig_and_clearing_removes_it() {
    let mut world = classic_test_world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let before = enemies_near(&world, PLAYER, 12.0);
    let output = act(
        &mut world,
        PLAYER,
        TestModeAction::SpawnCreature(EnemyKind::ALL[0]),
    );
    assert_eq!(outcome(&output), PlayerScopedCommandOutcome::Applied);
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::TestStock {
            what: TestStockEvent::CreatureSpawned,
            ..
        }
    )));
    assert_eq!(enemies_near(&world, PLAYER, 12.0), before + 1);
    // At a safe offset: not on top of the rig.
    let rig = world.player(PLAYER).expect("rig");
    let nearest = world
        .enemies()
        .map(|(entity, _)| {
            let (dx, dy) = (entity.body.x - rig.x, entity.body.y - rig.y);
            dx.mul_add(dx, dy * dy)
        })
        .fold(f32::MAX, f32::min);
    assert!(
        nearest >= (2.0 * TILE_SIZE) * (2.0 * TILE_SIZE),
        "{nearest}"
    );
    let _ = act(&mut world, PLAYER, TestModeAction::ClearNearbyEnemies);
    assert_eq!(enemies_near(&world, PLAYER, 12.0), 0);
}

#[test]
fn spawning_is_refused_while_creatures_are_switched_off() {
    let mut world = classic_test_world();
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let output = act(
        &mut world,
        PLAYER,
        TestModeAction::SpawnCreature(EnemyKind::ALL[0]),
    );
    assert_eq!(
        outcome(&output),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
}

#[test]
fn every_creature_and_boss_kind_spawns_beside_a_rig_without_trouble() {
    let mut world = deep_test_world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    // Somewhere underground with rock round the rig.
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::WarpToBand(crate::worldgen::Band::Stone),
    );
    for kind in EnemyKind::ALL {
        let output = act(&mut world, PLAYER, TestModeAction::SpawnCreature(kind));
        assert_eq!(
            outcome(&output),
            PlayerScopedCommandOutcome::Applied,
            "{kind:?}"
        );
        idle(&mut world, 2);
        let _ = act(&mut world, PLAYER, TestModeAction::ClearNearbyEnemies);
    }
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.hull > 0.0);
}

#[test]
fn frozen_ai_stands_still_until_it_is_let_go() {
    let mut world = classic_test_world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let flyer = EnemyKind::ALL
        .into_iter()
        .find(|kind| matches!(kind.stats().locomotion, crate::enemy::Locomotion::Flying))
        .expect("a flyer");
    let _ = act(&mut world, PLAYER, TestModeAction::SetAiFrozen(true));
    assert!(world.test_mode().expect("mode").ai_frozen);
    let _ = act(&mut world, PLAYER, TestModeAction::SpawnCreature(flyer));
    let positions = |world: &WorldState| -> Vec<(f32, f32)> {
        world
            .enemies()
            .map(|(entity, _)| (entity.body.x, entity.body.y))
            .collect()
    };
    let frozen = positions(&world);
    assert!(!frozen.is_empty());
    idle(&mut world, 120);
    assert_eq!(positions(&world), frozen, "nothing moved");
    let _ = act(&mut world, PLAYER, TestModeAction::SetAiFrozen(false));
    idle(&mut world, 120);
    assert_ne!(positions(&world), frozen, "they move again");
}

// --- Replication -------------------------------------------------------------------------------

#[test]
fn the_toolkit_state_replicates_and_round_trips() {
    let mut world = deep_test_world();
    let _ = act(&mut world, PLAYER, TestModeAction::SetAiFrozen(true));
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::SetWeather(Some(WeatherId::SNOW)),
    );
    let _ = act(&mut world, PLAYER, TestModeAction::SetDayLock(true));
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.test_mode(), world.test_mode());
    assert_eq!(loaded.clock_state(), world.clock_state());
    let mut client =
        WorldState::new_game_with_settings(PLAYER, &v11(0x57_0C4E), SimulationParams::default());
    for key in [crate::SectionKey::TestMode, crate::SectionKey::Clock] {
        client.apply_section(key, world.section(key).expect("section"));
    }
    assert_eq!(client.test_mode(), world.test_mode());
    assert!(client.clock().is_held());
}

#[test]
fn a_switch_flipped_from_another_place_reaches_the_whole_crew() {
    use crate::{
        place::{BodyId, Place, SpaceBodyKey},
        position::WorldPos,
    };
    let mut world = deep_test_world();
    world.add_player(GUEST);
    // The guest stands on a body: its commands run in that store's pass, with the home rig
    // set aside.
    let body = Place::OnBody(BodyId::Space(SpaceBodyKey::new(3, -2, 1, 0)));
    world
        .move_to_place(GUEST, body, WorldPos::tile_center(10, -2))
        .expect("arrives");
    let mut heart = world.heart().clone();
    heart.star_drive.clear();
    world.set_heart(heart);
    let output = act(&mut world, GUEST, TestModeAction::SetEndingDone(true));
    assert_eq!(outcome(&output), PlayerScopedCommandOutcome::Applied);
    assert!(world.ending_done());
    assert!(
        world.has_star_drive(PLAYER) && world.has_star_drive(GUEST),
        "the home rig gets the Star Drive too"
    );
    let _ = act(&mut world, GUEST, TestModeAction::SetStoryUnlocked(true));
    for player in [PLAYER, GUEST] {
        let unlocks = world.tombs().unlocks.get(&player).expect("tomb unlocks");
        assert_eq!(
            unlocks.codex.len(),
            crate::tomb::relic::Relic::ALL.len(),
            "{player:?}"
        );
    }
}

#[test]
fn the_story_unlock_reaches_the_galaxy_journal_and_every_rigs_derelict_logs() {
    use crate::galaxy_story::journal::{FRAGMENTS, SPACE_LOGS};
    let mut world = deep_test_world();
    let _ = act(&mut world, PLAYER, TestModeAction::SetStoryUnlocked(true));
    assert_eq!(world.galaxy_story().logs.len(), SPACE_LOGS.len());
    assert_eq!(world.galaxy_story().fragments.len(), FRAGMENTS.len());
    let lore = crate::poi::LORE.len();
    assert_eq!(world.player(PLAYER).expect("rig").journal.logs.len(), lore);
    // A rig joining later reads them too (the refill keeps the unlock whole).
    world.add_player(GUEST);
    let _ = idle(&mut world, 1);
    assert_eq!(world.player(GUEST).expect("rig").journal.logs.len(), lore);
    let _ = act(&mut world, PLAYER, TestModeAction::SetStoryUnlocked(false));
    assert!(world.galaxy_story().fragments.is_empty());
    assert!(
        world.galaxy_story().logs.is_empty(),
        "no chapter done, no log"
    );
    for player in [PLAYER, GUEST] {
        assert!(world.player(player).expect("rig").journal.logs.is_empty());
    }
}
