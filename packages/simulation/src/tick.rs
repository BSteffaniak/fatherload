//! Session tick rate and tick/second conversions.
//!
//! The simulation never assumes a particular tick rate. Every gameplay constant is authored in
//! per-second units (speeds, accelerations, drag half-lives, durations in milliseconds) and is
//! converted to per-tick values once, when a [`crate::WorldState`] is created for a session's
//! [`SimulationParams`]. The derived values live in the world ([`TickTiming`]) and are replicated
//! with it, so every peer of a session uses bit-identical per-tick constants.

use serde::{Deserialize, Serialize};

use crate::ids::SimulationTick;

/// Default session tick rate (plan §16 starting point).
pub const DEFAULT_TICK_RATE_HZ: u32 = 60;

/// Legacy alias of [`DEFAULT_TICK_RATE_HZ`] for the desktop shell, which still runs its own
/// fixed-step loop at the default rate. Gameplay code must use [`TickTiming`] instead.
pub const SIMULATION_HZ: u32 = DEFAULT_TICK_RATE_HZ;

/// [`SIMULATION_HZ`] as an `f32`.
#[allow(
    clippy::cast_precision_loss,
    reason = "small integer tick rates are exactly representable as f32"
)]
pub const SIMULATION_HZ_F32: f32 = SIMULATION_HZ as f32;

/// Fixed delta of the default tick rate. Legacy shell use only; gameplay code reads
/// [`TickTiming::delta_seconds`].
pub const FIXED_DELTA_SECONDS: f32 = 1.0 / SIMULATION_HZ_F32;

/// Session parameters the simulation needs at world creation (plan §16). Fixed for the lifetime
/// of a session.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SimulationParams {
    /// Simulation ticks per second.
    pub tick_rate_hz: u32,
}

impl SimulationParams {
    /// Creates parameters for `tick_rate_hz` (clamped to at least 1 Hz).
    #[must_use]
    pub const fn new(tick_rate_hz: u32) -> Self {
        Self {
            tick_rate_hz: if tick_rate_hz == 0 { 1 } else { tick_rate_hz },
        }
    }
}

impl Default for SimulationParams {
    fn default() -> Self {
        Self::new(DEFAULT_TICK_RATE_HZ)
    }
}

/// Tick-rate conversions for one session.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TickRate {
    hz: u32,
}

impl TickRate {
    /// Creates a tick rate (clamped to at least 1 Hz).
    #[must_use]
    pub const fn new(hz: u32) -> Self {
        Self {
            hz: if hz == 0 { 1 } else { hz },
        }
    }

    /// Ticks per second.
    #[must_use]
    pub const fn hz(self) -> u32 {
        self.hz
    }

    /// Ticks per second as `f32`.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "tick rates are small integers, exactly representable as f32"
    )]
    pub const fn hz_f32(self) -> f32 {
        self.hz as f32
    }

    /// Seconds per tick.
    #[must_use]
    pub const fn delta_seconds(self) -> f32 {
        1.0 / self.hz_f32()
    }

    /// Ticks covering an authored duration in milliseconds, rounded up (exact integer math).
    #[must_use]
    pub const fn ticks_for_millis(self, millis: u64) -> u64 {
        (millis.saturating_mul(self.hz as u64)).div_ceil(1000)
    }

    /// Ticks covering a (data-driven) duration in seconds, rounded up, at least one tick for any
    /// positive duration.
    #[must_use]
    pub fn ticks_for_seconds(self, seconds: f32) -> u64 {
        if seconds <= 0.0 || seconds.is_nan() {
            return 0;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "nonnegative bounded gameplay durations intentionally convert to fixed ticks"
        )]
        let ticks = (seconds * self.hz_f32()).ceil() as u64;
        ticks.max(1)
    }

    /// Seconds represented by a tick count.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "tick counts become presentation seconds"
    )]
    pub fn seconds_for_ticks(self, ticks: u64) -> f32 {
        (ticks as f64 / f64::from(self.hz)) as f32
    }

    /// Seconds from `now` until `deadline` (zero once passed).
    #[must_use]
    pub fn seconds_until(self, now: SimulationTick, deadline: SimulationTick) -> f32 {
        self.seconds_for_ticks(deadline.get().saturating_sub(now.get()))
    }

    /// Whether a periodic schedule fires on `tick`.
    ///
    /// The schedule fires every `period_millis`, offset by `phase_millis`, in simulated time. It
    /// fires on the first tick whose start time reaches each occurrence, so every occurrence
    /// fires exactly once at any tick rate (integer math, no drift). At 60 Hz a 750 ms period
    /// fires on every tick that is a multiple of 45.
    #[must_use]
    pub const fn schedule_fires(
        self,
        tick: SimulationTick,
        period_millis: u64,
        phase_millis: u64,
    ) -> bool {
        let tick = tick.get();
        if tick == 0 || period_millis == 0 {
            return false;
        }
        let hz = self.hz as u64;
        // Time in units of 1/(1000*hz) s: tick n starts at n*1000, occurrences are at
        // phase*hz + k*period*hz.
        let period = period_millis.saturating_mul(hz);
        let phase = phase_millis.saturating_mul(hz);
        let now = tick.saturating_mul(1000);
        let before = (tick - 1).saturating_mul(1000);
        if now < phase {
            return false;
        }
        let occurrences_now = (now - phase) / period;
        if before < phase {
            // The first occurrence (k = 0) happens in (before, now].
            return true;
        }
        let occurrences_before = (before - phase) / period;
        occurrences_now != occurrences_before
    }
}

impl Default for TickRate {
    fn default() -> Self {
        Self::new(DEFAULT_TICK_RATE_HZ)
    }
}

/// Converts `seconds` to ticks at the default rate. Legacy shell use only.
#[must_use]
pub fn simulation_ticks_for_seconds(seconds: f32) -> u64 {
    TickRate::default().ticks_for_seconds(seconds)
}

/// Seconds until `deadline` at the default rate. Legacy shell use only.
#[must_use]
pub fn simulation_seconds_until(now: SimulationTick, deadline: SimulationTick) -> f32 {
    TickRate::default().seconds_until(now, deadline)
}

#[cfg(test)]
mod tests {
    use super::{FIXED_DELTA_SECONDS, SIMULATION_HZ_F32, TickRate};
    use crate::ids::SimulationTick;

    #[test]
    fn derived_tick_constants_match_sixty_hertz_literals() {
        assert_eq!(SIMULATION_HZ_F32.to_bits(), 60.0_f32.to_bits());
        assert_eq!(FIXED_DELTA_SECONDS.to_bits(), (1.0_f32 / 60.0).to_bits());
        assert_eq!(
            TickRate::new(60).delta_seconds().to_bits(),
            FIXED_DELTA_SECONDS.to_bits()
        );
    }

    #[test]
    fn millisecond_durations_convert_exactly() {
        let rate = TickRate::new(60);
        assert_eq!(rate.ticks_for_millis(850), 51);
        assert_eq!(rate.ticks_for_millis(3_600), 216);
        assert_eq!(rate.ticks_for_millis(120_000), 7_200);
        assert_eq!(TickRate::new(30).ticks_for_millis(850), 26);
        assert_eq!(TickRate::new(240).ticks_for_millis(850), 204);
    }

    #[test]
    fn schedules_match_legacy_sixty_hertz_cadences() {
        let rate = TickRate::new(60);
        for tick in 1..2_000_u64 {
            let t = SimulationTick::new(tick);
            assert_eq!(
                rate.schedule_fires(t, 750, 0),
                tick.is_multiple_of(45),
                "{tick}"
            );
            assert_eq!(
                rate.schedule_fires(t, 1_500, 1_250),
                tick % 90 == 75,
                "{tick}"
            );
            assert_eq!(
                rate.schedule_fires(t, 1_500, 0),
                tick.is_multiple_of(90),
                "{tick}"
            );
        }
    }

    #[test]
    fn schedules_fire_once_per_period_at_any_rate() {
        for hz in [7_u32, 30, 60, 120, 144, 240] {
            let rate = TickRate::new(hz);
            let ticks = u64::from(hz) * 30;
            let fired = (1..=ticks)
                .filter(|tick| rate.schedule_fires(SimulationTick::new(*tick), 750, 0))
                .count();
            assert_eq!(fired, 40, "{hz} Hz");
        }
    }
}
