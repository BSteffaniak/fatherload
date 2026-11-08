//! Gamepad mapping: one sampled pad state ([`PadState`]) to a [`PlayerInput`] through a single
//! binding table ([`PAD_BINDINGS`]).
//!
//! Buttons use the Xbox layout as their canonical names. GLFW's standard gamepad mapping (which
//! raylib reads) reports every supported controller in that layout by *position*, so `A` is the
//! bottom face button on a Sony or Nintendo pad too. Nothing here touches raylib: the app
//! samples a [`PadState`] per connected pad and a [`PadMapper`] (one per pad, it remembers the
//! previous frame for edges and menu auto-repeat) turns it into input, so the whole mapping is
//! testable without a window.

use std::collections::BTreeSet;

use super::{PlayerInput, repeat::MenuRepeat};

/// Stick travel ignored as noise (analog walking starts past it).
pub const STICK_DEADZONE: f32 = 0.25;
/// Stick travel that counts as a digital direction (thrust, drill, menu steps).
pub const DIRECTION_THRESHOLD: f32 = 0.5;
/// Trigger travel (0..1) that counts as pressing it.
pub const TRIGGER_THRESHOLD: f32 = 0.5;

/// A gamepad button, named for its position on an Xbox controller.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PadButton {
    /// Bottom face button.
    A,
    /// Right face button.
    B,
    /// Left face button.
    X,
    /// Top face button.
    Y,
    LeftBumper,
    RightBumper,
    /// Left trigger pulled past [`TRIGGER_THRESHOLD`].
    LeftTrigger,
    /// Right trigger pulled past [`TRIGGER_THRESHOLD`].
    RightTrigger,
    /// View / Back / Select / Share / Minus (left of centre).
    View,
    /// Menu / Start / Options / Plus (right of centre).
    Menu,
    /// Guide / PS / Home (centre).
    Guide,
    LeftStick,
    RightStick,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

/// What a gamepad button can do. Directions (left stick and d-pad) are fixed: horizontal walks
/// and steps menus left/right, up thrusts and steps up, down drills and steps down.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PadAction {
    /// Held: fire the thruster (also: up on the stick or d-pad).
    Thrust,
    /// Pressed: confirm the selected menu row (and call the rescue crew).
    Confirm,
    /// Pressed: use a building, counter or door.
    Interact,
    /// Pressed: back out of a menu, modal, form or building (never pauses or quits).
    Back,
    /// Pressed: toggle the inventory.
    Inventory,
    /// Pressed: toggle the mine map.
    Map,
    /// Pressed: open or close the pause menu.
    Pause,
    /// Pressed: fire the scanner.
    Scan,
    /// Pressed: drop a bomb.
    Bomb,
    /// Pressed: previous tab / column in menus that have them.
    TabPrevious,
    /// Pressed: next tab / column in menus that have them.
    TabNext,
}

/// The controller layout: every `(action, button)` pair. An action may have several buttons
/// and a button several actions (in-play and in-menu meanings). Change controls here; button
/// prompts read this table too ([`primary_button`]), so they always match.
pub const PAD_BINDINGS: &[(PadAction, PadButton)] = &[
    (PadAction::Thrust, PadButton::A),
    (PadAction::Thrust, PadButton::RightTrigger),
    (PadAction::Confirm, PadButton::A),
    (PadAction::Interact, PadButton::X),
    (PadAction::Back, PadButton::B),
    (PadAction::Inventory, PadButton::Y),
    (PadAction::Map, PadButton::View),
    (PadAction::Pause, PadButton::Menu),
    (PadAction::Scan, PadButton::LeftBumper),
    (PadAction::Bomb, PadButton::RightBumper),
    (PadAction::TabPrevious, PadButton::LeftBumper),
    (PadAction::TabNext, PadButton::RightBumper),
];

/// The first button bound to `action` (what prompts show).
#[must_use]
pub fn primary_button(action: PadAction) -> Option<PadButton> {
    PAD_BINDINGS
        .iter()
        .find(|(bound, _)| *bound == action)
        .map(|(_, button)| *button)
}

/// One pad's sampled state for a frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PadState {
    /// Buttons held down (triggers included once past [`TRIGGER_THRESHOLD`]).
    pub buttons: BTreeSet<PadButton>,
    /// Left stick, -1 (left) to 1 (right).
    pub left_x: f32,
    /// Left stick, -1 (up) to 1 (down).
    pub left_y: f32,
}

impl PadState {
    #[must_use]
    pub fn is_down(&self, button: PadButton) -> bool {
        self.buttons.contains(&button)
    }

    /// Analog horizontal movement: the d-pad (full speed) or the stick past the deadzone.
    #[must_use]
    pub fn horizontal(&self) -> f32 {
        match self.dpad_horizontal() {
            0 => stick_axis(self.left_x),
            direction => f32::from(direction),
        }
    }

    /// Digital horizontal direction (-1, 0, 1) from the d-pad or the stick.
    #[must_use]
    pub fn horizontal_direction(&self) -> i8 {
        match self.dpad_horizontal() {
            0 => stick_direction(self.left_x),
            direction => direction,
        }
    }

    /// Digital vertical direction (-1 up, 0, 1 down) from the d-pad or the stick.
    #[must_use]
    pub fn vertical_direction(&self) -> i8 {
        match axis_from_buttons(
            self.is_down(PadButton::DPadUp),
            self.is_down(PadButton::DPadDown),
        ) {
            0 => stick_direction(self.left_y),
            direction => direction,
        }
    }

    fn dpad_horizontal(&self) -> i8 {
        axis_from_buttons(
            self.is_down(PadButton::DPadLeft),
            self.is_down(PadButton::DPadRight),
        )
    }

    /// Whether the stick is outside the deadzone.
    fn stick_engaged(&self) -> bool {
        self.left_x.abs() >= STICK_DEADZONE || self.left_y.abs() >= STICK_DEADZONE
    }

    /// Whether `button` went down since `previous`.
    #[must_use]
    pub fn pressed_since(&self, previous: &Self, button: PadButton) -> bool {
        self.is_down(button) && !previous.is_down(button)
    }

    /// Whether `action` was pressed since `previous` (any of its buttons).
    #[must_use]
    pub fn action_pressed_since(&self, previous: &Self, action: PadAction) -> bool {
        PAD_BINDINGS
            .iter()
            .any(|(bound, button)| *bound == action && self.pressed_since(previous, *button))
    }

    /// Whether `action` is held (any of its buttons).
    #[must_use]
    pub fn action_held(&self, action: PadAction) -> bool {
        PAD_BINDINGS
            .iter()
            .any(|(bound, button)| *bound == action && self.is_down(*button))
    }

    /// Whether this frame shows *new* deliberate input compared with `previous`: a button went
    /// down or the stick left the deadzone. Stick noise inside the deadzone and a stick or
    /// button merely held do not count, so a drifting stick never steals the prompts.
    #[must_use]
    pub fn new_activity_since(&self, previous: &Self) -> bool {
        self.buttons
            .iter()
            .any(|button| !previous.buttons.contains(button))
            || (self.stick_engaged() && !previous.stick_engaged())
    }
}

/// Turns one pad's frames into [`PlayerInput`]: edges for one-shot actions and auto-repeating
/// menu steps. Keep one per connected pad.
#[derive(Clone, Debug, Default)]
pub struct PadMapper {
    previous: PadState,
    vertical: MenuRepeat,
    horizontal: MenuRepeat,
}

impl PadMapper {
    /// Maps this frame's `state` (`delta_seconds` since the last frame).
    pub fn map(&mut self, state: &PadState, delta_seconds: f32) -> PlayerInput {
        let previous = &self.previous;
        let pressed = |action| state.action_pressed_since(previous, action);
        let vertical = state.vertical_direction();
        // Stick and d-pad are folded into one direction before the repeat timer, so holding
        // both never steps twice.
        let vertical_step = self.vertical.update(vertical, delta_seconds);
        let horizontal_step = self
            .horizontal
            .update(state.horizontal_direction(), delta_seconds);
        let input = PlayerInput {
            horizontal: state.horizontal(),
            thrust: state.action_held(PadAction::Thrust) || vertical < 0,
            drill_down: vertical > 0,
            interact: pressed(PadAction::Interact),
            confirm: pressed(PadAction::Confirm),
            back: pressed(PadAction::Back),
            pause: pressed(PadAction::Pause),
            inventory: pressed(PadAction::Inventory),
            map: pressed(PadAction::Map),
            scan: pressed(PadAction::Scan),
            bomb: pressed(PadAction::Bomb),
            tab_previous: pressed(PadAction::TabPrevious),
            tab_next: pressed(PadAction::TabNext),
            menu_up: vertical_step < 0,
            menu_down: vertical_step > 0,
            menu_left: horizontal_step < 0,
            menu_right: horizontal_step > 0,
            ..PlayerInput::default()
        };
        self.previous.clone_from(state);
        input
    }

    /// The state mapped last frame.
    #[must_use]
    pub const fn previous(&self) -> &PadState {
        &self.previous
    }
}

/// Applies the deadzone and clamps a stick axis.
#[must_use]
pub fn stick_axis(value: f32) -> f32 {
    if value.abs() < STICK_DEADZONE {
        0.0
    } else {
        value.clamp(-1.0, 1.0)
    }
}

fn stick_direction(value: f32) -> i8 {
    if value <= -DIRECTION_THRESHOLD {
        -1
    } else {
        i8::from(value >= DIRECTION_THRESHOLD)
    }
}

const fn axis_from_buttons(negative: bool, positive: bool) -> i8 {
    match (negative, positive) {
        (true, false) => -1,
        (false, true) => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn pad(buttons: &[PadButton]) -> PadState {
        PadState {
            buttons: buttons.iter().copied().collect(),
            ..PadState::default()
        }
    }

    fn stick(left_x: f32, left_y: f32) -> PadState {
        PadState {
            left_x,
            left_y,
            ..PadState::default()
        }
    }

    #[test]
    fn every_action_has_a_button_and_prompts_use_the_first() {
        for action in [
            PadAction::Thrust,
            PadAction::Confirm,
            PadAction::Interact,
            PadAction::Back,
            PadAction::Inventory,
            PadAction::Map,
            PadAction::Pause,
            PadAction::Scan,
            PadAction::Bomb,
            PadAction::TabPrevious,
            PadAction::TabNext,
        ] {
            assert!(primary_button(action).is_some(), "{action:?}");
        }
        assert_eq!(primary_button(PadAction::Thrust), Some(PadButton::A));
        assert_eq!(primary_button(PadAction::Back), Some(PadButton::B));
    }

    #[test]
    fn face_buttons_shoulders_and_menu_buttons_map_on_press() {
        let mut mapper = PadMapper::default();
        let input = mapper.map(
            &pad(&[
                PadButton::A,
                PadButton::B,
                PadButton::X,
                PadButton::Y,
                PadButton::View,
                PadButton::Menu,
                PadButton::LeftBumper,
                PadButton::RightBumper,
            ]),
            DT,
        );
        assert!(input.thrust && input.confirm);
        assert!(
            input.back && !input.cancel,
            "B backs out without Esc semantics"
        );
        assert!(input.interact);
        assert!(input.inventory && input.map && input.pause);
        assert!(input.scan && input.tab_previous);
        assert!(input.bomb && input.tab_next);
    }

    #[test]
    fn one_shot_actions_fire_once_per_press_but_thrust_is_held() {
        let mut mapper = PadMapper::default();
        let held = pad(&[PadButton::A, PadButton::X]);
        let first = mapper.map(&held, DT);
        let second = mapper.map(&held, DT);
        assert!(first.confirm && first.interact && first.thrust);
        assert!(!second.confirm && !second.interact);
        assert!(second.thrust, "holding A keeps thrusting");
        let released = mapper.map(&PadState::default(), DT);
        assert!(!released.thrust);
        assert!(mapper.map(&held, DT).confirm, "a new press confirms again");
    }

    #[test]
    fn right_trigger_and_up_also_thrust_and_down_drills() {
        let mut mapper = PadMapper::default();
        assert!(mapper.map(&pad(&[PadButton::RightTrigger]), DT).thrust);
        assert!(mapper.map(&pad(&[PadButton::DPadUp]), DT).thrust);
        assert!(mapper.map(&stick(0.0, -0.9), DT).thrust);
        let down = mapper.map(&stick(0.0, 0.9), DT);
        assert!(down.drill_down && !down.thrust);
        assert!(mapper.map(&pad(&[PadButton::DPadDown]), DT).drill_down);
    }

    #[test]
    fn stick_and_dpad_walk_with_a_deadzone() {
        let mut mapper = PadMapper::default();
        assert!(mapper.map(&stick(0.1, 0.1), DT).horizontal.abs() < f32::EPSILON);
        assert!((mapper.map(&stick(0.7, 0.0), DT).horizontal - 0.7).abs() < f32::EPSILON);
        assert!((mapper.map(&stick(-1.5, 0.0), DT).horizontal + 1.0).abs() < f32::EPSILON);
        assert!((mapper.map(&pad(&[PadButton::DPadLeft]), DT).horizontal + 1.0).abs() < 1e-6);
        // Slight downward drift while walking does not drill.
        assert!(!mapper.map(&stick(0.9, 0.3), DT).drill_down);
    }

    #[test]
    fn menus_step_once_when_stick_and_dpad_point_the_same_way() {
        let mut mapper = PadMapper::default();
        let both = PadState {
            left_y: 1.0,
            ..pad(&[PadButton::DPadDown])
        };
        let steps = (0..10)
            .map(|_| mapper.map(&both, DT))
            .filter(|input| input.menu_down)
            .count();
        assert_eq!(steps, 1, "no double step and no repeat before the delay");
    }

    #[test]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive step count"
    )]
    fn held_direction_auto_repeats_after_the_initial_delay() {
        let mut mapper = PadMapper::default();
        let down = pad(&[PadButton::DPadDown]);
        // 1 second of holding: 1 immediate step, then repeats every interval after the delay.
        let steps = (0..60)
            .map(|_| mapper.map(&down, DT))
            .filter(|input| input.menu_down)
            .count();
        let expected = 1
            + ((1.0 - super::super::repeat::REPEAT_DELAY) / super::super::repeat::REPEAT_INTERVAL)
                as usize;
        assert!(steps.abs_diff(expected) <= 1, "{steps} vs {expected}");
        assert!(mapper.map(&pad(&[PadButton::DPadRight]), DT).menu_right);
    }

    #[test]
    fn activity_needs_a_new_press_or_the_stick_leaving_the_deadzone() {
        let idle = PadState::default();
        assert!(!stick(0.2, -0.2).new_activity_since(&idle), "noise");
        assert!(stick(0.6, 0.0).new_activity_since(&idle));
        assert!(
            !stick(0.6, 0.0).new_activity_since(&stick(0.5, 0.0)),
            "held"
        );
        assert!(pad(&[PadButton::B]).new_activity_since(&idle));
        assert!(!pad(&[PadButton::B]).new_activity_since(&pad(&[PadButton::B])));
    }
}
