//! Planet ground scenes (Wave 11 polar track): a round body's unrolled strip drawn flat by the
//! ordinary renderer (the space-render track bends it into a ring), so the generated ground
//! can be checked: a barren moon's surface and hills, the strata and a halving ring deep down,
//! the core chamber, a rocky world and an ice world, and a craft tipped over after a bad
//! landing.

use fatherload_simulation::{
    PlayerId, WorldState,
    planet::{BodyGround, PlanetParams, PlanetThemeId},
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{Scene, SceneStage, reveal};

/// Replaces the staged world with a planet world for `seats`' first player (one seat).
fn on_planet(world: &mut WorldState, player: PlayerId, params: PlanetParams) {
    let mut planet = WorldState::new_planet_world(
        player,
        params,
        fatherload_simulation::SimulationParams::new(world.timing().rate.hz()),
    );
    planet.enable_test_mode();
    *world = planet;
}

#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn px(tile: i32) -> f32 {
    (tile as f32 + 0.5) * TILE_SIZE
}

/// The rig at column `x`, its middle `rows_up` rows above the top of row `floor`, turned to
/// `orientation`, with the area around revealed.
#[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
fn rig_at(world: &mut WorldState, player: PlayerId, x: i32, floor: i32, orientation: f32) {
    if let Some(rig) = world.player_mut(player) {
        rig.x = px(x);
        rig.y = (floor as f32).mul_add(TILE_SIZE, -11.0);
        rig.x_remainder = 0.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.attitude.orientation = orientation;
        rig.attitude.angular_velocity = 0.0;
    }
    reveal(world, player, x - 60..x + 60, floor - 40..floor + 30);
    world.stream_terrain_near_players();
}

/// Digs a shaft two tiles wide from the surface at column `x` down to row `bottom`.
fn shaft(world: &mut WorldState, x: i32, bottom: i32) {
    let top = world.terrain().surface_row(x);
    for y in top..=bottom {
        for dx in 0..2 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x: x + dx, y }, TileKind::Air);
        }
    }
}

const fn moon() -> PlanetParams {
    PlanetParams::new(0x4D00_4E00, 400, PlanetThemeId::BARREN)
}

fn surface_scene(stage: &mut SceneStage<'_>, params: PlanetParams, column: i32) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        on_planet(world, player, params);
        let floor = world.terrain().surface_row(column);
        rig_at(world, player, column, floor, 0.0);
    });
    stage.settle(20);
}

pub const PLANET_SCENES: &[Scene] = &[
    Scene {
        name: "planet_moon_surface",
        summary: "A barren moon of radius 400, unrolled: regolith crust over dust plains, low hills, the black sky (drawn flat; the polar renderer bends it)",
        setup: |stage| surface_scene(stage, moon(), 140),
    },
    Scene {
        name: "planet_moon_halving_ring",
        summary: "Deep in the moon: a shaft through the basalt mantle where the ring's columns halve, every dug ring tile open whole",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                on_planet(world, player, moon());
                let planet = *world.planet().expect("planet");
                let boundary = planet.grid().halving_rows()[1];
                shaft(world, 300, boundary + 6);
                rig_at(world, player, 300, boundary + 7, 0.0);
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "planet_moon_core_chamber",
        summary: "The moon's core chamber: a ring-shaped hall over the indestructible core, pillars holding the roof, platinum on the floor",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                on_planet(world, player, moon());
                let planet = *world.planet().expect("planet");
                shaft(world, 500, planet.core_chamber_top());
                rig_at(world, player, 500, planet.core_floor_row() - 1, 0.0);
            });
            stage.settle(30);
        },
    },
    Scene {
        name: "planet_rocky_surface",
        summary: "A rocky world of radius 1,500: sand crust, red soil and stone, taller hills",
        setup: |stage| {
            surface_scene(
                stage,
                PlanetParams::new(0x00C0_FFEE, 1_500, PlanetThemeId::ROCKY),
                700,
            );
        },
    },
    Scene {
        name: "planet_ice_surface",
        summary: "An ice world of radius 900: snow over a glacier shelf of moon ice",
        setup: |stage| {
            surface_scene(
                stage,
                PlanetParams::new(0x0000_1CE0, 900, PlanetThemeId::ICE),
                333,
            );
        },
    },
    Scene {
        name: "planet_tipped_over",
        summary: "A bad landing on the moon: the rig came down at 75 degrees, tipped over and lies on its side (thrusters toward upright right it)",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                on_planet(world, player, moon());
                let planet = *world.planet().expect("planet");
                // Flat ground to land on, and no landing legs.
                let column = (40..2_000)
                    .find(|column| {
                        let row = planet.surface_row(*column);
                        (-3..=3).all(|dx| planet.surface_row(column + dx) == row)
                    })
                    .unwrap_or(140);
                let floor = planet.surface_row(column);
                rig_at(world, player, column, floor - 6, 75.0_f32.to_radians());
                if let Some(rig) = world.player_mut(player) {
                    rig.hazard_gear.attitude.landing_legs = 0;
                }
            });
            stage.settle(220);
        },
    },
];
