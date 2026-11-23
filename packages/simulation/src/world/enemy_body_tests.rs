//! Enemy bodies obey the same terrain as rigs and never overlap rigs or pilots: no enemy of any
//! family ends a tick inside solid tiles over long seeded fights (every difficulty, several tick
//! rates, blasts and weapon knockback included), burrowers dig real tunnels (soft rock only),
//! enemies left in rock by a terrain change are resolved out the short way, bodies stop at
//! contact (and still bite), and point-blank shots hit an enemy touching the rig.

use super::WorldImport;
use crate::{
    ControlState, ExplosiveKind, PlacedBomb, PlayerId, SimulationParams, SimulationTick,
    TickInputs, WeaponControl, WorldState,
    enemy::{EnemyDifficulty, EnemyKind, EnemyMode, Locomotion},
    entity::{Body, EntityId, EntityKind, EntityState},
    event::GameplayEventKind,
    physics,
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    weapon::{MUZZLE_RISE, WeaponKind},
    worldgen::{
        TileRect, WorldSettings,
        sky::{self, SkyLandKind},
    },
};

use super::{enemies::RIG_CONTACT_HALF, movement::AUTHORITATIVE_PLAYER_RADIUS};

const PLAYER: PlayerId = PlayerId::new(0);
/// The arena: a cavern far below the surface, east of the town (as in the enemy tests).
const LEFT: i32 = 230;
const RIGHT: i32 = 330;
const TOP: i32 = 200;
const FLOOR: i32 = 232;
/// Deepest an enemy may overlap a rig's or pilot's box at the end of a tick, in pixels. Bodies
/// are pushed apart to exactly touching; the rig moves before enemies in a tick, and a push that
/// would put the enemy into rock is refused, so a rig ramming an enemy pinned against a wall can
/// leave a little overlap for a tick.
const OVERLAP_TOLERANCE: f32 = 1.0;

fn px(tile: f32) -> f32 {
    tile * TILE_SIZE
}

fn fill(
    world: &mut WorldState,
    xs: std::ops::Range<i32>,
    ys: std::ops::Range<i32>,
    kind: TileKind,
) {
    for y in ys {
        for x in xs.clone() {
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
}

#[allow(clippy::cast_precision_loss, reason = "small tile indices")]
fn floor_y(radius: f32) -> f32 {
    (FLOOR as f32).mul_add(TILE_SIZE, -radius) - 1.0
}

fn no_spawns(world: &mut WorldState) {
    for cy in (TOP - 40) / 16..=(FLOOR + 40) / 16 {
        for cx in (LEFT - 60) / 16..=(RIGHT + 60) / 16 {
            world.enemies.cool_chunk(
                crate::terrain::ChunkPosition { x: cx, y: cy },
                SimulationTick::new(u64::MAX),
            );
        }
    }
}

/// A rugged cavern at `hz`: pillars, a thin shelf, a low tunnel and a soft dirt bank with ore
/// in it, the rig parked on the floor in the middle, armed with a rivet gun.
fn arena(hz: u32) -> WorldState {
    let mut world = WorldState::from_import_with_params(
        &WorldImport::new_game(PLAYER),
        SimulationParams::new(hz),
    );
    fill(
        &mut world,
        LEFT - 6..RIGHT + 6,
        TOP - 6..FLOOR + 6,
        TileKind::Stone,
    );
    fill(&mut world, LEFT..RIGHT, TOP..FLOOR, TileKind::Air);
    // Pillars from the floor and from the ceiling.
    fill(&mut world, 252..254, FLOOR - 9..FLOOR, TileKind::Stone);
    fill(&mut world, 296..298, FLOOR - 7..FLOOR, TileKind::HardRock);
    fill(&mut world, 270..272, TOP..TOP + 12, TileKind::Stone);
    // A one-tile shelf and a low tunnel under a slab.
    fill(&mut world, 238..250, 214..215, TileKind::Stone);
    fill(&mut world, 305..318, FLOOR - 3..FLOOR - 2, TileKind::Stone);
    // A soft bank on the west wall with an ore seam (burrowers may not eat ore).
    fill(
        &mut world,
        LEFT..LEFT + 5,
        FLOOR - 12..FLOOR,
        TileKind::Dirt,
    );
    fill(
        &mut world,
        LEFT + 2..LEFT + 3,
        FLOOR - 10..FLOOR - 2,
        TileKind::Ore(MineralKind::Copper),
    );
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(280.5);
    player.y = floor_y(AUTHORITATIVE_PLAYER_RADIUS);
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    let _ = player.receive_weapon(WeaponKind::RivetGun);
    player.weapon.energy = player.battery_capacity();
    world
}

/// Whether `kind` lives in the caves (the arena); the wave 4 families have their own habitats
/// (the sky, the night surface, the Moon), soaked below.
const fn lives_underground(kind: EnemyKind) -> bool {
    // Tomb creatures (T28) have their own body test in `side_tomb_tests`; crypt creatures (T29)
    // live in crypt runs and `world/crypt_tests.rs` holds them to these rules there (spectres
    // drift through walls by design).
    !kind.is_sky()
        && !kind.is_lunar()
        && !crate::night::is_nocturnal(kind)
        && !kind.is_tomb()
        && !kind.is_crypt()
        // The Living Dark's creatures (Wave 8) hold to these rules in their own arena
        // (`living_dark_tests::the_living_darks_creatures_keep_the_body_rules`).
        && !kind.is_living_dark()
        // The Leviathan swims its own path in its arena (Wave 9,
        // `drowned_tests::a_bot_with_descent_iv_kit_beats_the_leviathan...`).
        && !matches!(kind, EnemyKind::Leviathan)
        // ... and the Sun Serpent orbits its well with its coils (Wave 9 integration: with every
        // Descent's kinds in the arena it lands by the rig;
        // `hollow_tests::a_bot_beats_the_sun_serpent_and_the_crew_gets_the_gravity_anchor`).
        && !matches!(kind, EnemyKind::SunSerpent | EnemyKind::SerpentCoil)
        // So do the distortion strata's (Wave 9; their twins only exist in a mirror hall):
        // `distortion_tests::the_distortion_creatures_keep_the_body_rules_in_a_fight`.
        && !kind.is_distortion()
        // So do the far Strange Layers' (Wave 9,
        // `far_strange_tests::the_far_strange_creatures_keep_the_body_rules`).
        && !kind.is_far_strange()
}

/// One of every cave kind at an open spot in the arena.
fn spawn_every_kind(world: &mut WorldState) -> Vec<EntityId> {
    let mut ids = Vec::new();
    let cave_kinds = EnemyKind::ALL
        .into_iter()
        .filter(|kind| lives_underground(*kind));
    for (index, kind) in (0_u16..).zip(cave_kinds) {
        // Twenty-two to a row, three columns apart; later rows (the Crust's and the Old Deep's
        // kinds, Wave 8) sit half a gap over, so every cave kind fits the arena.
        let column = f32::from(index % 22).mul_add(3.0, 258.5) + f32::from(index / 22) * 1.5;
        let column = if (295.0..299.0).contains(&column) {
            column + 4.0
        } else {
            column
        };
        let y = match kind.stats().locomotion {
            Locomotion::Walking | Locomotion::Stationary => floor_y(kind.stats().radius),
            Locomotion::Flying => px(f32::from(index % 4).mul_add(3.0, 216.5)),
            // In a pocket against the soft bank.
            Locomotion::Burrowing => px(225.5),
        };
        let x = if kind == EnemyKind::Burrower {
            px(235.5)
        } else {
            px(column)
        };
        ids.push(world.spawn_enemy(kind, x, y));
    }
    ids
}

fn enemy_bodies(world: &WorldState) -> Vec<(EntityId, Body, EnemyKind, EnemyMode)> {
    world
        .entities()
        .of_kind(EntityKind::Enemy)
        .filter_map(|entity| match entity.state {
            EntityState::Enemy(state) => Some((entity.id, entity.body, state.kind, state.mode)),
            _ => None,
        })
        .collect()
}

/// Overlap depth of an enemy's contact box (half extents `own`) with a box (0 when apart or
/// touching).
fn overlap(body: &Body, own: (f32, f32), centre: (f32, f32), half: (f32, f32)) -> f32 {
    let depth_x = own.0 + half.0 - (body.x - centre.0).abs();
    let depth_y = own.1 + half.1 - (body.y - centre.1).abs();
    depth_x.min(depth_y).max(0.0)
}

/// Asserts the body invariants for every enemy now: out of rock, off the rig and pilots.
fn assert_bodies(world: &WorldState, context: &str) -> f32 {
    let mut deepest: f32 = 0.0;
    for (id, body, kind, _) in enemy_bodies(world) {
        let radius = kind.stats().radius;
        assert!(
            !physics::collides(world.terrain(), body.x, body.y, radius),
            "{context}: {kind:?} #{} inside rock at ({}, {})",
            id.get(),
            body.x,
            body.y
        );
        for player in world.players.values().filter(|player| player.hull > 0.0) {
            let depth = overlap(
                &body,
                kind.contact_half(),
                (player.x, player.y),
                RIG_CONTACT_HALF,
            );
            deepest = deepest.max(depth);
        }
    }
    deepest
}

/// The rig's scripted dance: drive, hover, stop, shooting with auto-aim the whole time.
fn dance(tick: u64, hz: u64) -> ControlState {
    let phase = (tick / (hz * 3 / 2)) % 6;
    let (axis, thrust) = match phase {
        0 => (1.0, false),
        1 => (-1.0, false),
        2 => (0.0, true),
        3 => (-1.0, true),
        4 => (1.0, true),
        _ => (0.0, false),
    };
    ControlState::new(axis, thrust, false).with_weapon(WeaponControl {
        fire: true,
        auto_aim: true,
        aim_x: if tick.is_multiple_of(2) { 127 } else { -127 },
        aim_y: 0,
    })
}

/// A long fight: every kind plus the director's spawns, the rig dancing and shooting, a bomb
/// every few seconds next to an enemy. Checks the invariants after every tick; returns the
/// deepest rig overlap seen, the rig hits and how many enemy-ticks were checked.
fn long_fight(difficulty: EnemyDifficulty, hz: u32, seconds: u64) -> (f32, usize, usize) {
    let mut world = arena(hz);
    world.set_enemy_difficulty(difficulty);
    let _ = spawn_every_kind(&mut world);
    fight(world, hz, seconds, &format!("{difficulty:?}"))
}

/// Runs `world`'s fight for `seconds` at `hz` (the rig dancing and shooting, a bomb by the
/// first enemy every four seconds), checking the body invariants after every tick. Returns the
/// deepest rig overlap seen, the rig hits and how many enemy-ticks were checked.
fn fight(mut world: WorldState, hz: u32, seconds: u64, label: &str) -> (f32, usize, usize) {
    let hz = u64::from(hz);
    let mut deepest: f32 = 0.0;
    let mut struck = 0;
    let mut checked = 0;
    for tick in 0..seconds * hz {
        if tick % (hz * 4) == hz {
            // A bomb by the first enemy: blasts push bodies toward walls.
            if let Some((_, body, _, _)) = enemy_bodies(&world).first() {
                let (x, y) = (body.x + 6.0, body.y + 4.0);
                world.insert_charge(PlacedBomb {
                    id: 10_000 + tick,
                    x,
                    y,
                    previous_x: x,
                    previous_y: y,
                    kind: ExplosiveKind::Bomb,
                    stage: 1,
                    detonation_tick: SimulationTick::new(world.simulation_tick().get() + 2),
                    stuck: true,
                    ..PlacedBomb::default()
                });
            }
        }
        let inputs = TickInputs::held(PLAYER, dance(tick, hz));
        let output = world.simulate_tick(world.simulation_tick(), &inputs);
        struck += output
            .events
            .iter()
            .filter(|event| matches!(event.kind, GameplayEventKind::RigStruck { .. }))
            .count();
        // Keep the rig in the fight (and its gun loaded).
        let player = world.player_mut(PLAYER).expect("player");
        player.hull = player.max_hull();
        player.fuel = player.fuel_capacity;
        player.weapon.energy = player.battery_capacity();
        checked += world.entities().count_of(EntityKind::Enemy);
        deepest = deepest.max(assert_bodies(
            &world,
            &format!("{label} at {hz} Hz, tick {tick}"),
        ));
    }
    (deepest, struck, checked)
}

#[test]
fn no_enemy_ever_ends_a_tick_inside_rock_or_inside_the_rig() {
    for difficulty in [EnemyDifficulty::Normal, EnemyDifficulty::Hard] {
        let (deepest, struck, checked) = long_fight(difficulty, 60, 45);
        assert!(checked > 60 * 45 * 4, "a real fight: {checked}");
        assert!(
            struck > 0,
            "{difficulty:?}: enemies still reach and bite the rig"
        );
        assert!(
            deepest <= OVERLAP_TOLERANCE,
            "{difficulty:?}: an enemy overlapped the rig by {deepest} px"
        );
    }
}

/// Fits the rig for any habitat (it is kept topped up through the fight) and arms it.
fn arm_rig(world: &mut WorldState) {
    let player = world.player_mut(PLAYER).expect("player");
    player.hazard_gear.pressurized_hull = crate::moon::VACUUM_SEAL_TIER;
    player.hazard_gear.life_support = 1;
    player.hazard_gear.de_icer = 6;
    player.hazard_gear.altitude_intake = 6;
    let _ = player.receive_weapon(WeaponKind::RivetGun);
    player.weapon.energy = player.battery_capacity();
}

/// Parks the rig on the floor whose top row is `floor_row` at column `x`, and streams the
/// terrain around it.
#[allow(clippy::cast_precision_loss, reason = "small tile indices")]
fn park_on(world: &mut WorldState, x: i32, floor_row: i32) {
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(x as f32 + 0.5);
    player.x_remainder = 0.0;
    player.y = px(floor_row as f32) - AUTHORITATIVE_PLAYER_RADIUS - 0.5;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    world.stream_terrain_near_players();
}

/// Spawns `kinds` around the rig: walkers and burrowers on the ground beside it, fliers in the
/// air over it (spawns move any spot in rock to the nearest open one).
fn spawn_around_rig(world: &mut WorldState, kinds: &[EnemyKind]) {
    let rig = world.player(PLAYER).expect("player");
    let (x, y) = (rig.x, rig.y);
    for (index, kind) in (0_u16..).zip(kinds.iter().copied()) {
        let side = if index % 2 == 0 { 1.0 } else { -1.0 };
        let reach = f32::from(index / 2).mul_add(3.0, 4.0);
        let dx = px(side * reach);
        let at = match kind.stats().locomotion {
            Locomotion::Flying => (x + dx, y - px(f32::from(index % 3).mul_add(2.0, 3.0))),
            Locomotion::Burrowing => (x + dx, y + px(1.5)),
            Locomotion::Walking | Locomotion::Stationary => (
                x + dx,
                y + AUTHORITATIVE_PLAYER_RADIUS - kind.stats().radius - 1.0,
            ),
        };
        if kind == EnemyKind::PirateAirship {
            let _ = world.spawn_airship(at.0, at.1 - px(4.0));
        } else if kind != EnemyKind::AirshipTurret {
            let _ = world.spawn_enemy(kind, at.0, at.1);
        }
    }
}

/// The wave 4 kinds of the habitat `pick` selects.
fn kinds_where(pick: fn(EnemyKind) -> bool) -> Vec<EnemyKind> {
    EnemyKind::ALL
        .into_iter()
        .filter(|kind| pick(*kind))
        .collect()
}

/// A V9 world with the rig on a cloud archipelago isle (the hostile sky, away from sky towns
/// and cities) and every sky family around it: the isle's rock to collide with above and below.
fn sky_arena(hz: u32) -> WorldState {
    let settings = WorldSettings::new(0x5C1E_B0D1);
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(hz));
    let bands = settings.generator.sky_bands().expect("V9 has a sky");
    let search = TileRect {
        min: TilePosition {
            x: -6_000,
            y: sky::row_of(bands.archipelago + 900),
        },
        max: TilePosition {
            x: 6_000,
            y: sky::row_of(bands.archipelago + 40),
        },
    };
    let isle = sky::lands_touching(&settings, &search)
        .into_iter()
        .filter(|land| land.kind == SkyLandKind::ArchipelagoIsle && land.half_width >= 16)
        .filter(|land| {
            let top = TilePosition {
                x: land.center_x,
                y: land.top_row - 2,
            };
            world.hostile_sky(top) && !world.sky_safe_zone(top) && !world.enemy_safe_zone(top)
        })
        .min_by_key(|land| land.center_x.abs())
        .expect("an archipelago isle in the hostile sky");
    let floor = isle.surface_row(isle.center_x).expect("turf");
    park_on(&mut world, isle.center_x, floor);
    arm_rig(&mut world);
    spawn_around_rig(&mut world, &kinds_where(EnemyKind::is_sky));
    world
}

/// A V9 world at night with the rig on open, exposed ground and every night monster around
/// it (the dune stalker digs its way through the topsoil).
fn night_arena(hz: u32) -> WorldState {
    let settings = WorldSettings::new(0x416E_B0D1);
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(hz));
    let length = world.day_length();
    // 21:00 on the first day.
    world.set_world_time_millis(length.millis() + 84 * length.slice_millis());
    let column = (300..6_000)
        .find(|column| {
            let row = world.terrain().surface_row(*column);
            (column - 40..=column + 40).all(|x| {
                !world.night_safe_zone(TilePosition {
                    x,
                    y: world.terrain().surface_row(x) - 1,
                })
            }) && (column - 4..=column + 4).all(|x| world.terrain().surface_row(x) == row)
        })
        .expect("an exposed stretch of surface");
    let floor = world.terrain().surface_row(column);
    park_on(&mut world, column, floor);
    arm_rig(&mut world);
    spawn_around_rig(&mut world, &kinds_where(crate::night::is_nocturnal));
    world
}

/// A V9 world with the rig on open ground on the Moon and every lunar creature around it.
fn moon_arena(hz: u32) -> WorldState {
    let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(hz));
    let layout = world
        .moon_layout()
        .expect("the current generator fills the Moon");
    let moon = layout.moon;
    let column = (moon.center_x + 300..moon.center_x + moon.half_width - 300)
        .find(|x| {
            let row = layout.surface_row(*x).expect("covered");
            (-12..=12).all(|dx| layout.surface_row(x + dx) == Some(row))
                && !layout
                    .colony_area()
                    .expanded(40)
                    .contains(TilePosition { x: *x, y: row })
        })
        .expect("flat ground on the Moon");
    let floor = layout.surface_row(column).expect("covered");
    park_on(&mut world, column, floor);
    arm_rig(&mut world);
    spawn_around_rig(&mut world, &kinds_where(EnemyKind::is_lunar));
    world
}

#[test]
fn wave_four_families_keep_their_bodies_in_their_habitats() {
    // The sky families (wyverns, storm elementals, airships and their turrets, boarders,
    // nesters), the night monsters and the Moon's creatures obey the same body rules as the
    // cave kinds: never inside rock (the isles, the surface, the regolith), never over the rig.
    for (label, world) in [
        ("sky", sky_arena(60)),
        ("night", night_arena(60)),
        ("Moon", moon_arena(60)),
    ] {
        let spawned = world.entities().count_of(EntityKind::Enemy);
        assert!(spawned >= 4, "{label}: only {spawned} enemies spawned");
        let (deepest, _, checked) = fight(world, 60, 30, label);
        // (The rig's gun thins a pack fast: at least two enemies on average for the half
        // minute.)
        assert!(checked > 60 * 30 * 2, "{label}: a real fight: {checked}");
        assert!(
            deepest <= OVERLAP_TOLERANCE,
            "{label}: an enemy overlapped the rig by {deepest} px"
        );
    }
}

#[test]
fn bodies_hold_at_every_tick_rate() {
    for hz in [30, 120] {
        let (deepest, _, _) = long_fight(EnemyDifficulty::Hard, hz, 20);
        assert!(
            deepest <= OVERLAP_TOLERANCE,
            "{hz} Hz: an enemy overlapped the rig by {deepest} px"
        );
    }
}

#[test]
fn a_bat_stops_at_contact_bites_and_backs_off() {
    let mut world = arena(60);
    no_spawns(&mut world);
    let rig = (px(280.5), floor_y(AUTHORITATIVE_PLAYER_RADIUS));
    let bat = world.spawn_enemy(EnemyKind::CaveBat, rig.0 + px(3.0), rig.1 - px(1.0));
    let mut struck = 0;
    let mut nearest = f32::MAX;
    let mut farthest_after_bite: f32 = 0.0;
    for _ in 0..60 * 6 {
        let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        let bit = output
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::RigStruck { .. }));
        let player = world.player_mut(PLAYER).expect("player");
        player.hull = player.max_hull();
        let (px_, py_) = (player.x, player.y);
        let Some((_, body, _, _)) = enemy_bodies(&world).into_iter().find(|(id, ..)| *id == bat)
        else {
            panic!("the bat is alive");
        };
        let depth = overlap(
            &body,
            EnemyKind::CaveBat.contact_half(),
            (px_, py_),
            RIG_CONTACT_HALF,
        );
        assert!(
            depth <= OVERLAP_TOLERANCE,
            "the bat sat in the rig: {depth}"
        );
        let distance = (body.x - px_).hypot(body.y - py_);
        nearest = nearest.min(distance);
        if bit {
            struck += 1;
        }
        if struck > 0 {
            farthest_after_bite = farthest_after_bite.max(distance);
        }
    }
    assert!(struck >= 2, "it bit, more than once: {struck}");
    assert!(
        nearest
            >= EnemyKind::CaveBat
                .contact_half()
                .1
                .min(EnemyKind::CaveBat.contact_half().0)
                + RIG_CONTACT_HALF.1.min(RIG_CONTACT_HALF.0)
                - OVERLAP_TOLERANCE,
        "touching at closest: {nearest}"
    );
    assert!(
        farthest_after_bite > px(1.5),
        "it backed off between bites: {farthest_after_bite}"
    );
}

#[test]
fn melee_lunges_end_at_contact_and_recoil() {
    for kind in [
        EnemyKind::RockMimic,
        EnemyKind::Shellback,
        EnemyKind::MiningDrone,
        EnemyKind::RogueExcavator,
    ] {
        let mut world = arena(60);
        no_spawns(&mut world);
        let radius = kind.stats().radius;
        let y = match kind.stats().locomotion {
            Locomotion::Flying => floor_y(AUTHORITATIVE_PLAYER_RADIUS),
            _ => floor_y(radius),
        };
        // Inside a dormant mimic's waking range (2.5 tiles) and every lunge range.
        let id = world.spawn_enemy(kind, px(283.0), y);
        let mut struck = 0;
        for _ in 0..60 * 8 {
            let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
            struck += output
                .events
                .iter()
                .filter(|event| matches!(event.kind, GameplayEventKind::RigStruck { .. }))
                .count();
            let player = world.player_mut(PLAYER).expect("player");
            player.hull = player.max_hull();
            let _ = assert_bodies(&world, &format!("{kind:?}"));
            let rig = world.player(PLAYER).expect("player");
            if let Some((_, body, ..)) = enemy_bodies(&world).into_iter().find(|e| e.0 == id) {
                let depth = overlap(&body, kind.contact_half(), (rig.x, rig.y), RIG_CONTACT_HALF);
                assert!(
                    depth <= OVERLAP_TOLERANCE,
                    "{kind:?} inside the rig: {depth}"
                );
            }
        }
        assert!(struck >= 1, "{kind:?} still lands its lunge at contact");
    }
}

#[test]
fn pilots_on_foot_are_touched_never_overlapped() {
    let mut world = arena(60);
    no_spawns(&mut world);
    // Leave the rig and stand a few tiles off it.
    let inputs = TickInputs::default().act(PLAYER, crate::PlayerCommand::TogglePilot);
    let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    for _ in 0..20 {
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    }
    let Some((pilot_id, ..)) = world.pilot_of(PLAYER) else {
        // The pilot needs room to step out; the arena floor has it.
        panic!("the pilot is out");
    };
    let bat = world.spawn_enemy(EnemyKind::CaveBat, px(276.5), px(228.0));
    let mut struck = 0;
    for _ in 0..60 * 5 {
        let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        struck += output
            .events
            .iter()
            .filter(|event| matches!(event.kind, GameplayEventKind::PilotHurt { .. }))
            .count();
        world.entities.update(pilot_id, |entity| {
            if let EntityState::Pilot(pilot) = &mut entity.state {
                pilot.health = pilot.max_health;
            }
        });
        let Some((_, pilot_body, pilot)) = world.pilot_of(PLAYER) else {
            panic!("the pilot is alive");
        };
        let middle = pilot.middle(pilot_body);
        let half = (crate::pilot::PILOT_HALF_WIDTH, pilot.height() * 0.5);
        if let Some((_, body, ..)) = enemy_bodies(&world).into_iter().find(|e| e.0 == bat) {
            let depth = overlap(&body, EnemyKind::CaveBat.contact_half(), middle, half);
            assert!(
                depth <= OVERLAP_TOLERANCE,
                "the bat sat in the pilot: {depth}"
            );
        }
    }
    assert!(struck >= 1, "the bat still bites a pilot on foot");
}

#[test]
fn point_blank_shots_hit_an_enemy_touching_the_rig() {
    for weapon in [
        WeaponKind::RivetGun,
        WeaponKind::ScatterBlaster,
        WeaponKind::PlasmaCutter,
        WeaponKind::Railgun,
    ] {
        // Aims: straight up at a bat sitting on the rig's roof (its centre between the
        // hardpoint and the muzzle), and sideways at one hugging the hardpoint.
        for (offset, aim) in [((0.0, -16.0), (0, -127)), ((15.0, -14.0), (127, 0))] {
            let mut world = arena(60);
            no_spawns(&mut world);
            let player = world.player_mut(PLAYER).expect("player");
            let _ = player.receive_weapon(weapon);
            let _ = player.mount_weapon(Some(weapon));
            player.weapon.energy = player.battery_capacity();
            for ammo in crate::weapon::AmmoKind::ALL {
                player.cargo_bay.add(ammo.item(), 50);
            }
            let (x, y) = (player.x, player.y);
            // A tough target so a single hit doesn't kill it: a shellback hit point blank.
            let target = world.spawn_enemy(EnemyKind::CaveBat, x + offset.0, y + offset.1);
            let before = world
                .entities()
                .get(target)
                .and_then(|entity| entity.state.health())
                .map(|health| health.current)
                .expect("bat");
            let controls = ControlState::new(0.0, false, false).with_weapon(WeaponControl {
                fire: true,
                auto_aim: false,
                aim_x: aim.0,
                aim_y: aim.1,
            });
            let output =
                world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
            let fired = output
                .events
                .iter()
                .any(|event| matches!(event.kind, GameplayEventKind::WeaponFired { .. }));
            assert!(fired, "{weapon:?} fired");
            let after = world
                .entities()
                .get(target)
                .and_then(|entity| entity.state.health())
                .map_or(0.0, |health| health.current);
            assert!(
                after < before,
                "{weapon:?} aimed {aim:?} hit the bat at {offset:?} point blank ({before} -> \
                 {after})"
            );
            let _ = MUZZLE_RISE;
        }
    }
}

/// Solid tiles of the dirt bank the burrower tests bury the rig's corner in.
fn bank_tiles(world: &WorldState) -> usize {
    (258..290)
        .flat_map(|x| (FLOOR - 6..FLOOR).map(move |y| TilePosition { x, y }))
        .filter(|tile| world.terrain().is_solid_at(*tile))
        .count()
}

#[test]
fn a_hunting_burrower_chews_through_a_block_only_rarely_and_never_eats_ore() {
    use crate::enemy::{Damage, DamageKind, DamageSource};
    let mut world = arena(60);
    no_spawns(&mut world);
    // Bury the rig's corner of the arena in dirt: the only way to it is through the bank.
    fill(&mut world, 258..290, FLOOR - 6..FLOOR, TileKind::Dirt);
    fill(
        &mut world,
        263..264,
        FLOOR - 6..FLOOR,
        TileKind::Ore(MineralKind::Iron),
    );
    let ore_before: Vec<TilePosition> = (FLOOR - 6..FLOOR)
        .map(|y| TilePosition { x: 263, y })
        .collect();
    fill(&mut world, 250..290, FLOOR..FLOOR + 16, TileKind::Stone);
    // The rig in a pocket of its own, nine tiles from the burrower's den.
    fill(&mut world, 277..278, FLOOR - 1..FLOOR, TileKind::Air);
    let burrower = world.spawn_enemy(EnemyKind::Burrower, px(268.5), px(229.5));
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(277.5);
    player.velocity_x = 0.0;
    let solid_before = bank_tiles(&world);
    let seconds = 30;
    for tick in 0..60 * seconds {
        // Stung again and again it keeps hunting the rig (out of sight it would forget it).
        if tick % 120 == 0 {
            let mut output = crate::TickOutput::default();
            let _ = world.damage_enemy(
                burrower,
                Damage::new(0.01, DamageKind::Kinetic, DamageSource::Player(PLAYER)),
                &mut output,
            );
        }
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        let player = world.player_mut(PLAYER).expect("player");
        player.hull = player.max_hull();
        let _ = assert_bodies(&world, "burrower");
    }
    let dug = solid_before - bank_tiles(&world);
    let most = usize::try_from(seconds * 1_000 / super::enemies::BITE_MILLIS).expect("few") + 1;
    assert!(dug >= 1, "it can still bite its way toward its prey");
    assert!(dug <= most, "one tile a bite, a bite now and then: {dug}");
    for tile in ore_before {
        assert!(
            matches!(
                world.terrain().tile(tile).map(|t| t.kind),
                Some(TileKind::Ore(_))
            ),
            "the ore at {tile:?} is untouched"
        );
    }
}

/// Burrowers still live in the rock: one appearing inside solid dirt digs its den in place
/// (open space, not moved out into a cave), the director still picks solid soft rock for
/// them, and sealed in there it lies in wait: a rig drilling nearby is never noticed and the
/// ground around the den stays whole.
#[test]
fn burrowers_spawn_in_solid_soft_rock_by_digging_a_den_and_lie_in_wait() {
    let mut world = arena(60);
    no_spawns(&mut world);
    fill(&mut world, 258..290, FLOOR - 6..FLOOR, TileKind::Dirt);
    fill(&mut world, 250..290, FLOOR..FLOOR + 16, TileKind::Stone);
    fill(&mut world, 276..280, FLOOR..FLOOR + 1, TileKind::Dirt);
    let den = TilePosition {
        x: 270,
        y: FLOOR - 2,
    };
    assert!(
        world.site_fits(EnemyKind::Burrower, den),
        "solid soft rock is burrower habitat"
    );
    let (x, y) = (
        px(270.5),
        px(f32::from(i16::try_from(FLOOR).expect("row")) - 1.5),
    );
    let burrower = world.spawn_enemy(EnemyKind::Burrower, x, y);
    let (_, body, ..) = enemy_bodies(&world)
        .into_iter()
        .find(|e| e.0 == burrower)
        .expect("burrower");
    assert!(
        (body.x - x).abs() < 0.5 && (body.y - y).abs() < 0.5,
        "it stays in its den"
    );
    assert!(!physics::collides(world.terrain(), body.x, body.y, 8.0));
    assert!(!world.terrain().is_solid_at(den), "the den is dug");
    fill(&mut world, 277..278, FLOOR - 1..FLOOR, TileKind::Air);
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(277.5);
    player.velocity_x = 0.0;
    let solid_before = (258..276)
        .flat_map(|x| (FLOOR - 6..FLOOR).map(move |y| TilePosition { x, y }))
        .filter(|tile| world.terrain().is_solid_at(*tile))
        .count();
    for tick in 0..60 * 12 {
        let inputs = if tick % 120 < 45 {
            TickInputs::held(PLAYER, ControlState::new(0.0, false, true))
        } else {
            TickInputs::default()
        };
        let output = world.simulate_tick(world.simulation_tick(), &inputs);
        assert!(
            !output
                .events
                .iter()
                .any(|event| matches!(event.kind, GameplayEventKind::EnemyAlerted { .. })),
            "the drill gave the rig away through rock"
        );
        let _ = assert_bodies(&world, "burrower in its den");
    }
    let solid_after = (258..276)
        .flat_map(|x| (FLOOR - 6..FLOOR).map(move |y| TilePosition { x, y }))
        .filter(|tile| world.terrain().is_solid_at(*tile))
        .count();
    assert_eq!(
        solid_after, solid_before,
        "the ground around the den is whole"
    );
}

#[test]
fn a_burrower_with_an_open_way_to_its_prey_digs_nothing() {
    let mut world = arena(60);
    no_spawns(&mut world);
    // A flat open room: stone floor, the rig five tiles from the burrower, in plain sight.
    fill(&mut world, 258..290, FLOOR - 6..FLOOR, TileKind::Air);
    fill(&mut world, 250..296, FLOOR..FLOOR + 4, TileKind::Stone);
    let burrower = world.spawn_enemy(EnemyKind::Burrower, px(270.5), floor_y(9.0));
    let player = world.player_mut(PLAYER).expect("player");
    player.x = px(275.5);
    player.velocity_x = 0.0;
    let floor = |world: &WorldState| {
        (250..296)
            .flat_map(|x| (FLOOR..FLOOR + 4).map(move |y| TilePosition { x, y }))
            .filter(|tile| world.terrain().is_solid_at(*tile))
            .count()
    };
    let before = floor(&world);
    let mut bitten = 0;
    for _ in 0..60 * 20 {
        let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        bitten += output
            .events
            .iter()
            .filter(|event| matches!(event.kind, GameplayEventKind::RigStruck { .. }))
            .count();
        let player = world.player_mut(PLAYER).expect("player");
        player.hull = player.max_hull();
        let _ = assert_bodies(&world, "burrower in the open");
        assert!(enemy_bodies(&world).iter().any(|e| e.0 == burrower));
    }
    assert!(bitten >= 1, "it crawled to the rig and bit it");
    assert_eq!(
        floor(&world),
        before,
        "and ate none of the floor on the way"
    );
}

#[test]
fn enemies_left_in_rock_move_out_the_short_way() {
    let mut world = arena(60);
    no_spawns(&mut world);
    // A bat in a one-tile corridor; then the corridor's tile closes over it (a built piece, a
    // boulder settling). It must come out in the corridor, not above its ceiling.
    fill(&mut world, 300..320, 205..208, TileKind::Stone);
    fill(&mut world, 300..320, 206..207, TileKind::Air);
    let bat = world.spawn_enemy(EnemyKind::CaveBat, px(310.5), px(206.5));
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: 310, y: 206 }, TileKind::Stone);
    let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    let (_, body, ..) = enemy_bodies(&world)
        .into_iter()
        .find(|e| e.0 == bat)
        .expect("bat");
    assert!(!physics::collides(world.terrain(), body.x, body.y, 6.0));
    assert!(
        (px(206.0)..px(207.0)).contains(&body.y),
        "still in the corridor: {}",
        body.y / TILE_SIZE
    );
    // Spawns overlapping rock move to open space too.
    let wedged = world.spawn_enemy(EnemyKind::MiningDrone, px(306.5), px(205.6));
    let (_, body, ..) = enemy_bodies(&world)
        .into_iter()
        .find(|e| e.0 == wedged)
        .expect("drone");
    assert!(!physics::collides(world.terrain(), body.x, body.y, 7.0));
}

#[test]
fn a_hard_shove_into_a_wall_never_embeds_an_enemy() {
    let mut world = arena(60);
    no_spawns(&mut world);
    for kind in [
        EnemyKind::CaveBat,
        EnemyKind::Shellback,
        EnemyKind::Burrower,
    ] {
        let radius = kind.stats().radius;
        let y = match kind.stats().locomotion {
            Locomotion::Walking => floor_y(radius),
            _ => px(224.5),
        };
        let id = world.spawn_enemy(kind, px(329.0), y);
        let mut output = crate::TickOutput::default();
        let _ = world.damage_enemy(
            id,
            crate::enemy::Damage::new(
                1.0,
                crate::enemy::DamageKind::Kinetic,
                crate::enemy::DamageSource::World,
            )
            .with_impulse(2_000.0, -600.0),
            &mut output,
        );
        for _ in 0..30 {
            let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
            let _ = assert_bodies(&world, &format!("{kind:?} shoved"));
        }
    }
}

#[test]
fn enemy_bodies_are_deterministic_and_replicate() {
    let run = || {
        let mut world = arena(60);
        world.set_enemy_difficulty(EnemyDifficulty::Hard);
        let _ = spawn_every_kind(&mut world);
        let mut replica: WorldState =
            serde_json::from_str(&serde_json::to_string(&world).expect("save")).expect("load");
        for tick in 0..60 * 10 {
            let inputs = TickInputs::held(PLAYER, dance(tick, 60));
            let _ = world.simulate_tick(world.simulation_tick(), &inputs);
            let _ = replica.simulate_tick(replica.simulation_tick(), &inputs);
        }
        assert_eq!(
            world.simulation_state_hash(),
            replica.simulation_state_hash(),
            "a replica ticking the same inputs agrees"
        );
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
}
