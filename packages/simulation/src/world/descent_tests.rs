//! Deep Descent worlds (generator V11) in play: a cheating bot drills from the surface to the
//! bedrock floor, the world saves and loads (compactly), and creatures follow the registry.

use crate::{
    ControlState, EnemyDifficulty, PlayerId, TickInputs, WorldState,
    descent::{Descent, Stratum},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{
        GeneratorVersion, WorldSettings,
        descent::{floor_row, layout},
        strata::bedrock_top_row,
    },
};

const PLAYER: PlayerId = PlayerId::new(1);

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

fn deep_world(seed: u64) -> WorldState {
    WorldState::new_game_with_settings(PLAYER, &v11(seed), crate::SimulationParams::default())
}

#[test]
fn a_v11_world_starts_like_any_other() {
    let world = deep_world(0xD1_11_6A_4E);
    assert_eq!(world.world_settings().generator, GeneratorVersion::V11);
    assert_eq!(world.terrain().height(), v11(0).height_tiles());
    assert!(world.terrain().height() > 13_000);
    let rig = world.player(PLAYER).expect("a rig");
    let tile = rig.tile_in(world.frame());
    // The rig spawns on the surface, in the Topsoil, as in every world.
    assert!(tile.y <= crate::worldgen::strata::SKY_ROWS + 1, "{tile:?}");
    let below = TilePosition { x: tile.x, y: 12 };
    assert_eq!(world.terrain().stratum_at(below), Stratum::Topsoil);
    assert_eq!(world.terrain().layer_name(below), "Topsoil");
    let deep = TilePosition {
        x: tile.x,
        y: floor_row(&v11(0xD1_11_6A_4E)) - 2,
    };
    assert_eq!(world.terrain().stratum_at(deep), Stratum::TheHeart);
    assert_eq!(world.terrain().layer_name(deep), "The Heart");
}

#[test]
fn deep_strata_spawn_their_registry_creatures() {
    let settings = v11(4);
    let world =
        WorldState::new_game_with_settings(PLAYER, &settings, crate::SimulationParams::default());
    let layout = layout(&settings);
    for stratum in [
        Stratum::FungalForest,
        Stratum::TheSunforge,
        Stratum::BuriedMegacity,
    ] {
        let (top, bottom) = layout.rows(stratum);
        let tile = TilePosition {
            x: 40,
            y: i32::midpoint(top, bottom),
        };
        assert_eq!(world.terrain().stratum_at(tile), stratum);
        assert_eq!(
            world.terrain().creature_roster(tile),
            stratum.def().creatures
        );
    }
    // Older worlds keep their band rosters.
    let classic = WorldState::new_game(PLAYER);
    let tile = TilePosition { x: 40, y: 300 };
    assert_eq!(
        classic.terrain().creature_roster(tile),
        crate::enemy::band_roster(classic.terrain().band_at(tile))
    );
}

/// A test-mode rig (invulnerable, infinite fuel, instant drilling) holding "drill down" from
/// the surface reaches the bedrock floor of a V11 world: every stratum, the Hollow World's
/// drop and every gate chamber included.
#[test]
fn a_cheating_bot_drills_to_the_bottom_of_the_deep_descent() {
    let settings = v11(0x00C0_FFEE);
    // A low tick rate keeps the run short. 20 Hz is below the 30-240 Hz range the tick-rate
    // tests cover: test-mode drilling already stalls at 10 Hz in V10 worlds too (a rig holding
    // "drill down" from the spawn never gets past row 5), so this is not a V11 matter.
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, crate::SimulationParams::new(20));
    world.enable_test_mode();
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    // Centre the rig over its column, so it fits the one-tile shaft it drills.
    let frame = world.frame();
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.x = frame.centre_of(frame.col(rig.x), 0).0;
    }
    let start = world.player(PLAYER).expect("a rig").tile_in(world.frame());
    let floor = bedrock_top_row(&settings, start.x).expect("a floor");
    let controls = ControlState::new(0.0, false, true);
    let inputs = TickInputs::held(PLAYER, controls);
    let mut descents_seen = std::collections::BTreeSet::new();
    let mut deepest = start.y;
    let mut stalled = 0;
    for tick in 0..300_000_u32 {
        let now = world.simulation_tick();
        world.simulate_tick(now, &inputs);
        if tick % 30 == 0 {
            world.stream_terrain_near_players();
            // The floating frame re-centres as the rig goes down (F7): read through it.
            let tile = world.player(PLAYER).expect("a rig").tile_in(world.frame());
            descents_seen.insert(world.terrain().stratum_at(tile).descent());
            if tile.y > deepest {
                deepest = tile.y;
                stalled = 0;
            } else {
                stalled += 1;
                // Nudge sideways off anything undiggable (an outpost's plating).
                if stalled > 40 {
                    let rig = world.player_mut(PLAYER).expect("a rig");
                    rig.x = TILE_SIZE.mul_add(3.0, rig.x);
                    stalled = 0;
                }
            }
            if tile.y >= floor - 1 {
                println!("the bot reached the floor after {tick} ticks at 20 Hz");
                break;
            }
        }
    }
    assert!(
        deepest >= floor - 1,
        "the bot stopped at row {deepest} of {floor} ({:?})",
        world.terrain().stratum_at(TilePosition {
            x: start.x,
            y: deepest
        })
    );
    assert_eq!(descents_seen.len(), Descent::ALL.len(), "{descents_seen:?}");
    // A V11 world floats its frame (F7): 13,000 rows down, it has re-centred on the rig many
    // sectors below the origin.
    assert!(world.frame().floating());
    assert!(world.frame().sector_y >= 20, "{:?}", world.frame());
    // The floor holds.
    let rig = world.player(PLAYER).expect("a rig");
    let below = TilePosition {
        x: rig.tile_in(world.frame()).x,
        y: floor + 1,
    };
    assert_eq!(
        world.terrain().tile(below).map(|tile| tile.kind),
        Some(TileKind::Bedrock)
    );
}

/// Digs a long shaft and a few galleries across the Descents, the way a long game edits a deep
/// world.
fn excavate(world: &mut WorldState, settings: &WorldSettings) {
    let floor = floor_row(settings);
    for y in 6..floor - 2 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x: 60, y }, TileKind::Air);
    }
    let layout = layout(settings);
    for descent in Descent::ALL {
        let (top, bottom) = layout.descent_rows(descent);
        let row = i32::midpoint(top, bottom);
        for x in 20..140 {
            for y in row..row + 2 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Air);
            }
        }
    }
}

#[test]
fn a_dug_v11_world_saves_and_loads_compactly() {
    let settings = v11(0xFACE);
    let mut world = deep_world(0xFACE);
    excavate(&mut world, &settings);
    let edits = world.terrain().stored_tile_count();
    assert!(edits > 8_000, "{edits} edits");
    let json = serde_json::to_string(&world).expect("json");
    let restored: WorldState = serde_json::from_str(&json).expect("json restores");
    assert_eq!(restored.terrain().digest(), world.terrain().digest());
    assert_eq!(
        restored.terrain().generation_diffs(),
        world.terrain().generation_diffs()
    );
    assert_eq!(
        restored.simulation_state_hash(),
        world.simulation_state_hash(),
        "the loaded world is the saved one"
    );
    let bytes = postcard::to_allocvec(&world).expect("postcard");
    let from_bytes: WorldState = postcard::from_bytes(&bytes).expect("postcard restores");
    assert_eq!(from_bytes.terrain().digest(), world.terrain().digest());
    // The packed chunks are a fraction of the old nested-list form.
    let snapshot = world.terrain().snapshot();
    let packed_json = serde_json::to_string(&snapshot.packed).expect("json").len();
    let legacy_json = serde_json::to_string(&snapshot.packed.0)
        .expect("json")
        .len();
    let packed_binary = postcard::to_allocvec(&snapshot.packed)
        .expect("postcard")
        .len();
    let legacy_binary = postcard::to_allocvec(&snapshot.packed.0)
        .expect("postcard")
        .len();
    println!(
        "{edits} edited tiles in {} chunks: JSON {legacy_json} -> {packed_json} bytes, \
         binary {legacy_binary} -> {packed_binary} bytes",
        snapshot.packed.0.len()
    );
    assert!(
        packed_json * 3 < legacy_json,
        "{packed_json} vs {legacy_json}"
    );
    assert!(
        packed_binary * 2 < legacy_binary,
        "{packed_binary} vs {legacy_binary}"
    );
    // Older saves' nested-list chunks still load.
    let mut legacy = snapshot;
    legacy.chunks = std::mem::take(&mut legacy.packed.0);
    let terrain = crate::terrain::Terrain::from_snapshot(legacy).expect("legacy chunks load");
    assert_eq!(terrain.digest(), world.terrain().digest());
}
