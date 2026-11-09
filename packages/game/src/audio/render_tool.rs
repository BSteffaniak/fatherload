//! `fatherload --render-audio <dir> [--seed N]`: writes every patch's variations to WAV files for
//! listening review (loops are written twice over so the seam is audible), a scripted
//! `showcase.wav` played through the real mixer (pan, reverb, ducking, loops), and a level report
//! (`levels.tsv`, also printed) with the measurements the quality tests check.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    reason = "WAV encoding converts float samples and counts to integer fields; the showcase is \
              one long readable script"
)]

use std::{
    fmt::Write as _,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{
    analysis::Measurements,
    bank::{DEFAULT_BANK_SEED, SoundBank},
    dsp::{Rng, SAMPLE_RATE, SR, gain_to_db},
    mixer::{LoopKey, LoopRequest, Mixer, MixerCommand, OPEN_CUTOFF, PlayRequest},
    patch::{PEAK_CEILING, Patch},
};

/// Parsed `--render-audio` options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderAudioOptions {
    pub dir: PathBuf,
    pub seed: u64,
}

/// `Ok(None)` when `--render-audio` is absent.
///
/// # Errors
///
/// When the output directory or the seed is missing or malformed.
pub fn parse_args(args: &[String]) -> Result<Option<RenderAudioOptions>, String> {
    let Some(position) = args.iter().position(|arg| arg == "--render-audio") else {
        return Ok(None);
    };
    let dir = args
        .get(position + 1)
        .filter(|value| !value.starts_with("--"))
        .ok_or("--render-audio needs an output directory")?;
    let seed = match args.iter().position(|arg| arg == "--seed") {
        Some(seed_position) => args
            .get(seed_position + 1)
            .ok_or("--seed needs a value")?
            .parse()
            .map_err(|_| "--seed must be a whole number".to_owned())?,
        None => DEFAULT_BANK_SEED,
    };
    Ok(Some(RenderAudioOptions {
        dir: PathBuf::from(dir),
        seed,
    }))
}

/// Renders everything into `options.dir`, reporting to `out`.
///
/// # Errors
///
/// When a file cannot be written.
pub fn run(options: &RenderAudioOptions, out: &mut impl Write) -> Result<(), String> {
    fs::create_dir_all(&options.dir).map_err(|error| error.to_string())?;
    let bank = Arc::new(SoundBank::render(options.seed));
    let mut report = String::from(
        "patch\tvariation\tcategory\tseconds\tpeak_db\tloudness_db\ttarget_db\trms_db\tdc\tcentroid_hz\tseam\tflags\n",
    );
    let mut flagged = 0;
    for &patch in Patch::ALL {
        for variation in 0..bank.variations(patch) {
            let Some(samples) = bank.get(patch, variation) else {
                continue;
            };
            let measured = Measurements::of(samples);
            let flags = flags(patch, &measured);
            if !flags.is_empty() {
                flagged += 1;
            }
            let _ = writeln!(
                report,
                "{}\t{variation}\t{}\t{:.3}\t{:.2}\t{:.2}\t{:.1}\t{:.2}\t{:.5}\t{:.0}\t{}\t{}",
                patch.name(),
                patch.info().category.label(),
                measured.seconds,
                measured.peak_db,
                measured.loudness_db,
                patch.info().category.target_db(),
                measured.rms_db,
                measured.dc,
                measured.centroid_hz,
                if patch.looped() {
                    format!("{:.2}", measured.seam_ratio)
                } else {
                    "-".to_owned()
                },
                flags.join(","),
            );
            let audible: Vec<f32> = if patch.looped() {
                samples.iter().chain(samples.iter()).copied().collect()
            } else {
                samples.to_vec()
            };
            let path = options
                .dir
                .join(format!("{}_{variation}.wav", patch.name()));
            write_wav(&path, &audible, 1)?;
        }
    }
    let showcase = showcase(Arc::clone(&bank), options.seed);
    let showcase_peak = showcase.iter().fold(0.0_f32, |max, s| max.max(s.abs()));
    write_wav(&options.dir.join("showcase.wav"), &showcase, 2)?;
    fs::write(options.dir.join("levels.tsv"), &report).map_err(|error| error.to_string())?;
    let _ = write!(out, "{report}");
    let _ = writeln!(
        out,
        "\n{} patches, {} files, {:.1} MB of samples; showcase {:.1} s peaking {:.2} dBFS; {flagged} flagged",
        Patch::ALL.len(),
        Patch::ALL
            .iter()
            .map(|patch| bank.variations(*patch))
            .sum::<usize>(),
        bank.total_samples() as f32 * 4.0 / 1_048_576.0,
        showcase.len() as f32 / 2.0 / SR,
        gain_to_db(showcase_peak),
    );
    let _ = writeln!(out, "Wrote {}", options.dir.display());
    Ok(())
}

/// Problems the report calls out.
fn flags(patch: Patch, measured: &Measurements) -> Vec<&'static str> {
    let mut flags = Vec::new();
    if measured.peak_db > gain_to_db(PEAK_CEILING) + 0.01 {
        flags.push("CLIP");
    }
    if measured.dc.abs() > 0.002 {
        flags.push("DC");
    }
    if patch.looped() {
        if measured.seam_ratio > 2.0 {
            flags.push("SEAM");
        }
    } else if measured.edge > 1e-4 {
        flags.push("EDGE");
    }
    let target = patch.info().category.target_db();
    let limited = measured.peak_db > gain_to_db(PEAK_CEILING) - 0.1;
    if (measured.loudness_db - target).abs() > 0.5 && !(limited && measured.loudness_db < target) {
        flags.push("LEVEL");
    }
    flags
}

/// Writes 16-bit PCM WAV (`samples` interleaved for `channels`).
///
/// # Errors
///
/// When the file cannot be written.
pub fn write_wav(path: &Path, samples: &[f32], channels: u16) -> Result<(), String> {
    let data_len = u32::try_from(samples.len() * 2).map_err(|_| "audio too long".to_owned())?;
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let pcm = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

/// A ~36 s scripted tour through the real mixer: menus, a flight, drilling through dirt, rock
/// and ore, a bomb, damage and alarms, hazards, discoveries and rewards.
#[must_use]
pub fn showcase(bank: Arc<SoundBank>, seed: u64) -> Vec<f32> {
    const FPS: f32 = 60.0;
    const LENGTH: f32 = 37.0;
    let mut mixer = Mixer::new(seed);
    mixer.set_bank(bank);
    let mut rng = Rng::new(seed ^ 0x5EED);
    let mut out = Vec::new();
    let mut commands = Vec::new();
    let frames = (LENGTH * FPS) as u32;
    let mut crunch_at = 0.0;
    let mut glint_at = 0.0;
    let mut fuse_at: f32 = 13.2;
    for frame in 0..frames {
        let t = frame as f32 / FPS;
        let dt = 1.0 / FPS;
        let at = |moment: f32| t <= moment && moment < t + dt;
        commands.clear();
        let mut play = |patch: Patch, gain: f32, pan: f32| {
            commands.push(MixerCommand::Play(PlayRequest {
                gain,
                pan,
                ..PlayRequest::new(patch)
            }));
        };
        // Menus.
        for moment in [0.1, 0.3, 0.5] {
            if at(moment) {
                play(Patch::UiMove, 1.0, 0.0);
            }
        }
        if at(0.8) {
            play(Patch::UiConfirm, 1.0, 0.0);
        }
        if at(1.2) {
            play(Patch::UiBack, 1.0, 0.0);
        }
        if at(1.6) {
            play(Patch::UiError, 1.0, 0.0);
        }
        // Flight.
        if at(2.2) {
            play(Patch::ThrustIgnite, 0.45, 0.0);
        }
        let throttle = ramp(t, 2.2, 2.6, 5.4, 5.8);
        let climb = ramp(t, 2.8, 3.6, 4.6, 5.4);
        let mut loops = vec![(
            0,
            Patch::ThrustLoop,
            throttle * (0.55 + 0.45 * climb),
            0.0,
            0.85 + 0.15 * throttle + 0.1 * climb,
            700.0 + 9_000.0 * throttle.powf(1.5) * (0.7 + 0.3 * climb),
        )];
        if at(6.3) {
            play(Patch::LandMedium, 0.8, 0.0);
        }
        if at(6.9) {
            play(Patch::WallBump, 0.6, 0.1);
        }
        // Drilling: dirt, then stone, then ore.
        let soft = ramp(t, 7.4, 7.5, 10.2, 10.3);
        let hard = ramp(t, 10.6, 10.7, 14.9, 15.0);
        loops.push((1, Patch::DrillSoftLoop, soft * 0.9, 0.0, 1.0, OPEN_CUTOFF));
        loops.push((
            1,
            Patch::DrillHardLoop,
            hard * 0.9,
            0.0,
            if t > 12.8 { 0.92 } else { 1.05 },
            OPEN_CUTOFF,
        ));
        if at(7.4) || at(10.6) {
            play(Patch::DrillBite, 0.6, 0.0);
        }
        if soft > 0.5 && t >= crunch_at {
            commands.push(MixerCommand::Play(PlayRequest {
                gain: rng.range(0.25, 0.4),
                rate: rng.range(0.5, 0.65),
                ..PlayRequest::new(Patch::DrillCrunch)
            }));
            crunch_at = t + rng.range(0.3, 0.6);
        }
        if hard > 0.5 && t >= crunch_at {
            commands.push(MixerCommand::Play(PlayRequest {
                gain: rng.range(0.4, 0.8),
                rate: rng.range(0.9, 1.15),
                ..PlayRequest::new(Patch::DrillCrunch)
            }));
            crunch_at = t + rng.range(0.15, 0.4);
        }
        if hard > 0.5 && t > 12.8 && t >= glint_at {
            commands.push(MixerCommand::Play(PlayRequest {
                gain: rng.range(0.35, 0.6),
                ..PlayRequest::new(Patch::DrillGlint)
            }));
            glint_at = t + rng.range(0.25, 0.6);
        }
        let mut play = |patch: Patch, gain: f32, pan: f32| {
            commands.push(MixerCommand::Play(PlayRequest {
                gain,
                pan,
                ..PlayRequest::new(patch)
            }));
        };
        if at(10.3) {
            play(Patch::BreakSoft, 0.8, 0.0);
        }
        if at(12.7) {
            play(Patch::BreakHard, 0.8, 0.0);
        }
        if at(15.0) {
            play(Patch::BreakHard, 0.8, 0.0);
        }
        if at(15.2) {
            play(Patch::Pickup, 1.0, 0.0);
        }
        // A bomb off to the right: arming, a quickening fuse, the blast, loosened boulders.
        if at(16.0) {
            play(Patch::BombArm, 1.0, 0.4);
        }
        if (16.2..18.6).contains(&t) && t >= fuse_at.max(16.2) {
            let left = 18.6 - t;
            commands.push(MixerCommand::Play(PlayRequest {
                gain: 0.7,
                pan: 0.4,
                rate: 1.0 + (1.0 - left / 2.4) * 0.2,
                ..PlayRequest::new(Patch::FuseTick)
            }));
            fuse_at = t + (left * 0.22).clamp(0.08, 0.5);
        }
        let mut play = |patch: Patch, gain: f32, pan: f32| {
            commands.push(MixerCommand::Play(PlayRequest {
                gain,
                pan,
                ..PlayRequest::new(patch)
            }));
        };
        if at(18.6) {
            play(Patch::Explosion, 1.0, 0.4);
        }
        loops.push((
            2,
            Patch::BoulderRumbleLoop,
            ramp(t, 19.0, 19.4, 21.0, 21.6) * 0.8,
            0.3,
            1.0,
            8_000.0,
        ));
        // Damage and escalating alarms.
        if at(21.8) {
            play(Patch::HullHit, 1.0, 0.0);
        }
        for moment in [22.2, 24.4] {
            if at(moment) {
                play(Patch::HullLow, 0.8, 0.0);
            }
        }
        for moment in [25.6, 26.6] {
            if at(moment) {
                play(Patch::HullCritical, 0.8, 0.0);
            }
        }
        if at(23.2) {
            play(Patch::FuelLow, 0.8, 0.0);
        }
        if at(27.4) {
            play(Patch::FuelCritical, 0.8, 0.0);
        }
        // Hazards: gas to the left, lava to the right.
        loops.push((
            3,
            Patch::GasHissLoop,
            ramp(t, 22.0, 22.3, 24.5, 25.0) * 0.6,
            -0.6,
            1.0,
            OPEN_CUTOFF,
        ));
        loops.push((
            4,
            Patch::LavaSizzleLoop,
            ramp(t, 24.5, 25.0, 27.5, 28.0) * 0.7,
            0.6,
            1.0,
            OPEN_CUTOFF,
        ));
        // Discoveries and rewards.
        if at(28.2) {
            play(Patch::ScannerPulse, 1.0, 0.0);
        }
        if at(29.4) {
            play(Patch::PoiDiscovered, 1.0, 0.0);
        }
        if at(31.0) {
            play(Patch::CrateOpen, 1.0, -0.2);
        }
        if at(31.6) {
            play(Patch::SalvageOpen, 1.0, 0.2);
        }
        if at(32.3) {
            play(Patch::Coins, 1.0, 0.0);
        }
        if at(33.0) {
            play(Patch::Purchase, 1.0, 0.0);
        }
        if at(33.6) {
            play(Patch::Toast, 1.0, 0.0);
        }
        if at(34.0) {
            play(Patch::VaultFanfare, 1.0, 0.0);
        }
        if at(35.0) {
            play(Patch::Deploy, 1.0, -0.3);
        }
        if at(35.6) {
            play(Patch::VaultReset, 0.8, 0.0);
        }
        for (source, patch, gain, pan, rate, cutoff) in loops {
            commands.push(MixerCommand::Loop(LoopRequest {
                key: LoopKey::new(source, patch),
                patch,
                gain,
                pan,
                rate,
                cutoff,
            }));
        }
        mixer.apply(&commands);
        out.extend(mixer.render_seconds(dt));
    }
    // Let the last tails ring out, then the rescue and milestone on top.
    mixer.apply(&[MixerCommand::Play(PlayRequest::new(Patch::Rescue))]);
    out.extend(mixer.render_seconds(1.8));
    mixer.apply(&[MixerCommand::Play(PlayRequest::new(Patch::Milestone))]);
    out.extend(mixer.render_seconds(2.5));
    out
}

/// 0 before `start`, rising to 1 by `full`, holding until `release`, back to 0 by `end`.
fn ramp(t: f32, start: f32, full: f32, release: f32, end: f32) -> f32 {
    if t <= start || t >= end {
        0.0
    } else if t < full {
        (t - start) / (full - start)
    } else if t <= release {
        1.0
    } else {
        1.0 - (t - release) / (end - release)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_output_directory_and_seed() {
        let args = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(parse_args(&args(&["--other"])), Ok(None));
        assert_eq!(
            parse_args(&args(&["--render-audio", "out", "--seed", "9"])),
            Ok(Some(RenderAudioOptions {
                dir: PathBuf::from("out"),
                seed: 9
            }))
        );
        assert!(parse_args(&args(&["--render-audio"])).is_err());
    }

    #[test]
    fn wav_files_have_a_valid_header() {
        let dir = std::env::temp_dir().join(format!("fatherload-wav-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("tone.wav");
        write_wav(&path, &[0.0, 0.5, -0.5, 1.0], 2).expect("write");
        let bytes = fs::read(&path).expect("read");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(bytes.len(), 44 + 8);
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
        let _ = fs::remove_dir_all(&dir);
    }
}
