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

        // Bottom-left corner: the top edge belongs to the HUD cards, bottom-right to the minimap.
        let screen_height = draw.get_screen_height();
        draw.draw_text(
            &format!("Vol {:.0}% (+/-)", shell.master_volume * 100.0),
            12,
            screen_height - 26,
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
        let (visible_width, visible_height) = view.visible_world_size();
        let density = render_scale_for_draw(draw);
        {
            let view_camera = view_camera_2d(viewport, camera, view.world_scale, density);
            let mut world_draw = draw.begin_mode2D(view_camera);
            draw_world(
                &mut world_draw,
                Rectangle::new(camera.x, camera.y, visible_width, visible_height),
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
            Viewport, split_screen_viewports, visible_world_size, world_scale_for_screen,
        };
        for (width, height) in [(960, 540), (1280, 720), (1920, 1080), (2560, 1080)] {
            let world_scale = world_scale_for_screen(height);
            for density in [1.0, 2.0] {
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
