//! The exotic worlds' sounds (Wave 13 planets-d): a music bed and an ambience bed for each of
//! the gas giant, the tidally locked world and the ring world, the bosses' fight music, and the
//! one-shots of their rules (a bolt's crackle and its strike, the pressure's creak, hail and
//! meteorites landing, geysers hissing and blowing, tidal quakes, a meteor's whistle, venting
//! ice, live conduits and their cut, the storms, the bosses waking, telling and falling, and a
//! sky city's docking chime). Every name carries the track's `pd_` prefix.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop;
//! scattered events spread evenly over the render.

#![allow(
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::cast_precision_loss,
    reason = "presentation code: sound math reads top to bottom"
)]

use super::{
    FilterMode, Fm, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, burst, loop_events,
    loop_length, loop_note, mix_into, perc, render, saturate, scatter, scatter_grains, segments,
    sine, thump,
};

/// A soft mallet note (a vibraphone's): a sine with a quiet octave, slowly fading.
fn mallet(frequency: f32, length: f32) -> Vec<f32> {
    let mut a = Osc::new(Wave::Sine);
    let mut b = Osc::new(Wave::Sine);
    render(length, |t| {
        (a.next(frequency) + b.next(frequency * 4.0) * 0.12 * (-t * 6.0).exp())
            * perc(t, 0.004, length * 0.85)
            * (0.85 + 0.15 * sine(t, 5.5))
    })
}

/// A plucked string through a closing low-pass.
fn pluck(frequency: f32, length: f32) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, frequency * 5.0, 0.8);
    render(length, |t| {
        let cut = frequency * (1.4 + 5.0 * (-t * 7.0).exp());
        filter.run(osc.next(frequency), cut, 0.8) * perc(t, 0.003, length * 0.8)
    })
}

/// A gasbag's call far off: a low sine gliding up and back, like a whale's.
fn far_call(rng: &mut Rng, base: f32, length: f32) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut over = Osc::new(Wave::Triangle).with_phase(rng.unit());
    render(length, |t| {
        let rise = (t / length * std::f32::consts::PI).sin();
        let frequency = base * (1.0 + 0.35 * rise);
        (osc.next(frequency) + over.next(frequency * 2.0) * 0.2)
            * segments(
                t,
                &[
                    (0.0, 0.0),
                    (length * 0.3, 1.0),
                    (length * 0.7, 0.8),
                    (length, 0.0),
                ],
            )
    })
}

// ---------------------------------------------------------------------------------------------
// Music.
// ---------------------------------------------------------------------------------------------

/// The gas giant's music: drifting in D mixolydian: a breathing pad of near-unison saws, a slow
/// mallet ostinato that rises like warm air, and the gasbags' calls far off.
pub(super) fn gas_giant_music_loop(rng: &mut Rng) -> Vec<f32> {
    let pad_notes = [146.75_f32, 220.0, 261.75, 293.75];
    let mut pads: Vec<(Osc, Osc)> = pad_notes
        .iter()
        .map(|_| {
            (
                Osc::new(Wave::Saw).with_phase(rng.unit()),
                Osc::new(Wave::Saw).with_phase(rng.unit()),
            )
        })
        .collect();
    let mut filter = Svf::new(FilterMode::LowPass, 700.0, 0.7);
    let mut out = render(loop_length(), |t| {
        let body: f32 = pads
            .iter_mut()
            .zip(pad_notes)
            .map(|((a, b), note)| a.next(note) + b.next(note + 0.25))
            .sum();
        filter.run(
            body * 0.25,
            520.0 + 260.0 * (0.5 + 0.5 * sine(t, 0.25)),
            0.7,
        ) * 0.16
            * (0.7 + 0.3 * sine(t, 0.25))
    });
    let step = LOOP_SECONDS / 16.0;
    // D, E, F sharp, A, C (the mixolydian seventh), A, F sharp, E.
    let ostinato = [
        587.25_f32, 659.25, 740.0, 880.0, 1_046.5, 880.0, 740.0, 659.25,
    ];
    for bar in 0..2_usize {
        for (index, note) in ostinato.into_iter().enumerate() {
            let at = (bar * 8 + index) as f32 * step;
            let voice = mallet(note * if bar == 1 { 0.75 } else { 1.0 }, step * 2.5);
            loop_note(&mut out, &voice, at, 0.075);
        }
    }
    for (at, base) in [(LOOP_SECONDS * 0.2, 73.5_f32), (LOOP_SECONDS * 0.65, 98.0)] {
        let call = far_call(rng, base, LOOP_SECONDS * 0.25);
        loop_note(&mut out, &call, at, 0.11);
    }
    out
}

/// The locked world's music: one half lit and one half dark: a drone on A, a chord that turns
/// from A major (the day) to A minor (the night) and back, a slow pluck line and a ticking
/// pulse like a clock that never moves on.
pub(super) fn locked_music_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut drone_low = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut major = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut minor = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut fifth = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut out = render(loop_length(), |t| {
        // The third turns: C sharp by day, C by night, crossfading twice a loop.
        let day = 0.5 + 0.5 * sine(t, 0.25);
        let third = major.next(277.25) * day + minor.next(261.75) * (1.0 - day);
        (drone.next(110.0) * 0.5 + drone_low.next(55.0) * 0.5) * 0.14
            + (third + fifth.next(329.5)) * 0.045
    });
    let beat = LOOP_SECONDS / 16.0;
    for step in 0..16_usize {
        let tick = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            if step % 4 == 0 { 2_400.0 } else { 3_600.0 },
            4.0,
            0.03,
        );
        loop_note(
            &mut out,
            &tick,
            step as f32 * beat,
            if step % 4 == 0 { 0.07 } else { 0.035 },
        );
    }
    // A, C sharp, E, G sharp | A, C, E, G.
    let line = [
        (0, 440.0_f32),
        (2, 554.25),
        (4, 659.25),
        (6, 830.5),
        (8, 440.0),
        (10, 523.25),
        (12, 659.25),
        (14, 784.0),
    ];
    for (step, note) in line {
        let voice = pluck(note, beat * 2.0 + 0.4);
        loop_note(&mut out, &voice, step as f32 * beat, 0.09);
    }
    out
}

/// The ring world's music: bright and wheeling in C lydian: FM bells circling an arpeggio
/// like debris round the ring, a pulsing bass, and a shimmer high above.
pub(super) fn ring_music_loop(rng: &mut Rng) -> Vec<f32> {
    let mut bass = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut bass_filter = Svf::new(FilterMode::LowPass, 300.0, 1.0);
    let beat = LOOP_SECONDS / 16.0;
    let roots = [65.5_f32, 65.5, 73.5, 61.75];
    let mut out = render(loop_length(), |t| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a bar of four"
        )]
        let bar = ((t / (LOOP_SECONDS / 4.0)) as usize).min(3);
        let into = (t / (beat * 0.5)).fract();
        let pulse = (into * 40.0).min(1.0) * ((0.7 - into) * 40.0).clamp(0.0, 1.0);
        bass_filter.run(bass.next(roots[bar]), 260.0, 1.0) * 0.2 * pulse
    });
    // C, E, F sharp (the lydian fourth), G, B, G, F sharp, E.
    let arpeggio = [
        523.25_f32, 659.25, 740.0, 784.0, 987.75, 784.0, 740.0, 659.25,
    ];
    for bar in 0..2_usize {
        for (index, note) in arpeggio.into_iter().enumerate() {
            let at = (bar * 8 + index) as f32 * beat;
            let chime = bell(note * if bar == 1 { 1.125 } else { 1.0 }, 3.5, 0.8, 1.0);
            loop_note(&mut out, &chime, at, 0.06);
        }
    }
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut air = Svf::new(FilterMode::HighPass, 7_000.0, 0.7);
    let shimmer = render(loop_length(), |t| {
        air.process(noise.next()) * 0.025 * (0.6 + 0.4 * sine(t, 0.5))
    });
    mix_into(&mut out, &shimmer, 0.0, 1.0);
    out
}

/// A theme boss's fight: a driving bass in E minor, war drums on the beat and brass stabs
/// that climb each bar.
pub(super) fn boss_loop(rng: &mut Rng) -> Vec<f32> {
    let mut bass = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut bass_two = Osc::new(Wave::Square).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 420.0, 1.2);
    let beat = LOOP_SECONDS / 16.0;
    let notes = [82.5_f32, 82.5, 98.0, 87.25];
    let mut out = render(loop_length(), |t| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a bar of four"
        )]
        let bar = ((t / (LOOP_SECONDS / 4.0)) as usize).min(3);
        let into = (t / (beat * 0.5)).fract();
        let gate = (into * 50.0).min(1.0) * ((0.75 - into) * 50.0).clamp(0.0, 1.0);
        let body = bass.next(notes[bar]) + bass_two.next(notes[bar] * 0.5) * 0.4;
        saturate(filter.run(body * 0.5, 380.0, 1.2), 2.0) * 0.2 * gate
    });
    for step in 0..16_usize {
        if step % 2 == 0 {
            let hit = thump(70.0, 45.0, 0.3, 2.0);
            loop_note(
                &mut out,
                &hit,
                step as f32 * beat,
                if step % 4 == 0 { 0.4 } else { 0.22 },
            );
        }
    }
    let mut horn = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut horn_filter = Svf::new(FilterMode::LowPass, 1_200.0, 1.0);
    for (bar, note) in [329.5_f32, 392.0, 440.0, 493.75].into_iter().enumerate() {
        let stab = render(beat * 1.5, |t| {
            horn_filter.run(horn.next(note), 1_000.0, 1.0) * perc(t, 0.01, beat * 1.2)
        });
        loop_note(&mut out, &stab, (bar * 4) as f32 * beat + beat * 2.0, 0.09);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Ambience.
// ---------------------------------------------------------------------------------------------

/// The gas giant's air: a deep howl of wind through the bands and thunder rolling far below.
pub(super) fn gas_wind_loop(rng: &mut Rng) -> Vec<f32> {
    let mut wind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 260.0, 2.2);
    let mut out = render(loop_length(), |t| {
        let swell = 0.6 + 0.4 * sine(t, 0.25);
        band.run(wind.next(), 200.0 + 180.0 * (0.5 + 0.5 * sine(t, 0.5)), 2.2) * swell * 0.8
    });
    for at in scatter(rng, loop_events(1.0), 0.0, LOOP_SECONDS, 2.0) {
        let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
        let mut low = Svf::new(FilterMode::LowPass, 140.0, 0.8);
        let mut floor = Svf::new(FilterMode::HighPass, 45.0, 0.7);
        let roll = render(1.6, |t| {
            floor.process(low.process(noise.next()))
                * 1.6
                * segments(t, &[(0.0, 0.0), (0.2, 1.0), (1.6, 0.0)])
        });
        loop_note(&mut out, &roll, at, 0.3);
    }
    out
}

/// The locked world's air: a thin wind from the day side to the night, ice ticking as it
/// cools, a low hum of the stressed ground.
pub(super) fn locked_air_loop(rng: &mut Rng) -> Vec<f32> {
    let mut wind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 900.0, 1.4);
    let mut hum = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut out = render(loop_length(), |t| {
        band.run(
            wind.next(),
            700.0 + 400.0 * (0.5 + 0.5 * sine(t, 0.25)),
            1.4,
        ) * 0.35
            + hum.next(55.0) * 0.05 * (0.6 + 0.4 * sine(t, 0.5))
    });
    scatter_grains(
        rng,
        &mut out,
        loop_events(6.0),
        (0.0, LOOP_SECONDS),
        (3_000.0, 7_000.0),
        0.1,
        false,
    );
    out
}

/// The ring world's hum: resonant tones beating slowly, a sweep like the ring turning
/// overhead, and the odd ping of debris.
pub(super) fn ring_hum_loop(rng: &mut Rng) -> Vec<f32> {
    let mut a = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut b = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut c = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut sweep = Svf::new(FilterMode::BandPass, 800.0, 3.0);
    let mut out = render(loop_length(), |t| {
        (a.next(196.0) + b.next(196.5) * 0.8 + c.next(294.0) * 0.4)
            * 0.07
            * (0.7 + 0.3 * sine(t, 0.25))
            + sweep.run(
                noise.next(),
                500.0 + 1_200.0 * (0.5 + 0.5 * sine(t, 0.25)),
                3.0,
            ) * 0.12
    });
    for at in scatter(rng, loop_events(1.5), 0.0, LOOP_SECONDS, 1.0) {
        let note = [1_568.0_f32, 1_975.5, 2_349.25][rng.index(3)];
        let ping = bell(note, 3.5, 0.5, 0.6);
        loop_note(&mut out, &ping, at, 0.06);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// One-shots.
// ---------------------------------------------------------------------------------------------

/// A charged column flickers: a dry electric crackle rising.
pub(super) fn crackle(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; 1];
    for at in scatter(rng, 18, 0.0, 1.2, 1.4) {
        let spark = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            rng.range(3_000.0, 6_000.0),
            0.8,
            0.015,
        );
        mix_into(&mut out, &spark, at, 0.35 + 0.4 * at / 1.2);
    }
    out
}

/// A bolt strikes: a sharp crack and a rolling boom.
pub(super) fn bolt(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        1_500.0,
        0.7,
        0.12,
    );
    let boom = thump(80.0, 40.0, 0.7, 2.4);
    mix_into(&mut out, &boom, 0.02, 0.8);
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 300.0, 0.8);
    let mut floor = Svf::new(FilterMode::HighPass, 45.0, 0.7);
    let roll = render(1.4, |t| {
        floor.process(low.process(noise.next()))
            * 1.4
            * segments(t, &[(0.0, 0.0), (0.05, 1.0), (1.4, 0.0)])
    });
    mix_into(&mut out, &roll, 0.05, 0.7);
    out
}

/// The pressure bites: a hull groaning under the weight of the gas.
pub(super) fn pressure_creak(rng: &mut Rng) -> Vec<f32> {
    let mut groan = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::BandPass, 240.0, 4.0);
    render(1.2, |t| {
        filter.run(
            groan.next(70.0 + 25.0 * sine(t, 1.3)),
            220.0 + 120.0 * (t / 1.2),
            4.0,
        ) * segments(t, &[(0.0, 0.0), (0.2, 1.0), (0.9, 0.7), (1.2, 0.0)])
            * 0.9
    })
}

/// A hailstone or meteorite lands: a hard knock and a scatter of grit.
pub(super) fn impact(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(120.0, 55.0, 0.3, 2.2);
    let knock = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        2_000.0,
        1.2,
        0.06,
    );
    mix_into(&mut out, &knock, 0.0, 0.6);
    super::grains(rng, &mut out, 10, (0.03, 0.4), (1_500.0, 5_000.0), 0.3);
    out
}

/// A geyser vent hisses: steam rising in pitch.
pub(super) fn geyser_hiss(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 2_000.0, 2.0);
    render(1.8, |t| {
        filter.run(noise.next(), 1_500.0 + 3_000.0 * (t / 1.8), 2.0)
            * segments(t, &[(0.0, 0.0), (1.2, 1.0), (1.8, 0.4)])
            * 0.6
    })
}

/// A geyser blows: a roaring jet.
pub(super) fn geyser_blow(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 2_400.0, 0.8);
    let mut out = render(2.2, |t| {
        filter.run(noise.next(), 2_400.0 - 1_200.0 * (t / 2.2), 0.8)
            * segments(t, &[(0.0, 0.0), (0.08, 1.0), (1.6, 0.8), (2.2, 0.0)])
            * 1.1
    });
    let whump = thump(90.0, 50.0, 0.4, 1.6);
    mix_into(&mut out, &whump, 0.0, 0.6);
    out
}

/// A tidal quake rumbles: the ground groaning and grinding.
pub(super) fn quake(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 110.0, 0.9);
    let mut floor = Svf::new(FilterMode::HighPass, 45.0, 0.7);
    let mut out = render(3.0, |t| {
        floor.process(filter.process(noise.next()))
            * 2.2
            * (0.7 + 0.3 * sine(t, 6.0))
            * segments(t, &[(0.0, 0.0), (1.0, 1.0), (2.6, 0.8), (3.0, 0.0)])
    });
    super::grains(rng, &mut out, 16, (0.4, 2.8), (300.0, 1_800.0), 0.25);
    out
}

/// Ring debris is falling: a long whistle down.
pub(super) fn meteor_whistle(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 2_000.0, 6.0);
    render(1.6, |t| {
        let pitch = 2_200.0 - 1_400.0 * (t / 1.6);
        (osc.next(pitch) * 0.12 + filter.run(noise.next(), pitch, 6.0) * 0.4)
            * segments(t, &[(0.0, 0.0), (0.4, 0.8), (1.5, 1.0), (1.6, 0.0)])
    })
}

/// Buried ring ice vents: a sharp burst of hissing vapour.
pub(super) fn ice_vent(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        2_500.0,
        0.8,
        0.5,
    );
    let pop = thump(160.0, 90.0, 0.15, 1.4);
    mix_into(&mut out, &pop, 0.0, 0.6);
    let tink = bell(rng.range(2_400.0, 3_000.0), 3.1, 0.5, 0.3);
    mix_into(&mut out, &tink, 0.05, 0.3);
    out
}

/// A live conduit shocks: a buzzing zap.
pub(super) fn conduit_zap(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Square).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::BandPass, 1_200.0, 2.0);
    render(0.35, |t| {
        filter.run(osc.next(120.0 + 40.0 * sine(t, 31.0)), 1_200.0, 2.0) * perc(t, 0.005, 0.3) * 0.8
    })
}

/// A conduit's junction is cut: the run winds down.
pub(super) fn power_down(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 2_000.0, 1.2);
    render(1.0, |t| {
        let pitch = 440.0 * (-t * 2.5).exp() + 40.0;
        filter.run(osc.next(pitch), 600.0 + 1_400.0 * (1.0 - t), 1.2)
            * segments(t, &[(0.0, 0.0), (0.02, 1.0), (1.0, 0.0)])
            * 0.35
    })
}

/// The storm is rising: a wind swelling into a moan.
pub(super) fn storm_rising(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 260.0, 3.0);
    render(2.6, |t| {
        filter.run(noise.next(), 220.0 + 460.0 * (t / 2.6), 3.0)
            * 1.4
            * segments(t, &[(0.0, 0.0), (2.0, 1.0), (2.6, 0.0)])
    })
}

/// A theme boss wakes: a deep roar under a rising brass chord.
pub(super) fn boss_awake(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(60.0, 32.0, 1.0, 2.6);
    let mut roar = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 360.0, 1.0);
    let body = render(2.0, |t| {
        low.run(roar.next(), 300.0 + 200.0 * sine(t, 2.0).abs(), 1.0)
            * 1.5
            * segments(t, &[(0.0, 0.0), (0.2, 1.0), (1.4, 0.8), (2.0, 0.0)])
    });
    mix_into(&mut out, &body, 0.0, 0.8);
    for (index, note) in [164.75_f32, 196.0, 246.75].into_iter().enumerate() {
        let mut horn = Osc::new(Wave::Saw).with_phase(rng.unit());
        let mut filter = Svf::new(FilterMode::LowPass, 900.0, 1.0);
        let voice = render(1.4, |t| {
            filter.run(horn.next(note), 700.0 + 500.0 * (t / 1.4), 1.0)
                * segments(t, &[(0.0, 0.0), (0.2, 1.0), (1.1, 0.8), (1.4, 0.0)])
        });
        mix_into(&mut out, &voice, 0.3 + index as f32 * 0.12, 0.12);
    }
    out
}

/// A boss tells an attack: a rising warning tone over a charge.
pub(super) fn boss_tell(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut charge = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 600.0, 3.0);
    render(1.1, |t| {
        (osc.next(330.0 + 330.0 * (t / 1.1)) * 0.25
            + filter.run(charge.next(), 400.0 + 1_600.0 * (t / 1.1), 3.0) * 0.5)
            * segments(t, &[(0.0, 0.0), (0.1, 0.6), (1.0, 1.0), (1.1, 0.0)])
    })
}

/// A boss falls: a crash, then a victorious bright chord ringing out.
pub(super) fn boss_down(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(70.0, 35.0, 0.9, 2.6);
    let crash = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        1_200.0,
        0.7,
        0.6,
    );
    mix_into(&mut out, &crash, 0.0, 0.5);
    for (index, note) in [523.25_f32, 659.25, 784.0, 1_046.5].into_iter().enumerate() {
        let tone = bell(note, 1.0, 0.5, 2.0);
        mix_into(&mut out, &tone, 0.5 + index as f32 * 0.1, 0.25);
    }
    out
}

/// A rig docks at a sky city: two warm chimes and a soft clank of the clamps.
pub(super) fn dock_chime(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(180.0, 120.0, 0.12, 1.2);
    let mut fm = Fm::default();
    let first = render(0.9, |t| {
        fm.next(659.25, 1.0, 0.4 * (-t * 3.0).exp()) * perc(t, 0.004, 0.8)
    });
    mix_into(&mut out, &first, 0.05, 0.3);
    let second = bell(rng.range(985.0, 990.0), 2.0, 0.6, 1.0);
    mix_into(&mut out, &second, 0.25, 0.3);
    out
}
