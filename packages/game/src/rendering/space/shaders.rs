//! The space renderer's three fragment shaders, each as GLSL 330 (desktop) and GLSL ES 100
//! (`WebGL1`) from one body, like the post-process stack's ([`super::super::post_process`]).
//!
//! * [`PLANET_BODY`]: a round body seen from space: a lit sphere with theme-tinted surface
//!   bands (or weather stripes), a soft terminator toward its star, banded pixel-art light
//!   steps and an atmosphere glow round the rim. Drawn on one quad per body.
//! * [`POLAR_BODY`]: a body's ground bent into a ring: every screen pixel is turned into the
//!   craft's local frame, then into polar coordinates round the body's centre (radius and
//!   angle, worked out relative to the camera so a 60,000-column world stays exact), then into
//!   a texel of the flat strip the tiles were painted into (a fine window of tile art near the
//!   craft, a coarse one-texel-per-tile map further out). Columns halve toward the core; day
//!   and night follow the body's star; the sky's atmosphere thins with altitude.
//! * [`LENS_BODY`]: a black hole's point lens over the picture behind it (static, no flashing;
//!   its strength follows the screen effects setting).
//!
//! The polar and planet shaders ask for `highp` where the browser's GPU has it (every desktop
//! and phone GPU with `WebGL1` does) and fall back to `mediump`.

/// The fragment shader header for `gles` (GLSL ES 100) or the desktop (GLSL 330). raylib's
/// default vertex shader feeds `fragTexCoord` and `fragColor`.
#[must_use]
pub fn fragment_source(gles: bool, body: &str) -> String {
    let header = if gles {
        "#version 100\n\
         #ifdef GL_FRAGMENT_PRECISION_HIGH\n\
         precision highp float;\n\
         #else\n\
         precision mediump float;\n\
         #endif\n\
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
    format!("{header}{body}")
}

/// A round body from space (see the module docs).
pub const PLANET_BODY: &str = r"
uniform sampler2D texture0;
// sun direction in the quad (x right, y down), the disc's share of the quad, light floor
uniform vec4 uSun;
// atmosphere share of the radius, surface turn (radians), art pixel (uv), seed (0..1)
uniform vec4 uLook;
// banded (0/1), atmosphere strength, unused, unused
uniform vec4 uKind;
uniform vec3 uDark;
uniform vec3 uMid;
uniform vec3 uLight;
uniform vec3 uAtmos;

float hash3(vec3 p) {
    p = fract(p * 0.3183099 + vec3(0.1, 0.2, 0.3));
    p *= 17.0;
    return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

float valueNoise(vec3 x) {
    vec3 i = floor(x);
    vec3 f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    float a = mix(hash3(i), hash3(i + vec3(1.0, 0.0, 0.0)), f.x);
    float b = mix(hash3(i + vec3(0.0, 1.0, 0.0)), hash3(i + vec3(1.0, 1.0, 0.0)), f.x);
    float c = mix(hash3(i + vec3(0.0, 0.0, 1.0)), hash3(i + vec3(1.0, 0.0, 1.0)), f.x);
    float d = mix(hash3(i + vec3(0.0, 1.0, 1.0)), hash3(i + vec3(1.0, 1.0, 1.0)), f.x);
    return mix(mix(a, b, f.y), mix(c, d, f.y), f.z);
}

void main() {
    // Whole art pixels: the disc is pixel art at the game's scale.
    vec2 cell = max(uLook.z, 0.0001) * vec2(1.0);
    vec2 uv = (floor(fragTexCoord / cell) + vec2(0.5)) * cell;
    float atm = uLook.x;
    vec2 p = (uv * 2.0 - vec2(1.0)) * (1.0 + atm);
    float d = length(p);
    vec3 sun = normalize(vec3(uSun.x, uSun.y, 0.42));
    vec2 sun2 = normalize(uSun.xy + vec2(0.0001));
    // Ordered dither on the art grid, so the light steps blend like hand-placed pixels.
    vec2 grid = floor(fragTexCoord / cell);
    float dither = mod(grid.x + grid.y * 2.0, 4.0) * 0.25 - 0.375;
    if (d <= 1.0) {
        float z = sqrt(max(1.0 - d * d, 0.0));
        vec3 n = vec3(p.x, p.y, z);
        float s = sin(uLook.y);
        float c = cos(uLook.y);
        vec3 turned = vec3(n.x * c + n.z * s, n.y, n.z * c - n.x * s);
        vec3 seed = vec3(uLook.w * 37.0, uLook.w * 11.0, uLook.w * 23.0);
        float h;
        if (uKind.x > 0.5) {
            // Weather stripes, gently warped.
            float warp = valueNoise(turned * 3.0 + seed) * 0.35;
            h = fract(n.y * 3.2 + warp + uLook.w);
            h = abs(h * 2.0 - 1.0);
        } else {
            h = valueNoise(turned * 2.2 + seed) * 0.6 + valueNoise(turned * 5.0 + seed) * 0.3
                + valueNoise(turned * 11.0 + seed) * 0.1;
        }
        h += dither * 0.05;
        vec3 surface = h < 0.42 ? uDark : (h < 0.62 ? uMid : uLight);
        float lambert = dot(n, sun);
        // Four light steps with a dithered terminator.
        float light = clamp(lambert * 1.3 + 0.12 + dither * 0.12, 0.0, 1.0);
        light = max(floor(light * 4.0 + 0.5) / 4.0, uSun.w);
        // The night side keeps a cold trace of its colour.
        vec3 colour = surface * (vec3(0.05, 0.06, 0.12) + vec3(0.95, 0.94, 0.88) * light);
        // The atmosphere's rim, brighter on the day side.
        float rim = pow(1.0 - z, 3.0) * uKind.y;
        float day = clamp(dot(normalize(p + vec2(0.0001)), sun2) * 0.5 + 0.6, 0.0, 1.0);
        colour = mix(colour, uAtmos, clamp(rim * day * 1.4, 0.0, 0.85));
        OUT = vec4(colour, 1.0) * fragColor;
        return;
    }
    if (atm <= 0.0) {
        discard;
    }
    float t = (d - 1.0) / atm;
    if (t >= 1.0) {
        discard;
    }
    float glow = (1.0 - t) * (1.0 - t);
    float day = clamp(dot(p / d, sun2) * 0.6 + 0.45, 0.08, 1.0);
    float level = floor((glow * day * uKind.y + dither * 0.12) * 5.0 + 0.5) / 5.0;
    if (level <= 0.0) {
        discard;
    }
    OUT = vec4(uAtmos, clamp(level, 0.0, 0.9)) * fragColor;
}
";

/// A body's ground bent into a ring (see the module docs).
pub const POLAR_BODY: &str = r"
uniform sampler2D texture0;
uniform sampler2D uCoarse;
// view size (points), pivot (points)
uniform vec4 uView;
// camera radius (tiles), points per tile, roll (radians), surface columns per radian
uniform vec4 uCam;
// fine window: width, height (tiles), camera row - window's first row, unused
uniform vec4 uFine;
uniform vec4 uFineOffA;
uniform vec4 uFineOffB;
// coarse window: width, height (texels = tiles), camera row - first row, ring columns or 0
uniform vec4 uCoarseInfo;
uniform vec4 uCoarseOffA;
uniform vec4 uCoarseOffB;
// halving rows relative to the camera row (never: 1e6)
uniform vec4 uHalveA;
uniform vec4 uHalveB;
// camera row - surface row, core radius (tiles), atmosphere height (tiles; 0 airless), lamp
uniform vec4 uGeo;
// sun angle from the camera's up (radians, clockwise), night light, atmosphere strength, time
uniform vec4 uSun;
uniform vec3 uAtmos;

float pick(vec4 a, vec4 b, float k) {
    vec4 ka = vec4(0.0, 1.0, 2.0, 3.0);
    vec4 kb = vec4(4.0, 5.0, 6.0, 7.0);
    vec4 ma = vec4(1.0) - min(abs(ka - vec4(k)), vec4(1.0));
    vec4 mb = vec4(1.0) - min(abs(kb - vec4(k)), vec4(1.0));
    return dot(a, ma) + dot(b, mb);
}

void main() {
    vec2 screen = fragTexCoord * uView.xy - uView.zw;
    float s = sin(uCam.z);
    float c = cos(uCam.z);
    // Undo the screen's turn: the craft's local frame (x east, y down), in tiles.
    vec2 q = vec2(c * screen.x - s * screen.y, s * screen.x + c * screen.y) / uCam.y;
    float rc = uCam.x;
    float bx = q.x;
    float by = rc - q.y;
    float r = sqrt(bx * bx + by * by);
    // r - rc without cancelling two big numbers.
    float dr = (q.x * q.x + q.y * q.y - 2.0 * rc * q.y) / (r + rc);
    float phi = atan(bx, by);
    float rowRel = -dr;
    vec4 ha = step(uHalveA, vec4(rowRel));
    vec4 hb = step(uHalveB, vec4(rowRel));
    float band = dot(ha, vec4(1.0)) + dot(hb, vec4(1.0));
    float scale = exp2(band);
    float across = phi * uCam.w / scale;
    float depth = rowRel + uGeo.x;

    vec4 texel = vec4(0.0);
    float fx = (pick(uFineOffA, uFineOffB, band) + across) / uFine.x;
    float fy = (uFine.z + rowRel) / uFine.y;
    if (fx >= 0.0 && fx <= 1.0 && fy >= 0.0 && fy <= 1.0) {
        texel = TEX(texture0, vec2(fx, fy));
    } else {
        float cx = pick(uCoarseOffA, uCoarseOffB, band) + across;
        if (uCoarseInfo.w > 0.0) {
            float ring = uCoarseInfo.w / scale;
            cx = mod(cx, ring);
        }
        float cy = (uCoarseInfo.z + rowRel) / uCoarseInfo.y;
        cx = cx / uCoarseInfo.x;
        if (cx >= 0.0 && cx <= 1.0 && cy >= 0.0 && cy <= 1.0) {
            texel = TEX(uCoarse, vec2(cx, cy));
        }
    }
    // The core chamber: a slow molten glow at the centre.
    float coreR = uGeo.y;
    if (r < coreR) {
        float k = r / max(coreR, 0.001);
        vec3 hot = mix(vec3(1.0, 0.95, 0.7), vec3(0.95, 0.42, 0.16), k * k);
        float edge = 1.0 - smoothstep(0.82, 1.0, k);
        texel = vec4(mix(vec3(0.35, 0.1, 0.08), hot, edge), 1.0);
    }

    // Day and night round the body: the local up against the star.
    float dayf = cos(phi - uSun.x);
    float daylight = mix(uSun.y, 1.0, smoothstep(-0.14, 0.2, dayf));
    // Under the ground the sky's light fades within a few rows; the craft's lamp lights
    // around it.
    float buried = smoothstep(1.5, 9.0, depth);
    float lampR = uGeo.w;
    float lamp = 1.0 - smoothstep(lampR * 0.45, lampR, length(screen));
    // Deep ground keeps a dim ambient (the strata stay readable), whatever the hour.
    float light = max(mix(daylight, 0.26, buried), lamp * 0.95);
    // The core lights the rings round it.
    if (r < coreR * 3.0) {
        light = max(light, 1.0 - smoothstep(coreR, coreR * 3.0, r));
    }
    vec3 colour = texel.rgb * light;

    // The sky: an atmosphere thinning with altitude, lit on the day side.
    float alt = -depth;
    float air = 0.0;
    if (uGeo.z > 0.0 && alt > -2.0) {
        air = exp(-max(alt, 0.0) / uGeo.z) * uSun.z;
        air *= smoothstep(-0.25, 0.3, dayf) * 0.85 + 0.15;
        air = floor(air * 8.0 + 0.5) / 8.0;
    }
    vec3 sky = uAtmos * (0.35 + 0.65 * daylight);
    float alpha = texel.a + air * (1.0 - texel.a);
    vec3 outColour = alpha > 0.0
        ? (colour * texel.a + sky * air * (1.0 - texel.a)) / alpha
        : vec3(0.0);
    if (alpha <= 0.001) {
        discard;
    }
    OUT = vec4(outColour, alpha) * fragColor;
}
";

/// A black hole's point lens (see the module docs).
pub const LENS_BODY: &str = r"
uniform sampler2D texture0;
// lens centre (uv), horizon radius (share of the view's height), strength (0..1)
uniform vec4 uLens;
uniform vec2 uSize;

void main() {
    vec2 uv = fragTexCoord;
    float aspect = uSize.x / max(uSize.y, 1.0);
    vec2 offset = (uv - uLens.xy) * vec2(aspect, 1.0);
    float r = max(length(offset), 0.00001);
    float horizon = uLens.z;
    // The photon ring sits at about 2.6 horizon radii.
    float ring = horizon * 2.6;
    float bend = (ring * ring) / (r * r) * uLens.w;
    vec2 source = offset * (1.0 - min(bend, 3.0));
    vec2 suv = uLens.xy + source / vec2(aspect, 1.0);
    vec3 colour = TEX(texture0, clamp(suv, vec2(0.0), vec2(1.0))).rgb;
    // A faint photon ring, then the horizon's shadow (the shadow is the object itself, so it
    // stays even with the lensing off).
    float glow = exp(-abs(r - ring) / max(horizon * 0.12, 0.0001)) * 0.35 * uLens.w;
    colour += vec3(1.0, 0.82, 0.55) * glow;
    float shadow = 1.0 - smoothstep(horizon * 0.96, horizon * 1.04, r);
    colour *= 1.0 - shadow;
    OUT = vec4(clamp(colour, 0.0, 1.0), 1.0) * fragColor;
}
";

#[cfg(test)]
mod tests {
    use super::*;

    fn check_body(body: &str, uniforms: &[&str]) {
        for gles in [false, true] {
            let source = fragment_source(gles, body);
            assert_eq!(
                source.matches('{').count(),
                source.matches('}').count(),
                "balanced braces"
            );
            assert_eq!(
                source.matches('(').count(),
                source.matches(')').count(),
                "balanced parentheses"
            );
            for uniform in uniforms {
                assert!(source.contains(&format!(" {uniform};")), "{uniform}");
            }
            assert!(source.contains("void main()"));
        }
        assert!(fragment_source(true, body).starts_with("#version 100\n"));
        assert!(fragment_source(false, body).starts_with("#version 330\n"));
        // WebGL1: no texture(), in/out qualifiers, integer bit operations or bare integer
        // literals in the shared body.
        let code: Vec<&str> = body
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect();
        for line in &code {
            assert!(!line.contains("texture("), "use TEX(): {line}");
            assert!(!line.contains(" in "), "no in qualifiers: {line}");
            assert!(!line.contains("<<") && !line.contains(">>") && !line.contains(" & "));
        }
        for line in code {
            for token in line.split(|c: char| !c.is_ascii_alphanumeric() && c != '.') {
                assert!(
                    token.is_empty() || !token.chars().all(|c| c.is_ascii_digit()),
                    "integer literal {token} in: {line}"
                );
            }
        }
    }

    #[test]
    fn every_space_shader_is_well_formed_for_both_versions() {
        // GLSL reserves the old built-in noise1..noise4 names.
        for body in [PLANET_BODY, POLAR_BODY, LENS_BODY] {
            for reserved in ["noise1(", "noise2(", "noise3(", "noise4("] {
                assert!(!body.contains(reserved), "{reserved}");
            }
        }
        check_body(
            PLANET_BODY,
            &[
                "texture0", "uSun", "uLook", "uKind", "uDark", "uMid", "uLight", "uAtmos",
            ],
        );
        check_body(
            POLAR_BODY,
            &[
                "texture0",
                "uCoarse",
                "uView",
                "uCam",
                "uFine",
                "uFineOffA",
                "uFineOffB",
                "uCoarseInfo",
                "uCoarseOffA",
                "uCoarseOffB",
                "uHalveA",
                "uHalveB",
                "uGeo",
                "uSun",
                "uAtmos",
            ],
        );
        check_body(LENS_BODY, &["texture0", "uLens", "uSize"]);
        let es = fragment_source(true, POLAR_BODY);
        assert!(es.contains("GL_FRAGMENT_PRECISION_HIGH"));
    }
}
