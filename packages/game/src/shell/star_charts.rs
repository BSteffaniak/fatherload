//! The star charts page (Wave 10 ancients-a): the stars the crew has charted in the Archive of
//! Stars, in the map's page cycle after the expedition codex on Deep Descent worlds.
//!
//! Its rows are the charts read, in the order found (selecting one shows where it lies, what
//! star it is and how many worlds circle it), then "Turn the page" (on to the map), so it works
//! by keys, pad, mouse and touch alike: a tap selects a row and confirms it. The crew shares
//! one list (the buried city's shared section), so every seat and partner reads the same sky.

use fatherload_simulation::{WorldState, star_chart::StarChart};

use super::{ModalScreen, SeatUi, SoundCue};
use crate::input::PlayerInput;

/// One row of the star charts page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StarChartRow {
    /// A chart read (index into the crew's list).
    Chart(usize),
    /// On to the map.
    TurnPage,
}

/// The page's rows for `charts`.
#[must_use]
pub fn star_chart_rows(charts: &[StarChart]) -> Vec<StarChartRow> {
    let mut rows: Vec<StarChartRow> = (0..charts.len()).map(StarChartRow::Chart).collect();
    rows.push(StarChartRow::TurnPage);
    rows
}

/// What a chart says, in words (the page's footer).
#[must_use]
pub fn chart_words(chart: &StarChart) -> String {
    format!(
        "{}: a {} {} sectors out (sector {}, {}), {} world{}. Read at a plinth in the Archive \
         of Stars. One day a drive will need these numbers.",
        chart.name,
        chart.class.name(),
        chart.distance(),
        chart.sector.0,
        chart.sector.1,
        chart.worlds,
        if chart.worlds == 1 { "" } else { "s" },
    )
}

impl SeatUi {
    /// The star charts page.
    pub(super) fn handle_star_charts(
        &mut self,
        input: PlayerInput,
        world: &WorldState,
        player: Option<fatherload_simulation::PlayerId>,
        cues: &mut Vec<SoundCue>,
    ) {
        if input.backs_out() {
            self.modal = None;
            cues.push(SoundCue::UiBack);
            return;
        }
        let rows = star_chart_rows(world.star_charts());
        self.move_selection(input, rows.len().saturating_sub(1));
        self.selected_menu_item = self.selected_menu_item.min(rows.len().saturating_sub(1));
        if input.tab_previous {
            self.open_modal(ModalScreen::DescentProgress);
            return;
        }
        // The "Ask Index" page comes next while an archivist rides with the seat's player
        // (Wave 11 carryover), then the map.
        let next = if super::archivist::can_ask(world, player) {
            ModalScreen::Archivist
        } else if super::galaxy::has_galaxy_page(world) {
            // The builders' road (Wave 13 story).
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
            Some(StarChartRow::Chart(_)) => cues.push(SoundCue::UiMove),
            Some(StarChartRow::TurnPage) | None => self.open_modal(next),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fatherload_simulation::{SimulationTick, star_chart::chart_at_site};

    #[test]
    fn the_page_lists_every_chart_then_turns_the_page() {
        assert_eq!(star_chart_rows(&[]), vec![StarChartRow::TurnPage]);
        let charts = vec![
            chart_at_site(1, 10, 10_300, 200, SimulationTick::new(1)),
            chart_at_site(1, 50, 10_400, 600, SimulationTick::new(2)),
        ];
        assert_eq!(
            star_chart_rows(&charts),
            vec![
                StarChartRow::Chart(0),
                StarChartRow::Chart(1),
                StarChartRow::TurnPage
            ]
        );
        let words = chart_words(&charts[0]);
        assert!(words.contains(&charts[0].name));
        assert!(words.contains("sector"));
    }
}
