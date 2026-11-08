//! Input devices and which one each seat used last (drives the button prompts).

/// A keyboard control scheme.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum KeyboardLayout {
    /// Playing alone: WASD and the arrow keys both move, every hotkey works.
    #[default]
    Full,
    /// Split-screen seat 1: WASD plus the hotkeys (E use, Esc back, Tab, M ...).
    Wasd,
    /// Split-screen seat 2: arrow keys, Right Ctrl use, Right Shift back, keypad tools.
    Arrows,
}

/// Which face-button glyphs a controller should show (all report the Xbox layout by position).
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum PadFamily {
    #[default]
    Xbox,
    PlayStation,
    Nintendo,
}

impl PadFamily {
    /// Guesses the family from the controller name GLFW reports; unknown pads use Xbox glyphs.
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        let name = name.to_ascii_lowercase();
        let any = |needles: &[&str]| needles.iter().any(|needle| name.contains(needle));
        if any(&["xbox", "xinput", "x-box"]) {
            Self::Xbox
        } else if any(&[
            "playstation",
            "dualshock",
            "dualsense",
            "ps3",
            "ps4",
            "ps5",
            "sony",
        ]) || name == "wireless controller"
        {
            // DualShock 4 pads report a bare "Wireless Controller" on macOS.
            Self::PlayStation
        } else if any(&["nintendo", "switch", "joy-con", "pro controller"]) {
            Self::Nintendo
        } else {
            Self::Xbox
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Xbox => "Xbox",
            Self::PlayStation => "PlayStation",
            Self::Nintendo => "Nintendo",
        }
    }
}

/// The device a seat's prompts are drawn for.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum InputDevice {
    /// Keyboard and mouse, in one of the keyboard layouts.
    Keyboard(KeyboardLayout),
    /// A controller by raylib/GLFW index.
    Gamepad { index: u8, family: PadFamily },
    /// A split-screen seat no device drives yet (waiting for a controller to claim it).
    #[default]
    Unassigned,
}

impl InputDevice {
    /// The solo keyboard (the default before any input).
    pub const KEYBOARD: Self = Self::Keyboard(KeyboardLayout::Full);

    /// Whether this is a controller.
    #[must_use]
    pub const fn is_gamepad(self) -> bool {
        matches!(self, Self::Gamepad { .. })
    }

    /// Short description for the F3 overlay.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Keyboard(KeyboardLayout::Full) => "keyboard + mouse".to_owned(),
            Self::Keyboard(KeyboardLayout::Wasd) => "keyboard (WASD)".to_owned(),
            Self::Keyboard(KeyboardLayout::Arrows) => "keyboard (arrow keys)".to_owned(),
            Self::Gamepad { index, family } => {
                format!(
                    "gamepad {} ({} glyphs)",
                    u32::from(index) + 1,
                    family.label()
                )
            }
            Self::Unassigned => "none (press A or Start on a controller)".to_owned(),
        }
    }
}

/// Tracks the last device that produced meaningful input for one seat.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SeatActivity {
    current: Option<InputDevice>,
}

impl SeatActivity {
    /// Records this frame's `active` devices (those that produced new deliberate input) and
    /// returns the seat's prompt device. The current device wins ties, so prompts only switch
    /// when the *other* device is used; with no activity yet the seat shows `fallback`.
    pub fn observe(&mut self, active: &[InputDevice], fallback: InputDevice) -> InputDevice {
        let keep = self
            .current
            .is_some_and(|current| active.contains(&current));
        if !keep && let Some(first) = active.first() {
            self.current = Some(*first);
        }
        self.current.unwrap_or(fallback)
    }

    /// Forgets the gamepad with `index`, whatever family it reported.
    pub const fn forget_pad(&mut self, index: u8) {
        if matches!(self.current, Some(InputDevice::Gamepad { index: current, .. }) if current == index)
        {
            self.current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAD: InputDevice = InputDevice::Gamepad {
        index: 0,
        family: PadFamily::Xbox,
    };

    #[test]
    fn prompts_follow_the_last_device_used() {
        let mut seat = SeatActivity::default();
        assert_eq!(
            seat.observe(&[], InputDevice::KEYBOARD),
            InputDevice::KEYBOARD
        );
        assert_eq!(seat.observe(&[PAD], InputDevice::KEYBOARD), PAD);
        assert_eq!(
            seat.observe(&[], InputDevice::KEYBOARD),
            PAD,
            "idle keeps the pad"
        );
        assert_eq!(
            seat.observe(&[InputDevice::KEYBOARD], InputDevice::KEYBOARD),
            InputDevice::KEYBOARD,
            "a key press switches back at once"
        );
        assert_eq!(
            seat.observe(&[PAD, InputDevice::KEYBOARD], InputDevice::KEYBOARD),
            InputDevice::KEYBOARD,
            "the current device wins a tie"
        );
    }

    #[test]
    fn an_unplugged_pad_falls_back() {
        let mut seat = SeatActivity::default();
        seat.observe(&[PAD], InputDevice::KEYBOARD);
        seat.forget_pad(1);
        assert_eq!(seat.observe(&[], InputDevice::KEYBOARD), PAD);
        seat.forget_pad(0);
        assert_eq!(
            seat.observe(&[], InputDevice::KEYBOARD),
            InputDevice::KEYBOARD
        );
    }

    #[test]
    fn pad_families_come_from_controller_names() {
        for (name, family) in [
            ("Xbox Wireless Controller", PadFamily::Xbox),
            ("Xbox Series X Controller", PadFamily::Xbox),
            ("DualSense Wireless Controller", PadFamily::PlayStation),
            ("PS4 Controller", PadFamily::PlayStation),
            ("Wireless Controller", PadFamily::PlayStation),
            ("Nintendo Switch Pro Controller", PadFamily::Nintendo),
            ("Pro Controller", PadFamily::Nintendo),
            ("8BitDo SN30 Pro", PadFamily::Xbox),
            ("", PadFamily::Xbox),
        ] {
            assert_eq!(PadFamily::from_name(name), family, "{name}");
        }
    }
}
