//! The designed sound patches: every sound the game makes, synthesized from [`super::dsp`]
//! building blocks. Each patch renders deterministically from a seed (so each of its variations
//! is reproducible), then [`render_patch`] finishes it: DC removal, click-free edges, a seamless
//! seam for loops and loudness normalization to its category's target.
//!
//! The design of each patch is summarized in `packages/game/src/audio/README.md`.

#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    reason = "patch recipes are long lists of layered formulas kept close to how they sound"
)]

use std::f32::consts::TAU;

use super::{
    analysis,
    dsp::{
        Adsr, Decimator, Delay, FilterMode, Fm, Noise, NoiseColor, Osc, Rng, Svf, Wave, db_to_gain,
        decay, equal_power, fold, glide, mix_into, perc, render, samples_for, saturate, segments,
    },
};

/// Loudness class of a patch: every patch in a class is normalized to the same level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    /// Menu ticks and blips.
    Interface,
    /// Rewards and confirmations (coins, pickups, crates).
    Feedback,
    /// Physical hits (landings, bumps, damage, drill transients).
    Impact,
    /// Warning beeps.
    Alarm,
    /// Continuous sounds the director drives (thrust, drilling, hazards).
    Loop,
    /// Set-pieces (explosions, fanfares, stings).
    Big,
}

impl Category {
    /// Target loudness: the loudest 100 ms window's RMS (after a 100 Hz high-pass that stands in
    /// for perceptual weighting), in dBFS.
    #[must_use]
    pub const fn target_db(self) -> f32 {
        match self {
            Self::Interface => -24.0,
            Self::Feedback => -20.0,
            Self::Alarm => -21.0,
            Self::Impact => -17.0,
            Self::Loop => -22.0,
            Self::Big => -15.0,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Interface => "interface",
            Self::Feedback => "feedback",
            Self::Alarm => "alarm",
            Self::Impact => "impact",
            Self::Loop => "loop",
            Self::Big => "big",
        }
    }
}

/// Highest sample peak a finished patch may reach (-1 dBFS).
pub const PEAK_CEILING: f32 = 0.891;
/// Length of every loop patch (seconds). Long enough that repeating noise textures do not
/// become recognisable during minutes of drilling or flying (a 2 s frozen-noise loop does).
pub const LOOP_SECONDS: f32 = 4.0;
/// Crossfade folded over a loop's seam (seconds).
const LOOP_CROSSFADE_SECONDS: f32 = 0.3;

macro_rules! patches {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// Every designed sound.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum Patch {
            $($variant),+
        }

        impl Patch {
            /// Every patch, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// File-friendly name (`--render-audio` writes `<name>_<variation>.wav`).
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+
                }
            }
        }
    };
}

patches! {
    UiMove => "ui_move",
    UiConfirm => "ui_confirm",
    UiBack => "ui_back",
    UiError => "ui_error",
    Toast => "toast",
    Pickup => "pickup",
    Coins => "coins",
    Purchase => "purchase",
    CrateOpen => "crate_open",
    SalvageOpen => "salvage_open",
    BombArm => "bomb_arm",
    FuseTick => "fuse_tick",
    Deploy => "deploy",
    ScannerPulse => "scanner_pulse",
    DrillBite => "drill_bite",
    DrillGlint => "drill_glint",
    ThrustIgnite => "thrust_ignite",
    LandLight => "land_light",
    LandMedium => "land_medium",
    LandHeavy => "land_heavy",
    WallBump => "wall_bump",
    HullHit => "hull_hit",
    DrillCrunch => "drill_crunch",
    BreakSoft => "break_soft",
    BreakHard => "break_hard",
    VaultReset => "vault_reset",
    FuelLow => "alarm_fuel_low",
    FuelCritical => "alarm_fuel_critical",
    HullLow => "alarm_hull_low",
    HullCritical => "alarm_hull_critical",
    Explosion => "explosion",
    PoiDiscovered => "poi_discovered",
    VaultFanfare => "vault_fanfare",
    Milestone => "milestone",
    Rescue => "rescue",
    ThrustLoop => "loop_thrust",
    DrillSoftLoop => "loop_drill_soft",
    DrillHardLoop => "loop_drill_hard",
    GasHissLoop => "loop_gas_hiss",
    LavaSizzleLoop => "loop_lava_sizzle",
    BoulderRumbleLoop => "loop_boulder_rumble",
}

/// Playback metadata of a patch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatchInfo {
    pub category: Category,
    /// Pre-rendered variations (each from its own seed).
    pub variations: u8,
    /// Voice-stealing priority (higher survives).
    pub priority: u8,
    /// Most simultaneous voices of this patch; the oldest is stolen past it.
    pub max_instances: u8,
    /// Send level to the shared reverb bus.
    pub reverb_send: f32,
    /// Random playback-rate jitter per play (fraction, e.g. 0.04 = ±4%).
    pub pitch_jitter: f32,
    /// Random gain jitter per play (dB, ±).
    pub gain_jitter_db: f32,
    /// How deeply this patch ducks lower-priority voices while it plays (0 = not at all).
    pub ducks: f32,
}

impl PatchInfo {
    const fn new(category: Category) -> Self {
        let (priority, reverb_send) = match category {
            Category::Interface => (3, 0.05),
            Category::Feedback => (4, 0.15),
            Category::Impact => (5, 0.2),
            Category::Alarm => (7, 0.05),
            Category::Loop => (2, 0.12),
            Category::Big => (8, 0.3),
        };
        Self {
            category,
            variations: if matches!(category, Category::Loop) {
                1
            } else {
                4
            },
            priority,
            max_instances: 3,
            reverb_send,
            pitch_jitter: 0.03,
            gain_jitter_db: 1.0,
            ducks: 0.0,
        }
    }

    const fn instances(mut self, max: u8) -> Self {
        self.max_instances = max;
        self
    }

    const fn send(mut self, reverb_send: f32) -> Self {
        self.reverb_send = reverb_send;
        self
    }

    const fn jitter(mut self, pitch: f32, gain_db: f32) -> Self {
        self.pitch_jitter = pitch;
        self.gain_jitter_db = gain_db;
        self
    }

    const fn ducking(mut self, depth: f32) -> Self {
        self.ducks = depth;
        self
    }

    const fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

impl Patch {
    /// Stable index (declaration order) for table lookups.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Whether the patch is a seamless loop rather than a one-shot.
    #[must_use]
    pub const fn looped(self) -> bool {
        matches!(self.info().category, Category::Loop)
    }

    /// Playback metadata.
    #[must_use]
    pub const fn info(self) -> PatchInfo {
        use Category::{Alarm, Big, Feedback, Impact, Interface, Loop};
        match self {
            Self::UiMove => PatchInfo::new(Interface).instances(2).jitter(0.02, 0.5),
            Self::UiConfirm | Self::UiBack | Self::Toast => {
                PatchInfo::new(Interface).instances(2).jitter(0.01, 0.5)
            }
            Self::UiError => PatchInfo::new(Interface).instances(1).jitter(0.01, 0.5),
            Self::Pickup | Self::DrillGlint => PatchInfo::new(Feedback).jitter(0.05, 1.5),
            Self::Coins | Self::Purchase | Self::CrateOpen | Self::SalvageOpen => {
                PatchInfo::new(Feedback).instances(2).jitter(0.02, 0.5)
            }
            Self::BombArm | Self::Deploy | Self::ThrustIgnite => {
                PatchInfo::new(Feedback).instances(2)
            }
            Self::FuseTick => PatchInfo::new(Feedback).instances(4).jitter(0.01, 0.5),
            Self::ScannerPulse => PatchInfo::new(Feedback)
                .instances(1)
                .send(0.35)
                .jitter(0.01, 0.5),
            Self::DrillBite | Self::DrillCrunch => {
                PatchInfo::new(Impact).instances(3).jitter(0.08, 2.0)
            }
            Self::LandLight | Self::LandMedium | Self::WallBump | Self::BreakSoft => {
                PatchInfo::new(Impact).instances(2).jitter(0.05, 1.0)
            }
            Self::LandHeavy | Self::HullHit | Self::BreakHard => PatchInfo::new(Impact)
                .instances(2)
                .jitter(0.04, 1.0)
                .ducking(0.25),
            Self::VaultReset => PatchInfo::new(Impact).instances(1).jitter(0.01, 0.5),
            Self::FuelLow | Self::FuelCritical | Self::HullLow | Self::HullCritical => {
                PatchInfo::new(Alarm).instances(1).jitter(0.0, 0.0)
            }
            Self::Explosion => PatchInfo::new(Big)
                .instances(3)
                .send(0.35)
                .jitter(0.06, 1.0)
                .ducking(0.55)
                .priority(9),
            Self::PoiDiscovered | Self::VaultFanfare | Self::Milestone | Self::Rescue => {
                PatchInfo::new(Big)
                    .instances(1)
                    .jitter(0.0, 0.0)
                    .ducking(0.35)
            }
            Self::ThrustLoop
            | Self::DrillSoftLoop
            | Self::DrillHardLoop
            | Self::GasHissLoop
            | Self::LavaSizzleLoop
            | Self::BoulderRumbleLoop => PatchInfo::new(Loop).instances(4).jitter(0.0, 0.0),
        }
    }

    /// The raw design (before normalization and edge treatment) for variation `seed`.
    fn design(self, seed: u64) -> Vec<f32> {
        let mut rng = Rng::new(seed);
        let rng = &mut rng;
        match self {
            Self::UiMove => ui_move(rng),
            Self::UiConfirm => ui_confirm(rng),
            Self::UiBack => ui_back(rng),
            Self::UiError => ui_error(rng),
            Self::Toast => toast(rng),
            Self::Pickup => pickup(rng),
            Self::Coins => coins(rng),
            Self::Purchase => purchase(rng),
            Self::CrateOpen => crate_open(rng),
            Self::SalvageOpen => salvage_open(rng),
            Self::BombArm => bomb_arm(rng),
            Self::FuseTick => fuse_tick(rng),
            Self::Deploy => deploy(rng),
            Self::ScannerPulse => scanner_pulse(rng),
            Self::DrillBite => drill_bite(rng),
            Self::DrillGlint => drill_glint(rng),
            Self::ThrustIgnite => thrust_ignite(rng),
            Self::LandLight => landing(rng, 0),
            Self::LandMedium => landing(rng, 1),
            Self::LandHeavy => landing(rng, 2),
            Self::WallBump => wall_bump(rng),
            Self::HullHit => hull_hit(rng),
            Self::DrillCrunch => drill_crunch(rng),
            Self::BreakSoft => break_through(rng, false),
            Self::BreakHard => break_through(rng, true),
            Self::VaultReset => vault_reset(rng),
            Self::FuelLow => fuel_alarm(rng, false),
            Self::FuelCritical => fuel_alarm(rng, true),
            Self::HullLow => hull_alarm(rng, false),
            Self::HullCritical => hull_alarm(rng, true),
            Self::Explosion => explosion(rng),
            Self::PoiDiscovered => poi_discovered(rng),
            Self::VaultFanfare => vault_fanfare(rng),
            Self::Milestone => milestone(rng),
            Self::Rescue => rescue(rng),
            Self::ThrustLoop => thrust_loop(rng),
            Self::DrillSoftLoop => drill_soft_loop(rng),
            Self::DrillHardLoop => drill_hard_loop(rng),
            Self::GasHissLoop => gas_hiss_loop(rng),
            Self::LavaSizzleLoop => lava_sizzle_loop(rng),
            Self::BoulderRumbleLoop => boulder_rumble_loop(rng),
        }
    }
}

/// The seed of variation `variation` of `patch` under bank seed `seed`.
#[must_use]
pub const fn variation_seed(seed: u64, patch: Patch, variation: u8) -> u64 {
    seed.wrapping_mul(0x0100_0000_01B3)
        ^ ((patch.index() as u64) << 16)
        ^ (variation as u64).wrapping_mul(0x9E37_79B9)
}

/// Renders a finished patch: designed, DC-free, click-free at its edges (or seamless at its loop
/// point) and normalized to its category's loudness, peaking at most [`PEAK_CEILING`].
#[must_use]
pub fn render_patch(patch: Patch, seed: u64) -> Vec<f32> {
    let mut samples = patch.design(seed);
    for sample in &mut samples {
        if !sample.is_finite() {
            *sample = 0.0;
        }
    }
    if patch.looped() {
        // Before the fold, so the seam crossfades filtered audio with filtered audio.
        sub_cut(&mut samples, 2);
        samples = fold_loop(&samples);
        let mean = samples.iter().sum::<f32>() / samples.len().max(1) as f32;
        for sample in &mut samples {
            *sample -= mean;
        }
    } else {
        sub_cut(&mut samples, 1);
        trim_tail(&mut samples);
        fade_edges(&mut samples);
    }
    normalize(&mut samples, patch.info().category.target_db());
    samples
}

/// Below this (Hz) a patch's content is removed: brown-noise rumble and sub glides otherwise
/// pile up infrasonic energy nobody hears but every speaker and the limiter have to carry.
const SUB_CUT_HZ: f32 = 30.0;

/// High-passes `samples` at [`SUB_CUT_HZ`] with `stages` cascaded 12 dB/octave Butterworth
/// sections (this also removes any DC offset).
fn sub_cut(samples: &mut [f32], stages: usize) {
    for _ in 0..stages {
        let mut filter = Svf::new(
            FilterMode::HighPass,
            SUB_CUT_HZ,
            std::f32::consts::FRAC_1_SQRT_2,
        );
        for sample in samples.iter_mut() {
            *sample = filter.process(*sample);
        }
    }
}

/// Folds the last [`LOOP_CROSSFADE_SECONDS`] over the start so the buffer loops seamlessly:
/// the sample after the last one is (nearly) the one that originally followed it.
fn fold_loop(samples: &[f32]) -> Vec<f32> {
    let length = samples_for(LOOP_SECONDS).min(samples.len());
    let fade = samples_for(LOOP_CROSSFADE_SECONDS).min(samples.len() - length);
    let mut out = samples[..length].to_vec();
    for (index, slot) in out.iter_mut().enumerate().take(fade) {
        let (fade_out, fade_in) = equal_power(index as f32 / fade as f32);
        *slot = *slot * fade_in + samples[length + index] * fade_out;
    }
    out
}

/// Drops the inaudible tail (below -70 dBFS relative to the peak).
fn trim_tail(samples: &mut Vec<f32>) {
    let peak = analysis::peak(samples);
    let floor = peak * db_to_gain(-70.0);
    let end = samples
        .iter()
        .rposition(|s| s.abs() > floor)
        .map_or(0, |index| index + 1);
    samples.truncate((end + samples_for(0.01)).min(samples.len()));
}

/// A 1 ms fade-in and a 12 ms fade-out so a one-shot starts and ends at exactly zero.
fn fade_edges(samples: &mut [f32]) {
    let len = samples.len();
    let fade_in = samples_for(0.001).min(len);
    for (index, sample) in samples.iter_mut().enumerate().take(fade_in) {
        *sample *= index as f32 / fade_in as f32;
    }
    let fade_out = samples_for(0.012).min(len);
    for index in 0..fade_out {
        let position = len - 1 - index;
        samples[position] *= index as f32 / fade_out as f32;
    }
}

/// Scales `samples` to `target_db` loudness, then down further if that would exceed the peak
/// ceiling.
fn normalize(samples: &mut [f32], target_db: f32) {
    let loudness = analysis::loudness(samples);
    if loudness <= 1e-6 {
        return;
    }
    let mut gain = db_to_gain(target_db) / loudness;
    let peak = analysis::peak(samples);
    if peak * gain > PEAK_CEILING {
        gain = PEAK_CEILING / peak;
    }
    for sample in samples {
        *sample *= gain;
    }
}

// ---------------------------------------------------------------------------------------------
// Shared layers
// ---------------------------------------------------------------------------------------------

/// A sine at a fixed frequency (stateless).
fn sine(t: f32, frequency: f32) -> f32 {
    (TAU * frequency * t).sin()
}

/// A bank of decaying sine partials `(frequency, amplitude, decay seconds)`: bells, metal.
fn partials(t: f32, bank: &[(f32, f32, f32)]) -> f32 {
    bank.iter()
        .map(|&(frequency, amplitude, time)| sine(t, frequency) * amplitude * decay(t, time))
        .sum()
}

/// A short metallic clank: inharmonic partials around `base` Hz.
fn clank(rng: &mut Rng, base: f32, time: f32) -> Vec<f32> {
    let ratios = [1.0, 2.76, 5.40, 8.93];
    let bank: Vec<(f32, f32, f32)> = ratios
        .iter()
        .enumerate()
        .map(|(index, ratio)| {
            (
                base * ratio * rng.jitter(0.03),
                1.0 / (1.0 + index as f32),
                time * rng.jitter(0.2) / (1.0 + index as f32 * 0.5),
            )
        })
        .collect();
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hp = Svf::new(FilterMode::HighPass, 3_000.0, 0.7);
    render(time * 1.2, |t| {
        partials(t, &bank) + hp.process(noise.next()) * perc(t, 0.000_2, 0.01) * 0.6
    })
}

/// A low thump: a sine glide with a percussive envelope.
fn thump(start: f32, end: f32, time: f32, drive: f32) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine);
    render(time * 1.1, |t| {
        let frequency = glide(t, start, end, time * 0.15);
        saturate(osc.next(frequency) * perc(t, 0.001, time), drive)
    })
}

/// A filtered noise burst.
fn burst(
    seed: u64,
    color: NoiseColor,
    mode: FilterMode,
    cutoff: f32,
    q: f32,
    time: f32,
) -> Vec<f32> {
    let mut noise = Noise::new(color, seed);
    let mut filter = Svf::new(mode, cutoff, q);
    render(time * 1.1, |t| {
        filter.process(noise.next()) * perc(t, 0.000_5, time)
    })
}

/// `count` event times in `[start, end)`, clustered toward `start` when `skew` > 1.
fn scatter(rng: &mut Rng, count: usize, start: f32, end: f32, skew: f32) -> Vec<f32> {
    let mut times: Vec<f32> = (0..count)
        .map(|_| start + (end - start) * rng.unit().powf(skew))
        .collect();
    times.sort_by(f32::total_cmp);
    times
}

/// Grit: short band-passed noise grains (debris, crumbling dirt, rock chips), bunched early in
/// `span` and thinning out like settling debris.
fn grains(
    rng: &mut Rng,
    out: &mut Vec<f32>,
    count: usize,
    span: (f32, f32),
    band: (f32, f32),
    level: f32,
) {
    scatter_grains(rng, out, count, span, band, level, true);
}

/// Grains over `span`: settling as in [`grains`], or (for loops, where anything bunched or
/// fading pulses at the loop rate) spread evenly at a steady level.
fn scatter_grains(
    rng: &mut Rng,
    out: &mut Vec<f32>,
    count: usize,
    span: (f32, f32),
    band: (f32, f32),
    level: f32,
    settling: bool,
) {
    let skew = if settling { 1.8 } else { 1.0 };
    for time in scatter(rng, count, span.0, span.1, skew) {
        let cutoff = rng.range(band.0, band.1);
        let length = rng.range(0.008, 0.035);
        let grain = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            cutoff,
            1.4,
            length,
        );
        let fade = if settling {
            (1.0 - (time - span.0) / (span.1 - span.0).max(1e-3)).max(0.15)
        } else {
            1.0
        };
        mix_into(out, &grain, time, level * rng.range(0.4, 1.0) * fade);
    }
}

/// A note of a brassy synth voice: detuned saws through an enveloped low-pass.
fn brass(rng: &mut Rng, frequency: f32, gate: f32, brightness: f32) -> Vec<f32> {
    let detune = [0.996, 1.0, 1.004];
    let mut oscs: Vec<Osc> = detune
        .iter()
        .map(|_| Osc::new(Wave::Saw).with_phase(rng.unit()))
        .collect();
    let mut filter = Svf::new(FilterMode::LowPass, 500.0, 1.1);
    let amp = Adsr::new(0.02, 0.25, 0.75, 0.35);
    let cut = Adsr::new(0.04, 0.3, 0.45, 0.3);
    let vibrato_rate = rng.range(4.8, 5.6);
    render(gate + 0.5, |t| {
        let vibrato = 1.0 + 0.004 * sine(t, vibrato_rate) * (t / 0.4).min(1.0);
        let raw: f32 = oscs
            .iter_mut()
            .zip(detune)
            .map(|(osc, d)| osc.next(frequency * d * vibrato))
            .sum::<f32>()
            / 3.0;
        let cutoff = frequency * (1.5 + brightness * 6.0 * cut.level(t, gate));
        filter.run(raw, cutoff, 1.1) * amp.level(t, gate)
    })
}

/// An FM bell strike.
fn bell(frequency: f32, ratio: f32, index: f32, time: f32) -> Vec<f32> {
    let mut fm = Fm::default();
    render(time * 1.1, |t| {
        fm.next(frequency, ratio, index * decay(t, time * 0.4)) * perc(t, 0.001, time)
    })
}

// ---------------------------------------------------------------------------------------------
// Interface
// ---------------------------------------------------------------------------------------------

fn ui_move(rng: &mut Rng) -> Vec<f32> {
    let start = 2_300.0 * rng.jitter(0.03);
    let mut osc = Osc::new(Wave::Sine);
    let mut click = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hp = Svf::new(FilterMode::HighPass, 4_000.0, 0.7);
    render(0.07, |t| {
        let tone = osc.next(glide(t, start, start * 0.75, 0.012)) * perc(t, 0.001, 0.045);
        tone + hp.process(click.next()) * perc(t, 0.000_3, 0.006) * 0.12
    })
}

fn two_notes(rng: &mut Rng, notes: [f32; 2], gap: f32, wave: Wave, length: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, note) in notes.into_iter().enumerate() {
        let note = note * rng.jitter(0.006);
        let mut osc = Osc::new(wave);
        osc.width = 0.3;
        let mut sub = Osc::new(Wave::Sine);
        let mut filter = Svf::new(FilterMode::LowPass, 4_000.0, 0.8);
        let tone = render(length + 0.05, |t| {
            let cutoff = note * (1.5 + 5.0 * decay(t, 0.08));
            let raw = osc.next(note) * 0.6 + sub.next(note) * 0.5;
            filter.run(raw, cutoff, 0.8) * perc(t, 0.002, length)
        });
        mix_into(&mut out, &tone, gap * index as f32, 1.0);
    }
    out
}

fn ui_confirm(rng: &mut Rng) -> Vec<f32> {
    two_notes(rng, [784.0, 1_175.0], 0.06, Wave::Square, 0.12)
}

fn ui_back(rng: &mut Rng) -> Vec<f32> {
    two_notes(rng, [880.0, 587.0], 0.055, Wave::Triangle, 0.1)
}

fn ui_error(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for pulse in 0..2 {
        let mut a = Osc::new(Wave::Square);
        let mut b = Osc::new(Wave::Square);
        let mut filter = Svf::new(FilterMode::LowPass, 1_200.0, 1.2);
        let base = 150.0 * rng.jitter(0.01);
        let gate = 0.085;
        let env = Adsr::new(0.003, 0.05, 0.7, 0.03);
        let tone = render(gate + 0.06, |t| {
            let raw = (a.next(base) + b.next(base * 1.06)) * 0.5;
            fold(saturate(filter.process(raw), 2.0) * 1.4) * env.level(t, gate)
        });
        mix_into(&mut out, &tone, pulse as f32 * 0.13, 1.0);
    }
    out
}

fn toast(rng: &mut Rng) -> Vec<f32> {
    let root = 1_318.5 * rng.jitter(0.005);
    let mut out = bell(root, 1.0, 1.4, 0.5);
    mix_into(&mut out, &bell(root * 1.5, 2.0, 0.8, 0.35), 0.0, 0.35);
    out
}

// ---------------------------------------------------------------------------------------------
// Feedback
// ---------------------------------------------------------------------------------------------

/// Pentatonic choices for sparkly pickups.
const SPARKLE_NOTES: [f32; 6] = [1_568.0, 1_760.0, 2_093.0, 2_349.0, 2_637.0, 3_136.0];

fn pickup(rng: &mut Rng) -> Vec<f32> {
    let first = rng.index(SPARKLE_NOTES.len() - 2);
    let mut out = bell(SPARKLE_NOTES[first], 2.0, 1.6, 0.22);
    mix_into(
        &mut out,
        &bell(SPARKLE_NOTES[first + 2], 2.0, 1.2, 0.28),
        0.045,
        0.8,
    );
    out
}

fn coins(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(150.0, 110.0, 0.08, 1.0);
    for sample in &mut out {
        *sample *= 0.4;
    }
    let mut time = 0.02;
    let hits = 5 + rng.index(3);
    for index in 0..hits {
        let base = rng.range(2_700.0, 3_600.0) * (1.0 + index as f32 * 0.04);
        let ring_time = rng.range(0.12, 0.25);
        let coin = clank(rng, base, ring_time);
        mix_into(&mut out, &coin, time, rng.range(0.35, 0.6));
        time += rng.range(0.035, 0.09);
    }
    // The register's "ching".
    let ching = bell(2_637.0 * rng.jitter(0.01), 1.0, 0.6, 0.55);
    mix_into(&mut out, &ching, time + 0.03, 0.55);
    out
}

fn purchase(rng: &mut Rng) -> Vec<f32> {
    let notes = [523.25, 659.25, 783.99, 1_046.5];
    let mut out = Vec::new();
    for (index, note) in notes.into_iter().enumerate() {
        let gate = if index == 3 { 0.25 } else { 0.1 };
        let voice = brass(rng, note, gate, 0.9);
        mix_into(&mut out, &voice, index as f32 * 0.07, 0.5);
    }
    for time in scatter(rng, 6, 0.22, 0.6, 1.0) {
        let note = SPARKLE_NOTES[rng.index(SPARKLE_NOTES.len())] * 2.0;
        mix_into(&mut out, &bell(note, 2.0, 0.8, 0.12), time, 0.12);
    }
    let mut echo = Delay::new(0.3).with_damping(0.5);
    out.resize(out.len() + samples_for(0.4), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.11, 0.3) * 0.3;
    }
    out
}

fn crate_open(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        1_800.0,
        2.0,
        0.03,
    );
    let lid = thump(125.0 * rng.jitter(0.05), 80.0, 0.12, 1.5);
    mix_into(&mut out, &lid, 0.04, 0.9);
    let body = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        600.0,
        0.7,
        0.1,
    );
    mix_into(&mut out, &body, 0.04, 0.6);
    // A creaking hinge.
    let mut saw = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::BandPass, 900.0, 6.0);
    let pitch = 210.0 * rng.jitter(0.08);
    let creak = render(0.3, |t| {
        let frequency = pitch * (1.0 + 0.08 * sine(t, 23.0));
        filter.process(saw.next(frequency)) * segments(t, &[(0.0, 0.0), (0.05, 0.6), (0.3, 0.0)])
    });
    mix_into(&mut out, &creak, 0.08, 0.35);
    mix_into(&mut out, &bell(2_093.0, 2.0, 1.0, 0.18), 0.2, 0.2);
    mix_into(&mut out, &bell(2_637.0, 2.0, 1.0, 0.22), 0.26, 0.2);
    out
}

fn salvage_open(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 410.0, 0.4);
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::HighPass, 2_000.0, 0.8);
    let hiss = render(0.5, |t| {
        let cutoff = 2_000.0 + 4_000.0 * (t / 0.5);
        filter.run(noise.next(), cutoff, 0.8)
            * segments(t, &[(0.0, 0.0), (0.03, 0.8), (0.2, 0.5), (0.5, 0.0)])
    });
    mix_into(&mut out, &hiss, 0.06, 0.45);
    mix_into(&mut out, &thump(95.0, 60.0, 0.18, 1.5), 0.0, 0.8);
    mix_into(&mut out, &bell(1_760.0, 2.0, 1.0, 0.3), 0.3, 0.2);
    out
}

fn bomb_arm(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 300.0, 0.12);
    let mut osc = Osc::new(Wave::Square);
    osc.width = 0.25;
    let mut filter = Svf::new(FilterMode::LowPass, 3_500.0, 0.8);
    let env = Adsr::new(0.002, 0.02, 0.8, 0.02);
    let beep = render(0.1, |t| {
        filter.process(osc.next(1_760.0)) * env.level(t, 0.06)
    });
    mix_into(&mut out, &beep, 0.12, 0.35);
    out
}

fn fuse_tick(rng: &mut Rng) -> Vec<f32> {
    let tone = 3_200.0 * rng.jitter(0.02);
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hp = Svf::new(FilterMode::HighPass, 3_000.0, 0.7);
    render(0.05, |t| {
        hp.process(noise.next()) * perc(t, 0.000_3, 0.012)
            + sine(t, tone) * perc(t, 0.0005, 0.02) * 0.3
    })
}

fn deploy(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for click in 0..6 {
        let grain = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            2_500.0 * rng.jitter(0.1),
            2.5,
            0.012,
        );
        mix_into(&mut out, &grain, click as f32 * 0.04, 0.7);
    }
    mix_into(&mut out, &thump(90.0, 55.0, 0.2, 2.0), 0.28, 1.0);
    let body = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        400.0,
        0.7,
        0.15,
    );
    mix_into(&mut out, &body, 0.28, 0.6);
    mix_into(&mut out, &clank(rng, 620.0, 0.25), 0.3, 0.3);
    out
}

fn scanner_pulse(rng: &mut Rng) -> Vec<f32> {
    let start = 1_400.0 * rng.jitter(0.01);
    let mut osc = Osc::new(Wave::Sine);
    let mut shimmer = Fm::default();
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut sweep = Svf::new(FilterMode::BandPass, 300.0, 1.5);
    let mut echo = Delay::new(0.5).with_damping(0.4);
    render(1.6, |t| {
        let frequency = glide(t, start, start * 0.82, 0.25);
        let ping = (osc.next(frequency) + shimmer.next(frequency * 2.0, 1.5, 0.3) * 0.15)
            * perc(t, 0.002, 0.55);
        let whoosh = sweep.run(noise.next(), 300.0 + 2_700.0 * (t / 0.4).min(1.0), 1.5)
            * segments(t, &[(0.0, 0.0), (0.05, 0.5), (0.4, 0.0)]);
        let dry = ping + whoosh * 0.6;
        dry + echo.process(dry, 0.18, 0.45) * 0.5
    })
}

fn drill_bite(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        1_500.0 * rng.jitter(0.1),
        1.5,
        0.06,
    );
    for sample in &mut out {
        *sample = saturate(*sample * 2.0, 2.5);
    }
    mix_into(&mut out, &thump(180.0, 120.0, 0.05, 1.5), 0.0, 0.7);
    out
}

fn drill_glint(rng: &mut Rng) -> Vec<f32> {
    let notes = [2_637.0, 3_136.0, 3_520.0, 4_186.0];
    let note = notes[rng.index(notes.len())];
    let mut out = bell(note, 3.01, 0.8, 0.25);
    mix_into(&mut out, &bell(note * 1.5, 2.0, 0.5, 0.15), 0.03, 0.3);
    out
}

fn thrust_ignite(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 200.0, 0.9);
    let mut out = render(0.35, |t| {
        let cutoff = segments(t, &[(0.0, 200.0), (0.06, 2_500.0), (0.35, 400.0)]);
        filter.run(noise.next(), cutoff, 0.9) * perc(t, 0.01, 0.3)
    });
    mix_into(&mut out, &thump(75.0, 45.0, 0.2, 1.5), 0.0, 0.4);
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 600.0, 1.0);
    let fwoomp = render(0.3, |t| {
        band.run(air.next(), 500.0 + 2_000.0 * (t / 0.3), 1.0) * perc(t, 0.02, 0.25)
    });
    mix_into(&mut out, &fwoomp, 0.0, 2.5);
    out
}

// ---------------------------------------------------------------------------------------------
// Impacts
// ---------------------------------------------------------------------------------------------

fn landing(rng: &mut Rng, tier: u8) -> Vec<f32> {
    let weight = f32::from(tier);
    let time = 0.15 + weight * 0.22;
    let mut out = thump(
        (95.0 - weight * 12.0) * rng.jitter(0.05),
        55.0 - weight * 9.0,
        time,
        1.0 + weight,
    );
    let body = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        500.0 + weight * 350.0,
        0.7,
        0.06 + weight * 0.1,
    );
    mix_into(&mut out, &body, 0.0, 0.7);
    mix_into(
        &mut out,
        &clank(rng, 700.0 * (1.0 + 0.1 * weight), 0.06 + weight * 0.08),
        0.0,
        0.3 + weight * 0.1,
    );
    let knock = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        900.0 + weight * 300.0,
        1.2,
        0.03 + weight * 0.02,
    );
    mix_into(&mut out, &knock, 0.0, 0.5);
    if tier == 2 {
        let mut crunch = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            2_000.0,
            1.0,
            0.12,
        );
        let mut crush = Decimator::default();
        for sample in &mut crunch {
            *sample = crush.process(saturate(*sample * 3.0, 3.0), 3.0);
        }
        mix_into(&mut out, &crunch, 0.0, 0.35);
        grains(rng, &mut out, 18, (0.05, 0.7), (900.0, 3_500.0), 0.25);
    }
    out
}

fn wall_bump(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(140.0 * rng.jitter(0.05), 90.0, 0.09, 1.5);
    let body = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        700.0,
        0.9,
        0.04,
    );
    mix_into(&mut out, &body, 0.0, 0.6);
    mix_into(&mut out, &clank(rng, 1_300.0, 0.06), 0.0, 0.2);
    out
}

fn hull_hit(rng: &mut Rng) -> Vec<f32> {
    let mut crunch = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        1_100.0 * rng.jitter(0.1),
        0.8,
        0.12,
    );
    let mut crush = Decimator::default();
    for sample in &mut crunch {
        *sample = crush.process(saturate(*sample * 4.0, 4.0), 2.0) * 0.6;
    }
    let mut out = crunch;
    let resonances: [(f32, f32, f32); 3] = [
        (523.0 * rng.jitter(0.05), 0.5, 0.35),
        (1_287.0 * rng.jitter(0.05), 0.35, 0.25),
        (2_411.0 * rng.jitter(0.05), 0.25, 0.18),
    ];
    let resonance = render(0.4, |t| partials(t, &resonances));
    mix_into(&mut out, &resonance, 0.0, 0.7);
    mix_into(&mut out, &thump(110.0, 60.0, 0.15, 2.0), 0.0, 0.9);
    out
}

fn drill_crunch(rng: &mut Rng) -> Vec<f32> {
    let mut grit = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        rng.range(1_500.0, 3_000.0),
        1.2,
        0.05,
    );
    let mut crush = Decimator::default();
    for sample in &mut grit {
        *sample = crush.process(saturate(*sample * 3.0, 4.0), 3.0);
    }
    mix_into(&mut grit, &thump(220.0, 160.0, 0.03, 1.0), 0.0, 0.5);
    grit
}

fn break_through(rng: &mut Rng, hard: bool) -> Vec<f32> {
    let mut out = if hard {
        burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            2_500.0,
            0.7,
            0.015,
        )
    } else {
        Vec::new()
    };
    let (start, end, time) = if hard {
        (90.0, 50.0, 0.2)
    } else {
        (120.0, 70.0, 0.15)
    };
    mix_into(
        &mut out,
        &thump(start * rng.jitter(0.05), end, time, 2.0),
        0.0,
        0.9,
    );
    let rubble = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        if hard { 900.0 } else { 700.0 },
        0.7,
        0.3,
    );
    mix_into(&mut out, &rubble, 0.0, 0.7);
    if hard {
        grains(rng, &mut out, 22, (0.01, 0.45), (1_800.0, 3_500.0), 0.45);
        for sample in &mut out {
            *sample = saturate(*sample, 1.5);
        }
    } else {
        grains(rng, &mut out, 22, (0.01, 0.35), (500.0, 1_800.0), 0.9);
    }
    out
}

fn vault_reset(rng: &mut Rng) -> Vec<f32> {
    let mut square = Osc::new(Wave::Square);
    let mut saw = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, 1_500.0, 1.5);
    let start = 330.0 * rng.jitter(0.01);
    let mut out = render(0.6, |t| {
        let frequency = glide(t, start, start / 3.0, 0.2);
        let raw = square.next(frequency) * 0.5 + saw.next(frequency * 1.01) * 0.5;
        saturate(filter.process(raw), 1.5) * perc(t, 0.005, 0.55)
    });
    mix_into(&mut out, &clank(rng, 260.0, 0.3), 0.45, 0.6);
    mix_into(&mut out, &thump(80.0, 50.0, 0.2, 1.5), 0.45, 0.7);
    out
}

// ---------------------------------------------------------------------------------------------
// Alarms
// ---------------------------------------------------------------------------------------------

fn beep(frequency: f32, gate: f32, width: f32, drive: f32) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Square);
    osc.width = width;
    let mut filter = Svf::new(FilterMode::LowPass, frequency * 3.5, 0.8);
    let env = Adsr::new(0.003, 0.05, 0.8, 0.02);
    render(gate + 0.05, |t| {
        saturate(filter.process(osc.next(frequency)), drive) * env.level(t, gate)
    })
}

fn fuel_alarm(_rng: &mut Rng, critical: bool) -> Vec<f32> {
    let mut out = Vec::new();
    if critical {
        for index in 0..3 {
            mix_into(
                &mut out,
                &beep(1_046.5, 0.08, 0.35, 1.8),
                index as f32 * 0.13,
                1.0,
            );
        }
    } else {
        mix_into(&mut out, &beep(880.0, 0.12, 0.25, 1.0), 0.0, 1.0);
        mix_into(&mut out, &beep(660.0, 0.12, 0.25, 1.0), 0.15, 1.0);
    }
    out
}

fn hull_alarm(_rng: &mut Rng, critical: bool) -> Vec<f32> {
    let gate = 0.35;
    let env = Adsr::new(0.01, 0.1, 0.8, 0.08);
    if critical {
        let mut a = Osc::new(Wave::Saw);
        let mut b = Osc::new(Wave::Saw);
        let mut filter = Svf::new(FilterMode::LowPass, 2_500.0, 2.0);
        render(gate + 0.1, |t| {
            let raw = (a.next(620.0) + b.next(655.0)) * 0.5;
            let tremolo = 0.65 + 0.35 * sine(t, 18.0);
            saturate(filter.process(raw), 3.0) * tremolo * env.level(t, gate)
        })
    } else {
        let mut tri = Osc::new(Wave::Triangle);
        let mut sub = Osc::new(Wave::Sine);
        render(gate + 0.1, |t| {
            let frequency = 520.0 + 40.0 * sine(t, 14.0);
            (tri.next(frequency) * 0.7 + sub.next(frequency * 0.5) * 0.4) * env.level(t, gate)
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Set-pieces
// ---------------------------------------------------------------------------------------------

fn explosion(rng: &mut Rng) -> Vec<f32> {
    let mut crack_noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut crack_filter = Svf::new(FilterMode::HighPass, 1_500.0, 0.7);
    let mut boom = Osc::new(Wave::Sine);
    let mut body_noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut body_filter = Svf::new(FilterMode::LowPass, 2_500.0, 0.8);
    let mut mid_noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut mid_filter = Svf::new(FilterMode::BandPass, 600.0, 0.7);
    let low = 60.0 * rng.jitter(0.08);
    let body_time = 1.8 * rng.jitter(0.1);
    let mut out = render(2.2, |t| {
        let crack = crack_filter.process(crack_noise.next()) * perc(t, 0.000_2, 0.035);
        let sub = saturate(
            boom.next(glide(t, low, low * 0.47, 0.15)) * perc(t, 0.002, 1.4),
            2.5,
        );
        let body = body_filter.run(body_noise.next(), glide(t, 2_500.0, 180.0, 0.25), 0.8)
            * perc(t, 0.003, body_time);
        let mid = mid_filter.process(mid_noise.next()) * perc(t, 0.002, 0.9);
        crack * 0.9 + sub * 0.7 + body * 1.0 + mid * 2.2
    });
    grains(rng, &mut out, 60, (0.1, 2.6), (800.0, 4_500.0), 0.45);
    for time in scatter(rng, 8, 0.2, 1.6, 1.5) {
        let chunk = thump(rng.range(150.0, 250.0), 110.0, 0.05, 1.5);
        mix_into(&mut out, &chunk, time, rng.range(0.1, 0.25));
    }
    for sample in &mut out {
        *sample = saturate(*sample, 1.5);
    }
    out
}

fn poi_discovered(rng: &mut Rng) -> Vec<f32> {
    let gate = 1.0;
    let env = Adsr::new(0.25, 0.5, 0.7, 0.8);
    let notes = [220.0, 329.63, 493.88];
    let mut voices: Vec<(Osc, Osc)> = notes
        .iter()
        .map(|_| {
            (
                Osc::new(Wave::Saw).with_phase(rng.unit()),
                Osc::new(Wave::Saw).with_phase(rng.unit()),
            )
        })
        .collect();
    let mut filter = Svf::new(FilterMode::LowPass, 1_200.0, 0.9);
    let mut out = render(gate + 0.9, |t| {
        let raw: f32 = voices
            .iter_mut()
            .zip(notes)
            .enumerate()
            .map(|(index, ((a, b), note))| {
                let enter = index as f32 * 0.12;
                let level = env.level(t - enter, gate);
                (a.next(note * 0.997) + b.next(note * 1.003)) * level
            })
            .sum::<f32>()
            / 4.0;
        filter.run(raw, 700.0 + 600.0 * sine(t, 0.7).abs(), 0.9)
    });
    mix_into(&mut out, &bell(880.0, 3.5, 2.0, 1.4), 0.0, 0.35);
    mix_into(&mut out, &bell(1_318.5, 3.5, 1.5, 1.2), 0.18, 0.28);
    let mut echo = Delay::new(0.5).with_damping(0.45);
    out.resize(out.len() + samples_for(0.6), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.3, 0.35) * 0.35;
    }
    out
}

fn vault_fanfare(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(52.0, 35.0, 0.8, 2.0);
    let rumble = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        300.0,
        0.7,
        0.5,
    );
    mix_into(&mut out, &rumble, 0.0, 0.8);
    let arpeggio = [261.63, 329.63, 392.0];
    for (index, note) in arpeggio.into_iter().enumerate() {
        mix_into(
            &mut out,
            &brass(rng, note, 0.12, 0.8),
            0.15 + index as f32 * 0.15,
            0.35,
        );
    }
    for note in [261.63, 523.25, 659.25, 783.99] {
        mix_into(&mut out, &brass(rng, note, 1.5, 1.0), 0.6, 0.25);
    }
    for (index, note) in [2_093.0, 2_637.0, 3_136.0, 4_186.0].into_iter().enumerate() {
        mix_into(
            &mut out,
            &bell(note, 2.0, 1.0, 0.6),
            0.6 + index as f32 * 0.06,
            0.08,
        );
    }
    for sample in &mut out {
        *sample = saturate(*sample, 1.3);
    }
    out
}

fn milestone(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for note in [587.33, 739.99, 880.0] {
        mix_into(&mut out, &brass(rng, note, 0.6, 0.7), 0.0, 0.3);
    }
    for (index, note) in [1_174.7, 1_480.0, 1_760.0, 2_349.3].into_iter().enumerate() {
        mix_into(
            &mut out,
            &bell(note, 2.0, 1.2, 0.5),
            index as f32 * 0.07,
            0.18,
        );
    }
    out
}

fn rescue(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hp = Svf::new(FilterMode::HighPass, 4_000.0, 0.7);
    let mut out = render(1.4, |t| {
        let frequency = 200.0 * 8.0_f32.powf((t / 1.2).min(1.0));
        let tremolo = 0.6 + 0.4 * sine(t, 12.0);
        let env = segments(t, &[(0.0, 0.0), (0.1, 0.8), (1.1, 1.0), (1.4, 0.0)]);
        let beam = fm.next(frequency, 1.5, 3.0 * (1.0 - t / 1.4)) * tremolo * env;
        let sparkle = hp.process(noise.next()) * tremolo * env * 0.25;
        beam * 0.6 + sparkle
    });
    for note in [659.25, 987.77] {
        mix_into(&mut out, &brass(rng, note, 0.3, 0.6), 1.15, 0.25);
    }
    mix_into(&mut out, &bell(1_975.5, 2.0, 1.0, 0.4), 1.15, 0.2);
    out
}

// ---------------------------------------------------------------------------------------------
// Loops (rendered LOOP_SECONDS + crossfade, then folded seamless by `fold_loop`). Periodic parts
// use frequencies that complete whole cycles in LOOP_SECONDS (multiples of 0.5 Hz). Scattered
// events (pops, knocks, grains) are spread evenly over the whole render at a steady density, so
// nothing marks where the loop restarts.
// ---------------------------------------------------------------------------------------------

fn loop_length() -> f32 {
    LOOP_SECONDS + LOOP_CROSSFADE_SECONDS
}

/// How many scattered events a loop render holds at `per_second` events a second.
fn loop_events(per_second: f32) -> usize {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive event count"
    )]
    let count = (per_second * loop_length()).round() as usize;
    count
}

/// Slow random flutter: smoothed noise around 1.0 with `depth`.
fn flutter(rng: &mut Rng, speed: f32, depth: f32) -> impl FnMut() -> f32 + use<> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut smooth = Svf::new(FilterMode::LowPass, speed, 0.7);
    move || 1.0 + (smooth.process(noise.next()) * 25.0 * depth).clamp(-depth, depth)
}

fn thrust_loop(rng: &mut Rng) -> Vec<f32> {
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_filter = Svf::new(FilterMode::LowPass, 300.0, 0.8);
    let mut hiss = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut hiss_filter = Svf::new(FilterMode::BandPass, 1_200.0, 0.6);
    let mut hum_a = Osc::new(Wave::Saw);
    let mut hum_b = Osc::new(Wave::Saw);
    let mut hum_filter = Svf::new(FilterMode::LowPass, 250.0, 0.9);
    let mut roar = Noise::new(NoiseColor::White, rng.next_u64());
    let mut roar_filter = Svf::new(FilterMode::BandPass, 2_800.0, 0.8);
    let mut wobble = flutter(rng, 10.0, 0.15);
    let mut sputter = flutter(rng, 35.0, 0.35);
    render(loop_length(), |_| {
        let body = rumble_filter.process(rumble.next());
        let air = hiss_filter.process(hiss.next()) * wobble();
        let jet = roar_filter.process(roar.next()) * sputter();
        let hum = hum_filter.process(hum_a.next(55.0) + hum_b.next(82.5) * 0.6);
        saturate(body * 0.5 + air * 2.2 + jet * 0.35 + hum * 0.2, 1.5)
    })
}

fn drill_soft_loop(rng: &mut Rng) -> Vec<f32> {
    let mut motor_a = Osc::new(Wave::Saw);
    let mut motor_b = Osc::new(Wave::Square);
    let mut motor_filter = Svf::new(FilterMode::LowPass, 600.0, 0.9);
    let mut scrape = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut scrape_filter = Svf::new(FilterMode::BandPass, 900.0, 0.9);
    let mut clods = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut clod_filter = Svf::new(FilterMode::LowPass, 500.0, 0.7);
    let mut grit = flutter(rng, 30.0, 0.5);
    render(loop_length(), |t| {
        let rotation = sine(t, 16.0).abs();
        let motor = motor_filter.process(motor_a.next(95.0) + motor_b.next(190.0) * 0.4);
        let scraping = scrape_filter.process(scrape.next()) * (0.4 + 0.6 * rotation) * grit();
        let clod = clod_filter.process(clods.next()) * (0.5 + 0.5 * sine(t, 4.0));
        motor * 0.25 * (0.85 + 0.15 * rotation) + scraping * 0.9 + clod * 0.5
    })
}

fn drill_hard_loop(rng: &mut Rng) -> Vec<f32> {
    let mut motor_a = Osc::new(Wave::Saw);
    let mut motor_b = Osc::new(Wave::Saw);
    let mut motor_filter = Svf::new(FilterMode::LowPass, 900.0, 1.0);
    let mut grind = Noise::new(NoiseColor::White, rng.next_u64());
    let mut grind_filter = Svf::new(FilterMode::BandPass, 2_200.0, 1.5);
    let mut grind_post = Svf::new(FilterMode::LowPass, 4_500.0, 0.7);
    let mut rough = flutter(rng, 40.0, 0.6);
    let mut squeal = flutter(rng, 3.0, 0.02);
    // Phase-accumulating: `sine(t, f(t))` would turn the wobble into a sweep growing with `t`.
    let mut squeal_a = Osc::new(Wave::Sine);
    let mut squeal_b = Osc::new(Wave::Sine);
    render(loop_length(), |t| {
        let chatter = 0.55 + 0.45 * sine(t, 24.0).abs();
        let motor = motor_filter.process(motor_a.next(120.0) + motor_b.next(240.0) * 0.5);
        let grinding = grind_post.process(saturate(
            grind_filter.process(grind.next()) * 4.0 * chatter * rough(),
            5.0,
        ));
        let pitch = squeal();
        let metal = (squeal_a.next(3_100.0 * pitch) + squeal_b.next(4_700.0 * pitch) * 0.6) * 0.04;
        motor * 0.3 + grinding * 0.5 + metal
    })
}

fn gas_hiss_loop(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut high = Svf::new(FilterMode::HighPass, 2_500.0, 0.7);
    let mut band = Svf::new(FilterMode::BandPass, 6_000.0, 0.8);
    let mut swell = flutter(rng, 1.5, 0.3);
    render(loop_length(), |_| {
        let white = noise.next();
        (high.process(white) * 0.5 + band.process(white) * 0.6) * swell()
    })
}

fn lava_sizzle_loop(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut sizzle = Svf::new(FilterMode::BandPass, 5_000.0, 0.5);
    let mut crackle = flutter(rng, 60.0, 0.9);
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_filter = Svf::new(FilterMode::LowPass, 120.0, 0.8);
    let mut out = render(loop_length(), |_| {
        let crackling = sizzle.process(noise.next()) * crackle().powi(3) * 0.35;
        crackling + rumble_filter.process(rumble.next()) * 0.8
    });
    // Pops and bubbles.
    for time in scatter(rng, loop_events(11.5), 0.0, loop_length() - 0.1, 1.0) {
        let pop = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(3_000.0, 7_000.0),
            2.0,
            0.006,
        );
        mix_into(&mut out, &pop, time, rng.range(0.3, 0.8));
    }
    for time in scatter(rng, loop_events(4.5), 0.0, loop_length() - 0.1, 1.0) {
        let mut osc = Osc::new(Wave::Sine);
        let base = rng.range(180.0, 260.0);
        let bubble = render(0.05, |t| {
            osc.next(base * (1.0 + t * 30.0)) * perc(t, 0.003, 0.04)
        });
        mix_into(&mut out, &bubble, time, 0.25);
    }
    out
}

fn boulder_rumble_loop(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 150.0, 1.0);
    let mut grind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut grind_filter = Svf::new(FilterMode::BandPass, 550.0, 1.0);
    let mut grind_level = flutter(rng, 6.0, 0.7);
    let mut out = render(loop_length(), |t| {
        let roll = 0.7 + 0.3 * sine(t, 4.0);
        low.process(noise.next()) * roll * 0.8
            + grind_filter.process(grind.next()) * grind_level() * 2.0
    });
    for time in scatter(rng, loop_events(3.0), 0.0, loop_length() - 0.15, 1.0) {
        let knock = thump(rng.range(70.0, 100.0), 55.0, 0.1, 1.5);
        mix_into(&mut out, &knock, time, rng.range(0.2, 0.4));
        let crack = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(700.0, 1_400.0),
            1.5,
            0.04,
        );
        mix_into(&mut out, &crack, time, rng.range(0.3, 0.6));
    }
    scatter_grains(
        rng,
        &mut out,
        loop_events(13.0),
        (0.0, loop_length() - 0.1),
        (600.0, 2_000.0),
        0.2,
        false,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_names_are_unique() {
        let mut names: Vec<&str> = Patch::ALL.iter().map(|patch| patch.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Patch::ALL.len());
        assert!(
            Patch::ALL
                .iter()
                .enumerate()
                .all(|(index, patch)| patch.index() == index)
        );
    }

    #[test]
    fn renders_are_deterministic_per_seed() {
        for patch in [Patch::Coins, Patch::Explosion, Patch::DrillHardLoop] {
            let first = render_patch(patch, variation_seed(7, patch, 0));
            let again = render_patch(patch, variation_seed(7, patch, 0));
            let other = render_patch(patch, variation_seed(7, patch, 1));
            assert_eq!(first, again, "{} is not deterministic", patch.name());
            assert_ne!(first, other, "{} variations are identical", patch.name());
        }
    }
}
