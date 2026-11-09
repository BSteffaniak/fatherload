//! Signal-processing building blocks for the procedural sound engine: a deterministic random
//! source, band-limited oscillators, noise colours, envelopes, a state-variable filter, FM,
//! waveshaping, a delay line and a small stereo reverb. Everything runs at [`SAMPLE_RATE`] and is
//! allocation-free per sample (delay lines allocate once when built).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::suboptimal_flops,
    reason = "DSP converts bounded sample positions and frequencies between integer and float \
              domains, and keeps textbook formulas readable instead of fusing them"
)]

use std::f32::consts::{PI, TAU};

/// Output sample rate of every rendered sound and of the mixer (Hz).
pub const SAMPLE_RATE: u32 = 44_100;
/// [`SAMPLE_RATE`] as a float.
pub const SR: f32 = 44_100.0;

/// Number of samples in `seconds` of audio.
#[must_use]
pub fn samples_for(seconds: f32) -> usize {
    (seconds.max(0.0) * SR).round() as usize
}

/// Converts decibels to a linear gain.
#[must_use]
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Converts a linear gain to decibels (floored at -120 dB).
#[must_use]
pub fn gain_to_db(gain: f32) -> f32 {
    20.0 * gain.max(1e-6).log10()
}

/// A small deterministic random source (`SplitMix64`): the same seed always renders the same
/// sound, so variations are reproducible.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub const fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1_u64 << 24) as f32
    }

    /// Uniform in `[-1, 1)`.
    pub fn bipolar(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    /// Uniform in `[low, high)`.
    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    /// `1 ± amount`, for multiplicative jitter.
    pub fn jitter(&mut self, amount: f32) -> f32 {
        1.0 + self.bipolar() * amount
    }

    /// Uniform index below `count` (0 when `count` is 0).
    pub const fn index(&mut self, count: usize) -> usize {
        if count == 0 {
            0
        } else {
            (self.next_u64() % count as u64) as usize
        }
    }
}

/// The polyBLEP residual that removes the aliasing of a unit step at phase `t` (0..1) for a
/// phase increment of `dt`.
#[must_use]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        0.0
    } else if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Oscillator waveforms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Wave {
    Sine,
    Saw,
    Square,
    Triangle,
}

/// A band-limited oscillator (polyBLEP saw and square, a leaky-integrated square for the
/// triangle).
#[derive(Clone, Debug)]
pub struct Osc {
    wave: Wave,
    phase: f32,
    /// Pulse width of the square (0..1).
    pub width: f32,
    triangle: f32,
}

impl Osc {
    #[must_use]
    pub const fn new(wave: Wave) -> Self {
        Self {
            wave,
            phase: 0.0,
            width: 0.5,
            triangle: -1.0,
        }
    }

    /// Starts at `phase` (0..1) instead of 0.
    #[must_use]
    pub fn with_phase(mut self, phase: f32) -> Self {
        self.phase = phase.rem_euclid(1.0);
        // Start the triangle's integrator on its waveform so it has no settling offset.
        self.triangle = if self.phase < 0.5 {
            -1.0 + 4.0 * self.phase
        } else {
            3.0 - 4.0 * self.phase
        };
        self
    }

    /// The next sample at `frequency` Hz, in `[-1, 1]` (the square's polyBLEP may overshoot a
    /// hair).
    pub fn next(&mut self, frequency: f32) -> f32 {
        let dt = (frequency / SR).clamp(0.0, 0.49);
        let t = self.phase;
        let out = match self.wave {
            Wave::Sine => (TAU * t).sin(),
            Wave::Saw => 2.0 * t - 1.0 - poly_blep(t, dt),
            Wave::Square => self.square(t, dt),
            Wave::Triangle => {
                // Always a symmetric square, or the integrator would drift.
                let square = naive_square(t, dt, 0.5);
                // Leaky integration of the band-limited square: ±1 over half a period adds 2.
                self.triangle = 4.0 * dt * square + (1.0 - 0.05 * dt) * self.triangle;
                self.triangle.clamp(-1.2, 1.2)
            }
        };
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        out
    }

    fn square(&self, t: f32, dt: f32) -> f32 {
        naive_square(t, dt, self.width)
    }
}

/// A polyBLEP pulse of `width` at phase `t`, with its DC offset (`2 * width - 1`) removed so
/// narrow pulses do not thump when enveloped.
fn naive_square(t: f32, dt: f32, width: f32) -> f32 {
    let width = width.clamp(0.02, 0.98);
    let naive = if t < width { 1.0 } else { -1.0 };
    let falling = (t + 1.0 - width).fract();
    naive + poly_blep(t, dt) - poly_blep(falling, dt) - (2.0 * width - 1.0)
}

/// Two-operator FM: a sine carrier phase-modulated by a sine modulator at `ratio` times the
/// carrier frequency.
#[derive(Clone, Debug, Default)]
pub struct Fm {
    carrier: f32,
    modulator: f32,
}

impl Fm {
    /// The next sample at carrier `frequency` with modulator `ratio` and modulation `index`
    /// (radians of peak phase deviation).
    pub fn next(&mut self, frequency: f32, ratio: f32, index: f32) -> f32 {
        let modulation = (TAU * self.modulator).sin() * index;
        let out = (TAU * self.carrier + modulation).sin();
        self.carrier = (self.carrier + frequency / SR).fract();
        self.modulator = (self.modulator + frequency * ratio / SR).fract();
        out
    }
}

/// Noise colours.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoiseColor {
    White,
    Pink,
    Brown,
}

/// A noise source of one colour, normalized to roughly unit peak.
#[derive(Clone, Debug)]
pub struct Noise {
    color: NoiseColor,
    rng: Rng,
    pink: [f32; 7],
    brown: f32,
}

impl Noise {
    #[must_use]
    pub const fn new(color: NoiseColor, seed: u64) -> Self {
        Self {
            color,
            rng: Rng::new(seed),
            pink: [0.0; 7],
            brown: 0.0,
        }
    }

    pub fn next(&mut self) -> f32 {
        let white = self.rng.bipolar();
        match self.color {
            NoiseColor::White => white,
            NoiseColor::Pink => {
                // Paul Kellet's refined pink filter.
                let b = &mut self.pink;
                b[0] = 0.998_86 * b[0] + white * 0.055_517_9;
                b[1] = 0.993_32 * b[1] + white * 0.075_075_9;
                b[2] = 0.969 * b[2] + white * 0.153_852;
                b[3] = 0.866_5 * b[3] + white * 0.310_485_6;
                b[4] = 0.55 * b[4] + white * 0.532_952_2;
                b[5] = -0.761_6 * b[5] - white * 0.016_898;
                let pink = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + white * 0.536_2;
                b[6] = white * 0.115_926;
                pink * 0.2
            }
            NoiseColor::Brown => {
                self.brown = (self.brown + white * 0.02) * 0.998;
                (self.brown * 3.5).clamp(-1.0, 1.0)
            }
        }
    }
}

/// An attack/decay/sustain/release envelope evaluated at a time, for a gate held `gate` seconds.
/// Attack is linear; decay and release are exponential and reach -60 dB at their times.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adsr {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Adsr {
    #[must_use]
    pub const fn new(attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
        Self {
            attack,
            decay,
            sustain,
            release,
        }
    }

    /// Level at `t` seconds for a gate released at `gate` seconds.
    #[must_use]
    pub fn level(&self, t: f32, gate: f32) -> f32 {
        if t < 0.0 {
            return 0.0;
        }
        let held = |t: f32| {
            if t < self.attack {
                t / self.attack.max(1e-6)
            } else {
                let since = t - self.attack;
                self.sustain + (1.0 - self.sustain) * decay(since, self.decay)
            }
        };
        if t < gate {
            held(t)
        } else {
            held(gate) * decay(t - gate, self.release)
        }
    }
}

/// Exponential decay that reaches -60 dB after `time` seconds.
#[must_use]
pub fn decay(t: f32, time: f32) -> f32 {
    if t <= 0.0 {
        1.0
    } else {
        (-6.907_755 * t / time.max(1e-6)).exp()
    }
}

/// A quick linear attack then exponential decay: the classic percussive envelope.
#[must_use]
pub fn perc(t: f32, attack: f32, time: f32) -> f32 {
    if t < 0.0 {
        0.0
    } else if t < attack {
        t / attack.max(1e-6)
    } else {
        decay(t - attack, time)
    }
}

/// An exponential glide from `start` to `end` with time constant `tau` (pitch envelopes).
#[must_use]
pub fn glide(t: f32, start: f32, end: f32, tau: f32) -> f32 {
    end + (start - end) * (-t.max(0.0) / tau.max(1e-6)).exp()
}

/// Piecewise-linear interpolation through `(time, value)` points (held past either end).
#[must_use]
pub fn segments(t: f32, points: &[(f32, f32)]) -> f32 {
    let Some(&(first_t, first_v)) = points.first() else {
        return 0.0;
    };
    if t <= first_t {
        return first_v;
    }
    for pair in points.windows(2) {
        let (t0, v0) = pair[0];
        let (t1, v1) = pair[1];
        if t <= t1 {
            let span = (t1 - t0).max(1e-6);
            return v0 + (v1 - v0) * (t - t0) / span;
        }
    }
    points.last().map_or(0.0, |&(_, v)| v)
}

/// Equal-power crossfade gains `(out, in)` at position `x` (0..1).
#[must_use]
pub fn equal_power(x: f32) -> (f32, f32) {
    let x = x.clamp(0.0, 1.0);
    ((x * PI * 0.5).cos(), (x * PI * 0.5).sin())
}

/// State-variable filter outputs.
#[allow(
    clippy::enum_variant_names,
    reason = "low/high/band-pass are the filter terms of art"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterMode {
    LowPass,
    HighPass,
    BandPass,
}

/// A topology-preserving state-variable filter (Simper/Cytomic): stable under fast cutoff
/// modulation, so envelopes can sweep it per sample.
#[derive(Clone, Debug)]
pub struct Svf {
    mode: FilterMode,
    ic1: f32,
    ic2: f32,
    cutoff: f32,
    q: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
}

impl Svf {
    #[must_use]
    pub fn new(mode: FilterMode, cutoff: f32, q: f32) -> Self {
        let mut filter = Self {
            mode,
            ic1: 0.0,
            ic2: 0.0,
            cutoff: -1.0,
            q: -1.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            k: 0.0,
        };
        filter.set(cutoff, q);
        filter
    }

    /// Retunes the filter (only recomputes coefficients when they change).
    pub fn set(&mut self, cutoff: f32, q: f32) {
        let cutoff = cutoff.clamp(10.0, SR * 0.45);
        let q = q.max(0.1);
        if (cutoff - self.cutoff).abs() < 1e-3 && (q - self.q).abs() < 1e-4 {
            return;
        }
        self.cutoff = cutoff;
        self.q = q;
        let g = (PI * cutoff / SR).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// Filters one sample at the current tuning.
    pub fn process(&mut self, input: f32) -> f32 {
        let v3 = input - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        match self.mode {
            FilterMode::LowPass => v2,
            FilterMode::BandPass => v1,
            FilterMode::HighPass => input - self.k * v1 - v2,
        }
    }

    /// Retunes then filters one sample (for per-sample envelope modulation).
    pub fn run(&mut self, input: f32, cutoff: f32, q: f32) -> f32 {
        self.set(cutoff, q);
        self.process(input)
    }
}

/// Removes DC offset (a 20 Hz one-pole high-pass).
#[derive(Clone, Debug, Default)]
pub struct DcBlocker {
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    pub fn process(&mut self, input: f32) -> f32 {
        let y = input - self.x1 + 0.997_15 * self.y1;
        self.x1 = input;
        self.y1 = y;
        y
    }
}

/// Smooth saturation normalized so `drive` changes the colour, not the level of a full-scale
/// input.
#[must_use]
pub fn saturate(x: f32, drive: f32) -> f32 {
    let drive = drive.max(0.01);
    (x * drive).tanh() / drive.tanh()
}

/// Wavefolding: reflects the signal back inside `[-1, 1]` (metallic, buzzy harmonics).
#[must_use]
pub fn fold(x: f32) -> f32 {
    let mut y = x;
    for _ in 0..4 {
        if y > 1.0 {
            y = 2.0 - y;
        } else if y < -1.0 {
            y = -2.0 - y;
        } else {
            break;
        }
    }
    y.clamp(-1.0, 1.0)
}

/// Sample-and-hold decimation (crunchy, lo-fi grit): holds every `factor`-th sample.
#[derive(Clone, Debug, Default)]
pub struct Decimator {
    held: f32,
    counter: f32,
}

impl Decimator {
    pub fn process(&mut self, input: f32, factor: f32) -> f32 {
        self.counter += 1.0;
        if self.counter >= factor.max(1.0) {
            self.counter -= factor.max(1.0);
            self.held = input;
        }
        self.held
    }
}

/// A feedback delay line with a damped (low-passed) feedback path.
#[derive(Clone, Debug)]
pub struct Delay {
    buffer: Vec<f32>,
    write: usize,
    damping: f32,
    damp_state: f32,
}

impl Delay {
    /// A delay that can hold up to `max_seconds`.
    #[must_use]
    pub fn new(max_seconds: f32) -> Self {
        Self {
            buffer: vec![0.0; samples_for(max_seconds).max(2)],
            write: 0,
            damping: 0.3,
            damp_state: 0.0,
        }
    }

    /// Sets how much the feedback path darkens per repeat (0 = bright, 1 = very dark).
    #[must_use]
    pub const fn with_damping(mut self, damping: f32) -> Self {
        self.damping = damping;
        self
    }

    /// Writes `input` (plus `feedback` of the delayed signal) and returns the signal delayed by
    /// `seconds`.
    pub fn process(&mut self, input: f32, seconds: f32, feedback: f32) -> f32 {
        let len = self.buffer.len();
        let delay = samples_for(seconds).clamp(1, len - 1);
        let read = (self.write + len - delay) % len;
        let delayed = self.buffer[read];
        self.damp_state += (delayed - self.damp_state) * (1.0 - self.damping.clamp(0.0, 0.99));
        self.buffer[self.write] = input + self.damp_state * feedback;
        self.write = (self.write + 1) % len;
        delayed
    }
}

/// A damped comb filter (one Freeverb voice).
#[derive(Clone, Debug)]
struct Comb {
    buffer: Vec<f32>,
    index: usize,
    store: f32,
}

impl Comb {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
            store: 0.0,
        }
    }

    fn process(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let output = self.buffer[self.index];
        self.store = output * (1.0 - damping) + self.store * damping;
        self.buffer[self.index] = input + self.store * feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

/// A Schroeder all-pass diffuser.
#[derive(Clone, Debug)]
struct AllPass {
    buffer: Vec<f32>,
    index: usize,
}

impl AllPass {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = buffered - input;
        self.buffer[self.index] = input + buffered * 0.5;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

/// A small stereo Freeverb-style reverb: eight damped combs and four all-passes per side. The
/// mixer runs one as its shared reverb bus.
#[derive(Clone, Debug)]
pub struct Reverb {
    left_combs: Vec<Comb>,
    right_combs: Vec<Comb>,
    left_passes: Vec<AllPass>,
    right_passes: Vec<AllPass>,
    /// Comb feedback (room size), 0.7..0.98.
    pub room: f32,
    /// High-frequency damping, 0..1.
    pub damping: f32,
}

impl Reverb {
    /// A reverb whose delay lengths are scaled by `size` (1.0 = the classic Freeverb room).
    #[must_use]
    pub fn new(size: f32) -> Self {
        let scale = |length: usize| ((length as f32) * size).round() as usize;
        Self {
            left_combs: COMB_TUNING.iter().map(|&l| Comb::new(scale(l))).collect(),
            right_combs: COMB_TUNING
                .iter()
                .map(|&l| Comb::new(scale(l + STEREO_SPREAD)))
                .collect(),
            left_passes: ALLPASS_TUNING
                .iter()
                .map(|&l| AllPass::new(scale(l)))
                .collect(),
            right_passes: ALLPASS_TUNING
                .iter()
                .map(|&l| AllPass::new(scale(l + STEREO_SPREAD)))
                .collect(),
            room: 0.82,
            damping: 0.45,
        }
    }

    /// Processes one mono input sample into a wet stereo pair.
    pub fn process(&mut self, input: f32) -> (f32, f32) {
        let input = input * 0.015;
        let mut left = 0.0;
        let mut right = 0.0;
        for comb in &mut self.left_combs {
            left += comb.process(input, self.room, self.damping);
        }
        for comb in &mut self.right_combs {
            right += comb.process(input, self.room, self.damping);
        }
        for pass in &mut self.left_passes {
            left = pass.process(left);
        }
        for pass in &mut self.right_passes {
            right = pass.process(right);
        }
        (left, right)
    }
}

/// Renders `seconds` of audio by calling `voice(t)` once per sample with the time in seconds.
pub fn render(seconds: f32, mut voice: impl FnMut(f32) -> f32) -> Vec<f32> {
    let count = samples_for(seconds);
    (0..count).map(|index| voice(index as f32 / SR)).collect()
}

/// Adds `source` (scaled by `gain`) into `target` starting `offset` seconds in, growing `target`
/// as needed.
pub fn mix_into(target: &mut Vec<f32>, source: &[f32], offset: f32, gain: f32) {
    let start = samples_for(offset);
    if target.len() < start + source.len() {
        target.resize(start + source.len(), 0.0);
    }
    for (slot, sample) in target[start..].iter_mut().zip(source) {
        *slot += sample * gain;
    }
}

/// Equal-power pan gains `(left, right)` for `pan` in `[-1, 1]`.
#[must_use]
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * PI * 0.25;
    (angle.cos(), angle.sin())
}

/// Cubic Hermite interpolation of `buffer` at fractional `position`; `wrap` reads past the ends
/// cyclically (loops), otherwise as silence.
#[must_use]
pub fn hermite(buffer: &[f32], position: f64, wrap: bool) -> f32 {
    let len = buffer.len();
    if len == 0 {
        return 0.0;
    }
    let base = position.floor();
    let frac = (position - base) as f32;
    let base = base as i64;
    let at = |offset: i64| -> f32 {
        let index = base + offset;
        if wrap {
            buffer[index.rem_euclid(len as i64) as usize]
        } else if index < 0 || index >= len as i64 {
            0.0
        } else {
            buffer[index as usize]
        }
    };
    let (xm1, x0, x1, x2) = (at(-1), at(0), at(1), at(2));
    let c1 = 0.5 * (x1 - xm1);
    let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
    let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
    ((c3 * frac + c2) * frac + c1) * frac + x0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count()
    }

    #[test]
    fn oscillators_run_at_their_frequency() {
        for wave in [Wave::Sine, Wave::Saw, Wave::Square, Wave::Triangle] {
            let mut osc = Osc::new(wave).with_phase(0.25);
            // Settle the triangle's integrator, then count rising zero crossings over a second.
            for _ in 0..4_410 {
                osc.next(440.0);
            }
            let second = render(1.0, |_| osc.next(440.0));
            let crossings = zero_crossings(&second);
            assert!(
                (439..=441).contains(&crossings),
                "{wave:?} produced {crossings} cycles for 440 Hz"
            );
        }
    }

    #[test]
    fn band_limited_saw_has_less_aliasing_than_a_naive_one() {
        // A high saw: the polyBLEP version's jump at the wrap is spread over two samples.
        let mut osc = Osc::new(Wave::Saw);
        let blep = render(0.05, |_| osc.next(5_000.0));
        let max_step = blep
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0, f32::max);
        assert!(max_step < 1.9, "polyBLEP saw still jumps {max_step}");
    }

    #[test]
    fn oscillators_and_noise_stay_bounded_and_finite() {
        for color in [NoiseColor::White, NoiseColor::Pink, NoiseColor::Brown] {
            let mut noise = Noise::new(color, 7);
            let samples = render(2.0, |_| noise.next());
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.5));
            let mean = samples.iter().sum::<f32>() / samples.len() as f32;
            assert!(mean.abs() < 0.1, "{color:?} noise has DC {mean}");
        }
        for wave in [Wave::Sine, Wave::Saw, Wave::Square, Wave::Triangle] {
            let mut osc = Osc::new(wave);
            assert!(
                render(0.5, |t| osc.next(50.0 + t * 8_000.0))
                    .iter()
                    .all(|s| s.is_finite() && s.abs() <= 1.3)
            );
        }
    }

    #[test]
    fn adsr_has_the_expected_shape() {
        let env = Adsr::new(0.01, 0.1, 0.5, 0.2);
        assert!(env.level(0.0, 1.0).abs() < 1e-6);
        assert!((env.level(0.005, 1.0) - 0.5).abs() < 1e-3);
        assert!((env.level(0.01, 1.0) - 1.0).abs() < 1e-3);
        // Decays toward sustain, reaching it (within -60 dB of the gap) after the decay time.
        assert!((env.level(0.11, 1.0) - 0.5).abs() < 1e-3);
        assert!((env.level(0.9, 1.0) - 0.5).abs() < 1e-3);
        // Releases from the gate level to silence over the release time.
        assert!(env.level(1.1, 1.0) < 0.5 && env.level(1.1, 1.0) > 0.0);
        assert!(env.level(1.2, 1.0) < 0.5e-3);
        // Monotonic attack and release.
        let attack: Vec<f32> = (0..10).map(|i| env.level(i as f32 * 0.001, 1.0)).collect();
        assert!(attack.windows(2).all(|pair| pair[1] >= pair[0]));
        let release: Vec<f32> = (0..20)
            .map(|i| env.level(1.0 + i as f32 * 0.01, 1.0))
            .collect();
        assert!(release.windows(2).all(|pair| pair[1] <= pair[0]));
    }

    #[test]
    fn glide_and_segments_interpolate() {
        assert!((glide(0.0, 200.0, 50.0, 0.1) - 200.0).abs() < 1e-3);
        assert!((glide(10.0, 200.0, 50.0, 0.1) - 50.0).abs() < 1e-3);
        let points = [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)];
        assert!((segments(0.5, &points) - 0.5).abs() < 1e-6);
        assert!((segments(1.5, &points) - 0.5).abs() < 1e-6);
        assert!(segments(3.0, &points).abs() < 1e-6);
    }

    fn tone_gain(mode: FilterMode, cutoff: f32, frequency: f32) -> f32 {
        let mut filter = Svf::new(mode, cutoff, 0.707);
        let mut osc = Osc::new(Wave::Sine);
        let out = render(0.5, |_| filter.process(osc.next(frequency)));
        // Peak over the settled second half.
        out[out.len() / 2..]
            .iter()
            .fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn state_variable_filter_responds_sensibly() {
        // Low-pass: passes the low tone, attenuates two octaves above by ~24 dB.
        assert!(tone_gain(FilterMode::LowPass, 1_000.0, 100.0) > 0.95);
        assert!(tone_gain(FilterMode::LowPass, 1_000.0, 4_000.0) < 0.08);
        // -3 dB at the cutoff with Q 0.707.
        let at_cutoff = tone_gain(FilterMode::LowPass, 1_000.0, 1_000.0);
        assert!((at_cutoff - 0.707).abs() < 0.05, "{at_cutoff}");
        // High-pass mirrors it.
        assert!(tone_gain(FilterMode::HighPass, 1_000.0, 100.0) < 0.05);
        assert!(tone_gain(FilterMode::HighPass, 1_000.0, 8_000.0) > 0.95);
        // Band-pass peaks at the centre.
        let centre = tone_gain(FilterMode::BandPass, 1_000.0, 1_000.0);
        assert!(centre > tone_gain(FilterMode::BandPass, 1_000.0, 200.0) * 3.0);
        assert!(centre > tone_gain(FilterMode::BandPass, 1_000.0, 5_000.0) * 3.0);
    }

    #[test]
    fn filter_survives_extreme_modulation() {
        let mut filter = Svf::new(FilterMode::LowPass, 100.0, 12.0);
        let mut noise = Noise::new(NoiseColor::White, 3);
        let out = render(1.0, |t| {
            let cutoff = if (t * 50.0) as i32 % 2 == 0 {
                20.0
            } else {
                19_000.0
            };
            filter.run(noise.next(), cutoff, 12.0)
        });
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn reverb_and_delay_decay_to_silence() {
        let mut reverb = Reverb::new(1.0);
        let mut delay = Delay::new(0.5);
        let mut tail = 0.0_f32;
        for index in 0..(SAMPLE_RATE * 6) as usize {
            let input = if index == 0 { 1.0 } else { 0.0 };
            let (left, right) = reverb.process(input);
            let echo = delay.process(input, 0.2, 0.6);
            assert!(left.is_finite() && right.is_finite() && echo.is_finite());
            if index > (SAMPLE_RATE * 5) as usize {
                tail = tail.max(left.abs()).max(right.abs()).max(echo.abs());
            }
        }
        assert!(tail < 1e-3, "tail after five seconds: {tail}");
    }

    #[test]
    fn same_seed_same_noise() {
        let mut a = Noise::new(NoiseColor::Pink, 42);
        let mut b = Noise::new(NoiseColor::Pink, 42);
        let mut c = Noise::new(NoiseColor::Pink, 43);
        let left = render(0.1, |_| a.next());
        let right = render(0.1, |_| b.next());
        let other = render(0.1, |_| c.next());
        assert_eq!(left, right);
        assert_ne!(left, other);
    }

    #[test]
    fn hermite_reproduces_samples_and_wraps() {
        let buffer = [0.0, 1.0, 0.0, -1.0];
        assert!((hermite(&buffer, 1.0, false) - 1.0).abs() < 1e-6);
        assert!((hermite(&buffer, 3.0, true) + 1.0).abs() < 1e-6);
        assert!(hermite(&buffer, 10.0, false).abs() < 1e-6);
        assert!((hermite(&buffer, 5.0, true) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pan_is_equal_power() {
        for pan in [-1.0, -0.3, 0.0, 0.5, 1.0] {
            let (l, r) = pan_gains(pan);
            assert!((l * l + r * r - 1.0).abs() < 1e-5);
        }
        assert!(pan_gains(-1.0).1.abs() < 1e-6);
    }
}
