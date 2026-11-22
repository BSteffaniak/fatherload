//! Fluids (Part III F9): a test-pool row of every fluid, a rig submerged in a flooded chamber,
//! the pilot swimming, a sealed pocket breached into a tunnel mid-flood, and molten metal
//! meeting water.

#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    reason = "scene layouts mix small tile indices and world pixels"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    fluid::{FluidCell, FluidKind, demo_basins},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::TileRect,
};

use super::{Scene, SceneStage, reveal, tile_centre};
use crate::input::PlayerInput;

/// Left column of the staged chambers (clear of the town).
const LEFT: i32 = 60;
/// Top row of the staged chambers.
const TOP: i32 = 16;

pub const FLUID_SCENES: &[Scene] = &[
    Scene {
        name: "fluid_pools",
        summary: "Fluids: the test pools, water, acid, brine and liquid metal side by side, the rig hovering over them",
        setup: pools,
    },
    Scene {
        name: "fluid_rig_submerged",
        summary: "Fluids: a rig under water in a flooded chamber (submarine mode), bubbles and the depth tint around it",
        setup: rig_submerged,
    },
    Scene {
        name: "fluid_pilot_swimming",
        summary: "Fluids: the pilot swimming in a flooded chamber, its rig parked on the bank",
        setup: pilot_swimming,
    },
    Scene {
        name: "fluid_flood",
        summary: "Fluids: a sealed aquifer drilled open, water pouring down a shaft and flooding the tunnel",
        setup: flood,
    },
    Scene {
        name: "fluid_metal_meets_water",
        summary: "Fluids: molten metal pouring into a pool, setting into an iron crust in a hiss of steam",
        setup: metal_meets_water,
    },
];

fn set(world: &mut WorldState, at: TilePosition, kind: TileKind) {
    let _ = world.terrain_mut().set_kind(at, kind);
}

fn fill(
    world: &mut WorldState,
    xs: std::ops::Range<i32>,
    ys: std::ops::Range<i32>,
    kind: TileKind,
) {
    for y in ys {
        for x in xs.clone() {
            set(world, TilePosition { x, y }, kind);
        }
    }
}

const fn rect(xs: &std::ops::Range<i32>, ys: &std::ops::Range<i32>) -> TileRect {
    TileRect {
        min: TilePosition {
            x: xs.start,
            y: ys.start,
        },
        max: TilePosition {
            x: xs.end - 1,
            y: ys.end - 1,
        },
    }
}

/// A rock-walled chamber: stone from `xs` x `ys`, hollow inside (one tile of wall).
fn chamber(world: &mut WorldState, xs: std::ops::Range<i32>, ys: std::ops::Range<i32>) {
    fill(world, xs.clone(), ys.clone(), TileKind::Stone);
    fill(
        world,
        xs.start + 1..xs.end - 1,
        ys.start + 1..ys.end - 1,
        TileKind::Air,
    );
}

fn place(world: &mut WorldState, player: PlayerId, x: f32, y: f32) {
    if let Some(rig) = world.player_mut(player) {
        rig.x = x;
        rig.y = y;
        rig.x_remainder = 0.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
}

fn pools(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        chamber(world, LEFT..LEFT + 35, TOP..TOP + 18);
        for (at, tile) in demo_basins(TilePosition {
            x: LEFT + 3,
            y: TOP + 4,
        }) {
            let _ = world
                .terrain_mut()
                .set_tile_from_network(at, tile.kind, tile.durability);
        }
        // The rig rests on the wall between the acid and brine basins.
        place(
            world,
            player,
            tile_centre(LEFT + 3 + 16),
            (TOP + 8) as f32 * TILE_SIZE - 11.0,
        );
        reveal(world, player, LEFT - 2..LEFT + 37, TOP - 2..TOP + 20);
    });
    // The pools settle into their compressed rest.
    stage.settle(90);
}

/// A chamber two thirds flooded with water: columns `LEFT+1..LEFT+29`, water from row `TOP+6`.
fn flooded_chamber(world: &mut WorldState, player: PlayerId) {
    chamber(world, LEFT..LEFT + 30, TOP..TOP + 16);
    // A dry ledge on the left bank for a parked rig.
    fill(
        world,
        LEFT + 1..LEFT + 6,
        TOP + 7..TOP + 15,
        TileKind::Stone,
    );
    for y in TOP + 6..TOP + 15 {
        for x in LEFT + 6..LEFT + 29 {
            let _ = world.terrain_mut().set_tile_from_network(
                TilePosition { x, y },
                TileKind::Fluid(FluidKind::Water),
                FluidCell::full(FluidKind::Water).level,
            );
        }
    }
    reveal(world, player, LEFT - 2..LEFT + 32, TOP - 2..TOP + 18);
}

fn rig_submerged(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        flooded_chamber(world, player);
        place(world, player, tile_centre(LEFT + 16), tile_centre(TOP + 10));
    });
    // Steer a little so the rig is cruising, not resting.
    stage.hold(
        40,
        PlayerInput {
            horizontal: 1.0,
            ..PlayerInput::default()
        },
    );
    stage.settle(4);
}

fn pilot_swimming(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        flooded_chamber(world, player);
        // The rig parked on the ledge, the pilot out in the water.
        place(
            world,
            player,
            tile_centre(LEFT + 3),
            (TOP + 7) as f32 * TILE_SIZE - 11.0,
        );
        let _ = world.place_pilot(player, tile_centre(LEFT + 12), tile_centre(TOP + 9));
    });
    // Stroke along under the surface.
    stage.hold(
        30,
        PlayerInput {
            horizontal: 1.0,
            ..PlayerInput::default()
        },
    );
    stage.settle(4);
}

fn flood(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        // Solid rock with a sealed aquifer up top and a tunnel along the bottom; the rig has
        // just drilled the shaft between them.
        fill(world, LEFT..LEFT + 34, TOP..TOP + 18, TileKind::Stone);
        let pocket = (LEFT + 3..LEFT + 15, TOP + 2..TOP + 7);
        for y in pocket.1.clone() {
            for x in pocket.0.clone() {
                let _ = world.terrain_mut().set_tile_from_network(
                    TilePosition { x, y },
                    TileKind::Fluid(FluidKind::Water),
                    FluidCell::full(FluidKind::Water).level,
                );
            }
        }
        let tunnel = (LEFT + 4..LEFT + 33, TOP + 14..TOP + 16);
        fill(world, tunnel.0, tunnel.1, TileKind::Air);
        // The shaft from the pocket's floor down to the tunnel.
        fill(world, LEFT + 9..LEFT + 10, TOP + 7..TOP + 14, TileKind::Air);
        world.wake_fluids(rect(&pocket.0, &pocket.1));
        place(
            world,
            player,
            tile_centre(LEFT + 17),
            (TOP + 16) as f32 * TILE_SIZE - 11.0,
        );
        reveal(world, player, LEFT - 2..LEFT + 36, TOP - 2..TOP + 20);
    });
    stage.settle(75);
}

fn metal_meets_water(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        chamber(world, LEFT..LEFT + 26, TOP..TOP + 16);
        // A water pool on the floor, and a ledge of molten metal spilling over into it.
        let pool = (LEFT + 9..LEFT + 25, TOP + 12..TOP + 15);
        for y in pool.1.clone() {
            for x in pool.0.clone() {
                let _ = world.terrain_mut().set_tile_from_network(
                    TilePosition { x, y },
                    TileKind::Fluid(FluidKind::Water),
                    FluidCell::full(FluidKind::Water).level,
                );
            }
        }
        fill(
            world,
            LEFT + 1..LEFT + 9,
            TOP + 8..TOP + 15,
            TileKind::Stone,
        );
        let metal = (LEFT + 1..LEFT + 8, TOP + 5..TOP + 8);
        for y in metal.1.clone() {
            for x in metal.0.clone() {
                let _ = world.terrain_mut().set_tile_from_network(
                    TilePosition { x, y },
                    TileKind::Fluid(FluidKind::LiquidMetal),
                    FluidCell::full(FluidKind::LiquidMetal).level,
                );
            }
        }
        world.wake_fluids(rect(&metal.0, &metal.1));
        // The rig watches from a ledge over the pool's far end.
        fill(
            world,
            LEFT + 20..LEFT + 25,
            TOP + 9..TOP + 10,
            TileKind::Stone,
        );
        place(
            world,
            player,
            tile_centre(LEFT + 22),
            (TOP + 9) as f32 * TILE_SIZE - 11.0,
        );
        reveal(world, player, LEFT - 2..LEFT + 28, TOP - 2..TOP + 18);
    });
    stage.settle(150);
}
