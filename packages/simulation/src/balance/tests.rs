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

/// The climb probe flies the real tick: a bigger tank and a stronger engine both climb higher.
/// Without altitude intakes every climb tops out where thin air leaves the engine too weak
/// (about 480 tiles for a stock engine; even a stock tank gets most of the way), and thin air
/// slows the top of a climb, so the half-tank turn-back altitude sits past half the apex.
#[test]
fn bigger_tanks_and_engines_climb_higher() {
    use super::climb::climb;
    let stock = climb(11, 1, 1, 30).expect("stock climb");
    let tank = climb(11, 3, 1, 30).expect("tank 3 climb");
    let engine = climb(11, 1, 6, 30).expect("engine 6 climb");
    assert!(stock.apex_tiles > 200, "{stock:?}");
    assert!(tank.apex_tiles > stock.apex_tiles, "{tank:?} vs {stock:?}");
    // (V9's thirstier thin air narrows the engine's lead: about 40 % higher on the stock
    // tank, where V8's sky gave more than half again.)
    assert!(
        engine.apex_tiles > stock.apex_tiles * 4 / 3,
        "{engine:?} vs {stock:?}"
    );
    for result in [stock, engine] {
        assert!(
            result.turn_back_tiles >= result.apex_tiles / 2
                && result.turn_back_tiles < result.apex_tiles,
            "{result:?}"
        );
    }
}

/// Pinned: the fuel curve's yardstick. An everyday early trip (drill 50 rows down, mine for
/// two minutes, fly home) burns well under half a stock tank, so fuel only bites on deep and
/// long runs.
#[test]
fn an_everyday_trip_burns_well_under_half_a_stock_tank() {
    use super::trip::{TRIP_DEPTH_ROWS, everyday_trip};
    let trip = everyday_trip(11, 1, 30).expect("trip");
    println!("{trip:?} {:.1} %", trip.tank_percent());
    assert!(trip.landed, "{trip:?}");
    assert!(trip.depth_rows >= TRIP_DEPTH_ROWS, "{trip:?}");
    assert!(trip.tiles_mined >= 80, "{trip:?}");
    assert!(
        trip.tank_percent() < 40.0,
        "{:.0} % of a stock tank: {trip:?}",
        trip.tank_percent()
    );
}

/// The building probe places every module through real build commands and pays exactly the
/// listed price in credits and blocks (no block purchases folded into the credits).
#[test]
fn the_building_probe_pays_list_prices() {
    use crate::base::{ModuleKind, beacon_price};
    let run = super::building::run_building(30).expect("cavern");
    assert_eq!(run.results.len(), 10);
    for result in &run.results {
        assert_eq!(result.refused, None, "{result:?}");
        let credits = if result.item == ModuleKind::Beacon {
            beacon_price(result.nth.saturating_sub(1) as usize)
        } else {
            result.item.credits()
        };
        assert_eq!(result.credits, credits, "{result:?}");
        assert_eq!(result.blocks, result.item.blocks(), "{result:?}");
    }
    assert!(run.station_fuel_per_minute > 0);
}
