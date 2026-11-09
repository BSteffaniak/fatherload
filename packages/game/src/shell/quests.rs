//! Quests in the shell: the quest desk at each keeper's counter (a second tab next to the
//! counter's services), the per-seat quest log, the tracked quest and completion fanfares.
//!
//! The desk lists, in order: finished jobs to turn in, jobs on offer, jobs in progress (drop
//! them with a second confirm) and the next locked ones with what they need. A quest with co-op
//! objectives offered while other rigs are playing gets two rows: take it as a crew quest, or
//! take its solo variant; alone, only the solo variant is offered.

use fatherload_simulation::{
    ActiveQuest, PlayerId, SessionServiceRequest, WorldState,
    economy::SurfaceZone,
    quest::{QuestGiver, QuestId},
    world::{QuestAvailability, QuestOffer},
};

use super::{ModalScreen, SeatContext, SeatUi, SoundCue};
use crate::input::PlayerInput;

/// How long a fanfare banner stays up, in shell ticks (60 per second).
pub const FANFARE_TICKS: u64 = 270;
/// Locked quests a desk previews.
const LOCKED_PREVIEW: usize = 3;

/// What a fanfare banner celebrates (or mourns).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FanfareKind {
    /// A quest was turned in and paid.
    Complete,
    /// Every objective is done: head back to the giver.
    Ready,
    Failed,
    /// A blueprint was finished from its fragments.
    Blueprint,
}

/// A banner across the top of a seat's view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fanfare {
    pub kind: FanfareKind,
    pub title: String,
    pub detail: String,
    /// `ShellState::update_ticks` when it went up.
    pub tick: u64,
}

impl Fanfare {
    /// Whether the banner is still showing at `now`.
    #[must_use]
    pub const fn showing(&self, now: u64) -> bool {
        now.saturating_sub(self.tick) < FANFARE_TICKS
    }
}

/// One row of a quest desk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeskRow {
    /// A finished quest: confirm turns it in.
    TurnIn(QuestId),
    /// A quest on offer (or one the player cannot take yet because their log is full). A co-op
    /// quest offered while other rigs play is taken as a crew quest from this row.
    Offer(QuestOffer),
    /// The solo variant of a co-op quest, offered next to the crew row while other rigs play,
    /// so a pilot whose crew is busy elsewhere can still take it alone.
    SoloOffer(QuestOffer),
    /// A quest in progress: confirm (twice) drops it.
    Active(QuestId),
    /// A quest whose prerequisites are still missing.
    Locked(QuestOffer),
}

impl DeskRow {
    #[must_use]
    pub const fn quest(&self) -> QuestId {
        match self {
            Self::TurnIn(quest) | Self::Active(quest) => *quest,
            Self::Offer(offer) | Self::SoloOffer(offer) | Self::Locked(offer) => offer.quest,
        }
    }
}

/// The rows of `giver`'s desk for `player`.
#[must_use]
pub fn desk_rows(world: &WorldState, player: PlayerId, giver: QuestGiver) -> Vec<DeskRow> {
    let mine: Vec<&ActiveQuest> = world
        .visible_quests(player)
        .into_iter()
        .filter(|quest| quest.owner == player)
        .filter(|quest| quest.def().is_some_and(|def| def.giver == giver))
        .collect();
    let mut rows: Vec<DeskRow> = mine
        .iter()
        .filter(|quest| quest.ready)
        .map(|quest| DeskRow::TurnIn(quest.quest))
        .collect();
    let offers = world.quest_offers(player, giver);
    for offer in offers
        .iter()
        .filter(|offer| !matches!(offer.availability, QuestAvailability::Locked(_)))
    {
        rows.push(DeskRow::Offer(offer.clone()));
        if crew_mode(world, offer.quest) {
            rows.push(DeskRow::SoloOffer(offer.clone()));
        }
    }
    rows.extend(
        mine.iter()
            .filter(|quest| !quest.ready)
            .map(|quest| DeskRow::Active(quest.quest)),
    );
    rows.extend(
        offers
            .into_iter()
            .filter(|offer| matches!(offer.availability, QuestAvailability::Locked(_)))
            .take(LOCKED_PREVIEW)
            .map(DeskRow::Locked),
    );
    rows
}

/// Every quest `player`'s log shows: their solo quests, then the crew's.
#[must_use]
pub fn log_quests(world: &WorldState, player: PlayerId) -> Vec<&ActiveQuest> {
    world.visible_quests(player)
}

/// The quest the seat tracks on its HUD: its pick if still active, else the first one.
#[must_use]
pub fn tracked_quest<'w>(
    quests: &[&'w ActiveQuest],
    tracked: Option<QuestId>,
) -> Option<&'w ActiveQuest> {
    tracked
        .and_then(|wanted| quests.iter().find(|quest| quest.quest == wanted))
        .or_else(|| quests.first())
        .copied()
}

/// The counter modal a giver's building opens (the desk's other tab).
#[must_use]
pub const fn giver_counter(zone: SurfaceZone) -> Option<ModalScreen> {
    match zone {
        SurfaceZone::Fuel => Some(ModalScreen::Fuel),
        SurfaceZone::Repair => Some(ModalScreen::Repair),
        SurfaceZone::Depot => Some(ModalScreen::Depot),
        SurfaceZone::Headquarters => Some(ModalScreen::Headquarters),
        SurfaceZone::Explosives => Some(ModalScreen::Explosives),
        SurfaceZone::Salvage => Some(ModalScreen::Salvage),
        SurfaceZone::Shop | SurfaceZone::Bank => None,
    }
}

/// Whether `modal`, open inside `zone`, has a quest tab.
#[must_use]
pub fn has_quest_tab(zone: Option<SurfaceZone>, modal: ModalScreen) -> bool {
    zone.is_some_and(|zone| giver_counter(zone) == Some(modal) || modal == ModalScreen::QuestDesk)
}

/// Whether to take a co-op quest as a crew quest: only with other rigs in the world.
#[must_use]
pub fn crew_mode(world: &WorldState, quest: QuestId) -> bool {
    world.player_count() >= 2
        && quest
            .def()
            .is_some_and(fatherload_simulation::QuestDef::is_coop)
}

impl SeatUi {
    /// The desk row armed for dropping (a second confirm drops it).
    #[must_use]
    pub const fn quest_abandon_armed(&self) -> Option<QuestId> {
        self.quest_abandon_armed
    }

    /// Left/right (or LB/RB) at a keeper's counter flips between its services and its quest
    /// desk. Returns whether the input was used.
    pub(super) fn switch_quest_tab(&mut self, modal: ModalScreen, input: PlayerInput) -> bool {
        let flip = input.menu_left || input.menu_right || input.tab_previous || input.tab_next;
        let Some(zone) = self.interior_zone else {
            return false;
        };
        if !flip || !has_quest_tab(Some(zone), modal) {
            return false;
        }
        self.quest_abandon_armed = None;
        if modal == ModalScreen::QuestDesk {
            if let Some(counter) = giver_counter(zone) {
                self.modal = Some(counter);
                self.selected_menu_item = usize::from(counter == ModalScreen::Depot);
            }
        } else {
            self.modal = Some(ModalScreen::QuestDesk);
            self.selected_menu_item = 0;
        }
        true
    }

    /// Rows of the desk this seat has open.
    pub(super) fn desk_rows(&self, ctx: SeatContext<'_>) -> Vec<DeskRow> {
        let (Some(player), Some(giver)) = (ctx.player, self.interior_zone.and_then(QuestGiver::at))
        else {
            return Vec::new();
        };
        desk_rows(ctx.world, player, giver)
    }

    /// The tracked quest follows the quest log's selection.
    pub(super) fn track_log_selection(&mut self, ctx: SeatContext<'_>) {
        let Some(player) = ctx.player else {
            return;
        };
        let quests = log_quests(ctx.world, player);
        if let Some(quest) = quests.get(self.selected_menu_item) {
            self.tracked_quest = Some(quest.quest);
        }
    }

    /// Confirm on a desk row: the request to submit, if any.
    pub(super) fn confirm_quest_desk(
        &mut self,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
    ) -> Option<SessionServiceRequest> {
        let rows = self.desk_rows(ctx);
        let row = rows.get(self.selected_menu_item)?;
        let title = row.quest().def().map_or("that job", |def| def.title);
        match row {
            DeskRow::TurnIn(quest) => Some(SessionServiceRequest::TurnInQuest { quest: *quest }),
            DeskRow::Offer(offer) | DeskRow::SoloOffer(offer) => {
                if offer.availability == QuestAvailability::Available {
                    Some(SessionServiceRequest::AcceptQuest {
                        quest: offer.quest,
                        crew: matches!(row, DeskRow::Offer(_)) && crew_mode(ctx.world, offer.quest),
                    })
                } else {
                    "Your quest log is full: finish or drop a job first."
                        .clone_into(&mut self.message);
                    cues.push(SoundCue::Ui);
                    None
                }
            }
            DeskRow::Active(quest) => {
                if self.quest_abandon_armed == Some(*quest) {
                    self.quest_abandon_armed = None;
                    Some(SessionServiceRequest::AbandonQuest { quest: *quest })
                } else {
                    self.quest_abandon_armed = Some(*quest);
                    self.message = format!("Confirm again to drop \"{title}\".");
                    cues.push(SoundCue::Ui);
                    None
                }
            }
            DeskRow::Locked(offer) => {
                let needs = match &offer.availability {
                    QuestAvailability::Locked(missing) => missing
                        .iter()
                        .map(|prerequisite| prerequisite.describe(ctx.world.terrain().settings()))
                        .collect::<Vec<_>>()
                        .join(", "),
                    _ => String::new(),
                };
                self.message = format!("\"{title}\" needs: {needs}.");
                cues.push(SoundCue::Ui);
                None
            }
        }
    }

    /// The message for a resolved quest request.
    pub(super) fn quest_resolved(
        &mut self,
        request: SessionServiceRequest,
        ctx: SeatContext<'_>,
    ) -> Option<(String, SoundCue)> {
        let (quest, verb) = match request {
            SessionServiceRequest::AcceptQuest { quest, .. } => (quest, 0),
            SessionServiceRequest::TurnInQuest { quest } => (quest, 1),
            SessionServiceRequest::AbandonQuest { quest } => (quest, 2),
            _ => return None,
        };
        let def = quest.def()?;
        self.selected_menu_item = 0;
        Some(match verb {
            0 => {
                self.tracked_quest = Some(quest);
                let crew = ctx
                    .world
                    .crew_quests()
                    .active
                    .iter()
                    .any(|active| active.quest == quest && Some(active.owner) == ctx.player);
                (
                    format!(
                        "{}: \"{}\" is yours{}. It's tracked on your HUD.",
                        def.giver.name(),
                        def.title,
                        if crew { ", shared with the crew" } else { "" }
                    ),
                    SoundCue::Ui,
                )
            }
            1 => (
                format!("{}: {}", def.giver.name(), def.text.complete),
                SoundCue::Milestone,
            ),
            _ => (format!("Dropped \"{}\".", def.title), SoundCue::Ui),
        })
    }
}

/// Why a quest request was refused.
#[must_use]
pub const fn quest_unavailable_message(request: SessionServiceRequest) -> Option<&'static str> {
    match request {
        SessionServiceRequest::AcceptQuest { .. } => Some(
            "That job can't be taken right now: it's locked, or this world has no site for it.",
        ),
        SessionServiceRequest::TurnInQuest { .. } => {
            Some("Not finished yet: bring everything the job asks for.")
        }
        SessionServiceRequest::AbandonQuest { .. } => {
            Some("Jobs can only be dropped at their giver's counter.")
        }
        _ => None,
    }
}
