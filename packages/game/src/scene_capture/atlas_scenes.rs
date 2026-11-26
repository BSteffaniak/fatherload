//! The Atlas (Wave 14 W14-C): one scene per category, the search, both maps, split screen and
//! the touch screen, each staged through the seat's real inputs (F2, taps on its parts, typed
//! letters) in a test-mode Deep Descent world.

use fatherload_simulation::{
    WorldState,
    depth_progression::ProgressionRules,
    terrain::Terrain,
    worldgen::{GeneratorVersion, SPAWN_TILE_X, WorldSettings, descent::landmarks},
};

use super::{Scene, SceneStage, place_rig};
use crate::{
    atlas::{
        AtlasCategory,
        hit::{AtlasButton, AtlasHit},
    },
    input::{PlayerInput, devices::InputDevice},
};

/// The scenes, chained after the main list.
pub const ATLAS_SCENES: &[Scene] = &[
    Scene {
        name: "atlas_depth",
        summary: "The Atlas's Depth list: Descent III selected, its tile preview and card",
        setup: |stage| {
            open(stage, 1);
            pick(stage, 0, AtlasHit::Row(3));
        },
    },
    Scene {
        name: "atlas_stratum",
        summary: "The Atlas: a deep stratum (the Drowned Deep's Leviathan waters) with its creatures",
        setup: |stage| {
            open(stage, 1);
            search(stage, 0, "abyssal");
        },
    },
    Scene {
        name: "atlas_places",
        summary: "The Atlas's Places: the Moon selected",
        setup: |stage| {
            open(stage, 1);
            category(stage, 0, AtlasCategory::Places);
            pick(stage, 0, AtlasHit::Row(1));
        },
    },
    Scene {
        name: "atlas_bosses",
        summary: "The Atlas's Bosses: the Leviathan's gate, its sprite over the arena",
        setup: |stage| {
            open(stage, 1);
            category(stage, 0, AtlasCategory::Bosses);
            pick(stage, 0, AtlasHit::Row(3));
        },
    },
    Scene {
        name: "atlas_settlements",
        summary: "The Atlas's Settlements: a deep hall",
        setup: |stage| {
            open(stage, 1);
            category(stage, 0, AtlasCategory::Settlements);
            pick(stage, 0, AtlasHit::Row(0));
        },
    },
    Scene {
        name: "atlas_space",
        summary: "The Atlas's Space: a planet from orbit, its landing site and the galaxy minimap",
        setup: |stage| {
            open(stage, 1);
            category(stage, 0, AtlasCategory::Space);
            pick(stage, 0, AtlasHit::Row(2));
        },
    },
    Scene {
        name: "atlas_space_system",
        summary: "The Atlas's Space: a star system from afar",
        setup: |stage| {
            open(stage, 1);
            search(stage, 0, "system");
        },
    },
    Scene {
        name: "atlas_search",
        summary: "The Atlas's search: \"planet ice\" typed, the ice world shown",
        setup: |stage| {
            open(stage, 1);
            search(stage, 0, "planet ice");
        },
    },
    Scene {
        name: "atlas_players",
        summary: "The Atlas's Players in split screen: seat 1 previews seat 2",
        setup: |stage| {
            open(stage, 2);
            category(stage, 0, AtlasCategory::Players);
            stage.hold_seats(1, &[PlayerInput::default(), close()]);
        },
    },
    Scene {
        name: "atlas_bookmarks",
        summary: "The Atlas's Bookmarks after bookmarking two places",
        setup: |stage| {
            open(stage, 1);
            pick(stage, 0, AtlasHit::Row(2));
            pick(stage, 0, AtlasHit::Button(AtlasButton::Bookmark));
            category(stage, 0, AtlasCategory::Bosses);
            pick(stage, 0, AtlasHit::Row(1));
            pick(stage, 0, AtlasHit::Button(AtlasButton::Bookmark));
            category(stage, 0, AtlasCategory::Bookmarks);
        },
    },
    Scene {
        name: "atlas_recent",
        summary: "The Atlas's Recent after a trip: the place travelled to, Back ready",
        setup: |stage| {
            open(stage, 1);
            pick(stage, 0, AtlasHit::Row(1));
            pick(stage, 0, AtlasHit::Button(AtlasButton::Go));
            stage.settle(20);
            press(
                stage,
                0,
                PlayerInput {
                    atlas: true,
                    ..PlayerInput::default()
                },
            );
            category(stage, 0, AtlasCategory::Recent);
        },
    },
    Scene {
        name: "atlas_map_depth",
        summary: "The Atlas's map mode: all eight Descents, the cursor in Descent V",
        setup: |stage| {
            open(stage, 1);
            pick(stage, 0, AtlasHit::Button(AtlasButton::Map));
            pick(stage, 0, AtlasHit::Map(0.55, 0.52));
        },
    },
    Scene {
        name: "atlas_map_galaxy",
        summary: "The Atlas's galaxy map round home, the cursor on a system",
        setup: |stage| {
            open(stage, 1);
            category(stage, 0, AtlasCategory::Space);
            pick(stage, 0, AtlasHit::Button(AtlasButton::Map));
            pick(stage, 0, AtlasHit::Map(0.62, 0.41));
        },
    },
    Scene {
        name: "atlas_split",
        summary: "Two seats, each with its own Atlas: Depth and Space",
        setup: |stage| {
            open(stage, 2);
            pick(stage, 0, AtlasHit::Row(5));
            category(stage, 1, AtlasCategory::Space);
            pick(stage, 1, AtlasHit::Row(0));
        },
    },
    Scene {
        name: "atlas_touch",
        summary: "The Atlas on a touch screen (try --size 844x390 and 390x844)",
        setup: |stage| {
            open(stage, 1);
            category(stage, 0, AtlasCategory::Bosses);
            pick(stage, 0, AtlasHit::Row(4));
            stage.use_devices(&[InputDevice::Touch]);
        },
    },
];

/// Seed of the Atlas scenes' world.
const SCENE_SEED: u64 = 0x0A71_A514;

/// The staged world as a test-mode Deep Descent world (its eight Descents, logistics and
/// space), every rig on the ground by the spawn.
fn deep_test_world(world: &mut WorldState, seats: &[crate::runtime::LocalSeat]) {
    let settings = WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    };
    *world.terrain_mut() = Terrain::generate(&settings);
    world.set_progression_rules(ProgressionRules::Descent);
    world.install_logistics_sites(&landmarks::logistics_sites(&settings), false);
    world.enable_test_mode();
    for (index, seat) in seats.iter().enumerate() {
        let x = SPAWN_TILE_X + i32::try_from(index).unwrap_or(0) * 3;
        let y = world.terrain().surface_row(x) - 1;
        place_rig(world, seat.player, x, y);
    }
}

/// A test-mode world with `seats` seats, every seat's Atlas open (F2 / its touch button).
fn open(stage: &mut SceneStage<'_>, seats: usize) {
    stage.play(seats, deep_test_world);
    stage.settle(2);
    let open = PlayerInput {
        atlas: true,
        ..PlayerInput::default()
    };
    stage.hold_seats(1, &vec![open; seats]);
    stage.settle(1);
    // The road's first word from the archivist (space opens with the stock) is not the subject.
    stage.shell().message.clear();
    for ui in &mut stage.game.shell.seats {
        ui.message.clear();
    }
}

/// One frame with `input` on `seat` (the others idle).
fn press(stage: &mut SceneStage<'_>, seat: usize, input: PlayerInput) {
    let seats = stage.game.seats().len().max(seat + 1);
    let mut inputs = vec![PlayerInput::default(); seats];
    inputs[seat] = input;
    stage.hold_seats(1, &inputs);
}

/// `seat` taps or clicks `hit`.
fn pick(stage: &mut SceneStage<'_>, seat: usize, hit: AtlasHit) {
    press(
        stage,
        seat,
        PlayerInput {
            menu_pick: Some(hit.encode()),
            ..PlayerInput::default()
        },
    );
}

fn category(stage: &mut SceneStage<'_>, seat: usize, category: AtlasCategory) {
    pick(stage, seat, AtlasHit::Category(category.index()));
}

/// `seat` types `text` into the search box and finishes.
fn search(stage: &mut SceneStage<'_>, seat: usize, text: &str) {
    pick(stage, seat, AtlasHit::Search);
    for character in text.chars() {
        press(
            stage,
            seat,
            PlayerInput {
                text_input: Some(character),
                ..PlayerInput::default()
            },
        );
    }
}

/// Closes a seat's Atlas (F2 again).
fn close() -> PlayerInput {
    PlayerInput {
        atlas: true,
        ..PlayerInput::default()
    }
}
