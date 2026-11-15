//! Altitude physics in a V8 sky (expansion plan F6): gravity fields on rigs, drops and bombs,
//! updrafts, wind, cold, band reachability in the real sky, precision at the Moon, saves and
//! replication.

#![allow(
    clippy::suboptimal_flops,
    clippy::cast_possible_truncation,
    reason = "test arithmetic reads best as written"
)]

use crate::{
    ControlState, PlayerId, SimulationParams, SimulationTick, TickInputs, WorldState,
    environment::{self, MOON_GRAVITY, ORBIT_GRAVITY},
    hazard::PlacedBomb,
    item::{ItemKind, ItemStack},
    physics::BodyParams,
    player::fuel_capacity_for_level,
    sky::SkyTile,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        sky::{
            self, CONTINENTS_BOTTOM_ROWS, DEAD_AIR_BOTTOM_ROWS, LOW_ISLES_BOTTOM_ROWS,
            MOON_BOTTOM_ROWS, ORBIT_BOTTOM_ROWS, SKY_CEILING_ROWS,
        },
    },
};

const PLAYER: PlayerId = PlayerId::new(1);

fn world() -> WorldState {
    WorldState::new_game_with_params(PLAYER, SimulationParams::new(60))
}

#[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
fn pixel(tile: i32) -> f32 {
    (tile as f32 + 0.5) * TILE_SIZE
}

/// World pixel y of `altitude_rows` rows above row 0 (the middle of that row).
#[allow(clippy::cast_precision_loss, reason = "small altitudes")]
fn y_of(altitude_rows: i32) -> f32 {
    (-(altitude_rows as f32) + 0.5) * TILE_SIZE
}

/// A column with no land and no updraft between `from` and `to` (altitudes), away from the
/// Moon.
fn clear_column(settings: &WorldSettings, from: i32, to: i32) -> i32 {
    let moon = sky::moon_column(settings).expect("a moon");
    let away = if moon > 0 { -1 } else { 1 };
    (0..4_000)
        .map(|step| away * (300 + step * 3))
        .find(|x| {
            let rect = TileRect {
                min: TilePosition {
                    x: *x - 2,
                    y: sky::row_of(to),
                },
                max: TilePosition {
                    x: *x + 2,
                    y: sky::row_of(from),
                },
            };
            sky::lands_touching(settings, &rect).is_empty()
                && sky::updrafts_touching(settings, &rect).is_empty()
        })
        .expect("a clear column")
}

/// Puts the rig at rest at `(column, altitude)` with a full tank.
fn place(world: &mut WorldState, column: i32, altitude: i32) {
    let player = world.player_mut(PLAYER).expect("player");
    player.x = pixel(column);
    player.x_remainder = 0.0;
    player.y = y_of(altitude);
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    player.fuel = player.fuel_capacity;
}

fn fit(world: &mut WorldState, tank: u8, engine: u8, intake: u8) {
    let player = world.player_mut(PLAYER).expect("player");
    player.fuel_tank_level = tank;
    player.fuel_capacity = fuel_capacity_for_level(tank);
    player.fuel = player.fuel_capacity;
    player.engine_level = engine;
    player.hazard_gear.altitude_intake = intake;
}

fn step(world: &mut WorldState, steps: u32, thrust: bool) {
    for _ in 0..steps {
        let _ = world.step_authoritative_movement(PLAYER, 0.0, thrust);
    }
}

fn tick(world: &mut WorldState, thrust: bool) {
    let next = world.simulation_tick().next();
    let inputs = TickInputs::held(PLAYER, ControlState::new(0.0, thrust, false));
    let _ = world.simulate_tick(next, &inputs);
}

#[test]
fn orbit_is_nearly_weightless_and_the_low_sky_is_not() {
    let mut world = world();
    let settings = *world.world_settings();
    let low = clear_column(&settings, 400, 500);
    place(&mut world, low, 450);
    step(&mut world, 60, false);
    let low_fall = world.player(PLAYER).expect("player").velocity_y;
    let high = clear_column(&settings, ORBIT_BOTTOM_ROWS + 300, ORBIT_BOTTOM_ROWS + 500);
    place(&mut world, high, ORBIT_BOTTOM_ROWS + 400);
    step(&mut world, 60, false);
    let orbit_fall = world.player(PLAYER).expect("player").velocity_y;
    assert!(low_fall > 250.0, "falls in the low sky: {low_fall}");
    assert!(
        orbit_fall < 440.0 * ORBIT_GRAVITY * 1.2 && orbit_fall > 0.0,
        "drifts in orbit: {orbit_fall}"
    );
    // No air in orbit: a sideways drift keeps going.
    let player = world.player_mut(PLAYER).expect("player");
    player.velocity_x = 90.0;
    step(&mut world, 120, false);
    let drift = world.player(PLAYER).expect("player").velocity_x;
    assert!(drift > 85.0, "inertia-heavy drift: {drift}");
}

#[test]
fn the_moon_has_low_gravity_on_top() {
    let mut world = world();
    let settings = *world.world_settings();
    let moon = sky::moon(&settings).expect("a moon");
    let surface = moon.surface_row(moon.center_x).expect("covers its centre");
    // Free fall from rest above the surface.
    place(&mut world, moon.center_x, -(surface - 20));
    step(&mut world, 60, false);
    let fall = world.player(PLAYER).expect("player").velocity_y;
    assert!(
        (fall - 440.0 * MOON_GRAVITY).abs() < 20.0,
        "a second of lunar fall: {fall}"
    );
    // The rig lands and stands on the regolith.
    step(&mut world, 60 * 8, false);
    let rig = world.player(PLAYER).expect("player");
    let below = rig.tile_position(TILE_SIZE);
    assert_eq!(
        world
            .terrain()
            .tile(TilePosition {
                x: below.x,
                y: below.y + 1
            })
            .map(|tile| tile.kind),
        Some(TileKind::Sky(SkyTile::Regolith)),
        "standing on the Moon at {below:?}"
    );
    assert!(rig.velocity_y.abs() < f32::EPSILON);
}

#[test]
fn gravity_bombs_and_drops_feel_the_fields() {
    let mut world = world();
    let settings = *world.world_settings();
    let moon = sky::moon(&settings).expect("a moon");
    let surface = moon.surface_row(moon.center_x).expect("covers its centre");
    let terrain = world.terrain();
    let on_moon = BodyParams::BOMB.at(terrain, pixel(moon.center_x), pixel(surface - 5));
    assert!((on_moon.gravity - BodyParams::BOMB.gravity * MOON_GRAVITY).abs() < 1e-3);
    let ground = BodyParams::BOMB.at(terrain, pixel(10), pixel(2));
    assert_eq!(ground, BodyParams::BOMB, "unchanged on the ground");
    let column = clear_column(&settings, ORBIT_BOTTOM_ROWS + 300, ORBIT_BOTTOM_ROWS + 500);
    let orbit = BodyParams::DROP.at(terrain, pixel(column), y_of(ORBIT_BOTTOM_ROWS + 400));
    assert!(orbit.gravity < BodyParams::DROP.gravity * 0.05);
    // A bomb and a drop dropped in orbit barely fall in a second.
    let start_y = y_of(ORBIT_BOTTOM_ROWS + 400);
    world.insert_charge(PlacedBomb {
        id: 1,
        x: pixel(column),
        y: start_y,
        previous_x: pixel(column),
        previous_y: start_y,
        detonation_tick: SimulationTick::new(100_000),
        timer_seconds: 1_000.0,
        ..PlacedBomb::default()
    });
    let drop = world.spawn_item_drop(
        pixel(column + 3),
        start_y,
        (0.0, 0.0),
        ItemStack::new(ItemKind::Block(crate::item::BlockKind::Stone), 1),
        None,
    );
    // Keep the rig far away so it does not collect the drop.
    place(&mut world, column + 400, 300);
    for _ in 0..60 {
        tick(&mut world, false);
    }
    let bomb = world.bombs().first().copied().expect("the bomb");
    assert!(
        bomb.y - start_y < 20.0,
        "the bomb floats: {}",
        bomb.y - start_y
    );
    let drop = world.entities().get(drop).expect("the drop");
    assert!(drop.body.y - start_y < 20.0, "the drop floats");
}

#[test]
fn an_updraft_lifts_a_rig_without_thrust() {
    let mut world = world();
    let settings = *world.world_settings();
    let rect = TileRect {
        min: TilePosition {
            x: -3_000,
            y: sky::row_of(DEAD_AIR_BOTTOM_ROWS),
        },
        max: TilePosition {
            x: 3_000,
            y: sky::row_of(LOW_ISLES_BOTTOM_ROWS),
        },
    };
    let updraft = sky::updrafts_touching(&settings, &rect)
        .into_iter()
        .find(|updraft| {
            sky::lands_touching(&settings, &updraft.rect.expanded(3)).is_empty()
                && updraft.rect.height() > 60
        })
        .expect("an open updraft");
    let column = i32::midpoint(updraft.rect.min.x, updraft.rect.max.x);
    place(&mut world, column, -(updraft.rect.max.y - 10));
    let start = world.player(PLAYER).expect("player").y;
    step(&mut world, 90, false);
    let rig = world.player(PLAYER).expect("player");
    assert!(rig.y < start - TILE_SIZE * 3.0, "rose {} px", start - rig.y);
    assert!(
        (rig.fuel - rig.fuel_capacity).abs() < f32::EPSILON,
        "for free"
    );
}

#[test]
fn wind_carries_a_coasting_rig() {
    let mut world = world();
    let settings = *world.world_settings();
    // The windiest layer of the gap over a clear column.
    let column = clear_column(
        &settings,
        DEAD_AIR_BOTTOM_ROWS,
        CONTINENTS_BOTTOM_ROWS - 200,
    );
    #[allow(clippy::cast_precision_loss, reason = "small altitudes")]
    let altitude = (DEAD_AIR_BOTTOM_ROWS..CONTINENTS_BOTTOM_ROWS - 200)
        .step_by(16)
        .max_by_key(|rows| {
            (environment::wind_at(&settings, column, *rows as f32).abs() * 10.0) as i32
        })
        .expect("a layer");
    fit(&mut world, 15, 15, 6);
    place(&mut world, column, altitude);
    // Hover-ish with thrust taps while the wind takes hold.
    for index in 0..240 {
        let _ = world.step_authoritative_movement(PLAYER, 0.0, index % 3 == 0);
    }
    let rig = world.player(PLAYER).expect("player");
    let env = environment::environment_at(&settings, rig.x, rig.y);
    assert!(env.wind_x.abs() > 40.0, "a real crosswind: {}", env.wind_x);
    assert!(
        rig.velocity_x.signum() == env.wind_x.signum()
            && rig.velocity_x.abs() > env.wind_x.abs() * 0.5,
        "carried along: {} in a {} wind",
        rig.velocity_x,
        env.wind_x
    );
}

#[test]
fn cold_ices_the_hull_unless_the_de_icer_covers_it() {
    let mut damage = Vec::new();
    for de_icer in [0, environment::DE_ICER_MAX_LEVEL] {
        let mut world = world();
        let settings = *world.world_settings();
        let column = clear_column(
            &settings,
            DEAD_AIR_BOTTOM_ROWS + 1_000,
            DEAD_AIR_BOTTOM_ROWS + 1_300,
        );
        fit(&mut world, 15, 15, 6);
        world
            .player_mut(PLAYER)
            .expect("player")
            .hazard_gear
            .de_icer = de_icer;
        place(&mut world, column, DEAD_AIR_BOTTOM_ROWS + 1_200);
        let before = world.player(PLAYER).expect("player").hull;
        for _ in 0..120 {
            tick(&mut world, false);
        }
        damage.push(before - world.player(PLAYER).expect("player").hull);
    }
    assert!(damage[0] > 4.0, "two seconds of bitter cold: {}", damage[0]);
    assert!(damage[1].abs() < f32::EPSILON, "covered: {}", damage[1]);
}

#[test]
fn the_real_sky_unlocks_by_tier() {
    let settings = WorldSettings::default();
    let climb = |tank: u8, engine: u8, intake: u8, column: i32| {
        let mut world = world();
        fit(&mut world, tank, engine, intake);
        place(
            &mut world,
            column,
            crate::world::climb_tests::START_ALTITUDE_ROWS,
        );
        let mut highest = f32::MAX;
        for index in 0_u32.. {
            if index % 32 == 0 {
                let tile = world
                    .player(PLAYER)
                    .expect("player")
                    .tile_position(TILE_SIZE);
                let _ = world.terrain_mut().stream_around(&[tile]);
            }
            // Steer against the crosswinds to hold the column, as a pilot would.
            let drift =
                (pixel(column) - world.player(PLAYER).expect("player").x) / (TILE_SIZE * 4.0);
            let _ = world.step_authoritative_movement(PLAYER, drift.clamp(-1.0, 1.0), true);
            let rig = world.player(PLAYER).expect("player");
            highest = highest.min(rig.y);
            if (rig.fuel <= 0.0 && rig.velocity_y >= 0.0) || index > 2_000_000 {
                break;
            }
        }
        #[allow(clippy::cast_possible_truncation, reason = "altitudes")]
        let rows = (-highest / TILE_SIZE).floor() as i32;
        rows
    };
    let clear = clear_column(&settings, 100, SKY_CEILING_ROWS - 200);
    assert!(climb(1, 1, 0, clear) < LOW_ISLES_BOTTOM_ROWS, "a stock rig");
    assert!(
        climb(3, 4, 0, clear) >= LOW_ISLES_BOTTOM_ROWS,
        "tier 3 reaches the isles"
    );
    assert!(
        climb(15, 15, 0, clear) < CONTINENTS_BOTTOM_ROWS,
        "no intakes: stuck under the gap"
    );
    let near_max = climb(13, 15, 6, clear);
    assert!(
        near_max >= CONTINENTS_BOTTOM_ROWS,
        "a near-max rig with intakes crosses the gap: {near_max} < {CONTINENTS_BOTTOM_ROWS}"
    );
    // A maxed rig under the Moon flies up into its underside.
    let moon = sky::moon(&settings).expect("a moon");
    let under = (0..400)
        .map(|step| moon.center_x + if step % 2 == 0 { step * 3 } else { -step * 3 })
        .find(|x| {
            let rect = TileRect {
                min: TilePosition {
                    x: *x - 3,
                    y: sky::row_of(MOON_BOTTOM_ROWS - 40),
                },
                max: TilePosition { x: *x + 3, y: -100 },
            };
            sky::lands_touching(&settings, &rect).is_empty()
                && sky::updrafts_touching(&settings, &rect).is_empty()
        })
        .expect("a clear column under the Moon");
    let reached = climb(15, 15, 6, under);
    assert!(
        reached >= MOON_BOTTOM_ROWS - 20,
        "reached {reached} under the Moon"
    );
}

#[test]
fn positions_stay_precise_at_moon_height() {
    // T8 measured about half a pixel of f32 spacing at the numeric edge of space; the Moon's
    // top is far lower, where a world pixel y still resolves a sixteenth of a pixel or better.
    #[allow(clippy::cast_precision_loss, reason = "a whole-row altitude")]
    let top = -((SKY_CEILING_ROWS) as f32) * TILE_SIZE;
    let spacing = top.next_down() - top;
    assert!(spacing.abs() <= 1.0 / 16.0, "{spacing}");
    // A rig dropped on the Moon falls the same distance, to a sixteenth of a pixel, as the
    // ideal integration of the same motion.
    let mut world = world();
    let settings = *world.world_settings();
    let moon = sky::moon(&settings).expect("a moon");
    let surface = moon.surface_row(moon.center_x).expect("covers its centre");
    place(&mut world, moon.center_x, -(surface - 30));
    let start = world.player(PLAYER).expect("player").y;
    let timing = world.timing;
    let mut velocity = 0.0_f32;
    let mut ideal = 0.0_f64;
    for _ in 0..120 {
        let _ = world.step_authoritative_movement(PLAYER, 0.0, false);
        let rig = world.player(PLAYER).expect("player");
        let env = environment::environment_at(&settings, rig.x, rig.y);
        velocity = crate::quantize::quantize_simulation_scalar(
            env.body_gravity(super::AUTHORITATIVE_RIG_GRAVITY).mul_add(
                timing.vertical_acceleration_gain,
                velocity * env.drag_retention(timing.vertical_drag_per_tick),
            ),
        );
        ideal += f64::from(velocity) * f64::from(timing.delta_seconds);
    }
    let fallen = f64::from(world.player(PLAYER).expect("player").y - start);
    assert!(
        (fallen - ideal).abs() < 1.0,
        "fell {fallen} px, ideal {ideal} px"
    );
}

#[test]
fn island_edits_survive_saves_and_keyframes() {
    let mut world = world();
    let settings = *world.world_settings();
    let rect = TileRect {
        min: TilePosition {
            x: -2_000,
            y: sky::row_of(DEAD_AIR_BOTTOM_ROWS),
        },
        max: TilePosition {
            x: 2_000,
            y: sky::row_of(LOW_ISLES_BOTTOM_ROWS),
        },
    };
    let island = sky::lands_touching(&settings, &rect)
        .into_iter()
        .next()
        .expect("an island");
    let top = island
        .surface_row(island.center_x)
        .expect("covers its centre");
    let position = TilePosition {
        x: island.center_x,
        y: top,
    };
    assert!(world.terrain().is_solid_at(position));
    assert!(world.terrain_mut().set_kind(position, TileKind::Air));
    // A half-drilled sky tile (still sky rock, fewer chips left) is a diff too: its code must
    // round-trip through saves and keyframes.
    let chipped = TilePosition {
        x: island.center_x,
        y: top + 4,
    };
    let fresh = world.terrain().tile(chipped).expect("in the world");
    assert!(matches!(fresh.kind, TileKind::Sky(_)), "{fresh:?}");
    let _ = world.terrain_mut().chip(chipped);
    let chipped_tile = world.terrain().tile(chipped).expect("in the world");
    assert!(matches!(chipped_tile.kind, TileKind::Sky(_)));
    assert!(chipped_tile.durability < fresh.durability);
    world
        .player_mut(PLAYER)
        .expect("player")
        .hazard_gear
        .altitude_intake = 4;
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(
        loaded.terrain().tile(position).map(|tile| tile.kind),
        Some(TileKind::Air)
    );
    assert_eq!(loaded.terrain().tile(chipped), Some(chipped_tile));
    assert_eq!(
        loaded
            .player(PLAYER)
            .expect("player")
            .hazard_gear
            .altitude_intake,
        4
    );
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    // A client keyframe carries the island edit too.
    let keyframe = world.redacted_for(Some(PLAYER));
    let replica: WorldState =
        serde_json::from_str(&serde_json::to_string(&keyframe).expect("keyframe")).expect("back");
    assert_eq!(
        replica.terrain().tile(position).map(|tile| tile.kind),
        Some(TileKind::Air)
    );
    assert_eq!(replica.terrain().tile(chipped), Some(chipped_tile));
    // Older saves without the sky gear load with none fitted.
    let gear: crate::protection::HazardGear =
        serde_json::from_str(r#"{"heat_shield":2}"#).expect("old gear");
    assert_eq!((gear.altitude_intake, gear.de_icer), (0, 0));
}

#[test]
fn sky_flight_is_deterministic_and_tick_rate_safe() {
    let run = |hz: u32| {
        let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(hz));
        let settings = *world.world_settings();
        let column = clear_column(&settings, 600, 1_400);
        fit(&mut world, 8, 8, 2);
        place(&mut world, column, 700);
        for index in 0..hz * 6 {
            let thrust = !(index / (hz / 4)).is_multiple_of(3);
            let _ = world.step_authoritative_movement(PLAYER, 0.5, thrust);
        }
        world
    };
    let (a, b) = (run(60), run(60));
    assert_eq!(a.simulation_state_hash(), b.simulation_state_hash());
    let (fast, slow) = (run(60), run(30));
    let (fast, slow) = (
        fast.player(PLAYER).expect("player"),
        slow.player(PLAYER).expect("player"),
    );
    assert!(
        (fast.y - slow.y).abs() < TILE_SIZE * 3.0 && (fast.x - slow.x).abs() < TILE_SIZE * 3.0,
        "60 Hz ({}, {}) vs 30 Hz ({}, {})",
        fast.x,
        fast.y,
        slow.x,
        slow.y
    );
}
