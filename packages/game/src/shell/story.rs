//! Stories in the shell (WP-STORY): the talk panel's story rows, the per-seat journal and the
//! story moments (a new story, its climax, its end).
//!
//! The journal ([`super::ModalScreen::Journal`]) sits in the map's page cycle between the quest
//! log and the milestones. Its rows are the stories under way (picking one shows its text), the
//! stories told (their epilogues), "Give up this story" (confirm twice) and "Turn the page"
//! (on to the milestones), so every page works by keys, pad, mouse and touch alike: a tap
//! selects a row and confirms it, and confirming a story row only shows it.

use fatherload_simulation::{
    PlayerCommand,
    people::{ArcId, ArcLog, PeopleCommand},
    world::ArcTalk,
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::PlayerInput;

/// How long a story moment stays up, in shell ticks (60 per second): long enough to read.
pub const STORY_MOMENT_TICKS: u64 = 540;

/// What a story moment marks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryMomentKind {
    /// A story taken on.
    Begun,
    /// A story's last beat: its climax.
    Climax,
    /// A story told back to whoever asked.
    Ended,
    /// A story given up (or lost to a twist).
    Lost,
    /// An expedition log read (Wave 8 story).
    ExpeditionLog,
    /// One of Dov Halloran's radio echoes.
    RadioEcho,
    /// One of Dov's ghost recordings.
    GhostRecording,
    /// An anomaly room entered.
    Anomaly,
    /// A deep settlement remembering the Fathers' expedition (Wave 8 integration).
    Remembered,
    /// One of the Drowned and Frozen's story beats found at its landmark (Wave 9).
    DeepLore,
}

/// A card across the seat's view telling a moment of a story.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoryMoment {
    pub kind: StoryMomentKind,
    pub title: String,
    pub text: String,
    /// `ShellState::update_ticks` when it went up.
    pub tick: u64,
}

impl StoryMoment {
    #[must_use]
    pub const fn showing(&self, now: u64) -> bool {
        now.saturating_sub(self.tick) < STORY_MOMENT_TICKS
    }
}

/// A story row of the talk panel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryChoice {
    /// Ask whether they need a hand (or how the story they gave is going).
    Ask { running: bool },
    /// Take on the story they just pitched.
    Accept,
    /// Tell them the story is done, ending with outcome `outcome`.
    Ending { outcome: u8, label: &'static str },
}

impl StoryChoice {
    /// The row label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ask { running: false } => "Need a hand with anything?",
            Self::Ask { running: true } => "About that favour...",
            Self::Accept => "I'll do it.",
            Self::Ending { label, .. } => label,
        }
    }
}

/// The story rows the talk panel offers for `talk` (first, so stories are easy to find).
#[must_use]
pub fn story_choices(talk: &ArcTalk) -> Vec<StoryChoice> {
    match talk {
        ArcTalk::Nothing => Vec::new(),
        ArcTalk::HasWork(_) => vec![StoryChoice::Ask { running: false }],
        ArcTalk::Pitched(_) => vec![StoryChoice::Accept],
        ArcTalk::Running(_) => vec![StoryChoice::Ask { running: true }],
        ArcTalk::Ready { endings, .. } => endings
            .iter()
            .map(|(outcome, label)| StoryChoice::Ending {
                outcome: *outcome,
                label,
            })
            .collect(),
    }
}

/// One row of the journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalRow {
    /// A story under way (index into [`ArcLog::active`]).
    Running(usize),
    /// A story told (index into [`ArcLog::history`], newest first in the list).
    Told(usize),
    /// Give up the story last looked at.
    GiveUp,
    /// On to the milestones page.
    TurnPage,
}

/// Stories told the journal lists (the newest).
pub const TOLD_SHOWN: usize = fatherload_simulation::people::story::run::HISTORY_LIMIT;

/// The journal's rows for `log`.
#[must_use]
pub fn journal_rows(log: Option<&ArcLog>) -> Vec<JournalRow> {
    let mut rows = Vec::new();
    if let Some(log) = log {
        rows.extend((0..log.active.len()).map(JournalRow::Running));
        rows.extend(
            (0..log.history.len())
                .rev()
                .take(TOLD_SHOWN)
                .map(JournalRow::Told),
        );
        if !log.active.is_empty() {
            rows.push(JournalRow::GiveUp);
        }
    }
    rows.push(JournalRow::TurnPage);
    rows
}

/// What the seat's journal is showing: its rows and the story in focus.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SeatJournal {
    /// The row whose story the detail pane shows (a running or told story's row).
    pub focus: usize,
    /// The story a first "give up" confirm armed.
    pub abandon_armed: Option<ArcId>,
}

impl SeatJournal {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            focus: 0,
            abandon_armed: None,
        }
    }
}

impl SeatUi {
    /// The journal page.
    pub(super) fn handle_journal(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        if input.backs_out() {
            self.modal = None;
            self.journal.abandon_armed = None;
            return;
        }
        let log = ctx.player().map(|player| &player.quests.arcs);
        let rows = journal_rows(log);
        let before = self.selected_menu_item;
        self.move_selection(input, rows.len().saturating_sub(1));
        self.selected_menu_item = self.selected_menu_item.min(rows.len().saturating_sub(1));
        if self.selected_menu_item != before {
            self.journal.abandon_armed = None;
        }
        let selected = rows.get(self.selected_menu_item).copied();
        if matches!(selected, Some(JournalRow::Running(_) | JournalRow::Told(_))) {
            self.journal.focus = self.selected_menu_item;
        }
        if input.tab_next {
            self.open_modal(ModalScreen::Achievements);
            return;
        }
        if input.tab_previous {
            self.open_modal(ModalScreen::QuestLog);
            return;
        }
        if !input.confirm {
            return;
        }
        match selected {
            Some(JournalRow::TurnPage) | None => {
                self.journal.abandon_armed = None;
                self.open_modal(ModalScreen::Achievements);
            }
            Some(JournalRow::Running(_) | JournalRow::Told(_)) => cues.push(SoundCue::UiMove),
            Some(JournalRow::GiveUp) => {
                let focused = match rows.get(self.journal.focus) {
                    Some(JournalRow::Running(index)) => log.and_then(|log| log.active.get(*index)),
                    _ => log.and_then(|log| log.active.first()),
                };
                let Some(run) = focused else {
                    return;
                };
                let arc = run.arc();
                let title = run
                    .template()
                    .map_or("this story", |template| template.title);
                if self.journal.abandon_armed == Some(arc) {
                    self.journal.abandon_armed = None;
                    if ctx.player.is_some() {
                        requests.push(ShellRequest::Action {
                            seat: ctx.seat,
                            command: PlayerCommand::People(PeopleCommand::AbandonArc { arc }),
                        });
                    }
                    self.message = format!("You let \"{title}\" go.");
                    self.selected_menu_item = 0;
                    self.journal.focus = 0;
                    cues.push(SoundCue::UiBack);
                } else {
                    self.journal.abandon_armed = Some(arc);
                    self.message = format!("Give up \"{title}\"? Confirm again to let it go.");
                    cues.push(SoundCue::UiError);
                }
            }
        }
    }
}
