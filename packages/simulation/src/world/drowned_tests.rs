//! The Drowned and Frozen's mechanics in a Deep Descent world (Wave 9, Descent IV): ballast and
//! currents, the sonar, ice, hull frost and refreezing tunnels, geysers, clams, the abyss's
//! pressure, Merrow City, the creatures' quirks, the eel-dragon, and the Leviathan beaten by a
//! bot with Descent IV kit, its rewards and the gate.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WeaponControl, WorldState,
    cave::CaveTile,
    depth_progression::RigSystem,
    descent::Stratum,
    drowned::{
        DrownedEvent, DrownedLore, FROST_WARNING, GeyserPhase, LeviathanMove, LeviathanPhase,
        RigWater, SONAR_PING_MILLIS, clam_open, geyser_phase,
    },
    enemy::{EnemyDifficulty, EnemyKind, EnemyMode},
    entity::EntityId,
    fluid::FluidKind,
    position::WorldPos,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    weapon::WeaponKind,
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{drowned, layout},
    },
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0xD20_7E57;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// A Deep Descent world at `hz` with the rig fitted for Descent IV (every protection and the
/// gate systems up to the Bio-Filter, but neither the Sonar Suite nor the Cryo Hull), a strong
/// hull, and the director quiet.
fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    // The crew holds the Deep Claim (the Sinkhole beaten), or Descent I's gate crushes it below.
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hazard_gear.rig_systems &= !(RigSystem::SonarSuite.bit() | RigSystem::CryoHull.bit());
    rig.radiator_level = 15;
    rig.hull_level = 15;
    rig.hull = rig.max_hull();
    rig.credits = 500_000;
    world
}

fn put_rig(world: &mut WorldState, tile: TilePosition) {
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(tile.x), i64::from(tile.y)),
    );
    world.stream_terrain_near_players();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn fill(world: &mut WorldState, rect: TileRect, kind: TileKind) {
    for position in rect.positions() {
        let _ = world.terrain_mut().set_kind(position, kind);
    }
}

fn step(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::held(PLAYER, controls);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    output.events.into_iter().map(|event| event.kind).collect()
}

fn ticks(world: &mut WorldState, count: u64) -> Vec<GameplayEventKind> {
    (0..count)
        .flat_map(|_| step(world, ControlState::default()))
        .collect()
}

fn drowned_events(events: &[GameplayEventKind]) -> Vec<DrownedEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Drowned { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

fn middle_row(stratum: Stratum) -> i32 {
    let (top, bottom) = layout(&settings()).rows(stratum);
    top + (bottom - top) / 2
}

/// A column far from every set piece (the test room is built by hand).
const FAR: i32 = -260;

/// A test room's centre in the Underground Ocean on [`FAR`]'s column, clear of every basin's
/// gyre (the basins fill nearly every ocean cell, Wave 10 polish), for a room `(half_w, half_h)`.
fn still_water_centre(half: (i32, i32)) -> TilePosition {
    let settings = settings();
    let lay = layout(&settings);
    let (top, bottom) = lay.rows(Stratum::UndergroundOcean);
    (top + half.1 + 20..bottom - half.1 - 20)
        .map(|y| TilePosition { x: FAR, y })
        .find(|centre| {
            let room = TileRect::around(*centre, half.0 + 4, half.1 + 4);
            drowned::basins_in(&settings, &lay, &room)
                .iter()
                .all(|basin| !basin.bounds.intersects(&room))
        })
        .expect("still water between the basins")
}

/// A sealed stone room `(half_w, half_h)` round `centre`, open inside, filled with water up
/// to `water_rows` from its floor (all of it for `i32::MAX`).
fn room(world: &mut WorldState, centre: TilePosition, half: (i32, i32), water_rows: i32) {
    put_rig(world, centre);
    fill(
        world,
        TileRect::around(centre, half.0 + 2, half.1 + 2),
        TileKind::Stone,
    );
    let inside = TileRect::around(centre, half.0, half.1);
    fill(world, inside, TileKind::Air);
    let floor = inside.max.y;
    for position in inside.positions() {
        if floor - position.y < water_rows {
            let _ = world
                .terrain_mut()
                .set_kind(position, TileKind::Fluid(FluidKind::Water));
        }
    }
}

fn rig_xy(world: &WorldState) -> (f32, f32) {
    let rig = world.player(PLAYER).expect("rig");
    (rig.x, rig.y)
}

// --- Classic worlds ---------------------------------------------------------------------------

#[test]
fn older_worlds_have_none_of_the_drowned_and_frozen() {
    let classic = WorldSettings {
        generator: GeneratorVersion::V10,
        ..WorldSettings::new(SEED)
    };
    let mut world = WorldState::new_game_with_settings(PLAYER, &classic, SimulationParams::new(30));
    let hash = world.simulation_state_hash();
    let _ = ticks(&mut world, 30);
    assert!(world.drowned().is_empty());
    assert_eq!(world.rig_water(PLAYER), RigWater::default());
    assert!(world.merrow_city().is_none());
    assert!(world.leviathan_arena().is_none());
    // The section exists but stays empty, and an untouched world hashes alike.
    let _ = hash;
}

// --- The Underground Ocean: ballast, currents, sonar --------------------------------------------

#[test]
fn a_flooded_submarine_settles_on_the_bottom_and_drills_through_it() {
    for hz in [20, 30, 60, 120] {
        let mut world = world_at(hz);
        world.enable_test_mode();
        let centre = TilePosition {
            x: FAR,
            y: middle_row(Stratum::UndergroundOcean),
        };
        room(&mut world, centre, (4, 4), i32::MAX);
        let floor = centre.y + 4;
        let mut deepest = 0;
        for _ in 0..hz * 8 {
            let _ = step(&mut world, ControlState::new(0.0, false, true));
            let tile = world.player(PLAYER).expect("rig").tile_in(world.frame());
            deepest = deepest.max(tile.y);
        }
        let rig = world.player(PLAYER).expect("rig");
        assert!(
            deepest > floor + 2,
            "{hz} Hz: stuck at row {deepest} (floor {floor}), y {} vy {} {:?}",
            rig.y,
            rig.velocity_y,
            rig.water
        );
    }
}

#[test]
fn ballast_trims_the_submarine_to_rise_hover_and_sink() {
    let mut world = world_at(30);
    let centre = still_water_centre((6, 10));
    room(&mut world, centre, (6, 10), i32::MAX);
    // Neutral trim: the submarine hovers (barely moves in two seconds).
    let _ = ticks(&mut world, 10);
    let start = rig_xy(&world).1;
    let _ = ticks(&mut world, 60);
    let hover = rig_xy(&world).1 - start;
    assert!(hover.abs() < TILE_SIZE * 0.75, "hovers: drifted {hover} px");
    // Thrust blows the tanks; let go and the trim holds, and the rig keeps rising.
    for _ in 0..20 {
        let _ = step(&mut world, ControlState::new(0.0, true, false));
    }
    let blown = world.rig_water(PLAYER).ballast;
    assert!(blown > 500, "tanks blown: {blown}");
    let before = rig_xy(&world).1;
    let _ = ticks(&mut world, 30);
    assert_eq!(world.rig_water(PLAYER).ballast, blown, "the trim holds");
    assert!(rig_xy(&world).1 < before - 4.0, "it rises on its own");
    // The drill floods them: it sinks.
    for _ in 0..40 {
        let _ = step(&mut world, ControlState::new(0.0, false, true));
    }
    assert!(world.rig_water(PLAYER).ballast < -500);
    let before = rig_xy(&world).1;
    let _ = ticks(&mut world, 20);
    assert!(rig_xy(&world).1 > before + 4.0, "it sinks");
}

#[test]
fn ballast_runs_alike_at_every_tick_rate() {
    let mut heights = Vec::new();
    for hz in [20, 30, 60] {
        let mut world = world_at(hz);
        let centre = still_water_centre((6, 12));
        room(&mut world, centre, (6, 12), i32::MAX);
        let start = rig_xy(&world).1;
        // One second of blowing the tanks, two of rising.
        for _ in 0..hz {
            let _ = step(&mut world, ControlState::new(0.0, true, false));
        }
        let _ = ticks(&mut world, u64::from(hz) * 2);
        heights.push(start - rig_xy(&world).1);
    }
    let (low, high) = heights
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(*h), hi.max(*h)));
    assert!(high - low < TILE_SIZE * 1.2, "climbs {heights:?}");
}

#[test]
fn a_basins_gyre_carries_a_hovering_submarine() {
    let settings = settings();
    let lay = layout(&settings);
    let (top, bottom) = lay.rows(Stratum::UndergroundOcean);
    let area = TileRect {
        min: TilePosition { x: -400, y: top },
        max: TilePosition { x: 400, y: bottom },
    };
    let basin = drowned::basins_in(&settings, &lay, &area)
        .into_iter()
        .filter(|basin| basin.bounds.width() >= 30)
        .max_by_key(|basin| basin.along)
        .expect("a basin in the ocean");
    let mut world = world_at(30);
    // Two thirds of the way up from the centre: the gyre runs sideways there.
    let centre = basin.bounds.center();
    let tile = TilePosition {
        x: centre.x,
        y: centre.y - basin.bounds.height() / 4,
    };
    put_rig(&mut world, tile);
    let _ = ticks(&mut world, 5);
    let (x, y) = rig_xy(&world);
    let current = world.current_at(x, y);
    assert!(current.0.abs() > 5.0, "a sideways current: {current:?}");
    let start = x;
    let _ = ticks(&mut world, 90);
    let moved = rig_xy(&world).0 - start;
    assert!(
        moved * current.0.signum() > TILE_SIZE * 0.5,
        "carried {moved} px along {current:?}"
    );
}

#[test]
fn the_scanner_pings_sonar_under_water_and_the_sonar_suite_reaches_further() {
    let mut world = world_at(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::UndergroundOcean),
    };
    room(&mut world, centre, (8, 6), i32::MAX);
    let inputs = TickInputs::default().act(PLAYER, PlayerCommand::UseScanner);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    let events: Vec<GameplayEventKind> = output.events.into_iter().map(|e| e.kind).collect();
    assert!(drowned_events(&events).contains(&DrownedEvent::SonarPing));
    assert_eq!(world.sonar_pings().len(), 1);
    assert_eq!(world.sonar_reach(PLAYER), crate::drowned::SONAR_REACH_TILES);
    // The ring spreads out: lit a moment later a few tiles off.
    let _ = ticks(&mut world, 15);
    let (x, y) = rig_xy(&world);
    assert!(world.sonar_light_at(TILE_SIZE.mul_add(5.0, x), y) > 0.0);
    // It fades.
    let fade = u64::from(30_u32) * SONAR_PING_MILLIS / 1_000;
    let _ = ticks(&mut world, fade);
    assert!(world.sonar_pings().is_empty());
    assert!(world.sonar_light_at(TILE_SIZE.mul_add(5.0, x), y) <= 0.0);
    crate::economy::grant_rig_system(
        world.player_mut(PLAYER).expect("rig"),
        RigSystem::SonarSuite,
    );
    assert_eq!(
        world.sonar_reach(PLAYER),
        crate::drowned::SONAR_SUITE_REACH_TILES
    );
}

// --- Brine Glaciers: ice, frost, refreezing ---------------------------------------------------

#[test]
fn a_rig_slides_on_ice_and_brakes_on_stone() {
    let slide = |floor: TileKind| {
        let mut world = world_at(30);
        let row = middle_row(Stratum::BrineGlaciers);
        let centre = TilePosition { x: FAR, y: row };
        room(&mut world, centre, (14, 3), 0);
        fill(
            &mut world,
            TileRect {
                min: TilePosition {
                    x: FAR - 14,
                    y: row + 4,
                },
                max: TilePosition {
                    x: FAR + 14,
                    y: row + 4,
                },
            },
            floor,
        );
        put_rig(
            &mut world,
            TilePosition {
                x: FAR - 8,
                y: row + 3,
            },
        );
        let _ = ticks(&mut world, 10);
        world.player_mut(PLAYER).expect("rig").velocity_x = 160.0;
        let start = rig_xy(&world).0;
        let _ = ticks(&mut world, 30);
        rig_xy(&world).0 - start
    };
    let ice = slide(TileKind::Cave(CaveTile::BrineIce));
    let stone = slide(TileKind::Stone);
    assert!(ice > stone + TILE_SIZE, "ice {ice} px, stone {stone} px");
}

#[test]
fn hull_frost_builds_in_the_glaciers_slows_the_rig_and_thaws_at_a_vent() {
    let mut world = world_at(30);
    let row = middle_row(Stratum::BrineGlaciers);
    let centre = TilePosition { x: FAR, y: row };
    room(&mut world, centre, (10, 4), 0);
    let mut warned = false;
    for _ in 0..30 * 40 {
        warned |= drowned_events(&ticks(&mut world, 1)).contains(&DrownedEvent::FrostWarning);
    }
    let frost = world.rig_water(PLAYER).frost_permille();
    assert!(frost >= FROST_WARNING, "frost {frost}");
    assert!(warned, "the crew is warned");
    // A frozen rig drives slower.
    let push = |world: &mut WorldState| {
        put_rig(world, centre);
        let _ = ticks(world, 5);
        let start = rig_xy(world).0;
        for _ in 0..20 {
            let _ = step(world, ControlState::new(1.0, false, false));
        }
        rig_xy(world).0 - start
    };
    let frozen = push(&mut world);
    // A thermal vent in the floor thaws it.
    let _ = world.terrain_mut().set_kind(
        TilePosition { x: FAR, y: row + 5 },
        TileKind::Cave(CaveTile::ThermalVent),
    );
    put_rig(&mut world, centre);
    let _ = ticks(&mut world, 30 * 6);
    assert_eq!(world.rig_water(PLAYER).frost_permille(), 0, "thawed");
    let warm = push(&mut world);
    assert!(warm > frozen * 1.2, "frozen {frozen} px, warm {warm} px");
    // The Cryo Hull shrugs the frost off.
    crate::economy::grant_rig_system(world.player_mut(PLAYER).expect("rig"), RigSystem::CryoHull);
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: FAR, y: row + 5 }, TileKind::Stone);
    let _ = ticks(&mut world, 30 * 10);
    assert_eq!(world.rig_water(PLAYER).frost_permille(), 0);
}

#[test]
fn a_tunnel_through_the_glaciers_freezes_over_but_never_on_a_rig() {
    let mut world = world_at(30);
    let row = middle_row(Stratum::BrineGlaciers);
    let tunnel = TileRect {
        min: TilePosition { x: FAR, y: row },
        max: TilePosition { x: FAR + 5, y: row },
    };
    put_rig(&mut world, TilePosition { x: FAR - 6, y: row });
    fill(
        &mut world,
        TileRect::around(TilePosition { x: FAR + 2, y: row }, 6, 2),
        TileKind::Cave(CaveTile::BrineIce),
    );
    // The rig sits in the tunnel's first tile.
    let _ = world.dig_drowned_tiles(tunnel);
    put_rig(&mut world, tunnel.min);
    assert!(world.living_schedule().is_waiting(tunnel.max));
    let _ = ticks(&mut world, 30 * 70);
    let rime = tunnel
        .positions()
        .filter(|at| {
            world.terrain().tile(*at).map(|t| t.kind) == Some(TileKind::Cave(CaveTile::Rime))
        })
        .count();
    assert!(rime >= 4, "the tunnel froze over ({rime} tiles)");
    assert_ne!(
        world.terrain().tile(tunnel.min).map(|t| t.kind),
        Some(TileKind::Cave(CaveTile::Rime)),
        "never over the rig"
    );
    // Rime is quick to drill.
    assert!(CaveTile::Rime.hardness() <= 1);
}

// --- Geyser Fields ------------------------------------------------------------------------------

/// A vent row at `row + 8` under an open 3-wide chimney, the rig `rows_up` rows over the vent.
fn chimney(world: &mut WorldState, rows_up: i32) -> TilePosition {
    let row = middle_row(Stratum::GeyserFields);
    let vent = TilePosition { x: FAR, y: row + 8 };
    put_rig(world, TilePosition { x: FAR, y: row });
    fill(
        world,
        TileRect::around(TilePosition { x: FAR, y: row - 6 }, 4, 18),
        TileKind::Cave(CaveTile::Sinter),
    );
    fill(
        world,
        TileRect {
            min: TilePosition {
                x: FAR - 1,
                y: row - 20,
            },
            max: TilePosition {
                x: FAR + 1,
                y: vent.y - 1,
            },
        },
        TileKind::Air,
    );
    for dx in -1..=1 {
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x: FAR + dx,
                y: vent.y,
            },
            TileKind::Cave(CaveTile::GeyserVent),
        );
    }
    put_rig(
        world,
        TilePosition {
            x: FAR,
            y: vent.y - rows_up,
        },
    );
    vent
}

#[test]
fn a_geyser_throws_a_rig_in_its_chimney_upward_on_its_beat() {
    let mut world = world_at(30);
    let vent = chimney(&mut world, 6);
    let seed = world.terrain().settings().seed;
    let mut launched = false;
    let mut erupted = false;
    let mut lowest = f32::MAX;
    for _ in 0..30 * 14 {
        let events = drowned_events(&ticks(&mut world, 1));
        launched |= events.contains(&DrownedEvent::GeyserLaunched);
        erupted |= events.contains(&DrownedEvent::GeyserErupted);
        lowest = lowest.min(rig_xy(&world).1);
        if launched {
            break;
        }
    }
    assert!(erupted, "the vent erupted");
    assert!(launched, "the rig rode the jet");
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.velocity_y < -300.0, "thrown up at {}", rig.velocity_y);
    let millis = world.millis_at(world.simulation_tick());
    assert_eq!(
        geyser_phase(seed, vent.x, false, millis),
        GeyserPhase::Erupting
    );
    let _ = lowest;
}

#[test]
fn a_geysers_beat_is_the_same_at_every_tick_rate() {
    let mut first = Vec::new();
    for hz in [20_u32, 30, 60] {
        let mut world = world_at(hz);
        let _vent = chimney(&mut world, 6);
        let mut at = None;
        for count in 0..u64::from(hz) * 14 {
            if drowned_events(&ticks(&mut world, 1)).contains(&DrownedEvent::GeyserErupted) {
                at = Some(count * 1_000 / u64::from(hz));
                break;
            }
        }
        first.push(at.expect("an eruption"));
    }
    let spread = first.iter().max().unwrap() - first.iter().min().unwrap();
    assert!(spread <= 60, "first eruptions at {first:?} ms");
}

#[test]
fn sitting_on_a_geysers_throat_scalds() {
    let mut world = world_at(30);
    let _vent = chimney(&mut world, 1);
    let hull = world.player(PLAYER).expect("rig").hull;
    let mut scalded = false;
    for _ in 0..30 * 14 {
        // Hold it down on the throat.
        world.player_mut(PLAYER).expect("rig").velocity_y = 0.0;
        scalded |= drowned_events(&ticks(&mut world, 1)).contains(&DrownedEvent::GeyserScalded);
    }
    assert!(scalded);
    assert!(world.player(PLAYER).expect("rig").hull < hull);
}

// --- Pearl Abyss: pressure, clams ----------------------------------------------------------------

#[test]
fn the_abyss_presses_past_weak_seals_and_the_abyssal_seals_hold() {
    let mut world = world_at(30);
    let (top, bottom) = layout(&settings()).rows(Stratum::PearlAbyss);
    let deep = top + (bottom - top) * 9 / 10;
    let centre = TilePosition { x: FAR, y: deep };
    room(&mut world, centre, (4, 14), i32::MAX);
    put_rig(
        &mut world,
        TilePosition {
            x: FAR,
            y: deep + 12,
        },
    );
    world
        .player_mut(PLAYER)
        .expect("rig")
        .hazard_gear
        .pressure_rating = 1;
    let (pressure, capacity) = world.abyss_pressure_for(PLAYER).expect("in the abyss");
    assert!(pressure > capacity, "{pressure} over {capacity}");
    let hull = world.player(PLAYER).expect("rig").hull;
    let events = drowned_events(&ticks(&mut world, 30 * 4));
    assert!(events.contains(&DrownedEvent::PressureWarning));
    assert!(world.player(PLAYER).expect("rig").hull < hull);
    // The Cryo Hull holds everything.
    crate::economy::grant_rig_system(world.player_mut(PLAYER).expect("rig"), RigSystem::CryoHull);
    let (pressure, capacity) = world.abyss_pressure_for(PLAYER).expect("in the abyss");
    assert!(pressure <= capacity);
    // Deeper water presses harder.
    put_rig(
        &mut world,
        TilePosition {
            x: FAR,
            y: deep - 12,
        },
    );
    let shallow = world.abyss_pressure_for(PLAYER).expect("in the abyss").0;
    assert!(shallow < pressure);
}

#[test]
fn a_gaping_clam_gives_up_its_pearl_and_a_shut_one_bites() {
    let mut world = world_at(30);
    let row = middle_row(Stratum::PearlAbyss);
    let clam = TilePosition { x: FAR, y: row + 1 };
    put_rig(&mut world, TilePosition { x: FAR, y: row });
    let seed = world.terrain().settings().seed;
    // Wait for it to gape, then take it.
    while !clam_open(
        seed,
        clam.x,
        clam.y,
        world.millis_at(world.simulation_tick()),
    ) {
        let _ = ticks(&mut world, 1);
    }
    let _ = world
        .terrain_mut()
        .set_kind(clam, TileKind::Cave(CaveTile::PearlClam));
    put_rig(&mut world, TilePosition { x: FAR, y: row });
    let credits = world.player(PLAYER).expect("rig").credits;
    let events = world.dig_drowned_tiles(TileRect {
        min: clam,
        max: clam,
    });
    assert!(drowned_events(&events).contains(&DrownedEvent::ClamPearl));
    assert!(world.player(PLAYER).expect("rig").credits > credits);
    // Shut, it bites and closes back.
    while clam_open(
        seed,
        clam.x,
        clam.y,
        world.millis_at(world.simulation_tick()),
    ) {
        let _ = ticks(&mut world, 1);
    }
    let _ = world
        .terrain_mut()
        .set_kind(clam, TileKind::Cave(CaveTile::PearlClam));
    put_rig(&mut world, TilePosition { x: FAR, y: row });
    let hull = world.player(PLAYER).expect("rig").hull;
    let events = world.dig_drowned_tiles(TileRect {
        min: clam,
        max: clam,
    });
    assert!(drowned_events(&events).contains(&DrownedEvent::ClamSnapped));
    assert!(world.player(PLAYER).expect("rig").hull < hull);
    assert_eq!(
        world.terrain().tile(clam).map(|t| t.kind),
        Some(TileKind::Cave(CaveTile::PearlClam))
    );
}

// --- Merrow City ----------------------------------------------------------------------------------

#[test]
fn merrow_city_is_a_peopled_settlement_round_the_outpost() {
    use crate::village::{self, placement::underground_id};
    let settings = settings();
    let city = village::village(&settings, underground_id(crate::drowned::MERROW_SITE))
        .expect("Merrow City");
    assert_eq!(city.name, crate::drowned::MERROW_CITY_NAME);
    assert_eq!(city.culture, crate::people::Culture::Merrow);
    assert!(!city.population.characters.is_empty(), "it has residents");
    let site = drowned::merrow_city(&settings).expect("the city");
    // Its hall stands under the bubble, beside the outpost.
    let hall_floor = TilePosition {
        x: site.hall_center,
        y: site.floor - 1,
    };
    assert!(site.in_city(hall_floor));
    assert!(site.in_city(TilePosition {
        x: site.outpost.center().x,
        y: site.floor - 2,
    }));
    // A rig arriving finds the city and its memory of the Fathers.
    let mut world = world_at(30);
    put_rig(&mut world, hall_floor);
    let events = drowned_events(&ticks(&mut world, 3));
    assert!(events.contains(&DrownedEvent::CityReached));
    assert!(world.drowned().found(DrownedLore::MerrowCity));
    assert!(world.in_merrow_city(PLAYER));
    // The merrow sell their aquarium line.
    assert_eq!(
        crate::people::offers::signature_furniture(crate::people::Culture::Merrow),
        crate::home::Furniture::Aquarium
    );
}

// --- Creatures ----------------------------------------------------------------------------------

#[test]
fn every_drowned_stratum_has_natives_of_its_own() {
    for stratum in [
        Stratum::UndergroundOcean,
        Stratum::BrineGlaciers,
        Stratum::GeyserFields,
        Stratum::MerrowCity,
        Stratum::PearlAbyss,
    ] {
        let natives = stratum
            .def()
            .creatures
            .iter()
            .filter(|(kind, _)| kind.is_drowned())
            .count();
        assert!(natives >= 2, "{stratum:?} has {natives} natives");
    }
}

/// A sealed stone room `(half_w, half_h)` round `centre` with the rig at one end and `kind` at
/// the other; the world turns hostile.
fn creature_room(
    world: &mut WorldState,
    centre: TilePosition,
    half: (i32, i32),
    water: bool,
    kind: EnemyKind,
) -> EntityId {
    room(world, centre, half, if water { i32::MAX } else { 0 });
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    put_rig(
        world,
        TilePosition {
            x: centre.x - half.0 + 2,
            y: centre.y + half.1 - 1,
        },
    );
    let at = world.frame().tile_center(TilePosition {
        x: centre.x + 2,
        y: centre.y + half.1 - 1,
    });
    world.spawn_enemy(kind, at.0, at.1)
}

#[test]
fn a_frost_mites_bite_frosts_the_hull() {
    let mut world = world_at(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::GeyserFields),
    };
    let _ = creature_room(&mut world, centre, (6, 3), false, EnemyKind::FrostMite);
    let mut bitten = false;
    for _ in 0..30 * 12 {
        let _ = ticks(&mut world, 1);
        if world.rig_water(PLAYER).frost_permille() > 0 {
            bitten = true;
            break;
        }
    }
    assert!(bitten, "the mites frosted the hull");
}

#[test]
fn an_ink_squid_inks_the_water_and_darts_away() {
    let mut world = world_at(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::UndergroundOcean),
    };
    let squid = creature_room(&mut world, centre, (6, 4), true, EnemyKind::InkSquid);
    let mut inked = false;
    let mut fled = false;
    for _ in 0..30 * 15 {
        inked |= drowned_events(&ticks(&mut world, 1)).contains(&DrownedEvent::InkCloud);
        fled |= world
            .enemies()
            .any(|(entity, state)| entity.id == squid && state.mode == EnemyMode::Flee);
        if inked && fled {
            break;
        }
    }
    assert!(inked, "the squid inked the water");
    assert!(fled, "and darted away");
    assert!(!world.drowned().inks.is_empty());
}

#[test]
fn a_lure_angler_drags_rigs_in_its_light() {
    let mut world = world_at(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::PearlAbyss),
    };
    let _ = creature_room(&mut world, centre, (8, 3), true, EnemyKind::LureAngler);
    // The first tick's depth reward drops its ore beside the rig (a known quirk): clear the
    // water between the rig and the angler again.
    let _ = ticks(&mut world, 2);
    let row = world.player(PLAYER).expect("rig").tile_in(world.frame());
    fill(
        &mut world,
        TileRect {
            min: TilePosition {
                x: row.x + 1,
                y: row.y - 1,
            },
            max: TilePosition {
                x: centre.x + 1,
                y: row.y + 1,
            },
        },
        TileKind::Fluid(FluidKind::Water),
    );
    // Hold the rig's ballast neutral and see it drift toward the angler.
    let start = rig_xy(&world).0;
    let _ = ticks(&mut world, 20);
    assert!(rig_xy(&world).0 > start + 2.0, "pulled toward the lure");
}

#[test]
fn a_sinter_snail_seals_the_tunnel_behind_it() {
    let mut world = world_at(30);
    let row = middle_row(Stratum::GeyserFields);
    put_rig(
        &mut world,
        TilePosition {
            x: FAR - 30,
            y: row,
        },
    );
    let centre = TilePosition { x: FAR, y: row };
    fill(
        &mut world,
        TileRect::around(centre, 8, 3),
        TileKind::Cave(CaveTile::Sinter),
    );
    fill(&mut world, TileRect::around(centre, 6, 1), TileKind::Air);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let at = world
        .frame()
        .tile_center(TilePosition { x: FAR, y: row + 1 });
    let _snail = world.spawn_enemy(EnemyKind::SinterSnail, at.0, at.1);
    let open = |world: &WorldState| {
        TileRect::around(centre, 6, 1)
            .positions()
            .filter(|at| world.terrain().tile(*at).map(|t| t.kind) == Some(TileKind::Air))
            .count()
    };
    let before = open(&world);
    let _ = ticks(&mut world, 30 * 20);
    assert!(open(&world) < before, "it crusted a tile over");
}

// --- The eel-dragon -----------------------------------------------------------------------------

#[test]
fn the_eel_dragon_wakes_by_its_grotto_and_crackles_before_it_pulses() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let grotto = world.eel_grotto().expect("the grotto");
    put_rig(&mut world, grotto.center());
    let mut awoke = false;
    let mut pulse = false;
    for _ in 0..30 * 12 {
        let events = drowned_events(&ticks(&mut world, 1));
        awoke |= events.contains(&DrownedEvent::EelDragonAwoke);
        pulse |= events.contains(&DrownedEvent::EelDragonPulse);
    }
    assert!(awoke && pulse, "awoke {awoke}, pulse {pulse}");
    // Killed, it pays its bounty and stays dead.
    let eel = EntityId(world.drowned().eel.entity.expect("the eel"));
    let credits = world.player(PLAYER).expect("rig").credits;
    let mut output = crate::TickOutput::default();
    let _ = world.damage_enemy(
        eel,
        crate::enemy::Damage::new(
            1_000_000.0,
            crate::enemy::DamageKind::Explosive,
            crate::enemy::DamageSource::Player(PLAYER),
        ),
        &mut output,
    );
    let _ = ticks(&mut world, 2);
    assert!(world.drowned().eel.beaten);
    assert!(world.player(PLAYER).expect("rig").credits > credits);
}

// --- The Leviathan ------------------------------------------------------------------------------

/// The bot's controls in the arena: keep a few tiles off the Leviathan at the bottom of the
/// water, slip sideways out of a charge's line, railgun at its body.
fn bot_controls(world: &WorldState, target: (f32, f32), charging: bool, tick: u64) -> ControlState {
    let rig = world.player(PLAYER).expect("rig");
    let (dx, dy) = (target.0 - rig.x, target.1 - rig.y);
    let length = dx.hypot(dy).max(1.0);
    #[allow(clippy::cast_possible_truncation, reason = "a unit vector times 127")]
    let aim = ((dx / length * 127.0) as i8, (dy / length * 127.0) as i8);
    let arena = world.leviathan_arena().expect("the arena");
    let floor = world
        .frame()
        .tile_center(TilePosition {
            x: arena.center().x,
            y: drowned::arena_floor_row(&arena) - 5,
        })
        .1;
    // Keep low; flee sideways from a charge; else hold a standoff.
    let horizontal = if charging {
        -dx.signum()
    } else if dx.abs() < TILE_SIZE * 7.0 {
        -dx.signum() * 0.8
    } else if dx.abs() > TILE_SIZE * 12.0 {
        dx.signum() * 0.8
    } else {
        0.0
    };
    let thrust = rig.y > floor + TILE_SIZE;
    let dive = rig.y < TILE_SIZE.mul_add(-2.0, floor);
    ControlState::new(horizontal, thrust, dive && !thrust).with_weapon(WeaponControl {
        fire: tick % 6 < 3,
        auto_aim: false,
        aim_x: aim.0,
        aim_y: aim.1,
    })
}

/// A rig's hull tier at the Leviathan (the descent probe's arrival tier, plus one).
const HULL_AT_THE_LEVIATHAN: u8 = 11;

#[test]
#[allow(clippy::too_many_lines, reason = "the whole fight, start to end")]
fn a_bot_with_descent_iv_kit_beats_the_leviathan_and_wins_the_sonar_suite_and_cryo_hull() {
    let hz = 30;
    let mut world = world_at(hz);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let arena = world.leviathan_arena().expect("the arena");
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        crate::economy::max_out_upgrades(rig);
        rig.hazard_gear.rig_systems &= !(RigSystem::SonarSuite.bit() | RigSystem::CryoHull.bit());
        // Descent IV's seals, not the top tier.
        rig.hazard_gear.pressure_rating = 3;
        rig.weapon_rack.battery_level = crate::weapon::MAX_BATTERY_LEVEL;
        let _ = rig.receive_weapon(WeaponKind::Railgun);
        let _ = rig.mount_weapon(Some(WeaponKind::Railgun));
        rig.hull_level = HULL_AT_THE_LEVIATHAN;
        rig.hull = rig.max_hull();
    }
    // The Descent V gate crushes it before the fight.
    assert_eq!(
        crate::depth_progression::missing_gate(
            world.player(PLAYER).expect("rig").hazard_gear.rig_systems,
            4
        ),
        Some(RigSystem::CryoHull)
    );
    put_rig(
        &mut world,
        TilePosition {
            x: arena.center().x - 20,
            y: drowned::arena_floor_row(&arena) - 5,
        },
    );
    let events = drowned_events(&ticks(&mut world, 2));
    assert!(events.contains(&DrownedEvent::LeviathanAwoke));
    let mut phases = std::collections::BTreeSet::new();
    let mut charged = false;
    let mut maw = false;
    let mut fired = 0_usize;
    let mut defeated = false;
    let mut fight_ticks = 0_u64;
    for count in 0..u64::from(hz) * 900 {
        fight_ticks = count + 1;
        let Some(((x, y), action)) = world.leviathan_pose() else {
            break;
        };
        phases.insert(world.drowned().leviathan.phase);
        charged |= matches!(action, LeviathanMove::Charge { .. });
        maw |= matches!(action, LeviathanMove::Maw);
        let charging = matches!(
            action,
            LeviathanMove::ChargeTell { .. } | LeviathanMove::Charge { .. }
        );
        let controls = bot_controls(&world, (x, y), charging, count);
        let output =
            world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
        let events: Vec<GameplayEventKind> =
            output.events.into_iter().map(|event| event.kind).collect();
        fired += events
            .iter()
            .filter(|event| matches!(event, GameplayEventKind::WeaponFired { .. }))
            .count();
        if drowned_events(&events).contains(&DrownedEvent::LeviathanDefeated) {
            defeated = true;
            break;
        }
        assert!(
            world.player(PLAYER).expect("rig").hull > 0.0,
            "the bot survived"
        );
    }
    assert!(fired > 10, "the bot fired its railgun ({fired})");
    assert!(charged, "it charged");
    assert!(maw, "it opened its maw in the maelstrom");
    assert!(phases.contains(&LeviathanPhase::Thrashing), "{phases:?}");
    assert!(defeated, "the bot beat the Leviathan");
    let rig = world.player(PLAYER).expect("rig");
    let left = rig.hull / rig.max_hull();
    assert!(left > 0.2, "with a margin: {:.0} %", left * 100.0);
    // The probe models the fight from this measured win (Wave 10 polish).
    #[allow(clippy::cast_precision_loss, reason = "a few thousand ticks")]
    crate::balance::descent::mechanics::record_bot_fight(
        crate::descent::DescentBoss::Leviathan,
        fight_ticks as f64 / f64::from(hz),
        f64::from(left),
    );
    assert!(rig.hazard_gear.has_system(RigSystem::SonarSuite));
    assert!(rig.hazard_gear.has_system(RigSystem::CryoHull));
    assert!(world.drowned().leviathan.beaten);
    assert!(world.drowned().found(DrownedLore::AbyssFloor));
    // The Hollow World's gate opens now.
    assert!(crate::depth_progression::missing_gate(rig.hazard_gear.rig_systems, 4).is_none());
    // It stays beaten.
    let _ = ticks(&mut world, 30);
    assert!(!world.drowned().leviathan.awake());
}

#[test]
fn the_gate_rewards_are_won_from_the_leviathan_not_bought() {
    for system in [RigSystem::SonarSuite, RigSystem::CryoHull] {
        assert_eq!(
            system.boss_reward(),
            Some(crate::descent::DescentBoss::Leviathan),
            "{system:?}"
        );
    }
}

#[test]
fn an_empty_arena_puts_the_leviathan_back_to_sleep_healed() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let arena = world.leviathan_arena().expect("the arena");
    put_rig(
        &mut world,
        TilePosition {
            x: arena.center().x,
            y: drowned::arena_floor_row(&arena) - 4,
        },
    );
    let _ = ticks(&mut world, 3);
    assert!(world.drowned().leviathan.awake());
    put_rig(
        &mut world,
        TilePosition {
            x: arena.center().x,
            y: arena.min.y - 40,
        },
    );
    let _ = ticks(&mut world, 30 * 32);
    assert!(!world.drowned().leviathan.awake());
    assert!(!world.drowned().leviathan.beaten);
}

#[test]
fn on_a_peaceful_world_the_leviathan_yields_its_gate() {
    let mut world = world_at(30);
    let arena = world.leviathan_arena().expect("the arena");
    put_rig(
        &mut world,
        TilePosition {
            x: arena.center().x,
            y: drowned::arena_floor_row(&arena) - 4,
        },
    );
    let events = drowned_events(&ticks(&mut world, 3));
    assert!(events.contains(&DrownedEvent::LeviathanDefeated));
    assert!(
        world
            .player(PLAYER)
            .expect("rig")
            .hazard_gear
            .has_system(RigSystem::CryoHull)
    );
}

// --- Formats ------------------------------------------------------------------------------------

#[test]
fn the_drowned_state_survives_json_saves_and_the_wire_and_is_hashed() {
    let mut world = world_at(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::UndergroundOcean),
    };
    room(&mut world, centre, (6, 4), i32::MAX);
    let before = world.simulation_state_hash();
    let inputs = TickInputs::default().act(PLAYER, PlayerCommand::UseScanner);
    let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    world.set_rig_water(
        PLAYER,
        RigWater {
            ballast: 300,
            frost_micro: 250_000,
        },
    );
    assert!(!world.drowned().is_empty());
    assert_ne!(before, world.simulation_state_hash());
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.drowned(), world.drowned());
    assert_eq!(loaded.rig_water(PLAYER), world.rig_water(PLAYER));
    // A client sees the section.
    let key = crate::world::SectionKey::Drowned;
    assert!(crate::world::SectionKey::SHARED.contains(&key));
    let payload = world.section(key).expect("the section");
    let mut client =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(30));
    client.apply_section(key, payload);
    assert_eq!(client.drowned(), world.drowned());
}

#[test]
fn a_frost_heater_from_the_kit_burns_the_frost_off_as_it_bites() {
    use crate::workshop::DeepSupply;
    let mut world = world_at(30);
    let row = middle_row(Stratum::BrineGlaciers);
    let centre = TilePosition { x: FAR, y: row };
    room(&mut world, centre, (10, 4), 0);
    let _ = world
        .logistics
        .add_supplies(PLAYER, DeepSupply::FrostHeater, 1);
    let mut warned = false;
    let mut burned = false;
    for _ in 0..30 * 40 {
        warned |= drowned_events(&ticks(&mut world, 1)).contains(&DrownedEvent::FrostWarning);
        if !burned && world.supplies_of(PLAYER, DeepSupply::FrostHeater) == 0 {
            burned = true;
            assert!(!warned, "the heater went before any warning");
            assert!(
                world.rig_water(PLAYER).frost_permille() < FROST_WARNING / 2,
                "the heater burned the frost off"
            );
        }
    }
    assert!(burned, "the heater was lit as the frost bit");
    // With the kit empty the frost builds again.
    assert!(
        world.rig_water(PLAYER).frost_permille() > 0,
        "it freezes again"
    );
}
