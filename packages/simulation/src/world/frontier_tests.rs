//! The frontier worlds at play (Wave 12 planets-b): a digging bot reaches every stratum of a
//! small and a big body of each theme; every signature rule, hazard, the weather, the
//! volcanoes and the creatures do what they say; everything replays bit for bit.

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, SimulationTick, TickInputs,
    WorldState,
    enemy::{Damage, DamageKind, DamageSource, EnemyDifficulty, EnemyKind, EnemyMode},
    entity::EntityKind,
    event::TickOutput,
    frontier::{
        FrontierEvent, FrontierStyle, FrontierTile, StratumRule,
        generate::{light_wells, tube_way},
        rules::{self, WeatherPhase, Zone, zone_at},
        settlement_site,
    },
    planet::{BodyGround, PlanetGround, PlanetParams},
    protection::HazardGear,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x0F20_7E12;

fn world_on(style: FrontierStyle, radius: u32, hz: u32) -> WorldState {
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

fn frontier_events(events: &[GameplayEventKind]) -> Vec<FrontierEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Frontier { what, .. } => Some(*what),
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

/// Puts the rig on (`rows_up` rows above) tile row `floor` of column `column`, still.
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

// --- The digging bot ---------------------------------------------------------------------

/// Digs straight down from the settlement site until the rig has been in every stratum (and,
/// when `core`, the core chamber); the zones it went through, in order.
fn dig_down(style: FrontierStyle, radius: u32, core: bool) -> Vec<Zone> {
    let mut world = world_on(style, radius, 30);
    let hz = world.timing().rate.hz();
    let site = settlement_site(&ground(&world)).expect("a site");
    place(&mut world, site.column + 8, site.surface_row, 1.0);
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
        let zone = zone_at(&ground(&world), tile);
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
            if std::env::var("FRONTIER_DEBUG").is_ok() {
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

fn assert_every_stratum(style: FrontierStyle, radius: u32, zones: &[Zone]) {
    for index in 0..4 {
        assert!(
            zones.contains(&Zone::Stratum(index)),
            "{style:?} R={radius}: stratum {index} never reached: {zones:?}"
        );
    }
}

#[test]
fn a_digging_bot_reaches_every_stratum_and_the_core_of_a_small_body() {
    for style in FrontierStyle::ALL {
        let zones = dig_down(style, 600, true);
        assert_every_stratum(style, 600, &zones);
        assert_eq!(zones.last(), Some(&Zone::CoreChamber), "{style:?}");
    }
}

#[test]
fn a_digging_bot_reaches_every_stratum_of_a_big_planet() {
    for style in FrontierStyle::ALL {
        let zones = dig_down(style, 5_000, false);
        assert_every_stratum(style, 5_000, &zones);
    }
}

// --- Signature rules ---------------------------------------------------------------------

/// A desert world with the rig resting at its settlement; the rig's tile.
fn settled(style: FrontierStyle, radius: u32) -> (WorldState, TilePosition) {
    let mut world = world_on(style, radius, 30);
    let site = settlement_site(&ground(&world)).expect("a site");
    place(&mut world, site.column + 8, site.surface_row, 1.0);
    run_for(&mut world, 1_500);
    let tile = rig_tile(&world);
    (world, tile)
}

#[test]
fn loose_sand_slides_into_open_ground_but_never_into_a_rig() {
    let (mut world, rig) = settled(FrontierStyle::Desert, 600);
    // A column of sand over a pocket, beside the rig.
    let x = rig.x + 4;
    for y in rig.y - 3..rig.y {
        set(&mut world, x, y, TileKind::Frontier(FrontierTile::DuneSand));
    }
    for y in rig.y..rig.y + 3 {
        set(&mut world, x, y, TileKind::Air);
    }
    set(
        &mut world,
        x,
        rig.y + 3,
        TileKind::Frontier(FrontierTile::PackedSand),
    );
    let events = frontier_events(&run_for(&mut world, 1_200));
    assert!(events.contains(&FrontierEvent::SandSlid));
    // The three tiles of sand now rest at the pocket's bottom.
    for y in rig.y..rig.y + 3 {
        assert_eq!(
            kind_at(&world, x, y),
            TileKind::Frontier(FrontierTile::DuneSand),
            "row {y}"
        );
    }
    assert_eq!(kind_at(&world, x, rig.y - 3), TileKind::Air);
    // Sand over the rig stays put above it.
    let over = rig.y - 2;
    set(
        &mut world,
        rig.x,
        over,
        TileKind::Frontier(FrontierTile::DuneSand),
    );
    set(&mut world, rig.x, over + 1, TileKind::Air);
    run_for(&mut world, 1_000);
    assert_ne!(
        kind_at(&world, rig.x, rig.y),
        TileKind::Frontier(FrontierTile::DuneSand),
        "no sand falls into the rig"
    );
}

#[test]
fn a_cracked_ruin_floor_gives_way_under_a_resting_rig() {
    let (mut world, rig) = settled(FrontierStyle::Desert, 600);
    let floor = rig.y + 1;
    for x in rig.x - 3..=rig.x + 3 {
        set(
            &mut world,
            x,
            floor,
            TileKind::Frontier(FrontierTile::RuinFloor),
        );
        for y in floor + 1..floor + 4 {
            set(&mut world, x, y, TileKind::Air);
        }
    }
    let events = frontier_events(&run_for(&mut world, 3_000));
    assert!(events.contains(&FrontierEvent::FloorCrumbled), "{events:?}");
    for x in rig.x - 3..=rig.x + 3 {
        assert_eq!(kind_at(&world, x, floor), TileKind::Air, "column {x}");
    }
    assert!(rig_tile(&world).y > rig.y, "the rig dropped into the vault");
}

#[test]
fn glass_and_crystal_shatter_in_chains_and_glass_nicks_the_hull() {
    for (style, tile) in [
        (FrontierStyle::Crystal, FrontierTile::Crystal),
        (FrontierStyle::Desert, FrontierTile::DesertGlass),
    ] {
        let (mut world, rig) = settled(style, 600);
        vulnerable(&mut world);
        let hull = world.player(PLAYER).expect("rig").hull;
        let (cx, cy) = (rig.x + 8, rig.y - 8);
        for y in cy - 5..=cy + 5 {
            for x in cx - 5..=cx + 5 {
                set(&mut world, x, y, TileKind::Frontier(tile));
            }
        }
        set(&mut world, cx, cy, TileKind::Air);
        let mut output = TickOutput::default();
        world.frontier_mined(PLAYER, TilePosition { x: cx, y: cy }, tile, &mut output);
        let events = frontier_events(
            &output
                .events
                .iter()
                .map(|event| event.kind)
                .collect::<Vec<_>>(),
        );
        // Joined tiles within three break; beyond, the rest stands.
        assert_eq!(kind_at(&world, cx + 1, cy), TileKind::Air, "{style:?}");
        assert_eq!(kind_at(&world, cx, cy - 2), TileKind::Air, "{style:?}");
        assert_eq!(
            kind_at(&world, cx + 5, cy + 5),
            TileKind::Frontier(tile),
            "{style:?}"
        );
        let after = world.player(PLAYER).expect("rig").hull;
        if tile == FrontierTile::DesertGlass {
            assert!(events.contains(&FrontierEvent::GlassShattered));
            assert!(after < hull, "the shards nick the hull");
        } else {
            assert!(events.contains(&FrontierEvent::CrystalRang));
            assert!((after - hull).abs() < f32::EPSILON, "crystal only rings");
        }
    }
}

#[test]
fn obsidian_splinters_unless_the_boulder_deflector_turns_them() {
    let (mut world, rig) = settled(FrontierStyle::Volcanic, 600);
    vulnerable(&mut world);
    let at = TilePosition {
        x: rig.x + 3,
        y: rig.y,
    };
    let hull = world.player(PLAYER).expect("rig").hull;
    let mut output = TickOutput::default();
    world.frontier_mined(PLAYER, at, FrontierTile::Obsidian, &mut output);
    let bare = world.player(PLAYER).expect("rig").hull;
    assert!(bare < hull);
    if let Some(player) = world.player_mut(PLAYER) {
        player.hazard_gear.boulder_deflector = crate::protection::BOULDER_DEFLECTOR_MAX_LEVEL;
    }
    world.frontier_mined(PLAYER, at, FrontierTile::Obsidian, &mut output);
    assert!((world.player(PLAYER).expect("rig").hull - bare).abs() < f32::EPSILON);
}

#[test]
fn a_light_well_dug_a_path_opens_its_sealed_vault() {
    let mut world = world_on(FrontierStyle::Crystal, 1_000, 30);
    let well = light_wells(&ground(&world))[0];
    // Stand by the well's lens and dig the light its path.
    place(&mut world, well.column + 2, well.lens_row, 1.0);
    run_for(&mut world, 600);
    assert_eq!(
        kind_at(&world, well.receptor.x, well.receptor.y),
        TileKind::Frontier(FrontierTile::Receptor),
        "dark while the stone blocks it"
    );
    for y in well.lens_row + 1..well.prism.y {
        set(&mut world, well.column, y, TileKind::Air);
    }
    let way = if well.east { 1 } else { -1 };
    let mut x = well.prism.x + way;
    while x != well.receptor.x {
        set(&mut world, x, well.prism.y, TileKind::Air);
        x += way;
    }
    let events = frontier_events(&run_for(&mut world, 1_200));
    assert!(events.contains(&FrontierEvent::BeamLit), "{events:?}");
    assert!(events.contains(&FrontierEvent::VaultOpened));
    assert_eq!(
        kind_at(&world, well.receptor.x, well.receptor.y),
        TileKind::Frontier(FrontierTile::ReceptorLit)
    );
    let (x0, x1, y0, y1) = well.vault;
    for (x, y) in [(x0, y0), (x1, y1), (x0, y1), (x1, y0)] {
        assert_eq!(
            kind_at(&world, x, y),
            TileKind::Air,
            "the seal at ({x}, {y})"
        );
    }
}

/// The rig resting in a pocket dug at depth `depth` rows below the settlement site.
fn underground(style: FrontierStyle, radius: u32, depth: i32) -> (WorldState, TilePosition) {
    let mut world = world_on(style, radius, 30);
    let site = settlement_site(&ground(&world)).expect("a site");
    let (x, floor) = (site.column + 8, site.surface_row + depth);
    for y in floor - 4..floor {
        for dx in -3..=3 {
            set(&mut world, x + dx, y, TileKind::Air);
        }
    }
    for dx in -3..=3 {
        set(
            &mut world,
            x + dx,
            floor,
            TileKind::Frontier(FrontierTile::ShardStone),
        );
    }
    place(&mut world, x, floor, 1.0);
    run_for(&mut world, 800);
    let tile = rig_tile(&world);
    (world, tile)
}

/// A depth (rows below the surface) in the middle of stratum `index` of `style` at `radius`.
fn stratum_depth(radius: u32, index: usize) -> i32 {
    let rows = crate::frontier::strata_rows(radius);
    [125, 400, 675, 900][index] * rows / 1_000
}

#[test]
fn crystal_grows_back_into_a_tunnel_but_never_into_the_rig() {
    let depth = stratum_depth(1_000, 2);
    let (mut world, rig) = underground(FrontierStyle::Crystal, 1_000, depth);
    assert_eq!(
        rules::rule_at(&ground(&world), FrontierStyle::Crystal, rig),
        Some(StratumRule::Regrowth)
    );
    // An open nook with crystal on two sides, two tiles over from the rig.
    let nook = TilePosition {
        x: rig.x + 3,
        y: rig.y - 1,
    };
    set(&mut world, nook.x, nook.y, TileKind::Air);
    set(
        &mut world,
        nook.x + 1,
        nook.y,
        TileKind::Frontier(FrontierTile::Crystal),
    );
    set(
        &mut world,
        nook.x,
        nook.y - 1,
        TileKind::Frontier(FrontierTile::Crystal),
    );
    // And crystal hugging the rig's own tile.
    set(
        &mut world,
        rig.x - 1,
        rig.y - 1,
        TileKind::Frontier(FrontierTile::Crystal),
    );
    let events = frontier_events(&run_for(&mut world, 9_000));
    assert!(events.contains(&FrontierEvent::CrystalRegrew));
    assert_eq!(
        kind_at(&world, nook.x, nook.y),
        TileKind::Frontier(FrontierTile::Crystal)
    );
    assert_eq!(
        kind_at(&world, rig.x, rig.y),
        TileKind::Air,
        "never into the rig"
    );
}

#[test]
fn toxic_air_and_cold_bite_without_the_planet_gear_and_not_with_it() {
    let scrubber: fn(&mut HazardGear) = |gear| gear.gas_scrubber = 6;
    let de_icer: fn(&mut HazardGear) = |gear| gear.de_icer = 6;
    for (style, cover) in [
        (FrontierStyle::Volcanic, scrubber),
        (FrontierStyle::Crystal, de_icer),
    ] {
        let (mut world, _) = settled(style, 600);
        vulnerable(&mut world);
        // Heat is the classic model's: cool the rig so only the planet hazard bites.
        if let Some(rig) = world.player_mut(PLAYER) {
            rig.radiator_level = crate::depth_progression::DESCENT_MAX_UPGRADE_LEVEL;
            rig.hazard_gear.heat_shield = crate::protection::HEAT_SHIELD_MAX_LEVEL;
        }
        let hull = world.player(PLAYER).expect("rig").hull;
        run_for(&mut world, 2_000);
        let bitten = world.player(PLAYER).expect("rig").hull;
        assert!(bitten < hull - 1.0, "{style:?}: {hull} -> {bitten}");
        if let Some(rig) = world.player_mut(PLAYER) {
            cover(&mut rig.hazard_gear);
        }
        run_for(&mut world, 2_000);
        let covered = world.player(PLAYER).expect("rig").hull;
        assert!(
            (covered - bitten).abs() < 0.01,
            "{style:?}: {bitten} -> {covered}"
        );
    }
}

#[test]
fn the_heavy_lattice_pulls_half_again_as_hard() {
    let fall = |index: usize| {
        let depth = stratum_depth(1_000, index);
        let (mut world, rig) = underground(FrontierStyle::Crystal, 1_000, depth);
        // Open a shaft under the rig and let it fall for a quarter second.
        for y in rig.y + 1..rig.y + 12 {
            for dx in -1..=1 {
                set(&mut world, rig.x + dx, y, TileKind::Air);
            }
        }
        let start = world.player(PLAYER).expect("rig").y;
        run_for(&mut world, 250);
        world.player(PLAYER).expect("rig").y - start
    };
    let geodes = fall(2);
    let lattice = fall(3);
    assert!(lattice > geodes * 1.25, "{geodes} vs {lattice}");
}

#[test]
fn hot_draughts_carry_a_rig_along_a_lava_tube() {
    let mut world = world_on(FrontierStyle::Volcanic, 1_000, 30);
    let g = ground(&world);
    let column = 900;
    let surface = g.surface_row(column);
    let (row, way) = (surface..surface + 600)
        .find_map(|row| {
            (zone_at(&g, TilePosition { x: column, y: row }) == Zone::Stratum(1))
                .then(|| tube_way(&g, i64::from(column), row).map(|way| (row, way)))
                .flatten()
        })
        .expect("a lava tube under the column");
    // Clear the tube round the spot and set the rig in it.
    for y in row - 1..=row + 1 {
        for x in column - 6..=column + 6 {
            set(&mut world, x, y, TileKind::Air);
        }
    }
    set(
        &mut world,
        column,
        row + 2,
        TileKind::Frontier(FrontierTile::TubeCrust),
    );
    place(&mut world, column, row + 2, 1.0);
    run_for(&mut world, 400);
    let before = world.player(PLAYER).expect("rig").x;
    run_for(&mut world, 400);
    let moved = world.player(PLAYER).expect("rig").x - before;
    #[allow(clippy::cast_precision_loss, reason = "a sign")]
    let way = way as f32;
    assert!(moved * way > 30.0, "carried {way}: {moved}");
}

/// World time (milliseconds) of the first moment `phase` holds for `style`'s weather.
fn first_weather(style: FrontierStyle, phase: WeatherPhase) -> u64 {
    (0..2_000_u64)
        .map(|second| second * 1_000 + 500)
        .find(|millis| {
            let weather = rules::weather_at(style, SEED, *millis);
            weather.phase == phase
                && (phase != WeatherPhase::Storm || weather.strength_permille == 1_000)
        })
        .expect("the phase comes")
}

fn set_time(world: &mut WorldState, millis: u64) {
    let hz = u64::from(world.tick_rate_hz());
    world.set_simulation_tick(SimulationTick::new(millis * hz / 1_000));
}

#[test]
fn a_sandstorm_pushes_rigs_on_the_surface_with_the_wind() {
    let (mut world, _) = settled(FrontierStyle::Desert, 600);
    let storm = first_weather(FrontierStyle::Desert, WeatherPhase::Storm);
    set_time(&mut world, storm);
    let wind = f32::from(rules::weather_at(FrontierStyle::Desert, SEED, storm).wind);
    // Hover just over the flat ground so friction does not hold the rig.
    let before = world.player(PLAYER).expect("rig").x;
    run_for(&mut world, 500);
    let moved = world.player(PLAYER).expect("rig").x - before;
    assert!(moved * wind > 20.0, "wind {wind}: {moved}");
    // Calm air leaves it be.
    let calm = first_weather(FrontierStyle::Desert, WeatherPhase::Calm);
    let (mut world, _) = settled(FrontierStyle::Desert, 600);
    set_time(&mut world, calm);
    let before = world.player(PLAYER).expect("rig").x;
    run_for(&mut world, 500);
    assert!((world.player(PLAYER).expect("rig").x - before).abs() < 1.0);
    // Steering is no shelter: driving into the wind gains less ground than in calm air.
    let into_wind = |millis: u64| {
        let (mut world, _) = settled(FrontierStyle::Desert, 600);
        set_time(&mut world, millis);
        let before = world.player(PLAYER).expect("rig").x;
        let ticks = world.timing().ticks_for_millis(500);
        for _ in 0..ticks {
            step(&mut world, ControlState::new(-wind, false, false));
        }
        (world.player(PLAYER).expect("rig").x - before) * -wind
    };
    let (headwind, still_air) = (into_wind(storm), into_wind(calm));
    assert!(
        headwind < still_air - 10.0,
        "headwind {headwind} vs calm {still_air}"
    );
}

#[test]
fn a_lava_bomb_hurts_the_rig_it_lands_on_after_its_shadow_and_leaves_a_cinder() {
    let mut world = world_on(FrontierStyle::Volcanic, 1_000, 30);
    vulnerable(&mut world);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.radiator_level = crate::depth_progression::DESCENT_MAX_UPGRADE_LEVEL;
        rig.hazard_gear.heat_shield = crate::protection::HEAT_SHIELD_MAX_LEVEL;
        rig.hazard_gear.gas_scrubber = 6;
    }
    let g = ground(&world);
    // The first bomb of the Caldera's first eruption that falls on dry ground.
    let caldera = crate::frontier::landmark(&g).expect("the Caldera");
    let cone = crate::frontier::generate::volcano_cones(&g)
        .into_iter()
        .find(|cone| cone.caldera)
        .expect("a caldera");
    let _ = caldera;
    let bomb = (0..600_u64)
        .flat_map(|second| {
            rules::bombs_landing(&g, cone.index, second * 1_000, second * 1_000 + 1_000)
        })
        .find(|bomb| {
            let column = i32::try_from(bomb.column).expect("column");
            g.surface_row(column) < 0
                && !matches!(
                    g.generated_tile(column, g.surface_row(column) - 1).kind,
                    TileKind::Lava
                )
        })
        .expect("a bomb on dry ground");
    let column = i32::try_from(bomb.column).expect("column");
    let floor = g.surface_row(column);
    place(&mut world, column, floor, 1.0);
    run_for(&mut world, 300);
    set_time(&mut world, bomb.land - 600);
    assert!(rules::bombs_in_flight(&g, cone.index, bomb.land - 600).contains(&bomb));
    let hull = world.player(PLAYER).expect("rig").hull;
    let events = frontier_events(&run_for(&mut world, 900));
    assert!(
        events.contains(&FrontierEvent::LavaBombLanded),
        "{events:?}"
    );
    let after = world.player(PLAYER).expect("rig").hull;
    assert!(after < hull - 5.0, "{hull} -> {after}");
    // Moved off, the next bomb on dry ground leaves a cinder where it fell.
    let next = (0..600_u64)
        .flat_map(|second| {
            rules::bombs_landing(&g, cone.index, second * 1_000, second * 1_000 + 1_000)
        })
        .filter(|later| later.land > bomb.land + 2_000)
        .find(|later| {
            let column = i32::try_from(later.column).expect("column");
            (column - bomb_column(bomb)).abs() > 6
                && g.generated_tile(column, g.surface_row(column) - 1).kind == TileKind::Air
                && g.surface_row(column) < 0
        })
        .expect("another bomb");
    set_time(&mut world, next.land - 100);
    run_for(&mut world, 400);
    let column = i32::try_from(next.column).expect("column");
    let top = (g.surface_row(column) - 8..g.surface_row(column) + 2)
        .find(|row| !kind_at(&world, column, *row).is_open())
        .expect("ground");
    assert_eq!(
        kind_at(&world, column, top),
        TileKind::Frontier(FrontierTile::Cinder)
    );
}

fn bomb_column(bomb: rules::LavaBomb) -> i32 {
    i32::try_from(bomb.column).expect("column")
}

// --- Creatures ---------------------------------------------------------------------------

#[test]
fn each_world_spawns_its_own_creatures_in_fitting_spots_out_of_sight() {
    for style in FrontierStyle::ALL {
        for depth in [None, Some(1_usize), Some(3)] {
            let (mut world, rig) = depth.map_or_else(
                || settled(style, 1_000),
                |index| underground(style, 1_000, stratum_depth(1_000, index)),
            );
            world.set_enemy_difficulty(EnemyDifficulty::Hard);
            let zone = zone_at(&ground(&world), rig);
            let roster = crate::frontier::roster(style, zone);
            run_for(&mut world, 40_000);
            let mut kinds = Vec::new();
            for (entity, state) in world.enemies() {
                assert!(
                    crate::frontier::creatures(style).contains(&state.kind),
                    "{style:?}: {:?} is not native",
                    state.kind
                );
                let _ = entity;
                kinds.push(state.kind);
            }
            // On the open surface something always comes; below, only where there is room.
            assert!(
                !kinds.is_empty() || roster.is_empty() || depth.is_some(),
                "{style:?} {zone:?}: nothing came in 40 s"
            );
        }
    }
}

#[test]
fn neutral_fauna_never_hunt_and_shy_away_from_a_rig() {
    let (mut world, rig) = settled(FrontierStyle::Desert, 1_000);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let (x, y) = world.frame().centre_of(rig.x + 3, rig.y);
    let skipper = world.spawn_enemy(EnemyKind::DuneSkipper, x, y);
    let start = world.entities.get(skipper).expect("skipper").body.x;
    let rig_x = world.player(PLAYER).expect("rig").x;
    for _ in 0..world.timing().ticks_for_millis(3_000) {
        step(&mut world, idle());
        if let Some((_, state)) = world.enemy_parts(skipper) {
            assert!(!state.mode.hostile(), "a skipper never hunts");
            assert!(state.target.is_none());
        }
    }
    let end = world.entities.get(skipper).expect("skipper").body.x;
    assert!(
        (end - rig_x).abs() > (start - rig_x).abs() + TILE_SIZE,
        "it bolted: {start} -> {end} (rig at {rig_x})"
    );
}

#[test]
fn the_dune_wyrm_hears_drilling_three_times_as_far_in_the_galleries() {
    for (index, far) in [(0_usize, false), (2, true)] {
        let depth = stratum_depth(1_000, index);
        let (mut world, rig) = underground(FrontierStyle::Desert, 1_000, depth);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        // A wyrm 20 tiles off in the sand.
        let (x, y) = world.frame().centre_of(rig.x + 20, rig.y);
        let wyrm = world.spawn_enemy(EnemyKind::DuneWyrm, x, y);
        let mut noticed = false;
        for _ in 0..world.timing().ticks_for_millis(1_500) {
            step(&mut world, ControlState::new(0.0, false, true));
            if world
                .enemy_parts(wyrm)
                .is_some_and(|(_, state)| state.target == Some(PLAYER))
            {
                noticed = true;
            }
        }
        assert_eq!(
            noticed, far,
            "stratum {index}: a drill 20 tiles off is heard only in the galleries"
        );
    }
}

#[test]
fn a_hunting_dune_wyrm_ripples_the_sand_before_it_strikes() {
    let depth = stratum_depth(1_000, 2);
    let (mut world, rig) = underground(FrontierStyle::Desert, 1_000, depth);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let (x, y) = world.frame().centre_of(rig.x + 6, rig.y);
    let _wyrm = world.spawn_enemy(EnemyKind::DuneWyrm, x, y);
    let mut events = Vec::new();
    for _ in 0..world.timing().ticks_for_millis(6_000) {
        events.extend(step(&mut world, ControlState::new(0.0, false, true)));
    }
    assert!(
        frontier_events(&events).contains(&FrontierEvent::WyrmRipple),
        "the wyrm's telegraph is heard"
    );
}

#[test]
fn a_struck_chime_drifter_rings_the_crystal_round_it_to_pieces() {
    let (mut world, rig) = settled(FrontierStyle::Crystal, 1_000);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let (cx, cy) = (rig.x + 10, rig.y - 6);
    for y in cy - 4..=cy + 4 {
        for x in cx - 4..=cx + 4 {
            set(&mut world, x, y, TileKind::Air);
        }
    }
    for (dx, dy) in [(2, 0), (-2, 1), (0, 2), (4, 4)] {
        set(
            &mut world,
            cx + dx,
            cy + dy,
            TileKind::Frontier(FrontierTile::Crystal),
        );
    }
    let (x, y) = world.frame().centre_of(cx, cy);
    let drifter = world.spawn_enemy(EnemyKind::ChimeDrifter, x, y);
    let max = world.enemy_parts(drifter).expect("drifter").1.max_health;
    let mut output = TickOutput::default();
    let _ = world.damage_enemy(
        drifter,
        Damage::new(max * 0.3, DamageKind::Kinetic, DamageSource::Player(PLAYER)),
        &mut output,
    );
    let events = frontier_events(&step(&mut world, idle()));
    assert!(events.contains(&FrontierEvent::DrifterChimed));
    let here = world
        .enemy_parts(drifter)
        .map(|(body, _)| world.frame().tile_at(body.x, body.y))
        .expect("drifter");
    // The crystal within three tiles of it shattered; farther crystal stands.
    for (dx, dy) in [(2, 0), (-2, 1), (0, 2)] {
        let at = (cx + dx, cy + dy);
        if (at.0 - here.x).pow(2) + (at.1 - here.y).pow(2) <= 9 {
            assert_eq!(kind_at(&world, at.0, at.1), TileKind::Air);
        }
    }
    assert_eq!(
        kind_at(&world, cx + 4, cy + 4),
        TileKind::Frontier(FrontierTile::Crystal)
    );
}

#[test]
fn a_geode_mimic_lies_dormant_until_a_rig_comes_close() {
    let (mut world, rig) = underground(FrontierStyle::Crystal, 1_000, stratum_depth(1_000, 2));
    for y in rig.y - 3..=rig.y {
        for x in rig.x..=rig.x + 14 {
            set(&mut world, x, y, TileKind::Air);
        }
    }
    for x in rig.x..=rig.x + 14 {
        set(
            &mut world,
            x,
            rig.y + 1,
            TileKind::Frontier(FrontierTile::ShardStone),
        );
    }
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let (x, y) = world.frame().centre_of(rig.x + 12, rig.y);
    let mimic = world.spawn_enemy(EnemyKind::GeodeMimic, x, y);
    run_for(&mut world, 1_000);
    assert_eq!(
        world.enemy_parts(mimic).expect("mimic").1.mode,
        EnemyMode::Dormant
    );
    let (x, _) = world.frame().centre_of(rig.x + 10, rig.y);
    if let Some(player) = world.player_mut(PLAYER) {
        player.x = x;
    }
    run_for(&mut world, 1_000);
    assert_ne!(
        world.enemy_parts(mimic).map(|(_, state)| state.mode),
        Some(EnemyMode::Dormant),
        "it opens when a rig comes close"
    );
}

// --- Determinism -------------------------------------------------------------------------

/// A minute of play on `style`: creatures, weather and rules at work, the rig digging.
fn a_minute(
    style: FrontierStyle,
    hz: u32,
) -> (crate::hash::SimulationStateHash, Vec<GameplayEventKind>) {
    let mut world = world_on(style, 800, hz);
    world.set_enemy_difficulty(EnemyDifficulty::Hard);
    let site = settlement_site(&ground(&world)).expect("a site");
    place(&mut world, site.column + 8, site.surface_row, 1.0);
    let storm = first_weather(style, WeatherPhase::Rising);
    set_time(&mut world, storm);
    let mut events = Vec::new();
    for tick in 0..hz * 60 {
        let drill = !(tick / hz).is_multiple_of(3);
        events.extend(step(&mut world, ControlState::new(0.0, false, drill)));
    }
    (world.simulation_state_hash(), events)
}

#[test]
fn a_minute_on_each_world_replays_bit_for_bit_at_any_tick_rate() {
    for style in FrontierStyle::ALL {
        let (first, events) = a_minute(style, 30);
        let (again, events_again) = a_minute(style, 30);
        assert_eq!(first, again, "{style:?}");
        assert_eq!(events, events_again);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, GameplayEventKind::Frontier { .. })),
            "{style:?}: the world did something"
        );
        // Sixty hertz plays the same rules (its own exact numbers).
        let (fast, _) = a_minute(style, 60);
        assert_eq!(fast, a_minute(style, 60).0);
    }
    let _ = EntityKind::Enemy;
}
