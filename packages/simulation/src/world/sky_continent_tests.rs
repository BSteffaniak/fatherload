//! Wave 4 T21 in the world: the hostile sky and its safe cities, sky enemy spawning and AI
//! (swoops, lightning jolts, airships with turrets and boarders, harpy nesters, temple
//! guards), temple fittings worked on foot, the Act II weapons (arc rifle, gravity gun,
//! tether harpoon, flak), buying them, saves, replication and determinism.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    reason = "test fixtures convert small tile, tick and pixel numbers freely"
)]

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, SimulationTick, TickInputs,
    TickOutput, WeaponControl, WorldState,
    combat::{Damage, DamageSource},
    enemy::{EnemyDifficulty, EnemyKind, EnemyMode, SkyEnemyAction},
    entity::{EntityKind, EntityState},
    item::{BlockKind, ItemKind, ItemStack, TEMPLE_KEYSTONE},
    sky::{SkyBand, SkyTile},
    terrain::{ArtifactKind, StrategicResourceKind, TILE_SIZE, TileKind, TilePosition},
    weapon::{WeaponKind, quantize_aim},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        sky::{
            self, CONTINENTS_BOTTOM_ROWS, ORBIT_BOTTOM_ROWS, SkyLand,
            continent::{self, ContinentPlan},
        },
    },
};

const PLAYER: PlayerId = PlayerId::new(1);

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V9,
        ..WorldSettings::new(0x5EED)
    }
}

fn band() -> TileRect {
    TileRect {
        min: TilePosition {
            x: -8_000,
            y: sky::row_of(ORBIT_BOTTOM_ROWS),
        },
        max: TilePosition {
            x: 8_000,
            y: sky::row_of(CONTINENTS_BOTTOM_ROWS),
        },
    }
}

/// A V9 world (peaceful unless a test says otherwise) and the continent nearest town.
fn sky_world() -> (WorldState, SkyLand, ContinentPlan) {
    let settings = settings();
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(60));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let (land, plan) = continent::continents_touching(&settings, &band())
        .into_iter()
        .min_by_key(|(land, _)| land.center_x.abs())
        .expect("a continent");
    (world, land, plan)
}

fn pixel(tile: i32) -> f32 {
    (tile as f32 + 0.5) * TILE_SIZE
}

/// Puts the rig at rest standing on the surface row `row` at column `x`.
fn stand(world: &mut WorldState, x: i32, row: i32) {
    let player = world.player_mut(PLAYER).expect("player");
    player.x = pixel(x);
    player.x_remainder = 0.0;
    player.y = row as f32 * TILE_SIZE - 11.0;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    player.fuel = player.fuel_capacity;
    world.stream_terrain_near_players();
}

/// Holds the rig still at world pixel `(x, y)` (scenes where only the enemy should move).
fn hold(world: &mut WorldState, (x, y): (f32, f32)) {
    let player = world.player_mut(PLAYER).expect("player");
    player.x = x;
    player.x_remainder = 0.0;
    player.y = y;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

fn tick(world: &mut WorldState, inputs: &TickInputs) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, inputs)
}

fn idle() -> TickInputs {
    TickInputs::held(PLAYER, ControlState::new(0.0, false, false))
}

fn fire(aim: (f32, f32), on: bool) -> TickInputs {
    let (aim_x, aim_y) = quantize_aim(aim.0, aim.1).expect("aim");
    TickInputs::held(
        PLAYER,
        ControlState::new(0.0, false, false).with_weapon(WeaponControl {
            fire: on,
            auto_aim: false,
            aim_x,
            aim_y,
        }),
    )
}

/// Runs held-still ticks until `until` matches an event (or `limit` ticks pass).
fn run_held(
    world: &mut WorldState,
    at: (f32, f32),
    limit: u32,
    mut until: impl FnMut(&GameplayEventKind) -> bool,
) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..limit {
        hold(world, at);
        let output = tick(world, &idle());
        let done = output.events.iter().any(|event| until(&event.kind));
        events.extend(output.events.into_iter().map(|event| event.kind));
        if done {
            break;
        }
    }
    events
}

fn arm(world: &mut WorldState, weapon: WeaponKind) {
    let player = world.player_mut(PLAYER).expect("player");
    let _ = player.receive_weapon(weapon);
    let _ = player.mount_weapon(Some(weapon));
    assert_eq!(
        player.mounted_weapon.map(|mounted| mounted.kind),
        Some(weapon)
    );
    player.weapon.energy = player.battery_capacity();
}

/// A spot in the hostile sky away from the city: the rig standing on the temple's plain,
/// outside the temple.
fn plain_spot(plan: &ContinentPlan) -> (i32, i32) {
    let temple = plan.temple.expect("temple");
    let x = if temple.mirrored {
        temple.left + continent::TEMPLE_WIDTH + 3
    } else {
        temple.left - 3
    };
    (x, temple.top + continent::TEMPLE_GROUND_ROW)
}

/// Open air in the hostile sky: high over the city (well above its safe zone), clear of every
/// peak. The rig is held there with [`hold`].
fn hover_spot(plan: &ContinentPlan) -> (f32, f32) {
    let city = plan.city.expect("city");
    (
        pixel(i32::midpoint(city.first, city.last)),
        pixel(city.street_row - 60),
    )
}

#[test]
fn the_sky_is_hostile_from_the_archipelago_up_except_in_cities() {
    let (world, _land, plan) = sky_world();
    let city = plan.city.expect("city");
    let street = TilePosition {
        x: i32::midpoint(city.first, city.last),
        y: city.street_row - 1,
    };
    assert!(world.enemy_safe_zone(street), "sky cities are safe");
    let (x, row) = plain_spot(&plan);
    assert!(!world.enemy_safe_zone(TilePosition { x, y: row - 1 }));
    let low = TilePosition {
        x: 0,
        y: sky::row_of(sky::LOW_ISLES_BOTTOM_ROWS + 20),
    };
    assert!(world.enemy_safe_zone(low), "the low isles stay calm");
    let arch = TilePosition {
        x: 0,
        y: sky::row_of(sky::ARCHIPELAGO_BOTTOM_ROWS + 20),
    };
    assert!(!world.enemy_safe_zone(arch));
    // A V8 sky stays safe everywhere (as it was).
    let v8 = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::V8,
            ..settings()
        },
        SimulationParams::new(60),
    );
    assert!(v8.enemy_safe_zone(arch));
}

#[test]
fn sky_enemies_spawn_around_rigs_in_the_hostile_sky_but_not_when_peaceful() {
    let (mut world, land, plan) = sky_world();
    let (x, row) = plain_spot(&plan);
    stand(&mut world, x, row);
    let at = {
        let player = world.player(PLAYER).expect("player");
        (player.x, player.y)
    };
    run_held(&mut world, at, 60 * 20, |_| false);
    assert_eq!(world.entities().count_of(EntityKind::Enemy), 0, "peaceful");
    world.set_enemy_difficulty(EnemyDifficulty::Hard);
    run_held(&mut world, at, 60 * 90, |_| false);
    let kinds: Vec<EnemyKind> = world.enemies().map(|(_, state)| state.kind).collect();
    assert!(
        kinds.iter().any(|kind| kind.is_sky()),
        "sky enemies arrive: {kinds:?} near {}",
        land.center_x
    );
    assert!(
        kinds
            .iter()
            .all(|kind| kind.is_sky() || *kind == EnemyKind::SentryTurret),
        "{kinds:?}"
    );
    for (_, state) in world.enemies() {
        if state.kind.is_sky() && state.kind != EnemyKind::HarpyNester {
            assert!(state.tier >= crate::enemy::sky_tier(SkyBand::Archipelago));
        }
    }
}

#[test]
fn swoopers_climb_above_their_prey_and_dive_through_it() {
    let (mut world, _land, plan) = sky_world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let at = hover_spot(&plan);
    hold(&mut world, at);
    world.stream_terrain_near_players();
    let ray = world.spawn_enemy(
        EnemyKind::SkyRay,
        at.0 - TILE_SIZE * 6.0,
        at.1 - TILE_SIZE * 4.0,
    );
    let events = run_held(&mut world, at, 60 * 12, |event| {
        matches!(
            event,
            GameplayEventKind::RigStruck {
                kind: EnemyKind::SkyRay,
                ..
            }
        )
    });
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::SkyEnemyAction {
                action: SkyEnemyAction::Swoop,
                ..
            }
        )),
        "the ray hangs, then dives"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::RigStruck { .. })),
        "the dive hits"
    );
    assert!(world.entities().get(ray).is_some());
}

#[test]
fn storm_elementals_jolt_a_rigs_capacitor_and_weapon() {
    let (mut world, _land, plan) = sky_world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    arm(&mut world, WeaponKind::RivetGun);
    let at = hover_spot(&plan);
    hold(&mut world, at);
    world.stream_terrain_near_players();
    let hull = world.player(PLAYER).expect("player").hull;
    let _storm = world.spawn_enemy(
        EnemyKind::StormElemental,
        at.0 - TILE_SIZE * 5.0,
        at.1 - TILE_SIZE * 2.0,
    );
    let events = run_held(&mut world, at, 60 * 12, |event| {
        matches!(
            event,
            GameplayEventKind::LightningArc {
                struck: Some(PLAYER),
                ..
            }
        )
    });
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::LightningArc {
                struck: Some(PLAYER),
                ..
            }
        )),
        "{events:?}"
    );
    let player = world.player(PLAYER).expect("player");
    assert!(player.hull < hull);
    assert!(player.weapon.energy < 5.0, "capacitor drained");
    assert!(
        player.weapon.ready_tick > world.simulation_tick(),
        "weapon jammed"
    );
}

#[test]
fn airships_carry_turrets_drop_boarders_and_sink_with_them() {
    let (mut world, _land, plan) = sky_world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let at = hover_spot(&plan);
    hold(&mut world, at);
    world.stream_terrain_near_players();
    let hull = world.spawn_airship(at.0 - TILE_SIZE * 7.0, at.1 - TILE_SIZE * 6.0);
    let events = run_held(&mut world, at, 60 * 14, |event| {
        matches!(
            event,
            GameplayEventKind::SkyEnemyAction {
                action: SkyEnemyAction::BoardersDropped,
                ..
            }
        )
    });
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::SkyEnemyAction {
                action: SkyEnemyAction::BoardersDropped,
                ..
            }
        )),
        "boarders come down"
    );
    let hull_body = world.entities().get(hull).expect("hull").body;
    let turrets: Vec<_> = world
        .enemies()
        .filter(|(_, state)| state.kind == EnemyKind::AirshipTurret)
        .map(|(entity, state)| (entity.id, entity.body, state.home))
        .collect();
    assert_eq!(turrets.len(), 2);
    for (_, body, (ox, oy)) in &turrets {
        assert!((body.x - (hull_body.x + *ox as f32)).abs() < 1.0);
        assert!((body.y - (hull_body.y + *oy as f32)).abs() < 1.0);
    }
    let pirates = world
        .enemies()
        .filter(|(_, state)| state.kind == EnemyKind::SkyPirate && state.parent == Some(hull.0))
        .count();
    assert!(pirates >= 1);
    // Down the hull: the turrets go with it, the loot drops.
    let mut output = TickOutput::default();
    let outcome = world.damage_entity(
        hull,
        Damage::new(
            100_000.0,
            DamageSource::Weapon {
                player: PLAYER,
                weapon: WeaponKind::FlakCannon,
            },
        ),
        &mut output,
    );
    assert!(matches!(
        outcome,
        crate::combat::DamageOutcome::Destroyed { .. }
    ));
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::SkyEnemyAction {
            action: SkyEnemyAction::AirshipDowned,
            ..
        }
    )));
    for (id, ..) in turrets {
        assert!(
            world.entities().get(id).is_none(),
            "turrets sink with the hull"
        );
    }
    assert!(
        world.entities().count_of(EntityKind::Drop) > 0,
        "loot drops"
    );
}

#[test]
fn harpy_nesters_guard_their_nest_until_its_eggs_are_taken() {
    let (mut world, _land, plan) = sky_world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let nest = plan.nests[0].expect("nest");
    // Hover the rig 15 tiles off the nest, level with it: in sight, outside its guard.
    let at = (pixel(nest.x - 15), pixel(nest.y - 3));
    hold(&mut world, at);
    world.stream_terrain_near_players();
    let perch = (pixel(nest.x), pixel(nest.y - 3));
    let harpy = world.spawn_enemy(EnemyKind::HarpyNester, perch.0, perch.1);
    run_held(&mut world, at, 120, |_| false);
    let mode = world
        .enemies()
        .find(|(entity, _)| entity.id == harpy)
        .map(|(_, state)| state.mode)
        .expect("harpy");
    assert!(!mode.hostile(), "a calm nester away from trouble: {mode:?}");
    // Rob the nest.
    for dx in [-1, 0, 1] {
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x: nest.x + dx,
                y: nest.y - 1,
            },
            TileKind::Air,
        );
    }
    let events = run_held(&mut world, at, 240, |event| {
        matches!(
            event,
            GameplayEventKind::SkyEnemyAction {
                action: SkyEnemyAction::NesterEnraged,
                ..
            }
        )
    });
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::SkyEnemyAction {
                action: SkyEnemyAction::NesterEnraged,
                ..
            }
        )),
        "{events:?}"
    );
}

#[test]
fn temples_keep_guards_until_they_are_killed() {
    let (mut world, _land, plan) = sky_world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let temple = plan.temple.expect("temple");
    let (x, row) = plain_spot(&plan);
    stand(&mut world, x, row);
    let at = {
        let player = world.player(PLAYER).expect("player");
        (player.x, player.y)
    };
    run_held(&mut world, at, 60 * 3, |_| false);
    let posts = temple.guard_posts();
    let guards: Vec<_> = world
        .enemies()
        .filter(|(_, state)| {
            state.lair.is_some_and(|lair| {
                posts
                    .iter()
                    .any(|post| lair.x == post.x && lair.y == post.y)
            })
        })
        .map(|(entity, state)| (entity.id, state.kind))
        .collect();
    assert_eq!(guards.len(), 2, "{guards:?}");
    assert!(
        guards
            .iter()
            .any(|(_, kind)| *kind == EnemyKind::SentryTurret)
    );
    let mut output = TickOutput::default();
    for (id, _) in &guards {
        let _ = world.damage_entity(
            *id,
            Damage::new(
                100_000.0,
                DamageSource::Weapon {
                    player: PLAYER,
                    weapon: WeaponKind::Railgun,
                },
            ),
            &mut output,
        );
    }
    run_held(&mut world, at, 60 * 6, |_| false);
    assert!(
        !world
            .enemies()
            .any(|(_, state)| state.lair.is_some_and(|lair| posts
                .iter()
                .any(|post| lair.x == post.x && lair.y == post.y))),
        "cleared guards never come back"
    );
}

#[test]
#[allow(clippy::too_many_lines, reason = "the whole temple, in order")]
fn temple_levers_keystones_gates_and_chests_open_on_foot() {
    let (mut world, _land, plan) = sky_world();
    let temple = plan.temple.expect("temple");
    let lever = temple.tiles_of('L')[0];
    stand(&mut world, lever.x, lever.y + 1);
    world.stream_terrain_near_players();
    let gates = temple.tiles_of('D');
    for gate in &gates {
        assert_eq!(
            world.terrain().tile(*gate).map(|tile| tile.kind),
            Some(TileKind::Sky(SkyTile::TempleDoor))
        );
    }
    // A pilot swinging its pick at the lever pulls it.
    let side = if temple.mirrored { -1.0 } else { 1.0 };
    let pilot = world
        .place_pilot(
            PLAYER,
            pixel(lever.x) + side * TILE_SIZE,
            (lever.y + 1) as f32 * TILE_SIZE,
        )
        .expect("pilot");
    let _ = pilot;
    let mut pulled = Vec::new();
    for _ in 0..40 {
        let output = tick(&mut world, &fire((-side, 0.0), true));
        pulled.extend(output.events.into_iter().map(|event| event.kind));
    }
    assert!(
        pulled.iter().any(|event| matches!(
            event,
            GameplayEventKind::TempleLeverPulled { opened: 2, .. }
        )),
        "{pulled:?}"
    );
    for gate in &gates {
        assert_eq!(
            world.terrain().tile(*gate).map(|tile| tile.kind),
            Some(TileKind::Air)
        );
    }
    // The keystone gate stays shut without the keystone.
    let vault = temple.tiles_of('K')[0];
    let mut output = TickOutput::default();
    assert!(!world.work_temple_fitting(PLAYER, vault, SkyTile::KeystoneDoor, &mut output));
    // Prise the keystone: it drops as the temple's key.
    let keystone = temple.tiles_of('k')[0];
    assert!(world.work_temple_fitting(PLAYER, keystone, SkyTile::TempleKeystone, &mut output));
    assert!(
        world
            .entities()
            .of_kind(EntityKind::Drop)
            .any(|entity| matches!(
                &entity.state,
                EntityState::Drop(drop) if drop.stack.kind == TEMPLE_KEYSTONE
            ))
    );
    // A keystone that went into the rig's hold (boarding empties the pack) still opens it.
    let mut stowed = world.clone();
    stowed
        .player_mut(PLAYER)
        .expect("player")
        .cargo_bay
        .add(TEMPLE_KEYSTONE, 1);
    assert!(stowed.work_temple_fitting(PLAYER, vault, SkyTile::KeystoneDoor, &mut output));
    assert_eq!(
        stowed
            .player(PLAYER)
            .expect("player")
            .cargo_bay
            .count(TEMPLE_KEYSTONE),
        0,
        "the stowed keystone is spent"
    );
    world
        .player_mut(PLAYER)
        .expect("player")
        .pilot_pack
        .add(TEMPLE_KEYSTONE, 1);
    assert!(world.work_temple_fitting(PLAYER, vault, SkyTile::KeystoneDoor, &mut output));
    assert_eq!(
        world
            .player(PLAYER)
            .expect("player")
            .pilot_pack
            .count(TEMPLE_KEYSTONE),
        0,
        "the keystone is spent"
    );
    for tile in temple.tiles_of('K') {
        assert_eq!(
            world.terrain().tile(tile).map(|t| t.kind),
            Some(TileKind::Air)
        );
    }
    // A relic chest spills a sky relic and sky materials.
    let drops_before = world.entities().count_of(EntityKind::Drop);
    let chest = temple.tiles_of('C')[0];
    assert!(world.work_temple_fitting(PLAYER, chest, SkyTile::TempleCache, &mut output));
    assert!(world.entities().count_of(EntityKind::Drop) > drops_before);
    assert!(world.entities().of_kind(EntityKind::Drop).any(|entity| matches!(
        &entity.state,
        EntityState::Drop(drop) if drop.stack.kind == ItemKind::Artifact(ArtifactKind::SkyRelic)
    )));
    assert!(
        output
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::RelicChestOpened { .. }))
    );
    // Drills cannot touch temple stone.
    assert!(crate::terrain::is_indestructible(TileKind::Sky(
        SkyTile::TempleBrick
    )));
}

/// The rig on the city's street (a safe, flat, open spot) with `weapon` mounted.
fn street_range(weapon: WeaponKind) -> (WorldState, (f32, f32)) {
    let (mut world, _land, plan) = sky_world();
    let city = plan.city.expect("city");
    stand(&mut world, city.first + 8, city.street_row);
    arm(&mut world, weapon);
    run_held_idle(&mut world, 3);
    let at = {
        let player = world.player(PLAYER).expect("player");
        (player.x, player.y)
    };
    (world, at)
}

fn run_held_idle(world: &mut WorldState, ticks: u32) {
    for _ in 0..ticks {
        let _ = tick(world, &idle());
    }
}

#[test]
fn the_arc_rifle_chains_from_target_to_target() {
    let (mut world, (x, y)) = street_range(WeaponKind::ArcRifle);
    let dummies: Vec<_> = (0..4)
        .map(|index| {
            world.spawn_target_dummy(
                x + TILE_SIZE * (3.0 + index as f32 * 3.0),
                y - 14.0,
                500.0,
                0.0,
            )
        })
        .collect();
    let output = tick(&mut world, &fire((1.0, 0.0), true));
    let links = output
        .events
        .iter()
        .filter(|event| matches!(event.kind, GameplayEventKind::ArcChained { .. }))
        .count();
    assert_eq!(links, 4, "the first hit plus three leaps");
    let healths: Vec<f32> = dummies
        .iter()
        .map(|id| {
            world
                .entities()
                .get(*id)
                .and_then(|entity| entity.state.health())
                .map_or(0.0, |health| health.current)
        })
        .collect();
    assert!(healths.iter().all(|health| *health < 500.0), "{healths:?}");
    assert!(
        healths[0] < healths[3],
        "each leap loses a quarter: {healths:?}"
    );
}

#[test]
fn the_gravity_gun_vacuums_loot_and_rips_loose_blocks() {
    let (mut world, (x, y)) = street_range(WeaponKind::GravityGun);
    let _drop = world.spawn_item_drop(
        x + TILE_SIZE * 5.0,
        y,
        (0.0, 0.0),
        ItemStack::new(ItemKind::Mineral(crate::terrain::MineralKind::Gold), 1),
        None,
    );
    let wall = TilePosition {
        x: ((x + TILE_SIZE * 5.0) / TILE_SIZE).floor() as i32,
        y: ((y - 14.0) / TILE_SIZE).floor() as i32,
    };
    let _ = world.terrain_mut().set_kind(wall, TileKind::Dirt);
    for _ in 0..90 {
        let _ = tick(&mut world, &fire((1.0, 0.0), true));
    }
    assert_eq!(
        world.terrain().tile(wall).map(|tile| tile.kind),
        Some(TileKind::Air),
        "the loose block was ripped out"
    );
    let player = world.player(PLAYER).expect("player");
    assert!(
        player
            .cargo_bay
            .count(ItemKind::Mineral(crate::terrain::MineralKind::Gold))
            >= 1,
        "the gold was pulled in"
    );
    assert!(
        player.block_hopper.count(ItemKind::Block(BlockKind::Dirt)) >= 1,
        "the ripped block was pulled in"
    );
    assert!(
        player.weapon.energy < player.battery_capacity(),
        "holding drains energy"
    );
}

#[test]
fn the_gravity_gun_holds_enemies_helpless_and_flings_them() {
    let (mut world, (x, y)) = street_range(WeaponKind::GravityGun);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let ray = world.spawn_enemy(EnemyKind::SkyRay, x + TILE_SIZE * 3.0, y - TILE_SIZE);
    for _ in 0..30 {
        let _ = tick(&mut world, &fire((1.0, -0.3), true));
    }
    let state = world
        .enemies()
        .find(|(entity, _)| entity.id == ray)
        .map(|(_, state)| *state)
        .expect("ray");
    assert_eq!(state.mode, EnemyMode::Stunned);
    let output = tick(&mut world, &fire((1.0, -0.3), false));
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::GravityThrow { count, .. } if count >= 1
    )));
    if let Some((entity, after)) = world.enemies().find(|(entity, _)| entity.id == ray) {
        assert!(after.health < state.health, "a thrown enemy is hurt");
        assert!(entity.body.velocity_x > 200.0, "{:?}", entity.body);
    }
}

#[test]
fn the_tether_harpoon_swings_the_rig_and_reels_in() {
    let (mut world, land, plan) = sky_world();
    let city = plan.city.expect("city");
    let column = i32::midpoint(city.first, city.last);
    // The lowest solid tile of the column: the continent's underside.
    let underside = (land.top_row..land.top_row + land.depth + 40)
        .filter(|y| land.tile_at(column, *y).is_some())
        .max()
        .expect("underside");
    let at = (pixel(column), pixel(underside + 8));
    hold(&mut world, at);
    world.stream_terrain_near_players();
    arm(&mut world, WeaponKind::TetherHarpoon);
    let output = tick(&mut world, &fire((0.0, -1.0), true));
    assert!(
        output
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::TetherAttached { .. }))
    );
    let tether = world
        .player(PLAYER)
        .expect("player")
        .weapon
        .tether
        .expect("hooked");
    // Swing out sideways and fall: the line holds the rig.
    world.player_mut(PLAYER).expect("player").velocity_x = 300.0;
    for _ in 0..120 {
        let _ = tick(&mut world, &fire((0.0, -1.0), true));
        let player = world.player(PLAYER).expect("player");
        let distance = (player.x - tether.x).hypot(player.y - tether.y);
        assert!(
            distance <= tether.length + 2.0,
            "{distance} > {}",
            tether.length
        );
    }
    // Thrust with the trigger held reels the line in.
    let reel = TickInputs::held(
        PLAYER,
        ControlState::new(0.0, true, false).with_weapon(WeaponControl {
            fire: true,
            auto_aim: false,
            aim_x: 0,
            aim_y: -127,
        }),
    );
    for _ in 0..30 {
        let _ = tick(&mut world, &reel);
    }
    let shorter = world
        .player(PLAYER)
        .expect("player")
        .weapon
        .tether
        .expect("still hooked");
    assert!(shorter.length < tether.length);
    let output = tick(&mut world, &fire((0.0, -1.0), false));
    assert!(
        output
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::TetherReleased { .. }))
    );
    assert!(
        world
            .player(PLAYER)
            .expect("player")
            .weapon
            .tether
            .is_none()
    );
}

#[test]
fn flak_bursts_beside_airships_and_hurts_them_most() {
    let (mut world, (x, y)) = street_range(WeaponKind::FlakCannon);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let hull = world.spawn_airship(x + TILE_SIZE * 8.0, y - TILE_SIZE * 2.0);
    let start = world
        .enemies()
        .find(|(entity, _)| entity.id == hull)
        .map(|(_, state)| state.health)
        .expect("hull");
    let mut bursts = 0;
    for _ in 0..90 {
        let (hx, hy) = world
            .entities()
            .get(hull)
            .map(|entity| (entity.body.x, entity.body.y))
            .expect("hull");
        let player = world.player(PLAYER).expect("player");
        let aim = (hx - player.x, hy - (player.y - crate::weapon::MUZZLE_RISE));
        let output = tick(&mut world, &fire(aim, true));
        bursts += output
            .events
            .iter()
            .filter(|event| matches!(event.kind, GameplayEventKind::FlakBurst { .. }))
            .count();
    }
    let now = world
        .enemies()
        .find(|(entity, _)| entity.id == hull)
        .map_or(0.0, |(_, state)| state.health);
    assert!(bursts >= 1);
    assert!(now < start, "{now} vs {start}");
    assert!(
        EnemyKind::PirateAirship.resistance(crate::enemy::DamageKind::Explosive)
            > EnemyKind::PirateAirship.resistance(crate::enemy::DamageKind::Kinetic) * 3.0
    );
}

#[test]
fn act_two_weapons_cost_sky_materials_and_need_a_sky() {
    let (mut world, ..) = sky_world();
    {
        let player = world.player_mut(PLAYER).expect("player");
        player.credits = 100_000;
    }
    assert_eq!(
        world.buy_weapon(PLAYER, WeaponKind::ArcRifle, 3),
        crate::summary::PlayerScopedCommandOutcome::IgnoredUnavailable,
        "no storm glass"
    );
    world
        .player_mut(PLAYER)
        .expect("player")
        .cargo_bay
        .add(StrategicResourceKind::StormGlass, 7);
    assert_eq!(
        world.buy_weapon(PLAYER, WeaponKind::ArcRifle, 3),
        crate::summary::PlayerScopedCommandOutcome::Applied
    );
    let player = world.player(PLAYER).expect("player");
    assert_eq!(
        player.cargo_bay.material(StrategicResourceKind::StormGlass),
        1
    );
    assert!(player.weapon_rack.owned.contains_key(&WeaponKind::ArcRifle));
    // Not on a world without sky content.
    let mut v8 = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::V8,
            ..settings()
        },
        SimulationParams::new(60),
    );
    {
        let player = v8.player_mut(PLAYER).expect("player");
        player.credits = 100_000;
        player.cargo_bay.add(StrategicResourceKind::StormGlass, 7);
    }
    assert_eq!(
        v8.buy_weapon(PLAYER, WeaponKind::TetherHarpoon, 3),
        crate::summary::PlayerScopedCommandOutcome::IgnoredUnavailable
    );
}

#[test]
fn a_sky_battle_saves_replicates_and_replays_identically() {
    let (mut world, _land, plan) = sky_world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    arm(&mut world, WeaponKind::FlakCannon);
    let at = hover_spot(&plan);
    hold(&mut world, at);
    world.stream_terrain_near_players();
    let _hull = world.spawn_airship(at.0 - TILE_SIZE * 7.0, at.1 - TILE_SIZE * 6.0);
    let _ray = world.spawn_enemy(
        EnemyKind::SkyRay,
        at.0 + TILE_SIZE * 6.0,
        at.1 - TILE_SIZE * 5.0,
    );
    for _ in 0..120 {
        let _ = tick(&mut world, &fire((-1.0, -0.8), true));
    }
    let json = serde_json::to_string(&world).expect("json");
    let mut restored: WorldState = serde_json::from_str(&json).expect("restore");
    assert_eq!(
        restored.simulation_state_hash(),
        world.simulation_state_hash()
    );
    let bytes = postcard::to_allocvec(&world).expect("postcard");
    let from_bytes: WorldState = postcard::from_bytes(&bytes).expect("postcard restores");
    assert_eq!(
        from_bytes.simulation_state_hash(),
        world.simulation_state_hash()
    );
    // A client keyframe agrees on everything the client may see.
    let client = world.redacted_for(Some(PLAYER));
    assert_eq!(
        client.visible_state_hash(Some(PLAYER)),
        world.visible_state_hash(Some(PLAYER))
    );
    for step in 0..240_u32 {
        let inputs = fire((-1.0, -0.8 + (step % 7) as f32 * 0.1), step % 3 != 0);
        let tick_now = world.simulation_tick();
        let a = world.simulate_tick(tick_now, &inputs);
        let b = restored.simulate_tick(SimulationTick::new(tick_now.get()), &inputs);
        assert_eq!(a, b, "tick {step}");
    }
    assert_eq!(
        restored.simulation_state_hash(),
        world.simulation_state_hash()
    );
}
