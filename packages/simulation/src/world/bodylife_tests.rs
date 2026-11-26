//! Living bodies in the world (Wave 12 bodylife): each body's own day, its weather's wind and
//! hazards, its climate in place of the home bands, its air, one gravity per body, and its
//! markets. Lone planet worlds stand in for a body where the test needs only one; two real
//! bodies of a V11 world's space show seats in different places reading their own day.

use crate::flight::space::SpaceQuery;
use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WorldState,
    bodylife::{WeatherId, weather::SPELL_MILLIS, weather::next_spell_of},
    enemy::EnemyDifficulty,
    place::{BodyId, Place, SpaceBodyKey, StoreKey},
    planet::{BodyGround, PlanetGround, PlanetParams, PlanetThemeId},
    position::WorldPos,
    space::{Atmosphere, SpaceSector},
    terrain::{MineralKind, TILE_SIZE, TilePosition},
    world::{BodyLifeEvent, pilot::PilotAir},
    worldgen::{GeneratorVersion, WorldSettings},
};

const ONE: PlayerId = PlayerId::new(1);
const TWO: PlayerId = PlayerId::new(2);
const SEED: u64 = 0x0B0D_11FE;

fn lone(theme: PlanetThemeId, gravity_milli: u16, hz: u32) -> WorldState {
    let params = PlanetParams {
        gravity_milli,
        ..PlanetParams::new(0x00B0_D1E5, 640, theme)
    };
    let mut world = WorldState::new_planet_world(ONE, params, SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world
}

fn step(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let tick = world.simulation_tick();
    let inputs = world
        .players()
        .keys()
        .fold(TickInputs::default(), |inputs, id| {
            inputs.hold_as(*id, controls)
        });
    world
        .simulate_tick(tick, &inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn idle() -> ControlState {
    ControlState::new(0.0, false, false)
}

/// Puts the rig `rows` rows over the surface of strip column `column`, still.
fn hover(world: &mut WorldState, column: i32, rows: i32) {
    let surface = world.terrain().surface_row(column);
    let (x, y) = world.frame().centre_of(column, surface - rows);
    let rig = world.player_mut(ONE).expect("rig");
    rig.x = x;
    rig.y = y;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    world.stream_terrain_near_players();
}

fn rig_column(world: &WorldState) -> i64 {
    let rig = world.player(ONE).expect("rig");
    i64::from(world.frame().tile_at(rig.x, rig.y).x)
}

#[test]
fn a_storm_carries_an_airborne_rig_and_shelter_stops_it() {
    let mut world = lone(PlanetThemeId::ROCKY, 500, 60);
    let life = world
        .body_life(StoreKey::Home)
        .expect("a lone planet's life");
    assert_eq!(life.climate.atmosphere, Atmosphere::Breathable);
    let table = life.hooks().weather_table();
    let storm = next_spell_of(
        life.seed,
        table,
        Atmosphere::Breathable,
        WeatherId::STORM,
        0,
        400,
    )
    .expect("a storm on a rocky world");
    // Mid-spell: full strength (the ramp is shorter than half a spell).
    world.set_body_millis(storm - storm % SPELL_MILLIS + SPELL_MILLIS / 2);
    hover(&mut world, 200, 14);
    let rig = world.player(ONE).expect("rig");
    let wind = world
        .body_wind_at(rig.x, rig.y)
        .expect("wind under the open sky");
    assert!(wind.abs() > 60.0, "a storm's wind {wind}");
    assert!(
        world.body_visibility_at(rig.x, rig.y) < 0.6,
        "a storm hides the view"
    );
    for _ in 0..30 {
        let _ = step(&mut world, idle());
    }
    let drift = world.player(ONE).expect("rig").velocity_x;
    assert!(
        drift * wind > 0.0,
        "carried with the wind: {drift} vs {wind}"
    );
    assert!(drift.abs() > 20.0, "drift {drift}");
    // Under ground the sky does not reach.
    let rig = world.player(ONE).expect("rig");
    let tile = world.frame().tile_at(rig.x, rig.y);
    let below = TilePosition {
        x: tile.x,
        y: world.terrain().surface_row(tile.x) + 12,
    };
    assert!(!world.open_sky_at(below));
    let (bx, by) = world.frame().centre_of(below.x, below.y);
    assert!(world.body_wind_at(bx, by).is_none());
    // On a clear day nothing pushes.
    let clear = next_spell_of(
        life.seed,
        table,
        Atmosphere::Breathable,
        WeatherId::CLEAR,
        0,
        400,
    )
    .expect("a clear spell");
    world.set_body_millis(clear - clear % SPELL_MILLIS + SPELL_MILLIS / 2);
    hover(&mut world, 200, 14);
    for _ in 0..30 {
        let _ = step(&mut world, idle());
    }
    assert!(world.player(ONE).expect("rig").velocity_x.abs() < 1.0);
}

#[test]
fn the_wind_drifts_the_same_at_every_tick_rate() {
    let drift_at = |hz: u32| {
        let mut world = lone(PlanetThemeId::ROCKY, 500, hz);
        let life = world.body_life(StoreKey::Home).expect("life");
        let storm = next_spell_of(
            life.seed,
            life.hooks().weather_table(),
            Atmosphere::Breathable,
            WeatherId::STORM,
            0,
            400,
        )
        .expect("a storm");
        world.set_body_millis(storm - storm % SPELL_MILLIS + SPELL_MILLIS / 2);
        hover(&mut world, 200, 30);
        let start = world.player(ONE).expect("rig").x;
        for _ in 0..hz / 2 {
            let _ = step(&mut world, idle());
        }
        world.player(ONE).expect("rig").x - start
    };
    let (a, b, c) = (drift_at(30), drift_at(60), drift_at(120));
    assert!(a.abs() > 4.0, "drift {a}");
    for other in [b, c] {
        assert!(
            (a - other).abs() < a.abs().mul_add(0.2, 2.0),
            "{a} vs {other}"
        );
    }
}

#[test]
fn a_bodys_day_dawns_and_sets_at_each_column() {
    let mut world = lone(PlanetThemeId::BARREN, 400, 30);
    hover(&mut world, 300, 0);
    let life = world.body_life(StoreKey::Home).expect("life");
    let column = rig_column(&world);
    // Midnight is dark.
    let midnight = life.clock.millis_when(world.body_millis(), column, 0);
    world.set_body_millis(midnight);
    let night = world.body_reading_of(ONE).expect("a reading");
    // Just before the local dawn (local 06:00 is the sun on the horizon).
    let when = life.clock.millis_when(world.body_millis(), column, 248);
    world.set_body_millis(when);
    let mut dawned = false;
    for _ in 0..30 * 8 {
        dawned |= step(&mut world, idle()).iter().any(|event| {
            matches!(
                event,
                GameplayEventKind::BodyLife {
                    what: BodyLifeEvent::Dawn,
                    player_id: ONE,
                    ..
                }
            )
        });
    }
    assert!(dawned, "the sun rose at the rig's column");
    // Half a lap round it is the other side of the day.
    let width = i64::from(world.planet().expect("planet").strip_width());
    let here = life.clock.local_permille(world.body_millis(), column);
    let there = life
        .clock
        .local_permille(world.body_millis(), column + width / 2);
    assert_eq!((here + 500) % 1_000, there);
    // The night was dark; noon is bright.
    let noon = life.clock.millis_when(world.body_millis(), column, 500);
    world.set_body_millis(noon);
    let day = world.body_reading_of(ONE).expect("a reading");
    assert!(night.light_permille < 400 && day.light_permille > 900);
    assert!(day.climate.temperature_c > night.climate.temperature_c + 100);
}

#[test]
fn the_climate_replaces_the_home_bands_on_a_body() {
    let mut world = lone(PlanetThemeId::BARREN, 400, 30);
    hover(&mut world, 300, 0);
    for _ in 0..30 {
        let _ = step(&mut world, idle());
    }
    let life = world.body_life(StoreKey::Home).expect("life");
    let column = rig_column(&world);
    let planet = *world.planet().expect("planet");
    // Deep down the heat comes on; the surface at noon on an airless moon is merely hot.
    let deep = TilePosition {
        x: 300,
        y: planet.core_chamber_top() - 2,
    };
    let noon = life.clock.millis_when(world.body_millis(), column, 500);
    world.set_body_millis(noon);
    assert!(world.body_heat_units_at(deep).expect("on a body") > 3);
    let surface = TilePosition {
        x: 300,
        y: planet.surface_row(300) - 1,
    };
    assert_eq!(world.body_heat_units_at(surface), Some(0));
    assert_eq!(world.body_cold_at(surface), Some(0));
    // The airless night freezes a stock rig standing in the open.
    let midnight = life.clock.millis_when(world.body_millis(), column, 0);
    world.set_body_millis(midnight);
    assert!(world.body_cold_at(surface).expect("on a body") >= 2);
    let before = world.player(ONE).expect("rig").hull;
    for _ in 0..60 {
        let _ = step(&mut world, idle());
    }
    let after = world.player(ONE).expect("rig").hull;
    assert!(after < before - 1.0, "the cold bites: {before} -> {after}");
    // The home world's bands never apply: the classic reading here would be the planet
    // settings' band heat; the body's own says nothing burns at the surface by day.
    world.set_body_millis(life.clock.millis_when(world.body_millis(), column, 500));
    let hull = world.player(ONE).expect("rig").hull;
    for _ in 0..60 {
        let _ = step(&mut world, idle());
    }
    assert!((world.player(ONE).expect("rig").hull - hull).abs() < 0.01);
}

#[test]
fn a_pilot_breathes_only_a_breathable_sky() {
    let rocky = lone(PlanetThemeId::ROCKY, 600, 30);
    let barren = lone(PlanetThemeId::BARREN, 400, 30);
    for (world, breathes) in [(&rocky, true), (&barren, false)] {
        let surface = world.terrain().surface_row(100);
        let (x, y) = world.frame().centre_of(100, surface - 2);
        assert_eq!(world.body_breathable_at(x, y), Some(breathes));
        let air = world.pilot_air_at(x, y);
        assert_eq!(air == PilotAir::Breathable, breathes, "{air:?}");
    }
}

#[test]
fn gravity_scales_with_each_body_and_shows_one_number() {
    let fall = |gravity: u16| {
        let mut world = lone(PlanetThemeId::BARREN, gravity, 60);
        hover(&mut world, 300, 30);
        let start = world.player(ONE).expect("rig").y;
        for _ in 0..18 {
            let _ = step(&mut world, idle());
        }
        let shown = world.body_gravity_of(ONE).expect("on a body");
        (world.player(ONE).expect("rig").y - start, shown)
    };
    let (light, light_shown) = fall(300);
    let (heavy, heavy_shown) = fall(1_200);
    assert_eq!((light_shown, heavy_shown), (300, 1_200));
    let ratio = heavy / light;
    assert!(
        (3.0..=5.0).contains(&ratio),
        "fall ratio {ratio} ({light} vs {heavy})"
    );
    // Below the playable floor the HUD shows the floor the ground plays with.
    let (_, floor) = fall(100);
    assert_eq!(floor, 250);
}

/// Landable bodies of the world seeded `seed` near home: `(key, params)`.
fn landable_bodies(seed: u64) -> Vec<(SpaceBodyKey, PlanetParams, Atmosphere)> {
    let space = crate::space::Space::shared(crate::space::SpaceSeed::from(seed));
    let mut out = Vec::new();
    for y in -2..=2 {
        for x in -2..=2 {
            let sector = space.sector(SpaceSector::new(x, y));
            for system in &sector.systems {
                for body in &system.bodies {
                    let key = SpaceBodyKey {
                        sector: sector.coords,
                        system: system.index,
                        body: body.index,
                    };
                    if let Some(params) =
                        crate::place::planet_params_for(crate::space::SpaceSeed::from(seed), key)
                    {
                        out.push((key, params, body.atmosphere));
                    }
                }
            }
        }
    }
    out
}

fn deep_world() -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(SEED)
    };
    let mut world = WorldState::new_game_with_settings(ONE, &settings, SimulationParams::new(30));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.add_player(TWO);
    world
}

fn land(world: &mut WorldState, player: PlayerId, key: SpaceBodyKey, params: PlanetParams) {
    let ground = PlanetGround::new(params);
    let surface = ground.surface_row(64);
    world
        .move_to_place(
            player,
            Place::OnBody(BodyId::Space(key)),
            WorldPos::tile_center(64, i64::from(surface - 2)),
        )
        .expect("a landable body");
    let rig = world.player_mut(player).expect("rig");
    rig.flight.air = 300.0;
}

#[test]
fn seats_on_different_bodies_read_their_own_day_and_air() {
    let bodies = landable_bodies(SEED);
    let breathing = bodies
        .iter()
        .find(|(_, _, air)| air.breathable())
        .copied()
        .expect("a breathable body near home");
    let airless = bodies
        .iter()
        .find(|(key, _, air)| *air == Atmosphere::Vacuum && *key != breathing.0)
        .copied()
        .expect("an airless body near home");
    let mut world = deep_world();
    land(&mut world, ONE, breathing.0, breathing.1);
    land(&mut world, TWO, airless.0, airless.1);
    for _ in 0..90 {
        let _ = step(&mut world, idle());
    }
    let one = world.body_reading_of(ONE).expect("one is on a body");
    let two = world.body_reading_of(TWO).expect("two is on a body");
    let life_one = world.body_life_of_player(ONE).expect("life");
    let life_two = world.body_life_of_player(TWO).expect("life");
    assert_ne!(
        life_one.clock, life_two.clock,
        "each body keeps its own clock"
    );
    assert_eq!(
        one.local_permille,
        life_one.clock.local_permille(world.body_millis(), 64)
    );
    assert!(one.climate.breathable && !two.climate.breathable);
    // Life support: the breathable world refills, the airless one drains.
    let air_one = world.player(ONE).expect("rig").flight.air;
    let air_two = world.player(TWO).expect("rig").flight.air;
    assert!(air_one > 300.0, "refilled: {air_one}");
    assert!(air_two < 300.0, "drained: {air_two}");
    // The flight approach and the ground agree on the body's pull.
    let space = world.space();
    for (key, params, _) in [breathing, airless] {
        let body = space.body_ref(key).expect("a flight body");
        let flight = space.body(body).expect("found");
        let life =
            crate::world::body_life_of(crate::space::SpaceSeed::from(SEED), Some(key), params)
                .expect("life");
        let ground = f32::from(life.gravity_milli()) / 1_000.0;
        assert!((flight.surface_gravity / crate::flight::space::FLIGHT_G_PX - ground).abs() < 1e-4);
        assert!((params.gravity() - ground).abs() < 1e-4);
    }
}

#[test]
fn a_bodys_market_pays_its_own_prices() {
    let bodies = landable_bodies(SEED);
    let (key, params, _) = bodies[0];
    let mut world = deep_world();
    land(&mut world, ONE, key, params);
    let market = world.body_market(ONE, 0x000A_11E4, 64).expect("a market");
    let expected =
        market.mineral_price(MineralKind::Gold) * 3 + market.mineral_price(MineralKind::Iron) * 5;
    let rig = world.player_mut(ONE).expect("rig");
    rig.cargo_bay.add(MineralKind::Gold, 3);
    rig.cargo_bay.add(MineralKind::Iron, 5);
    let credits = rig.credits;
    let mut output = crate::TickOutput::default();
    assert_eq!(
        world.sell_cargo_at_market(ONE, &market, &mut output),
        crate::PlayerScopedCommandOutcome::Applied
    );
    let rig = world.player(ONE).expect("rig");
    assert_eq!(rig.credits, credits + expected);
    assert_eq!(rig.cargo_used(), 0);
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::BodyLife {
            what: BodyLifeEvent::Sold,
            detail,
            ..
        } if detail == expected
    )));
    // Not the home depot's price: the body's own.
    assert_ne!(
        market.mineral_price(MineralKind::Gold),
        MineralKind::Gold.value()
    );
}

#[test]
fn weather_turns_are_heard_and_replay_bit_for_bit() {
    let run = || {
        let mut world = lone(PlanetThemeId::ICE, 600, 30);
        hover(&mut world, 200, 0);
        let life = world.body_life(StoreKey::Home).expect("life");
        let table = life.hooks().weather_table();
        // Just before a spell whose kind differs from the one before it.
        let spell = (1..400)
            .find(|spell| {
                let kind = |at: u64| {
                    crate::bodylife::weather::spell_kind(life.seed, table, Atmosphere::Thin, at)
                };
                let (before, after) = (kind(spell - 1), kind(*spell));
                before != after && before != WeatherId::AURORA && after != WeatherId::AURORA
            })
            .expect("the weather changes");
        world.set_body_millis(spell * SPELL_MILLIS - 2_000);
        let mut turned = Vec::new();
        for _ in 0..30 * 4 {
            for event in step(&mut world, idle()) {
                if let GameplayEventKind::BodyLife {
                    what: BodyLifeEvent::WeatherTurned,
                    detail,
                    ..
                } = event
                {
                    turned.push(detail);
                }
            }
        }
        (turned, world.simulation_state_hash())
    };
    let (a, hash_a) = run();
    let (b, hash_b) = run();
    assert!(!a.is_empty(), "the turn is heard");
    assert_eq!(a, b);
    assert_eq!(hash_a, hash_b);
    let _ = TILE_SIZE;
}
