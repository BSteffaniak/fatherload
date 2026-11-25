//! Many places at once (Part III F11, `world/places.rs`): every occupied store ticks, an
//! unoccupied one is frozen and wakes exactly where it stopped, the per-tick budget bounds the
//! cost, and a world with bodies saves and reloads to the same state.

#![allow(
    clippy::suboptimal_flops,
    clippy::float_cmp,
    reason = "test positions are compared bit for bit on purpose"
)]

use std::time::Instant;

use crate::{
    ControlState, PlayerId, SimulationParams, TickInputs, WorldState,
    enemy::EnemyDifficulty,
    entity::EntityKind,
    item::{ItemKind, ItemStack},
    place::{BodyId, FlatTestGenerator, MAX_AWAKE_STORES_PER_TICK, Place, SpaceBodyKey, StoreKey},
    position::WorldPos,
    terrain::{MineralKind, TILE_SIZE},
    test_mode::TestModeAction,
    worldgen::{GeneratorVersion, WorldSettings},
};

const SEED: u64 = 0x0F11_0F11;
const ONE: PlayerId = PlayerId::new(1);
const TWO: PlayerId = PlayerId::new(2);
const THREE: PlayerId = PlayerId::new(3);
const FOUR: PlayerId = PlayerId::new(4);

const BODY_A: SpaceBodyKey = SpaceBodyKey::new(3, -2, 1, 0);
const BODY_B: SpaceBodyKey = SpaceBodyKey::new(-7, 5, 0, 2);

fn place_of(key: SpaceBodyKey) -> Place {
    Place::OnBody(BodyId::Space(key))
}

/// A V11 world (it ticks by region) with `players` rigs, peaceful.
fn deep_world(players: u64) -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(SEED)
    };
    let mut world = WorldState::new_game_with_settings(ONE, &settings, SimulationParams::new(60));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    for id in 2..=players {
        world.add_player(PlayerId::new(id));
    }
    world
}

/// Over the flat test body's surface at `column` (the rig drops onto row 0).
fn over_surface(column: i64) -> WorldPos {
    WorldPos::tile_center(column, -2)
}

fn hold(world: &WorldState, controls: impl Fn(PlayerId) -> ControlState) -> TickInputs {
    let mut inputs = TickInputs::default();
    for id in world.players().keys() {
        inputs = inputs.hold_as(*id, controls(*id));
    }
    inputs
}

fn run(world: &mut WorldState, ticks: u64, controls: impl Fn(PlayerId) -> ControlState) {
    for _ in 0..ticks {
        let inputs = hold(world, &controls);
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
    }
}

fn rig_px(world: &WorldState, player: PlayerId) -> (f32, f32) {
    let rig = world.player(player).expect("rig");
    (rig.x, rig.y)
}

#[test]
fn players_in_two_places_both_play_each_tick() {
    let mut world = deep_world(2);
    world.enable_test_mode();
    world
        .move_to_place(TWO, place_of(BODY_A), over_surface(10))
        .expect("arrives");
    assert_eq!(world.place_of(TWO), place_of(BODY_A));
    assert_eq!(world.place_of(ONE), Place::HOME);
    assert_eq!(world.store_of(TWO), StoreKey::Body(BODY_A));
    let home_digest = world.terrain().digest();
    let body_store = StoreKey::Body(BODY_A);
    let (one_x, one_y) = rig_px(&world, ONE);
    // One flies up at home while two lands on the body and drills down into it.
    let mut body_ops = 0;
    for tick in 0..240_u64 {
        let inputs = hold(&world, |id| {
            if id == ONE {
                ControlState::new(0.0, true, false)
            } else {
                ControlState::new(0.0, false, tick > 30)
            }
        });
        let now = world.simulation_tick();
        let output = world.simulate_tick(now, &inputs);
        body_ops += output.store_terrain_ops.len();
        assert!(
            output
                .store_terrain_ops
                .iter()
                .all(|(store, _)| *store == body_store)
        );
    }
    assert!(body_ops > 0, "the body's ground was dug");
    let body = world
        .places()
        .store(body_store)
        .expect("the body's ground exists");
    assert!(body.terrain().edited_chunk_count() > 0);
    let (x, y) = rig_px(&world, TWO);
    let frame = world.store_frame(body_store).expect("frame");
    assert!(
        frame.tile_at(x, y).y > 2,
        "the rig drilled down into the body: {:?}",
        frame.tile_at(x, y)
    );
    assert!(
        rig_px(&world, ONE).1 < one_y - TILE_SIZE,
        "one flew up at home"
    );
    assert!((rig_px(&world, ONE).0 - one_x).abs() < 1.0);
    // Nothing of the body leaked into the home world's ground.
    assert_eq!(world.terrain().digest(), home_digest);
    // The occupied regions cover both places.
    let regions = world.occupied_regions();
    assert!(regions.iter().any(|region| region.store == StoreKey::Home));
    assert!(regions.iter().any(|region| region.store == body_store));
}

/// A drop resting in the air over the body, five tiles up.
fn drop_on_body(world: &mut WorldState) -> crate::entity::EntityId {
    world
        .with_store_ground(StoreKey::Body(BODY_A), |world| {
            let (x, y) = world.local_pos(WorldPos::tile_center(14, -6));
            world.spawn_item_drop(
                x,
                y,
                (0.0, 0.0),
                ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 1),
                None,
            )
        })
        .expect("the body exists")
}

fn drop_y(world: &WorldState, id: crate::entity::EntityId) -> f32 {
    world
        .store_entities(StoreKey::Body(BODY_A))
        .and_then(|entities| entities.get(id))
        .expect("the drop is kept")
        .body
        .y
}

/// Leaves the body for home after a few ticks, waits, comes back and lets the drop land.
fn freeze_and_wake(world: &mut WorldState) -> (f32, f32, f32) {
    world
        .move_to_place(TWO, place_of(BODY_A), over_surface(10))
        .expect("arrives");
    let id = drop_on_body(world);
    run(world, 6, |_| ControlState::default());
    let falling = drop_y(world, id);
    let home = world
        .frame()
        .world(world.player_template().x, world.player_template().y);
    world
        .move_to_place(TWO, Place::HOME, home)
        .expect("comes home");
    run(world, 120, |_| ControlState::default());
    let frozen = drop_y(world, id);
    world
        .move_to_place(TWO, place_of(BODY_A), over_surface(10))
        .expect("goes back");
    run(world, 180, |_| ControlState::default());
    (falling, frozen, drop_y(world, id))
}

#[test]
fn an_empty_place_freezes_and_wakes_where_it_stopped() {
    let mut world = deep_world(2);
    let (falling, frozen, landed) = freeze_and_wake(&mut world);
    assert!(
        (falling - frozen).abs() < f32::EPSILON,
        "nobody there: the drop hung where it was ({falling} -> {frozen})"
    );
    assert!(
        landed > frozen + TILE_SIZE,
        "back again: it fell on ({landed})"
    );
    // Every step is a pure function of the state: a second run agrees bit for bit.
    let mut again = deep_world(2);
    let replay = freeze_and_wake(&mut again);
    assert_eq!(replay, (falling, frozen, landed));
    assert_eq!(world.simulation_state_hash(), again.simulation_state_hash());
}

#[test]
fn far_regions_of_the_home_world_freeze_on_deep_worlds() {
    let mut world = deep_world(1);
    let frame = world.frame();
    let (rig_x, rig_y) = rig_px(&world, ONE);
    let rig_tile = frame.tile_at(rig_x, rig_y);
    // Hanging in open sky: 40 tiles up near the rig, and 3,000 columns away.
    let near = world.spawn_item_drop(
        rig_x + 4.0 * TILE_SIZE,
        rig_y - 40.0 * TILE_SIZE,
        (0.0, 0.0),
        ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 1),
        None,
    );
    let far_x = frame.col_px(rig_tile.x + 3_000);
    let far = world.spawn_item_drop(
        far_x,
        rig_y - 40.0 * TILE_SIZE,
        (0.0, 0.0),
        ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 1),
        None,
    );
    let y_of = |world: &WorldState, id| world.entities().get(id).expect("kept").body.y;
    let (near_before, far_before) = (y_of(&world, near), y_of(&world, far));
    run(&mut world, 20, |_| ControlState::default());
    assert!(y_of(&world, near) > near_before, "near the rig: it falls");
    assert!(
        (y_of(&world, far) - far_before).abs() < f32::EPSILON,
        "3,000 columns from everyone: frozen"
    );
    // A classic world ticks every awake entity, as it always did.
    let mut classic = WorldState::new_game_with_settings(
        ONE,
        &WorldSettings {
            generator: GeneratorVersion::V10,
            ..WorldSettings::new(SEED)
        },
        SimulationParams::new(60),
    );
    assert!(!classic.ticks_by_region());
    let (cx, cy) = rig_px(&classic, ONE);
    let cframe = classic.frame();
    let far = classic.spawn_item_drop(
        cframe.col_px(cframe.tile_at(cx, cy).x + 3_000),
        cy - 40.0 * TILE_SIZE,
        (0.0, 0.0),
        ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 1),
        None,
    );
    let before = y_of(&classic, far);
    run(&mut classic, 20, |_| ControlState::default());
    assert!(y_of(&classic, far) > before);
}

/// Mean and worst tick time of `world` over `ticks` ticks, with every rig hovering.
fn time_ticks(world: &mut WorldState, ticks: u32) -> (f64, f64) {
    let mut total = 0.0;
    let mut worst = 0.0_f64;
    for tick in 0..ticks {
        let inputs = hold(world, |_| {
            ControlState::new(
                if tick % 120 < 60 { 1.0 } else { -1.0 },
                tick % 30 < 15,
                false,
            )
        });
        let now = world.simulation_tick();
        let started = Instant::now();
        let _ = world.simulate_tick(now, &inputs);
        let seconds = started.elapsed().as_secs_f64();
        total += seconds;
        worst = worst.max(seconds);
    }
    (total / f64::from(ticks), worst)
}

#[test]
fn four_players_in_four_places_stay_within_the_tick_budget() {
    // Baseline: one rig at home.
    let mut alone = deep_world(1);
    let _ = time_ticks(&mut alone, 30);
    let (alone_mean, _) = time_ticks(&mut alone, 240);
    // Four rigs: the surface, a mile down, a space body and free flight.
    let mut world = deep_world(4);
    let surface = world.actor_world_pos(ONE).expect("rig");
    world.teleport_rig_to(TWO, surface.offset(0.0, 3_000.0 * TILE_SIZE));
    world
        .move_to_place(THREE, place_of(BODY_A), over_surface(0))
        .expect("lands");
    world
        .move_to_place(FOUR, Place::Space, WorldPos::tile_center(0, 0))
        .expect("flies");
    let stores: std::collections::BTreeSet<StoreKey> = world
        .occupied_regions()
        .into_iter()
        .map(|region| region.store)
        .collect();
    assert_eq!(
        stores.len(),
        3,
        "home, the body and free flight: {stores:?}"
    );
    let _ = time_ticks(&mut world, 30);
    let (mean, worst) = time_ticks(&mut world, 240);
    println!(
        "F11 budget: one rig at home {:.3} ms/tick; four rigs in four places {:.3} ms/tick \
         (worst {:.3} ms)",
        alone_mean * 1e3,
        mean * 1e3,
        worst * 1e3
    );
    // Four occupants cost about four single-place ticks plus each store's own pass; far less
    // than ten (generous for loaded test machines).
    assert!(
        mean < alone_mean.mul_add(10.0, 0.004),
        "four places cost {mean} s/tick against {alone_mean} s/tick alone"
    );
    for player in [ONE, TWO, THREE, FOUR] {
        assert!(world.player(player).is_some_and(|rig| rig.x.is_finite()));
    }
}

#[test]
fn more_stores_than_the_budget_take_turns() {
    let stores = MAX_AWAKE_STORES_PER_TICK + 2;
    let mut world = deep_world(1);
    let mut players = Vec::new();
    for index in 0..stores {
        let id = PlayerId::new(10 + index as u64);
        world.add_player(id);
        let key = SpaceBodyKey::new(i32::try_from(index).expect("small"), 0, 0, 0);
        world
            .move_to_place(id, place_of(key), WorldPos::tile_center(0, -60))
            .expect("arrives");
        players.push(id);
    }
    let heights = |world: &WorldState| -> Vec<f32> {
        players
            .iter()
            .map(|id| world.player(*id).expect("rig").y)
            .collect()
    };
    let start = heights(&world);
    run(&mut world, 1, |_| ControlState::default());
    let after_one = heights(&world);
    let moved = start
        .iter()
        .zip(&after_one)
        .filter(|(before, after)| before != after)
        .count();
    assert_eq!(
        moved, MAX_AWAKE_STORES_PER_TICK,
        "one tick serves the budget"
    );
    run(&mut world, 1, |_| ControlState::default());
    let after_two = heights(&world);
    assert!(
        start
            .iter()
            .zip(&after_two)
            .all(|(before, after)| before != after),
        "the next tick serves the rest first: nobody starves"
    );
}

#[test]
fn a_world_with_bodies_saves_and_reloads_to_the_same_state() {
    let mut world = deep_world(3);
    world.enable_test_mode();
    world
        .move_to_place(TWO, place_of(BODY_A), over_surface(5))
        .expect("arrives");
    world
        .move_to_place(THREE, place_of(BODY_B), over_surface(-40))
        .expect("arrives");
    let _ = drop_on_body(&mut world);
    // Dig into both bodies.
    run(&mut world, 200, |id| {
        ControlState::new(0.0, false, id != ONE)
    });
    assert!(
        world
            .places()
            .stores()
            .all(|(_, body)| body.terrain().edited_chunk_count() > 0)
    );
    let hash = world.simulation_state_hash();
    let json = serde_json::to_string(&world).expect("saves");
    let loaded: WorldState = serde_json::from_str(&json).expect("loads");
    assert_eq!(loaded.simulation_state_hash(), hash);
    let bytes = postcard::to_allocvec(&world).expect("encodes");
    let decoded: WorldState = postcard::from_bytes(&bytes).expect("decodes");
    assert_eq!(decoded.simulation_state_hash(), hash);
    // And it plays on exactly as the original does.
    let (mut original, mut reloaded) = (world, loaded);
    run(&mut original, 90, |id| {
        ControlState::new(0.5, id == ONE, true)
    });
    run(&mut reloaded, 90, |id| {
        ControlState::new(0.5, id == ONE, true)
    });
    assert_eq!(
        original.simulation_state_hash(),
        reloaded.simulation_state_hash()
    );
    // The bodies' edits are saved packed (runs of equal tiles), not tile by tile.
    let body_json = serde_json::to_string(original.places()).expect("saves");
    assert!(body_json.contains("packed"));
}

#[test]
fn a_tampered_body_is_refused_on_load() {
    let mut world = deep_world(2);
    world.enable_test_mode();
    world
        .move_to_place(TWO, place_of(BODY_A), over_surface(5))
        .expect("arrives");
    run(&mut world, 120, |id| {
        ControlState::new(0.0, false, id == TWO)
    });
    let json = serde_json::to_string(&world).expect("saves");
    let digest = world
        .places()
        .store(StoreKey::Body(BODY_A))
        .expect("body")
        .terrain()
        .digest();
    let tampered = json.replace(&format!("\"digest\":{digest}"), "\"digest\":1");
    assert_ne!(tampered, json);
    assert!(serde_json::from_str::<WorldState>(&tampered).is_err());
}

#[test]
fn moving_keeps_cargo_and_state_and_takes_the_pilot_along() {
    let mut world = deep_world(1);
    world.enable_test_mode();
    let rig = world.player_mut(ONE).expect("rig");
    rig.fuel = 123.0;
    rig.velocity_x = 40.0;
    let fuel = rig.fuel;
    let cargo = rig.cargo_used();
    world
        .move_to_place(ONE, place_of(BODY_A), over_surface(0))
        .expect("arrives");
    let rig = world.player(ONE).expect("rig");
    assert!((rig.fuel - fuel).abs() < f32::EPSILON);
    assert_eq!(rig.cargo_used(), cargo);
    assert!(
        (rig.velocity_x - 40.0).abs() < f32::EPSILON,
        "momentum is kept"
    );
    let (place, at) = world.rig_world_pos(ONE).expect("placed");
    assert_eq!(place, place_of(BODY_A));
    assert!(at.distance(over_surface(0)) < 0.01);
    // Unknown players and places off the ground are refused.
    assert_eq!(
        world.move_to_place(PlayerId::new(99), Place::Space, WorldPos::tile_center(0, 0)),
        Err(crate::place::PlaceError::UnknownPlayer)
    );
    assert_eq!(
        world.move_to_place(
            ONE,
            place_of(BODY_A),
            over_surface(i64::from(FlatTestGenerator::HALF_WIDTH) + 50)
        ),
        Err(crate::place::PlaceError::OutsideGround)
    );
    // The test-mode warp goes through the tick (and so works online): home again.
    let mut inputs = TickInputs::default();
    inputs
        .players
        .entry(ONE)
        .or_default()
        .actions
        .push(crate::TickAction {
            sequence: crate::InputSequence::new(0),
            command: crate::PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::HOME)),
        });
    let now = world.simulation_tick();
    let output = world.simulate_tick(now, &inputs);
    assert_eq!(world.place_of(ONE), Place::HOME);
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        crate::GameplayEventKind::PlaceChanged {
            player_id: ONE,
            to: Place::OnBody(BodyId::Home),
            ..
        }
    )));
}

#[test]
fn the_moon_is_a_body_of_the_home_store() {
    let mut world = deep_world(1);
    world.enable_test_mode();
    let mut inputs = TickInputs::default();
    inputs
        .players
        .entry(ONE)
        .or_default()
        .actions
        .push(crate::TickAction {
            sequence: crate::InputSequence::new(0),
            command: crate::PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::OnBody(
                BodyId::Moon,
            ))),
        });
    let now = world.simulation_tick();
    let _ = world.simulate_tick(now, &inputs);
    assert_eq!(world.place_of(ONE), Place::OnBody(BodyId::Moon));
    assert_eq!(world.store_of(ONE), StoreKey::Home);
    assert!(
        world.places().is_empty(),
        "the Moon needs no store of its own"
    );
}

#[test]
fn one_pass_per_store_keeps_entity_ids_unique() {
    let mut world = deep_world(2);
    world.enable_test_mode();
    world
        .move_to_place(TWO, place_of(BODY_A), over_surface(0))
        .expect("arrives");
    let _ = drop_on_body(&mut world);
    let _home = world.spawn_item_drop(
        world.player(ONE).expect("rig").x,
        world.player(ONE).expect("rig").y - 3.0 * TILE_SIZE,
        (0.0, 0.0),
        ItemStack::new(ItemKind::Mineral(MineralKind::Iron), 1),
        None,
    );
    run(&mut world, 120, |_| ControlState::new(0.0, false, true));
    let mut ids: Vec<u64> = world.entities().iter().map(|entity| entity.id.0).collect();
    for (_, body) in world.places().stores() {
        ids.extend(body.entities().iter().map(|entity| entity.id.0));
    }
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "no id is used twice across stores");
    assert!(
        world
            .places()
            .store(StoreKey::Body(BODY_A))
            .is_some_and(|body| body.entities().count_of(EntityKind::Drop) > 0)
    );
}

#[test]
fn a_world_that_never_left_home_hashes_as_before() {
    let world = deep_world(1);
    assert!(world.places().is_empty());
    let mut moved = world.clone();
    moved
        .move_to_place(ONE, place_of(BODY_A), over_surface(0))
        .expect("arrives");
    assert_ne!(moved.simulation_state_hash(), world.simulation_state_hash());
    let mut back = moved.clone();
    let home = world.actor_world_pos(ONE).expect("rig");
    back.move_to_place(ONE, Place::HOME, home).expect("home");
    // The body's ground stays (frozen, saved) once visited.
    assert!(!back.places().is_empty());
}

/// Review fix: the town's counters are the home world's. A rig on a body standing where the
/// depot would be in its own ground's coordinates cannot sell, refuel or repair there, and no
/// surface zone is reported for it.
#[test]
fn the_home_towns_counters_are_not_on_other_bodies() {
    let mut world = deep_world(1);
    let depot = crate::surface::SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == crate::economy::SurfaceZone::Depot)
        .expect("a depot");
    let column = i64::from(depot.tile_x + depot.tile_width / 2);
    world
        .move_to_place(ONE, place_of(BODY_A), over_surface(column))
        .expect("lands");
    run(&mut world, 60, |_| ControlState::default());
    // In the body's own coordinates the rig stands over the depot's columns.
    let (x, y) = world
        .store_frame(world.store_of(ONE))
        .map(|frame| {
            let rig = world.player(ONE).expect("rig");
            frame.legacy_px(rig.x, rig.y)
        })
        .expect("a frame");
    assert!(
        crate::surface::surface_zone_at(x, y, TILE_SIZE).is_some(),
        "the test stands where the town would be"
    );
    assert_eq!(world.player_surface_zone(ONE), None);
    // Something to sell, a tank to fill and a hull to mend.
    let rig = world.player_mut(ONE).expect("rig");
    rig.cargo_bay.add(MineralKind::Copper, 3);
    rig.fuel = 5.0;
    rig.hull = 40.0;
    rig.credits = 10_000;
    for command in [
        crate::PlayerCommand::SellCargo,
        crate::PlayerCommand::Refuel,
        crate::PlayerCommand::Repair,
    ] {
        let mut inputs = TickInputs::default();
        inputs
            .players
            .entry(ONE)
            .or_default()
            .actions
            .push(crate::TickAction {
                sequence: crate::InputSequence::new(1),
                command: command.clone(),
            });
        let now = world.simulation_tick();
        let output = world.simulate_tick(now, &inputs);
        assert!(
            output
                .action_outcomes
                .iter()
                .all(|outcome| outcome.outcome
                    == crate::PlayerScopedCommandOutcome::IgnoredUnavailable),
            "{command:?} on a body: {:?}",
            output.action_outcomes
        );
    }
}

/// Review fix: a rig parked on a body stays where it was parked, in absolute terms, while the
/// home world's frame follows the rigs at home and while another rig's arrival moves the
/// body's frame.
#[test]
fn a_rig_parked_on_a_body_stays_put_through_frame_moves() {
    let mut world = deep_world(3);
    world
        .move_to_place(TWO, place_of(BODY_A), over_surface(10))
        .expect("lands");
    run(&mut world, 30, |_| ControlState::default());
    let (place, parked_at) = world.rig_world_pos(TWO).expect("placed");
    assert_eq!(place, place_of(BODY_A));
    assert!(world.park_player(TWO));
    // The home frame moves with the rig at home, a long way down.
    let home = world.actor_world_pos(ONE).expect("rig");
    let home_frame = world.frame();
    world.teleport_rig_to(ONE, home.offset(0.0, 4_000.0 * TILE_SIZE));
    run(&mut world, 5, |_| ControlState::default());
    assert_ne!(world.frame(), home_frame, "the home frame moved");
    // Another rig lands far across the body: its arrival moves the body's frame.
    world
        .move_to_place(
            THREE,
            place_of(BODY_A),
            over_surface(i64::from(FlatTestGenerator::HALF_WIDTH) - 20),
        )
        .expect("lands");
    run(&mut world, 5, |_| ControlState::default());
    assert!(world.unpark_player(TWO));
    let (place, back_at) = world.rig_world_pos(TWO).expect("placed");
    assert_eq!(place, place_of(BODY_A));
    assert!(
        back_at.distance(parked_at) < 0.01,
        "parked at {parked_at:?}, back at {back_at:?}"
    );
}

/// A real landable planet of `seed`'s space near home.
fn a_planet_near_home(seed: u64) -> SpaceBodyKey {
    let space = crate::space::Space::shared(seed);
    crate::space::report::sectors_around_home(3)
        .into_iter()
        .find_map(|coords| {
            let sector = space.sector(coords);
            sector.systems.iter().find_map(|system| {
                system.bodies.iter().find_map(|body| {
                    let id = crate::space::SpaceBodyId {
                        sector: coords,
                        system: system.index,
                        body: body.index,
                    };
                    crate::place::planet_params_for(seed, id).map(|_| id)
                })
            })
        })
        .expect("a landable planet near home")
}

#[test]
fn a_body_whose_ground_a_newer_build_regenerated_loads_fresh() {
    let mut world = deep_world(2);
    world.enable_test_mode();
    let planet = a_planet_near_home(SEED);
    world
        .move_to_place(TWO, place_of(planet), WorldPos::tile_center(0, -40))
        .expect("arrives");
    run(&mut world, 60, |_| ControlState::new(0.0, false, false));
    let store = world
        .places()
        .store(StoreKey::Body(planet))
        .expect("the planet's store");
    assert!(store.planet().is_some(), "a round body");
    let fingerprint = store.fingerprint();
    let json = serde_json::to_string(&world).expect("saves");
    // As if an older build had generated other ground there (Wave 11's barren ground on a
    // body Wave 12 re-themes): the save still loads, that body afresh.
    let older = json.replace(
        &format!("\"fingerprint\":{fingerprint}"),
        "\"fingerprint\":1",
    );
    assert_ne!(older, json);
    let loaded: WorldState = serde_json::from_str(&older).expect("loads");
    let body = loaded
        .places()
        .store(StoreKey::Body(planet))
        .expect("the body is still there");
    assert!(body.is_pristine());
    assert_eq!(body.fingerprint(), fingerprint);
}
