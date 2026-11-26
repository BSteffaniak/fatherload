//! The galaxy story end to end (Wave 13 story): a bot with the player's controls walks the
//! builders' road from the Archive's first chart to the Harbour, every chapter in turn
//! (test-mode warps between places, the plan allows them); the Warden is beaten fairly by a
//! bot; co-op partners share one road; the story survives a save and replicates; procedural
//! leads pick up, advance and pay.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "test bookkeeping"
)]

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TestModeAction,
    TickInputs, WorldState,
    alien::{
        AlienCommand, AlienEvent, AlienSpecies,
        quest::{ChapterGoal, chapters},
        town::{self, towns_of},
    },
    attitude::RigAttitude,
    enemy::EnemyDifficulty,
    entity::{EntityId, EntityKind, EntityState},
    field::trig::wrap_angle,
    flight::{FlightPlace, RigFlight},
    galaxy_story::{
        CHAPTER_COUNT, FinaleChoice, GalaxyCommand, GalaxyEvent, GalaxyStory, Goal, LeadStep,
        discovery::leads_of,
        plan::{self},
    },
    hostiles::{
        CombatCommand, ShipClass,
        combat::{bearing, length, nose},
    },
    input::WeaponControl,
    place::{BodyId, Place, SpaceBodyKey, StoreKey},
    space::{SpaceTile, SystemRef},
    world::{SectionKey, strip_point, strip_position},
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);
const PARTNER: PlayerId = PlayerId::new(1);
const SEED: u64 = 0x00C0_FFEE;
const HZ: u32 = 30;

/// A Deep Descent world in test mode whose crew woke the Heart: space open, the Star Drive, a
/// well-kept rig with the station's ship gear and the Descents' hazard gear.
pub(super) fn world_with(players: &[PlayerId]) -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        players[0],
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(SEED)
        },
        SimulationParams::new(HZ),
    );
    for player in &players[1..] {
        world.add_player(*player);
    }
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    world.enable_test_mode();
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    for player in players {
        heart.star_drive.insert(*player);
    }
    world.set_heart(heart);
    for player in players {
        let rig = world.player_mut(*player).expect("rig");
        crate::economy::max_out_upgrades(rig);
        rig.hazard_gear = crate::protection::HazardGear::MAXED;
        rig.hull = rig.max_hull();
        rig.fuel = rig.fuel_capacity;
        rig.credits = 20_000;
    }
    world
}

fn tick_for(
    world: &mut WorldState,
    player: PlayerId,
    controls: ControlState,
    command: Option<PlayerCommand>,
) -> Vec<GameplayEventKind> {
    let mut inputs = TickInputs::held(player, controls);
    if let Some(command) = command {
        inputs = inputs.act(player, command);
    }
    let tick = world.simulation_tick();
    world
        .simulate_tick(tick, &inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn tick(
    world: &mut WorldState,
    controls: ControlState,
    command: Option<PlayerCommand>,
) -> Vec<GameplayEventKind> {
    tick_for(world, PLAYER, controls, command)
}

pub(super) fn run(world: &mut WorldState, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(tick(world, ControlState::default(), None));
    }
    events
}

fn act_as(
    world: &mut WorldState,
    player: PlayerId,
    command: PlayerCommand,
) -> Vec<GameplayEventKind> {
    let mut events = tick_for(world, player, ControlState::default(), Some(command));
    events.extend(run(world, 1));
    events
}

pub(super) fn act(world: &mut WorldState, command: PlayerCommand) -> Vec<GameplayEventKind> {
    act_as(world, PLAYER, command)
}

pub(super) fn seconds(seconds: f32) -> u32 {
    (seconds * HZ as f32).round() as u32
}

pub(super) fn galaxy(events: &[GameplayEventKind]) -> Vec<(GalaxyEvent, u32)> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Galaxy { what, detail, .. } => Some((*what, *detail)),
            _ => None,
        })
        .collect()
}

fn alien_saw(events: &[GameplayEventKind], what: AlienEvent) -> bool {
    events
        .iter()
        .any(|event| matches!(event, GameplayEventKind::Alien { what: seen, .. } if *seen == what))
}

fn warp_to_story(world: &mut WorldState, player: PlayerId) -> Vec<GameplayEventKind> {
    let mut events = act_as(
        world,
        player,
        PlayerCommand::TestMode(TestModeAction::WarpToStory),
    );
    events.extend(run(world, 10));
    events
}

pub(super) fn goal(world: &WorldState) -> Option<Goal> {
    world.galaxy_story().current_goal().map(|goal| goal.goal)
}

/// Puts the player's rig on body `id` at strip `column`, `rows_up` rows over row `row`.
fn put_rig(world: &mut WorldState, id: SpaceBodyKey, column: f64, row: i32, rows_up: f64) {
    world
        .move_to_place(
            PLAYER,
            Place::OnBody(BodyId::Space(id)),
            strip_position(column + 0.5, f64::from(row) - rows_up),
        )
        .expect("on the body");
    let landed = world.space().body_ref(id).map(FlightPlace::Landed);
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.flight.air = 1.0e7;
        if let Some(landed) = landed {
            rig.flight.place = landed;
        }
    }
}

/// The NPC entity of resident `slot` of the town, once the rig stands beside them.
fn beside(
    world: &mut WorldState,
    id: SpaceBodyKey,
    found: &town::AlienTown,
    slot: usize,
) -> EntityId {
    let character = found.population.characters[slot].id;
    for _ in 0..4_000 {
        let npc = world
            .store_entities(StoreKey::Body(id))
            .and_then(|entities| {
                entities
                    .of_kind(EntityKind::Npc)
                    .find(|entity| {
                        matches!(entity.state, EntityState::Npc(state) if state.character == character)
                    })
                    .map(|entity| entity.id)
            });
        if let Some(npc) = npc
            && let Some((entity, state)) = world.npc(npc)
            && state.inside.is_none()
        {
            let frame = world.store_frame(StoreKey::Body(id)).expect("frame");
            let x = state.x_at(world.simulation_tick(), 0.0);
            let (u, _) = strip_point(frame.world(x, entity.body.y));
            put_rig(world, id, u.floor(), found.ground_row(), 1.0);
            let _ = run(world, 3);
            if world.npcs_in_reach(PLAYER).contains(&npc) {
                return npc;
            }
        }
        let _ = run(world, 30);
    }
    panic!(
        "{} never came out",
        found.population.characters[slot].name.full()
    );
}

/// The rig's strip row on the body it stands on.
fn rig_row(world: &WorldState, id: SpaceBodyKey) -> f64 {
    let frame = world.store_frame(StoreKey::Body(id)).expect("frame");
    let (x, y) = world.actor_position(PLAYER).expect("actor");
    strip_point(frame.world(x, y)).1
}

/// The boss bot: the player's controls only (rotation thrusters to face the boss, main and
/// retro thrust to hold a firing range, a strafe off any telegraphed line, the trigger when
/// lined up, missiles now and then), then on to the wreck.
fn boss_bot(world: &WorldState, tick: u32) -> (ControlState, Option<PlayerCommand>) {
    let rig = world.player(PLAYER).expect("rig");
    let pos = rig.flight.pos;
    let hostiles = world.hostiles();
    let target = hostiles
        .boss()
        .map(|boss| (boss.pos, (boss.vx, boss.vy), Some(*boss)))
        .or_else(|| {
            hostiles
                .ships
                .iter()
                .min_by(|a, b| pos.distance(a.pos).total_cmp(&pos.distance(b.pos)))
                .map(|ship| (ship.pos, (ship.vx, ship.vy), None))
        });
    let Some((at, (tvx, tvy), boss)) = target else {
        return (ControlState::default(), None);
    };
    let (dx, dy) = pos.delta(at);
    let distance = length(dx, dy).max(1.0);
    let (ux, uy) = (dx / distance, dy / distance);
    let range = if boss.is_some_and(|boss| boss.class == ShipClass::Warden) {
        620.0
    } else {
        520.0
    };
    let radial = ((distance - range) * 1.5).clamp(-450.0, 450.0);
    let (mut wx, mut wy) = (tvx + ux * radial, tvy + uy * radial);
    if boss.and_then(|boss| boss.telegraph).is_some() {
        wx += -uy * 420.0;
        wy += ux * 420.0;
    }
    let (ax, ay) = (wx - rig.flight.velocity_x, wy - rig.flight.velocity_y);
    let angle = rig.attitude.orientation;
    let (nx, ny) = nose(angle);
    let (rx, ry) = (-ny, nx);
    let forward = ax * nx + ay * ny;
    let side = ax * rx + ay * ry;
    let error = wrap_angle(bearing(dx, dy) - angle);
    let rotate = (error * 4.0 - rig.attitude.angular_velocity * 1.2).clamp(-1.0, 1.0);
    let fire = error.abs() < 0.1 && distance < 1_000.0;
    let controls = ControlState::new(
        (side / 250.0).clamp(-1.0, 1.0),
        forward > 60.0,
        forward < -60.0,
    )
    .with_rotation(rotate)
    .with_weapon(WeaponControl {
        fire,
        ..WeaponControl::default()
    });
    let missile = (fire && tick.is_multiple_of(60) && world.rig_combat(PLAYER).missiles > 0)
        .then_some(PlayerCommand::Combat(CombatCommand::FireMissile));
    (controls, missile)
}

/// Fights whatever boss is out until `done` says the fight is won (at most `limit` s).
fn fight(
    world: &mut WorldState,
    limit: f32,
    done: impl Fn(&WorldState) -> bool,
) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for tick_index in 0..seconds(limit) {
        let (controls, command) = boss_bot(world, tick_index);
        events.extend(tick(world, controls, command));
        if done(world) {
            break;
        }
    }
    events
}

/// The species chapter goals: land at the nearest species' first home, learn, trade, and
/// finish the elder's first chapter.
fn earn_a_species_chapter(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let plan = world.galaxy_plan().expect("plan");
    let id = plan
        .stop(world.galaxy_story().chapter)
        .and_then(|stop| stop.body)
        .expect("home");
    let params = crate::place::planet_params_for(SEED, id).expect("landable");
    let species = AlienSpecies::from_folk(params.folk).expect("settled");
    let found = std::sync::Arc::clone(&towns_of(&params)[0]);
    let mut events = act(
        world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::OnBody(BodyId::Space(
            id,
        )))),
    );
    events.extend(run(world, 40));
    assert!(
        world.aliens().log(PLAYER, species).met(),
        "first contact with the {species:?}"
    );
    let chapter = chapters(species)[0];
    let elder = beside(world, id, &found, town::ELDER);
    events.extend(act(
        world,
        PlayerCommand::Alien(AlienCommand::AskChapter { npc: elder }),
    ));
    events.extend(act(
        world,
        PlayerCommand::Alien(AlienCommand::AcceptChapter { npc: elder }),
    ));
    let ChapterGoal::Deliver { mineral, count } = chapter.goal else {
        panic!("a first chapter fetches: {:?}", chapter.goal);
    };
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(mineral, count);
    let elder = beside(world, id, &found, town::ELDER);
    let done = act(
        world,
        PlayerCommand::Alien(AlienCommand::TurnInChapter { npc: elder }),
    );
    assert!(
        alien_saw(&done, AlienEvent::ChapterCompleted),
        "the elder's chapter"
    );
    events.extend(done);
    events.extend(run(world, 20));
    events
}

/// Digs straight down (the player's drill) until the rig is `rows` under where it started.
fn dig_down(world: &mut WorldState, id: SpaceBodyKey, rows: f64) -> Vec<GameplayEventKind> {
    let start = rig_row(world, id);
    let mut events = Vec::new();
    for _ in 0..seconds(120.0) {
        events.extend(tick(world, ControlState::new(0.0, false, true), None));
        if rig_row(world, id) >= start + rows {
            break;
        }
    }
    events
}

/// One goal of the story done the way a player does it (warps between places).
#[allow(clippy::too_many_lines, reason = "one arm per kind of goal")]
pub(super) fn do_goal(world: &mut WorldState, goal: Goal) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    match goal {
        Goal::ReachSystem | Goal::ReachHarbour | Goal::Land => {
            events.extend(warp_to_story(world, PLAYER));
            events.extend(run(world, 20));
        }
        Goal::Scan => {
            events.extend(warp_to_story(world, PLAYER));
            events.extend(act(world, PlayerCommand::UseScanner));
            events.extend(run(world, 20));
        }
        Goal::FindRelic => {
            events.extend(warp_to_story(world, PLAYER));
            events.extend(run(world, 30));
            if goal_is(world, Goal::FindRelic)
                && let Some((body, site, _)) = world.galaxy_relic()
                && site.rows > 0
            {
                events.extend(dig_down(world, body, f64::from(site.rows)));
                events.extend(run(world, 20));
            }
        }
        Goal::Hold { seconds: hold, .. } => {
            // Hold the site (test-mode armour: the raid is real, the bot only stands).
            events.extend(act(
                world,
                PlayerCommand::TestMode(TestModeAction::SetInvulnerable(true)),
            ));
            for _ in 0..seconds(f32::from(hold) + 8.0) {
                events.extend(tick(world, ControlState::default(), None));
                if !goal_is(world, goal) {
                    break;
                }
            }
            events.extend(act(
                world,
                PlayerCommand::TestMode(TestModeAction::SetInvulnerable(false)),
            ));
        }
        Goal::MeetSpecies => events.extend(earn_a_species_chapter(world)),
        Goal::SpeciesChapter => {}
        Goal::BeatBoss(faction) => {
            events.extend(warp_to_story(world, PLAYER));
            events.extend(act(
                world,
                PlayerCommand::TestMode(TestModeAction::SetInvulnerable(true)),
            ));
            // The lair meets the rig on its next check; then the fight.
            events.extend(run(world, seconds(2.0)));
            events.extend(fight(world, 300.0, |world| {
                world.hostiles().bosses_beaten.contains(&faction)
            }));
            events.extend(act(
                world,
                PlayerCommand::TestMode(TestModeAction::SetInvulnerable(false)),
            ));
            events.extend(run(world, 20));
        }
        Goal::CalmWarden => {
            events.extend(warp_to_story(world, PLAYER));
            events.extend(act(
                world,
                PlayerCommand::TestMode(TestModeAction::SetInvulnerable(true)),
            ));
            // It comes out once the rig is rested from its last fight.
            events.extend(wait_for_warden(world));
            events.extend(fight(world, 400.0, |world| {
                world.galaxy_story().warden_calmed
            }));
            events.extend(act(
                world,
                PlayerCommand::TestMode(TestModeAction::SetInvulnerable(false)),
            ));
            events.extend(run(world, 20));
        }
        Goal::Choose => {
            events.extend(warp_to_story(world, PLAYER));
            events.extend(act(
                world,
                PlayerCommand::Galaxy(GalaxyCommand::Choose(FinaleChoice::RingTheBell)),
            ));
            events.extend(run(world, 10));
        }
    }
    events
}

fn goal_is(world: &WorldState, goal: Goal) -> bool {
    self::goal(world) == Some(goal)
}

#[test]
#[allow(clippy::too_many_lines, reason = "the whole road in order")]
fn a_bot_walks_the_whole_road_to_the_harbour_and_the_game_goes_on() {
    let mut world = world_with(&[PLAYER]);
    world.player_mut(PLAYER).expect("rig").hazard_gear.arms = crate::hostiles::ShipGear::MAXED;
    // The story begins once space is open, from the Archive's first chart.
    let events = run(&mut world, 3);
    let seen = galaxy(&events);
    assert!(seen.contains(&(GalaxyEvent::Begun, 0)));
    assert!(seen.contains(&(GalaxyEvent::ChapterBegun, 0)));
    assert!(
        seen.iter()
            .any(|(what, _)| *what == GalaxyEvent::ArchivistRemark)
    );
    let story = world.galaxy_story().clone();
    assert_eq!(
        story.anchor,
        Some(plan::anchor_of(SEED, world.star_charts()))
    );
    assert!(
        world.galaxy_waypoint().is_some(),
        "a waypoint for the first goal"
    );
    assert_eq!(world.flight_shared().crew_waypoint, world.galaxy_waypoint());
    // Set course: the player's own waypoint (the warp's target) is the story's.
    let set = act(&mut world, PlayerCommand::Galaxy(GalaxyCommand::SetCourse));
    assert!(galaxy(&set).contains(&(GalaxyEvent::CourseSet, 0)));
    assert_eq!(
        world.flight_shared().waypoint_of(PLAYER),
        world.galaxy_waypoint()
    );
    // Every goal in turn.
    let credits_before = world.player(PLAYER).expect("rig").credits;
    let mut all = Vec::new();
    let mut guard = 0;
    while let Some(goal) = goal(&world) {
        let before = (world.galaxy_story().chapter, world.galaxy_story().step);
        eprintln!("chapter {} goal {} {goal:?}", before.0 + 1, before.1 + 1);
        let events = do_goal(&mut world, goal);
        all.extend(events);
        let after = (world.galaxy_story().chapter, world.galaxy_story().step);
        assert_ne!(
            before,
            after,
            "goal {goal:?} of chapter {} done",
            before.0 + 1
        );
        guard += 1;
        assert!(guard < 64, "the road ends");
    }
    let seen = galaxy(&all);
    let story = world.galaxy_story();
    assert!(story.finished());
    assert_eq!(story.chapter, CHAPTER_COUNT);
    assert_eq!(story.choice, Some(FinaleChoice::RingTheBell));
    assert!(story.finale_millis.is_some(), "the ending plays");
    assert!(story.warden_calmed);
    for chapter in 0..CHAPTER_COUNT {
        assert!(
            seen.contains(&(GalaxyEvent::ChapterDone, u32::from(chapter))),
            "chapter {chapter} closed"
        );
    }
    assert_eq!(
        story.logs.len(),
        crate::galaxy_story::journal::SPACE_LOGS.len()
    );
    assert!(
        seen.iter()
            .any(|(what, _)| *what == GalaxyEvent::RelicFound)
    );
    assert!(seen.iter().any(|(what, _)| *what == GalaxyEvent::HoldBegun));
    assert!(
        seen.iter()
            .any(|(what, _)| *what == GalaxyEvent::WardenAppeared)
    );
    assert!(
        seen.iter()
            .any(|(what, _)| *what == GalaxyEvent::WardenCalmed)
    );
    assert!(seen.contains(&(GalaxyEvent::Chosen, FinaleChoice::RingTheBell.code())));
    assert!(
        world.player(PLAYER).expect("rig").credits > credits_before,
        "the road paid"
    );
    // The crew's stats count the road.
    let stats = world.galaxy_story().stats_of_seed(SEED);
    assert_eq!(stats.chapters_done, CHAPTER_COUNT);
    assert!(
        stats.systems_visited >= 6 && stats.worlds_landed >= 5,
        "{stats:?}"
    );
    assert!(stats.leads_found > 0, "leads picked up on the way");
    // The game goes on: the story is done, the archivist talks of what comes next, and new
    // systems still hold leads.
    assert!(world.galaxy_waypoint().is_none());
    let asked = act(
        &mut world,
        PlayerCommand::Galaxy(GalaxyCommand::AskArchivist),
    );
    let remark = galaxy(&asked)
        .into_iter()
        .find(|(what, _)| *what == GalaxyEvent::ArchivistRemark)
        .expect("the archivist answers");
    let line = crate::galaxy_story::journal::REMARKS[remark.1 as usize];
    assert_eq!(
        line.trigger,
        crate::galaxy_story::journal::RemarkTrigger::AfterTheRoad
    );
    // A choice cannot be made twice.
    let again = act(
        &mut world,
        PlayerCommand::Galaxy(GalaxyCommand::Choose(FinaleChoice::KeepTheLamp)),
    );
    assert!(
        galaxy(&again)
            .iter()
            .any(|(what, _)| *what == GalaxyEvent::Refused)
    );
    assert_eq!(world.galaxy_story().choice, Some(FinaleChoice::RingTheBell));
}

#[test]
fn the_warden_is_beaten_fairly_by_a_bot() {
    // Middling ship gear, no test-mode armour: the finale is winnable on skill.
    let mut world = world_with(&[PLAYER]);
    world.player_mut(PLAYER).expect("rig").hazard_gear.arms = crate::hostiles::ShipGear {
        cannon: 2,
        turret: 1,
        missiles: 2,
        shield: 2,
    };
    let mut story = GalaxyStory {
        begun: true,
        anchor: Some(plan::anchor_of(SEED, &[])),
        chapter: CHAPTER_COUNT - 1,
        ..GalaxyStory::default()
    };
    story.next_check_millis = 0;
    world.set_galaxy_story(story);
    assert_eq!(goal(&world), Some(Goal::CalmWarden));
    let _ = warp_to_story(&mut world, PLAYER);
    let _ = run(&mut world, seconds(1.0));
    assert!(
        world
            .hostiles()
            .ships
            .iter()
            .any(|ship| ship.class == ShipClass::Warden),
        "the Warden meets the rig at the gate"
    );
    // Every attack it makes is telegraphed long enough to dodge.
    for phase in 0..2 {
        for attack in crate::hostiles::boss::cycle(ShipClass::Warden, phase) {
            assert!(
                crate::hostiles::boss::warning(*attack) >= crate::hostiles::MIN_TELEGRAPH_STEPS
            );
        }
    }
    let events = fight(&mut world, 400.0, |world| {
        world.galaxy_story().warden_calmed
    });
    let rig = world.player(PLAYER).expect("rig");
    eprintln!("hull {:.0}/{:.0}", rig.hull, rig.max_hull());
    assert!(world.galaxy_story().warden_calmed, "the Warden is calm");
    assert!(
        galaxy(&events)
            .iter()
            .any(|(what, _)| *what == GalaxyEvent::WardenCalmed)
    );
    assert!(rig.hull > 0.0, "and the rig lived");
    // The Warden beats no faction: the Concord's own boss still waits.
    assert!(
        !world
            .hostiles()
            .bosses_beaten
            .contains(&crate::hostiles::Faction::Machine)
    );
    let _ = run(&mut world, 10);
    assert_eq!(goal(&world), Some(Goal::Choose));
}

#[test]
fn an_escape_from_the_warden_brings_it_back_later() {
    let mut world = world_with(&[PLAYER]);
    world.player_mut(PLAYER).expect("rig").hazard_gear.arms = crate::hostiles::ShipGear::MAXED;
    world.set_galaxy_story(GalaxyStory {
        begun: true,
        anchor: Some(plan::anchor_of(SEED, &[])),
        chapter: CHAPTER_COUNT - 1,
        ..GalaxyStory::default()
    });
    let _ = warp_to_story(&mut world, PLAYER);
    let _ = run(&mut world, seconds(1.0));
    assert!(world.hostiles().boss().is_some());
    // The escape jump gets away (the warp button in a fight), and the Warden goes.
    let _ = act(&mut world, PlayerCommand::Combat(CombatCommand::EscapeJump));
    let _ = run(&mut world, seconds(4.0));
    assert!(world.hostiles().engaged(PLAYER).is_none(), "away");
    assert!(!world.galaxy_story().warden_calmed);
    // Back at the gate after the rest, it comes out again: no countdown, no loss.
    let _ = warp_to_story(&mut world, PLAYER);
    let _ = wait_for_warden(&mut world);
    assert_eq!(world.galaxy_story().warden_on, Some(PLAYER));
}

/// Waits (at most 90 s, the rest after a fight) for the Warden to come out at the gate.
fn wait_for_warden(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..seconds(90.0) {
        events.extend(run(world, 1));
        if world
            .hostiles()
            .ships
            .iter()
            .any(|ship| ship.class == ShipClass::Warden)
        {
            return events;
        }
    }
    panic!("the Warden never came out at its gate");
}

#[test]
fn co_op_partners_share_one_road_from_different_places() {
    let mut world = world_with(&[PLAYER, PARTNER]);
    let _ = run(&mut world, 3);
    assert_eq!(goal(&world), Some(Goal::ReachSystem));
    // The partner reaches the first system while the player stays home: it counts for both.
    let events = warp_to_story(&mut world, PARTNER);
    let mut events = events;
    events.extend(run(&mut world, 20));
    assert_eq!(goal(&world), Some(Goal::Scan));
    assert!(galaxy(&events).contains(&(GalaxyEvent::GoalDone, 0)));
    assert_eq!(world.place_of(PLAYER), Place::HOME);
    assert_eq!(
        world
            .galaxy_story()
            .crew
            .get(&PARTNER)
            .map(|stats| stats.goals),
        Some(1)
    );
    // The player warps out to the world while the partner scans from orbit: one journal.
    let _ = act_as(&mut world, PARTNER, PlayerCommand::UseScanner);
    let _ = run(&mut world, 20);
    assert_eq!(goal(&world), Some(Goal::FindRelic));
    let _ = warp_to_story(&mut world, PLAYER);
    let _ = run(&mut world, 30);
    assert!(
        world.galaxy_story().chapter >= 1,
        "the player found the waymark for the crew"
    );
    // Both see the same waypoint (the crew's) and the same log.
    assert_eq!(
        world.flight_shared().waypoint_of(PLAYER),
        world.flight_shared().waypoint_of(PARTNER)
    );
    assert!(world.galaxy_story().logs.contains(&0));
}

#[test]
fn the_story_survives_a_save_and_replicates_as_a_section() {
    let mut world = world_with(&[PLAYER]);
    let _ = run(&mut world, 3);
    let _ = warp_to_story(&mut world, PLAYER);
    let _ = run(&mut world, 20);
    assert!(world.galaxy_story().step >= 1);
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.galaxy_story(), world.galaxy_story());
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    let bytes = postcard::to_allocvec(&world).expect("postcard");
    let loaded: WorldState = postcard::from_bytes(&bytes).expect("postcard restores");
    assert_eq!(loaded.galaxy_story(), world.galaxy_story());
    // The shared section carries it to a replica.
    let mut replica = world_with(&[PLAYER]);
    let payload = world.section(SectionKey::GalaxyStory).expect("section");
    replica.apply_section(SectionKey::GalaxyStory, payload);
    assert_eq!(replica.galaxy_story(), world.galaxy_story());
    assert!(SectionKey::SHARED.contains(&SectionKey::GalaxyStory));
    // An older world never begun hashes as before (the empty story is left out).
    let quiet = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(SEED),
        SimulationParams::new(HZ),
    );
    assert!(quiet.galaxy_story().is_empty());
}

#[test]
fn the_story_is_the_same_at_every_tick_rate() {
    let mut runs = Vec::new();
    for hz in [30, 60] {
        let mut world = WorldState::new_game_with_settings(
            PLAYER,
            &WorldSettings {
                generator: GeneratorVersion::DEEP_DESCENT,
                ..WorldSettings::new(SEED)
            },
            SimulationParams::new(hz),
        );
        world.enable_test_mode();
        let mut heart = world.heart().clone();
        heart.space_unlocked = true;
        heart.star_drive.insert(PLAYER);
        world.set_heart(heart);
        for _ in 0..hz * 2 {
            let tick = world.simulation_tick();
            let _ = world.simulate_tick(tick, &TickInputs::held(PLAYER, ControlState::default()));
        }
        let tick = world.simulation_tick();
        let inputs = TickInputs::held(PLAYER, ControlState::default())
            .act(PLAYER, PlayerCommand::TestMode(TestModeAction::WarpToStory));
        let _ = world.simulate_tick(tick, &inputs);
        for _ in 0..hz * 2 {
            let tick = world.simulation_tick();
            let _ = world.simulate_tick(tick, &TickInputs::held(PLAYER, ControlState::default()));
        }
        let story = world.galaxy_story().clone();
        runs.push((
            story.chapter,
            story.step,
            story.visited.clone(),
            story.leads.len(),
        ));
    }
    assert_eq!(runs[0], runs[1]);
}

#[test]
fn a_lead_is_picked_up_and_finished_in_its_system() {
    let mut world = world_with(&[PLAYER]);
    let _ = run(&mut world, 3);
    // A system near home with a lost ship (one step: fly to its transponder).
    let space = crate::space::Space::shared(SEED);
    let (system, lead) = (1..4)
        .flat_map(|ring: i32| (-ring..=ring).flat_map(move |y| (-ring..=ring).map(move |x| (x, y))))
        .flat_map(|(x, y)| {
            let coords = crate::space::SpaceSector::new(x, y);
            space
                .sector(coords)
                .systems
                .iter()
                .map(move |system| SystemRef {
                    sector: coords,
                    system: system.index,
                })
                .collect::<Vec<_>>()
        })
        .find_map(|system| {
            leads_of(SEED, system)
                .into_iter()
                .find(|lead| matches!(lead.steps.as_slice(), [LeadStep::ReachPoint(_)]))
                .map(|lead| (system, lead))
        })
        .expect("a lost ship near home");
    let LeadStep::ReachPoint(point) = lead.steps[0] else {
        unreachable!()
    };
    // Into the system (its star's side): the leads are picked up.
    let star = space.system(system).expect("system").centre;
    let _ = act(
        &mut world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::Space)),
    );
    launch(&mut world, star.offset(0, -40_000));
    let events = run(&mut world, 20);
    assert!(world.galaxy_story().visited.contains(&system));
    assert!(
        galaxy(&events)
            .iter()
            .any(|(what, _)| *what == GalaxyEvent::LeadFound)
    );
    let index = world
        .galaxy_story()
        .leads
        .iter()
        .position(|record| record.system == system && record.index == lead.index)
        .expect("in the discovery log");
    // Set course for it, then fly there.
    let set = act(
        &mut world,
        PlayerCommand::Galaxy(GalaxyCommand::SetLeadCourse {
            lead: u16::try_from(index).unwrap_or(0),
        }),
    );
    assert!(galaxy(&set).contains(&(GalaxyEvent::CourseSet, 1)));
    let credits = world.player(PLAYER).expect("rig").credits;
    launch(&mut world, point.offset(1_000, 0));
    let events = run(&mut world, 20);
    let seen = galaxy(&events);
    assert!(
        seen.contains(&(GalaxyEvent::LeadDone, index as u32)),
        "{seen:?}"
    );
    assert!(world.galaxy_story().leads[index].done);
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        credits + lead.credits,
        "paid"
    );
    assert!(world.galaxy_story().fragments.contains(&lead.fragment));
}

/// The rig into free flight at `at`, still.
pub(super) fn launch(world: &mut WorldState, at: SpaceTile) {
    let capacity = world.air_capacity_of(PLAYER);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.flight = RigFlight {
        place: FlightPlace::Space,
        pos: at.pos(),
        air: capacity,
        exit_column: 12,
        ..RigFlight::default()
    };
    rig.attitude = RigAttitude {
        orientation: 0.0,
        angular_velocity: 0.0,
    };
}

/// The first system near home (rings 1-3) with a lead whose steps match `shape`.
fn lead_near_home(shape: impl Fn(&[LeadStep]) -> bool) -> (SystemRef, crate::galaxy_story::Lead) {
    let space = crate::space::Space::shared(SEED);
    (1..4)
        .flat_map(|ring: i32| (-ring..=ring).flat_map(move |y| (-ring..=ring).map(move |x| (x, y))))
        .flat_map(|(x, y)| {
            let coords = crate::space::SpaceSector::new(x, y);
            space
                .sector(coords)
                .systems
                .iter()
                .map(move |system| SystemRef {
                    sector: coords,
                    system: system.index,
                })
                .collect::<Vec<_>>()
        })
        .find_map(|system| {
            leads_of(SEED, system)
                .into_iter()
                .find(|lead| shape(&lead.steps))
                .map(|lead| (system, lead))
        })
        .expect("a lead of that shape near home")
}

/// Into `system`'s reach in free flight (its leads picked up); the lead's discovery-log index.
fn enter_for_lead(world: &mut WorldState, system: SystemRef, lead: u8) -> usize {
    let star = crate::space::Space::shared(SEED)
        .system(system)
        .expect("system")
        .centre;
    let _ = act(
        world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::Space)),
    );
    launch(world, star.offset(0, -40_000));
    let _ = run(world, 20);
    world
        .galaxy_story()
        .leads
        .iter()
        .position(|record| record.system == system && record.index == lead)
        .expect("in the discovery log")
}

#[test]
fn a_distress_call_springs_a_real_ambush_and_pays_once_survived_and_docked() {
    let mut world = world_with(&[PLAYER]);
    world.player_mut(PLAYER).expect("rig").hazard_gear.arms = crate::hostiles::ShipGear::MAXED;
    let _ = run(&mut world, 3);
    let (system, lead) = lead_near_home(|steps| {
        matches!(
            steps,
            [
                LeadStep::ReachPoint(_),
                LeadStep::Survive(_),
                LeadStep::Dock
            ]
        )
    });
    let LeadStep::ReachPoint(beacon) = lead.steps[0] else {
        unreachable!()
    };
    let index = enter_for_lead(&mut world, system, lead.index);
    // The beacon is reached and the ambush springs on this rig.
    launch(&mut world, beacon.offset(1_000, 0));
    let events = run(&mut world, 20);
    assert!(galaxy(&events).contains(&(GalaxyEvent::LeadStep, index as u32)));
    let record = world.galaxy_story().leads[index];
    assert_eq!(record.step, 1, "on the survive step");
    assert_eq!(record.ambushed, Some(PLAYER));
    assert!(
        world.hostiles().engaged(PLAYER).is_some(),
        "a real fight at the beacon"
    );
    // Fight it off (the bot's controls; armour so the test is about the lead, not the dice).
    let _ = act(
        &mut world,
        PlayerCommand::TestMode(TestModeAction::SetInvulnerable(true)),
    );
    let ships = world.hostiles().ships.len();
    let fought = fight(&mut world, 240.0, |world| {
        world.hostiles().engaged(PLAYER).is_none()
    });
    let downed = fought
        .iter()
        .filter(|event| {
            matches!(event, GameplayEventKind::Hostile { what, .. }
                if *what == crate::hostiles::HostileEvent::ShipDestroyed)
        })
        .count();
    assert!(
        downed >= 1,
        "the ambush was fought ({downed} of {ships} ships down)"
    );
    let _ = act(
        &mut world,
        PlayerCommand::TestMode(TestModeAction::SetInvulnerable(false)),
    );
    let _ = run(&mut world, 20);
    assert!(
        world.hostiles().engaged(PLAYER).is_none(),
        "the ambush is over"
    );
    assert_eq!(
        world.galaxy_story().leads[index].step,
        2,
        "survived: now bring them in"
    );
    // Dock at the home station the way a player does: the survivors are delivered.
    let credits = world.player(PLAYER).expect("rig").credits;
    let galaxy_view = world.space();
    let station = crate::station::StationId {
        sector: crate::space::HOME_SECTOR,
        system: 0,
        index: 0,
    };
    let body = crate::station::profile::station_ref(&galaxy_view, station).expect("station");
    let found = crate::flight::space::SpaceQuery::body(&galaxy_view, body).expect("station");
    let port = found.port().expect("port");
    let orientation = found.dock_orientation();
    drop(galaxy_view);
    launch(&mut world, SpaceTile::of_pos(port.offset(10.0, 0.0)));
    world.player_mut(PLAYER).expect("rig").attitude = RigAttitude {
        orientation,
        angular_velocity: 0.0,
    };
    let _ = act(
        &mut world,
        PlayerCommand::Flight(crate::flight::FlightCommand::Use),
    );
    let events = run(&mut world, 20);
    assert!(
        galaxy(&events).contains(&(GalaxyEvent::LeadDone, index as u32)),
        "{:?}",
        world.rig_flight(PLAYER).map(|flight| flight.place)
    );
    assert!(world.player(PLAYER).expect("rig").credits >= credits + lead.credits);
}

#[test]
fn a_ruin_lead_is_landed_on_and_dug_to() {
    let mut world = world_with(&[PLAYER]);
    let _ = run(&mut world, 3);
    let (system, lead) = lead_near_home(
        |steps| matches!(steps, [LeadStep::Land(_), LeadStep::FindSite(_, site)] if site.rows > 0),
    );
    let LeadStep::FindSite(body, site) = lead.steps[1] else {
        unreachable!()
    };
    let index = enter_for_lead(&mut world, system, lead.index);
    // Set down a way off the mark: landed, and the compass points at it.
    let params = crate::place::planet_params_for(SEED, body).expect("landable");
    let ground = crate::planet::PlanetGround::new(params);
    let column = i32::try_from(site.column).expect("column");
    let off = column + 30;
    put_rig(
        &mut world,
        body,
        f64::from(off),
        crate::planet::BodyGround::surface_row(&ground, off),
        2.0,
    );
    let events = run(&mut world, 20);
    assert!(galaxy(&events).contains(&(GalaxyEvent::LeadStep, index as u32)));
    let bearing = world.galaxy_site_bearing(PLAYER).expect("a bearing");
    assert!(!bearing.story && bearing.buried);
    assert!(
        (-31..=-29).contains(&bearing.columns),
        "the mark is 30 columns left: {bearing:?}"
    );
    // Over the mark, then dig down to it with the drill.
    let surface = crate::planet::BodyGround::surface_row(&ground, column);
    put_rig(&mut world, body, f64::from(column), surface, 2.0);
    let _ = run(&mut world, 10);
    let mut events = dig_down(&mut world, body, f64::from(site.rows));
    events.extend(run(&mut world, 20));
    assert!(
        galaxy(&events).contains(&(GalaxyEvent::LeadDone, index as u32)),
        "the ruin's mark reached (rig at row {:.0}, mark at {})",
        rig_row(&world, body),
        surface + i32::from(site.rows)
    );
    assert!(world.galaxy_story().fragments.contains(&lead.fragment));
}

#[test]
fn a_hold_lands_real_raiders_by_the_site_and_counts_only_while_someone_holds_it() {
    let mut world = world_with(&[PLAYER]);
    // The Gathering Light's hold: the lamp-key on its world's surface, the Bloom coming.
    let chapter = 7;
    let def = crate::galaxy_story::chapter(chapter).expect("chapter");
    let step = def
        .goals
        .iter()
        .position(|goal| matches!(goal.goal, Goal::Hold { .. }))
        .expect("a hold");
    world.set_galaxy_story(GalaxyStory {
        begun: true,
        anchor: Some(plan::anchor_of(SEED, &[])),
        chapter,
        step: u8::try_from(step).expect("step"),
        ..GalaxyStory::default()
    });
    let mut events = warp_to_story(&mut world, PLAYER);
    let (body, ..) = world.galaxy_relic().expect("the relic");
    events.extend(run(&mut world, seconds(1.0)));
    assert!(
        galaxy(&events)
            .iter()
            .any(|(what, _)| *what == GalaxyEvent::HoldBegun)
    );
    let raid = world
        .hostiles()
        .raids
        .iter()
        .find(|raid| raid.store == StoreKey::Body(body))
        .cloned()
        .expect("a raid landed on the hold's world");
    assert!(raid.troopers.len() >= 2, "{} troopers", raid.troopers.len());
    let entities = world
        .store_entities(StoreKey::Body(body))
        .expect("the world's entities");
    let alive = raid
        .troopers
        .iter()
        .filter(|id| entities.contains(EntityId(**id)))
        .count();
    assert!(alive >= 2, "the troopers stand on the ground ({alive})");
    // The hold counts world time while the rig is there, and stops when it leaves.
    let held = world.galaxy_story().held_millis;
    assert!(held >= 500, "held {held} ms");
    let _ = act(
        &mut world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::Space)),
    );
    let _ = run(&mut world, seconds(3.0));
    let after = world.galaxy_story().held_millis;
    assert!(after <= held + 600, "no hold from orbit: {held} -> {after}");
    assert!(matches!(goal(&world), Some(Goal::Hold { .. })));
}

#[test]
fn after_the_heart_wakes_the_road_opens_when_the_crew_first_lifts_off() {
    let mut world = world_with(&[PLAYER]);
    // The Heart woke (the cable lit): its own ending has the screen, the road waits.
    let mut heart = world.heart().clone();
    heart.cable_lit_millis = Some(world.world_millis());
    world.set_heart(heart);
    let events = run(&mut world, seconds(2.0));
    assert!(galaxy(&events).is_empty(), "no road card over the ending");
    assert!(!world.galaxy_story().begun);
    // Up the cable and away: the first chart unrolls.
    let mut events = act(
        &mut world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::Space)),
    );
    events.extend(run(&mut world, 3));
    assert!(galaxy(&events).contains(&(GalaxyEvent::Begun, 0)));
    assert!(world.galaxy_story().begun);
}
