#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "rendering APIs use integer pixels while camera math uses floats"
)]

//! Draws a [`RenderSnapshot`] plus [`ShellState`]. The renderer never reads the simulation.

use std::{cell::RefCell, collections::BTreeMap};

use raylib::{ffi, prelude::*};

use drillgame_simulation::{
    DiscoveredTiles,
    terrain::{Terrain, TilePosition},
};

mod counters;
mod gallery;
mod interior;
mod layout;
mod menus;
pub mod pixel_art;
mod prompts;
mod screen;
pub mod terrain;
mod title;
mod ui_kit;
mod ui_sheet;
mod world;

use interior::{RoomArea, draw_interior};
use menus::{draw_online, draw_pause, draw_rescue};
use pixel_art::{font::load_pixel_font, palette, sprite};
use screen::{
    UiFrame, draw_depth_ruler_for_view, draw_heat_warning_for_view, draw_minimap_for_view,
    draw_modal, draw_toast, draw_view_hud, toast_message,
};
use terrain::TerrainRenderer;
use ui_kit::{TextStyle, UiCanvas, UiCanvasScope, UiFontFaces};
use world::{RigFacings, SkyArt, WorldScene, draw_world_scene, world_camera};

use crate::{
    input::PlayerInput,
    local_game::FrameReport,
    render_snapshot::{RenderSnapshot, ViewSnapshot},
    shell::{RunMode, ShellState},
};

/// Radius (tiles) around a newly explored tile whose darkness gradient changes.
const EXPLORATION_REDRAW_RADIUS_TILES: i32 = 12;

/// Physical framebuffer pixels per logical screen point (2.0 on Retina, 1.0 otherwise).
///
/// Outside a camera raylib scales logical points to the framebuffer itself, but `BeginMode2D`
/// replaces that transform, so a `Camera2D`'s offset and zoom are in physical pixels and must be
/// multiplied by this factor.
fn render_scale_for_draw(draw: &RaylibDrawHandle<'_>) -> f32 {
    pixel_density(
        (draw.get_render_width(), draw.get_render_height()),
        (draw.get_screen_width(), draw.get_screen_height()),
    )
}

fn pixel_density(render: (i32, i32), screen: (i32, i32)) -> f32 {
    let scale_x = render.0 as f32 / screen.0.max(1) as f32;
    let scale_y = render.1 as f32 / screen.1.max(1) as f32;
    scale_x.min(scale_y).max(1.0)
}

/// The physical-pixel `Camera2D` that draws world pixel `camera` (top-left) at the viewport's
/// top-left corner, `world_scale` screen points per world pixel on a `density` display.
fn view_camera_2d(
    viewport: crate::render_snapshot::Viewport,
    camera: Vector2,
    world_scale: f32,
    density: f32,
) -> Camera2D {
    let mut view_camera = world_camera(camera, world_scale * density);
    view_camera.offset = Vector2::new(viewport.x as f32 * density, viewport.y as f32 * density);
    view_camera
}

pub struct GameRenderer {
    terrain: TerrainRenderer,
    /// Sky gradient, hills and clouds, rasterized at startup.
    sky: SkyArt,
    /// Which way each rig last faced.
    rig_facings: RigFacings,
    /// Every sprite, rasterized at startup (see `pixel_art::sprite`).
    atlas: Option<Texture2D>,
    font_regular: Option<Font>,
    font_bold: Option<Font>,
    /// Dialog focus and scroll, kept apart for the global screens and for each seat.
    ui_states: RefCell<BTreeMap<UiOwner, layout::widgets::UiState>>,
    /// Discovered tiles already drawn by the terrain renderer (primary view).
    explored_drawn: DiscoveredTiles,
}

/// Whose dialogs a [`layout::widgets::UiState`] tracks: the global screens (title, pause,
/// options, slots, online) or one local seat's counter panels and personal dialogs.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum UiOwner {
    Global,
    Seat(usize),
}

/// Wheel (and right-stick) scrolling in art pixels: positive scrolls down. Keyboard navigation
/// moves the selection instead, and the dialog follows it (`layout::follow_focused_row`) only
/// when the selected row would leave the visible area or is the first or last row.
fn ui_scroll_delta(input: PlayerInput, step: f32) -> f32 {
    -input.ui_scroll * step
}

/// Applies one frame of explicit dialog scrolling to `state`: the wheel / right stick, Page
/// Up/Down (unless `page_keys_tune`: the F3 overlay owns them then) and Home/End. Does nothing
/// while no dialog is open. Never touches focus, so follow logic leaves the result alone until
/// the selection moves.
fn apply_modal_scroll(
    state: &mut layout::widgets::UiState,
    input: PlayerInput,
    page_keys_tune: bool,
) {
    const MODAL_SCROLL_STEP: f32 = 48.0;
    const MODAL_SCROLL_LIMIT: f32 = 10_000.0;
    if !state.has_modal() {
        return;
    }
    let id = || layout::widgets::WidgetId::new("modal-content");
    let scroll_delta = ui_scroll_delta(input, MODAL_SCROLL_STEP);
    if scroll_delta.abs() > f32::EPSILON {
        state.scroll_by(id(), scroll_delta, MODAL_SCROLL_LIMIT);
    }
    if !page_keys_tune {
        if input.page_up {
            state.scroll_pages(id(), -1.0);
        }
        if input.page_down {
            state.scroll_pages(id(), 1.0);
        }
    }
    if input.scroll_home {
        state.scroll_to_edge(id(), false);
    }
    if input.scroll_end {
        state.scroll_to_edge(id(), true);
    }
}

/// Routes one frame of menu input to the dialog states it belongs to (see
/// [`GameRenderer::apply_ui_input`]).
fn route_ui_input(
    states: &mut BTreeMap<UiOwner, layout::widgets::UiState>,
    inputs: &[PlayerInput],
    shell: &ShellState,
) {
    let page_keys_tune = shell.tuning_keys_active();
    let mut apply = |owner: UiOwner, input: PlayerInput| {
        let state = states.entry(owner).or_default();
        apply_modal_scroll(state, input, page_keys_tune);
        if input.menu_up {
            state.focus_previous();
        } else if input.menu_down {
            state.focus_next();
        } else if input.confirm {
            state.set_focused(layout::widgets::WidgetId::new("modal-content"));
        }
    };
    if shell.global_overlay() {
        let input = inputs
            .get(shell.menu_seat)
            .or_else(|| inputs.first())
            .copied()
            .unwrap_or_default();
        apply(UiOwner::Global, input);
    } else {
        for (seat, input) in inputs.iter().enumerate() {
            apply(UiOwner::Seat(seat), *input);
        }
    }
}

/// Adds every tile in `views` that `union` lacks to `union`, returning the newly added tiles
/// (each once, even when several views discovered it).
fn merge_explored<'a>(
    union: &mut DiscoveredTiles,
    views: impl Iterator<Item = &'a DiscoveredTiles>,
) -> Vec<TilePosition> {
    let mut added = Vec::new();
    for explored in views {
        // Collected first: `union` is borrowed by the diff and grows below.
        let fresh: Vec<TilePosition> = explored.newly_discovered_since(union).collect();
        for tile in fresh {
            if union.insert(tile) {
                added.push(tile);
            }
        }
    }
    added
}

impl GameRenderer {
    pub fn new(raylib: &mut RaylibHandle, thread: &RaylibThread, terrain: &Terrain) -> Self {
        Self {
            terrain: TerrainRenderer::new(raylib, thread, terrain),
            sky: SkyArt::load(raylib, thread),
            rig_facings: RigFacings::default(),
            atlas: sprite::load_atlas(raylib, thread),
            font_regular: load_pixel_font(raylib, thread, false),
            font_bold: load_pixel_font(raylib, thread, true),
            ui_states: RefCell::new(BTreeMap::new()),
            explored_drawn: DiscoveredTiles::default(),
        }
    }

    fn font_faces(&self) -> Option<UiFontFaces> {
        Some(UiFontFaces {
            regular: **self.font_regular.as_ref()?,
            bold: **self.font_bold.as_ref()?,
        })
    }

    /// Brings terrain textures and rig facings up to date with this frame's snapshot.
    pub fn sync(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        snapshot: &RenderSnapshot<'_>,
        report: &FrameReport,
    ) {
        if report.world_replaced {
            self.terrain.mark_all_dirty();
            self.explored_drawn = DiscoveredTiles::default();
            self.rig_facings.clear();
        }
        self.rig_facings.update(&snapshot.players);
        for tile in &report.changed_tiles {
            self.terrain.mark_tile_dirty(*tile);
        }
        // The fog shows everything any local player has explored: every split-screen view
        // shares one terrain cache, so a tile one seat uncovers is lit for all of them.
        let fresh = merge_explored(
            &mut self.explored_drawn,
            snapshot.views.iter().filter_map(|view| view.explored),
        );
        let radius = EXPLORATION_REDRAW_RADIUS_TILES;
        for tile in fresh {
            for (dx, dy) in [
                (0, 0),
                (-radius, -radius),
                (radius, -radius),
                (-radius, radius),
                (radius, radius),
            ] {
                self.terrain.mark_fog_dirty(TilePosition {
                    x: tile.x + dx,
                    y: tile.y + dy,
                });
            }
        }
        let explored = &self.explored_drawn;
        let views: Vec<Rectangle> = snapshot
            .views
            .iter()
            .map(|view| {
                let (width, height) = view.visible_world_size();
                Rectangle::new(view.camera.x, view.camera.y, width, height)
            })
            .collect();
        self.terrain
            .sync(raylib, thread, snapshot.terrain, explored, &views);
    }

    /// Applies menu input to the dialog UI states before the frame is drawn: scrolling (see
    /// [`apply_modal_scroll`]) and focus steps. `inputs` holds every local seat's input in seat
    /// order. While a global screen is up only the seat driving it scrolls it; otherwise each
    /// seat's input moves only that seat's own dialog.
    pub fn apply_ui_input(&self, inputs: &[PlayerInput], shell: &ShellState) {
        route_ui_input(&mut self.ui_states.borrow_mut(), inputs, shell);
    }

    /// Forgets dialog focus and scroll (scene captures start each scene fresh).
    pub fn reset_ui_state(&self) {
        self.ui_states.borrow_mut().clear();
    }

    /// Makes `owner`'s dialog state current for the layout code.
    fn begin_ui(&self, owner: UiOwner) {
        let mut state = self
            .ui_states
            .borrow()
            .get(&owner)
            .cloned()
            .unwrap_or_default();
        state.begin_frame();
        layout::set_current_ui_state(state);
    }

    /// Stores `owner`'s dialog state after drawing its UI.
    fn end_ui(&self, owner: UiOwner) {
        if let Some(mut state) = layout::take_current_ui_state() {
            state.end_frame();
            self.ui_states.borrow_mut().insert(owner, state);
        }
    }

    /// Draws one frame: every local seat's view (the world or the building it is in, with its
    /// own HUD, dialogs, prompts and toasts), then the global screens once over all of them.
    pub fn render(
        &self,
        draw: &mut RaylibDrawHandle<'_>,
        shell: &ShellState,
        snapshot: &RenderSnapshot<'_>,
    ) {
        sprite::set_current_atlas(self.atlas.as_ref());
        // The UI clock follows shell updates so scripted captures are deterministic.
        let seconds = shell.update_ticks as f32 / 60.0;
        let screen = UiCanvas::from_draw(draw);
        ui_kit::begin_frame(screen, self.font_faces(), seconds);
        draw.clear_background(palette::SKY_LIGHT);

        // The title and online screens paint their own backdrop over the whole canvas.
        if shell.gameplay_visible() {
            for view in &snapshot.views {
                self.render_seat(draw, shell, snapshot, view, screen);
            }
            draw_free_seats(draw, snapshot, screen);
        }

        self.begin_ui(UiOwner::Global);
        layout::set_modal_placement(layout::ModalPlacement::Centred);
        prompts::set_menu_device(shell.prompt_device(shell.menu_seat));
        let _canvas = UiCanvasScope::begin(draw);
        if shell.run_mode == RunMode::Lobby {
            title::draw_backdrop(draw);
        }
        if snapshot.views.len() > 1 && shell.gameplay_visible() {
            draw_view_dividers(draw, snapshot);
        }
        let menu_view = snapshot
            .views
            .get(shell.menu_seat)
            .or_else(|| snapshot.primary_view());
        if let Some(view) = menu_view {
            let ui = UiFrame {
                shell,
                seat: shell.seat(view.seat_index),
                snapshot,
                view,
                player: view.player,
            };
            match shell.run_mode {
                RunMode::Title => title::draw_title(draw, &ui),
                RunMode::Paused => draw_pause(draw, &ui),
                RunMode::Playing | RunMode::Lobby => {}
            }
            if let Some(modal) = shell.modal {
                draw_modal(draw, &ui, modal);
            }
        }

        if shell.run_mode == RunMode::Lobby && shell.modal.is_none() {
            draw_online(draw, shell);
        }
        // Over a global screen the toast is drawn once, above it (saves and loads report
        // here); otherwise each view shows its own.
        if shell.global_overlay() && shell.gameplay_visible() {
            let seat = shell.seat(shell.menu_seat);
            if let Some((message, age)) = toast_message(shell, seat) {
                draw_toast(draw, message, age, None);
            }
        }
        if shell.online.show_net_debug {
            draw_net_debug(draw, shell);
        }

        // Volume readout in menus (bottom-left; the HUD owns the corners while playing).
        if matches!(
            shell.run_mode,
            RunMode::Title | RunMode::Paused | RunMode::Lobby
        ) {
            let canvas = ui_kit::canvas();
            let volume = format!("VOL {:.0}%", shell.master_volume * 100.0);
            let y = canvas.height - 15;
            let hint = prompts::hint(crate::input::prompts::Prompt::Volume, "");
            let text_width = ui_kit::text_width(&volume, TextStyle::Body);
            if snapshot.views.len() > 1 && shell.gameplay_visible() {
                // Split-screen views reach the bottom-left corner (a docked counter panel can
                // sit there): the readout gets its own plate instead of overlapping them.
                let width = text_width
                    + prompts::prompt_width(hint, prompts::menu_device()).map_or(0, |w| w + 5);
                ui_kit::panel(
                    draw,
                    Rectangle::new(1.0, (y - 4) as f32, (width + 10) as f32, 18.0),
                    ui_kit::PanelStyle::Standard,
                );
            }
            ui_kit::text(draw, &volume, 6, y + 2, TextStyle::Body, palette::FOG);
            prompts::draw_prompt(
                draw,
                6 + text_width + 5,
                y,
                hint,
                prompts::menu_device(),
                palette::FOG,
            );
        }
        self.end_ui(UiOwner::Global);
    }

    /// Draws one seat's view: the world with its HUD, or the building the seat is inside, then
    /// the seat's own dialog (docked at the counter inside a building), rescue prompt and toast.
    /// In split-screen everything lands inside the seat's viewport, on a UI canvas covering just
    /// that viewport ([`UiCanvas::for_viewport`]), with the seat's own device prompts.
    fn render_seat(
        &self,
        draw: &mut RaylibDrawHandle<'_>,
        shell: &ShellState,
        snapshot: &RenderSnapshot<'_>,
        view: &ViewSnapshot<'_>,
        screen: UiCanvas,
    ) {
        let split = snapshot.views.len() > 1;
        let seat = shell.seat(view.seat_index);
        let canvas = if split {
            UiCanvas::for_viewport(screen, view.viewport)
        } else {
            screen
        };
        let _restore = ui_kit::use_canvas(canvas);
        prompts::set_menu_device(shell.prompt_device(view.seat_index));
        let owner = UiOwner::Seat(view.seat_index);
        self.begin_ui(owner);
        if split {
            begin_view_clip(view.viewport);
        }
        if seat.inside() {
            let area = if split {
                let (x, y, width, height) = canvas.physical_rect(view.viewport);
                RoomArea {
                    x,
                    y,
                    width,
                    height,
                    viewport: Some(view.viewport),
                    // Narrow views keep the room against the top, so it stays put when the
                    // counter panel docks below the floor and when it closes. A room drawn
                    // smaller than the UI (two seats at the minimum window) stays centred
                    // until the panel opens: at the top, the keeper's line would cover the
                    // name plate.
                    align_top: layout::counter_sheet_leaves_room(canvas.rect())
                        && (seat.modal.is_some()
                            || crate::pixel_scale::fit_pixel_size(
                                (width, height),
                                (interior::ROOM_WIDTH, interior::ROOM_HEIGHT),
                            ) >= canvas.pixel),
                }
            } else {
                RoomArea::window(draw.get_render_width(), draw.get_render_height())
            };
            layout::set_counter_sheet_top(
                area.align_top
                    .then(|| interior::floor_canvas_y(area) as f32),
            );
            draw_interior(
                draw,
                seat,
                view,
                &snapshot.market,
                area,
                split.then_some(view.seat_index),
            );
        } else {
            self.render_world(draw, shell, snapshot, view);
        }
        {
            layout::set_modal_placement(if seat.inside() {
                layout::ModalPlacement::Counter
            } else {
                layout::ModalPlacement::Centred
            });
            let _canvas = UiCanvasScope::begin(draw);
            let ui = UiFrame {
                shell,
                seat,
                snapshot,
                view,
                player: view.player,
            };
            let _ = layout::take_last_dialog_rect();
            if let Some(modal) = seat.modal {
                draw_modal(draw, &ui, modal);
            } else if shell.run_mode == RunMode::Playing
                && shell.modal.is_none()
                && !seat.inside()
                && view.failure.is_some()
            {
                draw_rescue(draw, &ui);
            }
            let docked = layout::take_last_dialog_rect().filter(|_| seat.inside());
            if !shell.global_overlay()
                && let Some((message, age)) = toast_message(shell, seat)
            {
                draw_toast(draw, message, age, docked);
            }
        }
        if split {
            end_view_clip();
        }
        layout::set_counter_sheet_top(None);
        self.end_ui(owner);
    }

    /// Draws the art reference sheet (palette, UI kit, icons, sprites) instead of the game.
    pub fn render_art_gallery(&self, draw: &mut RaylibDrawHandle<'_>) {
        sprite::set_current_atlas(self.atlas.as_ref());
        ui_kit::begin_frame(UiCanvas::from_draw(draw), self.font_faces(), 0.0);
        draw.clear_background(palette::NIGHT);
        let _canvas = UiCanvasScope::begin(draw);
        gallery::draw_art_gallery(draw);
    }

    /// Draws the button-prompt reference sheet instead of the game.
    pub fn render_prompt_sheet(&self, draw: &mut RaylibDrawHandle<'_>) {
        sprite::set_current_atlas(self.atlas.as_ref());
        ui_kit::begin_frame(UiCanvas::from_draw(draw), self.font_faces(), 0.0);
        draw.clear_background(palette::NIGHT);
        let _canvas = UiCanvasScope::begin(draw);
        prompts::draw_prompt_sheet(draw);
    }

    /// Draws the UI reference sheet (menu icons and interior props) instead of the game.
    pub fn render_ui_sheet(&self, draw: &mut RaylibDrawHandle<'_>) {
        sprite::set_current_atlas(self.atlas.as_ref());
        ui_kit::begin_frame(UiCanvas::from_draw(draw), self.font_faces(), 0.0);
        draw.clear_background(palette::NIGHT);
        let _canvas = UiCanvasScope::begin(draw);
        ui_sheet::draw_ui_sheet(draw);
    }

    /// The world through `view`'s camera, then its HUD on the current (seat) canvas.
    fn render_world(
        &self,
        draw: &mut RaylibDrawHandle<'_>,
        shell: &ShellState,
        snapshot: &RenderSnapshot<'_>,
        view: &ViewSnapshot<'_>,
    ) {
        let viewport = view.viewport;
        let camera = Vector2::new(view.camera.x, view.camera.y);
        let (visible_width, visible_height) = view.visible_world_size();
        let density = render_scale_for_draw(draw);
        {
            let view_camera = view_camera_2d(viewport, camera, view.world_scale, density);
            let mut world_draw = draw.begin_mode2D(view_camera);
            draw_world_scene(
                &mut world_draw,
                &WorldScene {
                    visible: Rectangle::new(camera.x, camera.y, visible_width, visible_height),
                    terrain_renderer: &self.terrain,
                    terrain: snapshot.terrain,
                    sky: &self.sky,
                    infrastructure: snapshot.infrastructure,
                    hazards: snapshot.hazards,
                    bombs: snapshot.bombs,
                    boulders: snapshot.boulders,
                    sparks: snapshot.sparks,
                    explosions: snapshot.explosions,
                    relay_countdowns: &snapshot.relay_countdowns,
                    players: &snapshot.players,
                    facings: &self.rig_facings,
                    local: snapshot
                        .players
                        .iter()
                        .find(|player| player.player_id == view.player_id),
                    active_drill: view.active_drill,
                    lost_cargo: view.lost_cargo,
                    explored: Some(&self.explored_drawn),
                    ticks: shell.update_ticks,
                },
            );
        }
        let _canvas = UiCanvasScope::begin(draw);
        draw_heat_warning_for_view(draw, snapshot.terrain, view);
        draw_view_hud(draw, shell, snapshot, view);
        draw_depth_ruler_for_view(draw, snapshot.terrain, view);
        draw_minimap_for_view(
            draw,
            snapshot.terrain,
            view,
            &snapshot.players,
            &self.explored_drawn,
        );
    }
}

/// F3 network overlay: client/server ticks, input margin, RTT, loss, corrections, snapshot buffer
/// depth and hash status (or the host's per-client view), plus the dev tuning keys. Drawn on the
/// UI canvas.
fn draw_net_debug(draw: &mut RaylibDrawHandle<'_>, shell: &ShellState) {
    let mut lines = if shell.online.net_debug.is_empty() {
        vec!["Network overlay: no session data yet.".to_owned()]
    } else {
        shell.online.net_debug.clone()
    };
    lines.extend(input_debug_lines(shell));
    if cfg!(debug_assertions) {
        lines.push(format!(
            "dev: F4 select ({}) | PgUp/PgDn adjust (host)",
            crate::runtime::TUNABLE_PARAMETERS[shell.online.tune_parameter]
        ));
    }
    let canvas = ui_kit::canvas();
    let line_height = TextStyle::Body.line_height();
    let max_text = (canvas.width - 24).min(420);
    // Wrap long lines; continuation rows are indented and keep their line's colour.
    let mut rows: Vec<(String, Color)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let color = if line.contains("MISMATCH") {
            palette::RED
        } else if index == 0 {
            palette::BONE
        } else if line.starts_with("dev:") {
            palette::FOG
        } else {
            palette::MINT
        };
        for (part, wrapped) in ui_kit::wrap(line.trim_end(), max_text - 8, TextStyle::Body)
            .into_iter()
            .enumerate()
        {
            let indent = if part == 0 { "" } else { "  " };
            rows.push((format!("{indent}{wrapped}"), color));
        }
    }
    let width = rows
        .iter()
        .map(|(row, _)| ui_kit::text_width(row, TextStyle::Body))
        .max()
        .unwrap_or(0)
        .max(ui_kit::text_width("NETWORK  F3", TextStyle::Bold))
        + 16;
    let height = line_height * i32::try_from(rows.len()).unwrap_or(0) + 24;
    let (left, top) = (4, 70);
    ui_kit::panel(
        draw,
        Rectangle::new(left as f32, top as f32, width as f32, height as f32),
        ui_kit::PanelStyle::Standard,
    );
    ui_kit::text(
        draw,
        "NETWORK",
        left + 8,
        top + 6,
        TextStyle::Bold,
        palette::GOLD,
    );
    // The overlay's toggle key (a keyboard hotkey whatever device is active).
    let toggle = prompts::hint(crate::input::prompts::Prompt::NetOverlay, "");
    let keyboard = crate::input::devices::InputDevice::KEYBOARD;
    let key_width = prompts::prompt_width(toggle, keyboard).unwrap_or(0);
    prompts::draw_prompt(
        draw,
        left + width - 7 - key_width,
        top + 4,
        toggle,
        keyboard,
        palette::FOG,
    );
    ui_kit::divider(draw, left + 6, top + 16, width - 12);
    for (index, (row, color)) in rows.iter().enumerate() {
        let row_index = i32::try_from(index).unwrap_or(0);
        ui_kit::text(
            draw,
            row,
            left + 8,
            top + 20 + line_height * row_index,
            TextStyle::Body,
            *color,
        );
    }
}

/// The F3 overlay's input section: each seat's active device and the connected controllers.
fn input_debug_lines(shell: &ShellState) -> Vec<String> {
    let seats = shell.seat_devices.len().max(1);
    let mut lines: Vec<String> = (0..seats)
        .map(|seat| {
            format!(
                "input: player {} uses {}",
                seat + 1,
                shell.prompt_device(seat).label()
            )
        })
        .collect();
    if shell.gamepads.is_empty() {
        lines.push("input: no controllers connected".to_owned());
    }
    for (index, name) in &shell.gamepads {
        lines.push(format!(
            "input: controller {} \"{name}\" ({} glyphs)",
            u32::from(*index) + 1,
            crate::input::devices::PadFamily::from_name(name).label()
        ));
    }
    lines
}

/// Ink seams between split-screen views, so each player's view reads as its own frame.
/// Fills the split-screen grid cells no seat uses (three seats leave one of four free) with a
/// card inviting a spare controller to join, instead of bare backdrop.
fn draw_free_seats(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    screen: UiCanvas,
) {
    use crate::input::{devices::InputDevice, prompts::Prompt};
    let count = snapshot.views.len();
    if count < 2 {
        return;
    }
    let (width, height) = snapshot.views.iter().fold((0, 0), |(width, height), view| {
        (
            width.max(view.viewport.x + view.viewport.width),
            height.max(view.viewport.y + view.viewport.height),
        )
    });
    for (index, viewport) in crate::render_snapshot::empty_split_cells(count, width, height)
        .into_iter()
        .enumerate()
    {
        let (x, y, cell_width, cell_height) = screen.physical_rect(viewport);
        draw.draw_rectangle(x, y, cell_width, cell_height, palette::NIGHT);
        let canvas = UiCanvas::for_viewport(screen, viewport);
        let _restore = ui_kit::use_canvas(canvas);
        let _canvas = UiCanvasScope::begin(draw);
        let seat = count + index;
        let title = format!("Player {}", seat + 1);
        let join = prompts::hint(Prompt::Join, "join");
        let device = InputDevice::Unassigned;
        let line = "Spare controller:";
        let prompt_width = prompts::prompt_width(join, device).unwrap_or(0);
        let body_width = ui_kit::text_width(line, TextStyle::Body) + 6 + prompt_width;
        let panel_width = body_width.max(ui_kit::text_width(&title, TextStyle::Bold)) + 24;
        let panel_height = 46;
        let left = (canvas.width - panel_width) / 2;
        let top = (canvas.height - panel_height) / 2;
        ui_kit::panel(
            draw,
            Rectangle::new(
                left as f32,
                top as f32,
                panel_width as f32,
                panel_height as f32,
            ),
            ui_kit::PanelStyle::Standard,
        );
        ui_kit::text_centred(
            draw,
            &title,
            canvas.width / 2,
            top + 8,
            TextStyle::Bold,
            palette::FOG,
        );
        let body_left = (canvas.width - body_width) / 2;
        ui_kit::text(
            draw,
            line,
            body_left,
            top + 28,
            TextStyle::Body,
            palette::MIST,
        );
        prompts::draw_prompt(
            draw,
            body_left + ui_kit::text_width(line, TextStyle::Body) + 6,
            top + 26,
            join,
            device,
            palette::MIST,
        );
    }
}

fn draw_view_dividers(draw: &mut RaylibDrawHandle<'_>, snapshot: &RenderSnapshot<'_>) {
    let canvas = ui_kit::canvas();
    for view in &snapshot.views {
        let rect = canvas.viewport_rect(view.viewport);
        let (x, y) = (rect.x as i32, rect.y as i32);
        let (right, bottom) = ((rect.x + rect.width) as i32, (rect.y + rect.height) as i32);
        if right < canvas.width {
            draw.draw_rectangle(right - 1, y, 2, bottom - y, palette::INK);
            draw.draw_rectangle(right - 2, y, 1, bottom - y, palette::SLATE);
        }
        if bottom < canvas.height {
            draw.draw_rectangle(x, bottom - 1, right - x, 2, palette::INK);
            draw.draw_rectangle(x, bottom - 2, right - x, 1, palette::SLATE);
        }
    }
}

fn begin_view_clip(viewport: crate::render_snapshot::Viewport) {
    // SAFETY: raylib scissor calls only set GPU state; they are balanced by `end_view_clip`
    // within the same drawing pass.
    unsafe {
        ffi::BeginScissorMode(viewport.x, viewport.y, viewport.width, viewport.height);
    }
}

fn end_view_clip() {
    // SAFETY: closes the scissor region opened by `begin_view_clip`.
    unsafe {
        ffi::EndScissorMode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < f32::EPSILON,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn pixel_density_is_render_pixels_per_screen_point() {
        assert_near(pixel_density((2560, 1440), (1280, 720)), 2.0);
        assert_near(pixel_density((1920, 1080), (1920, 1080)), 1.0);
        assert_near(pixel_density((0, 0), (0, 0)), 1.0);
    }

    /// Where a `Camera2D` puts world point `point`, in physical framebuffer pixels.
    fn project(camera: Camera2D, point: Vector2) -> Vector2 {
        Vector2::new(
            (point.x - camera.target.x).mul_add(camera.zoom, camera.offset.x),
            (point.y - camera.target.y).mul_add(camera.zoom, camera.offset.y),
        )
    }

    #[test]
    fn view_camera_centres_the_rig_in_its_viewport_on_any_display() {
        use crate::render_snapshot::{
            ScreenSize, Viewport, split_screen_viewports, visible_world_size,
            world_scale_for_screen,
        };
        for (width, height) in [(960, 540), (1280, 720), (1920, 1080), (2560, 1080)] {
            for density in [1.0, 2.0] {
                let world_scale = world_scale_for_screen(ScreenSize {
                    width,
                    height,
                    density,
                });
                for viewport in split_screen_viewports(2, width, height) {
                    let camera = Vector2::new(400.0, 1_200.0);
                    let (visible_width, visible_height) = visible_world_size(viewport, world_scale);
                    let rig = Vector2::new(
                        visible_width.mul_add(0.5, camera.x),
                        visible_height.mul_add(0.5, camera.y),
                    );
                    let camera_2d = view_camera_2d(viewport, camera, world_scale, density);
                    let on_screen = project(camera_2d, rig);
                    let Viewport {
                        x,
                        y,
                        width: view_width,
                        height: view_height,
                    } = viewport;
                    let expected_x = (x as f32 + view_width as f32 * 0.5) * density;
                    let expected_y = (y as f32 + view_height as f32 * 0.5) * density;
                    assert!((on_screen.x - expected_x).abs() < 0.01, "{on_screen:?}");
                    assert!((on_screen.y - expected_y).abs() < 0.01, "{on_screen:?}");
                    // The visible world's far corner lands on the viewport's far corner.
                    let corner = project(
                        camera_2d,
                        Vector2::new(camera.x + visible_width, camera.y + visible_height),
                    );
                    assert!((corner.x - (x + view_width) as f32 * density).abs() < 0.01);
                    assert!((corner.y - (y + view_height) as f32 * density).abs() < 0.01);
                }
            }
        }
    }

    /// A dialog state with an open, scrollable dialog (as drawing one leaves it).
    fn open_dialog() -> layout::widgets::UiState {
        let id = layout::widgets::WidgetId::new("modal-content");
        let mut state = layout::widgets::UiState::default();
        state.enter_modal("Upgrade Shop", &id);
        state.set_scroll_limit(id, 400.0);
        state
    }

    fn offset(states: &BTreeMap<UiOwner, layout::widgets::UiState>, owner: UiOwner) -> f32 {
        states.get(&owner).map_or(0.0, |state| {
            state.scroll_offset(&layout::widgets::WidgetId::new("modal-content"))
        })
    }

    #[test]
    fn each_seat_scrolls_only_its_own_dialog_and_global_screens_take_the_menu_seat() {
        let mut shell = ShellState::default();
        shell.enter_world("test", 0.0);
        shell.set_seat_count(2);
        let mut states = BTreeMap::from([
            (UiOwner::Seat(0), open_dialog()),
            (UiOwner::Seat(1), open_dialog()),
            (UiOwner::Global, open_dialog()),
        ]);
        let scroll = PlayerInput {
            ui_scroll: -1.0,
            ..PlayerInput::default()
        };
        // Seat 2's right stick scrolls seat 2's dialog only.
        route_ui_input(&mut states, &[PlayerInput::default(), scroll], &shell);
        assert_near(offset(&states, UiOwner::Seat(0)), 0.0);
        assert!(offset(&states, UiOwner::Seat(1)) > 0.0);
        assert_near(offset(&states, UiOwner::Global), 0.0);

        // Page Down on seat 1's keyboard pages seat 1's dialog.
        let page_down = PlayerInput {
            page_down: true,
            ..PlayerInput::default()
        };
        states
            .get_mut(&UiOwner::Seat(0))
            .expect("seat state")
            .set_page_step(layout::widgets::WidgetId::new("modal-content"), 100.0);
        route_ui_input(&mut states, &[page_down, PlayerInput::default()], &shell);
        assert_near(offset(&states, UiOwner::Seat(0)), 100.0);

        // With the pause menu up, only the seat that paused scrolls, and only the global state.
        shell.run_mode = crate::shell::RunMode::Paused;
        shell.menu_seat = 1;
        let before = (
            offset(&states, UiOwner::Seat(0)),
            offset(&states, UiOwner::Seat(1)),
        );
        route_ui_input(&mut states, &[scroll, PlayerInput::default()], &shell);
        assert_near(offset(&states, UiOwner::Global), 0.0);
        route_ui_input(&mut states, &[PlayerInput::default(), scroll], &shell);
        assert!(offset(&states, UiOwner::Global) > 0.0);
        assert_eq!(
            (
                offset(&states, UiOwner::Seat(0)),
                offset(&states, UiOwner::Seat(1))
            ),
            before
        );
    }

    #[test]
    fn seat_canvases_cover_their_viewport_at_a_readable_scale() {
        use crate::render_snapshot::split_screen_viewports;
        for (width, height, seats, pixel, canvas) in [
            (1280, 720, 2, 2, (320, 360)),
            (1280, 720, 4, 2, (320, 180)),
            (1920, 1080, 2, 3, (320, 360)),
            (1920, 1080, 4, 2, (480, 270)),
            (960, 540, 2, 2, (240, 270)),
            (960, 540, 4, 1, (480, 270)),
            (2560, 1440, 4, 3, (426, 240)),
        ] {
            let screen = UiCanvas::for_framebuffer(width, height, 1.0);
            for viewport in split_screen_viewports(seats, width, height) {
                let seat = UiCanvas::for_viewport(screen, viewport);
                assert_eq!(
                    (seat.pixel, (seat.width, seat.height)),
                    (pixel, canvas),
                    "{width}x{height}, {seats} seats"
                );
                assert_eq!((seat.origin_x, seat.origin_y), (viewport.x, viewport.y));
                // The seat's own view maps onto its whole canvas.
                let rect = seat.viewport_rect(viewport);
                assert_near(rect.x, 0.0);
                assert_near(rect.y, 0.0);
                assert!(rect.width <= seat.width as f32 && rect.height <= seat.height as f32);
                // And its canvas maps back inside the viewport (scissor clipping).
                let points = seat.to_points(seat.rect());
                assert!(points.x >= viewport.x as f32 - 0.01);
                assert!(points.x + points.width <= (viewport.x + viewport.width) as f32 + 0.01);
            }
        }
    }

    #[test]
    fn explored_union_merges_every_local_view_once() {
        let tile = |x, y| TilePosition { x, y };
        let mut first = DiscoveredTiles::default();
        let mut second = DiscoveredTiles::default();
        let _ = first.insert(tile(1, 5));
        let _ = first.insert(tile(2, 5));
        let _ = second.insert(tile(2, 5));
        let _ = second.insert(tile(40, 30));
        let mut union = DiscoveredTiles::default();
        let fresh = merge_explored(&mut union, [&first, &second].into_iter());
        assert_eq!(fresh.len(), 3, "shared tiles are added once: {fresh:?}");
        assert!(
            union.contains(tile(40, 30)),
            "player 2's discoveries light the fog"
        );
        assert!(
            merge_explored(&mut union, [&first, &second].into_iter()).is_empty(),
            "nothing new the second time"
        );
    }

    #[test]
    fn ui_scroll_delta_scrolls_with_the_wheel_but_not_menu_navigation() {
        assert_near(
            ui_scroll_delta(
                PlayerInput {
                    ui_scroll: 1.0,
                    ..PlayerInput::default()
                },
                48.0,
            ),
            -48.0,
        );
        for input in [
            PlayerInput {
                menu_down: true,
                ..PlayerInput::default()
            },
            PlayerInput {
                menu_up: true,
                ..PlayerInput::default()
            },
        ] {
            assert_near(ui_scroll_delta(input, 48.0), 0.0);
        }
    }
}
