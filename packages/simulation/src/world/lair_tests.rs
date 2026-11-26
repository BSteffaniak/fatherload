//! The Wave 12 worlds' theme bosses in play (Wave 14 finish, `crate::planet::lairs`): each wakes
//! when a rig enters its hall, tells its volleys, calls its brood, falls and pays (seal spent,
//! vault open, the hunt list's quarry down, never again), rests whole when the crew leaves, a
//! Peaceful lair yields; and a bot with a late rig beats every one of them for real.

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WorldState,
    enemy::{Damage, DamageKind, DamageSource, EnemyDifficulty, EnemyKind, EnemyMode},
    event::TickOutput,
    planet::{
        PlanetParams, PlanetThemeId,
        lairs::{LAIR_BOSSES, Lair, LairEvent, lair_of},
        outlands::OutlandTile,
    },
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);

fn world(theme: PlanetThemeId, hz: u32) -> WorldState {
    WorldState::new_planet_world(
        PLAYER,
        PlanetParams::new(0x5EED_0014, 1_600, theme),
        SimulationParams::new(hz),
    )
}

fn step(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let tick = world.simulation_tick();
    world
        .simulate_tick(tick, &TickInputs::held(PLAYER, controls))
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn lair_events(events: &[GameplayEventKind]) -> Vec<LairEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Lair { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

fn run(world: &mut WorldState, seconds: u32) -> Vec<LairEvent> {
    let hz = world.timing().rate.hz();
    let mut out = Vec::new();
    for _ in 0..hz * seconds {
        out.extend(lair_events(&step(
            world,
            ControlState::new(0.0, false, false),
        )));
    }
    out
}

fn place_rig(world: &mut WorldState, column: i32, row: i32) {
    let (x, y) = world.frame().centre_of(column, row);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.x = x;
        rig.y = y;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
    world.stream_terrain_near_players();
}

fn kind(world: &WorldState, column: i32, row: i32) -> TileKind {
    world
        .terrain()
        .tile(TilePosition { x: column, y: row })
        .expect("loaded")
        .kind
}

/// Puts the rig on the hall's floor ten tiles east of its middle; the lair.
fn into_the_lair(world: &mut WorldState) -> Lair {
    let planet = *world.planet().expect("planet");
    let lair = lair_of(&planet).expect("a lair");
    let (middle, _) = lair.middle();
    let column = i32::try_from(middle.rem_euclid(i64::from(planet.strip_width()))).unwrap_or(0);
    place_rig(world, column + 10, lair.floor - 1);
    lair
}

fn themes() -> impl Iterator<Item = PlanetThemeId> {
    LAIR_BOSSES.iter().map(|boss| boss.theme)
}

#[test]
fn each_wave_12_boss_wakes_calls_its_brood_falls_and_pays() {
    for theme in themes() {
        let mut world = world(theme, 30);
        let lair = into_the_lair(&mut world);
        let boss = lair.boss.kind;
        assert_eq!(world.wild_boss_beaten_here(), Some(false));
        let events = run(&mut world, 2);
        assert!(
            events.contains(&LairEvent::BossWoke),
            "{theme:?}: {events:?}"
        );
        let id = world
            .enemies()
            .find(|(_, state)| state.kind == boss)
            .map(|(entity, _)| entity.id)
            .expect("the boss is up");
        // It tells before it strikes: a wind-up pose, then the volley (the test rig is
        // invulnerable while we watch).
        world.enable_test_mode();
        let hz = world.timing().rate.hz();
        let (mut wound_up, mut summoned) = (false, false);
        for _ in 0..hz * 16 {
            summoned |= run_tick(&mut world).contains(&LairEvent::BossSummons);
            wound_up |= world
                .enemies()
                .any(|(_, state)| state.kind == boss && state.mode == EnemyMode::Windup);
        }
        assert!(wound_up, "{theme:?}: no tell");
        assert!(summoned, "{theme:?}: no warning before the brood");
        let brood = match boss {
            EnemyKind::BrambleKing => EnemyKind::Mossprowler,
            EnemyKind::Tidecaller => EnemyKind::Tidesnapper,
            EnemyKind::HoarfrostWyrm => EnemyKind::Rimewolf,
            EnemyKind::DuneTyrant => EnemyKind::GlassScorpion,
            EnemyKind::MagmaDrake => EnemyKind::CinderBat,
            _ => EnemyKind::ShardCrab,
        };
        assert!(
            world.enemies().any(|(_, state)| state.kind == brood),
            "{theme:?}: no {brood:?}"
        );
        // Nothing else comes into the hall.
        assert!(
            world
                .enemies()
                .all(|(_, state)| state.kind == boss || state.kind == brood),
            "{theme:?}: {:?}",
            world
                .enemies()
                .map(|(_, state)| state.kind)
                .collect::<Vec<_>>()
        );
        // Its fall spends the seal, opens the vault, names the boss and downs the quarry.
        let mut output = TickOutput::default();
        let _ = world.damage_enemy(
            id,
            Damage::new(1.0e9, DamageKind::Explosive, DamageSource::Player(PLAYER)),
            &mut output,
        );
        assert!(
            output.events.iter().any(|event| matches!(
                event.kind,
                GameplayEventKind::Lair {
                    what: LairEvent::BossDefeated,
                    boss: fell,
                    player_id: Some(PLAYER),
                    ..
                } if fell == boss
            )),
            "{theme:?}"
        );
        assert_eq!(world.wild_boss_beaten_here(), Some(true));
        let quarry = crate::endgame::hunt::Quarry::Theme(theme.0);
        assert!(world.quarry_beaten(quarry), "{theme:?}");
        assert!(output.events.iter().any(|event| matches!(
            event.kind,
            GameplayEventKind::Endgame {
                what: crate::endgame::EndgameEvent::QuarryDown,
                detail,
                ..
            } if detail == quarry.code()
        )));
        for (x, y) in lair.door {
            assert!(kind(&world, x, y).is_open(), "{theme:?}: the door opened");
        }
        assert_eq!(
            kind(&world, lair.seal.0, lair.seal.1),
            TileKind::Outland(OutlandTile::BossSealSpent)
        );
        assert_eq!(
            kind(&world, lair.relic.0, lair.relic.1),
            TileKind::Lair(lair.boss.relic)
        );
        // It never wakes again.
        let _ = run(&mut world, 2);
        assert!(
            !world.enemies().any(|(_, state)| state.kind == boss),
            "{theme:?}"
        );
    }
}

fn run_tick(world: &mut WorldState) -> Vec<LairEvent> {
    lair_events(&step(world, ControlState::new(0.0, false, false)))
}

#[test]
fn a_wave_12_boss_rests_whole_when_the_crew_leaves_and_a_peaceful_lair_yields() {
    let mut world = world(PlanetThemeId::JUNGLE, 30);
    let lair = into_the_lair(&mut world);
    let _ = run(&mut world, 2);
    assert!(
        world
            .enemies()
            .any(|(_, state)| state.kind == lair.boss.kind)
    );
    // Far off up the shaft and away: it rests (no countdown, nothing lost).
    let planet = *world.planet().expect("planet");
    let away =
        i32::try_from((lair.centre + 200).rem_euclid(i64::from(planet.strip_width()))).unwrap_or(0);
    let surface = crate::planet::BodyGround::surface_row(&planet, away);
    place_rig(&mut world, away, surface - 2);
    let _ = run(&mut world, 2);
    assert!(
        !world
            .enemies()
            .any(|(_, state)| state.kind == lair.boss.kind)
    );
    assert_eq!(world.wild_boss_beaten_here(), Some(false));
    // Peaceful: no fight, the lair yields its vault.
    let mut world = world_peaceful();
    let events = run(&mut world, 2);
    assert!(events.contains(&LairEvent::BossDefeated), "{events:?}");
    assert_eq!(world.wild_boss_beaten_here(), Some(true));
    assert!(world.quarry_beaten(crate::endgame::hunt::Quarry::Theme(PlanetThemeId::OCEAN.0)));
}

fn world_peaceful() -> WorldState {
    let mut world = world(PlanetThemeId::OCEAN, 30);
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let _ = into_the_lair(&mut world);
    world
}

/// The bot's controls in a lair (the outland lair bot's tactics): a stand a dozen tiles off
/// the boss, weaving over a 3 s beat, shooting the boss first, then the nearest of its brood.
fn lair_bot(world: &WorldState, lair: &Lair, tick: u32) -> ControlState {
    use crate::WeaponControl;
    let rig = world.player(PLAYER).expect("rig");
    let frame = world.frame();
    let width = i64::from(world.planet().expect("planet").strip_width());
    let (middle, _) = lair.middle();
    let middle = i32::try_from(middle.rem_euclid(width)).unwrap_or(0);
    let (mx, _) = frame.centre_of(middle, lair.floor - 1);
    let boss = world
        .enemies()
        .find(|(_, state)| state.kind == lair.boss.kind)
        .map(|(entity, _)| (entity.body.x, entity.body.y));
    let target = boss.or_else(|| {
        world
            .enemies()
            .map(|(entity, _)| (entity.body.x, entity.body.y))
            .min_by(|a, b| {
                (a.0 - rig.x)
                    .hypot(a.1 - rig.y)
                    .total_cmp(&(b.0 - rig.x).hypot(b.1 - rig.y))
            })
    });
    let anchor = boss.map_or(mx, |(bx, _)| bx);
    let side: f32 = if rig.x < anchor { -1.0 } else { 1.0 };
    let weave: f32 = if (tick / 45).is_multiple_of(2) {
        -1.0
    } else {
        1.0
    };
    let goal = side
        .mul_add(12.0, weave * 3.0)
        .mul_add(TILE_SIZE, anchor)
        .clamp(TILE_SIZE.mul_add(-23.0, mx), TILE_SIZE.mul_add(23.0, mx));
    let dx = goal - rig.x;
    let horizontal = if dx.abs() > TILE_SIZE * 0.5 {
        (dx / (TILE_SIZE * 2.0)).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let aim = target.map_or((0, -127), |(tx, ty)| {
        let (ax, ay) = (tx - rig.x, ty - rig.y);
        let length = ax.hypot(ay).max(1.0);
        #[allow(clippy::cast_possible_truncation, reason = "a unit vector times 127")]
        ((ax / length * 127.0) as i8, (ay / length * 127.0) as i8)
    });
    ControlState::new(horizontal, false, false).with_weapon(WeaponControl {
        fire: target.is_some() && tick % 6 < 3,
        auto_aim: false,
        aim_x: aim.0,
        aim_y: aim.1,
    })
}

/// Fair and winnable, measured: a bot with a late-game rig (every upgrade, a railgun) fights
/// each Wave 12 theme boss for real on Normal, without test mode or repairs, and wins inside
/// five minutes with its rig whole; each fight is long enough for the brood and hurts.
#[test]
fn a_bot_with_late_kit_beats_each_wave_12_boss_for_real() {
    use crate::weapon::WeaponKind;
    for theme in themes() {
        let hz = 30;
        let mut world = world(theme, hz);
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        let lair = into_the_lair(&mut world);
        {
            let rig = world.player_mut(PLAYER).expect("rig");
            crate::economy::max_out_upgrades(rig);
            rig.weapon_rack.battery_level = crate::weapon::MAX_BATTERY_LEVEL;
            let _ = rig.receive_weapon(WeaponKind::Railgun);
            let _ = rig.mount_weapon(Some(WeaponKind::Railgun));
            rig.hull = rig.max_hull();
        }
        let mut won = false;
        let (mut summons, mut least_hull, mut seconds) = (0, f32::MAX, 0);
        for tick in 0..hz * 300 {
            let controls = lair_bot(&world, &lair, tick);
            for what in lair_events(&step(&mut world, controls)) {
                match what {
                    LairEvent::BossSummons => summons += 1,
                    LairEvent::BossDefeated => won = true,
                    _ => {}
                }
            }
            let rig = world.player_mut(PLAYER).expect("rig");
            least_hull = least_hull.min(rig.hull / rig.max_hull());
            assert!(
                rig.hull > 0.0,
                "{theme:?}: the bot's rig was wrecked after {}s",
                tick / hz
            );
            rig.fuel = rig.fuel_capacity;
            if won {
                seconds = tick / hz;
                break;
            }
        }
        assert!(won, "{theme:?}: the bot beat the boss inside five minutes");
        assert_eq!(world.wild_boss_beaten_here(), Some(true));
        assert!(seconds >= 10, "{theme:?}: over in {seconds}s");
        assert!(summons >= 1, "{theme:?}: the brood came");
        assert!(
            least_hull < 0.98,
            "{theme:?}: the boss never touched the rig"
        );
        eprintln!("{theme:?}: won in {seconds}s, hull down to {least_hull:.2}, {summons} calls");
    }
}

/// Every Wave 12 boss is on the hunt list, and its waypoint is a world of its kind with its
/// lair.
#[test]
fn every_wave_12_boss_is_on_the_hunt_list_with_a_waypoint_to_its_lair() {
    let world = WorldState::new_game(PLAYER);
    let seed = world.world_seed();
    let hunt = world.hunt();
    for boss in LAIR_BOSSES {
        let quarry = crate::endgame::hunt::Quarry::Theme(boss.theme.0);
        let (_, beaten, waypoint) = hunt
            .iter()
            .find(|(entry, ..)| *entry == quarry)
            .unwrap_or_else(|| panic!("{} is not on the hunt list", boss.name));
        assert!(!beaten);
        assert!(waypoint.is_some(), "{}: no waypoint", boss.name);
        assert_eq!(
            crate::endgame::hunt::theme_boss_of(boss.kind),
            Some(boss.theme.0)
        );
        let theme = crate::endgame::hunt::THEME_BOSSES
            .iter()
            .find(|entry| entry.theme == boss.theme.0)
            .expect("registered");
        let body = crate::endgame::hunt::nearest_theme_world(seed, theme.world).expect("a world");
        let params = crate::place::planet_params_for(seed, body).expect("landable");
        assert_eq!(params.theme, boss.theme);
        let ground = crate::planet::PlanetGround::new(params);
        assert!(lair_of(&ground).is_some(), "{}: no lair there", boss.name);
    }
}
