//! The Options dialog's rows. The touch rows only show once a touch screen has been used.

use super::{ShellState, SoundCue};

/// Button sizes the touch controls offer, as scale factors (see
/// [`crate::input::touch::TouchSettings::scale`]).
const TOUCH_SIZES: [(f32, &str); 3] = [(0.85, "Small"), (1.0, "Medium"), (1.2, "Large")];

/// A row of the Options dialog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionsRow {
    Volume,
    Fullscreen,
    /// Touch button size.
    TouchSize,
    /// Which thumb gets the stick.
    TouchHand,
    /// The optional hold-to-thrust button.
    TouchThrust,
    Back,
}

impl ShellState {
    /// The Options rows, top to bottom.
    #[must_use]
    pub fn options_rows(&self) -> Vec<OptionsRow> {
        let mut rows = vec![OptionsRow::Volume, OptionsRow::Fullscreen];
        if self.touch_seen {
            rows.extend([
                OptionsRow::TouchSize,
                OptionsRow::TouchHand,
                OptionsRow::TouchThrust,
            ]);
        }
        rows.push(OptionsRow::Back);
        rows
    }

    /// The selected Options row.
    pub(super) fn options_row(&self) -> OptionsRow {
        let rows = self.options_rows();
        rows[self.selected_menu_item.min(rows.len() - 1)]
    }

    /// The name of the current touch button size.
    #[must_use]
    pub fn touch_size_label(&self) -> &'static str {
        TOUCH_SIZES[self.touch_size_index()].1
    }

    fn touch_size_index(&self) -> usize {
        let scale = self.touch.scale;
        (0..TOUCH_SIZES.len())
            .min_by(|left, right| {
                let distance = |index: &usize| (TOUCH_SIZES[*index].0 - scale).abs();
                distance(left).total_cmp(&distance(right))
            })
            .unwrap_or(1)
    }

    /// Steps a touch row's value by `step` (left/right, or confirm for +1). `false` when `row`
    /// is not a touch row.
    pub(super) fn step_touch_option(&mut self, row: OptionsRow, step: i32) -> bool {
        match row {
            OptionsRow::TouchSize => {
                let count = i32::try_from(TOUCH_SIZES.len()).unwrap_or(1);
                let index = i32::try_from(self.touch_size_index()).unwrap_or(0);
                let next = usize::try_from((index + step).rem_euclid(count)).unwrap_or(0);
                self.touch.scale = TOUCH_SIZES[next].0;
            }
            OptionsRow::TouchHand => self.touch.left_handed = !self.touch.left_handed,
            OptionsRow::TouchThrust => self.touch.thrust_button = !self.touch.thrust_button,
            OptionsRow::Volume | OptionsRow::Fullscreen | OptionsRow::Back => return false,
        }
        self.settings_dirty = true;
        self.sound_cues.push(SoundCue::Ui);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::ModalScreen;

    #[test]
    fn touch_rows_appear_once_a_touch_screen_was_used() {
        let mut shell = ShellState::default();
        assert_eq!(
            shell.options_rows(),
            vec![OptionsRow::Volume, OptionsRow::Fullscreen, OptionsRow::Back]
        );
        shell.touch_seen = true;
        assert_eq!(shell.options_rows().len(), 6);
        assert_eq!(shell.options_rows().last(), Some(&OptionsRow::Back));
    }

    #[test]
    fn touch_rows_cycle_and_mark_settings_for_saving() {
        let mut shell = ShellState {
            touch_seen: true,
            modal: Some(ModalScreen::Options),
            ..ShellState::default()
        };
        assert_eq!(shell.touch_size_label(), "Medium");
        assert!(shell.step_touch_option(OptionsRow::TouchSize, 1));
        assert_eq!(shell.touch_size_label(), "Large");
        assert!(shell.step_touch_option(OptionsRow::TouchSize, 1));
        assert_eq!(shell.touch_size_label(), "Small", "wraps");
        assert!(shell.step_touch_option(OptionsRow::TouchSize, -1));
        assert_eq!(shell.touch_size_label(), "Large");
        assert!(shell.take_settings_dirty());

        assert!(shell.step_touch_option(OptionsRow::TouchHand, 1));
        assert!(shell.touch.left_handed);
        assert!(shell.step_touch_option(OptionsRow::TouchThrust, 1));
        assert!(shell.touch.thrust_button);
        assert!(!shell.step_touch_option(OptionsRow::Back, 1));
        assert_eq!(shell.settings().touch, shell.touch);
    }
}
