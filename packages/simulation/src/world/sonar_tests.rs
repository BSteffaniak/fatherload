//! The Sonar Suite's one API (Wave 9 integration, `world/sonar.rs`): the scanner pings the
//! Drowned and Frozen's water and the Echo Caves' dark through the same hook, both read one
//! light, and the Leviathan's Sonar Suite reaches further in both.

use super::SonarMedium;
use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WorldState,
    depth_progression::RigSystem,
    descent::Stratum,
    drowned::{SONAR_REACH_TILES, SONAR_SUITE_REACH_TILES},
    enemy::EnemyDifficulty,
    far_strange::{SONAR_PING_TILES, SONAR_SUITE_PING_TILES},
    fluid::FluidKind,
    position::WorldPos,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{GeneratorVersion, TileRect, WorldSettings, descent::layout},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x50_7A12;
/// A column far from every set piece (the test rooms are built by hand).
const FAR: i32 = -300;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// A Deep Descent world with the rig fitted for Descent VI but without the Sonar Suite, the
/// earlier gates' rewards granted.
fn world() -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(30));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hazard_gear.rig_systems &= !(RigSystem::SonarSuite.bit() | RigSystem::PhaseDrill.bit());
    rig.radiator_level = 15;
    rig.hull_level = 15;
    rig.hull = rig.max_hull();
    world
}

fn middle_row(stratum: Stratum) -> i32 {
    let (top, bottom) = layout(&settings()).rows(stratum);
    top + (bottom - top) / 2
}

fn put_rig(world: &mut WorldState, tile: TilePosition) {
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(tile.x), i64::from(tile.y)),
    );
    world.stream_terrain_near_players();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn ticks(world: &mut WorldState, count: u64) {
    for _ in 0..count {
        let inputs = TickInputs::held(PLAYER, ControlState::default());
        let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    }
}

fn scan(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::default().act(PLAYER, PlayerCommand::UseScanner);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    output.events.into_iter().map(|event| event.kind).collect()
}

/// A sealed stone room round `centre` (half extents `half`), open inside, `fluid` filling it.
fn room(world: &mut WorldState, centre: TilePosition, half: (i32, i32), fluid: Option<FluidKind>) {
    put_rig(world, centre);
    ticks(world, 2);
    for position in TileRect::around(centre, half.0 + 2, half.1 + 2).positions() {
        let _ = world.terrain_mut().set_kind(position, TileKind::Stone);
    }
    let inside = fluid.map_or(TileKind::Air, TileKind::Fluid);
    for position in TileRect::around(centre, half.0, half.1).positions() {
        let _ = world.terrain_mut().set_kind(position, inside);
    }
    put_rig(world, centre);
}

fn grant_suite(world: &mut WorldState) {
    crate::economy::grant_rig_system(
        world.player_mut(PLAYER).expect("rig"),
        RigSystem::SonarSuite,
    );
}

#[test]
fn the_scanner_pings_the_waters_sonar_through_the_one_hook_and_lights_one_light() {
    let mut world = world();
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::UndergroundOcean),
    };
    room(&mut world, centre, (10, 6), Some(FluidKind::Water));
    assert_eq!(world.sonar_medium(PLAYER), Some(SonarMedium::Water));
    assert_eq!(
        world.sonar_reach_tiles(PLAYER, SonarMedium::Water),
        SONAR_REACH_TILES
    );
    let events = scan(&mut world);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::Drowned {
            what: crate::drowned::DrownedEvent::SonarPing,
            ..
        }
    )));
    assert_eq!(world.sonar_pings().len(), 1);
    assert!(
        world.far_strange().pings.is_empty(),
        "no Echo Caves wave here"
    );
    ticks(&mut world, 15);
    let (x, y) = {
        let rig = world.player(PLAYER).expect("rig");
        (rig.x, rig.y)
    };
    let lit = world.sonar_light_at(TILE_SIZE.mul_add(5.0, x), y);
    assert!(lit > 0.0, "the water's ring lights the unified light");
    assert!((lit - world.drowned_sonar_light_at(TILE_SIZE.mul_add(5.0, x), y)).abs() < 1e-6);
    grant_suite(&mut world);
    assert_eq!(
        world.sonar_reach_tiles(PLAYER, SonarMedium::Water),
        SONAR_SUITE_REACH_TILES
    );
}

#[test]
fn the_scanner_pings_the_echo_caves_through_the_same_hook_and_the_suite_reaches_further() {
    let mut world = world();
    let centre = TilePosition {
        x: 260,
        y: middle_row(Stratum::EchoCaves),
    };
    room(&mut world, centre, (20, 4), None);
    assert_eq!(world.sonar_medium(PLAYER), Some(SonarMedium::EchoCaves));
    assert_eq!(
        world.sonar_reach_tiles(PLAYER, SonarMedium::EchoCaves),
        i32::from(SONAR_PING_TILES)
    );
    let _ = scan(&mut world);
    assert_eq!(
        world.far_strange().pings.len(),
        1,
        "one wave, through one hook"
    );
    assert!(
        world.sonar_pings().is_empty(),
        "no water ping in the dry dark"
    );
    ticks(&mut world, 9);
    let near = TilePosition {
        x: centre.x + 6,
        y: centre.y,
    };
    assert!(
        world.sonar_light_at_tile(near) > 0.4,
        "the wave lights the unified light"
    );
    // The Sonar Suite: the next wave reaches further.
    ticks(&mut world, 30 * 6);
    grant_suite(&mut world);
    assert_eq!(
        world.sonar_reach_tiles(PLAYER, SonarMedium::EchoCaves),
        i32::from(SONAR_SUITE_PING_TILES)
    );
    let _ = scan(&mut world);
    assert_eq!(
        world.far_strange().pings.last().map(|ping| ping.reach),
        Some(SONAR_SUITE_PING_TILES)
    );
}

#[test]
fn dry_rock_outside_the_echo_caves_has_no_sonar() {
    let mut world = world();
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::MegafaunaJungle),
    };
    room(&mut world, centre, (6, 4), None);
    assert_eq!(world.sonar_medium(PLAYER), None);
    let mut output = crate::event::TickOutput::default();
    assert!(!world.sonar_ping(PLAYER, &mut output));
    let _ = scan(&mut world);
    assert_eq!(world.sonar_pings(), []);
    assert_eq!(world.far_strange().pings, []);
}

#[test]
fn an_echo_charge_from_the_kit_pings_as_far_as_the_sonar_suite() {
    use crate::workshop::DeepSupply;
    let mut world = world();
    let centre = TilePosition {
        x: 260,
        y: middle_row(Stratum::EchoCaves),
    };
    room(&mut world, centre, (20, 4), None);
    let _ = world
        .logistics
        .add_supplies(PLAYER, DeepSupply::EchoCharge, 1);
    let _ = scan(&mut world);
    assert_eq!(
        world.far_strange().pings.last().map(|ping| ping.reach),
        Some(SONAR_SUITE_PING_TILES),
        "the charge's long ping"
    );
    assert_eq!(
        world.supplies_of(PLAYER, DeepSupply::EchoCharge),
        0,
        "spent"
    );
    // The kit is empty: the next ping is the stock one.
    ticks(&mut world, 30 * 6);
    let _ = scan(&mut world);
    assert_eq!(
        world.far_strange().pings.last().map(|ping| ping.reach),
        Some(SONAR_PING_TILES)
    );
    // The Sonar Suite never spends a charge.
    let _ = world
        .logistics
        .add_supplies(PLAYER, DeepSupply::EchoCharge, 1);
    grant_suite(&mut world);
    ticks(&mut world, 30 * 6);
    let _ = scan(&mut world);
    assert_eq!(world.supplies_of(PLAYER, DeepSupply::EchoCharge), 1);
}
