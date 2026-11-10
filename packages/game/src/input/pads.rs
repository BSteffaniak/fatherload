//! Raylib/GLFW glue for controllers: samples every connected pad into a [`PadSample`] and
//! implements `fatherload --list-gamepads`.

use std::collections::BTreeMap;
#[cfg(any(test, not(target_os = "emscripten")))]
use std::{
    ffi::{CStr, c_char, c_int},
    time::{Duration, Instant},
};

use raylib::prelude::*;

use super::{
    gamepad::{PadButton, PadState, TRIGGER_THRESHOLD},
    router::PadSample,
};

/// Controller slots raylib polls (`MAX_GAMEPADS` in raylib's `rcore.c`).
pub const MAX_PADS: u8 = 4;

/// How long `--list-gamepads` keeps its window open so GLFW can report Bluetooth pads.
#[cfg(any(test, not(target_os = "emscripten")))]
const LIST_WAIT: Duration = Duration::from_millis(1_200);

// GLFW is statically linked into raylib; these read facts raylib does not expose (whether
// GLFW has a standard gamepad mapping, the raw axis/button counts and the GUID).
#[cfg(any(test, not(target_os = "emscripten")))]
unsafe extern "C" {
    fn glfwJoystickIsGamepad(jid: c_int) -> c_int;
    fn glfwGetJoystickAxes(jid: c_int, count: *mut c_int) -> *const f32;
    fn glfwGetJoystickButtons(jid: c_int, count: *mut c_int) -> *const u8;
    fn glfwGetJoystickGUID(jid: c_int) -> *const c_char;
    fn glfwGetGamepadName(jid: c_int) -> *const c_char;
}

const RAYLIB_BUTTONS: [(PadButton, GamepadButton); 15] = [
    (PadButton::A, GamepadButton::GAMEPAD_BUTTON_RIGHT_FACE_DOWN),
    (PadButton::B, GamepadButton::GAMEPAD_BUTTON_RIGHT_FACE_RIGHT),
    (PadButton::X, GamepadButton::GAMEPAD_BUTTON_RIGHT_FACE_LEFT),
    (PadButton::Y, GamepadButton::GAMEPAD_BUTTON_RIGHT_FACE_UP),
    (
        PadButton::LeftBumper,
        GamepadButton::GAMEPAD_BUTTON_LEFT_TRIGGER_1,
    ),
    (
        PadButton::RightBumper,
        GamepadButton::GAMEPAD_BUTTON_RIGHT_TRIGGER_1,
    ),
    (PadButton::View, GamepadButton::GAMEPAD_BUTTON_MIDDLE_LEFT),
    (PadButton::Menu, GamepadButton::GAMEPAD_BUTTON_MIDDLE_RIGHT),
    (PadButton::Guide, GamepadButton::GAMEPAD_BUTTON_MIDDLE),
    (
        PadButton::LeftStick,
        GamepadButton::GAMEPAD_BUTTON_LEFT_THUMB,
    ),
    (
        PadButton::RightStick,
        GamepadButton::GAMEPAD_BUTTON_RIGHT_THUMB,
    ),
    (
        PadButton::DPadUp,
        GamepadButton::GAMEPAD_BUTTON_LEFT_FACE_UP,
    ),
    (
        PadButton::DPadDown,
        GamepadButton::GAMEPAD_BUTTON_LEFT_FACE_DOWN,
    ),
    (
        PadButton::DPadLeft,
        GamepadButton::GAMEPAD_BUTTON_LEFT_FACE_LEFT,
    ),
    (
        PadButton::DPadRight,
        GamepadButton::GAMEPAD_BUTTON_LEFT_FACE_RIGHT,
    ),
];

/// Trigger travel 0..1 from raylib's -1 (released) .. 1 (pulled) axis.
fn trigger(raw: f32) -> f32 {
    ((raw + 1.0) * 0.5).clamp(0.0, 1.0)
}

/// Samples one pad (assumed available).
fn sample_pad(raylib: &RaylibHandle, index: i32) -> PadState {
    let mut state = PadState {
        left_x: raylib.get_gamepad_axis_movement(index, GamepadAxis::GAMEPAD_AXIS_LEFT_X),
        left_y: raylib.get_gamepad_axis_movement(index, GamepadAxis::GAMEPAD_AXIS_LEFT_Y),
        right_x: raylib.get_gamepad_axis_movement(index, GamepadAxis::GAMEPAD_AXIS_RIGHT_X),
        right_y: raylib.get_gamepad_axis_movement(index, GamepadAxis::GAMEPAD_AXIS_RIGHT_Y),
        ..PadState::default()
    };
    for (button, raylib_button) in RAYLIB_BUTTONS {
        if raylib.is_gamepad_button_down(index, raylib_button) {
            state.buttons.insert(button);
        }
    }
    for (button, axis) in [
        (
            PadButton::LeftTrigger,
            GamepadAxis::GAMEPAD_AXIS_LEFT_TRIGGER,
        ),
        (
            PadButton::RightTrigger,
            GamepadAxis::GAMEPAD_AXIS_RIGHT_TRIGGER,
        ),
    ] {
        if trigger(raylib.get_gamepad_axis_movement(index, axis)) >= TRIGGER_THRESHOLD {
            state.buttons.insert(button);
        }
    }
    state
}

/// Every connected controller's sample, by index (raylib checks presence each frame, so
/// controllers plugged in or out mid-game appear and disappear here).
#[must_use]
pub fn sample_pads(raylib: &RaylibHandle) -> BTreeMap<u8, PadSample> {
    (0..MAX_PADS)
        .filter(|index| raylib.is_gamepad_available(i32::from(*index)))
        .map(|index| {
            let raw = i32::from(index);
            let name = raylib
                .get_gamepad_name(raw)
                .unwrap_or_else(|| format!("Controller {}", index + 1));
            (
                index,
                PadSample {
                    name,
                    state: sample_pad(raylib, raw),
                },
            )
        })
        .collect()
}

/// What GLFW knows about one joystick.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(test, not(target_os = "emscripten")))]
pub struct PadReport {
    pub index: u8,
    pub name: String,
    /// Whether GLFW has a standard gamepad mapping (without one, no buttons or axes read).
    pub standard_mapping: bool,
    /// Name of the standard mapping, when there is one.
    pub mapping_name: Option<String>,
    pub axes: i32,
    pub buttons: i32,
    pub guid: String,
}

#[cfg(any(test, not(target_os = "emscripten")))]
impl PadReport {
    /// One line for the terminal.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "gamepad {index}: \"{name}\" | axes {axes} | buttons {buttons} | standard mapping: {mapping} | guid {guid}",
            index = self.index,
            name = self.name,
            axes = self.axes,
            buttons = self.buttons,
            mapping = match (&self.mapping_name, self.standard_mapping) {
                (Some(mapping), true) => format!("yes (\"{mapping}\")"),
                (None, true) => "yes".to_owned(),
                (_, false) => "NO (buttons will not read; the pad needs an SDL mapping)".to_owned(),
            },
            guid = self.guid,
        )
    }
}

#[cfg(any(test, not(target_os = "emscripten")))]
fn c_string(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    // SAFETY: GLFW returns NUL-terminated strings valid until the joystick disconnects or the
    // library terminates; this copies it immediately.
    Some(
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// Reports every connected joystick. Call with a live raylib window (GLFW initialized).
#[must_use]
#[cfg(any(test, not(target_os = "emscripten")))]
pub fn report_pads(raylib: &RaylibHandle) -> Vec<PadReport> {
    (0..MAX_PADS)
        .filter(|index| raylib.is_gamepad_available(i32::from(*index)))
        .map(|index| {
            let jid = c_int::from(index);
            let (mut axes, mut buttons) = (0, 0);
            // SAFETY: GLFW is initialized (raylib owns the window) and these calls only read
            // joystick state on the main thread; the out-pointers are valid locals.
            let (standard_mapping, mapping_name, guid) = unsafe {
                let _ = glfwGetJoystickAxes(jid, &raw mut axes);
                let _ = glfwGetJoystickButtons(jid, &raw mut buttons);
                let standard = glfwJoystickIsGamepad(jid) != 0;
                let mapping = if standard {
                    c_string(glfwGetGamepadName(jid))
                } else {
                    None
                };
                (standard, mapping, c_string(glfwGetJoystickGUID(jid)))
            };
            PadReport {
                index,
                name: raylib
                    .get_gamepad_name(i32::from(index))
                    .unwrap_or_default(),
                standard_mapping,
                mapping_name,
                axes,
                buttons,
                guid: guid.unwrap_or_default(),
            }
        })
        .collect()
}

/// Whether `args` ask for `--list-gamepads`.
#[must_use]
#[cfg(any(test, not(target_os = "emscripten")))]
pub fn wants_list(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--list-gamepads")
}

/// `fatherload --list-gamepads`: opens a tiny window for about a second (GLFW only enumerates
/// joysticks with a window and an event loop, and Bluetooth pads can take a moment), prints
/// every detected controller and exits.
#[cfg(any(test, not(target_os = "emscripten")))]
pub fn list_gamepads() {
    let (mut raylib, thread) = raylib::init()
        .size(320, 120)
        .title("Fatherload gamepad check")
        .build();
    raylib.set_target_fps(30);
    let start = Instant::now();
    while start.elapsed() < LIST_WAIT && !raylib.window_should_close() {
        let mut draw = raylib.begin_drawing(&thread);
        draw.clear_background(Color::BLACK);
        draw.draw_text("Checking controllers...", 12, 50, 20, Color::RAYWHITE);
    }
    let reports = report_pads(&raylib);
    if reports.is_empty() {
        println!(
            "No gamepads detected. Connect the controller (USB or Bluetooth), make sure it is \
             paired, then run --list-gamepads again."
        );
    }
    for report in &reports {
        println!("{}", report.line());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_rest_at_zero_and_pull_to_one() {
        assert!(trigger(-1.0).abs() < f32::EPSILON);
        assert!((trigger(1.0) - 1.0).abs() < f32::EPSILON);
        assert!((trigger(0.0) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn report_lines_flag_missing_mappings() {
        let mut report = PadReport {
            index: 0,
            name: "Xbox Wireless Controller".to_owned(),
            standard_mapping: true,
            mapping_name: Some("Xbox Controller".to_owned()),
            axes: 6,
            buttons: 16,
            guid: "050000005e040000".to_owned(),
        };
        assert!(report.line().contains("standard mapping: yes"));
        assert!(report.line().contains("axes 6"));
        report.standard_mapping = false;
        assert!(report.line().contains("standard mapping: NO"));
    }

    #[test]
    fn list_flag_is_detected() {
        assert!(wants_list(&["--list-gamepads".to_owned()]));
        assert!(!wants_list(&["--screenshot-scenes".to_owned()]));
    }
}
