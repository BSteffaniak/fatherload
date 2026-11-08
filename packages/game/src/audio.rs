#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "procedural audio converts bounded sample positions to PCM values"
)]

use std::{fs, path::PathBuf};

use raylib::prelude::*;

use crate::shell::SoundCue;

pub struct AudioBus {
    audio: &'static RaylibAudio,
    drill: Sound<'static>,
    sell: Sound<'static>,
    upgrade: Sound<'static>,
    damage: Sound<'static>,
    milestone: Sound<'static>,
    rescue: Sound<'static>,
    ui: Sound<'static>,
}

impl AudioBus {
    pub fn new() -> Result<Self, String> {
        let audio = Box::leak(Box::new(
            RaylibAudio::init_audio_device().map_err(|error| error.to_string())?,
        ));
        audio.set_master_volume(0.8);
        Ok(Self {
            audio,
            drill: sound(audio, 130.0, 0.09).ok_or("generated drill sound failed")?,
            sell: sound(audio, 680.0, 0.16).ok_or("generated sell sound failed")?,
            upgrade: sound(audio, 880.0, 0.18).ok_or("generated upgrade sound failed")?,
            damage: sound(audio, 90.0, 0.22).ok_or("generated damage sound failed")?,
            milestone: sound(audio, 520.0, 0.28).ok_or("generated milestone sound failed")?,
            rescue: sound(audio, 250.0, 0.36).ok_or("generated rescue sound failed")?,
            ui: sound(audio, 980.0, 0.06).ok_or("generated ui sound failed")?,
        })
    }

    pub fn set_volume(&self, volume: f32) {
        self.audio.set_master_volume(volume.clamp(0.0, 1.0));
    }

    pub fn play(&self, cues: &[SoundCue]) {
        for cue in cues {
            match cue {
                SoundCue::Drill => self.drill.play(),
                SoundCue::Sell => self.sell.play(),
                SoundCue::Upgrade => self.upgrade.play(),
                SoundCue::Damage => self.damage.play(),
                SoundCue::Milestone => self.milestone.play(),
                SoundCue::Rescue | SoundCue::Explosion => self.rescue.play(),
                SoundCue::Ui => self.ui.play(),
            }
        }
    }
}

fn sound(audio: &'static RaylibAudio, frequency: f32, seconds: f32) -> Option<Sound<'static>> {
    let wave_bytes = wav_bytes(frequency, seconds);
    let path = sound_path(frequency, seconds);
    fs::write(&path, wave_bytes).ok()?;
    audio.new_sound(path.to_str()?).ok()
}

fn sound_path(frequency: f32, seconds: f32) -> PathBuf {
    let mut path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    path.push("target");
    path.push("drillgame-audio");
    let _ = fs::create_dir_all(&path);
    path.push(format!(
        "drillgame-{frequency:.0}hz-{duration_ms:.0}ms.wav",
        duration_ms = seconds * 1_000.0
    ));
    path
}

fn wav_bytes(frequency: f32, seconds: f32) -> Vec<u8> {
    let sample_rate = 22_050_u32;
    let samples = (sample_rate as f32 * seconds) as u32;
    let data_len = samples * 2;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());

    for sample in 0..samples {
        let t = sample as f32 / sample_rate as f32;
        let envelope = 1.0 - sample as f32 / samples as f32;
        let value = (t * frequency * std::f32::consts::TAU).sin() * envelope * 0.35;
        let pcm = (value * f32::from(i16::MAX)) as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }

    bytes
}
