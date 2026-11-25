//! Descent VII part B's sounds (Wave 10 ancients-b: the Sleeping Garrison, the Seal Gate and
//! the Gatekeeper Colossus): a music bed per stratum (the garrison's martial snare and brass
//! ostinato, the Seal Gate's gilded pads and slow bell), an ambience bed each, the Colossus's
//! theme, and the one-shots of the forts, the defences, the gate and the fight.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop;
//! scattered events spread evenly over the render.

use super::{
    FilterMode, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, brass, burst, clank,
    grains, loop_length, loop_note, mix_into, perc, render, saturate, scatter_grains, segments,
    sine, thump,
};

/// An auto-turret's shot: a tight crack with a metallic tail.
pub(super) fn turret_shot(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        rng.range(2_400.0, 2_900.0),
        1.2,
        0.06,
    );
    mix_into(&mut out, &clank(rng, 1_300.0, 0.12), 0.0, 0.35);
    mix_into(&mut out, &thump(220.0, 90.0, 0.08, 1.6), 0.0, 0.6);
    out
}

/// A shock trap zapping: a crackling electric buzz that snaps off.
pub(super) fn trap_zap(rng: &mut Rng) -> Vec<f32> {
    let mut buzz = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 3_500.0, 2.0);
    render(0.45, |t| {
        let crackle = if noise.next() > 0.6 { 1.0 } else { 0.25 };
        let body = buzz.next(120.0 + 40.0 * sine(t, 60.0)) * 0.5;
        let hiss = band.run(noise.next(), 3_000.0 + 2_000.0 * sine(t, 7.0), 2.0);
        saturate((body + hiss) * crackle, 2.0) * perc(t, 0.002, 0.4)
    })
}

/// A fort's klaxon: two falling horn blasts (a wave's bays light).
pub(super) fn klaxon(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(1.6, |_| 0.0);
    for (start, pitch) in [(0.0_f32, 440.0_f32), (0.7, 392.0)] {
        let mut horn = Osc::new(Wave::Square).with_phase(rng.unit());
        let mut filter = Svf::new(FilterMode::LowPass, 1_400.0, 1.2);
        let blast = render(0.6, |t| {
            let shape = segments(t, &[(0.0, 0.0), (0.04, 1.0), (0.5, 0.8), (0.6, 0.0)]);
            filter.run(horn.next(pitch * (1.0 - 0.06 * t)), 1_600.0, 1.2) * shape * 0.5
        });
        mix_into(&mut out, &blast, start, 0.8);
    }
    out
}

/// A war bay opening: a pneumatic hiss and a heavy latch (the machines climb out).
pub(super) fn bay_open(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::HighPass,
        2_000.0,
        0.7,
        0.5,
    );
    mix_into(&mut out, &clank(rng, 260.0, 0.3), 0.05, 0.7);
    mix_into(&mut out, &thump(110.0, 50.0, 0.25, 1.8), 0.05, 0.7);
    out
}

/// A defence battered down: a metal crash and scattering plates.
pub(super) fn defence_break(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 420.0, 0.5);
    mix_into(&mut out, &thump(160.0, 40.0, 0.35, 2.0), 0.0, 0.8);
    grains(rng, &mut out, 18, (0.0, 0.6), (1_500.0, 6_000.0), 0.3);
    out
}

/// A fist slamming down: a huge low impact, stone cracking under it.
pub(super) fn fist_slam(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(90.0, 28.0, 0.9, 2.4);
    mix_into(
        &mut out,
        &burst(
            rng.next_u64(),
            NoiseColor::Brown,
            FilterMode::LowPass,
            700.0,
            0.8,
            0.8,
        ),
        0.0,
        0.8,
    );
    grains(rng, &mut out, 26, (0.02, 0.9), (400.0, 3_000.0), 0.35);
    out
}

/// The hall collapsing: a long avalanche of stone, a deep groan under it.
pub(super) fn hall_collapse(rng: &mut Rng) -> Vec<f32> {
    let mut groan = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 200.0, 1.4);
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 300.0, 0.7);
    let mut out = render(3.2, |t| {
        let shape = segments(t, &[(0.0, 0.0), (0.3, 1.0), (2.2, 0.8), (3.2, 0.0)]);
        let tone = filter.run(groan.next(42.0 + 6.0 * sine(t, 0.7)), 180.0, 1.4) * 0.4;
        (tone + low.process(rumble.next()) * 0.9) * shape
    });
    grains(rng, &mut out, 60, (0.1, 3.0), (300.0, 4_000.0), 0.4);
    for start in [0.4_f32, 1.1, 1.9] {
        mix_into(&mut out, &thump(70.0, 30.0, 0.5, 2.0), start, 0.6);
    }
    out
}

/// A socket of the Seal Gate lighting: a clear gold bell with a shimmer.
pub(super) fn socket_chime(rng: &mut Rng) -> Vec<f32> {
    let note = rng.range(880.0, 900.0);
    let mut out = bell(note, 2.0, 1.2, 1.4);
    mix_into(&mut out, &bell(note * 1.5, 3.0, 0.6, 1.0), 0.02, 0.4);
    out
}

/// The Seal Gate's door grinding down into the floor: stone on stone, deep and slow.
pub(super) fn door_grind(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 180.0, 1.0);
    let mut hum = Osc::new(Wave::Triangle);
    let mut out = render(2.6, |t| {
        let shape = segments(t, &[(0.0, 0.0), (0.2, 1.0), (2.2, 1.0), (2.6, 0.0)]);
        let judder = 0.7 + 0.3 * sine(t, 9.0);
        (band.run(noise.next(), 160.0 + 40.0 * sine(t, 3.0), 1.0) * judder + hum.next(55.0) * 0.25)
            * shape
    });
    grains(rng, &mut out, 20, (0.0, 2.4), (800.0, 3_000.0), 0.2);
    out
}

/// A tremor: a rolling low quake and a trickle of grit.
pub(super) fn tremor(rng: &mut Rng) -> Vec<f32> {
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 140.0, 0.9);
    // Keep the rumble's weight above the infrasound (brown noise drifts below 30 Hz).
    let mut floor = Svf::new(FilterMode::HighPass, 48.0, 0.7);
    let mut out = render(2.0, |t| {
        let shape = segments(t, &[(0.0, 0.0), (0.4, 1.0), (1.2, 0.7), (2.0, 0.0)]);
        floor.process(low.process(rumble.next())) * shape * (0.8 + 0.2 * sine(t, 5.0))
    });
    grains(rng, &mut out, 24, (0.3, 1.9), (1_000.0, 5_000.0), 0.2);
    out
}

/// Sleeping Garrison music: a martial snare pattern and a low brass ostinato in D minor over
/// a held drone.
pub(super) fn garrison_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Triangle);
    let mut out = render(loop_length(), |t| {
        drone.next(73.5) * 0.22 * (0.8 + 0.2 * sine(t, 0.25))
    });
    // Snare: sixteen steps, accents on the march.
    let step = LOOP_SECONDS / 16.0;
    let pattern = [
        1.0_f32, 0.0, 0.4, 0.0, 0.8, 0.0, 0.4, 0.4, 1.0, 0.0, 0.4, 0.0, 0.8, 0.4, 0.4, 0.6,
    ];
    for (index, accent) in pattern.into_iter().enumerate() {
        if accent <= 0.0 {
            continue;
        }
        let snare = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            1_800.0,
            0.9,
            0.09,
        );
        #[allow(clippy::cast_precision_loss, reason = "sixteen steps")]
        let start = index as f32 * step;
        loop_note(&mut out, &snare, start, 0.22 * accent);
    }
    // The brass ostinato: D, F, A, G, then down.
    let notes = [
        146.83_f32, 174.61, 220.0, 196.0, 174.61, 146.83, 130.81, 146.83,
    ];
    for (index, note) in notes.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "eight notes")]
        let start = index as f32 * LOOP_SECONDS / 8.0;
        loop_note(&mut out, &brass(rng, note, 0.28, 0.4), start, 0.18);
    }
    out
}

/// The Seal Gate music: slow gilded pads in a high major key and a deep bell every bar.
pub(super) fn seal_gate_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pad = Osc::new(Wave::Triangle);
    let mut third = Osc::new(Wave::Triangle);
    let mut fifth = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        let swell = 0.6 + 0.4 * sine(t, 0.25);
        (pad.next(261.5) * 0.35 + third.next(329.75) * 0.25 + fifth.next(392.0) * 0.2) * swell * 0.3
    });
    for start in [0.0_f32, LOOP_SECONDS / 2.0] {
        let toll = bell(98.0 * rng.jitter(0.002), 2.4, 1.6, 2.4);
        loop_note(&mut out, &toll, start, 0.3);
    }
    scatter_grains(
        rng,
        &mut out,
        6,
        (0.0, LOOP_SECONDS),
        (5_000.0, 8_000.0),
        0.03,
        false,
    );
    out
}

/// The Gatekeeper Colossus's theme: huge drums on every beat, a brass call and a grinding low
/// ostinato.
pub(super) fn colossus_loop(rng: &mut Rng) -> Vec<f32> {
    let mut grind = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 300.0, 1.3);
    let mut out = render(loop_length(), |t| {
        // A pulsing grind, smooth so the loop has no hard edges (nor a click at its seam).
        let gate = 0.8 + 0.2 * sine(t, 2.0);
        filter.run(grind.next(55.0), 260.0 + 120.0 * sine(t, 0.5), 1.3) * gate * 0.3
    });
    let step = LOOP_SECONDS / 16.0;
    for index in 0..16_usize {
        #[allow(clippy::cast_precision_loss, reason = "sixteen steps")]
        let start = index as f32 * step;
        if index % 4 == 0 || index % 8 == 6 {
            loop_note(&mut out, &thump(120.0, 38.0, 0.35, 2.2), start, 0.6);
        }
        if index % 4 == 2 {
            let hit = burst(
                rng.next_u64(),
                NoiseColor::White,
                FilterMode::BandPass,
                1_200.0,
                0.8,
                0.12,
            );
            loop_note(&mut out, &hit, start, 0.25);
        }
    }
    for (start, note) in [
        (0.0_f32, 220.0_f32),
        (1.0, 261.63),
        (2.0, 293.66),
        (3.0, 220.0),
    ] {
        loop_note(&mut out, &brass(rng, note, 0.5, 0.7), start, 0.16);
    }
    out
}

/// The Sleeping Garrison's ambience: dormant machines humming, a ratchet settling now and then.
pub(super) fn barracks_bed(rng: &mut Rng) -> Vec<f32> {
    let mut hum = Osc::new(Wave::Sine);
    let mut buzz = Osc::new(Wave::Triangle);
    let mut out = render(loop_length(), |t| {
        (hum.next(60.0) * 0.12 + buzz.next(120.0) * 0.03) * (0.8 + 0.2 * sine(t, 0.5))
    });
    for start in [0.6_f32, 2.4] {
        loop_note(&mut out, &clank(rng, 340.0, 0.2), start, 0.06);
    }
    out
}

/// The Seal Gate's ambience: deep resonance and trickling dust.
pub(super) fn seal_hall_bed(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Sine);
    let mut air = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 220.0, 0.7);
    let mut out = render(loop_length(), |t| {
        drone.next(49.0) * 0.1 * (0.7 + 0.3 * sine(t, 0.25)) + low.process(air.next()) * 0.08
    });
    scatter_grains(
        rng,
        &mut out,
        14,
        (0.0, LOOP_SECONDS),
        (2_000.0, 6_000.0),
        0.04,
        false,
    );
    out
}
