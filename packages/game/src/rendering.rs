#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "rendering APIs use integer pixels while camera math uses floats"
)]

//! Draws a [`RenderSnapshot`] plus [`ShellState`]. The renderer never reads the simulation.

use std::cell::RefCell;

use raylib::{ffi, prelude::*};

use drillgame_simulation::{
    DiscoveredTiles,
    terrain::{Terrain, TilePosition},
};

mod interior;
mod layout;
mod screen;
mod terrain;
mod world;

use interior::draw_interior;
use screen::{
    UiFrame, draw_depth_ruler_for_view, draw_game_over, draw_heat_warning_for_view,
    draw_minimap_for_view, draw_modal, draw_online, draw_pause, draw_title, draw_view_hud,
};
use terrain::TerrainRenderer;
use world::{draw_particles, draw_placed_bombs, draw_scanner_marks, draw_world, world_camera};

use crate::{
    input::PlayerInput,
    local_game::FrameReport,
    render_snapshot::{RenderSnapshot, ViewSnapshot},
    shell::{RunMode, ShellState},
};

pub use crate::render_snapshot::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// Radius (tiles) around a newly explored tile whose darkness gradient changes.
const EXPLORATION_REDRAW_RADIUS_TILES: i32 = 12;

fn render_scale_for_draw(draw: &RaylibDrawHandle<'_>) -> f32 {
    let scale_x = draw.get_render_width() as f32 / draw.get_screen_width().max(1) as f32;
    let scale_y = draw.get_render_height() as f32 / draw.get_screen_height().max(1) as f32;
    scale_x.min(scale_y).max(1.0)
}

pub struct GameRenderer {
    terrain: TerrainRenderer,
    ui_font_assets: UiFontAssets,
    ui_state: RefCell<layout::widgets::UiState>,
    /// Discovered tiles already drawn by the terrain renderer (primary view).
    explored_drawn: DiscoveredTiles,
}

struct UiFontAssets {
    title: Option<Font>,
    heading: Option<Font>,
    small: Option<Font>,
}

impl UiFontAssets {
    fn load(raylib: &mut RaylibHandle, thread: &RaylibThread) -> Self {
        Self {
            title: load_ui_font(
                raylib,
                thread,
                "title",
                include_bytes!("../../../assets/ui/title.ttf"),
                34,
            ),
            heading: load_ui_font(
                raylib,
                thread,
                "heading",
                include_bytes!("../../../assets/ui/heading.ttf"),
                20,
            ),
            small: load_ui_font(
                raylib,
                thread,
                "body",
                include_bytes!("../../../assets/ui/body.ttf"),
                15,
            ),
        }
    }

    fn fonts(&self) -> layout::UiFonts {
        let fallback = unsafe { ffi::GetFontDefault() };
        layout::UiFonts::from_raw(
            self.title.as_ref().map_or(fallback, |font| **font),
            self.heading.as_ref().map_or(fallback, |font| **font),
            self.small.as_ref().map_or(fallback, |font| **font),
        )
    }
}

fn load_ui_font(
    raylib: &mut RaylibHandle,
    thread: &RaylibThread,
    name: &str,
    data: &[u8],
    font_size: i32,
) -> Option<Font> {
    match raylib.load_font_from_memory(thread, ".ttf", data, font_size * 2, None) {
        Ok(font) => {
            unsafe {
                ffi::SetTextureFilter(
                    font.texture,
                    ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32,
                );
            }
            Some(font)
        }
        Err(error) => {
            eprintln!("Failed to load embedded UI font {name}: {error}");
            None
        }
    }
}

fn ui_scroll_delta(input: PlayerInput, step: f32) -> f32 {
    -input.ui_scroll * step + if input.menu_down { step } else { 0.0 }
        - if input.menu_up { step } else { 0.0 }
}

impl GameRenderer {
    pub fn new(raylib: &mut RaylibHandle, thread: &RaylibThread, terrain: &Terrain) -> Self {
        let ui_font_assets = UiFontAssets::load(raylib, thread);
        Self {
            terrain: TerrainRenderer::new(raylib, thread, terrain),
            ui_font_assets,
            ui_state: RefCell::new(layout::widgets::UiState::default()),
            explored_drawn: DiscoveredTiles::default(),
        }
    }

    /// Brings terrain textures up to date with this frame's snapshot.
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
        }
        for tile in &report.changed_tiles {
            self.terrain.mark_tile_dirty(*tile);
        }
        let empty = DiscoveredTiles::default();
        let explored = snapshot
            .primary_view()
            .and_then(|view| view.explored)
            .unwrap_or(&empty);
        if explored.len() != self.explored_drawn.len() {
            let radius = EXPLORATION_REDRAW_RADIUS_TILES;
            for tile in explored.newly_discovered_since(&self.explored_drawn) {
                for (dx, dy) in [
                    (0, 0),
                    (-radius, -radius),
                    (radius, -radius),
                    (-radius, radius),
                    (radius, radius),
                ] {
                    self.terrain.mark_tile_dirty(TilePosition {
                        x: tile.x + dx,
                        y: tile.y + dy,
                    });
                }
            }
            self.explored_drawn.clone_from(explored);
        }
        self.terrain
            .sync(raylib, thread, snapshot.terrain, explored);
    }

    pub fn apply_ui_input(&self, input: PlayerInput) {
        const MODAL_SCROLL_STEP: f32 = 48.0;
        const MODAL_SCROLL_LIMIT: f32 = 10_000.0;
        let scroll_delta = ui_scroll_delta(input, MODAL_SCROLL_STEP);
        if scroll_delta.abs() > f32::EPSILON {
            self.ui_state.borrow_mut().scroll_by(
                layout::widgets::WidgetId::new("modal-content"),
                scroll_delta,
                MODAL_SCROLL_LIMIT,
            );
        }
        if input.menu_up {
            self.ui_state.borrow_mut().focus_previous();
        } else if input.menu_down {
            self.ui_state.borrow_mut().focus_next();
        } else if input.confirm {
            self.ui_state
                .borrow_mut()
                .set_focused(layout::widgets::WidgetId::new("modal-content"));
        }
    }

    /// Draws one frame: every local view, then the shell overlays once.
    pub fn render(
        &self,
        draw: &mut RaylibDrawHandle<'_>,
        shell: &ShellState,
        snapshot: &RenderSnapshot<'_>,
    ) {
        layout::set_current_fonts(self.ui_font_assets.fonts());
        layout::set_current_ui_state(self.ui_state.borrow().clone());
        draw.clear_background(Color::new(105, 190, 235, 255));

        if shell.run_mode == RunMode::Interior {
            if let Some(view) = snapshot.primary_view() {
                draw_interior(draw, shell, view);
            }
        } else {
            for view in &snapshot.views {
                self.render_view(draw, shell, snapshot, view);
            }
        }

        if let Some(view) = snapshot.primary_view() {
            let ui = UiFrame {
                shell,
                snapshot,
                view,
                player: view.player,
            };
            match shell.run_mode {
                RunMode::Title => draw_title(draw, &ui),
                RunMode::Paused => draw_pause(draw, &ui),
                RunMode::Playing | RunMode::Interior | RunMode::Lobby => {}
            }
            if let Some(modal) = shell.modal {
                draw_modal(draw, &ui, modal);
            } else if shell.run_mode == RunMode::Playing && view.failure.is_some() {
                draw_game_over(draw, &ui);
            }
        }

        if shell.run_mode == RunMode::Lobby && shell.modal.is_none() {
            draw_online(draw, shell);
        }
        if shell.online.show_net_debug {
            draw_net_debug(draw, shell);
        }

        draw.draw_text(
            &format!("Vol {:.0}% (+/-)", shell.master_volume * 100.0),
            1030,
            20,
            18,
            Color::LIGHTGRAY,
        );
        if let Some(ui_state) = layout::take_current_ui_state() {
            *self.ui_state.borrow_mut() = ui_state;
        }
    }

    fn render_view(
        &self,
        draw: &mut RaylibDrawHandle<'_>,
        shell: &ShellState,
        snapshot: &RenderSnapshot<'_>,
        view: &ViewSnapshot<'_>,
    ) {
        let viewport = view.viewport;
        let clip = snapshot.views.len() > 1;
        if clip {
            begin_view_clip(viewport);
        }
        let camera = Vector2::new(view.camera.x, view.camera.y);
        let world_zoom = render_scale_for_draw(draw);
        {
            let mut view_camera = world_camera(camera, world_zoom);
            view_camera.offset = Vector2::new(viewport.x as f32, viewport.y as f32);
            let mut world_draw = draw.begin_mode2D(view_camera);
            draw_world(
                &mut world_draw,
                camera,
                &self.terrain,
                snapshot.terrain,
                snapshot.infrastructure,
                view.active_drill,
                shell.update_ticks,
            );
            draw_particles(
                &mut world_draw,
                view.lost_cargo,
                snapshot.boulders,
                snapshot.sparks,
            );
            draw_placed_bombs(&mut world_draw, snapshot.bombs);
            if let Some(local) = snapshot
                .players
                .iter()
                .find(|player| player.player_id == view.player_id)
            {
                draw_scanner_marks(&mut world_draw, snapshot.terrain, local);
            }
            for cloud in snapshot.hazards {
                world_draw.draw_circle_v(
                    Vector2::new(cloud.x, cloud.y),
                    cloud.radius,
                    Color::new(90, 190, 80, 70),
                );
            }
            for player in &snapshot.players {
                if player.player_id == view.player_id {
                    world::draw_player_at(&mut world_draw, player);
                } else {
                    world::draw_remote_player(&mut world_draw, player);
                }
            }
        }
        draw_heat_warning_for_view(draw, snapshot.terrain, view);
        draw_view_hud(draw, shell, snapshot, view);
        draw_depth_ruler_for_view(draw, snapshot.terrain, view);
        draw_minimap_for_view(draw, snapshot.terrain, view, &snapshot.players);
        if clip {
            end_view_clip();
        }
    }
}

/// F3 network overlay: client/server ticks, input margin, RTT, loss, corrections, snapshot buffer
/// depth and hash status (or the host's per-client view), plus the dev tuning keys.
fn draw_net_debug(draw: &mut RaylibDrawHandle<'_>, shell: &ShellState) {
    const LINE_HEIGHT: i32 = 18;
    let mut lines = if shell.online.net_debug.is_empty() {
        vec!["Network overlay: no session data yet.".to_owned()]
    } else {
        shell.online.net_debug.clone()
    };
    if cfg!(debug_assertions) {
        lines.push(format!(
            "dev: F4 select ({}) | PgUp/PgDn adjust (host)",
            crate::runtime::TUNABLE_PARAMETERS[shell.online.tune_parameter]
        ));
    }
    let width = lines
        .iter()
        .map(|line| draw.measure_text(line, 16))
        .max()
        .unwrap_or(0)
        + 20;
    let height = LINE_HEIGHT * i32::try_from(lines.len()).unwrap_or(0) + 12;
    draw.draw_rectangle(8, 90, width, height, Color::new(0, 0, 0, 190));
    for (index, line) in lines.iter().enumerate() {
        let color = if line.contains("MISMATCH") {
            Color::RED
        } else {
            Color::LIME
        };
        let row = i32::try_from(index).unwrap_or(0);
        draw.draw_text(line, 18, 96 + LINE_HEIGHT * row, 16, color);
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
    fn ui_scroll_delta_combines_mouse_wheel_and_keyboard_navigation() {
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
        assert_near(
            ui_scroll_delta(
                PlayerInput {
                    menu_down: true,
                    ..PlayerInput::default()
                },
                48.0,
            ),
            48.0,
        );
        assert_near(
            ui_scroll_delta(
                PlayerInput {
                    menu_up: true,
                    ..PlayerInput::default()
                },
                48.0,
            ),
            -48.0,
        );
    }
}
