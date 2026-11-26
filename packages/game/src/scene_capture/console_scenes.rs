//! Wave 14 console scenes: the command palette (typing with completions, a travel preview,
//! help, the touch CMD button) and each test-mode debug view. Every scene starts the way
//! `--god --start-at` does ([`LocalGame::start_test_world`]), so they double as a check that the
//! launch targets land.

use fatherload_simulation::{
    PlayerCommand, TestModeAction, descent::Descent, field::FieldPreset,
    test_mode::palette_stock::StockRequest,
};

use super::{Scene, SceneStage};
use crate::{
    console::{parse::TpTarget, start_at::DEFAULT_TEST_SEED},
    input::{PlayerInput, devices::InputDevice},
};

/// A test-mode world started at `target`, settled.
fn start(stage: &mut SceneStage<'_>, target: &TpTarget) {
    let report = stage.game.start_test_world(DEFAULT_TEST_SEED, Some(target));
    stage.absorb(report);
    stage.settle(30);
    // The subject is the palette and the views, not the story's cards or the arrival toast.
    for ui in &mut stage.game.shell.seats {
        ui.story_moment = None;
        ui.fanfare = None;
        ui.descent.banner = None;
    }
    "Test mode: press ` for the command palette.".clone_into(&mut stage.game.shell.message);
}

fn press(stage: &mut SceneStage<'_>, input: PlayerInput) {
    stage.hold(1, input);
}

/// Opens the palette, types `line` and (with `enter`) runs it.
fn type_line(stage: &mut SceneStage<'_>, line: &str, enter: bool) {
    if !stage.game.shell.console.open {
        press(
            stage,
            PlayerInput {
                console: true,
                ..PlayerInput::default()
            },
        );
    }
    for character in line.chars() {
        press(
            stage,
            PlayerInput {
                text_input: Some(character),
                ..PlayerInput::default()
            },
        );
    }
    if enter {
        press(
            stage,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        stage.settle(2);
    }
}

fn close(stage: &mut SceneStage<'_>) {
    press(
        stage,
        PlayerInput {
            console: true,
            ..PlayerInput::default()
        },
    );
}

/// Submits a test-mode action for the first seat (a scene's setup, as the palette would).
fn act(stage: &mut SceneStage<'_>, action: TestModeAction) {
    let player = stage.game.seats()[0].player;
    let command = PlayerCommand::TestMode(action);
    stage.play_on(|world| {
        let tick = world.simulation_tick();
        let mut inputs = fatherload_simulation::TickInputs::default();
        inputs.act_as(player, command);
        let _ = world.simulate_tick(tick, &inputs);
    });
}

/// A few creatures beside the rig (the stub of w14-stock's spawn, as `spawn` sends it).
fn spawn_some(stage: &mut SceneStage<'_>) {
    for kind in fatherload_simulation::EnemyKind::ALL.into_iter().take(3) {
        act(stage, TestModeAction::PaletteStock(StockRequest::Spawn(kind)));
    }
}

const OLD_DEEP: TpTarget = TpTarget::Descent(Descent::OldDeep);

/// Every console scene.
pub(super) const CONSOLE_SCENES: &[Scene] = &[
    Scene {
        name: "console_palette",
        summary: "Wave 14: the command palette mid-typing, its completions under the line",
        setup: |stage| {
            start(stage, &OLD_DEEP);
            type_line(stage, "help tp", true);
            type_line(stage, "tp descent 5 ?", true);
            type_line(stage, "tp planet ne", false);
        },
    },
    Scene {
        name: "console_travel",
        summary: "Wave 14: `tp boss leviathan` landed the rig in a fresh pocket by the gate",
        setup: |stage| {
            start(stage, &TpTarget::Descent(Descent::Crust));
            type_line(stage, "tp boss leviathan", true);
            stage.settle(20);
            stage.seat(0).story_moment = None;
            type_line(stage, "give dynamite 99", true);
        },
    },
    Scene {
        name: "console_touch",
        summary: "Wave 14: the touch screen's CMD button in a test-mode world",
        setup: |stage| {
            start(stage, &TpTarget::Descent(Descent::Crust));
            stage.use_devices(&[InputDevice::Touch]);
            stage.settle(4);
        },
    },
    Scene {
        name: "console_touch_palette",
        summary: "Wave 14: the palette open on touch: the menu's > completes, < recalls",
        setup: |stage| {
            start(stage, &TpTarget::Descent(Descent::Crust));
            stage.use_devices(&[InputDevice::Touch]);
            type_line(stage, "give all", true);
            type_line(stage, "tp str", false);
            press(
                stage,
                PlayerInput {
                    menu_right: true,
                    ..PlayerInput::default()
                },
            );
            type_line(stage, "c", false);
            stage.settle(2);
        },
    },
    Scene {
        name: "debug_hitboxes",
        summary: "Wave 14 debug view: hit circles (red), contact boxes (orange) and the rig's box",
        setup: |stage| {
            start(stage, &OLD_DEEP);
            spawn_some(stage);
            type_line(stage, "view hitboxes", true);
            close(stage);
            stage.settle(10);
        },
    },
    Scene {
        name: "debug_fields",
        summary: "Wave 14 debug view: field zone outlines with their feather and preset names",
        setup: |stage| {
            start(stage, &OLD_DEEP);
            act(stage, TestModeAction::PlaceFieldZone(FieldPreset::ALL[0]));
            type_line(stage, "view fields", true);
            close(stage);
            stage.settle(10);
        },
    },
    Scene {
        name: "debug_grid",
        summary: "Wave 14 debug view: the chunk grid (thin) and region cells (gold)",
        setup: |stage| {
            start(stage, &OLD_DEEP);
            type_line(stage, "view grid", true);
            close(stage);
            stage.settle(4);
        },
    },
    Scene {
        name: "debug_reveal",
        summary: "Wave 14 debug view: map reveal, no fog round a fresh deep pocket",
        setup: |stage| {
            start(stage, &TpTarget::Descent(Descent::LivingDark));
            type_line(stage, "view reveal", true);
            close(stage);
            stage.settle(4);
        },
    },
    Scene {
        name: "debug_freecam",
        summary: "Wave 14 debug view: the free camera panned off the rig and zoomed out twice",
        setup: |stage| {
            start(stage, &TpTarget::Descent(Descent::Crust));
            type_line(stage, "view reveal", true);
            type_line(stage, "view freecam", true);
            close(stage);
            for _ in 0..2 {
                press(
                    stage,
                    PlayerInput {
                        page_down: true,
                        ..PlayerInput::default()
                    },
                );
            }
            stage.hold(
                20,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "debug_fps",
        summary: "Wave 14 debug view: FPS and tick time, with the time scale at x2",
        setup: |stage| {
            start(stage, &OLD_DEEP);
            type_line(stage, "view fps", true);
            type_line(stage, "speed 2", true);
            close(stage);
            stage.settle(30);
        },
    },
];
