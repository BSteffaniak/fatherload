//! Villages end to end through `LocalGame::frame`: driving in raises the discovery banner, a
//! seat walks into a building, buys at its counter through the fixed tick and talks to the
//! keeper, and two split-screen seats use different village buildings at once.

use std::sync::Arc;

use fatherload_simulation::{
    terrain::{MineralKind, TILE_SIZE},
    village::{self, BuildingKind, Village, placement},
};

use super::{
    LocalGame, PlayerId, PlayerInput, WorldState, back_input, confirm_input, idle, interact,
    new_world, player, playing, press_seat,
};
use crate::shell::{ModalScreen, VILLAGE_COUNTER_X};

const FIRST: PlayerId = PlayerId::new(1);
const SECOND: PlayerId = PlayerId::new(2);

/// The first surface village (site 1 outward) with a building of `kind`, and that building.
fn village_with(world: &WorldState, kind: BuildingKind) -> (Arc<Village>, usize) {
    let settings = *world.world_settings();
    for site in (1..40).flat_map(|site| [site, -site]) {
        let Some(found) = village::village(&settings, placement::surface_id(site)) else {
            continue;
        };
        if let Some(index) = found.buildings.iter().position(|lot| lot.kind == kind) {
            return (found, index);
        }
    }
    panic!("no {kind:?} within forty sites");
}

/// Parks `player`'s rig on the ground in front of building `building` of `found`.
fn park_at(world: &mut WorldState, player: PlayerId, found: &Village, building: usize) {
    if world.player(player).is_none() {
        world.add_player(player);
    }
    let lot = &found.buildings[building];
    let rig = world.player_mut(player).expect("rig");
    #[allow(clippy::cast_precision_loss, reason = "tile indices")]
    {
        rig.x = (lot.first_column as f32 + 2.5) * TILE_SIZE;
        rig.y = (found.ground_row as f32 - 0.5) * TILE_SIZE;
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

/// Walks seat `seat` (inside a village building) to the counter.
fn walk_to_counter(game: &mut LocalGame, seat: usize) {
    for _ in 0..600 {
        let x = game
            .shell
            .seat(seat)
            .village
            .interior
            .map(|_| game.shell.seat(seat).interior_x);
        let Some(x) = x else {
            panic!("seat {seat} is not inside");
        };
        if (x - VILLAGE_COUNTER_X).abs() < 20.0 {
            return;
        }
        let mut inputs = vec![PlayerInput::default(); game.seats().len()];
        inputs[seat] = PlayerInput {
            horizontal: (VILLAGE_COUNTER_X - x).signum(),
            ..PlayerInput::default()
        };
        let _report = super::step_seats(game, &inputs);
    }
    panic!("never reached the counter");
}

#[test]
fn driving_in_raises_the_discovery_banner_once() {
    let mut world = new_world();
    let (found, building) = village_with(&world, BuildingKind::Market);
    park_at(&mut world, FIRST, &found, building);
    let mut game = playing(world, 1, "village-banner");
    idle(&mut game, 3);
    let banner = game.shell.seat(0).village.banner.clone().expect("banner");
    assert_eq!(banner.village, found.id);
    assert!(banner.first_visit, "the first visit is a discovery");
    assert_eq!(banner.title, found.name);
    assert!(player(&game, FIRST).villages.visited.contains(&found.id));
}

#[test]
fn a_seat_sells_at_a_trading_post_and_talks_to_the_keeper() {
    let mut world = new_world();
    let (found, market) = village_with(&world, BuildingKind::Market);
    park_at(&mut world, FIRST, &found, market);
    {
        let rig = world.player_mut(FIRST).expect("rig");
        rig.cargo_bay.add(MineralKind::Silver, 4);
        rig.credits = 10;
    }
    let mut game = playing(world, 1, "village-trade");
    idle(&mut game, 3);
    press_seat(&mut game, 0, interact());
    let interior = game.shell.seat(0).village.interior.expect("inside");
    assert_eq!(interior.village, found.id);
    assert_eq!(usize::from(interior.building), market);
    walk_to_counter(&mut game, 0);
    press_seat(&mut game, 0, interact());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::VillageCounter));
    // Row 0 sells the cargo.
    press_seat(&mut game, 0, confirm_input());
    idle(&mut game, 4);
    let rig = player(&game, FIRST);
    assert_eq!(rig.cargo_bay.mineral(MineralKind::Silver), 0);
    assert!(rig.credits > 10, "credits {}", rig.credits);
    assert!(
        game.shell.seat(0).message.contains("credits"),
        "{}",
        game.shell.seat(0).message
    );
    // The last row talks to the keeper.
    let rows = found.offers(market).len();
    for _ in 0..rows {
        press_seat(
            &mut game,
            0,
            PlayerInput {
                menu_down: true,
                ..PlayerInput::default()
            },
        );
    }
    press_seat(&mut game, 0, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::VillageTalk));
    let talk = game.shell.seat(0).village.talk.clone().expect("talk");
    assert!(!talk.lines.is_empty());
    // Back returns to the counter, again closes it, again leaves the building.
    press_seat(&mut game, 0, back_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::VillageCounter));
    press_seat(&mut game, 0, back_input());
    assert_eq!(game.shell.seat(0).modal, None);
    press_seat(&mut game, 0, back_input());
    assert!(!game.shell.seat(0).inside());
}

#[test]
fn split_screen_seats_use_different_village_buildings_at_once() {
    let mut world = new_world();
    let (found, market) = village_with(&world, BuildingKind::Market);
    let hall = found
        .buildings
        .iter()
        .position(|lot| lot.kind == BuildingKind::Hall)
        .expect("hall");
    park_at(&mut world, FIRST, &found, market);
    park_at(&mut world, SECOND, &found, hall);
    let mut game = playing(world, 2, "village-split");
    idle(&mut game, 3);
    let _report = super::step_seats(&mut game, &[interact(), interact()]);
    idle(&mut game, 1);
    let first = game.shell.seat(0).village.interior.expect("seat 0 inside");
    let second = game.shell.seat(1).village.interior.expect("seat 1 inside");
    assert_eq!(usize::from(first.building), market);
    assert_eq!(usize::from(second.building), hall);
    // The world rests only while every seat is indoors, as in town.
    assert!(!game.shell.world_should_tick());
    press_seat(&mut game, 1, back_input());
    assert!(!game.shell.seat(1).inside());
    assert!(game.shell.seat(0).inside());
}
