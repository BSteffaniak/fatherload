//! Contractor-built homes end to end through `LocalGame::frame` (WP-HOME): the HQ's land office
//! row sells a deed, use on a plot opens its site board and signs a contract, pushing up at a
//! finished home's door walks in (the world keeps ticking indoors and the touch controls switch
//! to the room), the bed and the decorating screens act through the simulation, and back
//! leaves.

use fatherload_simulation::{
    WorldState,
    economy::SurfaceZone,
    home::{Blueprint, Furniture, HomeJob, PlotSite, SiteKey, plot},
    surface::SURFACE_BUILDINGS,
    terrain::TILE_SIZE,
};

use super::{
    LocalGame, PLAYER, PlayerInput, back_input, confirm_input, down, idle, interact, playing,
    press, step,
};
use crate::{
    input::touch::TouchMode,
    shell::{
        ModalScreen,
        homes::{HomeAction, HomeScreen},
    },
};

fn town_site(world: &WorldState) -> PlotSite {
    plot::listed_site(world.world_settings(), SiteKey::Town(2)).expect("a town site")
}

/// The rig parked on `site`'s ground at its middle, and the pilot out at `column`.
fn on_plot(world: &mut WorldState, site: &PlotSite, pilot_column: Option<i32>) {
    let rig = world.player_mut(PLAYER).expect("rig");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        rig.x = (site.middle() as f32 + 0.5) * TILE_SIZE;
        rig.y = (site.ground_row as f32 - 0.5) * TILE_SIZE;
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.credits = 60_000;
    if let Some(column) = pilot_column {
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
        let _ = world.place_pilot(
            PLAYER,
            (column as f32 + 0.5) * TILE_SIZE,
            site.ground_row as f32 * TILE_SIZE,
        );
    }
}

fn up() -> PlayerInput {
    PlayerInput {
        thrust: true,
        ..PlayerInput::default()
    }
}

/// Moves the selection to the first row of the open home screen whose action matches.
fn select_row(game: &mut LocalGame, wanted: impl Fn(&HomeAction) -> bool) {
    idle(game, 1);
    let index = game
        .shell
        .seat(0)
        .home
        .view
        .rows
        .iter()
        .position(|row| wanted(&row.action))
        .expect("the row is listed");
    game.shell.seat_mut(0).selected_menu_item = index;
}

#[test]
fn the_land_office_sells_a_deed_from_the_hq() {
    let hq = SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == SurfaceZone::Headquarters)
        .copied()
        .expect("hq");
    let mut world = super::super::new_world();
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
        {
            rig.x = (hq.tile_x as f32 + 2.5) * TILE_SIZE;
        }
        rig.credits = 60_000;
    }
    let site = town_site(&world);
    let mut game = playing(world, 1, "home-land-office");
    idle(&mut game, 5);
    press(&mut game, interact());
    assert_eq!(
        game.shell.seat(0).interior_zone,
        Some(SurfaceZone::Headquarters)
    );
    // Walk to the counter and open the desk; the land office is its last row.
    game.shell.seat_mut(0).interior_x = crate::shell::interior_service_x(SurfaceZone::Headquarters);
    press(&mut game, interact());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Headquarters));
    for _ in 0..8 {
        press(&mut game, down());
    }
    press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Home));
    assert_eq!(game.shell.seat(0).home.screen, Some(HomeScreen::LandOffice));
    select_row(
        &mut game,
        |action| matches!(action, HomeAction::Command(fatherload_simulation::home::HomeCommand::BuyPlot { site: key, .. }) if *key == site.key),
    );
    press(&mut game, confirm_input());
    idle(&mut game, 3);
    let deeds: Vec<_> = game
        .world()
        .homes()
        .plots_of(PLAYER)
        .map(|plot| plot.site)
        .collect();
    assert_eq!(deeds, vec![site], "the deed was signed");
    // Back closes the land office.
    press(&mut game, back_input());
    assert_ne!(game.shell.seat(0).modal, Some(ModalScreen::Home));
}

#[test]
fn use_on_a_plot_opens_the_site_board_and_signs_a_contract() {
    let mut world = super::super::new_world();
    let site = town_site(&world);
    let plot = world.scenario_home(PLAYER, site, None);
    on_plot(&mut world, &site, Some(site.left + 2));
    let mut game = playing(world, 1, "home-site-board");
    idle(&mut game, 5);
    let snapshot = game.snapshot((1280, 720));
    let view = snapshot.primary_view().expect("view");
    assert!(view.plot_board.is_some(), "the site board prompt shows");
    drop(snapshot);
    press(&mut game, interact());
    assert_eq!(game.shell.seat(0).home.screen, Some(HomeScreen::Site));
    select_row(&mut game, |action| {
        matches!(
            action,
            HomeAction::Open(HomeScreen::Contract(HomeJob::Build(Blueprint::Cabin)))
        )
    });
    press(&mut game, confirm_input());
    assert_eq!(
        game.shell.seat(0).home.screen,
        Some(HomeScreen::Contract(HomeJob::Build(Blueprint::Cabin)))
    );
    select_row(&mut game, |action| matches!(action, HomeAction::Sign));
    press(&mut game, confirm_input());
    idle(&mut game, 3);
    let found = game.world().homes().plot(plot).expect("plot");
    assert!(found.construction.is_some(), "the contract was signed");
    assert_eq!(
        game.shell.seat(0).home.screen,
        Some(HomeScreen::Site),
        "back on the board"
    );
}

#[test]
fn pushing_up_at_the_door_walks_into_the_home_and_back_leaves() {
    let mut world = super::super::new_world();
    let site = town_site(&world);
    let plot = world.scenario_home(PLAYER, site, Some(Blueprint::Cabin));
    let door = world.homes().plot(plot).expect("plot").door_column();
    on_plot(&mut world, &site, Some(door));
    let mut game = playing(world, 1, "home-walk-in");
    idle(&mut game, 5);
    let tick = game.world().simulation_tick();
    let _report = step(&mut game, up());
    let inside = game.shell.seat(0).home.interior.expect("walked in");
    assert_eq!(inside.plot, plot);
    assert!(
        !game.shell.gameplay_input_enabled(0),
        "the room has the controls"
    );
    assert_eq!(
        crate::touch_screen::touch_context(&game.shell, None, 0).mode,
        TouchMode::Interior
    );
    // The world keeps ticking indoors (the clock in the windows, the bed at night).
    idle(&mut game, 30);
    assert!(game.world().simulation_tick().get() > tick.get() + 20);
    // Walking the room moves the pilot's room position, not its body outside.
    let x = game.shell.seat(0).home.interior.expect("inside").x;
    for _ in 0..20 {
        let _report = step(
            &mut game,
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
        );
    }
    assert!(game.shell.seat(0).home.interior.expect("inside").x > x);
    press(&mut game, back_input());
    assert!(game.shell.seat(0).home.interior.is_none(), "left");
}

#[test]
fn decorating_places_a_piece_through_the_simulation() {
    let mut world = super::super::new_world();
    let site = town_site(&world);
    let plot = world.scenario_home(PLAYER, site, Some(Blueprint::Cabin));
    world.grant_furniture(PLAYER, Furniture::Chair, 1);
    let door = world.homes().plot(plot).expect("plot").door_column();
    on_plot(&mut world, &site, Some(door));
    let mut game = playing(world, 1, "home-decorate");
    idle(&mut game, 5);
    let _report = step(&mut game, up());
    assert!(game.shell.seat(0).home.interior.is_some());
    game.shell
        .seat_mut(0)
        .open_home_screen(HomeScreen::Decor, Some(plot));
    select_row(&mut game, |action| {
        matches!(action, HomeAction::Open(HomeScreen::DecorPick))
    });
    press(&mut game, confirm_input());
    select_row(&mut game, |action| {
        matches!(
            action,
            HomeAction::Open(HomeScreen::DecorSpot(Furniture::Chair))
        )
    });
    press(&mut game, confirm_input());
    select_row(&mut game, |action| matches!(action, HomeAction::Command(_)));
    press(&mut game, confirm_input());
    idle(&mut game, 3);
    let room = &game
        .world()
        .homes()
        .plot(plot)
        .expect("plot")
        .home
        .as_ref()
        .expect("home")
        .rooms[0];
    assert!(
        room.decor
            .pieces
            .values()
            .any(|piece| piece.furniture == Furniture::Chair),
        "the chair stands in the room"
    );
    assert_eq!(
        game.world()
            .homes()
            .furniture_count(PLAYER, Furniture::Chair),
        0
    );
}
