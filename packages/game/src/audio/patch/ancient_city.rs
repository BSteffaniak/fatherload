//! The builders' buried city's sounds (Wave 10 ancients-a, Descent VII strata 1-3): a music
//! bed per stratum (the Megacity's lonely civic anthem, the Engine Halls' machine groove that
//! the director runs at the Great Engine's beat, the Archive's starlit chimes), an ambience bed
//! each, and the one-shots of the machinery, the archivist and the vaults. Patch names carry
//! the `city_` prefix.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop; the
//! engine groove has four strokes a loop so its rate can follow the beat.

use super::{
    FilterMode, Fm, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, grains,
    loop_length, loop_note, mix_into, perc, render, saturate, scatter_grains, segments, sine,
    thump,
};

/// A tram's bell: two bright dings, the second a third higher.
pub(super) fn tram_bell(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(1.2, |_| 0.0);
    let base = 1_046.5 * rng.jitter(0.004);
    mix_into(&mut out, &bell(base, 3.0, 1.2, 0.8), 0.0, 0.45);
    mix_into(&mut out, &bell(base * 1.26, 3.0, 1.2, 0.8), 0.18, 0.4);
    out
}

/// A lift's chime: a soft two-note fall, warm brass.
pub(super) fn lift_chime(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(1.4, |_| 0.0);
    let base = 783.99 * rng.jitter(0.003);
    mix_into(&mut out, &bell(base, 2.0, 0.6, 1.0), 0.0, 0.35);
    mix_into(&mut out, &bell(base * 0.75, 2.0, 0.6, 1.1), 0.22, 0.35);
    out
}

/// A piston's hiss: steam venting, bright then falling away.
pub(super) fn piston_hiss(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 1_800.0, 0.9);
    render(0.7, |t| {
        let shape = segments(t, &[(0.0, 0.0), (0.04, 1.0), (0.4, 0.5), (0.7, 0.0)]);
        band.run(noise.next(), 1_400.0 + 1_600.0 * shape, 0.9) * shape * 0.55
    })
}

/// A rig thrown off a rising piston: a deep thump and a ringing clank.
pub(super) fn piston_thump(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(110.0, 40.0, 0.35, 1.6);
    mix_into(
        &mut out,
        &bell(rng.range(310.0, 340.0), 3.7, 1.8, 0.5),
        0.01,
        0.25,
    );
    out
}

/// A crusher's slam: a huge metal blow, grit shaking loose.
pub(super) fn crusher_slam(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(90.0, 30.0, 0.5, 2.2);
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 900.0, 1.2);
    let crash = render(0.5, |t| {
        band.process(noise.next()) * perc(t, 0.002, 0.35) * 0.5
    });
    mix_into(&mut out, &crash, 0.0, 0.8);
    mix_into(
        &mut out,
        &bell(rng.range(180.0, 200.0), 2.9, 2.0, 0.6),
        0.0,
        0.3,
    );
    grains(rng, &mut out, 10, (0.05, 0.5), (2_000.0, 5_000.0), 0.06);
    out
}

/// The archivist speaking: three quick friendly blips, rising.
pub(super) fn archivist_chirp(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(0.5, |_| 0.0);
    let base = rng.range(880.0, 960.0);
    for (index, step) in [1.0_f32, 1.12, 1.33].into_iter().enumerate() {
        let mut fm = Fm::default();
        let blip = render(0.09, |t| {
            fm.next(base * step, 2.0, 0.8) * perc(t, 0.003, 0.08) * 0.4
        });
        #[allow(clippy::cast_precision_loss, reason = "three blips")]
        mix_into(&mut out, &blip, index as f32 * 0.1, 1.0);
    }
    out
}

/// The archivist's scan: a sweep up and a soft ping where it found ore.
pub(super) fn archivist_scan(rng: &mut Rng) -> Vec<f32> {
    let mut tone = Osc::new(Wave::Sine);
    let start = rng.range(500.0, 560.0);
    let mut out = render(0.5, |t| {
        let pitch = start * (1.0 + 2.0 * (t / 0.5));
        tone.next(pitch) * perc(t, 0.02, 0.4) * 0.25
    });
    mix_into(&mut out, &bell(1_760.0, 2.0, 0.4, 0.6), 0.35, 0.25);
    out
}

/// A star chart read: a shimmering major chord rolling up, sparkles over it.
pub(super) fn chart_chord(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(1.9, |_| 0.0);
    for (index, note) in [523.25_f32, 659.26, 783.99, 1_046.5, 1_318.5]
        .into_iter()
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "five notes")]
        let start = index as f32 * 0.09;
        mix_into(
            &mut out,
            &bell(note * rng.jitter(0.002), 3.0, 0.7, 1.4),
            start,
            0.22,
        );
    }
    grains(rng, &mut out, 14, (0.2, 1.3), (6_000.0, 10_000.0), 0.05);
    out
}

/// A vault's lock giving: a stone rumble, a heavy clunk and a bright chime.
pub(super) fn vault_give(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 200.0, 0.8);
    let mut out = render(1.6, |t| {
        low.process(noise.next())
            * segments(t, &[(0.0, 0.0), (0.2, 0.9), (0.9, 0.5), (1.3, 0.0)])
            * 0.5
    });
    mix_into(&mut out, &thump(140.0, 50.0, 0.25, 1.4), 0.9, 0.6);
    mix_into(&mut out, &bell(1_174.66, 2.0, 0.6, 1.0), 1.0, 0.25);
    out
}

/// The city's alarm: a soft two-tone call, twice (never harsh, never a siren).
pub(super) fn city_alarm(rng: &mut Rng) -> Vec<f32> {
    let mut tone = Osc::new(Wave::Triangle);
    let high = rng.range(650.0, 670.0);
    render(1.3, |t| {
        let note = if (t * 2.5).fract() < 0.5 {
            high
        } else {
            high * 0.84
        };
        tone.next(note) * segments(t, &[(0.0, 0.0), (0.05, 1.0), (1.1, 0.8), (1.3, 0.0)]) * 0.22
    })
}

/// A scrap crawler snatching ore: a metal scrape and a squeak.
pub(super) fn cargo_snatch(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 2_200.0, 3.0);
    let mut squeak = Osc::new(Wave::Saw);
    render(0.5, |t| {
        let scrape =
            band.run(noise.next(), 1_800.0 + 1_200.0 * (t / 0.5), 3.0) * perc(t, 0.01, 0.3);
        let pitch = 1_400.0 + 900.0 * sine(t, 9.0).abs();
        let cry =
            squeak.next(pitch) * segments(t, &[(0.0, 0.0), (0.25, 0.0), (0.3, 0.3), (0.5, 0.0)]);
        saturate(scrape * 0.5 + cry * 0.2, 1.3)
    })
}

/// A spark wisp's jolt: a crackle and an electric zap.
pub(super) fn spark_zap(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut fm = Fm::default();
    let base = rng.range(1_200.0, 1_400.0);
    render(0.4, |t| {
        let gate = if (t * 60.0).fract() < 0.4 { 1.0 } else { 0.0 };
        let crackle = noise.next() * gate * perc(t, 0.001, 0.25) * 0.35;
        let zap = fm.next(base * (1.0 - 0.6 * (t / 0.4)), 3.0, 2.5) * perc(t, 0.002, 0.2) * 0.3;
        crackle + zap
    })
}

/// Buried Megacity music: a lonely civic anthem, slow electric piano chords (D, B minor, G, A)
/// on the beat, a soft pad under them and a far tram bell.
pub(super) fn megacity_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pad = Osc::new(Wave::Triangle);
    let mut pad_two = Osc::new(Wave::Triangle);
    let mut out = render(loop_length(), |t| {
        let swell = 0.7 + 0.3 * sine(t, 0.25);
        (pad.next(146.75) * 0.35 + pad_two.next(220.0) * 0.25) * swell * 0.26
    });
    let chords: [[f32; 3]; 4] = [
        [293.75, 370.0, 440.0],
        [246.75, 293.75, 370.0],
        [196.0, 246.75, 293.75],
        [220.0, 277.25, 329.75],
    ];
    for (index, chord) in chords.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "four chords")]
        let start = index as f32 * LOOP_SECONDS / 4.0;
        for note in chord {
            let key = bell(note * rng.jitter(0.001), 1.0, 0.8, 1.4);
            loop_note(&mut out, &key, start, 0.12);
        }
    }
    loop_note(&mut out, &bell(1_318.5, 3.0, 1.0, 0.8), 2.5, 0.05);
    out
}

/// Engine Halls music: a driving machine groove, four strokes a loop (a thump on each, a
/// metal clank between), a low filtered bass and a hiss on the last stroke.
pub(super) fn engine_loop(rng: &mut Rng) -> Vec<f32> {
    let mut bass = Osc::new(Wave::Saw);
    let mut bass_filter = Svf::new(FilterMode::LowPass, 300.0, 1.2);
    let mut out = render(loop_length(), |t| {
        let gate = if (t * 2.0).fract() < 0.55 { 1.0 } else { 0.35 };
        bass_filter.run(bass.next(55.0), 220.0 + 160.0 * sine(t, 1.0).abs(), 1.2) * gate * 0.3
    });
    let stroke = LOOP_SECONDS / 4.0;
    for index in 0..4_usize {
        #[allow(clippy::cast_precision_loss, reason = "four strokes")]
        let start = index as f32 * stroke;
        loop_note(&mut out, &thump(120.0, 42.0, 0.28, 1.8), start, 0.55);
        let clank = bell(rng.range(400.0, 430.0), 3.7, 2.0, 0.25);
        loop_note(&mut out, &clank, start + stroke * 0.5, 0.18);
    }
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut band = Svf::new(FilterMode::HighPass, 4_000.0, 0.7);
    let hiss = render(0.5, |t| {
        band.process(noise.next()) * perc(t, 0.05, 0.4) * 0.3
    });
    loop_note(&mut out, &hiss, stroke * 3.0 + 0.2, 0.5);
    out
}

/// The Archive of Stars' music: slow starlit chimes on a pentatonic scale over a soft choir
/// pad with a gentle vibrato.
pub(super) fn archive_loop(rng: &mut Rng) -> Vec<f32> {
    let mut voices = [
        Osc::new(Wave::Sine),
        Osc::new(Wave::Sine),
        Osc::new(Wave::Sine),
    ];
    let mut out = render(loop_length(), |t| {
        let vibrato = 1.0 + 0.004 * sine(t, 5.0);
        let swell = 0.6 + 0.4 * sine(t, 0.25);
        let [a, b, c] = &mut voices;
        (a.next(261.75 * vibrato) * 0.3 + b.next(392.0 * vibrato) * 0.25 + c.next(523.5) * 0.12)
            * swell
            * 0.24
    });
    let scale = [1_046.5_f32, 1_174.66, 1_318.5, 1_568.0, 1_760.0];
    for index in 0..6_usize {
        #[allow(clippy::cast_precision_loss, reason = "six chimes")]
        let start = index as f32 * LOOP_SECONDS / 6.0 + 0.1;
        let note = scale[(index * 3 + 1) % scale.len()];
        loop_note(
            &mut out,
            &bell(note * rng.jitter(0.002), 3.5, 0.6, 1.6),
            start,
            0.08,
        );
    }
    out
}

/// The Megacity's ambience: a low mains hum, buzzing lamps and a far bell.
pub(super) fn city_hum_bed(rng: &mut Rng) -> Vec<f32> {
    let mut hum = Osc::new(Wave::Sine);
    let mut buzz_noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut buzz = Svf::new(FilterMode::BandPass, 120.0, 4.0);
    let mut out = render(loop_length(), |t| {
        hum.next(60.0) * 0.06 + buzz.process(buzz_noise.next()) * 0.04 * (0.7 + 0.3 * sine(t, 0.5))
    });
    loop_note(&mut out, &bell(1_046.5, 3.0, 0.8, 0.6), 1.7, 0.03);
    out
}

/// The Engine Halls' ambience: steam hissing in the walls and a deep far thud.
pub(super) fn engine_bed(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(loop_length(), |_| 0.0);
    loop_note(&mut out, &thump(70.0, 30.0, 0.5, 1.2), 0.3, 0.2);
    loop_note(&mut out, &thump(70.0, 30.0, 0.5, 1.2), 2.3, 0.14);
    scatter_grains(
        rng,
        &mut out,
        8,
        (0.0, LOOP_SECONDS),
        (3_000.0, 7_000.0),
        0.03,
        false,
    );
    out
}

/// The Archive's ambience: a hush, the orrery ticking softly, a page turning.
pub(super) fn stacks_bed(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(loop_length(), |_| 0.0);
    for index in 0..8_usize {
        #[allow(clippy::cast_precision_loss, reason = "eight ticks")]
        let start = index as f32 * LOOP_SECONDS / 8.0;
        loop_note(&mut out, &bell(2_640.0, 1.0, 0.1, 0.02), start, 0.025);
    }
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 3_000.0, 0.8);
    let page = render(0.4, |t| {
        band.process(noise.next()) * perc(t, 0.08, 0.3) * 0.2
    });
    loop_note(&mut out, &page, 1.3, 0.3);
    out
}
