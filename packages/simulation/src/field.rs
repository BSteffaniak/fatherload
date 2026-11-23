//! Field effects (expansion plan F9): deterministic per-region physics and screen fields.
//!
//! [`sample`] (and [`crate::WorldState::fields_at`]) answers, for any world pixel, a
//! [`FieldSample`]: the **gravity vector** (any direction and strength, in standard
//! gravities), **time scale**, ground **friction**, air **drag**, **control mirroring**,
//! **light level**, **sound occlusion**, whether a rig may **rotate freely** there, and the
//! **screen effects** a view centred there shows. It is [`FieldSample::NEUTRAL`] everywhere
//! today, so every existing world plays exactly as before.
//!
//! Two sources feed it, blended in order:
//!
//! 1. **The stratum profile** ([`stratum_profile_at`]): plain data ([`FieldProfile`]) per
//!    stratum, supplied by the Descent framework through [`STRATUM_FIELD_SOURCES`] (empty
//!    today: the integrator registers the stratum registry's lookup there). A source blends
//!    across its own boundaries with [`boundary_blend`] and [`FieldProfile::lerp`].
//! 2. **Authored zones** ([`FieldZone`], kept in [`FieldZones`]): small rectangles (anomaly
//!    rooms, boss arenas, dev zones) with a feathered edge, blended over the base by
//!    [`FieldZone::weight`] so nothing changes abruptly at a boundary.
//!
//! Rigs ([`crate::world`] movement), pilots, creatures, drops and charges (through
//! [`crate::physics::BodyParams::in_field`]) read the sample; light, sound occlusion and the
//! screen effects are presentation reads (the renderer's post-process stack, the audio
//! director).
//!
//! Everything is basic arithmetic on the position and plain data (no transcendental
//! functions; see [`trig`]), so hosts, predicting clients and every platform agree bit for
//! bit.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{position::Frame, terrain::TILE_SIZE, worldgen::WorldSettings};

pub mod presets;
pub mod trig;

pub use presets::FieldPreset;

/// A screen effect the post-process stack can apply (appended only: the order is the wire and
/// save order, and indexes [`ScreenEffects::levels`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ScreenEffect {
    /// The world drawn in big blocks (Pixel Rot).
    Pixelate,
    /// Fewer colours per channel (palette collapse).
    Palette,
    /// Over-exposure: everything brighter, highlights washed out.
    Exposure,
    /// Bright parts bleed light around them.
    Bloom,
    /// Colour drained toward grey.
    Desaturate,
    /// A colour grade toward [`ScreenEffects::grade_rgb`].
    ColourGrade,
    /// Red and blue split apart (Prism Desert).
    Chromatic,
    /// The picture mirrored left to right (Mirror Strata; pairs with control mirroring).
    Mirror,
    /// Wobble / heat haze: the picture ripples.
    Wobble,
    /// Lensing: the picture bulges around the view's centre (gravity wells).
    Lensing,
    /// CRT scanlines.
    Scanlines,
    /// Darkened corners.
    Vignette,
    /// Fog toward [`ScreenEffects::fog_rgb`], thicker away from the centre.
    Fog,
    /// Blur: the picture smears soft (the Fungal Forest's spore clouds, Wave 8).
    Blur,
}

impl ScreenEffect {
    /// Every effect, in stack order.
    pub const ALL: [Self; SCREEN_EFFECT_COUNT] = [
        Self::Pixelate,
        Self::Palette,
        Self::Exposure,
        Self::Bloom,
        Self::Desaturate,
        Self::ColourGrade,
        Self::Chromatic,
        Self::Mirror,
        Self::Wobble,
        Self::Lensing,
        Self::Scanlines,
        Self::Vignette,
        Self::Fog,
        Self::Blur,
    ];

    /// Index into [`ScreenEffects::levels`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// A short name (settings, scene names, debug overlays).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pixelate => "pixelate",
            Self::Palette => "palette",
            Self::Exposure => "exposure",
            Self::Bloom => "bloom",
            Self::Desaturate => "desaturate",
            Self::ColourGrade => "grade",
            Self::Chromatic => "chromatic",
            Self::Mirror => "mirror",
            Self::Wobble => "wobble",
            Self::Lensing => "lensing",
            Self::Scanlines => "scanlines",
            Self::Vignette => "vignette",
            Self::Fog => "fog",
            Self::Blur => "blur",
        }
    }
}

/// Number of [`ScreenEffect`]s.
pub const SCREEN_EFFECT_COUNT: usize = 14;

/// Screen effects as plain data: a level (percent, 0..=100) per [`ScreenEffect`] plus the
/// colours the grade and the fog lean toward.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ScreenEffects {
    /// Saved and sent as a list, so files written before an effect was appended (13 levels
    /// before the Wave 8 blur) load with the new effects off.
    #[serde(with = "levels_serde")]
    pub levels: [u8; SCREEN_EFFECT_COUNT],
    pub grade_rgb: [u8; 3],
    pub fog_rgb: [u8; 3],
}

impl ScreenEffects {
    /// No effect at all.
    pub const NONE: Self = Self {
        levels: [0; SCREEN_EFFECT_COUNT],
        grade_rgb: [255, 255, 255],
        fog_rgb: [128, 128, 128],
    };

    /// These effects with `effect` at `percent` (clamped to 100).
    #[must_use]
    pub const fn with(mut self, effect: ScreenEffect, percent: u8) -> Self {
        self.levels[effect.index()] = if percent > 100 { 100 } else { percent };
        self
    }

    /// These effects with the grade colour `rgb`.
    #[must_use]
    pub const fn graded(mut self, rgb: [u8; 3]) -> Self {
        self.grade_rgb = rgb;
        self
    }

    /// These effects with the fog colour `rgb`.
    #[must_use]
    pub const fn fogged(mut self, rgb: [u8; 3]) -> Self {
        self.fog_rgb = rgb;
        self
    }

    /// The level of `effect` (percent).
    #[must_use]
    pub const fn level(&self, effect: ScreenEffect) -> u8 {
        self.levels[effect.index()]
    }

    /// Whether every level is zero.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.levels.iter().all(|level| *level == 0)
    }
}

impl Default for ScreenEffects {
    fn default() -> Self {
        Self::NONE
    }
}

/// [`ScreenEffects::levels`] as a list: older lists (fewer effects) pad with zeros, longer ones
/// (a newer build's effects) drop what this build does not know.
mod levels_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::SCREEN_EFFECT_COUNT;

    pub fn serialize<S: Serializer>(
        levels: &[u8; SCREEN_EFFECT_COUNT],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        levels.as_slice().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; SCREEN_EFFECT_COUNT], D::Error> {
        let list = Vec::<u8>::deserialize(deserializer)?;
        let mut levels = [0; SCREEN_EFFECT_COUNT];
        for (slot, level) in levels.iter_mut().zip(list) {
            *slot = level;
        }
        Ok(levels)
    }
}

/// One region's field effects as plain data: what a stratum (the Descent framework's
/// per-stratum field-effect profile) or an authored zone declares.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct FieldProfile {
    /// Gravity, in standard gravities: `(0, 1)` is the ground's pull straight down (+y),
    /// `(0, -1)` upside down, `(1, 0)` sideways to the right, `(0, 0)` weightless. It
    /// multiplies whatever the altitude environment already does ([`crate::environment`]).
    pub gravity_x: f32,
    pub gravity_y: f32,
    /// How fast time runs for bodies here (1 normal, 0.5 slow motion, 2 fast forward).
    pub time_scale: f32,
    /// Ground grip: scales a grounded rig's braking (1 normal, 0.2 ice).
    pub friction: f32,
    /// Air drag: scales airborne braking and the fall-speed drag (1 normal).
    pub drag: f32,
    /// Light, `0.0..=1.0` (1 the normal light of the place, 0 total darkness).
    pub light: f32,
    /// How much sound is muffled, `0.0..=1.0`.
    pub sound_occlusion: f32,
    /// Left and right swapped for anyone steering here (the screen mirrors with
    /// [`ScreenEffect::Mirror`]).
    pub mirror_controls: bool,
    /// Rigs may rotate all the way round (space later; any field whose gravity is not
    /// straight down allows it anyway, see [`FieldSample::rotation_unlimited`]).
    pub free_rotation: bool,
    /// What a view centred here shows.
    pub screen: ScreenEffects,
}

impl FieldProfile {
    /// The ordinary world: standard gravity, time, grip, drag, full light, no effects.
    pub const NEUTRAL: Self = Self {
        gravity_x: 0.0,
        gravity_y: 1.0,
        time_scale: 1.0,
        friction: 1.0,
        drag: 1.0,
        light: 1.0,
        sound_occlusion: 0.0,
        mirror_controls: false,
        free_rotation: false,
        screen: ScreenEffects::NONE,
    };

    /// This profile with gravity `(x, y)` (standard gravities).
    #[must_use]
    pub const fn with_gravity(mut self, x: f32, y: f32) -> Self {
        self.gravity_x = x;
        self.gravity_y = y;
        self
    }

    /// This profile with time running at `scale`.
    #[must_use]
    pub const fn with_time_scale(mut self, scale: f32) -> Self {
        self.time_scale = scale;
        self
    }

    /// This profile with screen effects `screen`.
    #[must_use]
    pub const fn with_screen(mut self, screen: ScreenEffects) -> Self {
        self.screen = screen;
        self
    }

    /// The sample of a point fully inside this profile.
    #[must_use]
    pub fn sample(&self) -> FieldSample {
        FieldSample::NEUTRAL.blend(self, 1.0)
    }

    /// The profile `t` of the way from `self` to `other` (`0..=1`): numbers interpolate,
    /// switches flip at the halfway point, screen levels and colours interpolate (for
    /// stratum sources blending across their boundaries).
    #[must_use]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: f32, b: f32| (b - a).mul_add(t, a);
        let mix_u8 = |a: u8, b: u8| {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a rounded mix of two bytes is a byte"
            )]
            let value = mix(f32::from(a), f32::from(b)).round().clamp(0.0, 255.0) as u8;
            value
        };
        let mut screen = self.screen;
        for (index, level) in screen.levels.iter_mut().enumerate() {
            *level = mix_u8(*level, other.screen.levels[index]);
        }
        for channel in 0..3 {
            screen.grade_rgb[channel] = mix_u8(
                self.screen.grade_rgb[channel],
                other.screen.grade_rgb[channel],
            );
            screen.fog_rgb[channel] =
                mix_u8(self.screen.fog_rgb[channel], other.screen.fog_rgb[channel]);
        }
        let half = t >= 0.5;
        Self {
            gravity_x: mix(self.gravity_x, other.gravity_x),
            gravity_y: mix(self.gravity_y, other.gravity_y),
            time_scale: mix(self.time_scale, other.time_scale),
            friction: mix(self.friction, other.friction),
            drag: mix(self.drag, other.drag),
            light: mix(self.light, other.light),
            sound_occlusion: mix(self.sound_occlusion, other.sound_occlusion),
            mirror_controls: if half {
                other.mirror_controls
            } else {
                self.mirror_controls
            },
            free_rotation: if half {
                other.free_rotation
            } else {
                self.free_rotation
            },
            screen,
        }
    }
}

impl Default for FieldProfile {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// The blended fields at one point (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldSample {
    /// Gravity in standard gravities (`(0, 1)`: straight down at full strength).
    pub gravity_x: f32,
    pub gravity_y: f32,
    pub time_scale: f32,
    pub friction: f32,
    pub drag: f32,
    pub light: f32,
    pub sound_occlusion: f32,
    /// How mirrored the controls are, `0..=1` (mirrored past one half).
    pub mirror: f32,
    /// How free rotation is, `0..=1` (free past one half).
    pub free_rotation: f32,
    /// Screen effect levels, `0..=1` each, indexed by [`ScreenEffect::index`].
    pub screen: [f32; SCREEN_EFFECT_COUNT],
    /// Grade and fog colours, `0..=1` per channel.
    pub grade_rgb: [f32; 3],
    pub fog_rgb: [f32; 3],
}

/// Gravity below this strength (standard gravities) counts as weightless: a rig's frame is
/// then its own orientation.
pub const WEIGHTLESS_GRAVITY: f32 = 0.2;
/// Gravity leaning further than this from straight down (sine of the angle, about 6
/// degrees) lets rigs rotate freely.
pub const SHEAR_FREE_ROTATION: f32 = 0.1;

impl FieldSample {
    /// The ordinary world (see [`FieldProfile::NEUTRAL`]).
    pub const NEUTRAL: Self = Self {
        gravity_x: 0.0,
        gravity_y: 1.0,
        time_scale: 1.0,
        friction: 1.0,
        drag: 1.0,
        light: 1.0,
        sound_occlusion: 0.0,
        mirror: 0.0,
        free_rotation: 0.0,
        screen: [0.0; SCREEN_EFFECT_COUNT],
        grade_rgb: [1.0, 1.0, 1.0],
        fog_rgb: [0.5, 0.5, 0.5],
    };

    /// This sample with `weight` (`0..=1`) of `profile`'s departure from the ordinary
    /// world added: zones stack (a slow-time zone inside a mirror room is slow and mirrored),
    /// and every value moves linearly with the weight, so feathered edges blend smoothly.
    /// Light, occlusion and screen levels clamp to `0..=1`; a colour moves toward the
    /// profile's only where the profile uses it (its grade or fog level is not zero).
    #[must_use]
    pub fn blend(&self, profile: &FieldProfile, weight: f32) -> Self {
        let t = weight.clamp(0.0, 1.0);
        if t <= 0.0 {
            return *self;
        }
        let neutral = FieldProfile::NEUTRAL;
        let add = |value: f32, target: f32, base: f32| (target - base).mul_add(t, value);
        let flag = |on: bool| if on { 1.0 } else { 0.0 };
        let mut screen = self.screen;
        for (index, level) in screen.iter_mut().enumerate() {
            *level =
                add(*level, f32::from(profile.screen.levels[index]) / 100.0, 0.0).clamp(0.0, 1.0);
        }
        let mix = |a: f32, b: f32| (b - a).mul_add(t, a);
        let mut grade_rgb = self.grade_rgb;
        let mut fog_rgb = self.fog_rgb;
        let grades = profile.screen.level(ScreenEffect::ColourGrade) > 0;
        let fogs = profile.screen.level(ScreenEffect::Fog) > 0;
        for channel in 0..3 {
            if grades {
                grade_rgb[channel] = mix(
                    grade_rgb[channel],
                    f32::from(profile.screen.grade_rgb[channel]) / 255.0,
                );
            }
            if fogs {
                fog_rgb[channel] = mix(
                    fog_rgb[channel],
                    f32::from(profile.screen.fog_rgb[channel]) / 255.0,
                );
            }
        }
        Self {
            gravity_x: add(self.gravity_x, profile.gravity_x, neutral.gravity_x),
            gravity_y: add(self.gravity_y, profile.gravity_y, neutral.gravity_y),
            time_scale: add(self.time_scale, profile.time_scale, neutral.time_scale).max(0.0),
            friction: add(self.friction, profile.friction, neutral.friction).max(0.0),
            drag: add(self.drag, profile.drag, neutral.drag).max(0.0),
            light: add(self.light, profile.light, neutral.light).clamp(0.0, 1.0),
            sound_occlusion: add(
                self.sound_occlusion,
                profile.sound_occlusion,
                neutral.sound_occlusion,
            )
            .clamp(0.0, 1.0),
            mirror: add(self.mirror, flag(profile.mirror_controls), 0.0).clamp(0.0, 1.0),
            free_rotation: add(self.free_rotation, flag(profile.free_rotation), 0.0)
                .clamp(0.0, 1.0),
            screen,
            grade_rgb,
            fog_rgb,
        }
    }

    /// Whether the physics here is exactly the ordinary world's (gravity, time, grip, drag
    /// and controls): movement then takes the classic path, bit for bit.
    #[must_use]
    #[allow(clippy::float_cmp, reason = "the neutral values are exact constants")]
    pub fn is_neutral_physics(&self) -> bool {
        self.gravity_x == 0.0
            && self.gravity_y == 1.0
            && self.time_scale == 1.0
            && self.friction == 1.0
            && self.drag == 1.0
            && self.mirror == 0.0
            && self.free_rotation == 0.0
    }

    /// Whether left and right are swapped here.
    #[must_use]
    pub fn mirrored(&self) -> bool {
        self.mirror > 0.5
    }

    /// Gravity's strength, in standard gravities.
    #[must_use]
    pub fn gravity_strength(&self) -> f32 {
        self.gravity_x.hypot(self.gravity_y)
    }

    /// The direction "down" points here, as an angle from straight down (+y) toward -x
    /// (radians; 0 on the ground, pi upside down, pi/2 when gravity pulls to the left), or
    /// `None` when weightless ([`WEIGHTLESS_GRAVITY`]).
    #[must_use]
    pub fn down_angle(&self) -> Option<f32> {
        if self.gravity_strength() < WEIGHTLESS_GRAVITY {
            return None;
        }
        if self.gravity_x == 0.0 && self.gravity_y > 0.0 {
            return Some(0.0);
        }
        // down = (-sin a, cos a)
        Some(trig::atan2(-self.gravity_x, self.gravity_y))
    }

    /// Whether rigs may rotate all the way round here: the field says so, gravity leans
    /// away from straight down (gravity shear, upside-down rooms) or there is next to none.
    #[must_use]
    pub fn rotation_unlimited(&self) -> bool {
        let strength = self.gravity_strength();
        self.free_rotation > 0.5
            || strength < WEIGHTLESS_GRAVITY
            || self.gravity_y <= 0.0
            || self.gravity_x.abs() > SHEAR_FREE_ROTATION * strength
    }

    /// The level (`0..=1`) of `effect` a view centred here shows.
    #[must_use]
    pub const fn screen_level(&self, effect: ScreenEffect) -> f32 {
        self.screen[effect.index()]
    }
}

impl Default for FieldSample {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// An authored field zone: a rectangle of tiles (inclusive) with a feathered edge.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct FieldZone {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
    /// Tiles over which the zone fades in from its edge (0: a hard edge).
    pub feather_tiles: u16,
    pub profile: FieldProfile,
    /// The preset it was made from, if any (dev zones, anomaly rooms; names and art).
    #[serde(default)]
    pub preset: Option<FieldPreset>,
}

impl FieldZone {
    /// A zone of `profile` over the tile rectangle `min..=max`, feathered over
    /// `feather_tiles`.
    #[must_use]
    pub const fn new(
        (min_x, min_y): (i32, i32),
        (max_x, max_y): (i32, i32),
        feather_tiles: u16,
        profile: FieldProfile,
    ) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
            feather_tiles,
            profile,
            preset: None,
        }
    }

    /// A zone of `preset`'s profile centred on tile `(x, y)`, `half_width` x `half_height`
    /// tiles each way, feathered over its preset's edge.
    #[must_use]
    pub const fn of_preset(
        preset: FieldPreset,
        (x, y): (i32, i32),
        (half_width, half_height): (i32, i32),
    ) -> Self {
        Self {
            min_x: x - half_width,
            min_y: y - half_height,
            max_x: x + half_width,
            max_y: y + half_height,
            feather_tiles: preset.feather_tiles(),
            profile: preset.profile(),
            preset: Some(preset),
        }
    }

    /// How strongly the zone applies at world pixel `(x, y)` (the fixed origin frame): 0
    /// outside, 1 deeper inside than its feather, a smooth (smoothstep) ramp across the feather.
    #[must_use]
    pub fn weight(&self, x: f32, y: f32) -> f32 {
        self.weight_in(Frame::default(), x, y)
    }

    /// [`Self::weight`] at local pixel `(x, y)` of `frame` (F7): the zone's tile bounds are
    /// absolute, so they are measured from the frame's origin (exactly the old world pixels in
    /// the fixed origin frame).
    #[must_use]
    pub fn weight_in(&self, frame: Frame, x: f32, y: f32) -> f32 {
        let left = frame.col_px(self.min_x);
        let top = frame.row_px(self.min_y);
        let right = frame.col_px(self.max_x.saturating_add(1));
        let bottom = frame.row_px(self.max_y.saturating_add(1));
        if !(x >= left && x < right && y >= top && y < bottom) {
            return 0.0;
        }
        if self.feather_tiles == 0 {
            return 1.0;
        }
        let inside = (x - left).min(right - x).min(y - top).min(bottom - y);
        let t = (inside / (f32::from(self.feather_tiles) * TILE_SIZE)).clamp(0.0, 1.0);
        t * t * 2.0f32.mul_add(-t, 3.0)
    }

    /// Whether tile `(x, y)` lies inside the zone's rectangle.
    #[must_use]
    pub const fn contains_tile(&self, x: i32, y: i32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }
}

/// Authored field zones of a world, by id (blended in id order).
///
/// Saved with the world,
/// replicated as the shared [`crate::SectionKey::Fields`] section and hashed only once
/// non-empty, so worlds without zones keep their hashes.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FieldZones {
    #[serde(default)]
    zones: BTreeMap<u32, FieldZone>,
    #[serde(default)]
    next_id: u32,
}

/// Most zones a world keeps (dev placement drops the oldest beyond it).
pub const MAX_FIELD_ZONES: usize = 64;

impl FieldZones {
    /// Whether there are no zones.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// Every zone, by id.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &FieldZone)> + '_ {
        self.zones.iter().map(|(id, zone)| (*id, zone))
    }

    /// Adds `zone` and returns its id (dropping the oldest past [`MAX_FIELD_ZONES`]).
    pub fn insert(&mut self, zone: FieldZone) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.zones.insert(id, zone);
        while self.zones.len() > MAX_FIELD_ZONES {
            let _ = self.zones.pop_first();
        }
        id
    }

    /// Removes the zone `id`.
    pub fn remove(&mut self, id: u32) -> Option<FieldZone> {
        self.zones.remove(&id)
    }

    /// Removes every zone.
    pub fn clear(&mut self) {
        self.zones.clear();
    }

    /// The zones that cover local pixel `(x, y)` of `frame` with their weights, in blend
    /// order.
    pub fn weights_at(
        &self,
        frame: Frame,
        x: f32,
        y: f32,
    ) -> impl Iterator<Item = (&FieldZone, f32)> + '_ {
        self.zones.values().filter_map(move |zone| {
            let weight = zone.weight_in(frame, x, y);
            (weight > 0.0).then_some((zone, weight))
        })
    }
}

/// A stratum field source.
///
/// The field-effect profile of the stratum at local pixel `(x, y)` of `frame` (F7) in a world
/// with `settings`, already blended across its own boundaries, or `None` where it has nothing
/// to say (every generator before V11, the sky, the town).
pub type StratumFieldSource = fn(&WorldSettings, Frame, f32, f32) -> Option<FieldProfile>;

/// The registered stratum field sources, asked in order (the first answer wins).
///
/// **Integration seam (Wave 7, wired):** the Descent framework's stratum registry (F8)
/// registers its per-stratum profile lookup here ([]), so a V11 stratum's gravity, time,
/// grip, drag, light, mirroring and screen effects reach every reader of [`sample`] with no
/// other change. Sources must be pure functions of the settings and the position.
pub const STRATUM_FIELD_SOURCES: &[StratumFieldSource] = &[
    // Wave 8: the Void Geode Fields' low-gravity geodes (inside a hollow only; it blends the
    // stratum's own field in itself).
    crate::old_deep::fields::geode_field_source,
    // Wave 8 story: anomaly rooms answer inside their walls (blending in the stratum's own
    // profile), and nowhere else.
    crate::expedition::anomaly::anomaly_field_source,
    // Wave 9: the Upside-Down Isles' inverted gravity under each isle (it blends the stratum's
    // own field in itself).
    crate::hollow::fields::isle_field_source,
    crate::descent::fields::stratum_field_source,
];

/// The stratum profile at local pixel `(x, y)` of `frame` (see [`STRATUM_FIELD_SOURCES`]).
#[must_use]
pub fn stratum_profile_at(
    settings: &WorldSettings,
    frame: Frame,
    x: f32,
    y: f32,
) -> Option<FieldProfile> {
    STRATUM_FIELD_SOURCES
        .iter()
        .find_map(|source| source(settings, frame, x, y))
}

/// How far (`0..=1`) a point at `row` (fractional tile rows) has crossed a boundary.
///
/// The boundary is at `boundary_row`, blending over `half_width` rows each side with a smoothstep: 0 well above
/// it, 1 well below, 0.5 on it. For stratum sources blending neighbouring profiles with
/// [`FieldProfile::lerp`].
#[must_use]
pub fn boundary_blend(row: f32, boundary_row: f32, half_width: f32) -> f32 {
    if half_width <= 0.0 {
        return if row >= boundary_row { 1.0 } else { 0.0 };
    }
    let t = ((row - boundary_row) / (2.0 * half_width) + 0.5).clamp(0.0, 1.0);
    t * t * 2.0f32.mul_add(-t, 3.0)
}

/// The fields at world pixel `(x, y)` of the fixed origin frame (tests and fixed-frame
/// tools; the world reads [`sample_in`] through its frame).
#[must_use]
pub fn sample(zones: &FieldZones, settings: &WorldSettings, x: f32, y: f32) -> FieldSample {
    sample_in(zones, settings, Frame::default(), x, y)
}

/// The fields at local pixel `(x, y)` of `frame` (F7): the stratum profile, then every zone
/// over it.
#[must_use]
pub fn sample_in(
    zones: &FieldZones,
    settings: &WorldSettings,
    frame: Frame,
    x: f32,
    y: f32,
) -> FieldSample {
    let base = stratum_profile_at(settings, frame, x, y)
        .map_or(FieldSample::NEUTRAL, |profile| profile.sample());
    if zones.is_empty() {
        return base;
    }
    zones
        .weights_at(frame, x, y)
        .fold(base, |sample, (zone, weight)| {
            sample.blend(&zone.profile, weight)
        })
}

#[cfg(test)]
mod tests;
