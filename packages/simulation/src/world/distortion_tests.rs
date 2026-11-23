//! The distortion strata's mechanics in a Deep Descent world (Wave 9, Descent VI part A): mirror
//! halls flip the controls (and the aim) on host and replay alike, twins copy moves and shots,
//! gazers hold a rig's controls, reflections shatter and real ore pays, shear halls and the
//! Spire turn gravity, Pixel Rot deepens by tier, glitchlings teleport onto the rot grid, data
//! cores pay a whole block, the other creatures keep their body rules, landmarks are found once,
//! and the state replicates, hashes and spares older worlds.

#![allow(
    clippy::cast_precision_loss,
    reason = "tile coordinates of a test world are small"
)]

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WeaponControl,
    WorldState,
    descent::Stratum,
    distortion::{
        DistortionEvent, DistortionLandmark, WarpedTile, mirror, rot,
        shear::{self, turned},
    },
    enemy::{EnemyDifficulty, EnemyKind, EnemyMode, Locomotion},
    item::ItemKind,
    physics,
    position::WorldPos,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    weapon::WeaponKind,
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{landmarks, layout},
    },
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x5EA2_7E57;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// A Deep Descent world at `hz` with the rig fitted for the deep (every gate system, so no gate
/// crushes it in Descent VI) and the director quiet.
fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
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

fn step(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let output = world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
    output.events.into_iter().map(|event| event.kind).collect()
}

fn ticks(world: &mut WorldState, count: u64) -> Vec<GameplayEventKind> {
    (0..count)
        .flat_map(|_| step(world, ControlState::default()))
        .collect()
}

fn distortion_events(events: &[GameplayEventKind]) -> Vec<DistortionEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Distortion { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

/// The Looking-Glass Hall on the test seed.
fn hall() -> mirror::MirrorZone {
    mirror::looking_glass_hall(&settings(), &layout(&settings())).expect("the hall")
}

/// A spot well inside the hall, `offset` tiles left of its axis, at mid height, cleared.
fn inside_hall(world: &mut WorldState, offset: i32) -> TilePosition {
    let zone = hall();
    let middle = zone.bounds.center();
    let axis = zone.axis2() / 2;
    let spot = TilePosition {
        x: axis - offset,
        y: middle.y,
    };
    // Clear a room either side of the axis so the rig and its twin both stand in the open.
    fill(
        world,
        TileRect {
            min: TilePosition {
                x: zone.bounds.min.x + 4,
                y: middle.y - 4,
            },
            max: TilePosition {
                x: zone.bounds.max.x - 4,
                y: middle.y + 4,
            },
        },
        TileKind::Air,
    );
    spot
}

#[test]
fn a_mirror_hall_flips_the_screen_the_stick_and_the_aim() {
    let mut world = world_at(60);
    let spot = inside_hall(&mut world, 10);
    put_rig(&mut world, spot);
    let fields = world.fields_for(PLAYER);
    assert!(fields.mirrored(), "inside a hall the controls mirror");
    assert!(fields.screen_level(crate::field::ScreenEffect::Mirror) > 0.9);
    let controls = ControlState::new(1.0, false, false).with_weapon(WeaponControl {
        fire: false,
        auto_aim: false,
        aim_x: 100,
        aim_y: -20,
    });
    let mirrored = world.mirror_controls(PLAYER, controls);
    assert!(mirrored.horizontal() < 0.0);
    assert_eq!(mirrored.weapon.aim_x, -100);
    assert_eq!(mirrored.weapon.aim_y, -20);
    // Pushing right in the hall drives the rig left in the world (the screen shows it right).
    let start = world.player(PLAYER).expect("rig").x;
    for _ in 0..40 {
        let _ = step(&mut world, ControlState::new(1.0, false, false));
    }
    assert!(world.player(PLAYER).expect("rig").x < start - 4.0);
    // Outside every hall (Parallax Station's column stays plain) nothing mirrors.
    let column = landmarks::outpost_column(&settings());
    let (top, bottom) = layout(&settings()).rows(Stratum::MirrorStrata);
    for row in (top + 20..bottom - 20).step_by(8) {
        let sample = crate::field::sample(
            &crate::field::FieldZones::default(),
            &settings(),
            (column as f32 + 0.5) * TILE_SIZE,
            (row as f32 + 0.5) * TILE_SIZE,
        );
        assert!(!sample.mirrored(), "row {row}");
    }
}

#[test]
fn mirrored_steering_is_the_same_at_every_tick_rate_and_on_replay() {
    let run = |hz: u32| {
        let mut world = world_at(hz);
        let spot = inside_hall(&mut world, 12);
        put_rig(&mut world, spot);
        let seconds = 2;
        for index in 0..hz * seconds {
            let axis = if index < hz { 1.0 } else { -0.5 };
            // Thrust pulses by world time, so every tick rate pulses alike.
            let thrust = (index * 1_000 / hz) % 500 < 130;
            let _ = step(&mut world, ControlState::new(axis, thrust, false));
        }
        let rig = world.player(PLAYER).expect("rig");
        (rig.x, rig.y, world.simulation_state_hash())
    };
    let (x60, y60, hash60) = run(60);
    let (_, _, again) = run(60);
    assert_eq!(hash60, again, "a replay of the same inputs agrees exactly");
    let (x20, y20, _) = run(20);
    assert!((x60 - x20).abs() < TILE_SIZE * 1.5, "{x60} vs {x20}");
    assert!((y60 - y20).abs() < TILE_SIZE * 2.0, "{y60} vs {y20}");
}

#[test]
fn a_twin_steps_out_of_the_glass_and_copies_moves_and_shots_mirrored() {
    let mut world = world_at(60);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let spot = inside_hall(&mut world, 9);
    put_rig(&mut world, spot);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.weapon_rack.battery_level = crate::weapon::MAX_BATTERY_LEVEL;
        let _ = rig.receive_weapon(WeaponKind::RivetGun);
        let _ = rig.mount_weapon(Some(WeaponKind::RivetGun));
        rig.weapon.energy = rig.battery_capacity();
    }
    let events = ticks(&mut world, 3);
    assert!(distortion_events(&events).contains(&DistortionEvent::MirrorEntered));
    assert!(distortion_events(&events).contains(&DistortionEvent::TwinAppeared));
    let twin = world.twin_of(PLAYER).expect("a twin");
    let zone = hall();
    let body = |world: &WorldState| world.entities().get(twin).expect("twin").body;
    let rig_x = world.player(PLAYER).expect("rig").x;
    let frame = world.frame();
    let axis = (frame.col_px(zone.bounds.min.x) + frame.col_px(zone.bounds.max.x + 1)) * 0.5;
    assert!(
        (body(&world).x - 2.0f32.mul_add(axis, -rig_x)).abs() < TILE_SIZE,
        "the twin stands at the rig's reflection"
    );
    // The rig drifts one way; the twin drifts the other way, the same distance.
    let (rig_start, twin_start) = (world.player(PLAYER).expect("rig").x, body(&world).x);
    for _ in 0..45 {
        let _ = step(&mut world, ControlState::new(-1.0, false, false));
    }
    let rig_moved = world.player(PLAYER).expect("rig").x - rig_start;
    let twin_moved = body(&world).x - twin_start;
    assert!(
        rig_moved.abs() > TILE_SIZE * 0.5,
        "the rig moved {rig_moved}"
    );
    assert!(
        (twin_moved + rig_moved).abs() < TILE_SIZE,
        "mirrored: rig {rig_moved}, twin {twin_moved}"
    );
    // A shot: the twin fires its copy, mirrored left to right.
    let fire = ControlState::default().with_weapon(WeaponControl {
        fire: true,
        auto_aim: false,
        aim_x: 0,
        aim_y: 127,
    });
    let mut fired = None;
    let mut copied = false;
    for _ in 0..20 {
        let events = step(&mut world, fire);
        for event in &events {
            if let GameplayEventKind::WeaponFired {
                x, end_x, end_y, y, ..
            } = event
            {
                fired = Some((end_x - x, end_y - y));
            }
        }
        copied |= distortion_events(&events).contains(&DistortionEvent::TwinFired);
        if copied {
            break;
        }
    }
    assert!(fired.is_some(), "the rig fired");
    assert!(copied, "the twin fired its copy");
    let shard = world
        .entities()
        .iter()
        .find_map(|entity| match entity.state {
            crate::entity::EntityState::EnemyShot(flying)
                if flying.kind == crate::enemy::ShotKind::MirrorShard =>
            {
                Some(entity.body)
            }
            _ => None,
        })
        .expect("the twin's shard in flight");
    // Fired straight down: the copy flies down too (a mirror keeps up and down).
    assert!(shard.velocity_y > 0.0);
    // The twin fades when its rig leaves the hall.
    let outside = TilePosition {
        x: zone.bounds.min.x - 30,
        y: zone.bounds.center().y,
    };
    fill(&mut world, TileRect::around(outside, 3, 3), TileKind::Air);
    put_rig(&mut world, outside);
    let events = ticks(&mut world, 3);
    assert!(distortion_events(&events).contains(&DistortionEvent::TwinFaded));
    assert!(world.twin_of(PLAYER).is_none());
}

#[test]
fn a_broken_twins_hall_rests_before_another_steps_out() {
    let mut world = world_at(60);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let spot = inside_hall(&mut world, 9);
    put_rig(&mut world, spot);
    let _ = ticks(&mut world, 3);
    let twin = world.twin_of(PLAYER).expect("a twin");
    let output = &mut crate::event::TickOutput::default();
    let _ = world.damage_enemy(
        twin,
        crate::enemy::Damage::new(
            10_000.0,
            crate::enemy::DamageKind::Explosive,
            crate::enemy::DamageSource::Player(PLAYER),
        ),
        output,
    );
    assert!(world.twin_of(PLAYER).is_none());
    assert!(
        world
            .distortion()
            .hall_resting(hall().key(), world.simulation_tick().next())
    );
    let _ = ticks(&mut world, 60);
    assert!(world.twin_of(PLAYER).is_none(), "the hall rests");
}

#[test]
fn a_gazer_mirrors_the_controls_of_the_rig_it_sees() {
    let mut world = world_at(60);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let column = landmarks::outpost_column(&settings()) + 80;
    let row = {
        let (top, bottom) = layout(&settings()).rows(Stratum::MirrorStrata);
        top + (bottom - top) / 4
    };
    // A plain room (no hall), the gazer on its floor to the side.
    let room = TileRect::around(TilePosition { x: column, y: row }, 10, 4);
    fill(&mut world, room.expanded(2), TileKind::Stone);
    fill(&mut world, room, TileKind::Air);
    put_rig(
        &mut world,
        TilePosition {
            x: column - 4,
            y: row,
        },
    );
    let (gx, gy) = world.frame().tile_center(TilePosition {
        x: column + 5,
        y: room.max.y,
    });
    let eye = world.spawn_enemy(EnemyKind::Gazer, gx, gy);
    assert!(!world.fields_for(PLAYER).mirrored());
    let mut caught = false;
    for _ in 0..120 {
        let events = ticks(&mut world, 1);
        caught |= distortion_events(&events).contains(&DistortionEvent::Gazed);
        if caught {
            break;
        }
    }
    assert!(caught, "the gazer caught the rig");
    assert!(world.distortion_gazed(PLAYER));
    let view = world.distortion_view_fields(PLAYER, world.fields_for(PLAYER));
    assert!(view.mirrored());
    assert!(view.screen_level(crate::field::ScreenEffect::Mirror) > 0.9);
    // The gaze breaks with the gazer: the hold runs out.
    world.entities.despawn(eye);
    let _ = ticks(&mut world, 60);
    assert!(!world.distortion_gazed(PLAYER));
}

#[test]
fn real_mirror_ore_pays_and_a_reflection_shatters() {
    let mut world = world_at(60);
    let zone = hall();
    let (x, y) = (zone.bounds.min.x + 6, zone.bounds.center().y);
    fill(
        &mut world,
        TileRect::around(TilePosition { x, y }, 3, 2),
        TileKind::Air,
    );
    put_rig(&mut world, TilePosition { x, y });
    let below = TilePosition { x, y: y + 1 };
    let _ = world
        .terrain_mut()
        .set_kind(below, TileKind::Warped(WarpedTile::MirrorOre));
    let mut events = Vec::new();
    for _ in 0..240 {
        events.extend(step(&mut world, ControlState::new(0.0, false, true)));
        if world
            .terrain()
            .tile(below)
            .is_some_and(|tile| tile.kind.is_open())
        {
            break;
        }
    }
    let _ = ticks(&mut world, 2);
    assert!(distortion_events(&events).contains(&DistortionEvent::MirrorOreMined));
    let items: Vec<ItemKind> = world
        .entities()
        .iter()
        .filter_map(|entity| match &entity.state {
            crate::entity::EntityState::Drop(drop) => Some(drop.stack.kind),
            _ => None,
        })
        .chain(
            world
                .player(PLAYER)
                .expect("rig")
                .cargo_bay
                .iter()
                .map(|(kind, _)| kind),
        )
        .collect();
    assert!(
        items.contains(&ItemKind::Mineral(crate::terrain::MineralKind::Mythril)),
        "{items:?}"
    );
    assert!(
        items.contains(&ItemKind::Layer(Stratum::MirrorStrata)),
        "{items:?}"
    );
    // A reflection: it gives at once, nicks the hull and pays nothing.
    let hull = world.player(PLAYER).expect("rig").hull;
    let spot = world.player(PLAYER).expect("rig").tile_in(world.frame());
    let ghost = TilePosition {
        x: spot.x,
        y: spot.y + 1,
    };
    let _ = world
        .terrain_mut()
        .set_kind(ghost, TileKind::Warped(WarpedTile::MirrorGhost));
    let mut events = Vec::new();
    for _ in 0..120 {
        events.extend(step(&mut world, ControlState::new(0.0, false, true)));
        if world
            .terrain()
            .tile(ghost)
            .is_some_and(|tile| tile.kind.is_open())
        {
            break;
        }
    }
    events.extend(ticks(&mut world, 2));
    assert!(distortion_events(&events).contains(&DistortionEvent::GhostShattered));
    assert!(world.player(PLAYER).expect("rig").hull < hull);
}

#[test]
fn shear_halls_turn_gravity_and_the_spire_points_down_at_its_walls() {
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::GravityShear);
    let regions = shear::regions_touching(
        &settings,
        &layout,
        &TileRect {
            min: TilePosition { x: -900, y: top },
            max: TilePosition { x: 900, y: bottom },
        },
    );
    assert!(!regions.is_empty());
    let zones = crate::field::FieldZones::default();
    for region in regions.iter().take(8) {
        let centre = region.bounds.center();
        let sample = crate::field::sample(
            &zones,
            &settings,
            (centre.x as f32 + 0.5) * TILE_SIZE,
            (centre.y as f32 + 0.5) * TILE_SIZE,
        );
        let (dx, dy) = turned(region.turn);
        assert!(
            (sample.gravity_x - dx as f32).abs() < 0.05
                && (sample.gravity_y - dy as f32).abs() < 0.05,
            "{region:?}: {sample:?}"
        );
        assert!(sample.rotation_unlimited());
    }
    let spire = shear::spire(&settings, &layout).expect("the spire");
    let centre = spire.center();
    let at = |column: i32, row: i32| {
        crate::field::sample(
            &zones,
            &settings,
            (column as f32 + 0.5) * TILE_SIZE,
            (row as f32 + 0.5) * TILE_SIZE,
        )
    };
    assert!(at(spire.min.x + 5, centre.y).gravity_x < -0.9, "left wall");
    assert!(at(spire.max.x - 5, centre.y).gravity_x > 0.9, "right wall");
    assert!(at(centre.x, spire.min.y + 5).gravity_y < -0.9, "ceiling");
    assert!(at(centre.x, spire.max.y - 5).gravity_y > 0.9, "floor");
}

#[test]
fn a_rig_falls_onto_a_shear_halls_turned_floor() {
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::GravityShear);
    let region = shear::regions_touching(
        &settings,
        &layout,
        &TileRect {
            min: TilePosition { x: -900, y: top },
            max: TilePosition { x: 900, y: bottom },
        },
    )
    .into_iter()
    .find(|region| region.turn == 90 || region.turn == 270)
    .expect("a wall hall");
    let mut world = world_at(60);
    let centre = region.bounds.center();
    fill(&mut world, region.bounds.expanded(-3), TileKind::Air);
    put_rig(&mut world, centre);
    let start = world.player(PLAYER).expect("rig").x;
    let _ = ticks(&mut world, 90);
    let moved = world.player(PLAYER).expect("rig").x - start;
    let (dx, _) = region.down();
    assert!(
        moved * dx as f32 > TILE_SIZE * 2.0,
        "the rig fell toward the turned floor: {moved}"
    );
}

#[test]
fn pixel_rot_lowers_the_resolution_tier_by_tier() {
    let settings = settings();
    let layout = layout(&settings);
    let tops = rot::tier_tops(&layout);
    let zones = crate::field::FieldZones::default();
    let mut previous = (0.0, 0.0, 0.0);
    for (tier, top) in tops.iter().enumerate() {
        let row = top + 8;
        assert_eq!(
            rot::tier_at(&layout, row),
            Some(u8::try_from(tier).unwrap())
        );
        let sample = crate::field::sample(
            &zones,
            &settings,
            0.5 * TILE_SIZE,
            (row as f32 + 0.5) * TILE_SIZE,
        );
        let levels = (
            sample.screen_level(crate::field::ScreenEffect::Pixelate),
            sample.screen_level(crate::field::ScreenEffect::Palette),
            sample.screen_level(crate::field::ScreenEffect::Glitch),
        );
        assert!(
            levels.0 > previous.0 && levels.1 > previous.1,
            "tier {tier}: {levels:?}"
        );
        assert!(levels.2 >= previous.2);
        previous = levels;
        // The rock's blocks grow too.
        assert_eq!(
            rot::block_size(u8::try_from(tier).unwrap()),
            rot::BLOCK_SIZES[tier]
        );
    }
    assert!(
        previous.0 > 0.8,
        "the bottom is nearly all pixels: {previous:?}"
    );
    // A tier boundary eases in over a few rows (no pop).
    let boundary = tops[2];
    let mut last = None;
    for row in boundary - 2..boundary + 8 {
        let level = crate::field::sample(
            &zones,
            &settings,
            0.5 * TILE_SIZE,
            (row as f32 + 0.5) * TILE_SIZE,
        )
        .screen_level(crate::field::ScreenEffect::Pixelate);
        if let Some(last) = last {
            let change: f32 = level - last;
            assert!(change.abs() < 0.06, "row {row}: {last} -> {level}");
        }
        last = Some(level);
    }
}

#[test]
fn a_glitchling_shows_its_landing_then_teleports_onto_the_rot_grid() {
    let mut world = world_at(60);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let settings = settings();
    let layout = layout(&settings);
    let tops = rot::tier_tops(&layout);
    let row = tops[3] + 12;
    let column = landmarks::outpost_column(&settings) + 70;
    let room = TileRect::around(TilePosition { x: column, y: row }, 12, 6);
    fill(&mut world, room.expanded(2), TileKind::Stone);
    fill(&mut world, room, TileKind::Air);
    put_rig(
        &mut world,
        TilePosition {
            x: column,
            y: row + 4,
        },
    );
    let (gx, gy) = world.frame().tile_center(TilePosition {
        x: column - 9,
        y: row - 3,
    });
    let glitch = world.spawn_enemy(EnemyKind::Glitchling, gx, gy);
    let mut landing = None;
    let mut teleported = false;
    for _ in 0..600 {
        let events = ticks(&mut world, 1);
        if let Some((_, state)) = world.enemy_parts(glitch)
            && state.mode == EnemyMode::Windup
            && state.waypoint.is_some()
        {
            landing = state.waypoint;
        }
        if distortion_events(&events).contains(&DistortionEvent::GlitchTeleport) {
            teleported = true;
            break;
        }
    }
    let landing = landing.expect("it showed where it would land");
    assert!(teleported, "it teleported");
    let (body, _) = world.enemy_parts(glitch).expect("alive");
    let at = world.frame().tile_at(body.x, body.y);
    assert_eq!((at.x, at.y), landing, "it landed where its ghost was");
    // On the grid: the middle of a block of the tier's size.
    let size = rot::block_size(3);
    assert_eq!(landing.0.rem_euclid(size), size / 2);
    assert!(!physics::collides(
        world.terrain(),
        body.x,
        body.y,
        EnemyKind::Glitchling.stats().radius
    ));
}

#[test]
fn a_whole_data_block_pays_a_data_core() {
    let mut world = world_at(60);
    let settings = settings();
    let layout = layout(&settings);
    let tops = rot::tier_tops(&layout);
    // Find a data block in the second tier on the test seed.
    let cell = (0..400)
        .flat_map(|bx| (2..6).map(move |by| (bx, by)))
        .find_map(|(bx, by)| {
            let size = rot::block_size(1);
            let cell = rot::block_at(&layout, bx * size, tops[1] + by * size)?;
            rot::is_data_block(&settings, &cell).then_some(cell)
        })
        .expect("a data block");
    // Mine all but one tile, then the last one.
    let positions: Vec<TilePosition> = cell.bounds.positions().collect();
    let (last, rest) = positions.split_last().expect("tiles");
    for at in rest {
        let _ = world.terrain_mut().set_kind(*at, TileKind::Air);
    }
    let _ = world
        .terrain_mut()
        .set_kind(*last, TileKind::Warped(WarpedTile::DataOre));
    put_rig(
        &mut world,
        TilePosition {
            x: last.x,
            y: last.y - 1,
        },
    );
    let _ = world.terrain_mut().set_kind(
        TilePosition {
            x: last.x,
            y: last.y - 1,
        },
        TileKind::Air,
    );
    let _ = ticks(&mut world, 1);
    // Breaking the last tile (any way: here the terrain itself) recovers the core.
    let _ = world.terrain_mut().set_kind(*last, TileKind::Air);
    let events = ticks(&mut world, 1);
    let _ = events;
    let mut found = false;
    let mut world2 = world_at(60);
    for at in rest {
        let _ = world2.terrain_mut().set_kind(*at, TileKind::Air);
    }
    let _ = world2
        .terrain_mut()
        .set_kind(*last, TileKind::Warped(WarpedTile::DataOre));
    put_rig(
        &mut world2,
        TilePosition {
            x: last.x,
            y: last.y - 1,
        },
    );
    for _ in 0..300 {
        let events = step(&mut world2, ControlState::new(0.0, false, true));
        if distortion_events(&events).contains(&DistortionEvent::DataCore) {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "drilling the last tile of {cell:?} recovered a data core"
    );
}

#[test]
fn the_crew_finds_each_landmark_once() {
    let mut world = world_at(60);
    let zone = hall();
    let spot = inside_hall(&mut world, 8);
    let _ = zone;
    put_rig(&mut world, spot);
    let events = ticks(&mut world, 2);
    let found: Vec<DistortionLandmark> = events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::DistortionLandmarkFound { landmark, .. } => Some(*landmark),
            _ => None,
        })
        .collect();
    assert_eq!(found, vec![DistortionLandmark::LookingGlassHall]);
    assert!(
        world
            .distortion()
            .has_found(DistortionLandmark::LookingGlassHall)
    );
    let again = ticks(&mut world, 30);
    assert!(
        !again
            .iter()
            .any(|event| matches!(event, GameplayEventKind::DistortionLandmarkFound { .. }))
    );
}

#[test]
fn the_distortion_section_replicates_and_hashes_only_once_something_happened() {
    use crate::world::SectionKey;
    assert!(SectionKey::SHARED.contains(&SectionKey::Distortion));
    let mut world = world_at(60);
    let quiet = world.clone();
    let spot = inside_hall(&mut world, 8);
    put_rig(&mut world, spot);
    let _ = ticks(&mut world, 2);
    assert!(!world.distortion().is_empty());
    let mut replica = quiet;
    replica.apply_section(
        SectionKey::Distortion,
        world.section(SectionKey::Distortion).expect("section"),
    );
    assert_eq!(replica.distortion(), world.distortion());
    // An older world (no Descents) never touches it, so its hash stays its own.
    let mut classic = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(SEED),
        SimulationParams::new(30),
    );
    let _ = ticks(&mut classic, 30);
    assert!(classic.distortion().is_empty());
}

#[test]
fn a_lodestar_pulls_a_rig_toward_itself() {
    let mut world = world_at(60);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let column = landmarks::outpost_column(&settings()) + 90;
    let row = {
        let (top, _) = layout(&settings()).rows(Stratum::GravityShear);
        top + 20
    };
    let room = TileRect::around(TilePosition { x: column, y: row }, 12, 6);
    fill(&mut world, room.expanded(2), TileKind::Stone);
    fill(&mut world, room, TileKind::Air);
    put_rig(
        &mut world,
        TilePosition {
            x: column - 4,
            y: row,
        },
    );
    // The rig settles on the floor first, then the lodestar appears beside it.
    let _ = ticks(&mut world, 60);
    let floor = world.player(PLAYER).expect("rig").tile_in(world.frame()).y;
    let (lx, ly) = world.frame().tile_center(TilePosition {
        x: column + 2,
        y: floor - 2,
    });
    let _ = world.spawn_enemy(EnemyKind::Lodestar, lx, ly);
    let start = world.player(PLAYER).expect("rig").x;
    for _ in 0..40 {
        let _ = ticks(&mut world, 1);
    }
    let moved = world.player(PLAYER).expect("rig").x - start;
    assert!(
        moved > TILE_SIZE * 0.5,
        "pulled toward the lodestar: {moved}"
    );
}

#[test]
fn a_shear_mantis_hangs_then_drops_on_a_rig_passing_under() {
    let mut world = world_at(60);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let column = landmarks::outpost_column(&settings()) + 90;
    let row = {
        let (top, _) = layout(&settings()).rows(Stratum::GravityShear);
        top + 24
    };
    let room = TileRect::around(TilePosition { x: column, y: row }, 12, 5);
    fill(&mut world, room.expanded(2), TileKind::Stone);
    fill(&mut world, room, TileKind::Air);
    put_rig(
        &mut world,
        TilePosition {
            x: column - 10,
            y: room.max.y,
        },
    );
    let (mx, my) = world.frame().tile_center(TilePosition {
        x: column,
        y: room.max.y,
    });
    let mantis = world.spawn_enemy(EnemyKind::ShearMantis, mx, my);
    let _ = ticks(&mut world, 3);
    let (hung, state) = world.enemy_parts(mantis).expect("mantis");
    assert_eq!(state.mode, EnemyMode::Dormant);
    assert!(
        hung.y < TILE_SIZE.mul_add(-4.0, my),
        "it climbed to the ceiling"
    );
    // The rig drives under it.
    let mut dropped = false;
    for _ in 0..200 {
        let events = step(&mut world, ControlState::new(0.6, false, false));
        dropped |= distortion_events(&events).contains(&DistortionEvent::MantisDropped);
        if dropped {
            break;
        }
    }
    assert!(dropped, "it dropped on the rig");
}

#[test]
fn a_glassback_throws_frontal_shots_back() {
    let mut world = world_at(60);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let column = landmarks::outpost_column(&settings()) + 90;
    let row = {
        let (top, _) = layout(&settings()).rows(Stratum::MirrorStrata);
        top + 30
    };
    let room = TileRect::around(TilePosition { x: column, y: row }, 12, 4);
    fill(&mut world, room.expanded(2), TileKind::Stone);
    fill(&mut world, room, TileKind::Air);
    put_rig(
        &mut world,
        TilePosition {
            x: column - 6,
            y: room.max.y,
        },
    );
    let (bx, by) = world.frame().tile_center(TilePosition {
        x: column + 4,
        y: room.max.y,
    });
    let back = world.spawn_enemy(EnemyKind::Glassback, bx, by);
    world.entities.update(back, |entity| {
        if let crate::entity::EntityState::Enemy(state) = &mut entity.state {
            state.facing = -1;
        }
    });
    let output = &mut crate::event::TickOutput::default();
    let hit = world.damage_enemy(
        back,
        crate::enemy::Damage::new(
            20.0,
            crate::enemy::DamageKind::Kinetic,
            crate::enemy::DamageSource::Player(PLAYER),
        ),
        output,
    );
    assert_eq!(
        hit,
        crate::enemy::DamageOutcome::Ignored,
        "its face turned the shot"
    );
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::Distortion {
            what: DistortionEvent::ShotReflected,
            ..
        }
    )));
    // From behind it is only a crab.
    world.entities.update(back, |entity| {
        if let crate::entity::EntityState::Enemy(state) = &mut entity.state {
            state.facing = 1;
        }
    });
    let hit = world.damage_enemy(
        back,
        crate::enemy::Damage::new(
            20.0,
            crate::enemy::DamageKind::Kinetic,
            crate::enemy::DamageSource::Player(PLAYER),
        ),
        output,
    );
    assert!(matches!(hit, crate::enemy::DamageOutcome::Hurt { .. }));
}

#[test]
fn the_distortion_creatures_keep_the_body_rules_in_a_fight() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let column = landmarks::outpost_column(&settings()) + 120;
    let row = {
        let (top, _) = layout(&settings()).rows(Stratum::GravityShear);
        top + 30
    };
    let room = TileRect::around(TilePosition { x: column, y: row }, 30, 7);
    fill(&mut world, room.expanded(2), TileKind::Stone);
    fill(&mut world, room, TileKind::Air);
    put_rig(
        &mut world,
        TilePosition {
            x: column,
            y: room.max.y - 1,
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
        .filter(|kind| kind.is_distortion() && *kind != EnemyKind::MirrorTwin)
        .collect();
    for (index, kind) in (0_i32..).zip(kinds) {
        let x = room.min.x + 4 + index * 6;
        let tile = match kind.stats().locomotion {
            Locomotion::Walking | Locomotion::Stationary => TilePosition {
                x,
                y: room.max.y - 1,
            },
            _ => TilePosition {
                x,
                y: room.min.y + 2,
            },
        };
        let (cx, cy) = frame.tile_center(tile);
        let _ = world.spawn_enemy(kind, cx, cy);
    }
    let mut checked = 0_usize;
    for count in 0..30 * 20_u64 {
        let axis = if (count / 40) % 2 == 0 { 0.8 } else { -0.8 };
        let controls = ControlState::new(axis, count % 50 < 10, false).with_weapon(WeaponControl {
            fire: true,
            auto_aim: true,
            aim_x: 127,
            aim_y: 0,
        });
        let _ = step(&mut world, controls);
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
            if state.kind.stats().locomotion == Locomotion::Stationary {
                continue;
            }
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
                depth <= 2.5,
                "{:?} sits {depth} px inside the rig at tick {count}",
                state.kind
            );
            checked += 1;
        }
    }
    assert!(checked > 300, "a real fight: {checked}");
}
