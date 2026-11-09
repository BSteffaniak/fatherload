//! The pause menu's test-mode entries: the invulnerable hull and infinite fuel toggles and the
//! "Warp to Layer" band list. Each submits a `PlayerCommand::TestMode` action for the seat
//! driving the menu, so it works the same in split-screen and online.

use fatherload_simulation::{PlayerCommand, TestModeAction, TestModeState, worldgen::Band};

use crate::runtime::OnlineRole;

use super::{PauseOption, RunMode, ShellRequest, ShellState};

/// The bands "Warp to Layer" lists, top to bottom (row index = band index).
pub const WARP_BANDS: [Band; Band::ALL.len()] = Band::ALL;

impl PauseOption {
    /// Pause entries for the current kind of session, with the test-mode entries right after
    /// "Resume" in a test-mode world.
    #[must_use]
    pub fn for_session(role: Option<OnlineRole>, test_mode: bool) -> Vec<Self> {
        let mut options = Self::for_role(role);
        if test_mode {
            options.splice(
                1..1,
                [
                    Self::WarpToLayer,
                    Self::ToggleInvulnerable,
                    Self::ToggleInfiniteFuel,
                ],
            );
        }
        options
    }
}

impl ShellState {
    /// The test-mode toggles as the menus should show them: what the player last chose, until
    /// the world catches up.
    #[must_use]
    pub fn shown_test_mode(&self, world: Option<TestModeState>) -> Option<TestModeState> {
        let state = world?;
        Some(self.pending_test_mode.unwrap_or(state))
    }

    /// Forgets the pending toggles once the world shows them (or is no longer in test mode).
    pub(super) fn sync_pending_test_mode(&mut self, world: Option<TestModeState>) {
        if world.is_none() || world == self.pending_test_mode {
            self.pending_test_mode = None;
        }
    }

    /// Flips the toggle `option` for the crew.
    pub(super) fn toggle_test_mode(
        &mut self,
        option: PauseOption,
        world: Option<TestModeState>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let Some(mut state) = self.shown_test_mode(world) else {
            return;
        };
        let (action, name, on) = match option {
            PauseOption::ToggleInvulnerable => {
                state.invulnerable = !state.invulnerable;
                let on = state.invulnerable;
                (TestModeAction::SetInvulnerable(on), "Invulnerable hull", on)
            }
            PauseOption::ToggleInfiniteFuel => {
                state.infinite_fuel = !state.infinite_fuel;
                let on = state.infinite_fuel;
                (TestModeAction::SetInfiniteFuel(on), "Infinite fuel", on)
            }
            _ => return,
        };
        self.pending_test_mode = Some(state);
        requests.push(ShellRequest::Action {
            seat: self.menu_seat,
            command: PlayerCommand::TestMode(action),
        });
        self.message = format!("{name} {}.", if on { "on" } else { "off" });
    }

    /// Warps the menu seat's rig to the band selected in the warp list and resumes play.
    pub(super) fn confirm_warp(&mut self, requests: &mut Vec<ShellRequest>) {
        let band = WARP_BANDS[self.selected_menu_item.min(WARP_BANDS.len() - 1)];
        requests.push(ShellRequest::Action {
            seat: self.menu_seat,
            command: PlayerCommand::TestMode(TestModeAction::WarpToBand(band)),
        });
        self.modal = None;
        self.run_mode = RunMode::Playing;
        self.message = format!("Warped to the {}.", band.name());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_entries_follow_resume_only_in_test_mode() {
        let normal = PauseOption::for_session(None, false);
        assert_eq!(normal, PauseOption::for_role(None));
        let test = PauseOption::for_session(Some(OnlineRole::Client), true);
        assert_eq!(
            &test[..4],
            &[
                PauseOption::Resume,
                PauseOption::WarpToLayer,
                PauseOption::ToggleInvulnerable,
                PauseOption::ToggleInfiniteFuel,
            ]
        );
    }

    #[test]
    fn toggles_show_the_pending_choice_until_the_world_applies_it() {
        let mut shell = ShellState::default();
        let world = Some(TestModeState::default());
        let mut requests = Vec::new();
        shell.toggle_test_mode(PauseOption::ToggleInvulnerable, world, &mut requests);
        shell.toggle_test_mode(PauseOption::ToggleInfiniteFuel, world, &mut requests);
        let wanted = TestModeState {
            invulnerable: true,
            infinite_fuel: true,
        };
        assert_eq!(shell.shown_test_mode(world), Some(wanted));
        assert_eq!(requests.len(), 2);
        shell.sync_pending_test_mode(world);
        assert_eq!(shell.shown_test_mode(world), Some(wanted), "still pending");
        shell.sync_pending_test_mode(Some(wanted));
        assert_eq!(shell.pending_test_mode, None);
        // Not a test-mode world: nothing to toggle.
        requests.clear();
        shell.toggle_test_mode(PauseOption::ToggleInvulnerable, None, &mut requests);
        assert!(requests.is_empty());
    }
}
