//! Descent II, the Old Deep's sounds (Wave 8): a music bed for each of its seven strata, the
//! Star Core's theme, the dose meter's crackle, and the one-shots of its rules and set pieces.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop; scattered
//! events spread evenly over the render.

use super::{
    Adsr, FilterMode, Fm, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, brass, burst,
    clank, decay, flutter, glide, grains, loop_events, loop_length, mix_into, perc, render,
    saturate, scatter, scatter_grains, segments, sine, thump,
};

/// Crystal struck: a cascade of glassy FM bells on a pentatonic set, ringing out over each
/// other, and a shimmer of high grains as the shards fly.
pub(super) fn resonance_chime(rng: &mut Rng) -> Vec<f32> {
    let notes = [1_318.5_f32, 1_480.0, 1_661.0, 1_975.5, 2_217.5, 2_637.0];
    let mut out = render(1.6, |_| 0.0);
    for (index, time) in scatter(rng, 6, 0.0, 0.45, 1.3).into_iter().enumerate() {
        let note = notes[(index + usize::try_from(rng.next_u64() % 6).unwrap_or(0)) % notes.len()];
        mix_into(&mut out, &bell(note, 3.5, 1.4, 1.1), time, 0.22);
        mix_into(
            &mut out,
            &bell(note * 2.0, 1.0, 0.4, 0.5),
            time + 0.01,
            0.06,
        );
    }
    grains(rng, &mut out, 10, (0.0, 0.6), (5_000.0, 9_000.0), 0.12);
    out
}

/// A lift valve wheel turning: a rusty metal squeal sliding up, the clunk of the seat giving,
/// and a long hiss of pressure escaping.
pub(super) fn valve_turn(rng: &mut Rng) -> Vec<f32> {
    let mut squeal = Osc::new(Wave::Saw);
    let mut squeal_filter = Svf::new(FilterMode::BandPass, 1_400.0, 4.0);
    let start = rng.range(380.0, 440.0);
    let mut out = render(0.9, |t| {
        let pitch = glide(t, start, start * 1.6, 0.4) * (1.0 + 0.02 * sine(t, 17.0));
        squeal_filter.run(squeal.next(pitch), pitch * 2.0, 4.0)
            * segments(t, &[(0.0, 0.0), (0.1, 0.5), (0.7, 0.4), (0.9, 0.0)])
    });
    let clunk = clank(rng, 180.0, 0.5);
    mix_into(&mut out, &clunk, 0.75, 0.7);
    mix_into(&mut out, &thump(120.0, 60.0, 0.25, 1.5), 0.75, 0.6);
    let hiss = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::HighPass,
        2_500.0,
        0.7,
        1.2,
    );
    mix_into(&mut out, &hiss, 0.85, 0.35);
    out
}

/// A piston's ram slamming home: a heavy saturated thud, an iron clank and a short exhaust.
pub(super) fn piston_slam(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(110.0 * rng.jitter(0.05), 40.0, 0.35, 2.2);
    mix_into(&mut out, &clank(rng, 140.0, 0.6), 0.005, 0.6);
    let exhaust = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        3_000.0,
        0.8,
        0.25,
    );
    mix_into(&mut out, &exhaust, 0.05, 0.3);
    out
}

/// A dose meter step: a burst of geiger clicks and a falling two-tone alert.
pub(super) fn geiger_alert(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(0.8, |_| 0.0);
    for time in scatter(rng, 14, 0.0, 0.5, 1.0) {
        let click = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            3_500.0,
            0.7,
            0.004,
        );
        mix_into(&mut out, &click, time, rng.range(0.4, 0.8));
    }
    let mut tone = Osc::new(Wave::Square);
    let mut filter = Svf::new(FilterMode::LowPass, 1_800.0, 0.8);
    let alert = render(0.5, |t| {
        let note = if t < 0.22 { 988.0 } else { 740.0 };
        filter.process(tone.next(note)) * perc(t % 0.22, 0.004, 0.18) * 0.35
    });
    mix_into(&mut out, &alert, 0.2, 0.6);
    out
}

/// A heat storm gathering: a low horn swelling up a fifth over a brown-noise rumble.
pub(super) fn storm_siren(rng: &mut Rng) -> Vec<f32> {
    let mut horn = [Osc::new(Wave::Saw), Osc::new(Wave::Saw)];
    let mut horn_filter = Svf::new(FilterMode::LowPass, 600.0, 1.2);
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_filter = Svf::new(FilterMode::LowPass, 220.0, 0.7);
    render(2.4, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.6, 0.8), (1.8, 1.0), (2.4, 0.0)]);
        let pitch = glide(t, 82.5, 123.75, 0.9);
        let voice: f32 = horn
            .iter_mut()
            .zip([1.0, 1.006])
            .map(|(osc, d)| osc.next(pitch * d))
            .sum::<f32>()
            * 0.5;
        let brass_voice = horn_filter.run(voice, 300.0 + 700.0 * swell, 1.2);
        (saturate(brass_voice * 1.3, 1.4) * 0.6 + rumble_filter.process(rumble.next()) * 0.8)
            * swell
    })
}

/// A strike on the open Star Core: a crystalline boom, a bright bell toll and a cascading
/// shimmer, with a sub drop under it all.
pub(super) fn core_strike(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(70.0, 30.0, 0.9, 2.0);
    mix_into(&mut out, &bell(523.25, 1.41, 2.5, 2.2), 0.0, 0.4);
    mix_into(&mut out, &bell(784.0, 2.0, 1.2, 1.8), 0.02, 0.25);
    for (index, time) in scatter(rng, 8, 0.05, 0.9, 1.4).into_iter().enumerate() {
        let note = 1_046.5 * [1.0, 1.25, 1.5, 2.0][index % 4];
        mix_into(&mut out, &bell(note, 3.5, 0.8, 0.6), time, 0.1);
    }
    out
}

/// The Star Core waking: a deep chord swelling out of nothing, a slow bell tolling three times.
pub(super) fn core_wake(rng: &mut Rng) -> Vec<f32> {
    let mut voices = [
        Osc::new(Wave::Saw),
        Osc::new(Wave::Saw),
        Osc::new(Wave::Saw),
    ];
    let mut filter = Svf::new(FilterMode::LowPass, 400.0, 0.9);
    let mut out = render(3.2, |t| {
        let swell = segments(t, &[(0.0, 0.0), (1.6, 1.0), (2.6, 0.8), (3.2, 0.0)]);
        let chord: f32 = voices
            .iter_mut()
            .zip([55.0_f32, 82.5, 130.75])
            .map(|(osc, note)| osc.next(note))
            .sum::<f32>()
            / 3.0;
        filter.run(chord, 200.0 + 900.0 * swell, 0.9) * swell * 0.7
    });
    for toll in 0..3 {
        let at = 0.4 + toll as f32 * 0.9;
        mix_into(
            &mut out,
            &bell(220.0 * rng.jitter(0.01), 1.41, 3.0, 2.0),
            at,
            0.3,
        );
    }
    out
}

/// A story log found: a soft music-box motif (four notes, a falling answer) with a warm pad.
pub(super) fn deep_log(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(2.2, |_| 0.0);
    for (index, note) in [659.25_f32, 783.99, 987.77, 880.0, 739.99]
        .into_iter()
        .enumerate()
    {
        let at = index as f32 * 0.22;
        mix_into(&mut out, &bell(note, 4.0, 0.6, 1.2), at, 0.25);
    }
    let mut pad = [Osc::new(Wave::Triangle), Osc::new(Wave::Triangle)];
    let warm = render(2.0, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.6, 1.0), (2.0, 0.0)]);
        pad.iter_mut()
            .zip([329.63_f32, 493.88])
            .map(|(osc, note)| osc.next(note))
            .sum::<f32>()
            * 0.2
            * swell
    });
    mix_into(&mut out, &warm, 0.1, 0.6);
    let _ = rng.next_u64();
    out
}

/// A fossil bone crumbling: a dry crunch and a trickle of dust.
pub(super) fn bone_crumble(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_800.0,
        1.0,
        0.12,
    );
    grains(rng, &mut out, 16, (0.02, 0.6), (1_500.0, 4_500.0), 0.3);
    out
}

// --- The strata's beds --------------------------------------------------------------------------

/// Crystal Faults: glass bells arpeggiating an E major ninth over a soft, breathing pad, with a
/// high shimmer of grains.
pub(super) fn crystal_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pad = [
        Osc::new(Wave::Triangle),
        Osc::new(Wave::Triangle),
        Osc::new(Wave::Triangle),
    ];
    let mut out = render(loop_length(), |t| {
        let breath = 0.7 + 0.3 * sine(t, 0.25);
        pad.iter_mut()
            .zip([164.75_f32, 207.75, 247.0])
            .map(|(osc, note)| osc.next(note))
            .sum::<f32>()
            * 0.1
            * breath
    });
    let notes = [
        659.25_f32, 830.5, 987.75, 1_234.75, 1_480.0, 1_234.75, 987.75, 830.5,
    ];
    for (index, note) in notes.into_iter().enumerate() {
        let at = index as f32 * (LOOP_SECONDS / 8.0) + 0.02;
        mix_into(&mut out, &bell(note, 3.5, 1.0, 1.4), at, 0.16);
    }
    scatter_grains(
        rng,
        &mut out,
        loop_events(4.0),
        (0.0, loop_length()),
        (6_000.0, 9_500.0),
        0.04,
        false,
    );
    out
}

/// Fossil Oceans: the dry sea's slow swell, a low drone, distant whale-like glides and the
/// tick of settling grit.
pub(super) fn fossil_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Sine);
    let mut fifth = Osc::new(Wave::Sine);
    let mut sea = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut sea_filter = Svf::new(FilterMode::LowPass, 400.0, 0.7);
    let mut song = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        let swell = 0.6 + 0.4 * sine(t, 0.25);
        let call = (sine(t, 0.5) * 0.5 + 0.5).powi(3);
        let song_pitch = 330.0 + 110.0 * sine(t, 0.25);
        drone.next(55.0) * 0.25
            + fifth.next(82.5) * 0.12 * swell
            + sea_filter.run(sea.next(), 250.0 + 250.0 * swell, 0.7) * 0.5 * swell
            + song.next(song_pitch) * 0.08 * call
    });
    scatter_grains(
        rng,
        &mut out,
        loop_events(2.0),
        (0.0, loop_length()),
        (1_500.0, 3_500.0),
        0.05,
        false,
    );
    out
}

/// Pressure Cathedrals: a choir in a vast nave (vowel-filtered saws on a D minor chord), organ
/// fifths underneath and a far hiss of jets.
pub(super) fn cathedral_loop(rng: &mut Rng) -> Vec<f32> {
    let mut choir = [
        Osc::new(Wave::Saw),
        Osc::new(Wave::Saw),
        Osc::new(Wave::Saw),
        Osc::new(Wave::Saw),
    ];
    let mut vowel = Svf::new(FilterMode::BandPass, 800.0, 2.0);
    let mut organ = [Osc::new(Wave::Square), Osc::new(Wave::Square)];
    let mut organ_filter = Svf::new(FilterMode::LowPass, 500.0, 0.7);
    let mut jets = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut jets_filter = Svf::new(FilterMode::HighPass, 3_000.0, 0.6);
    render(loop_length(), |t| {
        let breath = 0.6 + 0.4 * sine(t, 0.25).abs();
        let voices: f32 = choir
            .iter_mut()
            .zip([146.75_f32, 174.5, 220.0, 293.75])
            .map(|(osc, note)| osc.next(note * (1.0 + 0.003 * sine(t, 5.0))))
            .sum::<f32>()
            / 4.0;
        let open = 700.0 + 300.0 * sine(t, 0.5);
        let sung = vowel.run(voices, open, 2.0) * 0.5 * breath;
        let low: f32 = organ
            .iter_mut()
            .zip([73.5_f32, 110.0])
            .map(|(osc, note)| osc.next(note))
            .sum::<f32>()
            * 0.5;
        sung + organ_filter.process(low) * 0.18 + jets_filter.process(jets.next()) * 0.03
    })
}

/// Radioactive Hollow: an uneasy pulse (a minor-second beat between two detuned sines), a
/// sickly high whine and a sparse crackle of clicks.
pub(super) fn hollow_loop(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Sine);
    let mut rub = Osc::new(Wave::Sine);
    let mut whine = Osc::new(Wave::Triangle);
    let mut pulse_bass = Osc::new(Wave::Saw);
    let mut bass_filter = Svf::new(FilterMode::LowPass, 300.0, 1.2);
    let mut out = render(loop_length(), |t| {
        let throb = perc(t % 0.5, 0.01, 0.35);
        low.next(98.0) * 0.18
            + rub.next(103.75) * 0.14
            + whine.next(1_661.0 + 20.0 * sine(t, 0.25)) * 0.025
            + bass_filter.process(pulse_bass.next(49.0)) * throb * 0.25
    });
    for time in scatter(rng, loop_events(6.0), 0.0, loop_length(), 1.0) {
        let click = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            4_000.0,
            0.7,
            0.003,
        );
        mix_into(&mut out, &click, time, rng.range(0.1, 0.25));
    }
    out
}

/// Ancient Machine Layer: the builders' works ticking over at 120 beats a minute, a saw
/// ostinato, metal clanks on the off-beats and a steam hiss on the bar.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the loop's step index"
)]
pub(super) fn machine_loop(rng: &mut Rng) -> Vec<f32> {
    let mut bass = Osc::new(Wave::Saw);
    let mut bass_filter = Svf::new(FilterMode::LowPass, 350.0, 1.3);
    let notes = [55.0_f32, 55.0, 65.5, 61.75];
    let mut out = render(loop_length(), |t| {
        let step = ((t / (LOOP_SECONDS / 8.0)) as usize) % 8;
        let local = t % (LOOP_SECONDS / 8.0);
        let note = notes[step / 2];
        bass_filter.process(bass.next(note)) * perc(local, 0.005, 0.3) * 0.35
    });
    for beat in 0..8 {
        let at = beat as f32 * (LOOP_SECONDS / 8.0);
        mix_into(&mut out, &thump(90.0, 50.0, 0.18, 1.8), at, 0.3);
        if beat % 2 == 1 {
            mix_into(&mut out, &clank(rng, 260.0, 0.25), at, 0.18);
        }
    }
    let hiss = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::HighPass,
        2_800.0,
        0.7,
        0.5,
    );
    mix_into(&mut out, &hiss, LOOP_SECONDS / 2.0, 0.15);
    out
}

/// Void Geode Fields: weightless sines gliding between notes of a Lydian chord, slow chimes and a
/// shimmering airy noise.
pub(super) fn geode_loop(rng: &mut Rng) -> Vec<f32> {
    let mut float = [
        Osc::new(Wave::Sine),
        Osc::new(Wave::Sine),
        Osc::new(Wave::Sine),
    ];
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut air_filter = Svf::new(FilterMode::BandPass, 2_400.0, 0.9);
    let mut out = render(loop_length(), |t| {
        let drift = 0.5 + 0.5 * sine(t, 0.25);
        let a = float[0].next(261.5 + 32.75 * drift);
        let b = float[1].next(392.0 - 24.5 * drift);
        let c = float[2].next(523.25 + 46.25 * (1.0 - drift));
        (a + b + c) * 0.08 + air_filter.run(air.next(), 1_800.0 + 1_200.0 * drift, 0.9) * 0.05
    });
    for (index, note) in [1_174.75_f32, 1_480.0, 1_760.0, 1_318.5]
        .into_iter()
        .enumerate()
    {
        mix_into(
            &mut out,
            &bell(note, 2.0, 0.8, 2.0),
            index as f32 * (LOOP_SECONDS / 4.0) + 0.3,
            0.1,
        );
    }
    out
}

/// Mantle Storm Zone: a deep molten rumble, crackling heat and a slow, heavy drum like a
/// heartbeat in the rock.
pub(super) fn mantle_loop(rng: &mut Rng) -> Vec<f32> {
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_filter = Svf::new(FilterMode::LowPass, 160.0, 0.9);
    let mut drone = Osc::new(Wave::Saw);
    let mut drone_filter = Svf::new(FilterMode::LowPass, 200.0, 1.0);
    let mut out = render(loop_length(), |t| {
        let heave = 0.7 + 0.3 * sine(t, 0.25);
        rumble_filter.process(rumble.next()) * 0.9 * heave
            + drone_filter.process(drone.next(41.25)) * 0.15
    });
    for beat in 0..4 {
        let at = beat as f32 * (LOOP_SECONDS / 4.0);
        mix_into(&mut out, &thump(65.0, 35.0, 0.5, 2.0), at, 0.35);
        mix_into(&mut out, &thump(60.0, 32.0, 0.4, 2.0), at + 0.28, 0.2);
    }
    scatter_grains(
        rng,
        &mut out,
        loop_events(9.0),
        (0.0, loop_length()),
        (2_000.0, 6_000.0),
        0.06,
        false,
    );
    out
}

/// The Star Core's theme: driving drums at 150 beats a minute, a bright FM arpeggio climbing
/// over a pulsing bass and brass stabs on each half bar.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the loop's step index"
)]
pub(super) fn star_core_loop(rng: &mut Rng) -> Vec<f32> {
    let mut bass = Osc::new(Wave::Saw);
    let mut bass_filter = Svf::new(FilterMode::LowPass, 420.0, 1.4);
    let mut lead = Fm::default();
    let arp = [
        261.5_f32, 329.75, 392.0, 523.25, 659.25, 523.25, 392.0, 329.75,
    ];
    let mut out = render(loop_length(), |t| {
        let sixteenth = LOOP_SECONDS / 16.0;
        let step = ((t / sixteenth) as usize) % 16;
        let local = t % sixteenth;
        let bass_note = if step < 8 { 65.5 } else { 58.25 };
        let low = saturate(
            bass_filter.process(bass.next(bass_note)) * perc(local, 0.004, 0.2) * 1.3,
            1.5,
        );
        let note = arp[step % 8] * 2.0;
        let high = lead.next(note, 2.0, 1.2 * decay(local, 0.08)) * perc(local, 0.002, 0.15);
        low * 0.28 + high * 0.12
    });
    for beat in 0..8 {
        let at = beat as f32 * (LOOP_SECONDS / 8.0);
        mix_into(&mut out, &thump(85.0, 40.0, 0.25, 2.2), at, 0.4);
        if beat % 2 == 1 {
            let snare = burst(
                rng.next_u64(),
                NoiseColor::White,
                FilterMode::BandPass,
                2_200.0,
                0.8,
                0.12,
            );
            mix_into(&mut out, &snare, at, 0.3);
        }
    }
    for start in [0.0_f32, 2.0] {
        for note in [523.25_f32, 659.25, 784.0] {
            mix_into(&mut out, &brass(rng, note, 0.25, 0.9), start, 0.1);
        }
    }
    out
}

/// The dose meter's crackle: random geiger clicks over a faint hum (the director scales it
/// with the dose).
pub(super) fn geiger_loop(rng: &mut Rng) -> Vec<f32> {
    let mut hum = Osc::new(Wave::Sine);
    let mut wobble = flutter(rng, 2.0, 0.3);
    let mut out = render(loop_length(), |_| hum.next(60.0) * 0.02 * wobble());
    for time in scatter(rng, loop_events(14.0), 0.0, loop_length(), 1.0) {
        let click = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            3_200.0,
            0.7,
            0.003,
        );
        mix_into(&mut out, &click, time, rng.range(0.3, 0.7));
    }
    let _ = Adsr::new(0.01, 0.1, 0.5, 0.1);
    out
}
