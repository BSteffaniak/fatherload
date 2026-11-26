//! The god-mode toolkit dialog (Wave 14, W14-A): the pause menu's "God-Mode Toolkit" entry in
//! a test-mode world. One row per tool: infinite supplies, restocking, the story unlock, each
//! Descent gate and the Heart's ending, the home clock and its lock, the bodies' weather, the
//! creature to spawn, spawning it, clearing nearby creatures, freezing their AI and the
//! enemy setting.
//!
//! Rows are stepped with up/down (or tapped), a row's value with left/right (the touch screen's
//! arrow buttons, a pad's d-pad; the tab buttons step the creature ten at a time) and confirm
//! (a tap) toggles or acts. Every tool submits a `PlayerCommand::TestMode` action for the seat
//! driving the menu, so it works the same in split-screen and online (the host's world
//! decides).

use fatherload_simulation::{
    EnemyDifficulty, PlayerCommand, TestModeAction, TestModeState, WorldState,
    bodylife::{WeatherId, weather::weather_kinds, weather_kind},
    descent::DescentBoss,
    enemy::EnemyKind,
};

use crate::input::PlayerInput;

use super::{ModalScreen, ShellRequest, ShellState};

/// One row of the toolkit dialog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolkitRow {
    /// The crew's infinite supplies (on by default).
    InfiniteSupplies,
    /// Refill the seat's consumables once.
    Restock,
    /// Fit anything the seat's rig lacks and fill every stack.
    StockAll,
    /// The story and codex unlocked (off by default).
    UnlockStory,
    /// A Descent's gate cleared (0 = the Crust .. 6 = the Ancients; Deep Descent worlds).
    Gate(u8),
    /// The Heart's ending done (Deep Descent worlds).
    EndingDone,
    /// The home clock's time of day.
    TimeOfDay,
    /// The day/night lock.
    DayLock,
    /// The weather forced on every body.
    Weather,
    /// The creature the next row spawns.
    Creature,
    /// Spawn it beside the seat's rig.
    SpawnCreature,
    /// Clear the creatures near the seat's rig.
    ClearEnemies,
    /// Freeze every creature's AI.
    FreezeAi,
    /// The world's enemy setting.
    Difficulty,
}

impl ToolkitRow {
    /// The rows for a world (`descents`: a Deep Descent world, with gates and the Heart).
    #[must_use]
    pub fn rows(descents: bool) -> Vec<Self> {
        let mut rows = vec![Self::InfiniteSupplies, Self::Restock, Self::StockAll];
        if descents {
            rows.push(Self::UnlockStory);
            rows.extend((0..7).map(Self::Gate));
            rows.push(Self::EndingDone);
        }
        rows.extend([
            Self::TimeOfDay,
            Self::DayLock,
            Self::Weather,
            Self::Creature,
            Self::SpawnCreature,
            Self::ClearEnemies,
            Self::FreezeAi,
            Self::Difficulty,
        ]);
        rows
    }

    /// The row's label.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::InfiniteSupplies => "Infinite Supplies".to_owned(),
            Self::Restock => "Restock Now".to_owned(),
            Self::StockAll => "Stock Everything".to_owned(),
            Self::UnlockStory => "Unlock Story & Codex".to_owned(),
            Self::Gate(descent) => format!(
                "Gate {}: {}",
                roman(descent + 1),
                DescentBoss::ALL[usize::from(descent).min(7)].name()
            ),
            Self::EndingDone => "Ending: The Heart Woke".to_owned(),
            Self::TimeOfDay => "Time of Day".to_owned(),
            Self::DayLock => "Day/Night Lock".to_owned(),
            Self::Weather => "Weather on Worlds".to_owned(),
            Self::Creature => "Creature".to_owned(),
            Self::SpawnCreature => "Spawn Beside Me".to_owned(),
            Self::ClearEnemies => "Clear Nearby Creatures".to_owned(),
            Self::FreezeAi => "Freeze Creature AI".to_owned(),
            Self::Difficulty => "Enemies".to_owned(),
        }
    }
}

/// `n` (1-8) in Roman numerals.
#[must_use]
pub const fn roman(n: u8) -> &'static str {
    match n {
        1 => "I",
        2 => "II",
        3 => "III",
        4 => "IV",
        5 => "V",
        6 => "VI",
        7 => "VII",
        _ => "VIII",
    }
}

/// The weathers the weather row cycles: each body's own forecast, then every registered kind.
#[must_use]
pub fn weather_choices() -> Vec<Option<WeatherId>> {
    std::iter::once(None)
        .chain(weather_kinds().map(|kind| Some(kind.id)))
        .collect()
}

/// A weather choice in words.
#[must_use]
pub const fn weather_name(choice: Option<WeatherId>) -> &'static str {
    match choice {
        None => "Forecast",
        Some(id) => weather_kind(id).name,
    }
}

/// Whether `kind` is a boss (a theme world's, a Descent's or the swarm's mother).
#[must_use]
pub fn is_boss(kind: EnemyKind) -> bool {
    kind.is_outland_boss()
        || kind.is_exotic_boss()
        || fatherload_simulation::endgame::hunt::theme_boss_of(kind).is_some()
}

/// The time-of-day row's stops when confirmed: dawn, morning, noon, dusk, night, midnight.
pub const TIME_STOPS: [u16; 6] = [5 * 60, 8 * 60, 12 * 60, 18 * 60 + 30, 22 * 60, 0];

/// What the toolkit's rows show of the world.
#[derive(Clone, Copy, Debug, Default)]
pub struct ToolkitView {
    pub test_mode: TestModeState,
    /// Gates cleared, one bit per Descent (0-6).
    pub gates: u8,
    pub ending_done: bool,
    /// The home clock's minutes past midnight.
    pub minutes: u16,
    pub held: bool,
    pub difficulty: EnemyDifficulty,
    pub descents: bool,
}

impl ToolkitView {
    /// The view of `world` (test mode's toggles as the seat last chose them).
    #[must_use]
    pub fn of(world: &WorldState, shown: Option<TestModeState>) -> Self {
        let clock = world.clock();
        let (hours, minutes) = clock.hours_minutes();
        Self {
            test_mode: shown.or_else(|| world.test_mode()).unwrap_or_default(),
            gates: (0..7_u8).fold(0, |bits, descent| {
                bits | (u8::from(world.gate_cleared(descent)) << descent)
            }),
            ending_done: world.ending_done(),
            minutes: u16::from(hours) * 60 + u16::from(minutes),
            held: clock.is_held(),
            difficulty: world.enemy_difficulty(),
            descents: world.world_settings().generator.has_descents(),
        }
    }

    /// Whether gate `descent` is cleared.
    #[must_use]
    pub const fn gate(&self, descent: u8) -> bool {
        self.gates & (1 << descent) != 0
    }
}

impl ShellState {
    /// The creature the toolkit spawns.
    #[must_use]
    pub const fn toolkit_creature(&self) -> EnemyKind {
        EnemyKind::ALL[self.toolkit_creature % EnemyKind::ALL.len()]
    }

    /// Handles the toolkit dialog (the menu seat's input; the world is read for the values).
    pub(super) fn handle_toolkit(
        &mut self,
        input: PlayerInput,
        world: &WorldState,
        requests: &mut Vec<ShellRequest>,
    ) {
        if input.backs_out() || input.pause {
            self.modal = None;
            return;
        }
        let view = ToolkitView::of(world, self.shown_test_mode(world.test_mode()));
        let rows = ToolkitRow::rows(view.descents);
        if input.menu_up {
            self.selected_menu_item = self.selected_menu_item.saturating_sub(1);
        }
        if input.menu_down {
            self.selected_menu_item += 1;
        }
        self.selected_menu_item = self.selected_menu_item.min(rows.len() - 1);
        let row = rows[self.selected_menu_item];
        let step = i32::from(input.menu_right) - i32::from(input.menu_left);
        let leap = i32::from(input.tab_next) - i32::from(input.tab_previous);
        if step != 0 || leap != 0 {
            self.step_toolkit(row, step, leap, &view, requests);
        }
        if input.confirm {
            self.confirm_toolkit(row, &view, requests);
        }
    }

    /// Steps a value row left or right (`leap`: the creature ten at a time).
    fn step_toolkit(
        &mut self,
        row: ToolkitRow,
        step: i32,
        leap: i32,
        view: &ToolkitView,
        requests: &mut Vec<ShellRequest>,
    ) {
        match row {
            ToolkitRow::Creature | ToolkitRow::SpawnCreature => {
                let count = i32::try_from(EnemyKind::ALL.len()).unwrap_or(i32::MAX);
                let at = i32::try_from(self.toolkit_creature).unwrap_or(0);
                let next = (at + step + leap * 10).rem_euclid(count);
                self.toolkit_creature = usize::try_from(next).unwrap_or(0);
                self.message = format!("Creature: {}.", self.toolkit_creature().name());
            }
            ToolkitRow::TimeOfDay => {
                let minutes = (i32::from(view.minutes) / 60 * 60 + step * 60).rem_euclid(24 * 60);
                self.set_toolkit_time(u16::try_from(minutes).unwrap_or(0), requests);
            }
            ToolkitRow::Weather => self.cycle_weather(view, step, requests),
            ToolkitRow::Difficulty => self.cycle_difficulty(view.difficulty, step, requests),
            _ if step != 0 => self.confirm_toolkit(row, view, requests),
            _ => {}
        }
    }

    /// Toggles or acts on a row.
    #[allow(clippy::too_many_lines, reason = "one arm per toolkit row")]
    fn confirm_toolkit(
        &mut self,
        row: ToolkitRow,
        view: &ToolkitView,
        requests: &mut Vec<ShellRequest>,
    ) {
        let mut state = view.test_mode;
        let (action, message) = match row {
            ToolkitRow::InfiniteSupplies => {
                state.infinite_supplies = !state.infinite_supplies;
                self.pending_test_mode = Some(state);
                (
                    TestModeAction::SetInfiniteSupplies(state.infinite_supplies),
                    format!(
                        "Infinite supplies {}.",
                        if state.infinite_supplies { "on" } else { "off" }
                    ),
                )
            }
            ToolkitRow::Restock => (
                TestModeAction::Restock,
                "Restocked: every consumable back to its stack.".to_owned(),
            ),
            ToolkitRow::StockAll => (
                TestModeAction::StockAll,
                "Stocked with everything the game has.".to_owned(),
            ),
            ToolkitRow::UnlockStory => {
                state.story_unlocked = !state.story_unlocked;
                self.pending_test_mode = Some(state);
                (
                    TestModeAction::SetStoryUnlocked(state.story_unlocked),
                    if state.story_unlocked {
                        "Story and codex unlocked.".to_owned()
                    } else {
                        "Story reset: every log and codex entry unread.".to_owned()
                    },
                )
            }
            ToolkitRow::Gate(descent) => {
                let cleared = !view.gate(descent);
                (
                    TestModeAction::SetGateCleared { descent, cleared },
                    format!(
                        "Gate {} {}.",
                        roman(descent + 1),
                        if cleared { "cleared" } else { "restored" }
                    ),
                )
            }
            ToolkitRow::EndingDone => (
                TestModeAction::SetEndingDone(!view.ending_done),
                if view.ending_done {
                    "The Heart sleeps again: the ending is undone.".to_owned()
                } else {
                    "The Heart woke: the ending counts as done.".to_owned()
                },
            ),
            ToolkitRow::TimeOfDay => {
                let next = TIME_STOPS
                    .iter()
                    .copied()
                    .find(|stop| *stop > view.minutes)
                    .unwrap_or(TIME_STOPS[0]);
                self.set_toolkit_time(next, requests);
                return;
            }
            ToolkitRow::DayLock => (
                TestModeAction::SetDayLock(!view.held),
                if view.held {
                    "The clock runs again.".to_owned()
                } else {
                    "The time of day is held.".to_owned()
                },
            ),
            ToolkitRow::Weather => {
                self.cycle_weather(view, 1, requests);
                return;
            }
            ToolkitRow::Creature => {
                self.step_toolkit(row, 1, 0, view, requests);
                return;
            }
            ToolkitRow::SpawnCreature => {
                let kind = self.toolkit_creature();
                if !view.difficulty.hostile() {
                    "Creatures are off (Peaceful): set Enemies first."
                        .clone_into(&mut self.message);
                    return;
                }
                (
                    TestModeAction::SpawnCreature(kind),
                    format!("{} spawned beside you.", kind.name()),
                )
            }
            ToolkitRow::ClearEnemies => (
                TestModeAction::ClearNearbyEnemies,
                "Creatures nearby cleared.".to_owned(),
            ),
            ToolkitRow::FreezeAi => {
                state.ai_frozen = !state.ai_frozen;
                self.pending_test_mode = Some(state);
                (
                    TestModeAction::SetAiFrozen(state.ai_frozen),
                    if state.ai_frozen {
                        "Creature AI frozen.".to_owned()
                    } else {
                        "Creatures move again.".to_owned()
                    },
                )
            }
            ToolkitRow::Difficulty => {
                self.cycle_difficulty(view.difficulty, 1, requests);
                return;
            }
        };
        self.message = message;
        self.toolkit_action(action, requests);
    }

    fn toolkit_action(&self, action: TestModeAction, requests: &mut Vec<ShellRequest>) {
        requests.push(ShellRequest::Action {
            seat: self.menu_seat,
            command: PlayerCommand::TestMode(action),
        });
    }

    fn set_toolkit_time(&mut self, minutes: u16, requests: &mut Vec<ShellRequest>) {
        self.message = format!("Time set to {:02}:{:02}.", minutes / 60, minutes % 60);
        self.toolkit_action(TestModeAction::SetTimeOfDay { minutes }, requests);
    }

    fn cycle_weather(&mut self, view: &ToolkitView, step: i32, requests: &mut Vec<ShellRequest>) {
        let choices = weather_choices();
        let count = i32::try_from(choices.len()).unwrap_or(1);
        let at = choices
            .iter()
            .position(|choice| *choice == view.test_mode.weather)
            .and_then(|at| i32::try_from(at).ok())
            .unwrap_or(0);
        let next = choices[usize::try_from((at + step).rem_euclid(count)).unwrap_or(0)];
        let mut state = view.test_mode;
        state.weather = next;
        self.pending_test_mode = Some(state);
        self.message = format!("Weather on every world: {}.", weather_name(next));
        self.toolkit_action(TestModeAction::SetWeather(next), requests);
    }

    fn cycle_difficulty(
        &mut self,
        difficulty: EnemyDifficulty,
        step: i32,
        requests: &mut Vec<ShellRequest>,
    ) {
        let all = EnemyDifficulty::ALL;
        let count = i32::try_from(all.len()).unwrap_or(1);
        let at = all
            .iter()
            .position(|level| *level == difficulty)
            .and_then(|at| i32::try_from(at).ok())
            .unwrap_or(0);
        let next = all[usize::try_from((at + step).rem_euclid(count)).unwrap_or(0)];
        requests.push(ShellRequest::Action {
            seat: self.menu_seat,
            command: PlayerCommand::SetEnemyDifficulty(next),
        });
        self.message = format!("Enemies: {} ({}).", next.name(), next.description());
    }

    /// Opens the toolkit dialog.
    pub(super) const fn open_toolkit(&mut self) {
        self.modal = Some(ModalScreen::TestToolkit);
        self.selected_menu_item = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{save::SettingsFile, shell::RunMode};
    use fatherload_simulation::{PlayerId, SimulationParams, worldgen::WorldSettings};

    fn deep_test_world() -> WorldState {
        let settings = WorldSettings {
            generator: fatherload_simulation::worldgen::GeneratorVersion::V11,
            ..WorldSettings::new(0x70_01C1)
        };
        let mut world = WorldState::new_game_with_settings(
            PlayerId::new(1),
            &settings,
            SimulationParams::default(),
        );
        world.enable_test_mode();
        world
    }

    fn press(f: impl FnOnce(&mut PlayerInput)) -> PlayerInput {
        let mut input = PlayerInput::default();
        f(&mut input);
        input
    }

    fn toolkit_shell() -> ShellState {
        let mut shell = ShellState::new(SettingsFile::default());
        shell.run_mode = RunMode::Paused;
        shell.open_toolkit();
        shell
    }

    fn actions(requests: &[ShellRequest]) -> Vec<PlayerCommand> {
        requests
            .iter()
            .filter_map(|request| match request {
                ShellRequest::Action { command, .. } => Some(command.clone()),
                _ => None,
            })
            .collect()
    }

    fn select(shell: &mut ShellState, world: &WorldState, row: ToolkitRow) {
        let rows = ToolkitRow::rows(world.world_settings().generator.has_descents());
        shell.selected_menu_item = rows.iter().position(|r| *r == row).expect("row");
    }

    #[test]
    fn every_row_submits_its_tool_for_the_menu_seat() {
        let world = deep_test_world();
        let rows = ToolkitRow::rows(true);
        assert!(rows.contains(&ToolkitRow::Gate(6)) && rows.contains(&ToolkitRow::EndingDone));
        assert!(!ToolkitRow::rows(false).contains(&ToolkitRow::Gate(0)));
        let mut shell = toolkit_shell();
        shell.menu_seat = 1;
        shell.toolkit_creature = 3;
        for row in rows {
            if row == ToolkitRow::Creature {
                continue;
            }
            select(&mut shell, &world, row);
            let mut requests = Vec::new();
            shell.handle_toolkit(press(|input| input.confirm = true), &world, &mut requests);
            let commands = actions(&requests);
            assert_eq!(commands.len(), 1, "{row:?}");
            assert!(
                requests
                    .iter()
                    .all(|request| matches!(request, ShellRequest::Action { seat: 1, .. }))
            );
            let expected = match row {
                ToolkitRow::InfiniteSupplies => {
                    PlayerCommand::TestMode(TestModeAction::SetInfiniteSupplies(false))
                }
                ToolkitRow::Restock => PlayerCommand::TestMode(TestModeAction::Restock),
                ToolkitRow::StockAll => PlayerCommand::TestMode(TestModeAction::StockAll),
                ToolkitRow::UnlockStory => {
                    PlayerCommand::TestMode(TestModeAction::SetStoryUnlocked(true))
                }
                ToolkitRow::Gate(descent) => {
                    PlayerCommand::TestMode(TestModeAction::SetGateCleared {
                        descent,
                        cleared: descent != 0,
                    })
                }
                ToolkitRow::EndingDone => {
                    PlayerCommand::TestMode(TestModeAction::SetEndingDone(true))
                }
                ToolkitRow::TimeOfDay | ToolkitRow::Weather | ToolkitRow::Difficulty => {
                    commands[0].clone()
                }
                ToolkitRow::DayLock => PlayerCommand::TestMode(TestModeAction::SetDayLock(true)),
                ToolkitRow::SpawnCreature => {
                    PlayerCommand::TestMode(TestModeAction::SpawnCreature(EnemyKind::ALL[3]))
                }
                ToolkitRow::ClearEnemies => {
                    PlayerCommand::TestMode(TestModeAction::ClearNearbyEnemies)
                }
                ToolkitRow::FreezeAi => PlayerCommand::TestMode(TestModeAction::SetAiFrozen(true)),
                ToolkitRow::Creature => unreachable!(),
            };
            assert_eq!(commands[0], expected, "{row:?}");
            shell.pending_test_mode = None;
        }
    }

    #[test]
    fn value_rows_step_left_and_right() {
        let world = deep_test_world();
        let mut shell = toolkit_shell();
        select(&mut shell, &world, ToolkitRow::Creature);
        let mut requests = Vec::new();
        shell.handle_toolkit(press(|input| input.menu_left = true), &world, &mut requests);
        assert_eq!(shell.toolkit_creature, EnemyKind::ALL.len() - 1, "wraps");
        shell.handle_toolkit(press(|input| input.tab_next = true), &world, &mut requests);
        assert_eq!(shell.toolkit_creature, 9);
        assert!(requests.is_empty(), "choosing a creature submits nothing");
        select(&mut shell, &world, ToolkitRow::Weather);
        shell.handle_toolkit(
            press(|input| input.menu_right = true),
            &world,
            &mut requests,
        );
        assert_eq!(
            actions(&requests),
            vec![PlayerCommand::TestMode(TestModeAction::SetWeather(
                weather_choices()[1]
            ))]
        );
        requests.clear();
        select(&mut shell, &world, ToolkitRow::TimeOfDay);
        shell.handle_toolkit(
            press(|input| input.menu_right = true),
            &world,
            &mut requests,
        );
        let (hours, _) = world.clock().hours_minutes();
        assert_eq!(
            actions(&requests),
            vec![PlayerCommand::TestMode(TestModeAction::SetTimeOfDay {
                minutes: (u16::from(hours) + 1) % 24 * 60
            })]
        );
        // Back closes the dialog.
        shell.handle_toolkit(press(|input| input.back = true), &world, &mut requests);
        assert_eq!(shell.modal, None);
    }

    #[test]
    fn spawning_on_a_peaceful_world_says_so_instead() {
        let mut world = deep_test_world();
        world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
        let mut shell = toolkit_shell();
        select(&mut shell, &world, ToolkitRow::SpawnCreature);
        let mut requests = Vec::new();
        shell.handle_toolkit(press(|input| input.confirm = true), &world, &mut requests);
        assert!(requests.is_empty());
        assert!(shell.message.contains("Peaceful"));
    }
}
