//! Hardpoint weapons through the tick: firing, projectiles, hitscan rays, the damage API,
//! energy and ammo, auto-aim, friendly fire, terrain damage, the shack and replication.

use super::WorldImport;
use crate::{
    ControlState, ExplosiveKind, GameplayEventKind, PlacedBomb, PlayerCommand, PlayerId,
    SessionServiceRequest, SimulationParams, SimulationTick, TickInputs, WeaponControl, WorldState,
    combat::{Damage, DamageOutcome, DamageSource, Health},
    entity::{EntityId, EntityKind},
    event::ImpactSurface,
    item::{ItemKind, ItemStack},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    weapon::{AmmoKind, MAX_WEAPON_MK, WeaponKind},
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);

/// An open air box (tiles 8..=40 by 8..=31) over a bedrock floor at row 32, with the rig
/// standing on the floor at the left, armed with `weapon` (Mk I) and a full capacitor.
fn range(weapon: WeaponKind) -> WorldState {
    range_at(weapon, 60)
}

fn range_at(weapon: WeaponKind, hz: u32) -> WorldState {
    let mut world = WorldState::from_import_with_params(
        &WorldImport::new_game(PLAYER),
        SimulationParams::new(hz),
    );
    for y in 8..=32 {
        for x in 8..=40 {
            let kind = if y == 32 {
                TileKind::Bedrock
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    let player = world.player_mut(PLAYER).expect("player");
    player.x = 12.5 * TILE_SIZE;
    player.y = 31.5 * TILE_SIZE;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    let _ = player.receive_weapon(weapon);
    player.weapon.energy = player.battery_capacity();
    player.fuel = player.fuel_capacity;
    world
}

/// A practice target standing on the floor at tile column `tile_x`, level with the muzzle.
fn dummy(world: &mut WorldState, tile_x: f32, health: f32, armor: f32) -> EntityId {
    world.spawn_target_dummy(
        tile_x * TILE_SIZE,
        31.5f32.mul_add(TILE_SIZE, -crate::weapon::MUZZLE_RISE),
        health,
        armor,
    )
}

fn trigger(fire: bool, aim: (i8, i8)) -> ControlState {
    ControlState::new(0.0, false, false).with_weapon(WeaponControl {
        fire,
        auto_aim: false,
        aim_x: aim.0,
        aim_y: aim.1,
    })
}

/// Holds `controls` for `ticks` full ticks; returns the events.
fn hold(world: &mut WorldState, controls: ControlState, ticks: u64) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        let inputs = TickInputs::held(PLAYER, controls);
        let output = world.simulate_tick(world.simulation_tick(), &inputs);
        events.extend(output.events.into_iter().map(|event| event.kind));
    }
    events
}

fn health_of(world: &WorldState, id: EntityId) -> Option<f32> {
    world
        .entities()
        .get(id)
        .and_then(|entity| entity.state.health())
        .map(|health| health.current)
}

fn count(events: &[GameplayEventKind], matches: impl Fn(&GameplayEventKind) -> bool) -> usize {
    events.iter().filter(|event| matches(event)).count()
}

fn kind_at(world: &WorldState, x: i32, y: i32) -> TileKind {
    world
        .terrain()
        .tile(TilePosition { x, y })
        .expect("tile")
        .kind
}

#[test]
fn rivets_fly_hit_a_target_and_wear_it_down_to_destruction() {
    let mut world = range(WeaponKind::RivetGun);
    let target = dummy(&mut world, 22.0, 30.0, 0.0);
    let events = hold(&mut world, trigger(true, (127, 0)), 2);
    assert_eq!(
        count(&events, |event| matches!(
            event,
            GameplayEventKind::WeaponFired { .. }
        )),
        1
    );
    assert_eq!(world.entities().count_of(EntityKind::Projectile), 1);
    let events = hold(&mut world, trigger(true, (127, 0)), 90);
    assert!(health_of(&world, target).is_none(), "destroyed");
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::EntityDestroyed {
            by: Some(PLAYER),
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::ProjectileImpact {
            surface: ImpactSurface::Target,
            ..
        }
    )));
    let energy = world.player(PLAYER).expect("player").weapon.energy;
    assert!(energy < world.player(PLAYER).expect("player").battery_capacity());
}

#[test]
fn semi_automatic_weapons_need_a_fresh_pull_and_cooldowns_hold_at_every_tick_rate() {
    let mut world = range(WeaponKind::Harpoon);
    let events = hold(&mut world, trigger(true, (127, -40)), 180);
    assert_eq!(
        count(&events, |event| matches!(
            event,
            GameplayEventKind::WeaponFired { .. }
        )),
        1,
        "holding the trigger fires a harpoon once"
    );
    for hz in [30, 60, 120] {
        let mut world = range_at(WeaponKind::RivetGun, hz);
        let events = hold(&mut world, trigger(true, (127, -60)), u64::from(hz));
        let shots = count(&events, |event| {
            matches!(event, GameplayEventKind::WeaponFired { .. })
        });
        // 150 ms between rivets: six or seven shots in a second at any rate.
        assert!((6..=7).contains(&shots), "{hz} Hz: {shots}");
    }
}

#[test]
fn the_capacitor_drains_clicks_dry_and_recharges_from_fuel() {
    let mut world = range(WeaponKind::Railgun);
    world.player_mut(PLAYER).expect("player").weapon.energy = 45.0;
    let events = hold(&mut world, trigger(true, (127, -30)), 1);
    assert_eq!(
        count(&events, |event| matches!(
            event,
            GameplayEventKind::WeaponFired { .. }
        )),
        1
    );
    // Wait out the cooldown with too little energy for another slug.
    let _ = hold(&mut world, trigger(false, (127, -30)), 100);
    world.player_mut(PLAYER).expect("player").weapon.energy = 10.0;
    let fuel_before = world.player(PLAYER).expect("player").fuel;
    let events = hold(&mut world, trigger(true, (127, -30)), 1);
    assert_eq!(
        count(&events, |event| matches!(
            event,
            GameplayEventKind::WeaponDry { .. }
        )),
        1,
        "too little energy left: the empty click"
    );
    let _ = hold(&mut world, trigger(false, (127, -30)), 120);
    let player = world.player(PLAYER).expect("player");
    assert!(
        player.weapon.energy > 40.0,
        "recharged: {}",
        player.weapon.energy
    );
    assert!(player.fuel < fuel_before, "recharging burns fuel");
    // An empty tank recharges nothing.
    let mut dry = range(WeaponKind::RivetGun);
    let player = dry.player_mut(PLAYER).expect("player");
    player.weapon.energy = 0.0;
    player.fuel = 0.0;
    let _ = hold(&mut dry, trigger(false, (127, 0)), 60);
    assert!(dry.player(PLAYER).expect("player").weapon.energy.abs() < f32::EPSILON);
    // A rig that owns no weapon (an old save's empty capacitor) never pays fuel for one.
    let mut unarmed = range(WeaponKind::RivetGun);
    let player = unarmed.player_mut(PLAYER).expect("player");
    player.weapon_rack.owned.clear();
    player.mounted_weapon = None;
    player.weapon.energy = 0.0;
    let fuel_before = player.fuel;
    let _ = hold(&mut unarmed, trigger(true, (127, 0)), 60);
    let player = unarmed.player(PLAYER).expect("player");
    assert!((player.fuel - fuel_before).abs() < f32::EPSILON);
    assert!(player.weapon.energy.abs() < f32::EPSILON);
}

#[test]
fn rockets_use_ammo_and_burst_into_a_blast_that_hurts_and_digs() {
    let mut world = range(WeaponKind::RocketLauncher);
    for y in 26..=31 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x: 24, y }, TileKind::Stone);
    }
    let target = dummy(&mut world, 23.0, 200.0, 0.0);
    world
        .player_mut(PLAYER)
        .expect("player")
        .cargo_bay
        .add(AmmoKind::Rocket.item(), 2);
    let events = hold(&mut world, trigger(true, (127, 0)), 1);
    assert_eq!(
        count(&events, |event| matches!(
            event,
            GameplayEventKind::WeaponFired { .. }
        )),
        1
    );
    assert_eq!(
        world.player(PLAYER).expect("player").ammo(AmmoKind::Rocket),
        1
    );
    let events = hold(&mut world, trigger(false, (127, 0)), 60);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::ExplosiveDetonated {
            kind: ExplosiveKind::Rocket,
            ..
        }
    )));
    let left = health_of(&world, target).expect("survives one rocket");
    assert!(left < 200.0 - 12.0, "direct hit plus blast: {left}");
    // Out of ammo: the empty click.
    let _ = hold(&mut world, trigger(true, (127, -100)), 1);
    let _ = hold(&mut world, trigger(false, (127, -100)), 60);
    let events = hold(&mut world, trigger(true, (127, -100)), 1);
    assert_eq!(
        count(&events, |event| matches!(
            event,
            GameplayEventKind::WeaponDry { .. }
        )),
        1
    );
}

#[test]
fn rocket_blasts_break_rock_where_they_land() {
    let mut world = range(WeaponKind::RocketLauncher);
    for y in 26..=31 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x: 24, y }, TileKind::Stone);
    }
    world
        .player_mut(PLAYER)
        .expect("player")
        .cargo_bay
        .add(AmmoKind::Rocket.item(), 1);
    let _ = hold(&mut world, trigger(true, (127, 0)), 1);
    let events = hold(&mut world, trigger(false, (127, 0)), 90);
    assert_eq!(
        kind_at(&world, 24, 31),
        TileKind::Air,
        "the warhead blew the wall open: {events:?}"
    );
}

#[test]
fn auto_aim_locks_the_nearest_target_in_the_facing_cone_but_not_through_smoke() {
    let mut world = range(WeaponKind::RivetGun);
    let behind = dummy(&mut world, 9.5, 50.0, 0.0);
    let ahead = dummy(&mut world, 19.0, 50.0, 0.0);
    let _far = dummy(&mut world, 26.0, 50.0, 0.0);
    let locked = world.auto_aim_target(PLAYER, (1.0, 0.0)).map(|(id, ..)| id);
    assert_eq!(locked, Some(ahead));
    let locked = world
        .auto_aim_target(PLAYER, (-1.0, 0.0))
        .map(|(id, ..)| id);
    assert_eq!(locked, Some(behind));
    // Auto-aim fires at the lock even when facing slightly off.
    let controls = ControlState::new(0.0, false, false).with_weapon(WeaponControl {
        fire: true,
        auto_aim: true,
        aim_x: 127,
        aim_y: 0,
    });
    let _ = hold(&mut world, controls, 40);
    assert!(health_of(&world, ahead).expect("alive") < 50.0);
    assert!((health_of(&world, behind).expect("alive") - 50.0).abs() < f32::EPSILON);
    // A smoke screen over the target hides it.
    world.insert_charge(PlacedBomb {
        id: 900,
        x: 19.0 * TILE_SIZE,
        y: 31.5 * TILE_SIZE,
        previous_x: 19.0 * TILE_SIZE,
        previous_y: 31.5 * TILE_SIZE,
        detonation_tick: world.simulation_tick().next(),
        kind: ExplosiveKind::Smoke,
        ..PlacedBomb::default()
    });
    let _ = hold(&mut world, trigger(false, (127, 0)), 90);
    let locked = world.auto_aim_target(PLAYER, (1.0, 0.0)).map(|(id, ..)| id);
    assert_ne!(locked, Some(ahead), "hidden in smoke");
}

#[test]
fn railgun_slugs_pierce_armour_and_targets_and_punch_through_soft_rock() {
    let mut world = range(WeaponKind::Railgun);
    let first = dummy(&mut world, 16.0, 200.0, 30.0);
    let second = dummy(&mut world, 18.0, 200.0, 30.0);
    for x in 20..=21 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x, y: 31 }, TileKind::Dirt);
    }
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: 22, y: 31 }, TileKind::Clay);
    let _ = world.terrain_mut().set_kind(
        TilePosition { x: 23, y: 31 },
        TileKind::Ore(MineralKind::Gold),
    );
    let behind = dummy(&mut world, 25.0, 200.0, 0.0);
    let _ = hold(&mut world, trigger(true, (127, 0)), 1);
    let damage = WeaponKind::Railgun.stats(0).damage;
    for id in [first, second] {
        let left = health_of(&world, id).expect("alive");
        assert!(
            (200.0 - left - damage).abs() < 0.01,
            "armour ignored: {left}"
        );
    }
    for x in 20..=22 {
        assert_eq!(kind_at(&world, x, 31), TileKind::Air, "tunnelled {x}");
    }
    assert_eq!(
        kind_at(&world, 23, 31),
        TileKind::Ore(MineralKind::Gold),
        "ore stops the slug unharmed"
    );
    assert!((health_of(&world, behind).expect("alive") - 200.0).abs() < f32::EPSILON);
}

#[test]
fn armour_blunts_light_weapons_much_more_than_heavy_ones() {
    let mut light = range(WeaponKind::RivetGun);
    let plated = dummy(&mut light, 18.0, 500.0, 6.0);
    let _ = hold(&mut light, trigger(true, (127, 0)), 60);
    let rivet_damage = 500.0 - health_of(&light, plated).expect("alive");
    let mut bare = range(WeaponKind::RivetGun);
    let soft = dummy(&mut bare, 18.0, 500.0, 0.0);
    let _ = hold(&mut bare, trigger(true, (127, 0)), 60);
    let bare_damage = 500.0 - health_of(&bare, soft).expect("alive");
    assert!(
        rivet_damage * 4.0 < bare_damage,
        "{rivet_damage} vs {bare_damage}"
    );
    // The damage API directly: armour and piercing.
    let mut world = range(WeaponKind::RivetGun);
    let id = dummy(&mut world, 30.0, 100.0, 10.0);
    let mut output = crate::TickOutput::default();
    let hit = |amount, armor_piercing| Damage {
        amount,
        source: DamageSource::Environment,
        armor_piercing,
        knockback: (0.0, 0.0),
    };
    assert_eq!(
        world.damage_entity(id, hit(30.0, 0.0), &mut output),
        DamageOutcome::Hurt {
            dealt: 20.0,
            remaining: 80.0
        }
    );
    assert_eq!(
        world.damage_entity(id, hit(500.0, 1.0), &mut output),
        DamageOutcome::Destroyed { dealt: 80.0 }
    );
    assert_eq!(
        world.damage_entity(id, hit(5.0, 0.0), &mut output),
        DamageOutcome::Ignored
    );
    assert!(!world.entities().contains(id));
}

#[test]
fn harpoons_reel_loose_loot_back_toward_the_rig() {
    let mut world = range(WeaponKind::Harpoon);
    let drop = world.spawn_item_drop(
        20.0 * TILE_SIZE,
        31.5 * TILE_SIZE,
        (0.0, 0.0),
        ItemStack::new(ItemKind::Mineral(MineralKind::Gold), 1),
        None,
    );
    let _ = hold(&mut world, trigger(false, (127, 0)), 40);
    let start = world.entities().get(drop).expect("drop").body.x;
    let events = hold(&mut world, trigger(true, (127, 3)), 60);
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::ProjectileImpact {
                surface: ImpactSurface::Loot,
                ..
            }
        )),
        "{events:?} {:?}",
        world.entities().get(drop).map(|entity| entity.body)
    );
    let after = world.entities().get(drop).expect("drop").body.x;
    assert!(after < start - TILE_SIZE, "reeled in: {start} -> {after}");
}

#[test]
fn friendly_fire_is_off_shots_fly_through_crewmates() {
    let mut world = range(WeaponKind::ScatterBlaster);
    world.add_player(OTHER);
    let other = world.player_mut(OTHER).expect("other");
    other.x = 15.0 * TILE_SIZE;
    other.y = 31.5 * TILE_SIZE;
    other.velocity_x = 0.0;
    other.velocity_y = 0.0;
    let hull = other.hull;
    let target = dummy(&mut world, 17.0, 100.0, 0.0);
    let _ = hold(&mut world, trigger(true, (127, 0)), 30);
    assert!((world.player(OTHER).expect("other").hull - hull).abs() < f32::EPSILON);
    assert!(
        health_of(&world, target).expect("alive") < 100.0,
        "pellets reached the target"
    );
}

#[test]
fn shooting_a_charge_sets_it_off() {
    let mut world = range(WeaponKind::RivetGun);
    // A C4 charge stuck to the floor, waiting for its detonator.
    let floor_y = 32.0f32.mul_add(TILE_SIZE, -7.0);
    world.insert_charge(PlacedBomb {
        id: 1,
        x: 20.0 * TILE_SIZE,
        y: floor_y,
        previous_x: 20.0 * TILE_SIZE,
        previous_y: floor_y,
        detonation_tick: crate::explosive::REMOTE_FUSE,
        kind: ExplosiveKind::C4,
        stuck: true,
        direction: crate::BlastDirection::Down,
        owner: Some(PLAYER),
        ..PlacedBomb::default()
    });
    let events = hold(&mut world, trigger(true, (127, 8)), 40);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::ExplosiveDetonated {
            kind: ExplosiveKind::C4,
            ..
        }
    )));
}

#[test]
fn a_railgun_slug_sets_off_a_charge_on_its_line() {
    let mut world = range(WeaponKind::Railgun);
    // A C4 charge stuck level with the muzzle, six tiles out.
    let y = 31.5f32.mul_add(TILE_SIZE, -crate::weapon::MUZZLE_RISE);
    world.insert_charge(PlacedBomb {
        id: 1,
        x: 18.5 * TILE_SIZE,
        y,
        previous_x: 18.5 * TILE_SIZE,
        previous_y: y,
        detonation_tick: crate::explosive::REMOTE_FUSE,
        kind: ExplosiveKind::C4,
        stuck: true,
        owner: Some(PLAYER),
        ..PlacedBomb::default()
    });
    let events = hold(&mut world, trigger(true, (127, 0)), 10);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::ProjectileImpact {
            surface: ImpactSurface::Charge,
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::ExplosiveDetonated {
            kind: ExplosiveKind::C4,
            ..
        }
    )));
}

#[test]
fn the_cutter_beam_melts_targets_and_slices_rock_slowly() {
    let mut world = range(WeaponKind::PlasmaCutter);
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: 16, y: 31 }, TileKind::Stone);
    // A few pulses do not get through stone...
    let _ = hold(&mut world, trigger(true, (127, 0)), 12);
    assert_eq!(kind_at(&world, 16, 31), TileKind::Stone);
    assert!(
        world.active_beam(PLAYER).is_some(),
        "the beam draws while held"
    );
    // ...but holding it does.
    let _ = hold(&mut world, trigger(true, (127, 0)), 60);
    assert_eq!(kind_at(&world, 16, 31), TileKind::Air);
    let target = dummy(&mut world, 17.5, 100.0, 0.0);
    let _ = hold(&mut world, trigger(true, (127, 0)), 30);
    assert!(health_of(&world, target).expect("alive") < 100.0);
    let _ = hold(&mut world, trigger(false, (127, 0)), 1);
    assert!(world.active_beam(PLAYER).is_none());
}

#[test]
fn blasts_damage_targets_by_distance_and_emps_only_hurt_machines() {
    let mut world = range(WeaponKind::RivetGun);
    let near = dummy(&mut world, 20.0, 300.0, 0.0);
    let far = dummy(&mut world, 23.0, 300.0, 0.0);
    world.insert_charge(PlacedBomb {
        id: 1,
        x: 20.6 * TILE_SIZE,
        y: 31.5 * TILE_SIZE,
        previous_x: 20.6 * TILE_SIZE,
        previous_y: 31.5 * TILE_SIZE,
        detonation_tick: world.simulation_tick().next(),
        kind: ExplosiveKind::C4,
        owner: Some(PLAYER),
        ..PlacedBomb::default()
    });
    let events = hold(&mut world, trigger(false, (127, 0)), 2);
    let near_left = health_of(&world, near).expect("alive");
    let far_left = health_of(&world, far).expect("alive");
    assert!(
        near_left < far_left && far_left < 300.0,
        "{near_left} {far_left}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::EntityDamaged { .. }))
    );
    // EMP: a machine takes damage, a plain target does not.
    let mut world = range(WeaponKind::RivetGun);
    let plain = dummy(&mut world, 20.0, 300.0, 0.0);
    let machine = dummy(&mut world, 21.0, 300.0, 0.0);
    let _ = world.entities_mut().update(machine, |entity| {
        if let Some(health) = entity.state.health_mut() {
            health.machine = true;
        }
    });
    world.insert_charge(PlacedBomb {
        id: 2,
        x: 20.5 * TILE_SIZE,
        y: 31.5 * TILE_SIZE,
        previous_x: 20.5 * TILE_SIZE,
        previous_y: 31.5 * TILE_SIZE,
        detonation_tick: world.simulation_tick().next(),
        kind: ExplosiveKind::Emp,
        ..PlacedBomb::default()
    });
    let _ = hold(&mut world, trigger(false, (127, 0)), 2);
    assert!((health_of(&world, plain).expect("alive") - 300.0).abs() < f32::EPSILON);
    assert!(health_of(&world, machine).expect("alive") < 300.0);
}

#[test]
#[allow(clippy::too_many_lines, reason = "one shop walkthrough")]
fn weapons_are_bought_upgraded_and_mounted_at_the_shack() {
    use crate::summary::PlayerScopedCommandOutcome::{Applied, IgnoredUnavailable};
    let mut world = range(WeaponKind::RivetGun);
    let player = world.player_mut(PLAYER).expect("player");
    player.credits = 20_000;
    let rivet_mk = player.weapon_rack.mk(WeaponKind::RivetGun);
    assert_eq!(rivet_mk, Some(0));
    let buy = |world: &mut WorldState, request| world.apply_service_request(PLAYER, request);
    // The railgun needs a level 3 shack.
    assert_eq!(
        buy(
            &mut world,
            SessionServiceRequest::BuyWeapon {
                weapon: WeaponKind::Railgun
            }
        ),
        IgnoredUnavailable
    );
    assert_eq!(
        buy(
            &mut world,
            SessionServiceRequest::BuyWeapon {
                weapon: WeaponKind::Harpoon
            }
        ),
        Applied
    );
    assert_eq!(
        buy(
            &mut world,
            SessionServiceRequest::BuyWeapon {
                weapon: WeaponKind::Harpoon
            }
        ),
        IgnoredUnavailable,
        "already owned"
    );
    for _ in 0..MAX_WEAPON_MK {
        assert_eq!(
            buy(
                &mut world,
                SessionServiceRequest::UpgradeWeapon {
                    weapon: WeaponKind::RivetGun
                }
            ),
            Applied
        );
    }
    assert_eq!(
        buy(
            &mut world,
            SessionServiceRequest::UpgradeWeapon {
                weapon: WeaponKind::RivetGun
            }
        ),
        IgnoredUnavailable,
        "maxed"
    );
    let player = world.player(PLAYER).expect("player");
    assert_eq!(
        player.mounted_weapon.map(|mounted| mounted.mk),
        Some(MAX_WEAPON_MK)
    );
    assert_eq!(
        buy(&mut world, SessionServiceRequest::UpgradeBattery),
        Applied
    );
    assert_eq!(
        buy(
            &mut world,
            SessionServiceRequest::BuyAmmo {
                ammo: AmmoKind::Rocket
            }
        ),
        Applied
    );
    let player = world.player(PLAYER).expect("player");
    assert_eq!(player.weapon_rack.battery_level, 1);
    assert_eq!(player.ammo(AmmoKind::Rocket), AmmoKind::Rocket.pack().0);
    assert_eq!(player.cargo_used(), 0, "ammo takes no cargo slots");
    // Mounting: owned weapons only, through the tick.
    let mount = |world: &mut WorldState, weapon| {
        let inputs = TickInputs::default().act(PLAYER, PlayerCommand::MountWeapon { weapon });
        world.simulate_tick(world.simulation_tick(), &inputs)
    };
    let output = mount(&mut world, Some(WeaponKind::Harpoon));
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::WeaponMounted {
            weapon: Some(WeaponKind::Harpoon),
            ..
        }
    )));
    let _ = mount(&mut world, Some(WeaponKind::Railgun));
    assert_eq!(
        world
            .player(PLAYER)
            .and_then(|player| player.mounted_weapon)
            .map(|mounted| mounted.kind),
        Some(WeaponKind::Harpoon),
        "not owned: ignored"
    );
    // Away from the shack the arms rack is closed.
    let outcome = world.simulate_tick(
        world.simulation_tick(),
        &TickInputs::default().act(
            PLAYER,
            PlayerCommand::Service(SessionServiceRequest::UpgradeBattery),
        ),
    );
    assert_eq!(outcome.action_outcomes[0].outcome, IgnoredUnavailable);
}

#[test]
fn projectiles_are_deterministic_and_survive_saves_and_replication_mid_flight() {
    let run = |world: &mut WorldState| {
        let target = dummy(world, 30.0, 400.0, 2.0);
        let _ = hold(world, trigger(true, (120, -30)), 5);
        target
    };
    let mut first = range(WeaponKind::ScatterBlaster);
    let mut second = range(WeaponKind::ScatterBlaster);
    let _ = run(&mut first);
    let _ = run(&mut second);
    assert!(
        first.entities().count_of(EntityKind::Projectile) > 3,
        "pellets in flight"
    );
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
    let json = serde_json::to_string(&first).expect("save");
    let mut restored: WorldState = serde_json::from_str(&json).expect("load");
    // A replica built from entity records and the per-player sections.
    let mut replica = restored.clone();
    {
        use crate::entity::EntityDelta;
        let despawned = replica.entities().iter().map(|entity| entity.id).collect();
        let updates = first.entities().iter().cloned().collect();
        let wire: EntityDelta = postcard::from_bytes(
            &postcard::to_allocvec(&EntityDelta { updates, despawned }).expect("encode"),
        )
        .expect("decode");
        replica.entities_mut().apply_delta(wire);
    }
    for world in [&mut first, &mut restored, &mut replica] {
        let _ = hold(world, trigger(true, (120, -30)), 90);
    }
    assert_eq!(
        first.simulation_state_hash(),
        restored.simulation_state_hash()
    );
    assert_eq!(
        first.simulation_state_hash(),
        replica.simulation_state_hash()
    );
}

#[test]
fn test_mode_rigs_carry_every_weapon_at_its_top_tier() {
    let mut world = WorldState::from_import(&WorldImport::new_game(PLAYER));
    world.enable_test_mode();
    let player = world.player(PLAYER).expect("player");
    for weapon in WeaponKind::ALL {
        assert_eq!(player.weapon_rack.mk(weapon), Some(MAX_WEAPON_MK));
    }
    assert!(player.mounted_weapon.is_some());
    assert!(player.ammo(AmmoKind::Rocket) >= 40);
    let _ = SimulationTick::new(0);
    let _ = Health::new(1.0, 0.0, 1.0);
}
