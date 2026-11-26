//! Space's music and ambience (Wave 13 art-audio, sound names `aa_*`).
//!
//! * **Stations**: each owner's own music while a rig is docked. The human hub plays a slow
//!   lounge of soft electric-piano sevenths over the station's hum; an alien-run station plays
//!   its people's signature (the same voice as their speech): the Vessi's hooted fifths sung as
//!   a duet over a breathing pad, the Tokkari's wooden clacker rhythm over a low hum, the
//!   Siphra's glass chimes ringing against each other.
//! * **Ambience by star class**: a warm low drone near yellow, red and binary suns; a high
//!   shimmer near blue giants and white dwarfs; a hollow, slowly breathing choir inside a
//!   nebula.
//! * **The Star Drive**: cruise's airy engine whine (the director raises its pitch with
//!   speed), warp's tunnel roar, the drive engaging (a spool rising into a boom) and dropping
//!   out at a star (a falling rush and a bell).
//! * **Combat music per faction** (the corsairs keep the hostiles track's pulse): the swarm's
//!   skittering, buzzing hunt and the machine fleet's cold square-wave march.
//! * **Docking**: the clamps' clunk and the hub's welcome chime.
//!
//! Loop drones and pulses complete whole cycles in a loop (multiples of 0.25 Hz); notes and
//! hits go through `loop_note`, so they ring across the seam.

#![allow(
    clippy::similar_names,
    reason = "oscillators and filters named for their voices"
)]

use super::{
    FilterMode, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, brass, burst, glide,
    loop_length, loop_note, mix_into, perc, render, saturate, segments, sine, thump,
};

/// A soft electric-piano note: an FM tine over a sine body, decaying slowly.
fn tine(rng: &mut Rng, frequency: f32, time: f32) -> Vec<f32> {
    let mut body = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut fm = super::Fm::default();
    render(time, |t| {
        let strike = super::decay(t, time * 0.18);
        (body.next(frequency) * 0.7 + fm.next(frequency, 1.0, 1.4 * strike) * 0.35)
            * perc(t, 0.004, time * 0.8)
    })
}

/// The human hub's lounge (docked at Halcyon Dock and every human station).
pub(super) fn station_human_loop(rng: &mut Rng) -> Vec<f32> {
    let mut hum = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut fan = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut fan_band = Svf::new(FilterMode::BandPass, 420.0, 0.7);
    let mut out = render(loop_length(), |t| {
        // The station's 110 Hz hum and its air handlers, breathing twice a loop.
        let breathe = 0.8 + 0.2 * sine(t, 0.5);
        hum.next(110.0) * 0.05 + fan_band.process(fan.next()) * 0.05 * breathe
    });
    // Four sevenths a loop: Fmaj7, Em7, Dm7, Cmaj7 (warm, unhurried).
    let chords: [[f32; 4]; 4] = [
        [174.61, 220.0, 261.63, 329.63],
        [164.81, 196.0, 246.94, 293.66],
        [146.83, 174.61, 220.0, 261.63],
        [130.81, 164.81, 196.0, 246.94],
    ];
    for (bar, chord) in chords.iter().enumerate() {
        let start = bar as f32;
        for (voice, &note) in chord.iter().enumerate() {
            let roll = voice as f32 * 0.03;
            loop_note(&mut out, &tine(rng, note, 1.6), start + roll, 0.13);
        }
        // A bass note on the bar and a soft answer on the off-beat.
        loop_note(&mut out, &tine(rng, chord[0] * 0.5, 1.2), start, 0.2);
        loop_note(
            &mut out,
            &tine(rng, chord[2] * 2.0, 0.5),
            start + 0.75,
            0.06,
        );
        // A brushed hat on the back beat.
        let brush = burst(
            rng.next_u64(),
            NoiseColor::Pink,
            FilterMode::HighPass,
            5_000.0,
            0.7,
            0.09,
        );
        loop_note(&mut out, &brush, start + 0.5, 0.05);
    }
    out
}

/// A hooted Vessi note, bending up into its pitch with a breath.
fn hoot(rng: &mut Rng, frequency: f32, time: f32) -> Vec<f32> {
    let mut tone = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut breath = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, frequency * 2.0, 2.5);
    let rate = rng.range(4.5, 5.5);
    render(time, |t| {
        let bend = 0.94 + 0.06 * (t / 0.15).min(1.0);
        let vibrato = 1.0 + 0.01 * sine(t, rate) * (t / 0.3).min(1.0);
        let shape = segments(
            t,
            &[(0.0, 0.0), (0.08, 1.0), (time * 0.7, 0.7), (time, 0.0)],
        );
        (tone.next(frequency * bend * vibrato) * 0.8 + band.process(breath.next()) * 0.25) * shape
    })
}

/// The Vessi's signature: a duet of hooted fifths over a breathing pad.
pub(super) fn station_vessi_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pad = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut fifth = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut out = render(loop_length(), |t| {
        let swell = 0.7 + 0.3 * sine(t, 0.25);
        (pad.next(147.0) * 0.5 + fifth.next(220.5) * 0.3) * swell * 0.12
    });
    // Call and answer, a fifth apart (D major pentatonic).
    let line = [
        (0.0_f32, 293.66_f32, 0.9_f32),
        (0.5, 440.0, 0.8),
        (1.25, 329.63, 0.6),
        (1.75, 493.88, 0.9),
        (2.5, 369.99, 0.7),
        (3.0, 554.37, 0.6),
        (3.5, 440.0, 0.5),
    ];
    for (start, note, length) in line {
        loop_note(&mut out, &hoot(rng, note, length), start, 0.22);
    }
    out
}

/// A Tokkari clack: a filtered tick on a wooden body.
fn clack(rng: &mut Rng, pitch: f32, time: f32) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut body = Svf::new(FilterMode::BandPass, pitch, 7.0);
    let mut knock = Osc::new(Wave::Sine).with_phase(rng.unit());
    render(time, |t| {
        (body.process(noise.next()) + knock.next(pitch * 0.4) * 0.5) * perc(t, 0.000_5, time * 0.7)
    })
}

/// The Tokkari's signature: a busy wooden clacker rhythm over a low hum.
pub(super) fn station_tokkari_loop(rng: &mut Rng) -> Vec<f32> {
    let mut hum = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 300.0, 0.9);
    let mut out = render(loop_length(), |t| {
        filter.run(hum.next(98.0), 260.0 + 80.0 * sine(t, 0.5), 0.9) * 0.1
    });
    // Sixteen steps: a clave pattern of high clacks, low knocks on the beat.
    let high = [0_usize, 3, 6, 10, 12];
    for step in 0..16_usize {
        let start = step as f32 * LOOP_SECONDS / 16.0;
        if step % 4 == 0 {
            loop_note(&mut out, &clack(rng, 420.0, 0.12), start, 0.4);
        }
        if high.contains(&step) {
            loop_note(&mut out, &clack(rng, 1_300.0, 0.06), start, 0.3);
        } else if step % 2 == 1 {
            loop_note(&mut out, &clack(rng, 2_100.0, 0.03), start, 0.1);
        }
    }
    // A marimba-ish answer in the second half.
    for (start, note) in [
        (2.0_f32, 392.0_f32),
        (2.25, 440.0),
        (2.5, 523.25),
        (3.25, 392.0),
    ] {
        loop_note(&mut out, &bell(note, 4.0, 0.6, 0.3), start, 0.16);
    }
    out
}

/// The Siphra's signature: glass chimes ringing and beating against each other.
pub(super) fn station_siphra_loop(rng: &mut Rng) -> Vec<f32> {
    let mut shimmer = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut beat = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut out = render(loop_length(), |t| {
        // Two high sines a quarter hertz apart: a slow, glassy beating.
        (shimmer.next(880.0) + beat.next(880.25)) * 0.03 + sine(t, 220.0) * 0.04
    });
    let chimes = [
        (0.0_f32, 1_046.5_f32),
        (0.4, 1_396.9),
        (1.1, 1_174.7),
        (1.6, 1_568.0),
        (2.3, 1_046.5),
        (2.8, 1_318.5),
        (3.4, 1_760.0),
    ];
    for (start, note) in chimes {
        loop_note(&mut out, &bell(note, 2.76, 1.8, 1.4), start, 0.14);
        loop_note(
            &mut out,
            &bell(note * 1.003, 3.1, 1.0, 1.0),
            start + 0.02,
            0.06,
        );
    }
    out
}

/// Deep space near a warm sun (yellow, red, binary): a slow low drone with a soft fifth.
pub(super) fn space_warm_loop(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut fifth = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 400.0, 0.8);
    let mut wind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut wind_band = Svf::new(FilterMode::BandPass, 600.0, 0.6);
    render(loop_length(), |t| {
        let swell = 0.75 + 0.25 * sine(t, 0.25);
        let drone = filter.run(
            low.next(82.5) * 0.6 + fifth.next(123.75) * 0.4,
            320.0 + 160.0 * sine(t, 0.5),
            0.8,
        );
        let solar = wind_band.run(wind.next(), 600.0 + 250.0 * sine(t, 0.25), 0.6) * 0.25;
        (drone + solar) * swell * 0.5
    })
}

/// Deep space near a bright star (a blue giant, a white dwarf): a high, crystalline shimmer.
pub(super) fn space_bright_loop(rng: &mut Rng) -> Vec<f32> {
    let mut a = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut b = Osc::new(Wave::Sine).with_phase(rng.unit());
    let mut c = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut hiss = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hiss_band = Svf::new(FilterMode::BandPass, 5_000.0, 1.2);
    let mut out = render(loop_length(), |t| {
        let swell = 0.7 + 0.3 * sine(t, 0.5);
        (a.next(660.0) * 0.3 + b.next(990.25) * 0.2 + c.next(330.0) * 0.25) * swell * 0.4
            + hiss_band.process(hiss.next()) * 0.06
    });
    for (start, note) in [(0.5_f32, 1_980.0_f32), (2.0, 2_640.0), (3.25, 2_200.0)] {
        loop_note(&mut out, &bell(note, 1.41, 0.8, 1.2), start, 0.05);
    }
    out
}

/// Inside a nebula: a hollow, breathing choir of formant-filtered saws, slowly drifting.
pub(super) fn space_nebula_loop(rng: &mut Rng) -> Vec<f32> {
    let mut voices = [
        Osc::new(Wave::Saw).with_phase(rng.unit()),
        Osc::new(Wave::Saw).with_phase(rng.unit()),
        Osc::new(Wave::Saw).with_phase(rng.unit()),
    ];
    let mut first = Svf::new(FilterMode::BandPass, 500.0, 3.0);
    let mut second = Svf::new(FilterMode::BandPass, 1_100.0, 3.0);
    render(loop_length(), |t| {
        let raw = voices[0].next(110.0) + voices[1].next(130.75) + voices[2].next(165.0);
        // The vowel drifts "oo" to "ah" and back over the loop.
        let open = 0.5 + 0.5 * sine(t, 0.25);
        let a = first.run(raw, 400.0 + 350.0 * open, 3.0);
        let b = second.run(raw, 900.0 + 400.0 * open, 3.0);
        let breathe = 0.7 + 0.3 * sine(t, 0.5);
        (a * 0.5 + b * 0.3) * breathe * 0.3
    })
}

/// Cruise: the drive's airy whine over a soft rush (the director raises its pitch with speed).
pub(super) fn cruise_loop(rng: &mut Rng) -> Vec<f32> {
    let mut whine = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut whine_filter = Svf::new(FilterMode::BandPass, 880.0, 4.0);
    let mut rush = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut rush_filter = Svf::new(FilterMode::BandPass, 1_400.0, 0.5);
    let mut low = Osc::new(Wave::Sine).with_phase(rng.unit());
    render(loop_length(), |t| {
        let flutter = 1.0 + 0.004 * sine(t, 6.0);
        let tone = whine_filter.process(whine.next(220.0 * flutter)) * 0.5;
        let air = rush_filter.run(rush.next(), 1_400.0 + 300.0 * sine(t, 0.5), 0.5) * 0.4;
        (tone + air + low.next(110.0) * 0.12) * 0.5
    })
}

/// Warp: the tunnel's roar, a rising shimmer on top.
pub(super) fn warp_loop(rng: &mut Rng) -> Vec<f32> {
    let mut roar = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut roar_filter = Svf::new(FilterMode::LowPass, 700.0, 0.7);
    let mut sheen = Noise::new(NoiseColor::White, rng.next_u64());
    let mut sheen_filter = Svf::new(FilterMode::BandPass, 3_000.0, 1.5);
    let mut hum = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut hum_filter = Svf::new(FilterMode::LowPass, 500.0, 1.0);
    render(loop_length(), |t| {
        let pulse = 0.8 + 0.2 * sine(t, 1.0);
        let body = roar_filter.run(roar.next(), 600.0 + 200.0 * sine(t, 0.5), 0.7) * 0.6;
        let shine = sheen_filter.run(sheen.next(), 2_600.0 + 900.0 * sine(t, 0.25), 1.5) * 0.15;
        let engine = hum_filter.process(hum.next(146.75)) * 0.25;
        saturate((body + shine + engine) * pulse, 1.3) * 0.6
    })
}

/// The Star Drive engaging: a spool whine rising two octaves into a deep boom and a rush.
pub(super) fn warp_engage(rng: &mut Rng) -> Vec<f32> {
    let seconds = 2.2;
    let mut out = vec![0.0; super::samples_for(seconds)];
    let mut spool = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut spool_filter = Svf::new(FilterMode::LowPass, 800.0, 2.0);
    let rise = render(0.9, |t| {
        let frequency = 110.0 * 4.0_f32.powf(t / 0.9);
        spool_filter.run(spool.next(frequency), frequency * 4.0, 2.0)
            * segments(t, &[(0.0, 0.0), (0.6, 0.8), (0.9, 1.0)])
    });
    mix_into(&mut out, &rise, 0.0, 0.45);
    mix_into(&mut out, &thump(120.0, 45.0, 0.9, 2.2), 0.9, 0.8);
    let mut rush = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut rush_filter = Svf::new(FilterMode::LowPass, 3_000.0, 0.6);
    let tail = render(1.3, |t| {
        rush_filter.run(rush.next(), 3_000.0 - 2_000.0 * (t / 1.3), 0.6)
            * segments(t, &[(0.0, 1.0), (1.3, 0.0)])
    });
    mix_into(&mut out, &tail, 0.9, 0.5);
    out
}

/// Dropping out of warp at a star: a falling rush and an arrival bell.
pub(super) fn warp_arrive(rng: &mut Rng) -> Vec<f32> {
    let seconds = 1.8;
    let mut out = vec![0.0; super::samples_for(seconds)];
    let mut rush = Noise::new(NoiseColor::White, rng.next_u64());
    let mut sweep = Svf::new(FilterMode::BandPass, 4_000.0, 0.8);
    let fall = render(0.9, |t| {
        sweep.run(rush.next(), glide(t, 4_000.0, 300.0, 0.25), 0.8)
            * segments(t, &[(0.0, 0.0), (0.05, 1.0), (0.9, 0.0)])
    });
    mix_into(&mut out, &fall, 0.0, 0.6);
    mix_into(&mut out, &bell(523.25, 1.5, 1.6, 1.2), 0.45, 0.35);
    mix_into(&mut out, &bell(783.99, 1.5, 1.2, 1.0), 0.6, 0.25);
    out
}

/// Cruise engaging: a quick upward sweep with a soft thump.
pub(super) fn cruise_engage(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 900.0, 0.8);
    let mut out = render(0.6, |t| {
        let tone = osc.next(glide(t, 220.0, 660.0, 0.2)) * perc(t, 0.01, 0.5) * 0.4;
        let whoosh = band.run(air.next(), 600.0 + 2_400.0 * (t / 0.6), 0.8) * perc(t, 0.05, 0.4);
        tone + whoosh * 0.5
    });
    mix_into(&mut out, &thump(140.0, 70.0, 0.15, 1.3), 0.0, 0.35);
    out
}

/// Docked: the clamps closing and the hub's welcome chime.
pub(super) fn dock_chime(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; super::samples_for(1.4)];
    mix_into(&mut out, &thump(160.0, 60.0, 0.2, 1.8), 0.0, 0.5);
    let clamp = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        2_400.0,
        3.0,
        0.06,
    );
    mix_into(&mut out, &clamp, 0.02, 0.4);
    mix_into(&mut out, &bell(659.25, 2.0, 1.0, 0.8), 0.35, 0.3);
    mix_into(&mut out, &bell(987.77, 2.0, 0.8, 0.7), 0.55, 0.25);
    out
}

/// The swarm's hunt: skittering clicks, a buzzing wing drone and a pulsing low bass.
pub(super) fn combat_swarm_loop(rng: &mut Rng) -> Vec<f32> {
    let mut wings = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut wing_filter = Svf::new(FilterMode::BandPass, 600.0, 2.0);
    let mut out = render(loop_length(), |t| {
        let flutter = 0.6 + 0.4 * sine(t, 12.0).abs();
        wing_filter.run(wings.next(196.0), 600.0 + 200.0 * sine(t, 0.5), 2.0) * flutter * 0.18
    });
    for step in 0..16_usize {
        let start = step as f32 * LOOP_SECONDS / 16.0;
        if step % 4 == 0 || step % 4 == 3 {
            loop_note(&mut out, &thump(98.0 * 2.0, 98.0, 0.2, 1.5), start, 0.45);
        }
        let skitter = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(2_500.0, 4_500.0),
            3.0,
            0.03,
        );
        loop_note(&mut out, &skitter, start + LOOP_SECONDS / 32.0, 0.18);
    }
    for (start, note) in [
        (0.75_f32, 466.16_f32),
        (1.75, 440.0),
        (2.75, 523.25),
        (3.5, 493.88),
    ] {
        let mut buzz = Osc::new(Wave::Saw).with_phase(rng.unit());
        let mut filter = Svf::new(FilterMode::LowPass, 1_200.0, 2.0);
        let stab = render(0.25, |t| {
            filter.process(buzz.next(note)) * perc(t, 0.004, 0.2) * 0.5
        });
        loop_note(&mut out, &stab, start, 0.2);
    }
    out
}

/// The machine fleet's march: cold square arpeggios over a steady electronic kick.
pub(super) fn combat_machine_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pad = Osc::new(Wave::Square).with_phase(rng.unit());
    let mut pad_filter = Svf::new(FilterMode::LowPass, 500.0, 1.2);
    let mut out = render(loop_length(), |t| {
        pad_filter.run(pad.next(110.0), 400.0 + 150.0 * sine(t, 0.25), 1.2) * 0.1
    });
    let arpeggio = [
        220.0_f32, 261.63, 329.63, 440.0, 329.63, 261.63, 220.0, 196.0,
    ];
    for step in 0..16_usize {
        let start = step as f32 * LOOP_SECONDS / 16.0;
        if step % 4 == 0 {
            loop_note(&mut out, &thump(150.0, 50.0, 0.25, 2.0), start, 0.5);
        }
        if step % 4 == 2 {
            let snap = burst(
                rng.next_u64(),
                NoiseColor::White,
                FilterMode::HighPass,
                3_000.0,
                0.7,
                0.05,
            );
            loop_note(&mut out, &snap, start, 0.2);
        }
        let note = arpeggio[step % arpeggio.len()];
        let mut blip = Osc::new(Wave::Square).with_phase(rng.unit());
        let tone = render(0.18, |t| blip.next(note * 2.0) * perc(t, 0.002, 0.12) * 0.4);
        loop_note(&mut out, &tone, start, 0.12);
    }
    loop_note(&mut out, &brass(rng, 110.0, 1.6, 0.4), 0.0, 0.12);
    loop_note(&mut out, &brass(rng, 103.83, 1.6, 0.4), 2.0, 0.12);
    out
}
