//! Free flight's per-group frames (Wave 13 QA): co-op rigs far apart in free flight each keep
//! their positions in a frame of their own group, so every rig's store position is exact
//! (sub-pixel) wherever it is in the galaxy; groups merge into the store's frame when they
//! meet; it is deterministic and tick-rate safe, and a save in the middle of a split reloads to
//! the same state.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops,
    clippy::float_cmp,
    reason = "positions are compared bit for bit on purpose"
)]

use crate::{
    ControlState, PlayerId, SimulationParams, TickInputs, WorldState,
    attitude::RigAttitude,
    enemy::EnemyDifficulty,
    flight::{
        FlightPlace, RigFlight,
        space::{self, galaxy_sector_of, home_star},
    },
    place::{Place, StoreKey},
    position::{SECTOR_PX, WorldPos},
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::places::SPACE_GROUP_LINK_SECTORS;

const ONE: PlayerId = PlayerId::new(0);
const TWO: PlayerId = PlayerId::new(1);
const THREE: PlayerId = PlayerId::new(2);

fn world_at(hz: u32) -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(0x5_F3A3)
    };
    let mut world = WorldState::new_game_with_settings(ONE, &settings, SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.add_player(TWO);
    world.add_player(THREE);
    for player in [ONE, TWO, THREE] {
        let rig = world.player_mut(player).expect("rig");
        crate::economy::max_out_upgrades(rig);
        rig.fuel = rig.fuel_capacity;
    }
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    for player in [ONE, TWO, THREE] {
        heart.star_drive.insert(player);
    }
    world.set_heart(heart);
    world
}

/// Open space `ring` galactic sectors east of home, `along` tiles further east.
fn open_space(ring: i64, along: f32) -> WorldPos {
    let (hx, hy) = galaxy_sector_of(home_star());
    let corner = space::galaxy_sector_origin((hx + ring, hy));
    corner.offset(space::tiles(6_000.0 + along), space::tiles(6_000.0))
}

/// Puts `player`'s rig straight into free flight at `pos`.
fn launch(world: &mut WorldState, player: PlayerId, pos: WorldPos) {
    let capacity = world.air_capacity_of(player);
    let park = world.space_park().expect("park");
    let rig = world.player_mut(player).expect("rig");
    rig.flight = RigFlight {
        place: FlightPlace::Space,
        pos,
        air: capacity,
        exit_column: 12,
        ..RigFlight::default()
    };
    rig.attitude = RigAttitude {
        orientation: 0.3,
        angular_velocity: 0.0,
    };
    rig.x = park.0;
    rig.y = park.1;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

/// Each player thrusts a little and turns its own way.
fn fly(world: &mut WorldState, ticks: u32) {
    for tick in 0..ticks {
        let mut inputs = TickInputs::default();
        for (index, player) in [ONE, TWO, THREE].into_iter().enumerate() {
            let turn = if (tick / 40 + index as u32).is_multiple_of(2) {
                0.4
            } else {
                -0.4
            };
            inputs = inputs.hold_as(player, ControlState::new(turn, tick % 3 == 0, false));
        }
        let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    }
}

/// Every rig in free flight: its store position, in its own frame, is its exact space
/// position, and small.
fn assert_exact(world: &WorldState) {
    for player in [ONE, TWO, THREE] {
        if world.store_of(player) != StoreKey::Space {
            continue;
        }
        let rig = world.player(player).expect("rig");
        let frame = world.player_frame(player).expect("a frame");
        // A group spans a chain of links; its frame recentres a sector behind its first rig.
        let bound = (2 * SPACE_GROUP_LINK_SECTORS + 2) as f32 * SECTOR_PX;
        assert!(
            rig.x.abs() < bound && rig.y.abs() < bound,
            "{player:?} is measured near its own frame: ({}, {})",
            rig.x,
            rig.y
        );
        let (x, y) = frame.local(rig.flight.pos);
        assert_eq!((rig.x, rig.y), (x, y), "{player:?} exact");
        let (_, pos) = world.rig_world_pos(player).expect("a position");
        let (dx, dy) = pos.delta_f64(rig.flight.pos);
        assert!(
            dx.abs() < 0.01 && dy.abs() < 0.01,
            "{player:?} off by ({dx}, {dy})"
        );
    }
}

#[test]
fn co_op_rigs_far_apart_in_free_flight_each_keep_an_exact_frame() {
    let mut world = world_at(30);
    // Two rigs close together, one three galactic sectors away (millions of tiles).
    launch(&mut world, ONE, open_space(1, 0.0));
    launch(&mut world, TWO, open_space(1, 900.0));
    launch(&mut world, THREE, open_space(4, 0.0));
    fly(&mut world, 3);
    for player in [ONE, TWO, THREE] {
        assert_eq!(world.place_of(player), Place::Space);
    }
    let groups = world.space_groups();
    assert_eq!(groups.len(), 2, "{groups:?}");
    assert_eq!(groups[0].members, vec![ONE, TWO]);
    assert_eq!(groups[0].frame, None, "the first group is the store's");
    assert_eq!(groups[1].members, vec![THREE]);
    assert!(groups[1].frame.is_some(), "the far rig has its own frame");
    assert_exact(&world);
    // They fly on for a while, each exact every tick.
    for _ in 0..20 {
        fly(&mut world, 15);
        assert_exact(&world);
    }
    // The far rig comes to the others: one group again, in the store's frame.
    let near = open_space(1, 1_800.0);
    let rig = world.player_mut(THREE).expect("rig");
    rig.flight.pos = near;
    fly(&mut world, 2);
    let groups = world.space_groups();
    assert_eq!(groups.len(), 1, "{groups:?}");
    assert_exact(&world);
    // And apart again.
    let rig = world.player_mut(THREE).expect("rig");
    rig.flight.pos = open_space(-3, 0.0);
    fly(&mut world, 2);
    assert_eq!(world.space_groups().len(), 2);
    assert_exact(&world);
}

#[test]
fn rigs_within_the_link_share_the_stores_frame() {
    let mut world = world_at(30);
    let reach = 512.0 * SPACE_GROUP_LINK_SECTORS as f32;
    launch(&mut world, ONE, open_space(2, 0.0));
    launch(&mut world, TWO, open_space(2, reach - 600.0));
    launch(&mut world, THREE, open_space(2, 2.0 * reach - 1_200.0));
    fly(&mut world, 2);
    // A chain of near rigs is one group (single linkage).
    assert_eq!(world.space_groups().len(), 1);
    assert_exact(&world);
}

/// Free flight split across the galaxy plays the same at 30 Hz twice (bit for bit), and a
/// save taken mid-split reloads to the same state and plays on alike.
#[test]
fn split_free_flight_is_deterministic_and_survives_a_save() {
    let run = || {
        let mut world = world_at(30);
        launch(&mut world, ONE, open_space(1, 0.0));
        launch(&mut world, TWO, open_space(3, 0.0));
        launch(&mut world, THREE, open_space(-2, 0.0));
        fly(&mut world, 240);
        world
    };
    let first = run();
    let second = run();
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
    assert_eq!(first.space_groups().len(), 3);
    assert_exact(&first);
    let saved = serde_json::to_string(&first).expect("save");
    let mut loaded: WorldState = serde_json::from_str(&saved).expect("load");
    assert_eq!(
        loaded.simulation_state_hash(),
        first.simulation_state_hash()
    );
    assert_exact(&loaded);
    let mut original = first;
    fly(&mut original, 60);
    fly(&mut loaded, 60);
    assert_eq!(
        loaded.simulation_state_hash(),
        original.simulation_state_hash()
    );
    assert_exact(&loaded);
}

/// The same flight at 60 Hz keeps every rig exact too.
#[test]
fn split_free_flight_is_exact_at_60_hz() {
    let mut world = world_at(60);
    launch(&mut world, ONE, open_space(1, 0.0));
    launch(&mut world, TWO, open_space(5, 0.0));
    launch(&mut world, THREE, open_space(5, 300.0));
    for _ in 0..10 {
        fly(&mut world, 30);
        assert_exact(&world);
    }
    assert_eq!(world.space_groups().len(), 2);
}

/// A thing of free flight near the first group (a pilot on foot, a drop, a target) keeps its
/// exact position while a far group ticks in its own frame: the far group's pass must not
/// round it through a frame millions of pixels away.
#[test]
fn a_far_group_leaves_the_first_groups_things_exact() {
    let run = |far: bool| {
        let mut world = world_at(30);
        launch(&mut world, ONE, open_space(1, 0.0));
        launch(&mut world, TWO, open_space(1, 900.0));
        if far {
            launch(&mut world, THREE, open_space(4, 0.0));
        }
        fly(&mut world, 3);
        let rig = world.player(ONE).expect("rig");
        let (x, y) = (rig.x + 3_000.37, rig.y - 1_000.11);
        let space = world
            .places
            .stores
            .get_mut(&StoreKey::Space)
            .expect("the space store");
        let id = space.entities.spawn(
            world.simulation_tick,
            crate::entity::Body::at(x, y),
            crate::entity::EntityState::Target(crate::entity::TargetState {
                health: crate::combat::Health::new(50.0, 0.0, 13.0),
            }),
            None,
        );
        fly(&mut world, 30);
        assert_eq!(world.space_groups().len(), if far { 2 } else { 1 });
        let body = world
            .store_entities(StoreKey::Space)
            .and_then(|entities| entities.get(id))
            .expect("the target")
            .body;
        (body.x, body.y)
    };
    assert_eq!(run(true), run(false), "the same place, bit for bit");
}
