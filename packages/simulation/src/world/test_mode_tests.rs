//! Test mode: the maxed starting loadout, the toggles, the warp and the records it skips.

use crate::{
    ControlState, PlayerCommand, PlayerId, PlayerScopedCommandOutcome, SectionKey, TickInputs,
    TickOutput, WorldState,
    economy::{MAX_DRILL_MOUNT_LEVEL, MAX_UPGRADE_LEVEL},
    rig::RigPartKind,
    terrain::{DeepStratum, TILE_SIZE, TileKind, TilePosition},
    test_mode::{TEST_MODE_CREDITS, TestModeAction},
    worldgen::Band,
};

const PLAYER: PlayerId = PlayerId::new(1);
const GUEST: PlayerId = PlayerId::new(2);

fn test_world() -> WorldState {
    let mut world = WorldState::new_game(PLAYER);
    world.enable_test_mode();
    world
}

fn act(world: &mut WorldState, player: PlayerId, action: TestModeAction) -> TickOutput {
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(player, PlayerCommand::TestMode(action));
    world.simulate_tick(tick, &inputs)
}

fn idle(world: &mut WorldState, ticks: u32) {
    let inputs = TickInputs::held(PLAYER, ControlState::default());
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
    }
}

#[test]
fn test_mode_starts_every_rig_maxed_with_every_part() {
    let mut world = test_world();
    world.add_player(GUEST);
    for player_id in [PLAYER, GUEST] {
        let player = world.player(player_id).expect("player");
        for level in [
            player.drill_strength,
            player.engine_level,
            player.fuel_tank_level,
            player.cargo_bay_level,
            player.hull_level,
            player.radiator_level,
            player.scanner_level,
        ] {
            assert_eq!(level, MAX_UPGRADE_LEVEL);
        }
        assert_eq!(player.drill_mount_level, MAX_DRILL_MOUNT_LEVEL);
        assert!(player.credits >= TEST_MODE_CREDITS);
        assert!((player.fuel - player.fuel_capacity).abs() < f32::EPSILON);
        assert!((player.hull - player.max_hull()).abs() < f32::EPSILON);
    }
    assert!(
        RigPartKind::ALL
            .iter()
            .all(|part| world.progression().rig_part_inventory.contains(part))
    );
}

#[test]
fn test_mode_actions_are_ignored_in_a_normal_world() {
    let mut world = WorldState::new_game(PLAYER);
    let output = act(&mut world, PLAYER, TestModeAction::SetInvulnerable(true));
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
    assert!(world.test_mode().is_none());
    let output = act(
        &mut world,
        PLAYER,
        TestModeAction::WarpToBand(Band::Deep(DeepStratum::MantleStormZone)),
    );
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
    assert!(output.terrain_ops.is_empty());
}

#[test]
fn toggles_refill_hull_and_fuel_every_tick() {
    let mut world = test_world();
    let output = act(&mut world, PLAYER, TestModeAction::SetInvulnerable(true));
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::Applied
    );
    let _ = act(&mut world, PLAYER, TestModeAction::SetInfiniteFuel(true));
    let state = world.test_mode().expect("test mode");
    assert!(state.invulnerable && state.infinite_fuel);
    {
        let player = world.player_mut(PLAYER).expect("player");
        player.hull = 1.0;
        player.fuel = 0.0;
    }
    idle(&mut world, 1);
    let player = world.player(PLAYER).expect("player");
    assert!((player.hull - player.max_hull()).abs() < f32::EPSILON);
    assert!((player.fuel - player.fuel_capacity).abs() < f32::EPSILON);
    assert!(world.failure_state(PLAYER).is_none());

    let _ = act(&mut world, PLAYER, TestModeAction::SetInvulnerable(false));
    world.player_mut(PLAYER).expect("player").hull = 1.0;
    idle(&mut world, 1);
    assert!(world.player(PLAYER).expect("player").hull < 2.0);
}

#[test]
fn warping_lands_the_rig_in_a_safe_pocket_at_the_top_of_every_band() {
    for band in Band::ALL {
        let mut world = test_world();
        let _ = act(&mut world, PLAYER, TestModeAction::SetInvulnerable(true));
        let output = act(&mut world, PLAYER, TestModeAction::WarpToBand(band));
        assert_eq!(
            output.action_outcomes[0].outcome,
            PlayerScopedCommandOutcome::Applied,
            "{band:?}"
        );
        let tile = world
            .player(PLAYER)
            .expect("player")
            .tile_position(TILE_SIZE);
        if band != Band::Topsoil {
            assert_eq!(world.terrain().band_at(tile), band, "{band:?} lands in it");
            assert!(!output.terrain_ops.is_empty(), "{band:?} carved a pocket");
        }
        // The pocket holds the rig: it settles without falling through or getting stuck.
        idle(&mut world, 120);
        let settled = world
            .player(PLAYER)
            .expect("player")
            .tile_position(TILE_SIZE);
        assert_eq!(settled.x, tile.x, "{band:?}");
        assert!(
            (settled.y - tile.y).abs() <= 1,
            "{band:?}: {tile:?} -> {settled:?}"
        );
        let below = TilePosition {
            x: settled.x,
            y: settled.y + 1,
        };
        assert!(world.terrain().is_solid_at(below), "{band:?} has a floor");
        for dy in -1..=1 {
            for dx in -2..=2 {
                let near = TilePosition {
                    x: settled.x + dx,
                    y: settled.y + dy,
                };
                assert!(
                    !matches!(
                        world.terrain().tile(near).map(|tile| tile.kind),
                        Some(TileKind::Lava | TileKind::Gas | TileKind::MagmaVent)
                    ),
                    "{band:?}: hazard next to the pocket"
                );
            }
        }
    }
}

#[test]
fn depth_records_and_milestones_do_not_count_in_test_mode() {
    let mut world = test_world();
    let _ = act(&mut world, PLAYER, TestModeAction::SetInvulnerable(true));
    let _ = act(
        &mut world,
        PLAYER,
        TestModeAction::WarpToBand(Band::Deep(DeepStratum::FossilOceans)),
    );
    idle(&mut world, 30);
    assert_eq!(world.progression().deepest_tile_reached, 0);
    assert!(
        world
            .progression()
            .deep_reward_milestones
            .values()
            .all(|milestone| *milestone == 0)
    );
}

#[test]
fn test_mode_replicates_as_a_shared_section_and_round_trips_through_serde() {
    let mut authority = test_world();
    let _ = act(
        &mut authority,
        PLAYER,
        TestModeAction::SetInfiniteFuel(true),
    );
    let mut replica = WorldState::new_game(PLAYER);
    assert_ne!(
        replica.section_hash(SectionKey::TestMode),
        authority.section_hash(SectionKey::TestMode)
    );
    let payload = authority.section(SectionKey::TestMode).expect("section");
    replica.apply_section(SectionKey::TestMode, payload);
    assert_eq!(replica.test_mode(), authority.test_mode());

    let json = serde_json::to_string(&authority).expect("serialize");
    let restored: WorldState = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.test_mode(), authority.test_mode());
    assert_eq!(
        restored.simulation_state_hash(),
        authority.simulation_state_hash()
    );
}

#[test]
fn warps_land_safely_on_generated_worlds_of_any_seed_and_size() {
    use crate::{
        SimulationParams,
        worldgen::{HazardIntensity, ResourceRichness, WorldSettings, WorldSize},
    };
    for (seed, size) in [
        (3, WorldSize::Standard),
        (77, WorldSize::Large),
        (90_210, WorldSize::Standard),
    ] {
        let settings = WorldSettings {
            size,
            richness: ResourceRichness::Rich,
            hazards: HazardIntensity::Brutal,
            ..WorldSettings::new(seed)
        };
        let mut world =
            WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
        world.enable_test_mode();
        let _ = act(&mut world, PLAYER, TestModeAction::SetInvulnerable(true));
        // Warp down through every band and back up, from wherever the last warp left the rig.
        for band in Band::ALL
            .into_iter()
            .skip(1)
            .chain(Band::ALL.into_iter().rev())
        {
            let output = act(&mut world, PLAYER, TestModeAction::WarpToBand(band));
            assert_eq!(
                output.action_outcomes[0].outcome,
                PlayerScopedCommandOutcome::Applied,
                "seed {seed}: {band:?}"
            );
            idle(&mut world, 60);
            let rig = world.player(PLAYER).expect("player");
            let tile = rig.tile_position(TILE_SIZE);
            if band != Band::Topsoil {
                assert_eq!(
                    world.terrain().band_at(tile),
                    band,
                    "seed {seed}: settles in {band:?}"
                );
                assert!(
                    world.terrain().is_solid_at(TilePosition {
                        x: tile.x,
                        y: tile.y + 1
                    }),
                    "seed {seed}: {band:?} pocket has a floor"
                );
            }
            assert!(rig.velocity_y.abs() < 1.0, "seed {seed}: {band:?} at rest");
        }
    }
}
