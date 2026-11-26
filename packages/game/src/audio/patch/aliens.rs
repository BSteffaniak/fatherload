//! The friendly aliens' sounds (Wave 12 aliens track): each species' voice and the moments of
//! first contact and of a tongue opening up.
//!
//! * Vessi: low hoots that bend up into a soft two-note chord, breathy, with a little vibrato.
//! * Tokkari: quick clicks and clacks (filtered noise ticks on a wooden body) in a rattling run.
//! * Siphra: glassy chimes, inharmonic FM bells ringing and beating against each other.
//! * First contact: a slow, shimmering three-part motif (a hoot, a clack, a chime) over a pad.
//! * A word learned: a small rising pair of bells.

#![allow(clippy::similar_names, reason = "oscillators named for their voices")]

use super::{
    FilterMode, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, decay, mix_into, perc, render,
    samples_for, segments,
};

/// A Vessi speaking: two hooted notes bending up into a chord a fifth apart.
pub(super) fn vessi_voice(rng: &mut Rng) -> Vec<f32> {
    let root = rng.range(120.0, 150.0);
    let seconds = 1.1;
    let mut low = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut high = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut breath = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, root * 3.0, 2.0);
    render(seconds, |t| {
        let bend = 1.0 + 0.06 * (t / 0.25).min(1.0);
        let vibrato = 1.0 + 0.012 * (t * 31.0).sin();
        let first = low.next(root * bend * vibrato)
            * segments(t, &[(0.0, 0.0), (0.06, 1.0), (0.5, 0.7), (0.62, 0.0)]);
        let chord = high.next(root * 1.5 * bend * vibrato)
            * segments(t, &[(0.0, 0.0), (0.45, 0.0), (0.55, 0.8), (seconds, 0.0)]);
        let air =
            band.process(breath.next()) * segments(t, &[(0.0, 0.0), (0.05, 0.3), (seconds, 0.0)]);
        (first * 0.5 + chord * 0.4 + air * 0.25) * 0.6
    })
}

/// A Tokkari speaking: a rattling run of clicks and clacks.
pub(super) fn tokkari_voice(rng: &mut Rng) -> Vec<f32> {
    let seconds = 0.8;
    let mut out = vec![0.0; samples_for(seconds)];
    let clicks = 7 + (rng.next_u64() % 4) as usize;
    let mut at = 0.0_f32;
    for index in 0..clicks {
        let pitch = rng.range(700.0, 1_500.0) * if index % 3 == 2 { 0.6 } else { 1.0 };
        let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
        let mut body = Svf::new(FilterMode::BandPass, pitch, 6.0);
        let mut knock = Osc::new(Wave::Sine);
        let click = render(0.07, |t| {
            (body.process(noise.next()) * 0.9 + knock.next(pitch * 0.35) * 0.4)
                * perc(t, 0.000_5, 0.05)
        });
        mix_into(&mut out, &click, at, 0.6);
        at += rng.range(0.045, 0.1);
        if at > seconds - 0.08 {
            break;
        }
    }
    out
}

/// A Siphra speaking: glass chimes ringing against each other.
pub(super) fn siphra_voice(rng: &mut Rng) -> Vec<f32> {
    let seconds = 1.4;
    let mut out = vec![0.0; samples_for(seconds)];
    let base = rng.range(880.0, 1_100.0);
    for (index, ratio) in [1.0_f32, 1.26, 1.5, 1.89].into_iter().enumerate() {
        let start = index as f32 * rng.range(0.08, 0.16);
        mix_into(&mut out, &bell(base * ratio, 3.51, 1.1, 1.0), start, 0.18);
    }
    let mut shimmer = Osc::new(Wave::Sine).with_phase(rng.unit());
    let pad = render(seconds, |t| {
        shimmer.next(base * 2.0 + (t * 6.0).sin() * 8.0) * decay(t, 0.6) * 0.06
    });
    mix_into(&mut out, &pad, 0.0, 1.0);
    out
}

/// First contact: a hoot, a clack and a chime answering each other over a slow pad.
pub(super) fn first_contact(rng: &mut Rng) -> Vec<f32> {
    let seconds = 3.2;
    let mut out = vec![0.0; samples_for(seconds)];
    let mut pad = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut fifth = Osc::new(Wave::Sine).with_phase(rng.unit());
    let body = render(seconds, |t| {
        let swell = segments(t, &[(0.0, 0.0), (1.0, 1.0), (seconds, 0.0)]);
        (pad.next(110.0) + fifth.next(165.0) * 0.6 + fifth.next(220.0) * 0.0) * swell * 0.12
    });
    mix_into(&mut out, &body, 0.0, 1.0);
    let mut voices = Rng::new(rng.next_u64());
    mix_into(&mut out, &vessi_voice(&mut voices), 0.1, 0.8);
    mix_into(&mut out, &tokkari_voice(&mut voices), 1.0, 0.7);
    mix_into(&mut out, &siphra_voice(&mut voices), 1.7, 0.8);
    out
}

/// A word learned: two small bells, the second higher.
pub(super) fn word_learned(rng: &mut Rng) -> Vec<f32> {
    let root = rng.range(1_040.0, 1_060.0);
    let mut out = vec![0.0; samples_for(0.9)];
    mix_into(&mut out, &bell(root, 2.0, 0.8, 0.5), 0.0, 0.25);
    mix_into(&mut out, &bell(root * 1.335, 2.0, 0.8, 0.6), 0.12, 0.25);
    out
}
