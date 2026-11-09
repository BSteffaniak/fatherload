//! Procedural audio (plan §10): a small synth engine renders every sound effect in code at
//! startup, a software mixer plays them on raylib's audio thread, and a director turns each
//! frame's cues and snapshot into mixer commands. No audio assets.
//!
//! - [`dsp`] — oscillators, noise, envelopes, filters, FM, waveshaping, delay and reverb.
//! - [`patch`] — the designed patches (see `audio/README.md` for the design table).
//! - [`bank`] — every patch's variations pre-rendered to sample buffers.
//! - [`mixer`] — voices, loops, pan, priorities and stealing, ducking, reverb bus, limiter.
//! - [`cues`] — cue -> patch and gameplay event -> cue tables, cue sources.
//! - [`director`] — continuous sounds, spatialization and alarm logic.
//! - [`analysis`] — level/spectrum/click measurements used by the tests and the render tool.
//! - [`render_tool`] — `drillgame --render-audio <dir>`.

pub mod analysis;
pub mod bank;
pub mod cues;
pub mod director;
pub mod dsp;
pub mod mixer;
pub mod patch;
pub mod render_tool;

use std::sync::{Arc, Mutex};

use raylib::{
    core::callbacks::audio_stream_callback::{
        set_audio_stream_callback, unset_audio_stream_callback,
    },
    prelude::*,
};

use self::{
    bank::{DEFAULT_BANK_SEED, SoundBank},
    director::{AudioFrame, Director},
    dsp::SAMPLE_RATE,
    mixer::Mixer,
};

/// Frames raylib asks the mixer for at a time (~23 ms at 44.1 kHz).
const STREAM_BUFFER_FRAMES: i32 = 1_024;

/// The desktop audio device: a stereo `f32` stream whose callback pulls frames from the shared
/// [`Mixer`], plus the [`Director`] that feeds it each frame.
pub struct AudioBus {
    stream: AudioStream<'static>,
    mixer: Arc<Mutex<Mixer>>,
    director: Director,
}

impl AudioBus {
    /// Opens the audio device and starts rendering the sound bank in the background (sounds
    /// requested before it is ready are skipped).
    ///
    /// # Errors
    ///
    /// When the audio device cannot be opened or the stream callback cannot be installed.
    pub fn new() -> Result<Self, String> {
        let audio: &'static RaylibAudio = Box::leak(Box::new(
            RaylibAudio::init_audio_device().map_err(|error| error.to_string())?,
        ));
        // Volume is applied (smoothed) inside the mixer.
        audio.set_master_volume(1.0);
        audio.set_audio_stream_buffer_size_default(STREAM_BUFFER_FRAMES);
        let stream = audio.new_audio_stream(SAMPLE_RATE, 32, 2);
        let mixer = Arc::new(Mutex::new(Mixer::new(DEFAULT_BANK_SEED)));
        let shared = Arc::clone(&mixer);
        set_audio_stream_callback::<f32, _>(&stream, move |buffer: &mut [f32]| {
            match shared.lock() {
                Ok(mut mixer) => mixer.render(buffer),
                Err(_) => buffer.fill(0.0),
            }
        })
        .map_err(|error| error.to_string())?;
        stream.play();
        let loader = Arc::clone(&mixer);
        std::thread::Builder::new()
            .name("drillgame-sound-bank".into())
            .spawn(move || {
                let bank = Arc::new(SoundBank::render(DEFAULT_BANK_SEED));
                if let Ok(mut mixer) = loader.lock() {
                    mixer.set_bank(bank);
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            stream,
            mixer,
            director: Director::new(DEFAULT_BANK_SEED),
        })
    }

    /// Plays this frame: places its cues, steers continuous sounds and applies `volume` (0..1).
    pub fn update(&mut self, frame: &AudioFrame<'_, '_>, volume: f32) {
        let commands = self.director.update(frame);
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.set_volume(volume);
            mixer.apply(commands);
        }
    }
}

impl Drop for AudioBus {
    fn drop(&mut self) {
        // Stop raylib calling into the mixer before the stream goes away.
        unset_audio_stream_callback(&self.stream);
    }
}

#[cfg(test)]
mod tests {
    //! Quality gates over every finished patch: the stand-in for a listening pass.

    use super::{
        analysis::{Measurements, loudness, seam_ratio},
        dsp::{db_to_gain, gain_to_db},
        patch::{Category, PEAK_CEILING, Patch, render_patch, variation_seed},
    };

    #[test]
    fn every_patch_meets_the_quality_bar() {
        for &patch in Patch::ALL {
            let samples = render_patch(patch, variation_seed(11, patch, 0));
            let name = patch.name();
            let info = patch.info();
            assert!(!samples.is_empty(), "{name} is empty");
            assert!(samples.iter().all(|s| s.is_finite()), "{name} has NaN/inf");
            let measured = Measurements::of(&samples);
            assert!(
                measured.peak_db <= gain_to_db(PEAK_CEILING) + 0.01,
                "{name} peaks at {:.2} dBFS",
                measured.peak_db
            );
            assert!(measured.dc.abs() < 0.002, "{name} has DC {}", measured.dc);
            // On target, unless the peak ceiling forced it quieter (very transient sounds).
            let target = info.category.target_db();
            let limited = measured.peak_db > gain_to_db(PEAK_CEILING) - 0.1;
            assert!(
                (measured.loudness_db - target).abs() < 0.5
                    || (limited && measured.loudness_db < target),
                "{name} loudness {:.1} dB vs target {target}",
                measured.loudness_db
            );
            if patch.looped() {
                assert!(
                    seam_ratio(&samples) < 2.0,
                    "{name} clicks at its loop point ({:.2})",
                    seam_ratio(&samples)
                );
                assert!((measured.seconds - super::patch::LOOP_SECONDS).abs() < 0.01);
            } else {
                assert!(measured.edge < 1e-4, "{name} starts or ends off zero");
                let (shortest, longest) = match info.category {
                    Category::Interface => (0.02, 0.8),
                    Category::Feedback | Category::Alarm | Category::Impact => (0.02, 2.0),
                    Category::Big => (0.8, 4.0),
                    Category::Loop => (0.0, f32::MAX),
                };
                assert!(
                    (shortest..=longest).contains(&measured.seconds),
                    "{name} lasts {:.2} s",
                    measured.seconds
                );
            }
            // A sensible brightness: nothing is pure sub-bass or pure hiss except by design.
            let (low, high) = match patch {
                Patch::GasHissLoop => (3_000.0, 12_000.0),
                // Thuds: their weight is in the low end, a clunk on top.
                Patch::BoulderRumbleLoop
                | Patch::LandLight
                | Patch::LandMedium
                | Patch::LandHeavy
                | Patch::WallBump
                | Patch::BreakSoft
                | Patch::BreakHard
                | Patch::Deploy
                | Patch::ThrustIgnite
                | Patch::Explosion
                | Patch::ThrustLoop => (40.0, 3_000.0),
                Patch::FuseTick | Patch::DrillGlint | Patch::UiMove | Patch::Pickup => {
                    (1_500.0, 12_000.0)
                }
                _ => (150.0, 8_000.0),
            };
            assert!(
                (low..=high).contains(&measured.centroid_hz),
                "{name} centroid {:.0} Hz outside {low}..{high}",
                measured.centroid_hz
            );
        }
    }

    #[test]
    fn variations_differ_but_stay_level_matched() {
        for &patch in &[
            Patch::HullHit,
            Patch::Coins,
            Patch::DrillCrunch,
            Patch::UiMove,
        ] {
            let levels: Vec<f32> = (0..patch.info().variations)
                .map(|variation| {
                    loudness(&render_patch(patch, variation_seed(5, patch, variation)))
                })
                .collect();
            let (min, max) = levels
                .iter()
                .fold((f32::MAX, 0.0_f32), |(lo, hi), &l| (lo.min(l), hi.max(l)));
            assert!(
                max / min < db_to_gain(3.0),
                "{} variations spread {:.1} dB",
                patch.name(),
                gain_to_db(max / min)
            );
        }
    }
}
