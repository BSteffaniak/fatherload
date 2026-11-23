//! The distortion strata's sounds (Wave 9, Descent VI part A): a bed for each stratum (Pixel
//! Rot's in three ever coarser bit depths, crossfaded by the rot tier) and the one-shots of
//! their rules.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop.

#![allow(
    clippy::cast_possible_truncation,
    reason = "step indexes of short renders are small and clamped"
)]

use super::{
    FilterMode, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, burst, glide, grains,
    loop_length, loop_note, mix_into, render, segments, sine, thump,
};

/// A mirror's chime: a glass bell swelling in reversed, then ringing forward, the two halves
/// meeting in the middle like a reflection.
pub(super) fn glass_chime(rng: &mut Rng) -> Vec<f32> {
    let pitch = [1_318.5_f32, 1_567.98, 1_760.0][usize::try_from(rng.next_u64() % 3).unwrap_or(0)];
    let tone = bell(pitch, 3.0, 1.2, 0.9);
    let mut reversed: Vec<f32> = tone.iter().rev().copied().collect();
    let length = reversed.len();
    reversed.extend_from_slice(&tone);
    // A high shimmer over the meeting point.
    let mut out = reversed;
    let seconds = 2.0 * 0.9 * 1.0;
    #[allow(clippy::cast_precision_loss, reason = "a short render")]
    let middle = length as f32 / (out.len() as f32) * seconds;
    grains(
        rng,
        &mut out,
        8,
        (middle - 0.1, middle + 0.25),
        (6_000.0, 9_500.0),
        0.06,
    );
    out
}

/// Glass shattering: a bright crack, a spray of shards, a few high clinks settling.
pub(super) fn mirror_shatter(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_200.0,
        0.8,
        0.35,
    );
    grains(rng, &mut out, 22, (0.0, 0.5), (4_500.0, 11_000.0), 0.22);
    for (index, time) in [0.05_f32, 0.14, 0.27, 0.41].into_iter().enumerate() {
        let note = 2_200.0 + 340.0 * f32::from(u8::try_from(index).unwrap_or(0));
        mix_into(
            &mut out,
            &bell(note * rng.jitter(0.03), 2.7, 0.8, 0.35),
            time,
            0.08,
        );
    }
    out
}

/// Gravity turning: a low groan sliding down and back, a deep rumble under it.
pub(super) fn gravity_groan(rng: &mut Rng) -> Vec<f32> {
    let mut groan = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, 400.0, 2.0);
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_filter = Svf::new(FilterMode::LowPass, 120.0, 0.7);
    let start = rng.range(95.0, 120.0);
    render(1.3, |t| {
        let pitch = glide(t, start, start * 0.62, 0.5) * (1.0 + 0.03 * sine(t, 3.0));
        let envelope = segments(t, &[(0.0, 0.0), (0.15, 1.0), (0.9, 0.7), (1.3, 0.0)]);
        filter.run(groan.next(pitch), 260.0 + 300.0 * envelope, 2.0) * 0.4 * envelope
            + rumble_filter.process(rumble.next()) * 0.5 * envelope
    })
}

/// A square wave at `frequency` (`t` in seconds).
fn square(t: f32, frequency: f32) -> f32 {
    if (t * frequency).fract() < 0.5 {
        1.0
    } else {
        -1.0
    }
}

/// `value` crushed to `levels` steps a side.
fn crush(value: f32, levels: f32) -> f32 {
    (value * levels).round() / levels
}

/// A glitch: three bit-crushed square blips at random steps, the last cut short.
pub(super) fn glitch_blip(rng: &mut Rng) -> Vec<f32> {
    let steps: Vec<f32> = (0..3).map(|_| rng.range(380.0, 1_600.0)).collect();
    render(0.24, |t| {
        let index = usize::try_from((t / 0.07) as i64).unwrap_or(0).min(2);
        let held = (t * 6_000.0).floor() / 6_000.0;
        let envelope = segments(t, &[(0.0, 0.0), (0.005, 1.0), (0.2, 0.6), (0.24, 0.0)]);
        crush(square(held, steps[index]) * 0.35 * envelope, 4.0)
    })
}

/// A data core recovered: a rising square arpeggio, an eight-bit chime.
pub(super) fn data_chirp(rng: &mut Rng) -> Vec<f32> {
    let root = rng.range(520.0, 560.0);
    let ratios = [1.0_f32, 1.25, 1.5, 2.0];
    render(0.5, |t| {
        let index = usize::try_from((t / 0.09) as i64).unwrap_or(0).min(3);
        let envelope = segments(t, &[(0.0, 0.0), (0.004, 1.0), (0.38, 0.7), (0.5, 0.0)]);
        crush(square(t, root * ratios[index]) * 0.3 * envelope, 6.0)
    })
}

/// The Mirror Strata's bed: a glassy celesta line that plays forward over the first half of
/// the loop and backward over the second (a palindrome), over a bowed glass pad and the ring
/// of far chimes.
pub(super) fn mirror_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pad = Osc::new(Wave::Sine);
    let mut pad_high = Osc::new(Wave::Sine);
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut air_filter = Svf::new(FilterMode::BandPass, 5_000.0, 1.5);
    let mut out = render(loop_length(), |t| {
        let swell = 0.6 + 0.4 * sine(t, 0.25);
        (pad.next(220.0) * 0.3 + pad_high.next(330.0) * 0.2) * swell * 0.3
            + air_filter.process(air.next()) * 0.02
    });
    // E minor, up and back: the second half is the first, reversed.
    let line = [659.26_f32, 783.99, 987.77, 880.0];
    let step = LOOP_SECONDS / 8.0;
    for (index, note) in line.iter().chain(line.iter().rev()).enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "eight steps")]
        let start = index as f32 * step;
        let celesta = bell(note * rng.jitter(0.001), 4.0, 1.0, 0.9);
        loop_note(&mut out, &celesta, start, 0.11);
    }
    out
}

/// The Gravity Shear's bed: an arpeggio that turns over itself (up, then the same notes down an
/// octave), low strings sliding a tone up and down, wind turning.
pub(super) fn shear_loop(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut low_filter = Svf::new(FilterMode::LowPass, 360.0, 1.4);
    let mut wind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut wind_filter = Svf::new(FilterMode::BandPass, 700.0, 0.9);
    let mut out = render(loop_length(), |t| {
        // The strings slide between D and E across the loop.
        let pitch = 73.42 * (1.0 + 0.06 * (0.5 + 0.5 * sine(t, 0.25)));
        low_filter.run(low.next(pitch), 300.0, 1.4) * 0.22
            + wind_filter.run(wind.next(), 500.0 + 400.0 * sine(t, 0.5), 0.9) * 0.05
    });
    let up = [293.66_f32, 349.23, 440.0, 523.25];
    let step = LOOP_SECONDS / 8.0;
    for (index, note) in up
        .iter()
        .copied()
        .chain(up.iter().rev().map(|note| note * 0.5))
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "eight steps")]
        let start = index as f32 * step;
        let pluck = bell(note * rng.jitter(0.002), 2.0, 2.2, 0.6);
        loop_note(&mut out, &pluck, start, 0.13);
    }
    for beat in [0.0, 2.0] {
        loop_note(&mut out, &thump(70.0, 40.0, 0.4, 1.2), beat, 0.25);
    }
    out
}

/// Pixel Rot's bed at bit depth `levels` and a sample-and-hold rate of `hold` Hz: a chiptune
/// lead and bass over a noise-channel tick. The deeper tiers play it coarser.
fn rot_tune(rng: &mut Rng, levels: f32, hold: f32) -> Vec<f32> {
    let lead = [
        523.25_f32, 659.26, 783.99, 659.26, 587.33, 698.46, 880.0, 698.46,
    ];
    let bass = [130.81_f32, 130.81, 146.83, 146.83];
    let mut tick = Noise::new(NoiseColor::White, rng.next_u64());
    let step = LOOP_SECONDS / 8.0;
    render(loop_length(), |t| {
        let sampled = (t * hold).floor() / hold;
        let phase = sampled % LOOP_SECONDS;
        let index = usize::try_from((phase / step) as i64).unwrap_or(0).min(7);
        let into = phase - step * f32::from(u8::try_from(index).unwrap_or(0));
        let gate = segments(
            into,
            &[(0.0, 0.0), (0.01, 1.0), (step * 0.7, 0.6), (step, 0.0)],
        );
        let lead_voice = square(sampled, lead[index]) * 0.16 * gate;
        let bass_voice = square(sampled, bass[index / 2]) * 0.14;
        let hat = if into < 0.03 { tick.next() * 0.08 } else { 0.0 };
        crush(lead_voice + bass_voice + hat, levels)
    })
}

/// Pixel Rot's bed, near its top: eight-bit, clean.
pub(super) fn rot_loop(rng: &mut Rng) -> Vec<f32> {
    rot_tune(rng, 32.0, 22_050.0)
}

/// Pixel Rot's bed, deeper: four-bit, held at 6 kHz.
pub(super) fn rot_loop_crushed(rng: &mut Rng) -> Vec<f32> {
    rot_tune(rng, 6.0, 6_000.0)
}

/// Pixel Rot's bed at its bottom: two-bit, held at 2 kHz, barely holding together.
pub(super) fn rot_loop_ruined(rng: &mut Rng) -> Vec<f32> {
    rot_tune(rng, 2.0, 2_000.0)
}
