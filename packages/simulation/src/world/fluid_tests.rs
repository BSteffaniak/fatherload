//! Fluids v1 (Part III F9): conservation, settling, determinism and tick-rate safety, flooding a
//! drilled tunnel, reactions, rigs and pilots in fluid, creatures keeping out, and saves.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    reason = "test fixtures convert small tile, tick and pixel numbers freely"
)]

use std::collections::BTreeMap;

use super::WorldImport;
use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TestModeAction,
    TickInputs, WorldState,
    entity::{EntityKind, EntityState},
    fluid::{FLUID_FULL, FluidCell, FluidKind, FluidReaction},
    pilot::PilotMode,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::TileRect,
};

const PLAYER: PlayerId = PlayerId::new(0);
/// The arena's floor row (bedrock).
const FLOOR: i32 = 32;

fn arena_with(params: SimulationParams) -> WorldState {
    let mut world = WorldState::from_import_with_params(&WorldImport::new_game(PLAYER), params);
    fill(&mut world, 8..=40, 8..=31, TileKind::Air);
    fill(&mut world, 8..=40, FLOOR..=FLOOR, TileKind::Bedrock);
    rest(&mut world, 10.5);
    world
}

fn arena() -> WorldState {
    arena_with(SimulationParams::default())
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

fn pour(
    world: &mut WorldState,
    xs: std::ops::RangeInclusive<i32>,
    ys: std::ops::RangeInclusive<i32>,
    kind: FluidKind,
) {
    fill(world, xs.clone(), ys.clone(), TileKind::Fluid(kind));
    world.wake_fluids(rect(xs, ys));
}

fn rect(xs: std::ops::RangeInclusive<i32>, ys: std::ops::RangeInclusive<i32>) -> TileRect {
    TileRect {
        min: TilePosition {
            x: *xs.start(),
            y: *ys.start(),
        },
        max: TilePosition {
            x: *xs.end(),
            y: *ys.end(),
        },
    }
}

fn rest(world: &mut WorldState, tile_x: f32) {
    let player = world.player_mut(PLAYER).expect("player");
    player.x = tile_x * TILE_SIZE;
    player.y = FLOOR as f32 * TILE_SIZE - 11.0;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

fn run(world: &mut WorldState, inputs: &TickInputs, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        events.extend(
            world
                .simulate_tick(tick, inputs)
                .events
                .into_iter()
                .map(|event| event.kind),
        );
    }
    events
}

fn idle() -> TickInputs {
    TickInputs::held(PLAYER, ControlState::default())
}

fn cells(world: &WorldState, area: TileRect) -> BTreeMap<TilePosition, FluidCell> {
    area.positions()
        .filter_map(|at| world.terrain().fluid_at(at).map(|cell| (at, cell)))
        .collect()
}

fn volume(world: &WorldState, area: TileRect, kind: FluidKind) -> u32 {
    cells(world, area)
        .values()
        .filter(|cell| cell.kind == kind)
        .map(|cell| u32::from(cell.level))
        .sum()
}

/// A closed rock box: interior columns 12..=21, rows 20..=31 (the bedrock floor under it).
fn closed_box(world: &mut WorldState) -> TileRect {
    fill(world, 11..=22, 19..=31, TileKind::Stone);
    fill(world, 12..=21, 20..=31, TileKind::Air);
    rect(11..=22, 19..=32)
}

#[test]
fn a_falling_blob_conserves_its_volume_and_settles_flat() {
    let mut world = arena();
    let area = closed_box(&mut world);
    pour(&mut world, 12..=15, 20..=24, FluidKind::Water);
    let start = volume(&world, area, FluidKind::Water);
    assert_eq!(start, 20 * u32::from(FLUID_FULL));
    let mut settled_after = None;
    for second in 0..60 {
        let _ = run(&mut world, &idle(), 60);
        assert_eq!(
            volume(&world, area, FluidKind::Water),
            start,
            "second {second}"
        );
        if world.fluid_schedule().is_idle() {
            settled_after = Some(second);
            break;
        }
    }
    assert!(settled_after.is_some(), "the pool comes to rest");
    // Twenty tiles of water over a ten-wide floor: two full rows (the lower one a little
    // compressed, the surface within a few levels of flat), nothing above.
    for x in 12..=21 {
        for y in [30, 31] {
            let cell = world.terrain().fluid_at(TilePosition { x, y });
            assert!(
                cell.is_some_and(|cell| cell.level >= FLUID_FULL - 15),
                "({x}, {y}) {cell:?}"
            );
        }
        assert!(
            world
                .terrain()
                .fluid_at(TilePosition { x, y: 28 })
                .is_none_or(|cell| cell.level <= 5),
            "column {x} has no third row"
        );
    }
}

#[test]
fn fluid_climbs_a_u_bend_to_its_own_level() {
    let mut world = arena();
    // Two shafts joined at the bottom: left 12..=13, right 18..=19, rows 20..=31, a channel
    // along row 31.
    fill(&mut world, 11..=20, 19..=31, TileKind::Stone);
    fill(&mut world, 12..=13, 20..=31, TileKind::Air);
    fill(&mut world, 18..=19, 20..=31, TileKind::Air);
    fill(&mut world, 12..=19, 31..=31, TileKind::Air);
    pour(&mut world, 12..=13, 22..=30, FluidKind::Water);
    let area = rect(11..=20, 19..=32);
    let start = volume(&world, area, FluidKind::Water);
    let _ = run(&mut world, &idle(), 60 * 90);
    assert_eq!(volume(&world, area, FluidKind::Water), start);
    let top = |x: i32| {
        (20..=31)
            .find(|y| {
                world
                    .terrain()
                    .fluid_at(TilePosition { x, y: *y })
                    .is_some()
            })
            .unwrap_or(32)
    };
    let (left, right) = (top(12), top(18));
    assert!(
        right < 31,
        "fluid rose in the right shaft (top row {right})"
    );
    assert!(
        (left - right).abs() <= 2,
        "levels even out: {left} vs {right}"
    );
}

/// Floods the closed box from a breach at 60 Hz or `hz`, returning the fluid after `seconds`.
fn flood(hz: u32, seconds: u32) -> BTreeMap<TilePosition, FluidCell> {
    let mut world = arena_with(SimulationParams::new(hz));
    let area = closed_box(&mut world);
    pour(&mut world, 12..=14, 20..=27, FluidKind::Water);
    pour(&mut world, 19..=21, 20..=23, FluidKind::Brine);
    let _ = run(&mut world, &idle(), hz * seconds);
    cells(&world, area)
}

#[test]
fn fluids_are_deterministic_and_tick_rate_safe() {
    let at_60 = flood(60, 6);
    assert_eq!(at_60, flood(60, 6), "same inputs, same fluid");
    assert_eq!(at_60, flood(20, 6), "20 Hz matches 60 Hz");
    assert_eq!(at_60, flood(30, 6), "30 Hz matches 60 Hz");
}

#[test]
fn denser_fluid_sinks_through_lighter() {
    let mut world = arena();
    let area = closed_box(&mut world);
    pour(&mut world, 12..=21, 30..=31, FluidKind::Water);
    pour(&mut world, 12..=21, 28..=29, FluidKind::Brine);
    let (water, brine) = (
        volume(&world, area, FluidKind::Water),
        volume(&world, area, FluidKind::Brine),
    );
    let _ = run(&mut world, &idle(), 60 * 10);
    assert_eq!(volume(&world, area, FluidKind::Water), water);
    assert_eq!(volume(&world, area, FluidKind::Brine), brine);
    for x in 12..=21 {
        assert_eq!(
            world
                .terrain()
                .fluid_at(TilePosition { x, y: 31 })
                .map(|cell| cell.kind),
            Some(FluidKind::Brine),
            "brine at the bottom of column {x}"
        );
    }
}

#[test]
fn drilling_into_a_sealed_pocket_floods_the_tunnel() {
    let mut world = arena();
    // A rock mass right of the rig with a sealed water pocket inside; the rig sits at column
    // 10 on the floor, a dirt wall two tiles thick (columns 11 and 12) between it and the
    // pocket's rock rim (column 13).
    fill(&mut world, 11..=24, 20..=31, TileKind::Stone);
    fill(&mut world, 11..=13, 31..=31, TileKind::Dirt);
    fill(
        &mut world,
        14..=23,
        21..=31,
        TileKind::Fluid(FluidKind::Water),
    );
    let area = rect(8..=24, 8..=32);
    let start = volume(&world, area, FluidKind::Water);
    // Sealed and untouched: nothing moves.
    let _ = run(&mut world, &idle(), 120);
    assert!(world.fluid_schedule().is_idle());
    assert_eq!(volume(&world, area, FluidKind::Water), start);
    let drill_right = TickInputs::held(PLAYER, ControlState::new(1.0, false, false));
    let mut breached = false;
    for _ in 0..60 * 20 {
        let events = run(&mut world, &drill_right, 1);
        if events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::FluidBreached { .. }))
        {
            breached = true;
            break;
        }
    }
    assert!(breached, "drilling through the rim breaches the pocket");
    let events = run(&mut world, &idle(), 60 * 8);
    assert_eq!(
        volume(&world, area, FluidKind::Water),
        start,
        "the flood keeps every drop"
    );
    // The tunnel and the arena floor beyond it are wet.
    for x in [9, 10, 11, 12] {
        assert!(
            world
                .terrain()
                .fluid_at(TilePosition { x, y: 31 })
                .is_some(),
            "water reached column {x}"
        );
    }
    assert!(
        events
            .iter()
            .chain(std::iter::empty())
            .all(|event| !matches!(event, GameplayEventKind::FluidReacted { .. }))
    );
}

#[test]
fn water_cools_liquid_metal_into_ore_and_quenches_lava() {
    let mut world = arena();
    let _ = closed_box(&mut world);
    // Metal pooled on the floor at the left, water poured beside it; lava at the right wall.
    pour(&mut world, 12..=13, 29..=31, FluidKind::LiquidMetal);
    fill(&mut world, 21..=21, 31..=31, TileKind::Lava);
    pour(&mut world, 15..=19, 29..=31, FluidKind::Water);
    let events = run(&mut world, &idle(), 60 * 6);
    let reactions: Vec<FluidReaction> = events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::FluidReacted { reaction, .. } => Some(*reaction),
            _ => None,
        })
        .collect();
    assert!(reactions.contains(&FluidReaction::Solidified));
    assert!(reactions.contains(&FluidReaction::Quenched));
    let ore = rect(12..=21, 20..=31).positions().any(|at| {
        world
            .terrain()
            .tile(at)
            .is_some_and(|tile| tile.kind == crate::fluid::COOLED_METAL_ORE)
    });
    assert!(ore, "the metal set into ore");
    assert_eq!(
        world
            .terrain()
            .tile(TilePosition { x: 21, y: 31 })
            .map(|tile| tile.kind),
        Some(TileKind::Cave(crate::cave::CaveTile::Obsidian)),
        "the lava set into obsidian"
    );
}

#[test]
fn acid_eats_soft_ground_and_hulls() {
    let mut world = arena();
    let _ = closed_box(&mut world);
    fill(&mut world, 12..=21, 30..=31, TileKind::Dirt);
    pour(&mut world, 12..=21, 27..=29, FluidKind::Acid);
    let events = run(&mut world, &idle(), 60 * 20);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::FluidReacted {
            reaction: FluidReaction::Dissolved,
            ..
        }
    )));
    let dirt_left = (12..=21)
        .flat_map(|x| [30, 31].map(|y| TilePosition { x, y }))
        .filter(|at| {
            world
                .terrain()
                .tile(*at)
                .is_some_and(|tile| tile.kind == TileKind::Dirt)
        })
        .count();
    assert!(dirt_left < 20, "acid ate into the dirt ({dirt_left} left)");

    // A rig sitting in acid loses hull; one in water does not.
    let hull_after = |kind: FluidKind| {
        let mut world = arena();
        pour(&mut world, 8..=14, 28..=31, kind);
        let start = world.player(PLAYER).expect("rig").hull;
        let _ = run(&mut world, &idle(), 60 * 3);
        start - world.player(PLAYER).expect("rig").hull
    };
    assert!(hull_after(FluidKind::Acid) > 5.0);
    assert!(hull_after(FluidKind::LiquidMetal) > hull_after(FluidKind::Acid));
    assert_eq!(hull_after(FluidKind::Water), 0.0);
}

#[test]
fn rigs_float_sink_and_dive_like_submarines() {
    // Dropped into a deep pool: water lets the rig sink slowly, brine floats it up.
    let depth_after = |kind: FluidKind, controls: ControlState| {
        let mut world = arena();
        pour(&mut world, 8..=40, 14..=31, kind);
        let player = world.player_mut(PLAYER).expect("rig");
        player.y = 20.5 * TILE_SIZE;
        let start = player.y;
        let events = run(&mut world, &TickInputs::held(PLAYER, controls), 60 * 2);
        (world.player(PLAYER).expect("rig").y - start, events)
    };
    let (water, events) = depth_after(FluidKind::Water, ControlState::default());
    assert!(water > 0.0, "a rig sinks in water ({water})");
    let mut dry = arena();
    dry.player_mut(PLAYER).expect("rig").y = 20.5 * TILE_SIZE;
    let _ = run(&mut dry, &idle(), 60 * 2);
    let falling = dry.player(PLAYER).expect("rig").y - 20.5 * TILE_SIZE;
    assert!(
        water < falling,
        "but much slower than through air ({water} vs {falling})"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::RigSurfaced { .. }))
    );
    let (brine, _) = depth_after(FluidKind::Brine, ControlState::default());
    assert!(brine < 0.0, "brine floats a rig up ({brine})");
    let (dive, _) = depth_after(FluidKind::Brine, ControlState::new(0.0, false, true));
    assert!(
        dive > brine + 20.0,
        "diving pushes down even in brine ({dive})"
    );
    // Steering is a slow cruise.
    let mut world = arena();
    pour(&mut world, 8..=40, 14..=31, FluidKind::Water);
    world.player_mut(PLAYER).expect("rig").y = 20.5 * TILE_SIZE;
    let _ = run(
        &mut world,
        &TickInputs::held(PLAYER, ControlState::new(1.0, false, false)),
        60 * 2,
    );
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.velocity_x > 20.0 && rig.velocity_x <= crate::fluid::SUBMARINE_TOP_SPEED);
}

#[test]
fn a_rig_going_under_and_coming_up_is_announced() {
    let mut world = arena();
    pour(&mut world, 8..=40, 26..=31, FluidKind::Water);
    world.player_mut(PLAYER).expect("rig").y = 22.5 * TILE_SIZE;
    let events = run(&mut world, &idle(), 60 * 2);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::RigSubmerged { .. }))
    );
    let events = run(
        &mut world,
        &TickInputs::held(PLAYER, ControlState::new(0.0, true, false)),
        60 * 4,
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::RigSurfaced { .. }))
    );
}

#[test]
fn pilots_swim_in_fluid_and_burn_in_acid() {
    let mut world = arena();
    // Out of the rig first, then flood the arena floor around the pilot.
    let _ = run(&mut world, &idle(), 10);
    let _ = run(
        &mut world,
        &TickInputs::default().act(PLAYER, PlayerCommand::TogglePilot),
        1,
    );
    let _ = run(&mut world, &TickInputs::default(), 20);
    assert!(world.is_on_foot(PLAYER));
    pour(&mut world, 8..=40, 27..=31, FluidKind::Water);
    let _ = run(&mut world, &TickInputs::default(), 30);
    let (_, _, state) = world.pilot_of(PLAYER).expect("pilot");
    assert_eq!(state.mode, PilotMode::Swim, "the pilot swims");
    let health = state.health;
    // Stroking up reaches the surface.
    let (_, body, _) = world.pilot_of(PLAYER).expect("pilot");
    let start_y = body.y;
    let _ = run(
        &mut world,
        &TickInputs::held(PLAYER, ControlState::new(0.0, false, false).with_jump(true)),
        40,
    );
    let (_, body, _) = world.pilot_of(PLAYER).expect("pilot");
    assert!(body.y < start_y, "stroking up rises");
    // Acid burns.
    fill(&mut world, 8..=40, 27..=31, TileKind::Air);
    pour(&mut world, 8..=40, 27..=31, FluidKind::Acid);
    let _ = run(&mut world, &TickInputs::default(), 60);
    let burned = world
        .entities()
        .of_kind(EntityKind::Pilot)
        .find_map(|entity| match &entity.state {
            EntityState::Pilot(state) if state.owner == PLAYER => Some(state.health),
            _ => None,
        });
    assert!(
        burned.is_none_or(|left| left < health),
        "acid hurts the pilot"
    );
}

#[test]
fn creatures_that_cannot_swim_turn_back_at_the_shore() {
    let mut world = arena();
    fill(&mut world, 23..=23, 28..=31, TileKind::Stone);
    pour(&mut world, 24..=40, 29..=31, FluidKind::Water);
    let id = world.spawn_enemy(
        crate::enemy::EnemyKind::CaveBat,
        16.5 * TILE_SIZE,
        30.0 * TILE_SIZE,
    );
    let mut wet = false;
    for _ in 0..60 * 20 {
        let _ = run(&mut world, &idle(), 1);
        if let Some(entity) = world.entities().get(id) {
            let tile = TilePosition {
                x: (entity.body.x / TILE_SIZE).floor() as i32,
                y: (entity.body.y / TILE_SIZE).floor() as i32,
            };
            wet |= world
                .terrain()
                .fluid_at(tile)
                .is_some_and(|cell| cell.level >= FLUID_FULL / 3);
        }
    }
    assert!(!wet, "the bat never entered the water");
}

#[test]
fn a_save_mid_flood_resumes_exactly() {
    let mut world = arena();
    let area = closed_box(&mut world);
    pour(&mut world, 12..=15, 20..=26, FluidKind::Water);
    let _ = run(&mut world, &idle(), 45);
    assert!(!world.fluid_schedule().is_idle(), "still flowing");
    let json = serde_json::to_string(&world).expect("json");
    let mut loaded: WorldState = serde_json::from_str(&json).expect("loads");
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    let _ = run(&mut world, &idle(), 300);
    let _ = run(&mut loaded, &idle(), 300);
    assert_eq!(cells(&loaded, area), cells(&world, area));
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
}

#[test]
fn the_test_pools_command_pours_every_fluid_beside_the_rig() {
    let mut world = arena();
    world.enable_test_mode();
    world.player_mut(PLAYER).expect("rig").y = 20.5 * TILE_SIZE;
    let events = run(
        &mut world,
        &TickInputs::default().act(
            PLAYER,
            PlayerCommand::TestMode(TestModeAction::PourTestPools),
        ),
        1,
    );
    let _ = events;
    let around = rect(9..=48, 14..=30);
    for kind in FluidKind::ALL {
        assert!(volume(&world, around, kind) > 0, "{kind:?} pool");
    }
    // The pools settle within a few seconds and then sit still.
    let _ = run(&mut world, &idle(), 60 * 5);
    assert!(world.fluid_schedule().is_idle());
}

#[test]
fn a_rig_drills_down_through_a_brine_floor_without_bobbing_off() {
    let mut world = arena();
    // A dirt floor under a brine pool; the rig dives to it and drills on down.
    fill(&mut world, 8..=40, 26..=31, TileKind::Dirt);
    pour(&mut world, 8..=40, 20..=25, FluidKind::Brine);
    let player = world.player_mut(PLAYER).expect("rig");
    player.y = 25.0 * TILE_SIZE + 10.0;
    let start = player.y;
    let _ = run(
        &mut world,
        &TickInputs::held(PLAYER, ControlState::new(0.0, false, true)),
        60 * 6,
    );
    let dug = world.player(PLAYER).expect("rig").y - start;
    assert!(
        dug > 2.0 * TILE_SIZE,
        "the rig drilled down through the floor ({dug} px)"
    );
}
