//! A station's hub through `LocalGame::frame` (Wave 12): two split-screen seats docked at two
//! stations, each on its own page; a confirmed row goes through the tick (a ship line fitted, a
//! round's rumour setting the seat's waypoint), Back returns to the concourse, and abandoning a
//! mission takes a second confirm.

use fatherload_simulation::{
    flight::{FlightPlace, SpaceQuery, nav::NavTarget},
    space::SpaceSector,
    station::{
        ShipLine, ShipUpgrade, StationId, StationOwner,
        profile::{profile_of, station_ref},
    },
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::*;
use crate::shell::{
    ModalScreen,
    station::{SeatStation, StationPage, StationRow, station_rows},
};

const SEED: u64 = 0x00C0_FFEE;
const HALCYON: StationId = StationId {
    sector: fatherload_simulation::space::HOME_SECTOR,
    system: 0,
    index: 0,
};

/// The first alien-run station with a bar near home.
fn alien_station(world: &WorldState) -> StationId {
    let galaxy = world.space();
    let space = galaxy.space();
    for y in -3..=3 {
        for x in -3..=3 {
            let sector = space.sector(SpaceSector::new(x, y));
            for system in &sector.systems {
                for station in &system.stations {
                    let profile = profile_of(SEED, system, station);
                    if matches!(profile.owner, StationOwner::Alien(_)) && profile.bar {
                        return profile.id;
                    }
                }
            }
        }
    }
    panic!("no alien station near home");
}

/// Docks `player`'s rig at `station` and opens its seat's hub.
fn dock(game: &mut LocalGame, seat: usize, player: PlayerId, station: StationId) {
    game.edit_world(|world| {
        let galaxy = world.space();
        let body = galaxy
            .body(station_ref(&galaxy, station).expect("ref"))
            .expect("body");
        let park = world.space_park().expect("park");
        let rig = world.player_mut(player).expect("rig");
        rig.flight.place = FlightPlace::Docked(body.body);
        rig.flight.pos = body.port().expect("port");
        rig.flight.air = 900.0;
        rig.x = park.0;
        rig.y = park.1;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.credits = fatherload_simulation::endgame::economy::space_credits(60_000);
    });
    let ui = game.shell.seat_mut(seat);
    ui.modal = Some(ModalScreen::Station);
    ui.station = SeatStation::new();
    ui.selected_menu_item = 0;
}

/// Picks `row` on `seat`'s current page and confirms it.
fn choose(game: &mut LocalGame, seat: usize, player: PlayerId, row: StationRow) {
    let world = game.world();
    let profile = world.docked_station(player).expect("docked");
    let page = game.shell.seat(seat).station.page;
    let index = station_rows(world, player, &profile, page)
        .iter()
        .position(|found| *found == row)
        .unwrap_or_else(|| panic!("{row:?} on {page:?}"));
    game.shell.seat_mut(seat).selected_menu_item = index;
    press_seat(game, seat, confirm_input());
}

#[test]
fn two_seats_at_two_stations_use_their_own_hubs() {
    let settings = WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    };
    let world = WorldState::new_game_with_settings(
        PLAYER,
        &settings,
        fatherload_simulation::SimulationParams::new(30),
    );
    let mut game = playing(world, 2, "qa-station-hubs");
    let seats: Vec<PlayerId> = game
        .runtime
        .local_seats()
        .iter()
        .map(|seat| seat.player)
        .collect();
    let alien = alien_station(game.world());
    dock(&mut game, 0, seats[0], HALCYON);
    dock(&mut game, 1, seats[1], alien);
    idle(&mut game, 2);
    // Seat 0: the shipyard, one engine tier fitted through the tick.
    choose(
        &mut game,
        0,
        seats[0],
        StationRow::Open(StationPage::Shipyard),
    );
    assert_eq!(game.shell.seat(0).station.page, StationPage::Shipyard);
    assert_eq!(
        game.shell.seat(1).station.page,
        StationPage::Hub,
        "seat 1's own page"
    );
    choose(
        &mut game,
        0,
        seats[0],
        StationRow::Fit(ShipUpgrade::Line(ShipLine::Engine)),
    );
    idle(&mut game, 3);
    assert_eq!(game.world().ship_gear(seats[0]).engine, 1);
    assert_eq!(game.world().ship_gear(seats[1]).engine, 0);
    // Seat 1: the alien bar, a round for a patron's rumour.
    choose(&mut game, 1, seats[1], StationRow::Open(StationPage::Bar));
    let patrons = game.world().station_patrons(alien);
    let index = patrons
        .iter()
        .position(|patron| patron.rumour.is_some())
        .expect("a rumour");
    let rumour = patrons[index].rumour.clone().expect("rumour");
    choose(
        &mut game,
        1,
        seats[1],
        StationRow::Patron(u8::try_from(index).expect("index")),
    );
    idle(&mut game, 3);
    let target = game.world().space().body_ref(rumour.body).expect("ref");
    assert_eq!(
        game.world().waypoint_of(seats[1]),
        Some(NavTarget::Body(target))
    );
    // Back to the concourse, then out.
    press_seat(
        &mut game,
        1,
        PlayerInput {
            back: true,
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(1).station.page, StationPage::Hub);
    assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Station));
    // Seat 0: take a mission, then abandon it with two confirms.
    press_seat(
        &mut game,
        0,
        PlayerInput {
            back: true,
            ..PlayerInput::default()
        },
    );
    choose(&mut game, 0, seats[0], StationRow::Open(StationPage::Board));
    choose(&mut game, 0, seats[0], StationRow::Offer(0));
    idle(&mut game, 3);
    assert_eq!(game.world().station_ledger().missions_of(seats[0]).len(), 1);
    choose(&mut game, 0, seats[0], StationRow::Mission(0));
    idle(&mut game, 3);
    assert_eq!(
        game.world().station_ledger().missions_of(seats[0]).len(),
        1,
        "one confirm only arms it"
    );
    assert_eq!(game.shell.seat(0).station.abandon_armed, Some(0));
    press_seat(&mut game, 0, confirm_input());
    idle(&mut game, 3);
    assert!(
        game.world()
            .station_ledger()
            .missions_of(seats[0])
            .is_empty()
    );
    // Each seat's view draws its own station headlessly.
    let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
    assert_eq!(snapshot.views.len(), 2);
}

#[test]
fn the_works_and_the_records_go_through_the_tick() {
    use fatherload_simulation::{endgame::materials::station_stock, terrain::MineralKind};
    let settings = WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    };
    let world = WorldState::new_game_with_settings(
        PLAYER,
        &settings,
        fatherload_simulation::SimulationParams::new(30),
    );
    let mut game = playing(world, 1, "qa-station-works");
    let seat = game.runtime.local_seats()[0].player;
    dock(&mut game, 0, seat, HALCYON);
    idle(&mut game, 2);
    // Wave 13 endgame: the hub lists the Works and the Records.
    choose(&mut game, 0, seat, StationRow::Open(StationPage::Works));
    assert_eq!(game.shell.seat(0).station.page, StationPage::Works);
    let profile = game.world().docked_station(seat).expect("docked");
    let (mineral, price) = station_stock(&profile)[0];
    assert_eq!(mineral, MineralKind::MeteoricIron);
    let before = game.world().player(seat).expect("rig").credits;
    choose(&mut game, 0, seat, StationRow::Material(mineral));
    idle(&mut game, 3);
    let rig = game.world().player(seat).expect("rig");
    assert_eq!(
        rig.cargo_bay.mineral(mineral),
        u32::from(crate::shell::station::MATERIAL_LOT)
    );
    assert_eq!(
        rig.credits,
        before - price * u32::from(crate::shell::station::MATERIAL_LOT)
    );
    // Halcyon builds the gate: its parts are rows.
    let rows = station_rows(game.world(), seat, &profile, StationPage::Works);
    assert!(rows.contains(&StationRow::Order(
        fatherload_simulation::endgame::WorkOrder::GatePart(0)
    )));
    press_seat(&mut game, 0, back_input());
    assert_eq!(game.shell.seat(0).station.page, StationPage::Hub);
    // The Records: a hunt-list row sets the waypoint on a faction boss's lair.
    choose(&mut game, 0, seat, StationRow::Open(StationPage::Records));
    choose(&mut game, 0, seat, StationRow::Quarry(0));
    idle(&mut game, 3);
    let lair = game
        .world()
        .hunt()
        .first()
        .and_then(|(_, _, lair)| *lair)
        .expect("the Gilded Maw's lair");
    assert_eq!(
        game.world().waypoint_of(seat),
        Some(NavTarget::System(lair))
    );
}
