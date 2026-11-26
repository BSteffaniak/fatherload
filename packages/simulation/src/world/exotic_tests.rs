//! The exotic worlds at play (Wave 13 planets-d): a digging bot reaches every stratum of a
//! small and a big body of each theme; the signature rules, hazards, schedules, creatures and
//! bosses do what they say; everything replays bit for bit.

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, SimulationTick, TickInputs,
    WorldState,
    enemy::{Damage, DamageKind, DamageSource, EnemyDifficulty, EnemyKind},
    entity::{EntityKind, EntityState},
    exotic::{
        ExoticEvent, ExoticRule, ExoticStyle, ExoticTile, boss, gas_giant, locked,
        rules::{self, Zone, zone_at},
        settlement_site,
    },
    planet::{BodyGround, PlanetGround, PlanetParams},
    protection::HazardGear,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x0D13_E7E5;

fn world_on(style: ExoticStyle, radius: u32, hz: u32) -> WorldState {
    let params = PlanetParams::new(SEED, radius, style.theme());
    let mut world = WorldState::new_planet_world(PLAYER, params, SimulationParams::new(hz));
    world.enable_test_mode();
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world
}

fn ground(world: &WorldState) -> PlanetGround {
    *world.planet().expect("a planet world")
}

fn step(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let tick = world.simulation_tick();
    world
        .simulate_tick(tick, &TickInputs::held(PLAYER, controls))
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn idle() -> ControlState {
    ControlState::new(0.0, false, false)
}

fn exotic_events(events: &[GameplayEventKind]) -> Vec<ExoticEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Exotic { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

fn run_for(world: &mut WorldState, millis: u64) -> Vec<GameplayEventKind> {
    let ticks = world.timing().ticks_for_millis(millis);
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(step(world, idle()));
    }
    events
}

/// Puts the rig `rows_up` rows above row `floor` of column `column`, still.
fn place(world: &mut WorldState, column: i32, floor: i32, rows_up: f32) {
    let (x, y) = world.frame().centre_of(column, floor);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.x = x;
        rig.y = (rows_up + 0.5).mul_add(-TILE_SIZE, y) + 2.0;
        rig.x_remainder = 0.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
    world.stream_terrain_near_players();
}

fn rig_tile(world: &WorldState) -> TilePosition {
    let rig = world.player(PLAYER).expect("rig");
    world.frame().tile_at(rig.x, rig.y)
}

fn set(world: &mut WorldState, x: i32, y: i32, kind: TileKind) {
    assert!(world.terrain_mut().set_kind(TilePosition { x, y }, kind));
}

fn kind_at(world: &WorldState, x: i32, y: i32) -> TileKind {
    world
        .terrain()
        .tile(TilePosition { x, y })
        .expect("tile")
        .kind
}

/// A rig the hazards can bite: test mode's hull refill off, no hazard gear.
fn vulnerable(world: &mut WorldState) {
    let _ = world.apply_test_mode_action(
        PLAYER,
        crate::test_mode::TestModeAction::SetInvulnerable(false),
    );
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.hazard_gear = HazardGear::default();
        rig.hull = rig.max_hull();
    }
}

fn hull(world: &WorldState) -> f32 {
    world.player(PLAYER).expect("rig").hull
}

fn set_time(world: &mut WorldState, millis: u64) {
    let hz = u64::from(world.tick_rate_hz());
    world.set_simulation_tick(SimulationTick::new(millis * hz / 1_000));
}

/// A pocket dug at `(x, floor)` (seven wide, four high, a floor of `rock` under it) with the
/// rig resting in it.
fn pocket(world: &mut WorldState, x: i32, floor: i32, rock: TileKind) -> TilePosition {
    for y in floor - 4..floor {
        for dx in -3..=3 {
            set(world, x + dx, y, TileKind::Air);
        }
    }
    for dx in -3..=3 {
        set(world, x + dx, floor, rock);
    }
    place(world, x, floor, 1.0);
    run_for(world, 600);
    rig_tile(world)
}

// --- The digging bot ---------------------------------------------------------------------

/// Digs straight down from the settlement until the rig has been in every stratum (and, when
/// `core`, the core chamber); the zones it went through, in order.
fn dig_down(style: ExoticStyle, radius: u32, core: bool) -> Vec<Zone> {
    let mut world = world_on(style, radius, 30);
    let hz = world.timing().rate.hz();
    let site = settlement_site(&ground(&world)).expect("a site");
    place(&mut world, site.first + 8, site.floor, 1.0);
    for _ in 0..hz * 2 {
        step(&mut world, idle());
    }
    let mut zones: Vec<Zone> = Vec::new();
    let mut last_row = rig_tile(&world).y;
    let (mut still, mut nudge, mut ticks) = (0_u32, 0_u32, 0_u32);
    loop {
        ticks += 1;
        assert!(
            ticks < hz * 1_500,
            "{style:?} R={radius}: the dig ran out of time at row {} in {zones:?}",
            rig_tile(&world).y
        );
        let controls = if nudge > 0 {
            nudge -= 1;
            let fraction = (world.player(PLAYER).expect("rig").x / TILE_SIZE).rem_euclid(1.0);
            ControlState::new(if fraction < 0.5 { 0.4 } else { -0.4 }, false, false)
        } else {
            ControlState::new(0.0, false, true)
        };
        step(&mut world, controls);
        let tile = rig_tile(&world);
        let zone = zone_at(&ground(&world), style, tile);
        if zones.last() != Some(&zone) {
            zones.push(zone);
        }
        if tile.y == last_row {
            still += 1;
        } else {
            still = 0;
            last_row = tile.y;
        }
        if still > hz * 2 {
            still = 0;
            nudge = hz / 6;
            if std::env::var("EXOTIC_DEBUG").is_ok() {
                let rig = world.player(PLAYER).expect("rig");
                let below: Vec<_> = (0..4)
                    .map(|dy| kind_at(&world, tile.x, tile.y + dy))
                    .collect();
                println!(
                    "stuck at {tile:?} v=({}, {}) drills={} below={below:?}",
                    rig.velocity_x,
                    rig.velocity_y,
                    world.active_drills.contains_key(&PLAYER)
                );
            }
        }
        let deep_enough = if core {
            zone == Zone::CoreChamber
        } else {
            zone == Zone::Stratum(3)
        };
        if deep_enough {
            break;
        }
    }
    zones
}

fn assert_every_stratum(style: ExoticStyle, radius: u32, zones: &[Zone]) {
    for index in 0..4 {
        assert!(
            zones.contains(&Zone::Stratum(index)),
            "{style:?} R={radius}: stratum {index} never reached: {zones:?}"
        );
    }
}

#[test]
fn a_digging_bot_reaches_every_stratum_and_the_core_of_a_small_body() {
    for style in ExoticStyle::ALL {
        let zones = dig_down(style, 640, true);
        assert_every_stratum(style, 640, &zones);
        assert_eq!(zones.last(), Some(&Zone::CoreChamber), "{style:?}");
    }
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_big_planet() {
    for style in ExoticStyle::ALL {
        let zones = dig_down(style, 5_000, false);
        assert_every_stratum(style, 5_000, &zones);
    }
}

// --- The gas giant -----------------------------------------------------------------------

/// A row in the middle of band `band` under the cloud tops at column `column`.
fn band_row(ground: &PlanetGround, column: i32, band: usize) -> i32 {
    let top = gas_giant::cloud_top(ground, i64::from(column));
    let start = gas_giant::band_top_row(ground, band);
    let end = gas_giant::band_top_row(ground, band + 1).max(start + 40);
    top + i32::midpoint(start, end)
}

/// Empties a column of open gas round `(x, y)` (so a rig can fall freely there).
fn open_column(world: &mut WorldState, x: i32, top: i32, bottom: i32) {
    for y in top..bottom {
        for dx in -3..=3 {
            set(world, x + dx, y, TileKind::Air);
        }
    }
}

#[test]
fn the_thick_gas_holds_a_falling_rig_up() {
    let mut world = world_on(ExoticStyle::GasGiant, 5_000, 30);
    let g = ground(&world);
    let x = 200;
    let y = band_row(&g, x, 0);
    open_column(&mut world, x, y - 10, y + 200);
    place(&mut world, x, y, 0.0);
    let mut fastest = 0.0_f32;
    for _ in 0..world.timing().ticks_for_millis(4_000) {
        step(&mut world, idle());
        fastest = fastest.max(world.player(PLAYER).expect("rig").velocity_y);
    }
    assert!(fastest > 20.0, "it falls");
    assert!(
        fastest <= rules::GAS_TERMINAL_FALL + 1.0,
        "a gentle fall: {fastest}"
    );
}

#[test]
fn jet_streams_carry_an_idle_rig_and_drilling_braces_against_them() {
    let mut world = world_on(ExoticStyle::GasGiant, 5_000, 30);
    let g = ground(&world);
    let x = 300;
    let y = band_row(&g, x, 0);
    let rig = pocket(
        &mut world,
        x,
        y,
        TileKind::Exotic(ExoticTile::MetallicHydrogen),
    );
    // Open the pocket's sides so the wind has room.
    for y in rig.y - 2..=rig.y {
        for dx in -12..=12 {
            set(&mut world, x + dx, y, TileKind::Air);
        }
        set(
            &mut world,
            x - 13,
            rig.y + 1,
            TileKind::Exotic(ExoticTile::MetallicHydrogen),
        );
    }
    for dx in -13..=13 {
        set(
            &mut world,
            x + dx,
            rig.y + 1,
            TileKind::Exotic(ExoticTile::MetallicHydrogen),
        );
    }
    let top = gas_giant::cloud_top(&g, i64::from(x));
    let way = rules::jet_direction(rig.y, top);
    let before = world.player(PLAYER).expect("rig").x;
    run_for(&mut world, 1_000);
    let moved = world.player(PLAYER).expect("rig").x - before;
    assert!(moved * way > 40.0, "carried {moved} (way {way})");
    let before = world.player(PLAYER).expect("rig").x;
    for _ in 0..world.timing().ticks_for_millis(500) {
        step(&mut world, ControlState::new(0.0, false, true));
    }
    let braced = world.player(PLAYER).expect("rig").x - before;
    assert!(
        braced.abs() < moved.abs() * 0.5,
        "drilling braces: {braced}"
    );
}

#[test]
fn a_bolt_flickers_then_strikes_whatever_stands_in_its_column() {
    let mut world = world_on(ExoticStyle::GasGiant, 5_000, 30);
    let g = ground(&world);
    let probe = TilePosition {
        x: 300,
        y: band_row(&g, 300, 1),
    };
    let bolt = rules::bolts_near(&g, probe, 3, 10_000, 120_000, 0.0)
        .into_iter()
        .next()
        .expect("a bolt in the Storm Belt");
    let y = bolt.top + 20;
    pocket(
        &mut world,
        bolt.column,
        y,
        TileKind::Exotic(ExoticTile::MetallicHydrogen),
    );
    vulnerable(&mut world);
    set_time(&mut world, bolt.tell_millis - 300);
    let start = hull(&world);
    let events = exotic_events(&run_for(&mut world, 2_200));
    let tell = events.iter().position(|e| *e == ExoticEvent::LightningTell);
    let strike = events
        .iter()
        .position(|e| *e == ExoticEvent::LightningStrike);
    assert!(tell.is_some() && strike.is_some(), "{events:?}");
    assert!(tell < strike, "told first");
    assert!(
        hull(&world) < start,
        "the bolt struck the rig in its column"
    );
}

#[test]
fn the_pressure_is_warned_before_it_crushes_and_the_struts_brace_against_it() {
    let mut world = world_on(ExoticStyle::GasGiant, 5_000, 30);
    let g = ground(&world);
    let x = 400;
    let top = gas_giant::cloud_top(&g, i64::from(x));
    // Just above the Pressure Deep: warned, unhurt.
    let above = top + gas_giant::band_top_row(&g, 2) - 30;
    pocket(
        &mut world,
        x,
        above,
        TileKind::Exotic(ExoticTile::MetallicHydrogen),
    );
    vulnerable(&mut world);
    let start = hull(&world);
    let events = exotic_events(&run_for(&mut world, 5_000));
    assert!(events.contains(&ExoticEvent::PressureWarning), "{events:?}");
    assert!((hull(&world) - start).abs() < f32::EPSILON, "no crush yet");
    // Deep in it: it crushes, harder deeper.
    let deep = band_row(&g, x, 2) + 60;
    pocket(
        &mut world,
        x,
        deep,
        TileKind::Exotic(ExoticTile::MetallicHydrogen),
    );
    vulnerable(&mut world);
    let start = hull(&world);
    run_for(&mut world, 2_000);
    let crushed = start - hull(&world);
    assert!(crushed > 0.0, "the pressure crushes");
    // High-gravity struts at the top tier brace the hull.
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.hull = rig.max_hull();
        rig.hazard_gear.ship.gravity_struts = 2;
    }
    let start = hull(&world);
    run_for(&mut world, 2_000);
    assert!((hull(&world) - start).abs() < f32::EPSILON, "braced");
}

#[test]
fn diamond_hail_lands_under_its_shadow_and_leaves_diamond() {
    let mut world = world_on(ExoticStyle::GasGiant, 5_000, 30);
    let g = ground(&world);
    let x = 500;
    let y = band_row(&g, x, 3);
    let rig = pocket(
        &mut world,
        x,
        y,
        TileKind::Exotic(ExoticTile::MetallicHydrogen),
    );
    let now = world.body_millis();
    let hail = rules::fallers_near(&g, ExoticStyle::GasGiant, rig, 2, now, now + 60_000, 0.0);
    let first = hail.first().expect("hail near the rig");
    // A floor under the hailstone's column, inside its cell.
    let floor = first.from_row + 20;
    for row in first.from_row..floor {
        set(&mut world, first.column, row, TileKind::Air);
    }
    set(
        &mut world,
        first.column,
        floor,
        TileKind::Exotic(ExoticTile::MetallicHydrogen),
    );
    set_time(&mut world, first.land_millis - 200);
    let events = exotic_events(&run_for(&mut world, 600));
    assert!(events.contains(&ExoticEvent::DiamondHail), "{events:?}");
    assert_eq!(
        kind_at(&world, first.column, floor - 1),
        TileKind::Exotic(ExoticTile::DiamondHail)
    );
}

// --- The locked world --------------------------------------------------------------------

#[test]
fn the_day_side_burns_the_night_side_freezes_and_the_twilight_is_mild() {
    let mut damage = Vec::new();
    for fraction in [0_i32, 2, 1] {
        let mut world = world_on(ExoticStyle::Locked, 1_200, 30);
        let g = ground(&world);
        let width = g.strip_width();
        // Noon, midnight, the evening terminator.
        let x = width * fraction / 4 + 37;
        let x = if fraction == 1 { width / 4 + 2 } else { x };
        place(&mut world, x, g.surface_row(x), 1.0);
        run_for(&mut world, 500);
        vulnerable(&mut world);
        let start = hull(&world);
        run_for(&mut world, 2_000);
        damage.push(start - hull(&world));
    }
    assert!(damage[0] > 0.0, "the day burns: {damage:?}");
    assert!(damage[1] > 0.0, "the night freezes: {damage:?}");
    assert!(
        damage[2].abs() < f32::EPSILON,
        "the twilight is mild: {damage:?}"
    );
    // Heat shielding stops the noon.
    let mut world = world_on(ExoticStyle::Locked, 1_200, 30);
    let g = ground(&world);
    place(&mut world, 37, g.surface_row(37), 1.0);
    run_for(&mut world, 500);
    vulnerable(&mut world);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.hazard_gear.ship.heat_shielding = 2;
    }
    let start = hull(&world);
    run_for(&mut world, 2_000);
    assert!((hull(&world) - start).abs() < f32::EPSILON);
}

#[test]
fn a_geyser_hisses_then_throws_a_rig_over_it_upward() {
    let mut world = world_on(ExoticStyle::Locked, 1_200, 30);
    let g = ground(&world);
    let vent = (0..400)
        .flat_map(|cx| (0..30).filter_map(move |cy| (cx, cy).into()))
        .find_map(|(cx, cy)| {
            locked::vent_in(&g, cx, cy).filter(|vent| g.stratum_index(vent.x, vent.y) == 1)
        })
        .expect("a vent in the Geyser Fields");
    world.stream_terrain_near_players();
    // The rig sits in the chimney over the vent.
    place(&mut world, vent.x, vent.y, 1.0);
    let quiet = (0..30_000_u64)
        .step_by(100)
        .find(|millis| {
            rules::geyser_phase(&g, vent, *millis) == rules::GeyserPhase::Quiet
                && rules::geyser_phase(&g, vent, millis + 3_000) == rules::GeyserPhase::Blowing
        })
        .expect("a cycle");
    set_time(&mut world, quiet);
    place(&mut world, vent.x, vent.y, 1.0);
    let mut lifted = false;
    let mut events = Vec::new();
    for _ in 0..world.timing().ticks_for_millis(4_500) {
        events.extend(step(&mut world, idle()));
        if world.player(PLAYER).expect("rig").velocity_y < -200.0 {
            lifted = true;
        }
    }
    let events = exotic_events(&events);
    let hiss = events.iter().position(|e| *e == ExoticEvent::GeyserHiss);
    let blow = events.iter().position(|e| *e == ExoticEvent::GeyserErupt);
    assert!(hiss.is_some() && hiss < blow, "{events:?}");
    assert!(lifted, "the plume threw the rig up");
}

#[test]
fn a_tidal_quake_rumbles_then_drops_the_rubble_over_a_rig() {
    let mut world = world_on(ExoticStyle::Locked, 1_200, 30);
    let g = ground(&world);
    let x = 120;
    let depth = g.core_chamber_top() * 62 / 100;
    let floor = g.surface_row(x) + depth;
    let rig = pocket(
        &mut world,
        x,
        floor,
        TileKind::Exotic(ExoticTile::FractureRock),
    );
    assert_eq!(
        rules::rule_at(&g, ExoticStyle::Locked, rig),
        Some(ExoticRule::TidalQuakes)
    );
    for dx in -3..=3 {
        set(
            &mut world,
            x + dx,
            rig.y - 3,
            TileKind::Exotic(ExoticTile::FaultRubble),
        );
    }
    let shock = (1_000..200_000_u64)
        .step_by(20)
        .find(|millis| rules::quake_shock(&g, *millis, millis + 20))
        .expect("a quake");
    set_time(&mut world, shock - rules::QUAKE_RUMBLE_MILLIS - 200);
    let events = run_for(&mut world, rules::QUAKE_RUMBLE_MILLIS + 800);
    let exotic = exotic_events(&events);
    let rumble = exotic.iter().position(|e| *e == ExoticEvent::QuakeRumble);
    let shaken = exotic.iter().position(|e| *e == ExoticEvent::QuakeShock);
    assert!(rumble.is_some() && rumble < shaken, "{exotic:?}");
    assert!(events.contains(&GameplayEventKind::BouldersChanged));
    assert_eq!(
        kind_at(&world, x, rig.y - 3),
        TileKind::Air,
        "the rubble fell"
    );
}

#[test]
fn the_lodestone_drags_an_idle_rig_toward_the_sunward_meridian() {
    let mut world = world_on(ExoticStyle::Locked, 1_200, 30);
    let g = ground(&world);
    let x = 300;
    let depth = g.core_chamber_top() * 88 / 100;
    let floor = g.surface_row(x) + depth;
    let rig = pocket(
        &mut world,
        x,
        floor,
        TileKind::Exotic(ExoticTile::Lodestone),
    );
    for dx in -12..=3 {
        for y in rig.y - 2..=rig.y {
            set(&mut world, x + dx, y, TileKind::Air);
        }
        set(
            &mut world,
            x + dx,
            rig.y + 1,
            TileKind::Exotic(ExoticTile::Lodestone),
        );
    }
    assert_eq!(
        rules::rule_at(&g, ExoticStyle::Locked, rig),
        Some(ExoticRule::LodestoneDrift)
    );
    let before = world.player(PLAYER).expect("rig").x;
    run_for(&mut world, 1_000);
    assert!(
        world.player(PLAYER).expect("rig").x < before - 20.0,
        "drawn toward column 0"
    );
}

// --- The ring world ----------------------------------------------------------------------

#[test]
fn a_meteor_whistles_down_and_leaves_its_meteorite() {
    let mut world = world_on(ExoticStyle::Ring, 2_500, 30);
    let g = ground(&world);
    let site = settlement_site(&g).expect("a site");
    place(&mut world, site.first + 4, site.floor, 1.0);
    run_for(&mut world, 500);
    let rig = rig_tile(&world);
    let now = world.body_millis();
    let meteor = rules::fallers_near(
        &g,
        ExoticStyle::Ring,
        rig,
        2,
        now + 3_000,
        now + 120_000,
        0.0,
    )
    .into_iter()
    .next()
    .expect("a meteor near the rig");
    set_time(
        &mut world,
        meteor.land_millis - rules::FALL_TELL_MILLIS - 300,
    );
    let events = exotic_events(&run_for(&mut world, rules::FALL_TELL_MILLIS + 600));
    let whistle = events
        .iter()
        .position(|e| *e == ExoticEvent::MeteorIncoming);
    let impact = events.iter().position(|e| *e == ExoticEvent::MeteorImpact);
    assert!(whistle.is_some() && whistle < impact, "{events:?}");
    let floor = (meteor.from_row..=meteor.to_row)
        .find(|row| !kind_at(&world, meteor.column, *row).is_open())
        .expect("ground");
    assert_eq!(
        kind_at(&world, meteor.column, floor),
        TileKind::Exotic(ExoticTile::Meteorite)
    );
}

#[test]
fn buried_ring_ice_vents_and_throws_the_drilling_rig_back() {
    let mut world = world_on(ExoticStyle::Ring, 1_200, 30);
    let g = ground(&world);
    let x = 200;
    let depth = g.core_chamber_top() * 36 / 100;
    let floor = g.surface_row(x) + depth;
    let rig = pocket(&mut world, x, floor, TileKind::Exotic(ExoticTile::Breccia));
    assert_eq!(
        rules::rule_at(&g, ExoticStyle::Ring, rig),
        Some(ExoticRule::VentingIce)
    );
    let target = TilePosition {
        x: rig.x + 1,
        y: rig.y,
    };
    let mut output = crate::event::TickOutput::default();
    world.exotic_mined(PLAYER, target, ExoticTile::RingIce, &mut output);
    let kinds: Vec<GameplayEventKind> = output.events.iter().map(|e| e.kind).collect();
    assert!(exotic_events(&kinds).contains(&ExoticEvent::IceVented));
    assert!(
        world.player(PLAYER).expect("rig").velocity_x < -100.0,
        "thrown back from the vent"
    );
}

#[test]
fn live_conduits_shock_and_a_cut_junction_darkens_its_run() {
    let mut world = world_on(ExoticStyle::Ring, 1_200, 30);
    let g = ground(&world);
    let x = 260;
    let depth = g.core_chamber_top() * 62 / 100;
    let floor = g.surface_row(x) + depth;
    let rig = pocket(
        &mut world,
        x,
        floor,
        TileKind::Exotic(ExoticTile::Ringstone),
    );
    for dx in -3..=8 {
        set(
            &mut world,
            x + dx,
            rig.y + 1,
            TileKind::Exotic(ExoticTile::ConduitLive),
        );
    }
    set(
        &mut world,
        x + 6,
        rig.y + 1,
        TileKind::Exotic(ExoticTile::ConduitJunction),
    );
    vulnerable(&mut world);
    let start = hull(&world);
    let events = exotic_events(&run_for(&mut world, 1_500));
    assert!(events.contains(&ExoticEvent::ConduitShock), "{events:?}");
    assert!(hull(&world) < start);
    let mut output = crate::event::TickOutput::default();
    set(&mut world, x + 6, rig.y + 1, TileKind::Air);
    world.exotic_mined(
        PLAYER,
        TilePosition {
            x: x + 6,
            y: rig.y + 1,
        },
        ExoticTile::ConduitJunction,
        &mut output,
    );
    for dx in -3..=5 {
        assert_eq!(
            kind_at(&world, x + dx, rig.y + 1),
            TileKind::Exotic(ExoticTile::ConduitDead),
            "column {dx}"
        );
    }
    let start = hull(&world);
    run_for(&mut world, 1_000);
    assert!(
        (hull(&world) - start).abs() < f32::EPSILON,
        "a dead run is safe"
    );
}

#[test]
fn the_skyring_holds_a_rig_up_and_the_dense_core_pulls_it_down() {
    let fall = |style: ExoticStyle, row_of: &dyn Fn(&PlanetGround) -> i32| {
        let mut world = world_on(style, 2_500, 30);
        let g = ground(&world);
        let x = 37;
        let y = row_of(&g);
        open_column(&mut world, x, y - 4, y + 60);
        place(&mut world, x, y, 0.0);
        for _ in 0..world.timing().ticks_for_millis(400) {
            step(&mut world, idle());
        }
        world.player(PLAYER).expect("rig").velocity_y
    };
    let ring_fall = fall(ExoticStyle::Ring, &|_| crate::exotic::ring::SKYRING_TOP + 8);
    let open_fall = fall(ExoticStyle::Ring, &|g| g.surface_row(37) - 40);
    assert!(
        ring_fall < open_fall * 0.6,
        "Skyring {ring_fall} vs open sky {open_fall}"
    );
    let core_fall = fall(ExoticStyle::Ring, &|g| {
        g.surface_row(37) + g.core_chamber_top() * 90 / 100
    });
    assert!(
        core_fall > open_fall * 1.2,
        "dense core {core_fall} vs {open_fall}"
    );
}

// --- Creatures ---------------------------------------------------------------------------

#[test]
fn the_spawner_brings_each_worlds_own_creatures() {
    for (style, depth) in [
        (ExoticStyle::GasGiant, -30),
        (ExoticStyle::Locked, 0),
        (ExoticStyle::Ring, 0),
    ] {
        let mut world = world_on(style, 1_500, 30);
        world.set_enemy_difficulty(EnemyDifficulty::Hard);
        let g = ground(&world);
        let site = settlement_site(&g).expect("a site");
        // Away from the settlement (safe ground), on the open surface.
        let x = site.centre() + 200;
        place(&mut world, x, g.surface_row(x) + depth.min(0), 1.0);
        run_for(&mut world, 40_000);
        let natives = style.creatures();
        let kinds: Vec<EnemyKind> = world
            .entities
            .of_kind(EntityKind::Enemy)
            .filter_map(|entity| match &entity.state {
                EntityState::Enemy(state) => Some(state.kind),
                _ => None,
            })
            .collect();
        assert!(!kinds.is_empty(), "{style:?}: something came");
        assert!(
            kinds.iter().all(|kind| natives.contains(kind)),
            "{style:?}: only its own: {kinds:?}"
        );
    }
}

#[test]
fn fauna_never_hunt_and_the_frost_stalker_sees_only_motion() {
    let mut world = world_on(ExoticStyle::Locked, 1_200, 30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let g = ground(&world);
    let width = g.strip_width();
    let x = width / 4 + 6;
    place(&mut world, x, g.surface_row(x), 1.0);
    run_for(&mut world, 500);
    let rig = rig_tile(&world);
    let (gx, gy) = world.frame().centre_of(rig.x + 3, rig.y);
    let grazer = world.spawn_enemy(EnemyKind::DuskGrazer, gx, gy);
    for _ in 0..world.timing().ticks_for_millis(3_000) {
        step(&mut world, idle());
        if let Some((_, state)) = world.enemy_parts(grazer) {
            assert!(state.target.is_none(), "a grazer never hunts");
        }
    }
    // A frost stalker on the night side: a still rig is lost to it, a moving one seen.
    let mut world = world_on(ExoticStyle::Locked, 1_200, 30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let x = width / 2 + 40;
    let floor = g.surface_row(x);
    for dx in -2..=14 {
        for y in floor - 4..floor {
            set(&mut world, x + dx, y, TileKind::Air);
        }
        set(
            &mut world,
            x + dx,
            floor,
            TileKind::Exotic(ExoticTile::NitrogenIce),
        );
    }
    place(&mut world, x, floor, 1.0);
    run_for(&mut world, 500);
    let rig = rig_tile(&world);
    let (sx, sy) = world.frame().centre_of(rig.x + 9, rig.y);
    let stalker = world.spawn_enemy(EnemyKind::FrostStalker, sx, sy);
    run_for(&mut world, 1_500);
    assert!(
        world
            .enemy_parts(stalker)
            .is_some_and(|(_, s)| s.target.is_none()),
        "a still rig is unseen"
    );
    let mut seen = false;
    for _ in 0..world.timing().ticks_for_millis(1_500) {
        step(&mut world, ControlState::new(0.8, false, false));
        if world
            .enemy_parts(stalker)
            .is_some_and(|(_, s)| s.target == Some(PLAYER))
        {
            seen = true;
        }
    }
    assert!(seen, "a moving rig is seen");
}

// --- The bosses --------------------------------------------------------------------------

/// A world of `style` with the rig standing in its boss's arena.
fn in_arena(style: ExoticStyle, difficulty: EnemyDifficulty) -> (WorldState, boss::Arena) {
    let mut world = world_on(style, 2_500, 30);
    world.set_enemy_difficulty(difficulty);
    let g = ground(&world);
    let arena = boss::arena(&g).expect("an arena");
    let x = arena.centre - arena.half_width + 6;
    place(&mut world, x, arena.floor, 1.0);
    // The gas giant's platform is narrower than its well: stand on the platform.
    if style == ExoticStyle::GasGiant {
        place(&mut world, arena.centre - 8, gas_giant::EYE_FLOOR, 1.0);
    }
    (world, arena)
}

fn boss_entity(world: &WorldState, kind: EnemyKind) -> Option<crate::entity::EntityId> {
    world
        .entities
        .of_kind(EntityKind::Enemy)
        .find(|entity| matches!(&entity.state, EntityState::Enemy(state) if state.kind == kind))
        .map(|entity| entity.id)
}

#[test]
fn each_boss_wakes_in_its_arena_tells_its_attacks_and_strikes() {
    for style in ExoticStyle::ALL {
        let (mut world, _) = in_arena(style, EnemyDifficulty::Normal);
        let events = exotic_events(&run_for(&mut world, 12_000));
        assert!(
            events.contains(&ExoticEvent::BossAwoke),
            "{style:?}: {events:?}"
        );
        assert!(events.contains(&ExoticEvent::BossTell), "{style:?}");
        assert!(boss_entity(&world, style.boss()).is_some(), "{style:?}");
        // Its view shows the marks while it tells.
        let mut marked = false;
        for _ in 0..world.timing().ticks_for_millis(8_000) {
            step(&mut world, idle());
            if world.exotic_boss_view().is_some_and(|view| {
                view.telling && (!view.marks.is_empty() || view.attack.is_some())
            }) {
                marked = true;
            }
        }
        assert!(marked, "{style:?}: a told attack");
    }
}

#[test]
fn a_boss_struck_while_open_takes_more_and_beaten_its_seal_becomes_its_trophy() {
    for style in ExoticStyle::ALL {
        let (mut world, arena) = in_arena(style, EnemyDifficulty::Normal);
        run_for(&mut world, 1_000);
        let id = boss_entity(&world, style.boss()).expect("the boss woke");
        let mut output = crate::event::TickOutput::default();
        let hit = Damage::new(10.0, DamageKind::Kinetic, DamageSource::Player(PLAYER));
        let before = world.enemy_parts(id).expect("boss").1.health;
        let _ = world.damage_enemy(id, hit, &mut output);
        let plain = before - world.enemy_parts(id).expect("boss").1.health;
        // Open it and strike again.
        world.entities.update(id, |entity| {
            if let EntityState::Enemy(state) = &mut entity.state {
                state.volley = crate::world::BOSS_STEP_EXPOSED;
            }
        });
        let before = world.enemy_parts(id).expect("boss").1.health;
        let _ = world.damage_enemy(id, hit, &mut output);
        let open = before - world.enemy_parts(id).expect("boss").1.health;
        assert!(open > plain * 2.0, "{style:?}: {plain} then {open}");
        assert_eq!(world.exotic_boss_beaten(), Some(false));
        let max = world.enemy_parts(id).expect("boss").1.max_health;
        let _ = world.damage_enemy(
            id,
            Damage::new(
                max * 10.0,
                DamageKind::Kinetic,
                DamageSource::Player(PLAYER),
            ),
            &mut output,
        );
        let kinds: Vec<GameplayEventKind> = output.events.iter().map(|e| e.kind).collect();
        assert!(
            exotic_events(&kinds).contains(&ExoticEvent::BossDefeated),
            "{style:?}"
        );
        assert_eq!(
            kind_at(&world, arena.seal.x, arena.seal.y),
            TileKind::Exotic(boss::trophy(style))
        );
        assert_eq!(world.exotic_boss_beaten(), Some(true));
        // It never wakes again.
        run_for(&mut world, 3_000);
        assert!(boss_entity(&world, style.boss()).is_none(), "{style:?}");
    }
}

#[test]
fn a_boss_sleeps_healed_when_the_crew_leaves_and_a_peaceful_world_opens_the_seal() {
    let (mut world, arena) = in_arena(ExoticStyle::Ring, EnemyDifficulty::Normal);
    run_for(&mut world, 1_000);
    assert!(boss_entity(&world, EnemyKind::RingWarden).is_some());
    let g = ground(&world);
    let far = arena.centre + 200;
    place(&mut world, far, g.surface_row(far), 1.0);
    run_for(&mut world, 1_000);
    assert!(
        boss_entity(&world, EnemyKind::RingWarden).is_none(),
        "asleep"
    );
    let (mut world, arena) = in_arena(ExoticStyle::Locked, EnemyDifficulty::Peaceful);
    let events = exotic_events(&run_for(&mut world, 1_000));
    assert!(events.contains(&ExoticEvent::BossDefeated));
    assert_eq!(
        kind_at(&world, arena.seal.x, arena.seal.y),
        TileKind::Exotic(ExoticTile::GloamTrophy)
    );
}

#[test]
fn the_slam_hurts_a_rig_on_the_ground_and_passes_under_one_in_the_air() {
    let (mut world, arena) = in_arena(ExoticStyle::Locked, EnemyDifficulty::Normal);
    run_for(&mut world, 500);
    let id = boss_entity(&world, EnemyKind::Gloamwarden).expect("awake");
    let g = ground(&world);
    let standing = arena.home.x + 3;
    place(&mut world, standing, g.surface_row(standing), 1.0);
    vulnerable(&mut world);
    // Tell a slam now, at the rig.
    let now = world.simulation_tick();
    world.entities.update(id, |entity| {
        if let EntityState::Enemy(state) = &mut entity.state {
            state.pattern = 0;
            state.volley = crate::world::BOSS_STEP_TELL;
            state.waypoint = Some((standing, arena.floor - 1));
            state.mode_until = now;
        }
    });
    let start = hull(&world);
    run_for(&mut world, 200);
    assert!(hull(&world) < start, "the shockwave ran under the rig");
}

// --- Determinism -------------------------------------------------------------------------

fn a_minute(
    style: ExoticStyle,
    hz: u32,
) -> (crate::hash::SimulationStateHash, Vec<GameplayEventKind>) {
    let mut world = world_on(style, 900, hz);
    world.set_enemy_difficulty(EnemyDifficulty::Hard);
    let g = ground(&world);
    let site = settlement_site(&g).expect("a site");
    let x = site.centre() + 150;
    place(&mut world, x, g.surface_row(x), 1.0);
    let mut events = Vec::new();
    for tick in 0..hz * 60 {
        let drill = !(tick / hz).is_multiple_of(3);
        events.extend(step(&mut world, ControlState::new(0.0, false, drill)));
    }
    (world.simulation_state_hash(), events)
}

#[test]
fn a_minute_on_each_world_replays_bit_for_bit_at_any_tick_rate() {
    for style in ExoticStyle::ALL {
        let (first, events) = a_minute(style, 30);
        let (again, events_again) = a_minute(style, 30);
        assert_eq!(first, again, "{style:?}");
        assert_eq!(events, events_again);
        let (fast, _) = a_minute(style, 60);
        assert_eq!(fast, a_minute(style, 60).0);
    }
}

#[test]
fn the_settlement_is_safe_and_the_wilds_are_not() {
    let world = world_on(ExoticStyle::Ring, 1_500, 30);
    let g = ground(&world);
    let site = settlement_site(&g).expect("a site");
    let on = TilePosition {
        x: site.centre(),
        y: site.floor - 1,
    };
    let off = TilePosition {
        x: site.centre() + 300,
        y: g.surface_row(site.centre() + 300) - 1,
    };
    assert!(world.enemy_safe_zone(on));
    assert!(!world.enemy_safe_zone(off));
    assert!(
        world
            .exotic_tier(off)
            .is_some_and(|tier| tier >= rules::EXOTIC_BASE_TIER)
    );
}

#[test]
fn creatures_set_down_by_hand_stay_where_they_are_put() {
    for style in ExoticStyle::ALL {
        let mut world = world_on(style, 1_200, 30);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        let g = ground(&world);
        let site = settlement_site(&g).expect("a site");
        place(&mut world, site.first, site.floor, 1.0);
        let ids: Vec<_> = style
            .creatures()
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                let column = site.first + 4 + i32::try_from(index).unwrap_or(0) * 4;
                let (x, y) = world.frame().centre_of(column, site.floor - 3);
                world.spawn_enemy(kind, x, y)
            })
            .collect();
        run_for(&mut world, 200);
        for id in ids {
            assert!(
                world.enemy_parts(id).is_some(),
                "{style:?}: a creature vanished"
            );
        }
    }
}
