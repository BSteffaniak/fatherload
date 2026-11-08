//! Session parameters and network tuning (plan §16).
//!
//! Nothing time-based is hard-coded in ticks: every value is authored in
//! milliseconds or Hz and converted with the session's `tick_rate_hz`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

const NANOS_PER_SEC: u128 = 1_000_000_000;

/// Parameters fixed for one session's lifetime. Chosen by the host and sent to
/// clients in [`crate::Welcome`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionParams {
    /// Simulation ticks per second. Every peer uses the host's value.
    pub tick_rate_hz: u32,
}

impl SessionParams {
    /// Starting-point tick rate from plan §16. Only used by `Default`.
    pub const DEFAULT_TICK_RATE_HZ: u32 = 60;

    /// Creates parameters for the given tick rate (clamped to at least 1 Hz).
    #[must_use]
    pub const fn new(tick_rate_hz: u32) -> Self {
        Self {
            tick_rate_hz: if tick_rate_hz == 0 { 1 } else { tick_rate_hz },
        }
    }

    /// Tick rate as a `u64`, never zero.
    #[must_use]
    pub const fn rate(&self) -> u64 {
        if self.tick_rate_hz == 0 {
            1
        } else {
            self.tick_rate_hz as u64
        }
    }

    /// Duration of one fixed tick (rounded to the nearest nanosecond).
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn tick_duration(&self) -> Duration {
        Duration::from_nanos(
            ((NANOS_PER_SEC + self.rate() as u128 / 2) / self.rate() as u128) as u64,
        )
    }

    /// Converts milliseconds to ticks, rounding up (safety margins never shrink).
    #[must_use]
    pub const fn ms_to_ticks_ceil(&self, ms: u32) -> u64 {
        (ms as u64 * self.rate()).div_ceil(1000)
    }

    /// Converts milliseconds to fractional ticks.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn ms_to_ticks_f64(&self, ms: f64) -> f64 {
        ms * self.rate() as f64 / 1000.0
    }

    /// Converts a duration to fractional ticks.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn duration_to_ticks_f64(&self, duration: Duration) -> f64 {
        duration.as_secs_f64() * self.rate() as f64
    }

    /// Converts a tick count to a duration.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn ticks_to_duration(&self, ticks: u64) -> Duration {
        Duration::from_nanos((ticks as u128 * NANOS_PER_SEC / self.rate() as u128) as u64)
    }

    /// Accumulator units per tick: the fixed-step accumulators count
    /// `nanoseconds * tick_rate`, so one tick is exactly one second of nanos.
    #[must_use]
    pub const fn accumulator_unit() -> u128 {
        NANOS_PER_SEC
    }
}

impl Default for SessionParams {
    fn default() -> Self {
        Self::new(Self::DEFAULT_TICK_RATE_HZ)
    }
}

/// How the remote-player interpolation delay is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterpDelay {
    /// Two snapshot intervals plus twice the measured frame-arrival jitter.
    Auto,
    /// A fixed delay in milliseconds.
    FixedMs(u32),
}

/// Network tuning. Can change live and can differ per client (plan §16).
///
/// Distances (`snap_threshold`, `correction_epsilon`, `relevance_radius`) are
/// in the simulation's world units as reported by [`crate::Pose`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetTuning {
    /// Host frames per second sent to this client (capped at the tick rate).
    pub snapshot_rate_hz: u32,
    /// Input frames carried per input datagram (newest plus redundant unacked).
    pub input_redundancy: u32,
    /// Desired input arrival margin on the host.
    pub input_lead_target_ms: u32,
    /// Client tick-speed nudge in percent when the margin is out of band.
    pub clock_adjust_pct: f32,
    /// Margin error beyond which the client hard-resyncs its tick.
    pub clock_resync_ms: u32,
    /// Remote interpolation delay policy.
    pub interp_delay: InterpDelay,
    /// Remote players extrapolate at most this long, then freeze.
    pub max_extrapolation_ms: u32,
    /// Host repeats a player's last controls for this long when input is missing.
    pub held_input_ms: u32,
    /// Half-life of the presentation-only correction offset.
    pub smoothing_half_life_ms: u32,
    /// Prediction errors at or above this distance snap instead of smoothing.
    pub snap_threshold: f32,
    /// Prediction errors below this distance are ignored (not a correction).
    pub correction_epsilon: f32,
    /// Interest radius for per-tick replication. `None` means infinite.
    pub relevance_radius: Option<f32>,
    /// Rate of position-only markers for players outside the relevance radius.
    pub far_player_rate_hz: u32,
    /// Host rejects input frames targeting further ahead than this.
    pub max_input_lead_ms: u32,
    /// Client stops predicting when this much input is unacknowledged.
    pub max_prediction_ms: u32,
    /// Maximum fixed ticks run by one `advance` call (catch-up clamp).
    pub max_catch_up_ticks: u32,
    /// Include the client-visible world hash every N frames (0 disables).
    pub debug_hash_interval: u32,
}

impl Default for NetTuning {
    fn default() -> Self {
        Self {
            snapshot_rate_hz: 30,
            input_redundancy: 8,
            input_lead_target_ms: 30,
            clock_adjust_pct: 3.0,
            clock_resync_ms: 250,
            interp_delay: InterpDelay::Auto,
            max_extrapolation_ms: 100,
            held_input_ms: 100,
            smoothing_half_life_ms: 50,
            snap_threshold: 2.0,
            correction_epsilon: 0.01,
            relevance_radius: None,
            far_player_rate_hz: 2,
            max_input_lead_ms: 1000,
            max_prediction_ms: 1000,
            max_catch_up_ticks: 8,
            debug_hash_interval: 0,
        }
    }
}

impl NetTuning {
    /// Snapshot interval in (fractional) ticks.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn snapshot_interval_ticks(&self, params: &SessionParams) -> f64 {
        let rate = self.snapshot_rate_hz.clamp(1, params.tick_rate_hz.max(1));
        params.rate() as f64 / f64::from(rate)
    }

    /// Target input margin in ticks (at least one: "arrived before simulation").
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub const fn input_lead_target_ticks(&self, params: &SessionParams) -> i64 {
        let ticks = params.ms_to_ticks_ceil(self.input_lead_target_ms) as i64;
        if ticks < 1 { 1 } else { ticks }
    }

    /// Remote interpolation delay in ticks for the given measured jitter.
    #[must_use]
    pub fn interp_delay_ticks(&self, params: &SessionParams, jitter: Duration) -> f64 {
        match self.interp_delay {
            InterpDelay::Auto => 2.0f64.mul_add(
                self.snapshot_interval_ticks(params),
                2.0 * params.duration_to_ticks_f64(jitter),
            ),
            InterpDelay::FixedMs(ms) => params.ms_to_ticks_f64(f64::from(ms)),
        }
    }
}
