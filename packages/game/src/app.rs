use std::sync::mpsc;

use crate::{
    audio::AudioBus,
    game_state::{
        GameState, OnlineNetworkTaskRequest, OnlineNetworkTaskResult, OnlineShutdownSummary,
        RealOnlineSessionController,
    },
    input::{
        combine_player_input, read_gamepad_input, read_input, read_input_with_arrow_aliases,
        read_primary_keyboard_input, read_secondary_keyboard_input,
    },
    input_mapping::map_local_input,
    multiplayer::{FIXED_DELTA_SECONDS, PlayerCommand},
    rendering::GameRenderer,
    save::{load_settings, save_settings},
    session::GameSession,
};

const WINDOW_WIDTH: i32 = 1280;
const WINDOW_HEIGHT: i32 = 720;
const TARGET_FPS: u32 = 60;
const FRAME_DELTA_SPIKE_WARN_SECONDS: f32 = FIXED_DELTA_SECONDS * 15.0;

enum OnlineTaskCompletion {
    Hosted(Result<RealOnlineSessionController, String>),
    JoinedDescriptor(Result<(RealOnlineSessionController, std::path::PathBuf), String>),
    Connected(Result<RealOnlineSessionController, String>),
    Reconnected(Result<RealOnlineSessionController, String>),
}

enum OnlineDescriptorAcceptCompletion {
    Accepted(Result<RealOnlineSessionController, String>),
}

struct OnlineTaskDispatcher {
    runtime: Option<tokio::runtime::Runtime>,
    controller: Option<RealOnlineSessionController>,
    pending_completion: Option<mpsc::Receiver<OnlineTaskCompletion>>,
    pending_descriptor_accept: Option<mpsc::Receiver<OnlineDescriptorAcceptCompletion>>,
    tick_accumulator_seconds: f32,
    live_tick_sequence: u32,
}

impl OnlineTaskDispatcher {
    fn new() -> Self {
        Self {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(1)
                .build()
                .ok(),
            controller: None,
            pending_completion: None,
            pending_descriptor_accept: None,
            tick_accumulator_seconds: 0.0,
            live_tick_sequence: 0,
        }
    }

    fn controller_mode_label(&self) -> Option<&'static str> {
        self.controller
            .as_ref()
            .map(RealOnlineSessionController::mode_label)
    }

    fn poll_descriptor_accept(&mut self, game: &mut GameState) {
        let Some(receiver) = &self.pending_descriptor_accept else {
            return;
        };
        let Ok(completion) = receiver.try_recv() else {
            return;
        };
        self.pending_descriptor_accept = None;
        match completion {
            OnlineDescriptorAcceptCompletion::Accepted(Ok(controller)) => {
                self.controller = Some(controller);
                game.apply_online_runtime_tick_status(
                    crate::game_state::OnlineRuntimeTickPresentation::awaiting(
                        "descriptor-host-accepted",
                    ),
                );
                game.apply_online_network_task_result(OnlineNetworkTaskResult::Hosted(
                    crate::game_state::RealOnlineSessionUxSnapshot::from_descriptor_host_accepted(
                        Some(1),
                    ),
                ));
            }
            OnlineDescriptorAcceptCompletion::Accepted(Err(error)) => {
                game.apply_online_network_task_result(OnlineNetworkTaskResult::Failed(error));
            }
        }
    }

    fn poll_completions(&mut self, game: &mut GameState) {
        let Some(receiver) = &self.pending_completion else {
            return;
        };
        let Ok(completion) = receiver.try_recv() else {
            return;
        };
        self.pending_completion = None;
        match completion {
            OnlineTaskCompletion::Hosted(Ok(controller)) => {
                let snapshot =
                    crate::game_state::RealOnlineSessionUxSnapshot::from_host_descriptor_ready(
                        Some(1),
                        &game.online_descriptor_path,
                    );
                game.apply_online_network_task_result(OnlineNetworkTaskResult::Hosted(snapshot));
                game.apply_online_runtime_tick_status(
                    crate::game_state::OnlineRuntimeTickPresentation::new(
                        "descriptor-host-pending",
                        "waiting for descriptor client",
                    ),
                );
                if let Some(status) = controller.lan_host_status_line() {
                    game.online_session_status_message = status;
                    game.message = game.online_session_status_message.clone();
                }
                self.spawn_descriptor_accept(controller);
            }
            OnlineTaskCompletion::JoinedDescriptor(Ok((controller, path))) => {
                self.controller = Some(controller);
                game.apply_online_runtime_tick_status(
                    crate::game_state::OnlineRuntimeTickPresentation::new(
                        "descriptor-client-connected",
                        "join accepted; awaiting ticks",
                    ),
                );
                game.apply_online_network_task_result(OnlineNetworkTaskResult::JoinedDescriptor(
                    crate::game_state::RealOnlineSessionUxSnapshot::from_descriptor_client_connected(
                        Some(2),
                        &path,
                    ),
                ));
            }
            OnlineTaskCompletion::Connected(Ok(controller)) => {
                self.controller = Some(controller);
                game.apply_online_runtime_tick_status(
                    crate::game_state::OnlineRuntimeTickPresentation::new(
                        "combined-localhost",
                        "connected; awaiting ticks",
                    ),
                );
                game.apply_online_network_task_result(OnlineNetworkTaskResult::Connected(
                    crate::game_state::RealOnlineSessionUxSnapshot::from_joined_session(Some(1)),
                ));
            }
            OnlineTaskCompletion::Reconnected(Ok(controller)) => {
                self.controller = Some(controller);
                game.apply_online_runtime_tick_status(
                    crate::game_state::OnlineRuntimeTickPresentation::reconnected(
                        "combined-localhost",
                    ),
                );
                game.apply_online_network_task_result(OnlineNetworkTaskResult::Reconnected(
                    crate::game_state::RealOnlineSessionUxSnapshot::from_reconnect(Some(1)),
                ));
            }
            OnlineTaskCompletion::Hosted(Err(error))
            | OnlineTaskCompletion::JoinedDescriptor(Err(error))
            | OnlineTaskCompletion::Connected(Err(error))
            | OnlineTaskCompletion::Reconnected(Err(error)) => {
                game.apply_online_network_task_result(OnlineNetworkTaskResult::Failed(error));
            }
        }
    }

    fn spawn_descriptor_accept(&mut self, mut controller: RealOnlineSessionController) {
        let Some(runtime) = &self.runtime else {
            self.controller = Some(controller);
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.pending_descriptor_accept = Some(receiver);
        runtime.spawn(async move {
            let mut game = GameState::new();
            let result = controller
                .accept_descriptor_client(&mut game)
                .await
                .map(|()| controller)
                .map_err(|error| format!("{error:?}"));
            let _ignored = sender.send(OnlineDescriptorAcceptCompletion::Accepted(result));
        });
    }

    fn spawn_host_descriptor(
        &mut self,
        path: std::path::PathBuf,
        bind_addr: std::net::SocketAddr,
        advertise_addr: std::net::SocketAddr,
        publish_lan: bool,
    ) {
        let Some(runtime) = &self.runtime else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.pending_completion = Some(receiver);
        runtime.spawn(async move {
            let mut game = GameState::new();
            game.online_host_bind_addr = bind_addr;
            game.online_host_advertise_addr = advertise_addr;
            let result = if publish_lan {
                RealOnlineSessionController::host_lan_game_pending(&mut game, &path)
            } else {
                RealOnlineSessionController::host_descriptor_file_pending(&mut game, &path)
            }
            .map_err(|error| format!("{error:?}"));
            let _ignored = sender.send(OnlineTaskCompletion::Hosted(result));
        });
    }

    fn spawn_join_descriptor(&mut self, path: std::path::PathBuf) {
        let Some(runtime) = &self.runtime else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.pending_completion = Some(receiver);
        runtime.spawn(async move {
            let mut game = GameState::new();
            let result = RealOnlineSessionController::connect_descriptor_client(&mut game, &path)
                .await
                .map(|controller| (controller, path))
                .map_err(|error| format!("{error:?}"));
            let _ignored = sender.send(OnlineTaskCompletion::JoinedDescriptor(result));
        });
    }

    fn spawn_join_lan(&mut self) {
        let Some(runtime) = &self.runtime else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.pending_completion = Some(receiver);
        runtime.spawn(async move {
            let result = async {
                let browser = crate::lan_discovery::LanDiscoveryBrowser::new()
                    .map_err(|error| format!("LAN discovery failed: {error}"))?;
                let report = browser
                    .browse_report_for(std::time::Duration::from_secs(6))
                    .map_err(|error| format!("LAN browse failed: {error}"))?;
                let diagnostic = format!(
                    "mDNS events seen: {}, resolved services: {}, usable Drillgame hosts: {}",
                    report.events_seen,
                    report.resolved_seen,
                    report.games.len()
                );
                let unusable = report
                    .unusable_resolved_services
                    .first()
                    .map_or_else(String::new, |service| format!(" First unusable service: {service}"));
                let game = report.games.into_iter().next().ok_or_else(|| {
                    format!(
                        "No usable Drillgame LAN hosts found after 6s. {diagnostic}.{unusable} If `dns-sd -L` sees the host, the app likely received no resolved service or discarded it because descriptor_addr/game_addr TXT data was missing or unparsable."
                    )
                })?;
                let mut descriptor =
                    crate::lan_discovery::fetch_descriptor(game.descriptor_addr)
                        .map_err(|error| format!("LAN descriptor fetch failed: {error}"))?;
                descriptor =
                    crate::lan_discovery::patch_descriptor_addr_for_lan(descriptor, game.game_addr);
                let path = std::env::temp_dir().join(format!(
                    "drillgame-lan-join-{}-{}.json",
                    std::process::id(),
                    game.session_id.replace(['/', ':', '\\'], "-")
                ));
                let json = serde_json::to_string(&descriptor).map_err(|error| error.to_string())?;
                std::fs::write(&path, json).map_err(|error| error.to_string())?;
                let mut local_game = GameState::new();
                let connect_result = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    RealOnlineSessionController::connect_descriptor_client(&mut local_game, &path),
                )
                .await
                .map_err(|_| {
                    crate::multiplayer::QuinnOnlineSessionError::Connect(
                        "timed out after 10s waiting for host to accept LAN descriptor client over UDP 4242"
                            .to_owned(),
                    )
                })
                .and_then(std::convert::identity);
                connect_result
                    .map(|controller| (controller, path))
                    .map_err(|error| format!("{error:?}"))
            }
            .await;
            let _ignored = sender.send(OnlineTaskCompletion::JoinedDescriptor(result));
        });
    }

    fn spawn_connect(&mut self) {
        let Some(runtime) = &self.runtime else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.pending_completion = Some(receiver);
        runtime.spawn(async move {
            let mut game = GameState::new();
            let result = RealOnlineSessionController::connect_localhost(&mut game)
                .await
                .map_err(|error| format!("{error:?}"));
            let _ignored = sender.send(OnlineTaskCompletion::Connected(result));
        });
    }

    fn spawn_reconnect(&mut self, mut controller: RealOnlineSessionController) {
        let Some(runtime) = &self.runtime else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.pending_completion = Some(receiver);
        runtime.spawn(async move {
            let mut game = GameState::new();
            let result = controller
                .reconnect(&mut game, crate::multiplayer::SessionToken::new(1))
                .await
                .map(|()| controller)
                .map_err(|error| format!("{error:?}"));
            let _ignored = sender.send(OnlineTaskCompletion::Reconnected(result));
        });
    }

    #[allow(
        clippy::too_many_lines,
        reason = "online task dispatcher keeps request routing and shutdown policy together"
    )]
    fn drain_and_execute(&mut self, game: &mut GameState) {
        self.poll_descriptor_accept(game);
        self.poll_completions(game);
        let Some(request) = game.take_online_network_task_request() else {
            return;
        };

        if self.pending_completion.is_some() && request != OnlineNetworkTaskRequest::Shutdown {
            game.apply_online_network_task_result(OnlineNetworkTaskResult::Failed(
                "Online network task already in progress".to_owned(),
            ));
            return;
        }

        match request {
            OnlineNetworkTaskRequest::HostDirectConnect
            | OnlineNetworkTaskRequest::JoinDirectConnect => {
                self.spawn_connect();
            }
            OnlineNetworkTaskRequest::HostDescriptorFile { path } => {
                game.online_session_status_message = format!(
                    "Preparing host descriptor at {}; waiting for remote miner after descriptor is written.",
                    path.display()
                );
                self.spawn_host_descriptor(
                    path,
                    game.online_host_bind_addr,
                    game.online_host_advertise_addr,
                    false,
                );
            }
            OnlineNetworkTaskRequest::HostLanGame => {
                "Starting LAN host: binding server, opening descriptor endpoint, publishing mDNS."
                    .clone_into(&mut game.online_session_status_message);
                self.spawn_host_descriptor(
                    std::env::temp_dir()
                        .join(format!("drillgame-lan-host-{}.json", std::process::id())),
                    game.online_host_bind_addr,
                    game.online_host_advertise_addr,
                    true,
                );
            }
            OnlineNetworkTaskRequest::JoinDescriptorFile { path } => {
                self.spawn_join_descriptor(path);
            }
            OnlineNetworkTaskRequest::JoinLanGame => {
                "Scanning LAN for Drillgame hosts via mDNS."
                    .clone_into(&mut game.online_session_status_message);
                self.spawn_join_lan();
            }
            OnlineNetworkTaskRequest::ReconnectDirectConnect => {
                if let Some(controller) = self.controller.take() {
                    self.spawn_reconnect(controller);
                } else {
                    game.apply_online_network_task_result(OnlineNetworkTaskResult::Failed(
                        "No active online session to reconnect".to_owned(),
                    ));
                }
            }
            OnlineNetworkTaskRequest::Shutdown => {
                let host_owns_save = game.online_host_owns_save;
                let mut role_label = game.online_role_label().to_owned();
                let mut attempted = false;
                let mut notified = false;
                let mut warning = None;
                if let (Some(runtime), Some(controller)) = (&self.runtime, &mut self.controller) {
                    match controller.mode_label() {
                        "descriptor-host-accepted" => {
                            "descriptor host".clone_into(&mut role_label);
                            attempted = true;
                            match runtime.block_on(
                                controller
                                    .descriptor_host_send_session_ended("host ended the session"),
                            ) {
                                Ok(()) => notified = true,
                                Err(error) => warning = Some(format!("{error:?}")),
                            }
                        }
                        "descriptor-client-connected" => {
                            "descriptor client".clone_into(&mut role_label);
                            attempted = true;
                            match runtime.block_on(controller.descriptor_client_send_session_ended(
                                "joined client left the session",
                            )) {
                                Ok(()) => notified = true,
                                Err(error) => warning = Some(format!("{error:?}")),
                            }
                        }
                        other => {
                            other.clone_into(&mut role_label);
                        }
                    }
                }
                let summary = OnlineShutdownSummary::from_notification(
                    role_label,
                    attempted,
                    notified,
                    warning,
                    host_owns_save,
                );
                self.controller = None;
                self.pending_completion = None;
                self.pending_descriptor_accept = None;
                game.apply_online_network_task_result(OnlineNetworkTaskResult::Shutdown(summary));
            }
        }
    }

    fn gameplay_commands_for_network_tick(
        game: &GameState,
        commands: Vec<PlayerCommand>,
    ) -> Vec<PlayerCommand> {
        if game.run_mode == crate::game_state::RunMode::Playing {
            commands
        } else {
            Vec::new()
        }
    }

    #[allow(
        dead_code,
        reason = "unit tests inspect tick payload construction directly"
    )]
    fn live_session_tick_input(
        &mut self,
        session: &mut GameSession,
        local_player_commands: Vec<PlayerCommand>,
    ) -> crate::multiplayer::QuinnSessionTickInput {
        let descriptor_client_connected = self
            .controller
            .as_ref()
            .is_some_and(|controller| controller.mode_label() == "descriptor-client-connected");
        let identity = session.prepare_online_tick_identity(descriptor_client_connected);
        let sequence = self.live_tick_sequence;
        self.live_tick_sequence = self.live_tick_sequence.wrapping_add(1);
        session
            .live_session_tick_input_with_provenance(
                identity.client_id,
                identity.player_id,
                sequence,
                local_player_commands,
            )
            .input
    }

    #[allow(
        clippy::too_many_lines,
        reason = "online host/client tick sequencing is kept together while transport ownership is being split"
    )]
    fn drive_scheduled_tick(
        &mut self,
        session: &mut GameSession,
        delta_seconds: f32,
        local_player_commands: Vec<PlayerCommand>,
    ) {
        if self.controller.is_none() {
            self.tick_accumulator_seconds = 0.0;
            return;
        }
        self.tick_accumulator_seconds += delta_seconds;
        if self.tick_accumulator_seconds < FIXED_DELTA_SECONDS {
            return;
        }
        self.tick_accumulator_seconds = 0.0;
        let start_session_requested = session.game_mut().take_online_start_session_request();
        let local_player_commands =
            Self::gameplay_commands_for_network_tick(session.game(), local_player_commands);
        let Some(controller) = &mut self.controller else {
            return;
        };
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                session.game_mut().apply_online_network_task_result(
                    OnlineNetworkTaskResult::Failed(error.to_string()),
                );
                return;
            }
        };
        let mode_label = controller.mode_label();
        if start_session_requested && mode_label == "descriptor-host-accepted" {
            let start_result = runtime.block_on(controller.descriptor_host_send_start_session(
                crate::multiplayer::SimulationTick::new(u64::from(self.live_tick_sequence)),
            ));
            if let Err(error) = start_result {
                session.game_mut().apply_online_network_task_result(
                    OnlineNetworkTaskResult::Failed(format!("{error:?}")),
                );
                return;
            }
            session.game_mut().note_online_host_start_sent(
                crate::multiplayer::SimulationTick::new(u64::from(self.live_tick_sequence)),
            );
        }
        let result = match mode_label {
            "descriptor-host-accepted" => {
                let command_summary =
                    match runtime.block_on(controller.descriptor_host_try_receive_command_packet(
                        std::time::Duration::from_millis(1),
                    )) {
                        Ok(summary) => summary,
                        Err(error) => {
                            session.game_mut().apply_online_network_task_result(
                                OnlineNetworkTaskResult::Failed(format!("{error:?}")),
                            );
                            return;
                        }
                    };
                if command_summary.is_some() {
                    let pre_replication_summary = crate::multiplayer::QuinnSessionTickSummary {
                        command_summary: command_summary.clone(),
                        snapshot_replicated: false,
                        delta_replicated: false,
                        terrain_chunk_response: None,
                        correction_summary: None,
                    };
                    let _application =
                        session.apply_online_tick_summary(mode_label, &pre_replication_summary);
                }
                let identity = session.prepare_online_tick_identity(false);
                let sequence = self.live_tick_sequence;
                self.live_tick_sequence = self.live_tick_sequence.wrapping_add(1);
                let input = session
                    .live_session_tick_input_with_provenance(
                        identity.client_id,
                        identity.player_id,
                        sequence,
                        local_player_commands,
                    )
                    .input;
                runtime.block_on(controller.drive_descriptor_host_replication_tick(
                    session.game_mut(),
                    input,
                    command_summary,
                ))
            }
            "descriptor-client-connected" => {
                let identity = session.prepare_online_tick_identity(true);
                let sequence = self.live_tick_sequence;
                self.live_tick_sequence = self.live_tick_sequence.wrapping_add(1);
                let input = session
                    .live_session_tick_input_with_provenance(
                        identity.client_id,
                        identity.player_id,
                        sequence,
                        local_player_commands,
                    )
                    .input;
                if let Some(packet) = &input.command_packet {
                    session.track_online_prediction_commands(packet);
                }
                runtime.block_on(
                    controller.drive_descriptor_client_outbound_tick(session.game_mut(), input),
                )
            }
            "descriptor-host-pending" => Ok(crate::multiplayer::QuinnSessionTickSummary {
                command_summary: None,
                snapshot_replicated: false,
                delta_replicated: false,
                terrain_chunk_response: None,
                correction_summary: None,
            }),
            _ => {
                let descriptor_client_connected = mode_label == "descriptor-client-connected";
                let identity = session.prepare_online_tick_identity(descriptor_client_connected);
                let sequence = self.live_tick_sequence;
                self.live_tick_sequence = self.live_tick_sequence.wrapping_add(1);
                let input = session
                    .live_session_tick_input_with_provenance(
                        identity.client_id,
                        identity.player_id,
                        sequence,
                        local_player_commands,
                    )
                    .input;
                runtime
                    .block_on(controller.drive_tick_input(session.game_mut(), input))
                    .map(|telemetry| telemetry.summary)
            }
        };
        let command_responses = if mode_label == "descriptor-client-connected" {
            Some(controller.drain_command_responses())
        } else {
            None
        };
        match result {
            Ok(summary) => {
                if let Some((acknowledgements, rejections)) = command_responses {
                    for acknowledgement in acknowledgements {
                        session.apply_online_command_acknowledgement(&acknowledgement);
                    }
                    for rejection in rejections {
                        session.apply_online_command_rejection(&rejection);
                    }
                }
                let application = session.apply_online_tick_summary(mode_label, &summary);
                let diagnostic = GameSession::online_tick_diagnostic(&summary, application);
                session.game_mut().apply_online_runtime_tick_status(
                    crate::game_state::OnlineRuntimeTickPresentation::new(mode_label, diagnostic),
                );
            }
            Err(error) => {
                session.game_mut().apply_online_network_task_result(
                    OnlineNetworkTaskResult::Failed(format!("{error:?}")),
                );
            }
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "main loop coordinates platform input, session stepping, audio, saving, and rendering"
)]
pub fn run() {
    let (mut raylib, thread) = raylib::init()
        .size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .highdpi()
        .title("Drillgame")
        .build();

    raylib.set_target_fps(TARGET_FPS);
    raylib.set_exit_key(None);

    let settings = load_settings();
    let mut session = GameSession::new();
    session.apply_settings(settings);
    if session.fullscreen() {
        raylib.toggle_fullscreen();
    }
    let audio = match AudioBus::new() {
        Ok(audio) => Some(audio),
        Err(error) => {
            eprintln!("Audio disabled: {error}");
            None
        }
    };

    let mut renderer = GameRenderer::new(&mut raylib, &thread, session.game());
    let mut online_tasks = OnlineTaskDispatcher::new();

    while !session.should_exit() {
        let delta_seconds = raylib.get_frame_time();
        debug_assert!(
            delta_seconds <= FRAME_DELTA_SPIKE_WARN_SECONDS,
            "large frame delta detected before fixed-tick simulation migration"
        );
        let exit_requested = raylib.window_should_close();
        let split_screen_active = session.local_split_screen_active();
        let input = if split_screen_active {
            read_input(&mut raylib, exit_requested)
        } else {
            read_input_with_arrow_aliases(&mut raylib, exit_requested)
        };
        let mapped_input = map_local_input(input);
        renderer.apply_ui_input(input);
        if mapped_input
            .client_actions
            .contains(&crate::multiplayer::ClientAction::ToggleLocalMultiplayer)
        {
            let enabled_split_screen = session.enable_default_local_split_screen();
            if enabled_split_screen {
                let player_slots =
                    u8::try_from(session.local_input_client_count()).unwrap_or(u8::MAX);
                session
                    .game_mut()
                    .mark_local_multiplayer_active(player_slots);
            }
        }
        if session.game_mut().take_local_multiplayer_request() {
            let _ = session.enable_default_local_split_screen();
            let player_slots = u8::try_from(session.local_input_client_count()).unwrap_or(u8::MAX);
            session
                .game_mut()
                .mark_local_multiplayer_active(player_slots);
        }
        online_tasks.drain_and_execute(session.game_mut());
        let online_authority_mode = online_tasks.controller_mode_label();
        let immediate_client_actions = mapped_input
            .client_actions
            .iter()
            .copied()
            .filter(|action| !matches!(action, crate::multiplayer::ClientAction::ExitRequested))
            .collect::<Vec<_>>();
        session.apply_client_actions(
            crate::multiplayer::LOCAL_CLIENT_ID,
            &immediate_client_actions,
        );
        if split_screen_active {
            let secondary_input = Some({
                let keyboard = read_secondary_keyboard_input(&raylib);
                read_gamepad_input(&raylib, 0)
                    .map_or(keyboard, |gamepad| combine_player_input(keyboard, gamepad))
            });
            let primary_input = read_primary_keyboard_input(&mut raylib);
            for local_input in crate::input_mapping::local_split_screen_inputs(
                crate::multiplayer::LOCAL_CLIENT_ID,
                primary_input,
                session.secondary_local_client_id(),
                secondary_input,
            ) {
                let _batch =
                    session.route_command_producer(local_input.client_id, local_input.producer);
            }
        }

        let authority_input = if split_screen_active {
            crate::game_state::input_without_session_gameplay_commands(input)
        } else {
            input
        };
        let authority_update = if online_authority_mode == Some("descriptor-client-connected") {
            if session.game().run_mode == crate::game_state::RunMode::Playing {
                crate::session::SessionAuthorityUpdateSummary {
                    used_legacy_presentation_adapter: false,
                    local_movement_authority: false,
                    command_adapter_count: 0,
                    current_tick: session.current_tick(),
                }
            } else {
                session.update_shell_frame_from_session_authority(authority_input, delta_seconds)
            }
        } else if split_screen_active {
            session.update_shell_frame_from_session_authority(authority_input, delta_seconds)
        } else {
            session.update_frame_from_session_authority(authority_input, delta_seconds)
        };
        if session.game().request_exit
            && session
                .game_mut()
                .request_online_shutdown_from_gameplay_exit()
        {
            online_tasks.drain_and_execute(session.game_mut());
        }
        if session.should_exit() {
            online_tasks.drain_and_execute(session.game_mut());
        }
        let _legacy_bridge_active = authority_update.legacy_bridge_active();
        online_tasks.drive_scheduled_tick(
            &mut session,
            delta_seconds,
            mapped_input.player_commands.clone(),
        );
        let world_delta = session.drain_world_delta();
        let _world_delta_is_empty = world_delta.is_empty();
        if input.fullscreen {
            raylib.toggle_fullscreen();
        }
        let settings_dirty = session.take_settings_dirty();
        if settings_dirty {
            session.note_save_session_transition_for_prediction();
        }
        if (input.fullscreen || input.volume_up || input.volume_down || settings_dirty)
            && let Err(error) = save_settings(session.current_settings())
        {
            eprintln!("Settings save failed: {error}");
        }
        if let Some(audio) = &audio {
            audio.set_volume(session.master_volume());
            audio.play(&session.game().sound_cues);
        }

        renderer.sync_delta(&mut raylib, &thread, session.game_mut(), &world_delta);

        let mut draw = raylib.begin_drawing(&thread);
        session.update_remote_timing_from_network_sample(0.0, 0.0);
        let _remote_timing = session.remote_timing();
        let prediction_plan = session.live_prediction_presentation_plan(0.0, 0.5, 0.0);
        let live_render_output = session.live_render_frame_output(&prediction_plan);
        renderer.render_live_frame_output(&mut draw, session.game(), &live_render_output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::{OnlineSessionUxState, RunMode};

    fn wait_for_online_completion(dispatcher: &mut OnlineTaskDispatcher, game: &mut GameState) {
        for _ in 0..50 {
            dispatcher.drain_and_execute(game);
            if dispatcher.pending_completion.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("online task did not complete");
    }

    #[test]
    fn split_screen_authority_input_strips_primary_gameplay_after_command_producers_route_it() {
        let mut session = GameSession::new();
        session.game_mut().run_mode = RunMode::Playing;
        assert!(session.enable_default_local_split_screen());
        let primary_input = crate::input::PlayerInput {
            horizontal: 1.0,
            thrust: true,
            cancel: true,
            map: true,
            ..crate::input::PlayerInput::default()
        };
        let producers = crate::input_mapping::local_split_screen_inputs(
            crate::multiplayer::LOCAL_CLIENT_ID,
            primary_input,
            session.secondary_local_client_id(),
            Some(crate::input::PlayerInput::default()),
        );
        for local_input in producers {
            let _batch =
                session.route_command_producer(local_input.client_id, local_input.producer);
        }

        let shell_input = crate::game_state::input_without_session_gameplay_commands(primary_input);
        let summary =
            session.update_shell_frame_from_session_authority(shell_input, FIXED_DELTA_SECONDS);

        assert!(!summary.legacy_bridge_active());
        assert!(summary.local_movement_authority);
        assert!(
            session
                .latest_local_authoritative_commands_for_tests()
                .iter()
                .any(|command| {
                    matches!(
                        command,
                        PlayerCommand::Movement {
                            horizontal,
                            thrust: true,
                            drill_down: false,
                        } if (*horizontal - 1.0).abs() < f32::EPSILON
                    )
                })
        );
        assert_eq!(session.game().run_mode, RunMode::Playing);
        assert_eq!(session.game().modal, None);
    }

    #[test]
    fn live_session_tick_input_uses_real_local_player_commands() {
        let mut session = GameSession::new();
        let mut dispatcher = OnlineTaskDispatcher::new();
        let input = dispatcher.live_session_tick_input(
            &mut session,
            vec![
                PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: true,
                    drill_down: true,
                },
                PlayerCommand::UseScanner,
            ],
        );

        let packet = input.command_packet.expect("command packet is generated");
        assert_eq!(packet.commands.len(), 2);
        assert_eq!(
            packet.commands[0].command,
            PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: true,
                drill_down: true,
            }
        );
        assert_eq!(packet.commands[1].command, PlayerCommand::UseScanner);
        assert_eq!(packet.commands[0].sequence, packet.commands[1].sequence);
        assert!(input.snapshot.is_some());
        assert!(matches!(
            input.delta,
            Some((_, crate::multiplayer::NetworkDeltaPayload::Players { .. }))
        ));
        assert!(input.terrain_chunk_request.is_some());
    }

    #[test]
    fn live_session_tick_input_uses_joined_client_network_identity() {
        let mut session = GameSession::new();
        session.game_mut().online_player_slot = Some(2);
        let mut dispatcher = OnlineTaskDispatcher::new();
        let input = dispatcher.live_session_tick_input(
            &mut session,
            vec![PlayerCommand::Movement {
                horizontal: -1.0,
                thrust: false,
                drill_down: false,
            }],
        );

        let packet = input.command_packet.expect("command packet is generated");
        let command = packet.commands.first().expect("sequenced command");
        assert_eq!(packet.client_id, crate::multiplayer::ClientId::new(2));
        assert_eq!(command.player_id, crate::multiplayer::PlayerId::new(2));
    }

    #[test]
    fn accepted_remote_commands_create_online_client_and_move_remote_player() {
        let mut session = GameSession::new();
        let summary = crate::multiplayer::CommandPacketExchangeSummary {
            client_id: crate::multiplayer::ClientId::new(2),
            acknowledged: 1,
            rejected: 0,
            authoritative_tick: session.current_tick(),
            accepted_commands: vec![crate::multiplayer::SequencedPlayerCommand {
                player_id: crate::multiplayer::PlayerId::new(2),
                sequence: crate::multiplayer::InputSequence::new(7),
                target_tick: session.current_tick(),
                command: PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: false,
                    drill_down: false,
                },
            }],
        };

        let applied = session.apply_accepted_online_remote_commands(&summary);

        assert_eq!(applied, 1);
        assert!(session.has_client(crate::multiplayer::ClientId::new(2)));
        let remote_player = session
            .world_snapshot()
            .network_snapshot()
            .players
            .into_iter()
            .find(|player| player.player_id == crate::multiplayer::PlayerId::new(2))
            .expect("remote player exists");
        assert!(remote_player.velocity_x > 0.0 || remote_player.x > session.game().player.x);
    }

    #[test]
    fn accepted_remote_drill_commands_mine_authoritative_terrain_through_app_session_path() {
        let mut session = GameSession::new();
        let remote_client = crate::multiplayer::ClientId::new(2);
        let remote_player = crate::multiplayer::PlayerId::new(2);
        assert!(session.add_local_client_player(remote_client, remote_player));
        let mut player = session.game().player.clone();
        player.x = crate::game_state::TILE_SIZE * 10.0;
        player.y = crate::game_state::TILE_SIZE * 10.0;
        player.drill_strength = 4;
        *session
            .world_mut()
            .player_mut(remote_player)
            .expect("remote player exists") = player;
        let target = crate::terrain::TilePosition { x: 10, y: 11 };
        assert!(
            session
                .world_mut()
                .terrain_mut()
                .set_kind(target, crate::terrain::TileKind::Dirt)
        );
        let summary = crate::multiplayer::CommandPacketExchangeSummary {
            client_id: remote_client,
            acknowledged: 1,
            rejected: 0,
            authoritative_tick: session.current_tick(),
            accepted_commands: vec![crate::multiplayer::SequencedPlayerCommand {
                player_id: remote_player,
                sequence: crate::multiplayer::InputSequence::new(8),
                target_tick: session.current_tick(),
                command: PlayerCommand::Movement {
                    horizontal: 0.0,
                    thrust: false,
                    drill_down: true,
                },
            }],
        };

        let applied = session.apply_accepted_online_remote_commands(&summary);
        let advance = session.advance_authoritative_world_ticks(30);
        let delta = session.drain_world_delta().compact_network_delta();

        assert_eq!(applied, 1);
        assert!(advance.terrain_events > 0);
        assert_eq!(
            session
                .world()
                .terrain()
                .tile(target)
                .expect("target tile exists")
                .kind,
            crate::terrain::TileKind::Air
        );
        assert!(matches!(
            delta,
            crate::session::CompactWorldDelta::TerrainChunks { .. }
        ));
    }

    #[test]
    fn descriptor_host_start_from_ui_sends_start_session_to_joined_client() {
        let path = std::env::temp_dir().join(format!(
            "drillgame-start-session-{}.json",
            std::process::id()
        ));
        let _ignored = std::fs::remove_file(&path);
        let mut host_dispatcher = OnlineTaskDispatcher::new();
        let mut join_dispatcher = OnlineTaskDispatcher::new();
        let mut host_game = GameState::new();
        host_game.online_descriptor_path = path.clone();
        let mut join_game = GameState::new();
        join_game.online_descriptor_path = path;

        host_game.online_network_task_request =
            Some(OnlineNetworkTaskRequest::HostDescriptorFile {
                path: host_game.online_descriptor_path.clone(),
            });
        host_dispatcher.drain_and_execute(&mut host_game);
        wait_for_online_completion(&mut host_dispatcher, &mut host_game);
        assert_eq!(
            host_game.online_session_state,
            OnlineSessionUxState::Hosting
        );

        join_game.online_network_task_request =
            Some(OnlineNetworkTaskRequest::JoinDescriptorFile {
                path: join_game.online_descriptor_path.clone(),
            });
        join_dispatcher.drain_and_execute(&mut join_game);
        wait_for_online_completion(&mut join_dispatcher, &mut join_game);
        wait_for_online_completion(&mut host_dispatcher, &mut host_game);
        assert_eq!(
            host_game.online_session_state,
            OnlineSessionUxState::Connected
        );
        assert_eq!(
            join_game.online_session_state,
            OnlineSessionUxState::Connected
        );

        let mut host_session = GameSession::new();
        host_session.game_mut().online_session_state = OnlineSessionUxState::Connected;
        host_session.game_mut().online_host_owns_save = true;
        host_session.game_mut().online_player_slot = Some(1);
        host_session.game_mut().online_remote_player_connected = true;
        host_session.game_mut().online_local_ready = true;
        host_session.game_mut().online_remote_player_ready = true;
        host_session.game_mut().request_online_gameplay_start();
        assert_eq!(host_session.game().run_mode, RunMode::Playing);

        host_dispatcher.drive_scheduled_tick(&mut host_session, FIXED_DELTA_SECONDS, Vec::new());

        let mut join_session = GameSession::new();
        join_session.game_mut().online_session_state = OnlineSessionUxState::Connected;
        join_session.game_mut().online_host_owns_save = false;
        join_session.game_mut().online_player_slot = Some(2);
        join_session.game_mut().online_remote_player_connected = true;
        join_session.game_mut().online_local_ready = true;
        join_session.game_mut().online_remote_player_ready = true;
        join_dispatcher.drive_scheduled_tick(&mut join_session, FIXED_DELTA_SECONDS, Vec::new());

        assert_eq!(join_session.game().run_mode, RunMode::Playing);
    }

    #[test]
    fn accepted_remote_service_commands_are_host_authoritative_through_app_session_path() {
        let mut session = GameSession::new();
        let remote_client = crate::multiplayer::ClientId::new(2);
        let remote_player = crate::multiplayer::PlayerId::new(2);
        assert!(session.add_local_client_player(remote_client, remote_player));
        {
            let player = session
                .world_mut()
                .player_mut(remote_player)
                .expect("remote player exists");
            player.credits = 10_000;
            player.fuel = 1.0;
            player.hull = 1.0;
        }
        let target_tick = session.current_tick();
        let summary = crate::multiplayer::CommandPacketExchangeSummary {
            client_id: remote_client,
            acknowledged: 3,
            rejected: 0,
            authoritative_tick: target_tick,
            accepted_commands: vec![
                crate::multiplayer::SequencedPlayerCommand {
                    player_id: remote_player,
                    sequence: crate::multiplayer::InputSequence::new(9),
                    target_tick,
                    command: PlayerCommand::Refuel,
                },
                crate::multiplayer::SequencedPlayerCommand {
                    player_id: remote_player,
                    sequence: crate::multiplayer::InputSequence::new(10),
                    target_tick,
                    command: PlayerCommand::Repair,
                },
                crate::multiplayer::SequencedPlayerCommand {
                    player_id: remote_player,
                    sequence: crate::multiplayer::InputSequence::new(11),
                    target_tick,
                    command: PlayerCommand::BuyUpgrade { index: 0 },
                },
            ],
        };

        let applied = session.apply_accepted_online_remote_commands(&summary);
        let player = session
            .world()
            .player(remote_player)
            .expect("remote player exists");
        let transaction_kinds = session
            .world()
            .service_transactions()
            .iter()
            .map(|transaction| transaction.kind)
            .collect::<Vec<_>>();

        assert_eq!(applied, 3);
        assert!(player.fuel > 1.0);
        assert!(player.hull > 1.0);
        assert!(transaction_kinds.contains(&crate::session::PlayerTransactionKind::Refuel));
        assert!(transaction_kinds.contains(&crate::session::PlayerTransactionKind::Repair));
        assert!(transaction_kinds.contains(&crate::session::PlayerTransactionKind::BuyUpgrade));
        assert!(session.game().player.credits < 10_000);
    }

    #[test]
    fn accepted_remote_rescue_updates_authoritative_survival_snapshot_for_replication() {
        let mut session = GameSession::new();
        let remote_client = crate::multiplayer::ClientId::new(2);
        let remote_player = crate::multiplayer::PlayerId::new(2);
        assert!(session.add_local_client_player(remote_client, remote_player));
        {
            let player = session
                .world_mut()
                .player_mut(remote_player)
                .expect("remote player exists");
            player.fuel = 0.0;
            player.hull = 0.0;
            player.x = 500.0;
            player.y = 900.0;
        }
        let before = session
            .world()
            .player_survival_snapshot(remote_player)
            .expect("survival snapshot before rescue");
        let target_tick = session.current_tick();
        let summary = crate::multiplayer::CommandPacketExchangeSummary {
            client_id: remote_client,
            acknowledged: 1,
            rejected: 0,
            authoritative_tick: target_tick,
            accepted_commands: vec![crate::multiplayer::SequencedPlayerCommand {
                player_id: remote_player,
                sequence: crate::multiplayer::InputSequence::new(12),
                target_tick,
                command: PlayerCommand::Rescue,
            }],
        };

        let applied = session.apply_accepted_online_remote_commands(&summary);
        let after = session
            .world()
            .player_survival_snapshot(remote_player)
            .expect("survival snapshot after rescue");
        let replicated = session
            .world_snapshot()
            .network_snapshot()
            .players
            .into_iter()
            .find(|player| player.player_id == remote_player)
            .expect("remote player replicated");

        assert_eq!(applied, 1);
        assert!(before.disabled);
        assert!(!after.disabled);
        assert!(after.fuel > before.fuel);
        assert!(after.hull > before.hull);
        assert!((replicated.fuel - after.fuel).abs() < f32::EPSILON);
        assert!((replicated.hull - after.hull).abs() < f32::EPSILON);
    }

    #[test]
    fn live_session_tick_input_requests_chunk_containing_visible_player() {
        let mut session = GameSession::new();
        session
            .world_mut()
            .player_mut(crate::multiplayer::LOCAL_PLAYER_ID)
            .expect("local world player exists")
            .x = crate::game_state::TILE_SIZE * 33.0;
        session
            .world_mut()
            .player_mut(crate::multiplayer::LOCAL_PLAYER_ID)
            .expect("local world player exists")
            .y = crate::game_state::TILE_SIZE * 18.0;
        let mut dispatcher = OnlineTaskDispatcher::new();

        let input = dispatcher.live_session_tick_input(&mut session, Vec::new());

        assert_eq!(input.terrain_chunk_request, Some((2, 1, 0, 0)));
    }

    #[test]
    fn replicated_remote_presentations_sync_into_session_world() {
        let mut session = GameSession::new();
        session.game_mut().online_remote_player_snapshots.push(
            crate::game_state::OnlineRemotePlayerPresentation {
                player_id: crate::multiplayer::PlayerId::new(2),
                x: 42.0,
                y: 84.0,
                velocity_x: 3.0,
                velocity_y: -1.0,
                fuel: 77.0,
                hull: 66.0,
                credits: 55,
                cargo_used: 4,
                cargo: std::collections::BTreeMap::new(),
                artifacts: std::collections::BTreeMap::new(),
                materials: std::collections::BTreeMap::new(),
            },
        );

        let synced = session
            .sync_legacy_remote_presentations_to_world()
            .remote_players_updated;

        assert_eq!(synced, 1);
        assert!(session.has_client(crate::multiplayer::ClientId::new(2)));
        let snapshot = session.world_snapshot().network_snapshot();
        let remote = snapshot
            .players
            .iter()
            .find(|player| player.player_id == crate::multiplayer::PlayerId::new(2))
            .expect("remote player syncs into world");
        assert!((remote.x - 42.0).abs() < f32::EPSILON);
        assert!((remote.y - 84.0).abs() < f32::EPSILON);
        assert_eq!(remote.credits, 55);
    }

    #[test]
    fn live_session_tick_input_keeps_idle_movement_when_no_commands_are_available() {
        let mut session = GameSession::new();
        let mut dispatcher = OnlineTaskDispatcher::new();
        let input = dispatcher.live_session_tick_input(&mut session, Vec::new());

        let packet = input.command_packet.expect("idle packet is generated");
        assert_eq!(packet.commands.len(), 1);
        assert_eq!(
            packet.commands[0].command,
            PlayerCommand::Movement {
                horizontal: 0.0,
                thrust: false,
                drill_down: false,
            }
        );
    }

    #[test]
    fn online_task_dispatcher_executes_queued_connect_scheduled_tick_and_shutdown() {
        let mut game = GameState::new();
        let mut session = GameSession::new();
        let mut dispatcher = OnlineTaskDispatcher::new();

        game.online_network_task_request = Some(OnlineNetworkTaskRequest::HostDirectConnect);
        dispatcher.drain_and_execute(&mut game);
        wait_for_online_completion(&mut dispatcher, &mut game);
        dispatcher.drive_scheduled_tick(&mut session, FIXED_DELTA_SECONDS, Vec::new());

        assert_eq!(
            session.game().online_session_state,
            OnlineSessionUxState::Connected,
            "{}",
            session.game().message
        );
        assert!(session.game().message.contains("Real Quinn tick"));
        assert_eq!(game.run_mode, crate::game_state::RunMode::Playing);
        assert_eq!(game.modal, None);
        assert!(game.online_network_task_request.is_none());

        game.online_network_task_request = Some(OnlineNetworkTaskRequest::Shutdown);
        dispatcher.drain_and_execute(&mut game);

        assert_eq!(game.online_session_state, OnlineSessionUxState::Shutdown);
    }

    #[test]
    fn online_task_dispatcher_hosts_descriptor_file_without_entering_gameplay() {
        let mut game = GameState::new();
        let unique_path = std::env::temp_dir().join(format!(
            "drillgame-ui-host-descriptor-{}.json",
            std::process::id()
        ));
        let _ignored = std::fs::remove_file(&unique_path);
        game.online_descriptor_path = unique_path.clone();
        game.online_host_bind_addr = "127.0.0.1:0".parse().expect("bind addr parses");
        game.online_host_advertise_addr = "127.0.0.1:4242".parse().expect("advertise addr parses");
        let mut dispatcher = OnlineTaskDispatcher::new();

        game.online_network_task_request = Some(OnlineNetworkTaskRequest::HostDescriptorFile {
            path: unique_path.clone(),
        });
        dispatcher.drain_and_execute(&mut game);
        wait_for_online_completion(&mut dispatcher, &mut game);

        assert_eq!(game.online_session_state, OnlineSessionUxState::Hosting);
        assert_eq!(game.run_mode, crate::game_state::RunMode::Title);
        assert!(game.message.contains("Host descriptor ready"));
        assert!(
            std::fs::read_to_string(&unique_path)
                .expect("descriptor file written")
                .contains("127.0.0.1:4242")
        );
        assert!(dispatcher.controller.is_none());
        assert!(dispatcher.pending_descriptor_accept.is_some());
        let _ignored = std::fs::remove_file(unique_path);
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn online_task_dispatcher_accepts_descriptor_join_after_host_publish() {
        let unique_path = std::env::temp_dir().join(format!(
            "drillgame-ui-host-join-descriptor-{}.json",
            std::process::id()
        ));
        let _ignored = std::fs::remove_file(&unique_path);
        let mut host_game = GameState::new();
        host_game.online_descriptor_path = unique_path.clone();
        host_game.online_host_bind_addr = "127.0.0.1:0".parse().expect("bind addr parses");
        host_game.online_host_advertise_addr =
            "127.0.0.1:0".parse().expect("advertise addr parses");
        let mut host_dispatcher = OnlineTaskDispatcher::new();
        host_game.online_network_task_request =
            Some(OnlineNetworkTaskRequest::HostDescriptorFile {
                path: unique_path.clone(),
            });
        host_dispatcher.drain_and_execute(&mut host_game);
        wait_for_online_completion(&mut host_dispatcher, &mut host_game);
        assert_eq!(
            host_game.online_session_state,
            OnlineSessionUxState::Hosting
        );
        assert!(host_dispatcher.pending_descriptor_accept.is_some());

        let mut join_game = GameState::new();
        let mut join_dispatcher = OnlineTaskDispatcher::new();
        join_game.online_network_task_request =
            Some(OnlineNetworkTaskRequest::JoinDescriptorFile {
                path: unique_path.clone(),
            });
        join_dispatcher.drain_and_execute(&mut join_game);
        for _ in 0..100 {
            host_dispatcher.drain_and_execute(&mut host_game);
            join_dispatcher.drain_and_execute(&mut join_game);
            if host_dispatcher.pending_descriptor_accept.is_none()
                && join_dispatcher.pending_completion.is_none()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert_eq!(
            host_game.online_session_state,
            OnlineSessionUxState::Connected
        );
        assert_ne!(host_game.run_mode, RunMode::Playing);
        assert_eq!(
            join_game.online_session_state,
            OnlineSessionUxState::Connected
        );
        assert_eq!(
            host_dispatcher
                .controller
                .as_ref()
                .expect("accepted host controller")
                .mode_label(),
            "descriptor-host-accepted"
        );
        assert_eq!(
            join_dispatcher
                .controller
                .as_ref()
                .expect("joined client controller")
                .mode_label(),
            "descriptor-client-connected"
        );

        let mut join_session = GameSession::new();
        join_session.game_mut().online_player_name = "Client QA".to_owned();
        join_session.game_mut().player.x = crate::game_state::TILE_SIZE * 18.0;
        join_session.game_mut().player.y = crate::game_state::TILE_SIZE * 19.0;
        join_session.game_mut().online_local_ready = true;
        join_dispatcher.drive_scheduled_tick(&mut join_session, FIXED_DELTA_SECONDS, Vec::new());
        assert!(
            join_session
                .game()
                .online_session_status_message
                .contains("Descriptor client sent live command tick")
        );
        let mut host_session = GameSession::new();
        host_session.game_mut().online_player_name = "Host QA".to_owned();
        {
            let host_player = host_session
                .world_mut()
                .player_mut(crate::multiplayer::LOCAL_PLAYER_ID)
                .expect("host local player exists");
            host_player.x = 123.0;
            host_player.y = 456.0;
            host_player.velocity_x = 7.0;
            host_player.velocity_y = -8.0;
            host_player.fuel = 321.0;
            host_player.hull = 654.0;
            host_player.credits = 999;
            host_player
                .cargo
                .insert(crate::terrain::MineralKind::Copper, 3);
            host_player
                .cargo
                .insert(crate::terrain::MineralKind::Gold, 2);
        }
        host_session
            .world_mut()
            .set_scanner_cooldown_seconds(crate::multiplayer::LOCAL_PLAYER_ID, 3.5);
        let terrain_position = crate::terrain::TilePosition { x: 18, y: 19 };
        host_session
            .world_mut()
            .terrain_mut()
            .set_kind(terrain_position, crate::terrain::TileKind::Lava);
        let _revisions = host_session.mark_world_terrain_tiles_changed([terrain_position]);
        host_session.game_mut().terrain.set_kind(
            crate::terrain::TilePosition { x: 18, y: 19 },
            crate::terrain::TileKind::Air,
        );
        join_session.game_mut().terrain.set_kind(
            crate::terrain::TilePosition { x: 18, y: 19 },
            crate::terrain::TileKind::Air,
        );
        host_session.game_mut().online_local_ready = true;
        host_session.game_mut().run_mode = RunMode::Playing;
        host_dispatcher.drive_scheduled_tick(&mut host_session, FIXED_DELTA_SECONDS, Vec::new());
        assert!(
            host_session
                .game()
                .online_session_status_message
                .contains("command=true")
        );
        assert!(host_session.game().online_remote_player_ready);
        assert_eq!(
            host_session.game().online_remote_player_name.as_deref(),
            Some("Client QA")
        );
        assert!(
            host_session
                .game()
                .online_last_replication_status
                .contains("host sent")
        );
        assert!(
            host_session
                .game()
                .online_last_terrain_status
                .contains("answered chunk")
        );
        assert_eq!(
            host_session.game().online_diagnostic_controller_mode,
            "descriptor-host-accepted"
        );
        assert!(
            host_session
                .game()
                .online_diagnostic_last_tick
                .contains("command=true")
        );
        join_dispatcher.drive_scheduled_tick(&mut join_session, FIXED_DELTA_SECONDS, Vec::new());
        assert!(
            join_session
                .game()
                .online_session_status_message
                .contains("received")
        );
        assert!(join_session.game().online_remote_player_ready);
        assert_eq!(
            join_session.game().online_remote_player_name.as_deref(),
            Some("Host QA")
        );
        assert_eq!(join_session.game().run_mode, RunMode::Playing);
        assert!((join_session.game().player.x - 123.0).abs() < f32::EPSILON);
        assert!((join_session.game().player.y - 456.0).abs() < f32::EPSILON);
        assert!((join_session.game().player.velocity_x - 7.0).abs() < f32::EPSILON);
        assert!((join_session.game().player.velocity_y + 8.0).abs() < f32::EPSILON);
        assert!((join_session.game().player.fuel - 321.0).abs() < f32::EPSILON);
        assert!((join_session.game().player.hull - 654.0).abs() < f32::EPSILON);
        assert_eq!(join_session.game().player.credits, 999);
        let joined_world_player = join_session
            .world()
            .player(crate::multiplayer::PlayerId::new(2))
            .expect("joined client local player synced into session world");
        assert!((joined_world_player.x - 123.0).abs() < f32::EPSILON);
        assert_eq!(joined_world_player.credits, 999);
        assert_eq!(
            join_session
                .game()
                .player
                .cargo
                .get(&crate::terrain::MineralKind::Copper),
            Some(&3)
        );
        assert_eq!(
            join_session
                .game()
                .player
                .cargo
                .get(&crate::terrain::MineralKind::Gold),
            Some(&2)
        );
        assert_eq!(join_session.game().player.cargo_used(), 5);
        assert!((join_session.game().scanner_cooldown_seconds - 3.5).abs() < f32::EPSILON);
        assert_eq!(join_session.game().online_remote_player_snapshots.len(), 1);
        let remote_player = &join_session.game().online_remote_player_snapshots[0];
        assert_eq!(remote_player.player_id, crate::multiplayer::LOCAL_PLAYER_ID);
        assert!((remote_player.x - 123.0).abs() < f32::EPSILON);
        assert!((remote_player.y - 456.0).abs() < f32::EPSILON);
        assert_eq!(remote_player.credits, 999);
        assert_eq!(remote_player.cargo_used, 5);
        assert_eq!(
            remote_player
                .cargo
                .get(&crate::terrain::MineralKind::Copper),
            Some(&3)
        );
        assert!(
            join_session
                .game()
                .online_multiplayer_status_lines()
                .iter()
                .any(|line| {
                    line.contains("Remote snapshot players")
                        && line.contains("p1")
                        && line.contains("Copperx3")
                })
        );
        assert!(
            join_session
                .game()
                .online_last_replication_status
                .contains("received")
        );
        assert!(
            join_session
                .game()
                .online_last_replicated_player_status
                .contains("tick")
        );
        assert!(
            join_session
                .game()
                .online_last_terrain_status
                .contains("applied chunk")
        );
        assert_eq!(
            join_session
                .game()
                .terrain
                .tile(crate::terrain::TilePosition { x: 18, y: 19 })
                .expect("replicated terrain tile exists")
                .kind,
            crate::terrain::TileKind::Lava
        );
        assert!(
            join_session
                .game()
                .visual_changes
                .changed_tiles
                .contains(&crate::terrain::TilePosition { x: 18, y: 19 })
        );
        assert_eq!(
            join_session.game().online_diagnostic_controller_mode,
            "descriptor-client-connected"
        );

        host_session.game_mut().online_network_task_request =
            Some(OnlineNetworkTaskRequest::Shutdown);
        host_dispatcher.drain_and_execute(host_session.game_mut());
        assert_eq!(
            host_session.game().online_session_state,
            OnlineSessionUxState::Shutdown
        );
        join_dispatcher.drive_scheduled_tick(&mut join_session, FIXED_DELTA_SECONDS, Vec::new());
        assert_eq!(
            join_session.game().online_session_state,
            OnlineSessionUxState::Shutdown
        );
        assert!(
            join_session
                .game()
                .online_session_status_message
                .contains("ended by host")
        );
        let _ignored = std::fs::remove_file(unique_path);
    }

    #[test]
    fn online_task_dispatcher_reports_missing_join_descriptor_file() {
        let mut game = GameState::new();
        let mut dispatcher = OnlineTaskDispatcher::new();
        game.online_network_task_request = Some(OnlineNetworkTaskRequest::JoinDescriptorFile {
            path: std::path::PathBuf::from("/tmp/drillgame-missing-join-descriptor.json"),
        });

        dispatcher.drain_and_execute(&mut game);
        wait_for_online_completion(&mut dispatcher, &mut game);

        assert_eq!(game.online_session_state, OnlineSessionUxState::Error);
        assert!(game.message.contains("host descriptor could not be read"));
        assert!(game.message.contains("descriptor file/path"));
        assert!(game.online_network_task_request.is_none());
    }

    #[test]
    fn online_task_dispatcher_builds_tick_payload_from_live_session_state() {
        let mut dispatcher = OnlineTaskDispatcher::new();
        let mut session = GameSession::new();
        let input = dispatcher.live_session_tick_input(&mut session, Vec::new());
        let packet = input.command_packet.expect("command packet");
        let command = packet.commands.first().expect("sequenced command");

        assert_eq!(packet.client_id, session.local_client().client_id);
        assert_eq!(
            command.player_id,
            session.local_client().controlled_player_id
        );
        assert_eq!(command.target_tick.get(), session.current_tick().get() + 1);
        assert!(!input.snapshot.expect("snapshot").players.is_empty());
    }

    #[test]
    fn online_task_dispatcher_reports_reconnect_without_active_session() {
        let mut game = GameState::new();
        let mut dispatcher = OnlineTaskDispatcher::new();

        game.online_network_task_request = Some(OnlineNetworkTaskRequest::ReconnectDirectConnect);
        dispatcher.drain_and_execute(&mut game);

        assert_eq!(game.online_session_state, OnlineSessionUxState::Error);
        assert!(game.message.contains("Rejoin"));
    }
}
