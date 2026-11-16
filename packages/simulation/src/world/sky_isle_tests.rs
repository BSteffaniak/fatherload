//! Sky isles in play (WP-SKY, T20): caches and nests, fuel buoys and their economics (they
//! never open the dead-air gap), sky crafting at sky town counters, collapsing isles and
//! their chain reactions, jet streams and vents, determinism, saves and replication.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "test arithmetic on small tile coordinates"
)]

use std::sync::Arc;

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, PlayerScopedCommandOutcome,
    SessionServiceRequest, SimulationParams, SimulationTick, TickInputs, TickOutput, WorldState,
    climb::{CLIMB_CEILINGS, ENGINE_TIERS, INTAKE_TIERS, TANK_TIERS},
    entity::EntityKind,
    environment,
    hazard::PlacedBomb,
    milestone::{DEAD_AIR_REACH_ROWS, crosses_dead_air},
    player::{Player, fuel_capacity_for_level},
    sky::{SkyTile, SkyTownKind},
    skycraft::SkyRecipe,
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    village::{self, Village, VillageDeal, VillageService},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        sky::{
            self, ARCHIPELAGO_BOTTOM_ROWS, BuoySite, DEAD_AIR_BOTTOM_ROWS, LOW_ISLES_BOTTOM_ROWS,
            SkyLand,
            isles::{BUOY_PRICE, buoy_fill_level, buoy_of, buoy_refill},
        },
    },
};

const PLAYER: PlayerId = PlayerId::new(1);
const SEED: u64 = 0x5C1E_7020;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V9,
        ..WorldSettings::new(SEED)
    }
}

fn world() -> WorldState {
    WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(60))
}

fn pixel(tile: i32) -> f32 {
    (tile as f32 + 0.5) * TILE_SIZE
}

/// Puts the rig at rest standing on row `ground` of column `x`.
fn park(world: &mut WorldState, x: i32, ground: i32) {
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.x = pixel(x);
    rig.y = (ground as f32 - 0.5) * TILE_SIZE;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.x_remainder = 0.0;
    world.stream_terrain_near_players();
}

fn step(world: &mut WorldState, inputs: &TickInputs) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, inputs)
}

fn idle() -> TickInputs {
    TickInputs::held(PLAYER, ControlState::new(0.0, false, false))
}

fn outcome(output: &TickOutput) -> PlayerScopedCommandOutcome {
    output.action_outcomes.first().expect("one action").outcome
}

/// Islands of the low isle band near town.
fn low_isles() -> Vec<SkyLand> {
    sky::lands_touching(
        &settings(),
        &TileRect {
            min: TilePosition {
                x: -6_000,
                y: sky::row_of(ARCHIPELAGO_BOTTOM_ROWS),
            },
            max: TilePosition {
                x: 6_000,
                y: sky::row_of(LOW_ISLES_BOTTOM_ROWS),
            },
        },
    )
}

fn nearest_buoy() -> (SkyLand, BuoySite) {
    let mut buoys: Vec<(SkyLand, BuoySite)> = low_isles()
        .into_iter()
        .filter_map(|land| buoy_of(&land).map(|buoy| (land, buoy)))
        .collect();
    buoys.sort_by_key(|(_, buoy)| buoy.pad.x.abs());
    buoys.first().copied().expect("a buoy near town")
}

#[test]
fn drilling_a_sky_cache_pays_credits_and_spills_sky_loot() {
    let mut world = world();
    let land = low_isles()
        .into_iter()
        .filter(|land| land.content().and_then(|content| content.cache).is_some())
        .min_by_key(|land| land.center_x.abs())
        .expect("a cache");
    let cache = land
        .content()
        .and_then(|content| content.cache)
        .expect("cache");
    park(&mut world, cache.x, cache.y);
    // A shaft down to the cache.
    let top = land.surface_row(cache.x).expect("on the island");
    for y in top - 3..cache.y {
        assert!(
            world
                .terrain_mut()
                .set_kind(TilePosition { x: cache.x, y }, TileKind::Air)
        );
    }
    assert_eq!(
        world.terrain().tile(cache).map(|tile| tile.kind),
        Some(TileKind::Sky(SkyTile::Cache))
    );
    park(&mut world, cache.x, cache.y);
    world.player_mut(PLAYER).expect("rig").drill_strength = 4;
    let credits = world.player(PLAYER).expect("rig").credits;
    let mut opened = None;
    for _ in 0..600 {
        for event in step(
            &mut world,
            &TickInputs::held(PLAYER, ControlState::new(0.0, false, true)),
        )
        .events
        {
            if let GameplayEventKind::SkyCacheOpened { credits, tile, .. } = event.kind {
                opened = Some((credits, tile));
            }
        }
        if opened.is_some() {
            break;
        }
    }
    let (paid, tile) = opened.expect("the cache opened");
    assert_eq!(tile, SkyTile::Cache);
    assert!(paid > 0);
    assert_eq!(world.player(PLAYER).expect("rig").credits, credits + paid);
    // The skystone spilled (floating, or already in the bay).
    for _ in 0..60 {
        let _ = step(&mut world, &idle());
    }
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        rig.cargo_bay.mineral(MineralKind::Skystone) > 0
            || world.entities().count_of(EntityKind::Drop) > 0
    );
}

#[test]
fn blasts_spare_sky_caches_so_their_loot_is_never_lost() {
    // Like cave caches, sky caches shrug off blasts (a bomb has nobody to pay); drilling them
    // or bringing their island down spills the loot.
    let land = low_isles()
        .into_iter()
        .find(|land| {
            land.content()
                .is_some_and(|content| content.cache.is_some())
        })
        .expect("an isle with a cache");
    let cache = land
        .content()
        .and_then(|content| content.cache)
        .expect("cache");
    let mut world = world();
    park(&mut world, cache.x, cache.y - 12);
    assert_eq!(
        world.terrain().tile(cache).map(|tile| tile.kind),
        Some(TileKind::Sky(SkyTile::Cache))
    );
    let _ = world.terrain_mut().blast_radius(cache, 3);
    assert_eq!(
        world.terrain().tile(cache).map(|tile| tile.kind),
        Some(TileKind::Sky(SkyTile::Cache)),
        "the cache survives the blast"
    );
}

#[test]
fn a_buoy_refills_a_low_tank_once_for_a_price() {
    let mut world = world();
    let (_, buoy) = nearest_buoy();
    park(&mut world, buoy.pad.x, buoy.pad.y);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.fuel_tank_level = 3;
        rig.fuel_capacity = fuel_capacity_for_level(3);
        rig.fuel = 20.0;
        rig.credits = 1_000;
    }
    assert_eq!(world.buoy_at_rig(PLAYER), Some(buoy));
    let request = PlayerCommand::Service(SessionServiceRequest::SkyBuoy { isle: buoy.isle });
    let bought = step(&mut world, &idle().act(PLAYER, request.clone()));
    assert_eq!(outcome(&bought), PlayerScopedCommandOutcome::Applied);
    assert!(
        bought
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::BuoyRefuelled { .. }))
    );
    let rig = world.player(PLAYER).expect("rig");
    let capacity = fuel_capacity_for_level(3);
    assert_eq!(rig.credits, 1_000 - BUOY_PRICE);
    assert!(
        (rig.fuel - (20.0 + buoy_refill(capacity, 20.0))).abs() < 2.0,
        "{}",
        rig.fuel
    );
    assert!(world.buoy_used(PLAYER, buoy.isle));
    // Once per buoy.
    world.player_mut(PLAYER).expect("rig").fuel = 10.0;
    let again = step(&mut world, &idle().act(PLAYER, request.clone()));
    assert_ne!(outcome(&again), PlayerScopedCommandOutcome::Applied);
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        1_000 - BUOY_PRICE
    );
    // Off the pad, nothing.
    let mut fresh = self::world();
    park(&mut fresh, buoy.pad.x + 30, buoy.pad.y - 20);
    fresh.player_mut(PLAYER).expect("rig").credits = 1_000;
    let far = step(&mut fresh, &idle().act(PLAYER, request.clone()));
    assert_ne!(outcome(&far), PlayerScopedCommandOutcome::Applied);
    // A tank already over the buoy's fill level is refused (and keeps its credits).
    let mut full = self::world();
    park(&mut full, buoy.pad.x, buoy.pad.y);
    {
        let rig = full.player_mut(PLAYER).expect("rig");
        rig.fuel_tank_level = 12;
        rig.fuel_capacity = fuel_capacity_for_level(12);
        rig.fuel = rig.fuel_capacity * 0.6;
        rig.credits = 1_000;
    }
    let refused = step(&mut full, &idle().act(PLAYER, request));
    assert_ne!(outcome(&refused), PlayerScopedCommandOutcome::Applied);
    assert_eq!(full.player(PLAYER).expect("rig").credits, 1_000);
}

#[test]
fn drained_buoys_are_saved_with_the_player() {
    let mut world = world();
    let (_, buoy) = nearest_buoy();
    world
        .player_mut(PLAYER)
        .expect("rig")
        .villages
        .buoys
        .insert(buoy.isle);
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert!(loaded.buoy_used(PLAYER, buoy.isle));
    // Older journals load without the field.
    let old: village::VillageJournal = serde_json::from_str("{}").expect("old journal");
    assert!(old.buoys.is_empty());
}

/// Climbs `player`'s rig (at rest at `from` rows, `column`) holding thrust in still air until it
/// reaches `to` rows or runs dry; the fuel left (`None` if it never got there).
fn fuel_left_at(loadout: (u8, u8, u8), column: i32, from: i32, fuel: f32, to: i32) -> Option<f32> {
    environment::with_still_air(|| {
        let mut world = WorldState::new_game_with_settings(
            PLAYER,
            &WorldSettings::new(SEED),
            SimulationParams::new(60),
        );
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.fuel_tank_level = loadout.0;
        rig.fuel_capacity = fuel_capacity_for_level(loadout.0);
        rig.fuel = fuel;
        rig.engine_level = loadout.1;
        rig.hazard_gear.altitude_intake = loadout.2;
        rig.x = pixel(column);
        rig.y = -(from as f32) * TILE_SIZE;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        for step in 0_u64..2_000_000 {
            if step % 32 == 0 {
                let tile = world.player(PLAYER).expect("rig").tile_position(TILE_SIZE);
                let _ = world.terrain_mut().stream_around(&[tile]);
            }
            let _ = world.step_authoritative_movement(PLAYER, 0.0, true);
            let rig = world.player(PLAYER).expect("rig");
            if -rig.y / TILE_SIZE >= to as f32 {
                return Some(rig.fuel);
            }
            if rig.fuel <= 0.0 && rig.velocity_y >= 0.0 {
                return None;
            }
        }
        None
    })
}

/// A column with open sky all the way up (no land), for still-air climbs.
fn open_column() -> i32 {
    let settings = WorldSettings::new(SEED);
    (0..4_000)
        .map(|step| -600 - step * 3)
        .find(|x| {
            sky::lands_touching(
                &settings,
                &TileRect {
                    min: TilePosition {
                        x: x - 2,
                        y: -sky::SKY_CEILING_ROWS,
                    },
                    max: TilePosition { x: x + 2, y: -100 },
                },
            )
            .is_empty()
        })
        .expect("an open column")
}

#[test]
fn buoys_never_open_the_dead_air_gap() {
    // Every buoy hangs in the low isle band, so the best a rig can leave one with is
    // `buoy_fill_level` of fuel at the band's top. For every loadout that cannot cross the gap
    // on its own (and is anywhere near it), a rig that climbed to that altitude on a full tank
    // still has at least as much fuel: buoys never put a rig in a better state than the
    // climb table already measured, so `crosses_dead_air` (the Act II gate) stays honest.
    let column = open_column();
    let buoy_top = ARCHIPELAGO_BOTTOM_ROWS;
    for (t, tank) in TANK_TIERS.iter().enumerate() {
        for (e, engine) in ENGINE_TIERS.iter().enumerate() {
            for (i, intake) in INTAKE_TIERS.iter().enumerate() {
                let ceiling = CLIMB_CEILINGS[t][e][i];
                if !(DEAD_AIR_REACH_ROWS - 3_500..DEAD_AIR_REACH_ROWS).contains(&ceiling) {
                    continue;
                }
                let capacity = fuel_capacity_for_level(*tank);
                let left = fuel_left_at(
                    (*tank, *engine, *intake),
                    column,
                    crate::world::climb_tests::START_ALTITUDE_ROWS,
                    capacity,
                    buoy_top,
                )
                .expect("a near-gap rig climbs out of the low isles");
                assert!(
                    buoy_fill_level(capacity) <= left,
                    "tank {tank} engine {engine} intake {intake}: a buoy leaves {} fuel at the \
                     band's top, a full-tank climb {left}",
                    buoy_fill_level(capacity)
                );
            }
        }
    }
    // And measured outright for the best loadout that cannot cross: from the highest buoy
    // altitude with a buoy-filled tank it falls far short of the continents.
    let mut player = Player::new(0.0, 0.0);
    player.fuel_tank_level = 14;
    player.engine_level = 15;
    player.hazard_gear.altitude_intake = 4;
    assert!(!crosses_dead_air(&player));
    let reached = fuel_left_at(
        (14, 15, 4),
        column,
        buoy_top,
        buoy_fill_level(fuel_capacity_for_level(14)),
        DEAD_AIR_BOTTOM_ROWS,
    );
    assert!(
        reached.is_none(),
        "a buoy-filled tank reaches the dead-air gap"
    );
}

#[test]
fn sky_refuels_never_open_the_dead_air_gap() {
    // Sky towns (and island fuel stations) sell fuel high in the archipelago, just under the
    // gap. A full tank bought there would carry tier 10-12 tanks with intakes across, so a rig
    // that cannot cross on a full tank from the ground is only topped up to
    // `milestone::sky_refuel_level`. Measured with the real movement step from the highest a
    // sky town stands and from the middle of the archipelago: no such rig reaches the
    // continents' floor.
    let column = open_column();
    let top_town = DEAD_AIR_BOTTOM_ROWS - 24 - 34;
    let middle = i32::midpoint(ARCHIPELAGO_BOTTOM_ROWS, DEAD_AIR_BOTTOM_ROWS);
    let mut measured = 0;
    for (t, tank) in TANK_TIERS.iter().enumerate() {
        for (e, engine) in ENGINE_TIERS.iter().enumerate() {
            for (i, intake) in INTAKE_TIERS.iter().enumerate() {
                let ceiling = CLIMB_CEILINGS[t][e][i];
                let mut player = Player::new(0.0, 0.0);
                player.fuel_tank_level = *tank;
                player.fuel_capacity = fuel_capacity_for_level(*tank);
                player.engine_level = *engine;
                player.hazard_gear.altitude_intake = *intake;
                if crosses_dead_air(&player) {
                    assert!(
                        (crate::milestone::sky_refuel_level(&player, top_town)
                            - player.fuel_capacity)
                            .abs()
                            < f32::EPSILON,
                        "a rig that crosses anyway fills right up"
                    );
                    continue;
                }
                if ceiling < 2_500 {
                    continue;
                }
                for from in [top_town, middle] {
                    let level = crate::milestone::sky_refuel_level(&player, from);
                    assert!(level < player.fuel_capacity);
                    let reached = fuel_left_at(
                        (*tank, *engine, *intake),
                        column,
                        from,
                        level,
                        DEAD_AIR_REACH_ROWS,
                    );
                    assert!(
                        reached.is_none(),
                        "tank {tank} engine {engine} intake {intake}: topped up to {level} at \
                         {from} rows it crosses the dead-air gap"
                    );
                    measured += 1;
                }
            }
        }
    }
    assert!(measured > 20, "only {measured} loadouts measured");
    // The control: unrationed, a full tank bought at the top town does carry a tier 12 tank
    // with top intakes (which cannot cross from the ground) over the gap.
    let mut player = Player::new(0.0, 0.0);
    player.fuel_tank_level = 12;
    player.engine_level = 12;
    player.hazard_gear.altitude_intake = 6;
    assert!(!crosses_dead_air(&player));
    assert!(
        fuel_left_at(
            (12, 12, 6),
            column,
            top_town,
            fuel_capacity_for_level(12),
            DEAD_AIR_REACH_ROWS,
        )
        .is_some(),
        "the measurement sees a full tank cross"
    );
}

#[test]
fn a_sky_town_rations_fuel_for_rigs_that_cannot_cross() {
    let (found, building) = sky_town(SkyTownKind::CloudFarm);
    let refuel = found
        .offers(building)
        .iter()
        .position(|offer| matches!(offer.deal, VillageDeal::Refuel { .. }))
        .expect("the granary sells fuel");
    let mut world = world();
    let lot = found.buildings[building].clone();
    park(
        &mut world,
        lot.first_column + lot.width / 2,
        found.ground_row,
    );
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.fuel_tank_level = 10;
        rig.fuel_capacity = fuel_capacity_for_level(10);
        rig.engine_level = 12;
        rig.hazard_gear.altitude_intake = 6;
        rig.fuel = 1.0;
        rig.credits = 1_000_000;
    }
    let rig = world.player(PLAYER).expect("rig");
    let level = crate::milestone::sky_refuel_level(rig, (-rig.y / TILE_SIZE).floor() as i32);
    assert!(level < fuel_capacity_for_level(10) - 1.0, "level {level}");
    let output = step(
        &mut world,
        &idle().act(PLAYER, deal(&found, building, refuel)),
    );
    assert_eq!(outcome(&output), PlayerScopedCommandOutcome::Applied);
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        (rig.fuel - level).abs() <= 1.0,
        "topped up to {} (level {level})",
        rig.fuel
    );
}

/// The first sky town of `kind` near town and the index of its specialty building.
fn sky_town(kind: SkyTownKind) -> (Arc<Village>, usize) {
    let settings = settings();
    for bx in (0..12).flat_map(|bx| [bx, -bx - 1]) {
        for by in 0..12 {
            let Some(site) = sky::sky_town_site(&settings, bx, by) else {
                continue;
            };
            if site.kind != kind {
                continue;
            }
            let found = village::village(&settings, site.id).expect("built");
            let index = found
                .buildings
                .iter()
                .position(|building| building.kind == found.specialty.building())
                .expect("its specialty");
            return (found, index);
        }
    }
    panic!("no {kind:?}");
}

fn deal(found: &Village, building: usize, offer: usize) -> PlayerCommand {
    PlayerCommand::Service(SessionServiceRequest::Village {
        village: found.id,
        building: u8::try_from(building).expect("index"),
        offer: u8::try_from(offer).expect("index"),
        service: VillageService::of(found.buildings[building].kind).expect("service"),
    })
}

#[test]
fn an_airship_dock_fits_aether_thrusters_for_credits_and_sky_ore() {
    let mut world = world();
    let (found, dock) = sky_town(SkyTownKind::AirshipDock);
    let lot = found.buildings[dock].clone();
    park(
        &mut world,
        lot.first_column + lot.width / 2,
        found.ground_row,
    );
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.credits = 50_000;
        rig.cargo_bay.add(MineralKind::Skystone, 60);
        rig.cargo_bay.add(MineralKind::CloudSilver, 30);
    }
    assert_eq!(
        world
            .player_village_building(PLAYER)
            .map(|(v, b)| (v.id, b)),
        Some((found.id, dock))
    );
    let offers = found.offers(dock);
    let row = |tier: u8| {
        offers
            .iter()
            .position(|offer| {
                offer.deal
                    == VillageDeal::SkyCraft {
                        recipe: SkyRecipe::AetherThrusters,
                        tier,
                    }
            })
            .expect("the row")
    };
    // Tier 2 before tier 1: refused.
    let early = step(&mut world, &idle().act(PLAYER, deal(&found, dock, row(2))));
    assert_ne!(outcome(&early), PlayerScopedCommandOutcome::Applied);
    let crafted = step(&mut world, &idle().act(PLAYER, deal(&found, dock, row(1))));
    assert_eq!(outcome(&crafted), PlayerScopedCommandOutcome::Applied);
    assert!(crafted.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::VillageServiceUsed {
            service: VillageService::AirshipDock,
            ..
        }
    )));
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(rig.hazard_gear.aether_thrusters, 1);
    let cost = crate::skycraft::cost(SkyRecipe::AetherThrusters, 1);
    assert_eq!(
        rig.cargo_bay.mineral(MineralKind::Skystone),
        60 - cost.skystone
    );
    assert_eq!(rig.credits, 50_000 - cost.credits);
}

#[test]
fn every_sky_town_kind_has_its_counter_and_sky_offers() {
    for kind in SkyTownKind::ALL {
        let (found, index) = sky_town(kind);
        assert!(found.culture.is_sky(), "{kind:?}");
        let offers = found.offers(index);
        assert!(!offers.is_empty(), "{kind:?}");
        let crafts = offers
            .iter()
            .filter(|offer| matches!(offer.deal, VillageDeal::SkyCraft { .. }))
            .count();
        match kind {
            SkyTownKind::FloatingMarket => assert!(offers.iter().any(|offer| matches!(
                offer.deal,
                VillageDeal::SellCargo {
                    only: Some(MineralKind::Skystone | MineralKind::CloudSilver),
                    ..
                }
            ))),
            _ => assert!(crafts > 0, "{kind:?} crafts"),
        }
        // Residents with sky occupations.
        assert!(
            found
                .population
                .characters
                .iter()
                .any(|who| crate::people::Role::SKY.contains(&who.role)),
            "{kind:?} has sky folk"
        );
    }
}

/// The crumbling low isle nearest town with another crumbling isle close by.
fn crumbling_pair() -> (SkyLand, SkyLand) {
    let lands: Vec<SkyLand> = low_isles()
        .into_iter()
        .filter(|land| {
            land.content()
                .and_then(|content| content.keystone)
                .is_some()
        })
        .collect();
    let mut pairs: Vec<(SkyLand, SkyLand)> = Vec::new();
    for a in &lands {
        for b in &lands {
            if a.id != b.id
                && a.bounds()
                    .expanded(crate::world::sky_isles::CHAIN_RADIUS - 4)
                    .intersects(&b.bounds())
            {
                pairs.push((*a, *b));
            }
        }
    }
    pairs.sort_by_key(|(a, _)| a.center_x.abs());
    pairs
        .first()
        .copied()
        .expect("two crumbling isles side by side")
}

fn island_tiles_left(world: &WorldState, land: &SkyLand) -> usize {
    land.bounds()
        .positions()
        .filter(|position| land.tile_at(position.x, position.y).is_some())
        .filter(|position| {
            world
                .terrain()
                .tile(*position)
                .is_some_and(|tile| tile.kind != TileKind::Air)
        })
        .count()
}

#[test]
fn a_gravity_bomb_on_a_keystone_brings_down_a_cluster() {
    let mut world = world();
    let (first, second) = crumbling_pair();
    let keystone = first
        .content()
        .and_then(|content| content.keystone)
        .expect("keystone");
    // Watch from a safe distance above.
    park(&mut world, keystone.x + 40, keystone.y - 30);
    world.player_mut(PLAYER).expect("rig").fuel = 5_000.0;
    let before = (
        island_tiles_left(&world, &first),
        island_tiles_left(&world, &second),
    );
    assert!(before.0 > 20 && before.1 > 20);
    let now = world.simulation_tick().get();
    world.insert_charge(PlacedBomb {
        id: 7,
        x: pixel(keystone.x),
        y: pixel(keystone.y),
        previous_x: pixel(keystone.x),
        previous_y: pixel(keystone.y),
        detonation_tick: SimulationTick::new(now + 2),
        timer_seconds: 0.03,
        ..PlacedBomb::default()
    });
    let mut collapses = 0;
    let hover = TickInputs::held(PLAYER, ControlState::new(0.0, true, false));
    for tick in 0..60 * 12 {
        let inputs = if tick % 3 == 2 { idle() } else { hover.clone() };
        for event in step(&mut world, &inputs).events {
            if matches!(event.kind, GameplayEventKind::IsleCollapsing { .. }) {
                collapses += 1;
            }
        }
    }
    assert!(
        collapses >= 2,
        "the keystone broke and the shock cracked the neighbour"
    );
    assert_eq!(
        island_tiles_left(&world, &first),
        0,
        "the island fell apart"
    );
    assert_eq!(
        island_tiles_left(&world, &second),
        0,
        "and took its neighbour with it"
    );
}

#[test]
fn islands_without_a_broken_keystone_stand() {
    let mut world = world();
    let (first, _) = crumbling_pair();
    let keystone = first
        .content()
        .and_then(|content| content.keystone)
        .expect("keystone");
    park(&mut world, keystone.x + 40, keystone.y - 30);
    world.player_mut(PLAYER).expect("rig").fuel = 5_000.0;
    let before = island_tiles_left(&world, &first);
    let hover = TickInputs::held(PLAYER, ControlState::new(0.0, true, false));
    for tick in 0..120 {
        let inputs = if tick % 3 == 2 { idle() } else { hover.clone() };
        let _ = step(&mut world, &inputs);
    }
    assert_eq!(island_tiles_left(&world, &first), before);
}

#[test]
fn a_jet_stream_carries_a_rig_along_the_chain() {
    let settings = settings();
    let stream = sky::jet_streams_touching(
        &settings,
        &TileRect {
            min: TilePosition {
                x: -3_000,
                y: sky::row_of(DEAD_AIR_BOTTOM_ROWS),
            },
            max: TilePosition {
                x: 3_000,
                y: sky::row_of(ARCHIPELAGO_BOTTOM_ROWS),
            },
        },
    )
    .into_iter()
    .find(|stream| stream.rect.max.x - stream.rect.min.x > 90)
    .expect("a long jet stream");
    let mut world = world();
    let start = if stream.velocity > 0 {
        stream.rect.min.x + 20
    } else {
        stream.rect.max.x - 20
    };
    let row = i32::midpoint(stream.rect.min.y, stream.rect.max.y);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.x = pixel(start);
        rig.y = pixel(row);
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.fuel = 5_000.0;
        rig.fuel_capacity = 5_000.0;
        rig.engine_level = 10;
    }
    world.stream_terrain_near_players();
    let mut caught = false;
    let x0 = world.player(PLAYER).expect("rig").x;
    for _ in 0..120 {
        // A perfect hover: the rig stays level in the ribbon.
        {
            let rig = world.player_mut(PLAYER).expect("rig");
            rig.y = pixel(row);
            rig.velocity_y = 0.0;
        }
        caught |= step(&mut world, &idle())
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::JetStreamCaught { .. }));
    }
    let moved = (world.player(PLAYER).expect("rig").x - x0) / TILE_SIZE;
    assert!(caught || moved.abs() > 0.0);
    assert!(
        moved * stream.velocity.signum() as f32 > 15.0,
        "carried {moved} tiles along a {} px/s stream",
        stream.velocity
    );
}

#[test]
fn sky_isle_play_is_deterministic_and_replicates() {
    let run = || {
        let mut world = world();
        let (_, buoy) = nearest_buoy();
        park(&mut world, buoy.pad.x, buoy.pad.y);
        world.player_mut(PLAYER).expect("rig").fuel = 30.0;
        world.player_mut(PLAYER).expect("rig").credits = 900;
        let request = PlayerCommand::Service(SessionServiceRequest::SkyBuoy { isle: buoy.isle });
        let _ = step(&mut world, &idle().act(PLAYER, request));
        for _ in 0..30 {
            let _ = step(&mut world, &idle());
        }
        world
    };
    let (a, b) = (run(), run());
    assert_eq!(
        serde_json::to_string(&a).expect("a"),
        serde_json::to_string(&b).expect("b")
    );
    // The owner's keyframe carries the drained buoy (private loadout).
    let keyframe = a.redacted_for(Some(PLAYER));
    let json = serde_json::to_string(&keyframe).expect("keyframe");
    let back: WorldState = serde_json::from_str(&json).expect("back");
    let (_, buoy) = nearest_buoy();
    assert!(back.buoy_used(PLAYER, buoy.isle));
}

#[test]
fn sky_towns_are_safe_in_the_hostile_sky() {
    // Integration seam (T20 x T21): the archipelago is hostile sky on V9 worlds, but its sky
    // towns keep the village rule, so nothing hunts in or around them.
    let world = world();
    let (found, _) = sky_town(SkyTownKind::AirshipDock);
    let street = TilePosition {
        x: found.center_column(),
        y: found.ground_row - 2,
    };
    assert!(world.hostile_sky(street), "the archipelago is hostile sky");
    assert!(world.sky_safe_zone(street));
    assert!(world.enemy_safe_zone(street));
    let open_air = TilePosition {
        x: found.last_column + 200,
        y: found.ground_row - 2,
    };
    assert!(world.hostile_sky(open_air));
    assert!(
        !world.enemy_safe_zone(open_air),
        "open sky beyond the town is not safe"
    );
}
