//! World tests of hostile factions and space combat (Wave 12 hostiles): pacing (calm at home,
//! encounters farther out), the faction AIs (pirates hail and take tribute or turn hostile,
//! the swarm flocks, the machines keep formation and telegraph lances), aiming that is always
//! the player's, determinism and tick-rate safety, the escape jump, the cargo pod of a wrecked
//! rig, bosses beaten by a bot with the player's controls, saves, and ground raids.

#![allow(
    clippy::suboptimal_flops,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test bookkeeping"
)]

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WorldState,
    attitude::RigAttitude,
    enemy::EnemyDifficulty,
    field::trig::wrap_angle,
    flight::{
        FlightCommand, FlightPlace, RigFlight,
        space::{self, galaxy_sector_of, home_star, nearest_surface},
    },
    hostiles::{
        Attack, COMBAT_STEP_HZ, CombatCommand, EncounterKind, EncounterState, Faction,
        HostileEvent, ShipClass, ShipGear, ShipMode, Shooter, ShotKind,
        ai::{self, slot_offset},
        combat::{bearing, from_ship_frame, length, nose},
        step_at_tick,
    },
    input::WeaponControl,
    position::WorldPos,
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x5_7A2D;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// A deep world at `hz` with a well-kept rig (every rig upgrade, middling ship gear) and the
/// Star Drive.
fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    // Hostiles find rigs only where enemies are on (a peaceful world's space is calm).
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hazard_gear.arms = ShipGear {
        cannon: 1,
        turret: 1,
        missiles: 1,
        shield: 1,
    };
    rig.hull = rig.max_hull();
    rig.fuel = rig.fuel_capacity;
    rig.credits = 5_000;
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    heart.star_drive.insert(PLAYER);
    world.set_heart(heart);
    world
}

/// Open space `ring` galactic sectors east of home (a sector's corner: clear of every system).
fn open_space(ring: i64) -> WorldPos {
    let (hx, hy) = galaxy_sector_of(home_star());
    let corner = space::galaxy_sector_origin((hx + ring, hy));
    corner.offset(space::tiles(6_000.0), space::tiles(6_000.0))
}

/// The rig put straight into space at `pos`, still, turned `orientation`.
fn launch_to(world: &mut WorldState, pos: WorldPos, orientation: f32) {
    let park = world.space_park().expect("park");
    let capacity = world.air_capacity_of(PLAYER);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.flight = RigFlight {
        place: FlightPlace::Space,
        pos,
        air: capacity,
        exit_column: 12,
        ..RigFlight::default()
    };
    rig.attitude = RigAttitude {
        orientation,
        angular_velocity: 0.0,
    };
    rig.x = park.0;
    rig.y = park.1;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn tick_with(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let output = world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
    output.events.into_iter().map(|event| event.kind).collect()
}

fn act(
    world: &mut WorldState,
    controls: ControlState,
    command: PlayerCommand,
) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::held(PLAYER, controls).act(PLAYER, command);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    output.events.into_iter().map(|event| event.kind).collect()
}

fn hostile_events(events: &[GameplayEventKind]) -> Vec<(HostileEvent, u32)> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Hostile { what, detail, .. } => Some((*what, *detail)),
            _ => None,
        })
        .collect()
}

fn step_now(world: &WorldState) -> u64 {
    step_at_tick(world.simulation_tick().get(), world.tick_rate_hz())
}

/// Starts an encounter of `ships` on the rig (as a roll would).
fn spawn(world: &mut WorldState, faction: Faction, kind: EncounterKind, ships: &[ShipClass]) {
    let step = step_now(world);
    let mut output = crate::TickOutput::default();
    world
        .start_encounter(PLAYER, faction, kind, ships, 3, step, &mut output)
        .expect("encounter");
}

fn seconds(world: &WorldState, seconds: f32) -> u32 {
    (seconds * world.tick_rate_hz() as f32).round() as u32
}

fn rig_pos(world: &WorldState) -> WorldPos {
    world.player(PLAYER).expect("rig").flight.pos
}

#[test]
fn home_is_calm_and_encounters_find_rigs_farther_out() {
    let mut events = Vec::new();
    let mut world = world_at(30);
    let home = open_space(0);
    launch_to(&mut world, home, 0.0);
    let bodies = world.space_bodies_near(home);
    assert!(nearest_surface(&bodies, home).0 > space::tiles(20_000.0));
    for _ in 0..seconds(&world, 240.0) {
        events.extend(tick_with(&mut world, ControlState::default()));
    }
    assert!(
        !hostile_events(&events)
            .iter()
            .any(|(what, _)| *what == HostileEvent::EncounterStarted),
        "the home sector is calm"
    );
    // Four sectors out, something finds the rig within a few minutes.
    let mut world = world_at(30);
    let far = open_space(4);
    launch_to(&mut world, far, 0.0);
    let mut found = None;
    for _ in 0..seconds(&world, 240.0) {
        let events = tick_with(&mut world, ControlState::default());
        if let Some((_, faction)) = hostile_events(&events)
            .into_iter()
            .find(|(what, _)| *what == HostileEvent::EncounterStarted)
        {
            found = Faction::from_code(faction);
            break;
        }
    }
    assert!(found.is_some(), "an encounter four sectors out");
    assert!(!world.hostiles().ships.is_empty());
    // A peaceful world's space stays calm even far out.
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    launch_to(&mut world, far, 0.0);
    for _ in 0..seconds(&world, 240.0) {
        let _ = tick_with(&mut world, ControlState::default());
    }
    assert!(
        world.hostiles().encounters.is_empty(),
        "peaceful space is calm"
    );
}

#[test]
fn encounters_scale_with_distance_and_the_rigs_tier() {
    use crate::hostiles::encounter::{budget, chance_permille, compose, cost};
    assert_eq!(chance_permille(0, 10), 0, "calm at home");
    for ring in 1..8_u8 {
        assert!(chance_permille(ring + 1, 0) >= chance_permille(ring, 0));
        assert!(budget(ring + 1, 0) >= budget(ring, 0));
    }
    assert!(budget(3, 8) > budget(3, 0), "a better-armed rig draws more");
    for faction in Faction::ALL {
        for spend in [2_u32, 5, 9, 14] {
            for roll in 0..20_u64 {
                let ships = compose(faction, spend, roll);
                assert!(!ships.is_empty());
                assert!(ships.iter().all(|ship| ship.faction() == faction));
                let spent: u32 = ships.iter().map(|ship| cost(*ship)).sum();
                assert!(spent <= spend.max(3), "{faction:?} {spend}: {ships:?}");
            }
        }
    }
    let big = compose(Faction::Machine, 14, 3);
    assert!(big.contains(&ShipClass::Lancer), "a formation has a leader");
}

#[test]
fn pirates_hail_and_leave_once_paid() {
    let mut world = world_at(30);
    launch_to(&mut world, open_space(2), 0.0);
    spawn(
        &mut world,
        Faction::Pirates,
        EncounterKind::Roaming,
        &[ShipClass::Cutter, ShipClass::Cutter, ShipClass::Marauder],
    );
    let mut demand = None;
    for _ in 0..seconds(&world, 15.0) {
        let events = tick_with(&mut world, ControlState::default());
        if let Some((_, credits)) = hostile_events(&events)
            .into_iter()
            .find(|(what, _)| *what == HostileEvent::Hailed)
        {
            demand = Some(credits);
            break;
        }
    }
    let demand = demand.expect("the pirates hail");
    assert_eq!(world.hostiles().demand_for(PLAYER), Some(demand));
    // Nobody fired while they hailed.
    assert!(
        world.player(PLAYER).expect("rig").hull >= world.player(PLAYER).expect("rig").max_hull()
    );
    let credits = world.player(PLAYER).expect("rig").credits;
    // The use button pays (as the tribute command does).
    let events = act(
        &mut world,
        ControlState::default(),
        PlayerCommand::Flight(FlightCommand::Use),
    );
    assert!(hostile_events(&events).contains(&(HostileEvent::TributePaid, demand)));
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        credits - demand.min(credits)
    );
    let mut over = false;
    for _ in 0..seconds(&world, 40.0) {
        let events = tick_with(&mut world, ControlState::default());
        if hostile_events(&events)
            .iter()
            .any(|(what, _)| *what == HostileEvent::EncounterOver)
        {
            over = true;
            break;
        }
    }
    assert!(over, "paid pirates leave");
    assert!(world.hostiles().ships.is_empty());
    // A rig that never fired nor was hit keeps a charged deflector and a full rack after it.
    let combat = world.rig_combat(PLAYER);
    let gear = world.player(PLAYER).expect("rig").hazard_gear.arms;
    assert_eq!(
        combat.missiles,
        crate::hostiles::missile_capacity(gear, false)
    );
    assert!((combat.shield - crate::hostiles::shield_capacity(gear)).abs() < 0.01);
}

#[test]
fn pirates_turn_hostile_when_shot() {
    let mut world = world_at(30);
    launch_to(&mut world, open_space(2), 0.0);
    spawn(
        &mut world,
        Faction::Pirates,
        EncounterKind::Roaming,
        &[ShipClass::Cutter, ShipClass::Cutter],
    );
    // Wait for the hail, then turn on them and fire.
    for _ in 0..seconds(&world, 15.0) {
        let _ = tick_with(&mut world, ControlState::default());
        if world.hostiles().demand_for(PLAYER).is_some() {
            break;
        }
    }
    assert!(world.hostiles().demand_for(PLAYER).is_some());
    let mut hostile = false;
    for _ in 0..seconds(&world, 10.0) {
        let controls = aim_at_nearest(&world);
        let events = tick_with(&mut world, controls);
        if hostile_events(&events)
            .iter()
            .any(|(what, _)| *what == HostileEvent::TurnedHostile)
        {
            hostile = true;
            break;
        }
    }
    assert!(hostile, "shooting a hailing pirate starts the fight");
    let encounter = world.hostiles().engaged(PLAYER).expect("still fighting");
    assert_eq!(encounter.state, EncounterState::Fighting);
}

/// Controls that turn the rig toward the nearest ship and fire once lined up (the player's
/// controls only: rotation thrusters and the trigger).
fn aim_at_nearest(world: &WorldState) -> ControlState {
    let rig = world.player(PLAYER).expect("rig");
    let pos = rig.flight.pos;
    let Some(target) = world
        .hostiles()
        .ships
        .iter()
        .min_by(|a, b| pos.distance(a.pos).total_cmp(&pos.distance(b.pos)))
    else {
        return ControlState::default();
    };
    let (dx, dy) = pos.delta(target.pos);
    let error = wrap_angle(bearing(dx, dy) - rig.attitude.orientation);
    let rotate = (error * 3.0 - rig.attitude.angular_velocity * 0.9).clamp(-1.0, 1.0);
    ControlState::default()
        .with_rotation(rotate)
        .with_weapon(WeaponControl {
            fire: error.abs() < 0.12,
            ..WeaponControl::default()
        })
}

#[test]
fn aiming_is_always_the_players() {
    for auto in [false, true] {
        let mut world = world_at(60);
        let angle = 0.9;
        launch_to(&mut world, open_space(2), angle);
        // A ship well off the nose: nothing turns the guns toward it.
        spawn(
            &mut world,
            Faction::Machine,
            EncounterKind::Roaming,
            &[ShipClass::Drone],
        );
        let controls = ControlState::default().with_weapon(WeaponControl {
            fire: true,
            auto_aim: auto,
            // The turret aims to the craft's right.
            aim_x: 100,
            aim_y: 0,
        });
        let _ = tick_with(&mut world, controls);
        let shots: Vec<_> = world
            .hostiles()
            .shots
            .iter()
            .filter(|shot| matches!(shot.from, Shooter::Rig(_)))
            .copied()
            .collect();
        let cannon = shots
            .iter()
            .find(|shot| shot.kind == ShotKind::Cannon)
            .expect("the cannon fired");
        let heading = bearing(cannon.vx, cannon.vy);
        assert!(
            wrap_angle(heading - angle).abs() < 1e-3,
            "the cannon fires down the nose (auto {auto}): {heading}"
        );
        let turret = shots
            .iter()
            .find(|shot| shot.kind == ShotKind::Turret)
            .expect("the turret fired");
        let heading = bearing(turret.vx, turret.vy);
        assert!(
            wrap_angle(heading - (angle + std::f32::consts::FRAC_PI_2)).abs() < 1e-3,
            "the turret fires along the seat's aim, relative to the craft (auto {auto})"
        );
    }
}

#[test]
fn the_swarm_flocks_round_its_prey() {
    let mut world = world_at(30);
    launch_to(&mut world, open_space(3), 0.0);
    spawn(
        &mut world,
        Faction::Swarm,
        EncounterKind::Roaming,
        &[ShipClass::Mite; 8],
    );
    let mut closest = 0.0_f32;
    let mut spread = 0.0_f32;
    let mut samples = 0.0_f32;
    let mut snapshots = 0.0_f32;
    for tick in 0..seconds(&world, 12.0) {
        let _ = tick_with(&mut world, ControlState::default());
        if tick < seconds(&world, 4.0) {
            continue;
        }
        let ships = &world.hostiles().ships;
        let mut nearest_sum = 0.0;
        for (index, a) in ships.iter().enumerate() {
            let mut nearest = f32::MAX;
            for (other, b) in ships.iter().enumerate() {
                if other == index {
                    continue;
                }
                let distance = a.pos.distance(b.pos);
                nearest = nearest.min(distance);
                if other > index {
                    spread += distance;
                    samples += 1.0;
                }
            }
            nearest_sum += nearest;
        }
        closest += nearest_sum / ships.len().max(1) as f32;
        snapshots += 1.0;
    }
    let mean = spread / samples;
    let closest = closest / snapshots;
    assert!(
        closest > 24.0,
        "separation keeps mites apart: {closest} px to the nearest on average"
    );
    assert!(mean < 600.0, "cohesion keeps the flock together: {mean}");
    let rig = rig_pos(&world);
    assert!(
        world
            .hostiles()
            .ships
            .iter()
            .all(|ship| ship.pos.distance(rig) < 1_400.0),
        "the flock hangs round its prey"
    );
    // Mites take turns diving.
    assert!(
        world
            .hostiles()
            .ships
            .iter()
            .any(|ship| ship.phase == 1 || ship.next_attack > 0)
    );
}

#[test]
fn machines_keep_formation_and_telegraph_their_lances() {
    let mut world = world_at(30);
    launch_to(&mut world, open_space(4), 0.0);
    spawn(
        &mut world,
        Faction::Machine,
        EncounterKind::Roaming,
        &[
            ShipClass::Lancer,
            ShipClass::Drone,
            ShipClass::Drone,
            ShipClass::Drone,
            ShipClass::Drone,
        ],
    );
    let mut slot_error = 0.0_f32;
    let mut samples = 0.0_f32;
    let mut warned_for = 0_u64;
    let mut lance_fired = false;
    let mut warning_seen = 0_u64;
    for tick in 0..seconds(&world, 20.0) {
        // Weave from side to side so the formation has to follow.
        let side = if (tick / 60) % 2 == 0 { 0.3 } else { -0.3 };
        let _ = tick_with(&mut world, ControlState::new(side, false, false));
        let ships = world.hostiles().ships.clone();
        let Some(leader) = ships.iter().find(|ship| ship.class == ShipClass::Lancer) else {
            break;
        };
        if let Some(telegraph) = leader.telegraph {
            warned_for = warned_for.max(step_now(&world) - telegraph.since);
            warning_seen = warning_seen.max(telegraph.fire_at - telegraph.since);
        }
        if world
            .hostiles()
            .shots
            .iter()
            .any(|shot| shot.kind == ShotKind::Lance)
        {
            lance_fired = true;
        }
        if tick < seconds(&world, 5.0) {
            continue;
        }
        for drone in ships.iter().filter(|ship| ship.class == ShipClass::Drone) {
            let (sx, sy) = slot_offset(drone.slot);
            let (ox, oy) = from_ship_frame(leader.angle, sx, sy);
            let slot = leader.pos.offset(ox, oy);
            slot_error += drone.pos.distance(slot);
            samples += 1.0;
        }
    }
    let mean = slot_error / samples.max(1.0);
    assert!(mean < 90.0, "drones keep their V slots: {mean} px off");
    assert!(lance_fired, "the lancer fires its lance");
    assert!(
        warning_seen >= ai::lance_warning(),
        "a lance is telegraphed first ({warning_seen} steps)"
    );
}

/// A tick's worth of the hostile state that should not depend on the tick rate.
fn summary(world: &WorldState) -> Vec<(ShipClass, f32, f32, ShipMode)> {
    let rig = rig_pos(world);
    world
        .hostiles()
        .ships
        .iter()
        .map(|ship| {
            let (dx, dy) = rig.delta(ship.pos);
            (ship.class, dx, dy, ship.mode)
        })
        .collect()
}

#[test]
fn combat_is_deterministic_and_tick_rate_safe() {
    let play = |hz: u32| {
        let mut world = world_at(hz);
        launch_to(&mut world, open_space(3), 0.4);
        spawn(
            &mut world,
            Faction::Pirates,
            EncounterKind::Roaming,
            &[ShipClass::Cutter, ShipClass::Marauder],
        );
        for _ in 0..seconds(&world, 12.0) {
            let _ = tick_with(&mut world, ControlState::default());
        }
        world
    };
    let a = play(30);
    let b = play(30);
    assert_eq!(
        a.hostiles(),
        b.hostiles(),
        "the same inputs, the same fight"
    );
    assert_eq!(a.simulation_state_hash(), b.simulation_state_hash());
    let base = summary(&a);
    assert!(!base.is_empty());
    for hz in [60, 120] {
        let other = summary(&play(hz));
        assert_eq!(other.len(), base.len(), "{hz} Hz");
        for (one, two) in base.iter().zip(&other) {
            assert_eq!(one.0, two.0);
            assert_eq!(one.3, two.3, "{hz} Hz: the same course");
            let gap = length(one.1 - two.1, one.2 - two.2);
            assert!(gap < 24.0, "{hz} Hz: {gap} px apart");
        }
    }
    // Every tick rate runs the same combat steps a second.
    for hz in [30, 60, 120] {
        assert_eq!(step_at_tick(u64::from(hz), hz), COMBAT_STEP_HZ);
    }
}

#[test]
fn an_escape_jump_always_gets_away() {
    let mut world = world_at(30);
    launch_to(&mut world, open_space(5), 0.0);
    spawn(
        &mut world,
        Faction::Machine,
        EncounterKind::Boss,
        &[
            ShipClass::Arbiter,
            ShipClass::Lancer,
            ShipClass::Drone,
            ShipClass::Drone,
        ],
    );
    // No fuel, no waypoint, the drive cold, under fire: the warp button still spools a jump.
    world.player_mut(PLAYER).expect("rig").fuel = 0.0;
    for _ in 0..seconds(&world, 3.0) {
        let _ = tick_with(&mut world, ControlState::default());
    }
    let start = rig_pos(&world);
    let events = act(
        &mut world,
        ControlState::default(),
        PlayerCommand::Flight(FlightCommand::Warp),
    );
    assert!(hostile_events(&events).contains(&(HostileEvent::EscapeSpooling, 0)));
    let mut escaped = false;
    for _ in 0..seconds(&world, 4.0) {
        let events = tick_with(&mut world, ControlState::default());
        if hostile_events(&events).contains(&(HostileEvent::Escaped, 0)) {
            escaped = true;
            break;
        }
    }
    assert!(escaped, "the jump went");
    assert!(
        world.hostiles().engaged(PLAYER).is_none(),
        "the fight is over"
    );
    assert!(world.hostiles().ships.is_empty());
    assert!(rig_pos(&world).distance(start) > space::tiles(10_000.0));
    assert!(world.player(PLAYER).expect("rig").hull > 0.0);
}

#[test]
fn a_lair_meets_the_rig_and_an_escape_jump_gets_away_from_it() {
    let mut world = world_at(60);
    let lair = world.boss_lair(Faction::Pirates).expect("a pirate lair");
    // Right in the lair: the boss comes.
    launch_to(&mut world, lair.pos, 0.0);
    let mut met = false;
    for _ in 0..seconds(&world, 3.0) {
        let events = tick_with(&mut world, ControlState::default());
        if hostile_events(&events)
            .iter()
            .any(|(what, _)| *what == HostileEvent::BossAppeared)
        {
            met = true;
            break;
        }
    }
    assert!(met, "the Pirate King waits in his lair");
    assert!(world.hostiles().boss().is_some());
    // The escape jump (30,000 tiles) is shorter than the lair's reach (45,000): the rest after
    // it must keep the boss from finding the rig again straight away.
    let _ = act(
        &mut world,
        ControlState::default(),
        PlayerCommand::Flight(FlightCommand::Warp),
    );
    let mut escaped = false;
    for _ in 0..seconds(&world, 4.0) {
        let events = tick_with(&mut world, ControlState::default());
        if hostile_events(&events)
            .iter()
            .any(|(what, _)| *what == HostileEvent::Escaped)
        {
            escaped = true;
            break;
        }
    }
    assert!(escaped, "the jump went");
    assert!(
        rig_pos(&world).distance(lair.pos) < space::tiles(45_000.0),
        "still inside the lair's reach (the case this test is about)"
    );
    for _ in 0..seconds(&world, 30.0) {
        let events = tick_with(&mut world, ControlState::default());
        assert!(
            !hostile_events(&events)
                .iter()
                .any(|(what, _)| *what == HostileEvent::EncounterStarted),
            "an escaped rig rests before anything finds it again"
        );
    }
    assert!(world.hostiles().engaged(PLAYER).is_none());
}

#[test]
fn a_rig_wrecked_in_a_fight_leaves_a_pod_and_is_towed_home() {
    let mut world = world_at(30);
    launch_to(&mut world, open_space(3), 0.0);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hull = 1.0;
    rig.cargo_bay.add(
        crate::item::ItemKind::Mineral(crate::terrain::MineralKind::Gold),
        10,
    );
    let mut combat = world.rig_combat(PLAYER);
    combat.shield = 0.0;
    combat.hit_at = 1;
    let mut hostiles = world.hostiles().clone();
    hostiles.rigs.insert(PLAYER, combat);
    world.set_hostiles(hostiles);
    spawn(
        &mut world,
        Faction::Swarm,
        EncounterKind::Roaming,
        &[ShipClass::Mite; 6],
    );
    let mut home = false;
    for _ in 0..seconds(&world, 30.0) {
        let _ = tick_with(&mut world, ControlState::default());
        if world.player(PLAYER).expect("rig").flight.place == FlightPlace::Home {
            home = true;
            break;
        }
    }
    assert!(home, "the beacon tows a wrecked rig home");
    let pod = world
        .hostiles()
        .wrecks
        .iter()
        .find(|wreck| wreck.owner == Some(PLAYER))
        .expect("a pod with half the cargo");
    assert_eq!(pod.cargo.iter().map(|stack| stack.count).sum::<u32>(), 5);
    assert!(world.hostiles().engaged(PLAYER).is_none());
}

/// The boss bot: the player's controls only. It turns to face the boss with the rotation
/// thrusters, holds a firing range with the main and retro thrusters, strafes off any
/// telegraphed line, fires when lined up and launches missiles; afterwards it flies to the
/// wreck.
fn boss_bot(world: &WorldState, tick: u32) -> (ControlState, Option<PlayerCommand>) {
    let rig = world.player(PLAYER).expect("rig");
    let pos = rig.flight.pos;
    let hostiles = world.hostiles();
    // The boss (or, with it gone, its escorts, then the wreck to salvage).
    let target = hostiles
        .boss()
        .map(|boss| (boss.pos, (boss.vx, boss.vy), Some(*boss)))
        .or_else(|| {
            hostiles
                .ships
                .iter()
                .min_by(|a, b| pos.distance(a.pos).total_cmp(&pos.distance(b.pos)))
                .map(|ship| (ship.pos, (ship.vx, ship.vy), None))
        })
        .or_else(|| {
            hostiles
                .wrecks
                .first()
                .map(|wreck| (wreck.pos, (wreck.vx, wreck.vy), None))
        });
    let Some((at, (tvx, tvy), boss)) = target else {
        return (ControlState::default(), None);
    };
    let (dx, dy) = pos.delta(at);
    let distance = length(dx, dy).max(1.0);
    let (ux, uy) = (dx / distance, dy / distance);
    let fighting = !hostiles.ships.is_empty();
    // The velocity it wants: hold a firing range off the target (or fly onto the wreck),
    // matching its drift, and slide sideways off any telegraphed line.
    let range = if fighting { 520.0 } else { 0.0 };
    let radial = ((distance - range) * 1.5).clamp(-450.0, 450.0);
    let (mut wx, mut wy) = (tvx + ux * radial, tvy + uy * radial);
    if boss.and_then(|boss| boss.telegraph).is_some() {
        wx += -uy * 420.0;
        wy += ux * 420.0;
    }
    let (ax, ay) = (wx - rig.flight.velocity_x, wy - rig.flight.velocity_y);
    // Into the craft's frame: its thrusters push along the nose and the right side.
    let angle = rig.attitude.orientation;
    let (nx, ny) = nose(angle);
    let (rx, ry) = (-ny, nx);
    let forward = ax * nx + ay * ny;
    let side = ax * rx + ay * ry;
    let error = wrap_angle(bearing(dx, dy) - angle);
    let rotate = (error * 4.0 - rig.attitude.angular_velocity * 1.2).clamp(-1.0, 1.0);
    let fire = fighting && error.abs() < 0.1 && distance < 900.0;
    let controls = ControlState::new(
        (side / 250.0).clamp(-1.0, 1.0),
        forward > 60.0,
        forward < -60.0,
    )
    .with_rotation(rotate)
    .with_weapon(WeaponControl {
        fire,
        ..WeaponControl::default()
    });
    let missile = (fire && tick.is_multiple_of(60) && world.rig_combat(PLAYER).missiles > 0)
        .then_some(PlayerCommand::Combat(CombatCommand::FireMissile));
    (controls, missile)
}

fn fight_boss(faction: Faction, hz: u32) -> (WorldState, Vec<(HostileEvent, u32)>) {
    let mut world = world_at(hz);
    launch_to(&mut world, open_space(4), 0.0);
    spawn(&mut world, faction, EncounterKind::Boss, &[faction.boss()]);
    let mut events = Vec::new();
    for tick in 0..seconds(&world, 300.0) {
        let (controls, command) = boss_bot(&world, tick);
        let out = match command.clone() {
            Some(command) => act(&mut world, controls, command),
            None => tick_with(&mut world, controls),
        };
        events.extend(hostile_events(&out));
        if tick % (hz * 2) == 0 && tick < hz * 20 {
            let rig = world.player(PLAYER).expect("rig");
            eprintln!(
                "  v=({:.0},{:.0}) ang={:.2} spin={:.2} ctl thrust={} retro={} rot={:.2} strafe={:.2} fire={} drive={:?}",
                rig.flight.velocity_x,
                rig.flight.velocity_y,
                rig.attitude.orientation,
                rig.attitude.angular_velocity,
                controls.thrust,
                controls.drill_down,
                controls.rotation(),
                controls.horizontal(),
                controls.weapon.fire,
                rig.flight.drive
            );
        }
        if tick % (hz * 10) == 0 {
            let rig = world.player(PLAYER).expect("rig");
            let boss = world.hostiles().boss().copied();
            eprintln!(
                "{faction:?} t={} rig hull {:.0}/{:.0} shield {:.0} place {:?} boss {:?} ships {} wrecks {}",
                tick / hz,
                rig.hull,
                rig.max_hull(),
                world.rig_combat(PLAYER).shield,
                rig.flight.place,
                boss.map(|b| (
                    b.hull as i32,
                    b.shield as i32,
                    rig.flight.pos.distance(b.pos) as i32,
                    b.mode,
                    b.phase
                )),
                world.hostiles().ships.len(),
                world.hostiles().wrecks.len(),
            );
        }
        if world.hostiles().has_tech(faction.tech()) {
            break;
        }
    }
    (world, events)
}

#[test]
fn the_pirate_king_is_beaten_by_a_bot() {
    let (world, events) = fight_boss(Faction::Pirates, 30);
    assert!(events.contains(&(HostileEvent::BossDefeated, Faction::Pirates.code())));
    assert!(world.hostiles().bosses_beaten.contains(&Faction::Pirates));
    assert!(
        world.hostiles().has_tech(Faction::Pirates.tech()),
        "its tech salvaged"
    );
    assert!(
        events
            .iter()
            .any(|(what, detail)| *what == HostileEvent::BossTelegraph
                && Attack::from_code(*detail) == Some(Attack::Broadside)),
        "it telegraphed its broadsides"
    );
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        rig.hull > rig.max_hull() * 0.2,
        "and the rig lived: {}",
        rig.hull
    );
}

#[test]
fn the_swarm_queen_is_beaten_by_a_bot() {
    let (world, events) = fight_boss(Faction::Swarm, 30);
    assert!(events.contains(&(HostileEvent::BossDefeated, Faction::Swarm.code())));
    assert!(world.hostiles().has_tech(Faction::Swarm.tech()));
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        rig.hull > rig.max_hull() * 0.2,
        "and the rig lived: {}",
        rig.hull
    );
}

#[test]
fn the_arbiter_is_beaten_by_a_bot() {
    let (world, events) = fight_boss(Faction::Machine, 60);
    assert!(events.contains(&(HostileEvent::BossDefeated, Faction::Machine.code())));
    assert!(world.hostiles().has_tech(Faction::Machine.tech()));
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        rig.hull > rig.max_hull() * 0.2,
        "and the rig lived: {}",
        rig.hull
    );
}

#[test]
fn every_boss_attack_is_telegraphed_long_enough() {
    use crate::hostiles::{MIN_TELEGRAPH_STEPS, boss};
    for attack in Attack::ALL {
        assert!(boss::warning(attack) >= MIN_TELEGRAPH_STEPS, "{attack:?}");
    }
    for class in [
        ShipClass::PirateKing,
        ShipClass::SwarmQueen,
        ShipClass::Arbiter,
    ] {
        for phase in 0..2 {
            assert!(!boss::cycle(class, phase).is_empty());
        }
    }
}

#[test]
fn hostiles_survive_a_save_and_replicate_as_a_section() {
    let mut world = world_at(30);
    launch_to(&mut world, open_space(3), 0.0);
    spawn(
        &mut world,
        Faction::Machine,
        EncounterKind::Roaming,
        &[ShipClass::Lancer, ShipClass::Drone],
    );
    for _ in 0..seconds(&world, 4.0) {
        let controls = aim_at_nearest(&world);
        let _ = tick_with(&mut world, controls);
    }
    assert!(!world.hostiles().shots.is_empty() || !world.hostiles().ships.is_empty());
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.hostiles(), world.hostiles());
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    // The shared section carries it to a replica.
    let mut replica = world_at(30);
    let payload = world.section(super::SectionKey::Hostiles).expect("section");
    replica.apply_section(super::SectionKey::Hostiles, payload);
    assert_eq!(replica.hostiles(), world.hostiles());
    assert!(super::SectionKey::SHARED.contains(&super::SectionKey::Hostiles));
}

#[test]
fn a_missile_locks_only_inside_the_noses_cone() {
    let mut world = world_at(60);
    launch_to(&mut world, open_space(3), 0.0);
    spawn(
        &mut world,
        Faction::Machine,
        EncounterKind::Roaming,
        &[ShipClass::Drone],
    );
    let ship = world.hostiles().ships[0];
    let (dx, dy) = rig_pos(&world).delta(ship.pos);
    let at_ship = bearing(dx, dy);
    // Turned well away from it: the missile flies straight.
    world.player_mut(PLAYER).expect("rig").attitude.orientation = at_ship + 1.2;
    let _ = act(
        &mut world,
        ControlState::default(),
        PlayerCommand::Combat(CombatCommand::FireMissile),
    );
    let missile = world
        .hostiles()
        .shots
        .iter()
        .find(|shot| shot.kind == ShotKind::Missile)
        .copied()
        .expect("fired");
    assert_eq!(missile.lock, None);
    // Nose on it: it locks.
    world.player_mut(PLAYER).expect("rig").attitude.orientation = at_ship;
    let mut hostiles = world.hostiles().clone();
    hostiles.rigs.entry(PLAYER).or_default().missiles = 2;
    world.set_hostiles(hostiles);
    let _ = act(
        &mut world,
        ControlState::default(),
        PlayerCommand::Combat(CombatCommand::FireMissile),
    );
    assert!(
        world
            .hostiles()
            .shots
            .iter()
            .any(|shot| shot.kind == ShotKind::Missile && shot.lock.is_some())
    );
    let (nx, ny) = nose(at_ship);
    assert!(nx.is_finite() && ny.is_finite());
}

#[test]
fn a_raid_lands_troopers_on_a_body_by_the_rig() {
    use crate::place::{BodyId, Place, SpaceBodyKey, StoreKey};
    let mut world = world_at(30);
    world.enable_test_mode();
    // A landable world of a system two sectors out.
    let space = crate::space::Space::shared(SEED);
    let (hx, hy) = galaxy_sector_of(home_star());
    let sector = space.sector(crate::space::SpaceSector::new(hx as i32 + 2, hy as i32));
    let system = sector.systems.first().expect("a system");
    let key = SpaceBodyKey::new(hx as i32 + 2, hy as i32, system.index, 0);
    let place = Place::OnBody(BodyId::Space(key));
    // Air for an airless world (the beacon would tow a rig without it home).
    let capacity = world.air_capacity_of(PLAYER);
    world.player_mut(PLAYER).expect("rig").flight.air = capacity;
    let _ = act(
        &mut world,
        ControlState::default(),
        PlayerCommand::TestMode(crate::TestModeAction::WarpToPlace(place)),
    );
    for _ in 0..seconds(&world, 1.0) {
        let _ = tick_with(&mut world, ControlState::default());
    }
    assert_eq!(world.place_of(PLAYER), place);
    let _ = act(
        &mut world,
        ControlState::default(),
        PlayerCommand::TestMode(crate::TestModeAction::SpawnRaid(Faction::Pirates)),
    );
    let raid = world.hostiles().raids.first().expect("a raid").clone();
    assert_eq!(raid.store, StoreKey::Body(key));
    for _ in 0..seconds(&world, 3.0) {
        let _ = tick_with(&mut world, ControlState::default());
    }
    let entities = world.store_entities(StoreKey::Body(key)).expect("the body");
    let troopers: Vec<_> = raid
        .troopers
        .iter()
        .filter_map(|id| entities.get(crate::entity::EntityId(*id)))
        .collect();
    assert_eq!(troopers.len(), raid.troopers.len(), "every trooper landed");
    let terrain = world.store_terrain(StoreKey::Body(key)).expect("ground");
    for trooper in troopers {
        assert!(
            !crate::physics::solid_at(terrain, trooper.body.x, trooper.body.y),
            "troopers keep out of the rock"
        );
    }
}
