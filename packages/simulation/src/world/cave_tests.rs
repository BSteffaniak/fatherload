//! Cave ecosystems in play (WP-CAVES): discovery, stalactites, caches, critters, cave water and
//! bouncy mushrooms.

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WorldState,
    cave::{CaveKind, CaveTile},
    entity::{EntityKind, EntityState},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{FeatureKind, TileRect, WorldFeature, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(1);

fn world() -> WorldState {
    WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(0x0CA7_E002),
        SimulationParams::new(60),
    )
}

fn find_cave(world: &WorldState, kind: CaveKind) -> WorldFeature {
    let terrain = world.terrain();
    terrain
        .features_in(
            &TileRect {
                min: TilePosition { x: -3_000, y: 0 },
                max: TilePosition {
                    x: 3_000,
                    y: terrain.height() - 1,
                },
            },
            Some(&[FeatureKind::Cave]),
        )
        .into_iter()
        .find(|feature| feature.detail.cave() == Some(kind))
        .expect("a cave of the kind near spawn")
}

#[allow(
    clippy::cast_precision_loss,
    reason = "tile coordinates within the world"
)]
fn put_rig(world: &mut WorldState, tile: TilePosition) {
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.x = (tile.x as f32 + 0.5) * TILE_SIZE;
    rig.y = (tile.y as f32 + 0.5) * TILE_SIZE;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.x_remainder = 0.0;
    rig.fuel = 1_000.0;
    rig.fuel_capacity = 1_000.0;
}

fn tick(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::held(PLAYER, controls);
    let now = world.simulation_tick();
    world
        .simulate_tick(now, &inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn idle() -> ControlState {
    ControlState::new(0.0, false, false)
}

#[test]
fn entering_a_cave_records_it_once() {
    let mut world = world();
    let cave = find_cave(&world, CaveKind::GrandCavern);
    put_rig(&mut world, cave.center);
    let events = tick(&mut world, idle());
    assert!(events.contains(&GameplayEventKind::CaveDiscovered {
        player_id: PLAYER,
        feature: cave.id,
        cave: CaveKind::GrandCavern,
    }));
    assert_eq!(
        world
            .player(PLAYER)
            .expect("rig")
            .journal
            .caves
            .get(&cave.id),
        Some(&CaveKind::GrandCavern)
    );
    let again = tick(&mut world, idle());
    assert!(
        !again
            .iter()
            .any(|event| matches!(event, GameplayEventKind::CaveDiscovered { .. }))
    );
}

/// An open pocket of `width` by `height` tiles at `origin` (top-left) with a stone floor.
fn pocket(world: &mut WorldState, origin: TilePosition, width: i32, height: i32) {
    for y in origin.y - 1..=origin.y + height {
        for x in origin.x - 1..=origin.x + width {
            let edge =
                x < origin.x || x >= origin.x + width || y < origin.y || y >= origin.y + height;
            let kind = if edge { TileKind::Stone } else { TileKind::Air };
            assert!(world.terrain_mut().set_kind(TilePosition { x, y }, kind));
        }
    }
}

#[test]
fn a_stalactite_over_a_rig_falls() {
    let mut world = world();
    let origin = TilePosition { x: 400, y: 120 };
    pocket(&mut world, origin, 6, 7);
    // A loose one (most stalactites hold for good, see `stalactite_is_loose`).
    let seed = world.terrain().seed();
    let hanging = (origin.x..origin.x + 6)
        .map(|x| TilePosition { x, y: 120 })
        .find(|&tile| super::caves::stalactite_is_loose(seed, tile))
        .expect("a loose stalactite tile along the ceiling");
    world
        .terrain_mut()
        .set_kind(hanging, TileKind::Cave(CaveTile::Stalactite));
    put_rig(
        &mut world,
        TilePosition {
            x: hanging.x,
            y: 126,
        },
    );
    let boulders = world.entities().count_of(EntityKind::Boulder);
    let events = tick(&mut world, idle());
    assert!(events.contains(&GameplayEventKind::StalactiteFell {
        x: hanging.x,
        y: 120
    }));
    assert_eq!(
        world.terrain().tile(hanging).map(|tile| tile.kind),
        Some(TileKind::Air)
    );
    assert_eq!(world.entities().count_of(EntityKind::Boulder), boulders + 1);
    // It lands on the rig within a couple of seconds.
    let mut hit = false;
    for _ in 0..180 {
        hit |= tick(&mut world, idle())
            .contains(&GameplayEventKind::PlayerDamaged { player_id: PLAYER });
    }
    assert!(hit, "the falling stalactite hit the rig below");
}

#[test]
fn a_stalactite_behind_rock_stays_put() {
    let mut world = world();
    let origin = TilePosition { x: 400, y: 120 };
    pocket(&mut world, origin, 6, 7);
    let hanging = TilePosition { x: 403, y: 120 };
    world
        .terrain_mut()
        .set_kind(hanging, TileKind::Cave(CaveTile::Stalactite));
    world
        .terrain_mut()
        .set_kind(TilePosition { x: 403, y: 123 }, TileKind::Stone);
    put_rig(&mut world, TilePosition { x: 403, y: 126 });
    let events = tick(&mut world, idle());
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::StalactiteFell { .. }))
    );
}

#[test]
fn drilling_a_cache_pays_credits_and_spills_loot() {
    let mut world = world();
    let origin = TilePosition { x: 400, y: 120 };
    pocket(&mut world, origin, 4, 3);
    let cache = TilePosition { x: 401, y: 123 };
    world
        .terrain_mut()
        .set_kind(cache, TileKind::Cave(CaveTile::TreasureCache));
    put_rig(&mut world, TilePosition { x: 401, y: 122 });
    world.player_mut(PLAYER).expect("rig").drill_strength = 4;
    let credits = world.player(PLAYER).expect("rig").credits;
    // WP-DROPS: the spilled loot is either still floating or already in the cargo bay (the
    // rig's magnet picks it up within a few ticks).
    let loot = |world: &WorldState| {
        world.entities().count_of(EntityKind::Drop)
            + world.player(PLAYER).expect("rig").cargo_bay.total() as usize
    };
    let before = loot(&world);
    let mut opened = None;
    for _ in 0..600 {
        for event in tick(&mut world, ControlState::new(0.0, false, true)) {
            if let GameplayEventKind::CaveCacheOpened {
                credits,
                tile,
                furniture,
                ..
            } = event
            {
                opened = Some((credits, tile, furniture));
            }
        }
        if opened.is_some() {
            break;
        }
    }
    let (paid, tile, furniture) = opened.expect("the cache opened");
    assert_eq!(tile, CaveTile::TreasureCache);
    assert!(paid > 0);
    assert_eq!(world.player(PLAYER).expect("rig").credits, credits + paid);
    assert!(loot(&world) > before, "the cache's loot spilled");
    // WP-HOME: an antique piece, when the cache held one, went into the home stock.
    assert_eq!(
        furniture,
        crate::home::decor::cache_find(world.terrain().seed(), cache.x, cache.y, false)
    );
    if let Some(piece) = furniture {
        assert_eq!(world.homes().furniture_count(PLAYER, piece), 1);
    }
}

#[test]
fn caches_hold_antique_furniture_now_and_then() {
    use crate::home::{Furniture, decor::cache_find};
    let mut treasure = 0;
    for x in 0..300 {
        let relic = cache_find(7, x, 40, true).expect("relic caches always hold a piece");
        assert!(Furniture::CACHE_FINDS[3..].contains(&relic));
        assert!(!Furniture::CATALOGUE.contains(&relic));
        assert_eq!(cache_find(7, x, 40, false), cache_find(7, x, 40, false));
        treasure += usize::from(cache_find(7, x, 40, false).is_some());
    }
    // About one treasure cache in three.
    assert!((60..=140).contains(&treasure), "{treasure} of 300");
}

fn critters_of(world: &WorldState, cave: &WorldFeature) -> usize {
    world
        .entities()
        .of_kind(EntityKind::Critter)
        .filter(|entity| matches!(entity.state, EntityState::Critter(c) if c.cave == cave.id))
        .count()
}

#[test]
fn caves_keep_critters_near_rigs_and_send_them_away_after() {
    let mut world = world();
    let cave = find_cave(&world, CaveKind::FungalGrotto);
    put_rig(&mut world, cave.center);
    for _ in 0..40 {
        let _ = tick(&mut world, idle());
    }
    let count = critters_of(&world, &cave);
    assert!((3..=8).contains(&count), "{count} critters");
    // Critters are harmless and still: their records never change after spawning.
    let revisions: Vec<u64> = world
        .entities()
        .of_kind(EntityKind::Critter)
        .map(|entity| entity.revision)
        .collect();
    for _ in 0..40 {
        let _ = tick(&mut world, idle());
    }
    assert_eq!(critters_of(&world, &cave), count, "no duplicate slots");
    let later: Vec<u64> = world
        .entities()
        .of_kind(EntityKind::Critter)
        .map(|entity| entity.revision)
        .collect();
    assert_eq!(revisions, later);
    // Far away, they leave.
    put_rig(
        &mut world,
        TilePosition {
            x: cave.center.x + 600,
            y: 6,
        },
    );
    for _ in 0..40 {
        let _ = tick(&mut world, idle());
    }
    assert_eq!(critters_of(&world, &cave), 0);
}

#[test]
fn cave_water_slows_a_falling_rig() {
    let mut world = world();
    let cave = find_cave(&world, CaveKind::FloodedCave);
    let water = cave
        .bounds
        .positions()
        .filter(|tile| world.terrain().water_at(*tile))
        .find(|tile| {
            (1..=3).all(|dy| {
                world.terrain().water_at(TilePosition {
                    x: tile.x,
                    y: tile.y + dy,
                })
            })
        })
        .expect("deep water");
    put_rig(&mut world, water);
    for _ in 0..20 {
        let _ = tick(&mut world, idle());
    }
    let in_water = world.player(PLAYER).expect("rig").velocity_y;
    let mut dry = self::world();
    pocket(&mut dry, TilePosition { x: 400, y: 100 }, 3, 12);
    put_rig(&mut dry, TilePosition { x: 401, y: 101 });
    for _ in 0..20 {
        let _ = tick(&mut dry, idle());
    }
    let in_air = dry.player(PLAYER).expect("rig").velocity_y;
    assert!(
        in_water > 0.0 && in_water < in_air * 0.5,
        "{in_water} vs {in_air}"
    );
}

#[test]
fn a_mushroom_cap_bounces_a_falling_rig() {
    let mut world = world();
    let origin = TilePosition { x: 400, y: 100 };
    pocket(&mut world, origin, 3, 12);
    for x in 400..403 {
        world.terrain_mut().set_kind(
            TilePosition { x, y: 111 },
            TileKind::Cave(CaveTile::MushroomCap),
        );
    }
    put_rig(&mut world, TilePosition { x: 401, y: 101 });
    let mut bounced = false;
    for _ in 0..120 {
        let _ = tick(&mut world, idle());
        if world.player(PLAYER).expect("rig").velocity_y < -200.0 {
            bounced = true;
            break;
        }
    }
    assert!(bounced, "the cap sprang the rig back up");
    let hull = world.player(PLAYER).expect("rig").hull;
    assert!(hull > 0.0);
}

/// Two seats in one cave (split-screen or online): each rig finds the cave for its own journal,
/// the cave keeps one population of critters (not one per rig), and it stays while either rig
/// is near.
#[test]
fn two_rigs_share_a_cave_and_its_critters() {
    const OTHER: PlayerId = PlayerId::new(2);
    let mut world = world();
    world.add_player(OTHER);
    let cave = find_cave(&world, CaveKind::FungalGrotto);
    put_rig(&mut world, cave.center);
    let (x, y) = {
        let rig = world.player(PLAYER).expect("rig");
        (rig.x, rig.y)
    };
    let other = world.player_mut(OTHER).expect("second rig");
    other.x = x;
    other.y = y;
    other.velocity_x = 0.0;
    other.velocity_y = 0.0;
    other.x_remainder = 0.0;
    other.fuel = 1_000.0;
    other.fuel_capacity = 1_000.0;
    let both = || TickInputs::held(PLAYER, idle()).hold(OTHER, idle());
    let mut found = Vec::new();
    for _ in 0..40 {
        let now = world.simulation_tick();
        found.extend(
            world
                .simulate_tick(now, &both())
                .events
                .into_iter()
                .filter_map(|event| match event.kind {
                    GameplayEventKind::CaveDiscovered { player_id, .. } => Some(player_id),
                    _ => None,
                }),
        );
    }
    found.sort();
    assert_eq!(found, vec![PLAYER, OTHER], "each rig finds the cave once");
    for player in [PLAYER, OTHER] {
        assert!(
            world
                .player(player)
                .expect("rig")
                .journal
                .caves
                .contains_key(&cave.id)
        );
    }
    let count = critters_of(&world, &cave);
    assert!((3..=8).contains(&count), "{count} critters for two rigs");
    // One rig leaves; the other keeps the cave alive.
    put_rig(
        &mut world,
        TilePosition {
            x: cave.center.x + 600,
            y: 6,
        },
    );
    for _ in 0..40 {
        let now = world.simulation_tick();
        let _ = world.simulate_tick(now, &both());
    }
    assert_eq!(critters_of(&world, &cave), count);
}
