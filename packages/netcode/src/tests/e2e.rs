//! End-to-end harness tests (plan §11.2, §15, §16).

use std::time::Duration;

use super::support::{assert_bounded, assert_converged};
use crate::harness::{HarnessConfig, ToyHarness, format_sweep, parameter_sweep, sweep_links};
use crate::toy::ToySim;
use crate::{ClientPhase, LinkConditions, NetSimulation, SessionParams};

#[test]
fn zero_latency_has_zero_corrections() {
    let mut harness = ToyHarness::new(HarnessConfig::new(
        SessionParams::new(60),
        3,
        LinkConditions::perfect(),
    ));
    harness.run_ticks(60 * 60);
    harness.settle(Duration::from_secs(1));
    assert!(harness.all_in_game());
    let summary = harness.summary();
    assert_eq!(summary.corrections, 0, "{summary:?}");
    assert_eq!(summary.clock_resyncs, 0);
    assert_converged(&harness);
    assert_bounded(&harness);
    harness.assert_actions_exactly_once();
    assert!(harness.world().dug_tiles() > 20, "bots actually played");
}

#[test]
fn small_lan_latency_has_zero_corrections() {
    let link = LinkConditions::new(5, 1, 0.0);
    let mut harness = ToyHarness::new(HarnessConfig::new(SessionParams::new(60), 2, link));
    harness.run_ticks(60 * 30);
    harness.settle(Duration::from_secs(1));
    let summary = harness.summary();
    assert_eq!(summary.corrections, 0, "{summary:?}");
    assert_converged(&harness);
    harness.assert_actions_exactly_once();
}

#[test]
fn bad_internet_corrections_are_rare_and_actions_exactly_once() {
    // 150 ms +/- 20 ms jitter with 3 % loss (plus duplication and reordering on
    // the datagram uplink) for 10k host ticks.
    let link = LinkConditions::new(150, 20, 3.0).with_chaos(2.0, 5.0);
    let mut harness = ToyHarness::new(HarnessConfig::new(SessionParams::new(60), 3, link));
    harness.run_for(Duration::from_secs(3));
    harness.reset_client_counters();
    harness.run_ticks(10_000);
    harness.settle(Duration::from_secs(3));
    let summary = harness.summary();
    println!("{summary:#?}");
    assert!(
        summary.corrections_per_client_sec < 0.5,
        "corrections should be rare: {summary:?}"
    );
    assert!(
        summary.mean_correction < 1.0,
        "corrections should be small: {summary:?}"
    );
    assert_converged(&harness);
    assert_bounded(&harness);
    harness.assert_actions_exactly_once();

    // input_margin converges into the band and stays there.
    let params = harness.config.params;
    let target = harness.config.tuning.input_lead_target_ticks(&params);
    for client in &harness.clients {
        let tail = &client.margins[client.margins.len() / 2..];
        assert!(!tail.is_empty());
        let late = tail.iter().filter(|m| **m < 1).count();
        assert!(
            late * 50 <= tail.len(),
            "inputs rarely late: {late}/{}",
            tail.len()
        );
        let max = *tail.iter().max().unwrap();
        assert!(max <= target * 2 + 12, "margin stays bounded: max {max}");
        let host_stats = harness.host.client_stats(client.id).unwrap();
        assert!(
            host_stats.frames_on_time > host_stats.frames_late * 20,
            "{host_stats:?}"
        );
    }
}

#[test]
fn input_margin_converges_without_rtt_hint() {
    // 80 ms: within the resync threshold, converges through the 3 % speed
    // adjust. 250 ms: beyond it, converges through a hard resync.
    for (latency, expect_resync) in [(80u64, false), (250, true)] {
        let link = LinkConditions::new(latency, 5, 0.0);
        let mut config = HarnessConfig::new(SessionParams::new(60), 2, link);
        config.provide_rtt = false;
        let mut harness = ToyHarness::new(config);
        harness.run_for(Duration::from_secs(12));
        let target = harness
            .config
            .tuning
            .input_lead_target_ticks(&harness.config.params);
        for client in &harness.clients {
            let resyncs = client.session.stats().clock_resyncs;
            assert_eq!(
                resyncs > 0,
                expect_resync,
                "latency {latency}: resyncs {resyncs}"
            );
            let tail = &client.margins[client.margins.len().saturating_sub(60)..];
            assert!(!tail.is_empty());
            for margin in tail {
                assert!(
                    (1..=target * 2 + 2).contains(margin),
                    "latency {latency}: margin {margin} out of band"
                );
            }
        }
        assert_converged(&harness);
    }
}

#[test]
fn works_at_tick_rates_30_60_120() {
    for rate in [30u32, 60, 120] {
        let link = LinkConditions::new(40, 5, 1.0).with_chaos(1.0, 2.0);
        let mut config = HarnessConfig::new(SessionParams::new(rate), 2, link);
        config.tuning.snapshot_rate_hz = rate.min(30);
        let mut harness = ToyHarness::new(config);
        harness.run_for(Duration::from_secs(20));
        harness.settle(Duration::from_secs(2));
        assert!(harness.host_tick().0 >= u64::from(rate) * 21, "rate {rate}");
        assert_converged(&harness);
        assert_bounded(&harness);
        harness.assert_actions_exactly_once();
        let summary = harness.summary();
        assert!(
            summary.corrections_per_client_sec < 1.0,
            "rate {rate}: {summary:?}"
        );
    }
}

#[test]
fn scales_to_2_4_and_16_clients() {
    for clients in [2usize, 4, 16] {
        let link = LinkConditions::new(30, 5, 1.0).with_chaos(1.0, 1.0);
        let mut harness =
            ToyHarness::new(HarnessConfig::new(SessionParams::new(60), clients, link));
        harness.run_for(Duration::from_secs(12));
        harness.settle(Duration::from_secs(2));
        assert_eq!(harness.world().player_ids().len(), clients);
        assert!(harness.all_in_game());
        assert_converged(&harness);
        assert_bounded(&harness);
        harness.assert_actions_exactly_once();
    }
}

#[test]
fn relevance_radius_bounds_frame_size_with_16_clients() {
    let link = LinkConditions::new(10, 2, 0.0);
    let run = |radius: Option<f32>| {
        let mut config = HarnessConfig::new(SessionParams::new(60), 16, link);
        config.tuning.relevance_radius = radius;
        config.columns = 32;
        let mut harness = ToyHarness::new(config);
        harness.run_for(Duration::from_secs(3));
        harness.reset_client_counters();
        harness.run_for(Duration::from_secs(5));
        harness
    };
    let infinite = run(None);
    let limited = run(Some(6.0));
    let a = infinite.summary();
    let b = limited.summary();
    assert!(
        b.bytes_down_per_client_sec * 2.0 < a.bytes_down_per_client_sec,
        "interest management shrinks frames: {} vs {}",
        b.bytes_down_per_client_sec,
        a.bytes_down_per_client_sec
    );
    // With a 6-tile radius and 8-tile columns each client only gets itself in
    // full; far players arrive as markers.
    for client in &limited.clients {
        let view = client.session.view();
        assert!(view.far_players.len() >= 10, "far markers present");
    }
    assert!(
        b.max_frame_bytes < 400,
        "bounded frame size {}",
        b.max_frame_bytes
    );
    assert_converged(&infinite);
}

#[test]
fn join_in_progress_converges() {
    let link = LinkConditions::new(60, 10, 1.0).with_chaos(1.0, 2.0);
    let mut harness = ToyHarness::new(HarnessConfig::new(SessionParams::new(60), 2, link));
    harness.run_for(Duration::from_secs(10));
    let dug_before = harness.world().dug_tiles();
    assert!(dug_before > 0);
    let index = harness.add_client(None);
    harness.run_for(Duration::from_secs(10));
    harness.settle(Duration::from_secs(2));
    let late = &harness.clients[index];
    assert_eq!(late.session.phase(), ClientPhase::InGame);
    assert!(late.hash_compares > 100);
    assert_eq!(late.hash_mismatches, 0);
    // The joiner's terrain matches the host's (it received the keyframe plus ops).
    let authority = late.session.authority().unwrap();
    assert!(authority.dug_tiles() >= dug_before);
    assert_eq!(harness.world().player_ids().len(), 3);
    assert_converged(&harness);
    harness.assert_actions_exactly_once();
}

#[test]
fn reconnect_reclaims_player() {
    let link = LinkConditions::new(40, 5, 1.0);
    let mut harness = ToyHarness::new(HarnessConfig::new(SessionParams::new(60), 2, link));
    harness.run_for(Duration::from_secs(5));
    let player = harness.clients[0].session.player().unwrap();
    let token = harness.clients[0].session.reconnect_token().unwrap();
    let wallet_actions = harness.world().wallet(player).unwrap().actions_applied;
    harness.disconnect(0);
    harness.run_for(Duration::from_secs(2));
    // The player is reserved and idle, not removed.
    assert!(harness.world().player_ids().contains(&player));
    let index = harness.add_client(Some(token));
    harness.run_for(Duration::from_secs(5));
    harness.settle(Duration::from_secs(2));
    let rejoined = &harness.clients[index];
    assert_eq!(rejoined.session.player(), Some(player));
    assert_eq!(rejoined.session.phase(), ClientPhase::InGame);
    assert_eq!(harness.world().player_ids().len(), 2);
    assert!(harness.world().wallet(player).unwrap().actions_applied >= wallet_actions);
    assert!(rejoined.hash_compares > 50);
    assert_eq!(rejoined.hash_mismatches, 0);
    // The reconnected session restarts action ids at 1; the host reset its
    // ledger, so they are applied (not dropped as duplicates).
    assert!(rejoined.session.actions_issued() > 0);
    assert_eq!(rejoined.session.unacked_actions_len(), 0);
}

#[test]
fn reservation_expires_and_player_is_removed() {
    let link = LinkConditions::new(10, 0, 0.0);
    let mut config = HarnessConfig::new(SessionParams::new(60), 2, link);
    config.seed = 9;
    let mut harness = ToyHarness::new(config);
    harness.run_for(Duration::from_secs(1));
    let player = harness.clients[1].session.player().unwrap();
    harness.disconnect(1);
    // Default grace is 30 s.
    harness.run_for(Duration::from_secs(31));
    assert!(!harness.world().player_ids().contains(&player));
    assert_converged(&harness);
    let remaining = &harness.clients[0];
    assert!(
        !remaining
            .session
            .authority()
            .unwrap()
            .player_ids()
            .contains(&player)
    );
}

#[test]
fn epoch_change_rejects_stale_and_converges() {
    let link = LinkConditions::new(50, 10, 2.0).with_chaos(1.0, 3.0);
    let mut harness = ToyHarness::new(HarnessConfig::new(SessionParams::new(60), 3, link));
    harness.run_for(Duration::from_secs(5));
    let params = harness.config.params;
    let old_epoch = harness.host.epoch();
    harness
        .host
        .new_epoch(crate::toy::ToySim::new(&params, harness.config.columns, 48));
    harness.run_for(Duration::from_secs(5));
    assert!(
        harness.host.stats().stale_epoch_inputs > 0,
        "in-flight old inputs were rejected"
    );
    for client in &harness.clients {
        assert_eq!(client.session.epoch(), harness.host.epoch());
        assert_ne!(client.session.epoch(), old_epoch);
        assert_eq!(client.session.phase(), ClientPhase::InGame);
    }
    harness.reset_client_counters();
    harness.run_for(Duration::from_secs(5));
    assert_converged(&harness);
    assert_bounded(&harness);
}

#[test]
fn lobby_waits_for_start() {
    let link = LinkConditions::new(20, 2, 0.0);
    let mut config = HarnessConfig::new(SessionParams::new(60), 3, link);
    config.start_in_lobby = true;
    let mut harness = ToyHarness::new(config);
    harness.run_for(Duration::from_secs(1));
    assert_eq!(harness.host_tick().0, 0);
    for client in &mut harness.clients {
        assert_eq!(client.session.phase(), ClientPhase::Lobby);
        assert_eq!(client.session.lobby().len(), 3);
        client.session.set_ready(true);
    }
    harness.run_for(Duration::from_millis(200));
    assert!(harness.host.roster().iter().all(|entry| entry.ready));
    harness.host.start_game();
    harness.run_for(Duration::from_secs(5));
    assert!(harness.all_in_game());
    assert!(harness.host_tick().0 > 250);
    assert_converged(&harness);
}

#[test]
fn per_client_snapshot_rates_differ() {
    let link = LinkConditions::new(20, 2, 0.0);
    let mut harness = ToyHarness::new(HarnessConfig::new(SessionParams::new(60), 2, link));
    let slow = harness.clients[1].id;
    let mut tuning = harness.config.tuning;
    tuning.snapshot_rate_hz = 10;
    harness.host.set_client_tuning(slow, tuning);
    harness.run_for(Duration::from_secs(1));
    harness.reset_client_counters();
    let before: Vec<u64> = harness.clients.iter().map(|c| c.frames_down).collect();
    harness.run_for(Duration::from_secs(5));
    let fast_frames = harness.clients[0].frames_down - before[0];
    let slow_frames = harness.clients[1].frames_down - before[1];
    assert!((140..=160).contains(&fast_frames), "fast {fast_frames}");
    assert!((45..=55).contains(&slow_frames), "slow {slow_frames}");
    assert_converged(&harness);
}

#[test]
fn remote_players_interpolate_smoothly() {
    let link = LinkConditions::new(50, 10, 1.0);
    let mut harness = ToyHarness::new(HarnessConfig::new(SessionParams::new(60), 2, link));
    harness.run_for(Duration::from_secs(3));
    let other = harness.clients[1].session.player().unwrap();
    let mut last = None;
    let mut max_step = 0.0f32;
    for _ in 0..600 {
        harness.step_dt(Duration::from_millis(8));
        let view = harness.clients[0].session.view();
        if let Some(pose) = view.remote_poses.get(&other).copied() {
            if let Some(previous) = last {
                let step: f32 = pose.distance(&previous);
                max_step = max_step.max(step);
            }
            last = Some(pose);
        }
        assert!(view.local_pose.is_some());
    }
    assert!(last.is_some(), "remote player visible");
    // Max speed is 10 tiles/s; an 8 ms frame should move well under 0.5 tiles.
    assert!(max_step < 0.5, "remote motion is smooth: {max_step}");
}

#[test]
#[ignore = "parameter sweep report; run with --ignored --nocapture"]
fn parameter_sweep_report() {
    let rows = parameter_sweep::<ToySim>(&[30, 60, 120], &[10, 20, 30, 60], &sweep_links(), 3, 15);
    println!("{}", format_sweep(&rows));
}

#[test]
fn shared_column_contention_converges() {
    // All bots share one column, so they dig each other's tiles and the
    // client's prediction (others on neutral input) is genuinely wrong at
    // times. Authority must still match the host exactly, corrections must
    // stay small and actions must still be applied exactly once.
    let link = LinkConditions::new(150, 20, 3.0).with_chaos(2.0, 5.0);
    let mut config = HarnessConfig::new(SessionParams::new(60), 4, link);
    config.columns = 1;
    let mut harness = ToyHarness::new(config);
    harness.run_for(Duration::from_secs(3));
    harness.reset_client_counters();
    harness.run_ticks(3_000);
    harness.settle(Duration::from_secs(3));
    let summary = harness.summary();
    assert!(
        summary.corrections_per_client_sec < 1.0,
        "corrections stay rare under contention: {summary:?}"
    );
    assert!(
        summary.mean_correction < 1.0,
        "corrections stay small under contention: {summary:?}"
    );
    assert_converged(&harness);
    assert_bounded(&harness);
    harness.assert_actions_exactly_once();
}
