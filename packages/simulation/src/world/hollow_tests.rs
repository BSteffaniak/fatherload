//! The Hollow World's mechanics in a Deep Descent world (Wave 9, Descent V): the inner day on
//! world time alone, the breakthrough's reveal, the Inner Sky's wind, riding a crestback,
//! landing upside down under an isle (and on the roof), the Sunforge's light and shade, the
//! Gravity Anchor, the creatures' contact rules, the Hollow section's replication and hash,
//! and the Sun Serpent beaten by a bot with a Descent V rig.

#![allow(
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    reason = "test arithmetic on positions and whole fights read best plainly"
)]

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WeaponControl, WorldState,
    depth_progression::RigSystem,
    descent::Stratum,
    enemy::{EnemyDifficulty, EnemyKind},
    entity::EntityId,
    hollow::{
        ANCHOR_MILLIS, EXPOSURE_SCORCH_AT, HollowAction, HollowEvent, INNER_DAWN_MILLIS,
        INNER_DAY_MILLIS, INNER_NOON_MILLIS, InnerPhase, SerpentPhase,
    },
    logistics::LogisticsCommand,
    pilot::PilotAction,
    position::WorldPos,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    weapon::WeaponKind,
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{self, hollow as places, layout},
    },
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x5744_0905;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// A Deep Descent world at `hz` with the rig fitted for Descent V (every earlier gate system,
/// every protection; the Gravity Anchor only when `anchor`), the director quiet.
fn world_at(hz: u32, anchor: bool) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    if !anchor {
        rig.hazard_gear.rig_systems &= !RigSystem::GravityAnchor.bit();
    }
    rig.radiator_level = 15;
    rig.hull_level = 15;
    rig.hull = rig.max_hull();
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

fn step(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::held(PLAYER, controls);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    output.events.into_iter().map(|event| event.kind).collect()
}

fn ticks(world: &mut WorldState, count: u64) -> Vec<GameplayEventKind> {
    (0..count)
        .flat_map(|_| step(world, ControlState::default()))
        .collect()
}

fn hollow_events(events: &[GameplayEventKind]) -> Vec<HollowEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Hollow { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

fn act(
    world: &mut WorldState,
    command: PlayerCommand,
) -> (PlayerScopedCommandOutcome, Vec<GameplayEventKind>) {
    let inputs = TickInputs::default().act(PLAYER, command);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    (
        output
            .action_outcomes
            .first()
            .map_or(PlayerScopedCommandOutcome::IgnoredUnavailable, |outcome| {
                outcome.outcome
            }),
        output.events.into_iter().map(|event| event.kind).collect(),
    )
}

fn anchor(world: &mut WorldState, action: HollowAction) -> PlayerScopedCommandOutcome {
    act(
        world,
        PlayerCommand::Logistics(LogisticsCommand::Hollow(action)),
    )
    .0
}

/// A sealed box of loam `half` tiles round `centre`, open inside.
fn chamber(world: &mut WorldState, centre: TilePosition, half: (i32, i32)) {
    fill(
        world,
        TileRect::around(centre, half.0 + 2, half.1 + 2),
        TileKind::Hollow(crate::hollow::HollowTile::Loam),
    );
    fill(
        world,
        TileRect::around(centre, half.0, half.1),
        TileKind::Air,
    );
}

/// A column of the Inner Sky's great cavern clear of floating stones in `rows` below its roof.
fn open_sky_column(rows: std::ops::Range<i32>) -> i32 {
    let settings = settings();
    let layout = layout(&settings);
    (0..400)
        .map(|step| {
            if step % 2 == 0 {
                step / 2
            } else {
                -(step / 2) - 1
            }
        })
        .find(|x| {
            rows.clone().all(|dy| {
                let y = descent::hollow_rows(&settings, &layout, *x).0 + dy;
                descent::hollow_open(&settings, &layout, *x, y)
                    && !places::sky_stone(&settings, &layout, *x, y)
            })
        })
        .expect("an open column")
}

// --- The inner day -----------------------------------------------------------------------------

#[test]
fn the_inner_day_turns_on_world_time_at_every_tick_rate() {
    for hz in [20, 30, 60, 120] {
        let mut world = world_at(hz, false);
        world.set_world_time_millis(INNER_DAY_MILLIS * 3 + INNER_DAWN_MILLIS + 1_000);
        assert_eq!(world.inner_sun_now().phase, InnerPhase::Day, "{hz} Hz");
        assert_eq!(world.inner_sun_now().light, 1_000);
        // Ninety seconds of play at any rate lands on the same light.
        let at = INNER_DAY_MILLIS * 3 + INNER_DAWN_MILLIS + INNER_NOON_MILLIS - 30_000;
        world.set_world_time_millis(at);
        let _ = ticks(&mut world, u64::from(hz) * 90);
        let sun = world.inner_sun_now();
        assert_eq!(sun.phase, InnerPhase::Dusk, "{hz} Hz");
        let expected = crate::hollow::inner_sun_at(at + 90_000);
        assert_eq!(sun, expected, "{hz} Hz");
    }
}

#[test]
fn the_breakthrough_plays_once_and_dawn_and_night_are_announced() {
    let mut world = world_at(30, false);
    let x = open_sky_column(0..10);
    let settings = settings();
    let layout = layout(&settings);
    let y = descent::hollow_rows(&settings, &layout, x).0 + 4;
    // Just before the inner night falls.
    let night_falls = INNER_DAY_MILLIS * 5
        + INNER_DAWN_MILLIS
        + INNER_NOON_MILLIS
        + crate::hollow::INNER_DUSK_MILLIS
        - 2_000;
    world.set_world_time_millis(night_falls);
    put_rig(&mut world, TilePosition { x, y });
    let events = ticks(&mut world, 3);
    assert_eq!(
        hollow_events(&events)
            .iter()
            .filter(|what| **what == HollowEvent::Breakthrough)
            .count(),
        1
    );
    assert!(world.hollow().revealed.contains_key(&PLAYER));
    let events = ticks(&mut world, 30 * 4);
    assert!(!hollow_events(&events).contains(&HollowEvent::Breakthrough));
    assert!(hollow_events(&events).contains(&HollowEvent::InnerNight));
}

// --- The Inner Sky's wind ------------------------------------------------------------------------

#[test]
fn the_wind_carries_a_flying_rig_alike_at_every_tick_rate() {
    let mut drifts = Vec::new();
    for hz in [30, 60, 120] {
        let mut world = world_at(hz, false);
        let x = open_sky_column(120..200);
        let settings = settings();
        let layout = layout(&settings);
        let y = descent::hollow_rows(&settings, &layout, x).0 + 160;
        put_rig(&mut world, TilePosition { x, y });
        let start = world.player(PLAYER).expect("rig").x;
        // Hover (thrust pulses) for four seconds.
        for tick in 0..u64::from(hz) * 4 {
            let thrust = tick % u64::from(hz / 10).max(1) < u64::from(hz / 20).max(1);
            let _ = step(&mut world, ControlState::new(0.0, thrust, false));
        }
        let drift = world.player(PLAYER).expect("rig").x - start;
        assert!(
            drift.abs() > TILE_SIZE,
            "{hz} Hz: the wind carried it {drift}"
        );
        drifts.push(drift);
    }
    // The same wind at every rate (within a little).
    let spread = drifts.iter().copied().fold(f32::MIN, f32::max)
        - drifts.iter().copied().fold(f32::MAX, f32::min);
    assert!(spread < drifts[0].abs() * 0.35, "{drifts:?}");
}

// --- Riding --------------------------------------------------------------------------------------

#[test]
fn a_pilot_climbs_onto_a_crestback_rides_it_faster_and_gets_down() {
    let mut world = world_at(60, false);
    // Creatures about (a peaceful world clears them).
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let settings = settings();
    let layout = layout(&settings);
    let (top, _) = layout.rows(Stratum::MegafaunaJungle);
    let centre = TilePosition { x: 0, y: top + 60 };
    chamber(&mut world, centre, (40, 4));
    put_rig(
        &mut world,
        TilePosition {
            x: centre.x + 30,
            y: centre.y + 3,
        },
    );
    let floor = centre.y + 4;
    let (px, feet) = world.frame().tile_center(TilePosition {
        x: centre.x - 20,
        y: floor,
    });
    let feet = TILE_SIZE.mul_add(0.5, feet);
    let _ = world.place_pilot(PLAYER, px, feet - 0.25);
    let (cx, cy) = world.frame().tile_center(TilePosition {
        x: centre.x - 20,
        y: floor,
    });
    let mount = world.spawn_enemy(EnemyKind::Crestback, cx, cy - TILE_SIZE * 0.3);
    let _ = ticks(&mut world, 30);
    // Unmounted: walk right for a second.
    let walk = |world: &mut WorldState| {
        let start = world.pilot_of(PLAYER).expect("pilot").1.x;
        for _ in 0..60 {
            let _ = step(world, ControlState::new(1.0, false, false));
        }
        world.pilot_of(PLAYER).expect("pilot").1.x - start
    };
    let on_foot = walk(&mut world);
    // Back beside the crestback, then up.
    let mount_at = world.entities().get(mount).expect("the crestback").body;
    if let Some((pilot, _, _)) = world.pilot_of(PLAYER) {
        world.edit_pilot(pilot, |body, _| {
            body.x = mount_at.x;
            body.velocity_x = 0.0;
        });
    }
    let (outcome, events) = act(&mut world, PlayerCommand::Pilot(PilotAction::Use));
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(hollow_events(&events).contains(&HollowEvent::Mounted));
    assert_eq!(world.mount_of(PLAYER), Some(mount));
    let riding = walk(&mut world);
    assert!(
        riding > on_foot * 1.4,
        "riding {riding} vs walking {on_foot}"
    );
    // The crestback is carried under its rider.
    let pilot_x = world.pilot_of(PLAYER).expect("pilot").1.x;
    let mount_x = world.entities().get(mount).expect("the crestback").body.x;
    assert!((pilot_x - mount_x).abs() < 1.0);
    let (_, events) = act(&mut world, PlayerCommand::Pilot(PilotAction::Use));
    assert!(hollow_events(&events).contains(&HollowEvent::Dismounted));
    assert_eq!(world.mount_of(PLAYER), None);
    assert_eq!(
        world.pilot_of(PLAYER).expect("pilot").2.boons,
        crate::pilot::PilotBoons::default()
    );
}

// --- The Upside-Down Isles -----------------------------------------------------------------------

/// The isle the landing tests use, with the rig `below` rows under its underside.
fn under_isle(world: &mut WorldState, below: i32) -> places::Isle {
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::UpsideDownIsles);
    let isle = places::isles_touching(
        &settings,
        &layout,
        &TileRect {
            min: TilePosition { x: -600, y: top },
            max: TilePosition { x: 600, y: bottom },
        },
    )
    .into_iter()
    .filter(|isle| isle.bounds.width() >= 16)
    .min_by_key(|isle| isle.bounds.center().x.abs())
    .expect("an isle");
    let centre = isle.bounds.min.x + isle.bounds.width() / 2;
    put_rig(
        world,
        TilePosition {
            x: centre,
            y: isle.underside() + below,
        },
    );
    isle
}

#[test]
fn under_an_isle_gravity_points_up_and_a_flipped_rig_lands_feet_first() {
    for hz in [30, 60] {
        let mut world = world_at(hz, false);
        let isle = under_isle(&mut world, 8);
        let (x, y) = {
            let rig = world.player(PLAYER).expect("rig");
            (rig.x, rig.y)
        };
        assert!(world.fields_at(x, y).gravity_y < -0.5, "gravity points up");
        // Turned half way round with the thrusters (nothing turns it for the pilot).
        world.player_mut(PLAYER).expect("rig").attitude.orientation = std::f32::consts::PI;
        let events = ticks(&mut world, u64::from(hz) * 4);
        assert!(
            hollow_events(&events).contains(&HollowEvent::LandedUpsideDown),
            "{hz} Hz: {:?}",
            hollow_events(&events)
        );
        let rig = world.player(PLAYER).expect("rig");
        assert!(
            rig.tile_in(world.frame()).y <= isle.underside() + 2,
            "it fell up"
        );
        assert!(world.hollow().isle_landings >= 1);
    }
}

#[test]
fn a_rig_not_turned_lands_on_its_roof_and_dents() {
    let mut world = world_at(60, false);
    let _ = under_isle(&mut world, 8);
    let before = world.player(PLAYER).expect("rig").hull;
    let events = ticks(&mut world, 240);
    assert!(hollow_events(&events).contains(&HollowEvent::RoofLanding));
    assert!(world.player(PLAYER).expect("rig").hull < before);
    assert!(!hollow_events(&events).contains(&HollowEvent::LandedUpsideDown));
}

// --- The Sunforge --------------------------------------------------------------------------------

/// A forge-sky tile in the light (nothing under it for the shade scan) or in a stone's shade.
fn forge_spot(world: &WorldState, shaded: bool) -> TilePosition {
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::TheSunforge);
    for y in (top + 40)..(bottom - 80) {
        for x in -300..300 {
            let tile = TilePosition { x, y };
            if !places::forge_open(&settings, &layout, x, y)
                || world.hollow_stratum_at(tile) != Some(Stratum::TheSunforge)
            {
                continue;
            }
            let stone_below = (2..10).any(|dy| !places::forge_open(&settings, &layout, x, y + dy));
            let clear_below = (1..50).all(|dy| places::forge_open(&settings, &layout, x, y + dy))
                && (1..50).all(|dy| places::forge_open(&settings, &layout, x - 8, y + dy))
                && (1..50).all(|dy| places::forge_open(&settings, &layout, x + 8, y + dy));
            if (shaded && stone_below) || (!shaded && clear_below) {
                return tile;
            }
        }
    }
    panic!("no forge spot");
}

#[test]
fn the_sunforge_heats_a_rig_in_the_light_and_its_shade_cools_it() {
    let mut world = world_at(60, false);
    world.set_world_time_millis(INNER_DAY_MILLIS * 2 + INNER_DAWN_MILLIS + 10_000);
    let lit = forge_spot(&world, false);
    // Hold the rig still in the air (a box round it keeps it there).
    put_rig(&mut world, lit);
    assert!(world.sunlit_at(lit), "in the light");
    let rig_hull = world.player(PLAYER).expect("rig").hull;
    for _ in 0..60 * 30 {
        world.teleport_rig_to(
            PLAYER,
            WorldPos::tile_center(i64::from(lit.x), i64::from(lit.y)),
        );
        let _ = step(&mut world, ControlState::default());
    }
    let hot = world.hollow().exposure_of(PLAYER);
    assert!(hot >= EXPOSURE_SCORCH_AT, "heated to {hot}");
    assert!(
        world.player(PLAYER).expect("rig").hull < rig_hull,
        "scorched"
    );
    let shade = forge_spot(&world, true);
    put_rig(&mut world, shade);
    assert!(!world.sunlit_at(shade), "in shade");
    for _ in 0..60 * 10 {
        world.teleport_rig_to(
            PLAYER,
            WorldPos::tile_center(i64::from(shade.x), i64::from(shade.y)),
        );
        let _ = step(&mut world, ControlState::default());
    }
    assert!(
        world.hollow().exposure_of(PLAYER) < hot / 2,
        "cooled in the shade"
    );
}

#[test]
fn shade_follows_the_light_and_night_is_cooler() {
    let day = crate::hollow::inner_sun_at(INNER_DAWN_MILLIS + 1);
    let night = crate::hollow::inner_sun_at(INNER_DAY_MILLIS - 1);
    assert!(day.light > night.light * 3);
    // The light's slant swings across the day, so shadows lean.
    let morning = crate::hollow::inner_sun_at(INNER_DAWN_MILLIS);
    let evening = crate::hollow::inner_sun_at(INNER_DAWN_MILLIS + INNER_NOON_MILLIS);
    assert!(morning.slant < 0 && evening.slant > 0);
}

// --- The Gravity Anchor --------------------------------------------------------------------------

#[test]
fn the_gravity_anchor_holds_a_turned_rig_to_a_wall_then_lets_go_and_recharges() {
    // Without the Sun Serpent's reward there is nothing to engage.
    let mut world = world_at(60, false);
    assert_eq!(
        anchor(&mut world, HollowAction::EngageAnchor),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
    let mut world = world_at(60, true);
    let settings = settings();
    let layout = layout(&settings);
    let (top, _) = layout.rows(Stratum::MegafaunaJungle);
    let centre = TilePosition { x: 0, y: top + 60 };
    chamber(&mut world, centre, (8, 8));
    put_rig(&mut world, centre);
    // Turned a quarter clockwise: its feet point left, to the wall.
    world.player_mut(PLAYER).expect("rig").attitude.orientation = std::f32::consts::FRAC_PI_2;
    assert_eq!(
        anchor(&mut world, HollowAction::ToggleAnchor),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.anchor_quarter_for(PLAYER), Some(1));
    let start = world.player(PLAYER).expect("rig").x;
    let _ = ticks(&mut world, 90);
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        rig.x < start - TILE_SIZE * 3.0,
        "pulled to the left wall: {} -> {}",
        start,
        rig.x
    );
    // Held against the wall, not falling.
    let y_before = rig.y;
    let _ = ticks(&mut world, 60);
    assert!((world.player(PLAYER).expect("rig").y - y_before).abs() < TILE_SIZE);
    // It lets go when its time is up, and needs a while to recharge.
    let events = ticks(&mut world, (ANCHOR_MILLIS / 1_000) * 60);
    assert!(hollow_events(&events).contains(&HollowEvent::AnchorReleased));
    assert_eq!(world.anchor_quarter_for(PLAYER), None);
    assert_eq!(
        anchor(&mut world, HollowAction::EngageAnchor),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
    let _ = ticks(&mut world, 60 * 9);
    assert_eq!(
        anchor(&mut world, HollowAction::ToggleAnchor),
        PlayerScopedCommandOutcome::Applied
    );
    // The toggle lets it go early.
    assert_eq!(
        anchor(&mut world, HollowAction::ToggleAnchor),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.anchor_quarter_for(PLAYER), None);
}

// --- Creatures -----------------------------------------------------------------------------------

#[test]
fn a_gust_bladder_bursts_on_a_rig_and_shoves_it_and_lode_mites_cling_until_shaken_off() {
    let mut world = world_at(60, false);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let settings = settings();
    let layout = layout(&settings);
    let (top, _) = layout.rows(Stratum::MegafaunaJungle);
    let centre = TilePosition { x: 0, y: top + 60 };
    chamber(&mut world, centre, (14, 8));
    put_rig(&mut world, centre);
    let (x, y) = {
        let rig = world.player(PLAYER).expect("rig");
        (rig.x, rig.y)
    };
    let _ = world.spawn_enemy(EnemyKind::GustBladder, x + 6.0, y);
    let events = ticks(&mut world, 3);
    assert!(hollow_events(&events).contains(&HollowEvent::GustBurst));
    assert!(
        world.player(PLAYER).expect("rig").velocity_x < -50.0,
        "shoved away"
    );
    // Mites.
    put_rig(&mut world, centre);
    let (x, y) = {
        let rig = world.player(PLAYER).expect("rig");
        (rig.x, rig.y)
    };
    let mite = world.spawn_enemy(EnemyKind::LodeMite, x + 8.0, y);
    let mut clung = false;
    for _ in 0..240 {
        let _ = step(&mut world, ControlState::default());
        if world.enemies().any(|(entity, state)| {
            entity.id == mite && state.phase == crate::world::LODE_MITE_CLINGING_PHASE
        }) {
            clung = true;
            break;
        }
    }
    assert!(clung, "the mite clung on");
    // A hard spin throws it off.
    let mut freed = false;
    for _ in 0..240 {
        let _ = step(
            &mut world,
            ControlState::new(0.0, true, false).with_rotation(1.0),
        );
        if world.enemies().any(|(entity, state)| {
            entity.id == mite && state.phase != crate::world::LODE_MITE_CLINGING_PHASE
        }) {
            freed = true;
            break;
        }
    }
    assert!(freed, "spun off");
}

// --- State ---------------------------------------------------------------------------------------

#[test]
fn the_hollow_section_replicates_and_hashes_only_once_something_happened() {
    let mut world = world_at(30, true);
    let quiet = world.simulation_state_hash();
    assert!(world.hollow().is_empty());
    world.player_mut(PLAYER).expect("rig").attitude.orientation = std::f32::consts::PI;
    assert_eq!(
        anchor(&mut world, HollowAction::ToggleAnchor),
        PlayerScopedCommandOutcome::Applied
    );
    assert!(!world.hollow().is_empty());
    assert_ne!(world.simulation_state_hash(), quiet);
    let section = world
        .section(crate::SectionKey::Hollow)
        .expect("the hollow section");
    let mut replica = world_at(30, true);
    replica.apply_section(crate::SectionKey::Hollow, section);
    assert_eq!(replica.hollow(), world.hollow());
    // It saves as JSON.
    let json = serde_json::to_string(world.hollow()).expect("json");
    let back: crate::hollow::Hollow = serde_json::from_str(&json).expect("back");
    assert_eq!(&back, world.hollow());
}

// --- The Sun Serpent -----------------------------------------------------------------------------

/// A rig's hull tier at the Sun Serpent (the descent probe's arrival tier in the Sunforge).
const HULL_AT_THE_SERPENT: u8 = 13;

/// The bot: keep to the well's outer ring away from the serpent's head, hide behind a pillar
/// when the sun flares, and shoot the nearest glowing scale (the head once it falls).
fn serpent_bot(world: &WorldState, tick: u64) -> ControlState {
    let centre = world.sun_well().expect("the well");
    let frame = world.frame();
    let (cx, cy) = frame.tile_center(centre);
    let rig = world.player(PLAYER).expect("rig");
    let fight = &world.hollow().serpent;
    // Where to be: in a pillar's shadow when a flare comes, else on the outer ring opposite
    // the head.
    let pillar = places::shade_pillars(centre)
        .into_iter()
        .map(|tile| frame.tile_center(tile))
        .min_by(|a, b| {
            (a.0 - rig.x)
                .hypot(a.1 - rig.y)
                .total_cmp(&(b.0 - rig.x).hypot(b.1 - rig.y))
        })
        .expect("pillars");
    let head = fight
        .head
        .and_then(|id| world.entities().get(EntityId(id)))
        .map(|entity| (entity.body.x, entity.body.y));
    let goal = if fight.flare_warned.is_some() || fight.flaring() {
        let (dx, dy) = (pillar.0 - cx, pillar.1 - cy);
        let length = dx.hypot(dy).max(1.0);
        (
            dx / length * TILE_SIZE * 2.6 + pillar.0,
            dy / length * TILE_SIZE * 2.6 + pillar.1,
        )
    } else if fight.phase == SerpentPhase::Exposed {
        // Fallen to its inner ring: in close, just outside its head, with a clear shot.
        let (hx, hy) = head.unwrap_or((cx, cy));
        let (dx, dy) = (hx - cx, hy - cy);
        let length = dx.hypot(dy).max(1.0);
        (
            dx / length * TILE_SIZE * 5.0 + hx,
            dy / length * TILE_SIZE * 5.0 + hy,
        )
    } else {
        let (hx, hy) = head.unwrap_or((cx, cy - TILE_SIZE * 15.0));
        let (dx, dy) = (cx - hx, cy - hy);
        let length = dx.hypot(dy).max(1.0);
        (
            dx / length * TILE_SIZE * 21.5 + cx,
            dy / length * TILE_SIZE * 21.5 + cy,
        )
    };
    let target = if fight.phase == SerpentPhase::Exposed {
        head
    } else {
        world
            .enemies()
            .filter(|(_, state)| {
                state.kind == EnemyKind::SerpentCoil
                    && state.phase == crate::world::SERPENT_SCALE_PHASE
            })
            .map(|(entity, _)| (entity.body.x, entity.body.y))
            .min_by(|a, b| {
                (a.0 - rig.x)
                    .hypot(a.1 - rig.y)
                    .total_cmp(&(b.0 - rig.x).hypot(b.1 - rig.y))
            })
    };
    // Round the inner sun, never through it: far round the ring, head for a point a little
    // way round toward the goal first.
    let angle = |x: f32, y: f32| (y - cy).atan2(x - cx);
    let (here, there) = (angle(rig.x, rig.y), angle(goal.0, goal.1));
    let turn = crate::field::trig::wrap_angle(there - here);
    let goal = if turn.abs() > 0.9 {
        let step = here + turn.signum() * 0.8;
        let radius = TILE_SIZE * 19.0;
        (
            step.cos().mul_add(radius, cx),
            step.sin().mul_add(radius, cy),
        )
    } else {
        goal
    };
    let (dx, dy) = (goal.0 - rig.x, goal.1 - rig.y);
    let horizontal = if dx.abs() > TILE_SIZE * 0.5 {
        (dx / (TILE_SIZE * 3.0)).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let thrust = dy < -TILE_SIZE * 0.3 || rig.velocity_y > 120.0;
    let aim = target.map_or((0, -127), |(tx, ty)| {
        let (ax, ay) = (tx - rig.x, ty - rig.y);
        let length = ax.hypot(ay).max(1.0);
        #[allow(clippy::cast_possible_truncation, reason = "a unit vector times 127")]
        ((ax / length * 127.0) as i8, (ay / length * 127.0) as i8)
    });
    ControlState::new(horizontal, thrust, false).with_weapon(WeaponControl {
        fire: target.is_some() && tick % 6 < 3,
        auto_aim: false,
        aim_x: aim.0,
        aim_y: aim.1,
    })
}

#[test]
#[allow(clippy::too_many_lines, reason = "the whole fight, start to end")]
fn a_bot_beats_the_sun_serpent_and_the_crew_gets_the_gravity_anchor() {
    let hz = 30;
    let mut world = world_at(hz, false);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let centre = world.sun_well().expect("the well");
    let start = places::shade_pillars(centre)[2];
    put_rig(
        &mut world,
        TilePosition {
            x: start.x - 3,
            y: start.y - 3,
        },
    );
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        crate::economy::max_out_upgrades(rig);
        rig.hazard_gear.rig_systems &= !RigSystem::GravityAnchor.bit();
        rig.weapon_rack.battery_level = crate::weapon::MAX_BATTERY_LEVEL;
        let _ = rig.receive_weapon(WeaponKind::Railgun);
        let _ = rig.mount_weapon(Some(WeaponKind::Railgun));
        // The hull a rig reaches the Sunforge with, not the test rig's top tier.
        rig.hull_level = HULL_AT_THE_SERPENT;
        rig.hull = rig.max_hull();
    }
    let events = ticks(&mut world, 2);
    assert!(hollow_events(&events).contains(&HollowEvent::SerpentAwakened));
    assert!(world.hollow().serpent.awake());
    assert_eq!(
        world.hollow().serpent.coils.len(),
        crate::hollow::SERPENT_COILS
    );
    // Armoured: a hit on the head does nothing while a scale is whole.
    let head = EntityId(world.hollow().serpent.head.expect("the head"));
    let mut output = crate::TickOutput::default();
    assert_eq!(
        world.damage_enemy(
            head,
            crate::enemy::Damage::new(
                500.0,
                crate::enemy::DamageKind::Kinetic,
                crate::enemy::DamageSource::Player(PLAYER)
            ),
            &mut output
        ),
        crate::enemy::DamageOutcome::Ignored
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut defeated = false;
    let (mut exposed_ticks, mut near_sum, mut head_hits, mut shots) = (0_u32, 0.0_f32, 0, 0);
    let mut fight_ticks = 0_u64;
    for count in 0..u64::from(hz) * 900 {
        fight_ticks = count + 1;
        let controls = serpent_bot(&world, count);
        let events = step(&mut world, controls);
        if world.hollow().serpent.phase == SerpentPhase::Exposed {
            exposed_ticks += 1;
            let rig = world.player(PLAYER).expect("rig");
            if let Some(head) = world
                .hollow()
                .serpent
                .head
                .and_then(|id| world.entities().get(EntityId(id)))
            {
                near_sum += (head.body.x - rig.x).hypot(head.body.y - rig.y) / TILE_SIZE;
            }
        }
        head_hits += events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    GameplayEventKind::EnemyHurt {
                        kind: EnemyKind::SunSerpent,
                        ..
                    }
                )
            })
            .count();
        shots += events
            .iter()
            .filter(|event| matches!(event, GameplayEventKind::WeaponFired { .. }))
            .count();
        for what in hollow_events(&events) {
            seen.insert(what);
            if what == HollowEvent::SerpentDefeated {
                defeated = true;
            }
        }
        if defeated {
            break;
        }
        assert!(
            world.player(PLAYER).expect("rig").hull > 0.0,
            "the bot survived (tick {count}, seen {seen:?}, scales {:?}, head {:?}, exposures {}, exposed ticks {exposed_ticks} mean distance {:.1}, head hits {head_hits}, shots {shots})",
            world.hollow().serpent.scales,
            world
                .hollow()
                .serpent
                .head
                .and_then(|id| world.entities().get(EntityId(id)))
                .and_then(|entity| entity.state.health()),
            world.hollow().serpent.cycles,
            near_sum / exposed_ticks.max(1) as f32
        );
    }
    let rig = world.player(PLAYER).expect("rig");
    let left = rig.hull / rig.max_hull();
    assert!(
        defeated,
        "the bot beat the serpent (seen {seen:?}, scales {:?}, hull {:.0} %, cycles {}, exposed ticks {exposed_ticks} mean distance {:.1}, head hits {head_hits}, shots {shots}, head {:?})",
        world.hollow().serpent.scales,
        left * 100.0,
        world.hollow().serpent.cycles,
        near_sum / exposed_ticks.max(1) as f32,
        world
            .hollow()
            .serpent
            .head
            .and_then(|id| world.entities().get(EntityId(id)))
            .and_then(|entity| entity.state.health())
            .map(|health| health.current)
    );
    // The probe models the fight from this measured win (Wave 10 polish).
    #[allow(clippy::cast_precision_loss, reason = "a few thousand ticks")]
    crate::balance::descent::mechanics::record_bot_fight(
        crate::descent::DescentBoss::SunSerpent,
        fight_ticks as f64 / f64::from(hz),
        f64::from(left),
    );
    assert!(
        left > 0.2,
        "with a margin: {:.0} % of the hull left",
        left * 100.0
    );
    for beat in [
        HollowEvent::ScaleBroken,
        HollowEvent::SerpentExposed,
        HollowEvent::SerpentTell,
        HollowEvent::FlareWarning,
    ] {
        assert!(seen.contains(&beat), "{beat:?} in {seen:?}");
    }
    assert!(world.hollow().serpent.beaten);
    assert!(rig.hazard_gear.has_system(RigSystem::GravityAnchor));
    // The Strange Layers' gate opens.
    assert!(crate::depth_progression::missing_gate(rig.hazard_gear.rig_systems, 5).is_none());
    let _ = ticks(&mut world, 30);
    assert!(!world.hollow().serpent.awake());
}

#[test]
fn on_a_peaceful_world_the_sun_well_yields_its_gate_unfought() {
    let mut world = world_at(30, false);
    let centre = world.sun_well().expect("the well");
    let pillar = places::shade_pillars(centre)[0];
    put_rig(
        &mut world,
        TilePosition {
            x: pillar.x,
            y: pillar.y - 4,
        },
    );
    let events = ticks(&mut world, 3);
    assert!(hollow_events(&events).contains(&HollowEvent::SerpentDefeated));
    assert!(world.hollow().serpent.beaten);
    assert!(
        world
            .player(PLAYER)
            .expect("rig")
            .hazard_gear
            .has_system(RigSystem::GravityAnchor)
    );
}

#[test]
fn older_worlds_never_see_the_hollow_world() {
    let mut world = WorldState::new_game(PLAYER);
    let quiet = world.simulation_state_hash();
    let _ = ticks(&mut world, 120);
    assert!(world.hollow().is_empty());
    assert!(world.sun_well().is_none());
    let _ = quiet;
}

#[test]
fn a_shade_canopy_from_the_kit_shades_a_rig_as_the_light_starts_to_scorch() {
    use crate::workshop::DeepSupply;
    let mut world = world_at(60, false);
    world.set_world_time_millis(INNER_DAY_MILLIS * 2 + INNER_DAWN_MILLIS + 10_000);
    let lit = forge_spot(&world, false);
    put_rig(&mut world, lit);
    let _ = world
        .logistics
        .add_supplies(PLAYER, DeepSupply::ShadeCanopy, 1);
    let mut shaded = false;
    for _ in 0..60 * 30 {
        world.teleport_rig_to(
            PLAYER,
            WorldPos::tile_center(i64::from(lit.x), i64::from(lit.y)),
        );
        let _ = step(&mut world, ControlState::default());
        if !shaded && world.supplies_of(PLAYER, DeepSupply::ShadeCanopy) == 0 {
            shaded = true;
            assert!(
                world.hollow().exposure_of(PLAYER) < EXPOSURE_SCORCH_AT / 2,
                "the canopy cooled the rig"
            );
        }
    }
    assert!(shaded, "the canopy was thrown as the scorch began");
    // Without another the light heats it again.
    assert!(world.hollow().exposure_of(PLAYER) > 0, "it heats again");
}
