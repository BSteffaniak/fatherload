//! Top-down crypts in the world (WP-TOMBS T29): the pilot's body never clips crypt walls,
//! rolls pass through strikes, traps and pits, crypt creatures keep to the body rules (spectres
//! alone drift through walls), the scripted bot beats the crypt lord through its phases and
//! clears a crypt of every tier, traps telegraph and bite on a tick-rate-safe cycle, and saves
//! keep the crypt's trap state.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    reason = "test fixtures convert small tile, tick and pixel numbers freely"
)]

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, TickOutput,
    WeaponKind, WorldState,
    balance::tombs::{TombBot, find_site},
    enemy::{EnemyDifficulty, EnemyKind},
    physics,
    pilot::{PILOT_HALF_WIDTH, PilotView, TOP_DOWN_FOOTPRINT, top_down::footprint_blocked},
    terrain::{TILE_SIZE, TilePosition},
    tomb::{
        RoomKind, RunOutcome, SiteStyle, TombSite,
        crypt::{CryptTrap, CryptTrapKind},
    },
    weapon::AmmoKind,
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V10,
        ..WorldSettings::default()
    }
}

fn crypt(tier: u8) -> TombSite {
    find_site(&settings(), SiteStyle::TopDown, tier).expect("a crypt of the tier")
}

fn world_at(hz: u32, difficulty: EnemyDifficulty) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(difficulty);
    world
}

/// Kit that fits `tier` (the probe's), with a rifle.
fn kit(world: &mut WorldState, tier: u8) {
    if let Some(rig) = world.player_mut(PLAYER) {
        let kit = tier.saturating_sub(1).min(3);
        rig.pilot_gear.suit = kit;
        rig.pilot_gear.armor = kit;
        let _ = rig.pilot_arsenal.receive(WeaponKind::Rifle);
        let _ = rig.pilot_arsenal.receive(WeaponKind::Sidearm);
        rig.pilot_arsenal.equipped = Some(WeaponKind::Rifle);
        let _ = rig
            .pilot_arsenal
            .add_ammo(AmmoKind::Cartridge, crate::pilot::arsenal::MAX_PILOT_AMMO);
    }
}

fn enter(world: &mut WorldState, site: &TombSite) -> u32 {
    let _ = world.place_pilot(PLAYER, 0.0, 0.0);
    world
        .scenario_enter_tomb(site, &[PLAYER])
        .expect("the run starts")
}

fn tick(world: &mut WorldState, inputs: &TickInputs) -> Vec<GameplayEventKind> {
    let now = world.simulation_tick();
    let output: TickOutput = world.simulate_tick(now, inputs);
    output.events.into_iter().map(|event| event.kind).collect()
}

/// The pilot view of the world's terrain for footprint tests.
struct Blocks<'a>(&'a WorldState);

impl crate::pilot::PilotTerrain for Blocks<'_> {
    fn blocks(&self, tile: TilePosition) -> bool {
        crate::world::pilot::pilot_blocks(self.0.terrain(), tile)
    }
    fn climbable(&self, _: TilePosition) -> bool {
        false
    }
    fn water(&self, _: TilePosition) -> bool {
        false
    }
    fn platform_top(&self, _: f32, _: f32, _: f32, _: f32) -> Option<f32> {
        None
    }
}

/// Every crypt creature keeps to the body rules after a tick: out of the walls (spectres
/// excepted) and never overlapping the pilot by more than a rounding hair.
fn assert_bodies(world: &WorldState) {
    let pilot = world
        .pilot_of(PLAYER)
        .map(|(_, body, state)| (state.middle(body), state.height() * 0.5));
    for (entity, state) in world.enemies() {
        if !state.kind.is_crypt() {
            continue;
        }
        let radius = state.kind.stats().radius;
        if !state.kind.passes_walls() {
            assert!(
                !physics::collides(world.terrain(), entity.body.x, entity.body.y, radius),
                "{:?} inside a wall at ({}, {}) mode {:?} v ({}, {}) prev ({}, {}) tiles {:?}",
                state.kind,
                entity.body.x,
                entity.body.y,
                state.mode,
                entity.body.velocity_x,
                entity.body.velocity_y,
                entity.body.previous_x,
                entity.body.previous_y,
                [-radius, 0.0, radius]
                    .into_iter()
                    .flat_map(|dy| [-radius, 0.0, radius].into_iter().map(move |dx| (dx, dy)))
                    .map(|(dx, dy)| world
                        .terrain()
                        .tile(TilePosition {
                            x: ((entity.body.x + dx) / TILE_SIZE).floor() as i32,
                            y: ((entity.body.y + dy) / TILE_SIZE).floor() as i32
                        })
                        .map(|t| t.kind))
                    .collect::<Vec<_>>()
            );
        }
        if let Some(((px, py), half_h)) = pilot {
            let (own_w, own_h) = state.kind.contact_half();
            let depth_x = own_w + PILOT_HALF_WIDTH - (entity.body.x - px).abs();
            let depth_y = own_h + half_h - (entity.body.y - py).abs();
            assert!(
                depth_x <= 1.5 || depth_y <= 1.5,
                "{:?} overlaps the pilot by ({depth_x}, {depth_y})",
                state.kind
            );
        }
    }
}

#[test]
fn the_pilot_and_crypt_creatures_keep_to_the_walls_through_a_fight() {
    let site = crypt(2);
    let mut world = world_at(60, EnemyDifficulty::Normal);
    kit(&mut world, 2);
    enter(&mut world, &site);
    let mut bot = TombBot::new(PLAYER, true);
    let mut creatures_seen = std::collections::BTreeSet::new();
    for _ in 0..60 * 150 {
        if world.tomb_run_of(PLAYER).is_none() {
            break;
        }
        let inputs = bot.inputs(&world);
        let _ = tick(&mut world, &inputs);
        if let Some((_, body, state)) = world.pilot_of(PLAYER)
            && state.view == PilotView::TopDown
        {
            assert!(
                !footprint_blocked(&Blocks(&world), body.x, body.y),
                "the pilot's footprint is in a wall at ({}, {})",
                body.x,
                body.y
            );
        }
        assert_bodies(&world);
        creatures_seen.extend(world.enemies().map(|(_, state)| state.kind));
    }
    assert!(
        creatures_seen.iter().any(|kind| kind.is_crypt()),
        "the fight had crypt creatures: {creatures_seen:?}"
    );
}

/// The interior of the first room of `kind` on `player`'s floor.
fn room(world: &WorldState, kind: RoomKind) -> crate::worldgen::features::TileRect {
    world
        .tomb_run_of(PLAYER)
        .and_then(|run| run.rooms.iter().find(|room| room.kind == kind).copied())
        .expect("the room")
        .interior
}

fn stand_at(world: &mut WorldState, tile: TilePosition) {
    let (x, feet) = (
        (tile.x as f32 + 0.5) * TILE_SIZE,
        (tile.y + 1) as f32 * TILE_SIZE - 4.0,
    );
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(id, |body, state| {
        body.x = x;
        body.y = feet;
        body.previous_x = x;
        body.previous_y = feet;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
        state.heading_x = 127;
        state.heading_y = 0;
    });
}

/// A trap overlay on every tile of `interior`'s middle rows, `kind`, for the room of index 0.
fn field(
    interior: crate::worldgen::features::TileRect,
    kind: CryptTrapKind,
    phase: u16,
) -> Vec<CryptTrap> {
    let mid = interior.center().y;
    (interior.min.x + 1..interior.max.x)
        .flat_map(|x| {
            (mid - 1..=mid + 1).map(move |y| CryptTrap {
                at: TilePosition { x, y },
                kind,
                dir: (0, 0),
                phase,
                hidden: false,
                room: 0,
            })
        })
        .collect()
}

/// A crypt run on Normal (traps bite: on Peaceful they are harmless, like every hazard of the
/// tombs) with the floor's creatures cleared away.
fn quiet_crypt(site: &TombSite, hz: u32) -> WorldState {
    let mut world = world_at(hz, EnemyDifficulty::Normal);
    enter(&mut world, site);
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    world
}

fn hurt_total(events: &[GameplayEventKind]) -> u32 {
    events
        .iter()
        .map(|event| match event {
            GameplayEventKind::PilotHurt { amount, .. } => u32::from(*amount),
            _ => 0,
        })
        .sum()
}

#[test]
fn a_roll_passes_through_spikes_strikes_and_pits_but_standing_does_not() {
    let site = crypt(1);
    let mut world = quiet_crypt(&site, 60);
    let interior = room(&world, RoomKind::Entry);
    let mid = interior.center().y;
    // Spikes always up (phase inside the active window, and a cycle longer than the test).
    world.scenario_crypt_traps(PLAYER, field(interior, CryptTrapKind::Spikes, 1_700), 0);
    stand_at(
        &mut world,
        TilePosition {
            x: interior.min.x,
            y: mid,
        },
    );
    let roll = TickInputs::held(PLAYER, ControlState::new(1.0, false, false).with_jump(true));
    let mut events = Vec::new();
    // The roll's guarded stretch: no hurt over the spike field.
    for _ in 0..15 {
        events.extend(tick(&mut world, &roll));
    }
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::CryptRoll { .. }))
    );
    assert_eq!(hurt_total(&events), 0, "rolling over spikes: {events:?}");
    // A strike mid-roll misses.
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(id, |_, state| {
        state.roll = 0.2;
        state.roll_x = 127;
        state.roll_y = 0;
    });
    let mut output = TickOutput::default();
    assert!(!world.strike_pilot(id, 10.0, (0.0, 0.0), 100.0, &mut output));
    // Standing in them hurts.
    world.edit_pilot(id, |_, state| state.roll = 0.0);
    let still = TickInputs::held(PLAYER, ControlState::default());
    let mut standing = Vec::new();
    for _ in 0..30 {
        standing.extend(tick(&mut world, &still));
    }
    assert!(hurt_total(&standing) > 0, "standing on spikes hurts");

    // Pits: rolling over a pit row crosses it; walking in falls.
    world.scenario_crypt_traps(PLAYER, field(interior, CryptTrapKind::Pit, 0), 0);
    stand_at(
        &mut world,
        TilePosition {
            x: interior.min.x,
            y: mid,
        },
    );
    world.edit_pilot(id, |_, state| {
        state.roll_cooldown = 0.0;
        state.health = state.max_health;
    });
    let mut rolled = Vec::new();
    for _ in 0..12 {
        rolled.extend(tick(&mut world, &roll));
    }
    assert!(
        !rolled
            .iter()
            .any(|event| matches!(event, GameplayEventKind::CryptPitFall { .. })),
        "a roll sails over the pits"
    );
    let walk = TickInputs::held(PLAYER, ControlState::new(1.0, false, false));
    let mut walked = Vec::new();
    for _ in 0..60 {
        walked.extend(tick(&mut world, &walk));
    }
    assert!(
        walked
            .iter()
            .any(|event| matches!(event, GameplayEventKind::CryptPitFall { .. })),
        "walking into a pit falls in"
    );
    // The fall climbs out onto safe floor.
    let (_, body, state) = world.pilot_of(PLAYER).expect("pilot");
    let centre = TilePosition {
        x: (body.x / TILE_SIZE).floor() as i32,
        y: ((body.y - TOP_DOWN_FOOTPRINT * 0.5) / TILE_SIZE).floor() as i32,
    };
    let run = world.tomb_run_of(PLAYER).expect("run");
    assert!(
        run.crypt
            .trap_at(centre)
            .is_none_or(|trap| trap.kind != CryptTrapKind::Pit)
    );
    assert!(state.health < state.max_health);
}

#[test]
fn cracked_floors_warn_then_give_way_and_plates_click_before_their_arrows() {
    let site = crypt(1);
    let mut world = quiet_crypt(&site, 60);
    let interior = room(&world, RoomKind::Entry);
    let centre = interior.center();
    let crack = CryptTrap {
        at: centre,
        kind: CryptTrapKind::CrackedFloor,
        dir: (0, 0),
        phase: 0,
        hidden: false,
        room: 0,
    };
    let plate = CryptTrap {
        at: TilePosition {
            x: centre.x + 3,
            y: centre.y,
        },
        kind: CryptTrapKind::ArrowPlate,
        dir: (0, 1),
        phase: 0,
        hidden: false,
        room: 0,
    };
    world.scenario_crypt_traps(PLAYER, vec![crack, plate], 0);
    stand_at(&mut world, centre);
    let still = TickInputs::held(PLAYER, ControlState::default());
    let first = tick(&mut world, &still);
    assert!(first.iter().any(|event| matches!(
        event,
        GameplayEventKind::CryptTrap {
            kind: CryptTrapKind::CrackedFloor,
            fired: false,
            ..
        }
    )));
    // The beat before it gives way: room to step off.
    let crumble = world.timing().ticks_for_millis(super::CRUMBLE_MILLIS);
    for _ in 0..crumble.saturating_sub(3) {
        let events = tick(&mut world, &still);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, GameplayEventKind::CryptPitFall { .. }))
        );
    }
    let mut later = Vec::new();
    for _ in 0..10 {
        later.extend(tick(&mut world, &still));
    }
    assert!(
        later
            .iter()
            .any(|event| matches!(event, GameplayEventKind::CryptPitFall { .. }))
    );
    assert_eq!(
        world
            .tomb_run_of(PLAYER)
            .and_then(|run| run.crypt.trap_at(centre).map(|trap| trap.kind)),
        Some(CryptTrapKind::Pit)
    );
    // The plate: a click, then arrows from both walls down its column.
    stand_at(&mut world, plate.at);
    let mut events = Vec::new();
    let mut click_at = None;
    let mut arrows_at = None;
    for step in 0..60 {
        let now = tick(&mut world, &still);
        if click_at.is_none()
            && now.iter().any(|event| {
                matches!(
                    event,
                    GameplayEventKind::CryptTrap {
                        kind: CryptTrapKind::ArrowPlate,
                        fired: false,
                        ..
                    }
                )
            })
        {
            click_at = Some(step);
        }
        if arrows_at.is_none()
            && now.iter().any(|event| {
                matches!(
                    event,
                    GameplayEventKind::CryptTrap {
                        kind: CryptTrapKind::ArrowPlate,
                        fired: true,
                        ..
                    }
                )
            })
        {
            arrows_at = Some(step);
        }
        events.extend(now);
    }
    let (click, arrows) = (click_at.expect("a click"), arrows_at.expect("arrows"));
    let delay = world.timing().ticks_for_millis(super::PLATE_DELAY_MILLIS);
    assert!(
        u64::try_from(arrows - click).unwrap_or(0) + 1 >= delay,
        "{click} {arrows}"
    );
    assert!(
        hurt_total(&events) > 0,
        "standing on the plate takes an arrow"
    );
}

#[test]
fn crypt_creatures_obey_walls_and_spectres_drift_through_them() {
    let site = crypt(1);
    let mut world = world_at(60, EnemyDifficulty::Normal);
    enter(&mut world, &site);
    world.scenario_crypt_traps(PLAYER, Vec::new(), 0);
    let interior = room(&world, RoomKind::Entry);
    stand_at(&mut world, interior.center());
    world.enable_test_mode();
    // One of each kind in the room's corners, a spectre deep in the wall to the north.
    let corner = |dx: i32, dy: i32| {
        (
            (interior.min.x + dx) as f32 * TILE_SIZE + 16.0,
            (interior.min.y + dy) as f32 * TILE_SIZE + 16.0,
        )
    };
    for (kind, (dx, dy)) in [
        (EnemyKind::CryptSkeleton, (1, 1)),
        (EnemyKind::Cultist, (9, 1)),
        (EnemyKind::CryptGolem, (1, 7)),
    ] {
        let (x, y) = corner(dx, dy);
        let _ = world.spawn_enemy(kind, x, y);
    }
    let (sx, sy) = corner(5, -1);
    let spectre = world.spawn_enemy(EnemyKind::Spectre, sx, sy);
    world.entities_mut().update(spectre, |entity| {
        entity.body.x = sx;
        entity.body.y = sy;
    });
    let invulnerable = |world: &mut WorldState| {
        let id = world.pilot_of(PLAYER).expect("pilot").0;
        world.edit_pilot(id, |_, state| state.health = state.max_health);
    };
    let still = TickInputs::held(PLAYER, ControlState::default());
    let mut spectre_in_wall = false;
    let mut closest = f32::MAX;
    for _ in 0..60 * 15 {
        let _ = tick(&mut world, &still);
        invulnerable(&mut world);
        assert_bodies(&world);
        if let Some(entity) = world.entities().get(spectre) {
            let radius = EnemyKind::Spectre.stats().radius;
            spectre_in_wall |=
                physics::collides(world.terrain(), entity.body.x, entity.body.y, radius);
            let (px, py) = world
                .pilot_of(PLAYER)
                .map(|(_, body, state)| state.middle(body))
                .expect("pilot");
            closest = closest.min((entity.body.x - px).hypot(entity.body.y - py));
        }
    }
    assert!(spectre_in_wall, "the spectre was inside the wall");
    assert!(
        closest < TILE_SIZE * 2.0,
        "and came through it to the pilot: {closest}"
    );
}

#[test]
fn the_scripted_bot_beats_the_crypt_lord_through_its_phases() {
    let site = crypt(1);
    let mut world = world_at(60, EnemyDifficulty::Normal);
    kit(&mut world, 2);
    enter(&mut world, &site);
    world.scenario_tomb_floor(PLAYER, site.floors() - 1);
    // Straight to the lord's hall (the rooms before it are the other tests' business).
    let hall = room(&world, RoomKind::Boss);
    stand_at(
        &mut world,
        TilePosition {
            x: hall.min.x + 2,
            y: hall.center().y + 4,
        },
    );
    let mut bot = TombBot::new(PLAYER, true);
    let mut phases = std::collections::BTreeSet::new();
    let mut defeated = false;
    for _ in 0..60 * 60 * 6 {
        if world.tomb_run_of(PLAYER).is_none() {
            break;
        }
        let inputs = bot.inputs(&world);
        for event in tick(&mut world, &inputs) {
            match event {
                GameplayEventKind::CryptLordPhase { phase, .. } => {
                    phases.insert(phase);
                }
                GameplayEventKind::TombGuardianDefeated { .. } => defeated = true,
                _ => {}
            }
        }
    }
    assert!(defeated, "the lord fell (phases seen {phases:?})");
    assert_eq!(
        phases.into_iter().collect::<Vec<_>>(),
        vec![1, 2, 3],
        "through all three phases"
    );
    assert_eq!(
        world
            .tombs()
            .summaries
            .get(&PLAYER)
            .map(|summary| summary.outcome),
        Some(RunOutcome::Finished)
    );
}

#[test]
fn the_scripted_bot_clears_a_crypt_of_every_tier() {
    for tier in 1..=crate::tomb::MAX_TIER {
        let site = crypt(tier);
        // Peaceful: the floors, traps, puzzles and picks of each tier (the lord has its own
        // test on Normal).
        let mut world = world_at(60, EnemyDifficulty::Peaceful);
        kit(&mut world, tier.max(4));
        enter(&mut world, &site);
        let mut bot = TombBot::new(PLAYER, false);
        let mut outcome = None;
        for _ in 0..60 * 60 * 15 {
            if world.tomb_run_of(PLAYER).is_none() {
                break;
            }
            let inputs = bot.inputs(&world);
            for event in tick(&mut world, &inputs) {
                if let GameplayEventKind::TombRunEnded { outcome: ended, .. } = event {
                    outcome = Some(ended);
                }
            }
        }
        assert_eq!(
            outcome,
            Some(RunOutcome::Finished),
            "tier {tier}: {:?}",
            world.tombs().summaries.get(&PLAYER)
        );
    }
}

#[test]
fn crypt_traps_bite_the_same_at_every_tick_rate() {
    let damage = |hz: u32| {
        let site = crypt(1);
        let mut world = quiet_crypt(&site, hz);
        let interior = room(&world, RoomKind::Entry);
        // A spike field and a flame line on their cycles.
        let mut traps = field(interior, CryptTrapKind::Spikes, 0);
        traps.retain(|trap| trap.at.x < interior.center().x);
        traps.extend(
            field(interior, CryptTrapKind::FlameJet, 400)
                .into_iter()
                .filter(|trap| trap.at.x >= interior.center().x),
        );
        world.scenario_crypt_traps(PLAYER, traps, 0);
        stand_at(
            &mut world,
            TilePosition {
                x: interior.center().x - 2,
                y: interior.center().y,
            },
        );
        let still = TickInputs::held(PLAYER, ControlState::default());
        let mut events = Vec::new();
        // The health actually lost (shown hit numbers round each tick's sliver).
        let mut lost = 0.0_f32;
        for _ in 0..hz * 6 {
            events.extend(tick(&mut world, &still));
            let (id, _, state) = world.pilot_of(PLAYER).expect("pilot");
            lost += state.max_health - state.health;
            // Keep standing (no downing) to measure the whole cycle.
            world.edit_pilot(id, |_, state| state.health = state.max_health);
        }
        let hurt = lost.round() as u32;
        let roll_events = events
            .iter()
            .filter(|event| matches!(event, GameplayEventKind::CryptTrap { .. }))
            .count();
        (hurt, roll_events)
    };
    let (base, cues) = damage(60);
    assert!(base > 0);
    for hz in [30, 120] {
        let (other, other_cues) = damage(hz);
        let diff = (f64::from(other) - f64::from(base)).abs() / f64::from(base);
        assert!(diff < 0.08, "{hz} Hz: {other} vs {base}");
        assert_eq!(other_cues, cues, "{hz} Hz: the same warnings and bursts");
    }
}

#[test]
fn a_crypt_saved_mid_floor_keeps_its_traps_and_creatures() {
    let site = crypt(1);
    let mut world = world_at(60, EnemyDifficulty::Normal);
    enter(&mut world, &site);
    let interior = room(&world, RoomKind::Entry);
    let crack = CryptTrap {
        at: interior.center(),
        kind: CryptTrapKind::CrackedFloor,
        dir: (0, 0),
        phase: 0,
        hidden: true,
        room: 0,
    };
    world.scenario_crypt_traps(PLAYER, vec![crack], 0);
    stand_at(&mut world, interior.center());
    let still = TickInputs::held(PLAYER, ControlState::default());
    let _ = tick(&mut world, &still);
    let json = serde_json::to_string(&world).expect("save");
    let mut loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.tombs(), world.tombs());
    assert!(
        !world
            .tomb_run_of(PLAYER)
            .expect("run")
            .crypt
            .crumbling
            .is_empty()
    );
    for _ in 0..120 {
        let a = tick(&mut world, &still);
        let b = tick(&mut loaded, &still);
        assert_eq!(a, b);
    }
    assert_eq!(
        world.simulation_state_hash(),
        loaded.simulation_state_hash()
    );
}

/// The entry room and a room beside it east or west, sharing rows (for tests across a wall).
fn entry_and_neighbour(
    world: &WorldState,
) -> (
    crate::worldgen::features::TileRect,
    crate::worldgen::features::TileRect,
) {
    let entry = room(world, RoomKind::Entry);
    let neighbour = world
        .tomb_run_of(PLAYER)
        .and_then(|run| {
            run.rooms
                .iter()
                .map(|room| room.interior)
                .filter(|other| *other != entry)
                .filter(|other| other.min.y <= entry.max.y && other.max.y >= entry.min.y)
                .filter(|other| other.min.x > entry.max.x || other.max.x < entry.min.x)
                .min_by_key(|other| (other.center().x - entry.center().x).abs())
        })
        .expect("a room beside the entry");
    (entry, neighbour)
}

#[test]
fn a_spectre_crosses_the_masonry_between_rooms_to_its_prey() {
    let site = crypt(1);
    let mut world = world_at(60, EnemyDifficulty::Normal);
    enter(&mut world, &site);
    world.scenario_crypt_traps(PLAYER, Vec::new(), 0);
    world.enable_test_mode();
    let (entry, other) = entry_and_neighbour(&world);
    // Both near the shared rows' top, away from the doorway in the middle of the wall.
    let row = entry.min.y.max(other.min.y) + 1;
    let (pilot_x, spectre_x) = if other.min.x > entry.max.x {
        (entry.max.x - 1, other.min.x + 2)
    } else {
        (entry.min.x + 1, other.max.x - 2)
    };
    stand_at(&mut world, TilePosition { x: pilot_x, y: row });
    let (sx, sy) = (
        (spectre_x as f32 + 0.5) * TILE_SIZE,
        (row as f32 + 0.5) * TILE_SIZE,
    );
    let spectre = world.spawn_enemy(EnemyKind::Spectre, sx, sy);
    world.entities_mut().update(spectre, |entity| {
        entity.body.x = sx;
        entity.body.y = sy;
        entity.body.previous_x = sx;
        entity.body.previous_y = sy;
    });
    let still = TickInputs::held(PLAYER, ControlState::default());
    let mut through_wall = false;
    let mut reached = false;
    for _ in 0..60 * 15 {
        let _ = tick(&mut world, &still);
        let id = world.pilot_of(PLAYER).expect("pilot").0;
        world.edit_pilot(id, |_, state| state.health = state.max_health);
        assert_bodies(&world);
        let Some(entity) = world.entities().get(spectre) else {
            break;
        };
        let centre = TilePosition {
            x: (entity.body.x / TILE_SIZE).floor() as i32,
            y: (entity.body.y / TILE_SIZE).floor() as i32,
        };
        through_wall |= crate::world::pilot::pilot_blocks(world.terrain(), centre);
        let (px, py) = world
            .pilot_of(PLAYER)
            .map(|(_, body, state)| state.middle(body))
            .expect("pilot");
        if (entity.body.x - px).hypot(entity.body.y - py) < TILE_SIZE * 1.5 {
            reached = true;
            break;
        }
    }
    assert!(through_wall, "the spectre's middle passed through the wall");
    assert!(reached, "and it reached the pilot on the far side");
}

#[test]
fn a_golem_never_slams_through_a_wall() {
    let site = crypt(1);
    let mut world = world_at(60, EnemyDifficulty::Normal);
    enter(&mut world, &site);
    world.scenario_crypt_traps(PLAYER, Vec::new(), 0);
    world.enable_test_mode();
    let (entry, other) = entry_and_neighbour(&world);
    let row = entry.min.y.max(other.min.y) + 1;
    // Each flush against its side of the wall: well inside a slam's reach, no line of sight.
    let (pilot_x, golem_x) = if other.min.x > entry.max.x {
        (entry.max.x, other.min.x)
    } else {
        (entry.min.x, other.max.x)
    };
    stand_at(&mut world, TilePosition { x: pilot_x, y: row });
    // Pressed right up to the wall on both sides (the wall is a tile thick).
    let side: f32 = if golem_x > pilot_x { 1.0 } else { -1.0 };
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(id, |body, _| {
        body.x = side.mul_add(TILE_SIZE * 0.5 - PILOT_HALF_WIDTH - 0.5, body.x);
        body.previous_x = body.x;
    });
    let radius = EnemyKind::CryptGolem.stats().radius;
    let (gx, gy) = (
        (-side).mul_add(
            TILE_SIZE * 0.5 - radius - 0.5,
            (golem_x as f32 + 0.5) * TILE_SIZE,
        ),
        (row as f32 + 0.5) * TILE_SIZE,
    );
    let golem = world.spawn_enemy(EnemyKind::CryptGolem, gx, gy);
    let now = world.simulation_tick();
    world.entities_mut().update(golem, |entity| {
        entity.body.x = gx;
        entity.body.y = gy;
        if let crate::entity::EntityState::Enemy(state) = &mut entity.state {
            state.target = Some(PLAYER);
            state.last_seen = now;
            state.mode = crate::enemy::EnemyMode::Chase;
        }
    });
    let (px, py) = world
        .pilot_of(PLAYER)
        .map(|(_, body, state)| state.middle(body))
        .expect("pilot");
    let reach = EnemyKind::CryptGolem.stats().attack_range_tiles * TILE_SIZE;
    assert!(
        (gx - px).hypot(gy - py) < reach.min(crate::world::SLAM_REACH),
        "in reach: {}",
        (gx - px).hypot(gy - py)
    );
    assert!(!physics::line_clear(world.terrain(), gx, gy, px, py));
    let still = TickInputs::held(PLAYER, ControlState::default());
    // Pinned in place for a few seconds: it may not wind up through the masonry.
    for _ in 0..60 * 4 {
        world.entities_mut().update(golem, |entity| {
            entity.body.x = gx;
            entity.body.y = gy;
            entity.body.velocity_x = 0.0;
            entity.body.velocity_y = 0.0;
        });
        let events = tick(&mut world, &still);
        assert!(
            !events.iter().any(|event| matches!(
                event,
                GameplayEventKind::CryptSlam { .. } | GameplayEventKind::CryptTell { .. }
            )),
            "a golem wound up or slammed through a wall"
        );
        assert_eq!(hurt_total(&events), 0, "nothing reached the pilot");
    }
}
