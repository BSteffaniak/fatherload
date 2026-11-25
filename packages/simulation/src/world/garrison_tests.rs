//! Descent VII part B in a Deep Descent world (Wave 10 ancients-b): garrison forts that wake in
//! waves and sleep again, the build kit's defences (turrets that fire, barricades machines
//! batter, traps that stun; saved and replicated), tremors and falling stones, the Seal Gate
//! reading every key, the Gatekeeper Colossus beaten by a bot with Descent VII kit across its
//! collapsing hall, and the Heart Key gating Descent VIII.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WeaponControl, WorldState,
    base::{BuildCommand, BuildRefusal, Buildable, Facing},
    cave::CaveTile,
    depth_progression::RigSystem,
    descent::{DescentBoss, Stratum},
    enemy::{
        Damage, DamageKind, DamageOutcome, DamageSource, EnemyDifficulty, EnemyKind, EnemyMode,
    },
    entity::{EntityId, EntityState},
    garrison::{
        ABANDON_MILLIS, COLLAPSE_TELL_MILLIS, ColossusStage, DefenceKind, FortKind, FortPhase,
        GarrisonEvent, GarrisonLandmark, STIR_MILLIS, SealKey, WAVE_TELL_MILLIS,
    },
    hazard::PlacedBomb,
    position::WorldPos,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    weapon::WeaponKind,
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{
            garrison::{self as sites, Fort},
            layout,
        },
    },
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0xA4C_7E57;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// The rig systems a rig holds on reaching Descent VII: every gate's reward through the Phase
/// Drill, the companions too, never the Heart Key.
fn descent_seven_systems() -> u8 {
    RigSystem::ALL
        .into_iter()
        .filter(|system| *system != RigSystem::HeartKey)
        .fold(0, |bits, system| bits | system.bit())
}

/// A Deep Descent world at `hz`, the rig fitted for Descent VII and the director quiet.
fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hazard_gear.rig_systems = descent_seven_systems();
    rig.radiator_level = 15;
    rig.hull_level = 15;
    rig.hull = rig.max_hull();
    rig.credits = 9_000_000;
    world
}

fn put_rig(world: &mut WorldState, tile: TilePosition) {
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(tile.x), i64::from(tile.y)),
    );
    world.stream_terrain_near_players();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn tick_with(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let output = world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
    output.events.into_iter().map(|event| event.kind).collect()
}

fn ticks(world: &mut WorldState, count: u64) -> Vec<GameplayEventKind> {
    (0..count)
        .flat_map(|_| tick_with(world, ControlState::default()))
        .collect()
}

fn garrison_events(events: &[GameplayEventKind]) -> Vec<GarrisonEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Garrison { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

fn command(
    world: &mut WorldState,
    command: PlayerCommand,
) -> (PlayerScopedCommandOutcome, Vec<GameplayEventKind>) {
    let inputs = TickInputs::default().act(PLAYER, command);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    let outcome = output
        .action_outcomes
        .first()
        .map_or(PlayerScopedCommandOutcome::IgnoredUnavailable, |outcome| {
            outcome.outcome
        });
    (
        outcome,
        output.events.into_iter().map(|event| event.kind).collect(),
    )
}

/// A barracks fort of the test world (the first one rolled near the spawn).
fn barracks() -> Fort {
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::SleepingGarrison);
    let area = TileRect {
        min: TilePosition { x: -900, y: top },
        max: TilePosition {
            x: 1_100,
            y: bottom,
        },
    };
    let mut forts = sites::forts_with(&settings, &layout, &area);
    forts.sort_by_key(|fort| (fort.bounds.min.x - 97).abs());
    forts
        .into_iter()
        .find(|fort| fort.kind == FortKind::Barracks)
        .expect("a barracks fort")
}

/// The rig on `fort`'s floor, four tiles left of its pylon.
fn into_fort(world: &mut WorldState, fort: &Fort) -> TilePosition {
    let pylon = fort.pylon();
    let spot = TilePosition {
        x: pylon.x - 4,
        y: pylon.y,
    };
    put_rig(world, spot);
    spot
}

fn kill_wave(world: &mut WorldState, pylon: TilePosition) {
    let machines = world
        .garrison()
        .forts
        .get(&pylon)
        .map(|fort| fort.machines.clone())
        .unwrap_or_default();
    for id in machines {
        let mut output = crate::TickOutput::default();
        let _ = world.damage_enemy(
            EntityId(id),
            Damage::new(
                100_000.0,
                DamageKind::Piercing,
                DamageSource::Player(PLAYER),
            ),
            &mut output,
        );
    }
}

// --- The generated places ------------------------------------------------------------------------

#[test]
fn the_garrison_the_seal_gate_and_the_hall_are_generated_in_the_world() {
    let mut world = world_at(30);
    let fort = barracks();
    into_fort(&mut world, &fort);
    let pylon = fort.pylon();
    assert_eq!(
        world.terrain().tile(pylon).map(|tile| tile.kind),
        Some(TileKind::Cave(CaveTile::HoldPylon))
    );
    let hall = world.colossus_hall().expect("the hall");
    let door = world.seal_door().expect("the door");
    put_rig(
        &mut world,
        TilePosition {
            x: hall.center().x,
            y: hall.min.y - 4,
        },
    );
    for at in door.positions() {
        assert_eq!(
            world.terrain().tile(at).map(|tile| tile.kind),
            Some(TileKind::Cave(CaveTile::SealDoor))
        );
    }
    let floor = sites::cracked_floor(&hall);
    assert_eq!(
        world.terrain().tile(floor.center()).map(|tile| tile.kind),
        Some(TileKind::Cave(CaveTile::CrackedFloor))
    );
    // The Heart Seal lies under the hall, across every column.
    let layout = world.descent_strata_layout().expect("layout");
    let column = hall.center().x;
    let seal_row = (hall.max.y..hall.max.y + 120)
        .find(|row| sites::heart_seal(world.terrain().settings(), &layout, column, *row))
        .expect("the seal under the hall");
    put_rig(
        &mut world,
        TilePosition {
            x: column,
            y: seal_row - 3,
        },
    );
    assert_eq!(
        world
            .terrain()
            .tile(TilePosition {
                x: column,
                y: seal_row
            })
            .map(|tile| tile.kind),
        Some(TileKind::Cave(CaveTile::HeartSeal))
    );
}

// --- Forts and waves -----------------------------------------------------------------------------

#[test]
fn a_fort_stirs_telegraphs_its_waves_and_sleeps_again_paying_its_armoury() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let fort = barracks();
    let pylon = fort.pylon();
    into_fort(&mut world, &fort);
    let events = garrison_events(&ticks(&mut world, 2));
    assert!(events.contains(&GarrisonEvent::FortStirred));
    assert_eq!(world.garrison().forts[&pylon].phase, FortPhase::Stirring);
    // The build window, then the first wave's bays glow.
    let stir = u64::from(30_u32) * STIR_MILLIS / 1_000;
    let events = garrison_events(&ticks(&mut world, stir));
    assert!(events.contains(&GarrisonEvent::WaveTelegraphed));
    let state = &world.garrison().forts[&pylon];
    assert_eq!(state.phase, FortPhase::Telegraph);
    assert!(!state.lit_bays.is_empty());
    let lit = state.lit_bays.clone();
    let events = garrison_events(&ticks(&mut world, 30 * WAVE_TELL_MILLIS / 1_000 + 1));
    assert!(events.contains(&GarrisonEvent::WaveWoke));
    let state = &world.garrison().forts[&pylon];
    assert_eq!(state.phase, FortPhase::Wave);
    assert_eq!(state.wave, 1);
    // The machines climbed out of the lit bays.
    let frame = world.frame();
    for id in &state.machines {
        let entity = world.entities().get(EntityId(*id)).expect("a machine");
        let tile = frame.tile_at(entity.body.x, entity.body.y);
        assert!(
            lit.iter()
                .any(|bay| (bay.x - tile.x).abs() <= 2 && (bay.y - tile.y).abs() <= 3),
            "a machine at {tile:?} came from a lit bay {lit:?}"
        );
        assert!(fort.bounds.contains(tile));
    }
    let mut slept = false;
    let mut cleared = 0;
    let credits = world.player(PLAYER).expect("rig").credits;
    for _ in 0..fort.kind.waves() {
        kill_wave(&mut world, pylon);
        for _ in 0..30 * 20 {
            let events = garrison_events(&ticks(&mut world, 1));
            cleared += events
                .iter()
                .filter(|event| **event == GarrisonEvent::WaveCleared)
                .count();
            slept |= events.contains(&GarrisonEvent::GarrisonSlept);
            if slept || world.garrison().forts[&pylon].phase == FortPhase::Wave {
                break;
            }
            // Keep the rig whole and in place.
            let rig = world.player_mut(PLAYER).expect("rig");
            rig.hull = rig.max_hull();
        }
    }
    assert!(slept, "the garrison slept after its last wave");
    assert_eq!(cleared, usize::from(fort.kind.waves()) - 1);
    assert_eq!(world.garrison().forts[&pylon].phase, FortPhase::Asleep);
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.credits > credits, "the armoury paid");
    assert!(rig.carried(crate::item::ItemKind::Layer(Stratum::SleepingGarrison)) > 0);
    // It sleeps for good.
    let _ = ticks(&mut world, 60);
    assert!(!world.fort_awake(pylon));
}

#[test]
fn a_fort_nobody_holds_stands_down_and_its_waves_start_over() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let fort = barracks();
    let pylon = fort.pylon();
    into_fort(&mut world, &fort);
    let _ = ticks(&mut world, 2);
    world.stage_fort_wave(pylon, 2);
    let _ = ticks(&mut world, 30 * WAVE_TELL_MILLIS / 1_000 + 2);
    assert_eq!(world.garrison().forts[&pylon].wave, 2);
    let machines = world.garrison().forts[&pylon].machines.clone();
    assert!(!machines.is_empty());
    put_rig(
        &mut world,
        TilePosition {
            x: fort.bounds.max.x + 80,
            y: pylon.y,
        },
    );
    let events = garrison_events(&ticks(&mut world, 30 * ABANDON_MILLIS / 1_000 + 30));
    assert!(events.contains(&GarrisonEvent::StoodDown));
    assert!(!world.garrison().forts.contains_key(&pylon));
    for id in machines {
        assert!(
            world.entities().get(EntityId(id)).is_none(),
            "back in its bay"
        );
    }
    // Back in, it stirs from the start.
    into_fort(&mut world, &fort);
    let events = garrison_events(&ticks(&mut world, 2));
    assert!(events.contains(&GarrisonEvent::FortStirred));
    assert_eq!(world.garrison().forts[&pylon].wave, 0);
}

#[test]
fn waves_wake_and_sleep_at_the_same_world_time_at_every_tick_rate_and_deterministically() {
    let run = |hz: u32| {
        let mut world = world_at(hz);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        let fort = barracks();
        into_fort(&mut world, &fort);
        let mut woke_at = Vec::new();
        for _ in 0..u64::from(hz) * 30 {
            let events = garrison_events(&ticks(&mut world, 1));
            if events.contains(&GarrisonEvent::WaveWoke) {
                woke_at.push(world.millis_at(world.simulation_tick()));
            }
            let rig = world.player_mut(PLAYER).expect("rig");
            rig.hull = rig.max_hull();
        }
        (
            woke_at,
            world.garrison().forts[&fort.pylon()].machines.len(),
        )
    };
    let (slow, slow_count) = run(20);
    let (fast, fast_count) = run(60);
    assert_eq!(slow.len(), 1, "the first wave woke");
    assert!(slow[0].abs_diff(fast[0]) <= 50, "{slow:?} vs {fast:?}");
    assert_eq!(slow_count, fast_count);
    let hash = |hz| {
        let mut world = world_at(hz);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        let fort = barracks();
        into_fort(&mut world, &fort);
        for count in 0..u64::from(hz) * 30 {
            let _ = tick_with(
                &mut world,
                ControlState::new(if count % 60 < 30 { 0.4 } else { -0.4 }, false, false),
            );
        }
        world.simulation_state_hash()
    };
    assert_eq!(hash(30), hash(30), "deterministic");
}

// --- Defences ------------------------------------------------------------------------------------

fn place(
    world: &mut WorldState,
    kind: DefenceKind,
    at: TilePosition,
) -> PlayerScopedCommandOutcome {
    command(
        world,
        PlayerCommand::Build(BuildCommand::Place {
            item: Buildable::Defence(kind),
            at,
            facing: Facing::Right,
        }),
    )
    .0
}

#[test]
#[allow(clippy::too_many_lines, reason = "every placement rule in turn")]
fn defences_go_up_in_a_fort_without_a_claim_but_not_in_the_open() {
    let mut world = world_at(30);
    let fort = barracks();
    let spot = into_fort(&mut world, &fort);
    let at = TilePosition {
        x: spot.x - 2,
        y: spot.y,
    };
    // Locked until a crew reaches the Sleeping Garrison.
    assert_eq!(
        world.check_build(
            PLAYER,
            &BuildCommand::Place {
                item: Buildable::Defence(DefenceKind::Turret),
                at,
                facing: Facing::Right,
            }
        ),
        Err(BuildRefusal::NeedsGarrison)
    );
    let _ = ticks(&mut world, 3);
    assert!(world.defences_unlocked());
    let credits = world.player(PLAYER).expect("rig").credits;
    assert_eq!(
        place(&mut world, DefenceKind::Turret, at),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.garrison().defences.len(), 1);
    assert!(
        world.player(PLAYER).expect("rig").credits < credits,
        "it cost credits"
    );
    assert!(world.garrison().defence_at(at).is_some());
    assert!(
        world
            .garrison()
            .defence_at(TilePosition {
                x: at.x,
                y: at.y - 1
            })
            .is_some(),
        "a turret is two tall"
    );
    // A barricade is a solid block.
    let wall = TilePosition {
        x: spot.x + 3,
        y: spot.y,
    };
    assert_eq!(
        place(&mut world, DefenceKind::Barricade, wall),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        world.terrain().tile(wall).map(|tile| tile.kind),
        Some(TileKind::Cave(CaveTile::WarBarricade))
    );
    // Not in the open rock outside every claim and fort (the Seal Gate has no forts).
    let seal_rows = layout(&settings()).rows(Stratum::TheSealGate);
    let outside = TilePosition {
        x: 60,
        y: seal_rows.0 + 40,
    };
    let _ = world.terrain_mut().set_kind(
        TilePosition {
            x: outside.x - 1,
            y: outside.y + 1,
        },
        TileKind::Stone,
    );
    put_rig(&mut world, outside);
    let _ = world.terrain_mut().set_kind(
        TilePosition {
            x: outside.x - 1,
            y: outside.y,
        },
        TileKind::Air,
    );
    assert_eq!(
        world.check_build(
            PLAYER,
            &BuildCommand::Place {
                item: Buildable::Defence(DefenceKind::ShockTrap),
                at: TilePosition {
                    x: outside.x - 1,
                    y: outside.y,
                },
                facing: Facing::Right,
            }
        ),
        Err(BuildRefusal::NoClaim)
    );
    // Taking one down refunds half and clears its block.
    put_rig(&mut world, spot);
    let (outcome, _) = command(
        &mut world,
        PlayerCommand::Build(BuildCommand::Remove { at: wall }),
    );
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert_eq!(
        world.terrain().tile(wall).map(|tile| tile.kind),
        Some(TileKind::Air)
    );
    assert_eq!(world.garrison().defences.len(), 1);
    // No claim may be staked over a fort.
    assert_eq!(
        world.check_build(
            PLAYER,
            &BuildCommand::Place {
                item: Buildable::Module(crate::base::ModuleKind::Beacon),
                at: spot,
                facing: Facing::Right,
            }
        ),
        Err(BuildRefusal::InFort)
    );
}

#[test]
fn turrets_fire_at_machines_traps_stun_them_and_machines_batter_barricades() {
    let mut world = world_at(30);
    let fort = barracks();
    let spot = into_fort(&mut world, &fort);
    let _ = ticks(&mut world, 2);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    // Out of the fort's waves' way: stage the defences straight in.
    let turret = TilePosition {
        x: spot.x - 1,
        y: spot.y,
    };
    world.stage_defence(PLAYER, DefenceKind::Turret, turret);
    let (x, y) = world.frame().tile_center(TilePosition {
        x: spot.x - 7,
        y: spot.y,
    });
    let drone = world.spawn_enemy(EnemyKind::LancerDrone, x, y - TILE_SIZE);
    let mut fired = false;
    let mut killed = false;
    for _ in 0..30 * 8 {
        let events = ticks(&mut world, 1);
        fired |= garrison_events(&events).contains(&GarrisonEvent::TurretFired);
        if world.entities().get(drone).is_none() {
            killed = true;
            break;
        }
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = rig.max_hull();
    }
    assert!(fired, "the turret fired");
    assert!(killed, "the turret shot the drone down");
    let (_, defence) = world.garrison().defence_at(turret).expect("the turret");
    assert!(defence.fired.is_some(), "its tracer is in the state");
    // A shock trap stuns a sentry walking over it.
    let trap = TilePosition {
        x: spot.x + 4,
        y: spot.y,
    };
    world.stage_defence(PLAYER, DefenceKind::ShockTrap, trap);
    let (tx, ty) = world.frame().tile_center(trap);
    let sentry = world.spawn_enemy(EnemyKind::WarSentry, tx, ty);
    let mut sprung = false;
    let mut stunned = false;
    for _ in 0..30 * 3 {
        let events = ticks(&mut world, 1);
        sprung |= garrison_events(&events).contains(&GarrisonEvent::TrapSprung);
        stunned |= world.entities().get(sentry).is_some_and(|entity| {
            matches!(entity.state, EntityState::Enemy(state) if state.mode == EnemyMode::Stunned)
        });
        if sprung && stunned {
            break;
        }
    }
    assert!(sprung && stunned, "the trap zapped and stunned the sentry");
    // A walker batters the barricade in front of it down.
    let mut world = world_at(30);
    let spot = into_fort(&mut world, &fort);
    let _ = ticks(&mut world, 2);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let wall = TilePosition {
        x: spot.x - 3,
        y: spot.y,
    };
    world.stage_defence(PLAYER, DefenceKind::Barricade, wall);
    let (wx, wy) = world.frame().tile_center(TilePosition {
        x: wall.x - 2,
        y: wall.y,
    });
    let _walker = world.spawn_enemy(EnemyKind::SiegeWalker, wx, wy - 8.0);
    let mut destroyed = false;
    for _ in 0..30 * 40 {
        let events = ticks(&mut world, 1);
        destroyed |= garrison_events(&events).contains(&GarrisonEvent::DefenceDestroyed);
        if destroyed {
            break;
        }
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = rig.max_hull();
    }
    assert!(destroyed, "the barricade was battered down");
    assert_eq!(
        world.terrain().tile(wall).map(|tile| tile.kind),
        Some(TileKind::Air)
    );
}

#[test]
fn defences_save_replicate_and_hash() {
    let mut world = world_at(30);
    let fort = barracks();
    let spot = into_fort(&mut world, &fort);
    let classic = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(SEED),
        SimulationParams::new(30),
    );
    assert!(classic.garrison().is_empty());
    let before = world.simulation_state_hash();
    world.stage_defence(
        PLAYER,
        DefenceKind::Turret,
        TilePosition {
            x: spot.x - 1,
            y: spot.y,
        },
    );
    assert_ne!(
        world.simulation_state_hash(),
        before,
        "hashed once it holds anything"
    );
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.garrison(), world.garrison());
    assert!(crate::world::SectionKey::SHARED.contains(&crate::world::SectionKey::Garrison));
    let mut client =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(30));
    let payload = world
        .section(crate::world::SectionKey::Garrison)
        .expect("section");
    client.apply_section(crate::world::SectionKey::Garrison, payload);
    assert_eq!(client.garrison().defences, world.garrison().defences);
    // An older save without the field loads empty.
    let mut value: serde_json::Value = serde_json::from_str(&json).expect("value");
    let object = value.as_object_mut().expect("object");
    assert!(object.remove("garrison").is_some());
    assert!(object.remove("garrison_schedule").is_some());
    let older: WorldState = serde_json::from_value(value).expect("loads");
    assert!(older.garrison().is_empty());
    // Client keyframes leave the host's bookkeeping out.
    assert_eq!(
        world.redacted_for(Some(PLAYER)).garrison_schedule(),
        &crate::garrison::GarrisonSchedule::default()
    );
}

// --- Tremors -------------------------------------------------------------------------------------

#[test]
fn tremors_drop_stones_that_dust_marks_first_at_every_tick_rate() {
    let drop_at = |hz: u32| {
        let mut world = world_at(hz);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        let settings = settings();
        let layout = layout(&settings);
        let (top, bottom) = layout.rows(Stratum::TheSealGate);
        let row = top + (bottom - top) / 3;
        let spot = TilePosition { x: 40, y: row };
        put_rig(&mut world, spot);
        // A low cave over the rig.
        for at in TileRect::around(spot, 5, 3).positions() {
            let _ = world.terrain_mut().set_kind(at, TileKind::Air);
        }
        for x in spot.x - 6..=spot.x + 6 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y: spot.y - 4 }, TileKind::Stone);
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y: spot.y + 1 }, TileKind::Stone);
        }
        let mut tremor = None;
        let mut stone = None;
        for _ in 0..u64::from(hz) * 60 {
            let events = garrison_events(&ticks(&mut world, 1));
            if tremor.is_none() && events.contains(&GarrisonEvent::Tremor) {
                tremor = Some(world.millis_at(world.simulation_tick()));
                assert!(!world.garrison().drops.is_empty(), "dust marks the stones");
            }
            if tremor.is_some()
                && stone.is_none()
                && world.entities().iter().any(|entity| {
                    matches!(entity.state, EntityState::EnemyShot(falling)
                        if falling.kind == crate::enemy::ShotKind::FallingStone)
                })
            {
                stone = Some(world.millis_at(world.simulation_tick()));
            }
            let rig = world.player_mut(PLAYER).expect("rig");
            rig.hull = rig.max_hull();
            rig.velocity_x = 0.0;
        }
        (tremor.expect("a tremor"), stone.expect("a stone fell"))
    };
    let (tremor_slow, stone_slow) = drop_at(20);
    let (tremor_fast, stone_fast) = drop_at(60);
    assert!(tremor_slow.abs_diff(tremor_fast) <= 60);
    let tell = crate::garrison::TREMOR_TELL_MILLIS;
    assert!(stone_slow - tremor_slow + 60 >= tell);
    assert!(stone_fast - tremor_fast + 60 >= tell);
}

// --- The Seal Gate -------------------------------------------------------------------------------

/// The rig in the Seal Court, at the door.
fn at_the_door(world: &mut WorldState) -> TileRect {
    let door = world.seal_door().expect("the door");
    let hall = world.colossus_hall().expect("the hall");
    let side = sites::door_side(world.terrain().settings());
    let x = if side < 0 {
        door.min.x - 3
    } else {
        door.max.x + 3
    };
    put_rig(
        world,
        TilePosition {
            x,
            y: sites::upper_floor_row(&hall) - 1,
        },
    );
    door
}

#[test]
fn the_seal_gate_checks_every_key_then_lights_its_sockets_and_opens() {
    for missing in SealKey::ALL {
        let mut world = world_at(30);
        if missing == SealKey::DeepClaim {
            world.progression.deep_claim_status = crate::economy::DeepClaimStatus::Locked;
            world.crust.deep_claim = false;
        }
        let rig = world.player_mut(PLAYER).expect("rig");
        for system in missing.systems() {
            rig.hazard_gear.rig_systems &= !system.bit();
        }
        let door = at_the_door(&mut world);
        let events = garrison_events(&ticks(&mut world, 60));
        assert!(
            events.contains(&GarrisonEvent::SealRefused),
            "refused without the {}",
            missing.name()
        );
        assert_eq!(world.missing_seal_key(PLAYER), Some(missing));
        assert!(!events.contains(&GarrisonEvent::SealOpening));
        assert_eq!(
            events
                .iter()
                .filter(|event| **event == GarrisonEvent::SealRefused)
                .count(),
            1,
            "it says why once"
        );
        assert_eq!(
            world.terrain().tile(door.min).map(|tile| tile.kind),
            Some(TileKind::Cave(CaveTile::SealDoor))
        );
    }
    let mut world = world_at(30);
    let door = at_the_door(&mut world);
    assert!(world.holds_every_seal_key(PLAYER));
    let events = garrison_events(&ticks(&mut world, 30 * 10));
    assert!(events.contains(&GarrisonEvent::SealOpening));
    assert_eq!(
        events
            .iter()
            .filter(|event| **event == GarrisonEvent::SocketLit)
            .count(),
        SealKey::ALL.len(),
        "six sockets, one by one"
    );
    assert!(world.garrison().seal.open);
    for at in door.positions() {
        assert!(
            world
                .terrain()
                .tile(at)
                .is_some_and(|tile| tile.kind.is_open()),
            "the door sank"
        );
    }
    assert!(
        world
            .garrison()
            .found
            .contains(&GarrisonLandmark::SealCourt)
    );
}

// --- The Gatekeeper Colossus ---------------------------------------------------------------------

/// A rig's hull tier at the Colossus (the descent probe's arrival tier in the Seal Gate, plus
/// one it can afford there).
const HULL_AT_THE_COLOSSUS: u8 = 15;

/// The rig inside the hall, on the door-side floor.
fn into_hall(world: &mut WorldState) -> TileRect {
    let hall = world.colossus_hall().expect("the hall");
    let side = sites::door_side(world.terrain().settings());
    let x = if side < 0 {
        hall.min.x + 8
    } else {
        hall.max.x - 8
    };
    put_rig(
        world,
        TilePosition {
            x,
            y: sites::upper_floor_row(&hall) - 1,
        },
    );
    hall
}

/// The bot's controls: keep a working distance from the Colossus, step out from under a
/// raised fist, hover over a sweeping one, dodge dust, and aim the railgun at the head.
fn colossus_bot(world: &WorldState, tick: u64) -> ControlState {
    let rig = world.player(PLAYER).expect("rig");
    let colossus = &world.garrison().colossus;
    let core = colossus
        .entity
        .and_then(|id| world.entities().get(EntityId(id)))
        .map_or((rig.x, rig.y), |entity| (entity.body.x, entity.body.y));
    let head = colossus
        .parts
        .iter()
        .find(|(part, _)| *part == crate::garrison::ColossusPart::Head)
        .and_then(|(_, id)| world.entities().get(EntityId(*id)))
        .map_or(core, |entity| (entity.body.x, entity.body.y));
    let (dx, dy) = (head.0 - rig.x, head.1 - rig.y);
    let length = dx.hypot(dy).max(1.0);
    #[allow(clippy::cast_possible_truncation, reason = "a unit vector times 127")]
    let aim = ((dx / length * 127.0) as i8, (dy / length * 127.0) as i8);
    let frame = world.frame();
    let tile = frame.tile_at(rig.x, rig.y);
    let gap = core.0 - rig.x;
    let mut strafe = if gap.abs() < TILE_SIZE * 7.0 {
        -gap.signum()
    } else if gap.abs() > TILE_SIZE * 12.0 {
        gap.signum()
    } else if (tick / 45).is_multiple_of(2) {
        0.4
    } else {
        -0.4
    };
    let mut thrust = tick % 40 < 4;
    let now = world.simulation_tick();
    if let Some(fist) = colossus.fist {
        match fist.kind {
            crate::garrison::FistMoveKind::Slam if now < fist.lands => {
                if (fist.to.x - tile.x).abs() <= 4 {
                    strafe = if tile.x >= fist.to.x { 1.0 } else { -1.0 };
                }
            }
            crate::garrison::FistMoveKind::Sweep if now < fist.lands => thrust = true,
            _ => {}
        }
    }
    if world
        .garrison()
        .drops
        .iter()
        .any(|drop| (drop.x - tile.x).abs() <= 1)
    {
        strafe = if tick.is_multiple_of(2) { 1.0 } else { -1.0 };
    }
    // Stand off the hall's walls.
    let hall = world.colossus_hall().expect("hall");
    if tile.x <= hall.min.x + 4 {
        strafe = 1.0;
    } else if tile.x >= hall.max.x - 4 {
        strafe = -1.0;
    }
    ControlState::new(strafe, thrust, false).with_weapon(WeaponControl {
        fire: tick % 6 < 3,
        auto_aim: false,
        aim_x: aim.0,
        aim_y: aim.1,
    })
}

#[test]
#[allow(clippy::too_many_lines, reason = "the whole fight, start to end")]
fn a_bot_with_descent_vii_kit_beats_the_colossus_across_its_collapsing_hall_and_wins_the_heart_key()
{
    let hz = 30;
    let mut world = world_at(hz);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let hall = into_hall(&mut world);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        crate::economy::max_out_upgrades(rig);
        rig.hazard_gear.rig_systems = descent_seven_systems();
        rig.weapon_rack.battery_level = crate::weapon::MAX_BATTERY_LEVEL;
        let _ = rig.receive_weapon(WeaponKind::Railgun);
        let _ = rig.mount_weapon(Some(WeaponKind::Railgun));
        rig.hull_level = HULL_AT_THE_COLOSSUS;
        rig.hull = rig.max_hull();
    }
    let events = garrison_events(&ticks(&mut world, 2));
    assert!(events.contains(&GarrisonEvent::ColossusAwoke));
    assert!(world.garrison().colossus.awake());
    assert_eq!(world.garrison().colossus.parts.len(), 4);
    let mut stages = vec![world.garrison().colossus.stage];
    let mut collapse_started: Option<u64> = None;
    let mut telegraphs = Vec::new();
    let mut ignored_mid_collapse = false;
    let mut slams = 0;
    let mut defeated = false;
    let mut fight_ticks = 0_u64;
    for count in 0..u64::from(hz) * 1_200 {
        if world.garrison().colossus.entity.is_none() {
            break;
        }
        let controls = colossus_bot(&world, count);
        let output =
            world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
        let events: Vec<GameplayEventKind> =
            output.events.into_iter().map(|event| event.kind).collect();
        let ours = garrison_events(&events);
        slams += ours
            .iter()
            .filter(|event| **event == GarrisonEvent::FistSlam)
            .count();
        if ours.contains(&GarrisonEvent::HallCollapsing) {
            collapse_started = Some(count);
            let id = EntityId(world.garrison().colossus.entity.expect("core"));
            let mut output = crate::TickOutput::default();
            ignored_mid_collapse |= world.damage_enemy(
                id,
                Damage::new(50.0, DamageKind::Piercing, DamageSource::Player(PLAYER)),
                &mut output,
            ) == DamageOutcome::Ignored;
        }
        if ours.contains(&GarrisonEvent::HallCollapsed) {
            if let Some(start) = collapse_started.take() {
                telegraphs.push(count - start);
            }
            stages.push(world.garrison().colossus.stage);
            // The hall changed: the cracked floor (then the vault) is gone.
            let floor = sites::cracked_floor(&hall);
            assert!(
                world
                    .terrain()
                    .tile(floor.center())
                    .is_some_and(|tile| tile.kind.is_open()),
                "the floor gave way"
            );
        }
        if ours.contains(&GarrisonEvent::ColossusDefeated) {
            defeated = true;
            fight_ticks = count;
            break;
        }
        assert!(
            world.player(PLAYER).expect("rig").hull > 0.0,
            "the bot survived (stages {stages:?})"
        );
    }
    assert!(
        defeated,
        "the bot beat the Colossus (stages seen {stages:?})"
    );
    assert_eq!(stages, ColossusStage::ALL.to_vec(), "every stage in order");
    assert!(ignored_mid_collapse, "collapses shrug off hits");
    assert!(slams > 0, "it slammed its fists");
    let warning = u64::from(hz) * COLLAPSE_TELL_MILLIS / 1_000;
    assert_eq!(telegraphs.len(), 2);
    assert!(
        telegraphs.iter().all(|ticks| ticks.abs_diff(warning) <= 1),
        "{telegraphs:?}"
    );
    let vault = sites::cracked_vault(&hall);
    assert!(
        world
            .terrain()
            .tile(vault.center())
            .is_some_and(|tile| tile.kind.is_open()),
        "the vault came down"
    );
    let left =
        world.player(PLAYER).expect("rig").hull / world.player(PLAYER).expect("rig").max_hull();
    assert!(
        left > 0.15,
        "with a margin: {:.0} % of the hull left",
        left * 100.0
    );
    // The probe models the fight from this measured win (Wave 10 integration).
    #[allow(clippy::cast_precision_loss, reason = "a few thousand ticks")]
    crate::balance::descent::mechanics::record_bot_fight(
        crate::descent::DescentBoss::GatekeeperColossus,
        fight_ticks as f64 / f64::from(hz),
        f64::from(left),
    );
    assert!(world.garrison().colossus.beaten);
    assert!(
        world
            .player(PLAYER)
            .expect("rig")
            .hazard_gear
            .has_system(RigSystem::HeartKey)
    );
    // Its floor opened on the way down.
    let shaft = sites::victory_shaft(&hall);
    assert!(
        world
            .terrain()
            .tile(shaft.center())
            .is_some_and(|tile| tile.kind.is_open())
    );
    let _ = ticks(&mut world, 30);
    assert!(!world.garrison().colossus.awake());
}

#[test]
fn the_colossus_parts_are_drawn_where_they_hit_and_pass_hits_to_the_core() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let _ = into_hall(&mut world);
    let _ = ticks(&mut world, 2);
    let core = EntityId(world.garrison().colossus.entity.expect("awake"));
    let core_body = world.entities().get(core).expect("core").body;
    for (part, id) in world.garrison().colossus.parts.clone() {
        let entity = world.entities().get(EntityId(id)).expect("part");
        let (dx, dy) = part.rest_offset();
        if part.hand().is_none() {
            assert!((entity.body.x - core_body.x - dx).abs() < 1.0, "{part:?}");
            assert!((entity.body.y - core_body.y - dy).abs() < 1.0, "{part:?}");
        }
        let health = entity.state.health().expect("a hit circle");
        assert!((health.radius - part.radius()).abs() < 0.01, "{part:?}");
    }
    let health = |world: &WorldState| match world.entities().get(core).expect("core").state {
        EntityState::Enemy(state) => state.health,
        _ => 0.0,
    };
    let before = health(&world);
    let head = world
        .garrison()
        .colossus
        .parts
        .iter()
        .find(|(part, _)| *part == crate::garrison::ColossusPart::Head)
        .map(|(_, id)| EntityId(*id))
        .expect("head");
    let mut output = crate::TickOutput::default();
    let outcome = world.damage_enemy(
        head,
        Damage::new(20.0, DamageKind::Piercing, DamageSource::Player(PLAYER)),
        &mut output,
    );
    assert!(matches!(outcome, DamageOutcome::Hurt { .. }));
    assert!(health(&world) < before - 20.0, "the head is the weak point");
}

#[test]
fn on_a_peaceful_world_the_colossus_yields_and_a_lonely_hall_sleeps_keeping_its_collapse() {
    let mut world = world_at(30);
    let _ = into_hall(&mut world);
    let events = garrison_events(&ticks(&mut world, 3));
    assert!(events.contains(&GarrisonEvent::ColossusDefeated));
    assert!(
        world
            .player(PLAYER)
            .expect("rig")
            .hazard_gear
            .has_system(RigSystem::HeartKey)
    );
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let hall = into_hall(&mut world);
    let _ = ticks(&mut world, 3);
    world.stage_colossus(ColossusStage::Fallen, false);
    assert_eq!(world.garrison().colossus.arena, ColossusStage::Fallen);
    put_rig(
        &mut world,
        TilePosition {
            x: hall.max.x + 60,
            y: hall.min.y,
        },
    );
    let _ = ticks(&mut world, 30 * 32);
    assert!(!world.garrison().colossus.awake());
    assert!(!world.garrison().colossus.beaten);
    // It wakes again in the fallen hall, at that stage's health.
    let _ = into_hall(&mut world);
    let _ = ticks(&mut world, 3);
    assert!(world.garrison().colossus.awake());
    assert_eq!(world.garrison().colossus.stage, ColossusStage::Fallen);
}

// --- The Heart Key -------------------------------------------------------------------------------

#[test]
fn the_heart_key_is_won_from_the_colossus_and_alone_opens_the_heart_seal() {
    assert_eq!(
        RigSystem::HeartKey.boss_reward(),
        Some(DescentBoss::GatekeeperColossus)
    );
    let gate = Stratum::TheSealGate.def().gate.expect("the gate");
    assert_eq!(gate.boss, DescentBoss::GatekeeperColossus);
    let mut world = world_at(30);
    let hall = world.colossus_hall().expect("hall");
    let layout = world.descent_strata_layout().expect("layout");
    let column = hall.max.x + 20;
    let seal_row = (hall.min.y..hall.max.y + 140)
        .find(|row| sites::heart_seal(world.terrain().settings(), &layout, column, *row))
        .expect("the seal");
    let above = TilePosition {
        x: column,
        y: seal_row - 1,
    };
    put_rig(&mut world, above);
    for x in column - 2..=column + 2 {
        for y in seal_row - 3..seal_row {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
    }
    put_rig(&mut world, above);
    let seal = TilePosition {
        x: column,
        y: seal_row,
    };
    let drill_down = |world: &mut WorldState| {
        for _ in 0..30 * 10 {
            let _ = tick_with(world, ControlState::new(0.0, false, true));
        }
    };
    drill_down(&mut world);
    assert_eq!(
        world.terrain().tile(seal).map(|tile| tile.kind),
        Some(TileKind::Cave(CaveTile::HeartSeal)),
        "the seal held without the key"
    );
    // A blast does not break it either.
    let (bx, by) = world.frame().tile_center(seal);
    world.insert_charge(PlacedBomb {
        id: 9_200,
        x: bx,
        y: by - TILE_SIZE,
        previous_x: bx,
        previous_y: by - TILE_SIZE,
        kind: crate::explosive::ExplosiveKind::C4,
        detonation_tick: world.simulation_tick().next(),
        ..PlacedBomb::default()
    });
    let _ = ticks(&mut world, 5);
    assert_eq!(
        world.terrain().tile(seal).map(|tile| tile.kind),
        Some(TileKind::Cave(CaveTile::HeartSeal)),
        "blast-proof"
    );
    put_rig(&mut world, above);
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::grant_rig_system(rig, RigSystem::HeartKey);
    rig.hull = rig.max_hull();
    drill_down(&mut world);
    assert!(
        world
            .terrain()
            .tile(seal)
            .is_some_and(|tile| tile.kind.is_open()),
        "the Heart Key cut the seal"
    );
}

// --- The body rules ------------------------------------------------------------------------------

#[test]
#[allow(clippy::too_many_lines, reason = "the arena, the spawns and the fight")]
fn the_garrison_creatures_keep_the_body_rules() {
    use crate::{enemy::Locomotion, physics};
    let hz = 30;
    let mut world = world_at(hz);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let settings = settings();
    let (top_row, bottom_row) = layout(&settings).rows(Stratum::TheSealGate);
    let row = top_row + (bottom_row - top_row) / 3;
    let (left, right, top, floor) = (140, 220, row - 10, row + 8);
    put_rig(
        &mut world,
        TilePosition {
            x: 180,
            y: floor - 1,
        },
    );
    let _ = ticks(&mut world, 2);
    let fill = |world: &mut WorldState, rect: TileRect, kind: TileKind| {
        for position in rect.positions() {
            let _ = world.terrain_mut().set_kind(position, kind);
        }
    };
    fill(
        &mut world,
        TileRect {
            min: TilePosition {
                x: left - 4,
                y: top - 4,
            },
            max: TilePosition {
                x: right + 4,
                y: floor + 4,
            },
        },
        TileKind::Stone,
    );
    fill(
        &mut world,
        TileRect {
            min: TilePosition { x: left, y: top },
            max: TilePosition {
                x: right,
                y: floor - 1,
            },
        },
        TileKind::Air,
    );
    put_rig(
        &mut world,
        TilePosition {
            x: 180,
            y: floor - 1,
        },
    );
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        let _ = rig.receive_weapon(WeaponKind::RivetGun);
        let _ = rig.mount_weapon(Some(WeaponKind::RivetGun));
        rig.weapon.energy = rig.battery_capacity();
    }
    let frame = world.frame();
    let kinds: Vec<EnemyKind> = EnemyKind::ALL
        .into_iter()
        .filter(|kind| kind.is_garrison() && WorldState::garrison_spawn_allowed(*kind))
        .collect();
    assert_eq!(kinds.len(), 5);
    for (index, kind) in (0_i32..).zip(kinds) {
        let column = left + 8 + (index * 9) % (right - left - 12);
        let tile = match kind.stats().locomotion {
            Locomotion::Walking | Locomotion::Stationary => TilePosition {
                x: column,
                y: floor - 1 - i32::from(kind.stats().radius > 12.0),
            },
            Locomotion::Flying => TilePosition {
                x: column,
                y: top + 3 + index % 5,
            },
            Locomotion::Burrowing => TilePosition {
                x: left + 5,
                y: floor - 3,
            },
        };
        let (x, y) = frame.tile_center(tile);
        let _ = world.spawn_enemy(kind, x, y);
    }
    let mut checked = 0_usize;
    for count in 0..u64::from(hz) * 30 {
        let axis = if (count / 40) % 2 == 0 { 0.8 } else { -0.8 };
        let controls = ControlState::new(axis, count % 50 < 10, false).with_weapon(WeaponControl {
            fire: true,
            auto_aim: true,
            aim_x: 127,
            aim_y: 0,
        });
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
        let rig = world.player(PLAYER).expect("rig").clone();
        for (entity, state) in world.enemies() {
            let radius = state.kind.stats().radius;
            assert!(
                !physics::collides(world.terrain(), entity.body.x, entity.body.y, radius),
                "{:?} inside rock at tick {count}",
                state.kind
            );
            let (own_w, own_h) = state.kind.contact_half();
            let (rig_w, rig_h) = super::enemies::RIG_CONTACT_HALF;
            let depth = (own_w + rig_w - (entity.body.x - rig.x).abs())
                .min(own_h + rig_h - (entity.body.y - rig.y).abs())
                .max(0.0);
            let side_x = (entity.body.x - rig.x).signum();
            let side_y = (entity.body.y - rig.y).signum();
            let pinned = physics::collides(
                world.terrain(),
                side_x.mul_add(own_w + rig_w, rig.x),
                entity.body.y,
                radius,
            ) && physics::collides(
                world.terrain(),
                entity.body.x,
                side_y.mul_add(own_h + rig_h, rig.y),
                radius,
            );
            if pinned {
                continue;
            }
            assert!(
                depth <= 1.0,
                "{:?} #{} sits {depth} px inside the rig at tick {count}",
                state.kind,
                entity.id.get(),
            );
            checked += 1;
        }
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = rig.max_hull();
    }
    assert!(checked > 1_000, "a real fight: {checked}");
}
