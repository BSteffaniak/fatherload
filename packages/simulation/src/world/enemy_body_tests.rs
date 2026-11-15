//! Enemy bodies obey the same terrain as rigs and never overlap rigs or pilots: no enemy of any
//! family ends a tick inside solid tiles over long seeded fights (every difficulty, several tick
//! rates, blasts and weapon knockback included), burrowers dig real tunnels (soft rock only),
//! enemies left in rock by a terrain change are resolved out the short way, bodies stop at
//! contact (and still bite), and point-blank shots hit an enemy touching the rig.

use super::WorldImport;
use crate::{
    ControlState, ExplosiveKind, PlacedBomb, PlayerId, SimulationParams, SimulationTick,
    TickInputs, WeaponControl, WorldState,
    enemy::{EnemyDifficulty, EnemyKind, EnemyMode, Locomotion},
    entity::{Body, EntityId, EntityKind, EntityState},
    event::GameplayEventKind,
    physics,
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    weapon::{MUZZLE_RISE, WeaponKind},
};

use super::{enemies::RIG_CONTACT_HALF, movement::AUTHORITATIVE_PLAYER_RADIUS};

const PLAYER: PlayerId = PlayerId::new(0);
/// The arena: a cavern far below the surface, east of the town (as in the enemy tests).
const LEFT: i32 = 230;
const RIGHT: i32 = 330;
const TOP: i32 = 200;
const FLOOR: i32 = 232;
/// Deepest an enemy may overlap a rig's or pilot's box at the end of a tick, in pixels. Bodies
/// are pushed apart to exactly touching; the rig moves before enemies in a tick, and a push that
/// would put the enemy into rock is refused, so a rig ramming an enemy pinned against a wall can
/// leave a little overlap for a tick.
const OVERLAP_TOLERANCE: f32 = 1.0;

fn px(tile: f32) -> f32 {
    tile * TILE_SIZE
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

#[allow(clippy::cast_precision_loss, reason = "small tile indices")]
fn floor_y(radius: f32) -> f32 {
    (FLOOR as f32).mul_add(TILE_SIZE, -radius) - 1.0
}

fn no_spawns(world: &mut WorldState) {
    for cy in (TOP - 40) / 16..=(FLOOR + 40) / 16 {
        for cx in (LEFT - 60) / 16..=(RIGHT + 60) / 16 {
            world.enemies.cool_chunk(
                crate::terrain::ChunkPosition { x: cx, y: cy },
                SimulationTick::new(u64::MAX),
            );
        }
    }
}

/// A rugged cavern at `hz`: pillars, a thin shelf, a low tunnel and a soft dirt bank with ore
/// in it, the rig parked on the floor in the middle, armed with a rivet gun.
fn arena(hz: u32) -> WorldState {
    let mut world = WorldState::from_import_with_params(
        &WorldImport::new_game(PLAYER),
        SimulationParams::new(hz),
    );
    fill(
        &mut world,
        LEFT - 6..RIGHT + 6,
        TOP - 6..FLOOR + 6,
        TileKind::Stone,
    );
    fill(&mut world, LEFT..RIGHT, TOP..FLOOR, TileKind::Air);
    // Pillars from the floor and from the ceiling.
    fill(&mut world, 252..254, FLOOR - 9..FLOOR, TileKind::Stone);
    fill(&mut world, 296..298, FLOOR - 7..FLOOR, TileKind::HardRock);
    fill(&mut world, 270..272, TOP..TOP + 12, TileKind::Stone);
    // A one-tile shelf and a low tunnel under a slab.
    fill(&mut world, 238..250, 214..215, TileKind::Stone);
    fill(&mut world, 305..318, FLOOR - 3..FLOOR - 2, TileKind::Stone);
    // A soft bank on the west wall with an ore seam (burrowers may not eat ore).
    fill(
        &mut world,
        LEFT..LEFT + 5,
        FLOOR - 12..FLOOR,
        TileKind::Dirt,
    );
    fill(
        &mut world,
        LEFT + 2..LEFT + 3,
        FLOOR - 10..FLOOR - 2,
        TileKind::Ore(MineralKind::Copper),
    );
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(280.5);
    player.y = floor_y(AUTHORITATIVE_PLAYER_RADIUS);
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    let _ = player.receive_weapon(WeaponKind::RivetGun);
    player.weapon.energy = player.battery_capacity();
    world
}

/// One of every kind at an open spot in the arena.
fn spawn_every_kind(world: &mut WorldState) -> Vec<EntityId> {
    let mut ids = Vec::new();
    for (index, kind) in (0_u16..).zip(EnemyKind::ALL) {
        let column = f32::from(index).mul_add(6.0, 258.5);
        let column = if (295.0..299.0).contains(&column) {
            column + 4.0
        } else {
            column
        };
        let y = match kind.stats().locomotion {
            Locomotion::Walking | Locomotion::Stationary => floor_y(kind.stats().radius),
            Locomotion::Flying => px(f32::from(index % 4).mul_add(3.0, 216.5)),
            // In a pocket against the soft bank.
            Locomotion::Burrowing => px(225.5),
        };
        let x = if kind == EnemyKind::Burrower {
            px(235.5)
        } else {
            px(column)
        };
        ids.push(world.spawn_enemy(kind, x, y));
    }
    ids
}

fn enemy_bodies(world: &WorldState) -> Vec<(EntityId, Body, EnemyKind, EnemyMode)> {
    world
        .entities()
        .of_kind(EntityKind::Enemy)
        .filter_map(|entity| match entity.state {
            EntityState::Enemy(state) => Some((entity.id, entity.body, state.kind, state.mode)),
            _ => None,
        })
        .collect()
}

/// Overlap depth of an enemy's contact box (half extents `own`) with a box (0 when apart or
/// touching).
fn overlap(body: &Body, own: (f32, f32), centre: (f32, f32), half: (f32, f32)) -> f32 {
    let depth_x = own.0 + half.0 - (body.x - centre.0).abs();
    let depth_y = own.1 + half.1 - (body.y - centre.1).abs();
    depth_x.min(depth_y).max(0.0)
}

/// Asserts the body invariants for every enemy now: out of rock, off the rig and pilots.
fn assert_bodies(world: &WorldState, context: &str) -> f32 {
    let mut deepest: f32 = 0.0;
    for (id, body, kind, _) in enemy_bodies(world) {
        let radius = kind.stats().radius;
        assert!(
            !physics::collides(world.terrain(), body.x, body.y, radius),
            "{context}: {kind:?} #{} inside rock at ({}, {})",
            id.get(),
            body.x,
            body.y
        );
        for player in world.players.values().filter(|player| player.hull > 0.0) {
            let depth = overlap(
                &body,
                kind.contact_half(),
                (player.x, player.y),
                RIG_CONTACT_HALF,
            );
            deepest = deepest.max(depth);
        }
    }
    deepest
}

/// The rig's scripted dance: drive, hover, stop, shooting with auto-aim the whole time.
fn dance(tick: u64, hz: u64) -> ControlState {
    let phase = (tick / (hz * 3 / 2)) % 6;
    let (axis, thrust) = match phase {
        0 => (1.0, false),
        1 => (-1.0, false),
        2 => (0.0, true),
        3 => (-1.0, true),
        4 => (1.0, true),
        _ => (0.0, false),
    };
    ControlState::new(axis, thrust, false).with_weapon(WeaponControl {
        fire: true,
        auto_aim: true,
        aim_x: if tick.is_multiple_of(2) { 127 } else { -127 },
        aim_y: 0,
    })
}

/// A long fight: every kind plus the director's spawns, the rig dancing and shooting, a bomb
/// every few seconds next to an enemy. Checks the invariants after every tick; returns the
/// deepest rig overlap seen, the rig hits and how many enemy-ticks were checked.
fn long_fight(difficulty: EnemyDifficulty, hz: u32, seconds: u64) -> (f32, usize, usize) {
    let mut world = arena(hz);
    world.set_enemy_difficulty(difficulty);
    let _ = spawn_every_kind(&mut world);
    let hz = u64::from(hz);
    let mut deepest: f32 = 0.0;
    let mut struck = 0;
    let mut checked = 0;
    for tick in 0..seconds * hz {
        if tick % (hz * 4) == hz {
            // A bomb by the first enemy: blasts push bodies toward walls.
            if let Some((_, body, _, _)) = enemy_bodies(&world).first() {
                let (x, y) = (body.x + 6.0, body.y + 4.0);
                world.insert_charge(PlacedBomb {
                    id: 10_000 + tick,
                    x,
                    y,
                    previous_x: x,
                    previous_y: y,
                    kind: ExplosiveKind::Bomb,
                    stage: 1,
                    detonation_tick: SimulationTick::new(world.simulation_tick().get() + 2),
                    stuck: true,
                    ..PlacedBomb::default()
                });
            }
        }
        let inputs = TickInputs::held(PLAYER, dance(tick, hz));
        let output = world.simulate_tick(world.simulation_tick(), &inputs);
        struck += output
            .events
            .iter()
            .filter(|event| matches!(event.kind, GameplayEventKind::RigStruck { .. }))
            .count();
        // Keep the rig in the fight (and its gun loaded).
        let player = world.player_mut(PLAYER).expect("player");
        player.hull = player.max_hull();
        player.fuel = player.fuel_capacity;
        player.weapon.energy = player.battery_capacity();
        checked += world.entities().count_of(EntityKind::Enemy);
        deepest = deepest.max(assert_bodies(
            &world,
            &format!("{difficulty:?} at {hz} Hz, tick {tick}"),
        ));
    }
    (deepest, struck, checked)
}

#[test]
fn no_enemy_ever_ends_a_tick_inside_rock_or_inside_the_rig() {
    for difficulty in [EnemyDifficulty::Normal, EnemyDifficulty::Hard] {
        let (deepest, struck, checked) = long_fight(difficulty, 60, 45);
        assert!(checked > 60 * 45 * 4, "a real fight: {checked}");
        assert!(
            struck > 0,
            "{difficulty:?}: enemies still reach and bite the rig"
        );
        assert!(
            deepest <= OVERLAP_TOLERANCE,
            "{difficulty:?}: an enemy overlapped the rig by {deepest} px"
        );
    }
}

#[test]
fn bodies_hold_at_every_tick_rate() {
    for hz in [30, 120] {
        let (deepest, _, _) = long_fight(EnemyDifficulty::Hard, hz, 20);
        assert!(
            deepest <= OVERLAP_TOLERANCE,
            "{hz} Hz: an enemy overlapped the rig by {deepest} px"
        );
    }
}

#[test]
fn a_bat_stops_at_contact_bites_and_backs_off() {
    let mut world = arena(60);
    no_spawns(&mut world);
    let rig = (px(280.5), floor_y(AUTHORITATIVE_PLAYER_RADIUS));
    let bat = world.spawn_enemy(EnemyKind::CaveBat, rig.0 + px(3.0), rig.1 - px(1.0));
    let mut struck = 0;
    let mut nearest = f32::MAX;
    let mut farthest_after_bite: f32 = 0.0;
    for _ in 0..60 * 6 {
        let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        let bit = output
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::RigStruck { .. }));
        let player = world.player_mut(PLAYER).expect("player");
        player.hull = player.max_hull();
        let (px_, py_) = (player.x, player.y);
        let Some((_, body, _, _)) = enemy_bodies(&world).into_iter().find(|(id, ..)| *id == bat)
        else {
            panic!("the bat is alive");
        };
        let depth = overlap(
            &body,
            EnemyKind::CaveBat.contact_half(),
            (px_, py_),
            RIG_CONTACT_HALF,
        );
        assert!(
            depth <= OVERLAP_TOLERANCE,
            "the bat sat in the rig: {depth}"
        );
        let distance = (body.x - px_).hypot(body.y - py_);
        nearest = nearest.min(distance);
        if bit {
            struck += 1;
        }
        if struck > 0 {
            farthest_after_bite = farthest_after_bite.max(distance);
        }
    }
    assert!(struck >= 2, "it bit, more than once: {struck}");
    assert!(
        nearest
            >= EnemyKind::CaveBat
                .contact_half()
                .1
                .min(EnemyKind::CaveBat.contact_half().0)
                + RIG_CONTACT_HALF.1.min(RIG_CONTACT_HALF.0)
                - OVERLAP_TOLERANCE,
        "touching at closest: {nearest}"
    );
    assert!(
        farthest_after_bite > px(1.5),
        "it backed off between bites: {farthest_after_bite}"
    );
}

#[test]
fn melee_lunges_end_at_contact_and_recoil() {
    for kind in [
        EnemyKind::RockMimic,
        EnemyKind::Shellback,
        EnemyKind::MiningDrone,
        EnemyKind::RogueExcavator,
    ] {
        let mut world = arena(60);
        no_spawns(&mut world);
        let radius = kind.stats().radius;
        let y = match kind.stats().locomotion {
            Locomotion::Flying => floor_y(AUTHORITATIVE_PLAYER_RADIUS),
            _ => floor_y(radius),
        };
        // Inside a dormant mimic's waking range (2.5 tiles) and every lunge range.
        let id = world.spawn_enemy(kind, px(283.0), y);
        let mut struck = 0;
        for _ in 0..60 * 8 {
            let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
            struck += output
                .events
                .iter()
                .filter(|event| matches!(event.kind, GameplayEventKind::RigStruck { .. }))
                .count();
            let player = world.player_mut(PLAYER).expect("player");
            player.hull = player.max_hull();
            let _ = assert_bodies(&world, &format!("{kind:?}"));
            let rig = world.player(PLAYER).expect("player");
            if let Some((_, body, ..)) = enemy_bodies(&world).into_iter().find(|e| e.0 == id) {
                let depth = overlap(&body, kind.contact_half(), (rig.x, rig.y), RIG_CONTACT_HALF);
                assert!(
                    depth <= OVERLAP_TOLERANCE,
                    "{kind:?} inside the rig: {depth}"
                );
            }
        }
        assert!(struck >= 1, "{kind:?} still lands its lunge at contact");
    }
}

#[test]
fn pilots_on_foot_are_touched_never_overlapped() {
    let mut world = arena(60);
    no_spawns(&mut world);
    // Leave the rig and stand a few tiles off it.
    let inputs = TickInputs::default().act(PLAYER, crate::PlayerCommand::TogglePilot);
    let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    for _ in 0..20 {
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    }
    let Some((pilot_id, ..)) = world.pilot_of(PLAYER) else {
        // The pilot needs room to step out; the arena floor has it.
        panic!("the pilot is out");
    };
    let bat = world.spawn_enemy(EnemyKind::CaveBat, px(276.5), px(228.0));
    let mut struck = 0;
    for _ in 0..60 * 5 {
        let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        struck += output
            .events
            .iter()
            .filter(|event| matches!(event.kind, GameplayEventKind::PilotHurt { .. }))
            .count();
        world.entities.update(pilot_id, |entity| {
            if let EntityState::Pilot(pilot) = &mut entity.state {
                pilot.health = pilot.max_health;
            }
        });
        let Some((_, pilot_body, pilot)) = world.pilot_of(PLAYER) else {
            panic!("the pilot is alive");
        };
        let middle = pilot.middle(pilot_body);
        let half = (crate::pilot::PILOT_HALF_WIDTH, pilot.height() * 0.5);
        if let Some((_, body, ..)) = enemy_bodies(&world).into_iter().find(|e| e.0 == bat) {
            let depth = overlap(&body, EnemyKind::CaveBat.contact_half(), middle, half);
            assert!(
                depth <= OVERLAP_TOLERANCE,
                "the bat sat in the pilot: {depth}"
            );
        }
    }
    assert!(struck >= 1, "the bat still bites a pilot on foot");
}

#[test]
fn point_blank_shots_hit_an_enemy_touching_the_rig() {
    for weapon in [
        WeaponKind::RivetGun,
        WeaponKind::ScatterBlaster,
        WeaponKind::PlasmaCutter,
        WeaponKind::Railgun,
    ] {
        // Aims: straight up at a bat sitting on the rig's roof (its centre between the
        // hardpoint and the muzzle), and sideways at one hugging the hardpoint.
        for (offset, aim) in [((0.0, -16.0), (0, -127)), ((15.0, -14.0), (127, 0))] {
            let mut world = arena(60);
            no_spawns(&mut world);
            let player = world.player_mut(PLAYER).expect("player");
            let _ = player.receive_weapon(weapon);
            let _ = player.mount_weapon(Some(weapon));
            player.weapon.energy = player.battery_capacity();
            for ammo in crate::weapon::AmmoKind::ALL {
                player.cargo_bay.add(ammo.item(), 50);
            }
            let (x, y) = (player.x, player.y);
            // A tough target so a single hit doesn't kill it: a shellback hit point blank.
            let target = world.spawn_enemy(EnemyKind::CaveBat, x + offset.0, y + offset.1);
            let before = world
                .entities()
                .get(target)
                .and_then(|entity| entity.state.health())
                .map(|health| health.current)
                .expect("bat");
            let controls = ControlState::new(0.0, false, false).with_weapon(WeaponControl {
                fire: true,
                auto_aim: false,
                aim_x: aim.0,
                aim_y: aim.1,
            });
            let output =
                world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
            let fired = output
                .events
                .iter()
                .any(|event| matches!(event.kind, GameplayEventKind::WeaponFired { .. }));
            assert!(fired, "{weapon:?} fired");
            let after = world
                .entities()
                .get(target)
                .and_then(|entity| entity.state.health())
                .map_or(0.0, |health| health.current);
            assert!(
                after < before,
                "{weapon:?} aimed {aim:?} hit the bat at {offset:?} point blank ({before} -> \
                 {after})"
            );
            let _ = MUZZLE_RISE;
        }
    }
}

#[test]
fn burrowers_dig_real_tunnels_through_soft_rock_and_never_eat_ore() {
    let mut world = arena(60);
    no_spawns(&mut world);
    // Bury the rig's corner of the arena in dirt: the burrower must dig to it.
    fill(&mut world, 258..276, FLOOR - 6..FLOOR, TileKind::Dirt);
    fill(
        &mut world,
        263..264,
        FLOOR - 6..FLOOR,
        TileKind::Ore(MineralKind::Iron),
    );
    let ore_before: Vec<TilePosition> = (FLOOR - 6..FLOOR)
        .map(|y| TilePosition { x: 263, y })
        .collect();
    // A pocket in the dirt, nine tiles from the rig (inside its hearing).
    fill(&mut world, 268..269, FLOOR - 2..FLOOR - 1, TileKind::Air);
    // Soft ground under the rig to drill into (a stock drill hears like this).
    fill(&mut world, 250..290, FLOOR..FLOOR + 16, TileKind::Stone);
    fill(&mut world, 276..280, FLOOR..FLOOR + 1, TileKind::Dirt);
    let burrower = world.spawn_enemy(EnemyKind::Burrower, px(268.5), px(229.5));
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(277.5);
    let air_before = (258..276)
        .flat_map(|x| (FLOOR - 6..FLOOR).map(move |y| TilePosition { x, y }))
        .filter(|tile| !world.terrain().is_solid_at(*tile))
        .count();
    // Drilling draws it (it hears the drill through the dirt).
    let drilling = TickInputs::held(PLAYER, ControlState::new(0.0, false, true));
    let mut moved: f32 = 0.0;
    for tick in 0..60 * 10 {
        // A short burst gives the rig away; then it sits still (and the burrower comes to it).
        let inputs = if tick < 45 {
            drilling.clone()
        } else {
            TickInputs::default()
        };
        let _ = world.simulate_tick(world.simulation_tick(), &inputs);
        let player = world.player_mut(PLAYER).expect("player");
        player.hull = player.max_hull();
        let _ = assert_bodies(&world, "burrower");
        if let Some((_, body, ..)) = enemy_bodies(&world).into_iter().find(|e| e.0 == burrower) {
            moved = moved.max((body.x - px(268.5)).abs());
        }
    }
    let air_after = (258..276)
        .flat_map(|x| (FLOOR - 6..FLOOR).map(move |y| TilePosition { x, y }))
        .filter(|tile| !world.terrain().is_solid_at(*tile))
        .count();
    assert!(moved > px(3.0), "it dug toward the rig: {moved}");
    assert!(
        air_after > air_before + 3,
        "a visible tunnel: {air_before} -> {air_after}"
    );
    for tile in ore_before {
        assert!(
            matches!(
                world.terrain().tile(tile).map(|t| t.kind),
                Some(TileKind::Ore(_))
            ),
            "the ore at {tile:?} is untouched"
        );
    }
}

#[test]
fn enemies_left_in_rock_move_out_the_short_way() {
    let mut world = arena(60);
    no_spawns(&mut world);
    // A bat in a one-tile corridor; then the corridor's tile closes over it (a built piece, a
    // boulder settling). It must come out in the corridor, not above its ceiling.
    fill(&mut world, 300..320, 205..208, TileKind::Stone);
    fill(&mut world, 300..320, 206..207, TileKind::Air);
    let bat = world.spawn_enemy(EnemyKind::CaveBat, px(310.5), px(206.5));
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: 310, y: 206 }, TileKind::Stone);
    let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    let (_, body, ..) = enemy_bodies(&world)
        .into_iter()
        .find(|e| e.0 == bat)
        .expect("bat");
    assert!(!physics::collides(world.terrain(), body.x, body.y, 6.0));
    assert!(
        (px(206.0)..px(207.0)).contains(&body.y),
        "still in the corridor: {}",
        body.y / TILE_SIZE
    );
    // Spawns overlapping rock move to open space too.
    let wedged = world.spawn_enemy(EnemyKind::MiningDrone, px(306.5), px(205.6));
    let (_, body, ..) = enemy_bodies(&world)
        .into_iter()
        .find(|e| e.0 == wedged)
        .expect("drone");
    assert!(!physics::collides(world.terrain(), body.x, body.y, 7.0));
}

#[test]
fn a_hard_shove_into_a_wall_never_embeds_an_enemy() {
    let mut world = arena(60);
    no_spawns(&mut world);
    for kind in [
        EnemyKind::CaveBat,
        EnemyKind::Shellback,
        EnemyKind::Burrower,
    ] {
        let radius = kind.stats().radius;
        let y = match kind.stats().locomotion {
            Locomotion::Walking => floor_y(radius),
            _ => px(224.5),
        };
        let id = world.spawn_enemy(kind, px(329.0), y);
        let mut output = crate::TickOutput::default();
        let _ = world.damage_enemy(
            id,
            crate::enemy::Damage::new(
                1.0,
                crate::enemy::DamageKind::Kinetic,
                crate::enemy::DamageSource::World,
            )
            .with_impulse(2_000.0, -600.0),
            &mut output,
        );
        for _ in 0..30 {
            let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
            let _ = assert_bodies(&world, &format!("{kind:?} shoved"));
        }
    }
}

#[test]
fn enemy_bodies_are_deterministic_and_replicate() {
    let run = || {
        let mut world = arena(60);
        world.set_enemy_difficulty(EnemyDifficulty::Hard);
        let _ = spawn_every_kind(&mut world);
        let mut replica: WorldState =
            serde_json::from_str(&serde_json::to_string(&world).expect("save")).expect("load");
        for tick in 0..60 * 10 {
            let inputs = TickInputs::held(PLAYER, dance(tick, 60));
            let _ = world.simulate_tick(world.simulation_tick(), &inputs);
            let _ = replica.simulate_tick(replica.simulation_tick(), &inputs);
        }
        assert_eq!(
            world.simulation_state_hash(),
            replica.simulation_state_hash(),
            "a replica ticking the same inputs agrees"
        );
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
}
