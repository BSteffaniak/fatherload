//! Space combat's sounds (Wave 12 hostiles): the rig's cannon and missiles, hits on ships and
//! their breaking up, the deflector failing, a pirate hail, a boss's warning klaxon and its
//! arrival, the hostiles' own shots, and two music loops: the dogfight's driving pulse and the
//! bosses' theme.
//!
//! Loop notes are multiples of 0.25 Hz so every voice completes whole cycles in a loop;
//! scattered events spread evenly over the render.

use super::{
    FilterMode, LOOP_SECONDS, Noise, NoiseColor, Osc, Rng, Svf, Wave, bell, brass, burst, glide,
    loop_length, loop_note, mix_into, perc, render, saturate, segments, sine, thump,
};

/// The rig's cannon: a bright, short pew with a falling pitch and a click of recoil.
pub(super) fn hx_cannon(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Square).with_phase(rng.unit());
    let top = rng.range(1_500.0, 1_700.0);
    let mut out = render(0.14, |t| {
        let frequency = glide(t, top, 380.0, 0.025);
        osc.next(frequency) * perc(t, 0.001, 0.12) * 0.5
    });
    mix_into(&mut out, &thump(180.0, 70.0, 0.05, 1.4), 0.0, 0.45);
    out
}

/// A missile leaving the rack: a hiss that swells into a rushing roar and pulls away.
pub(super) fn hx_missile(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 900.0, 0.9);
    let mut motor = Osc::new(Wave::Saw).with_phase(rng.unit());
    render(0.8, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.06, 1.0), (0.3, 0.7), (0.8, 0.0)]);
        let cutoff = 600.0 + 2_400.0 * segments(t, &[(0.0, 0.0), (0.15, 1.0), (0.8, 0.3)]);
        let hiss = band.run(noise.next(), cutoff, 0.9);
        let buzz = motor.next(90.0 + 50.0 * t) * 0.18;
        saturate((hiss + buzz) * swell, 1.5)
    })
}

/// A rig's shot striking a ship: a metallic tick with a short ring.
pub(super) fn hx_ship_hit(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        rng.range(3_000.0, 3_600.0),
        2.0,
        0.04,
    );
    mix_into(
        &mut out,
        &bell(rng.range(1_150.0, 1_300.0), 1.41, 1.2, 0.12),
        0.0,
        0.3,
    );
    out
}

/// A ship breaking up in vacuum: a muffled thud felt through the hull, crackle and debris.
pub(super) fn hx_ship_boom(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(90.0, 30.0, 0.7, 2.4);
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 700.0, 0.7);
    let rumble = render(1.1, |t| {
        low.run(noise.next(), 900.0 - 600.0 * (t / 1.1), 0.7) * perc(t, 0.004, 0.9)
    });
    mix_into(&mut out, &rumble, 0.0, 0.8);
    for index in 0..6_u8 {
        let at = 0.05 + f32::from(index) * rng.range(0.05, 0.09);
        let crackle = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            2_500.0,
            0.8,
            0.03,
        );
        mix_into(&mut out, &crackle, at, 0.2);
    }
    out
}

/// The deflector failing: a falling electric sweep that sputters out.
pub(super) fn hx_shield_down(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 2_000.0, 2.5);
    render(0.6, |t| {
        let frequency = glide(t, 880.0, 110.0, 0.18);
        let sputter = if (t * 40.0).fract() < 0.6 { 1.0 } else { 0.35 };
        filter.run(osc.next(frequency), 2_400.0 - 2_000.0 * (t / 0.6), 2.5)
            * perc(t, 0.004, 0.55)
            * sputter
            * 0.6
    })
}

/// A pirate hail: a radio squelch and a two-note chirp, a little menacing.
pub(super) fn hx_hail(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        1_800.0,
        1.5,
        0.12,
    );
    let mut chirp = Osc::new(Wave::Square).with_phase(rng.unit());
    let notes = render(0.5, |t| {
        let frequency = if t < 0.2 { 660.0 } else { 494.0 };
        let gate = if (0.02..0.18).contains(&t) || (0.24..0.46).contains(&t) {
            1.0
        } else {
            0.0
        };
        chirp.next(frequency) * gate * 0.3
    });
    mix_into(&mut out, &notes, 0.1, 1.0);
    out
}

/// A boss winding up an attack: a two-tone klaxon, clearly a warning.
pub(super) fn hx_boss_warn(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Square).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 1_600.0, 1.2);
    render(0.7, |t| {
        let frequency = if (t * 2.5).fract() < 0.5 {
            392.0
        } else {
            523.25
        };
        filter.run(osc.next(frequency), 1_600.0, 1.2) * perc(t, 0.01, 0.65) * 0.45
    })
}

/// A boss arriving: a deep horn under a falling sub swell and a struck bell.
pub(super) fn hx_boss_appear(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(70.0, 32.0, 1.6, 1.8);
    for (start, note) in [(0.0_f32, 110.0_f32), (0.5, 103.83), (1.0, 98.0)] {
        mix_into(&mut out, &brass(rng, note, 0.6, 0.6), start, 0.35);
    }
    mix_into(&mut out, &bell(220.0, 1.41, 1.6, 2.0), 0.0, 0.25);
    out
}

/// A hostile ship's shot: a lower, buzzier zap than the rig's.
pub(super) fn hx_enemy_shot(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 1_800.0, 1.6);
    let top = rng.range(700.0, 820.0);
    render(0.18, |t| {
        let frequency = glide(t, top, 160.0, 0.05);
        filter.run(osc.next(frequency), 1_800.0, 1.6) * perc(t, 0.002, 0.16) * 0.5
    })
}

/// The dogfight: a driving minor pulse (bass on every beat, a hat off it), a stabbing synth
/// riff and a held pad, D minor.
pub(super) fn hx_combat_loop(rng: &mut Rng) -> Vec<f32> {
    let mut pad = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut pad_filter = Svf::new(FilterMode::LowPass, 700.0, 0.9);
    let mut out = render(loop_length(), |t| {
        pad_filter.run(pad.next(146.75), 600.0 + 200.0 * sine(t, 0.25), 0.9) * 0.12
    });
    let step = LOOP_SECONDS / 16.0;
    let bass = [73.5_f32, 73.5, 87.25, 73.5, 65.5, 73.5, 98.0, 87.25];
    for index in 0..16_usize {
        #[allow(clippy::cast_precision_loss, reason = "sixteen steps")]
        let start = index as f32 * step;
        if index % 2 == 0 {
            loop_note(
                &mut out,
                &thump(bass[index / 2] * 2.0, bass[index / 2], 0.22, 1.6),
                start,
                0.5,
            );
        } else {
            let hat = burst(
                rng.next_u64(),
                NoiseColor::White,
                FilterMode::HighPass,
                7_000.0,
                0.7,
                0.04,
            );
            loop_note(&mut out, &hat, start, 0.16);
        }
    }
    for (start, note) in [
        (0.5_f32, 587.25_f32),
        (1.25, 523.25),
        (2.5, 698.5),
        (3.25, 659.25),
    ] {
        let mut stab = Osc::new(Wave::Square).with_phase(rng.unit());
        let hit = render(0.3, |t| stab.next(note) * perc(t, 0.003, 0.25) * 0.4);
        loop_note(&mut out, &hit, start, 0.18);
    }
    out
}

/// A boss's theme: slow brass in a dark mode over war drums and a pulsing low drone.
pub(super) fn hx_boss_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 240.0, 1.4);
    let mut out = render(loop_length(), |t| {
        let gate = 0.75 + 0.25 * sine(t, 1.0);
        filter.run(drone.next(55.0), 220.0 + 90.0 * sine(t, 0.5), 1.4) * gate * 0.3
    });
    let step = LOOP_SECONDS / 8.0;
    for index in 0..8_usize {
        #[allow(clippy::cast_precision_loss, reason = "eight steps")]
        let start = index as f32 * step;
        loop_note(&mut out, &thump(110.0, 40.0, 0.4, 2.0), start, 0.55);
        if index % 2 == 1 {
            let snare = burst(
                rng.next_u64(),
                NoiseColor::White,
                FilterMode::BandPass,
                1_600.0,
                0.7,
                0.14,
            );
            loop_note(&mut out, &snare, start, 0.22);
        }
    }
    for (start, note) in [
        (0.0_f32, 220.0_f32),
        (1.0, 233.0),
        (2.0, 196.0),
        (3.0, 207.75),
    ] {
        loop_note(&mut out, &brass(rng, note, 0.8, 0.65), start, 0.17);
    }
    out
}
