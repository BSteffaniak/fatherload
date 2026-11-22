//! Online play far from the origin (expansion plan §15 F7): a floating-frame world whose rigs
//! dig and fly 10^9 tiles out, over a lossy link, re-centring its frame while clients predict.
//! Every client's authority must match the host at every compared frame (hash checks), with
//! the frame move replicated first in the tick's live section.

use std::time::Duration;

use fatherload_simulation::{
    PlayerId as SimPlayerId, SimulationParams, WorldState,
    enemy::EnemyDifficulty,
    player::Player,
    position::{Frame, SECTOR_TILES, WorldPos},
    terrain::{MineralKind, TileKind, TilePosition},
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::support::{assert_bounded, assert_converged};
use crate::harness::{Harness, HarnessConfig, HarnessSim};
use crate::world::frame_origin;
use crate::{LinkConditions, NetSimulation, SessionParams};

/// The far arena's sector: column 10^9 is its left edge.
const FAR_SECTOR: i64 = 1_953_125;

/// A V5 world with a floating frame, a stone-walled arena at column 10^9 (a copper seam in
/// its floor for the bots' drills) and the spawn inside it. The frame starts one sector west,
/// so the first tick with a rig moves it.
fn far_world(rate: u32) -> WorldState {
    let first = SimPlayerId::new(1);
    let settings = WorldSettings {
        generator: GeneratorVersion::V5,
        ..WorldSettings::new(0x0F7F_0001)
    };
    let mut world =
        WorldState::new_game_with_settings(first, &settings, SimulationParams::new(rate));
    world.remove_player(first);
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world
        .terrain_mut()
        .set_column_limit_for_tests(2_100_000_000);
    world.set_floating_frame(true);
    world.relocate_frame(Frame::floating_at(FAR_SECTOR - 1, 0));
    let column = i32::try_from(FAR_SECTOR * SECTOR_TILES).expect("inside the i32 grid");
    for y in 6..=28 {
        for x in column - 60..=column + 60 {
            let kind = match y {
                10..=19 if (column - 58..=column + 58).contains(&x) => TileKind::Air,
                21 | 22 if (column - 58..=column + 58).contains(&x) => {
                    TileKind::Ore(MineralKind::Copper)
                }
                _ => TileKind::Stone,
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    let (x, y) = world.local_pos(WorldPos::tile_center(i64::from(column) + 3, 18));
    let mut template = Player::new(x, y);
    template.credits = 600;
    template.bombs = 6;
    world.set_player_template(template);
    world
}

#[test]
fn world_far_from_the_origin_replicates_and_predicts_exactly() {
    let rate = 60;
    let mut config = HarnessConfig::new(
        SessionParams::new(rate),
        0,
        // 150 ms +/- 20 ms jitter with 3 % loss (plus duplication and reordering).
        LinkConditions::new(150, 20, 3.0).with_chaos(2.0, 5.0),
    );
    config.seed = 0x0F7;
    let mut harness: Harness<WorldState> = Harness::new(config);
    harness.restart_host(far_world(rate));
    for _ in 0..3 {
        let _ = harness.add_client(None);
    }
    harness.run_ticks(u64::from(rate) * 25);
    harness.settle(Duration::from_secs(2));
    assert!(harness.all_in_game());
    let host = harness.world();
    assert_eq!(
        host.frame().sector_x,
        FAR_SECTOR,
        "the frame moved onto the arena's sector"
    );
    assert_converged(&harness);
    assert_bounded(&harness);
    harness.assert_actions_exactly_once();
    // Every rig is out there, 10^9 tiles east, and a client sees the same frame and places.
    let origin = frame_origin(host);
    for id in NetSimulation::player_ids(host) {
        let pose = host.player_pose(id).expect("pose");
        assert_eq!(pose.origin, origin);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a pose a few tiles from its origin"
        )]
        let column = pose.origin[0] + pose.x.floor() as i64;
        assert!(
            (FAR_SECTOR * SECTOR_TILES - 60..FAR_SECTOR * SECTOR_TILES + 60).contains(&column),
            "{column}"
        );
    }
    assert!(
        host.play_progress() > far_world(rate).play_progress(),
        "the bots dug far out"
    );
}
