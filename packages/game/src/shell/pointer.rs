//! Picking a menu row directly (a tap or a click) instead of stepping to it.
//!
//! [`crate::input::PlayerInput::menu_pick`] names a row of whatever menu the seat is looking
//! at. The shell selects that row and treats the input as a confirm, so every screen's own
//! handler runs exactly as if the player had stepped there and pressed confirm.

use super::{ModalScreen, RunMode, ShellState};
use crate::input::PlayerInput;

impl ShellState {
    /// Turns each seat's row pick into "that row is selected, and confirm was pressed".
    pub(super) fn take_menu_picks(&mut self, inputs: &[PlayerInput]) -> Vec<PlayerInput> {
        inputs
            .iter()
            .enumerate()
            .map(|(seat, input)| {
                let mut input = *input;
                if let Some(row) = input.menu_pick.take()
                    && self.pick_row(seat, row)
                {
                    input.confirm = true;
                }
                input
            })
            .collect()
    }

    /// Selects `row` in the menu `seat` drives. `false` when it drives none.
    fn pick_row(&mut self, seat: usize, row: usize) -> bool {
        // Global screens belong to the menu seat; the first seat (keyboard, mouse and touch
        // screen) can always work them too.
        let drives_global = seat == 0 || seat == self.menu_seat;
        if let Some(modal) = self.modal {
            if !drives_global {
                return false;
            }
            if modal == ModalScreen::WorldSetup {
                if let Some(setup) = self.world_setup.as_mut() {
                    setup.selected = row;
                }
            } else {
                self.selected_menu_item = row;
            }
            return true;
        }
        match self.run_mode {
            RunMode::Title => {
                if drives_global {
                    self.selected_title_item = row;
                }
                drives_global
            }
            RunMode::Paused => {
                if drives_global {
                    self.selected_pause_item = row;
                }
                drives_global
            }
            RunMode::Lobby => {
                if drives_global {
                    self.online.selected = row;
                }
                drives_global
            }
            RunMode::Playing => {
                let seat = self.seat_mut(seat);
                match seat.modal {
                    None => false,
                    // A confirmation shows the one offer being confirmed; the selection stays
                    // on the row that led here. The ending's selection is its page (Wave 10):
                    // a tap anywhere turns it.
                    Some(
                        ModalScreen::FuelConfirm
                        | ModalScreen::RepairConfirm
                        | ModalScreen::ShopConfirm
                        | ModalScreen::Ending,
                    ) => true,
                    Some(_) => {
                        seat.selected_menu_item = row;
                        true
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::SettingsFile;

    fn pick(row: usize) -> PlayerInput {
        PlayerInput {
            menu_pick: Some(row),
            ..PlayerInput::default()
        }
    }

    #[test]
    fn a_pick_selects_the_row_of_the_screen_in_front_and_confirms() {
        let mut shell = ShellState::new(SettingsFile::default());
        assert_eq!(shell.run_mode, RunMode::Title);
        let inputs = shell.take_menu_picks(&[pick(2)]);
        assert!(inputs[0].confirm && inputs[0].menu_pick.is_none());
        assert_eq!(shell.selected_title_item, 2);

        shell.run_mode = RunMode::Paused;
        shell.take_menu_picks(&[pick(3)]);
        assert_eq!(shell.selected_pause_item, 3);

        // A global dialog in front of the pause menu takes the pick instead.
        shell.modal = Some(ModalScreen::Options);
        shell.take_menu_picks(&[pick(1)]);
        assert_eq!(shell.selected_menu_item, 1);
        assert_eq!(shell.selected_pause_item, 3);

        shell.open_world_setup(crate::shell::SetupPurpose::Local { seats: 1 });
        shell.take_menu_picks(&[pick(4)]);
        assert_eq!(shell.world_setup.map(|setup| setup.selected), Some(4));
    }

    #[test]
    fn seat_dialogs_take_their_own_seats_picks() {
        let mut shell = ShellState::new(SettingsFile::default());
        shell.run_mode = RunMode::Playing;
        shell.set_seat_count(2);
        let idle = shell.take_menu_picks(&[pick(1), pick(1)]);
        assert!(
            !idle[0].confirm && !idle[1].confirm,
            "no menu open: a pick is not a confirm"
        );

        shell.seat_mut(1).modal = Some(ModalScreen::Depot);
        let inputs = shell.take_menu_picks(&[pick(2), pick(4)]);
        assert!(!inputs[0].confirm && inputs[1].confirm);
        assert_eq!(shell.seat(1).selected_menu_item, 4);
        assert_eq!(shell.seat(0).selected_menu_item, 0);
    }

    #[test]
    fn confirmation_screens_keep_the_row_that_led_to_them() {
        let mut shell = ShellState::new(SettingsFile::default());
        shell.run_mode = RunMode::Playing;
        shell.seat_mut(0).modal = Some(ModalScreen::ShopConfirm);
        shell.seat_mut(0).selected_menu_item = 5;
        let inputs = shell.take_menu_picks(&[pick(0)]);
        assert!(inputs[0].confirm);
        assert_eq!(shell.seat(0).selected_menu_item, 5);
    }

    #[test]
    fn only_the_menu_seat_and_the_first_seat_work_global_screens() {
        let mut shell = ShellState::new(SettingsFile::default());
        shell.run_mode = RunMode::Paused;
        shell.set_seat_count(3);
        shell.menu_seat = 1;
        let inputs = shell.take_menu_picks(&[PlayerInput::default(), pick(2), pick(4)]);
        assert!(inputs[1].confirm && !inputs[2].confirm);
        assert_eq!(shell.selected_pause_item, 2);
    }
}
