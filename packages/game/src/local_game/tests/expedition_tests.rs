//! The Fathers' expedition end to end through `LocalGame::frame` (Wave 8 story): reading a
//! canister puts the log's card up on every local seat, an echo plays its card, the journal
//! follows the tomb codex in the map's pages on a Deep Descent world (and not on a classic
//! one), and a tap on a journal row selects it while the last row turns the page.

use fatherload_simulation::{
    SimulationParams, WorldState,
    expedition::{ECHOES, ExpeditionState, sites},
    position::WorldPos,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::{LocalGame, PlayerId, PlayerInput, confirm_input, idle, playing, press};
use crate::shell::{ModalScreen, StoryMomentKind, expedition::expedition_rows};

const PLAYER: PlayerId = PlayerId::new(1);

fn deep_world() -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(0x0E4D_1710)
    };
    WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default())
}

fn put_at_log(game: &mut LocalGame, number: u8) {
    let settings = *game.world().terrain().settings();
    let site = sites::sites(&settings)[usize::from(number - 1)];
    let tile = site.canister();
    game.edit_world(|world| {
        world.teleport_rig_to(
            PLAYER,
            WorldPos::tile_center(i64::from(tile.x), i64::from(tile.y)),
        );
    });
}

#[test]
fn reading_a_canister_puts_its_log_card_up_and_the_journal_lists_it() {
    let mut world = deep_world();
    world.set_expedition(ExpeditionState {
        echoes: u8::try_from(ECHOES.len()).unwrap_or(u8::MAX),
        ..ExpeditionState::default()
    });
    let mut game = playing(world, 1, "expedition-card");
    idle(&mut game, 2);
    put_at_log(&mut game, 1);
    idle(&mut game, 2);
    assert_eq!(game.world().expedition().found, 1);
    let moment = game.shell.seat(0).story_moment.clone().expect("a card");
    assert_eq!(moment.kind, StoryMomentKind::ExpeditionLog);
    assert!(
        moment.title.contains("The Fathers' Load"),
        "{}",
        moment.title
    );
    assert!(game.shell.seat(0).message.contains("credits"));
    // The journal: tomb codex, then the expedition logs, then the map again.
    game.shell.seat_mut(0).modal = Some(ModalScreen::TombCodex);
    let _ = press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::ExpeditionLog));
    // A tap on the log's row selects it and stays on the page.
    let rows = expedition_rows(game.world().expedition());
    assert_eq!(rows.len(), 2);
    let _ = press(
        &mut game,
        PlayerInput {
            menu_pick: Some(0),
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::ExpeditionLog));
    assert_eq!(game.shell.seat(0).selected_menu_item, 0);
    // A tap on the last row turns the page: the codex (Wave 9), then the map.
    let _ = press(
        &mut game,
        PlayerInput {
            menu_pick: Some(1),
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::ExpeditionCodex));
    // Nothing found yet: only "Turn the page", which goes on to the Descent progress page
    // (Wave 10), then the star charts and, with none read, the map.
    let entries = crate::shell::expedition::codex_entries(game.world());
    assert_eq!(crate::shell::expedition::codex_rows(&entries).len(), 1);
    let _ = press(
        &mut game,
        PlayerInput {
            menu_pick: Some(0),
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::DescentProgress));
    let _ = press(
        &mut game,
        PlayerInput {
            tab_next: true,
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::StarCharts));
    let _ = press(
        &mut game,
        PlayerInput {
            menu_pick: Some(0),
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Map));
}

/// Wave 9: the codex keeps the strongbox diary and the builders' tablets the crew has read,
/// rows by touch like the journal, and the tabs page back to the journal.
#[test]
fn the_codex_lists_the_diary_and_tablets_the_crew_has_read() {
    use crate::shell::expedition::{CodexRow, codex_entries, codex_rows};
    let mut world = deep_world();
    world.set_codex_finds_for_tests(&[1], &[fatherload_simulation::old_deep::LOG_TABLET_FIRST]);
    let mut game = playing(world, 1, "expedition-codex");
    idle(&mut game, 2);
    let entries = codex_entries(game.world());
    let rows = codex_rows(&entries);
    assert_eq!(rows.len(), 3);
    let CodexRow::Entry(first) = rows[0] else {
        panic!("a diary row first: {rows:?}");
    };
    assert_eq!(
        entries[first].title,
        fatherload_simulation::crust::CRUST_LOGS[1].0
    );
    let CodexRow::Entry(second) = rows[1] else {
        panic!("a tablet row second: {rows:?}");
    };
    assert_eq!(entries[second].title, "The anchor");
    game.shell.seat_mut(0).modal = Some(ModalScreen::ExpeditionCodex);
    let _ = press(
        &mut game,
        PlayerInput {
            menu_pick: Some(1),
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::ExpeditionCodex));
    assert_eq!(game.shell.seat(0).selected_menu_item, 1);
    let _ = press(
        &mut game,
        PlayerInput {
            tab_previous: true,
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::ExpeditionLog));
}

#[test]
fn a_sealed_canister_says_so_and_an_echo_plays_its_card() {
    let mut game = playing(deep_world(), 1, "expedition-echo");
    idle(&mut game, 2);
    put_at_log(&mut game, 2);
    idle(&mut game, 2);
    assert_eq!(game.world().expedition().found, 0);
    assert!(game.shell.seat(0).message.contains("sealed"));
    assert!(game.world().expedition().sealed.contains(&2));
    // The lead names the log actually missing, however far back it is.
    assert_eq!(
        crate::shell::expedition::sealed_words(6, 2),
        "A sealed canister: log 6. Ines keyed it to the entry before. Find log 3 first."
    );
    // Somewhere below the first echo's depth (the canister above stays sealed, nothing read).
    let row = game.world().next_echo_row().expect("an echo left") + 2;
    let settings = *game.world().terrain().settings();
    let column = sites::sites(&settings)[0].bounds.min.x - 8;
    game.edit_world(|world| {
        world.teleport_rig_to(
            PLAYER,
            WorldPos::tile_center(i64::from(column), i64::from(row)),
        );
    });
    idle(&mut game, 2);
    assert_eq!(game.world().expedition().echoes, 1);
    let moment = game.shell.seat(0).story_moment.clone().expect("a card");
    assert_eq!(moment.kind, StoryMomentKind::RadioEcho);
    assert!(moment.text.contains("Dov Halloran"));
}

#[test]
fn classic_worlds_keep_the_old_page_cycle() {
    let mut game = playing(WorldState::new_game(PLAYER), 1, "expedition-classic");
    idle(&mut game, 1);
    game.shell.seat_mut(0).modal = Some(ModalScreen::TombCodex);
    let _ = press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Map));
    assert!(game.world().expedition().is_empty());
}

#[test]
fn arriving_at_the_old_deeps_outpost_its_people_come_out_to_talk() {
    // Wave 9: Cathedral Station has resident people, who tell their memories of the Fathers
    // in conversation; the Wave 8 card no longer goes up there.
    let mut world = deep_world();
    world.set_expedition(ExpeditionState {
        found: 21,
        echoes: u8::try_from(ECHOES.len()).unwrap_or(u8::MAX),
        ..ExpeditionState::default()
    });
    let outpost = world
        .logistics()
        .outposts
        .values()
        .find(|outpost| outpost.descent == 1)
        .expect("the Old Deep's outpost")
        .clone();
    let mut game = playing(world, 1, "settlement-memory");
    idle(&mut game, 2);
    game.edit_world(|world| {
        world.grant_deep_claim();
        world.teleport_rig_to(
            PLAYER,
            WorldPos::tile_center(i64::from(outpost.column), i64::from(outpost.floor_row - 1)),
        );
    });
    idle(&mut game, 30);
    assert!(
        game.shell
            .seat(0)
            .story_moment
            .as_ref()
            .is_none_or(|moment| moment.kind != StoryMomentKind::Remembered)
    );
    let station = fatherload_simulation::people::deep_folk::CATHEDRAL_STATION;
    let folk = game
        .world()
        .entities()
        .of_kind(fatherload_simulation::entity::EntityKind::Npc)
        .filter(|entity| {
            matches!(entity.state, fatherload_simulation::entity::EntityState::Npc(state)
                if state.character.village == station)
        })
        .count();
    assert_eq!(folk, 5, "the station's five residents are about");
}

/// Wave 10: a new Descent raises its card on every seat, the codex turns to the Descent
/// progress page (rows by touch, tabs back to the codex) and the map after it.
#[test]
fn a_new_descent_has_its_card_and_the_progress_page_follows_the_codex() {
    use crate::shell::expedition::{ProgressRow, progress_rows};
    let mut world = deep_world();
    world.set_expedition(ExpeditionState {
        echoes: u8::try_from(ECHOES.len()).unwrap_or(u8::MAX),
        ..ExpeditionState::default()
    });
    let mut game = playing(world, 2, "descent-progress");
    idle(&mut game, 2);
    // Somewhere in the Old Deep, clear of every canister.
    let settings = *game.world().terrain().settings();
    let site = sites::sites(&settings)[12];
    let tile = site.canister();
    game.edit_world(|world| {
        world.grant_deep_claim();
        world.teleport_rig_to(
            PLAYER,
            WorldPos::tile_center(i64::from(tile.x - 40), i64::from(tile.y)),
        );
    });
    idle(&mut game, 2);
    assert!(
        game.world()
            .expedition()
            .reached(fatherload_simulation::descent::Descent::OldDeep)
    );
    for seat in 0..2 {
        let moment = game.shell.seat(seat).story_moment.clone().expect("a card");
        assert_eq!(moment.kind, StoryMomentKind::DescentReached);
        assert!(moment.title.contains("The Old Deep"), "{}", moment.title);
    }
    // The page cycle: codex, then progress, then the map.
    game.shell.seat_mut(0).modal = Some(ModalScreen::ExpeditionCodex);
    let _ = press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::DescentProgress));
    let rows = progress_rows(game.world().expedition());
    assert_eq!(rows.len(), 9);
    assert_eq!(rows[8], ProgressRow::TurnPage);
    // A tap on a Descent's row selects it and stays.
    let _ = press(
        &mut game,
        PlayerInput {
            menu_pick: Some(1),
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::DescentProgress));
    assert_eq!(game.shell.seat(0).selected_menu_item, 1);
    let _ = press(
        &mut game,
        PlayerInput {
            tab_previous: true,
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::ExpeditionCodex));
    game.shell.seat_mut(0).modal = Some(ModalScreen::DescentProgress);
    let _ = press(
        &mut game,
        PlayerInput {
            menu_pick: Some(8),
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Map));
}

/// Wave 10: waking the Heart opens the ending's pages on every local seat; a tap turns each
/// page and the last one closes it, and the game goes on.
#[test]
fn waking_the_heart_opens_the_ending_on_every_seat_and_taps_turn_its_pages() {
    use crate::shell::expedition::ENDING_PAGES;
    let mut world = deep_world();
    world.set_expedition(ExpeditionState {
        echoes: u8::try_from(ECHOES.len()).unwrap_or(u8::MAX),
        ..ExpeditionState::default()
    });
    let mut game = playing(world, 2, "story-ending");
    idle(&mut game, 2);
    put_at_log(&mut game, 1);
    game.edit_world(|world| {
        world.note_descent_boss_beaten(fatherload_simulation::descent::DescentBoss::TheHeart);
    });
    idle(&mut game, 2);
    assert!(game.world().expedition().cable_lit);
    for seat in 0..2 {
        assert_eq!(game.shell.seat(seat).modal, Some(ModalScreen::Ending));
        assert_eq!(game.shell.seat(seat).selected_menu_item, 0);
    }
    for page in 1..ENDING_PAGES {
        let _ = press(
            &mut game,
            PlayerInput {
                menu_pick: Some(0),
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.seat(0).selected_menu_item, page);
    }
    let _ = press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, None, "the last page closes");
    // It plays once: more ticks never reopen it.
    idle(&mut game, 5);
    assert_eq!(game.shell.seat(0).modal, None);
}
