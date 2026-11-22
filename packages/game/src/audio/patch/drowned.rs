//! Descent IV, the Drowned and Frozen's sounds (Wave 9): a music bed for each of its five
//! strata, the Leviathan's theme, and the one-shots of its rules and set pieces.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop;
//! scattered events spread evenly over the render.

use super::{
    FilterMode, Fm, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, brass, burst,
    clank, glide, grains, loop_events, loop_length, mix_into, perc, render, saturate, scatter,
    scatter_grains, segments, sine, thump,
};

/// A sonar ping: a clean, round sine "pong" that rings out, then its echo coming back off the
/// rock a beat later, fainter and darker.
pub(super) fn sonar_ping(rng: &mut Rng) -> Vec<f32> {
    let pitch = 1_480.0 * rng.jitter(0.01);
    let ping = |gain: f32, darkness: f32| {
        let mut tone = Osc::new(Wave::Sine);
        let mut shimmer = Osc::new(Wave::Sine);
        render(0.9, move |t| {
            let body = tone.next(pitch * (1.0 - 0.02 * t)) * perc(t, 0.002, 0.55);
            let overtone = shimmer.next(pitch * 2.01) * perc(t, 0.001, 0.15) * (1.0 - darkness);
            (body + overtone * 0.3) * gain
        })
    };
    let mut out = ping(0.8, 0.0);
    mix_into(&mut out, &ping(0.3, 0.7), 0.45, 1.0);
    mix_into(&mut out, &ping(0.12, 0.9), 0.8, 1.0);
    out
}

/// A geyser blowing: a deep rumble swelling into a roaring white hiss of steam, then the patter
/// of falling droplets.
pub(super) fn geyser_blast(rng: &mut Rng) -> Vec<f32> {
    let mut steam = Noise::new(NoiseColor::White, rng.next_u64());
    let mut steam_filter = Svf::new(FilterMode::BandPass, 2_200.0, 0.6);
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_filter = Svf::new(FilterMode::LowPass, 140.0, 0.9);
    let mut out = render(2.2, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.12, 1.0), (1.4, 0.8), (2.2, 0.0)]);
        let hiss = steam_filter.run(steam.next(), 1_600.0 + 1_400.0 * swell, 0.6) * swell;
        let low = rumble_filter.process(rumble.next())
            * segments(t, &[(0.0, 0.4), (0.3, 1.0), (2.2, 0.0)]);
        saturate(hiss * 0.8 + low * 1.2, 1.3)
    });
    grains(rng, &mut out, 18, (1.0, 2.1), (2_500.0, 6_500.0), 0.12);
    out
}

/// Ice giving way: a sharp glassy crack, a ringing high crystal tail and a low groan of the
/// glacier settling.
pub(super) fn ice_crack(rng: &mut Rng) -> Vec<f32> {
    let crack = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_200.0,
        0.9,
        0.05,
    );
    let mut out = render(1.1, |_| 0.0);
    mix_into(&mut out, &crack, 0.0, 0.9);
    mix_into(
        &mut out,
        &bell(2_637.0 * rng.jitter(0.02), 3.1, 1.6, 0.6),
        0.01,
        0.2,
    );
    let mut groan = Osc::new(Wave::Saw);
    let mut groan_filter = Svf::new(FilterMode::LowPass, 260.0, 2.0);
    let base = rng.range(60.0, 80.0);
    let low = render(1.0, |t| {
        groan_filter.process(groan.next(base * (1.0 - 0.2 * t))) * perc(t, 0.1, 0.7) * 0.4
    });
    mix_into(&mut out, &low, 0.08, 1.0);
    out
}

/// The hull under pressure: a long, low metallic groan bending downward, rivets ticking.
pub(super) fn hull_groan(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let base = rng.range(52.0, 64.0);
    let mut out = render(1.8, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.4, 1.0), (1.3, 0.7), (1.8, 0.0)]);
        saturate(fm.next(base * (1.0 - 0.12 * t), 2.76, 1.8 * swell), 1.5) * swell * 0.7
    });
    for time in scatter(rng, 5, 0.2, 1.6, 1.0) {
        mix_into(&mut out, &clank(rng, 900.0, 0.08), time, 0.25);
    }
    out
}

/// A clam snapping shut: a hollow wooden clack and a wet slap.
pub(super) fn clam_snap(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 340.0, 0.12);
    mix_into(&mut out, &thump(220.0, 90.0, 0.12, 1.6), 0.0, 0.8);
    let slap = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        1_200.0,
        1.2,
        0.08,
    );
    mix_into(&mut out, &slap, 0.01, 0.5);
    out
}

/// The Leviathan's roar: an enormous, slow, whale-like FM call gliding down a fifth over a sub
/// swell and a roiling of water.
pub(super) fn leviathan_roar(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let mut sub = Osc::new(Wave::Sine);
    let mut water = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut water_filter = Svf::new(FilterMode::LowPass, 300.0, 0.8);
    let base = rng.range(72.0, 82.0);
    render(2.8, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.5, 1.0), (1.9, 0.8), (2.8, 0.0)]);
        let pitch = glide(t, base * 1.5, base, 1.6) * (1.0 + 0.03 * sine(t, 3.5));
        let call = fm.next(pitch, 1.5, 2.6 * swell);
        let low = sub.next(base * 0.5);
        let roil = water_filter.process(water.next());
        saturate(call * 0.55 + low * 0.6 + roil * 0.5, 1.8) * swell
    })
}

/// A warm merrow chime: a harp's rising arpeggio and a soft vowel-like pad (arriving in the
/// city, a story found).
pub(super) fn merrow_chime(rng: &mut Rng) -> Vec<f32> {
    let mut pad = Osc::new(Wave::Triangle);
    let mut out = render(1.6, |t| {
        pad.next(293.66) * segments(t, &[(0.0, 0.0), (0.3, 0.2), (1.6, 0.0)])
    });
    for (index, note) in [587.33_f32, 739.99, 880.0, 1_174.66]
        .into_iter()
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "four notes")]
        let start = index as f32 * 0.08;
        mix_into(
            &mut out,
            &bell(note * rng.jitter(0.002), 2.0, 0.6, 0.9),
            start,
            0.35,
        );
    }
    out
}

/// The Underground Ocean: a slow swell of low strings rising and falling like a tide, far whale
/// calls gliding, and bubbles rising.
pub(super) fn ocean_loop(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Triangle);
    let mut fifth = Osc::new(Wave::Triangle);
    let mut sea = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut sea_filter = Svf::new(FilterMode::LowPass, 360.0, 0.7);
    let mut whale = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        let tide = 0.6 + 0.4 * sine(t, 0.25);
        let call = whale.next(220.0 + 40.0 * sine(t, 0.5)) * 0.06 * (0.5 + 0.5 * sine(t, 0.25));
        (low.next(73.5) * 0.45 + fifth.next(110.0) * 0.25) * tide * 0.35
            + sea_filter.process(sea.next()) * 0.25 * tide
            + call
    });
    let step = LOOP_SECONDS / 4.0;
    for (index, note) in [293.75_f32, 329.5, 293.75, 246.75].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "four steps")]
        let start = index as f32 * step;
        mix_into(&mut out, &brass(rng, note, step * 0.9, 0.08), start, 0.1);
    }
    scatter_grains(
        rng,
        &mut out,
        loop_events(5.0),
        (0.0, loop_length()),
        (900.0, 2_400.0),
        0.05,
        false,
    );
    out
}

/// Brine Glaciers: a cold celesta line in fifths over bowed glass and the slow creak of ice.
pub(super) fn glacier_loop(rng: &mut Rng) -> Vec<f32> {
    let mut glass = [Osc::new(Wave::Sine), Osc::new(Wave::Sine)];
    let mut wind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut wind_filter = Svf::new(FilterMode::BandPass, 1_800.0, 1.2);
    let mut out = render(loop_length(), |t| {
        let bow = 0.6 + 0.4 * sine(t, 0.25);
        glass
            .iter_mut()
            .zip([440.0_f32, 659.25])
            .map(|(osc, note)| osc.next(note))
            .sum::<f32>()
            * 0.06
            * bow
            + wind_filter.process(wind.next()) * 0.04 * (0.5 + 0.5 * sine(t, 0.5))
    });
    let notes = [
        1_318.5_f32,
        987.75,
        1_174.75,
        880.0,
        1_318.5,
        1_480.0,
        1_174.75,
        987.75,
    ];
    for (index, note) in notes.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "eight steps")]
        let at = index as f32 * (LOOP_SECONDS / 8.0) + 0.03;
        mix_into(&mut out, &bell(note, 4.0, 0.9, 0.9), at, 0.12);
    }
    for time in [0.9_f32, 2.9] {
        let mut creak = Osc::new(Wave::Saw);
        let mut creak_filter = Svf::new(FilterMode::BandPass, 380.0, 3.0);
        let groan = render(0.8, |t| {
            creak_filter.process(creak.next(70.0 * (1.0 + 0.2 * t))) * perc(t, 0.1, 0.6) * 0.25
        });
        mix_into(&mut out, &groan, time, 0.5);
    }
    out
}

/// Geyser Fields: a bubbling marimba ostinato, kettle drums on the beat and the hiss of steam
/// swelling with the bar.
pub(super) fn geyser_loop(rng: &mut Rng) -> Vec<f32> {
    let mut steam = Noise::new(NoiseColor::White, rng.next_u64());
    let mut steam_filter = Svf::new(FilterMode::BandPass, 2_600.0, 0.7);
    let mut drone = Osc::new(Wave::Triangle);
    let mut out = render(loop_length(), |t| {
        let bar = (t % LOOP_SECONDS) / LOOP_SECONDS;
        drone.next(65.5) * 0.12 + steam_filter.process(steam.next()) * 0.05 * bar * bar
    });
    let step = LOOP_SECONDS / 16.0;
    let figure = [
        196.0_f32, 246.75, 293.75, 246.75, 220.0, 293.75, 329.5, 293.75,
    ];
    for index in 0..16_usize {
        #[allow(clippy::cast_precision_loss, reason = "sixteen steps")]
        let start = index as f32 * step;
        let note = figure[index % figure.len()];
        mix_into(
            &mut out,
            &bell(note * rng.jitter(0.002), 4.0, 0.6, 0.3),
            start,
            if index % 4 == 0 { 0.15 } else { 0.08 },
        );
    }
    for beat in 0..4_u8 {
        let start = f32::from(beat);
        mix_into(&mut out, &thump(110.0, 55.0, 0.35, 1.4), start, 0.4);
    }
    out
}

/// Merrow City: a warm, lilting harp waltz over voice-like pads, the fish-folk's song.
pub(super) fn merrow_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pads = [
        Osc::new(Wave::Triangle),
        Osc::new(Wave::Triangle),
        Osc::new(Wave::Triangle),
    ];
    let mut vowel = Svf::new(FilterMode::BandPass, 800.0, 2.0);
    let mut out = render(loop_length(), |t| {
        let breath = 0.7 + 0.3 * sine(t, 0.5);
        let raw = pads
            .iter_mut()
            .zip([146.75_f32, 185.0, 220.0])
            .map(|(osc, note)| osc.next(note))
            .sum::<f32>();
        vowel.run(raw, 700.0 + 300.0 * sine(t, 0.25), 2.0) * 0.12 * breath
    });
    let step = LOOP_SECONDS / 12.0;
    let waltz = [
        587.25_f32, 740.0, 880.0, 740.0, 659.25, 880.0, 987.75, 880.0, 740.0, 659.25, 587.25, 440.0,
    ];
    for (index, note) in waltz.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "twelve steps")]
        let start = index as f32 * step;
        let gain = if index % 3 == 0 { 0.16 } else { 0.09 };
        mix_into(
            &mut out,
            &bell(note * rng.jitter(0.002), 2.0, 0.7, 0.8),
            start,
            gain,
        );
    }
    out
}

/// Pearl Abyss: a crushing low drone, a slow heartbeat of the deep and the hull ticking under
/// pressure.
pub(super) fn abyss_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut drone_filter = Svf::new(FilterMode::LowPass, 160.0, 1.4);
    let mut sub = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        drone_filter.run(drone.next(41.25), 120.0 + 40.0 * sine(t, 0.25), 1.4) * 0.3
            + sub.next(30.75) * 0.2
    });
    for beat in [0.0_f32, 2.0] {
        mix_into(&mut out, &thump(60.0, 34.0, 0.4, 1.6), beat, 0.5);
        mix_into(&mut out, &thump(52.0, 30.0, 0.3, 1.4), beat + 0.35, 0.35);
    }
    for time in scatter(rng, 4, 0.1, loop_length() - 0.2, 1.0) {
        mix_into(&mut out, &clank(rng, 1_100.0, 0.05), time, 0.08);
    }
    out
}

/// The Leviathan's fight: thundering toms, a brass call of falling fifths and a churning low
/// sea; urgent, then crashing.
pub(super) fn leviathan_loop(rng: &mut Rng) -> Vec<f32> {
    let mut sea = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut sea_filter = Svf::new(FilterMode::LowPass, 260.0, 0.8);
    let mut drone = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut drone_filter = Svf::new(FilterMode::LowPass, 400.0, 1.2);
    let mut out = render(loop_length(), |t| {
        sea_filter.process(sea.next()) * 0.25
            + drone_filter.run(drone.next(55.0), 300.0 + 120.0 * sine(t, 0.5), 1.2) * 0.18
    });
    let beat = LOOP_SECONDS / 8.0;
    for step in 0..8_u8 {
        let start = f32::from(step) * beat;
        let tom = thump(if step % 2 == 0 { 110.0 } else { 150.0 }, 44.0, 0.25, 1.8);
        mix_into(
            &mut out,
            &tom,
            start,
            if step % 4 == 0 { 0.6 } else { 0.32 },
        );
    }
    for (index, note) in [220.0_f32, 146.75, 196.0, 130.75].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "four calls")]
        let start = index as f32 * (LOOP_SECONDS / 4.0);
        mix_into(&mut out, &brass(rng, note, 0.7, 0.3), start, 0.16);
    }
    out
}
