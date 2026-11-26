//! Tomb and crypt runs at the seat (WP-TOMBS, T27).
//!
//! Entering, shrines, caches, the stairs and the way out are the use button on foot (the
//! simulation's tomb `Usable`s, so keyboard, gamepad and the touch USE button all work them);
//! this module adds the words for those presses and three seat dialogs:
//!
//! * the **relic pick** ([`ModalScreen::RelicPick`]): opens by itself when the seat's member
//!   has offers between floors; one row per offer plus "Take nothing", sent as
//!   [`TombCommand::PickRelic`] (rows are tapped like every menu's);
//! * the **run summary** ([`ModalScreen::TombSummary`]): opens when a new summary for the
//!   seat's player arrives (cleared, made it out, party lost);
//! * the **tomb codex** ([`ModalScreen::TombCodex`]): relics found, tomb maps, trophies and lore
//!   pages, the page after the milestones in the map's page cycle.

use fatherload_simulation::{
    PlayerCommand,
    mechanism::Usable,
    tomb::{RunPhase, SiteOpening, TOMB_KEY, TombCommand},
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::PlayerInput;

/// A seat's tomb bookkeeping.
#[derive(Clone, Copy, Debug, Default)]
pub struct SeatTomb {
    /// The serial of the last run summary this seat showed (or saw at start-up).
    pub summary_seen: Option<u32>,
    /// The seat has looked at the summaries once (old ones never pop up).
    pub primed: bool,
    /// The (run, floor) whose relic pick this seat has sent and not yet seen applied: the
    /// pick does not reopen while the command travels (a tick locally, a round trip online).
    pub pick_sent: Option<(u32, u8)>,
}

impl SeatTomb {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            summary_seen: None,
            primed: false,
            pick_sent: None,
        }
    }
}

impl SeatUi {
    /// Opens the relic pick while the seat's member has offers, and the summary when a new
    /// one arrives; closes the pick once it is no longer wanted.
    pub(super) fn watch_tomb(&mut self, ctx: SeatContext<'_>, cues: &mut Vec<SoundCue>) {
        let Some(player) = ctx.player else {
            return;
        };
        let tombs = ctx.world.tombs();
        let summary = tombs.summaries.get(&player).map(|summary| summary.serial);
        if !self.tomb.primed {
            self.tomb.primed = true;
            self.tomb.summary_seen = summary;
        }
        let pick_sent = self.tomb.pick_sent;
        let picking = tombs.run_of(player).is_some_and(|run| {
            run.phase == RunPhase::Picking
                && pick_sent != Some((run.id, run.floor))
                && run
                    .member(player)
                    .is_some_and(|member| !member.picked && !member.offers.is_empty())
        });
        if picking && self.modal.is_none() {
            self.modal = Some(ModalScreen::RelicPick);
            self.selected_menu_item = 0;
            // No toast: the pick's own title says it, and a toast would cover its footer.
            cues.push(SoundCue::Ui);
        } else if !picking && self.modal == Some(ModalScreen::RelicPick) {
            self.modal = None;
        }
        if summary.is_some() && summary != self.tomb.summary_seen && !picking {
            self.tomb.summary_seen = summary;
            if self.modal.is_none() || self.modal == Some(ModalScreen::RelicPick) {
                self.modal = Some(ModalScreen::TombSummary);
                self.selected_menu_item = 0;
            }
        }
    }

    /// One frame of the relic pick.
    pub(super) fn handle_relic_pick(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let Some(player) = ctx.player else {
            self.modal = None;
            return;
        };
        let offers = ctx
            .world
            .tombs()
            .member_of(player)
            .map_or_default(|member| member.offers.clone());
        if offers.is_empty() {
            self.modal = None;
            return;
        }
        let last = offers.len();
        self.move_selection(input, last);
        if !input.confirm && !input.interact {
            return;
        }
        let choice = (self.selected_menu_item < last)
            .then(|| u8::try_from(self.selected_menu_item).ok())
            .flatten();
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::Tomb(TombCommand::PickRelic(choice)),
        });
        self.tomb.pick_sent = ctx
            .world
            .tombs()
            .run_of(player)
            .map(|run| (run.id, run.floor));
        self.message = choice
            .and_then(|index| offers.get(usize::from(index)))
            .map_or_else(
                || "You leave the relics where they lie.".to_owned(),
                |relic| {
                    let style = ctx
                        .world
                        .tombs()
                        .run_of(player)
                        .map_or(fatherload_simulation::tomb::SiteStyle::SideView, |run| {
                            run.site.style
                        });
                    format!("{}: {}", relic.name(), relic.blurb_in(style))
                },
            );
        cues.push(SoundCue::Ui);
        self.modal = None;
    }

    /// One frame of the run summary: any confirm or back closes it.
    pub(super) fn handle_tomb_summary(&mut self, input: PlayerInput, cues: &mut Vec<SoundCue>) {
        if input.confirm || input.interact || input.backs_out() {
            self.modal = None;
            cues.push(SoundCue::UiBack);
        }
    }

    /// One frame of the tomb codex page: confirm turns to the map, back closes.
    pub(super) fn handle_tomb_codex(
        &mut self,
        input: PlayerInput,
        world: &fatherload_simulation::WorldState,
        cues: &mut Vec<SoundCue>,
    ) {
        if input.backs_out() {
            self.modal = None;
            cues.push(SoundCue::UiBack);
            return;
        }
        if input.tab_previous {
            self.open_modal(ModalScreen::Achievements);
            return;
        }
        if input.confirm || input.tab_next {
            // The expedition journal (Wave 8 story) follows on Deep Descent worlds.
            self.open_modal(if super::expedition::has_expedition_page(world) {
                ModalScreen::ExpeditionLog
            } else {
                ModalScreen::Map
            });
        }
    }
}

/// What the seat hears when its use button works a tomb fixture.
pub(super) fn use_line(ctx: SeatContext<'_>, usable: Usable) -> String {
    match usable {
        Usable::TombDoor { sealed: true, .. } => {
            let Some(site) = ctx
                .player
                .and_then(|player| ctx.world.tomb_door_in_reach(player))
            else {
                return "The seal holds.".to_owned();
            };
            match site.opening {
                SiteOpening::Key => {
                    let has_key = ctx.player().is_some_and(|player| {
                        player.pilot_pack.count(TOMB_KEY) > 0 || player.carried(TOMB_KEY) > 0
                    });
                    if has_key {
                        format!("The tomb key turns. The {} is open.", site.title())
                    } else {
                        "A keyhole shaped like the builders' sigil. Tomb keys turn up in old ruin chests and tomb vaults.".to_owned()
                    }
                }
                SiteOpening::Story => {
                    let told = ctx
                        .player()
                        .is_some_and(|player| !player.quests.arcs.finished.is_empty());
                    if told {
                        "The carved tale glows under your hand and the seal parts.".to_owned()
                    } else {
                        SiteOpening::Story.hint().to_owned()
                    }
                }
                SiteOpening::Explosives => SiteOpening::Explosives.hint().to_owned(),
            }
        }
        Usable::TombDoor { .. } => ctx
            .player
            .and_then(|player| ctx.world.tomb_door_in_reach(player))
            .map_or_else(
                || "You step inside.".to_owned(),
                |site| match site.style {
                    fatherload_simulation::tomb::SiteStyle::TopDown => format!(
                        "Into the {}: {} floors seen from above. {} rolls through traps and blows, aim anywhere; the rig waits outside.",
                        site.title(),
                        site.floors(),
                        super::key_for(crate::input::prompts::Prompt::Jump, ctx.device),
                    ),
                    fatherload_simulation::tomb::SiteStyle::SideView => format!(
                        "Into the {}: {} floors. The rig waits outside.",
                        site.title(),
                        site.floors()
                    ),
                },
            ),
        Usable::TombShrine { .. } => {
            "The shrine lamp takes your loot into its keeping. The run is saved.".to_owned()
        }
        Usable::TombStairs => {
            let guarded = ctx
                .player
                .and_then(|player| ctx.world.tomb_run_of(player))
                .is_some_and(|run| run.on_last_floor() && !run.boss_defeated);
            if guarded {
                "The guardian still stands between you and the way out.".to_owned()
            } else {
                "You take the stairs.".to_owned()
            }
        }
        Usable::TombExit => "Back up into the light, loot and all.".to_owned(),
        Usable::TombCache => "You crack the cache's seal.".to_owned(),
        _ => String::new(),
    }
}
