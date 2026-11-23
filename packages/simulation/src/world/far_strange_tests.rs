//! The far Strange Layers in a Deep Descent world (Wave 9 strange-b, Descent VI strata 4-6 and
//! its gate): slow pools and fast lanes, time bubbles, creatures at their own speeds (mites,
//! sloths' amber, newts' rewinds), sonar pings in the dark, blind hunters and shriekers, lamp
//! filters and colour-keyed walls, scarabs, spires and wraiths, builder alloy and the Phase
//! Drill, and The Glitch beaten by a bot with Descent VI kit.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WeaponControl, WorldState,
    cave::CaveTile,
    depth_progression::RigSystem,
    descent::Stratum,
    enemy::{
        Damage, DamageKind, DamageOutcome, DamageSource, EnemyDifficulty, EnemyKind, EnemyMode,
    },
    entity::{EntityId, EntityState},
    explosive::ExplosiveKind,
    far_strange::{
        FarLandmark, FarStrangeAction, FarStrangeEvent, GLITCH_REWRITE_MILLIS, GlitchRule,
        LampFilter, PRISM_CLOSE_MILLIS, SONAR_PING_TILES, SONAR_SUITE_PING_TILES,
        TICK_MITE_LIFE_SECONDS, scarab_shell,
    },
    hazard::PlacedBomb,
    logistics::LogisticsCommand,
    position::WorldPos,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    weapon::WeaponKind,
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{
            far_strange::{self as sites, FarStampKind},
            layout,
        },
    },
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x5B9_7E57;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// The rig systems a rig holds on reaching Descent VI: every gate's reward through the
/// Gravity Anchor (Descent V's), the companions too, never the Phase Drill.
fn descent_six_systems() -> u8 {
    RigSystem::ALL
        .into_iter()
        .filter(|system| !matches!(system, RigSystem::PhaseDrill | RigSystem::HeartKey))
        .fold(0, |bits, system| bits | system.bit())
}

/// A Deep Descent world at `hz`, the rig fitted for Descent VI and the director quiet.
fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hazard_gear.rig_systems = descent_six_systems();
    rig.radiator_level = 15;
    rig.hull_level = 15;
    rig.hull = rig.max_hull();
    rig.credits = 5_000_000;
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

fn fill(world: &mut WorldState, rect: TileRect, kind: TileKind) {
    for position in rect.positions() {
        let _ = world.terrain_mut().set_kind(position, kind);
    }
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

fn far_events(events: &[GameplayEventKind]) -> Vec<FarStrangeEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::FarStrange { what, .. } => Some(*what),
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

fn act(world: &mut WorldState, action: FarStrangeAction) -> PlayerScopedCommandOutcome {
    command(
        world,
        PlayerCommand::Logistics(LogisticsCommand::FarStrange(action)),
    )
    .0
}

fn middle_row(stratum: Stratum) -> i32 {
    let (top, bottom) = layout(&settings()).rows(stratum);
    top + (bottom - top) / 2
}

/// A sealed stone arena `2 * half_w + 1` wide centred on `(x, y)`, open inside, the rig on its
/// floor in the middle.
fn arena(world: &mut WorldState, x: i32, y: i32, half_w: i32, half_h: i32) -> TilePosition {
    put_rig(world, TilePosition { x, y });
    // A first tick here grants the depth's reward relic (it lands beside the rig): carve after.
    let _ = ticks(world, 2);
    fill(
        world,
        TileRect::around(TilePosition { x, y }, half_w + 2, half_h + 2),
        TileKind::Stone,
    );
    fill(
        world,
        TileRect::around(TilePosition { x, y }, half_w, half_h),
        TileKind::Air,
    );
    let floor = TilePosition { x, y: y + half_h };
    put_rig(world, floor);
    // A deeper row may grant another relic beside the rig: clear the arena once more.
    let _ = ticks(world, 2);
    fill(
        world,
        TileRect::around(TilePosition { x, y }, half_w, half_h),
        TileKind::Air,
    );
    put_rig(world, floor);
    floor
}

fn rig_xy(world: &WorldState) -> (f32, f32) {
    let rig = world.player(PLAYER).expect("rig");
    (rig.x, rig.y)
}

// --- The Time Sludge --------------------------------------------------------------------------

/// The first time pocket of `kind` in the Time Sludge, its centre open.
fn pocket(kind: crate::far_strange::TimePocketKind) -> sites::FarStamp {
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::TimeSludge);
    let area = TileRect {
        min: TilePosition { x: -800, y: top },
        max: TilePosition { x: 800, y: bottom },
    };
    sites::stamps_with(&settings, &layout, &area)
        .into_iter()
        .find(|stamp| {
            stamp.kind == FarStampKind::TimePocket(kind)
                && sites::pocket_open(stamp, stamp.bounds.center().x, stamp.bounds.center().y)
        })
        .expect("a pocket")
}

#[test]
fn slow_pools_and_fast_lanes_bend_time_for_a_falling_rig_alike_at_every_tick_rate() {
    use crate::far_strange::TimePocketKind;
    let fall = |hz: u32, kind: Option<TimePocketKind>| {
        let mut world = world_at(hz);
        // The top open row of the pocket's middle column (a neutral shaft for no pocket).
        let start = if let Some(kind) = kind {
            let stamp = pocket(kind);
            let x = stamp.bounds.center().x;
            let y = (stamp.bounds.min.y..=stamp.bounds.max.y)
                .find(|y| sites::pocket_open(&stamp, x, *y))
                .expect("an open row");
            put_rig(&mut world, TilePosition { x, y });
            TilePosition { x, y }
        } else {
            let centre = TilePosition {
                x: 300,
                y: middle_row(Stratum::TimeSludge),
            };
            put_rig(&mut world, centre);
            fill(&mut world, TileRect::around(centre, 1, 4), TileKind::Air);
            TilePosition {
                x: 300,
                y: centre.y - 3,
            }
        };
        // The depth's reward relic lands beside the rig on its first tick here: clear it.
        let _ = ticks(&mut world, 2);
        fill(&mut world, TileRect::around(start, 2, 0), TileKind::Air);
        fill(
            &mut world,
            TileRect {
                min: start,
                max: TilePosition {
                    x: start.x,
                    y: start.y + 2,
                },
            },
            TileKind::Air,
        );
        put_rig(&mut world, start);
        let (_, y0) = rig_xy(&world);
        let _ = ticks(&mut world, u64::from(hz) / 5);
        rig_xy(&world).1 - y0
    };
    for hz in [20, 60] {
        let neutral = fall(hz, None);
        let slow = fall(hz, Some(TimePocketKind::Slow));
        let fast = fall(hz, Some(TimePocketKind::Fast));
        assert!(neutral > 1.0, "a rig falls ({neutral})");
        assert!(
            slow < neutral * 0.8,
            "slow pool at {hz} Hz: {slow} vs {neutral}"
        );
        assert!(
            fast > neutral * 1.1,
            "fast lane at {hz} Hz: {fast} vs {neutral}"
        );
    }
    // The same fall at 20 and 60 Hz, within a pixel or two.
    let slow20 = fall(20, Some(TimePocketKind::Slow));
    let slow60 = fall(60, Some(TimePocketKind::Slow));
    assert!((slow20 - slow60).abs() < 3.0, "{slow20} vs {slow60}");
}

#[test]
fn crossing_into_a_pocket_is_announced_once() {
    let mut world = world_at(30);
    let stamp = pocket(crate::far_strange::TimePocketKind::Slow);
    let centre = stamp.bounds.center();
    put_rig(&mut world, centre);
    let events = far_events(&ticks(&mut world, 5));
    assert_eq!(
        events
            .iter()
            .filter(|event| **event == FarStrangeEvent::TimeShift)
            .count(),
        1
    );
}

/// Creature `kind` in an open arena at the Time Sludge's middle, `dx` tiles right of the rig.
fn creature_arena(world: &mut WorldState, kind: EnemyKind, dx: i32) -> (EntityId, TilePosition) {
    let floor = arena(world, 200, middle_row(Stratum::TimeSludge), 16, 4);
    let (x, y) = world.frame().tile_center(TilePosition {
        x: floor.x + dx,
        y: floor.y - 1,
    });
    (world.spawn_enemy(kind, x, y), floor)
}

fn enemy(world: &WorldState, id: EntityId) -> Option<(f32, f32, crate::enemy::EnemyState)> {
    world
        .entities()
        .get(id)
        .and_then(|entity| match entity.state {
            EntityState::Enemy(state) => Some((entity.body.x, entity.body.y, state)),
            _ => None,
        })
}

#[test]
fn a_time_bubble_slows_every_creature_inside_it_and_lapses() {
    let travel = |bubbled: bool| {
        let mut world = world_at(30);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        let (id, floor) = creature_arena(&mut world, EnemyKind::RewindNewt, 7);
        if bubbled {
            let (x, y) = world.frame().tile_center(TilePosition {
                x: floor.x + 7,
                y: floor.y - 1,
            });
            world.insert_charge(PlacedBomb {
                id: 9_001,
                x,
                y,
                previous_x: x,
                previous_y: y,
                kind: ExplosiveKind::TimeBubble,
                detonation_tick: world.simulation_tick().next(),
                owner: Some(PLAYER),
                ..PlacedBomb::default()
            });
        }
        let start = enemy(&world, id).expect("newt").0;
        let _ = ticks(&mut world, 45);
        let (x, _, _) = enemy(&world, id).expect("newt");
        ((x - start).abs(), world)
    };
    let (free, _) = travel(false);
    let (slowed, mut world) = travel(true);
    assert!(free > TILE_SIZE * 1.5, "the newt closed in ({free})");
    assert!(
        slowed < free * 0.5,
        "the bubble slowed it: {slowed} vs {free}"
    );
    assert_eq!(world.time_bubbles().len(), 1);
    // Ten seconds on, the bubble is gone.
    let _ = ticks(&mut world, 30 * 11);
    assert!(world.time_bubbles().is_empty());
}

#[test]
fn time_bubbles_sell_at_outposts_and_are_thrown_like_any_charge() {
    let mut world = world_at(30);
    let settings = settings();
    let layout = layout(&settings);
    let outpost = crate::worldgen::descent::landmarks::outpost(
        &settings,
        &layout,
        crate::descent::Descent::StrangeLayers,
    )
    .expect("an outpost");
    // Away from a seller: refused.
    put_rig(
        &mut world,
        TilePosition {
            x: outpost.bounds.center().x + 200,
            y: outpost.bounds.center().y,
        },
    );
    assert_ne!(
        act(&mut world, FarStrangeAction::BuyTimeBubbles),
        PlayerScopedCommandOutcome::Applied
    );
    put_rig(
        &mut world,
        TilePosition {
            x: outpost.bounds.center().x,
            y: outpost.bounds.max.y - 2,
        },
    );
    let _ = ticks(&mut world, 2);
    let credits = world.player(PLAYER).expect("rig").credits;
    assert_eq!(
        act(&mut world, FarStrangeAction::BuyTimeBubbles),
        PlayerScopedCommandOutcome::Applied
    );
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(rig.explosive_count(ExplosiveKind::TimeBubble), 3);
    assert!(rig.credits < credits);
    assert!(!ExplosiveKind::TimeBubble.counts_as_bomb());
}

#[test]
fn tick_mites_live_fast_and_crumble_young_at_every_tick_rate() {
    let life = |hz: u32| {
        let mut world = world_at(hz);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        let (id, _) = creature_arena(&mut world, EnemyKind::TickMite, 12);
        let mut seconds = 0;
        let mut crumbled = false;
        for count in 0..u64::from(hz) * 40 {
            let events = far_events(&ticks(&mut world, 1));
            if events.contains(&FarStrangeEvent::MiteCrumbled) {
                crumbled = true;
                seconds = count / u64::from(hz);
                break;
            }
            if world.entities().get(id).is_none() {
                break;
            }
        }
        assert!(crumbled, "the mite crumbled at {hz} Hz");
        seconds
    };
    let at20 = life(20);
    let at60 = life(60);
    assert!(
        at20.abs_diff(u64::from(TICK_MITE_LIFE_SECONDS)) <= 1,
        "{at20}"
    );
    assert!(at20.abs_diff(at60) <= 1, "{at20} vs {at60}");
    // It lives fast: it moves at more than its stat speed's worth of the world's seconds.
    assert!(crate::far_strange::creature_tempo(EnemyKind::TickMite) > 1.5);
    assert!(crate::far_strange::creature_tempo(EnemyKind::AmberSloth) < 1.0);
}

#[test]
fn an_amber_sloths_touch_holds_a_rig_in_amber() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let (_, _) = creature_arena(&mut world, EnemyKind::AmberSloth, 2);
    let mut ambered = false;
    for _ in 0..30 * 30 {
        let events = far_events(&ticks(&mut world, 1));
        if events.contains(&FarStrangeEvent::Ambered) {
            ambered = true;
            break;
        }
    }
    assert!(ambered, "the sloth reached the rig and ambered it");
    assert!(world.rig_ambered(PLAYER));
    // An ambered rig climbs slower than a free one.
    let climb = |world: &mut WorldState| {
        let (_, y0) = rig_xy(world);
        for _ in 0..10 {
            let _ = tick_with(world, ControlState::new(0.0, true, false));
        }
        y0 - rig_xy(world).1
    };
    let slow = climb(&mut world);
    let _ = ticks(&mut world, 30 * 3);
    assert!(!world.rig_ambered(PLAYER), "the amber wore off");
    let here = world.player(PLAYER).expect("rig").tile_in(world.frame());
    put_rig(&mut world, here);
    let free = climb(&mut world);
    assert!(slow < free * 0.75, "ambered {slow} vs free {free}");
}

#[test]
fn a_rewind_newt_rewinds_when_badly_hurt_but_not_inside_a_bubble() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let (id, _) = creature_arena(&mut world, EnemyKind::RewindNewt, 7);
    // Let it remember a few seconds (it walks toward the rig meanwhile).
    let _ = ticks(&mut world, 30 * 4);
    let (_, _, before) = enemy(&world, id).expect("newt");
    let max = before.max_health;
    let mut output = crate::TickOutput::default();
    let outcome = world.damage_enemy(
        id,
        Damage::new(max * 0.7, DamageKind::Kinetic, DamageSource::Player(PLAYER)),
        &mut output,
    );
    assert!(matches!(outcome, DamageOutcome::Hurt { .. }));
    let (_, _, after) = enemy(&world, id).expect("newt");
    assert!(
        after.health > max * 0.6,
        "it rewound its health ({} of {max})",
        after.health
    );
    assert_eq!(after.phase, 1);
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::FarStrange {
            what: FarStrangeEvent::Rewound,
            ..
        }
    )));
    // Inside a time bubble it cannot rewind.
    let (x, y, _) = enemy(&world, id).expect("newt");
    world.insert_charge(PlacedBomb {
        id: 9_002,
        x,
        y,
        previous_x: x,
        previous_y: y,
        kind: ExplosiveKind::TimeBubble,
        detonation_tick: world.simulation_tick().next(),
        ..PlacedBomb::default()
    });
    let _ = ticks(&mut world, 30 * 2);
    let (_, _, state) = enemy(&world, id).expect("newt");
    let mut output = crate::TickOutput::default();
    let _ = world.damage_enemy(
        id,
        Damage::new(
            max.mul_add(-0.2, state.health),
            DamageKind::Kinetic,
            DamageSource::Player(PLAYER),
        ),
        &mut output,
    );
    let (_, _, bubbled) = enemy(&world, id).expect("newt");
    assert!(
        bubbled.health < max * 0.3,
        "no rewind in a bubble ({})",
        bubbled.health
    );
    assert_eq!(bubbled.phase, 1);
}

// --- The Echo Caves --------------------------------------------------------------------------

fn echo_arena(world: &mut WorldState) -> TilePosition {
    arena(world, 260, middle_row(Stratum::EchoCaves), 20, 5)
}

#[test]
fn a_scanner_press_in_the_echo_caves_is_a_sonar_ping_that_lights_the_cave_for_a_moment() {
    let mut world = world_at(30);
    // Without the Sonar Suite: the stock ping.
    world
        .player_mut(PLAYER)
        .expect("rig")
        .hazard_gear
        .rig_systems &= !RigSystem::SonarSuite.bit();
    let floor = echo_arena(&mut world);
    // The lamp is a pinprick here.
    assert!(world.lamp_scale_for(PLAYER) < 0.3);
    let near = TilePosition {
        x: floor.x + 6,
        y: floor.y - 1,
    };
    assert!(world.echo_sonar_light_at(near) <= 0.0);
    let (_, events) = command(&mut world, PlayerCommand::UseScanner);
    assert!(far_events(&events).contains(&FarStrangeEvent::SonarPing));
    let ping = world.far_strange().pings.last().copied().expect("a ping");
    assert_eq!(ping.reach, SONAR_PING_TILES);
    let _ = ticks(&mut world, 9);
    assert!(
        world.echo_sonar_light_at(near) > 0.4,
        "the wave lit the cave"
    );
    let far = TilePosition {
        x: floor.x + 19,
        y: floor.y - 1,
    };
    assert!(
        world.echo_sonar_light_at(far) <= 0.0,
        "past the stock ping's reach"
    );
    // It fades, and the ping is forgotten.
    let _ = ticks(&mut world, 30 * 5);
    assert!(world.echo_sonar_light_at(near) <= 0.0);
    assert!(world.far_strange().pings.is_empty());
}

#[test]
fn the_sonar_suite_pings_further_and_a_hush_moth_swallows_pings() {
    let mut world = world_at(30);
    world
        .player_mut(PLAYER)
        .expect("rig")
        .hazard_gear
        .rig_systems |= RigSystem::SonarSuite.bit();
    let floor = echo_arena(&mut world);
    let _ = command(&mut world, PlayerCommand::UseScanner);
    assert_eq!(
        world.far_strange().pings.last().map(|ping| ping.reach),
        Some(SONAR_SUITE_PING_TILES)
    );
    let _ = ticks(&mut world, 30 * 6);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let (x, y) = world.frame().tile_center(TilePosition {
        x: floor.x + 1,
        y: floor.y - 2,
    });
    let _moth = world.spawn_enemy(EnemyKind::HushMoth, x, y);
    assert!(world.hushed(rig_xy(&world).0, rig_xy(&world).1));
    let _ = command(&mut world, PlayerCommand::UseScanner);
    let reach = world
        .far_strange()
        .pings
        .last()
        .map_or(0, |ping| ping.reach);
    assert!(
        reach < SONAR_SUITE_PING_TILES / 2,
        "hushed ping reach {reach}"
    );
    // And the hushed rig makes no noise.
    assert!(world.rig_noise(PLAYER) <= 0.0);
}

#[test]
fn blind_hunters_hear_a_drilling_rig_but_not_a_still_one() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let floor = echo_arena(&mut world);
    let (x, y) = world.frame().tile_center(TilePosition {
        x: floor.x + 7,
        y: floor.y - 1,
    });
    let stalker = world.spawn_enemy(EnemyKind::EchoStalker, x, y);
    // A still, quiet rig: it never notices.
    let _ = ticks(&mut world, 30 * 3);
    let (_, _, state) = enemy(&world, stalker).expect("stalker");
    assert_ne!(state.mode, EnemyMode::Chase, "a silent rig is unheard");
    assert!(world.rig_noise(PLAYER) <= 0.0);
    // Drilling: it hears and comes.
    let mut chased = false;
    for _ in 0..30 * 4 {
        let _ = tick_with(&mut world, ControlState::new(0.0, false, true));
        if enemy(&world, stalker).is_some_and(|(_, _, state)| state.target == Some(PLAYER)) {
            chased = true;
            break;
        }
    }
    assert!(chased, "the drilling rig was heard");
}

#[test]
fn a_hovering_rig_is_loud_its_thrusters_not_its_speed_make_the_noise() {
    let mut world = world_at(30);
    let _ = echo_arena(&mut world);
    let _ = ticks(&mut world, 30);
    assert!(world.rig_noise(PLAYER) <= 0.0, "a resting rig is silent");
    // Pressed against the cave's ceiling the rig does not move, but its thrusters roar.
    for _ in 0..30 * 3 {
        let _ = tick_with(&mut world, ControlState::new(0.0, true, false));
    }
    let player = world.player(PLAYER).expect("rig");
    assert!(
        player.velocity_x.hypot(player.velocity_y) < 30.0,
        "pinned under the ceiling"
    );
    let heard = world.rig_noise(PLAYER);
    assert!(heard >= 9.0, "thrust noise {heard}");
    // Let go: silent again once it settles.
    let _ = ticks(&mut world, 30 * 3);
    assert!(world.rig_noise(PLAYER) <= 0.0);
}

#[test]
fn a_shriekers_scream_pings_the_cave_and_calls_the_stalkers() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let floor = echo_arena(&mut world);
    let at = |dx: i32| {
        world.frame().tile_center(TilePosition {
            x: floor.x + dx,
            y: floor.y - 2,
        })
    };
    let (sx, sy) = at(3);
    let (kx, ky) = at(-16);
    let _shrieker = world.spawn_enemy(EnemyKind::Shrieker, sx, sy);
    let stalker = world.spawn_enemy(EnemyKind::EchoStalker, kx, ky);
    let mut shrieked = false;
    for _ in 0..30 * 6 {
        let events = far_events(&ticks(&mut world, 1));
        if events.contains(&FarStrangeEvent::Shriek) {
            shrieked = true;
            break;
        }
    }
    assert!(shrieked, "the shrieker found the rig and screamed");
    assert!(
        world
            .far_strange()
            .pings
            .iter()
            .any(|ping| ping.player.is_none())
    );
    let (_, _, state) = enemy(&world, stalker).expect("stalker");
    assert_eq!(state.target, Some(PLAYER), "the scream called the stalker");
}

#[test]
fn striking_echo_quartz_rings_it() {
    let mut world = world_at(30);
    let floor = echo_arena(&mut world);
    let pillar = TilePosition {
        x: floor.x + 3,
        y: floor.y,
    };
    fill(
        &mut world,
        TileRect {
            min: TilePosition {
                x: pillar.x,
                y: pillar.y - 4,
            },
            max: pillar,
        },
        TileKind::Cave(CaveTile::EchoQuartz),
    );
    let mut rang = false;
    for _ in 0..30 * 3 {
        let events = far_events(&tick_with(&mut world, ControlState::new(1.0, false, false)));
        if events.contains(&FarStrangeEvent::QuartzRang) {
            rang = true;
            break;
        }
    }
    assert!(rang, "driving into the pillar rang it");
}

// --- The Prism Desert ------------------------------------------------------------------------

fn prism_arena(world: &mut WorldState) -> TilePosition {
    arena(world, 240, middle_row(Stratum::PrismDesert), 14, 4)
}

#[test]
fn keyed_walls_open_only_to_their_own_colour_and_close_when_the_light_leaves() {
    let mut world = world_at(30);
    let floor = prism_arena(&mut world);
    let wall = TileRect {
        min: TilePosition {
            x: floor.x + 3,
            y: floor.y - 2,
        },
        max: TilePosition {
            x: floor.x + 3,
            y: floor.y,
        },
    };
    fill(&mut world, wall, TileKind::Cave(CaveTile::PrismRed));
    // Keyed walls are proof against drills and blasts.
    assert!(world.terrain().is_indestructible_at(wall.max));
    let _ = ticks(&mut world, 10);
    assert_eq!(
        world.terrain().tile(wall.max).map(|tile| tile.kind),
        Some(TileKind::Cave(CaveTile::PrismRed)),
        "white light opens nothing"
    );
    assert!(world.filter_swap_here(PLAYER));
    // Green: still shut. Red: open.
    assert_eq!(
        act(&mut world, FarStrangeAction::SetFilter(LampFilter::Green)),
        PlayerScopedCommandOutcome::Applied
    );
    let _ = ticks(&mut world, 5);
    assert!(
        world
            .terrain()
            .tile(wall.max)
            .is_some_and(|tile| !tile.kind.is_open())
    );
    let events = {
        let (_, mut events) = command(
            &mut world,
            PlayerCommand::Logistics(LogisticsCommand::FarStrange(FarStrangeAction::SetFilter(
                LampFilter::Red,
            ))),
        );
        events.extend(ticks(&mut world, 3));
        far_events(&events)
    };
    assert!(events.contains(&FarStrangeEvent::KeyedOpened));
    assert!(wall.positions().all(|tile| {
        world
            .terrain()
            .tile(tile)
            .is_some_and(|found| found.kind.is_open())
    }));
    // The light leaves (white again): they close after a while.
    let _ = act(&mut world, FarStrangeAction::CycleFilter);
    let _ = act(&mut world, FarStrangeAction::CycleFilter);
    let _ = act(&mut world, FarStrangeAction::CycleFilter);
    assert_eq!(world.lamp_filter_of(PLAYER), LampFilter::White);
    let _ = ticks(&mut world, 30 * PRISM_CLOSE_MILLIS / 1_000 + 30);
    assert!(
        wall.positions()
            .all(|tile| world.terrain().tile(tile).map(|found| found.kind)
                == Some(TileKind::Cave(CaveTile::PrismRed)))
    );
}

#[test]
fn a_keyed_wall_never_closes_on_a_rig_inside_it() {
    let mut world = world_at(30);
    let floor = prism_arena(&mut world);
    let spot = TilePosition {
        x: floor.x + 2,
        y: floor.y,
    };
    fill(
        &mut world,
        TileRect {
            min: TilePosition {
                x: spot.x,
                y: spot.y - 1,
            },
            max: spot,
        },
        TileKind::Cave(CaveTile::PrismBlue),
    );
    world.set_lamp_filter(PLAYER, LampFilter::Blue);
    let _ = ticks(&mut world, 3);
    // Drive into the opened gap, then turn the light off while inside it.
    put_rig(&mut world, spot);
    world.set_lamp_filter(PLAYER, LampFilter::White);
    let _ = ticks(&mut world, 30 * 5);
    assert!(
        world
            .terrain()
            .tile(spot)
            .is_some_and(|tile| tile.kind.is_open())
    );
    assert!(world.far_schedule().open_keyed.contains_key(&spot));
}

#[test]
fn a_prism_scarab_cracks_only_in_its_shells_colour_of_light() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let floor = prism_arena(&mut world);
    let (x, y) = world.frame().tile_center(TilePosition {
        x: floor.x + 10,
        y: floor.y - 1,
    });
    let scarab = world.spawn_enemy(EnemyKind::PrismScarab, x, y);
    let shell = scarab_shell(scarab.get(), world.millis_at(world.simulation_tick()));
    let wrong = LampFilter::KEYED
        .into_iter()
        .find(|filter| *filter != shell)
        .expect("another colour");
    let hit = |world: &mut WorldState| {
        let mut output = crate::TickOutput::default();
        match world.damage_enemy(
            scarab,
            Damage::new(10.0, DamageKind::Kinetic, DamageSource::Player(PLAYER)),
            &mut output,
        ) {
            DamageOutcome::Hurt { dealt } | DamageOutcome::Killed { dealt } => dealt,
            DamageOutcome::Ignored => 0.0,
        }
    };
    world.set_lamp_filter(PLAYER, wrong);
    let unmatched = hit(&mut world);
    world.set_lamp_filter(PLAYER, shell);
    let matched = hit(&mut world);
    assert!(
        matched > unmatched * 4.0,
        "matched {matched} vs {unmatched}"
    );
}

#[test]
fn a_spires_bolt_of_the_lamps_colour_passes_through_the_rig() {
    // The fan is one bolt of each colour.
    let colours: Vec<_> = (0..3)
        .map(|index| super::enemies::prism_shot(index).prism_colour())
        .collect();
    assert_eq!(
        colours,
        vec![
            Some(LampFilter::Red),
            Some(LampFilter::Green),
            Some(LampFilter::Blue)
        ]
    );
    let run = |filter: LampFilter| {
        let mut world = world_at(30);
        // A hostile world (Peaceful clears every enemy shot).
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        world.set_lamp_filter(PLAYER, filter);
        let floor = prism_arena(&mut world);
        let hull = world.player(PLAYER).expect("rig").hull;
        let (x, y) = rig_xy(&world);
        let _ = world.spawn_enemy_shot_for_tests(
            crate::enemy::ShotKind::PrismRed,
            (x - 60.0, y),
            (230.0, 0.0),
        );
        let _ = floor;
        let _ = ticks(&mut world, 30);
        hull - world.player(PLAYER).expect("rig").hull
    };
    assert!(
        run(LampFilter::White) > 0.0,
        "a red bolt hits a white-lit rig"
    );
    assert!(
        run(LampFilter::Red) <= 0.0,
        "it passes through a red-lit one"
    );
}

#[test]
fn a_lens_wraith_is_unseen_outside_its_own_colour_of_light() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let floor = prism_arena(&mut world);
    let (x, y) = world.frame().tile_center(TilePosition {
        x: floor.x + 4,
        y: floor.y - 2,
    });
    let wraith = world.spawn_enemy(EnemyKind::LensWraith, x, y);
    let state = enemy(&world, wraith).expect("wraith").2;
    let colour = WorldState::lens_wraith_colour(wraith.get());
    world.set_lamp_filter(PLAYER, LampFilter::White);
    assert!(world.far_unseen_by(PLAYER, wraith, &state));
    world.set_lamp_filter(PLAYER, colour);
    assert!(!world.far_unseen_by(PLAYER, wraith, &state));
}

#[test]
fn reaching_a_far_landmark_tells_its_story_beat_once() {
    let mut world = world_at(30);
    let settings = settings();
    let layout = layout(&settings);
    let hall = sites::clock_hall(&settings, &layout).expect("the clock hall");
    put_rig(
        &mut world,
        TilePosition {
            x: hall.center().x,
            y: hall.max.y - 2,
        },
    );
    let events = far_events(&ticks(&mut world, 3));
    assert_eq!(
        events
            .iter()
            .filter(|event| **event == FarStrangeEvent::LandmarkFound)
            .count(),
        1
    );
    assert!(world.far_strange().found.contains(&FarLandmark::ClockHall));
    assert_eq!(
        world.far_landmark_at(TilePosition {
            x: hall.center().x,
            y: hall.max.y - 2
        }),
        Some(FarLandmark::ClockHall)
    );
    assert!(!far_events(&ticks(&mut world, 3)).contains(&FarStrangeEvent::LandmarkFound));
}

// --- Builder alloy and the Phase Drill --------------------------------------------------------

#[test]
fn builder_alloy_seals_descent_vii_and_yields_only_to_the_phase_drill() {
    let mut world = world_at(30);
    let settings = settings();
    let layout = layout(&settings);
    // The seal is generated under the Prism Desert.
    let x = 320;
    let top = (layout.rows(Stratum::PrismDesert).1 - 60..layout.rows(Stratum::PrismDesert).1 + 80)
        .find(|y| sites::alloy_seal(&settings, &layout, x, *y))
        .expect("the seal");
    put_rig(&mut world, TilePosition { x, y: top - 1 });
    fill(
        &mut world,
        TileRect::around(TilePosition { x, y: top - 2 }, 1, 1),
        TileKind::Air,
    );
    put_rig(&mut world, TilePosition { x, y: top - 1 });
    assert_eq!(
        world
            .terrain()
            .tile(TilePosition { x, y: top })
            .map(|tile| tile.kind),
        Some(TileKind::Cave(CaveTile::BuilderAlloy))
    );
    // No Phase Drill: the drill refuses (and test mode is off).
    for _ in 0..30 * 4 {
        let _ = tick_with(&mut world, ControlState::new(0.0, false, true));
    }
    assert_eq!(
        world
            .terrain()
            .tile(TilePosition { x, y: top })
            .map(|tile| tile.kind),
        Some(TileKind::Cave(CaveTile::BuilderAlloy)),
        "the alloy held"
    );
    // A blast does not break it either.
    let blast = world
        .terrain_mut()
        .blast_radius(TilePosition { x, y: top }, 2);
    assert!(
        !blast
            .broken
            .iter()
            .any(|(_, kind)| *kind == TileKind::Cave(CaveTile::BuilderAlloy))
    );
    // With the Phase Drill it cuts through.
    put_rig(&mut world, TilePosition { x, y: top - 1 });
    world
        .player_mut(PLAYER)
        .expect("rig")
        .hazard_gear
        .rig_systems |= RigSystem::PhaseDrill.bit();
    let mut cut = false;
    for _ in 0..30 * 20 {
        let _ = tick_with(&mut world, ControlState::new(0.0, false, true));
        if world
            .terrain()
            .tile(TilePosition { x, y: top })
            .is_some_and(|tile| tile.kind.is_open())
        {
            cut = true;
            break;
        }
    }
    assert!(cut, "the Phase Drill cut the alloy");
}

#[test]
fn the_phase_drill_is_won_from_the_glitch_never_sold() {
    assert_eq!(
        RigSystem::PhaseDrill.boss_reward(),
        Some(crate::descent::DescentBoss::TheGlitch)
    );
    let gate = Stratum::PrismDesert.def().gate.expect("the gate");
    assert_eq!(gate.boss, crate::descent::DescentBoss::TheGlitch);
}

// --- The Glitch -------------------------------------------------------------------------------

/// A rig's hull tier at The Glitch (the descent probe's arrival tier in the Prism Desert, plus
/// one it can afford there).
const HULL_AT_THE_GLITCH: u8 = 13;

/// The bot's controls: keep its distance from The Glitch and aim the railgun at it.
fn glitch_bot(world: &WorldState, target: (f32, f32), tick: u64) -> ControlState {
    let rig = world.player(PLAYER).expect("rig");
    let (dx, dy) = (target.0 - rig.x, target.1 - rig.y);
    let length = dx.hypot(dy).max(1.0);
    #[allow(clippy::cast_possible_truncation, reason = "a unit vector times 127")]
    let aim = ((dx / length * 127.0) as i8, (dy / length * 127.0) as i8);
    let fields = world.fields_at(rig.x, rig.y);
    // Mirrored controls: the bot reads the rule and steers (and, since the distortion strata
    // mirror the aim with the screen, aims) the other way.
    let mirrored = world.rig_controls_mirrored(PLAYER);
    // The rotation thrusters, worked by hand like a player: keep the rig's feet towards
    // whatever way gravity points now (the rules turn it), so the main thruster fights it.
    let level = fields.down_angle().unwrap_or(0.0);
    let lean = crate::field::trig::wrap_angle(rig.attitude.orientation - level);
    let mut rotate = lean
        .mul_add(-3.0, -rig.attitude.angular_velocity * 0.6)
        .clamp(-1.0, 1.0);
    // The stick drives along gravity's own horizontal.
    let (sin_level, cos_level) = (level.sin(), level.cos());
    let along = dx.mul_add(cos_level, dy * sin_level);
    let mut strafe = if length < TILE_SIZE * 6.0 {
        -along.signum()
    } else if length > TILE_SIZE * 14.0 {
        along.signum()
    } else if (tick / 50).is_multiple_of(2) {
        0.5
    } else {
        -0.5
    };
    if mirrored {
        strafe = -strafe;
        rotate = -rotate;
    }
    let thrust = tick % 50 < 5;
    ControlState::new(strafe, thrust, false)
        .with_rotation(rotate)
        .with_weapon(WeaponControl {
            fire: tick % 6 < 3,
            auto_aim: false,
            aim_x: if mirrored {
                aim.0.saturating_neg()
            } else {
                aim.0
            },
            aim_y: aim.1,
        })
}

#[test]
#[allow(clippy::too_many_lines, reason = "the whole fight, start to end")]
fn a_bot_with_descent_vi_kit_beats_the_glitch_through_every_rule_and_wins_the_phase_drill() {
    let hz = 30;
    let mut world = world_at(hz);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let arena = world.glitch_arena().expect("the arena");
    let inside = sites::arena_inside(&arena);
    put_rig(
        &mut world,
        TilePosition {
            x: inside.min.x + 6,
            y: inside.max.y - 1,
        },
    );
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        crate::economy::max_out_upgrades(rig);
        rig.hazard_gear.rig_systems = descent_six_systems();
        rig.weapon_rack.battery_level = crate::weapon::MAX_BATTERY_LEVEL;
        let _ = rig.receive_weapon(WeaponKind::Railgun);
        let _ = rig.mount_weapon(Some(WeaponKind::Railgun));
        rig.hull_level = HULL_AT_THE_GLITCH;
        rig.hull = rig.max_hull();
    }
    let events = far_events(&ticks(&mut world, 2));
    assert!(events.contains(&FarStrangeEvent::GlitchAwoke));
    assert!(world.far_strange().glitch.awake());
    let mut rules = vec![world.far_strange().glitch.rule];
    let mut rewrite_started: Option<u64> = None;
    let mut telegraphs = Vec::new();
    let mut ignored_mid_rewrite = false;
    let mut felt_up = false;
    let mut felt_sideways = false;
    let mut felt_mirror = false;
    let mut felt_warp = false;
    let mut blinked = false;
    let mut defeated = false;
    for count in 0..u64::from(hz) * 900 {
        let Some(target) = world
            .far_strange()
            .glitch
            .entity
            .and_then(|id| world.entities().get(EntityId(id)))
            .map(|entity| (entity.body.x, entity.body.y))
        else {
            break;
        };
        let controls = glitch_bot(&world, target, count);
        let output =
            world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
        let events: Vec<GameplayEventKind> =
            output.events.into_iter().map(|event| event.kind).collect();
        let far = far_events(&events);
        blinked |= far.contains(&FarStrangeEvent::GlitchBlinked);
        if far.contains(&FarStrangeEvent::GlitchRewriting) {
            rewrite_started = Some(count);
            // Mid-rewrite it cannot be hurt.
            let id = EntityId(world.far_strange().glitch.entity.expect("glitch"));
            let mut output = crate::TickOutput::default();
            ignored_mid_rewrite |= world.damage_enemy(
                id,
                Damage::new(50.0, DamageKind::Piercing, DamageSource::Player(PLAYER)),
                &mut output,
            ) == DamageOutcome::Ignored;
        }
        if far.contains(&FarStrangeEvent::GlitchRewrote) {
            if let Some(start) = rewrite_started.take() {
                telegraphs.push(count - start);
            }
            rules.push(world.far_strange().glitch.rule);
        }
        let (rx, ry) = rig_xy(&world);
        let fields = world.fields_at(rx, ry);
        felt_up |= fields.gravity_y < -0.5;
        felt_sideways |= fields.gravity_x > 0.5;
        felt_mirror |= fields.mirror > 0.5;
        felt_warp |= fields.time_scale > 1.3;
        if far.contains(&FarStrangeEvent::GlitchDefeated) {
            defeated = true;
            break;
        }
        assert!(
            world.player(PLAYER).expect("rig").hull > 0.0,
            "the bot survived (rules {rules:?})"
        );
    }
    assert!(defeated, "the bot beat The Glitch (rules seen {rules:?})");
    assert_eq!(rules, GlitchRule::PHASES.to_vec(), "every rule in order");
    assert!(ignored_mid_rewrite, "rewrites shrug off hits");
    // Every rewrite was telegraphed for the full warning.
    let warning = u64::from(hz) * GLITCH_REWRITE_MILLIS / 1_000;
    assert_eq!(telegraphs.len(), 4);
    assert!(
        telegraphs.iter().all(|ticks| ticks.abs_diff(warning) <= 1),
        "{telegraphs:?}"
    );
    assert!(
        felt_up && felt_mirror && felt_warp && felt_sideways,
        "the rig felt every rule"
    );
    assert!(blinked, "it blinked across its hall");
    let left =
        world.player(PLAYER).expect("rig").hull / world.player(PLAYER).expect("rig").max_hull();
    assert!(
        left > 0.2,
        "with a margin: {:.0} % of the hull left",
        left * 100.0
    );
    assert!(world.far_strange().glitch.beaten);
    assert!(
        world
            .player(PLAYER)
            .expect("rig")
            .hazard_gear
            .has_system(RigSystem::PhaseDrill)
    );
    // The arena's rules are its own again.
    assert!(world.far_strange().glitch.zone.is_none());
    let (rx, ry) = rig_xy(&world);
    assert!((world.fields_at(rx, ry).gravity_y - 1.0).abs() < 0.01);
    // It stays beaten.
    let _ = ticks(&mut world, 30);
    assert!(!world.far_strange().glitch.awake());
}

#[test]
fn every_rule_holds_at_full_strength_against_the_halls_floor_and_gravity_up_lifts_the_rig() {
    let mut world = world_at(60);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let arena = world.glitch_arena().expect("the arena");
    let inside = sites::arena_inside(&arena);
    let floor = TilePosition {
        x: inside.center().x - 9,
        y: inside.max.y,
    };
    put_rig(&mut world, floor);
    let _ = ticks(&mut world, 60);
    assert!(world.far_strange().glitch.awake());
    let at_floor = |world: &WorldState| {
        let (x, y) = world.frame().tile_center(floor);
        world.fields_at(x, y)
    };
    world.stage_glitch_rule(GlitchRule::Mirrored, false);
    assert!(at_floor(&world).mirror > 0.99);
    world.stage_glitch_rule(GlitchRule::TimeWarp, false);
    assert!((at_floor(&world).time_scale - crate::far_strange::GLITCH_TIME_WARP).abs() < 0.01);
    world.stage_glitch_rule(GlitchRule::Sideways, false);
    assert!(at_floor(&world).gravity_x > 0.99);
    world.stage_glitch_rule(GlitchRule::GravityUp, false);
    assert!(at_floor(&world).gravity_y < -0.99);
    // A rig resting on the floor falls up to the ceiling.
    let (_, start) = rig_xy(&world);
    let _ = ticks(&mut world, 60 * 3);
    let (_, end) = rig_xy(&world);
    let rows = (start - end) / TILE_SIZE;
    assert!(
        rows > f32::from(u8::try_from(inside.height() - 3).unwrap_or(0)),
        "fell up {rows} rows"
    );
}

#[test]
fn on_a_peaceful_world_the_glitch_yields_its_gate_and_a_lonely_hall_sleeps() {
    let mut world = world_at(30);
    let arena = world.glitch_arena().expect("the arena");
    let inside = sites::arena_inside(&arena);
    put_rig(
        &mut world,
        TilePosition {
            x: inside.center().x,
            y: inside.max.y - 1,
        },
    );
    let events = far_events(&ticks(&mut world, 3));
    assert!(events.contains(&FarStrangeEvent::GlitchDefeated));
    assert!(
        world
            .player(PLAYER)
            .expect("rig")
            .hazard_gear
            .has_system(RigSystem::PhaseDrill)
    );

    // A hostile world: it wakes, then sleeps (and heals) once the hall is empty.
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    put_rig(
        &mut world,
        TilePosition {
            x: inside.center().x,
            y: inside.max.y - 1,
        },
    );
    let _ = ticks(&mut world, 3);
    assert!(world.far_strange().glitch.awake());
    put_rig(
        &mut world,
        TilePosition {
            x: arena.max.x + 60,
            y: inside.max.y,
        },
    );
    let _ = ticks(&mut world, 30 * 32);
    assert!(!world.far_strange().glitch.awake());
    assert!(!world.far_strange().glitch.beaten);
}

// --- State, formats, determinism ---------------------------------------------------------------

#[test]
fn far_state_saves_replicates_hashes_and_leaves_older_worlds_alone() {
    let mut world = world_at(30);
    let classic_hash = {
        let classic = WorldState::new_game_with_settings(
            PLAYER,
            &WorldSettings::new(SEED),
            SimulationParams::new(30),
        );
        assert!(classic.far_strange().is_empty());
        classic.simulation_state_hash()
    };
    let before = world.simulation_state_hash();
    world.set_lamp_filter(PLAYER, LampFilter::Green);
    world.stage_sonar_ping(Some(PLAYER), TilePosition { x: 0, y: 0 }, 9);
    assert_ne!(
        world.simulation_state_hash(),
        before,
        "the far state is hashed once it holds anything"
    );
    // Saves round-trip.
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.far_strange(), world.far_strange());
    // Clients receive it as a shared section.
    assert!(crate::world::SectionKey::SHARED.contains(&crate::world::SectionKey::FarStrange));
    let mut client =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(30));
    let payload = world
        .section(crate::world::SectionKey::FarStrange)
        .expect("section");
    client.apply_section(crate::world::SectionKey::FarStrange, payload);
    assert_eq!(client.lamp_filter_of(PLAYER), LampFilter::Green);
    // An older world's hash is untouched by the new (empty) state.
    let again = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(SEED),
        SimulationParams::new(30),
    );
    assert_eq!(again.simulation_state_hash(), classic_hash);
}

#[test]
fn the_far_strange_layers_are_deterministic() {
    let run = || {
        let mut world = world_at(30);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        let floor = echo_arena(&mut world);
        let (x, y) = world.frame().tile_center(TilePosition {
            x: floor.x + 9,
            y: floor.y - 1,
        });
        let _ = world.spawn_enemy(EnemyKind::EchoStalker, x, y);
        let _ = world.spawn_enemy(EnemyKind::TickMite, x - 40.0, y);
        let _ = command(&mut world, PlayerCommand::UseScanner);
        for count in 0..300_u64 {
            let _ = tick_with(
                &mut world,
                ControlState::new(
                    if count % 60 < 30 { 0.5 } else { -0.5 },
                    false,
                    count % 90 < 20,
                ),
            );
        }
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
#[allow(clippy::too_many_lines, reason = "the arena, the spawns and the fight")]
fn the_far_strange_creatures_keep_the_body_rules() {
    use crate::{enemy::Locomotion, physics};
    let hz = 30;
    let mut world = world_at(hz);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let row = middle_row(Stratum::PrismDesert);
    let (left, right, top, floor) = (140, 220, row - 10, row + 8);
    put_rig(
        &mut world,
        TilePosition {
            x: 180,
            y: floor - 1,
        },
    );
    let _ = ticks(&mut world, 2);
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
    // A pillar to dodge round.
    fill(
        &mut world,
        TileRect {
            min: TilePosition {
                x: 160,
                y: floor - 6,
            },
            max: TilePosition {
                x: 161,
                y: floor - 1,
            },
        },
        TileKind::Stone,
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
    // A time bubble in the middle of the fight: slowed creatures keep the rules too.
    let (bx, by) = world.frame().tile_center(TilePosition {
        x: 170,
        y: floor - 3,
    });
    world.insert_charge(PlacedBomb {
        id: 9_100,
        x: bx,
        y: by,
        previous_x: bx,
        previous_y: by,
        kind: ExplosiveKind::TimeBubble,
        detonation_tick: world.simulation_tick().next(),
        ..PlacedBomb::default()
    });
    let frame = world.frame();
    let kinds: Vec<EnemyKind> = EnemyKind::ALL
        .into_iter()
        .filter(|kind| kind.is_far_strange())
        .collect();
    for (index, kind) in (0_i32..).zip(kinds) {
        let column = left + 8 + (index * 6) % (right - left - 12);
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
        // Keep the rig in the fight.
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = rig.max_hull();
    }
    assert!(checked > 1_000, "a real fight: {checked}");
}
