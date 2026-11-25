//! Split screen with seats in two places (Wave 11 integration): one seat's rig flies the
//! space bot down onto a charted moon's round ground while the other's climbs past the Moon
//! into space; each seat's view shows its own place (the body's polar strip for one, free
//! flight for the other), and the game keeps ticking both places.

use fatherload_simulation::{
    ControlState, PlayerCommand, TickInputs,
    flight::autopilot::{Leg, SpaceBot, chart_with_a_small_moon, home_dock},
    place::{BodyId, Place, StoreKey},
    position::WorldPos,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::*;

const SEED: u64 = 0x5EED_0011;

/// Runs one tick of the world with both seats' bots.
fn tick_both(world: &mut WorldState, bots: &mut [(PlayerId, SpaceBot); 2]) {
    let mut inputs = TickInputs::default();
    let mut commands: Vec<(PlayerId, PlayerCommand)> = Vec::new();
    for (player, bot) in bots.iter_mut() {
        let (controls, command): (ControlState, Option<PlayerCommand>) = bot.next(world, *player);
        inputs = inputs.hold_as(*player, controls);
        if let Some(command) = command {
            commands.push((*player, command));
        }
    }
    for (player, command) in commands {
        inputs.act_as(player, command);
    }
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    let events: Vec<_> = output.events.iter().map(|event| event.kind).collect();
    for (player, bot) in bots.iter_mut() {
        let mine: Vec<_> = events
            .iter()
            .copied()
            .filter(|event| match event {
                fatherload_simulation::GameplayEventKind::Flight { player_id, .. } => {
                    player_id == player
                }
                fatherload_simulation::GameplayEventKind::Planet { player_id, .. } => {
                    *player_id == Some(*player)
                }
                _ => false,
            })
            .collect();
        bot.observe(&mine);
    }
}

#[test]
fn split_screen_seats_in_two_places_each_see_their_own() {
    let settings = WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    };
    let world = WorldState::new_game_with_settings(
        PLAYER,
        &settings,
        fatherload_simulation::SimulationParams::new(30),
    );
    let mut game = playing(world, 2, "qa-space-seats");
    let seats: Vec<PlayerId> = game
        .runtime
        .local_seats()
        .iter()
        .map(|seat| seat.player)
        .collect();
    assert_eq!(seats.len(), 2);
    let (chart, system, moon) = chart_with_a_small_moon(SEED).expect("a charted small moon");
    let dock = home_dock(game.world()).expect("Halcyon Dock");
    let mut bots = [
        (
            seats[0],
            SpaceBot::new([Leg::Leave, Leg::Warp(system), Leg::Land(moon)]),
        ),
        (seats[1], SpaceBot::new([Leg::Leave, Leg::Dock(dock)])),
    ];
    game.edit_world(|world| {
        world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Peaceful);
        world.enable_test_mode();
        world.record_star_chart(chart);
        let mut heart = world.heart().clone();
        heart.space_unlocked = true;
        for player in &seats {
            heart.star_drive.insert(*player);
        }
        world.set_heart(heart);
        let line = world.space_line_row().expect("space line");
        let column = fatherload_simulation::worldgen::sky::moon_column(world.terrain().settings())
            .expect("moon");
        for (index, player) in seats.iter().enumerate() {
            world.teleport_rig_to(
                *player,
                WorldPos::tile_center(
                    i64::from(column + 600 + 20 * i32::try_from(index).unwrap_or(0)),
                    i64::from(line + 6),
                ),
            );
            if let Some(rig) = world.player_mut(*player) {
                rig.hazard_gear.attitude = fatherload_simulation::attitude::AttitudeGear::default();
                rig.hazard_gear.flight = fatherload_simulation::flight::FlightGear::default();
                rig.velocity_x = 0.0;
                rig.velocity_y = 0.0;
            }
        }
        world.stream_terrain_near_players();
        // Both bots fly their legs (seat 0 down onto the moon, seat 1 out past the Moon to
        // Halcyon Dock, where it stays docked).
        for _ in 0..30 * 900 {
            if bots.iter().all(|(_, bot)| bot.done()) {
                break;
            }
            tick_both(world, &mut bots);
        }
    });
    assert!(bots.iter().all(|(_, bot)| bot.done()), "both bots flew");
    let world = game.world();
    let id = world
        .space()
        .space_body(moon.body)
        .expect("a generated body");
    assert_eq!(world.place_of(seats[0]), Place::OnBody(BodyId::Space(id)));
    assert_eq!(world.place_of(seats[1]), Place::Space);
    // The game keeps ticking both places through its own frame loop.
    idle(&mut game, 30);
    let world = game.world();
    assert_eq!(world.store_of(seats[0]), StoreKey::Body(id));
    assert!(world.in_space(seats[1]));
    // Each seat's view: seat 0 the moon's round ground, seat 1 free flight.
    let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
    assert_eq!(snapshot.views.len(), 2);
    let ground = snapshot.views[0]
        .place_scene
        .as_ref()
        .expect("seat 0 sees its place");
    assert_eq!(ground.store, StoreKey::Body(id));
    assert!(ground.body.is_some(), "the body's place in space");
    assert!(
        ground.terrain.planet_ground().is_some(),
        "the round body's polar strip"
    );
    assert!(
        crate::rendering::space::live::LiveStrip::of(ground.terrain).is_some(),
        "the polar renderer reads it"
    );
    assert!(snapshot.views[1].place_scene.is_none());
    assert!(snapshot.views[1].player.flight.away(), "seat 1 flies");
}
