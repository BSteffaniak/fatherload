//! WP-TOMBS T28, world side: side-view traps hurt on the world clock (the same at every tick
//! rate, identical on every peer), leave their safe windows and strips, crumbling floors fall
//! and set again, sand rises in its basin, tomb creatures never sit in rock or on a pilot,
//! both guardians fall to the scripted bot through all their phases, a side-view run clears on
//! every tier and a party of two clears one together.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    reason = "test fixtures convert small tile, tick and pixel numbers freely"
)]

use crate::{
    GameplayEventKind, PlayerId, SimulationParams, TickInputs, WorldState,
    balance::tombs::{TombBot, find_site},
    base::BuiltTile,
    enemy::{EnemyDifficulty, EnemyKind},
    entity::EntityState,
    physics,
    pilot::PILOT_HALF_WIDTH,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    tomb::{
        RoomKind, RunOutcome, SiteStyle, TombSite,
        floor::plan_floor,
        realize::{RealizedFloor, TrapKind, realizer_for},
        side_view::FLOOR_ROW,
        traps::{CRUMBLE_MILLIS, REFORM_MILLIS, SAND_PERIOD_MILLIS, sand_state},
    },
    weapon::AmmoKind,
    worldgen::{GeneratorVersion, WorldSettings, features::TileRect, tomb_sites},
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);

fn settings(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V10,
        ..WorldSettings::new(seed)
    }
}

/// A side-view site of `tier` on some seed (searching a few), with its settings.
fn side_site(tier: u8) -> (WorldSettings, TombSite) {
    [7_u64, 11, 4_242, 90_001, 31, 1_234]
        .into_iter()
        .find_map(|seed| {
            let settings = settings(seed);
            find_site(&settings, SiteStyle::SideView, tier).map(|site| (settings, site))
        })
        .expect("a side-view site of the tier")
}

/// A fresh world at `hz` on `settings` with `difficulty`, the player kitted out.
fn world_on(settings: &WorldSettings, hz: u32, difficulty: EnemyDifficulty) -> WorldState {
    let mut world = WorldState::new_game_with_settings(PLAYER, settings, SimulationParams::new(hz));
    world.set_enemy_difficulty(difficulty);
    kit(&mut world, PLAYER, 2);
    world
}

fn kit(world: &mut WorldState, player: PlayerId, tier: u8) {
    if world.player_mut(player).is_none() {
        world.add_player(player);
    }
    if let Some(rig) = world.player_mut(player) {
        rig.pilot_gear.suit = tier;
        rig.pilot_gear.armor = tier;
        rig.pilot_gear.jetpack = 1;
        let _ = rig.pilot_arsenal.receive(crate::WeaponKind::Rifle);
        let _ = rig.pilot_arsenal.receive(crate::WeaponKind::Sidearm);
        rig.pilot_arsenal.equipped = Some(crate::WeaponKind::Rifle);
        let _ = rig
            .pilot_arsenal
            .add_ammo(AmmoKind::Cartridge, crate::pilot::arsenal::MAX_PILOT_AMMO);
    }
}

/// The first run counter whose first floor satisfies `wanted`.
fn run_with(site: &TombSite, wanted: impl Fn(&RealizedFloor) -> bool) -> u32 {
    (0..400)
        .find(|run| wanted(&realizer_for(SiteStyle::SideView).realize(&plan_floor(site, *run, 0))))
        .expect("a floor with what the test needs")
}

fn has_trap(floor: &RealizedFloor, kind: TrapKind) -> bool {
    floor.cells.iter().any(|(_, cell)| {
        matches!(cell, TileKind::Built(BuiltTile::TombTrap { kind: found, .. }) if *found == kind.index())
    })
}

/// Removes every creature (trap tests: traps hurt at the Normal difficulty's damage, creatures
/// would muddle the count).
fn no_creatures(world: &mut WorldState) {
    let ids: Vec<crate::entity::EntityId> = world.enemies().map(|(entity, _)| entity.id).collect();
    for id in ids {
        world.entities.despawn(id);
    }
}

/// Enters run `run` of `site` with `party`.
fn enter(world: &mut WorldState, site: &TombSite, run: u32, party: &[PlayerId]) {
    for player in party {
        kit(world, *player, 2);
        let _ = world.place_pilot(*player, 0.0, 0.0);
    }
    world.scenario_next_tomb_run(site.id, run);
    world
        .scenario_enter_tomb(site, party)
        .expect("the run starts");
    if world.enemy_difficulty() == EnemyDifficulty::Normal && party.len() == 1 && run != 0 {
        no_creatures(world);
    }
}

/// Absolute tiles of every trap of `kind` on the run's floor.
fn traps(world: &WorldState, kind: TrapKind) -> Vec<TilePosition> {
    let run = world.tomb_run_of(PLAYER).expect("a run");
    let origin = tomb_sites::slot_origin(run.slot);
    WorldState::tomb_floor_of(run)
        .cells
        .iter()
        .filter(|(_, cell)| matches!(cell, TileKind::Built(BuiltTile::TombTrap { kind: found, .. }) if *found == kind.index()))
        .map(|(tile, _)| TilePosition {
            x: tile.x + origin.x,
            y: tile.y + origin.y,
        })
        .collect()
}

/// The absolute floor row (the open row the party stands in).
fn standing_row(world: &WorldState) -> i32 {
    let run = world.tomb_run_of(PLAYER).expect("a run");
    tomb_sites::slot_origin(run.slot).y + FLOOR_ROW - 1
}

/// Holds `player`'s pilot still with its feet on tile `tile`.
fn stand(world: &mut WorldState, player: PlayerId, tile: TilePosition) {
    let x = (tile.x as f32 + 0.5) * TILE_SIZE;
    let feet = (tile.y + 1) as f32 * TILE_SIZE - 0.25;
    let (id, _, _) = world.pilot_of(player).expect("a pilot");
    world.edit_pilot(id, |body, _| {
        body.x = x;
        body.y = feet;
        body.previous_x = x;
        body.previous_y = feet;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
    });
}

fn health(world: &WorldState, player: PlayerId) -> f32 {
    world
        .pilot_of(player)
        .map_or(0.0, |(_, _, state)| state.health)
}

/// Runs `seconds` of wall time with no input; returns the events.
fn idle(world: &mut WorldState, seconds: f32) -> Vec<GameplayEventKind> {
    let ticks = (seconds * world.tick_rate_hz() as f32).round() as u32;
    let mut events = Vec::new();
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        events.extend(
            world
                .simulate_tick(tick, &TickInputs::default())
                .events
                .into_iter()
                .map(|event| event.kind),
        );
    }
    events
}

/// A pilot parked under a blade's swing for a few seconds: hurt at every tick rate by about the
/// same amount, and identically on two peers.
fn under_a_blade(hz: u32) -> (f32, crate::SimulationStateHash) {
    let (settings, site) = side_site(1);
    let mut world = world_on(&settings, hz, EnemyDifficulty::Normal);
    let run = run_with(&site, |floor| has_trap(floor, TrapKind::Blade));
    enter(&mut world, &site, run, &[PLAYER]);
    let pivot = traps(&world, TrapKind::Blade)[0];
    let row = standing_row(&world);
    stand(&mut world, PLAYER, TilePosition { x: pivot.x, y: row });
    let start = health(&world, PLAYER);
    let mut events = Vec::new();
    for _ in 0..hz * 5 {
        // Keep the pilot pinned under the pivot (the blade shoves nothing; the test holds it).
        stand(&mut world, PLAYER, TilePosition { x: pivot.x, y: row });
        let tick = world.simulation_tick();
        events.extend(world.simulate_tick(tick, &TickInputs::default()).events);
    }
    let _ = events;
    (
        start - health(&world, PLAYER),
        world.simulation_state_hash(),
    )
}

#[test]
fn blades_hurt_on_the_world_clock_at_every_tick_rate_and_on_every_peer() {
    let (at_30, _) = under_a_blade(30);
    let (at_60, hash_a) = under_a_blade(60);
    let (again, hash_b) = under_a_blade(60);
    let (at_120, _) = under_a_blade(120);
    assert!(at_60 > 10.0, "the blade hurts: {at_60}");
    assert_eq!(at_60, again);
    assert_eq!(hash_a, hash_b, "two peers agree");
    for other in [at_30, at_120] {
        assert!(
            (other - at_60).abs() <= at_60 * 0.25,
            "tick-rate safe: {at_30} / {at_60} / {at_120}"
        );
    }
}

#[test]
fn the_strip_between_two_blades_is_safe_all_cycle_long() {
    let (settings, site) = side_site(1);
    let mut world = world_on(&settings, 60, EnemyDifficulty::Normal);
    let run = run_with(&site, |floor| has_trap(floor, TrapKind::Blade));
    enter(&mut world, &site, run, &[PLAYER]);
    let blades = traps(&world, TrapKind::Blade);
    let middle = TilePosition {
        x: i32::midpoint(blades[0].x, blades[1].x),
        y: standing_row(&world),
    };
    stand(&mut world, PLAYER, middle);
    let start = health(&world, PLAYER);
    idle(&mut world, 6.0);
    assert!((health(&world, PLAYER) - start).abs() < f32::EPSILON);
}

#[test]
fn a_crusher_slams_after_its_tell_hurts_and_shoves_a_pilot_out_from_under_it() {
    let (settings, site) = side_site(1);
    let mut world = world_on(&settings, 60, EnemyDifficulty::Normal);
    let run = run_with(&site, |floor| has_trap(floor, TrapKind::Crusher));
    enter(&mut world, &site, run, &[PLAYER]);
    let housing = traps(&world, TrapKind::Crusher)[0];
    let row = standing_row(&world);
    stand(
        &mut world,
        PLAYER,
        TilePosition {
            x: housing.x,
            y: row,
        },
    );
    let start = health(&world, PLAYER);
    let events = idle(&mut world, 4.0);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::TombTrapSprung {
            kind: TrapKind::Crusher,
            ..
        }
    )));
    assert!(health(&world, PLAYER) < start, "the slam hurts");
    let (_, body, _) = world.pilot_of(PLAYER).expect("a pilot");
    let column = housing.x as f32 * TILE_SIZE;
    assert!(
        body.x + PILOT_HALF_WIDTH <= column + 2.0
            || body.x - PILOT_HALF_WIDTH >= column + TILE_SIZE - 2.0,
        "shoved out from under the block: {} vs column {column}",
        body.x
    );
}

#[test]
fn crumbling_slabs_fall_a_moment_after_a_step_and_set_again() {
    let (settings, site) = side_site(1);
    let mut world = world_on(&settings, 60, EnemyDifficulty::Normal);
    let run = run_with(&site, |floor| has_trap(floor, TrapKind::Crumble));
    enter(&mut world, &site, run, &[PLAYER]);
    let slab = traps(&world, TrapKind::Crumble)[0];
    stand(
        &mut world,
        PLAYER,
        TilePosition {
            x: slab.x,
            y: slab.y - 1,
        },
    );
    let events = idle(&mut world, (CRUMBLE_MILLIS as f32 + 200.0) / 1_000.0);
    assert_eq!(
        world.terrain().tile(slab).map(|found| found.kind),
        Some(TileKind::Air),
        "the slab fell"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::TombTrapSprung {
            kind: TrapKind::Crumble,
            ..
        }
    )));
    // Out of the hole, then the slab sets again.
    let row = standing_row(&world);
    stand(
        &mut world,
        PLAYER,
        TilePosition {
            x: slab.x - 4,
            y: row,
        },
    );
    idle(&mut world, REFORM_MILLIS as f32 / 1_000.0 + 0.5);
    assert!(matches!(
        world.terrain().tile(slab).map(|found| found.kind),
        Some(TileKind::Built(BuiltTile::TombTrap { kind, .. })) if kind == TrapKind::Crumble.index()
    ));
    assert!(
        world
            .tomb_run_of(PLAYER)
            .expect("a run")
            .crumbling
            .is_empty()
    );
}

#[test]
fn sand_fills_its_basin_on_a_cycle_and_hurts_only_those_under_it() {
    let (settings, site) = side_site(1);
    let mut world = world_on(&settings, 60, EnemyDifficulty::Normal);
    let run = run_with(&site, |floor| has_trap(floor, TrapKind::SandSluice));
    enter(&mut world, &site, run, &[PLAYER]);
    let grate = traps(&world, TrapKind::SandSluice)[0];
    let room: TileRect = world
        .tomb_run_of(PLAYER)
        .and_then(|run| {
            run.rooms
                .iter()
                .find(|room| {
                    room.interior.contains(TilePosition {
                        x: grate.x,
                        y: grate.y - 1,
                    })
                })
                .map(|room| room.interior)
        })
        .expect("the basin's room");
    // In the basin over a whole cycle: hurt only while the sand is up.
    stand(
        &mut world,
        PLAYER,
        TilePosition {
            x: grate.x,
            y: grate.y - 1,
        },
    );
    let mut hurt_dry = false;
    let mut hurt_wet = false;
    for _ in 0..(SAND_PERIOD_MILLIS * 60 / 1_000) {
        stand(
            &mut world,
            PLAYER,
            TilePosition {
                x: grate.x,
                y: grate.y - 1,
            },
        );
        // Topped up every tick so a whole cycle in the sand never downs the pilot.
        let (id, _, _) = world.pilot_of(PLAYER).expect("a pilot");
        world.edit_pilot(id, |_, state| state.health = state.max_health);
        let before = health(&world, PLAYER);
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &TickInputs::default());
        let millis = crate::tomb::traps::world_millis(world.simulation_tick().get(), 60);
        let hurt = health(&world, PLAYER) < before - 0.01;
        if sand_state(room, millis).1 <= 0.0 {
            hurt_dry |= hurt;
        } else {
            hurt_wet |= hurt;
        }
    }
    assert!(hurt_wet, "the sand hurts in the basin");
    assert!(!hurt_dry, "never while the basin is dry");
    // On the edge (the doorway line) a whole cycle does nothing.
    let edge = TilePosition {
        x: room.min.x,
        y: standing_row(&world),
    };
    let held = health(&world, PLAYER);
    for _ in 0..(SAND_PERIOD_MILLIS * 60 / 1_000) {
        stand(&mut world, PLAYER, edge);
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &TickInputs::default());
    }
    assert!(health(&world, PLAYER) >= held - 0.01);
}

#[test]
fn tomb_creatures_never_end_a_tick_in_rock_or_on_the_pilot() {
    let (settings, site) = side_site(1);
    let mut world = world_on(&settings, 60, EnemyDifficulty::Normal);
    let run = run_with(&site, |floor| {
        floor.rooms.iter().any(|room| room.kind == RoomKind::Ambush)
    });
    enter(&mut world, &site, run, &[PLAYER]);
    let room = world
        .tomb_run_of(PLAYER)
        .and_then(|run| run.rooms.iter().find(|room| room.kind == RoomKind::Ambush))
        .map(|room| room.interior)
        .expect("an ambush room");
    let row = standing_row(&world);
    stand(
        &mut world,
        PLAYER,
        TilePosition {
            x: room.min.x + 2,
            y: row,
        },
    );
    for (index, kind) in [
        EnemyKind::Mummy,
        EnemyKind::CryptBat,
        EnemyKind::SkeletalArcher,
        EnemyKind::AnimatedStatue,
        EnemyKind::SunColossus,
        EnemyKind::HollowMatriarch,
    ]
    .into_iter()
    .enumerate()
    {
        let x = (room.min.x + 3 + index as i32 * 2) as f32 * TILE_SIZE;
        let _ = world.spawn_enemy(kind, x, (row - 2) as f32 * TILE_SIZE);
    }
    for tick in 0..60 * 15 {
        let now = world.simulation_tick();
        let _ = world.simulate_tick(now, &TickInputs::default());
        let pilot = world.pilot_of(PLAYER).map(|(_, body, state)| {
            let (mx, my) = state.middle(body);
            (mx, my, state.height() * 0.5)
        });
        for (entity, state) in world.enemies() {
            let body = entity.body;
            let spec = state.kind.stats();
            assert!(
                !physics::collides(world.terrain(), body.x, body.y, spec.radius),
                "{:?} in rock at tick {tick}",
                state.kind
            );
            if let Some((mx, my, half_height)) = pilot {
                let (half_x, half_y) = state.kind.contact_half();
                let overlap_x = (half_x + PILOT_HALF_WIDTH) - (body.x - mx).abs();
                let overlap_y = (half_y + half_height) - (body.y - my).abs();
                assert!(
                    overlap_x <= 1.5 || overlap_y <= 1.5,
                    "{:?} on the pilot at tick {tick}: {overlap_x} x {overlap_y}",
                    state.kind
                );
            }
        }
    }
}

/// A scripted armed bot against `kind` on its crypt's floor; returns whether it won and the
/// phases the guardian went through.
fn guardian_fight(kind: EnemyKind) -> (bool, Vec<u8>) {
    let (settings, site) = side_site(1);
    let mut world = world_on(&settings, 60, EnemyDifficulty::Normal);
    enter(&mut world, &site, 0, &[PLAYER]);
    kit(&mut world, PLAYER, 3);
    world.scenario_tomb_floor(PLAYER, site.floors() - 1);
    let room = world
        .tomb_run_of(PLAYER)
        .and_then(|run| run.rooms.iter().find(|room| room.kind == RoomKind::Boss))
        .map(|room| room.interior)
        .expect("the guardian's crypt");
    stand(
        &mut world,
        PLAYER,
        TilePosition {
            x: room.min.x + 2,
            y: room.max.y,
        },
    );
    world
        .scenario_tomb_guardian(PLAYER, kind)
        .expect("the guardian wakes");
    let mut bot = TombBot::new(PLAYER, true);
    let mut phases = Vec::new();
    for _ in 0..60 * 60 * 4 {
        let Some(run) = world.tomb_run_of(PLAYER) else {
            break;
        };
        if run.boss_defeated {
            return (true, phases);
        }
        let inputs = bot.inputs(&world);
        let tick = world.simulation_tick();
        for event in world.simulate_tick(tick, &inputs).events {
            if let GameplayEventKind::TombGuardianPhase {
                phase, kind: seen, ..
            } = event.kind
            {
                assert_eq!(seen, kind);
                phases.push(phase);
            }
        }
    }
    (
        world
            .tomb_run_of(PLAYER)
            .is_some_and(|run| run.boss_defeated),
        phases,
    )
}

#[test]
fn the_sunwrought_colossus_falls_to_the_bot_through_its_phases() {
    let (won, phases) = guardian_fight(EnemyKind::SunColossus);
    assert!(won, "the bot fells the colossus");
    assert_eq!(phases, vec![2, 3], "it turns twice");
}

#[test]
fn the_hollow_matriarch_falls_to_the_bot_through_her_phases() {
    let (won, phases) = guardian_fight(EnemyKind::HollowMatriarch);
    assert!(won, "the bot fells the matriarch");
    assert_eq!(phases, vec![2, 3], "she turns twice");
}

/// Plays a whole side-view run of a tier-`tier` site with an armed bot per member.
fn clear_run(tier: u8, party: &[PlayerId], difficulty: EnemyDifficulty) -> Vec<RunOutcome> {
    let (settings, site) = side_site(tier);
    let mut world = world_on(&settings, 60, difficulty);
    enter(&mut world, &site, 0, party);
    for player in party {
        kit(&mut world, *player, 3);
    }
    let mut bots: Vec<TombBot> = party
        .iter()
        .map(|player| TombBot::new(*player, difficulty != EnemyDifficulty::Peaceful))
        .collect();
    let mut outcomes = Vec::new();
    for _ in 0..60 * 60 * 14 {
        if party
            .iter()
            .all(|player| world.tomb_run_of(*player).is_none())
        {
            break;
        }
        let mut inputs = TickInputs::default();
        for bot in &mut bots {
            let mine = bot.inputs(&world);
            inputs.players.extend(mine.players);
        }
        let tick = world.simulation_tick();
        for event in world.simulate_tick(tick, &inputs).events {
            if let GameplayEventKind::TombRunEnded { outcome, .. } = event.kind {
                outcomes.push(outcome);
            }
        }
    }
    outcomes
}

#[test]
fn a_side_view_run_clears_on_every_tier() {
    for tier in 1..=crate::tomb::MAX_TIER {
        let outcomes = clear_run(tier, &[PLAYER], EnemyDifficulty::Peaceful);
        assert_eq!(outcomes, vec![RunOutcome::Finished], "tier {tier}");
    }
}

#[test]
fn an_armed_bot_clears_a_first_tier_tomb_on_normal() {
    let outcomes = clear_run(1, &[PLAYER], EnemyDifficulty::Normal);
    assert_eq!(outcomes, vec![RunOutcome::Finished]);
}

#[test]
fn a_party_of_two_clears_a_side_view_tomb_together() {
    let outcomes = clear_run(1, &[PLAYER, OTHER], EnemyDifficulty::Normal);
    assert_eq!(outcomes.len(), 2, "{outcomes:?}");
    assert!(
        outcomes
            .iter()
            .all(|outcome| *outcome == RunOutcome::Finished),
        "{outcomes:?}"
    );
}

#[test]
fn the_guardian_room_wakes_a_side_view_guardian_with_phases_saved_in_the_run() {
    // The phase rides in the run (saved, replicated) so a reload does not replay the turn.
    let (settings, site) = side_site(1);
    let mut world = world_on(&settings, 60, EnemyDifficulty::Normal);
    enter(&mut world, &site, 0, &[PLAYER]);
    world.scenario_tomb_floor(PLAYER, site.floors() - 1);
    let boss = world
        .scenario_tomb_guardian(PLAYER, EnemyKind::SunColossus)
        .expect("the guardian wakes");
    world.scenario_hurt_enemy_to(boss, 0.5);
    let events = idle(&mut world, 0.2);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::TombGuardianPhase { phase: 2, .. }))
    );
    let run = world.tomb_run_of(PLAYER).expect("a run").clone();
    assert_eq!(run.boss_phase, 2);
    let json = serde_json::to_string(&run).expect("serializes");
    let back: crate::tomb::TombRun = serde_json::from_str(&json).expect("round-trips");
    assert_eq!(back, run);
    assert!(matches!(
        world.entities.get(boss).map(|entity| entity.state),
        Some(EntityState::Enemy(_))
    ));
}

/// Every tomb creature and both guardians fight a pilot who keeps moving (walking both ways,
/// jumping, shooting on auto-aim) in the guardian's crypt at 30, 60 and 120 Hz: no creature,
/// raised mummies and summoned bats included, ever ends a tick in rock or overlapping the pilot.
#[test]
fn tomb_creatures_keep_their_bodies_against_a_moving_pilot_at_every_tick_rate() {
    let (settings, site) = side_site(1);
    for hz in [30_u32, 60, 120] {
        let mut world = world_on(&settings, hz, EnemyDifficulty::Hard);
        enter(&mut world, &site, 0, &[PLAYER]);
        world.scenario_tomb_floor(PLAYER, site.floors() - 1);
        no_creatures(&mut world);
        let room = world
            .tomb_run_of(PLAYER)
            .and_then(|run| run.rooms.iter().find(|room| room.kind == RoomKind::Boss))
            .map(|room| room.interior)
            .expect("the guardian's crypt");
        stand(
            &mut world,
            PLAYER,
            TilePosition {
                x: room.min.x + 6,
                y: room.max.y,
            },
        );
        // The colossus in its second phase (it raises mummies), the matriarch in her third
        // (bats and dives).
        if let Some(colossus) = world.scenario_tomb_guardian(PLAYER, EnemyKind::SunColossus) {
            world.scenario_hurt_enemy_to(colossus, 0.55);
        }
        for (index, kind) in [
            EnemyKind::Mummy,
            EnemyKind::CryptBat,
            EnemyKind::SkeletalArcher,
            EnemyKind::AnimatedStatue,
            EnemyKind::HollowMatriarch,
        ]
        .into_iter()
        .enumerate()
        {
            let x = (room.min.x + 2 + index as i32 * 4) as f32 * TILE_SIZE;
            let id = world.spawn_enemy(kind, x, (room.max.y - 1) as f32 * TILE_SIZE);
            if kind == EnemyKind::HollowMatriarch {
                world.scenario_hurt_enemy_to(id, 0.28);
            }
        }
        let mut most = 0;
        let ticks = u64::from(hz) * 20;
        for tick in 0..ticks {
            let phase = (tick * 2 / u64::from(hz)) % 4;
            let axis = match phase {
                0 => 1.0,
                2 => -1.0,
                _ => 0.0,
            };
            let jump = phase == 1 && tick % u64::from(hz) < u64::from(hz) / 5;
            let controls = crate::ControlState::new(axis, false, false)
                .with_jump(jump)
                .with_weapon(crate::WeaponControl {
                    fire: true,
                    auto_aim: true,
                    aim_x: 0,
                    aim_y: 0,
                });
            let now = world.simulation_tick();
            let _ = world.simulate_tick(now, &TickInputs::held(PLAYER, controls));
            // Keep the pilot in the fight.
            if let Some((id, _, _)) = world.pilot_of(PLAYER) {
                world.edit_pilot(id, |_, state| state.health = state.max_health);
            }
            let pilot = world.pilot_of(PLAYER).map(|(_, body, state)| {
                let (mx, my) = state.middle(body);
                (mx, my, state.height() * 0.5)
            });
            most = most.max(world.enemies().count());
            for (entity, state) in world.enemies() {
                let body = entity.body;
                assert!(
                    !physics::collides(world.terrain(), body.x, body.y, state.kind.stats().radius),
                    "{:?} in rock at {hz} Hz, tick {tick}: ({}, {})",
                    state.kind,
                    body.x,
                    body.y
                );
                if let Some((mx, my, half_height)) = pilot {
                    let (half_x, half_y) = state.kind.contact_half();
                    let overlap_x = (half_x + PILOT_HALF_WIDTH) - (body.x - mx).abs();
                    let overlap_y = (half_y + half_height) - (body.y - my).abs();
                    assert!(
                        overlap_x <= 1.5 || overlap_y <= 1.5,
                        "{:?} on the pilot at {hz} Hz, tick {tick}: {overlap_x} x {overlap_y}",
                        state.kind
                    );
                }
            }
        }
        assert!(
            most > 6,
            "the guardians raised and summoned at {hz} Hz: {most}"
        );
    }
}

/// A pilot sheltering under one of the crypt's platforms never makes a guardian forget them,
/// walk home and heal: it keeps after its prey (the colossus leaps off the ledge above them)
/// and its wounds stay.
#[test]
fn a_guardian_never_forgets_a_pilot_under_its_crypts_platforms_or_heals() {
    // The default world's first mesa tomb (its crypt's west platform shelters the pilot).
    let settings = settings(crate::worldgen::DEFAULT_WORLD_SEED);
    let site = tomb_sites::sites_touching(
        &settings,
        &TileRect {
            min: TilePosition { x: -9_000, y: -100 },
            max: TilePosition { x: 9_000, y: 900 },
        },
    )
    .into_iter()
    .find(|site| {
        site.style == SiteStyle::SideView && site.setting == crate::tomb::SiteSetting::Mesa
    })
    .expect("a mesa tomb");
    for kind in [EnemyKind::SunColossus, EnemyKind::HollowMatriarch] {
        let mut world = world_on(&settings, 60, EnemyDifficulty::Normal);
        enter(&mut world, &site, 0, &[PLAYER]);
        world.scenario_tomb_floor(PLAYER, site.floors() - 1);
        no_creatures(&mut world);
        let room = world
            .tomb_run_of(PLAYER)
            .and_then(|run| run.rooms.iter().find(|room| room.kind == RoomKind::Boss))
            .map(|room| room.interior)
            .expect("the guardian's crypt");
        // Under the west platform (the realizer's left + 2 ..= left + 4, four rows up).
        let shelter = TilePosition {
            x: room.min.x + 3,
            y: room.max.y,
        };
        let boss = world
            .scenario_tomb_guardian(PLAYER, kind)
            .expect("the guardian wakes");
        world.scenario_hurt_enemy_to(boss, 0.5);
        let wounded = world
            .enemies()
            .find(|(entity, _)| entity.id == boss)
            .map(|(_, state)| state.health)
            .expect("the guardian");
        let mut hunted = false;
        let mut came_down = false;
        for tick in 0..60 * 20 {
            stand(&mut world, PLAYER, shelter);
            if let Some((id, _, _)) = world.pilot_of(PLAYER) {
                world.edit_pilot(id, |_, state| state.health = state.max_health);
            }
            let now = world.simulation_tick();
            let _ = world.simulate_tick(now, &TickInputs::default());
            let Some((_, state)) = world.enemies().find(|(entity, _)| entity.id == boss) else {
                break;
            };
            assert_ne!(
                state.mode,
                crate::enemy::EnemyMode::Return,
                "{kind:?} walked home at tick {tick}"
            );
            assert!(
                state.health <= wounded,
                "{kind:?} healed at tick {tick}: {} > {wounded}",
                state.health
            );
            hunted |= state.target.is_some();
            let floor = (shelter.y + 1) as f32 * TILE_SIZE;
            let (_, body, _) = world.pilot_of(PLAYER).expect("a pilot");
            if let Some((guardian, _)) = world.enemies().find(|(entity, _)| entity.id == boss) {
                came_down |= tick > 300
                    && guardian.body.y > floor - TILE_SIZE * 1.5
                    && (guardian.body.x - body.x).abs() < TILE_SIZE * 5.0;
            }
            assert!(
                !hunted || state.target.is_some(),
                "{kind:?} lost its prey at tick {tick}"
            );
        }
        assert!(hunted, "{kind:?} went after the pilot");
        if kind == EnemyKind::SunColossus {
            assert!(
                came_down,
                "the colossus came down off the ledge to the pilot"
            );
        }
    }
}
