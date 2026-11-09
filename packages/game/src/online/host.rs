//! The transport side of an online host: one `quic_transport` network thread feeding a
//! [`HostSession`], plus LAN publishing.
//!
//! The game loop calls [`HostNet::pump`] once per frame (non-blocking drain of transport events
//! into the session) and [`HostNet::send`] with whatever the session queued. Nothing here
//! blocks the game thread: stopping the network thread happens on a helper thread.

use std::{collections::BTreeMap, net::SocketAddr};

use fatherload_netcode::{ClientId, HostSession};
use fatherload_quic_transport::{
    Direction, DisconnectReason, HostConfig as TransportHostConfig, HostDescriptor, LinkStats,
    NetErrorKind, NetEvent, NetHandle,
    lan::{LanDescriptorServer, LanDiscoveryPublisher, LanGameAdvertisement, likely_lan_ip},
};
use fatherload_simulation::WorldState;

use super::{
    bridge::{self, RateMeter, WorldServerMessage},
    net_config,
};

/// Where the host endpoint is in its lifecycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostNetStatus {
    /// Binding the endpoint.
    Starting,
    /// Accepting connections.
    Ready {
        /// Address peers should connect to (unspecified binds are shown as the LAN address).
        addr: SocketAddr,
        /// Descriptor with the pinned certificate, `addr` patched in.
        descriptor: HostDescriptor,
    },
    /// The endpoint could not be bound.
    Failed(String),
}

/// One connected peer as the transport sees it.
#[derive(Clone, Debug)]
pub struct HostConnection {
    /// Peer address.
    pub remote: SocketAddr,
    /// Latest transport statistics.
    pub stats: Option<LinkStats>,
}

/// mDNS advertisement plus the descriptor endpoint, alive while hosting.
struct LanPresence {
    _publisher: LanDiscoveryPublisher,
    _server: LanDescriptorServer,
}

impl std::fmt::Debug for LanPresence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LanPresence")
    }
}

/// The network side of an online host.
#[derive(Debug)]
pub struct HostNet {
    net: Option<NetHandle>,
    status: HostNetStatus,
    conns: BTreeMap<ClientId, HostConnection>,
    publish_lan: bool,
    lan: Option<LanPresence>,
    up: RateMeter,
    down: RateMeter,
    notices: Vec<String>,
}

impl HostNet {
    /// Starts the network thread and binds a host endpoint on `bind` (`HostReady` follows on a
    /// later [`Self::pump`]). With `publish_lan`, the session is advertised over mDNS once bound.
    ///
    /// # Errors
    ///
    /// Returns an error if the network thread cannot be started.
    pub fn start(bind: SocketAddr, publish_lan: bool) -> std::io::Result<Self> {
        let net = NetHandle::start(net_config())?;
        net.host(bind, TransportHostConfig::default())
            .map_err(std::io::Error::other)?;
        Ok(Self {
            net: Some(net),
            status: HostNetStatus::Starting,
            conns: BTreeMap::new(),
            publish_lan,
            lan: None,
            up: RateMeter::default(),
            down: RateMeter::default(),
            notices: Vec::new(),
        })
    }

    /// Endpoint status.
    #[must_use]
    pub const fn status(&self) -> &HostNetStatus {
        &self.status
    }

    /// The address peers connect to, once bound.
    #[must_use]
    pub const fn addr(&self) -> Option<SocketAddr> {
        match &self.status {
            HostNetStatus::Ready { addr, .. } => Some(*addr),
            _ => None,
        }
    }

    /// The descriptor (JSON) peers can join with, once bound.
    #[must_use]
    pub fn descriptor_json(&self) -> Option<String> {
        match &self.status {
            HostNetStatus::Ready { descriptor, .. } => descriptor.to_json().ok(),
            _ => None,
        }
    }

    /// Whether the session is advertised on the LAN.
    #[must_use]
    pub const fn lan_published(&self) -> bool {
        self.lan.is_some()
    }

    /// Live transport connections.
    #[must_use]
    pub const fn connections(&self) -> &BTreeMap<ClientId, HostConnection> {
        &self.conns
    }

    /// Bytes per second sent / received over the last second.
    #[must_use]
    pub const fn byte_rates(&self) -> (f32, f32) {
        (self.up.rate(), self.down.rate())
    }

    /// Notices (errors, joins, leaves) since the last call.
    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    /// Drains every pending transport event into `session`. Never blocks.
    pub fn pump(&mut self, session: &mut HostSession<WorldState>, delta_seconds: f32) {
        self.up.tick(delta_seconds);
        self.down.tick(delta_seconds);
        let Some(events) = self.net.as_ref().map(NetHandle::drain) else {
            return;
        };
        let mut malformed = Vec::new();
        for event in events {
            match event {
                NetEvent::HostReady { addr, descriptor } => self.on_ready(addr, descriptor),
                NetEvent::Connected {
                    conn,
                    remote,
                    direction: Direction::Inbound,
                } => {
                    let client = bridge::client_id(conn);
                    self.conns.insert(
                        client,
                        HostConnection {
                            remote,
                            stats: None,
                        },
                    );
                    session.client_connected(client);
                }
                NetEvent::Connected { .. } | NetEvent::ShutdownComplete => {}
                NetEvent::Message { conn, bytes, .. } => {
                    self.down.add(bytes.len() as u64);
                    let client = bridge::client_id(conn);
                    if let Err(error) = session.handle_client_bytes(client, &bytes) {
                        self.notices
                            .push(format!("Dropped a malformed message from {conn}: {error}"));
                        malformed.push(conn);
                    }
                }
                NetEvent::Disconnected { conn, reason } => {
                    let client = bridge::client_id(conn);
                    if self.conns.remove(&client).is_some() {
                        session.client_disconnected(client);
                        if !matches!(reason, DisconnectReason::Local(_)) {
                            self.notices
                                .push(format!("A player disconnected ({}).", reason_text(&reason)));
                        }
                    }
                }
                NetEvent::Stats { conn, stats } => {
                    if let Some(connection) = self.conns.get_mut(&bridge::client_id(conn)) {
                        connection.stats = Some(stats);
                    }
                }
                NetEvent::Error(error) => {
                    if error.kind == NetErrorKind::Host {
                        self.status = HostNetStatus::Failed(error.message.clone());
                    }
                    self.notices.push(error.to_string());
                }
            }
        }
        if let Some(net) = self.net.as_ref() {
            for conn in malformed {
                let _ignored = net.disconnect(conn, "malformed message");
            }
        }
    }

    /// Sends the session's queued messages.
    pub fn send(&mut self, outbound: Vec<(ClientId, WorldServerMessage)>) {
        let Some(net) = self.net.as_ref() else {
            return;
        };
        for (client, message) in outbound {
            match bridge::send_server(net, client, &message) {
                Ok(size) => self.up.add(size as u64),
                Err(error) => self.notices.push(format!("Send failed: {error}")),
            }
        }
    }

    /// Closes every connection (after queued messages, e.g. `SessionEnded`, are flushed) and
    /// stops the network thread, waiting for it. For headless tools and tests; the game thread
    /// simply drops the host, which does the same on a helper thread.
    pub fn shutdown_blocking(mut self) {
        self.lan = None;
        if let Some(net) = self.net.take() {
            let _events = net.shutdown();
        }
    }

    fn on_ready(&mut self, bound: SocketAddr, descriptor: HostDescriptor) {
        let addr = if bound.ip().is_unspecified() {
            SocketAddr::new(
                likely_lan_ip().unwrap_or_else(|| std::net::Ipv4Addr::LOCALHOST.into()),
                bound.port(),
            )
        } else {
            bound
        };
        let descriptor = descriptor.with_host_addr(addr);
        if self.publish_lan && self.lan.is_none() {
            match publish_on_lan(&descriptor) {
                Ok(lan) => self.lan = Some(lan),
                Err(error) => self
                    .notices
                    .push(format!("LAN discovery unavailable: {error}")),
            }
        }
        self.status = HostNetStatus::Ready { addr, descriptor };
    }
}

impl Drop for HostNet {
    fn drop(&mut self) {
        let lan = self.lan.take();
        let net = self.net.take();
        if lan.is_some() || net.is_some() {
            // Stopping flushes queued reliable messages and waits for peers (bounded by the
            // transport's close timeout), and the descriptor server joins its thread, so neither
            // runs on the game thread.
            let spawned = std::thread::Builder::new()
                .name("fatherload-net-stop".to_owned())
                .spawn(move || drop((lan, net)));
            if let Err(error) = spawned {
                eprintln!("Could not stop the network thread in the background: {error}");
            }
        }
    }
}

fn publish_on_lan(descriptor: &HostDescriptor) -> Result<LanPresence, String> {
    let server = LanDescriptorServer::serve(
        SocketAddr::new(std::net::Ipv4Addr::UNSPECIFIED.into(), 0),
        descriptor,
    )
    .map_err(|error| error.to_string())?;
    let descriptor_addr = SocketAddr::new(descriptor.host_addr.ip(), server.local_addr().port());
    let advertisement =
        LanGameAdvertisement::from_descriptor(descriptor).with_descriptor_addr(descriptor_addr);
    let publisher =
        LanDiscoveryPublisher::publish(&advertisement).map_err(|error| error.to_string())?;
    Ok(LanPresence {
        _publisher: publisher,
        _server: server,
    })
}

/// Human-readable disconnect reason.
#[must_use]
pub fn reason_text(reason: &DisconnectReason) -> String {
    match reason {
        DisconnectReason::Local(reason) => format!("closed locally: {reason}"),
        DisconnectReason::Remote(reason) => format!("closed by peer: {reason}"),
        DisconnectReason::TimedOut => "timed out".to_owned(),
        DisconnectReason::Protocol(reason) => format!("protocol error: {reason}"),
        DisconnectReason::Lost(reason) => format!("connection lost: {reason}"),
    }
}
