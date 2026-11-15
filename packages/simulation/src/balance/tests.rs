//! Balance harness tests: the full report (ignored; run it in release), a smoke test of the
//! economy bot, and cheap pinned checks guarding the pacing ratios.

use super::{BalanceConfig, bot::Strategy, run_report};

fn report_config() -> BalanceConfig {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let seeds = std::env::var("BALANCE_SEEDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let minutes = std::env::var("BALANCE_MINUTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(40);
    let only = std::env::var("BALANCE_ONLY")
        .ok()
        .map(|value| value.split(',').map(str::to_owned).collect());
    BalanceConfig {
        seeds: BalanceConfig::seeds(seeds),
        minutes,
        threads,
        only,
        ..BalanceConfig::default()
    }
}

#[test]
#[ignore = "balance report: run with --release -- --ignored --nocapture (BALANCE_SEEDS, BALANCE_MINUTES, BALANCE_ONLY)"]
fn balance_report() {
    println!("{}", run_report(&report_config()).to_markdown());
}

#[test]
fn the_economy_bot_mines_sells_and_upgrades() {
    let run = super::economy::run_economy(4_242, Strategy::Prospector, 6, 30);
    assert!(run.stats.trips >= 1, "{:?}", run.stats);
    assert!(run.earned_per_minute() > 0.0, "{:?}", run.stats);
    assert!(!run.stats.purchases.is_empty(), "{:?}", run.stats);
}

#[test]
#[ignore = "debug trace: BALANCE_TRACE=1 BOT_SEED=.. BOT_STRATEGY=.. BOT_MINUTES=.."]
fn trace_economy_bot() {
    let var = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse().ok())
    };
    let strategy = match std::env::var("BOT_STRATEGY").as_deref() {
        Ok("shaft") => Strategy::Shaft,
        Ok("charges") => Strategy::Charges,
        _ => Strategy::Prospector,
    };
    let run = super::economy::run_economy(
        var("BOT_SEED").unwrap_or(11),
        strategy,
        var("BOT_MINUTES").map_or(10, |minutes: u64| u32::try_from(minutes).unwrap_or(10)),
        60,
    );
    println!("{:#?}", run.stats);
}
