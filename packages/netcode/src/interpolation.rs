//! Remote-player interpolation, the interpolation timeline and the
//! presentation-only correction offset (plan §5, §8).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

use crate::ids::{PlayerId, Tick};
use crate::params::{NetTuning, SessionParams};
use crate::simulation::Pose;

/// Per-remote-player pose history for interpolation.
#[derive(Clone, Debug)]
pub struct SnapshotBuffer {
    players: BTreeMap<PlayerId, VecDeque<(Tick, Pose)>>,
    capacity: usize,
}

impl Default for SnapshotBuffer {
    fn default() -> Self {
        Self::new(128)
    }
}

impl SnapshotBuffer {
    /// Creates a buffer keeping at most `capacity` samples per player.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            players: BTreeMap::new(),
            capacity: capacity.max(2),
        }
    }

    /// Records a pose; out-of-order or duplicate ticks are ignored.
    pub fn push(&mut self, player: PlayerId, tick: Tick, pose: Pose) {
        let samples = self.players.entry(player).or_default();
        if samples.back().is_some_and(|(last, _)| *last >= tick) {
            return;
        }
        samples.push_back((tick, pose));
        while samples.len() > self.capacity {
            samples.pop_front();
        }
    }

    /// Drops samples no longer needed to sample at `time` or later (keeps one
    /// sample at or before `time` and at least two in total).
    #[allow(clippy::cast_precision_loss)]
    pub fn prune_before(&mut self, time: f64) {
        for samples in self.players.values_mut() {
            while samples.len() > 2
                && samples
                    .get(1)
                    .is_some_and(|(tick, _)| tick.0 as f64 <= time)
            {
                samples.pop_front();
            }
        }
    }

    /// Keeps only the listed players.
    pub fn retain_players(&mut self, players: &BTreeSet<PlayerId>) {
        self.players.retain(|player, _| players.contains(player));
    }

    /// Removes everything.
    pub fn clear(&mut self) {
        self.players.clear();
    }

    /// Players with history.
    pub fn players(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.players.keys().copied()
    }

    /// Number of samples held for `player`.
    #[must_use]
    pub fn depth(&self, player: PlayerId) -> usize {
        self.players.get(&player).map_or(0, VecDeque::len)
    }

    /// Largest per-player sample count.
    #[must_use]
    pub fn max_depth(&self) -> usize {
        self.players.values().map(VecDeque::len).max().unwrap_or(0)
    }

    /// Samples `player` at fractional tick `time`, extrapolating at most
    /// `max_extrapolation` ticks past the newest sample, then freezing.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn sample(&self, player: PlayerId, time: f64, max_extrapolation: f64) -> Option<Pose> {
        let samples = self.players.get(&player)?;
        let (first_tick, first) = *samples.front()?;
        if time <= first_tick.0 as f64 {
            return Some(first);
        }
        let (last_tick, last) = *samples.back()?;
        if time >= last_tick.0 as f64 {
            if samples.len() < 2 {
                return Some(last);
            }
            let (prev_tick, prev) = samples[samples.len() - 2];
            let span = (last_tick.0 - prev_tick.0) as f64;
            let over = (time - last_tick.0 as f64).min(max_extrapolation.max(0.0));
            let t = (over / span) as f32;
            let mut pose = last.translated((last.x - prev.x) * t, (last.y - prev.y) * t);
            pose.mode = last.mode;
            return Some(pose);
        }
        let index = samples.partition_point(|(tick, _)| (tick.0 as f64) <= time);
        let (a_tick, a) = samples[index - 1];
        let (b_tick, b) = samples[index];
        let t = ((time - a_tick.0 as f64) / (b_tick.0 - a_tick.0) as f64) as f32;
        Some(a.lerp(&b, t))
    }
}

/// The remote interpolation timeline: newest server tick received plus elapsed
/// time minus the interpolation delay, adjusted smoothly.
#[derive(Clone, Debug, Default)]
pub struct InterpTimeline {
    time: Option<f64>,
    newest: Option<(Tick, Duration)>,
    jitter_secs: f64,
}

impl InterpTimeline {
    /// Records a frame for `tick` arriving at local time `now`.
    #[allow(clippy::cast_precision_loss)]
    pub fn on_frame(&mut self, tick: Tick, now: Duration, params: &SessionParams) {
        if let Some((last_tick, last_arrival)) = self.newest {
            if tick <= last_tick {
                return;
            }
            let expected = (tick.0 - last_tick.0) as f64 / params.rate() as f64;
            let actual = now.saturating_sub(last_arrival).as_secs_f64();
            let deviation = (actual - expected).abs();
            self.jitter_secs += (deviation - self.jitter_secs) / 16.0;
        }
        self.newest = Some((tick, now));
    }

    /// Measured frame-arrival jitter (RFC 3550 style mean deviation).
    #[must_use]
    pub fn jitter(&self) -> Duration {
        Duration::from_secs_f64(self.jitter_secs.max(0.0))
    }

    /// Current interpolation time in fractional ticks.
    #[must_use]
    pub const fn time(&self) -> Option<f64> {
        self.time
    }

    /// Advances the timeline by `dt` toward its target.
    #[allow(clippy::cast_precision_loss)]
    pub fn advance(
        &mut self,
        dt: Duration,
        now: Duration,
        delay_ticks: f64,
        params: &SessionParams,
        tuning: &NetTuning,
    ) {
        let Some((newest, arrival)) = self.newest else {
            return;
        };
        let since = params.duration_to_ticks_f64(now.saturating_sub(arrival));
        let target = newest.0 as f64 + since - delay_ticks;
        let Some(time) = self.time.as_mut() else {
            self.time = Some(target);
            return;
        };
        let error = target - *time;
        let resync = params.ms_to_ticks_f64(f64::from(tuning.clock_resync_ms));
        if error.abs() > resync {
            *time = target;
            return;
        }
        let pct = f64::from(tuning.clock_adjust_pct) / 100.0;
        let speed = 1.0 + (error * 0.1).clamp(-pct, pct);
        *time = params.duration_to_ticks_f64(dt).mul_add(speed, *time);
    }

    /// Forgets everything (epoch change).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Presentation-only offset that hides small prediction corrections.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VisualOffset {
    /// Horizontal offset.
    pub x: f32,
    /// Vertical offset.
    pub y: f32,
}

impl VisualOffset {
    /// Adds an error to hide.
    pub fn add(&mut self, dx: f32, dy: f32) {
        self.x += dx;
        self.y += dy;
    }

    /// Exponential decay with the given half-life.
    #[allow(clippy::cast_possible_truncation)]
    pub fn decay(&mut self, dt: Duration, half_life_ms: u32) {
        if half_life_ms == 0 {
            self.reset();
            return;
        }
        let factor = 0.5f64.powf(dt.as_secs_f64() * 1000.0 / f64::from(half_life_ms)) as f32;
        self.x *= factor;
        self.y *= factor;
        if self.magnitude() < 1e-4 {
            self.reset();
        }
    }

    /// Clears the offset (snap).
    pub const fn reset(&mut self) {
        self.x = 0.0;
        self.y = 0.0;
    }

    /// Offset length.
    #[must_use]
    pub fn magnitude(&self) -> f32 {
        self.x.hypot(self.y)
    }
}
