//! Deterministic quantization of simulation scalars (positions, velocities, fuel).

pub const SIMULATION_SUBPIXELS_PER_PIXEL: f32 = 1_024.0;

#[must_use]
pub fn quantize_simulation_scalar(value: f32) -> f32 {
    (value * SIMULATION_SUBPIXELS_PER_PIXEL).round() / SIMULATION_SUBPIXELS_PER_PIXEL
}
