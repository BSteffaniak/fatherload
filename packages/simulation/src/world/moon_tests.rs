//! The Moon in the world (WP-MOON): approach gating and vacuum, the landing, breathing on
//! foot, low gravity, the colony, the facility's puzzle, the Warden, the core, lunar enemies,
//! the uplink, falling home, saves, replication, determinism and precision.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    reason = "test arithmetic on small tile coordinates; scenarios read top to bottom"
)]

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, PlayerScopedCommandOutcome,
    SimulationParams, TickInputs, WorldState,
    base::{BuildCommand, BuildRefusal, Buildable, Facing, ModuleKind, ModuleService},
    enemy::{Damage, DamageKind, DamageSource, EnemyKind},
    environment::MOON_GRAVITY,
    moon::{ColonyOffer, LunarTile, MoonCommand, VacuumBreach},
    terrain::{StrategicResourceKind, TILE_SIZE, TileKind, TilePosition},
    weapon::WeaponKind,
    worldgen::moon::{
        CORE_FOOT_LEDGE_ROWS, CORE_PLINTH_ROWS, CORE_SIDE_PLINTH_ROWS, DOME_RADIUS, MoonLayout,
    },
};

const PLAYER: PlayerId = PlayerId::new(1);

fn world() -> WorldState {
    WorldState::new_game_with_params(PLAYER, SimulationParams::new(60))
}

fn layout(world: &WorldState) -> MoonLayout {
    world
        .moon_layout()
        .expect("the current generator fills the Moon")
}

fn px(tile: i32) -> f32 {
    (tile as f32 + 0.5) * TILE_SIZE
}

/// Parks the rig resting on the floor whose top row is `floor_row`, at column `x`.
fn park_on(world: &mut WorldState, x: i32, floor_row: i32) {
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(x);
    player.x_remainder = 0.0;
    player.y = floor_row as f32 * TILE_SIZE - super::movement::AUTHORITATIVE_PLAYER_RADIUS - 0.5;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

fn moon_gear(world: &mut WorldState) {
    let player = world.player_mut(PLAYER).expect("player");
    player.hazard_gear.pressurized_hull = crate::moon::VACUUM_SEAL_TIER;
    player.hazard_gear.life_support = 1;
    player.hazard_gear.de_icer = 5;
}

fn tick(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let next = world.simulation_tick().next();
    let inputs = TickInputs::held(PLAYER, ControlState::default());
    world
        .simulate_tick(next, &inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn act(
    world: &mut WorldState,
    command: PlayerCommand,
) -> (PlayerScopedCommandOutcome, Vec<GameplayEventKind>) {
    let inputs = TickInputs::default().act(PLAYER, command);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    let outcome = output.action_outcomes.first().expect("an outcome").outcome;
    (
        outcome,
        output.events.into_iter().map(|event| event.kind).collect(),
    )
}

/// A natural stretch of the Moon's top (no site, tube, tree or crater rim) near the middle.
fn open_ground(world: &WorldState) -> (i32, i32) {
    let layout = layout(world);
    let moon = layout.moon;
    (moon.center_x + 300..moon.center_x + moon.half_width - 300)
        .chain((moon.center_x - moon.half_width + 300..moon.center_x - 300).rev())
        .find(|x| {
            let row = layout.surface_row(*x).expect("covered");
            (-12..=12).all(|dx| {
                layout.surface_row(x + dx) == Some(row)
                    && (1..=6).all(|up| {
                        world
                            .terrain()
                            .tile(TilePosition {
                                x: x + dx,
                                y: row - up,
                            })
                            .is_some_and(|t| t.kind == TileKind::Air)
                    })
            }) && layout.tube_at(*x, row + 30).is_none()
                && !layout
                    .colony_area()
                    .expanded(20)
                    .contains(TilePosition { x: *x, y: row })
        })
        .map(|x| (x, layout.surface_row(x).expect("covered")))
        .expect("some flat ground on the Moon")
}

#[test]
fn vacuum_crushes_a_rig_without_the_gear_and_says_what_is_missing() {
    let mut world = world();
    let (x, row) = open_ground(&world);
    // Start just under the Moon's halo but in air... then move into vacuum above the top.
    park_on(&mut world, x, row);
    let before = world.player(PLAYER).expect("player").hull;
    let player = world.player_mut(PLAYER).expect("player");
    player.y -= 2.0 * TILE_SIZE;
    let events = tick(&mut world);
    // Placed straight into vacuum (no edge from air), so no warning yet, but damage.
    let _ = events;
    for _ in 0..60 {
        let _ = tick(&mut world);
    }
    let after = world.player(PLAYER).expect("player").hull;
    assert!(
        before - after > 15.0,
        "a second in vacuum buckles the frame: {before} -> {after}"
    );
    // The warning fires on the edge from air into vacuum.
    assert!(world.in_vacuum(px(x), row as f32 * TILE_SIZE - 40.0));
    assert!(!world.in_vacuum(px(x), 0.0));
    // With the gear: no damage.
    moon_gear(&mut world);
    let player = world.player_mut(PLAYER).expect("player");
    player.hull = player.max_hull();
    let full = player.hull;
    for _ in 0..60 {
        let _ = tick(&mut world);
    }
    assert!((world.player(PLAYER).expect("player").hull - full).abs() < 0.5);
}

#[test]
fn the_breach_warning_fires_climbing_into_orbit() {
    let mut world = world();
    let settings = *world.world_settings();
    let moon = crate::worldgen::sky::moon(&settings).expect("a moon");
    // A column well clear of the Moon, climbing through the orbit floor's vacuum fade.
    let x = moon.center_x - moon.half_width - 600;
    let orbit = crate::worldgen::sky::ORBIT_BOTTOM_ROWS;
    // Just under where the air gives out.
    let edge = (orbit - 200..orbit + 300)
        .find(|altitude| world.in_vacuum(px(x), -(*altitude as f32) * TILE_SIZE))
        .expect("vacuum above the orbit floor");
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(x);
    player.y = -((edge - 2) as f32) * TILE_SIZE;
    player.velocity_y = -1_200.0;
    player.fuel = player.fuel_capacity;
    let mut warned = None;
    for _ in 0..1_200 {
        for event in tick(&mut world) {
            if let GameplayEventKind::MoonVacuumBreach { breach, .. } = event {
                warned = Some(breach);
            }
        }
        let player = world.player_mut(PLAYER).expect("player");
        player.velocity_y = -1_200.0;
        player.hull = player.max_hull();
        if warned.is_some() {
            break;
        }
    }
    assert_eq!(warned, Some(VacuumBreach::Hull));
    assert!(VacuumBreach::Hull.message().contains("Pressurized Hull"));
}

#[test]
fn landing_on_the_moon_plays_once() {
    let mut world = world();
    moon_gear(&mut world);
    let layout = layout(&world);
    let (first, last, row) = layout.pad();
    park_on(&mut world, i32::midpoint(first, last), row);
    let mut landed = 0;
    for _ in 0..30 {
        landed += tick(&mut world)
            .iter()
            .filter(|event| matches!(event, GameplayEventKind::MoonLanded { .. }))
            .count();
    }
    assert_eq!(landed, 1);
    assert!(world.player(PLAYER).expect("player").lunar.landed);
    // Lift off and land again: no second set piece.
    let player = world.player_mut(PLAYER).expect("player");
    player.y -= 3.0 * TILE_SIZE;
    for _ in 0..120 {
        assert!(
            !tick(&mut world)
                .iter()
                .any(|event| matches!(event, GameplayEventKind::MoonLanded { .. }))
        );
    }
}

#[test]
fn gravity_is_low_on_the_moon_for_rigs_and_pilots() {
    let mut world = world();
    let (x, row) = open_ground(&world);
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(x);
    player.y = (row - 30) as f32 * TILE_SIZE;
    player.velocity_y = 0.0;
    for _ in 0..60 {
        let _ = world.step_authoritative_movement(PLAYER, 0.0, false);
    }
    let fall = world.player(PLAYER).expect("player").velocity_y;
    assert!(
        (fall - 440.0 * MOON_GRAVITY).abs() < 25.0,
        "lunar fall {fall}"
    );
    // A pilot's gravity there is the same fraction of its ground gravity.
    let g = crate::environment::environment_at(
        world.world_settings(),
        px(x),
        (row - 1) as f32 * TILE_SIZE,
    )
    .body_gravity(crate::pilot::GRAVITY);
    assert!(g < crate::pilot::GRAVITY * 0.4, "{g}");
    // No air: no drag on a sideways drift, no wind.
    let env = crate::environment::environment_at(
        world.world_settings(),
        px(x),
        (row - 3) as f32 * TILE_SIZE,
    );
    assert!(env.air_density < 0.01 && env.wind_x.abs() < f32::EPSILON);
}

#[test]
fn a_pilot_breathes_in_the_colony_the_facility_and_on_the_air_line_but_not_out_in_the_open() {
    let mut world = world();
    moon_gear(&mut world);
    let layout = layout(&world);
    let (x, row) = open_ground(&world);
    // The rig parked far away: the pilot's oxygen runs down out on the regolith.
    park_on(
        &mut world,
        x + 200,
        layout.surface_row(x + 200).expect("covered"),
    );
    let pilot = world
        .place_pilot(PLAYER, px(x), row as f32 * TILE_SIZE - 0.5)
        .expect("pilot");
    let oxygen = |world: &WorldState| world.pilot_of(PLAYER).expect("on foot").2.oxygen;
    let start = oxygen(&world);
    for _ in 0..120 {
        let _ = tick(&mut world);
    }
    assert!(oxygen(&world) < start - 1.5, "drains in vacuum");
    // Within the rig's life-support air line it refills.
    park_on(&mut world, x + 3, row);
    let low = oxygen(&world);
    for _ in 0..120 {
        let _ = tick(&mut world);
    }
    assert!(oxygen(&world) > low, "the air line feeds the suit");
    // Inside the colony dome it breathes too, with the rig far away.
    park_on(
        &mut world,
        x + 200,
        layout.surface_row(x + 200).expect("covered"),
    );
    let cx = layout.colony.center_x;
    let g = layout.colony.ground_row;
    world.edit_pilot(pilot, |body, state| {
        body.x = px(cx);
        body.y = g as f32 * TILE_SIZE - 0.5;
        state.oxygen = state.max_oxygen * 0.5;
    });
    let before = oxygen(&world);
    for _ in 0..60 {
        let _ = tick(&mut world);
    }
    assert!(oxygen(&world) > before, "pressurized dome");
    // Behind the alien facility's veil the old air holds: the long dungeon on foot is far
    // past any air line, so the pilot breathes in its hall, arena and the core chamber.
    let plan = layout.facility;
    for spot in [plan.hall.center(), plan.arena.center(), layout.core] {
        assert!(layout.facility_air(spot.x, spot.y), "{spot:?}");
        assert!(!world.in_vacuum(px(spot.x), px(spot.y)), "{spot:?}");
    }
    let hall = plan.hall;
    world.edit_pilot(pilot, |body, state| {
        body.x = px(hall.center().x);
        body.y = (hall.max.y + 1) as f32 * TILE_SIZE - 0.5;
        state.oxygen = state.max_oxygen * 0.5;
    });
    let before = oxygen(&world);
    for _ in 0..60 {
        let _ = tick(&mut world);
    }
    assert!(oxygen(&world) > before, "the facility's air");
    // The rock around it is still vacuum.
    assert!(!layout.facility_air(hall.center().x, hall.min.y - 4));
}

#[test]
fn the_colony_trades_only_with_rigs_inside_it() {
    let mut world = world();
    moon_gear(&mut world);
    let layout = layout(&world);
    let (first, last, row) = layout.pad();
    let player = world.player_mut(PLAYER).expect("player");
    player.credits = 100_000;
    player.fuel_capacity = 3_000.0;
    player.fuel = 10.0;
    player.cargo_bay.add(StrategicResourceKind::Helium3, 12);
    player.cargo_bay.add(StrategicResourceKind::MoonIce, 2);
    player.cargo_bay.add(StrategicResourceKind::AlienAlloy, 20);
    // Out of the colony: refused.
    let (x, ground) = open_ground(&world);
    park_on(&mut world, x, ground);
    let (outcome, _) = act(
        &mut world,
        PlayerCommand::Moon(MoonCommand::Colony(ColonyOffer::MeltIce)),
    );
    assert_eq!(outcome, PlayerScopedCommandOutcome::IgnoredUnavailable);
    assert!(!world.at_colony_counter(PLAYER));
    // Beside the dome the rig's use button stays free to climb out; on foot the counter opens
    // anywhere in the colony.
    let (cx, g) = (layout.colony.center_x, layout.colony.ground_row);
    park_on(&mut world, cx + DOME_RADIUS + 2, g);
    assert!(world.player_in_colony(PLAYER));
    assert!(!world.at_colony_counter(PLAYER));
    assert!(world.can_exit_rig(PLAYER) || world.is_on_foot(PLAYER));
    let pilot = world
        .place_pilot(PLAYER, px(cx), g as f32 * TILE_SIZE - 0.5)
        .expect("pilot");
    assert!(world.at_colony_counter(PLAYER));
    let _ = world.entities.despawn(pilot);
    // On the pad: melt ice into fuel, then buy the reactor and a cutter, then sell He-3.
    park_on(&mut world, i32::midpoint(first, last), row);
    assert!(world.at_colony_counter(PLAYER));
    let (outcome, events) = act(
        &mut world,
        PlayerCommand::Moon(MoonCommand::Colony(ColonyOffer::MeltIce)),
    );
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::ColonyServiceUsed {
            offer: ColonyOffer::MeltIce,
            ..
        }
    )));
    let player = world.player(PLAYER).expect("player");
    assert!(player.fuel > 150.0, "{}", player.fuel);
    assert_eq!(player.cargo_bay.material(StrategicResourceKind::MoonIce), 0);
    for offer in [
        ColonyOffer::Reactor,
        ColonyOffer::AlloyDrill,
        ColonyOffer::BuyWeapon(WeaponKind::LunarCutter),
        ColonyOffer::UpgradeWeapon(WeaponKind::LunarCutter),
    ] {
        let (outcome, _) = act(&mut world, PlayerCommand::Moon(MoonCommand::Colony(offer)));
        assert_eq!(outcome, PlayerScopedCommandOutcome::Applied, "{offer:?}");
    }
    let player = world.player(PLAYER).expect("player");
    assert!(player.lunar.reactor && player.lunar.alloy_drill);
    assert_eq!(player.weapon_rack.mk(WeaponKind::LunarCutter), Some(1));
    assert_eq!(player.cargo_bay.material(StrategicResourceKind::Helium3), 2);
    // The mortar wants 4 helium-3: there are 2 left.
    let (outcome, _) = act(
        &mut world,
        PlayerCommand::Moon(MoonCommand::Colony(ColonyOffer::BuyWeapon(
            WeaponKind::LowGMortar,
        ))),
    );
    assert_eq!(outcome, PlayerScopedCommandOutcome::IgnoredUnavailable);
    let credits = world.player(PLAYER).expect("player").credits;
    let (outcome, _) = act(
        &mut world,
        PlayerCommand::Moon(MoonCommand::Colony(ColonyOffer::SellHaul)),
    );
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    let player = world.player(PLAYER).expect("player");
    assert_eq!(player.credits, credits + 2 * crate::moon::HELIUM3_PRICE);
    // The shack never sells or tunes Moon weapons.
    assert_eq!(WeaponKind::LunarCutter.shack_level(), u8::MAX);
}

#[test]
fn the_reactor_and_the_alloy_drill_change_the_rig() {
    let mut world = world();
    let player = world.player_mut(PLAYER).expect("player");
    player.lunar.reactor = true;
    assert!((player.lunar.burn_factor() - crate::moon::REACTOR_BURN_FACTOR).abs() < f32::EPSILON);
    player.lunar.alloy_drill = true;
    assert_eq!(player.lunar.drill_bonus(), 1);
}

fn lit(world: &WorldState, tile: TilePosition) -> bool {
    world.terrain().tile(tile).map(|t| t.kind) == Some(TileKind::Lunar(LunarTile::PylonLit))
}

#[test]
fn the_pylons_open_the_gate_only_in_their_marked_order() {
    let mut world = world();
    let layout = layout(&world);
    let plan = layout.facility;
    let mut output = crate::TickOutput::default();
    // Order of pylon indices by their place in the sequence.
    let mut sequence = [0_usize; 3];
    for (index, place) in plan.order.iter().enumerate() {
        sequence[usize::from(*place)] = index;
    }
    let strike = |world: &mut WorldState, index: usize, output: &mut crate::TickOutput| {
        let tile = plan.pylons[index];
        let kind = world.terrain().tile(tile).expect("a pylon").kind;
        assert!(world.pick_facility(PLAYER, tile, kind, output));
    };
    // The first right, then a wrong one: everything goes dark.
    strike(&mut world, sequence[0], &mut output);
    assert!(lit(&world, plan.pylons[sequence[0]]));
    strike(&mut world, sequence[2], &mut output);
    assert!(plan.pylons.iter().all(|pylon| !lit(&world, *pylon)));
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::FacilityPylon { reset: true, .. }
    )));
    // In order: the gate slides open.
    for index in sequence {
        strike(&mut world, index, &mut output);
    }
    assert!(plan.pylons.iter().all(|pylon| lit(&world, *pylon)));
    assert!(
        plan.gate
            .positions()
            .all(|tile| { world.terrain().tile(tile).map(|t| t.kind) == Some(TileKind::Air) })
    );
    assert!(
        output
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::FacilityGateOpened { .. }))
    );
    // Nothing breaks the facility.
    assert!(world.terrain().is_indestructible_at(plan.pylons[0]));
}

#[test]
fn the_warden_wakes_in_its_arena_and_its_fall_opens_the_way_to_the_core() {
    let mut world = world();
    let layout = layout(&world);
    let plan = layout.facility;
    assert!(world.core_sealed());
    // A pilot walks into the arena: the Warden wakes, shielded.
    let floor = plan.arena.max.y + 1;
    let pilot_x = plan.arena.center().x;
    let _ = world.place_pilot(PLAYER, px(pilot_x), floor as f32 * TILE_SIZE - 0.5);
    let events = tick(&mut world);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::WardenAwakened { .. }))
    );
    let (warden, state) = world
        .enemies()
        .find(|(_, state)| state.kind == EnemyKind::Warden)
        .map(|(entity, state)| (entity.id, *state))
        .expect("awake");
    assert!(state.shield > 0.0);
    // Only one Warden.
    let _ = tick(&mut world);
    assert_eq!(
        world
            .enemies()
            .filter(|(_, s)| s.kind == EnemyKind::Warden)
            .count(),
        1
    );
    // Breaking a conduit with the pick knocks its shield down.
    let mut output = crate::TickOutput::default();
    let conduit = plan.conduits[0];
    assert!(world.pick_facility(
        PLAYER,
        conduit,
        TileKind::Lunar(LunarTile::Conduit),
        &mut output
    ));
    let shield = world
        .enemies()
        .find(|(entity, _)| entity.id == warden)
        .map(|(_, state)| state.shield)
        .expect("still there");
    assert!(shield <= 0.0, "{shield}");
    assert!(
        output
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::WardenStaggered { .. }))
    );
    // Bring it down.
    for _ in 0..40 {
        if world.enemies().all(|(entity, _)| entity.id != warden) {
            break;
        }
        let _ = world.damage_enemy(
            warden,
            Damage::new(400.0, DamageKind::Explosive, DamageSource::Player(PLAYER)),
            &mut output,
        );
    }
    assert!(!world.warden_awake());
    assert!(!world.core_sealed());
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::WardenDefeated { by: Some(PLAYER) }
    )));
    let player = world.player(PLAYER).expect("player");
    assert!(player.lunar.warden_defeated);
    assert!(player.weapon_rack.mk(WeaponKind::VoidLance).is_some());
    // It never wakes again.
    let _ = tick(&mut world);
    assert!(!world.warden_awake());
}

#[test]
fn on_peaceful_the_seal_opens_without_a_fight() {
    let mut world = world();
    world.set_enemy_difficulty(crate::EnemyDifficulty::Peaceful);
    let plan = layout(&world).facility;
    let floor = plan.arena.max.y + 1;
    let _ = world.place_pilot(
        PLAYER,
        px(plan.arena.center().x),
        floor as f32 * TILE_SIZE - 0.5,
    );
    let events = tick(&mut world);
    assert!(!world.warden_awake());
    assert!(!world.core_sealed());
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::WardenDefeated { .. }))
    );
}

#[test]
fn a_pilot_reaches_the_core_once() {
    let mut world = world();
    let core = layout(&world).core;
    let _ = world.place_pilot(
        PLAYER,
        px(core.x + 3),
        (core.y + 4) as f32 * TILE_SIZE - 0.5,
    );
    let mut reached = 0;
    for _ in 0..10 {
        reached += tick(&mut world)
            .iter()
            .filter(|event| matches!(event, GameplayEventKind::MoonCoreReached { .. }))
            .count();
    }
    assert_eq!(reached, 1);
    assert!(world.player(PLAYER).expect("player").lunar.core_reached);
    // The rig cannot drill into the core's shell.
    assert!(world.terrain().is_indestructible_at(TilePosition {
        x: core.x + crate::worldgen::moon::CORE_SHELL_RADIUS - 1,
        y: core.y
    }));
}

#[test]
fn moon_creatures_roam_the_moon_but_never_the_colony() {
    let mut world = world();
    moon_gear(&mut world);
    let layout = layout(&world);
    let (x, row) = open_ground(&world);
    assert_eq!(
        world.lunar_safe_zone(TilePosition { x, y: row - 1 }),
        Some(false)
    );
    assert!(!world.enemy_safe_zone(TilePosition { x, y: row - 1 }));
    let pad = layout.pad();
    assert!(world.enemy_safe_zone(TilePosition {
        x: pad.0,
        y: pad.2 - 1
    }));
    assert_eq!(world.lunar_safe_zone(TilePosition { x: 0, y: 10 }), None);
    park_on(&mut world, x, row);
    for _ in 0..60 * 40 {
        let _ = tick(&mut world);
        let player = world.player_mut(PLAYER).expect("player");
        player.hull = player.max_hull();
        if world.enemies().any(|(_, state)| state.kind.is_lunar()) {
            break;
        }
    }
    assert!(
        world.enemies().any(|(_, state)| state.kind.is_lunar()),
        "something crawls out of the dust"
    );
    // Parked in the colony: none spawn around it.
    let mut quiet = self::world();
    moon_gear(&mut quiet);
    park_on(&mut quiet, i32::midpoint(pad.0, pad.1), pad.2);
    for _ in 0..60 * 20 {
        let _ = tick(&mut quiet);
    }
    assert!(!quiet.enemies().any(|(_, state)| state.kind.is_lunar()));
}

/// Carves a flat 25-wide pocket of air over a stone floor at `(x, floor)`.
fn flatten(world: &mut WorldState, x: i32, floor: i32) {
    for dx in -12..=12 {
        for dy in 1..=8 {
            let _ = world.terrain_mut().set_kind(
                TilePosition {
                    x: x + dx,
                    y: floor - dy,
                },
                TileKind::Air,
            );
        }
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x: x + dx,
                y: floor,
            },
            TileKind::Stone,
        );
    }
}

fn build(world: &mut WorldState, item: Buildable, at: TilePosition) -> Result<(), BuildRefusal> {
    let command = BuildCommand::Place {
        item,
        at,
        facing: Facing::Right,
    };
    world.check_build(PLAYER, &command)?;
    let (outcome, _) = act(world, PlayerCommand::Build(command));
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    Ok(())
}

fn uplink_at(world: &WorldState, x: i32, floor: i32) -> crate::base::StructureId {
    world
        .bases()
        .module_at(TilePosition { x, y: floor - 1 })
        .filter(|module| module.kind == ModuleKind::LunarUplink)
        .map(|module| module.id)
        .expect("an uplink")
}

#[test]
fn lunar_uplinks_beam_rigs_between_the_moon_and_the_ground() {
    let mut world = world();
    moon_gear(&mut world);
    world.player_mut(PLAYER).expect("player").credits = 200_000;
    let (mx, mrow) = open_ground(&world);
    flatten(&mut world, mx, mrow);
    park_on(&mut world, mx, mrow);
    assert!(
        build(
            &mut world,
            Buildable::Module(ModuleKind::Beacon),
            TilePosition { x: mx, y: mrow - 1 }
        )
        .is_ok()
    );
    assert!(
        build(
            &mut world,
            Buildable::Module(ModuleKind::Generator),
            TilePosition {
                x: mx - 4,
                y: mrow - 1
            }
        )
        .is_ok()
    );
    // Not before landing.
    let uplink = TilePosition {
        x: mx + 4,
        y: mrow - 1,
    };
    assert_eq!(
        build(
            &mut world,
            Buildable::Module(ModuleKind::LunarUplink),
            uplink
        ),
        Err(BuildRefusal::NeedsMoonLanding)
    );
    world.player_mut(PLAYER).expect("player").lunar.landed = true;
    assert!(
        build(
            &mut world,
            Buildable::Module(ModuleKind::LunarUplink),
            uplink
        )
        .is_ok()
    );
    let moon_uplink = uplink_at(&world, mx + 4, mrow);
    // Beam home: no ground uplink yet, so the town.
    park_on(&mut world, mx + 5, mrow);
    let (outcome, events) = act(
        &mut world,
        PlayerCommand::Build(BuildCommand::Use {
            structure: moon_uplink,
            service: ModuleService::Beam,
        }),
    );
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::UplinkBeamed { to_moon: false, .. }
    )));
    assert!(
        world.player(PLAYER).expect("player").y > -40.0 * TILE_SIZE,
        "home in town"
    );
    // A ground uplink beams back up.
    let settings = *world.world_settings();
    let gx = (0..400)
        .map(|step| -200 - step * 5)
        .find(|x| {
            let row = world.terrain().surface_row(*x);
            !crate::worldgen::streamed::in_village_area(
                &settings,
                TilePosition { x: *x, y: row },
                40,
            )
        })
        .expect("open country");
    let grow = world.terrain().surface_row(gx);
    flatten(&mut world, gx, grow);
    park_on(&mut world, gx, grow);
    assert_eq!(
        build(
            &mut world,
            Buildable::Module(ModuleKind::Beacon),
            TilePosition { x: gx, y: grow - 1 }
        ),
        Ok(())
    );
    assert_eq!(
        build(
            &mut world,
            Buildable::Module(ModuleKind::Generator),
            TilePosition {
                x: gx - 4,
                y: grow - 1
            }
        ),
        Ok(())
    );
    assert_eq!(
        build(
            &mut world,
            Buildable::Module(ModuleKind::LunarUplink),
            TilePosition {
                x: gx + 4,
                y: grow - 1
            }
        ),
        Ok(())
    );
    let ground_uplink = uplink_at(&world, gx + 4, grow);
    park_on(&mut world, gx + 5, grow);
    let (outcome, events) = act(
        &mut world,
        PlayerCommand::Build(BuildCommand::Use {
            structure: ground_uplink,
            service: ModuleService::Beam,
        }),
    );
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::UplinkBeamed { to_moon: true, .. }))
    );
    let player = world.player(PLAYER).expect("player");
    assert!(world.on_moon(TilePosition {
        x: (player.x / TILE_SIZE) as i32,
        y: (player.y / TILE_SIZE) as i32
    }));
    // And the Moon uplink now beams to the ground one, not the town.
    let destination = world
        .uplink_destination(PLAYER, moon_uplink)
        .expect("a target");
    assert!((destination.0 / TILE_SIZE - (gx + 4) as f32).abs() < 3.0);
}

#[test]
fn falling_off_the_moon_is_a_long_ride_a_sound_rig_survives() {
    let mut world = world();
    moon_gear(&mut world);
    crate::economy::max_out_upgrades(world.player_mut(PLAYER).expect("player"));
    let layout = layout(&world);
    let moon = layout.moon;
    // Drop off the limb, beside the Moon, down a column no island or continent catches.
    let settings = *world.world_settings();
    let x = (0..2_000)
        .map(|step| moon.center_x + moon.half_width + 40 + step * 7)
        .find(|x| {
            let rect = crate::worldgen::TileRect {
                min: TilePosition {
                    x: x - 2,
                    y: moon.top_row,
                },
                max: TilePosition { x: x + 2, y: -1 },
            };
            crate::worldgen::sky::lands_touching(&settings, &rect).is_empty()
                && crate::worldgen::sky::updrafts_touching(&settings, &rect).is_empty()
        })
        .expect("a clear column");
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(x);
    player.y = (moon.top_row as f32) * TILE_SIZE;
    player.velocity_y = 0.0;
    let start = world.player(PLAYER).expect("player").y;
    let mut fastest: f32 = 0.0;
    let mut steps = 0;
    let mut resting = 0;
    // Down past the islands (the wind may carry it onto one) or all the way to the ground.
    while resting < 120 {
        let _ = world.step_authoritative_movement(PLAYER, 0.0, false);
        let player = world.player(PLAYER).expect("player");
        fastest = fastest.max(player.velocity_y);
        steps += 1;
        resting = if player.velocity_y.abs() < f32::EPSILON {
            resting + 1
        } else {
            0
        };
        assert!(steps < 60 * 60 * 30, "lands eventually");
    }
    let player = world.player(PLAYER).expect("player");
    let fallen_rows = (player.y - start) / TILE_SIZE;
    assert!(
        fallen_rows > 4_000.0,
        "a long fall: {fallen_rows} rows in {steps} steps"
    );
    assert!(
        player.hull > player.max_hull() * 0.9,
        "a sound rig shrugs it off: {}",
        player.hull
    );
    assert!(fastest < 2_500.0, "{fastest}");
}

#[test]
fn moon_progress_and_facility_state_survive_saves_and_keyframes() {
    let mut world = world();
    let plan = layout(&world).facility;
    let mut output = crate::TickOutput::default();
    let first = plan
        .order
        .iter()
        .position(|place| *place == 0)
        .expect("a first pylon");
    assert!(world.pick_facility(
        PLAYER,
        plan.pylons[first],
        TileKind::Lunar(LunarTile::Pylon),
        &mut output
    ));
    let player = world.player_mut(PLAYER).expect("player");
    player.lunar.landed = true;
    player.lunar.reactor = true;
    player.hazard_gear.pressurized_hull = 3;
    player.cargo_bay.add(StrategicResourceKind::AlienAlloy, 3);
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert!(lit(&loaded, plan.pylons[first]));
    let player = loaded.player(PLAYER).expect("player");
    assert!(player.lunar.landed && player.lunar.reactor);
    assert_eq!(player.hazard_gear.pressurized_hull, 3);
    assert_eq!(
        player.cargo_bay.material(StrategicResourceKind::AlienAlloy),
        3
    );
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    let keyframe = world.redacted_for(Some(PLAYER));
    let replica: WorldState =
        serde_json::from_str(&serde_json::to_string(&keyframe).expect("keyframe")).expect("back");
    assert!(lit(&replica, plan.pylons[first]));
    assert_eq!(
        replica.visible_state_hash(Some(PLAYER)),
        world.visible_state_hash(Some(PLAYER))
    );
    // Old saves: no lunar progress, no moon gear.
    let progress: crate::moon::LunarProgress = serde_json::from_str("{}").expect("old");
    assert!(progress.is_default());
    let gear: crate::protection::HazardGear =
        serde_json::from_str(r#"{"heat_shield":1}"#).expect("old gear");
    assert_eq!((gear.pressurized_hull, gear.life_support), (0, 0));
}

#[test]
fn lunar_play_is_deterministic() {
    let run = || {
        let mut world = world();
        moon_gear(&mut world);
        let (x, row) = open_ground(&world);
        park_on(&mut world, x, row);
        let _ = world.place_pilot(PLAYER, px(x + 2), row as f32 * TILE_SIZE - 0.5);
        for index in 0..600_u32 {
            let next = world.simulation_tick().next();
            let controls = ControlState::new(
                if (index / 40) % 2 == 0 { 1.0 } else { -1.0 },
                index % 90 < 10,
                false,
            );
            let _ = world.simulate_tick(next, &TickInputs::held(PLAYER, controls));
        }
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn positions_stay_precise_at_the_moon() {
    let world = world();
    let layout = layout(&world);
    // The top of the comms mast is the highest thing anyone stands on.
    let top = (layout.colony.ground_row - 24) as f32 * TILE_SIZE;
    let step = crate::quantize::quantize_simulation_scalar(top + 0.0625) - top;
    assert!(
        step > 0.0 && step <= 0.07,
        "sub-pixel steps resolve: {step}"
    );
    let core = layout.core.y as f32 * TILE_SIZE;
    assert!(crate::quantize::quantize_simulation_scalar(core + 0.0625) > core);
}

fn ledge(world: &WorldState, x: i32, y: i32) -> bool {
    world.terrain().tile(TilePosition { x, y }).map(|t| t.kind)
        == Some(TileKind::Lunar(LunarTile::FacilityWall))
}

/// The open column of the first shaft ledge among `rows` (nearest first).
fn shaft_gap(world: &WorldState, shaft: crate::worldgen::TileRect, rows: Vec<i32>) -> Option<i32> {
    rows.into_iter()
        .find(|row| ledge(world, shaft.center().x, *row))
        .and_then(|row| {
            [shaft.min.x, shaft.max.x]
                .into_iter()
                .find(|x| !ledge(world, *x, row))
        })
}

/// A stop on the way up a shaft: standing on row `stand` (the lowest stop also on anything
/// below it), the pilot walks to column `ground` (`None`: stays put), jumps and drifts toward
/// column `aim`.
struct ClimbStop {
    stand: i32,
    ground: Option<i32>,
    aim: i32,
}

/// Climbs `PLAYER`'s pilot up through `stops` (lowest first) and then ledge by ledge out of the
/// top of `shaft`, with no jetpack: `Err` with where it got stuck.
fn climb_out(
    world: &mut WorldState,
    shaft: crate::worldgen::TileRect,
    stops: &[ClimbStop],
) -> Result<(), String> {
    let mut aim = None;
    for step in 0..60 * 90 {
        let (body, state) = world
            .pilot_of(PLAYER)
            .map(|(_, b, s)| (*b, *s))
            .expect("alive");
        let feet = (body.y / TILE_SIZE).floor() as i32;
        if feet < shaft.min.y {
            return Ok(());
        }
        let stand = feet + 1;
        let stop = stops
            .iter()
            .enumerate()
            .find(|(index, stop)| stop.stand == stand || (*index == 0 && stand > stop.stand))
            .map(|(_, stop)| stop);
        let (toward, jump) = if !state.grounded {
            (aim, false)
        } else if let Some(stop) = stop {
            aim = Some(stop.aim);
            let under = stop
                .ground
                .is_none_or(|column| (px(column) - body.x).abs() < 3.0);
            (stop.ground, under)
        } else {
            let head = ((body.y - 40.0) / TILE_SIZE).floor() as i32;
            let column = shaft_gap(world, shaft, (shaft.min.y..head).rev().collect())
                .unwrap_or_else(|| shaft.center().x);
            aim = Some(shaft.center().x);
            (Some(column), (px(column) - body.x).abs() < 3.0)
        };
        let _ = pilot_tick(world, toward, !state.grounded, false, jump && step % 2 == 0);
    }
    let (_, body, _) = world.pilot_of(PLAYER).expect("alive");
    Err(format!(
        "stuck at {} {}",
        body.x / TILE_SIZE,
        body.y / TILE_SIZE
    ))
}

/// One tick of `PLAYER`'s pilot steering toward column `toward` (`None`: no steering).
fn pilot_tick(
    world: &mut WorldState,
    toward: Option<i32>,
    up: bool,
    down: bool,
    jump: bool,
) -> Vec<GameplayEventKind> {
    let (_, body, _) = world.pilot_of(PLAYER).expect("on foot");
    let steer = toward.map_or(0.0, |column| {
        let offset = px(column) - body.x;
        if offset.abs() < 3.0 {
            0.0
        } else {
            offset.signum() * 0.6
        }
    });
    let next = world.simulation_tick().next();
    let controls = ControlState::new(steer, up, down).with_jump(jump);
    world
        .simulate_tick(next, &TickInputs::held(PLAYER, controls))
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

#[test]
fn a_bare_pilot_climbs_down_to_the_core_unhurt_and_back_up_without_a_jetpack() {
    // The core shaft is some 80 rows deep: a straight drop in low gravity reaches terminal
    // speed and kills a suitless pilot, so its ledges block the middle and the way down zigzags
    // from ledge to ledge; at the bottom the chamber's plinths step back up to the lowest ledge.
    let mut world = world();
    world.set_enemy_difficulty(crate::EnemyDifficulty::Peaceful);
    let layout = layout(&world);
    let shaft = layout.facility.core_shaft;
    let core = layout.core;
    let gap = |world: &WorldState, rows: Vec<i32>| shaft_gap(world, shaft, rows);
    let _ = world.place_pilot(
        PLAYER,
        px(shaft.center().x),
        shaft.min.y as f32 * TILE_SIZE - 0.5,
    );
    let full = world.pilot_of(PLAYER).expect("on foot").2.health;
    let mut reached = false;
    for _ in 0..60 * 60 {
        let (_, body, state) = world.pilot_of(PLAYER).expect("alive");
        assert!(state.health >= full, "hurt at row {}", body.y / TILE_SIZE);
        let stand = (body.y / TILE_SIZE).floor() as i32 + 1;
        let target = if stand <= core.y - CORE_PLINTH_ROWS {
            // Down the shaft through each ledge's gap; off the veil's plinth to the side one.
            gap(&world, (stand..=core.y - CORE_FOOT_LEDGE_ROWS).collect()).unwrap_or(core.x + 3)
        } else {
            core.x
        };
        // Down held: never hang off the ledges on the way.
        let events = pilot_tick(&mut world, Some(target), false, true, false);
        if events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::MoonCoreReached { .. }))
        {
            reached = true;
            break;
        }
    }
    let (_, body, _) = world.pilot_of(PLAYER).expect("alive");
    assert!(
        reached,
        "the pilot reaches the core: stuck at {} {} (core {core:?})",
        body.x / TILE_SIZE,
        body.y / TILE_SIZE
    );
    for _ in 0..60 {
        let _ = pilot_tick(&mut world, None, false, false, false);
    }
    // Back up: the heart -> the side plinth -> the veil's plinth -> ledge after ledge.
    let stops = [
        ClimbStop {
            stand: core.y - 2,
            ground: None,
            aim: core.x + 3,
        },
        ClimbStop {
            stand: core.y - CORE_SIDE_PLINTH_ROWS,
            ground: None,
            aim: core.x,
        },
    ];
    if let Err(stuck) = climb_out(&mut world, shaft, &stops) {
        panic!("the pilot climbs out of the core: {stuck} (core {core:?})");
    }
}

#[test]
fn a_bare_pilot_climbs_from_the_puzzle_hall_back_up_to_the_temple() {
    let mut world = world();
    world.set_enemy_difficulty(crate::EnemyDifficulty::Peaceful);
    let plan = layout(&world).facility;
    let (fx, g, d) = (plan.temple.center_x, plan.temple.ground_row, plan.direction);
    // Down the temple shaft unhurt.
    let shaft = plan.shaft;
    let _ = world.place_pilot(
        PLAYER,
        px(shaft.center().x),
        shaft.min.y as f32 * TILE_SIZE - 0.5,
    );
    let full = world.pilot_of(PLAYER).expect("on foot").2.health;
    for _ in 0..60 * 30 {
        let (_, body, state) = world.pilot_of(PLAYER).expect("alive");
        assert!(state.health >= full, "hurt at row {}", body.y / TILE_SIZE);
        let stand = (body.y / TILE_SIZE).floor() as i32 + 1;
        if stand > plan.hall.max.y {
            break;
        }
        let target = if stand <= shaft.max.y {
            shaft_gap(&world, shaft, (stand..=shaft.max.y).collect()).unwrap_or(fx)
        } else {
            fx + 5 * d
        };
        let _ = pilot_tick(&mut world, Some(target), false, true, false);
    }
    for _ in 0..90 {
        let _ = pilot_tick(&mut world, None, false, false, false);
    }
    let (_, body, state) = world.pilot_of(PLAYER).expect("alive");
    assert!(state.health >= full);
    assert_eq!(
        (body.y / TILE_SIZE).floor() as i32 + 1,
        plan.hall.max.y + 1,
        "on the hall floor"
    );
    // Back up: the floor -> the side plinth -> the plinth under the shaft -> its ledges.
    let stops = [
        ClimbStop {
            stand: plan.hall.max.y + 1,
            ground: Some(fx + 5 * d),
            aim: fx + 3 * d,
        },
        ClimbStop {
            stand: g + crate::worldgen::moon::HALL_SIDE_PLINTH_ROWS,
            ground: None,
            aim: fx,
        },
    ];
    if let Err(stuck) = climb_out(&mut world, shaft, &stops) {
        panic!("the pilot climbs out of the hall: {stuck} (shaft {shaft:?})");
    }
}
