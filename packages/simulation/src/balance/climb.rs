//! The climb probe: how high a rig gets on one tank, per fuel-tank and engine tier.
//!
//! A rig of the given tiers stands on the surface in a gap between the town's buildings and
//! holds thrust straight up through the real tick until the tank is dry and it starts to fall.
//! The probe records the apex (tiles above the surface), the time to it, and the *turn-back
//! altitude*: how high it was when half the tank was gone, the most a player can climb and
//! still have the other half to come back down under control.
//!
//! The plan's sky bands are placed as fractions of these measurements (expansion plan §9, "the
//! climb harness"). The probe reads nothing but the tick, so once altitude physics (air
//! density, cold, gravity fields: Wave 3 track T16) lands, its numbers follow automatically;
//! T16's own climb harness stays the authority for band placement, and this table is the
//! balance report's view of it.

use super::{
    BalanceProbe, ProbeContext, ReportSection, Table,
    economy::{BOT_PLAYER, inputs_for, new_world},
    grouped, run_parallel,
};
use crate::{
    ControlState, WorldState,
    economy::UpgradeKind,
    economy::{MAX_UPGRADE_LEVEL, upgrade_cost},
    player::fuel_capacity_for_level,
    terrain::TILE_SIZE,
};

/// Fuel-tank tiers the table compares.
pub const TANK_TIERS: [u8; 5] = [1, 3, 6, 10, MAX_UPGRADE_LEVEL];
/// Engine tiers the table compares.
pub const ENGINE_TIERS: [u8; 3] = [1, 6, MAX_UPGRADE_LEVEL];
/// The column the rig climbs from (the economy bot's first shaft: a gap between buildings).
const CLIMB_COLUMN: i32 = 78;
/// Longest climb simulated (game seconds).
const MAX_CLIMB_SECONDS: u32 = 2_400;

/// One climb's outcome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimbResult {
    pub tank: u8,
    pub engine: u8,
    /// Fuel the tank holds.
    pub fuel: f32,
    /// Highest point reached, in tiles above the surface.
    pub apex_tiles: i32,
    /// Seconds from lift-off to the apex.
    pub apex_seconds: f32,
    /// Altitude (tiles above the surface) when half the tank was burned.
    pub turn_back_tiles: i32,
}

/// Tiles above the surface row of `world`'s climb column for a rig at `y` (pixels).
fn tiles_above(world: &WorldState, y: f32) -> i32 {
    world.terrain().surface_row(CLIMB_COLUMN) - (y / TILE_SIZE).floor() as i32
}

/// Climbs a rig with fuel tank `tank` and engine `engine` straight up from the surface of
/// `seed`'s world until the tank runs dry.
#[must_use]
pub fn climb(seed: u64, tank: u8, engine: u8, tick_rate_hz: u32) -> Option<ClimbResult> {
    let mut world = new_world(seed, tick_rate_hz);
    world.set_enemy_difficulty(crate::EnemyDifficulty::Peaceful);
    let surface = world.terrain().surface_row(CLIMB_COLUMN);
    let fuel = fuel_capacity_for_level(tank);
    {
        let rig = world.player_mut(BOT_PLAYER)?;
        rig.x = (CLIMB_COLUMN as f32 + 0.5) * TILE_SIZE;
        rig.y = surface as f32 * TILE_SIZE - 11.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.fuel_tank_level = tank;
        rig.fuel_capacity = fuel;
        rig.fuel = fuel;
        rig.engine_level = engine;
    }
    let hz = world.tick_rate_hz();
    let column_x = (CLIMB_COLUMN as f32 + 0.5) * TILE_SIZE;
    let mut apex = world.player(BOT_PLAYER)?.y;
    let mut apex_tick = 0;
    let mut turn_back = None;
    for tick in 0..MAX_CLIMB_SECONDS * hz {
        let rig = world.player(BOT_PLAYER)?;
        let horizontal = ((column_x - rig.x) / TILE_SIZE).clamp(-1.0, 1.0) * 0.3;
        let _ = world.simulate_tick(
            world.simulation_tick(),
            &inputs_for(
                BOT_PLAYER,
                ControlState::new(horizontal, true, false),
                Vec::new(),
            ),
        );
        let rig = world.player(BOT_PLAYER)?;
        if rig.y < apex {
            apex = rig.y;
            apex_tick = tick + 1;
        }
        if turn_back.is_none() && rig.fuel <= fuel * 0.5 {
            turn_back = Some(tiles_above(&world, rig.y));
        }
        if rig.fuel <= 0.0 && rig.velocity_y > 0.0 {
            break;
        }
    }
    Some(ClimbResult {
        tank,
        engine,
        fuel,
        apex_tiles: tiles_above(&world, apex),
        apex_seconds: apex_tick as f32 / hz as f32,
        turn_back_tiles: turn_back.unwrap_or_else(|| tiles_above(&world, apex)),
    })
}

/// Credits to buy fuel-tank tiers up to `tank` from stock.
fn tank_price(tank: u8) -> u32 {
    (1..tank)
        .map(|level| upgrade_cost(UpgradeKind::FuelTank, level))
        .sum()
}

/// The climb section (the sky probe hook of the plan's T19 row).
pub struct ClimbProbe;

impl BalanceProbe for ClimbProbe {
    fn name(&self) -> &'static str {
        "climb"
    }

    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let hz = context.config.tick_rate_hz;
        let seed = context.config.seeds.first().copied().unwrap_or(11);
        let mut section = ReportSection::new(
            "Climb",
            "A rig stands on the surface between the town's buildings and holds thrust straight \
             up until the tank is dry (real tick, one seed: the open sky does not depend on it). \
             \"Apex\" is the highest point in tiles above the surface; \"turn back\" is the \
             altitude with half the tank burned, the most a player can climb and still come \
             down under power. Sky bands are meant to be placed as fractions of these.",
        );
        let mut headers = vec!["Fuel tank (fuel, price from stock)".to_owned()];
        for engine in ENGINE_TIERS {
            headers.push(format!("Engine {engine}: apex / turn back / s"));
        }
        let mut table = Table {
            caption: "Altitude on one tank, by fuel tank and engine tier:".to_owned(),
            headers,
            rows: Vec::new(),
        };
        let jobs: Vec<_> = TANK_TIERS
            .iter()
            .flat_map(|tank| ENGINE_TIERS.iter().map(move |engine| (*tank, *engine)))
            .map(|(tank, engine)| move || climb(seed, tank, engine, hz))
            .collect();
        let mut results = run_parallel(context.config.threads, jobs).into_iter();
        for tank in TANK_TIERS {
            let mut row = vec![format!(
                "Tier {tank} ({:.0}, {} cr)",
                fuel_capacity_for_level(tank),
                grouped(f64::from(tank_price(tank)))
            )];
            for engine in ENGINE_TIERS {
                let cell = results.next().flatten().map_or_else(
                    || "-".to_owned(),
                    |result| {
                        section.metric(
                            format!("climb.tank{tank}.engine{engine}.apex_tiles"),
                            f64::from(result.apex_tiles),
                        );
                        section.metric(
                            format!("climb.tank{tank}.engine{engine}.turn_back_tiles"),
                            f64::from(result.turn_back_tiles),
                        );
                        format!(
                            "{} / {} / {:.0}",
                            grouped(f64::from(result.apex_tiles)),
                            grouped(f64::from(result.turn_back_tiles)),
                            result.apex_seconds
                        )
                    },
                );
                row.push(cell);
            }
            table.row(row);
        }
        section.tables.push(table);
        section
    }
}
