//! The Atlas (Wave 14 W14-C) through `LocalGame::frame`: F2 opens it only in a test-mode world,
//! its categories, search and map follow the seat's inputs, Go travels (after the preview),
//! Back comes home, and each split-screen seat drives its own.

use fatherload_simulation::{
    destination::DestinationKind as IndexKind,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::*;
use crate::atlas::{
    AtlasCategory,
    hit::{AtlasButton, AtlasHit},
    index::DestinationKind,
    map::MapKind,
};

const SEED: u64 = 0x0A71_A514;

/// A test-mode Deep Descent world (its Descents, bosses and space) with `seats` seats.
fn atlas_game(seats: usize, name: &str) -> LocalGame {
    let settings = WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    };
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &settings,
        fatherload_simulation::SimulationParams::new(30),
    );
    world.enable_test_mode();
    let mut game = playing(world, seats, name);
    idle(&mut game, 3);
    game
}

fn f2() -> PlayerInput {
    PlayerInput {
        atlas: true,
        local_multiplayer_toggle: true,
        ..PlayerInput::default()
    }
}

fn pick(hit: AtlasHit) -> PlayerInput {
    PlayerInput {
        menu_pick: Some(hit.encode()),
        ..PlayerInput::default()
    }
}

fn typed(character: char) -> PlayerInput {
    PlayerInput {
        text_input: Some(character),
        ..PlayerInput::default()
    }
}

fn depth_of(game: &LocalGame, player: PlayerId) -> i32 {
    let world = game.runtime.world();
    world.player(player).expect("rig").tile_in(world.frame()).y
}

#[test]
fn f2_opens_the_atlas_only_in_a_test_mode_world() {
    let mut normal = playing(new_world(), 1, "atlas-normal");
    idle(&mut normal, 2);
    press_seat(&mut normal, 0, f2());
    assert_ne!(normal.shell.seat(0).modal, Some(ModalScreen::Atlas));
    assert!(
        !PauseOption::for_session(None, false).contains(&PauseOption::Atlas),
        "a normal world's pause menu has no Atlas"
    );

    let mut game = atlas_game(1, "atlas-open");
    press_seat(&mut game, 0, f2());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Atlas));
    assert_eq!(
        game.seats().len(),
        1,
        "F2 opened the Atlas, no seat was added"
    );
    let atlas = &game.shell.seat(0).atlas;
    assert_eq!(atlas.category, AtlasCategory::Depth);
    assert!(
        atlas
            .shown
            .iter()
            .filter(|destination| destination.kind == DestinationKind::Index(IndexKind::Descent))
            .count()
            == 8,
        "all eight Descents are listed"
    );
    // Gameplay keys do nothing while it is open; F2 closes it.
    press_seat(&mut game, 0, f2());
    assert_eq!(game.shell.seat(0).modal, None);
    // The pause menu opens it too.
    assert!(PauseOption::for_session(None, true).contains(&PauseOption::Atlas));
}

#[test]
fn categories_search_and_the_preview_follow_the_inputs() {
    let mut game = atlas_game(1, "atlas-browse");
    press_seat(&mut game, 0, f2());
    press_seat(
        &mut game,
        0,
        pick(AtlasHit::Category(AtlasCategory::Bosses.index())),
    );
    let atlas = &game.shell.seat(0).atlas;
    assert_eq!(atlas.category, AtlasCategory::Bosses);
    // The index lists the arenas nearest first: the Crust's Sinkhole leads at the surface.
    assert_eq!(
        atlas.current().map(|destination| destination.name.as_str()),
        Some("The Sinkhole")
    );
    assert_eq!(
        atlas.current().map(|destination| destination.kind),
        Some(DestinationKind::Index(IndexKind::BossArena))
    );
    // Next of the same kind, and the list's own steps.
    press_seat(&mut game, 0, pick(AtlasHit::Button(AtlasButton::Next)));
    assert_eq!(game.shell.seat(0).atlas.selected, 1);
    press_seat(&mut game, 0, down());
    assert_eq!(game.shell.seat(0).atlas.selected, 2);
    // Right steps the category.
    press_seat(
        &mut game,
        0,
        PlayerInput {
            menu_right: true,
            ..PlayerInput::default()
        },
    );
    assert_eq!(
        game.shell.seat(0).atlas.category,
        AtlasCategory::Settlements
    );

    // Type-ahead: "/" starts it, letters type (even W, A, S, D and E), Enter finishes.
    press_seat(&mut game, 0, typed('/'));
    assert!(game.shell.seat(0).atlas.typing);
    assert!(
        game.shell.text_entry(),
        "global hotkeys keep off the typing"
    );
    for character in "leviathan".chars() {
        press_seat(&mut game, 0, typed(character));
    }
    let atlas = &game.shell.seat(0).atlas;
    assert_eq!(atlas.search, "leviathan");
    assert!(
        atlas
            .shown
            .first()
            .is_some_and(|destination| destination.keywords.contains("leviathan")),
        "{:?}",
        atlas.shown.first().map(|destination| &destination.name)
    );
    press_seat(&mut game, 0, confirm_input());
    assert!(!game.shell.seat(0).atlas.typing);
    assert_eq!(
        game.shell.seat(0).modal,
        Some(ModalScreen::Atlas),
        "Enter only finished the typing: travel always asks first"
    );
    // "descent 5" and "planet ice" find their places.
    for (query, wanted) in [
        ("descent 5", DestinationKind::Index(IndexKind::Descent)),
        ("planet ice", DestinationKind::Index(IndexKind::Planet)),
    ] {
        press_seat(&mut game, 0, pick(AtlasHit::Search));
        for _ in 0..12 {
            press_seat(
                &mut game,
                0,
                PlayerInput {
                    text_backspace: true,
                    ..PlayerInput::default()
                },
            );
        }
        for character in query.chars() {
            press_seat(&mut game, 0, typed(character));
        }
        let first = game.shell.seat(0).atlas.shown.first().cloned();
        assert_eq!(
            first.map(|destination| destination.kind),
            Some(wanted),
            "{query}"
        );
        press_seat(&mut game, 0, confirm_input());
    }
    // Back out: the search clears, then the Atlas closes.
    press_seat(&mut game, 0, back_input());
    assert_eq!(game.shell.seat(0).atlas.search, "");
    press_seat(&mut game, 0, back_input());
    assert_eq!(game.shell.seat(0).modal, None);
}

fn back_input() -> PlayerInput {
    PlayerInput {
        back: true,
        ..PlayerInput::default()
    }
}

#[test]
fn go_travels_after_the_preview_and_back_comes_home() {
    let mut game = atlas_game(1, "atlas-travel");
    let start = depth_of(&game, PLAYER);
    press_seat(&mut game, 0, f2());
    // Descent II, previewed (selected), then Go.
    let row = game
        .shell
        .seat(0)
        .atlas
        .shown
        .iter()
        .position(|destination| destination.name.starts_with("Descent II:"))
        .expect("Descent II is listed");
    press_seat(&mut game, 0, pick(AtlasHit::Row(row)));
    assert_eq!(
        game.shell.seat(0).modal,
        Some(ModalScreen::Atlas),
        "picking a row only previews it"
    );
    assert_eq!(depth_of(&game, PLAYER), start, "nothing moved yet");
    press_seat(&mut game, 0, pick(AtlasHit::Button(AtlasButton::Go)));
    idle(&mut game, 3);
    assert_eq!(game.shell.seat(0).modal, None, "Go closes the Atlas");
    let deep = depth_of(&game, PLAYER);
    assert!(
        deep > start + 500,
        "the rig travelled down: {start} -> {deep}"
    );
    // The world keeps where it was on the player's back-stack (saved and replicated).
    assert_eq!(
        game.world().player_travel(PLAYER).recent.len(),
        1,
        "where it was is kept for Back"
    );

    // Back: the Atlas's Back button travels to where the rig was.
    press_seat(&mut game, 0, f2());
    press_seat(&mut game, 0, pick(AtlasHit::Button(AtlasButton::Back)));
    idle(&mut game, 3);
    assert!(
        depth_of(&game, PLAYER) < start + 20,
        "back at the surface: {}",
        depth_of(&game, PLAYER)
    );
    assert_eq!(game.world().player_travel(PLAYER).recent, []);
}

#[test]
fn map_mode_moves_a_cursor_and_the_spot_follows() {
    let mut game = atlas_game(1, "atlas-map");
    press_seat(&mut game, 0, f2());
    press_seat(
        &mut game,
        0,
        PlayerInput {
            map: true,
            ..PlayerInput::default()
        },
    );
    let map = game.shell.seat(0).atlas.map.clone().expect("map mode");
    assert_eq!(map.kind, MapKind::Depth, "at home the depth map opens");
    let before = map.spot.expect("a spot under the cursor").target;
    for _ in 0..6 {
        press_seat(&mut game, 0, down());
    }
    let after = game
        .shell
        .seat(0)
        .atlas
        .map
        .as_ref()
        .and_then(|map| map.spot.clone())
        .expect("spot");
    match (before, after.target) {
        (crate::atlas::AtlasTarget::Home(a), crate::atlas::AtlasTarget::Home(b)) => {
            assert!(
                b.y > a.y + 100,
                "the cursor went down the Descents: {a:?} -> {b:?}"
            );
        }
        other => panic!("depth spots expected: {other:?}"),
    }
    // A tap on the map moves the cursor there; the Space map is the galaxy.
    press_seat(&mut game, 0, pick(AtlasHit::Map(0.5, 0.9)));
    let cursor = game.shell.seat(0).atlas.map.as_ref().expect("map").cursor;
    assert!((cursor.1 - 0.9).abs() < 0.01);
    // Map again: the galaxy (space is open on a Deep Descent world), then the list.
    press_seat(&mut game, 0, pick(AtlasHit::Button(AtlasButton::Map)));
    assert_eq!(
        game.shell.seat(0).atlas.map.as_ref().map(|map| map.kind),
        Some(MapKind::Galaxy)
    );
    press_seat(&mut game, 0, pick(AtlasHit::Button(AtlasButton::Map)));
    assert!(game.shell.seat(0).atlas.map.is_none(), "and the list");
    press_seat(
        &mut game,
        0,
        pick(AtlasHit::Category(AtlasCategory::Space.index())),
    );
    press_seat(&mut game, 0, pick(AtlasHit::Button(AtlasButton::Map)));
    assert_eq!(
        game.shell.seat(0).atlas.map.as_ref().map(|map| map.kind),
        Some(MapKind::Galaxy)
    );
}

#[test]
fn each_split_screen_seat_has_its_own_atlas() {
    let mut game = atlas_game(2, "atlas-split");
    let inputs = [f2(), f2()];
    let _ = step_seats(&mut game, &inputs);
    idle(&mut game, 1);
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Atlas));
    assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Atlas));
    press_seat(
        &mut game,
        1,
        pick(AtlasHit::Category(AtlasCategory::Players.index())),
    );
    assert_eq!(game.shell.seat(0).atlas.category, AtlasCategory::Depth);
    let players = &game.shell.seat(1).atlas;
    assert_eq!(players.category, AtlasCategory::Players);
    assert_eq!(players.shown.len(), 1, "the other seat's player");
    // Seat 2 bookmarks; seat 1's bookmarks stay empty (the world keeps each player's).
    let second = game.seats()[1].player;
    press_seat(&mut game, 1, pick(AtlasHit::Button(AtlasButton::Bookmark)));
    idle(&mut game, 2);
    assert_eq!(game.world().player_travel(second).bookmarks.len(), 1);
    assert_eq!(game.world().player_travel(PLAYER).bookmarks, []);
    assert_eq!(game.shell.seat(1).atlas.marks.len(), 1);
    assert_eq!(game.shell.seat(0).atlas.marks, []);
    // Seat 1 travels; seat 2's rig stays.
    let second_depth = depth_of(&game, second);
    press_seat(&mut game, 0, pick(AtlasHit::Row(4)));
    press_seat(&mut game, 0, pick(AtlasHit::Button(AtlasButton::Go)));
    idle(&mut game, 3);
    assert_eq!(depth_of(&game, second), second_depth);
    assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Atlas));
}

/// A tester's pass (review): Go on one destination of every kind the Atlas lists, one after
/// another without coming home between, and each trip lands where its preview said (the
/// destination index's `TravelTo`): a planet, moon, lair or alien town on that body, a space
/// target in free flight, a home target within a pocket's reach of its tile.
#[test]
fn go_reaches_every_kind_of_destination_one_after_another() {
    use crate::atlas::AtlasTarget;
    use fatherload_simulation::place::{BodyId, Place};

    let mut game = atlas_game(1, "atlas-every-kind");
    let mut seen = std::collections::BTreeSet::new();
    let mut trips = 0;
    for category in [
        AtlasCategory::Depth,
        AtlasCategory::Places,
        AtlasCategory::Bosses,
        AtlasCategory::Settlements,
        AtlasCategory::Space,
    ] {
        press_seat(&mut game, 0, f2());
        press_seat(&mut game, 0, pick(AtlasHit::Category(category.index())));
        let shown = game.shell.seat(0).atlas.shown.clone();
        assert!(!shown.is_empty(), "{category:?} lists something");
        press_seat(&mut game, 0, f2());
        assert_eq!(game.shell.seat(0).modal, None, "F2 closes it again");
        for kind in shown.iter().map(|destination| destination.kind) {
            if !seen.insert(kind) {
                continue;
            }
            press_seat(&mut game, 0, f2());
            press_seat(&mut game, 0, pick(AtlasHit::Category(category.index())));
            // Lists of places near the seat change as it travels: find the kind's row again.
            let Some(row) = game
                .shell
                .seat(0)
                .atlas
                .shown
                .iter()
                .position(|listed| listed.kind == kind)
            else {
                press_seat(&mut game, 0, f2());
                continue;
            };
            press_seat(&mut game, 0, pick(AtlasHit::Row(row)));
            let destination = game
                .shell
                .seat(0)
                .atlas
                .current()
                .cloned()
                .expect("the row previews first");
            assert_eq!(destination.kind, kind);
            press_seat(&mut game, 0, pick(AtlasHit::Button(AtlasButton::Go)));
            idle(&mut game, 12);
            let world = game.runtime.world();
            let place = world.place_of(PLAYER);
            let what = format!(
                "{category:?} {:?} {} ({})",
                destination.kind,
                destination.name,
                game.shell.seat(0).message
            );
            match destination.target {
                AtlasTarget::Body { body, .. } => {
                    assert_eq!(place, Place::OnBody(BodyId::Space(body)), "{what}");
                }
                AtlasTarget::Space { .. } => assert_eq!(place, Place::Space, "{what}"),
                AtlasTarget::Home(tile) => {
                    assert!(place.is_home_store(), "{what}: {place:?}");
                    let at = world.player(PLAYER).expect("rig").tile_in(world.frame());
                    // Safe arrival settles onto a floor within 24 rows and carves its pocket
                    // at the nearest column clear of fixtures.
                    assert!(
                        (at.y - tile.y).abs() <= 32 && (at.x - tile.x).abs() <= 200,
                        "{what}: arrived at {at:?} for {tile:?}"
                    );
                }
                AtlasTarget::Story => {}
            }
            trips += 1;
        }
    }
    assert!(trips >= 25, "one trip per kind: {trips} ({seen:?})");
}

/// The god-mode toolkit end to end through the game loop (Wave 14 integration, §19): the
/// Atlas shows a preview of every kind of destination it lists and each one carries the
/// index's travel; the palette drives the same index and toolkit actions (stock, a theme
/// boss's lair, a flagship, a village, a station, back); two split-screen seats use their own
/// Atlases to go to two different places.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the Atlas, the palette and split-screen in turn"
)]
fn the_toolkit_end_to_end_atlas_previews_palette_and_split_screen() {
    use crate::{atlas::AtlasTarget, rendering::atlas::preview::has_preview};
    use fatherload_simulation::place::{BodyId, Place};

    // 1. The Atlas: every kind it lists has a preview and travels by the index.
    let mut game = atlas_game(1, "toolkit-atlas");
    press_seat(&mut game, 0, f2());
    let mut kinds = std::collections::BTreeSet::new();
    for category in [
        AtlasCategory::Depth,
        AtlasCategory::Places,
        AtlasCategory::Bosses,
        AtlasCategory::Settlements,
        AtlasCategory::Space,
    ] {
        press_seat(&mut game, 0, pick(AtlasHit::Category(category.index())));
        let shown = game.shell.seat(0).atlas.shown.clone();
        for (row, destination) in shown.iter().enumerate() {
            if !kinds.insert(destination.kind) {
                continue;
            }
            press_seat(&mut game, 0, pick(AtlasHit::Row(row)));
            let current = game.shell.seat(0).atlas.current().cloned().expect("chosen");
            assert_eq!(current.kind, destination.kind);
            assert!(
                has_preview(game.world(), &current.target),
                "{:?} {} has a preview",
                current.kind,
                current.name
            );
            assert!(
                current.request.is_some() || current.target == AtlasTarget::Story,
                "{:?} travels by the index",
                current.kind
            );
        }
    }
    for wanted in [
        IndexKind::Descent,
        IndexKind::Stratum,
        IndexKind::BossArena,
        IndexKind::ThemeBossLair,
        IndexKind::FactionBossLair,
        IndexKind::Village,
        IndexKind::Settlement,
        IndexKind::AlienTown,
        IndexKind::Planet,
        IndexKind::Station,
    ] {
        assert!(
            kinds.contains(&DestinationKind::Index(wanted)),
            "the Atlas lists a {wanted:?}"
        );
    }
    press_seat(&mut game, 0, f2());

    // 2. The palette: the same index and toolkit actions.
    let backtick = PlayerInput {
        console: true,
        ..PlayerInput::default()
    };
    let line = |game: &mut LocalGame, text: &str| {
        for character in text.chars() {
            press_seat(game, 0, typed(character));
        }
        press_seat(game, 0, confirm_input());
        idle(game, 6);
    };
    press_seat(&mut game, 0, backtick);
    assert!(game.shell.console.open);
    line(&mut game, "give all");
    assert_eq!(game.world().test_stock_gaps(PLAYER), [] as [String; 0]);
    line(&mut game, "tp boss sporemother");
    assert!(
        matches!(
            game.world().place_of(PLAYER),
            Place::OnBody(BodyId::Space(_))
        ),
        "at the Sporemother's lair: {:?}",
        game.world().place_of(PLAYER)
    );
    line(&mut game, "tp boss pirates");
    assert_eq!(
        game.world().place_of(PLAYER),
        Place::Space,
        "at the flagship"
    );
    line(&mut game, "tp village");
    assert!(
        game.world().place_of(PLAYER).is_home_store(),
        "at a village"
    );
    line(&mut game, "tp station");
    assert_eq!(game.world().place_of(PLAYER), Place::Space, "by a station");
    line(&mut game, "tp back");
    assert!(
        game.world().place_of(PLAYER).is_home_store(),
        "back at the village"
    );
    press_seat(&mut game, 0, backtick);

    // 3. Split-screen: each seat's own Atlas takes its own rig somewhere else.
    let mut game = atlas_game(2, "toolkit-split");
    let second = game.seats()[1].player;
    let _ = step_seats(&mut game, &[f2(), f2()]);
    idle(&mut game, 1);
    press_seat(
        &mut game,
        1,
        pick(AtlasHit::Category(AtlasCategory::Space.index())),
    );
    let planet = game
        .shell
        .seat(1)
        .atlas
        .shown
        .iter()
        .position(|destination| destination.kind == DestinationKind::Index(IndexKind::Planet))
        .expect("a planet");
    press_seat(&mut game, 1, pick(AtlasHit::Row(planet)));
    press_seat(&mut game, 1, pick(AtlasHit::Button(AtlasButton::Go)));
    let descent = game
        .shell
        .seat(0)
        .atlas
        .shown
        .iter()
        .position(|destination| destination.name.starts_with("Descent IV:"))
        .expect("Descent IV");
    press_seat(&mut game, 0, pick(AtlasHit::Row(descent)));
    press_seat(&mut game, 0, pick(AtlasHit::Button(AtlasButton::Go)));
    idle(&mut game, 6);
    assert!(
        matches!(
            game.world().place_of(second),
            Place::OnBody(BodyId::Space(_))
        ),
        "seat 2 on a planet"
    );
    assert_eq!(game.world().place_of(PLAYER), Place::HOME, "seat 1 at home");
    assert!(depth_of(&game, PLAYER) > 3_000, "seat 1 down in Descent IV");
}
