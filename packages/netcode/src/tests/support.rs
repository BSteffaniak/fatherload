//! Assertions shared by the end-to-end suites.

use crate::harness::{Harness, HarnessSim};

/// Authority hashes matched the host at every compared frame.
pub fn assert_converged<S: HarnessSim>(harness: &Harness<S>) {
    let summary = harness.summary();
    assert!(summary.hash_compares > 0, "harness compared hashes");
    assert_eq!(
        summary.hash_mismatches, 0,
        "authority hash == host hash at every frame"
    );
    assert!(summary.debug_hash_checks > 0, "client checked debug hashes");
    assert_eq!(summary.debug_hash_mismatches, 0);
}

/// No queue, buffer or ledger grew without bound.
pub fn assert_bounded<S: HarnessSim>(harness: &Harness<S>) {
    let params = harness.config.params;
    let max_prediction = params.ms_to_ticks_ceil(harness.config.tuning.max_prediction_ms);
    for client in harness.clients.iter().filter(|c| c.connected) {
        let session = &client.session;
        assert!(
            session.pending_len() as u64 <= max_prediction,
            "pending bounded"
        );
        assert!(session.stats().max_pending as u64 <= max_prediction);
        assert!(
            session.remote_buffer().max_depth() <= 128,
            "snapshot buffer bounded"
        );
        assert!(
            session.predicted_event_keys() < 256,
            "event dedup set bounded"
        );
        assert!(
            session.unacked_actions_len() < 64,
            "unacked actions bounded"
        );
        assert!(harness.host.buffered_inputs(client.id) as u64 <= max_prediction);
        assert!(
            harness.host.action_backlog(client.id) < 64,
            "host ledger bounded"
        );
        assert!(client.down.in_flight() < 1000, "downlink queue bounded");
    }
}
