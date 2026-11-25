//! The story's finale in a Deep Descent world (Wave 10 story): the crew's progress through the
//! Descent (Descents reached, gate bosses beaten), the ending (the Heart woken and the cable
//! lit), the finale's milestones firing once, the last logs read in order, and the journal's new
//! fields through saves, hashes and co-op replication.

use crate::{
    GameplayEventKind, PlayerId, SectionKey, SimulationParams, TickInputs, WorldState,
    descent::{Descent, DescentBoss},
    enemy::EnemyDifficulty,
    expedition::{
        ExpeditionState, LOGS,
        progress::{StoryEvent, descent_rows, totals},
        sites,
    },
    milestone::Milestone,
    position::WorldPos,
    terrain::TilePosition,
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);
const PARTNER: PlayerId = PlayerId::new(1);

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

/// A world whose rig is fitted for every Descent: the crew holds the Deep Claim and the rig
/// every earlier gate's reward, all its protection and a strong hull.
fn deep_world(seed: u64) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &v11(seed), SimulationParams::default());
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    world.add_player(PARTNER);
    for player in [PLAYER, PARTNER] {
        let rig = world.player_mut(player).expect("rig");
        rig.hazard_gear = crate::protection::HazardGear::MAXED;
        rig.radiator_level = 15;
        rig.hull_level = 15;
        rig.hull = rig.max_hull();
    }
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

fn story(events: &[GameplayEventKind]) -> Vec<StoryEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Story { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

fn earned(events: &[GameplayEventKind], player: PlayerId) -> Vec<Milestone> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::MilestoneEarned {
                player_id,
                milestone,
            } if *player_id == player => Some(*milestone),
            _ => None,
        })
        .collect()
}

/// A canister of log `number` in Descent `descent`: the rig parked there stands in it.
fn first_site_in(world: &WorldState, descent: Descent) -> TilePosition {
    sites::sites(world.terrain().settings())
        .into_iter()
        .find(|site| site.stratum.descent() == descent)
        .expect("a log site in every Descent")
        .canister()
}

#[test]
fn reaching_a_descent_records_it_and_the_ones_above_once() {
    let mut world = deep_world(0x5EED);
    // The first tick primes every milestone silently.
    let first = step(&mut world);
    assert_eq!(
        story(&first),
        vec![StoryEvent::BossBeaten(DescentBoss::TheSinkhole)],
        "the town is not underground (the Deep Claim counts as the Sinkhole won)"
    );
    // Into the Drowned and Frozen: Descents I-IV count as reached, one event for IV.
    let at = first_site_in(&world, Descent::DrownedAndFrozen);
    put(&mut world, PLAYER, at);
    let events = step(&mut world);
    assert_eq!(
        story(&events),
        vec![StoryEvent::DescentReached(Descent::DrownedAndFrozen)]
    );
    for descent in Descent::ALL {
        assert_eq!(
            world.expedition().reached(descent),
            descent <= Descent::DrownedAndFrozen,
            "{descent:?}"
        );
    }
    let mut milestones = earned(&events, PLAYER);
    // Standing there longer raises nothing new.
    for _ in 0..3 {
        let again = step(&mut world);
        assert!(story(&again).is_empty());
        milestones.extend(earned(&again, PLAYER));
    }
    for milestone in [
        Milestone::ReachedOldDeep,
        Milestone::ReachedLivingDark,
        Milestone::ReachedDrowned,
    ] {
        assert_eq!(
            milestones.iter().filter(|m| **m == milestone).count(),
            1,
            "{milestone:?} fires once"
        );
    }
    assert!(!milestones.contains(&Milestone::ReachedHollow));
    // The partner's rig earns the crew's milestones too (the journal is shared).
    assert!(
        world
            .player(PARTNER)
            .expect("rig")
            .milestones
            .has(Milestone::ReachedDrowned)
    );
    // The partner goes on to the Heart: one event for VIII, every Descent now reached.
    let at = first_site_in(&world, Descent::Heart);
    put(&mut world, PARTNER, at);
    let events = step(&mut world);
    assert_eq!(
        story(&events),
        vec![StoryEvent::DescentReached(Descent::Heart)]
    );
    assert!(Descent::ALL.iter().all(|d| world.expedition().reached(*d)));
    assert_eq!(totals(world.expedition()).0, 8);
}

#[test]
fn gate_bosses_are_tracked_and_waking_the_heart_lights_the_cable_once() {
    let mut world = deep_world(0xB055);
    let at = first_site_in(&world, Descent::Crust);
    put(&mut world, PLAYER, at);
    let mut all = step(&mut world);
    // The Deep Claim was granted: the Sinkhole counts as beaten.
    assert!(story(&all).contains(&StoryEvent::BossBeaten(DescentBoss::TheSinkhole)));
    // Fights with their own state are read; the Ancients' and the Heart's are noted.
    world.living_dark.mind.beaten = true;
    world.drowned.leviathan.beaten = true;
    let events = step(&mut world);
    assert_eq!(
        story(&events),
        vec![
            StoryEvent::BossBeaten(DescentBoss::MycelialMind),
            StoryEvent::BossBeaten(DescentBoss::Leviathan),
        ]
    );
    all.extend(events);
    world.note_descent_boss_beaten(DescentBoss::GatekeeperColossus);
    world.note_descent_boss_beaten(DescentBoss::GatekeeperColossus);
    let events = step(&mut world);
    assert_eq!(
        story(&events),
        vec![StoryEvent::BossBeaten(DescentBoss::GatekeeperColossus)]
    );
    all.extend(events);
    assert!(!world.expedition().cable_lit);
    world.note_descent_boss_beaten(DescentBoss::TheHeart);
    let events = step(&mut world);
    assert_eq!(
        story(&events),
        vec![
            StoryEvent::BossBeaten(DescentBoss::TheHeart),
            StoryEvent::CableLit
        ]
    );
    assert!(world.expedition().cable_lit);
    all.extend(events);
    for _ in 0..3 {
        let again = step(&mut world);
        assert!(story(&again).is_empty(), "every beat fires once");
        all.extend(again);
    }
    let milestones = earned(&all, PLAYER);
    // The first check primes silently, the Sinkhole with it.
    assert!(
        world
            .player(PLAYER)
            .expect("rig")
            .milestones
            .has(Milestone::BeatSinkhole)
    );
    for milestone in [
        Milestone::BeatMind,
        Milestone::BeatLeviathan,
        Milestone::BeatColossus,
        Milestone::LookUp,
    ] {
        assert_eq!(
            milestones.iter().filter(|m| **m == milestone).count(),
            1,
            "{milestone:?}"
        );
    }
    assert!(!milestones.contains(&Milestone::BeatStarCore));
    // The page: five of eight gates won.
    let rows = descent_rows(world.expedition());
    assert_eq!(rows.iter().filter(|row| row.boss_beaten).count(), 5);
    assert_eq!(totals(world.expedition()).1, 5);
    assert!(rows[7].boss_beaten && rows[6].boss_beaten && !rows[1].boss_beaten);
}

#[test]
fn the_last_logs_are_read_in_order_down_to_the_heart() {
    let mut world = deep_world(0x0DD_BA11);
    let mut state = world.expedition().clone();
    state.found = 60;
    world.set_expedition(state);
    let all = sites::sites(world.terrain().settings());
    // The Heart's last canister first: sealed until its turn.
    put(&mut world, PLAYER, all[79].canister());
    let events = step(&mut world);
    assert!(events.contains(&GameplayEventKind::ExpeditionLogSealed {
        player_id: PLAYER,
        log: 80,
    }));
    // Logs 61-79 in order; reading 79 opens the sealed 80 at once.
    let mut read = Vec::new();
    for site in &all[60..79] {
        put(&mut world, PLAYER, site.canister());
        for event in step(&mut world) {
            if let GameplayEventKind::ExpeditionLogFound { log, .. } = event {
                read.push(log);
            }
        }
    }
    assert_eq!(read, (61..=80).collect::<Vec<u8>>());
    assert!(world.expedition().trail_complete());
    let next = step(&mut world);
    let milestones = earned(&next, PLAYER);
    // Chapters VII-VIII and the whole trail (the earlier ones were primed silently: the
    // journal was set before the first check).
    let log = &world.player(PLAYER).expect("rig").milestones;
    for milestone in [
        Milestone::BuildersCity,
        Milestone::StarDriveChapter,
        Milestone::WholeTrail,
    ] {
        assert!(log.has(milestone), "{milestone:?} ({milestones:?})");
    }
    assert_eq!(usize::from(world.expedition().found), LOGS.len());
}

#[test]
fn the_finale_saves_loads_hashes_and_reaches_co_op_partners() {
    let mut host = deep_world(0xC0_0B);
    let mut replica = host.clone();
    let at = first_site_in(&host, Descent::Ancients);
    put(&mut host, PARTNER, at);
    host.note_descent_boss_beaten(DescentBoss::TheHeart);
    step(&mut host);
    let state = host.expedition().clone();
    assert!(state.reached(Descent::Ancients) && state.cable_lit);
    // Saved and loaded.
    let json = serde_json::to_string(&host).expect("json");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(*loaded.expedition(), state);
    assert_eq!(loaded.simulation_state_hash(), host.simulation_state_hash());
    // A journal saved before Wave 10 loads with no progress.
    let mut value = serde_json::to_value(&state).expect("value");
    let object = value.as_object_mut().expect("object");
    for field in ["descents", "bosses", "cable_lit"] {
        object.remove(field);
    }
    let old: ExpeditionState = serde_json::from_value(value).expect("old journal");
    assert_eq!(old.descents, 0);
    assert!(!old.cable_lit);
    // Replicated to a partner.
    let payload = host
        .section(SectionKey::Expedition)
        .expect("shared section");
    replica.apply_section(SectionKey::Expedition, payload);
    assert_eq!(*replica.expedition(), state);
    assert_eq!(descent_rows(replica.expedition()), descent_rows(&state));
}

#[test]
fn classic_worlds_track_no_descent_progress() {
    let mut classic = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(0xC1A5),
        SimulationParams::default(),
    );
    classic.note_descent_boss_beaten(DescentBoss::TheHeart);
    for _ in 0..3 {
        assert!(story(&step(&mut classic)).is_empty());
    }
    assert!(classic.expedition().is_empty());
}
