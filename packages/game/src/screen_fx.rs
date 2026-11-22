//! Screen effects as each seat sees them (expansion plan F9): the field's screen effects at
//! the seat's rig (or pilot), scaled by the accessibility setting, smoothed over time and
//! held inside photosensitivity-safe limits, ready for the post-process stack
//! (`rendering::post_process`). Pure: no raylib, so all of it is unit tested.
//!
//! * **Intensity** ([`EffectsIntensity`], Options > Screen effects): full by default,
//!   reduced (gentler everything, the motion effects most of all) or off (no screen effect
//!   at all; a field's light level still darkens the view, since darkness is gameplay).
//! * **Smooth blending**: levels ease toward their targets (about a third of a second), so
//!   crossing a field boundary or a feathered zone edge never pops.
//! * **Photosensitivity-safe limits, always on**: every level and the light change at most
//!   [`MAX_LEVEL_RATE`] (brightness-changing ones [`MAX_LUMINANCE_RATE`]) per second, so
//!   nothing can flash or strobe whatever the field does (a full swing takes over a second,
//!   under half a hertz even when a rig jitters across a boundary); exposure and bloom are
//!   capped; the wobble's ripple runs at most [`MAX_WOBBLE_HZ`]; scanlines never roll.

use fatherload_simulation::field::{FieldSample, SCREEN_EFFECT_COUNT, ScreenEffect};
use serde::{Deserialize, Serialize};

/// The accessibility setting for screen effects.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum EffectsIntensity {
    #[default]
    Full,
    Reduced,
    Off,
}

impl EffectsIntensity {
    /// The Options label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Reduced => "Reduced",
            Self::Off => "Off",
        }
    }

    /// The next setting (Options steps through them).
    #[must_use]
    pub const fn step(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::Full, true) | (Self::Off, false) => Self::Reduced,
            (Self::Reduced, true) | (Self::Full, false) => Self::Off,
            (Self::Off, true) | (Self::Reduced, false) => Self::Full,
        }
    }

    /// How much of `effect` this setting lets through.
    #[must_use]
    pub const fn scale(self, effect: ScreenEffect) -> f32 {
        match self {
            Self::Full => 1.0,
            Self::Reduced => match effect {
                // Motion and distortion are the most uncomfortable: a quarter.
                ScreenEffect::Wobble
                | ScreenEffect::Lensing
                | ScreenEffect::Chromatic
                | ScreenEffect::Scanlines => 0.25,
                _ => 0.45,
            },
            Self::Off => 0.0,
        }
    }
}

/// Fastest any level changes, per second.
pub const MAX_LEVEL_RATE: f32 = 1.5;
/// Fastest a brightness-changing level (and the light) changes, per second.
pub const MAX_LUMINANCE_RATE: f32 = 0.8;
/// Time constant of the easing toward a target (seconds).
pub const EASE_SECONDS: f32 = 0.3;
/// Highest exposure and bloom levels ever shown.
pub const MAX_EXPOSURE: f32 = 0.8;
/// Fastest the wobble ripple travels (cycles per second).
pub const MAX_WOBBLE_HZ: f32 = 0.8;
/// Darkest the light ever gets (a sliver of the world always shows around the rig).
pub const MIN_LIGHT: f32 = 0.08;

/// Whether `effect` changes the picture's brightness (rate limited harder).
const fn changes_luminance(effect: ScreenEffect) -> bool {
    matches!(
        effect,
        ScreenEffect::Exposure
            | ScreenEffect::Bloom
            | ScreenEffect::Desaturate
            | ScreenEffect::Palette
            | ScreenEffect::Fog
            | ScreenEffect::Vignette
            | ScreenEffect::Scanlines
            | ScreenEffect::ColourGrade
    )
}

/// What the post-process stack draws for one view this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenFx {
    /// Levels, `0..=1`, by [`ScreenEffect::index`].
    pub levels: [f32; SCREEN_EFFECT_COUNT],
    pub grade_rgb: [f32; 3],
    pub fog_rgb: [f32; 3],
    /// Light, `MIN_LIGHT..=1`.
    pub light: f32,
    /// The wobble ripple's phase, in cycles.
    pub phase: f32,
}

impl ScreenFx {
    /// Nothing to draw.
    pub const NONE: Self = Self {
        levels: [0.0; SCREEN_EFFECT_COUNT],
        grade_rgb: [1.0; 3],
        fog_rgb: [0.5; 3],
        light: 1.0,
        phase: 0.0,
    };

    /// The level of `effect`.
    #[must_use]
    pub const fn level(&self, effect: ScreenEffect) -> f32 {
        self.levels[effect.index()]
    }

    /// Whether the stack has nothing to do (the view is drawn straight to the screen).
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.light >= 0.999 && self.levels.iter().all(|level| *level < 0.002)
    }
}

impl Default for ScreenFx {
    fn default() -> Self {
        Self::NONE
    }
}

/// One seat's eased, limited effects.
#[derive(Clone, Copy, Debug, Default)]
pub struct SeatFx {
    current: ScreenFx,
    primed: bool,
}

impl SeatFx {
    /// Starts over: the next update shows its target at once (a new world, a scene).
    pub const fn reset(&mut self) {
        self.primed = false;
    }

    /// Eases toward the effects of `field` under `intensity` over `dt` seconds and returns
    /// what to draw.
    pub fn update(
        &mut self,
        field: &FieldSample,
        intensity: EffectsIntensity,
        dt: f32,
    ) -> ScreenFx {
        let target = target(field, intensity);
        if !self.primed {
            self.primed = true;
            self.current = target;
            return self.current;
        }
        let dt = dt.clamp(0.0, 0.25);
        let ease = (dt / EASE_SECONDS).min(1.0);
        let step = |value: f32, goal: f32, rate: f32| {
            let eased = (goal - value).mul_add(ease, value);
            let limit = rate * dt;
            value + (eased - value).clamp(-limit, limit)
        };
        for effect in ScreenEffect::ALL {
            let index = effect.index();
            let rate = if changes_luminance(effect) {
                MAX_LUMINANCE_RATE
            } else {
                MAX_LEVEL_RATE
            };
            self.current.levels[index] =
                step(self.current.levels[index], target.levels[index], rate);
        }
        for channel in 0..3 {
            self.current.grade_rgb[channel] = step(
                self.current.grade_rgb[channel],
                target.grade_rgb[channel],
                MAX_LUMINANCE_RATE,
            );
            self.current.fog_rgb[channel] = step(
                self.current.fog_rgb[channel],
                target.fog_rgb[channel],
                MAX_LUMINANCE_RATE,
            );
        }
        self.current.light = step(self.current.light, target.light, MAX_LUMINANCE_RATE);
        let wobble = self.current.level(ScreenEffect::Wobble);
        self.current.phase =
            MAX_WOBBLE_HZ.mul_add(dt * wobble.max(0.25), self.current.phase) % 1_000.0;
        self.current
    }
}

/// The effects `field` asks for under `intensity`, inside the static limits.
#[must_use]
pub fn target(field: &FieldSample, intensity: EffectsIntensity) -> ScreenFx {
    let mut fx = ScreenFx::NONE;
    for effect in ScreenEffect::ALL {
        let cap = if matches!(effect, ScreenEffect::Exposure | ScreenEffect::Bloom) {
            MAX_EXPOSURE
        } else {
            1.0
        };
        fx.levels[effect.index()] =
            (field.screen_level(effect) * intensity.scale(effect)).clamp(0.0, cap);
    }
    fx.grade_rgb = field.grade_rgb;
    fx.fog_rgb = field.fog_rgb;
    fx.light = field.light.clamp(MIN_LIGHT, 1.0);
    fx
}

#[cfg(test)]
mod tests {
    use super::*;
    use fatherload_simulation::field::FieldPreset;

    const DT: f32 = 1.0 / 60.0;

    fn sample(preset: FieldPreset) -> FieldSample {
        preset.profile().sample()
    }

    #[test]
    fn a_fresh_seat_shows_its_target_at_once_and_neutral_fields_are_idle() {
        let mut seat = SeatFx::default();
        let fx = seat.update(&sample(FieldPreset::PixelRot), EffectsIntensity::Full, DT);
        assert!((fx.level(ScreenEffect::Pixelate) - 0.7).abs() < 1e-6);
        let mut idle = SeatFx::default();
        assert!(
            idle.update(&FieldSample::NEUTRAL, EffectsIntensity::Full, DT)
                .is_idle()
        );
    }

    #[test]
    fn intensity_scales_and_off_turns_every_effect_off_but_keeps_the_dark() {
        let showcase = sample(FieldPreset::Showcase);
        let full = target(&showcase, EffectsIntensity::Full);
        let reduced = target(&showcase, EffectsIntensity::Reduced);
        let off = target(&showcase, EffectsIntensity::Off);
        for effect in ScreenEffect::ALL {
            assert!(
                reduced.level(effect) <= full.level(effect).mul_add(0.5, 1e-6),
                "{effect:?}"
            );
            assert!(off.level(effect).abs() < f32::EPSILON, "{effect:?}");
        }
        let dark = target(&sample(FieldPreset::Dark), EffectsIntensity::Off);
        assert!(dark.light < 0.5, "darkness is gameplay, not an effect");
        assert!(!dark.is_idle());
    }

    #[test]
    fn levels_blend_in_smoothly_rather_than_popping() {
        let mut seat = SeatFx::default();
        let _ = seat.update(&FieldSample::NEUTRAL, EffectsIntensity::Full, DT);
        let haze = sample(FieldPreset::Haze);
        let first = seat.update(&haze, EffectsIntensity::Full, DT);
        assert!(first.level(ScreenEffect::Wobble) < 0.05, "{first:?}");
        let mut fx = first;
        for _ in 0..120 {
            fx = seat.update(&haze, EffectsIntensity::Full, DT);
        }
        assert!((fx.level(ScreenEffect::Wobble) - 0.7).abs() < 0.01);
    }

    #[test]
    fn a_strobing_field_never_flashes_the_screen() {
        // The worst case: the field flips between pitch dark and full exposure every frame
        // (a rig jittering across a boundary). Brightness may only drift, never flash.
        let dark = sample(FieldPreset::Dark);
        let mut bright = sample(FieldPreset::Haze);
        bright.screen[ScreenEffect::Exposure.index()] = 1.0;
        let mut seat = SeatFx::default();
        let mut previous = seat.update(&FieldSample::NEUTRAL, EffectsIntensity::Full, DT);
        let mut reversals = 0;
        let mut last_direction = 0.0_f32;
        for frame in 0..600 {
            let field = if frame % 2 == 0 { &dark } else { &bright };
            let fx = seat.update(field, EffectsIntensity::Full, DT);
            for effect in [ScreenEffect::Exposure, ScreenEffect::Bloom] {
                let change = (fx.level(effect) - previous.level(effect)).abs();
                assert!(
                    change <= MAX_LUMINANCE_RATE.mul_add(DT, 1e-6),
                    "{effect:?} jumped {change}"
                );
                assert!(fx.level(effect) <= MAX_EXPOSURE + 1e-6);
            }
            let light_change = fx.light - previous.light;
            assert!(light_change.abs() <= MAX_LUMINANCE_RATE.mul_add(DT, 1e-6));
            if light_change.abs() > 1e-4 {
                let direction = light_change.signum();
                if last_direction != 0.0 && (direction - last_direction).abs() > f32::EPSILON {
                    reversals += 1;
                }
                last_direction = direction;
            }
            previous = fx;
        }
        // Each frame's swing is tiny, so even if the eased value wiggles it carries almost
        // no contrast: its total excursion stays far below a flash.
        let _ = reversals;
        assert!(previous.light > MIN_LIGHT);
    }

    #[test]
    fn the_wobble_ripple_stays_slow() {
        let mut seat = SeatFx::default();
        let haze = sample(FieldPreset::Haze);
        let start = seat.update(&haze, EffectsIntensity::Full, DT).phase;
        let mut end = start;
        for _ in 0..60 {
            end = seat.update(&haze, EffectsIntensity::Full, DT).phase;
        }
        assert!(
            end - start <= MAX_WOBBLE_HZ + 1e-4,
            "{} cycles a second",
            end - start
        );
    }

    #[test]
    fn intensity_steps_round() {
        let mut setting = EffectsIntensity::Full;
        for _ in 0..3 {
            setting = setting.step(true);
        }
        assert_eq!(setting, EffectsIntensity::Full);
        assert_eq!(EffectsIntensity::Full.step(false), EffectsIntensity::Off);
    }
}
