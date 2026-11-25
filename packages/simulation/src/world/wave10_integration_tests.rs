//! Wave 10 integration: the seams between the five tracks (Descent VII's buried city and
//! garrison, Descent VIII's Heart, the story's finale and the polish), checked on one world.

use crate::{
    GameplayEventKind, PlayerId, SectionKey, SimulationParams, TickInputs, WorldState,
    ancient_city::archivist::{self, STORY_LINE_BASE, Trigger},
    depth_progression::RigSystem,
    descent::{Descent, DescentBoss, Stratum},
    enemy::EnemyDifficulty,
    expedition::{
        LOGS,
        archivist::ARCHIVIST_LINES,
        progress::StoryEvent,
        sites::{self, DEEPER_SET_PIECES},
    },
    position::WorldPos,
    worldgen::{GeneratorVersion, WorldSettings, descent::layout},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x1A7E_6A7E;

fn settings(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

/// A V11 world whose rig carries every gate's reward below Descent I (and the Heart Key).
fn deep_world(seed: u64) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(seed), SimulationParams::new(30));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    for system in RigSystem::ALL {
        crate::economy::grant_rig_system(rig, system);
    }
    rig.hull = rig.max_hull();
    rig.fuel = rig.fuel_capacity;
    world
}

fn middle_row(world: &WorldState, stratum: Stratum) -> i32 {
    let (top, bottom) = layout(world.world_settings()).rows(stratum);
    i32::midpoint(top, bottom)
}

fn tick(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let output = world.simulate_tick(
        world.simulation_tick(),
        &TickInputs::held(PLAYER, crate::ControlState::default()),
    );
    output.events.into_iter().map(|event| event.kind).collect()
}

/// The archivist companion (ancients-a) speaks the story track's lines: every other musing is
/// one of [`ARCHIVIST_LINES`], said by an id past [`STORY_LINE_BASE`] that the game's speech
/// box reads back, each once.
#[test]
fn the_archivist_muses_in_the_story_tracks_words() {
    let mut world = deep_world(SEED);
    let row = middle_row(&world, Stratum::BuriedMegacity);
    world.teleport_rig_to(PLAYER, WorldPos::tile_center(0, i64::from(row)));
    world.stream_terrain_near_players();
    world.wake_archivist_for(PLAYER);
    // The crew has reached Descent VII and read some of the trail.
    for descent in Descent::ALL.into_iter().take(Descent::Ancients.index() + 1) {
        world.expedition.descents |= 1 << descent.index();
    }
    world.expedition.found = 65;
    let mut output = crate::TickOutput::default();
    let mut said = Vec::new();
    for _ in 0..4 {
        // Make the gap since its last line long enough for a musing.
        if let Some(companion) = world.ancient_city.archivists.get_mut(&PLAYER) {
            companion.line = None;
        }
        assert!(world.archivist_say(PLAYER, Trigger::Muse, &mut output));
        let companion = &world.ancient_city.archivists[&PLAYER];
        said.push(companion.line.expect("a line").0);
    }
    let story: Vec<u16> = said
        .iter()
        .copied()
        .filter(|id| *id >= STORY_LINE_BASE)
        .collect();
    assert_eq!(
        story.len(),
        2,
        "every other musing is the story's: {said:?}"
    );
    for id in &said {
        assert!(archivist::line_text(*id).is_some(), "line {id} has words");
    }
    for id in story {
        let line = &ARCHIVIST_LINES[usize::from(id - STORY_LINE_BASE)];
        assert!(line.open(world.expedition()), "{} is open", line.id);
        assert_eq!(archivist::line_text(id), Some(line.line));
    }
    let mut unique = said.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), said.len(), "each line once: {said:?}");
}

/// The Colossus's own fight state (ancients-b) and the Heart's (heart) are what the story's
/// Descent progress reads: beating the Colossus is noted, and waking the Heart lights the cable
/// for the story's ending as well as the Heart's own (space, the Star Drive, the beam).
#[test]
fn the_gate_fights_drive_the_story_and_waking_the_heart_ends_it() {
    let mut world = deep_world(SEED);
    let row = middle_row(&world, Stratum::TheHeart);
    world.teleport_rig_to(PLAYER, WorldPos::tile_center(0, i64::from(row)));
    world.stream_terrain_near_players();
    assert!(!world.descent_boss_beaten(DescentBoss::GatekeeperColossus));
    world.garrison.colossus.beaten = true;
    assert!(world.descent_boss_beaten(DescentBoss::GatekeeperColossus));
    let events = tick(&mut world);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::Story {
            what: StoryEvent::BossBeaten(DescentBoss::GatekeeperColossus),
            ..
        }
    )));
    assert!(!world.expedition().cable_lit);
    let mut output = crate::TickOutput::default();
    let millis = world.millis_at(world.simulation_tick());
    world.heart_wakes(Some(PLAYER), millis, &mut output);
    assert!(world.space_unlocked());
    assert!(world.has_star_drive(PLAYER));
    assert!(world.ending_elapsed().is_some());
    assert!(world.cable_path().is_some());
    let mut lit = false;
    for _ in 0..3 {
        lit |= tick(&mut world).iter().any(|event| {
            matches!(
                event,
                GameplayEventKind::Story {
                    what: StoryEvent::CableLit,
                    ..
                }
            )
        });
    }
    assert!(lit, "the story's ending fires off the Heart's own waking");
    assert!(world.expedition().cable_lit);
    assert!(world.expedition().beat(DescentBoss::TheHeart));
}

/// Every new section is shared (replicated to clients), and a V11 world carrying all of
/// Wave 10's state (star charts, the garrison's gate, the Heart's flags, the story's
/// progress) saves and loads to the same state hash.
#[test]
fn wave_ten_state_is_shared_and_round_trips_a_save() {
    for key in [
        SectionKey::AncientCity,
        SectionKey::Garrison,
        SectionKey::Heart,
        SectionKey::Expedition,
        SectionKey::Logistics,
    ] {
        assert!(SectionKey::SHARED.contains(&key), "{key:?} is shared");
    }
    let mut world = deep_world(SEED);
    let chart = crate::star_chart::chart_at_site(SEED, 12, 3_400, 500, world.simulation_tick());
    let system = chart.system;
    world.ancient_city.charts.push(chart);
    world.garrison.colossus.beaten = true;
    let mut output = crate::TickOutput::default();
    let millis = world.millis_at(world.simulation_tick());
    world.heart_wakes(Some(PLAYER), millis, &mut output);
    for _ in 0..3 {
        let _ = tick(&mut world);
    }
    assert!(world.star_chart_known(system));
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    assert!(loaded.star_chart_known(system));
    assert!(loaded.space_unlocked());
    assert!(loaded.has_star_drive(PLAYER));
    assert!(loaded.expedition().cable_lit);
    assert!(loaded.descent_boss_beaten(DescentBoss::GatekeeperColossus));
    assert_eq!(loaded.ending_elapsed(), world.ending_elapsed());
}

/// The story's last twenty logs lie in Descents VII and VIII, each canister clear of the
/// buried city's, the garrison's and the Heart's set pieces, on three seeds.
#[test]
fn the_last_logs_lie_in_the_builders_world_off_its_set_pieces() {
    for seed in [SEED, 0, 7] {
        let settings = settings(seed);
        let layout = layout(&settings);
        let sites = sites::sites(&settings);
        for site in sites.iter().filter(|site| usize::from(site.log) >= 60) {
            let stratum = site.stratum;
            assert!(
                stratum.descent().index() >= Descent::Ancients.index(),
                "log {} lies in {stratum:?}",
                site.log + 1
            );
            let room = site.bounds;
            assert!(
                !crate::worldgen::descent::garrison::crosses_set_pieces(&settings, &layout, &room),
                "log {} sits in the garrison's set pieces (seed {seed})",
                site.log + 1
            );
            assert!(
                !DEEPER_SET_PIECES
                    .iter()
                    .any(|crosses| crosses(&settings, &room)),
                "log {} sits in a deeper set piece (seed {seed})",
                site.log + 1
            );
        }
        assert_eq!(sites.len(), LOGS.len());
    }
}

/// The new Descents' rewards gate the way down: the Heart Key the Colossus hands over opens
/// both the Heart Seal over Descent VIII and the Heart chamber's own seal, and nothing else
/// does.
#[test]
fn the_heart_key_opens_both_seals_and_only_it_does() {
    use crate::{cave::CaveTile, heart::CoreTile, terrain::TileKind};
    let mut world = deep_world(SEED);
    let seals = [
        TileKind::Cave(CaveTile::HeartSeal),
        TileKind::Core(CoreTile::HeartSeal),
    ];
    for seal in seals {
        assert!(!world.heart_seal_refuses(PLAYER, seal), "{seal:?} yields");
    }
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hazard_gear.rig_systems &= !RigSystem::HeartKey.bit();
    for seal in seals {
        assert!(world.heart_seal_refuses(PLAYER, seal), "{seal:?} refuses");
    }
    assert!(!world.heart_seal_refuses(PLAYER, TileKind::Core(CoreTile::StarRock)));
}

/// The Heart Seal over Descent VIII is whole where the Heart's own pass runs under it: no
/// weightless hall, vein or cave of the Veins cuts it, so only the Heart Key opens the way
/// down (three seeds, a wide span of columns).
#[test]
fn the_heart_seal_is_whole_over_the_hearts_own_halls() {
    use crate::{cave::CaveTile, terrain::TileKind, worldgen::streamed::generated_tile};
    for seed in [SEED, 0, 7] {
        let settings = settings(seed);
        let layout = layout(&settings);
        let (top, _) = layout.rows(Stratum::DarkMatterVeins);
        for x in (-700..700).step_by(3) {
            let rows: Vec<i32> = (top - 80..top + 80)
                .filter(|y| {
                    crate::worldgen::descent::garrison::heart_seal(&settings, &layout, x, *y)
                })
                .collect();
            assert!(!rows.is_empty(), "the seal spans column {x} (seed {seed})");
            for y in rows {
                let tile = crate::terrain::TilePosition { x, y };
                let kind = generated_tile(&settings, tile).kind;
                // The cable's lift shaft (ruin masonry) and bedrock hold the line too.
                assert!(
                    matches!(
                        kind,
                        TileKind::Cave(CaveTile::HeartSeal)
                            | TileKind::Bedrock
                            | TileKind::Built(crate::base::BuiltTile::RuinWall)
                    ),
                    "the seal at {tile:?} is {kind:?} (seed {seed})"
                );
            }
        }
    }
}
