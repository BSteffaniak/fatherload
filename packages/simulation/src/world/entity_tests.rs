//! Entity store behaviour inside the world tick: shared physics matching bombs, drop sleep and
//! wake, blasts, expiry, tick-rate stability, persistence, and the performance benchmark.

use std::time::{Duration, Instant};

use super::WorldImport;
use crate::{
    ControlState, PlacedBomb, PlayerId, SimulationParams, SimulationTick, TickInputs, WorldState,
    entity::{Body, EntityId, EntityKind},
    item::{ItemKind, ItemStack},
    physics::{self, Blast, BodyParams},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);

/// A big open cave (tiles 8..=72 x 8..=40) with a bedrock floor at row 40, the rig parked far
/// away on the surface.
fn cavern(hz: u32) -> WorldState {
    let mut world = WorldState::from_import_with_params(
        &WorldImport::new_game(PLAYER),
        SimulationParams::new(hz),
    );
    for y in 8..=40 {
        for x in 8..=72 {
            let kind = if y == 40 {
                TileKind::Bedrock
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    world
}

fn tick(world: &mut WorldState) -> crate::TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, &TickInputs::default())
}

fn copper() -> ItemStack {
    ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 1)
}

fn px(tile: f32) -> f32 {
    tile * TILE_SIZE
}

#[test]
fn shared_body_physics_reproduces_bomb_motion_exactly() {
    let mut world = cavern(60);
    for x in 20..=30 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x, y: 30 }, TileKind::Stone);
    }
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: 34, y: 37 }, TileKind::Stone);
    let launches: [(f32, f32, f32, f32); 6] = [
        (px(25.5), px(20.0), 0.0, 0.0),
        (px(22.0), px(29.0), 340.0, -200.0),
        (px(33.0), px(36.5), 590.0, 10.0),
        (px(40.0), px(12.0), -250.0, -500.0),
        (px(10.0), px(39.5), -600.0, 0.0),
        (px(24.0), px(29.7), 0.0, 0.0),
    ];
    let mut bodies = Vec::new();
    for (id, (x, y, vx, vy)) in launches.into_iter().enumerate() {
        world.insert_charge(PlacedBomb {
            id: id as u64,
            x,
            y,
            velocity_x: vx,
            velocity_y: vy,
            previous_x: x,
            previous_y: y,
            detonation_tick: SimulationTick::new(100_000),
            timer_seconds: 1_000.0,
            ..PlacedBomb::default()
        });
        bodies.push(Body::at(x, y).moving(vx, vy));
    }
    let terrain = world.terrain().clone();
    let dt = world.timing().delta_seconds;
    for step in 0..600 {
        tick(&mut world);
        for body in &mut bodies {
            let _ = physics::step_body(body, &BodyParams::BOMB, &terrain, dt);
        }
        for (bomb, body) in world.bombs().iter().zip(&bodies) {
            assert_eq!(
                (bomb.x, bomb.y, bomb.velocity_x, bomb.velocity_y),
                (body.x, body.y, body.velocity_x, body.velocity_y),
                "bomb {} diverged at step {step}",
                bomb.id
            );
            assert_eq!(
                (bomb.previous_x, bomb.previous_y),
                (body.previous_x, body.previous_y)
            );
        }
    }
}

#[test]
fn drops_fall_rest_sleep_and_wake_when_their_support_is_dug_out() {
    let mut world = cavern(60);
    for x in 30..=34 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x, y: 20 }, TileKind::Stone);
    }
    let id = world.spawn_item_drop(px(32.5), px(12.0), (0.0, 0.0), copper(), None);
    for _ in 0..120 {
        tick(&mut world);
    }
    let resting = world.entities().get(id).cloned().expect("drop");
    assert!(resting.body.asleep, "the drop fell asleep on the ledge");
    assert!((resting.body.y + BodyParams::DROP.radius - px(20.0)).abs() <= 1.0);
    let revision = resting.revision;
    for _ in 0..60 {
        tick(&mut world);
    }
    assert_eq!(
        world.entities().get(id).map(|entity| entity.revision),
        Some(revision),
        "a sleeping drop does not change (so it is not resent)"
    );
    // Dig the tile under it (inside a tick, as drilling would): it wakes and falls.
    world.terrain_mut().start_journal();
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: 32, y: 20 }, TileKind::Air);
    let changes = world.terrain_mut().finish_journal();
    world.wake_entities_near(&changes);
    assert!(!world.entities().get(id).expect("drop").body.asleep);
    for _ in 0..120 {
        tick(&mut world);
    }
    let fallen = world.entities().get(id).expect("drop");
    assert!(
        fallen.body.y > px(38.0),
        "it fell to the cave floor: {}",
        fallen.body.y
    );
    world.entities().assert_indices_consistent();
}

#[test]
fn bomb_blasts_throw_nearby_drops_but_not_gas_or_boulders() {
    let mut world = cavern(60);
    let near = world.spawn_item_drop(px(40.5), px(39.5), (0.0, 0.0), copper(), None);
    let far = world.spawn_item_drop(px(60.5), px(39.5), (0.0, 0.0), copper(), None);
    for _ in 0..90 {
        tick(&mut world);
    }
    assert!(world.entities().get(near).expect("near").body.asleep);
    world.push_entities_with_blast(&Blast::bomb(px(39.0), px(39.5)));
    let thrown = world.entities().get(near).expect("near");
    assert!(!thrown.body.asleep);
    assert!(thrown.body.velocity_x > 100.0 && thrown.body.velocity_y < 0.0);
    assert!(
        world.entities().get(far).expect("far").body.asleep,
        "out of reach"
    );
    // A real bomb in the tick does the same through its detonation event.
    world.insert_charge(PlacedBomb {
        id: 1,
        x: px(59.5),
        y: px(39.5),
        previous_x: px(59.5),
        previous_y: px(39.5),
        detonation_tick: world.simulation_tick().next(),
        ..PlacedBomb::default()
    });
    tick(&mut world);
    let far = world.entities().get(far).expect("far survives");
    assert!(
        far.body.velocity_x > 0.0 && !far.body.asleep,
        "the bomb threw it"
    );
}

#[test]
fn drops_expire_on_schedule_at_every_tick_rate() {
    for hz in [30, 60, 120] {
        let mut world = cavern(hz);
        let id = world.spawn_item_drop(px(20.5), px(30.0), (0.0, 0.0), copper(), Some(1_000));
        let mut ticks = 0;
        while world.entities().contains(id) {
            tick(&mut world);
            ticks += 1;
            assert!(ticks < hz * 3, "{hz} Hz: the drop expires");
        }
        assert_eq!(ticks, hz, "{hz} Hz: one second");
    }
}

#[test]
fn drop_motion_is_tick_rate_consistent() {
    let landing = |hz: u32| {
        let mut world = cavern(hz);
        let id = world.spawn_item_drop(px(20.5), px(10.0), (160.0, -120.0), copper(), None);
        for _ in 0..hz * 3 {
            tick(&mut world);
        }
        let body = world.entities().get(id).expect("drop").body;
        (body.x, body.y)
    };
    let (x60, y60) = landing(60);
    for hz in [30, 120] {
        let (x, y) = landing(hz);
        assert!(
            (y - y60).abs() <= 1.0,
            "{hz} Hz rests on the same floor: {y} vs {y60}"
        );
        assert!(
            (x - x60).abs() <= TILE_SIZE,
            "{hz} Hz lands near the 60 Hz spot: {x} vs {x60}"
        );
    }
}

#[test]
fn entities_survive_save_and_keyframe_round_trips_and_replay_identically() {
    let mut world = cavern(60);
    for index in 0..40_u16 {
        let x = f32::from(index).mul_add(37.0, px(10.5));
        world.spawn_item_drop(
            x,
            px(12.0),
            (f32::from(index % 5) * 20.0, 0.0),
            copper(),
            None,
        );
    }
    for _ in 0..30 {
        tick(&mut world);
    }
    let json: WorldState =
        serde_json::from_str(&serde_json::to_string(&world).expect("json")).expect("json back");
    let bytes = postcard::to_stdvec(&world).expect("postcard");
    let keyframe: WorldState = postcard::from_bytes(&bytes).expect("postcard back");
    for copy in [json, keyframe] {
        assert_eq!(copy.simulation_state_hash(), world.simulation_state_hash());
        copy.entities().assert_indices_consistent();
        let mut original = world.clone();
        let mut copy = copy;
        for _ in 0..200 {
            assert_eq!(tick(&mut original), tick(&mut copy));
            assert_eq!(
                original.simulation_state_hash(),
                copy.simulation_state_hash()
            );
        }
    }
}

#[test]
fn spatial_queries_find_entities_by_area_in_id_order() {
    let mut world = cavern(60);
    let ids: Vec<EntityId> = (0..30_u16)
        .map(|index| {
            world.spawn_item_drop(
                f32::from(index).mul_add(60.0, px(10.0)),
                px(20.0),
                (0.0, 0.0),
                copper(),
                None,
            )
        })
        .collect();
    let found = world
        .entities()
        .ids_in_radius(px(10.0) + 300.0, px(20.0), 130.0);
    assert_eq!(found, ids[3..=7].to_vec());
    let rect = world
        .entities()
        .ids_in_rect(0.0, 0.0, px(10.0) + 59.0, px(30.0));
    assert_eq!(rect, vec![ids[0]]);
    assert_eq!(world.entities().count_of(EntityKind::Drop), 30);
}

/// Per-tick cost of the whole world tick with thousands of sleeping drops and hundreds of
/// active entities (drops kept bouncing by repeated blasts, gas clouds, boulders), against the
/// same world without them, plus the host's per-client entity diff for a relevance radius.
///
/// `cargo test -p fatherload_simulation entity_benchmark_report -- --ignored --nocapture`
/// (add `--release` for release numbers).
#[test]
#[ignore = "benchmark report; run explicitly"]
#[allow(
    clippy::too_many_lines,
    clippy::suboptimal_flops,
    reason = "one self-contained benchmark scenario and report"
)]
fn entity_benchmark_report() {
    const SLEEPING: usize = 5_000;
    const ACTIVE_DROPS: usize = 300;
    const CLOUDS: usize = 60;
    const BOULDERS: usize = 40;
    const TICKS: u32 = 600;

    let build = |with_entities: bool| {
        let mut world = cavern(60);
        // Rig parked inside the cave so hazards and pickups are near something.
        if let Some(player) = world.player_mut(PLAYER) {
            player.x = px(40.5);
            player.y = px(30.5);
            player.hull = 1.0e9;
        }
        if !with_entities {
            return world;
        }
        // Thousands of drops resting on a shelf of ledges across the cave.
        for x in 8..=72 {
            for y in [16, 24, 32] {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Stone);
            }
        }
        for index in 0..SLEEPING {
            #[allow(clippy::cast_precision_loss, reason = "benchmark layout")]
            let (column, shelf) = ((index % 600) as f32, (index / 600) % 3);
            let floor = [16.0, 24.0, 32.0][shelf];
            world.spawn_item_drop(
                px(8.2) + column * (px(64.5) / 600.0),
                px(floor) - BodyParams::DROP.radius,
                (0.0, 0.0),
                copper(),
                None,
            );
        }
        for index in 0..ACTIVE_DROPS {
            #[allow(clippy::cast_precision_loss, reason = "benchmark layout")]
            let x = px(9.0) + (index as f32) * (px(62.0) / ACTIVE_DROPS as f32);
            world.spawn_item_drop(x, px(36.0), (40.0, -300.0), copper(), None);
        }
        world.restore_legacy_live_entities(
            (0..CLOUDS)
                .map(|index| {
                    #[allow(clippy::cast_precision_loss, reason = "benchmark layout")]
                    let x = px(9.0) + index as f32 * 30.0;
                    crate::HazardCloud {
                        x,
                        y: px(28.0),
                        life: 1.0e6,
                        radius: 10.0,
                    }
                })
                .collect(),
            Vec::new(),
            &[],
            &[],
        );
        world
    };
    let run = |mut world: WorldState, with_entities: bool| {
        let mut total = Duration::ZERO;
        let mut worst = Duration::ZERO;
        let mut awake_sum = 0_usize;
        for step in 0..TICKS {
            if with_entities && step % 20 == 0 {
                // Keep the active layer bouncing and keep boulders coming.
                for index in 0..6_u16 {
                    let x = px(12.0) + f32::from(index) * px(10.0);
                    world.push_entities_with_blast(&Blast {
                        reach: px(6.0),
                        ..Blast::bomb(x, px(39.5))
                    });
                }
                for index in 0..(BOULDERS / 30).max(1) {
                    #[allow(clippy::cast_precision_loss, reason = "benchmark layout")]
                    let x = px(12.0) + index as f32 * px(9.0);
                    world.spawn_falling_boulder(x, px(33.0), 100, 3_000);
                }
            }
            let start = Instant::now();
            let tick_number = world.simulation_tick();
            let _ = world.simulate_tick(
                tick_number,
                &TickInputs::default().hold(PLAYER, ControlState::default()),
            );
            let elapsed = start.elapsed();
            total += elapsed;
            worst = worst.max(elapsed);
            awake_sum += world.entities().awake_count();
        }
        (total / TICKS, worst, awake_sum / TICKS as usize, world)
    };
    let (base_mean, base_worst, _, _) = run(build(false), false);
    let (mean, worst, awake, world) = run(build(true), true);
    // Host-side per-client diff: relevance radius of 20 tiles around the rig, baseline from the
    // previous frame (most entities unchanged).
    let baseline = world.entity_revisions(Some((40.5, 30.5)), Some(20.0));
    let start = Instant::now();
    let mut changed = 0;
    for _ in 0..100 {
        let current = world.entity_revisions(Some((40.5, 30.5)), Some(20.0));
        changed = current
            .iter()
            .zip(&baseline)
            .filter(|(now, before)| now != before)
            .count();
    }
    let diff = start.elapsed() / 100;
    // A burst of removals (a mass expiry of plain-block drops, a magnet sweep): every other
    // entity, in one compacting pass versus one shift per entity.
    let burst: std::collections::BTreeSet<EntityId> = world
        .entities()
        .iter()
        .step_by(2)
        .map(|entity| entity.id)
        .collect();
    let mut batch_store = world.entities().clone();
    let start = Instant::now();
    let removed = batch_store.despawn_many(&burst).len();
    let batch_burst = start.elapsed();
    let mut single_store = world.entities().clone();
    let start = Instant::now();
    for id in &burst {
        single_store.despawn(*id);
    }
    let single_burst = start.elapsed();
    assert_eq!(batch_store, single_store);
    let record_bytes = postcard::to_stdvec(&world.full_entity_delta())
        .expect("encode")
        .len();
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!("entity benchmark ({profile}, 60 Hz, {TICKS} ticks)");
    println!(
        "  entities: {} total, {awake} awake on average ({SLEEPING} placed asleep-to-be, {ACTIVE_DROPS} bouncing drops, {CLOUDS} gas clouds, boulders every 1/3 s)",
        world.entities().len()
    );
    println!("  world tick without entities: mean {base_mean:?}, worst {base_worst:?}");
    println!("  world tick with entities:    mean {mean:?}, worst {worst:?}");
    println!(
        "  entity cost per tick: {:?} (budget at 60 Hz: 16.7 ms)",
        mean.saturating_sub(base_mean)
    );
    println!(
        "  per-client relevance scan (20-tile radius, {} entities in range): {diff:?}",
        baseline.len()
    );
    println!("  ({changed} changed since the previous frame in that scan)");
    println!(
        "  burst despawn of {removed} entities: {batch_burst:?} batched ({single_burst:?} one by one)"
    );
    println!("  full entity snapshot (keyframe share): {record_bytes} bytes");
}
