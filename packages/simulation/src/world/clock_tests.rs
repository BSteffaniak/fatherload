//! The world clock (Part II F4): determinism, tick-rate safety, phase boundaries and day
//! events, beds and the sleep vote, light levels, saves and replication.

use serde::{Serialize, de::DeserializeOwned};

use super::{SectionKey, WorldImport};
use crate::{
    GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, SimulationTick, TickInputs,
    WorldState,
    base::{BuildCommand, BuildRefusal, Buildable, Facing, ModuleKind, ModuleService, StructureId},
    clock::{
        ClockState, DAWN_START_SLICE, DUSK_START_SLICE, DayLength, DayPhase, NIGHT_LIGHT,
        SLEEP_FADE_MILLIS, SleepCommand, SleepRule, WorldClock,
    },
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);
const FLOOR: i32 = 70;
const LEFT: i32 = 10;
const RIGHT: i32 = 44;
const BEACON: TilePosition = TilePosition {
    x: 26,
    y: FLOOR - 1,
};

fn wire<T: Serialize + DeserializeOwned>(value: &T) -> T {
    let bytes = postcard::to_stdvec(value).expect("encode");
    postcard::from_bytes(&bytes).expect("decode")
}

/// World time of slice `slice` of day `day` (from 1) at `length`.
fn slice_millis(length: DayLength, day: u64, slice: u32) -> u64 {
    (day - 1) * length.millis() + u64::from(slice) * length.slice_millis()
}

fn park(world: &mut WorldState, player_id: PlayerId, tile_x: i32, tile_y: i32) {
    let player = world.player_mut(player_id).expect("player");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        player.x = (tile_x as f32 + 0.5) * TILE_SIZE;
        player.y = (tile_y as f32 + 0.5) * TILE_SIZE;
    }
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

/// A deep cavern with a claim and a bed in it, the rig parked at the bed.
fn bedroom() -> (WorldState, StructureId) {
    let mut world = WorldState::from_import(&WorldImport::new_game(PLAYER));
    for y in 52..=FLOOR + 1 {
        for x in LEFT - 1..=RIGHT + 1 {
            let kind = if y >= FLOOR || !(LEFT..=RIGHT).contains(&x) {
                TileKind::Stone
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    park(&mut world, PLAYER, BEACON.x, FLOOR - 1);
    world.player_mut(PLAYER).expect("player").credits = 50_000;
    for (item, at) in [
        (ModuleKind::Beacon, BEACON),
        (
            ModuleKind::Bed,
            TilePosition {
                x: BEACON.x + 2,
                y: FLOOR - 1,
            },
        ),
    ] {
        let (outcome, _) = act(
            &mut world,
            PLAYER,
            PlayerCommand::Build(BuildCommand::Place {
                item: Buildable::Module(item),
                at,
                facing: Facing::Right,
            }),
        );
        assert_eq!(outcome, PlayerScopedCommandOutcome::Applied, "{item:?}");
    }
    let bed = world
        .bases()
        .modules()
        .find(|module| module.kind == ModuleKind::Bed)
        .map(|module| module.id)
        .expect("bed");
    park(&mut world, PLAYER, BEACON.x + 2, FLOOR - 1);
    (world, bed)
}

fn sleep(bed: StructureId) -> PlayerCommand {
    PlayerCommand::Build(BuildCommand::Use {
        structure: bed,
        service: ModuleService::Sleep,
    })
}

fn act(
    world: &mut WorldState,
    player_id: PlayerId,
    command: PlayerCommand,
) -> (PlayerScopedCommandOutcome, Vec<GameplayEventKind>) {
    let inputs = TickInputs::default().act(player_id, command);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    let outcome = output
        .action_outcomes
        .first()
        .map(|outcome| outcome.outcome)
        .expect("one outcome");
    (
        outcome,
        output.events.into_iter().map(|event| event.kind).collect(),
    )
}

/// Runs `ticks` idle ticks and returns every event.
fn run(world: &mut WorldState, ticks: u64) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        events.extend(output.events.into_iter().map(|event| event.kind));
    }
    events
}

fn is_clock_event(event: &GameplayEventKind) -> bool {
    matches!(
        event,
        GameplayEventKind::Dawn { .. }
            | GameplayEventKind::Dusk { .. }
            | GameplayEventKind::NewDay { .. }
            | GameplayEventKind::NightSkipped { .. }
    )
}

#[test]
fn a_new_world_starts_on_a_morning_of_day_one() {
    let world = WorldState::new_game(PLAYER);
    let clock = world.clock();
    assert_eq!(clock.day(), 1);
    assert_eq!(clock.phase(), DayPhase::Day);
    assert!(!world.is_night());
    assert_eq!(world.day_length(), DayLength::Standard);
    assert!(world.clock_state().is_default());
}

#[test]
fn day_events_fire_once_at_each_boundary() {
    let mut world = WorldState::new_game(PLAYER);
    let length = DayLength::Short;
    world.set_day_length(length);
    // One second before dusk of day 1, run two seconds.
    world.set_world_time_millis(slice_millis(length, 1, DUSK_START_SLICE) - 1_000);
    let events: Vec<_> = run(&mut world, 120)
        .into_iter()
        .filter(is_clock_event)
        .collect();
    assert_eq!(events, vec![GameplayEventKind::Dusk { day: 1 }]);
    assert_eq!(world.clock().phase(), DayPhase::Dusk);
    // Midnight.
    world.set_world_time_millis(slice_millis(length, 2, 0) - 500);
    let events: Vec<_> = run(&mut world, 60)
        .into_iter()
        .filter(is_clock_event)
        .collect();
    assert_eq!(events, vec![GameplayEventKind::NewDay { day: 2 }]);
    assert!(world.is_night());
    // Dawn.
    world.set_world_time_millis(slice_millis(length, 2, DAWN_START_SLICE) - 500);
    let events: Vec<_> = run(&mut world, 60)
        .into_iter()
        .filter(is_clock_event)
        .collect();
    assert_eq!(events, vec![GameplayEventKind::Dawn { day: 2 }]);
    assert_eq!(world.clock().phase(), DayPhase::Dawn);
}

#[test]
fn the_clock_is_tick_rate_safe() {
    // The same real seconds read the same time of day, and dusk fires in the same second, at
    // every tick rate.
    let mut dusk_seconds = Vec::new();
    for hz in [30, 60, 120] {
        let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(hz));
        let length = DayLength::Short;
        world.set_day_length(length);
        world.set_world_time_millis(slice_millis(length, 1, DUSK_START_SLICE) - 2_000);
        let mut fired = None;
        for tick in 0..u64::from(hz) * 4 {
            let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
            if output
                .events
                .iter()
                .any(|event| matches!(event.kind, GameplayEventKind::Dusk { .. }))
            {
                fired = Some((tick + 1) * 1_000 / u64::from(hz));
            }
        }
        let clock = world.clock();
        assert_eq!(clock.phase(), DayPhase::Dusk, "{hz} Hz");
        dusk_seconds.push(fired.expect("dusk fired"));
        assert_eq!(
            clock.world_millis(),
            slice_millis(length, 1, DUSK_START_SLICE) + 2_000,
            "{hz} Hz"
        );
    }
    for millis in &dusk_seconds {
        assert!(
            (millis.abs_diff(dusk_seconds[0])) <= 34,
            "dusk at {dusk_seconds:?} ms"
        );
    }
}

#[test]
fn the_clock_is_deterministic() {
    let run_once = || {
        let (mut world, bed) = bedroom();
        world.set_world_time_millis(slice_millis(DayLength::Standard, 1, 80));
        let _ = act(&mut world, PLAYER, sleep(bed));
        let events = run(&mut world, 200);
        (world.simulation_state_hash(), events, world.clock())
    };
    let (hash_a, events_a, clock_a) = run_once();
    let (hash_b, events_b, clock_b) = run_once();
    assert_eq!(hash_a, hash_b);
    assert_eq!(events_a, events_b);
    assert_eq!(clock_a, clock_b);
}

#[test]
fn beds_are_for_the_night() {
    let (mut world, bed) = bedroom();
    // Morning: too early.
    let refusal = world.check_build(
        PLAYER,
        &BuildCommand::Use {
            structure: bed,
            service: ModuleService::Sleep,
        },
    );
    assert_eq!(refusal.err(), Some(BuildRefusal::NotTired));
    let (outcome, _) = act(&mut world, PLAYER, sleep(bed));
    assert_eq!(outcome, PlayerScopedCommandOutcome::IgnoredUnavailable);
    assert!(!world.is_in_bed(PLAYER));
}

#[test]
fn sleeping_alone_skips_the_night_to_dawn() {
    let (mut world, bed) = bedroom();
    let length = world.day_length();
    world.set_world_time_millis(slice_millis(length, 1, 85));
    let (outcome, events) = act(&mut world, PLAYER, sleep(bed));
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(events.contains(&GameplayEventKind::BaseServiceUsed {
        player_id: PLAYER,
        structure: bed,
        service: ModuleService::Sleep,
    }));
    assert!(world.is_in_bed(PLAYER));
    let fade = world.timing().rate.ticks_for_millis(SLEEP_FADE_MILLIS);
    let events = run(&mut world, fade + 2);
    assert!(
        events.contains(&GameplayEventKind::NightSkipped { day: 2 }),
        "{events:?}"
    );
    assert!(events.contains(&GameplayEventKind::NewDay { day: 2 }));
    assert!(events.contains(&GameplayEventKind::Dawn { day: 2 }));
    let clock = world.clock();
    assert_eq!(clock.day(), 2);
    assert_eq!(clock.phase(), DayPhase::Dawn);
    assert!(!world.is_in_bed(PLAYER));
    assert!(world.clock_state().sleep_deadline.is_none());
}

#[test]
fn the_crew_votes_by_going_to_bed() {
    let (mut world, bed) = bedroom();
    world.add_player(OTHER);
    park(&mut world, OTHER, BEACON.x + 3, FLOOR - 1);
    let length = world.day_length();
    world.set_world_time_millis(slice_millis(length, 1, 80));
    let fade = world.timing().rate.ticks_for_millis(SLEEP_FADE_MILLIS);
    let (outcome, _) = act(&mut world, PLAYER, sleep(bed));
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    let events = run(&mut world, fade * 2);
    assert!(
        !events.iter().any(is_clock_event),
        "one of two is not enough"
    );
    assert_eq!(world.awake_players(), vec![OTHER]);
    assert!(world.sleep_countdown_seconds().is_none());
    // The second player lies down: the countdown runs and the night ends.
    let (outcome, _) = act(&mut world, OTHER, sleep(bed));
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(world.sleep_countdown_seconds().is_some());
    let events = run(&mut world, fade + 1);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::NightSkipped { .. }))
    );
    assert_eq!(world.clock().phase(), DayPhase::Dawn);

    // Next night the host allows anyone to sleep: one player is enough.
    world.set_world_time_millis(slice_millis(length, 2, 82));
    let (outcome, events) = act(
        &mut world,
        PLAYER,
        PlayerCommand::Sleep(SleepCommand::SetRule(SleepRule::Anyone)),
    );
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(events.contains(&GameplayEventKind::SleepRuleChanged {
        player_id: PLAYER,
        rule: SleepRule::Anyone,
    }));
    let _ = act(&mut world, OTHER, sleep(bed));
    let events = run(&mut world, fade + 1);
    assert!(events.contains(&GameplayEventKind::NightSkipped { day: 3 }));
}

#[test]
fn getting_up_or_driving_off_breaks_the_vote() {
    let (mut world, bed) = bedroom();
    let length = world.day_length();
    world.set_world_time_millis(slice_millis(length, 1, 80));
    world.add_player(OTHER);
    park(&mut world, OTHER, BEACON.x + 3, FLOOR - 1);
    let _ = act(&mut world, PLAYER, sleep(bed));
    let _ = act(&mut world, OTHER, sleep(bed));
    assert!(world.sleep_countdown_seconds().is_some());
    let (outcome, events) = act(&mut world, OTHER, PlayerCommand::Sleep(SleepCommand::GetUp));
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(events.contains(&GameplayEventKind::GotUp { player_id: OTHER }));
    assert!(world.sleep_countdown_seconds().is_none());
    // Driving away from the bed gets you up too.
    park(&mut world, PLAYER, LEFT + 2, FLOOR - 1);
    let events = run(&mut world, 1);
    assert!(events.contains(&GameplayEventKind::GotUp { player_id: PLAYER }));
    assert!(!world.is_in_bed(PLAYER));
}

#[test]
fn a_departing_player_leaves_their_bed() {
    let (mut world, bed) = bedroom();
    world.set_world_time_millis(slice_millis(world.day_length(), 1, 80));
    world.add_player(OTHER);
    park(&mut world, OTHER, BEACON.x + 3, FLOOR - 1);
    let _ = act(&mut world, OTHER, sleep(bed));
    assert!(world.is_in_bed(OTHER));
    world.remove_player(OTHER);
    assert!(!world.is_in_bed(OTHER));
}

#[test]
fn light_follows_the_sun_and_stays_out_of_the_ground() {
    let mut world = WorldState::new_game(PLAYER);
    // Away from the town's lit streets.
    let x = 12;
    let surface = world.terrain().surface_row(x);
    // Keep the rig's lamps out of it.
    park(&mut world, PLAYER, 180, surface - 40);
    let length = world.day_length();
    world.set_world_time_millis(slice_millis(length, 1, 48));
    assert_eq!(world.light_at(x, surface - 2), 1_000);
    assert_eq!(world.light_at(x, surface + 30), 0, "deep rock is dark");
    world.set_world_time_millis(slice_millis(length, 1, 88));
    let night = world.light_at(x, surface - 2);
    assert!(night < 250, "{night}");
    assert!(night >= NIGHT_LIGHT);
    assert_eq!(world.light_at(x, surface + 30), 0);
    // A rig's lamps light the night around it.
    park(&mut world, PLAYER, x, surface - 2);
    assert!(world.light_at(x + 1, surface - 2) > 700);
}

#[test]
fn the_clock_saves_and_replicates() {
    let (mut world, bed) = bedroom();
    world.set_day_length(DayLength::Long);
    world.set_world_time_millis(slice_millis(DayLength::Long, 3, 84));
    world.add_player(OTHER);
    park(&mut world, OTHER, BEACON.x + 3, FLOOR - 1);
    let _ = act(&mut world, PLAYER, sleep(bed));
    let _ = act(
        &mut world,
        PLAYER,
        PlayerCommand::Sleep(SleepCommand::SetRule(SleepRule::Everyone)),
    );
    assert!(world.is_in_bed(PLAYER));

    let json = serde_json::to_string(&world).expect("json");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.clock_state(), world.clock_state());
    assert_eq!(loaded.clock(), world.clock());
    let wired: WorldState = wire(&world);
    assert_eq!(wired.clock(), world.clock());

    // A replica built from an old keyframe catches up through the Clock section.
    let mut replica = WorldState::from_import(&WorldImport::new_game(PLAYER));
    replica.set_simulation_tick(world.simulation_tick());
    let section = world.section(SectionKey::Clock).expect("clock section");
    replica.apply_section(SectionKey::Clock, wire(&section));
    assert_eq!(replica.clock_state(), world.clock_state());
    assert_eq!(replica.clock(), world.clock());
    assert!(SectionKey::SHARED.contains(&SectionKey::Clock));
    assert!(SectionKey::Clock.visible_to(None));

    // A save from before the clock loads with the standard day.
    let mut value = serde_json::to_value(&world).expect("value");
    value.as_object_mut().expect("object").remove("clock");
    let old: WorldState = serde_json::from_value(value).expect("old save");
    assert_eq!(old.clock_state(), &ClockState::default());
    assert_eq!(old.day_length(), DayLength::Standard);
}

#[test]
fn untouched_clocks_do_not_change_world_hashes() {
    let world = WorldState::new_game(PLAYER);
    let mut other = world.clone();
    other.set_day_length(DayLength::Standard);
    assert_eq!(world.simulation_state_hash(), other.simulation_state_hash());
    other.set_day_length(DayLength::Short);
    assert_ne!(world.simulation_state_hash(), other.simulation_state_hash());
    let _ = WorldClock::at(
        SimulationTick::new(0),
        world.timing().rate,
        world.clock_state(),
        0,
    );
}

#[test]
fn the_moon_hangs_above_the_sky_generators_moon() {
    use crate::worldgen::{GeneratorVersion, WorldSettings, sky};
    let seed = 0x0E00_0042;
    let sky_world = WorldSettings {
        generator: GeneratorVersion::V8,
        ..WorldSettings::new(seed)
    };
    let world = WorldState::new_game_with_settings(PLAYER, &sky_world, SimulationParams::new(60));
    let anchor = world.moon_anchor();
    assert_eq!(Some(anchor.column), sky::moon_column(&sky_world));
    assert_eq!(anchor.row, sky::row_of(sky::MOON_BOTTOM_ROWS));

    let old_world = WorldSettings {
        generator: GeneratorVersion::V7,
        ..WorldSettings::new(seed)
    };
    let world = WorldState::new_game_with_settings(PLAYER, &old_world, SimulationParams::new(60));
    assert_eq!(
        world.moon_anchor(),
        crate::clock::default_moon_anchor(seed),
        "worlds without a sky keep the seed's backdrop Moon"
    );
}
