//! Built-in link conditioner: artificial latency, jitter, loss and duplication applied to
//! outgoing traffic inside the network thread.

use std::{fmt, str::FromStr, time::Duration};

/// Environment variable read by [`LinkConditionerConfig::from_env`].
///
/// Format: `latency=80,jitter=15,loss=2,dup=0[,seed=7]` (milliseconds and percentages).
pub const NET_SIM_ENV_VAR: &str = "DRILLGAME_NET_SIM";

/// Upper bound for any single conditioner delay; larger specs are rejected when parsed and
/// clamped when sampled, so a typo cannot overflow timer arithmetic.
const MAX_DELAY: Duration = Duration::from_hours(1);

/// Link conditioner settings. The default applies no conditioning.
///
/// - Reliable messages are only **delayed** (`latency` ± `jitter`), never dropped,
///   duplicated or reordered.
/// - Unreliable messages are delayed independently (so jitter can reorder them), dropped with
///   probability `loss_percent`, and duplicated with probability `duplicate_percent`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinkConditionerConfig {
    /// One-way base delay added to every outgoing message.
    pub latency: Duration,
    /// Maximum deviation (uniform, ±) applied on top of `latency`.
    pub jitter: Duration,
    /// Percentage (0-100) of unreliable messages dropped.
    pub loss_percent: f64,
    /// Percentage (0-100) of unreliable messages sent twice.
    pub duplicate_percent: f64,
    /// RNG seed; `None` derives one from the clock.
    pub seed: Option<u64>,
}

impl LinkConditionerConfig {
    /// Whether this configuration changes traffic at all.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.latency.is_zero()
            || !self.jitter.is_zero()
            || self.loss_percent > 0.0
            || self.duplicate_percent > 0.0
    }

    /// Read [`NET_SIM_ENV_VAR`]. Returns `Ok(None)` when unset or empty.
    ///
    /// # Errors
    ///
    /// Returns an error if the variable is set but malformed.
    pub fn from_env() -> Result<Option<Self>, ParseLinkConditionerError> {
        match std::env::var(NET_SIM_ENV_VAR) {
            Ok(value) if !value.trim().is_empty() => value.parse().map(Some),
            _ => Ok(None),
        }
    }
}

/// Error parsing a link conditioner specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseLinkConditionerError(pub String);

impl fmt::Display for ParseLinkConditionerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {NET_SIM_ENV_VAR} value: {}", self.0)
    }
}

impl std::error::Error for ParseLinkConditionerError {}

impl FromStr for LinkConditionerConfig {
    type Err = ParseLinkConditionerError;

    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        let mut config = Self::default();
        for part in spec
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| ParseLinkConditionerError(format!("expected key=value: {part}")))?;
            let value = value.trim();
            let number = |value: &str| {
                value
                    .parse::<f64>()
                    .ok()
                    .filter(|number| number.is_finite() && *number >= 0.0)
                    .ok_or_else(|| ParseLinkConditionerError(format!("bad number: {part}")))
            };
            let millis = |value: &str| {
                Duration::try_from_secs_f64(number(value)? / 1000.0)
                    .ok()
                    .filter(|duration| *duration <= MAX_DELAY)
                    .ok_or_else(|| {
                        ParseLinkConditionerError(format!("duration out of range: {part}"))
                    })
            };
            match key.trim() {
                "latency" => config.latency = millis(value)?,
                "jitter" => config.jitter = millis(value)?,
                "loss" => config.loss_percent = number(value)?.min(100.0),
                "dup" | "duplicate" => config.duplicate_percent = number(value)?.min(100.0),
                "seed" => {
                    config.seed = Some(
                        value
                            .parse()
                            .map_err(|_| ParseLinkConditionerError(format!("bad seed: {part}")))?,
                    );
                }
                other => {
                    return Err(ParseLinkConditionerError(format!("unknown key: {other}")));
                }
            }
        }
        Ok(config)
    }
}

/// Stateful conditioner owned by the network thread.
#[derive(Debug)]
pub struct LinkConditioner {
    config: LinkConditionerConfig,
    rng: SplitMix64,
}

impl LinkConditioner {
    pub fn new(config: LinkConditionerConfig) -> Self {
        let seed = config.seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0x5eed, |elapsed| {
                    u64::from(elapsed.subsec_nanos()) ^ elapsed.as_secs()
                })
        });
        Self {
            config,
            rng: SplitMix64(seed),
        }
    }

    /// Delay for one message: `latency` plus uniform jitter in `[-jitter, +jitter]`.
    pub fn sample_delay(&mut self) -> Duration {
        let latency = self.config.latency.as_secs_f64();
        let jitter = self.config.jitter.as_secs_f64();
        let offset = if jitter > 0.0 {
            self.rng.next_f64().mul_add(2.0, -1.0) * jitter
        } else {
            0.0
        };
        Duration::try_from_secs_f64((latency + offset).max(0.0))
            .unwrap_or(MAX_DELAY)
            .min(MAX_DELAY)
    }

    /// Delivery plan for one unreliable message: one delay per copy to send (empty = dropped).
    pub fn plan_unreliable(&mut self) -> Vec<Duration> {
        if self.rng.next_f64() * 100.0 < self.config.loss_percent {
            return Vec::new();
        }
        let mut plan = vec![self.sample_delay()];
        if self.rng.next_f64() * 100.0 < self.config.duplicate_percent {
            plan.push(self.sample_delay());
        }
        plan
    }
}

/// Small deterministic PRNG (`SplitMix64`); good enough for traffic shaping.
#[derive(Debug)]
struct SplitMix64(u64);

impl SplitMix64 {
    const fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    #[allow(
        clippy::cast_precision_loss,
        reason = "53-bit mantissa conversion is exact by construction"
    )]
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_spec() {
        let config: LinkConditionerConfig = "latency=80,jitter=15,loss=2,dup=0.5,seed=9"
            .parse()
            .expect("parses");
        assert_eq!(config.latency, Duration::from_millis(80));
        assert_eq!(config.jitter, Duration::from_millis(15));
        assert!((config.loss_percent - 2.0).abs() < f64::EPSILON);
        assert!((config.duplicate_percent - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.seed, Some(9));
        assert!(config.is_active());
    }

    #[test]
    fn empty_spec_is_inactive() {
        let config: LinkConditionerConfig = " ".parse().expect("parses");
        assert!(!config.is_active());
    }

    #[test]
    fn rejects_bad_specs() {
        assert!("latency".parse::<LinkConditionerConfig>().is_err());
        assert!("latency=-3".parse::<LinkConditionerConfig>().is_err());
        assert!("speed=3".parse::<LinkConditionerConfig>().is_err());
        assert!("latency=1e30".parse::<LinkConditionerConfig>().is_err());
        assert!("jitter=1e300".parse::<LinkConditionerConfig>().is_err());
    }

    #[test]
    fn delay_stays_within_jitter_bounds() {
        let mut conditioner = LinkConditioner::new(LinkConditionerConfig {
            latency: Duration::from_millis(50),
            jitter: Duration::from_millis(10),
            seed: Some(1),
            ..LinkConditionerConfig::default()
        });
        for _ in 0..1_000 {
            let delay = conditioner.sample_delay();
            assert!(delay >= Duration::from_millis(40) && delay <= Duration::from_millis(60));
        }
    }

    #[test]
    fn loss_and_duplication_rates_are_roughly_respected() {
        let mut conditioner = LinkConditioner::new(LinkConditionerConfig {
            loss_percent: 20.0,
            duplicate_percent: 50.0,
            seed: Some(42),
            ..LinkConditionerConfig::default()
        });
        let mut dropped = 0;
        let mut duplicated = 0;
        for _ in 0..10_000 {
            match conditioner.plan_unreliable().len() {
                0 => dropped += 1,
                2 => duplicated += 1,
                _ => {}
            }
        }
        assert!((1_500..2_500).contains(&dropped), "dropped {dropped}");
        assert!(
            (3_500..4_500).contains(&duplicated),
            "duplicated {duplicated}"
        );
    }
}
