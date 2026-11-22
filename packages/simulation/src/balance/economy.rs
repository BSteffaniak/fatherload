//! Economy runs and the progression curve.
//!
//! A [`MinerBot`] plays a fresh world for the configured game minutes per seed and
//! [`Strategy`]; the report shows credits per minute by strategy, where the money went, how
//! deep the bot got, and when each upgrade tier landed.

use std::collections::BTreeMap;

use super::{
    BalanceConfig, BalanceProbe, ProbeContext, ReportSection, Table,
    bot::{BotStats, MinerBot, Strategy, TripEnd, TripRecord},
    grouped, mean, median, run_parallel,
};
use crate::{
    EnemyDifficulty, PlayerCommand, PlayerId, PlayerTickInput, SimulationParams, TickAction,
    TickInputs, WorldState,
    economy::{UpgradeKind, current_level},
    ids::InputSequence,
    terrain::TilePosition,
    worldgen::{Band, WorldSettings},
};

/// The bot's player.
pub const BOT_PLAYER: PlayerId = PlayerId::new(0);
/// The core upgrade lines the progression curve follows.
pub const CORE_LINES: [UpgradeKind; 6] = [
    UpgradeKind::Drill,
    UpgradeKind::CargoBay,
    UpgradeKind::FuelTank,
    UpgradeKind::Engine,
    UpgradeKind::Hull,
    UpgradeKind::Radiator,
];
/// Tiers the time-to-tier table reports.
pub const REPORTED_TIERS: [u8; 7] = [2, 3, 4, 5, 6, 8, 10];
/// The column the report reads depth bands at (the first shaft).
const DEPTH_COLUMN: i32 = 78;

/// The rig and the bot's tallies at the end of one game minute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    pub minute: u32,
    /// Credits earned from sales so far.
    pub earned: u64,
    /// Credits on hand.
    pub credits: u32,
    /// Deepest row reached so far.
    pub deepest_row: i32,
    /// Its band.
    pub band: Band,
    /// Levels of [`CORE_LINES`].
    pub levels: [u8; 6],
}

/// One economy run.
#[derive(Clone, Debug, PartialEq)]
pub struct EconomyRun {
    pub seed: u64,
    pub strategy: Strategy,
    pub minutes: u32,
    pub samples: Vec<Sample>,
    pub stats: BotStats,
    /// Game minute each band was first reached.
    pub band_minutes: BTreeMap<usize, f32>,
}

impl EconomyRun {
    /// Credits earned per minute over the whole run.
    #[must_use]
    pub fn earned_per_minute(&self) -> f64 {
        self.earned_between(0, self.minutes)
    }

    /// Credits earned per minute between minutes `from` and `to`.
    #[must_use]
    pub fn earned_between(&self, from: u32, to: u32) -> f64 {
        let at = |minute: u32| {
            if minute == 0 {
                0
            } else {
                self.samples
                    .iter()
                    .find(|sample| sample.minute >= minute)
                    .or_else(|| self.samples.last())
                    .map_or(0, |sample| sample.earned)
            }
        };
        let span = f64::from(to.saturating_sub(from).max(1));
        (at(to) as f64 - at(from) as f64) / span
    }

    /// Game minute line `kind` reached `level`, if it did.
    #[must_use]
    pub fn minute_of_tier(&self, kind: UpgradeKind, level: u8) -> Option<f32> {
        if level <= kind.starting_level() {
            return Some(0.0);
        }
        self.stats
            .purchases
            .iter()
            .find(|(_, line, reached)| *line == kind && *reached >= level)
            .map(|(seconds, ..)| seconds / 60.0)
    }

    /// The sample at or after `minute` (the last one past the end).
    #[must_use]
    pub fn sample_at(&self, minute: u32) -> Option<&Sample> {
        self.samples
            .iter()
            .find(|sample| sample.minute >= minute)
            .or_else(|| self.samples.last())
    }
}

/// A fresh standard world for `seed` on the current generator, Normal enemies, one rig.
#[must_use]
pub fn new_world(seed: u64, tick_rate_hz: u32) -> WorldState {
    new_world_on(
        seed,
        tick_rate_hz,
        crate::worldgen::GeneratorVersion::CURRENT,
    )
}

/// [`new_world`] on `generator` (the balance report's `--generator`, e.g. the Deep Descent
/// preview).
#[must_use]
pub fn new_world_on(
    seed: u64,
    tick_rate_hz: u32,
    generator: crate::worldgen::GeneratorVersion,
) -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        BOT_PLAYER,
        &WorldSettings {
            generator,
            ..WorldSettings::new(seed)
        },
        SimulationParams::new(tick_rate_hz),
    );
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    world
}

/// Builds one tick's inputs for `player`.
#[must_use]
pub fn inputs_for(
    player: PlayerId,
    controls: crate::ControlState,
    actions: Vec<PlayerCommand>,
) -> TickInputs {
    let mut inputs = TickInputs::default();
    inputs.players.insert(
        player,
        PlayerTickInput {
            controls: Some(controls),
            actions: actions
                .into_iter()
                .enumerate()
                .map(|(index, command)| TickAction {
                    sequence: InputSequence::new(u32::try_from(index).unwrap_or(u32::MAX)),
                    command,
                })
                .collect(),
        },
    );
    inputs
}

/// Plays `minutes` of `strategy` on `seed`'s world.
#[must_use]
pub fn run_economy(seed: u64, strategy: Strategy, minutes: u32, tick_rate_hz: u32) -> EconomyRun {
    run_economy_on(
        seed,
        strategy,
        minutes,
        tick_rate_hz,
        crate::worldgen::GeneratorVersion::CURRENT,
    )
}

/// [`run_economy`] on a world of `generator`.
#[must_use]
pub fn run_economy_on(
    seed: u64,
    strategy: Strategy,
    minutes: u32,
    tick_rate_hz: u32,
    generator: crate::worldgen::GeneratorVersion,
) -> EconomyRun {
    let mut world = new_world_on(seed, tick_rate_hz, generator);
    let mut bot = MinerBot::new(BOT_PLAYER, strategy);
    let ticks_per_minute = u64::from(world.tick_rate_hz()) * 60;
    let mut samples = Vec::new();
    let mut band_minutes = BTreeMap::new();
    let mut deepest_band = 0;
    let trace = std::env::var_os("BALANCE_TRACE").is_some();
    for tick in 0..ticks_per_minute * u64::from(minutes) {
        let (controls, actions) = bot.decide(&world);
        let inputs = inputs_for(BOT_PLAYER, controls, actions);
        let output = world.simulate_tick(world.simulation_tick(), &inputs);
        bot.observe(&world, &output);
        if trace
            && tick % u64::from(world.tick_rate_hz()) == 0
            && let Some(rig) = world.player(BOT_PLAYER)
        {
            eprintln!(
                "{:6.0}s {:40} x={:6.1} y={:6.1} fuel={:5.0}/{:.0} hull={:4.0} cargo={}/{} cr={} mc={}",
                bot.clock(),
                bot.describe(),
                rig.x / crate::TILE_SIZE,
                rig.y / crate::TILE_SIZE,
                rig.fuel,
                rig.fuel_capacity,
                rig.hull,
                rig.cargo_used(),
                rig.cargo_capacity,
                rig.credits,
                rig.explosive_count(crate::ExplosiveKind::Mining)
            );
        }
        let band = world
            .terrain()
            .band_at(TilePosition {
                x: DEPTH_COLUMN,
                y: bot.stats().deepest_row,
            })
            .index();
        if band > deepest_band || tick == 0 {
            for reached in deepest_band..=band {
                band_minutes
                    .entry(reached)
                    .or_insert(tick as f32 / ticks_per_minute as f32);
            }
            deepest_band = band;
        }
        if (tick + 1) % ticks_per_minute == 0
            && let Some(rig) = world.player(BOT_PLAYER)
        {
            let stats = bot.stats();
            samples.push(Sample {
                minute: u32::try_from((tick + 1) / ticks_per_minute).unwrap_or(u32::MAX),
                earned: stats.ore_income + stats.block_income,
                credits: rig.credits,
                deepest_row: stats.deepest_row,
                band: Band::from_index(deepest_band),
                levels: CORE_LINES.map(|kind| current_level(rig, kind)),
            });
        }
    }
    EconomyRun {
        seed,
        strategy,
        minutes,
        samples,
        stats: bot.stats().clone(),
        band_minutes,
    }
}

/// Every (seed, strategy) economy run of `config`, in seed-then-strategy order.
#[must_use]
pub fn run_all(config: &BalanceConfig) -> Vec<EconomyRun> {
    let jobs: Vec<_> = config
        .seeds
        .iter()
        .flat_map(|seed| {
            config
                .strategies
                .iter()
                .map(move |strategy| (*seed, *strategy))
        })
        .map(|(seed, strategy)| {
            let minutes = config.minutes;
            let hz = config.tick_rate_hz;
            let generator = config.generator;
            move || run_economy_on(seed, strategy, minutes, hz, generator)
        })
        .collect();
    run_parallel(config.threads, jobs)
}

fn runs_of(runs: &[EconomyRun], strategy: Strategy) -> Vec<&EconomyRun> {
    runs.iter().filter(|run| run.strategy == strategy).collect()
}

/// The `share` quantile (0..=1) of `values` (0 when empty), nearest rank.
fn quantile(values: &[f64], share: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((sorted.len() - 1) as f64 * share).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

/// Fuel per trip by the tank tier it was flown with, over `runs`: how much of the tank an
/// everyday trip (shaft mouth back to town) burns. Records the metrics
/// `fuel.tank{t}.median_tank_percent`, `fuel.tank{t}.p90_tank_percent` and
/// `fuel.tank{t}.fuel_turn_percent` (share of trips that turned home on fuel).
fn trip_fuel_table(runs: &[&EconomyRun], section: &mut ReportSection) -> Table {
    let mut by_tank: BTreeMap<u8, Vec<TripRecord>> = BTreeMap::new();
    for record in runs.iter().flat_map(|run| &run.stats.trip_log) {
        by_tank.entry(record.tank_level).or_default().push(*record);
    }
    let mut table = Table::new(
        "Fuel per trip (shaft mouth back to town) by the fuel tank flown:",
        &[
            "Tank (fuel)",
            "Trips",
            "Median fuel",
            "Median % of tank",
            "90th pct % of tank",
            "Median depth (rows)",
            "Median trip (min)",
            "Turned home on fuel",
        ],
    );
    for (tank, records) in by_tank {
        let collect = |f: &dyn Fn(&TripRecord) -> f64| records.iter().map(f).collect::<Vec<_>>();
        let shares = collect(&|r| f64::from(r.fuel_used / r.fuel_capacity.max(1.0)) * 100.0);
        let fuel = collect(&|r| f64::from(r.fuel_used));
        let depth = collect(&|r| f64::from(r.depth_rows));
        let minutes = collect(&|r| f64::from(r.seconds) / 60.0);
        let capacity = records.iter().map(|r| r.fuel_capacity).fold(0.0, f32::max);
        let on_fuel = records.iter().filter(|r| r.end == TripEnd::Fuel).count() as f64
            / records.len() as f64
            * 100.0;
        section.metric(
            format!("fuel.tank{tank}.median_tank_percent"),
            median(&shares),
        );
        section.metric(
            format!("fuel.tank{tank}.p90_tank_percent"),
            quantile(&shares, 0.9),
        );
        section.metric(format!("fuel.tank{tank}.fuel_turn_percent"), on_fuel);
        table.row(vec![
            format!("Tier {tank} ({capacity:.0})"),
            records.len().to_string(),
            format!("{:.0}", median(&fuel)),
            format!("{:.0} %", median(&shares)),
            format!("{:.0} %", quantile(&shares, 0.9)),
            format!("{:.0}", median(&depth)),
            format!("{:.1}", median(&minutes)),
            format!("{on_fuel:.0} %"),
        ]);
    }
    table
}

/// The economy section: credits per minute by strategy and where the money went.
pub struct EconomyProbe;

impl BalanceProbe for EconomyProbe {
    fn name(&self) -> &'static str {
        "economy"
    }

    #[allow(clippy::too_many_lines, reason = "one report section, table by table")]
    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let minutes = context.config.minutes;
        let strategies = context.config.strategies.clone();
        let runs = context.economy();
        let mut section = ReportSection::new(
            "Economy",
            format!(
                "The economy bot plays a fresh world from a stock rig for {minutes} game minutes \
                 per seed: shafts in the town's gaps, sideways ore tunnels (prospector, charges), \
                 mining charges on ore-rich rows (charges), trips home to sell, refuel, repair \
                 and upgrade. Credits are sale income (ore, artifacts and blocks at market \
                 price); means over seeds."
            ),
        );
        let quarter = (minutes / 4).max(1);
        let mut income = Table::new(
            "Credits earned per minute (sales), by stretch of the run:",
            &[
                "Strategy",
                "Whole run",
                &format!("0-{quarter} min"),
                &format!("{quarter}-{} min", quarter * 2),
                &format!("{}-{minutes} min", quarter * 2),
                "Trips",
                "Rescues",
                "Shafts",
                "Deepest band",
                "Blocks share",
            ],
        );
        let mut risk = Table::new(
            "Danger and arms (per run, means):",
            &[
                "Strategy",
                "Enemy hull loss / min",
                "Hazard hull loss / min",
                "Kills",
                "Charges set off",
                "Upgrades bought",
                "Upgrade spend",
                "Upkeep spend",
                "Arms spend",
            ],
        );
        for strategy in strategies {
            let of = runs_of(runs, strategy);
            if of.is_empty() {
                continue;
            }
            let per =
                |f: &dyn Fn(&EconomyRun) -> f64| mean(&of.iter().map(|r| f(r)).collect::<Vec<_>>());
            let whole = per(&|run| run.earned_per_minute());
            let deepest = of
                .iter()
                .filter_map(|run| run.samples.last().map(|sample| sample.band.index()))
                .max()
                .unwrap_or(0);
            let blocks = per(&|run| {
                let total = run.stats.ore_income + run.stats.block_income;
                if total == 0 {
                    0.0
                } else {
                    run.stats.block_income as f64 / total as f64
                }
            });
            income.row(vec![
                strategy.name().to_owned(),
                grouped(whole),
                grouped(per(&|run| run.earned_between(0, quarter))),
                grouped(per(&|run| run.earned_between(quarter, quarter * 2))),
                grouped(per(&|run| run.earned_between(quarter * 2, minutes))),
                format!("{:.1}", per(&|run| f64::from(run.stats.trips))),
                format!("{:.1}", per(&|run| f64::from(run.stats.rescues))),
                format!("{:.1}", per(&|run| f64::from(run.stats.shafts))),
                Band::from_index(deepest).name().to_owned(),
                format!("{:.0} %", blocks * 100.0),
            ]);
            let minutes_f = f64::from(minutes.max(1));
            risk.row(vec![
                strategy.name().to_owned(),
                format!("{:.1}", per(&|run| run.stats.enemy_damage) / minutes_f),
                format!("{:.1}", per(&|run| run.stats.hazard_damage) / minutes_f),
                format!("{:.1}", per(&|run| f64::from(run.stats.kills))),
                format!("{:.1}", per(&|run| f64::from(run.stats.charges_used))),
                format!("{:.1}", per(&|run| run.stats.purchases.len() as f64)),
                grouped(per(&|run| run.stats.upgrade_spend as f64)),
                grouped(per(&|run| run.stats.upkeep_spend as f64)),
                grouped(per(&|run| run.stats.arms_spend as f64)),
            ]);
            let name = strategy.name();
            section.metric(format!("economy.{name}.earned_per_min"), whole);
            section.metric(
                format!("economy.{name}.late_earned_per_min"),
                per(&|run| run.earned_between(quarter * 2, minutes)),
            );
            section.metric(
                format!("economy.{name}.rescues"),
                per(&|run| f64::from(run.stats.rescues)),
            );
            section.metric(format!("economy.{name}.deepest_band"), deepest as f64);
            section.metric(
                format!("economy.{name}.enemy_damage_per_min"),
                per(&|run| run.stats.enemy_damage) / minutes_f,
            );
        }
        section.tables.push(income);
        section.tables.push(risk);
        let metrics = section.metrics.clone();
        let get = |key: &str| metrics.get(key).copied().unwrap_or(0.0);
        let prospector = get("economy.prospector.earned_per_min");
        let charges = get("economy.charges.earned_per_min");
        if prospector > 0.0 && charges > 0.0 {
            section.notes.push(format!(
                "Mining charges vs drilling (whole loop, before charge costs): {:.2}x.",
                charges / prospector
            ));
        }
        section
    }
}

/// The progression curve: time to each upgrade tier and depth over time.
pub struct ProgressionProbe;

impl BalanceProbe for ProgressionProbe {
    fn name(&self) -> &'static str {
        "progression"
    }

    #[allow(clippy::too_many_lines, reason = "one report section, table by table")]
    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let minutes = context.config.minutes;
        let strategies = context.config.strategies.clone();
        let runs = context.economy();
        let mut section = ReportSection::new(
            "Progression",
            "When the economy bot reached each tier of the core lines (median game minute over \
             the seeds of the reference strategy, the prospector, or of every strategy run when \
             it is not; the share of runs that got there in brackets when not all did), how \
             deep it was over time, and when it first entered each band.",
        );
        // The time-to-tier table follows the reference player: mixing in the shaft-only
        // baseline, which never affords an upgrade, would drag every median down.
        let reference = runs_of(runs, Strategy::Prospector);
        let tier_runs: Vec<&EconomyRun> = if reference.is_empty() {
            runs.iter().collect()
        } else {
            reference
        };
        let mut headers = vec!["Line".to_owned()];
        headers.extend(REPORTED_TIERS.iter().map(|tier| format!("Tier {tier}")));
        let mut tiers = Table {
            caption: "Minutes to reach each tier:".to_owned(),
            headers,
            rows: Vec::new(),
        };
        for kind in CORE_LINES {
            let mut row = vec![format!("{kind:?}")];
            for tier in REPORTED_TIERS {
                let reached: Vec<f64> = tier_runs
                    .iter()
                    .filter_map(|run| run.minute_of_tier(kind, tier).map(f64::from))
                    .collect();
                let cell = if reached.is_empty() {
                    "-".to_owned()
                } else if reached.len() == tier_runs.len() {
                    format!("{:.1}", median(&reached))
                } else {
                    format!(
                        "{:.1} ({}/{})",
                        median(&reached),
                        reached.len(),
                        tier_runs.len()
                    )
                };
                if kind == UpgradeKind::Drill && !reached.is_empty() {
                    section.metric(format!("progression.drill{tier}_minute"), median(&reached));
                }
                row.push(cell);
            }
            tiers.row(row);
        }
        section.tables.push(tiers);
        let trips = trip_fuel_table(&tier_runs, &mut section);
        section.tables.push(trips);

        let mut headers = vec!["Minute".to_owned()];
        headers.extend(
            strategies
                .iter()
                .map(|s| format!("{} depth (row, band)", s.name())),
        );
        headers.extend(strategies.iter().map(|s| format!("{} cr/min", s.name())));
        let mut depth = Table {
            caption: "Depth reached and earning rate over time (medians):".to_owned(),
            headers,
            rows: Vec::new(),
        };
        let marks: Vec<u32> = [5, 10, 15, 20, 30, 40, 60, 90, 120, 180, 240]
            .into_iter()
            .filter(|mark| *mark <= minutes)
            .collect();
        let mut previous = 0;
        for mark in marks {
            let mut row = vec![mark.to_string()];
            for strategy in &strategies {
                let of = runs_of(runs, *strategy);
                let rows: Vec<f64> = of
                    .iter()
                    .filter_map(|run| run.sample_at(mark).map(|s| f64::from(s.deepest_row)))
                    .collect();
                let bands: Vec<f64> = of
                    .iter()
                    .filter_map(|run| run.sample_at(mark).map(|s| s.band.index() as f64))
                    .collect();
                row.push(format!(
                    "{:.0} ({})",
                    median(&rows),
                    Band::from_index(median(&bands).round() as usize).name()
                ));
            }
            for strategy in &strategies {
                let of = runs_of(runs, *strategy);
                let rates: Vec<f64> = of
                    .iter()
                    .map(|run| run.earned_between(previous, mark))
                    .collect();
                row.push(grouped(median(&rates)));
            }
            previous = mark;
            depth.row(row);
        }
        section.tables.push(depth);

        let mut headers = vec!["Band".to_owned()];
        headers.extend(strategies.iter().map(|s| s.name().to_owned()));
        let mut bands = Table {
            caption: "Minute each band was first entered (median of the runs that got there):"
                .to_owned(),
            headers,
            rows: Vec::new(),
        };
        for band in Band::ALL.iter().skip(1) {
            let mut row = vec![band.name().to_owned()];
            let mut any = false;
            for strategy in &strategies {
                let of = runs_of(runs, *strategy);
                let reached: Vec<f64> = of
                    .iter()
                    .filter_map(|run| run.band_minutes.get(&band.index()).map(|m| f64::from(*m)))
                    .collect();
                if reached.is_empty() {
                    row.push("-".to_owned());
                } else {
                    any = true;
                    row.push(format!(
                        "{:.1} ({}/{})",
                        median(&reached),
                        reached.len(),
                        of.len()
                    ));
                }
            }
            if any {
                bands.row(row);
            }
        }
        section.tables.push(bands);
        section
    }
}
