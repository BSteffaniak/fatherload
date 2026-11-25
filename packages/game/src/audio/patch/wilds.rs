//! The wild worlds' music and ambience beds (Wave 12 planets-a): one bed per theme, each a
//! piece of music woven into its world's sound, plus a storm bed for squalls and blizzards.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop.

#![allow(
    clippy::suboptimal_flops,
    clippy::cast_precision_loss,
    reason = "presentation code: sound math reads top to bottom"
)]

use super::{
    FilterMode, Fm, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, loop_length,
    loop_note, render, segments, sine, thump,
};

/// The jungle world's bed: a warm wooden marimba line in A dorian over a frog-and-insect
/// chorus, soft log drums, and a far trumpeting call of something big once a loop; under it
/// the hiss of leaves.
pub(super) fn jungle_world_loop(rng: &mut Rng) -> Vec<f32> {
    let mut insects = Noise::new(NoiseColor::White, rng.next_u64());
    let mut insect_filter = Svf::new(FilterMode::BandPass, 4_400.0, 4.0);
    let mut leaves = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut leaf_filter = Svf::new(FilterMode::HighPass, 2_400.0, 0.7);
    let mut out = render(loop_length(), |t| {
        // Insects pulse in a quick shimmer; leaves rustle in slow waves.
        insect_filter.process(insects.next()) * 0.018 * (0.5 + 0.5 * sine(t, 3.0))
            + leaf_filter.process(leaves.next()) * 0.012 * (0.6 + 0.4 * sine(t, 0.25))
    });
    // Log drums on a lilting eight.
    let step = LOOP_SECONDS / 16.0;
    for (index, hit) in [
        1.0_f32, 0.0, 0.0, 0.6, 0.0, 0.0, 0.8, 0.0, 1.0, 0.0, 0.5, 0.0, 0.0, 0.6, 0.4, 0.0,
    ]
    .into_iter()
    .enumerate()
    {
        if hit > 0.0 {
            let pitch = if index % 4 == 0 { 120.0 } else { 180.0 };
            let drum = thump(pitch, pitch * 0.7, 0.16, 1.2);
            loop_note(&mut out, &drum, index as f32 * step, 0.18 * hit);
        }
    }
    // The marimba: A dorian, rising and falling.
    let line = [
        440.0_f32, 523.25, 587.25, 659.25, 587.25, 493.75, 440.0, 392.0,
    ];
    for (index, note) in line.into_iter().enumerate() {
        let start = index as f32 * step * 2.0;
        let marimba = bell(note * rng.jitter(0.002), 4.0, 0.55, 0.45);
        loop_note(&mut out, &marimba, start, 0.11);
        if index % 2 == 1 {
            let echo = bell(note * 2.0, 4.0, 0.4, 0.3);
            loop_note(&mut out, &echo, start + step, 0.04);
        }
    }
    // A tree frog's two-note croak.
    for (index, start) in [0.7_f32, 2.9].into_iter().enumerate() {
        let mut croak = Osc::new(Wave::Square).with_phase(rng.unit());
        let mut croak_filter = Svf::new(FilterMode::BandPass, 900.0, 3.0);
        let pitch = if index == 0 { 330.0 } else { 293.75 };
        let call = render(0.18, |t| {
            croak_filter.process(croak.next(pitch))
                * segments(t, &[(0.0, 0.0), (0.02, 1.0), (0.18, 0.0)])
        });
        loop_note(&mut out, &call, start, 0.06);
        loop_note(&mut out, &call, start + 0.22, 0.05);
    }
    // Far away, a trundlehorn calls.
    let mut horn = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut horn_filter = Svf::new(FilterMode::LowPass, 420.0, 1.2);
    let call = render(1.4, |t| {
        let pitch = 110.0 + 22.0 * segments(t, &[(0.0, 0.0), (0.5, 1.0), (1.4, 0.4)]);
        horn_filter.process(horn.next(pitch))
            * segments(t, &[(0.0, 0.0), (0.4, 1.0), (1.1, 0.5), (1.4, 0.0)])
    });
    loop_note(&mut out, &call, 2.2, 0.07);
    out
}

/// The archipelago's bed: waves breaking on a slow swell, a gentle steel-drum line in D major
/// pentatonic, a gull's cry, and the low warm hum of open water.
pub(super) fn ocean_world_loop(rng: &mut Rng) -> Vec<f32> {
    let mut surf = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut surf_filter = Svf::new(FilterMode::LowPass, 900.0, 0.8);
    let mut hum = Osc::new(Wave::Sine);
    let mut hum_fifth = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        // Two breakers a loop: the swell rises, crashes, washes out.
        let swell = segments(t % 2.0, &[(0.0, 0.15), (1.1, 0.7), (1.3, 1.0), (2.0, 0.15)]);
        surf_filter.process(surf.next()) * 0.05 * swell
            + (hum.next(73.5) * 0.5 + hum_fifth.next(110.25) * 0.3) * 0.06
    });
    // Steel drum: D major pentatonic, lazy and bright.
    let step = LOOP_SECONDS / 8.0;
    let line = [
        587.25_f32, 740.0, 880.0, 740.0, 659.25, 587.25, 493.75, 587.25,
    ];
    for (index, note) in line.into_iter().enumerate() {
        if index == 3 {
            continue;
        }
        let mut fm = Fm::default();
        let length = step * 1.4;
        let pan = render(length, |t| {
            fm.next(
                note * rng.jitter(0.001),
                1.5,
                1.2 * (1.0 - t / length).max(0.0),
            ) * segments(t, &[(0.0, 0.0), (0.005, 1.0), (length, 0.0)])
        });
        loop_note(&mut out, &pan, index as f32 * step, 0.1);
    }
    // A gull, far off.
    let mut gull = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let cry = render(0.5, |t| {
        let pitch = 1_400.0 + 500.0 * segments(t, &[(0.0, 0.0), (0.12, 1.0), (0.5, 0.1)]);
        gull.next(pitch) * segments(t, &[(0.0, 0.0), (0.05, 1.0), (0.4, 0.4), (0.5, 0.0)])
    });
    loop_note(&mut out, &cry, 1.6, 0.025);
    loop_note(&mut out, &cry, 2.25, 0.018);
    out
}

/// The glacier's bed: a wide, cold pad of open fifths on E, wind keening over the ice, slow
/// glassy bells falling like snow, and the deep groan of the glacier moving once a loop.
pub(super) fn glacier_world_loop(rng: &mut Rng) -> Vec<f32> {
    let mut wind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut wind_filter = Svf::new(FilterMode::BandPass, 1_200.0, 2.5);
    let mut low = Osc::new(Wave::Triangle);
    let mut fifth = Osc::new(Wave::Triangle);
    let mut high = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        let keen = 900.0 + 700.0 * (0.5 + 0.5 * sine(t, 0.25));
        wind_filter.run(wind.next(), keen, 2.5) * 0.03 * (0.5 + 0.5 * sine(t, 0.5))
            + (low.next(82.5) * 0.4 + fifth.next(123.75) * 0.3 + high.next(330.0) * 0.08)
                * (0.75 + 0.25 * sine(t, 0.25))
                * 0.25
    });
    // Glass bells, falling slowly: E minor, high and sparse.
    let falls = [
        (0.2_f32, 1_318.5_f32),
        (1.1, 987.75),
        (1.9, 1_174.75),
        (3.0, 880.0),
    ];
    for (start, note) in falls {
        let chime = bell(note * rng.jitter(0.002), 3.5, 1.1, 1.6);
        loop_note(&mut out, &chime, start, 0.05);
    }
    // The glacier groans.
    let mut groan = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut groan_filter = Svf::new(FilterMode::LowPass, 160.0, 1.6);
    let creak = render(1.2, |t| {
        let pitch = 41.25 + 6.0 * sine(t, 3.0);
        groan_filter.process(groan.next(pitch))
            * segments(t, &[(0.0, 0.0), (0.3, 1.0), (0.9, 0.7), (1.2, 0.0)])
    });
    loop_note(&mut out, &creak, 2.6, 0.1);
    out
}

/// A storm over a wild world (squalls and blizzards): roaring, gusting wind and driven
/// spray or snow, swelling and easing.
pub(super) fn wild_storm_loop(rng: &mut Rng) -> Vec<f32> {
    let mut roar = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut hiss = Noise::new(NoiseColor::White, rng.next_u64());
    let mut roar_filter = Svf::new(FilterMode::LowPass, 500.0, 0.8);
    let mut hiss_filter = Svf::new(FilterMode::BandPass, 3_000.0, 1.2);
    render(loop_length(), |t| {
        let gust = 0.55 + 0.45 * sine(t, 0.5) * sine(t, 0.25).abs();
        roar_filter.run(roar.next(), 300.0 + 500.0 * gust, 0.8) * 0.25 * gust
            + hiss_filter.process(hiss.next()) * 0.025 * (0.4 + 0.6 * gust)
    })
}
