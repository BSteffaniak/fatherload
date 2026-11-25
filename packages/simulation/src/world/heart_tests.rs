//! Descent VIII in a Deep Descent world (Wave 10): dark matter's inverted cargo physics,
//! gravity wells at every tick rate, the magnetic rails, the Heart's fight beaten by a bot with
//! full kit, `space_unlocked` and the Star Drive saved and replicated, the ending's shield and
//! the Star Drive's boost in the sky.

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WeaponControl,
    WorldState,
    depth_progression::RigSystem,
    descent::Stratum,
    enemy::{Damage, DamageKind, DamageOutcome, DamageSource, EnemyDifficulty},
    entity::EntityId,
    heart::{
        CHAMBER_RADIUS, COIL_COUNT, CoreTile, ENDING_SHIELD_MILLIS, FLOOR_BELOW_TILES, HeartEvent,
        HeartPhase, WEIGHTLESS_LUMPS,
    },
    ids::SimulationTick,
    item::ItemKind,
    position::WorldPos,
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    weapon::WeaponKind,
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{heart as places, layout},
    },
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x4EA7_7E57;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// Every gate's reward through the Heart Key: what a rig holds on reaching Descent VIII.
fn descent_eight_systems() -> u8 {
    RigSystem::ALL
        .into_iter()
        .fold(0, |bits, system| bits | system.bit())
}

/// A Deep Descent world at `hz`, the rig fitted with the full kit and the director quiet.
fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hazard_gear.rig_systems = descent_eight_systems();
    rig.hull = rig.max_hull();
    rig.cargo_capacity = 400;
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

fn heart_events(events: &[GameplayEventKind]) -> Vec<HeartEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Heart { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

fn rig_xy(world: &WorldState) -> (f32, f32) {
    let rig = world.player(PLAYER).expect("rig");
    (rig.x, rig.y)
}

fn middle_row(stratum: Stratum) -> i32 {
    let (top, bottom) = layout(&settings()).rows(stratum);
    top + (bottom - top) / 2
}

/// A tall open box in Dark Matter Veins (rock round it), the rig in its middle.
fn dark_box(world: &mut WorldState) -> TilePosition {
    let middle = TilePosition {
        x: 2_000,
        y: middle_row(Stratum::DarkMatterVeins),
    };
    let rect = TileRect::around(middle, 6, 20);
    fill(world, rect.expanded(2), TileKind::Core(CoreTile::NullStone));
    fill(world, rect, TileKind::Air);
    put_rig(world, middle);
    middle
}

fn bay(world: &mut WorldState, dark: u32, ore: u32) {
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.cargo_bay = crate::item::ItemContainer::default();
    if dark > 0 {
        rig.cargo_bay
            .add(ItemKind::Layer(Stratum::DarkMatterVeins), dark);
    }
    if ore > 0 {
        rig.cargo_bay.add(MineralKind::Iron, ore);
    }
}

// --- Dark matter --------------------------------------------------------------------------------

/// How far (pixels, positive down) the rig moves in `seconds` with `dark` lumps and `ore` units
/// aboard, from the middle of the dark box, at `hz`.
fn drift(hz: u32, dark: u32, ore: u32, seconds: u32) -> (f32, Vec<HeartEvent>) {
    let mut world = world_at(hz);
    let _ = dark_box(&mut world);
    bay(&mut world, dark, ore);
    let start = rig_xy(&world).1;
    let events = heart_events(&ticks(&mut world, u64::from(hz * seconds)));
    (rig_xy(&world).1 - start, events)
}

#[test]
fn dark_matter_makes_a_rig_lighter_until_it_falls_up_and_ore_brings_it_down() {
    // Empty: the classic fall.
    let (empty, _) = drift(30, 0, 0, 1);
    assert!(empty > TILE_SIZE * 2.0, "an empty rig falls ({empty})");
    // A little dark matter: it falls slower.
    let (light, _) = drift(30, 4, 0, 1);
    assert!(
        light > 0.0 && light < empty * 0.8,
        "lighter: {light} vs {empty}"
    );
    // Too much: it falls up, and the crew hears it.
    let (up, events) = drift(30, WEIGHTLESS_LUMPS + 4, 0, 1);
    assert!(up < -TILE_SIZE, "it falls up ({up})");
    assert!(events.contains(&HeartEvent::RigFloating));
    // Ordinary ore weighs it back down: plan the load.
    let (planned, _) = drift(30, WEIGHTLESS_LUMPS + 4, 60, 1);
    assert!(planned > 0.0, "with ore aboard it falls again ({planned})");
    // Every unit of ore counts in full (dark matter takes no bay slot, so none come off): 20
    // ore make up for 4 extra lumps exactly.
    let mut world = world_at(30);
    bay(&mut world, WEIGHTLESS_LUMPS + 4, 20);
    assert_eq!(
        world.cargo_mass_of(PLAYER),
        crate::heart::cargo_mass_permille(WEIGHTLESS_LUMPS + 4, 20)
    );
    assert_eq!(world.cargo_mass_of(PLAYER), 0);
    // The same at every tick rate.
    let (up_60, _) = drift(60, WEIGHTLESS_LUMPS + 4, 0, 1);
    assert!(
        (up - up_60).abs() < TILE_SIZE * 0.75,
        "30 Hz {up} vs 60 Hz {up_60}"
    );
}

#[test]
fn a_light_rig_turns_freely_and_dropping_the_dark_matter_settles_it() {
    let mut world = world_at(30);
    let _ = dark_box(&mut world);
    bay(&mut world, WEIGHTLESS_LUMPS + 4, 0);
    let _ = ticks(&mut world, 20);
    assert!(world.heart().floating.contains(&PLAYER));
    // Weightless in the middle of the box, the player rolls it right over with the rotation
    // thrusters (beyond the usual lean).
    let middle = dark_box(&mut world);
    put_rig(&mut world, middle);
    bay(&mut world, WEIGHTLESS_LUMPS, 0);
    let mut events = Vec::new();
    for _ in 0..150 {
        events.extend(heart_events(&tick_with(
            &mut world,
            ControlState::default().with_rotation(1.0),
        )));
    }
    let orientation = world
        .player(PLAYER)
        .expect("rig")
        .attitude
        .orientation
        .abs();
    assert!(orientation > 1.2, "rolled over: {orientation}");
    // No longer falling up: it settled.
    assert!(events.contains(&HeartEvent::RigSettled));
    bay(&mut world, 0, 0);
    let _ = ticks(&mut world, 2);
    assert!(world.heart().floating.is_empty());
}

// --- Gravity wells ------------------------------------------------------------------------------

/// A generated well near the middle of the Singularity Fields.
fn a_well() -> places::Well {
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::SingularityFields);
    let area = TileRect {
        min: TilePosition { x: -300, y: top },
        max: TilePosition { x: 300, y: bottom },
    };
    places::wells_touching(&settings, &layout, &area)
        .into_iter()
        .find(|well| well.radius >= 8 && well.radius < places::LENS_RADIUS)
        .expect("a well")
}

/// The rig's distance (pixels) from the well's point after `seconds` with no input, starting
/// `offset` tiles east of it.
fn fall_into(hz: u32, well: places::Well, offset: i32, seconds: u32) -> (f32, f32, f32) {
    let mut world = world_at(hz);
    put_rig(
        &mut world,
        TilePosition {
            x: well.centre.x + offset,
            y: well.centre.y,
        },
    );
    let _ = ticks(&mut world, u64::from(hz * seconds));
    let (x, y) = rig_xy(&world);
    let (cx, cy) = world.frame().tile_center(well.centre);
    ((x - cx).hypot(y - cy), x, y)
}

#[test]
fn a_gravity_well_pulls_a_rig_in_deterministically_at_every_tick_rate() {
    let well = a_well();
    let offset = well.radius - 2;
    #[allow(clippy::cast_precision_loss, reason = "a few tiles")]
    let start = offset as f32 * TILE_SIZE;
    let (near_30, x_a, y_a) = fall_into(30, well, offset, 1);
    assert!(
        near_30 < start - TILE_SIZE,
        "pulled in: {near_30} from {start}"
    );
    // Bit for bit the same run twice.
    let (_, x_b, y_b) = fall_into(30, well, offset, 1);
    assert_eq!(
        (x_a.to_bits(), y_a.to_bits()),
        (x_b.to_bits(), y_b.to_bits())
    );
    // And alike at another tick rate.
    let (near_60, _, _) = fall_into(60, well, offset, 1);
    assert!(
        (near_30 - near_60).abs() < TILE_SIZE,
        "30 Hz {near_30} vs 60 Hz {near_60}"
    );
    // The field bends light near the point and lets the rig turn.
    let world = world_at(30);
    let (cx, cy) = world.frame().tile_center(well.centre);
    let fields = world.fields_at(TILE_SIZE.mul_add(3.0, cx), cy);
    assert!(
        fields.gravity_x < -0.5,
        "toward the point: {}",
        fields.gravity_x
    );
    assert!(
        fields.screen[crate::field::ScreenEffect::Lensing.index()] > 0.3,
        "lensing near the point"
    );
}

#[test]
fn the_horizon_crushes_a_hull_that_sits_in_a_well() {
    let well = a_well();
    let mut world = world_at(30);
    put_rig(&mut world, well.centre);
    let before = world.player(PLAYER).expect("rig").hull;
    let events = heart_events(&ticks(&mut world, 60));
    let after = world.player(PLAYER).expect("rig").hull;
    assert!(after < before, "{before} -> {after}");
    assert!(events.contains(&HeartEvent::WellCaught));
    assert!(events.contains(&HeartEvent::HorizonCrush));
}

// --- The magnetic rails -------------------------------------------------------------------------

#[test]
fn a_magnetic_rail_ride_boosts_switches_to_the_low_line_jumps_and_catches() {
    let hz = 30;
    let settings = settings();
    let layout = layout(&settings);
    let (upper, lower) = places::rail_rows(&settings, &layout);
    let basin = places::basin_at(&settings, 600);
    let mut world = world_at(hz);
    // On the upper line west of the boosters, resting on the rail.
    put_rig(
        &mut world,
        TilePosition {
            x: basin.start + basin.pillar + 2,
            y: upper - 1,
        },
    );
    let _ = ticks(&mut world, 10);
    let mut boarded = false;
    let mut boosted = false;
    let mut switched = false;
    let mut caught = false;
    let mut lowest_row = upper - 1;
    let mut top_speed = 0.0_f32;
    for _ in 0..hz * 30 {
        let tile = world.player(PLAYER).expect("rig").tile_in(world.frame());
        // Hold down at the switch for the low line.
        let down = tile.x >= basin.junctions.0 - 3 && tile.x <= basin.junctions.0 + 1;
        let events = tick_with(&mut world, ControlState::new(1.0, false, down));
        boarded |= events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::CartBoarded { .. }));
        switched |= events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::CartSwitched { .. }));
        let heart = heart_events(&events);
        boosted |= heart.contains(&HeartEvent::RailBoost);
        caught |= heart.contains(&HeartEvent::RailCatch);
        let rig = world.player(PLAYER).expect("rig");
        top_speed = top_speed.max(rig.velocity_x);
        lowest_row = lowest_row.max(rig.tile_in(world.frame()).y);
        if tile.x > basin.gap + 12 {
            break;
        }
    }
    assert!(boarded, "the rig boarded the rail");
    assert!(boosted, "a booster threw it to speed");
    assert!(
        top_speed
            > f32::from(u16::try_from(crate::heart::BOOSTER_SPEED / 100).unwrap()) / 10.0
                * TILE_SIZE
                * 0.9,
        "fast: {top_speed}"
    );
    assert!(switched, "the switch took the low line");
    assert!(
        lowest_row >= lower - 1,
        "it rode the low line (row {lowest_row})"
    );
    assert!(caught, "the magnetic catch took it over the jump");
    assert!(world.player(PLAYER).expect("rig").hull > 0.0);
}

// --- The Heart's fight --------------------------------------------------------------------------

fn chamber_floor(world: &WorldState, dx: i32) -> TilePosition {
    let centre = world.heart_centre().expect("the heart");
    TilePosition {
        x: centre.x + dx,
        y: centre.y + FLOOR_BELOW_TILES - 1,
    }
}

#[test]
fn the_heart_wakes_for_a_rig_in_its_chamber_and_sleeps_when_left() {
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let floor = chamber_floor(&world, -14);
    put_rig(&mut world, floor);
    let events = heart_events(&ticks(&mut world, 2));
    assert!(events.contains(&HeartEvent::HeartStirred));
    assert_eq!(world.heart().fight.phase, HeartPhase::Coils);
    assert_eq!(world.heart().fight.coils_left(), COIL_COUNT);
    // Sealed while its coils orbit.
    let core = EntityId(world.heart().fight.entity.expect("the core"));
    let mut output = crate::TickOutput::default();
    assert_eq!(
        world.damage_enemy(
            core,
            Damage::new(500.0, DamageKind::Piercing, DamageSource::Player(PLAYER)),
            &mut output
        ),
        DamageOutcome::Ignored
    );
    // Leave it alone: it sleeps and heals.
    let centre = world.heart_centre().expect("the heart");
    put_rig(
        &mut world,
        TilePosition {
            x: centre.x,
            y: centre.y - CHAMBER_RADIUS - 12,
        },
    );
    fill(
        &mut world,
        TileRect::around(
            TilePosition {
                x: centre.x,
                y: centre.y - CHAMBER_RADIUS - 12,
            },
            2,
            2,
        ),
        TileKind::Air,
    );
    let events = heart_events(&ticks(&mut world, 30 * 22));
    assert!(events.contains(&HeartEvent::HeartSlept));
    assert_eq!(world.heart().fight.phase, HeartPhase::Asleep);
    assert!(world.entities().get(core).is_none());
}

/// The bot: keep to the floor away from the core, shoot the nearest coil while they orbit,
/// then the core; hop now and then.
fn heart_bot(world: &WorldState, target: (f32, f32), tick: u64) -> ControlState {
    let rig = world.player(PLAYER).expect("rig");
    let centre = world
        .frame()
        .tile_center(world.heart_centre().expect("the heart"));
    let (dx, dy) = (target.0 - rig.x, target.1 - rig.y);
    let length = dx.hypot(dy).max(1.0);
    #[allow(clippy::cast_possible_truncation, reason = "a unit vector times 127")]
    let aim = ((dx / length * 127.0) as i8, (dy / length * 127.0) as i8);
    // Keep to the floor 5-10 tiles out from under the core (clear of the ledges overhead and
    // under the coils' orbit), on whichever side the rig is; pace to spoil the bolts' aim.
    let side = if rig.x < centre.0 { -1.0 } else { 1.0 };
    let from_core = (rig.x - centre.0).abs() / TILE_SIZE;
    let strafe = if from_core < 5.0 {
        side
    } else if from_core > 10.0 {
        -side
    } else if (tick / 25).is_multiple_of(2) {
        0.8
    } else {
        -0.8
    };
    // A pull drags it up toward the core: thrusting would only help it, so it holds the
    // floor; a little hop now and then spoils the bolts' aim.
    let pulled = world.heart().fight.phase.eq(&HeartPhase::Collapse)
        || world.heart().fight.phase == HeartPhase::Coils;
    let floor = TILE_SIZE.mul_add(
        f32::from(i16::try_from(FLOOR_BELOW_TILES).unwrap()) - 1.0,
        centre.1,
    );
    let thrust = !pulled && tick % 90 < 3 && rig.y > floor - TILE_SIZE;
    ControlState::new(strafe, thrust, false).with_weapon(WeaponControl {
        fire: tick % 6 < 3,
        auto_aim: false,
        aim_x: aim.0,
        aim_y: aim.1,
    })
}

/// The bot's target: the nearest coil while they orbit, else the core.
fn heart_target(world: &WorldState) -> Option<(f32, f32)> {
    let rig = rig_xy(world);
    let fight = &world.heart().fight;
    let coil = fight
        .coils
        .iter()
        .flatten()
        .filter_map(|id| world.entities().get(EntityId(*id)))
        .map(|entity| (entity.body.x, entity.body.y))
        .min_by(|a, b| {
            (a.0 - rig.0)
                .hypot(a.1 - rig.1)
                .total_cmp(&(b.0 - rig.0).hypot(b.1 - rig.1))
        });
    coil.or_else(|| {
        fight
            .entity
            .and_then(|id| world.entities().get(EntityId(id)))
            .map(|entity| (entity.body.x, entity.body.y))
    })
}

#[test]
#[allow(clippy::too_many_lines, reason = "the whole fight, start to end")]
fn a_bot_with_full_kit_wakes_the_heart_through_all_three_phases() {
    let hz = 30;
    let mut world = world_at(hz);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let floor = chamber_floor(&world, -15);
    put_rig(&mut world, floor);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.weapon_rack.battery_level = crate::weapon::MAX_BATTERY_LEVEL;
        let _ = rig.receive_weapon(WeaponKind::Railgun);
        // The full kit: the top Mk.
        rig.weapon_rack
            .owned
            .insert(WeaponKind::Railgun, crate::weapon::MAX_WEAPON_MK);
        let _ = rig.mount_weapon(Some(WeaponKind::Railgun));
        rig.hull = rig.max_hull();
    }
    let events = heart_events(&ticks(&mut world, 2));
    assert!(events.contains(&HeartEvent::HeartStirred));
    let mut phases = vec![world.heart().fight.phase];
    let mut seen = Vec::new();
    let mut pulse_told = false;
    let mut pull_after_tell = true;
    let mut exhales = 0;
    let mut woke = false;
    let mut count = 0_u64;
    while count < u64::from(hz) * 1_200 {
        let Some(target) = heart_target(&world) else {
            break;
        };
        let controls = heart_bot(&world, target, count);
        let events = heart_events(&tick_with(&mut world, controls));
        for event in &events {
            match event {
                HeartEvent::PulseTell => pulse_told = true,
                HeartEvent::Pulse => pull_after_tell &= pulse_told,
                HeartEvent::Exhale => exhales += 1,
                _ => {}
            }
        }
        seen.extend(events.iter().copied());
        let phase = world.heart().fight.phase;
        if phases.last() != Some(&phase) {
            phases.push(phase);
        }
        if events.contains(&HeartEvent::HeartAwakened) {
            woke = true;
            break;
        }
        assert!(
            world.player(PLAYER).expect("rig").hull > 0.0,
            "the bot survived (phases {phases:?}, {count} ticks)"
        );
        count += 1;
    }
    assert!(
        woke,
        "the bot woke the Heart (phases {phases:?}, {count} ticks)"
    );
    assert_eq!(
        phases,
        vec![
            HeartPhase::Coils,
            HeartPhase::Ignition,
            HeartPhase::Collapse,
            HeartPhase::Awakened
        ]
    );
    assert!(pulse_told && pull_after_tell, "every pull was told first");
    assert!(exhales >= 1, "it was struck while it exhaled");
    assert!(seen.contains(&HeartEvent::CoilBroken));
    assert!(seen.contains(&HeartEvent::CableLit));
    assert!(seen.contains(&HeartEvent::SpaceUnlocked));
    let rig = world.player(PLAYER).expect("rig");
    let left = rig.hull / rig.max_hull();
    assert!(
        left > 0.2,
        "with a margin: {:.0} % of the hull left",
        left * 100.0
    );
    // The cable lit, space is open, the Star Drive is the rig's.
    assert!(world.space_unlocked());
    assert!(world.has_star_drive(PLAYER));
    assert!(world.space_access(PLAYER).may_leave());
    assert!(world.ending_elapsed().is_some());
    // It stays awake: the chamber is calm.
    let _ = ticks(&mut world, 30);
    assert!(world.heart().fight.beaten);
    assert!(world.heart().fight.entity.is_none());
}

#[test]
fn a_peaceful_world_yields_the_heart_unfought() {
    let mut world = world_at(30);
    let floor = chamber_floor(&world, 12);
    put_rig(&mut world, floor);
    let events = heart_events(&ticks(&mut world, 3));
    assert!(events.contains(&HeartEvent::HeartAwakened));
    assert!(world.space_unlocked());
}

// --- Space, the Star Drive and the ending -------------------------------------------------------

#[test]
fn space_unlocked_and_the_star_drive_save_replicate_and_reach_later_joiners() {
    let mut world = world_at(30);
    let quiet = world.simulation_state_hash();
    assert!(world.heart().is_empty());
    world.stage_heart_phase(HeartPhase::Awakened, 0);
    assert!(world.space_unlocked());
    assert!(world.has_star_drive(PLAYER));
    assert_ne!(world.simulation_state_hash(), quiet);
    // Replicated as the shared Heart section.
    let section = world
        .section(crate::SectionKey::Heart)
        .expect("the heart section");
    let mut replica = world_at(30);
    replica.apply_section(crate::SectionKey::Heart, section);
    assert_eq!(replica.heart(), world.heart());
    assert!(replica.space_unlocked());
    assert!(crate::SectionKey::SHARED.contains(&crate::SectionKey::Heart));
    // Saved with the world.
    let json = serde_json::to_string(&world).expect("json");
    let back: WorldState = serde_json::from_str(&json).expect("back");
    assert!(back.space_unlocked());
    assert!(back.has_star_drive(PLAYER));
    assert_eq!(
        back.heart().cable_lit_millis,
        world.heart().cable_lit_millis
    );
    // A rig joining later is fitted too.
    let joiner = PlayerId::new(1);
    world.add_player(joiner);
    let events = heart_events(&ticks(&mut world, 2));
    assert!(events.contains(&HeartEvent::StarDriveFitted));
    assert!(world.has_star_drive(joiner));
}

#[test]
fn nobody_is_hurt_while_the_ending_plays_then_play_goes_on() {
    let mut world = world_at(30);
    world.stage_heart_phase(HeartPhase::Awakened, 0);
    assert!(world.heart_shields_rigs());
    let hull = world.player(PLAYER).expect("rig").hull;
    assert!(!world.damage_player(PLAYER, 50.0));
    assert!((world.player(PLAYER).expect("rig").hull - hull).abs() < f32::EPSILON);
    // After the sequence, hulls take damage again (the game keeps going).
    let later = world.simulation_tick().get() + ENDING_SHIELD_MILLIS * 30 / 1_000 + 30;
    world.set_simulation_tick(SimulationTick::new(later));
    assert!(!world.heart_shields_rigs());
    assert!(world.damage_player(PLAYER, 50.0));
}

#[test]
fn the_star_drive_boosts_a_rig_in_the_sky_and_the_stars_open_over_the_moon() {
    let climb = |drive: bool| {
        let mut world = world_at(30);
        if drive {
            world.stage_heart_phase(HeartPhase::Awakened, 0);
        }
        let spawn = world.player(PLAYER).expect("rig").tile_in(world.frame());
        put_rig(&mut world, TilePosition { x: spawn.x, y: -60 });
        let start = rig_xy(&world).1;
        for _ in 0..30 {
            let _ = tick_with(&mut world, ControlState::new(0.0, true, false));
        }
        start - rig_xy(&world).1
    };
    let plain = climb(false);
    let boosted = climb(true);
    assert!(
        boosted > plain * 1.4,
        "the Star Drive climbs faster: {boosted} vs {plain}"
    );
    // Over the Moon after the ending: the stars are open (once).
    let mut world = world_at(30);
    world.stage_heart_phase(HeartPhase::Awakened, 0);
    let moon_top = world
        .cable_path()
        .and_then(|path| path.moon_top)
        .expect("a moon");
    put_rig(
        &mut world,
        TilePosition {
            x: 0,
            y: moon_top - 60,
        },
    );
    let events = heart_events(&ticks(&mut world, 2));
    assert!(events.contains(&HeartEvent::StarsOpen));
    let events = heart_events(&ticks(&mut world, 2));
    assert!(!events.contains(&HeartEvent::StarsOpen));
}

#[test]
fn the_heart_seal_yields_only_to_the_heart_key() {
    let mut world = world_at(30);
    assert!(!world.heart_seal_refuses(PLAYER, TileKind::Core(CoreTile::HeartSeal)));
    world
        .player_mut(PLAYER)
        .expect("rig")
        .hazard_gear
        .rig_systems &= !RigSystem::HeartKey.bit();
    assert!(world.heart_seal_refuses(PLAYER, TileKind::Core(CoreTile::HeartSeal)));
    assert!(!world.heart_seal_refuses(PLAYER, TileKind::Core(CoreTile::StarRock)));
}

#[test]
fn older_worlds_never_see_the_heart() {
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(SEED),
        SimulationParams::new(30),
    );
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.cargo_bay
        .add(ItemKind::Layer(Stratum::DarkMatterVeins), 20);
    assert_eq!(
        world.cargo_mass_of(PLAYER),
        crate::heart::cargo_mass_permille(20, 0)
    );
    let _ = ticks(&mut world, 30);
    assert!(world.heart().is_empty());
    assert!(world.heart_centre().is_none());
    assert!(!world.space_unlocked());
}

// --- The creatures --------------------------------------------------------------------------------

#[test]
#[allow(clippy::too_many_lines)]
fn the_hearts_creatures_keep_the_body_rules() {
    use crate::{
        enemy::{EnemyKind, Locomotion},
        physics,
    };
    let hz = 30;
    let mut world = world_at(hz);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let row = middle_row(Stratum::DarkMatterVeins) + 60;
    let (left, right, top, floor) = (2_140, 2_220, row - 10, row + 8);
    put_rig(
        &mut world,
        TilePosition {
            x: 2_180,
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
        TileKind::Core(CoreTile::NullStone),
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
    put_rig(
        &mut world,
        TilePosition {
            x: 2_180,
            y: floor - 1,
        },
    );
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        let _ = rig.receive_weapon(WeaponKind::RivetGun);
        let _ = rig.mount_weapon(Some(WeaponKind::RivetGun));
        rig.weapon.energy = rig.battery_capacity();
        // Dark matter aboard: the moths come for it.
        rig.cargo_bay
            .add(ItemKind::Layer(Stratum::DarkMatterVeins), 3);
    }
    let frame = world.frame();
    let kinds: Vec<EnemyKind> = EnemyKind::ALL
        .into_iter()
        .filter(|kind| kind.is_heart())
        // The Heart's core and coils are held by its fight in its chamber.
        .filter(|kind| !matches!(kind, EnemyKind::StarHeart | EnemyKind::DriveCoil))
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
                "{:?} #{} sits {depth} px inside the rig at tick {count}: {:?} {:?}",
                state.kind,
                entity.id.get(),
                (entity.body.x, entity.body.y),
                state.mode
            );
            checked += 1;
        }
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = rig.max_hull();
    }
    assert!(checked > 1_000, "a real fight: {checked}");
}

#[test]
fn a_null_moth_drinks_dark_matter_from_the_bay() {
    use crate::enemy::EnemyKind;
    let mut world = world_at(30);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let middle = dark_box(&mut world);
    // Weightless: the rig hangs in the middle of the box while the moth comes.
    bay(&mut world, WEIGHTLESS_LUMPS, 0);
    let (x, y) = world.frame().tile_center(TilePosition {
        x: middle.x + 3,
        y: middle.y,
    });
    let _ = world.spawn_enemy(EnemyKind::NullMoth, x, y);
    let mut drank = false;
    for _ in 0..30 * 10 {
        let events = heart_events(&tick_with(&mut world, ControlState::default()));
        drank |= events.contains(&HeartEvent::MothDrank);
        if drank {
            break;
        }
    }
    assert!(drank, "the moth drank a lump");
    let left = world
        .player(PLAYER)
        .expect("rig")
        .cargo_bay
        .count(ItemKind::Layer(Stratum::DarkMatterVeins));
    assert!(left < WEIGHTLESS_LUMPS, "{left} lumps left");
}
