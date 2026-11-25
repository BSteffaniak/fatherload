#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "screen geometry: f64 world deltas become f32 points, texture sizes are small"
)]

//! Space and round bodies (Wave 11 space-render, expansion plan F12).
//!
//! Draws one seat's [`view::SpaceRenderView`]: the deep starfield in parallax layers (with
//! the galaxy's band, its haze and far galaxies), nebulae, stars by class (glow, spikes and
//! slow flares), round planets and moons ([`bodies`], the planet shader), asteroid belts and
//! single rocks, comets with their two tails, derelicts and stations (readable placeholders),
//! black holes (accretion disc, shadow and a lens over everything behind them), wormholes,
//! and the Star Drive's cruise lines and warp streaks ([`motion`]). Near a body its ground
//! is drawn with the polar shader ([`polar`]): the strip's tiles bent into the ring, with the
//! things on it upright in their local frame.
//!
//! **The camera is locked to the craft**: the craft is drawn upright at the view's centre and
//! the world is turned by `-roll` round it, so the screen turns only when the player turns the
//! craft. Every position is a [`WorldPos`] and is projected through an exact `f64` delta from
//! the camera ([`Projection`]), so nothing depends on how far from home the craft is.
//!
//! Screen effects follow the accessibility setting: the lens's strength and the streaks'
//! length scale with it, and nothing here flashes (every animation is slow and small).

pub mod bodies;
pub mod fixtures;
pub mod live;
pub mod motion;
pub mod objects;
pub mod polar;
pub mod shaders;
pub mod starfield;
pub mod view;

use raylib::{ffi, prelude::*};

use fatherload_simulation::{field::ScreenEffect, position::WorldPos};

use super::pixel_art::{palette, sprite};
use view::{SpaceCamera, SpaceRenderView};

/// The deep sky behind everything.
pub const SPACE_BLACK: Color = Color::new(0x07, 0x06, 0x10, 0xff);

/// Screen projection of one view: the camera's exact position, the turn and the zoom.
#[derive(Clone, Copy, Debug)]
pub struct Projection {
    pub camera: WorldPos,
    /// The craft on screen (points).
    pub pivot: (f32, f32),
    /// The craft's turn (radians, clockwise); the world is drawn turned by `-roll`.
    pub roll: f32,
    sin: f32,
    cos: f32,
    /// Points per world pixel.
    pub scale: f32,
    pub px_per_tile: f32,
    /// The view (points).
    pub bounds: Rectangle,
}

impl Projection {
    #[must_use]
    pub fn new(camera: &SpaceCamera, bounds: Rectangle) -> Self {
        let (sin, cos) = (-camera.roll).sin_cos();
        Self {
            camera: camera.centre,
            pivot: (
                bounds.x + bounds.width * 0.5,
                bounds.y + bounds.height * 0.5,
            ),
            roll: camera.roll,
            sin,
            cos,
            scale: camera.scale(),
            px_per_tile: camera.px_per_tile,
            bounds,
        }
    }

    /// `(x, y)` in the space frame's orientation turned onto the screen.
    #[must_use]
    pub fn turn(&self, (x, y): (f32, f32)) -> (f32, f32) {
        (self.cos * x - self.sin * y, self.sin * x + self.cos * y)
    }

    /// Where `position` is on screen (points). Far points are clamped (still in the right
    /// direction), so a star a light year away never makes a NaN.
    #[must_use]
    pub fn project(&self, position: WorldPos) -> (f32, f32) {
        const LIMIT: f64 = 1.0e7;
        let (dx, dy) = self.camera.delta_f64(position);
        let scale = f64::from(self.scale);
        let (mut x, mut y) = (dx * scale, dy * scale);
        let far = x.abs().max(y.abs());
        if far > LIMIT {
            x *= LIMIT / far;
            y *= LIMIT / far;
        }
        let (x, y) = self.turn((x as f32, y as f32));
        (self.pivot.0 + x, self.pivot.1 + y)
    }

    /// Points for `tiles` tiles.
    #[must_use]
    pub fn tiles(&self, tiles: f32) -> f32 {
        tiles * self.px_per_tile
    }

    /// A direction of the space frame (radians, clockwise from right) on screen.
    #[must_use]
    pub fn screen_angle(&self, angle: f32) -> f32 {
        angle - self.roll
    }

    /// Half the view's diagonal (points): the reach of the view round the pivot at any turn.
    #[must_use]
    pub fn reach(&self) -> f32 {
        self.bounds.width.hypot(self.bounds.height) * 0.5
    }

    /// Whether a circle at `point` of `radius` points can touch the view.
    #[must_use]
    pub fn visible(&self, point: (f32, f32), radius: f32) -> bool {
        point.0 + radius >= self.bounds.x
            && point.0 - radius <= self.bounds.x + self.bounds.width
            && point.1 + radius >= self.bounds.y
            && point.1 - radius <= self.bounds.y + self.bounds.height
    }
}

/// Compiles a fragment shader, or `None` (and a note) when the driver refuses it.
pub(super) fn load_shader(
    raylib: &mut RaylibHandle,
    thread: &RaylibThread,
    source: &str,
    what: &str,
) -> Option<Shader> {
    let shader = raylib.load_shader_from_memory(thread, None, Some(source));
    let default_id = unsafe { ffi::rlGetShaderIdDefault() };
    if shader.id == 0 || shader.id == default_id {
        eprintln!("space: the {what} shader did not compile; using the CPU fallback");
        None
    } else {
        Some(shader)
    }
}

/// Small textures every space layer shares, rasterized at startup.
pub struct SpaceArt {
    /// A soft round glow in a few hard steps (pixel-art halos), white.
    pub glow: Option<Texture2D>,
    /// Three cloud shapes for nebulae and the galaxy's haze, white.
    pub clouds: [Option<Texture2D>; 3],
    /// A white quad for shader passes.
    pub white: Option<Texture2D>,
}

impl SpaceArt {
    fn load(raylib: &mut RaylibHandle, thread: &RaylibThread) -> Self {
        let mut texture = |width: i32, pixels: Vec<u8>| {
            let image = sprite::blank_image(width, width);
            // SAFETY: the blank image is width x width RGBA8; `pixels` has exactly that many
            // bytes and the image owns its buffer for this copy.
            unsafe {
                let data = image.data.cast::<u8>();
                std::ptr::copy_nonoverlapping(pixels.as_ptr(), data, pixels.len());
            }
            let texture = raylib.load_texture_from_image(thread, &image).ok();
            if let Some(texture) = &texture {
                texture.set_texture_filter(thread, TextureFilter::TEXTURE_FILTER_POINT);
            }
            texture
        };
        Self {
            glow: texture(64, starfield::glow_pixels(64)),
            clouds: [
                texture(96, starfield::cloud_pixels(96, 11)),
                texture(96, starfield::cloud_pixels(96, 23)),
                texture(96, starfield::cloud_pixels(96, 37)),
            ],
            white: texture(4, vec![255; 4 * 4 * 4]),
        }
    }
}

/// The space renderer: shaders, shared art and the polar ground's textures.
pub struct SpaceRenderer {
    art: SpaceArt,
    planets: bodies::PlanetShader,
    lens: Option<(Shader, i32, i32)>,
    /// The picture behind a black hole, for its lens (view sized, logical points).
    behind: Option<RenderTexture2D>,
    /// The first seat's ground (and the staged views').
    pub polar: polar::PolarGround,
    /// The other split-screen seats' grounds (made when a seat first stands on a body), so
    /// seats on different bodies keep their own textures.
    seats: Vec<polar::PolarGround>,
}

impl SpaceRenderer {
    pub fn new(raylib: &mut RaylibHandle, thread: &RaylibThread) -> Self {
        let gles = cfg!(target_os = "emscripten");
        let lens = load_shader(
            raylib,
            thread,
            &shaders::fragment_source(gles, shaders::LENS_BODY),
            "lens",
        )
        .map(|shader| {
            let lens = shader.get_shader_location("uLens");
            let size = shader.get_shader_location("uSize");
            (shader, lens, size)
        });
        Self {
            art: SpaceArt::load(raylib, thread),
            planets: bodies::PlanetShader::new(raylib, thread),
            lens,
            behind: None,
            polar: polar::PolarGround::new(raylib, thread),
            seats: Vec::new(),
        }
    }

    /// Seat `seat`'s ground renderer.
    fn ground(&self, seat: usize) -> Option<&polar::PolarGround> {
        if seat == 0 {
            Some(&self.polar)
        } else {
            self.seats.get(seat - 1)
        }
    }

    /// Marks a changed tile of seat `seat`'s body for repainting (Wave 11 integration: the
    /// body store's tile writes, as the home world's renderer is told of its own).
    pub fn mark_ground_tile(
        &mut self,
        seat: usize,
        tile: fatherload_simulation::terrain::TilePosition,
    ) {
        if seat == 0 {
            self.polar.mark_tile_dirty(tile);
        } else if let Some(ground) = self.seats.get_mut(seat - 1) {
            ground.mark_tile_dirty(tile);
        }
    }

    /// Whether every space shader compiled (scene capture reports it).
    #[cfg(not(target_os = "emscripten"))]
    #[must_use]
    pub const fn shaders_ready(&self) -> (bool, bool, bool) {
        (
            self.planets.ready(),
            self.polar.shader_ready(),
            self.lens.is_some(),
        )
    }

    /// Brings the ground's textures and the lens's target up to date (once per frame, before
    /// drawing). `size` is the view's size in points.
    pub fn sync(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        view: &SpaceRenderView<'_>,
        size: (i32, i32),
    ) {
        self.sync_seat(raylib, thread, view, size, 0);
    }

    /// [`Self::sync`] for split-screen seat `seat` (each seat keeps its own ground).
    pub fn sync_seat(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        view: &SpaceRenderView<'_>,
        size: (i32, i32),
        seat: usize,
    ) {
        if let Some(ground) = &view.ground {
            let reach = (size.0 as f32).hypot(size.1 as f32) * 0.5;
            let force_quads = self.polar.force_quads;
            let polar = if seat == 0 {
                &mut self.polar
            } else {
                while self.seats.len() < seat {
                    let mut extra = polar::PolarGround::new(raylib, thread);
                    extra.force_quads = force_quads;
                    self.seats.push(extra);
                }
                &mut self.seats[seat - 1]
            };
            polar.sync(raylib, thread, ground, view.camera.px_per_tile, reach);
        }
        let wants_lens = self.lens.is_some()
            && view.black_holes.iter().any(|hole| {
                let projection = Projection::new(
                    &view.camera,
                    Rectangle::new(0.0, 0.0, size.0 as f32, size.1 as f32),
                );
                let point = projection.project(hole.centre);
                projection.visible(point, projection.tiles(hole.disc_tiles * 3.0))
            });
        if wants_lens {
            let fits = self.behind.as_ref().is_some_and(|target| {
                target.texture.width == size.0 && target.texture.height == size.1
            });
            if !fits {
                self.behind = raylib
                    .load_render_texture(thread, size.0.max(1) as u32, size.1.max(1) as u32)
                    .ok();
            }
        }
    }

    /// Draws `view` over `viewport` (logical points).
    pub fn draw(
        &self,
        draw: &mut RaylibDrawHandle<'_>,
        view: &SpaceRenderView<'_>,
        viewport: Rectangle,
    ) {
        self.draw_seat(draw, view, viewport, 0);
    }

    /// [`Self::draw`] for split-screen seat `seat` (its own ground).
    pub fn draw_seat(
        &self,
        draw: &mut RaylibDrawHandle<'_>,
        view: &SpaceRenderView<'_>,
        viewport: Rectangle,
        seat: usize,
    ) {
        let projection = Projection::new(&view.camera, viewport);
        let lensed = view
            .black_holes
            .iter()
            .map(|hole| (hole, projection.project(hole.centre)))
            .find(|(hole, point)| {
                projection.visible(*point, projection.tiles(hole.disc_tiles * 3.0))
            });
        let lens_strength = view.effects.scale(ScreenEffect::Lensing);
        match (lensed, self.lens.as_ref(), self.behind.as_ref()) {
            (Some((hole, point)), Some((shader, lens_at, size_at)), Some(target))
                if target.texture.width == viewport.width as i32
                    && target.texture.height == viewport.height as i32 =>
            {
                // Everything far is drawn into the lens's target with the view's top-left at
                // the origin, then put on screen through the lens.
                let local = Projection::new(
                    &view.camera,
                    Rectangle::new(0.0, 0.0, viewport.width, viewport.height),
                );
                // SAFETY: a texture pass balanced within this block (the frame's draw handle
                // is not used while it is open).
                unsafe {
                    ffi::BeginTextureMode(**target);
                    ffi::ClearBackground(SPACE_BLACK);
                }
                self.draw_far(draw, view, &local);
                // SAFETY: closes the texture pass above.
                unsafe { ffi::EndTextureMode() };
                let horizon = projection.tiles(hole.horizon_tiles) / viewport.height.max(1.0);
                let lens = [
                    (point.0 - viewport.x) / viewport.width.max(1.0),
                    1.0 - (point.1 - viewport.y) / viewport.height.max(1.0),
                    horizon,
                    lens_strength,
                ];
                let size = [viewport.width, viewport.height];
                // SAFETY: uniforms of a live shader, a balanced shader mode and a live texture.
                unsafe {
                    ffi::SetShaderValue(
                        **shader,
                        *lens_at,
                        lens.as_ptr().cast(),
                        ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC4 as i32,
                    );
                    ffi::SetShaderValue(
                        **shader,
                        *size_at,
                        size.as_ptr().cast(),
                        ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC2 as i32,
                    );
                    ffi::BeginShaderMode(**shader);
                    ffi::DrawTexturePro(
                        target.texture,
                        ffi::Rectangle {
                            x: 0.0,
                            y: 0.0,
                            width: viewport.width,
                            height: -viewport.height,
                        },
                        viewport,
                        ffi::Vector2 { x: 0.0, y: 0.0 },
                        0.0,
                        Color::WHITE,
                    );
                    ffi::EndShaderMode();
                }
            }
            _ => {
                draw.draw_rectangle_rec(viewport, SPACE_BLACK);
                self.draw_far(draw, view, &projection);
                if let Some((hole, point)) = lensed {
                    // No lens shader: the shadow and its photon ring, plainly.
                    objects::draw_black_hole_shadow(draw, hole, point, &projection);
                }
            }
        }
        if lensed.is_some() {
            // The disc's near half passes in front of the shadow.
            objects::draw_black_hole_front(view, &projection);
        }
        if let Some(ground) = &view.ground {
            let camera =
                polar::PolarGround::ring_camera(ground, projection.px_per_tile, projection.pivot);
            let toward = view.light_toward(ground.body.centre);
            let sun = polar::sun_angle(ground, toward);
            let blend = bodies::ground_blend(projection.px_per_tile);
            if let Some(polar) = self.ground(seat) {
                polar.draw(ground, &camera, viewport, sun, blend, view.seconds);
            }
            if blend > 0.5 {
                polar::PolarGround::draw_things(draw, ground, &camera, sun, view.seconds);
            }
        }
        motion::draw_motion(draw, view, &projection);
        objects::draw_nebula_fog(draw, view, &projection);
        if let Some(craft) = view.craft {
            objects::draw_craft(draw, craft, &projection, view.seconds);
        }
    }

    /// Everything that is far from the craft (and bent by a black hole's lens).
    fn draw_far(
        &self,
        draw: &mut RaylibDrawHandle<'_>,
        view: &SpaceRenderView<'_>,
        projection: &Projection,
    ) {
        starfield::draw_sky(draw, &self.art, view, projection);
        objects::draw_nebulae(&self.art, view, projection);
        objects::draw_wormholes(draw, &self.art, view, projection);
        objects::draw_stars(draw, &self.art, view, projection);
        objects::draw_black_hole_discs(&self.art, view, projection);
        objects::draw_belts(draw, view, projection);
        self.planets.draw_bodies(draw, &self.art, view, projection);
        objects::draw_asteroids(draw, view, projection);
        objects::draw_comets(draw, &self.art, view, projection);
        objects::draw_structures(draw, &self.art, view, projection);
    }
}

/// Draws `texture` centred at `centre`, `size` points across, turned `degrees`, tinted.
pub(super) fn draw_centred(
    texture: &Texture2D,
    centre: (f32, f32),
    size: (f32, f32),
    degrees: f32,
    tint: Color,
) {
    // SAFETY: draws a live texture inside the frame.
    unsafe {
        ffi::DrawTexturePro(
            **texture,
            ffi::Rectangle {
                x: 0.0,
                y: 0.0,
                width: texture.width as f32,
                height: texture.height as f32,
            },
            ffi::Rectangle {
                x: centre.0,
                y: centre.1,
                width: size.0,
                height: size.1,
            },
            ffi::Vector2 {
                x: size.0 * 0.5,
                y: size.1 * 0.5,
            },
            degrees,
            tint,
        );
    }
}

/// Runs `body` with additive blending (glows), then back to alpha blending.
pub(super) fn additive(body: impl FnOnce()) {
    // SAFETY: a blend mode balanced within this call.
    unsafe { ffi::BeginBlendMode(ffi::BlendMode::BLEND_ADDITIVE as i32) };
    body();
    // SAFETY: closes the blend mode above.
    unsafe { ffi::EndBlendMode() };
}

/// `colour` at `alpha` (0..=1).
#[must_use]
pub(super) fn fade(colour: Color, alpha: f32) -> Color {
    palette::with_alpha(colour, (alpha.clamp(0.0, 1.0) * f32::from(colour.a)) as u8)
}

/// A well-mixed 64-bit hash of a few integers (deterministic on every platform).
#[must_use]
pub fn hash64(values: &[i64]) -> u64 {
    let mut h: u64 = 0x9e37_79b9_7f4a_7c15;
    for value in values {
        h ^= *value as u64;
        h = h.wrapping_add(0x9e37_79b9_7f4a_7c15);
        h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        h ^= h >> 31;
    }
    h
}

/// A hash as a float in `0..1`, from bits `shift..shift + 16`.
#[must_use]
pub fn unit(hash: u64, shift: u32) -> f32 {
    f32::from(((hash >> shift) & 0xffff) as u16) / 65_536.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_projection_turns_the_world_against_the_craft() {
        let camera = SpaceCamera {
            centre: WorldPos::new(1_000_000_000_000, -7, 100.0, 100.0),
            roll: std::f32::consts::FRAC_PI_2,
            px_per_tile: 32.0,
        };
        let projection = Projection::new(&camera, Rectangle::new(0.0, 0.0, 1280.0, 720.0));
        assert_eq!(projection.project(camera.centre), (640.0, 360.0));
        // A point one tile to the space frame's right: the craft turned clockwise a quarter,
        // so it shows above the craft.
        let right = camera.centre.offset(32.0, 0.0);
        let (x, y) = projection.project(right);
        assert!(
            (x - 640.0).abs() < 1e-3 && (y - 328.0).abs() < 1e-3,
            "{x} {y}"
        );
        // A point a light year away stays finite.
        let far = WorldPos::new(i64::MAX / 4, 0, 0.0, 0.0);
        let (x, y) = projection.project(far);
        assert!(x.is_finite() && y.is_finite());
    }

    #[test]
    fn hashes_are_stable() {
        assert_eq!(hash64(&[1, 2, 3]), hash64(&[1, 2, 3]));
        assert_ne!(hash64(&[1, 2, 3]), hash64(&[3, 2, 1]));
        let u = unit(hash64(&[42]), 8);
        assert!((0.0..1.0).contains(&u));
    }
}
