//! The climb harness (expansion plan F6): how high each fuel tank, engine and intake tier
//! climbs, the table the sky bands are defined from ([`crate::climb`]), and reachability of the
//! bands in a real V8 sky.

use crate::{
    PlayerId, SimulationParams, WorldState,
    climb::{CLIMB_CEILINGS, ENGINE_TIERS, INTAKE_TIERS, TANK_TIERS, ceiling},
    environment,
    player::fuel_capacity_for_level,
    terrain::{TILE_SIZE, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        sky::{
            self, ARCHIPELAGO_BOTTOM_ROWS, CONTINENTS_BOTTOM_ROWS, LOW_ISLES_BOTTOM_ROWS,
            MOON_BOTTOM_ROWS, SKY_CEILING_ROWS,
        },
    },
};

const PLAYER: PlayerId = PlayerId::new(1);
/// The harness starts every climb at rest this many rows above row 0 (above every peak).
pub const START_ALTITUDE_ROWS: i32 = 100;

/// A loadout to climb with.
#[derive(Clone, Copy, Debug)]
struct Loadout {
    tank: u8,
    engine: u8,
    intake: u8,
}

/// A column of `settings` whose sky holds no land and no updraft from the ground up to the
/// sky's ceiling, searched outward from `near` (on the side away from the Moon).
fn clear_column(settings: &WorldSettings, near: i32) -> i32 {
    let away = sky::moon_column(settings).map_or(1, |moon| if moon > near { -1 } else { 1 });
    (0..4_000)
        .map(|step| near + away * step * 3)
        .find(|x| {
            let rect = TileRect {
                min: TilePosition {
                    x: *x - 2,
                    y: -SKY_CEILING_ROWS,
                },
                max: TilePosition {
                    x: *x + 2,
                    y: -START_ALTITUDE_ROWS,
                },
            };
            sky::lands_touching(settings, &rect).is_empty()
                && sky::updrafts_touching(settings, &rect).is_empty()
        })
        .expect("a clear column")
}

/// A world with one rig fitted with `loadout`, a full tank, at rest at `START_ALTITUDE_ROWS`
/// above column `column`.
fn rig_world(loadout: Loadout, column: i32, hz: u32) -> WorldState {
    let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(hz));
    let player = world.player_mut(PLAYER).expect("player");
    player.fuel_tank_level = loadout.tank;
    player.fuel_capacity = fuel_capacity_for_level(loadout.tank);
    player.fuel = player.fuel_capacity;
    player.engine_level = loadout.engine;
    player.hazard_gear.altitude_intake = loadout.intake;
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        player.x = (column as f32 + 0.5) * TILE_SIZE;
        player.y = -(START_ALTITUDE_ROWS as f32) * TILE_SIZE;
    }
    player.x_remainder = 0.0;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    world
}

/// Holds thrust until the tank is dry and the rig starts falling; the highest altitude
/// reached, in rows above row 0.
#[allow(
    clippy::cast_possible_truncation,
    reason = "altitudes are a few thousand rows"
)]
fn climb(world: &mut WorldState) -> i32 {
    let mut highest = f32::MAX;
    for step in 0_u64.. {
        if step % 32 == 0 {
            let tile = world
                .player(PLAYER)
                .expect("player")
                .tile_position(TILE_SIZE);
            let _ = world.terrain_mut().stream_around(&[tile]);
        }
        let _ = world.step_authoritative_movement(PLAYER, 0.0, true);
        let rig = world.player(PLAYER).expect("player");
        highest = highest.min(rig.y);
        if rig.fuel <= 0.0 && rig.velocity_y >= 0.0 {
            break;
        }
        assert!(step < 10_000_000, "a climb that never ends");
    }
    (-highest / TILE_SIZE).floor() as i32
}

/// The still-air ceiling of `loadout` (the climb table's measurement).
fn measure(loadout: Loadout, hz: u32) -> i32 {
    let settings = WorldSettings::default();
    let column = clear_column(&settings, -600);
    environment::with_still_air(|| {
        let mut world = rig_world(loadout, column, hz);
        climb(&mut world) / 10 * 10
    })
}

#[test]
fn climb_table_matches_the_simulation() {
    // A sample across the table: the bands' defining entries and both ends.
    for (tank, engine, intake) in [
        (1, 1, 0),
        (2, 4, 0),
        (4, 4, 0),
        (10, 8, 2),
        (15, 15, 0),
        (13, 15, 6),
        (15, 15, 6),
    ] {
        let measured = measure(
            Loadout {
                tank,
                engine,
                intake,
            },
            60,
        );
        let table = ceiling(tank, engine, intake);
        assert!(
            (measured - table).abs() <= table / 50 + 10,
            "tank {tank}, engine {engine}, intakes {intake}: measured {measured}, table {table} \
             (re-run climb_table_report and update crate::climb)"
        );
    }
}

#[test]
#[ignore = "report: measures the whole climb table (run in release, paste into crate::climb)"]
fn climb_table_report() {
    println!("pub const CLIMB_CEILINGS: [[[i32; 4]; 5]; 15] = [");
    for tank in TANK_TIERS {
        print!("    [");
        for (index, engine) in ENGINE_TIERS.iter().enumerate() {
            let row: Vec<String> = INTAKE_TIERS
                .iter()
                .map(|intake| {
                    measure(
                        Loadout {
                            tank,
                            engine: *engine,
                            intake: *intake,
                        },
                        60,
                    )
                    .to_string()
                })
                .collect();
            print!("{}[{}]", if index == 0 { "" } else { ", " }, row.join(", "));
        }
        println!("], // tank {tank}");
    }
    println!("];");
    let _ = CLIMB_CEILINGS;
}

#[test]
fn the_table_unlocks_the_sky_in_order() {
    // Design claims of the bands, read straight off the table.
    assert!(
        ceiling(1, 1, 0) < LOW_ISLES_BOTTOM_ROWS,
        "a stock rig stays below the isles"
    );
    assert!(
        ceiling(2, 4, 0) >= LOW_ISLES_BOTTOM_ROWS,
        "tank 2 reaches the low isles"
    );
    assert!(
        ceiling(4, 4, 0) >= ARCHIPELAGO_BOTTOM_ROWS,
        "tank 4 reaches the archipelago"
    );
    assert!(
        ceiling(15, 15, 0) < CONTINENTS_BOTTOM_ROWS,
        "without intakes no tank crosses the dead-air gap ({} < {CONTINENTS_BOTTOM_ROWS})",
        ceiling(15, 15, 0)
    );
    assert!(
        ceiling(13, 15, 6) >= CONTINENTS_BOTTOM_ROWS,
        "a near-max tank with top intakes crosses it"
    );
    assert!(
        ceiling(12, 15, 6) < CONTINENTS_BOTTOM_ROWS,
        "a tier 12 tank does not, even with top intakes"
    );
    assert!(
        ceiling(15, 15, 6) >= MOON_BOTTOM_ROWS,
        "a maxed rig reaches the Moon"
    );
}
