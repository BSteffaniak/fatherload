//! F9 field effects and craft rotation in the world: rigs, drops and pilots under any
//! gravity direction, the time scale across tick rates, mirrored controls, rotation limits
//! and settling, craft-relative thrust, and the classic path where nothing changes.

use crate::{
    ControlState, PlayerId, SimulationParams, TickInputs, WorldState,
    attitude::{AttitudeAssists, BASE_TILT_LIMIT},
    field::{FieldPreset, FieldProfile, FieldZone},
    item::{ItemKind, ItemStack},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);

/// An open air box (tiles 8..=56 x 8..=40) walled in dirt, the rig resting mid-air at
/// tile (32, 24).
fn chamber(hz: u32) -> WorldState {
    let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(hz));
    fill(&mut world, 4..=60, 4..=44, TileKind::Dirt);
    fill(&mut world, 8..=56, 8..=40, TileKind::Air);
    park(&mut world, 32.5, 24.5);
    world
}

fn fill(
    world: &mut WorldState,
    xs: std::ops::RangeInclusive<i32>,
    ys: std::ops::RangeInclusive<i32>,
    kind: TileKind,
) {
    for y in ys {
        for x in xs.clone() {
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
}

fn park(world: &mut WorldState, tile_x: f32, tile_y: f32) {
    let player = world.player_mut(PLAYER).expect("player");
    player.x = tile_x * TILE_SIZE;
    player.y = tile_y * TILE_SIZE;
    player.x_remainder = 0.0;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    player.fuel = player.fuel_capacity;
}

/// Covers the whole chamber with `profile` (a hard edge, so the rig feels all of it).
fn flood(world: &mut WorldState, profile: FieldProfile) {
    let _ = world.place_field_zone(FieldZone::new((0, 0), (64, 48), 0, profile));
}

fn run(world: &mut WorldState, controls: ControlState, ticks: u32) {
    let inputs = TickInputs::held(PLAYER, controls);
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
    }
}

fn idle() -> ControlState {
    ControlState::new(0.0, false, false)
}

fn rig(world: &WorldState) -> &crate::Player {
    world.player(PLAYER).expect("player")
}

#[test]
fn a_rig_falls_sideways_in_a_sideways_field() {
    let mut world = chamber(60);
    flood(&mut world, FieldPreset::GravityShear.profile());
    let (x0, y0) = (rig(&world).x, rig(&world).y);
    run(&mut world, idle(), 30);
    let moved = (rig(&world).x - x0, rig(&world).y - y0);
    assert!(moved.0 > TILE_SIZE, "falls right: {moved:?}");
    assert!(moved.1.abs() < 2.0, "not down: {moved:?}");
    // It lands on the right wall and rests there, "on the ground" of its frame.
    run(&mut world, idle(), 240);
    let rested = rig(&world);
    assert!(rested.x > 55.0 * TILE_SIZE, "{}", rested.x);
    assert!(rested.velocity_x.abs() < 1.0);
}

#[test]
fn a_rig_falls_up_in_an_upside_down_room_and_thrust_pushes_it_harder() {
    let mut world = chamber(60);
    flood(&mut world, FieldPreset::GravityFlip.profile());
    let y0 = rig(&world).y;
    run(&mut world, idle(), 20);
    let rose = y0 - rig(&world).y;
    assert!(rose > 10.0, "falls toward the ceiling: {rose}");
    // An upright rig's thruster pushes along its own up axis: toward the ceiling here.
    let mut thrusting = chamber(60);
    flood(&mut thrusting, FieldPreset::GravityFlip.profile());
    run(&mut thrusting, ControlState::new(0.0, true, false), 20);
    assert!(
        y0 - rig(&thrusting).y > rose,
        "thrust adds to the fall upward"
    );
}

#[test]
fn drops_fall_sideways_in_a_sideways_field() {
    let mut world = chamber(60);
    flood(&mut world, FieldPreset::GravityShear.profile());
    park(&mut world, 12.5, 30.5);
    let id = world
        .spawn_loot(
            32.5 * TILE_SIZE,
            20.5 * TILE_SIZE,
            (0.0, 0.0),
            ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 1),
            None,
        )
        .expect("drop");
    run(&mut world, idle(), 30);
    let body = world.entities().get(id).expect("drop").body;
    assert!(body.x > 33.5 * TILE_SIZE, "drifts right: {}", body.x);
    assert!(
        (body.y - 20.5 * TILE_SIZE).abs() < 2.0,
        "not down: {}",
        body.y
    );
}

#[test]
fn time_scale_slows_a_fall_the_same_at_every_tick_rate() {
    let fall = |hz: u32, slow: bool| {
        let mut world = chamber(hz);
        if slow {
            flood(&mut world, FieldPreset::SlowTime.profile());
        }
        let y0 = rig(&world).y;
        run(&mut world, idle(), hz / 2);
        rig(&world).y - y0
    };
    let normal = fall(60, false);
    let reference = fall(60, true);
    assert!(
        reference < normal * 0.4,
        "half time scale, about a quarter of the drop: {reference} vs {normal}"
    );
    for hz in [20, 30, 120] {
        let other = fall(hz, true);
        assert!(
            (other - reference).abs() < reference * 0.03 + 0.5,
            "{hz} Hz: {other} vs {reference}"
        );
    }
}

#[test]
fn mirrored_controls_steer_the_other_way() {
    let mut world = chamber(60);
    flood(&mut world, FieldPreset::Mirror.profile());
    park(&mut world, 32.5, 39.5);
    run(&mut world, idle(), 20);
    let x0 = rig(&world).x;
    run(&mut world, ControlState::new(1.0, false, false), 30);
    assert!(
        rig(&world).x < x0 - TILE_SIZE,
        "right steers left in a mirror field"
    );
}

#[test]
fn rotation_leans_only_to_the_limit_on_the_home_world_and_settles_level_on_landing() {
    let mut world = chamber(60);
    park(&mut world, 32.5, 12.5);
    let lean_right = ControlState::new(0.0, true, false).with_rotation(1.0);
    run(&mut world, lean_right, 60);
    let leaning = rig(&world).attitude;
    assert!(
        (leaning.orientation - BASE_TILT_LIMIT).abs() < 0.02,
        "{leaning:?}"
    );
    // Thrust is craft-relative: leaning right while thrusting flies right.
    let x0 = rig(&world).x;
    run(&mut world, lean_right, 30);
    assert!(
        rig(&world).x > x0 + 8.0,
        "leaning thrust flies sideways: {} -> {}",
        x0,
        rig(&world).x
    );
    // Released, it keeps leaning (nothing tilts it back automatically) and falls to the
    // floor, where it settles exactly level.
    run(&mut world, idle(), 20);
    assert!(
        rig(&world).attitude.orientation > 0.3,
        "no automatic tilt in flight"
    );
    run(&mut world, idle(), 240);
    assert!(
        rig(&world).attitude.is_level(),
        "{:?}",
        rig(&world).attitude
    );
    // Resting on the floor, the rotation thrusters do nothing.
    run(&mut world, idle().with_rotation(1.0), 30);
    assert!(rig(&world).attitude.is_level());
}

#[test]
fn rigs_turn_all_the_way_round_in_a_gravity_shear() {
    let mut world = chamber(60);
    flood(&mut world, FieldPreset::Weightless.profile());
    let mut turned = 0.0;
    let mut last = rig(&world).attitude.orientation;
    for _ in 0..180 {
        run(&mut world, idle().with_rotation(1.0), 1);
        let now = rig(&world).attitude.orientation;
        turned += crate::field::trig::wrap_angle(now - last);
        last = now;
    }
    assert!(turned > std::f32::consts::TAU, "turned {turned}");
}

#[test]
fn assists_work_only_when_fitted_and_switched_on() {
    let spin_then_release = |fitted: bool, switched: bool| {
        let mut world = chamber(60);
        flood(&mut world, FieldPreset::Weightless.profile());
        if fitted {
            world
                .player_mut(PLAYER)
                .expect("player")
                .hazard_gear
                .attitude = crate::attitude::AttitudeGear::MAXED;
        }
        run(&mut world, idle().with_rotation(1.0), 30);
        let assists = AttitudeAssists {
            gyro: switched,
            horizon: false,
        };
        run(&mut world, idle().with_assists(assists), 60);
        rig(&world).attitude.angular_velocity.abs()
    };
    assert!(
        spin_then_release(false, true) > 0.1,
        "not fitted: spin carries on"
    );
    assert!(
        spin_then_release(true, false) > 0.1,
        "fitted but off by default"
    );
    assert!(
        spin_then_release(true, true) < f32::EPSILON,
        "the gyro stops the spin"
    );
}

#[test]
fn a_rig_cannot_turn_a_corner_into_rock() {
    let mut world = chamber(60);
    flood(&mut world, FieldPreset::Weightless.profile());
    // A rig half a pixel from a wall: turning swings a corner into it.
    let wall = 40.0 * TILE_SIZE;
    fill(&mut world, 40..=56, 8..=40, TileKind::Dirt);
    park(&mut world, 30.0, 24.5);
    world.player_mut(PLAYER).expect("player").x = wall - 11.0;
    run(&mut world, idle().with_rotation(1.0), 60);
    let player = rig(&world);
    assert!(
        player.attitude.orientation.abs() < 0.2,
        "{:?}",
        player.attitude
    );
    assert!(!super::movement::rig_collides(
        world.terrain(),
        player.x,
        player.y,
        player.attitude.orientation
    ));
}

#[test]
fn a_neutral_world_ignores_rotation_input_on_the_ground_bit_for_bit() {
    // Rotation input while resting on the home world changes nothing: the classic path runs
    // and the worlds agree exactly.
    let mut plain = chamber(60);
    let mut turning = chamber(60);
    for world in [&mut plain, &mut turning] {
        park(world, 32.5, 39.5);
        run(world, idle(), 120);
        assert!(world.player(PLAYER).expect("player").velocity_y.abs() < f32::EPSILON);
    }
    run(&mut plain, ControlState::new(0.5, false, false), 120);
    run(
        &mut turning,
        ControlState::new(0.5, false, false).with_rotation(-1.0),
        120,
    );
    assert_eq!(
        plain.simulation_state_hash(),
        turning.simulation_state_hash()
    );
    assert!(plain.field_zones().is_empty());
}

#[test]
fn field_zones_round_trip_through_replication_and_change_the_hash_only_when_present() {
    let mut world = chamber(60);
    let before = world.simulation_state_hash();
    let _ = world.place_field_zone(FieldZone::of_preset(FieldPreset::Prism, (32, 24), (4, 4)));
    assert_ne!(world.simulation_state_hash(), before);
    let section = world.section(crate::SectionKey::Fields).expect("section");
    let mut replica = chamber(60);
    replica.apply_section(crate::SectionKey::Fields, section);
    assert_eq!(replica.field_zones(), world.field_zones());
    world.clear_field_zones();
    assert_eq!(world.simulation_state_hash(), before);
}

#[test]
fn test_mode_places_and_clears_dev_zones() {
    let mut world = chamber(60);
    world.enable_test_mode();
    let mut inputs = TickInputs::default();
    inputs.act_as(
        PLAYER,
        crate::PlayerCommand::TestMode(crate::TestModeAction::PlaceFieldZone(FieldPreset::Dark)),
    );
    let tick = world.simulation_tick();
    let _ = world.simulate_tick(tick, &inputs);
    let here = world.fields_for(PLAYER);
    assert!(here.light < 0.5, "{here:?}");
    let mut clear = TickInputs::default();
    clear.act_as(
        PLAYER,
        crate::PlayerCommand::TestMode(crate::TestModeAction::ClearFieldZones),
    );
    let tick = world.simulation_tick();
    let _ = world.simulate_tick(tick, &clear);
    assert!(world.field_zones().is_empty());
}
