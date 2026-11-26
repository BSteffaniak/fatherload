//! The co-op soak (Wave 13 QA): a host with its own seat and two clients roam the galaxy apart
//! (each on its own theme world, in free flight far from the others, at home), over lossy links,
//! for a long simulated time, with the host saving, quitting and reloading, and a client
//! dropping and reconnecting; nothing desyncs, nothing grows without bound and no tick runs
//! away.
//!
//! `world_soak_long` (ignored) is the long run for the QA report:
//! `FATHERLOAD_SOAK_MINUTES=60 cargo test --release -p fatherload_netcode world_soak_long -- --ignored --nocapture`.

use std::time::{Duration, Instant};

use fatherload_simulation::{
    ControlState, PlayerCommand, TestModeAction, WorldState,
    enemy::EnemyDifficulty,
    place::{BodyId, Place, SpaceBodyKey},
    space::{Space, SpaceSector, Theme},
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::support::{assert_bounded, assert_converged};
use crate::harness::{Harness, HarnessConfig};
use crate::world::to_sim_player;
use crate::{LinkConditions, PlayerId, ProfileId, SeatId, SessionParams};

type WorldHarness = Harness<WorldState>;

const SEED: u64 = 0x50A4_0013;
const HOST: SeatId = SeatId(0);
const HOST_PROFILE: ProfileId = ProfileId(0x0a57_0000_0000_0001);
const ALEX: ProfileId = ProfileId(0xa1e8_0000_0000_0013);
const SAM: ProfileId = ProfileId(0x5a3a_0000_0000_0013);

/// A Deep Descent world with the stars open, test mode on (the roamers warp) and creatures
/// about.
fn galaxy_world() -> WorldState {
    let first = fatherload_simulation::PlayerId::new(0);
    let mut world = WorldState::new_game_with_settings(
        first,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(SEED)
        },
        fatherload_simulation::SimulationParams::new(30),
    );
    world.remove_player(first);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    world.enable_test_mode();
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    world.set_heart(heart);
    let mut template = world.player_template().clone();
    fatherload_simulation::economy::max_out_upgrades(&mut template);
    template.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
    template.hull = template.max_hull();
    template.credits = 50_000;
    world.set_player_template(template);
    world
}

/// A landable body near home whose space theme is `theme`.
fn body_of_theme(theme: Theme) -> Place {
    let space = Space::shared(SEED);
    for ring in 0..=8_i32 {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let sector = SpaceSector::new(x, y);
                for system in &space.sector(sector).systems {
                    for body in &system.bodies {
                        let id = SpaceBodyKey {
                            sector,
                            system: system.index,
                            body: body.index,
                        };
                        if body.theme == theme
                            && fatherload_simulation::place::planet_params_for(SEED, id).is_some()
                        {
                            return Place::OnBody(BodyId::Space(id));
                        }
                    }
                }
            }
        }
    }
    panic!("no {theme:?} body near home");
}

/// Where each roamer goes, leg by leg (a leg a simulated minute): the host, then each client.
fn itineraries() -> [Vec<Place>; 3] {
    [
        vec![
            body_of_theme(Theme::DesertDunes),
            Place::Space,
            body_of_theme(Theme::Ice),
            Place::HOME,
        ],
        vec![
            body_of_theme(Theme::Jungle),
            Place::Space,
            body_of_theme(Theme::OceanArchipelago),
            body_of_theme(Theme::Volcanic),
        ],
        vec![
            Place::Space,
            body_of_theme(Theme::Volcanic),
            Place::HOME,
            body_of_theme(Theme::Crystal),
        ],
    ]
}

fn warp(place: Place) -> PlayerCommand {
    PlayerCommand::TestMode(TestModeAction::WarpToPlace(place))
}

fn harness() -> WorldHarness {
    let link = LinkConditions {
        latency: Duration::from_millis(50),
        jitter: Duration::from_millis(10),
        loss: 0.01,
        ..LinkConditions::default()
    };
    let mut config = HarnessConfig::new(SessionParams::new(30), 0, link);
    config.seed = 13;
    let mut harness = WorldHarness::new(config);
    harness.restart_host(galaxy_world());
    harness
}

fn wait_in_game(harness: &mut WorldHarness) {
    let mut guard = 0;
    while !harness.all_in_game() {
        harness.step();
        guard += 1;
        assert!(guard < 10_000, "clients never got in game");
    }
}

/// The host's seat (the saved rig of its profile after a reload, else a new one).
fn seat_host(harness: &mut WorldHarness, saved: Option<PlayerId>) -> PlayerId {
    let player = match saved {
        Some(player) => {
            assert!(
                harness.host.adopt_local_player(HOST, player),
                "host rig back"
            );
            player
        }
        None => harness.host.add_local_player(HOST),
    };
    harness
        .host
        .set_local_identity(HOST, Some(HOST_PROFILE), "Host");
    player
}

/// Sizes that must stay bounded over the run.
#[derive(Clone, Copy, Debug)]
struct Sizes {
    stores: usize,
    entities: usize,
    save_bytes: usize,
    keyframe_bytes: usize,
}

fn sizes(harness: &WorldHarness, viewer: PlayerId) -> Sizes {
    let world = harness.world();
    let entities = world.entities().len()
        + world
            .places()
            .stores()
            .map(|(_, body)| body.entities().len())
            .sum::<usize>();
    Sizes {
        stores: world.places().stores().count(),
        entities,
        save_bytes: serde_json::to_vec(world).map_or(0, |bytes| bytes.len()),
        keyframe_bytes: postcard::to_allocvec(&world.redacted_for(Some(to_sim_player(viewer))))
            .map_or(0, |bytes| bytes.len()),
    }
}

/// Runs the soak for `minutes` simulated minutes (a leg a minute, a reload every
/// `reload_every` legs, a reconnect in the second leg) and returns the host's tick costs.
#[allow(
    clippy::too_many_lines,
    reason = "one long session, read top to bottom"
)]
fn soak(minutes: u32, reload_every: u32) -> Vec<Duration> {
    let mut harness = harness();
    let mut host_player = seat_host(&mut harness, None);
    let mut alex = harness.add_client_as(None, Some(ALEX), "Alex");
    let mut sam = harness.add_client_as(None, Some(SAM), "Sam");
    wait_in_game(&mut harness);
    let alex_player = harness.clients[alex].session.player().expect("alex");
    let sam_player = harness.clients[sam].session.player().expect("sam");
    let plans = itineraries();
    let tick_rate = harness.config.params.rate();
    let mut costs: Vec<Duration> = Vec::new();
    let mut first_sizes: Option<Sizes> = None;
    let mut most = (0_usize, 0_usize);
    for minute in 0..minutes {
        let leg = minute as usize;
        // Each roamer goes to its next place.
        harness.host.submit_local_input(
            host_player,
            ControlState::default(),
            vec![warp(plans[0][leg % plans[0].len()])],
        );
        harness.clients[alex]
            .queued_actions
            .push(warp(plans[1][leg % plans[1].len()]));
        harness.clients[sam]
            .queued_actions
            .push(warp(plans[2][leg % plans[2].len()]));
        // A minute of play there (the clients' bots dig, bomb, fly and fire; the host's rig
        // drives and drills), timing the host's ticks.
        let end = harness.host_tick().0 + 60 * tick_rate;
        let mut frame = 0_u64;
        let mut reconnected = false;
        let mut arrived = [false; 3];
        while harness.host_tick().0 < end {
            frame += 1;
            let controls = match (frame / 90) % 4 {
                0 => ControlState::new(1.0, false, false),
                1 => ControlState::new(0.0, false, true),
                2 => ControlState::new(-1.0, false, false),
                _ => ControlState::default(),
            };
            harness
                .host
                .submit_local_input(host_player, controls, Vec::new());
            let before = harness.host_tick();
            let started = Instant::now();
            harness.step();
            if harness.host_tick() != before {
                costs.push(started.elapsed());
            }
            // Everyone gets where its itinerary says (a bot may fly off again later: it roams).
            let world = harness.world();
            for (index, (player, plan)) in [
                (host_player, &plans[0]),
                (alex_player, &plans[1]),
                (sam_player, &plans[2]),
            ]
            .into_iter()
            .enumerate()
            {
                let wanted = plan[leg % plan.len()];
                arrived[index] |= world.place_of(to_sim_player(player)).store() == wanted.store();
            }
            // The second leg: Sam drops off and reconnects with its token.
            if minute == 1 && !reconnected && harness.host_tick().0 + 30 * tick_rate >= end {
                reconnected = true;
                let token = harness.clients[sam]
                    .session
                    .reconnect_token()
                    .expect("a token");
                harness.disconnect(sam);
                harness.run_for(Duration::from_secs(3));
                sam = harness.add_client(Some(token));
                wait_in_game(&mut harness);
                assert_eq!(harness.clients[sam].session.player(), Some(sam_player));
            }
        }
        assert_eq!(
            arrived, [true; 3],
            "minute {minute}: everyone reached its place"
        );
        harness.settle(Duration::from_secs(2));
        assert_converged(&harness);
        assert_bounded(&harness);
        harness.reset_measurements();
        let now = sizes(&harness, alex_player);
        let first = *first_sizes.get_or_insert(now);
        most = (most.0.max(now.stores), most.1.max(now.entities));
        eprintln!(
            "minute {}: {now:?}, host tick mean {:?}",
            minute + 1,
            costs
                .iter()
                .rev()
                .take(usize::try_from(60 * tick_rate).unwrap_or(0))
                .sum::<Duration>()
                / u32::try_from(60 * tick_rate).unwrap_or(1)
        );
        // Bounded: every place visited holds one store, the creatures stay capped, and the
        // save and keyframes grow only with what was dug (linearly, never faster).
        let minutes_in = usize::try_from(minute + 1).unwrap_or(usize::MAX);
        assert!(now.stores <= 9, "{now:?}");
        assert!(now.entities < 2_000, "{now:?}");
        assert!(
            now.save_bytes <= first.save_bytes + minutes_in * 160_000,
            "the save grows without bound: {first:?} -> {now:?}"
        );
        assert!(
            now.keyframe_bytes <= first.keyframe_bytes + minutes_in * 40_000,
            "keyframes grow without bound: {first:?} -> {now:?}"
        );
        // Save, quit and reload every few legs: everyone comes back to the same rigs.
        if reload_every > 0 && (minute + 1) % reload_every == 0 && minute + 1 < minutes {
            let json = serde_json::to_string(harness.world()).expect("save");
            let saved: WorldState = serde_json::from_str(&json).expect("load");
            assert_eq!(
                saved.simulation_state_hash(),
                harness.world().simulation_state_hash(),
                "the save reloads to the same world"
            );
            harness.restart_host(saved);
            host_player = seat_host(&mut harness, Some(host_player));
            alex = harness.add_client_as(None, Some(ALEX), "Alex");
            sam = harness.add_client_as(None, Some(SAM), "Sam");
            wait_in_game(&mut harness);
            assert_eq!(harness.clients[alex].session.player(), Some(alex_player));
            assert_eq!(harness.clients[sam].session.player(), Some(sam_player));
            harness.reset_measurements();
        }
    }
    eprintln!("most stores {}, most entities {}", most.0, most.1);
    costs
}

/// Mean and worst of `costs`, and the 99th percentile.
fn spread(costs: &[Duration]) -> (Duration, Duration, Duration) {
    let mut sorted = costs.to_vec();
    sorted.sort_unstable();
    let mean = sorted.iter().sum::<Duration>() / u32::try_from(sorted.len().max(1)).unwrap_or(1);
    let p99 = sorted[(sorted.len() - 1) * 99 / 100];
    (mean, p99, *sorted.last().unwrap_or(&Duration::ZERO))
}

#[test]
fn world_soak_host_and_two_clients_roam_apart_with_reloads_and_a_reconnect() {
    // Three simulated minutes: three legs over eight places, two reloads, a reconnect.
    let costs = soak(3, 1);
    let (mean, p99, worst) = spread(&costs);
    eprintln!("host ticks: mean {mean:?}, p99 {p99:?}, worst {worst:?}");
    // Generous for a debug build on a loaded machine; the release numbers are the report's.
    assert!(mean < Duration::from_millis(250), "mean host tick {mean:?}");
}

#[test]
#[ignore = "the long soak for the QA report (release)"]
fn world_soak_long() {
    let minutes = std::env::var("FATHERLOAD_SOAK_MINUTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(30);
    let costs = soak(minutes, 5);
    let (mean, p99, worst) = spread(&costs);
    eprintln!(
        "{minutes} simulated minutes: host ticks mean {mean:?}, p99 {p99:?}, worst {worst:?}"
    );
    assert!(mean < Duration::from_millis(20), "mean host tick {mean:?}");
    assert!(p99 < Duration::from_millis(100), "p99 host tick {p99:?}");
}
