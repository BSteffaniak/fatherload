//! Touch-screen scenes: existing scenes shown with the touch controls over them, so the
//! on-screen stick, thumb cluster and top row can be checked at phone sizes
//! (`--size 844x390`, `--size 390x844`).

use super::{Scene, SceneStage, all_scenes};
use crate::input::{
    devices::InputDevice,
    touch::{TouchControl, TouchView},
};

/// The scenes, chained after the main list.
pub const TOUCH_SCENES: &[Scene] = &[
    Scene {
        name: "touch_space",
        summary: "Touch controls in space (Wave 11): CRUISE, PING, WARP and the rotation pair; the stick thrusts",
        setup: |stage| restage(stage, "space_nav_hud", TouchView::default()),
    },
    Scene {
        name: "touch_hud",
        summary: "Touch controls while digging: the resting stick, bomb, scan and tools buttons, the top row",
        setup: |stage| restage(stage, "shallow_dig", TouchView::default()),
    },
    Scene {
        name: "touch_stick_held",
        summary: "Touch controls with a thumb on the stick and the tools fan open",
        setup: |stage| {
            restage(
                stage,
                "world_props",
                TouchView {
                    stick: Some(((150.0, 250.0), (185.0, 225.0))),
                    held: vec![TouchControl::Tools],
                    tools_open: true,
                    ..TouchView::default()
                },
            );
        },
    },
    Scene {
        name: "touch_armed",
        summary: "Touch controls with a weapon mounted: fire held and dragged to aim, the weapon chip",
        setup: |stage| {
            restage(
                stage,
                "weapon_rivet_gun",
                TouchView {
                    held: vec![TouchControl::Fire],
                    aim: Some((TouchControl::Fire, (-0.8, -0.3))),
                    ..TouchView::default()
                },
            );
        },
    },
    Scene {
        name: "touch_detonator",
        summary: "Touch controls with C4 waiting: the detonator button appears",
        setup: |stage| restage(stage, "explosives_c4_armed", TouchView::default()),
    },
    Scene {
        name: "touch_door_prompt",
        summary: "Touch controls at a building door: the enter prompt is the button",
        setup: |stage| restage(stage, "keyboard_hud", TouchView::default()),
    },
    Scene {
        name: "touch_build",
        summary: "Build mode on a touch screen: place, take down, turn and piece buttons beside the build bar",
        setup: |stage| restage(stage, "base_build_ui", TouchView::default()),
    },
    Scene {
        name: "touch_room",
        summary: "Inside a building on a touch screen: the stick walks, use and back buttons",
        setup: |stage| restage(stage, "room_fuel", TouchView::default()),
    },
    Scene {
        name: "touch_counter",
        summary: "A shop counter on a touch screen",
        setup: |stage| restage(stage, "interior_shop", TouchView::default()),
    },
    Scene {
        name: "touch_pilot_ruin",
        summary: "On foot in a ruin on a touch screen: jump, fire, bomb, the USE button at the lever, ROPE and the weapon swap",
        setup: |stage| restage(stage, "pilot_lever_ruin", TouchView::default()),
    },
    Scene {
        name: "touch_pause",
        summary: "The pause menu on a touch screen",
        setup: |stage| restage(stage, "pause_menu", TouchView::default()),
    },
];

/// Stages the scene called `name`, then hands the first seat to the touch screen showing `view`
/// (fingers on the controls).
fn restage(stage: &mut SceneStage<'_>, name: &str, view: TouchView) {
    if let Some(scene) = all_scenes().find(|scene| scene.name == name) {
        (scene.setup)(stage);
    }
    stage.use_devices(&[InputDevice::Touch]);
    stage.touch_view = view;
}
