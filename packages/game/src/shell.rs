//! Shell state: menus, modals, interiors, settings and local presentation timers.
//!
//! The shell owns no gameplay truth (plan §10). It reads the world through [`ShellContext`] to
//! decide what a menu shows or allows, and turns menu choices into [`ShellRequest`]s that the app
//! carries out through the runtime (service actions, rescue) or the save module.

use drillgame_simulation::{
    PlayerCommand, PlayerId, RecipeKind, SessionServiceRequest, WorldState,
    economy::{DeepClaimStatus, SurfaceZone, TownBuilding, upgrade_offers, upgrade_tier_name},
    surface::surface_building_at_tile,
    terrain::TILE_SIZE,
};

use crate::{input::PlayerInput, save};

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

/// Pause menu entries. (Online session entries return with the online runtime.)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PauseOption {
    Resume,
    Save,
    Load,
    Options,
    ExitToDesktop,
}

impl PauseOption {
    pub const ALL: [Self; 5] = [
        Self::Resume,
        Self::Save,
        Self::Load,
        Self::Options,
        Self::ExitToDesktop,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume",
            Self::Save => "Save Game",
            Self::Load => "Load Game",
            Self::Options => "Options",
            Self::ExitToDesktop => "Exit to Desktop",
        }
    }
}

/// Title menu entries. (Host/Join return with the online runtime.)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TitleOption {
    Resume,
    NewGame,
    LocalMultiplayer,
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
            Self::LoadSlot => "Load Slot",
            Self::Options => "Options",
            Self::Exit => "Exit",
        }
    }

    /// Title entries, with "Resume" first when a save exists.
    #[must_use]
    pub fn all(saves_exist: bool) -> Vec<Self> {
        let mut options = Vec::with_capacity(6);
        if saves_exist {
            options.push(Self::Resume);
        }
        options.extend([Self::NewGame, Self::LocalMultiplayer]);
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
    /// Close the app.
    Exit,
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

    fn current_zone(&self) -> Option<SurfaceZone> {
        let player = self.player()?;
        if player.y > 5.5 * TILE_SIZE {
            return None;
        }
        surface_building_at_tile(player.tile_position(TILE_SIZE).x).map(|building| building.zone)
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
        if input.save && self.run_mode != RunMode::Title {
            requests.push(ShellRequest::QuickSave);
        }
        if input.load {
            requests.push(ShellRequest::QuickLoad);
        }
        if self.handle_exit_modal(input, &mut requests) {
            return requests;
        }
        if input.exit_requested {
            self.request_exit_or_prompt();
            return requests;
        }
        if self.run_mode != RunMode::Title && input_changes_game(input) {
            self.save_dirty = true;
        }
        if input.local_multiplayer_toggle && self.run_mode != RunMode::Title {
            requests.push(ShellRequest::AddLocalSeat);
        }
        if self.run_mode != RunMode::Title {
            toggle_modal(&mut self.modal, input.inventory, ModalScreen::Inventory);
            toggle_modal(&mut self.modal, input.map, ModalScreen::Map);
            toggle_modal(&mut self.modal, input.help, ModalScreen::Help);
        }
        if input.volume_up {
            self.adjust_volume(0.1);
        }
        if input.volume_down {
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
            RunMode::Interior => self.handle_interior(input, delta_seconds, ctx, &mut requests),
            RunMode::Playing => {
                if ctx.failed() {
                    if input.interact || input.confirm {
                        requests.push(ShellRequest::Action(PlayerCommand::Rescue));
                        "Rescue crew dispatched.".clone_into(&mut self.message);
                        self.sound_cues.push(SoundCue::Rescue);
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
        if input.menu_up {
            self.selected_pause_item = self.selected_pause_item.saturating_sub(1);
        }
        if input.menu_down {
            self.selected_pause_item =
                (self.selected_pause_item + 1).min(PauseOption::ALL.len() - 1);
        }
        if input.pause || input.cancel {
            self.run_mode = RunMode::Playing;
            return;
        }
        if !input.confirm {
            return;
        }
        match PauseOption::ALL[self.selected_pause_item] {
            PauseOption::Resume => self.run_mode = RunMode::Playing,
            PauseOption::Save => self.open_modal(ModalScreen::SaveSlots),
            PauseOption::Load => self.open_modal(ModalScreen::LoadSlots),
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
        if self.save_dirty {
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
