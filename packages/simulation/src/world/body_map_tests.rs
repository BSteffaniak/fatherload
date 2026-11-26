//! A round body's own map and its lap-aware tile keys (Wave 12 systems): what a rig on a
//! body discovers is that body's map (the home map is untouched), keyed round the lap, saved
//! with the body; drill and pick work and their comparisons wrap the seam.

use crate::{
    ControlState, PlayerId, SimulationParams, TickInputs, WorldState,
    enemy::EnemyDifficulty,
    place::{BodyId, Place, SpaceBodyKey, StoreKey},
    terrain::TilePosition,
    test_mode::TestModeAction,
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x0B0D_7A95;

fn world() -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    };
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(30));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.enable_test_mode();
    world
}

/// The first landable generated body of the home system.
fn landable() -> SpaceBodyKey {
    let space = crate::space::Space::shared(crate::space::SpaceSeed::from(SEED));
    let home = space.home_system();
    home.bodies
        .iter()
        .filter(|body| !body.is_home())
        .find_map(|body| {
            let key = SpaceBodyKey {
                sector: crate::space::HOME_SECTOR,
                system: home.index,
                body: body.index,
            };
            crate::place::planet_params_for(crate::space::SpaceSeed::from(SEED), key).map(|_| key)
        })
        .expect("a landable body at home")
}

fn warp(world: &mut WorldState, place: Place) {
    let inputs = TickInputs::held(PLAYER, ControlState::default()).act(
        PLAYER,
        crate::PlayerCommand::TestMode(TestModeAction::WarpToPlace(place)),
    );
    let now = world.simulation_tick();
    let _ = world.simulate_tick(now, &inputs);
}

fn run(world: &mut WorldState, ticks: u32) {
    for _ in 0..ticks {
        let now = world.simulation_tick();
        let _ = world.simulate_tick(now, &TickInputs::held(PLAYER, ControlState::default()));
    }
}

#[test]
fn a_body_keeps_its_own_map_round_the_lap_and_saves_it() {
    let mut world = world();
    run(&mut world, 3);
    let home_before = world
        .discovered_tiles(PLAYER)
        .map_or(0, crate::world::DiscoveredTiles::len);
    let key = landable();
    let store = StoreKey::Body(key);
    warp(&mut world, Place::OnBody(BodyId::Space(key)));
    assert_eq!(world.store_of(PLAYER), store);
    run(&mut world, 30);
    let body_map = world.store_discovered(store, PLAYER).expect("a body map");
    assert!(
        body_map.len() > 50,
        "the rig sees round itself: {}",
        body_map.len()
    );
    assert_eq!(
        world
            .discovered_tiles(PLAYER)
            .map_or(0, crate::world::DiscoveredTiles::len),
        home_before,
        "the home map is untouched"
    );
    // Keyed round the lap: a tile and its copy a lap over are one map tile.
    let terrain = world.store_terrain(store).expect("ground");
    let lap = i32::try_from(
        terrain
            .planet_ground()
            .expect("round")
            .grid()
            .surface_columns(),
    )
    .expect("columns");
    let rig = world.player(PLAYER).expect("rig");
    let tile = terrain.frame().tile_at(rig.x, rig.y);
    let wrapped = terrain.lap_tile(tile);
    assert!((0..lap).contains(&wrapped.x));
    // A tile labelled a lap over reads the same map tile.
    let probe = TilePosition {
        x: wrapped.x,
        y: terrain.min_y() + 1,
    };
    assert_eq!(
        body_map.contains(terrain.lap_tile(probe)),
        body_map.contains(terrain.lap_tile(TilePosition {
            x: probe.x + lap,
            ..probe
        }))
    );
    assert!(terrain.same_tile(
        TilePosition {
            x: tile.x + lap,
            y: tile.y
        },
        tile
    ));
    assert!(!terrain.same_tile(
        TilePosition {
            x: tile.x + 1,
            y: tile.y
        },
        tile
    ));
    // Saved with the body.
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.store_discovered(store, PLAYER), Some(body_map));
    // Home again, the body's map stays the body's.
    let before = body_map.len();
    warp(&mut world, Place::HOME);
    run(&mut world, 5);
    assert_eq!(
        world
            .store_discovered(store, PLAYER)
            .map(crate::world::DiscoveredTiles::len),
        Some(before)
    );
}

#[test]
fn the_home_world_compares_tiles_plainly() {
    let world = world();
    let terrain = world.terrain();
    let tile = TilePosition { x: 10, y: 4 };
    assert_eq!(terrain.lap_tile(tile), tile);
    assert!(!terrain.same_tile(
        tile,
        TilePosition {
            x: 10 + 4_096,
            y: 4
        }
    ));
}

/// Body maps travel by relevance (Wave 13 QA): a keyframe carries only its viewer's own maps
/// of the bodies (as the home map), never another player's, and a spectator gets none; the
/// viewer's visible state is unchanged and its map of a body it left is still there.
#[test]
fn a_keyframe_carries_only_its_viewers_body_maps() {
    const OTHER: PlayerId = PlayerId::new(1);
    let mut world = world();
    world.add_player(OTHER);
    let key = landable();
    let store = StoreKey::Body(key);
    for player in [PLAYER, OTHER] {
        let inputs = TickInputs::held(player, ControlState::default()).act(
            player,
            crate::PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::OnBody(
                BodyId::Space(key),
            ))),
        );
        let now = world.simulation_tick();
        let _ = world.simulate_tick(now, &inputs);
    }
    run(&mut world, 20);
    // The first player goes home: its map of the body stays (and stays its own).
    warp(&mut world, Place::HOME);
    run(&mut world, 3);
    assert_eq!(
        world.places().map_holders(),
        [PLAYER, OTHER].into_iter().collect()
    );
    for viewer in [PLAYER, OTHER] {
        let keyframe = world.redacted_for(Some(viewer));
        assert_eq!(
            keyframe.places().map_holders(),
            std::iter::once(viewer).collect(),
            "{viewer:?} gets its own maps only"
        );
        assert_eq!(
            keyframe.store_discovered(store, viewer),
            world.store_discovered(store, viewer),
            "{viewer:?}'s map whole"
        );
        assert_eq!(
            keyframe.visible_state_hash(Some(viewer)),
            world.visible_state_hash(Some(viewer))
        );
        // Smaller on the wire than the whole world's maps.
        let sent = postcard::to_allocvec(&keyframe).expect("encode").len();
        let mut everyone = keyframe;
        everyone.places = world.places.clone();
        let all = postcard::to_allocvec(&everyone).expect("encode").len();
        assert!(sent < all, "{sent} bytes against {all}");
    }
    assert!(world.redacted_for(None).places().map_holders().is_empty());
}

/// A replica applying a body store's frame that its host relabelled by whole laps moves
/// nothing (Wave 13 QA, found by the co-op soak): the host's own pass treats a lap as the same
/// place, and the replica used to shift every creature and drop of the body a lap away, out of
/// step with the host until each one changed again.
#[test]
fn a_replica_applying_a_store_frame_relabelled_by_a_lap_moves_nothing() {
    let mut world = world();
    let key = landable();
    let store = StoreKey::Body(key);
    warp(&mut world, Place::OnBody(BodyId::Space(key)));
    run(&mut world, 5);
    let (x, y) = {
        let rig = world.player(PLAYER).expect("rig");
        (rig.x, rig.y)
    };
    let stack = crate::item::ItemStack::new(
        crate::item::ItemKind::Mineral(crate::terrain::MineralKind::Copper),
        1,
    );
    world
        .with_store_ground(store, |ground| {
            ground.spawn_item_drop(x + 40.0, y - 20.0, (0.0, 0.0), stack, None)
        })
        .expect("the body's ground");
    let positions = |world: &WorldState| -> Vec<(u64, f32, f32)> {
        world
            .store_entities(store)
            .expect("entities")
            .iter()
            .map(|entity| (entity.id.get(), entity.body.x, entity.body.y))
            .collect()
    };
    let before = positions(&world);
    assert_ne!(before, [] as [(u64, f32, f32); 0]);
    let frame = world.store_frame(store).expect("frame");
    let lap = world
        .store_terrain(store)
        .and_then(crate::terrain::Terrain::planet_ground)
        .expect("round ground")
        .grid()
        .lap_sectors();
    for laps in [1, -1, 3] {
        let relabelled = crate::position::Frame {
            sector_x: frame.sector_x + laps * lap,
            ..frame
        };
        world.apply_store_frames(&[(store, relabelled)]);
        assert_eq!(world.store_frame(store), Some(relabelled));
        assert_eq!(positions(&world), before, "{laps} laps over: nothing moved");
    }
    // A real move still moves everything with it.
    let moved = crate::position::Frame {
        sector_x: frame.sector_x + 3 * lap + 1,
        ..frame
    };
    world.apply_store_frames(&[(store, moved)]);
    let after = positions(&world);
    for ((_, old_x, _), (_, new_x, _)) in before.iter().zip(&after) {
        assert!((old_x - new_x - crate::position::SECTOR_PX).abs() < 0.01);
    }
}
