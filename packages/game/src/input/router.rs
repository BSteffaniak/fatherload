//! Routes one frame of sampled devices to per-seat [`PlayerInput`]: gamepad mapping, seat
//! assignment ([`super::seats`]), hot-plug notices and each seat's active prompt device. Pure:
//! the app samples raylib into a [`DeviceFrame`] and tests script one.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    PlayerInput, combine_player_input,
    devices::{InputDevice, KeyboardLayout, PadFamily, SeatActivity},
    gamepad::{PadAction, PadMapper, PadState},
    seats::{Claim, SeatRoster},
};

/// One connected controller's sample.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PadSample {
    /// Name GLFW reports (glyph family, notices, diagnostics).
    pub name: String,
    pub state: PadState,
}

/// Every device sampled this frame.
#[derive(Clone, Debug, Default)]
pub struct DeviceFrame {
    /// The main keyboard: WASD + hotkeys, plus arrow aliases when playing alone.
    pub keyboard: PlayerInput,
    /// The split-screen arrow-keys scheme (ignored when playing alone).
    pub arrow_keys: PlayerInput,
    /// A mouse button or the wheel was used (counts as keyboard + mouse activity).
    pub mouse_used: bool,
    /// Connected controllers by index.
    pub pads: BTreeMap<u8, PadSample>,
}

/// Something the app should tell the player about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputNotice {
    Connected {
        index: u8,
        name: String,
    },
    Disconnected {
        index: u8,
        name: String,
    },
    /// A controller took over seat `seat` (0-based).
    Joined {
        index: u8,
        seat: usize,
    },
    /// Every seat has a controller and no more seats can be added.
    SeatsFull {
        index: u8,
    },
}

impl InputNotice {
    /// The toast shown for this notice.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Connected { name, .. } => format!("Controller connected: {name}"),
            Self::Disconnected { name, .. } => format!("Controller disconnected: {name}"),
            Self::Joined { index, seat } => format!(
                "Controller {} joined as Player {}.",
                u32::from(*index) + 1,
                seat + 1
            ),
            Self::SeatsFull { index } => format!(
                "Controller {}: every split-screen seat already has a controller.",
                u32::from(*index) + 1
            ),
        }
    }
}

/// What [`InputRouter::route`] produced.
#[derive(Clone, Debug, Default)]
pub struct RoutedInput {
    /// Input per seat, in seat order (`seat_count` entries).
    pub seats: Vec<PlayerInput>,
    /// Prompt device per seat.
    pub devices: Vec<InputDevice>,
    pub notices: Vec<InputNotice>,
    /// A controller wants a new split-screen seat; call [`InputRouter::seat_added`] once the
    /// game added it.
    pub wants_new_seat: Option<u8>,
}

/// Device bookkeeping that persists across frames.
#[derive(Debug, Default)]
pub struct InputRouter {
    mappers: BTreeMap<u8, PadMapper>,
    names: BTreeMap<u8, String>,
    roster: SeatRoster,
    activity: Vec<SeatActivity>,
    max_seats: usize,
}

impl InputRouter {
    /// A router allowing up to `max_seats` local seats.
    #[must_use]
    pub fn new(max_seats: usize) -> Self {
        Self {
            max_seats,
            ..Self::default()
        }
    }

    /// Connected controllers and their names.
    #[must_use]
    pub const fn pad_names(&self) -> &BTreeMap<u8, String> {
        &self.names
    }

    /// Routes one frame for a game with `seat_count` local seats.
    pub fn route(
        &mut self,
        frame: &DeviceFrame,
        seat_count: usize,
        delta_seconds: f32,
    ) -> RoutedInput {
        let seat_count = seat_count.max(1);
        let mut routed = RoutedInput::default();
        self.hot_plug(frame, &mut routed);
        let connected: BTreeSet<u8> = frame.pads.keys().copied().collect();
        self.activity.resize_with(seat_count, SeatActivity::default);
        for (pad, seat) in self.roster.set_seat_count(seat_count, &connected) {
            self.reseat(pad, seat);
        }

        // Map every pad (edges and repeat timers advance even while unassigned).
        let mut mapped: BTreeMap<u8, (PlayerInput, bool)> = BTreeMap::new();
        for (index, sample) in &frame.pads {
            let pad_mapper = self.mappers.entry(*index).or_default();
            let active = sample.state.new_activity_since(pad_mapper.previous());
            let claim_press = sample
                .state
                .action_pressed_since(pad_mapper.previous(), PadAction::Confirm)
                || sample
                    .state
                    .action_pressed_since(pad_mapper.previous(), PadAction::Pause);
            let input = pad_mapper.map(&sample.state, delta_seconds);
            if self.roster.seat_of(*index).is_none() && claim_press {
                // The claiming press is swallowed so it neither confirms nor pauses.
                match self.roster.claim(*index, self.max_seats) {
                    Some(Claim::Seat(seat)) => {
                        self.reseat(*index, seat);
                        routed.notices.push(InputNotice::Joined {
                            index: *index,
                            seat,
                        });
                    }
                    Some(Claim::NewSeat) => routed.wants_new_seat = Some(*index),
                    Some(Claim::Full) => {
                        routed
                            .notices
                            .push(InputNotice::SeatsFull { index: *index });
                    }
                    None => {}
                }
                continue;
            }
            mapped.insert(*index, (input, active));
        }

        for seat in 0..seat_count {
            let mut input = PlayerInput::default();
            let mut active = Vec::new();
            let mut fallback = InputDevice::Unassigned;
            if let Some(layout) = self.roster.keyboard(seat) {
                let keyboard = match layout {
                    KeyboardLayout::Full | KeyboardLayout::Wasd => frame.keyboard,
                    KeyboardLayout::Arrows => frame.arrow_keys,
                };
                let device = InputDevice::Keyboard(layout);
                fallback = device;
                if keyboard.has_activity() || (seat == 0 && frame.mouse_used) {
                    active.push(device);
                }
                input = keyboard;
            }
            for (index, (pad_input, pad_active)) in &mapped {
                if self.roster.seat_of(*index) != Some(seat) {
                    continue;
                }
                let device = self.pad_device(*index);
                if fallback == InputDevice::Unassigned {
                    fallback = device;
                }
                if *pad_active {
                    active.push(device);
                }
                input = combine_player_input(input, *pad_input);
            }
            if fallback == InputDevice::Unassigned
                && let Some(pad) = self.roster.pad(seat)
            {
                // A seated controller that is unplugged right now still owns the seat.
                fallback = self.pad_device(pad);
            }
            routed.seats.push(input);
            routed
                .devices
                .push(self.activity[seat].observe(&active, fallback));
        }
        routed
    }

    /// The game added `seat` for the controller that asked for it.
    pub fn seat_added(&mut self, pad: u8, seat: usize) {
        self.roster.seat_added(pad, seat);
        self.reseat(pad, seat);
        self.activity.resize_with(seat + 1, SeatActivity::default);
    }

    fn pad_device(&self, index: u8) -> InputDevice {
        InputDevice::Gamepad {
            index,
            family: self
                .names
                .get(&index)
                .map_or(PadFamily::Xbox, |name| PadFamily::from_name(name)),
        }
    }

    /// `pad` moved to `seat`: other seats stop showing its prompts, `seat` starts.
    fn reseat(&mut self, pad: u8, seat: usize) {
        for activity in &mut self.activity {
            activity.forget_pad(pad);
        }
        let device = self.pad_device(pad);
        if let Some(activity) = self.activity.get_mut(seat) {
            activity.observe(&[device], device);
        }
    }

    fn hot_plug(&mut self, frame: &DeviceFrame, routed: &mut RoutedInput) {
        let gone: Vec<u8> = self
            .names
            .keys()
            .filter(|index| !frame.pads.contains_key(index))
            .copied()
            .collect();
        for index in gone {
            let name = self.names.remove(&index).unwrap_or_default();
            self.mappers.remove(&index);
            for activity in &mut self.activity {
                activity.forget_pad(index);
            }
            routed
                .notices
                .push(InputNotice::Disconnected { index, name });
        }
        for (index, sample) in &frame.pads {
            if self.names.get(index) != Some(&sample.name) {
                if !self.names.contains_key(index) {
                    routed.notices.push(InputNotice::Connected {
                        index: *index,
                        name: sample.name.clone(),
                    });
                }
                self.names.insert(*index, sample.name.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::gamepad::PadButton;

    const DT: f32 = 1.0 / 60.0;

    fn sample(name: &str, buttons: &[PadButton]) -> PadSample {
        PadSample {
            name: name.to_owned(),
            state: PadState {
                buttons: buttons.iter().copied().collect(),
                ..PadState::default()
            },
        }
    }

    fn with_pads(pads: &[(u8, PadSample)]) -> DeviceFrame {
        DeviceFrame {
            pads: pads.iter().cloned().collect(),
            ..DeviceFrame::default()
        }
    }

    const XBOX: &str = "Xbox Wireless Controller";

    fn pad0() -> InputDevice {
        InputDevice::Gamepad {
            index: 0,
            family: PadFamily::Xbox,
        }
    }

    #[test]
    fn solo_merges_keyboard_and_any_controller_and_prompts_follow_the_last_used() {
        let mut router = InputRouter::new(4);
        let output = router.route(&DeviceFrame::default(), 1, DT);
        assert_eq!(output.devices, vec![InputDevice::KEYBOARD]);

        let output = router.route(&with_pads(&[(2, sample(XBOX, &[PadButton::Y]))]), 1, DT);
        assert!(output.seats[0].inventory, "pad 2 drives the only seat");
        assert_eq!(
            output.devices[0],
            InputDevice::Gamepad {
                index: 2,
                family: PadFamily::Xbox
            }
        );

        let mut keys = with_pads(&[(2, sample(XBOX, &[PadButton::Y]))]);
        keys.keyboard.interact = true;
        let output = router.route(&keys, 1, DT);
        assert!(output.seats[0].interact);
        assert_eq!(
            output.devices[0],
            InputDevice::KEYBOARD,
            "held Y is not new"
        );
    }

    #[test]
    fn stick_noise_never_switches_prompts() {
        let mut router = InputRouter::new(4);
        let mut frame = with_pads(&[(0, sample(XBOX, &[]))]);
        router.route(&frame, 1, DT);
        frame.keyboard.confirm = true;
        router.route(&frame, 1, DT);
        frame.keyboard.confirm = false;
        if let Some(pad) = frame.pads.get_mut(&0) {
            pad.state.left_x = 0.2;
            pad.state.left_y = -0.15;
        }
        let output = router.route(&frame, 1, DT);
        assert_eq!(output.devices[0], InputDevice::KEYBOARD);
        if let Some(pad) = frame.pads.get_mut(&0) {
            pad.state.left_x = 0.8;
        }
        assert_eq!(router.route(&frame, 1, DT).devices[0], pad0());
    }

    #[test]
    fn hot_plug_reports_connections_and_falls_back_to_the_keyboard() {
        let mut router = InputRouter::new(4);
        let output = router.route(&with_pads(&[(0, sample(XBOX, &[PadButton::A]))]), 1, DT);
        assert_eq!(
            output.notices,
            vec![InputNotice::Connected {
                index: 0,
                name: XBOX.to_owned()
            }]
        );
        assert_eq!(output.devices[0], pad0());
        let output = router.route(&DeviceFrame::default(), 1, DT);
        assert_eq!(
            output.notices,
            vec![InputNotice::Disconnected {
                index: 0,
                name: XBOX.to_owned()
            }]
        );
        assert_eq!(output.devices[0], InputDevice::KEYBOARD);
        // Reconnecting with A held is a fresh press (new mapper), so it acts right away.
        let output = router.route(&with_pads(&[(0, sample(XBOX, &[PadButton::A]))]), 1, DT);
        assert!(output.seats[0].confirm);
    }

    #[test]
    fn split_screen_seats_keyboards_and_controllers_separately() {
        let mut router = InputRouter::new(4);
        let mut frame = with_pads(&[(0, sample(XBOX, &[]))]);
        frame.keyboard.horizontal = -1.0;
        let output = router.route(&frame, 2, DT);
        assert_eq!(
            output.devices,
            vec![InputDevice::Keyboard(KeyboardLayout::Wasd), pad0()],
            "the auto-seated controller's prompts show on seat 2"
        );
        frame.arrow_keys.horizontal = 1.0;
        let output = router.route(&frame, 2, DT);
        assert!((output.seats[0].horizontal + 1.0).abs() < f32::EPSILON);
        assert!((output.seats[1].horizontal - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            output.devices[1],
            InputDevice::Keyboard(KeyboardLayout::Arrows),
            "using the arrow keys switches seat 2's prompts"
        );
        frame.arrow_keys.horizontal = 0.0;
        if let Some(pad) = frame.pads.get_mut(&0) {
            pad.state.buttons.insert(PadButton::X);
        }
        let output = router.route(&frame, 2, DT);
        assert!(output.seats[1].interact && !output.seats[0].interact);
        assert_eq!(output.devices[1], pad0());
    }

    #[test]
    fn an_extra_controller_claims_a_new_seat_with_a_or_start() {
        let mut router = InputRouter::new(3);
        let frame = with_pads(&[
            (0, sample(XBOX, &[])),
            (1, sample(XBOX, &[])),
            (2, sample(XBOX, &[])),
        ]);
        router.route(&frame, 2, DT);
        // Pads 0 and 1 fill the arrow-keys and WASD seats; pad 2 is left over.
        let press = with_pads(&[
            (0, sample(XBOX, &[])),
            (1, sample(XBOX, &[])),
            (2, sample(XBOX, &[PadButton::Menu])),
        ]);
        let output = router.route(&press, 2, DT);
        assert_eq!(output.wants_new_seat, Some(2));
        assert!(
            !output.seats.iter().any(|seat| seat.pause),
            "claim press is swallowed"
        );
        router.seat_added(2, 2);
        let mut frame = with_pads(&[
            (0, sample(XBOX, &[])),
            (1, sample(XBOX, &[])),
            (2, sample(XBOX, &[PadButton::Menu, PadButton::Y])),
        ]);
        let output = router.route(&frame, 3, DT);
        assert_eq!(output.devices.len(), 3);
        assert!(output.seats[2].inventory);
        assert_eq!(
            output.devices[2],
            InputDevice::Gamepad {
                index: 2,
                family: PadFamily::Xbox
            }
        );
        frame.pads.insert(3, sample(XBOX, &[PadButton::A]));
        let output = router.route(&frame, 3, DT);
        assert!(
            output
                .notices
                .contains(&InputNotice::SeatsFull { index: 3 })
        );
    }

    #[test]
    fn a_seat_waiting_for_a_controller_is_unassigned() {
        let mut router = InputRouter::new(4);
        let output = router.route(&DeviceFrame::default(), 3, DT);
        assert_eq!(output.devices[2], InputDevice::Unassigned);
        let output = router.route(
            &with_pads(&[(4, sample("DualSense", &[PadButton::A]))]),
            3,
            DT,
        );
        assert_eq!(
            output.notices.last(),
            Some(&InputNotice::Joined { index: 4, seat: 2 })
        );
        assert_eq!(
            output.devices[2],
            InputDevice::Gamepad {
                index: 4,
                family: PadFamily::PlayStation
            }
        );
    }
}
