#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "render target sizes are small whole pixel counts"
)]

//! The render post-process pipeline (expansion plan F9).
//!
//! A view whose seat sees screen effects, a darkened field or a rolled camera is drawn in
//! up to three passes instead of straight to the screen:
//!
//! 1. **World pass**: the seat's world is drawn into its own render texture, at the view's
//!    physical pixel size, or, while the camera rolls with the craft, into a square big
//!    enough to cover the view at any angle, centred on the rig.
//! 2. **Roll pass** (rolled camera only): that square is drawn turned by the rig's
//!    orientation the other way around the rig into a view-sized texture, so the world turns
//!    around the craft and the craft stays upright on screen.
//! 3. **Effect pass**: the view-sized picture is drawn onto the screen through one shader
//!    that runs the stack in order: lensing, wobble / heat haze, pixelation, mirror,
//!    chromatic aberration, bloom, exposure, the field's light, desaturation, colour grade,
//!    palette reduction, fog, scanlines, vignette. Every stage costs nothing at level 0, and
//!    with every level at 0 the pass draws plainly (no shader).
//!
//! The HUD, dialogs and prompts are drawn afterwards on the seat's canvas, so they stay
//! upright and untouched. Each split-screen seat has its own textures, levels and roll. A
//! view with nothing to do (every existing scene) is drawn straight to the screen exactly as
//! before. The shader exists as GLSL 330 (desktop) and GLSL ES 100 (`WebGL1`), generated from
//! one body ([`fragment_source`]); levels are eased and limited by [`crate::screen_fx`].

use raylib::{ffi, prelude::*};

use fatherload_simulation::field::ScreenEffect;

use crate::{
    render_snapshot::{RenderSnapshot, Viewport},
    screen_fx::{ScreenFx, SeatFx},
};

/// Largest render texture side (pixels) the pipeline asks for.
const MAX_TARGET_SIDE: i32 = 4_096;
/// Rolled world textures grow in steps of this many pixels (fewer reallocations as the
/// camera lead moves the pivot).
const ROLL_TARGET_STEP: i32 = 64;

/// One view's plan for this frame (computed in [`PostProcess::prepare`]).
#[derive(Clone, Copy, Debug)]
pub struct ViewPlan {
    pub fx: ScreenFx,
    /// Camera roll (radians, the rig's orientation); 0 when not rolling.
    pub roll: f32,
    /// The rig on screen, in physical pixels from the view's top-left corner.
    pub pivot: (f32, f32),
    /// Size of the world pass texture (physical pixels).
    pub world_size: (i32, i32),
    /// Size of the view (physical pixels).
    pub view_size: (i32, i32),
    /// World pixels per physical pixel's reciprocal: physical pixels per world pixel.
    pub zoom: f32,
    /// One art pixel in physical pixels (pixelation blocks and scanline pitch).
    pub art_pixel: f32,
}

impl ViewPlan {
    /// Whether the camera rolls this frame.
    #[must_use]
    pub fn rolled(&self) -> bool {
        self.roll != 0.0
    }

    /// The world rectangle (world pixels) the world pass covers, given the view's camera
    /// (top-left world pixel) and the rig's world position.
    #[must_use]
    pub fn world_rect(&self, camera: (f32, f32), rig: (f32, f32)) -> Rectangle {
        let width = self.world_size.0 as f32 / self.zoom;
        let height = self.world_size.1 as f32 / self.zoom;
        if self.rolled() {
            Rectangle::new(rig.0 - width * 0.5, rig.1 - height * 0.5, width, height)
        } else {
            Rectangle::new(camera.0, camera.1, width, height)
        }
    }
}

#[derive(Default)]
struct SeatTargets {
    world: Option<RenderTexture2D>,
    view: Option<RenderTexture2D>,
}

/// Uniform locations of the effect shader.
#[derive(Clone, Copy, Debug, Default)]
struct Locations {
    a: i32,
    b: i32,
    c: i32,
    d: i32,
    grade: i32,
    fog: i32,
    size: i32,
}

/// The pipeline: the effect shader and each seat's targets, levels and plan.
pub struct PostProcess {
    shader: Option<Shader>,
    locations: Locations,
    targets: Vec<SeatTargets>,
    fx: Vec<SeatFx>,
    plans: Vec<Option<ViewPlan>>,
}

impl PostProcess {
    /// Compiles the effect shader for this platform (GLSL ES 100 in the browser, 330 on the
    /// desktop). Without it (a driver that refuses it) effects are skipped; the roll still
    /// works.
    pub fn new(raylib: &mut RaylibHandle, thread: &RaylibThread) -> Self {
        let source = fragment_source(cfg!(target_os = "emscripten"));
        let shader = raylib.load_shader_from_memory(thread, None, Some(&source));
        let default_id = unsafe { ffi::rlGetShaderIdDefault() };
        let (shader, locations) = if shader.id == 0 || shader.id == default_id {
            eprintln!("post-process: the effect shader did not compile; screen effects are off");
            (None, Locations::default())
        } else {
            let location = |name: &str| shader.get_shader_location(name);
            let locations = Locations {
                a: location("uA"),
                b: location("uB"),
                c: location("uC"),
                d: location("uD"),
                grade: location("uGrade"),
                fog: location("uFogColor"),
                size: location("uSize"),
            };
            (Some(shader), locations)
        };
        Self {
            shader,
            locations,
            targets: Vec::new(),
            fx: Vec::new(),
            plans: Vec::new(),
        }
    }

    /// Whether the effect shader compiled (scene capture reports it).
    #[must_use]
    pub const fn shader_ready(&self) -> bool {
        self.shader.is_some()
    }

    /// Forgets every seat's eased levels (the next frame shows its targets at once).
    pub fn reset(&mut self) {
        for seat in &mut self.fx {
            seat.reset();
        }
    }

    /// The plan for `seat` this frame, if its view goes through the pipeline.
    #[must_use]
    pub fn plan(&self, seat: usize) -> Option<ViewPlan> {
        self.plans.get(seat).copied().flatten()
    }

    /// Eases every seat's levels, plans its passes and makes sure its render textures exist
    /// at the right sizes (once per frame, before drawing).
    pub fn prepare(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        snapshot: &RenderSnapshot<'_>,
    ) {
        let density = super::pixel_density(
            (raylib.get_render_width(), raylib.get_render_height()),
            (raylib.get_screen_width(), raylib.get_screen_height()),
        );
        let dt = raylib.get_frame_time();
        let seats = snapshot
            .views
            .iter()
            .map(|view| view.seat_index + 1)
            .max()
            .unwrap_or(0);
        if self.fx.len() < seats {
            self.fx.resize_with(seats, SeatFx::default);
            self.targets.resize_with(seats, SeatTargets::default);
        }
        self.plans.clear();
        self.plans.resize(seats, None);
        for view in &snapshot.views {
            let seat = view.seat_index;
            let eased = self.fx[seat].update(&view.fields, snapshot.effects, dt);
            let fx = if self.shader.is_none() {
                ScreenFx::NONE
            } else {
                eased
            };
            let roll = view.roll;
            if fx.is_idle() && roll == 0.0 {
                continue;
            }
            let zoom = view.world_scale * density;
            let view_size = physical(view.viewport, density);
            let rig = snapshot
                .players
                .iter()
                .find(|player| player.player_id == view.player_id)
                .map_or((view.camera.x, view.camera.y), |player| {
                    (player.x, player.y)
                });
            let pivot = (
                (rig.0 - view.camera.x) * zoom,
                (rig.1 - view.camera.y) * zoom,
            );
            let world_size = if roll == 0.0 {
                view_size
            } else {
                let reach = [
                    (0.0, 0.0),
                    (view_size.0 as f32, 0.0),
                    (0.0, view_size.1 as f32),
                    (view_size.0 as f32, view_size.1 as f32),
                ]
                .into_iter()
                .map(|(x, y)| (x - pivot.0).hypot(y - pivot.1))
                .fold(0.0_f32, f32::max);
                let needed = (reach * 2.0).ceil() as i32 + 2;
                let side = ((needed + ROLL_TARGET_STEP - 1) / ROLL_TARGET_STEP * ROLL_TARGET_STEP)
                    .min(MAX_TARGET_SIDE);
                (side, side)
            };
            let targets = &mut self.targets[seat];
            ensure_target(raylib, thread, &mut targets.world, world_size);
            if roll != 0.0 {
                ensure_target(raylib, thread, &mut targets.view, view_size);
            }
            let ready = targets.world.is_some() && (roll == 0.0 || targets.view.is_some());
            if !ready {
                continue;
            }
            self.plans[seat] = Some(ViewPlan {
                fx,
                roll,
                pivot,
                world_size,
                view_size,
                zoom,
                art_pixel: crate::pixel_scale::WORLD_PIXELS_PER_ART_PIXEL * zoom,
            });
        }
    }

    /// Starts the world pass of `seat` (the world is then drawn with a camera whose origin is
    /// the texture's top-left corner).
    pub fn begin_world(&self, seat: usize) {
        if let Some(target) = self.targets.get(seat).and_then(|t| t.world.as_ref()) {
            // SAFETY: balanced by `end_world` within the same frame; the target outlives it.
            unsafe {
                ffi::BeginTextureMode(**target);
                ffi::ClearBackground(super::palette::SKY_LIGHT);
            }
        }
    }

    /// Ends the world pass.
    pub fn end_world() {
        // SAFETY: closes the texture mode opened by `begin_world`.
        unsafe { ffi::EndTextureMode() };
    }

    /// The roll pass (when rolled) and the effect pass: puts `seat`'s picture on the screen
    /// over `viewport` (logical points).
    pub fn composite(&self, seat: usize, plan: &ViewPlan, viewport: Viewport) {
        let Some(targets) = self.targets.get(seat) else {
            return;
        };
        let Some(world) = targets.world.as_ref() else {
            return;
        };
        let picture = if plan.rolled() {
            let Some(view) = targets.view.as_ref() else {
                return;
            };
            let side = plan.world_size.0 as f32;
            // SAFETY: a texture pass balanced within this block.
            unsafe {
                ffi::BeginTextureMode(**view);
                ffi::ClearBackground(super::palette::SKY_LIGHT);
                ffi::DrawTexturePro(
                    world.texture,
                    ffi::Rectangle {
                        x: 0.0,
                        y: 0.0,
                        width: side,
                        height: -side,
                    },
                    ffi::Rectangle {
                        x: plan.pivot.0,
                        y: plan.pivot.1,
                        width: side,
                        height: side,
                    },
                    ffi::Vector2 {
                        x: side * 0.5,
                        y: side * 0.5,
                    },
                    -plan.roll.to_degrees(),
                    Color::WHITE,
                );
                ffi::EndTextureMode();
            }
            view
        } else {
            world
        };
        let shaded = !plan.fx.is_idle();
        if shaded {
            self.apply_uniforms(plan);
        }
        let (width, height) = (plan.view_size.0 as f32, plan.view_size.1 as f32);
        // SAFETY: a shader mode balanced within this block, drawing a live texture.
        unsafe {
            if shaded && let Some(shader) = self.shader.as_ref() {
                ffi::BeginShaderMode(**shader);
            }
            ffi::DrawTexturePro(
                picture.texture,
                ffi::Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height: -height,
                },
                ffi::Rectangle {
                    x: viewport.x as f32,
                    y: viewport.y as f32,
                    width: viewport.width as f32,
                    height: viewport.height as f32,
                },
                ffi::Vector2 { x: 0.0, y: 0.0 },
                0.0,
                Color::WHITE,
            );
            if shaded && self.shader.is_some() {
                ffi::EndShaderMode();
            }
        }
    }

    fn apply_uniforms(&self, plan: &ViewPlan) {
        let Some(shader) = self.shader.as_ref() else {
            return;
        };
        let fx = &plan.fx;
        let level = |effect: ScreenEffect| fx.level(effect);
        let vec4 = |values: [f32; 4]| values;
        let uniforms: [(i32, [f32; 4], i32); 4] = [
            (
                self.locations.a,
                vec4([
                    level(ScreenEffect::Pixelate),
                    level(ScreenEffect::Palette),
                    level(ScreenEffect::Exposure),
                    level(ScreenEffect::Bloom),
                ]),
                4,
            ),
            (
                self.locations.b,
                vec4([
                    level(ScreenEffect::Desaturate),
                    level(ScreenEffect::ColourGrade),
                    level(ScreenEffect::Chromatic),
                    level(ScreenEffect::Mirror),
                ]),
                4,
            ),
            (
                self.locations.c,
                vec4([
                    level(ScreenEffect::Wobble),
                    level(ScreenEffect::Lensing),
                    level(ScreenEffect::Scanlines),
                    level(ScreenEffect::Vignette),
                ]),
                4,
            ),
            (
                self.locations.d,
                vec4([
                    level(ScreenEffect::Fog),
                    fx.phase,
                    fx.light,
                    plan.art_pixel.max(1.0),
                ]),
                4,
            ),
        ];
        // SAFETY: uploads uniforms of a live shader with matching types and sizes.
        unsafe {
            for (location, values, _) in uniforms {
                ffi::SetShaderValue(
                    **shader,
                    location,
                    values.as_ptr().cast(),
                    ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC4 as i32,
                );
            }
            ffi::SetShaderValue(
                **shader,
                self.locations.grade,
                fx.grade_rgb.as_ptr().cast(),
                ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC3 as i32,
            );
            ffi::SetShaderValue(
                **shader,
                self.locations.fog,
                fx.fog_rgb.as_ptr().cast(),
                ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC3 as i32,
            );
            let size = [plan.view_size.0 as f32, plan.view_size.1 as f32];
            ffi::SetShaderValue(
                **shader,
                self.locations.size,
                size.as_ptr().cast(),
                ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC2 as i32,
            );
        }
    }
}

/// A viewport's size in physical pixels.
fn physical(viewport: Viewport, density: f32) -> (i32, i32) {
    (
        ((viewport.width as f32 * density).round() as i32).clamp(1, MAX_TARGET_SIDE),
        ((viewport.height as f32 * density).round() as i32).clamp(1, MAX_TARGET_SIDE),
    )
}

/// Makes sure `target` is a render texture of `size` (recreated when the size changes).
fn ensure_target(
    raylib: &mut RaylibHandle,
    thread: &RaylibThread,
    target: &mut Option<RenderTexture2D>,
    size: (i32, i32),
) {
    let fits = target
        .as_ref()
        .is_some_and(|texture| texture.texture.width == size.0 && texture.texture.height == size.1);
    if fits {
        return;
    }
    *target = None;
    match raylib.load_render_texture(thread, size.0.max(1) as u32, size.1.max(1) as u32) {
        Ok(texture) => *target = Some(texture),
        Err(error) => eprintln!("post-process: no render texture {size:?}: {error}"),
    }
}

/// The effect shader's fragment source: GLSL ES 100 for `WebGL1` (`gles`), GLSL 330 for the
/// desktop, from one body. raylib's default vertex shader feeds it `fragTexCoord` and
/// `fragColor`.
#[must_use]
pub fn fragment_source(gles: bool) -> String {
    let header = if gles {
        "#version 100\n\
         precision mediump float;\n\
         varying vec2 fragTexCoord;\n\
         varying vec4 fragColor;\n\
         #define TEX texture2D\n\
         #define OUT gl_FragColor\n"
    } else {
        "#version 330\n\
         in vec2 fragTexCoord;\n\
         in vec4 fragColor;\n\
         out vec4 finalColor;\n\
         #define TEX texture\n\
         #define OUT finalColor\n"
    };
    format!("{header}{FRAGMENT_BODY}")
}

/// The stack, shared by both GLSL versions (see the module docs for the order).
const FRAGMENT_BODY: &str = r"
uniform sampler2D texture0;
uniform vec4 colDiffuse;
// pixelate, palette, exposure, bloom
uniform vec4 uA;
// desaturate, colour grade, chromatic aberration, mirror
uniform vec4 uB;
// wobble, lensing, scanlines, vignette
uniform vec4 uC;
// fog, wobble phase (cycles), light, one art pixel (texels)
uniform vec4 uD;
uniform vec3 uGrade;
uniform vec3 uFogColor;
uniform vec2 uSize;

vec3 fetch(vec2 uv) {
    return TEX(texture0, clamp(uv, vec2(0.0), vec2(1.0))).rgb;
}

void main() {
    vec2 uv = fragTexCoord;
    float aspect = uSize.x / max(uSize.y, 1.0);
    vec2 centred = uv - vec2(0.5);
    float dist = length(centred * vec2(aspect, 1.0));

    // Lensing: the picture bulges around the centre.
    if (uC.y > 0.0) {
        float r2 = dist * dist;
        uv = vec2(0.5) + centred * (1.0 - uC.y * 0.3 * exp(-r2 * 5.0));
    }
    // Wobble / heat haze: a slow ripple (the phase advances under 1 Hz).
    if (uC.x > 0.0) {
        float t = uD.y * 6.2831853;
        uv.x += sin(uv.y * 36.0 + t) * 0.004 * uC.x;
        uv.y += sin(uv.x * 22.0 + t * 0.7) * 0.0025 * uC.x;
    }
    // Pixelation: blocks of whole art pixels.
    if (uA.x > 0.0) {
        float block = max(uD.w, 1.0) * floor(1.0 + uA.x * 5.0);
        vec2 cells = uSize / block;
        uv = (floor(uv * cells) + vec2(0.5)) / cells;
    }
    // Mirror (cross-faded while it blends in).
    vec2 mirrored = vec2(1.0 - uv.x, uv.y);
    vec2 suv = uB.w > 0.5 ? mirrored : uv;
    vec3 colour = fetch(uv);
    if (uB.w > 0.0) {
        colour = mix(colour, fetch(mirrored), uB.w);
    }
    // Chromatic aberration: red and blue split outward.
    if (uB.z > 0.0) {
        vec2 offset = (suv - vec2(0.5)) * 0.014 * uB.z + vec2(0.0025 * uB.z, 0.0);
        colour.r = fetch(suv + offset).r;
        colour.b = fetch(suv - offset).b;
    }
    // Bloom: bright neighbours bleed in.
    if (uA.w > 0.0) {
        vec2 px = (2.0 + 3.0 * uA.w) * max(uD.w, 1.0) * 0.5 / uSize;
        vec3 glow = max(fetch(suv + vec2(px.x, 0.0)) - 0.55, 0.0);
        glow += max(fetch(suv - vec2(px.x, 0.0)) - 0.55, 0.0);
        glow += max(fetch(suv + vec2(0.0, px.y)) - 0.55, 0.0);
        glow += max(fetch(suv - vec2(0.0, px.y)) - 0.55, 0.0);
        glow += max(fetch(suv + px) - 0.55, 0.0);
        glow += max(fetch(suv - px) - 0.55, 0.0);
        glow += max(fetch(suv + vec2(px.x, -px.y)) - 0.55, 0.0);
        glow += max(fetch(suv + vec2(-px.x, px.y)) - 0.55, 0.0);
        colour += glow * (uA.w * 0.4);
    }
    // Exposure, with a soft shoulder.
    if (uA.z > 0.0) {
        colour *= 1.0 + uA.z * 1.1;
        colour = colour / (vec3(1.0) + colour * (uA.z * 0.35));
    }
    // The field's light: dark away from the rig's lamp at the centre.
    if (uD.z < 1.0) {
        float lamp = clamp(1.0 - dist * 2.2, 0.0, 1.0);
        colour *= uD.z + (1.0 - uD.z) * lamp * lamp * 0.85;
    }
    // Desaturation.
    if (uB.x > 0.0) {
        float grey = dot(colour, vec3(0.299, 0.587, 0.114));
        colour = mix(colour, vec3(grey), uB.x);
    }
    // Colour grade toward the field's colour.
    if (uB.y > 0.0) {
        vec3 graded = colour * uGrade * 1.25 + (uGrade - vec3(0.5)) * 0.08;
        colour = mix(colour, graded, uB.y);
    }
    // Palette reduction with a static ordered dither on the art grid.
    if (uA.y > 0.0) {
        float levels = mix(24.0, 3.0, uA.y);
        vec2 cell = floor(gl_FragCoord.xy / max(uD.w, 1.0));
        float dither = (mod(cell.x + cell.y * 2.0, 4.0) * 0.25 - 0.375) * uA.y;
        colour = floor(colour * levels + vec3(0.5 + dither)) / levels;
    }
    // Fog, thicker away from the centre.
    if (uD.x > 0.0) {
        float thickness = uD.x * (0.3 + 0.7 * smoothstep(0.08, 0.7, dist));
        colour = mix(colour, uFogColor, clamp(thickness, 0.0, 0.92));
    }
    // Scanlines: every other art pixel row, never moving.
    if (uC.z > 0.0) {
        float row = mod(floor(gl_FragCoord.y / max(uD.w, 1.0)), 2.0);
        colour *= 1.0 - uC.z * 0.32 * row;
    }
    // Vignette.
    if (uC.w > 0.0) {
        colour *= 1.0 - uC.w * 0.85 * smoothstep(0.3, 0.95, dist);
    }
    OUT = vec4(clamp(colour, 0.0, 1.0), 1.0) * colDiffuse * fragColor;
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_shader_versions_are_well_formed() {
        for gles in [false, true] {
            let source = fragment_source(gles);
            let opens = source.matches('{').count();
            let closes = source.matches('}').count();
            assert_eq!(opens, closes, "balanced braces (gles {gles})");
            assert_eq!(
                source.matches('(').count(),
                source.matches(')').count(),
                "balanced parentheses (gles {gles})"
            );
            for uniform in [
                "uA",
                "uB",
                "uC",
                "uD",
                "uGrade",
                "uFogColor",
                "uSize",
                "texture0",
            ] {
                assert!(source.contains(&format!(" {uniform};")), "{uniform}");
            }
            assert!(source.contains("void main()"));
        }
        let es = fragment_source(true);
        assert!(es.starts_with("#version 100\n"));
        assert!(es.contains("precision mediump float;"));
        assert!(es.contains("varying vec2 fragTexCoord;"));
        // WebGL1 has no `texture()`, `in`/`out` qualifiers or integer bit operations.
        let body_uses = |needle: &str| FRAGMENT_BODY.contains(needle);
        assert!(!body_uses("texture("), "use TEX()");
        assert!(!body_uses(" in "), "no in qualifiers in the shared body");
        assert!(!body_uses("<<") && !body_uses(">>") && !body_uses(" & "));
        // Every float literal in GLSL ES needs a decimal point: no bare `1` multiplications.
        for line in FRAGMENT_BODY
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
        {
            for token in line.split(|c: char| !c.is_ascii_alphanumeric() && c != '.') {
                assert!(
                    token.is_empty() || !token.chars().all(|c| c.is_ascii_digit()),
                    "integer literal {token} in: {line}"
                );
            }
        }
        let desktop = fragment_source(false);
        assert!(desktop.starts_with("#version 330\n"));
        assert!(desktop.contains("out vec4 finalColor;"));
    }

    #[test]
    fn a_rolled_plan_covers_the_view_around_the_rig() {
        let plan = ViewPlan {
            fx: ScreenFx::NONE,
            roll: 0.6,
            pivot: (640.0, 360.0),
            world_size: (1_536, 1_536),
            view_size: (1_280, 720),
            zoom: 2.0,
            art_pixel: 6.0,
        };
        let rect = plan.world_rect((0.0, 0.0), (1_000.0, 500.0));
        assert!((rect.x + rect.width * 0.5 - 1_000.0).abs() < 1e-3);
        assert!((rect.y + rect.height * 0.5 - 500.0).abs() < 1e-3);
        // The square reaches every corner of the view at any angle.
        let half_diagonal = 640.0_f32.hypot(360.0);
        assert!(plan.world_size.0 as f32 * 0.5 >= half_diagonal);
    }
}
