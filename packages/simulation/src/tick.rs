//! Fixed simulation tick rate and tick/second conversions.

use crate::ids::SimulationTick;

/// Fixed-tick simulation rate targeted by the multiplayer-ready architecture.
pub const SIMULATION_HZ: u32 = 60;

/// [`SIMULATION_HZ`] as an `f32`, for tick/second conversions.
#[allow(
    clippy::cast_precision_loss,
    reason = "small integer tick rates are exactly representable as f32"
)]
pub const SIMULATION_HZ_F32: f32 = SIMULATION_HZ as f32;

/// Fixed-tick simulation delta in seconds, derived from [`SIMULATION_HZ`].
pub const FIXED_DELTA_SECONDS: f32 = 1.0 / SIMULATION_HZ_F32;

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

#[cfg(test)]
mod tests {
    use super::{FIXED_DELTA_SECONDS, SIMULATION_HZ_F32};

    #[test]
    fn derived_tick_constants_match_sixty_hertz_literals() {
        assert_eq!(SIMULATION_HZ_F32.to_bits(), 60.0_f32.to_bits());
        assert_eq!(FIXED_DELTA_SECONDS.to_bits(), (1.0_f32 / 60.0).to_bits());
    }
}
