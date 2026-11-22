//! The authored Crust in the tick (Wave 8): mud and slate grip, mine-cart rides (junctions,
//! ramps, tick rates), timber sets that collapse when undercut, slate slides, aquifers that
//! flood, the lava tubes' heat, strongboxes, the Deep Claim gate and the Sinkhole, won by a bot
//! and lost by one.

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WorldState,
    crust::{self, CrustTile, DRILL_II_LEVEL},
    descent::Stratum,
    enemy::EnemyDifficulty,
    event::GameplayEvent,
    fluid::FluidKind,
    position::WorldPos,
    terrain::{SurfaceMaterial, TileKind, TilePosition},
    worldgen::{GeneratorVersion, WorldSettings, descent::layout},
};

const PLAYER: PlayerId = PlayerId::new(0);

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

/// A V11 world with a peaceful crew, its rig at `tile` (resting on the floor beneath).
fn world_at(seed: u64, hz: u32, tile: TilePosition) -> WorldState {
    let settings = v11(seed);
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(tile.x), i64::from(tile.y)),
    );
    world.stream_terrain_near_players();
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hazard_gear = crate::protection::HazardGear::MAXED;
        rig.drill_strength = 6;
        rig.radiator_level = 15;
        rig.fuel = rig.fuel_capacity;
    }
    world
}

/// A row in the middle of `stratum` in seed `seed`'s world.
fn row_in(seed: u64, stratum: Stratum) -> i32 {
    let (top, bottom) = layout(&v11(seed)).rows(stratum);
    i32::midpoint(top, bottom)
}

/// Fills the box `(x0..=x1, y0..=y1)` with `kind`.
fn fill(world: &mut WorldState, (x0, y0): (i32, i32), (x1, y1): (i32, i32), kind: TileKind) {
    for y in y0..=y1 {
        for x in x0..=x1 {
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
}

/// Re-places the rig in the middle of `tile` at rest.
fn put(world: &mut WorldState, tile: TilePosition) {
    let (x, y) = world.frame().tile_center(tile);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.x = x;
    rig.y = y;
    rig.x_remainder = 0.0;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.cart = None;
}

fn step(world: &mut WorldState, control: ControlState) -> Vec<GameplayEvent> {
    let inputs = TickInputs::default().hold_as(PLAYER, control);
    world.simulate_tick(world.simulation_tick(), &inputs).events
}

fn tile_of(world: &WorldState) -> TilePosition {
    world.player(PLAYER).expect("rig").tile_in(world.frame())
}

fn hold(horizontal: f32, thrust: bool, down: bool) -> ControlState {
    ControlState::new(horizontal, thrust, down)
}

/// A rail line on row `rail` from `x0` to `x1` with three rows of air over it, in solid stone.
fn rail_line(world: &mut WorldState, x0: i32, x1: i32, rail: i32) {
    fill(
        world,
        (x0 - 3, rail - 6),
        (x1 + 3, rail + 2),
        TileKind::Stone,
    );
    fill(world, (x0, rail - 3), (x1, rail - 1), TileKind::Air);
    fill(
        world,
        (x0, rail),
        (x1, rail),
        TileKind::Crust(CrustTile::Rail),
    );
}

/// Settles the rig on the ground (a few still ticks).
fn settle(world: &mut WorldState) {
    for _ in 0..40 {
        let _ = step(world, hold(0.0, false, false));
    }
}

#[test]
fn a_cart_rides_the_old_rails_takes_the_junction_and_burns_no_fuel() {
    let seed = 21;
    let rail = row_in(seed, Stratum::GreyStone);
    let x0 = 40;
    let mut world = world_at(
        seed,
        60,
        TilePosition {
            x: x0 + 1,
            y: rail - 1,
        },
    );
    // Upper line x0..x0+40; a ramp down from the junction at x0+12 to a lower line.
    rail_line(&mut world, x0, x0 + 40, rail);
    for step_x in 1..=4 {
        let x = x0 + 12 + step_x;
        // Air over the ramp, but the main line keeps its rail where the ramp dives under it.
        for y in rail + step_x - 3..rail + step_x {
            if y != rail {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Air);
            }
        }
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x,
                y: rail + step_x,
            },
            TileKind::Crust(CrustTile::Rail),
        );
    }
    fill(
        &mut world,
        (x0 + 17, rail + 1),
        (x0 + 34, rail + 3),
        TileKind::Air,
    );
    fill(
        &mut world,
        (x0 + 17, rail + 4),
        (x0 + 34, rail + 4),
        TileKind::Crust(CrustTile::Rail),
    );
    let _ = world.terrain_mut().set_kind(
        TilePosition {
            x: x0 + 35,
            y: rail + 3,
        },
        TileKind::Stone,
    );
    put(
        &mut world,
        TilePosition {
            x: x0 + 1,
            y: rail - 1,
        },
    );
    settle(&mut world);
    let fuel = world.player(PLAYER).expect("rig").fuel;
    let mut boarded = false;
    let mut switched = false;
    let mut left = false;
    for tick in 0..60 * 20 {
        // The stick held east (it boards), then down too (the lower branch at the junction).
        let events = step(&mut world, hold(1.0, false, tick > 20));
        for event in events {
            match event.kind {
                GameplayEventKind::CartBoarded { .. } => boarded = true,
                GameplayEventKind::CartSwitched { .. } => switched = true,
                GameplayEventKind::CartLeft { .. } => left = true,
                _ => {}
            }
        }
        if left {
            break;
        }
    }
    assert!(boarded && switched && left, "{boarded} {switched} {left}");
    let tile = tile_of(&world);
    assert_eq!(tile.y, rail + 3, "on the lower line: {tile:?}");
    assert!(tile.x >= x0 + 32, "at the end of the lower line: {tile:?}");
    let spent = fuel - world.player(PLAYER).expect("rig").fuel;
    // Only the boarding tick's drive and the stop at the end can cost fuel; the ride is free.
    assert!(spent < 1.0, "the ride burned {spent} fuel");
}

#[test]
fn a_cart_keeps_to_the_main_line_without_the_stick_down_and_thrust_jumps_off() {
    let seed = 22;
    let rail = row_in(seed, Stratum::GreyStone);
    let x0 = -80;
    let mut world = world_at(
        seed,
        60,
        TilePosition {
            x: x0 + 1,
            y: rail - 1,
        },
    );
    rail_line(&mut world, x0, x0 + 30, rail);
    put(
        &mut world,
        TilePosition {
            x: x0 + 1,
            y: rail - 1,
        },
    );
    settle(&mut world);
    for _ in 0..30 {
        let _ = step(&mut world, hold(1.0, false, false));
    }
    assert!(world.riding_cart(PLAYER));
    assert_eq!(tile_of(&world).y, rail - 1);
    let events = step(&mut world, hold(0.0, true, false));
    assert!(!world.riding_cart(PLAYER));
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::CartLeft { .. }))
    );
}

/// World seconds until a cart from the start of a 60-tile line reaches its end, at `hz`.
fn ride_seconds(hz: u32) -> f32 {
    let seed = 23;
    let rail = row_in(seed, Stratum::GreyStone);
    let x0 = 200;
    let mut world = world_at(
        seed,
        hz,
        TilePosition {
            x: x0 + 1,
            y: rail - 1,
        },
    );
    rail_line(&mut world, x0, x0 + 60, rail);
    put(
        &mut world,
        TilePosition {
            x: x0 + 1,
            y: rail - 1,
        },
    );
    settle(&mut world);
    for tick in 0..hz * 30 {
        let events = step(&mut world, hold(1.0, false, false));
        if events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::CartLeft { .. }))
        {
            #[allow(clippy::cast_precision_loss, reason = "a few thousand ticks")]
            return tick as f32 / hz as f32;
        }
    }
    f32::MAX
}

#[test]
fn cart_rides_take_the_same_time_at_every_tick_rate() {
    let base = ride_seconds(60);
    assert!(base < 20.0, "the ride ended: {base}");
    for hz in [30, 120] {
        let other = ride_seconds(hz);
        assert!(
            (other - base).abs() <= 0.1,
            "{hz} Hz rode {other} s against {base} s"
        );
    }
}

#[test]
fn mud_bogs_the_drive_down_and_slate_keeps_a_rig_sliding() {
    let seed = 24;
    let row = row_in(seed, Stratum::Topsoil);
    let run = |floor: TileKind, release: bool| -> f32 {
        let mut world = world_at(seed, 60, TilePosition { x: 60, y: row });
        fill(&mut world, (50, row - 4), (130, row + 3), TileKind::Stone);
        fill(&mut world, (55, row - 3), (125, row), TileKind::Air);
        fill(&mut world, (55, row + 1), (125, row + 1), floor);
        put(&mut world, TilePosition { x: 58, y: row });
        settle(&mut world);
        // Up to speed first, then measure a second and a half at speed.
        for _ in 0..90 {
            let _ = step(&mut world, hold(1.0, false, false));
        }
        let start = world.player(PLAYER).expect("rig").x;
        for _ in 0..90 {
            let _ = step(&mut world, hold(1.0, false, false));
        }
        if release {
            for _ in 0..90 {
                let _ = step(&mut world, hold(0.0, false, false));
            }
        }
        world.player(PLAYER).expect("rig").x - start
    };
    let stone = run(TileKind::Stone, false);
    let mud = run(TileKind::Surface(SurfaceMaterial::Mud), false);
    assert!(mud < stone * 0.6, "mud {mud} against stone {stone}");
    let stone_coast = run(TileKind::Stone, true);
    let slate_coast = run(TileKind::Crust(CrustTile::Slate), true);
    assert!(
        slate_coast > stone_coast + 32.0,
        "slate slides {slate_coast} against {stone_coast}"
    );
}

#[test]
fn a_timber_set_collapses_after_a_creak_when_its_floor_is_undercut() {
    let seed = 25;
    let rail = row_in(seed, Stratum::GreyStone);
    let x0 = 300;
    let mut world = world_at(
        seed,
        60,
        TilePosition {
            x: x0 + 5,
            y: rail - 1,
        },
    );
    rail_line(&mut world, x0, x0 + 12, rail);
    fill(
        &mut world,
        (x0, rail - 4),
        (x0 + 12, rail - 4),
        TileKind::Crust(CrustTile::Timber),
    );
    put(
        &mut world,
        TilePosition {
            x: x0 + 5,
            y: rail - 1,
        },
    );
    settle(&mut world);
    let mut creaked = None;
    let mut collapsed = None;
    for tick in 0..60 * 8 {
        // Drill down through the rail bed under the set, then sit still.
        let down = creaked.is_none();
        let events = step(&mut world, hold(0.0, false, down));
        for event in events {
            match event.kind {
                GameplayEventKind::SupportCreaked { .. } => creaked = creaked.or(Some(tick)),
                GameplayEventKind::SupportCollapsed { .. } => collapsed = Some(tick),
                _ => {}
            }
        }
        if collapsed.is_some() {
            break;
        }
    }
    let (Some(creak), Some(collapse)) = (creaked, collapsed) else {
        panic!("creaked {creaked:?}, collapsed {collapsed:?}");
    };
    // A fair warning: the creak comes a second and a half before the roof.
    assert!(collapse - creak >= 90, "{creak} -> {collapse}");
    let beam = world
        .terrain()
        .tile(TilePosition {
            x: x0 + 5,
            y: rail - 4,
        })
        .map(|tile| tile.kind);
    assert_eq!(beam, Some(TileKind::Air), "the beam came down");
    // The set's falling roof came down as boulders.
    assert!(
        world
            .entities()
            .of_kind(crate::entity::EntityKind::Boulder)
            .count()
            > 0
    );
}

#[test]
fn a_slate_shelf_left_hanging_slides_down() {
    let seed = 26;
    let row = row_in(seed, Stratum::SlateShelves);
    let x0 = 400;
    let mut world = world_at(
        seed,
        60,
        TilePosition {
            x: x0 + 12,
            y: row + 1,
        },
    );
    fill(
        &mut world,
        (x0 - 4, row - 6),
        (x0 + 18, row + 6),
        TileKind::Stone,
    );
    // A shelf over a cave, held up at its east end by a pillar of stone; a low tunnel east of
    // the pillar for the rig.
    fill(&mut world, (x0, row + 1), (x0 + 9, row + 4), TileKind::Air);
    fill(
        &mut world,
        (x0, row),
        (x0 + 10, row),
        TileKind::Crust(CrustTile::Slate),
    );
    fill(
        &mut world,
        (x0 + 11, row + 1),
        (x0 + 15, row + 1),
        TileKind::Air,
    );
    put(
        &mut world,
        TilePosition {
            x: x0 + 12,
            y: row + 1,
        },
    );
    settle(&mut world);
    let mut shuddered = false;
    let mut slid = false;
    for _ in 0..60 * 8 {
        // Drill west into the pillar under the shelf's end.
        let events = step(&mut world, hold(-1.0, false, false));
        for event in events {
            match event.kind {
                GameplayEventKind::SlateShudder { .. } => shuddered = true,
                GameplayEventKind::SlateSlid { .. } => slid = true,
                _ => {}
            }
        }
        if slid {
            break;
        }
    }
    assert!(shuddered && slid, "{shuddered} {slid}");
    let left = (x0..=x0 + 10)
        .filter(|x| {
            world
                .terrain()
                .tile(TilePosition { x: *x, y: row })
                .is_some_and(|tile| tile.kind == TileKind::Crust(CrustTile::Slate))
        })
        .count();
    assert!(left < 4, "the shelf came down ({left} left)");
}

#[test]
fn an_aquifer_floods_the_tunnel_that_breaches_it() {
    let seed = 27;
    let row = row_in(seed, Stratum::ClayBeds);
    let x0 = 500;
    let mut world = world_at(seed, 60, TilePosition { x: x0, y: row });
    fill(
        &mut world,
        (x0 - 4, row - 8),
        (x0 + 24, row + 4),
        TileKind::Clay,
    );
    // A tunnel west, a sealed pocket of water east, one clay tile between them.
    fill(&mut world, (x0 - 2, row - 1), (x0 + 6, row), TileKind::Air);
    for y in row - 5..=row {
        for x in x0 + 8..=x0 + 18 {
            let _ = world.terrain_mut().set_tile_from_network(
                TilePosition { x, y },
                TileKind::Fluid(FluidKind::Water),
                crate::fluid::FLUID_FULL,
            );
        }
    }
    put(&mut world, TilePosition { x: x0 + 5, y: row });
    settle(&mut world);
    let mut breached = false;
    for _ in 0..60 * 12 {
        let events = step(&mut world, hold(1.0, false, false));
        breached |= events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::FluidBreached { .. }));
    }
    assert!(breached, "the drill broke into the pocket");
    let flooded = (x0 - 2..=x0 + 6)
        .filter(|x| {
            world
                .terrain()
                .fluid_at(TilePosition { x: *x, y: row })
                .is_some()
        })
        .count();
    assert!(flooded >= 5, "the tunnel flooded: {flooded} cells");
}

#[test]
fn lava_tubes_run_hotter_than_the_basalt_and_heat_is_per_stratum() {
    let seed = 28;
    let settings = v11(seed);
    let (top, bottom) = layout(&settings).rows(Stratum::BasaltColumns);
    let tube = (top..bottom)
        .flat_map(|y| (-400..400).map(move |x| TilePosition { x, y }))
        .find(|at| crust::lava_tube_at(&settings, at.x, at.y))
        .expect("a lava tube");
    let mut world = world_at(seed, 60, tube);
    let base = crust::stratum_heat_units(Stratum::BasaltColumns);
    assert_eq!(
        world.heat_units_at(tube),
        base + crust::LAVA_TUBE_HEAT_UNITS
    );
    let outside = TilePosition {
        x: tube.x,
        y: tube.y - 8,
    };
    assert_eq!(world.heat_units_at(outside), base);
    // A rig with a tier 2 radiator is cool in the basalt but not in the tube.
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.radiator_level = 2;
    rig.hazard_gear = crate::protection::HazardGear::default();
    let hull = world.player(PLAYER).expect("rig").hull;
    let mut entered = false;
    for _ in 0..60 {
        let events = step(&mut world, hold(0.0, true, false));
        entered |= events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::LavaTubeEntered { .. }));
        put(&mut world, tube);
    }
    assert!(entered);
    assert!(
        world.player(PLAYER).expect("rig").hull < hull,
        "the tube's heat bit"
    );
    // The topsoil is cool for a stock radiator; the old band heat no longer cooks deep rigs.
    assert_eq!(crust::stratum_heat_units(Stratum::Topsoil), 0);
    let classic = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(seed),
        SimulationParams::default(),
    );
    assert_eq!(
        classic.heat_units_at(TilePosition { x: 0, y: 100 }),
        crate::protection::band_heat_units(
            classic.terrain().band_at(TilePosition { x: 0, y: 100 })
        )
    );
}

#[test]
fn a_strongbox_pays_and_reads_its_log_once() {
    let seed = 29;
    let settings = v11(seed);
    let cabin = crust::cabin(&settings).expect("a cabin");
    let strongbox = TilePosition {
        x: cabin.max.x - 2,
        y: cabin.max.y - 1,
    };
    let mut world = world_at(
        seed,
        60,
        TilePosition {
            x: strongbox.x - 1,
            y: strongbox.y,
        },
    );
    put(
        &mut world,
        TilePosition {
            x: strongbox.x - 1,
            y: strongbox.y,
        },
    );
    settle(&mut world);
    let credits = world.player(PLAYER).expect("rig").credits;
    let mut log = None;
    for _ in 0..60 * 6 {
        let events = step(&mut world, hold(1.0, false, false));
        for event in events {
            if let GameplayEventKind::StrongboxOpened {
                log: read, first, ..
            } = event.kind
            {
                log = Some((read, first));
            }
        }
        if log.is_some() {
            break;
        }
    }
    assert_eq!(
        log,
        Some((0, true)),
        "the cabin's log, read for the first time"
    );
    assert!(world.player(PLAYER).expect("rig").credits > credits);
    assert!(world.crust().logs.contains(&0));
}

#[test]
fn the_deep_claim_gates_the_old_deep() {
    let seed = 30;
    let row = row_in(seed, Stratum::CrystalFaults);
    let mut world = world_at(seed, 60, TilePosition { x: 0, y: row });
    world.set_progression_rules(crate::depth_progression::ProgressionRules::Descent);
    world.crust.deep_claim = false;
    fill(&mut world, (-3, row - 3), (3, row + 2), TileKind::Air);
    fill(&mut world, (-3, row + 1), (3, row + 1), TileKind::Stone);
    put(&mut world, TilePosition { x: 0, y: row });
    let hull = world.player(PLAYER).expect("rig").hull;
    let mut warned = 0;
    for _ in 0..120 {
        let events = step(&mut world, hold(0.0, false, false));
        warned += events
            .iter()
            .filter(|event| matches!(event.kind, GameplayEventKind::DeepClaimMissing { .. }))
            .count();
    }
    assert_eq!(warned, 1, "one warning a trip");
    assert!(world.player(PLAYER).expect("rig").hull < hull);
    world.grant_deep_claim();
    let hull = world.player(PLAYER).expect("rig").hull;
    for _ in 0..120 {
        let _ = step(&mut world, hold(0.0, false, false));
    }
    assert!((world.player(PLAYER).expect("rig").hull - hull).abs() < f32::EPSILON);
}

/// The Sinkhole with a rig parked beside its core.
fn at_the_core(seed: u64) -> (WorldState, crust::SinkholeSite) {
    let settings = v11(seed);
    let site = crust::sinkhole_site(&settings).expect("a sinkhole");
    let beside = TilePosition {
        x: site.core.x - 1,
        y: site.core.y,
    };
    let mut world = world_at(seed, 60, beside);
    world.crust.deep_claim = false;
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        // A Descent I rig: tier 4 engine, stock everything else.
        rig.engine_level = 4;
        rig.hazard_gear = crate::protection::HazardGear::default();
        rig.drill_strength = 4;
    }
    put(&mut world, beside);
    settle(&mut world);
    (world, site)
}

#[test]
fn a_bot_pulls_the_sinkhole_core_and_climbs_out_before_the_collapse() {
    let (mut world, site) = at_the_core(31);
    let mut rumbled = false;
    let mut escaped = false;
    let mut claimed = false;
    let mut at_the_top = false;
    for _ in 0..60 * 60 {
        // Drill east into the core; once it is out, climb the funnel to the adit and run east
        // down it to the camp.
        let tile = tile_of(&world);
        at_the_top |= rumbled && tile.y <= site.lip_row + 2;
        let control = if !rumbled {
            hold(1.0, false, false)
        } else if !at_the_top {
            // Up the funnel, edging toward its middle.
            let toward = (site.core.x + 1 - tile.x).signum();
            #[allow(clippy::cast_precision_loss, reason = "a sign")]
            hold(toward as f32 * 0.3, true, false)
        } else {
            hold(1.0, tile.y > site.lip_row + 2, false)
        };
        for event in step(&mut world, control) {
            match event.kind {
                GameplayEventKind::SinkholeRumble { .. } => rumbled = true,
                GameplayEventKind::SinkholeEscaped { .. } => escaped = true,
                GameplayEventKind::DeepClaimGranted { .. } => claimed = true,
                GameplayEventKind::SinkholeCaught { .. } => {
                    panic!("caught at {:?}", tile_of(&world))
                }
                _ => {}
            }
        }
        if claimed {
            break;
        }
    }
    assert!(
        rumbled && escaped && claimed,
        "{rumbled} {escaped} {claimed}"
    );
    assert!(world.crust().deep_claim);
    assert!(world.player(PLAYER).expect("rig").drill_strength >= DRILL_II_LEVEL);
    assert!(world.crust().sinkhole.is_none());
    // The chamber is rubble now.
    let core = world.terrain().tile(site.core).map(|tile| tile.kind);
    assert_eq!(core, Some(TileKind::Crust(CrustTile::Rubble)));
}

#[test]
fn a_rig_that_stays_put_is_caught_set_down_in_the_camp_and_the_sinkhole_resets() {
    let (mut world, site) = at_the_core(32);
    let hull = world.player(PLAYER).expect("rig").hull;
    let mut caught = false;
    let mut reset = false;
    let mut rumbled = false;
    for _ in 0..60 * 60 {
        let control = if rumbled {
            hold(0.0, false, false)
        } else {
            hold(1.0, false, false)
        };
        for event in step(&mut world, control) {
            match event.kind {
                GameplayEventKind::SinkholeRumble { .. } => rumbled = true,
                GameplayEventKind::SinkholeCaught { .. } => caught = true,
                GameplayEventKind::SinkholeReset { .. } => reset = true,
                _ => {}
            }
        }
        if reset {
            break;
        }
    }
    assert!(rumbled && caught && reset, "{rumbled} {caught} {reset}");
    assert!(!world.crust().deep_claim);
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.hull < hull && rig.hull > 0.0, "dented, not wrecked");
    assert!(
        site.camp.expanded(1).contains(tile_of(&world)),
        "set down in the camp"
    );
    // The core sits in the floor again: the set piece can be retried.
    assert_eq!(
        world.terrain().tile(site.core).map(|tile| tile.kind),
        Some(TileKind::Crust(CrustTile::SinkholeCore))
    );
}

#[test]
fn every_crust_stratum_has_natives_of_its_own() {
    use crate::enemy::EnemyKind;
    let natives = [
        (Stratum::Topsoil, EnemyKind::BurrowMole),
        (Stratum::Topsoil, EnemyKind::MudLeech),
        (Stratum::ClayBeds, EnemyKind::PotterWasp),
        (Stratum::GreyStone, EnemyKind::MineBat),
        (Stratum::GreyStone, EnemyKind::TimberBeetle),
        (Stratum::SlateShelves, EnemyKind::SlateSkink),
        (Stratum::SlateShelves, EnemyKind::ShelfCrab),
        (Stratum::BasaltColumns, EnemyKind::TubeCrawler),
        (Stratum::BasaltColumns, EnemyKind::CinderMoth),
    ];
    for (stratum, kind) in natives {
        assert!(
            stratum
                .def()
                .creatures
                .iter()
                .any(|(each, _)| *each == kind),
            "{kind:?} lives in {stratum:?}"
        );
        // Their contact boxes are at least their terrain boxes, and close to their art.
        let (half_w, half_h) = kind.contact_half();
        let radius = kind.stats().radius;
        assert!(half_w >= radius && half_h >= radius * 0.75, "{kind:?}");
    }
    for stratum in crate::descent::Descent::Crust.strata() {
        let creatures = stratum.def().creatures;
        assert!((2..=4).contains(&creatures.len()), "{stratum:?}");
    }
}

#[test]
fn a_mud_leech_drinks_fuel_instead_of_denting_the_hull() {
    use crate::enemy::EnemyKind;
    let seed = 33;
    let row = row_in(seed, Stratum::Topsoil);
    let mut world = world_at(seed, 60, TilePosition { x: 30, y: row });
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    fill(&mut world, (20, row - 4), (44, row + 3), TileKind::Stone);
    fill(&mut world, (22, row - 3), (42, row), TileKind::Air);
    put(&mut world, TilePosition { x: 30, y: row });
    settle(&mut world);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hazard_gear = crate::protection::HazardGear::default();
    }
    let (x, y) = world.frame().tile_center(TilePosition { x: 31, y: row });
    let _ = world.spawn_enemy(EnemyKind::MudLeech, x, y);
    let fuel = world.player(PLAYER).expect("rig").fuel;
    let mut latched = false;
    for _ in 0..60 * 8 {
        latched |= step(&mut world, hold(0.0, false, false))
            .iter()
            .any(|event| {
                matches!(
                    event.kind,
                    GameplayEventKind::CrustCreature {
                        action: crust::CrustCreatureAction::LeechLatched,
                        ..
                    }
                )
            });
        if latched {
            break;
        }
    }
    assert!(latched, "the leech latched on");
    assert!(world.player(PLAYER).expect("rig").fuel < fuel - 5.0);
}

#[test]
fn a_timber_beetle_chews_through_a_set_and_brings_it_down() {
    use crate::enemy::EnemyKind;
    let seed = 34;
    let rail = row_in(seed, Stratum::GreyStone);
    let x0 = 600;
    let mut world = world_at(
        seed,
        60,
        TilePosition {
            x: x0 + 30,
            y: rail - 1,
        },
    );
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    rail_line(&mut world, x0, x0 + 40, rail);
    fill(
        &mut world,
        (x0, rail - 4),
        (x0 + 40, rail - 4),
        TileKind::Crust(CrustTile::Timber),
    );
    // The rig far off and out of sight behind a wall: the beetle stays at rest.
    fill(
        &mut world,
        (x0 + 20, rail - 3),
        (x0 + 20, rail - 1),
        TileKind::Stone,
    );
    put(
        &mut world,
        TilePosition {
            x: x0 + 36,
            y: rail - 1,
        },
    );
    let (x, y) = world.frame().tile_center(TilePosition {
        x: x0 + 6,
        y: rail - 1,
    });
    let _ = world.spawn_enemy(EnemyKind::TimberBeetle, x, y);
    let mut chewed = false;
    let mut collapsed = false;
    for _ in 0..60 * 90 {
        for event in step(&mut world, hold(0.0, false, false)) {
            match event.kind {
                GameplayEventKind::CrustCreature {
                    action: crust::CrustCreatureAction::BeetleChewed,
                    ..
                } => chewed = true,
                GameplayEventKind::SupportCollapsed { .. } => collapsed = true,
                _ => {}
            }
        }
        if collapsed {
            break;
        }
    }
    assert!(chewed && collapsed, "{chewed} {collapsed}");
}

#[test]
fn gravel_hollow_is_a_peopled_settlement_beside_the_first_outpost() {
    use crate::village::{self, BuildingKind, placement::underground_id};
    let seed = 35;
    let settings = v11(seed);
    let hollow = village::village(&settings, underground_id(0)).expect("Gravel Hollow");
    assert_eq!(hollow.name, crust::GRAVEL_HOLLOW_NAME);
    assert_eq!(hollow.population.village_name, crust::GRAVEL_HOLLOW_NAME);
    assert!(hollow.population.characters.len() >= 4, "people live there");
    assert!(
        hollow
            .buildings
            .iter()
            .any(|building| building.kind == BuildingKind::Market),
        "a shop"
    );
    assert_eq!(
        crate::worldgen::descent::stratum_at(&settings, hollow.center_column(), hollow.ground_row),
        Stratum::GreyStone
    );
    // Beside the Descent I outpost (fuel, repair and selling at its counter), on its floor.
    let world = WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    let outpost = world
        .logistics()
        .outposts
        .values()
        .find(|outpost| outpost.descent == 0)
        .expect("the Descent I outpost");
    assert_eq!(outpost.floor_row, hollow.ground_row);
    assert!((outpost.column - hollow.center_column()).abs() < 60);
    // Its people walk the street once a rig is there.
    let mut world = world_at(
        seed,
        60,
        TilePosition {
            x: hollow.center_column(),
            y: hollow.ground_row - 1,
        },
    );
    for _ in 0..60 * 3 {
        let _ = step(&mut world, hold(0.0, false, false));
    }
    let people = world
        .entities()
        .of_kind(crate::entity::EntityKind::Npc)
        .count();
    assert!(people >= 2, "{people} people about");
    // No other world has it.
    assert!(village::village(&WorldSettings::new(seed), underground_id(0)).is_none());
}
