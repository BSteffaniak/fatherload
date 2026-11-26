//! Contractor-built homes in the shell (WP-HOME): the land office, surveying a spot, the site
//! board on a plot (blueprints, contractors, rushing, site events, the deed), walking into a
//! finished home on foot, its rooms and decorating.
//!
//! Every home screen is one modal ([`ModalScreen::Home`]) whose rows the seat rebuilds from
//! the world every frame ([`HomeView`]): each row carries the action confirming it does, so a
//! tap, a click, the keyboard and a controller all pick rows the same way, and the renderer
//! only draws what is listed. Actions are [`HomeCommand`]s checked with
//! [`WorldState::check_home`] first, so a refused row says why instead of failing silently.
//!
//! Inside a home the pilot walks the rooms of one floor left and right (the shell's room
//! units, like village buildings); the front door is at the left of the first ground-floor
//! room, stairs stand at the right of each floor's first room, and walking off a room's edge
//! goes through to the next. The world keeps ticking indoors (the clock shows in the windows,
//! the bed works at night), while the pilot outside stands at the door with neutral controls.

use fatherload_simulation::{
    PlayerCommand, PlayerId, WorldState,
    home::{
        Blueprint, BuildStage, Expansion, Furniture, HomeAccess, HomeCommand, HomeJob, HomeRefusal,
        Plot, PlotId, PlotSize, SiteEventKind, SiteKey,
        decor::{DecorRow, DecorService, ROOM_COLUMNS, ROOM_ROWS, displayable},
    },
    item::ItemKind,
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::{PlayerInput, prompts::Prompt};

/// Room units (0..640) of the front door's middle.
pub const HOME_DOOR_X: f32 = 40.0;
/// Room units of the staircase up (first room of each floor).
pub const STAIRS_UP_X: f32 = 520.0;
/// Room units of the staircase down.
pub const STAIRS_DOWN_X: f32 = 590.0;
/// Reach (room units) for doors, stairs and furniture.
pub const HOME_REACH: f32 = 34.0;
/// Room units per grid column (8 columns between the walls).
pub const CELL_UNITS: f32 = 64.0;
/// Room units of the first grid column's left edge.
pub const GRID_LEFT: f32 = 64.0;
/// Frames the world keeps ticking after a home command (a few ticks of round trip).
const HOME_PENDING_FRAMES: u8 = 20;
/// How far (land office) listed sites are looked for around the office.
const OFFICE_REACH_COLUMNS: i32 = 2_400;

/// Which home screen the modal shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeScreen {
    /// The land office: listed sites near the office, your deeds, the furniture catalogue.
    LandOffice,
    /// The furniture catalogue.
    Catalogue,
    /// Surveying the spot the seat stands on (sizes and prices).
    Survey,
    /// A plot's site board.
    Site,
    /// Choosing a contractor (and whether to supply blocks) for a job.
    Contract(HomeJob),
    /// The deed: visitors and co-owners.
    Deed,
    /// Inside: the home menu (decorate, storage, crafting, leaving).
    Menu,
    /// Decorating the current room.
    Decor,
    /// Choosing a piece from stock for the current room.
    DecorPick,
    /// Choosing a spot for `Furniture`.
    DecorSpot(Furniture),
    /// The home's storage.
    Store,
    /// Choosing a trophy for the case at `cell` of the current room.
    Display(u8),
    /// The workbench's recipes.
    Craft,
}

/// What confirming a row does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeAction {
    /// Send a home command (checked first).
    Command(HomeCommand),
    /// Open another home screen.
    Open(HomeScreen),
    /// Pick the contractor at this index of the offer list.
    Contractor(usize),
    /// Flip "supply blocks".
    ToggleSupply,
    /// Sign the contract being drafted.
    Sign,
    /// Walk into the finished home.
    GoInside,
    /// Leave the home.
    Leave,
    /// A line to read (nothing happens).
    Info,
}

/// One row of a home screen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomeRow {
    pub label: String,
    pub detail: String,
    pub value: String,
    pub enabled: bool,
    /// Why it is refused (the footer shows it when selected).
    pub refusal: Option<String>,
    pub action: HomeAction,
    /// Selected (a contractor, the supply toggle).
    pub chosen: bool,
}

impl HomeRow {
    fn new(label: impl Into<String>, value: impl Into<String>, action: HomeAction) -> Self {
        Self {
            label: label.into(),
            detail: String::new(),
            value: value.into(),
            enabled: true,
            refusal: None,
            action,
            chosen: false,
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    fn refused(mut self, refusal: Option<HomeRefusal>) -> Self {
        if let Some(refusal) = refusal {
            self.enabled = false;
            self.refusal = Some(refusal.message().to_owned());
        }
        self
    }

    const fn chosen(mut self, chosen: bool) -> Self {
        self.chosen = chosen;
        self
    }
}

/// A status line at the top of a home screen.
#[derive(Clone, Debug, PartialEq)]
pub struct HomeStatus {
    pub label: String,
    pub value: String,
    /// 0..1 progress bar (`None`: a plain line).
    pub meter: Option<f32>,
}

/// What the home modal shows this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HomeView {
    pub title: String,
    pub subtitle: String,
    pub status: Vec<HomeStatus>,
    pub rows_title: String,
    pub rows: Vec<HomeRow>,
    /// A line from the contractor (or the clerk).
    pub quote: Option<String>,
}

/// Where the seat is inside a home.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HomeInterior {
    pub plot: PlotId,
    /// Index into the home's rooms.
    pub room: usize,
    /// Room units (0..640).
    pub x: f32,
    pub facing: f32,
    pub stride: f32,
}

/// The contract being drafted on the site board.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HireDraft {
    /// Index into the contractor offers.
    pub contractor: usize,
    pub supply: bool,
}

/// A seat's home state.
#[derive(Clone, Debug, Default)]
pub struct SeatHome {
    pub interior: Option<HomeInterior>,
    pub screen: Option<HomeScreen>,
    /// The plot the site board, contract or deed screens refer to.
    pub plot: Option<PlotId>,
    pub draft: HireDraft,
    pub view: HomeView,
    /// The screen opened from (Back returns to it).
    back: Vec<HomeScreen>,
    /// Frames the world keeps ticking for a home command this seat just sent (a single-player
    /// world rests while its seat is inside a town or village building, like the land office).
    pending_frames: u8,
}

impl SeatHome {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            interior: None,
            screen: None,
            plot: None,
            draft: HireDraft {
                contractor: 0,
                supply: false,
            },
            view: HomeView {
                title: String::new(),
                subtitle: String::new(),
                status: Vec::new(),
                rows_title: String::new(),
                rows: Vec::new(),
                quote: None,
            },
            back: Vec::new(),
            pending_frames: 0,
        }
    }

    /// Whether a home command from this seat may still be waiting for its tick.
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.pending_frames > 0
    }

    /// One frame passed.
    pub const fn tick_pending(&mut self) {
        self.pending_frames = self.pending_frames.saturating_sub(1);
    }

    /// The decorating preview: the spot of the selected row on the spot screen.
    #[must_use]
    pub fn preview(&self, selected: usize) -> Option<(u8, Furniture)> {
        let Some(HomeScreen::DecorSpot(furniture)) = self.screen else {
            return None;
        };
        match self.view.rows.get(selected)?.action {
            HomeAction::Command(HomeCommand::Place { cell, .. }) => Some((cell, furniture)),
            _ => None,
        }
    }
}

/// The plot whose front door `player`'s pilot stands at, if it may walk in.
#[must_use]
pub fn home_door_here(world: &WorldState, player: PlayerId) -> Option<PlotId> {
    let tile = world.actor_tile(player)?;
    let plot = world.homes().plot_at(tile)?;
    let home = plot.home.as_ref()?;
    let door = plot.door_column();
    let near_door = (tile.x - door).abs() <= 1 && (tile.y - plot.site.ground_row).abs() <= 3;
    let _ = home;
    (near_door && world.may_enter_home(player, plot.id)).then_some(plot.id)
}

/// The grid cell a room unit position faces (the floor row).
#[must_use]
pub fn cell_column_at(x: f32) -> Option<u8> {
    let column = ((x - GRID_LEFT) / CELL_UNITS).floor();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small column index"
    )]
    (0.0..f32::from(ROOM_COLUMNS))
        .contains(&column)
        .then_some(column as u8)
}

/// A spot's name ("floor 3", "wall 5").
#[must_use]
pub fn spot_name(cell: u8) -> String {
    let column = cell % ROOM_COLUMNS + 1;
    match DecorRow::of_cell(cell) {
        DecorRow::Floor => format!("Floor, spot {column}"),
        DecorRow::Wall => format!("Wall, spot {column}"),
        DecorRow::Ceiling => format!("Ceiling, spot {column}"),
    }
}

/// The plot's name for lists ("Medium plot, edge of town").
#[must_use]
pub fn plot_name(plot: &Plot) -> String {
    format!(
        "{}, {}",
        plot.site.size.name(),
        plot.site.setting.name().to_lowercase()
    )
}

fn item_name(item: ItemKind) -> String {
    match item {
        ItemKind::Mineral(mineral) => mineral.name().to_owned(),
        ItemKind::Artifact(artifact) => artifact.name().to_owned(),
        other => other.name().to_owned(),
    }
}

/// "about 3 days", "half a day": world milliseconds as in-game days.
#[must_use]
pub fn days_text(millis: u64, day_millis: u64) -> String {
    let tenths = millis * 10 / day_millis.max(1);
    match tenths {
        0..=2 => "a few hours".to_owned(),
        3..=6 => "half a day".to_owned(),
        7..=12 => "about a day".to_owned(),
        13..=17 => "a day and a half".to_owned(),
        _ => format!("about {} days", tenths.div_ceil(10)),
    }
}

impl SeatUi {
    /// Opens home screen `screen` (about `plot`).
    pub(super) fn open_home(&mut self, screen: HomeScreen, plot: Option<PlotId>) {
        if let Some(current) = self.home.screen
            && self.modal == Some(ModalScreen::Home)
            && current != screen
        {
            self.home.back.push(current);
        } else {
            self.home.back.clear();
        }
        self.home.screen = Some(screen);
        if plot.is_some() {
            self.home.plot = plot;
        }
        self.modal = Some(ModalScreen::Home);
        self.selected_menu_item = 0;
    }

    /// Opens home screen `screen` about `plot` straight away (screenshot scenes and tests);
    /// the rows fill in on the next frame.
    #[cfg(any(test, not(target_os = "emscripten")))]
    pub fn open_home_screen(&mut self, screen: HomeScreen, plot: Option<PlotId>) {
        self.open_home(screen, plot);
    }

    /// Closes the home modal (and its screen stack).
    fn close_home(&mut self) {
        self.home.screen = None;
        self.home.back.clear();
        if self.modal == Some(ModalScreen::Home) {
            self.modal = None;
        }
    }

    /// Back one home screen (closing the modal from the first one).
    fn home_back(&mut self) {
        if let Some(previous) = self.home.back.pop() {
            self.home.screen = Some(previous);
            self.selected_menu_item = 0;
        } else {
            self.close_home();
        }
    }

    /// Use on a plot: opens its site board. Returns whether it did.
    pub(super) fn try_open_site(&mut self, ctx: SeatContext<'_>, cues: &mut Vec<SoundCue>) -> bool {
        let Some(plot) = ctx.player.and_then(|player| ctx.world.plot_here(player)) else {
            return false;
        };
        let id = plot.id;
        self.home.draft = HireDraft::default();
        self.open_home(HomeScreen::Site, Some(id));
        cues.push(SoundCue::Ui);
        true
    }

    /// The lift from a base claim up to a home: use in the claim rides it. Returns whether it
    /// did.
    pub(super) fn try_lift_home(
        &mut self,
        ctx: SeatContext<'_>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        let Some(player) = ctx.player else {
            return false;
        };
        if !ctx.on_foot() {
            return false;
        }
        let Some(plot) = ctx.world.lift_home_from_here(player) else {
            return false;
        };
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::Home(HomeCommand::RideLift { plot, down: false }),
        });
        "The private lift hums up to your house.".clone_into(&mut self.message);
        true
    }

    /// On foot outdoors: pushing up at a finished home's front door (or under the cellar
    /// hatch) walks in. Returns whether the seat went inside.
    pub(super) fn walk_into_home(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
    ) -> bool {
        if !ctx.on_foot() || self.modal.is_some() || !input.thrust || input.jump {
            return false;
        }
        let Some(player) = ctx.player else {
            return false;
        };
        if let Some(plot) = home_door_here(ctx.world, player) {
            if self.door_latch {
                return false;
            }
            self.enter_home(ctx, plot, 0, HOME_DOOR_X + 46.0, cues);
            return true;
        }
        if let Some(plot) = ctx.world.in_cellar_of(player) {
            // Up the cellar ladder into the basement.
            let basement = ctx
                .world
                .homes()
                .plot(plot)
                .and_then(|plot| plot.home.as_ref())
                .and_then(|home| home.rooms.iter().position(|room| room.floor < 0));
            if let Some(room) = basement {
                self.enter_home(ctx, plot, room, 300.0, cues);
                return true;
            }
        }
        false
    }

    /// Puts the seat inside `plot`'s home in room `room` at room units `x`.
    pub(super) fn enter_home(
        &mut self,
        ctx: SeatContext<'_>,
        plot: PlotId,
        room: usize,
        x: f32,
        cues: &mut Vec<SoundCue>,
    ) {
        self.home.interior = Some(HomeInterior {
            plot,
            room,
            x,
            facing: 1.0,
            stride: 0.0,
        });
        self.close_home();
        self.door_latch = true;
        cues.push(SoundCue::HatchOpen);
        let name = ctx
            .world
            .homes()
            .plot(plot)
            .and_then(|plot| plot.home.as_ref())
            .map_or("the house", |home| home.blueprint.name());
        let owner = ctx
            .world
            .homes()
            .plot(plot)
            .is_some_and(|plot| ctx.player.is_some_and(|player| plot.deed.owns(player)));
        self.message = if owner {
            format!(
                "Home. Walk the rooms; {} uses furniture, stairs and the door, {} leaves.",
                super::key_for(Prompt::Use, ctx.device),
                super::key_for(Prompt::Back, ctx.device),
            )
        } else {
            format!(
                "Visiting the {}. Make yourself at home.",
                name.to_lowercase()
            )
        };
    }

    /// Leaves the home: back outside at its door.
    fn leave_home(&mut self, cues: &mut Vec<SoundCue>) {
        self.home.interior = None;
        self.close_home();
        self.door_latch = true;
        cues.push(SoundCue::HatchClose);
        "Back outside.".clone_into(&mut self.message);
    }

    /// One frame inside a home (walking, doors, stairs, furniture). Returns `true` when the
    /// seat asked for the pause menu.
    #[allow(
        clippy::too_many_lines,
        reason = "walking, doors, stairs and furniture are one frame of the room"
    )]
    pub(super) fn handle_home_interior(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        let Some(mut inside) = self.home.interior else {
            return false;
        };
        let home = ctx
            .world
            .homes()
            .plot(inside.plot)
            .and_then(|plot| plot.home.clone());
        let allowed = ctx
            .player
            .is_some_and(|player| ctx.world.may_enter_home(player, inside.plot));
        let Some(home) = home.filter(|_| allowed && ctx.on_foot()) else {
            // The home is gone, the owners locked the door, or the pilot boarded / died.
            self.home.interior = None;
            self.close_home();
            return false;
        };
        if self.modal == Some(ModalScreen::Home) {
            self.handle_home_modal(input, ctx, cues, requests);
            return false;
        }
        if self.modal.is_some() {
            // The inventory, map or help over the room.
            if input.backs_out() {
                self.modal = None;
            }
            return false;
        }
        if input.pause {
            return true;
        }
        if input.backs_out() {
            self.leave_home(cues);
            return false;
        }
        inside.room = inside.room.min(home.rooms.len().saturating_sub(1));
        let floor = home.rooms.get(inside.room).map_or(0, |room| room.floor);
        let on_floor: Vec<usize> = home.rooms_on(floor).map(|(index, _)| index).collect();
        let place = on_floor
            .iter()
            .position(|index| *index == inside.room)
            .unwrap_or(0);
        let movement = input.horizontal;
        if movement.abs() > f32::EPSILON {
            inside.facing = movement.signum();
        }
        inside.stride = if movement.abs() > 0.2 {
            inside.stride + delta_seconds
        } else {
            0.0
        };
        inside.x = movement.mul_add(185.0 * delta_seconds, inside.x);
        let front_room = floor == 0 && place == 0;
        if inside.x > 628.0 {
            if let Some(next) = on_floor.get(place + 1) {
                inside.room = *next;
                inside.x = 24.0;
            } else {
                inside.x = 628.0;
            }
        } else if inside.x < 12.0 {
            if place > 0 {
                inside.room = on_floor[place - 1];
                inside.x = 616.0;
            } else if front_room && movement < -0.5 {
                self.home.interior = Some(inside);
                self.leave_home(cues);
                return false;
            } else {
                inside.x = 12.0;
            }
        }
        self.home.interior = Some(inside);
        if !input.interact {
            return false;
        }
        let first_room = place == 0;
        let (low, high) = home.floor_span();
        if front_room && (inside.x - HOME_DOOR_X).abs() <= HOME_REACH {
            self.leave_home(cues);
            return false;
        }
        let stairs = [
            (STAIRS_UP_X, floor < high, 1_i8),
            (STAIRS_DOWN_X, floor > low, -1_i8),
        ];
        for (stairs_x, exists, step) in stairs {
            if first_room && exists && (inside.x - stairs_x).abs() <= HOME_REACH {
                let target = floor + step;
                if let Some((index, _)) = home.rooms_on(target).next() {
                    inside.room = index;
                    inside.x = if step > 0 {
                        STAIRS_UP_X - 40.0
                    } else {
                        STAIRS_DOWN_X - 40.0
                    };
                    self.home.interior = Some(inside);
                    cues.push(SoundCue::PilotLand);
                    self.message = if step > 0 {
                        "Up the stairs.".to_owned()
                    } else if target < 0 {
                        "Down to the basement.".to_owned()
                    } else {
                        "Down the stairs.".to_owned()
                    };
                }
                return false;
            }
        }
        let room = &home.rooms[inside.room];
        let plot = inside.plot;
        let room_index = u8::try_from(inside.room).unwrap_or(u8::MAX);
        // The basement's hatch (left) and the lift (right).
        if room.floor < 0 {
            if (inside.x - 120.0).abs() <= HOME_REACH {
                self.send_home(ctx, HomeCommand::Hatch { plot }, requests, cues);
                self.home.interior = None;
                self.close_home();
                self.door_latch = true;
                "Down the hatch into the cellar. Push up under it to climb back in."
                    .clone_into(&mut self.message);
                return false;
            }
            if home.lift && (inside.x - 440.0).abs() <= HOME_REACH {
                self.send_home(
                    ctx,
                    HomeCommand::RideLift { plot, down: true },
                    requests,
                    cues,
                );
                self.home.interior = None;
                self.close_home();
                self.door_latch = true;
                "The lift cage rattles down to your base.".clone_into(&mut self.message);
                return false;
            }
        }
        // The piece in front of the pilot.
        if let Some(column) = cell_column_at(inside.x)
            && let Some((cell, piece)) = room.decor.piece_at(column)
            && let Some(service) = piece.furniture.service()
        {
            match service {
                DecorService::Sleep => {
                    let night = ctx.world.clock().phase().sleepable();
                    self.send_home(
                        ctx,
                        HomeCommand::UsePiece {
                            plot,
                            room: room_index,
                            cell,
                        },
                        requests,
                        cues,
                    );
                    self.message = if night {
                        "You turn in for the night.".to_owned()
                    } else {
                        "Home sweet home: the rescue crew will bring you here. Come back at dusk to sleep.".to_owned()
                    };
                    if night {
                        cues.push(SoundCue::BedDown);
                    } else {
                        cues.push(SoundCue::HomeSet);
                    }
                }
                DecorService::Cook => {
                    self.send_home(
                        ctx,
                        HomeCommand::UsePiece {
                            plot,
                            room: room_index,
                            cell,
                        },
                        requests,
                        cues,
                    );
                    "A hot meal by the stove. You feel patched up and ready."
                        .clone_into(&mut self.message);
                }
                DecorService::Store => self.open_home(HomeScreen::Store, Some(plot)),
                DecorService::Display => self.open_home(HomeScreen::Display(cell), Some(plot)),
                DecorService::Craft => self.open_home(HomeScreen::Craft, Some(plot)),
                DecorService::Stargaze => {
                    self.message = stargaze_line(ctx.world);
                    cues.push(SoundCue::Ui);
                }
            }
            return false;
        }
        // Nothing to use here: the home menu.
        self.open_home(HomeScreen::Menu, Some(plot));
        cues.push(SoundCue::Ui);
        false
    }

    /// Sends a home command for the seat's player when the world accepts it, else says why.
    fn send_home(
        &mut self,
        ctx: SeatContext<'_>,
        command: HomeCommand,
        requests: &mut Vec<ShellRequest>,
        cues: &mut Vec<SoundCue>,
    ) -> bool {
        let Some(player) = ctx.player else {
            return false;
        };
        match ctx.world.check_home(player, &command) {
            Ok(_) => {
                requests.push(ShellRequest::Action {
                    seat: ctx.seat,
                    command: PlayerCommand::Home(command),
                });
                self.home.pending_frames = HOME_PENDING_FRAMES;
                true
            }
            Err(refusal) => {
                refusal.message().clone_into(&mut self.message);
                cues.push(SoundCue::UiError);
                false
            }
        }
    }

    /// The home modal: rebuilds the screen's rows, then moves the selection and confirms.
    pub(super) fn handle_home_modal(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let Some(screen) = self.home.screen else {
            self.modal = None;
            return;
        };
        self.home.view = build_view(self, screen, ctx);
        if input.backs_out() {
            self.home_back();
            cues.push(SoundCue::UiBack);
            return;
        }
        let rows = self.home.view.rows.len();
        self.move_selection(input, rows.saturating_sub(1));
        if input.tab_next || input.tab_previous || input.menu_left || input.menu_right {
            // Left and right flip the supply toggle on the contract screen.
            if matches!(screen, HomeScreen::Contract(_))
                && (input.menu_left || input.menu_right || input.tab_next || input.tab_previous)
            {
                self.home.draft.supply = !self.home.draft.supply;
                cues.push(SoundCue::Ui);
            }
        }
        if !(input.confirm || input.interact) {
            return;
        }
        let Some(row) = self.home.view.rows.get(self.selected_menu_item).cloned() else {
            return;
        };
        if !row.enabled {
            if let Some(refusal) = row.refusal {
                self.message = refusal;
            }
            cues.push(SoundCue::UiError);
            return;
        }
        self.confirm_home_row(screen, &row, ctx, cues, requests);
        // Rebuild at once so the next frame's rows match what was just done.
        if let Some(screen) = self.home.screen {
            self.home.view = build_view(self, screen, ctx);
        }
    }

    #[allow(clippy::too_many_lines, reason = "one arm per row action")]
    fn confirm_home_row(
        &mut self,
        screen: HomeScreen,
        row: &HomeRow,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        match row.action {
            HomeAction::Command(command) => {
                if !self.send_home(ctx, command, requests, cues) {
                    return;
                }
                cues.push(SoundCue::Ui);
                self.message = sent_message(ctx.world, command, row);
                match command {
                    HomeCommand::Place { .. } => {
                        // Back to the room overview, two screens up.
                        self.home
                            .back
                            .retain(|screen| *screen != HomeScreen::DecorPick);
                        self.home_back();
                    }
                    HomeCommand::Display { .. } | HomeCommand::CraftFurniture { .. } => {
                        self.home_back();
                    }
                    _ => {}
                }
                let _ = screen;
            }
            HomeAction::Open(next) => {
                if let HomeScreen::Contract(_) = next {
                    self.home.draft = HireDraft::default();
                }
                self.open_home(next, None);
                cues.push(SoundCue::Ui);
            }
            HomeAction::Contractor(index) => {
                self.home.draft.contractor = index;
                cues.push(SoundCue::Ui);
            }
            HomeAction::ToggleSupply => {
                self.home.draft.supply = !self.home.draft.supply;
                cues.push(SoundCue::Ui);
            }
            HomeAction::Sign => {
                let (Some(plot), HomeScreen::Contract(job), Some(player)) =
                    (self.home.plot, screen, ctx.player)
                else {
                    return;
                };
                let offers = ctx.world.contractors_for(plot, player);
                let Some(offer) = offers.get(self.home.draft.contractor) else {
                    return;
                };
                let command = HomeCommand::Hire {
                    plot,
                    job,
                    contractor: offer.character,
                    supply: self.home.draft.supply,
                };
                if self.send_home(ctx, command, requests, cues) {
                    self.message = format!(
                        "Contract signed: {} starts on the {} at once.",
                        offer.name,
                        job.name().to_lowercase()
                    );
                    // Back to the site board to watch it go up.
                    self.home.back.clear();
                    self.home.screen = Some(HomeScreen::Site);
                    self.selected_menu_item = 0;
                }
            }
            HomeAction::GoInside => {
                let Some(plot) = self.home.plot else {
                    return;
                };
                self.enter_home(ctx, plot, 0, HOME_DOOR_X + 46.0, cues);
            }
            HomeAction::Leave => self.leave_home(cues),
            HomeAction::Info => {
                if !row.detail.is_empty() {
                    self.message.clone_from(&row.detail);
                }
            }
        }
    }
}

/// What a telescope shows (a line about the Moon that hangs still).
fn stargaze_line(world: &WorldState) -> String {
    let clock = world.clock();
    if clock.is_night() {
        format!(
            "Through the brass: the Moon, {} tonight, hanging exactly where it always hangs. Nobody knows why.",
            clock.moon_phase().name().to_lowercase()
        )
    } else {
        "Too bright for stars. The Moon is a pale coin over the same old column of sky.".to_owned()
    }
}

/// The toast after a home command is sent.
fn sent_message(world: &WorldState, command: HomeCommand, row: &HomeRow) -> String {
    match command {
        HomeCommand::BuyPlot { .. } => format!(
            "Deed signed: {}. Head there and use the site board to hire a builder.",
            row.label
        ),
        HomeCommand::Rush { .. } => "Coins change hands: the crew works double shifts.".to_owned(),
        HomeCommand::AnswerEvent { .. } => "The contractor nods: back to work.".to_owned(),
        HomeCommand::SetAccess { access, .. } => format!("Visitors: {}.", access.name()),
        HomeCommand::CoOwner { add: true, .. } => "Deed amended: a new co-owner.".to_owned(),
        HomeCommand::CoOwner { add: false, .. } => "Deed amended: co-owner removed.".to_owned(),
        HomeCommand::BuyFurniture { furniture } => format!(
            "{} bought: it waits in your stock to be placed at home.",
            furniture.name()
        ),
        HomeCommand::CraftFurniture { furniture, .. } => {
            format!("You knock together a {}.", furniture.name().to_lowercase())
        }
        HomeCommand::Place { furniture, .. } => format!("{} placed.", furniture.name()),
        HomeCommand::PickUp { .. } => "Picked up: back in your stock.".to_owned(),
        HomeCommand::Restyle {
            wallpaper: true, ..
        } => "Fresh wallpaper.".to_owned(),
        HomeCommand::Restyle {
            wallpaper: false, ..
        } => "A new floor laid.".to_owned(),
        HomeCommand::Display { item, .. } => {
            format!("{} goes on display.", item_name(item))
        }
        HomeCommand::Stash { deposit: true, .. } => "Cargo stowed in the house.".to_owned(),
        HomeCommand::Stash { deposit: false, .. } => "Loaded the rig from storage.".to_owned(),
        HomeCommand::Hire { .. }
        | HomeCommand::UsePiece { .. }
        | HomeCommand::RideLift { .. }
        | HomeCommand::Hatch { .. } => {
            let _ = world;
            String::new()
        }
    }
}

/// Rebuilds `screen`'s rows from the world.
#[allow(clippy::too_many_lines, reason = "one screen per arm")]
fn build_view(seat: &SeatUi, screen: HomeScreen, ctx: SeatContext<'_>) -> HomeView {
    let world = ctx.world;
    let Some(player) = ctx.player else {
        return HomeView::default();
    };
    let check = |command: HomeCommand| world.check_home(player, &command).err();
    let plot = seat.home.plot.and_then(|id| world.homes().plot(id));
    let now = world.world_millis();
    let day = world.day_length().millis();
    let credits = world.player(player).map_or(0, |rig| rig.credits);
    let mut view = HomeView::default();
    match screen {
        HomeScreen::LandOffice => {
            let sky_office = world.at_sky_land_office(player).is_some();
            if sky_office {
                "Sky Land Office".clone_into(&mut view.title);
                "Deeds to plots on the isles, and furniture for the wind"
                    .clone_into(&mut view.subtitle);
            } else {
                "Land Office".clone_into(&mut view.title);
                "Deeds to building plots, and fine furniture".clone_into(&mut view.subtitle);
            }
            view.status.push(HomeStatus {
                label: "Your deeds".to_owned(),
                value: format!(
                    "{} of {}",
                    world.homes().deeds_held(player),
                    fatherload_simulation::home::MAX_PLOTS
                ),
                meter: None,
            });
            "Plots for sale".clone_into(&mut view.rows_title);
            let column = world.actor_tile(player).map_or(0, |tile| tile.x);
            let mut sites = world.land_office_listing(player, column, OFFICE_REACH_COLUMNS);
            sites.sort_by_key(|site| ((site.middle() - column).abs(), site.left));
            for site in sites.into_iter().take(8) {
                let sold = world.homes().plot_of_site(site.key);
                let distance = site.middle() - column;
                let way = if distance < 0 { "west" } else { "east" };
                let label = format!(
                    "{}, {}",
                    site.size.name(),
                    site.setting.name().to_lowercase()
                );
                let detail = format!("{} tiles {way}, {} wide", distance.abs(), site.width());
                let mut row = HomeRow::new(
                    label,
                    format!("{} cr", site.price),
                    HomeAction::Command(HomeCommand::BuyPlot {
                        site: site.key,
                        size: site.size,
                    }),
                )
                .detail(detail);
                if let Some(owned) = sold {
                    row.value = if owned.deed.owner == player {
                        "YOURS".to_owned()
                    } else {
                        "SOLD".to_owned()
                    };
                    row.enabled = false;
                    row.refusal = Some(HomeRefusal::SiteTaken.message().to_owned());
                } else {
                    row = row.refused(check(HomeCommand::BuyPlot {
                        site: site.key,
                        size: site.size,
                    }));
                }
                view.rows.push(row);
            }
            view.rows.push(
                HomeRow::new(
                    "Furniture catalogue",
                    format!("{} pieces", world.land_office_catalogue(player).len()),
                    HomeAction::Open(HomeScreen::Catalogue),
                )
                .detail(if sky_office {
                    "Hammocks, barometers, and everything the ground offices sell"
                } else {
                    "Beds, lamps, rugs, trophy cases and more"
                }),
            );
            view.quote = Some(if sky_office {
                "Isle plots are small and sold as they float: no cellars, nothing below but sky."
                    .to_owned()
            } else {
                "Any free spot outside town is yours too: stand on it and survey it from your inventory."
                    .to_owned()
            });
        }
        HomeScreen::Catalogue => {
            "Furniture Catalogue".clone_into(&mut view.title);
            format!("{credits} credits to spend").clone_into(&mut view.subtitle);
            "Delivered to your stock".clone_into(&mut view.rows_title);
            for furniture in world.land_office_catalogue(player) {
                let owned = world.homes().furniture_count(player, furniture);
                view.rows.push(
                    HomeRow::new(
                        furniture.name(),
                        format!("{} cr", furniture.catalogue_price()),
                        HomeAction::Command(HomeCommand::BuyFurniture { furniture }),
                    )
                    .detail(if owned > 0 {
                        format!("{owned} in stock")
                    } else {
                        furniture_blurb(furniture).to_owned()
                    })
                    .refused(check(HomeCommand::BuyFurniture { furniture })),
                );
            }
        }
        HomeScreen::Survey => {
            "Survey This Spot".clone_into(&mut view.title);
            "Stake a plot of open ground where you stand".clone_into(&mut view.subtitle);
            "Plot size".clone_into(&mut view.rows_title);
            for size in PlotSize::ALL {
                let survey = world.survey_plot(player, size);
                let command = HomeCommand::BuyPlot {
                    site: SiteKey::Surveyed,
                    size,
                };
                let row = match survey {
                    Ok(site) => HomeRow::new(
                        format!("{}, {}", size.name(), site.setting.name().to_lowercase()),
                        format!("{} cr", site.price),
                        HomeAction::Command(command),
                    )
                    .detail(format!("{} tiles wide", size.width()))
                    .refused(check(command)),
                    Err(refusal) => HomeRow::new(size.name(), "-", HomeAction::Command(command))
                        .detail(format!("{} tiles wide", size.width()))
                        .refused(Some(refusal)),
                };
                view.rows.push(row);
            }
            view.quote = Some(
                "Far from town is cheaper; levelling rough ground costs extra. Sky islands and the Moon can be surveyed too."
                    .to_owned(),
            );
        }
        HomeScreen::Site => {
            let Some(plot) = plot else {
                return view;
            };
            build_site(&mut view, plot, player, world, now, day, check);
        }
        HomeScreen::Contract(job) => {
            let Some(plot) = plot else {
                return view;
            };
            format!("Hire a builder: {}", job.name()).clone_into(&mut view.title);
            plot_name(plot).clone_into(&mut view.subtitle);
            let offers = world.contractors_for(plot.id, player);
            let chosen = seat
                .home
                .draft
                .contractor
                .min(offers.len().saturating_sub(1));
            let blocks = world.player(player).map_or(0, |rig| {
                rig.block_hopper
                    .blocks()
                    .map(|(_, count)| count)
                    .sum::<u32>()
            });
            "The contract".clone_into(&mut view.rows_title);
            let supply_row = HomeRow::new(
                "Supply blocks from the hopper",
                if seat.home.draft.supply { "ON" } else { "OFF" },
                HomeAction::ToggleSupply,
            )
            .detail(format!(
                "{blocks} blocks aboard; the job uses {}",
                job.blocks()
            ))
            .chosen(seat.home.draft.supply);
            if let Some(offer) = offers.get(chosen) {
                let quote = world.hire_quote(
                    player,
                    plot.id,
                    job,
                    offer.character,
                    seat.home.draft.supply,
                );
                let mut sign = HomeRow::new("Sign the contract", "", HomeAction::Sign);
                match quote {
                    Ok((hire, _)) => {
                        sign.value = format!("{} cr", hire.price);
                        let mut lines = vec![format!(
                            "{} for {}",
                            offer.name,
                            days_text(hire.millis, day)
                        )];
                        if hire.supply_discount > 0 {
                            lines.push(format!(
                                "{} blocks supplied: -{} cr",
                                hire.supplied_blocks, hire.supply_discount
                            ));
                        }
                        if hire.repeat_discount > 0 {
                            lines.push(format!("returning client: -{} cr", hire.repeat_discount));
                        }
                        sign.detail = lines.join(", ");
                        if credits < hire.price {
                            sign = sign.refused(Some(HomeRefusal::CannotAfford));
                        }
                    }
                    Err(refusal) => sign = sign.refused(Some(refusal)),
                }
                view.rows.push(sign);
                if let Some(character) = offer.character
                    && let Some(person) = fatherload_simulation::people::routine::character(
                        world.world_settings(),
                        character,
                    )
                {
                    view.quote = Some(format!(
                        "{}: \"{}\"",
                        person.name.short(),
                        offer.profile.pitch(&person)
                    ));
                }
            }
            view.rows.push(supply_row);
            for (index, offer) in offers.iter().enumerate() {
                let quote = world.hire_quote(player, plot.id, job, offer.character, false);
                let (value, detail) = match &quote {
                    Ok((hire, _)) => (
                        format!("{} cr", hire.price),
                        format!(
                            "{} - {}, {}, quality {}{}",
                            offer.from,
                            offer.profile.style.name(),
                            days_text(hire.millis, day),
                            hire.quote.quality,
                            if offer.past_jobs > 0 {
                                " - remembers you"
                            } else {
                                ""
                            }
                        ),
                    ),
                    Err(refusal) => ("-".to_owned(), refusal.message().to_owned()),
                };
                view.rows.push(
                    HomeRow::new(offer.name.clone(), value, HomeAction::Contractor(index))
                        .detail(detail)
                        .chosen(index == chosen),
                );
            }
        }
        HomeScreen::Deed => {
            let Some(plot) = plot else {
                return view;
            };
            "The Deed".clone_into(&mut view.title);
            plot_name(plot).clone_into(&mut view.subtitle);
            view.status.push(HomeStatus {
                label: "Owner".to_owned(),
                value: if plot.deed.owner == player {
                    "You".to_owned()
                } else {
                    format!("Player {}", plot.deed.owner.get() + 1)
                },
                meter: None,
            });
            "Who may come in".clone_into(&mut view.rows_title);
            let next = plot.deed.access.next();
            view.rows.push(
                HomeRow::new(
                    "Visitors",
                    plot.deed.access.name(),
                    HomeAction::Command(HomeCommand::SetAccess {
                        plot: plot.id,
                        access: next,
                    }),
                )
                .detail(format!("Change to: {}", next.name()))
                .refused(check(HomeCommand::SetAccess {
                    plot: plot.id,
                    access: next,
                })),
            );
            for other in world.player_ids().filter(|other| *other != plot.deed.owner) {
                let listed = plot.deed.co_owners.contains(&other);
                let command = HomeCommand::CoOwner {
                    plot: plot.id,
                    player: other,
                    add: !listed,
                };
                view.rows.push(
                    HomeRow::new(
                        format!("Player {} co-owns", other.get() + 1),
                        if listed { "YES" } else { "NO" },
                        HomeAction::Command(command),
                    )
                    .detail("Co-owners decorate, hire and sleep here")
                    .refused(check(command)),
                );
            }
        }
        HomeScreen::Menu => {
            let Some(plot) = plot else {
                return view;
            };
            let Some(home) = plot.home.as_ref() else {
                return view;
            };
            format!("{} - {}", home.blueprint.name(), plot.site.setting.name())
                .clone_into(&mut view.title);
            let room = seat
                .home
                .interior
                .and_then(|inside| home.rooms.get(inside.room))
                .map_or("", |room| room.kind.name());
            room.clone_into(&mut view.subtitle);
            view.status.push(HomeStatus {
                label: "Quality".to_owned(),
                value: "*".repeat(usize::from(home.quality)),
                meter: None,
            });
            view.status.push(HomeStatus {
                label: "Storage".to_owned(),
                value: format!(
                    "{} / {}",
                    home.storage.slots_used(),
                    home.storage_capacity()
                ),
                meter: None,
            });
            "At home".clone_into(&mut view.rows_title);
            view.rows.push(
                HomeRow::new(
                    "Decorate this room",
                    "",
                    HomeAction::Open(HomeScreen::Decor),
                )
                .detail("Place furniture, lamps and rugs; new wallpaper and floors")
                .refused((!plot.deed.may_decorate(player)).then_some(HomeRefusal::NotPermitted)),
            );
            view.rows.push(
                HomeRow::new("Storage", "", HomeAction::Open(HomeScreen::Store))
                    .detail("Stow cargo in the chests, or load it back into the rig"),
            );
            if plot.deed.owner == player {
                view.rows.push(
                    HomeRow::new("The deed", "", HomeAction::Open(HomeScreen::Deed))
                        .detail("Visitors and co-owners"),
                );
            }
            view.rows
                .push(HomeRow::new("Leave the house", "", HomeAction::Leave));
        }
        HomeScreen::Decor => {
            let Some((home, room_index)) = current_room(seat, world) else {
                return view;
            };
            let room = &home.rooms[room_index];
            format!("Decorate: {}", room.kind.name()).clone_into(&mut view.title);
            let stock: u32 = world.homes().furniture_of(player).values().sum();
            format!("{stock} pieces in your stock").clone_into(&mut view.subtitle);
            let plot_id = seat.home.plot.unwrap_or_default();
            let room_u8 = u8::try_from(room_index).unwrap_or(u8::MAX);
            "This room".clone_into(&mut view.rows_title);
            view.rows.push(
                HomeRow::new("Place a piece", "", HomeAction::Open(HomeScreen::DecorPick))
                    .detail("From your stock: bought, crafted, gifts and finds"),
            );
            for restyle in [true, false] {
                let command = HomeCommand::Restyle {
                    plot: plot_id,
                    room: room_u8,
                    wallpaper: restyle,
                };
                let (label, current, next) = if restyle {
                    (
                        "Wallpaper",
                        room.decor.wallpaper.name(),
                        room.decor.wallpaper.next().name(),
                    )
                } else {
                    (
                        "Floor",
                        room.decor.flooring.name(),
                        room.decor.flooring.next().name(),
                    )
                };
                view.rows.push(
                    HomeRow::new(
                        format!("{label}: {current}"),
                        format!("{} cr", fatherload_simulation::home::decor::RESTYLE_PRICE),
                        HomeAction::Command(command),
                    )
                    .detail(format!("Change to {next}"))
                    .refused(check(command)),
                );
            }
            for (cell, piece) in room.decor.all() {
                let command = HomeCommand::PickUp {
                    plot: plot_id,
                    room: room_u8,
                    cell,
                };
                let shows = piece
                    .display
                    .map_or_default(|item| format!(" (showing {})", item_name(item)));
                view.rows.push(
                    HomeRow::new(
                        format!("Pick up the {}", piece.furniture.name().to_lowercase()),
                        "",
                        HomeAction::Command(command),
                    )
                    .detail(format!("{}{shows}", spot_name(cell)))
                    .refused(check(command)),
                );
            }
        }
        HomeScreen::DecorPick => {
            let Some((home, room_index)) = current_room(seat, world) else {
                return view;
            };
            let room = &home.rooms[room_index];
            "Your Stock".clone_into(&mut view.title);
            format!("for the {}", room.kind.name().to_lowercase()).clone_into(&mut view.subtitle);
            "Pieces".clone_into(&mut view.rows_title);
            for (furniture, count) in world.homes().furniture_of(player) {
                let fits =
                    (0..ROOM_COLUMNS * ROOM_ROWS).any(|cell| room.decor.fits(furniture, cell));
                view.rows.push(
                    HomeRow::new(
                        furniture.name(),
                        format!("x{count}"),
                        HomeAction::Open(HomeScreen::DecorSpot(furniture)),
                    )
                    .detail(furniture_blurb(furniture))
                    .refused((!fits).then_some(HomeRefusal::NoSpace)),
                );
            }
            if view.rows.is_empty() {
                view.rows.push(
                    HomeRow::new("Nothing in stock", "", HomeAction::Info)
                        .detail("Buy furniture at a land office, or craft it at a workbench."),
                );
            }
        }
        HomeScreen::DecorSpot(furniture) => {
            let Some((home, room_index)) = current_room(seat, world) else {
                return view;
            };
            let room = &home.rooms[room_index];
            format!("Place the {}", furniture.name().to_lowercase()).clone_into(&mut view.title);
            room.kind.name().clone_into(&mut view.subtitle);
            "Where".clone_into(&mut view.rows_title);
            let plot_id = seat.home.plot.unwrap_or_default();
            for cell in 0..ROOM_COLUMNS * ROOM_ROWS {
                if !room.decor.fits(furniture, cell) {
                    continue;
                }
                let command = HomeCommand::Place {
                    plot: plot_id,
                    room: u8::try_from(room_index).unwrap_or(u8::MAX),
                    cell,
                    furniture,
                };
                view.rows.push(
                    HomeRow::new(spot_name(cell), "", HomeAction::Command(command))
                        .refused(check(command)),
                );
            }
        }
        HomeScreen::Store => {
            let Some(plot) = plot else {
                return view;
            };
            let Some(home) = plot.home.as_ref() else {
                return view;
            };
            "Storage".clone_into(&mut view.title);
            format!(
                "{} of {} slots used",
                home.storage.slots_used(),
                home.storage_capacity()
            )
            .clone_into(&mut view.subtitle);
            for (item, count) in home.storage.iter().take(6) {
                view.status.push(HomeStatus {
                    label: item_name(item),
                    value: format!("x{count}"),
                    meter: None,
                });
            }
            "Chests".clone_into(&mut view.rows_title);
            for deposit in [true, false] {
                let command = HomeCommand::Stash {
                    plot: plot.id,
                    deposit,
                };
                view.rows.push(
                    HomeRow::new(
                        if deposit {
                            "Stow the rig's cargo"
                        } else {
                            "Load everything into the rig"
                        },
                        "",
                        HomeAction::Command(command),
                    )
                    .detail(if deposit {
                        "Ore, relics and materials"
                    } else {
                        "As much as the bay holds"
                    })
                    .refused(check(command)),
                );
            }
        }
        HomeScreen::Display(cell) => {
            let Some((_, room_index)) = current_room(seat, world) else {
                return view;
            };
            "Trophy Case".clone_into(&mut view.title);
            "Show off a relic or a rare gem from the bay".clone_into(&mut view.subtitle);
            "From the cargo bay".clone_into(&mut view.rows_title);
            let plot_id = seat.home.plot.unwrap_or_default();
            if let Some(rig) = world.player(player) {
                for (item, count) in rig.cargo_bay.iter().filter(|(item, _)| displayable(*item)) {
                    let command = HomeCommand::Display {
                        plot: plot_id,
                        room: u8::try_from(room_index).unwrap_or(u8::MAX),
                        cell,
                        item,
                    };
                    view.rows.push(
                        HomeRow::new(
                            item_name(item),
                            format!("x{count}"),
                            HomeAction::Command(command),
                        )
                        .refused(check(command)),
                    );
                }
            }
            if view.rows.is_empty() {
                view.rows.push(
                    HomeRow::new("Nothing worth showing aboard", "", HomeAction::Info)
                        .detail("Bring an artifact or a rare gem (emerald and up) home."),
                );
            }
        }
        HomeScreen::Craft => {
            "Workbench".clone_into(&mut view.title);
            let blocks: u32 = world.player(player).map_or(0, |rig| {
                rig.block_hopper.blocks().map(|(_, count)| count).sum()
            });
            format!("{blocks} blocks in the hopper").clone_into(&mut view.subtitle);
            "Make".clone_into(&mut view.rows_title);
            let plot_id = seat.home.plot.unwrap_or_default();
            for furniture in Furniture::CRAFTABLE {
                let Some((blocks, price)) = furniture.craft_cost() else {
                    continue;
                };
                let command = HomeCommand::CraftFurniture {
                    plot: plot_id,
                    furniture,
                };
                view.rows.push(
                    HomeRow::new(
                        furniture.name(),
                        format!("{blocks} blocks + {price} cr"),
                        HomeAction::Command(command),
                    )
                    .refused(check(command)),
                );
            }
        }
    }
    view
}

/// The home and room index the seat stands in.
fn current_room(
    seat: &SeatUi,
    world: &WorldState,
) -> Option<(fatherload_simulation::home::Home, usize)> {
    let inside = seat.home.interior?;
    let home = world.homes().plot(inside.plot)?.home.clone()?;
    (inside.room < home.rooms.len()).then_some((home, inside.room))
}

/// The site board's status and rows.
#[allow(
    clippy::too_many_lines,
    reason = "the board's three states read best together"
)]
fn build_site(
    view: &mut HomeView,
    plot: &Plot,
    player: PlayerId,
    world: &WorldState,
    now: u64,
    day: u64,
    check: impl Fn(HomeCommand) -> Option<HomeRefusal>,
) {
    let owns = plot.deed.owns(player);
    view.title = match (&plot.construction, &plot.home) {
        (Some(work), _) => format!("{} going up", work.job.name()),
        (None, Some(home)) => home.blueprint.name().to_owned(),
        (None, None) => "Building Plot".to_owned(),
    };
    view.subtitle = plot_name(plot);
    view.status.push(HomeStatus {
        label: "Deed".to_owned(),
        value: if plot.deed.owner == player {
            "Yours".to_owned()
        } else if owns {
            "Co-owned".to_owned()
        } else {
            format!("Player {}'s", plot.deed.owner.get() + 1)
        },
        meter: None,
    });
    if let Some(work) = &plot.construction {
        let stage = work.stage(now);
        let progress = work.progress_per_mille(now);
        #[allow(clippy::cast_precision_loss, reason = "per mille")]
        view.status.push(HomeStatus {
            label: work.job.name(),
            value: format!("{} - {}%", stage.name(), progress / 10),
            meter: Some(progress as f32 / 1_000.0),
        });
        view.status.push(HomeStatus {
            label: "Finished".to_owned(),
            value: if work.hold_since.is_some() {
                "On hold".to_owned()
            } else {
                format!("in {}", days_text(work.remaining_millis(now), day))
            },
            meter: None,
        });
        let name = if work.guild {
            "The guild foreman".to_owned()
        } else {
            fatherload_simulation::people::routine::character(
                world.world_settings(),
                work.character(),
            )
            .map_or_else(
                || "The contractor".to_owned(),
                |person| person.name.short().to_owned(),
            )
        };
        view.quote = Some(format!("{name}: \"{}\"", stage.line()));
        "The works".clone_into(&mut view.rows_title);
        if let Some((_, event)) = work.open_event() {
            view.quote = Some(format!(
                "{name}: \"{}\"",
                event.kind.line(work.contractor.style)
            ));
            match event.kind {
                SiteEventKind::Shortage { block, count } => {
                    let bring = HomeCommand::AnswerEvent {
                        plot: plot.id,
                        pay: false,
                    };
                    let pay = HomeCommand::AnswerEvent {
                        plot: plot.id,
                        pay: true,
                    };
                    view.rows.push(
                        HomeRow::new(
                            format!("Bring {count} {}s", block.name().to_lowercase()),
                            "",
                            HomeAction::Command(bring),
                        )
                        .detail("From the hopper")
                        .refused(check(bring)),
                    );
                    view.rows.push(
                        HomeRow::new(
                            "Pay for the stock",
                            format!("{} cr", count * 12),
                            HomeAction::Command(pay),
                        )
                        .refused(check(pay)),
                    );
                }
                SiteEventKind::Request { item } => {
                    let give = HomeCommand::AnswerEvent {
                        plot: plot.id,
                        pay: false,
                    };
                    let decline = HomeCommand::AnswerEvent {
                        plot: plot.id,
                        pay: true,
                    };
                    view.rows.push(
                        HomeRow::new(
                            format!("Hand over a piece of {}", item_name(item).to_lowercase()),
                            "+1 quality",
                            HomeAction::Command(give),
                        )
                        .detail("From the cargo bay")
                        .refused(check(give)),
                    );
                    view.rows.push(
                        HomeRow::new("Not this time", "", HomeAction::Command(decline))
                            .refused(check(decline)),
                    );
                }
                SiteEventKind::Anecdote { .. } => {
                    let nod = HomeCommand::AnswerEvent {
                        plot: plot.id,
                        pay: false,
                    };
                    view.rows.push(
                        HomeRow::new("Nod along", "", HomeAction::Command(nod)).refused(check(nod)),
                    );
                }
            }
        }
        let rush = HomeCommand::Rush { plot: plot.id };
        let rush_price = work.rush_quote(now).map(|(price, _)| price);
        view.rows.push(
            HomeRow::new(
                "Pay to rush the work",
                rush_price.map_or_else(|| "-".to_owned(), |price| format!("{price} cr")),
                HomeAction::Command(rush),
            )
            .detail("Double shifts: half the work left is done at once")
            .refused(check(rush)),
        );
    } else if let Some(home) = &plot.home {
        view.status.push(HomeStatus {
            label: "Quality".to_owned(),
            value: "*".repeat(usize::from(home.quality)),
            meter: None,
        });
        view.status.push(HomeStatus {
            label: "Rooms".to_owned(),
            value: format!("{} on {} floor(s)", home.rooms.len(), home.floors),
            meter: None,
        });
        "Your home".clone_into(&mut view.rows_title);
        let can_enter = world.may_enter_home(player, plot.id);
        view.rows.push(
            HomeRow::new("Go inside", "", HomeAction::GoInside)
                .detail("On foot: push up at the front door")
                .refused(if !can_enter {
                    Some(HomeRefusal::NotPermitted)
                } else if !world.is_on_foot(player) {
                    Some(HomeRefusal::NeedsPilot)
                } else {
                    None
                }),
        );
        if owns {
            for expansion in Expansion::OFFERED {
                let job = HomeJob::Expand(expansion);
                let refusal = world
                    .hire_quote(
                        player,
                        plot.id,
                        job,
                        world
                            .contractors_for(plot.id, player)
                            .first()
                            .and_then(|offer| offer.character),
                        false,
                    )
                    .err();
                let price = job.job(plot.site.size).price;
                view.rows.push(
                    HomeRow::new(
                        expansion.name(),
                        format!("~{price} cr"),
                        HomeAction::Open(HomeScreen::Contract(job)),
                    )
                    .detail(expansion_blurb(expansion))
                    .refused(refusal.filter(|refusal| *refusal != HomeRefusal::CannotAfford)),
                );
            }
        }
        if home.has_garage() {
            view.rows.push(
                HomeRow::new("Garage bay", "", HomeAction::Info).detail(
                    "Park the rig in the bay (left of the house) to patch its hull for free.",
                ),
            );
        }
    } else {
        "Blueprints".clone_into(&mut view.rows_title);
        view.quote =
            Some("Bare ground and a good view. Choose a plan and hire a builder.".to_owned());
        if owns {
            for blueprint in Blueprint::ALL {
                let job = HomeJob::Build(blueprint);
                let plan = job.job(plot.site.size);
                let refusal = if blueprint.min_plot() > plot.site.size {
                    Some(HomeRefusal::PlotTooSmall)
                } else if !blueprint.allowed_on(plot.site.setting) {
                    Some(HomeRefusal::WrongSetting)
                } else {
                    None
                };
                let (width, height) = blueprint.footprint();
                view.rows.push(
                    HomeRow::new(
                        blueprint.name(),
                        format!("~{} cr", plan.price),
                        HomeAction::Open(HomeScreen::Contract(job)),
                    )
                    .detail(format!(
                        "{} rooms, {width}x{height} tiles, about {} - {}",
                        blueprint.rooms().len(),
                        days_text(u64::from(plan.tenth_days) * day / 10, day),
                        blueprint.blurb()
                    ))
                    .refused(refusal),
                );
            }
        } else {
            view.rows
                .push(HomeRow::new("Someone else's plot", "", HomeAction::Info));
        }
    }
    if plot.deed.owner == player {
        view.rows.push(
            HomeRow::new(
                "The deed",
                plot.deed.access.name(),
                HomeAction::Open(HomeScreen::Deed),
            )
            .detail("Visitors and co-owners"),
        );
    }
    let _ = BuildStage::Done;
    let _ = HomeAccess::Crew;
}

const fn furniture_blurb(furniture: Furniture) -> &'static str {
    match furniture.service() {
        Some(DecorService::Sleep) => "Sleep the night away; makes this your home point",
        Some(DecorService::Cook) => "A hot meal patches the pilot up",
        Some(DecorService::Store) => "Holds cargo",
        Some(DecorService::Display) => "Shows off a relic or a rare gem",
        Some(DecorService::Craft) => "Make furniture from blocks",
        Some(DecorService::Stargaze) => "Watch the Moon that never moves",
        None if furniture.lights() => "Lights the room (windows glow at night)",
        None => match furniture.row() {
            DecorRow::Floor => "Stands on the floor",
            DecorRow::Wall => "Hangs on the wall",
            DecorRow::Ceiling => "Hangs from the ceiling",
        },
    }
}

const fn expansion_blurb(expansion: Expansion) -> &'static str {
    match expansion {
        Expansion::Room(_) => "One more room on the ground floor",
        Expansion::Floor => "A new storey: a bedroom and a study",
        Expansion::Garage => "A bay to park the rig in (repairs at cost, 1 cr a point)",
        Expansion::Basement => "A cellar under the house, with a hatch down into the mine",
        Expansion::Lift => "A private lift from the basement to your claim below",
    }
}
