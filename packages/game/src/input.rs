use std::collections::BTreeSet;

use raylib::prelude::*;

const GAMEPAD_DEADZONE: f32 = 0.25;

#[allow(
    clippy::struct_excessive_bools,
    reason = "input snapshots are simple edge/level booleans"
)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerInput {
    pub horizontal: f32,
    pub thrust: bool,
    pub drill_down: bool,
    pub interact: bool,
    pub confirm: bool,
    pub cancel: bool,
    pub pause: bool,
    pub menu_up: bool,
    pub menu_down: bool,
    pub menu_left: bool,
    pub menu_right: bool,
    pub details: bool,
    pub inventory: bool,
    pub save: bool,
    pub load: bool,
    pub selected_upgrade: Option<usize>,
    pub map: bool,
    pub help: bool,
    pub volume_up: bool,
    pub volume_down: bool,
    pub fullscreen: bool,
    pub local_multiplayer_toggle: bool,
    pub bomb: bool,
    pub scan: bool,
    pub place_relay: bool,
    pub place_drone: bool,
    pub place_lift: bool,
    pub place_support: bool,
    pub place_pump: bool,
    pub place_processor: bool,
    pub exit_requested: bool,
    pub text_input: Option<char>,
    pub text_backspace: bool,
    pub ui_scroll: f32,
    /// F3: toggle the network debug overlay.
    pub net_debug: bool,
    /// F4 (dev, overlay open): select the next live tuning value.
    pub tune_next: bool,
    /// Page Up (dev, overlay open): raise the selected tuning value.
    pub tune_up: bool,
    /// Page Down (dev, overlay open): lower the selected tuning value.
    pub tune_down: bool,
    /// Ctrl+V: paste the clipboard into the focused text field.
    pub paste: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum GamepadInputButton {
    Thrust,
    Interact,
    Bomb,
    Scan,
}

#[derive(Clone, Debug, Default)]
pub struct GamepadInputState {
    pub left_x: f32,
    pub left_y: f32,
    pub buttons: BTreeSet<GamepadInputButton>,
}

#[must_use]
pub fn read_input(raylib: &mut RaylibHandle, exit_requested: bool) -> PlayerInput {
    let mut input = read_primary_keyboard_input(raylib);
    input.exit_requested = exit_requested;
    input
}

#[must_use]
pub fn read_input_with_arrow_aliases(
    raylib: &mut RaylibHandle,
    exit_requested: bool,
) -> PlayerInput {
    let mut input = read_primary_keyboard_input_with_arrow_aliases(raylib);
    input.exit_requested = exit_requested;
    input
}

#[must_use]
pub fn read_gamepad_input(raylib: &RaylibHandle, gamepad: i32) -> Option<PlayerInput> {
    if !raylib.is_gamepad_available(gamepad) {
        return None;
    }
    let mut buttons = BTreeSet::new();
    if raylib.is_gamepad_button_down(gamepad, GamepadButton::GAMEPAD_BUTTON_RIGHT_FACE_DOWN) {
        buttons.insert(GamepadInputButton::Thrust);
    }
    if raylib.is_gamepad_button_pressed(gamepad, GamepadButton::GAMEPAD_BUTTON_RIGHT_FACE_LEFT) {
        buttons.insert(GamepadInputButton::Interact);
    }
    if raylib.is_gamepad_button_pressed(gamepad, GamepadButton::GAMEPAD_BUTTON_RIGHT_TRIGGER_1) {
        buttons.insert(GamepadInputButton::Bomb);
    }
    if raylib.is_gamepad_button_pressed(gamepad, GamepadButton::GAMEPAD_BUTTON_LEFT_TRIGGER_1) {
        buttons.insert(GamepadInputButton::Scan);
    }
    Some(map_gamepad_state(&GamepadInputState {
        left_x: raylib.get_gamepad_axis_movement(gamepad, GamepadAxis::GAMEPAD_AXIS_LEFT_X),
        left_y: raylib.get_gamepad_axis_movement(gamepad, GamepadAxis::GAMEPAD_AXIS_LEFT_Y),
        buttons,
    }))
}

#[must_use]
pub fn map_gamepad_state(state: &GamepadInputState) -> PlayerInput {
    let interact = state.buttons.contains(&GamepadInputButton::Interact);
    PlayerInput {
        horizontal: gamepad_axis(state.left_x),
        thrust: state.buttons.contains(&GamepadInputButton::Thrust),
        drill_down: gamepad_axis(state.left_y) > 0.0,
        interact,
        confirm: interact,
        bomb: state.buttons.contains(&GamepadInputButton::Bomb),
        scan: state.buttons.contains(&GamepadInputButton::Scan),
        ..PlayerInput::default()
    }
}

#[must_use]
pub fn combine_player_input(primary: PlayerInput, secondary: PlayerInput) -> PlayerInput {
    PlayerInput {
        horizontal: if secondary.horizontal.abs() > primary.horizontal.abs() {
            secondary.horizontal
        } else {
            primary.horizontal
        },
        thrust: primary.thrust || secondary.thrust,
        drill_down: primary.drill_down || secondary.drill_down,
        interact: primary.interact || secondary.interact,
        confirm: primary.confirm || secondary.confirm,
        cancel: primary.cancel || secondary.cancel,
        pause: primary.pause || secondary.pause,
        menu_up: primary.menu_up || secondary.menu_up,
        menu_down: primary.menu_down || secondary.menu_down,
        menu_left: primary.menu_left || secondary.menu_left,
        menu_right: primary.menu_right || secondary.menu_right,
        details: primary.details || secondary.details,
        inventory: primary.inventory || secondary.inventory,
        save: primary.save || secondary.save,
        load: primary.load || secondary.load,
        selected_upgrade: primary.selected_upgrade.or(secondary.selected_upgrade),
        map: primary.map || secondary.map,
        help: primary.help || secondary.help,
        volume_up: primary.volume_up || secondary.volume_up,
        volume_down: primary.volume_down || secondary.volume_down,
        fullscreen: primary.fullscreen || secondary.fullscreen,
        local_multiplayer_toggle: primary.local_multiplayer_toggle
            || secondary.local_multiplayer_toggle,
        bomb: primary.bomb || secondary.bomb,
        scan: primary.scan || secondary.scan,
        place_relay: primary.place_relay || secondary.place_relay,
        place_drone: primary.place_drone || secondary.place_drone,
        place_lift: primary.place_lift || secondary.place_lift,
        place_support: primary.place_support || secondary.place_support,
        place_pump: primary.place_pump || secondary.place_pump,
        place_processor: primary.place_processor || secondary.place_processor,
        exit_requested: primary.exit_requested || secondary.exit_requested,
        text_input: primary.text_input.or(secondary.text_input),
        text_backspace: primary.text_backspace || secondary.text_backspace,
        ui_scroll: primary.ui_scroll + secondary.ui_scroll,
        net_debug: primary.net_debug || secondary.net_debug,
        tune_next: primary.tune_next || secondary.tune_next,
        tune_up: primary.tune_up || secondary.tune_up,
        tune_down: primary.tune_down || secondary.tune_down,
        paste: primary.paste || secondary.paste,
    }
}

#[must_use]
pub fn read_primary_keyboard_input(raylib: &mut RaylibHandle) -> PlayerInput {
    read_primary_keyboard_input_with_options(raylib, false)
}

#[must_use]
pub fn read_primary_keyboard_input_with_arrow_aliases(raylib: &mut RaylibHandle) -> PlayerInput {
    read_primary_keyboard_input_with_options(raylib, true)
}

fn read_primary_keyboard_input_with_options(
    raylib: &mut RaylibHandle,
    include_arrow_aliases: bool,
) -> PlayerInput {
    let left = raylib.is_key_down(KeyboardKey::KEY_A)
        || (include_arrow_aliases && raylib.is_key_down(KeyboardKey::KEY_LEFT));
    let right = raylib.is_key_down(KeyboardKey::KEY_D)
        || (include_arrow_aliases && raylib.is_key_down(KeyboardKey::KEY_RIGHT));
    let up = raylib.is_key_down(KeyboardKey::KEY_W)
        || raylib.is_key_down(KeyboardKey::KEY_SPACE)
        || (include_arrow_aliases && raylib.is_key_down(KeyboardKey::KEY_UP));
    let down = raylib.is_key_down(KeyboardKey::KEY_S)
        || (include_arrow_aliases && raylib.is_key_down(KeyboardKey::KEY_DOWN));
    let ctrl_down = raylib.is_key_down(KeyboardKey::KEY_LEFT_CONTROL)
        || raylib.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL);
    let tab_pressed = raylib.is_key_pressed(KeyboardKey::KEY_TAB);
    let text_input = raylib.get_char_pressed();

    PlayerInput {
        horizontal: horizontal_axis(left, right),
        thrust: up,
        drill_down: down,
        interact: raylib.is_key_pressed(KeyboardKey::KEY_E),
        confirm: raylib.is_key_pressed(KeyboardKey::KEY_ENTER)
            || raylib.is_key_pressed(KeyboardKey::KEY_E),
        cancel: raylib.is_key_pressed(KeyboardKey::KEY_BACKSPACE)
            || raylib.is_key_pressed(KeyboardKey::KEY_ESCAPE),
        pause: raylib.is_key_pressed(KeyboardKey::KEY_P),
        menu_up: raylib.is_key_pressed(KeyboardKey::KEY_UP)
            || raylib.is_key_pressed(KeyboardKey::KEY_W),
        menu_down: raylib.is_key_pressed(KeyboardKey::KEY_DOWN)
            || raylib.is_key_pressed(KeyboardKey::KEY_S),
        menu_left: raylib.is_key_pressed(KeyboardKey::KEY_LEFT)
            || raylib.is_key_pressed(KeyboardKey::KEY_A),
        menu_right: raylib.is_key_pressed(KeyboardKey::KEY_RIGHT)
            || raylib.is_key_pressed(KeyboardKey::KEY_D),
        details: ctrl_down && raylib.is_key_down(KeyboardKey::KEY_TAB),
        inventory: tab_pressed && !ctrl_down,
        save: raylib.is_key_pressed(KeyboardKey::KEY_F5),
        load: raylib.is_key_pressed(KeyboardKey::KEY_F9),
        selected_upgrade: selected_upgrade(raylib),
        map: raylib.is_key_pressed(KeyboardKey::KEY_M),
        help: raylib.is_key_pressed(KeyboardKey::KEY_H),
        volume_up: raylib.is_key_pressed(KeyboardKey::KEY_EQUAL)
            || raylib.is_key_pressed(KeyboardKey::KEY_KP_ADD),
        volume_down: raylib.is_key_pressed(KeyboardKey::KEY_MINUS)
            || raylib.is_key_pressed(KeyboardKey::KEY_KP_SUBTRACT),
        fullscreen: raylib.is_key_pressed(KeyboardKey::KEY_F11),
        local_multiplayer_toggle: raylib.is_key_pressed(KeyboardKey::KEY_F2),
        bomb: raylib.is_key_pressed(KeyboardKey::KEY_B),
        scan: raylib.is_key_pressed(KeyboardKey::KEY_C),
        place_relay: raylib.is_key_pressed(KeyboardKey::KEY_R),
        place_drone: raylib.is_key_pressed(KeyboardKey::KEY_T),
        place_lift: raylib.is_key_pressed(KeyboardKey::KEY_L),
        place_support: raylib.is_key_pressed(KeyboardKey::KEY_U),
        place_pump: raylib.is_key_pressed(KeyboardKey::KEY_O),
        place_processor: raylib.is_key_pressed(KeyboardKey::KEY_P),
        exit_requested: false,
        text_input,
        text_backspace: raylib.is_key_pressed(KeyboardKey::KEY_BACKSPACE),
        ui_scroll: raylib.get_mouse_wheel_move(),
        net_debug: raylib.is_key_pressed(KeyboardKey::KEY_F3),
        tune_next: raylib.is_key_pressed(KeyboardKey::KEY_F4),
        tune_up: raylib.is_key_pressed(KeyboardKey::KEY_PAGE_UP),
        tune_down: raylib.is_key_pressed(KeyboardKey::KEY_PAGE_DOWN),
        paste: ctrl_down && raylib.is_key_pressed(KeyboardKey::KEY_V),
    }
}

#[must_use]
pub fn read_secondary_keyboard_input(raylib: &RaylibHandle) -> PlayerInput {
    let left = raylib.is_key_down(KeyboardKey::KEY_LEFT);
    let right = raylib.is_key_down(KeyboardKey::KEY_RIGHT);
    let up = raylib.is_key_down(KeyboardKey::KEY_UP);
    let down = raylib.is_key_down(KeyboardKey::KEY_DOWN);

    PlayerInput {
        horizontal: horizontal_axis(left, right),
        thrust: up,
        drill_down: down,
        interact: raylib.is_key_pressed(KeyboardKey::KEY_RIGHT_CONTROL),
        confirm: raylib.is_key_pressed(KeyboardKey::KEY_RIGHT_CONTROL),
        cancel: raylib.is_key_pressed(KeyboardKey::KEY_RIGHT_SHIFT),
        bomb: raylib.is_key_pressed(KeyboardKey::KEY_KP_0),
        scan: raylib.is_key_pressed(KeyboardKey::KEY_KP_DECIMAL),
        ..PlayerInput::default()
    }
}

fn selected_upgrade(raylib: &RaylibHandle) -> Option<usize> {
    if raylib.is_key_pressed(KeyboardKey::KEY_ONE) {
        Some(0)
    } else if raylib.is_key_pressed(KeyboardKey::KEY_TWO) {
        Some(1)
    } else if raylib.is_key_pressed(KeyboardKey::KEY_THREE) {
        Some(2)
    } else if raylib.is_key_pressed(KeyboardKey::KEY_FOUR) {
        Some(3)
    } else if raylib.is_key_pressed(KeyboardKey::KEY_FIVE) {
        Some(4)
    } else if raylib.is_key_pressed(KeyboardKey::KEY_SIX) {
        Some(5)
    } else if raylib.is_key_pressed(KeyboardKey::KEY_SEVEN) {
        Some(6)
    } else if raylib.is_key_pressed(KeyboardKey::KEY_EIGHT) {
        Some(7)
    } else {
        None
    }
}

const fn horizontal_axis(left: bool, right: bool) -> f32 {
    match (left, right) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        _ => 0.0,
    }
}

fn gamepad_axis(value: f32) -> f32 {
    if value.abs() < GAMEPAD_DEADZONE {
        0.0
    } else {
        value.clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        GamepadInputButton, GamepadInputState, PlayerInput, combine_player_input, map_gamepad_state,
    };

    #[test]
    fn gamepad_state_maps_stick_and_buttons_to_player_input() {
        let input = map_gamepad_state(&GamepadInputState {
            left_x: 0.7,
            left_y: 0.8,
            buttons: BTreeSet::from([
                GamepadInputButton::Thrust,
                GamepadInputButton::Interact,
                GamepadInputButton::Bomb,
                GamepadInputButton::Scan,
            ]),
        });

        assert!(roughly_eq(input.horizontal, 0.7));
        assert!(input.thrust);
        assert!(input.drill_down);
        assert!(input.interact);
        assert!(input.confirm);
        assert!(input.bomb);
        assert!(input.scan);
    }

    #[test]
    fn gamepad_state_applies_deadzone_and_clamps_axis() {
        let deadzone = map_gamepad_state(&GamepadInputState {
            left_x: 0.1,
            left_y: 0.1,
            ..GamepadInputState::default()
        });
        let clamped = map_gamepad_state(&GamepadInputState {
            left_x: -1.5,
            left_y: 1.5,
            ..GamepadInputState::default()
        });

        assert!(roughly_eq(deadzone.horizontal, 0.0));
        assert!(!deadzone.drill_down);
        assert!(roughly_eq(clamped.horizontal, -1.0));
        assert!(clamped.drill_down);
    }

    #[test]
    fn combined_player_input_allows_keyboard_and_gamepad_to_share_local_slot() {
        let keyboard = PlayerInput {
            horizontal: -0.5,
            thrust: true,
            text_input: Some('a'),
            ..PlayerInput::default()
        };
        let gamepad = PlayerInput {
            horizontal: 0.9,
            bomb: true,
            scan: true,
            text_backspace: true,
            ..PlayerInput::default()
        };

        let combined = combine_player_input(keyboard, gamepad);

        assert!(roughly_eq(combined.horizontal, 0.9));
        assert!(combined.thrust);
        assert!(combined.bomb);
        assert!(combined.scan);
        assert_eq!(combined.text_input, Some('a'));
        assert!(combined.text_backspace);
    }

    fn roughly_eq(left: f32, right: f32) -> bool {
        (left - right).abs() < f32::EPSILON
    }
}
