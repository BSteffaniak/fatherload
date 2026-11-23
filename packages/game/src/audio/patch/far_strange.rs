//! The far Strange Layers' sounds (Wave 9 strange-b, Descent VI strata 4-6 and its gate): a
//! music bed per stratum (the Time Sludge's lurching music box, the Echo Caves' answered
//! silence, the Prism Desert's split arpeggios), an ambience bed each, The Glitch's stuttering
//! theme, and the one-shots of their rules.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop; scattered
//! events spread evenly over the render.

use super::{
    FilterMode, Fm, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, grains,
    loop_length, loop_note, mix_into, perc, render, saturate, scatter_grains, segments, sine,
    thump,
};

/// A sonar ping: a clean sine chirp sweeping down, its echo answering twice, fainter, from far
/// off.
pub(super) fn sonar_ping(rng: &mut Rng) -> Vec<f32> {
    let mut tone = Osc::new(Wave::Sine);
    let start = rng.range(1_900.0, 2_100.0);
    let ping = render(0.5, |t| {
        let pitch = start * (1.0 - 0.35 * (t / 0.5).min(1.0));
        tone.next(pitch) * perc(t, 0.003, 0.32)
    });
    let mut out = render(1.6, |_| 0.0);
    mix_into(&mut out, &ping, 0.0, 0.8);
    mix_into(&mut out, &ping, 0.42, 0.28);
    mix_into(&mut out, &ping, 0.86, 0.1);
    out
}

/// A time bubble bursting: a soft glassy "whomp" that slows down, a clock tick stretching out.
pub(super) fn time_bubble(rng: &mut Rng) -> Vec<f32> {
    let mut body = Fm::default();
    let base = rng.range(220.0, 260.0);
    let mut out = render(1.4, |t| {
        let slow = 1.0 - 0.6 * (t / 1.4).min(1.0);
        body.next(base * slow, 1.5, 2.2 * perc(t, 0.01, 0.5)) * perc(t, 0.02, 1.0) * 0.55
    });
    for (index, start) in [0.05_f32, 0.3, 0.7].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "three ticks")]
        let pitch = 1_800.0 - index as f32 * 300.0;
        mix_into(&mut out, &bell(pitch, 2.0, 0.6, 0.12), start, 0.25);
    }
    out
}

/// A newt rewinding: a reversed swell (rising out of nothing) with a tape-flutter warble.
pub(super) fn rewind(rng: &mut Rng) -> Vec<f32> {
    let mut saw = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 600.0, 1.6);
    let base = rng.range(180.0, 220.0);
    render(0.7, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.6, 1.0), (0.7, 0.0)]);
        let pitch = base * (1.6 - 0.6 * t / 0.7) * (1.0 + 0.05 * sine(t, 13.0));
        filter.run(saw.next(pitch), 400.0 + 2_400.0 * swell, 1.6) * swell * 0.5
    })
}

/// A shrieker's scream: a piercing, wavering band of noise over a rising screech.
pub(super) fn shriek(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 3_000.0, 6.0);
    let mut voice = Fm::default();
    let base = rng.range(900.0, 1_100.0);
    render(0.9, |t| {
        let shape = segments(t, &[(0.0, 0.0), (0.05, 1.0), (0.6, 0.8), (0.9, 0.0)]);
        let wobble = 1.0 + 0.08 * sine(t, 23.0);
        let pitch = base * (1.0 + 0.6 * t / 0.9) * wobble;
        let screech = voice.next(pitch, 1.5, 1.8);
        let hiss = band.run(noise.next(), pitch * 3.0, 6.0);
        saturate(screech * 0.4 + hiss * 0.5, 1.6) * shape
    })
}

/// Echo quartz ringing: a clear, long glass bell with a beating partial.
pub(super) fn quartz_ring(rng: &mut Rng) -> Vec<f32> {
    let note = rng.range(1_160.0, 1_240.0);
    let mut out = bell(note, 2.76, 0.9, 2.4);
    mix_into(&mut out, &bell(note * 1.003, 4.1, 0.5, 2.0), 0.0, 0.35);
    out
}

/// A keyed wall opening to its light: a shimmering glass chord, three notes melting down.
pub(super) fn keyed_glass(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(1.2, |_| 0.0);
    for (index, note) in [1_568.0_f32, 1_318.5, 1_046.5].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "three notes")]
        let start = index as f32 * 0.06;
        mix_into(
            &mut out,
            &bell(note * rng.jitter(0.002), 3.5, 1.0, 0.9),
            start,
            0.3,
        );
    }
    grains(rng, &mut out, 6, (0.0, 0.5), (6_000.0, 9_000.0), 0.08);
    out
}

/// The Glitch rewriting a rule: a bit-crushed stutter, the same blip repeating faster and
/// higher, then a hard cut.
pub(super) fn glitch_rewrite(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(1.3, |_| 0.0);
    let mut time = 0.0_f32;
    let mut gap = 0.16_f32;
    let mut pitch = rng.range(300.0, 340.0);
    // Sixteen blips at most, each sooner than the last (they stop by 1.1 s).
    for _ in 0..16 {
        if time >= 1.1 {
            break;
        }
        let mut square = Osc::new(Wave::Square);
        let blip = render(gap * 0.7, |t| {
            let crushed = (square.next(pitch) * 4.0).round() / 4.0;
            crushed * perc(t, 0.002, gap * 0.6) * 0.5
        });
        mix_into(&mut out, &blip, time, 0.6);
        time += gap;
        gap = (gap * 0.86).max(0.04);
        pitch *= 1.07;
    }
    mix_into(&mut out, &thump(140.0, 50.0, 0.25, 1.4), 1.1, 0.8);
    out
}

/// The Glitch blinking: a quick digital "zip" across the stereo field of the mind.
pub(super) fn glitch_blink(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let start = rng.range(2_400.0, 2_800.0);
    render(0.3, |t| {
        let pitch = start * (1.0 - 0.8 * (t / 0.3));
        fm.next(pitch, 2.0, 3.0) * perc(t, 0.002, 0.25) * 0.5
    })
}

/// Time Sludge music: a music box that drags and lurches, each phrase slower than it should be
/// then hurrying to catch up, over a ticking clock and a warm low drone.
pub(super) fn time_sludge_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Triangle);
    let mut fifth = Osc::new(Wave::Triangle);
    let mut out = render(loop_length(), |t| {
        let swell = 0.75 + 0.25 * sine(t, 0.25);
        (drone.next(98.0) * 0.5 + fifth.next(146.75) * 0.3) * swell * 0.3
    });
    // The tick and the tock, unevenly spaced (time drags, then rushes).
    let ticks = [0.0_f32, 0.62, 1.05, 1.32, 2.0, 2.62, 3.05, 3.32];
    for (index, start) in ticks.into_iter().enumerate() {
        let pitch = if index % 2 == 0 { 2_200.0 } else { 1_650.0 };
        loop_note(&mut out, &bell(pitch, 1.0, 0.3, 0.05), start, 0.12);
    }
    // The music box line: a lullaby in G, notes landing late and early.
    let line: [(f32, f32); 8] = [
        (0.0, 783.99),
        (0.55, 659.26),
        (0.9, 587.33),
        (1.5, 523.25),
        (2.0, 587.33),
        (2.6, 659.26),
        (2.9, 783.99),
        (3.5, 987.77),
    ];
    for (start, note) in line {
        let pluck = bell(note * rng.jitter(0.002), 5.0, 1.4, 1.0);
        loop_note(&mut out, &pluck, start, 0.16);
    }
    out
}

/// Echo Caves music: near silence; a single soft note now and then, answered by its own echo a
/// beat later and fainter, over a breath of low wind.
pub(super) fn echo_caves_loop(rng: &mut Rng) -> Vec<f32> {
    let mut air = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut air_filter = Svf::new(FilterMode::LowPass, 180.0, 0.7);
    let mut out = render(loop_length(), |t| {
        air_filter.process(air.next()) * 0.18 * (0.6 + 0.4 * sine(t, 0.25))
    });
    for (start, note) in [(0.2_f32, 440.0_f32), (2.2, 392.0)] {
        let tone = bell(note, 2.0, 0.4, 1.6);
        loop_note(&mut out, &tone, start, 0.2);
        loop_note(&mut out, &tone, start + 0.75, 0.08);
        loop_note(&mut out, &tone, start + 1.5, 0.03);
    }
    out
}

/// Prism Desert music: a shimmering arpeggio split three ways (the same figure in three
/// detunings, a little apart), over a glassy pad.
pub(super) fn prism_desert_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pad = Osc::new(Wave::Triangle);
    let mut pad_two = Osc::new(Wave::Triangle);
    let mut out = render(loop_length(), |t| {
        let swell = 0.7 + 0.3 * sine(t, 0.5);
        (pad.next(220.0) * 0.4 + pad_two.next(329.75) * 0.3) * swell * 0.28
    });
    let step = LOOP_SECONDS / 16.0;
    let figure = [659.26_f32, 783.99, 987.77, 1_174.66];
    for index in 0..16_usize {
        let note = figure[index % figure.len()];
        #[allow(clippy::cast_precision_loss, reason = "sixteen steps")]
        let start = index as f32 * step;
        // Red, green and blue: three detunings, a hair apart in time.
        for (split, detune) in [(0.0_f32, 1.0_f32), (0.012, 1.004), (0.024, 0.996)] {
            let pluck = bell(note * detune * rng.jitter(0.001), 3.0, 0.9, 0.5);
            loop_note(&mut out, &pluck, start + split, 0.05);
        }
    }
    out
}

/// The Glitch's theme: bit-crushed drums whose pattern rewrites itself every bar, a gritty
/// bass on a stuck note and stray digital chirps.
pub(super) fn glitch_loop(rng: &mut Rng) -> Vec<f32> {
    let mut bass = Osc::new(Wave::Square);
    let mut bass_filter = Svf::new(FilterMode::LowPass, 500.0, 1.4);
    let mut out = render(loop_length(), |t| {
        let gate = if (t * 4.0).fract() < 0.6 { 1.0 } else { 0.3 };
        let crushed = (bass.next(55.0) * 3.0).round() / 3.0;
        bass_filter.run(crushed, 420.0 + 300.0 * sine(t, 0.5), 1.4) * gate * 0.32
    });
    // Four bars of eighths, each bar's kick pattern rewritten.
    let patterns: [[bool; 8]; 4] = [
        [true, false, false, true, false, false, true, false],
        [true, true, false, false, true, false, false, true],
        [true, false, true, false, true, false, true, true],
        [true, false, false, false, true, true, false, true],
    ];
    let step = LOOP_SECONDS / 32.0;
    for (bar, pattern) in patterns.iter().enumerate() {
        for (beat, hit) in pattern.iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "thirty-two steps")]
            let start = (bar * 8 + beat) as f32 * step;
            if *hit {
                loop_note(&mut out, &thump(160.0, 45.0, 0.12, 1.8), start, 0.5);
            }
            if beat % 2 == 1 {
                let mut hat = Noise::new(NoiseColor::White, rng.next_u64());
                let mut band = Svf::new(FilterMode::HighPass, 7_000.0, 0.7);
                let tick = render(0.05, |t| band.process(hat.next()) * perc(t, 0.001, 0.04));
                loop_note(&mut out, &tick, start, 0.18);
            }
        }
    }
    scatter_grains(
        rng,
        &mut out,
        10,
        (0.0, LOOP_SECONDS),
        (2_000.0, 6_000.0),
        0.05,
        false,
    );
    out
}

/// The Time Sludge's ambience: clockwork ticking slow and fast, drips that hang.
pub(super) fn clockwork_bed(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(loop_length(), |_| 0.0);
    for index in 0..12_usize {
        #[allow(clippy::cast_precision_loss, reason = "twelve ticks")]
        let start = index as f32 * LOOP_SECONDS / 12.0;
        let pitch = if index % 3 == 0 { 3_200.0 } else { 2_400.0 };
        loop_note(
            &mut out,
            &bell(pitch * rng.jitter(0.01), 1.0, 0.2, 0.03),
            start,
            0.05,
        );
    }
    out
}

/// The Echo Caves' ambience: dead air, one far drip and its echoes.
pub(super) fn echoes_bed(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(loop_length(), |_| 0.0);
    let drip = bell(rng.range(1_400.0, 1_600.0), 1.5, 0.5, 0.25);
    for (offset, gain) in [(0.5_f32, 0.12_f32), (0.95, 0.05), (1.4, 0.02)] {
        loop_note(&mut out, &drip, offset, gain);
    }
    out
}

/// The Prism Desert's ambience: a glassy hiss of prism sand and edges singing.
pub(super) fn prism_bed(rng: &mut Rng) -> Vec<f32> {
    let mut sand = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 4_000.0, 0.8);
    let mut sing = Osc::new(Wave::Sine);
    render(loop_length(), |t| {
        band.process(sand.next()) * 0.05 * (0.6 + 0.4 * sine(t, 0.25))
            + sing.next(2_637.0) * 0.012 * (0.5 + 0.5 * sine(t, 0.5))
    })
}
