//! The "Ask Index" page (Wave 11 carryover): questions for the seat's archivist.
//!
//! The companion (Wave 10) only spoke unprompted. This page lists what its player may ask it
//! ([`fatherload_simulation::ancient_city::questions::questions`]): nearby ore, the crew's
//! finds and every open topic of the story's lore (the Descents reached, the Fathers and each
//! of them, the Heart, the stars, after the end). Choosing one sends
//! [`LogisticsCommand::AskArchivist`] for the seat's own player (so split-screen seats each ask
//! their own archivist, and online the question is a replicated command); the answer comes back
//! as the archivist's line, shown in the seat's speech box and under the page's list.
//!
//! It opens from the inventory ("Ask Index", while an archivist rides with the seat's player)
//! and sits in the map's page cycle after the star charts. Rows work by keys, pad, mouse and
//! touch alike: a tap selects a row and asks it; the last row turns the page to the map.

use fatherload_simulation::{
    PlayerCommand, PlayerId, WorldState,
    ancient_city::questions::{ArchivistQuestion, questions},
    expedition::ExpeditionState,
    logistics::LogisticsCommand,
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::PlayerInput;

/// One row of the page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchivistRow {
    /// A question.
    Ask(ArchivistQuestion),
    /// On to the map.
    TurnPage,
}

/// The page's rows for a crew with `state`.
#[must_use]
pub fn archivist_rows(state: &ExpeditionState) -> Vec<ArchivistRow> {
    let mut rows: Vec<ArchivistRow> = questions(state)
        .into_iter()
        .map(ArchivistRow::Ask)
        .collect();
    rows.push(ArchivistRow::TurnPage);
    rows
}

/// Whether `player` has an archivist to ask.
#[must_use]
pub fn can_ask(world: &WorldState, player: Option<PlayerId>) -> bool {
    player.is_some_and(|player| world.archivist_of(player).is_some())
}

impl SeatUi {
    /// Opens the page (the seat's player must have an archivist).
    pub(super) fn open_archivist(&mut self, ctx: &SeatContext<'_>, cues: &mut Vec<SoundCue>) {
        if !can_ask(ctx.world, ctx.player) {
            return;
        }
        self.open_modal(ModalScreen::Archivist);
        cues.push(SoundCue::Ui);
    }

    /// The "Ask Index" page.
    pub(super) fn handle_archivist(
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
        let rows = archivist_rows(ctx.world.expedition());
        self.move_selection(input, rows.len().saturating_sub(1));
        self.selected_menu_item = self.selected_menu_item.min(rows.len().saturating_sub(1));
        if input.tab_previous {
            self.open_modal(ModalScreen::StarCharts);
            return;
        }
        // The builders' road comes next once the galaxy story has begun (Wave 13).
        let next = if super::galaxy::has_galaxy_page(ctx.world) {
            ModalScreen::GalaxyJournal
        } else {
            ModalScreen::Map
        };
        if input.tab_next {
            self.open_modal(next);
            return;
        }
        if !input.confirm {
            return;
        }
        match rows.get(self.selected_menu_item) {
            Some(ArchivistRow::Ask(question)) => {
                if !can_ask(ctx.world, ctx.player) {
                    self.modal = None;
                    return;
                }
                requests.push(ShellRequest::Action {
                    seat: ctx.seat,
                    command: PlayerCommand::Logistics(LogisticsCommand::AskArchivist(*question)),
                });
                self.message = format!("You ask Index: {}", question.asked());
                cues.push(SoundCue::Ui);
            }
            Some(ArchivistRow::TurnPage) | None => self.open_modal(next),
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

    #[test]
    fn the_page_asks_ore_and_finds_first_then_turns_the_page() {
        let settings = WorldSettings {
            generator: GeneratorVersion::V11,
            ..WorldSettings::new(3)
        };
        let world = WorldState::new_game_with_settings(
            PlayerId::new(1),
            &settings,
            SimulationParams::default(),
        );
        let rows = archivist_rows(world.expedition());
        assert_eq!(rows[0], ArchivistRow::Ask(ArchivistQuestion::NearbyOre));
        assert_eq!(rows[1], ArchivistRow::Ask(ArchivistQuestion::Finds));
        assert_eq!(rows.last(), Some(&ArchivistRow::TurnPage));
        // No archivist rides with a new crew.
        assert!(!can_ask(&world, Some(PlayerId::new(1))));
    }
}
