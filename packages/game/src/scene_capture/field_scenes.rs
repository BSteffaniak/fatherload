//! F9 scenes: every screen effect of the post-process stack on its own (`fx_*`), all of them
//! at once (`fx_combined`), a dark field, per-seat effects in split-screen, and the rolling
//! camera: a rig leaning in flight over town on the home world, standing on a wall in a
//! gravity shear and on the ceiling of an upside-down room.

use fatherload_simulation::{
    PlayerId, WorldState,
    field::{FieldPreset, FieldProfile, FieldZone, ScreenEffect, ScreenEffects},
};

use super::{Scene, SceneStage, carve, park_rig, reveal, tile_centre};
use crate::input::PlayerInput;

/// The town column the surface scenes park at (between the depot and the shop).
const TOWN_COLUMN: i32 = 112;

/// A zone of `profile` covering the view around tile `(x, y)`.
fn cover(world: &mut WorldState, (x, y): (i32, i32), profile: FieldProfile) {
    let _ = world.place_field_zone(FieldZone::new(
        (x - 60, y - 40),
        (x + 60, y + 40),
        0,
        profile,
    ));
}

/// Parks the rig on the surface at the town column, the town around it under `profile`.
fn surface_with(stage: &mut SceneStage<'_>, profile: FieldProfile) {
    stage.play(1, |world, seats| {
        park_rig(world, seats[0].player, TOWN_COLUMN, 4);
        cover(world, (TOWN_COLUMN, 4), profile);
    });
    stage.settle(20);
}

/// A profile with only `effect`, at `percent`.
const fn only(effect: ScreenEffect, percent: u8) -> FieldProfile {
    FieldProfile::NEUTRAL.with_screen(
        ScreenEffects::NONE
            .with(effect, percent)
            .graded([255, 150, 90])
            .fogged([150, 160, 175]),
    )
}

/// An explored cavern underground (tiles `x0..x1` x `y0..y1`), walls and floor of the
/// band's rock around it, with the rig standing on its floor.
fn cavern(world: &mut WorldState, player: PlayerId, rig_tile: (i32, i32)) {
    let (x, y) = rig_tile;
    carve(world, x - 12..x + 13, y - 8..y + 1);
    reveal(world, player, x - 30..x + 31, y - 20..y + 12);
    if let Some(rig) = world.player_mut(player) {
        rig.x = tile_centre(x);
        rig.y = tile_centre(y);
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.x_remainder = 0.0;
    }
}

macro_rules! effect_scene {
    ($name:literal, $summary:literal, $effect:expr, $percent:expr) => {
        Scene {
            name: $name,
            summary: $summary,
            setup: |stage| surface_with(stage, only($effect, $percent)),
        }
    };
}

/// Every F9 scene.
pub(super) const FIELD_SCENES: &[Scene] = &[
    effect_scene!(
        "fx_pixelate",
        "Screen effect: pixelation over town (Pixel Rot)",
        ScreenEffect::Pixelate,
        80
    ),
    effect_scene!(
        "fx_palette",
        "Screen effect: palette reduction with a static dither",
        ScreenEffect::Palette,
        85
    ),
    effect_scene!(
        "fx_exposure",
        "Screen effect: over-exposure (capped, never flashing)",
        ScreenEffect::Exposure,
        100
    ),
    effect_scene!(
        "fx_bloom",
        "Screen effect: bloom around bright parts",
        ScreenEffect::Bloom,
        100
    ),
    effect_scene!(
        "fx_desaturate",
        "Screen effect: colour drained to grey",
        ScreenEffect::Desaturate,
        90
    ),
    effect_scene!(
        "fx_grade",
        "Screen effect: a warm colour grade",
        ScreenEffect::ColourGrade,
        80
    ),
    effect_scene!(
        "fx_chromatic",
        "Screen effect: chromatic aberration (Prism Desert)",
        ScreenEffect::Chromatic,
        90
    ),
    effect_scene!(
        "fx_mirror",
        "Screen effect: the picture mirrored (Mirror Strata)",
        ScreenEffect::Mirror,
        100
    ),
    effect_scene!(
        "fx_wobble",
        "Screen effect: wobble / heat haze",
        ScreenEffect::Wobble,
        100
    ),
    effect_scene!(
        "fx_lensing",
        "Screen effect: gravity lensing around the centre",
        ScreenEffect::Lensing,
        100
    ),
    effect_scene!(
        "fx_scanlines",
        "Screen effect: static scanlines",
        ScreenEffect::Scanlines,
        90
    ),
    effect_scene!(
        "fx_vignette",
        "Screen effect: a heavy vignette",
        ScreenEffect::Vignette,
        100
    ),
    effect_scene!("fx_fog", "Screen effect: grey fog", ScreenEffect::Fog, 85),
    Scene {
        name: "fx_combined",
        summary: "Every screen effect at once (the showcase preset) over town",
        setup: |stage| surface_with(stage, FieldPreset::Showcase.profile()),
    },
    Scene {
        name: "fx_dark",
        summary: "A dark field underground: only the rig's lamp lights the cavern",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                cavern(world, player, (100, 40));
                cover(world, (100, 40), FieldPreset::Dark.profile());
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "fx_pixel_rot_cavern",
        summary: "Pixel rot underground: pixelation, a collapsed palette and scanlines",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                cavern(world, player, (100, 40));
                cover(world, (100, 40), FieldPreset::PixelRot.profile());
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "fx_split_screen",
        summary: "Per-seat effects: seat 1 in a prism field over town, seat 2 underground in \
                  fog, leaning its rig",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, TOWN_COLUMN, 4);
                cover(world, (TOWN_COLUMN, 4), FieldPreset::Prism.profile());
                cavern(world, seats[1].player, (100, 40));
                let _ = world.place_field_zone(FieldZone::new(
                    (88, 30),
                    (112, 41),
                    0,
                    FieldPreset::Fog.profile(),
                ));
            });
            stage.settle(10);
            stage.hold_seats(
                24,
                &[
                    PlayerInput::default(),
                    PlayerInput {
                        thrust: true,
                        rotate: 1.0,
                        ..PlayerInput::default()
                    },
                ],
            );
        },
    },
    Scene {
        name: "rotate_home_flying",
        summary: "The rig leaning about 35 degrees as it flies over town: the camera turns with \
                  the craft, the HUD stays upright",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, TOWN_COLUMN, 4);
            });
            stage.settle(10);
            stage.hold(
                30,
                PlayerInput {
                    thrust: true,
                    rotate: 1.0,
                    ..PlayerInput::default()
                },
            );
            stage.hold(
                20,
                PlayerInput {
                    thrust: true,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "rotate_shear_wall",
        summary: "A gravity shear underground: gravity pulls right, the rig stands on the right \
                  wall and the camera turns so that wall is the floor",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                cavern(world, player, (100, 40));
                cover(world, (100, 40), FieldPreset::GravityShear.profile());
                if let Some(rig) = world.player_mut(player) {
                    // Against the right wall (tile 112 is rock), turned to stand on it.
                    rig.x =
                        0.5f32.mul_add(-fatherload_simulation::TILE_SIZE, tile_centre(112)) - 11.0;
                    rig.y = tile_centre(37);
                    rig.attitude.orientation = -std::f32::consts::FRAC_PI_2;
                }
            });
            stage.settle(40);
        },
    },
    Scene {
        name: "rotate_upside_down",
        summary: "An upside-down room: the rig turned all the way round stands on the ceiling",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                cavern(world, player, (100, 40));
                cover(world, (100, 40), FieldPreset::GravityFlip.profile());
                if let Some(rig) = world.player_mut(player) {
                    rig.y = tile_centre(32);
                    rig.attitude.orientation = std::f32::consts::PI;
                }
            });
            stage.settle(60);
        },
    },
    Scene {
        name: "rotate_spinning_weightless",
        summary: "Weightless: the rig spun past 120 degrees with its rotation thrusters",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                cavern(world, player, (100, 40));
                cover(world, (100, 40), FieldPreset::Weightless.profile());
                if let Some(rig) = world.player_mut(player) {
                    rig.y = tile_centre(36);
                }
            });
            stage.hold(
                26,
                PlayerInput {
                    rotate: -1.0,
                    ..PlayerInput::default()
                },
            );
        },
    },
];
