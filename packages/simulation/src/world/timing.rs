//! Per-tick constants derived once from the session tick rate.

use serde::{Deserialize, Serialize};

use crate::tick::{SimulationParams, TickRate};

use super::movement::{
    AUTHORITATIVE_AIR_DRAG_HALF_LIFE_SECONDS, AUTHORITATIVE_AIR_DRAG_RATE_PER_SECOND,
    AUTHORITATIVE_GROUND_DRAG_HALF_LIFE_SECONDS, AUTHORITATIVE_GROUND_DRAG_RATE_PER_SECOND,
};

/// Per-tick values derived from per-second gameplay constants for one session's tick rate.
///
/// Computed once at world creation (the only place a transcendental function is evaluated) and
/// stored in the world, so replicas built from a keyframe use the host's exact bits.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TickTiming {
    /// The session tick rate.
    pub rate: TickRate,
    /// Seconds per tick.
    pub delta_seconds: f32,
    /// Horizontal velocity retained per tick while airborne: `0.5^(dt / half_life)`.
    pub air_drag_per_tick: f32,
    /// Horizontal velocity retained per tick while grounded.
    pub ground_drag_per_tick: f32,
    /// Velocity gained per tick per unit of airborne acceleration: `(1 - drag) / drag_rate`.
    ///
    /// Together with the drag this integrates `dv/dt = a - k v` exactly for an input held over
    /// the tick, so speeds and distances do not depend on the tick rate.
    pub air_acceleration_gain: f32,
    /// Velocity gained per tick per unit of grounded acceleration.
    pub ground_acceleration_gain: f32,
}

impl TickTiming {
    /// Derives per-tick values for `params`.
    #[must_use]
    pub fn new(params: SimulationParams) -> Self {
        let rate = TickRate::new(params.tick_rate_hz);
        let dt = 1.0 / f64::from(rate.hz());
        let retention = |half_life: f32| 0.5_f64.powf(dt / f64::from(half_life));
        let air = retention(AUTHORITATIVE_AIR_DRAG_HALF_LIFE_SECONDS);
        let ground = retention(AUTHORITATIVE_GROUND_DRAG_HALF_LIFE_SECONDS);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "derived per-tick constants are stored as f32 like all gameplay scalars"
        )]
        Self {
            rate,
            delta_seconds: rate.delta_seconds(),
            air_drag_per_tick: air as f32,
            ground_drag_per_tick: ground as f32,
            air_acceleration_gain: ((1.0 - air) / f64::from(AUTHORITATIVE_AIR_DRAG_RATE_PER_SECOND))
                as f32,
            ground_acceleration_gain: ((1.0 - ground)
                / f64::from(AUTHORITATIVE_GROUND_DRAG_RATE_PER_SECOND))
                as f32,
        }
    }

    /// Ticks covering an authored duration in milliseconds.
    #[must_use]
    pub const fn ticks_for_millis(&self, millis: u64) -> u64 {
        self.rate.ticks_for_millis(millis)
    }
}

impl Default for TickTiming {
    fn default() -> Self {
        Self::new(SimulationParams::default())
    }
}

#[cfg(test)]
mod tests {
    use super::TickTiming;
    use crate::tick::SimulationParams;

    #[test]
    fn sixty_hertz_matches_the_legacy_per_tick_constants() {
        let timing = TickTiming::new(SimulationParams::new(60));
        assert!((timing.air_drag_per_tick - 0.86).abs() < 1e-5);
        assert!((timing.ground_drag_per_tick - 0.78).abs() < 1e-5);
        // Legacy update: v' = (v + a dt) * drag, i.e. gain == drag * dt.
        assert!((timing.air_acceleration_gain - 0.86 / 60.0).abs() < 1e-6);
        assert!((timing.ground_acceleration_gain - 0.78 / 60.0).abs() < 1e-6);
    }
}
