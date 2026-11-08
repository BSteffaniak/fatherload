//! Fixed simulation tick rate and tick/second conversions.

use crate::ids::SimulationTick;

/// Fixed-tick simulation rate targeted by the multiplayer-ready architecture.
pub const SIMULATION_HZ: u32 = 60;

/// Fixed-tick simulation delta in seconds.
pub const FIXED_DELTA_SECONDS: f32 = 1.0 / 60.0;

pub const SIMULATION_HZ_F32: f32 = 60.0;

#[must_use]
pub fn simulation_ticks_for_seconds(seconds: f32) -> u64 {
    if seconds <= 0.0 {
        return 0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "nonnegative bounded gameplay durations intentionally convert to fixed ticks"
    )]
    let ticks = (seconds * SIMULATION_HZ_F32).ceil() as u64;
    ticks.max(1)
}

#[must_use]
pub fn simulation_seconds_until(now: SimulationTick, deadline: SimulationTick) -> f32 {
    let remaining = deadline.get().saturating_sub(now.get());
    let bounded_remaining = u16::try_from(remaining).unwrap_or(u16::MAX);
    f32::from(bounded_remaining) / SIMULATION_HZ_F32
}
