//! Network thread configuration.

use std::time::Duration;

use crate::conditioner::{LinkConditionerConfig, ParseLinkConditionerError};

/// Default upper bound for one reliable frame payload (16 MiB).
pub const DEFAULT_MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Settings fixed for the lifetime of a [`crate::NetHandle`].
#[derive(Clone, Debug, PartialEq)]
pub struct NetConfig {
    /// QUIC keep-alive interval (must be below `idle_timeout`).
    pub keep_alive_interval: Duration,
    /// QUIC idle timeout; a silent peer is disconnected with [`crate::DisconnectReason::TimedOut`].
    pub idle_timeout: Duration,
    /// Maximum payload size of one framed message on the control stream, enforced on send
    /// and receive. A peer exceeding it is disconnected with a protocol error.
    pub max_frame_bytes: usize,
    /// How often [`crate::NetEvent::Stats`] is emitted per connection (`None` disables).
    pub stats_interval: Option<Duration>,
    /// How long a clean disconnect waits for the peer to acknowledge before force-closing.
    pub close_timeout: Duration,
    /// Time allowed for an inbound connection to open its control stream.
    pub handshake_timeout: Duration,
    /// Tokio worker threads for the network runtime.
    pub worker_threads: usize,
    /// Artificial link conditions applied to outgoing traffic.
    pub link: LinkConditionerConfig,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            keep_alive_interval: Duration::from_secs(2),
            idle_timeout: Duration::from_secs(10),
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            stats_interval: Some(Duration::from_secs(1)),
            close_timeout: Duration::from_secs(2),
            handshake_timeout: Duration::from_secs(5),
            worker_threads: 2,
            link: LinkConditionerConfig::default(),
        }
    }
}

impl NetConfig {
    /// Defaults, with the link conditioner taken from `DRILLGAME_NET_SIM` when set.
    ///
    /// # Errors
    ///
    /// Returns an error if `DRILLGAME_NET_SIM` is set but malformed.
    pub fn from_env() -> Result<Self, ParseLinkConditionerError> {
        Ok(Self {
            link: LinkConditionerConfig::from_env()?.unwrap_or_default(),
            ..Self::default()
        })
    }
}
