//! Wave 8's tracks wired together in a Deep Descent world (generator V11): the Fathers'
//! expedition placed in the authored strata and read at their caches, the deep settlements
//! remembering the Fathers, the anomaly rooms through Descents I-III, and the progression's
//! shops matching the gates the Descent tracks hold (the Deep Claim and Drill II, the Star
//! Core's Pressure Hull, the Mycelial Mind's Bio-Filter and Lamp II).

use crate::{
    GameplayEventKind, PlayerId, SimulationParams, TickOutput, WorldState, crust,
    depth_progression::{ProgressionRules, RigSystem, core_tier_cap},
    descent::{Descent, DescentBoss, Stratum},
    economy::{UpgradeKind, UpgradeLock, upgrade_lock_for, upgrade_offers_for},
    expedition::{
        ExpeditionState, LOGS,
        anomaly::rooms_with,
        memories::{Settlement, line_for_visit},
        sites,
    },
    old_deep::{self, DeepTile},
    terrain::TilePosition,
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{crust as crust_rules, layout, living},
    },
};

const PLAYER: PlayerId = PlayerId::new(0);

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

fn rect(min: (i32, i32), max: (i32, i32)) -> TileRect {
    TileRect {
        min: TilePosition { x: min.0, y: min.1 },
        max: TilePosition { x: max.0, y: max.1 },
    }
}

/// The strata tracks' set pieces of a world: the Crust's cabin, Gravel Hollow's hall and the
/// Sinkhole; the Old Deep's Cathedral, Star Core arena, Glowmother's den and Fathers' camps.
/// (The mine works and the Living Dark's stamps are looked up per area.)
fn set_pieces(settings: &WorldSettings) -> Vec<(&'static str, TileRect)> {
    let layout = layout(settings);
    let mut pieces = Vec::new();
    pieces.extend(crust::cabin(settings).map(|cabin| ("the cabin", cabin)));
    if let Some((center, top)) = crust::gravel_hollow(settings) {
        pieces.push((
            "Gravel Hollow",
            rect((center - 40, top - 6), (center + 40, top + 14)),
        ));
    }
    pieces.extend(crust::sinkhole_site(settings).map(|site| ("the Sinkhole", site.zone)));
    pieces.extend(
        old_deep::sites::cathedral(settings, &layout).map(|site| ("the Cathedral", site.bounds)),
    );
    pieces.extend(
        old_deep::sites::star_core_arena(settings, &layout)
            .map(|site| ("the Star Core", site.bounds)),
    );
    if let Some((centre, (w, h))) = old_deep::sites::glowmother_den(settings, &layout) {
        pieces.push((
            "the Glowmother's den",
            rect((centre.x - w, centre.y - h), (centre.x + w, centre.y + h)),
        ));
    }
    for stratum in Descent::OldDeep.strata() {
        if let Some(camp) = old_deep::sites::expedition_camp(settings, &layout, stratum) {
            pieces.push((
                "a Fathers' camp",
                rect((camp.x - 3, camp.y - 3), (camp.x + 3, camp.y + 1)),
            ));
        }
    }
    pieces
}

/// What of the strata tracks' authored places `area` crosses, if anything.
fn crossed(
    settings: &WorldSettings,
    pieces: &[(&'static str, TileRect)],
    area: &TileRect,
) -> Option<String> {
    let layout = layout(settings);
    if let Some((name, _)) = pieces.iter().find(|(_, bounds)| bounds.intersects(area)) {
        return Some((*name).to_owned());
    }
    if let Some(works) = crust_rules::works_touching(settings, &layout, area)
        .into_iter()
        .find(|works| works.bounds().intersects(area))
    {
        return Some(format!("the mine works at row {}", works.rail_row));
    }
    living::stamps_with(settings, &layout, area)
        .into_iter()
        .find(|stamp| stamp.bounds.intersects(area))
        .map(|stamp| format!("the Living Dark's {:?}", stamp.kind))
}

#[test]
fn the_fathers_camps_and_the_anomaly_rooms_keep_off_the_strata_set_pieces() {
    for seed in [0, 1, 7, 0x5EED] {
        let settings = v11(seed);
        let layout = layout(&settings);
        let pieces = set_pieces(&settings);
        // Every log site lies in its own log's stratum, clear of every set piece.
        let logs = sites::sites(&settings);
        assert_eq!(logs.len(), LOGS.len(), "seed {seed}");
        for site in &logs {
            let log = &LOGS[usize::from(site.log)];
            assert_eq!(
                layout.stratum_of_row(site.bounds.center().y),
                log.stratum,
                "seed {seed}: log {}",
                site.log + 1
            );
            assert_eq!(
                crossed(&settings, &pieces, &site.bounds),
                None,
                "seed {seed}: log {} at {:?}",
                site.log + 1,
                site.bounds
            );
        }
        // The anomaly rooms of Descents I-III: there are some, and none cut a set piece.
        let (top, _) = layout.descent_rows(Descent::Crust);
        let (_, bottom) = layout.descent_rows(Descent::LivingDark);
        let rooms = rooms_with(&settings, &layout, &rect((-600, top), (600, bottom)));
        assert!(rooms.len() >= 10, "seed {seed}: {} rooms", rooms.len());
        let descents: std::collections::BTreeSet<Descent> = rooms
            .iter()
            .map(|room| layout.stratum_of_row(room.bounds.center().y).descent())
            .collect();
        assert_eq!(descents.len(), 3, "seed {seed}: rooms in {descents:?}");
        for room in &rooms {
            assert_eq!(
                crossed(&settings, &pieces, &room.bounds),
                None,
                "seed {seed}: {:?} room at {:?}",
                room.kind,
                room.bounds
            );
        }
    }
}

#[test]
fn an_old_deep_fathers_camp_reads_the_expedition_log_of_its_stratum() {
    let settings = v11(3);
    let layout = layout(&settings);
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    world.grant_deep_claim();
    // Each camp holds the first log the expedition wrote in its stratum.
    for (index, stratum) in Descent::OldDeep.strata().enumerate() {
        let camp_log = old_deep::sites::camp_log(stratum);
        assert_eq!(usize::from(camp_log), index);
        let number = old_deep::expedition_log_of_camp(camp_log).expect("a story log");
        assert_eq!(LOGS[usize::from(number - 1)].stratum, stratum);
    }
    assert_eq!(
        old_deep::expedition_log_of_camp(old_deep::LOG_TABLET_FIRST),
        None
    );
    assert_eq!(
        old_deep::expedition_log_of_camp(old_deep::LOG_CORE_REVEAL),
        None
    );
    let camp = old_deep::sites::expedition_camp(&settings, &layout, Stratum::FossilOceans)
        .expect("the Fossil Oceans' camp");
    let number = old_deep::expedition_log_of_camp(old_deep::sites::camp_log(Stratum::FossilOceans))
        .expect("its log");
    // Reached before the trail: the canister stays sealed.
    let mut output = TickOutput::default();
    world.mined_deep_tile(PLAYER, camp, DeepTile::ExpeditionLog, &mut output);
    assert!(output.events.iter().any(|event| event.kind
        == GameplayEventKind::ExpeditionLogSealed {
            player_id: PLAYER,
            log: number,
        }));
    assert!(world.expedition().sealed.contains(&number));
    // With the trail read up to it, the camp opens the log, and the journal moves on.
    world.set_expedition(ExpeditionState {
        found: number - 1,
        ..world.expedition().clone()
    });
    let mut output = TickOutput::default();
    world.mined_deep_tile(PLAYER, camp, DeepTile::ExpeditionLog, &mut output);
    assert!(output.events.iter().any(|event| event.kind
        == GameplayEventKind::ExpeditionLogFound {
            player_id: PLAYER,
            log: number,
        }));
    assert!(world.expedition().found >= number);
}

#[test]
fn the_deep_settlements_remember_the_fathers_as_the_trail_is_read() {
    let fresh = ExpeditionState::default();
    let far = ExpeditionState {
        found: u8::try_from(LOGS.len()).unwrap_or(u8::MAX),
        ..ExpeditionState::default()
    };
    for settlement in Settlement::ALL {
        // Something to say from the start, more once the trail is read.
        let early = line_for_visit(settlement, &fresh, 0).expect("an open line");
        assert!(early.after_log.is_none(), "{settlement:?}");
        let told: std::collections::BTreeSet<&str> = (0..20)
            .filter_map(|visit| line_for_visit(settlement, &far, visit))
            .map(|memory| memory.id)
            .collect();
        assert!(told.len() >= 4, "{settlement:?}: {told:?}");
        assert!(early.told().contains(early.line));
    }
    // The newest line opened is the first told.
    let gravel = line_for_visit(Settlement::GravelHollow, &far, 0).expect("a line");
    assert_eq!(gravel.after_log, Some(6));
}

#[test]
fn gravel_hollows_people_tell_of_the_fathers_when_asked_for_news() {
    use crate::{
        PlayerCommand, TickInputs,
        entity::{EntityKind, EntityState},
        people::{PeopleCommand, TalkTopic},
        position::WorldPos,
        village::{self, placement::underground_id},
    };
    let settings = v11(0);
    let hollow = village::village(&settings, underground_id(0)).expect("Gravel Hollow");
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    world.set_enemy_difficulty(crate::enemy::EnemyDifficulty::Peaceful);
    world.set_expedition(ExpeditionState {
        found: 6,
        ..ExpeditionState::default()
    });
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(
            i64::from(hollow.center_column()),
            i64::from(hollow.ground_row - 1),
        ),
    );
    world.stream_terrain_near_players();
    for _ in 0..180 {
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    }
    let rig = world.player(PLAYER).expect("rig");
    let (rx, ry) = (rig.x, rig.y);
    let npc = world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter(|entity| matches!(entity.state, EntityState::Npc(_)))
        .min_by(|a, b| {
            (a.body.x - rx)
                .hypot(a.body.y - ry)
                .total_cmp(&(b.body.x - rx).hypot(b.body.y - ry))
        })
        .expect("Gravel Hollow's people")
        .id;
    // Talk needs the person in reach: stand the rig beside them.
    let (x, y) = world.npc_position(npc).expect("npc");
    let at = world.world_pos(x, y - 2.0);
    world.teleport_rig_to(PLAYER, at);
    let inputs = TickInputs::default().act(
        PLAYER,
        PlayerCommand::People(PeopleCommand::Talk {
            npc,
            topic: TalkTopic::Rumours,
        }),
    );
    let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    let conversation = world.people().conversation(PLAYER).expect("a conversation");
    assert!(
        conversation
            .lines
            .iter()
            .any(|line| line.text.starts_with("The Fathers?")),
        "{:?}",
        conversation.lines
    );
}

#[test]
fn the_shops_match_the_gates_the_descents_hold() {
    let settings = v11(5);
    let layout = crate::depth_progression::layout_for(&settings);
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    world.set_progression_rules(ProgressionRules::Descent);
    let rules = ProgressionRules::Descent;
    let lock = |world: &WorldState, kind: UpgradeKind, deepest: i32| {
        let index = UpgradeKind::ALL
            .iter()
            .position(|each| *each == kind)
            .expect("line");
        let offer = upgrade_offers_for(world.player(PLAYER).expect("rig"), rules)[index];
        upgrade_lock_for(&offer, &settings, deepest, rules, u8::MAX)
    };
    // The Crust's drill cap stops one short of Drill II: the Sinkhole's Deep Claim gives it,
    // and the shop's next tier opens with the Old Deep.
    assert_eq!(crust::DRILL_II_LEVEL, core_tier_cap(0) + 1);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.drill_strength = core_tier_cap(0);
    }
    let crust_floor = crate::depth_progression::DescentLayout::descent(&layout, 0)
        .map_or(0, |span| span.top + 10);
    assert!(matches!(
        lock(&world, UpgradeKind::Drill, crust_floor),
        Some(UpgradeLock::Descent(_))
    ));
    world.grant_deep_claim();
    world.player_mut(PLAYER).expect("rig").drill_strength = crust::DRILL_II_LEVEL;
    // The bosses' rewards are won, never sold.
    for (kind, boss) in [
        (UpgradeKind::DescentPressureHull, DescentBoss::StarCore),
        (UpgradeKind::BioFilter, DescentBoss::MycelialMind),
        (UpgradeKind::LampII, DescentBoss::MycelialMind),
    ] {
        assert_eq!(
            lock(&world, kind, i32::MAX),
            Some(UpgradeLock::Boss(boss)),
            "{kind:?}"
        );
    }
    // And the registry's gates ask for exactly those rewards.
    let gate = |descent: Descent| descent.last_stratum().def().gate.expect("a gate");
    assert_eq!(gate(Descent::OldDeep).boss, DescentBoss::StarCore);
    assert!(
        gate(Descent::OldDeep)
            .rewards
            .contains(&crate::descent::DescentKey::PressureHull)
    );
    assert_eq!(gate(Descent::LivingDark).boss, DescentBoss::MycelialMind);
    for system in [
        RigSystem::PressureHull,
        RigSystem::BioFilter,
        RigSystem::LampII,
    ] {
        assert!(system.boss_reward().is_some(), "{system:?}");
    }
}
