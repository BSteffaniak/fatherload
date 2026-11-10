//! Objective measurements of rendered sounds, standing in for listening: levels, DC offset,
//! spectral centroid and click/discontinuity checks. The `--render-audio` report and the audio
//! tests both use these.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "analysis converts sample counts to floats for averages and keeps textbook FFT \
              formulas readable"
)]

#[cfg(any(test, not(target_os = "emscripten")))]
use std::f32::consts::TAU;

use super::dsp::{FilterMode, Svf, samples_for};
#[cfg(any(test, not(target_os = "emscripten")))]
use super::dsp::{SR, gain_to_db};

/// Largest absolute sample.
#[must_use]
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |max, s| max.max(s.abs()))
}

/// Root-mean-square level over the whole buffer.
#[must_use]
#[cfg(any(test, not(target_os = "emscripten")))]
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Mean sample value.
#[must_use]
#[cfg(any(test, not(target_os = "emscripten")))]
pub fn dc_offset(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().sum::<f32>() / samples.len() as f32
}

/// Perceived loudness proxy: the loudest 100 ms window's RMS after a 100 Hz high-pass (low
/// rumble reads quieter than its energy suggests). Buffers shorter than the window use their
/// whole length.
#[must_use]
pub fn loudness(samples: &[f32]) -> f32 {
    let mut filter = Svf::new(FilterMode::HighPass, 100.0, 0.5);
    let weighted: Vec<f32> = samples.iter().map(|s| filter.process(*s)).collect();
    let window = samples_for(0.1).min(weighted.len()).max(1);
    let mut sum: f32 = weighted[..window.min(weighted.len())]
        .iter()
        .map(|s| s * s)
        .sum();
    let mut loudest = sum;
    for index in window..weighted.len() {
        sum +=
            weighted[index] * weighted[index] - weighted[index - window] * weighted[index - window];
        loudest = loudest.max(sum);
    }
    (loudest.max(0.0) / window as f32).sqrt()
}

/// How a loop's wrap (last sample -> first) compares with its ordinary steps: the wrap step
/// divided by the 99.9th percentile of the internal steps. Around 1 or below is seamless; a
/// click would stand far above it.
#[must_use]
#[cfg(any(test, not(target_os = "emscripten")))]
pub fn seam_ratio(samples: &[f32]) -> f32 {
    if samples.len() < 4 {
        return 0.0;
    }
    let wrap = (samples[0] - samples[samples.len() - 1]).abs();
    let mut steps: Vec<f32> = samples
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .collect();
    steps.sort_by(f32::total_cmp);
    let index = ((steps.len() as f32) * 0.999) as usize;
    let typical = steps[index.min(steps.len() - 1)].max(1e-6);
    wrap / typical
}

/// Spectral centroid (Hz): the power-weighted mean frequency, averaged over 2048-sample frames
/// weighted by their energy.
#[must_use]
#[cfg(any(test, not(target_os = "emscripten")))]
pub fn spectral_centroid(samples: &[f32]) -> f32 {
    const FRAME: usize = 2_048;
    let mut weighted = 0.0_f64;
    let mut total = 0.0_f64;
    let mut real = vec![0.0_f32; FRAME];
    let mut imag = vec![0.0_f32; FRAME];
    let mut start = 0;
    while start < samples.len() {
        for index in 0..FRAME {
            let window = 0.5 - 0.5 * (TAU * index as f32 / FRAME as f32).cos();
            real[index] = samples.get(start + index).copied().unwrap_or(0.0) * window;
            imag[index] = 0.0;
        }
        fft(&mut real, &mut imag);
        for bin in 1..FRAME / 2 {
            let power = f64::from(real[bin] * real[bin] + imag[bin] * imag[bin]);
            let frequency = bin as f64 * f64::from(SR) / FRAME as f64;
            weighted += power * frequency;
            total += power;
        }
        start += FRAME / 2;
    }
    if total <= 0.0 {
        0.0
    } else {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "centroid is far below f32 range"
        )]
        let centroid = (weighted / total) as f32;
        centroid
    }
}

/// In-place radix-2 FFT (`real.len()` must be a power of two).
#[cfg(any(test, not(target_os = "emscripten")))]
fn fft(real: &mut [f32], imag: &mut [f32]) {
    let n = real.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
    }
    let mut length = 2;
    while length <= n {
        let angle = -TAU / length as f32;
        for start in (0..n).step_by(length) {
            for k in 0..length / 2 {
                let (sin, cos) = (angle * k as f32).sin_cos();
                let a = start + k;
                let b = a + length / 2;
                let tr = real[b] * cos - imag[b] * sin;
                let ti = real[b] * sin + imag[b] * cos;
                real[b] = real[a] - tr;
                imag[b] = imag[a] - ti;
                real[a] += tr;
                imag[a] += ti;
            }
        }
        length <<= 1;
    }
}

/// Everything the render report prints about one buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg(any(test, not(target_os = "emscripten")))]
pub struct Measurements {
    pub seconds: f32,
    pub peak_db: f32,
    pub rms_db: f32,
    pub loudness_db: f32,
    pub dc: f32,
    pub centroid_hz: f32,
    /// First and last sample magnitudes (one-shots must start and end silent).
    pub edge: f32,
    /// Loop wrap step relative to ordinary steps (loops only meaningful).
    pub seam_ratio: f32,
}

#[cfg(any(test, not(target_os = "emscripten")))]
impl Measurements {
    #[must_use]
    pub fn of(samples: &[f32]) -> Self {
        let edge = samples
            .first()
            .map_or(0.0, |s| s.abs())
            .max(samples.last().map_or(0.0, |s| s.abs()));
        Self {
            seconds: samples.len() as f32 / SR,
            peak_db: gain_to_db(peak(samples)),
            rms_db: gain_to_db(rms(samples)),
            loudness_db: gain_to_db(loudness(samples)),
            dc: dc_offset(samples),
            centroid_hz: spectral_centroid(samples),
            edge,
            seam_ratio: seam_ratio(samples),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::dsp::{Osc, Wave, render};
    use super::*;

    #[test]
    fn centroid_finds_a_sine() {
        let mut osc = Osc::new(Wave::Sine);
        let tone = render(0.5, |_| osc.next(1_000.0));
        let centroid = spectral_centroid(&tone);
        assert!((centroid - 1_000.0).abs() < 40.0, "{centroid}");
    }

    #[test]
    fn levels_of_a_full_scale_sine() {
        let mut osc = Osc::new(Wave::Sine);
        let tone = render(0.5, |_| osc.next(1_000.0));
        assert!((peak(&tone) - 1.0).abs() < 1e-3);
        assert!((rms(&tone) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-2);
        assert!(dc_offset(&tone).abs() < 1e-3);
        assert!((loudness(&tone) - std::f32::consts::FRAC_1_SQRT_2).abs() < 2e-2);
    }

    #[test]
    fn seam_ratio_spots_a_click() {
        let mut osc = Osc::new(Wave::Sine);
        // Exactly 100 cycles: seamless.
        let seamless = render(0.1, |_| osc.next(1_000.0));
        assert!(seam_ratio(&seamless) < 1.5);
        let mut broken = seamless;
        broken.truncate(broken.len() - 11);
        assert!(seam_ratio(&broken) > 3.0);
    }
}
