//! Shell state: menus, modals, interiors, settings and local presentation timers.
//!
//! The shell owns no gameplay truth (plan §10). It reads the world through [`ShellContext`] to
//! decide what a menu shows or allows, and turns menu choices into [`ShellRequest`]s that the app
//! carries out through the runtime (service actions, rescue) or the save module.

use drillgame_simulation::{
    PlayerCommand, PlayerId, RecipeKind, SessionServiceRequest, WorldState,
    economy::{DeepClaimStatus, SurfaceZone, TownBuilding, upgrade_offers, upgrade_tier_name},
};

use drillgame_quic_transport::lan::LanDiscoveredGame;

use crate::{
    input::PlayerInput,
    runtime::{LobbyInfo, OnlineRole},
    save,
};

/// Top-level app mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunMode {
    /// Title menu; the world is idle behind it.
    Title,
    /// Live gameplay.
    Playing,
    /// Pause menu (single-player pause stops the host).
    Paused,
    /// Inside a surface building.
    Interior,
    /// Online screens: join forms, LAN list, lobby roster, session notices.
    Lobby,
}

/// Modal screens layered over the current mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalScreen {
    Fuel,
    FuelConfirm,
    Repair,
    RepairConfirm,
    Depot,
    Headquarters,
    DepotReceiptHistory,
    Shop,
    ShopConfirm,
    Bank,
    Explosives,
    Salvage,
    Options,
    SaveSlots,
    LoadSlots,
    ExitConfirm,
    UnsavedExitConfirm,
    Map,
    Help,
    TownDevelopment,
    ExpeditionBoard,
    ResearchLog,
    Crafting,
    Inventory,
}

/// Pause menu entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PauseOption {
    Resume,
    Save,
    Load,
    /// Open a local game to LAN / direct-IP players.
    OpenOnline,
    /// Show the online session (address, roster).
    SessionInfo,
    /// Host: end the online session for everyone.
    EndSession,
    /// Client: leave the host's session.
    LeaveSession,
    Options,
    ExitToDesktop,
}

impl PauseOption {
    /// Pause entries for the current kind of session: joined clients cannot save or load (the
    /// host owns the session save).
    #[must_use]
    pub fn for_role(role: Option<OnlineRole>) -> Vec<Self> {
        match role {
            None => vec![
                Self::Resume,
                Self::Save,
                Self::Load,
                Self::OpenOnline,
                Self::Options,
                Self::ExitToDesktop,
            ],
            Some(OnlineRole::Host) => vec![
                Self::Resume,
                Self::SessionInfo,
                Self::Save,
                Self::Load,
                Self::EndSession,
                Self::Options,
                Self::ExitToDesktop,
            ],
            Some(OnlineRole::Client) => vec![
                Self::Resume,
                Self::SessionInfo,
                Self::LeaveSession,
                Self::Options,
                Self::ExitToDesktop,
            ],
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume",
            Self::Save => "Save Game",
            Self::Load => "Load Game",
            Self::OpenOnline => "Open to LAN / Online",
            Self::SessionInfo => "Online Session",
            Self::EndSession => "End Online Session",
            Self::LeaveSession => "Leave Session",
            Self::Options => "Options",
            Self::ExitToDesktop => "Exit to Desktop",
        }
    }
}

/// Title menu entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TitleOption {
    Resume,
    NewGame,
    LocalMultiplayer,
    HostOnline,
    JoinOnline,
    LoadSlot,
    Options,
    Exit,
}

impl TitleOption {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume Saved Session",
            Self::NewGame => "New Game",
            Self::LocalMultiplayer => "Local Split-Screen",
            Self::HostOnline => "Host Online Game",
            Self::JoinOnline => "Join Online Game",
            Self::LoadSlot => "Load Slot",
            Self::Options => "Options",
            Self::Exit => "Exit",
        }
    }

    /// Title entries, with "Resume" first when a save exists.
    #[must_use]
    pub fn all(saves_exist: bool) -> Vec<Self> {
        let mut options = Vec::with_capacity(8);
        if saves_exist {
            options.push(Self::Resume);
        }
        options.extend([
            Self::NewGame,
            Self::LocalMultiplayer,
            Self::HostOnline,
            Self::JoinOnline,
        ]);
        if saves_exist {
            options.push(Self::LoadSlot);
        }
        options.extend([Self::Options, Self::Exit]);
        options
    }
}

/// Sound effects the shell or gameplay events ask the audio bus to play.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundCue {
    Drill,
    Sell,
    Upgrade,
    Damage,
    Milestone,
    Rescue,
    Explosion,
    Ui,
}

/// Interior service animation after a completed fuel/repair purchase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceAnimation {
    Fuel,
    Repair,
}

/// Something the shell needs the app (runtime / saves) to do.
#[derive(Clone, Debug, PartialEq)]
pub enum ShellRequest {
    /// Start a fresh world with this many local seats.
    NewGame { seats: usize },
    /// Load the most recent save.
    ResumeLatest,
    /// Quick save (F5) to the main save file.
    QuickSave,
    /// Quick load (F9) from the main save file.
    QuickLoad,
    /// Save into a slot.
    SaveSlot(usize),
    /// Load from a slot.
    LoadSlot(usize),
    /// Save to the main file, then exit.
    SaveAndExit,
    /// A one-shot gameplay action for the primary seat (services, rescue).
    Action(PlayerCommand),
    /// Add another local split-screen seat.
    AddLocalSeat,
    /// Host a new online game (lobby first).
    HostOnline,
    /// Open the running local game to remote players.
    OpenOnline,
    /// Join a host by `ip:port` (or bare ip with the default port).
    JoinAddress(String),
    /// Join a host by its descriptor JSON (pinned certificate).
    JoinDescriptor(String),
    /// Browse the LAN for advertised sessions.
    BrowseLan,
    /// Join the n-th discovered LAN session.
    JoinLan(usize),
    /// Client: set the lobby ready flag.
    SetReady(bool),
    /// Host: start the game from the lobby.
    StartOnline,
    /// Host: end the session; client: leave it.
    LeaveOnline,
    /// Rejoin the last host with the reconnect token.
    Reconnect,
    /// Put text on the system clipboard.
    CopyToClipboard(String),
    /// Dev: step the selected live network tuning value.
    AdjustTuning { parameter: usize, step: i32 },
    /// Close the app.
    Exit,
}

/// Which online screen [`RunMode::Lobby`] shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineScreen {
    /// Choose how to join.
    JoinMenu,
    /// Type an address.
    JoinAddress,
    /// Paste a descriptor.
    JoinDescriptor,
    /// Pick a discovered LAN session.
    LanBrowser,
    /// Session roster (host or client), before or during the game.
    Lobby,
    /// A session-ended / rejected / disconnected message.
    Notice,
}

/// Lobby menu entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LobbyAction {
    /// Host: start the game.
    Start,
    /// Return to the running game.
    BackToGame,
    /// Host: copy the descriptor to the clipboard.
    CopyDescriptor,
    /// Client: toggle ready.
    ToggleReady,
    /// Host: end the session; client: leave it.
    Leave,
}

impl LobbyAction {
    /// Menu label (`host` picks the host wording of `Leave`).
    #[must_use]
    pub const fn label(self, host: bool) -> &'static str {
        match self {
            Self::Start => "Start game",
            Self::BackToGame => "Back to game",
            Self::CopyDescriptor => "Copy descriptor to clipboard",
            Self::ToggleReady => "Toggle ready",
            Self::Leave => {
                if host {
                    "End session"
                } else {
                    "Leave session"
                }
            }
        }
    }
}

/// Session notice menu entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeAction {
    /// Rejoin with the reconnect token.
    Reconnect,
    /// Back to the title menu.
    BackToTitle,
}

impl NoticeAction {
    /// Menu label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Reconnect => "Reconnect",
            Self::BackToTitle => "Back to title",
        }
    }
}

/// Longest address / descriptor text accepted.
const MAX_ONLINE_TEXT: usize = 16 * 1024;

/// Entries of the join menu.
pub const JOIN_MENU: [&str; 4] = [
    "Join by address (ip:port)",
    "Join by descriptor (paste with Ctrl+V)",
    "Find LAN games",
    "Back",
];

/// Online shell state: which screen, form text, LAN results and the latest lobby facts.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent online UI toggles"
)]
#[derive(Clone, Debug)]
pub struct OnlineShell {
    pub screen: OnlineScreen,
    pub selected: usize,
    /// Text being typed (address / descriptor).
    pub text: String,
    pub lan_games: Vec<LanDiscoveredGame>,
    pub lan_status: String,
    /// Refreshed every frame from the runtime while online.
    pub lobby: Option<LobbyInfo>,
    /// Refreshed every frame from the runtime.
    pub role: Option<OnlineRole>,
    pub ready: bool,
    pub notice: String,
    pub can_reconnect: bool,
    /// F3 network overlay.
    pub show_net_debug: bool,
    /// Refreshed every frame while the overlay is shown.
    pub net_debug: Vec<String>,
    /// Live tuning parameter the dev keys adjust.
    pub tune_parameter: usize,
    /// Last address typed into the join form.
    pub last_address: String,
}

impl Default for OnlineShell {
    fn default() -> Self {
        Self {
            screen: OnlineScreen::JoinMenu,
            selected: 0,
            text: String::new(),
            lan_games: Vec::new(),
            lan_status: String::new(),
            lobby: None,
            role: None,
            ready: false,
            notice: String::new(),
            can_reconnect: false,
            show_net_debug: false,
            net_debug: Vec::new(),
            tune_parameter: 0,
            last_address: format!("127.0.0.1:{}", crate::online::DEFAULT_PORT),
        }
    }
}

impl OnlineShell {
    /// Whether the game of the current online session has started.
    #[must_use]
    pub fn started(&self) -> bool {
        self.lobby.as_ref().is_some_and(|lobby| lobby.started)
    }

    /// Lobby menu entries for the current role and game state.
    #[must_use]
    pub fn lobby_options(&self) -> Vec<LobbyAction> {
        let first = if self.started() {
            LobbyAction::BackToGame
        } else if self.role == Some(OnlineRole::Host) {
            LobbyAction::Start
        } else {
            LobbyAction::ToggleReady
        };
        if self.role == Some(OnlineRole::Host) {
            vec![first, LobbyAction::CopyDescriptor, LobbyAction::Leave]
        } else {
            vec![first, LobbyAction::Leave]
        }
    }

    /// Notice menu entries.
    #[must_use]
    pub fn notice_options(&self) -> Vec<NoticeAction> {
        if self.can_reconnect {
            vec![NoticeAction::Reconnect, NoticeAction::BackToTitle]
        } else {
            vec![NoticeAction::BackToTitle]
        }
    }

    /// LAN browser entries: discovered sessions, then refresh and back.
    #[must_use]
    pub fn lan_options(&self) -> Vec<String> {
        self.lan_games
            .iter()
            .map(|game| format!("{} ({})", game.instance_name, game.game_addr))
            .chain(["Refresh".to_owned(), "Back".to_owned()])
            .collect()
    }
}

/// Read-only gameplay facts the shell needs, taken from the runtime's world.
#[derive(Clone, Copy, Debug)]
pub struct ShellContext<'w> {
    /// The authoritative (or predicted) world.
    pub world: &'w WorldState,
    /// The player the shell's menus act for (seat 0).
    pub player: Option<PlayerId>,
    /// Whether any save exists (title "Resume").
    pub saves_exist: bool,
}

impl ShellContext<'_> {
    fn player(&self) -> Option<&drillgame_simulation::Player> {
        self.player.and_then(|player| self.world.player(player))
    }

    /// The building the player stands at: the same rule the host uses to validate service
    /// actions, so a menu that opens here is a menu whose transactions the host accepts.
    fn current_zone(&self) -> Option<SurfaceZone> {
        self.world.player_surface_zone(self.player?)
    }

    fn deep_claim_unlocked(&self) -> bool {
        self.world.progression().deep_claim_status == DeepClaimStatus::Unlocked
    }

    fn failed(&self) -> bool {
        self.player
            .is_some_and(|player| self.world.failure_state(player).is_some())
    }
}

/// A service transaction submitted from a menu, waiting for its tick result.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PendingService {
    request: SessionServiceRequest,
    credits: u32,
    hull: f32,
}

/// All shell (non-gameplay) state.
#[allow(
    clippy::struct_excessive_bools,
    reason = "shell flags are independent UI toggles"
)]
#[derive(Clone, Debug)]
pub struct ShellState {
    pub run_mode: RunMode,
    pub modal: Option<ModalScreen>,
    pub selected_menu_item: usize,
    pub selected_title_item: usize,
    pub selected_pause_item: usize,
    pub show_details: bool,
    pub message: String,
    pub interior_zone: Option<SurfaceZone>,
    pub interior_x: f32,
    pub interior_facing: f32,
    pub master_volume: f32,
    pub fullscreen: bool,
    pub settings_dirty: bool,
    pub save_dirty: bool,
    pub request_exit: bool,
    pub update_ticks: u64,
    pub play_seconds: f32,
    pub service_animation: Option<ServiceAnimation>,
    pub service_animation_seconds: f32,
    pub sound_cues: Vec<SoundCue>,
    pub depot_receipts: Vec<String>,
    pub online: OnlineShell,
    /// Clipboard text pasted this frame (Ctrl+V), for the descriptor form.
    pub pasted: Option<String>,
    pending_services: Vec<PendingService>,
}

impl Default for ShellState {
    fn default() -> Self {
        Self::new(save::SettingsFile::default())
    }
}

impl ShellState {
    /// A shell on the title screen with the given settings.
    #[must_use]
    pub fn new(settings: save::SettingsFile) -> Self {
        Self {
            run_mode: RunMode::Title,
            modal: None,
            selected_menu_item: 0,
            selected_title_item: 0,
            selected_pause_item: 0,
            show_details: false,
            message: "Mine ore, sell cargo, and buy upgrades. Press E at surface buildings."
                .to_owned(),
            interior_zone: None,
            interior_x: 88.0,
            interior_facing: 1.0,
            master_volume: settings.master_volume,
            fullscreen: settings.fullscreen,
            settings_dirty: false,
            save_dirty: false,
            request_exit: false,
            update_ticks: 0,
            play_seconds: 0.0,
            service_animation: None,
            service_animation_seconds: 0.0,
            sound_cues: Vec::new(),
            depot_receipts: Vec::new(),
            online: OnlineShell::default(),
            pasted: None,
            pending_services: Vec::new(),
        }
    }

    /// Current settings for persisting.
    #[must_use]
    pub const fn settings(&self) -> save::SettingsFile {
        save::SettingsFile {
            master_volume: self.master_volume,
            fullscreen: self.fullscreen,
        }
    }

    /// Whether held gameplay controls should reach the world this frame.
    #[must_use]
    pub const fn gameplay_input_enabled(&self) -> bool {
        matches!(self.run_mode, RunMode::Playing) && self.modal.is_none()
    }

    /// Whether the local host should tick. Single-player pause, menus and building interiors
    /// freeze the world, except that a transaction submitted from a building menu runs the ticks
    /// needed to resolve it.
    #[must_use]
    pub const fn world_should_tick(&self) -> bool {
        matches!(self.run_mode, RunMode::Playing) || !self.pending_services.is_empty()
    }

    /// Resets per-run shell state after a new game or load.
    pub fn enter_world(&mut self, message: impl Into<String>, play_seconds: f32) {
        self.run_mode = RunMode::Playing;
        self.modal = None;
        self.interior_zone = None;
        self.selected_menu_item = 0;
        self.save_dirty = false;
        self.play_seconds = play_seconds;
        self.service_animation = None;
        self.service_animation_seconds = 0.0;
        self.pending_services.clear();
        self.message = message.into();
    }

    /// Handles one frame of shell input (the primary seat's full input).
    #[allow(
        clippy::too_many_lines,
        reason = "top-level shell dispatcher keeps frame order explicit"
    )]
    pub fn update(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
        ctx: ShellContext<'_>,
    ) -> Vec<ShellRequest> {
        let mut requests = Vec::new();
        self.update_ticks = self.update_ticks.saturating_add(1);
        self.sound_cues.clear();
        self.show_details = input.details;
        self.service_animation_seconds = (self.service_animation_seconds - delta_seconds).max(0.0);
        if self.service_animation_seconds <= 0.0 {
            self.service_animation = None;
        }
        let text_entry = self.text_entry();
        let in_game = !matches!(self.run_mode, RunMode::Title | RunMode::Lobby);
        if input.net_debug {
            self.online.show_net_debug = !self.online.show_net_debug;
        }
        if self.online.show_net_debug && cfg!(debug_assertions) && !text_entry {
            if input.tune_next {
                self.online.tune_parameter =
                    (self.online.tune_parameter + 1) % crate::runtime::TUNABLE_PARAMETERS.len();
            }
            for (pressed, step) in [(input.tune_up, 1), (input.tune_down, -1)] {
                if pressed {
                    requests.push(ShellRequest::AdjustTuning {
                        parameter: self.online.tune_parameter,
                        step,
                    });
                }
            }
        }
        if input.save && in_game {
            requests.push(ShellRequest::QuickSave);
        }
        if input.load && self.run_mode != RunMode::Lobby {
            requests.push(ShellRequest::QuickLoad);
        }
        if self.handle_exit_modal(input, &mut requests) {
            return requests;
        }
        if input.exit_requested {
            self.request_exit_or_prompt();
            return requests;
        }
        if in_game && input_changes_game(input) {
            self.save_dirty = true;
        }
        if input.local_multiplayer_toggle && in_game {
            requests.push(ShellRequest::AddLocalSeat);
        }
        if in_game {
            toggle_modal(&mut self.modal, input.inventory, ModalScreen::Inventory);
            toggle_modal(&mut self.modal, input.map, ModalScreen::Map);
            toggle_modal(&mut self.modal, input.help, ModalScreen::Help);
        }
        if input.volume_up && !text_entry {
            self.adjust_volume(0.1);
        }
        if input.volume_down && !text_entry {
            self.adjust_volume(-0.1);
        }
        if input.fullscreen {
            self.fullscreen = !self.fullscreen;
            self.settings_dirty = true;
            self.sound_cues.push(SoundCue::Ui);
        }
        if matches!(self.run_mode, RunMode::Playing | RunMode::Interior) {
            self.play_seconds += delta_seconds;
        }
        match self.run_mode {
            RunMode::Title => {
                if !self.handle_modal(input, ctx, &mut requests) {
                    self.handle_title_menu(input, ctx, &mut requests);
                }
            }
            RunMode::Paused => self.handle_pause_menu(input, &mut requests),
            RunMode::Lobby => self.handle_online(input, &mut requests),
            RunMode::Interior => self.handle_interior(input, delta_seconds, ctx, &mut requests),
            RunMode::Playing => {
                if ctx.failed() {
                    if input.interact || input.confirm {
                        requests.push(ShellRequest::Action(PlayerCommand::Rescue));
                        // The rescue sound plays from the world's `RescueTriggered` event once the
                        // host applies it, so a refused rescue stays silent and it never doubles.
                        "Rescue crew dispatched.".clone_into(&mut self.message);
                    }
                    if input.pause || input.cancel {
                        self.run_mode = RunMode::Paused;
                    }
                    return requests;
                }
                if self.handle_modal(input, ctx, &mut requests) {
                    return requests;
                }
                if input.pause || input.cancel {
                    self.run_mode = RunMode::Paused;
                    return requests;
                }
                if input.interact {
                    if let Some(zone) = ctx.current_zone() {
                        self.enter_interior(zone);
                    } else {
                        "No surface service here.".clone_into(&mut self.message);
                    }
                }
            }
        }
        requests
    }

    /// Whether an online text field has focus (typing must not trigger hotkeys).
    #[must_use]
    pub const fn text_entry(&self) -> bool {
        matches!(self.run_mode, RunMode::Lobby)
            && matches!(
                self.online.screen,
                OnlineScreen::JoinAddress | OnlineScreen::JoinDescriptor
            )
    }

    /// Shows an online screen.
    pub fn show_online(&mut self, screen: OnlineScreen) {
        self.run_mode = RunMode::Lobby;
        self.modal = None;
        self.online.screen = screen;
        self.online.selected = 0;
        match screen {
            OnlineScreen::JoinAddress => self.online.text.clone_from(&self.online.last_address),
            OnlineScreen::JoinDescriptor => self.online.text.clear(),
            OnlineScreen::JoinMenu
            | OnlineScreen::LanBrowser
            | OnlineScreen::Lobby
            | OnlineScreen::Notice => {}
        }
    }

    /// Shows a session notice (host ended the session, rejected, disconnected).
    pub fn online_notice(&mut self, notice: impl Into<String>, can_reconnect: bool) {
        self.online.notice = notice.into();
        self.online.can_reconnect = can_reconnect;
        self.online.ready = false;
        self.show_online(OnlineScreen::Notice);
    }

    /// Returns to the title menu (after leaving an online session).
    pub fn return_to_title(&mut self, message: impl Into<String>) {
        self.run_mode = RunMode::Title;
        self.modal = None;
        self.interior_zone = None;
        self.selected_title_item = 0;
        self.online.ready = false;
        self.pending_services.clear();
        self.message = message.into();
    }

    fn handle_online(&mut self, input: PlayerInput, requests: &mut Vec<ShellRequest>) {
        let pasted = self.pasted.take();
        match self.online.screen {
            OnlineScreen::JoinMenu => {
                if input.cancel {
                    self.return_to_title("Mine ore, sell cargo, and buy upgrades.");
                    return;
                }
                match self.online_menu_choice(input, JOIN_MENU.len()) {
                    Some(0) => self.show_online(OnlineScreen::JoinAddress),
                    Some(1) => self.show_online(OnlineScreen::JoinDescriptor),
                    Some(2) => {
                        self.show_online(OnlineScreen::LanBrowser);
                        "Searching the LAN...".clone_into(&mut self.online.lan_status);
                        requests.push(ShellRequest::BrowseLan);
                    }
                    Some(_) => self.return_to_title("Mine ore, sell cargo, and buy upgrades."),
                    None => {}
                }
            }
            OnlineScreen::JoinAddress | OnlineScreen::JoinDescriptor => {
                self.handle_text_form(input, pasted, requests);
            }
            OnlineScreen::LanBrowser => {
                if input.cancel {
                    self.show_online(OnlineScreen::JoinMenu);
                    return;
                }
                let found = self.online.lan_games.len();
                match self.online_menu_choice(input, found + 2) {
                    Some(index) if index < found => requests.push(ShellRequest::JoinLan(index)),
                    Some(index) if index == found => {
                        "Searching the LAN...".clone_into(&mut self.online.lan_status);
                        requests.push(ShellRequest::BrowseLan);
                    }
                    Some(_) => self.show_online(OnlineScreen::JoinMenu),
                    None => {}
                }
            }
            OnlineScreen::Lobby => {
                if input.cancel && self.online.started() {
                    self.run_mode = RunMode::Playing;
                    return;
                }
                let options = self.online.lobby_options();
                let Some(choice) = self.online_menu_choice(input, options.len()) else {
                    return;
                };
                match options[choice] {
                    LobbyAction::Start => requests.push(ShellRequest::StartOnline),
                    LobbyAction::BackToGame => self.run_mode = RunMode::Playing,
                    LobbyAction::CopyDescriptor => {
                        if let Some(descriptor) = self
                            .online
                            .lobby
                            .as_ref()
                            .and_then(|lobby| lobby.descriptor.clone())
                        {
                            requests.push(ShellRequest::CopyToClipboard(descriptor));
                            "Descriptor copied: joiners paste it with Ctrl+V."
                                .clone_into(&mut self.message);
                        } else {
                            "The host endpoint is not ready yet.".clone_into(&mut self.message);
                        }
                    }
                    LobbyAction::ToggleReady => {
                        self.online.ready = !self.online.ready;
                        requests.push(ShellRequest::SetReady(self.online.ready));
                    }
                    LobbyAction::Leave => requests.push(ShellRequest::LeaveOnline),
                }
            }
            OnlineScreen::Notice => {
                if input.cancel {
                    self.return_to_title("Mine ore, sell cargo, and buy upgrades.");
                    return;
                }
                let options = self.online.notice_options();
                match self
                    .online_menu_choice(input, options.len())
                    .map(|choice| options[choice])
                {
                    Some(NoticeAction::Reconnect) => requests.push(ShellRequest::Reconnect),
                    Some(NoticeAction::BackToTitle) => {
                        self.return_to_title("Mine ore, sell cargo, and buy upgrades.");
                    }
                    None => {}
                }
            }
        }
    }

    /// Address / descriptor entry: typing, Backspace, Ctrl+V paste, Enter to join, Esc back.
    fn handle_text_form(
        &mut self,
        input: PlayerInput,
        pasted: Option<String>,
        requests: &mut Vec<ShellRequest>,
    ) {
        // `E` also counts as "confirm" in menus; while typing it is just a letter.
        let typed_confirm_key = matches!(input.text_input, Some('e' | 'E'));
        if let Some(text) = pasted {
            self.online.text.push_str(text.trim());
        }
        if input.text_backspace {
            self.online.text.pop();
        } else if input.cancel {
            self.show_online(OnlineScreen::JoinMenu);
            return;
        }
        if let Some(character) = input.text_input
            && !character.is_control()
            && self.online.text.len() < MAX_ONLINE_TEXT
        {
            self.online.text.push(character);
        }
        let text = self.online.text.trim().to_owned();
        if input.confirm && !typed_confirm_key && !text.is_empty() {
            if self.online.screen == OnlineScreen::JoinAddress {
                self.online.last_address.clone_from(&text);
                requests.push(ShellRequest::JoinAddress(text));
            } else {
                requests.push(ShellRequest::JoinDescriptor(text));
            }
        }
    }

    fn online_menu_choice(&mut self, input: PlayerInput, count: usize) -> Option<usize> {
        if input.menu_up {
            self.online.selected = self.online.selected.saturating_sub(1);
        }
        if input.menu_down {
            self.online.selected += 1;
        }
        self.online.selected = self.online.selected.min(count.saturating_sub(1));
        input.confirm.then_some(self.online.selected)
    }

    fn adjust_volume(&mut self, delta: f32) {
        self.master_volume = (self.master_volume + delta).clamp(0.0, 1.0);
        self.message = format!("Volume: {:.0}%", self.master_volume * 100.0);
        self.settings_dirty = true;
        self.sound_cues.push(SoundCue::Ui);
    }

    fn handle_title_menu(
        &mut self,
        input: PlayerInput,
        ctx: ShellContext<'_>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let options = TitleOption::all(ctx.saves_exist);
        if input.menu_up {
            self.selected_title_item = self.selected_title_item.saturating_sub(1);
        }
        if input.menu_down {
            self.selected_title_item += 1;
        }
        self.selected_title_item = self.selected_title_item.min(options.len() - 1);
        if input.cancel {
            self.request_exit = true;
            requests.push(ShellRequest::Exit);
            return;
        }
        if !input.confirm {
            return;
        }
        match options[self.selected_title_item] {
            TitleOption::Resume => requests.push(ShellRequest::ResumeLatest),
            TitleOption::NewGame => requests.push(ShellRequest::NewGame { seats: 1 }),
            TitleOption::LocalMultiplayer => requests.push(ShellRequest::NewGame { seats: 2 }),
            TitleOption::HostOnline => requests.push(ShellRequest::HostOnline),
            TitleOption::JoinOnline => self.show_online(OnlineScreen::JoinMenu),
            TitleOption::LoadSlot => self.open_modal(ModalScreen::LoadSlots),
            TitleOption::Options => self.open_modal(ModalScreen::Options),
            TitleOption::Exit => {
                self.request_exit = true;
                requests.push(ShellRequest::Exit);
            }
        }
    }

    fn handle_pause_menu(&mut self, input: PlayerInput, requests: &mut Vec<ShellRequest>) {
        if self.modal.is_some() {
            // Slot / options modals opened from the pause menu.
            let _handled = self.handle_menu_modal(input, requests);
            return;
        }
        let options = PauseOption::for_role(self.online.role);
        if input.menu_up {
            self.selected_pause_item = self.selected_pause_item.saturating_sub(1);
        }
        if input.menu_down {
            self.selected_pause_item += 1;
        }
        self.selected_pause_item = self.selected_pause_item.min(options.len() - 1);
        if input.pause || input.cancel {
            self.run_mode = RunMode::Playing;
            return;
        }
        if !input.confirm {
            return;
        }
        match options[self.selected_pause_item] {
            PauseOption::Resume => self.run_mode = RunMode::Playing,
            PauseOption::Save => self.open_modal(ModalScreen::SaveSlots),
            PauseOption::Load => self.open_modal(ModalScreen::LoadSlots),
            PauseOption::OpenOnline => {
                self.run_mode = RunMode::Playing;
                requests.push(ShellRequest::OpenOnline);
            }
            PauseOption::SessionInfo => self.show_online(OnlineScreen::Lobby),
            PauseOption::EndSession | PauseOption::LeaveSession => {
                requests.push(ShellRequest::LeaveOnline);
            }
            PauseOption::Options => self.open_modal(ModalScreen::Options),
            PauseOption::ExitToDesktop => self.request_exit_or_prompt(),
        }
    }

    /// Slot and options modals usable from title and pause menus.
    fn handle_menu_modal(&mut self, input: PlayerInput, requests: &mut Vec<ShellRequest>) -> bool {
        let Some(modal) = self.modal else {
            return false;
        };
        if input.cancel {
            self.modal = None;
            return true;
        }
        let max_item = match modal {
            ModalScreen::Options => 2,
            ModalScreen::SaveSlots | ModalScreen::LoadSlots => save::save_slot_count() - 1,
            _ => 0,
        };
        self.move_selection(input, max_item);
        if input.confirm {
            match modal {
                ModalScreen::Options => self.confirm_options(),
                ModalScreen::SaveSlots => {
                    requests.push(ShellRequest::SaveSlot(self.selected_menu_item));
                }
                ModalScreen::LoadSlots => {
                    requests.push(ShellRequest::LoadSlot(self.selected_menu_item));
                }
                _ => {}
            }
        }
        true
    }

    const fn open_modal(&mut self, modal: ModalScreen) {
        self.modal = Some(modal);
        self.selected_menu_item = 0;
    }

    fn move_selection(&mut self, input: PlayerInput, max_item: usize) {
        if input.menu_up {
            self.selected_menu_item = self.selected_menu_item.saturating_sub(1);
        }
        if input.menu_down {
            self.selected_menu_item = (self.selected_menu_item + 1).min(max_item);
        }
    }

    const fn request_exit_or_prompt(&mut self) {
        // A joined client cannot save (the host owns the session save), so it is never offered
        // "save and exit".
        let can_save = !matches!(self.online.role, Some(OnlineRole::Client));
        if self.save_dirty && can_save {
            self.open_modal(ModalScreen::UnsavedExitConfirm);
        } else {
            self.modal = Some(ModalScreen::ExitConfirm);
        }
    }

    fn handle_exit_modal(&mut self, input: PlayerInput, requests: &mut Vec<ShellRequest>) -> bool {
        match self.modal {
            Some(ModalScreen::ExitConfirm) => {
                if input.cancel {
                    self.modal = None;
                } else if input.confirm {
                    self.request_exit = true;
                    requests.push(ShellRequest::Exit);
                }
                true
            }
            Some(ModalScreen::UnsavedExitConfirm) => {
                if input.cancel {
                    self.modal = None;
                    return true;
                }
                self.move_selection(input, 2);
                if input.confirm {
                    match self.selected_menu_item {
                        0 => requests.push(ShellRequest::SaveAndExit),
                        1 => {
                            self.request_exit = true;
                            requests.push(ShellRequest::Exit);
                        }
                        _ => self.modal = None,
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn enter_interior(&mut self, zone: SurfaceZone) {
        self.run_mode = RunMode::Interior;
        self.interior_zone = Some(zone);
        self.interior_x = 82.0;
        self.interior_facing = 1.0;
        self.modal = None;
        self.selected_menu_item = 0;
        self.sound_cues.push(SoundCue::Ui);
        self.message = format!(
            "Entered {}. Walk to a counter and press E; door exits.",
            surface_zone_label(zone)
        );
    }

    fn handle_interior(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
        ctx: ShellContext<'_>,
        requests: &mut Vec<ShellRequest>,
    ) {
        if self.handle_modal(input, ctx, requests) {
            return;
        }
        if input.pause {
            self.run_mode = RunMode::Paused;
            return;
        }
        let movement = input.horizontal;
        if movement.abs() > f32::EPSILON {
            self.interior_facing = movement.signum();
        }
        self.interior_x = movement
            .mul_add(185.0 * delta_seconds, self.interior_x)
            .clamp(42.0, 598.0);
        if input.cancel || (input.interact && self.interior_x < 74.0) {
            self.run_mode = RunMode::Playing;
            self.interior_zone = None;
            self.modal = None;
            self.sound_cues.push(SoundCue::Ui);
            "Back outside.".clone_into(&mut self.message);
            return;
        }
        if input.interact {
            self.open_interior_hotspot();
        }
    }

    fn open_interior_hotspot(&mut self) {
        let Some(zone) = self.interior_zone else {
            return;
        };
        if (self.interior_x - interior_service_x(zone)).abs() > 70.0 {
            "Walk to the service counter or the exit door.".clone_into(&mut self.message);
            return;
        }
        self.selected_menu_item = 0;
        self.modal = Some(match zone {
            SurfaceZone::Fuel => ModalScreen::Fuel,
            SurfaceZone::Repair => ModalScreen::Repair,
            SurfaceZone::Depot => {
                self.selected_menu_item = 1;
                ModalScreen::Depot
            }
            SurfaceZone::Headquarters => ModalScreen::Headquarters,
            SurfaceZone::Shop => ModalScreen::Shop,
            SurfaceZone::Bank => ModalScreen::Bank,
            SurfaceZone::Explosives => ModalScreen::Explosives,
            SurfaceZone::Salvage => ModalScreen::Salvage,
        });
        self.sound_cues.push(SoundCue::Ui);
    }

    #[allow(
        clippy::too_many_lines,
        reason = "exhaustive modal dispatch keeps every building menu in one place"
    )]
    fn handle_modal(
        &mut self,
        input: PlayerInput,
        ctx: ShellContext<'_>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        let Some(modal) = self.modal else {
            return false;
        };
        if matches!(
            modal,
            ModalScreen::Options | ModalScreen::SaveSlots | ModalScreen::LoadSlots
        ) {
            return self.handle_menu_modal(input, requests);
        }
        if input.cancel {
            self.modal = None;
            return true;
        }
        let upgrade_count = ctx
            .player()
            .map_or(1, |player| upgrade_offers(player).len());
        let progression = ctx.world.progression();
        let max_item = match modal {
            ModalScreen::Depot => 4,
            ModalScreen::Fuel | ModalScreen::Repair | ModalScreen::Bank => 2,
            ModalScreen::Explosives => 3,
            ModalScreen::Salvage => 5,
            ModalScreen::Headquarters => {
                if ctx.deep_claim_unlocked() {
                    6
                } else {
                    2
                }
            }
            ModalScreen::Crafting => RecipeKind::ALL.len() - 1,
            ModalScreen::TownDevelopment => TownBuilding::ALL.len() - 1,
            ModalScreen::ExpeditionBoard => progression
                .expedition_offers
                .len()
                .saturating_add(progression.active_expeditions.len())
                .saturating_sub(1),
            ModalScreen::Shop | ModalScreen::ShopConfirm => upgrade_count - 1,
            _ => 0,
        };
        self.move_selection(input, max_item);
        if matches!(modal, ModalScreen::Shop) {
            if input.menu_left {
                self.selected_menu_item = self.selected_menu_item.saturating_sub(1);
            }
            if input.menu_right {
                self.selected_menu_item = (self.selected_menu_item + 1).min(upgrade_count - 1);
            }
        }
        if let Some(index) = input.selected_upgrade
            && matches!(modal, ModalScreen::Shop | ModalScreen::ShopConfirm)
        {
            self.selected_menu_item = index.min(upgrade_count - 1);
        }
        if !input.confirm {
            return true;
        }
        let service = match modal {
            ModalScreen::Fuel => {
                self.modal = Some(ModalScreen::FuelConfirm);
                None
            }
            ModalScreen::FuelConfirm => {
                self.modal = Some(ModalScreen::Fuel);
                Some(SessionServiceRequest::Refuel {
                    menu_item: self.selected_menu_item,
                })
            }
            ModalScreen::Repair => {
                self.modal = Some(ModalScreen::RepairConfirm);
                None
            }
            ModalScreen::RepairConfirm => {
                self.modal = Some(ModalScreen::Repair);
                Some(SessionServiceRequest::Repair {
                    menu_item: self.selected_menu_item,
                })
            }
            ModalScreen::Depot => match self.selected_menu_item {
                0 => Some(SessionServiceRequest::CompleteDepotWork),
                1 => Some(SessionServiceRequest::SellCargo),
                2 => Some(SessionServiceRequest::AutoSortLowGradeCargo),
                3 => Some(SessionServiceRequest::SellScanData),
                _ => {
                    self.modal = Some(ModalScreen::DepotReceiptHistory);
                    None
                }
            },
            ModalScreen::DepotReceiptHistory => {
                self.modal = Some(ModalScreen::Depot);
                None
            }
            ModalScreen::Headquarters => self.confirm_headquarters(ctx),
            ModalScreen::Shop => {
                self.modal = Some(ModalScreen::ShopConfirm);
                None
            }
            ModalScreen::ShopConfirm => {
                self.modal = Some(ModalScreen::Shop);
                Some(SessionServiceRequest::BuyUpgrade {
                    index: self.selected_menu_item,
                })
            }
            ModalScreen::Bank => Some(match self.selected_menu_item {
                0 => SessionServiceRequest::Finance,
                1 => SessionServiceRequest::BuyInsurance,
                _ => SessionServiceRequest::StartSideContract,
            }),
            ModalScreen::Explosives => Some(match self.selected_menu_item {
                0 => SessionServiceRequest::BuyBombBundle { count: 3, cost: 55 },
                1 => SessionServiceRequest::BuyBombBundle {
                    count: 7,
                    cost: 120,
                },
                2 => SessionServiceRequest::BuyMiningRockets,
                _ => SessionServiceRequest::ClaimFreeTestCharge,
            }),
            ModalScreen::Salvage => Some(match self.selected_menu_item {
                0 => SessionServiceRequest::SalvageRecoverLostCargo,
                1 => SessionServiceRequest::SalvagePatchHull,
                2 => SessionServiceRequest::SalvageLaunchDrone,
                3 => SessionServiceRequest::SalvageRecoverWreckedPart,
                4 => SessionServiceRequest::SalvageClearCollapseZones,
                _ => SessionServiceRequest::SalvageSellScrapTip,
            }),
            ModalScreen::TownDevelopment => Some(SessionServiceRequest::UpgradeTownBuilding {
                building: TownBuilding::ALL
                    [self.selected_menu_item.min(TownBuilding::ALL.len() - 1)],
            }),
            ModalScreen::ExpeditionBoard => {
                let offers = progression.expedition_offers.len();
                Some(if self.selected_menu_item < offers {
                    SessionServiceRequest::AcceptExpedition {
                        index: self.selected_menu_item,
                    }
                } else {
                    SessionServiceRequest::AbandonExpedition {
                        index: self.selected_menu_item - offers,
                    }
                })
            }
            ModalScreen::Crafting => Some(SessionServiceRequest::CraftRecipe {
                recipe: RecipeKind::ALL[self.selected_menu_item.min(RecipeKind::ALL.len() - 1)],
            }),
            ModalScreen::Options
            | ModalScreen::SaveSlots
            | ModalScreen::LoadSlots
            | ModalScreen::ExitConfirm
            | ModalScreen::UnsavedExitConfirm
            | ModalScreen::Map
            | ModalScreen::Help
            | ModalScreen::Inventory
            | ModalScreen::ResearchLog => None,
        };
        if let Some(request) = service {
            self.submit_service(request, ctx, requests);
        }
        true
    }

    fn confirm_headquarters(&mut self, ctx: ShellContext<'_>) -> Option<SessionServiceRequest> {
        let unlocked = ctx.deep_claim_unlocked();
        match self.selected_menu_item {
            0 => Some(SessionServiceRequest::CompleteDepotWork),
            1 => {
                self.message = hq_story_message(ctx.world);
                self.sound_cues.push(SoundCue::Milestone);
                None
            }
            2 => Some(SessionServiceRequest::Finance),
            3 if unlocked => {
                self.open_modal(ModalScreen::TownDevelopment);
                None
            }
            4 if unlocked => {
                self.open_modal(ModalScreen::ExpeditionBoard);
                None
            }
            5 if unlocked => {
                self.open_modal(ModalScreen::ResearchLog);
                None
            }
            _ if unlocked => {
                self.open_modal(ModalScreen::Crafting);
                None
            }
            _ => Some(SessionServiceRequest::Finance),
        }
    }

    fn submit_service(
        &mut self,
        request: SessionServiceRequest,
        ctx: ShellContext<'_>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let Some(player) = ctx.player() else {
            "Service unavailable for current player.".clone_into(&mut self.message);
            return;
        };
        self.pending_services.push(PendingService {
            request,
            credits: player.credits,
            hull: player.hull,
        });
        requests.push(ShellRequest::Action(PlayerCommand::Service(request)));
        "Processing...".clone_into(&mut self.message);
    }

    /// Reports a resolved service action for the primary seat (from the tick's events).
    #[allow(
        clippy::too_many_lines,
        reason = "one message per service keeps the result wording in one place"
    )]
    pub fn service_resolved(
        &mut self,
        request: SessionServiceRequest,
        applied: bool,
        ctx: ShellContext<'_>,
    ) {
        let Some(index) = self
            .pending_services
            .iter()
            .position(|pending| pending.request == request)
        else {
            return;
        };
        let pending = self.pending_services.remove(index);
        if !applied {
            unavailable_message(request).clone_into(&mut self.message);
            return;
        }
        let Some(player) = ctx.player() else {
            return;
        };
        let spent = pending.credits.saturating_sub(player.credits);
        let earned = player.credits.saturating_sub(pending.credits);
        let mut cue = SoundCue::Upgrade;
        self.message = match request {
            SessionServiceRequest::Refuel { .. } => {
                self.service_animation = Some(ServiceAnimation::Fuel);
                self.service_animation_seconds = 1.4;
                if spent == 0 {
                    "Fuel already full or no credits available.".to_owned()
                } else {
                    format!("Fuel topped up for {spent} credits.")
                }
            }
            SessionServiceRequest::Repair { .. } => {
                self.service_animation = Some(ServiceAnimation::Repair);
                self.service_animation_seconds = 1.4;
                if spent == 0 {
                    "Hull already repaired or no credits available.".to_owned()
                } else {
                    format!("Hull repaired for {spent} credits.")
                }
            }
            SessionServiceRequest::SellCargo | SessionServiceRequest::AutoSortLowGradeCargo => {
                cue = SoundCue::Sell;
                if earned == 0 {
                    "No cargo to sell.".to_owned()
                } else {
                    let receipt = format!("Sold cargo for {earned} credits.");
                    self.push_receipt(receipt.clone());
                    receipt
                }
            }
            SessionServiceRequest::BuyUpgrade { index } => {
                upgrade_offers(player).get(index).map_or_else(
                    || "Upgrade purchased.".to_owned(),
                    |offer| {
                        format!(
                            "Bought {}.",
                            upgrade_tier_name(offer.kind, offer.level.saturating_sub(1))
                        )
                    },
                )
            }
            SessionServiceRequest::BuyBombBundle { .. }
            | SessionServiceRequest::BuyMiningRockets
            | SessionServiceRequest::ClaimFreeTestCharge => {
                format!("Nix packed your charges. You now carry {}.", player.bombs)
            }
            SessionServiceRequest::SalvagePatchHull => format!(
                "Salvage Yard patch job restored {:.0} hull.",
                player.hull - pending.hull
            ),
            SessionServiceRequest::Finance => {
                cue = SoundCue::Sell;
                if earned > 0 {
                    format!(
                        "HQ finance issued a {earned} credit advance. Risk-adjusted payoff: {} credits.",
                        player.loan_debt
                    )
                } else {
                    format!(
                        "Paid {spent} credits toward HQ debt. Remaining: {}.",
                        player.loan_debt
                    )
                }
            }
            SessionServiceRequest::BuyInsurance => format!(
                "Ledger sold tier {} rescue insurance. Higher tiers reduce fees and cargo loss.",
                player.insurance_tier
            ),
            SessionServiceRequest::CraftRecipe { recipe } => {
                format!("Crafted {}: {}.", recipe.name(), recipe.description())
            }
            SessionServiceRequest::UpgradeTownBuilding { building } => format!(
                "{} upgraded to level {}. Deep Claim reputation increased.",
                building.name(),
                ctx.world.progression().town_development.level(building)
            ),
            SessionServiceRequest::SalvageRecoverLostCargo
            | SessionServiceRequest::SalvageLaunchDrone => {
                "Mara recovered your marked lost cargo.".to_owned()
            }
            SessionServiceRequest::SalvageRecoverWreckedPart => {
                "Mara recovered a wrecked rig part.".to_owned()
            }
            SessionServiceRequest::SalvageClearCollapseZones => {
                cue = SoundCue::Milestone;
                format!("Collapse-zone warnings cleared for {earned} credits.")
            }
            SessionServiceRequest::SalvageSellScrapTip | SessionServiceRequest::SellScanData => {
                cue = SoundCue::Sell;
                format!("Sold data for {earned} credits.")
            }
            SessionServiceRequest::CompleteDepotWork => {
                cue = SoundCue::Sell;
                let receipt = format!("Contract work complete: earned {earned} credits.");
                self.push_receipt(receipt.clone());
                receipt
            }
            SessionServiceRequest::StartSideContract => {
                cue = SoundCue::Ui;
                ctx.world
                    .progression()
                    .active_side_contracts
                    .last()
                    .map_or_else(
                        || "Side contract posted.".to_owned(),
                        |contract| {
                            format!(
                                "Side contract posted: {:?} x{}.",
                                contract.target, contract.required
                            )
                        },
                    )
            }
            SessionServiceRequest::AcceptExpedition { .. } => {
                cue = SoundCue::Ui;
                "Expedition accepted.".to_owned()
            }
            SessionServiceRequest::AbandonExpedition { .. } => {
                cue = SoundCue::Ui;
                "Expedition abandoned.".to_owned()
            }
        };
        self.sound_cues.push(cue);
    }

    fn push_receipt(&mut self, receipt: String) {
        self.depot_receipts.push(receipt);
        if self.depot_receipts.len() > 5 {
            self.depot_receipts.remove(0);
        }
    }

    fn confirm_options(&mut self) {
        match self.selected_menu_item {
            0 => self.master_volume = (self.master_volume + 0.1).min(1.0),
            1 => self.master_volume = (self.master_volume - 0.1).max(0.0),
            _ => self.fullscreen = !self.fullscreen,
        }
        self.message = if self.selected_menu_item < 2 {
            format!("Volume: {:.0}%", self.master_volume * 100.0)
        } else if self.fullscreen {
            "Fullscreen preference enabled; F11 toggles immediately.".to_owned()
        } else {
            "Windowed preference enabled; F11 toggles immediately.".to_owned()
        };
        self.settings_dirty = true;
        self.sound_cues.push(SoundCue::Ui);
    }

    /// Takes the "settings changed" flag.
    pub const fn take_settings_dirty(&mut self) -> bool {
        let dirty = self.settings_dirty;
        self.settings_dirty = false;
        dirty
    }
}

fn toggle_modal(modal: &mut Option<ModalScreen>, pressed: bool, screen: ModalScreen) {
    if pressed {
        *modal = if *modal == Some(screen) {
            None
        } else {
            Some(screen)
        };
    }
}

const fn unavailable_message(request: SessionServiceRequest) -> &'static str {
    match request {
        SessionServiceRequest::Refuel { .. } | SessionServiceRequest::Repair { .. } => {
            "Service unavailable right now."
        }
        SessionServiceRequest::SellCargo | SessionServiceRequest::AutoSortLowGradeCargo => {
            "No cargo the depot will buy."
        }
        SessionServiceRequest::BuyUpgrade { .. } => {
            "Upgrade purchase unavailable: check credits and tier."
        }
        SessionServiceRequest::CompleteDepotWork => "No contract is ready to complete.",
        _ => "Not enough credits or materials, or the service is locked.",
    }
}

const fn input_changes_game(input: PlayerInput) -> bool {
    input.horizontal != 0.0
        || input.thrust
        || input.drill_down
        || input.interact
        || input.confirm
        || input.bomb
        || input.scan
        || input.place_relay
        || input.place_drone
        || input.place_lift
        || input.place_support
        || input.place_pump
        || input.place_processor
}

fn hq_story_message(world: &WorldState) -> String {
    if world.won_game() {
        return "Director Vale: The Star Core is secure. Deep Claim operations are open: build relays, chase expeditions, catalogue strata, and push for legendary blueprints.".to_owned();
    }
    match world.progression().deepest_tile_reached {
        0..=19 => "Director Vale: Bring us contract cargo and prove this shaft is profitable.",
        20..=39 => "Mechanic Iona: Silver strata ahead. Upgrade before chasing deep seams.",
        40..=59 => "Surveyor Kade: Relic signals are stronger. Gas pockets are no longer rumors.",
        60..=79 => "Director Vale: Thermal readings are ugly. Radiators and hull plating are survival gear.",
        _ => "Surveyor Kade: Star Core harmonics are below. Expect vents, blasts, and cave-ins.",
    }
    .to_owned()
}

/// Human-readable building name.
#[must_use]
pub const fn surface_zone_label(zone: SurfaceZone) -> &'static str {
    match zone {
        SurfaceZone::Fuel => "Fuel Station",
        SurfaceZone::Repair => "Repair Garage",
        SurfaceZone::Depot => "Ore Depot",
        SurfaceZone::Headquarters => "HQ",
        SurfaceZone::Shop => "Upgrade Shop",
        SurfaceZone::Bank => "Bank",
        SurfaceZone::Explosives => "Explosives Shack",
        SurfaceZone::Salvage => "Salvage Yard",
    }
}

/// Interior x position (room units) of a building's service counter.
#[must_use]
pub const fn interior_service_x(zone: SurfaceZone) -> f32 {
    match zone {
        SurfaceZone::Fuel => 430.0,
        SurfaceZone::Repair => 405.0,
        SurfaceZone::Depot => 455.0,
        SurfaceZone::Headquarters => 390.0,
        SurfaceZone::Shop => 450.0,
        SurfaceZone::Bank => 380.0,
        SurfaceZone::Explosives => 431.0,
        SurfaceZone::Salvage => 410.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joined_clients_are_never_offered_save_and_exit() {
        let mut shell = ShellState::new(crate::save::SettingsFile::default());
        shell.enter_world("test", 0.0);
        shell.save_dirty = true;

        shell.request_exit_or_prompt();
        assert_eq!(shell.modal, Some(ModalScreen::UnsavedExitConfirm));

        shell.modal = None;
        shell.online.role = Some(OnlineRole::Client);
        shell.request_exit_or_prompt();
        assert_eq!(shell.modal, Some(ModalScreen::ExitConfirm));
    }
}
