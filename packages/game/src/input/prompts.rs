//! Action-based button prompts: what to show for an action on a given device.
//!
//! UI code names an action ([`Prompt`]) and the seat's active [`InputDevice`]; [`glyphs`] returns
//! the keys or controller buttons to draw (rendering turns them into pixel keycaps and pad
//! glyphs) and [`inline_text`] a short text form for sentences and toasts. Controller buttons come
//! from the binding table in [`super::gamepad`], so prompts follow any change to it.

use super::{
    devices::{InputDevice, KeyboardLayout, PadFamily},
    gamepad::{PadAction, PadAxis, PadButton, SCROLL_AXIS, primary_button},
    touch::touch_labels,
};

/// Something the player can do, as shown in a prompt.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Prompt {
    /// Move the menu selection up/down.
    Navigate,
    /// Change a value / move left-right in a menu.
    Adjust,
    /// Confirm the selected row.
    Confirm,
    /// Back out / close / leave.
    Back,
    /// Use a building, counter or door.
    Use,
    /// Walk / drive left-right.
    Walk,
    Thrust,
    Drill,
    Scan,
    /// Drop, throw or place the selected explosive.
    Bomb,
    /// Select the next carried explosive.
    CycleExplosive,
    /// Fire the remote detonator: every waiting C4 at once.
    Detonate,
    /// Fire the remote detonator in sequence (a controller holds the detonate button).
    DetonateSequence,
    /// Aim a throw freely (a controller's right stick; keyboards aim with the movement keys).
    Aim,
    /// Fire the hardpoint weapon (held).
    FireWeapon,
    /// Swap the next owned weapon onto the hardpoint.
    CycleWeapon,
    /// Aim the hardpoint weapon (the mouse, or a controller's right stick).
    AimWeapon,
    /// Previous/next tab or column.
    Tab,
    Pause,
    Map,
    Inventory,
    Help,
    Fullscreen,
    Volume,
    QuickSave,
    QuickLoad,
    /// Paste from the clipboard into a text field.
    Paste,
    /// Delete the last typed character.
    Delete,
    /// Place infrastructure kits (one key per kit).
    Infrastructure,
    /// The diagnostics overlay.
    Details,
    /// The network overlay.
    NetOverlay,
    /// A controller claiming a split-screen seat.
    Join,
    /// Scroll a dialog whose content is taller than it (wheel, Page Up/Down, right stick).
    Scroll,
    /// Enter or leave build mode.
    BuildMode,
    /// Build mode: place the selected piece.
    BuildPlace,
    /// Build mode: deconstruct what the cursor is on.
    BuildRemove,
    /// Build mode: rotate the selected piece.
    BuildRotate,
    /// Build mode: previous / next piece.
    BuildCycle,
    /// Build mode: move the cursor.
    BuildCursor,
    // --- On foot (Part II F5): the seat's prompts switch to these while its pilot is out. ---
    /// Jump (held in the air: the jetpack).
    Jump,
    /// Climb a ladder or rope up / down.
    Climb,
    /// Crouch and crawl.
    Crouch,
    /// Swing the hand pick (held).
    Pick,
    /// Climb out of the rig.
    ExitRig,
    /// Board the rig.
    BoardRig,
    /// The rope: grapple, winch, let go (on foot, WP-PILOT).
    Rope,
    /// The Gravity Anchor (in the rig, Wave 9).
    Anchor,
}

/// A controller glyph: a button or a direction cluster.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PadGlyph {
    Button(PadButton),
    /// D-pad with up and down lit.
    DPadVertical,
    /// D-pad with left and right lit.
    DPadHorizontal,
    /// D-pad with down lit.
    DPadDown,
    /// The left stick.
    LeftStick,
    /// The right stick (dialog scrolling).
    RightStick,
}

/// One drawable piece of a prompt.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Glyph {
    /// A keyboard keycap with this label.
    Key(&'static str),
    /// A controller glyph in a pad family's style.
    Pad(PadGlyph, PadFamily),
    /// The mouse wheel (or a trackpad's two-finger scroll).
    MouseWheel,
}

/// The glyphs to draw for `prompt` on `device`; empty when the device has no binding for it
/// (the prompt is then left out).
#[must_use]
pub fn glyphs(prompt: Prompt, device: InputDevice) -> Vec<Glyph> {
    match device {
        InputDevice::Keyboard(layout) => {
            let keys = keyboard_keys(prompt, layout);
            // The mouse belongs to the main keyboard: its wheel scrolls before the keys do.
            let wheel = (prompt == Prompt::Scroll && !keys.is_empty()).then_some(Glyph::MouseWheel);
            wheel
                .into_iter()
                .chain(keys.iter().map(|key| Glyph::Key(key)))
                .collect()
        }
        InputDevice::Gamepad { family, .. } => pad_glyphs(prompt)
            .into_iter()
            .map(|glyph| Glyph::Pad(glyph, family))
            .collect(),
        // Touch prompts are words on a keycap; most actions are on-screen buttons instead.
        InputDevice::Touch => touch_labels(prompt)
            .iter()
            .map(|label| Glyph::Key(label))
            .collect(),
        // A seat nobody drives yet shows how a controller joins.
        InputDevice::Unassigned => pad_glyphs(prompt)
            .into_iter()
            .map(|glyph| Glyph::Pad(glyph, PadFamily::Xbox))
            .collect(),
    }
}

/// `prompt`'s keys/buttons as text for a sentence ("E", "A/D", "LB/RB"), or `None` when the
/// device has no binding for it.
#[must_use]
pub fn inline_text(prompt: Prompt, device: InputDevice) -> Option<String> {
    let parts: Vec<&str> = glyphs(prompt, device)
        .into_iter()
        .map(|glyph| match glyph {
            Glyph::Key(label) => label,
            Glyph::Pad(glyph, family) => pad_glyph_name(glyph, family),
            Glyph::MouseWheel => "Wheel",
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Keyboard keys for `prompt` in `layout` (labels as printed on the keycaps).
#[must_use]
pub const fn keyboard_keys(prompt: Prompt, layout: KeyboardLayout) -> &'static [&'static str] {
    use KeyboardLayout::{Arrows, Full, Wasd};
    match (prompt, layout) {
        // On foot (Part II F5) the arrow keys and W/S climb.
        (Prompt::Navigate, Full | Arrows) | (Prompt::Climb, Arrows) => &["\u{2191}", "\u{2193}"],
        (Prompt::Navigate, Wasd) | (Prompt::Climb, Full | Wasd) => &["W", "S"],
        (Prompt::Adjust | Prompt::Tab, Full | Arrows) | (Prompt::Walk, Arrows) => {
            &["\u{2190}", "\u{2192}"]
        }
        (Prompt::Adjust | Prompt::Tab | Prompt::Walk, Wasd) | (Prompt::Walk, Full) => &["A", "D"],
        (Prompt::Confirm, Full | Wasd) => &["Enter"],
        (
            Prompt::Confirm | Prompt::Use | Prompt::BuildPlace | Prompt::ExitRig | Prompt::BoardRig,
            Arrows,
        ) => &["RCtrl"],
        (Prompt::Back | Prompt::Pause, Full | Wasd) => &["Esc"],
        (Prompt::Back, Arrows) => &["RShift"],
        (Prompt::Pause, Arrows) => &["RAlt"],
        (Prompt::Inventory, Arrows) => &["Num1"],
        (Prompt::Map, Arrows) => &["Num2"],
        (Prompt::Use | Prompt::ExitRig | Prompt::BoardRig, Full | Wasd)
        | (Prompt::BuildPlace, Wasd) => &["E"],
        (Prompt::Thrust, Full | Wasd) => &["W"],
        // The arrow-keys seat jumps with up.
        (Prompt::Thrust | Prompt::Jump, Arrows) => &["\u{2191}"],
        (Prompt::Drill | Prompt::Crouch, Full | Wasd) => &["S"],
        (Prompt::Drill | Prompt::Crouch, Arrows) => &["\u{2193}"],
        (Prompt::Scan, Full | Wasd) => &["C"],
        (Prompt::Scan, Arrows) => &["Num."],
        (Prompt::Bomb, Full | Wasd) => &["B"],
        (Prompt::Bomb, Arrows) => &["Num0"],
        (Prompt::CycleExplosive, Full | Wasd) => &["Q"],
        (Prompt::CycleExplosive, Arrows) => &["Num3"],
        (Prompt::Detonate, Full | Wasd) => &["F"],
        (Prompt::Detonate, Arrows) => &["Num4"],
        (Prompt::DetonateSequence, Full | Wasd) => &["G"],
        (Prompt::DetonateSequence, Arrows) => &["Num5"],
        (Prompt::Aim, Full | Wasd) | (Prompt::BuildCursor, Wasd) => &["W", "A", "S", "D"],
        (Prompt::Aim | Prompt::BuildCursor, Arrows) => {
            &["\u{2190}", "\u{2191}", "\u{2192}", "\u{2193}"]
        }
        // On foot the trigger swings the hand pick.
        (Prompt::FireWeapon | Prompt::Pick, Full | Wasd) => &["LMB", "V"],
        // Build mode shares the weapon keys (weapons hold fire while a seat builds).
        (Prompt::FireWeapon | Prompt::BuildRemove | Prompt::Pick, Arrows) => &["Num6"],
        (Prompt::CycleWeapon, Full | Wasd) | (Prompt::BuildRemove, Wasd) => &["X"],
        (Prompt::CycleWeapon | Prompt::BuildRotate, Arrows) => &["Num7"],
        (Prompt::AimWeapon, Full | Wasd) => &["Mouse"],
        (Prompt::Map, Full | Wasd) => &["M"],
        (Prompt::Inventory, Full | Wasd) => &["Tab"],
        (Prompt::Help, Full | Wasd) => &["H"],
        (Prompt::Fullscreen, Full | Wasd) => &["F11"],
        (Prompt::Volume, Full | Wasd) => &["-", "+"],
        (Prompt::QuickSave, Full | Wasd) => &["F5"],
        (Prompt::QuickLoad, Full | Wasd) => &["F9"],
        (Prompt::Paste, Full | Wasd) => &["Ctrl+V"],
        (Prompt::Delete, Full | Wasd) => &["Bksp"],
        (Prompt::Infrastructure, Full | Wasd) => &["R", "T", "L", "U", "O", "P"],
        (Prompt::Details, Full | Wasd) => &["Ctrl+Tab"],
        (Prompt::NetOverlay, Full | Wasd) => &["F3"],
        (Prompt::Scroll, Full | Wasd) => &["PgUp", "PgDn"],
        (Prompt::BuildMode, Full | Wasd) => &["N"],
        (Prompt::BuildMode, Arrows) => &["Num8"],
        (Prompt::BuildPlace, Full) => &["Click", "E"],
        (Prompt::BuildRemove, Full) => &["RClick", "X"],
        // On foot R throws the rope; in build mode it rotates.
        (Prompt::BuildRotate | Prompt::Rope, Full | Wasd) => &["R"],
        (Prompt::BuildCycle, Full | Wasd) => &["Z", "Q"],
        (Prompt::BuildCycle, Arrows) => &["Num9", "Num3"],
        (Prompt::BuildCursor, Full) => &["Mouse", "W", "A", "S", "D"],
        (Prompt::Jump, Full | Wasd) => &["Space"],
        (Prompt::Rope | Prompt::Anchor, Arrows) => &["Num9"],
        (Prompt::Anchor, Full | Wasd) => &["/"],
        // The arrow-keys seat leaves the global hotkeys and scrolling to the main keyboard,
        // and keyboards never "join".
        (
            Prompt::Help
            | Prompt::Fullscreen
            | Prompt::Volume
            | Prompt::QuickSave
            | Prompt::QuickLoad
            | Prompt::Paste
            | Prompt::Delete
            | Prompt::Infrastructure
            | Prompt::Details
            | Prompt::NetOverlay
            | Prompt::Scroll
            | Prompt::AimWeapon,
            Arrows,
        )
        | (Prompt::Join, _) => &[],
    }
}

/// Controller glyphs for `prompt`.
#[must_use]
pub fn pad_glyphs(prompt: Prompt) -> Vec<PadGlyph> {
    let button = |action| primary_button(action).map(PadGlyph::Button);
    let both = |first, second| button(first).into_iter().chain(button(second)).collect();
    match prompt {
        Prompt::Navigate | Prompt::Climb => vec![PadGlyph::DPadVertical],
        Prompt::Adjust => vec![PadGlyph::DPadHorizontal],
        Prompt::Walk => vec![PadGlyph::LeftStick],
        Prompt::Scroll => vec![match SCROLL_AXIS {
            PadAxis::LeftX | PadAxis::LeftY => PadGlyph::LeftStick,
            PadAxis::RightX | PadAxis::RightY => PadGlyph::RightStick,
        }],
        Prompt::Drill | Prompt::Crouch => vec![PadGlyph::DPadDown],
        Prompt::Confirm | Prompt::Join | Prompt::BuildPlace => {
            button(PadAction::Confirm).into_iter().collect()
        }
        Prompt::Back => button(PadAction::Back).into_iter().collect(),
        // In build mode the use button deconstructs.
        Prompt::Use | Prompt::BuildRemove => button(PadAction::Interact).into_iter().collect(),
        Prompt::Thrust => button(PadAction::Thrust).into_iter().collect(),
        Prompt::Scan => button(PadAction::Scan).into_iter().collect(),
        Prompt::Bomb => button(PadAction::Bomb).into_iter().collect(),
        Prompt::CycleExplosive => button(PadAction::CycleExplosive).into_iter().collect(),
        // A tap fires everything; holding the same button fires the sequence.
        Prompt::Detonate | Prompt::DetonateSequence => {
            button(PadAction::Detonate).into_iter().collect()
        }
        Prompt::Aim | Prompt::AimWeapon => vec![PadGlyph::RightStick],
        Prompt::FireWeapon => button(PadAction::FireWeapon).into_iter().collect(),
        Prompt::CycleWeapon => button(PadAction::CycleWeapon).into_iter().collect(),
        Prompt::Tab | Prompt::BuildCycle => both(PadAction::TabPrevious, PadAction::TabNext),
        Prompt::Pause => button(PadAction::Pause).into_iter().collect(),
        Prompt::Map => button(PadAction::Map).into_iter().collect(),
        Prompt::Inventory => button(PadAction::Inventory).into_iter().collect(),
        Prompt::BuildMode => button(PadAction::BuildMode).into_iter().collect(),
        Prompt::BuildRotate => button(PadAction::BuildRotate).into_iter().collect(),
        Prompt::BuildCursor => vec![PadGlyph::DPadVertical, PadGlyph::DPadHorizontal],
        Prompt::Jump => button(PadAction::Jump).into_iter().collect(),
        Prompt::Pick => button(PadAction::SwingPick).into_iter().collect(),
        Prompt::ExitRig | Prompt::BoardRig => button(PadAction::BoardRig).into_iter().collect(),
        Prompt::Rope => button(PadAction::Rope).into_iter().collect(),
        Prompt::Anchor => button(PadAction::Anchor).into_iter().collect(),
        // Keyboard-only: no controller binding.
        Prompt::Help
        | Prompt::Fullscreen
        | Prompt::Volume
        | Prompt::QuickSave
        | Prompt::QuickLoad
        | Prompt::Paste
        | Prompt::Delete
        | Prompt::Infrastructure
        | Prompt::Details
        | Prompt::NetOverlay => Vec::new(),
    }
}

/// What a button is called on a controller of `family` (face buttons follow the printed
/// symbol at that position; GLFW reports positions in the Xbox layout).
#[must_use]
pub const fn pad_button_name(button: PadButton, family: PadFamily) -> &'static str {
    use PadFamily::{Nintendo, PlayStation, Xbox};
    match (button, family) {
        (PadButton::A, Xbox) | (PadButton::B, Nintendo) => "A",
        (PadButton::B, Xbox) | (PadButton::A, Nintendo) => "B",
        (PadButton::X, Xbox) | (PadButton::Y, Nintendo) => "X",
        (PadButton::Y, Xbox) | (PadButton::X, Nintendo) => "Y",
        (PadButton::A, PlayStation) => "Cross",
        (PadButton::B, PlayStation) => "Circle",
        (PadButton::X, PlayStation) => "Square",
        (PadButton::Y, PlayStation) => "Triangle",
        (PadButton::LeftBumper, Xbox) => "LB",
        (PadButton::RightBumper, Xbox) => "RB",
        (PadButton::LeftTrigger, Xbox) => "LT",
        (PadButton::RightTrigger, Xbox) => "RT",
        (PadButton::LeftBumper, PlayStation) => "L1",
        (PadButton::RightBumper, PlayStation) => "R1",
        (PadButton::LeftTrigger, PlayStation) => "L2",
        (PadButton::RightTrigger, PlayStation) => "R2",
        (PadButton::LeftBumper, Nintendo) => "L",
        (PadButton::RightBumper, Nintendo) => "R",
        (PadButton::LeftTrigger, Nintendo) => "ZL",
        (PadButton::RightTrigger, Nintendo) => "ZR",
        (PadButton::View, Xbox) => "View",
        (PadButton::View, PlayStation) => "Share",
        (PadButton::View, Nintendo) => "Minus",
        (PadButton::Menu, Xbox) => "Menu",
        (PadButton::Menu, PlayStation) => "Options",
        (PadButton::Menu, Nintendo) => "Plus",
        (PadButton::Guide, _) => "Home",
        (PadButton::LeftStick, _) => "L3",
        (PadButton::RightStick, _) => "R3",
        (PadButton::DPadUp, _) => "D-pad up",
        (PadButton::DPadDown, _) => "D-pad down",
        (PadButton::DPadLeft, _) => "D-pad left",
        (PadButton::DPadRight, _) => "D-pad right",
    }
}

/// Text name of a controller glyph.
#[must_use]
pub const fn pad_glyph_name(glyph: PadGlyph, family: PadFamily) -> &'static str {
    match glyph {
        PadGlyph::Button(button) => pad_button_name(button, family),
        PadGlyph::DPadVertical => "D-pad up/down",
        PadGlyph::DPadHorizontal => "D-pad left/right",
        PadGlyph::DPadDown => "D-pad down",
        PadGlyph::LeftStick => "Left stick",
        PadGlyph::RightStick => "Right stick",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XBOX: InputDevice = InputDevice::Gamepad {
        index: 0,
        family: PadFamily::Xbox,
    };

    #[test]
    fn keyboard_prompts_show_the_keys_of_the_seats_layout() {
        assert_eq!(
            glyphs(Prompt::Use, InputDevice::KEYBOARD),
            vec![Glyph::Key("E")]
        );
        let arrows = InputDevice::Keyboard(KeyboardLayout::Arrows);
        assert_eq!(glyphs(Prompt::Use, arrows), vec![Glyph::Key("RCtrl")]);
        assert_eq!(glyphs(Prompt::Confirm, arrows), vec![Glyph::Key("RCtrl")]);
        assert_eq!(inline_text(Prompt::Back, arrows).as_deref(), Some("RShift"));
        assert_eq!(inline_text(Prompt::Pause, arrows).as_deref(), Some("RAlt"));
        assert_eq!(
            inline_text(Prompt::Navigate, arrows).as_deref(),
            Some("\u{2191}/\u{2193}")
        );
        assert_eq!(
            inline_text(Prompt::Inventory, arrows).as_deref(),
            Some("Num1")
        );
        assert_eq!(inline_text(Prompt::Map, arrows).as_deref(), Some("Num2"));
        assert_eq!(
            inline_text(Prompt::Walk, InputDevice::KEYBOARD).as_deref(),
            Some("A/D")
        );
        assert_eq!(
            inline_text(Prompt::Back, InputDevice::KEYBOARD).as_deref(),
            Some("Esc")
        );
    }

    #[test]
    fn controller_prompts_follow_the_binding_table() {
        let pad = |prompt| glyphs(prompt, XBOX);
        let button = |button| Glyph::Pad(PadGlyph::Button(button), PadFamily::Xbox);
        assert_eq!(pad(Prompt::Confirm), vec![button(PadButton::A)]);
        assert_eq!(pad(Prompt::Back), vec![button(PadButton::B)]);
        assert_eq!(pad(Prompt::Use), vec![button(PadButton::X)]);
        assert_eq!(pad(Prompt::Inventory), vec![button(PadButton::Y)]);
        assert_eq!(pad(Prompt::Map), vec![button(PadButton::View)]);
        assert_eq!(pad(Prompt::Pause), vec![button(PadButton::Menu)]);
        assert_eq!(
            pad(Prompt::Tab),
            vec![
                button(PadButton::LeftBumper),
                button(PadButton::RightBumper)
            ]
        );
        assert_eq!(
            pad(Prompt::Navigate),
            vec![Glyph::Pad(PadGlyph::DPadVertical, PadFamily::Xbox)]
        );
        assert!(
            pad(Prompt::Fullscreen).is_empty(),
            "keyboard-only prompts hide"
        );
        assert_eq!(inline_text(Prompt::Tab, XBOX).as_deref(), Some("LB/RB"));
    }

    #[test]
    fn other_pad_families_name_their_own_buttons() {
        let ps = InputDevice::Gamepad {
            index: 1,
            family: PadFamily::PlayStation,
        };
        let nintendo = InputDevice::Gamepad {
            index: 2,
            family: PadFamily::Nintendo,
        };
        assert_eq!(inline_text(Prompt::Confirm, ps).as_deref(), Some("Cross"));
        assert_eq!(inline_text(Prompt::Back, ps).as_deref(), Some("Circle"));
        // Nintendo prints B at the bottom (Xbox A) position.
        assert_eq!(inline_text(Prompt::Confirm, nintendo).as_deref(), Some("B"));
        assert_eq!(inline_text(Prompt::Tab, nintendo).as_deref(), Some("L/R"));
    }

    #[test]
    fn scroll_prompts_show_the_wheel_and_page_keys_or_the_right_stick() {
        assert_eq!(
            glyphs(Prompt::Scroll, InputDevice::KEYBOARD),
            vec![Glyph::MouseWheel, Glyph::Key("PgUp"), Glyph::Key("PgDn")]
        );
        assert_eq!(
            glyphs(
                Prompt::Scroll,
                InputDevice::Keyboard(KeyboardLayout::Arrows)
            ),
            []
        );
        assert_eq!(
            glyphs(Prompt::Scroll, XBOX),
            vec![Glyph::Pad(PadGlyph::RightStick, PadFamily::Xbox)]
        );
        assert_eq!(
            inline_text(Prompt::Scroll, InputDevice::KEYBOARD).as_deref(),
            Some("Wheel/PgUp/PgDn")
        );
        assert_eq!(
            inline_text(Prompt::Scroll, XBOX).as_deref(),
            Some("Right stick")
        );
    }

    #[test]
    fn unassigned_seats_show_how_to_join() {
        assert_eq!(
            glyphs(Prompt::Join, InputDevice::Unassigned),
            vec![Glyph::Pad(PadGlyph::Button(PadButton::A), PadFamily::Xbox)]
        );
        assert_eq!(glyphs(Prompt::Join, InputDevice::KEYBOARD), []);
    }
}
