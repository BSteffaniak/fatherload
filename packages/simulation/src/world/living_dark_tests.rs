//! The Living Dark's mechanics in a Deep Descent world (Wave 8, Descent III): mushroom and
//! cap-crawler bounces, spore clouds (the screen field, the Bio-Filter, the sting), roots and
//! flesh growing back (budgeted, never sealing a body in, scorched by axes and fire), the
//! Hive's politics (errands, theft, a hostile Court, the truce and the broken Swarm,
//! replicated), the acid tide, the Glow Sea's creature-only light, and the Mycelial Mind beaten
//! by a bot with its rewards.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WeaponControl, WorldState,
    cave::CaveTile,
    depth_progression::RigSystem,
    descent::{Descent, Stratum},
    enemy::{Damage, DamageKind, DamageSource, EnemyDifficulty, EnemyKind},
    entity::EntityId,
    field::ScreenEffect,
    fluid::FluidKind,
    living_dark::{
        CourtService, Errand, LivingDarkAction, LivingDarkEvent, POSSESSED_PHASE,
        SPORE_CLOUD_MILLIS, SwarmState, TidePhase, tide_at,
    },
    logistics::LogisticsCommand,
    position::WorldPos,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    weapon::WeaponKind,
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{
            layout,
            living::{self, LivingStampKind},
        },
    },
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x11D4_7E57;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// A Deep Descent world at `hz` with the rig fitted for the deep (every protection and gate
/// system but the Mind's rewards, a strong hull) and the director quiet.
fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    // The crew holds the Deep Claim (the Sinkhole beaten), or Descent I's gate crushes it below.
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hazard_gear.rig_systems &= !(RigSystem::BioFilter.bit() | RigSystem::LampII.bit());
    rig.radiator_level = 15;
    rig.hull_level = 15;
    rig.hull = rig.max_hull();
    rig.credits = 500_000;
    world
}

/// Puts the rig at rest at the centre of tile `tile`.
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

fn fill(world: &mut WorldState, rect: TileRect, kind: TileKind) {
    for position in rect.positions() {
        let _ = world.terrain_mut().set_kind(position, kind);
    }
}

/// One tick with the rig's controls held neutral (a rig with no input does not move).
fn tick(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::held(PLAYER, ControlState::default());
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    output.events.into_iter().map(|event| event.kind).collect()
}

fn ticks(world: &mut WorldState, count: u64) -> Vec<GameplayEventKind> {
    (0..count).flat_map(|_| tick(world)).collect()
}

fn living_events(events: &[GameplayEventKind]) -> Vec<LivingDarkEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::LivingDark { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

/// A row in the middle of `stratum` (layout rows) on the test seed.
fn middle_row(stratum: Stratum) -> i32 {
    let (top, bottom) = layout(&settings()).rows(stratum);
    top + (bottom - top) / 2
}

/// A sealed 13 x 7 arena of stone centred on `(x, y)`, open inside, the rig at its middle.
fn arena(world: &mut WorldState, x: i32, y: i32) {
    put_rig(world, TilePosition { x, y });
    fill(
        world,
        TileRect::around(TilePosition { x, y }, 8, 5),
        TileKind::Stone,
    );
    fill(
        world,
        TileRect::around(TilePosition { x, y }, 6, 3),
        TileKind::Air,
    );
    put_rig(world, TilePosition { x, y: y + 2 });
}

fn act(world: &mut WorldState, action: LivingDarkAction) -> PlayerScopedCommandOutcome {
    let inputs = TickInputs::default().act(
        PLAYER,
        PlayerCommand::Logistics(LogisticsCommand::LivingDark(action)),
    );
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    output
        .action_outcomes
        .first()
        .map_or(PlayerScopedCommandOutcome::IgnoredUnavailable, |outcome| {
            outcome.outcome
        })
}

// --- The Fungal Forest ------------------------------------------------------------------------

#[test]
fn a_rig_bounces_off_a_giant_mushroom_cap_and_a_cap_crawler() {
    let mut world = world_at(60);
    let row = middle_row(Stratum::FungalForest);
    arena(&mut world, 60, row);
    // A cap across the arena's floor, the rig falling onto it from high.
    fill(
        &mut world,
        TileRect {
            min: TilePosition { x: 55, y: row + 3 },
            max: TilePosition { x: 65, y: row + 3 },
        },
        TileKind::Cave(CaveTile::MushroomCap),
    );
    put_rig(&mut world, TilePosition { x: 60, y: row - 2 });
    world.player_mut(PLAYER).expect("rig").velocity_y = 400.0;
    let mut bounced = false;
    for _ in 0..40 {
        let _ = tick(&mut world);
        if world.player(PLAYER).expect("rig").velocity_y < -200.0 {
            bounced = true;
            break;
        }
    }
    assert!(bounced, "the cap threw the rig back up");
    // A cap crawler's back is a cap too.
    let mut world = world_at(60);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    arena(&mut world, 60, row);
    let frame = world.frame();
    let (x, floor_y) = frame.tile_center(TilePosition { x: 60, y: row + 2 });
    let crawler = world.spawn_enemy(EnemyKind::CapCrawler, x, floor_y);
    let crawler_y = world.entities().get(crawler).expect("crawler").body.y;
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.x = x;
    rig.y = crawler_y - 36.0;
    rig.velocity_y = 420.0;
    let events = ticks(&mut world, 20);
    assert!(
        living_events(&events).contains(&LivingDarkEvent::CapBounce),
        "the crawler's cap bounced the rig"
    );
}

#[test]
fn a_burst_spore_pod_clouds_the_screen_stings_and_clears_and_the_bio_filter_sees_through() {
    let mut world = world_at(60);
    let row = middle_row(Stratum::FungalForest);
    arena(&mut world, 60, row);
    let pod = TilePosition { x: 61, y: row + 2 };
    let _ = world
        .terrain_mut()
        .set_kind(pod, TileKind::Cave(CaveTile::SporePod));
    // The rig brushes the pod: it bursts.
    let events = ticks(&mut world, 2);
    assert!(living_events(&events).contains(&LivingDarkEvent::SporeBurst));
    assert_eq!(world.terrain().tile(pod).expect("tile").kind, TileKind::Air);
    assert_eq!(world.living_dark().spores.len(), 1);
    let (x, y) = {
        let rig = world.player(PLAYER).expect("rig");
        (rig.x, rig.y)
    };
    let field = world.fields_at(x, y);
    assert!(field.screen_level(ScreenEffect::Blur) > 0.3, "{field:?}");
    assert!(field.screen_level(ScreenEffect::ColourGrade) > 0.3);
    // It stings a hull without the Bio-Filter.
    let hull = world.player(PLAYER).expect("rig").hull;
    let _ = ticks(&mut world, 60);
    assert!(
        world.player(PLAYER).expect("rig").hull < hull,
        "the spores sting"
    );
    // The Bio-Filter's rig sees through it (its view) and takes no sting.
    crate::economy::grant_rig_system(world.player_mut(PLAYER).expect("rig"), RigSystem::BioFilter);
    let seen = world.view_fields_for(PLAYER, x, y);
    assert!(seen.screen_level(ScreenEffect::Blur) < 0.2, "{seen:?}");
    let hull = world.player(PLAYER).expect("rig").hull;
    let _ = ticks(&mut world, 30);
    assert!(
        (world.player(PLAYER).expect("rig").hull - hull).abs() < f32::EPSILON,
        "the Bio-Filter keeps spores out"
    );
    // The cloud clears in its time, and its field zone with it.
    let _ = ticks(&mut world, SPORE_CLOUD_MILLIS * 60 / 1_000);
    assert!(world.living_dark().spores.is_empty());
    assert!(world.field_zones().is_empty());
}

// --- Roots and flesh ------------------------------------------------------------------------

/// Ticks until `tile` is `kind` (at most `limit` ticks); returns the ticks it took.
fn ticks_until(
    world: &mut WorldState,
    tile: TilePosition,
    kind: TileKind,
    limit: u64,
) -> Option<u64> {
    for count in 0..limit {
        if world.terrain().tile(tile).map(|tile| tile.kind) == Some(kind) {
            return Some(count);
        }
        let _ = tick(world);
    }
    None
}

#[test]
fn roots_grow_back_over_a_dug_tunnel_but_never_into_a_rig_and_never_where_axes_or_fire_cut() {
    let hz = 20;
    let mut world = world_at(hz);
    let row = middle_row(Stratum::RootLabyrinth);
    arena(&mut world, 60, row);
    let tunnel = TileRect {
        min: TilePosition { x: 40, y: row },
        max: TilePosition { x: 50, y: row },
    };
    fill(&mut world, tunnel.expanded(2), TileKind::Stone);
    world.dig_living_tiles(tunnel);
    assert_eq!(world.living_schedule().waiting.len(), 11);
    // The waiting roots save with the world (integration fix: tile-keyed maps save as pairs,
    // since JSON keys must be strings).
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.living_schedule(), world.living_schedule());
    // A root axe's shaft and its walls never close.
    let probe = TilePosition { x: 45, y: row };
    let regrown = ticks_until(
        &mut world,
        probe,
        TileKind::Cave(CaveTile::Root),
        u64::from(hz) * 200,
    )
    .expect("the roots grew back");
    let seconds = regrown / u64::from(hz);
    assert!((60..=200).contains(&seconds), "grew back after {seconds} s");
    // Over minutes, not all at once: the whole tunnel is shut once the last tile is due.
    let _ = ticks(&mut world, u64::from(hz) * 80);
    assert!(tunnel.positions().all(
        |tile| world.terrain().tile(tile).expect("tile").kind == TileKind::Cave(CaveTile::Root)
    ));
    // A tile with the rig in it waits.
    let mut world = world_at(hz);
    arena(&mut world, 60, row);
    let _ = ticks(&mut world, u64::from(hz));
    let under = world.player(PLAYER).expect("rig").tile_in(world.frame());
    let _ = world
        .terrain_mut()
        .set_kind(under, TileKind::Cave(CaveTile::Root));
    world.dig_living_tiles(TileRect {
        min: under,
        max: under,
    });
    let _ = ticks(&mut world, u64::from(hz) * 220);
    assert_eq!(
        world.terrain().tile(under).expect("tile").kind,
        TileKind::Air
    );
    assert!(
        world.living_schedule().is_waiting(under),
        "it tries again later"
    );
    // A root axe cuts a shaft up through roots that never grows back.
    let mut world = world_at(hz);
    arena(&mut world, 60, row);
    let at = world.player(PLAYER).expect("rig").tile_in(world.frame());
    let above = TileRect {
        min: TilePosition {
            x: at.x - 1,
            y: at.y - 14,
        },
        max: TilePosition {
            x: at.x + 1,
            y: at.y - 1,
        },
    };
    fill(&mut world, above, TileKind::Cave(CaveTile::Root));
    assert_eq!(
        act(&mut world, LivingDarkAction::FireRootAxe),
        PlayerScopedCommandOutcome::IgnoredUnavailable,
        "no axes aboard"
    );
    world.living_dark.axes.insert(PLAYER, 1);
    assert_eq!(
        act(&mut world, LivingDarkAction::FireRootAxe),
        PlayerScopedCommandOutcome::Applied
    );
    let shaft = TilePosition {
        x: at.x,
        y: at.y - 6,
    };
    assert_eq!(
        world.terrain().tile(shaft).expect("tile").kind,
        TileKind::Air
    );
    assert!(world.living_schedule().scorched.contains_key(&shaft));
    let _ = ticks(&mut world, u64::from(hz) * 220);
    assert_eq!(
        world.terrain().tile(shaft).expect("tile").kind,
        TileKind::Air
    );
    assert_eq!(world.living_dark().axes_of(PLAYER), 0);
}

#[test]
fn roots_never_close_a_lift_shaft() {
    let hz = 10;
    let mut world = world_at(hz);
    let row = middle_row(Stratum::RootLabyrinth);
    arena(&mut world, 60, row);
    let column = TileRect {
        min: TilePosition { x: 45, y: row - 8 },
        max: TilePosition { x: 45, y: row + 8 },
    };
    fill(&mut world, column.expanded(2), TileKind::Stone);
    let id = world.logistics.allocate_id();
    world.logistics.lifts.insert(
        id,
        crate::logistics::LiftShaft {
            id,
            column: 45,
            stations: [row - 8, row + 8].into_iter().collect(),
            origin: crate::logistics::LiftOrigin::Built { owner: PLAYER },
            restored: true,
            cab_row: row + 8,
        },
    );
    // The shaft and a side tunnel off it are dug the same moment: only the side tunnel closes.
    let side = TilePosition { x: 46, y: row };
    world.dig_living_tiles(column);
    world.dig_living_tiles(TileRect {
        min: side,
        max: side,
    });
    let _ = ticks(&mut world, u64::from(hz) * 220);
    assert!(
        column
            .positions()
            .all(|tile| world.terrain().tile(tile).expect("tile").kind == TileKind::Air),
        "the lift's shaft stays open"
    );
    assert_eq!(
        world.terrain().tile(side).expect("tile").kind,
        TileKind::Cave(CaveTile::Root)
    );
}

#[test]
fn fire_burns_roots_away_for_good() {
    let mut world = world_at(30);
    let row = middle_row(Stratum::RootLabyrinth);
    arena(&mut world, 60, row);
    let rect = TileRect::around(TilePosition { x: 60, y: row - 1 }, 2, 1);
    fill(&mut world, rect, TileKind::Cave(CaveTile::Root));
    let frame = world.frame();
    let (x, y) = frame.tile_center(TilePosition { x: 60, y: row - 1 });
    let blast = super::bombs::Blast {
        kind: crate::ExplosiveKind::Incendiary,
        x,
        y,
        owner: Some(PLAYER),
    };
    let mut output = crate::TickOutput::default();
    world.living_dark_blasts(&[blast], &mut output);
    assert!(
        rect.positions()
            .all(|tile| world.terrain().tile(tile).expect("tile").kind == TileKind::Air)
    );
    assert!(
        rect.positions()
            .all(|tile| world.living_schedule().scorched.contains_key(&tile))
    );
}

#[test]
fn mined_flesh_heals_back_faster_deeper() {
    let hz = 30;
    let mut world = world_at(hz);
    let (top, bottom) = layout(&settings()).rows(Stratum::FleshCaverns);
    let shallow = top + 30;
    let deep = bottom - 30;
    let mut took = Vec::new();
    for row in [shallow, deep] {
        arena(&mut world, 60, row);
        let wall = TileRect {
            min: TilePosition { x: 40, y: row },
            max: TilePosition { x: 41, y: row },
        };
        fill(
            &mut world,
            wall.expanded(1),
            TileKind::Cave(CaveTile::Flesh),
        );
        world.dig_living_tiles(wall);
        let healed = ticks_until(
            &mut world,
            wall.min,
            TileKind::Cave(CaveTile::Flesh),
            u64::from(hz) * 80,
        )
        .expect("the flesh healed");
        took.push(healed / u64::from(hz));
    }
    assert!((15..=60).contains(&took[0]), "{took:?}");
    assert!(took[1] < took[0], "deeper heals faster: {took:?}");
}

// --- The Hive -------------------------------------------------------------------------------

fn kill(world: &mut WorldState, id: EntityId) {
    let mut output = crate::TickOutput::default();
    let _ = world.damage_enemy(
        id,
        Damage::new(1.0e6, DamageKind::Explosive, DamageSource::Player(PLAYER)),
        &mut output,
    );
}

#[test]
#[allow(clippy::too_many_lines, reason = "one story of the Hive, start to end")]
fn the_hive_remembers_the_crews_choices_and_replicates_them() {
    let mut world = world_at(30);
    let settings = settings();
    let layout = layout(&settings);
    let hall = living::court_hall(&settings, &layout).expect("the Court");
    let gallery = living::matriarch_gallery(&settings, &layout).expect("the gallery");
    // The Court's counter, inside its hall.
    put_rig(
        &mut world,
        TilePosition {
            x: hall.center().x,
            y: hall.max.y - 2,
        },
    );
    let _ = ticks(&mut world, 2);
    assert!(world.at_court(PLAYER));
    assert_eq!(
        act(
            &mut world,
            LivingDarkAction::Court(CourtService::TakeErrand)
        ),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        world.living_dark().hive.errand,
        Some(Errand::SoldiersAtTheGate)
    );
    // Cull eight of the Swarm.
    let frame = world.frame();
    for _ in 0..8 {
        let (x, y) = frame.tile_center(TilePosition {
            x: hall.center().x,
            y: hall.center().y,
        });
        let id = world.spawn_enemy(EnemyKind::HiveSoldier, x, y);
        kill(&mut world, id);
    }
    assert!(world.living_dark().hive.errand_ready());
    let credits = world.player(PLAYER).expect("rig").credits;
    assert_eq!(
        act(&mut world, LivingDarkAction::Court(CourtService::TurnIn)),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(
        world.living_dark().hive.standing,
        Errand::SoldiersAtTheGate.reward().0
    );
    assert!(world.player(PLAYER).expect("rig").credits > credits);
    // The stolen brood: jelly cells broken in the matriarch's gallery.
    assert_eq!(
        act(
            &mut world,
            LivingDarkAction::Court(CourtService::TakeErrand)
        ),
        PlayerScopedCommandOutcome::Applied
    );
    let cells: Vec<TilePosition> = gallery.positions().take(6).collect();
    for cell in &cells {
        let _ = world
            .terrain_mut()
            .set_kind(*cell, TileKind::Cave(CaveTile::JellyCell));
    }
    world.dig_living_tiles(TileRect {
        min: cells[0],
        max: cells[5],
    });
    assert!(
        world.living_dark().hive.errand_ready(),
        "{:?}",
        world.living_dark().hive
    );
    let _ = act(&mut world, LivingDarkAction::Court(CourtService::TurnIn));
    // Stealing from the Court sours it, and past a point it turns hostile.
    let standing = world.living_dark().hive.standing;
    let comb = TileRect {
        min: TilePosition {
            x: hall.min.x + 2,
            y: hall.min.y,
        },
        max: TilePosition {
            x: hall.max.x - 2,
            y: hall.min.y + 1,
        },
    };
    world.dig_living_tiles(comb);
    assert!(world.living_dark().hive.standing < standing);
    assert!(world.living_dark().hive.court_hostile());
    assert_eq!(
        act(
            &mut world,
            LivingDarkAction::Court(CourtService::RoyalJelly)
        ),
        PlayerScopedCommandOutcome::IgnoredUnavailable,
        "a hostile Court sells nothing"
    );
    assert_eq!(
        act(
            &mut world,
            LivingDarkAction::Court(CourtService::MakeAmends)
        ),
        PlayerScopedCommandOutcome::Applied
    );
    assert!(!world.living_dark().hive.court_hostile());
    // The last errand ends in a choice: here, the truce.
    assert_eq!(
        act(
            &mut world,
            LivingDarkAction::Court(CourtService::TakeErrand)
        ),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.living_dark().hive.errand, Some(Errand::TheMatriarch));
    assert_eq!(
        act(
            &mut world,
            LivingDarkAction::Court(CourtService::BrokerTruce)
        ),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.living_dark().hive.swarm, SwarmState::Truce);
    assert!(
        !world
            .living_dark()
            .hive
            .hostile_kind(EnemyKind::HiveSoldier)
    );
    // A replica learns the Hive's state from the shared section, and the hash covers it.
    let mut replica = world.clone();
    replica.living_dark = crate::living_dark::LivingDark::default();
    assert_ne!(
        replica.simulation_state_hash(),
        world.simulation_state_hash()
    );
    let section = world
        .section(crate::SectionKey::LivingDark)
        .expect("section");
    replica.apply_section(crate::SectionKey::LivingDark, section);
    assert_eq!(replica.living_dark(), world.living_dark());
    assert_eq!(
        replica.simulation_state_hash(),
        world.simulation_state_hash()
    );
}

#[test]
fn auto_aim_spares_the_courts_drones_while_the_court_is_friendly() {
    let mut world = world_at(30);
    let (x, y) = (60, middle_row(Stratum::TheHive));
    arena(&mut world, x, y);
    let rig = world.player(PLAYER).expect("rig");
    let (rx, ry) = (rig.x, rig.y);
    let drone = world.spawn_enemy(EnemyKind::HiveDrone, TILE_SIZE.mul_add(3.0, rx), ry - 8.0);
    let _fish = world.spawn_enemy(EnemyKind::LureFish, TILE_SIZE.mul_add(2.0, rx), ry - 8.0);
    assert_eq!(
        world.auto_aim_target(PLAYER, (1.0, 0.0)),
        None,
        "a friendly Court's drone and a lure fish are never locked onto"
    );
    // A Swarm soldier is.
    let soldier = world.spawn_enemy(EnemyKind::HiveSoldier, TILE_SIZE.mul_add(4.0, rx), ry - 8.0);
    assert_eq!(
        world.auto_aim_target(PLAYER, (1.0, 0.0)).map(|(id, ..)| id),
        Some(soldier)
    );
    // A hostile Court's drones are fair game.
    world.living_dark.hive.standing = -60;
    assert_eq!(
        world.auto_aim_target(PLAYER, (1.0, 0.0)).map(|(id, ..)| id),
        Some(drone)
    );
}

#[test]
fn the_matriarch_falls_and_the_swarm_breaks() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let settings = settings();
    let layout = layout(&settings);
    let gallery = living::matriarch_gallery(&settings, &layout).expect("the gallery");
    put_rig(
        &mut world,
        TilePosition {
            x: gallery.min.x + 4,
            y: gallery.max.y - 2,
        },
    );
    let _ = ticks(&mut world, 3);
    let matriarch = world
        .enemies()
        .find(|(_, state)| state.kind == EnemyKind::SwarmMatriarch)
        .map(|(entity, _)| entity.id)
        .expect("she waits in her gallery");
    kill(&mut world, matriarch);
    assert_eq!(world.living_dark().hive.swarm, SwarmState::Broken);
    assert!(!world.living_spawn_allowed(EnemyKind::HiveSoldier));
    let _ = ticks(&mut world, 3);
    assert!(
        world
            .enemies()
            .all(|(_, state)| state.kind != EnemyKind::SwarmMatriarch),
        "she does not come back"
    );
}

// --- The Flesh Caverns' tide and heartbeat ----------------------------------------------------

#[test]
fn the_acid_tide_rises_and_ebbs_in_a_gullet_at_any_tick_rate() {
    assert_eq!(tide_at(0).0, TidePhase::Rising);
    assert_eq!(tide_at(40_000), (TidePhase::High, 1_000));
    assert_eq!(tide_at(85_000), (TidePhase::Low, 0));
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.descent_rows(Descent::LivingDark);
    let area = TileRect {
        min: TilePosition { x: -600, y: top },
        max: TilePosition { x: 600, y: bottom },
    };
    let gullet = living::stamps_with(&settings, &layout, &area)
        .into_iter()
        .find(|stamp| stamp.kind == LivingStampKind::Gullet)
        .expect("a gullet");
    let acid = |world: &WorldState| {
        gullet
            .bounds
            .positions()
            .filter(|tile| {
                world
                    .terrain()
                    .fluid_at(*tile)
                    .is_some_and(|cell| cell.kind == FluidKind::Acid)
            })
            .count()
    };
    let mut highs = Vec::new();
    for hz in [20, 60] {
        let mut world = world_at(hz);
        world.enable_test_mode();
        put_rig(
            &mut world,
            TilePosition {
                x: gullet.bounds.center().x,
                y: gullet.bounds.min.y - 4,
            },
        );
        // Into the high water (40 s), then out to low water (88 s).
        let _ = ticks(&mut world, u64::from(hz) * 40);
        let high = acid(&world);
        assert!(high > 10, "high water at {hz} Hz: {high}");
        let _ = ticks(&mut world, u64::from(hz) * 48);
        let low = acid(&world);
        assert!(low < high / 3, "low water at {hz} Hz: {low} after {high}");
        highs.push(high);
    }
    let spread = highs[0].abs_diff(highs[1]);
    assert!(
        spread * 5 <= highs[0].max(highs[1]),
        "tick-rate safe: {highs:?}"
    );
}

#[test]
fn the_heartbeat_quickens_deeper_and_with_the_tide() {
    use crate::living_dark::heartbeat_bpm;
    assert!(heartbeat_bpm(900, TidePhase::Low) > heartbeat_bpm(100, TidePhase::Low));
    assert!(heartbeat_bpm(500, TidePhase::Rising) > heartbeat_bpm(500, TidePhase::Low));
    let mut world = world_at(30);
    put_rig(
        &mut world,
        TilePosition {
            x: 60,
            y: middle_row(Stratum::FleshCaverns),
        },
    );
    assert!(world.heartbeat_for(PLAYER).is_some());
    put_rig(
        &mut world,
        TilePosition {
            x: 60,
            y: middle_row(Stratum::GlowSea),
        },
    );
    assert!(world.heartbeat_for(PLAYER).is_none());
}

// --- The Glow Sea -----------------------------------------------------------------------------

#[test]
fn in_the_glow_sea_only_creatures_light_the_dark_and_lamp_ii_widens_the_rigs_lamp() {
    let mut world = world_at(30);
    let row = middle_row(Stratum::GlowSea);
    arena(&mut world, 60, row);
    let frame = world.frame();
    let far = frame.tile_center(TilePosition { x: 75, y: row });
    let dark = world.glow_light_at(far.0, far.1).expect("in the Glow Sea");
    assert!(dark < 0.1, "the dark: {dark}");
    let jelly = frame.tile_center(TilePosition { x: 75, y: row - 1 });
    let _ = world.spawn_enemy(EnemyKind::GlowJelly, jelly.0, jelly.1);
    let lit = world.glow_light_at(far.0, far.1).expect("in the Glow Sea");
    assert!(lit > dark + 0.3, "a jelly lights it: {lit}");
    // The rig lamp is a candle until Lamp II.
    assert!(world.lamp_scale_for(PLAYER) < 0.5);
    let near = frame.tile_center(TilePosition { x: 63, y: row + 2 });
    let candle = world.glow_light_at(near.0, near.1).expect("lit");
    crate::economy::grant_rig_system(world.player_mut(PLAYER).expect("rig"), RigSystem::LampII);
    assert!(world.lamp_scale_for(PLAYER) > 1.0);
    let lamp = world.glow_light_at(near.0, near.1).expect("lit");
    assert!(lamp > candle, "Lamp II reaches farther: {candle} -> {lamp}");
    // Elsewhere there is no Glow Sea light to speak of.
    put_rig(
        &mut world,
        TilePosition {
            x: 60,
            y: middle_row(Stratum::TheHive),
        },
    );
    let (x, y) = {
        let rig = world.player(PLAYER).expect("rig");
        (rig.x, rig.y)
    };
    assert!(world.glow_light_at(x, y).is_none());
}

// --- The Mycelial Mind ------------------------------------------------------------------------

/// The bot's controls: aim the railgun at `target`, strafe to dodge the Mind's orbs.
fn bot_controls(world: &WorldState, target: (f32, f32), tick: u64) -> ControlState {
    let rig = world.player(PLAYER).expect("rig");
    let (dx, dy) = (target.0 - rig.x, target.1 - rig.y);
    let length = dx.hypot(dy).max(1.0);
    #[allow(clippy::cast_possible_truncation, reason = "a unit vector times 127")]
    let aim = ((dx / length * 127.0) as i8, (dy / length * 127.0) as i8);
    // Close in to within railgun reach, then weave to dodge the Mind's orbs.
    let strafe = if dx.abs() > TILE_SIZE * 9.0 {
        dx.signum()
    } else if (tick / 45).is_multiple_of(2) {
        0.6
    } else {
        -0.6
    };
    // The railgun wants a fresh pull for every slug.
    // Hop now and then; climb toward a target high above.
    let thrust = dy < -TILE_SIZE * 4.0 || tick % 40 < 6;
    ControlState::new(strafe, thrust, false).with_weapon(WeaponControl {
        fire: tick % 6 < 3,
        auto_aim: false,
        aim_x: aim.0,
        aim_y: aim.1,
    })
}

/// A rig's hull tier at the Mycelial Mind (the descent probe's arrival tier in the Glow Sea,
/// plus the one it can afford there).
const HULL_AT_THE_MIND: u8 = 10;

#[test]
#[allow(clippy::too_many_lines, reason = "the whole fight, start to end")]
fn a_bot_beats_the_mycelial_mind_and_the_crew_gets_the_bio_filter_and_lamp_ii() {
    let hz = 30;
    let mut world = world_at(hz);
    // A hostile world: Peaceful has no creatures (and opens the gate without a fight).
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let settings = settings();
    let layout = layout(&settings);
    let hollow = living::mind_hollow(&settings, &layout).expect("the hollow");
    let start = TilePosition {
        x: hollow.center().x - 10,
        y: hollow.max.y - 6,
    };
    put_rig(&mut world, start);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        crate::economy::max_out_upgrades(rig);
        rig.hazard_gear.rig_systems &= !(RigSystem::BioFilter.bit() | RigSystem::LampII.bit());
        rig.weapon_rack.battery_level = crate::weapon::MAX_BATTERY_LEVEL;
        let _ = rig.receive_weapon(WeaponKind::Railgun);
        let _ = rig.mount_weapon(Some(WeaponKind::Railgun));
        // The hull a rig reaches the Glow Sea with (the descent probe's arrival tier), not
        // the test rig's top tier: the fight must be fair for the rig that gets there.
        rig.hull_level = HULL_AT_THE_MIND;
        rig.hull = rig.max_hull();
    }
    let events = ticks(&mut world, 2);
    assert!(living_events(&events).contains(&LivingDarkEvent::MindAwakened));
    assert!(world.living_dark().mind.awake());
    assert_eq!(
        world.living_dark().mind.nodes.len(),
        crate::living_dark::MIND_NODES
    );
    // Shielded: a hit on the Mind does nothing while a node lives.
    let mind = EntityId(world.living_dark().mind.entity.expect("the Mind"));
    let mut output = crate::TickOutput::default();
    assert_eq!(
        world.damage_enemy(
            mind,
            Damage::new(500.0, DamageKind::Kinetic, DamageSource::Player(PLAYER)),
            &mut output
        ),
        crate::enemy::DamageOutcome::Ignored
    );
    let mut fired = 0_usize;
    let mut possessed = false;
    let mut exposed = false;
    let mut defeated = false;
    let mut fight_ticks = 0_u64;
    for count in 0..u64::from(hz) * 900 {
        fight_ticks = count + 1;
        // The nearest node, then the Mind once they are down.
        let (rx, ry) = {
            let rig = world.player(PLAYER).expect("rig");
            (rig.x, rig.y)
        };
        let target = world
            .living_dark()
            .mind
            .nodes
            .iter()
            .filter_map(|id| world.entities().get(EntityId(*id)))
            .map(|entity| (entity.body.x, entity.body.y))
            .min_by(|a, b| {
                (a.0 - rx)
                    .hypot(a.1 - ry)
                    .total_cmp(&(b.0 - rx).hypot(b.1 - ry))
            })
            .or_else(|| {
                world
                    .living_dark()
                    .mind
                    .entity
                    .and_then(|id| world.entities().get(EntityId(id)))
                    .map(|entity| (entity.body.x, entity.body.y))
            });
        let Some(target) = target else {
            break;
        };
        let controls = bot_controls(&world, target, count);
        let output =
            world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
        let events: Vec<GameplayEventKind> =
            output.events.into_iter().map(|event| event.kind).collect();
        fired += events
            .iter()
            .filter(|event| matches!(event, GameplayEventKind::WeaponFired { .. }))
            .count();
        let living = living_events(&events);
        possessed |= living.contains(&LivingDarkEvent::MindPossessed)
            || world
                .enemies()
                .any(|(_, state)| state.phase == POSSESSED_PHASE);
        exposed |= living.contains(&LivingDarkEvent::MindExposed);
        if living.contains(&LivingDarkEvent::MindDefeated) {
            defeated = true;
            break;
        }
        assert!(
            world.player(PLAYER).expect("rig").hull > 0.0,
            "the bot survived"
        );
    }
    assert!(fired > 10, "the bot fired its railgun ({fired})");
    let left =
        world.player(PLAYER).expect("rig").hull / world.player(PLAYER).expect("rig").max_hull();
    assert!(
        left > 0.2,
        "with a margin: {:.0} % of the hull left",
        left * 100.0
    );
    // The probe models the fight from this measured win (Wave 10 polish).
    #[allow(clippy::cast_precision_loss, reason = "a few thousand ticks")]
    crate::balance::descent::mechanics::record_bot_fight(
        crate::descent::DescentBoss::MycelialMind,
        fight_ticks as f64 / f64::from(hz),
        f64::from(left),
    );
    assert!(possessed, "the Mind possessed a creature");
    assert!(exposed, "its nodes fell and it was exposed");
    assert!(
        defeated,
        "the bot beat the Mind (left at {:?}, nodes {})",
        world
            .living_dark()
            .mind
            .entity
            .and_then(|id| world.entities().get(EntityId(id)))
            .and_then(|entity| entity.state.health())
            .map(|health| health.current),
        world.living_dark().mind.nodes.len()
    );
    assert!(world.living_dark().mind.beaten);
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.hazard_gear.has_system(RigSystem::BioFilter));
    assert!(rig.hazard_gear.has_system(RigSystem::LampII));
    // The Bio-Filter keeps acid out now, and Descent IV's gate opens.
    assert!((world.submersion_gear(PLAYER).acid_resistance - 1.0).abs() < f32::EPSILON);
    assert!(crate::depth_progression::missing_gate(rig.hazard_gear.rig_systems, 3).is_none());
    // It stays beaten.
    let _ = ticks(&mut world, 30);
    assert!(!world.living_dark().mind.awake());
}

#[test]
fn on_a_peaceful_world_the_mind_yields_its_gate_without_a_fight() {
    let mut world = world_at(30);
    let settings = settings();
    let layout = layout(&settings);
    let hollow = living::mind_hollow(&settings, &layout).expect("the hollow");
    put_rig(
        &mut world,
        TilePosition {
            x: hollow.center().x,
            y: hollow.max.y - 6,
        },
    );
    let events = ticks(&mut world, 3);
    assert!(living_events(&events).contains(&LivingDarkEvent::MindDefeated));
    assert!(world.living_dark().mind.beaten);
    assert!(
        world
            .player(PLAYER)
            .expect("rig")
            .hazard_gear
            .has_system(RigSystem::BioFilter)
    );
}

#[test]
fn the_living_dark_is_deterministic_and_leaves_older_worlds_alone() {
    let run = || {
        let mut world = world_at(30);
        let row = middle_row(Stratum::RootLabyrinth);
        arena(&mut world, 60, row);
        world.dig_living_tiles(TileRect::around(TilePosition { x: 50, y: row }, 3, 0));
        world.release_spore_cloud(TilePosition { x: 60, y: row });
        let _ = ticks(&mut world, 30 * 200);
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
    // A V10 world never touches it.
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(SEED),
        SimulationParams::new(30),
    );
    let _ = ticks(&mut world, 30);
    assert!(world.living_dark().is_empty());
    assert!(world.descent_strata_layout().is_none());
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the arena, the cast and the fight in one place"
)]
fn the_living_darks_creatures_keep_the_body_rules() {
    use crate::{enemy::Locomotion, physics};
    let hz = 30;
    let mut world = world_at(hz);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let row = middle_row(Stratum::FungalForest);
    let (left, right, top, floor) = (20, 100, row - 10, row + 8);
    put_rig(
        &mut world,
        TilePosition {
            x: 60,
            y: floor - 1,
        },
    );
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
    // Pillars and a flesh bank for the borers.
    fill(
        &mut world,
        TileRect {
            min: TilePosition {
                x: 40,
                y: floor - 6,
            },
            max: TilePosition {
                x: 41,
                y: floor - 1,
            },
        },
        TileKind::Stone,
    );
    fill(
        &mut world,
        TileRect {
            min: TilePosition {
                x: left,
                y: floor - 8,
            },
            max: TilePosition {
                x: left + 4,
                y: floor - 1,
            },
        },
        TileKind::Cave(CaveTile::Flesh),
    );
    put_rig(
        &mut world,
        TilePosition {
            x: 60,
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
        .filter(|kind| kind.is_living_dark())
        .collect();
    for (index, kind) in (0_i32..).zip(kinds) {
        let column = left + 8 + (index * 4) % (right - left - 12);
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
            // Rock wins over the push (the body rules): a creature pinned between the rig and
            // rock on both of its escape sides may be left touching deeper for a tick.
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
                "{:?} #{} sits {depth} px inside the rig at tick {count}: {:?} {:?} rig ({}, {})",
                state.kind,
                entity.id.get(),
                (entity.body.x, entity.body.y),
                state.mode,
                rig.x,
                rig.y
            );
            checked += 1;
        }
    }
    assert!(checked > 1_000, "a real fight: {checked}");
}
