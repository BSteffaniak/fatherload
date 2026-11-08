//! Seeded deterministic link simulator.

use std::collections::BTreeMap;
use std::time::Duration;

/// `SplitMix64`: tiny, fast, deterministic RNG for link simulation and bots.
#[derive(Clone, Debug)]
pub struct DetRng {
    state: u64,
}

impl DetRng {
    /// Creates a generator from a seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next 64 random bits.
    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform float in `[0, 1)`.
    #[allow(clippy::cast_precision_loss)]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// `true` with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        p > 0.0 && self.next_f64() < p
    }

    /// Uniform integer in `[0, n)` (`0` when `n == 0`).
    pub const fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }
}

/// Link impairments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkConditions {
    /// One-way base latency.
    pub latency: Duration,
    /// Uniform jitter in `[-jitter, +jitter]` added to the latency.
    pub jitter: Duration,
    /// Loss probability per packet.
    pub loss: f64,
    /// Duplication probability per packet (unreliable links only).
    pub duplicate: f64,
    /// Probability a packet takes an extra random delay (unreliable only).
    pub reorder: f64,
}

impl LinkConditions {
    /// A perfect link.
    #[must_use]
    pub const fn perfect() -> Self {
        Self {
            latency: Duration::ZERO,
            jitter: Duration::ZERO,
            loss: 0.0,
            duplicate: 0.0,
            reorder: 0.0,
        }
    }

    /// Latency and jitter in milliseconds, loss in percent; no duplication or
    /// reordering.
    #[must_use]
    pub fn new(latency_ms: u64, jitter_ms: u64, loss_pct: f64) -> Self {
        Self {
            latency: Duration::from_millis(latency_ms),
            jitter: Duration::from_millis(jitter_ms),
            loss: loss_pct / 100.0,
            duplicate: 0.0,
            reorder: 0.0,
        }
    }

    /// Adds duplication and reordering probabilities (percent).
    #[must_use]
    pub fn with_chaos(mut self, duplicate_pct: f64, reorder_pct: f64) -> Self {
        self.duplicate = duplicate_pct / 100.0;
        self.reorder = reorder_pct / 100.0;
        self
    }
}

impl Default for LinkConditions {
    fn default() -> Self {
        Self::perfect()
    }
}

/// Counters for one link direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkStats {
    /// Packets handed to `send`.
    pub sent: u64,
    /// Packets delivered by `poll` (including duplicates).
    pub delivered: u64,
    /// Packets lost (unreliable) or retransmitted (reliable).
    pub lost: u64,
    /// Extra copies created.
    pub duplicated: u64,
    /// Largest number of packets in flight at once.
    pub max_in_flight: usize,
}

/// A one-way link with deterministic impairments.
///
/// * Unreliable mode (datagrams): packets may be dropped, duplicated and
///   reordered.
/// * Reliable mode (an ordered QUIC stream): nothing is dropped; a "lost"
///   packet is delayed by a retransmission round trip and everything behind
///   it waits (head-of-line blocking), preserving order.
#[derive(Clone, Debug)]
pub struct LinkSim<T> {
    conditions: LinkConditions,
    reliable: bool,
    rng: DetRng,
    queue: BTreeMap<(u128, u64), T>,
    next_id: u64,
    last_delivery: u128,
    stats: LinkStats,
}

impl<T: Clone> LinkSim<T> {
    /// Creates a link.
    #[must_use]
    pub const fn new(conditions: LinkConditions, reliable: bool, seed: u64) -> Self {
        Self {
            conditions,
            reliable,
            rng: DetRng::new(seed),
            queue: BTreeMap::new(),
            next_id: 0,
            last_delivery: 0,
            stats: LinkStats {
                sent: 0,
                delivered: 0,
                lost: 0,
                duplicated: 0,
                max_in_flight: 0,
            },
        }
    }

    /// Current impairments.
    #[must_use]
    pub const fn conditions(&self) -> LinkConditions {
        self.conditions
    }

    /// Counters.
    #[must_use]
    pub const fn stats(&self) -> LinkStats {
        self.stats
    }

    /// Packets in flight.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.queue.len()
    }

    /// Drops everything in flight (connection torn down).
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Sends `packet` at local time `now`.
    pub fn send(&mut self, now: Duration, packet: T) {
        self.stats.sent += 1;
        let now = now.as_nanos();
        if self.reliable {
            let mut at = now + self.sample_delay();
            while self.rng.chance(self.conditions.loss) {
                self.stats.lost += 1;
                at += 2 * self.conditions.latency.as_nanos() + self.sample_jitter_abs();
            }
            at = at.max(self.last_delivery);
            self.last_delivery = at;
            self.enqueue(at, packet);
            return;
        }
        if self.rng.chance(self.conditions.loss) {
            self.stats.lost += 1;
            return;
        }
        if self.rng.chance(self.conditions.duplicate) {
            self.stats.duplicated += 1;
            let at = now + self.sample_delay() + self.sample_reorder();
            self.enqueue(at, packet.clone());
        }
        let at = now + self.sample_delay() + self.sample_reorder();
        self.enqueue(at, packet);
    }

    /// Returns every packet due at or before `now`, in delivery order.
    pub fn poll(&mut self, now: Duration) -> Vec<T> {
        let now = now.as_nanos();
        let later = self.queue.split_off(&(now + 1, 0));
        let due = std::mem::replace(&mut self.queue, later);
        self.stats.delivered += due.len() as u64;
        due.into_values().collect()
    }

    fn enqueue(&mut self, at: u128, packet: T) {
        self.queue.insert((at, self.next_id), packet);
        self.next_id += 1;
        self.stats.max_in_flight = self.stats.max_in_flight.max(self.queue.len());
    }

    fn sample_delay(&mut self) -> u128 {
        let base = self.conditions.latency.as_nanos();
        let jitter = self.conditions.jitter.as_nanos();
        if jitter == 0 {
            return base;
        }
        let span = u64::try_from(jitter * 2 + 1).unwrap_or(u64::MAX);
        let offset = u128::from(self.rng.below(span));
        (base + offset).saturating_sub(jitter)
    }

    fn sample_jitter_abs(&mut self) -> u128 {
        let jitter = u64::try_from(self.conditions.jitter.as_nanos()).unwrap_or(u64::MAX);
        u128::from(self.rng.below(jitter.saturating_add(1)))
    }

    fn sample_reorder(&mut self) -> u128 {
        if !self.rng.chance(self.conditions.reorder) {
            return 0;
        }
        let span = self.conditions.jitter.as_nanos() * 2 + self.conditions.latency.as_nanos() / 2;
        let span = u64::try_from(span).unwrap_or(u64::MAX).max(1);
        u128::from(self.rng.below(span))
    }
}
