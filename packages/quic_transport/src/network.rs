//! The network thread: one Tokio runtime, all Quinn endpoints and per-connection tasks.
//!
//! Task layout per connection:
//! - **reader**: reads length-prefixed frames from the control stream until it ends. It is
//!   never wrapped in a timeout, so no partially read frame can be dropped. It is the only
//!   task that reports a connection's end (via [`Internal::ReaderFinished`]), which keeps
//!   `Disconnected` ordered after every reliable `Message`.
//! - **writer**: owns the control stream's send half; writes frames in order (sleeping first
//!   when the link conditioner delays them) and performs the clean-close handshake.
//! - **datagrams**: forwards QUIC datagrams as unreliable messages.
//! - **stats** (optional): periodic `LinkStats`.
//!
//! Control stream wire format: the client opens one bidirectional stream and writes
//! [`STREAM_MAGIC`]. After that both directions carry frames of
//! `[u32 big-endian length][u8 tag][payload]`, where `length = payload.len() + 1`.

use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    sync::{Arc, Mutex, PoisonError, mpsc},
    time::Duration,
};

use bytes::Bytes;
use quinn::{
    ConnectionError, Endpoint, EndpointConfig, ReadError, ReadExactError, RecvStream,
    SendDatagramError, SendStream, TokioRuntime, VarInt, rustls::pki_types::CertificateDer,
};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::JoinHandle,
    time::{Instant, sleep, sleep_until, timeout},
};

use crate::{
    api::{
        Channel, ConnId, Direction, DisconnectReason, HostConfig, JoinTarget, LinkStats,
        NetCommand, NetError, NetErrorKind, NetEvent,
    },
    conditioner::LinkConditioner,
    config::NetConfig,
    tls::{self, HostDescriptor},
};

/// First bytes on every control stream, written by the connecting side.
pub const STREAM_MAGIC: &[u8; 4] = b"DGQ1";

const TAG_RELIABLE: u8 = 0;
const TAG_UNRELIABLE: u8 = 1;
const TAG_GOODBYE: u8 = 2;
const HEADER_BYTES: usize = 5;

const CODE_CLOSE: u32 = 0;
const CODE_GOODBYE_ACK: u32 = 1;
const CODE_PROTOCOL: u32 = 2;

type EventTx = mpsc::Sender<NetEvent>;

fn emit(events: &EventTx, event: NetEvent) {
    // The receiver only disappears when the handle is gone; nothing left to tell.
    let _ignored = events.send(event);
}

fn emit_error(events: &EventTx, kind: NetErrorKind, conn: Option<ConnId>, message: String) {
    emit(
        events,
        NetEvent::Error(NetError {
            kind,
            conn,
            message,
        }),
    );
}

/// Messages from spawned tasks back to the network main loop.
enum Internal {
    Established {
        connection: quinn::Connection,
        send: SendStream,
        recv: RecvStream,
        direction: Direction,
        /// Client endpoints are owned by their connection.
        endpoint: Option<Endpoint>,
    },
    ReaderFinished {
        conn: ConnId,
        reason: DisconnectReason,
    },
}

enum WriterMsg {
    Frame {
        tag: u8,
        bytes: Vec<u8>,
        ready_at: Option<Instant>,
    },
    Goodbye {
        reason: String,
    },
}

/// Per-connection state shared between the main loop and the connection's tasks.
///
/// `closed` is a mutex rather than an atomic so that a task can check it and emit an event
/// atomically: once [`close_entry`] has set it (just before `Disconnected` is emitted), no
/// task can emit a `Message` or `Stats` for that connection any more, even on another
/// worker thread.
#[derive(Default)]
struct ConnShared {
    closed: Mutex<bool>,
}

impl ConnShared {
    fn is_closed(&self) -> bool {
        *self.closed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn close(&self) {
        *self.closed.lock().unwrap_or_else(PoisonError::into_inner) = true;
    }

    /// Emit `event` unless the connection has been closed. Returns `false` once closed.
    #[allow(
        clippy::significant_drop_tightening,
        reason = "the lock must be held across emit so close() cannot interleave"
    )]
    fn emit_if_open(&self, events: &EventTx, event: NetEvent) -> bool {
        let closed = self.closed.lock().unwrap_or_else(PoisonError::into_inner);
        if *closed {
            return false;
        }
        emit(events, event);
        true
    }
}

struct ConnEntry {
    connection: quinn::Connection,
    writer: UnboundedSender<WriterMsg>,
    shared: Arc<ConnShared>,
    local_reason: Option<String>,
    last_reliable_ready: Instant,
    background: Vec<JoinHandle<()>>,
    _endpoint: Option<Endpoint>,
}

pub struct Network {
    config: NetConfig,
    events: EventTx,
    internal_tx: UnboundedSender<Internal>,
    internal_rx: UnboundedReceiver<Internal>,
    next_conn: u64,
    conns: BTreeMap<ConnId, ConnEntry>,
    host_endpoints: Vec<Endpoint>,
    conditioner: Option<LinkConditioner>,
    shutting_down: bool,
}

impl Network {
    pub fn new(config: NetConfig, events: EventTx) -> Self {
        let (internal_tx, internal_rx) = unbounded_channel();
        let conditioner = config
            .link
            .is_active()
            .then(|| LinkConditioner::new(config.link));
        Self {
            config,
            events,
            internal_tx,
            internal_rx,
            next_conn: 1,
            conns: BTreeMap::new(),
            host_endpoints: Vec::new(),
            conditioner,
            shutting_down: false,
        }
    }

    pub async fn run(mut self, mut commands: UnboundedReceiver<NetCommand>) {
        let mut deadline: Option<Instant> = None;
        loop {
            if deadline.is_some() && self.conns.is_empty() {
                break;
            }
            let shutdown_at = deadline.unwrap_or_else(|| Instant::now() + Duration::from_hours(1));
            tokio::select! {
                command = commands.recv(), if deadline.is_none() => match command {
                    Some(NetCommand::Shutdown) | None => deadline = Some(self.begin_shutdown()),
                    Some(command) => self.handle_command(command),
                },
                Some(internal) = self.internal_rx.recv() => self.handle_internal(internal),
                () = sleep_until(shutdown_at), if deadline.is_some() => {
                    self.force_close_all();
                    break;
                }
            }
        }
        for endpoint in &self.host_endpoints {
            endpoint.close(VarInt::from_u32(CODE_CLOSE), b"shutdown");
        }
        for endpoint in &self.host_endpoints {
            let _ignored = timeout(Duration::from_secs(1), endpoint.wait_idle()).await;
        }
        emit(&self.events, NetEvent::ShutdownComplete);
    }

    fn handle_command(&mut self, command: NetCommand) {
        match command {
            NetCommand::Host { bind, config } => self.host(bind, &config),
            NetCommand::Join { target } => {
                tokio::spawn(join_task(
                    target,
                    self.config.clone(),
                    self.internal_tx.clone(),
                    self.events.clone(),
                ));
            }
            NetCommand::Send {
                conn,
                channel,
                bytes,
            } => self.send(conn, channel, bytes),
            NetCommand::Disconnect { conn, reason } => {
                if !self.disconnect(conn, reason) {
                    emit_error(
                        &self.events,
                        NetErrorKind::Disconnect,
                        Some(conn),
                        "unknown connection".to_owned(),
                    );
                }
            }
            NetCommand::Shutdown => {}
        }
    }

    fn host(&mut self, bind: SocketAddr, config: &HostConfig) {
        match self.bind_host(bind, config) {
            Ok((endpoint, addr, descriptor)) => {
                emit(&self.events, NetEvent::HostReady { addr, descriptor });
                tokio::spawn(accept_loop(
                    endpoint.clone(),
                    self.config.handshake_timeout,
                    self.internal_tx.clone(),
                ));
                self.host_endpoints.push(endpoint);
            }
            Err(message) => emit_error(&self.events, NetErrorKind::Host, None, message),
        }
    }

    fn bind_host(
        &self,
        bind: SocketAddr,
        config: &HostConfig,
    ) -> Result<(Endpoint, SocketAddr, HostDescriptor), String> {
        let identity = tls::generate_identity(&config.server_name)?;
        let certificate_der = identity.certificate.as_ref().to_vec();
        let server_config = tls::server_config(identity, &self.config)?;
        let socket = UdpSocket::bind(bind).map_err(|error| format!("bind {bind}: {error}"))?;
        let endpoint = Endpoint::new(
            EndpointConfig::default(),
            Some(server_config),
            socket,
            Arc::new(TokioRuntime),
        )
        .map_err(|error| format!("endpoint: {error}"))?;
        let addr = endpoint
            .local_addr()
            .map_err(|error| format!("local address: {error}"))?;
        let descriptor = HostDescriptor {
            host_addr: connectable_addr(addr),
            server_name: config.server_name.clone(),
            certificate_der,
        };
        Ok((endpoint, addr, descriptor))
    }

    fn send(&mut self, conn: ConnId, channel: Channel, bytes: Vec<u8>) {
        if bytes.len() > self.config.max_frame_bytes {
            emit_error(
                &self.events,
                NetErrorKind::Send,
                Some(conn),
                format!(
                    "payload of {} bytes exceeds max frame size {}",
                    bytes.len(),
                    self.config.max_frame_bytes
                ),
            );
            return;
        }
        let Some(entry) = self.conns.get_mut(&conn) else {
            emit_error(
                &self.events,
                NetErrorKind::Send,
                Some(conn),
                "unknown connection".to_owned(),
            );
            return;
        };
        if entry.local_reason.is_some() {
            emit_error(
                &self.events,
                NetErrorKind::Send,
                Some(conn),
                "connection is disconnecting".to_owned(),
            );
            return;
        }
        match channel {
            Channel::Reliable => {
                let ready_at = self.conditioner.as_mut().map(|conditioner| {
                    let at = (Instant::now() + conditioner.sample_delay())
                        .max(entry.last_reliable_ready);
                    entry.last_reliable_ready = at;
                    at
                });
                let _ignored = entry.writer.send(WriterMsg::Frame {
                    tag: TAG_RELIABLE,
                    bytes,
                    ready_at,
                });
            }
            Channel::Unreliable => {
                let bytes = Bytes::from(bytes);
                let Some(conditioner) = self.conditioner.as_mut() else {
                    send_unreliable_now(&entry.connection, &entry.writer, &bytes);
                    return;
                };
                for delay in conditioner.plan_unreliable() {
                    let connection = entry.connection.clone();
                    let writer = entry.writer.clone();
                    let bytes = bytes.clone();
                    tokio::spawn(async move {
                        sleep(delay).await;
                        send_unreliable_now(&connection, &writer, &bytes);
                    });
                }
            }
        }
    }

    /// Start a clean close. Returns `false` for an unknown connection.
    fn disconnect(&mut self, conn: ConnId, reason: String) -> bool {
        let Some(entry) = self.conns.get_mut(&conn) else {
            return false;
        };
        if entry.local_reason.is_none() {
            entry.local_reason = Some(reason.clone());
            // Bound the whole clean close: a peer that stops reading would otherwise stall
            // the writer on flow control forever (keep-alives prevent the idle timeout).
            let deadline =
                entry.last_reliable_ready.max(Instant::now()) + self.config.close_timeout * 2;
            let connection = entry.connection.clone();
            let watchdog_reason = reason.clone();
            entry.background.push(tokio::spawn(async move {
                sleep_until(deadline).await;
                connection.close(VarInt::from_u32(CODE_CLOSE), watchdog_reason.as_bytes());
            }));
            if entry.writer.send(WriterMsg::Goodbye { reason }).is_err() {
                // Writer already gone (connection failing); close directly.
                entry
                    .connection
                    .close(VarInt::from_u32(CODE_CLOSE), b"disconnect");
            }
        }
        true
    }

    fn begin_shutdown(&mut self) -> Instant {
        self.shutting_down = true;
        for endpoint in &self.host_endpoints {
            endpoint.set_server_config(None);
        }
        let ids: Vec<ConnId> = self.conns.keys().copied().collect();
        for id in ids {
            self.disconnect(id, "shutdown".to_owned());
        }
        Instant::now() + self.config.close_timeout + Duration::from_millis(500)
    }

    fn force_close_all(&mut self) {
        let conns = std::mem::take(&mut self.conns);
        for (conn, entry) in conns {
            let reason = entry
                .local_reason
                .clone()
                .unwrap_or_else(|| "shutdown".to_owned());
            close_entry(&entry);
            emit(
                &self.events,
                NetEvent::Disconnected {
                    conn,
                    reason: DisconnectReason::Local(reason),
                },
            );
        }
    }

    fn handle_internal(&mut self, internal: Internal) {
        match internal {
            Internal::Established {
                connection,
                send,
                recv,
                direction,
                endpoint,
            } => {
                if self.shutting_down {
                    connection.close(VarInt::from_u32(CODE_CLOSE), b"shutdown");
                    return;
                }
                self.register(connection, send, recv, direction, endpoint);
            }
            Internal::ReaderFinished { conn, reason } => {
                if let Some(entry) = self.conns.remove(&conn) {
                    close_entry(&entry);
                    let reason = entry.local_reason.map_or(reason, DisconnectReason::Local);
                    emit(&self.events, NetEvent::Disconnected { conn, reason });
                }
            }
        }
    }

    fn register(
        &mut self,
        connection: quinn::Connection,
        send: SendStream,
        recv: RecvStream,
        direction: Direction,
        endpoint: Option<Endpoint>,
    ) {
        let conn = ConnId(self.next_conn);
        self.next_conn += 1;
        let shared = Arc::new(ConnShared::default());
        let (writer_tx, writer_rx) = unbounded_channel();

        emit(
            &self.events,
            NetEvent::Connected {
                conn,
                remote: connection.remote_address(),
                direction,
            },
        );

        tokio::spawn(writer_task(
            connection.clone(),
            send,
            writer_rx,
            self.config.close_timeout,
        ));
        tokio::spawn(reader_task(
            conn,
            connection.clone(),
            recv,
            self.config.max_frame_bytes,
            self.events.clone(),
            Arc::clone(&shared),
            self.internal_tx.clone(),
        ));
        let mut background = vec![tokio::spawn(datagram_task(
            conn,
            connection.clone(),
            self.events.clone(),
            Arc::clone(&shared),
        ))];
        if let Some(interval) = self.config.stats_interval {
            background.push(tokio::spawn(stats_task(
                conn,
                connection.clone(),
                interval,
                self.events.clone(),
                Arc::clone(&shared),
            )));
        }

        self.conns.insert(
            conn,
            ConnEntry {
                connection,
                writer: writer_tx,
                shared,
                local_reason: None,
                last_reliable_ready: Instant::now(),
                background,
                _endpoint: endpoint,
            },
        );
    }
}

fn close_entry(entry: &ConnEntry) {
    entry.shared.close();
    for task in &entry.background {
        task.abort();
    }
    entry
        .connection
        .close(VarInt::from_u32(CODE_CLOSE), b"closed");
}

/// Replace an unspecified bind address with something a client can actually dial.
fn connectable_addr(addr: SocketAddr) -> SocketAddr {
    if !addr.ip().is_unspecified() {
        return addr;
    }
    let ip = crate::lan::likely_lan_ip().unwrap_or_else(|| match addr.ip() {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
    });
    SocketAddr::new(ip, addr.port())
}

fn send_unreliable_now(
    connection: &quinn::Connection,
    writer: &UnboundedSender<WriterMsg>,
    bytes: &Bytes,
) {
    let fits = connection
        .max_datagram_size()
        .is_some_and(|max| bytes.len() <= max);
    if fits {
        match connection.send_datagram(bytes.clone()) {
            Ok(()) | Err(SendDatagramError::ConnectionLost(_)) => return,
            Err(_) => {}
        }
    }
    let _ignored = writer.send(WriterMsg::Frame {
        tag: TAG_UNRELIABLE,
        bytes: bytes.to_vec(),
        ready_at: None,
    });
}

async fn accept_loop(
    endpoint: Endpoint,
    handshake_timeout: Duration,
    internal_tx: UnboundedSender<Internal>,
) {
    while let Some(incoming) = endpoint.accept().await {
        tokio::spawn(accept_one(incoming, handshake_timeout, internal_tx.clone()));
    }
}

async fn accept_one(
    incoming: quinn::Incoming,
    handshake_timeout: Duration,
    internal_tx: UnboundedSender<Internal>,
) {
    let Ok(connection) = incoming.await else {
        return;
    };
    // Bounded wait for the control stream. Dropping a half-read magic here is harmless:
    // a failed handshake closes the whole connection.
    let handshake = async {
        let (send, mut recv) = connection.accept_bi().await.ok()?;
        let mut magic = [0_u8; STREAM_MAGIC.len()];
        recv.read_exact(&mut magic).await.ok()?;
        (&magic == STREAM_MAGIC).then_some((send, recv))
    };
    match timeout(handshake_timeout, handshake).await {
        Ok(Some((send, recv))) => {
            let _ignored = internal_tx.send(Internal::Established {
                connection,
                send,
                recv,
                direction: Direction::Inbound,
                endpoint: None,
            });
        }
        _ => connection.close(VarInt::from_u32(CODE_PROTOCOL), b"handshake failed"),
    }
}

async fn join_task(
    target: JoinTarget,
    config: NetConfig,
    internal_tx: UnboundedSender<Internal>,
    events: EventTx,
) {
    let (addr, server_name, pinned) = match target {
        JoinTarget::Descriptor(descriptor) => (
            descriptor.host_addr,
            descriptor.server_name,
            Some(CertificateDer::from(descriptor.certificate_der)),
        ),
        JoinTarget::Addr(addr) => (addr, tls::DEFAULT_SERVER_NAME.to_owned(), None),
    };
    match join(addr, &server_name, pinned, &config).await {
        Ok((connection, send, recv, endpoint)) => {
            let _ignored = internal_tx.send(Internal::Established {
                connection,
                send,
                recv,
                direction: Direction::Outbound,
                endpoint: Some(endpoint),
            });
        }
        Err(message) => emit_error(
            &events,
            NetErrorKind::Join,
            None,
            format!("join {addr} failed: {message}"),
        ),
    }
}

async fn join(
    addr: SocketAddr,
    server_name: &str,
    pinned: Option<CertificateDer<'static>>,
    config: &NetConfig,
) -> Result<(quinn::Connection, SendStream, RecvStream, Endpoint), String> {
    let bind = match addr.ip() {
        IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        IpAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    };
    let socket = UdpSocket::bind(bind).map_err(|error| format!("bind: {error}"))?;
    let mut endpoint = Endpoint::new(
        EndpointConfig::default(),
        None,
        socket,
        Arc::new(TokioRuntime),
    )
    .map_err(|error| format!("endpoint: {error}"))?;
    endpoint.set_default_client_config(tls::client_config(pinned, config)?);
    let connection = endpoint
        .connect(addr, server_name)
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    let (mut send, recv) = connection
        .open_bi()
        .await
        .map_err(|error| error.to_string())?;
    send.write_all(STREAM_MAGIC)
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, send, recv, endpoint))
}

async fn write_frame(send: &mut SendStream, tag: u8, payload: &[u8]) -> Result<(), String> {
    let length = u32::try_from(payload.len() + 1).map_err(|_| "frame too large".to_owned())?;
    let mut header = [0_u8; HEADER_BYTES];
    header[..4].copy_from_slice(&length.to_be_bytes());
    header[4] = tag;
    send.write_all(&header)
        .await
        .map_err(|error| error.to_string())?;
    send.write_all(payload)
        .await
        .map_err(|error| error.to_string())
}

async fn writer_task(
    connection: quinn::Connection,
    mut send: SendStream,
    mut queue: UnboundedReceiver<WriterMsg>,
    close_timeout: Duration,
) {
    while let Some(message) = queue.recv().await {
        match message {
            WriterMsg::Frame {
                tag,
                bytes,
                ready_at,
            } => {
                if let Some(ready_at) = ready_at {
                    sleep_until(ready_at).await;
                }
                if write_frame(&mut send, tag, &bytes).await.is_err() {
                    return;
                }
            }
            WriterMsg::Goodbye { reason } => {
                // Everything queued before this has been written. Tell the peer, then wait
                // for it to close after reading all of it (bounded), then close ourselves.
                let _ignored = write_frame(&mut send, TAG_GOODBYE, reason.as_bytes()).await;
                let _ignored = send.finish();
                let _ignored = timeout(close_timeout, connection.closed()).await;
                connection.close(VarInt::from_u32(CODE_CLOSE), reason.as_bytes());
                return;
            }
        }
    }
}

fn reason_from_connection_error(error: &ConnectionError) -> DisconnectReason {
    match error {
        ConnectionError::ApplicationClosed(close) => {
            DisconnectReason::Remote(String::from_utf8_lossy(&close.reason).into_owned())
        }
        ConnectionError::TimedOut => DisconnectReason::TimedOut,
        ConnectionError::LocallyClosed => DisconnectReason::Local("closed".to_owned()),
        other => DisconnectReason::Lost(other.to_string()),
    }
}

fn reason_from_read_error(
    error: ReadExactError,
    connection: &quinn::Connection,
) -> DisconnectReason {
    let reason = match error {
        ReadExactError::ReadError(ReadError::ConnectionLost(error)) => {
            return reason_from_connection_error(&error);
        }
        ReadExactError::FinishedEarly(_) => {
            DisconnectReason::Protocol("control stream ended without goodbye".to_owned())
        }
        ReadExactError::ReadError(ReadError::Reset(code)) => {
            DisconnectReason::Protocol(format!("control stream reset ({code})"))
        }
        ReadExactError::ReadError(other) => DisconnectReason::Lost(other.to_string()),
    };
    connection.close(VarInt::from_u32(CODE_PROTOCOL), b"control stream failed");
    reason
}

async fn reader_task(
    conn: ConnId,
    connection: quinn::Connection,
    mut recv: RecvStream,
    max_frame_bytes: usize,
    events: EventTx,
    shared: Arc<ConnShared>,
    internal_tx: UnboundedSender<Internal>,
) {
    let protocol_violation = |message: String| {
        connection.close(VarInt::from_u32(CODE_PROTOCOL), message.as_bytes());
        DisconnectReason::Protocol(message)
    };
    let reason = loop {
        let mut header = [0_u8; HEADER_BYTES];
        if let Err(error) = recv.read_exact(&mut header).await {
            break reason_from_read_error(error, &connection);
        }
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let Some(payload_len) = usize::try_from(length)
            .ok()
            .and_then(|length| length.checked_sub(1))
            .filter(|len| *len <= max_frame_bytes)
        else {
            break protocol_violation(format!("invalid frame length {length}"));
        };
        let mut payload = vec![0_u8; payload_len];
        if let Err(error) = recv.read_exact(&mut payload).await {
            break reason_from_read_error(error, &connection);
        }
        let channel = match header[4] {
            TAG_RELIABLE => Channel::Reliable,
            TAG_UNRELIABLE => Channel::Unreliable,
            TAG_GOODBYE => {
                connection.close(VarInt::from_u32(CODE_GOODBYE_ACK), b"goodbye");
                break DisconnectReason::Remote(String::from_utf8_lossy(&payload).into_owned());
            }
            tag => break protocol_violation(format!("unknown frame tag {tag}")),
        };
        // Gated so a force-closed connection (shutdown deadline) emits nothing after its
        // `Disconnected`; the normal path ends this loop before the main loop closes it.
        shared.emit_if_open(
            &events,
            NetEvent::Message {
                conn,
                channel,
                bytes: payload,
            },
        );
    };
    let _ignored = internal_tx.send(Internal::ReaderFinished { conn, reason });
}

async fn datagram_task(
    conn: ConnId,
    connection: quinn::Connection,
    events: EventTx,
    shared: Arc<ConnShared>,
) {
    while let Ok(datagram) = connection.read_datagram().await {
        let open = shared.emit_if_open(
            &events,
            NetEvent::Message {
                conn,
                channel: Channel::Unreliable,
                bytes: datagram.to_vec(),
            },
        );
        if !open {
            break;
        }
    }
}

async fn stats_task(
    conn: ConnId,
    connection: quinn::Connection,
    interval: Duration,
    events: EventTx,
    shared: Arc<ConnShared>,
) {
    let mut ticker = tokio::time::interval_at(Instant::now() + interval, interval);
    loop {
        ticker.tick().await;
        if shared.is_closed() {
            break;
        }
        let stats = connection.stats();
        #[allow(
            clippy::cast_precision_loss,
            reason = "packet counters stay far below 2^52"
        )]
        let loss = if stats.path.sent_packets == 0 {
            0.0
        } else {
            stats.path.lost_packets as f64 / stats.path.sent_packets as f64
        };
        let open = shared.emit_if_open(
            &events,
            NetEvent::Stats {
                conn,
                stats: LinkStats {
                    rtt: stats.path.rtt,
                    sent_packets: stats.path.sent_packets,
                    lost_packets: stats.path.lost_packets,
                    loss,
                    bytes_sent: stats.udp_tx.bytes,
                    bytes_received: stats.udp_rx.bytes,
                    max_datagram_size: connection.max_datagram_size(),
                },
            },
        );
        if !open {
            break;
        }
    }
}
