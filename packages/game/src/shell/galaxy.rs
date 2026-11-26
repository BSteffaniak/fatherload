//! The galaxy story in the shell (Wave 13 story): the road's journal page, the finale's
//! choice, the words for the cards and messages its events raise, and the HUD's goal line.
//!
//! * **The journal** ([`ModalScreen::GalaxyJournal`], "The Builders' Road"): set course for
//!   the current goal, ask the archivist, the finale's choice once it is open; then each
//!   chapter's page (newest first: the one under way with its goals ticked off), the space logs
//!   found, the discovery log's leads (selecting an open lead sets course for it) and the
//!   crew's completion stats. It sits after the star charts (and the "Ask Index" page) in the
//!   map's page cycle once the story has begun, and the nav chart opens it in space. Rows work
//!   by keys, pad, mouse and touch alike: a tap selects a row and confirms it.
//! * **The choice** ([`ModalScreen::GalaxyChoice`]): opens by itself when the Warden is calm
//!   and the seat's rig is at the Harbour's gate (and from the journal): the bell, the lamp,
//!   or not yet. Choosing sends the crew's one choice.
//! * **The ending** plays per seat from the choice (`SeatUi::galaxy_finale`, a
//!   [`FinaleView`]): each seat skips only its own.
//!
//! Everything reads the crew's one [`fatherload_simulation::galaxy_story::GalaxyStory`], so
//! every seat and partner reads the same road.

use fatherload_simulation::{
    PlayerCommand, PlayerId, WorldState,
    galaxy_story::{
        self, CHAPTER_COUNT, GalaxyCommand, GalaxyEvent, GalaxyRefusal, GalaxyStory, Goal,
        finale::{self, FinaleChoice, FinaleView},
        journal::{FRAGMENTS, REMARKS, SPACE_LOGS},
    },
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue, StoryMomentKind};
use crate::input::PlayerInput;

/// Most leads the journal lists (open ones first).
pub const JOURNAL_LEADS: usize = 24;

/// One row of the road's journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GalaxyRow {
    /// Set course for the current goal (the player's own waypoint).
    SetCourse,
    /// The finale's choice (open once the Warden is calm).
    Choose,
    /// Ask the archivist about the road.
    AskArchivist,
    /// A chapter's page (done, or under way).
    Chapter(u8),
    /// A space log found.
    Log(u8),
    /// A lead of the discovery log (its index).
    Lead(usize),
    /// The crew's completion stats.
    Stats,
    /// On to the map.
    TurnPage,
}

/// Whether the road's journal is in the page cycle (the story has begun).
#[must_use]
pub const fn has_galaxy_page(world: &WorldState) -> bool {
    world.galaxy_story().begun
}

/// The journal's rows.
#[must_use]
pub fn galaxy_rows(world: &WorldState) -> Vec<GalaxyRow> {
    let story = world.galaxy_story();
    let mut rows = Vec::new();
    if !story.begun {
        rows.push(GalaxyRow::TurnPage);
        return rows;
    }
    if world.galaxy_waypoint().is_some() {
        rows.push(GalaxyRow::SetCourse);
    }
    if story.current_goal().map(|goal| goal.goal) == Some(Goal::Choose) {
        rows.push(GalaxyRow::Choose);
    }
    rows.push(GalaxyRow::AskArchivist);
    let last = story.chapter.min(CHAPTER_COUNT - 1);
    for chapter in (0..=last).rev() {
        rows.push(GalaxyRow::Chapter(chapter));
    }
    for log in story.logs.iter().rev() {
        rows.push(GalaxyRow::Log(*log));
    }
    let open = story
        .leads
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, record)| !record.done)
        .map(|(index, _)| index);
    let done = story
        .leads
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, record)| record.done)
        .map(|(index, _)| index);
    rows.extend(open.chain(done).take(JOURNAL_LEADS).map(GalaxyRow::Lead));
    rows.push(GalaxyRow::Stats);
    rows.push(GalaxyRow::TurnPage);
    rows
}

/// Chapter `chapter`'s title with its number ("3. The Silent Foundry").
#[must_use]
pub fn chapter_title(chapter: u8) -> String {
    galaxy_story::chapter(chapter)
        .map_or_else(String::new, |def| format!("{}. {}", chapter + 1, def.title))
}

/// The current goal's line for the HUD: the chapter and what to do now.
#[must_use]
pub fn goal_line(world: &WorldState) -> Option<(String, String)> {
    let story = world.galaxy_story();
    let goal = story.current_goal()?;
    let words = world.galaxy_words(story.chapter);
    let mut text = galaxy_story::fill(goal.text, &words);
    if let Goal::Hold { seconds, .. } = goal.goal
        && story.raid_landed
    {
        let held = story.held_millis / 1_000;
        text = format!("{text} ({held}/{seconds} s)");
    }
    Some((chapter_title(story.chapter), text))
}

/// Where the site the seat is looking for on this world lies, in words (the story's relic,
/// else the nearest open lead's ruin mark): "312 columns right to the foundry door, then dig
/// 30 rows down". A world's strip is thousands of columns round; this is the ground's compass.
#[must_use]
pub fn site_words(world: &WorldState, player: PlayerId) -> Option<String> {
    let bearing = world.galaxy_site_bearing(player)?;
    let name = if bearing.story {
        world
            .galaxy_relic()
            .map_or("the relic", |(_, _, name)| name)
            .to_owned()
    } else {
        "the ruin's mark".to_owned()
    };
    let (above, below) = galaxy_story::RELIC_REACH_ROWS;
    // A hold counts from further out than a find.
    let holding = bearing.story
        && world
            .galaxy_story()
            .current_goal()
            .is_some_and(|goal| matches!(goal.goal, Goal::Hold { .. }));
    let reach = if holding {
        galaxy_story::HOLD_REACH_COLUMNS
    } else {
        galaxy_story::RELIC_REACH_COLUMNS
    };
    let near = bearing.columns.abs() <= reach;
    let plural = |count: i32, one: &str, many: &str| {
        if count.abs() == 1 {
            format!("1 {one}")
        } else {
            format!("{} {many}", count.abs())
        }
    };
    Some(if !near {
        let side = if bearing.columns > 0 { "right" } else { "left" };
        let mut words = format!(
            "{} {side} to {name}",
            plural(bearing.columns, "column", "columns")
        );
        if bearing.buried {
            words.push_str(", then dig down");
        }
        words
    } else if bearing.rows > below {
        format!("Dig {} down to {name}", plural(bearing.rows, "row", "rows"))
    } else if bearing.rows < -above {
        format!("{} up to {name}", plural(bearing.rows, "row", "rows"))
    } else {
        format!("At {name}")
    })
}

/// A chapter's page: its intro, its goals (ticked off) and, once done, its outro.
#[must_use]
pub fn chapter_page(world: &WorldState, chapter: u8) -> String {
    let story = world.galaxy_story();
    let Some(def) = galaxy_story::chapter(chapter) else {
        return String::new();
    };
    let words = world.galaxy_words(chapter);
    // The goals first (ticked off), then the chapter's words: the done chapter's close, the
    // one under way's opening.
    let mut page = String::new();
    for (index, goal) in def.goals.iter().enumerate() {
        let done =
            chapter < story.chapter || chapter == story.chapter && index < usize::from(story.step);
        page.push_str(if done { "[x] " } else { "[ ] " });
        page.push_str(&galaxy_story::fill(goal.text, &words));
        page.push('\n');
    }
    let close = if chapter < story.chapter {
        def.outro
    } else {
        def.intro
    };
    page.push_str(&galaxy_story::fill(close, &words));
    page
}

/// A space log's title and words.
#[must_use]
pub fn log_entry(log: u8) -> Option<(String, String)> {
    let entry = SPACE_LOGS.get(usize::from(log))?;
    Some((
        entry.title.to_owned(),
        format!("{}: {}", entry.voice.signature(), entry.text),
    ))
}

/// Lead `index` of the discovery log: its title, its kind and step in words, and its text.
#[must_use]
pub fn lead_entry(world: &WorldState, index: usize) -> Option<(String, String, String)> {
    let record = world.galaxy_story().leads.get(index)?;
    let lead = GalaxyStory::lead_of_seed(world.world_seed(), record)?;
    let system = world.space().space().system(record.system)?;
    let status = if record.done {
        format!("{}: done", lead.kind.label())
    } else {
        format!(
            "{}: step {} of {}",
            lead.kind.label(),
            record.step + 1,
            lead.steps.len()
        )
    };
    let next = if record.done {
        format!(
            "Done: {} credits. {}",
            crate::credits_text::grouped(lead.credits),
            FRAGMENTS
                .get(usize::from(lead.fragment))
                .copied()
                .unwrap_or_default()
        )
    } else {
        format!(
            "{} Next: {} Pays {} credits. Confirm to set course.",
            lead.text,
            lead.step_text(record.step, &system),
            crate::credits_text::grouped(lead.credits)
        )
    };
    Some((lead.title, status, next))
}

/// The crew's completion stats, in words.
#[must_use]
pub fn stats_words(world: &WorldState) -> String {
    let stats = world.galaxy_story().stats_of_seed(world.world_seed());
    format!(
        "Chapters {} of {} | Goals {} of {} | Systems visited {} | Worlds landed on {} | \
         Leads {} found, {} done ({} lost ships, {} distress calls, {} ruins, {} surveys, {} \
         requests) | Logs {} of {} | Fragments {} of {}",
        stats.chapters_done,
        CHAPTER_COUNT,
        stats.goals_done,
        galaxy_story::goal_count(),
        stats.systems_visited,
        stats.worlds_landed,
        stats.leads_found,
        stats.leads_done,
        stats.leads_by_kind[0],
        stats.leads_by_kind[1],
        stats.leads_by_kind[2],
        stats.leads_by_kind[3],
        stats.leads_by_kind[4],
        stats.logs,
        SPACE_LOGS.len(),
        stats.fragments,
        FRAGMENTS.len(),
    )
}

/// What the archivist says in a remark.
#[must_use]
pub fn remark_words(remark: u32) -> Option<&'static str> {
    REMARKS
        .get(usize::try_from(remark).ok()?)
        .map(|line| line.line)
}

/// What a galaxy event raises: a card on every local seat (the crew's road is shared) and/or
/// a line for the seat whose player did it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GalaxyWords {
    pub card: Option<(StoryMomentKind, String, String)>,
    pub message: Option<String>,
    /// The message goes to every local seat (not only the doer's).
    pub everyone: bool,
}

/// The words of `what` (with `detail`) in `world` (the state after the tick).
#[must_use]
#[allow(clippy::too_many_lines, reason = "one arm per galaxy event")]
pub fn event_words(world: &WorldState, what: GalaxyEvent, detail: u32) -> GalaxyWords {
    let chapter_of = |detail: u32| u8::try_from(detail).unwrap_or(u8::MAX);
    match what {
        GalaxyEvent::Begun => GalaxyWords {
            message: Some(
                "The builders' road is open: the journal (map pages) keeps where it goes."
                    .to_owned(),
            ),
            everyone: true,
            ..GalaxyWords::default()
        },
        GalaxyEvent::ChapterBegun => {
            let chapter = chapter_of(detail);
            let words = world.galaxy_words(chapter);
            GalaxyWords {
                card: galaxy_story::chapter(chapter).map(|def| {
                    (
                        StoryMomentKind::RoadChapter,
                        chapter_title(chapter),
                        galaxy_story::fill(def.intro, &words),
                    )
                }),
                ..GalaxyWords::default()
            }
        }
        GalaxyEvent::ChapterDone => {
            let chapter = chapter_of(detail);
            let words = world.galaxy_words(chapter);
            GalaxyWords {
                card: galaxy_story::chapter(chapter).map(|def| {
                    (
                        StoryMomentKind::RoadFound,
                        format!("{} (done)", chapter_title(chapter)),
                        galaxy_story::fill(def.outro, &words),
                    )
                }),
                message: galaxy_story::chapter(chapter).map(|def| {
                    format!(
                        "Chapter {} done: +{} credits. The journal keeps it.",
                        chapter + 1,
                        crate::credits_text::grouped(def.credits)
                    )
                }),
                everyone: false,
            }
        }
        GalaxyEvent::GoalDone => GalaxyWords {
            message: goal_line(world).map(|(_, next)| format!("Done. Next: {next}")),
            everyone: true,
            ..GalaxyWords::default()
        },
        GalaxyEvent::LogFound => GalaxyWords {
            card: log_entry(chapter_of(detail))
                .map(|(title, text)| (StoryMomentKind::RoadLog, title, text)),
            ..GalaxyWords::default()
        },
        GalaxyEvent::RelicFound => GalaxyWords {
            message: galaxy_story::chapter(chapter_of(detail))
                .map(|def| format!("Found {}.", def.relic)),
            everyone: true,
            ..GalaxyWords::default()
        },
        GalaxyEvent::HoldBegun => GalaxyWords {
            message: Some("Raiders are dropping in: hold the site!".to_owned()),
            everyone: true,
            ..GalaxyWords::default()
        },
        GalaxyEvent::WardenAppeared => GalaxyWords {
            card: Some((
                StoryMomentKind::RoadChapter,
                "The Warden".to_owned(),
                "A door the size of a moon turns its lamps on you. Watch its warnings: it \
                 always shows you what comes next. The warp button escapes."
                    .to_owned(),
            )),
            ..GalaxyWords::default()
        },
        GalaxyEvent::WardenCalmed => GalaxyWords {
            card: Some((
                StoryMomentKind::RoadFound,
                "The Warden is calm".to_owned(),
                format!(
                    "Its lamps go dark one by one, and then, very slowly, its eye turns to your \
                     drive and lights gold. {}",
                    finale::HEART_TEXT
                ),
            )),
            ..GalaxyWords::default()
        },
        GalaxyEvent::Chosen => GalaxyWords {
            message: FinaleChoice::from_code(detail)
                .map(|choice| format!("{}: {}", finale::title(choice), finale::lead(choice))),
            everyone: true,
            ..GalaxyWords::default()
        },
        GalaxyEvent::LeadFound => GalaxyWords {
            message: lead_entry(world, usize::try_from(detail).unwrap_or(usize::MAX)).map(
                |(title, status, _)| format!("New lead: {title} ({status}). See the journal."),
            ),
            ..GalaxyWords::default()
        },
        GalaxyEvent::LeadStep => GalaxyWords {
            message: lead_entry(world, usize::try_from(detail).unwrap_or(usize::MAX))
                .map(|(title, status, _)| format!("{title}: {status}.")),
            ..GalaxyWords::default()
        },
        GalaxyEvent::LeadDone => {
            let entry = lead_entry(world, usize::try_from(detail).unwrap_or(usize::MAX));
            GalaxyWords {
                card: entry.map(|(title, _, text)| (StoryMomentKind::RoadDiscovery, title, text)),
                ..GalaxyWords::default()
            }
        }
        GalaxyEvent::ArchivistRemark => GalaxyWords {
            message: remark_words(detail).map(|line| format!("Index: \"{line}\"")),
            everyone: true,
            ..GalaxyWords::default()
        },
        GalaxyEvent::CourseSet => GalaxyWords {
            message: Some(if detail == 0 {
                "Course set for the road: the warp's target is the story's.".to_owned()
            } else {
                "Course set for the lead.".to_owned()
            }),
            ..GalaxyWords::default()
        },
        GalaxyEvent::Refused => GalaxyWords {
            message: GalaxyRefusal::from_code(detail).map(|why| why.message().to_owned()),
            ..GalaxyWords::default()
        },
        GalaxyEvent::FragmentFound => GalaxyWords {
            message: FRAGMENTS
                .get(usize::try_from(detail).unwrap_or(usize::MAX))
                .map(|fragment| format!("A builders' fragment: {fragment}")),
            ..GalaxyWords::default()
        },
    }
}

/// Whether the seat's `player` stands where the choice is made (the Warden calm, at the gate).
#[must_use]
pub fn choice_open_for(world: &WorldState, player: Option<PlayerId>) -> bool {
    let story = world.galaxy_story();
    if story.current_goal().map(|goal| goal.goal) != Some(Goal::Choose) {
        return false;
    }
    let (Some(player), Some(plan)) = (player, world.galaxy_plan()) else {
        return false;
    };
    world.player(player).is_some_and(|rig| {
        rig.flight.away()
            && plan.harbour.near_gate(
                fatherload_simulation::space::SpaceTile::of_pos(rig.flight.pos),
                finale::CHOICE_REACH_TILES,
            )
    })
}

/// The choice page's rows.
pub const CHOICE_ROWS: [Option<FinaleChoice>; 3] = [
    Some(FinaleChoice::RingTheBell),
    Some(FinaleChoice::KeepTheLamp),
    None,
];

/// The seat's state for the galaxy story.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SeatGalaxy {
    /// The seat's own view of the ending (its skips).
    pub finale: FinaleView,
    /// The choice page was offered at the gate (it opens once by itself).
    pub choice_offered: bool,
}

impl SeatGalaxy {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            finale: FinaleView::new(),
            choice_offered: false,
        }
    }
}

impl SeatUi {
    /// Opens the road's journal (once the story has begun).
    pub(super) fn open_galaxy_journal(&mut self, world: &WorldState, cues: &mut Vec<SoundCue>) {
        if !has_galaxy_page(world) {
            return;
        }
        self.open_modal(ModalScreen::GalaxyJournal);
        cues.push(SoundCue::Ui);
    }

    /// Opens the choice by itself once, when the seat's rig reaches the gate with the Warden
    /// calm (and nothing else is open).
    pub(super) fn watch_galaxy_choice(&mut self, ctx: &SeatContext<'_>) {
        if self.modal.is_some() || self.galaxy.choice_offered {
            return;
        }
        if choice_open_for(ctx.world, ctx.player) {
            self.galaxy.choice_offered = true;
            self.open_modal(ModalScreen::GalaxyChoice);
        }
    }

    /// The road's journal.
    pub(super) fn handle_galaxy_journal(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        if input.backs_out() {
            self.modal = None;
            cues.push(SoundCue::UiBack);
            return;
        }
        let rows = galaxy_rows(ctx.world);
        self.move_selection(input, rows.len().saturating_sub(1));
        self.selected_menu_item = self.selected_menu_item.min(rows.len().saturating_sub(1));
        if input.tab_previous {
            self.open_modal(if super::archivist::can_ask(ctx.world, ctx.player) {
                ModalScreen::Archivist
            } else {
                ModalScreen::StarCharts
            });
            return;
        }
        if input.tab_next {
            self.open_modal(ModalScreen::Map);
            return;
        }
        if !input.confirm {
            return;
        }
        let command = |command: GalaxyCommand| ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::Galaxy(command),
        };
        match rows.get(self.selected_menu_item) {
            Some(GalaxyRow::SetCourse) => {
                requests.push(command(GalaxyCommand::SetCourse));
                cues.push(SoundCue::Ui);
            }
            Some(GalaxyRow::AskArchivist) => {
                requests.push(command(GalaxyCommand::AskArchivist));
                cues.push(SoundCue::Ui);
            }
            Some(GalaxyRow::Choose) => {
                self.open_modal(ModalScreen::GalaxyChoice);
                cues.push(SoundCue::Ui);
            }
            Some(GalaxyRow::Lead(index)) => {
                let open = ctx
                    .world
                    .galaxy_story()
                    .leads
                    .get(*index)
                    .is_some_and(|record| !record.done);
                if open {
                    requests.push(command(GalaxyCommand::SetLeadCourse {
                        lead: u16::try_from(*index).unwrap_or(u16::MAX),
                    }));
                    cues.push(SoundCue::Ui);
                } else {
                    cues.push(SoundCue::UiMove);
                }
            }
            Some(GalaxyRow::Chapter(_) | GalaxyRow::Log(_) | GalaxyRow::Stats) => {
                cues.push(SoundCue::UiMove);
            }
            Some(GalaxyRow::TurnPage) | None => self.open_modal(ModalScreen::Map),
        }
    }

    /// The finale's choice.
    pub(super) fn handle_galaxy_choice(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        if input.backs_out() {
            self.modal = None;
            cues.push(SoundCue::UiBack);
            return;
        }
        self.move_selection(input, CHOICE_ROWS.len() - 1);
        self.selected_menu_item = self.selected_menu_item.min(CHOICE_ROWS.len() - 1);
        if !input.confirm {
            return;
        }
        self.modal = None;
        if let Some(choice) = CHOICE_ROWS.get(self.selected_menu_item).copied().flatten() {
            requests.push(ShellRequest::Action {
                seat: ctx.seat,
                command: PlayerCommand::Galaxy(GalaxyCommand::Choose(choice)),
            });
            cues.push(SoundCue::Ui);
        } else {
            "The bell and the lamp wait. Open the journal when you are ready."
                .clone_into(&mut self.message);
            cues.push(SoundCue::UiBack);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fatherload_simulation::{
        SimulationParams,
        worldgen::{GeneratorVersion, WorldSettings},
    };

    fn world() -> WorldState {
        let settings = WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(0x00C0_FFEE)
        };
        WorldState::new_game_with_settings(PlayerId::new(1), &settings, SimulationParams::default())
    }

    #[test]
    fn the_page_waits_for_the_road_then_lists_it() {
        let mut world = world();
        assert!(!has_galaxy_page(&world));
        assert_eq!(galaxy_rows(&world), vec![GalaxyRow::TurnPage]);
        world.set_galaxy_story(GalaxyStory {
            begun: true,
            anchor: Some(galaxy_story::plan::anchor_of(world.world_seed(), &[])),
            chapter: 2,
            step: 1,
            logs: [0, 1, 2, 3].into_iter().collect(),
            ..GalaxyStory::default()
        });
        let rows = galaxy_rows(&world);
        assert_eq!(rows[0], GalaxyRow::SetCourse);
        assert!(rows.contains(&GalaxyRow::AskArchivist));
        // Chapters newest first: the one under way, then the two done.
        let chapters: Vec<GalaxyRow> = rows
            .iter()
            .copied()
            .filter(|row| matches!(row, GalaxyRow::Chapter(_)))
            .collect();
        assert_eq!(
            chapters,
            vec![
                GalaxyRow::Chapter(2),
                GalaxyRow::Chapter(1),
                GalaxyRow::Chapter(0)
            ]
        );
        assert_eq!(rows.last(), Some(&GalaxyRow::TurnPage));
        // The page ticks off the goals done and fills the names in.
        let page = chapter_page(&world, 2);
        assert!(page.contains("[x]") && page.contains("[ ]"), "{page}");
        assert!(!page.contains('{'));
        let (title, line) = goal_line(&world).expect("a goal");
        assert!(title.starts_with("3. "));
        assert!(!line.contains('{'));
        assert!(stats_words(&world).contains("Chapters 2 of 10"));
    }

    #[test]
    fn every_event_has_words() {
        let mut world = world();
        world.set_galaxy_story(GalaxyStory {
            begun: true,
            anchor: Some(galaxy_story::plan::anchor_of(world.world_seed(), &[])),
            ..GalaxyStory::default()
        });
        for what in GalaxyEvent::ALL {
            let words = event_words(&world, what, 0);
            let silent = matches!(
                what,
                GalaxyEvent::LeadFound | GalaxyEvent::LeadStep | GalaxyEvent::LeadDone
            );
            assert!(
                silent || words.card.is_some() || words.message.is_some(),
                "{what:?}"
            );
        }
        assert!(log_entry(0).is_some());
        assert!(remark_words(0).is_some());
    }
}
