//! Tomb runs end to end through `LocalGame::frame` (WP-TOMBS): the use button at an open door
//! takes the pilot in, the relic pick opens by itself between floors and takes a pick from the
//! keyboard (and from a tapped row), the run summary pops up when the run ends, the codex sits
//! after the milestones in the map's page cycle, and resting at a shrine saves the run.

use fatherload_simulation::{
    WorldState,
    terrain::TilePosition,
    tomb::{RunOutcome, RunPhase, SiteStyle, TombSite},
    worldgen::{features::TileRect, tomb_sites},
};

use super::{
    LocalGame, PLAYER, PlayerInput, confirm_input, down, idle, interact, playing, press,
    press_seat, step_seats,
};
use crate::shell::ModalScreen;

fn site(world: &WorldState, style: SiteStyle) -> TombSite {
    tomb_sites::sites_touching(
        world.world_settings(),
        &TileRect {
            min: TilePosition { x: -6_000, y: -100 },
            max: TilePosition { x: 6_000, y: 900 },
        },
    )
    .into_iter()
    .find(|site| site.style == style)
    .expect("a site of the style")
}

/// A local game whose pilot stands at the (unsealed) door of the first side-view site.
fn at_the_door() -> (LocalGame, TombSite) {
    let mut world = WorldState::new_game(PLAYER);
    let site = site(&world, SiteStyle::SideView);
    let _ = world.unseal_tomb_site(
        &site,
        None,
        &mut fatherload_simulation::TickOutput::default(),
    );
    let (x, feet) = WorldState::outside_spot(&site, 0);
    let _ = world.place_pilot(PLAYER, x, feet);
    (playing(world, 1, "tombs"), site)
}

#[test]
fn the_use_button_at_an_open_door_takes_the_pilot_into_the_run() {
    let (mut game, site) = at_the_door();
    idle(&mut game, 2);
    let _ = press(&mut game, interact());
    idle(&mut game, 2);
    let run = game.world().tomb_run_of(PLAYER).expect("in the run");
    assert_eq!(run.site.id, site.id);
    assert!(
        game.shell.seat(0).message.contains("floors"),
        "{}",
        game.shell.seat(0).message
    );
}

#[test]
fn the_relic_pick_opens_between_floors_and_takes_a_keyboard_pick() {
    let (mut game, site) = at_the_door();
    game.edit_world(|world| {
        let _ = world.scenario_enter_tomb(&site, &[PLAYER]);
        world.scenario_tomb_stairs(PLAYER);
    });
    idle(&mut game, 2);
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::RelicPick));
    let offered = game
        .world()
        .tombs()
        .member_of(PLAYER)
        .map(|member| member.offers.clone())
        .expect("offers");
    let _ = press(&mut game, down());
    let _ = press(&mut game, confirm_input());
    idle(&mut game, 3);
    let run = game.world().tomb_run_of(PLAYER).expect("still in the run");
    assert_eq!(run.floor, 1, "the only member picked: the next floor");
    assert_eq!(run.phase, RunPhase::Exploring);
    assert_eq!(
        run.member(PLAYER).map(|member| member.relics.clone()),
        Some(vec![offered[1]])
    );
    assert_ne!(game.shell.seat(0).modal, Some(ModalScreen::RelicPick));
}

#[test]
fn a_sent_pick_never_reopens_the_pick_while_it_travels() {
    let (mut game, site) = at_the_door();
    game.edit_world(|world| {
        let _ = world.scenario_enter_tomb(&site, &[PLAYER]);
        world.scenario_tomb_stairs(PLAYER);
    });
    idle(&mut game, 2);
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::RelicPick));
    let _ = press(&mut game, confirm_input());
    for _ in 0..4 {
        assert_ne!(game.shell.seat(0).modal, Some(ModalScreen::RelicPick));
        idle(&mut game, 1);
    }
    // The next floor's pick opens again.
    game.edit_world(|world| world.scenario_tomb_stairs(PLAYER));
    idle(&mut game, 2);
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::RelicPick));
}

#[test]
fn a_split_screen_pick_in_flight_keeps_the_pick_closed() {
    let (game, site) = at_the_door();
    let world = game.world().clone();
    let mut game = playing(world, 2, "tombs-split");
    idle(&mut game, 2);
    let players: Vec<_> = game.seats().iter().map(|seat| seat.player).collect();
    assert_eq!(players.len(), 2, "{players:?}");
    let (seat, first) = (0, players[0]);
    game.edit_world(|world| {
        let _ = world.scenario_enter_tomb(&site, &players);
        world.scenario_tomb_stairs(first);
    });
    idle(&mut game, 2);
    assert_eq!(game.shell.seat(seat).modal, Some(ModalScreen::RelicPick));
    let offered = game
        .world()
        .tombs()
        .member_of(first)
        .map(|member| member.offers.clone())
        .expect("offers");
    press_seat(&mut game, seat, confirm_input());
    // The partner has not picked, so the run still waits. Put the member back to unpicked as
    // a client sees it while its pick is still on the wire: the pick must stay closed.
    game.edit_world(|world| {
        world.scenario_edit_tomb_member(first, |member| {
            member.picked = false;
            member.relics.clear();
            member.offers.clone_from(&offered);
        });
    });
    for _ in 0..3 {
        idle(&mut game, 1);
        assert_ne!(game.shell.seat(seat).modal, Some(ModalScreen::RelicPick));
    }
}

#[test]
fn a_tapped_row_picks_too_and_the_last_row_takes_nothing() {
    let (mut game, site) = at_the_door();
    game.edit_world(|world| {
        let _ = world.scenario_enter_tomb(&site, &[PLAYER]);
        world.scenario_tomb_stairs(PLAYER);
    });
    idle(&mut game, 2);
    let rows = game
        .world()
        .tombs()
        .member_of(PLAYER)
        .map_or(0, |member| member.offers.len());
    let _ = press(
        &mut game,
        PlayerInput {
            menu_pick: Some(rows),
            ..PlayerInput::default()
        },
    );
    idle(&mut game, 3);
    let run = game.world().tomb_run_of(PLAYER).expect("still in the run");
    assert_eq!(run.floor, 1);
    assert!(
        run.member(PLAYER)
            .is_some_and(|member| member.relics.is_empty())
    );
}

#[test]
fn the_summary_pops_up_when_the_run_ends_and_closes_with_confirm() {
    let (mut game, site) = at_the_door();
    game.edit_world(|world| {
        let _ = world.scenario_enter_tomb(&site, &[PLAYER]);
    });
    idle(&mut game, 2);
    game.edit_world(|world| {
        if let Some(id) = world.tomb_run_of(PLAYER).map(|run| run.id) {
            world.scenario_end_tomb_run(id, RunOutcome::Wiped);
        }
    });
    idle(&mut game, 2);
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::TombSummary));
    let _ = press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, None);
}

#[test]
fn the_codex_follows_the_milestones_in_the_map_cycle() {
    let (mut game, _) = at_the_door();
    idle(&mut game, 1);
    game.shell.seat_mut(0).modal = Some(ModalScreen::Achievements);
    let _ = press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::TombCodex));
    let _ = press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Map));
}

/// Split screen with mixed view modes (WP-TOMBS T29): seat one's pilot crawls a crypt from
/// above (eight-way, no gravity, its view zoomed in) while seat two flies its rig in the side
/// view outside; each seat's input drives its own mode.
#[test]
fn split_screen_seats_play_a_crypt_from_above_and_the_world_from_the_side() {
    let mut world = WorldState::new_game(PLAYER);
    let crypt = site(&world, SiteStyle::TopDown);
    let _ = world.place_pilot(PLAYER, 0.0, 0.0);
    let mut game = playing(world, 2, "crypt-split");
    idle(&mut game, 2);
    let players: Vec<_> = game.seats().iter().map(|seat| seat.player).collect();
    let (first, second) = (players[0], players[1]);
    game.edit_world(|world| {
        let _ = world.scenario_enter_tomb(&crypt, &[first]);
        world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Peaceful);
    });
    idle(&mut game, 2);
    assert!(
        game.world().tomb_run_of(second).is_none(),
        "seat two stayed outside"
    );
    let feet = |game: &LocalGame, player| {
        game.world()
            .pilot_of(player)
            .map(|(_, body, state)| (body.x, body.y, state.view))
    };
    let (_, before, view) = feet(&game, first).expect("seat one on foot");
    assert_eq!(view, fatherload_simulation::pilot::PilotView::TopDown);
    let rig_before = game.world().player(second).map(|rig| rig.y).expect("rig");
    // Seat one holds up (north, seen from above); seat two holds thrust (up, in the sky).
    let up = PlayerInput {
        thrust: true,
        ..PlayerInput::default()
    };
    for _ in 0..20 {
        let _ = step_seats(&mut game, &[up, up]);
    }
    let (_, after, _) = feet(&game, first).expect("seat one on foot");
    assert!(
        after < before - 20.0,
        "seat one walked north: {before} -> {after}"
    );
    let rig_after = game.world().player(second).map(|rig| rig.y).expect("rig");
    assert!(
        rig_after < rig_before,
        "seat two's rig flew up in the side view"
    );
    // Seat one's view is the crypt's closer one; seat two's the side view's.
    let snapshot = game.snapshot((1280, 720));
    let scale = |player| {
        snapshot
            .views
            .iter()
            .find(|view| view.player_id == player)
            .map(|view| view.world_scale)
            .expect("a view")
    };
    let ratio = scale(first) / scale(second);
    assert!(
        (ratio - crate::render_snapshot::CRYPT_ZOOM).abs() < 1e-3,
        "{ratio}"
    );
}
