//! The frontier worlds' sounds (Wave 12 planets-b): a music bed and an ambience bed for each of
//! the dune, volcanic and crystal worlds, and the one-shots of their rules (sand sliding, floors
//! giving way, glass and crystal shattering and ringing, obsidian splinters, a light well's beam
//! and its vault, a volcano's rumble, eruption and bombs, the storms, regrowth, the chime
//! drifter and the dune wyrm's ripple). Every name carries the track's `pb_` prefix.
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

/// A plucked string (an oud's or a kanun's): a bright saw through a closing low-pass.
fn pluck(frequency: f32, length: f32, brightness: f32) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, frequency * 6.0, 0.9);
    render(length, |t| {
        let cut = frequency * (1.5 + brightness * 8.0 * (-t * 9.0).exp());
        filter.run(osc.next(frequency), cut, 0.9) * perc(t, 0.002, length * 0.8)
    })
}

/// A frame drum's hit: a low skin thump and a slap of noise.
fn frame_drum(seed: u64, low: bool) -> Vec<f32> {
    let mut out = if low {
        thump(110.0, 70.0, 0.25, 1.4)
    } else {
        thump(220.0, 180.0, 0.08, 1.1)
    };
    let slap = burst(
        seed,
        NoiseColor::Pink,
        FilterMode::BandPass,
        if low { 900.0 } else { 2_400.0 },
        1.0,
        0.05,
    );
    mix_into(&mut out, &slap, 0.0, if low { 0.3 } else { 0.5 });
    out
}

// ---------------------------------------------------------------------------------------------
// Music.
// ---------------------------------------------------------------------------------------------

/// The dune world's music: a caravan's slow walk in D hijaz: a drone on D, a frame drum's
/// dum-tek, and an oud line wandering the augmented second (E flat to F sharp).
pub(super) fn dune_music_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut drone_fifth = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 400.0, 0.8);
    let mut out = render(loop_length(), |t| {
        let body = drone.next(73.5) * 0.6 + drone_fifth.next(110.0) * 0.4;
        filter.run(body, 320.0 + 80.0 * sine(t, 0.25), 0.8) * 0.18
    });
    let beat = LOOP_SECONDS / 16.0;
    // Dum . . tek . tek dum . | dum . . tek . tek . tek
    let pattern = [
        (0, true),
        (3, false),
        (5, false),
        (6, true),
        (8, true),
        (11, false),
        (13, false),
        (15, false),
    ];
    for (step, low) in pattern {
        let hit = frame_drum(rng.next_u64(), low);
        loop_note(
            &mut out,
            &hit,
            step as f32 * beat,
            if low { 0.32 } else { 0.18 },
        );
    }
    // D, E flat, F sharp, G, A, B flat, C, D.
    let line = [
        (0, 293.75_f32, 2),
        (2, 311.0, 1),
        (3, 370.0, 1),
        (4, 392.0, 2),
        (6, 370.0, 1),
        (7, 311.0, 1),
        (8, 293.75, 3),
        (11, 220.0, 1),
        (12, 233.0, 1),
        (13, 261.75, 1),
        (14, 233.0, 1),
        (15, 220.0, 1),
    ];
    for (step, note, length) in line {
        let voice = pluck(note, beat * length as f32 + 0.25, 0.7);
        loop_note(&mut out, &voice, step as f32 * beat, 0.16);
    }
    out
}

/// The volcanic world's music: a heavy march in E phrygian: taiko thumps, a growling distorted
/// riff falling to the flat second, and a horn calling over the smoke.
pub(super) fn volcanic_music_loop(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut low_two = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 260.0, 1.3);
    let beat = LOOP_SECONDS / 16.0;
    // The riff: E, E, F, E, G, F, E, D (eight beats each held two sixteenths).
    let riff = [82.5_f32, 82.5, 87.25, 82.5, 98.0, 87.25, 82.5, 73.5];
    let mut out = render(loop_length(), |t| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a step of the riff"
        )]
        let index = ((t / (beat * 2.0)) as usize).min(riff.len() - 1);
        let note = riff[index];
        // Each held note opens and closes over a few milliseconds (a hard gate clicked, and
        // at the loop's seam the first note's random phase landed off zero).
        let into = (t / (beat * 2.0)).fract();
        let gate = (into * 60.0).min(1.0) * ((0.85 - into) * 60.0).clamp(0.0, 1.0);
        let body = low.next(note) + low_two.next(note * 1.005);
        saturate(filter.run(body * 0.5, 240.0, 1.3), 2.4) * 0.22 * gate
    });
    for step in [0_usize, 3, 4, 8, 10, 11, 12, 14] {
        let hit = thump(65.0, 45.0, 0.4, 2.0);
        loop_note(
            &mut out,
            &hit,
            step as f32 * beat,
            if step % 4 == 0 { 0.45 } else { 0.28 },
        );
    }
    // A horn call: B, C, B (the flat sixth's pull), twice an octave apart.
    let mut horn = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut horn_filter = Svf::new(FilterMode::LowPass, 900.0, 1.0);
    let call = render(beat * 6.0, |t| {
        let note = if t < beat * 2.0 {
            247.0
        } else if t < beat * 3.0 {
            261.75
        } else {
            247.0
        };
        horn_filter.run(horn.next(note), 700.0 + 300.0 * (t / (beat * 6.0)), 1.0)
            * segments(
                t,
                &[
                    (0.0, 0.0),
                    (0.08, 1.0),
                    (beat * 5.0, 0.8),
                    (beat * 6.0, 0.0),
                ],
            )
    });
    loop_note(&mut out, &call, beat * 8.0, 0.1);
    out
}

/// The crystal world's music: a glass harmonica in E lydian: slow chiming arpeggios, a
/// shimmering pad of near-unison sines and a bell that answers itself an octave up.
pub(super) fn crystal_music_loop(rng: &mut Rng) -> Vec<f32> {
    let pad_notes = [164.75_f32, 207.75, 246.75, 311.0];
    let mut pads: Vec<(Osc, Osc)> = pad_notes
        .iter()
        .map(|_| {
            (
                Osc::new(Wave::Sine).with_phase(rng.unit()),
                Osc::new(Wave::Sine).with_phase(rng.unit()),
            )
        })
        .collect();
    let mut out = render(loop_length(), |t| {
        pads.iter_mut()
            .zip(pad_notes)
            .map(|((a, b), note)| a.next(note) + b.next(note + 0.5))
            .sum::<f32>()
            * 0.05
            * (0.75 + 0.25 * sine(t, 0.25))
    });
    let step = LOOP_SECONDS / 16.0;
    // E, G sharp, A sharp (the lydian fourth), B, D sharp, B, A sharp, G sharp.
    let arpeggio = [
        659.25_f32, 830.5, 932.25, 987.75, 1_244.5, 987.75, 932.25, 830.5,
    ];
    for bar in 0..2_usize {
        for (index, note) in arpeggio.into_iter().enumerate() {
            let at = (bar * 8 + index) as f32 * step;
            let chime = bell(note * if bar == 1 { 0.75 } else { 1.0 }, 3.5, 0.9, 1.4);
            loop_note(&mut out, &chime, at, 0.07);
        }
    }
    for (at, note) in [(0.0_f32, 329.5_f32), (LOOP_SECONDS * 0.5, 370.0)] {
        let mut fm = Fm::default();
        let toll = render(LOOP_SECONDS * 0.48, |t| {
            fm.next(note, 1.0, 0.6 * (-t * 1.5).exp()) * perc(t, 0.01, LOOP_SECONDS * 0.4)
        });
        loop_note(&mut out, &toll, at, 0.14);
        let echo = bell(note * 2.0, 3.5, 0.5, 1.2);
        loop_note(&mut out, &echo, at + step * 3.0, 0.05);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Ambience.
// ---------------------------------------------------------------------------------------------

/// The dune world's air: wind over the dunes rising and falling, a hiss of blown sand.
pub(super) fn dune_wind_loop(rng: &mut Rng) -> Vec<f32> {
    let mut wind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 500.0, 1.6);
    let mut hiss = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hiss_filter = Svf::new(FilterMode::HighPass, 4_000.0, 0.7);
    render(loop_length(), |t| {
        let swell = 0.6 + 0.4 * sine(t, 0.25);
        band.run(wind.next(), 380.0 + 260.0 * (0.5 + 0.5 * sine(t, 0.5)), 1.6) * swell * 0.7
            + hiss_filter.process(hiss.next()) * 0.05 * swell
    })
}

/// The volcanic world's ground: a deep rumble, lava bubbling and the crackle of cooling rock.
pub(super) fn volcano_loop(rng: &mut Rng) -> Vec<f32> {
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 90.0, 0.9);
    let mut out = render(loop_length(), |t| {
        low.process(rumble.next()) * 1.4 * (0.75 + 0.25 * sine(t, 0.5))
    });
    for at in scatter(rng, loop_events(2.0), 0.0, LOOP_SECONDS, 1.0) {
        let pitch = rng.range(120.0, 220.0);
        let mut osc = Osc::new(Wave::Sine);
        let blub = render(0.12, |t| {
            osc.next(pitch * (1.0 + t * 5.0)) * perc(t, 0.005, 0.08)
        });
        loop_note(&mut out, &blub, at, 0.18);
    }
    scatter_grains(
        rng,
        &mut out,
        loop_events(9.0),
        (0.0, LOOP_SECONDS),
        (2_000.0, 6_000.0),
        0.12,
        false,
    );
    out
}

/// The crystal world's hum: high sines beating against each other and a chime now and then.
pub(super) fn crystal_hum_loop(rng: &mut Rng) -> Vec<f32> {
    let mut a = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut b = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut c = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut out = render(loop_length(), |t| {
        (a.next(1_318.5) + b.next(1_319.5) * 0.8 + c.next(1_976.0) * 0.4)
            * 0.08
            * (0.7 + 0.3 * sine(t, 0.25))
    });
    for at in scatter(rng, loop_events(1.5), 0.0, LOOP_SECONDS, 1.0) {
        let note = [1_318.5_f32, 1_661.0, 1_976.0, 2_489.0][rng.index(4)];
        let ping = bell(note, 3.5, 0.6, 0.8);
        loop_note(&mut out, &ping, at, 0.08);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// One-shots.
// ---------------------------------------------------------------------------------------------

/// Loose sand slides into a tunnel: a falling hiss and the patter of grains settling.
pub(super) fn sand_slide(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 3_000.0, 0.8);
    let mut out = render(0.8, |t| {
        filter.run(noise.next(), 3_000.0 - 2_400.0 * (t / 0.8), 0.8)
            * segments(t, &[(0.0, 0.0), (0.06, 1.0), (0.5, 0.5), (0.8, 0.0)])
    });
    let mut grit = Vec::new();
    super::grains(rng, &mut grit, 18, (0.05, 0.75), (2_500.0, 7_000.0), 0.35);
    mix_into(&mut out, &grit, 0.0, 1.0);
    out
}

/// A cracked ruin floor gives way: a stone crack, then rubble falling.
pub(super) fn floor_crumble(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        1_800.0,
        0.8,
        0.08,
    );
    let drop = thump(90.0, 50.0, 0.4, 1.8);
    mix_into(&mut out, &drop, 0.06, 0.8);
    super::grains(rng, &mut out, 24, (0.08, 0.9), (400.0, 2_400.0), 0.5);
    out
}

/// Desert glass shatters in a chain: a bright crash and a spray of tinkling shards.
pub(super) fn glass_shatter(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_500.0,
        0.9,
        0.25,
    );
    for at in scatter(rng, 14, 0.0, 0.6, 1.6) {
        let shard = bell(rng.range(2_200.0, 4_600.0), 2.3, 0.8, 0.18);
        mix_into(&mut out, &shard, at, 0.25);
    }
    out
}

/// Resonant crystal rings: a cluster of inharmonic bells that sing on, then fall apart.
pub(super) fn crystal_ring(rng: &mut Rng) -> Vec<f32> {
    let base = rng.range(880.0, 990.0);
    let mut out = bell(base, 2.76, 1.4, 1.6);
    let high = bell(base * 1.5, 3.1, 0.9, 1.2);
    mix_into(&mut out, &high, 0.02, 0.5);
    let break_up = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        5_000.0,
        0.8,
        0.12,
    );
    mix_into(&mut out, &break_up, 0.25, 0.4);
    out
}

/// Obsidian splinters at the drill: a sharp glassy snap.
pub(super) fn splinter(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        4_200.0,
        2.0,
        0.05,
    );
    let ping = bell(rng.range(1_800.0, 2_300.0), 3.7, 1.2, 0.12);
    mix_into(&mut out, &ping, 0.0, 0.6);
    out
}

/// A light well's beam reaches its receptor: a rising, shimmering major chord.
pub(super) fn beam_lit(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; 1];
    for (index, note) in [659.25_f32, 830.5, 987.75, 1_318.5].into_iter().enumerate() {
        let tone = bell(note, 1.0, 0.4, 1.4);
        mix_into(&mut out, &tone, index as f32 * 0.09, 0.3);
    }
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::HighPass, 6_000.0, 0.7);
    let shimmer = render(1.2, |t| {
        filter.process(noise.next()) * 0.1 * segments(t, &[(0.0, 0.0), (0.3, 1.0), (1.2, 0.0)])
    });
    mix_into(&mut out, &shimmer, 0.0, 1.0);
    out
}

/// A sealed vault opens: a crystal grinding apart and a bright, deep chord ringing out.
pub(super) fn vault_open(rng: &mut Rng) -> Vec<f32> {
    let mut grind = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::BandPass, 400.0, 2.5);
    let mut out = render(0.9, |t| {
        filter.run(grind.next(60.0 + 30.0 * sine(t, 13.0)), 400.0, 2.5)
            * segments(t, &[(0.0, 0.0), (0.1, 1.0), (0.8, 0.6), (0.9, 0.0)])
            * 0.6
    });
    for (index, note) in [329.5_f32, 493.75, 659.25].into_iter().enumerate() {
        let tone = bell(note, 2.0, 0.6, 2.0);
        mix_into(&mut out, &tone, 0.7 + index as f32 * 0.05, 0.35);
    }
    out
}

/// A volcano begins to rumble: a deep swell of the ground.
pub(super) fn rumble(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 120.0, 0.9);
    // The brown noise's drift below hearing goes (it only ate headroom; the integrated
    // stack's patch order reseeded this one past the quality bar's sub-bass line).
    let mut floor = Svf::new(FilterMode::HighPass, 45.0, 0.7);
    render(2.4, |t| {
        floor.process(filter.process(noise.next()))
            * 2.2
            * (0.7 + 0.3 * sine(t, 7.0))
            * segments(t, &[(0.0, 0.0), (0.8, 1.0), (2.0, 0.8), (2.4, 0.0)])
    })
}

/// A volcano erupts: a great boom and the roar of the vent.
pub(super) fn eruption(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(70.0, 35.0, 0.9, 2.6);
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 400.0, 0.8);
    let roar = render(2.2, |t| {
        filter.run(noise.next(), 500.0 - 250.0 * (t / 2.2), 0.8)
            * 1.6
            * segments(t, &[(0.0, 0.0), (0.05, 1.0), (1.2, 0.7), (2.2, 0.0)])
    });
    mix_into(&mut out, &roar, 0.0, 0.9);
    out
}

/// A lava bomb lands: a falling whistle, a thud and a sizzle.
pub(super) fn lava_bomb(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine);
    let mut out = render(0.35, |t| {
        osc.next(1_400.0 - 1_000.0 * (t / 0.35))
            * 0.15
            * segments(t, &[(0.0, 0.0), (0.1, 1.0), (0.35, 0.0)])
    });
    let hit = thump(110.0, 50.0, 0.35, 2.2);
    mix_into(&mut out, &hit, 0.32, 0.9);
    let sizzle = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_000.0,
        0.7,
        0.6,
    );
    mix_into(&mut out, &sizzle, 0.34, 0.25);
    out
}

/// A storm is coming: a low wind rising to a moan.
pub(super) fn storm_rising(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 300.0, 3.0);
    render(2.6, |t| {
        filter.run(noise.next(), 250.0 + 500.0 * (t / 2.6), 3.0)
            * 1.4
            * segments(t, &[(0.0, 0.0), (2.0, 1.0), (2.6, 0.0)])
    })
}

/// The storm breaks: a big gust.
pub(super) fn storm_break(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 800.0, 1.2);
    render(1.6, |t| {
        filter.run(noise.next(), 600.0 + 900.0 * sine(t, 0.6).abs(), 1.2)
            * segments(t, &[(0.0, 0.0), (0.15, 1.0), (1.0, 0.7), (1.6, 0.0)])
    })
}

/// Crystal grows back: a soft creak and a tinkle.
pub(super) fn crystal_grow(rng: &mut Rng) -> Vec<f32> {
    let mut creak = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut out = render(0.4, |t| {
        creak.next(300.0 + 200.0 * (t / 0.4)) * 0.2 * perc(t, 0.05, 0.3)
    });
    let tink = bell(rng.range(2_000.0, 2_600.0), 3.1, 0.6, 0.3);
    mix_into(&mut out, &tink, 0.25, 0.4);
    out
}

/// A struck chime drifter rings: a deep, pure bell with a long tail.
pub(super) fn drifter_chime(rng: &mut Rng) -> Vec<f32> {
    let base = rng.range(390.0, 400.0);
    let mut fm = Fm::default();
    let mut out = render(1.75, |t| {
        fm.next(base, 1.0, 0.5 * (-t * 2.0).exp()) * perc(t, 0.002, 1.5)
    });
    let overtone = bell(base * 2.0, 3.5, 0.4, 1.2);
    mix_into(&mut out, &overtone, 0.0, 0.3);
    out
}

/// A dune wyrm's ripple: the sand hissing and the ground groaning before it erupts.
pub(super) fn wyrm_ripple(rng: &mut Rng) -> Vec<f32> {
    let mut groan = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 200.0, 1.4);
    let mut out = render(1.1, |t| {
        filter.run(groan.next(55.0 + 20.0 * (t / 1.1)), 180.0, 1.4)
            * segments(t, &[(0.0, 0.0), (0.3, 1.0), (1.1, 0.0)])
    });
    let hiss = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_600.0,
        1.0,
        0.9,
    );
    mix_into(&mut out, &hiss, 0.05, 0.4);
    out
}
