//! End-to-end tests on the real gameplay world (`fatherload_simulation::WorldState`).
//!
//! One host and N clients over seeded `LinkSim`s, driven by bots that dig down (placing bombs,
//! scanning and deploying infrastructure on the way), fly up, land back at the depot, sell, and
//! try to buy upgrades and refuel (which the host refuses away from the shop and fuel station).
//! Every message crosses the real postcard codec.

use std::time::Duration;

use fatherload_simulation::WorldState;

use super::support::{assert_bounded, assert_converged};
use crate::harness::{
    Harness, HarnessConfig, HarnessSim, format_sweep, parameter_sweep, sweep_links,
};
use crate::{LinkConditions, SessionParams};

type WorldHarness = Harness<WorldState>;

fn harness(rate: u32, clients: usize, link: LinkConditions) -> WorldHarness {
    WorldHarness::new(HarnessConfig::new(SessionParams::new(rate), clients, link))
}

/// The bots really played: terrain was dug and every kind of scripted action took effect.
fn assert_played(harness: &WorldHarness) {
    let fresh = WorldState::harness_world(&harness.config);
    assert!(
        harness.world().play_progress() > fresh.play_progress() + harness.clients.len(),
        "bots dug terrain"
    );
    // The surface drops settled and some were disturbed by digging or blasts since.
    let entities = harness.world().entities();
    assert!(
        entities.iter().any(|entity| entity.body.asleep),
        "drops sleep"
    );
    // Every surface drop starts in the air above the ground: one below the surface fell into a
    // shaft or was dug out by a bot (every broken tile drops), or the bots' magnets took some.
    let fresh_drops = fresh.entities().len();
    assert!(
        entities
            .iter()
            .any(|entity| entity.body.y > 7.0 * fatherload_simulation::TILE_SIZE)
            || entities.len() != fresh_drops,
        "drops were dug, thrown or collected: {:?}",
        entities
            .iter()
            .map(|entity| (entity.body.x, entity.body.y))
            .collect::<Vec<_>>()
    );
    for label in [
        "applied scanner",
        "applied infrastructure",
        "applied bomb",
        "bomb detonated",
        "applied explosive",
        "applied detonator",
        "explosive detonated",
        // WP-DROPS: the bots' digging and blasts dropped loot their magnets collected (host
        // authoritative; clients predicted the same pickups, see `assert_converged`).
        "items collected",
        "weapon fired",
    ] {
        assert!(
            harness.event_counts.get(label).copied().unwrap_or(0) > 0,
            "{label}: {:?}",
            harness.event_counts
        );
    }
}

fn assert_economy_played(harness: &WorldHarness) {
    // Sales at the depot apply; shop and fuel transactions issued there are refused by the host.
    for label in ["applied sell", "ignored action"] {
        assert!(
            harness.event_counts.get(label).copied().unwrap_or(0) > 0,
            "{label}: {:?}",
            harness.event_counts
        );
    }
    for label in ["applied buy", "applied refuel"] {
        assert_eq!(
            harness.event_counts.get(label).copied().unwrap_or(0),
            0,
            "{label} away from its building: {:?}",
            harness.event_counts
        );
    }
}

#[test]
fn world_zero_latency_has_zero_corrections() {
    let mut harness = harness(60, 3, LinkConditions::perfect());
    harness.run_ticks(60 * 40);
    harness.settle(Duration::from_secs(1));
    assert!(harness.all_in_game());
    let summary = harness.summary();
    assert_eq!(summary.corrections, 0, "{summary:?}");
    assert_eq!(summary.clock_resyncs, 0);
    assert_converged(&harness);
    assert_bounded(&harness);
    harness.assert_actions_exactly_once();
    assert_played(&harness);
    assert_economy_played(&harness);
}

#[test]
fn world_private_state_reaches_only_its_owner() {
    use crate::world::to_sim_player;

    let mut harness = harness(60, 3, LinkConditions::perfect());
    harness.run_ticks(60 * 40);
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    let host = harness.world();
    let template_credits = host.player_template().credits;
    let mut other_rich = false;
    for client in &harness.clients {
        let me = client.session.player().expect("joined");
        let authority = client.session.authority().expect("in game");
        for player in host.player_ids() {
            let host_rig = host.player(player).expect("host rig");
            let client_rig = authority.player(player).expect("roster replicated");
            assert_eq!(
                client_rig.hull_level, host_rig.hull_level,
                "rig build is public"
            );
            if player == to_sim_player(me) {
                assert_eq!(client_rig.credits, host_rig.credits);
                assert_eq!(
                    authority.discovered_tile_count(player),
                    host.discovered_tile_count(player)
                );
                assert!(host.discovered_tile_count(player) > 0);
            } else {
                assert_eq!(client_rig.credits, template_credits, "loadouts are private");
                assert_eq!(authority.discovered_tile_count(player), 0);
                other_rich |= host_rig.credits != template_credits;
            }
        }
    }
    assert!(other_rich, "some rig's credits changed on the host");

    // Action results (which name purchases and whether they were affordable) reach only the
    // player who submitted the action.
    for client in &mut harness.clients {
        let me = client.session.player().expect("joined");
        let mut own = 0;
        for presented in client.session.drain_events() {
            if let crate::world::WorldEvent::ActionResolved { .. } = presented.event.payload {
                assert_eq!(presented.event.player, Some(me), "another player's action");
                own += 1;
            }
        }
        assert!(own > 0, "the client saw its own action results");
    }
}

/// Has `player` accept `quest` on `world` (standing at the giver's door for one tick).
fn accept_quest(
    world: &mut WorldState,
    player: fatherload_simulation::PlayerId,
    quest: fatherload_simulation::QuestId,
    crew: bool,
) {
    use fatherload_simulation::{
        InputSequence, PlayerCommand, PlayerTickInput, TILE_SIZE, TickAction, TickInputs,
        quest::binding::giver_door, service::SessionServiceRequest,
    };
    let door = giver_door(quest.def().expect("quest").giver);
    let rig = world.player_mut(player).expect("rig");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        rig.x = (door.x as f32 + 0.5) * TILE_SIZE;
        rig.y = (door.y as f32 + 0.5) * TILE_SIZE;
    }
    let mut inputs = TickInputs::default();
    inputs.players.insert(
        player,
        PlayerTickInput {
            controls: None,
            actions: vec![TickAction {
                sequence: InputSequence::new(0),
                command: PlayerCommand::Service(SessionServiceRequest::AcceptQuest { quest, crew }),
            }],
        },
    );
    let tick = world.simulation_tick();
    let output = world.simulate_tick(tick, &inputs);
    assert!(
        output.events.iter().any(|event| matches!(
            event.kind,
            fatherload_simulation::GameplayEventKind::QuestAccepted { .. }
        )),
        "{quest:?} accepted"
    );
}

#[test]
fn world_quests_replicate_solo_privately_and_crew_quests_to_everyone() {
    use fatherload_simulation::{QuestId, quest::QuestLog};

    let mut harness = harness(60, 2, LinkConditions::new(20, 3, 0.5));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    let mut world = harness.world().clone();
    let players: Vec<_> = world.player_ids().collect();
    let (lead, wing) = (players[0], players[1]);
    // The lead takes a crew job, the wing a private solo job.
    {
        // A crew delivery job: it needs no generated features, so it binds in the harness world.
        let log = &mut world.player_mut(lead).expect("lead").quests;
        log.reputation
            .insert(fatherload_simulation::QuestGiver::Kade, 10);
        log.deepest_row = fatherload_simulation::worldgen::strata::scaled_row(
            &fatherload_simulation::worldgen::WorldSettings::default(),
            45,
        );
    }
    accept_quest(&mut world, lead, QuestId(104), true);
    accept_quest(&mut world, wing, QuestId(101), false);
    assert_eq!(world.crew_quests().active.len(), 1);
    harness.host.new_epoch(world);
    harness.run_for(Duration::from_secs(3));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);

    let host = harness.world();
    assert_eq!(
        host.crew_quests().active.len(),
        1,
        "the crew job is still on"
    );
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let me = crate::world::to_sim_player(client.session.player().expect("joined"));
        let authority = client.session.authority().expect("in game");
        assert_eq!(
            authority.crew_quests(),
            host.crew_quests(),
            "crew quests are shared"
        );
        for player in [lead, wing] {
            let seen = &authority.player(player).expect("rig").quests;
            if player == me {
                // `deepest_row` follows the rig's live depth, so the client's latest authority
                // (a few ticks behind the host's current tick) can trail it by a row while the
                // rig is still settling; `assert_converged` already compared matching ticks.
                let host_log = &host.player(player).expect("rig").quests;
                let mut seen = seen.clone();
                seen.deepest_row = host_log.deepest_row;
                assert_eq!(&seen, host_log);
            } else {
                assert_eq!(seen, &QuestLog::default(), "quest logs are private");
            }
        }
    }
    assert!(
        host.player(wing)
            .expect("wing")
            .quests
            .active
            .iter()
            .any(|quest| quest.quest == QuestId(101)),
        "the solo job is on the host"
    );
}

#[test]
fn world_bad_internet_corrections_are_bounded_and_actions_exactly_once() {
    // 150 ms +/- 20 ms jitter with 3 % loss (plus duplication and reordering).
    let link = LinkConditions::new(150, 20, 3.0).with_chaos(2.0, 5.0);
    let mut harness = harness(60, 3, link);
    harness.run_for(Duration::from_secs(3));
    harness.reset_client_counters();
    harness.run_ticks(60 * 100);
    harness.settle(Duration::from_secs(3));
    let summary = harness.summary();
    println!("{summary:#?}");
    assert!(
        summary.corrections_per_client_sec < 0.5,
        "corrections stay rare: {summary:?}"
    );
    assert!(
        summary.mean_correction < 1.0,
        "corrections stay small (tiles): {summary:?}"
    );
    assert_converged(&harness);
    assert_bounded(&harness);
    harness.assert_actions_exactly_once();
    assert_played(&harness);
}

#[test]
fn world_works_at_tick_rates_30_60_120() {
    for rate in [30_u32, 60, 120] {
        // Zero latency: still zero corrections at every rate.
        let mut perfect = harness(rate, 2, LinkConditions::perfect());
        perfect.run_for(Duration::from_secs(12));
        perfect.settle(Duration::from_secs(1));
        let summary = perfect.summary();
        assert_eq!(summary.corrections, 0, "rate {rate}: {summary:?}");
        assert_converged(&perfect);
        perfect.assert_actions_exactly_once();
        assert_eq!(perfect.world().tick_rate_hz(), rate);

        // Moderate internet.
        let link = LinkConditions::new(40, 5, 1.0).with_chaos(1.0, 2.0);
        let mut config = HarnessConfig::new(SessionParams::new(rate), 2, link);
        config.tuning.snapshot_rate_hz = rate.min(30);
        let mut lossy = WorldHarness::new(config);
        lossy.run_for(Duration::from_secs(12));
        lossy.settle(Duration::from_secs(2));
        assert!(lossy.host_tick().0 >= u64::from(rate) * 13, "rate {rate}");
        assert_converged(&lossy);
        assert_bounded(&lossy);
        lossy.assert_actions_exactly_once();
        let summary = lossy.summary();
        assert!(
            summary.corrections_per_client_sec < 1.0,
            "rate {rate}: {summary:?}"
        );
    }
}

#[test]
fn world_scales_to_2_4_and_16_clients() {
    for (clients, seconds) in [(2_usize, 12_u64), (4, 12), (16, 6)] {
        let link = LinkConditions::new(30, 5, 1.0).with_chaos(1.0, 1.0);
        let mut harness = harness(60, clients, link);
        harness.run_for(Duration::from_secs(seconds));
        harness.settle(Duration::from_secs(2));
        assert_eq!(harness.world().player_count(), clients);
        assert!(harness.all_in_game());
        assert_converged(&harness);
        assert_bounded(&harness);
        harness.assert_actions_exactly_once();
        let summary = harness.summary();
        println!("{clients} clients: {summary:?}");
        assert!(
            summary.corrections_per_client_sec < 1.0,
            "{clients} clients: {summary:?}"
        );
    }
}

#[test]
fn world_join_in_progress_converges() {
    let link = LinkConditions::new(20, 3, 0.5);
    let mut harness = harness(60, 2, link);
    harness.run_for(Duration::from_secs(4));
    let late = harness.add_client(None);
    harness.run_for(Duration::from_secs(6));
    harness.settle(Duration::from_secs(1));
    assert!(harness.all_in_game());
    assert!(harness.clients[late].session.player().is_some());
    assert_eq!(harness.world().player_count(), 3);
    assert_converged(&harness);
    harness.assert_actions_exactly_once();
}

#[test]
#[ignore = "real-world parameter sweep report; run with --ignored --nocapture"]
fn world_parameter_sweep_report() {
    let rows =
        parameter_sweep::<WorldState>(&[30, 60, 120], &[10, 20, 30, 60], &sweep_links(), 3, 15);
    println!("{}", format_sweep(&rows));
}

#[test]
fn world_rig_cosmetics_replicate_to_everyone_and_unlocks_stay_private() {
    use fatherload_simulation::{
        CosmeticItem, InputSequence, PlayerCommand, PlayerTickInput, RigCosmetics, TILE_SIZE,
        TickAction, TickInputs,
        cosmetics::{Decal, FlameColor, PaintColor},
        economy::SurfaceZone,
        service::SessionServiceRequest,
        surface::SURFACE_BUILDINGS,
    };

    let mut harness = harness(60, 2, LinkConditions::new(20, 3, 0.5));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    let mut world = harness.world().clone();
    let players: Vec<_> = world.player_ids().collect();
    let painter = players[0];
    let look = RigCosmetics {
        primary: PaintColor::Cobalt,
        trim: PaintColor::Crimson,
        flame: FlameColor::Blue,
        decal: Decal::Bolt,
        ..RigCosmetics::default()
    };
    // The painter repaints at the Repair Garage's paint bay inside the fixed tick.
    let garage = SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == SurfaceZone::Repair)
        .expect("repair garage");
    let rig = world.player_mut(painter).expect("painter");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        rig.x = (garage.tile_x as f32 + 2.5) * TILE_SIZE;
        rig.y = 4.5 * TILE_SIZE;
    }
    rig.unlock_cosmetic(CosmeticItem::Decal(Decal::Crown));
    let mut inputs = TickInputs::default();
    inputs.players.insert(
        painter,
        PlayerTickInput {
            controls: None,
            actions: vec![TickAction {
                sequence: InputSequence::new(0),
                command: PlayerCommand::Service(SessionServiceRequest::CustomizeRig {
                    cosmetics: Some(look),
                }),
            }],
        },
    );
    let tick = world.simulation_tick();
    let output = world.simulate_tick(tick, &inputs);
    assert!(
        output.events.iter().any(|event| matches!(
            event.kind,
            fatherload_simulation::GameplayEventKind::RigCustomized { .. }
        )),
        "the paint job applied"
    );
    harness.host.new_epoch(world);
    harness.run_for(Duration::from_secs(2));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let me = crate::world::to_sim_player(client.session.player().expect("joined"));
        let seen = client
            .session
            .authority()
            .expect("in game")
            .player(painter)
            .expect("painter replicated");
        assert_eq!(seen.cosmetics, Some(look), "every client sees the paint");
        assert_eq!(
            seen.cosmetic_unlocks
                .items
                .contains(&CosmeticItem::Decal(Decal::Crown)),
            me == painter,
            "unlocks reach only their owner"
        );
    }
}

#[test]
fn world_enemies_replicate_under_latency_and_hit_rigs() {
    use fatherload_simulation::{
        EnemyKind, TILE_SIZE,
        entity::EntityKind,
        terrain::{TileKind, TilePosition},
    };

    // 80 ms +/- 10 ms with a little loss: enemies chase the rigs and replicate as entities.
    let mut harness = harness(60, 2, LinkConditions::new(80, 10, 1.0));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    let mut world = harness.world().clone();
    let players: Vec<_> = world.player_ids().collect();
    // An arena deep east of the town: the rigs on its floor with a swarm, a spitter and a drone.
    let (x0, floor) = (260, 120);
    for y in floor - 8..floor {
        for x in x0 - 12..x0 + 12 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
    }
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    let px = |tile: i32| tile as f32 * TILE_SIZE;
    for (index, player) in (0_i32..).zip(&players) {
        let rig = world.player_mut(*player).expect("rig");
        rig.x = px(x0 - 4 + index * 3);
        rig.y = px(floor) - 11.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
    for dx in [-8, -6, 5, 7] {
        let _ = world.spawn_enemy(EnemyKind::CaveBat, px(x0 + dx), px(floor - 5));
    }
    let _ = world.spawn_enemy(EnemyKind::MiningDrone, px(x0 + 9), px(floor - 6));
    let _ = world.spawn_enemy(
        EnemyKind::AcidSpitter,
        px(x0 + 10),
        px(floor) - EnemyKind::AcidSpitter.stats().radius - 1.0,
    );
    let hull_before: Vec<f32> = players
        .iter()
        .map(|player| world.player(*player).expect("rig").hull)
        .collect();
    harness.host.new_epoch(world);
    // The bots soon fly off (and the enemies left behind despawn), so watch the replicas as the
    // fight happens.
    let mut seen = vec![0_usize; harness.clients.len()];
    for _ in 0..60 {
        harness.run_for(Duration::from_millis(100));
        for (count, client) in seen.iter_mut().zip(&harness.clients) {
            if let Some(replica) = client.session.authority() {
                *count = (*count).max(replica.entities().count_of(EntityKind::Enemy));
            }
        }
    }
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    assert_bounded(&harness);
    let host = harness.world();
    assert!(
        players
            .iter()
            .zip(&hull_before)
            .any(|(player, before)| host.player(*player).is_some_and(|rig| rig.hull < *before)),
        "the enemies hurt a rig"
    );
    for (client, count) in harness.clients.iter().zip(seen) {
        assert_eq!(client.hash_mismatches, 0);
        assert!(count > 0, "enemies near the rig reach the client");
    }
}
