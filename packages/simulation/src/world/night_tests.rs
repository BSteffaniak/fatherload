//! Night-time gameplay (WP-NIGHT): monsters rising by the clock and burrowing at dawn, safety by
//! light, palisades, homes, camps and settlements, light that matters to monsters, the Moon,
//! the night market, nocturnal ore, Peaceful, determinism, saves and replication.

#![allow(
    clippy::suboptimal_flops,
    reason = "test geometry reads clearer unfused"
)]

use std::collections::BTreeMap;

use serde::{Serialize, de::DeserializeOwned};

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, PlayerScopedCommandOutcome,
    SectionKey, SessionServiceRequest, SimulationParams, TickInputs, TickOutput, WorldState,
    base::{BuildCommand, Buildable, BuiltTile, Facing, Fixture, ModuleKind},
    clock::{DayLength, MoonPhase, WorldClock},
    enemy::{EnemyDifficulty, EnemyKind, EnemyMode},
    entity::{EntityDelta, EntityId, EntityKind, EntityState},
    night::{self, NIGHT_SAFE_LIGHT},
    terrain::{MineralKind, StrategicResourceKind, TILE_SIZE, TileKind, TilePosition},
    village::{self, BuildingKind, VillageDeal, VillageService, placement, services::MarketTime},
    worldgen::{WorldSettings, biome::Biome},
};

const PLAYER: PlayerId = PlayerId::new(1);
const OTHER: PlayerId = PlayerId::new(2);
const SEED: u64 = 0x416E_7465;

fn wire<T: Serialize + DeserializeOwned>(value: &T) -> T {
    let bytes = postcard::to_stdvec(value).expect("encode");
    postcard::from_bytes(&bytes).expect("decode")
}

fn world() -> WorldState {
    WorldState::new_game_with_settings(PLAYER, &WorldSettings::new(SEED), SimulationParams::new(60))
}

/// World time of slice `slice` on the first day from day 1 whose Moon is `moon`.
fn time_of(world: &WorldState, slice: u32, moon: Option<MoonPhase>) -> u64 {
    let length = world.day_length();
    let seed = world.terrain().seed();
    let day = (0..8)
        .find(|day| {
            let clock = WorldClock::from_world_millis(day * length.millis(), length, seed);
            moon.is_none_or(|wanted| clock.moon_phase() == wanted)
        })
        .unwrap_or(0);
    day * length.millis() + u64::from(slice) * length.slice_millis()
}

/// 21:00 (slice 84) on a night whose Moon is a first quarter (ordinary numbers, no rare spawn).
fn set_night(world: &mut WorldState) {
    let time = time_of(world, 84, Some(MoonPhase::FirstQuarter));
    world.set_world_time_millis(time);
}

fn set_dawn(world: &mut WorldState) {
    let now = world.clock().world_millis();
    let length = world.day_length();
    let into = now % length.millis();
    let dawn = u64::from(crate::clock::DAWN_START_SLICE) * length.slice_millis() + 1;
    let next = if into < dawn {
        now - into + dawn
    } else {
        now - into + length.millis() + dawn
    };
    world.set_world_time_millis(next + 2 * length.slice_millis());
}

fn ground(world: &WorldState, column: i32) -> i32 {
    world.terrain().surface_row(column)
}

/// A column east of town on open, exposed ground (not desert, flat for a few tiles either side).
fn exposed_column(world: &WorldState) -> i32 {
    (300..6_000)
        .find(|column| {
            let row = ground(world, *column);
            let biome = crate::worldgen::biome::biome_at(world.world_settings(), *column);
            biome != Biome::Desert
                && biome != Biome::Mountains
                && (column - 40..=column + 40).all(|x| {
                    !world.night_safe_zone(TilePosition {
                        x,
                        y: ground(world, x) - 1,
                    })
                })
                && (column - 3..=column + 3).all(|x| ground(world, x) == row)
        })
        .expect("an exposed stretch of surface")
}

/// Parks `player`'s rig on the ground at `column`.
fn park(world: &mut WorldState, player: PlayerId, column: i32) {
    let row = ground(world, column);
    let rig = world.player_mut(player).expect("rig");
    #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
    {
        rig.x = (column as f32 + 0.5) * TILE_SIZE;
        rig.y = (row as f32 - 0.5) * TILE_SIZE;
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    world.stream_terrain_near_players();
}

fn step(world: &mut WorldState, inputs: &TickInputs) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, inputs)
}

fn idle() -> TickInputs {
    TickInputs::held(PLAYER, ControlState::new(0.0, false, false))
}

fn run(world: &mut WorldState, ticks: u32) -> Vec<GameplayEventKind> {
    (0..ticks)
        .flat_map(|_| step(world, &idle()).events)
        .map(|event| event.kind)
        .collect()
}

fn night_count(world: &WorldState) -> usize {
    world.night_monsters().count()
}

fn act(world: &mut WorldState, player: PlayerId, command: PlayerCommand) -> TickOutput {
    let inputs = idle().act(player, command);
    step(world, &inputs)
}

fn build(world: &mut WorldState, item: Buildable, at: TilePosition) {
    let output = act(
        world,
        PLAYER,
        PlayerCommand::Build(BuildCommand::Place {
            item,
            at,
            facing: Facing::Right,
        }),
    );
    assert_eq!(
        output.action_outcomes.first().expect("outcome").outcome,
        PlayerScopedCommandOutcome::Applied,
        "{item:?} at {at:?}"
    );
}

#[test]
fn night_monsters_rise_after_dusk_and_burrow_at_dawn() {
    let mut world = world();
    let column = exposed_column(&world);
    park(&mut world, PLAYER, column);
    // By day nothing rises.
    let events = run(&mut world, 60 * 20);
    assert_eq!(night_count(&world), 0, "nothing by day");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::NightFell { .. }))
    );
    set_night(&mut world);
    let events = run(&mut world, 60 * 40);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::NightFell { .. })),
        "night falls"
    );
    let risen = night_count(&world);
    assert!(risen > 0, "night monsters rise round an exposed rig");
    assert!(world.night_ledger().active_night.is_some());
    // Every one is a creature of this biome's night.
    let biome = crate::worldgen::biome::biome_at(world.world_settings(), column);
    let roster = night::night_roster(biome);
    let mut wrong = Vec::new();
    for (_, state) in world.night_monsters() {
        let at_home = roster.iter().any(|(kind, _)| *kind == state.kind);
        if !at_home {
            wrong.push(state.kind);
        }
    }
    // Monsters rising 30 columns off may belong to the neighbouring biome's roster.
    assert!(wrong.len() < risen, "mostly {biome:?} monsters: {wrong:?}");
    // Dawn: they burrow and flee, and leave.
    set_dawn(&mut world);
    let events = run(&mut world, 60 * 5);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::NightSurvived { .. })),
        "dawn relief"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::NightMonsterBurrowed { .. })),
        "they burrow"
    );
    assert_eq!(night_count(&world), 0, "gone by morning");
    assert_eq!(world.night_ledger().nights_survived, 1);
    assert!(world.night_ledger().active_night.is_none());
}

#[test]
fn nothing_rises_underground_in_safety_or_on_peaceful() {
    // In town at night: safe.
    let mut town = world();
    set_night(&mut town);
    let _ = run(&mut town, 60 * 30);
    assert_eq!(night_count(&town), 0, "the town is safe at night");
    // Down a shaft at night: tunnels are not theirs.
    let mut deep = world();
    let column = exposed_column(&deep);
    park(&mut deep, PLAYER, column);
    let row = ground(&deep, column) + 20;
    for y in row - 1..=row {
        let _ = deep
            .terrain_mut()
            .set_kind(TilePosition { x: column, y }, TileKind::Air);
    }
    let rig = deep.player_mut(PLAYER).expect("rig");
    #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
    {
        rig.y = (row as f32 + 0.5) * TILE_SIZE;
    }
    assert!(deep.night_safe_zone(TilePosition { x: column, y: row }));
    set_night(&mut deep);
    let _ = run(&mut deep, 60 * 20);
    assert_eq!(night_count(&deep), 0, "nothing hunts down a shaft");
    // Peaceful: none, and turning it on clears the night.
    let mut calm = world();
    let column = exposed_column(&calm);
    park(&mut calm, PLAYER, column);
    set_night(&mut calm);
    let _ = run(&mut calm, 60 * 30);
    assert!(night_count(&calm) > 0);
    calm.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let events = run(&mut calm, 60 * 20);
    assert_eq!(night_count(&calm), 0, "peaceful sweeps the night clean");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::NightFell { .. }))
    );
}

#[test]
fn safety_follows_light_palisades_homes_camps_and_settlements() {
    let mut world = world();
    let column = exposed_column(&world);
    park(&mut world, PLAYER, column);
    world.player_mut(PLAYER).expect("rig").credits = 100_000;
    let row = ground(&world, column) - 1;
    let spot = |dx: i32| TilePosition {
        x: column + dx,
        y: row,
    };
    assert!(!world.night_safe_zone(spot(0)), "open ground is dangerous");
    // A claim alone is not enough at night ...
    build(&mut world, Buildable::Module(ModuleKind::Beacon), spot(0));
    assert!(
        !world.night_safe_zone(spot(4)),
        "a bare claim is not safe at night"
    );
    assert!(
        world.enemy_safe_zone(spot(4)),
        "(though deep enemies keep off it)"
    );
    // ... a lamp's pool is.
    build(&mut world, Buildable::Fixture(Fixture::Light), spot(4));
    assert!(world.fixture_light_at(column + 4, row) >= NIGHT_SAFE_LIGHT);
    assert!(world.night_safe_zone(spot(5)), "in the lamp's pool");
    assert!(world.night_safe_zone(spot(7)), "at its edge");
    assert!(!world.night_safe_zone(spot(-6)), "far from the lamp");
    // Palisades on both sides fence the claim between them.
    assert!(!world.palisade_guarded(spot(-4)));
    park(&mut world, PLAYER, column - 6);
    for dy in 0..2 {
        build(
            &mut world,
            Buildable::Tile(BuiltTile::Palisade),
            TilePosition {
                x: column - 9,
                y: row - dy,
            },
        );
    }
    assert!(!world.palisade_guarded(spot(-4)), "one side is not a fence");
    park(&mut world, PLAYER, column + 6);
    for dy in 0..2 {
        build(
            &mut world,
            Buildable::Tile(BuiltTile::Palisade),
            TilePosition {
                x: column + 9,
                y: row - dy,
            },
        );
    }
    assert!(world.palisade_guarded(spot(-4)));
    assert!(world.night_safe_zone(spot(-4)), "fenced in");
    assert!(!world.night_safe_zone(spot(-12)), "outside the fence");
    // A home (a claim with a bed) is safe through and through.
    let mut home = self::world();
    park(&mut home, PLAYER, column);
    home.player_mut(PLAYER).expect("rig").credits = 100_000;
    build(&mut home, Buildable::Module(ModuleKind::Beacon), spot(0));
    build(&mut home, Buildable::Module(ModuleKind::Bed), spot(3));
    assert!(home.night_safe_zone(spot(-8)), "anywhere in a home's claim");
    // The town, villages and wanderers' camps.
    assert!(world.night_safe_zone(TilePosition {
        x: 115,
        y: ground(&world, 115) - 1
    }));
    let settings = *world.world_settings();
    let found = village::village(&settings, placement::surface_id(1)).expect("village");
    let middle = i32::midpoint(found.first_column, found.last_column);
    assert!(world.night_safe_zone(TilePosition {
        x: middle,
        y: found.ground_row - 1
    }));
    let camp = crate::people::camps::camps_between(&settings, -4_000, 4_000)
        .into_iter()
        .next()
        .expect("a camp near town");
    let fire = TilePosition {
        x: camp.column,
        y: camp.ground_row - 1,
    };
    assert!(world.night_safe_zone(fire), "camps are safe");
    assert!(
        world.fixture_light_at(fire.x, fire.y) > 0,
        "campfires give light"
    );
}

#[test]
fn a_hound_strikes_on_the_open_ground_but_not_under_a_lamp() {
    let mut world = world();
    let column = exposed_column(&world);
    park(&mut world, PLAYER, column);
    set_night(&mut world);
    let row = ground(&world, column);
    #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
    let (x, y) = (
        (column as f32 + 3.5) * TILE_SIZE,
        row as f32 * TILE_SIZE - 9.0,
    );
    let _hound = world.spawn_enemy(EnemyKind::ShadowHound, x, y);
    let events = run(&mut world, 60 * 6);
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::RigStruck {
                kind: EnemyKind::ShadowHound,
                ..
            }
        )),
        "the hound pounces on the open ground"
    );
    // Under a lamp it holds off.
    let mut lit = self::world();
    park(&mut lit, PLAYER, column);
    lit.player_mut(PLAYER).expect("rig").credits = 100_000;
    build(
        &mut lit,
        Buildable::Module(ModuleKind::Beacon),
        TilePosition {
            x: column - 2,
            y: row - 1,
        },
    );
    build(
        &mut lit,
        Buildable::Fixture(Fixture::Light),
        TilePosition {
            x: column,
            y: row - 2,
        },
    );
    set_night(&mut lit);
    let _hound = lit.spawn_enemy(EnemyKind::ShadowHound, x + 3.0 * TILE_SIZE, y);
    let events = run(&mut lit, 60 * 6);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::RigStruck { .. })),
        "nothing strikes in the lamp's light"
    );
}

#[test]
fn light_draws_moths_scares_hounds_and_lights_pilots() {
    let mut world = world();
    let column = exposed_column(&world);
    park(&mut world, PLAYER, column);
    world.player_mut(PLAYER).expect("rig").credits = 100_000;
    set_night(&mut world);
    let row = ground(&world, column);
    // A lamp out past the claim's edge... (lamps need a claim: one beacon far off the rig)
    build(
        &mut world,
        Buildable::Module(ModuleKind::Beacon),
        TilePosition {
            x: column,
            y: row - 1,
        },
    );
    build(
        &mut world,
        Buildable::Fixture(Fixture::Light),
        TilePosition {
            x: column + 6,
            y: row - 3,
        },
    );
    // ... and the rig parked well away from it.
    park(&mut world, PLAYER, column - 30);
    #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
    let lamp = (
        (column as f32 + 6.5) * TILE_SIZE,
        (row as f32 - 2.5) * TILE_SIZE,
    );
    let moths = world.spawn_enemy(EnemyKind::MothSwarm, lamp.0 + 40.0, lamp.1 - 30.0);
    let _ = run(&mut world, 60 * 5);
    let entity = world.entities().get(moths).expect("moths");
    let distance = (entity.body.x - lamp.0).hypot(entity.body.y - lamp.1);
    assert!(
        distance < TILE_SIZE * 5.0,
        "the swarm circles the lamp ({distance})"
    );
    // A pilot's helmet lamp lights the dark.
    let mut dark = self::world();
    let column = exposed_column(&dark);
    park(&mut dark, PLAYER, column);
    set_night(&mut dark);
    let row = ground(&dark, column);
    let far = column + 15;
    let before = dark.light_at(far, row - 2);
    #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
    let placed = dark.place_pilot(
        PLAYER,
        (far as f32 + 0.5) * TILE_SIZE,
        row as f32 * TILE_SIZE,
    );
    assert!(placed.is_some(), "the pilot stands out");
    assert!(
        dark.light_at(far, row - 2) > before,
        "the helmet lamp lights it"
    );
    // A lit flare sends a hound running.
    let mut flare = self::world();
    park(&mut flare, PLAYER, column);
    set_night(&mut flare);
    #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
    let (hx, hy) = (
        (column as f32 + 12.5) * TILE_SIZE,
        row as f32 * TILE_SIZE - 9.0,
    );
    let hound = flare.spawn_enemy(EnemyKind::ShadowHound, hx, hy);
    let (fx, fy) = (hx - 2.0 * TILE_SIZE, hy - 4.0);
    flare.insert_charge(crate::PlacedBomb {
        id: 900,
        x: fx,
        y: fy,
        previous_x: fx,
        previous_y: fy,
        kind: crate::ExplosiveKind::Flare,
        stage: 1,
        detonation_tick: crate::SimulationTick::new(u64::MAX),
        stuck: true,
        ..crate::PlacedBomb::default()
    });
    let _ = run(&mut flare, 30);
    let mode = flare
        .entities()
        .get(hound)
        .and_then(|entity| match entity.state {
            EntityState::Enemy(state) => Some(state.mode),
            _ => None,
        });
    assert_eq!(mode, Some(EnemyMode::Flee), "the flare scares it off");
}

#[test]
fn the_moon_sets_the_night() {
    // A full moon lets more rise round each actor and lights the night brighter.
    let mut full = world();
    let time = time_of(&full, 84, Some(MoonPhase::Full));
    full.set_world_time_millis(time);
    let mut quarter = world();
    set_night(&mut quarter);
    assert!(full.clock().light_level() > quarter.clock().light_level());
    // A new moon brings the rare spawn, once.
    let mut new = world();
    let column = exposed_column(&new);
    park(&mut new, PLAYER, column);
    let time = time_of(&new, 82, Some(MoonPhase::New));
    new.set_world_time_millis(time);
    let mut rare = 0;
    for _ in 0..60 * 150 {
        rare += step(&mut new, &idle())
            .events
            .iter()
            .filter(|event| matches!(event.kind, GameplayEventKind::RareNightSpawn { .. }))
            .count();
        if rare > 0
            && new
                .night_monsters()
                .any(|(_, s)| s.kind == EnemyKind::UmbralAlpha)
        {
            break;
        }
    }
    assert_eq!(rare, 1, "the alpha walks on a new-moon night");
    assert!(
        new.night_monsters()
            .any(|(_, state)| state.kind == EnemyKind::UmbralAlpha)
    );
    let _ = run(&mut new, 60 * 30);
    assert_eq!(
        new.night_monsters()
            .filter(|(_, state)| state.kind == EnemyKind::UmbralAlpha)
            .count(),
        1,
        "only once a night"
    );
}

#[test]
fn night_kills_drop_night_goods_and_count_in_the_ledger() {
    let mut world = world();
    let column = exposed_column(&world);
    park(&mut world, PLAYER, column);
    set_night(&mut world);
    let _ = run(&mut world, 2);
    let row = ground(&world, column);
    #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
    let (x, y) = (
        (column as f32 + 2.5) * TILE_SIZE,
        row as f32 * TILE_SIZE - 40.0,
    );
    let wisp = world.spawn_enemy(EnemyKind::WillOWisp, x, y);
    let mut output = TickOutput::default();
    let outcome = world.damage_enemy(
        wisp,
        crate::enemy::Damage::new(
            500.0,
            crate::enemy::DamageKind::Explosive,
            crate::enemy::DamageSource::Player(PLAYER),
        ),
        &mut output,
    );
    assert!(matches!(
        outcome,
        crate::enemy::DamageOutcome::Killed { .. }
    ));
    assert_eq!(world.night_ledger().kills_of(EnemyKind::WillOWisp), 1);
    assert_eq!(world.night_ledger().night_kills, 1);
    let essence = world
        .entities()
        .of_kind(EntityKind::Drop)
        .filter_map(|entity| match entity.state {
            EntityState::Drop(drop) => Some(drop.stack),
            _ => None,
        })
        .any(|stack| stack.kind == StrategicResourceKind::WispEssence.into());
    assert!(essence, "a wisp leaves its essence");
}

#[test]
fn the_night_market_opens_and_day_counters_close() {
    let mut world = world();
    let settings = *world.world_settings();
    let (found, market) = (1..30)
        .flat_map(|site| [site, -site])
        .find_map(|site| {
            let found = village::village(&settings, placement::surface_id(site))?;
            let index = found
                .buildings
                .iter()
                .position(|b| b.kind == BuildingKind::Market)?;
            Some((found, index))
        })
        .expect("a market");
    let day = found.offers_at(market, MarketTime::DAY);
    assert!(
        day.iter()
            .any(|offer| matches!(offer.deal, VillageDeal::SellCargo { .. }))
    );
    let night = MarketTime {
        night: true,
        day: 4,
    };
    let stock = found.offers_at(market, night);
    assert!(
        stock
            .iter()
            .any(|offer| matches!(offer.deal, VillageDeal::SellNightGoods { .. })),
        "the market buys night goods after dark"
    );
    for (index, building) in found.buildings.iter().enumerate() {
        let Some(service) = VillageService::of(building.kind) else {
            continue;
        };
        if service.closes_at_night() && service != VillageService::TradingPost {
            assert!(
                found.offers_at(index, night).is_empty(),
                "{service:?} shuts"
            );
        }
        if matches!(service, VillageService::Inn | VillageService::FuelDepot) {
            assert_eq!(found.offers_at(index, night), found.offers(index));
        }
    }
    // Selling night goods at the night market, at the door.
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.cargo_bay.add(StrategicResourceKind::ShadowPelt, 2);
    rig.cargo_bay.add(StrategicResourceKind::MothSilk, 3);
    let column = found.buildings[market].first_column + 2;
    let rig = world.player_mut(PLAYER).expect("rig");
    #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
    {
        rig.x = (column as f32 + 0.5) * TILE_SIZE;
        rig.y = (found.ground_row as f32 - 0.5) * TILE_SIZE;
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    world.stream_terrain_near_players();
    set_night(&mut world);
    let time = world.market_time();
    assert!(time.night);
    let offers = found.offers_at(market, time);
    let index = offers
        .iter()
        .position(|offer| matches!(offer.deal, VillageDeal::SellNightGoods { .. }))
        .expect("sell row");
    let credits = world.player(PLAYER).expect("rig").credits;
    let output = act(
        &mut world,
        PLAYER,
        PlayerCommand::Service(SessionServiceRequest::Village {
            village: found.id,
            building: u8::try_from(market).expect("index"),
            offer: u8::try_from(index).expect("index"),
            service: VillageService::TradingPost,
        }),
    );
    assert_eq!(
        output.action_outcomes.first().expect("outcome").outcome,
        PlayerScopedCommandOutcome::Applied
    );
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        rig.credits >= credits + 2 * 90 + 3 * 45,
        "paid at least full value"
    );
    assert_eq!(rig.cargo_bay.count(StrategicResourceKind::ShadowPelt), 0);
}

#[test]
fn nocturnal_ore_glows_and_reveals_itself_at_night() {
    let mut world = world();
    let column = exposed_column(&world);
    park(&mut world, PLAYER, column);
    let row = ground(&world, column);
    let ore = TilePosition {
        x: column + 9,
        y: row + 8,
    };
    let _ = world
        .terrain_mut()
        .set_kind(ore, TileKind::Ore(MineralKind::Silver));
    let _ = run(&mut world, 2);
    let lit_by_day = world.light_at(ore.x, ore.y);
    assert!(world.nocturnal_ore_at(ore));
    set_night(&mut world);
    // Out of the rig's lamps: measure at the ore, where the rock is dark.
    let glow = world.light_at(ore.x, ore.y);
    assert!(
        glow >= night::NOCTURNAL_ORE_LIGHT,
        "it glows at night ({glow})"
    );
    assert_eq!(lit_by_day, 0, "the rock is dark by day");
    let _ = run(&mut world, 70);
    assert!(
        world
            .discovered_tiles(PLAYER)
            .is_some_and(|tiles| tiles.contains(ore)),
        "and shows itself on the map"
    );
    // The glow is the night's gift, not its danger: on a Peaceful world it still shows itself.
    let mut calm = world_peaceful_with_ore(column, ore);
    set_night(&mut calm);
    let _ = run(&mut calm, 70);
    assert_eq!(night_count(&calm), 0, "nothing rises on Peaceful");
    assert!(
        calm.discovered_tiles(PLAYER)
            .is_some_and(|tiles| tiles.contains(ore)),
        "nocturnal ore shows itself on Peaceful worlds too"
    );
}

fn world_peaceful_with_ore(column: i32, ore: TilePosition) -> WorldState {
    let mut world = world();
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    park(&mut world, PLAYER, column);
    let _ = world
        .terrain_mut()
        .set_kind(ore, TileKind::Ore(MineralKind::Silver));
    world
}

#[test]
fn people_keep_the_skys_night() {
    let mut world = world();
    for slice in [10_u32, 20, 40, 76, 79, 90] {
        let time = time_of(&world, slice, None) + 1;
        world.set_world_time_millis(time);
        assert_eq!(
            world.people_clock().is_night(),
            world.is_night(),
            "slice {slice}"
        );
    }
}

#[test]
fn nights_are_deterministic_and_replicate_exactly() {
    let make = || {
        let mut world = world();
        world.add_player(OTHER);
        let column = exposed_column(&world);
        park(&mut world, PLAYER, column);
        park(&mut world, OTHER, column + 6);
        set_night(&mut world);
        world
    };
    let mut a = make();
    let mut b = make();
    for index in 0..60 * 25 {
        let ea: Vec<_> = step(&mut a, &idle()).events;
        let eb: Vec<_> = step(&mut b, &idle()).events;
        assert_eq!(ea, eb, "events diverged at tick {index}");
    }
    assert_eq!(a.simulation_state_hash(), b.simulation_state_hash());
    assert!(night_count(&a) > 0);
    // Saves (JSON) keep the night, the ledger and every monster.
    let json = serde_json::to_string(&a).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.simulation_state_hash(), a.simulation_state_hash());
    assert_eq!(loaded.night_ledger(), a.night_ledger());
    // A keyframe plus entity deltas and the enemies section keeps a replica exact.
    let mut replica: WorldState = wire(&a);
    let mut held: BTreeMap<EntityId, u64> = a.entity_revisions(None, None).into_iter().collect();
    for _ in 0..60 * 10 {
        let _ = step(&mut a, &idle());
        replica.set_simulation_tick(a.simulation_tick());
        for id in [PLAYER, OTHER] {
            replica.apply_player_tick_state(id, wire(&a.player_tick_state(id).expect("rig")));
        }
        replica.apply_live_entities(wire(&a.live_entities()));
        let current = a.entity_revisions(None, None);
        let updates = current
            .iter()
            .filter(|(id, revision)| held.get(id) != Some(revision))
            .filter_map(|(id, _)| a.entity_record(*id))
            .collect();
        let despawned = held
            .keys()
            .filter(|id| {
                current
                    .binary_search_by_key(*id, |(other, _)| *other)
                    .is_err()
            })
            .copied()
            .collect();
        replica.apply_entity_delta(wire(&EntityDelta { updates, despawned }));
        held = current.into_iter().collect();
        for key in a.section_keys() {
            replica.apply_section(key, wire(&a.section(key).expect("section")));
        }
        assert_eq!(replica.simulation_state_hash(), a.simulation_state_hash());
    }
    assert_eq!(replica.night_ledger(), a.night_ledger());
    let _ = SectionKey::Enemies;
}

#[test]
fn night_rules_are_tick_rate_safe() {
    for hz in [30, 120] {
        let mut world = WorldState::new_game_with_settings(
            PLAYER,
            &WorldSettings::new(SEED),
            SimulationParams::new(hz),
        );
        let column = exposed_column(&world);
        park(&mut world, PLAYER, column);
        set_night(&mut world);
        let events: Vec<GameplayEventKind> = (0..hz * 40)
            .flat_map(|_| step(&mut world, &idle()).events)
            .map(|event| event.kind)
            .collect();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, GameplayEventKind::NightFell { .. })),
            "{hz} Hz"
        );
        assert!(night_count(&world) > 0, "{hz} Hz");
        let _ = DayLength::Standard;
    }
}

#[test]
fn a_contractor_built_home_keeps_the_night_off_its_plot() {
    // Integration seam (T24 x T25): a home plot is safe from night monsters like a bed in a
    // claim, a camp or a village.
    use crate::home::{Blueprint, PlotSetting, PlotSite, PlotSize, SiteKey};
    let mut world = world();
    let column = exposed_column(&world);
    let row = ground(&world, column);
    let spot = TilePosition {
        x: column,
        y: row - 1,
    };
    assert!(!world.night_safe_zone(spot), "open ground before the plot");
    let site = PlotSite {
        key: SiteKey::Surveyed,
        size: PlotSize::Small,
        setting: PlotSetting::Wilds,
        left: column - PlotSize::Small.width() / 2,
        ground_row: row,
        price: 0,
    };
    let _ = world.scenario_home(PLAYER, site, Some(Blueprint::Cabin));
    assert!(world.night_safe_zone(spot), "the plot is safe at night");
    assert!(world.enemy_safe_zone(spot));
}
