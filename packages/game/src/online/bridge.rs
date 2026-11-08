//! The net bridge: maps netcode messages to `quic_transport` bytes and back (plan §9, §10).
//!
//! `quic_transport` moves opaque bytes on a reliable ordered control stream or as best-effort
//! datagrams; `drillgame_netcode` produces and consumes typed [`ClientMessage`] /
//! [`ServerMessage`] values. This module is the only place the two meet:
//!
//! - every message is encoded with the versioned postcard codec (`drillgame_netcode::protocol`);
//! - client `Inputs` travel as datagrams ([`Channel::Unreliable`]); everything else (hello,
//!   ready, leave, keyframe requests, and every host message including frames) goes on the
//!   reliable stream, as plan §9 prescribes until playtests show head-of-line stalls;
//! - transport connection ids double as netcode client ids.

use drillgame_netcode::{ClientId, ClientMessage, CodecError, ServerMessage, protocol};
use drillgame_quic_transport::{Channel, ConnId, NetHandle, NetThreadStopped};
use drillgame_simulation::WorldState;

/// Netcode client message over the real world.
pub type WorldClientMessage = ClientMessage<WorldState>;
/// Netcode host message over the real world.
pub type WorldServerMessage = ServerMessage<WorldState>;

/// Why a message could not be handed to the transport.
#[derive(Debug)]
pub enum BridgeError {
    /// The message could not be encoded.
    Codec(CodecError),
    /// The network thread has stopped.
    Stopped,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Codec(error) => write!(f, "encode failed: {error}"),
            Self::Stopped => f.write_str("network thread has stopped"),
        }
    }
}

impl std::error::Error for BridgeError {}

impl From<CodecError> for BridgeError {
    fn from(error: CodecError) -> Self {
        Self::Codec(error)
    }
}

impl From<NetThreadStopped> for BridgeError {
    fn from(_: NetThreadStopped) -> Self {
        Self::Stopped
    }
}

/// The netcode client id for a transport connection.
#[must_use]
pub const fn client_id(conn: ConnId) -> ClientId {
    ClientId(conn.0)
}

/// The transport connection for a netcode client id.
#[must_use]
pub const fn conn_id(client: ClientId) -> ConnId {
    ConnId(client.0)
}

/// Delivery class of a client message: input datagrams are unreliable, the rest reliable.
#[must_use]
pub const fn client_channel(message: &WorldClientMessage) -> Channel {
    if message.is_unreliable() {
        Channel::Unreliable
    } else {
        Channel::Reliable
    }
}

/// Delivery class of a host message. Every host message, frames included, uses the ordered
/// control stream (plan §9).
#[must_use]
pub const fn server_channel(_message: &WorldServerMessage) -> Channel {
    Channel::Reliable
}

/// Encodes a client message with its channel.
///
/// # Errors
///
/// Returns the codec error if serialization fails.
pub fn encode_client(message: &WorldClientMessage) -> Result<(Channel, Vec<u8>), CodecError> {
    Ok((client_channel(message), protocol::encode(message)?))
}

/// Encodes a host message with its channel.
///
/// # Errors
///
/// Returns the codec error if serialization fails.
pub fn encode_server(message: &WorldServerMessage) -> Result<(Channel, Vec<u8>), CodecError> {
    Ok((server_channel(message), protocol::encode(message)?))
}

/// Sends one client message to the host connection. Returns the payload size.
///
/// # Errors
///
/// Returns an error if encoding fails or the network thread stopped.
pub fn send_client(
    net: &NetHandle,
    conn: ConnId,
    message: &WorldClientMessage,
) -> Result<usize, BridgeError> {
    let (channel, bytes) = encode_client(message)?;
    let size = bytes.len();
    send_bytes(net, conn, channel, bytes)?;
    Ok(size)
}

/// Sends one host message to a client. Returns the payload size.
///
/// # Errors
///
/// Returns an error if encoding fails or the network thread stopped.
pub fn send_server(
    net: &NetHandle,
    client: ClientId,
    message: &WorldServerMessage,
) -> Result<usize, BridgeError> {
    let (channel, bytes) = encode_server(message)?;
    let size = bytes.len();
    send_bytes(net, conn_id(client), channel, bytes)?;
    Ok(size)
}

fn send_bytes(
    net: &NetHandle,
    conn: ConnId,
    channel: Channel,
    bytes: Vec<u8>,
) -> Result<(), NetThreadStopped> {
    match channel {
        Channel::Reliable => net.send_reliable(conn, bytes),
        Channel::Unreliable => net.send_unreliable(conn, bytes),
    }
}

/// A byte-rate meter over a sliding one-second window.
#[derive(Clone, Copy, Debug, Default)]
pub struct RateMeter {
    window_seconds: f32,
    window_count: u64,
    rate: f32,
}

impl RateMeter {
    /// Counts `amount` (bytes, corrections, ...).
    pub const fn add(&mut self, amount: u64) {
        self.window_count = self.window_count.saturating_add(amount);
    }

    /// Advances the window; the rate updates once per second.
    #[allow(
        clippy::cast_precision_loss,
        reason = "per-second counters are small enough for f32 display"
    )]
    pub fn tick(&mut self, delta_seconds: f32) {
        self.window_seconds += delta_seconds.max(0.0);
        if self.window_seconds >= 1.0 {
            self.rate = self.window_count as f32 / self.window_seconds;
            self.window_seconds = 0.0;
            self.window_count = 0;
        }
    }

    /// Most recent per-second rate.
    #[must_use]
    pub const fn rate(&self) -> f32 {
        self.rate
    }
}

#[cfg(test)]
mod tests {
    use drillgame_netcode::{ActionId, PROTOCOL_VERSION, ReconnectToken, SessionEpoch};

    use super::*;

    #[test]
    fn inputs_use_datagrams_and_everything_else_the_stream() {
        let inputs = WorldClientMessage::Inputs {
            epoch: SessionEpoch(1),
            frames: Vec::new(),
            actions: Vec::new(),
            action_floor: ActionId(1),
        };
        assert_eq!(client_channel(&inputs), Channel::Unreliable);
        let hello = WorldClientMessage::Hello {
            token: Some(ReconnectToken(7)),
            name: "Miner".to_owned(),
            version: PROTOCOL_VERSION,
        };
        assert_eq!(client_channel(&hello), Channel::Reliable);
        assert_eq!(
            client_channel(&WorldClientMessage::Leave),
            Channel::Reliable
        );
        let ended = WorldServerMessage::SessionEnded {
            reason: drillgame_netcode::EndReason::HostClosed,
        };
        assert_eq!(server_channel(&ended), Channel::Reliable);
    }

    #[test]
    fn encoded_messages_round_trip_through_the_codec() {
        let hello = WorldClientMessage::Hello {
            token: None,
            name: "Rig".to_owned(),
            version: PROTOCOL_VERSION,
        };
        let (_, bytes) = encode_client(&hello).expect("encodes");
        let decoded: WorldClientMessage = protocol::decode(&bytes).expect("decodes");
        assert!(matches!(decoded, WorldClientMessage::Hello { name, .. } if name == "Rig"));
        assert_eq!(client_id(ConnId(9)), ClientId(9));
        assert_eq!(conn_id(ClientId(9)), ConnId(9));
    }

    #[test]
    fn rate_meter_reports_per_second_rates() {
        let mut meter = RateMeter::default();
        meter.add(500);
        meter.tick(0.5);
        assert!(
            meter.rate().abs() < f32::EPSILON,
            "no rate before a full second"
        );
        meter.add(500);
        meter.tick(0.5);
        assert!((meter.rate() - 1_000.0).abs() < 1.0);
        meter.tick(1.0);
        assert!(meter.rate().abs() < f32::EPSILON, "an idle second reads zero");
    }
}
