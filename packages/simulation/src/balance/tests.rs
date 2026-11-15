//! Balance harness tests: the full report (ignored; run it in release), a smoke test of the
//! economy bot, and cheap pinned checks guarding the pacing ratios.

use super::{
    BalanceConfig,
    bot::Strategy,
    combat::{LOADOUTS, duel},
    run_report,
};
use crate::{EnemyKind, enemy::tier_scale, terrain::DeepStratum, worldgen::Band};

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
    assert_eq!(run.stats.rescues, 0, "{:?}", run.stats.rescue_log);
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

/// Pinned (T19 pacing pass): a burrower's lunge in the clay beds, the first band a new player
/// drills into, takes at most this much of a stock 100-hull rig per second of its lunge cycle
/// (wind-up plus cooldown). It was 10 x 1.32 every 2.55 s (5.2 hull/s) before the pass.
#[test]
fn early_burrowers_bite_softly() {
    let stats = EnemyKind::Burrower.stats();
    let cycle = (stats.windup_millis + stats.cooldown_millis) as f32 / 1000.0;
    let per_second = stats.contact_damage * tier_scale(1) / cycle;
    assert!(per_second <= 2.6, "{per_second} hull/s");
    assert!(
        stats.hearing_tiles <= 12.0,
        "hears drills {} tiles off",
        stats.hearing_tiles
    );
}

/// Pinned: elites are set pieces you can finish. A starter rivet gun kills a brood lord at its
/// shallowest lair inside a minute (63 s before the pass), and a top railgun drops a guardian
/// in the machine layer inside half a minute (41 s before).
#[test]
fn elites_fall_in_set_piece_time() {
    let lord = duel(
        EnemyKind::BroodLord,
        Band::Deep(DeepStratum::FossilOceans),
        LOADOUTS[0],
        30,
    )
    .expect("arena");
    assert!(
        lord.seconds.is_some_and(|seconds| seconds < 55.0),
        "brood lord vs rivet gun: {lord:?}"
    );
    let guardian = duel(
        EnemyKind::AncientGuardian,
        Band::Deep(DeepStratum::AncientMachineLayer),
        LOADOUTS[4],
        30,
    )
    .expect("arena");
    assert!(
        guardian.seconds.is_some_and(|seconds| seconds < 32.0),
        "guardian vs railgun: {guardian:?}"
    );
}

/// Pinned: ordinary armoured enemies still take real shooting (the elite pass did not make
/// everything trivial): a mid-depth shellback shrugs off the starter gun for a while.
#[test]
fn armour_still_matters() {
    let shellback = duel(EnemyKind::Shellback, Band::Slate, LOADOUTS[0], 30).expect("arena");
    assert!(
        shellback.seconds.is_some_and(|seconds| seconds > 8.0),
        "shellback vs rivet gun: {shellback:?}"
    );
}
