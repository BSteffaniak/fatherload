//! The outland worlds' sounds (Wave 13 planets-c): a piece of music woven into its world's
//! ambience for each of the fungal, machine and dead worlds, and the one-shots of their rules
//! and their theme bosses. Every name carries the track's `pc_` prefix.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop;
//! scattered textures spread evenly over the render.

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
    FilterMode, Fm, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, burst, grains,
    loop_events, loop_length, loop_note, mix_into, perc, render, saturate, scatter_grains,
    segments, sine, thump,
};

// ---------------------------------------------------------------------------------------------
// Music.
// ---------------------------------------------------------------------------------------------

/// The fungal world's music: a slow, wet, wondering piece in F lydian. A breathing pad of
/// soft square waves under a glassy kalimba line, drips falling into pools, and the puff of
/// spores now and then; under it all the hush of the mycelium.
pub(super) fn fungal_music_loop(rng: &mut Rng) -> Vec<f32> {
    let mut hush = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut hush_filter = Svf::new(FilterMode::BandPass, 700.0, 0.7);
    let mut pad = [
        Osc::new(Wave::Square).with_phase(rng.unit()),
        Osc::new(Wave::Square).with_phase(rng.unit()),
        Osc::new(Wave::Square).with_phase(rng.unit()),
    ];
    let mut pad_filter = Svf::new(FilterMode::LowPass, 900.0, 0.8);
    // F, A, C (an F major triad, breathing in and out once a loop).
    let notes = [174.5_f32, 220.0, 261.75];
    let mut out = render(loop_length(), |t| {
        let breath = 0.55 + 0.45 * sine(t, 0.25).abs();
        let chord: f32 = pad
            .iter_mut()
            .zip(notes)
            .map(|(osc, note)| osc.next(note))
            .sum::<f32>()
            / 3.0;
        pad_filter.process(chord) * 0.05 * breath + hush_filter.process(hush.next()) * 0.012
    });
    // The kalimba: a lydian line (the raised fourth, B natural, its colour).
    let step = LOOP_SECONDS / 16.0;
    let line = [
        (0, 698.5_f32),
        (2, 880.0),
        (3, 987.75),
        (5, 1_046.5),
        (8, 880.0),
        (10, 783.75),
        (11, 987.75),
        (13, 698.5),
    ];
    for (index, note) in line {
        let tine = bell(note * rng.jitter(0.002), 5.0, 0.9, 0.7);
        loop_note(&mut out, &tine, index as f32 * step, 0.08);
        let echo = bell(note * 2.0, 5.0, 0.5, 0.4);
        loop_note(&mut out, &echo, index as f32 * step + step * 1.5, 0.025);
    }
    // Drips into pools: a falling sine blip, evenly spread.
    for index in 0..loop_events(1.25) {
        let start = (index as f32 + rng.unit() * 0.6) * LOOP_SECONDS / loop_events(1.25) as f32;
        let mut drip = Osc::new(Wave::Sine);
        let pitch = rng.range(1_400.0, 2_200.0);
        let blip = render(0.12, |t| {
            drip.next(pitch * (1.0 - t * 3.0)) * perc(t, 0.001, 0.08)
        });
        loop_note(&mut out, &blip, start, 0.05);
    }
    // A spore puff: a breath of filtered noise.
    for start in [1.3_f32, 3.4] {
        let puff = burst(
            rng.next_u64(),
            NoiseColor::Pink,
            FilterMode::BandPass,
            1_200.0,
            0.8,
            0.5,
        );
        loop_note(&mut out, &puff, start, 0.06);
    }
    out
}

/// The machine world's music: the planet is a machine and still runs. A pulse of pistons in
/// E minor, a clockwork arpeggio of struck metal, a deep reactor hum, and far off the clank of
/// the gear works on the half bar.
pub(super) fn machine_music_loop(rng: &mut Rng) -> Vec<f32> {
    let mut hum = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut hum_filter = Svf::new(FilterMode::LowPass, 180.0, 1.4);
    let mut whine = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut out = render(loop_length(), |t| {
        hum_filter.process(hum.next(41.25)) * 0.09 * (0.8 + 0.2 * sine(t, 1.0))
            + whine.next(1_975.5) * 0.004 * (0.5 + 0.5 * sine(t, 0.5))
    });
    let step = LOOP_SECONDS / 16.0;
    // Pistons: a hiss and a thud on every beat, the off-beat lighter.
    for index in 0..16 {
        let heavy = index % 4 == 0;
        let thud = thump(if heavy { 90.0 } else { 140.0 }, 60.0, 0.12, 1.6);
        loop_note(
            &mut out,
            &thud,
            index as f32 * step,
            if heavy { 0.16 } else { 0.06 },
        );
        if index % 2 == 1 {
            let hiss = burst(
                rng.next_u64(),
                NoiseColor::White,
                FilterMode::HighPass,
                5_000.0,
                0.7,
                0.08,
            );
            loop_note(&mut out, &hiss, index as f32 * step, 0.03);
        }
    }
    // The clockwork arpeggio: E minor, struck metal.
    let notes = [
        329.75_f32, 392.0, 493.75, 659.25, 587.25, 493.75, 392.0, 440.0,
    ];
    for (index, note) in notes.into_iter().enumerate() {
        let strike = bell(note, 3.5, 1.4, 0.35);
        loop_note(&mut out, &strike, index as f32 * step * 2.0 + step, 0.07);
    }
    // The gear works clank on the half bar.
    for start in [0.0_f32, LOOP_SECONDS / 2.0] {
        let clank = bell(146.75, 2.71, 3.0, 0.5);
        loop_note(&mut out, &clank, start + step * 0.5, 0.06);
    }
    out
}

/// The dead world's music: a lament for a world that died. A low choir-like drone on D, a
/// slow bowed line in D aeolian over it, the wind across the ash plains, and a single far
/// bell, as if from the Hollow Throne.
pub(super) fn dead_music_loop(rng: &mut Rng) -> Vec<f32> {
    let mut wind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut wind_filter = Svf::new(FilterMode::BandPass, 500.0, 1.2);
    let mut choir = [
        Osc::new(Wave::Saw).with_phase(rng.unit()),
        Osc::new(Wave::Saw).with_phase(rng.unit()),
    ];
    let mut choir_filter = Svf::new(FilterMode::LowPass, 600.0, 2.0);
    let mut bow = Osc::new(Wave::Triangle).with_phase(rng.unit());
    // The bowed line: D aeolian, a note a beat-pair, sliding between.
    let line = [
        293.75_f32, 349.25, 329.75, 293.75, 261.75, 293.75, 220.0, 261.75,
    ];
    let mut out = render(loop_length(), |t| {
        let gust = 0.5 + 0.5 * sine(t, 0.25);
        let air = wind_filter.run(wind.next(), 380.0 + 260.0 * gust, 1.2) * 0.02 * gust;
        let drone = choir_filter.process(choir[0].next(73.5) + choir[1].next(110.25)) * 0.035;
        let index = ((t / (LOOP_SECONDS / 8.0)) as usize) % line.len();
        let within = (t % (LOOP_SECONDS / 8.0)) / (LOOP_SECONDS / 8.0);
        let swell = (within * std::f32::consts::PI).sin();
        let voice = bow.next(line[index] * (1.0 + 0.004 * sine(t, 5.0))) * 0.03 * swell;
        air + drone + voice
    });
    let toll = bell(146.75, 2.76, 2.2, 3.2);
    loop_note(&mut out, &toll, 0.6, 0.07);
    out
}

// ---------------------------------------------------------------------------------------------
// One-shots.
// ---------------------------------------------------------------------------------------------

/// A cap throws a rig back up: a rubbery boing.
pub(super) fn cap_bounce(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine).with_phase(rng.unit());
    let base = rng.range(150.0, 180.0);
    render(0.45, |t| {
        let pitch =
            base * (1.0 + 1.6 * (-t * 9.0).exp()) * (1.0 + 0.12 * sine(t, 22.0) * (-t * 6.0).exp());
        saturate(osc.next(pitch) * perc(t, 0.002, 0.4), 1.3)
    })
}

/// The mycelium creeps back: a soft wet crackle of threads knitting.
pub(super) fn mycelium_creep(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(0.9, |_| 0.0);
    grains(rng, &mut out, 26, (0.0, 0.85), (1_800.0, 4_200.0), 0.18);
    let swell = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        800.0,
        0.7,
        0.8,
    );
    mix_into(&mut out, &swell, 0.0, 0.3);
    out
}

/// A glowcap flashes: a bright rising chime and a shimmer.
pub(super) fn glowflash(rng: &mut Rng) -> Vec<f32> {
    let mut out = bell(1_046.5 * rng.jitter(0.01), 2.0, 1.2, 0.9);
    let high = bell(2_093.0, 3.0, 0.6, 0.6);
    mix_into(&mut out, &high, 0.03, 0.6);
    let mut shimmer = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::HighPass, 7_000.0, 0.7);
    let air = render(0.7, |t| {
        filter.process(shimmer.next()) * perc(t, 0.01, 0.6) * 0.4
    });
    mix_into(&mut out, &air, 0.0, 1.0);
    out
}

/// A spore vent swells: a rising hiss with a deep, building pressure note.
pub(super) fn vent_swell(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 400.0, 1.2);
    let mut low = Osc::new(Wave::Sine);
    render(1.8, |t| {
        let rise = t / 1.8;
        filter.run(noise.next(), 300.0 + 2_400.0 * rise, 1.2) * rise * 0.8
            + low.next(55.0 + 40.0 * rise) * 0.35 * rise
    })
}

/// A spore vent blasts: a whoosh of spores.
pub(super) fn vent_blast(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 3_000.0, 0.8);
    let mut out = render(1.2, |t| {
        filter.run(noise.next(), 4_000.0 - 2_800.0 * (t / 1.2), 0.8)
            * segments(t, &[(0.0, 0.0), (0.04, 1.0), (0.5, 0.6), (1.2, 0.0)])
    });
    let thud = thump(80.0, 45.0, 0.3, 1.8);
    mix_into(&mut out, &thud, 0.0, 0.6);
    out
}

/// The gear works clank: a heavy iron clunk, the ratchet's chatter after it.
pub(super) fn gear_clank(rng: &mut Rng) -> Vec<f32> {
    let mut out = bell(98.0 * rng.jitter(0.01), 2.71, 4.0, 0.7);
    let thud = thump(110.0, 70.0, 0.15, 2.0);
    mix_into(&mut out, &thud, 0.0, 0.8);
    for index in 0..6 {
        let tick = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            3_200.0,
            2.0,
            0.02,
        );
        mix_into(&mut out, &tick, 0.18 + index as f32 * 0.07, 0.4);
    }
    out
}

/// A relay flips: a clack and a rising electric tone (a falling one turns it dark).
pub(super) fn relay_flip(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        2_400.0,
        1.4,
        0.04,
    );
    let mut fm = Fm::default();
    let tone = render(0.35, |t| {
        fm.next(440.0 + 440.0 * (t / 0.35), 2.0, 0.8) * perc(t, 0.005, 0.3)
    });
    mix_into(&mut out, &tone, 0.02, 0.5);
    out
}

/// A tomb's seal breaks: stone grinding, a low gust out of the dark.
pub(super) fn tomb_open(rng: &mut Rng) -> Vec<f32> {
    // The slab grinding open: a rasping band of noise sliding down.
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 900.0, 1.6);
    let mut out = render(1.4, |t| {
        filter.run(noise.next(), 1_400.0 - 600.0 * (t / 1.4), 1.6)
            * segments(t, &[(0.0, 0.0), (0.08, 1.0), (0.9, 0.7), (1.4, 0.0)])
    });
    grains(rng, &mut out, 26, (0.0, 1.2), (900.0, 3_000.0), 0.3);
    // A cold breath out of the dark.
    let mut gust = Osc::new(Wave::Saw);
    let mut gust_filter = Svf::new(FilterMode::BandPass, 500.0, 3.0);
    let moan = render(1.3, |t| {
        gust_filter.process(gust.next(165.0 + 12.0 * sine(t, 0.8))) * perc(t, 0.4, 1.2) * 0.3
    });
    mix_into(&mut out, &moan, 0.2, 1.0);
    out
}

/// A ghost light's touch: a cold glassy whisper.
pub(super) fn wisp_chill(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 5_000.0, 6.0);
    let mut out = render(0.9, |t| {
        filter.run(noise.next(), 6_000.0 - 3_000.0 * (t / 0.9), 6.0) * perc(t, 0.05, 0.8) * 0.7
    });
    let glass = bell(1_567.75, 1.5, 0.4, 0.8);
    mix_into(&mut out, &glass, 0.05, 0.4);
    out
}

/// A theme boss wakes: a low roar rising out of the dark, a struck gong.
pub(super) fn boss_wake(rng: &mut Rng) -> Vec<f32> {
    let mut roar = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut roar_filter = Svf::new(FilterMode::LowPass, 400.0, 1.5);
    let mut out = render(2.4, |t| {
        let pitch = 55.0 + 20.0 * segments(t, &[(0.0, 0.0), (1.0, 1.0), (2.4, 0.6)]);
        roar_filter.run(
            roar.next(pitch),
            200.0 + 900.0 * segments(t, &[(0.0, 0.0), (0.9, 1.0), (2.4, 0.3)]),
            1.5,
        ) * segments(t, &[(0.0, 0.0), (0.6, 1.0), (1.8, 0.7), (2.4, 0.0)])
    });
    let gong = bell(73.5, 1.41, 3.0, 2.2);
    mix_into(&mut out, &gong, 0.0, 0.6);
    out
}

/// A theme boss calls its brood: three rising horn-like calls.
pub(super) fn brood_call(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(1.3, |_| 0.0);
    for (index, pitch) in [220.0_f32, 261.75, 329.75].into_iter().enumerate() {
        let mut osc = Osc::new(Wave::Square).with_phase(rng.unit());
        let mut filter = Svf::new(FilterMode::LowPass, 1_200.0, 1.5);
        let call = render(0.4, |t| {
            filter.process(osc.next(pitch * (1.0 + 0.05 * t))) * perc(t, 0.03, 0.35)
        });
        mix_into(&mut out, &call, index as f32 * 0.32, 0.5);
    }
    out
}

/// A theme boss falls: a long collapse, a ringing chord as the vault opens.
pub(super) fn boss_fall(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        1_600.0,
        0.8,
        1.6,
    );
    let boom = thump(70.0, 35.0, 0.9, 2.2);
    mix_into(&mut out, &boom, 0.0, 0.6);
    for (index, note) in [293.75_f32, 369.0, 440.0, 587.25].into_iter().enumerate() {
        let ring = bell(note, 2.0, 0.8, 2.0);
        mix_into(&mut out, &ring, 0.8 + index as f32 * 0.12, 0.4);
    }
    let mut dust = Vec::new();
    scatter_grains(rng, &mut dust, 24, (0.1, 1.6), (900.0, 3_600.0), 0.3, true);
    mix_into(&mut out, &dust, 0.0, 1.0);
    out
}
