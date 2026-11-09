#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # `fatherload_quic_transport`
//!
//! Moves **bytes** between Fatherload peers over QUIC. It knows nothing about gameplay or
//! netcode messages; callers encode/decode their own payloads.
//!
//! ## Model
//!
//! [`NetHandle::start`] spawns one network thread that owns one multi-thread Tokio runtime
//! for its whole life. The game loop talks to it only through channels:
//!
//! - **Commands** ([`NetCommand`]): `Host`, `Join`, `Send`, `Disconnect`, `Shutdown`.
//!   Sending never blocks.
//! - **Events** ([`NetEvent`]): `HostReady`, `Connected`, `Message`, `Disconnected`, `Stats`,
//!   `Error`, `ShutdownComplete`. Drain them each frame with [`NetHandle::try_recv`] or
//!   [`NetHandle::drain`]; neither blocks.
//!
//! ```no_run
//! use fatherload_quic_transport::{HostConfig, NetConfig, NetEvent, NetHandle};
//!
//! let net = NetHandle::start(NetConfig::from_env().unwrap_or_default())?;
//! net.host("0.0.0.0:4242".parse()?, HostConfig::default())?;
//! loop {
//!     for event in net.drain() {
//!         match event {
//!             NetEvent::Connected { conn, .. } => net.send_reliable(conn, b"welcome".to_vec())?,
//!             NetEvent::Message { conn, channel, bytes } => { /* decode */ }
//!             NetEvent::Disconnected { conn, reason } => { /* drop player */ }
//!             _ => {}
//!         }
//!     }
//!     // ... simulate and render one frame ...
//! #   break;
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Channels
//!
//! - [`Channel::Reliable`]: every connection has exactly one long-lived bidirectional
//!   **control stream**, opened at connect. All reliable messages travel on it in order,
//!   framed as `[u32 BE length][u8 tag][payload]`. Frames larger than
//!   [`NetConfig::max_frame_bytes`] are rejected on send and treated as a protocol violation
//!   on receive.
//! - [`Channel::Unreliable`]: QUIC datagrams. If the peer does not support datagrams or the
//!   payload exceeds `max_datagram_size()`, the message falls back to the control stream (it
//!   still arrives tagged `Unreliable`).
//!
//! Reader tasks are never wrapped in timeouts, so partially read frames cannot be lost to
//! cancellation. Keep-alive and idle timeout live in the Quinn transport config.
//!
//! ## Connection lifecycle
//!
//! A host accepts any number of connections. `Disconnect` (and `Shutdown`, for every
//! connection) flushes queued reliable messages, sends a goodbye frame with the reason and
//! waits up to [`NetConfig::close_timeout`] for the peer to close. Each `Connected` is
//! followed by exactly one `Disconnected`, which always comes after that connection's
//! reliable messages.
//!
//! ## Certificates and joining
//!
//! Each `Host` generates a self-signed certificate (rcgen) and reports a serializable
//! [`HostDescriptor`] in `HostReady`. [`JoinTarget::Descriptor`] pins that exact certificate.
//! [`JoinTarget::Addr`] joins by IP alone: still encrypted, but the host is not
//! authenticated. There is no NAT traversal.
//!
//! ## LAN discovery
//!
//! The [`lan`] module publishes/browses sessions over mDNS and serves the descriptor over a
//! tiny TCP endpoint. It is a small synchronous API separate from the handle.
//!
//! ## Link conditioner
//!
//! [`LinkConditionerConfig`] adds latency, jitter, loss and duplication to outgoing traffic
//! inside the network thread. Reliable messages are only delayed (order preserved);
//! unreliable messages are delayed independently, dropped and duplicated. Set it in
//! [`NetConfig::link`] or via `FATHERLOAD_NET_SIM="latency=80,jitter=15,loss=2,dup=0"` with
//! [`NetConfig::from_env`].

mod api;
mod conditioner;
mod config;
mod handle;
pub mod lan;
mod network;
mod tls;

pub use api::{
    Channel, ConnId, Direction, DisconnectReason, HostConfig, JoinTarget, LinkStats, NetCommand,
    NetError, NetErrorKind, NetEvent,
};
pub use conditioner::{LinkConditionerConfig, NET_SIM_ENV_VAR, ParseLinkConditionerError};
pub use config::{DEFAULT_MAX_FRAME_BYTES, NetConfig};
pub use handle::{NetHandle, NetThreadStopped};
pub use network::STREAM_MAGIC;
pub use tls::{ALPN, DEFAULT_SERVER_NAME, HostDescriptor};
