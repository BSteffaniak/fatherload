//! The story finale's sounds (Wave 10 story): the call that marks a new Descent reached, and
//! the long rising swell of the ending, when the Heart wakes and light runs up the cable.

use super::{Osc, Rng, Wave, bell, mix_into, render, samples_for, segments};

/// A new Descent reached: a low bell answered a fifth and an octave up, over a deep hum that
/// swells and fades, like the cable's hum taking notice.
pub(super) fn story_descent_call(rng: &mut Rng) -> Vec<f32> {
    let root = rng.range(146.0, 150.0);
    let mut out = vec![0.0; samples_for(3.0)];
    let mut hum = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut fifth = Osc::new(Wave::Sine).with_phase(rng.unit());
    let drone = render(3.0, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.9, 1.0), (3.0, 0.0)]);
        (hum.next(root * 0.5) + fifth.next(root * 0.75) * 0.5) * swell * 0.16
    });
    mix_into(&mut out, &drone, 0.0, 1.0);
    for (index, (ratio, gain)) in [(1.0_f32, 0.32_f32), (1.5, 0.24), (2.0, 0.2)]
        .into_iter()
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "three bells")]
        let start = 0.05 + index as f32 * 0.42;
        mix_into(
            &mut out,
            &bell(root * 2.0 * ratio, 2.2, 1.0, 1.3),
            start,
            gain,
        );
    }
    out
}

/// The ending: chords climbing from the Heart to the Moon (I, IV, V, then a high I held), a
/// shimmer of bells running upward over them, and a warm pad that breathes out at the end.
pub(super) fn story_cable_lit(rng: &mut Rng) -> Vec<f32> {
    let seconds = 3.9;
    let mut out = vec![0.0; samples_for(seconds)];
    let mut pad = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut third = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut fifth = Osc::new(Wave::Sine).with_phase(rng.unit());
    let chords: [(f32, [f32; 3]); 4] = [
        (0.0, [130.81, 164.81, 196.0]),
        (0.7, [174.61, 220.0, 261.63]),
        (1.4, [196.0, 246.94, 293.66]),
        (2.1, [261.63, 329.63, 392.0]),
    ];
    let body = render(seconds, |t| {
        let current = chords
            .iter()
            .rev()
            .find(|(start, _)| t >= *start)
            .map_or(chords[0].1, |(_, notes)| *notes);
        let swell = segments(t, &[(0.0, 0.0), (1.0, 0.8), (2.2, 1.0), (seconds, 0.0)]);
        (pad.next(current[0]) + third.next(current[1]) * 0.7 + fifth.next(current[2]) * 0.6)
            * swell
            * 0.11
    });
    mix_into(&mut out, &body, 0.0, 1.0);
    // Bells running up the cable: a rising line, faster as it climbs.
    let line = [
        523.25_f32, 587.33, 659.25, 783.99, 880.0, 1_046.5, 1_174.7, 1_318.5, 1_567.98, 2_093.0,
    ];
    let mut at = 0.15_f32;
    for (index, note) in line.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "ten bells")]
        let gain = 0.12 + index as f32 * 0.01;
        mix_into(&mut out, &bell(note, 2.4, 1.2, 0.9), at, gain);
        at += 0.27 - (index as f32).min(8.0) * 0.012;
    }
    // The dock wakes: a last high chord of bells, held.
    for note in [1_046.5_f32, 1_318.5, 1_567.98, 2_093.0] {
        mix_into(&mut out, &bell(note, 3.0, 1.0, 1.3), 2.3, 0.1);
    }
    out
}
