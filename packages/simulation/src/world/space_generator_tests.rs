//! The space generator per world (Wave 14 finish): a new world's galaxy is V2 (small bodies
//! carry themes), a save of Waves 11-13 keeps V1's moons exactly, and a body's store remembers
//! the generator it was built by.

use crate::{
    PlayerId, WorldState,
    place::StoreKey,
    space::{BodyKind, Space, SpaceBodyId, SpaceGenerator, SpaceSeed, Theme},
    worldgen::DEFAULT_WORLD_SEED,
};

const PLAYER: PlayerId = PlayerId::new(1);

/// A small body near home whose theme V2 changed (and both themes).
fn a_rethemed_moon(seed: u64) -> (SpaceBodyId, Theme, Theme) {
    let v1 = Space::new(SpaceSeed::new(seed, SpaceGenerator::V1));
    let v2 = Space::new(SpaceSeed::new(seed, SpaceGenerator::V2));
    for coords in crate::space::report::sectors_around_home(2) {
        for (a, b) in v1
            .sector(coords)
            .systems
            .iter()
            .zip(&v2.sector(coords).systems)
        {
            for (x, y) in a.bodies.iter().zip(&b.bodies) {
                if matches!(x.kind, BodyKind::Moon | BodyKind::CraterRock)
                    && x.theme != y.theme
                    && y.theme != Theme::Barren
                {
                    return (
                        SpaceBodyId {
                            sector: coords,
                            system: a.index,
                            body: x.index,
                        },
                        x.theme,
                        y.theme,
                    );
                }
            }
        }
    }
    panic!("no rethemed small body near home");
}

/// Strips the field a Wave 11-13 save never had.
fn as_old_save(world: &WorldState) -> WorldState {
    let mut value = serde_json::to_value(world).expect("save");
    value
        .as_object_mut()
        .expect("world")
        .remove("space_generator");
    serde_json::from_value(value).expect("an old save loads")
}

#[test]
fn a_new_world_takes_v2_and_an_old_save_keeps_v1() {
    let world = WorldState::new_game(PLAYER);
    assert_eq!(world.space_generator(), SpaceGenerator::V2);
    assert_eq!(world.world_seed().generator, SpaceGenerator::V2);
    assert_eq!(world.world_seed().seed, DEFAULT_WORLD_SEED);
    let old = as_old_save(&world);
    assert_eq!(old.space_generator(), SpaceGenerator::V1);
    // The field never touches the state hash (it never changes in a world).
    assert_eq!(old.simulation_state_hash(), world.simulation_state_hash());
    // A save of this build keeps its generator.
    let json = serde_json::to_string(&world).expect("save");
    let again: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(again.space_generator(), SpaceGenerator::V2);
}

#[test]
fn an_old_worlds_moon_keeps_its_v1_ground_and_a_new_worlds_carries_its_theme() {
    let (id, before, after) = a_rethemed_moon(DEFAULT_WORLD_SEED);
    let new = WorldState::new_game(PLAYER);
    let mut old = new.clone();
    old.set_space_generator(SpaceGenerator::V1);
    let theme_of = |world: &WorldState| {
        crate::place::planet_params_for(world.world_seed(), id)
            .expect("landable")
            .theme
    };
    assert_eq!(theme_of(&old), crate::place::planet_theme_of(before));
    assert_eq!(theme_of(&new), crate::place::planet_theme_of(after));
    // The body's store is built by the world's generator, and a save keeps it so.
    for (mut world, theme) in [(old, before), (new, after)] {
        world.ensure_store(StoreKey::Body(id));
        let ground = world
            .store_terrain(StoreKey::Body(id))
            .and_then(|terrain| terrain.planet_ground().copied())
            .expect("the moon's ground");
        assert_eq!(ground.params().theme, crate::place::planet_theme_of(theme));
        let json = serde_json::to_string(&world).expect("save");
        let loaded: WorldState = serde_json::from_str(&json).expect("load");
        let ground = loaded
            .store_terrain(StoreKey::Body(id))
            .and_then(|terrain| terrain.planet_ground().copied())
            .expect("the moon's ground");
        assert_eq!(ground.params().theme, crate::place::planet_theme_of(theme));
    }
}

#[test]
fn an_old_saves_body_store_without_a_generator_loads_v1_ground() {
    let (id, before, _) = a_rethemed_moon(DEFAULT_WORLD_SEED);
    let mut world = WorldState::new_game(PLAYER);
    world.set_space_generator(SpaceGenerator::V1);
    world.ensure_store(StoreKey::Body(id));
    // A Wave 13 save: no generator on the world, none on its stores.
    let mut value = serde_json::to_value(&world).expect("save");
    value
        .as_object_mut()
        .expect("world")
        .remove("space_generator");
    strip_store_generators(&mut value);
    let loaded: WorldState = serde_json::from_value(value).expect("an old save loads");
    assert_eq!(loaded.space_generator(), SpaceGenerator::V1);
    let ground = loaded
        .store_terrain(StoreKey::Body(id))
        .and_then(|terrain| terrain.planet_ground().copied())
        .expect("the moon's ground");
    assert_eq!(ground.params().theme, crate::place::planet_theme_of(before));
}

/// Removes every `space_generator` key below the world's top level (the stores' own).
fn strip_store_generators(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("space_generator");
            for child in map.values_mut() {
                strip_store_generators(child);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                strip_store_generators(child);
            }
        }
        _ => {}
    }
}
