//! Shell state: menus, modals, interiors, settings and local presentation timers.
//!
//! The shell owns no gameplay truth (plan §10). It reads the world through [`ShellContext`] to
//! decide what a menu shows or allows, and turns menu choices into [`ShellRequest`]s that the app
//! carries out through the runtime (service actions, rescue) or the save module.

use fatherload_simulation::{
    PlayerCommand, PlayerId, RecipeKind, SessionServiceRequest, TestModeState, WorldState,
    economy::{DeepClaimStatus, SurfaceZone, TownBuilding, upgrade_offers, upgrade_tier_name},
    worldgen::WorldSettings,
};

use fatherload_quic_transport::lan::LanDiscoveredGame;

use std::collections::BTreeMap;

use crate::{
    input::{
        PlayerInput,
        devices::InputDevice,
        prompts::{Prompt, inline_text},
    },
    runtime::{LobbyInfo, OnlineRole},
    save,
};

mod quests;
mod test_mode;
mod world_setup;

pub use quests::{
    DeskRow, Fanfare, FanfareKind, desk_rows, has_quest_tab, log_quests, tracked_quest,
};
pub use test_mode::WARP_BANDS;

pub use world_setup::{SetupOutcome, SetupPurpose, WorldSetup, random_world_seed};

/// Row index of "Start" in the world setup dialog.
#[cfg(test)]
#[must_use]
pub fn world_setup_start_row() -> usize {
    world_setup::SetupRow::ALL
        .iter()
        .position(|row| *row == world_setup::SetupRow::Start)
        .unwrap_or_default()
}

/// Top-level app mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunMode {
    /// Title menu; the world is idle behind it.
    Title,
    /// Live gameplay.
    Playing,
    /// The shared pause menu, opened by any seat (single-player pause stops the host). Being
    /// inside a building is per seat: see [`SeatUi::interior_zone`].
    Paused,
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
    /// The new-world setup dialog (seed, size, richness, hazards); its state is
    /// [`ShellState::world_setup`].
    WorldSetup,
    /// Test mode: the band list the pause menu's "Warp to Layer" opens.
    WarpToLayer,
    /// A keeper's quest desk: the second tab of their counter.
    QuestDesk,
    /// The seat's quest log (flips with the mine map and the collection log).
    QuestLog,
}

impl ModalScreen {
    /// Whether this dialog is global (options, save/load slots, exit confirmations): it covers
    /// every view and lives in [`ShellState::modal`]. Every other dialog belongs to one seat
    /// ([`SeatUi::modal`]) and is drawn in that seat's view.
    #[must_use]
    pub const fn is_global(self) -> bool {
        matches!(
            self,
            Self::Options
                | Self::SaveSlots
                | Self::LoadSlots
                | Self::ExitConfirm
                | Self::UnsavedExitConfirm
                | Self::WorldSetup
                | Self::WarpToLayer
        )
    }
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
    /// Copy the world seed to the clipboard (to share the world).
    CopySeed,
    ExitToDesktop,
    /// Test mode: pick a band to warp this seat's rig to.
    WarpToLayer,
    /// Test mode: toggle the crew's invulnerable hulls.
    ToggleInvulnerable,
    /// Test mode: toggle the crew's infinite fuel.
    ToggleInfiniteFuel,
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
                Self::CopySeed,
                Self::ExitToDesktop,
            ],
            Some(OnlineRole::Host) => vec![
                Self::Resume,
                Self::SessionInfo,
                Self::Save,
                Self::Load,
                Self::EndSession,
                Self::Options,
                Self::CopySeed,
                Self::ExitToDesktop,
            ],
            Some(OnlineRole::Client) => vec![
                Self::Resume,
                Self::SessionInfo,
                Self::LeaveSession,
                Self::Options,
                Self::CopySeed,
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
            Self::CopySeed => "Copy World Seed",
            Self::ExitToDesktop => "Exit to Desktop",
            Self::WarpToLayer => "Warp to Layer",
            Self::ToggleInvulnerable => "Invulnerable Hull",
            Self::ToggleInfiniteFuel => "Infinite Fuel",
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

/// Sound effects the shell or gameplay events ask the audio bus to play (each maps to a designed
/// patch in `audio::cues`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundCue {
    Drill,
    Sell,
    Upgrade,
    Damage,
    Milestone,
    Rescue,
    Explosion,
    /// Menu confirm / open.
    Ui,
    /// Menu selection moved.
    UiMove,
    /// Menu closed / backed out.
    UiBack,
    /// A refused action.
    UiError,
    /// Ore or an artifact went into the cargo bay.
    Pickup,
    BombPlaced,
    BombBounce,
    InfrastructurePlaced,
    Scanner,
    PoiDiscovered,
    CrateOpened,
    SalvageOpened,
    VaultOpened,
    VaultReset,
}

/// Interior service animation after a completed fuel/repair purchase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceAnimation {
    Fuel,
    Repair,
}

/// Something the shell needs the app (runtime / saves) to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellRequest {
    /// Start a fresh world generated from `settings` with this many local seats.
    NewGame {
        seats: usize,
        settings: WorldSettings,
        /// Start the world in test mode.
        test_mode: bool,
    },
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
    /// A one-shot gameplay action (services, rescue) for local seat `seat`'s player.
    Action { seat: usize, command: PlayerCommand },
    /// Add another local split-screen seat.
    AddLocalSeat,
    /// Host a new online game (lobby first) in a world generated from `settings`.
    /// `test_mode` starts it in test mode (for everyone who joins).
    HostOnline {
        settings: WorldSettings,
        test_mode: bool,
    },
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
    "Join by descriptor",
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
}

/// Read-only gameplay facts the shell needs, taken from the runtime's world.
#[derive(Clone, Copy, Debug)]
pub struct ShellContext<'w> {
    /// The authoritative (or predicted) world.
    pub world: &'w WorldState,
    /// The player each local seat controls, in seat order.
    pub players: &'w [PlayerId],
    /// Whether any save exists (title "Resume").
    pub saves_exist: bool,
}

impl<'w> ShellContext<'w> {
    /// What seat `seat`'s menus read.
    fn seat(&self, seat: usize, device: InputDevice) -> SeatContext<'w> {
        SeatContext {
            world: self.world,
            seat,
            player: self.players.get(seat).copied(),
            device,
        }
    }
}

/// What one seat's menus read: the world, the seat's player and its prompt device.
#[derive(Clone, Copy, Debug)]
struct SeatContext<'w> {
    world: &'w WorldState,
    seat: usize,
    player: Option<PlayerId>,
    device: InputDevice,
}

impl SeatContext<'_> {
    fn player(&self) -> Option<&fatherload_simulation::Player> {
        self.player.and_then(|player| self.world.player(player))
    }

    /// The building the seat's rig stands at: the same rule the host uses to validate service
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

/// `prompt`'s key or button on `device`, for messages ("press E" / "press X").
fn key_for(prompt: Prompt, device: InputDevice) -> String {
    inline_text(prompt, device).unwrap_or_else(|| "the use button".to_owned())
}

/// A service transaction submitted from a menu, waiting for its tick result.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PendingService {
    request: SessionServiceRequest,
    credits: u32,
    hull: f32,
}

/// One local seat's own menus: outdoors or inside a building (which one, and where the rig
/// stands), its open counter panel or personal dialog, the selected row, transactions waiting
/// for their tick, the service animation, its messages and its depot receipts.
///
/// Seats never share this state: in split-screen each player walks into buildings, shops and
/// reads their own dialogs in their own view while the others keep playing. Nothing here is
/// saved; loading a game puts every seat back outdoors.
#[derive(Clone, Debug)]
pub struct SeatUi {
    /// The seat's counter panel or personal dialog (never a global one, see
    /// [`ModalScreen::is_global`]).
    pub modal: Option<ModalScreen>,
    /// Row selected in `modal`.
    pub selected_menu_item: usize,
    /// The building the seat is inside (`None`: outdoors).
    pub interior_zone: Option<SurfaceZone>,
    /// Where the rig stands in the room (room units, 0..640).
    pub interior_x: f32,
    /// Which way the rig faces in the room (-1 left, 1 right).
    pub interior_facing: f32,
    pub service_animation: Option<ServiceAnimation>,
    pub service_animation_seconds: f32,
    /// The seat's last few depot receipts.
    pub depot_receipts: Vec<String>,
    /// The seat's latest message (service results, door hints, rescue), shown as a toast in
    /// its own view.
    pub message: String,
    /// `ShellState::update_ticks` when `message` last changed.
    pub message_tick: u64,
    message_seen: String,
    pending_services: Vec<PendingService>,
    /// The quest the seat tracks on its HUD (`None`: the first active one).
    pub tracked_quest: Option<fatherload_simulation::quest::QuestId>,
    /// A desk row confirmed once: a second confirm drops that quest.
    quest_abandon_armed: Option<fatherload_simulation::quest::QuestId>,
    /// The latest quest banner (completion fanfare, ready, failed, blueprint).
    pub fanfare: Option<Fanfare>,
}

/// The state of a seat that does not exist (yet): outdoors with nothing open.
static IDLE_SEAT: SeatUi = SeatUi::new();

impl Default for SeatUi {
    fn default() -> Self {
        Self::new()
    }
}

impl SeatUi {
    /// A seat outdoors with nothing open.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            modal: None,
            selected_menu_item: 0,
            interior_zone: None,
            interior_x: 88.0,
            interior_facing: 1.0,
            service_animation: None,
            service_animation_seconds: 0.0,
            depot_receipts: Vec::new(),
            message: String::new(),
            message_tick: 0,
            message_seen: String::new(),
            pending_services: Vec::new(),
            tracked_quest: None,
            quest_abandon_armed: None,
            fanfare: None,
        }
    }

    /// Whether the seat is inside a building.
    #[must_use]
    pub const fn inside(&self) -> bool {
        self.interior_zone.is_some()
    }

    /// Whether a transaction from this seat is waiting for its tick.
    #[must_use]
    pub const fn has_pending_service(&self) -> bool {
        !self.pending_services.is_empty()
    }

    /// Back outdoors with nothing open (new game, load, back to the title). Receipts and the
    /// last message stay.
    fn reset_outdoors(&mut self) {
        self.modal = None;
        self.interior_zone = None;
        self.selected_menu_item = 0;
        self.service_animation = None;
        self.service_animation_seconds = 0.0;
        self.pending_services.clear();
    }

    fn track_message(&mut self, tick: u64) {
        if self.message != self.message_seen {
            self.message_seen.clone_from(&self.message);
            self.message_tick = tick;
        }
    }

    fn tick_animation(&mut self, delta_seconds: f32) {
        self.service_animation_seconds = (self.service_animation_seconds - delta_seconds).max(0.0);
        if self.service_animation_seconds <= 0.0 {
            self.service_animation = None;
        }
    }

    /// One frame of this seat's input while the game runs. Returns `true` when the seat asked
    /// for the (shared) pause menu.
    fn update(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        debug_assert!(
            self.modal.is_none_or(|modal| !modal.is_global()),
            "global dialogs live in ShellState::modal"
        );
        toggle_modal(&mut self.modal, input.inventory, ModalScreen::Inventory);
        toggle_modal(&mut self.modal, input.map, ModalScreen::Map);
        toggle_modal(&mut self.modal, input.help, ModalScreen::Help);
        if self.inside() {
            return self.handle_interior(input, delta_seconds, ctx, cues, requests);
        }
        if ctx.failed() {
            if self.modal.is_some() {
                // A dialog opened before (or while) the rig was stranded hides the rescue
                // prompt: Back closes it first, and nothing else reaches the hidden prompt.
                if input.backs_out() {
                    self.modal = None;
                    return false;
                }
                return input.pause;
            }
            if input.interact || input.confirm {
                requests.push(ShellRequest::Action {
                    seat: ctx.seat,
                    command: PlayerCommand::Rescue,
                });
                // The rescue sound plays from the world's `RescueTriggered` event once the
                // host applies it, so a refused rescue stays silent and it never doubles.
                "Rescue crew dispatched.".clone_into(&mut self.message);
            }
            return input.pause || input.cancel;
        }
        if self.handle_modal(input, ctx, cues, requests) {
            return false;
        }
        if input.pause || input.cancel {
            return true;
        }
        if input.interact {
            if let Some(zone) = ctx.current_zone() {
                self.enter_interior(zone, ctx.device, cues);
            } else {
                "No surface service here.".clone_into(&mut self.message);
            }
        }
        false
    }

    fn enter_interior(&mut self, zone: SurfaceZone, device: InputDevice, cues: &mut Vec<SoundCue>) {
        self.interior_zone = Some(zone);
        self.interior_x = 82.0;
        self.interior_facing = 1.0;
        self.modal = None;
        self.selected_menu_item = 0;
        cues.push(SoundCue::Ui);
        self.message = format!(
            "Entered {}. Walk to the counter and press {}; {} leaves.",
            surface_zone_label(zone),
            key_for(Prompt::Use, device),
            key_for(Prompt::Back, device),
        );
    }

    fn handle_interior(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        if self.handle_modal(input, ctx, cues, requests) {
            return false;
        }
        if input.pause {
            return true;
        }
        let movement = input.horizontal;
        if movement.abs() > f32::EPSILON {
            self.interior_facing = movement.signum();
        }
        self.interior_x = movement
            .mul_add(185.0 * delta_seconds, self.interior_x)
            .clamp(42.0, 598.0);
        if input.backs_out() || (input.interact && self.interior_x < 74.0) {
            self.interior_zone = None;
            self.modal = None;
            cues.push(SoundCue::Ui);
            "Back outside.".clone_into(&mut self.message);
            return false;
        }
        if input.interact {
            self.open_interior_hotspot(cues);
        }
        false
    }

    fn open_interior_hotspot(&mut self, cues: &mut Vec<SoundCue>) {
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
        cues.push(SoundCue::Ui);
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

    #[allow(
        clippy::too_many_lines,
        reason = "exhaustive modal dispatch keeps every building menu in one place"
    )]
    fn handle_modal(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        let Some(modal) = self.modal else {
            return false;
        };
        if input.backs_out() {
            self.modal = None;
            return true;
        }
        if self.switch_quest_tab(modal, input) {
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
            ModalScreen::QuestDesk => self.desk_rows(ctx).len().saturating_sub(1),
            ModalScreen::QuestLog => ctx.player.map_or(0, |player| {
                log_quests(ctx.world, player).len().saturating_sub(1)
            }),
            _ => 0,
        };
        let before = self.selected_menu_item;
        self.move_selection(input, max_item);
        if self.selected_menu_item != before {
            self.quest_abandon_armed = None;
        }
        if modal == ModalScreen::QuestLog {
            self.track_log_selection(ctx);
        }
        if matches!(modal, ModalScreen::Shop) {
            if input.menu_left || input.tab_previous {
                self.selected_menu_item = self.selected_menu_item.saturating_sub(1);
            }
            if input.menu_right || input.tab_next {
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
            // The mine map and the collection log flip between each other.
            ModalScreen::Map => {
                self.open_modal(ModalScreen::ResearchLog);
                None
            }
            ModalScreen::ResearchLog => {
                self.open_modal(ModalScreen::QuestLog);
                None
            }
            ModalScreen::QuestLog => {
                self.modal = Some(ModalScreen::Map);
                None
            }
            ModalScreen::QuestDesk => self.confirm_quest_desk(ctx, cues),
            ModalScreen::Headquarters => self.confirm_headquarters(ctx, cues),
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
            | ModalScreen::WorldSetup
            | ModalScreen::WarpToLayer
            | ModalScreen::Help
            | ModalScreen::Inventory => None,
        };
        if let Some(request) = service {
            self.submit_service(request, ctx, requests);
        }
        true
    }

    fn confirm_headquarters(
        &mut self,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
    ) -> Option<SessionServiceRequest> {
        let unlocked = ctx.deep_claim_unlocked();
        match self.selected_menu_item {
            0 => Some(SessionServiceRequest::CompleteDepotWork),
            1 => {
                self.message = hq_story_message(ctx.world);
                cues.push(SoundCue::Milestone);
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
        ctx: SeatContext<'_>,
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
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::Service(request),
        });
        "Processing...".clone_into(&mut self.message);
    }

    /// Reports a resolved service action of this seat's player (from the tick's events).
    #[allow(
        clippy::too_many_lines,
        reason = "one message per service keeps the result wording in one place"
    )]
    fn service_resolved(
        &mut self,
        request: SessionServiceRequest,
        applied: bool,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
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
            quests::quest_unavailable_message(request)
                .unwrap_or_else(|| unavailable_message(request))
                .clone_into(&mut self.message);
            cues.push(SoundCue::UiError);
            return;
        }
        if let Some((message, cue)) = self.quest_resolved(request, ctx) {
            self.message = message;
            cues.push(cue);
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
            SessionServiceRequest::AcceptQuest { .. }
            | SessionServiceRequest::TurnInQuest { .. }
            | SessionServiceRequest::AbandonQuest { .. } => return,
        };
        cues.push(cue);
    }

    fn push_receipt(&mut self, receipt: String) {
        self.depot_receipts.push(receipt);
        if self.depot_receipts.len() > 5 {
            self.depot_receipts.remove(0);
        }
    }
}

/// All shell (non-gameplay) state: the global screens (title, pause, options, save slots, exit
/// confirmations, online lobby), settings and global messages, plus one [`SeatUi`] per local
/// seat for everything a player does on their own.
#[allow(
    clippy::struct_excessive_bools,
    reason = "shell flags are independent UI toggles"
)]
#[derive(Clone, Debug)]
pub struct ShellState {
    pub run_mode: RunMode,
    /// The open global dialog: options, save/load slots or an exit confirmation (see
    /// [`ModalScreen::is_global`]). Seat dialogs live in [`SeatUi::modal`].
    pub modal: Option<ModalScreen>,
    /// Row selected in the global dialog.
    pub selected_menu_item: usize,
    pub selected_title_item: usize,
    pub selected_pause_item: usize,
    pub show_details: bool,
    /// The latest global message (saves, loads, volume, controllers, sessions), shown as a
    /// toast in every view.
    pub message: String,
    /// `update_ticks` when `message` last changed (drives the on-screen toast).
    pub message_tick: u64,
    /// The message `message_tick` was taken for.
    message_seen: String,
    pub master_volume: f32,
    pub fullscreen: bool,
    /// Windowed size/maximized state, kept current by the desktop app and persisted with settings.
    pub window: save::WindowSettings,
    pub settings_dirty: bool,
    pub save_dirty: bool,
    pub request_exit: bool,
    pub update_ticks: u64,
    pub play_seconds: f32,
    pub sound_cues: Vec<SoundCue>,
    pub online: OnlineShell,
    /// Clipboard text pasted this frame (Ctrl+V), for the descriptor form.
    pub pasted: Option<String>,
    /// Each local seat's active input device, in seat order (button prompts follow it).
    pub seat_devices: Vec<InputDevice>,
    /// Connected controllers by index and name (F3 overlay).
    pub gamepads: BTreeMap<u8, String>,
    /// Each local seat's own menus and interior, in seat order.
    pub seats: Vec<SeatUi>,
    /// The seat whose input drives the global menus: the one that opened the pause menu (seat
    /// 0 for the title, the lobby and the window-close prompt).
    pub menu_seat: usize,
    /// The seat that opened the pause menu (the menu says so; it drives the menu unless its
    /// controller was unplugged, see `menu_seat`).
    pub paused_by: usize,
    /// The new-world setup dialog, while [`ModalScreen::WorldSetup`] is open.
    pub world_setup: Option<WorldSetup>,
    /// Test-mode toggles chosen in the pause menu that the world has not applied yet (a paused
    /// local world applies them when it resumes).
    pub pending_test_mode: Option<TestModeState>,
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
            message: "Mine ore, sell cargo, and buy upgrades at the surface buildings.".to_owned(),
            message_tick: 0,
            message_seen: String::new(),
            master_volume: settings.master_volume,
            fullscreen: settings.fullscreen,
            window: settings.window,
            settings_dirty: false,
            save_dirty: false,
            request_exit: false,
            update_ticks: 0,
            play_seconds: 0.0,
            sound_cues: Vec::new(),
            online: OnlineShell::default(),
            pasted: None,
            seat_devices: Vec::new(),
            gamepads: BTreeMap::new(),
            seats: vec![SeatUi::new()],
            menu_seat: 0,
            paused_by: 0,
            world_setup: None,
            pending_test_mode: None,
        }
    }

    /// The device `seat`'s prompts are drawn for (the keyboard until a device is known).
    #[must_use]
    pub fn prompt_device(&self, seat: usize) -> InputDevice {
        self.seat_devices
            .get(seat)
            .copied()
            .unwrap_or(InputDevice::KEYBOARD)
    }

    /// Seat `seat`'s menus (an idle outdoor seat when it does not exist).
    #[must_use]
    pub fn seat(&self, seat: usize) -> &SeatUi {
        self.seats.get(seat).unwrap_or(&IDLE_SEAT)
    }

    /// Seat `seat`'s menus, adding seats up to it.
    pub fn seat_mut(&mut self, seat: usize) -> &mut SeatUi {
        if self.seats.len() <= seat {
            self.seats.resize_with(seat + 1, SeatUi::new);
        }
        &mut self.seats[seat]
    }

    /// Matches the number of local seats (new seats start outdoors).
    pub fn set_seat_count(&mut self, count: usize) {
        self.seats.resize_with(count.max(1), SeatUi::new);
        if self.menu_seat >= self.seats.len() {
            self.menu_seat = 0;
        }
    }

    /// Puts seat `seat` inside `zone` at the door, as walking in does.
    #[cfg(test)]
    pub fn enter_interior(&mut self, seat: usize, zone: SurfaceZone) {
        let device = self.prompt_device(seat);
        let mut cues = Vec::new();
        self.seat_mut(seat).enter_interior(zone, device, &mut cues);
        self.sound_cues.extend(cues);
    }

    /// Whether Page Up / Page Down tune live network values (the F3 overlay is open in a dev
    /// build) instead of scrolling dialogs. Exactly one of the two ever handles the keys.
    #[must_use]
    pub const fn tuning_keys_active(&self) -> bool {
        self.online.show_net_debug && cfg!(debug_assertions)
    }

    /// Whether the world is on screen behind the shell (not the title or online screens).
    #[must_use]
    pub const fn gameplay_visible(&self) -> bool {
        !matches!(self.run_mode, RunMode::Title | RunMode::Lobby)
    }

    /// Whether a global screen (title, pause, online, options, slots, exit prompt) is up: it
    /// covers every view and takes the menu seat's input.
    #[must_use]
    pub const fn global_overlay(&self) -> bool {
        !matches!(self.run_mode, RunMode::Playing) || self.modal.is_some()
    }

    /// Current settings for persisting.
    #[must_use]
    pub const fn settings(&self) -> save::SettingsFile {
        save::SettingsFile {
            master_volume: self.master_volume,
            fullscreen: self.fullscreen,
            window: self.window,
        }
    }

    /// Whether `seat`'s held gameplay controls should reach its rig this frame: the game runs,
    /// no global dialog is open and the seat is outdoors with nothing open.
    #[must_use]
    pub fn gameplay_input_enabled(&self, seat: usize) -> bool {
        let seat = self.seat(seat);
        matches!(self.run_mode, RunMode::Playing)
            && self.modal.is_none()
            && seat.modal.is_none()
            && !seat.inside()
    }

    /// Whether the local host should tick: while the game runs and at least one local seat is
    /// outdoors. Alone, entering a building freezes the world as it always has; in split-screen
    /// the world keeps running for whoever is still outside and freezes only once every seat is
    /// inside a building or someone opened the shared pause menu. A transaction submitted from
    /// a building menu always runs the ticks needed to resolve it. Online hosts never pause
    /// (see `Runtime::set_paused`).
    #[must_use]
    pub fn world_should_tick(&self) -> bool {
        let anyone_outdoors = self.seats.iter().any(|seat| !seat.inside());
        let pending = self.seats.iter().any(SeatUi::has_pending_service);
        (matches!(self.run_mode, RunMode::Playing) && anyone_outdoors) || pending
    }

    /// Resets per-run shell state after a new game or load: every seat is back outdoors.
    pub fn enter_world(&mut self, message: impl Into<String>, play_seconds: f32) {
        self.run_mode = RunMode::Playing;
        self.modal = None;
        self.selected_menu_item = 0;
        self.menu_seat = 0;
        for seat in &mut self.seats {
            seat.reset_outdoors();
        }
        self.save_dirty = false;
        self.play_seconds = play_seconds;
        self.pending_test_mode = None;
        self.message = message.into();
    }

    /// Handles one frame of shell input: `inputs` holds every local seat's input in seat order
    /// (seat 0 first; its keyboard owns the global hotkeys). While the game runs each seat's
    /// input drives only that seat's menus; the global menus take the menu seat's input.
    #[allow(
        clippy::too_many_lines,
        reason = "top-level shell dispatcher keeps frame order explicit"
    )]
    pub fn update(
        &mut self,
        inputs: &[PlayerInput],
        delta_seconds: f32,
        ctx: ShellContext<'_>,
    ) -> Vec<ShellRequest> {
        let primary = inputs.first().copied().unwrap_or_default();
        let mut requests = Vec::new();
        self.set_seat_count(ctx.players.len().max(inputs.len()));
        self.update_ticks = self.update_ticks.saturating_add(1);
        if self.message != self.message_seen {
            self.message_seen.clone_from(&self.message);
            self.message_tick = self.update_ticks;
        }
        for seat in &mut self.seats {
            seat.track_message(self.update_ticks);
            seat.tick_animation(delta_seconds);
        }
        self.sound_cues.clear();
        self.show_details = primary.details;
        let text_entry = self.text_entry();
        let in_game = !matches!(self.run_mode, RunMode::Title | RunMode::Lobby);
        if primary.net_debug {
            self.online.show_net_debug = !self.online.show_net_debug;
        }
        if self.tuning_keys_active() && !text_entry {
            if primary.tune_next {
                self.online.tune_parameter =
                    (self.online.tune_parameter + 1) % crate::runtime::TUNABLE_PARAMETERS.len();
            }
            for (pressed, step) in [(primary.page_up, 1), (primary.page_down, -1)] {
                if pressed {
                    requests.push(ShellRequest::AdjustTuning {
                        parameter: self.online.tune_parameter,
                        step,
                    });
                }
            }
        }
        if primary.save && in_game {
            requests.push(ShellRequest::QuickSave);
        }
        if primary.load && self.run_mode != RunMode::Lobby {
            requests.push(ShellRequest::QuickLoad);
        }
        self.reclaim_orphaned_menu();
        self.sync_pending_test_mode(ctx.world.test_mode());
        let menu_input = inputs.get(self.menu_seat).copied().unwrap_or(primary);
        if self.handle_exit_modal(menu_input, &mut requests) {
            return requests;
        }
        if primary.exit_requested {
            self.menu_seat = 0;
            self.request_exit_or_prompt();
            return requests;
        }
        if in_game && inputs.iter().copied().any(input_changes_game) {
            self.save_dirty = true;
        }
        if primary.local_multiplayer_toggle && in_game {
            requests.push(ShellRequest::AddLocalSeat);
        }
        if primary.volume_up && !text_entry {
            self.adjust_volume(0.1);
        }
        if primary.volume_down && !text_entry {
            self.adjust_volume(-0.1);
        }
        if primary.fullscreen {
            self.fullscreen = !self.fullscreen;
            self.settings_dirty = true;
            self.sound_cues.push(SoundCue::Ui);
        }
        if !text_entry {
            self.push_navigation_cues(inputs, menu_input);
        }
        match self.run_mode {
            RunMode::Title => {
                if self.modal == Some(ModalScreen::WorldSetup) {
                    self.handle_world_setup(primary, &mut requests);
                } else if !self.handle_menu_modal(primary, &mut requests) {
                    self.handle_title_menu(primary, ctx, &mut requests);
                }
            }
            RunMode::Paused => {
                let seed = ctx.world.world_settings().seed_text();
                self.handle_pause_menu(menu_input, &seed, ctx.world.test_mode(), &mut requests);
            }
            RunMode::Lobby => self.handle_online(primary, &mut requests),
            RunMode::Playing => {
                self.play_seconds += delta_seconds;
                let mut first_pauser = None;
                // Every seat's press of this frame lands, whatever another seat did: a door,
                // a purchase or a rescue pressed on the frame someone pauses still happens.
                for seat in 0..self.seats.len() {
                    let input = inputs.get(seat).copied().unwrap_or_default();
                    let seat_ctx = ctx.seat(seat, self.prompt_device(seat));
                    let paused = self.seats[seat].update(
                        input,
                        delta_seconds,
                        seat_ctx,
                        &mut self.sound_cues,
                        &mut requests,
                    );
                    if paused && first_pauser.is_none() {
                        first_pauser = Some(seat);
                    }
                }
                if let Some(seat) = first_pauser {
                    // Any seat opens the shared pause menu and drives it until it closes.
                    self.run_mode = RunMode::Paused;
                    self.menu_seat = seat;
                    self.paused_by = seat;
                }
            }
        }
        requests
    }

    /// Menu navigation sounds: a tick when an open menu's selection steps, a back sound when one
    /// is backed out of. Global menus listen to the seat driving them; while playing, each seat
    /// with its own dialog open is heard.
    fn push_navigation_cues(&mut self, inputs: &[PlayerInput], menu_input: PlayerInput) {
        let stepped = |input: &PlayerInput| input.menu_up || input.menu_down;
        let backed = |input: &PlayerInput| input.back || input.cancel;
        let global = !matches!(self.run_mode, RunMode::Playing) || self.modal.is_some();
        let menus: Vec<PlayerInput> = if global {
            vec![menu_input]
        } else {
            inputs
                .iter()
                .enumerate()
                .filter(|(seat, _)| {
                    self.seats
                        .get(*seat)
                        .is_some_and(|seat| seat.modal.is_some())
                })
                .map(|(_, input)| *input)
                .collect()
        };
        for input in &menus {
            if stepped(input) {
                self.sound_cues.push(SoundCue::UiMove);
            }
            if backed(input) {
                self.sound_cues.push(SoundCue::UiBack);
            }
        }
    }

    /// Hands the global menus to seat 0 when the seat driving them can no longer press anything:
    /// its controller was unplugged (or it has no device at all). Otherwise a player who pauses
    /// and pulls their controller would leave everyone stuck in the pause menu. Seat 0 always
    /// has the keyboard.
    fn reclaim_orphaned_menu(&mut self) {
        if self.menu_seat == 0 || !self.global_overlay() {
            return;
        }
        let orphaned = match self.prompt_device(self.menu_seat) {
            InputDevice::Gamepad { index, .. } => !self.gamepads.contains_key(&index),
            InputDevice::Unassigned => true,
            InputDevice::Keyboard(_) => false,
        };
        if orphaned {
            self.message = format!(
                "Player {}'s controller disconnected: Player 1 has the menu.",
                self.menu_seat + 1
            );
            self.menu_seat = 0;
        }
    }

    /// Reports a resolved service action of seat `seat`'s player (from the tick's events).
    pub fn service_resolved(
        &mut self,
        seat: usize,
        request: SessionServiceRequest,
        applied: bool,
        ctx: ShellContext<'_>,
    ) {
        let seat_ctx = ctx.seat(seat, self.prompt_device(seat));
        if let Some(state) = self.seats.get_mut(seat) {
            state.service_resolved(request, applied, seat_ctx, &mut self.sound_cues);
        }
    }

    /// Raises a quest banner in seat `seat`'s view.
    pub fn quest_fanfare(
        &mut self,
        seat: usize,
        kind: FanfareKind,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let tick = self.update_ticks;
        if let Some(state) = self.seats.get_mut(seat) {
            state.fanfare = Some(Fanfare {
                kind,
                title: title.into(),
                detail: detail.into(),
                tick,
            });
        }
    }

    /// Whether an online text field has focus (typing must not trigger hotkeys).
    #[must_use]
    pub fn text_entry(&self) -> bool {
        let online_form = matches!(self.run_mode, RunMode::Lobby)
            && matches!(
                self.online.screen,
                OnlineScreen::JoinAddress | OnlineScreen::JoinDescriptor
            );
        let seed_field = self.modal == Some(ModalScreen::WorldSetup)
            && self.world_setup.as_ref().is_some_and(WorldSetup::typing);
        online_form || seed_field
    }

    /// Shows an online screen.
    pub fn show_online(&mut self, screen: OnlineScreen) {
        self.run_mode = RunMode::Lobby;
        self.modal = None;
        self.menu_seat = 0;
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
        self.menu_seat = 0;
        for seat in &mut self.seats {
            seat.reset_outdoors();
        }
        self.selected_title_item = 0;
        self.online.ready = false;
        self.message = message.into();
    }

    fn handle_online(&mut self, input: PlayerInput, requests: &mut Vec<ShellRequest>) {
        let pasted = self.pasted.take();
        match self.online.screen {
            OnlineScreen::JoinMenu => {
                if input.backs_out() {
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
                if input.backs_out() {
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
                if input.backs_out() && self.online.started() {
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
                            self.message = format!(
                                "Descriptor copied: joiners paste it with {}.",
                                inline_text(Prompt::Paste, InputDevice::KEYBOARD)
                                    .unwrap_or_default()
                            );
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
                if input.backs_out() {
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
        } else if input.backs_out() {
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
            TitleOption::NewGame => self.open_world_setup(SetupPurpose::Local { seats: 1 }),
            TitleOption::LocalMultiplayer => {
                self.open_world_setup(SetupPurpose::Local { seats: 2 });
            }
            TitleOption::HostOnline => self.open_world_setup(SetupPurpose::HostOnline),
            TitleOption::JoinOnline => self.show_online(OnlineScreen::JoinMenu),
            TitleOption::LoadSlot => self.open_modal(ModalScreen::LoadSlots),
            TitleOption::Options => self.open_modal(ModalScreen::Options),
            TitleOption::Exit => {
                self.request_exit = true;
                requests.push(ShellRequest::Exit);
            }
        }
    }

    /// Opens the world setup dialog with a fresh random seed.
    pub fn open_world_setup(&mut self, purpose: SetupPurpose) {
        self.world_setup = Some(WorldSetup::new(purpose, random_world_seed()));
        self.open_modal(ModalScreen::WorldSetup);
    }

    fn handle_world_setup(&mut self, input: PlayerInput, requests: &mut Vec<ShellRequest>) {
        let pasted = self.pasted.take();
        let Some(setup) = self.world_setup.as_mut() else {
            self.modal = None;
            return;
        };
        match setup.update(input, pasted, random_world_seed) {
            SetupOutcome::Stay => {}
            SetupOutcome::Close => {
                self.modal = None;
                self.world_setup = None;
            }
            SetupOutcome::Start(purpose, settings, test_mode) => {
                self.modal = None;
                self.world_setup = None;
                requests.push(match purpose {
                    SetupPurpose::Local { seats } => ShellRequest::NewGame {
                        seats,
                        settings,
                        test_mode,
                    },
                    SetupPurpose::HostOnline => ShellRequest::HostOnline {
                        settings,
                        test_mode,
                    },
                });
            }
        }
    }

    fn handle_pause_menu(
        &mut self,
        input: PlayerInput,
        seed: &str,
        test_mode: Option<TestModeState>,
        requests: &mut Vec<ShellRequest>,
    ) {
        if self.modal.is_some() {
            // Slot / options modals opened from the pause menu.
            let _handled = self.handle_menu_modal(input, requests);
            return;
        }
        let options = PauseOption::for_session(self.online.role, test_mode.is_some());
        if input.menu_up {
            self.selected_pause_item = self.selected_pause_item.saturating_sub(1);
        }
        if input.menu_down {
            self.selected_pause_item += 1;
        }
        self.selected_pause_item = self.selected_pause_item.min(options.len() - 1);
        if input.pause || input.backs_out() {
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
            PauseOption::CopySeed => {
                requests.push(ShellRequest::CopyToClipboard(seed.to_owned()));
                self.message = format!("World seed {seed} copied: share it to share this world.");
            }
            PauseOption::ExitToDesktop => self.request_exit_or_prompt(),
            PauseOption::WarpToLayer => self.open_modal(ModalScreen::WarpToLayer),
            PauseOption::ToggleInvulnerable | PauseOption::ToggleInfiniteFuel => {
                self.toggle_test_mode(options[self.selected_pause_item], test_mode, requests);
            }
        }
    }

    /// Slot and options modals usable from title and pause menus.
    fn handle_menu_modal(&mut self, input: PlayerInput, requests: &mut Vec<ShellRequest>) -> bool {
        let Some(modal) = self.modal else {
            return false;
        };
        if input.backs_out() {
            self.modal = None;
            return true;
        }
        let max_item = match modal {
            ModalScreen::Options => 2,
            ModalScreen::WarpToLayer => test_mode::WARP_BANDS.len() - 1,
            ModalScreen::SaveSlots | ModalScreen::LoadSlots => save::save_slot_count() - 1,
            _ => 0,
        };
        self.move_selection(input, max_item);
        if modal == ModalScreen::Options && self.selected_menu_item == 0 {
            if input.menu_left || input.tab_previous {
                self.adjust_volume(-0.1);
            } else if input.menu_right || input.tab_next {
                self.adjust_volume(0.1);
            }
        }
        if input.confirm {
            match modal {
                ModalScreen::Options => self.confirm_options(),
                ModalScreen::SaveSlots => {
                    requests.push(ShellRequest::SaveSlot(self.selected_menu_item));
                }
                ModalScreen::LoadSlots => {
                    requests.push(ShellRequest::LoadSlot(self.selected_menu_item));
                }
                ModalScreen::WarpToLayer => self.confirm_warp(requests),
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

    /// Closes an exit prompt. Over the pause menu the seat that paused gets the menu back (the
    /// window-close prompt is always answered by player 1).
    fn close_exit_prompt(&mut self) {
        self.modal = None;
        if self.run_mode == RunMode::Paused {
            self.menu_seat = self.paused_by.min(self.seats.len().saturating_sub(1));
        }
    }

    fn handle_exit_modal(&mut self, input: PlayerInput, requests: &mut Vec<ShellRequest>) -> bool {
        match self.modal {
            Some(ModalScreen::ExitConfirm) => {
                if input.backs_out() {
                    self.close_exit_prompt();
                } else if input.confirm {
                    self.request_exit = true;
                    requests.push(ShellRequest::Exit);
                }
                true
            }
            Some(ModalScreen::UnsavedExitConfirm) => {
                if input.backs_out() {
                    self.close_exit_prompt();
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
                        _ => self.close_exit_prompt(),
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Confirm on an options row (the rows `menus::draw_options` lists): volume steps up,
    /// fullscreen toggles, back closes.
    fn confirm_options(&mut self) {
        match self.selected_menu_item {
            0 => self.adjust_volume(0.1),
            1 => {
                self.fullscreen = !self.fullscreen;
                let mode = if self.fullscreen {
                    "Fullscreen"
                } else {
                    "Windowed"
                };
                self.message = inline_text(Prompt::Fullscreen, self.prompt_device(0)).map_or_else(
                    || format!("{mode} enabled."),
                    |key| format!("{mode} enabled; {key} toggles it anywhere."),
                );
                self.settings_dirty = true;
                self.sound_cues.push(SoundCue::Ui);
            }
            _ => self.modal = None,
        }
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
            "Upgrade unavailable: check credits, tier and depth unlocks."
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
    // Story beats follow classic nominal depths (deep worlds stretch them with their bands).
    match world
        .terrain()
        .nominal_row(world.progression().deepest_tile_reached)
    {
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
        assert!(
            ModalScreen::UnsavedExitConfirm.is_global() && ModalScreen::ExitConfirm.is_global()
        );
        assert!(!ModalScreen::Shop.is_global() && !ModalScreen::Inventory.is_global());

        shell.request_exit_or_prompt();
        assert_eq!(shell.modal, Some(ModalScreen::UnsavedExitConfirm));

        shell.modal = None;
        shell.online.role = Some(OnlineRole::Client);
        shell.request_exit_or_prompt();
        assert_eq!(shell.modal, Some(ModalScreen::ExitConfirm));
    }

    fn ctx(world: &WorldState) -> ShellContext<'_> {
        // Tests are short-lived: leaking the seat list keeps the context `Copy` like the app's.
        let players: &'static [PlayerId] = Box::leak(world.player_ids().collect());
        ShellContext {
            world,
            players,
            saves_exist: false,
        }
    }

    fn back() -> PlayerInput {
        PlayerInput {
            back: true,
            ..PlayerInput::default()
        }
    }

    #[test]
    fn page_keys_tune_only_while_the_dev_overlay_is_open() {
        let world = WorldState::new_game(PlayerId::new(1));
        let mut shell = ShellState::default();
        shell.enter_world("test", 0.0);
        let page_up = PlayerInput {
            page_up: true,
            ..PlayerInput::default()
        };
        let tunes = |requests: &[ShellRequest]| {
            requests
                .iter()
                .any(|request| matches!(request, ShellRequest::AdjustTuning { step: 1, .. }))
        };
        assert!(
            !shell.tuning_keys_active(),
            "closed overlay: the keys scroll"
        );
        assert!(!tunes(&shell.update(&[page_up], 0.016, ctx(&world))));

        shell.online.show_net_debug = true;
        assert_eq!(shell.tuning_keys_active(), cfg!(debug_assertions));
        assert_eq!(
            tunes(&shell.update(&[page_up], 0.016, ctx(&world))),
            cfg!(debug_assertions),
            "open overlay in a dev build: the keys tune (and the renderer leaves them alone)"
        );
    }

    #[test]
    fn gamepad_back_closes_menus_but_never_pauses_or_quits() {
        let world = WorldState::new_game(PlayerId::new(1));
        let mut shell = ShellState::default();
        let requests = shell.update(&[back()], 0.016, ctx(&world));
        assert!(
            requests.is_empty() && !shell.request_exit,
            "B on the title does nothing"
        );
        assert_eq!(shell.run_mode, RunMode::Title);

        shell.enter_world("test", 0.0);
        shell.update(&[back()], 0.016, ctx(&world));
        assert_eq!(
            shell.run_mode,
            RunMode::Playing,
            "B while playing does not pause"
        );

        shell.seat_mut(0).modal = Some(ModalScreen::Inventory);
        shell.update(&[back()], 0.016, ctx(&world));
        assert_eq!(shell.seat(0).modal, None, "B closes a modal");

        shell.run_mode = RunMode::Paused;
        shell.update(&[back()], 0.016, ctx(&world));
        assert_eq!(
            shell.run_mode,
            RunMode::Playing,
            "B resumes from the pause menu"
        );

        shell.seat_mut(0).interior_zone = Some(SurfaceZone::Fuel);
        shell.update(&[back()], 0.016, ctx(&world));
        assert!(!shell.seat(0).inside(), "B leaves a building");
        assert_eq!(shell.run_mode, RunMode::Playing);

        let escape = PlayerInput {
            cancel: true,
            ..PlayerInput::default()
        };
        shell.update(&[escape], 0.016, ctx(&world));
        assert_eq!(shell.run_mode, RunMode::Paused, "Esc still pauses");
    }

    #[test]
    fn menus_tick_on_navigation_but_driving_stays_quiet() {
        let world = WorldState::new_game(PlayerId::new(1));
        let down = PlayerInput {
            menu_down: true,
            ..PlayerInput::default()
        };
        let mut shell = ShellState::default();
        shell.update(&[down], 0.016, ctx(&world));
        assert!(shell.sound_cues.contains(&SoundCue::UiMove));
        // Outdoors with nothing open, up/down drive the rig: no menu sounds.
        shell.enter_world("", 0.0);
        shell.update(&[down], 0.016, ctx(&world));
        assert!(!shell.sound_cues.contains(&SoundCue::UiMove));
    }

    #[test]
    fn options_adjust_volume_with_left_right_and_confirm_matches_the_rows() {
        let world = WorldState::new_game(PlayerId::new(1));
        let mut shell = ShellState {
            master_volume: 0.5,
            modal: Some(ModalScreen::Options),
            ..ShellState::default()
        };
        shell.update(
            &[PlayerInput {
                tab_next: true,
                ..PlayerInput::default()
            }],
            0.016,
            ctx(&world),
        );
        assert!((shell.master_volume - 0.6).abs() < 1e-5);
        shell.update(
            &[PlayerInput {
                menu_left: true,
                ..PlayerInput::default()
            }],
            0.016,
            ctx(&world),
        );
        assert!((shell.master_volume - 0.5).abs() < 1e-5);
        shell.selected_menu_item = 1;
        let fullscreen = shell.fullscreen;
        shell.update(
            &[PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            }],
            0.016,
            ctx(&world),
        );
        assert_ne!(shell.fullscreen, fullscreen, "row 1 toggles fullscreen");
        assert!(
            (shell.master_volume - 0.5).abs() < 1e-5,
            "and leaves the volume"
        );
        shell.selected_menu_item = 2;
        shell.update(
            &[PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            }],
            0.016,
            ctx(&world),
        );
        assert_eq!(shell.modal, None, "Back closes");
    }

    #[test]
    fn messages_name_the_active_devices_buttons() {
        let mut shell = ShellState::default();
        shell.enter_interior(0, SurfaceZone::Fuel);
        let message = &shell.seat(0).message;
        assert!(message.contains("press E"), "{message}");
        shell.seat_devices = vec![
            InputDevice::KEYBOARD,
            InputDevice::Gamepad {
                index: 0,
                family: crate::input::devices::PadFamily::Xbox,
            },
        ];
        shell.enter_interior(1, SurfaceZone::Fuel);
        let message = &shell.seat(1).message;
        assert!(message.contains("press X"), "{message}");
        assert!(message.contains("B leaves"), "{message}");
        assert!(
            shell.seat(0).message.contains("press E"),
            "each seat's messages name its own device"
        );
    }
}
