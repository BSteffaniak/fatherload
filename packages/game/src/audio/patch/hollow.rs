//! Descent V, the Hollow World's sounds (Wave 9): a music bed for each of its four strata, the
//! Sun Serpent's theme, three ambience beds, and the one-shots of its set pieces (the
//! breakthrough's sting, gusts, beasts, the serpent, the sun's flare and the Gravity Anchor).
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop; scattered
//! events spread evenly over the render.

#![allow(
    clippy::suboptimal_flops,
    clippy::while_float,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::bool_to_int_with_if,
    clippy::manual_range_contains,
    reason = "presentation code: art and sound math, layout passes read top to bottom"
)]

use super::{
    FilterMode, Fm, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, brass, burst,
    loop_events, loop_length, loop_note, mix_into, perc, render, saturate, scatter, scatter_grains,
    segments, sine, thump,
};

/// Inner Sky music: a wide horn theme in A over open fifths and an airy pad, a glassy bell
/// answering each phrase: warm, spacious, the light of a sun underground.
pub(super) fn inner_sky_loop(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Triangle);
    let mut fifth = Osc::new(Wave::Triangle);
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut air_filter = Svf::new(FilterMode::BandPass, 1_800.0, 0.6);
    let mut out = render(loop_length(), |t| {
        let swell = 0.75 + 0.25 * sine(t, 0.25);
        (low.next(110.0) * 0.4 + fifth.next(165.0) * 0.3) * swell * 0.3
            + air_filter.process(air.next()) * 0.035 * (0.6 + 0.4 * sine(t, 0.5))
    });
    let step = LOOP_SECONDS / 4.0;
    for (index, note) in [440.0_f32, 659.25, 554.25, 493.75].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "four steps")]
        let start = index as f32 * step;
        let horn = brass(rng, note * 0.5, step * 0.85, 0.25);
        loop_note(&mut out, &horn, start, 0.13);
        let chime = bell(note * 2.0, 3.0, 0.9, 1.4);
        loop_note(&mut out, &chime, start + step * 0.5, 0.05);
    }
    out
}

/// Megafauna Jungle music: hand drums on a lilting six, a marimba figure in D pentatonic and a
/// far bellow of something enormous once a loop.
pub(super) fn jungle_loop(rng: &mut Rng) -> Vec<f32> {
    let mut insects = Noise::new(NoiseColor::White, rng.next_u64());
    let mut insect_filter = Svf::new(FilterMode::BandPass, 5_200.0, 3.0);
    let mut out = render(loop_length(), |t| {
        insect_filter.process(insects.next()) * 0.02 * (0.6 + 0.4 * sine(t, 2.0))
    });
    let step = LOOP_SECONDS / 12.0;
    let pattern = [
        1.0_f32, 0.0, 0.5, 0.8, 0.0, 0.5, 1.0, 0.0, 0.5, 0.8, 0.4, 0.5,
    ];
    for (index, hit) in pattern.into_iter().enumerate() {
        if hit <= 0.0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss, reason = "twelve steps")]
        let start = index as f32 * step;
        let pitch = if index % 3 == 0 { 140.0 } else { 210.0 };
        let drum = thump(pitch, pitch * 0.6, 0.18, 1.4);
        loop_note(&mut out, &drum, start, 0.22 * hit);
    }
    let figure = [293.75_f32, 349.25, 440.0, 392.0, 523.25, 440.0];
    for (index, note) in figure.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "six notes")]
        let start = index as f32 * step * 2.0 + step;
        let marimba = bell(note * rng.jitter(0.002), 4.0, 0.6, 0.4);
        loop_note(&mut out, &marimba, start, 0.12);
    }
    let mut bellow = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut bellow_filter = Svf::new(FilterMode::LowPass, 300.0, 1.4);
    let call = render(1.6, |t| {
        let pitch = 70.0 + 12.0 * segments(t, &[(0.0, 0.0), (0.4, 1.0), (1.6, 0.2)]);
        bellow_filter.run(bellow.next(pitch), 260.0 + 200.0 * (t * 2.0).min(1.0), 1.4)
            * segments(t, &[(0.0, 0.0), (0.3, 1.0), (1.2, 0.6), (1.6, 0.0)])
    });
    loop_note(&mut out, &call, 1.5, 0.12);
    out
}

/// Upside-Down Isles music: a dreamy arpeggio that rises where it should fall, every note
/// swelling in instead of striking (like a bell played backwards), over a floating pad.
pub(super) fn isles_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pad = Osc::new(Wave::Triangle);
    let mut pad_high = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        (pad.next(130.75) * 0.3 + pad_high.next(196.0) * 0.2) * (0.7 + 0.3 * sine(t, 0.25)) * 0.3
    });
    let step = LOOP_SECONDS / 8.0;
    let line = [
        261.75_f32, 329.5, 392.0, 523.25, 392.0, 329.5, 293.75, 246.75,
    ];
    for (index, note) in line.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "eight steps")]
        let start = index as f32 * step;
        let mut fm = Fm::default();
        let length = step * 0.95;
        let swell = render(length, |t| {
            // Backwards: the envelope rises to the end and cuts.
            let rise = (t / length).powi(2);
            fm.next(note * 2.0, 2.0, 0.8 * rise)
                * rise
                * segments(t, &[(length - 0.02, 1.0), (length, 0.0)])
        });
        loop_note(&mut out, &swell, start, 0.12);
    }
    let _ = rng.next_u64();
    out
}

/// The Sunforge's music: a shimmering, blazing drone of detuned saws in E, the heat's
/// shimmer on top, and a slow brass swell that never quite resolves.
pub(super) fn sunforge_loop(rng: &mut Rng) -> Vec<f32> {
    let mut a = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut b = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut c = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 900.0, 1.1);
    let mut shimmer = Fm::default();
    let mut out = render(loop_length(), |t| {
        let body = a.next(82.5) + b.next(82.75) + c.next(123.75) * 0.6;
        let open = 700.0 + 500.0 * (0.5 + 0.5 * sine(t, 0.25));
        filter.run(body * 0.22, open, 1.1)
            + shimmer.next(1_320.0, 1.5, 0.6) * 0.025 * (0.5 + 0.5 * sine(t, 0.5))
    });
    let swell = brass(rng, 329.5, LOOP_SECONDS * 0.6, 0.3);
    loop_note(&mut out, &swell, 0.4, 0.12);
    let answer = brass(rng, 311.0, LOOP_SECONDS * 0.25, 0.25);
    loop_note(&mut out, &answer, LOOP_SECONDS * 0.7, 0.1);
    out
}

/// The Sun Serpent's theme: a driving low pulse in E phrygian, a brass riff that coils round
/// the root and a gong once a loop.
pub(super) fn serpent_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut drone_filter = Svf::new(FilterMode::LowPass, 400.0, 1.3);
    let mut out = render(loop_length(), |t| {
        drone_filter.run(drone.next(41.25), 300.0 + 150.0 * sine(t, 0.5), 1.3) * 0.2
    });
    let step = LOOP_SECONDS / 16.0;
    for index in 0..16_usize {
        #[allow(clippy::cast_precision_loss, reason = "sixteen steps")]
        let start = index as f32 * step;
        let accent = if index % 4 == 0 { 1.0 } else { 0.55 };
        let pulse = thump(110.0, 55.0, 0.14, 1.8);
        loop_note(&mut out, &pulse, start, 0.18 * accent);
    }
    let riff = [
        164.75_f32, 174.5, 164.75, 196.0, 174.5, 164.75, 146.75, 164.75,
    ];
    for (index, note) in riff.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "eight steps")]
        let start = index as f32 * step * 2.0;
        let horn = brass(rng, note, step * 1.6, 0.45);
        loop_note(&mut out, &horn, start, 0.12);
    }
    let gong = bell(98.0, 1.41, 2.4, 3.0);
    loop_note(&mut out, &gong, 0.0, 0.14);
    out
}

/// The jungle's ambience: insects shimmering, birds calling here and there (spread evenly so
/// nothing pulses at the loop rate) and leaves stirring.
pub(super) fn jungle_ambience_loop(rng: &mut Rng) -> Vec<f32> {
    let mut leaves = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut leaf_filter = Svf::new(FilterMode::BandPass, 2_400.0, 0.5);
    let mut cicadas = Noise::new(NoiseColor::White, rng.next_u64());
    let mut cicada_filter = Svf::new(FilterMode::BandPass, 6_400.0, 4.0);
    let mut out = render(loop_length(), |t| {
        leaf_filter.process(leaves.next()) * 0.05
            + cicada_filter.process(cicadas.next()) * 0.03 * (0.7 + 0.3 * sine(t, 1.5))
    });
    for time in scatter(rng, loop_events(1.0), 0.0, loop_length() - 0.4, 1.0) {
        let pitch = rng.range(1_800.0, 3_200.0);
        let mut chirp = Osc::new(Wave::Sine);
        let call = render(0.35, |t| {
            chirp.next(pitch * (1.0 + 0.25 * sine(t, 9.0)))
                * perc(t, 0.01, 0.12)
                * (if t < 0.15 || t > 0.2 { 1.0 } else { 0.0 })
        });
        mix_into(&mut out, &call, time, 0.08);
    }
    out
}

/// The isles' ambience: air whistling the wrong way past hanging stone, a slow rising sweep.
pub(super) fn updraft_loop(rng: &mut Rng) -> Vec<f32> {
    let mut wind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 600.0, 1.2);
    let mut whistle = Osc::new(Wave::Sine);
    render(loop_length(), |t| {
        let sweep = 0.5 + 0.5 * sine(t, 0.25);
        band.run(wind.next(), 400.0 + 900.0 * sweep, 1.2) * 0.25
            + whistle.next(880.0 + 220.0 * sweep) * 0.01
    })
}

/// The Sunforge's ambience: the inner sun's roar far below and stone ticking as it heats.
pub(super) fn forge_ambience_loop(rng: &mut Rng) -> Vec<f32> {
    let mut roar = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut roar_filter = Svf::new(FilterMode::LowPass, 220.0, 0.9);
    let mut out = render(loop_length(), |t| {
        roar_filter.process(roar.next()) * 0.5 * (0.8 + 0.2 * sine(t, 0.5))
    });
    scatter_grains(
        rng,
        &mut out,
        loop_events(4.0),
        (0.0, loop_length() - 0.05),
        (2_500.0, 5_000.0),
        0.12,
        false,
    );
    out
}

/// The breakthrough: the rock sky gives and the light floods in. A deep swell rising into a
/// bright major chord with a choir-like shimmer, held, then let go.
pub(super) fn sun_sting(rng: &mut Rng) -> Vec<f32> {
    let seconds = 4.2;
    let chord = [110.0_f32, 164.75, 220.0, 277.25, 330.0, 440.0];
    let mut voices: Vec<(Osc, Osc)> = chord
        .iter()
        .map(|_| {
            (
                Osc::new(Wave::Saw).with_phase(rng.unit()),
                Osc::new(Wave::Saw).with_phase(rng.unit()),
            )
        })
        .collect();
    let mut filter = Svf::new(FilterMode::LowPass, 400.0, 0.9);
    let mut shimmer = Fm::default();
    render(seconds, |t| {
        let swell = segments(t, &[(0.0, 0.0), (1.4, 1.0), (3.0, 0.85), (seconds, 0.0)]);
        let body: f32 = voices
            .iter_mut()
            .zip(chord)
            .map(|((a, b), note)| a.next(note) + b.next(note * 1.006))
            .sum::<f32>()
            / 12.0;
        let open = 3_200.0f32.mul_add(swell, 300.0);
        filter.run(body, open, 0.9) * swell + shimmer.next(1_760.0, 2.0, 1.2) * 0.06 * swell
    })
}

/// A gust bladder bursting: a wet pop and a rushing whoosh of air.
pub(super) fn wind_gust(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(260.0, 120.0, 0.08, 1.6);
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 900.0, 0.8);
    let rush = render(0.9, |t| {
        band.run(air.next(), 600.0 + 1_800.0 * (1.0 - t / 0.9), 0.8) * perc(t, 0.02, 0.6)
    });
    mix_into(&mut out, &rush, 0.02, 0.8);
    out
}

/// A great beast's bellow: a low saw call through an opening filter, with a growl.
pub(super) fn beast_bellow(rng: &mut Rng) -> Vec<f32> {
    let base = rng.range(62.0, 78.0);
    let mut voice = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 300.0, 1.5);
    let mut growl = Noise::new(NoiseColor::Brown, rng.next_u64());
    render(1.5, |t| {
        let pitch = base * (1.0 + 0.2 * segments(t, &[(0.0, 0.0), (0.35, 1.0), (1.5, 0.3)]));
        let open = 250.0 + 500.0 * segments(t, &[(0.0, 0.0), (0.3, 1.0), (1.5, 0.2)]);
        let level = segments(t, &[(0.0, 0.0), (0.2, 1.0), (1.1, 0.7), (1.5, 0.0)]);
        saturate(
            filter.run(voice.next(pitch) + growl.next() * 0.3, open, 1.5) * level,
            1.4,
        )
    })
}

/// A roc's cry: a piercing falling screech with a rasp.
pub(super) fn roc_cry(rng: &mut Rng) -> Vec<f32> {
    let start = rng.range(2_200.0, 2_600.0);
    let mut voice = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::BandPass, 2_000.0, 2.0);
    render(0.7, |t| {
        let pitch = start * (1.0 - 0.35 * (t / 0.7)) * (1.0 + 0.03 * sine(t, 38.0));
        filter.run(voice.next(pitch), pitch, 2.0)
            * segments(t, &[(0.0, 0.0), (0.05, 1.0), (0.5, 0.6), (0.7, 0.0)])
    })
}

/// The Sun Serpent's roar: a huge hiss over a low growl and a metallic ring of its scales.
pub(super) fn serpent_roar(rng: &mut Rng) -> Vec<f32> {
    let mut hiss = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hiss_filter = Svf::new(FilterMode::HighPass, 3_000.0, 0.7);
    let mut growl = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut growl_filter = Svf::new(FilterMode::LowPass, 400.0, 1.4);
    let mut out = render(1.8, |t| {
        let level = segments(t, &[(0.0, 0.0), (0.15, 1.0), (1.2, 0.7), (1.8, 0.0)]);
        (hiss_filter.process(hiss.next()) * 0.4
            + growl_filter.run(growl.next(55.0 + 15.0 * sine(t, 3.0)), 500.0, 1.4))
            * level
    });
    let ring = bell(660.0, 2.76, 1.5, 1.4);
    mix_into(&mut out, &ring, 0.1, 0.3);
    out
}

/// The inner sun about to flare: a slow rising swell of heat (it warns; nothing pulses fast).
pub(super) fn flare_swell(rng: &mut Rng) -> Vec<f32> {
    let seconds = 2.6;
    let mut a = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut b = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 300.0, 1.2);
    render(seconds, |t| {
        let rise = (t / seconds).powi(2);
        let pitch = 110.0 + 110.0 * rise;
        filter.run(
            (a.next(pitch) + b.next(pitch * 1.5)) * 0.4,
            300.0 + 3_000.0 * rise,
            1.2,
        ) * segments(t, &[(0.0, 0.0), (seconds - 0.1, 1.0), (seconds, 0.0)])
    })
}

/// The Gravity Anchor taking hold: a deep magnetic thunk and a hum settling.
pub(super) fn anchor_thunk(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(90.0, 45.0, 0.4, 2.0);
    let mut hum = Osc::new(Wave::Sine);
    let mut buzz = Osc::new(Wave::Square);
    let mut filter = Svf::new(FilterMode::LowPass, 500.0, 1.0);
    let tail = render(0.9, |t| {
        (hum.next(120.0) * 0.6 + filter.run(buzz.next(60.0), 400.0, 1.0) * 0.2) * perc(t, 0.02, 0.6)
    });
    mix_into(&mut out, &tail, 0.03, 0.6);
    let click = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_500.0,
        0.7,
        0.02,
    );
    mix_into(&mut out, &click, 0.0, 0.4);
    out
}
