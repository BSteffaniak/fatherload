//! Space flight's dialogs (Wave 11): the nav chart (the map button in space) and a docked
//! station's counter.
//!
//! Both are lists of rows, so they work by keys, pad, mouse and touch alike (a tap selects a
//! row and confirms it). Every change goes to the simulation as a
//! [`PlayerCommand::Flight`] for the seat's player, so it works the same in split-screen and
//! online.
//!
//! * **The nav chart** lists what the seat can set a waypoint on: with a nav suite fitted,
//!   every star system the crew knows (home, the Archive's charts, systems found by flying and
//!   scanning) and the bodies of the system the rig is in; without one, only the way home.
//!   Then the warp, sharing the waypoint with the crew, clearing it, the station's counter
//!   while docked, and the emergency beacon.
//! * **The station**'s hub is `shell/station.rs` (Wave 12).

use fatherload_simulation::{
    Player, PlayerCommand, PlayerId, WorldState,
    flight::{
        BodyKind, BodyRef, FlightCommand, FlightPlace, SpaceQuery, SystemRef,
        nav::NavTarget,
        space::{self, galaxy_sector_of},
    },
    star_chart::StarChart,
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::PlayerInput;

/// One row of the nav chart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavRow {
    /// Set the waypoint on this.
    Target(NavTarget),
    /// Spool a warp to the waypoint's system.
    Warp,
    /// Share the waypoint with the crew.
    Share,
    /// Clear the waypoint.
    Clear,
    /// The docked station's counter.
    Station,
    /// The emergency beacon (towed home for a fee).
    Beacon,
    Close,
}

/// The nav chart's rows for `player` (see the module docs).
#[must_use]
pub fn nav_rows(world: &WorldState, player: PlayerId) -> Vec<NavRow> {
    let Some(rig) = world.player(player) else {
        return vec![NavRow::Close];
    };
    nav_rows_for(
        rig,
        &world.known_systems(),
        world.waypoint_of(player),
        &world.space(),
    )
}

/// The nav chart's rows for `rig` (shared by the shell and the renderer).
#[must_use]
pub fn nav_rows_for(
    rig: &Player,
    known: &[SystemRef],
    waypoint: Option<NavTarget>,
    space: &dyn SpaceQuery,
) -> Vec<NavRow> {
    let mut rows = Vec::new();
    let flight = rig.flight;
    let nav = rig.hazard_gear.flight.nav_suite > 0;
    if nav {
        rows.extend(
            known
                .iter()
                .map(|system| NavRow::Target(NavTarget::System(*system))),
        );
        // The bodies of the system the rig is in (its galactic sector's).
        if let Some(system) = space.system_at(galaxy_sector_of(flight.pos)) {
            for body in space.bodies_of(&system) {
                if body.kind != BodyKind::Star {
                    rows.push(NavRow::Target(NavTarget::Body(body.body)));
                }
            }
        }
    } else {
        rows.push(NavRow::Target(NavTarget::Body(space::HOME_BODY)));
        rows.push(NavRow::Target(NavTarget::Body(BodyRef {
            system: space::HOME_SYSTEM,
            index: space::STATION_INDEX,
        })));
    }
    let elsewhere = |system: SystemRef| galaxy_sector_of(flight.pos) != system.sector;
    if nav && matches!(waypoint, Some(NavTarget::System(system)) if elsewhere(system)) {
        rows.push(NavRow::Warp);
    }
    if waypoint.is_some() {
        rows.push(NavRow::Share);
        rows.push(NavRow::Clear);
    }
    if matches!(flight.place, FlightPlace::Docked(_)) {
        rows.push(NavRow::Station);
    }
    if flight.away() {
        rows.push(NavRow::Beacon);
    }
    rows.push(NavRow::Close);
    rows
}

/// The name of a nav target (a system's builders' name, a body's).
#[must_use]
pub fn target_name(charts: &[StarChart], target: NavTarget) -> String {
    match target {
        NavTarget::System(system) => system_name(charts, system),
        NavTarget::Body(body) => {
            if body == space::HOME_BODY {
                return "Home".to_owned();
            }
            let system = system_name(charts, body.system);
            if body.index == space::STATION_INDEX {
                if body.system.is_home() {
                    "Halcyon Dock".to_owned()
                } else {
                    format!("{system} Station")
                }
            } else if body.index == 0 {
                system
            } else {
                format!("{system} {}", roman(body.index))
            }
        }
    }
}

/// A system's name: home, the Archive's name for a charted one, else a catalogue name.
#[must_use]
pub fn system_name(charts: &[StarChart], system: SystemRef) -> String {
    if system.is_home() {
        return "Home system".to_owned();
    }
    charts
        .iter()
        .find(|chart| chart.system == system.id)
        .map_or_else(
            || fatherload_simulation::star_chart::star_name(system.id),
            |chart| chart.name.clone(),
        )
}

/// `index` in Roman numerals (worlds are numbered from their star).
#[must_use]
pub fn roman(index: u8) -> &'static str {
    const NUMERALS: [&str; 11] = [
        "", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X",
    ];
    NUMERALS.get(usize::from(index)).copied().unwrap_or("X+")
}

impl SeatUi {
    /// The nav chart.
    pub(super) fn handle_nav_chart(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        if input.backs_out() || input.map {
            self.modal = None;
            cues.push(SoundCue::UiBack);
            return;
        }
        let Some(player) = ctx.player else {
            self.modal = None;
            return;
        };
        let rows = nav_rows(ctx.world, player);
        self.move_selection(input, rows.len().saturating_sub(1));
        self.selected_menu_item = self.selected_menu_item.min(rows.len().saturating_sub(1));
        if !input.confirm {
            return;
        }
        let command = match rows.get(self.selected_menu_item) {
            Some(NavRow::Target(target)) => FlightCommand::SetWaypoint(Some(*target)),
            Some(NavRow::Warp) => {
                self.modal = None;
                FlightCommand::Warp
            }
            Some(NavRow::Share) => FlightCommand::ShareWaypoint,
            Some(NavRow::Clear) => FlightCommand::SetWaypoint(None),
            Some(NavRow::Station) => {
                self.open_modal(ModalScreen::Station);
                self.station = super::station::SeatStation::new();
                cues.push(SoundCue::Ui);
                return;
            }
            Some(NavRow::Beacon) => {
                self.modal = None;
                FlightCommand::Beacon
            }
            Some(NavRow::Close) | None => {
                self.modal = None;
                cues.push(SoundCue::UiBack);
                return;
            }
        };
        cues.push(SoundCue::Ui);
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::Flight(command),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worlds_are_numbered_in_roman() {
        assert_eq!(roman(3), "III");
    }
}
