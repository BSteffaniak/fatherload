//! Round bodies seen from space: a lit sphere in pixel-art light steps with theme-tinted
//! surface bands (or a gas giant's weather stripes), the terminator facing its star and an
//! atmosphere glow round the rim ([`super::shaders::PLANET_BODY`]). Without the shader the
//! disc is drawn in circles: a dark disc, its lit half and the atmosphere's rings.
//!
//! As the camera comes down to a body, [`ground_blend`] fades its polar ground in over the
//! disc, so the zoom from a small round world to the ground under the craft is seamless.

use raylib::{ffi, prelude::*};

use super::{
    Projection, SpaceArt, additive, draw_centred, fade, shaders,
    view::{BodyView, SpaceRenderView},
};

/// The polar ground's share of the picture at `px_per_tile` (0: only the disc).
#[must_use]
pub fn ground_blend(px_per_tile: f32) -> f32 {
    let t = ((px_per_tile - 0.9) / (3.5 - 0.9)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Copy, Debug, Default)]
struct Locations {
    sun: i32,
    look: i32,
    kind: i32,
    dark: i32,
    mid: i32,
    light: i32,
    atmos: i32,
}

/// The planet shader and its uniform locations.
pub struct PlanetShader {
    shader: Option<Shader>,
    locations: Locations,
}

/// A colour as shader floats.
fn rgb(colour: Color) -> [f32; 3] {
    [
        f32::from(colour.r) / 255.0,
        f32::from(colour.g) / 255.0,
        f32::from(colour.b) / 255.0,
    ]
}

impl PlanetShader {
    pub fn new(raylib: &mut RaylibHandle, thread: &RaylibThread) -> Self {
        let source = shaders::fragment_source(cfg!(target_os = "emscripten"), shaders::PLANET_BODY);
        let shader = super::load_shader(raylib, thread, &source, "planet");
        let locations = shader.as_ref().map_or_else(Locations::default, |shader| {
            let at = |name: &str| shader.get_shader_location(name);
            Locations {
                sun: at("uSun"),
                look: at("uLook"),
                kind: at("uKind"),
                dark: at("uDark"),
                mid: at("uMid"),
                light: at("uLight"),
                atmos: at("uAtmos"),
            }
        });
        Self { shader, locations }
    }

    #[must_use]
    pub const fn ready(&self) -> bool {
        self.shader.is_some()
    }

    /// Draws every body that is on screen, far ones first.
    pub fn draw_bodies(
        &self,
        draw: &mut impl RaylibDraw,
        art: &SpaceArt,
        view: &SpaceRenderView<'_>,
        projection: &Projection,
    ) {
        let mut bodies: Vec<(&BodyView, (f32, f32), f32)> = view
            .bodies
            .iter()
            .map(|body| {
                let point = projection.project(body.centre);
                (body, point, projection.tiles(body.radius_tiles))
            })
            .filter(|(body, point, radius)| {
                projection.visible(*point, radius * (1.0 + body.atmosphere) + 8.0)
            })
            .collect();
        bodies.sort_by(|a, b| a.2.total_cmp(&b.2));
        for (body, point, radius) in bodies {
            // A body whose ground fills the view is the ground pass's to draw.
            let ground = view
                .ground
                .as_ref()
                .is_some_and(|ground| ground.body.id == body.id);
            if ground && ground_blend(projection.px_per_tile) >= 1.0 {
                continue;
            }
            let toward = view.light_toward(body.centre);
            let sun = projection.turn(toward);
            if radius < 1.5 {
                let (_, mid) = (body.theme.palette().0[0], body.theme.palette().0[1]);
                draw.draw_rectangle_rec(
                    Rectangle::new(point.0.round() - 1.0, point.1.round() - 1.0, 2.0, 2.0),
                    mid,
                );
                continue;
            }
            if radius > 60_000.0 {
                continue;
            }
            match (self.shader.as_ref(), art.white.as_ref()) {
                (Some(shader), Some(white)) => {
                    self.draw_shaded(shader, white, body, point, radius, toward, projection);
                }
                _ => draw_plain(draw, art, body, point, radius, sun),
            }
        }
    }

    #[allow(clippy::too_many_arguments, reason = "one body's whole look")]
    fn draw_shaded(
        &self,
        shader: &Shader,
        white: &Texture2D,
        body: &BodyView,
        point: (f32, f32),
        radius: f32,
        sun: (f32, f32),
        projection: &Projection,
    ) {
        let (colours, atmos) = body.theme.palette();
        let atm = if body.atmosphere > 0.0 {
            body.atmosphere.clamp(0.06, 0.4)
        } else {
            0.0
        };
        let side = radius * 2.0 * (1.0 + atm);
        // About one art pixel per two points: the disc is pixel art like the ground, but a
        // small moon still shows a few dozen pixels across.
        let art_pixel = (radius / 48.0).clamp(1.0, 3.0);
        // The quad turns with the view, so the disc's bands, caps and light stay the body's
        // own; the light is given in the space frame.
        let spin = body.spin;
        let seed = (body.seed % 997) as f32 / 997.0;
        let vec4 = |values: [f32; 4]| values;
        let uniforms: [(i32, [f32; 4]); 3] = [
            (self.locations.sun, vec4([sun.0, sun.1, 0.0, 0.06])),
            (
                self.locations.look,
                vec4([atm, spin, art_pixel / side, seed]),
            ),
            (
                self.locations.kind,
                vec4([
                    f32::from(u8::from(body.theme.banded())),
                    if atm > 0.0 { 1.0 } else { 0.25 },
                    0.0,
                    0.0,
                ]),
            ),
        ];
        let colour3 = [
            (self.locations.dark, rgb(colours[0])),
            (self.locations.mid, rgb(colours[1])),
            (self.locations.light, rgb(colours[2])),
            (self.locations.atmos, rgb(atmos)),
        ];
        // SAFETY: uniforms of a live shader with matching types, and a balanced shader mode.
        unsafe {
            for (location, values) in uniforms {
                ffi::SetShaderValue(
                    **shader,
                    location,
                    values.as_ptr().cast(),
                    ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC4 as i32,
                );
            }
            for (location, values) in colour3 {
                ffi::SetShaderValue(
                    **shader,
                    location,
                    values.as_ptr().cast(),
                    ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC3 as i32,
                );
            }
            ffi::BeginShaderMode(**shader);
        }
        draw_centred(
            white,
            point,
            (side, side),
            -projection.roll.to_degrees(),
            Color::WHITE,
        );
        // SAFETY: closes the shader mode above.
        unsafe { ffi::EndShaderMode() };
    }
}

/// The CPU fallback: a dark disc, its lit half toward the star and a few atmosphere rings.
fn draw_plain(
    draw: &mut impl RaylibDraw,
    art: &SpaceArt,
    body: &BodyView,
    point: (f32, f32),
    radius: f32,
    sun: (f32, f32),
) {
    let (colours, atmos) = body.theme.palette();
    let centre = Vector2::new(point.0, point.1);
    if body.atmosphere > 0.0
        && let Some(glow) = art.glow.as_ref()
    {
        additive(|| {
            let size = radius * 2.0 * (1.0 + body.atmosphere.clamp(0.06, 0.4)) * 1.15;
            draw_centred(glow, point, (size, size), 0.0, fade(atmos, 0.55));
        });
    }
    let night = Color::new(colours[0].r / 4, colours[0].g / 4, colours[0].b / 3, 255);
    draw.draw_circle_v(centre, radius, night);
    // raylib's sector angles start at the right and turn clockwise on screen.
    let facing = sun.1.atan2(sun.0).to_degrees();
    draw.draw_circle_sector(centre, radius, facing - 90.0, facing + 90.0, 32, colours[1]);
    draw.draw_circle_sector(
        centre,
        radius * 0.8,
        facing - 60.0,
        facing + 60.0,
        24,
        colours[2],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ground_fades_in_as_the_camera_comes_down() {
        assert!(ground_blend(0.2).abs() < f32::EPSILON);
        assert!((ground_blend(8.0) - 1.0).abs() < f32::EPSILON);
        assert!(ground_blend(2.0) > 0.0 && ground_blend(2.0) < 1.0);
    }
}
