//! Device input: keyboard sampling, the gamepad mapping, seat routing and button prompts.
//!
//! [`PlayerInput`] is one seat's frame of intent. Keyboards are read straight from raylib here;
//! controllers are sampled into [`gamepad::PadState`]s and mapped by [`gamepad::PadMapper`];
//! [`router::InputRouter`] assigns devices to seats and tracks each seat's active device, which
//! [`prompts`] turns into keycaps or controller glyphs.

pub mod devices;
pub mod gamepad;
pub mod pads;
pub mod prompts;
pub mod repeat;
pub mod router;
pub mod seats;

use raylib::prelude::*;

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
    /// Esc-style cancel (keyboard): backs out of menus, and also pauses play or quits from the
    /// title.
    pub cancel: bool,
    /// Gamepad Back (B): backs out of menus, modals, forms and buildings only; never pauses
    /// play or quits.
    pub back: bool,
    pub pause: bool,
    pub menu_up: bool,
    pub menu_down: bool,
    pub menu_left: bool,
    pub menu_right: bool,
    /// Previous tab / column (gamepad LB in menus).
    pub tab_previous: bool,
    /// Next tab / column (gamepad RB in menus).
    pub tab_next: bool,
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
    /// Dialog scrolling in mouse-wheel notches (positive scrolls up): the wheel or trackpad,
    /// plus the controller's right stick (already scaled by frame time).
    pub ui_scroll: f32,
    /// F3: toggle the network debug overlay.
    pub net_debug: bool,
    /// F4 (dev, overlay open): select the next live tuning value.
    pub tune_next: bool,
    /// Page Up: scrolls a dialog up a page. With the F3 overlay open in a dev build it raises
    /// the selected tuning value instead (see `ShellState::tuning_keys_active`).
    pub page_up: bool,
    /// Page Down: scrolls a dialog down a page, or lowers the tuning value (as `page_up`).
    pub page_down: bool,
    /// Home: scrolls a dialog to the top.
    pub scroll_home: bool,
    /// End: scrolls a dialog to the bottom.
    pub scroll_end: bool,
    /// Ctrl+V: paste the clipboard into the focused text field.
    pub paste: bool,
}

impl PlayerInput {
    /// Whether this input backs out of the current menu (Esc/Backspace or gamepad B).
    #[must_use]
    pub const fn backs_out(&self) -> bool {
        self.cancel || self.back
    }

    /// Whether the input shows deliberate use of its device (any key, button or movement),
    /// for switching a seat's prompts to it. Closing the window does not count.
    #[must_use]
    pub fn has_activity(&self) -> bool {
        let Self {
            horizontal,
            thrust,
            drill_down,
            interact,
            confirm,
            cancel,
            back,
            pause,
            menu_up,
            menu_down,
            menu_left,
            menu_right,
            tab_previous,
            tab_next,
            details,
            inventory,
            save,
            selected_upgrade,
            load,
            map,
            help,
            volume_up,
            volume_down,
            fullscreen,
            local_multiplayer_toggle,
            bomb,
            scan,
            place_relay,
            place_drone,
            place_lift,
            place_support,
            place_pump,
            place_processor,
            exit_requested: _,
            text_input,
            text_backspace,
            ui_scroll,
            net_debug,
            tune_next,
            page_up,
            page_down,
            scroll_home,
            scroll_end,
            paste,
        } = *self;
        horizontal.abs() > f32::EPSILON
            || ui_scroll.abs() > f32::EPSILON
            || selected_upgrade.is_some()
            || text_input.is_some()
            || [
                thrust,
                drill_down,
                interact,
                confirm,
                cancel,
                back,
                pause,
                menu_up,
                menu_down,
                menu_left,
                menu_right,
                tab_previous,
                tab_next,
                details,
                inventory,
                save,
                load,
                map,
                help,
                volume_up,
                volume_down,
                fullscreen,
                local_multiplayer_toggle,
                bomb,
                scan,
                place_relay,
                place_drone,
                place_lift,
                place_support,
                place_pump,
                place_processor,
                text_backspace,
                net_debug,
                tune_next,
                page_up,
                page_down,
                scroll_home,
                scroll_end,
                paste,
            ]
            .into_iter()
            .any(|pressed| pressed)
    }
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
        back: primary.back || secondary.back,
        pause: primary.pause || secondary.pause,
        menu_up: primary.menu_up || secondary.menu_up,
        menu_down: primary.menu_down || secondary.menu_down,
        menu_left: primary.menu_left || secondary.menu_left,
        menu_right: primary.menu_right || secondary.menu_right,
        tab_previous: primary.tab_previous || secondary.tab_previous,
        tab_next: primary.tab_next || secondary.tab_next,
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
        page_up: primary.page_up || secondary.page_up,
        page_down: primary.page_down || secondary.page_down,
        scroll_home: primary.scroll_home || secondary.scroll_home,
        scroll_end: primary.scroll_end || secondary.scroll_end,
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
        back: false,
        tab_previous: false,
        tab_next: false,
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
        page_up: raylib.is_key_pressed(KeyboardKey::KEY_PAGE_UP)
            || raylib.is_key_pressed_repeat(KeyboardKey::KEY_PAGE_UP),
        page_down: raylib.is_key_pressed(KeyboardKey::KEY_PAGE_DOWN)
            || raylib.is_key_pressed_repeat(KeyboardKey::KEY_PAGE_DOWN),
        scroll_home: raylib.is_key_pressed(KeyboardKey::KEY_HOME),
        scroll_end: raylib.is_key_pressed(KeyboardKey::KEY_END),
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

#[cfg(test)]
mod tests {
    use super::{PlayerInput, combine_player_input};

    #[test]
    fn back_backs_out_without_counting_as_cancel() {
        let back = PlayerInput {
            back: true,
            ..PlayerInput::default()
        };
        assert!(back.backs_out() && !back.cancel);
        assert!(back.has_activity());
        assert!(!PlayerInput::default().has_activity());
        assert!(
            !PlayerInput {
                exit_requested: true,
                ..PlayerInput::default()
            }
            .has_activity(),
            "closing the window is not keyboard use"
        );
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
