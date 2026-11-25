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

use fatherload_simulation::terrain::TilePosition;

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
#[derive(Clone, Debug, Default, PartialEq)]
struct Window {
    body: u64,
    revision: u64,
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
    geo: i32,
    sun: i32,
    atmos: i32,
}

/// The polar ground pass: its shader and the painted windows.
pub struct PolarGround {
    shader: Option<Shader>,
    locations: Locations,
    fine: Option<Texture2D>,
    fine_window: Option<Window>,
    coarse: Option<Texture2D>,
    coarse_window: Option<Window>,
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
                geo: at("uGeo"),
                sun: at("uSun"),
                atmos: at("uAtmos"),
            }
        });
        Self {
            shader,
            locations,
            fine: None,
            fine_window: None,
            coarse: None,
            coarse_window: None,
            force_quads: false,
            coarse_used: false,
        }
    }

    /// Whether the polar shader compiled.
    #[must_use]
    pub const fn shader_ready(&self) -> bool {
        self.shader.is_some()
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
        self.coarse_used =
            needed > FINE_MAX || reach_tiles > geometry.radius_of_row(ground.row).max(1.0) * 0.35;
        let fine = fine_window(ground, geometry, strip.revision(), needed);
        if self.fine_window.as_ref() != Some(&fine) {
            let pixels = paint_fine(strip, &fine);
            let size = (fine.size.0 * TILE_ART, fine.size.1 * TILE_ART);
            upload(raylib, thread, &mut self.fine, size, &pixels);
            self.fine_window = Some(fine);
        }
        if self.coarse_used {
            let coarse = coarse_window(ground, geometry, strip.revision());
            if self.coarse_window.as_ref() != Some(&coarse) {
                let pixels = paint_coarse(strip, &coarse);
                upload(raylib, thread, &mut self.coarse, coarse.size, &pixels);
                self.coarse_window = Some(coarse);
            }
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
        let Some(window) = self.fine_window.as_ref() else {
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
            .coarse_window
            .as_ref()
            .filter(|_| coarse_used)
            .cloned()
            .unwrap_or_default();
        let offsets = |window: &Window| -> [f32; 8] {
            let mut out = [0.0; 8];
            for (band, slot) in out.iter_mut().enumerate() {
                let band_column = camera.column / f64::from(1_u32 << band);
                *slot = (band_column - window.origins[band.min(MAX_HALVINGS)] as f64) as f32;
            }
            out
        };
        let fine_off = offsets(window);
        let coarse_off = if coarse_window.size.0 > 0 {
            offsets(&coarse_window)
        } else {
            // Nothing painted: every coarse lookup lands outside.
            [-1.0e6; 8]
        };
        let mut halve = [1.0e6_f32; 8];
        for (slot, row) in halve.iter_mut().zip(&geometry.halvings) {
            *slot = *row as f32 - camera.row;
        }
        let (body_atmosphere, atmos) = atmosphere(ground);
        let lamp = lamp_radius(camera.px_per_tile);
        let uniforms: [(i32, [f32; 4]); 12] = [
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
                [
                    camera.radius,
                    camera.px_per_tile,
                    camera.roll,
                    (camera.columns / TAU) as f32,
                ],
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
                    if coarse_window.ring {
                        camera.columns as f32
                    } else {
                        0.0
                    },
                ],
            ),
            (self.locations.coarse_a, quad(&coarse_off, 0)),
            (self.locations.coarse_b, quad(&coarse_off, 4)),
            (self.locations.halve_a, quad(&halve, 0)),
            (self.locations.halve_b, quad(&halve, 4)),
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
        let (Some(fine), Some(window)) = (self.fine.as_ref(), self.fine_window.as_ref()) else {
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
                let scale = f64::from(1_u32 << band);
                let origin = window.origins[band as usize] as f64;
                let depth = (row - geometry.surface_row) as f32;
                for x in (0..window.size.0).step_by(STEP as usize) {
                    let (c0, c1) = (
                        (origin + f64::from(x)) * scale,
                        (origin + f64::from(x + STEP)) * scale,
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
        sun: f32,
        seconds: f32,
    ) {
        let geometry = ground.strip.geometry();
        for thing in ground.things {
            let ((x, y), turn) = camera.place(geometry, thing.column, thing.row);
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
            let light = (daylight + (0.26 - daylight) * buried).max(lamp * 0.95);
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

const fn quad(values: &[f32; 8], start: usize) -> [f32; 4] {
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
    let (_, colour) = body.theme.palette();
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
fn fine_window(
    ground: &GroundView<'_>,
    geometry: &PolarGeometry,
    revision: u64,
    needed: i32,
) -> Window {
    let fit = |least: i32| {
        let snapped = (needed + WINDOW_STEP - 1) / WINDOW_STEP * WINDOW_STEP;
        snapped.clamp(least, FINE_MAX)
    };
    let (width, height) = (fit(FINE_TILES.0), fit(FINE_TILES.1));
    let step = i64::from(WINDOW_STEP);
    let row0 = snap_down(ground.row.floor() as i64 - i64::from(height / 2), step) as i32;
    let mut origins = [0_i64; MAX_HALVINGS + 1];
    for (band, origin) in origins.iter_mut().enumerate() {
        let band_column = (ground.column / f64::from(1_u32 << band)).floor() as i64;
        *origin = snap_down(band_column - i64::from(width / 2), step);
    }
    Window {
        body: ground.body.id,
        revision,
        row0: row0.max(geometry.surface_row - SKY_ROWS - height),
        size: (width, height),
        origins,
        ring: false,
    }
}

/// The coarse window the camera needs: whole rings of a small world, else a window.
fn coarse_window(ground: &GroundView<'_>, geometry: &PolarGeometry, revision: u64) -> Window {
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
            let band_column = (ground.column / f64::from(1_u32 << band)).floor() as i64;
            *origin = snap_down(band_column - i64::from(width / 2), 256);
        }
    }
    Window {
        body: ground.body.id,
        // Small rings repaint on every change; big windows only as the camera moves (a dig
        // shows in the fine window at once).
        revision: if ring { revision } else { 0 },
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

/// Paints the fine window: tile art, RGBA, transparent sky.
fn paint_fine(strip: &dyn PolarStrip, window: &Window) -> Vec<u8> {
    let geometry = strip.geometry();
    let terrain = strip.terrain();
    let (width, height) = window.size;
    let stride = (width * TILE_ART) as usize;
    let mut art = vec![TRANSPARENT; stride * (height * TILE_ART) as usize];
    for y in 0..height {
        let row = window.row0 + y;
        if row > geometry.last_row() {
            break;
        }
        let origin = window.origins[geometry.band(row) as usize];
        for x in 0..width {
            if let Some(tile) = strip_tile(strip, origin + i64::from(x), row) {
                tile_art::paint_tile(
                    terrain,
                    tile,
                    &mut art,
                    stride,
                    ((x * TILE_ART) as usize, (y * TILE_ART) as usize),
                );
            }
        }
    }
    let mut pixels = vec![0_u8; art.len() * 4];
    for (index, value) in art.iter().enumerate() {
        if *value != TRANSPARENT {
            let colour = color_at(*value);
            pixels[index * 4..index * 4 + 4]
                .copy_from_slice(&[colour.r, colour.g, colour.b, colour.a]);
        }
    }
    pixels
}

/// Paints the coarse window: one texel per tile, the sky clear, caves dark.
fn paint_coarse(strip: &dyn PolarStrip, window: &Window) -> Vec<u8> {
    let geometry = strip.geometry();
    let terrain = strip.terrain();
    let (width, height) = window.size;
    let mut pixels = vec![0_u8; (width * height * 4) as usize];
    for y in 0..height {
        let row = window.row0 + y;
        if row > geometry.last_row() {
            break;
        }
        let band = geometry.band(row);
        let columns = if window.ring {
            geometry.columns_at(row) as i32
        } else {
            width
        };
        let origin = window.origins[band as usize];
        for x in 0..columns.min(width) {
            let Some(tile) = strip_tile(strip, origin + i64::from(x), row) else {
                continue;
            };
            let Some(found) = terrain.tile(tile) else {
                continue;
            };
            let colour = if found.kind.is_open() {
                if tile.y < terrain.surface_row(tile.x) {
                    continue;
                }
                let back = tile_art::material_color(found.kind, terrain.band_at(tile));
                Color::new(back.r / 2, back.g / 2, back.b / 2, 255)
            } else {
                tile_art::material_color(found.kind, terrain.band_at(tile))
            };
            let at = ((y * width + x) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[colour.r, colour.g, colour.b, 255]);
        }
    }
    pixels
}

/// Makes `target` a texture of `size` holding `pixels` (point filtered).
fn upload(
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

/// One thing on the ground, drawn with its feet at the origin, upright in its own frame.
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
