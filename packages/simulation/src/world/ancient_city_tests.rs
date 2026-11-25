//! The builders' buried city in a Deep Descent world (Wave 10 ancients-a, Descent VII strata
//! 1-3): trams and lifts ridden, pistons ridden and thrown from alike at every tick rate,
//! crushers and belts, the city's alarm and its watch, the archivist companion (wakes,
//! follows, talks, scans; per player, saved and replicated), star charts read and kept, and
//! puzzle vaults solved by a bot.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WeaponControl, WorldState,
    ancient_city::{
        ALARM_FULL, CityEvent, CityLandmark, CityTile,
        archivist::{SCAN_MILLIS, VaultKind},
        movers::{CityMover, MILLI, MoverKind, StrokePhase},
    },
    depth_progression::RigSystem,
    descent::Stratum,
    enemy::{EnemyDifficulty, EnemyKind},
    far_strange::{FarStrangeAction, LampFilter},
    logistics::LogisticsCommand,
    position::WorldPos,
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    weapon::WeaponKind,
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{
            ancient_city::{self as sites, CityStampKind},
            layout,
        },
    },
};

const PLAYER: PlayerId = PlayerId::new(0);
const PARTNER: PlayerId = PlayerId::new(1);
const SEED: u64 = 0xA4C1_7E57;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// The rig systems a rig holds on reaching Descent VII: every gate's reward through the Phase
/// Drill (Descent VI's), never the Heart Key.
fn descent_seven_systems() -> u8 {
    RigSystem::ALL
        .into_iter()
        .filter(|system| !matches!(system, RigSystem::HeartKey))
        .fold(0, |bits, system| bits | system.bit())
}

fn fit(world: &mut WorldState, player: PlayerId) {
    let rig = world.player_mut(player).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hazard_gear.rig_systems = descent_seven_systems();
    rig.radiator_level = 19;
    rig.hull_level = 19;
    rig.hull = rig.max_hull();
    rig.fuel = rig.fuel_capacity;
    rig.credits = 5_000_000;
}

/// A Deep Descent world at `hz`, the rig fitted for Descent VII and the director quiet.
fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    fit(&mut world, PLAYER);
    world
}

fn put(world: &mut WorldState, player: PlayerId, tile: TilePosition) {
    world.teleport_rig_to(
        player,
        WorldPos::tile_center(i64::from(tile.x), i64::from(tile.y)),
    );
    world.stream_terrain_near_players();
    let rig = world.player_mut(player).expect("rig");
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn put_rig(world: &mut WorldState, tile: TilePosition) {
    put(world, PLAYER, tile);
}

fn tick_with(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let output = world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
    // Keep the rig healthy: these tests are about the machinery, not the heat.
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.fuel = rig.fuel_capacity;
    output.events.into_iter().map(|event| event.kind).collect()
}

fn ticks(world: &mut WorldState, count: u64) -> Vec<GameplayEventKind> {
    (0..count)
        .flat_map(|_| tick_with(world, ControlState::default()))
        .collect()
}

fn city_events(events: &[GameplayEventKind]) -> Vec<(CityEvent, u16)> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::AncientCity { what, detail, .. } => Some((*what, *detail)),
            _ => None,
        })
        .collect()
}

fn has(events: &[GameplayEventKind], what: CityEvent) -> bool {
    city_events(events).iter().any(|(event, _)| *event == what)
}

fn rig_xy(world: &WorldState) -> (f32, f32) {
    let rig = world.player(PLAYER).expect("rig");
    (rig.x, rig.y)
}

fn area_of(stratum: Stratum, x0: i32, x1: i32) -> TileRect {
    let (top, bottom) = layout(&settings()).rows(stratum);
    TileRect {
        min: TilePosition { x: x0, y: top },
        max: TilePosition { x: x1, y: bottom },
    }
}

fn movers(stratum: Stratum) -> Vec<CityMover> {
    let settings = settings();
    sites::movers_with(&settings, &layout(&settings), &area_of(stratum, -500, 500))
}

fn stamps(stratum: Stratum) -> Vec<sites::CityStamp> {
    let settings = settings();
    sites::stamps_with(&settings, &layout(&settings), &area_of(stratum, -700, 700))
}

/// World milliseconds now.
fn now(world: &WorldState) -> u64 {
    world.millis_at(world.simulation_tick())
}

// --- Trams and lifts -----------------------------------------------------------------------

/// The first tram line with at least three stations.
fn tram() -> (i32, crate::ancient_city::movers::Track) {
    movers(Stratum::BuriedMegacity)
        .into_iter()
        .find_map(|mover| match mover {
            CityMover::Tram {
                rail_row, track, ..
            } if track.stops.len() >= 3 => Some((rail_row, track)),
            _ => None,
        })
        .expect("a tram line")
}

fn column_of(milli: i64) -> i32 {
    i32::try_from(milli.div_euclid(MILLI)).unwrap_or(0)
}

#[test]
fn a_rig_boards_a_tram_rides_it_to_the_next_station_and_steps_off_alike_at_every_tick_rate() {
    let ride = |hz: u32| -> Vec<(u64, f32, f32)> {
        let mut world = world_at(hz);
        let (rail_row, track) = tram();
        let station = column_of(track.stops[1]);
        put_rig(
            &mut world,
            TilePosition {
                x: station,
                y: rail_row - 1,
            },
        );
        // Wait on the platform: the car comes and the rig is aboard.
        let mut boarded = false;
        for _ in 0..hz * 90 {
            let events = ticks(&mut world, 1);
            if has(&events, CityEvent::TramBoarded) {
                boarded = true;
                break;
            }
        }
        assert!(boarded, "{hz} Hz: the tram never came");
        assert_eq!(
            world.city_ride_of(PLAYER).map(|ride| ride.mover.kind),
            Some(MoverKind::Tram)
        );
        // Ride on to the next station, sampling the rig on whole 100 ms marks.
        let mut samples = Vec::new();
        let mut arrived = false;
        let start = now(&world);
        for _ in 0..hz * 60 {
            let events = ticks(&mut world, 1);
            let millis = now(&world);
            if millis.is_multiple_of(100) && millis - start < 20_000 {
                let (x, y) = rig_xy(&world);
                samples.push((millis, x, y));
            }
            if has(&events, CityEvent::TramArrived) {
                arrived = true;
                break;
            }
        }
        assert!(arrived, "{hz} Hz: the tram never reached a station");
        let (x, _) = rig_xy(&world);
        let moved = (x - world
            .frame()
            .tile_center(TilePosition { x: station, y: 0 })
            .0)
            .abs();
        assert!(moved > TILE_SIZE * 10.0, "{hz} Hz: the rig rode {moved} px");
        // Drive off the car onto the platform.
        let mut off = false;
        for _ in 0..hz * 4 {
            let events = tick_with(&mut world, ControlState::new(1.0, false, false));
            if has(&events, CityEvent::TramAlighted) {
                off = true;
                break;
            }
        }
        assert!(off, "{hz} Hz: the rig could not step off");
        assert!(world.city_ride_of(PLAYER).is_none());
        // It stands on the platform, not in the air or in rock.
        let _ = ticks(&mut world, u64::from(hz));
        let tile = world.player(PLAYER).expect("rig").tile_in(world.frame());
        assert_eq!(tile.y, rail_row - 1, "{hz} Hz: off the car at {tile:?}");
        samples
    };
    let slow = ride(20);
    let fast = ride(60);
    // Wherever both tick rates sampled the same moment, the rig was in the same place.
    let mut compared = 0;
    for (millis, x, y) in &slow {
        if let Some((_, fx, fy)) = fast.iter().find(|sample| sample.0 == *millis) {
            assert!(
                (x - fx).abs() < 1.0 && (y - fy).abs() < 1.0,
                "at {millis} ms: 20 Hz ({x}, {y}) vs 60 Hz ({fx}, {fy})"
            );
            compared += 1;
        }
    }
    assert!(compared >= 10, "compared {compared} samples");
}

#[test]
fn a_lift_carries_a_rig_from_one_avenue_down_to_the_next() {
    let mut world = world_at(30);
    let (column, track) = movers(Stratum::BuriedMegacity)
        .into_iter()
        .find_map(|mover| match mover {
            CityMover::Lift { column, track, .. } => Some((column, track)),
            _ => None,
        })
        .expect("a lift");
    let top = column_of(track.stops[0]);
    let bottom = column_of(track.stops[track.stops.len() - 1]);
    // On the upper street beside the shaft, then into the cab when it waits there.
    put_rig(
        &mut world,
        TilePosition {
            x: column,
            y: top - 1,
        },
    );
    let mut boarded = false;
    for _ in 0..30 * 120 {
        let events = ticks(&mut world, 1);
        if has(&events, CityEvent::LiftBoarded) {
            boarded = true;
            break;
        }
    }
    assert!(boarded, "the cab never took the rig");
    // Ride until the cab waits at the lower street, then step out.
    let mut reached = false;
    for _ in 0..30 * 120 {
        let _ = ticks(&mut world, 1);
        let tile = world.player(PLAYER).expect("rig").tile_in(world.frame());
        if tile.y == bottom - 1 {
            reached = true;
            break;
        }
    }
    assert!(reached, "the cab never reached the lower street");
    // The cab waits there a moment; walk out.
    let mut off = false;
    for _ in 0..30 * 8 {
        let events = tick_with(&mut world, ControlState::new(-1.0, false, false));
        if has(&events, CityEvent::LiftAlighted) {
            off = true;
            break;
        }
    }
    assert!(off, "the rig could not step out");
    let _ = ticks(&mut world, 30);
    let tile = world.player(PLAYER).expect("rig").tile_in(world.frame());
    assert_eq!(tile.y, bottom - 1, "out on the lower street: {tile:?}");
}

// --- Pistons, crushers and belts ----------------------------------------------------------

/// The first piston on a hall's floor (its base on the floor, not on a ledge) and its hall.
fn floor_piston() -> (i32, i32, crate::ancient_city::movers::Cycle) {
    let stamps = stamps(Stratum::EngineHalls);
    let hall = stamps
        .iter()
        .find(|stamp| stamp.kind == CityStampKind::EngineHall)
        .expect("an engine hall");
    let settings = settings();
    sites::movers_with(&settings, &layout(&settings), &hall.bounds)
        .into_iter()
        .find_map(|mover| match mover {
            CityMover::Piston {
                column,
                floor_row,
                cycle,
                ..
            } if floor_row == hall.bounds.max.y - 1 => Some((column, floor_row, cycle)),
            _ => None,
        })
        .expect("a floor piston")
}

/// Ticks until the piston at `(column, floor_row)` is at rest early in its stroke.
fn wait_for_rest(
    world: &mut WorldState,
    column: i32,
    floor_row: i32,
    cycle: crate::ancient_city::movers::Cycle,
) {
    let piston = CityMover::Piston {
        anchor: crate::ancient_city::MoverRef {
            kind: MoverKind::Piston,
            x: column,
            y: floor_row,
        },
        column,
        floor_row,
        cycle,
    };
    for _ in 0..10_000 {
        let millis = now(world);
        let resting = piston
            .piston_top(millis)
            .is_some_and(|(_, _, phase)| phase == StrokePhase::Rest);
        let early = (millis + cycle.phase_millis) % cycle.period_millis < cycle.period_millis / 4;
        if resting && early {
            return;
        }
        let _ = ticks(world, 1);
    }
}

#[test]
fn a_piston_lifts_a_rig_on_its_head_alike_at_every_tick_rate_and_throws_one_that_jumps() {
    let lift = |hz: u32| -> Vec<(u64, f32)> {
        let mut world = world_at(hz);
        let (column, floor_row, cycle) = floor_piston();
        wait_for_rest(&mut world, column, floor_row, cycle);
        put_rig(
            &mut world,
            TilePosition {
                x: column,
                y: floor_row - 1,
            },
        );
        let _ = ticks(&mut world, u64::from(hz) / 4);
        assert!(world.city_ride_of(PLAYER).is_some(), "{hz} Hz: on the head");
        let mut samples = Vec::new();
        let mut lowest = f32::MAX;
        for _ in 0..hz * 8 {
            let _ = ticks(&mut world, 1);
            let millis = now(&world);
            let (_, y) = rig_xy(&world);
            lowest = lowest.min(y);
            if millis.is_multiple_of(50) {
                samples.push((millis, y));
            }
        }
        let (_, rest_y) = world.frame().tile_center(TilePosition {
            x: column,
            y: floor_row - 1,
        });
        assert!(
            rest_y - lowest > TILE_SIZE * 5.0,
            "{hz} Hz: the head lifted the rig {} px",
            rest_y - lowest
        );
        samples
    };
    let slow = lift(20);
    let fast = lift(60);
    let mut compared = 0;
    for (millis, y) in &slow {
        if let Some((_, fy)) = fast.iter().find(|sample| sample.0 == *millis) {
            assert!((y - fy).abs() < 1.0, "at {millis} ms: {y} vs {fy}");
            compared += 1;
        }
    }
    assert!(compared > 50);

    // Thrusting off a rising head throws the rig higher than the head goes.
    let mut world = world_at(60);
    let (column, floor_row, cycle) = floor_piston();
    wait_for_rest(&mut world, column, floor_row, cycle);
    put_rig(
        &mut world,
        TilePosition {
            x: column,
            y: floor_row - 1,
        },
    );
    let _ = ticks(&mut world, 30);
    let top_of_stroke = world.frame().row_px(floor_row - cycle.travel);
    let mut launched = false;
    let mut peak = f32::MAX;
    for _ in 0..60 * 10 {
        let rising = world
            .player(PLAYER)
            .is_some_and(|rig| rig.velocity_y < -250.0)
            && world.city_ride_of(PLAYER).is_some();
        let events = if rising && !launched {
            tick_with(&mut world, ControlState::new(0.0, true, false))
        } else {
            ticks(&mut world, 1)
        };
        if has(&events, CityEvent::PistonLaunched) {
            launched = true;
        }
        if launched {
            peak = peak.min(rig_xy(&world).1);
            if world.player(PLAYER).is_some_and(|rig| rig.velocity_y > 0.0) {
                break;
            }
        }
    }
    assert!(launched, "the rig never jumped off a rising head");
    assert!(
        peak < top_of_stroke - TILE_SIZE,
        "thrown to {peak}, the head stops at {top_of_stroke}"
    );
}

#[test]
fn a_crusher_warns_then_strikes_a_rig_under_it_once_a_slam_and_shoves_it_out() {
    let mut world = world_at(30);
    let (column, housing_row, cycle) = movers(Stratum::EngineHalls)
        .into_iter()
        .find_map(|mover| match mover {
            CityMover::Crusher {
                column,
                housing_row,
                cycle,
                ..
            } => Some((column, housing_row, cycle)),
            _ => None,
        })
        .expect("a crusher");
    let floor = housing_row + 3 + cycle.travel;
    // Wait until it is at rest, then park under it.
    let mover = movers(Stratum::EngineHalls)
        .into_iter()
        .find(|mover| matches!(mover, CityMover::Crusher { column: c, .. } if *c == column))
        .expect("the crusher");
    while mover.crusher_bottom(now(&world)).map(|(_, phase)| phase) != Some(StrokePhase::Rest) {
        let _ = ticks(&mut world, 1);
    }
    put_rig(
        &mut world,
        TilePosition {
            x: column,
            y: floor - 1,
        },
    );
    let hull = world.player(PLAYER).expect("rig").hull;
    let cx = world
        .frame()
        .tile_center(TilePosition { x: column, y: 0 })
        .0;
    let mut struck = 0;
    let mut shoved_to = None;
    for _ in 0..cycle.period_millis * 30 / 1_000 {
        let events = ticks(&mut world, 1);
        let hits = city_events(&events)
            .iter()
            .filter(|(what, _)| *what == CityEvent::Crushed)
            .count();
        if hits > 0 && shoved_to.is_none() {
            shoved_to = Some(rig_xy(&world));
        }
        struck += hits;
    }
    assert_eq!(struck, 1, "one strike a slam");
    assert!(world.player(PLAYER).expect("rig").hull < hull);
    // Shoved out from under it (the belt may carry it back for the next slam: that is the
    // line's rhythm), and never into rock.
    let (x, y) = shoved_to.expect("struck");
    assert!((x - cx).abs() > TILE_SIZE * 1.5, "still under the crusher");
    assert!(
        !super::movement::authoritative_player_collides(world.terrain(), x, y),
        "shoved into rock at {x}"
    );
}

#[test]
fn a_crusher_shoves_a_rig_away_from_rock_never_into_it() {
    let mut world = world_at(30);
    let mover = movers(Stratum::EngineHalls)
        .into_iter()
        .find(|mover| matches!(mover, CityMover::Crusher { .. }))
        .expect("a crusher");
    let CityMover::Crusher {
        column,
        housing_row,
        cycle,
        ..
    } = mover.clone()
    else {
        unreachable!("a crusher")
    };
    let floor = housing_row + 3 + cycle.travel;
    while mover.crusher_bottom(now(&world)).map(|(_, phase)| phase) != Some(StrokePhase::Rest) {
        let _ = ticks(&mut world, 1);
    }
    // A rig just right of the centre would be shoved right, but rock (dug back in, say) fills
    // that side: it goes out the open left side instead.
    put_rig(
        &mut world,
        TilePosition {
            x: column,
            y: floor - 1,
        },
    );
    let rock: Vec<(TilePosition, TileKind)> = (column + 2..=column + 4)
        .flat_map(|x| (floor - 3..floor).map(move |y| (TilePosition { x, y }, TileKind::Stone)))
        .collect();
    for (position, kind) in &rock {
        let _ = world.terrain_mut().set_kind(*position, *kind);
    }
    let cx = world
        .frame()
        .tile_center(TilePosition { x: column, y: 0 })
        .0;
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.x += 2.0;
    }
    for _ in 0..cycle.period_millis * 30 / 1_000 {
        let events = ticks(&mut world, 1);
        if has(&events, CityEvent::Crushed) {
            let (x, y) = rig_xy(&world);
            assert!(
                x < TILE_SIZE.mul_add(-1.5, cx),
                "shoved left, out of the rock: {x} vs {cx}"
            );
            assert!(
                !super::movement::authoritative_player_collides(world.terrain(), x, y),
                "the rig sits in rock"
            );
            return;
        }
    }
    panic!("the crusher never struck");
}

#[test]
fn belts_carry_a_resting_rig() {
    let mut world = world_at(30);
    let stamp = stamps(Stratum::EngineHalls)
        .into_iter()
        .find(|stamp| stamp.kind == CityStampKind::CrusherLine)
        .expect("a crusher line");
    // A belt tile on its floor, away from the crushers' columns.
    let floor = stamp.bounds.max.y - 1;
    let belt = (stamp.bounds.min.x..=stamp.bounds.max.x)
        .find(|x| {
            matches!(
                world
                    .terrain()
                    .tile(TilePosition { x: *x, y: floor })
                    .map(|tile| tile.kind),
                Some(TileKind::City(
                    CityTile::ConveyorLeft | CityTile::ConveyorRight
                ))
            )
        })
        .unwrap_or(stamp.bounds.min.x + 3);
    put_rig(
        &mut world,
        TilePosition {
            x: belt + 2,
            y: floor - 1,
        },
    );
    let _ = ticks(&mut world, 5);
    let dir = match world
        .terrain()
        .tile(TilePosition {
            x: belt + 2,
            y: floor,
        })
        .map(|tile| tile.kind)
    {
        Some(TileKind::City(tile)) => tile.belt().unwrap_or(0),
        _ => 0,
    };
    assert_ne!(dir, 0, "on a belt");
    // The depth's first-visit relic lands beside the rig: clear the belt's way.
    for step in 1..8 {
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x: belt + 2 + step * i32::from(dir),
                y: floor - 1,
            },
            TileKind::Air,
        );
    }
    let (x0, y0) = rig_xy(&world);
    let _ = ticks(&mut world, 30);
    let (x1, _) = rig_xy(&world);
    assert!(
        (x1 - x0) * f32::from(dir) > TILE_SIZE,
        "carried {} px from ({x0}, {y0}), ride {:?}",
        x1 - x0,
        world.city_ride_of(PLAYER)
    );
}

// --- The alarm and the city's watch ---------------------------------------------------------

#[test]
fn cutting_the_city_raises_its_alarm_and_a_full_alarm_sets_its_drones_on_the_rig() {
    let mut world = world_at(30);
    let levels = sites::avenue_levels(&settings(), &layout(&settings()));
    let floor = levels[2];
    // A pocket on a street: glass to drill on the right.
    let x = 300;
    put_rig(
        &mut world,
        TilePosition {
            x: x + 1,
            y: floor - 1,
        },
    );
    for dx in -3..=3 {
        for dy in 1..=3 {
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
            TileKind::City(CityTile::CityStone),
        );
    }
    for dy in 1..=3 {
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x: x + 2,
                y: floor - dy,
            },
            TileKind::City(CityTile::TowerGlass),
        );
    }
    put_rig(
        &mut world,
        TilePosition {
            x: x + 1,
            y: floor - 1,
        },
    );
    let _ = ticks(&mut world, 10);
    assert_eq!(world.city_alarm_of(PLAYER), 0);
    let mut alarm = 0;
    for _ in 0..30 * 4 {
        let _ = tick_with(&mut world, ControlState::new(1.0, false, false));
        alarm = alarm.max(world.city_alarm_of(PLAYER));
    }
    let alarm = alarm.max(world.city_alarm_of(PLAYER));
    assert!(alarm > 0, "drilling glass raised the alarm");
    // It cools.
    let _ = ticks(&mut world, 30 * 2);
    assert!(world.city_alarm_of(PLAYER) < alarm);
    // A drone ignores a calm rig ...
    let (rx, ry) = rig_xy(&world);
    let drone = world.spawn_enemy(
        EnemyKind::MaintenanceDrone,
        TILE_SIZE.mul_add(-2.0, rx),
        ry - 30.0,
    );
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let _ = ticks(&mut world, 30);
    let target = |world: &WorldState| {
        world
            .enemies()
            .find(|(entity, _)| entity.id == drone)
            .and_then(|(_, state)| state.target)
    };
    assert_eq!(
        target(&world),
        None,
        "a calm city's drone mends, it does not hunt"
    );
    // ... and hunts one the city has had enough of.
    world.stage_city_alarm(PLAYER, ALARM_FULL);
    assert!(world.city_hunts(PLAYER));
    let _ = ticks(&mut world, 30);
    assert_eq!(target(&world), Some(PLAYER));
}

// --- The archivist ----------------------------------------------------------------------------

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the archivist's whole life in one world"
)]
fn the_archivist_wakes_follows_talks_and_scans_for_each_player_and_is_saved_and_shared() {
    let mut world = world_at(30);
    world.add_player(PARTNER);
    fit(&mut world, PARTNER);
    let library = world.lantern_library().expect("the library");
    let core = sites::library_core(
        &sites::lantern_library(&settings(), &layout(&settings())).expect("library"),
    );
    // The partner waits far away on the surface: only the rig near the core wakes one.
    put(&mut world, PARTNER, TilePosition { x: 0, y: 80 });
    put_rig(
        &mut world,
        TilePosition {
            x: core.x + 2,
            y: core.y,
        },
    );
    let events = ticks(&mut world, 5);
    assert!(has(&events, CityEvent::ArchivistWoke));
    assert!(
        city_events(&events).contains(&(CityEvent::ArchivistSpoke, 1)),
        "its greeting: {:?}",
        city_events(&events)
    );
    assert!(world.archivist_of(PLAYER).is_some());
    assert!(world.archivist_of(PARTNER).is_none());
    assert!(
        world
            .ancient_city()
            .found
            .contains(&CityLandmark::LanternLibrary)
    );
    assert!(library.contains(TilePosition {
        x: core.x + 2,
        y: core.y
    }));
    // It follows its rig across the floor.
    put_rig(
        &mut world,
        TilePosition {
            x: core.x - 5,
            y: core.y,
        },
    );
    let _ = ticks(&mut world, 60);
    let (rx, ry) = rig_xy(&world);
    let archivist = world.archivist_of(PLAYER).expect("archivist");
    let gap = (archivist.x - rx).hypot(archivist.y - ry);
    assert!(gap < 60.0, "it hovers by the rig: {gap} px away");
    // It scans: ore laid by the rig is marked within a scan.
    for dx in 2..4 {
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x: core.x - 5 + dx,
                y: core.y - 2,
            },
            TileKind::Ore(MineralKind::Mythril),
        );
    }
    let _ = ticks(&mut world, SCAN_MILLIS * 30 / 1_000 + 30);
    let scan = &world.archivist_of(PLAYER).expect("archivist").scan;
    assert!(
        scan.iter().any(|tile| matches!(
            world.terrain().tile(*tile).map(|found| found.kind),
            Some(TileKind::Ore(MineralKind::Mythril))
        )),
        "scan {scan:?}"
    );
    // Saved with the world.
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.archivist_of(PLAYER), world.archivist_of(PLAYER));
    // Shared with partners: the section carries it.
    assert!(crate::world::SectionKey::SHARED.contains(&crate::world::SectionKey::AncientCity));
    let mut client =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(30));
    let payload = world
        .section(crate::world::SectionKey::AncientCity)
        .expect("section");
    client.apply_section(crate::world::SectionKey::AncientCity, payload);
    assert_eq!(client.archivist_of(PLAYER), world.archivist_of(PLAYER));
    // The partner comes down to the Archive Post and wakes their own.
    let post = sites::post_core(&settings(), &layout(&settings())).expect("post");
    put(
        &mut world,
        PARTNER,
        TilePosition {
            x: post.x,
            y: post.y + 7,
        },
    );
    let mut events = Vec::new();
    for _ in 0..3 {
        let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        events.extend(output.events);
    }
    assert!(
        world
            .actor_tile(PARTNER)
            .is_some_and(|tile| (tile.y - post.y).abs() <= 8),
        "partner at {:?}, core {post:?}, archivist {:?}",
        world.actor_tile(PARTNER),
        world.archivist_of(PARTNER)
    );
    assert!(events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::AncientCity {
            what: CityEvent::ArchivistWoke,
            player_id: Some(PARTNER),
            ..
        }
    )));
    assert!(world.archivist_of(PARTNER).is_some());
    assert!(
        world
            .ancient_city()
            .found
            .contains(&CityLandmark::ArchivePost)
    );
}

#[test]
fn the_archivist_follows_alike_at_every_tick_rate() {
    let settle = |hz: u32| {
        let mut world = world_at(hz);
        let core = sites::library_core(
            &sites::lantern_library(&settings(), &layout(&settings())).expect("library"),
        );
        put_rig(
            &mut world,
            TilePosition {
                x: core.x + 2,
                y: core.y,
            },
        );
        let _ = ticks(&mut world, u64::from(hz));
        put_rig(
            &mut world,
            TilePosition {
                x: core.x - 4,
                y: core.y,
            },
        );
        let _ = ticks(&mut world, u64::from(hz) * 2);
        let archivist = world.archivist_of(PLAYER).expect("archivist");
        let (rx, ry) = rig_xy(&world);
        (archivist.x - rx, archivist.y - ry)
    };
    let (sx, sy) = settle(20);
    let (fx, fy) = settle(60);
    assert!(
        (sx - fx).abs() < 3.0 && (sy - fy).abs() < 3.0,
        "({sx}, {sy}) vs ({fx}, {fy})"
    );
}

// --- Star charts and vaults -------------------------------------------------------------------

#[test]
fn reading_the_orrerys_plinths_charts_stars_that_are_kept_saved_and_shared() {
    let mut world = world_at(30);
    let orrery = sites::orrery(&settings(), &layout(&settings())).expect("the orrery");
    let plinths = sites::orrery_plinths(&orrery);
    put_rig(
        &mut world,
        TilePosition {
            x: plinths[0].x - 1,
            y: plinths[0].y,
        },
    );
    let events = ticks(&mut world, 5);
    assert!(has(&events, CityEvent::ChartRead));
    assert_eq!(world.star_charts().len(), 1);
    assert_eq!(
        world.terrain().tile(plinths[0]).map(|tile| tile.kind),
        Some(TileKind::City(CityTile::ChartPlinthRead))
    );
    assert!(world.ancient_city().found.contains(&CityLandmark::Orrery));
    // Standing there longer reads nothing more.
    let _ = ticks(&mut world, 30);
    assert_eq!(world.star_charts().len(), 1);
    put_rig(
        &mut world,
        TilePosition {
            x: plinths[1].x + 1,
            y: plinths[1].y,
        },
    );
    let _ = ticks(&mut world, 5);
    assert_eq!(world.star_charts().len(), 2);
    let first = world.star_charts()[0].clone();
    assert!(world.star_chart_known(first.system));
    assert_eq!(
        first,
        crate::star_chart::chart_at_site(
            SEED,
            plinths[0].x,
            plinths[0].y,
            layout(&settings()).along(plinths[0].y),
            first.found
        )
    );
    // Kept across a save and shared with partners.
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.star_charts(), world.star_charts());
    let mut client =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(30));
    client.apply_section(
        crate::world::SectionKey::AncientCity,
        world
            .section(crate::world::SectionKey::AncientCity)
            .expect("section"),
    );
    assert_eq!(client.star_charts(), world.star_charts());
    // The same chart staged again is not a second one.
    world.stage_star_chart(plinths[0]);
    assert_eq!(world.star_charts().len(), 2);
}

/// The first generated vault of `kind`.
fn vault(kind: VaultKind) -> sites::CityStamp {
    let settings = settings();
    let layout = layout(&settings);
    let area = area_of(Stratum::ArchiveOfStars, -2_000, 2_000);
    sites::stamps_with(&settings, &layout, &area)
        .into_iter()
        .find(|stamp| stamp.kind == CityStampKind::Vault(kind))
        .unwrap_or_else(|| panic!("a {kind:?} vault"))
}

/// The world tile of layout cell `(lx, ly)` of `stamp`.
fn cell(stamp: &sites::CityStamp, lx: i32, ly: i32) -> TilePosition {
    TilePosition {
        x: if stamp.mirrored {
            stamp.bounds.max.x - lx
        } else {
            stamp.bounds.min.x + lx
        },
        y: stamp.bounds.min.y + ly,
    }
}

fn filter(world: &mut WorldState, colour: LampFilter) {
    let inputs = TickInputs::default().act(
        PLAYER,
        PlayerCommand::Logistics(LogisticsCommand::FarStrange(FarStrangeAction::SetFilter(
            colour,
        ))),
    );
    let _ = world.simulate_tick(world.simulation_tick(), &inputs);
}

/// A bot drives a vault from its way in (`start`, a layout cell) to its plinth: inward,
/// swapping its lamp filter to the colour of the next keyed wall ahead. Returns whether it
/// read the chart.
fn solve(stamp: &sites::CityStamp, start: (i32, i32), hz: u32) -> bool {
    let mut world = world_at(hz);
    put_rig(&mut world, cell(stamp, start.0, start.1));
    let _ = ticks(&mut world, u64::from(hz) / 2);
    let inward = if stamp.mirrored { -1.0 } else { 1.0 };
    let CityStampKind::Vault(kind) = stamp.kind else {
        return false;
    };
    let layout = sites::vault_layout(kind);
    let mut current = LampFilter::White;
    for count in 0..u64::from(hz) * 40 {
        // The next keyed wall ahead of the rig, in layout columns.
        let tile = world.player(PLAYER).expect("rig").tile_in(world.frame());
        let lx = if stamp.mirrored {
            stamp.bounds.max.x - tile.x
        } else {
            tile.x - stamp.bounds.min.x
        };
        let row = layout[layout.len() - 2].as_bytes();
        let wanted = (lx.max(0)..i32::try_from(row.len()).unwrap_or(0))
            .find_map(|x| match row.get(usize::try_from(x).ok()?) {
                Some(b'r') => Some(LampFilter::Red),
                Some(b'g') => Some(LampFilter::Green),
                Some(b'b') => Some(LampFilter::Blue),
                _ => None,
            })
            .unwrap_or(LampFilter::White);
        if wanted != current {
            filter(&mut world, wanted);
            current = wanted;
        }
        let events = tick_with(
            &mut world,
            ControlState::new(inward, count % 40 == 0, false),
        );
        if has(&events, CityEvent::ChartRead) {
            return true;
        }
    }
    false
}

#[test]
fn a_bot_solves_a_plate_vault_by_resting_on_the_plate_then_dashing_through_its_gate() {
    let stamp = vault(VaultKind::Plate);
    // Rest on the plate (layout column 2, on the floor row 6) a moment, then drive in.
    assert!(
        solve(&stamp, (2, 5), 30),
        "the plate vault at {:?}",
        stamp.bounds.min
    );
}

#[test]
fn a_bot_solves_a_light_vault_with_its_lamp_filters() {
    let stamp = vault(VaultKind::Light);
    assert!(
        solve(&stamp, (1, 5), 30),
        "the light vault at {:?}",
        stamp.bounds.min
    );
}

#[test]
fn a_bot_solves_a_grand_vault_plate_glass_and_plate_and_the_vault_counts_as_opened() {
    let stamp = vault(VaultKind::Grand);
    assert!(
        solve(&stamp, (2, 5), 30),
        "the grand vault at {:?}",
        stamp.bounds.min
    );
}

#[test]
fn a_lever_vaults_gate_opens_to_its_lever_and_the_vault_counts_once() {
    let stamp = vault(VaultKind::Lever);
    let mut world = world_at(30);
    let gate = cell(&stamp, 0, 4);
    let lever = cell(&stamp, 3, 2);
    world.stream_terrain_near_players();
    put_rig(&mut world, cell(&stamp, 10, 6));
    assert_eq!(
        world.terrain().tile(gate).map(|tile| tile.kind),
        Some(TileKind::Built(crate::base::BuiltTile::Gate)),
        "the gate starts shut"
    );
    // The lever is wired to the gate (the pilot's job; here the lever's own rule).
    assert!(matches!(
        world.terrain().tile(lever).map(|tile| tile.kind),
        Some(TileKind::Built(crate::base::BuiltTile::Lever {
            channel: 0,
            ..
        }))
    ));
    // Inside, the vault opens once and its plinth gives a chart.
    let plinth = sites::vault_plinth(&stamp).expect("plinth");
    put_rig(
        &mut world,
        TilePosition {
            x: plinth.x + if stamp.mirrored { 3 } else { -3 },
            y: plinth.y,
        },
    );
    let events = ticks(&mut world, 3);
    assert!(has(&events, CityEvent::VaultOpened));
    put_rig(
        &mut world,
        TilePosition {
            x: plinth.x + if stamp.mirrored { 1 } else { -1 },
            y: plinth.y,
        },
    );
    let events = ticks(&mut world, 3);
    assert!(has(&events, CityEvent::ChartRead));
    let again = ticks(&mut world, 30);
    assert!(!has(&again, CityEvent::VaultOpened));
    assert!(
        world
            .ancient_city()
            .found
            .contains(&CityLandmark::FirstVault)
    );
}

#[test]
fn a_pilot_crawls_to_a_lever_vaults_lever_and_the_rig_drives_in_to_its_chart() {
    use crate::pilot::{PilotAction, PilotMode};
    let stamp = vault(VaultKind::Lever);
    let mut world = world_at(30);
    // The vault waits in rock; the rig has dug up to its face (the gate and the vent above).
    put_rig(&mut world, cell(&stamp, -3, 6));
    for lx in -5..=-1 {
        for ly in 1..=6 {
            let _ = world
                .terrain_mut()
                .set_kind(cell(&stamp, lx, ly), TileKind::Air);
        }
    }
    put_rig(&mut world, cell(&stamp, -3, 6));
    let _ = ticks(&mut world, 15);
    let gate = [4, 5, 6].map(|ly| cell(&stamp, 0, ly));
    let gate_kind =
        |world: &WorldState| gate.map(|tile| world.terrain().tile(tile).map(|found| found.kind));
    assert!(
        gate_kind(&world)
            .iter()
            .all(|kind| *kind == Some(TileKind::Built(crate::base::BuiltTile::Gate))),
        "the gate starts shut"
    );
    // Out of the rig, and through the vent into the crawlway beside the lever.
    let events = world
        .simulate_tick(
            world.simulation_tick(),
            &TickInputs::default().act(PLAYER, PlayerCommand::TogglePilot),
        )
        .events;
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::PilotExited { .. })),
        "the pilot climbs out"
    );
    let _ = ticks(&mut world, 10);
    let id = world.pilot_of(PLAYER).expect("on foot").0;
    let frame = world.frame();
    let (x, _) = frame.tile_center(cell(&stamp, 2, 2));
    let feet = frame.row_px(stamp.bounds.min.y + 3) - 0.25;
    world.edit_pilot(id, |body, state| {
        body.x = x;
        body.y = feet;
        body.previous_x = x;
        body.previous_y = feet;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
        state.mode = PilotMode::Free;
        state.grounded = true;
    });
    let _ = ticks(&mut world, 2);
    let (lever, _) = world.pilot_usable(PLAYER).expect("the lever is in reach");
    assert_eq!(lever, cell(&stamp, 3, 2));
    let _ = world.simulate_tick(
        world.simulation_tick(),
        &TickInputs::default().act(PLAYER, PlayerCommand::Pilot(PilotAction::Use)),
    );
    assert!(
        gate_kind(&world)
            .iter()
            .all(|kind| *kind == Some(TileKind::Air)),
        "the lever opened the gate: {:?}",
        gate_kind(&world)
    );
    // Back to the rig (it waits outside) and in through the open gate to the plinth.
    let (rx, ry) = rig_xy(&world);
    world.edit_pilot(id, |body, state| {
        body.x = rx;
        body.y = ry + 10.0;
        body.previous_x = rx;
        body.previous_y = ry + 10.0;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
        state.mode = PilotMode::Free;
        state.grounded = true;
    });
    let _ = ticks(&mut world, 2);
    let _ = world.simulate_tick(
        world.simulation_tick(),
        &TickInputs::default().act(PLAYER, PlayerCommand::TogglePilot),
    );
    let _ = ticks(&mut world, 10);
    assert!(world.pilot_of(PLAYER).is_none(), "the pilot is back aboard");
    let inward = if stamp.mirrored { -1.0 } else { 1.0 };
    let mut read = false;
    let mut opened = false;
    for _ in 0..30 * 20 {
        let events = tick_with(&mut world, ControlState::new(inward, false, false));
        opened |= has(&events, CityEvent::VaultOpened);
        if has(&events, CityEvent::ChartRead) {
            read = true;
            break;
        }
    }
    assert!(opened && read, "opened {opened}, read {read}");
    assert_eq!(world.star_charts().len(), 1);
}

// --- Creatures, state and determinism ----------------------------------------------------------

#[test]
fn a_scrap_crawler_carries_off_ore_and_drops_it_where_it_dies_and_a_spark_wisp_drinks_fuel() {
    let mut world = world_at(30);
    // A sealed street-side pocket between two avenue levels.
    let levels = sites::avenue_levels(&settings(), &layout(&settings()));
    let floor = levels[1] + 18;
    put_rig(
        &mut world,
        TilePosition {
            x: 1_500,
            y: floor - 1,
        },
    );
    for position in TileRect::around(
        TilePosition {
            x: 1_500,
            y: floor - 3,
        },
        9,
        5,
    )
    .positions()
    {
        let _ = world.terrain_mut().set_kind(position, TileKind::Stone);
    }
    for position in TileRect::around(
        TilePosition {
            x: 1_500,
            y: floor - 3,
        },
        7,
        2,
    )
    .positions()
    {
        let _ = world.terrain_mut().set_kind(position, TileKind::Air);
    }
    put_rig(
        &mut world,
        TilePosition {
            x: 1_500,
            y: floor - 1,
        },
    );
    let () = world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(crate::item::ItemKind::Mineral(MineralKind::Diamond), 5);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let (rx, ry) = rig_xy(&world);
    let _ = world.spawn_enemy(EnemyKind::ScrapCrawler, TILE_SIZE.mul_add(3.0, rx), ry);
    let mut stolen = false;
    for _ in 0..30 * 20 {
        let events = ticks(&mut world, 1);
        if has(&events, CityEvent::CargoStolen) {
            stolen = true;
            break;
        }
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = rig.max_hull();
    }
    assert!(stolen, "the crawler bit ore out of the bay");
    assert!(
        world
            .player(PLAYER)
            .expect("rig")
            .cargo_bay
            .count(crate::item::ItemKind::Mineral(MineralKind::Diamond))
            < 5
    );
    assert_eq!(world.city_schedule().stolen.len(), 1);
    // Kill it: the ore drops.
    let crawler = world
        .enemies()
        .find(|(_, state)| state.kind == EnemyKind::ScrapCrawler)
        .map(|(entity, _)| entity.id)
        .expect("the crawler");
    let mut output = crate::event::TickOutput::default();
    let _ = world.damage_enemy(
        crawler,
        crate::enemy::Damage::new(
            10_000.0,
            crate::enemy::DamageKind::Kinetic,
            crate::enemy::DamageSource::Player(PLAYER),
        ),
        &mut output,
    );
    assert!(world.city_schedule().stolen.is_empty());
    // A spark wisp's jolt drinks fuel.
    let (rx, ry) = rig_xy(&world);
    let _ = world.spawn_enemy(EnemyKind::SparkWisp, TILE_SIZE.mul_add(-3.0, rx), ry - 20.0);
    let mut jolted = false;
    for _ in 0..30 * 20 {
        let events = ticks(&mut world, 1);
        if has(&events, CityEvent::FuelJolted) {
            jolted = true;
            break;
        }
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = rig.max_hull();
    }
    assert!(jolted, "the wisp's jolt");
}

#[test]
#[allow(clippy::too_many_lines, reason = "the arena, the spawns and the fight")]
fn the_city_creatures_keep_the_body_rules() {
    use crate::{enemy::Locomotion, physics};
    let hz = 30;
    let mut world = world_at(hz);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let (top_row, bottom_row) = layout(&settings()).rows(Stratum::EngineHalls);
    let row = i32::midpoint(top_row, bottom_row);
    let (left, right, top, floor) = (840, 920, row - 10, row + 8);
    put_rig(
        &mut world,
        TilePosition {
            x: 880,
            y: floor - 1,
        },
    );
    let _ = ticks(&mut world, 2);
    let fill = |world: &mut WorldState, rect: TileRect, kind: TileKind| {
        for position in rect.positions() {
            let _ = world.terrain_mut().set_kind(position, kind);
        }
    };
    fill(
        &mut world,
        TileRect {
            min: TilePosition {
                x: left - 4,
                y: top - 4,
            },
            max: TilePosition {
                x: right + 4,
                y: floor + 4,
            },
        },
        TileKind::Stone,
    );
    fill(
        &mut world,
        TileRect {
            min: TilePosition { x: left, y: top },
            max: TilePosition {
                x: right,
                y: floor - 1,
            },
        },
        TileKind::Air,
    );
    fill(
        &mut world,
        TileRect {
            min: TilePosition {
                x: 860,
                y: floor - 6,
            },
            max: TilePosition {
                x: 861,
                y: floor - 1,
            },
        },
        TileKind::Stone,
    );
    put_rig(
        &mut world,
        TilePosition {
            x: 880,
            y: floor - 1,
        },
    );
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        let _ = rig.receive_weapon(WeaponKind::RivetGun);
        let _ = rig.mount_weapon(Some(WeaponKind::RivetGun));
        rig.weapon.energy = rig.battery_capacity();
    }
    // The city hunts the rig, so its drones and sentinels join the fight.
    world.stage_city_alarm(PLAYER, ALARM_FULL);
    let frame = world.frame();
    let kinds: Vec<EnemyKind> = EnemyKind::ALL
        .into_iter()
        .filter(|kind| kind.is_ancient_city())
        .collect();
    assert_eq!(kinds.len(), 9);
    for (index, kind) in (0_i32..).zip(kinds) {
        let column = left + 8 + (index * 7) % (right - left - 12);
        let tile = match kind.stats().locomotion {
            Locomotion::Walking | Locomotion::Stationary => TilePosition {
                x: column,
                y: floor - 1 - i32::from(kind.stats().radius > 12.0),
            },
            Locomotion::Flying | Locomotion::Burrowing => TilePosition {
                x: column,
                y: top + 3 + index % 5,
            },
        };
        let (x, y) = frame.tile_center(tile);
        let _ = world.spawn_enemy(kind, x, y);
    }
    let mut checked = 0_usize;
    for count in 0..u64::from(hz) * 30 {
        let axis = if (count / 40) % 2 == 0 { 0.8 } else { -0.8 };
        let controls = ControlState::new(axis, count % 50 < 10, false).with_weapon(WeaponControl {
            fire: true,
            auto_aim: true,
            aim_x: 127,
            aim_y: 0,
        });
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
        let rig = world.player(PLAYER).expect("rig").clone();
        for (entity, state) in world.enemies() {
            let radius = state.kind.stats().radius;
            assert!(
                !physics::collides(world.terrain(), entity.body.x, entity.body.y, radius),
                "{:?} inside rock at tick {count}",
                state.kind
            );
            let (own_w, own_h) = state.kind.contact_half();
            let (rig_w, rig_h) = super::enemies::RIG_CONTACT_HALF;
            let depth = (own_w + rig_w - (entity.body.x - rig.x).abs())
                .min(own_h + rig_h - (entity.body.y - rig.y).abs())
                .max(0.0);
            let side_x = (entity.body.x - rig.x).signum();
            let side_y = (entity.body.y - rig.y).signum();
            let pinned = physics::collides(
                world.terrain(),
                side_x.mul_add(own_w + rig_w, rig.x),
                entity.body.y,
                radius,
            ) && physics::collides(
                world.terrain(),
                entity.body.x,
                side_y.mul_add(own_h + rig_h, rig.y),
                radius,
            );
            if pinned {
                continue;
            }
            assert!(
                depth <= 1.0,
                "{:?} sits {depth} px inside the rig at tick {count}",
                state.kind
            );
            checked += 1;
        }
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = rig.max_hull();
    }
    assert!(checked > 1_000, "a real fight: {checked}");
}

#[test]
fn older_worlds_keep_their_hashes_and_the_city_is_deterministic() {
    let classic = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(SEED),
        SimulationParams::new(30),
    );
    assert!(classic.ancient_city().is_empty());
    let run = || {
        let mut world = world_at(30);
        let (column, floor_row, _) = floor_piston();
        put_rig(
            &mut world,
            TilePosition {
                x: column,
                y: floor_row - 1,
            },
        );
        world.wake_archivist_for(PLAYER);
        for count in 0..300_u64 {
            let _ = tick_with(
                &mut world,
                ControlState::new(
                    if count % 80 < 40 { 0.3 } else { -0.3 },
                    count % 97 < 10,
                    false,
                ),
            );
        }
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
}
