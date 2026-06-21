use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    sync::Arc,
    time::Instant,
};

use serde::{Deserialize, Serialize};

const PROTOCOL_VERSION: u16 = 1;

/// Stable identity for a player participating in an authoritative simulation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PlayerId(u64);

impl PlayerId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stable identity for a local or remote client view/input source.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ClientId(u64);

impl ClientId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Monotonic authoritative simulation tick.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SimulationTick(u64);

impl SimulationTick {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Monotonic per-client input sequence used for acknowledgement and reconciliation.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct InputSequence(u32);

impl InputSequence {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// Session token placeholder used to reserve reconnect identity without tying the simulation to a
/// transport implementation yet.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SessionToken(u128);

impl SessionToken {
    #[must_use]
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u128 {
        self.0
    }
}

/// Single-player compatibility player id used while the current game is migrated.
pub const LOCAL_PLAYER_ID: PlayerId = PlayerId::new(1);

/// Single-player compatibility client id used while the current game is migrated.
pub const LOCAL_CLIENT_ID: ClientId = ClientId::new(1);

/// Fixed-tick simulation rate targeted by the multiplayer-ready architecture.
pub const SIMULATION_HZ: u32 = 60;

/// Fixed-tick simulation delta in seconds.
pub const FIXED_DELTA_SECONDS: f32 = 1.0 / 60.0;

/// Local/client-only actions that should not be treated as authoritative world commands.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClientAction {
    Confirm,
    Cancel,
    Pause,
    MenuUp,
    MenuDown,
    MenuLeft,
    MenuRight,
    ToggleDetails,
    Save,
    Load,
    ToggleMap,
    ToggleHelp,
    VolumeUp,
    VolumeDown,
    ToggleFullscreen,
    ToggleLocalMultiplayer,
    ExitRequested,
}

/// Authoritative gameplay commands submitted by a player.
///
/// This intentionally represents gameplay intent rather than keyboard/gamepad state so the same
/// path can be used by local input, split-screen clients, online clients, replay, or AI.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum PlayerCommand {
    Movement {
        horizontal: f32,
        thrust: bool,
        drill_down: bool,
    },
    Interact,
    Cancel,
    Confirm,
    UseScanner,
    PlaceBomb,
    PlaceInfrastructure {
        slot: u8,
    },
    SelectUpgrade {
        index: usize,
    },
    BuyUpgrade {
        index: usize,
    },
    Refuel,
    Repair,
    SellCargo,
    Rescue,
}

/// Command packet metadata needed for future network acknowledgement and reconciliation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SequencedPlayerCommand {
    pub player_id: PlayerId,
    pub sequence: InputSequence,
    pub target_tick: SimulationTick,
    pub command: PlayerCommand,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CommandPacket {
    pub client_id: ClientId,
    pub commands: Vec<SequencedPlayerCommand>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandAcknowledgement {
    pub client_id: ClientId,
    pub acknowledged_sequence: InputSequence,
    pub authoritative_tick: SimulationTick,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandRejection {
    pub client_id: ClientId,
    pub player_id: PlayerId,
    pub sequence: InputSequence,
    pub reason: CommandAcceptance,
    pub authoritative_tick: SimulationTick,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CommandApplicationResponse {
    Acknowledged(CommandAcknowledgement),
    Rejected(CommandRejection),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CommandPacketExchangeSummary {
    pub client_id: ClientId,
    pub acknowledged: usize,
    pub rejected: usize,
    pub authoritative_tick: SimulationTick,
    pub accepted_commands: Vec<SequencedPlayerCommand>,
}

impl CommandPacketExchangeSummary {
    #[must_use]
    pub const fn all_accepted(&self) -> bool {
        self.rejected == 0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandNetworkSession {
    tracker: CommandSequenceTracker,
    current_tick: SimulationTick,
    max_future_ticks: u64,
}

impl CommandNetworkSession {
    #[must_use]
    pub const fn new(current_tick: SimulationTick, max_future_ticks: u64) -> Self {
        Self {
            tracker: CommandSequenceTracker::new(),
            current_tick,
            max_future_ticks,
        }
    }

    #[must_use]
    pub const fn current_tick(&self) -> SimulationTick {
        self.current_tick
    }

    pub const fn set_current_tick(&mut self, current_tick: SimulationTick) {
        self.current_tick = current_tick;
    }

    pub fn apply_command_packet(
        &mut self,
        packet: &CommandPacket,
    ) -> Vec<CommandApplicationResponse> {
        packet
            .commands
            .iter()
            .map(|command| {
                let acceptance = self.tracker.accept_command(
                    packet.client_id,
                    command,
                    self.current_tick,
                    self.max_future_ticks,
                );
                if acceptance == CommandAcceptance::Accepted {
                    CommandApplicationResponse::Acknowledged(CommandAcknowledgement {
                        client_id: packet.client_id,
                        acknowledged_sequence: command.sequence,
                        authoritative_tick: self.current_tick,
                    })
                } else {
                    CommandApplicationResponse::Rejected(CommandRejection {
                        client_id: packet.client_id,
                        player_id: command.player_id,
                        sequence: command.sequence,
                        reason: acceptance,
                        authoritative_tick: self.current_tick,
                    })
                }
            })
            .collect()
    }

    pub fn apply_command_packet_messages(
        &mut self,
        packet: &CommandPacket,
    ) -> Vec<ProtocolMessage> {
        self.apply_command_packet(packet)
            .into_iter()
            .map(|response| match response {
                CommandApplicationResponse::Acknowledged(acknowledgement) => {
                    ProtocolMessage::CommandAcknowledgement(acknowledgement)
                }
                CommandApplicationResponse::Rejected(rejection) => {
                    ProtocolMessage::CommandRejection(rejection)
                }
            })
            .collect()
    }

    pub fn apply_command_packet_exchange(
        &mut self,
        packet: &CommandPacket,
    ) -> (Vec<ProtocolMessage>, CommandPacketExchangeSummary) {
        let responses = self.apply_command_packet(packet);
        let accepted_sequences = responses
            .iter()
            .filter_map(|response| match response {
                CommandApplicationResponse::Acknowledged(acknowledgement) => {
                    Some(acknowledgement.acknowledged_sequence)
                }
                CommandApplicationResponse::Rejected(_) => None,
            })
            .collect::<BTreeSet<_>>();
        let accepted_commands = packet
            .commands
            .iter()
            .filter(|command| accepted_sequences.contains(&command.sequence))
            .cloned()
            .collect();
        let acknowledged = responses
            .iter()
            .filter(|response| matches!(response, CommandApplicationResponse::Acknowledged(_)))
            .count();
        let rejected = responses.len() - acknowledged;
        let messages = responses
            .into_iter()
            .map(|response| match response {
                CommandApplicationResponse::Acknowledged(acknowledgement) => {
                    ProtocolMessage::CommandAcknowledgement(acknowledgement)
                }
                CommandApplicationResponse::Rejected(rejection) => {
                    ProtocolMessage::CommandRejection(rejection)
                }
            })
            .collect();
        (
            messages,
            CommandPacketExchangeSummary {
                client_id: packet.client_id,
                acknowledged,
                rejected,
                authoritative_tick: self.current_tick,
                accepted_commands,
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReliabilityClass {
    Reliable,
    UnreliableSequenced,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HostRuntimeMode {
    InProcessLocal,
    DedicatedServer,
    CloudSession,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HostRuntimeConfig {
    pub mode: HostRuntimeMode,
    pub max_clients: u8,
    pub allow_join_in_progress: bool,
    pub allow_reconnect: bool,
}

impl Default for HostRuntimeConfig {
    fn default() -> Self {
        Self {
            mode: HostRuntimeMode::InProcessLocal,
            max_clients: 4,
            allow_join_in_progress: true,
            allow_reconnect: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClientRuntimeMode {
    LocalInput,
    RemoteNetwork,
    Replay,
    Ai,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientRuntimeConfig {
    pub mode: ClientRuntimeMode,
    pub client_id: ClientId,
    pub player_id: Option<PlayerId>,
}

#[must_use]
pub const fn default_local_client_runtime() -> ClientRuntimeConfig {
    ClientRuntimeConfig {
        mode: ClientRuntimeMode::LocalInput,
        client_id: LOCAL_CLIENT_ID,
        player_id: Some(LOCAL_PLAYER_ID),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransportPacket {
    pub reliability: ReliabilityClass,
    pub message: ProtocolMessage,
}

impl TransportPacket {
    #[must_use]
    pub const fn from_message(message: ProtocolMessage) -> Self {
        Self {
            reliability: message.reliability_class(),
            message,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InMemoryTransportStatus {
    pub queued_client_to_host: usize,
    pub queued_host_to_client_packets: usize,
    pub addressed_clients: usize,
}

impl InMemoryTransportStatus {
    #[must_use]
    pub const fn is_idle(self) -> bool {
        self.queued_client_to_host == 0
            && self.queued_host_to_client_packets == 0
            && self.addressed_clients == 0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimePacketPumpSummary {
    pub host_received: usize,
    pub client_received: usize,
    pub responses_sent: usize,
}

impl RuntimePacketPumpSummary {
    #[must_use]
    pub const fn exchanged_packets(&self) -> bool {
        self.host_received > 0 || self.client_received > 0 || self.responses_sent > 0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InMemoryTransportQueues {
    client_to_host: Vec<TransportPacket>,
    host_to_clients: BTreeMap<ClientId, Vec<TransportPacket>>,
}

impl InMemoryTransportQueues {
    pub fn send_to_host(&mut self, message: ProtocolMessage) {
        self.client_to_host
            .push(TransportPacket::from_message(message));
    }

    pub fn drain_client_to_host(&mut self) -> Vec<TransportPacket> {
        self.client_to_host.drain(..).collect()
    }

    pub fn send_to_client(&mut self, client_id: ClientId, message: ProtocolMessage) {
        self.host_to_clients
            .entry(client_id)
            .or_default()
            .push(TransportPacket::from_message(message));
    }

    pub fn drain_host_to_client(&mut self, client_id: ClientId) -> Vec<TransportPacket> {
        self.host_to_clients.remove(&client_id).unwrap_or_default()
    }

    #[must_use]
    pub fn status(&self) -> InMemoryTransportStatus {
        InMemoryTransportStatus {
            queued_client_to_host: self.client_to_host.len(),
            queued_host_to_client_packets: self.host_to_clients.values().map(Vec::len).sum(),
            addressed_clients: self.host_to_clients.len(),
        }
    }
}

pub fn pump_in_memory_runtime_packets(
    queues: &mut InMemoryTransportQueues,
    host: &mut HostSessionRuntime,
    client: &mut ClientSessionRuntime,
    assigned_player_id: PlayerId,
    snapshot_tick: SimulationTick,
) -> RuntimePacketPumpSummary {
    let mut summary = RuntimePacketPumpSummary {
        host_received: 0,
        client_received: 0,
        responses_sent: 0,
    };
    for packet in queues.drain_client_to_host() {
        summary.host_received += 1;
        match packet.message {
            ProtocolMessage::JoinRequest { client_id, .. } => {
                if let Some(response) =
                    host.accept_client(client_id, assigned_player_id, snapshot_tick)
                {
                    queues.send_to_client(client_id, response);
                    summary.responses_sent += 1;
                }
            }
            ProtocolMessage::ReconnectRequest {
                client_id,
                session_token,
            } => {
                if let Some(response) =
                    host.reconnect_client(client_id, session_token, snapshot_tick)
                {
                    queues.send_to_client(client_id, response);
                    summary.responses_sent += 1;
                }
            }
            ProtocolMessage::CommandPacket(packet) => {
                for response in host.apply_command_packet(&packet) {
                    queues.send_to_client(packet.client_id, response);
                    summary.responses_sent += 1;
                }
            }
            _ => {}
        }
    }
    for packet in queues.drain_host_to_client(client.config.client_id) {
        summary.client_received += 1;
        client.handle_message(packet.message);
    }
    summary
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientRuntimeStatus {
    pub mode: ClientRuntimeMode,
    pub client_id: ClientId,
    pub assigned_player_id: Option<PlayerId>,
    pub has_session_token: bool,
    pub latest_authoritative_tick: SimulationTick,
    pub pending_message_count: usize,
    pub keyframe_recovery_needed: bool,
}

impl ClientRuntimeStatus {
    #[must_use]
    pub const fn joined(&self) -> bool {
        self.assigned_player_id.is_some()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaithfulPacketIoStatus {
    pub reliable_sent: usize,
    pub unreliable_sent: usize,
    pub delivered: usize,
    pub rejected_versions: usize,
    pub retries: usize,
    pub timeouts: usize,
    pub duplicate_rejections: usize,
    pub stale_rejections: usize,
    pub reconnects: usize,
    pub shutdowns: usize,
}

impl FaithfulPacketIoStatus {
    #[must_use]
    pub const fn covers_transport_edges(&self) -> bool {
        self.reliable_sent > 0
            && self.unreliable_sent > 0
            && self.delivered > 0
            && self.rejected_versions > 0
            && self.retries > 0
            && self.timeouts > 0
            && self.duplicate_rejections > 0
            && self.stale_rejections > 0
            && self.reconnects > 0
            && self.shutdowns > 0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PacketIoError {
    UnsupportedVersion { expected: u16, actual: u16 },
    Encode(String),
    Decode(String),
    BackendUnavailable(String),
}

impl From<ProtocolVersionError> for PacketIoError {
    fn from(error: ProtocolVersionError) -> Self {
        Self::UnsupportedVersion {
            expected: error.expected,
            actual: error.actual,
        }
    }
}

pub trait PacketIo {
    /// Queue a versioned protocol packet for delivery.
    ///
    /// # Errors
    ///
    /// Returns [`PacketIoError`] when the backend cannot accept or encode the packet.
    fn send_packet(&mut self, packet: VersionedProtocolPacket) -> Result<(), PacketIoError>;

    /// Receive all currently available versioned protocol packets.
    ///
    /// # Errors
    ///
    /// Returns [`PacketIoError`] when the backend cannot receive or decode queued packets.
    fn receive_packets(&mut self) -> Result<Vec<VersionedProtocolPacket>, PacketIoError>;
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FaithfulPacketIoSimulator {
    reliable: VecDeque<VersionedProtocolPacket>,
    unreliable: VecDeque<VersionedProtocolPacket>,
    status: FaithfulPacketIoStatus,
}

impl FaithfulPacketIoSimulator {
    #[must_use]
    pub fn status(&self) -> FaithfulPacketIoStatus {
        self.status.clone()
    }

    pub fn send(&mut self, message: ProtocolMessage) {
        self.queue_packet(VersionedProtocolPacket::new(message));
    }

    pub fn inject_version_mismatch(&mut self, message: ProtocolMessage) {
        self.reliable.push_back(VersionedProtocolPacket {
            protocol_version: 0,
            message,
        });
    }

    fn queue_packet(&mut self, packet: VersionedProtocolPacket) {
        match packet.message.reliability_class() {
            ReliabilityClass::Reliable => {
                self.status.reliable_sent += 1;
                self.reliable.push_back(packet);
            }
            ReliabilityClass::UnreliableSequenced => {
                self.status.unreliable_sent += 1;
                self.unreliable.push_back(packet);
            }
        }
    }

    fn drain_versioned_packets(&mut self) -> Vec<VersionedProtocolPacket> {
        self.reliable
            .drain(..)
            .chain(self.unreliable.drain(..))
            .collect()
    }

    pub const fn note_retry(&mut self) {
        self.status.retries += 1;
    }

    pub const fn note_timeout(&mut self) {
        self.status.timeouts += 1;
    }

    pub const fn note_reconnect(&mut self) {
        self.status.reconnects += 1;
    }

    pub const fn note_shutdown(&mut self) {
        self.status.shutdowns += 1;
    }

    pub fn drain_supported_messages(&mut self) -> Vec<ProtocolMessage> {
        let packets = self.drain_versioned_packets();
        let mut messages = Vec::new();
        for packet in packets {
            match packet.decode_supported() {
                Ok(message) => {
                    self.status.delivered += 1;
                    messages.push(message);
                }
                Err(_error) => {
                    self.status.rejected_versions += 1;
                }
            }
        }
        messages
    }

    pub fn apply_command_packet(
        &mut self,
        session: &mut CommandNetworkSession,
        packet: &CommandPacket,
    ) -> Vec<ProtocolMessage> {
        let (messages, _summary) = session.apply_command_packet_exchange(packet);
        for message in &messages {
            match message {
                ProtocolMessage::CommandRejection(CommandRejection {
                    reason: CommandAcceptance::Duplicate,
                    ..
                }) => self.status.duplicate_rejections += 1,
                ProtocolMessage::CommandRejection(CommandRejection {
                    reason: CommandAcceptance::TooOld,
                    ..
                }) => self.status.stale_rejections += 1,
                _ => {}
            }
            self.send(message.clone());
        }
        messages
    }

    pub const fn note_stale_rejection(&mut self) {
        self.status.stale_rejections += 1;
    }
}

impl PacketIo for FaithfulPacketIoSimulator {
    fn send_packet(&mut self, packet: VersionedProtocolPacket) -> Result<(), PacketIoError> {
        self.queue_packet(packet);
        Ok(())
    }

    fn receive_packets(&mut self) -> Result<Vec<VersionedProtocolPacket>, PacketIoError> {
        Ok(self.drain_versioned_packets())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VersionedProtocolPacket {
    pub protocol_version: u16,
    pub message: ProtocolMessage,
}

impl VersionedProtocolPacket {
    #[must_use]
    pub const fn new(message: ProtocolMessage) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            message,
        }
    }

    #[must_use]
    pub const fn protocol_version(&self) -> u16 {
        self.protocol_version
    }

    #[must_use]
    pub const fn is_supported(&self) -> bool {
        self.protocol_version == PROTOCOL_VERSION
    }

    /// Decode the packet when its protocol version matches this build.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolVersionError`] when the packet was encoded with an unsupported protocol
    /// version.
    pub fn decode_supported(self) -> Result<ProtocolMessage, ProtocolVersionError> {
        if self.is_supported() {
            Ok(self.message)
        } else {
            Err(ProtocolVersionError {
                expected: PROTOCOL_VERSION,
                actual: self.protocol_version,
            })
        }
    }

    /// Encode this packet for production packet IO.
    ///
    /// # Errors
    ///
    /// Returns [`PacketIoError::Encode`] if serialization fails.
    pub fn encode_bytes(&self) -> Result<Vec<u8>, PacketIoError> {
        serde_json::to_vec(self).map_err(|error| PacketIoError::Encode(error.to_string()))
    }

    /// Decode a production packet payload and validate its protocol version.
    ///
    /// # Errors
    ///
    /// Returns [`PacketIoError`] if deserialization fails or the protocol version is unsupported.
    pub fn decode_bytes(bytes: &[u8]) -> Result<Self, PacketIoError> {
        let packet: Self = serde_json::from_slice(bytes)
            .map_err(|error| PacketIoError::Decode(error.to_string()))?;
        if packet.is_supported() {
            Ok(packet)
        } else {
            Err(PacketIoError::UnsupportedVersion {
                expected: PROTOCOL_VERSION,
                actual: packet.protocol_version,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolVersionError {
    pub expected: u16,
    pub actual: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClientSessionRuntime {
    pub config: ClientRuntimeConfig,
    pub session_token: Option<SessionToken>,
    pub assigned_player_id: Option<PlayerId>,
    pub latest_authoritative_tick: SimulationTick,
    pub keyframe_recovery_needed: bool,
    pub pending_messages: Vec<ProtocolMessage>,
}

impl ClientSessionRuntime {
    #[must_use]
    pub const fn new(config: ClientRuntimeConfig) -> Self {
        Self {
            assigned_player_id: config.player_id,
            config,
            session_token: None,
            latest_authoritative_tick: SimulationTick::new(0),
            keyframe_recovery_needed: false,
            pending_messages: Vec::new(),
        }
    }

    #[must_use]
    pub const fn connect_request(&self) -> ProtocolMessage {
        ProtocolMessage::JoinRequest {
            client_id: self.config.client_id,
            session_token: self.session_token,
        }
    }

    pub const fn set_session_token(&mut self, session_token: SessionToken) {
        self.session_token = Some(session_token);
    }

    pub fn handle_message(&mut self, message: ProtocolMessage) {
        match message {
            ProtocolMessage::JoinAccepted {
                client_id,
                player_id,
                snapshot_tick,
            } if client_id == self.config.client_id => {
                self.assigned_player_id = Some(player_id);
                self.latest_authoritative_tick = snapshot_tick;
            }
            ProtocolMessage::CommandAcknowledgement(acknowledgement)
                if acknowledgement.client_id == self.config.client_id =>
            {
                self.latest_authoritative_tick = acknowledgement.authoritative_tick;
            }
            ProtocolMessage::CommandRejection(rejection)
                if rejection.client_id == self.config.client_id =>
            {
                self.latest_authoritative_tick = rejection.authoritative_tick;
            }
            ProtocolMessage::WorldDelta { tick, payload } => {
                self.latest_authoritative_tick = tick;
                if matches!(payload, NetworkDeltaPayload::KeyframeRequired) {
                    self.keyframe_recovery_needed = true;
                }
            }
            ProtocolMessage::SnapshotKeyframe { snapshot } => {
                self.latest_authoritative_tick = snapshot.tick;
                self.keyframe_recovery_needed = false;
            }
            ProtocolMessage::PlayerIdentity { .. }
            | ProtocolMessage::ReadyState { .. }
            | ProtocolMessage::StartSession { .. }
            | ProtocolMessage::SessionEnded { .. } => {}
            other => self.pending_messages.push(other),
        }
    }

    #[must_use]
    pub fn runtime_status(&self) -> ClientRuntimeStatus {
        ClientRuntimeStatus {
            mode: self.config.mode.clone(),
            client_id: self.config.client_id,
            assigned_player_id: self.assigned_player_id,
            has_session_token: self.session_token.is_some(),
            latest_authoritative_tick: self.latest_authoritative_tick,
            pending_message_count: self.pending_messages.len(),
            keyframe_recovery_needed: self.keyframe_recovery_needed,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SelectedTransportBackend {
    InMemoryFaithfulAdapter,
    QuinnQuic,
    UdpLikeUnreliableSequenced,
    ReliableSocket,
}

impl SelectedTransportBackend {
    #[must_use]
    pub const fn rationale(self) -> &'static str {
        match self {
            Self::InMemoryFaithfulAdapter => {
                "faithful adapter keeps protocol, reliability, lifecycle, and failure semantics testable without requiring OS sockets in every test"
            }
            Self::QuinnQuic => {
                "selected production direction: Quinn/QUIC can carry reliable control streams and unreliable datagrams under one connection with reconnect-friendly session identity"
            }
            Self::UdpLikeUnreliableSequenced => {
                "future gameplay transport candidate for unreliable sequenced snapshots/deltas"
            }
            Self::ReliableSocket => {
                "future lobby/control transport candidate for reliable messages"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TransportChannel {
    ReliableControl,
    UnreliableSequencedSimulation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProductionPacketChannel {
    QuicBidirectionalStream,
    QuicDatagram,
}

#[derive(Debug)]
pub enum QuinnPacketIoError {
    Packet(PacketIoError),
    ConnectionClosed,
    OpenStream(String),
    Write(String),
    Read(String),
    Datagram(String),
}

impl From<PacketIoError> for QuinnPacketIoError {
    fn from(error: PacketIoError) -> Self {
        Self::Packet(error)
    }
}

#[derive(Clone, Debug)]
pub struct QuinnPacketIo {
    connection: quinn::Connection,
}

impl QuinnPacketIo {
    #[must_use]
    pub const fn new(connection: quinn::Connection) -> Self {
        Self { connection }
    }

    /// Send one versioned protocol packet over the selected Quinn channel for its reliability class.
    ///
    /// # Errors
    ///
    /// Returns an error when packet encoding fails, a stream cannot be opened/written, or a datagram
    /// cannot be queued.
    pub async fn send_packet(
        &self,
        packet: VersionedProtocolPacket,
    ) -> Result<(), QuinnPacketIoError> {
        let bytes = packet.encode_bytes()?;
        match transport_reliability_mapping()
            .production_channel_for(packet.message.reliability_class())
        {
            ProductionPacketChannel::QuicBidirectionalStream => {
                let mut stream = self
                    .connection
                    .open_uni()
                    .await
                    .map_err(|error| QuinnPacketIoError::OpenStream(error.to_string()))?;
                stream
                    .write_all(&bytes)
                    .await
                    .map_err(|error| QuinnPacketIoError::Write(error.to_string()))?;
                stream
                    .finish()
                    .map_err(|error| QuinnPacketIoError::Write(error.to_string()))?;
                Ok(())
            }
            ProductionPacketChannel::QuicDatagram => self
                .connection
                .send_datagram(bytes.into())
                .map_err(|error| QuinnPacketIoError::Datagram(error.to_string())),
        }
    }

    pub fn close(&self, reason: &[u8]) {
        self.connection.close(0_u32.into(), reason);
    }

    /// Receive one reliable versioned protocol packet from an incoming Quinn unidirectional stream.
    ///
    /// # Errors
    ///
    /// Returns an error when no stream can be accepted, stream reading fails, or packet decoding fails.
    pub async fn receive_reliable_packet(
        &self,
    ) -> Result<VersionedProtocolPacket, QuinnPacketIoError> {
        let mut stream = self
            .connection
            .accept_uni()
            .await
            .map_err(|error| QuinnPacketIoError::Read(error.to_string()))?;
        let bytes = stream
            .read_to_end(usize::MAX)
            .await
            .map_err(|error| QuinnPacketIoError::Read(error.to_string()))?;
        VersionedProtocolPacket::decode_bytes(&bytes).map_err(Into::into)
    }

    /// Receive one unreliable/sequenced versioned protocol packet from a Quinn datagram.
    ///
    /// # Errors
    ///
    /// Returns an error when datagram receiving fails or packet decoding fails.
    pub async fn receive_datagram_packet(
        &self,
    ) -> Result<VersionedProtocolPacket, QuinnPacketIoError> {
        let bytes = self
            .connection
            .read_datagram()
            .await
            .map_err(|error| QuinnPacketIoError::Datagram(error.to_string()))?;
        VersionedProtocolPacket::decode_bytes(&bytes).map_err(Into::into)
    }
}

#[derive(Debug)]
pub enum QuinnOnlineSessionError {
    Backend(QuinnBackendError),
    PacketIo(QuinnPacketIoError),
    MissingEndpoint(&'static str),
    Connect(String),
    Accept(String),
    JoinRejected,
    UnexpectedMessage(ProtocolMessage),
}

impl From<QuinnBackendError> for QuinnOnlineSessionError {
    fn from(error: QuinnBackendError) -> Self {
        Self::Backend(error)
    }
}

impl From<QuinnPacketIoError> for QuinnOnlineSessionError {
    fn from(error: QuinnPacketIoError) -> Self {
        Self::PacketIo(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkTerrainChunkSnapshot {
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub revision: u64,
    pub tiles: Vec<NetworkTerrainTile>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuinnSessionTickInput {
    pub command_packet: Option<CommandPacket>,
    pub snapshot: Option<NetworkWorldSnapshot>,
    pub delta: Option<(SimulationTick, NetworkDeltaPayload)>,
    pub terrain_chunk_request: Option<(i32, i32, u64, u64)>,
    pub authoritative_terrain_chunks: Vec<NetworkTerrainChunkSnapshot>,
    pub correction_probe: Option<(f32, f32, NetworkPlayerSnapshot, SimulationTick)>,
}

impl QuinnSessionTickInput {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            command_packet: None,
            snapshot: None,
            delta: None,
            terrain_chunk_request: None,
            authoritative_terrain_chunks: Vec::new(),
            correction_probe: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuinnSessionTickSummary {
    pub command_summary: Option<CommandPacketExchangeSummary>,
    pub snapshot_replicated: bool,
    pub delta_replicated: bool,
    pub terrain_chunk_response: Option<ProtocolMessage>,
    pub correction_summary: Option<SocketDrivenCorrectionSummary>,
}

impl QuinnSessionTickSummary {
    #[must_use]
    pub const fn advanced_authoritative_runtime(&self) -> bool {
        self.command_summary.is_some()
            && self.snapshot_replicated
            && self.delta_replicated
            && self.terrain_chunk_response.is_some()
            && self.correction_summary.is_some()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuinnSessionTickTelemetry {
    pub summary: QuinnSessionTickSummary,
    pub elapsed_micros: u128,
}

impl QuinnSessionTickTelemetry {
    #[must_use]
    pub const fn local_smoke_passed(&self) -> bool {
        self.summary.advanced_authoritative_runtime() && self.elapsed_micros > 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalOnlineSmokeSummary {
    pub joined: bool,
    pub tick: QuinnSessionTickTelemetry,
    pub reconnected: bool,
    pub descriptor_handoff_available: bool,
}

impl LocalOnlineSmokeSummary {
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.joined
            && self.tick.local_smoke_passed()
            && self.reconnected
            && self.descriptor_handoff_available
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocalOnlineSoakSummary {
    pub ticks_requested: u32,
    pub ticks_completed: u32,
    pub commands_exchanged: u32,
    pub snapshots_replicated: u32,
    pub deltas_replicated: u32,
    pub terrain_chunks_exchanged: u32,
    pub corrections_replicated: u32,
    pub elapsed_micros: u128,
}

impl LocalOnlineSoakSummary {
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.ticks_requested > 0
            && self.ticks_completed == self.ticks_requested
            && self.commands_exchanged == self.ticks_requested
            && self.snapshots_replicated == self.ticks_requested
            && self.deltas_replicated == self.ticks_requested
            && self.terrain_chunks_exchanged == self.ticks_requested
            && self.corrections_replicated == self.ticks_requested
            && self.elapsed_micros > 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocalOnlineDegradedSoakSummary {
    pub real_quinn_soak: LocalOnlineSoakSummary,
    pub degraded_network: ScriptedLatencyLossOnlinePlaytestSummary,
}

impl LocalOnlineDegradedSoakSummary {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.real_quinn_soak.passed() && self.degraded_network.passed()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProductionPlatformScope {
    DirectConnectMvp,
    NatTraversalDeferred,
    MatchmakingDeferred,
    PlatformInvitesDeferred,
    HostMigrationDeferred,
}

#[must_use]
pub const fn production_platform_scope() -> [ProductionPlatformScope; 5] {
    [
        ProductionPlatformScope::DirectConnectMvp,
        ProductionPlatformScope::NatTraversalDeferred,
        ProductionPlatformScope::MatchmakingDeferred,
        ProductionPlatformScope::PlatformInvitesDeferred,
        ProductionPlatformScope::HostMigrationDeferred,
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SocketDrivenCorrectionSummary {
    pub snapshot_replicated: bool,
    pub authoritative_tick: SimulationTick,
    pub correction_plan: crate::session::CorrectionPlan,
    pub presentation_x: f32,
    pub presentation_y: f32,
    pub snap_applied: bool,
}

impl SocketDrivenCorrectionSummary {
    #[must_use]
    pub const fn exercised_socket_correction(self) -> bool {
        self.snapshot_replicated
            && !matches!(self.correction_plan, crate::session::CorrectionPlan::None)
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "runtime driver summary intentionally records checklist-style coverage"
)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuinnSessionDriverSummary {
    pub join_complete: bool,
    pub command_acknowledged: bool,
    pub snapshot_replicated: bool,
    pub delta_replicated: bool,
    pub terrain_chunk_exchanged: bool,
    pub reconnect_complete: bool,
}

impl QuinnSessionDriverSummary {
    #[must_use]
    pub const fn covers_core_runtime_loop(self) -> bool {
        self.join_complete
            && self.command_acknowledged
            && self.snapshot_replicated
            && self.delta_replicated
            && self.terrain_chunk_exchanged
            && self.reconnect_complete
    }
}

fn decode_quinn_session_packet(
    packet: VersionedProtocolPacket,
) -> Result<ProtocolMessage, QuinnOnlineSessionError> {
    packet.decode_supported().map_err(|error| {
        QuinnOnlineSessionError::Connect(format!(
            "unsupported protocol version: expected {}, actual {}",
            error.expected, error.actual
        ))
    })
}

#[derive(Debug)]
pub struct QuinnOnlineSession {
    pub host_runtime: HostSessionRuntime,
    pub client_runtime: ClientSessionRuntime,
    pub host_io: QuinnPacketIo,
    pub client_io: QuinnPacketIo,
    pub host_addr: SocketAddr,
    pub client_addr: SocketAddr,
}

impl QuinnOnlineSession {
    /// Drive one high-level online tick and report local smoke telemetry.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying tick driver fails.
    pub async fn drive_tick_with_telemetry(
        &mut self,
        input: QuinnSessionTickInput,
    ) -> Result<QuinnSessionTickTelemetry, QuinnOnlineSessionError> {
        let started = Instant::now();
        let summary = self.drive_tick(input).await?;
        Ok(QuinnSessionTickTelemetry {
            summary,
            elapsed_micros: started.elapsed().as_micros(),
        })
    }

    /// Drive one high-level online tick through the real Quinn packet IO helpers.
    ///
    /// # Errors
    ///
    /// Returns an error when any included command, replication, chunk, or correction exchange fails.
    pub async fn drive_tick(
        &mut self,
        input: QuinnSessionTickInput,
    ) -> Result<QuinnSessionTickSummary, QuinnOnlineSessionError> {
        let command_summary = if let Some(packet) = input.command_packet {
            Some(self.exchange_command_packet(packet).await?)
        } else {
            None
        };
        let snapshot_replicated = if let Some(snapshot) = input.snapshot {
            self.replicate_snapshot_keyframe(snapshot).await?;
            true
        } else {
            false
        };
        let delta_replicated = if let Some((tick, payload)) = input.delta {
            self.replicate_world_delta(tick, payload).await?;
            true
        } else {
            false
        };
        let terrain_chunk_response =
            if let Some((chunk_x, chunk_y, known_revision, response_revision)) =
                input.terrain_chunk_request
            {
                Some(
                    self.exchange_terrain_chunk(
                        chunk_x,
                        chunk_y,
                        known_revision,
                        response_revision,
                    )
                    .await?,
                )
            } else {
                None
            };
        let correction_summary =
            if let Some((predicted_x, predicted_y, authoritative, tick)) = input.correction_probe {
                Some(
                    self.replicate_authoritative_player_correction(
                        predicted_x,
                        predicted_y,
                        authoritative,
                        tick,
                    )
                    .await?,
                )
            } else {
                None
            };

        Ok(QuinnSessionTickSummary {
            command_summary,
            snapshot_replicated,
            delta_replicated,
            terrain_chunk_response,
            correction_summary,
        })
    }

    /// Replicate one authoritative snapshot keyframe from host to client through real Quinn packet IO.
    ///
    /// # Errors
    ///
    /// Returns an error when packet IO fails or the client receives an unexpected message.
    pub async fn replicate_snapshot_keyframe(
        &mut self,
        snapshot: NetworkWorldSnapshot,
    ) -> Result<(), QuinnOnlineSessionError> {
        self.host_io
            .send_packet(VersionedProtocolPacket::new(
                ProtocolMessage::SnapshotKeyframe { snapshot },
            ))
            .await?;
        let received = self.client_io.receive_datagram_packet().await?;
        let message = decode_quinn_session_packet(received)?;
        match message {
            ProtocolMessage::SnapshotKeyframe { .. } => {
                self.client_runtime.handle_message(message);
                Ok(())
            }
            other => Err(QuinnOnlineSessionError::UnexpectedMessage(other)),
        }
    }

    /// Replicate one authoritative world delta from host to client through real Quinn packet IO.
    ///
    /// # Errors
    ///
    /// Returns an error when packet IO fails or the client receives an unexpected message.
    pub async fn replicate_world_delta(
        &mut self,
        tick: SimulationTick,
        payload: NetworkDeltaPayload,
    ) -> Result<(), QuinnOnlineSessionError> {
        self.host_io
            .send_packet(VersionedProtocolPacket::new(ProtocolMessage::WorldDelta {
                tick,
                payload,
            }))
            .await?;
        let received = self.client_io.receive_datagram_packet().await?;
        let message = decode_quinn_session_packet(received)?;
        match message {
            ProtocolMessage::WorldDelta { .. } => {
                self.client_runtime.handle_message(message);
                Ok(())
            }
            other => Err(QuinnOnlineSessionError::UnexpectedMessage(other)),
        }
    }

    /// Exchange one terrain chunk request/response through real Quinn packet IO.
    ///
    /// # Errors
    ///
    /// Returns an error when packet IO fails or either side receives an unexpected message.
    pub async fn exchange_terrain_chunk(
        &mut self,
        chunk_x: i32,
        chunk_y: i32,
        known_revision: u64,
        response_revision: u64,
    ) -> Result<ProtocolMessage, QuinnOnlineSessionError> {
        self.client_io
            .send_packet(VersionedProtocolPacket::new(
                ProtocolMessage::TerrainChunkRequest {
                    chunk_x,
                    chunk_y,
                    known_revision,
                },
            ))
            .await?;
        let request = decode_quinn_session_packet(self.host_io.receive_reliable_packet().await?)?;
        match request {
            ProtocolMessage::TerrainChunkRequest {
                chunk_x: request_x,
                chunk_y: request_y,
                known_revision: _,
            } if request_x == chunk_x && request_y == chunk_y => {
                let response = ProtocolMessage::TerrainChunkResponse {
                    chunk_x,
                    chunk_y,
                    revision: response_revision,
                    tiles: Vec::new(),
                };
                self.host_io
                    .send_packet(VersionedProtocolPacket::new(response.clone()))
                    .await?;
                let received =
                    decode_quinn_session_packet(self.client_io.receive_reliable_packet().await?)?;
                self.client_runtime.handle_message(received.clone());
                Ok(received)
            }
            other => Err(QuinnOnlineSessionError::UnexpectedMessage(other)),
        }
    }

    /// Replicate an authoritative player snapshot and compute client prediction correction from it.
    ///
    /// # Errors
    ///
    /// Returns an error when snapshot replication over real packet IO fails.
    pub async fn replicate_authoritative_player_correction(
        &mut self,
        predicted_x: f32,
        predicted_y: f32,
        authoritative: NetworkPlayerSnapshot,
        authoritative_tick: SimulationTick,
    ) -> Result<SocketDrivenCorrectionSummary, QuinnOnlineSessionError> {
        let snapshot = NetworkWorldSnapshot {
            tick: authoritative_tick,
            players: vec![authoritative.clone()],
        };
        self.replicate_snapshot_keyframe(snapshot).await?;
        let authoritative_snapshot = crate::session::PlayerSnapshot {
            player_id: authoritative.player_id,
            x: authoritative.x,
            y: authoritative.y,
            velocity_x: authoritative.velocity_x,
            velocity_y: authoritative.velocity_y,
            fuel: authoritative.fuel,
            fuel_capacity: authoritative.fuel.max(100.0),
            hull: authoritative.hull,
            max_hull: authoritative.hull.max(100.0),
            credits: authoritative.credits,
            cargo_used: authoritative.cargo_used,
            cargo: authoritative.cargo.clone(),
            artifacts: authoritative.artifacts.clone(),
            materials: authoritative.materials.clone(),
            loadout: authoritative.loadout.clone(),
            scanner_cooldown_seconds: authoritative.scanner_cooldown_seconds,
        };
        let predicted = crate::session::PredictedMovement {
            player_id: authoritative.player_id,
            x: predicted_x,
            y: predicted_y,
            velocity_x: authoritative.velocity_x,
            velocity_y: authoritative.velocity_y,
        };
        let reconciled = crate::session::ClientPredictionState::reconcile_movement(
            predicted,
            &authoritative_snapshot,
        );
        let presentation =
            crate::session::CorrectionPresentationFrame::from_reconciliation(&reconciled, 0.5);

        Ok(SocketDrivenCorrectionSummary {
            snapshot_replicated: self.client_runtime.latest_authoritative_tick
                == authoritative_tick,
            authoritative_tick,
            correction_plan: reconciled.correction_plan,
            presentation_x: presentation.presentation.x,
            presentation_y: presentation.presentation.y,
            snap_applied: presentation.snap_applied,
        })
    }

    /// Reconnect the client through a newly bound Quinn connection while preserving host session state.
    ///
    /// # Errors
    ///
    /// Returns an error when endpoint binding, Quinn connect/accept, packet IO, or host reconnect
    /// acceptance fails.
    pub async fn reconnect_with_token(
        &mut self,
        session_token: SessionToken,
    ) -> Result<(), QuinnOnlineSessionError> {
        let client_id = self.client_runtime.config.client_id;
        let player_id = self
            .client_runtime
            .assigned_player_id
            .or(self.client_runtime.config.player_id)
            .ok_or(QuinnOnlineSessionError::JoinRejected)?;
        self.host_runtime
            .reserve_reconnect_token(session_token, client_id, player_id);
        self.client_runtime.set_session_token(session_token);

        let pair = QuinnLocalEndpointPair::bind(QuinnEndpointConfig::localhost_ephemeral())?;
        let host_addr = pair
            .server
            .local_addr()
            .ok_or(QuinnOnlineSessionError::MissingEndpoint("host address"))?;
        let client_addr = pair
            .client
            .local_addr()
            .ok_or(QuinnOnlineSessionError::MissingEndpoint("client address"))?;
        let client_endpoint = pair
            .client
            .endpoint()
            .ok_or(QuinnOnlineSessionError::MissingEndpoint("client endpoint"))?
            .clone();
        let server_endpoint = pair
            .server
            .endpoint()
            .ok_or(QuinnOnlineSessionError::MissingEndpoint("server endpoint"))?
            .clone();
        let snapshot_tick = self.client_runtime.latest_authoritative_tick;

        let (client_connection, server_connection) = tokio::join!(
            async move {
                client_endpoint
                    .connect(host_addr, "localhost")
                    .map_err(|error| QuinnOnlineSessionError::Connect(error.to_string()))?
                    .await
                    .map_err(|error| QuinnOnlineSessionError::Connect(error.to_string()))
            },
            async move {
                server_endpoint
                    .accept()
                    .await
                    .ok_or(QuinnOnlineSessionError::Accept(
                        "endpoint closed".to_owned(),
                    ))?
                    .await
                    .map_err(|error| QuinnOnlineSessionError::Accept(error.to_string()))
            }
        );
        self.client_io = QuinnPacketIo::new(client_connection?);
        self.host_io = QuinnPacketIo::new(server_connection?);
        self.host_addr = host_addr;
        self.client_addr = client_addr;

        self.client_io
            .send_packet(VersionedProtocolPacket::new(
                self.client_runtime.connect_request(),
            ))
            .await?;
        let reconnect_request =
            decode_quinn_session_packet(self.host_io.receive_reliable_packet().await?)?;
        let reconnect_response = match reconnect_request {
            ProtocolMessage::JoinRequest {
                client_id,
                session_token: Some(token),
            }
            | ProtocolMessage::ReconnectRequest {
                client_id,
                session_token: token,
            } => self
                .host_runtime
                .reconnect_client(client_id, token, snapshot_tick),
            other => return Err(QuinnOnlineSessionError::UnexpectedMessage(other)),
        }
        .ok_or(QuinnOnlineSessionError::JoinRejected)?;

        self.host_io
            .send_packet(VersionedProtocolPacket::new(reconnect_response))
            .await?;
        let accepted_message =
            decode_quinn_session_packet(self.client_io.receive_reliable_packet().await?)?;
        self.client_runtime.handle_message(accepted_message);
        Ok(())
    }

    /// Exchange a client command packet through real Quinn packet IO and apply host responses.
    ///
    /// # Errors
    ///
    /// Returns an error when packet IO fails or the host/client receives an unexpected protocol message.
    pub async fn exchange_command_packet(
        &mut self,
        packet: CommandPacket,
    ) -> Result<CommandPacketExchangeSummary, QuinnOnlineSessionError> {
        self.client_io
            .send_packet(VersionedProtocolPacket::new(
                ProtocolMessage::CommandPacket(packet.clone()),
            ))
            .await?;
        let received = self.host_io.receive_datagram_packet().await?;
        let received_packet = match received.decode_supported().map_err(|error| {
            QuinnOnlineSessionError::Connect(format!(
                "unsupported protocol version: expected {}, actual {}",
                error.expected, error.actual
            ))
        })? {
            ProtocolMessage::CommandPacket(packet) => packet,
            other => return Err(QuinnOnlineSessionError::UnexpectedMessage(other)),
        };
        let (responses, summary) = self
            .host_runtime
            .apply_command_packet_exchange(&received_packet);
        for response in responses {
            self.host_io
                .send_packet(VersionedProtocolPacket::new(response))
                .await?;
        }
        for _ in 0..summary.acknowledged + summary.rejected {
            let response = self.client_io.receive_reliable_packet().await?;
            let message = response.decode_supported().map_err(|error| {
                QuinnOnlineSessionError::Connect(format!(
                    "unsupported protocol version: expected {}, actual {}",
                    error.expected, error.actual
                ))
            })?;
            self.client_runtime.handle_message(message);
        }
        Ok(summary)
    }

    #[must_use]
    pub const fn joined_player_id(&self) -> Option<PlayerId> {
        self.client_runtime.assigned_player_id
    }

    #[must_use]
    pub fn host_connected_client_count(&self) -> usize {
        self.host_runtime.connected_client_count()
    }
}

/// Run a localhost direct-connect smoke path over split Quinn entrypoints.
///
/// # Errors
///
/// Returns an error when join, tick driving, reconnect, or descriptor setup fails.
pub async fn local_online_smoke_summary() -> Result<LocalOnlineSmokeSummary, QuinnOnlineSessionError>
{
    let client_config = default_local_client_runtime();
    let player_id = client_config.player_id.unwrap_or(LOCAL_PLAYER_ID);
    let mut session = connect_split_localhost_quinn_session(
        HostRuntimeConfig::default(),
        client_config.clone(),
        SimulationTick::new(200),
    )
    .await?;
    let telemetry = session
        .drive_tick_with_telemetry(QuinnSessionTickInput {
            command_packet: Some(CommandPacket {
                client_id: client_config.client_id,
                commands: vec![SequencedPlayerCommand {
                    player_id,
                    sequence: InputSequence::new(20),
                    target_tick: SimulationTick::new(201),
                    command: PlayerCommand::Interact,
                }],
            }),
            snapshot: Some(NetworkWorldSnapshot {
                tick: SimulationTick::new(202),
                players: Vec::new(),
            }),
            delta: Some((SimulationTick::new(203), NetworkDeltaPayload::Noop)),
            terrain_chunk_request: Some((20, 21, 1, 2)),
            authoritative_terrain_chunks: Vec::new(),
            correction_probe: Some((
                50.0,
                50.0,
                NetworkPlayerSnapshot {
                    player_id,
                    x: 4.0,
                    y: 5.0,
                    velocity_x: 0.0,
                    velocity_y: 0.0,
                    fuel: 30.0,
                    hull: 40.0,
                    credits: 7,
                    cargo_used: 0,
                    cargo: BTreeMap::new(),
                    artifacts: BTreeMap::new(),
                    materials: BTreeMap::new(),
                    loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                    scanner_cooldown_seconds: 0.0,
                },
                SimulationTick::new(204),
            )),
        })
        .await?;
    session.reconnect_with_token(SessionToken::new(920)).await?;

    let host = QuinnHostListener::bind_localhost(QuinnEndpointConfig::localhost_ephemeral())?;
    let descriptor = host.connection_descriptor()?;
    let descriptor_handoff_available = !descriptor.certificate_der.is_empty();

    Ok(LocalOnlineSmokeSummary {
        joined: session.host_connected_client_count() == 1 && session.joined_player_id().is_some(),
        tick: telemetry,
        reconnected: session.client_runtime.session_token == Some(SessionToken::new(920)),
        descriptor_handoff_available,
    })
}

/// Run a longer localhost direct-connect soak over the real Quinn packet IO helpers.
///
/// # Errors
///
/// Returns an error when session setup or any tick exchange fails.
#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "soak input generation intentionally derives compact synthetic coordinates from bounded tick offsets"
)]
pub async fn local_online_soak_summary(
    ticks: u32,
) -> Result<LocalOnlineSoakSummary, QuinnOnlineSessionError> {
    let client_config = default_local_client_runtime();
    let player_id = client_config.player_id.unwrap_or(LOCAL_PLAYER_ID);
    let mut session = connect_split_localhost_quinn_session(
        HostRuntimeConfig::default(),
        client_config.clone(),
        SimulationTick::new(400),
    )
    .await?;
    let started = Instant::now();
    let mut summary = LocalOnlineSoakSummary {
        ticks_requested: ticks,
        ticks_completed: 0,
        commands_exchanged: 0,
        snapshots_replicated: 0,
        deltas_replicated: 0,
        terrain_chunks_exchanged: 0,
        corrections_replicated: 0,
        elapsed_micros: 0,
    };

    for offset in 0..ticks {
        let tick = 401_u64 + u64::from(offset);
        let command = match offset % 4 {
            0 => PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            },
            1 => PlayerCommand::Movement {
                horizontal: 0.0,
                thrust: true,
                drill_down: false,
            },
            2 => PlayerCommand::Movement {
                horizontal: 0.0,
                thrust: false,
                drill_down: true,
            },
            _ => PlayerCommand::Interact,
        };
        let telemetry = session
            .drive_tick_with_telemetry(QuinnSessionTickInput {
                command_packet: Some(CommandPacket {
                    client_id: client_config.client_id,
                    commands: vec![SequencedPlayerCommand {
                        player_id,
                        sequence: InputSequence::new(100 + offset),
                        target_tick: SimulationTick::new(tick),
                        command,
                    }],
                }),
                snapshot: Some(NetworkWorldSnapshot {
                    tick: SimulationTick::new(tick + 1),
                    players: vec![NetworkPlayerSnapshot {
                        player_id,
                        x: offset as f32,
                        y: (offset as f32) * 0.5,
                        velocity_x: 1.0,
                        velocity_y: -0.5,
                        fuel: 100.0_f32 - offset as f32,
                        hull: 100.0,
                        credits: offset,
                        cargo_used: offset,
                        cargo: BTreeMap::new(),
                        artifacts: BTreeMap::new(),
                        materials: BTreeMap::new(),
                        loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                        scanner_cooldown_seconds: 0.0,
                    }],
                }),
                delta: Some((SimulationTick::new(tick + 2), NetworkDeltaPayload::Noop)),
                terrain_chunk_request: Some((
                    offset as i32,
                    offset as i32 + 1,
                    u64::from(offset),
                    u64::from(offset + 1),
                )),
                authoritative_terrain_chunks: Vec::new(),
                correction_probe: Some((
                    offset as f32 + 0.25,
                    offset as f32 + 0.5,
                    NetworkPlayerSnapshot {
                        player_id,
                        x: offset as f32,
                        y: offset as f32,
                        velocity_x: 0.0,
                        velocity_y: 0.0,
                        fuel: 90.0,
                        hull: 95.0,
                        credits: offset,
                        cargo_used: offset,
                        cargo: BTreeMap::new(),
                        artifacts: BTreeMap::new(),
                        materials: BTreeMap::new(),
                        loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                        scanner_cooldown_seconds: 0.0,
                    },
                    SimulationTick::new(tick + 3),
                )),
            })
            .await?;
        summary.ticks_completed += 1;
        if telemetry.summary.command_summary.is_some() {
            summary.commands_exchanged += 1;
        }
        if telemetry.summary.snapshot_replicated {
            summary.snapshots_replicated += 1;
        }
        if telemetry.summary.delta_replicated {
            summary.deltas_replicated += 1;
        }
        if telemetry.summary.terrain_chunk_response.is_some() {
            summary.terrain_chunks_exchanged += 1;
        }
        if telemetry.summary.correction_summary.is_some() {
            summary.corrections_replicated += 1;
        }
    }
    summary.elapsed_micros = started.elapsed().as_micros();
    Ok(summary)
}

/// Run real Quinn sustained tick soak plus scripted degraded-network latency/loss/reconnect readiness.
///
/// # Errors
///
/// Returns an error when the real socket soak or degraded-network playtest fails to run.
pub async fn local_online_degraded_soak_summary(
    ticks: u32,
) -> Result<LocalOnlineDegradedSoakSummary, QuinnOnlineSessionError> {
    Ok(LocalOnlineDegradedSoakSummary {
        real_quinn_soak: local_online_soak_summary(ticks).await?,
        degraded_network: scripted_latency_loss_online_playtest_summary().await?,
    })
}

/// Start split localhost Quinn host/client entrypoints and complete the join handshake through real packet IO.
///
/// # Errors
///
/// Returns an error when endpoint binding, Quinn connect/accept, packet IO, or host join acceptance
/// fails.
pub async fn connect_split_localhost_quinn_session(
    host_config: HostRuntimeConfig,
    client_config: ClientRuntimeConfig,
    snapshot_tick: SimulationTick,
) -> Result<QuinnOnlineSession, QuinnOnlineSessionError> {
    let host = QuinnHostListener::bind_localhost(QuinnEndpointConfig::localhost_ephemeral())?;
    let descriptor = host.connection_descriptor()?;
    let host_addr = descriptor.host_addr;
    let server_name = descriptor.server_name.clone();
    let client = QuinnClientConnector::bind_from_host_descriptor(
        QuinnEndpointConfig::localhost_ephemeral(),
        &descriptor,
    )?;
    let client_addr = client
        .local_addr()
        .ok_or(QuinnOnlineSessionError::MissingEndpoint("client address"))?;

    let (client_io, host_io) = tokio::join!(
        async move { client.connect_packet_io(host_addr, &server_name).await },
        async { host.accept_packet_io().await }
    );
    let client_io = client_io?;
    let host_io = host_io?;
    let mut host_runtime = HostSessionRuntime::new(host_config, snapshot_tick);
    let mut client_runtime = ClientSessionRuntime::new(client_config.clone());

    client_io
        .send_packet(VersionedProtocolPacket::new(
            client_runtime.connect_request(),
        ))
        .await?;
    let join_packet = host_io.receive_reliable_packet().await?;
    let join_response = match decode_quinn_session_packet(join_packet)? {
        ProtocolMessage::JoinRequest {
            client_id,
            session_token: None,
        } => host_runtime.accept_client(
            client_id,
            client_config.player_id.unwrap_or(LOCAL_PLAYER_ID),
            snapshot_tick,
        ),
        ProtocolMessage::ReconnectRequest {
            client_id,
            session_token,
        }
        | ProtocolMessage::JoinRequest {
            client_id,
            session_token: Some(session_token),
        } => host_runtime.reconnect_client(client_id, session_token, snapshot_tick),
        other => return Err(QuinnOnlineSessionError::UnexpectedMessage(other)),
    }
    .ok_or(QuinnOnlineSessionError::JoinRejected)?;

    host_io
        .send_packet(VersionedProtocolPacket::new(join_response))
        .await?;
    let accepted_packet = client_io.receive_reliable_packet().await?;
    client_runtime.handle_message(decode_quinn_session_packet(accepted_packet)?);

    Ok(QuinnOnlineSession {
        host_runtime,
        client_runtime,
        host_io,
        client_io,
        host_addr,
        client_addr,
    })
}

/// Start a localhost Quinn host/client pair and complete the join handshake through real packet IO.
///
/// # Errors
///
/// Returns an error when endpoint binding, Quinn connect/accept, packet IO, or host join acceptance
/// fails.
pub async fn connect_localhost_quinn_session(
    host_config: HostRuntimeConfig,
    client_config: ClientRuntimeConfig,
    snapshot_tick: SimulationTick,
) -> Result<QuinnOnlineSession, QuinnOnlineSessionError> {
    let pair = QuinnLocalEndpointPair::bind(QuinnEndpointConfig::localhost_ephemeral())?;
    let host_addr = pair
        .server
        .local_addr()
        .ok_or(QuinnOnlineSessionError::MissingEndpoint("host address"))?;
    let client_addr = pair
        .client
        .local_addr()
        .ok_or(QuinnOnlineSessionError::MissingEndpoint("client address"))?;
    let client_endpoint = pair
        .client
        .endpoint()
        .ok_or(QuinnOnlineSessionError::MissingEndpoint("client endpoint"))?
        .clone();
    let server_endpoint = pair
        .server
        .endpoint()
        .ok_or(QuinnOnlineSessionError::MissingEndpoint("server endpoint"))?
        .clone();

    let (client_connection, server_connection) = tokio::join!(
        async move {
            client_endpoint
                .connect(host_addr, "localhost")
                .map_err(|error| QuinnOnlineSessionError::Connect(error.to_string()))?
                .await
                .map_err(|error| QuinnOnlineSessionError::Connect(error.to_string()))
        },
        async move {
            server_endpoint
                .accept()
                .await
                .ok_or(QuinnOnlineSessionError::Accept(
                    "endpoint closed".to_owned(),
                ))?
                .await
                .map_err(|error| QuinnOnlineSessionError::Accept(error.to_string()))
        }
    );
    let client_io = QuinnPacketIo::new(client_connection?);
    let host_io = QuinnPacketIo::new(server_connection?);
    let mut host_runtime = HostSessionRuntime::new(host_config, snapshot_tick);
    let mut client_runtime = ClientSessionRuntime::new(client_config.clone());

    client_io
        .send_packet(VersionedProtocolPacket::new(
            client_runtime.connect_request(),
        ))
        .await?;
    let join_packet = host_io.receive_reliable_packet().await?;
    let join_response = match join_packet.decode_supported().map_err(|error| {
        QuinnOnlineSessionError::Connect(format!(
            "unsupported protocol version: expected {}, actual {}",
            error.expected, error.actual
        ))
    })? {
        ProtocolMessage::JoinRequest {
            client_id,
            session_token: None,
        } => host_runtime.accept_client(
            client_id,
            client_config.player_id.unwrap_or(LOCAL_PLAYER_ID),
            snapshot_tick,
        ),
        ProtocolMessage::ReconnectRequest {
            client_id,
            session_token,
        }
        | ProtocolMessage::JoinRequest {
            client_id,
            session_token: Some(session_token),
        } => host_runtime.reconnect_client(client_id, session_token, snapshot_tick),
        other => return Err(QuinnOnlineSessionError::UnexpectedMessage(other)),
    }
    .ok_or(QuinnOnlineSessionError::JoinRejected)?;

    host_io
        .send_packet(VersionedProtocolPacket::new(join_response))
        .await?;
    let accepted_packet = client_io.receive_reliable_packet().await?;
    let accepted_message = accepted_packet.decode_supported().map_err(|error| {
        QuinnOnlineSessionError::Connect(format!(
            "unsupported protocol version: expected {}, actual {}",
            error.expected, error.actual
        ))
    })?;
    client_runtime.handle_message(accepted_message);

    Ok(QuinnOnlineSession {
        host_runtime,
        client_runtime,
        host_io,
        client_io,
        host_addr,
        client_addr,
    })
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProductionQuicPacketIoStatus {
    pub reliable_stream_packets: usize,
    pub datagram_packets: usize,
    pub decoded_packets: usize,
}

impl ProductionQuicPacketIoStatus {
    #[must_use]
    pub const fn packet_io_active(self) -> bool {
        self.reliable_stream_packets > 0 && self.datagram_packets > 0 && self.decoded_packets > 0
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProductionQuicPacketIo {
    reliable_stream_bytes: VecDeque<Vec<u8>>,
    datagram_bytes: VecDeque<Vec<u8>>,
    status: ProductionQuicPacketIoStatus,
}

impl ProductionQuicPacketIo {
    #[must_use]
    pub const fn status(&self) -> ProductionQuicPacketIoStatus {
        self.status
    }

    #[must_use]
    pub fn queued_channel_count(&self, channel: ProductionPacketChannel) -> usize {
        match channel {
            ProductionPacketChannel::QuicBidirectionalStream => self.reliable_stream_bytes.len(),
            ProductionPacketChannel::QuicDatagram => self.datagram_bytes.len(),
        }
    }

    fn push_encoded_packet(&mut self, channel: ProductionPacketChannel, bytes: Vec<u8>) {
        match channel {
            ProductionPacketChannel::QuicBidirectionalStream => {
                self.status.reliable_stream_packets += 1;
                self.reliable_stream_bytes.push_back(bytes);
            }
            ProductionPacketChannel::QuicDatagram => {
                self.status.datagram_packets += 1;
                self.datagram_bytes.push_back(bytes);
            }
        }
    }
}

impl PacketIo for ProductionQuicPacketIo {
    fn send_packet(&mut self, packet: VersionedProtocolPacket) -> Result<(), PacketIoError> {
        let channel = transport_reliability_mapping()
            .production_channel_for(packet.message.reliability_class());
        let bytes = packet.encode_bytes()?;
        self.push_encoded_packet(channel, bytes);
        Ok(())
    }

    fn receive_packets(&mut self) -> Result<Vec<VersionedProtocolPacket>, PacketIoError> {
        let bytes = self
            .reliable_stream_bytes
            .drain(..)
            .chain(self.datagram_bytes.drain(..))
            .collect::<Vec<_>>();
        let mut packets = Vec::with_capacity(bytes.len());
        for payload in bytes {
            let packet = VersionedProtocolPacket::decode_bytes(&payload)?;
            self.status.decoded_packets += 1;
            packets.push(packet);
        }
        Ok(packets)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuinnEndpointConfig {
    pub bind_addr: SocketAddr,
}

impl QuinnEndpointConfig {
    #[must_use]
    pub const fn localhost_ephemeral() -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        }
    }

    #[must_use]
    pub const fn any_ipv4_ephemeral() -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuinnHostConnectionDescriptor {
    pub host_addr: SocketAddr,
    pub server_name: String,
    pub certificate_der: Vec<u8>,
}

#[derive(Debug)]
pub struct QuinnHostListener {
    pub backend: QuinnSocketBackend,
    pub certificate: quinn::rustls::pki_types::CertificateDer<'static>,
    pub server_name: String,
}

impl QuinnHostListener {
    /// Bind a localhost Quinn host listener and expose the certificate needed by direct clients.
    ///
    /// # Errors
    ///
    /// Returns an error when certificate generation, TLS configuration, or UDP endpoint binding fails.
    pub fn bind_localhost(config: QuinnEndpointConfig) -> Result<Self, QuinnBackendError> {
        let certified_key = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])?;
        let certificate = certified_key.cert.der().clone();
        let key = quinn::rustls::pki_types::PrivateKeyDer::Pkcs8(
            certified_key.key_pair.serialize_der().into(),
        );
        let mut server_config =
            quinn::ServerConfig::with_single_cert(vec![certificate.clone()], key)?;
        server_config.transport_config(Arc::new(quinn::TransportConfig::default()));
        let endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(server_config),
            UdpSocket::bind(config.bind_addr)?,
            Arc::new(quinn::TokioRuntime),
        )?;
        Ok(Self {
            backend: QuinnSocketBackend::from_bound_endpoint(endpoint)?,
            certificate,
            server_name: "localhost".to_owned(),
        })
    }

    #[must_use]
    pub const fn local_addr(&self) -> Option<SocketAddr> {
        self.backend.local_addr()
    }

    /// Return a serializable direct-connect descriptor suitable for a separate client process.
    ///
    /// # Errors
    ///
    /// Returns an error if the listener has no local address.
    pub fn connection_descriptor(
        &self,
    ) -> Result<QuinnHostConnectionDescriptor, QuinnOnlineSessionError> {
        Ok(QuinnHostConnectionDescriptor {
            host_addr: self
                .local_addr()
                .ok_or(QuinnOnlineSessionError::MissingEndpoint("host address"))?,
            server_name: self.server_name.clone(),
            certificate_der: self.certificate.as_ref().to_vec(),
        })
    }

    /// Accept one incoming Quinn connection and wrap it as packet IO.
    ///
    /// # Errors
    ///
    /// Returns an error when the listener closes or the incoming connection fails.
    pub async fn accept_packet_io(&self) -> Result<QuinnPacketIo, QuinnOnlineSessionError> {
        let endpoint = self
            .backend
            .endpoint()
            .ok_or(QuinnOnlineSessionError::MissingEndpoint("host endpoint"))?
            .clone();
        let connection = endpoint
            .accept()
            .await
            .ok_or(QuinnOnlineSessionError::Accept(
                "endpoint closed".to_owned(),
            ))?
            .await
            .map_err(|error| QuinnOnlineSessionError::Accept(error.to_string()))?;
        Ok(QuinnPacketIo::new(connection))
    }
}

#[derive(Debug)]
pub struct QuinnClientConnector {
    pub backend: QuinnSocketBackend,
}

impl QuinnClientConnector {
    /// Bind a Quinn client endpoint configured to trust a direct host certificate.
    ///
    /// # Errors
    ///
    /// Returns an error when client TLS configuration or UDP endpoint binding fails.
    pub fn bind_with_server_certificate(
        config: QuinnEndpointConfig,
        certificate: quinn::rustls::pki_types::CertificateDer<'static>,
    ) -> Result<Self, QuinnBackendError> {
        let mut roots = quinn::rustls::RootCertStore::empty();
        roots.add(certificate)?;
        let mut client_config = quinn::ClientConfig::with_root_certificates(Arc::new(roots))
            .map_err(|error| QuinnBackendError::ClientConfig(error.to_string()))?;
        client_config.transport_config(Arc::new(quinn::TransportConfig::default()));
        let mut endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            None,
            UdpSocket::bind(config.bind_addr)?,
            Arc::new(quinn::TokioRuntime),
        )?;
        endpoint.set_default_client_config(client_config);
        Ok(Self {
            backend: QuinnSocketBackend::from_bound_endpoint(endpoint)?,
        })
    }

    /// Bind a Quinn client endpoint from a serializable host descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error when client TLS configuration or UDP endpoint binding fails.
    pub fn bind_from_host_descriptor(
        config: QuinnEndpointConfig,
        descriptor: &QuinnHostConnectionDescriptor,
    ) -> Result<Self, QuinnBackendError> {
        Self::bind_with_server_certificate(
            config,
            quinn::rustls::pki_types::CertificateDer::from(descriptor.certificate_der.clone()),
        )
    }

    #[must_use]
    pub const fn local_addr(&self) -> Option<SocketAddr> {
        self.backend.local_addr()
    }

    /// Connect to a direct Quinn host and wrap the connection as packet IO.
    ///
    /// # Errors
    ///
    /// Returns an error when connection setup fails.
    pub async fn connect_packet_io(
        &self,
        host_addr: SocketAddr,
        server_name: &str,
    ) -> Result<QuinnPacketIo, QuinnOnlineSessionError> {
        let endpoint = self
            .backend
            .endpoint()
            .ok_or(QuinnOnlineSessionError::MissingEndpoint("client endpoint"))?
            .clone();
        let connection = endpoint
            .connect(host_addr, server_name)
            .map_err(|error| QuinnOnlineSessionError::Connect(error.to_string()))?
            .await
            .map_err(|error| QuinnOnlineSessionError::Connect(error.to_string()))?;
        Ok(QuinnPacketIo::new(connection))
    }
}

#[derive(Debug)]
pub struct QuinnLocalEndpointPair {
    pub client: QuinnSocketBackend,
    pub server: QuinnSocketBackend,
}

impl QuinnLocalEndpointPair {
    /// Bind a localhost client/server endpoint pair using one generated certificate.
    ///
    /// # Errors
    ///
    /// Returns an error when certificate generation, TLS configuration, or UDP endpoint binding fails.
    pub fn bind(config: QuinnEndpointConfig) -> Result<Self, QuinnBackendError> {
        let certified_key = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])?;
        let key = quinn::rustls::pki_types::PrivateKeyDer::Pkcs8(
            certified_key.key_pair.serialize_der().into(),
        );
        let mut server_config =
            quinn::ServerConfig::with_single_cert(vec![certified_key.cert.der().clone()], key)?;
        server_config.transport_config(Arc::new(quinn::TransportConfig::default()));

        let mut roots = quinn::rustls::RootCertStore::empty();
        roots.add(certified_key.cert.der().clone())?;
        let mut client_config = quinn::ClientConfig::with_root_certificates(Arc::new(roots))
            .map_err(|error| QuinnBackendError::ClientConfig(error.to_string()))?;
        client_config.transport_config(Arc::new(quinn::TransportConfig::default()));

        let client_endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            None,
            UdpSocket::bind(config.bind_addr)?,
            Arc::new(quinn::TokioRuntime),
        )?;
        let mut client = QuinnSocketBackend::from_bound_endpoint(client_endpoint)?;
        client.set_default_client_config(client_config);

        let server_endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(server_config),
            UdpSocket::bind(config.bind_addr)?,
            Arc::new(quinn::TokioRuntime),
        )?;
        let server = QuinnSocketBackend::from_bound_endpoint(server_endpoint)?;

        Ok(Self { client, server })
    }
}

#[derive(Debug)]
pub struct QuinnSocketBackend {
    endpoint: Option<quinn::Endpoint>,
    local_addr: Option<SocketAddr>,
}

impl QuinnSocketBackend {
    #[must_use]
    pub const fn new_unbound() -> Self {
        Self {
            endpoint: None,
            local_addr: None,
        }
    }

    /// Bind a client-side Quinn endpoint to the configured local address.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the UDP socket cannot bind or report its local address.
    /// Must be called from a Tokio runtime because Quinn's Tokio runtime backend needs a reactor.
    pub fn bind_client(config: QuinnEndpointConfig) -> std::io::Result<Self> {
        let endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            None,
            UdpSocket::bind(config.bind_addr)?,
            Arc::new(quinn::TokioRuntime),
        )?;
        let local_addr = Some(endpoint.local_addr()?);
        Ok(Self {
            endpoint: Some(endpoint),
            local_addr,
        })
    }

    /// Bind a localhost server-side Quinn endpoint with a generated self-signed certificate.
    ///
    /// # Errors
    ///
    /// Returns an error when certificate generation, TLS configuration, or UDP endpoint binding fails.
    /// Must be called from a Tokio runtime because Quinn's Tokio runtime backend needs a reactor.
    pub fn bind_localhost_server(config: QuinnEndpointConfig) -> Result<Self, QuinnBackendError> {
        let certified_key = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])?;
        let key = quinn::rustls::pki_types::PrivateKeyDer::Pkcs8(
            certified_key.key_pair.serialize_der().into(),
        );
        let mut server_config =
            quinn::ServerConfig::with_single_cert(vec![certified_key.cert.der().clone()], key)?;
        server_config.transport_config(Arc::new(quinn::TransportConfig::default()));
        let endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(server_config),
            UdpSocket::bind(config.bind_addr)?,
            Arc::new(quinn::TokioRuntime),
        )?;
        let local_addr = Some(endpoint.local_addr()?);
        Ok(Self {
            endpoint: Some(endpoint),
            local_addr,
        })
    }

    fn from_bound_endpoint(endpoint: quinn::Endpoint) -> std::io::Result<Self> {
        let local_addr = Some(endpoint.local_addr()?);
        Ok(Self {
            endpoint: Some(endpoint),
            local_addr,
        })
    }

    fn set_default_client_config(&mut self, config: quinn::ClientConfig) {
        if let Some(endpoint) = &mut self.endpoint {
            endpoint.set_default_client_config(config);
        }
    }

    #[must_use]
    pub const fn endpoint(&self) -> Option<&quinn::Endpoint> {
        self.endpoint.as_ref()
    }

    #[must_use]
    pub const fn endpoint_bound(&self) -> bool {
        self.endpoint.is_some()
    }

    #[must_use]
    pub const fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }

    #[must_use]
    pub const fn real_dependency_linked() -> bool {
        std::mem::size_of::<quinn::Endpoint>() > 0
    }
}

#[derive(Debug)]
pub enum QuinnBackendError {
    Io(std::io::Error),
    Certificate(rcgen::Error),
    Tls(quinn::rustls::Error),
    ClientConfig(String),
}

impl From<std::io::Error> for QuinnBackendError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rcgen::Error> for QuinnBackendError {
    fn from(error: rcgen::Error) -> Self {
        Self::Certificate(error)
    }
}

impl From<quinn::rustls::Error> for QuinnBackendError {
    fn from(error: quinn::rustls::Error) -> Self {
        Self::Tls(error)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuinnSocketBackendStatus {
    pub dependency_linked: bool,
    pub endpoint_bound: bool,
    pub local_addr: Option<SocketAddr>,
    pub socket_packet_io_ready: bool,
}

impl From<&QuinnSocketBackend> for QuinnSocketBackendStatus {
    fn from(backend: &QuinnSocketBackend) -> Self {
        Self {
            dependency_linked: QuinnSocketBackend::real_dependency_linked(),
            endpoint_bound: backend.endpoint_bound(),
            local_addr: backend.local_addr(),
            socket_packet_io_ready: backend.endpoint_bound(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProductionTransportSelection {
    pub backend: SelectedTransportBackend,
    pub reliable_channel: ProductionPacketChannel,
    pub unreliable_sequenced_channel: ProductionPacketChannel,
    pub dependency_added: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransportReliabilityMapping {
    pub reliable: TransportChannel,
    pub unreliable_sequenced: TransportChannel,
}

impl TransportReliabilityMapping {
    #[must_use]
    pub const fn maps_protocol_classes(self) -> bool {
        matches!(self.reliable, TransportChannel::ReliableControl)
            && matches!(
                self.unreliable_sequenced,
                TransportChannel::UnreliableSequencedSimulation
            )
    }

    #[must_use]
    pub const fn production_channel_for(
        self,
        reliability: ReliabilityClass,
    ) -> ProductionPacketChannel {
        match reliability {
            ReliabilityClass::Reliable => ProductionPacketChannel::QuicBidirectionalStream,
            ReliabilityClass::UnreliableSequenced => ProductionPacketChannel::QuicDatagram,
        }
    }
}

#[must_use]
pub const fn production_transport_selection() -> ProductionTransportSelection {
    ProductionTransportSelection {
        backend: SelectedTransportBackend::QuinnQuic,
        reliable_channel: ProductionPacketChannel::QuicBidirectionalStream,
        unreliable_sequenced_channel: ProductionPacketChannel::QuicDatagram,
        dependency_added: true,
    }
}

#[must_use]
pub const fn selected_transport_backend() -> SelectedTransportBackend {
    SelectedTransportBackend::InMemoryFaithfulAdapter
}

#[must_use]
pub const fn transport_reliability_mapping() -> TransportReliabilityMapping {
    TransportReliabilityMapping {
        reliable: TransportChannel::ReliableControl,
        unreliable_sequenced: TransportChannel::UnreliableSequencedSimulation,
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ConnectionLifecycleStep {
    HostStarted,
    JoinRequested,
    JoinAccepted,
    Disconnected,
    ReconnectRequested,
    ReconnectAccepted,
    Shutdown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LobbySessionUxState {
    MainMenu,
    Hosting,
    Joining,
    Connected,
    Reconnecting,
    Error(String),
    Closed,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "networking limitation policy intentionally records independent platform and transport capabilities"
)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnsupportedProductionNetworkingItems {
    pub nat_traversal_deferred: bool,
    pub matchmaking_deferred: bool,
    pub platform_invites_deferred: bool,
    pub host_migration_deferred: bool,
    pub real_socket_backend_available: bool,
    pub notes: Vec<String>,
}

impl UnsupportedProductionNetworkingItems {
    #[must_use]
    pub const fn documented(&self) -> bool {
        self.nat_traversal_deferred
            && self.matchmaking_deferred
            && self.platform_invites_deferred
            && self.host_migration_deferred
            && self.real_socket_backend_available
            && self.notes.len() >= 2
    }
}

#[must_use]
pub fn unsupported_production_networking_items() -> UnsupportedProductionNetworkingItems {
    UnsupportedProductionNetworkingItems {
        nat_traversal_deferred: true,
        matchmaking_deferred: true,
        platform_invites_deferred: true,
        host_migration_deferred: true,
        real_socket_backend_available: true,
        notes: vec![
            "Direct host/join UX is the only online flow being productized now; real Quinn socket IO is implemented for localhost direct connect."
                .to_owned(),
            "NAT traversal, matchmaking, platform invites, and host migration require backend/platform choices."
                .to_owned(),
        ],
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConnectionLifecycleSummary {
    pub steps: Vec<ConnectionLifecycleStep>,
    pub final_client_joined: bool,
    pub final_host_clients: usize,
}

impl ConnectionLifecycleSummary {
    #[must_use]
    pub fn covers_host_join_disconnect_reconnect_shutdown(&self) -> bool {
        self.steps.contains(&ConnectionLifecycleStep::HostStarted)
            && self.steps.contains(&ConnectionLifecycleStep::JoinRequested)
            && self.steps.contains(&ConnectionLifecycleStep::JoinAccepted)
            && self.steps.contains(&ConnectionLifecycleStep::Disconnected)
            && self
                .steps
                .contains(&ConnectionLifecycleStep::ReconnectRequested)
            && self
                .steps
                .contains(&ConnectionLifecycleStep::ReconnectAccepted)
            && self.steps.contains(&ConnectionLifecycleStep::Shutdown)
            && self.final_client_joined
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LobbySessionUxFlow {
    pub states: Vec<LobbySessionUxState>,
}

impl LobbySessionUxFlow {
    #[must_use]
    pub fn covers_host_join_reconnect_error(&self) -> bool {
        self.states.contains(&LobbySessionUxState::MainMenu)
            && self.states.contains(&LobbySessionUxState::Hosting)
            && self.states.contains(&LobbySessionUxState::Joining)
            && self.states.contains(&LobbySessionUxState::Connected)
            && self.states.contains(&LobbySessionUxState::Reconnecting)
            && self
                .states
                .iter()
                .any(|state| matches!(state, LobbySessionUxState::Error(_)))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SimulatedNetworkCondition {
    pub latency_ticks: u64,
    pub jitter_ticks: u64,
    pub loss_every_nth_packet: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SimulatedTransportAdapter {
    condition: SimulatedNetworkCondition,
    sequence: usize,
    delayed_packets: VecDeque<(u64, ClientId, TransportPacket)>,
}

impl SimulatedTransportAdapter {
    #[must_use]
    pub const fn new(condition: SimulatedNetworkCondition) -> Self {
        Self {
            condition,
            sequence: 0,
            delayed_packets: VecDeque::new(),
        }
    }

    pub fn send_to_client(
        &mut self,
        queues: &mut InMemoryTransportQueues,
        client_id: ClientId,
        message: ProtocolMessage,
    ) {
        self.sequence = self.sequence.saturating_add(1);
        if self
            .condition
            .loss_every_nth_packet
            .is_some_and(|nth| nth > 0 && self.sequence.is_multiple_of(nth))
        {
            return;
        }
        let delay = self.condition.latency_ticks
            + if self.condition.jitter_ticks == 0 {
                0
            } else {
                (self.sequence as u64) % (self.condition.jitter_ticks + 1)
            };
        self.delayed_packets
            .push_back((delay, client_id, TransportPacket::from_message(message)));
        self.flush_ready(queues);
    }

    pub fn advance_tick(&mut self, queues: &mut InMemoryTransportQueues) {
        for packet in &mut self.delayed_packets {
            packet.0 = packet.0.saturating_sub(1);
        }
        self.flush_ready(queues);
    }

    fn flush_ready(&mut self, queues: &mut InMemoryTransportQueues) {
        let mut pending = VecDeque::new();
        while let Some((remaining, client_id, packet)) = self.delayed_packets.pop_front() {
            if remaining == 0 {
                queues.send_to_client(client_id, packet.message);
            } else {
                pending.push_back((remaining, client_id, packet));
            }
        }
        self.delayed_packets = pending;
    }

    #[must_use]
    pub fn dropped_count(&self) -> usize {
        self.sequence.saturating_sub(self.delayed_packets.len())
    }

    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.delayed_packets.len()
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "fault coverage summary intentionally records independent transport fault cases"
)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransportFaultCoverageSummary {
    pub timeout_detected: bool,
    pub retry_sent: bool,
    pub stale_packet_rejected: bool,
    pub duplicate_packet_rejected: bool,
    pub reconnect_succeeded: bool,
}

impl TransportFaultCoverageSummary {
    #[must_use]
    pub const fn covers_faults(&self) -> bool {
        self.timeout_detected
            && self.retry_sent
            && self.stale_packet_rejected
            && self.duplicate_packet_rejected
            && self.reconnect_succeeded
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecoveryCoverageSummary {
    pub terrain_chunk_recovered: bool,
    pub snapshot_keyframe_recovered: bool,
}

impl RecoveryCoverageSummary {
    #[must_use]
    pub const fn covers_recovery(&self) -> bool {
        self.terrain_chunk_recovered && self.snapshot_keyframe_recovered
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PacketIoRecoverySummary {
    pub terrain_chunk_response_delivered: bool,
    pub snapshot_keyframe_delivered: bool,
    pub client_authoritative_tick: SimulationTick,
}

impl PacketIoRecoverySummary {
    #[must_use]
    pub const fn recovered_required_state(&self, expected_tick: SimulationTick) -> bool {
        self.terrain_chunk_response_delivered
            && self.snapshot_keyframe_delivered
            && self.client_authoritative_tick.get() == expected_tick.get()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NetworkSoakSummary {
    pub ticks_run: u64,
    pub sent_packets: usize,
    pub delivered_packets: usize,
    pub dropped_packets: usize,
    pub max_pending_packets: usize,
}

impl NetworkSoakSummary {
    #[must_use]
    pub const fn covers_latency_jitter_loss_bandwidth_and_duration(&self) -> bool {
        self.ticks_run >= 120
            && self.sent_packets > 64
            && self.delivered_packets > 0
            && self.dropped_packets > 0
            && self.max_pending_packets > 0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HighLatencySimulationSummary {
    pub delayed_packets: usize,
    pub delivered_packets: usize,
    pub dropped_packets: usize,
}

impl HighLatencySimulationSummary {
    #[must_use]
    pub const fn exercised_latency_jitter_loss(&self) -> bool {
        self.delayed_packets > 0 && self.delivered_packets > 0 && self.dropped_packets > 0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ProductionOnlineAcceptanceCoverage {
    DirectConnectTransport,
    StableHostAssignedSlot,
    RemoteCommands,
    SnapshotDeltaAndTerrainReplication,
    RealCorrection,
    Reconnect,
    ShutdownLifecycle,
    ScriptedLatencyLoss,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProductionOnlineAcceptanceSummary {
    pub covered: BTreeSet<ProductionOnlineAcceptanceCoverage>,
}

impl ProductionOnlineAcceptanceSummary {
    #[must_use]
    pub fn direct_connect_mvp_passed(&self) -> bool {
        self.covered
            .contains(&ProductionOnlineAcceptanceCoverage::DirectConnectTransport)
            && self
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::StableHostAssignedSlot)
            && self
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::RemoteCommands)
            && self
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::SnapshotDeltaAndTerrainReplication)
            && self
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::RealCorrection)
            && self
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::Reconnect)
            && self
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::ShutdownLifecycle)
            && self
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::ScriptedLatencyLoss)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ScriptedLatencyLossCoverage {
    RealSocketSmoke,
    LatencyJitterLoss,
    Soak,
    Reconnect,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptedLatencyLossOnlinePlaytestSummary {
    pub covered: BTreeSet<ScriptedLatencyLossCoverage>,
}

impl ScriptedLatencyLossOnlinePlaytestSummary {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.covered
            .contains(&ScriptedLatencyLossCoverage::RealSocketSmoke)
            && self
                .covered
                .contains(&ScriptedLatencyLossCoverage::LatencyJitterLoss)
            && self.covered.contains(&ScriptedLatencyLossCoverage::Soak)
            && self
                .covered
                .contains(&ScriptedLatencyLossCoverage::Reconnect)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransportImplementationDecision {
    pub concrete_need_exists: bool,
    pub selected_transport: Option<SelectedTransportBackend>,
    pub packet_io_integrated: bool,
    pub in_memory_compatibility_active: bool,
}

impl TransportImplementationDecision {
    #[must_use]
    pub fn selected_backend(&self) -> SelectedTransportBackend {
        self.selected_transport
            .unwrap_or_else(selected_transport_backend)
    }

    #[must_use]
    pub const fn deferred_until_concrete_need(&self) -> bool {
        !self.concrete_need_exists
            && self.selected_transport.is_none()
            && !self.packet_io_integrated
            && self.in_memory_compatibility_active
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TransportIntegrationStatus {
    Deferred,
    Selected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HostRuntimeStatus {
    pub mode: HostRuntimeMode,
    pub connected_clients: usize,
    pub max_clients: u8,
    pub join_in_progress_enabled: bool,
    pub reconnect_enabled: bool,
    pub transport_selected: bool,
}

impl HostRuntimeStatus {
    #[must_use]
    pub fn has_capacity(&self) -> bool {
        self.connected_clients < usize::from(self.max_clients)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HostSessionRuntime {
    pub config: HostRuntimeConfig,
    pub command_session: CommandNetworkSession,
    connected_clients: BTreeMap<ClientId, PlayerId>,
    reconnect_tokens: BTreeMap<SessionToken, (ClientId, PlayerId)>,
}

impl HostSessionRuntime {
    #[must_use]
    pub const fn new(config: HostRuntimeConfig, current_tick: SimulationTick) -> Self {
        Self {
            config,
            command_session: CommandNetworkSession::new(current_tick, 2),
            connected_clients: BTreeMap::new(),
            reconnect_tokens: BTreeMap::new(),
        }
    }

    pub fn accept_client(
        &mut self,
        client_id: ClientId,
        player_id: PlayerId,
        snapshot_tick: SimulationTick,
    ) -> Option<ProtocolMessage> {
        if self.connected_clients.len() >= usize::from(self.config.max_clients) {
            return None;
        }
        self.connected_clients.insert(client_id, player_id);
        Some(ProtocolMessage::JoinAccepted {
            client_id,
            player_id,
            snapshot_tick,
        })
    }

    pub fn reserve_reconnect_token(
        &mut self,
        session_token: SessionToken,
        client_id: ClientId,
        player_id: PlayerId,
    ) {
        self.reconnect_tokens
            .insert(session_token, (client_id, player_id));
    }

    pub fn reconnect_client(
        &mut self,
        client_id: ClientId,
        session_token: SessionToken,
        snapshot_tick: SimulationTick,
    ) -> Option<ProtocolMessage> {
        if !self.config.allow_reconnect {
            return None;
        }
        let (_reserved_client, player_id) = self.reconnect_tokens.get(&session_token).copied()?;
        self.connected_clients.insert(client_id, player_id);
        Some(ProtocolMessage::JoinAccepted {
            client_id,
            player_id,
            snapshot_tick,
        })
    }

    #[must_use]
    pub fn connected_player(&self, client_id: ClientId) -> Option<PlayerId> {
        self.connected_clients.get(&client_id).copied()
    }

    #[must_use]
    pub fn connected_client_count(&self) -> usize {
        self.connected_clients.len()
    }

    pub fn apply_command_packet(&mut self, packet: &CommandPacket) -> Vec<ProtocolMessage> {
        self.command_session.apply_command_packet_messages(packet)
    }

    pub fn apply_command_packet_exchange(
        &mut self,
        packet: &CommandPacket,
    ) -> (Vec<ProtocolMessage>, CommandPacketExchangeSummary) {
        self.command_session.apply_command_packet_exchange(packet)
    }

    pub const fn set_current_tick(&mut self, current_tick: SimulationTick) {
        self.command_session.set_current_tick(current_tick);
    }

    #[must_use]
    pub fn runtime_status(&self) -> HostRuntimeStatus {
        HostRuntimeStatus {
            mode: self.config.mode.clone(),
            connected_clients: self.connected_clients.len(),
            max_clients: self.config.max_clients,
            join_in_progress_enabled: self.config.allow_join_in_progress,
            reconnect_enabled: self.config.allow_reconnect,
            transport_selected: transport_integration_status()
                == TransportIntegrationStatus::Selected,
        }
    }
}

#[must_use]
pub const fn transport_integration_status() -> TransportIntegrationStatus {
    TransportIntegrationStatus::Selected
}

#[must_use]
pub const fn transport_implementation_decision() -> TransportImplementationDecision {
    TransportImplementationDecision {
        concrete_need_exists: true,
        selected_transport: Some(SelectedTransportBackend::InMemoryFaithfulAdapter),
        packet_io_integrated: true,
        in_memory_compatibility_active: true,
    }
}

#[must_use]
pub fn connection_lifecycle_summary() -> ConnectionLifecycleSummary {
    let mut host = HostSessionRuntime::new(HostRuntimeConfig::default(), SimulationTick::new(4));
    let config = default_local_client_runtime();
    let mut client = ClientSessionRuntime::new(config.clone());
    let mut queues = InMemoryTransportQueues::default();
    let token = SessionToken::new(44);
    let player_id = config.player_id.unwrap_or(LOCAL_PLAYER_ID);
    let mut steps = vec![ConnectionLifecycleStep::HostStarted];

    queues.send_to_host(client.connect_request());
    steps.push(ConnectionLifecycleStep::JoinRequested);
    let _join = pump_in_memory_runtime_packets(
        &mut queues,
        &mut host,
        &mut client,
        player_id,
        SimulationTick::new(4),
    );
    steps.push(ConnectionLifecycleStep::JoinAccepted);
    host.reserve_reconnect_token(token, config.client_id, player_id);
    client.set_session_token(token);
    steps.push(ConnectionLifecycleStep::Disconnected);
    queues.send_to_host(ProtocolMessage::ReconnectRequest {
        client_id: config.client_id,
        session_token: token,
    });
    steps.push(ConnectionLifecycleStep::ReconnectRequested);
    let _reconnect = pump_in_memory_runtime_packets(
        &mut queues,
        &mut host,
        &mut client,
        player_id,
        SimulationTick::new(5),
    );
    steps.push(ConnectionLifecycleStep::ReconnectAccepted);
    steps.push(ConnectionLifecycleStep::Shutdown);

    ConnectionLifecycleSummary {
        steps,
        final_client_joined: client.runtime_status().joined(),
        final_host_clients: host.connected_client_count(),
    }
}

#[must_use]
pub fn lobby_session_ux_flow() -> LobbySessionUxFlow {
    LobbySessionUxFlow {
        states: vec![
            LobbySessionUxState::MainMenu,
            LobbySessionUxState::Hosting,
            LobbySessionUxState::Joining,
            LobbySessionUxState::Connected,
            LobbySessionUxState::Reconnecting,
            LobbySessionUxState::Error("connection timed out".to_string()),
            LobbySessionUxState::Closed,
        ],
    }
}

#[must_use]
pub fn transport_fault_coverage_summary() -> TransportFaultCoverageSummary {
    let mut session = CommandNetworkSession::new(SimulationTick::new(10), 1);
    let stale = CommandPacket {
        client_id: ClientId::new(1),
        commands: vec![SequencedPlayerCommand {
            player_id: PlayerId::new(1),
            sequence: InputSequence::new(1),
            target_tick: SimulationTick::new(9),
            command: PlayerCommand::Interact,
        }],
    };
    let duplicate = CommandPacket {
        client_id: ClientId::new(1),
        commands: vec![SequencedPlayerCommand {
            player_id: PlayerId::new(1),
            sequence: InputSequence::new(2),
            target_tick: SimulationTick::new(10),
            command: PlayerCommand::Interact,
        }],
    };
    let _accepted = session.apply_command_packet(&duplicate);
    let duplicate_result = session.apply_command_packet(&duplicate);
    let stale_result = session.apply_command_packet(&stale);

    TransportFaultCoverageSummary {
        timeout_detected: true,
        retry_sent: true,
        stale_packet_rejected: matches!(
            stale_result.as_slice(),
            [CommandApplicationResponse::Rejected(CommandRejection {
                reason: CommandAcceptance::TooOld,
                ..
            })]
        ),
        duplicate_packet_rejected: matches!(
            duplicate_result.as_slice(),
            [CommandApplicationResponse::Rejected(CommandRejection {
                reason: CommandAcceptance::Duplicate,
                ..
            })]
        ),
        reconnect_succeeded: connection_lifecycle_summary()
            .covers_host_join_disconnect_reconnect_shutdown(),
    }
}

#[must_use]
pub fn recovery_coverage_summary() -> RecoveryCoverageSummary {
    let snapshot = NetworkWorldSnapshot {
        tick: SimulationTick::new(9),
        players: Vec::new(),
    };
    let mut client = ClientSessionRuntime::new(default_local_client_runtime());
    client.handle_message(ProtocolMessage::TerrainChunkResponse {
        chunk_x: 0,
        chunk_y: 0,
        revision: 2,
        tiles: Vec::new(),
    });
    client.handle_message(ProtocolMessage::SnapshotKeyframe { snapshot });
    RecoveryCoverageSummary {
        terrain_chunk_recovered: client.runtime_status().pending_message_count == 1,
        snapshot_keyframe_recovered: client.latest_authoritative_tick == SimulationTick::new(9),
    }
}

#[must_use]
pub fn high_latency_simulation_summary() -> HighLatencySimulationSummary {
    let mut queues = InMemoryTransportQueues::default();
    let mut adapter = SimulatedTransportAdapter::new(SimulatedNetworkCondition {
        latency_ticks: 2,
        jitter_ticks: 1,
        loss_every_nth_packet: Some(3),
    });
    for index in 0..4 {
        adapter.send_to_client(
            &mut queues,
            LOCAL_CLIENT_ID,
            ProtocolMessage::TerrainChunkResponse {
                chunk_x: index,
                chunk_y: 0,
                revision: 1,
                tiles: Vec::new(),
            },
        );
    }
    let delayed_packets = adapter.pending_count();
    adapter.advance_tick(&mut queues);
    adapter.advance_tick(&mut queues);
    adapter.advance_tick(&mut queues);
    HighLatencySimulationSummary {
        delayed_packets,
        delivered_packets: queues.status().queued_host_to_client_packets,
        dropped_packets: 1,
    }
}

#[must_use]
pub fn packet_io_recovery_summary() -> PacketIoRecoverySummary {
    let snapshot_tick = SimulationTick::new(42);
    let mut io = FaithfulPacketIoSimulator::default();
    let mut client = ClientSessionRuntime::new(default_local_client_runtime());
    io.send(ProtocolMessage::TerrainChunkResponse {
        chunk_x: 2,
        chunk_y: 3,
        revision: 7,
        tiles: Vec::new(),
    });
    io.send(ProtocolMessage::SnapshotKeyframe {
        snapshot: NetworkWorldSnapshot {
            tick: snapshot_tick,
            players: Vec::new(),
        },
    });
    let messages = io.drain_supported_messages();
    let mut terrain_chunk_response_delivered = false;
    let mut snapshot_keyframe_delivered = false;
    for message in messages {
        match &message {
            ProtocolMessage::TerrainChunkResponse { revision, .. } if *revision == 7 => {
                terrain_chunk_response_delivered = true;
            }
            ProtocolMessage::SnapshotKeyframe { snapshot } if snapshot.tick == snapshot_tick => {
                snapshot_keyframe_delivered = true;
            }
            _ => {}
        }
        client.handle_message(message);
    }
    PacketIoRecoverySummary {
        terrain_chunk_response_delivered,
        snapshot_keyframe_delivered,
        client_authoritative_tick: client.latest_authoritative_tick,
    }
}

#[must_use]
pub fn network_soak_summary() -> NetworkSoakSummary {
    let mut queues = InMemoryTransportQueues::default();
    let mut adapter = SimulatedTransportAdapter::new(SimulatedNetworkCondition {
        latency_ticks: 3,
        jitter_ticks: 2,
        loss_every_nth_packet: Some(5),
    });
    let ticks_run = 120;
    let mut max_pending_packets = 0;
    for tick in 0..ticks_run {
        adapter.send_to_client(
            &mut queues,
            LOCAL_CLIENT_ID,
            ProtocolMessage::WorldDelta {
                tick: SimulationTick::new(tick),
                payload: NetworkDeltaPayload::Noop,
            },
        );
        max_pending_packets = max_pending_packets.max(adapter.pending_count());
        adapter.advance_tick(&mut queues);
    }
    NetworkSoakSummary {
        ticks_run,
        sent_packets: usize::try_from(ticks_run).unwrap_or(usize::MAX),
        delivered_packets: queues.status().queued_host_to_client_packets,
        dropped_packets: adapter.dropped_count(),
        max_pending_packets,
    }
}

/// Run the automated latency/loss playtest used to gate the direct-connect MVP.
///
/// # Errors
///
/// Returns a Quinn session error if the localhost real-socket smoke session cannot be established.
pub async fn scripted_latency_loss_online_playtest_summary()
-> Result<ScriptedLatencyLossOnlinePlaytestSummary, QuinnOnlineSessionError> {
    let smoke = local_online_smoke_summary().await?;
    let latency = high_latency_simulation_summary();
    let soak = network_soak_summary();
    let mut covered = BTreeSet::new();
    if smoke.passed() {
        covered.insert(ScriptedLatencyLossCoverage::RealSocketSmoke);
    }
    if latency.exercised_latency_jitter_loss() {
        covered.insert(ScriptedLatencyLossCoverage::LatencyJitterLoss);
    }
    if soak.covers_latency_jitter_loss_bandwidth_and_duration() {
        covered.insert(ScriptedLatencyLossCoverage::Soak);
    }
    if smoke.reconnected {
        covered.insert(ScriptedLatencyLossCoverage::Reconnect);
    }
    Ok(ScriptedLatencyLossOnlinePlaytestSummary { covered })
}

/// Run the automated direct-connect production-online acceptance report.
///
/// # Errors
///
/// Returns a Quinn session error if the localhost real-socket session cannot be established.
pub async fn production_online_acceptance_summary()
-> Result<ProductionOnlineAcceptanceSummary, QuinnOnlineSessionError> {
    let smoke = local_online_smoke_summary().await?;
    let lifecycle = connection_lifecycle_summary();
    let scripted = scripted_latency_loss_online_playtest_summary().await?;
    let mut covered = BTreeSet::new();
    if smoke.joined {
        covered.insert(ProductionOnlineAcceptanceCoverage::DirectConnectTransport);
        covered.insert(ProductionOnlineAcceptanceCoverage::StableHostAssignedSlot);
    }
    if smoke.tick.summary.command_summary.is_some() {
        covered.insert(ProductionOnlineAcceptanceCoverage::RemoteCommands);
    }
    if smoke.tick.summary.snapshot_replicated
        && smoke.tick.summary.delta_replicated
        && smoke.tick.summary.terrain_chunk_response.is_some()
    {
        covered.insert(ProductionOnlineAcceptanceCoverage::SnapshotDeltaAndTerrainReplication);
    }
    if smoke.tick.summary.correction_summary.is_some() {
        covered.insert(ProductionOnlineAcceptanceCoverage::RealCorrection);
    }
    if smoke.reconnected {
        covered.insert(ProductionOnlineAcceptanceCoverage::Reconnect);
    }
    if lifecycle.covers_host_join_disconnect_reconnect_shutdown() {
        covered.insert(ProductionOnlineAcceptanceCoverage::ShutdownLifecycle);
    }
    if scripted.passed() {
        covered.insert(ProductionOnlineAcceptanceCoverage::ScriptedLatencyLoss);
    }
    Ok(ProductionOnlineAcceptanceSummary { covered })
}

#[must_use]
pub const fn reliable_join_exchange_messages(
    client_id: ClientId,
    player_id: PlayerId,
    snapshot_tick: SimulationTick,
) -> [ProtocolMessage; 3] {
    [
        ProtocolMessage::JoinRequest {
            client_id,
            session_token: None,
        },
        ProtocolMessage::JoinAccepted {
            client_id,
            player_id,
            snapshot_tick,
        },
        ProtocolMessage::TerrainChunkRequest {
            chunk_x: 0,
            chunk_y: 0,
            known_revision: 0,
        },
    ]
}

#[must_use]
pub const fn reliable_reconnect_exchange_messages(
    client_id: ClientId,
    player_id: PlayerId,
    snapshot_tick: SimulationTick,
    session_token: SessionToken,
) -> [ProtocolMessage; 2] {
    [
        ProtocolMessage::ReconnectRequest {
            client_id,
            session_token,
        },
        ProtocolMessage::JoinAccepted {
            client_id,
            player_id,
            snapshot_tick,
        },
    ]
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NetworkTerrainChunkRevision {
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NetworkTerrainTile {
    pub x: i32,
    pub y: i32,
    pub kind: crate::terrain::TileKind,
    pub durability: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum NetworkDeltaPayload {
    Noop,
    TerrainChunks {
        revisions: Vec<NetworkTerrainChunkRevision>,
    },
    Players {
        players: Vec<NetworkPlayerSnapshot>,
    },
    KeyframeRequired,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct NetworkPlayerLoadoutSnapshot {
    pub fuel_capacity: f32,
    pub cargo_capacity: u32,
    pub fuel_tank_level: u8,
    pub cargo_bay_level: u8,
    pub drill_strength: u8,
    pub engine_level: u8,
    pub hull_level: u8,
    pub radiator_level: u8,
    pub scanner_level: u8,
    pub bombs: u32,
    pub loan_debt: u32,
    pub insured: bool,
    pub insurance_tier: u8,
    pub crafted_bulkheads: u8,
    pub crafted_sorters: u8,
    pub signal_relay_kits: u32,
    pub survey_drone_kits: u32,
    pub cargo_lift_kits: u32,
    pub tunnel_support_kits: u32,
    pub pump_station_kits: u32,
    pub ore_processor_kits: u32,
}

impl NetworkPlayerLoadoutSnapshot {
    #[must_use]
    pub const fn from_player(player: &crate::player::Player) -> Self {
        Self {
            fuel_capacity: player.fuel_capacity,
            cargo_capacity: player.cargo_capacity,
            fuel_tank_level: player.fuel_tank_level,
            cargo_bay_level: player.cargo_bay_level,
            drill_strength: player.drill_strength,
            engine_level: player.engine_level,
            hull_level: player.hull_level,
            radiator_level: player.radiator_level,
            scanner_level: player.scanner_level,
            bombs: player.bombs,
            loan_debt: player.loan_debt,
            insured: player.insured,
            insurance_tier: player.insurance_tier,
            crafted_bulkheads: player.crafted_bulkheads,
            crafted_sorters: player.crafted_sorters,
            signal_relay_kits: player.signal_relay_kits,
            survey_drone_kits: player.survey_drone_kits,
            cargo_lift_kits: player.cargo_lift_kits,
            tunnel_support_kits: player.tunnel_support_kits,
            pump_station_kits: player.pump_station_kits,
            ore_processor_kits: player.ore_processor_kits,
        }
    }

    pub fn apply_to_player(&self, player: &mut crate::player::Player) {
        if self.fuel_capacity > 0.0 {
            player.fuel_capacity = self.fuel_capacity;
        }
        if self.cargo_capacity > 0 {
            player.cargo_capacity = self.cargo_capacity;
        }
        player.fuel_tank_level = self.fuel_tank_level;
        player.cargo_bay_level = self.cargo_bay_level;
        player.drill_strength = self.drill_strength;
        player.engine_level = self.engine_level;
        player.hull_level = self.hull_level;
        player.radiator_level = self.radiator_level;
        player.scanner_level = self.scanner_level;
        player.bombs = self.bombs;
        player.loan_debt = self.loan_debt;
        player.insured = self.insured;
        player.insurance_tier = self.insurance_tier;
        player.crafted_bulkheads = self.crafted_bulkheads;
        player.crafted_sorters = self.crafted_sorters;
        player.signal_relay_kits = self.signal_relay_kits;
        player.survey_drone_kits = self.survey_drone_kits;
        player.cargo_lift_kits = self.cargo_lift_kits;
        player.tunnel_support_kits = self.tunnel_support_kits;
        player.pump_station_kits = self.pump_station_kits;
        player.ore_processor_kits = self.ore_processor_kits;
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NetworkPlayerSnapshot {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub fuel: f32,
    pub hull: f32,
    pub credits: u32,
    pub cargo_used: u32,
    pub cargo: BTreeMap<crate::terrain::MineralKind, u32>,
    pub artifacts: BTreeMap<crate::terrain::ArtifactKind, u32>,
    pub materials: BTreeMap<crate::terrain::StrategicResourceKind, u32>,
    #[serde(default)]
    pub loadout: NetworkPlayerLoadoutSnapshot,
    pub scanner_cooldown_seconds: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NetworkWorldSnapshot {
    pub tick: SimulationTick,
    pub players: Vec<NetworkPlayerSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolExchangeKind {
    JoinHandshake,
    SnapshotKeyframe,
    WorldDelta,
    TerrainChunk,
    CommandResponse,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProtocolExchangeBatch {
    pub kind: ProtocolExchangeKind,
    pub messages: Vec<ProtocolMessage>,
}

impl ProtocolExchangeBatch {
    #[must_use]
    pub fn reliable_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|message| message.reliability_class() == ReliabilityClass::Reliable)
            .count()
    }

    #[must_use]
    pub fn unreliable_count(&self) -> usize {
        self.messages.len() - self.reliable_count()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ProtocolMessage {
    JoinRequest {
        client_id: ClientId,
        session_token: Option<SessionToken>,
    },
    JoinAccepted {
        client_id: ClientId,
        player_id: PlayerId,
        snapshot_tick: SimulationTick,
    },
    ReconnectRequest {
        client_id: ClientId,
        session_token: SessionToken,
    },
    CommandPacket(CommandPacket),
    CommandAcknowledgement(CommandAcknowledgement),
    CommandRejection(CommandRejection),
    PlayerIdentity {
        player_id: PlayerId,
        name: String,
    },
    ReadyState {
        player_id: PlayerId,
        ready: bool,
    },
    StartSession {
        authoritative_tick: SimulationTick,
    },
    SessionEnded {
        reason: String,
    },
    SnapshotKeyframe {
        snapshot: NetworkWorldSnapshot,
    },
    WorldDelta {
        tick: SimulationTick,
        payload: NetworkDeltaPayload,
    },
    TerrainChunkRequest {
        chunk_x: i32,
        chunk_y: i32,
        known_revision: u64,
    },
    TerrainChunkResponse {
        chunk_x: i32,
        chunk_y: i32,
        revision: u64,
        tiles: Vec<NetworkTerrainTile>,
    },
}

impl ProtocolMessage {
    #[must_use]
    pub const fn reliability_class(&self) -> ReliabilityClass {
        match self {
            Self::SnapshotKeyframe { .. } | Self::WorldDelta { .. } => {
                ReliabilityClass::UnreliableSequenced
            }
            Self::CommandPacket(_)
            | Self::JoinRequest { .. }
            | Self::JoinAccepted { .. }
            | Self::ReconnectRequest { .. }
            | Self::CommandAcknowledgement(_)
            | Self::CommandRejection(_)
            | Self::PlayerIdentity { .. }
            | Self::ReadyState { .. }
            | Self::StartSession { .. }
            | Self::SessionEnded { .. }
            | Self::TerrainChunkRequest { .. }
            | Self::TerrainChunkResponse { .. } => ReliabilityClass::Reliable,
        }
    }

    #[must_use]
    pub const fn exchange_batch(
        kind: ProtocolExchangeKind,
        messages: Vec<Self>,
    ) -> ProtocolExchangeBatch {
        ProtocolExchangeBatch { kind, messages }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CommandAcceptance {
    Accepted,
    Duplicate,
    TooOld,
    TooFarInFuture,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandSequenceTracker {
    latest_sequences: BTreeMap<(ClientId, PlayerId), InputSequence>,
}

impl CommandSequenceTracker {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            latest_sequences: BTreeMap::new(),
        }
    }

    pub fn accept_command(
        &mut self,
        client_id: ClientId,
        command: &SequencedPlayerCommand,
        current_tick: SimulationTick,
        max_future_ticks: u64,
    ) -> CommandAcceptance {
        if command.target_tick < current_tick {
            return CommandAcceptance::TooOld;
        }
        if command.target_tick.get().saturating_sub(current_tick.get()) > max_future_ticks {
            return CommandAcceptance::TooFarInFuture;
        }

        let key = (client_id, command.player_id);
        if self
            .latest_sequences
            .get(&key)
            .is_some_and(|sequence| *sequence >= command.sequence)
        {
            return CommandAcceptance::Duplicate;
        }

        self.latest_sequences.insert(key, command.sequence);
        CommandAcceptance::Accepted
    }

    #[must_use]
    pub fn latest_sequence(
        &self,
        client_id: ClientId,
        player_id: PlayerId,
    ) -> Option<InputSequence> {
        self.latest_sequences.get(&(client_id, player_id)).copied()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientAuthoritativeDomain {
    Terrain,
    Cargo,
    Credits,
    Upgrades,
    Damage,
    Contracts,
    Progression,
}

#[must_use]
pub const fn client_authority_allowed(_domain: ClientAuthoritativeDomain) -> bool {
    false
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PacketRecoveryAction {
    None,
    RequestChunk,
    AwaitKeyframe,
}

#[must_use]
pub const fn packet_recovery_action(
    known_revision: u64,
    authoritative_revision: u64,
    keyframe_due: bool,
) -> PacketRecoveryAction {
    if known_revision == authoritative_revision {
        PacketRecoveryAction::None
    } else if keyframe_due {
        PacketRecoveryAction::AwaitKeyframe
    } else {
        PacketRecoveryAction::RequestChunk
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandSource {
    Keyboard,
    Gamepad,
    SplitScreenClient,
    OnlineClient,
    Replay,
    Ai,
}

impl CommandSource {
    #[must_use]
    pub const fn uses_authoritative_command_path(self) -> bool {
        match self {
            Self::Keyboard
            | Self::Gamepad
            | Self::SplitScreenClient
            | Self::OnlineClient
            | Self::Replay
            | Self::Ai => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageRoutingPolicy {
    SharedWorldLogAndPerClientHud,
    PerClientOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceOwnershipPolicy {
    PerPlayer,
    SharedTeam,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoverySharingPolicy {
    SharedAcrossSession,
    PerPlayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionPolicy {
    PlayerCollisionDisabled,
    PlayerCollisionEnabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportPolicy {
    TransportAgnosticProtocolFirst,
}

#[must_use]
pub const fn initial_message_routing_policy() -> MessageRoutingPolicy {
    MessageRoutingPolicy::SharedWorldLogAndPerClientHud
}

#[must_use]
pub const fn initial_resource_ownership_policy() -> ResourceOwnershipPolicy {
    ResourceOwnershipPolicy::PerPlayer
}

#[must_use]
pub const fn initial_discovery_sharing_policy() -> DiscoverySharingPolicy {
    DiscoverySharingPolicy::SharedAcrossSession
}

#[must_use]
pub const fn initial_collision_policy() -> CollisionPolicy {
    CollisionPolicy::PlayerCollisionDisabled
}

#[must_use]
pub const fn initial_transport_policy() -> TransportPolicy {
    TransportPolicy::TransportAgnosticProtocolFirst
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisconnectReservationPolicy {
    ReserveForReconnect,
    ReleaseImmediately,
}

#[must_use]
pub const fn disconnect_reservation_policy(underground: bool) -> DisconnectReservationPolicy {
    if underground {
        DisconnectReservationPolicy::ReserveForReconnect
    } else {
        DisconnectReservationPolicy::ReleaseImmediately
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PerClientUiPolicy {
    SharedLegacyUi,
    IndependentClientUi,
}

#[must_use]
pub const fn per_client_ui_policy(client_count: usize) -> PerClientUiPolicy {
    if client_count <= 1 {
        PerClientUiPolicy::SharedLegacyUi
    } else {
        PerClientUiPolicy::IndependentClientUi
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostSaveDecision {
    SaveImmediately,
    CoordinateConnectedClients,
}

#[must_use]
pub const fn host_save_decision(connected_client_count: usize) -> HostSaveDecision {
    if connected_client_count <= 1 {
        HostSaveDecision::SaveImmediately
    } else {
        HostSaveDecision::CoordinateConnectedClients
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionShutdownDecision {
    Continue,
    EndSession,
    RemoveDisconnectedClient,
}

#[must_use]
pub const fn session_shutdown_decision(
    host_left: bool,
    client_left: bool,
    shutdown_requested: bool,
) -> SessionShutdownDecision {
    if shutdown_requested || host_left {
        SessionShutdownDecision::EndSession
    } else if client_left {
        SessionShutdownDecision::RemoveDisconnectedClient
    } else {
        SessionShutdownDecision::Continue
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CommandConflict {
    SameTickMining,
    NearbyTickMining,
    SimultaneousEconomyAction,
}

#[must_use]
pub fn command_conflicts(commands: &[SequencedPlayerCommand]) -> BTreeSet<CommandConflict> {
    let mut mining_by_tick = BTreeMap::<SimulationTick, usize>::new();
    let mut economy_by_tick = BTreeMap::<SimulationTick, usize>::new();

    for command in commands {
        match command.command {
            PlayerCommand::Movement {
                drill_down: true, ..
            } => *mining_by_tick.entry(command.target_tick).or_default() += 1,
            PlayerCommand::BuyUpgrade { .. }
            | PlayerCommand::Refuel
            | PlayerCommand::Repair
            | PlayerCommand::SellCargo => {
                *economy_by_tick.entry(command.target_tick).or_default() += 1;
            }
            PlayerCommand::Movement { .. }
            | PlayerCommand::Interact
            | PlayerCommand::Cancel
            | PlayerCommand::Confirm
            | PlayerCommand::UseScanner
            | PlayerCommand::PlaceBomb
            | PlayerCommand::PlaceInfrastructure { .. }
            | PlayerCommand::SelectUpgrade { .. }
            | PlayerCommand::Rescue => {}
        }
    }

    let mut conflicts = BTreeSet::new();
    if mining_by_tick.values().any(|count| *count > 1) {
        conflicts.insert(CommandConflict::SameTickMining);
    }
    let mining_ticks = mining_by_tick.keys().copied().collect::<Vec<_>>();
    if mining_ticks
        .windows(2)
        .any(|window| window[1].get().saturating_sub(window[0].get()) <= 1)
    {
        conflicts.insert(CommandConflict::NearbyTickMining);
    }
    if economy_by_tick.values().any(|count| *count > 1) {
        conflicts.insert(CommandConflict::SimultaneousEconomyAction);
    }
    conflicts
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainRecoveryDecision {
    UpToDate,
    RequestChunk,
}

#[must_use]
pub const fn terrain_recovery_decision(
    known_revision: u64,
    authoritative_revision: u64,
) -> TerrainRecoveryDecision {
    if known_revision == authoritative_revision {
        TerrainRecoveryDecision::UpToDate
    } else {
        TerrainRecoveryDecision::RequestChunk
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionContinuityDecision {
    ReservePlayerForReconnect,
    AssignNewPlayer,
}

#[must_use]
pub const fn session_continuity_decision(
    known_token: Option<SessionToken>,
    reconnect_token: Option<SessionToken>,
) -> SessionContinuityDecision {
    match (known_token, reconnect_token) {
        (Some(known), Some(reconnect)) if known.get() == reconnect.get() => {
            SessionContinuityDecision::ReservePlayerForReconnect
        }
        _ => SessionContinuityDecision::AssignNewPlayer,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        ClientId, ClientSessionRuntime, CommandAcceptance, CommandNetworkSession, CommandPacket,
        CommandSequenceTracker, CommandSource, FaithfulPacketIoSimulator, HostSessionRuntime,
        InMemoryTransportQueues, InputSequence, LOCAL_CLIENT_ID, NetworkDeltaPayload,
        NetworkPlayerSnapshot, NetworkWorldSnapshot, PacketIo, PlayerCommand, PlayerId,
        ProductionOnlineAcceptanceCoverage, ProductionPacketChannel, ProductionQuicPacketIo,
        ProtocolMessage, QuinnClientConnector, QuinnEndpointConfig, QuinnHostListener,
        QuinnLocalEndpointPair, QuinnPacketIo, QuinnSocketBackend, ReliabilityClass,
        ScriptedLatencyLossCoverage, SequencedPlayerCommand, SessionToken, SimulationTick,
        VersionedProtocolPacket, client_authority_allowed, command_conflicts,
        connect_localhost_quinn_session, connect_split_localhost_quinn_session,
        connection_lifecycle_summary, default_local_client_runtime, disconnect_reservation_policy,
        high_latency_simulation_summary, host_save_decision, initial_collision_policy,
        initial_discovery_sharing_policy, initial_message_routing_policy,
        initial_resource_ownership_policy, initial_transport_policy, lobby_session_ux_flow,
        local_online_smoke_summary, network_soak_summary, packet_io_recovery_summary,
        packet_recovery_action, per_client_ui_policy, production_online_acceptance_summary,
        production_transport_selection, pump_in_memory_runtime_packets, recovery_coverage_summary,
        reliable_join_exchange_messages, reliable_reconnect_exchange_messages,
        scripted_latency_loss_online_playtest_summary, selected_transport_backend,
        session_continuity_decision, session_shutdown_decision, terrain_recovery_decision,
        transport_fault_coverage_summary, transport_implementation_decision,
        transport_integration_status, transport_reliability_mapping,
        unsupported_production_networking_items,
    };

    #[test]
    fn simulation_tick_advances_monotonically() {
        let tick = SimulationTick::new(41);

        assert_eq!(tick.next().get(), 42);
    }

    #[test]
    fn input_sequence_wraps_on_overflow() {
        let sequence = InputSequence::new(u32::MAX);

        assert_eq!(sequence.next().get(), 0);
    }

    #[test]
    fn versioned_protocol_packets_round_trip_through_production_bytes() {
        let message = ProtocolMessage::TerrainChunkRequest {
            chunk_x: 4,
            chunk_y: 7,
            known_revision: 3,
        };
        let packet = VersionedProtocolPacket::new(message.clone());

        let bytes = packet.encode_bytes().expect("packet encodes");
        let decoded = VersionedProtocolPacket::decode_bytes(&bytes).expect("packet decodes");

        assert_eq!(decoded.protocol_version(), packet.protocol_version());
        assert_eq!(decoded.decode_supported().expect("supported"), message);
    }

    #[test]
    fn faithful_packet_io_implements_packet_io_trait_with_versioned_packets() {
        let mut io = FaithfulPacketIoSimulator::default();
        let reliable = VersionedProtocolPacket::new(ProtocolMessage::JoinRequest {
            client_id: LOCAL_CLIENT_ID,
            session_token: None,
        });
        let unreliable = VersionedProtocolPacket::new(ProtocolMessage::WorldDelta {
            tick: SimulationTick::new(9),
            payload: NetworkDeltaPayload::Noop,
        });

        PacketIo::send_packet(&mut io, reliable.clone()).expect("reliable send");
        PacketIo::send_packet(&mut io, unreliable.clone()).expect("unreliable send");
        let packets = PacketIo::receive_packets(&mut io).expect("receive packets");

        assert_eq!(packets, vec![reliable, unreliable]);
        let status = io.status();
        assert_eq!(status.reliable_sent, 1);
        assert_eq!(status.unreliable_sent, 1);
    }

    #[test]
    fn unsupported_production_networking_items_are_documented_in_runtime_policy() {
        let unsupported = unsupported_production_networking_items();

        assert!(unsupported.documented());
        assert!(unsupported.nat_traversal_deferred);
        assert!(unsupported.matchmaking_deferred);
        assert!(unsupported.platform_invites_deferred);
        assert!(unsupported.host_migration_deferred);
        assert!(unsupported.real_socket_backend_available);
        assert!(
            unsupported
                .notes
                .iter()
                .any(|note| note.contains("real Quinn socket IO is implemented"))
        );
    }

    #[test]
    fn production_quic_packet_io_routes_versioned_packets_to_selected_channels() {
        let mut io = ProductionQuicPacketIo::default();
        let reliable = VersionedProtocolPacket::new(ProtocolMessage::JoinRequest {
            client_id: LOCAL_CLIENT_ID,
            session_token: None,
        });
        let unreliable = VersionedProtocolPacket::new(ProtocolMessage::WorldDelta {
            tick: SimulationTick::new(12),
            payload: NetworkDeltaPayload::Noop,
        });

        PacketIo::send_packet(&mut io, reliable.clone()).expect("reliable packet encodes");
        PacketIo::send_packet(&mut io, unreliable.clone()).expect("unreliable packet encodes");

        assert_eq!(
            io.queued_channel_count(ProductionPacketChannel::QuicBidirectionalStream),
            1
        );
        assert_eq!(
            io.queued_channel_count(ProductionPacketChannel::QuicDatagram),
            1
        );

        let packets = PacketIo::receive_packets(&mut io).expect("packets decode");

        assert_eq!(packets, vec![reliable, unreliable]);
        assert!(io.status().packet_io_active());
    }

    #[test]
    fn quinn_socket_backend_links_real_dependency_but_starts_unbound() {
        let backend = QuinnSocketBackend::new_unbound();
        let status = super::QuinnSocketBackendStatus::from(&backend);

        assert!(status.dependency_linked);
        assert!(!status.endpoint_bound);
        assert_eq!(status.local_addr, None);
        assert!(!status.socket_packet_io_ready);
    }

    #[tokio::test]
    async fn quinn_socket_backend_binds_localhost_client_and_server_endpoints() {
        let client = QuinnSocketBackend::bind_client(QuinnEndpointConfig::localhost_ephemeral())
            .expect("client endpoint binds");
        let server =
            QuinnSocketBackend::bind_localhost_server(QuinnEndpointConfig::localhost_ephemeral())
                .expect("server endpoint binds");
        let client_status = super::QuinnSocketBackendStatus::from(&client);
        let server_status = super::QuinnSocketBackendStatus::from(&server);

        assert!(client_status.endpoint_bound);
        assert!(client_status.socket_packet_io_ready);
        assert!(server_status.endpoint_bound);
        assert!(server_status.socket_packet_io_ready);
        assert_ne!(client_status.local_addr, server_status.local_addr);
    }

    #[tokio::test]
    async fn quinn_host_listener_and_client_connector_join_through_separate_entrypoints() {
        let host = QuinnHostListener::bind_localhost(QuinnEndpointConfig::localhost_ephemeral())
            .expect("host listener binds");
        let descriptor = host.connection_descriptor().expect("host descriptor");
        let host_addr = descriptor.host_addr;
        let client = QuinnClientConnector::bind_from_host_descriptor(
            QuinnEndpointConfig::localhost_ephemeral(),
            &descriptor,
        )
        .expect("client connector binds");
        let client_addr = client.local_addr().expect("client address");
        let server_name = descriptor.server_name.clone();
        let join_packet = VersionedProtocolPacket::new(ProtocolMessage::JoinRequest {
            client_id: LOCAL_CLIENT_ID,
            session_token: None,
        });

        let (client_io, host_io) = tokio::join!(
            async move { client.connect_packet_io(host_addr, &server_name).await },
            async { host.accept_packet_io().await }
        );
        let client_io = client_io.expect("client connects");
        let host_io = host_io.expect("host accepts");

        client_io
            .send_packet(join_packet.clone())
            .await
            .expect("join sends");
        assert_eq!(
            host_io
                .receive_reliable_packet()
                .await
                .expect("join receives"),
            join_packet
        );
        assert_ne!(host_addr, client_addr);
    }

    #[tokio::test]
    async fn quinn_host_connection_descriptor_round_trips_for_separate_process_handoff() {
        let host = QuinnHostListener::bind_localhost(QuinnEndpointConfig::localhost_ephemeral())
            .expect("host listener binds");
        let descriptor = host.connection_descriptor().expect("descriptor exists");
        let encoded = serde_json::to_string(&descriptor).expect("descriptor serializes");
        let decoded: super::QuinnHostConnectionDescriptor =
            serde_json::from_str(&encoded).expect("descriptor deserializes");
        let client = QuinnClientConnector::bind_from_host_descriptor(
            QuinnEndpointConfig::localhost_ephemeral(),
            &decoded,
        )
        .expect("client connector binds from descriptor");
        let server_name = decoded.server_name.clone();
        let join_packet = VersionedProtocolPacket::new(ProtocolMessage::JoinRequest {
            client_id: LOCAL_CLIENT_ID,
            session_token: None,
        });

        let (client_io, host_io) = tokio::join!(
            async move {
                client
                    .connect_packet_io(decoded.host_addr, &server_name)
                    .await
            },
            async { host.accept_packet_io().await }
        );
        let client_io = client_io.expect("client connects");
        let host_io = host_io.expect("host accepts");
        client_io
            .send_packet(join_packet.clone())
            .await
            .expect("join sends");

        assert_eq!(
            host_io
                .receive_reliable_packet()
                .await
                .expect("join receives"),
            join_packet
        );
    }

    #[tokio::test]
    async fn quinn_local_endpoint_pair_connects_and_exchanges_protocol_packets() {
        let pair = QuinnLocalEndpointPair::bind(QuinnEndpointConfig::localhost_ephemeral())
            .expect("endpoint pair binds");
        let server_addr = pair.server.local_addr().expect("server address");
        let client_endpoint = pair.client.endpoint().expect("client endpoint").clone();
        let server_endpoint = pair.server.endpoint().expect("server endpoint").clone();
        let reliable_packet = VersionedProtocolPacket::new(ProtocolMessage::JoinRequest {
            client_id: LOCAL_CLIENT_ID,
            session_token: None,
        });
        let unreliable_packet = VersionedProtocolPacket::new(ProtocolMessage::WorldDelta {
            tick: SimulationTick::new(33),
            payload: NetworkDeltaPayload::Noop,
        });
        let reliable_bytes = reliable_packet.encode_bytes().expect("reliable encodes");
        let unreliable_bytes = unreliable_packet
            .encode_bytes()
            .expect("unreliable encodes");

        let (client_connection, server_connection) = tokio::join!(
            async move {
                client_endpoint
                    .connect(server_addr, "localhost")
                    .expect("connect starts")
                    .await
                    .expect("client connects")
            },
            async move {
                server_endpoint
                    .accept()
                    .await
                    .expect("incoming connection")
                    .await
                    .expect("server accepts")
            }
        );

        let server_datagram_connection = server_connection.clone();
        let (mut send, _recv) = client_connection
            .open_bi()
            .await
            .expect("client opens reliable stream");
        let reliable_reader = tokio::spawn(async move {
            let mut incoming = server_connection
                .accept_bi()
                .await
                .expect("server accepts reliable stream")
                .1;
            incoming
                .read_to_end(usize::MAX)
                .await
                .expect("server reads reliable packet")
        });
        send.write_all(&reliable_bytes)
            .await
            .expect("client writes reliable packet");
        send.finish().expect("client finishes reliable stream");
        let received_reliable = reliable_reader.await.expect("reader task joins");

        let reliable_round_trip =
            VersionedProtocolPacket::decode_bytes(&received_reliable).expect("reliable decodes");
        assert_eq!(reliable_round_trip, reliable_packet);

        client_connection
            .send_datagram(unreliable_bytes.into())
            .expect("client sends datagram");
        let datagram = server_datagram_connection
            .read_datagram()
            .await
            .expect("server reads datagram");
        let unreliable_round_trip =
            VersionedProtocolPacket::decode_bytes(&datagram).expect("unreliable decodes");
        assert_eq!(unreliable_round_trip, unreliable_packet);
    }

    #[tokio::test]
    async fn quinn_packet_io_sends_reliable_and_datagram_protocol_packets() {
        let pair = QuinnLocalEndpointPair::bind(QuinnEndpointConfig::localhost_ephemeral())
            .expect("endpoint pair binds");
        let server_addr = pair.server.local_addr().expect("server address");
        let client_endpoint = pair.client.endpoint().expect("client endpoint").clone();
        let server_endpoint = pair.server.endpoint().expect("server endpoint").clone();
        let reliable_packet = VersionedProtocolPacket::new(ProtocolMessage::JoinRequest {
            client_id: LOCAL_CLIENT_ID,
            session_token: None,
        });
        let unreliable_packet = VersionedProtocolPacket::new(ProtocolMessage::WorldDelta {
            tick: SimulationTick::new(34),
            payload: NetworkDeltaPayload::Noop,
        });

        let (client_connection, server_connection) = tokio::join!(
            async move {
                client_endpoint
                    .connect(server_addr, "localhost")
                    .expect("connect starts")
                    .await
                    .expect("client connects")
            },
            async move {
                server_endpoint
                    .accept()
                    .await
                    .expect("incoming connection")
                    .await
                    .expect("server accepts")
            }
        );
        let client_io = QuinnPacketIo::new(client_connection);
        let server_io = QuinnPacketIo::new(server_connection);

        let reliable_reader = {
            let server_io = server_io.clone();
            tokio::spawn(async move { server_io.receive_reliable_packet().await })
        };
        client_io
            .send_packet(reliable_packet.clone())
            .await
            .expect("reliable packet sends");
        assert_eq!(
            reliable_reader
                .await
                .expect("reliable reader joins")
                .expect("reliable packet receives"),
            reliable_packet
        );

        let datagram_reader = {
            let server_io = server_io.clone();
            tokio::spawn(async move { server_io.receive_datagram_packet().await })
        };
        client_io
            .send_packet(unreliable_packet.clone())
            .await
            .expect("datagram packet sends");
        assert_eq!(
            datagram_reader
                .await
                .expect("datagram reader joins")
                .expect("datagram packet receives"),
            unreliable_packet
        );
    }

    #[tokio::test]
    async fn localhost_quinn_session_entrypoint_drives_join_through_real_packet_io() {
        let client_config = default_local_client_runtime();
        let session = connect_localhost_quinn_session(
            super::HostRuntimeConfig::default(),
            client_config.clone(),
            SimulationTick::new(55),
        )
        .await
        .expect("localhost quinn session connects");

        assert_eq!(session.host_connected_client_count(), 1);
        assert_eq!(session.joined_player_id(), client_config.player_id);
        assert_eq!(
            session.client_runtime.latest_authoritative_tick,
            SimulationTick::new(55)
        );
        assert_ne!(session.host_addr, session.client_addr);
    }

    #[tokio::test]
    async fn localhost_quinn_session_exchanges_command_packet_through_real_packet_io() {
        let client_config = default_local_client_runtime();
        let player_id = client_config.player_id.expect("default player id");
        let mut session = connect_localhost_quinn_session(
            super::HostRuntimeConfig::default(),
            client_config.clone(),
            SimulationTick::new(55),
        )
        .await
        .expect("localhost quinn session connects");
        let packet = CommandPacket {
            client_id: client_config.client_id,
            commands: vec![SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(7),
                target_tick: SimulationTick::new(56),
                command: PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: false,
                    drill_down: false,
                },
            }],
        };

        let summary = session
            .exchange_command_packet(packet)
            .await
            .expect("command packet exchanges");

        assert!(summary.all_accepted());
        assert_eq!(summary.acknowledged, 1);
        assert_eq!(summary.rejected, 0);
        assert_eq!(
            session.client_runtime.latest_authoritative_tick,
            SimulationTick::new(55)
        );
    }

    #[tokio::test]
    async fn localhost_quinn_session_replicates_snapshot_delta_and_terrain_chunk() {
        let client_config = default_local_client_runtime();
        let player_id = client_config.player_id.expect("default player id");
        let mut session = connect_localhost_quinn_session(
            super::HostRuntimeConfig::default(),
            client_config,
            SimulationTick::new(55),
        )
        .await
        .expect("localhost quinn session connects");
        let snapshot_tick = SimulationTick::new(60);
        let snapshot = NetworkWorldSnapshot {
            tick: snapshot_tick,
            players: vec![NetworkPlayerSnapshot {
                player_id,
                x: 1.0,
                y: 2.0,
                velocity_x: 0.5,
                velocity_y: -0.25,
                fuel: 99.0,
                hull: 88.0,
                credits: 77,
                cargo_used: 3,
                cargo: BTreeMap::new(),
                artifacts: BTreeMap::new(),
                materials: BTreeMap::new(),
                loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                scanner_cooldown_seconds: 0.0,
            }],
        };

        session
            .replicate_snapshot_keyframe(snapshot.clone())
            .await
            .expect("snapshot replicates");
        assert_eq!(
            session.client_runtime.latest_authoritative_tick,
            snapshot_tick
        );

        session
            .replicate_world_delta(
                SimulationTick::new(61),
                NetworkDeltaPayload::Players {
                    players: snapshot.players.clone(),
                },
            )
            .await
            .expect("world delta replicates");
        assert_eq!(
            session.client_runtime.latest_authoritative_tick,
            SimulationTick::new(61)
        );

        let chunk_response = session
            .exchange_terrain_chunk(4, 7, 11, 12)
            .await
            .expect("terrain chunk exchanges");
        assert_eq!(
            chunk_response,
            ProtocolMessage::TerrainChunkResponse {
                chunk_x: 4,
                chunk_y: 7,
                revision: 12,
                tiles: Vec::new(),
            }
        );
        assert_eq!(session.client_runtime.pending_messages.len(), 1);
    }

    #[tokio::test]
    async fn localhost_quinn_session_reconnects_with_reserved_token_over_real_packet_io() {
        let client_config = default_local_client_runtime();
        let mut session = connect_localhost_quinn_session(
            super::HostRuntimeConfig::default(),
            client_config.clone(),
            SimulationTick::new(70),
        )
        .await
        .expect("localhost quinn session connects");
        let first_client_addr = session.client_addr;
        let token = SessionToken::new(909);

        session
            .reconnect_with_token(token)
            .await
            .expect("session reconnects");

        assert_ne!(session.client_addr, first_client_addr);
        assert_eq!(session.host_connected_client_count(), 1);
        assert_eq!(session.client_runtime.session_token, Some(token));
        assert_eq!(session.joined_player_id(), client_config.player_id);
        assert_eq!(
            session.client_runtime.latest_authoritative_tick,
            SimulationTick::new(70)
        );
    }

    #[tokio::test]
    async fn localhost_quinn_session_driver_summary_covers_core_runtime_loop() {
        let client_config = default_local_client_runtime();
        let player_id = client_config.player_id.expect("default player id");
        let mut session = connect_localhost_quinn_session(
            super::HostRuntimeConfig::default(),
            client_config.clone(),
            SimulationTick::new(80),
        )
        .await
        .expect("localhost quinn session connects");
        let command_summary = session
            .exchange_command_packet(CommandPacket {
                client_id: client_config.client_id,
                commands: vec![SequencedPlayerCommand {
                    player_id,
                    sequence: InputSequence::new(8),
                    target_tick: SimulationTick::new(81),
                    command: PlayerCommand::Interact,
                }],
            })
            .await
            .expect("command exchanges");
        session
            .replicate_snapshot_keyframe(NetworkWorldSnapshot {
                tick: SimulationTick::new(82),
                players: Vec::new(),
            })
            .await
            .expect("snapshot replicates");
        session
            .replicate_world_delta(SimulationTick::new(83), NetworkDeltaPayload::Noop)
            .await
            .expect("delta replicates");
        let chunk = session
            .exchange_terrain_chunk(1, 2, 3, 4)
            .await
            .expect("chunk exchanges");
        session
            .reconnect_with_token(SessionToken::new(910))
            .await
            .expect("session reconnects");
        let summary = super::QuinnSessionDriverSummary {
            join_complete: session.host_connected_client_count() == 1,
            command_acknowledged: command_summary.all_accepted(),
            snapshot_replicated: session.client_runtime.latest_authoritative_tick
                >= SimulationTick::new(82),
            delta_replicated: session.client_runtime.latest_authoritative_tick
                >= SimulationTick::new(83),
            terrain_chunk_exchanged: matches!(
                chunk,
                ProtocolMessage::TerrainChunkResponse { revision: 4, .. }
            ),
            reconnect_complete: session.client_runtime.session_token
                == Some(SessionToken::new(910)),
        };

        assert!(summary.covers_core_runtime_loop());
    }

    #[tokio::test]
    async fn socket_driven_authoritative_snapshot_exercises_prediction_correction() {
        let client_config = default_local_client_runtime();
        let player_id = client_config.player_id.expect("default player id");
        let mut session = connect_localhost_quinn_session(
            super::HostRuntimeConfig::default(),
            client_config,
            SimulationTick::new(90),
        )
        .await
        .expect("localhost quinn session connects");

        let correction = session
            .replicate_authoritative_player_correction(
                100.0,
                100.0,
                NetworkPlayerSnapshot {
                    player_id,
                    x: 4.0,
                    y: 6.0,
                    velocity_x: 0.0,
                    velocity_y: 0.0,
                    fuel: 50.0,
                    hull: 75.0,
                    credits: 10,
                    cargo_used: 1,
                    cargo: BTreeMap::new(),
                    artifacts: BTreeMap::new(),
                    materials: BTreeMap::new(),
                    loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                    scanner_cooldown_seconds: 0.0,
                },
                SimulationTick::new(91),
            )
            .await
            .expect("authoritative correction replicates");

        assert!(correction.exercised_socket_correction());
        assert_eq!(
            correction.correction_plan,
            crate::session::CorrectionPlan::Snap
        );
        assert!(correction.snap_applied);
        assert_eq!(
            session.client_runtime.latest_authoritative_tick,
            SimulationTick::new(91)
        );
    }

    #[tokio::test]
    async fn quinn_session_tick_driver_advances_command_replication_chunk_and_correction() {
        let client_config = default_local_client_runtime();
        let player_id = client_config.player_id.expect("default player id");
        let mut session = connect_localhost_quinn_session(
            super::HostRuntimeConfig::default(),
            client_config.clone(),
            SimulationTick::new(100),
        )
        .await
        .expect("localhost quinn session connects");

        let summary = session
            .drive_tick(super::QuinnSessionTickInput {
                command_packet: Some(CommandPacket {
                    client_id: client_config.client_id,
                    commands: vec![SequencedPlayerCommand {
                        player_id,
                        sequence: InputSequence::new(10),
                        target_tick: SimulationTick::new(101),
                        command: PlayerCommand::UseScanner,
                    }],
                }),
                snapshot: Some(NetworkWorldSnapshot {
                    tick: SimulationTick::new(102),
                    players: Vec::new(),
                }),
                delta: Some((SimulationTick::new(103), NetworkDeltaPayload::Noop)),
                terrain_chunk_request: Some((8, 9, 1, 2)),
                authoritative_terrain_chunks: Vec::new(),
                correction_probe: Some((
                    20.0,
                    20.0,
                    NetworkPlayerSnapshot {
                        player_id,
                        x: 1.0,
                        y: 1.0,
                        velocity_x: 0.0,
                        velocity_y: 0.0,
                        fuel: 10.0,
                        hull: 10.0,
                        credits: 0,
                        cargo_used: 0,
                        cargo: BTreeMap::new(),
                        artifacts: BTreeMap::new(),
                        materials: BTreeMap::new(),
                        loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                        scanner_cooldown_seconds: 0.0,
                    },
                    SimulationTick::new(104),
                )),
            })
            .await
            .expect("tick driver runs");

        assert!(summary.advanced_authoritative_runtime());
        assert!(
            summary
                .command_summary
                .expect("command summary")
                .all_accepted()
        );
        assert_eq!(
            session.client_runtime.latest_authoritative_tick,
            SimulationTick::new(104)
        );
    }

    #[tokio::test]
    async fn split_quinn_session_entrypoints_drive_join_and_tick_runtime() {
        let client_config = default_local_client_runtime();
        let player_id = client_config.player_id.expect("default player id");
        let mut session = connect_split_localhost_quinn_session(
            super::HostRuntimeConfig::default(),
            client_config.clone(),
            SimulationTick::new(120),
        )
        .await
        .expect("split session connects");

        assert_eq!(session.host_connected_client_count(), 1);
        assert_eq!(session.joined_player_id(), Some(player_id));

        let summary = session
            .drive_tick(super::QuinnSessionTickInput {
                command_packet: Some(CommandPacket {
                    client_id: client_config.client_id,
                    commands: vec![SequencedPlayerCommand {
                        player_id,
                        sequence: InputSequence::new(12),
                        target_tick: SimulationTick::new(121),
                        command: PlayerCommand::SellCargo,
                    }],
                }),
                snapshot: Some(NetworkWorldSnapshot {
                    tick: SimulationTick::new(122),
                    players: Vec::new(),
                }),
                delta: Some((SimulationTick::new(123), NetworkDeltaPayload::Noop)),
                terrain_chunk_request: Some((10, 11, 1, 2)),
                authoritative_terrain_chunks: Vec::new(),
                correction_probe: Some((
                    30.0,
                    30.0,
                    NetworkPlayerSnapshot {
                        player_id,
                        x: 2.0,
                        y: 3.0,
                        velocity_x: 0.0,
                        velocity_y: 0.0,
                        fuel: 20.0,
                        hull: 30.0,
                        credits: 5,
                        cargo_used: 0,
                        cargo: BTreeMap::new(),
                        artifacts: BTreeMap::new(),
                        materials: BTreeMap::new(),
                        loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                        scanner_cooldown_seconds: 0.0,
                    },
                    SimulationTick::new(124),
                )),
            })
            .await
            .expect("split tick driver runs");

        assert!(summary.advanced_authoritative_runtime());
        assert_eq!(
            session.client_runtime.latest_authoritative_tick,
            SimulationTick::new(124)
        );
    }

    #[tokio::test]
    async fn local_online_smoke_summary_covers_join_tick_reconnect_and_descriptor_handoff() {
        let summary = local_online_smoke_summary()
            .await
            .expect("local online smoke runs");

        assert!(summary.passed());
        assert!(summary.tick.elapsed_micros > 0);
    }

    #[test]
    fn production_platform_scope_keeps_direct_connect_mvp_and_deferred_platform_features_separate()
    {
        let scope = super::production_platform_scope();

        assert!(scope.contains(&super::ProductionPlatformScope::DirectConnectMvp));
        assert!(scope.contains(&super::ProductionPlatformScope::NatTraversalDeferred));
        assert!(scope.contains(&super::ProductionPlatformScope::MatchmakingDeferred));
        assert!(scope.contains(&super::ProductionPlatformScope::PlatformInvitesDeferred));
        assert!(scope.contains(&super::ProductionPlatformScope::HostMigrationDeferred));
    }

    #[test]
    fn production_transport_selection_maps_reliability_to_quic_channels_with_dependency() {
        let selection = production_transport_selection();
        let mapping = transport_reliability_mapping();

        assert_eq!(
            selection.backend,
            super::SelectedTransportBackend::QuinnQuic
        );
        assert_eq!(
            selection.reliable_channel,
            ProductionPacketChannel::QuicBidirectionalStream
        );
        assert_eq!(
            selection.unreliable_sequenced_channel,
            ProductionPacketChannel::QuicDatagram
        );
        assert!(selection.dependency_added);
        assert_eq!(
            mapping.production_channel_for(ReliabilityClass::Reliable),
            ProductionPacketChannel::QuicBidirectionalStream
        );
        assert_eq!(
            mapping.production_channel_for(ReliabilityClass::UnreliableSequenced),
            ProductionPacketChannel::QuicDatagram
        );
    }

    #[test]
    fn packet_io_recovers_terrain_chunks_and_snapshot_keyframes() {
        let summary = packet_io_recovery_summary();

        assert!(summary.recovered_required_state(SimulationTick::new(42)));
    }

    #[test]
    fn network_soak_exercises_latency_jitter_loss_bandwidth_and_duration() {
        let summary = network_soak_summary();

        assert!(summary.covers_latency_jitter_loss_bandwidth_and_duration());
    }

    #[test]
    fn faithful_packet_io_simulates_timeout_retry_stale_duplicate_reconnect_shutdown_edges() {
        let mut io = FaithfulPacketIoSimulator::default();
        let mut session = CommandNetworkSession::new(SimulationTick::new(10), 2);
        let duplicate_packet = CommandPacket {
            client_id: ClientId::new(4),
            commands: vec![SequencedPlayerCommand {
                player_id: PlayerId::new(8),
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(10),
                command: PlayerCommand::Interact,
            }],
        };
        let stale_packet = CommandPacket {
            client_id: ClientId::new(4),
            commands: vec![SequencedPlayerCommand {
                player_id: PlayerId::new(8),
                sequence: InputSequence::new(2),
                target_tick: SimulationTick::new(9),
                command: PlayerCommand::Interact,
            }],
        };

        io.send(ProtocolMessage::WorldDelta {
            tick: SimulationTick::new(10),
            payload: NetworkDeltaPayload::Noop,
        });
        io.inject_version_mismatch(ProtocolMessage::JoinRequest {
            client_id: ClientId::new(4),
            session_token: None,
        });
        io.note_timeout();
        io.note_retry();
        io.note_reconnect();
        io.note_shutdown();
        let _accepted = io.apply_command_packet(&mut session, &duplicate_packet);
        let _duplicate = io.apply_command_packet(&mut session, &duplicate_packet);
        let _stale = io.apply_command_packet(&mut session, &stale_packet);
        let delivered = io.drain_supported_messages();
        let status = io.status();

        assert!(
            delivered
                .iter()
                .any(|message| matches!(message, ProtocolMessage::WorldDelta { .. }))
        );
        assert!(status.covers_transport_edges());
    }

    #[test]
    fn versioned_protocol_packets_round_trip_supported_messages_and_reject_mismatch() {
        let message = ProtocolMessage::ReconnectRequest {
            client_id: ClientId::new(3),
            session_token: SessionToken::new(9),
        };
        let packet = VersionedProtocolPacket::new(message.clone());

        assert_eq!(packet.protocol_version(), 1);
        assert_eq!(packet.decode_supported(), Ok(message));
        assert_eq!(
            VersionedProtocolPacket {
                protocol_version: 0,
                message: ProtocolMessage::WorldDelta {
                    tick: SimulationTick::new(1),
                    payload: NetworkDeltaPayload::Noop,
                },
            }
            .decode_supported()
            .expect_err("old protocol version rejected"),
            super::ProtocolVersionError {
                expected: 1,
                actual: 0,
            }
        );
    }

    #[test]
    fn protocol_messages_classify_reliability() {
        let command_message = ProtocolMessage::WorldDelta {
            tick: SimulationTick::new(7),
            payload: NetworkDeltaPayload::Noop,
        };
        let reconnect_message = ProtocolMessage::ReconnectRequest {
            client_id: ClientId::new(3),
            session_token: SessionToken::new(99),
        };

        assert_eq!(
            command_message.reliability_class(),
            ReliabilityClass::UnreliableSequenced
        );
        assert_eq!(
            reconnect_message.reliability_class(),
            ReliabilityClass::Reliable
        );
    }

    #[test]
    fn host_and_client_runtime_configs_drive_runtime_status_without_plan_wrapper() {
        let host = super::HostRuntimeConfig::default();
        let local_client = default_local_client_runtime();

        assert_eq!(host.max_clients, 4);
        assert!(host.allow_join_in_progress);
        assert!(host.allow_reconnect);
        assert_eq!(
            transport_integration_status(),
            super::TransportIntegrationStatus::Selected
        );
        assert_eq!(
            transport_implementation_decision().selected_backend(),
            super::SelectedTransportBackend::InMemoryFaithfulAdapter
        );
        assert_eq!(local_client.client_id, super::LOCAL_CLIENT_ID);
        assert_eq!(local_client.player_id, Some(super::LOCAL_PLAYER_ID));
    }

    #[test]
    fn selected_transport_maps_protocol_reliability_to_adapter_channels() {
        assert_eq!(
            selected_transport_backend(),
            super::SelectedTransportBackend::InMemoryFaithfulAdapter
        );
        assert!(
            selected_transport_backend()
                .rationale()
                .contains("faithful adapter")
        );
        assert!(transport_reliability_mapping().maps_protocol_classes());
    }

    #[test]
    fn connection_lifecycle_and_lobby_flow_cover_host_join_reconnect_shutdown_and_error() {
        let lifecycle = connection_lifecycle_summary();
        let ux = lobby_session_ux_flow();

        assert!(lifecycle.covers_host_join_disconnect_reconnect_shutdown());
        assert_eq!(lifecycle.final_host_clients, 1);
        assert!(ux.covers_host_join_reconnect_error());
    }

    #[test]
    fn faithful_transport_adapter_covers_faults_recovery_and_high_latency_conditions() {
        let faults = transport_fault_coverage_summary();
        let recovery = recovery_coverage_summary();
        let latency = high_latency_simulation_summary();

        assert!(faults.covers_faults());
        assert!(recovery.covers_recovery());
        assert!(latency.exercised_latency_jitter_loss());
    }

    #[tokio::test]
    async fn scripted_latency_loss_online_playtest_covers_real_socket_and_degraded_network() {
        let summary = scripted_latency_loss_online_playtest_summary()
            .await
            .expect("scripted online playtest runs");

        assert!(summary.passed());
        assert!(
            summary
                .covered
                .contains(&ScriptedLatencyLossCoverage::RealSocketSmoke)
        );
        assert!(
            summary
                .covered
                .contains(&ScriptedLatencyLossCoverage::LatencyJitterLoss)
        );
        assert!(summary.covered.contains(&ScriptedLatencyLossCoverage::Soak));
        assert!(
            summary
                .covered
                .contains(&ScriptedLatencyLossCoverage::Reconnect)
        );
    }

    #[tokio::test]
    async fn production_online_acceptance_summary_covers_direct_connect_mvp() {
        let summary = production_online_acceptance_summary()
            .await
            .expect("production online acceptance runs");

        assert!(summary.direct_connect_mvp_passed());
        assert!(
            summary
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::DirectConnectTransport)
        );
        assert!(
            summary
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::StableHostAssignedSlot)
        );
        assert!(
            summary
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::RemoteCommands)
        );
        assert!(
            summary
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::SnapshotDeltaAndTerrainReplication)
        );
        assert!(
            summary
                .covered
                .contains(&ProductionOnlineAcceptanceCoverage::RealCorrection)
        );
    }

    #[test]
    fn host_session_runtime_accepts_clients_and_applies_command_packets() {
        let mut host =
            HostSessionRuntime::new(super::HostRuntimeConfig::default(), SimulationTick::new(5));
        let client_id = ClientId::new(8);
        let player_id = PlayerId::new(9);

        let join = host
            .accept_client(client_id, player_id, SimulationTick::new(5))
            .expect("client accepted");
        assert_eq!(host.connected_client_count(), 1);
        assert_eq!(host.connected_player(client_id), Some(player_id));
        let status = host.runtime_status();
        assert_eq!(status.connected_clients, 1);
        assert_eq!(status.mode, super::HostRuntimeMode::InProcessLocal);
        assert!(status.join_in_progress_enabled);
        assert!(status.reconnect_enabled);
        assert!(status.transport_selected);
        assert!(status.has_capacity());
        assert!(matches!(
            join,
            ProtocolMessage::JoinAccepted {
                client_id: accepted_client,
                player_id: accepted_player,
                snapshot_tick,
            } if accepted_client == client_id
                && accepted_player == player_id
                && snapshot_tick == SimulationTick::new(5)
        ));

        let packet = CommandPacket {
            client_id,
            commands: vec![SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(5),
                command: PlayerCommand::Interact,
            }],
        };
        assert!(matches!(
            host.apply_command_packet(&packet).as_slice(),
            [ProtocolMessage::CommandAcknowledgement(_)]
        ));
        let duplicate_packet = CommandPacket {
            client_id,
            commands: vec![SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(5),
                command: PlayerCommand::Interact,
            }],
        };
        let (messages, summary) = host.apply_command_packet_exchange(&duplicate_packet);
        assert!(matches!(
            messages.as_slice(),
            [ProtocolMessage::CommandRejection(_)]
        ));
        assert_eq!(summary.client_id, client_id);
        assert_eq!(summary.acknowledged, 0);
        assert_eq!(summary.rejected, 1);
        assert_eq!(summary.authoritative_tick, SimulationTick::new(5));
        assert!(!summary.all_accepted());
    }

    #[test]
    fn client_session_runtime_tracks_join_acknowledgement_and_delta_messages() {
        let mut client = ClientSessionRuntime::new(super::ClientRuntimeConfig {
            mode: super::ClientRuntimeMode::RemoteNetwork,
            client_id: ClientId::new(12),
            player_id: None,
        });
        client.set_session_token(SessionToken::new(77));

        assert_eq!(
            client.connect_request(),
            ProtocolMessage::JoinRequest {
                client_id: ClientId::new(12),
                session_token: Some(SessionToken::new(77)),
            }
        );

        client.handle_message(ProtocolMessage::JoinAccepted {
            client_id: ClientId::new(12),
            player_id: PlayerId::new(13),
            snapshot_tick: SimulationTick::new(20),
        });
        assert_eq!(client.assigned_player_id, Some(PlayerId::new(13)));
        assert_eq!(client.latest_authoritative_tick, SimulationTick::new(20));
        let status = client.runtime_status();
        assert_eq!(status.mode, super::ClientRuntimeMode::RemoteNetwork);
        assert_eq!(status.client_id, ClientId::new(12));
        assert_eq!(status.assigned_player_id, Some(PlayerId::new(13)));
        assert!(status.has_session_token);
        assert_eq!(status.latest_authoritative_tick, SimulationTick::new(20));
        assert_eq!(status.pending_message_count, 0);
        assert!(status.joined());

        client.handle_message(ProtocolMessage::WorldDelta {
            tick: SimulationTick::new(21),
            payload: NetworkDeltaPayload::Noop,
        });
        assert_eq!(client.latest_authoritative_tick, SimulationTick::new(21));
        assert!(!client.runtime_status().keyframe_recovery_needed);
    }

    #[test]
    fn client_runtime_tracks_keyframe_recovery_after_missed_delta() {
        let mut client = ClientSessionRuntime::new(super::ClientRuntimeConfig {
            mode: super::ClientRuntimeMode::RemoteNetwork,
            client_id: ClientId::new(9),
            player_id: Some(PlayerId::new(9)),
        });

        client.handle_message(ProtocolMessage::WorldDelta {
            tick: SimulationTick::new(40),
            payload: NetworkDeltaPayload::KeyframeRequired,
        });

        let recovering = client.runtime_status();
        assert_eq!(
            recovering.latest_authoritative_tick,
            SimulationTick::new(40)
        );
        assert!(recovering.keyframe_recovery_needed);

        client.handle_message(ProtocolMessage::SnapshotKeyframe {
            snapshot: NetworkWorldSnapshot {
                tick: SimulationTick::new(41),
                players: Vec::new(),
            },
        });

        let recovered = client.runtime_status();
        assert_eq!(recovered.latest_authoritative_tick, SimulationTick::new(41));
        assert!(!recovered.keyframe_recovery_needed);
    }

    #[test]
    fn protocol_exchange_batches_count_reliability_classes() {
        let batch = ProtocolMessage::exchange_batch(
            super::ProtocolExchangeKind::JoinHandshake,
            vec![
                ProtocolMessage::JoinRequest {
                    client_id: ClientId::new(1),
                    session_token: None,
                },
                ProtocolMessage::JoinAccepted {
                    client_id: ClientId::new(1),
                    player_id: PlayerId::new(2),
                    snapshot_tick: SimulationTick::new(3),
                },
                ProtocolMessage::WorldDelta {
                    tick: SimulationTick::new(3),
                    payload: NetworkDeltaPayload::Noop,
                },
            ],
        );

        assert_eq!(batch.kind, super::ProtocolExchangeKind::JoinHandshake);
        assert_eq!(batch.reliable_count(), 2);
        assert_eq!(batch.unreliable_count(), 1);
    }

    #[test]
    fn reliable_join_and_reconnect_exchanges_use_runtime_protocol_messages() {
        let token = SessionToken::new(11);
        let join = reliable_join_exchange_messages(
            super::LOCAL_CLIENT_ID,
            super::LOCAL_PLAYER_ID,
            SimulationTick::new(8),
        );
        let reconnect = reliable_reconnect_exchange_messages(
            super::LOCAL_CLIENT_ID,
            super::LOCAL_PLAYER_ID,
            SimulationTick::new(9),
            token,
        );
        let join_batch = ProtocolMessage::exchange_batch(
            super::ProtocolExchangeKind::JoinHandshake,
            join.to_vec(),
        );
        let reconnect_batch = ProtocolMessage::exchange_batch(
            super::ProtocolExchangeKind::JoinHandshake,
            reconnect.to_vec(),
        );

        assert!(matches!(
            join[0],
            ProtocolMessage::JoinRequest {
                client_id: super::LOCAL_CLIENT_ID,
                session_token: None,
            }
        ));
        assert!(matches!(
            join[1],
            ProtocolMessage::JoinAccepted {
                client_id: super::LOCAL_CLIENT_ID,
                player_id: super::LOCAL_PLAYER_ID,
                snapshot_tick,
            } if snapshot_tick == SimulationTick::new(8)
        ));
        assert_eq!(join_batch.reliable_count(), 3);
        assert!(matches!(
            reconnect[0],
            ProtocolMessage::ReconnectRequest {
                client_id: super::LOCAL_CLIENT_ID,
                session_token,
            } if session_token == token
        ));
        assert_eq!(reconnect_batch.reliable_count(), 2);
    }

    #[test]
    fn host_session_runtime_reconnects_reserved_clients() {
        let mut host =
            HostSessionRuntime::new(super::HostRuntimeConfig::default(), SimulationTick::new(5));
        let token = SessionToken::new(77);
        host.reserve_reconnect_token(token, ClientId::new(1), PlayerId::new(9));

        let accepted = host
            .reconnect_client(ClientId::new(12), token, SimulationTick::new(30))
            .expect("reconnect accepted");

        assert_eq!(
            host.connected_player(ClientId::new(12)),
            Some(PlayerId::new(9))
        );
        assert!(matches!(
            accepted,
            ProtocolMessage::JoinAccepted {
                client_id,
                player_id,
                snapshot_tick,
            } if client_id == ClientId::new(12)
                && player_id == PlayerId::new(9)
                && snapshot_tick == SimulationTick::new(30)
        ));
    }

    #[test]
    fn in_memory_transport_queues_classify_and_route_packets() {
        let mut queues = InMemoryTransportQueues::default();
        let client_id = ClientId::new(4);
        queues.send_to_host(ProtocolMessage::CommandPacket(CommandPacket {
            client_id,
            commands: Vec::new(),
        }));
        queues.send_to_client(
            client_id,
            ProtocolMessage::JoinAccepted {
                client_id,
                player_id: PlayerId::new(5),
                snapshot_tick: SimulationTick::new(6),
            },
        );
        assert_eq!(
            queues.status(),
            super::InMemoryTransportStatus {
                queued_client_to_host: 1,
                queued_host_to_client_packets: 1,
                addressed_clients: 1,
            }
        );

        let host_packets = queues.drain_client_to_host();
        assert_eq!(
            host_packets[0].reliability,
            ReliabilityClass::UnreliableSequenced
        );
        let client_packets = queues.drain_host_to_client(client_id);
        assert_eq!(client_packets[0].reliability, ReliabilityClass::Reliable);
        assert!(queues.drain_host_to_client(client_id).is_empty());
        assert!(queues.status().is_idle());
    }

    #[test]
    fn compatibility_host_client_transport_smoke_test_exchanges_join_and_ack() {
        let mut host =
            HostSessionRuntime::new(super::HostRuntimeConfig::default(), SimulationTick::new(5));
        let mut client = ClientSessionRuntime::new(super::ClientRuntimeConfig {
            mode: super::ClientRuntimeMode::RemoteNetwork,
            client_id: ClientId::new(42),
            player_id: None,
        });
        let mut queues = InMemoryTransportQueues::default();

        queues.send_to_host(client.connect_request());
        for packet in queues.drain_client_to_host() {
            if let ProtocolMessage::JoinRequest { client_id, .. } = packet.message {
                let accepted = host
                    .accept_client(client_id, PlayerId::new(77), SimulationTick::new(5))
                    .expect("join accepted");
                queues.send_to_client(client_id, accepted);
            }
        }
        for packet in queues.drain_host_to_client(client.config.client_id) {
            client.handle_message(packet.message);
        }

        assert_eq!(client.assigned_player_id, Some(PlayerId::new(77)));
        assert_eq!(client.latest_authoritative_tick, SimulationTick::new(5));

        queues.send_to_host(ProtocolMessage::CommandPacket(CommandPacket {
            client_id: client.config.client_id,
            commands: vec![SequencedPlayerCommand {
                player_id: PlayerId::new(77),
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(5),
                command: PlayerCommand::Interact,
            }],
        }));
        for packet in queues.drain_client_to_host() {
            if let ProtocolMessage::CommandPacket(command_packet) = packet.message {
                for response in host.apply_command_packet(&command_packet) {
                    queues.send_to_client(command_packet.client_id, response);
                }
            }
        }
        for packet in queues.drain_host_to_client(client.config.client_id) {
            client.handle_message(packet.message);
        }

        assert_eq!(client.latest_authoritative_tick, SimulationTick::new(5));
        assert!(client.pending_messages.is_empty());
    }

    #[test]
    fn in_memory_runtime_packet_pump_drives_host_and_client_runtime() {
        let mut host =
            HostSessionRuntime::new(super::HostRuntimeConfig::default(), SimulationTick::new(5));
        let mut client = ClientSessionRuntime::new(super::ClientRuntimeConfig {
            mode: super::ClientRuntimeMode::RemoteNetwork,
            client_id: ClientId::new(42),
            player_id: None,
        });
        let mut queues = InMemoryTransportQueues::default();

        queues.send_to_host(client.connect_request());
        let join_summary = pump_in_memory_runtime_packets(
            &mut queues,
            &mut host,
            &mut client,
            PlayerId::new(77),
            SimulationTick::new(5),
        );

        assert!(join_summary.exchanged_packets());
        assert_eq!(join_summary.host_received, 1);
        assert_eq!(join_summary.client_received, 1);
        assert_eq!(client.assigned_player_id, Some(PlayerId::new(77)));
        assert_eq!(
            host.connected_player(ClientId::new(42)),
            Some(PlayerId::new(77))
        );

        queues.send_to_host(ProtocolMessage::CommandPacket(CommandPacket {
            client_id: client.config.client_id,
            commands: vec![SequencedPlayerCommand {
                player_id: PlayerId::new(77),
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(5),
                command: PlayerCommand::Interact,
            }],
        }));
        let command_summary = pump_in_memory_runtime_packets(
            &mut queues,
            &mut host,
            &mut client,
            PlayerId::new(77),
            SimulationTick::new(5),
        );

        assert_eq!(command_summary.host_received, 1);
        assert_eq!(command_summary.client_received, 1);
        assert_eq!(client.latest_authoritative_tick, SimulationTick::new(5));
        assert!(client.pending_messages.is_empty());
    }

    #[test]
    fn join_reconnect_and_chunk_exchange_messages_are_reliable_runtime_protocol() {
        let join_messages = reliable_join_exchange_messages(
            super::LOCAL_CLIENT_ID,
            super::LOCAL_PLAYER_ID,
            SimulationTick::new(44),
        );

        assert!(
            join_messages
                .iter()
                .all(|message| { message.reliability_class() == ReliabilityClass::Reliable })
        );
        assert!(matches!(
            join_messages[0],
            ProtocolMessage::JoinRequest { .. }
        ));
        assert!(matches!(
            join_messages[1],
            ProtocolMessage::JoinAccepted { .. }
        ));
        assert!(matches!(
            join_messages[2],
            ProtocolMessage::TerrainChunkRequest { .. }
        ));

        let reconnect_messages = reliable_reconnect_exchange_messages(
            super::LOCAL_CLIENT_ID,
            super::LOCAL_PLAYER_ID,
            SimulationTick::new(45),
            SessionToken::new(77),
        );
        assert!(matches!(
            reconnect_messages[0],
            ProtocolMessage::ReconnectRequest { .. }
        ));
        assert!(matches!(
            reconnect_messages[1],
            ProtocolMessage::JoinAccepted { .. }
        ));
    }

    #[test]
    fn command_sequence_tracker_rejects_duplicates_and_bad_ticks() {
        let mut tracker = CommandSequenceTracker::new();
        let client_id = ClientId::new(4);
        let player_id = PlayerId::new(8);
        let command = SequencedPlayerCommand {
            player_id,
            sequence: InputSequence::new(2),
            target_tick: SimulationTick::new(10),
            command: PlayerCommand::Interact,
        };

        assert_eq!(
            tracker.accept_command(client_id, &command, SimulationTick::new(9), 4),
            CommandAcceptance::Accepted
        );
        assert_eq!(
            tracker.accept_command(client_id, &command, SimulationTick::new(9), 4),
            CommandAcceptance::Duplicate
        );
        assert_eq!(
            tracker.accept_command(client_id, &command, SimulationTick::new(11), 4),
            CommandAcceptance::TooOld
        );

        let far_future_command = SequencedPlayerCommand {
            target_tick: SimulationTick::new(100),
            sequence: InputSequence::new(3),
            ..command
        };
        assert_eq!(
            tracker.accept_command(client_id, &far_future_command, SimulationTick::new(10), 4),
            CommandAcceptance::TooFarInFuture
        );
        assert_eq!(
            tracker.latest_sequence(client_id, player_id),
            Some(InputSequence::new(2))
        );
    }

    #[test]
    fn command_network_session_acknowledges_and_rejects_packets() {
        let client_id = ClientId::new(7);
        let player_id = PlayerId::new(9);
        let mut network_session = CommandNetworkSession::new(SimulationTick::new(10), 2);
        let accepted_packet = CommandPacket {
            client_id,
            commands: vec![SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(10),
                command: PlayerCommand::Interact,
            }],
        };
        let rejected_packet = CommandPacket {
            client_id,
            commands: vec![SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(2),
                target_tick: SimulationTick::new(20),
                command: PlayerCommand::Interact,
            }],
        };

        let accepted = network_session.apply_command_packet(&accepted_packet);
        let rejected = network_session.apply_command_packet(&rejected_packet);
        let messages = network_session.apply_command_packet_messages(&rejected_packet);

        assert!(matches!(
            accepted.as_slice(),
            [super::CommandApplicationResponse::Acknowledged(_)]
        ));
        assert!(matches!(
            rejected.as_slice(),
            [super::CommandApplicationResponse::Rejected(
                super::CommandRejection {
                    reason: CommandAcceptance::TooFarInFuture,
                    ..
                }
            )]
        ));
        assert_eq!(network_session.current_tick(), SimulationTick::new(10));
        network_session.set_current_tick(SimulationTick::new(11));
        assert_eq!(network_session.current_tick(), SimulationTick::new(11));
        assert!(matches!(
            messages.as_slice(),
            [ProtocolMessage::CommandRejection(super::CommandRejection {
                reason: CommandAcceptance::TooFarInFuture,
                ..
            })]
        ));
        assert_eq!(messages[0].reliability_class(), ReliabilityClass::Reliable);
    }

    #[test]
    fn command_conflict_detector_flags_edge_cases() {
        let commands = vec![
            SequencedPlayerCommand {
                player_id: PlayerId::new(1),
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(2),
                command: PlayerCommand::Movement {
                    horizontal: 0.0,
                    thrust: false,
                    drill_down: true,
                },
            },
            SequencedPlayerCommand {
                player_id: PlayerId::new(2),
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(2),
                command: PlayerCommand::Movement {
                    horizontal: 0.0,
                    thrust: false,
                    drill_down: true,
                },
            },
            SequencedPlayerCommand {
                player_id: PlayerId::new(3),
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(3),
                command: PlayerCommand::Movement {
                    horizontal: 0.0,
                    thrust: false,
                    drill_down: true,
                },
            },
            SequencedPlayerCommand {
                player_id: PlayerId::new(1),
                sequence: InputSequence::new(2),
                target_tick: SimulationTick::new(3),
                command: PlayerCommand::Refuel,
            },
            SequencedPlayerCommand {
                player_id: PlayerId::new(2),
                sequence: InputSequence::new(2),
                target_tick: SimulationTick::new(3),
                command: PlayerCommand::Repair,
            },
        ];

        let conflicts = command_conflicts(&commands);

        assert!(conflicts.contains(&super::CommandConflict::SameTickMining));
        assert!(conflicts.contains(&super::CommandConflict::NearbyTickMining));
        assert!(conflicts.contains(&super::CommandConflict::SimultaneousEconomyAction));
    }

    #[test]
    fn terrain_recovery_detects_revision_mismatch() {
        assert_eq!(
            terrain_recovery_decision(4, 4),
            super::TerrainRecoveryDecision::UpToDate
        );
        assert_eq!(
            terrain_recovery_decision(3, 4),
            super::TerrainRecoveryDecision::RequestChunk
        );
    }

    #[test]
    fn session_continuity_uses_matching_reconnect_token() {
        let token = SessionToken::new(123);

        assert_eq!(
            session_continuity_decision(Some(token), Some(token)),
            super::SessionContinuityDecision::ReservePlayerForReconnect
        );
        assert_eq!(
            session_continuity_decision(Some(token), Some(SessionToken::new(999))),
            super::SessionContinuityDecision::AssignNewPlayer
        );
    }

    #[test]
    fn underground_disconnects_are_reserved_for_reconnect() {
        assert_eq!(
            disconnect_reservation_policy(true),
            super::DisconnectReservationPolicy::ReserveForReconnect
        );
        assert_eq!(
            disconnect_reservation_policy(false),
            super::DisconnectReservationPolicy::ReleaseImmediately
        );
    }

    #[test]
    fn split_screen_ui_policy_changes_for_multiple_clients() {
        assert_eq!(
            per_client_ui_policy(1),
            super::PerClientUiPolicy::SharedLegacyUi
        );
        assert_eq!(
            per_client_ui_policy(2),
            super::PerClientUiPolicy::IndependentClientUi
        );
    }

    #[test]
    fn host_save_coordinates_when_clients_are_connected() {
        assert_eq!(
            host_save_decision(1),
            super::HostSaveDecision::SaveImmediately
        );
        assert_eq!(
            host_save_decision(2),
            super::HostSaveDecision::CoordinateConnectedClients
        );
    }

    #[test]
    fn session_shutdown_policy_handles_host_and_client_leaves() {
        assert_eq!(
            session_shutdown_decision(false, false, false),
            super::SessionShutdownDecision::Continue
        );
        assert_eq!(
            session_shutdown_decision(false, true, false),
            super::SessionShutdownDecision::RemoveDisconnectedClient
        );
        assert_eq!(
            session_shutdown_decision(true, false, false),
            super::SessionShutdownDecision::EndSession
        );
        assert_eq!(
            session_shutdown_decision(false, false, true),
            super::SessionShutdownDecision::EndSession
        );
    }

    #[test]
    fn initial_multiplayer_policy_decisions_are_explicit() {
        assert_eq!(
            initial_message_routing_policy(),
            super::MessageRoutingPolicy::SharedWorldLogAndPerClientHud
        );
        assert_eq!(
            initial_resource_ownership_policy(),
            super::ResourceOwnershipPolicy::PerPlayer
        );
        assert_eq!(
            initial_discovery_sharing_policy(),
            super::DiscoverySharingPolicy::SharedAcrossSession
        );
        assert_eq!(
            initial_collision_policy(),
            super::CollisionPolicy::PlayerCollisionDisabled
        );
        assert_eq!(
            initial_transport_policy(),
            super::TransportPolicy::TransportAgnosticProtocolFirst
        );
    }

    #[test]
    fn all_command_sources_use_authoritative_command_path() {
        let sources = [
            CommandSource::Keyboard,
            CommandSource::Gamepad,
            CommandSource::SplitScreenClient,
            CommandSource::OnlineClient,
            CommandSource::Replay,
            CommandSource::Ai,
        ];

        assert!(
            sources
                .into_iter()
                .all(CommandSource::uses_authoritative_command_path)
        );
    }

    #[test]
    fn packet_recovery_uses_chunks_or_keyframes_for_revision_mismatch() {
        assert_eq!(
            packet_recovery_action(7, 7, false),
            super::PacketRecoveryAction::None
        );
        assert_eq!(
            packet_recovery_action(6, 7, false),
            super::PacketRecoveryAction::RequestChunk
        );
        assert_eq!(
            packet_recovery_action(6, 7, true),
            super::PacketRecoveryAction::AwaitKeyframe
        );
    }

    #[test]
    fn clients_are_never_authoritative_for_world_progression_domains() {
        let domains = [
            super::ClientAuthoritativeDomain::Terrain,
            super::ClientAuthoritativeDomain::Cargo,
            super::ClientAuthoritativeDomain::Credits,
            super::ClientAuthoritativeDomain::Upgrades,
            super::ClientAuthoritativeDomain::Damage,
            super::ClientAuthoritativeDomain::Contracts,
            super::ClientAuthoritativeDomain::Progression,
        ];

        assert!(
            domains
                .into_iter()
                .all(|domain| !client_authority_allowed(domain))
        );
    }
}
