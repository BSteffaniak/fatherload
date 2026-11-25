//! Many places at once (Part III F11): a rig landed on a space body (the place view stub's
//! ground, stars and HUD), a rig in free flight, and split-screen seats in different places,
//! each with its own camera on its own ground.

#![allow(
    clippy::suboptimal_flops,
    reason = "scene staging: plain arithmetic on tile positions"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    item::{ItemKind, ItemStack},
    place::{BodyId, Place, SpaceBodyKey, StoreKey},
    position::WorldPos,
    terrain::{MineralKind, TileKind, TilePosition},
};

use super::{Scene, SceneStage, park_rig, sink_rig};

/// The body every place scene lands on.
const BODY: SpaceBodyKey = SpaceBodyKey::new(5, -3, 1, 2);

const fn body() -> Place {
    Place::OnBody(BodyId::Space(BODY))
}

/// Lands `player` on the body at `column`, standing on its ground, with a short shaft dug
/// beside it and a few loose nuggets lying about.
fn land(world: &mut WorldState, player: PlayerId, column: i64) {
    world
        .move_to_place(player, body(), WorldPos::tile_center(column, -1))
        .expect("the rig lands");
    let _ = world.with_store_ground(StoreKey::Body(BODY), |world| {
        let shaft = i32::try_from(column).unwrap_or(0) + 4;
        for y in 0..7 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x: shaft, y }, TileKind::Air);
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x: shaft + 1, y }, TileKind::Air);
        }
        for (dx, mineral) in [(-3.0, MineralKind::Copper), (7.5, MineralKind::Iron)] {
            let (x, y) = world.local_pos(WorldPos::tile_center(column, -1));
            let _ = world.spawn_item_drop(
                x + dx * 32.0,
                y,
                (0.0, 0.0),
                ItemStack::new(ItemKind::Mineral(mineral), 1),
                None,
            );
        }
    });
}

pub(super) const PLACE_SCENES: &[Scene] = &[
    Scene {
        name: "place_body_landed",
        summary: "A rig landed on a space body: its ground, a dug shaft and the stars (F11 stub)",
        setup: |stage: &mut SceneStage<'_>| {
            stage.play(1, |world, seats| land(world, seats[0].player, 0));
            stage.settle(40);
        },
    },
    Scene {
        name: "place_free_flight",
        summary: "A rig in free flight in space (F11 stub: stars, no ground)",
        setup: |stage: &mut SceneStage<'_>| {
            stage.play(1, |world, seats| {
                world
                    .move_to_place(seats[0].player, Place::Space, WorldPos::tile_center(0, 0))
                    .expect("flies");
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "split_two_places",
        summary: "Split-screen: one seat in town, the other on a space body (F11)",
        setup: |stage: &mut SceneStage<'_>| {
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, 86, 4);
                if let Some(second) = seats.get(1) {
                    land(world, second.player, -10);
                }
            });
            stage.settle(40);
        },
    },
    Scene {
        name: "split_four_places",
        summary: "Four seats in four places: town, underground, a space body and free flight (F11)",
        setup: |stage: &mut SceneStage<'_>| {
            stage.play(4, |world, seats| {
                park_rig(world, seats[0].player, 86, 4);
                if let Some(seat) = seats.get(1) {
                    sink_rig(world, seat.player, 112, 16);
                }
                if let Some(seat) = seats.get(2) {
                    land(world, seat.player, 20);
                }
                if let Some(seat) = seats.get(3) {
                    world
                        .move_to_place(seat.player, Place::Space, WorldPos::tile_center(40, 0))
                        .expect("flies");
                }
            });
            stage.settle(40);
        },
    },
];
