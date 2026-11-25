//! Space combat's scenes (Wave 12 hostiles; a Deep Descent world after the ending, the rig in
//! open space a few sectors from home): each faction (corsairs hailing, the swarm flocking,
//! a machine formation), a dogfight, each boss winding up a telegraphed attack, salvage after
//! a fight, a ground raid on a body, and the touch combat cluster.

#![allow(
    clippy::missing_const_for_fn,
    clippy::cast_precision_loss,
    reason = "scene staging: plain arithmetic on positions"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    attitude::RigAttitude,
    flight::{
        FlightPlace, RigFlight, StarDrive,
        space::{self, galaxy_sector_of, home_star},
    },
    hostiles::{
        EncounterKind, EncounterState, Faction, ShipClass, ShipGear, ShipMode,
        combat::{bearing, from_ship_frame},
    },
    position::WorldPos,
    terrain::Terrain,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::{Scene, SceneStage, all_scenes};
use crate::input::{PlayerInput, devices::InputDevice, touch::TouchView};

/// Seed of every hostile scene world (the Deep Descent scenes' seed).
const SCENE_SEED: u64 = 0x0D3E_9DE5;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// Open space `ring` galactic sectors east of home (a sector's corner, clear of systems).
fn open_space(ring: i64) -> WorldPos {
    let (hx, hy) = galaxy_sector_of(home_star());
    space::galaxy_sector_origin((hx + ring, hy))
        .offset(space::tiles(6_000.0), space::tiles(6_000.0))
}

/// A Deep Descent world after the ending with `player`'s rig fitted for space (good ship gear)
/// in open space `ring` sectors out, turned `orientation`.
fn arm(world: &mut WorldState, player: PlayerId, ring: i64, orientation: f32) {
    *world.terrain_mut() = Terrain::generate(&settings());
    let hz = u64::from(world.tick_rate_hz());
    world.set_simulation_tick(fatherload_simulation::SimulationTick::new(600 * hz));
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Peaceful);
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    heart.star_drive.insert(player);
    world.set_heart(heart);
    let park = world.space_park().unwrap_or((0.0, 0.0));
    let capacity = fatherload_simulation::flight::air_capacity(3);
    if let Some(rig) = world.player_mut(player) {
        fatherload_simulation::economy::max_out_upgrades(rig);
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.hazard_gear.arms = ShipGear {
            cannon: 2,
            turret: 1,
            missiles: 2,
            shield: 2,
        };
        rig.hull = rig.max_hull() * 0.85;
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.credits = 6_400;
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
            pos: open_space(ring),
            velocity_x: 0.0,
            velocity_y: 0.0,
            drive: StarDrive::Off,
            heat: 0.0,
            air: capacity * 0.8,
            exit_column: 120,
            alerts: 0,
            ..RigFlight::default()
        };
    }
}

/// Brings `faction`'s ships of the running encounter in round the rig: each at `range` px on
/// a spread of bearings round the nose, facing the rig, the encounter `state`.
fn gather(world: &mut WorldState, range: f32, spread: f32, state: EncounterState) {
    let Some(player) = world.player_ids().next() else {
        return;
    };
    let Some(rig) = world.rig_flight(player).map(|flight| flight.pos) else {
        return;
    };
    let heading = world
        .player(player)
        .map_or(0.0, |player| player.attitude.orientation);
    let mut hostiles = world.hostiles().clone();
    let count = hostiles.ships.len().max(1) as f32;
    for (index, ship) in hostiles.ships.iter_mut().enumerate() {
        let share = if count > 1.0 {
            index as f32 / (count - 1.0) - 0.5
        } else {
            0.0
        };
        let angle = heading + share * spread;
        let reach = ((index % 3) as f32).mul_add(60.0, range);
        let (sin, cos) = angle.sin_cos();
        ship.pos = rig.offset(sin * reach, -cos * reach);
        let (dx, dy) = ship.pos.delta(rig);
        ship.angle = bearing(dx, dy);
        ship.vx = 0.0;
        ship.vy = 0.0;
    }
    for encounter in &mut hostiles.encounters {
        encounter.state = state;
    }
    if state == EncounterState::Fighting {
        for ship in &mut hostiles.ships {
            ship.mode = ShipMode::Attacking;
        }
    }
    world.set_hostiles(hostiles);
}

fn encounter(stage: &mut SceneStage<'_>, faction: Faction, ships: &'static [ShipClass]) {
    stage.play(1, |world, seats| {
        arm(world, seats[0].player, 3, 0.3);
        let _ = world.stage_encounter(seats[0].player, faction, EncounterKind::Roaming, ships, 3);
    });
}

/// Corsairs closing in and hailing: their demand on the HUD.
fn pirates(stage: &mut SceneStage<'_>) {
    encounter(
        stage,
        Faction::Pirates,
        &[ShipClass::Cutter, ShipClass::Marauder, ShipClass::Cutter],
    );
    stage.play_on(|world| gather(world, 700.0, 1.4, EncounterState::Closing));
    stage.settle(150);
}

/// A flock of the Bloom round the rig: mites wheeling, stingers holding off.
fn swarm(stage: &mut SceneStage<'_>) {
    encounter(
        stage,
        Faction::Swarm,
        &[
            ShipClass::Mite,
            ShipClass::Mite,
            ShipClass::Mite,
            ShipClass::Stinger,
            ShipClass::Mite,
            ShipClass::Mite,
            ShipClass::Mite,
            ShipClass::Stinger,
            ShipClass::Mite,
            ShipClass::Mite,
        ],
    );
    stage.play_on(|world| gather(world, 600.0, 1.8, EncounterState::Fighting));
    stage.settle(110);
}

/// A Concord formation: a lancer leading drones in a V, its lance charging.
fn machines(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        arm(world, seats[0].player, 5, 0.0);
        let _ = world.stage_encounter(
            seats[0].player,
            Faction::Machine,
            EncounterKind::Roaming,
            &[
                ShipClass::Lancer,
                ShipClass::Drone,
                ShipClass::Drone,
                ShipClass::Drone,
                ShipClass::Drone,
            ],
            5,
        );
    });
    stage.play_on(|world| {
        // The formation lined up ahead, in its slots, nose on the rig.
        let Some(player) = world.player_ids().next() else {
            return;
        };
        let Some(rig) = world.rig_flight(player).map(|flight| flight.pos) else {
            return;
        };
        let mut hostiles = world.hostiles().clone();
        let lead = rig.offset(120.0, -640.0);
        let (dx, dy) = lead.delta(rig);
        let face = bearing(dx, dy);
        for ship in &mut hostiles.ships {
            ship.angle = face;
            ship.mode = ShipMode::Attacking;
            ship.pos = if ship.class == ShipClass::Lancer {
                lead
            } else {
                let (sx, sy) = fatherload_simulation::hostiles::ai::slot_offset(ship.slot);
                let (ox, oy) = from_ship_frame(face, sx, sy);
                lead.offset(ox, oy)
            };
        }
        for encounter in &mut hostiles.encounters {
            encounter.state = EncounterState::Fighting;
        }
        world.set_hostiles(hostiles);
    });
    stage.settle(70);
}

/// A dogfight: corsairs strafing, the rig turned onto one with its guns blazing.
fn dogfight(stage: &mut SceneStage<'_>) {
    encounter(
        stage,
        Faction::Pirates,
        &[
            ShipClass::Cutter,
            ShipClass::Cutter,
            ShipClass::Marauder,
            ShipClass::Cutter,
        ],
    );
    stage.play_on(|world| gather(world, 380.0, 2.2, EncounterState::Fighting));
    stage.hold(
        50,
        PlayerInput {
            fire: true,
            ..PlayerInput::default()
        },
    );
}

/// `faction`'s boss close by, a telegraphed attack winding up.
fn boss(stage: &mut SceneStage<'_>, faction: Faction, frames: usize) {
    stage.play(1, |world, seats| {
        arm(world, seats[0].player, 5, 0.0);
        let _ = world.stage_encounter(
            seats[0].player,
            faction,
            EncounterKind::Boss,
            &[faction.boss()],
            5,
        );
    });
    stage.play_on(|world| {
        gather(world, 430.0, 0.0, EncounterState::Fighting);
        let mut hostiles = world.hostiles().clone();
        if let Some(boss) = hostiles.ships.iter_mut().find(|ship| ship.class.is_boss()) {
            // The Pirate King shows its flank; the others face the rig.
            if faction == Faction::Pirates {
                boss.angle += std::f32::consts::FRAC_PI_2;
            }
            boss.next_attack = 0;
            boss.hull = boss.class.stats().hull * 0.72;
        }
        world.set_hostiles(hostiles);
    });
    stage.settle(frames);
}

/// After a fight: wrecks adrift, the boss's tech glowing in its wreck.
fn salvage(stage: &mut SceneStage<'_>) {
    encounter(
        stage,
        Faction::Pirates,
        &[ShipClass::Cutter, ShipClass::Marauder, ShipClass::Cutter],
    );
    stage.play_on(|world| {
        gather(world, 260.0, 2.4, EncounterState::Fighting);
        let mut hostiles = world.hostiles().clone();
        for ship in &mut hostiles.ships {
            ship.hull = 0.0;
        }
        world.set_hostiles(hostiles);
    });
    stage.settle(30);
}

/// A raid on a body: corsairs set down by the rig on the charted small moon's ground.
fn raid(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        arm(world, seats[0].player, 0, 0.0);
        world.enable_test_mode();
        let Some((chart, _, body)) =
            fatherload_simulation::flight::autopilot::chart_with_a_small_moon(SCENE_SEED)
        else {
            return;
        };
        world.record_star_chart(chart);
        let Some(ground) = world.body_ground(body.body) else {
            return;
        };
        let place = fatherload_simulation::place::Place::OnBody(
            fatherload_simulation::place::BodyId::Space(ground.id),
        );
        let planet = fatherload_simulation::planet::PlanetGround::new(ground.params);
        let column = 300;
        let surface = fatherload_simulation::planet::BodyGround::surface_row(&planet, column);
        let at = fatherload_simulation::world::strip_position(
            f64::from(column) + 0.5,
            f64::from(surface - 1),
        );
        if let Some(rig) = world.player_mut(seats[0].player) {
            rig.flight.pos = body.centre;
        }
        let _ = world.move_to_place(seats[0].player, place, at);
        if let Some(rig) = world.player_mut(seats[0].player) {
            rig.flight.place = FlightPlace::Landed(body.body);
            rig.attitude = RigAttitude::LEVEL;
        }
    });
    stage.settle(60);
    stage.hold(1, PlayerInput::default());
    stage.play_on(|world| {
        let Some(player) = world.player_ids().next() else {
            return;
        };
        let _ = world.stage_raid(player, Faction::Pirates);
    });
    stage.settle(150);
}

/// The touch combat cluster during the dogfight.
fn touch_combat(stage: &mut SceneStage<'_>) {
    if let Some(scene) = all_scenes().find(|scene| scene.name == "space_pirates_hail") {
        (scene.setup)(stage);
    }
    stage.use_devices(&[InputDevice::Touch]);
    stage.touch_view = TouchView::default();
}

pub const HOSTILE_SCENES: &[Scene] = &[
    Scene {
        name: "space_pirates_hail",
        summary: "Rustwake corsairs close in and hail: their demand on the HUD, pay or fight",
        setup: pirates,
    },
    Scene {
        name: "space_swarm_flock",
        summary: "A flock of the Bloom wheeling round the rig: mites and stingers keeping apart and together",
        setup: swarm,
    },
    Scene {
        name: "space_machine_formation",
        summary: "A Concord formation: a lancer leading drones in a V, its lance's line telegraphed",
        setup: machines,
    },
    Scene {
        name: "space_dogfight",
        summary: "A dogfight: corsairs strafing, the rig's cannon and turret firing, hits sparking",
        setup: dogfight,
    },
    Scene {
        name: "space_boss_pirate_king",
        summary: "The Gilded Maw, the Pirate King's flagship, its broadside telegraphed down its flank",
        setup: |stage| boss(stage, Faction::Pirates, 75),
    },
    Scene {
        name: "space_boss_swarm_queen",
        summary: "The Swarm Queen swelling before a spore ring (the gap marked), her bar on the HUD",
        setup: |stage| boss(stage, Faction::Swarm, 70),
    },
    Scene {
        name: "space_boss_arbiter",
        summary: "The Arbiter, the Concord's capital ship: its turrets' volley paths, then its lance",
        setup: |stage| boss(stage, Faction::Machine, 66),
    },
    Scene {
        name: "space_salvage",
        summary: "After a fight: corsair wrecks adrift with their salvage glints",
        setup: salvage,
    },
    Scene {
        name: "space_ground_raid",
        summary: "Corsair raiders dropped on the charted small moon's ground beside the rig",
        setup: raid,
    },
    Scene {
        name: "touch_space_combat",
        summary: "Touch controls in a fight (Wave 12): FIRE, MSL, ESCAPE and PAY",
        setup: touch_combat,
    },
];
