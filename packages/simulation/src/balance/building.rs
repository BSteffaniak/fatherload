//! The building probe.
//!
//! A rig stakes claims and builds every module in a cavern through real [`BuildCommand`]s,
//! and the report shows what each cost in credits and blocks, and in minutes of the economy
//! bot's income when it reached that depth.

use super::{
    BalanceProbe, ProbeContext, ReportSection, Table,
    bot::Strategy,
    combat::band_top,
    economy::{BOT_PLAYER, EconomyRun, inputs_for},
    grouped, median,
};
use crate::{
    ControlState, PlayerCommand, SimulationParams, WorldState,
    base::{BuildCommand, Buildable, Facing, ModuleKind, beacon_price},
    item::{BlockKind, ItemKind},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    world::WorldImport,
    worldgen::Band,
};

/// Where the probe builds (deep enough to unlock every module).
const BUILD_BAND: Band = Band::Slate;
/// Column of the cavern's first claim beacon.
const CAVERN_X: i32 = 330;
/// Blocks in the hopper at the start: enough for every module, so each one is paid for in
/// blocks plus its list price and the two columns stay separate (a short hopper would buy the
/// missing blocks and fold them into the credits).
const STARTING_BLOCKS: u32 = 200;

/// One placement's outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildResult {
    pub item: ModuleKind,
    /// The ordinal of a beacon (1st, 2nd claim), 0 for other modules.
    pub nth: u32,
    pub credits: u32,
    pub blocks: u32,
    /// Refusal message, if refused.
    pub refused: Option<String>,
}

/// What the probe measured.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildingRun {
    pub results: Vec<BuildResult>,
    /// Fuel the station synthesized in one game minute (powered).
    pub station_fuel_per_minute: u32,
}

fn hopper_blocks(world: &WorldState) -> u32 {
    world.player(BOT_PLAYER).map_or(0, |rig| {
        rig.block_hopper
            .iter()
            .filter(|(kind, _)| matches!(kind, ItemKind::Block(_)))
            .map(|(_, count)| count)
            .sum()
    })
}

/// Builds two claims of modules in a cavern at [`BUILD_BAND`]'s depth.
#[must_use]
pub fn run_building(tick_rate_hz: u32) -> Option<BuildingRun> {
    let import = WorldImport::new_game(BOT_PLAYER);
    let mut world =
        WorldState::from_import_with_params(&import, SimulationParams::new(tick_rate_hz));
    let top = band_top(&world, CAVERN_X, BUILD_BAND)? + 4;
    let floor = top + 6;
    for y in top - 1..=floor + 1 {
        for x in CAVERN_X - 14..=CAVERN_X + 36 {
            let inside = y >= top && y < floor && x > CAVERN_X - 13 && x < CAVERN_X + 35;
            let kind = if inside {
                TileKind::Air
            } else {
                TileKind::Stone
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    {
        let rig = world.player_mut(BOT_PLAYER)?;
        rig.credits = 100_000;
        rig.block_hopper_capacity = STARTING_BLOCKS;
        let _ = rig.stow(ItemKind::Block(BlockKind::Stone), STARTING_BLOCKS);
    }
    // Reaching the band unlocks its modules and claims; the rig walks there in the tick.
    let park = |world: &mut WorldState, tile_x: i32| {
        if let Some(rig) = world.player_mut(BOT_PLAYER) {
            rig.x = (tile_x as f32 + 0.5) * TILE_SIZE;
            rig.y = floor as f32 * TILE_SIZE - 11.0;
            rig.velocity_x = 0.0;
            rig.velocity_y = 0.0;
        }
    };
    park(&mut world, CAVERN_X);
    for _ in 0..world.tick_rate_hz() {
        let _ = world.simulate_tick(
            world.simulation_tick(),
            &inputs_for(BOT_PLAYER, ControlState::default(), Vec::new()),
        );
    }
    let row = floor - 1;
    let second = CAVERN_X + 21;
    let plan: [(ModuleKind, i32, u32); 10] = [
        (ModuleKind::Beacon, CAVERN_X, 1),
        (ModuleKind::Generator, CAVERN_X - 3, 0),
        (ModuleKind::FuelStation, CAVERN_X - 7, 0),
        (ModuleKind::Bed, CAVERN_X - 9, 0),
        (ModuleKind::Storage, CAVERN_X + 2, 0),
        (ModuleKind::Workshop, CAVERN_X + 3, 0),
        (ModuleKind::RepairBay, CAVERN_X + 6, 0),
        (ModuleKind::Beacon, second, 2),
        (ModuleKind::Refinery, second - 4, 0),
        (ModuleKind::CargoUplink, second + 2, 0),
    ];
    let mut run = BuildingRun::default();
    for (item, x, nth) in plan {
        // Stand beside the footprint (within build reach, never on it).
        let width = item.footprint().0;
        let centre = if x < second - 10 { CAVERN_X } else { second };
        park(&mut world, if x < centre { x + width + 1 } else { x - 2 });
        let credits = world.player(BOT_PLAYER)?.credits;
        let blocks = hopper_blocks(&world);
        let command = BuildCommand::Place {
            item: Buildable::Module(item),
            at: TilePosition { x, y: row },
            facing: Facing::Right,
        };
        let refused = world
            .check_build(BOT_PLAYER, &command)
            .err()
            .map(super::super::base::BuildRefusal::message);
        let _ = world.simulate_tick(
            world.simulation_tick(),
            &inputs_for(
                BOT_PLAYER,
                ControlState::default(),
                vec![PlayerCommand::Build(command)],
            ),
        );
        run.results.push(BuildResult {
            item,
            nth,
            credits: credits.saturating_sub(world.player(BOT_PLAYER)?.credits),
            blocks: blocks.saturating_sub(hopper_blocks(&world)),
            refused,
        });
    }
    run.station_fuel_per_minute = station_fuel_per_minute(&mut world);
    Some(run)
}

/// Fuel `world`'s fuel stations synthesize in one game minute of ticks (a station starts with
/// a part-full tank, so the minute is measured as a difference).
fn station_fuel_per_minute(world: &mut WorldState) -> u32 {
    let station_fuel = |world: &WorldState| {
        world
            .bases()
            .modules()
            .filter(|module| module.kind == ModuleKind::FuelStation)
            .map(|module| module.fuel)
            .sum::<u32>()
    };
    let before = station_fuel(world);
    let ticks = world.tick_rate_hz() * 60;
    for _ in 0..ticks {
        let _ = world.simulate_tick(
            world.simulation_tick(),
            &inputs_for(BOT_PLAYER, ControlState::default(), Vec::new()),
        );
    }
    station_fuel(world).saturating_sub(before)
}

/// Median credits per minute the prospector earned around the minute it first reached band
/// `band` (a few minutes either side), if any run got there.
fn income_at_band(runs: &[EconomyRun], band: Band) -> Option<f64> {
    let rates: Vec<f64> = runs
        .iter()
        .filter(|run| run.strategy == Strategy::Prospector)
        .filter_map(|run| {
            let minute = *run.band_minutes.get(&band.index())?;
            let from = (minute as u32).saturating_sub(3);
            Some(run.earned_between(from, (from + 6).min(run.minutes)))
        })
        .collect();
    (!rates.is_empty()).then(|| median(&rates))
}

/// Claim and module costs.
pub struct BuildingProbe;

impl BalanceProbe for BuildingProbe {
    fn name(&self) -> &'static str {
        "building"
    }

    #[allow(clippy::too_many_lines, reason = "one report section, table by table")]
    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let hz = context.config.tick_rate_hz;
        let use_economy = context.config.wants("economy") || context.config.wants("progression");
        let runs: Vec<EconomyRun> = if use_economy {
            context.economy().to_vec()
        } else {
            Vec::new()
        };
        let mut section = ReportSection::new(
            "Building",
            format!(
                "A rig with {STARTING_BLOCKS} stone blocks in its hopper stakes two claims in a \
                 cavern in the {} and places every module through real build commands (a player \
                 short of blocks pays their market price on top). \"Minutes of income\" divides the price by what the \
                 prospector bot was earning per minute when it first reached the module's \
                 unlock band (the clay beds when it has none).",
                BUILD_BAND.name()
            ),
        );
        let Some(run) = run_building(hz) else {
            section
                .notes
                .push("The cavern could not be carved.".to_owned());
            return section;
        };
        let mut table = Table::new(
            "Prices paid:",
            &[
                "Module",
                "Credits",
                "Blocks",
                "Unlocks at",
                "Income there (cr/min)",
                "Minutes of income",
            ],
        );
        for result in &run.results {
            let band = result.item.unlock_band().unwrap_or(Band::Clay);
            let income = income_at_band(&runs, band);
            let name = if result.item == ModuleKind::Beacon {
                format!("{} (claim {})", result.item.name(), result.nth)
            } else {
                result.item.name().to_owned()
            };
            let cells = if let Some(refused) = &result.refused {
                vec![
                    name,
                    "refused".to_owned(),
                    refused.clone(),
                    String::new(),
                    String::new(),
                    String::new(),
                ]
            } else {
                vec![
                    name,
                    grouped(f64::from(result.credits)),
                    result.blocks.to_string(),
                    band.name().to_owned(),
                    income.map_or_else(|| "-".to_owned(), grouped),
                    income.filter(|rate| *rate > 0.0).map_or_else(
                        || "-".to_owned(),
                        |rate| format!("{:.1}", f64::from(result.credits) / rate),
                    ),
                ]
            };
            if result.refused.is_none() {
                let key = format!(
                    "building.{}{}.credits",
                    result.item.name().to_lowercase().replace(' ', "_"),
                    if result.nth > 1 { "_2" } else { "" }
                );
                section.metric(key, f64::from(result.credits));
            }
            table.row(cells);
        }
        section.tables.push(table);
        let mut claims = Table::new(
            "Claim beacons by claims already held:",
            &["Held", "Beacon price"],
        );
        for held in 0..4 {
            claims.row(vec![
                held.to_string(),
                grouped(f64::from(beacon_price(held))),
            ]);
        }
        section.tables.push(claims);
        let station = run
            .results
            .iter()
            .find(|result| result.item == ModuleKind::FuelStation && result.refused.is_none())
            .map(|result| result.credits);
        section.metric(
            "building.fuel_station.fuel_per_min",
            f64::from(run.station_fuel_per_minute),
        );
        if let Some(cost) = station {
            let outpost = cost
                + run
                    .results
                    .iter()
                    .filter(|r| {
                        r.refused.is_none()
                            && r.nth <= 1
                            && matches!(r.item, ModuleKind::Beacon | ModuleKind::Generator)
                    })
                    .map(|r| r.credits)
                    .sum::<u32>();
            section.notes.push(format!(
                "An underground fuel outpost (beacon, generator, station) costs {} credits; the \
                 station synthesizes {} fuel per game minute while powered (town fuel is 1 cr a \
                 unit), so the fuel alone repays it in {:.0} minutes, before the trips it saves.",
                grouped(f64::from(outpost)),
                run.station_fuel_per_minute,
                f64::from(outpost) / f64::from(run.station_fuel_per_minute.max(1))
            ));
            section.metric("building.fuel_outpost.credits", f64::from(outpost));
        }
        section
    }
}
