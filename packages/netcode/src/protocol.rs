//! Wire protocol (plan §7) and the versioned postcard codec.
//!
//! Every encoded message is `[PROTOCOL_VERSION, postcard bytes...]`.
//! `ClientMessage::Inputs` is meant for unreliable datagrams; everything else
//! goes over the reliable ordered control stream.

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::ids::{ActionId, ClientId, InputSeq, PlayerId, ReconnectToken, SessionEpoch, Tick};
use crate::params::{NetTuning, SessionParams};
use crate::simulation::{ActionEnvelope, NetSimulation, Pose};

/// Version byte prefixed to every encoded message.
pub const PROTOCOL_VERSION: u8 = 1;

/// One client input frame: the controls and actions for exactly one tick.
///
/// The client predicts exactly the frame it sends (`target_tick` is the tick
/// the client simulated with it).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct InputFrame<S: NetSimulation> {
    /// Strictly increasing per client.
    pub seq: InputSeq,
    /// The tick this input is for (the tick produced by simulating it).
    pub target_tick: Tick,
    /// Held controls.
    pub controls: S::Controls,
    /// One-shot actions issued on this tick.
    pub actions: Vec<ActionEnvelope<S::Action>>,
}

/// An unacknowledged action resent outside its (no longer redundant) frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct TaggedAction<S: NetSimulation> {
    /// Sequence of the frame it was issued in.
    pub seq: InputSeq,
    /// Tick it was issued for.
    pub target_tick: Tick,
    /// The action.
    pub envelope: ActionEnvelope<S::Action>,
}

/// Client to host.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub enum ClientMessage<S: NetSimulation> {
    /// First message on a connection.
    Hello {
        /// Token from a previous `Welcome`, to reclaim the same player.
        token: Option<ReconnectToken>,
        /// Display name.
        name: String,
        /// Must equal [`PROTOCOL_VERSION`].
        version: u8,
    },
    /// Input datagram: newest frame plus redundant unacknowledged frames.
    Inputs {
        /// Epoch the inputs belong to; stale epochs are rejected.
        epoch: SessionEpoch,
        /// Newest frame plus up to `input_redundancy - 1` older unacked frames.
        frames: Vec<InputFrame<S>>,
        /// Unacknowledged actions whose frames are no longer carried.
        actions: Vec<TaggedAction<S>>,
        /// Every action id below this is settled from the client's view.
        action_floor: ActionId,
    },
    /// Lobby ready toggle.
    Ready {
        /// Ready state.
        ready: bool,
    },
    /// Ask for a full keyframe (e.g. after a debug hash mismatch).
    KeyframeRequest {
        /// Current epoch.
        epoch: SessionEpoch,
    },
    /// Leave the session for good (no reconnect reservation).
    Leave,
}

impl<S: NetSimulation> ClientMessage<S> {
    /// Whether this message belongs on the unreliable datagram channel.
    #[must_use]
    pub const fn is_unreliable(&self) -> bool {
        matches!(self, Self::Inputs { .. })
    }
}

/// Lobby roster entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyEntry {
    /// The player.
    pub player: PlayerId,
    /// Display name.
    pub name: String,
    /// Ready toggle.
    pub ready: bool,
    /// `false` while the player's client is disconnected but reserved.
    pub connected: bool,
    /// Whether the player is a local seat on the host.
    pub local: bool,
}

/// Position-only data for players outside the relevance radius (minimap).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerMarker {
    /// The player.
    pub player: PlayerId,
    /// Its pose.
    pub pose: Pose,
}

/// A gameplay event with its stable identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetEvent<E> {
    /// Tick that produced it.
    pub tick: Tick,
    /// Owning player, if any.
    pub player: Option<PlayerId>,
    /// Input sequence the host applied for `player` on that tick, if any.
    pub seq: Option<InputSeq>,
    /// Event type discriminant.
    pub kind: u32,
    /// Index among events with the same `(player, kind)` on that tick.
    pub ordinal: u32,
    /// The event.
    pub payload: E,
}

/// Why the host refused a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    /// Protocol versions differ.
    VersionMismatch {
        /// Host version.
        expected: u8,
        /// Client version.
        found: u8,
    },
    /// The reconnect token is currently owned by another live connection.
    TokenInUse,
    /// The session is no longer accepting players.
    NotAccepting,
}

/// Why the session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    /// The host closed the session.
    HostClosed,
    /// The host removed this client.
    Kicked,
    /// This client left on its own (local only; never sent by the host).
    Left,
}

/// Initial state for a newly joined client.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Welcome<S: NetSimulation> {
    /// Transport connection id.
    pub client: ClientId,
    /// Assigned (or reclaimed) player.
    pub player: PlayerId,
    /// Current epoch.
    pub epoch: SessionEpoch,
    /// Token to reclaim `player` after a disconnect.
    pub token: ReconnectToken,
    /// Host tick at the time of the keyframe.
    pub host_tick: Tick,
    /// `false` while in the lobby (wait for `StartGame`).
    pub started: bool,
    /// Fixed session parameters.
    pub session_params: SessionParams,
    /// This client's network tuning.
    pub tuning: NetTuning,
    /// Full world at `host_tick`.
    pub keyframe: S::Keyframe,
}

/// Host to client, per frame (plan §7).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct ServerFrame<S: NetSimulation> {
    /// Epoch.
    pub epoch: SessionEpoch,
    /// Host tick this frame describes.
    pub tick: Tick,
    /// Highest input sequence consumed by the simulation for this client.
    pub ack_seq: Option<InputSeq>,
    /// Every action id at or below this is applied (or settled).
    pub ack_action: ActionId,
    /// Minimum `(target tick of newest input) - (host tick at arrival)` since
    /// the previous frame.
    pub input_margin: Option<i32>,
    /// Sequence of the newest input the margin was measured from.
    pub margin_seq: InputSeq,
    /// Full player roster, only when it changed for this client.
    pub roster: Option<Vec<PlayerId>>,
    /// Per-tick state of relevant players (always includes the viewer).
    pub players: Vec<(PlayerId, S::PlayerState)>,
    /// Low-rate markers for non-relevant players, when due.
    pub far_players: Option<Vec<PlayerMarker>>,
    /// Relevance-filtered per-tick entities.
    pub entities: S::Entities,
    /// Op-log entries since the previous frame, in order.
    pub terrain_ops: Vec<(Tick, S::TerrainOp)>,
    /// On-change sections whose hash changed.
    pub sections: Vec<(S::SectionKey, S::Section)>,
    /// On-change sections that no longer exist.
    pub removed_sections: Vec<S::SectionKey>,
    /// Gameplay events since the previous frame.
    pub events: Vec<NetEvent<S::Event>>,
    /// Full-world hash after this tick (debug builds / harness).
    pub debug_hash: Option<u64>,
}

/// Host to client.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub enum ServerMessage<S: NetSimulation> {
    /// Join accepted.
    Welcome(Box<Welcome<S>>),
    /// Per-frame replication.
    Frame(Box<ServerFrame<S>>),
    /// Lobby roster and ready states.
    Lobby {
        /// Roster.
        roster: Vec<LobbyEntry>,
    },
    /// The game started at `tick`.
    StartGame {
        /// Epoch.
        epoch: SessionEpoch,
        /// Tick the host starts simulating from.
        tick: Tick,
    },
    /// Full world (on request or after an epoch change).
    Keyframe {
        /// Epoch.
        epoch: SessionEpoch,
        /// Tick of `world`.
        tick: Tick,
        /// Full world.
        world: Box<S::Keyframe>,
    },
    /// This client's tuning changed.
    TuningChanged {
        /// New tuning.
        tuning: NetTuning,
    },
    /// Connection refused.
    Rejected {
        /// Why.
        reason: RejectReason,
    },
    /// The session is over.
    SessionEnded {
        /// Why.
        reason: EndReason,
    },
}

/// Codec failure.
#[derive(Debug)]
pub enum CodecError {
    /// Zero-length message.
    Empty,
    /// The version byte did not match [`PROTOCOL_VERSION`].
    Version {
        /// Expected version.
        expected: u8,
        /// Received version.
        found: u8,
    },
    /// Postcard serialization or deserialization error.
    Postcard(postcard::Error),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty message"),
            Self::Version { expected, found } => {
                write!(
                    f,
                    "protocol version mismatch: expected {expected}, found {found}"
                )
            }
            Self::Postcard(error) => write!(f, "postcard: {error}"),
        }
    }
}

impl std::error::Error for CodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Postcard(error) => Some(error),
            _ => None,
        }
    }
}

impl From<postcard::Error> for CodecError {
    fn from(error: postcard::Error) -> Self {
        Self::Postcard(error)
    }
}

/// Encodes a message as `[PROTOCOL_VERSION, postcard...]`.
///
/// # Errors
///
/// Returns [`CodecError::Postcard`] if serialization fails.
pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>, CodecError> {
    Ok(postcard::to_extend(message, vec![PROTOCOL_VERSION])?)
}

/// Decodes a message produced by [`encode`].
///
/// # Errors
///
/// Returns an error for empty input, a version mismatch or malformed bytes.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
    let (&version, body) = bytes.split_first().ok_or(CodecError::Empty)?;
    if version != PROTOCOL_VERSION {
        return Err(CodecError::Version {
            expected: PROTOCOL_VERSION,
            found: version,
        });
    }
    Ok(postcard::from_bytes(body)?)
}
