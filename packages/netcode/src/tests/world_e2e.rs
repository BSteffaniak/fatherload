//! End-to-end tests on the real gameplay world (`drillgame_simulation::WorldState`).
//!
//! One host and N clients over seeded `LinkSim`s, driven by bots that dig down (placing bombs,
//! scanning and deploying infrastructure on the way), fly up, land back at the depot, sell, and
//! try to buy upgrades and refuel (which the host refuses away from the shop and fuel station).
//! Every message crosses the real postcard codec.

use std::time::Duration;

use drillgame_simulation::WorldState;

use super::support::{assert_bounded, assert_converged};
use crate::harness::{
    Harness, HarnessConfig, HarnessSim, format_sweep, parameter_sweep, sweep_links,
};
use crate::{LinkConditions, SessionParams};

type WorldHarness = Harness<WorldState>;

fn harness(rate: u32, clients: usize, link: LinkConditions) -> WorldHarness {
    WorldHarness::new(HarnessConfig::new(SessionParams::new(rate), clients, link))
}

/// The bots really played: terrain was dug and every kind of scripted action took effect.
fn assert_played(harness: &WorldHarness) {
    let fresh = WorldState::harness_world(&harness.config);
    assert!(
        harness.world().play_progress() > fresh.play_progress() + harness.clients.len(),
        "bots dug terrain"
    );
    for label in [
        "applied scanner",
        "applied infrastructure",
        "applied bomb",
        "bomb detonated",
    ] {
        assert!(
            harness.event_counts.get(label).copied().unwrap_or(0) > 0,
            "{label}: {:?}",
            harness.event_counts
        );
    }
}

fn assert_economy_played(harness: &WorldHarness) {
    // Sales at the depot apply; shop and fuel transactions issued there are refused by the host.
    for label in ["applied sell", "ignored action"] {
        assert!(
            harness.event_counts.get(label).copied().unwrap_or(0) > 0,
            "{label}: {:?}",
            harness.event_counts
        );
    }
    for label in ["applied buy", "applied refuel"] {
        assert_eq!(
            harness.event_counts.get(label).copied().unwrap_or(0),
            0,
            "{label} away from its building: {:?}",
            harness.event_counts
        );
    }
}

#[test]
fn world_zero_latency_has_zero_corrections() {
    let mut harness = harness(60, 3, LinkConditions::perfect());
    harness.run_ticks(60 * 40);
    harness.settle(Duration::from_secs(1));
    assert!(harness.all_in_game());
    let summary = harness.summary();
    assert_eq!(summary.corrections, 0, "{summary:?}");
    assert_eq!(summary.clock_resyncs, 0);
    assert_converged(&harness);
    assert_bounded(&harness);
    harness.assert_actions_exactly_once();
    assert_played(&harness);
    assert_economy_played(&harness);
}

#[test]
fn world_private_state_reaches_only_its_owner() {
    use crate::world::to_sim_player;

    let mut harness = harness(60, 3, LinkConditions::perfect());
    harness.run_ticks(60 * 40);
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    let host = harness.world();
    let template_credits = host.player_template().credits;
    let mut other_rich = false;
    for client in &harness.clients {
        let me = client.session.player().expect("joined");
        let authority = client.session.authority().expect("in game");
        for player in host.player_ids() {
            let host_rig = host.player(player).expect("host rig");
            let client_rig = authority.player(player).expect("roster replicated");
            assert_eq!(
                client_rig.hull_level, host_rig.hull_level,
                "rig build is public"
            );
            if player == to_sim_player(me) {
                assert_eq!(client_rig.credits, host_rig.credits);
                assert_eq!(
                    authority.discovered_tile_count(player),
                    host.discovered_tile_count(player)
                );
                assert!(host.discovered_tile_count(player) > 0);
            } else {
                assert_eq!(client_rig.credits, template_credits, "loadouts are private");
                assert_eq!(authority.discovered_tile_count(player), 0);
                other_rich |= host_rig.credits != template_credits;
            }
        }
    }
    assert!(other_rich, "some rig's credits changed on the host");
}

#[test]
fn world_bad_internet_corrections_are_bounded_and_actions_exactly_once() {
    // 150 ms +/- 20 ms jitter with 3 % loss (plus duplication and reordering).
    let link = LinkConditions::new(150, 20, 3.0).with_chaos(2.0, 5.0);
    let mut harness = harness(60, 3, link);
    harness.run_for(Duration::from_secs(3));
    harness.reset_client_counters();
    harness.run_ticks(60 * 100);
    harness.settle(Duration::from_secs(3));
    let summary = harness.summary();
    println!("{summary:#?}");
    assert!(
        summary.corrections_per_client_sec < 0.5,
        "corrections stay rare: {summary:?}"
    );
    assert!(
        summary.mean_correction < 1.0,
        "corrections stay small (tiles): {summary:?}"
    );
    assert_converged(&harness);
    assert_bounded(&harness);
    harness.assert_actions_exactly_once();
    assert_played(&harness);
}

#[test]
fn world_works_at_tick_rates_30_60_120() {
    for rate in [30_u32, 60, 120] {
        // Zero latency: still zero corrections at every rate.
        let mut perfect = harness(rate, 2, LinkConditions::perfect());
        perfect.run_for(Duration::from_secs(12));
        perfect.settle(Duration::from_secs(1));
        let summary = perfect.summary();
        assert_eq!(summary.corrections, 0, "rate {rate}: {summary:?}");
        assert_converged(&perfect);
        perfect.assert_actions_exactly_once();
        assert_eq!(perfect.world().tick_rate_hz(), rate);

        // Moderate internet.
        let link = LinkConditions::new(40, 5, 1.0).with_chaos(1.0, 2.0);
        let mut config = HarnessConfig::new(SessionParams::new(rate), 2, link);
        config.tuning.snapshot_rate_hz = rate.min(30);
        let mut lossy = WorldHarness::new(config);
        lossy.run_for(Duration::from_secs(12));
        lossy.settle(Duration::from_secs(2));
        assert!(lossy.host_tick().0 >= u64::from(rate) * 13, "rate {rate}");
        assert_converged(&lossy);
        assert_bounded(&lossy);
        lossy.assert_actions_exactly_once();
        let summary = lossy.summary();
        assert!(
            summary.corrections_per_client_sec < 1.0,
            "rate {rate}: {summary:?}"
        );
    }
}

#[test]
fn world_scales_to_2_4_and_16_clients() {
    for (clients, seconds) in [(2_usize, 12_u64), (4, 12), (16, 6)] {
        let link = LinkConditions::new(30, 5, 1.0).with_chaos(1.0, 1.0);
        let mut harness = harness(60, clients, link);
        harness.run_for(Duration::from_secs(seconds));
        harness.settle(Duration::from_secs(2));
        assert_eq!(harness.world().player_count(), clients);
        assert!(harness.all_in_game());
        assert_converged(&harness);
        assert_bounded(&harness);
        harness.assert_actions_exactly_once();
        let summary = harness.summary();
        println!("{clients} clients: {summary:?}");
        assert!(
            summary.corrections_per_client_sec < 1.0,
            "{clients} clients: {summary:?}"
        );
    }
}

#[test]
fn world_join_in_progress_converges() {
    let link = LinkConditions::new(20, 3, 0.5);
    let mut harness = harness(60, 2, link);
    harness.run_for(Duration::from_secs(4));
    let late = harness.add_client(None);
    harness.run_for(Duration::from_secs(6));
    harness.settle(Duration::from_secs(1));
    assert!(harness.all_in_game());
    assert!(harness.clients[late].session.player().is_some());
    assert_eq!(harness.world().player_count(), 3);
    assert_converged(&harness);
    harness.assert_actions_exactly_once();
}

#[test]
#[ignore = "real-world parameter sweep report; run with --ignored --nocapture"]
fn world_parameter_sweep_report() {
    let rows =
        parameter_sweep::<WorldState>(&[30, 60, 120], &[10, 20, 30, 60], &sweep_links(), 3, 15);
    println!("{}", format_sweep(&rows));
}
