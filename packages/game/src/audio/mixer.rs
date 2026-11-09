//! The real-time software mixer: plays pre-rendered patches as voices with per-voice gain,
//! stereo pan, playback rate and low-pass, keeps loops alive while they are refreshed, limits
//! voices by priority, ducks quieter sounds under set-pieces and runs a shared reverb bus and a
//! master limiter. It renders interleaved stereo `f32` frames; the desktop app pulls them from
//! raylib's audio thread, the `--render-audio` tool and the tests render offline.
//!
//! The render path does not allocate: voices live in a fixed slot array and parameters are
//! smoothed per 64-frame block, then ramped linearly across it (no zipper noise or clicks).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "the mixer converts sample positions and counts between integer and float domains"
)]

use std::sync::Arc;

use super::{
    bank::SoundBank,
    dsp::{FilterMode, Reverb, Rng, SR, Svf, db_to_gain, hermite, pan_gains},
    patch::Patch,
};

/// Voices that may sound at once (more requests steal by priority).
pub const MAX_VOICES: usize = 24;
/// Slots: voices plus room for stolen voices to fade out without clicking.
const VOICE_SLOTS: usize = MAX_VOICES + 8;
/// Frames per parameter-smoothing block.
const BLOCK: usize = 64;
/// A cutoff at or above this is treated as "filter open".
pub const OPEN_CUTOFF: f32 = 18_000.0;
/// Fade applied to a stolen voice (seconds).
const STEAL_FADE_SECONDS: f32 = 0.006;
/// A loop not refreshed for this long fades out on its own (seconds).
const LOOP_KEEPALIVE_SECONDS: f32 = 0.35;
/// Master limiter ceiling (linear).
const LIMITER_CEILING: f32 = 0.95;
/// Wet level of the reverb bus.
const REVERB_RETURN: f32 = 0.55;

/// Plays a one-shot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayRequest {
    pub patch: Patch,
    /// Linear gain (before the patch's random gain jitter).
    pub gain: f32,
    /// Stereo position, -1 (left) to 1 (right).
    pub pan: f32,
    /// Playback rate (pitch), 1 = as rendered.
    pub rate: f32,
    /// Low-pass cutoff in Hz ([`OPEN_CUTOFF`] or above = unfiltered); distance muffling.
    pub cutoff: f32,
}

impl PlayRequest {
    /// Centred, unity gain and rate, unfiltered.
    #[must_use]
    pub const fn new(patch: Patch) -> Self {
        Self {
            patch,
            gain: 1.0,
            pan: 0.0,
            rate: 1.0,
            cutoff: 20_000.0,
        }
    }
}

/// Identifies a continuous sound across frames (who is making it and which patch).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LoopKey(pub u32);

impl LoopKey {
    /// The loop of `patch` owned by `source` (a seat, a hazard...).
    #[must_use]
    pub const fn new(source: u32, patch: Patch) -> Self {
        Self((source << 8) | (patch.index() as u32 & 0xFF))
    }
}

/// Starts, steers or (with zero gain) fades out a loop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopRequest {
    pub key: LoopKey,
    pub patch: Patch,
    pub gain: f32,
    pub pan: f32,
    pub rate: f32,
    pub cutoff: f32,
}

/// Everything the game can ask of the mixer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MixerCommand {
    Play(PlayRequest),
    Loop(LoopRequest),
    /// Fades every loop out (pause, world replaced).
    StopLoops,
}

#[derive(Clone, Debug)]
struct Voice {
    active: bool,
    patch: Patch,
    variation: usize,
    looped: bool,
    loop_key: Option<LoopKey>,
    position: f64,
    priority: u8,
    send: f32,
    started: u64,
    refreshed: u64,
    gain: f32,
    gain_target: f32,
    pan: f32,
    pan_target: f32,
    rate: f32,
    rate_target: f32,
    cutoff: f32,
    cutoff_target: f32,
    filter: Svf,
    /// 1 while playing; ramps to 0 when stolen.
    kill: f32,
    killing: bool,
    /// Gains applied at the end of the last block (ramp start of the next).
    last_left: f32,
    last_right: f32,
    last_send: f32,
}

impl Voice {
    fn idle() -> Self {
        Self {
            active: false,
            patch: Patch::UiMove,
            variation: 0,
            looped: false,
            loop_key: None,
            position: 0.0,
            priority: 0,
            send: 0.0,
            started: 0,
            refreshed: 0,
            gain: 0.0,
            gain_target: 0.0,
            pan: 0.0,
            pan_target: 0.0,
            rate: 1.0,
            rate_target: 1.0,
            cutoff: 20_000.0,
            cutoff_target: 20_000.0,
            filter: Svf::new(FilterMode::LowPass, 20_000.0, 0.707),
            kill: 1.0,
            killing: false,
            last_left: 0.0,
            last_right: 0.0,
            last_send: 0.0,
        }
    }
}

/// The software mixer. See the module docs.
#[derive(Debug)]
pub struct Mixer {
    bank: Option<Arc<SoundBank>>,
    voices: Vec<Voice>,
    reverb: Reverb,
    rng: Rng,
    last_variation: Vec<usize>,
    clock: u64,
    volume: f32,
    volume_target: f32,
    duck: f32,
    duck_target: f32,
    duck_priority: u8,
    duck_hold: u64,
    limiter: f32,
    /// Peak absolute output sample since the last [`Mixer::take_peak`].
    peak: f32,
}

impl Mixer {
    /// A silent mixer; sounds play once a bank is attached.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            bank: None,
            voices: (0..VOICE_SLOTS).map(|_| Voice::idle()).collect(),
            reverb: Reverb::new(1.05),
            rng: Rng::new(seed),
            last_variation: vec![usize::MAX; Patch::ALL.len()],
            clock: 0,
            volume: 1.0,
            volume_target: 1.0,
            duck: 0.0,
            duck_target: 0.0,
            duck_priority: 0,
            duck_hold: 0,
            limiter: 0.0,
            peak: 0.0,
        }
    }

    /// Attaches the rendered sounds.
    pub fn set_bank(&mut self, bank: Arc<SoundBank>) {
        self.bank = Some(bank);
    }

    /// Output volume (0..1), smoothed so changes never click.
    pub const fn set_volume(&mut self, volume: f32) {
        self.volume_target = volume.clamp(0.0, 1.0);
    }

    /// Applies a batch of commands.
    pub fn apply(&mut self, commands: &[MixerCommand]) {
        for command in commands {
            match *command {
                MixerCommand::Play(request) => self.play(request),
                MixerCommand::Loop(request) => self.steer_loop(request),
                MixerCommand::StopLoops => {
                    for voice in &mut self.voices {
                        if voice.active && voice.looped {
                            voice.gain_target = 0.0;
                        }
                    }
                }
            }
        }
    }

    /// Starts a one-shot (dropped if every voice is busy with something more important).
    pub fn play(&mut self, request: PlayRequest) {
        let info = request.patch.info();
        let Some(variations) = self.bank_variations(request.patch) else {
            return;
        };
        if request.gain <= 1e-4 {
            return;
        }
        let Some(slot) = self.allocate(request.patch, info.priority, info.max_instances) else {
            return;
        };
        let variation = self.pick_variation(request.patch, variations);
        let rate = request.rate * self.rng.jitter(info.pitch_jitter);
        let gain = request.gain * db_to_gain(self.rng.bipolar() * info.gain_jitter_db);
        if info.ducks > 0.0 && gain > 0.2 {
            self.duck_target = self.duck_target.max(info.ducks * gain.min(1.0));
            self.duck_priority = self.duck_priority.max(info.priority);
            self.duck_hold = self.clock + (SR * 0.35) as u64;
        }
        let clock = self.clock;
        let voice = &mut self.voices[slot];
        *voice = Voice {
            active: true,
            patch: request.patch,
            variation,
            looped: false,
            loop_key: None,
            position: 0.0,
            priority: info.priority,
            send: info.reverb_send,
            started: clock,
            refreshed: clock,
            gain,
            gain_target: gain,
            pan: request.pan,
            pan_target: request.pan,
            rate,
            rate_target: rate,
            cutoff: request.cutoff,
            cutoff_target: request.cutoff,
            filter: Svf::new(FilterMode::LowPass, request.cutoff.min(OPEN_CUTOFF), 0.707),
            kill: 1.0,
            killing: false,
            last_left: 0.0,
            last_right: 0.0,
            last_send: 0.0,
        };
        // A one-shot starts at its full gain: its own attack is in the buffer.
        let (left, right) = pan_gains(request.pan);
        voice.last_left = left * gain;
        voice.last_right = right * gain;
        voice.last_send = gain * info.reverb_send;
    }

    fn steer_loop(&mut self, request: LoopRequest) {
        let clock = self.clock;
        if let Some(voice) = self
            .voices
            .iter_mut()
            .find(|voice| voice.active && !voice.killing && voice.loop_key == Some(request.key))
        {
            voice.gain_target = request.gain.max(0.0);
            voice.pan_target = request.pan;
            voice.rate_target = request.rate;
            voice.cutoff_target = request.cutoff;
            voice.refreshed = clock;
            return;
        }
        if request.gain <= 1e-4 {
            return;
        }
        let info = request.patch.info();
        let Some(variations) = self.bank_variations(request.patch) else {
            return;
        };
        let Some(slot) = self.allocate(request.patch, info.priority, info.max_instances) else {
            return;
        };
        let variation = self.pick_variation(request.patch, variations);
        let length = self
            .bank
            .as_ref()
            .and_then(|bank| bank.get(request.patch, variation))
            .map_or(1, <[f32]>::len);
        // Start somewhere random so two seats' identical loops never phase against each other.
        let start = f64::from(self.rng.unit()) * length as f64;
        self.voices[slot] = Voice {
            active: true,
            patch: request.patch,
            variation,
            looped: true,
            loop_key: Some(request.key),
            position: start,
            priority: info.priority,
            send: info.reverb_send,
            started: clock,
            refreshed: clock,
            gain: 0.0,
            gain_target: request.gain,
            pan: request.pan,
            pan_target: request.pan,
            rate: request.rate,
            rate_target: request.rate,
            cutoff: request.cutoff,
            cutoff_target: request.cutoff,
            filter: Svf::new(FilterMode::LowPass, request.cutoff.min(OPEN_CUTOFF), 0.707),
            kill: 1.0,
            killing: false,
            last_left: 0.0,
            last_right: 0.0,
            last_send: 0.0,
        };
    }

    fn bank_variations(&self, patch: Patch) -> Option<usize> {
        let count = self.bank.as_ref()?.variations(patch);
        (count > 0).then_some(count)
    }

    /// A random variation, never the one this patch played last (when it has more than one).
    fn pick_variation(&mut self, patch: Patch, variations: usize) -> usize {
        let last = self.last_variation[patch.index()];
        let mut variation = self.rng.index(variations);
        if variations > 1 && variation == last {
            variation = (variation + 1 + self.rng.index(variations - 1)) % variations;
        }
        self.last_variation[patch.index()] = variation;
        variation
    }

    /// Finds a slot for a new voice of `patch`, stealing (with a short fade) the oldest instance
    /// past its patch's cap, or the least important voice when all voices are busy. `None` when
    /// everything playing outranks the newcomer.
    fn allocate(&mut self, patch: Patch, priority: u8, max_instances: u8) -> Option<usize> {
        let mut count = 0;
        let mut oldest: Option<usize> = None;
        for (index, voice) in self.voices.iter().enumerate() {
            if voice.active && !voice.killing && voice.patch == patch {
                count += 1;
                if oldest.is_none_or(|best| voice.started < self.voices[best].started) {
                    oldest = Some(index);
                }
            }
        }
        if count >= usize::from(max_instances.max(1))
            && let Some(index) = oldest
        {
            self.steal(index);
        }
        let sounding = self
            .voices
            .iter()
            .filter(|voice| voice.active && !voice.killing)
            .count();
        if sounding >= MAX_VOICES {
            let victim = self
                .voices
                .iter()
                .enumerate()
                .filter(|(_, voice)| voice.active && !voice.killing)
                .min_by(|(_, a), (_, b)| {
                    a.priority.cmp(&b.priority).then(a.started.cmp(&b.started))
                })
                .map(|(index, _)| index)?;
            if self.voices[victim].priority > priority {
                return None;
            }
            self.steal(victim);
        }
        if let Some(free) = self.voices.iter().position(|voice| !voice.active) {
            return Some(free);
        }
        // Every slot is busy fading: cut the quietest fade short.
        let quietest = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, voice)| voice.killing)
            .min_by(|(_, a), (_, b)| a.kill.total_cmp(&b.kill))
            .map(|(index, _)| index)?;
        self.voices[quietest].active = false;
        Some(quietest)
    }

    fn steal(&mut self, index: usize) {
        let voice = &mut self.voices[index];
        voice.killing = true;
        voice.loop_key = None;
    }

    /// Renders interleaved stereo frames into `out` (`out.len()` / 2 frames).
    pub fn render(&mut self, out: &mut [f32]) {
        for block in out.chunks_mut(BLOCK * 2) {
            self.render_block(block);
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the per-sample voice loop stays in one place so it is easy to keep allocation-free"
    )]
    fn render_block(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        out.fill(0.0);
        if frames == 0 {
            return;
        }
        let block_seconds = frames as f32 / SR;
        // Block-rate parameter smoothing.
        let volume_step = 1.0 - (-block_seconds / 0.05).exp();
        let previous_volume = self.volume;
        self.volume += (self.volume_target - self.volume) * volume_step;
        self.update_duck(block_seconds);
        let keepalive = (LOOP_KEEPALIVE_SECONDS * SR) as u64;
        let kill_step = block_seconds / STEAL_FADE_SECONDS;
        let Some(bank) = self.bank.clone() else {
            return;
        };
        let duck_gain = 1.0 - self.duck;
        let duck_priority = self.duck_priority;
        let mut send = [0.0_f32; BLOCK];
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }
            let Some(buffer) = bank.get(voice.patch, voice.variation) else {
                voice.active = false;
                continue;
            };
            if voice.looped && self.clock.saturating_sub(voice.refreshed) > keepalive {
                voice.gain_target = 0.0;
            }
            let rise = if voice.looped {
                if voice.gain_target > voice.gain {
                    0.06
                } else {
                    0.1
                }
            } else {
                0.02
            };
            voice.gain += (voice.gain_target - voice.gain) * (1.0 - (-block_seconds / rise).exp());
            let glide = 1.0 - (-block_seconds / 0.06).exp();
            voice.pan += (voice.pan_target - voice.pan) * glide;
            voice.rate += (voice.rate_target - voice.rate) * glide;
            voice.cutoff += (voice.cutoff_target - voice.cutoff) * glide;
            if voice.killing {
                voice.kill = (voice.kill - kill_step).max(0.0);
            }
            let ducked = if voice.priority < duck_priority {
                duck_gain
            } else {
                1.0
            };
            let level = voice.gain * voice.kill * ducked;
            let (pan_left, pan_right) = pan_gains(voice.pan);
            let (left_end, right_end, send_end) =
                (pan_left * level, pan_right * level, level * voice.send);
            // The filter always runs (only its output is bypassed while open), so a cutoff
            // gliding below OPEN_CUTOFF picks up a settled filter instead of one starting
            // from zero state mid-sound, which would click.
            let filtered = voice.cutoff < OPEN_CUTOFF;
            voice.filter.set(voice.cutoff.min(OPEN_CUTOFF), 0.707);
            let rate = f64::from(voice.rate.clamp(0.05, 4.0));
            let length = buffer.len() as f64;
            for frame in 0..frames {
                let ramp = (frame + 1) as f32 / frames as f32;
                let left = voice.last_left + (left_end - voice.last_left) * ramp;
                let right = voice.last_right + (right_end - voice.last_right) * ramp;
                let wet = voice.last_send + (send_end - voice.last_send) * ramp;
                let raw = hermite(buffer, voice.position, voice.looped);
                let low = voice.filter.process(raw);
                let sample = if filtered { low } else { raw };
                out[frame * 2] += sample * left;
                out[frame * 2 + 1] += sample * right;
                send[frame] += sample * wet;
                voice.position += rate;
                if voice.looped {
                    if voice.position >= length {
                        voice.position -= length;
                    }
                } else if voice.position >= length + 2.0 {
                    voice.active = false;
                    break;
                }
            }
            voice.last_left = left_end;
            voice.last_right = right_end;
            voice.last_send = send_end;
            let faded_loop = voice.looped && voice.gain_target <= 1e-4 && voice.gain < 1e-3;
            if (voice.killing && voice.kill <= 0.0) || faded_loop {
                voice.active = false;
            }
        }
        let release = (-1.0 / (0.12 * SR)).exp();
        for frame in 0..frames {
            let (wet_left, wet_right) = self.reverb.process(send[frame]);
            let ramp = (frame + 1) as f32 / frames as f32;
            let volume = previous_volume + (self.volume - previous_volume) * ramp;
            let mut left = (out[frame * 2] + wet_left * REVERB_RETURN) * volume;
            let mut right = (out[frame * 2 + 1] + wet_right * REVERB_RETURN) * volume;
            // Peak limiter: instant attack, smooth release.
            let level = left.abs().max(right.abs());
            self.limiter = if level > self.limiter {
                level
            } else {
                self.limiter * release + level * (1.0 - release)
            };
            if self.limiter > LIMITER_CEILING {
                let gain = LIMITER_CEILING / self.limiter;
                left *= gain;
                right *= gain;
            }
            left = left.clamp(-1.0, 1.0);
            right = right.clamp(-1.0, 1.0);
            self.peak = self.peak.max(left.abs()).max(right.abs());
            out[frame * 2] = left;
            out[frame * 2 + 1] = right;
        }
        self.clock += frames as u64;
    }

    fn update_duck(&mut self, block_seconds: f32) {
        if self.clock >= self.duck_hold {
            self.duck_target *= (-block_seconds / 0.25).exp();
            if self.duck_target < 1e-3 {
                self.duck_target = 0.0;
                self.duck_priority = 0;
            }
        }
        let speed = if self.duck_target > self.duck {
            0.01
        } else {
            0.08
        };
        self.duck += (self.duck_target - self.duck) * (1.0 - (-block_seconds / speed).exp());
    }

    /// Renders `seconds` of output offline (tools and tests).
    #[must_use]
    pub fn render_seconds(&mut self, seconds: f32) -> Vec<f32> {
        let mut out = vec![0.0; super::dsp::samples_for(seconds) * 2];
        self.render(&mut out);
        out
    }
}

/// Introspection for tests.
#[cfg(test)]
impl Mixer {
    /// Voices currently sounding (including ones fading out after being stolen).
    #[must_use]
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|voice| voice.active).count()
    }

    /// Voices of `patch` currently sounding (not counting stolen ones fading out).
    #[must_use]
    pub fn voices_of(&self, patch: Patch) -> usize {
        self.voices
            .iter()
            .filter(|voice| voice.active && !voice.killing && voice.patch == patch)
            .count()
    }

    /// The current gain target of loop `key`, if it is sounding.
    #[must_use]
    pub fn loop_gain(&self, key: LoopKey) -> Option<f32> {
        self.voices
            .iter()
            .find(|voice| voice.active && voice.loop_key == Some(key))
            .map(|voice| voice.gain)
    }

    /// How much quieter ducked voices currently are (0 = not ducked).
    #[must_use]
    pub const fn duck_depth(&self) -> f32 {
        self.duck
    }

    /// The loudest output sample since the last call.
    pub const fn take_peak(&mut self) -> f32 {
        let peak = self.peak;
        self.peak = 0.0;
        peak
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixer_with(patches: &[Patch]) -> Mixer {
        let mut mixer = Mixer::new(1);
        mixer.set_bank(Arc::new(SoundBank::render_only(3, patches)));
        mixer
    }

    #[test]
    fn voices_are_limited_and_output_never_clips() {
        let mut mixer = mixer_with(&[Patch::Explosion, Patch::UiMove, Patch::HullHit]);
        for _ in 0..60 {
            mixer.play(PlayRequest {
                gain: 4.0,
                ..PlayRequest::new(Patch::HullHit)
            });
            mixer.play(PlayRequest::new(Patch::UiMove));
            mixer.play(PlayRequest {
                gain: 4.0,
                ..PlayRequest::new(Patch::Explosion)
            });
        }
        assert!(mixer.active_voices() <= VOICE_SLOTS);
        assert!(mixer.voices_of(Patch::Explosion) <= 3);
        assert!(mixer.voices_of(Patch::HullHit) <= 2);
        let out = mixer.render_seconds(0.5);
        assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(mixer.take_peak() <= 1.0);
    }

    #[test]
    fn low_priority_sounds_cannot_steal_from_important_ones() {
        let mut mixer = mixer_with(&[Patch::Explosion, Patch::UiMove, Patch::ThrustLoop]);
        // Fill every voice with distinct high-priority loops.
        for source in 0..MAX_VOICES as u32 {
            mixer.apply(&[MixerCommand::Loop(LoopRequest {
                key: LoopKey::new(source, Patch::ThrustLoop),
                patch: Patch::ThrustLoop,
                gain: 0.5,
                pan: 0.0,
                rate: 1.0,
                cutoff: 20_000.0,
            })]);
        }
        // Loops cap at four instances; the rest of the slots are free for one-shots.
        assert_eq!(mixer.voices_of(Patch::ThrustLoop), 4);
        for _ in 0..MAX_VOICES {
            mixer.play(PlayRequest::new(Patch::Explosion));
        }
        let explosions = mixer.voices_of(Patch::Explosion);
        mixer.play(PlayRequest::new(Patch::UiMove));
        assert_eq!(mixer.voices_of(Patch::Explosion), explosions);
    }

    #[test]
    fn loops_fade_in_follow_and_fade_out_without_clicks() {
        let mut mixer = mixer_with(&[Patch::ThrustLoop]);
        let key = LoopKey::new(0, Patch::ThrustLoop);
        let request = |gain| {
            MixerCommand::Loop(LoopRequest {
                key,
                patch: Patch::ThrustLoop,
                gain,
                pan: 0.0,
                rate: 1.0,
                cutoff: 20_000.0,
            })
        };
        let mut output = Vec::new();
        for frame in 0..90 {
            // On, then steered (pitch and filter swept), then off.
            let steer = (frame as f32 / 30.0).min(1.0);
            mixer.apply(&[MixerCommand::Loop(LoopRequest {
                rate: 1.0 + 0.3 * steer,
                cutoff: 20_000.0 - 18_000.0 * steer,
                gain: if frame < 60 { 1.0 } else { 0.0 },
                ..match request(1.0) {
                    MixerCommand::Loop(loop_request) => loop_request,
                    _ => unreachable!(),
                }
            })]);
            output.extend(mixer.render_seconds(1.0 / 60.0));
        }
        output.extend(mixer.render_seconds(0.5));
        assert!(mixer.loop_gain(key).is_none(), "loop should have stopped");
        // Starts, steering and stops are ramped: the output is never more jagged than the
        // loop's own texture (largest second difference), so nothing clicks.
        let left: Vec<f32> = output.iter().step_by(2).copied().collect();
        assert!(left[0].abs() < 0.01);
        let jagged = |samples: &[f32]| {
            samples
                .windows(3)
                .map(|w| (w[2] - 2.0 * w[1] + w[0]).abs())
                .fold(0.0, f32::max)
        };
        let bank = SoundBank::render_only(3, &[Patch::ThrustLoop]);
        let raw = bank.get(Patch::ThrustLoop, 0).expect("rendered");
        let mut wrapped = raw.to_vec();
        wrapped.extend_from_slice(&raw[..2]);
        assert!(
            jagged(&left) <= jagged(&wrapped) * 1.05,
            "output {} vs loop {}",
            jagged(&left),
            jagged(&wrapped)
        );
    }

    #[test]
    fn loops_expire_without_refresh() {
        let mut mixer = mixer_with(&[Patch::GasHissLoop]);
        let key = LoopKey::new(9, Patch::GasHissLoop);
        mixer.apply(&[MixerCommand::Loop(LoopRequest {
            key,
            patch: Patch::GasHissLoop,
            gain: 1.0,
            pan: 0.0,
            rate: 1.0,
            cutoff: 20_000.0,
        })]);
        let _ = mixer.render_seconds(0.2);
        assert!(mixer.loop_gain(key).is_some());
        let _ = mixer.render_seconds(2.0);
        assert!(mixer.loop_gain(key).is_none());
    }

    #[test]
    fn explosions_duck_quieter_sounds() {
        let mut mixer = mixer_with(&[Patch::Explosion]);
        assert!(mixer.duck_depth() < 1e-6);
        mixer.play(PlayRequest::new(Patch::Explosion));
        let _ = mixer.render_seconds(0.1);
        assert!(mixer.duck_depth() > 0.3);
        let _ = mixer.render_seconds(3.0);
        assert!(mixer.duck_depth() < 0.05);
    }

    #[test]
    fn pan_places_sound_in_the_stereo_field() {
        let mut mixer = mixer_with(&[Patch::UiConfirm]);
        mixer.play(PlayRequest {
            pan: -1.0,
            ..PlayRequest::new(Patch::UiConfirm)
        });
        let out = mixer.render_seconds(0.1);
        let left: f32 = out.iter().step_by(2).map(|s| s * s).sum();
        let right: f32 = out.iter().skip(1).step_by(2).map(|s| s * s).sum();
        // The right channel only carries the (centred) reverb return.
        assert!(left > right * 4.0, "left {left} right {right}");
    }
}
