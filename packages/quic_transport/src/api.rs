//! Command and event types exchanged between the game loop and the network thread.

use std::{fmt, net::SocketAddr, time::Duration};

use crate::descriptor::HostDescriptor;

/// Identifier for one live QUIC connection, unique for the lifetime of a [`crate::NetHandle`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnId(pub u64);

impl fmt::Display for ConnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "conn#{}", self.0)
    }
}

/// Delivery class for a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    /// Reliable and ordered, carried on the connection's single control stream.
    Reliable,
    /// Best effort, carried as a QUIC datagram. Falls back to the control stream when the
    /// peer does not support datagrams or the payload exceeds the current datagram size.
    Unreliable,
}

/// Which side opened a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    /// Accepted by a local host endpoint.
    Inbound,
    /// Opened by a local `Join`.
    Outbound,
}

/// Options for a hosting endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostConfig {
    /// TLS server name embedded in the self-signed certificate and the descriptor.
    pub server_name: String,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            server_name: crate::DEFAULT_SERVER_NAME.to_owned(),
        }
    }
}

/// Where to connect for a `Join`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JoinTarget {
    /// Connect using a host descriptor; the host certificate is pinned to the descriptor's.
    Descriptor(HostDescriptor),
    /// Direct-IP join. The connection is encrypted but the host certificate is not
    /// authenticated (there is no out-of-band certificate to pin against).
    Addr(SocketAddr),
}

/// Commands sent from the game loop to the network thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetCommand {
    /// Bind a host endpoint and accept any number of connections.
    Host {
        /// Local UDP address to bind (port 0 picks an ephemeral port).
        bind: SocketAddr,
        /// Hosting options.
        config: HostConfig,
    },
    /// Connect to a host.
    Join {
        /// Where to connect.
        target: JoinTarget,
    },
    /// Send one message on a connection.
    Send {
        /// Destination connection.
        conn: ConnId,
        /// Delivery class.
        channel: Channel,
        /// Opaque payload.
        bytes: Vec<u8>,
    },
    /// Flush pending reliable messages, then close the connection cleanly.
    Disconnect {
        /// Connection to close.
        conn: ConnId,
        /// Reason delivered to the peer.
        reason: String,
    },
    /// Disconnect everything, close endpoints and stop the network thread.
    Shutdown,
}

/// Why a connection ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisconnectReason {
    /// Closed by a local `Disconnect` or `Shutdown`, with the given reason.
    Local(String),
    /// Closed by the peer, with the reason it supplied.
    Remote(String),
    /// The idle timeout elapsed without hearing from the peer.
    TimedOut,
    /// The peer violated the framing protocol (oversized frame, unknown tag, ...).
    Protocol(String),
    /// The connection failed for another transport-level reason.
    Lost(String),
}

/// Periodic per-connection transport statistics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkStats {
    /// Smoothed round-trip time estimate.
    pub rtt: Duration,
    /// Packets sent on the path so far.
    pub sent_packets: u64,
    /// Packets declared lost on the path so far.
    pub lost_packets: u64,
    /// `lost_packets / sent_packets`, in `0.0..=1.0`.
    pub loss: f64,
    /// UDP payload bytes sent.
    pub bytes_sent: u64,
    /// UDP payload bytes received.
    pub bytes_received: u64,
    /// Largest datagram payload the peer currently accepts, if datagrams are supported.
    pub max_datagram_size: Option<usize>,
}

/// Which operation an error relates to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NetErrorKind {
    /// Binding or configuring a host endpoint failed.
    Host,
    /// A join attempt failed.
    Join,
    /// A send could not be performed (unknown connection, oversized payload, ...).
    Send,
    /// A disconnect could not be performed.
    Disconnect,
}

/// A non-fatal error report from the network thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetError {
    /// Operation the error relates to.
    pub kind: NetErrorKind,
    /// Connection involved, if any.
    pub conn: Option<ConnId>,
    /// Human readable detail.
    pub message: String,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.conn {
            Some(conn) => write!(f, "{:?} error on {conn}: {}", self.kind, self.message),
            None => write!(f, "{:?} error: {}", self.kind, self.message),
        }
    }
}

impl std::error::Error for NetError {}

/// Events produced by the network thread, drained by the game loop.
#[derive(Clone, Debug, PartialEq)]
pub enum NetEvent {
    /// A host endpoint is bound and accepting connections.
    HostReady {
        /// Actual bound local address.
        addr: SocketAddr,
        /// Descriptor clients can use to join with a pinned certificate.
        descriptor: HostDescriptor,
    },
    /// A connection finished its handshake and its control stream is open.
    Connected {
        /// New connection id.
        conn: ConnId,
        /// Peer address.
        remote: SocketAddr,
        /// Which side opened the connection.
        direction: Direction,
    },
    /// A message arrived.
    Message {
        /// Source connection.
        conn: ConnId,
        /// Channel the sender used.
        channel: Channel,
        /// Opaque payload.
        bytes: Vec<u8>,
    },
    /// A connection ended. Emitted exactly once per `Connected`, after all of its reliable
    /// messages.
    Disconnected {
        /// Connection that ended.
        conn: ConnId,
        /// Why it ended.
        reason: DisconnectReason,
    },
    /// Periodic transport statistics for a connection.
    Stats {
        /// Connection measured.
        conn: ConnId,
        /// Measurements.
        stats: LinkStats,
    },
    /// A non-fatal error.
    Error(NetError),
    /// The network thread has finished shutting down; no further events follow.
    ShutdownComplete,
}
