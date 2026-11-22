//! Named field profiles.
//!
//! The anomaly rooms of the plan (§14 cross-Descent systems) and the dev zones placed through test mode ([`crate::TestModeAction::PlaceFieldZone`]) and the
//! screenshot scenes. Strata define their own profiles; these are building blocks and
//! showcases.

use serde::{Deserialize, Serialize};

use super::{FieldProfile, ScreenEffect, ScreenEffects};

/// A named field profile (appended only: wire and save order).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FieldPreset {
    /// Gravity pulls up: an upside-down room.
    GravityFlip,
    /// Gravity pulls to the right (gravity shear, 90 degrees).
    GravityShear,
    /// Next to no gravity: drift and spin.
    Weightless,
    /// Slow motion (half speed).
    SlowTime,
    /// Fast forward (one and a half times).
    FastTime,
    /// Mirrored screen and controls.
    Mirror,
    /// Pixel rot: pixelation and a collapsed palette.
    PixelRot,
    /// Near-total darkness with a vignette.
    Dark,
    /// Split colours (chromatic aberration) and a warm grade.
    Prism,
    /// Heat haze and bloom.
    Haze,
    /// A gravity well's lensing with a cold grade.
    Lens,
    /// Thick fog and muffled sound.
    Fog,
    /// Slick ice: almost no grip and thin drag.
    Ice,
    /// Every screen effect at once (the combined showcase; physics stays ordinary).
    Showcase,
    /// A spore cloud of the Living Dark (Wave 8): the picture blurs and shifts toward a sickly
    /// violet-green. A rig with the Bio-Filter sees through it.
    Spores,
}

impl FieldPreset {
    /// Every preset.
    pub const ALL: [Self; 15] = [
        Self::GravityFlip,
        Self::GravityShear,
        Self::Weightless,
        Self::SlowTime,
        Self::FastTime,
        Self::Mirror,
        Self::PixelRot,
        Self::Dark,
        Self::Prism,
        Self::Haze,
        Self::Lens,
        Self::Fog,
        Self::Ice,
        Self::Showcase,
        Self::Spores,
    ];

    /// A display name.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::GravityFlip => "Upside-down room",
            Self::GravityShear => "Gravity shear",
            Self::Weightless => "Weightless",
            Self::SlowTime => "Slow time",
            Self::FastTime => "Fast time",
            Self::Mirror => "Mirror room",
            Self::PixelRot => "Pixel rot",
            Self::Dark => "Darkness",
            Self::Prism => "Prism",
            Self::Haze => "Heat haze",
            Self::Lens => "Gravity lens",
            Self::Fog => "Fog bank",
            Self::Ice => "Ice field",
            Self::Showcase => "Every effect",
            Self::Spores => "Spore cloud",
        }
    }

    /// Tiles over which a zone of this preset fades in from its edge.
    #[must_use]
    pub const fn feather_tiles(self) -> u16 {
        match self {
            Self::GravityFlip | Self::GravityShear | Self::Weightless => 2,
            _ => 3,
        }
    }

    /// The preset's field profile.
    #[must_use]
    pub const fn profile(self) -> FieldProfile {
        let neutral = FieldProfile::NEUTRAL;
        let none = ScreenEffects::NONE;
        match self {
            Self::GravityFlip => neutral.with_gravity(0.0, -1.0).with_screen(
                none.with(ScreenEffect::Lensing, 25)
                    .with(ScreenEffect::Vignette, 30),
            ),
            Self::GravityShear => neutral.with_gravity(1.0, 0.0).with_screen(
                none.with(ScreenEffect::Lensing, 20)
                    .with(ScreenEffect::Vignette, 25),
            ),
            Self::Weightless => {
                let mut profile = neutral.with_gravity(0.0, 0.05);
                profile.free_rotation = true;
                profile.drag = 0.3;
                profile
            }
            Self::SlowTime => neutral.with_time_scale(0.5).with_screen(
                none.with(ScreenEffect::Desaturate, 55)
                    .with(ScreenEffect::ColourGrade, 40)
                    .graded([150, 190, 255]),
            ),
            Self::FastTime => neutral.with_time_scale(1.5).with_screen(
                none.with(ScreenEffect::ColourGrade, 45)
                    .with(ScreenEffect::Chromatic, 20)
                    .graded([255, 200, 120]),
            ),
            Self::Mirror => {
                let mut profile = neutral.with_screen(
                    none.with(ScreenEffect::Mirror, 100)
                        .with(ScreenEffect::ColourGrade, 25)
                        .graded([210, 160, 255]),
                );
                profile.mirror_controls = true;
                profile
            }
            Self::PixelRot => neutral.with_screen(
                none.with(ScreenEffect::Pixelate, 70)
                    .with(ScreenEffect::Palette, 75)
                    .with(ScreenEffect::Scanlines, 30),
            ),
            Self::Dark => {
                let mut profile = neutral.with_screen(none.with(ScreenEffect::Vignette, 90));
                profile.light = 0.15;
                profile.sound_occlusion = 0.3;
                profile
            }
            Self::Prism => neutral.with_screen(
                none.with(ScreenEffect::Chromatic, 80)
                    .with(ScreenEffect::ColourGrade, 30)
                    .graded([255, 220, 170]),
            ),
            Self::Haze => neutral.with_screen(
                none.with(ScreenEffect::Wobble, 70)
                    .with(ScreenEffect::Bloom, 60)
                    .with(ScreenEffect::Exposure, 25),
            ),
            Self::Lens => neutral.with_screen(
                none.with(ScreenEffect::Lensing, 80)
                    .with(ScreenEffect::ColourGrade, 35)
                    .graded([170, 200, 255]),
            ),
            Self::Fog => {
                let mut profile = neutral.with_screen(
                    none.with(ScreenEffect::Fog, 75)
                        .with(ScreenEffect::Desaturate, 25)
                        .fogged([150, 160, 170]),
                );
                profile.sound_occlusion = 0.6;
                profile
            }
            Self::Ice => {
                let mut profile = neutral.with_screen(
                    none.with(ScreenEffect::ColourGrade, 35)
                        .with(ScreenEffect::Bloom, 30)
                        .graded([190, 230, 255]),
                );
                profile.friction = 0.15;
                profile.drag = 0.6;
                profile
            }
            Self::Showcase => {
                let mut screen = none;
                let mut index = 0;
                while index < screen.levels.len() {
                    screen.levels[index] = 45;
                    index += 1;
                }
                neutral.with_screen(
                    screen
                        .with(ScreenEffect::Mirror, 0)
                        .graded([255, 190, 150])
                        .fogged([120, 110, 140]),
                )
            }
            Self::Spores => {
                let mut profile = neutral.with_screen(
                    none.with(ScreenEffect::Blur, 70)
                        .with(ScreenEffect::ColourGrade, 60)
                        .with(ScreenEffect::Fog, 30)
                        .graded([190, 255, 150])
                        .fogged([150, 110, 170]),
                );
                profile.sound_occlusion = 0.3;
                profile
            }
        }
    }
}
