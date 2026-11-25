//! Planet worlds (Wave 11 polar track): a small moon of radius 400 is fully playable. A bot
//! lands, drives all the way round it, digs to the core chamber through every halving ring and
//! takes off; gravity is the body's and never turns the craft; landing is a skill; saves and
//! replicas keep the wrapped strip; everything is deterministic at every tick rate.

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WorldState,
    planet::{
        BodyGround, PlanetParams, PlanetThemeId,
        landing::{PlanetEvent, tip_angle},
    },
    terrain::{TILE_SIZE, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);
const DEG: f32 = std::f32::consts::PI / 180.0;

fn moon_params() -> PlanetParams {
    PlanetParams::new(0x4D00_4E00, 400, PlanetThemeId::BARREN)
}

fn moon(hz: u32) -> WorldState {
    WorldState::new_planet_world(PLAYER, moon_params(), SimulationParams::new(hz))
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

fn planet_events(events: &[GameplayEventKind]) -> Vec<PlanetEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Planet { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

fn rig(world: &WorldState) -> &crate::Player {
    world.player(PLAYER).expect("player")
}

/// The rig's strip column (`0..width`) and row.
fn strip_tile(world: &WorldState) -> (i64, i64) {
    let width = i64::from(world.planet().expect("planet").strip_width());
    let (column, row) = world.actor_world_pos(PLAYER).expect("rig").tile();
    (column.rem_euclid(width), row)
}

fn idle() -> ControlState {
    ControlState::new(0.0, false, false)
}

/// What the bot saw.
#[derive(Debug, Default)]
struct BotRun {
    landed: bool,
    lap_ticks: u32,
    max_frame_sector: i64,
    min_frame_sector: i64,
    core_ticks: u32,
    core_row: i64,
    take_off_ticks: u32,
    events: Vec<PlanetEvent>,
}

/// Lands, drives a full lap round the moon (clockwise), digs to the core chamber and flies
/// back out above the surface. Panics if a phase runs out of time.
#[allow(clippy::too_many_lines, reason = "the bot's four phases read in order")]
fn run_bot(world: &mut WorldState) -> BotRun {
    let hz = world.timing().rate.hz();
    let planet = *world.planet().expect("planet");
    let width = i64::from(planet.strip_width());
    let mut run = BotRun::default();
    // 1. Land: the rig starts a few rows up and comes down on its tracks.
    for _ in 0..hz * 5 {
        let events = planet_events(&step(world, idle()));
        if events.contains(&PlanetEvent::Landed) {
            run.landed = true;
        }
        run.events.extend(events);
        if run.landed && rig(world).velocity_y == 0.0 {
            break;
        }
    }
    assert!(run.landed, "the bot lands: {:?}", run.events);
    // 2. Drive all the way round, hopping steps with short burns.
    let (start, _) = strip_tile(world);
    let mut last = start;
    let mut travelled = 0_i64;
    let mut stuck = 0_u32;
    let mut burn = 0_u32;
    let budget = hz * 400;
    let mut ticks = 0;
    while travelled < width {
        ticks += 1;
        assert!(
            ticks < budget,
            "lap ran out of time at {travelled} of {width}"
        );
        // A hill ahead: climb over it (holding into it would drill a tunnel through).
        let wall_ahead = {
            let player = rig(world);
            let frame = world.frame();
            let ahead = frame.tile_at(TILE_SIZE.mul_add(0.6, player.x), player.y);
            world.terrain().is_solid_at(ahead)
        };
        if wall_ahead {
            burn = burn.max(2);
        }
        let thrust = burn > 0;
        burn = burn.saturating_sub(1);
        let push = if wall_ahead { 0.0 } else { 1.0 };
        run.events.extend(planet_events(&step(
            world,
            ControlState::new(push, thrust, false),
        )));
        let (column, _) = strip_tile(world);
        let mut delta = column - last;
        if delta > width / 2 {
            delta -= width;
        } else if delta < -width / 2 {
            delta += width;
        }
        travelled += delta;
        last = column;
        if rig(world).velocity_x.abs() < 30.0 {
            stuck += 1;
        } else {
            stuck = 0;
        }
        if stuck > hz / 10 && burn == 0 {
            burn = hz / 4;
            stuck = 0;
        }
        let frame = world.frame();
        run.max_frame_sector = run.max_frame_sector.max(frame.sector_x);
        run.min_frame_sector = run.min_frame_sector.min(frame.sector_x);
    }
    run.lap_ticks = ticks;
    // Settle on the ground before digging.
    for _ in 0..hz * 3 {
        step(world, idle());
    }
    // 3. Dig straight down to the core chamber.
    let mut ticks = 0;
    let mut last_row = strip_tile(world).1;
    let mut still = 0;
    let mut nudge = 0;
    loop {
        ticks += 1;
        assert!(
            ticks < hz * 600,
            "dig ran out of time at row {}",
            strip_tile(world).1
        );
        // Perched over a gap (the drill has nothing under the rig's middle): shuffle along.
        let controls = if nudge > 0 {
            nudge -= 1;
            // Toward the middle of the column under the rig (where the open shaft is).
            let fraction = (rig(world).x / TILE_SIZE).rem_euclid(1.0);
            let toward = if fraction < 0.5 { 0.4 } else { -0.4 };
            ControlState::new(toward, false, false)
        } else {
            ControlState::new(0.0, false, true)
        };
        run.events.extend(planet_events(&step(world, controls)));
        let (_, row) = strip_tile(world);
        if row == last_row {
            still += 1;
        } else {
            still = 0;
            last_row = row;
        }
        if still > hz * 2 {
            still = 0;
            nudge = hz / 6;
        }
        if planet.in_core_chamber(i32::try_from(row).expect("row")) {
            break;
        }
    }
    run.core_ticks = ticks;
    run.core_row = strip_tile(world).1;
    // 4. Take off: thrust back up the shaft and out above the hills.
    let mut ticks = 0;
    loop {
        ticks += 1;
        assert!(
            ticks < hz * 300,
            "take-off ran out of time at {:?}: {:?} {:?}",
            strip_tile(world),
            (
                rig(world).x,
                rig(world).y,
                rig(world).velocity_x,
                rig(world).velocity_y
            ),
            rig(world).attitude
        );
        step(world, ControlState::new(0.0, true, false));
        if strip_tile(world).1 < i64::from(planet.strip_top()) {
            break;
        }
    }
    run.take_off_ticks = ticks;
    run
}

fn enable_bot_mode(world: &mut WorldState) {
    // Test mode: the top engine and drill (the bot proves the place, not the economy).
    world.enable_test_mode();
}

#[test]
fn a_bot_lands_drives_round_a_small_moon_digs_to_the_core_and_takes_off() {
    let mut world = moon(60);
    enable_bot_mode(&mut world);
    let planet = *world.planet().expect("planet");
    assert_eq!(planet.grid().radius(), 400);
    let run = run_bot(&mut world);
    // The frame wraps by whole laps: it never leaves the strip's lap of sectors.
    assert!(run.min_frame_sector >= 0, "{run:?}");
    assert!(
        run.max_frame_sector < planet.grid().lap_sectors(),
        "{run:?}"
    );
    assert!(
        run.core_row >= i64::from(planet.core_chamber_top()),
        "{run:?}"
    );
    // The shaft crossed every halving boundary, and each ring tile it cut is open whole: the
    // ring really has half the tiles of the one above.
    let column = strip_tile_column_of_shaft(&world);
    for start in planet.grid().halving_rows() {
        for row in [*start, start + 3] {
            if row >= planet.core_chamber_top() {
                continue;
            }
            let tile = planet.grid().tile_of(column, row);
            let first = planet.grid().first_strip_column(tile);
            for cell in first..first + planet.grid().span_at(row) {
                let position = TilePosition {
                    x: i32::try_from(cell).expect("col"),
                    y: row,
                };
                assert!(
                    world
                        .terrain()
                        .tile(position)
                        .is_some_and(|tile| tile.kind.is_open()),
                    "row {row} cell {cell} of the shaft's ring tile is open"
                );
            }
        }
    }
    // The rig is above the hills, out toward space.
    assert!(strip_tile(&world).1 < i64::from(planet.strip_top()));
    assert!(rig(&world).hull > 0.0);
    eprintln!(
        "moon bot: lap {} ticks, core {} ticks, take-off {} ticks",
        run.lap_ticks, run.core_ticks, run.take_off_ticks
    );
}

/// The column the bot dug its shaft in (where it stood when it reached the core).
fn strip_tile_column_of_shaft(world: &WorldState) -> i64 {
    // The shaft runs up from wherever the rig went down; the rig flew straight up it, so
    // its column now is the shaft's.
    strip_tile(world).0
}

#[test]
fn the_moon_bot_is_deterministic_and_tick_rate_safe() {
    let mut hashes = Vec::new();
    for _ in 0..2 {
        let mut world = moon(30);
        enable_bot_mode(&mut world);
        let run = run_bot(&mut world);
        hashes.push((world.simulation_state_hash(), run.events));
    }
    assert_eq!(hashes[0], hashes[1]);
    // And at 120 Hz the same moon plays through.
    let mut world = moon(120);
    enable_bot_mode(&mut world);
    let run = run_bot(&mut world);
    assert!(run.landed);
}

#[test]
fn gravity_is_the_bodys_and_pulls_down_the_strip() {
    let mut world = moon(60);
    let g = moon_params().gravity();
    assert!(g < 0.5, "a small moon pulls lightly: {g}");
    // Lift the rig high and drop it.
    let surface = world.terrain().surface_row(80);
    let (x, y) = world.frame().centre_of(80, surface - 40);
    {
        let player = world.player_mut(PLAYER).expect("rig");
        player.x = x;
        player.y = y;
        player.velocity_x = 0.0;
        player.velocity_y = 0.0;
    }
    let y0 = rig(&world).y;
    for _ in 0..30 {
        step(&mut world, idle());
    }
    let fallen = rig(&world).y - y0;
    // Half a second of fall under the moon's pull: roughly g/2 t^2 of the rig's 440 px/s^2.
    let expected = 0.5 * 440.0 * g * 0.25;
    assert!(fallen > 0.0, "falls down the strip");
    assert!(
        (fallen - expected).abs() < expected * 0.35,
        "fell {fallen} px, expected about {expected}"
    );
    // A cleared field zone list comes back next tick: the body's gravity stays.
    world.clear_field_zones();
    step(&mut world, idle());
    assert!(!world.field_zones().is_empty());
}

#[test]
fn gravity_never_turns_the_craft_in_flight() {
    let mut world = moon(60);
    let surface = world.terrain().surface_row(300);
    let (x, y) = world.frame().centre_of(300, surface - 60);
    {
        let player = world.player_mut(PLAYER).expect("rig");
        player.x = x;
        player.y = y;
        player.attitude.orientation = 1.0;
        player.attitude.angular_velocity = 0.0;
    }
    for _ in 0..60 {
        step(&mut world, idle());
        assert!(
            (rig(&world).attitude.orientation - 1.0).abs() < 1e-4,
            "only the thrusters turn the craft"
        );
    }
    // The player's rotation thrusters turn it all the way round (no limit in a body's sky).
    for _ in 0..240 {
        step(&mut world, idle().with_rotation(1.0));
    }
    let after = rig(&world).attitude;
    assert!(after.angular_velocity > 2.0, "{after:?}");
}

/// A planet world with the rig `rows_up` rows above a flat stretch of ground, turned to
/// `orientation`, with landing legs tier `legs`.
fn drop_test(orientation: f32, rows_up: i32, legs: u8) -> WorldState {
    let mut world = moon(60);
    let terrain = world.terrain();
    // A column whose neighbours are level (no slope to muddy the landing).
    let column = (40..2_000)
        .find(|column| {
            let row = terrain.surface_row(*column);
            (-3..=3).all(|dx| terrain.surface_row(column + dx) == row)
        })
        .expect("flat ground");
    let surface = terrain.surface_row(column);
    let (x, y) = world.frame().centre_of(column, surface - rows_up);
    let player = world.player_mut(PLAYER).expect("rig");
    player.x = x;
    player.y = y;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    player.attitude.orientation = orientation;
    player.attitude.angular_velocity = 0.0;
    player.hazard_gear.attitude.landing_legs = legs;
    world
}

fn land(world: &mut WorldState) -> Vec<PlanetEvent> {
    let mut events = Vec::new();
    for _ in 0..600 {
        events.extend(planet_events(&step(world, idle())));
        if !events.is_empty() {
            break;
        }
    }
    events
}

#[test]
fn a_level_touchdown_is_clean_and_rests_level() {
    let mut world = drop_test(0.0, 6, 0);
    let hull = rig(&world).hull;
    assert_eq!(land(&mut world), vec![PlanetEvent::Landed]);
    assert!((rig(&world).hull - hull).abs() < f32::EPSILON);
    for _ in 0..60 {
        step(&mut world, idle());
    }
    assert!(rig(&world).attitude.orientation.abs() < 1e-3);
}

#[test]
fn a_tilted_touchdown_slides_or_scrapes_then_settles_on_its_tracks() {
    let mut world = drop_test(30.0 * DEG, 10, 0);
    let hull = rig(&world).hull;
    let events = land(&mut world);
    assert!(
        events.contains(&PlanetEvent::HullScraped) || events.contains(&PlanetEvent::Slid),
        "{events:?}"
    );
    assert!(rig(&world).velocity_x > 0.0, "slides the way it leans");
    assert!(rig(&world).hull <= hull);
    // Contact rolls it back onto its tracks: smoothly (never a snap in one tick), then level.
    let mut previous = rig(&world).attitude.orientation;
    for _ in 0..120 {
        step(&mut world, idle());
        let now = rig(&world).attitude.orientation;
        assert!((now - previous).abs() < 0.2, "a roll, not a snap");
        previous = now;
    }
    assert!(previous.abs() < 1e-3, "settled level: {previous}");
}

#[test]
fn landing_legs_make_the_same_touchdown_clean() {
    let mut world = drop_test(20.0 * DEG, 10, 1);
    assert_eq!(land(&mut world), vec![PlanetEvent::Landed]);
}

#[test]
fn a_steep_touchdown_tips_over_and_the_player_rights_it() {
    let mut world = drop_test(75.0 * DEG, 8, 0);
    let hull = rig(&world).hull;
    let events = land(&mut world);
    assert_eq!(events, vec![PlanetEvent::Tipped]);
    assert!(rig(&world).hull < hull - 5.0, "a tip-over knocks the hull");
    // It falls onto its side and lies there: the tracks cannot drive it.
    for _ in 0..90 {
        step(&mut world, ControlState::new(1.0, false, false));
    }
    let lying = rig(&world).attitude.orientation;
    assert!(
        (lying - std::f32::consts::FRAC_PI_2).abs() < 0.05,
        "on its side: {lying}"
    );
    assert!(rig(&world).velocity_x.abs() < 1.0, "no drive while tipped");
    // Holding the rotation thrusters toward upright rights it.
    let mut righted = false;
    for _ in 0..240 {
        let events = planet_events(&step(&mut world, idle().with_rotation(-1.0)));
        if events.contains(&PlanetEvent::Righted) {
            righted = true;
        }
        if righted && rig(&world).attitude.orientation.abs() < tip_angle(0) * 0.5 {
            break;
        }
    }
    assert!(righted, "righted: {:?}", rig(&world).attitude);
    for _ in 0..90 {
        step(&mut world, idle());
    }
    assert!(rig(&world).attitude.orientation.abs() < 1e-3);
}

#[test]
fn walking_round_the_moon_on_foot_or_by_rig_wraps_the_tiles() {
    // Digging a tile on one lap is dug on every lap: one place, one tile.
    let mut world = moon(60);
    let width = world.planet().expect("planet").strip_width();
    let below = TilePosition { x: 70, y: 20 };
    assert!(
        world
            .terrain_mut()
            .set_kind(below, crate::terrain::TileKind::Air)
    );
    for lap in [-2, -1, 1, 3] {
        let lapped = TilePosition {
            x: below.x + lap * width,
            y: below.y,
        };
        assert_eq!(world.terrain().tile(lapped), world.terrain().tile(below));
    }
    assert_eq!(world.terrain().edited_chunk_count(), 1);
}

#[test]
fn a_planet_world_saves_and_restores_with_its_ground() {
    let mut world = moon(60);
    enable_bot_mode(&mut world);
    // Dig a little shaft through a ring boundary's tile so the save holds mirrored edits.
    let planet = *world.planet().expect("planet");
    let row = planet.grid().halving_rows()[0];
    assert!(
        world
            .terrain_mut()
            .set_kind(TilePosition { x: 9, y: row }, crate::terrain::TileKind::Air)
    );
    for _ in 0..30 {
        step(&mut world, ControlState::new(1.0, false, false));
    }
    let json = serde_json::to_string(world.terrain()).expect("save");
    let restored: crate::terrain::Terrain = serde_json::from_str(&json).expect("load");
    assert_eq!(restored.digest(), world.terrain().digest());
    assert_eq!(
        restored.planet_ground().map(|planet| *planet.params()),
        Some(moon_params())
    );
    for x in 0..16 {
        let position = TilePosition { x, y: row };
        assert_eq!(restored.tile(position), world.terrain().tile(position));
    }
    // The binary keyframe codec too.
    let bytes = postcard::to_allocvec(world.terrain()).expect("encode");
    let decoded: crate::terrain::Terrain = postcard::from_bytes(&bytes).expect("decode");
    assert_eq!(decoded.digest(), world.terrain().digest());
    assert_eq!(decoded.frame(), world.terrain().frame());
}

#[test]
fn a_replica_follows_the_frame_across_the_seam() {
    let mut world = moon(60);
    enable_bot_mode(&mut world);
    let width = world.planet().expect("planet").strip_width();
    // Park the rig just short of the seam, then drive across it.
    let surface = world.terrain().surface_row(width - 40);
    let (x, y) = world.frame().centre_of(width - 40, surface - 2);
    world.teleport_rig_to(PLAYER, world.world_pos(x, y));
    let mut replica = world.clone();
    let before = world.actor_world_pos(PLAYER).expect("rig");
    for _ in 0..600 {
        step(&mut world, ControlState::new(1.0, false, false));
    }
    let (column, _) = strip_tile(&world);
    assert!(column < 400, "crossed the seam: {column}");
    let frame = world.frame();
    assert!((0..world.planet().expect("planet").grid().lap_sectors()).contains(&frame.sector_x));
    // A replica moved into the host's frame keeps its rig where it was (whole laps move
    // nothing).
    replica.move_frame(frame);
    let after = replica.actor_world_pos(PLAYER).expect("rig");
    let lap_px = f64::from(width) * f64::from(TILE_SIZE);
    let (dx, dy) = before.delta_f64(after);
    let laps = (dx / lap_px).round();
    assert!(
        (dx - laps * lap_px).abs() < 0.01 && dy.abs() < 0.01,
        "{dx} {dy}"
    );
}

#[test]
fn ground_gameplay_reuses_the_home_world_code() {
    // A planet strip is a streamed world: drops, digging and the dig journal behave as at
    // home. Dig one tile by hand and see a drop or ore count change like anywhere else.
    let mut world = moon(60);
    enable_bot_mode(&mut world);
    let before = world.terrain().stored_tile_count();
    for _ in 0..120 {
        step(&mut world, ControlState::new(0.0, false, true));
    }
    assert!(
        world.terrain().stored_tile_count() > before,
        "the drill dug"
    );
    // No home town on a moon.
    assert!(world.player_surface_zone(PLAYER).is_none());
    assert_eq!(
        world.terrain().layer_name(TilePosition { x: 0, y: -100 }),
        world.planet().expect("planet").theme().name
    );
    let _ = world.planet().expect("planet").gravity();
}
