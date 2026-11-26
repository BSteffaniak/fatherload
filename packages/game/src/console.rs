//! The test-mode command palette (Wave 14 console, expansion plan §19 W14-D).
//!
//! The backtick key (or the touch screen's `CMD` button, which brings up the phone's keyboard
//! through the page's text field in the browser build) opens it in a test-mode world; it never
//! opens anywhere else. Typed commands ([`parse`]) complete with Tab ([`complete`]) and run
//! through [`dispatch`]: travel and stocking submit the same `TestModeAction`s the Atlas,
//! `--start-at` and the bots submit, for the seat that opened the palette (seat 1's keyboard
//! or the touch screen); debug views and the time scale are local switches.
//!
//! While it is open the palette owns its seat's keys: the seat's rig stands still and its
//! hotkeys rest. The world keeps running (`speed pause` stops it).

pub mod complete;
pub mod dispatch;
pub mod names;
pub mod parse;
pub mod start_at;
#[cfg(test)]
mod tests;

use fatherload_simulation::{PlayerCommand, PlayerId, WorldState};

use crate::input::PlayerInput;

use self::dispatch::{DispatchContext, Effect, SeatTravel};

/// Most lines of output the palette keeps.
pub const MAX_LINES: usize = 14;
/// Most commands the history keeps.
const MAX_HISTORY: usize = 64;
/// Longest command line.
pub const MAX_TEXT: usize = 120;
/// Most completions shown under the line.
pub const MAX_SHOWN_COMPLETIONS: usize = 6;

/// The palette's state (one palette; it acts for the seat that opened it).
#[derive(Clone, Debug, Default)]
pub struct Console {
    /// Open and taking its seat's keys.
    pub open: bool,
    /// The world is a test-mode world being played (the palette may open).
    pub available: bool,
    /// The seat it acts for.
    pub seat: usize,
    /// The line being typed.
    pub text: String,
    /// Output, oldest first.
    pub lines: Vec<String>,
    /// Completions of the line as last offered (Tab cycles them).
    pub completions: Vec<String>,
    /// Which completion Tab put on the line.
    pub completion_at: Option<usize>,
    history: Vec<String>,
    history_at: Option<usize>,
    /// Each seat's travel memory (cursors, back-stack, bookmarks, preview).
    travel: Vec<SeatTravel>,
    /// Actions waiting for a seat's rig to reach the home world.
    waiting: Vec<(usize, PlayerCommand)>,
}

/// What the world and seats look like to the palette this frame.
#[derive(Clone, Copy, Debug)]
pub struct ConsoleContext<'w> {
    pub world: &'w WorldState,
    /// The player each local seat controls.
    pub players: &'w [PlayerId],
    pub online: bool,
}

impl Console {
    /// Opens the palette for `seat`.
    pub fn open_for(&mut self, seat: usize) {
        if !self.available {
            return;
        }
        self.open = true;
        self.seat = seat;
        self.text.clear();
        self.completions.clear();
        self.completion_at = None;
        if self.lines.is_empty() {
            self.lines
                .push("Test-mode palette: help lists commands, Tab completes.".to_owned());
        }
    }

    /// Closes the palette.
    pub fn close(&mut self) {
        self.open = false;
        self.text.clear();
        self.completions.clear();
        self.completion_at = None;
        self.history_at = None;
    }

    /// Forgets every seat's travel memory and waiting actions (a new world).
    pub fn reset_world(&mut self) {
        self.close();
        self.travel.clear();
        self.waiting.clear();
    }

    /// The bookmark names of the palette's seat (completion).
    #[must_use]
    pub fn bookmarks(&self) -> Vec<String> {
        self.travel
            .get(self.seat)
            .map(|travel| travel.bookmarks.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn seat_travel(&mut self, seat: usize) -> &mut SeatTravel {
        if self.travel.len() <= seat {
            self.travel.resize_with(seat + 1, SeatTravel::default);
        }
        &mut self.travel[seat]
    }

    /// Adds a line of output.
    pub fn print(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
        if self.lines.len() > MAX_LINES {
            let extra = self.lines.len() - MAX_LINES;
            self.lines.drain(..extra);
        }
    }

    /// One frame of the palette seat's keys while open. Returns the effects of a command run
    /// this frame (for the palette's seat), and closes the palette on the backtick or Esc.
    pub fn handle(&mut self, input: PlayerInput, ctx: ConsoleContext<'_>) -> Vec<(usize, Effect)> {
        // `E` also counts as "confirm" in menus; while typing it is just a letter.
        let typed_confirm_key = matches!(input.text_input, Some('e' | 'E'));
        if input.console || input.back || (input.cancel && !input.text_backspace) {
            self.close();
            return Vec::new();
        }
        if input.text_backspace {
            self.text.pop();
            self.completion_at = None;
        }
        if let Some(character) = input.text_input
            && !character.is_control()
            && !matches!(character, '`' | '~')
            && self.text.len() < MAX_TEXT
        {
            self.text.push(character);
            self.completion_at = None;
        }
        if input.inventory {
            self.tab_complete();
        } else if input.text_input.is_some() || input.text_backspace {
            self.completions = complete::complete(&self.text, &self.bookmarks());
        }
        if input.text_input.is_none() {
            if input.menu_up {
                self.history_step(true);
            } else if input.menu_down {
                self.history_step(false);
            }
        }
        if input.confirm && !typed_confirm_key {
            let line = std::mem::take(&mut self.text);
            self.completions.clear();
            self.completion_at = None;
            self.history_at = None;
            return self.run(&line, ctx);
        }
        Vec::new()
    }

    /// Tab: the next completion of the line (the first when the line changed).
    fn tab_complete(&mut self) {
        if self.completion_at.is_none() {
            self.completions = complete::complete(&self.text, &self.bookmarks());
        }
        match self.completions.len() {
            0 => {}
            1 => {
                self.text = format!("{} ", self.completions[0]);
                self.completions = complete::complete(&self.text, &self.bookmarks());
                self.completion_at = None;
            }
            count => {
                let common = complete::common_prefix(&self.completions);
                if self.completion_at.is_none() && common.len() > self.text.trim_end().len() {
                    self.text = common;
                    self.completions = complete::complete(&self.text, &self.bookmarks());
                } else {
                    let next = self.completion_at.map_or(0, |at| (at + 1) % count);
                    self.completion_at = Some(next);
                    self.text.clone_from(&self.completions[next]);
                }
            }
        }
    }

    fn history_step(&mut self, older: bool) {
        if self.history.is_empty() {
            return;
        }
        let last = self.history.len() - 1;
        self.history_at = match (self.history_at, older) {
            (None, true) => Some(last),
            (None, false) => None,
            (Some(at), true) => Some(at.saturating_sub(1)),
            (Some(at), false) if at >= last => None,
            (Some(at), false) => Some(at + 1),
        };
        self.text = self
            .history_at
            .map(|at| self.history[at].clone())
            .unwrap_or_default();
    }

    /// Runs one command line for the palette's seat (also the tests' and tools' entry).
    pub fn run(&mut self, line: &str, ctx: ConsoleContext<'_>) -> Vec<(usize, Effect)> {
        let line = line.trim();
        if line.is_empty() {
            // Enter on an empty line travels to the previewed destination.
            if self
                .travel
                .get(self.seat)
                .is_some_and(|seat| seat.preview.is_some())
            {
                return self.run("go", ctx);
            }
            return Vec::new();
        }
        self.print(format!("> {line}"));
        if self.history.last().is_none_or(|last| last != line) {
            self.history.push(line.to_owned());
            if self.history.len() > MAX_HISTORY {
                self.history.remove(0);
            }
        }
        if matches!(line, "close" | "exit" | "quit") {
            self.close();
            return Vec::new();
        }
        let command = match parse::parse(line) {
            Ok(Some(command)) => command,
            Ok(None) => return Vec::new(),
            Err(error) => {
                self.print(format!("  {error}"));
                return Vec::new();
            }
        };
        let seat = self.seat;
        let Some(player) = ctx.players.get(seat).copied() else {
            self.print("  no rig for this seat");
            return Vec::new();
        };
        let dispatch_ctx = DispatchContext {
            world: ctx.world,
            player,
            online: ctx.online,
        };
        let done = dispatch::dispatch(command, dispatch_ctx, self.seat_travel(seat));
        for line in done.lines {
            self.print(format!("  {line}"));
        }
        let mut effects = Vec::new();
        for effect in done.effects {
            if let Effect::ActWhenHome(command) = effect {
                self.waiting.push((seat, command));
            } else {
                effects.push((seat, effect));
            }
        }
        effects
    }

    /// Actions whose seat's rig has reached the home world since they were queued.
    pub fn take_ready(&mut self, ctx: ConsoleContext<'_>) -> Vec<(usize, PlayerCommand)> {
        let at_home = |seat: usize| {
            ctx.players.get(seat).is_some_and(|player| {
                ctx.world.place_of(*player).is_home_store()
                    && ctx
                        .world
                        .player(*player)
                        .is_some_and(|rig| !rig.flight.away())
            })
        };
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|(seat, _)| at_home(*seat));
        self.waiting = waiting;
        ready
    }
}
