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

mod gallery;
mod interior;
mod layout;
mod pixel_art;
mod screen;
mod terrain;
mod ui_kit;
mod world;

use interior::draw_interior;
use pixel_art::{font::load_pixel_font, palette, sprite};
use screen::{
    UiFrame, draw_depth_ruler_for_view, draw_game_over, draw_heat_warning_for_view,
    draw_minimap_for_view, draw_modal, draw_online, draw_pause, draw_title, draw_view_hud,
};
use terrain::TerrainRenderer;
use ui_kit::{TextStyle, UiCanvas, UiCanvasScope, UiFontFaces};
use world::{SkyArt, WorldScene, draw_world_scene, world_camera};

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
    /// Every sprite, rasterized at startup (see `pixel_art::sprite`).
    atlas: Option<Texture2D>,
    font_regular: Option<Font>,
    font_bold: Option<Font>,
    ui_state: RefCell<layout::widgets::UiState>,
    /// Discovered tiles already drawn by the terrain renderer (primary view).
    explored_drawn: DiscoveredTiles,
}

fn ui_scroll_delta(input: PlayerInput, step: f32) -> f32 {
    -input.ui_scroll * step + if input.menu_down { step } else { 0.0 }
        - if input.menu_up { step } else { 0.0 }
}

impl GameRenderer {
    pub fn new(raylib: &mut RaylibHandle, thread: &RaylibThread, terrain: &Terrain) -> Self {
        Self {
            terrain: TerrainRenderer::new(raylib, thread, terrain),
            sky: SkyArt::load(raylib, thread),
            atlas: sprite::load_atlas(raylib, thread),
            font_regular: load_pixel_font(raylib, thread, false),
            font_bold: load_pixel_font(raylib, thread, true),
            ui_state: RefCell::new(layout::widgets::UiState::default()),
            explored_drawn: DiscoveredTiles::default(),
        }
    }

    fn font_faces(&self) -> Option<UiFontFaces> {
        Some(UiFontFaces {
            regular: **self.font_regular.as_ref()?,
            bold: **self.font_bold.as_ref()?,
        })
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
                    self.terrain.mark_fog_dirty(TilePosition {
                        x: tile.x + dx,
                        y: tile.y + dy,
                    });
                }
            }
            self.explored_drawn.clone_from(explored);
        }
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

    /// Draws one frame: every local view, then the shell overlays once on the UI canvas.
    pub fn render(
        &self,
        draw: &mut RaylibDrawHandle<'_>,
        shell: &ShellState,
        snapshot: &RenderSnapshot<'_>,
    ) {
        layout::set_current_ui_state(self.ui_state.borrow().clone());
        sprite::set_current_atlas(self.atlas.as_ref());
        // The UI clock follows shell updates so scripted captures are deterministic.
        let seconds = shell.update_ticks as f32 / 60.0;
        ui_kit::begin_frame(UiCanvas::from_draw(draw), self.font_faces(), seconds);
        layout::set_modal_placement(if shell.run_mode == RunMode::Interior {
            layout::ModalPlacement::Counter
        } else {
            layout::ModalPlacement::Centred
        });
        draw.clear_background(palette::SKY_LIGHT);

        if shell.run_mode == RunMode::Interior {
            if let Some(view) = snapshot.primary_view() {
                draw_interior(draw, shell, view);
            }
        } else {
            for view in &snapshot.views {
                self.render_view(draw, shell, snapshot, view);
            }
        }

        let _canvas = UiCanvasScope::begin(draw);
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

        // Volume readout in menus (bottom-left; the HUD owns the corners while playing).
        if matches!(
            shell.run_mode,
            RunMode::Title | RunMode::Paused | RunMode::Lobby
        ) {
            let canvas = ui_kit::canvas();
            ui_kit::text(
                draw,
                &format!("VOL {:.0}%  (+/-)", shell.master_volume * 100.0),
                6,
                canvas.height - 13,
                TextStyle::Body,
                palette::FOG,
            );
        }
        if let Some(ui_state) = layout::take_current_ui_state() {
            *self.ui_state.borrow_mut() = ui_state;
        }
    }

    /// Draws the art reference sheet (palette, UI kit, icons, sprites) instead of the game.
    pub fn render_art_gallery(&self, draw: &mut RaylibDrawHandle<'_>) {
        sprite::set_current_atlas(self.atlas.as_ref());
        ui_kit::begin_frame(UiCanvas::from_draw(draw), self.font_faces(), 0.0);
        draw.clear_background(palette::NIGHT);
        let _canvas = UiCanvasScope::begin(draw);
        gallery::draw_art_gallery(draw);
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
                    players: &snapshot.players,
                    local: snapshot
                        .players
                        .iter()
                        .find(|player| player.player_id == view.player_id),
                    active_drill: view.active_drill,
                    lost_cargo: view.lost_cargo,
                    explored: view.explored,
                    ticks: shell.update_ticks,
                },
            );
        }
        // The title and online screens show the world behind them without a HUD.
        if !matches!(shell.run_mode, RunMode::Title | RunMode::Lobby) {
            let _canvas = UiCanvasScope::begin(draw);
            draw_heat_warning_for_view(draw, snapshot.terrain, view);
            draw_view_hud(draw, shell, snapshot, view);
            draw_depth_ruler_for_view(draw, snapshot.terrain, view);
            draw_minimap_for_view(draw, snapshot.terrain, view, &snapshot.players);
        }
        if clip {
            end_view_clip();
        }
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
    if cfg!(debug_assertions) {
        lines.push(format!(
            "dev: F4 select ({}) | PgUp/PgDn adjust (host)",
            crate::runtime::TUNABLE_PARAMETERS[shell.online.tune_parameter]
        ));
    }
    let line_height = TextStyle::Body.line_height();
    let width = lines
        .iter()
        .map(|line| ui_kit::text_width(line, TextStyle::Body))
        .max()
        .unwrap_or(0)
        + 14;
    let height = line_height * i32::try_from(lines.len()).unwrap_or(0) + 10;
    let top = 70;
    ui_kit::panel(
        draw,
        Rectangle::new(4.0, top as f32, width as f32, height as f32),
        ui_kit::PanelStyle::Standard,
    );
    for (index, line) in lines.iter().enumerate() {
        let color = if line.contains("MISMATCH") {
            palette::RED
        } else {
            palette::LIME
        };
        let row = i32::try_from(index).unwrap_or(0);
        ui_kit::text(
            draw,
            line,
            11,
            top + 6 + line_height * row,
            TextStyle::Body,
            color,
        );
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
