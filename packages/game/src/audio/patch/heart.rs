//! Descent VIII, the Heart's sounds (Wave 10): a music bed for each of its four strata, the
//! Heart's fight theme and the ending's theme (the credits), four ambience beds, and the
//! one-shots of its set pieces (the cable lighting, the drive's pulses, ignition and breath,
//! coils breaking, dark matter, the rails, the wells and the metal sea).
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop; scattered
//! events spread evenly over the render.

#![allow(
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::similar_names,
    reason = "presentation code: sound math reads top to bottom"
)]

use super::{
    FilterMode, Fm, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, brass, burst,
    loop_events, loop_length, loop_note, mix_into, perc, render, saturate, scatter, scatter_grains,
    segments, sine, thump,
};

/// A note that swells in and cuts (a bell played backwards: dark matter falls up).
fn reversed(frequency: f32, length: f32, index: f32) -> Vec<f32> {
    let mut fm = Fm::default();
    render(length, |t| {
        let rise = (t / length).powi(2);
        fm.next(frequency, 2.0, index * rise)
            * rise
            * segments(t, &[(length - 0.03, 1.0), (length, 0.0)])
    })
}

/// Dark Matter Veins music: a weightless waltz in D minor: a low pulse on one, two soft pads
/// on two and three, and a celesta line of reversed notes that rise where they should fall.
pub(super) fn dark_matter_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pad = Osc::new(Wave::Triangle);
    let mut pad_high = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        (pad.next(73.5) * 0.35 + pad_high.next(110.0) * 0.2) * (0.7 + 0.3 * sine(t, 0.25)) * 0.28
    });
    let beat = LOOP_SECONDS / 12.0;
    for bar in 0..4_usize {
        #[allow(clippy::cast_precision_loss, reason = "four bars")]
        let start = bar as f32 * beat * 3.0;
        let low = thump(73.5, 55.0, 0.3, 1.2);
        loop_note(&mut out, &low, start, 0.16);
        for offbeat in 1..=2_usize {
            #[allow(clippy::cast_precision_loss, reason = "two beats")]
            let at = start + offbeat as f32 * beat;
            let chord = bell(
                220.0 + 73.25 * if bar % 2 == 0 { 1.0 } else { 0.5 },
                1.0,
                0.4,
                0.4,
            );
            loop_note(&mut out, &chord, at, 0.05);
        }
    }
    let line = [587.25_f32, 440.0, 523.25, 349.25, 392.0, 440.0];
    for (index, note) in line.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "six notes")]
        let start = index as f32 * beat * 2.0;
        let swell = reversed(note, beat * 1.9, 1.1);
        loop_note(&mut out, &swell, start, 0.1);
    }
    let _ = rng.next_u64();
    out
}

/// Singularity Fields music: a bending drone: two detuned saws whose pitch slides down toward
/// a well and back, a slow bell that stretches as it rings, distant and cold.
pub(super) fn singularity_loop(rng: &mut Rng) -> Vec<f32> {
    let mut a = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut b = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 500.0, 1.2);
    let mut out = render(loop_length(), |t| {
        // One slow bend down and back each loop.
        let bend = 1.0 - 0.06 * (0.5 - 0.5 * sine(t, 0.25));
        let body = a.next(55.0 * bend) + b.next(82.5 * bend);
        filter.run(body * 0.2, 380.0 + 260.0 * (0.5 + 0.5 * sine(t, 0.5)), 1.2)
    });
    let step = LOOP_SECONDS / 4.0;
    for (index, note) in [329.5_f32, 311.0, 277.25, 246.75].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "four steps")]
        let start = index as f32 * step;
        let mut fm = Fm::default();
        let length = step * 0.95;
        let stretched = render(length, |t| {
            // The pitch sags as the note rings: light bending.
            let sag = 1.0 - 0.03 * (t / length);
            fm.next(note * sag, 1.41, 1.2 * (1.0 - t / length)) * perc(t, 0.01, length * 0.7)
        });
        loop_note(&mut out, &stretched, start, 0.12);
    }
    out
}

/// Core Ocean music: a rolling barcarolle in A minor over the rails' electric hum: a lilting
/// six of low plucks, a silvery melody and the sea's slow swell.
pub(super) fn core_ocean_loop(rng: &mut Rng) -> Vec<f32> {
    let mut hum = Osc::new(Wave::Square);
    let mut hum_filter = Svf::new(FilterMode::LowPass, 300.0, 1.0);
    let mut swell = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut swell_filter = Svf::new(FilterMode::LowPass, 180.0, 0.8);
    let mut out = render(loop_length(), |t| {
        hum_filter.run(hum.next(60.0), 260.0, 1.0) * 0.05
            + swell_filter.process(swell.next()) * 0.35 * (0.6 + 0.4 * sine(t, 0.25))
    });
    let step = LOOP_SECONDS / 12.0;
    let bass = [110.0_f32, 164.75, 220.0, 130.75, 196.0, 261.75];
    for bar in 0..2_usize {
        for (index, note) in bass.into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "twelve steps")]
            let start = (bar * 6 + index) as f32 * step;
            let pluck = bell(note * if bar == 1 { 0.89 } else { 1.0 }, 1.0, 0.8, 0.5);
            loop_note(&mut out, &pluck, start, 0.1);
        }
    }
    let melody = [659.25_f32, 587.25, 523.25, 493.75, 523.25, 440.0];
    for (index, note) in melody.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "six notes")]
        let start = index as f32 * step * 2.0;
        let voice = bell(note, 3.0, 0.6, step * 2.2);
        loop_note(&mut out, &voice, start, 0.07);
    }
    out
}

/// The Heart's music: a slow, enormous heartbeat under rising choir pads in C, the drive
/// breathing in and out, warm and awed.
pub(super) fn heart_loop(rng: &mut Rng) -> Vec<f32> {
    let chord = [130.75_f32, 196.0, 261.75, 329.5];
    let mut voices: Vec<(Osc, Osc)> = chord
        .iter()
        .map(|_| {
            (
                Osc::new(Wave::Saw).with_phase(rng.unit()),
                Osc::new(Wave::Saw).with_phase(rng.unit()),
            )
        })
        .collect();
    let mut filter = Svf::new(FilterMode::LowPass, 600.0, 0.8);
    let mut out = render(loop_length(), |t| {
        // One breath per loop: in for half, out for half.
        let breath = 0.5 - 0.5 * sine(t, 0.25);
        let body: f32 = voices
            .iter_mut()
            .zip(chord)
            .map(|((a, b), note)| a.next(note) + b.next(note + 0.25))
            .sum::<f32>()
            / 8.0;
        filter.run(body, 500.0 + 900.0 * breath, 0.8) * (0.4 + 0.4 * breath) * 0.45
    });
    // The heartbeat: lub-dub twice a loop.
    for beat in 0..2_usize {
        #[allow(clippy::cast_precision_loss, reason = "two beats")]
        let start = beat as f32 * LOOP_SECONDS * 0.5;
        let lub = thump(65.0, 40.0, 0.35, 1.6);
        loop_note(&mut out, &lub, start, 0.26);
        let dub = thump(80.0, 48.0, 0.28, 1.6);
        loop_note(&mut out, &dub, start + 0.32, 0.18);
    }
    out
}

/// The Heart's fight: the drive waking pulse by pulse: a driving eighth-note bass in C minor, a
/// brass call that climbs a step each phrase, the heartbeat as the kick, choir stabs.
pub(super) fn heart_fight_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut drone_filter = Svf::new(FilterMode::LowPass, 500.0, 1.2);
    let mut out = render(loop_length(), |t| {
        drone_filter.run(drone.next(65.5), 380.0 + 200.0 * sine(t, 0.5), 1.2) * 0.16
    });
    let step = LOOP_SECONDS / 16.0;
    let bass = [65.5_f32, 65.5, 98.0, 65.5, 77.75, 65.5, 98.0, 87.25];
    for index in 0..16_usize {
        #[allow(clippy::cast_precision_loss, reason = "sixteen steps")]
        let start = index as f32 * step;
        let note = bass[index % bass.len()];
        let pluck = thump(note * 2.0, note, 0.12, 2.2);
        loop_note(&mut out, &pluck, start, 0.14);
        if index % 4 == 0 {
            let kick = thump(70.0, 38.0, 0.25, 2.0);
            loop_note(&mut out, &kick, start, 0.24);
        }
    }
    let call = [261.75_f32, 311.0, 349.25, 392.0];
    for (index, note) in call.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "four phrases")]
        let start = index as f32 * step * 4.0;
        let horn = brass(rng, note, step * 2.6, 0.55);
        loop_note(&mut out, &horn, start, 0.12);
        let stab = bell(note * 2.0, 1.0, 0.7, 0.3);
        loop_note(&mut out, &stab, start + step * 3.0, 0.05);
    }
    out
}

/// The ending's theme (the credits): the Heart's chord resolved into a bright major, a slow
/// hymn-like brass melody climbing to the Moon, bells on top, no beat.
pub(super) fn ending_loop(rng: &mut Rng) -> Vec<f32> {
    let chord = [130.75_f32, 196.0, 261.75, 329.5, 392.0];
    let mut voices: Vec<Osc> = chord
        .iter()
        .map(|_| Osc::new(Wave::Triangle).with_phase(rng.unit()))
        .collect();
    let mut shimmer = Fm::default();
    let mut out = render(loop_length(), |t| {
        let body: f32 = voices
            .iter_mut()
            .zip(chord)
            .map(|(osc, note)| osc.next(note))
            .sum::<f32>()
            / 5.0;
        body * 0.28 * (0.8 + 0.2 * sine(t, 0.25)) + shimmer.next(1_046.5, 2.0, 0.8) * 0.02
    });
    let step = LOOP_SECONDS / 4.0;
    for (index, note) in [523.25_f32, 587.25, 659.25, 784.0].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "four steps")]
        let start = index as f32 * step;
        let horn = brass(rng, note * 0.5, step * 0.9, 0.3);
        loop_note(&mut out, &horn, start, 0.12);
        let chime = bell(note * 2.0, 3.0, 0.8, 1.2);
        loop_note(&mut out, &chime, start + step * 0.5, 0.045);
    }
    out
}

/// Dark Matter Veins' ambience: a deep weightless hum and grit falling upward (rising ticks).
pub(super) fn void_hum_loop(rng: &mut Rng) -> Vec<f32> {
    let mut hum = Osc::new(Wave::Sine);
    let mut hum_high = Osc::new(Wave::Sine);
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut air_filter = Svf::new(FilterMode::BandPass, 900.0, 0.7);
    let mut out = render(loop_length(), |t| {
        hum.next(48.0) * 0.25
            + hum_high.next(96.25) * 0.06 * (0.6 + 0.4 * sine(t, 0.5))
            + air_filter.process(air.next()) * 0.04
    });
    for time in scatter(rng, loop_events(2.0), 0.0, loop_length() - 0.2, 1.0) {
        let start = rng.range(1_800.0, 2_600.0);
        let mut tick = Osc::new(Wave::Sine);
        let grain = render(0.12, |t| {
            tick.next(start * (1.0 + 2.0 * t)) * perc(t, 0.002, 0.06)
        });
        mix_into(&mut out, &grain, time, 0.05);
    }
    out
}

/// The Singularity Fields' ambience: stretched rock groaning and the wells' low pull.
pub(super) fn gravity_loop(rng: &mut Rng) -> Vec<f32> {
    let mut groan = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut groan_filter = Svf::new(FilterMode::BandPass, 140.0, 2.0);
    let mut pull = Osc::new(Wave::Sine);
    render(loop_length(), |t| {
        let sweep = 0.5 + 0.5 * sine(t, 0.25);
        groan_filter.run(groan.next(), 90.0 + 120.0 * sweep, 2.0) * 0.5
            + pull.next(44.0) * 0.18 * (0.5 + 0.5 * sine(t, 0.5))
    })
}

/// The Core Ocean's ambience: heavy slow waves of metal and the rails' hum.
pub(super) fn metal_sea_loop(rng: &mut Rng) -> Vec<f32> {
    let mut wave = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut wave_filter = Svf::new(FilterMode::LowPass, 260.0, 0.9);
    let mut hum = Osc::new(Wave::Square);
    let mut hum_filter = Svf::new(FilterMode::LowPass, 240.0, 1.0);
    let mut out = render(loop_length(), |t| {
        let swell = 0.5 - 0.5 * sine(t, 0.5);
        wave_filter.process(wave.next()) * (0.25 + 0.45 * swell)
            + hum_filter.run(hum.next(60.0), 200.0, 1.0) * 0.03
    });
    scatter_grains(
        rng,
        &mut out,
        loop_events(1.5),
        (0.0, loop_length() - 0.05),
        (700.0, 1_400.0),
        0.08,
        false,
    );
    out
}

/// The Heart's ambience: the Star Drive's vast slow breath and a crackle of light.
pub(super) fn star_drive_loop(rng: &mut Rng) -> Vec<f32> {
    let mut breath = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut breath_filter = Svf::new(FilterMode::LowPass, 400.0, 0.7);
    let mut core = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        let inhale = 0.5 - 0.5 * sine(t, 0.25);
        breath_filter.run(breath.next(), 200.0 + 700.0 * inhale, 0.7) * (0.15 + 0.35 * inhale)
            + core.next(55.0) * 0.12
    });
    scatter_grains(
        rng,
        &mut out,
        loop_events(3.0),
        (0.0, loop_length() - 0.05),
        (3_000.0, 6_000.0),
        0.07,
        false,
    );
    out
}

/// The cable lighting up (the ending's sting): a deep swell rising through every register
/// into a huge major chord with a shimmer that climbs and climbs, held, then let go.
pub(super) fn cable_light(rng: &mut Rng) -> Vec<f32> {
    let seconds = 3.9;
    let chord = [65.5_f32, 98.0, 130.75, 196.0, 261.75, 329.5, 392.0, 523.25];
    let mut voices: Vec<(Osc, Osc)> = chord
        .iter()
        .map(|_| {
            (
                Osc::new(Wave::Saw).with_phase(rng.unit()),
                Osc::new(Wave::Saw).with_phase(rng.unit()),
            )
        })
        .collect();
    let mut filter = Svf::new(FilterMode::LowPass, 300.0, 0.9);
    let mut shimmer = Fm::default();
    render(seconds, |t| {
        let swell = segments(t, &[(0.0, 0.0), (1.6, 1.0), (3.0, 0.9), (seconds, 0.0)]);
        let body: f32 = voices
            .iter_mut()
            .zip(chord)
            .map(|((a, b), note)| a.next(note) + b.next(note * 1.005))
            .sum::<f32>()
            / 16.0;
        let open = 4_000.0f32.mul_add(swell, 250.0);
        // The shimmer climbs with the light up the cable.
        let climb = 880.0 * (1.0 + t / seconds * 1.5);
        filter.run(body, open, 0.9) * swell + shimmer.next(climb, 2.0, 1.0) * 0.05 * swell
    })
}

/// A gravity pulse's tell: a vast inhale, air rushing toward the core.
pub(super) fn heart_inhale(rng: &mut Rng) -> Vec<f32> {
    let seconds = 1.8;
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 300.0, 0.9);
    let mut low = Osc::new(Wave::Sine);
    render(seconds, |t| {
        let rise = (t / seconds).powi(2);
        band.run(air.next(), 250.0 + 1_500.0 * rise, 0.9) * rise
            + low.next(40.0 + 30.0 * rise)
                * 0.3
                * rise
                * segments(t, &[(0.0, 0.0), (seconds - 0.08, 1.0), (seconds, 0.0)])
    })
}

/// A gravity pulse pulling: a deep thrum that bends down.
pub(super) fn heart_pull(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 200.0, 1.4);
    let mut out = render(1.9, |t| {
        let pitch = 55.0 * (1.0 - 0.25 * (t / 1.9));
        saturate(filter.run(low.next(pitch), 220.0, 1.4), 1.5)
            * segments(t, &[(0.0, 0.0), (0.1, 1.0), (1.5, 0.8), (1.9, 0.0)])
    });
    let hit = thump(80.0, 35.0, 0.6, 2.0);
    mix_into(&mut out, &hit, 0.0, 0.6);
    out
}

/// Ignition (and the collapse, and the Heart stirring): a roar of fire and a rising whine.
pub(super) fn heart_ignite(rng: &mut Rng) -> Vec<f32> {
    let mut roar = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut roar_filter = Svf::new(FilterMode::LowPass, 400.0, 0.8);
    let mut whine = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut whine_filter = Svf::new(FilterMode::BandPass, 800.0, 3.0);
    render(2.4, |t| {
        let level = segments(t, &[(0.0, 0.0), (0.1, 1.0), (1.4, 0.7), (2.4, 0.0)]);
        let pitch = 220.0 + 660.0 * (t / 2.4);
        (roar_filter.run(roar.next(), 300.0 + 2_000.0 * level, 0.8) * 0.8
            + whine_filter.run(whine.next(pitch), pitch, 3.0) * 0.25)
            * level
    })
}

/// The collapse's exhale: a long falling breath out.
pub(super) fn heart_exhale(rng: &mut Rng) -> Vec<f32> {
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 1_400.0, 0.8);
    render(1.6, |t| {
        band.run(air.next(), 1_600.0 - 1_000.0 * (t / 1.6), 0.8)
            * segments(t, &[(0.0, 0.0), (0.08, 1.0), (1.6, 0.0)])
    })
}

/// A drive coil breaking: a metallic crack and an electric zap.
pub(super) fn coil_break(rng: &mut Rng) -> Vec<f32> {
    let mut out = bell(330.0, 2.76, 2.0, 0.8);
    let crack = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        2_000.0,
        0.8,
        0.12,
    );
    mix_into(&mut out, &crack, 0.0, 0.8);
    let mut zap = Osc::new(Wave::Square);
    let buzz = render(0.3, |t| {
        zap.next(880.0 - 600.0 * (t / 0.3)) * perc(t, 0.002, 0.2)
    });
    mix_into(&mut out, &buzz, 0.02, 0.3);
    out
}

/// Dark matter: an inverted chime: a note that swells in and cuts, climbing a fifth.
pub(super) fn dark_matter_chime(rng: &mut Rng) -> Vec<f32> {
    let base = rng.range(560.0, 620.0);
    let mut out = reversed(base, 0.35, 1.2);
    let high = reversed(base * 1.5, 0.3, 1.0);
    mix_into(&mut out, &high, 0.12, 0.7);
    out
}

/// A booster rail or the magnetic catch: an electric zap and a magnetic clunk.
pub(super) fn rail_zap(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(120.0, 60.0, 0.15, 1.8);
    let mut zap = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::BandPass, 1_200.0, 2.0);
    let buzz = render(0.45, |t| {
        filter.run(
            zap.next(300.0 + 900.0 * (t / 0.45)),
            1_200.0 + 1_600.0 * (t / 0.45),
            2.0,
        ) * perc(t, 0.005, 0.3)
    });
    mix_into(&mut out, &buzz, 0.0, 0.6);
    out
}

/// A gravity well catching a rig (and its horizon crushing): a low groan bending down.
pub(super) fn well_groan(rng: &mut Rng) -> Vec<f32> {
    let mut groan = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 300.0, 1.6);
    render(1.3, |t| {
        let pitch = 70.0 * (1.0 - 0.4 * (t / 1.3));
        filter.run(groan.next(pitch), 260.0, 1.6)
            * segments(t, &[(0.0, 0.0), (0.15, 1.0), (1.0, 0.6), (1.3, 0.0)])
    })
}

/// A mercury eel breaking the metal: a heavy plunge and a ringing ripple.
pub(super) fn metal_splash(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(150.0, 70.0, 0.25, 1.6);
    let slosh = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        900.0,
        0.8,
        0.4,
    );
    mix_into(&mut out, &slosh, 0.0, 0.7);
    let ring = bell(990.0, 3.5, 1.0, 0.7);
    mix_into(&mut out, &ring, 0.05, 0.25);
    out
}
