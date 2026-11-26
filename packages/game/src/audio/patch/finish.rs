//! Part III's last sounds (Wave 14 finish): the Wave 12 worlds' theme bosses' music stings
//! (a lair's boss wakes, falls) and the set pieces' own music (the Works, the Halcyon Gate,
//! the Harbour). Every name carries the track's `f14_` prefix.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop;
//! nothing pulses fast (the effects' photosensitivity limits hold for sound-driven visuals).

#![allow(
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "presentation code: sound math reads top to bottom"
)]

use super::{
    FilterMode, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, mix_into, render, segments, thump,
};

// ---------------------------------------------------------------------------------------------
// The lair bosses' stings.
// ---------------------------------------------------------------------------------------------

/// A brass-like stab: detuned saws through a swelling low-pass.
fn brass(rng: &mut Rng, note: f32, seconds: f32, swell: f32) -> Vec<f32> {
    let mut a = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut b = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 600.0, 1.1);
    render(seconds, |t| {
        let env = segments(
            t,
            &[
                (0.0, 0.0),
                (0.06, 1.0),
                (seconds * 0.6, 0.8),
                (seconds, 0.0),
            ],
        );
        let cutoff = 500.0 + swell * 2_200.0 * env;
        filter.run((a.next(note) + b.next(note * 1.006)) * 0.5, cutoff, 1.1) * env
    })
}

/// A lair's theme boss wakes: a minor brass call over a timpani roll, a low gong under it (the
/// music's sting before the boss's own roar and the fight's loop).
pub(super) fn lair_wake(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(2.8, |_| 0.0);
    // Timpani: a soft roll swelling into the call.
    for index in 0..10 {
        let hit = thump(98.0, 82.0, 0.35, 1.4);
        let gain = 0.12 + index as f32 * 0.03;
        mix_into(&mut out, &hit, index as f32 * 0.07, gain);
    }
    // The call: D minor, a rising fourth then the minor third (D, G, F).
    for (index, (at, note, len)) in [
        (0.7_f32, 146.75_f32, 0.45_f32),
        (1.1, 196.0, 0.45),
        (1.5, 174.5, 1.1),
    ]
    .into_iter()
    .enumerate()
    {
        let stab = brass(rng, note, len, 0.6 + index as f32 * 0.2);
        mix_into(&mut out, &stab, at, 0.42);
        let octave = brass(rng, note * 2.0, len, 0.4);
        mix_into(&mut out, &octave, at, 0.16);
    }
    let gong = bell(73.5, 1.41, 3.0, 2.0);
    mix_into(&mut out, &gong, 1.5, 0.45);
    out
}

/// A lair's theme boss falls: the call answered in the major, a bright chord ringing out as
/// the vault opens.
pub(super) fn lair_fall(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(3.2, |_| 0.0);
    let boom = thump(80.0, 40.0, 0.8, 2.0);
    mix_into(&mut out, &boom, 0.0, 0.5);
    // D, F sharp, A, then the high D held over the chord.
    for (index, note) in [293.75_f32, 369.0, 440.0].into_iter().enumerate() {
        let stab = brass(rng, note, 0.35, 0.7);
        mix_into(&mut out, &stab, 0.25 + index as f32 * 0.18, 0.32);
    }
    for note in [293.75_f32, 369.0, 440.0, 587.25] {
        let chord = brass(rng, note, 1.8, 0.8);
        mix_into(&mut out, &chord, 0.85, 0.2);
        let ring = bell(note * 2.0, 2.0, 0.7, 2.0);
        mix_into(&mut out, &ring, 0.85, 0.12);
    }
    let mut dust = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut shimmer = Svf::new(FilterMode::HighPass, 4_000.0, 0.7);
    let air = render(2.2, |t| {
        shimmer.process(dust.next()) * segments(t, &[(0.0, 0.0), (0.5, 1.0), (2.2, 0.0)]) * 0.05
    });
    mix_into(&mut out, &air, 0.85, 1.0);
    out
}
