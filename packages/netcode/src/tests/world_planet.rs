//! Online play on a round body (Wave 11 polar track): a radius-400 moon whose strip wraps,
//! the bots spawned just short of the seam so they drive, dig and land across it while the
//! host relabels its frame by whole laps. Every client's authority must match the host.

use std::time::Duration;

use fatherload_simulation::{
    PlayerId as SimPlayerId, SimulationParams, WorldState,
    enemy::EnemyDifficulty,
    planet::{BodyGround, PlanetParams, PlanetThemeId},
    player::Player,
    position::{Frame, SECTOR_TILES, WorldPos},
};

use super::support::{assert_bounded, assert_converged};
use crate::harness::{Harness, HarnessConfig};
use crate::{LinkConditions, SessionParams};

/// A moon world with no players yet: the spawn template a few tiles short of the strip's seam.
fn moon_world(rate: u32) -> WorldState {
    let first = SimPlayerId::new(1);
    let params = PlanetParams::new(0x4D00_4E00, 400, PlanetThemeId::BARREN);
    let mut world = WorldState::new_planet_world(first, params, SimulationParams::new(rate));
    world.remove_player(first);
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let planet = *world.planet().expect("planet");
    let width = planet.strip_width();
    let column = width - 12;
    let row = planet.surface_row(column) - 2;
    // The frame on the seam's last sector, so the spawn is near its origin.
    world.relocate_frame(Frame::floating_at(planet.grid().lap_sectors() - 1, 0));
    let (x, y) = world.local_pos(WorldPos::tile_center(i64::from(column), i64::from(row)));
    let mut template = Player::new(x, y);
    template.credits = 600;
    world.set_player_template(template);
    world
}

#[test]
fn a_moon_replicates_and_predicts_exactly_across_its_seam() {
    let rate = 60;
    let mut config = HarnessConfig::new(
        SessionParams::new(rate),
        0,
        // 120 ms +/- 20 ms jitter with 2 % loss (plus duplication and reordering).
        LinkConditions::new(120, 20, 2.0).with_chaos(2.0, 5.0),
    );
    config.seed = 0x0400;
    let mut harness: Harness<WorldState> = Harness::new(config);
    harness.restart_host(moon_world(rate));
    for _ in 0..3 {
        let _ = harness.add_client(None);
    }
    harness.run_ticks(u64::from(rate) * 20);
    harness.settle(Duration::from_secs(2));
    assert!(harness.all_in_game());
    assert_converged(&harness);
    assert_bounded(&harness);
    harness.assert_actions_exactly_once();
    let host = harness.world();
    let planet = *host.planet().expect("planet");
    // The frame stays labelled within one lap of the strip.
    assert!((0..planet.grid().lap_sectors()).contains(&host.frame().sector_x));
    assert_eq!(
        SECTOR_TILES * planet.grid().lap_sectors(),
        i64::from(planet.strip_width())
    );
    assert!(
        !host.field_zones().is_empty(),
        "the moon's gravity replicates"
    );
}
