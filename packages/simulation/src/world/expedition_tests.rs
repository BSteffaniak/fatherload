//! The Fathers' expedition in a Deep Descent world (Wave 8 story): canisters read in order
//! (a later one stays sealed until the one before is read), echoes at their depths, anomaly
//! rooms entered, milestones, saves and replication.

use crate::{
    GameplayEventKind, PlayerId, SectionKey, SimulationParams, TickInputs, WorldState,
    enemy::EnemyDifficulty,
    expedition::{
        ECHOES, ExpeditionState, LOGS, anomaly, log_bounty,
        sites::{self, ExpeditionSite},
    },
    milestone::Milestone,
    position::WorldPos,
    terrain::{TileKind, TilePosition},
    worldgen::{GeneratorVersion, TileRect, WorldSettings, descent::layout},
};

use super::expedition::echo_row;

const PLAYER: PlayerId = PlayerId::new(0);
const PARTNER: PlayerId = PlayerId::new(1);

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

fn world(seed: u64) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &v11(seed), SimulationParams::default());
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world
}

fn step(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let tick = world.simulation_tick();
    world
        .simulate_tick(tick, &TickInputs::default())
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn put(world: &mut WorldState, player: PlayerId, tile: TilePosition) {
    world.teleport_rig_to(
        player,
        WorldPos::tile_center(i64::from(tile.x), i64::from(tile.y)),
    );
}

fn site(world: &WorldState, number: u8) -> ExpeditionSite {
    sites::sites(world.terrain().settings())[usize::from(number - 1)]
}

/// Parks the rig in the middle of log `number`'s room and runs one tick.
fn visit(world: &mut WorldState, number: u8) -> Vec<GameplayEventKind> {
    let room = site(world, number);
    put(world, PLAYER, room.canister());
    step(world)
}

fn found(events: &[GameplayEventKind]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::ExpeditionLogFound { log, .. } => Some(*log),
            _ => None,
        })
        .collect()
}

#[test]
fn the_generator_carves_every_log_site() {
    let world = world(0x5EED);
    for site in sites::sites(world.terrain().settings()) {
        let inside = site.interior().center();
        let tile = world.terrain().tile(inside).map(|tile| tile.kind);
        assert!(
            matches!(tile, Some(TileKind::Air | TileKind::Fluid(_))),
            "log {} at {inside:?}: {tile:?}",
            site.log + 1
        );
        let floor = world.terrain().tile(TilePosition {
            x: inside.x,
            y: site.floor_row(),
        });
        assert!(matches!(floor.map(|t| t.kind), Some(TileKind::Built(_))));
    }
}

#[test]
fn logs_are_read_in_order_and_a_later_canister_waits_sealed() {
    let mut world = world(0x0DD_BA11);
    let credits = world.player(PLAYER).expect("rig").credits;
    // Log 3 first: sealed, nothing read.
    let events = visit(&mut world, 3);
    assert!(events.contains(&GameplayEventKind::ExpeditionLogSealed {
        player_id: PLAYER,
        log: 3,
    }));
    assert_eq!(world.expedition().found, 0);
    // Standing there longer seals it only once.
    let again = step(&mut world);
    assert!(
        !again
            .iter()
            .any(|e| matches!(e, GameplayEventKind::ExpeditionLogSealed { .. }))
    );
    // Log 2 is sealed too; log 1 opens all three, in order.
    visit(&mut world, 2);
    assert_eq!(world.expedition().found, 0);
    let events = visit(&mut world, 1);
    assert_eq!(found(&events), vec![1, 2, 3]);
    assert_eq!(world.expedition().found, 3);
    assert!(world.expedition().sealed.is_empty());
    let paid = world.player(PLAYER).expect("rig").credits - credits;
    assert_eq!(
        paid,
        3 * log_bounty(LOGS[0].descent()),
        "each canister pays its finder"
    );
    // Reading never skips: log 5 before 4 stays sealed, and the read logs stay a prefix.
    visit(&mut world, 5);
    assert_eq!(world.expedition().found, 3);
    let events = visit(&mut world, 4);
    assert_eq!(found(&events), vec![4, 5]);
    // A canister already read does nothing.
    assert!(found(&visit(&mut world, 2)).is_empty());
    assert_eq!(
        world.expedition().read_logs().count(),
        usize::from(world.expedition().found)
    );
}

#[test]
fn echoes_play_in_order_at_their_depths_one_card_at_a_time() {
    let mut world = world(0xEC40);
    world.enable_test_mode();
    let settings = *world.terrain().settings();
    let layout = layout(&settings);
    // Above the first echo's depth: nothing.
    let first = echo_row(&layout, &ECHOES[0]);
    let column = sites::sites(&settings)[0].bounds.min.x - 6;
    put(
        &mut world,
        PLAYER,
        TilePosition {
            x: column,
            y: first - 4,
        },
    );
    assert!(
        !step(&mut world)
            .iter()
            .any(|e| matches!(e, GameplayEventKind::ExpeditionEcho { .. }))
    );
    // Dropping far below the third echo plays the first now and the others later, in order.
    let third = echo_row(&layout, &ECHOES[2]);
    put(
        &mut world,
        PLAYER,
        TilePosition {
            x: column,
            y: third + 2,
        },
    );
    let mut heard = Vec::new();
    let mut ticks = 0;
    while heard.len() < 3 && ticks < 60 * 40 {
        put(
            &mut world,
            PLAYER,
            TilePosition {
                x: column,
                y: third + 2,
            },
        );
        for event in step(&mut world) {
            if let GameplayEventKind::ExpeditionEcho { echo, .. } = event {
                heard.push((echo, world.simulation_tick().get()));
            }
        }
        ticks += 1;
    }
    let order: Vec<u8> = heard.iter().map(|(echo, _)| *echo).collect();
    assert_eq!(order, vec![1, 2, 3]);
    let gap = world
        .timing()
        .ticks_for_millis(crate::expedition::ECHO_GAP_MILLIS);
    assert!(heard.windows(2).all(|pair| pair[1].1 - pair[0].1 >= gap));
    assert_eq!(world.expedition().echoes, 3);
    assert_eq!(world.next_echo_row(), Some(echo_row(&layout, &ECHOES[3])));
}

#[test]
fn echo_gaps_last_the_same_seconds_at_any_tick_rate() {
    let mut gaps = Vec::new();
    for hz in [30, 60, 120] {
        let mut world =
            WorldState::new_game_with_settings(PLAYER, &v11(0xEC41), SimulationParams::new(hz));
        world.enable_test_mode();
        let settings = *world.terrain().settings();
        let deep = echo_row(&layout(&settings), &ECHOES[1]) + 1;
        let column = sites::sites(&settings)[0].bounds.min.x - 6;
        let mut ticks = Vec::new();
        for _ in 0..(hz * 30) {
            put(&mut world, PLAYER, TilePosition { x: column, y: deep });
            if step(&mut world)
                .iter()
                .any(|e| matches!(e, GameplayEventKind::ExpeditionEcho { .. }))
            {
                ticks.push(world.simulation_tick().get());
            }
        }
        assert_eq!(ticks.len(), 2, "{hz} Hz");
        gaps.push((ticks[1] - ticks[0]) * 1_000 / u64::from(hz));
    }
    assert!(
        gaps.iter().all(|gap| gap.abs_diff(gaps[0]) <= 34),
        "{gaps:?}"
    );
}

/// The first anomaly room of `kind` in a strip of the Crust and Old Deep.
fn room_of(settings: &WorldSettings, kind: anomaly::AnomalyKind) -> anomaly::AnomalyRoom {
    let layout = layout(settings);
    let area = TileRect {
        min: TilePosition {
            x: -1_500,
            y: layout.tops[0],
        },
        max: TilePosition {
            x: 1_500,
            y: layout.descent_rows(crate::descent::Descent::OldDeep).1,
        },
    };
    anomaly::rooms_in(settings, &area)
        .into_iter()
        .find(|room| room.kind == kind)
        .expect("a room of the kind")
}

#[test]
fn entering_anomaly_rooms_feels_their_fields_and_earns_the_milestones() {
    let mut world = world(0x5EED);
    // The first milestone check records silently.
    step(&mut world);
    let settings = *world.terrain().settings();
    for (index, kind) in anomaly::AnomalyKind::ALL.into_iter().enumerate() {
        let room = room_of(&settings, kind);
        // The generator stamped it: open inside, its caches in place.
        let centre = room.interior().center();
        assert!(matches!(
            world
                .terrain()
                .tile(TilePosition {
                    x: centre.x,
                    y: centre.y + 1
                })
                .map(|t| t.kind),
            Some(TileKind::Air | TileKind::Cave(_))
        ));
        for cache in room.caches().into_iter().flatten() {
            assert!(matches!(
                world.terrain().tile(cache).map(|t| t.kind),
                Some(TileKind::Cave(_))
            ));
        }
        put(
            &mut world,
            PLAYER,
            TilePosition {
                x: room.interior().min.x + 1,
                y: centre.y,
            },
        );
        let events = step(&mut world);
        assert!(
            events.contains(&GameplayEventKind::AnomalyEntered {
                player_id: PLAYER,
                kind,
                first_of_kind: true,
            }),
            "{kind:?}: {events:?}"
        );
        // The rig feels the room's rule where it stands.
        let fields = world.fields_for(PLAYER);
        match kind {
            anomaly::AnomalyKind::GravityFlip => assert!(fields.gravity_y < 0.0),
            anomaly::AnomalyKind::PixelRot => assert!(fields.screen[0] > 0.0),
            anomaly::AnomalyKind::TimeFrozen => assert!(fields.time_scale < 0.5),
            anomaly::AnomalyKind::Mirror => assert!(fields.mirrored()),
        }
        if index == 0 {
            assert!(events.contains(&GameplayEventKind::MilestoneEarned {
                player_id: PLAYER,
                milestone: Milestone::FirstAnomaly,
            }));
        }
        // Once per room.
        assert!(
            !step(&mut world)
                .iter()
                .any(|e| matches!(e, GameplayEventKind::AnomalyEntered { .. }))
        );
    }
    assert_eq!(world.expedition().anomaly_kind_count(), 4);
    let player = world.player(PLAYER).expect("rig");
    assert!(player.milestones.has(Milestone::EveryAnomaly));
}

#[test]
fn reading_the_first_log_earns_the_trail_milestone() {
    let mut world = world(0x7A11);
    step(&mut world);
    let events = visit(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::MilestoneEarned {
        player_id: PLAYER,
        milestone: Milestone::TrailFound,
    }));
    let mut state = world.expedition().clone();
    state.found = 10;
    world.set_expedition(state);
    let events = step(&mut world);
    assert!(events.contains(&GameplayEventKind::MilestoneEarned {
        player_id: PLAYER,
        milestone: Milestone::ShapedWorld,
    }));
}

#[test]
fn the_journal_saves_loads_and_hashes_only_once_used() {
    let mut world = world(0x5A7E);
    let empty_hash = world.simulation_state_hash();
    assert!(world.expedition().is_empty());
    visit(&mut world, 1);
    visit(&mut world, 3);
    assert_eq!(world.expedition().found, 1);
    let json = serde_json::to_string(&world).expect("json");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.expedition(), world.expedition());
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    assert_ne!(world.simulation_state_hash(), empty_hash);
    // A save from before the expedition loads with an empty journal.
    let mut value: serde_json::Value = serde_json::to_value(&world).expect("value");
    value.as_object_mut().expect("object").remove("expedition");
    let old: WorldState = serde_json::from_value(value).expect("old save");
    assert_eq!(*old.expedition(), ExpeditionState::default());
    // An older world never fills it.
    let mut classic = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(0x5A7E),
        SimulationParams::default(),
    );
    for _ in 0..5 {
        step(&mut classic);
    }
    assert!(classic.expedition().is_empty());
}

#[test]
fn co_op_partners_share_one_journal_through_replication() {
    let mut host = world(0xC0_0B);
    host.add_player(PARTNER);
    let mut replica = host.clone();
    // The partner reads the first canister; the host's own rig is elsewhere.
    let first = site(&host, 1);
    put(&mut host, PARTNER, first.canister());
    let events = step(&mut host);
    assert!(events.contains(&GameplayEventKind::ExpeditionLogFound {
        player_id: PARTNER,
        log: 1,
    }));
    assert_ne!(
        host.section_hash(SectionKey::Expedition),
        replica.section_hash(SectionKey::Expedition)
    );
    let payload = host
        .section(SectionKey::Expedition)
        .expect("shared section");
    assert!(SectionKey::Expedition.visible_to(Some(PLAYER)));
    assert!(SectionKey::Expedition.visible_to(None));
    replica.apply_section(SectionKey::Expedition, payload);
    assert_eq!(replica.expedition(), host.expedition());
    assert_eq!(
        host.section_hash(SectionKey::Expedition),
        replica.section_hash(SectionKey::Expedition)
    );
    // Both seats' milestones count the shared trail.
    step(&mut host);
    for player in [PLAYER, PARTNER] {
        assert!(
            host.player(player)
                .expect("rig")
                .milestones
                .has(Milestone::TrailFound)
        );
    }
}

/// A rig that digs up under an upturned room climbs in through its floor hatch and falls on
/// upward to the ceiling where the cache waits (played, not teleported inside).
#[test]
fn a_rig_climbs_in_through_the_hatch_and_falls_up_in_the_upturned_room() {
    let mut world = world(0x5EED);
    let settings = *world.terrain().settings();
    let room = room_of(&settings, anomaly::AnomalyKind::GravityFlip);
    let hatch: Vec<TilePosition> = room
        .bounds
        .positions()
        .filter(|tile| room.is_door(*tile))
        .collect();
    assert_eq!(hatch.len(), 2);
    // A shaft dug up to the hatch from below, as a rig coming from underneath would leave it.
    let below = room.bounds.max.y;
    for door in &hatch {
        assert_eq!(
            world.terrain().tile(*door).map(|t| t.kind),
            Some(TileKind::Air)
        );
        for y in below + 1..=below + 6 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x: door.x, y }, TileKind::Air);
        }
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x: door.x,
                y: below + 7,
            },
            TileKind::Stone,
        );
    }
    put(
        &mut world,
        PLAYER,
        TilePosition {
            x: hatch[0].x,
            y: below + 6,
        },
    );
    step(&mut world);
    let climb = TickInputs::held(PLAYER, crate::ControlState::new(0.0, true, false));
    let mut entered = false;
    for _ in 0..360 {
        let tick = world.simulation_tick();
        let events = world.simulate_tick(tick, &climb).events;
        entered |= events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::AnomalyEntered { .. }));
        if entered
            && world
                .actor_tile(PLAYER)
                .is_some_and(|tile| tile.y < room.interior().max.y - 1)
        {
            break;
        }
    }
    assert!(
        entered,
        "the rig never climbed in: {:?} for {:?}",
        world.actor_tile(PLAYER),
        room.bounds
    );
    // Let go: the room takes the rig the rest of the way up.
    let idle = TickInputs::held(PLAYER, crate::ControlState::new(0.0, false, false));
    for _ in 0..240 {
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &idle);
    }
    let tile = world.actor_tile(PLAYER).expect("rig");
    assert!(
        room.interior().contains(tile) && tile.y <= room.interior().min.y + 1,
        "the rig rests on the ceiling: {tile:?} in {:?}",
        room.interior()
    );
    assert!(world.fields_for(PLAYER).gravity_y < -0.9);
}

/// A rig drives into a mirror room through a side doorway: once the room mirrors the screen
/// and the stick, a player keeps pushing toward the room as the screen now shows it (the
/// other way), and the rig carries on in.
#[test]
fn a_rig_drives_into_the_mirror_room_through_a_doorway() {
    let mut world = world(0x5EED);
    let settings = *world.terrain().settings();
    let room = room_of(&settings, anomaly::AnomalyKind::Mirror);
    let floor_row = room.bounds.max.y - 1;
    for x in room.bounds.min.x - 4..room.bounds.min.x {
        for y in floor_row - 2..=floor_row {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
        let _ = world.terrain_mut().set_kind(
            TilePosition {
                x,
                y: floor_row + 1,
            },
            TileKind::Stone,
        );
    }
    put(
        &mut world,
        PLAYER,
        TilePosition {
            x: room.bounds.min.x - 3,
            y: floor_row,
        },
    );
    let mut inside = false;
    for _ in 0..600 {
        let toward = if world.fields_for(PLAYER).mirrored() {
            -1.0
        } else {
            1.0
        };
        let inputs = TickInputs::held(PLAYER, crate::ControlState::new(toward, false, false));
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
        if world
            .actor_tile(PLAYER)
            // Two tiles in: the twin cache waits on the floor one tile further.
            .is_some_and(|tile| tile.x >= room.interior().min.x + 2)
        {
            inside = true;
            break;
        }
    }
    assert!(
        inside,
        "stalled at {:?} for {:?}",
        world.actor_tile(PLAYER),
        room.bounds
    );
    assert!(world.expedition().anomaly_kinds & anomaly::AnomalyKind::Mirror.bit() != 0);
}
