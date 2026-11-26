//! One living galaxy, end to end (Wave 12 integration): a bot with the player's controls
//! leaves home, mines a belt rock, docks at Halcyon Dock to sell and to buy a ship line and
//! planet gear, takes a pirate bounty off a board and beats it, lands on each of the six theme
//! worlds (by test-mode warp between systems) and checks its strata, creatures, weather and
//! settlement, meets an alien species, trades, learns words and completes their first
//! chapter, crosses a wormhole and flies home.
//!
//! The legs between systems are test-mode warps (the plan allows them); everything on a leg
//! is the world's own rules with the player's inputs.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "test bookkeeping"
)]

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TestModeAction,
    TickInputs, WorldState,
    alien::{
        AlienCommand, AlienEvent, AlienSpecies,
        atlas::first_home,
        quest::{ChapterGoal, chapters},
        town::{self, towns_of},
    },
    attitude::RigAttitude,
    enemy::{EnemyDifficulty, EnemyKind},
    entity::{EntityId, EntityKind, EntityState},
    field::trig::wrap_angle,
    flight::{
        FlightCommand, FlightPlace, RIG_CONTACT, RigFlight, StationService,
        autopilot::{Leg, SpaceBot},
        physics::up_angle,
        space::{self, BodyKind, FlightBody, GeneratedSpace, SpaceQuery},
        systems::{BodyExtra, SystemsEvent, asteroid_mineral},
    },
    hostiles::{
        CombatCommand, Faction, HostileEvent, ShipGearKind,
        combat::{bearing, length, nose},
    },
    input::WeaponControl,
    place::{BodyId, Place, SpaceBodyKey, StoreKey},
    planet::BodyGround,
    position::WorldPos,
    space::{HOME_SECTOR, SpaceSector, Theme},
    station::{
        MissionKind, ShipLine, ShipUpgrade, SpaceGear, StationAction, StationEvent, StationId,
        bounty::bounty_target_of,
        profile::{profile_of, station_ref},
    },
    world::{strip_point, strip_position},
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x00C0_FFEE;
const HZ: u32 = 30;
const HALCYON: StationId = StationId {
    sector: HOME_SECTOR,
    system: 0,
    index: 0,
};

/// A Deep Descent world in test mode with a rig that beat the Heart (its upgrades, the Star
/// Drive), a modest purse, no space gear yet, and the hostile factions on.
fn world() -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(SEED)
        },
        SimulationParams::new(HZ),
    );
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    world.enable_test_mode();
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    heart.star_drive.insert(PLAYER);
    world.set_heart(heart);
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    // The Descents' own hazard systems (cryo hull, filters...) but none of space's gear yet.
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hazard_gear.ship = crate::station::ShipGear::default();
    rig.hazard_gear.arms = crate::hostiles::ShipGear::default();
    rig.hull = rig.max_hull();
    rig.fuel = rig.fuel_capacity;
    rig.credits = crate::endgame::economy::space_credits(16_000);
    world
}

pub(super) fn tick(
    world: &mut WorldState,
    controls: ControlState,
    command: Option<PlayerCommand>,
) -> Vec<GameplayEventKind> {
    let mut inputs = TickInputs::held(PLAYER, controls);
    if let Some(command) = command {
        inputs = inputs.act(PLAYER, command);
    }
    let tick = world.simulation_tick();
    world
        .simulate_tick(tick, &inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

pub(super) fn run(world: &mut WorldState, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(tick(world, ControlState::default(), None));
    }
    events
}

pub(super) fn act(world: &mut WorldState, command: PlayerCommand) -> Vec<GameplayEventKind> {
    let mut events = tick(world, ControlState::default(), Some(command));
    events.extend(run(world, 1));
    events
}

fn hub(world: &mut WorldState, action: StationAction) -> Vec<(StationEvent, u32)> {
    station_events(&act(
        world,
        PlayerCommand::Flight(FlightCommand::Station(StationService::Hub(action))),
    ))
}

fn station_events(events: &[GameplayEventKind]) -> Vec<(StationEvent, u32)> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Station { what, detail, .. } => Some((*what, *detail)),
            _ => None,
        })
        .collect()
}

fn hostile_events(events: &[GameplayEventKind]) -> Vec<(HostileEvent, u32)> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Hostile { what, detail, .. } => Some((*what, *detail)),
            _ => None,
        })
        .collect()
}

pub(super) fn alien_saw(events: &[GameplayEventKind], what: AlienEvent) -> bool {
    events
        .iter()
        .any(|event| matches!(event, GameplayEventKind::Alien { what: seen, .. } if *seen == what))
}

pub(super) fn seconds(seconds: f32) -> u32 {
    (seconds * HZ as f32).round() as u32
}

/// The rig put straight into space at `pos`, still, turned `orientation` (a test warp).
fn launch_to(world: &mut WorldState, pos: WorldPos, orientation: f32) {
    let park = world.space_park().expect("park");
    let capacity = world.air_capacity_of(PLAYER);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.flight = RigFlight {
        place: FlightPlace::Space,
        pos,
        air: capacity,
        exit_column: 12,
        ..RigFlight::default()
    };
    rig.attitude = RigAttitude {
        orientation,
        angular_velocity: 0.0,
    };
    rig.x = park.0;
    rig.y = park.1;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

/// Docks at `station` the way a player does: in reach of the port, aligned, slow, use.
fn dock_at(world: &mut WorldState, station: StationId) {
    let galaxy = world.space();
    let body = galaxy
        .body(station_ref(&galaxy, station).expect("station ref"))
        .expect("station body");
    let port = body.port().expect("port");
    launch_to(world, port.offset(10.0, 0.0), body.dock_orientation());
    let _ = act(world, PlayerCommand::Flight(FlightCommand::Use));
    assert_eq!(
        world.rig_flight(PLAYER).expect("rig").place,
        FlightPlace::Docked(body.body),
        "docked at {station:?}"
    );
}

/// Sectors round home, nearest rings first.
fn sectors(reach: i32) -> Vec<SpaceSector> {
    let mut out = Vec::new();
    for ring in 0..=reach {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) == ring {
                    out.push(SpaceSector::new(HOME_SECTOR.x + x, HOME_SECTOR.y + y));
                }
            }
        }
    }
    out
}

/// A big belt rock near home.
fn belt_rock(world: &WorldState) -> FlightBody {
    let generated = GeneratedSpace::new(SEED);
    let space = crate::space::Space::shared(SEED);
    for sector in sectors(4) {
        for system in &space.sector(sector).systems {
            for belt in &system.belts {
                let mid = i64::from(belt.inner / 2 + belt.outer / 2);
                let at = system.centre.offset(mid, 0).pos();
                if let Some(rock) = generated
                    .rocks_near(at, 6_000, world.space_systems())
                    .into_iter()
                    .find(|rock| rock.radius >= space::tiles(16.0))
                {
                    return rock;
                }
            }
        }
    }
    panic!("no belt rock near home");
}

/// A landable planet with space theme `theme` near home (`SpaceBodyKey`).
pub(super) fn body_of_theme(theme: Theme) -> SpaceBodyKey {
    let space = crate::space::Space::shared(SEED);
    let mut best: Option<(u32, SpaceBodyKey)> = None;
    for sector in sectors(6) {
        for system in &space.sector(sector).systems {
            for body in &system.bodies {
                if body.theme != theme || body.is_home() {
                    continue;
                }
                let id = SpaceBodyKey {
                    sector,
                    system: system.index,
                    body: body.index,
                };
                if crate::place::planet_params_for(SEED, id).is_none() {
                    continue;
                }
                // The biggest found (a planet's full strata), first found among equals.
                if best.is_none_or(|(radius, _)| body.radius > radius) {
                    best = Some((body.radius, id));
                }
            }
        }
        if best.is_some_and(|(radius, _)| radius >= 1_500) {
            break;
        }
    }
    best.map_or_else(|| panic!("no {theme:?} body near home"), |(_, id)| id)
}

/// A bounty against pirates on a board near home: (station, slot, target id).
fn pirate_bounty(world: &WorldState) -> (StationId, u8, crate::hostiles::BountyTarget) {
    let space = crate::space::Space::shared(SEED);
    for sector in sectors(2) {
        for system in &space.sector(sector).systems {
            for station in &system.stations {
                let profile = profile_of(SEED, system, station);
                if !profile.bounties {
                    continue;
                }
                for (slot, offer) in world.station_board(profile.id).iter().enumerate() {
                    let Some(offer) = offer else {
                        continue;
                    };
                    if let Some(target) = bounty_target_of(SEED, offer.kind)
                        && target.faction == Faction::Pirates
                    {
                        return (profile.id, u8::try_from(slot).unwrap_or(0), target);
                    }
                }
            }
        }
    }
    panic!("no pirate bounty on any board near home");
}

/// The player's controls against the nearest hostile ship: hold a firing range, keep the nose
/// on it (rotation thrusters only: nothing turns the rig for the player) and fire when lined
/// up; missiles now and then.
fn fight_bot(world: &WorldState, tick: u32) -> (ControlState, Option<PlayerCommand>) {
    let rig = world.player(PLAYER).expect("rig");
    let pos = rig.flight.pos;
    let Some(target) = world
        .hostiles()
        .ships
        .iter()
        .min_by(|a, b| pos.distance(a.pos).total_cmp(&pos.distance(b.pos)))
    else {
        return (ControlState::default(), None);
    };
    let (dx, dy) = pos.delta(target.pos);
    let distance = length(dx, dy).max(1.0);
    let (ux, uy) = (dx / distance, dy / distance);
    let radial = ((distance - 480.0) * 1.5).clamp(-450.0, 450.0);
    let (wx, wy) = (target.vx + ux * radial, target.vy + uy * radial);
    let (ax, ay) = (wx - rig.flight.velocity_x, wy - rig.flight.velocity_y);
    let angle = rig.attitude.orientation;
    let (nx, ny) = nose(angle);
    let (rx, ry) = (-ny, nx);
    let forward = ax * nx + ay * ny;
    let side = ax * rx + ay * ry;
    let error = wrap_angle(bearing(dx, dy) - angle);
    let rotate = (error * 4.0 - rig.attitude.angular_velocity * 1.2).clamp(-1.0, 1.0);
    let fire = error.abs() < 0.1 && distance < 900.0;
    let controls = ControlState::new(
        (side / 250.0).clamp(-1.0, 1.0),
        forward > 60.0,
        forward < -60.0,
    )
    .with_rotation(rotate)
    .with_weapon(WeaponControl {
        fire,
        ..WeaponControl::default()
    });
    let missile = (fire && tick.is_multiple_of(60) && world.rig_combat(PLAYER).missiles > 0)
        .then_some(PlayerCommand::Combat(CombatCommand::FireMissile));
    (controls, missile)
}

/// Puts the rig on the body it is on at strip `column`, `rows_up` rows over row `row` (a test
/// warp within the body).
pub(super) fn put_rig(
    world: &mut WorldState,
    id: SpaceBodyKey,
    column: f64,
    row: i32,
    rows_up: f64,
) {
    world
        .move_to_place(
            PLAYER,
            Place::OnBody(BodyId::Space(id)),
            strip_position(column + 0.5, f64::from(row) - rows_up),
        )
        .expect("on the body");
    let landed = world.space().body_ref(id).map(FlightPlace::Landed);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.flight.air = 1.0e7;
        if let Some(landed) = landed {
            rig.flight.place = landed;
        }
    }
}

/// The NPC entity of resident `slot` of the town, once the rig stands beside them.
pub(super) fn beside(
    world: &mut WorldState,
    id: SpaceBodyKey,
    found: &town::AlienTown,
    slot: usize,
) -> EntityId {
    let character = found.population.characters[slot].id;
    for _ in 0..4_000 {
        let npc = world
            .store_entities(StoreKey::Body(id))
            .and_then(|entities| {
                entities
                    .of_kind(EntityKind::Npc)
                    .find(|entity| {
                        matches!(entity.state, EntityState::Npc(state) if state.character == character)
                    })
                    .map(|entity| entity.id)
            });
        if let Some(npc) = npc
            && let Some((entity, state)) = world.npc(npc)
            && state.inside.is_none()
        {
            let frame = world.store_frame(StoreKey::Body(id)).expect("frame");
            let x = state.x_at(world.simulation_tick(), 0.0);
            let (u, _) = strip_point(frame.world(x, entity.body.y));
            put_rig(world, id, u.floor(), found.ground_row(), 1.0);
            run(world, 3);
            if world.npcs_in_reach(PLAYER).contains(&npc) {
                return npc;
            }
        }
        run(world, 30);
    }
    panic!(
        "{} never came out",
        found.population.characters[slot].name.full()
    );
}

/// Checks a landed theme world: its strata, its storm in the body's forecast, its creatures
/// round the rig, its level settlement site. Returns the native creature met.
#[allow(clippy::too_many_lines, reason = "one world's checks in turn")]
pub(super) fn tour_theme(world: &mut WorldState, theme: Theme) -> EnemyKind {
    let id = body_of_theme(theme);
    let place = Place::OnBody(BodyId::Space(id));
    let _ = act(
        world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(place)),
    );
    let _ = run(world, 20);
    assert_eq!(world.place_of(PLAYER), place, "{theme:?}: landed");
    let store = StoreKey::Body(id);
    let ground = *world
        .store_terrain(store)
        .and_then(|terrain| terrain.planet_ground())
        .expect("round ground");
    let hooks = ground.hooks();
    assert_eq!(
        hooks.id(),
        crate::place::planet_theme_of(theme),
        "{theme:?}"
    );
    // Its strata: the theme's own, down to a core 800 rows or more under a planet's surface.
    let names: Vec<&str> = hooks.strata().iter().map(|stratum| stratum.name).collect();
    let (site, width) = hooks.settlement_site(&ground).expect("a settlement site");
    let surface = BodyGround::surface_row(&ground, site);
    let depth = ground.core_chamber_top() - surface;
    if ground.params().radius >= 1_500 {
        assert!(depth >= 800, "{theme:?}: {depth} rows of strata");
    }
    let mut met_strata = Vec::new();
    for share in [10, 45, 85] {
        let row = surface + depth * share / 100;
        let name = ground.layer_name(site, row);
        assert!(
            names.contains(&name),
            "{theme:?}: {name} is one of {names:?}"
        );
        if !met_strata.contains(&name) {
            met_strata.push(name);
        }
    }
    assert!(met_strata.len() >= 2, "{theme:?}: strata {met_strata:?}");
    // Its settlement site is level (the town and the theme's clearing, island or rim).
    let rows: Vec<i32> = (site - width / 4..=site + width / 4)
        .map(|column| BodyGround::surface_row(&ground, column))
        .collect();
    let spread = rows.iter().max().unwrap_or(&0) - rows.iter().min().unwrap_or(&0);
    assert!(spread <= 1, "{theme:?}: settlement spread {spread}");
    // Its weather: a storm spell of the body's own forecast is the theme's storm, and the
    // theme's own rules see it blowing too.
    let life = world.body_life(store).expect("the body's life");
    let table = hooks.weather_table();
    let theme_id = ground.params().theme;
    let spell = crate::bodylife::weather::SPELL_MILLIS;
    let forecast = |millis: u64, night: bool| {
        crate::bodylife::weather::forecast(life.seed, table, life.climate.atmosphere, millis, night)
            .kind
    };
    // The theme's storm: an exotic world's own kind (the locked world's first is its
    // night-only snow, never in a day-side forecast); else the first of its table that blows
    // on this body by day or by night (an airless dead world never feels its grey wind, only
    // its ghost lights after dark).
    let exotic = crate::exotic::style_of(theme_id);
    let (storm_kind, millis) = (1..2_000)
        .map(|index| index * spell + spell / 2)
        .find_map(|millis| {
            let kind = forecast(millis, false);
            let kind = if exotic.is_none() && kind == crate::bodylife::WeatherId::CLEAR {
                forecast(millis, true)
            } else {
                kind
            };
            let wanted = crate::exotic::style_of(theme_id).map_or_else(
                || {
                    kind != crate::bodylife::WeatherId::CLEAR
                        && table.iter().any(|odds| odds.kind == kind)
                },
                |style| kind == crate::exotic::hooks::storm_kind(style),
            );
            wanted.then_some((kind, millis))
        })
        .expect("a storm spell");
    let params = *ground.params();
    if let Some(style) = crate::frontier::style_of(params.theme) {
        assert_eq!(
            crate::frontier::rules::weather_at(style, params.seed, millis).phase,
            crate::frontier::rules::WeatherPhase::Storm,
            "{theme:?}: the frontier storm is the forecast's"
        );
    } else if crate::planet::outlands::life::outland_theme(params.theme).is_some() {
        // The outland worlds' storms are bodylife kinds with their own effects (Wave 13).
        assert!(
            (1_000..=1_019).contains(&storm_kind.0),
            "{theme:?}: the outland storm is the forecast's ({storm_kind:?})"
        );
    } else if let Some(style) = crate::exotic::style_of(params.theme) {
        assert!(
            crate::exotic::rules::storm_strength(style, life.seed, millis) > 0.0,
            "{theme:?}: the exotic storm is the forecast's"
        );
    } else {
        let weather =
            crate::planet::wilds::life::weather_at(&params, millis).expect("a wild theme");
        assert!(
            weather.intensity > 0,
            "{theme:?}: the wild storm is the forecast's"
        );
    }
    // Its creatures: the theme's own come round a rig out in the open (away from the town).
    let natives: Vec<u16> = hooks
        .creature_spawns()
        .iter()
        .map(|spawn| spawn.creature)
        .collect();
    assert!(!natives.is_empty(), "{theme:?}: a roster");
    let strip = ground.strip_width();
    let mut met = None;
    for (attempt, offset) in [strip / 4, strip / 2, strip * 3 / 4, strip / 8]
        .into_iter()
        .enumerate()
    {
        let column = (site + offset).rem_euclid(strip);
        let row = BodyGround::surface_row(&ground, column);
        put_rig(world, id, f64::from(column), row, 2.0);
        // Dig a little to stir the underground hunters too.
        for _ in 0..seconds(40.0) {
            let mut controls = ControlState::default();
            controls.drill_down = attempt % 2 == 1;
            let _ = tick(world, controls, None);
            met = world.store_entities(store).and_then(|entities| {
                entities
                    .of_kind(EntityKind::Enemy)
                    .find_map(|entity| match entity.state {
                        EntityState::Enemy(state) if natives.contains(&(state.kind as u16)) => {
                            Some(state.kind)
                        }
                        _ => None,
                    })
            });
            if met.is_some() {
                break;
            }
        }
        if met.is_some() {
            break;
        }
        let enemies: Vec<EnemyKind> = world
            .store_entities(store)
            .map(|entities| {
                entities
                    .of_kind(EntityKind::Enemy)
                    .filter_map(|entity| match entity.state {
                        EntityState::Enemy(state) => Some(state.kind),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let rig = world.player(PLAYER).expect("rig");
        eprintln!(
            "{theme:?} attempt {attempt}: enemies {enemies:?}, rig hull {:.0} at ({:.0},{:.0}) in {:?}, tick {}",
            rig.hull,
            rig.x,
            rig.y,
            world.place_of(PLAYER),
            world.simulation_tick().get()
        );
    }
    let met = met.unwrap_or_else(|| panic!("{theme:?}: no native creature came"));
    // Nothing of home's roster came to a theme world's ground.
    let strangers = world.store_entities(store).map_or(0, |entities| {
        entities
            .of_kind(EntityKind::Enemy)
            .filter(|entity| {
                matches!(entity.state, EntityState::Enemy(state)
                    if !natives.contains(&(state.kind as u16))
                        && !crate::hostiles::raid::is_trooper(state.kind))
            })
            .count()
    });
    assert_eq!(strangers, 0, "{theme:?}: only its own creatures");
    met
}

#[test]
fn a_bot_tours_the_whole_galaxy_and_comes_home() {
    tour();
}

#[allow(clippy::too_many_lines, reason = "the whole journey, leg by leg")]
fn tour() {
    let mut world = world();
    // 1. Leave home over the Moon with the player's controls (from just under the space line
    // over the Moon, where a climb from the surface comes out).
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
    let mut bot = SpaceBot::new([Leg::Leave]);
    for _ in 0..seconds(240.0) {
        if bot.done() {
            break;
        }
        let (controls, command) = bot.next(&world, PLAYER);
        let events = tick(&mut world, controls, command);
        bot.observe(&events);
    }
    assert!(bot.done(), "left home");
    assert!(world.rig_flight(PLAYER).expect("rig").away());
    // 2. Mine a belt rock: the drill rests on it.
    let rock = belt_rock(&world);
    let BodyExtra::Rock(facts) = rock.extra else {
        unreachable!()
    };
    let up = up_angle(0.0, -1.0);
    launch_to(
        &mut world,
        rock.centre.offset(0.0, -(rock.radius + RIG_CONTACT + 1.0)),
        up,
    );
    let mut mined = 0;
    for _ in 0..seconds(6.0) {
        mined += tick(&mut world, ControlState::default(), None)
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    GameplayEventKind::SpaceSystems {
                        what: SystemsEvent::OreMined,
                        ..
                    }
                )
            })
            .count();
    }
    assert!(mined >= 2, "the drill bit the rock ({mined})");
    let ore = asteroid_mineral(facts.ore);
    assert!(world.player(PLAYER).expect("rig").cargo_bay.mineral(ore) >= 2);
    // 3. Dock at Halcyon: sell, fit a ship line and planet gear, arm the ship.
    dock_at(&mut world, HALCYON);
    let credits = world.player(PLAYER).expect("rig").credits;
    let sold = hub(&mut world, StationAction::SellOre);
    assert!(
        sold.iter().any(|(what, _)| *what == StationEvent::OreSold),
        "{sold:?}"
    );
    assert!(world.player(PLAYER).expect("rig").credits > credits);
    let fitted = hub(
        &mut world,
        StationAction::Fit(ShipUpgrade::Line(ShipLine::Engine)),
    );
    assert!(
        fitted
            .iter()
            .any(|(what, _)| *what == StationEvent::ShipUpgraded),
        "{fitted:?}"
    );
    let gear = hub(
        &mut world,
        StationAction::Fit(ShipUpgrade::Gear(SpaceGear::HeatShielding)),
    );
    assert!(
        gear.iter()
            .any(|(what, _)| *what == StationEvent::GearFitted),
        "{gear:?}"
    );
    for kind in [ShipGearKind::Cannon, ShipGearKind::Shield] {
        let _ = act(
            &mut world,
            PlayerCommand::Combat(CombatCommand::BuyGear(kind)),
        );
    }
    assert!(world.player(PLAYER).expect("rig").hazard_gear.arms.cannon > 0);
    // 4. Take a pirate bounty off a board near home.
    let (station, slot, target) = pirate_bounty(&world);
    dock_at(&mut world, station);
    let taken = hub(&mut world, StationAction::Accept(slot));
    assert!(
        taken
            .iter()
            .any(|(what, _)| *what == StationEvent::MissionAccepted),
        "{taken:?}"
    );
    assert_eq!(
        world.bounty_status(target.id),
        crate::hostiles::BountyStatus::Posted
    );
    assert!(
        world
            .station_ledger()
            .missions_of(PLAYER)
            .iter()
            .any(|mission| matches!(mission.kind, MissionKind::Bounty { .. }))
    );
    // 5. Fly to it, fight it and beat it with the player's controls.
    world.player_mut(PLAYER).expect("rig").hull = world.player(PLAYER).expect("rig").max_hull();
    launch_to(&mut world, target.pos.offset(900.0, 0.0), 0.0);
    let credits = world.player(PLAYER).expect("rig").credits;
    let mut claimed = false;
    let mut paid = false;
    for at in 0..seconds(300.0) {
        let (controls, command) = fight_bot(&world, at);
        let events = tick(&mut world, controls, command);
        claimed |= hostile_events(&events)
            .iter()
            .any(|(what, _)| *what == HostileEvent::BountyClaimed);
        paid |= station_events(&events)
            .iter()
            .any(|(what, _)| *what == StationEvent::MissionPaid);
        if claimed && paid {
            break;
        }
    }
    assert!(claimed, "the pirate bounty is claimed");
    assert!(paid, "the board's mission closes");
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        credits + target.reward,
        "the bounty paid"
    );
    assert!(
        !world
            .station_ledger()
            .missions_of(PLAYER)
            .iter()
            .any(|mission| matches!(mission.kind, MissionKind::Bounty { .. }))
    );
    // 6. Land on each of the six theme worlds and see them alive.
    let mut met = Vec::new();
    for theme in [
        Theme::Jungle,
        Theme::OceanArchipelago,
        Theme::Ice,
        Theme::DesertDunes,
        Theme::Volcanic,
        Theme::Crystal,
    ] {
        met.push((theme, tour_theme(&mut world, theme)));
    }
    eprintln!("natives met: {met:?}");
    // 7. Meet an alien species, trade, learn, and complete their first chapter.
    let species = AlienSpecies::Tokkari;
    let id = first_home(SEED, species).expect("a first home");
    let params = crate::place::planet_params_for(SEED, id).expect("landable");
    let towns = towns_of(&params);
    let found = std::sync::Arc::clone(&towns[0]);
    let events = act(
        &mut world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::OnBody(BodyId::Space(
            id,
        )))),
    );
    let mut events = events;
    events.extend(run(&mut world, 30));
    // First contact: here, or already on one of their worlds the tour landed on.
    assert!(
        alien_saw(&events, AlienEvent::FirstContact) || world.aliens().log(PLAYER, species).met(),
        "first contact"
    );
    assert!(world.alien_town_at(PLAYER).is_some(), "in their town");
    let before = world.understanding(PLAYER, species);
    let teacher = beside(&mut world, id, &found, town::TEACHER);
    let lesson = act(
        &mut world,
        PlayerCommand::Alien(AlienCommand::Lesson { npc: teacher }),
    );
    assert!(alien_saw(&lesson, AlienEvent::Lesson));
    assert!(
        world.understanding(PLAYER, species) > before,
        "words learned"
    );
    let (prized, _) = species.def().valued_ores[0];
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(prized, 4);
    let trader = beside(&mut world, id, &found, town::TRADER);
    let sale = act(
        &mut world,
        PlayerCommand::Alien(AlienCommand::Sell {
            npc: trader,
            mineral: prized,
            count: 4,
        }),
    );
    assert!(alien_saw(&sale, AlienEvent::Sold), "a trade");
    let chapter = chapters(species)[0];
    let elder = beside(&mut world, id, &found, town::ELDER);
    let offered = act(
        &mut world,
        PlayerCommand::Alien(AlienCommand::AskChapter { npc: elder }),
    );
    assert!(alien_saw(&offered, AlienEvent::ChapterOffered));
    let accepted = act(
        &mut world,
        PlayerCommand::Alien(AlienCommand::AcceptChapter { npc: elder }),
    );
    assert!(alien_saw(&accepted, AlienEvent::ChapterAccepted));
    let ChapterGoal::Deliver { mineral, count } = chapter.goal else {
        panic!("a first chapter fetches: {:?}", chapter.goal);
    };
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(mineral, count);
    let elder = beside(&mut world, id, &found, town::ELDER);
    let done = act(
        &mut world,
        PlayerCommand::Alien(AlienCommand::TurnInChapter { npc: elder }),
    );
    assert!(
        alien_saw(&done, AlienEvent::ChapterCompleted),
        "a chapter done"
    );
    // 8. Through a wormhole.
    let generated = GeneratedSpace::new(SEED);
    let mouth = sectors(12)
        .into_iter()
        .find_map(|sector| {
            let centre = crate::space::SpaceTile {
                x: sector.origin().x + crate::space::SPACE_SECTOR_TILES / 2,
                y: sector.origin().y + crate::space::SPACE_SECTOR_TILES / 2,
            };
            let pos = centre.pos();
            generated.bodies_near(pos).into_iter().find(|body| {
                body.kind == BodyKind::Wormhole
                    && space::galaxy_sector_of(body.centre) == space::galaxy_sector_of(pos)
            })
        })
        .expect("a wormhole within twelve sectors");
    let start = mouth.centre.offset(mouth.radius + space::tiles(40.0), 0.0);
    // Back up into free flight from the alien world (a test warp), then over to the mouth.
    let _ = act(
        &mut world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::Space)),
    );
    let _ = run(&mut world, 2);
    launch_to(&mut world, start, up_angle(-1.0, 0.0));
    world.player_mut(PLAYER).expect("rig").flight.velocity_x = -1_000.0;
    let mut through = false;
    for _ in 0..seconds(8.0) {
        through |= tick(&mut world, ControlState::default(), None)
            .iter()
            .any(|event| {
                matches!(
                    event,
                    GameplayEventKind::SpaceSystems {
                        what: SystemsEvent::WormholeExited,
                        ..
                    }
                )
            });
        if through {
            break;
        }
    }
    assert!(through, "out of the far mouth");
    // 9. Home: into the home world's approach and down under the sky.
    let galaxy = world.space();
    let home = galaxy
        .bodies_of(&galaxy.system(space::HOME_SYSTEM).expect("home system"))
        .into_iter()
        .find(|body| body.kind == BodyKind::Home)
        .expect("the home world");
    launch_to(
        &mut world,
        home.centre
            .offset(0.0, -(home.radius + home.approach * 0.5)),
        up_angle(0.0, -1.0),
    );
    let mut bot = SpaceBot::new([Leg::Home]);
    for _ in 0..seconds(240.0) {
        if bot.done() {
            break;
        }
        let (controls, command) = bot.next(&world, PLAYER);
        let events = tick(&mut world, controls, command);
        bot.observe(&events);
    }
    assert!(bot.done(), "flew home");
    let _ = run(&mut world, 30);
    assert_eq!(world.place_of(PLAYER), Place::HOME, "home again");
    assert!(!world.rig_flight(PLAYER).expect("rig").away());
}
