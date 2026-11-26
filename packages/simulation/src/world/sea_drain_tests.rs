//! The ocean's sea draining into a shaft (Wave 13 QA): the fluid step's cost while a shaft
//! sunk from the seabed of a planet-wide sea fills, and that the faster step moves exactly the
//! water the reference step did (same tiles, same schedule, tick for tick).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "test fixtures convert small tile and tick numbers freely"
)]

use crate::{
    ControlState, PlayerId, SimulationParams, TickInputs, WorldState,
    hash::StateHasher,
    planet::{BodyGround, PlanetParams, PlanetThemeId, wilds::ocean, world_of},
    terrain::{TileKind, TilePosition},
    worldgen::TileRect,
};

/// The shaft's tiles (inclusive).
#[derive(Clone, Copy, Debug)]
struct Shaft {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

const PLAYER: PlayerId = PlayerId::new(0);
/// Rows of the shaft under the seabed.
const SHAFT_ROWS: i32 = 70;

fn ocean_world(hz: u32) -> WorldState {
    WorldState::new_planet_world(
        PLAYER,
        PlanetParams::new(0x5EED_0012, 3_000, PlanetThemeId::OCEAN),
        SimulationParams::new(hz),
    )
}

/// A column of deep open sea away from the island, and its seabed row.
fn open_sea(world: &WorldState) -> (i32, i32) {
    let planet = *world.planet().expect("planet");
    let site = world_of(PlanetThemeId::OCEAN)
        .and_then(|theme| theme.settlement_site(&planet))
        .expect("an island");
    let width = i64::from(planet.strip_width());
    let column = (0..width)
        .filter(|column| {
            crate::planet::wilds::wrap_delta(*column, i64::from(site.centre()), width).abs() > 400
        })
        .find(|column| ocean::sea_depth(&planet, *column) > 14)
        .and_then(|column| i32::try_from(column).ok())
        .expect("open sea");
    (column, planet.surface_row(column))
}

/// Sinks a three-wide shaft from the seabed at `column` and wakes the water round its mouth
/// (as a drill's tile writes do); the rig waits on the island, out of the way.
fn sink_shaft(world: &mut WorldState, column: i32, seabed: i32) -> Shaft {
    let rect = Shaft {
        left: column - 1,
        top: seabed,
        right: column + 1,
        bottom: seabed + SHAFT_ROWS,
    };
    {
        let terrain = world.terrain_mut();
        for y in rect.top..=rect.bottom {
            for x in rect.left..=rect.right {
                let _ = terrain.set_kind(TilePosition { x, y }, TileKind::Air);
            }
        }
    }
    world.wake_fluids(TileRect {
        min: TilePosition {
            x: rect.left - 1,
            y: rect.top - 2,
        },
        max: TilePosition {
            x: rect.right + 1,
            y: rect.bottom,
        },
    });
    rect
}

fn rest(world: &mut WorldState) {
    let tick = world.simulation_tick();
    let _ = world.simulate_tick(
        tick,
        &TickInputs::held(PLAYER, ControlState::new(0.0, false, false)),
    );
}

/// The water round the shaft and the fluid schedule: what the step decided.
fn fluid_digest(world: &WorldState, around: Shaft) -> u64 {
    let mut hasher = StateHasher::new();
    for y in around.top - 30..=around.bottom + 2 {
        for x in around.left - 400..=around.right + 400 {
            let tile = world.terrain().tile(TilePosition { x, y });
            hasher.write_u64(tile.map_or(u64::MAX, crate::terrain::Tile::code));
            hasher.write_u64(tile.map_or(0, |tile| u64::from(tile.durability)));
        }
    }
    hasher.write_u64(world.fluids.awake_count() as u64);
    for position in world.fluids.woken() {
        hasher.write_u64(u64::from(position.x.cast_unsigned()));
        hasher.write_u64(u64::from(position.y.cast_unsigned()));
    }
    hasher.finish().get()
}

/// Ticks a fresh ocean world with a new shaft, returning the digest after every tick.
fn drain(hz: u32, ticks: u32, reference: bool) -> Vec<u64> {
    super::fluids::use_reference_step(reference);
    let mut world = ocean_world(hz);
    let (column, seabed) = open_sea(&world);
    let planet = *world.planet().expect("planet");
    let island = world_of(PlanetThemeId::OCEAN)
        .and_then(|theme| theme.settlement_site(&planet))
        .expect("an island");
    let (x, y) = world.frame().centre_of(island.centre(), island.floor - 2);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.x = x;
        rig.y = y;
    }
    world.stream_terrain_near_players();
    let rect = sink_shaft(&mut world, column, seabed);
    let digests = (0..ticks)
        .map(|_| {
            rest(&mut world);
            fluid_digest(&world, rect)
        })
        .collect();
    super::fluids::use_reference_step(false);
    digests
}

/// The faster step is the reference step, tick for tick: the same water in the same tiles and
/// the same cells left awake, so saves, replays and replicas agree with every earlier build.
#[test]
fn the_faster_fluid_step_drains_the_sea_exactly_like_the_reference() {
    for hz in [30, 60] {
        let fast = drain(hz, 240, false);
        let reference = drain(hz, 240, true);
        for (tick, (a, b)) in fast.iter().zip(&reference).enumerate() {
            assert_eq!(a, b, "{hz} Hz: the steps part at tick {tick}");
        }
    }
}

/// The cost of a draining sea (release): `cargo test --release -p fatherload_simulation
/// sea_drain_cost -- --ignored --nocapture`.
#[test]
#[ignore = "a timing report"]
fn sea_drain_cost() {
    let which = std::env::var("SEA_DRAIN_STEP").unwrap_or_default();
    let steps: &[bool] = match which.as_str() {
        "fast" => &[false],
        "reference" => &[true],
        _ => &[true, false],
    };
    for &reference in steps {
        super::fluids::use_reference_step(reference);
        let mut world = ocean_world(30);
        let (column, seabed) = open_sea(&world);
        let rect = sink_shaft(&mut world, column, seabed);
        let (mut total, mut worst) = (0_u128, 0_u128);
        let ticks: u32 = 30 * 60;
        let mut woken = 0;
        for tick in 0..ticks {
            let started = std::time::Instant::now();
            rest(&mut world);
            let spent = started.elapsed().as_micros();
            total += spent;
            worst = worst.max(spent);
            woken = woken.max(world.fluids.awake_count());
            if tick % 300 == 299 {
                eprintln!(
                    "  {}s: {} woken now, mean {} us/tick so far",
                    (tick + 1) / 30,
                    world.fluids.awake_count(),
                    total / u128::from(tick + 1)
                );
            }
        }
        let _ = rect;
        eprintln!(
            "reference {reference}: mean {} us/tick, worst {worst} us, peak {woken} woken",
            total / u128::from(ticks)
        );
    }
    super::fluids::use_reference_step(false);
}
