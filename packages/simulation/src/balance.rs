//! The balance harness (expansion plan T19).
//!
//! Deterministic bot players drive the real simulation across many seeds, measuring the
//! economy, combat, building and progression, and print a readable report.
//!
//! Everything here plays through [`WorldState::simulate_tick`](crate::WorldState::simulate_tick)
//! exactly like a human seat would: the bots submit held [`ControlState`](crate::ControlState)s
//! and one-shot [`PlayerCommand`](crate::PlayerCommand)s, and every number is read back from the
//! world. Probes may *set up* a scenario directly (carve an arena, hand a rig a weapon), but
//! never shortcut the tick. Runs are deterministic: the same [`BalanceConfig`] prints the same
//! report on every machine.
//!
//! - [`economy`]: the economy bot ([`bot::MinerBot`]): drill, prospect sideways for ore, bomb
//!   with mining charges, fly home, sell, refuel, repair and buy upgrades, for hours of game
//!   time, one run per seed and [`Strategy`].
//! - [`combat`]: duels (time to kill per weapon loadout against every enemy family at its
//!   depth tiers, elites included) and exposure runs (enemy and hazard damage per depth band
//!   while drilling, the early burrower threat).
//! - [`building`]: what claims, modules and the underground fuel station cost in credits,
//!   blocks and minutes of income.
//! - Progression (in [`economy`]): time to each upgrade tier, depth over time and credits per
//!   minute by strategy.
//! - [`climb`]: the climb bot (the plan's T19 row): apex and turn-back altitude on one tank
//!   per fuel-tank and engine tier, through the real tick.
//! - [`tombs`]: tomb and crypt runs (Wave 5 T27): run length, death rate and loot per tier
//!   and style, from a scripted bot playing whole runs ([`tombs::TombBot`]).
//! - [`trip`]: the everyday trip (drill 50 rows down, mine two minutes, fly home): the fuel
//!   it burns against each tank, the yardstick of the fuel curve.
//!
//! The report: `fatherload --balance-report [--seeds N] [--minutes M] [--only a,b]` (the game
//! binary), or `cargo test --release -p fatherload_simulation balance_report -- --ignored
//! --nocapture`.
//!
//! # Adding a probe (Wave 4 hooks)
//!
//! Implement [`BalanceProbe`] and add it to [`standard_probes`]. A probe gets a
//! [`ProbeContext`] (the config plus the cached economy runs, so it can express costs in
//! minutes of income) and returns a [`ReportSection`] of [`Table`]s and named
//! [`ReportSection::metrics`] that pinned tests can guard. Planned: a sky probe (time to each
//! island band, building on [`climb`]'s altitude table), a Moon
//! probe (round-trip fuel, low-g combat) and a pilot probe (on-foot time to kill, jetpack
//! reach). The economy bot's [`bot::MinerBot`] takes new [`Strategy`] variants for new loops.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "report arithmetic: tick counts, credits and tile coordinates are small and exact \
              enough in f32/f64"
)]

pub mod bot;
pub mod building;
pub mod climb;
pub mod combat;
pub mod crypt_bot;
pub mod economy;
pub mod night;
pub mod second_act;
#[cfg(test)]
mod tests;
pub mod tomb_side;
pub mod tombs;
pub mod trip;

use std::{collections::BTreeMap, fmt::Write as _};

use crate::DEFAULT_TICK_RATE_HZ;

pub use bot::Strategy;
pub use economy::EconomyRun;

/// Seeds the report uses by default (`--seeds N` takes the first `N`, then derives more).
pub const DEFAULT_SEEDS: [u64; 8] = [
    11, 4_242, 90_001, 777_777, 31_337, 2_026, 600_613, 8_675_309,
];

/// What a balance run measures and for how long.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalanceConfig {
    /// World seeds (every economy and exposure run is repeated per seed).
    pub seeds: Vec<u64>,
    /// Game minutes per economy run.
    pub minutes: u32,
    /// Session tick rate (the simulation is tick-rate safe; lower is faster to run).
    pub tick_rate_hz: u32,
    /// Economy strategies to compare.
    pub strategies: Vec<Strategy>,
    /// Worker threads for independent runs (1: run everything on the calling thread).
    pub threads: usize,
    /// Probe names to run (`None`: every probe in [`standard_probes`]).
    pub only: Option<Vec<String>>,
    /// The generator the economy runs' worlds use (`--generator`; the current one by default,
    /// [`crate::worldgen::GeneratorVersion::DEEP_DESCENT`] for the Deep Descent preview).
    pub generator: crate::worldgen::GeneratorVersion,
}

impl Default for BalanceConfig {
    fn default() -> Self {
        Self {
            seeds: DEFAULT_SEEDS[..3].to_vec(),
            minutes: 40,
            tick_rate_hz: DEFAULT_TICK_RATE_HZ,
            strategies: Strategy::ALL.to_vec(),
            threads: 1,
            only: None,
            generator: crate::worldgen::GeneratorVersion::CURRENT,
        }
    }
}

impl BalanceConfig {
    /// The first `count` seeds of [`DEFAULT_SEEDS`], extended deterministically past eight.
    #[must_use]
    pub fn seeds(count: usize) -> Vec<u64> {
        (0..count)
            .map(|index| {
                DEFAULT_SEEDS.get(index).copied().unwrap_or_else(|| {
                    (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xBA1A_11CE
                })
            })
            .collect()
    }

    /// Whether probe `name` is selected.
    #[must_use]
    pub fn wants(&self, name: &str) -> bool {
        self.only
            .as_ref()
            .is_none_or(|only| only.iter().any(|wanted| wanted == name))
    }
}

/// A table in a report section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    /// One line above the table.
    pub caption: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// An empty table with `headers`.
    #[must_use]
    pub fn new(caption: impl Into<String>, headers: &[&str]) -> Self {
        Self {
            caption: caption.into(),
            headers: headers.iter().map(|header| (*header).to_owned()).collect(),
            rows: Vec::new(),
        }
    }

    /// Appends a row.
    pub fn row(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }

    /// The table as GitHub-flavoured Markdown.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        if !self.caption.is_empty() {
            let _ = writeln!(out, "{}\n", self.caption);
        }
        let _ = writeln!(out, "| {} |", self.headers.join(" | "));
        let _ = writeln!(
            out,
            "|{}|",
            self.headers
                .iter()
                .map(|_| "---")
                .collect::<Vec<_>>()
                .join("|")
        );
        for row in &self.rows {
            let _ = writeln!(out, "| {} |", row.join(" | "));
        }
        out
    }
}

/// One probe's part of the report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReportSection {
    pub title: String,
    /// A paragraph explaining what was measured and how.
    pub intro: String,
    pub tables: Vec<Table>,
    /// Observations (warnings when a ratio looks off).
    pub notes: Vec<String>,
    /// Named numbers for pinned tests and tooling (e.g. `economy.prospector.credits_per_min`).
    pub metrics: BTreeMap<String, f64>,
}

impl ReportSection {
    /// An empty section.
    #[must_use]
    pub fn new(title: impl Into<String>, intro: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            intro: intro.into(),
            ..Self::default()
        }
    }

    /// Records a metric.
    pub fn metric(&mut self, name: impl Into<String>, value: f64) {
        self.metrics.insert(name.into(), value);
    }
}

/// A whole balance report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BalanceReport {
    /// One line describing the run (seeds, minutes, tick rate).
    pub header: String,
    pub sections: Vec<ReportSection>,
}

impl BalanceReport {
    /// Every section's metrics, merged.
    #[must_use]
    pub fn metrics(&self) -> BTreeMap<String, f64> {
        self.sections
            .iter()
            .flat_map(|section| section.metrics.clone())
            .collect()
    }

    /// The report as Markdown.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = format!("# Fatherload balance report\n\n{}\n", self.header);
        for section in &self.sections {
            let _ = write!(out, "\n## {}\n\n{}\n", section.title, section.intro);
            for table in &section.tables {
                let _ = write!(out, "\n{}", table.to_markdown());
            }
            if !section.notes.is_empty() {
                out.push('\n');
                for note in &section.notes {
                    let _ = writeln!(out, "- {note}");
                }
            }
        }
        out
    }
}

/// What a probe sees: the run's config and the economy runs, computed once on first use and
/// shared by every probe that wants them.
pub struct ProbeContext {
    pub config: BalanceConfig,
    economy: Option<Vec<EconomyRun>>,
}

impl ProbeContext {
    /// A context for `config`.
    #[must_use]
    pub const fn new(config: BalanceConfig) -> Self {
        Self {
            config,
            economy: None,
        }
    }

    /// The economy runs (one per seed and strategy), simulated on first call.
    pub fn economy(&mut self) -> &[EconomyRun] {
        let config = &self.config;
        self.economy
            .get_or_insert_with(|| economy::run_all(config))
            .as_slice()
    }
}

/// One measurement in the report (see the module docs for adding one).
pub trait BalanceProbe: Sync {
    /// Short name used by `--only` (lowercase, no spaces).
    fn name(&self) -> &'static str;
    /// Runs the probe.
    fn run(&self, context: &mut ProbeContext) -> ReportSection;
}

/// Every probe of the standard report, in report order.
#[must_use]
pub fn standard_probes() -> Vec<Box<dyn BalanceProbe>> {
    vec![
        Box::new(economy::EconomyProbe),
        Box::new(economy::ProgressionProbe),
        Box::new(combat::CombatProbe),
        Box::new(combat::ExposureProbe),
        Box::new(building::BuildingProbe),
        Box::new(climb::ClimbProbe),
        Box::new(trip::TripProbe),
        Box::new(night::NightProbe),
        Box::new(second_act::SecondActProbe),
        Box::new(tombs::TombsProbe),
    ]
}

/// Runs the selected probes of [`standard_probes`].
#[must_use]
pub fn run_report(config: &BalanceConfig) -> BalanceReport {
    run_probes(config, &standard_probes())
}

/// Runs the selected `probes`.
#[must_use]
pub fn run_probes(config: &BalanceConfig, probes: &[Box<dyn BalanceProbe>]) -> BalanceReport {
    let mut context = ProbeContext::new(config.clone());
    let sections = probes
        .iter()
        .filter(|probe| config.wants(probe.name()))
        .map(|probe| probe.run(&mut context))
        .collect();
    BalanceReport {
        header: format!(
            "Seeds {:?}, {} game minutes per economy run, {} Hz, strategies {}.",
            config.seeds,
            config.minutes,
            config.tick_rate_hz,
            config
                .strategies
                .iter()
                .map(|strategy| strategy.name())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        sections,
    }
}

/// Runs `jobs` on up to `threads` worker threads, keeping their order.
pub(crate) fn run_parallel<T: Send, F: Fn() -> T + Send + Sync>(
    threads: usize,
    jobs: Vec<F>,
) -> Vec<T> {
    if threads <= 1 || jobs.len() <= 1 {
        return jobs.into_iter().map(|job| job()).collect();
    }
    let mut slots: Vec<Option<T>> = jobs.iter().map(|_| None).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(&mut slots);
    std::thread::scope(|scope| {
        for _ in 0..threads.min(jobs.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(job) = jobs.get(index) else {
                        break;
                    };
                    let value = job();
                    if let Ok(mut slots) = results.lock() {
                        slots[index] = Some(value);
                    }
                }
            });
        }
    });
    slots.into_iter().flatten().collect()
}

/// `value` with thousands separators and no decimals.
#[must_use]
pub fn grouped(value: f64) -> String {
    let rounded = value.round() as i64;
    let digits = rounded.unsigned_abs().to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if rounded < 0 { format!("-{out}") } else { out }
}

/// Mean of `values` (0 when empty).
#[must_use]
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// Median of `values` (0 when empty).
#[must_use]
pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        f64::midpoint(sorted[middle - 1], sorted[middle])
    } else {
        sorted[middle]
    }
}
