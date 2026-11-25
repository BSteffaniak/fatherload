//! Space flight's scenes (Wave 11, a Deep Descent world after the ending, the Star Drive
//! fitted): leaving the home world over the Moon, a full turn in vacuum, cruising across the
//! home system, a warp's streak, the nav HUD with a ping and a crewmate, docking at a station
//! and its counter, the nav chart, re-entry heat over a world, a landing, and two seats in
//! space at once. Placeholder space art until the space-render track's.

#![allow(
    clippy::missing_const_for_fn,
    reason = "scene staging: plain arithmetic on positions"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    attitude::RigAttitude,
    flight::{
        BodyKind, FlightPlace, RigFlight, SpaceQuery, StarDrive, WarpRun,
        nav::{self, NavTarget, ScanPing},
        space::{self, at_tile},
    },
    position::WorldPos,
    star_chart::chart_at_site,
    terrain::Terrain,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::{Scene, SceneStage};
use crate::shell::ModalScreen;

/// Seed of every flight scene world (the Deep Descent scenes' seed).
const SCENE_SEED: u64 = 0x0D3E_9DE5;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// A Deep Descent world after the ending: space open, a few charts read, ten minutes on the
/// clock (so a warp can be staged part way through).
fn space_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
    let hz = u64::from(world.tick_rate_hz());
    world.set_simulation_tick(fatherload_simulation::SimulationTick::new(600 * hz));
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Peaceful);
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    world.set_heart(heart);
    let mut city = world.ancient_city().clone();
    for (index, depth) in [120_u16, 380, 640, 900].into_iter().enumerate() {
        let x = 40 + i32::try_from(index).unwrap_or(0) * 17;
        city.charts.push(chart_at_site(
            SCENE_SEED,
            x,
            10_300,
            depth,
            world.simulation_tick(),
        ));
    }
    world.set_ancient_city(city);
}

/// `player`'s rig fitted for space (the nav suite when `nav`) and put at `pos` in space,
/// turned `orientation`, moving at `velocity`, its drive `drive`.
fn launch(
    world: &mut WorldState,
    player: PlayerId,
    pos: WorldPos,
    orientation: f32,
    velocity: (f32, f32),
    drive: StarDrive,
    nav: bool,
) {
    let mut heart = world.heart().clone();
    heart.star_drive.insert(player);
    world.set_heart(heart);
    let park = world.space_park().unwrap_or((0.0, 0.0));
    let capacity = fatherload_simulation::flight::air_capacity(
        fatherload_simulation::protection::HazardGear::MAXED.life_support,
    );
    if let Some(rig) = world.player_mut(player) {
        fatherload_simulation::economy::max_out_upgrades(rig);
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        if !nav {
            rig.hazard_gear.flight.nav_suite = 0;
        }
        rig.hull = rig.max_hull();
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.credits = 48_000;
        rig.x = park.0;
        rig.y = park.1;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.attitude = RigAttitude {
            orientation: fatherload_simulation::attitude::quantize_angle(orientation),
            angular_velocity: 0.0,
        };
        rig.flight = RigFlight {
            place: FlightPlace::Space,
            pos,
            velocity_x: velocity.0,
            velocity_y: velocity.1,
            drive,
            heat: 0.0,
            air: capacity * 0.8,
            exit_column: 120,
            alerts: 0,
        };
    }
}

fn home_bodies(world: &WorldState) -> Vec<fatherload_simulation::flight::FlightBody> {
    let space = world.space();
    space
        .system_at((0, 0))
        .map(|home| space.bodies_of(&home))
        .unwrap_or_default()
}

/// The first world of the home system past the home world (a landable one, with air if any).
fn home_planet(world: &WorldState) -> fatherload_simulation::flight::FlightBody {
    let bodies = home_bodies(world);
    bodies
        .iter()
        .copied()
        .filter(|body| body.kind == BodyKind::Planet)
        .max_by(|a, b| a.atmosphere.total_cmp(&b.atmosphere))
        .unwrap_or(bodies[0])
}

fn station(world: &WorldState) -> fatherload_simulation::flight::FlightBody {
    home_bodies(world)
        .into_iter()
        .find(|body| body.kind == BodyKind::Station)
        .expect("the home system has a station")
}

/// A star system east of home (a warp's target).
fn system_east(world: &WorldState) -> fatherload_simulation::flight::SpaceSystem {
    let space = world.space();
    (2..12)
        .find_map(|gx| space.system_at((gx, 0)))
        .expect("a system to the east")
}

/// Just out of the home world's approach sphere, climbing away from it.
fn leaving(stage: &mut SceneStage<'_>, turned: bool) {
    stage.play(1, |world, seats| {
        space_world(world);
        let launch_at = space::home_launch();
        let orientation = if turned { 2.4 } else { 0.0 };
        launch(
            world,
            seats[0].player,
            launch_at,
            orientation,
            (60.0, -380.0),
            StarDrive::Off,
            false,
        );
        if turned && let Some(rig) = world.player_mut(seats[0].player) {
            rig.attitude.angular_velocity = 1.6;
        }
    });
    stage.settle(if turned { 30 } else { 40 });
}

/// Cruising across the home system toward one of its worlds.
fn cruise(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        let target = home_planet(world);
        let home = space::home_centre();
        // A third of the way out from home toward the world.
        let (dx, dy) = home.delta_f64(target.centre);
        let start = space::advance(home, dx * 0.35, dy * 0.35);
        let bearing = nav::bearing(start, target.centre);
        let speed = 260_000.0;
        let (sin, cos) = bearing.sin_cos();
        launch(
            world,
            seats[0].player,
            start,
            bearing,
            (sin * speed, -cos * speed),
            StarDrive::Cruise { speed },
            true,
        );
        set_waypoint(world, seats[0].player, NavTarget::Body(target.body));
    });
    stage.settle(30);
}

/// Mid-warp toward a star to the east.
fn warp(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        let target = system_east(world);
        let start = at_tile(30_000, 30_000);
        let bearing = nav::bearing(start, target.star);
        let arrival = fatherload_simulation::world::flight_warp_arrival(start, target.star);
        let duration = fatherload_simulation::flight::physics::warp_duration_millis(f64::from(
            start.distance(arrival),
        ));
        let now = world.millis_at(world.simulation_tick());
        launch(
            world,
            seats[0].player,
            start,
            bearing,
            (0.0, 0.0),
            StarDrive::Warp(WarpRun {
                origin: start,
                arrival,
                target: target.system,
                start: now.saturating_sub(u64::from(duration) * 2 / 5),
                duration_millis: duration,
            }),
            true,
        );
        set_waypoint(world, seats[0].player, NavTarget::System(target.system));
    });
    stage.settle(12);
}

/// Deep space with the nav suite: a charted star as the waypoint, a ping just sent, a
/// crewmate flying alongside.
fn nav_hud(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        let player = seats[0].player;
        let start = at_tile(470_000, 300_000);
        let target = world
            .known_systems()
            .into_iter()
            .find(|system| !system.is_home())
            .unwrap_or(space::HOME_SYSTEM);
        let star = world
            .space()
            .system(target)
            .map_or(start, |system| system.star);
        launch(
            world,
            player,
            start,
            nav::bearing(start, star) + 0.25,
            (320.0, -140.0),
            StarDrive::Off,
            true,
        );
        set_waypoint(world, player, NavTarget::System(target));
    });
    stage.settle(20);
    stage.play_on(|world| {
        let Some(player) = world.player_ids().next() else {
            return;
        };
        let centre = world
            .rig_flight(player)
            .map_or_else(WorldPos::default, |flight| flight.pos);
        let now = world.millis_at(world.simulation_tick());
        let mut shared = world.flight_shared().clone();
        shared.pings.insert(
            player,
            ScanPing {
                at: now.saturating_sub(450),
                centre,
                range: nav::ping_range(3, false),
                fogged: false,
            },
        );
        world.set_flight_shared(shared);
    });
    stage.settle(2);
}

/// Lined up at the home station's port, slow: the docking guide's checks all green.
fn docking(stage: &mut SceneStage<'_>, docked: bool) {
    stage.play(1, |world, seats| {
        space_world(world);
        let station = station(world);
        let port = station.port().expect("port");
        let (dx, dy) = station.centre.delta(port);
        let length = dx.hypot(dy).max(1.0);
        let pos = port.offset(dx / length * 40.0, dy / length * 40.0);
        launch(
            world,
            seats[0].player,
            pos,
            station.dock_orientation() + 0.06,
            (-dx / length * 12.0, -dy / length * 12.0),
            StarDrive::Off,
            true,
        );
        if docked && let Some(rig) = world.player_mut(seats[0].player) {
            rig.flight.place = FlightPlace::Docked(station.body);
            rig.flight.pos = port;
            rig.flight.velocity_x = 0.0;
            rig.flight.velocity_y = 0.0;
            rig.fuel = rig.fuel_capacity * 0.35;
            rig.hull = rig.max_hull() * 0.6;
        }
    });
    stage.settle(10);
    if docked {
        let seat = stage.seat(0);
        seat.modal = Some(ModalScreen::Station);
        seat.selected_menu_item = 2;
        stage.settle(2);
    }
}

/// The nav chart open in space.
fn nav_chart(stage: &mut SceneStage<'_>) {
    nav_hud(stage);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::NavChart);
    seat.selected_menu_item = 1;
    stage.settle(2);
}

/// Diving into a world's air too steep and fast: the hull heats, the plasma glows.
fn reentry(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        let planet = home_planet(world);
        let depth = planet.atmosphere.max(space::tiles(300.0));
        // A third of the way down the air, falling straight in, nose first.
        let pos = planet.centre.offset(0.0, -(planet.radius + depth * 0.55));
        launch(
            world,
            seats[0].player,
            pos,
            std::f32::consts::PI * 0.92,
            (120.0, 2_600.0),
            StarDrive::Off,
            true,
        );
        if let Some(rig) = world.player_mut(seats[0].player) {
            rig.flight.place = FlightPlace::Approach(planet.body);
            rig.flight.heat = 1.1;
        }
    });
    stage.settle(8);
}

/// Landed on a world: the view at one to one, the curve of the ground.
fn landed(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        space_world(world);
        let planet = home_planet(world);
        let pos = planet.centre.offset(
            0.0,
            -(planet.radius + fatherload_simulation::flight::RIG_CONTACT),
        );
        launch(
            world,
            seats[0].player,
            pos,
            0.0,
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
        if let Some(rig) = world.player_mut(seats[0].player) {
            rig.flight.place = FlightPlace::Landed(planet.body);
        }
    });
    stage.settle(10);
}

/// Two seats in space: one cruising, one warping (per-seat cameras and HUDs).
fn split(stage: &mut SceneStage<'_>) {
    stage.play(2, |world, seats| {
        space_world(world);
        let home = space::home_launch();
        launch(
            world,
            seats[0].player,
            home,
            0.4,
            (300.0, -900.0),
            StarDrive::Off,
            true,
        );
        let target = system_east(world);
        let start = at_tile(30_000, 30_000);
        let arrival = fatherload_simulation::world::flight_warp_arrival(start, target.star);
        let duration = fatherload_simulation::flight::physics::warp_duration_millis(f64::from(
            start.distance(arrival),
        ));
        let now = world.millis_at(world.simulation_tick());
        launch(
            world,
            seats[1].player,
            start,
            nav::bearing(start, target.star),
            (0.0, 0.0),
            StarDrive::Warp(WarpRun {
                origin: start,
                arrival,
                target: target.system,
                start: now.saturating_sub(u64::from(duration) / 2),
                duration_millis: duration,
            }),
            true,
        );
        set_waypoint(world, seats[1].player, NavTarget::System(target.system));
    });
    stage.settle(12);
}

fn set_waypoint(world: &mut WorldState, player: PlayerId, target: NavTarget) {
    let mut shared = world.flight_shared().clone();
    shared.waypoints.insert(player, target);
    world.set_flight_shared(shared);
}

pub const FLIGHT_SCENES: &[Scene] = &[
    Scene {
        name: "space_leaving_home",
        summary: "Past the Moon with the Star Drive: open space, the home world and its Moon below, the flight HUD (no nav suite yet)",
        setup: |stage| leaving(stage, false),
    },
    Scene {
        name: "space_full_turn",
        summary: "A full turn in vacuum: the rig spinning on its own momentum, the camera locked to it, the HUD upright",
        setup: |stage| leaving(stage, true),
    },
    Scene {
        name: "space_cruise",
        summary: "Cruising across the home system toward a world: streaking stars, the drive's glow, the nav panel's ETA",
        setup: cruise,
    },
    Scene {
        name: "space_warp",
        summary: "Mid-warp to a star to the east: the tunnel of light, the target star's tint, the warp's progress",
        setup: warp,
    },
    Scene {
        name: "space_nav_hud",
        summary: "Deep space with the nav suite: a charted star as the waypoint, a scanner ping's ring, a crewmate alongside and one far off",
        setup: nav_hud,
    },
    Scene {
        name: "space_nav_chart",
        summary: "The nav chart: known star systems, nearby bodies, the warp and the beacon",
        setup: nav_chart,
    },
    Scene {
        name: "space_docking",
        summary: "Lined up at the home station's port: the docking guide's range, speed and alignment checks",
        setup: |stage| docking(stage, false),
    },
    Scene {
        name: "space_station",
        summary: "Docked: the station's counter (fuel, repairs, flight gear, save, undock)",
        setup: |stage| docking(stage, true),
    },
    Scene {
        name: "space_reentry",
        summary: "Diving into a world's air too steep and fast: re-entry plasma and the heat warning",
        setup: reentry,
    },
    Scene {
        name: "space_landed",
        summary: "Landed on a world of the home system: the view at one to one over its curved ground",
        setup: landed,
    },
    Scene {
        name: "space_split_screen",
        summary: "Two seats in space: one climbing away from home, one in warp, each with its own camera and HUD",
        setup: split,
    },
];
