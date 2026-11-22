//! World positions without limits (expansion plan §15 F7): a rig far from the origin flies,
//! collides and lands exactly as near it; floating frames re-centre without anything moving;
//! fixed frames never move; runs are deterministic and tick-rate safe.

use crate::{
    ControlState, PlayerId, SimulationParams, TickInputs, WorldState,
    enemy::EnemyDifficulty,
    item::{ItemKind, ItemStack},
    position::{Frame, SECTOR_PX, SECTOR_TILES, WorldPos},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(1);
const SEED: u64 = 0x0F7F_7F7F;
/// The arena's centre column near the origin (its offset inside its sector is what the far
/// arenas copy).
const NEAR_COLUMN: i64 = 200;
/// The arena's floor row (shallow: the band, its heat and its rules are the same far out).
const FLOOR_ROW: i32 = 20;
/// Column limit of the far test worlds (past 10^9 tiles).
const FAR_COLUMN_LIMIT: i32 = 2_100_000_000;

/// Sector offsets of the far arenas: about 10^6 px, 10^9 px and 10^9 tiles out.
const FAR_SECTORS: [i64; 3] = [61, 61_035, 1_953_125];

fn streamed_world(rate: u32) -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V5,
        ..WorldSettings::new(SEED)
    };
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(rate));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world
        .terrain_mut()
        .set_column_limit_for_tests(FAR_COLUMN_LIMIT);
    world
}

/// A closed stone box: air from `column - 28` to `column + 28`, rows `FLOOR_ROW - 10` to
/// `FLOOR_ROW - 1`, walls and floor two tiles thick.
fn carve_arena(world: &mut WorldState, column: i32) {
    for y in FLOOR_ROW - 12..=FLOOR_ROW + 1 {
        for x in column - 30..=column + 30 {
            let inside = (column - 28..=column + 28).contains(&x)
                && (FLOOR_ROW - 10..FLOOR_ROW).contains(&y);
            let kind = if inside {
                TileKind::Air
            } else {
                TileKind::Stone
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
}

/// A floating-frame world with the rig hovering in an arena `sectors` sectors right of the
/// near arena.
fn arena_world(sectors: i64, rate: u32) -> WorldState {
    let mut world = streamed_world(rate);
    world.set_floating_frame(true);
    let column = NEAR_COLUMN + sectors * SECTOR_TILES;
    let column_i32 = i32::try_from(column).expect("inside the i32 grid");
    carve_arena(&mut world, column_i32);
    // Start a few tiles from the right wall so the flight meets it.
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(column + 26, i64::from(FLOOR_ROW - 3)),
    );
    world
}

/// Hover, fly up and right into the wall, back left, then let go and land.
fn arena_inputs(tick: u64) -> TickInputs {
    let controls = match tick % 420 {
        60..150 => ControlState::new(1.0, true, false),
        150..210 => ControlState::new(-1.0, true, false),
        210..240 => ControlState::new(1.0, false, false),
        _ => ControlState::default(),
    };
    TickInputs::default().hold(PLAYER, controls)
}

/// The rig's state, bit for bit, each tick of the arena script.
fn arena_trace(world: &mut WorldState, ticks: u64) -> Vec<[u32; 6]> {
    (0..ticks)
        .map(|tick| {
            let now = world.simulation_tick();
            let _ = world.simulate_tick(now, &arena_inputs(tick));
            let rig = world.player(PLAYER).expect("rig");
            [
                rig.x.to_bits(),
                rig.y.to_bits(),
                rig.velocity_x.to_bits(),
                rig.velocity_y.to_bits(),
                rig.hull.to_bits(),
                rig.fuel.to_bits(),
            ]
        })
        .collect()
}

/// A rig flying, hitting a wall and landing 10^6 px, 10^9 px and 10^9 tiles from the origin
/// behaves bit for bit like the same flight near it.
#[test]
fn a_far_rig_flies_collides_and_lands_exactly_as_near_the_origin() {
    let mut near = arena_world(0, 60);
    let near_trace = arena_trace(&mut near, 420);
    let fastest = near_trace
        .iter()
        .map(|state| f32::from_bits(state[2]))
        .fold(0.0_f32, f32::max);
    // The script really flew, met the wall and landed.
    let rig = near.player(PLAYER).expect("rig");
    assert!(rig.velocity_y.abs() < f32::EPSILON, "landed: {rig:?}");
    assert!(
        near_trace
            .windows(2)
            .take(150)
            .any(|pair| f32::from_bits(pair[0][2]) > 1.0 && f32::from_bits(pair[1][2]) == 0.0),
        "stopped by the wall (fastest {fastest} px/s)"
    );
    for sectors in FAR_SECTORS {
        let mut far = arena_world(sectors, 60);
        assert_eq!(
            far.frame().sector_x,
            sectors,
            "the frame sits on the far sector"
        );
        let far_trace = arena_trace(&mut far, 420);
        assert_eq!(far_trace, near_trace, "{sectors} sectors out");
        // Same absolute offset from the arena, exactly.
        let near_pos = near.actor_world_pos(PLAYER).expect("near");
        let far_pos = far.actor_world_pos(PLAYER).expect("far");
        assert_eq!(far_pos.sector_x - near_pos.sector_x, sectors);
        assert_eq!((far_pos.x, far_pos.y), (near_pos.x, near_pos.y));
        let tile = far.player(PLAYER).expect("rig").tile_in(far.frame());
        let expected = i64::from(near.player(PLAYER).expect("rig").tile_in(near.frame()).x)
            + sectors * SECTOR_TILES;
        assert_eq!(
            i64::from(tile.x),
            expected,
            "absolute tile columns past 10^9"
        );
    }
}

/// The same far flight twice gives the same world (state hash), and the frame is part of it.
#[test]
fn far_runs_are_deterministic() {
    let run = || {
        let mut world = arena_world(FAR_SECTORS[2], 60);
        let _ = arena_trace(&mut world, 300);
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
}

/// At 30, 60 and 120 Hz the far flight matches the near one at the same rate exactly (the
/// tick-rate behaviour itself is pinned by the movement tick-rate tests).
#[test]
fn far_flight_is_tick_rate_safe() {
    for rate in [30, 120] {
        let ticks = 420 * u64::from(rate) / 60;
        let mut near = arena_world(0, rate);
        let mut far = arena_world(FAR_SECTORS[1], rate);
        let scaled = |world: &mut WorldState| -> Vec<[u32; 6]> {
            (0..ticks)
                .map(|tick| {
                    let now = world.simulation_tick();
                    let _ = world.simulate_tick(now, &arena_inputs(tick * 60 / u64::from(rate)));
                    let rig = world.player(PLAYER).expect("rig");
                    [
                        rig.x.to_bits(),
                        rig.y.to_bits(),
                        rig.velocity_x.to_bits(),
                        rig.velocity_y.to_bits(),
                        rig.hull.to_bits(),
                        rig.fuel.to_bits(),
                    ]
                })
                .collect()
        };
        assert_eq!(scaled(&mut far), scaled(&mut near), "{rate} Hz");
    }
}

/// Driving about 1,200 tiles east through a tunnel moves a floating frame (several sector hops):
/// the rig's absolute position never jumps, its local position stays within a sector or so of
/// the frame, a drop left behind keeps its absolute place, and the drive ends where a
/// fixed-frame world's drive ends (to f32's resolution there).
#[test]
#[allow(clippy::cast_precision_loss, reason = "test distances")]
fn a_floating_frame_follows_the_rig_and_nothing_moves() {
    let drive = |floating: bool| {
        let mut world = streamed_world(60);
        world.enable_test_mode();
        world.set_floating_frame(floating);
        // A tunnel from column 150 to 2,700 at rows 17-18.
        for x in 140..=2_710 {
            for y in 14..=21 {
                let open = (150..=2_700).contains(&x) && (17..=18).contains(&y);
                let kind = if open { TileKind::Air } else { TileKind::Stone };
                let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
            }
        }
        world.teleport_rig_to(PLAYER, WorldPos::tile_center(180, 18));
        let start = world.actor_world_pos(PLAYER).expect("rig");
        // Far behind the rig, out of its magnet's reach.
        let (dx, dy) = world.local_pos(WorldPos::tile_center(152, 18));
        let _ = world.spawn_item_drop(
            dx,
            dy,
            (0.0, 0.0),
            ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 1),
            None,
        );
        let drop_start = world
            .entities()
            .iter()
            .next()
            .map(|entity| world.world_pos(entity.body.x, entity.body.y))
            .expect("drop");
        let mut last = 0.0_f64;
        let mut hops = 0;
        let mut frame = world.frame();
        for tick in 0..3_000_u64 {
            let now = world.simulation_tick();
            let _ = world.simulate_tick(
                now,
                &TickInputs::default().hold(PLAYER, ControlState::new(1.0, false, false)),
            );
            let here = world.actor_world_pos(PLAYER).expect("rig");
            let travelled = start.delta_f64(here).0;
            assert!(
                travelled >= last - 1.0e-6,
                "tick {tick}: no jump back ({travelled} < {last})"
            );
            assert!(travelled - last < 64.0, "tick {tick}: no jump forward");
            last = travelled;
            if world.frame() != frame {
                hops += 1;
                frame = world.frame();
            }
            if floating {
                let rig = world.player(PLAYER).expect("rig");
                assert!(
                    rig.x.abs() <= SECTOR_PX * 1.5,
                    "tick {tick}: local x {}",
                    rig.x
                );
            }
        }
        let drop_now = world
            .entities()
            .iter()
            .next()
            .map(|entity| world.world_pos(entity.body.x, entity.body.y))
            .expect("drop");
        // It fell to the tunnel floor and stayed in its column.
        assert!(
            drop_start.distance(drop_now) < TILE_SIZE,
            "the drop stayed put: {drop_start:?} {drop_now:?}"
        );
        (last, hops, world.frame(), drop_now)
    };
    let (floating_travel, hops, frame, floating_drop) = drive(true);
    let (fixed_travel, fixed_hops, fixed_frame, fixed_drop) = drive(false);
    assert_eq!(
        floating_drop, fixed_drop,
        "the drop rests in the same place in both frames"
    );
    assert!(
        floating_travel > 1_100.0 * f64::from(TILE_SIZE),
        "drove far: {floating_travel}"
    );
    assert!(hops >= 2, "the frame hopped: {hops}");
    assert!(frame.sector_x >= 2);
    assert_eq!(
        (fixed_hops, fixed_frame),
        (0, Frame::FIXED),
        "a fixed frame never moves"
    );
    assert!(
        (floating_travel - fixed_travel).abs() < 0.5,
        "floating {floating_travel} vs fixed {fixed_travel}"
    );
}

/// V1-V10 worlds keep the fixed frame at the origin, wherever their rigs go.
#[test]
fn old_generators_keep_the_fixed_frame() {
    for generator in GeneratorVersion::ALL {
        assert!(!generator.floating_frame(), "{generator:?}");
    }
    let mut world = WorldState::new_game(PLAYER);
    assert_eq!(world.frame(), Frame::FIXED);
    world.teleport_rig_to(
        PLAYER,
        WorldPos::from_legacy_px(4_000.0 * TILE_SIZE, -30.0 * TILE_SIZE),
    );
    for _ in 0..30 {
        let now = world.simulation_tick();
        let _ = world.simulate_tick(now, &TickInputs::default());
    }
    assert_eq!(world.frame(), Frame::FIXED);
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        (rig.x - 128_000.0).abs() < 64.0,
        "local pixels are world pixels"
    );
}

/// A replica that receives the authority's per-tick live section moves into the new frame
/// first, so records measured in it apply cleanly and the replica's hash matches.
#[test]
fn a_replica_follows_a_frame_move() {
    let mut host = arena_world(FAR_SECTORS[0], 60);
    host.relocate_frame(Frame::floating_at(
        FAR_SECTORS[0] - 1,
        host.frame().sector_y,
    ));
    let mut replica = host.clone();
    let now = host.simulation_tick();
    let _ = host.simulate_tick(now, &arena_inputs(0));
    assert_eq!(host.frame().sector_x, FAR_SECTORS[0], "the host re-centred");
    replica.apply_live_entities(host.live_entities());
    assert_eq!(replica.frame(), host.frame());
    replica.apply_player_tick_state(PLAYER, host.player_tick_state(PLAYER).expect("state"));
    replica.set_simulation_tick(host.simulation_tick());
    let host_rig = host.player(PLAYER).expect("rig");
    let replica_rig = replica.player(PLAYER).expect("rig");
    assert_eq!((host_rig.x, host_rig.y), (replica_rig.x, replica_rig.y));
    // The template (shifted with everything else) still names the same absolute spawn.
    assert_eq!(
        host.world_pos(host.player_template().x, host.player_template().y),
        replica.world_pos(replica.player_template().x, replica.player_template().y)
    );
}

/// The micro-benchmark behind F7's position model (see the simulation README, "Wave 7
/// positions"): tick cost of a busy world in a fixed and a floating frame, the cost of moving
/// the frame, and a synthetic per-body step in three position representations.
#[test]
#[ignore = "benchmark report: cargo test -p fatherload_simulation --release position_cost_report -- --ignored --nocapture"]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "benchmark"
)]
#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::items_after_statements,
    clippy::suboptimal_flops,
    clippy::precedence,
    reason = "one self-contained benchmark"
)]
fn position_cost_report() {
    use std::time::{Duration, Instant};
    const DROPS: usize = 4_000;
    const TICKS: u32 = 600;
    let busy = |floating: bool| {
        let mut world = streamed_world(60);
        world.set_floating_frame(floating);
        carve_arena(&mut world, 200);
        for x in 172..=228 {
            let _ = world.terrain_mut().set_kind(
                TilePosition {
                    x,
                    y: FLOOR_ROW - 5,
                },
                TileKind::Stone,
            );
        }
        world.teleport_rig_to(PLAYER, WorldPos::tile_center(200, i64::from(FLOOR_ROW - 2)));
        for index in 0..DROPS {
            let (x, y) = world.local_pos(WorldPos::tile_center(
                173 + (index % 54) as i64,
                i64::from(FLOOR_ROW - 7 - (index / 54 % 3) as i32),
            ));
            let _ = world.spawn_item_drop(
                x,
                y,
                ((index % 7) as f32 * 10.0 - 30.0, -120.0),
                ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 1),
                None,
            );
        }
        world
    };
    let time_ticks = |mut world: WorldState| {
        let start = Instant::now();
        for tick in 0..u64::from(TICKS) {
            let now = world.simulation_tick();
            let _ = world.simulate_tick(now, &arena_inputs(tick));
        }
        (start.elapsed() / TICKS, world)
    };
    // Best of three alternating runs each (the first run also warms the chunk caches).
    let mut fixed_tick = Duration::MAX;
    let mut floating_tick = Duration::MAX;
    let mut survivors = 0;
    for _ in 0..3 {
        fixed_tick = fixed_tick.min(time_ticks(busy(false)).0);
        let (tick, world) = time_ticks(busy(true));
        floating_tick = floating_tick.min(tick);
        survivors = world.entities().len();
    }
    // Moving the frame of the freshly built world: every rig, drop and index entry shifts.
    let mut world = busy(true);
    let entities = world.entities().len();
    let rounds = 50;
    let start = Instant::now();
    for round in 0..rounds {
        let frame = world.frame();
        let step = if round % 2 == 0 { 1 } else { -1 };
        world.relocate_frame(Frame::floating_at(frame.sector_x + step, frame.sector_y));
    }
    let reframe = start.elapsed() / rounds;

    // Synthetic body step: integrate, quantize, four tile lookups (floor divisions).
    const BODIES: usize = 100_000;
    const STEPS: usize = 50;
    let dt = 1.0 / 60.0_f32;
    let quantize = crate::quantize::quantize_simulation_scalar;
    let mut sink = 0_i64;
    let frame = Frame::floating_at(1_953_125, 0);
    // (a) f32 local to the frame (the chosen model).
    let mut local: Vec<(f32, f32, f32)> = (0..BODIES)
        .map(|i| ((i % 512) as f32 * 3.0, 0.0, 37.0))
        .collect();
    let start = Instant::now();
    for _ in 0..STEPS {
        for body in &mut local {
            body.0 = quantize(body.2.mul_add(dt, body.0));
            let left = frame.col(body.0 - 5.0);
            let right = frame.col(body.0 + 5.0);
            sink = sink.wrapping_add(i64::from(left ^ right ^ frame.row(body.1)));
        }
    }
    let f32_cost = start.elapsed();
    // (b) i64 fixed-point sub-pixels (1/1024 px), converted for the f32 physics each step.
    let mut fixed: Vec<(i64, i64, f32)> = (0..BODIES)
        .map(|i| {
            (
                ((1_000_000_000_i64 * 32) << 10) + ((i % 512) as i64 * 3 << 10),
                0,
                37.0,
            )
        })
        .collect();
    let start = Instant::now();
    for _ in 0..STEPS {
        for body in &mut fixed {
            let x = (body.0 & ((SECTOR_TILES * 32 << 10) - 1)) as f32 / 1024.0;
            let moved = quantize(body.2.mul_add(dt, x));
            body.0 += ((moved - x) * 1024.0).round() as i64;
            let left = (body.0 - (5 << 10)).div_euclid(32 << 10);
            let right = (body.0 + (5 << 10)).div_euclid(32 << 10);
            sink = sink.wrapping_add(left ^ right ^ body.1.div_euclid(32 << 10));
        }
    }
    let fixed_cost = start.elapsed();
    // (c) a WorldPos per body (sector + f32), normalized after every move.
    let mut sectored: Vec<(WorldPos, f32)> = (0..BODIES)
        .map(|i| {
            (
                WorldPos::from_tile(1_000_000_000 + (i % 512) as i64, 0),
                37.0,
            )
        })
        .collect();
    let start = Instant::now();
    for _ in 0..STEPS {
        for body in &mut sectored {
            body.0 = body.0.offset(quantize(body.1 * dt), 0.0);
            let left = body.0.offset(-5.0, 0.0).tile().0;
            let right = body.0.offset(5.0, 0.0).tile().0;
            sink = sink.wrapping_add(left ^ right ^ body.0.tile().1);
        }
    }
    let sector_cost = start.elapsed();
    let per = |total: Duration| total.as_nanos() as f64 / (BODIES * STEPS) as f64;
    println!(
        "F7 position cost report ({DROPS} drops spawned, {survivors} entities after {TICKS} ticks of merging):"
    );
    println!("  busy tick, fixed frame:      {fixed_tick:?}");
    println!("  busy tick, floating frame:   {floating_tick:?}");
    println!("  frame move ({entities} entities): {reframe:?}");
    println!("  body step, f32 frame-local:  {:.2} ns", per(f32_cost));
    println!("  body step, i64 fixed-point:  {:.2} ns", per(fixed_cost));
    println!("  body step, per-body sector:  {:.2} ns", per(sector_cost));
    println!("  (sink {sink})");
}
