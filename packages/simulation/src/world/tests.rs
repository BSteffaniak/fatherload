#![allow(
    clippy::too_many_lines,
    clippy::float_cmp,
    reason = "canonical simulation tests enumerate exact deterministic outcomes"
)]

use std::collections::BTreeMap;

use crate::{
    DrillDirection, DrillState, FIXED_DELTA_SECONDS, InfrastructureKind, InputSequence,
    PlayerCommand, PlayerId, SequencedPlayerCommand, SimulationTick, TILE_SIZE, WorldImport,
    WorldState,
    terrain::{TileKind, TilePosition},
};

const LOCAL_PLAYER_ID: PlayerId = PlayerId::new(1);

#[test]
fn world_state_applies_player_scoped_drilling_intent() {
    let mut world = WorldState::new_game(LOCAL_PLAYER_ID);

    let outcome = world.apply_player_command(
        LOCAL_PLAYER_ID,
        &PlayerCommand::Movement {
            horizontal: 0.0,
            thrust: false,
            drill_down: true,
        },
    );

    assert_eq!(outcome, crate::PlayerScopedCommandOutcome::Applied);
    assert!(world.active_drill(LOCAL_PLAYER_ID).is_some());

    world.apply_player_command(
        LOCAL_PLAYER_ID,
        &PlayerCommand::Movement {
            horizontal: 0.0,
            thrust: false,
            drill_down: false,
        },
    );

    assert!(world.active_drill(LOCAL_PLAYER_ID).is_none());
}

#[test]
fn world_state_applies_player_scoped_bomb_and_infrastructure_placement() {
    let mut game = WorldImport::new_game(LOCAL_PLAYER_ID);
    game.player.bombs = 1;
    game.player.signal_relay_kits = 1;
    let mut world = WorldState::from_import(&game);

    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::PlaceBomb),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.bomb_count(), 1);
    assert_eq!(
        world.player(LOCAL_PLAYER_ID).expect("player exists").bombs,
        0
    );

    assert_eq!(
        world.apply_player_command(
            LOCAL_PLAYER_ID,
            &PlayerCommand::PlaceInfrastructure { slot: 0 },
        ),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.infrastructure_count(), 1);
    assert_eq!(
        world.infrastructure()[0].kind,
        InfrastructureKind::SignalRelay
    );
    assert_eq!(
        world
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .signal_relay_kits,
        0
    );
}

#[test]
fn world_state_records_player_scoped_service_transactions() {
    let mut game = WorldImport::new_game(LOCAL_PLAYER_ID);
    game.player.fuel = 1.0;
    game.player.hull = 1.0;
    game.player.credits = 1_000;
    game.player.add_cargo(crate::terrain::MineralKind::Copper);
    let mut world = WorldState::from_import(&game);

    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::Refuel),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::Repair),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::SellCargo),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::BuyUpgrade { index: 0 }),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::Rescue),
        crate::PlayerScopedCommandOutcome::Applied
    );

    assert_eq!(world.service_transactions().len(), 5);
    assert_eq!(
        world.service_transactions()[0].kind,
        crate::PlayerTransactionKind::Refuel
    );
    assert!(
        world.service_transactions()[0].credits_after
            < world.service_transactions()[0].credits_before
    );
    assert_eq!(
        world.service_transactions()[2].kind,
        crate::PlayerTransactionKind::SellCargo
    );
    assert_eq!(world.service_transactions()[2].cargo_after, 0);
    assert_eq!(
        world.service_transactions()[4].kind,
        crate::PlayerTransactionKind::Rescue
    );
}

#[test]
fn world_state_exposes_mutable_player_lookup() {
    let mut world = WorldState::new_game(LOCAL_PLAYER_ID);

    world
        .player_mut(LOCAL_PLAYER_ID)
        .expect("local player exists")
        .credits = 123;

    assert_eq!(
        world
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .credits,
        123
    );
}

#[test]
fn compatibility_world_applies_player_scoped_commands_to_selected_player() {
    let mut world = WorldState::new_game(LOCAL_PLAYER_ID);

    assert_eq!(
        world.apply_player_command(
            LOCAL_PLAYER_ID,
            &PlayerCommand::Movement {
                horizontal: 0.75,
                thrust: true,
                drill_down: false,
            },
        ),
        crate::PlayerScopedCommandOutcome::Applied
    );
    let velocity_x = world
        .player(LOCAL_PLAYER_ID)
        .expect("player exists")
        .velocity_x;
    assert!((velocity_x - 0.75).abs() < f32::EPSILON);
    assert_eq!(
        world.apply_player_command(PlayerId::new(999), &PlayerCommand::Refuel),
        crate::PlayerScopedCommandOutcome::UnknownPlayer
    );
}

#[test]
fn compatibility_world_applies_player_scoped_resource_commands() {
    let mut world = WorldState::new_game(LOCAL_PLAYER_ID);
    let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
    player.fuel = 1.0;
    player.hull = 1.0;
    player.credits = 10_000;
    player.bombs = 1;

    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::Refuel),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::Repair),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::PlaceBomb),
        crate::PlayerScopedCommandOutcome::Applied
    );
    let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
    assert!((player.fuel - player.fuel_capacity).abs() < f32::EPSILON);
    assert!((player.hull - player.max_hull()).abs() < f32::EPSILON);
    assert_eq!(player.bombs, 0);
}

#[test]
fn compatibility_world_tracks_per_player_active_drill_and_scanner_cooldown() {
    let mut world = WorldState::new_game(LOCAL_PLAYER_ID);
    let drill = DrillState {
        target: crate::terrain::TilePosition { x: 1, y: 2 },
        direction: crate::DrillDirection::Down,
        progress: 0.5,
        initial_durability: 3,
        seconds_per_chip: 0.25,
        sound_timer: 0.0,
        dust_timer: 0.0,
    };

    world.set_active_drill(LOCAL_PLAYER_ID, Some(drill));
    world.set_scanner_cooldown_seconds(LOCAL_PLAYER_ID, 2.0);

    assert_eq!(
        world
            .active_drill(LOCAL_PLAYER_ID)
            .expect("drill set")
            .target
            .y,
        2
    );
    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::UseScanner),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert!(
        world
            .scanner_cooldown_seconds(LOCAL_PLAYER_ID)
            .expect("cooldown set")
            > 0.0
    );
    world.set_active_drill(LOCAL_PLAYER_ID, None);
    assert!(world.active_drill(LOCAL_PLAYER_ID).is_none());
}

#[test]
fn compatibility_world_summarizes_inventory_and_applies_upgrade_intent() {
    let mut game = WorldImport::new_game(LOCAL_PLAYER_ID);
    game.player.credits = 420;
    let mut world = WorldState::from_import(&game);
    let before = world
        .player_inventory_summary(LOCAL_PLAYER_ID)
        .expect("player summary");

    assert_eq!(before.cargo_used, 0);
    assert_eq!(before.credits, 420);
    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::BuyUpgrade { index: 0 }),
        crate::PlayerScopedCommandOutcome::Applied
    );
    let after = world
        .player_inventory_summary(LOCAL_PLAYER_ID)
        .expect("player summary");
    assert!(after.upgrade_level_total > before.upgrade_level_total);
}

#[test]
fn canonical_simulation_hash_matches_for_identical_tick_replay() {
    fn run() -> (crate::SimulationStateHash, crate::TickOutput) {
        let game = WorldImport::new_game(LOCAL_PLAYER_ID);
        let mut world = WorldState::from_import(&game);
        let tick = SimulationTick::new(7);
        let commands = vec![
            SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(11),
                target_tick: tick,
                command: PlayerCommand::Movement {
                    horizontal: 0.375,
                    thrust: true,
                    drill_down: false,
                },
            },
            SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(12),
                target_tick: tick,
                command: PlayerCommand::PlaceBomb,
            },
        ];
        let inputs = crate::TickInputs::from_commands(&commands);
        let mut terrain_revisions = crate::TerrainRevisionTracker::default();
        let output = world.simulate_tick(tick, &inputs, &mut terrain_revisions);
        (world.simulation_state_hash(), output)
    }

    let (first_hash, first_output) = run();
    let (second_hash, second_output) = run();

    assert_eq!(first_output, second_output);
    assert_eq!(first_hash, second_hash);
    assert_ne!(first_hash.get(), 0);
}

#[test]
fn simulation_hash_changes_when_gameplay_state_changes() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    let before = world.simulation_state_hash();
    world
        .player_mut(LOCAL_PLAYER_ID)
        .expect("player exists")
        .credits += 1;

    assert_ne!(before, world.simulation_state_hash());
}

#[test]
fn canonical_timers_use_tick_deadlines_and_rng_state_is_replayable() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut first = WorldState::from_import(&game);
    let mut second = WorldState::from_import(&game);
    first.set_scanner_cooldown_seconds(LOCAL_PLAYER_ID, 1.0);
    second.set_scanner_cooldown_seconds(LOCAL_PLAYER_ID, 1.0);
    let mut first_revisions = crate::TerrainRevisionTracker::default();
    let mut second_revisions = crate::TerrainRevisionTracker::default();

    for tick in 0..60 {
        let tick = SimulationTick::new(tick);
        let inputs = crate::TickInputs::default();
        let _first = first.simulate_tick(tick, &inputs, &mut first_revisions);
        let _second = second.simulate_tick(tick, &inputs, &mut second_revisions);
    }

    assert_eq!(first.scanner_cooldown_seconds(LOCAL_PLAYER_ID), Some(0.0));
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
    let first_values = [first.rng.next_u64(), first.rng.next_u64()];
    let second_values = [second.rng.next_u64(), second.rng.next_u64()];
    assert_eq!(first_values, second_values);
    assert_ne!(first_values[0], first_values[1]);
}

#[test]
fn canonical_tick_applies_depth_pressure_and_lava_damage() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    let lava = TilePosition { x: 20, y: 100 };
    assert!(world.terrain_mut().set_kind(lava, TileKind::Lava));
    {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = 20.0 * TILE_SIZE;
        player.y = 100.0 * TILE_SIZE;
        player.hull = 100.0;
    }
    let mut terrain_revisions = crate::TerrainRevisionTracker::default();

    let output = world.simulate_tick(
        SimulationTick::new(8),
        &crate::TickInputs::default(),
        &mut terrain_revisions,
    );

    assert!(world.player(LOCAL_PLAYER_ID).expect("player exists").hull < 100.0);
    assert!(output.events.iter().any(|event| {
        event.kind
            == crate::GameplayEventKind::PlayerDamaged {
                player_id: LOCAL_PLAYER_ID,
            }
    }));
}

#[test]
fn canonical_deep_rewards_and_escape_sequence_are_tick_deterministic() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut first = WorldState::from_import(&game);
    let mut second = first.clone();
    for world in [&mut first, &mut second] {
        world.player_mut(LOCAL_PLAYER_ID).expect("player exists").y = 120.0 * TILE_SIZE;
        world.progression.escape_deadline = Some(SimulationTick::new(2));
        world.set_simulation_tick(SimulationTick::new(0));
    }
    let mut first_revisions = crate::TerrainRevisionTracker::default();
    let mut second_revisions = crate::TerrainRevisionTracker::default();
    let first_output = first.simulate_tick(
        SimulationTick::new(0),
        &crate::TickInputs::default(),
        &mut first_revisions,
    );
    let second_output = second.simulate_tick(
        SimulationTick::new(0),
        &crate::TickInputs::default(),
        &mut second_revisions,
    );
    assert_eq!(first_output, second_output);
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
    assert!(first_output.events.iter().any(|event| {
        event.kind
            == crate::GameplayEventKind::DeepRewardGranted {
                player_id: LOCAL_PLAYER_ID,
            }
    }));
    assert!(
        first
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .materials
            .contains_key(&crate::terrain::StrategicResourceKind::PressurePearl)
    );
    let output = first.simulate_tick(
        SimulationTick::new(1),
        &crate::TickInputs::default(),
        &mut first_revisions,
    );
    assert!(first.failure_state(LOCAL_PLAYER_ID).is_some());
    assert!(first.progression.escape_deadline.is_none());
    assert!(output.events.iter().any(|event| {
        event.kind
            == crate::GameplayEventKind::PlayerDamaged {
                player_id: LOCAL_PLAYER_ID,
            }
    }));
}

#[test]
fn canonical_world_events_expire_by_day_deadline() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    world
        .world_event_severity
        .insert(crate::WorldEventKind::MarketBoom, 2);
    world
        .world_event_days_remaining
        .insert(crate::WorldEventKind::MarketBoom, 1);
    world.progression.town_event_day = 1;
    world.progression.next_day_tick = SimulationTick::new(1);
    let mut revisions = crate::TerrainRevisionTracker::default();
    let _output = world.simulate_tick(
        SimulationTick::new(0),
        &crate::TickInputs::default(),
        &mut revisions,
    );
    assert!(
        !world
            .world_event_severity
            .contains_key(&crate::WorldEventKind::MarketBoom)
    );
    assert!(
        !world
            .world_event_days_remaining
            .contains_key(&crate::WorldEventKind::MarketBoom)
    );
}

#[test]
fn canonical_day_and_world_event_schedule_is_tick_deterministic() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut first = WorldState::from_import(&game);
    let mut second = first.clone();
    for world in [&mut first, &mut second] {
        world.progression.town_development.bank_level = 2;
        world.progression.town_event_day = 2;
        world.progression.market_salt = 0;
        world.progression.next_day_tick = SimulationTick::new(1);
        world.infrastructure.push(crate::PlacedInfrastructure {
            kind: InfrastructureKind::PumpStation,
            position: TilePosition { x: 20, y: 10 },
            durability: 100,
        });
    }
    let mut first_revisions = crate::TerrainRevisionTracker::default();
    let mut second_revisions = crate::TerrainRevisionTracker::default();
    let first_output = first.simulate_tick(
        SimulationTick::new(0),
        &crate::TickInputs::default(),
        &mut first_revisions,
    );
    let second_output = second.simulate_tick(
        SimulationTick::new(0),
        &crate::TickInputs::default(),
        &mut second_revisions,
    );
    assert_eq!(first_output, second_output);
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
    assert_eq!(first.progression.town_event_day, 3);
    assert!(!first.world_event_severity.is_empty());
    assert!(
        first
            .world_event_days_remaining
            .values()
            .all(|days| *days > 0)
    );
    assert_eq!(first.infrastructure[0].durability, 65);
    assert_eq!(
        first
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .credits,
        24
    );
    assert_eq!(first.progression.next_day_tick.get(), 18_001);
}

#[test]
fn canonical_salvage_subservices_update_only_canonical_state() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    world.progression.town_development.salvage_yard_level = 4;
    world.progression.town_development.scanner_lab_level = 2;
    world.rescue_counts.insert(LOCAL_PLAYER_ID, 1);
    world.collapse_warnings = vec![TilePosition { x: 10, y: 20 }, TilePosition { x: 11, y: 20 }];
    world.scan_markers = vec![crate::ScanMarker {
        position: TilePosition { x: 12, y: 20 },
        kind: TileKind::Gas,
    }];
    {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.cargo.insert(crate::terrain::MineralKind::Copper, 2);
        player.cargo.insert(crate::terrain::MineralKind::Diamond, 1);
    }
    for request in [
        crate::SessionServiceRequest::SalvageRecoverWreckedPart,
        crate::SessionServiceRequest::SalvageClearCollapseZones,
        crate::SessionServiceRequest::SalvageSellScrapTip,
        crate::SessionServiceRequest::SellScanData,
        crate::SessionServiceRequest::AutoSortLowGradeCargo,
    ] {
        assert_eq!(
            world.apply_canonical_salvage_service(LOCAL_PLAYER_ID, request),
            Some(crate::PlayerScopedCommandOutcome::Applied)
        );
    }
    assert!(world.collapse_warnings.is_empty());
    assert!(world.scan_markers.is_empty());
    assert!(
        world
            .progression
            .rig_part_inventory
            .contains(&crate::RigPartKind::ArmoredCargoBay)
    );
    let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
    assert!(
        !player
            .cargo
            .contains_key(&crate::terrain::MineralKind::Copper)
    );
    assert_eq!(
        player.cargo.get(&crate::terrain::MineralKind::Diamond),
        Some(&1)
    );
    assert!(player.credits > 0);
    assert!(world.progression.total_earnings > 0);
}

#[test]
fn canonical_market_sale_applies_events_depot_bonus_and_history() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut baseline = WorldState::from_import(&game);
    let mut boom = baseline.clone();
    for world in [&mut baseline, &mut boom] {
        world.progression.town_development.depot_level = 2;
        world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("player exists")
            .cargo
            .insert(crate::terrain::MineralKind::Diamond, 2);
    }
    boom.world_event_severity
        .insert(crate::WorldEventKind::MarketBoom, 1);
    boom.world_event_severity
        .insert(crate::WorldEventKind::RareBuyer, 1);
    assert_eq!(
        baseline.sell_canonical_cargo(LOCAL_PLAYER_ID),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        boom.sell_canonical_cargo(LOCAL_PLAYER_ID),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert!(
        boom.player(LOCAL_PLAYER_ID).expect("player exists").credits
            > baseline
                .player(LOCAL_PLAYER_ID)
                .expect("player exists")
                .credits
    );
    assert_eq!(
        boom.player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .cargo_used(),
        0
    );

    let previous_day = boom.progression.town_event_day;
    boom.advance_canonical_market_day();
    assert_eq!(boom.progression.town_event_day, previous_day + 1);
    assert!(boom.progression.market_history.len() <= 7);
    assert!(
        boom.progression
            .mineral_market_history
            .values()
            .all(|history| history.len() <= 7)
    );
}

#[test]
fn canonical_expedition_acceptance_and_special_rewards_are_deterministic() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut first = WorldState::from_import(&game);
    let mut second = first.clone();
    for world in [&mut first, &mut second] {
        world.progression.deep_claim_status = crate::economy::DeepClaimStatus::Unlocked;
        world.progression.expedition_offers = vec![crate::Expedition {
            kind: crate::ExpeditionObjectiveKind::RetrieveArtifact,
            target: TileKind::Artifact(crate::terrain::ArtifactKind::Fossil),
            required: 1,
            reward: 600,
            expires_day: 2,
        }];
        assert_eq!(
            world.accept_canonical_expedition(0),
            crate::PlayerScopedCommandOutcome::Applied
        );
        world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("player exists")
            .artifacts
            .insert(crate::terrain::ArtifactKind::Fossil, 1);
        assert_eq!(
            world.complete_canonical_progression_for_player(LOCAL_PLAYER_ID),
            crate::PlayerScopedCommandOutcome::Applied
        );
    }
    assert!(first.progression.expedition_offers.is_empty());
    assert!(first.progression.active_expeditions.is_empty());
    assert!(
        first
            .progression
            .legendary_blueprints
            .contains(&crate::LegendaryBlueprint::StarPlating)
    );
    assert!(
        first
            .progression
            .rig_part_inventory
            .contains(&crate::RigPartKind::ResonanceDrill)
    );
    assert_eq!(first.progression.town_development.reputation, 1);
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
}

#[test]
fn canonical_salvage_recovers_exact_lost_inventory_and_can_launch_drone() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    world.progression.town_development.salvage_yard_level = 2;
    world.lost_cargo_sites.insert(
        LOCAL_PLAYER_ID,
        crate::LostCargoSite {
            x: 20.0 * TILE_SIZE,
            y: 40.0 * TILE_SIZE,
            minerals: BTreeMap::from([(crate::terrain::MineralKind::Copper, 2)]),
            artifacts: BTreeMap::from([(crate::terrain::ArtifactKind::Fossil, 1)]),
        },
    );
    world
        .player_mut(LOCAL_PLAYER_ID)
        .expect("player exists")
        .credits = 100;

    assert_eq!(
        world.apply_canonical_progression_service(
            LOCAL_PLAYER_ID,
            crate::SessionServiceRequest::SalvageLaunchDrone,
        ),
        Some(crate::PlayerScopedCommandOutcome::Applied)
    );
    let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
    assert_eq!(
        player.cargo.get(&crate::terrain::MineralKind::Copper),
        Some(&2)
    );
    assert_eq!(
        player.artifacts.get(&crate::terrain::ArtifactKind::Fossil),
        Some(&1)
    );
    assert_eq!(player.credits, 82);
    assert!(!world.lost_cargo_sites.contains_key(&LOCAL_PLAYER_ID));
    assert!(world.infrastructure.iter().any(|item| {
        item.kind == InfrastructureKind::SurveyDrone
            && item.position == TilePosition { x: 20, y: 40 }
    }));
}

#[test]
fn canonical_side_contract_lifecycle_posts_completes_and_expires() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut first = WorldState::from_import(&game);
    let mut second = first.clone();
    for world in [&mut first, &mut second] {
        assert_eq!(
            world.start_canonical_side_contract(),
            crate::PlayerScopedCommandOutcome::Applied
        );
        let contract = world.progression.active_side_contracts[0];
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        if let TileKind::Ore(mineral) = contract.target {
            player.cargo.insert(mineral, contract.required);
        }
        assert_eq!(
            world.complete_canonical_side_contract(LOCAL_PLAYER_ID),
            crate::PlayerScopedCommandOutcome::Applied
        );
    }
    assert!(first.progression.active_side_contracts.is_empty());
    assert!(first.progression.total_earnings > 0);
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );

    first.progression.town_event_day = 3;
    first
        .progression
        .active_side_contracts
        .push(crate::SideContract {
            kind: crate::SideContractKind::Rush,
            target: TileKind::Ore(crate::terrain::MineralKind::Ruby),
            required: 1,
            expires_day: Some(2),
        });
    first
        .player_mut(LOCAL_PLAYER_ID)
        .expect("player exists")
        .cargo
        .insert(crate::terrain::MineralKind::Ruby, 1);
    assert_eq!(
        first.complete_canonical_side_contract(LOCAL_PLAYER_ID),
        crate::PlayerScopedCommandOutcome::IgnoredUnavailable
    );
    assert!(first.progression.active_side_contracts.is_empty());
}

#[test]
fn canonical_banking_insurance_crafting_and_town_upgrades_are_transactional() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
    player.credits = 2_000;

    assert_eq!(
        world.apply_canonical_progression_service(
            LOCAL_PLAYER_ID,
            crate::SessionServiceRequest::Finance,
        ),
        Some(crate::PlayerScopedCommandOutcome::Applied)
    );
    let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
    assert!(player.loan_debt > 0);
    assert!(player.credits > 2_000);
    assert_eq!(
        world.apply_canonical_progression_service(
            LOCAL_PLAYER_ID,
            crate::SessionServiceRequest::BuyInsurance,
        ),
        Some(crate::PlayerScopedCommandOutcome::Applied)
    );
    assert!(
        world
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .insured
    );

    let recipe = crate::RecipeKind::AuxiliaryTank;
    for (material, required) in recipe.cost() {
        world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("player exists")
            .materials
            .insert(*material, *required);
    }
    let capacity_before = world
        .player(LOCAL_PLAYER_ID)
        .expect("player exists")
        .fuel_capacity;
    assert_eq!(
        world.apply_canonical_progression_service(
            LOCAL_PLAYER_ID,
            crate::SessionServiceRequest::CraftRecipe { recipe },
        ),
        Some(crate::PlayerScopedCommandOutcome::Applied)
    );
    assert!(
        world
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .fuel_capacity
            > capacity_before
    );

    let building = crate::economy::TownBuilding::Depot;
    let cost = world.progression.town_development.upgrade_cost(building);
    world
        .player_mut(LOCAL_PLAYER_ID)
        .expect("player exists")
        .credits = cost;
    assert_eq!(
        world.apply_canonical_progression_service(
            LOCAL_PLAYER_ID,
            crate::SessionServiceRequest::UpgradeTownBuilding { building },
        ),
        Some(crate::PlayerScopedCommandOutcome::Applied)
    );
    assert_eq!(world.progression.town_development.level(building), 1);
    assert_eq!(world.progression.town_development.reputation, 1);
    assert!(world.service_transactions.len() >= 4);
}

#[test]
fn canonical_story_contract_progression_consumes_cargo_and_hashes_state() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut first = WorldState::from_import(&game);
    let mut second = first.clone();
    for world in [&mut first, &mut second] {
        let contract = world.progression.contracts.active.clone();
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        match contract.target {
            crate::contract::ContractTarget::Mineral(mineral) => {
                player.cargo.insert(mineral, contract.required);
            }
            crate::contract::ContractTarget::Artifact(artifact) => {
                player.artifacts.insert(artifact, contract.required);
            }
        }
    }
    assert_eq!(
        first.complete_canonical_progression_for_player(LOCAL_PLAYER_ID),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        second.complete_canonical_progression_for_player(LOCAL_PLAYER_ID),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(first.progression.contracts.completed, 1);
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
    assert!(
        first
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .cargo_used()
            == 0
    );
    assert!(first.progression.total_earnings > 0);
}

#[test]
fn canonical_expedition_completion_consumes_delivery_and_rewards_player() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    let expedition = crate::Expedition {
        kind: crate::ExpeditionObjectiveKind::DeliverCargo,
        target: TileKind::Ore(crate::terrain::MineralKind::Copper),
        required: 2,
        reward: 350,
        expires_day: 2,
    };
    world.progression.active_expeditions.push(expedition);
    let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
    player.cargo.insert(crate::terrain::MineralKind::Copper, 2);
    let credits_before = player.credits;

    assert_eq!(
        world.complete_canonical_progression_for_player(LOCAL_PLAYER_ID),
        crate::PlayerScopedCommandOutcome::Applied
    );
    let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
    assert_eq!(player.credits, credits_before + expedition.reward);
    assert!(
        !player
            .cargo
            .contains_key(&crate::terrain::MineralKind::Copper)
    );
    assert!(world.progression.active_expeditions.is_empty());
    assert_eq!(world.progression.expeditions_completed, 1);
    assert_eq!(world.progression.total_earnings, expedition.reward);
}

#[test]
fn canonical_heat_wave_and_deep_instability_affect_survival_deterministically() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut baseline = WorldState::from_import(&game);
    let mut heat_wave = baseline.clone();
    heat_wave
        .world_event_severity
        .insert(crate::WorldEventKind::HeatWave, 2);
    for world in [&mut baseline, &mut heat_wave] {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.y = 40.0 * TILE_SIZE;
        player.hull = 100.0;
    }
    for world in [&mut baseline, &mut heat_wave] {
        let mut revisions = crate::TerrainRevisionTracker::default();
        let _output = world.simulate_tick(
            SimulationTick::new(0),
            &crate::TickInputs::default(),
            &mut revisions,
        );
    }
    assert!(
        heat_wave
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .hull
            < baseline
                .player(LOCAL_PLAYER_ID)
                .expect("player exists")
                .hull
    );

    let mut first = WorldState::from_import(&game);
    let mut second = first.clone();
    for world in [&mut first, &mut second] {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.y = 110.0 * TILE_SIZE;
        player.fuel = 100.0;
        player.hull = 100.0;
        player.cargo.insert(crate::terrain::MineralKind::Diamond, 1);
        world.set_simulation_tick(SimulationTick::new(239));
        world.deep_run_states.insert(
            LOCAL_PLAYER_ID,
            super::DeepRunState {
                trip_ticks: 100,
                instability: 100.0,
            },
        );
    }
    let mut first_revisions = crate::TerrainRevisionTracker::default();
    let mut second_revisions = crate::TerrainRevisionTracker::default();
    let first_output = first.simulate_tick(
        SimulationTick::new(239),
        &crate::TickInputs::default(),
        &mut first_revisions,
    );
    let second_output = second.simulate_tick(
        SimulationTick::new(239),
        &crate::TickInputs::default(),
        &mut second_revisions,
    );
    assert_eq!(first_output, second_output);
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
    assert_eq!(first.falling_boulders.len(), 3);
    assert!(!first.hazards.is_empty());
    assert!(first.player(LOCAL_PLAYER_ID).expect("player exists").fuel < 100.0);
}

#[test]
fn canonical_rescue_site_recovers_lost_cargo_once() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = 20.0 * TILE_SIZE;
        player.y = 40.0 * TILE_SIZE;
        player.cargo.insert(crate::terrain::MineralKind::Copper, 4);
    }
    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::Rescue),
        crate::PlayerScopedCommandOutcome::Applied
    );
    let site = world
        .lost_cargo_sites()
        .get(&LOCAL_PLAYER_ID)
        .cloned()
        .expect("rescue records lost cargo site");
    let retained_count = world
        .player(LOCAL_PLAYER_ID)
        .expect("player exists")
        .cargo
        .get(&crate::terrain::MineralKind::Copper)
        .copied()
        .unwrap_or(0);
    {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = site.x;
        player.y = site.y;
    }
    let mut revisions = crate::TerrainRevisionTracker::default();
    let output = world.simulate_tick(
        SimulationTick::new(0),
        &crate::TickInputs::default(),
        &mut revisions,
    );
    assert!(!world.lost_cargo_sites.contains_key(&LOCAL_PLAYER_ID));
    assert_eq!(
        world
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .cargo
            .get(&crate::terrain::MineralKind::Copper),
        Some(&retained_count.saturating_add(site.count()))
    );
    assert!(output.events.iter().any(|event| {
        event.kind
            == crate::GameplayEventKind::CargoChanged {
                player_id: LOCAL_PLAYER_ID,
            }
    }));
}

#[test]
fn canonical_survival_applies_hull_and_pump_heat_modifiers() {
    fn depth_damage(part: Option<crate::RigPartKind>) -> f32 {
        let game = WorldImport::new_game(LOCAL_PLAYER_ID);
        let mut world = WorldState::from_import(&game);
        if let Some(part) = part {
            world.set_player_movement_modifiers(LOCAL_PLAYER_ID, 0, [part]);
        }
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.y = 100.0 * TILE_SIZE;
        player.hull = 100.0;
        let mut revisions = crate::TerrainRevisionTracker::default();
        let _output = world.simulate_tick(
            SimulationTick::new(0),
            &crate::TickInputs::default(),
            &mut revisions,
        );
        100.0 - world.player(LOCAL_PLAYER_ID).expect("player exists").hull
    }
    assert!(depth_damage(Some(crate::RigPartKind::ThermalHull)) < depth_damage(None));
    assert!(depth_damage(Some(crate::RigPartKind::PressureHull)) < depth_damage(None));
    assert!(depth_damage(Some(crate::RigPartKind::ImpactHull)) > depth_damage(None));

    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut unprotected = WorldState::from_import(&game);
    let mut protected = WorldState::from_import(&game);
    for world in [&mut unprotected, &mut protected] {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = 20.0 * TILE_SIZE;
        player.y = 10.0 * TILE_SIZE;
        player.hull = 100.0;
        assert!(
            world
                .terrain_mut()
                .set_kind(TilePosition { x: 21, y: 10 }, TileKind::Lava)
        );
    }
    protected.infrastructure.push(crate::PlacedInfrastructure {
        kind: InfrastructureKind::PumpStation,
        position: TilePosition { x: 20, y: 10 },
        durability: 100,
    });
    for world in [&mut unprotected, &mut protected] {
        let mut revisions = crate::TerrainRevisionTracker::default();
        let _output = world.simulate_tick(
            SimulationTick::new(0),
            &crate::TickInputs::default(),
            &mut revisions,
        );
    }
    assert!(
        protected
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .hull
            > unprotected
                .player(LOCAL_PLAYER_ID)
                .expect("player exists")
                .hull
    );
}

#[test]
fn canonical_rescue_applies_fee_cargo_loss_and_partial_recovery() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
    player.y = 40.0 * TILE_SIZE;
    player.credits = 500;
    player.fuel = 0.0;
    player.hull = 0.0;
    player.cargo.insert(crate::terrain::MineralKind::Iron, 8);

    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::Rescue),
        crate::PlayerScopedCommandOutcome::Applied
    );
    let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
    assert_eq!(player.credits, 330);
    assert_eq!(
        player.cargo.get(&crate::terrain::MineralKind::Iron),
        Some(&4)
    );
    assert!(12.0f32.mul_add(-TILE_SIZE, player.x).abs() < f32::EPSILON);
    assert!(4.0f32.mul_add(-TILE_SIZE, player.y).abs() < f32::EPSILON);
    assert!(player.fuel_capacity.mul_add(-0.5, player.fuel).abs() < f32::EPSILON);
    assert!(player.max_hull().mul_add(-0.5, player.hull).abs() < f32::EPSILON);
    assert!(world.failure_state(LOCAL_PLAYER_ID).is_none());
}

#[test]
fn canonical_falling_boulders_use_tick_deadlines_and_damage_players() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut first = WorldState::from_import(&game);
    let mut second = first.clone();
    for world in [&mut first, &mut second] {
        let (x, y) = {
            let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
            player.x = 20.0 * TILE_SIZE;
            player.y = 10.0 * TILE_SIZE;
            player.hull = 100.0;
            (player.x, player.y)
        };
        world.spawn_falling_boulder(x, y, 1, 30);
    }

    let mut first_revisions = crate::TerrainRevisionTracker::default();
    let mut second_revisions = crate::TerrainRevisionTracker::default();
    let first_output = first.simulate_tick(
        SimulationTick::new(0),
        &crate::TickInputs::default(),
        &mut first_revisions,
    );
    let second_output = second.simulate_tick(
        SimulationTick::new(0),
        &crate::TickInputs::default(),
        &mut second_revisions,
    );

    assert!(first.player(LOCAL_PLAYER_ID).expect("player exists").hull < 100.0);
    assert!(first.falling_boulders.is_empty());
    assert_eq!(first_output, second_output);
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
    assert!(first_output.events.iter().any(|event| {
        event.kind
            == crate::GameplayEventKind::PlayerDamaged {
                player_id: LOCAL_PLAYER_ID,
            }
    }));
}

#[test]
fn canonical_explosive_pocket_schedules_deterministic_cave_in() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
    player.x = 20.0 * TILE_SIZE;
    player.y = 10.0 * TILE_SIZE;
    let target = TilePosition { x: 20, y: 11 };
    assert!(
        world
            .terrain_mut()
            .set_kind(target, TileKind::ExplosivePocket)
    );
    world.set_active_drill(
        LOCAL_PLAYER_ID,
        Some(DrillState {
            target,
            direction: DrillDirection::Down,
            progress: 1.0,
            initial_durability: 1,
            seconds_per_chip: FIXED_DELTA_SECONDS,
            sound_timer: 0.0,
            dust_timer: 0.0,
        }),
    );
    let mut revisions = crate::TerrainRevisionTracker::default();
    let output = world.simulate_tick(
        SimulationTick::new(0),
        &crate::TickInputs::default(),
        &mut revisions,
    );

    assert_eq!(world.falling_boulders.len(), 3);
    assert_eq!(world.boulder_warning_deadlines.len(), 3);
    assert_eq!(world.boulder_expiry_ticks.len(), 3);
    assert!(
        output
            .events
            .iter()
            .any(|event| { event.kind == crate::GameplayEventKind::BouldersChanged })
    );
}

#[test]
fn canonical_drilling_applies_rig_hardness_and_pump_modifiers() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let target = TilePosition { x: 20, y: 101 };

    let mut baseline = WorldState::from_import(&game);
    let mut resonance = WorldState::from_import(&game);
    for world in [&mut baseline, &mut resonance] {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = 20.0 * TILE_SIZE;
        player.y = 100.0 * TILE_SIZE;
        player.drill_strength = 1;
        assert!(world.terrain_mut().set_kind(target, TileKind::Dirt));
    }
    resonance.set_player_movement_modifiers(
        LOCAL_PLAYER_ID,
        0,
        [crate::RigPartKind::ResonanceDrill],
    );
    let command = PlayerCommand::Movement {
        horizontal: 0.0,
        thrust: false,
        drill_down: true,
    };
    assert_eq!(
        baseline.apply_player_command(LOCAL_PLAYER_ID, &command),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        resonance.apply_player_command(LOCAL_PLAYER_ID, &command),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert!(
        resonance
            .active_drill(LOCAL_PLAYER_ID)
            .expect("drill exists")
            .seconds_per_chip
            < baseline
                .active_drill(LOCAL_PLAYER_ID)
                .expect("drill exists")
                .seconds_per_chip
    );

    let mut pumped = WorldState::from_import(&game);
    {
        let player = pumped.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = 20.0 * TILE_SIZE;
        player.y = 10.0 * TILE_SIZE;
    }
    let gas = TilePosition { x: 20, y: 11 };
    assert!(pumped.terrain_mut().set_kind(gas, TileKind::Gas));
    pumped.infrastructure.push(crate::PlacedInfrastructure {
        kind: InfrastructureKind::PumpStation,
        position: gas,
        durability: 100,
    });
    pumped.set_active_drill(
        LOCAL_PLAYER_ID,
        Some(DrillState {
            target: gas,
            direction: DrillDirection::Down,
            progress: 1.0,
            initial_durability: 1,
            seconds_per_chip: FIXED_DELTA_SECONDS,
            sound_timer: 0.0,
            dust_timer: 0.0,
        }),
    );
    let mut revisions = crate::TerrainRevisionTracker::default();
    let _output = pumped.simulate_tick(
        SimulationTick::new(1),
        &crate::TickInputs::default(),
        &mut revisions,
    );
    assert!(pumped.hazards().is_empty());
}

#[test]
fn canonical_drilling_collects_ore_artifacts_and_applies_hazard_outcomes() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = 20.0 * TILE_SIZE;
        player.y = 5.0 * TILE_SIZE;
        player.drill_strength = u8::MAX;
    }
    let target = TilePosition { x: 20, y: 6 };
    assert!(world.terrain_mut().set_tile_from_network(
        target,
        TileKind::Artifact(crate::terrain::ArtifactKind::Fossil),
        1,
    ));
    world.set_active_drill(
        LOCAL_PLAYER_ID,
        Some(DrillState {
            target,
            direction: DrillDirection::Down,
            progress: 1.0,
            initial_durability: 1,
            seconds_per_chip: FIXED_DELTA_SECONDS,
            sound_timer: 0.0,
            dust_timer: 0.0,
        }),
    );
    let mut terrain_revisions = crate::TerrainRevisionTracker::default();
    let output = world.simulate_tick(
        SimulationTick::new(5),
        &crate::TickInputs::default(),
        &mut terrain_revisions,
    );
    assert_eq!(
        world
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .artifacts
            .get(&crate::terrain::ArtifactKind::Fossil),
        Some(&1),
    );
    assert!(output.terrain_changed_tiles.contains(&target));

    assert!(world.terrain_mut().set_kind(target, TileKind::Gas));
    let fuel_before = world.player(LOCAL_PLAYER_ID).expect("player exists").fuel;
    world.set_active_drill(
        LOCAL_PLAYER_ID,
        Some(DrillState {
            target,
            direction: DrillDirection::Down,
            progress: 1.0,
            initial_durability: 1,
            seconds_per_chip: FIXED_DELTA_SECONDS,
            sound_timer: 0.0,
            dust_timer: 0.0,
        }),
    );
    let output = world.simulate_tick(
        SimulationTick::new(6),
        &crate::TickInputs::default(),
        &mut terrain_revisions,
    );
    assert!(world.player(LOCAL_PLAYER_ID).expect("player exists").fuel < fuel_before);
    assert!(!world.hazards().is_empty());
    assert!(
        output
            .events
            .iter()
            .any(|event| event.kind == crate::GameplayEventKind::HazardChanged)
    );
}

#[test]
fn canonical_movement_applies_rig_and_mechanic_modifiers() {
    fn run(parts: &[crate::RigPartKind], mechanic_level: u8) -> (f32, f32, f32) {
        let game = WorldImport::new_game(LOCAL_PLAYER_ID);
        let mut world = WorldState::from_import(&game);
        world.set_player_movement_modifiers(LOCAL_PLAYER_ID, mechanic_level, parts.iter().copied());
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.cargo_capacity = 10;
        player.cargo.insert(crate::terrain::MineralKind::Copper, 10);
        let fuel_before = player.fuel;
        let _step =
            world.step_authoritative_movement(LOCAL_PLAYER_ID, 1.0, true, FIXED_DELTA_SECONDS);
        let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
        (player.velocity_x, fuel_before - player.fuel, player.hull)
    }

    let baseline = run(&[], 0);
    let burst = run(&[crate::RigPartKind::BurstEngine], 0);
    let hauler = run(&[crate::RigPartKind::HaulerEngine], 0);
    assert!(burst.0 > baseline.0);
    assert!(burst.1 > baseline.1);
    assert!(hauler.0 > baseline.0);

    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut unprotected = WorldState::from_import(&game);
    let mut protected = WorldState::from_import(&game);
    protected.set_player_movement_modifiers(LOCAL_PLAYER_ID, 4, [crate::RigPartKind::ImpactHull]);
    for world in [&mut unprotected, &mut protected] {
        let target = TilePosition { x: 20, y: 6 };
        assert!(world.terrain_mut().set_kind(target, TileKind::Stone));
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = 20.0 * TILE_SIZE;
        player.y = 5.0 * TILE_SIZE;
        player.velocity_y = super::AUTHORITATIVE_MAX_FALL_SPEED;
        player.hull = 100.0;
        let _step =
            world.step_authoritative_movement(LOCAL_PLAYER_ID, 0.0, false, FIXED_DELTA_SECONDS);
    }
    assert!(
        protected
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .hull
            > unprotected
                .player(LOCAL_PLAYER_ID)
                .expect("player exists")
                .hull
    );
}

#[test]
fn canonical_collision_applies_landing_and_wall_damage() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    let target = TilePosition { x: 20, y: 6 };
    assert!(world.terrain_mut().set_kind(target, TileKind::Stone));
    {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = 20.0 * TILE_SIZE;
        player.y = 5.0 * TILE_SIZE;
        player.velocity_y = super::AUTHORITATIVE_MAX_FALL_SPEED;
        player.hull = 100.0;
    }
    let _step = world.step_authoritative_movement(LOCAL_PLAYER_ID, 0.0, false, FIXED_DELTA_SECONDS);
    let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
    assert!(player.velocity_y.abs() < f32::EPSILON);
    assert!(player.hull < 100.0);

    let wall = TilePosition { x: 21, y: 5 };
    assert!(world.terrain_mut().set_kind(wall, TileKind::Stone));
    {
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = 20.0f32.mul_add(TILE_SIZE, 20.0);
        player.y = 5.0 * TILE_SIZE;
        player.velocity_x = super::AUTHORITATIVE_SAFE_LANDING_SPEED;
        player.hull = 100.0;
    }
    let _step = world.step_authoritative_movement(LOCAL_PLAYER_ID, 1.0, false, FIXED_DELTA_SECONDS);
    let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
    assert!(player.velocity_x.abs() < f32::EPSILON);
    assert!(player.hull < 100.0);
}

#[test]
fn canonical_movement_burns_fuel_and_caps_fall_speed() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    let initial_fuel = world.player(LOCAL_PLAYER_ID).expect("player exists").fuel;
    let mut terrain_revisions = crate::TerrainRevisionTracker::default();
    let thrust = [SequencedPlayerCommand {
        player_id: LOCAL_PLAYER_ID,
        sequence: InputSequence::new(1),
        target_tick: SimulationTick::new(0),
        command: PlayerCommand::Movement {
            horizontal: 0.0,
            thrust: true,
            drill_down: false,
        },
    }];
    let _output = world.simulate_tick(
        SimulationTick::new(0),
        &crate::TickInputs::from_commands(&thrust),
        &mut terrain_revisions,
    );
    let expected_burn = super::AUTHORITATIVE_FUEL_BURN_PER_SECOND * FIXED_DELTA_SECONDS;
    assert!(
        (world.player(LOCAL_PLAYER_ID).expect("player exists").fuel
            - crate::quantize::quantize_simulation_scalar(initial_fuel - expected_burn))
        .abs()
            < f32::EPSILON
    );

    world
        .player_mut(LOCAL_PLAYER_ID)
        .expect("player exists")
        .velocity_y = super::AUTHORITATIVE_MAX_FALL_SPEED * 2.0;
    let idle = [SequencedPlayerCommand {
        player_id: LOCAL_PLAYER_ID,
        sequence: InputSequence::new(2),
        target_tick: SimulationTick::new(1),
        command: PlayerCommand::Movement {
            horizontal: 0.0,
            thrust: false,
            drill_down: false,
        },
    }];
    let _output = world.simulate_tick(
        SimulationTick::new(1),
        &crate::TickInputs::from_commands(&idle),
        &mut terrain_revisions,
    );
    assert!(
        world
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .velocity_y
            <= super::AUTHORITATIVE_MAX_FALL_SPEED
    );
}

#[test]
fn canonical_movement_is_quantized_and_tick_deterministic() {
    fn run() -> (crate::SimulationStateHash, crate::player::Player) {
        let game = WorldImport::new_game(LOCAL_PLAYER_ID);
        let mut world = WorldState::from_import(&game);
        let mut terrain_revisions = crate::TerrainRevisionTracker::default();
        for tick in 0..120 {
            let commands = [SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(tick),
                target_tick: SimulationTick::new(u64::from(tick)),
                command: PlayerCommand::Movement {
                    horizontal: 0.333_333_34,
                    thrust: tick < 30,
                    drill_down: false,
                },
            }];
            let inputs = crate::TickInputs::from_commands(&commands);
            let _output = world.simulate_tick(
                SimulationTick::new(u64::from(tick)),
                &inputs,
                &mut terrain_revisions,
            );
        }
        let player = world
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .clone();
        (world.simulation_state_hash(), player)
    }

    let (first_hash, first_player) = run();
    let (second_hash, second_player) = run();

    assert_eq!(first_hash, second_hash);
    assert_eq!(first_player, second_player);
    for value in [
        first_player.x,
        first_player.y,
        first_player.velocity_x,
        first_player.velocity_y,
        first_player.fuel,
    ] {
        let subpixels = value * crate::quantize::SIMULATION_SUBPIXELS_PER_PIXEL;
        assert!((subpixels - subpixels.round()).abs() < f32::EPSILON);
    }
}

#[test]
fn canonical_tick_advances_bombs_and_infrastructure() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
    player.bombs = 1;
    player.signal_relay_kits = 1;
    let tick = SimulationTick::new(3);
    let commands = vec![
        SequencedPlayerCommand {
            player_id: LOCAL_PLAYER_ID,
            sequence: InputSequence::new(1),
            target_tick: tick,
            command: PlayerCommand::PlaceBomb,
        },
        SequencedPlayerCommand {
            player_id: LOCAL_PLAYER_ID,
            sequence: InputSequence::new(2),
            target_tick: tick,
            command: PlayerCommand::PlaceInfrastructure { slot: 0 },
        },
    ];
    let inputs = crate::TickInputs::from_commands(&commands);
    let mut terrain_revisions = crate::TerrainRevisionTracker::default();
    let _output = world.simulate_tick(tick, &inputs, &mut terrain_revisions);

    assert_eq!(world.bomb_count(), 1);
    assert_eq!(world.infrastructure_count(), 1);
    let target = TilePosition { x: 10, y: 10 };
    assert!(world.terrain_mut().set_kind(target, TileKind::Dirt));
    world.bombs[0].x = 10.0 * TILE_SIZE;
    world.bombs[0].y = 10.0 * TILE_SIZE;
    world.bombs[0].timer_seconds = FIXED_DELTA_SECONDS;
    world.bomb_detonation_ticks[0] = tick.next().next();

    let output = world.simulate_tick(
        tick.next(),
        &crate::TickInputs::default(),
        &mut terrain_revisions,
    );

    assert_eq!(world.bomb_count(), 0);
    assert_eq!(world.infrastructure_count(), 1);
    assert!(output.terrain_changed_tiles.contains(&target));
    assert!(!output.terrain_revisions.is_empty());
}

#[test]
fn canonical_tick_advances_scanner_and_hazard_state() {
    let game = WorldImport::new_game(LOCAL_PLAYER_ID);
    let mut world = WorldState::from_import(&game);
    let (player_x, player_y, hull_before) = {
        let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
        (player.x, player.y, player.hull)
    };
    world.set_scanner_cooldown_seconds(LOCAL_PLAYER_ID, 1.0);
    world.hazards.push(crate::HazardCloud {
        x: player_x,
        y: player_y,
        life: 1.0,
        radius: 20.0,
    });
    let mut terrain_revisions = crate::TerrainRevisionTracker::default();

    let output = world.simulate_tick(
        SimulationTick::new(4),
        &crate::TickInputs::default(),
        &mut terrain_revisions,
    );

    assert!(
        world
            .scanner_cooldown_seconds(LOCAL_PLAYER_ID)
            .expect("cooldown exists")
            < 1.0
    );
    assert!(world.player(LOCAL_PLAYER_ID).expect("player exists").hull < hull_before);
    assert!(output.events.iter().any(|event| {
        event.kind
            == crate::GameplayEventKind::PlayerDamaged {
                player_id: LOCAL_PLAYER_ID,
            }
    }));
}

#[test]
fn authoritative_world_handles_economy_progression_debt_and_victory_transactions() {
    let mut world = WorldState::new_game(LOCAL_PLAYER_ID);
    let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
    player.credits = 10_000;
    player.loan_debt = 500;
    let credits_before_upgrade = player.credits;

    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::BuyUpgrade { index: 0 }),
        crate::PlayerScopedCommandOutcome::Applied
    );
    let upgraded_player = world.player(LOCAL_PLAYER_ID).expect("player exists");
    assert!(upgraded_player.drill_strength > 1);
    assert!(upgraded_player.credits < credits_before_upgrade);
    assert!(world.apply_contract_reward(LOCAL_PLAYER_ID, 250).is_some());
    assert!(world.start_expedition(LOCAL_PLAYER_ID, 100).is_some());
    assert!(world.repay_debt(LOCAL_PLAYER_ID, 200).is_some());
    assert!(world.award_victory(LOCAL_PLAYER_ID, 1_000).is_some());

    let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
    assert_eq!(player.loan_debt, 300);
    assert!(world.won_game());
    assert!(
        world
            .service_transactions()
            .iter()
            .any(|transaction| transaction.kind == crate::PlayerTransactionKind::CompleteContract)
    );
    assert!(
        world
            .service_transactions()
            .iter()
            .any(|transaction| transaction.kind == crate::PlayerTransactionKind::StartExpedition)
    );
    assert!(
        world
            .service_transactions()
            .iter()
            .any(|transaction| transaction.kind == crate::PlayerTransactionKind::RepayDebt)
    );
    assert!(
        world
            .service_transactions()
            .iter()
            .any(|transaction| transaction.kind == crate::PlayerTransactionKind::WinGame)
    );
}

#[test]
fn authoritative_world_handles_hazard_damage_failure_scanner_and_bomb_effects() {
    let mut world = WorldState::new_game(LOCAL_PLAYER_ID);
    let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
    player.hull = 5.0;
    player.fuel = 0.0;
    player.bombs = 1;
    let scanner_target = player.tile_position(TILE_SIZE);
    let bomb_target = TilePosition { x: 3, y: 6 };
    assert!(world.terrain.set_kind(bomb_target, TileKind::Dirt));

    let survival = world
        .apply_hazard_damage(LOCAL_PLAYER_ID, 6.0)
        .expect("survival snapshot");
    assert!(survival.disabled);
    assert!(survival.stranded);
    assert!(world.failure_state(LOCAL_PLAYER_ID).is_some());
    assert_eq!(world.discovered_tile_count(LOCAL_PLAYER_ID), 0);
    assert!(world.reveal_scanner_area(LOCAL_PLAYER_ID, scanner_target, 2) > 0);
    assert!(world.discovered_tile_count(LOCAL_PLAYER_ID) > 0);
    assert_eq!(
        world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::PlaceBomb),
        crate::PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.bomb_count(), 1);
    let blast = world.detonate_bomb_at(bomb_target, 1);
    assert!(blast.cleared > 0);
    assert_eq!(
        world.terrain().tile(bomb_target).expect("tile").kind,
        TileKind::Air
    );
}

#[test]
fn terrain_revision_tracker_coalesces_changed_tiles_by_chunk() {
    let mut tracker = crate::TerrainRevisionTracker::default();
    let revisions = tracker.mark_tiles_changed([
        crate::terrain::TilePosition { x: 0, y: 0 },
        crate::terrain::TilePosition { x: 3, y: 4 },
        crate::terrain::TilePosition { x: 17, y: 0 },
    ]);

    assert_eq!(revisions.len(), 2);
    assert_eq!(
        tracker.revision(crate::TerrainChunkPosition { x: 0, y: 0 }),
        1
    );
    assert_eq!(
        tracker.revision(crate::TerrainChunkPosition { x: 1, y: 0 }),
        1
    );
}
