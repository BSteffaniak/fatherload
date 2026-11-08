use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    game_state::{
        GameState, OnlineSessionBoundaryStatus, OnlineSessionUxState, RealOnlineSessionUxSnapshot,
        RunMode,
    },
    player::Player,
    terrain::TilePosition,
};

#[allow(
    dead_code,
    reason = "real online controller is exercised by tests until desktop async UI ownership calls it"
)]
pub enum RealOnlineSessionMode {
    CombinedLocalhost(crate::multiplayer::QuinnOnlineSession),
    DescriptorHostPending {
        listener: crate::multiplayer::QuinnHostListener,
        descriptor_path: PathBuf,
        descriptor: crate::multiplayer::QuinnHostConnectionDescriptor,
        lan_descriptor_server: Option<crate::lan_discovery::LanDescriptorServer>,
        lan_publisher: Option<crate::lan_discovery::LanDiscoveryPublisher>,
    },
    DescriptorHostAccepted {
        host_runtime: crate::multiplayer::HostSessionRuntime,
        host_io: crate::multiplayer::QuinnPacketIo,
        descriptor_path: PathBuf,
        descriptor: crate::multiplayer::QuinnHostConnectionDescriptor,
        lan_descriptor_server: Option<crate::lan_discovery::LanDescriptorServer>,
        lan_publisher: Option<crate::lan_discovery::LanDiscoveryPublisher>,
    },
    DescriptorClientConnected {
        client_runtime: crate::multiplayer::ClientSessionRuntime,
        connector: crate::multiplayer::QuinnClientConnector,
        packet_io: crate::multiplayer::QuinnPacketIo,
        descriptor_path: PathBuf,
        descriptor: crate::multiplayer::QuinnHostConnectionDescriptor,
    },
}

impl RealOnlineSessionMode {
    const fn session_mut(&mut self) -> Option<&mut crate::multiplayer::QuinnOnlineSession> {
        match self {
            Self::CombinedLocalhost(session) => Some(session),
            Self::DescriptorHostPending { .. }
            | Self::DescriptorHostAccepted { .. }
            | Self::DescriptorClientConnected { .. } => None,
        }
    }

    const fn label(&self) -> &'static str {
        match self {
            Self::CombinedLocalhost(_) => "combined-localhost",
            Self::DescriptorHostPending { .. } => "descriptor-host-pending",
            Self::DescriptorHostAccepted { .. } => "descriptor-host-accepted",
            Self::DescriptorClientConnected { .. } => "descriptor-client-connected",
        }
    }
}

pub fn write_online_descriptor_file(
    path: &Path,
    json: &str,
) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            crate::multiplayer::QuinnOnlineSessionError::Accept(error.to_string())
        })?;
    }
    std::fs::write(path, json)
        .map_err(|error| crate::multiplayer::QuinnOnlineSessionError::Accept(error.to_string()))
}

type PendingPlayerDelta = (
    crate::multiplayer::SimulationTick,
    Vec<crate::multiplayer::NetworkPlayerSnapshot>,
);
type PendingTerrainChunk = (i32, i32, u64, Vec<crate::multiplayer::NetworkTerrainTile>);
type ReplicatedWorldUpdates = (
    Vec<crate::multiplayer::NetworkWorldSnapshot>,
    Vec<PendingPlayerDelta>,
    Vec<PendingTerrainChunk>,
);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OnlineSessionEvent {
    RemoteIdentity {
        player_id: crate::multiplayer::PlayerId,
        name: String,
    },
    RemoteReady {
        player_id: crate::multiplayer::PlayerId,
        ready: bool,
    },
    StartSession(crate::multiplayer::SimulationTick),
    ReplicationStatus(String),
    ReplicatedPlayerStatus(String),
    KeyframeRequired,
    HostEnded(String),
}

pub struct RealOnlineSessionController {
    mode: RealOnlineSessionMode,
    player_slot: Option<u8>,
    next_sequence: u32,
    next_tick: u64,
    pending_command_acknowledgements: Vec<crate::multiplayer::CommandAcknowledgement>,
    pending_command_rejections: Vec<crate::multiplayer::CommandRejection>,
    pending_world_snapshots: Vec<crate::multiplayer::NetworkWorldSnapshot>,
    pending_player_deltas: Vec<PendingPlayerDelta>,
    pending_terrain_chunks: Vec<PendingTerrainChunk>,
    pending_events: Vec<OnlineSessionEvent>,
    remote_shutdown: bool,
    typed_epoch: crate::multiplayer::SessionEpoch,
    typed_keyframe: Option<crate::multiplayer::AuthoritativeKeyframe>,
}

#[allow(
    dead_code,
    reason = "real online controller is exercised by tests until desktop async UI ownership calls it"
)]
impl RealOnlineSessionController {
    /// Connect a localhost split Quinn session and apply joined UX state.
    ///
    /// # Errors
    ///
    /// Returns an error when Quinn endpoint setup, connect/accept, or join handshake fails.
    pub async fn connect_localhost(
        game: &mut GameState,
    ) -> Result<Self, crate::multiplayer::QuinnOnlineSessionError> {
        let client_config = crate::multiplayer::default_local_client_runtime();
        let session = crate::multiplayer::connect_split_localhost_quinn_session(
            crate::multiplayer::HostRuntimeConfig::default(),
            client_config,
            crate::multiplayer::SimulationTick::new(300),
        )
        .await?;
        let controller = Self {
            mode: RealOnlineSessionMode::CombinedLocalhost(session),
            player_slot: Some(1),
            next_sequence: 30,
            next_tick: 301,
            pending_command_acknowledgements: Vec::new(),
            pending_command_rejections: Vec::new(),
            pending_world_snapshots: Vec::new(),
            pending_player_deltas: Vec::new(),
            pending_terrain_chunks: Vec::new(),
            pending_events: Vec::new(),
            remote_shutdown: false,
            typed_epoch: crate::multiplayer::SessionEpoch::new(1),
            typed_keyframe: None,
        };
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot::from_joined_session(
            controller.player_slot,
        ));
        Ok(controller)
    }

    pub fn host_descriptor_file_pending(
        game: &mut GameState,
        path: &Path,
    ) -> Result<Self, crate::multiplayer::QuinnOnlineSessionError> {
        Self::host_descriptor_file_pending_with_lan(game, path, false)
    }

    pub fn host_lan_game_pending(
        game: &mut GameState,
        path: &Path,
    ) -> Result<Self, crate::multiplayer::QuinnOnlineSessionError> {
        Self::host_descriptor_file_pending_with_lan(game, path, true)
    }

    fn host_descriptor_file_pending_with_lan(
        game: &mut GameState,
        path: &Path,
        publish_lan: bool,
    ) -> Result<Self, crate::multiplayer::QuinnOnlineSessionError> {
        let listener = crate::multiplayer::QuinnHostListener::bind_localhost(
            crate::multiplayer::QuinnEndpointConfig {
                bind_addr: game.online_host_bind_addr,
            },
        )?;
        let mut descriptor = listener.connection_descriptor()?;
        if !publish_lan && game.online_host_advertise_addr.port() != 0 {
            descriptor.host_addr = game.online_host_advertise_addr;
        }
        if publish_lan
            && (descriptor.host_addr.ip().is_loopback()
                || descriptor.host_addr.ip().is_unspecified())
            && let Some(lan_ip) = crate::lan_discovery::likely_lan_ip()
        {
            descriptor.host_addr = SocketAddr::new(lan_ip, descriptor.host_addr.port());
        }
        let json = serde_json::to_string(&descriptor).map_err(|error| {
            crate::multiplayer::QuinnOnlineSessionError::Accept(error.to_string())
        })?;
        write_online_descriptor_file(path, &json)?;
        let (lan_descriptor_server, lan_publisher) = if publish_lan {
            let descriptor_server = crate::lan_discovery::LanDescriptorServer::serve(
                crate::lan_discovery::localhost_descriptor_bind_addr(),
                &descriptor,
            )
            .map_err(|error| {
                crate::multiplayer::QuinnOnlineSessionError::Accept(format!(
                    "LAN descriptor server failed: {error}"
                ))
            })?;
            let descriptor_addr = SocketAddr::new(
                descriptor.host_addr.ip(),
                descriptor_server.local_addr().port(),
            );
            let advertisement =
                crate::lan_discovery::LanGameAdvertisement::from_descriptor(&descriptor)
                    .with_descriptor_addr(descriptor_addr);
            game.online_session_status_message = format!(
                "LAN mDNS host ready as `{}` at {}; descriptor endpoint {}; waiting for client.",
                advertisement.instance_name, descriptor.host_addr, descriptor_addr
            );
            let publisher = crate::lan_discovery::LanDiscoveryPublisher::publish(&advertisement)
                .map_err(|error| {
                    crate::multiplayer::QuinnOnlineSessionError::Accept(format!(
                        "LAN mDNS publish failed: {error}"
                    ))
                })?;
            (Some(descriptor_server), Some(publisher))
        } else {
            (None, None)
        };
        let controller = Self {
            mode: RealOnlineSessionMode::DescriptorHostPending {
                listener,
                descriptor_path: path.to_path_buf(),
                descriptor,
                lan_descriptor_server,
                lan_publisher,
            },
            player_slot: Some(1),
            next_sequence: 30,
            next_tick: 301,
            pending_command_acknowledgements: Vec::new(),
            pending_command_rejections: Vec::new(),
            pending_world_snapshots: Vec::new(),
            pending_player_deltas: Vec::new(),
            pending_terrain_chunks: Vec::new(),
            pending_events: Vec::new(),
            remote_shutdown: false,
            typed_epoch: crate::multiplayer::SessionEpoch::new(1),
            typed_keyframe: None,
        };
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot::from_host_descriptor_ready(
            Some(1),
            path,
        ));
        Ok(controller)
    }

    pub async fn connect_descriptor_client(
        game: &mut GameState,
        path: &Path,
    ) -> Result<Self, crate::multiplayer::QuinnOnlineSessionError> {
        let json = std::fs::read_to_string(path).map_err(|error| {
            crate::multiplayer::QuinnOnlineSessionError::Connect(format!(
                "descriptor file {} could not be read: {error}",
                path.display()
            ))
        })?;
        let descriptor: crate::multiplayer::QuinnHostConnectionDescriptor =
            serde_json::from_str(&json).map_err(|error| {
                crate::multiplayer::QuinnOnlineSessionError::Connect(format!(
                    "descriptor file {} could not be parsed: {error}",
                    path.display()
                ))
            })?;
        let client_bind_config = if descriptor.host_addr.ip().is_loopback() {
            crate::multiplayer::QuinnEndpointConfig::localhost_ephemeral()
        } else {
            crate::multiplayer::QuinnEndpointConfig::any_ipv4_ephemeral()
        };
        let connector = crate::multiplayer::QuinnClientConnector::bind_from_host_descriptor(
            client_bind_config,
            &descriptor,
        )?;
        let packet_io = connector
            .connect_packet_io(descriptor.host_addr, &descriptor.server_name)
            .await?;
        let client_config = crate::multiplayer::default_local_client_runtime();
        let client_id = client_config.client_id;
        let mut client_runtime = crate::multiplayer::ClientSessionRuntime::new(client_config);
        packet_io
            .send_client_packet(crate::multiplayer::ClientPacket::new(
                crate::multiplayer::ClientMessage::JoinRequest {
                    client_id,
                    session_token: None,
                },
            ))
            .await?;
        let accepted_packet = packet_io.receive_reliable_server_packet().await?;
        let (player_id, epoch, keyframe) = match accepted_packet.message {
            crate::multiplayer::ServerMessage::JoinAccepted {
                client_id: accepted_client,
                player_id,
                epoch,
                keyframe,
            } if accepted_client == client_id => (player_id, epoch, keyframe),
            other => {
                return Err(crate::multiplayer::QuinnOnlineSessionError::Connect(
                    format!("unexpected typed join response: {other:?}"),
                ));
            }
        };
        client_runtime.assigned_player_id = Some(player_id);
        client_runtime.latest_authoritative_tick = keyframe.snapshot.tick;
        let controller = Self {
            mode: RealOnlineSessionMode::DescriptorClientConnected {
                client_runtime,
                connector,
                packet_io,
                descriptor_path: path.to_path_buf(),
                descriptor,
            },
            player_slot: Some(2),
            next_sequence: 30,
            next_tick: 301,
            pending_command_acknowledgements: Vec::new(),
            pending_command_rejections: Vec::new(),
            pending_world_snapshots: Vec::new(),
            pending_player_deltas: Vec::new(),
            pending_terrain_chunks: Vec::new(),
            pending_events: Vec::new(),
            remote_shutdown: false,
            typed_epoch: epoch,
            typed_keyframe: Some(keyframe),
        };
        game.apply_real_online_session_ux(
            RealOnlineSessionUxSnapshot::from_descriptor_client_connected(
                controller.player_slot,
                path,
            ),
        );
        Ok(controller)
    }

    pub async fn accept_descriptor_client(
        &mut self,
        game: &mut GameState,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorHostPending {
            listener,
            descriptor_path,
            descriptor,
            lan_descriptor_server,
            lan_publisher,
        } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "pending descriptor host listener",
                ),
            );
        };
        let host_io = listener.accept_packet_io().await?;
        let mut host_runtime = crate::multiplayer::HostSessionRuntime::new(
            crate::multiplayer::HostRuntimeConfig::default(),
            crate::multiplayer::SimulationTick::new(300),
        );
        let world = <crate::session::WorldState as crate::session::LegacyWorldStateAdapter>::from_legacy_game(game);
        let epoch = self.typed_epoch;
        let keyframe = crate::multiplayer::AuthoritativeKeyframe {
            epoch,
            snapshot: crate::session::WorldSnapshot::from_world(world.simulation_tick(), &world)
                .network_snapshot(),
            canonical_state: serde_json::to_vec(&world)
                .expect("typed join canonical world encodes"),
            state_hash: Some(world.simulation_state_hash().get()),
        };
        let join_packet = host_io.receive_reliable_client_packet().await?;
        self.typed_keyframe = Some(keyframe.clone());
        let join_response = match join_packet.message {
            crate::multiplayer::ClientMessage::JoinRequest {
                client_id,
                session_token: None,
            } => {
                if host_runtime
                    .accept_client(
                        client_id,
                        crate::multiplayer::PlayerId::new(2),
                        keyframe.snapshot.tick,
                    )
                    .is_none()
                {
                    None
                } else {
                    Some(crate::multiplayer::ServerMessage::JoinAccepted {
                        client_id,
                        player_id: crate::multiplayer::PlayerId::new(2),
                        epoch,
                        keyframe: keyframe.clone(),
                    })
                }
            }
            crate::multiplayer::ClientMessage::JoinRequest {
                client_id,
                session_token: Some(session_token),
            } => host_runtime.reconnect_client_typed(client_id, session_token, epoch, keyframe),
            other => {
                return Err(crate::multiplayer::QuinnOnlineSessionError::Connect(
                    format!("unexpected typed join request: {other:?}"),
                ));
            }
        }
        .ok_or(crate::multiplayer::QuinnOnlineSessionError::JoinRejected)?;
        host_io
            .send_server_packet(crate::multiplayer::ServerPacket::new(join_response))
            .await?;
        let descriptor_path = descriptor_path.clone();
        let descriptor = descriptor.clone();
        let lan_descriptor_server = lan_descriptor_server.take();
        let lan_publisher = lan_publisher.take();
        self.mode = RealOnlineSessionMode::DescriptorHostAccepted {
            host_runtime,
            host_io,
            descriptor_path,
            descriptor,
            lan_descriptor_server,
            lan_publisher,
        };
        game.apply_real_online_session_ux(
            RealOnlineSessionUxSnapshot::from_descriptor_host_accepted(self.player_slot),
        );
        Ok(())
    }

    pub async fn drive_descriptor_host_outbound_tick(
        &mut self,
        game: &mut GameState,
        input: crate::multiplayer::QuinnSessionTickInput,
    ) -> Result<
        crate::multiplayer::QuinnSessionTickSummary,
        crate::multiplayer::QuinnOnlineSessionError,
    > {
        let command_summary = self
            .descriptor_host_try_receive_command_packet(
                game,
                Duration::from_millis(1),
                input
                    .snapshot
                    .as_ref()
                    .map_or(crate::multiplayer::SimulationTick::new(0), |snapshot| {
                        snapshot.tick
                    }),
                &input.authoritative_terrain_chunks,
            )
            .await?;
        self.drive_descriptor_host_replication_tick(game, input, command_summary)
            .await
    }

    pub async fn drive_descriptor_host_replication_tick(
        &mut self,
        game: &mut GameState,
        input: crate::multiplayer::QuinnSessionTickInput,
        command_summary: Option<crate::multiplayer::CommandPacketExchangeSummary>,
    ) -> Result<
        crate::multiplayer::QuinnSessionTickSummary,
        crate::multiplayer::QuinnOnlineSessionError,
    > {
        self.descriptor_host_send_player_identity(&game.online_player_name)
            .await?;
        self.descriptor_host_send_ready_state(game.online_local_ready)
            .await?;
        if game.run_mode == RunMode::Playing
            && game.online_local_ready
            && game.online_remote_player_ready
        {
            let authoritative_tick = input
                .snapshot
                .as_ref()
                .map_or(crate::multiplayer::SimulationTick::new(0), |snapshot| {
                    snapshot.tick
                });
            self.descriptor_host_send_start_session(authoritative_tick)
                .await?;
        }
        let snapshot_replicated = if let Some(snapshot) = input.snapshot {
            self.descriptor_host_send_snapshot(snapshot).await?;
            true
        } else {
            false
        };
        let delta_replicated = if let Some((tick, payload)) = input.delta {
            self.descriptor_host_send_world_delta(tick, payload).await?;
            true
        } else {
            false
        };
        if delta_replicated {
            game.apply_online_replication_status("host sent world delta");
        } else if snapshot_replicated {
            game.apply_online_replication_status("host sent snapshot keyframe");
        }
        let summary = crate::multiplayer::QuinnSessionTickSummary {
            command_summary,
            snapshot_replicated,
            delta_replicated,
            terrain_chunk_response: None,
            correction_summary: None,
        };
        if self.player_slot.is_some() {
            game.online_player_slot = self.player_slot;
        }
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_remote_player_connected = true;
        if game.online_remote_player_name.is_none() {
            game.online_remote_player_name = Some("Remote miner".to_owned());
        }
        game.refresh_online_runtime_statuses();
        Ok(summary)
    }

    pub async fn drive_descriptor_client_outbound_tick(
        &mut self,
        input: crate::multiplayer::QuinnSessionTickInput,
        player_name: &str,
        local_ready: bool,
    ) -> Result<
        crate::multiplayer::QuinnSessionTickSummary,
        crate::multiplayer::QuinnOnlineSessionError,
    > {
        let _received_messages = self
            .descriptor_client_try_receive_pending_messages(Duration::from_millis(1))
            .await?;
        let summary = crate::multiplayer::QuinnSessionTickSummary {
            command_summary: None,
            snapshot_replicated: false,
            delta_replicated: false,
            terrain_chunk_response: None,
            correction_summary: None,
        };
        if self.remote_shutdown {
            return Ok(summary);
        }
        let command_sent = if let Some(packet) = input.command_packet {
            self.descriptor_client_send_command_packet_unacknowledged(packet)
                .await?;
            true
        } else {
            false
        };
        let terrain_requested = if let Some((chunk_x, chunk_y, known_revision, _desired_revision)) =
            input.terrain_chunk_request
        {
            self.descriptor_client_send_terrain_chunk_request_unacknowledged(
                chunk_x,
                chunk_y,
                known_revision,
            )
            .await?;
            true
        } else {
            false
        };
        self.descriptor_client_send_player_identity(player_name)
            .await?;
        self.descriptor_client_send_ready_state(local_ready).await?;
        let _ = (command_sent, terrain_requested);
        Ok(summary)
    }

    pub async fn descriptor_host_send_player_identity(
        &mut self,
        name: &str,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorHostAccepted { host_io, .. } = &mut self.mode else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "accepted descriptor host packet io",
                ),
            );
        };
        host_io
            .send_server_packet(crate::multiplayer::ServerPacket::new(
                crate::multiplayer::ServerMessage::PlayerIdentity {
                    epoch: self.typed_epoch,
                    player_id: crate::multiplayer::PlayerId::new(1),
                    name: name.to_owned(),
                },
            ))
            .await?;
        Ok(())
    }

    pub async fn descriptor_host_send_ready_state(
        &mut self,
        ready: bool,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorHostAccepted { host_io, .. } = &mut self.mode else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "accepted descriptor host packet io",
                ),
            );
        };
        host_io
            .send_server_packet(crate::multiplayer::ServerPacket::new(
                crate::multiplayer::ServerMessage::ReadyState {
                    epoch: self.typed_epoch,
                    player_id: crate::multiplayer::PlayerId::new(1),
                    ready,
                },
            ))
            .await?;
        Ok(())
    }

    pub async fn descriptor_host_send_start_session(
        &mut self,
        authoritative_tick: crate::multiplayer::SimulationTick,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorHostAccepted { host_io, .. } = &mut self.mode else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "accepted descriptor host packet io",
                ),
            );
        };
        host_io
            .send_server_packet(crate::multiplayer::ServerPacket::new(
                crate::multiplayer::ServerMessage::StartSession {
                    epoch: self.typed_epoch,
                    authoritative_tick,
                },
            ))
            .await?;
        Ok(())
    }

    pub async fn descriptor_host_try_receive_ready_state(
        &mut self,
        game: &mut GameState,
        timeout: Duration,
        authoritative_terrain_chunks: &[crate::multiplayer::NetworkTerrainChunkSnapshot],
    ) -> Result<bool, crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorHostAccepted { host_io, .. } = &mut self.mode else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "accepted descriptor host packet io",
                ),
            );
        };
        let mut received_any = false;
        for _ in 0..4 {
            let received =
                match tokio::time::timeout(timeout, host_io.receive_reliable_client_packet()).await
                {
                    Ok(Ok(packet)) => packet,
                    Ok(Err(error)) => return Err(error.into()),
                    Err(_) => break,
                };
            match received.message {
                crate::multiplayer::ClientMessage::PlayerIdentity {
                    epoch,
                    player_id,
                    name,
                } if epoch == self.typed_epoch => {
                    game.apply_online_remote_identity(player_id, &name);
                    received_any = true;
                }
                crate::multiplayer::ClientMessage::ReadyState {
                    epoch,
                    player_id,
                    ready,
                } if epoch == self.typed_epoch => {
                    game.apply_online_remote_ready_state(player_id, ready);
                    received_any = true;
                }
                crate::multiplayer::ClientMessage::TerrainChunkRequest {
                    epoch,
                    chunk_x,
                    chunk_y,
                    known_revision,
                } if epoch == self.typed_epoch => {
                    let chunk = authoritative_terrain_chunks
                        .iter()
                        .find(|chunk| chunk.chunk_x == chunk_x && chunk.chunk_y == chunk_y)
                        .or_else(|| {
                            authoritative_terrain_chunks
                                .iter()
                                .find(|chunk| chunk.revision > known_revision)
                        })
                        .or_else(|| authoritative_terrain_chunks.first());
                    let (response_chunk_x, response_chunk_y, revision, tiles) = chunk.map_or_else(
                        || {
                            let revision = known_revision.saturating_add(1);
                            let tiles =
                                network_terrain_chunk_tiles(&game.terrain, chunk_x, chunk_y);
                            (chunk_x, chunk_y, revision, tiles)
                        },
                        |chunk| {
                            (
                                chunk.chunk_x,
                                chunk.chunk_y,
                                chunk.revision.max(known_revision.saturating_add(1)),
                                chunk.tiles.clone(),
                            )
                        },
                    );
                    host_io
                        .send_server_packet(crate::multiplayer::ServerPacket::new(
                            crate::multiplayer::ServerMessage::TerrainChunk {
                                epoch: self.typed_epoch,
                                chunk_x: response_chunk_x,
                                chunk_y: response_chunk_y,
                                revision,
                                tiles,
                            },
                        ))
                        .await?;
                    game.apply_online_terrain_status(format!(
                        "answered chunk ({response_chunk_x},{response_chunk_y}) rev {revision}"
                    ));
                    received_any = true;
                }
                crate::multiplayer::ClientMessage::Disconnect {
                    epoch,
                    client_id: _,
                } if epoch == self.typed_epoch => {
                    game.apply_online_session_boundary_status(
                        &OnlineSessionBoundaryStatus::client_left("client disconnected"),
                    );
                    received_any = true;
                }
                other => {
                    return Err(crate::multiplayer::QuinnOnlineSessionError::Connect(
                        format!("unexpected typed client control message: {other:?}"),
                    ));
                }
            }
        }
        Ok(received_any)
    }

    pub async fn descriptor_host_send_session_ended(
        &mut self,
        reason: &str,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorHostAccepted { host_io, .. } = &mut self.mode else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "accepted descriptor host packet io",
                ),
            );
        };
        host_io
            .send_server_packet(crate::multiplayer::ServerPacket::new(
                crate::multiplayer::ServerMessage::SessionEnded {
                    epoch: self.typed_epoch,
                    reason: reason.to_owned(),
                },
            ))
            .await?;
        Ok(())
    }

    pub async fn descriptor_client_send_player_identity(
        &mut self,
        name: &str,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorClientConnected { packet_io, .. } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "connected descriptor client packet io",
                ),
            );
        };
        packet_io
            .send_client_packet(crate::multiplayer::ClientPacket::new(
                crate::multiplayer::ClientMessage::PlayerIdentity {
                    epoch: self.typed_epoch,
                    player_id: crate::multiplayer::PlayerId::new(2),
                    name: name.to_owned(),
                },
            ))
            .await?;
        Ok(())
    }

    pub async fn descriptor_client_send_ready_state(
        &mut self,
        ready: bool,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorClientConnected { packet_io, .. } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "connected descriptor client packet io",
                ),
            );
        };
        packet_io
            .send_client_packet(crate::multiplayer::ClientPacket::new(
                crate::multiplayer::ClientMessage::ReadyState {
                    epoch: self.typed_epoch,
                    player_id: crate::multiplayer::PlayerId::new(2),
                    ready,
                },
            ))
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub async fn descriptor_host_try_receive_command_packet(
        &mut self,
        game: &mut GameState,
        timeout: Duration,
        current_tick: crate::multiplayer::SimulationTick,
        authoritative_terrain_chunks: &[crate::multiplayer::NetworkTerrainChunkSnapshot],
    ) -> Result<
        Option<crate::multiplayer::CommandPacketExchangeSummary>,
        crate::multiplayer::QuinnOnlineSessionError,
    > {
        let RealOnlineSessionMode::DescriptorHostAccepted {
            host_runtime,
            host_io,
            ..
        } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "accepted descriptor host packet io",
                ),
            );
        };
        host_runtime.set_current_tick(current_tick);
        let mut merged_summary: Option<crate::multiplayer::CommandPacketExchangeSummary> = None;
        for _ in 0..32 {
            let received =
                match tokio::time::timeout(timeout, host_io.receive_reliable_client_packet()).await
                {
                    Ok(Ok(packet)) => packet,
                    Ok(Err(error)) => return Err(error.into()),
                    Err(_) => break,
                };
            match received.message {
                crate::multiplayer::ClientMessage::CommandPacket { epoch, packet }
                    if epoch == self.typed_epoch =>
                {
                    let (responses, summary) = host_runtime.apply_command_packet_exchange(&packet);
                    for response in responses {
                        let message = match response {
                            crate::multiplayer::ProtocolMessage::CommandAcknowledgement(
                                acknowledgement,
                            ) => crate::multiplayer::ServerMessage::CommandAcknowledgement {
                                epoch: self.typed_epoch,
                                acknowledgement,
                            },
                            crate::multiplayer::ProtocolMessage::CommandRejection(rejection) => {
                                crate::multiplayer::ServerMessage::CommandRejection {
                                    epoch: self.typed_epoch,
                                    rejection,
                                }
                            }
                            other => {
                                return Err(
                                    crate::multiplayer::QuinnOnlineSessionError::UnexpectedMessage(
                                        other,
                                    ),
                                );
                            }
                        };
                        host_io
                            .send_server_packet(crate::multiplayer::ServerPacket::new(message))
                            .await?;
                    }
                    if let Some(merged) = &mut merged_summary {
                        merged.acknowledged += summary.acknowledged;
                        merged.rejected += summary.rejected;
                        merged.authoritative_tick = current_tick;
                        merged.accepted_commands.extend(summary.accepted_commands);
                    } else {
                        merged_summary = Some(summary);
                    }
                }
                crate::multiplayer::ClientMessage::PlayerIdentity {
                    epoch,
                    player_id,
                    name,
                } if epoch == self.typed_epoch => {
                    game.apply_online_remote_identity(player_id, &name);
                }
                crate::multiplayer::ClientMessage::ReadyState {
                    epoch,
                    player_id,
                    ready,
                } if epoch == self.typed_epoch => {
                    game.apply_online_remote_ready_state(player_id, ready);
                }
                crate::multiplayer::ClientMessage::TerrainChunkRequest {
                    epoch,
                    chunk_x,
                    chunk_y,
                    known_revision,
                } if epoch == self.typed_epoch => {
                    let chunk = authoritative_terrain_chunks
                        .iter()
                        .find(|chunk| chunk.chunk_x == chunk_x && chunk.chunk_y == chunk_y)
                        .or_else(|| {
                            authoritative_terrain_chunks
                                .iter()
                                .find(|chunk| chunk.revision > known_revision)
                        })
                        .or_else(|| authoritative_terrain_chunks.first());
                    let (response_chunk_x, response_chunk_y, revision, tiles) = chunk.map_or_else(
                        || {
                            let revision = known_revision.saturating_add(1);
                            let tiles =
                                network_terrain_chunk_tiles(&game.terrain, chunk_x, chunk_y);
                            (chunk_x, chunk_y, revision, tiles)
                        },
                        |chunk| {
                            (
                                chunk.chunk_x,
                                chunk.chunk_y,
                                chunk.revision.max(known_revision.saturating_add(1)),
                                chunk.tiles.clone(),
                            )
                        },
                    );
                    host_io
                        .send_server_packet(crate::multiplayer::ServerPacket::new(
                            crate::multiplayer::ServerMessage::TerrainChunk {
                                epoch: self.typed_epoch,
                                chunk_x: response_chunk_x,
                                chunk_y: response_chunk_y,
                                revision,
                                tiles,
                            },
                        ))
                        .await?;
                    game.apply_online_terrain_status(format!(
                        "answered chunk ({response_chunk_x},{response_chunk_y}) rev {revision}"
                    ));
                }
                crate::multiplayer::ClientMessage::Disconnect {
                    epoch,
                    client_id: _,
                } if epoch == self.typed_epoch => {
                    game.apply_online_session_boundary_status(
                        &OnlineSessionBoundaryStatus::client_left("client disconnected"),
                    );
                }
                other => {
                    return Err(crate::multiplayer::QuinnOnlineSessionError::Connect(
                        format!("unexpected typed client message: {other:?}"),
                    ));
                }
            }
        }
        Ok(merged_summary)
    }

    pub async fn descriptor_client_send_session_ended(
        &mut self,
        reason: &str,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorClientConnected { packet_io, .. } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "connected descriptor client packet io",
                ),
            );
        };
        let _ = reason;
        packet_io
            .send_client_packet(crate::multiplayer::ClientPacket::new(
                crate::multiplayer::ClientMessage::Disconnect {
                    epoch: self.typed_epoch,
                    client_id: crate::multiplayer::ClientId::new(2),
                },
            ))
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub async fn descriptor_client_try_receive_pending_messages(
        &mut self,
        timeout: Duration,
    ) -> Result<usize, crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorClientConnected {
            client_runtime,
            packet_io,
            ..
        } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "connected descriptor client packet io",
                ),
            );
        };
        let mut received_count = 0;
        if self.remote_shutdown {
            return Ok(received_count);
        }
        for _ in 0..40 {
            match tokio::time::timeout(timeout, packet_io.receive_reliable_server_packet()).await {
                Ok(Ok(packet)) => {
                    if packet.message.epoch() != self.typed_epoch {
                        return Err(crate::multiplayer::QuinnOnlineSessionError::Connect(
                            "received stale typed server message".to_owned(),
                        ));
                    }
                    match packet.message {
                        crate::multiplayer::ServerMessage::PlayerIdentity {
                            player_id,
                            name,
                            ..
                        } => self
                            .pending_events
                            .push(OnlineSessionEvent::RemoteIdentity { player_id, name }),
                        crate::multiplayer::ServerMessage::ReadyState {
                            player_id, ready, ..
                        } => self
                            .pending_events
                            .push(OnlineSessionEvent::RemoteReady { player_id, ready }),
                        crate::multiplayer::ServerMessage::StartSession {
                            authoritative_tick,
                            ..
                        } => {
                            client_runtime.latest_authoritative_tick = authoritative_tick;
                            self.pending_events
                                .push(OnlineSessionEvent::StartSession(authoritative_tick));
                        }
                        crate::multiplayer::ServerMessage::CommandAcknowledgement {
                            acknowledgement,
                            ..
                        } => {
                            client_runtime.latest_authoritative_tick =
                                acknowledgement.authoritative_tick;
                            self.pending_command_acknowledgements.push(acknowledgement);
                        }
                        crate::multiplayer::ServerMessage::CommandRejection {
                            rejection, ..
                        } => {
                            client_runtime.latest_authoritative_tick = rejection.authoritative_tick;
                            self.pending_command_rejections.push(rejection);
                        }
                        crate::multiplayer::ServerMessage::TerrainChunk {
                            chunk_x,
                            chunk_y,
                            revision,
                            tiles,
                            ..
                        } => self
                            .pending_terrain_chunks
                            .push((chunk_x, chunk_y, revision, tiles)),
                        crate::multiplayer::ServerMessage::SnapshotKeyframe {
                            snapshot, ..
                        } => {
                            let tick = snapshot.tick;
                            client_runtime.latest_authoritative_tick = tick;
                            self.pending_world_snapshots.push(snapshot);
                            self.pending_events
                                .push(OnlineSessionEvent::ReplicationStatus(format!(
                                    "snapshot keyframe tick {} received",
                                    tick.get()
                                )));
                        }
                        crate::multiplayer::ServerMessage::WorldDelta { tick, payload, .. } => {
                            client_runtime.latest_authoritative_tick = tick;
                            let status = match payload {
                                crate::multiplayer::NetworkDeltaPayload::Noop => {
                                    format!("tick {}: noop delta", tick.get())
                                }
                                crate::multiplayer::NetworkDeltaPayload::TerrainChunks {
                                    revisions,
                                } => format!(
                                    "tick {}: {} terrain chunk revision(s)",
                                    tick.get(),
                                    revisions.len()
                                ),
                                crate::multiplayer::NetworkDeltaPayload::Players { players } => {
                                    let count = players.len();
                                    self.pending_player_deltas.push((tick, players));
                                    format!(
                                        "tick {}: {count} authoritative player update(s)",
                                        tick.get()
                                    )
                                }
                                crate::multiplayer::NetworkDeltaPayload::KeyframeRequired => {
                                    client_runtime.keyframe_recovery_needed = true;
                                    self.pending_events
                                        .push(OnlineSessionEvent::KeyframeRequired);
                                    format!("tick {}: keyframe required", tick.get())
                                }
                            };
                            self.pending_events
                                .push(OnlineSessionEvent::ReplicationStatus(format!(
                                    "world delta tick {} received",
                                    tick.get()
                                )));
                            self.pending_events
                                .push(OnlineSessionEvent::ReplicatedPlayerStatus(status));
                        }
                        crate::multiplayer::ServerMessage::SessionEnded { reason, .. } => {
                            self.remote_shutdown = true;
                            self.pending_events
                                .push(OnlineSessionEvent::HostEnded(reason));
                        }
                        other => {
                            return Err(crate::multiplayer::QuinnOnlineSessionError::Connect(
                                format!("unexpected typed server message: {other:?}"),
                            ));
                        }
                    }
                    received_count += 1;
                }
                Ok(Err(error)) => {
                    self.remote_shutdown = true;
                    self.pending_events
                        .push(OnlineSessionEvent::HostEnded(format!(
                            "transport closed ({error:?})"
                        )));
                    break;
                }
                Err(_) => break,
            }
        }
        Ok(received_count)
    }

    pub async fn descriptor_client_send_terrain_chunk_request_unacknowledged(
        &mut self,
        chunk_x: i32,
        chunk_y: i32,
        known_revision: u64,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorClientConnected { packet_io, .. } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "connected descriptor client packet io",
                ),
            );
        };
        packet_io
            .send_client_packet(crate::multiplayer::ClientPacket::new(
                crate::multiplayer::ClientMessage::TerrainChunkRequest {
                    epoch: self.typed_epoch,
                    chunk_x,
                    chunk_y,
                    known_revision,
                },
            ))
            .await?;
        Ok(())
    }

    pub async fn descriptor_client_send_command_packet_unacknowledged(
        &mut self,
        packet: crate::multiplayer::CommandPacket,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorClientConnected { packet_io, .. } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "connected descriptor client packet io",
                ),
            );
        };
        packet_io
            .send_client_packet(crate::multiplayer::ClientPacket::new(
                crate::multiplayer::ClientMessage::CommandPacket {
                    epoch: self.typed_epoch,
                    packet,
                },
            ))
            .await?;
        Ok(())
    }

    pub async fn descriptor_host_send_snapshot(
        &mut self,
        snapshot: crate::multiplayer::NetworkWorldSnapshot,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorHostAccepted { host_io, .. } = &mut self.mode else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "accepted descriptor host packet io",
                ),
            );
        };
        host_io
            .send_server_packet(crate::multiplayer::ServerPacket::new(
                crate::multiplayer::ServerMessage::SnapshotKeyframe {
                    epoch: self.typed_epoch,
                    snapshot,
                },
            ))
            .await?;
        Ok(())
    }

    pub async fn descriptor_host_send_world_delta(
        &mut self,
        tick: crate::multiplayer::SimulationTick,
        payload: crate::multiplayer::NetworkDeltaPayload,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorHostAccepted { host_io, .. } = &mut self.mode else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "accepted descriptor host packet io",
                ),
            );
        };
        host_io
            .send_server_packet(crate::multiplayer::ServerPacket::new(
                crate::multiplayer::ServerMessage::WorldDelta {
                    epoch: self.typed_epoch,
                    tick,
                    payload,
                },
            ))
            .await?;
        Ok(())
    }

    pub async fn descriptor_client_receive_replication(
        &mut self,
    ) -> Result<crate::multiplayer::ProtocolMessage, crate::multiplayer::QuinnOnlineSessionError>
    {
        let RealOnlineSessionMode::DescriptorClientConnected {
            client_runtime,
            packet_io,
            ..
        } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "connected descriptor client packet io",
                ),
            );
        };
        let message = match packet_io.receive_reliable_server_packet().await?.message {
            crate::multiplayer::ServerMessage::SnapshotKeyframe { epoch: _, snapshot } => {
                crate::multiplayer::ProtocolMessage::SnapshotKeyframe { snapshot }
            }
            crate::multiplayer::ServerMessage::WorldDelta {
                epoch: _,
                tick,
                payload,
            } => crate::multiplayer::ProtocolMessage::WorldDelta { tick, payload },
            other => {
                return Err(crate::multiplayer::QuinnOnlineSessionError::Connect(
                    format!("unexpected typed replication message: {other:?}"),
                ));
            }
        };
        match message {
            crate::multiplayer::ProtocolMessage::SnapshotKeyframe { .. }
            | crate::multiplayer::ProtocolMessage::WorldDelta { .. } => {
                client_runtime.handle_message(message.clone());
                Ok(message)
            }
            other => Err(crate::multiplayer::QuinnOnlineSessionError::UnexpectedMessage(other)),
        }
    }

    pub async fn descriptor_host_answer_terrain_request(
        &mut self,
        response_revision: u64,
    ) -> Result<crate::multiplayer::ProtocolMessage, crate::multiplayer::QuinnOnlineSessionError>
    {
        let RealOnlineSessionMode::DescriptorHostAccepted { host_io, .. } = &mut self.mode else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "accepted descriptor host packet io",
                ),
            );
        };
        let request = host_io
            .receive_reliable_packet()
            .await?
            .decode_supported()
            .map_err(|error| {
                crate::multiplayer::QuinnOnlineSessionError::Connect(format!(
                    "unsupported protocol version: expected {}, actual {}",
                    error.expected, error.actual
                ))
            })?;
        let response = match request {
            crate::multiplayer::ProtocolMessage::TerrainChunkRequest {
                chunk_x,
                chunk_y,
                known_revision: _,
            } => crate::multiplayer::ProtocolMessage::TerrainChunkResponse {
                chunk_x,
                chunk_y,
                revision: response_revision,
                tiles: Vec::new(),
            },
            other => {
                return Err(crate::multiplayer::QuinnOnlineSessionError::UnexpectedMessage(other));
            }
        };
        host_io
            .send_packet(crate::multiplayer::VersionedProtocolPacket::new(
                response.clone(),
            ))
            .await?;
        Ok(response)
    }

    pub async fn descriptor_client_request_terrain_chunk(
        &mut self,
        chunk_x: i32,
        chunk_y: i32,
        known_revision: u64,
    ) -> Result<crate::multiplayer::ProtocolMessage, crate::multiplayer::QuinnOnlineSessionError>
    {
        let RealOnlineSessionMode::DescriptorClientConnected {
            client_runtime,
            packet_io,
            ..
        } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "connected descriptor client packet io",
                ),
            );
        };
        packet_io
            .send_packet(crate::multiplayer::VersionedProtocolPacket::new(
                crate::multiplayer::ProtocolMessage::TerrainChunkRequest {
                    chunk_x,
                    chunk_y,
                    known_revision,
                },
            ))
            .await?;
        let response = packet_io
            .receive_reliable_packet()
            .await?
            .decode_supported()
            .map_err(|error| {
                crate::multiplayer::QuinnOnlineSessionError::Connect(format!(
                    "unsupported protocol version: expected {}, actual {}",
                    error.expected, error.actual
                ))
            })?;
        match response {
            crate::multiplayer::ProtocolMessage::TerrainChunkResponse { .. } => {
                client_runtime.handle_message(response.clone());
                Ok(response)
            }
            other => Err(crate::multiplayer::QuinnOnlineSessionError::UnexpectedMessage(other)),
        }
    }

    pub async fn descriptor_host_receive_command_packet(
        &mut self,
    ) -> Result<
        crate::multiplayer::CommandPacketExchangeSummary,
        crate::multiplayer::QuinnOnlineSessionError,
    > {
        let RealOnlineSessionMode::DescriptorHostAccepted {
            host_runtime,
            host_io,
            ..
        } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "accepted descriptor host packet io",
                ),
            );
        };
        let received = host_io.receive_reliable_client_packet().await?;
        let packet = match received.message {
            crate::multiplayer::ClientMessage::CommandPacket { epoch, packet }
                if epoch == self.typed_epoch =>
            {
                packet
            }
            other => {
                return Err(crate::multiplayer::QuinnOnlineSessionError::Connect(
                    format!("unexpected typed command message: {other:?}"),
                ));
            }
        };
        let (responses, summary) = host_runtime.apply_command_packet_exchange(&packet);
        for response in responses {
            let message = match response {
                crate::multiplayer::ProtocolMessage::CommandAcknowledgement(acknowledgement) => {
                    crate::multiplayer::ServerMessage::CommandAcknowledgement {
                        epoch: self.typed_epoch,
                        acknowledgement,
                    }
                }
                crate::multiplayer::ProtocolMessage::CommandRejection(rejection) => {
                    crate::multiplayer::ServerMessage::CommandRejection {
                        epoch: self.typed_epoch,
                        rejection,
                    }
                }
                other => {
                    return Err(
                        crate::multiplayer::QuinnOnlineSessionError::UnexpectedMessage(other),
                    );
                }
            };
            host_io
                .send_server_packet(crate::multiplayer::ServerPacket::new(message))
                .await?;
        }
        Ok(summary)
    }

    pub async fn descriptor_client_send_command_packet(
        &mut self,
        packet: crate::multiplayer::CommandPacket,
        expected_responses: usize,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        let RealOnlineSessionMode::DescriptorClientConnected {
            client_runtime,
            packet_io,
            ..
        } = &mut self.mode
        else {
            return Err(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "connected descriptor client packet io",
                ),
            );
        };
        packet_io
            .send_client_packet(crate::multiplayer::ClientPacket::new(
                crate::multiplayer::ClientMessage::CommandPacket {
                    epoch: self.typed_epoch,
                    packet,
                },
            ))
            .await?;
        for _ in 0..expected_responses {
            let response = packet_io.receive_reliable_server_packet().await?;
            let message = match response.message {
                crate::multiplayer::ServerMessage::CommandAcknowledgement {
                    epoch: _,
                    acknowledgement,
                } => crate::multiplayer::ProtocolMessage::CommandAcknowledgement(acknowledgement),
                crate::multiplayer::ServerMessage::CommandRejection {
                    epoch: _,
                    rejection,
                } => crate::multiplayer::ProtocolMessage::CommandRejection(rejection),
                other => {
                    return Err(crate::multiplayer::QuinnOnlineSessionError::Connect(
                        format!("unexpected typed command response: {other:?}"),
                    ));
                }
            };
            client_runtime.handle_message(message);
        }
        Ok(())
    }

    #[must_use]
    pub const fn player_slot(&self) -> Option<u8> {
        self.player_slot
    }

    #[must_use]
    pub const fn mode_label(&self) -> &'static str {
        self.mode.label()
    }

    #[must_use]
    pub const fn latest_authoritative_tick(&self) -> Option<crate::multiplayer::SimulationTick> {
        match &self.mode {
            RealOnlineSessionMode::DescriptorClientConnected { client_runtime, .. } => {
                Some(client_runtime.latest_authoritative_tick)
            }
            RealOnlineSessionMode::CombinedLocalhost(session) => {
                Some(session.client_runtime.latest_authoritative_tick)
            }
            _ => None,
        }
    }

    pub fn drain_events(&mut self) -> Vec<OnlineSessionEvent> {
        std::mem::take(&mut self.pending_events)
    }

    pub fn drain_command_responses(
        &mut self,
    ) -> (
        Vec<crate::multiplayer::CommandAcknowledgement>,
        Vec<crate::multiplayer::CommandRejection>,
    ) {
        (
            std::mem::take(&mut self.pending_command_acknowledgements),
            std::mem::take(&mut self.pending_command_rejections),
        )
    }

    pub fn drain_replicated_world_updates(&mut self) -> ReplicatedWorldUpdates {
        (
            std::mem::take(&mut self.pending_world_snapshots),
            std::mem::take(&mut self.pending_player_deltas),
            std::mem::take(&mut self.pending_terrain_chunks),
        )
    }

    #[must_use]
    pub fn lan_host_status_line(&self) -> Option<String> {
        match &self.mode {
            RealOnlineSessionMode::DescriptorHostPending {
                descriptor,
                lan_publisher: Some(_),
                ..
            }
            | RealOnlineSessionMode::DescriptorHostAccepted {
                descriptor,
                lan_publisher: Some(_),
                ..
            } => Some(format!(
                "LAN host ready at {}; advertised by mDNS; waiting for client.",
                descriptor.host_addr
            )),
            RealOnlineSessionMode::CombinedLocalhost(_)
            | RealOnlineSessionMode::DescriptorHostPending { .. }
            | RealOnlineSessionMode::DescriptorHostAccepted { .. }
            | RealOnlineSessionMode::DescriptorClientConnected { .. } => None,
        }
    }

    /// Drive one real network tick from a caller-provided payload and apply online UX state.
    ///
    /// # Errors
    ///
    /// Returns an error when the real Quinn tick driver fails.
    pub async fn drive_tick_input(
        &mut self,
        game: &mut GameState,
        input: crate::multiplayer::QuinnSessionTickInput,
    ) -> Result<
        crate::multiplayer::QuinnSessionTickTelemetry,
        crate::multiplayer::QuinnOnlineSessionError,
    > {
        let telemetry = self
            .mode
            .session_mut()
            .ok_or(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "active online session",
                ),
            )?
            .drive_tick_with_telemetry(input)
            .await?;
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot::from_tick_summary(
            &telemetry.summary,
            self.player_slot,
        ));
        Ok(telemetry)
    }

    /// Drive one real network telemetry tick and apply online UX state.
    ///
    /// # Errors
    ///
    /// Returns an error when the real Quinn tick driver fails.
    pub async fn drive_telemetry_tick(
        &mut self,
        game: &mut GameState,
    ) -> Result<
        crate::multiplayer::QuinnSessionTickTelemetry,
        crate::multiplayer::QuinnOnlineSessionError,
    > {
        let player_id = self
            .mode
            .session_mut()
            .ok_or(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "active online session",
                ),
            )?
            .joined_player_id()
            .unwrap_or(crate::multiplayer::LOCAL_PLAYER_ID);
        let client_id = self
            .mode
            .session_mut()
            .ok_or(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "active online session",
                ),
            )?
            .client_runtime
            .config
            .client_id;
        let sequence = self.next_sequence;
        let tick = self.next_tick;
        let snapshot_tick = crate::multiplayer::SimulationTick::new(tick + 1);
        let live_snapshot = live_player_network_snapshot(game, player_id, snapshot_tick);
        let live_player = live_snapshot
            .players
            .first()
            .cloned()
            .expect("live player snapshot contains local player");
        let terrain_request = live_player_terrain_request(game);
        let telemetry = self
            .mode
            .session_mut()
            .ok_or(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "active online session",
                ),
            )?
            .drive_tick_with_telemetry(crate::multiplayer::QuinnSessionTickInput {
                command_packet: Some(crate::multiplayer::CommandPacket {
                    client_id,
                    commands: vec![crate::multiplayer::SequencedPlayerCommand {
                        player_id,
                        sequence: crate::multiplayer::InputSequence::new(sequence),
                        target_tick: crate::multiplayer::SimulationTick::new(tick),
                        command: live_player_command(&game.player),
                    }],
                }),
                snapshot: Some(live_snapshot),
                delta: Some((
                    crate::multiplayer::SimulationTick::new(tick + 2),
                    crate::multiplayer::NetworkDeltaPayload::Players {
                        players: vec![live_player.clone()],
                    },
                )),
                terrain_chunk_request: Some(terrain_request),
                authoritative_terrain_chunks: Vec::new(),
                correction_probe: Some((
                    live_player.x + 24.0,
                    live_player.y,
                    live_player,
                    crate::multiplayer::SimulationTick::new(tick + 3),
                )),
            })
            .await?;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.next_tick = self.next_tick.saturating_add(4);
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot::from_tick_summary(
            &telemetry.summary,
            self.player_slot,
        ));
        Ok(telemetry)
    }

    /// Reconnect the owned real Quinn session and apply reconnect UX state.
    ///
    /// # Errors
    ///
    /// Returns an error when reconnect setup or handshake fails.
    pub async fn reconnect(
        &mut self,
        game: &mut GameState,
        token: crate::multiplayer::SessionToken,
    ) -> Result<(), crate::multiplayer::QuinnOnlineSessionError> {
        self.mode
            .session_mut()
            .ok_or(
                crate::multiplayer::QuinnOnlineSessionError::MissingEndpoint(
                    "active online session",
                ),
            )?
            .reconnect_with_token(token)
            .await?;
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot::from_reconnect(
            self.player_slot,
        ));
        Ok(())
    }
}

fn live_player_command(player: &Player) -> crate::multiplayer::PlayerCommand {
    crate::multiplayer::PlayerCommand::Movement {
        horizontal: player.velocity_x.clamp(-1.0, 1.0),
        thrust: player.velocity_y < -0.01,
        drill_down: player.velocity_y > 0.01,
    }
}

pub fn live_player_network_snapshot(
    game: &GameState,
    player_id: crate::multiplayer::PlayerId,
    tick: crate::multiplayer::SimulationTick,
) -> crate::multiplayer::NetworkWorldSnapshot {
    crate::multiplayer::NetworkWorldSnapshot {
        tick,
        players: vec![crate::multiplayer::NetworkPlayerSnapshot {
            player_id,
            x: game.player.x,
            y: game.player.y,
            velocity_x: game.player.velocity_x,
            velocity_y: game.player.velocity_y,
            fuel: game.player.fuel,
            hull: game.player.hull,
            credits: game.player.credits,
            cargo_used: game.player.cargo_used(),
            cargo: game.player.cargo.clone(),
            artifacts: game.player.artifacts.clone(),
            materials: game.player.materials.clone(),
            loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::from_player(&game.player),
            scanner_cooldown_seconds: 0.0,
        }],
    }
}

pub const fn is_allowed_descriptor_path_character(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(character, '/' | '\\' | '.' | '-' | '_' | ':' | ' ' | '~')
}

pub const fn is_allowed_socket_address_character(character: char) -> bool {
    character.is_ascii_digit() || matches!(character, '.' | ':' | '[' | ']')
}

fn network_terrain_chunk_tiles(
    terrain: &crate::terrain::Terrain,
    chunk_x: i32,
    chunk_y: i32,
) -> Vec<crate::multiplayer::NetworkTerrainTile> {
    const NETWORK_TERRAIN_CHUNK_SIZE_TILES: i32 = 16;
    let start_x = chunk_x * NETWORK_TERRAIN_CHUNK_SIZE_TILES;
    let start_y = chunk_y * NETWORK_TERRAIN_CHUNK_SIZE_TILES;
    let end_x = (start_x + NETWORK_TERRAIN_CHUNK_SIZE_TILES).min(terrain.width());
    let end_y = (start_y + NETWORK_TERRAIN_CHUNK_SIZE_TILES).min(terrain.height());
    let mut tiles = Vec::new();
    for y in start_y.max(0)..end_y.max(0) {
        for x in start_x.max(0)..end_x.max(0) {
            let position = TilePosition { x, y };
            if let Some(tile) = terrain.tile(position) {
                tiles.push(crate::multiplayer::NetworkTerrainTile {
                    x,
                    y,
                    kind: tile.kind,
                    durability: tile.durability,
                });
            }
        }
    }
    tiles
}

#[cfg(test)]
pub fn apply_network_terrain_chunk_to_game(
    game: &mut GameState,
    chunk_x: i32,
    chunk_y: i32,
    revision: u64,
    tiles: &[crate::multiplayer::NetworkTerrainTile],
) -> usize {
    let mut changed_tiles = Vec::new();
    for tile in tiles {
        let position = TilePosition {
            x: tile.x,
            y: tile.y,
        };
        if let Some(current) = game.terrain.tile(position)
            && current.kind == tile.kind
            && current.durability == tile.durability
        {
            continue;
        }
        if game
            .terrain
            .set_tile_from_network(position, tile.kind, tile.durability)
        {
            changed_tiles.push(position);
        }
    }
    if changed_tiles.is_empty() {
        if !game.online_last_terrain_status.contains("applied chunk") {
            game.apply_online_terrain_status(format!(
                "received chunk ({chunk_x},{chunk_y}) rev {revision} with no visible tile changes"
            ));
        }
        game.apply_online_sync_loop_status(crate::game_state::OnlineSyncLoopStatus::terrain(
            tiles.len(),
            0,
        ));
        0
    } else {
        let visible_tiles = changed_tiles.len();
        game.visual_changes.changed_tiles.extend(changed_tiles);
        game.apply_online_terrain_status(format!(
            "applied chunk ({chunk_x},{chunk_y}) rev {revision}: {visible_tiles} visible tiles"
        ));
        game.apply_online_sync_loop_status(crate::game_state::OnlineSyncLoopStatus::terrain(
            tiles.len(),
            visible_tiles,
        ));
        visible_tiles
    }
}

#[cfg(test)]
pub fn apply_network_snapshot_local_player_presentation_adapter(
    game: &mut GameState,
    snapshot: &crate::multiplayer::NetworkWorldSnapshot,
) -> bool {
    let Some(preferred_player_id) = game
        .online_player_slot
        .map(|slot| crate::multiplayer::PlayerId::new(u64::from(slot)))
    else {
        return false;
    };
    let Some(player) = snapshot
        .players
        .iter()
        .find(|player| player.player_id == preferred_player_id)
    else {
        return false;
    };
    apply_network_player_snapshot_to_game(game, player);
    true
}

#[cfg(test)]
fn apply_network_player_snapshot_to_game(
    game: &mut GameState,
    snapshot: &crate::multiplayer::NetworkPlayerSnapshot,
) {
    game.player.x = snapshot.x;
    game.player.y = snapshot.y;
    game.player.velocity_x = snapshot.velocity_x;
    game.player.velocity_y = snapshot.velocity_y;
    game.player.fuel = snapshot.fuel;
    game.player.hull = snapshot.hull;
    game.player.credits = snapshot.credits;
    game.player.cargo = snapshot.cargo.clone();
    game.player.artifacts = snapshot.artifacts.clone();
    game.player.materials = snapshot.materials.clone();
    snapshot.loadout.apply_to_player(&mut game.player);
    game.scanner_cooldown_seconds = snapshot.scanner_cooldown_seconds;
    game.mark_full_terrain_refresh();
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "presentation coordinates intentionally map to terrain request tiles"
)]
fn live_player_terrain_request(game: &GameState) -> (i32, i32, u64, u64) {
    (
        game.player.x.floor() as i32,
        game.player.y.floor() as i32,
        game.update_ticks,
        u64::from(game.total_resources_refined),
    )
}
