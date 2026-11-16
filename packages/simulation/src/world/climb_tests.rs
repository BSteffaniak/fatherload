//! The climb harness (expansion plan F6): how high each fuel tank, engine and intake tier
//! climbs, the tables the sky bands are defined from ([`crate::climb`]): the current fuel
//! curve's (re-measured here) and V8's frozen one, and the band tables calibrated from them.

use crate::{
    PlayerId, SimulationParams, WorldState,
    climb::{
        CLIMB_CEILINGS, ClimbTable, ENGINE_TIERS, INTAKE_TIERS, TANK_TIERS, V8_CLIMB_CEILINGS,
        V9_CLIMB_CEILINGS, ceiling_in,
    },
    environment,
    player::fuel_capacity_for_level,
    terrain::{TILE_SIZE, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        sky::{
            self, ARCHIPELAGO_BOTTOM_ROWS, CONTINENTS_BOTTOM_ROWS, DEAD_AIR_BOTTOM_ROWS,
            LOW_ISLES_BOTTOM_ROWS, MOON_BOTTOM_ROWS, NEXT_SKY_BANDS, ORBIT_BOTTOM_ROWS,
            SKY_CEILING_ROWS, SkyBands, V8_SKY_BANDS,
        },
    },
};

const PLAYER: PlayerId = PlayerId::new(1);
/// The harness starts every climb at rest this many rows above row 0 (above every peak).
pub const START_ALTITUDE_ROWS: i32 = crate::climb::START_ALTITUDE_ROWS;

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
    let ceiling = sky::bands_of(settings).map_or(SKY_CEILING_ROWS, |bands| bands.ceiling_rows());
    let away = sky::moon_column(settings).map_or(1, |moon| if moon > near { -1 } else { 1 });
    (0..4_000)
        .map(|step| near + away * step * 3)
        .find(|x| {
            let rect = TileRect {
                min: TilePosition {
                    x: *x - 2,
                    y: -ceiling,
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
fn rig_world(settings: &WorldSettings, loadout: Loadout, column: i32, hz: u32) -> WorldState {
    let mut world = WorldState::new_game_with_settings(PLAYER, settings, SimulationParams::new(hz));
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

/// A world of generator `generator` (its sky's physics: V8's thin-air burn for
/// [`CLIMB_CEILINGS`], V9's steeper one for [`V9_CLIMB_CEILINGS`]).
fn settings_of(generator: GeneratorVersion) -> WorldSettings {
    WorldSettings {
        generator,
        ..WorldSettings::default()
    }
}

/// The still-air ceiling of `loadout` in `generator`'s sky (the climb table's measurement).
fn measure_in(generator: GeneratorVersion, loadout: Loadout, hz: u32) -> i32 {
    let settings = settings_of(generator);
    let column = clear_column(&settings, -600);
    environment::with_still_air(|| {
        let mut world = rig_world(&settings, loadout, column, hz);
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
        for (generator, table) in [
            (GeneratorVersion::V8, &CLIMB_CEILINGS),
            (GeneratorVersion::V9, &V9_CLIMB_CEILINGS),
        ] {
            let loadout = Loadout {
                tank,
                engine,
                intake,
            };
            let measured = measure_in(generator, loadout, 60);
            let table = ceiling_in(table, tank, engine, intake);
            assert!(
                (measured - table).abs() <= table / 50 + 10,
                "{generator:?} tank {tank}, engine {engine}, intakes {intake}: measured \
                 {measured}, table {table} (re-run the climb table reports, update crate::climb)"
            );
        }
    }
}

#[test]
#[ignore = "report: measures the whole climb table (run in release, paste into crate::climb)"]
fn climb_table_report() {
    print_climb_table(GeneratorVersion::V8, "CLIMB_CEILINGS");
}

#[test]
#[ignore = "report: measures V9's whole climb table (run in release, paste into crate::climb)"]
fn v9_climb_table_report() {
    print_climb_table(GeneratorVersion::V9, "V9_CLIMB_CEILINGS");
}

/// Measures and prints the whole climb table of `generator`'s sky as `name`.
fn print_climb_table(generator: GeneratorVersion, name: &str) {
    // One thread per tank tier: the top tanks climb for many game minutes.
    #[allow(
        clippy::needless_collect,
        reason = "every thread is spawned before any is joined"
    )]
    let rows: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = TANK_TIERS
            .iter()
            .map(|tank| {
                scope.spawn(move || {
                    let cells: Vec<String> = ENGINE_TIERS
                        .iter()
                        .map(|engine| {
                            let row: Vec<String> = INTAKE_TIERS
                                .iter()
                                .map(|intake| {
                                    measure_in(
                                        generator,
                                        Loadout {
                                            tank: *tank,
                                            engine: *engine,
                                            intake: *intake,
                                        },
                                        60,
                                    )
                                    .to_string()
                                })
                                .collect();
                            format!("[{}]", row.join(", "))
                        })
                        .collect();
                    format!("    [{}], // tank {tank}", cells.join(", "))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("climb thread"))
            .collect()
    });
    println!("pub const {name}: ClimbTable = [");
    for row in rows {
        println!("{row}");
    }
    println!("];");
}

/// The design claims of a band table calibrated from `table`.
fn assert_unlocks_in_order(table: &ClimbTable, bands: &SkyBands) {
    let ceiling = |tank, engine, intake| ceiling_in(table, tank, engine, intake);
    assert!(
        ceiling(1, 1, 0) < bands.island_reach_rows(),
        "a stock rig stays below the isles"
    );
    assert!(
        ceiling(2, 4, 0) >= bands.low_isles,
        "tank 2 reaches the low isles"
    );
    assert!(
        ceiling(4, 4, 0) >= bands.archipelago,
        "tank 4 reaches the archipelago"
    );
    assert!(
        ceiling(15, 15, 0) < bands.continents,
        "without intakes no tank crosses the dead-air gap ({} < {})",
        ceiling(15, 15, 0),
        bands.continents
    );
    assert!(
        ceiling(13, 15, 6) >= bands.continents,
        "a near-max tank with top intakes crosses it"
    );
    assert!(
        ceiling(12, 15, 6) < bands.continents,
        "a tier 12 tank does not, even with top intakes"
    );
    assert!(
        ceiling(15, 15, 6) >= bands.moon,
        "a maxed rig reaches the Moon"
    );
    assert!(
        bands.low_isles < bands.archipelago
            && bands.archipelago < bands.dead_air
            && bands.dead_air < bands.continents
            && bands.continents < bands.orbit
            && bands.orbit < bands.moon,
        "{bands:?}"
    );
}

#[test]
fn the_table_unlocks_the_sky_in_order() {
    // V9's (and the next generator's) bands, on V9's own table.
    assert_eq!(NEXT_SKY_BANDS, SkyBands::calibrated(&V9_CLIMB_CEILINGS));
    assert_unlocks_in_order(&V9_CLIMB_CEILINGS, &sky::V9_SKY_BANDS);
    // The rule on V8's physics with today's tanks would also hold its order (but put the sky
    // far too high for f32 pixels).
    assert_unlocks_in_order(&CLIMB_CEILINGS, &SkyBands::calibrated(&CLIMB_CEILINGS));
    // V8's frozen bands, on the frozen table they were calibrated from.
    assert_unlocks_in_order(&V8_CLIMB_CEILINGS, &V8_SKY_BANDS);
}

#[test]
fn v8_bands_are_frozen() {
    // V8 worlds must keep generating identically: their bands are the old calibration exactly.
    assert_eq!(
        SkyBands::calibrated(&V8_CLIMB_CEILINGS).with_climb(&CLIMB_CEILINGS),
        V8_SKY_BANDS
    );
    assert_eq!(
        (
            LOW_ISLES_BOTTOM_ROWS,
            ARCHIPELAGO_BOTTOM_ROWS,
            DEAD_AIR_BOTTOM_ROWS,
            CONTINENTS_BOTTOM_ROWS,
            ORBIT_BOTTOM_ROWS,
            MOON_BOTTOM_ROWS,
            SKY_CEILING_ROWS,
        ),
        (220, 540, 4_480, 8_080, 9_030, 10_320, 11_296)
    );
    assert_eq!(GeneratorVersion::V8.sky_bands(), Some(V8_SKY_BANDS));
    assert_eq!(GeneratorVersion::V7.sky_bands(), None);
}

#[test]
fn next_bands_are_pinned() {
    // The table a next sky generator adopts: today V9's own (a change here means the fuel
    // curve or V9's climb physics moved: re-check the design claims above).
    assert_eq!(NEXT_SKY_BANDS, sky::V9_SKY_BANDS);
}

#[test]
fn v9_bands_are_frozen() {
    // V9's bands are its own climb table's calibration, frozen as literals: they must never
    // move, even when the fuel curve or the climb table does.
    assert_eq!(GeneratorVersion::V9.sky_bands(), Some(sky::V9_SKY_BANDS));
    assert_eq!(
        sky::V9_SKY_BANDS,
        SkyBands {
            low_isles: 310,
            archipelago: 740,
            dead_air: 4_710,
            continents: 12_010,
            orbit: 13_430,
            moon: 15_360,
            climb: &V9_CLIMB_CEILINGS,
        }
    );
    assert_eq!(SkyBands::calibrated(&V9_CLIMB_CEILINGS), sky::V9_SKY_BANDS);
    // Everything in a V9 sky up to the Moon's surface and its tallest structures stays where
    // world pixels resolve 1/32 px (f32 spacing below 2^19 px), as in V8's: the pilot's
    // ledge and ladder physics was built for it.
    #[allow(clippy::cast_precision_loss, reason = "a whole-row altitude")]
    let moon_top = (sky::V9_SKY_BANDS.moon + sky::MOON_DEPTH_ROWS + 100) as f32 * TILE_SIZE;
    assert!(moon_top < 524_288.0, "V9's Moon tops out at {moon_top} px");
}
