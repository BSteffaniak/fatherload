//! The shell's side of the test-mode command palette (`crate::console`): opening it in a
//! test-mode world, feeding it its seat's keys while it is open (the seat's rig and hotkeys
//! rest meanwhile) and carrying out what its commands ask: actions for the seat, debug views and
//! the time scale.

use crate::{
    console::{ConsoleContext, dispatch::Effect},
    input::PlayerInput,
};

use super::{RunMode, ShellContext, ShellRequest, ShellState};

/// A frame of input with only the window's close request left (the palette ate the rest).
fn eaten(input: PlayerInput) -> PlayerInput {
    PlayerInput {
        exit_requested: input.exit_requested,
        ..PlayerInput::default()
    }
}

impl ShellState {
    /// The palette's frame: runs before every other handler so its keys reach nothing else.
    pub(super) fn update_console(
        &mut self,
        inputs: &mut [PlayerInput],
        ctx: ShellContext<'_>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let in_game = !matches!(self.run_mode, RunMode::Title | RunMode::Lobby);
        self.console.available = in_game && ctx.world.is_test_mode();
        if !self.console.available {
            if self.console.open {
                self.console.close();
            }
            return;
        }
        let console_ctx = ConsoleContext {
            world: ctx.world,
            players: ctx.players,
            online: ctx.online,
        };
        let playing = self.run_mode == RunMode::Playing && self.modal.is_none();
        if !playing {
            // The pause menu (or a global dialog) took over.
            self.console.close();
            return;
        }
        if self.console.open {
            let seat = self.console.seat;
            let input = inputs.get(seat).copied().unwrap_or_default();
            let effects = self.console.handle(input, console_ctx);
            self.apply_console_effects(effects, requests);
            if let Some(slot) = inputs.get_mut(seat) {
                *slot = eaten(*slot);
            }
        } else if let Some(seat) = inputs
            .iter()
            .position(|input| input.console)
            .filter(|seat| self.seat(*seat).modal.is_none())
        {
            self.console.open_for(seat);
            inputs[seat] = eaten(inputs[seat]);
        }
    }

    /// Carries out a command's effects.
    pub(super) fn apply_console_effects(
        &mut self,
        effects: Vec<(usize, Effect)>,
        requests: &mut Vec<ShellRequest>,
    ) {
        for (seat, effect) in effects {
            match effect {
                Effect::Act(command) => {
                    requests.push(ShellRequest::Action { seat, command });
                }
                Effect::View(view, on) => {
                    let on = match on {
                        Some(on) => self.debug_views.set(view, on),
                        None => self.debug_views.toggle(view),
                    };
                    self.console.print(format!(
                        "  {}: {}{}",
                        view.title(),
                        if on { "on" } else { "off" },
                        if on && view == crate::debug_views::DebugView::FreeCamera {
                            " (stick pans, PgUp/PgDn zoom)"
                        } else {
                            ""
                        }
                    ));
                }
                Effect::ViewsOff => {
                    for view in crate::debug_views::DebugView::ALL {
                        self.debug_views.set(view, false);
                    }
                }
                Effect::Speed(scale) => self.time_scale = scale,
            }
        }
    }
}
