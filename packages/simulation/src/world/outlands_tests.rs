//! The outland worlds in play (Wave 13 planets-c): a digging bot reaches every stratum and the
//! core chamber of a small and a big world of each theme; the creatures keep their rules in the
//! planet's local frame; each stratum's signature rule and each hazard does what it says; the
//! theme bosses wake, telegraph, fall and pay; everything is deterministic at every tick rate.

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WorldState,
    enemy::{Damage, DamageKind, DamageSource, EnemyDifficulty, EnemyKind, EnemyMode},
    entity::EntityState,
    event::TickOutput,
    planet::{
        BodyGround, PlanetParams, PlanetThemeId,
        outlands::{
            OUTLAND_THEMES, OutlandTile, OutlandsEvent, boss_lair, dead, fungal, life, machine,
        },
        world_of,
    },
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::outlands::{CONVEYOR_SPEED, VENT_LIFT};

const PLAYER: PlayerId = PlayerId::new(0);

fn world(theme: PlanetThemeId, radius: u32, hz: u32) -> WorldState {
    WorldState::new_planet_world(
        PLAYER,
        PlanetParams::new(0x5EED_0013, radius, theme),
        SimulationParams::new(hz),
    )
}

/// [`world`], for a test whose own `world` shadows it.
fn new_world(theme: PlanetThemeId, radius: u32, hz: u32) -> WorldState {
    world(theme, radius, hz)
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

fn outland_events(events: &[GameplayEventKind]) -> Vec<OutlandsEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Outlands { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

/// Steps `seconds` idle, gathering the outland events.
fn run(world: &mut WorldState, seconds: u32) -> Vec<OutlandsEvent> {
    let hz = world.timing().rate.hz();
    let mut out = Vec::new();
    for _ in 0..hz * seconds {
        out.extend(outland_events(&step(world, idle())));
    }
    out
}

fn rig_tile(world: &WorldState) -> TilePosition {
    let rig = world.player(PLAYER).expect("rig");
    world.frame().tile_at(rig.x, rig.y)
}

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

fn hull(world: &WorldState) -> f32 {
    world.player(PLAYER).expect("rig").hull
}

fn set(world: &mut WorldState, column: i32, row: i32, kind: TileKind) {
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: column, y: row }, kind);
}

fn kind(world: &WorldState, column: i32, row: i32) -> TileKind {
    world
        .terrain()
        .tile(TilePosition { x: column, y: row })
        .expect("loaded")
        .kind
}

/// Opens a pocket of air `half` tiles round `(column, row)`.
fn carve(world: &mut WorldState, column: i32, row: i32, half: i32) {
    for dy in -half..=half {
        for dx in -half..=half {
            set(world, column + dx, row + dy, TileKind::Air);
        }
    }
}

/// The settlement site's middle column and floor.
fn site(world: &WorldState) -> (i32, i32) {
    let planet = *world.planet().expect("planet");
    let site = world_of(planet.params().theme)
        .and_then(|theme| theme.settlement_site(&planet))
        .expect("a site");
    (site.centre(), site.floor)
}

/// Open surface away from the settlement, clear air over level ground.
fn wild_spot(world: &WorldState) -> (i32, i32) {
    let planet = *world.planet().expect("planet");
    let (centre, _) = site(world);
    (centre + 160..centre + 3_000)
        .map(|column| (column, planet.surface_row(column)))
        .find(|&(column, floor)| {
            (-2..=2).all(|dx| planet.surface_row(column + dx) == floor)
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
}

/// A radius-5,000 world's strata: 1,715 rows of them (the bot digs at 15 Hz).
fn big_dig(theme: PlanetThemeId) {
    let mut world = world(theme, 5_000, 15);
    let dig = dig_to_the_core(&mut world);
    assert_every_stratum(theme, 5_000, &dig);
    assert!(
        (1_600..1_800).contains(&dig.bottom),
        "{theme:?}: the core chamber of a radius-5,000 world lies 1,700 rows down ({})",
        dig.bottom
    );
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_small_fungal_world() {
    small_dig(PlanetThemeId::FUNGAL);
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_small_machine_world() {
    small_dig(PlanetThemeId::MACHINE);
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_small_dead_world() {
    small_dig(PlanetThemeId::DEAD);
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_big_fungal_world() {
    big_dig(PlanetThemeId::FUNGAL);
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_big_machine_world() {
    big_dig(PlanetThemeId::MACHINE);
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_big_dead_world() {
    big_dig(PlanetThemeId::DEAD);
}

#[test]
fn the_settlement_is_safe_and_the_outland_surface_is_not() {
    for theme in OUTLAND_THEMES {
        let world = world(theme, 1_500, 30);
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
}

#[test]
fn a_minute_on_each_world_replays_bit_for_bit_and_alike_at_every_tick_rate() {
    for theme in OUTLAND_THEMES {
        let play = |hz: u32| {
            let mut world = world(theme, 1_500, hz);
            let (column, floor) = wild_spot(&world);
            place_rig(&mut world, column, floor - 1);
            let events = run(&mut world, 60);
            (events, world.simulation_state_hash(), rig_tile(&world))
        };
        let (events_a, hash_a, _) = play(30);
        let (events_b, hash_b, _) = play(30);
        assert_eq!(events_a, events_b, "{theme:?}");
        assert_eq!(hash_a, hash_b, "{theme:?}");
        let (_, _, tile_60) = play(60);
        let (_, _, tile_30) = play(30);
        assert!((tile_60.x - tile_30.x).abs() <= 2, "{theme:?}");
    }
}

fn enemy_state(world: &WorldState, id: crate::entity::EntityId) -> crate::enemy::EnemyState {
    match world.entities().get(id).map(|entity| entity.state) {
        Some(EntityState::Enemy(state)) => state,
        other => panic!("not an enemy: {other:?}"),
    }
}

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

#[test]
fn neutral_fauna_never_attack_first_and_answer_a_blow() {
    for (theme, kind) in [
        (PlanetThemeId::FUNGAL, EnemyKind::Capgrazer),
        (PlanetThemeId::MACHINE, EnemyKind::Scuttlebot),
        (PlanetThemeId::DEAD, EnemyKind::Ashrunner),
    ] {
        let mut world = world(theme, 1_500, 30);
        let (column, floor) = wild_spot(&world);
        place_rig(&mut world, column, floor - 1);
        let _ = run(&mut world, 1);
        let id = creature_beside(&mut world, kind, 4, -1);
        let _ = run(&mut world, 6);
        let state = enemy_state(&world, id);
        assert!(state.target.is_none(), "{kind:?} never hunts first");
        assert!(!state.mode.hostile(), "{kind:?}");
        assert!(WorldState::outlands_peaceful(&enemy_state(&world, id)));
        let mut output = TickOutput::default();
        let _ = world.damage_enemy(
            id,
            Damage::new(5.0, DamageKind::Kinetic, DamageSource::Player(PLAYER)),
            &mut output,
        );
        let _ = run(&mut world, 1);
        let state = enemy_state(&world, id);
        assert_eq!(state.last_attacker, Some(PLAYER), "{kind:?}");
        assert!(!WorldState::outlands_peaceful(&state));
    }
}

#[test]
fn a_lanternfly_keeps_a_rig_company_and_an_ashrunner_bolts() {
    let mut world = world(PlanetThemeId::FUNGAL, 1_500, 30);
    let (column, floor) = wild_spot(&world);
    place_rig(&mut world, column, floor - 1);
    let id = creature_beside(&mut world, EnemyKind::Lanternfly, 9, -6);
    let distance = |world: &WorldState| {
        let rig = world.player(PLAYER).expect("rig");
        let body = world.entities().get(id).expect("fly").body;
        (body.x - rig.x).hypot(body.y - rig.y) / TILE_SIZE
    };
    let far = distance(&world);
    let _ = run(&mut world, 6);
    assert!(
        distance(&world) < far - 2.0,
        "{far} -> {}",
        distance(&world)
    );
    assert!(distance(&world) < 6.0);
    let mut world = new_world(PlanetThemeId::DEAD, 1_500, 30);
    let (column, floor) = wild_spot(&world);
    place_rig(&mut world, column, floor - 1);
    let id = creature_beside(&mut world, EnemyKind::Ashrunner, 3, 0);
    let start = world.entities().get(id).expect("runner").body.x;
    let _ = run(&mut world, 3);
    let end = world.entities().get(id).expect("runner").body.x;
    assert!(
        (end - start).abs() > TILE_SIZE * 2.0,
        "it bolts: {start} -> {end}"
    );
}

fn world_of_kind(theme: PlanetThemeId) -> WorldState {
    let mut world = world(theme, 1_500, 30);
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let (column, floor) = wild_spot(&world);
    place_rig(&mut world, column, floor - 1);
    world
}

#[test]
fn outland_creatures_spawn_from_their_themes_roster() {
    for theme in OUTLAND_THEMES {
        let mut world = world(theme, 2_000, 30);
        let (column, floor) = wild_spot(&world);
        place_rig(&mut world, column, floor - 1);
        let _ = run(&mut world, 40);
        let roster = life::outland_theme(theme).expect("life").creatures;
        let kinds: Vec<EnemyKind> = world.enemies().map(|(_, state)| state.kind).collect();
        assert!(!kinds.is_empty(), "{theme:?}: nothing came");
        for kind in &kinds {
            assert!(
                roster.iter().any(|creature| creature.kind == *kind),
                "{theme:?}: {kind:?} is not native"
            );
        }
        let mut world = world_of_kind(theme);
        let _ = run(&mut world, 40);
        for (_, state) in world.enemies() {
            assert!(
                state.kind.is_outland_fauna(),
                "{theme:?} peaceful: {:?}",
                state.kind
            );
        }
    }
}

#[test]
fn outland_creatures_are_contact_boxed_and_the_bosses_are_big() {
    for kind in EnemyKind::ALL.into_iter().filter(|kind| kind.is_outlands()) {
        let (w, h) = kind.contact_half();
        assert!(w >= 8.0 && h >= 7.0, "{kind:?}");
        assert!(kind.stats().radius <= w.max(h), "{kind:?}");
    }
    for boss in crate::planet::outlands::THEME_BOSSES {
        let (w, h) = boss.kind.contact_half();
        assert!(w >= 40.0 && h >= 40.0, "{:?} reads big", boss.kind);
        assert!(boss.kind.stats().elite);
        assert!(
            boss.kind.stats().windup_millis >= 1_000,
            "{:?} telegraphs",
            boss.kind
        );
    }
}

#[test]
fn a_cap_throws_a_falling_rig_back_up() {
    let mut world = world(PlanetThemeId::FUNGAL, 1_500, 30);
    let planet = *world.planet().expect("planet");
    let mushroom = (0..400)
        .find_map(|cell| fungal::mushroom_in_cell(&planet, cell))
        .expect("a mushroom");
    let centre = i32::try_from(mushroom.column + mushroom.width / 2).unwrap_or(0);
    let top = (mushroom.top - 10..=mushroom.top)
        .find(|row| {
            planet.generated_tile(centre, *row).kind == TileKind::Outland(OutlandTile::CapFlesh)
        })
        .expect("the cap's top");
    place_rig(&mut world, centre, top - 4);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.velocity_y = 320.0;
    }
    let mut bounced = false;
    for _ in 0..30 {
        let events = outland_events(&step(&mut world, idle()));
        if events.contains(&OutlandsEvent::CapBounce) {
            bounced = true;
            assert!(world.player(PLAYER).expect("rig").velocity_y < -200.0);
            break;
        }
    }
    assert!(bounced, "the cap threw the rig");
}

#[test]
fn the_mycelium_grows_back_once_the_rig_moves_on() {
    let mut world = world(PlanetThemeId::FUNGAL, 1_500, 30);
    let (column, floor) = site(&world);
    let row = floor + 20;
    place_rig(&mut world, column, row);
    carve(&mut world, column, row, 1);
    // A tunnel off to the side, dug out of the mat.
    let planet = *world.planet().expect("planet");
    let tunnel: Vec<(i32, i32)> = (5..9)
        .map(|dx| (column + dx, row))
        .filter(|(x, y)| {
            planet.generated_tile(*x, *y).kind == TileKind::Outland(OutlandTile::MyceliumMat)
        })
        .collect();
    assert_ne!(tunnel, [] as [(i32, i32); 0]);
    for (x, y) in &tunnel {
        set(&mut world, *x, *y, TileKind::Air);
    }
    let events = run(&mut world, 40);
    let grown = tunnel
        .iter()
        .filter(|(x, y)| kind(&world, *x, *y) == TileKind::Outland(OutlandTile::MyceliumMat))
        .count();
    assert!(grown > 0, "{events:?}");
    assert!(events.contains(&OutlandsEvent::MyceliumRegrew));
    // Never onto the rig.
    let tile = rig_tile(&world);
    assert!(kind(&world, tile.x, tile.y).is_open());
}

#[test]
fn a_glowcaps_flash_stuns_the_hunters_but_not_the_fauna() {
    let mut world = world(PlanetThemeId::FUNGAL, 1_500, 30);
    let (column, floor) = wild_spot(&world);
    place_rig(&mut world, column, floor - 1);
    let hunter = creature_beside(&mut world, EnemyKind::Sporeling, 3, 0);
    let grazer = creature_beside(&mut world, EnemyKind::Capgrazer, -4, 0);
    let target = TilePosition {
        x: column + 1,
        y: floor - 1,
    };
    let mut output = TickOutput::default();
    world.outland_tile_mined(PLAYER, target, OutlandTile::Glowcap, &mut output);
    assert!(
        outland_events(
            &output
                .events
                .iter()
                .map(|event| event.kind)
                .collect::<Vec<_>>()
        )
        .contains(&OutlandsEvent::GlowFlash)
    );
    assert_eq!(enemy_state(&world, hunter).mode, EnemyMode::Stunned);
    assert_ne!(enemy_state(&world, grazer).mode, EnemyMode::Stunned);
}

#[test]
fn a_spore_vent_swells_then_blasts_a_rig_up_its_chimney() {
    let mut world = world(PlanetThemeId::FUNGAL, 2_000, 30);
    let planet = *world.planet().expect("planet");
    let deep = planet.core_floor_row() * 5 / 6;
    let vent = (0..4_000)
        .step_by(10)
        .find_map(|column| fungal::vent_in_cell(&planet, column, deep))
        .expect("a vent");
    let column = i32::try_from(vent.column).unwrap_or(0);
    // Time it to rest just before the swell.
    let mut millis = 0;
    while life::vent_phase(&planet, vent.column, millis) != life::VentPhase::Resting
        || life::vent_phase(&planet, vent.column, millis + 1_000) != life::VentPhase::Swelling
    {
        millis += 100;
    }
    world.set_body_millis(millis);
    world.enable_test_mode();
    place_rig(&mut world, column, vent.row - 2);
    let mut seen = Vec::new();
    let mut lifted = false;
    for _ in 0..30 * 5 {
        // Hold the rig in the chimney.
        place_rig_keep_speed(&mut world, column, vent.row - 2);
        let events = outland_events(&step(&mut world, idle()));
        if events.contains(&OutlandsEvent::VentBlast) {
            lifted = world.player(PLAYER).expect("rig").velocity_y <= -VENT_LIFT + 1.0;
        }
        seen.extend(events);
        if lifted {
            break;
        }
    }
    let swell = seen
        .iter()
        .position(|event| *event == OutlandsEvent::VentSwelling);
    let blast = seen
        .iter()
        .position(|event| *event == OutlandsEvent::VentBlast);
    assert!(
        swell.is_some() && blast.is_some() && swell < blast,
        "{seen:?}"
    );
    assert!(lifted, "the blast threw the rig up");
}

/// Puts the rig at `(column, row)` keeping its velocity.
fn place_rig_keep_speed(world: &mut WorldState, column: i32, row: i32) {
    let (x, y) = world.frame().centre_of(column, row);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.x = x;
        rig.y = y;
    }
}

/// The first generated `tile` in `rows` across the first 4,000 columns.
fn find_tile(world: &WorldState, tile: OutlandTile, rows: std::ops::Range<i32>) -> TilePosition {
    let planet = *world.planet().expect("planet");
    (0..4_000)
        .flat_map(|column| {
            rows.clone()
                .map(move |row| TilePosition { x: column, y: row })
        })
        .find(|position| {
            planet.generated_tile(position.x, position.y).kind == TileKind::Outland(tile)
        })
        .unwrap_or_else(|| panic!("no {tile:?} in {rows:?}"))
}

#[test]
fn a_belt_carries_a_resting_rig_its_way() {
    let mut world = world(PlanetThemeId::MACHINE, 2_000, 30);
    let planet = *world.planet().expect("planet");
    let crust = planet.core_floor_row() / 6;
    let belt = find_tile(&world, OutlandTile::ConveyorEast, crust - 40..crust + 40);
    place_rig(&mut world, belt.x, belt.y - 1);
    carve(&mut world, belt.x, belt.y - 2, 1);
    for dx in -1..=1 {
        set(
            &mut world,
            belt.x + dx,
            belt.y,
            TileKind::Outland(OutlandTile::ConveyorEast),
        );
    }
    world.enable_test_mode();
    let start = world.player(PLAYER).expect("rig").x;
    let _ = run(&mut world, 1);
    let moved = world.player(PLAYER).expect("rig").x - start;
    assert!(moved > CONVEYOR_SPEED * 0.5, "{moved}");
}

#[test]
fn the_gears_clank_then_grind_and_rest() {
    let mut world = world(PlanetThemeId::MACHINE, 2_000, 30);
    let planet = *world.planet().expect("planet");
    let works = planet.core_floor_row() / 2;
    let gear = find_tile(&world, OutlandTile::Gear, works - 40..works + 40);
    // Beside the gear, in its room's air.
    place_rig(&mut world, gear.x - 1, gear.y);
    carve(&mut world, gear.x - 2, gear.y, 0);
    set(&mut world, gear.x - 1, gear.y, TileKind::Air);
    world.set_body_millis(0);
    let mut hulls = Vec::new();
    let mut events = Vec::new();
    for _ in 0..30 * 8 {
        place_rig_keep_speed(&mut world, gear.x - 1, gear.y);
        events.extend(outland_events(&step(&mut world, idle())));
        hulls.push((world.body_millis(), hull(&world)));
    }
    assert!(events.contains(&OutlandsEvent::GearsClank), "{events:?}");
    assert!(events.contains(&OutlandsEvent::GearGrind), "{events:?}");
    // The grind is the turning's: the hull drops faster while the gears turn than at rest (the
    // deep's heat bites alike in both).
    let loss = |phase: life::GearPhase| {
        let losses: Vec<f32> = hulls
            .windows(2)
            .filter(|pair| life::gear_phase(pair[1].0) == phase)
            .map(|pair| pair[0].1 - pair[1].1)
            .collect();
        #[allow(clippy::cast_precision_loss, reason = "a few hundred ticks")]
        let count = losses.len().max(1) as f32;
        losses.iter().sum::<f32>() / count
    };
    let (turning, resting) = (
        loss(life::GearPhase::Turning),
        loss(life::GearPhase::Resting),
    );
    assert!(
        turning > resting.mul_add(2.0, 0.01),
        "turning {turning} vs resting {resting}"
    );
}

#[test]
fn three_lit_relays_open_a_power_vault() {
    let mut world = world(PlanetThemeId::MACHINE, 3_000, 30);
    let planet = *world.planet().expect("planet");
    let width = i64::from(planet.strip_width());
    let shell = planet.core_floor_row() * 5 / 6;
    let vault = (0..width / machine::VAULT_CELL.0)
        .flat_map(|cell| [shell - 30, shell, shell + 30].map(|row| (cell, row)))
        .find_map(|(cell, row)| machine::vault_in_cell(&planet, cell * machine::VAULT_CELL.0, row))
        .expect("a vault");
    let relays = vault.relays(width);
    place_rig(&mut world, relays[1].0, vault.row - 2);
    let lit = |world: &WorldState| {
        relays.map(|(x, y)| kind(world, x, y) == TileKind::Outland(OutlandTile::RelayOn))
    };
    let start = lit(&world);
    assert!(!start.iter().all(|lit| *lit));
    // Lights out on three in a row: find the presses that light them all.
    let mut solution = None;
    for mask in 0..8_u8 {
        let mut state = start;
        for index in 0..3 {
            if mask & (1 << index) != 0 {
                for (other, light) in state.iter_mut().enumerate() {
                    if other.abs_diff(index) <= 1 {
                        *light = !*light;
                    }
                }
            }
        }
        if state.iter().all(|light| *light) {
            solution = Some(mask);
            break;
        }
    }
    let mask = solution.expect("every start can be solved");
    let mut events = Vec::new();
    for (index, (x, y)) in relays.into_iter().enumerate() {
        if mask & (1 << index) == 0 {
            continue;
        }
        let before = kind(&world, x, y);
        let TileKind::Outland(tile) = before else {
            panic!("{before:?}");
        };
        // The drill's path: the relay is mined out, then the hook flips it back in.
        set(&mut world, x, y, TileKind::Air);
        let mut output = TickOutput::default();
        world.outland_tile_mined(PLAYER, TilePosition { x, y }, tile, &mut output);
        events.extend(outland_events(
            &output
                .events
                .iter()
                .map(|event| event.kind)
                .collect::<Vec<_>>(),
        ));
    }
    assert_eq!(lit(&world), [true; 3]);
    assert!(events.contains(&OutlandsEvent::VaultOpened), "{events:?}");
    let seal = (0..machine::VAULT_BOX.0).any(|dx| {
        let x = i32::try_from((vault.column + dx * vault.span).rem_euclid(width)).unwrap_or(0);
        kind(&world, x, vault.row) == TileKind::Outland(OutlandTile::PowerSeal)
    });
    assert!(!seal, "the seals are gone");
}

#[test]
fn loose_ash_pours_into_an_open_shaft_but_never_onto_the_rig() {
    let mut world = world(PlanetThemeId::DEAD, 1_500, 30);
    let (column, floor) = site(&world);
    let row = floor + 12;
    place_rig(&mut world, column, row);
    carve(&mut world, column, row, 1);
    // A column of ash over an empty shaft beside the rig.
    let x = column + 5;
    for y in row - 6..row - 3 {
        set(&mut world, x, y, TileKind::Outland(OutlandTile::AshDrift));
    }
    for y in row - 3..row + 3 {
        set(&mut world, x, y, TileKind::Air);
    }
    set(
        &mut world,
        x,
        row + 3,
        TileKind::Outland(OutlandTile::AshStone),
    );
    let events = run(&mut world, 3);
    assert_eq!(
        kind(&world, x, row + 2),
        TileKind::Outland(OutlandTile::AshDrift),
        "the ash reached the shaft's floor"
    );
    assert!(events.contains(&OutlandsEvent::AshPoured));
    let tile = rig_tile(&world);
    assert!(kind(&world, tile.x, tile.y).is_open());
}

#[test]
fn breaking_a_tomb_seal_wakes_its_wardens() {
    for peaceful in [false, true] {
        let mut world = world(PlanetThemeId::DEAD, 2_000, 30);
        if peaceful {
            world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
        }
        let planet = *world.planet().expect("planet");
        let necropolis = planet.core_floor_row() / 2;
        let seal = find_tile(
            &world,
            OutlandTile::TombSeal,
            necropolis - 36..necropolis + 36,
        );
        place_rig(&mut world, seal.x, seal.y - 4);
        let tomb = dead::tomb_at(&planet, i64::from(seal.x), seal.y).expect("its tomb");
        set(&mut world, seal.x, seal.y, TileKind::Air);
        let mut output = TickOutput::default();
        world.outland_tile_mined(PLAYER, seal, OutlandTile::TombSeal, &mut output);
        let events = outland_events(
            &output
                .events
                .iter()
                .map(|event| event.kind)
                .collect::<Vec<_>>(),
        );
        assert!(events.contains(&OutlandsEvent::TombOpened));
        let width = i64::from(planet.strip_width());
        for (x, y) in tomb.seal(width) {
            assert!(kind(&world, x, y).is_open(), "the seal fell");
        }
        let wardens = world
            .enemies()
            .filter(|(_, state)| state.kind == EnemyKind::Gravewarden)
            .count();
        assert_eq!(wardens, if peaceful { 0 } else { 2 });
    }
}

#[test]
fn a_ghost_light_chills_the_hull_unless_the_cold_gear_keeps_it_out() {
    let chill = |cold_gear: bool| {
        let mut world = world(PlanetThemeId::DEAD, 2_000, 30);
        let planet = *world.planet().expect("planet");
        let mantle = planet.core_floor_row() * 5 / 6;
        if cold_gear && let Some(rig) = world.player_mut(PLAYER) {
            rig.hazard_gear.rig_systems |= crate::depth_progression::RigSystem::CryoHull.bit();
        }
        let wisp_id = life::wisps_near(&planet, 200, mantle, world.body_millis())
            .first()
            .expect("a wisp")
            .id;
        let before = hull(&world);
        let mut events = Vec::new();
        for _ in 0..30 * 4 {
            let millis = world.body_millis();
            let wisp = life::wisps_near(&planet, 200, mantle, millis)
                .into_iter()
                .find(|wisp| wisp.id == wisp_id)
                .expect("the same wisp");
            let (x, y) = wisp.tiles();
            #[allow(clippy::cast_possible_truncation, reason = "a tile")]
            let (column, row) = (x.floor() as i32, y.floor() as i32);
            if rig_tile(&world) != (TilePosition { x: column, y: row }) {
                place_rig(&mut world, column, row);
                carve(&mut world, column, row, 1);
            }
            place_rig_keep_speed(&mut world, column, row);
            events.extend(outland_events(&step(&mut world, idle())));
        }
        (before - hull(&world), events)
    };
    // (The deep's heat bites both alike; the wisp's chill is the difference.)
    let (bare, events) = chill(false);
    assert!(events.contains(&OutlandsEvent::WispChill));
    let (geared, events) = chill(true);
    assert!(!events.contains(&OutlandsEvent::WispChill));
    assert!(bare > geared + 5.0, "bare {bare} vs geared {geared}");
}

#[test]
fn spore_air_and_reactor_heat_bite_without_their_gear() {
    for (theme, system) in [
        (
            PlanetThemeId::FUNGAL,
            crate::depth_progression::RigSystem::BioFilter,
        ),
        (
            PlanetThemeId::MACHINE,
            crate::depth_progression::RigSystem::BioFilter,
        ),
    ] {
        let bite = |geared: bool| {
            let mut world = world(theme, 2_000, 30);
            let planet = *world.planet().expect("planet");
            let (column, floor) = site(&world);
            let row = floor + planet.core_floor_row() * 9 / 10 - 8;
            place_rig(&mut world, column, row);
            carve(&mut world, column, row, 1);
            if geared && let Some(rig) = world.player_mut(PLAYER) {
                rig.hazard_gear.rig_systems |= system.bit();
            }
            let hazards = world.outland_hazards_for(PLAYER);
            let before = hull(&world);
            for _ in 0..30 * 3 {
                place_rig_keep_speed(&mut world, column, row);
                step(&mut world, idle());
            }
            (hazards, before - hull(&world))
        };
        let (hazards, bare) = bite(false);
        assert!(!hazards.is_empty(), "{theme:?}");
        assert!(bare > 0.5, "{theme:?}: {bare}");
        if theme == PlanetThemeId::FUNGAL {
            // (The deep's heat bites both alike; the spores are the difference.)
            let (_, geared) = bite(true);
            assert!(
                bare > geared + 0.5,
                "the Bio-Filter keeps the spores out: {bare} vs {geared}"
            );
        }
    }
}

/// Walks the rig into a lair (its middle, on the floor).
fn into_the_lair(world: &mut WorldState) -> crate::planet::outlands::BossLair {
    let planet = *world.planet().expect("planet");
    let lair = boss_lair(&planet).expect("a lair");
    let (middle, _) = lair.middle();
    let column = i32::try_from(middle.rem_euclid(i64::from(planet.strip_width()))).unwrap_or(0);
    place_rig(world, column + 10, lair.floor - 1);
    lair
}

#[test]
fn each_theme_boss_wakes_calls_its_brood_falls_and_pays() {
    for theme in OUTLAND_THEMES {
        let mut world = world(theme, 1_500, 30);
        let lair = into_the_lair(&mut world);
        let boss = lair.boss.kind;
        let events = run(&mut world, 2);
        assert!(
            events.contains(&OutlandsEvent::BossWoke),
            "{theme:?}: {events:?}"
        );
        let id = world
            .enemies()
            .find(|(_, state)| state.kind == boss)
            .map(|(entity, _)| entity.id)
            .expect("the boss is up");
        assert_eq!(world.outland_boss_beaten_here(), Some(false));
        // Within a brood period it warns, then calls (the test rig is invulnerable).
        world.enable_test_mode();
        let events = run(&mut world, 16);
        let warn = events
            .iter()
            .position(|event| *event == OutlandsEvent::BossSummons);
        assert!(warn.is_some(), "{theme:?}: {events:?}");
        let brood = match boss {
            EnemyKind::Sporemother => EnemyKind::Sporeling,
            EnemyKind::ReactorWarden => EnemyKind::WardDrone,
            _ => EnemyKind::Shade,
        };
        assert!(
            world.enemies().any(|(_, state)| state.kind == brood),
            "{theme:?}"
        );
        // Its fall spends the seal, opens the vault and is announced.
        let mut output = TickOutput::default();
        let _ = world.damage_enemy(
            id,
            Damage::new(1.0e9, DamageKind::Explosive, DamageSource::Player(PLAYER)),
            &mut output,
        );
        let events = outland_events(
            &output
                .events
                .iter()
                .map(|event| event.kind)
                .collect::<Vec<_>>(),
        );
        assert!(events.contains(&OutlandsEvent::BossDefeated), "{theme:?}");
        assert_eq!(world.outland_boss_beaten_here(), Some(true));
        // The endgame's hunt list has it down (Wave 13 integration).
        let quarry = crate::endgame::hunt::Quarry::Theme(theme.0);
        assert!(world.quarry_beaten(quarry), "{theme:?}");
        assert!(
            output.events.iter().any(|event| matches!(
                event.kind,
                GameplayEventKind::Endgame {
                    what: crate::endgame::EndgameEvent::QuarryDown,
                    detail,
                    ..
                } if detail == quarry.code()
            )),
            "{theme:?}"
        );
        for (x, y) in lair.door {
            assert!(
                kind(&world, x, y).is_open(),
                "{theme:?}: the vault door opened"
            );
        }
        assert_eq!(
            kind(&world, lair.relic.0, lair.relic.1),
            TileKind::Outland(lair.boss.relic)
        );
        // It never wakes again.
        let _ = run(&mut world, 2);
        assert!(
            !world.enemies().any(|(_, state)| state.kind == boss),
            "{theme:?}"
        );
    }
}

#[test]
fn a_boss_rests_whole_when_the_crew_leaves_its_lair_and_peaceful_lairs_yield() {
    let mut world = world(PlanetThemeId::MACHINE, 1_500, 30);
    let lair = into_the_lair(&mut world);
    let _ = run(&mut world, 2);
    assert!(
        world
            .enemies()
            .any(|(_, state)| state.kind == lair.boss.kind)
    );
    let (column, floor) = site(&world);
    place_rig(&mut world, column, floor - 1);
    let _ = run(&mut world, 2);
    assert!(
        !world
            .enemies()
            .any(|(_, state)| state.kind == lair.boss.kind)
    );
    assert_eq!(world.outland_boss_beaten_here(), Some(false));
    // Peaceful: no fight; the lair yields its vault.
    let mut world = world_peaceful_lair();
    let events = run(&mut world, 2);
    assert!(events.contains(&OutlandsEvent::BossDefeated));
    assert_eq!(world.outland_boss_beaten_here(), Some(true));
}

fn world_peaceful_lair() -> WorldState {
    let mut world = world(PlanetThemeId::DEAD, 1_500, 30);
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let _ = into_the_lair(&mut world);
    world
}

/// The bot's controls in a theme boss's lair: keep to the floor a dozen tiles off the boss,
/// weave a few tiles back and forth (its volleys are aimed where the rig was), and shoot the
/// nearest of the boss and its brood.
fn lair_bot(
    world: &WorldState,
    lair: &crate::planet::outlands::BossLair,
    tick: u32,
) -> ControlState {
    use crate::WeaponControl;
    let rig = world.player(PLAYER).expect("rig");
    let frame = world.frame();
    let width = i64::from(world.planet().expect("planet").strip_width());
    let (middle, _) = lair.middle();
    let middle = i32::try_from(middle.rem_euclid(width)).unwrap_or(0);
    let (mx, _) = frame.centre_of(middle, lair.floor - 1);
    let boss = world
        .enemies()
        .find(|(_, state)| state.kind == lair.boss.kind)
        .map(|(entity, _)| (entity.body.x, entity.body.y));
    // The boss first (the railgun's slug pierces any brood in the way).
    let target = boss.or_else(|| {
        world
            .enemies()
            .filter(|(_, state)| state.kind.is_outlands() && !state.kind.is_outland_fauna())
            .map(|(entity, _)| (entity.body.x, entity.body.y))
            .min_by(|a, b| {
                (a.0 - rig.x)
                    .hypot(a.1 - rig.y)
                    .total_cmp(&(b.0 - rig.x).hypot(b.1 - rig.y))
            })
    });
    // A stand a dozen tiles off the boss on the rig's side, weaving over a 3 s beat.
    let anchor = boss.map_or(mx, |(bx, _)| bx);
    let side: f32 = if rig.x < anchor { -1.0 } else { 1.0 };
    let weave: f32 = if (tick / 45).is_multiple_of(2) {
        -1.0
    } else {
        1.0
    };
    let goal = side
        .mul_add(12.0, weave * 3.0)
        .mul_add(TILE_SIZE, anchor)
        .clamp(TILE_SIZE.mul_add(-23.0, mx), TILE_SIZE.mul_add(23.0, mx));
    let dx = goal - rig.x;
    let horizontal = if dx.abs() > TILE_SIZE * 0.5 {
        (dx / (TILE_SIZE * 2.0)).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let aim = target.map_or((0, -127), |(tx, ty)| {
        let (ax, ay) = (tx - rig.x, ty - rig.y);
        let length = ax.hypot(ay).max(1.0);
        #[allow(clippy::cast_possible_truncation, reason = "a unit vector times 127")]
        ((ax / length * 127.0) as i8, (ay / length * 127.0) as i8)
    });
    ControlState::new(horizontal, false, false).with_weapon(WeaponControl {
        fire: target.is_some() && tick % 6 < 3,
        auto_aim: false,
        aim_x: aim.0,
        aim_y: aim.1,
    })
}

/// Fair and winnable, measured: a bot with a late-game rig (the kit a crew brings this far
/// out: every upgrade, a railgun) fights each theme boss for real on Normal, without test
/// mode, and wins inside five minutes with its rig whole and no repairs.
#[test]
fn a_bot_with_late_kit_beats_each_theme_boss_for_real() {
    use crate::weapon::WeaponKind;
    for theme in OUTLAND_THEMES {
        let hz = 30;
        let mut world = world(theme, 1_500, hz);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        let lair = into_the_lair(&mut world);
        {
            let rig = world.player_mut(PLAYER).expect("rig");
            crate::economy::max_out_upgrades(rig);
            rig.weapon_rack.battery_level = crate::weapon::MAX_BATTERY_LEVEL;
            let _ = rig.receive_weapon(WeaponKind::Railgun);
            let _ = rig.mount_weapon(Some(WeaponKind::Railgun));
            rig.hull = rig.max_hull();
        }
        let mut won = false;
        let (mut summons, mut least_hull, mut seconds) = (0, f32::MAX, 0);
        for tick in 0..hz * 300 {
            let controls = lair_bot(&world, &lair, tick);
            let events = step(&mut world, controls);
            for what in outland_events(&events) {
                match what {
                    OutlandsEvent::BossSummons => summons += 1,
                    OutlandsEvent::BossDefeated => won = true,
                    _ => {}
                }
            }
            let rig = world.player_mut(PLAYER).expect("rig");
            least_hull = least_hull.min(rig.hull / rig.max_hull());
            assert!(
                rig.hull > 0.0,
                "{theme:?}: the bot's rig was wrecked after {}s",
                tick / hz
            );
            rig.fuel = rig.fuel_capacity;
            if won {
                seconds = tick / hz;
                break;
            }
        }
        assert!(won, "{theme:?}: the bot beat the boss inside five minutes");
        assert_eq!(world.outland_boss_beaten_here(), Some(true));
        // A real fight: long enough for its brood, and it hurt.
        assert!(seconds >= 10, "{theme:?}: over in {seconds}s");
        assert!(summons >= 1, "{theme:?}: the brood came");
        assert!(
            least_hull < 0.98,
            "{theme:?}: the boss never touched the rig"
        );
        eprintln!("{theme:?}: won in {seconds}s, hull down to {least_hull:.2}, {summons} calls");
    }
}

/// The home world's roaming roster (cave bats and the like) stays home: an outland world's
/// creatures come from its own spawner only, so none wander into a fungal forest or a lair.
#[test]
fn the_home_roster_never_spawns_on_an_outland_world() {
    for theme in OUTLAND_THEMES {
        let mut world = world(theme, 1_500, 30);
        let lair = into_the_lair(&mut world);
        let tile = TilePosition {
            x: lair.seal.0,
            y: lair.floor - 2,
        };
        assert!(
            world.terrain().creature_roster(tile).is_empty(),
            "{theme:?}"
        );
        let _ = run(&mut world, 20);
        assert!(
            world.enemies().all(|(_, state)| state.kind.is_outlands()),
            "{theme:?}: {:?}",
            world
                .enemies()
                .map(|(_, state)| state.kind)
                .collect::<Vec<_>>()
        );
    }
}
