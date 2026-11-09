//! The world setup dialog shown before a new world starts (title "New Game", "Local
//! Split-Screen" and "Host Online Game"): seed (random by default, re-roll, type or paste),
//! world size, resource richness and hazard intensity.

use drillgame_simulation::worldgen::{
    HazardIntensity, ResourceRichness, WorldSettings, WorldSize, format_seed, parse_seed,
};

use crate::input::PlayerInput;

/// Longest seed text accepted in the field.
pub const MAX_SEED_TEXT: usize = 32;
/// Random seeds use this many bits (12 hex digits: short enough to read out and share).
const RANDOM_SEED_BITS: u32 = 48;

/// What the new world is for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupPurpose {
    /// A local game with this many split-screen seats.
    Local { seats: usize },
    /// A hosted online game (the host's settings define the session).
    HostOnline,
}

/// Rows of the dialog, top to bottom.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupRow {
    Seed,
    Reroll,
    Size,
    Richness,
    Hazards,
    Start,
    Back,
}

impl SetupRow {
    pub const ALL: [Self; 7] = [
        Self::Seed,
        Self::Reroll,
        Self::Size,
        Self::Richness,
        Self::Hazards,
        Self::Start,
        Self::Back,
    ];
}

/// What a dialog update asks the shell to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupOutcome {
    Stay,
    Close,
    Start(SetupPurpose, WorldSettings),
}

/// The dialog's state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldSetup {
    pub purpose: SetupPurpose,
    pub settings: WorldSettings,
    pub selected: usize,
    /// The seed text being typed, while the seed field is focused.
    pub seed_edit: Option<String>,
}

/// A fresh random world seed from OS randomness (the simulation itself stays deterministic:
/// randomness only ever picks the seed).
#[must_use]
pub fn random_world_seed() -> u64 {
    crate::online::random_token_seed() & ((1 << RANDOM_SEED_BITS) - 1)
}

/// Steps through `options` from `current` by `step` (wrapping).
fn cycle<T: Copy + PartialEq>(options: &[T], current: T, step: isize) -> T {
    let index = options
        .iter()
        .position(|option| *option == current)
        .unwrap_or(0);
    let len = isize::try_from(options.len()).unwrap_or(1);
    let next = (isize::try_from(index).unwrap_or(0) + step).rem_euclid(len);
    options[usize::try_from(next).unwrap_or(0)]
}

impl WorldSetup {
    /// A dialog for `purpose` with standard settings and `seed`.
    #[must_use]
    pub const fn new(purpose: SetupPurpose, seed: u64) -> Self {
        Self {
            purpose,
            settings: WorldSettings::new(seed),
            selected: 0,
            seed_edit: None,
        }
    }

    /// The highlighted row.
    #[must_use]
    pub fn row(&self) -> SetupRow {
        SetupRow::ALL[self.selected.min(SetupRow::ALL.len() - 1)]
    }

    /// The seed as shown in the field: the text being typed, or the current seed.
    #[must_use]
    pub fn seed_text(&self) -> String {
        self.seed_edit
            .clone()
            .unwrap_or_else(|| format_seed(self.settings.seed))
    }

    /// Whether the seed field is taking typed text.
    #[must_use]
    pub const fn typing(&self) -> bool {
        self.seed_edit.is_some()
    }

    /// Applies one frame of menu input. `pasted` is clipboard text pasted this frame and
    /// `reroll` draws a fresh random seed.
    pub fn update(
        &mut self,
        input: PlayerInput,
        pasted: Option<String>,
        reroll: impl FnOnce() -> u64,
    ) -> SetupOutcome {
        if self.seed_edit.is_some() {
            self.update_typing(input, pasted);
            return SetupOutcome::Stay;
        }
        // Pasting anywhere in the dialog takes the clipboard as the seed.
        if let Some(seed) = pasted.as_deref().and_then(parse_seed) {
            self.settings.seed = seed;
            self.selected = 0;
            return SetupOutcome::Stay;
        }
        if input.backs_out() {
            return SetupOutcome::Close;
        }
        if input.menu_up {
            self.selected = self.selected.saturating_sub(1);
        }
        if input.menu_down {
            self.selected = (self.selected + 1).min(SetupRow::ALL.len() - 1);
        }
        let step = if input.menu_left || input.tab_previous {
            -1
        } else {
            isize::from(input.menu_right || input.tab_next || input.confirm)
        };
        match self.row() {
            SetupRow::Size if step != 0 => {
                self.settings.size = cycle(&WorldSize::ALL, self.settings.size, step);
            }
            SetupRow::Richness if step != 0 => {
                self.settings.richness =
                    cycle(&ResourceRichness::ALL, self.settings.richness, step);
            }
            SetupRow::Hazards if step != 0 => {
                self.settings.hazards = cycle(&HazardIntensity::ALL, self.settings.hazards, step);
            }
            SetupRow::Seed if input.confirm => self.seed_edit = Some(String::new()),
            SetupRow::Reroll if input.confirm => self.settings.seed = reroll(),
            SetupRow::Start if input.confirm => {
                return SetupOutcome::Start(self.purpose, self.settings);
            }
            SetupRow::Back if input.confirm => return SetupOutcome::Close,
            _ => {}
        }
        SetupOutcome::Stay
    }

    /// Typing into the seed field: characters, Backspace, paste; Enter keeps the typed seed and
    /// Esc keeps the old one.
    fn update_typing(&mut self, input: PlayerInput, pasted: Option<String>) {
        let Some(text) = self.seed_edit.as_mut() else {
            return;
        };
        if let Some(pasted) = pasted {
            text.push_str(pasted.trim());
            text.truncate(MAX_SEED_TEXT);
        }
        // `E` also counts as "confirm" in menus; while typing it is just a letter.
        let typed_confirm_key = matches!(input.text_input, Some('e' | 'E'));
        if input.text_backspace {
            text.pop();
        } else if input.backs_out() {
            self.seed_edit = None;
            return;
        }
        if let Some(character) = input.text_input
            && !character.is_control()
            && text.len() < MAX_SEED_TEXT
        {
            text.push(character);
        }
        if input.confirm && !typed_confirm_key {
            if let Some(seed) = parse_seed(text) {
                self.settings.seed = seed;
            }
            self.seed_edit = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(setup: &mut WorldSetup, input: PlayerInput) -> SetupOutcome {
        setup.update(input, None, || 0xABC)
    }

    const NONE: PlayerInput = PlayerInput {
        horizontal: 0.0,
        thrust: false,
        drill_down: false,
        interact: false,
        confirm: false,
        cancel: false,
        back: false,
        pause: false,
        menu_up: false,
        menu_down: false,
        menu_left: false,
        menu_right: false,
        tab_previous: false,
        tab_next: false,
        details: false,
        inventory: false,
        save: false,
        load: false,
        selected_upgrade: None,
        map: false,
        help: false,
        volume_up: false,
        volume_down: false,
        fullscreen: false,
        local_multiplayer_toggle: false,
        bomb: false,
        scan: false,
        place_relay: false,
        place_drone: false,
        place_lift: false,
        place_support: false,
        place_pump: false,
        place_processor: false,
        exit_requested: false,
        text_input: None,
        text_backspace: false,
        ui_scroll: 0.0,
        net_debug: false,
        tune_next: false,
        page_up: false,
        page_down: false,
        scroll_home: false,
        scroll_end: false,
        paste: false,
    };
    const DOWN: PlayerInput = PlayerInput {
        menu_down: true,
        ..NONE
    };
    const CONFIRM: PlayerInput = PlayerInput {
        confirm: true,
        ..NONE
    };

    #[test]
    fn rerolls_cycles_options_and_starts() {
        let mut setup = WorldSetup::new(SetupPurpose::Local { seats: 2 }, 7);
        press(&mut setup, DOWN);
        assert_eq!(setup.row(), SetupRow::Reroll);
        press(&mut setup, CONFIRM);
        assert_eq!(setup.settings.seed, 0xABC);
        press(&mut setup, DOWN);
        press(&mut setup, CONFIRM);
        assert_eq!(setup.settings.size, WorldSize::Large);
        press(&mut setup, DOWN);
        press(
            &mut setup,
            PlayerInput {
                menu_left: true,
                ..NONE
            },
        );
        assert_eq!(setup.settings.richness, ResourceRichness::Lean);
        press(&mut setup, DOWN);
        press(&mut setup, CONFIRM);
        assert_eq!(setup.settings.hazards, HazardIntensity::Brutal);
        press(&mut setup, DOWN);
        let outcome = press(&mut setup, CONFIRM);
        assert_eq!(
            outcome,
            SetupOutcome::Start(SetupPurpose::Local { seats: 2 }, setup.settings)
        );
    }

    #[test]
    fn typed_and_pasted_seeds_replace_the_seed() {
        let mut setup = WorldSetup::new(SetupPurpose::HostOnline, 1);
        press(&mut setup, CONFIRM);
        assert!(setup.typing());
        for character in "beef".chars() {
            press(
                &mut setup,
                PlayerInput {
                    text_input: Some(character),
                    ..NONE
                },
            );
        }
        // Typing an `e` is a letter, not a confirm.
        press(
            &mut setup,
            PlayerInput {
                text_input: Some('e'),
                confirm: true,
                ..NONE
            },
        );
        assert_eq!(setup.seed_text(), "beefe");
        press(&mut setup, CONFIRM);
        assert!(!setup.typing());
        assert_eq!(setup.settings.seed, 0xBEEFE);
        setup.update(NONE, Some("D111-6A4E".to_owned()), || 0);
        assert_eq!(setup.settings.seed, 0xD111_6A4E);
        // Esc while typing keeps the old seed.
        press(&mut setup, CONFIRM);
        press(
            &mut setup,
            PlayerInput {
                cancel: true,
                ..NONE
            },
        );
        assert!(!setup.typing());
        assert_eq!(setup.settings.seed, 0xD111_6A4E);
        assert_eq!(
            press(
                &mut setup,
                PlayerInput {
                    cancel: true,
                    ..NONE
                }
            ),
            SetupOutcome::Close
        );
    }

    #[test]
    fn random_seeds_fit_in_twelve_hex_digits() {
        for _ in 0..8 {
            assert!(random_world_seed() < 1 << 48);
        }
    }
}
