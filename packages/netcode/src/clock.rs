//! Client tick clock: run-ahead lead, smooth speed adjust and hard resync
//! (plan §5).

use std::time::Duration;

use crate::ids::{InputSeq, Tick};
use crate::params::{NetTuning, SessionParams};

/// Current speed adjustment of the client tick clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClockSpeed {
    /// Real time.
    #[default]
    Normal,
    /// `clock_adjust_pct` faster (inputs are arriving too late).
    Fast,
    /// `clock_adjust_pct` slower (inputs are arriving too early).
    Slow,
}

/// What the clock did with an `input_margin` report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginResponse {
    /// The report predates the last resync and was ignored.
    Ignored,
    /// Margin in band; speed unchanged.
    Steady,
    /// Speed changed to the contained value.
    Adjusted(ClockSpeed),
    /// Hard resync by `delta` ticks (positive: jump ahead, negative: stall).
    Resynced {
        /// Tick correction applied.
        delta: i64,
    },
}

/// The client's fixed-tick clock.
///
/// It accumulates `nanoseconds * tick_rate * speed_permille` so one tick is an
/// exact integer threshold and no time is lost to rounding. Hard resyncs never
/// move the tick backwards: running ahead too far stalls for that many ticks,
/// running behind schedules extra catch-up ticks. That keeps input sequence
/// and target tick strictly increasing together.
#[derive(Clone, Debug, Default)]
pub struct ClientClock {
    tick: Tick,
    acc: u128,
    speed: ClockSpeed,
    resync_guard: InputSeq,
    pending_jump: u64,
    stall: u64,
    started: bool,
}

const PERMILLE: u128 = 1000;

impl ClientClock {
    /// Creates a stopped clock.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts (or restarts) the clock at `tick`.
    pub fn start(&mut self, tick: Tick) {
        *self = Self {
            tick,
            started: true,
            ..Self::default()
        };
    }

    /// The last tick the client produced.
    #[must_use]
    pub const fn tick(&self) -> Tick {
        self.tick
    }

    /// Current speed adjustment.
    #[must_use]
    pub const fn speed(&self) -> ClockSpeed {
        self.speed
    }

    /// Ticks still owed from a forward resync.
    #[must_use]
    pub const fn pending_jump(&self) -> u64 {
        self.pending_jump
    }

    /// Fraction of the next tick already accumulated, in `[0, 1)`.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn alpha(&self) -> f32 {
        let unit = SessionParams::accumulator_unit() * PERMILLE;
        ((self.acc as f64) / (unit as f64)).clamp(0.0, 1.0) as f32
    }

    /// Advances real time by `dt` and returns how many ticks are due. Call
    /// [`ClientClock::step`] once per due tick.
    pub fn advance(&mut self, dt: Duration, params: &SessionParams, tuning: &NetTuning) -> u32 {
        if !self.started {
            return 0;
        }
        let adjust = adjust_permille(tuning);
        let permille = match self.speed {
            ClockSpeed::Normal => PERMILLE,
            ClockSpeed::Fast => PERMILLE + adjust,
            ClockSpeed::Slow => PERMILLE - adjust,
        };
        let unit = SessionParams::accumulator_unit() * PERMILLE;
        self.acc += dt.as_nanos() * u128::from(params.rate()) * permille;
        let max = u64::from(tuning.max_catch_up_ticks.max(1));
        let mut due = 0u64;
        while self.acc >= unit && due < max {
            self.acc -= unit;
            due += 1;
        }
        if self.acc >= unit {
            // Catch-up clamp: drop the backlog rather than spiral.
            self.acc %= unit;
        }
        let stalled = due.min(self.stall);
        self.stall -= stalled;
        due -= stalled;
        let extra = self.pending_jump.min(max);
        self.pending_jump -= extra;
        due += extra;
        u32::try_from(due).unwrap_or(u32::MAX)
    }

    /// Advances to the next tick and returns it.
    pub const fn step(&mut self) -> Tick {
        self.tick = self.tick.next();
        self.tick
    }

    /// Schedules catch-up so the clock reaches at least `tick`.
    pub fn catch_up_to(&mut self, tick: Tick) {
        let behind = u64::try_from(tick.diff(self.tick)).unwrap_or(0);
        self.stall = 0;
        self.pending_jump = self.pending_jump.max(behind);
    }

    /// Feeds an `input_margin` report from a server frame.
    ///
    /// `margin_seq` is the input the margin was measured from; `next_seq` is the
    /// next sequence this client will send (used to ignore stale reports after
    /// a resync).
    pub fn on_margin(
        &mut self,
        margin: i64,
        margin_seq: InputSeq,
        next_seq: InputSeq,
        params: &SessionParams,
        tuning: &NetTuning,
    ) -> MarginResponse {
        if margin_seq < self.resync_guard {
            return MarginResponse::Ignored;
        }
        let target = tuning.input_lead_target_ticks(params);
        let low = ((target + 1) / 2).max(1);
        let high = target * 2;
        let resync = i64::try_from(params.ms_to_ticks_ceil(tuning.clock_resync_ms))
            .unwrap_or(i64::MAX)
            .max(high + 1);
        let error = margin - target;
        if error.abs() > resync {
            let delta = target - margin;
            if delta > 0 {
                self.pending_jump += delta.unsigned_abs();
            } else {
                self.stall += delta.unsigned_abs();
            }
            self.speed = ClockSpeed::Normal;
            self.resync_guard = next_seq;
            return MarginResponse::Resynced { delta };
        }
        let previous = self.speed;
        self.speed = match self.speed {
            _ if margin < low => ClockSpeed::Fast,
            _ if margin > high => ClockSpeed::Slow,
            ClockSpeed::Fast if margin >= target => ClockSpeed::Normal,
            ClockSpeed::Slow if margin <= target => ClockSpeed::Normal,
            speed => speed,
        };
        if self.speed == previous {
            MarginResponse::Steady
        } else {
            MarginResponse::Adjusted(self.speed)
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn adjust_permille(tuning: &NetTuning) -> u128 {
    let permille = (f64::from(tuning.clock_adjust_pct) * 10.0).round();
    permille.clamp(0.0, 500.0) as u128
}
