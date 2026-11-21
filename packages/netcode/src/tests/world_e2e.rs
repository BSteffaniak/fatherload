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
        // Part II F5: every bot climbs out, walks, jumps, picks at the ground and boards again.
        "pilot exited",
        "pilot jumped",
        "pick struck",
        "pilot boarded",
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

#[test]
fn world_bases_replicate_to_every_client() {
    use fatherload_simulation::{
        InputSequence, PlayerCommand, PlayerTickInput, TILE_SIZE, TickAction, TickInputs,
        base::{BuildCommand, Buildable, BuiltTile, Facing, ModuleKind},
        terrain::{TileKind, TilePosition},
    };

    let mut harness = harness(60, 2, LinkConditions::new(20, 3, 0.5));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    let mut world = harness.world().clone();
    let builder = world.player_ids().next().expect("a player");
    // A claim deep under the west meadow: a pocket with a stone floor, the rig on it.
    let floor = 60;
    for y in floor - 6..=floor {
        for x in 10..=30 {
            let kind = if y == floor {
                TileKind::Stone
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    let rig = world.player_mut(builder).expect("builder");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        rig.x = 20.5 * TILE_SIZE;
        rig.y = (floor as f32 - 0.5) * TILE_SIZE;
    }
    rig.credits = 10_000;
    let place = |item, x| {
        PlayerCommand::Build(BuildCommand::Place {
            item,
            at: TilePosition { x, y: floor - 1 },
            facing: Facing::Right,
        })
    };
    for command in [
        place(Buildable::Module(ModuleKind::Beacon), 18),
        place(Buildable::Module(ModuleKind::Storage), 22),
        place(Buildable::Tile(BuiltTile::Plating), 24),
    ] {
        let mut inputs = TickInputs::default();
        inputs.players.insert(
            builder,
            PlayerTickInput {
                controls: None,
                actions: vec![TickAction {
                    sequence: InputSequence::new(0),
                    command,
                }],
            },
        );
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
    }
    assert_eq!(world.bases().modules().count(), 2, "beacon and chest built");
    harness.host.new_epoch(world);
    harness.run_for(Duration::from_secs(2));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let seen = client.session.authority().expect("in game");
        assert_eq!(
            seen.bases().claims().count(),
            1,
            "every client sees the claim"
        );
        assert_eq!(seen.bases().modules().count(), 2);
        assert_eq!(
            seen.terrain()
                .tile(TilePosition {
                    x: 24,
                    y: floor - 1
                })
                .map(|tile| tile.kind),
            Some(TileKind::Built(BuiltTile::Plating)),
            "built tiles travel with the terrain"
        );
    }
}

/// A client builds mid-session through its own input stream (not a host-side world swap):
/// the host validates the build actions inside the tick, and every client, the builder
/// included, converges on the claim, the module and the built tile.
#[test]
fn world_client_build_actions_reach_the_host_and_every_client() {
    use fatherload_simulation::{
        PlayerCommand, TILE_SIZE,
        base::{BuildCommand, Buildable, BuiltTile, Facing, ModuleKind},
        terrain::{TileKind, TilePosition},
    };

    let mut harness = harness(60, 2, LinkConditions::new(30, 4, 1.0));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    for client in &mut harness.clients {
        client.bot.active = false;
    }
    let builder = crate::world::to_sim_player(harness.clients[0].session.player().expect("joined"));
    let floor = 60;
    let mut world = harness.world().clone();
    for y in floor - 6..=floor {
        for x in 10..=30 {
            let kind = if y == floor {
                TileKind::Stone
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    let rig = world.player_mut(builder).expect("builder");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        rig.x = 20.5 * TILE_SIZE;
        rig.y = (floor as f32 - 0.5) * TILE_SIZE;
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.credits = 10_000;
    harness.host.new_epoch(world);
    harness.run_for(Duration::from_secs(1));
    let place = |item, x| {
        PlayerCommand::Build(BuildCommand::Place {
            item,
            at: TilePosition { x, y: floor - 1 },
            facing: Facing::Right,
        })
    };
    harness.clients[0].queued_actions.extend([
        place(Buildable::Module(ModuleKind::Beacon), 18),
        place(Buildable::Module(ModuleKind::Storage), 22),
        place(Buildable::Tile(BuiltTile::Plating), 24),
    ]);
    harness.run_for(Duration::from_secs(2));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    harness.assert_actions_exactly_once();
    let host = harness.world();
    assert_eq!(
        host.bases().claims().count(),
        1,
        "the host staked the claim"
    );
    assert_eq!(host.bases().modules().count(), 2, "beacon and chest built");
    assert!(
        host.player(builder).expect("builder").credits < 10_000,
        "the builder paid"
    );
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let seen = client.session.authority().expect("in game");
        assert_eq!(seen.bases(), host.bases(), "every client sees the base");
        assert_eq!(
            seen.terrain()
                .tile(TilePosition {
                    x: 24,
                    y: floor - 1
                })
                .map(|tile| tile.kind),
            Some(TileKind::Built(BuiltTile::Plating)),
            "built tiles travel with the terrain"
        );
    }
}

/// The sleep vote online (Part II F4): two clients each lie down in a bed through their own
/// input streams. One in bed is not enough; once both are, the host skips the night and every
/// client converges on the same morning (the jump travels in the `Clock` section).
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one online scenario: set up, vote, converge"
)]
fn world_sleep_vote_skips_the_night_for_every_client() {
    use fatherload_simulation::{
        InputSequence, PlayerCommand, PlayerId, PlayerTickInput, TILE_SIZE, TickAction, TickInputs,
        base::{BuildCommand, Buildable, Facing, ModuleKind, ModuleService},
        clock::{DayLength, DayPhase},
        terrain::{TileKind, TilePosition},
    };

    let floor = 60;
    let park = |world: &mut WorldState, player: PlayerId, tile_x: i32| {
        let rig = world.player_mut(player).expect("rig");
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
        {
            rig.x = (tile_x as f32 + 0.5) * TILE_SIZE;
            rig.y = (floor as f32 - 0.5) * TILE_SIZE;
        }
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    };

    let mut harness = harness(60, 2, LinkConditions::new(30, 4, 1.0));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    for client in &mut harness.clients {
        client.bot.active = false;
    }
    let sleepers: Vec<PlayerId> = harness
        .clients
        .iter()
        .map(|client| crate::world::to_sim_player(client.session.player().expect("joined")))
        .collect();
    // A cavern deep under the west meadow with a claim and a bed; both rigs parked at the bed.
    let mut world = harness.world().clone();
    for y in floor - 6..=floor {
        for x in 10..=30 {
            let kind = if y == floor {
                TileKind::Stone
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    park(&mut world, sleepers[0], 18);
    world.player_mut(sleepers[0]).expect("rig").credits = 10_000;
    for (kind, x) in [(ModuleKind::Beacon, 18), (ModuleKind::Bed, 20)] {
        let mut inputs = TickInputs::default();
        inputs.players.insert(
            sleepers[0],
            PlayerTickInput {
                controls: None,
                actions: vec![TickAction {
                    sequence: InputSequence::new(0),
                    command: PlayerCommand::Build(BuildCommand::Place {
                        item: Buildable::Module(kind),
                        at: TilePosition { x, y: floor - 1 },
                        facing: Facing::Right,
                    }),
                }],
            },
        );
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
    }
    let bed = world
        .bases()
        .modules()
        .find(|module| module.kind == ModuleKind::Bed)
        .map(|module| module.id)
        .expect("bed built");
    park(&mut world, sleepers[0], 20);
    park(&mut world, sleepers[1], 21);
    // Late in the night of day 1.
    let length = world.day_length();
    assert_eq!(length, DayLength::Standard);
    world.set_world_time_millis(84 * length.slice_millis());
    assert!(world.is_night());
    harness.host.new_epoch(world);
    harness.run_for(Duration::from_secs(1));
    let sleep = || {
        PlayerCommand::Build(BuildCommand::Use {
            structure: bed,
            service: ModuleService::Sleep,
        })
    };

    harness.clients[0].queued_actions.push(sleep());
    harness.run_for(Duration::from_secs(4));
    {
        let host = harness.world();
        assert!(host.is_in_bed(sleepers[0]), "the first player lay down");
        assert!(host.is_night(), "one of two in bed is not enough");
        assert_eq!(host.awake_players(), vec![sleepers[1]]);
    }

    harness.clients[1].queued_actions.push(sleep());
    harness.run_for(Duration::from_secs(5));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    harness.assert_actions_exactly_once();
    let host = harness.world();
    let morning = host.clock();
    assert_eq!(morning.day(), 2, "the night was skipped into day 2");
    assert_eq!(morning.phase(), DayPhase::Dawn);
    assert_eq!(
        host.awake_players().len(),
        sleepers.len(),
        "everyone got up"
    );
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let seen = client.session.authority().expect("in game");
        assert_eq!(seen.clock_state(), host.clock_state());
        let seen_clock = seen.clock();
        assert_eq!(
            (seen_clock.day(), seen_clock.phase()),
            (2, DayPhase::Dawn),
            "every client wakes on the same morning"
        );
    }
}

/// People in a village (WP-PEOPLE) replicate to every client under latency: the NPC entities
/// walking their routines, and a conversation a client opens with a keeper through its own
/// input stream (the host validates the talk at the counter's door and every client sees the
/// same lines in the people ledger).
#[test]
fn world_people_and_conversations_replicate_to_every_client() {
    use fatherload_simulation::{
        PlayerCommand, SimulationParams, TILE_SIZE,
        entity::{EntityKind, EntityState},
        people::{PeopleCommand, TalkTopic},
        village::{self, BuildingKind, placement},
        worldgen::{GeneratorVersion, WorldSettings},
    };

    let mut harness = harness(60, 2, LinkConditions::new(30, 4, 1.0));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    for client in &mut harness.clients {
        client.bot.active = false;
    }
    // The same session on a world with villages: both rigs on the first village's street, the
    // talker at the trading post's door.
    let settings = WorldSettings {
        generator: GeneratorVersion::V7,
        ..WorldSettings::new(0x5EED_0017)
    };
    let villages = WorldState::new_game_with_settings(
        fatherload_simulation::PlayerId::new(99),
        &settings,
        SimulationParams::new(60),
    );
    let mut world = harness.world().clone();
    *world.terrain_mut() = villages.terrain().clone();
    let found = village::village(&settings, placement::surface_id(1)).expect("village");
    let market = found
        .buildings
        .iter()
        .position(|lot| lot.kind == BuildingKind::Market)
        .expect("a trading post");
    let talker = crate::world::to_sim_player(harness.clients[0].session.player().expect("joined"));
    let other = crate::world::to_sim_player(harness.clients[1].session.player().expect("joined"));
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    for (player, column) in [
        (talker, found.buildings[market].first_column + 2),
        (other, i32::midpoint(found.plaza.0, found.plaza.1)),
    ] {
        let rig = world.player_mut(player).expect("rig");
        rig.x = (column as f32 + 0.5) * TILE_SIZE;
        rig.y = (found.ground_row as f32 - 0.5) * TILE_SIZE;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
    harness.host.new_epoch(world);
    harness.run_for(Duration::from_secs(2));
    let keeper = found.buildings[market].keeper.expect("a keeper");
    let npc = harness
        .world()
        .entities()
        .of_kind(EntityKind::Npc)
        .find(|entity| {
            matches!(entity.state, EntityState::Npc(state)
                if state.character.village == found.id && state.character.index == keeper)
        })
        .map(|entity| entity.id)
        .expect("the keeper is about");
    harness.clients[0]
        .queued_actions
        .push(PlayerCommand::People(PeopleCommand::Talk {
            npc,
            topic: TalkTopic::Chat,
        }));
    harness.run_for(Duration::from_secs(2));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    harness.assert_actions_exactly_once();
    let host = harness.world();
    let conversation = host
        .people()
        .conversation(talker)
        .expect("the host opened the conversation");
    assert_eq!(conversation.npc, npc);
    assert!(!conversation.lines.is_empty());
    let people = host.entities().of_kind(EntityKind::Npc).count();
    assert_eq!(people, found.population.characters.len());
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let seen = client.session.authority().expect("in game");
        assert_eq!(
            seen.people(),
            host.people(),
            "every client hears the same lines"
        );
        assert_eq!(
            seen.entities().of_kind(EntityKind::Npc).count(),
            people,
            "the villagers reach every client"
        );
    }
}

/// The Moon online (WP-MOON): rigs parked on the lunar colony's pad, a facility pylon lit on
/// the host and the Moon gear in every loadout reach every client in the keyframe; a client's
/// colony deal sent through its own input stream is validated and applied by the host, and
/// everyone converges.
#[test]
fn world_moon_replicates_and_colony_deals_cross_the_wire() {
    use fatherload_simulation::{
        PlayerCommand, SimulationParams, TILE_SIZE, moon,
        terrain::TileKind,
        worldgen::{GeneratorVersion, WorldSettings},
    };

    let mut harness = harness(60, 2, LinkConditions::new(25, 3, 0.5));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    for client in &mut harness.clients {
        client.bot.active = false;
    }
    // The same session on a world with the filled Moon (the harness's own world is a small
    // bounded one).
    let settings = WorldSettings {
        generator: GeneratorVersion::V9,
        ..WorldSettings::new(0x5EED_0022)
    };
    let lunar = WorldState::new_game_with_settings(
        fatherload_simulation::PlayerId::new(99),
        &settings,
        SimulationParams::new(60),
    );
    let mut world = harness.world().clone();
    *world.terrain_mut() = lunar.terrain().clone();
    let layout = world.moon_layout().expect("V9 fills the Moon");
    let (first, last, row) = layout.pad();
    let players: Vec<_> = world.player_ids().collect();
    for (index, player_id) in players.iter().enumerate() {
        let rig = world.player_mut(*player_id).expect("player");
        let column = first + 2 + i32::try_from(index).unwrap_or(0) * 3;
        assert!(column < last);
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
        {
            rig.x = (column as f32 + 0.5) * TILE_SIZE;
            rig.y = (row as f32).mul_add(TILE_SIZE, -11.0);
        }
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.hazard_gear.pressurized_hull = moon::VACUUM_SEAL_TIER;
        rig.hazard_gear.life_support = 1;
        rig.hazard_gear.de_icer = 6;
        rig.lunar.landed = true;
        rig.credits = 5_000;
        rig.fuel = 10.0;
    }
    let plan = layout.facility;
    let first_pylon = plan
        .order
        .iter()
        .position(|place| *place == 0)
        .expect("a first pylon");
    let _ = world.strike_facility(players[0], plan.pylons[first_pylon]);
    harness.host.new_epoch(world);
    harness.run_for(Duration::from_secs(2));
    for client in &mut harness.clients {
        client
            .queued_actions
            .push(PlayerCommand::Moon(moon::MoonCommand::Colony(
                moon::ColonyOffer::Refuel,
            )));
    }
    harness.run_for(Duration::from_secs(3));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    let host = harness.world();
    for player_id in &players {
        let rig = host.player(*player_id).expect("player");
        assert!(
            rig.fuel > 90.0,
            "the colony refuelled {player_id:?}: {}",
            rig.fuel
        );
        assert!(rig.credits < 5_000);
    }
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let seen = client.session.authority().expect("in game");
        assert_eq!(
            seen.terrain()
                .tile(plan.pylons[first_pylon])
                .map(|tile| tile.kind),
            Some(TileKind::Lunar(moon::LunarTile::PylonLit)),
            "the lit pylon travels with the terrain"
        );
    }
}

#[test]
fn world_enemy_difficulty_is_the_hosts_call() {
    use fatherload_simulation::{EnemyDifficulty, PlayerCommand};

    let mut harness = harness(60, 2, LinkConditions::new(30, 4, 1.0));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    for client in &mut harness.clients {
        client.bot.active = false;
    }
    assert_eq!(harness.world().enemy_difficulty(), EnemyDifficulty::Normal);
    // A client (a modified build, or a stale menu) asks for Peaceful: the host acknowledges
    // the action so it is not resent, but never applies it.
    harness.clients[0]
        .queued_actions
        .push(PlayerCommand::SetEnemyDifficulty(EnemyDifficulty::Peaceful));
    harness.run_for(Duration::from_secs(2));
    harness.settle(Duration::from_secs(1));
    assert_eq!(
        harness.world().enemy_difficulty(),
        EnemyDifficulty::Normal,
        "the host keeps its setting"
    );
    let client_id = harness.clients[0].id;
    assert_eq!(
        harness
            .host
            .client_stats(client_id)
            .map(|stats| stats.host_only_refused),
        Some(1)
    );
    for client in &harness.clients {
        assert_eq!(client.session.unacked_actions_len(), 0, "acknowledged");
        let seen = client.session.authority().expect("in game");
        assert_eq!(seen.enemy_difficulty(), EnemyDifficulty::Normal);
    }
    assert_converged(&harness);
}

/// Contractor-built homes (WP-HOME) replicate to every client under latency: a client
/// decorates its finished cabin and signs a contract for a new room through its own input
/// stream, the host validates both at the plot, and every client sees the same plots, decor
/// and contract in the shared homes section.
#[test]
fn world_homes_decor_and_contracts_replicate_to_every_client() {
    use fatherload_simulation::{
        PlayerCommand, TILE_SIZE,
        home::{Blueprint, Expansion, Furniture, HomeCommand, HomeJob, RoomKind, plot},
    };

    let mut harness = harness(60, 2, LinkConditions::new(30, 4, 1.0));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    for client in &mut harness.clients {
        client.bot.active = false;
    }
    let owner = crate::world::to_sim_player(harness.clients[0].session.player().expect("joined"));
    let mut world = harness.world().clone();
    let settings = *world.world_settings();
    let site = plot::listed_sites_between(&settings, -2_000, 2_000)
        .into_iter()
        .find(|site| site.size >= fatherload_simulation::home::PlotSize::Small)
        .expect("a listed site");
    let plot = world.scenario_home(owner, site, Some(Blueprint::Cabin));
    world.grant_furniture(owner, Furniture::Plant, 1);
    if let Some(rig) = world.player_mut(owner) {
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
        {
            rig.x = (site.middle() as f32 + 0.5) * TILE_SIZE;
            rig.y = (site.ground_row as f32 - 0.5) * TILE_SIZE;
        }
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.credits = 50_000;
    }
    harness.host.new_epoch(world);
    harness.run_for(Duration::from_secs(1));
    let contractor = harness
        .world()
        .contractors_for(plot, owner)
        .first()
        .and_then(|offer| offer.character);
    harness.clients[0]
        .queued_actions
        .push(PlayerCommand::Home(HomeCommand::Place {
            plot,
            room: 0,
            cell: 3,
            furniture: Furniture::Plant,
        }));
    harness.clients[0]
        .queued_actions
        .push(PlayerCommand::Home(HomeCommand::Hire {
            plot,
            job: HomeJob::Expand(Expansion::Room(RoomKind::Kitchen)),
            contractor,
            supply: false,
        }));
    harness.run_for(Duration::from_secs(2));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    harness.assert_actions_exactly_once();
    let host = harness.world();
    let found = host.homes().plot(plot).expect("the plot");
    let home = found.home.as_ref().expect("the cabin");
    assert_eq!(
        home.rooms[0]
            .decor
            .piece_at(3)
            .map(|(_, piece)| piece.furniture),
        Some(Furniture::Plant),
        "the plant was placed on the host"
    );
    assert!(found.construction.is_some(), "the contract was signed");
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let seen = client.session.authority().expect("in game");
        assert_eq!(
            seen.homes(),
            host.homes(),
            "every client sees the same homes"
        );
    }
}

/// Tomb runs online (WP-TOMBS): both clients' pilots enter a V10 tomb site together on the
/// host, the party reaches the relic picks, each client sends its own pick through its input
/// stream, and when both have picked the host builds the next floor in the run slot. The run
/// (floor, relics, loot), the floor's tiles and the pilots' top-down or side view converge on
/// every client with no hash mismatch.
#[test]
fn world_tomb_runs_replicate_and_relic_picks_cross_the_wire() {
    use fatherload_simulation::{
        PlayerCommand, SimulationParams,
        tomb::{RunPhase, TombCommand},
        worldgen::{GeneratorVersion, WorldSettings, features::TileRect, tomb_sites},
    };

    let mut harness = harness(60, 2, LinkConditions::new(25, 3, 0.5));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    for client in &mut harness.clients {
        client.bot.active = false;
    }
    let settings = WorldSettings {
        generator: GeneratorVersion::V10,
        ..WorldSettings::new(0x5EED_0027)
    };
    let tombed = WorldState::new_game_with_settings(
        fatherload_simulation::PlayerId::new(99),
        &settings,
        SimulationParams::new(60),
    );
    let mut world = harness.world().clone();
    *world.terrain_mut() = tombed.terrain().clone();
    let site = tomb_sites::sites_touching(
        &settings,
        &TileRect {
            min: fatherload_simulation::terrain::TilePosition { x: -6_000, y: -100 },
            max: fatherload_simulation::terrain::TilePosition { x: 6_000, y: 900 },
        },
    )
    .into_iter()
    .next()
    .expect("a tomb site");
    let party: Vec<_> = harness
        .clients
        .iter()
        .map(|client| crate::world::to_sim_player(client.session.player().expect("joined")))
        .collect();
    let id = world
        .scenario_enter_tomb(&site, &party)
        .expect("the run starts");
    world.scenario_tomb_stairs(party[0]);
    assert_eq!(
        world.tombs().runs.get(&id).map(|run| run.phase),
        Some(RunPhase::Picking)
    );
    harness.host.new_epoch(world);
    harness.run_for(Duration::from_secs(2));
    for client in &mut harness.clients {
        client
            .queued_actions
            .push(PlayerCommand::Tomb(TombCommand::PickRelic(Some(0))));
    }
    harness.run_for(Duration::from_secs(3));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    harness.assert_actions_exactly_once();
    let host = harness.world();
    let run = host.tombs().runs.get(&id).expect("the run goes on");
    assert_eq!(run.floor, 1, "both picked: the party went down");
    assert_eq!(run.phase, RunPhase::Exploring);
    for member in &run.members {
        assert_eq!(
            member.relics.len(),
            1,
            "{:?} picked its own relic",
            member.player
        );
    }
    let origin = tomb_sites::slot_origin(run.slot);
    let floor = WorldState::tomb_floor_of(run);
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let seen = client.session.authority().expect("in game");
        assert_eq!(seen.tombs(), host.tombs(), "every client sees the same run");
        for (tile, kind) in floor.cells.iter().take(64) {
            let at = fatherload_simulation::terrain::TilePosition {
                x: origin.x + tile.x,
                y: origin.y + tile.y,
            };
            assert_eq!(
                seen.terrain().tile(at).map(|found| found.kind),
                host.terrain().tile(at).map(|found| found.kind),
                "the floor's tiles travel with the terrain ({kind:?} at {at:?})"
            );
        }
        for player in &party {
            assert_eq!(
                seen.pilot_of(*player).map(|(_, _, state)| state.view),
                host.pilot_of(*player).map(|(_, _, state)| state.view)
            );
        }
    }
}

/// Side-view tomb traps online (WP-TOMBS T28): a party of two clients enters a side-view tomb
/// whose first floor has a collapsing bridge, with the first pilot standing on one of its
/// slabs. The slab crumbles and falls on the host's clock (a terrain change and a
/// `TombTrapSprung` cue), the clock-driven traps and the tomb creatures play out, and every
/// client converges on the same run, slabs and tiles with no hash mismatch.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one online scenario, set up and checked"
)]
fn world_side_view_tomb_traps_replicate() {
    use fatherload_simulation::{
        SimulationParams,
        balance::tombs::find_site,
        base::BuiltTile,
        terrain::{TileKind, TilePosition},
        tomb::{
            SiteStyle,
            floor::plan_floor,
            realize::{TrapKind, realizer_for},
        },
        worldgen::{GeneratorVersion, WorldSettings, tomb_sites},
    };

    let mut harness = harness(60, 2, LinkConditions::new(25, 3, 0.5));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    for client in &mut harness.clients {
        client.bot.active = false;
    }
    let is_slab = |kind: &TileKind| matches!(kind, TileKind::Built(BuiltTile::TombTrap { kind, .. }) if *kind == TrapKind::Crumble.index());
    let (settings, site) = [7_u64, 11, 4_242, 90_001]
        .into_iter()
        .find_map(|seed| {
            let settings = WorldSettings {
                generator: GeneratorVersion::V10,
                ..WorldSettings::new(seed)
            };
            find_site(&settings, SiteStyle::SideView, 1).map(|site| (settings, site))
        })
        .expect("a side-view site");
    let run = (0..400)
        .find(|run| {
            realizer_for(SiteStyle::SideView)
                .realize(&plan_floor(&site, *run, 0))
                .cells
                .iter()
                .any(|(_, kind)| is_slab(kind))
        })
        .expect("a floor with a collapsing bridge");
    let tombed = WorldState::new_game_with_settings(
        fatherload_simulation::PlayerId::new(99),
        &settings,
        SimulationParams::new(60),
    );
    let mut world = harness.world().clone();
    *world.terrain_mut() = tombed.terrain().clone();
    let party: Vec<_> = harness
        .clients
        .iter()
        .map(|client| crate::world::to_sim_player(client.session.player().expect("joined")))
        .collect();
    world.scenario_next_tomb_run(site.id, run);
    let id = world
        .scenario_enter_tomb(&site, &party)
        .expect("the run starts");
    let slab = {
        let tomb = world.tombs().runs.get(&id).expect("the run");
        let origin = tomb_sites::slot_origin(tomb.slot);
        WorldState::tomb_floor_of(tomb)
            .cells
            .iter()
            .find(|(_, kind)| is_slab(kind))
            .map(|(tile, _)| TilePosition {
                x: origin.x + tile.x,
                y: origin.y + tile.y,
            })
            .expect("a slab")
    };
    let (pilot, _, _) = world.pilot_of(party[0]).expect("a pilot");
    #[allow(clippy::cast_precision_loss, reason = "tile coordinates")]
    let (x, feet) = (
        (slab.x as f32 + 0.5) * fatherload_simulation::terrain::TILE_SIZE,
        (slab.y as f32).mul_add(fatherload_simulation::terrain::TILE_SIZE, -0.25),
    );
    world.edit_pilot(pilot, |body, _| {
        body.x = x;
        body.y = feet;
        body.previous_x = x;
        body.previous_y = feet;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
    });
    harness.host.new_epoch(world);
    harness.run_for(Duration::from_secs(3));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    let host = harness.world();
    assert_eq!(
        host.terrain().tile(slab).map(|found| found.kind),
        Some(TileKind::Air),
        "the slab fell on the host"
    );
    assert!(
        host.tombs()
            .runs
            .get(&id)
            .is_some_and(|run| run.crumbling.iter().any(|fallen| fallen.tile == slab)),
        "the run remembers the fallen slab"
    );
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let seen = client.session.authority().expect("in game");
        // The fight goes on to the last tick (a pilot may be downed a tick before the end), so
        // the whole run is compared through the hashes; the slabs settled seconds ago.
        assert_eq!(
            seen.tombs().runs.get(&id).map(|run| &run.crumbling),
            host.tombs().runs.get(&id).map(|run| &run.crumbling),
            "every client sees the same fallen slabs"
        );
        assert_eq!(
            seen.terrain().tile(slab).map(|found| found.kind),
            host.terrain().tile(slab).map(|found| found.kind),
            "the fallen slab travels with the terrain"
        );
    }
}

/// Top-down crypts online (WP-TOMBS T29): both clients' pilots crawl a crypt floor seen from
/// above, walking eight ways and dodge-rolling under client prediction while crypt creatures
/// hunt them and the floor's traps cycle; every client's world converges on the host's with no
/// hash mismatch, the pilots' rolls and headings included.
#[test]
#[allow(
    clippy::cast_precision_loss,
    reason = "small tile coordinates to world pixels"
)]
fn world_crypt_runs_predict_rolls_and_converge() {
    use fatherload_simulation::{
        SimulationParams,
        pilot::PilotView,
        tomb::SiteStyle,
        worldgen::{GeneratorVersion, WorldSettings, features::TileRect, tomb_sites},
    };

    let mut harness = harness(60, 2, LinkConditions::new(40, 5, 1.0));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    let settings = WorldSettings {
        generator: GeneratorVersion::V10,
        ..WorldSettings::new(0x5EED_0029)
    };
    let crypted = WorldState::new_game_with_settings(
        fatherload_simulation::PlayerId::new(99),
        &settings,
        SimulationParams::new(60),
    );
    let mut world = harness.world().clone();
    *world.terrain_mut() = crypted.terrain().clone();
    let site = tomb_sites::sites_touching(
        &settings,
        &TileRect {
            min: fatherload_simulation::terrain::TilePosition { x: -9_000, y: -100 },
            max: fatherload_simulation::terrain::TilePosition { x: 9_000, y: 900 },
        },
    )
    .into_iter()
    .find(|site| site.style == SiteStyle::TopDown)
    .expect("a crypt site");
    let party: Vec<_> = harness
        .clients
        .iter()
        .map(|client| crate::world::to_sim_player(client.session.player().expect("joined")))
        .collect();
    let id = world
        .scenario_enter_tomb(&site, &party)
        .expect("the run starts");
    // A pack waiting in the entry room.
    if let Some(run) = world.tombs().runs.get(&id).cloned()
        && let Some(room) = run.rooms.first()
    {
        for (dx, kind) in [
            (1, fatherload_simulation::EnemyKind::CryptSkeleton),
            (9, fatherload_simulation::EnemyKind::Cultist),
        ] {
            let x = ((room.interior.min.x + dx) as f32).mul_add(32.0, 16.0);
            let y = ((room.interior.min.y + 1) as f32).mul_add(32.0, 16.0);
            let _ = world.spawn_enemy(kind, x, y);
        }
    }
    harness.host.new_epoch(world);
    // The bots walk, hop (roll) and swing on their usual script, without one-shot actions.
    for client in &mut harness.clients {
        client.bot.issue_actions = false;
    }
    harness.run_for(Duration::from_secs(20));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    let host = harness.world();
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let seen = client.session.authority().expect("in game");
        assert_eq!(
            seen.tombs(),
            host.tombs(),
            "every client sees the same crypt"
        );
        for player in &party {
            let (Some(theirs), Some(ours)) = (seen.pilot_of(*player), host.pilot_of(*player))
            else {
                continue;
            };
            // Positions are compared by the per-tick hashes above (the authority may trail
            // the host by a tick or two here); the view is settled.
            assert_eq!(theirs.2.view, ours.2.view);
        }
    }
    assert!(
        party.iter().any(|player| host
            .pilot_of(*player)
            .is_some_and(|(_, _, state)| state.view == PilotView::TopDown)
            || host.tomb_run_of(*player).is_none()),
        "the party played the crypt from above"
    );
}
