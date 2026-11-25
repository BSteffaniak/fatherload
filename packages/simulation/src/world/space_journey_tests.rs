//! The whole Wave 11 loop, end to end (integration): the Heart's ending gives the Star Drive,
//! a rig flies up past the Moon into space, warps to a system from the Archive's star charts
//! (a real system of the space generator), sets a nav waypoint on a small moon there, comes in,
//! re-enters, lands on its round ground (radial gravity, the polar strip), drives all the way
//! round it, digs toward the core, takes off, warps to another star (home's), docks at Halcyon
//! Dock and flies home. The bot ([`crate::flight::autopilot::SpaceBot`]) does it with the
//! player's controls only (rotation thrusters, main, retro and side thrust, cruise, warp,
//! use); no assist is fitted.

use crate::{
    ControlState, PlayerCommand, PlayerId, SimulationParams, TickInputs, WorldState,
    enemy::EnemyDifficulty,
    event::TickOutput,
    flight::{
        FlightEvent, FlightPlace,
        autopilot::{Leg, SpaceBot, chart_with_a_small_moon, home_dock, strip_tile},
        nav::NavTarget,
        space::{self, FlightBody, SpaceQuery, SystemRef},
    },
    place::{BodyId, Place, StoreKey},
    position::WorldPos,
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x5EED_0011;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// One tick with `player`'s held controls (and an action, if any).
fn tick(
    world: &mut WorldState,
    player: PlayerId,
    controls: ControlState,
    command: Option<PlayerCommand>,
) -> TickOutput {
    let mut inputs = TickInputs::held(player, controls);
    if let Some(command) = command {
        inputs = inputs.act(player, command);
    }
    world.simulate_tick(world.simulation_tick(), &inputs)
}

/// Runs `bot` for the player until it has finished `legs` legs in all; returns the flight
/// events of those ticks.
fn fly(world: &mut WorldState, bot: &mut SpaceBot, legs: usize) -> Vec<FlightEvent> {
    let mut seen = Vec::new();
    while bot.finished.len() < legs {
        // A leg that ends inside `next` hands over to the next leg's first move at once.
        let (controls, command) = bot.next(world, PLAYER);
        let output = tick(world, PLAYER, controls, command);
        let events: Vec<_> = output.events.iter().map(|event| event.kind).collect();
        for event in &events {
            if let crate::GameplayEventKind::Flight { what, .. } = event {
                seen.push(*what);
            }
        }
        bot.observe(&events);
    }
    seen
}

/// A V11 world at `hz` with one rig in test mode (top engine and drill, infinite fuel: the
/// bot proves the places, not the economy), the Heart awake and the chart read; the rig just
/// under the space line over the Moon.
fn journey_world(hz: u32) -> (WorldState, SystemRef, FlightBody) {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.enable_test_mode();
    // No assists: the bot flies and lands on the thrusters alone.
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.hazard_gear.attitude = crate::attitude::AttitudeGear::default();
        rig.hazard_gear.flight = crate::flight::FlightGear::default();
    }
    // The Heart's ending: space opens and every rig gets the Star Drive.
    let mut output = TickOutput::default();
    world.heart_wakes(None, 0, &mut output);
    assert!(world.space_access(PLAYER).may_leave());
    // The Archive's chart, leading to a real system of the space generator.
    let (chart, system, body) = chart_with_a_small_moon(SEED).expect("a charted small moon");
    world.ancient_city.charts.push(chart);
    assert!(world.known_systems().contains(&system));
    let line = world.space_line_row().expect("space line");
    let column = crate::worldgen::sky::moon_column(world.terrain().settings()).expect("moon") + 600;
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(column), i64::from(line + 6)),
    );
    world.stream_terrain_near_players();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    (world, system, body)
}

/// The whole journey at `hz`; returns the world at its end.
#[allow(clippy::too_many_lines, reason = "the journey's legs read in order")]
fn journey(hz: u32) -> WorldState {
    let (mut world, system, body) = journey_world(hz);
    let galaxy = world.space();
    let station = home_dock(&world).expect("Halcyon Dock");
    let ground = world.body_ground(body.body).expect("round ground");
    let width = i64::from(ground.params.grid().surface_columns());
    let mut bot = SpaceBot::new([
        Leg::Leave,
        Leg::Warp(system),
        Leg::Waypoint(NavTarget::Body(body.body)),
        Leg::Land(body),
        Leg::Drive(width),
        Leg::Dig(24),
        Leg::TakeOff,
        Leg::Warp(space::HOME_SYSTEM),
        Leg::Dock(station),
        Leg::Undock,
        Leg::Home,
    ]);
    // 1. Up past the Moon into space, seamlessly: the rig is now in free flight.
    assert!(fly(&mut world, &mut bot, 1).contains(&FlightEvent::EnteredSpace));
    assert_eq!(world.place_of(PLAYER), Place::Space);
    assert!(world.store_of(PLAYER).is_home());
    let launched = world.player(PLAYER).expect("rig").flight.pos;
    assert!(launched.distance(space::home_launch()) < space::tiles(50.0));
    // 2. Warp to the charted system.
    assert!(fly(&mut world, &mut bot, 2).contains(&FlightEvent::WarpArrived));
    let star = galaxy.system(system).expect("system").star;
    let arrived = world.player(PLAYER).expect("rig").flight.pos;
    assert!(
        arrived.distance(star) < space::tiles(260_000.0),
        "arrived at the chart's star"
    );
    // 3. The nav HUD's waypoint on the moon.
    fly(&mut world, &mut bot, 3);
    let _ = tick(&mut world, PLAYER, ControlState::default(), None);
    assert_eq!(world.waypoint_of(PLAYER), Some(NavTarget::Body(body.body)));
    assert_eq!(
        NavTarget::Body(body.body).position(&galaxy),
        Some(body.centre)
    );
    // 4. Approach, re-entry and the landing on its round ground.
    fly(&mut world, &mut bot, 4);
    let id = galaxy.space_body(body.body).expect("a generated body");
    assert_eq!(world.place_of(PLAYER), Place::OnBody(BodyId::Space(id)));
    assert_eq!(world.store_of(PLAYER), StoreKey::Body(id));
    let planet = *world
        .store_terrain(StoreKey::Body(id))
        .and_then(crate::terrain::Terrain::planet_ground)
        .expect("a round body's strip");
    assert_eq!(i64::from(planet.strip_width()), width);
    assert!(matches!(
        world.player(PLAYER).expect("rig").flight.place,
        FlightPlace::Landed(_)
    ));
    // The home world's store is untouched by the body's ground.
    assert!(world.terrain().planet_ground().is_none());
    // 5. All the way round the moon (the strip wraps).
    fly(&mut world, &mut bot, 5);
    // 6. Toward the core.
    let (_, surface_row) = strip_tile(&world, PLAYER).expect("on the ground");
    fly(&mut world, &mut bot, 6);
    let (_, row) = strip_tile(&world, PLAYER).expect("on the ground");
    assert!(row >= surface_row + 20, "dug {surface_row} -> {row}");
    // 7. Take off, back into space round the same body.
    assert!(fly(&mut world, &mut bot, 7).contains(&FlightEvent::TookOff));
    assert_eq!(world.place_of(PLAYER), Place::Space);
    let flight = world.player(PLAYER).expect("rig").flight;
    assert!(
        flight.pos.distance(body.centre) < body.radius + space::tiles(200.0),
        "left from the body's surface"
    );
    // The dug shaft stays in the body's store (frozen while nobody is there).
    let edited = world
        .places()
        .store(StoreKey::Body(id))
        .expect("kept")
        .terrain()
        .edited_chunk_count();
    assert!(edited > 0);
    // 8. Warp to another star: home's.
    assert!(fly(&mut world, &mut bot, 8).contains(&FlightEvent::WarpArrived));
    // 9. Dock at Halcyon Dock, then undock.
    let docking = fly(&mut world, &mut bot, 10);
    assert!(docking.contains(&FlightEvent::Docked));
    assert!(docking.contains(&FlightEvent::Undocked));
    // 10. Home.
    let home = fly(&mut world, &mut bot, 11);
    assert!(home.contains(&FlightEvent::ReturnedHome));
    assert_eq!(world.place_of(PLAYER), Place::HOME);
    assert!(!world.in_space(PLAYER));
    assert!(world.player(PLAYER).expect("rig").hull > 0.0);
    eprintln!(
        "space journey at {hz} Hz: {} ticks in all; legs {:?}",
        world.simulation_tick().get(),
        bot.finished
            .iter()
            .map(|(_, ticks)| *ticks)
            .collect::<Vec<_>>()
    );
    world
}

#[test]
fn a_bot_flies_from_the_heart_to_the_stars_lands_on_a_round_moon_and_comes_home() {
    let first = journey(30);
    // Deterministic: the same journey again ends in the same world, bit for bit.
    let again = journey(30);
    assert_eq!(first.simulation_state_hash(), again.simulation_state_hash());
}

#[test]
fn the_journey_plays_through_at_sixty_hertz_too() {
    let world = journey(60);
    assert!(
        world
            .places()
            .stores()
            .any(|(key, _)| matches!(key, StoreKey::Body(_)))
    );
}
