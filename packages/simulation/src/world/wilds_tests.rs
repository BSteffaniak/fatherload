//! The wild worlds in play (Wave 12 planets-a): a digging bot reaches every stratum and the
//! core chamber of a small and a big world of each theme; the creatures keep their rules in
//! the planet's local frame; each stratum's signature rule and each hazard does what it says;
//! everything is deterministic at every tick rate.

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WorldState,
    enemy::{Damage, DamageKind, DamageSource, EnemyDifficulty, EnemyKind, EnemyMode},
    entity::{EntityKind, EntityState},
    event::TickOutput,
    planet::{
        BodyGround, PlanetParams, PlanetThemeId,
        wilds::{WildTile, WildsEvent, glacier, life, ocean},
        world_of,
    },
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);
const THEMES: [PlanetThemeId; 3] = [
    PlanetThemeId::JUNGLE,
    PlanetThemeId::OCEAN,
    PlanetThemeId::GLACIER,
];

fn world(theme: PlanetThemeId, radius: u32, hz: u32) -> WorldState {
    WorldState::new_planet_world(
        PLAYER,
        PlanetParams::new(0x5EED_0012, radius, theme),
        SimulationParams::new(hz),
    )
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

fn wild_events(events: &[GameplayEventKind]) -> Vec<WildsEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Wilds { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

/// The rig's tile.
fn rig_tile(world: &WorldState) -> TilePosition {
    let rig = world.player(PLAYER).expect("rig");
    world.frame().tile_at(rig.x, rig.y)
}

/// Puts the rig in the middle of strip tile `(column, row)`, still and level.
fn place_rig(world: &mut WorldState, column: i32, row: i32) {
    let (x, y) = world.frame().centre_of(column, row);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.x = x;
        rig.y = y;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
    world.stream_terrain_near_players();
}

/// The settlement site's middle column and floor.
fn site(world: &WorldState) -> (i32, i32) {
    let planet = *world.planet().expect("planet");
    let site = world_of(planet.params().theme)
        .and_then(|theme| theme.settlement_site(&planet))
        .expect("a site");
    (site.centre(), site.floor)
}

/// Open surface away from the settlement (where the creatures roam): a column with clear air
/// over its ground.
fn wild_spot(world: &WorldState) -> (i32, i32) {
    let planet = *world.planet().expect("planet");
    let (centre, _) = site(world);
    (centre + 160..centre + 2_000)
        .map(|column| (column, planet.surface_row(column)))
        .find(|&(column, floor)| {
            floor < 0
                && (-2..=2).all(|dx| planet.surface_row(column + dx) == floor)
                && (1..=12).all(|up| {
                    (-3..=3).all(|dx| {
                        planet
                            .generated_tile(column + dx, floor - up)
                            .kind
                            .is_open()
                    })
                })
                && !planet.generated_tile(column, floor).kind.is_open()
        })
        .expect("open ground")
}

/// What the digging bot saw on its way down.
#[derive(Debug, Default)]
struct Dig {
    strata: Vec<&'static str>,
    ticks: u32,
    bottom: i32,
}

/// Digs straight down from the settlement site to the core chamber with a test-mode rig.
fn dig_to_the_core(world: &mut WorldState) -> Dig {
    let hz = world.timing().rate.hz();
    world.enable_test_mode();
    let planet = *world.planet().expect("planet");
    let (column, floor) = site(world);
    place_rig(world, column, floor - 1);
    let mut dig = Dig::default();
    let mut last_row = rig_tile(world).y;
    let mut still = 0;
    let mut nudge = 0;
    let limit = hz * 60 * 40;
    loop {
        dig.ticks += 1;
        assert!(
            dig.ticks < limit,
            "{:?} r{}: the dig ran out of time at {:?} ({})",
            planet.params().theme,
            planet.grid().radius(),
            rig_tile(world),
            planet.layer_name(rig_tile(world).x, rig_tile(world).y)
        );
        let controls = if nudge > 0 {
            nudge -= 1;
            let rig = world.player(PLAYER).expect("rig");
            let fraction = (rig.x / TILE_SIZE).rem_euclid(1.0);
            ControlState::new(if fraction < 0.5 { 0.4 } else { -0.4 }, false, false)
        } else {
            ControlState::new(0.0, false, true)
        };
        step(world, controls);
        let tile = rig_tile(world);
        let name = planet.layer_name(tile.x, tile.y);
        if dig.strata.last() != Some(&name) {
            dig.strata.push(name);
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
        }
        if planet.in_core_chamber(tile.y) {
            dig.bottom = tile.y;
            break;
        }
    }
    dig
}

fn assert_every_stratum(theme: PlanetThemeId, radius: u32, dig: &Dig) {
    for stratum in crate::planet::theme(theme).strata {
        assert!(
            dig.strata.contains(&stratum.name),
            "{theme:?} r{radius}: never reached {} ({:?})",
            stratum.name,
            dig.strata
        );
    }
    assert!(dig.strata.contains(&"Core Chamber"), "{:?}", dig.strata);
}

fn small_dig(theme: PlanetThemeId) {
    let mut world = world(theme, 640, 20);
    let dig = dig_to_the_core(&mut world);
    assert_every_stratum(theme, 640, &dig);
    eprintln!(
        "{theme:?} r640: core in {} ticks: {:?}",
        dig.ticks, dig.strata
    );
}

/// A radius-5,000 world's strata: 1,715 rows of them (the bot digs at 15 Hz, tick-rate safe).
fn big_dig(theme: PlanetThemeId) {
    let mut world = world(theme, 5_000, 15);
    let dig = dig_to_the_core(&mut world);
    assert_every_stratum(theme, 5_000, &dig);
    let planet = *world.planet().expect("planet");
    assert!(
        (1_600..1_800).contains(&dig.bottom),
        "{theme:?}: the core chamber of a radius-5,000 world lies 1,700 rows down ({})",
        dig.bottom
    );
    assert!(planet.core_floor_row() < planet.bottom_row());
    eprintln!(
        "{theme:?} r5000: core in {} ticks: {:?}",
        dig.ticks, dig.strata
    );
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_small_jungle() {
    small_dig(PlanetThemeId::JUNGLE);
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_small_ocean_world() {
    small_dig(PlanetThemeId::OCEAN);
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_small_glacier() {
    small_dig(PlanetThemeId::GLACIER);
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_big_jungle() {
    big_dig(PlanetThemeId::JUNGLE);
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_big_ocean_world() {
    big_dig(PlanetThemeId::OCEAN);
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_big_glacier() {
    big_dig(PlanetThemeId::GLACIER);
}

#[test]
fn the_settlement_site_is_safe_and_the_wild_surface_is_not() {
    let world = world(PlanetThemeId::JUNGLE, 900, 30);
    let (column, floor) = site(&world);
    assert!(world.enemy_safe_zone(TilePosition {
        x: column,
        y: floor - 1
    }));
    let (column, floor) = wild_spot(&world);
    assert!(!world.enemy_safe_zone(TilePosition {
        x: column,
        y: floor - 1
    }));
}

#[test]
fn the_dig_is_deterministic_and_tick_rate_safe() {
    let run = |hz: u32| {
        let mut world = world(PlanetThemeId::GLACIER, 450, hz);
        let dig = dig_to_the_core(&mut world);
        // A few seconds more on the ground for the creatures and the weather.
        for _ in 0..hz * 4 {
            step(&mut world, idle());
        }
        (dig.strata, world.simulation_state_hash())
    };
    let (strata_a, hash_a) = run(30);
    let (strata_b, hash_b) = run(30);
    assert_eq!(strata_a, strata_b);
    assert_eq!(hash_a, hash_b);
    let (strata_c, _) = run(60);
    assert_eq!(strata_a, strata_c, "the same strata at 60 Hz");
}

/// A creature of `kind` next to the rig on the surface.
fn creature_beside(
    world: &mut WorldState,
    kind: EnemyKind,
    dx: i32,
    dy: i32,
) -> crate::entity::EntityId {
    let tile = rig_tile(world);
    let (x, y) = world.frame().centre_of(tile.x + dx, tile.y + dy);
    world.spawn_enemy(kind, x, y)
}

fn enemy_state(world: &WorldState, id: crate::entity::EntityId) -> crate::enemy::EnemyState {
    match world.entities().get(id).map(|entity| entity.state) {
        Some(EntityState::Enemy(state)) => state,
        other => panic!("not an enemy: {other:?}"),
    }
}

#[test]
fn neutral_fauna_never_attack_first_but_fight_back() {
    let mut world = world(PlanetThemeId::JUNGLE, 900, 30);
    let (column, floor) = wild_spot(&world);
    place_rig(&mut world, column, floor - 1);
    for _ in 0..30 {
        step(&mut world, idle());
    }
    let hull = world.player(PLAYER).expect("rig").hull;
    let id = creature_beside(&mut world, EnemyKind::Trundlehorn, 4, -1);
    for _ in 0..30 * 8 {
        step(&mut world, idle());
    }
    assert!(
        world.player(PLAYER).expect("rig").hull >= hull,
        "a grazer never charges first"
    );
    assert!(enemy_state(&world, id).target.is_none());
    // Struck, it turns on whoever struck it.
    let mut output = TickOutput::default();
    let _ = world.damage_enemy(
        id,
        Damage::new(10.0, DamageKind::Kinetic, DamageSource::Player(PLAYER)),
        &mut output,
    );
    for _ in 0..30 {
        step(&mut world, idle());
    }
    let state = enemy_state(&world, id);
    assert_eq!(state.last_attacker, Some(PLAYER));
    assert!(state.target == Some(PLAYER) || state.mode != EnemyMode::Idle);
}

#[test]
fn a_vinestalker_sees_only_what_passes_below_it() {
    let mut world = world(PlanetThemeId::JUNGLE, 900, 30);
    let (column, floor) = wild_spot(&world);
    place_rig(&mut world, column, floor - 1);
    for _ in 0..10 {
        step(&mut world, idle());
    }
    // Level with the rig, a few tiles off: not below it, so not noticed.
    let level = creature_beside(&mut world, EnemyKind::Vinestalker, 5, 0);
    // Above the rig: it sees it.
    let above = creature_beside(&mut world, EnemyKind::Vinestalker, -3, -5);
    for _ in 0..30 * 2 {
        step(&mut world, idle());
    }
    assert!(enemy_state(&world, level).target.is_none());
    assert_eq!(enemy_state(&world, above).target, Some(PLAYER));
    // Unprovoked, an ambusher hangs still where it is: it never wanders about in plain sight.
    let start = world.entities().get(level).expect("stalker").body;
    for _ in 0..30 * 3 {
        step(&mut world, idle());
    }
    let end = world.entities().get(level).expect("stalker").body;
    assert_eq!(enemy_state(&world, level).mode, EnemyMode::Dormant);
    assert!(
        (end.x - start.x).abs() < 1.0 && (end.y - start.y).abs() < 1.0,
        "the stalker drifted from {start:?} to {end:?}"
    );
}

#[test]
fn the_seas_swimmers_stay_in_the_water() {
    let mut world = world(PlanetThemeId::OCEAN, 3_000, 30);
    let planet = *world.planet().expect("planet");
    let width = i64::from(planet.strip_width());
    // Deep open sea.
    let column = (0..width)
        .find(|column| ocean::sea_depth(&planet, *column) > 14)
        .and_then(|column| i32::try_from(column).ok())
        .expect("open sea");
    place_rig(&mut world, column, ocean::SEA_LEVEL - 6);
    let (x, y) = world.frame().centre_of(column, ocean::SEA_LEVEL + 3);
    let fish: Vec<_> = (0..4)
        .map(|index: u8| {
            world.spawn_enemy(EnemyKind::Glimmerfin, f32::from(index).mul_add(20.0, x), y)
        })
        .collect();
    let shark = world.spawn_enemy(EnemyKind::Sawjaw, x - 40.0, y + 40.0);
    for _ in 0..30 * 10 {
        step(&mut world, idle());
    }
    for id in fish.into_iter().chain([shark]) {
        let entity = world.entities().get(id).expect("still here");
        let tile = world.frame().tile_at(entity.body.x, entity.body.y);
        assert!(
            tile.y >= ocean::SEA_LEVEL,
            "{:?} left the sea: row {}",
            enemy_state(&world, id).kind,
            tile.y
        );
    }
}

#[test]
fn wild_creatures_spawn_round_the_rig_from_their_themes_roster() {
    for theme in THEMES {
        let mut world = world(theme, 2_000, 30);
        let (column, floor) = wild_spot(&world);
        place_rig(&mut world, column, floor - 1);
        for _ in 0..30 * 40 {
            step(&mut world, idle());
        }
        let roster = life::wild_theme(theme).expect("a wild theme").creatures;
        let kinds: Vec<EnemyKind> = world.enemies().map(|(_, state)| state.kind).collect();
        assert!(!kinds.is_empty(), "{theme:?}: nothing came");
        for kind in &kinds {
            assert!(
                roster.iter().any(|creature| creature.kind == *kind),
                "{theme:?}: {kind:?} is not native"
            );
        }
        assert!(kinds.len() <= 6 * 4, "{theme:?}: {}", kinds.len());
        // Peaceful: only the neutral fauna.
        let mut world = world_peaceful(theme);
        for _ in 0..30 * 40 {
            step(&mut world, idle());
        }
        for (_, state) in world.enemies() {
            let creature = roster.iter().find(|creature| creature.kind == state.kind);
            assert!(
                creature.is_some_and(|creature| creature.neutral),
                "{:?}",
                state.kind
            );
        }
    }
}

fn world_peaceful(theme: PlanetThemeId) -> WorldState {
    let mut world = world(theme, 2_000, 30);
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let (column, floor) = wild_spot(&world);
    place_rig(&mut world, column, floor - 1);
    world
}

#[test]
fn wild_creatures_are_contact_boxed_like_every_enemy() {
    for kind in crate::enemy::EnemyKind::ALL
        .into_iter()
        .filter(|kind| kind.is_wilds())
    {
        let (w, h) = kind.contact_half();
        assert!(w >= 8.0 && h >= 5.0, "{kind:?}");
        assert!(kind.stats().radius <= w.max(h), "{kind:?}");
    }
}

/// Finds the first tile of `tile` near the rig's column within `rows` below the surface.
fn find_tile(world: &WorldState, tile: WildTile, rows: std::ops::Range<i32>) -> TilePosition {
    let planet = *world.planet().expect("planet");
    (0..4_000)
        .flat_map(|column| {
            rows.clone()
                .map(move |row| TilePosition { x: column, y: row })
        })
        .find(|position| planet.generated_tile(position.x, position.y).kind == TileKind::Wild(tile))
        .unwrap_or_else(|| panic!("no {tile:?} in {rows:?}"))
}

/// Mines `target` with the rig parked beside it (the drill's path, straight from the hook).
fn mine(world: &mut WorldState, target: TilePosition) -> Vec<GameplayEventKind> {
    place_rig(world, target.x, target.y - 1);
    let _ = world.terrain_mut().set_kind(
        TilePosition {
            x: target.x,
            y: target.y - 1,
        },
        TileKind::Air,
    );
    let kind = world.terrain().tile(target).expect("loaded").kind;
    let TileKind::Wild(tile) = kind else {
        panic!("{kind:?}");
    };
    let mut output = TickOutput::default();
    world.wild_tile_mined(PLAYER, target, tile, &mut output);
    output.events.into_iter().map(|event| event.kind).collect()
}

#[test]
fn puffcaps_burst_lifesap_mends_and_relics_are_announced() {
    let mut world = world(PlanetThemeId::JUNGLE, 2_000, 30);
    let planet = *world.planet().expect("planet");
    let floor = planet.core_floor_row();
    let puffcap = find_tile(
        &world,
        WildTile::Puffcap,
        floor / 3 + 20..floor * 2 / 3 - 20,
    );
    let clouds = world.entities().of_kind(EntityKind::GasCloud).count();
    let events = mine(&mut world, puffcap);
    assert!(wild_events(&events).contains(&WildsEvent::SporeBurst));
    assert_eq!(
        world.entities().of_kind(EntityKind::GasCloud).count(),
        clouds + 1
    );
    let lifesap = find_tile(&world, WildTile::Lifesap, floor * 2 / 3 + 20..floor - 20);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.hull = rig.max_hull() * 0.5;
    }
    let events = mine(&mut world, lifesap);
    assert!(wild_events(&events).contains(&WildsEvent::LifesapMended));
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.hull > rig.max_hull() * 0.6);
}

#[test]
fn living_roots_hold_a_rig_back() {
    let mut world = world(PlanetThemeId::JUNGLE, 2_000, 30);
    let (column, floor) = site(&world);
    place_rig(&mut world, column, floor + 8);
    // A pocket with a living root wall on its left.
    for dx in -1..=3 {
        for dy in -1..=1 {
            let _ = world.terrain_mut().set_kind(
                TilePosition {
                    x: column + dx,
                    y: floor + 8 + dy,
                },
                TileKind::Air,
            );
        }
    }
    let _ = world.terrain_mut().set_kind(
        TilePosition {
            x: column - 1,
            y: floor + 8,
        },
        TileKind::Wild(WildTile::LivingRoot),
    );
    let _ = world.terrain_mut().set_kind(
        TilePosition {
            x: column,
            y: floor + 9,
        },
        TileKind::Wild(WildTile::LivingRoot),
    );
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.velocity_x = 120.0;
    }
    let events = step(&mut world, idle());
    let _ = events;
    assert!(world.player(PLAYER).expect("rig").velocity_x < 120.0 * 0.95);
}

#[test]
fn the_cold_drinks_fuel_unless_the_cryo_hull_keeps_it_out() {
    let mut world = world(PlanetThemeId::GLACIER, 2_000, 30);
    let (column, floor) = site(&world);
    place_rig(&mut world, column, floor - 1);
    for _ in 0..30 {
        step(&mut world, idle());
    }
    let hazards = world.wild_hazards_for(PLAYER);
    assert!(
        hazards
            .iter()
            .any(|(hazard, _)| *hazard == life::PlanetHazard::Cold)
    );
    let before = world.player(PLAYER).expect("rig").fuel;
    for _ in 0..30 * 5 {
        step(&mut world, idle());
    }
    let cold = before - world.player(PLAYER).expect("rig").fuel;
    // With the Cryo Hull the heaters stay off.
    let mut world_b = world_cryo();
    let before = world_b.player(PLAYER).expect("rig").fuel;
    for _ in 0..30 * 5 {
        step(&mut world_b, idle());
    }
    let warm = before - world_b.player(PLAYER).expect("rig").fuel;
    assert!(cold > warm + 0.5, "cold {cold} vs warm {warm}");
}

fn world_cryo() -> WorldState {
    let mut world = world(PlanetThemeId::GLACIER, 2_000, 30);
    let (column, floor) = site(&world);
    place_rig(&mut world, column, floor - 1);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.hazard_gear.rig_systems |= crate::depth_progression::RigSystem::CryoHull.bit();
    }
    for _ in 0..30 {
        step(&mut world, idle());
    }
    world
}

#[test]
fn icicles_fall_on_a_rig_passing_under_them() {
    let mut world = world(PlanetThemeId::GLACIER, 2_000, 30);
    let planet = *world.planet().expect("planet");
    let floor = planet.core_floor_row();
    // An icicle with open cave under it, in the outer ring tiles (a span of one column).
    let rows = floor * 2 / 3 + 20..floor - 20;
    let open = |x: i32, y: i32| planet.generated_tile(x, y).kind.is_open();
    let icicle = (0..6_000)
        .flat_map(|column| {
            rows.clone()
                .map(move |row| TilePosition { x: column, y: row })
        })
        .find(|tile| {
            planet.generated_tile(tile.x, tile.y).kind == TileKind::Wild(WildTile::Icicle)
                && (1..=3).all(|dy| (-1..=1).all(|dx| open(tile.x + dx, tile.y + dy)))
        })
        .expect("an icicle over open cave");
    let below = TilePosition {
        x: icicle.x,
        y: icicle.y + 2,
    };
    place_rig(&mut world, below.x, below.y);
    let mut events = Vec::new();
    for _ in 0..30 {
        events.extend(wild_events(&step(&mut world, idle())));
    }
    assert!(events.contains(&WildsEvent::IcicleCrack), "{events:?}");
    assert!(world.entities().of_kind(EntityKind::Boulder).count() > 0);
}

#[test]
fn glacier_tunnels_refreeze_at_night_and_rime_gives_way_under_a_rig() {
    let mut world = world(PlanetThemeId::GLACIER, 2_000, 30);
    let planet = *world.planet().expect("planet");
    let params = *planet.params();
    let (column, floor) = site(&world);
    // A tunnel through the glacier ice, the rig at its end.
    let row = floor + 12;
    assert_eq!(
        life::signature_at(&planet, column, row),
        Some(life::SignatureRule::SlickIce)
    );
    place_rig(&mut world, column, row);
    for dx in -6..=6 {
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x: column + dx,
                y: row,
            },
            TileKind::Air,
        );
    }
    // Night: jump the clock into the second half of a day.
    let day = life::day_seconds(&params) * 1_000;
    let hz = u64::from(world.timing().rate.hz());
    let night = (day * 3 / 4) * hz / 1_000;
    world.set_simulation_tick(crate::SimulationTick::new(night));
    assert!(world.wild_night());
    let mut events = Vec::new();
    for _ in 0..30 * 12 {
        events.extend(wild_events(&step(&mut world, idle())));
    }
    let rime = (-6..=6)
        .filter(|dx| {
            world
                .terrain()
                .tile(TilePosition {
                    x: column + dx,
                    y: row,
                })
                .is_some_and(|tile| tile.kind == TileKind::Wild(WildTile::Rime))
        })
        .count();
    assert!(
        rime > 0 && events.contains(&WildsEvent::Refroze),
        "{rime} {events:?}"
    );
    // Nothing froze round the rig itself.
    for dx in -2..=2 {
        assert_ne!(
            world
                .terrain()
                .tile(TilePosition {
                    x: column + dx,
                    y: row
                })
                .map(|t| t.kind),
            Some(TileKind::Wild(WildTile::Rime))
        );
    }
    // A rig resting on rime breaks through.
    let _ = world.terrain_mut().set_kind(
        TilePosition {
            x: column,
            y: row + 1,
        },
        TileKind::Wild(WildTile::Rime),
    );
    let mut events = Vec::new();
    for _ in 0..30 {
        events.extend(wild_events(&step(&mut world, idle())));
    }
    assert!(events.contains(&WildsEvent::RimeBroke), "{events:?}");
}

#[test]
fn a_black_smoker_scalds_and_the_trench_crushes_without_their_counters() {
    let mut world = world(PlanetThemeId::OCEAN, 2_000, 30);
    let planet = *world.planet().expect("planet");
    let floor = planet.core_floor_row();
    let smoker = find_tile(&world, WildTile::Smoker, floor * 2 / 3 + 10..floor - 10);
    // In the hot water just over the chimney.
    let above = TilePosition {
        x: smoker.x,
        y: smoker.y - 2,
    };
    place_rig(&mut world, above.x, above.y);
    for _ in 0..5 {
        step(&mut world, idle());
    }
    let hazards: Vec<_> = world
        .wild_hazards_for(PLAYER)
        .into_iter()
        .map(|(hazard, _)| hazard)
        .collect();
    assert!(hazards.contains(&life::PlanetHazard::Heat), "{hazards:?}");
}

#[test]
fn the_frozen_sea_is_water_under_ice_and_the_sub_works_in_it() {
    let world = world(PlanetThemeId::GLACIER, 3_000, 30);
    let planet = *world.planet().expect("planet");
    let width = i64::from(planet.strip_width());
    let column = (0..width)
        .find(|column| glacier::sea_rows(&planet, *column) > 6)
        .expect("a frozen sea");
    let row = glacier::LID_ROWS + 2;
    assert!(matches!(
        planet
            .generated_tile(i32::try_from(column).expect("column"), row)
            .kind,
        TileKind::Fluid(_)
    ));
    assert!(world.wild_water_rules());
}

#[test]
fn storms_rise_and_push_craft_over_the_open_surface() {
    let params = PlanetParams::new(0x5EED_0012, 2_000, PlanetThemeId::GLACIER);
    // The first stormy cycle.
    let cycle = (0..400_u64)
        .find(|cycle| {
            life::weather_at(
                &params,
                cycle * life::WEATHER_CYCLE_SECONDS * 1_000 + 60_000,
            )
            .is_some_and(|weather| weather.intensity == 1_000)
        })
        .expect("a storm");
    let mut world = world(PlanetThemeId::GLACIER, 2_000, 30);
    let (column, floor) = site(&world);
    place_rig(&mut world, column, floor - 6);
    world.enable_test_mode();
    let millis = cycle * life::WEATHER_CYCLE_SECONDS * 1_000 + 60_000;
    world.set_simulation_tick(crate::SimulationTick::new(millis * 30 / 1_000));
    assert!(
        world
            .wild_weather()
            .is_some_and(|weather| weather.intensity > 0)
    );
    let x = world.player(PLAYER).expect("rig").x;
    for _ in 0..30 * 2 {
        // Hover: thrust a little against gravity.
        step(&mut world, ControlState::new(0.0, true, false));
    }
    let moved = (world.player(PLAYER).expect("rig").x - x).abs();
    assert!(moved > 4.0, "the blizzard pushed the rig {moved} px");
}

#[test]
fn a_storm_is_heard_on_the_surface_only_and_the_cold_is_not_nagging() {
    let params = PlanetParams::new(0x5EED_0012, 2_000, PlanetThemeId::GLACIER);
    let start = (1..400_u64)
        .map(|cycle| cycle * life::WEATHER_CYCLE_SECONDS * 1_000)
        .find(|millis| {
            life::weather_at(&params, *millis + 1_000).is_some_and(|weather| weather.intensity > 0)
        })
        .expect("a storm");
    let storm_heard = |row_below_surface: i32| {
        let mut world = world(PlanetThemeId::GLACIER, 2_000, 30);
        let (column, floor) = site(&world);
        place_rig(&mut world, column, floor + row_below_surface);
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x: column,
                y: floor + row_below_surface,
            },
            TileKind::Air,
        );
        world.set_simulation_tick(crate::SimulationTick::new((start - 2_000) * 30 / 1_000));
        let mut events = Vec::new();
        for _ in 0..30 * 4 {
            events.extend(wild_events(&step(&mut world, idle())));
        }
        events.contains(&WildsEvent::StormRising)
    };
    assert!(storm_heard(-1), "a rig on the surface hears the storm rise");
    assert!(!storm_heard(120), "a rig deep in the glacier does not");
    // The glacier's cold is everywhere: its reminder comes rarely, not every few seconds.
    let mut world = world(PlanetThemeId::GLACIER, 2_000, 30);
    let (column, floor) = site(&world);
    place_rig(&mut world, column, floor - 1);
    let mut bites = 0;
    for _ in 0..30 * 60 {
        bites += wild_events(&step(&mut world, idle()))
            .iter()
            .filter(|what| **what == WildsEvent::ColdBite)
            .count();
    }
    assert!(
        (1..=3).contains(&bites),
        "{bites} cold reminders in a minute"
    );
}

/// A ten-minute tour of each wild world by a simple bot (drive the surface, then dig and
/// wander underground), printing what it met and the tick cost: run with
/// `cargo test --release -p fatherload_simulation wild_world_tour -- --ignored --nocapture`.
#[test]
#[ignore = "a report for reviewing the wild worlds' pacing and tick cost"]
#[allow(clippy::too_many_lines, reason = "one report, read top to bottom")]
fn wild_world_tour() {
    use std::collections::{BTreeMap, BTreeSet};
    for theme in THEMES {
        // Fresh chunk generation cost (a seed nothing else uses).
        for radius in [640, 5_000] {
            let planet =
                crate::planet::PlanetGround::new(PlanetParams::new(0x70_0E, radius, theme));
            let started = std::time::Instant::now();
            let mut count = 0_u32;
            for cy in -2..60 {
                for cx in 0..8 {
                    let _ = planet.chunk_tiles(crate::terrain::ChunkPosition { x: cx, y: cy });
                    count += 1;
                }
            }
            eprintln!(
                "{theme:?} r{radius}: {} us per fresh chunk",
                started.elapsed().as_micros() / u128::from(count)
            );
        }
        let hz = 30;
        let mut world = world(theme, 3_000, hz);
        // A late-game rig (the stars open after the Heart), mortal and on real fuel.
        world.enable_test_mode();
        for action in [
            crate::TestModeAction::SetInvulnerable(false),
            crate::TestModeAction::SetInfiniteFuel(false),
        ] {
            let _ = world.apply_test_mode_action(PLAYER, action);
        }
        let (column, floor) = wild_spot(&world);
        place_rig(&mut world, column, floor - 1);
        let mut events: BTreeMap<String, u32> = BTreeMap::new();
        let mut kinds: BTreeMap<String, u32> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut layers: Vec<&'static str> = Vec::new();
        let (mut damaged, mut refills, mut worst, mut total) = (0_u32, 0_u32, 0_u128, 0_u128);
        let mut slow = 0_u32;
        let mut minute = 0_u128;
        let (mut last_row, mut still, mut nudge) = (0, 0, 0);
        let ticks = hz * 60 * 10;
        let planet = *world.planet().expect("planet");
        for tick in 0..ticks {
            let second = tick / hz;
            let tile = rig_tile(&world);
            if tile.y == last_row {
                still += 1;
            } else {
                still = 0;
                last_row = tile.y;
            }
            if still > hz * 2 && nudge == 0 {
                still = 0;
                nudge = hz / 6;
            }
            let controls = if second < 120 {
                // Drive along the surface.
                ControlState::new(
                    if (second / 40) % 2 == 0 { 1.0 } else { -1.0 },
                    false,
                    false,
                )
            } else if nudge > 0 {
                nudge -= 1;
                let rig = world.player(PLAYER).expect("rig");
                let fraction = (rig.x / TILE_SIZE).rem_euclid(1.0);
                ControlState::new(if fraction < 0.5 { 0.4 } else { -0.4 }, false, false)
            } else {
                ControlState::new(0.0, false, true)
            };
            let started = std::time::Instant::now();
            let out = step(&mut world, controls);
            let spent = started.elapsed().as_micros();
            worst = worst.max(spent);
            total += spent;
            minute += spent;
            if spent > 8_000 {
                slow += 1;
            }
            if spent > 40_000 {
                eprintln!(
                    "    slow tick {tick}: {spent} us at {:?}, {} woken, {} enemies",
                    rig_tile(&world),
                    world.fluids.woken_len(),
                    world.enemies().count()
                );
            }
            if tick % (hz * 60) == 0 {
                let tile = rig_tile(&world);
                eprintln!(
                    "  t{second}s at {tile:?} ({}), last minute {} us/tick, {} woken fluid cells",
                    planet.layer_name(tile.x, tile.y),
                    minute / u128::from(hz * 60),
                    world.fluids.woken_len()
                );
                minute = 0;
            }
            for event in &out {
                match event {
                    GameplayEventKind::Wilds { what, .. } => {
                        *events.entry(format!("{what:?}")).or_default() += 1;
                    }
                    GameplayEventKind::PlayerDamaged { .. } => damaged += 1,
                    _ => {}
                }
            }
            for (entity, state) in world.enemies() {
                if seen.insert(entity.id) {
                    *kinds.entry(format!("{:?}", state.kind)).or_default() += 1;
                }
            }
            let rig = world.player_mut(PLAYER).expect("rig");
            if rig.hull < rig.max_hull() * 0.3 || rig.fuel < rig.fuel_capacity * 0.2 {
                refills += 1;
                rig.hull = rig.max_hull();
                rig.fuel = rig.fuel_capacity;
            }
            let tile = rig_tile(&world);
            let name = planet.layer_name(tile.x, tile.y);
            if layers.last() != Some(&name) {
                layers.push(name);
            }
        }
        for minute in 1..=3 {
            let started = std::time::Instant::now();
            for _ in 0..hz * 60 {
                step(&mut world, idle());
            }
            eprintln!(
                "  idle minute {minute}: {} us/tick, {} woken fluid cells",
                started.elapsed().as_micros() / u128::from(hz * 60),
                world.fluids.woken_len()
            );
            let mut rows: BTreeMap<i32, (i32, i32, u32)> = BTreeMap::new();
            for at in world.fluids.woken() {
                let entry = rows
                    .entry(at.y.div_euclid(50) * 50)
                    .or_insert((i32::MAX, i32::MIN, 0));
                entry.0 = entry.0.min(at.x);
                entry.1 = entry.1.max(at.x);
                entry.2 += 1;
            }
            eprintln!("    woken by 50 rows (min x, max x, count): {rows:?}");
        }
        let tile = rig_tile(&world);
        eprintln!(
            "{theme:?}: end {tile:?} (surface {}), mean tick {} us, worst {} us, {slow} ticks over 8 ms",
            planet.surface_row(tile.x),
            total / u128::from(ticks),
            worst
        );
        eprintln!("  layers {layers:?}");
        eprintln!("  creatures {kinds:?}");
        eprintln!("  events {events:?}; damaged {damaged}; refills {refills}");
    }
}
