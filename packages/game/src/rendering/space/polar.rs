//! A body's ground, bent into a ring (expansion plan F12).
//!
//! The ground is painted flat, in strip coordinates, with the home world's own tile art: a
//! **fine** window of tile art round the craft (96 x 64 tiles, art-pixel exact) and, while the
//! camera is far enough out to see past it, a **coarse** map of one texel per tile (the whole
//! ring of a small world, or a window of a big one). The polar shader
//! ([`super::shaders::POLAR_BODY`]) then bends both into the ring on the GPU: exact curves at
//! every pixel, with each deeper band's columns halved, the core chamber at the centre, the
//! body's day and night, the dark of the deep ground with the craft's lamp, and the sky's
//! atmosphere. Without the shader (a driver that refuses it) the fine window is drawn as
//! curved quads instead ([`PolarGround::draw_quads`]), the plan's other route.
//!
//! The fine window's art is kept on the CPU and repainted a little at a time ([`fine`]): a
//! moved window copies what it already holds, a changed tile ([`PolarGround::mark_tile_dirty`])
//! repaints its rows, and new cells show the coarse map until their art is painted.
//!
//! Everything is worked out relative to the camera: the camera's column is an `f64`, each
//! band's offset into its window is taken in `f64` on the CPU, and the shader only ever sees
//! small numbers (tiles from the craft), so a 60,000-column world draws as exactly as a small
//! moon.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "texture sizes, tile windows and texels are small whole numbers drawn as floats"
)]

use std::f64::consts::TAU;

use raylib::{ffi, prelude::*};

use fatherload_simulation::terrain::{Terrain, TilePosition};

mod coarse;
mod fine;

#[cfg(not(target_os = "emscripten"))]
pub use coarse::paint_whole_frames;

use super::{
    shaders,
    view::{GroundThing, GroundThingKind, GroundView, MAX_HALVINGS, PolarGeometry, PolarStrip},
};
use crate::rendering::{
    pixel_art::{
        palette,
        shade::{TRANSPARENT, color_at},
        sprite,
    },
    terrain::tile_art::{self, TILE_ART},
};

/// The fine window's smallest size (tiles); it grows to cover the view when zoomed out.
pub const FINE_TILES: (i32, i32) = (96, 64);
/// The fine window's largest side (tiles): 3,840 art pixels, inside every `WebGL1` GPU's
/// 4,096.
const FINE_MAX: i32 = 240;
/// Windows move in steps of this many tiles (fewer repaints).
const WINDOW_STEP: i32 = 16;
/// Largest coarse map side (texels): every `WebGL1` GPU takes 4,096.
const COARSE_MAX: i32 = 4_096;
/// A coarse window of a big world (texels).
const COARSE_WINDOW: (i32, i32) = (2_048, 1_024);
/// Rows of sky above the surface row the windows keep.
const SKY_ROWS: i32 = 48;

/// One painted window of the strip.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Window {
    body: u64,
    /// First row.
    row0: i32,
    /// Size (tiles).
    size: (i32, i32),
    /// Each band's first column (band columns).
    origins: [i64; MAX_HALVINGS + 1],
    /// Whether the coarse window holds whole rings (columns wrap).
    ring: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Locations {
    coarse: i32,
    view: i32,
    cam: i32,
    fine: i32,
    fine_a: i32,
    fine_b: i32,
    coarse_info: i32,
    coarse_a: i32,
    coarse_b: i32,
    halve_a: i32,
    halve_b: i32,
    cols_a: i32,
    cols_b: i32,
    band8: i32,
    geo: i32,
    sun: i32,
    atmos: i32,
}

/// The polar ground pass: its shader and the painted windows.
pub struct PolarGround {
    shader: Option<Shader>,
    locations: Locations,
    fine: Option<Texture2D>,
    fine_art: fine::FineArt,
    coarse: Option<Texture2D>,
    /// The coarse map's texels, painted a little at a time (Wave 13 QA).
    coarse_map: coarse::CoarseMap,
    /// Draw the curved quads even with the shader (the fallback's own screenshot).
    pub force_quads: bool,
    /// Whether this frame's view reaches past the fine window (the coarse map is drawn).
    coarse_used: bool,
}

/// Where the camera is on the ring this frame (all the shader's and the quads' numbers).
#[derive(Clone, Copy, Debug)]
pub struct RingCamera {
    /// Surface column (continuous).
    pub column: f64,
    pub row: f32,
    /// Radius of the camera (tiles).
    pub radius: f32,
    /// Roll against the local up (radians, clockwise).
    pub roll: f32,
    pub px_per_tile: f32,
    /// Surface columns.
    pub columns: f64,
    /// The pivot (screen points).
    pub pivot: (f32, f32),
}

impl RingCamera {
    /// Where the ground point at surface column `column`, row `row` is on screen, and the turn
    /// (radians, clockwise) of its local up there.
    #[must_use]
    pub fn place(&self, geometry: &PolarGeometry, column: f64, row: f32) -> ((f32, f32), f32) {
        let mut delta = (column - self.column).rem_euclid(self.columns);
        if delta > self.columns * 0.5 {
            delta -= self.columns;
        }
        let phi = delta / self.columns * TAU;
        let radius = f64::from(geometry.radius_of_row(row));
        let (sin, cos) = phi.sin_cos();
        // The planet frame with the camera's up along +y; the craft's frame has y down.
        let x = radius * sin;
        let y = f64::from(self.radius) - radius * cos;
        let (x, y) = (x as f32 * self.px_per_tile, y as f32 * self.px_per_tile);
        let (s, c) = (-self.roll).sin_cos();
        (
            (self.pivot.0 + c * x - s * y, self.pivot.1 + s * x + c * y),
            phi as f32 - self.roll,
        )
    }
}

impl PolarGround {
    pub fn new(raylib: &mut RaylibHandle, thread: &RaylibThread) -> Self {
        let source = shaders::fragment_source(cfg!(target_os = "emscripten"), shaders::POLAR_BODY);
        let shader = super::load_shader(raylib, thread, &source, "polar ground");
        let locations = shader.as_ref().map_or_else(Locations::default, |shader| {
            let at = |name: &str| shader.get_shader_location(name);
            Locations {
                coarse: at("uCoarse"),
                view: at("uView"),
                cam: at("uCam"),
                fine: at("uFine"),
                fine_a: at("uFineOffA"),
                fine_b: at("uFineOffB"),
                coarse_info: at("uCoarseInfo"),
                coarse_a: at("uCoarseOffA"),
                coarse_b: at("uCoarseOffB"),
                halve_a: at("uHalveA"),
                halve_b: at("uHalveB"),
                cols_a: at("uColsA"),
                cols_b: at("uColsB"),
                band8: at("uBand8"),
                geo: at("uGeo"),
                sun: at("uSun"),
                atmos: at("uAtmos"),
            }
        });
        Self {
            shader,
            locations,
            fine: None,
            fine_art: fine::FineArt::default(),
            coarse: None,
            coarse_map: coarse::CoarseMap::default(),
            force_quads: false,
            coarse_used: false,
        }
    }

    /// Whether every cell of the fine window has its art (scene capture waits for it).
    #[cfg(not(target_os = "emscripten"))]
    #[must_use]
    pub const fn settled(&self) -> bool {
        !(self.fine_art.has_pending() || self.coarse_used && self.coarse_map.has_pending())
    }

    /// Whether the polar shader compiled.
    #[cfg(not(target_os = "emscripten"))]
    #[must_use]
    pub const fn shader_ready(&self) -> bool {
        self.shader.is_some()
    }

    /// Tile `tile` of the strip's terrain changed (the game calls this for each tile write of
    /// the seat's body store, `SpaceRenderer::mark_ground_tile`, as the home world's renderer
    /// is told of its own). Its rows repaint on the next sync; a strip whose revision changes with no tile
    /// marked repaints everything over a few frames. Assumes `PolarStrip::tile_at` keeps rows
    /// (band columns may map anywhere).
    pub fn mark_tile_dirty(&mut self, tile: TilePosition) {
        self.fine_art.mark_dirty(tile);
        let reach = tile_art::TELLTALE_RADIUS.max(1);
        self.coarse_map
            .dirty_rows
            .extend(tile.y - reach..=tile.y + reach);
    }

    /// Repaints the windows the camera needs (once per frame, before drawing).
    pub fn sync(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        ground: &GroundView<'_>,
        px_per_tile: f32,
        reach: f32,
    ) {
        let strip = ground.strip;
        let geometry = strip.geometry();
        // Tiles from the craft to the view's farthest corner, plus a window step of slack.
        let needed = (2.0 * reach / px_per_tile.max(0.001)).ceil() as i32 + 2 * WINDOW_STEP;
        // Close to the centre the view takes in a wide angle of the deep rings, more columns
        // than the window holds: the coarse map fills in round it.
        let reach_tiles = reach / px_per_tile.max(0.001);
        let revision = strip.revision();
        let fine = fine_window(ground, geometry, needed);
        self.fine_art
            .sync(strip, &fine, (ground.column, ground.row), revision);
        self.upload_fine(raylib, thread);
        // Cells whose art is not painted yet show the coarse map.
        self.coarse_used = needed > FINE_MAX
            || reach_tiles > geometry.radius_of_row(ground.row).max(1.0) * 0.35
            || self.fine_art.has_pending();
        if self.coarse_used {
            self.sync_coarse(raylib, thread, ground, revision);
        }
    }

    /// Uploads what of the fine art changed this frame.
    fn upload_fine(&mut self, raylib: &mut RaylibHandle, thread: &RaylibThread) {
        let Some(window) = self.fine_art.window.as_ref() else {
            return;
        };
        let size = (window.size.0 * TILE_ART, window.size.1 * TILE_ART);
        let fits = self
            .fine
            .as_ref()
            .is_some_and(|texture| texture.width == size.0 && texture.height == size.1);
        let upload = std::mem::take(&mut self.fine_art.upload);
        match upload {
            _ if !fits => upload_texture(raylib, thread, &mut self.fine, size, &self.fine_art.rgba),
            fine::Upload::Everything => {
                upload_texture(raylib, thread, &mut self.fine, size, &self.fine_art.rgba);
            }
            fine::Upload::Rows(first, last) => {
                if let Some(texture) = self.fine.as_ref() {
                    update_rows(
                        texture,
                        &self.fine_art.rgba,
                        size.0,
                        first * TILE_ART,
                        (last + 1) * TILE_ART,
                    );
                }
            }
            fine::Upload::Nothing => {}
        }
    }

    /// Brings the coarse map up to the camera and the strip's changes, painting what the
    /// frame's budget allows (Wave 13 QA: a moved window keeps what it holds).
    fn sync_coarse(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        ground: &GroundView<'_>,
        revision: u64,
    ) {
        let strip = ground.strip;
        let window = coarse_window(ground, strip.geometry());
        self.coarse_map
            .sync(strip, &window, ground.row, revision, coarse::frame_budget());
        let fits = self.coarse.as_ref().is_some_and(|texture| {
            texture.width == window.size.0 && texture.height == window.size.1
        });
        match std::mem::take(&mut self.coarse_map.upload) {
            _ if !fits => upload_texture(
                raylib,
                thread,
                &mut self.coarse,
                window.size,
                &self.coarse_map.pixels,
            ),
            fine::Upload::Everything => upload_texture(
                raylib,
                thread,
                &mut self.coarse,
                window.size,
                &self.coarse_map.pixels,
            ),
            fine::Upload::Rows(first, last) => {
                if let Some(texture) = self.coarse.as_ref() {
                    update_rows(
                        texture,
                        &self.coarse_map.pixels,
                        window.size.0,
                        first,
                        last + 1,
                    );
                }
            }
            fine::Upload::Nothing => {}
        }
    }

    /// The ring camera for `ground` seen at `px_per_tile` with the craft at `pivot`.
    #[must_use]
    pub fn ring_camera(ground: &GroundView<'_>, px_per_tile: f32, pivot: (f32, f32)) -> RingCamera {
        let geometry = ground.strip.geometry();
        RingCamera {
            column: ground.column,
            row: ground.row,
            radius: geometry.radius_of_row(ground.row),
            roll: ground.roll,
            px_per_tile,
            columns: f64::from(geometry.surface_columns),
            pivot,
        }
    }

    /// Draws the ground over `viewport` (logical points) with the craft at `pivot`. `sun` is
    /// the angle of the body's star from the camera's local up (radians, clockwise); `blend`
    /// fades the whole pass (approaching from space).
    #[allow(clippy::too_many_lines, reason = "the shader's uniforms, one by one")]
    pub fn draw(
        &self,
        ground: &GroundView<'_>,
        camera: &RingCamera,
        viewport: Rectangle,
        sun: f32,
        blend: f32,
        seconds: f32,
    ) {
        let Some(fine) = self.fine.as_ref() else {
            return;
        };
        let Some(window) = self.fine_art.window.as_ref() else {
            return;
        };
        if blend <= 0.0 {
            return;
        }
        let Some(shader) = self.shader.as_ref().filter(|_| !self.force_quads) else {
            self.draw_quads(ground, camera, sun, blend);
            return;
        };
        let geometry = ground.strip.geometry();
        let coarse_used = self.coarse_used;
        let coarse_texture = self.coarse.as_ref().filter(|_| coarse_used).unwrap_or(fine);
        let coarse_window = self
            .coarse_map
            .window
            .as_ref()
            .filter(|_| coarse_used)
            .cloned()
            .unwrap_or_default();
        // Per band (0..=8): the camera's offset into each window, and the ring's columns.
        let offsets = |window: &Window| -> [f32; MAX_HALVINGS + 1] {
            let mut out = [0.0; MAX_HALVINGS + 1];
            for (band, slot) in out.iter_mut().enumerate() {
                let band_column = geometry.band_column(band as u32, camera.column);
                *slot = (band_column - window.origins[band] as f64) as f32;
            }
            out
        };
        let fine_off = offsets(window);
        let coarse_off = if coarse_window.size.0 > 0 {
            offsets(&coarse_window)
        } else {
            // Nothing painted: every coarse lookup lands outside.
            [-1.0e6; MAX_HALVINGS + 1]
        };
        let mut ring_columns = [0.0_f32; MAX_HALVINGS + 1];
        for (band, slot) in ring_columns.iter_mut().enumerate() {
            *slot = geometry.columns_in_band(band as u32) as f32;
        }
        let mut halve = [1.0e6_f32; 8];
        for (slot, row) in halve.iter_mut().zip(&geometry.halvings) {
            *slot = *row as f32 - camera.row;
        }
        let (body_atmosphere, atmos) = atmosphere(ground);
        let lamp = lamp_radius(camera.px_per_tile);
        let uniforms: [(i32, [f32; 4]); 15] = [
            (
                self.locations.view,
                [
                    viewport.width,
                    viewport.height,
                    camera.pivot.0 - viewport.x,
                    camera.pivot.1 - viewport.y,
                ],
            ),
            (
                self.locations.cam,
                [camera.radius, camera.px_per_tile, camera.roll, 0.0],
            ),
            (
                self.locations.fine,
                [
                    window.size.0 as f32,
                    window.size.1 as f32,
                    camera.row - window.row0 as f32,
                    0.0,
                ],
            ),
            (self.locations.fine_a, quad(&fine_off, 0)),
            (self.locations.fine_b, quad(&fine_off, 4)),
            (
                self.locations.coarse_info,
                [
                    coarse_window.size.0.max(1) as f32,
                    coarse_window.size.1.max(1) as f32,
                    camera.row - coarse_window.row0 as f32,
                    if coarse_window.ring { 1.0 } else { 0.0 },
                ],
            ),
            (self.locations.coarse_a, quad(&coarse_off, 0)),
            (self.locations.coarse_b, quad(&coarse_off, 4)),
            (self.locations.halve_a, quad(&halve, 0)),
            (self.locations.halve_b, quad(&halve, 4)),
            (self.locations.cols_a, quad(&ring_columns, 0)),
            (self.locations.cols_b, quad(&ring_columns, 4)),
            (
                self.locations.band8,
                [
                    fine_off[MAX_HALVINGS],
                    coarse_off[MAX_HALVINGS],
                    ring_columns[MAX_HALVINGS],
                    0.0,
                ],
            ),
            (
                self.locations.geo,
                [
                    camera.row - geometry.surface_row as f32,
                    geometry.core_radius_tiles,
                    body_atmosphere,
                    lamp,
                ],
            ),
            (
                self.locations.sun,
                [
                    sun,
                    ground.night_light,
                    if body_atmosphere > 0.0 { 0.92 } else { 0.0 },
                    seconds,
                ],
            ),
        ];
        // SAFETY: uniforms of a live shader with matching types; the shader mode and the
        // coarse texture binding are balanced within this block.
        unsafe {
            for (location, values) in uniforms {
                ffi::SetShaderValue(
                    **shader,
                    location,
                    values.as_ptr().cast(),
                    ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC4 as i32,
                );
            }
            ffi::SetShaderValue(
                **shader,
                self.locations.atmos,
                atmos.as_ptr().cast(),
                ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC3 as i32,
            );
            ffi::BeginShaderMode(**shader);
            ffi::SetShaderValueTexture(**shader, self.locations.coarse, **coarse_texture);
            ffi::DrawTexturePro(
                **fine,
                ffi::Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: fine.width as f32,
                    height: fine.height as f32,
                },
                viewport,
                ffi::Vector2 { x: 0.0, y: 0.0 },
                0.0,
                Color::new(255, 255, 255, (blend.clamp(0.0, 1.0) * 255.0) as u8),
            );
            ffi::EndShaderMode();
        }
    }

    /// The fallback: the fine window as curved quads (a few tiles wide each, so the curve
    /// stays smooth), lit per quad.
    pub fn draw_quads(&self, ground: &GroundView<'_>, camera: &RingCamera, sun: f32, blend: f32) {
        const STEP: i32 = 4;
        let (Some(fine), Some(window)) = (self.fine.as_ref(), self.fine_art.window.as_ref()) else {
            return;
        };
        let geometry = ground.strip.geometry();
        let alpha = (blend.clamp(0.0, 1.0) * 255.0) as u8;
        // SAFETY: an immediate-mode batch on the live fine texture, closed in this block.
        unsafe {
            ffi::rlSetTexture(fine.id);
            ffi::rlBegin(ffi::RL_QUADS as i32);
            for y in 0..window.size.1 {
                let row = window.row0 + y;
                let band = geometry.band(row);
                let origin = window.origins[band as usize] as f64;
                let depth = (row - geometry.surface_row) as f32;
                for x in (0..window.size.0).step_by(STEP as usize) {
                    let (c0, c1) = (
                        geometry.surface_column(band, origin + f64::from(x)),
                        geometry.surface_column(band, origin + f64::from(x + STEP)),
                    );
                    let (r0, r1) = (row as f32, (row + 1) as f32);
                    let corners = [
                        camera.place(geometry, c0, r0).0,
                        camera.place(geometry, c0, r1).0,
                        camera.place(geometry, c1, r1).0,
                        camera.place(geometry, c1, r0).0,
                    ];
                    let u0 = x as f32 / window.size.0 as f32;
                    let u1 = (x + STEP) as f32 / window.size.0 as f32;
                    let v0 = y as f32 / window.size.1 as f32;
                    let v1 = (y + 1) as f32 / window.size.1 as f32;
                    let mid = (c0 + c1) * 0.5;
                    let phi = ((mid - camera.column) / camera.columns * TAU) as f32;
                    let day = (phi - sun).cos();
                    let daylight = ground.night_light
                        + (1.0 - ground.night_light) * smoothstep(-0.14, 0.2, day);
                    let buried = smoothstep(1.5, 9.0, depth);
                    let light = daylight + (0.26 - daylight) * buried;
                    let shade = (light * 255.0) as u8;
                    ffi::rlColor4ub(shade, shade, shade, alpha);
                    for ((sx, sy), (u, v)) in
                        corners
                            .into_iter()
                            .zip([(u0, v0), (u0, v1), (u1, v1), (u1, v0)])
                    {
                        ffi::rlTexCoord2f(u, v);
                        ffi::rlVertex2f(sx, sy);
                    }
                }
            }
            ffi::rlEnd();
            ffi::rlSetTexture(0);
        }
    }

    /// Draws what stands on the ground, each in the local up frame of where it is and lit
    /// like the ground there (`sun` as for [`Self::draw`]).
    pub fn draw_things(
        draw: &mut impl RaylibDraw,
        ground: &GroundView<'_>,
        camera: &RingCamera,
        viewport: Rectangle,
        sun: f32,
        seconds: f32,
    ) {
        let geometry = ground.strip.geometry();
        // Nothing off the view is drawn: a thing whose feet are farther outside `viewport`
        // than the largest thing stands (an alien town's building, Wave 12: a dozen tiles)
        // cannot show. A settled body's towns hold dozens of buildings of hundreds of
        // rectangles each, most of them round the far side of the ring.
        let margin = 16.0 * camera.px_per_tile;
        for thing in ground.things {
            let ((x, y), turn) = camera.place(geometry, thing.column, thing.row);
            if x < viewport.x - margin
                || x > viewport.x + viewport.width + margin
                || y < viewport.y - margin
                || y > viewport.y + viewport.height + margin
            {
                continue;
            }
            let lamp_reach = lamp_radius(camera.px_per_tile);
            let lamp = 1.0
                - smoothstep(
                    lamp_reach * 0.45,
                    lamp_reach,
                    (x - camera.pivot.0).hypot(y - camera.pivot.1),
                );
            let phi = turn + camera.roll;
            let daylight = ground.night_light
                + (1.0 - ground.night_light) * smoothstep(-0.14, 0.2, (phi - sun).cos());
            let depth = thing.row - geometry.surface_row as f32;
            let buried = smoothstep(1.5, 9.0, depth);
            // A glowing creature lights itself (Wave 14 finish: a boss's core in its dark
            // lair; the Ashen Sovereign was a shadow outside the craft's lamp).
            let own = match thing.kind {
                GroundThingKind::Enemy { kind, .. } => (kind.light_tiles() / 6.0).min(0.85),
                _ => 0.0,
            };
            let light = (daylight + (0.26 - daylight) * buried)
                .max(lamp * 0.95)
                .max(own);
            // SAFETY: a matrix push balanced by the pop below.
            unsafe {
                ffi::rlPushMatrix();
                ffi::rlTranslatef(x, y, 0.0);
                ffi::rlRotatef(turn.to_degrees(), 0.0, 0.0, 1.0);
            }
            draw_thing(
                draw,
                thing,
                camera.px_per_tile,
                seconds,
                light.clamp(0.0, 1.0),
            );
            // SAFETY: closes the push above.
            unsafe { ffi::rlPopMatrix() };
        }
    }
}

/// The angle of the ground's star from the camera's local up (radians, clockwise), given
/// the direction from the body toward its star in the space frame (`toward`, y down).
#[must_use]
pub fn sun_angle(ground: &GroundView<'_>, toward: (f32, f32)) -> f32 {
    let geometry = ground.strip.geometry();
    let star = toward.0.atan2(-toward.1);
    let camera_angle =
        (ground.column / f64::from(geometry.surface_columns) * TAU) as f32 + ground.body.spin;
    star - camera_angle
}

/// The craft's lamp's reach on screen (points).
fn lamp_radius(px_per_tile: f32) -> f32 {
    (7.0 * px_per_tile).max(60.0)
}

/// `colour` dimmed to `light`.
fn shade(colour: Color, light: f32) -> Color {
    let scale = |channel: u8| (f32::from(channel) * light) as u8;
    Color::new(scale(colour.r), scale(colour.g), scale(colour.b), colour.a)
}

const fn quad(values: &[f32], start: usize) -> [f32; 4] {
    [
        values[start],
        values[start + 1],
        values[start + 2],
        values[start + 3],
    ]
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The sky's scale height (tiles; 0 airless) and its colour.
fn atmosphere(ground: &GroundView<'_>) -> (f32, [f32; 3]) {
    let body = &ground.body;
    let colour = body.face().air;
    let height = if body.atmosphere > 0.0 {
        (body.atmosphere * body.radius_tiles).clamp(18.0, 220.0) * 0.5
    } else {
        0.0
    };
    (
        height,
        [
            f32::from(colour.r) / 255.0,
            f32::from(colour.g) / 255.0,
            f32::from(colour.b) / 255.0,
        ],
    )
}

const fn snap_down(value: i64, step: i64) -> i64 {
    value.div_euclid(step) * step
}

/// The fine window the camera needs.
fn fine_window(ground: &GroundView<'_>, geometry: &PolarGeometry, needed: i32) -> Window {
    let fit = |least: i32| {
        let snapped = (needed + WINDOW_STEP - 1) / WINDOW_STEP * WINDOW_STEP;
        snapped.clamp(least, FINE_MAX)
    };
    let (width, height) = (fit(FINE_TILES.0), fit(FINE_TILES.1));
    let step = i64::from(WINDOW_STEP);
    let row0 = snap_down(ground.row.floor() as i64 - i64::from(height / 2), step) as i32;
    let mut origins = [0_i64; MAX_HALVINGS + 1];
    for (band, origin) in origins.iter_mut().enumerate() {
        let band_column = geometry.band_column(band as u32, ground.column).floor() as i64;
        *origin = snap_down(band_column - i64::from(width / 2), step);
    }
    Window {
        body: ground.body.id,
        row0: row0.max(geometry.surface_row - SKY_ROWS - height),
        size: (width, height),
        origins,
        ring: false,
    }
}

/// The coarse window the camera needs: whole rings of a small world, else a window.
fn coarse_window(ground: &GroundView<'_>, geometry: &PolarGeometry) -> Window {
    let top = geometry.surface_row - SKY_ROWS;
    let bottom = geometry.last_row() + 1;
    let columns = geometry.surface_columns as i32;
    let ring = columns <= COARSE_MAX;
    let width = if ring { columns } else { COARSE_WINDOW.0 };
    let (row0, height) = if bottom - top <= COARSE_WINDOW.0 {
        (top, bottom - top)
    } else {
        let step = 256;
        let row0 = snap_down(
            ground.row.floor() as i64 - i64::from(COARSE_WINDOW.1 / 2),
            step,
        ) as i32;
        (row0.max(top), COARSE_WINDOW.1)
    };
    let mut origins = [0_i64; MAX_HALVINGS + 1];
    if !ring {
        for (band, origin) in origins.iter_mut().enumerate() {
            let band_column = geometry.band_column(band as u32, ground.column).floor() as i64;
            *origin = snap_down(band_column - i64::from(width / 2), 256);
        }
    }
    Window {
        body: ground.body.id,
        row0,
        size: (width, height.max(1)),
        origins,
        ring,
    }
}

/// The terrain tile of band column `column` of `row`, wrapped round the ring.
fn strip_tile(strip: &dyn PolarStrip, column: i64, row: i32) -> Option<TilePosition> {
    let geometry = strip.geometry();
    let band = geometry.band(row);
    let ring = i64::from(geometry.columns_at(row));
    strip.tile_at(band, column.rem_euclid(ring) as u32, row)
}

/// The mean colour of `tile`'s art (`fallback` when it paints nothing opaque).
fn art_mean(terrain: &Terrain, tile: TilePosition, fallback: Color) -> Color {
    let side = TILE_ART as usize;
    let mut art = vec![TRANSPARENT; side * side];
    tile_art::paint_tile(terrain, tile, &mut art, side, (0, 0));
    let (mut sum, mut count) = ([0_u32; 3], 0_u32);
    for value in art.iter().filter(|value| **value != TRANSPARENT) {
        let colour = color_at(*value);
        sum[0] += u32::from(colour.r);
        sum[1] += u32::from(colour.g);
        sum[2] += u32::from(colour.b);
        count += 1;
    }
    if count == 0 {
        return fallback;
    }
    Color::new(
        (sum[0] / count) as u8,
        (sum[1] / count) as u8,
        (sum[2] / count) as u8,
        255,
    )
}

/// Uploads texel rows `first..end` of `pixels` (RGBA, `width` texels a row) into `texture`.
fn update_rows(texture: &Texture2D, pixels: &[u8], width: i32, first: i32, end: i32) {
    let start = (first * width * 4) as usize;
    let Some(rows) = pixels.get(start..(end * width * 4) as usize) else {
        return;
    };
    // SAFETY: a live texture of `width` texels a row; `rows` holds exactly `end - first` rows
    // of RGBA texels, the texture's format.
    unsafe {
        ffi::UpdateTextureRec(
            **texture,
            ffi::Rectangle {
                x: 0.0,
                y: first as f32,
                width: width as f32,
                height: (end - first) as f32,
            },
            rows.as_ptr().cast(),
        );
    }
}

/// Makes `target` a texture of `size` holding `pixels` (point filtered).
fn upload_texture(
    raylib: &mut RaylibHandle,
    thread: &RaylibThread,
    target: &mut Option<Texture2D>,
    size: (i32, i32),
    pixels: &[u8],
) {
    let fits = target
        .as_ref()
        .is_some_and(|texture| texture.width == size.0 && texture.height == size.1);
    if !fits {
        *target = raylib
            .load_texture_from_image(thread, &sprite::blank_image(size.0, size.1))
            .ok();
        if let Some(texture) = target.as_ref() {
            texture.set_texture_filter(thread, TextureFilter::TEXTURE_FILTER_POINT);
        }
    }
    if let Some(texture) = target.as_mut()
        && let Err(error) = texture.update_texture(pixels)
    {
        eprintln!("polar ground: texture update failed: {error}");
    }
}

/// A builder waymark on round ground (Wave 13 story), its feet at the origin: a pylon of the
/// cable's own pale stuff on a plinth with a lamp at its head (teal, warming slowly), a vault
/// door set in the rock for a buried relic, a broken pillar with an amber lamp for a lead's
/// ruin. The glow breathes slowly in brightness only.
fn draw_waymark(
    draw: &mut impl RaylibDraw,
    relic: bool,
    buried: bool,
    art: f32,
    seconds: f32,
    light: f32,
) {
    let px = |value: f32| value * art;
    let rect = |x: f32, y: f32, w: f32, h: f32| Rectangle::new(px(x), px(y), px(w), px(h));
    let breathe = 0.75 + 0.25 * (seconds * 1.3).sin();
    let lamp = if relic { palette::MINT } else { palette::AMBER };
    if buried {
        // A round vault door in the rock, its ring of lamps.
        let middle = Vector2::new(0.0, px(-9.0));
        draw.draw_circle_v(middle, px(16.0), fade(lamp, 0.18 * breathe));
        draw.draw_circle_v(middle, px(10.0), shade(palette::INK, light));
        draw.draw_circle_v(middle, px(9.0), shade(palette::STEEL_DARK, light));
        draw.draw_circle_v(middle, px(7.0), shade(palette::STEEL, light));
        draw.draw_circle_v(middle, px(3.0), shade(palette::GOLD, light));
        for index in 0..8 {
            let angle = index as f32 * std::f32::consts::TAU / 8.0;
            draw.draw_rectangle_rec(
                Rectangle::new(
                    angle.cos() * px(8.0) - px(1.0),
                    angle.sin() * px(8.0) - px(10.0),
                    px(2.0),
                    px(2.0),
                ),
                fade(lamp, 0.6 + 0.4 * breathe),
            );
        }
        return;
    }
    let height = if relic { 44.0 } else { 22.0 };
    let lamp_at = Vector2::new(0.0, px(-height - 9.0));
    // The glow round the lamp first (behind).
    draw.draw_circle_v(lamp_at, px(16.0), fade(lamp, 0.16 * breathe));
    // Plinth, an ink edge, the pillar (lit on its left), cap and lamp.
    draw.draw_rectangle_rec(
        rect(-11.0, -5.0, 22.0, 5.0),
        shade(palette::STONE_DARK, light),
    );
    draw.draw_rectangle_rec(rect(-10.0, -6.0, 20.0, 1.0), shade(palette::STONE, light));
    draw.draw_rectangle_rec(
        rect(-5.0, -6.0 - height, 10.0, height + 1.0),
        shade(palette::INK, light),
    );
    draw.draw_rectangle_rec(
        rect(-4.0, -5.0 - height, 8.0, height),
        shade(palette::BONE, light),
    );
    draw.draw_rectangle_rec(
        rect(1.0, -5.0 - height, 3.0, height),
        shade(palette::STEEL_LIGHT, light),
    );
    draw.draw_rectangle_rec(
        rect(-4.0, -5.0 - height, 1.0, height),
        shade(palette::CREAM, light),
    );
    if relic {
        // Script grooves down the face, and the cap.
        for row in 0..7 {
            draw.draw_rectangle_rec(
                rect(-2.0, -11.0 - row as f32 * 5.0, 3.0, 1.0),
                fade(palette::TEAL, 0.5 + 0.5 * breathe),
            );
        }
        draw.draw_rectangle_rec(
            rect(-7.0, -9.0 - height, 14.0, 4.0),
            shade(palette::STEEL_LIGHT, light),
        );
    } else {
        // A broken top.
        draw.draw_rectangle_rec(
            rect(-4.0, -7.0 - height, 4.0, 2.0),
            shade(palette::BONE, light),
        );
    }
    draw.draw_circle_v(lamp_at, px(3.5), lamp);
    draw.draw_circle_v(
        Vector2::new(px(-1.0), px(-height - 10.0)),
        px(1.2),
        palette::CREAM,
    );
}

/// A frontier world's mark (Wave 12 planets-b) at the origin: a bomb's shadow (a dark ring
/// closing in, reddening as it falls: the telegraph), a glowing bomb, a beam of light, the
/// sand's ripple over a wyrm.
fn draw_frontier_mark(
    draw: &mut impl RaylibDraw,
    mark: super::view::FrontierMark,
    art: f32,
    seconds: f32,
) {
    use super::view::FrontierMark;
    match mark {
        FrontierMark::BombShadow(progress) => {
            let t = f32::from(progress) / 1_000.0;
            let radius = (1.0 + 2.2 * (1.0 - t)) * 8.0 * art;
            draw.draw_ellipse(
                0,
                (-art) as i32,
                radius,
                radius * 0.35,
                fade(palette::INK, 0.25 + 0.4 * t),
            );
            draw.draw_ellipse_lines(
                0,
                (-art) as i32,
                radius * 1.1,
                radius * 0.4,
                fade(palette::RED, 0.4 + 0.5 * t),
            );
        }
        FrontierMark::Bomb => {
            let r = 3.5 * art;
            draw.draw_circle_v(Vector2::new(0.0, -r), r * 1.8, fade(palette::ORANGE, 0.3));
            draw.draw_circle_v(Vector2::new(0.0, -r), r, palette::RED);
            draw.draw_circle_v(Vector2::new(-r * 0.3, -r * 1.3), r * 0.45, palette::GOLD);
        }
        FrontierMark::Beam { across } => {
            let pulse = 0.55 + 0.15 * (seconds * 3.0).sin();
            let tile = 16.0 * art;
            let (w, h) = if across {
                (tile, tile * 0.3)
            } else {
                (tile * 0.3, tile)
            };
            draw.draw_rectangle_rec(
                Rectangle::new(
                    -w * 0.5 - art,
                    -tile * 0.5 - h * 0.5 - art,
                    w + 2.0 * art,
                    h + 2.0 * art,
                ),
                fade(palette::GOLD, 0.25 * pulse),
            );
            draw.draw_rectangle_rec(
                Rectangle::new(-w * 0.5, -tile * 0.5 - h * 0.5, w, h),
                fade(palette::CREAM, pulse),
            );
        }
        FrontierMark::Ripple => {
            for ring in 0..3_u8 {
                let phase = (seconds * 1.5 + f32::from(ring) / 3.0).fract();
                let radius = (4.0 + 18.0 * phase) * art;
                draw.draw_ellipse_lines(
                    0,
                    (-art) as i32,
                    radius,
                    radius * 0.25,
                    fade(palette::SAND, 1.0 - phase),
                );
            }
        }
    }
}

/// An outland world's mark (Wave 13 planets-c) at the origin (its feet): a ghost light's soft
/// glow drifting (a slow pulse, never a flash), a vent's violet swell growing at its mouth (the
/// telegraph), its blast of spores up the chimney, a turning gear's sparks.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "screen-space marks"
)]
fn draw_outland_mark(
    draw: &mut impl RaylibDraw,
    mark: super::view::OutlandMark,
    art: f32,
    seconds: f32,
) {
    use super::view::OutlandMark;
    let tile = 16.0 * art;
    match mark {
        OutlandMark::Wisp(id) => {
            let phase = f32::from(id % 97) * 0.13;
            let pulse = 0.75 + 0.25 * (seconds * 1.3 + phase).sin();
            let centre = Vector2::new(0.0, -tile * 0.5);
            draw.draw_circle_v(centre, tile * 1.8, fade(palette::MINT, 0.06 * pulse));
            draw.draw_circle_v(centre, tile * 0.9, fade(palette::MINT, 0.12 * pulse));
            draw.draw_circle_v(centre, tile * 0.55, fade(palette::MINT, 0.22 * pulse));
            draw.draw_circle_v(centre, tile * 0.3, fade(palette::ICE, 0.7 * pulse));
            // A tail of fainter motes behind it.
            for trail in 1..4_u8 {
                let t = f32::from(trail);
                let offset = Vector2::new(
                    -t * 3.0 * art * (seconds * 0.7 + phase).cos(),
                    -tile * 0.5 + t * 2.5 * art,
                );
                draw.draw_circle_v(offset, art * (3.0 - t * 0.6), fade(palette::MINT, 0.4 / t));
            }
        }
        OutlandMark::VentSwell(progress) => {
            let t = f32::from(progress) / 1_000.0;
            let radius = (0.3 + 0.7 * t) * tile * 0.8;
            draw.draw_circle_v(
                Vector2::new(0.0, -tile * 0.5),
                radius,
                fade(palette::VIOLET, 0.25 + 0.35 * t),
            );
            draw.draw_circle_v(
                Vector2::new(0.0, -tile * 0.5),
                radius * 0.5,
                fade(palette::PINK, 0.3 + 0.4 * t),
            );
            // Motes rising from the mouth.
            for mote in 0..4_u8 {
                let rise = (seconds * 1.2 + f32::from(mote) * 0.25).fract();
                draw.draw_circle_v(
                    Vector2::new(
                        (f32::from(mote) - 1.5) * 3.0 * art,
                        -tile * (0.5 + rise * 1.5),
                    ),
                    art * 1.5,
                    fade(palette::LAVENDER, (1.0 - rise) * t),
                );
            }
        }
        OutlandMark::VentBlast(progress) => {
            let t = f32::from(progress) / 1_000.0;
            let strength = (1.0 - t).max(0.2);
            for puff in 0..3_u8 {
                let wobble = (seconds * 6.0 + f32::from(puff) * 2.1).sin() * 4.0 * art;
                let rise = (seconds * 3.0 + f32::from(puff) / 3.0).fract();
                draw.draw_circle_v(
                    Vector2::new(wobble, -tile * rise),
                    tile * 0.5,
                    fade(palette::LAVENDER, 0.42 * strength),
                );
                draw.draw_circle_v(
                    Vector2::new(wobble * 0.5, -tile * rise),
                    tile * 0.22,
                    fade(palette::PINK, 0.55 * strength),
                );
            }
        }
        OutlandMark::GearSpin => {
            // Sparks thrown off the turning teeth, round the gear's top.
            for spark in 0..3_u8 {
                let angle = seconds * 5.0 + f32::from(spark) * 2.1;
                let reach = tile * 0.6;
                let at = Vector2::new(angle.cos() * reach, -tile * 0.2 - angle.sin().abs() * reach);
                draw.draw_circle_v(at, art * 1.2, fade(palette::AMBER, 0.85));
                draw.draw_circle_v(at, art * 2.6, fade(palette::ORANGE, 0.25));
            }
        }
    }
}

/// An exotic world's mark (Wave 13 planets-d) at the origin (its foot; up is -y). Tells glow
/// steadily and pulse slowly (well under 3 Hz, never a strobe); a strike is one brief bright
/// line; a shadow darkens and closes in as its stone falls.
#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    reason = "one block per mark, drawn in local pixels"
)]
fn draw_exotic_mark(
    draw: &mut impl RaylibDraw,
    mark: super::view::ExoticMark,
    art: f32,
    seconds: f32,
) {
    use super::view::ExoticMark;
    let tile = 16.0 * art;
    let pulse = 0.5 + 0.5 * (seconds * std::f32::consts::TAU * 1.2).sin();
    match mark {
        ExoticMark::BoltTell { rows } => {
            let height = f32::from(rows) * tile;
            // A faint charged column with sparks crawling up it.
            draw.draw_rectangle_rec(
                Rectangle::new(-tile * 1.5, -height, tile * 3.0, height),
                fade(palette::LAVENDER, 0.08 + 0.1 * pulse),
            );
            for spark in 0..6_u16 {
                let t = ((seconds * 0.7 + f32::from(spark) / 6.0).fract()) * height;
                let wobble = ((f32::from(spark) * 1.7 + seconds * 2.0).sin()) * tile * 0.6;
                draw.draw_circle_v(
                    Vector2::new(wobble, -t),
                    art * 1.5,
                    fade(palette::ICE, 0.5 + 0.3 * pulse),
                );
            }
        }
        ExoticMark::Bolt { rows } => {
            let height = f32::from(rows) * tile;
            let steps = (rows / 3).max(2);
            let mut previous = Vector2::new(0.0, 0.0);
            for step in 1..=steps {
                let y = -height * f32::from(step) / f32::from(steps);
                let x = if step == steps {
                    0.0
                } else if step % 2 == 0 {
                    tile * 0.6
                } else {
                    -tile * 0.6
                };
                let next = Vector2::new(x, y);
                draw.draw_line_ex(previous, next, art * 5.0, fade(palette::LAVENDER, 0.5));
                draw.draw_line_ex(previous, next, art * 2.0, palette::BONE);
                previous = next;
            }
        }
        ExoticMark::FallShadow { progress, meteor } => {
            let t = f32::from(progress) / 1_000.0;
            let radius = (1.0 + 2.2 * (1.0 - t)) * 8.0 * art;
            let rim = if meteor {
                palette::ORANGE
            } else {
                palette::SKY_LIGHT
            };
            draw.draw_ellipse(
                0,
                (-art) as i32,
                radius,
                radius * 0.35,
                fade(palette::INK, 0.25 + 0.4 * t),
            );
            draw.draw_ellipse_lines(
                0,
                (-art) as i32,
                radius * 1.1,
                radius * 0.4,
                fade(rim, 0.4 + 0.5 * t),
            );
        }
        ExoticMark::Falling { meteor } => {
            let r = 3.0 * art;
            if meteor {
                draw.draw_line_ex(
                    Vector2::new(0.0, -r),
                    Vector2::new(r * 2.0, -r * 7.0),
                    r,
                    fade(palette::AMBER, 0.4),
                );
                draw.draw_circle_v(Vector2::new(0.0, -r), r, palette::STEEL);
                draw.draw_circle_v(Vector2::new(-r * 0.3, -r * 1.3), r * 0.4, palette::GOLD);
            } else {
                draw.draw_poly(Vector2::new(0.0, -r), 4, r * 1.2, 45.0, palette::ICE);
                draw.draw_poly_lines(Vector2::new(0.0, -r), 4, r * 1.2, 45.0, palette::SKY);
            }
        }
        ExoticMark::Plume {
            rows,
            blowing,
            night,
        } => {
            let colour = if night { palette::ICE } else { palette::BONE };
            if blowing {
                let height = f32::from(rows) * tile;
                for puff in 0..8_u16 {
                    let t = ((seconds * 1.4 + f32::from(puff) / 8.0).fract()) * height;
                    let spread = tile * (0.6 + 0.9 * t / height.max(1.0));
                    draw.draw_circle_v(
                        Vector2::new(((f32::from(puff) * 2.3).sin()) * spread * 0.4, -t),
                        spread * 0.6,
                        fade(colour, 0.55 * (1.0 - t / height.max(1.0)) + 0.1),
                    );
                }
            } else {
                for wisp in 0..3_u16 {
                    let t = (seconds * 0.9 + f32::from(wisp) / 3.0).fract();
                    draw.draw_circle_v(
                        Vector2::new(
                            (t * 6.0 + f32::from(wisp)).sin() * art * 3.0,
                            -t * tile * 1.5,
                        ),
                        art * 2.5,
                        fade(colour, 0.5 * (1.0 - t)),
                    );
                }
            }
        }
        ExoticMark::BossColumn { rows, strike } => {
            let height = f32::from(rows) * tile;
            let (body, edge) = if strike {
                (fade(palette::BONE, 0.7), palette::GOLD)
            } else {
                (
                    fade(palette::GOLD, 0.12 + 0.12 * pulse),
                    fade(palette::GOLD, 0.6),
                )
            };
            draw.draw_rectangle_rec(
                Rectangle::new(-tile * 1.5, -height, tile * 3.0, height),
                body,
            );
            draw.draw_rectangle_lines_ex(
                Rectangle::new(-tile * 1.5, -height, tile * 3.0, height),
                art,
                edge,
            );
        }
        ExoticMark::BossRow { columns, strike } => {
            let width = f32::from(columns) * tile;
            let (body, edge) = if strike {
                (fade(palette::MINT, 0.75), palette::BONE)
            } else {
                (
                    fade(palette::MINT, 0.12 + 0.12 * pulse),
                    fade(palette::MINT, 0.6),
                )
            };
            draw.draw_rectangle_rec(
                Rectangle::new(-tile * 0.5, -tile * 2.0, width, tile * 3.0),
                body,
            );
            draw.draw_rectangle_lines_ex(
                Rectangle::new(-tile * 0.5, -tile * 2.0, width, tile * 3.0),
                art,
                edge,
            );
        }
        ExoticMark::BossFloor { columns, strike } => {
            let width = f32::from(columns) * tile;
            let (body, edge) = if strike {
                (fade(palette::ORANGE, 0.75), palette::GOLD)
            } else {
                (
                    fade(palette::ORANGE, 0.12 + 0.12 * pulse),
                    fade(palette::ORANGE, 0.6),
                )
            };
            draw.draw_rectangle_rec(
                Rectangle::new(-tile * 0.5, -tile * 2.0, width, tile * 2.0),
                body,
            );
            draw.draw_rectangle_lines_ex(
                Rectangle::new(-tile * 0.5, -tile * 2.0, width, tile * 2.0),
                art,
                edge,
            );
        }
        ExoticMark::Gale => {
            for ring in 0..3_u8 {
                let phase = (seconds * 0.8 + f32::from(ring) / 3.0).fract();
                let radius = (3.0 + 12.0 * phase) * tile * 0.5;
                draw.draw_circle_lines(0, (-tile) as i32, radius, fade(palette::MIST, 1.0 - phase));
            }
        }
    }
}

/// `colour` at opacity `alpha` (0-1).
fn fade(colour: Color, alpha: f32) -> Color {
    #[allow(clippy::cast_sign_loss, reason = "clamped to 0..=255")]
    let alpha = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    palette::with_alpha(colour, alpha)
}

/// One thing on the ground, drawn with its feet at the origin, upright in its own frame.
#[allow(clippy::too_many_lines, reason = "one arm per kind of thing")]
fn draw_thing(
    draw: &mut impl RaylibDraw,
    thing: &GroundThing,
    px_per_tile: f32,
    seconds: f32,
    light: f32,
) {
    use crate::rendering::pixel_art::sprites;
    // One art pixel: a sixteenth of a tile.
    let art = px_per_tile / TILE_ART as f32;
    match thing.kind {
        GroundThingKind::Pilot => {
            let look = crate::rendering::pilot_art::PilotLook::new(
                fatherload_simulation::RigCosmetics::default_for(
                    fatherload_simulation::PlayerId::new(1),
                ),
                fatherload_simulation::pilot::PilotGear::default(),
            );
            let frame = crate::rendering::pilot_art::PilotFrame {
                pose: fatherload_simulation::pilot::PilotPose::Idle,
                seconds,
                swinging: false,
                arms: crate::rendering::pilot_art::PilotArms::NONE,
                low_gravity: true,
                downed: false,
            };
            let canvas = crate::rendering::pilot_art::paint_pilot(&look, frame);
            canvas.draw(draw, Vector2::zero(), art, false, None);
            if light < 0.98 {
                // The dark laid over the suit (the canvas tints every colour but its ink).
                let dark = Color::new(4, 4, 12, ((1.0 - light) * 255.0) as u8);
                canvas.draw(draw, Vector2::zero(), art, false, Some(dark));
            }
        }
        GroundThingKind::Creature(index) => {
            let list = [
                &sprites::enemies::SHELLBACK,
                &sprites::enemies::BURROWER,
                &sprites::enemies::SLATE_SKINK,
            ];
            let sprite = list[usize::from(index) % list.len()];
            let frame = sprite.frame_at(seconds);
            let (w, h) = (sprite.width() as f32 * art, sprite.height() as f32 * art);
            let tint = shade(Color::WHITE, light);
            sprite::draw_sprite(draw, sprite, frame, Vector2::new(-w * 0.5, -h), art, tint);
        }
        GroundThingKind::Halo { radius, colour } => {
            // Rings of light, fainter outward (no flicker: a lamp burns steady).
            let reach = f32::from(radius) * px_per_tile;
            let (r, g, b) = colour;
            for step in 0..7 {
                let share = 1.0 - step as f32 / 7.0;
                draw.draw_circle_v(
                    Vector2::zero(),
                    reach * share,
                    Color::new(r, g, b, (6.0 + 8.0 * (1.0 - share)) as u8),
                );
            }
        }
        GroundThingKind::Enemy { kind, frame, left } => {
            let sprite = crate::rendering::world::enemy_sprite(kind);
            let scale = art * crate::rendering::world::enemy_art_scale(kind);
            let (w, h) = (
                sprite.width() as f32 * scale,
                sprite.height() as f32 * scale,
            );
            let tint = shade(Color::WHITE, light);
            sprite::draw_sprite_flipped(
                draw,
                sprite,
                usize::from(frame),
                Vector2::new(-w * 0.5, -h),
                scale,
                left && crate::rendering::world::enemy_faces_right(kind),
                tint,
            );
        }
        GroundThingKind::Drop => {
            let size = 6.0 * art;
            draw.draw_rectangle_rec(
                Rectangle::new(-size * 0.5, -size, size, size),
                shade(palette::GOLD, light),
            );
            draw.draw_rectangle_rec(
                Rectangle::new(-size * 0.5, -size, size * 0.5, size * 0.5),
                shade(palette::CREAM, light),
            );
            draw.draw_rectangle_rec(
                Rectangle::new(0.0, -size * 0.5, size * 0.5, size * 0.5),
                shade(palette::AMBER, light),
            );
        }
        GroundThingKind::Frontier(mark) => draw_frontier_mark(draw, mark, art, seconds),
        GroundThingKind::Outland(mark) => draw_outland_mark(draw, mark, art, seconds),
        GroundThingKind::Exotic(mark) => draw_exotic_mark(draw, mark, art, seconds),
        GroundThingKind::AlienFolk {
            character,
            facing,
            stride,
            talking,
        } => {
            let Some(found) = fatherload_simulation::alien::town_by_id(character.village) else {
                return;
            };
            let Some(person) = found
                .population
                .characters
                .get(usize::from(character.index))
            else {
                return;
            };
            let pose = crate::rendering::people_art::Pose {
                facing: i32::from(facing),
                stride: stride.map(f32::from),
                talking,
            };
            let pen = crate::rendering::alien_art::ArtPen::new(0.0, 0.0, art, light);
            crate::rendering::alien_art::draw_alien(
                draw,
                pen,
                person,
                found.species,
                pose,
                seconds,
            );
        }
        GroundThingKind::AlienBuilding {
            species,
            kind,
            width,
            height,
            look,
        } => {
            let pen = crate::rendering::alien_art::ArtPen::new(0.0, 0.0, art, light);
            crate::rendering::alien_art::draw_building(
                draw, pen, species, kind, width, height, look, seconds,
            );
        }
        GroundThingKind::Waymark { relic, buried } => {
            draw_waymark(draw, relic, buried, art, seconds, light);
        }
        GroundThingKind::Raider { kind, frame, left } => {
            let Some(sprite) = crate::rendering::hostiles_view::trooper_sprite(kind) else {
                return;
            };
            let (w, h) = (sprite.width() as f32 * art, sprite.height() as f32 * art);
            let tint = shade(Color::WHITE, light);
            sprite::draw_sprite_flipped(
                draw,
                sprite,
                usize::from(frame),
                Vector2::new(-w * 0.5, -h),
                art,
                left,
                tint,
            );
        }
        GroundThingKind::ParkedRig => {
            let look = crate::rendering::rig_art::RigLook {
                player_id: fatherload_simulation::PlayerId::new(2),
                cosmetics: fatherload_simulation::RigCosmetics::default_for(
                    fatherload_simulation::PlayerId::new(2),
                ),
                tiers: crate::rendering::rig_art::RigTiers::all(3),
            };
            let canvas = crate::rendering::rig_art::paint_rig(
                &look,
                &crate::rendering::rig_art::RigPose::idle((seconds * 60.0) as u64),
            );
            let (w, h) = (
                crate::rendering::rig_art::BODY_WIDTH as f32 * art,
                crate::rendering::rig_art::BODY_HEIGHT as f32 * art,
            );
            let origin = Vector2::new(-w * 0.5, -h);
            if light >= 0.98 {
                canvas.draw(draw, origin, art, true);
            } else if let Some((x0, y0, x1, y1)) = canvas.bounds() {
                // Lit cell by cell (mirrored: it faces left).
                let flip = crate::rendering::rig_art::BODY_WIDTH - 1;
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        if let Some(colour) = canvas.get(x, y) {
                            draw.draw_rectangle_rec(
                                Rectangle::new(
                                    origin.x + (flip - x) as f32 * art,
                                    origin.y + y as f32 * art,
                                    art,
                                    art,
                                ),
                                shade(colour, light),
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_point_is_the_pivot_and_columns_run_east() {
        let geometry = PolarGeometry::standard(400.0, 40);
        let camera = RingCamera {
            column: 1_000.25,
            row: 38.0,
            radius: geometry.radius_of_row(38.0),
            roll: 0.0,
            px_per_tile: 32.0,
            columns: f64::from(geometry.surface_columns),
            pivot: (640.0, 360.0),
        };
        let ((x, y), turn) = camera.place(&geometry, 1_000.25, 38.0);
        assert!((x - 640.0).abs() < 1e-3 && (y - 360.0).abs() < 1e-3);
        assert!(turn.abs() < 1e-6);
        // Ten columns east, on the surface: right of and below the pivot (the horizon drops),
        // tilted clockwise.
        let ((x, y), turn) = camera.place(&geometry, 1_010.25, 40.0);
        assert!(x > 640.0 && y > 360.0 && turn > 0.0);
    }

    #[test]
    fn a_small_world_shows_its_curve_at_the_horizon() {
        // The plan's table: R = 400 drops about 16 px over 20 tiles at 32 px per tile.
        let geometry = PolarGeometry::standard(400.0, 40);
        let camera = RingCamera {
            column: 500.0,
            row: 40.0,
            radius: geometry.radius_of_row(40.0),
            roll: 0.0,
            px_per_tile: 32.0,
            columns: f64::from(geometry.surface_columns),
            pivot: (0.0, 0.0),
        };
        let columns_per_tile = f64::from(geometry.surface_columns) / (TAU * 400.0);
        let ((_, drop), _) = camera.place(&geometry, 500.0 + 20.0 * columns_per_tile, 40.0);
        assert!((drop - 16.0).abs() < 1.0, "{drop}");
        let big = PolarGeometry::standard(5_000.0, 40);
        let camera = RingCamera {
            radius: big.radius_of_row(40.0),
            columns: f64::from(big.surface_columns),
            ..camera
        };
        let columns_per_tile = f64::from(big.surface_columns) / (TAU * 5_000.0);
        let ((_, drop), _) = camera.place(&big, 500.0 + 20.0 * columns_per_tile, 40.0);
        assert!((drop - 1.3).abs() < 0.2, "{drop}");
    }

    #[test]
    fn a_roll_turns_the_world_the_other_way() {
        let geometry = PolarGeometry::standard(1_500.0, 40);
        let camera = RingCamera {
            column: 0.0,
            row: 30.0,
            radius: geometry.radius_of_row(30.0),
            roll: std::f32::consts::FRAC_PI_2,
            px_per_tile: 10.0,
            columns: f64::from(geometry.surface_columns),
            pivot: (0.0, 0.0),
        };
        // The craft turned a quarter clockwise: its right now points down, so the ground below
        // it is on the screen's right.
        let ((x, y), _) = camera.place(&geometry, 0.0, 40.0);
        assert!(x > 90.0 && y.abs() < 1.0, "{x} {y}");
    }
}
