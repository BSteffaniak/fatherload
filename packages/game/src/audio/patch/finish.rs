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
    FilterMode, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, burst, loop_length,
    loop_note, mix_into, perc, render, segments, sine, thump,
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

// ---------------------------------------------------------------------------------------------
// The set pieces' music.
// ---------------------------------------------------------------------------------------------

/// A soft plucked tone (a tine with a quick rise and a long ring).
fn pluck(rng: &mut Rng, note: f32, seconds: f32) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut over = Osc::new(Wave::Sine).with_phase(rng.unit());
    render(seconds, |t| {
        (osc.next(note) * 0.8 + over.next(note * 2.0) * 0.25) * perc(t, 0.004, seconds * 0.8)
    })
}

/// A breathing pad of detuned saws through a low-pass (the chord held a whole loop).
fn pad(rng: &mut Rng, notes: &[f32], cutoff: f32, level: f32) -> Vec<f32> {
    let mut voices: Vec<(Osc, Osc)> = notes
        .iter()
        .map(|_| {
            (
                Osc::new(Wave::Saw).with_phase(rng.unit()),
                Osc::new(Wave::Saw).with_phase(rng.unit()),
            )
        })
        .collect();
    let mut filter = Svf::new(FilterMode::LowPass, cutoff, 0.8);
    let notes = notes.to_vec();
    render(loop_length(), |t| {
        let breath = 0.75 + 0.25 * sine(t, 0.25).abs();
        let sum: f32 = voices
            .iter_mut()
            .zip(&notes)
            .map(|((a, b), note)| a.next(*note) + b.next(*note * 1.005))
            .sum::<f32>()
            / (notes.len() as f32 * 2.0);
        filter.process(sum) * level * breath
    })
}

/// The Works (the hub's late-goals page): a workshop's music in C major. A steady plucked
/// arpeggio over a warm pad, the bay's anvil ringing on the beat, a welder's hiss now and then
/// and the cranes' low drone: building something great, unhurried.
pub(super) fn works_music_loop(rng: &mut Rng) -> Vec<f32> {
    // C, then F over C, a bar each, twice.
    let mut out = pad(rng, &[130.75, 196.0, 261.75, 329.75], 900.0, 0.05);
    let fourth = pad(rng, &[130.75, 174.5, 261.75, 349.25], 900.0, 0.03);
    mix_into(&mut out, &fourth, 0.0, 0.5);
    let step = LOOP_SECONDS / 16.0;
    let arpeggio = [
        523.25_f32, 659.25, 784.0, 659.25, 523.25, 784.0, 1_046.5, 784.0, 698.5, 880.0, 1_046.5,
        880.0, 698.5, 523.25, 659.25, 784.0,
    ];
    for (index, note) in arpeggio.into_iter().enumerate() {
        let note = note * rng.jitter(0.001);
        loop_note(&mut out, &pluck(rng, note, 0.6), index as f32 * step, 0.06);
    }
    // The anvil on beats one and three: a ringing bell over a soft thump.
    for beat in [0.0_f32, 2.0] {
        let ring = bell(1_318.5, 2.76, 1.2, 0.9);
        loop_note(&mut out, &ring, beat, 0.07);
        let knock = thump(150.0, 90.0, 0.12, 1.2);
        loop_note(&mut out, &knock, beat, 0.12);
    }
    // A welder's hiss on the last beat.
    let hiss = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        4_500.0,
        0.7,
        0.5,
    );
    loop_note(&mut out, &hiss, 3.2, 0.025);
    // The cranes' drone.
    let mut drone = Osc::new(Wave::Sine).with_phase(rng.unit());
    let low = render(loop_length(), |t| {
        drone.next(65.5) * (0.04 + 0.01 * sine(t, 0.5))
    });
    mix_into(&mut out, &low, 0.0, 1.0);
    out
}

/// The Halcyon Gate (building it at Halcyon Dock, riding it): a grand slow choir of a pad in E
/// major, a great bell tolling once a loop, a glassy shimmer turning over it: something very
/// old and very large waking up.
pub(super) fn gate_music_loop(rng: &mut Rng) -> Vec<f32> {
    let mut out = pad(rng, &[82.5, 164.75, 247.0, 311.25, 370.0], 1_300.0, 0.07);
    let lift = pad(rng, &[82.5, 185.0, 277.25, 370.0, 440.0], 1_600.0, 0.04);
    mix_into(&mut out, &lift, 0.0, 0.6);
    // The toll: a deep bell and its octave.
    let toll = bell(164.75, 1.41, 2.0, 3.6);
    loop_note(&mut out, &toll, 0.0, 0.16);
    let high = bell(659.25, 1.41, 1.2, 2.5);
    loop_note(&mut out, &high, 2.0, 0.05);
    // The shimmer: soft high plucks rising, a note a half-beat.
    let step = LOOP_SECONDS / 8.0;
    for (index, note) in [
        1_318.5_f32,
        1_480.0,
        1_661.25,
        1_975.5,
        1_661.25,
        1_480.0,
        1_318.5,
        1_244.5,
    ]
    .into_iter()
    .enumerate()
    {
        loop_note(&mut out, &pluck(rng, note, 0.9), index as f32 * step, 0.025);
    }
    out
}

/// The Harbour (the world-ships moored round their red star): a lullaby in A minor. A
/// rocking two-note bass, a slow music-box melody, the mooring bells' far chimes and the hush
/// of a sea that is only starlight.
pub(super) fn harbour_music_loop(rng: &mut Rng) -> Vec<f32> {
    let mut out = pad(rng, &[110.0, 220.0, 261.75, 329.75], 700.0, 0.045);
    let mut hush = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 500.0, 0.6);
    let sea = render(loop_length(), |t| {
        band.process(hush.next()) * 0.025 * (0.5 + 0.5 * sine(t, 0.25).abs())
    });
    mix_into(&mut out, &sea, 0.0, 1.0);
    // The rocking bass: A, E, A, E (a beat each).
    for (beat, note) in [110.0_f32, 82.5, 110.0, 82.5].into_iter().enumerate() {
        loop_note(&mut out, &pluck(rng, note, 1.2), beat as f32, 0.12);
    }
    // The music box: A minor, falling and rising again.
    let step = LOOP_SECONDS / 8.0;
    for (index, note) in [
        880.0_f32, 784.0, 659.25, 698.5, 659.25, 523.25, 587.25, 659.25,
    ]
    .into_iter()
    .enumerate()
    {
        let note = note * rng.jitter(0.001);
        let tine = bell(note, 4.0, 0.8, 0.9);
        loop_note(&mut out, &tine, index as f32 * step, 0.05);
    }
    // The mooring bells, far off.
    for at in [1.5_f32, 3.5] {
        let chime = bell(1_760.0, 2.0, 0.6, 1.4);
        loop_note(&mut out, &chime, at, 0.02);
    }
    out
}
