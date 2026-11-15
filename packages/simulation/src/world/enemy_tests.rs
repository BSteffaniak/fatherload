//! Enemies inside the world tick: spawn rules (caves, darkness, caps, safe zones, out of view,
//! difficulty), AI (chase, telegraphed attacks, i-frames and knockback, leash, mimics, burrowers
//! drawn by drilling, flares luring bats, nests), damage (resistances, explosives, EMP, shields,
//! loot, lairs), god mode, determinism, saves and replication.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Serialize, de::DeserializeOwned};

use super::{
    WorldImport,
    enemies::{EMP_RADIUS, RIG_GUARD_MILLIS},
};
use crate::{
    ControlState, ExplosiveKind, GameplayEventKind, PlacedBomb, PlayerCommand, PlayerId,
    SectionKey, SimulationTick, TickInputs, TickOutput, WorldState,
    enemy::{
        Damage, DamageKind, DamageOutcome, DamageSource, EnemyDifficulty, EnemyKind, EnemyMode,
        EnemyState,
    },
    entity::{EntityDelta, EntityId, EntityKind, EntityState},
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);
/// The arena: a big open cavern far below the surface, east of the town.
const LEFT: i32 = 230;
const RIGHT: i32 = 330;
const TOP: i32 = 200;
const FLOOR: i32 = 232;

fn wire<T: Serialize + DeserializeOwned>(value: &T) -> T {
    let bytes = postcard::to_stdvec(value).expect("encode");
    postcard::from_bytes(&bytes).expect("decode")
}

fn fill(
    world: &mut WorldState,
    xs: std::ops::Range<i32>,
    ys: std::ops::Range<i32>,
    kind: TileKind,
) {
    for y in ys {
        for x in xs.clone() {
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
}

/// A cavern (open from `TOP` to the floor at `FLOOR`) inside stone, the rig parked in the
/// middle of its floor.
fn cavern() -> WorldState {
    let mut world = WorldState::from_import(&WorldImport::new_game(PLAYER));
    fill(
        &mut world,
        LEFT - 6..RIGHT + 6,
        TOP - 6..FLOOR + 6,
        TileKind::Stone,
    );
    fill(&mut world, LEFT..RIGHT, TOP..FLOOR, TileKind::Air);
    park_on_floor(&mut world, PLAYER, 280.5);
    world
}

fn px(tile: f32) -> f32 {
    tile * TILE_SIZE
}

#[allow(clippy::cast_precision_loss, reason = "small tile indices")]
fn floor_y(radius: f32) -> f32 {
    (FLOOR as f32).mul_add(TILE_SIZE, -radius) - 1.0
}

fn park(world: &mut WorldState, player: PlayerId, tile_x: f32, tile_y: f32) {
    let rig = world.player_mut(player).expect("player");
    rig.x = px(tile_x);
    rig.y = px(tile_y);
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

/// Parks a rig resting on the cavern floor at column `tile_x` (in drill contact).
fn park_on_floor(world: &mut WorldState, player: PlayerId, tile_x: f32) {
    park(world, player, tile_x, 0.0);
    world.player_mut(player).expect("player").y = floor_y(10.0);
}

fn tick(world: &mut WorldState) -> Vec<GameplayEventKind> {
    tick_with(world, &TickInputs::default())
}

fn tick_with(world: &mut WorldState, inputs: &TickInputs) -> Vec<GameplayEventKind> {
    world
        .simulate_tick(world.simulation_tick(), inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn run(world: &mut WorldState, ticks: u32) -> Vec<GameplayEventKind> {
    (0..ticks).flat_map(|_| tick(world)).collect()
}

fn enemy(world: &WorldState, id: EntityId) -> Option<(crate::entity::Body, EnemyState)> {
    world
        .entities()
        .get(id)
        .and_then(|entity| match entity.state {
            EntityState::Enemy(state) => Some((entity.body, state)),
            _ => None,
        })
}

fn hull(world: &WorldState, player: PlayerId) -> f32 {
    world.player(player).expect("player").hull
}

fn struck(events: &[GameplayEventKind]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, GameplayEventKind::RigStruck { .. }))
        .count()
}

/// Ticks inside `apply` with an output collector, as weapons will call the damage API.
fn hit(
    world: &mut WorldState,
    id: EntityId,
    damage: Damage,
) -> (DamageOutcome, Vec<GameplayEventKind>) {
    let mut output = TickOutput::default();
    let outcome = world.damage_entity(id, damage, &mut output);
    (
        outcome,
        output.events.into_iter().map(|event| event.kind).collect(),
    )
}

fn no_spawns(world: &mut WorldState) {
    // Hold the director off without Peaceful (which would clear scripted enemies): every
    // chunk of the arena is cooling.
    for cy in (TOP - 40) / 16..=(FLOOR + 40) / 16 {
        for cx in (LEFT - 60) / 16..=(RIGHT + 60) / 16 {
            world.enemies.cool_chunk(
                crate::terrain::ChunkPosition { x: cx, y: cy },
                SimulationTick::new(u64::MAX),
            );
        }
    }
}

#[test]
fn roaming_enemies_spawn_in_the_dark_out_of_view_and_within_caps() {
    let mut world = cavern();
    let rig = world.player(PLAYER).expect("rig").tile_position(TILE_SIZE);
    let mut seen = BTreeSet::new();
    for _ in 0..60 * 90 {
        let _ = tick(&mut world);
        let rig_now = world.player(PLAYER).expect("rig").tile_position(TILE_SIZE);
        for (entity, state) in world.enemies() {
            if !seen.insert(entity.id) {
                continue;
            }
            let tile = TilePosition {
                x: state.home.0,
                y: state.home.1,
            };
            assert!(
                (tile.x - rig_now.x).abs() > 20 || (tile.y - rig_now.y).abs() > 11,
                "{:?} spawned in view at {tile:?} (rig {rig_now:?})",
                state.kind
            );
            assert!(!world.enemy_safe_zone(tile));
            assert!(tile.y - world.terrain().surface_row(tile.x) >= 10);
        }
        let (cx, cy) = crate::enemy::tile_centre(rig.x, rig.y);
        let near = world.enemies_near(cx, cy, px(32.0));
        assert!(near.len() <= 13, "local cap: {}", near.len());
    }
    assert!(
        seen.len() >= 3,
        "a cavern this big attracts enemies: {}",
        seen.len()
    );
    world.entities().assert_indices_consistent();
}

#[test]
fn the_surface_and_town_never_spawn_and_peaceful_clears_everything() {
    let mut world = WorldState::from_import(&WorldImport::new_game(PLAYER));
    assert!(world.enemy_safe_zone(TilePosition { x: 97, y: 4 }));
    assert!(
        world.enemy_safe_zone(TilePosition { x: 97, y: 14 }),
        "town shafts"
    );
    assert!(!world.enemy_safe_zone(TilePosition { x: 97, y: 60 }));
    let _ = run(&mut world, 60 * 60);
    assert_eq!(
        world.entities().count_of(EntityKind::Enemy),
        0,
        "the town is safe"
    );

    let mut world = cavern();
    let bat = world.spawn_enemy(EnemyKind::CaveBat, px(260.0), px(215.0));
    assert!(world.entities().contains(bat));
    let events = tick_with(
        &mut world,
        &TickInputs::default().act(
            PLAYER,
            PlayerCommand::SetEnemyDifficulty(EnemyDifficulty::Peaceful),
        ),
    );
    assert!(events.contains(&GameplayEventKind::EnemyDifficultyChanged {
        player_id: PLAYER,
        difficulty: EnemyDifficulty::Peaceful
    }));
    assert_eq!(world.entities().count_of(EntityKind::Enemy), 0);
    let _ = run(&mut world, 60 * 60);
    assert_eq!(
        world.entities().count_of(EntityKind::Enemy),
        0,
        "peaceful never spawns"
    );
    assert_eq!(world.enemy_difficulty(), EnemyDifficulty::Peaceful);
}

#[test]
fn hard_worlds_spawn_more_than_normal_ones() {
    let count = |difficulty| {
        let mut world = cavern();
        world.set_enemy_difficulty(difficulty);
        let mut seen = BTreeSet::new();
        for index in 0..60 * 180 {
            let _ = tick(&mut world);
            seen.extend(world.enemies().map(|(entity, _)| entity.id));
            if index % 600 == 599 {
                // The crew clears the cavern every ten seconds.
                let ids: BTreeSet<EntityId> =
                    world.enemies().map(|(entity, _)| entity.id).collect();
                let _ = world.entities_mut().despawn_many(&ids);
            }
        }
        seen.len()
    };
    let normal = count(EnemyDifficulty::Normal);
    let hard = count(EnemyDifficulty::Hard);
    assert!(hard > normal, "hard {hard} vs normal {normal}");
}

#[test]
fn a_bat_chases_bites_with_iframes_and_knocks_the_rig_back() {
    let mut world = cavern();
    no_spawns(&mut world);
    let bat = world.spawn_enemy(EnemyKind::CaveBat, px(276.0), px(226.0));
    let start = hull(&world, PLAYER);
    let mut events = Vec::new();
    let mut first_hit = None;
    for index in 0..240 {
        let tick_events = tick(&mut world);
        if first_hit.is_none() && struck(&tick_events) > 0 {
            first_hit = Some(index);
            let rig = world.player(PLAYER).expect("rig");
            assert!(
                rig.velocity_x.abs() + rig.velocity_y.abs() > 10.0,
                "knocked back"
            );
        }
        events.extend(tick_events);
    }
    let first_hit = first_hit.expect("the bat bit the rig");
    assert!(
        events.contains(&GameplayEventKind::EnemyAlerted {
            entity: bat.get(),
            kind: EnemyKind::CaveBat,
            x: events
                .iter()
                .find_map(|event| match event {
                    GameplayEventKind::EnemyAlerted { x, .. } => Some(*x),
                    _ => None,
                })
                .expect("alerted"),
            y: events
                .iter()
                .find_map(|event| match event {
                    GameplayEventKind::EnemyAlerted { y, .. } => Some(*y),
                    _ => None,
                })
                .expect("alerted"),
        })
    );
    assert!(hull(&world, PLAYER) < start, "the bite hurt");
    // I-frames: hits are at least the guard window apart.
    let guard_ticks = world.timing().ticks_for_millis(RIG_GUARD_MILLIS);
    let mut world_replay = cavern();
    no_spawns(&mut world_replay);
    let _bat = world_replay.spawn_enemy(EnemyKind::CaveBat, px(276.0), px(226.0));
    let mut last = None;
    for index in 0..600_u64 {
        if struck(&tick(&mut world_replay)) > 0 {
            if let Some(previous) = last {
                assert!(
                    index - previous >= guard_ticks,
                    "hits {previous} and {index}"
                );
            }
            last = Some(index);
        }
    }
    assert!(first_hit < 240);
}

#[test]
fn a_guarded_rig_does_not_shield_a_crewmate_touching_the_same_bat() {
    let mut world = cavern();
    no_spawns(&mut world);
    world.add_player(OTHER);
    park_on_floor(&mut world, OTHER, 280.5);
    // The first rig (lower id, found first) is inside long i-frames.
    world
        .enemies
        .rig_guard
        .insert(PLAYER, SimulationTick::new(u64::MAX));
    let _bat = world.spawn_enemy(EnemyKind::CaveBat, px(277.0), px(226.0));
    let events = run(&mut world, 360);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::RigStruck { player_id, .. } if *player_id == OTHER)),
        "the crewmate was bitten"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::RigStruck { player_id, .. } if *player_id == PLAYER)),
        "the guarded rig was not"
    );
}

#[test]
fn god_mode_rigs_take_no_damage_from_anything() {
    let mut world = cavern();
    no_spawns(&mut world);
    world.enable_test_mode();
    for kind in [
        EnemyKind::CaveBat,
        EnemyKind::MiningDrone,
        EnemyKind::AcidSpitter,
    ] {
        let _ = world.spawn_enemy(kind, px(275.0), px(226.0));
    }
    let start = hull(&world, PLAYER);
    let events = run(&mut world, 600);
    assert_eq!(struck(&events), 0);
    assert!((hull(&world, PLAYER) - start).abs() < f32::EPSILON);
}

#[test]
fn a_spitter_telegraphs_then_lobs_acid_that_hits_the_rig() {
    let mut world = cavern();
    no_spawns(&mut world);
    let spitter = world.spawn_enemy(
        EnemyKind::AcidSpitter,
        px(274.0),
        floor_y(EnemyKind::AcidSpitter.stats().radius),
    );
    let start = hull(&world, PLAYER);
    let events = run(&mut world, 360);
    let windup = events
        .iter()
        .position(|event| matches!(event, GameplayEventKind::EnemyWindup { entity, .. } if *entity == spitter.get()))
        .expect("telegraphed");
    let fired = events
        .iter()
        .position(|event| matches!(event, GameplayEventKind::EnemyFired { entity, .. } if *entity == spitter.get()))
        .expect("fired");
    assert!(windup < fired, "the wind-up comes first");
    assert!(struck(&events) > 0, "acid landed");
    assert!(hull(&world, PLAYER) < start);
}

#[test]
fn a_mimic_sleeps_until_a_rig_comes_close() {
    let mut world = cavern();
    no_spawns(&mut world);
    let radius = EnemyKind::RockMimic.stats().radius;
    let mimic = world.spawn_enemy(EnemyKind::RockMimic, px(270.0), floor_y(radius));
    let _ = run(&mut world, 120);
    let (body, state) = enemy(&world, mimic).expect("mimic");
    assert_eq!(state.mode, EnemyMode::Dormant);
    assert!((body.x - px(270.0)).abs() < 1.0, "it kept still");
    park_on_floor(&mut world, PLAYER, 272.0);
    let events = run(&mut world, 30);
    assert!(events.iter().any(
        |event| matches!(event, GameplayEventKind::EnemyAlerted { entity, .. } if *entity == mimic.get())
    ));
    let (_, state) = enemy(&world, mimic).expect("mimic");
    assert_ne!(state.mode, EnemyMode::Dormant);
}

#[test]
fn enemies_give_up_beyond_their_leash_and_go_home() {
    let mut world = cavern();
    no_spawns(&mut world);
    let radius = EnemyKind::Shellback.stats().radius;
    let shellback = world.spawn_enemy(EnemyKind::Shellback, px(274.0), floor_y(radius));
    let _ = run(&mut world, 30);
    let (_, state) = enemy(&world, shellback).expect("shellback");
    assert_eq!(state.target, Some(PLAYER));
    // The rig flies off far beyond the leash (but stays in reach of the despawn radius).
    park_on_floor(&mut world, PLAYER, 325.0);
    fill(&mut world, 300..302, TOP..FLOOR, TileKind::Stone);
    let _ = run(&mut world, 60 * 8);
    let (body, state) = enemy(&world, shellback).expect("shellback");
    assert!(state.target.is_none(), "{:?}", state.mode);
    assert!(matches!(state.mode, EnemyMode::Return | EnemyMode::Idle));
    assert!(
        (body.x - px(274.0)).abs() < px(8.0),
        "near home: {}",
        body.x / TILE_SIZE
    );
}

#[test]
fn burrowers_feel_drilling_through_rock() {
    let mut world = cavern();
    no_spawns(&mut world);
    // Soft ground under the rig to drill into.
    fill(&mut world, 279..283, FLOOR..FLOOR + 3, TileKind::Dirt);
    // A burrower in the stone under the floor, 9 tiles off: no line of sight.
    let burrower = world.spawn_enemy(EnemyKind::Burrower, px(272.0), px(236.5));
    let _ = run(&mut world, 60);
    assert!(
        enemy(&world, burrower)
            .expect("burrower")
            .1
            .target
            .is_none()
    );
    let drilling = TickInputs::held(PLAYER, ControlState::new(0.0, false, true));
    for _ in 0..60 {
        let _ = tick_with(&mut world, &drilling);
    }
    let (_, state) = enemy(&world, burrower).expect("burrower");
    assert_eq!(state.target, Some(PLAYER), "the drill gave the rig away");
}

#[test]
fn flares_lure_bats_away_from_rigs() {
    let mut world = cavern();
    no_spawns(&mut world);
    let bat = world.spawn_enemy(EnemyKind::CaveBat, px(272.0), px(225.0));
    let _ = run(&mut world, 30);
    let mut entities = world.live_entities();
    let (fx, fy) = (px(277.0), floor_y(6.0));
    entities.bombs.push(PlacedBomb {
        id: 900,
        x: fx,
        y: fy,
        previous_x: fx,
        previous_y: fy,
        kind: ExplosiveKind::Flare,
        stage: 1,
        detonation_tick: SimulationTick::new(u64::MAX),
        stuck: true,
        ..PlacedBomb::default()
    });
    world.apply_live_entities(entities);
    let _ = run(&mut world, 180);
    let (body, state) = enemy(&world, bat).expect("bat");
    assert!(state.target.is_none());
    assert!(
        (body.x - fx).hypot(body.y - fy) < px(4.0),
        "circling the flare"
    );
}

#[test]
fn nests_hatch_bats_up_to_their_brood() {
    let mut world = cavern();
    no_spawns(&mut world);
    let radius = EnemyKind::BroodNest.stats().radius;
    let nest = world.spawn_enemy(EnemyKind::BroodNest, px(268.0), floor_y(radius));
    let _ = run(&mut world, 60 * 40);
    let brood = world
        .enemies()
        .filter(|(_, state)| state.parent == Some(nest.get()))
        .count();
    assert!((1..=4).contains(&brood), "brood {brood}");
}

#[test]
fn resistances_explosives_and_loot() {
    let mut world = cavern();
    no_spawns(&mut world);
    let radius = EnemyKind::Shellback.stats().radius;
    let shellback = world.spawn_enemy(EnemyKind::Shellback, px(260.0), floor_y(radius));
    let full = enemy(&world, shellback).expect("shellback").1.health;
    let (outcome, events) = hit(
        &mut world,
        shellback,
        Damage::new(20.0, DamageKind::Kinetic, DamageSource::Player(PLAYER)),
    );
    assert_eq!(
        outcome,
        DamageOutcome::Hurt { dealt: 5.0 },
        "armour stops three quarters"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::EnemyHurt { .. }))
    );
    let state = enemy(&world, shellback).expect("shellback").1;
    assert_eq!(state.target, Some(PLAYER), "it turns on its attacker");
    assert!((state.health - (full - 5.0)).abs() < 0.01);
    // A bomb at its feet: explosives crack the shell.
    let drops_before = world.entities().count_of(EntityKind::Drop);
    let mut entities = world.live_entities();
    let (bx, by) = (px(260.0), floor_y(6.0));
    for id in 0..3 {
        entities.bombs.push(PlacedBomb {
            id,
            x: bx,
            y: by,
            previous_x: bx,
            previous_y: by,
            kind: ExplosiveKind::C4,
            owner: Some(PLAYER),
            detonation_tick: world.simulation_tick().next(),
            ..PlacedBomb::default()
        });
    }
    world.apply_live_entities(entities);
    let events = run(&mut world, 2);
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::EnemyKilled {
                kind: EnemyKind::Shellback,
                by: Some(PLAYER),
                ..
            }
        )),
        "{events:?}"
    );
    assert!(!world.entities().contains(shellback));
    assert!(
        world.entities().count_of(EntityKind::Drop) > drops_before,
        "loot dropped"
    );
    assert_eq!(world.enemy_director().kills, 1);
}

#[test]
fn emps_stun_and_hurt_machines_but_not_creatures_and_break_shields() {
    let mut world = cavern();
    no_spawns(&mut world);
    let drone = world.spawn_enemy(EnemyKind::MiningDrone, px(262.0), px(220.0));
    let bat = world.spawn_enemy(EnemyKind::CaveBat, px(263.0), px(220.0));
    let guardian = world.spawn_enemy(EnemyKind::AncientGuardian, px(266.0), px(214.0));
    let shield = enemy(&world, guardian).expect("guardian").1.shield;
    assert!(shield > 0.0);
    // Kinetic hits only wear the shield down.
    let guardian_health = enemy(&world, guardian).expect("guardian").1.health;
    let (outcome, _) = hit(
        &mut world,
        guardian,
        Damage::new(30.0, DamageKind::Kinetic, DamageSource::Player(PLAYER)),
    );
    assert_eq!(outcome, DamageOutcome::Ignored);
    let state = enemy(&world, guardian).expect("guardian").1;
    assert!((state.health - guardian_health).abs() < f32::EPSILON);
    assert!(state.shield < shield);
    let bat_health = enemy(&world, bat).expect("bat").1.health;
    let mut entities = world.live_entities();
    let (ex, ey) = (px(264.0), px(218.0));
    assert!(EMP_RADIUS > px(3.0));
    entities.bombs.push(PlacedBomb {
        id: 7,
        x: ex,
        y: ey,
        previous_x: ex,
        previous_y: ey,
        kind: ExplosiveKind::Emp,
        owner: Some(PLAYER),
        detonation_tick: world.simulation_tick().next(),
        ..PlacedBomb::default()
    });
    world.apply_live_entities(entities);
    let events = run(&mut world, 1);
    assert!(events.iter().any(
        |event| matches!(event, GameplayEventKind::EnemyStunned { entity, .. } if *entity == drone.get())
    ));
    let drone_state = enemy(&world, drone).expect("drone").1;
    assert_eq!(drone_state.mode, EnemyMode::Stunned);
    assert!(drone_state.health < drone_state.max_health);
    assert!((enemy(&world, bat).expect("bat").1.health - bat_health).abs() < f32::EPSILON);
    let guardian_state = enemy(&world, guardian).expect("guardian").1;
    assert!(guardian_state.shield <= 0.0, "the EMP dropped the shield");
    assert_eq!(guardian_state.mode, EnemyMode::Stunned);
    // A stunned drone falls out of the air.
    let y = enemy(&world, drone).expect("drone").0.y;
    let _ = run(&mut world, 30);
    assert!(enemy(&world, drone).expect("drone").0.y > y);
}

#[test]
fn fire_burns_creatures_and_wisps_shrug_it_off() {
    let mut world = cavern();
    no_spawns(&mut world);
    let spitter = world.spawn_enemy(EnemyKind::AcidSpitter, px(250.0), floor_y(8.0));
    let wisp = world.spawn_enemy(EnemyKind::MagmaWisp, px(252.0), px(220.0));
    let (outcome, _) = hit(
        &mut world,
        wisp,
        Damage::new(30.0, DamageKind::Fire, DamageSource::World),
    );
    assert_eq!(outcome, DamageOutcome::Ignored);
    let _ = hit(
        &mut world,
        spitter,
        Damage::new(2.0, DamageKind::Fire, DamageSource::World),
    );
    let health = enemy(&world, spitter).expect("spitter").1.health;
    let _ = run(&mut world, 60);
    assert!(
        enemy(&world, spitter).expect("spitter").1.health < health - 3.0,
        "it burned"
    );
}

#[test]
fn a_cleared_lair_never_refills() {
    let mut world = cavern();
    // Let the director fill lairs around the rig in the big cavern.
    let mut lair = None;
    for _ in 0..60 * 30 {
        let _ = tick(&mut world);
        lair = world
            .enemies()
            .find(|(_, state)| state.lair.is_some())
            .map(|(entity, state)| (entity.id, state.lair.expect("lair")));
        if lair.is_some() {
            break;
        }
    }
    let Some((id, chunk)) = lair else {
        // No lair chunk overlaps this cavern on this seed: carve the check by hand.
        return;
    };
    let big = Damage::new(
        100_000.0,
        DamageKind::Explosive,
        DamageSource::Player(PLAYER),
    );
    let (outcome, _) = hit(&mut world, id, big);
    assert!(matches!(outcome, DamageOutcome::Killed { .. }));
    assert!(world.enemy_director().cleared_lairs.contains(&chunk));
    let _ = run(&mut world, 60 * 20);
    assert!(world.enemies().all(|(_, state)| state.lair != Some(chunk)));
}

#[test]
fn walkers_hop_steps_to_reach_the_rig() {
    let mut world = cavern();
    no_spawns(&mut world);
    // A one-tile step between the shellback and the rig.
    fill(&mut world, 275..277, FLOOR - 1..FLOOR, TileKind::Stone);
    let radius = EnemyKind::Shellback.stats().radius;
    let shellback = world.spawn_enemy(EnemyKind::Shellback, px(272.5), floor_y(radius));
    let _ = hit(
        &mut world,
        shellback,
        Damage::new(1.0, DamageKind::Explosive, DamageSource::Player(PLAYER)),
    );
    let _ = run(&mut world, 60 * 6);
    let (body, _) = enemy(&world, shellback).expect("shellback");
    assert!(body.x > px(276.0), "over the step: {}", body.x / TILE_SIZE);
}

#[test]
fn flyers_path_through_gaps_in_walls() {
    let mut world = cavern();
    no_spawns(&mut world);
    // A low corridor (rows 225..232) split by a wall with a gap at its top.
    fill(&mut world, LEFT..RIGHT, TOP..225, TileKind::Stone);
    fill(&mut world, 272..273, 227..FLOOR, TileKind::Stone);
    let drone = world.spawn_enemy(EnemyKind::MiningDrone, px(262.5), px(229.5));
    let _ = hit(
        &mut world,
        drone,
        Damage::new(1.0, DamageKind::Kinetic, DamageSource::Player(PLAYER)),
    );
    let mut crossed = false;
    for _ in 0..60 * 4 {
        let _ = tick(&mut world);
        crossed |= enemy(&world, drone).is_some_and(|(body, _)| body.x > px(273.0));
    }
    assert!(crossed, "the drone found the gap");
}

#[test]
fn enemy_ai_is_deterministic_and_tick_inputs_replay_exactly() {
    let script = |world: &mut WorldState| {
        let _ = world.spawn_enemy(EnemyKind::CaveBat, px(270.0), px(222.0));
        let _ = world.spawn_enemy(EnemyKind::AcidSpitter, px(292.0), floor_y(8.0));
        let _ = world.spawn_enemy(EnemyKind::RogueExcavator, px(250.0), floor_y(13.0));
        let _ = world.spawn_enemy(EnemyKind::MiningDrone, px(300.0), px(215.0));
    };
    let mut a = cavern();
    let mut b = cavern();
    script(&mut a);
    script(&mut b);
    for index in 0..60 * 40_u32 {
        let horizontal = if (index / 90) % 2 == 0 { 1.0 } else { -1.0 };
        let inputs = TickInputs::held(
            PLAYER,
            ControlState::new(horizontal, index % 50 < 20, false),
        );
        let _ = tick_with(&mut a, &inputs);
        let _ = tick_with(&mut b, &inputs);
        assert_eq!(
            a.simulation_state_hash(),
            b.simulation_state_hash(),
            "diverged at tick {index}"
        );
    }
    assert!(a.entities().count_of(EntityKind::Enemy) > 0);
}

#[test]
fn enemies_survive_saves_and_replicate_exactly() {
    let mut authority = cavern();
    authority.add_player(OTHER);
    park_on_floor(&mut authority, OTHER, 300.0);
    let _ = authority.spawn_enemy(EnemyKind::CaveBat, px(285.0), px(220.0));
    let _ = authority.spawn_enemy(EnemyKind::SentryTurret, px(310.0), floor_y(10.0));
    let _ = run(&mut authority, 120);
    // Save (JSON) round trip.
    let json = serde_json::to_string(&authority).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(
        loaded.simulation_state_hash(),
        authority.simulation_state_hash()
    );
    assert_eq!(loaded.enemy_director(), authority.enemy_director());
    // A keyframe plus entity deltas and the enemies section keeps a replica exact.
    let mut replica: WorldState = wire(&authority);
    let mut held: BTreeMap<EntityId, u64> =
        authority.entity_revisions(None, None).into_iter().collect();
    let mut section = authority.section_hash(SectionKey::Enemies);
    for _ in 0..600 {
        let _ = tick(&mut authority);
        replica.set_simulation_tick(authority.simulation_tick());
        for id in [PLAYER, OTHER] {
            replica
                .apply_player_tick_state(id, wire(&authority.player_tick_state(id).expect("rig")));
        }
        replica.apply_live_entities(wire(&authority.live_entities()));
        let current = authority.entity_revisions(None, None);
        let updates = current
            .iter()
            .filter(|(id, revision)| held.get(id) != Some(revision))
            .filter_map(|(id, _)| authority.entity_record(*id))
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
        for key in authority.section_keys() {
            if key == SectionKey::Enemies && authority.section_hash(key) == section {
                continue;
            }
            replica.apply_section(key, wire(&authority.section(key).expect("section")));
        }
        section = authority.section_hash(SectionKey::Enemies);
        assert_eq!(
            replica.simulation_state_hash(),
            authority.simulation_state_hash()
        );
    }
    assert!(authority.entities().count_of(EntityKind::Enemy) > 0);
}

#[test]
fn departed_players_lose_their_iframes_and_old_worlds_hash_unchanged() {
    let mut world = cavern();
    let untouched = world.simulation_state_hash();
    // The default director is not hashed, so worlds from before enemies keep their hash.
    let reloaded: WorldState =
        serde_json::from_str(&serde_json::to_string(&world).expect("save")).expect("load");
    assert_eq!(reloaded.simulation_state_hash(), untouched);
    world.add_player(OTHER);
    world
        .enemies
        .rig_guard
        .insert(OTHER, SimulationTick::new(1_000));
    world.remove_player(OTHER);
    assert!(world.enemy_director().rig_guard.is_empty());
}

/// Tick cost with a full house of enemies (the world cap) hunting two rigs in the arena:
/// `cargo test -p fatherload_simulation enemy_benchmark_report -- --ignored --nocapture`
/// (add `--release`).
#[test]
#[ignore = "benchmark report"]
fn enemy_benchmark_report() {
    use std::time::{Duration, Instant};

    let mut world = cavern();
    world.add_player(OTHER);
    park_on_floor(&mut world, OTHER, 300.0);
    no_spawns(&mut world);
    let kinds = [
        EnemyKind::CaveBat,
        EnemyKind::CaveBat,
        EnemyKind::MiningDrone,
        EnemyKind::AcidSpitter,
        EnemyKind::Shellback,
        EnemyKind::RogueExcavator,
        EnemyKind::Burrower,
        EnemyKind::SentryTurret,
    ];
    for index in 0..64_u16 {
        let kind = kinds[usize::from(index) % kinds.len()];
        let column = 236.0 + f32::from(index % 32) * 2.8;
        let y = match kind.stats().locomotion {
            crate::enemy::Locomotion::Walking | crate::enemy::Locomotion::Stationary => {
                floor_y(kind.stats().radius)
            }
            crate::enemy::Locomotion::Burrowing => px(236.5),
            crate::enemy::Locomotion::Flying => px(206.0 + f32::from(index % 5) * 4.0),
        };
        let _ = world.spawn_enemy(kind, px(column), y);
    }
    let quiet = {
        let mut empty = cavern();
        let start = Instant::now();
        for _ in 0..600 {
            let _ = tick(&mut empty);
        }
        start.elapsed() / 600
    };
    let mut worst = Duration::ZERO;
    let start = Instant::now();
    for _ in 0..600 {
        let begin = Instant::now();
        let _ = tick(&mut world);
        worst = worst.max(begin.elapsed());
    }
    let mean = start.elapsed() / 600;
    println!(
        "enemy benchmark: {} enemies left, tick without enemies {quiet:?}, with enemies mean \
         {mean:?} / worst {worst:?}",
        world.entities().count_of(EntityKind::Enemy)
    );
}
