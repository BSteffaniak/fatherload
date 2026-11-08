//! One frame of local play, independent of the window (plan §10 frame loop).
//!
//! `sample input -> shell actions to shell state, gameplay input per local seat ->
//! runtime.advance(dt) -> RenderSnapshot -> render`. The raylib app only samples devices, calls
//! [`LocalGame::frame`] and draws [`LocalGame::snapshot`], so tests drive the exact same path with
//! scripted inputs.

use std::{net::SocketAddr, time::Duration};

use drillgame_netcode::ReconnectToken;
use drillgame_quic_transport::{HostDescriptor, JoinTarget, lan::LanDiscoveredGame};
use drillgame_simulation::{PlayerCommand, PlayerId, WorldState};

use crate::{
    effects::{Effects, EventFeed},
    input::PlayerInput,
    input_mapping::seat_input,
    online::{
        self, DEFAULT_PORT,
        client::OnlineClient,
        lan::{self, Background},
    },
    render_snapshot::{RenderSnapshot, ScreenSize},
    runtime::{
        ClientState, LocalHost, LocalSeat, OnlineRole, Runtime, RuntimeEvent, RuntimeEventKind,
        SeatInput,
    },
    save::{self, SaveError, SettingsFile, WorldSave},
    shell::{OnlineScreen, RunMode, ShellContext, ShellRequest, ShellState, SoundCue},
};

/// How often an online host autosaves the session (clients never save).
const AUTOSAVE_SECONDS: f32 = 300.0;

/// Where saves go. The app uses the real save directory; tests use a temp path.
#[derive(Clone, Debug)]
pub enum SaveTarget {
    /// The user's save files (main file + slots).
    UserFiles,
    /// A single file, used for the main save and every slot (tests).
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "headless tests save to a temp file")
    )]
    File(std::path::PathBuf),
}

/// One frame of sampled input: the primary seat's full input (also drives the shell) and the
/// gameplay input of every further seat, in seat order.
#[derive(Clone, Debug, Default)]
pub struct FrameInput {
    pub primary: PlayerInput,
    pub other_seats: Vec<PlayerInput>,
    /// Clipboard text, sampled when the primary seat pressed paste.
    pub clipboard: Option<String>,
}

impl FrameInput {
    #[must_use]
    pub const fn primary(primary: PlayerInput) -> Self {
        Self {
            primary,
            other_seats: Vec::new(),
            clipboard: None,
        }
    }
}

/// What a frame did that the window layer must react to.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FrameReport {
    /// Fixed ticks simulated.
    pub ticks_run: u32,
    /// The world was replaced (new game / load): redraw all terrain.
    pub world_replaced: bool,
    /// Terrain tiles changed this frame.
    pub changed_tiles: Vec<drillgame_simulation::terrain::TilePosition>,
    /// Sounds to play.
    pub sound_cues: Vec<SoundCue>,
    /// Settings changed and should be persisted.
    pub settings_changed: bool,
    /// Text to put on the system clipboard.
    pub clipboard: Option<String>,
}

/// Shell + runtime + presentation effects for local play.
#[derive(Debug)]
pub struct LocalGame {
    pub shell: ShellState,
    runtime: Runtime,
    events: EventFeed,
    effects: Effects,
    frame_events: Vec<RuntimeEvent>,
    save_target: SaveTarget,
    /// Where the last join went and the token to reclaim the player there.
    last_join: Option<(JoinTarget, Option<ReconnectToken>)>,
    lan_browse: Option<Background<Vec<LanDiscoveredGame>>>,
    lan_fetch: Option<Background<HostDescriptor>>,
    client_entered: bool,
    autosave_seconds: f32,
    host_bind: SocketAddr,
}

impl LocalGame {
    /// A game on the title screen with a fresh world idling behind it.
    #[must_use]
    pub fn new(settings: SettingsFile, save_target: SaveTarget) -> Self {
        Self {
            shell: ShellState::new(settings),
            runtime: Runtime::local(new_world(), 1),
            events: EventFeed::default(),
            effects: Effects::default(),
            frame_events: Vec::new(),
            save_target,
            last_join: None,
            lan_browse: None,
            lan_fetch: None,
            client_entered: false,
            autosave_seconds: 0.0,
            host_bind: SocketAddr::new(std::net::Ipv4Addr::UNSPECIFIED.into(), DEFAULT_PORT),
        }
    }

    /// Uses `bind` for hosting instead of the default port (tests bind an ephemeral port).
    #[cfg(test)]
    pub const fn set_host_bind(&mut self, bind: SocketAddr) {
        self.host_bind = bind;
    }

    /// Ends any online session cleanly before the app closes (the host tells its clients, a
    /// client leaves), waiting for the network thread to flush.
    pub fn shutdown(self) {
        match self.runtime {
            Runtime::Host(mut host) => {
                host.end_session();
                if let Some(net) = host.take_net() {
                    net.shutdown_blocking();
                }
            }
            Runtime::Client(client) => client.leave_blocking(),
        }
    }

    /// The gameplay authority.
    #[cfg(test)]
    #[must_use]
    pub const fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// The world the runtime exposes.
    #[must_use]
    pub fn world(&self) -> &WorldState {
        self.runtime.world()
    }

    /// Local seats in order.
    #[must_use]
    pub fn seats(&self) -> &[LocalSeat] {
        self.runtime.local_seats()
    }

    /// Whether the app should close.
    #[must_use]
    pub const fn should_exit(&self) -> bool {
        self.shell.request_exit
    }

    /// Runs one frame.
    pub fn frame(&mut self, input: &FrameInput, delta_seconds: f32) -> FrameReport {
        let mut report = FrameReport::default();
        self.runtime.pump_network(delta_seconds);
        self.sync_online(&mut report);
        self.poll_lan(&mut report);
        self.shell.pasted.clone_from(&input.clipboard);
        let primary_player = self.seats().first().map(|seat| seat.player);
        let on_title = self.shell.run_mode == RunMode::Title;
        let requests = self.shell.update(
            input.primary,
            delta_seconds,
            shell_context(&self.runtime, &self.save_target, primary_player, on_title),
        );
        let mut primary_actions = Vec::new();
        for request in requests {
            self.handle_request(request, &mut primary_actions, &mut report);
        }

        let gameplay = self.shell.gameplay_input_enabled();
        let seats = self.seats().to_vec();
        for (index, seat) in seats.iter().enumerate() {
            let sampled = if index == 0 {
                Some(&input.primary)
            } else {
                input.other_seats.get(index - 1)
            };
            let mut seat_frame = match sampled {
                Some(sampled) if gameplay => seat_input(sampled),
                _ => SeatInput::default(),
            };
            if index == 0 {
                seat_frame.actions.append(&mut primary_actions);
            } else if gameplay
                && let Some(sampled) = sampled
                && (sampled.interact || sampled.confirm)
                && self.world().failure_state(seat.player).is_some()
            {
                seat_frame.actions.push(PlayerCommand::Rescue);
            }
            self.runtime.submit(seat.seat, seat_frame);
        }

        self.runtime.set_paused(!self.shell.world_should_tick());
        let frame = self
            .runtime
            .advance(Duration::try_from_secs_f32(delta_seconds).unwrap_or_default());
        report.ticks_run = frame.ticks_run;
        report.world_replaced |= frame.world_replaced;
        report.changed_tiles.extend(frame.changed_tiles);
        self.frame_events = self.events.present(frame.events);
        self.observe_events(primary_player);
        let local_players: Vec<PlayerId> = self.seats().iter().map(|seat| seat.player).collect();
        self.effects.update(delta_seconds);
        let effect_cues =
            self.effects
                .apply(&self.frame_events, self.runtime.world(), &local_players);
        report
            .sound_cues
            .extend(self.shell.sound_cues.iter().copied());
        report.sound_cues.extend(effect_cues);
        report.settings_changed = self.shell.take_settings_dirty();
        self.autosave(delta_seconds);
        report
    }

    /// The immutable view of this frame for rendering.
    #[must_use]
    pub fn snapshot(&self, screen: impl Into<ScreenSize>) -> RenderSnapshot<'_> {
        let mut snapshot = RenderSnapshot::build(
            self.runtime.world(),
            self.runtime.local_seats(),
            screen,
            (self.effects.sparks(), self.effects.explosions()),
            &self.frame_events,
        );
        // A client draws its rig where prediction + smoothing put it (the camera follows that
        // same transform) and remote rigs where interpolation puts them.
        snapshot.apply_pose_overrides(&self.runtime.pose_overrides());
        snapshot
    }

    /// Replaces the session with a fresh local world of `seats` seats, lets `arrange` set up that
    /// world (rig positions, terrain, cargo...) and enters it. Scene capture uses this to build
    /// scripted states; gameplay never calls it.
    pub fn stage_local_world(
        &mut self,
        seats: usize,
        arrange: impl FnOnce(&mut WorldState, &[LocalSeat]),
    ) -> FrameReport {
        let mut runtime = Runtime::local(new_world(), seats);
        let staged_seats = runtime.local_seats().to_vec();
        let mut world = runtime.world().clone();
        arrange(&mut world, &staged_seats);
        runtime.load_world(world);
        end_runtime(std::mem::replace(&mut self.runtime, runtime));
        let mut report = FrameReport::default();
        self.reset_presentation(&mut report);
        self.shell.enter_world(String::new(), 0.0);
        report
    }

    /// The current world as a save.
    #[must_use]
    pub fn world_save(&self) -> WorldSave {
        WorldSave {
            world: self.runtime.world().clone(),
            play_seconds: self.shell.play_seconds,
        }
    }

    fn observe_events(&mut self, primary_player: Option<PlayerId>) {
        let resolved: Vec<_> = self
            .frame_events
            .iter()
            .filter(|event| event.player.is_some() && event.player == primary_player)
            .filter_map(|event| match &event.event {
                RuntimeEventKind::ActionResolved {
                    command: PlayerCommand::Service(request),
                    applied,
                } => Some((*request, *applied)),
                _ => None,
            })
            .collect();
        for (request, applied) in resolved {
            self.shell.service_resolved(
                request,
                applied,
                shell_context(&self.runtime, &self.save_target, primary_player, false),
            );
        }
    }

    fn handle_request(
        &mut self,
        request: ShellRequest,
        primary_actions: &mut Vec<PlayerCommand>,
        report: &mut FrameReport,
    ) {
        match request {
            ShellRequest::NewGame { seats } => self.new_game(seats, report),
            ShellRequest::ResumeLatest => {
                let loaded = match &self.save_target {
                    SaveTarget::UserFiles => save::load_latest_world(),
                    SaveTarget::File(path) => save::load_world_from_path(path),
                };
                self.apply_load(loaded, "Resumed saved session.", report);
            }
            ShellRequest::QuickLoad => {
                let loaded = match &self.save_target {
                    SaveTarget::UserFiles => save::load_world(),
                    SaveTarget::File(path) => save::load_world_from_path(path),
                };
                self.apply_load(loaded, "Game loaded.", report);
            }
            ShellRequest::LoadSlot(slot) => {
                let loaded = match &self.save_target {
                    SaveTarget::UserFiles => save::load_world_slot(slot),
                    SaveTarget::File(path) => save::load_world_from_path(path),
                };
                self.apply_load(loaded, &format!("Loaded slot {}.", slot + 1), report);
            }
            ShellRequest::QuickSave => {
                let result = self.write_save(None);
                self.report_save(result, "Game saved.");
            }
            ShellRequest::SaveSlot(slot) => {
                let result = self.write_save(Some(slot));
                self.report_save(result, &format!("Saved slot {}.", slot + 1));
            }
            ShellRequest::SaveAndExit => match self.write_save(None) {
                Ok(()) => self.shell.request_exit = true,
                Err(error) => self.shell.message = format!("Save before exit failed: {error}"),
            },
            ShellRequest::Action(command) => primary_actions.push(command),
            ShellRequest::AddLocalSeat => self.add_local_seat(),
            ShellRequest::HostOnline => self.host_online(report),
            ShellRequest::OpenOnline => self.open_online(),
            ShellRequest::JoinAddress(text) => match parse_join_addr(&text) {
                Some(addr) => self.start_join(JoinTarget::Addr(addr), None, report),
                None => self.shell.message = format!("Not an address: {text}"),
            },
            ShellRequest::JoinDescriptor(text) => match HostDescriptor::from_json(&text) {
                Ok(descriptor) => self.start_join(JoinTarget::Descriptor(descriptor), None, report),
                Err(error) => self.shell.message = format!("Not a host descriptor: {error}"),
            },
            ShellRequest::BrowseLan => self.lan_browse = Some(lan::browse()),
            ShellRequest::JoinLan(index) => {
                if let Some(game) = self.shell.online.lan_games.get(index) {
                    self.shell.online.lan_status = format!("Contacting {}...", game.instance_name);
                    self.lan_fetch = Some(lan::fetch(game));
                }
            }
            ShellRequest::SetReady(ready) => self.runtime.set_ready(ready),
            ShellRequest::StartOnline => {
                self.runtime.start_online_game();
                self.shell
                    .enter_world("The session started. Good digging!", 0.0);
                self.shell.sound_cues.push(SoundCue::Milestone);
            }
            ShellRequest::LeaveOnline => self.leave_online(report),
            ShellRequest::Reconnect => {
                if let Some((target, token)) = self.last_join.clone() {
                    self.start_join(target, token, report);
                }
            }
            ShellRequest::CopyToClipboard(text) => report.clipboard = Some(text),
            ShellRequest::AdjustTuning { parameter, step } => {
                self.shell.message = self.runtime.adjust_tuning(parameter, step);
            }
            ShellRequest::Exit => self.shell.request_exit = true,
        }
    }

    fn new_game(&mut self, seats: usize, report: &mut FrameReport) {
        self.runtime = Runtime::local(new_world(), seats);
        self.reset_presentation(report);
        self.shell.enter_world(
            "Welcome to the dig site. Visit the depot for contracts.",
            0.0,
        );
        if seats > 1 {
            self.shell.message = format!(
                "Local split-screen with {seats} players: Player 1 WASD, Player 2 arrow keys / gamepad."
            );
        }
        self.shell.sound_cues.push(SoundCue::Milestone);
    }

    fn add_local_seat(&mut self) {
        if let Some(seat) = self.runtime.add_local_seat() {
            self.shell.message = format!(
                "Player {} joined local split-screen ({} seats).",
                seat.seat.0 + 1,
                self.seats().len()
            );
        } else {
            self.shell.message = format!(
                "Local split-screen is limited to {} seats.",
                crate::runtime::MAX_LOCAL_SEATS
            );
        }
    }

    fn leave_online(&mut self, report: &mut FrameReport) {
        let message = match self.runtime.online_role() {
            Some(OnlineRole::Host) => "You ended the online session.",
            Some(OnlineRole::Client) => "You left the online session.",
            None => "",
        };
        self.go_local(report);
        self.shell.return_to_title(message);
    }

    /// Mirrors the runtime's online facts into the shell and reacts to a client's session
    /// ending (host ended it, rejected us, connection lost).
    fn sync_online(&mut self, report: &mut FrameReport) {
        self.shell.online.role = self.runtime.online_role();
        self.shell.online.lobby = self.runtime.lobby_info();
        for notice in self.runtime.take_notices() {
            self.shell.message = notice;
        }
        if self.shell.online.show_net_debug {
            self.shell.online.net_debug = self.runtime.net_debug_lines();
        }
        if let Some(error) = self.runtime.host_failure() {
            self.go_local(report);
            self.shell
                .online_notice(format!("Could not host: {error}"), false);
            return;
        }
        match self.runtime.client_state() {
            Some(ClientState::InGame) if !self.client_entered => {
                self.client_entered = true;
                if self.shell.run_mode == RunMode::Lobby
                    && self.shell.online.screen == OnlineScreen::Lobby
                {
                    self.shell.enter_world("Joined the session.", 0.0);
                    self.shell.sound_cues.push(SoundCue::Milestone);
                }
            }
            Some(ClientState::Finished {
                message,
                can_reconnect,
            }) => {
                if let Runtime::Client(client) = &self.runtime {
                    self.last_join = Some((client.target().clone(), client.reconnect_token()));
                }
                self.go_local(report);
                self.shell.online_notice(message, can_reconnect);
            }
            _ => {}
        }
    }

    fn poll_lan(&mut self, report: &mut FrameReport) {
        if let Some(result) = self.lan_browse.as_ref().and_then(Background::poll) {
            self.lan_browse = None;
            match result {
                Ok(games) => {
                    self.shell.online.lan_status = if games.is_empty() {
                        "No LAN games found. Refresh to search again.".to_owned()
                    } else {
                        format!("Found {} LAN game(s).", games.len())
                    };
                    self.shell.online.lan_games = games;
                }
                Err(error) => {
                    self.shell.online.lan_status = format!("LAN discovery failed: {error}");
                }
            }
        }
        if let Some(result) = self.lan_fetch.as_ref().and_then(Background::poll) {
            self.lan_fetch = None;
            match result {
                Ok(descriptor) => {
                    self.start_join(JoinTarget::Descriptor(descriptor), None, report);
                }
                Err(error) => {
                    self.shell.online.lan_status = format!("Could not reach that game: {error}");
                }
            }
        }
    }

    fn host_online(&mut self, report: &mut FrameReport) {
        match LocalHost::online(new_world(), 1, self.host_bind, true) {
            Ok(mut host) => {
                host.set_host_name(&online::player_name());
                self.go_online(Runtime::Host(Box::new(host)), report);
                self.shell.online.ready = false;
                self.shell.show_online(OnlineScreen::Lobby);
                "Hosting: share the address or descriptor, then start the game."
                    .clone_into(&mut self.shell.message);
            }
            Err(error) => self.shell.message = format!("Could not host: {error}"),
        }
    }

    fn open_online(&mut self) {
        let bind = self.host_bind;
        let Runtime::Host(host) = &mut self.runtime else {
            return;
        };
        match host.open_online(bind, true) {
            Ok(()) => {
                host.set_host_name(&online::player_name());
                "Opened to LAN/online. Pause > Online Session shows the address."
                    .clone_into(&mut self.shell.message);
            }
            Err(error) => self.shell.message = format!("Could not open the game: {error}"),
        }
    }

    fn start_join(
        &mut self,
        target: JoinTarget,
        token: Option<ReconnectToken>,
        report: &mut FrameReport,
    ) {
        let fallback = self.runtime.world().clone();
        match OnlineClient::join(target.clone(), online::player_name(), token, fallback) {
            Ok(client) => {
                self.last_join = Some((target, token));
                self.go_online(Runtime::Client(Box::new(client)), report);
                self.shell.online.ready = false;
                self.shell.show_online(OnlineScreen::Lobby);
                "Connecting...".clone_into(&mut self.shell.message);
            }
            Err(error) => self.shell.message = format!("Could not join: {error}"),
        }
    }

    /// Replaces the runtime with an online one, ending any previous session.
    fn go_online(&mut self, runtime: Runtime, report: &mut FrameReport) {
        let previous = std::mem::replace(&mut self.runtime, runtime);
        end_runtime(previous);
        self.client_entered = false;
        self.autosave_seconds = 0.0;
        self.reset_presentation(report);
    }

    /// Ends the online session (host: tell clients; client: leave) and idles a fresh local world.
    fn go_local(&mut self, report: &mut FrameReport) {
        self.go_online(Runtime::local(new_world(), 1), report);
    }

    fn autosave(&mut self, delta_seconds: f32) {
        let hosting = self.runtime.online_role() == Some(OnlineRole::Host)
            && self.shell.online.started()
            && !matches!(self.shell.run_mode, RunMode::Title);
        if !hosting {
            self.autosave_seconds = 0.0;
            return;
        }
        self.autosave_seconds += delta_seconds;
        if self.autosave_seconds >= AUTOSAVE_SECONDS {
            self.autosave_seconds = 0.0;
            let result = self.write_save(None);
            self.report_save(result, "Autosaved the session.");
        }
    }

    fn write_save(&self, slot: Option<usize>) -> Result<(), SaveError> {
        if !self.runtime.can_save() {
            return Err(SaveError::SaveDenied(
                "only the host can write the session save".to_owned(),
            ));
        }
        let save = self.world_save();
        match (&self.save_target, slot) {
            (SaveTarget::UserFiles, None) => save::save_world(&save),
            (SaveTarget::UserFiles, Some(slot)) => save::save_world_slot(&save, slot),
            (SaveTarget::File(path), _) => save::save_world_to_path(&save, path),
        }
    }

    fn report_save(&mut self, result: Result<(), SaveError>, success: &str) {
        match result {
            Ok(()) => {
                self.shell.save_dirty = false;
                success.clone_into(&mut self.shell.message);
                self.shell.sound_cues.push(SoundCue::Ui);
            }
            Err(error) => self.shell.message = format!("Save failed: {error}"),
        }
    }

    fn apply_load(
        &mut self,
        loaded: Result<WorldSave, SaveError>,
        success: &str,
        report: &mut FrameReport,
    ) {
        match loaded {
            Ok(save) => {
                if self.shell.run_mode == RunMode::Title {
                    // From the title, a load starts a fresh single-seat session.
                    self.runtime = Runtime::local(save.world, 1);
                } else if !self.runtime.load_world(save.world) {
                    let refusal = if self.runtime.can_save() {
                        "That save runs at another tick rate than this online session."
                    } else {
                        "Joined clients cannot load over the host's session."
                    };
                    refusal.clone_into(&mut self.shell.message);
                    return;
                }
                self.reset_presentation(report);
                self.shell.enter_world(success, save.play_seconds);
                self.shell.sound_cues.push(SoundCue::Ui);
            }
            Err(error) => self.shell.message = format!("Load failed: {error}"),
        }
    }

    fn reset_presentation(&mut self, report: &mut FrameReport) {
        self.effects.clear();
        self.events = EventFeed::default();
        self.frame_events.clear();
        report.world_replaced = true;
    }
}

/// Ends a runtime that is being replaced: a host ends its online session for its clients, a
/// client leaves (the network threads stop in the background).
fn end_runtime(runtime: Runtime) {
    match runtime {
        Runtime::Host(mut host) => host.end_session(),
        Runtime::Client(client) => client.leave(),
    }
}

/// Parses `ip:port`, or a bare IP with the default port.
fn parse_join_addr(text: &str) -> Option<SocketAddr> {
    let text = text.trim();
    text.parse().ok().or_else(|| {
        text.parse::<std::net::IpAddr>()
            .ok()
            .map(|ip| SocketAddr::new(ip, DEFAULT_PORT))
    })
}

/// The shell's read-only view of the world. `probe_saves` controls whether save files are looked
/// up on disk: only the title menu reads `saves_exist`, so gameplay frames skip the file-system
/// probes on the game thread.
fn shell_context<'a>(
    runtime: &'a Runtime,
    save_target: &SaveTarget,
    player: Option<PlayerId>,
    probe_saves: bool,
) -> ShellContext<'a> {
    ShellContext {
        world: runtime.world(),
        player,
        saves_exist: probe_saves
            && match save_target {
                SaveTarget::UserFiles => save::saves_exist(),
                SaveTarget::File(path) => path.exists(),
            },
    }
}

/// A brand-new single-rig world.
#[must_use]
pub fn new_world() -> WorldState {
    WorldState::new_game(save::LEGACY_PLAYER_ID)
}

#[cfg(test)]
mod tests {
    //! Headless app-level tests: scripted input through `LocalGame::frame`, exactly the path the
    //! window drives, with the runtime's `HostSession` as the only gameplay authority.

    use drillgame_simulation::{
        economy::SurfaceZone,
        surface::SURFACE_BUILDINGS,
        terrain::{MineralKind, TILE_SIZE},
    };

    use super::*;
    use crate::{
        render_snapshot::{SCREEN_HEIGHT, SCREEN_WIDTH},
        runtime::MAX_LOCAL_SEATS,
        shell::{ModalScreen, PauseOption, TitleOption, interior_service_x},
    };

    const DT: f32 = 1.0 / 60.0;
    const PLAYER: PlayerId = save::LEGACY_PLAYER_ID;

    fn temp_save(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "drillgame-local-game-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ))
    }

    fn title_game(name: &str) -> LocalGame {
        LocalGame::new(SettingsFile::default(), SaveTarget::File(temp_save(name)))
    }

    /// A game already playing on `world` with `seats` seats.
    fn playing(world: WorldState, seats: usize, name: &str) -> LocalGame {
        let mut game = title_game(name);
        game.runtime = Runtime::local(world, seats);
        game.shell.enter_world("test", 0.0);
        game
    }

    fn step(game: &mut LocalGame, input: PlayerInput) -> FrameReport {
        game.frame(&FrameInput::primary(input), DT)
    }

    fn idle(game: &mut LocalGame, frames: usize) {
        for _ in 0..frames {
            let _report = step(game, PlayerInput::default());
        }
    }

    fn press(game: &mut LocalGame, input: PlayerInput) -> FrameReport {
        let report = step(game, input);
        idle(game, 1);
        report
    }

    fn player(game: &LocalGame, id: PlayerId) -> drillgame_simulation::Player {
        game.world().player(id).expect("player exists").clone()
    }

    fn rig_at_building(
        zone: SurfaceZone,
        configure: impl FnOnce(&mut drillgame_simulation::Player),
    ) -> WorldState {
        let building = SURFACE_BUILDINGS
            .iter()
            .find(|building| building.zone == zone)
            .expect("building exists");
        let mut world = new_world();
        let rig = world.player_mut(PLAYER).expect("rig");
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinate")]
        let x = (building.tile_x as f32 + 2.5) * TILE_SIZE;
        rig.x = x;
        configure(rig);
        world
    }

    /// Enters the building the rig stands at and walks to its service counter.
    fn walk_to_counter(game: &mut LocalGame, zone: SurfaceZone) {
        let _report = press(
            game,
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Interior);
        assert_eq!(game.shell.interior_zone, Some(zone));
        let target = interior_service_x(zone);
        for _ in 0..600 {
            let delta = target - game.shell.interior_x;
            if delta.abs() < 10.0 {
                break;
            }
            let _report = step(
                game,
                PlayerInput {
                    horizontal: delta.signum(),
                    ..PlayerInput::default()
                },
            );
        }
        let _report = press(
            game,
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
        );
    }

    fn confirm(game: &mut LocalGame) {
        let _report = press(
            game,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        // Let the submitted transaction tick through the host.
        idle(game, 3);
    }

    #[test]
    fn new_game_from_title_runs_a_single_seat_host() {
        let mut game = title_game("new-game");
        assert_eq!(game.shell.run_mode, RunMode::Title);
        let idle_report = step(&mut game, PlayerInput::default());
        assert_eq!(idle_report.ticks_run, 0, "the world idles behind the title");

        let report = press(
            &mut game,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        assert!(report.world_replaced);
        assert_eq!(game.shell.run_mode, RunMode::Playing);
        assert!(matches!(game.runtime(), Runtime::Host(_)));
        assert_eq!(game.seats().len(), 1);
        assert_eq!(game.seats()[0].player, PLAYER);
        assert_eq!(game.runtime().epoch(), 1);
    }

    #[test]
    fn held_movement_moves_the_rig_and_the_camera_follows() {
        let mut game = playing(new_world(), 1, "move");
        idle(&mut game, 30);
        let start = player(&game, PLAYER);
        let mut ticks = 0;
        for _ in 0..60 {
            ticks += step(
                &mut game,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            )
            .ticks_run;
        }
        let moved = player(&game, PLAYER);
        assert!(moved.x > start.x + TILE_SIZE, "{} -> {}", start.x, moved.x);
        assert!(
            (55..=65).contains(&ticks),
            "fixed ticks track real time: {ticks}"
        );

        let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
        let view = snapshot.primary_view().expect("one view");
        assert_eq!(snapshot.views.len(), 1);
        assert!((view.camera.x - (moved.x - 640.0)).abs() < f32::EPSILON);
        assert!((view.camera.y - (moved.y - 360.0)).abs() < f32::EPSILON);
        assert!((view.hud.fuel - moved.fuel).abs() < f32::EPSILON);
    }

    #[test]
    fn drilling_down_digs_terrain_and_reports_changed_tiles() {
        let mut world = new_world();
        // Centre the rig over a column so it fits down the shaft it digs.
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.x = (rig.x / TILE_SIZE)
            .floor()
            .mul_add(TILE_SIZE, TILE_SIZE / 2.0);
        let mut game = playing(world, 1, "drill");
        idle(&mut game, 30);
        let start = player(&game, PLAYER);
        let mut changed = Vec::new();
        for _ in 0..600 {
            let report = step(
                &mut game,
                PlayerInput {
                    drill_down: true,
                    ..PlayerInput::default()
                },
            );
            changed.extend(report.changed_tiles);
        }
        let end = player(&game, PLAYER);
        assert!(!changed.is_empty(), "drilling changed terrain");
        assert!(
            end.y > start.y + TILE_SIZE,
            "rig dug down: {} -> {}",
            start.y,
            end.y
        );
        let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
        let view = snapshot.primary_view().expect("view");
        assert!(
            view.is_explored(end.tile_position(TILE_SIZE)),
            "ticks reveal tiles"
        );
    }

    #[test]
    fn selling_cargo_at_the_depot_goes_through_the_runtime() {
        let world = rig_at_building(SurfaceZone::Depot, |rig| {
            rig.cargo.insert(MineralKind::Gold, 3);
            rig.credits = 0;
        });
        let mut game = playing(world, 1, "sell");
        idle(&mut game, 5);
        walk_to_counter(&mut game, SurfaceZone::Depot);
        assert_eq!(game.shell.modal, Some(ModalScreen::Depot));
        assert_eq!(game.shell.selected_menu_item, 1, "depot opens on 'sell'");
        let tick_before = game.world().simulation_tick();
        confirm(&mut game);

        let rig = player(&game, PLAYER);
        assert!(rig.cargo.is_empty(), "cargo sold");
        assert!(rig.credits > 0, "credits earned");
        assert!(
            game.shell.message.starts_with("Sold cargo for"),
            "{}",
            game.shell.message
        );
        assert!(
            game.world().simulation_tick() > tick_before,
            "the sale resolved inside a fixed tick"
        );
        assert_eq!(game.shell.depot_receipts.len(), 1);
        assert!(
            !game.shell.world_should_tick(),
            "the interior freezes the world again"
        );
    }

    #[test]
    fn buying_an_upgrade_at_the_shop_goes_through_the_runtime() {
        let world = rig_at_building(SurfaceZone::Shop, |rig| rig.credits = 10_000);
        let mut game = playing(world, 1, "buy");
        idle(&mut game, 5);
        let before = player(&game, PLAYER);
        walk_to_counter(&mut game, SurfaceZone::Shop);
        assert_eq!(game.shell.modal, Some(ModalScreen::Shop));
        confirm(&mut game);
        assert_eq!(game.shell.modal, Some(ModalScreen::ShopConfirm));
        confirm(&mut game);

        let after = player(&game, PLAYER);
        assert_eq!(after.drill_strength, before.drill_strength + 1);
        assert!(after.credits < before.credits);
        assert!(
            game.shell.message.starts_with("Bought"),
            "{}",
            game.shell.message
        );

        // Leaving the building resumes play.
        let _report = press(
            &mut game,
            PlayerInput {
                cancel: true,
                ..PlayerInput::default()
            },
        );
        let _report = press(
            &mut game,
            PlayerInput {
                cancel: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Playing);
    }

    #[test]
    fn save_and_load_round_trip_restores_the_world_in_a_new_epoch() {
        let path = temp_save("round-trip");
        let mut game = LocalGame::new(SettingsFile::default(), SaveTarget::File(path.clone()));
        let _report = press(
            &mut game,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        for _ in 0..40 {
            let _report = step(
                &mut game,
                PlayerInput {
                    horizontal: -1.0,
                    ..PlayerInput::default()
                },
            );
        }
        idle(&mut game, 30);
        let _report = press(
            &mut game,
            PlayerInput {
                save: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.message, "Game saved.");
        assert!(path.exists());
        let saved = save::load_world_from_path(&path).expect("save readable");
        let saved_hash = saved.world.simulation_state_hash();
        let epoch_before = game.runtime().epoch();

        for _ in 0..60 {
            let _report = step(
                &mut game,
                PlayerInput {
                    horizontal: 1.0,
                    thrust: true,
                    ..PlayerInput::default()
                },
            );
        }
        assert_ne!(game.world().simulation_state_hash(), saved_hash);

        let mut load = PlayerInput {
            load: true,
            ..PlayerInput::default()
        };
        // Pause first so no tick runs between the load and the comparison.
        let _report = press(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        load.pause = false;
        let report = step(&mut game, load);
        let _ignored = std::fs::remove_file(&path);
        assert!(report.world_replaced);
        assert_eq!(
            game.runtime().epoch(),
            epoch_before + 1,
            "loading starts a new epoch"
        );
        assert_eq!(game.shell.run_mode, RunMode::Playing);
        // The load resumed play, so at most this frame's ticks ran on top of the save.
        assert!(report.ticks_run <= 1);
        if report.ticks_run == 0 {
            assert_eq!(game.world().simulation_state_hash(), saved_hash);
        }
        assert_eq!(
            player(&game, PLAYER).credits,
            saved.world.player(PLAYER).expect("saved rig").credits
        );
        assert_eq!(game.seats().len(), 1);
    }

    #[test]
    fn local_split_screen_runs_two_independent_seats() {
        let mut game = title_game("split-two");
        let _report = press(
            &mut game,
            PlayerInput {
                menu_down: true,
                ..PlayerInput::default()
            },
        );
        let _report = press(
            &mut game,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.seats().len(), 2);
        let first = game.seats()[0].player;
        let second = game.seats()[1].player;
        assert_ne!(first, second);
        let world_players: Vec<PlayerId> = game.world().player_ids().collect();
        assert_eq!(world_players, vec![first, second], "no orphan rigs");
        idle(&mut game, 20);
        let a0 = player(&game, first);
        let b0 = player(&game, second);
        for _ in 0..45 {
            let _report = game.frame(
                &FrameInput {
                    clipboard: None,
                    primary: PlayerInput::default(),
                    other_seats: vec![PlayerInput {
                        horizontal: -1.0,
                        ..PlayerInput::default()
                    }],
                },
                DT,
            );
        }
        let a1 = player(&game, first);
        let b1 = player(&game, second);
        assert!((a1.x - a0.x).abs() < 1.0, "seat 1 stayed put");
        assert!(b1.x < b0.x - TILE_SIZE, "seat 2 moved left");

        let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
        assert_eq!(snapshot.views.len(), 2);
        for view in &snapshot.views {
            let rig = player(&game, view.player_id);
            #[allow(clippy::cast_precision_loss, reason = "viewport size")]
            let expected = rig.x - view.viewport.width as f32 / 2.0;
            assert!((view.camera.x - expected).abs() < f32::EPSILON);
        }
        assert_ne!(snapshot.views[0].viewport, snapshot.views[1].viewport);
    }

    #[test]
    fn split_screen_grows_to_four_seats_and_stops_at_the_cap() {
        let mut game = playing(new_world(), 2, "split-four");
        for expected in 3..=MAX_LOCAL_SEATS {
            let _report = press(
                &mut game,
                PlayerInput {
                    local_multiplayer_toggle: true,
                    ..PlayerInput::default()
                },
            );
            assert_eq!(game.seats().len(), expected);
        }
        let _report = press(
            &mut game,
            PlayerInput {
                local_multiplayer_toggle: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.seats().len(), MAX_LOCAL_SEATS);
        assert!(
            game.shell.message.contains("limited"),
            "{}",
            game.shell.message
        );

        idle(&mut game, 20);
        let fourth = game.seats()[3].player;
        let before = player(&game, fourth);
        for _ in 0..45 {
            let _report = game.frame(
                &FrameInput {
                    clipboard: None,
                    primary: PlayerInput::default(),
                    other_seats: vec![
                        PlayerInput::default(),
                        PlayerInput::default(),
                        PlayerInput {
                            horizontal: 1.0,
                            ..PlayerInput::default()
                        },
                    ],
                },
                DT,
            );
        }
        assert!(player(&game, fourth).x > before.x + TILE_SIZE);
        let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
        assert_eq!(snapshot.views.len(), 4);
        assert_eq!(snapshot.players.len(), 4);
        let ids: std::collections::BTreeSet<PlayerId> =
            snapshot.views.iter().map(|view| view.player_id).collect();
        assert_eq!(ids.len(), 4, "every view follows its own rig");
    }

    #[test]
    fn pause_stops_the_host_and_resume_continues() {
        let mut game = playing(new_world(), 1, "pause");
        idle(&mut game, 10);
        let _report = step(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Paused);
        let tick = game.world().simulation_tick();
        for _ in 0..30 {
            let report = step(
                &mut game,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
            assert_eq!(report.ticks_run, 0);
        }
        assert_eq!(
            game.world().simulation_tick(),
            tick,
            "paused host does not tick"
        );
        let _report = step(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Playing);
        idle(&mut game, 10);
        assert!(game.world().simulation_tick() > tick);
    }

    #[test]
    fn a_stranded_rig_calls_rescue_through_the_runtime() {
        let mut world = new_world();
        world.player_mut(PLAYER).expect("rig").fuel = 0.0;
        let mut game = playing(world, 1, "rescue");
        idle(&mut game, 2);
        assert!(game.world().failure_state(PLAYER).is_some());
        let _report = press(
            &mut game,
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
        );
        idle(&mut game, 3);
        assert!(game.world().failure_state(PLAYER).is_none(), "rescued");
        assert_eq!(game.world().rescue_count(PLAYER), 1);
    }

    #[test]
    fn other_seats_call_rescue_only_while_gameplay_runs() {
        let second = PlayerId::new(2);
        let mut world = new_world();
        world.add_player(second);
        world.player_mut(second).expect("seat 2 rig").fuel = 0.0;
        let mut game = playing(world, 2, "seat-rescue");
        assert_eq!(game.seats()[1].player, second);
        idle(&mut game, 2);
        assert!(game.world().failure_state(second).is_some());
        let seat_two_interact = |game: &mut LocalGame| {
            let _report = game.frame(
                &FrameInput {
                    clipboard: None,
                    primary: PlayerInput::default(),
                    other_seats: vec![PlayerInput {
                        interact: true,
                        ..PlayerInput::default()
                    }],
                },
                DT,
            );
        };

        // Pressed while the primary player has the game paused: nothing is queued for later.
        let _report = press(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Paused);
        seat_two_interact(&mut game);
        let _report = press(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        idle(&mut game, 5);
        assert_eq!(game.world().rescue_count(second), 0, "no deferred rescue");

        seat_two_interact(&mut game);
        idle(&mut game, 3);
        assert_eq!(game.world().rescue_count(second), 1);
        assert!(game.world().failure_state(second).is_none(), "rescued");
    }

    #[test]
    fn title_menu_offers_host_and_join() {
        let options = TitleOption::all(false);
        assert!(options.contains(&TitleOption::HostOnline));
        assert!(options.contains(&TitleOption::JoinOnline));
        let mut game = title_game("join-menu");
        let join_index = options
            .iter()
            .position(|option| *option == TitleOption::JoinOnline)
            .expect("join entry");
        for _ in 0..join_index {
            let _report = press(
                &mut game,
                PlayerInput {
                    menu_down: true,
                    ..PlayerInput::default()
                },
            );
        }
        let _report = press(
            &mut game,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Lobby);
        assert_eq!(game.shell.online.screen, OnlineScreen::JoinMenu);
    }

    /// Two whole apps (shell + runtime + net bridge) driven frame by frame in real time.
    struct Pair {
        host: LocalGame,
        client: LocalGame,
        last: std::time::Instant,
    }
    impl Pair {
        fn frame(&mut self, host_input: PlayerInput, client_input: PlayerInput) {
            let now = std::time::Instant::now();
            let dt = (now - self.last).as_secs_f32();
            self.last = now;
            let _host = self.host.frame(&FrameInput::primary(host_input), dt);
            let _client = self.client.frame(&FrameInput::primary(client_input), dt);
            std::thread::sleep(Duration::from_millis(4));
        }
        fn until(&mut self, what: &str, done: impl Fn(&Self) -> bool) {
            self.holding_until(what, PlayerInput::default(), done);
        }
        /// Frames with the client holding `client_input` until `done`. The runtimes tick
        /// from real time, so holding for a fixed frame count would cover a
        /// machine-dependent number of ticks.
        fn holding_until(
            &mut self,
            what: &str,
            client_input: PlayerInput,
            done: impl Fn(&Self) -> bool,
        ) {
            let deadline = std::time::Instant::now() + Duration::from_secs(8);
            while !done(self) {
                assert!(std::time::Instant::now() < deadline, "timed out: {what}");
                self.frame(PlayerInput::default(), client_input);
            }
        }
    }

    /// Hosts from the title, joins a second app by typing the address, readies up and starts:
    /// both apps end up playing. Returns the pair and the client's player.
    fn host_and_join_over_real_quic() -> (Pair, PlayerId) {
        let confirm = PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        };

        let mut host = title_game("online-host");
        host.set_host_bind("127.0.0.1:0".parse().expect("addr"));
        let mut requests = Vec::new();
        host.handle_request(
            ShellRequest::HostOnline,
            &mut requests,
            &mut FrameReport::default(),
        );
        assert_eq!(host.shell.run_mode, RunMode::Lobby);
        let mut pair = Pair {
            host,
            client: title_game("online-client"),
            last: std::time::Instant::now(),
        };
        pair.until("the host endpoint", |pair| {
            pair.host
                .shell
                .online
                .lobby
                .as_ref()
                .is_some_and(|lobby| lobby.addr.is_some())
        });
        let addr = pair
            .host
            .shell
            .online
            .lobby
            .as_ref()
            .and_then(|lobby| lobby.addr);
        let port = addr.expect("bound").port();

        // Join by typing the address into the form.
        pair.client.shell.show_online(OnlineScreen::JoinAddress);
        pair.client.shell.online.text.clear();
        for character in format!("127.0.0.1:{port}").chars() {
            pair.frame(
                PlayerInput::default(),
                PlayerInput {
                    text_input: Some(character),
                    ..PlayerInput::default()
                },
            );
        }
        pair.frame(PlayerInput::default(), confirm);
        pair.until("the client in the lobby", |pair| {
            pair.host
                .shell
                .online
                .lobby
                .as_ref()
                .is_some_and(|lobby| lobby.roster.len() == 2)
                && pair.client.runtime().client_state() == Some(ClientState::Lobby)
        });
        assert_eq!(pair.client.shell.run_mode, RunMode::Lobby);
        assert_eq!(pair.client.shell.online.role, Some(OnlineRole::Client));

        // Ready up (first lobby entry), then the host starts (first lobby entry).
        pair.frame(PlayerInput::default(), confirm);
        pair.until("the ready flag on the host", |pair| {
            pair.host
                .shell
                .online
                .lobby
                .as_ref()
                .is_some_and(|lobby| lobby.roster.iter().any(|entry| !entry.local && entry.ready))
        });
        pair.frame(confirm, PlayerInput::default());
        assert_eq!(pair.host.shell.run_mode, RunMode::Playing);
        pair.until("the client in game", |pair| {
            pair.client.shell.run_mode == RunMode::Playing
        });
        let client_player = pair.client.seats()[0].player;
        assert!(pair.host.world().player(client_player).is_some());
        (pair, client_player)
    }

    /// Two whole apps (shell + runtime + net bridge) on real QUIC: host, join by address, ready,
    /// start, play, local-only pause, client save refusal, and the host ending the session.
    #[test]
    fn two_apps_host_join_play_and_end_over_real_quic() {
        let (mut pair, client_player) = host_and_join_over_real_quic();

        // The client's rig moves on the host.
        let start_x = pair.host.world().player(client_player).expect("rig").x;
        let right = PlayerInput {
            horizontal: 1.0,
            ..PlayerInput::default()
        };
        pair.holding_until("the client's movement on the host", right, |pair| {
            pair.host.world().player(client_player).expect("rig").x > start_x + TILE_SIZE
        });

        // Pausing online is a local overlay: the world keeps running.
        pair.frame(
            PlayerInput::default(),
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(pair.client.shell.run_mode, RunMode::Paused);
        let tick = pair.host.world().simulation_tick();
        pair.until("host ticks during the client's pause", |pair| {
            pair.host.world().simulation_tick().get() > tick.get() + 10
        });
        assert!(
            !PauseOption::for_role(Some(OnlineRole::Client)).contains(&PauseOption::Save),
            "joined clients have no save entry"
        );
        pair.frame(
            PlayerInput::default(),
            PlayerInput {
                save: true,
                ..PlayerInput::default()
            },
        );
        assert!(
            pair.client.shell.message.contains("only the host"),
            "{}",
            pair.client.shell.message
        );

        // The host ends the session: the client is told and returns to a local runtime.
        let mut requests = Vec::new();
        pair.host.handle_request(
            ShellRequest::LeaveOnline,
            &mut requests,
            &mut FrameReport::default(),
        );
        assert_eq!(pair.host.shell.run_mode, RunMode::Title);
        assert_eq!(pair.host.runtime().online_role(), None);
        pair.until("the client's notice", |pair| {
            pair.client.shell.run_mode == RunMode::Lobby
                && pair.client.shell.online.screen == OnlineScreen::Notice
        });
        assert!(
            pair.client.shell.online.notice.contains("host ended"),
            "{}",
            pair.client.shell.online.notice
        );
        assert_eq!(pair.client.runtime().online_role(), None);
    }
}
