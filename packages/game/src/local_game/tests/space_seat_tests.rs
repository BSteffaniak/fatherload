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
#[allow(
    clippy::too_many_lines,
    reason = "one journey of two seats, then each seat's view"
)]
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
    // Wave 12 systems: free flight is its own store (no ground to draw).
    assert!(
        snapshot.views[1]
            .place_scene
            .as_ref()
            .is_none_or(|scene| scene.store == StoreKey::Space && scene.body.is_none())
    );
    // The seat on the moon reads the moon's own life (Wave 12 bodylife): its hour, not the
    // home clock's, its air and gravity.
    let life = ground
        .body
        .as_ref()
        .and_then(|body| body.life.as_ref())
        .expect("the body's life for its seat");
    let body_life = world.body_life_of_player(seats[0]).expect("on a body");
    assert_eq!(
        life.reading.local_permille,
        world
            .body_reading_of(seats[0])
            .expect("a reading")
            .local_permille
    );
    assert_eq!(life.gravity_milli, body_life.gravity_milli());
    assert!(!life.name.is_empty());
    assert!(snapshot.views[1].player.flight.away(), "seat 1 flies");
}

/// A landable body near home whose space theme is `theme` (the first found).
fn body_of_theme(seed: u64, theme: fatherload_simulation::space::Theme) -> Place {
    let space = fatherload_simulation::space::Space::shared(seed);
    for ring in 0..=6_i32 {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let sector = fatherload_simulation::space::SpaceSector::new(x, y);
                for system in &space.sector(sector).systems {
                    for body in &system.bodies {
                        let id = fatherload_simulation::place::SpaceBodyKey {
                            sector,
                            system: system.index,
                            body: body.index,
                        };
                        if body.theme == theme
                            && fatherload_simulation::place::planet_params_for(seed, id).is_some()
                        {
                            return Place::OnBody(BodyId::Space(id));
                        }
                    }
                }
            }
        }
    }
    panic!("no {theme:?} body near home");
}

/// Split screen with the two seats on two theme worlds (Wave 12 integration): each seat's
/// view is its own body's round ground, with its own theme and its own life.
#[test]
fn split_screen_seats_on_a_jungle_and_a_volcanic_world_each_see_their_own() {
    use fatherload_simulation::space::Theme;
    let settings = WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    };
    let world = WorldState::new_game_with_settings(
        PLAYER,
        &settings,
        fatherload_simulation::SimulationParams::new(30),
    );
    let mut game = playing(world, 2, "qa-theme-seats");
    let seats: Vec<PlayerId> = game
        .runtime
        .local_seats()
        .iter()
        .map(|seat| seat.player)
        .collect();
    let places = [
        body_of_theme(SEED, Theme::Jungle),
        body_of_theme(SEED, Theme::Volcanic),
    ];
    game.edit_world(|world| {
        world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
        world.enable_test_mode();
        for (player, place) in seats.iter().zip(places) {
            if let Some(rig) = world.player_mut(*player) {
                fatherload_simulation::economy::max_out_upgrades(rig);
                rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
                rig.hull = rig.max_hull();
            }
            let mut inputs = TickInputs::default().hold_as(*player, ControlState::default());
            inputs.act_as(
                *player,
                PlayerCommand::TestMode(fatherload_simulation::TestModeAction::WarpToPlace(place)),
            );
            let _ = world.simulate_tick(world.simulation_tick(), &inputs);
        }
    });
    idle(&mut game, 60);
    let world = game.world();
    for (player, place) in seats.iter().zip(places) {
        assert_eq!(world.place_of(*player), place, "each seat on its own world");
        assert!(world.body_reading_of(*player).is_some(), "its own reading");
    }
    let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
    assert_eq!(snapshot.views.len(), 2);
    let themes: Vec<_> = snapshot
        .views
        .iter()
        .zip(places)
        .map(|(view, place)| {
            let scene = view.place_scene.as_ref().expect("each seat sees its place");
            assert_eq!(scene.store, place.store(), "its own store");
            let ground = scene.terrain.planet_ground().expect("round ground");
            let life = scene
                .body
                .as_ref()
                .and_then(|body| body.life.as_ref())
                .expect("the body's life for its seat");
            assert!(!life.name.is_empty());
            ground.params().theme
        })
        .collect();
    assert_eq!(
        themes,
        vec![
            fatherload_simulation::place::planet_theme_of(Theme::Jungle),
            fatherload_simulation::place::planet_theme_of(Theme::Volcanic),
        ],
        "each seat draws its own world"
    );
}

/// Events are routed by place (Wave 13 QA): a player-less event of one body (an eruption on a
/// volcanic world) reaches the seat on that body (its message and a sound in that store) and
/// never the seat at home, whose rig may stand at the same pixels of its own ground.
#[test]
fn a_player_less_event_on_a_body_reaches_only_the_seat_there() {
    use fatherload_simulation::{
        GameplayEvent, GameplayEventId, GameplayEventKind, SimulationTick, frontier::FrontierEvent,
        space::Theme,
    };
    let settings = WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    };
    let world = WorldState::new_game_with_settings(
        PLAYER,
        &settings,
        fatherload_simulation::SimulationParams::new(30),
    );
    let mut game = playing(world, 2, "qa-routed-events");
    let seats: Vec<PlayerId> = game
        .runtime
        .local_seats()
        .iter()
        .map(|seat| seat.player)
        .collect();
    let volcanic = body_of_theme(SEED, Theme::Volcanic);
    game.edit_world(|world| {
        world.enable_test_mode();
        if let Some(rig) = world.player_mut(seats[0]) {
            rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        }
        let mut inputs = TickInputs::default().hold_as(seats[0], ControlState::default());
        inputs.act_as(
            seats[0],
            PlayerCommand::TestMode(fatherload_simulation::TestModeAction::WarpToPlace(volcanic)),
        );
        let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    });
    idle(&mut game, 10);
    assert_eq!(game.world().place_of(seats[0]), volcanic);
    assert!(game.world().place_of(seats[1]).store().is_home());
    let (x, y) = game
        .world()
        .player(seats[1])
        .map(|rig| (rig.x, rig.y))
        .expect("the home seat's rig");
    #[allow(clippy::cast_possible_truncation, reason = "test pixels")]
    let eruption = |store| RuntimeEvent {
        epoch: 1,
        tick: 9_999,
        player: None,
        kind: 1,
        ordinal: 0,
        event: RuntimeEventKind::Gameplay(GameplayEvent {
            id: GameplayEventId {
                tick: SimulationTick::new(9_999),
                ordinal: 0,
            },
            kind: GameplayEventKind::Frontier {
                what: FrontierEvent::EruptionStarted,
                player_id: None,
                x: (x / 16.0) as i32,
                y: (y / 16.0) as i32,
            },
            store,
        }),
    };
    for seat in 0..2 {
        game.shell.seat_mut(seat).message.clear();
    }
    game.frame_events = vec![eruption(volcanic.store())];
    game.observe_events();
    assert!(
        game.shell.seat(0).message.contains("ERUPTION"),
        "the seat on the volcanic world is told: {:?}",
        game.shell.seat(0).message
    );
    assert!(
        game.shell.seat(1).message.is_empty(),
        "the seat at home is not: {:?}",
        game.shell.seat(1).message
    );
    // Its sounds are the body's: a listener elsewhere never hears them.
    let cues = game
        .effects
        .apply(&game.frame_events, game.runtime.world(), &seats);
    assert!(cues.iter().all(|cue| cue.store == volcanic.store()));
    // With only the home seat local, a body's player-less event makes no sound at all.
    let mut effects = crate::effects::Effects::default();
    let home_only = effects.apply(
        &[eruption(volcanic.store())],
        game.runtime.world(),
        &seats[1..],
    );
    assert!(home_only.is_empty(), "nobody local is there: {home_only:?}");
}
