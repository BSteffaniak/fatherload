//! Space's things (Wave 12 systems): a star's flare warning and its burst, a black hole's
//! strain alarm, a wormhole's tunnel and the crunch of ore broken off an asteroid in flight.

#![allow(
    clippy::suboptimal_flops,
    reason = "presentation code: sound math reads top to bottom"
)]

use super::{
    FilterMode, Noise, NoiseColor, Osc, Rng, Svf, Wave, mix_into, perc, render, samples_for,
    segments,
};

/// A flare coming: three slow rising tones over a low solar rumble (unhurried: there is time).
pub(super) fn flare_warning(rng: &mut Rng) -> Vec<f32> {
    let seconds = 2.4;
    let mut out = vec![0.0; samples_for(seconds)];
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut lowpass = Svf::new(FilterMode::LowPass, 180.0, 0.6);
    let rumble = render(seconds, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.8, 1.0), (seconds, 0.0)]);
        lowpass.process(noise.next()) * swell * 0.5
    });
    mix_into(&mut out, &rumble, 0.0, 1.0);
    let base = rng.range(392.0, 400.0);
    for index in 0..3_u8 {
        let start = f32::from(index) * 0.55;
        let pitch = base * (1.0 + f32::from(index) * 0.125);
        let mut tone = Osc::new(Wave::Triangle).with_phase(rng.unit());
        let note = render(0.5, |t| {
            tone.next(pitch * (1.0 + t * 0.06))
                * segments(t, &[(0.0, 0.0), (0.05, 1.0), (0.5, 0.0)])
        });
        mix_into(&mut out, &note, start, 0.3);
    }
    out
}

/// A flare striking: a roaring wash of bright noise that swells and slowly lets go.
pub(super) fn flare_burst(rng: &mut Rng) -> Vec<f32> {
    let seconds = 2.8;
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 900.0, 0.4);
    let mut low = Osc::new(Wave::Sine).with_phase(rng.unit());
    render(seconds, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.4, 1.0), (1.2, 0.8), (seconds, 0.0)]);
        band.set(600.0 + 1_400.0 * swell, 0.4);
        (band.process(noise.next()) * 0.8 + low.next(55.0) * 0.4) * swell
    })
}

/// A black hole's pull beating the thrusters: a low two-tone alarm bending down.
pub(super) fn hole_warning(rng: &mut Rng) -> Vec<f32> {
    let seconds = 1.6;
    let mut tone = Osc::new(Wave::Square).with_phase(rng.unit());
    let mut lowpass = Svf::new(FilterMode::LowPass, 900.0, 0.7);
    render(seconds, |t| {
        let high = (t * 2.5).fract() < 0.5;
        let pitch = if high { 330.0 } else { 247.0 } * (1.0 - t * 0.12);
        lowpass.process(tone.next(pitch))
            * segments(
                t,
                &[
                    (0.0, 0.0),
                    (0.04, 1.0),
                    (seconds - 0.2, 0.8),
                    (seconds, 0.0),
                ],
            )
            * 0.4
    })
}

/// A wormhole's tunnel: a deep whoosh rising through a filter sweep, with a hollow ring.
pub(super) fn transit(rng: &mut Rng) -> Vec<f32> {
    let seconds = 3.2;
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut sweep = Svf::new(FilterMode::BandPass, 200.0, 0.3);
    let mut hollow = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut sub = Osc::new(Wave::Sine).with_phase(rng.unit());
    render(seconds, |t| {
        let share = t / seconds;
        let swell = segments(t, &[(0.0, 0.0), (0.5, 1.0), (2.6, 0.9), (seconds, 0.0)]);
        sweep.set(200.0 + 2_600.0 * share * share, 0.3);
        (sweep.process(noise.next()) * 0.6
            + hollow.next(220.0 + 440.0 * share) * 0.15
            + sub.next(48.0) * 0.35)
            * swell
    })
}

/// Ore broken off a rock: a dry crunch and a bright glint.
pub(super) fn ore_crunch(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples_for(0.45)];
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 1_400.0, 0.5);
    let crunch = render(0.25, |t| low.process(noise.next()) * perc(t, 0.002, 0.08));
    mix_into(&mut out, &crunch, 0.0, 0.9);
    let pitch = rng.range(1_500.0, 1_800.0);
    let mut glint = Osc::new(Wave::Sine).with_phase(rng.unit());
    let shine = render(0.35, |t| glint.next(pitch) * perc(t, 0.001, 0.12));
    mix_into(&mut out, &shine, 0.06, 0.35);
    out
}
