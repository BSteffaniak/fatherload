//! The pilot on foot (Part II F5): the pilot beside its parked rig, every pose of the sprite
//! sheet in a row, climbing the outpost's ladder, crawling through a crawlway behind vent
//! grates, jetpacking over the meadow, walking a village inn, the pilot's HUD and pack, and a
//! split-screen pair (one seat on foot, one in its rig).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops,
    reason = "scene layouts mix small tile indices and world pixels"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    item::{BlockKind, ItemKind},
    pilot::{PilotGear, PilotMode},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    village::{BuildingKind, VillageSetting},
};

use super::{
    ARROWS, Scene, SceneStage, WASD,
    base_scenes::{FLOOR, ROOM_RIGHT, build_outpost, stand},
    carve, reveal, sink_rig, tile_centre,
    village_scenes::{find_village, inside, park_in},
};
use crate::{input::PlayerInput, shell::ModalScreen};

/// The meadow's ground row (a flat strip west of town).
const GROUND: i32 = 5;

/// Flattens columns 2..60 west of town to a dirt meadow on the classic ground line.
fn meadow(world: &mut WorldState) {
    for x in 2..60 {
        for y in -4..GROUND {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
        for y in GROUND..GROUND + 3 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Dirt);
        }
    }
}

/// `player`'s rig resting on the floor row `floor` (the top of the floor tiles) at column `x`.
fn rest_rig(world: &mut WorldState, player: PlayerId, x: i32, floor: i32) {
    if let Some(rig) = world.player_mut(player) {
        rig.x = tile_centre(x);
        rig.y = floor as f32 * TILE_SIZE - 11.0;
        rig.x_remainder = 0.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
}

/// Puts `player`'s pilot out with `gear`, its feet at world `(x, feet)`.
fn pilot_out(world: &mut WorldState, player: PlayerId, gear: PilotGear, x: f32, feet: f32) {
    if let Some(rig) = world.player_mut(player) {
        rig.pilot_gear = gear;
    }
    let _ = world.place_pilot(player, x, feet - 0.25);
}

fn walk(horizontal: f32) -> PlayerInput {
    PlayerInput {
        horizontal,
        ..PlayerInput::default()
    }
}

pub const PILOT_SCENES: &[Scene] = &[
    Scene {
        name: "pilot_beside_rig",
        summary: "On foot on the meadow: the pilot walks away from its parked rig (hazard lights blinking, lamp left on)",
        setup: beside_rig,
    },
    Scene {
        name: "pilot_sheet",
        summary: "The pilot sprite sheet: idle, walk, run, jump, fall, climb, crouch, crawl, swim, hurt, jetpack, hang and the pick swing, then suit paint jobs and tiers",
        setup: |stage| {
            stage.rig_sheet = Some(crate::rendering::rig_art::sheet::RigSheet::Pilots);
        },
    },
    Scene {
        name: "pilot_animation_strip",
        summary: "Wave 6: every pilot animation frame by frame (walk, run, Moon lope and float, jump, ledge, glide, jetpack, swim, hurt, downed, climb)",
        setup: |stage| {
            stage.rig_sheet = Some(crate::rendering::rig_art::sheet::RigSheet::PilotStrip);
        },
    },
    Scene {
        name: "ammo_icons",
        summary: "Wave 6: each ammunition type's own icon (rockets, rifle cartridges, shotgun shells), large and at HUD size",
        setup: |stage| {
            stage.rig_sheet = Some(crate::rendering::rig_art::sheet::RigSheet::Ammo);
        },
    },
    Scene {
        name: "pilot_ladder_climb",
        summary: "Climbing the underground outpost's lift ladder on foot, the rig parked in the room below",
        setup: ladder,
    },
    Scene {
        name: "pilot_crawlway",
        summary: "Crawling through a 1-tile crawlway between two vent grates: the parked rig cannot follow",
        setup: crawlway,
    },
    Scene {
        name: "pilot_jetpack",
        summary: "A jetpack burn over the meadow: twin flames, the burn gauge over the helmet and in the HUD",
        setup: jetpack,
    },
    Scene {
        name: "pilot_village_interior",
        summary: "The pilot walked into a village inn (no rig at the door flow): the room on foot",
        setup: village_interior,
    },
    Scene {
        name: "pilot_inventory_pack",
        summary: "The inventory on foot: the pilot pack's meter and rows to stow stacks in the rig or pack them",
        setup: inventory_pack,
    },
    Scene {
        name: "pilot_split_screen",
        summary: "Split-screen: player 1 on foot beside its rig (pilot HUD, board prompt), player 2 drilling in its rig",
        setup: split_screen,
    },
];

fn beside_rig(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        meadow(world);
        reveal(world, player, 0..60, -4..GROUND + 6);
        rest_rig(world, player, 20, GROUND);
        pilot_out(
            world,
            player,
            PilotGear {
                jetpack: 1,
                suit: 1,
                ..PilotGear::default()
            },
            tile_centre(20) + 34.0,
            GROUND as f32 * TILE_SIZE,
        );
    });
    stage.settle(10);
    stage.hold(26, walk(1.0));
}

fn ladder(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        build_outpost(world, player);
        stand(world, player, 106, FLOOR - 1);
        pilot_out(
            world,
            player,
            PilotGear {
                jetpack: 0,
                suit: 2,
                ..PilotGear::default()
            },
            tile_centre(ROOM_RIGHT - 1),
            (FLOOR - 4) as f32 * TILE_SIZE,
        );
        if let Some((pilot, _, _)) = world.pilot_of(player) {
            world.edit_pilot(pilot, |_, state| state.mode = PilotMode::Climb);
        }
    });
    stage.settle(4);
    stage.hold(
        14,
        PlayerInput {
            thrust: true,
            ..PlayerInput::default()
        },
    );
}

/// The crawlway's row (its floor is the row below).
const CRAWL_ROW: i32 = 41;

fn crawlway(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        for y in CRAWL_ROW - 9..CRAWL_ROW + 4 {
            for x in 88..132 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Stone);
            }
        }
        carve(world, 90..104, CRAWL_ROW - 3..CRAWL_ROW + 1);
        carve(world, 114..130, CRAWL_ROW - 3..CRAWL_ROW + 1);
        let _ = world.carve_crawlway(
            TilePosition {
                x: 104,
                y: CRAWL_ROW,
            },
            10,
        );
        rest_rig(world, player, 98, CRAWL_ROW + 1);
        reveal(world, player, 84..136, CRAWL_ROW - 10..CRAWL_ROW + 5);
        pilot_out(
            world,
            player,
            PilotGear {
                jetpack: 0,
                suit: 1,
                ..PilotGear::default()
            },
            tile_centre(107),
            (CRAWL_ROW + 1) as f32 * TILE_SIZE,
        );
        if let Some((pilot, _, _)) = world.pilot_of(player) {
            world.edit_pilot(pilot, |_, state| state.crouching = true);
        }
    });
    stage.settle(4);
    stage.hold(
        20,
        PlayerInput {
            horizontal: 1.0,
            drill_down: true,
            ..PlayerInput::default()
        },
    );
}

fn jetpack(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        meadow(world);
        reveal(world, player, 0..60, -8..GROUND + 6);
        rest_rig(world, player, 16, GROUND);
        pilot_out(
            world,
            player,
            PilotGear {
                jetpack: 2,
                suit: 2,
                ..PilotGear::default()
            },
            tile_centre(22),
            (GROUND as f32 - 2.5) * TILE_SIZE,
        );
    });
    stage.hold(
        40,
        PlayerInput {
            horizontal: 0.4,
            jump: true,
            ..PlayerInput::default()
        },
    );
}

fn village_interior(stage: &mut SceneStage<'_>) {
    let mut chosen = None;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let found = find_village(world, |found| {
            found.setting == VillageSetting::Surface
                && found
                    .buildings
                    .iter()
                    .any(|lot| lot.kind == BuildingKind::Inn)
        });
        let building = found
            .buildings
            .iter()
            .position(|lot| lot.kind == BuildingKind::Inn)
            .expect("an inn");
        let door = found.buildings[building].first_column + 2;
        park_in(world, player, &found, door - 6);
        pilot_out(
            world,
            player,
            PilotGear {
                jetpack: 1,
                suit: 0,
                ..PilotGear::default()
            },
            tile_centre(door),
            found.ground_row as f32 * TILE_SIZE,
        );
        chosen = Some((found, building));
    });
    stage.settle(5);
    let (found, building) = chosen.expect("staged");
    inside(stage, &found, building, 250.0);
    stage.hold(12, walk(1.0));
}

fn inventory_pack(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        meadow(world);
        reveal(world, player, 0..60, -4..GROUND + 6);
        rest_rig(world, player, 20, GROUND);
        if let Some(rig) = world.player_mut(player) {
            rig.cargo_bay.add(MineralKind::Gold, 4);
            rig.cargo_bay.add(MineralKind::Copper, 7);
            rig.block_hopper.add(ItemKind::Block(BlockKind::Stone), 12);
            rig.pilot_pack.add(MineralKind::Silver, 3);
            rig.pilot_pack.add(ItemKind::Block(BlockKind::Dirt), 2);
        }
        pilot_out(
            world,
            player,
            PilotGear {
                jetpack: 1,
                suit: 1,
                ..PilotGear::default()
            },
            tile_centre(20) + 34.0,
            GROUND as f32 * TILE_SIZE,
        );
    });
    stage.settle(5);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::Inventory);
    seat.selected_menu_item = 4;
}

fn split_screen(stage: &mut SceneStage<'_>) {
    stage.play(2, |world, seats| {
        let (first, second) = (seats[0].player, seats[1].player);
        meadow(world);
        reveal(world, first, 0..60, -4..GROUND + 6);
        rest_rig(world, first, 20, GROUND);
        pilot_out(
            world,
            first,
            PilotGear {
                jetpack: 1,
                suit: 1,
                ..PilotGear::default()
            },
            tile_centre(20) + 30.0,
            GROUND as f32 * TILE_SIZE,
        );
        sink_rig(world, second, 44, 14);
    });
    stage.use_devices(&[WASD, ARROWS]);
    stage.settle(12);
}
