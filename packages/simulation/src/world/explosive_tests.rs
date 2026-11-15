//! Scenarios for the explosive kinds: sticking, remote and sequenced detonation, cluster
//! splitting, shaped tunnels, loot-safe and incendiary blasts, flares, pushes, purchase, saves and
//! replication.

use super::{WorldImport, quests::QuestNotes};
use crate::{
    BlastDirection, ExplosiveKind, GameplayEventKind, PlacedBomb, PlayerCommand, PlayerId,
    SessionServiceRequest, SimulationTick, ThrowAim, TickInputs, WorldState,
    explosive::{CLUSTER_BOMBLETS, REMOTE_FUSE, explosive_offer},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);

/// An open air box (tiles 8..=32 both ways) over a bedrock floor at row 32, with the rig parked
/// out of the way at the top left.
fn arena() -> WorldState {
    let mut world = WorldState::from_import(&WorldImport::new_game(PLAYER));
    for y in 8..=32 {
        for x in 8..=40 {
            let kind = if y == 32 {
                TileKind::Bedrock
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    park(&mut world, PLAYER, 9.5, 9.5);
    world
}

fn park(world: &mut WorldState, player_id: PlayerId, tile_x: f32, tile_y: f32) {
    let player = world.player_mut(player_id).expect("player");
    player.x = tile_x * TILE_SIZE;
    player.y = tile_y * TILE_SIZE;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

fn fill(
    world: &mut WorldState,
    xs: std::ops::RangeInclusive<i32>,
    ys: std::ops::RangeInclusive<i32>,
    kind: TileKind,
) {
    for y in ys {
        for x in xs.clone() {
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
}

fn kind_at(world: &WorldState, x: i32, y: i32) -> TileKind {
    world
        .terrain()
        .tile(TilePosition { x, y })
        .expect("tile")
        .kind
}

fn charge(
    id: u64,
    kind: ExplosiveKind,
    tile_x: f32,
    tile_y: f32,
    deadline: SimulationTick,
) -> PlacedBomb {
    let x = tile_x * TILE_SIZE;
    let y = tile_y * TILE_SIZE;
    PlacedBomb {
        id,
        x,
        y,
        previous_x: x,
        previous_y: y,
        detonation_tick: deadline,
        kind,
        owner: Some(PLAYER),
        ..PlacedBomb::default()
    }
}

/// Bomb-only ticks (as the plain bomb tests do).
fn advance(world: &mut WorldState, count: u64) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..count {
        let tick = world.simulation_tick().next();
        let mut output = crate::TickOutput {
            tick,
            ..crate::TickOutput::default()
        };
        world.advance_bombs(tick, &mut QuestNotes::default(), &mut output);
        events.extend(output.events.into_iter().map(|event| event.kind));
        world.set_simulation_tick(tick);
    }
    events
}

/// Full simulation ticks with `command` from `player_id` on the first.
fn act(
    world: &mut WorldState,
    player_id: PlayerId,
    command: PlayerCommand,
) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::default().act(player_id, command);
    world
        .simulate_tick(world.simulation_tick(), &inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn detonations(events: &[GameplayEventKind], kind: ExplosiveKind) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(event, GameplayEventKind::ExplosiveDetonated { kind: k, .. } if *k == kind)
        })
        .count()
}

fn far_future() -> SimulationTick {
    SimulationTick::new(1_000_000)
}

#[test]
fn a_thrown_sticky_bomb_sticks_to_the_wall_it_hits_then_blows() {
    let mut world = arena();
    fill(&mut world, 16..=16, 8..=31, TileKind::Stone);
    park(&mut world, PLAYER, 13.5, 20.5);
    world
        .player_mut(PLAYER)
        .unwrap()
        .add_explosives(ExplosiveKind::Sticky, 1);
    let events = act(
        &mut world,
        PLAYER,
        PlayerCommand::DeployExplosive {
            kind: ExplosiveKind::Sticky,
            aim: ThrowAim::RIGHT,
        },
    );
    assert!(events.contains(&GameplayEventKind::ExplosiveDeployed {
        player_id: PLAYER,
        kind: ExplosiveKind::Sticky
    }));
    assert_eq!(
        world
            .player(PLAYER)
            .unwrap()
            .explosive_count(ExplosiveKind::Sticky),
        0
    );
    let events = advance(&mut world, 30);
    let bomb = world.bombs[0];
    assert!(bomb.stuck, "stuck to the wall");
    assert_eq!(bomb.direction, BlastDirection::Right);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::ChargeStuck { .. }))
    );
    let resting = (bomb.x, bomb.y);
    advance(&mut world, 30);
    assert_eq!(
        (world.bombs[0].x, world.bombs[0].y),
        resting,
        "no gravity once stuck"
    );
    let events = advance(&mut world, 120);
    assert!(world.bombs.is_empty());
    assert_eq!(detonations(&events, ExplosiveKind::Sticky), 1);
    assert_eq!(
        kind_at(&world, 16, 20),
        TileKind::Air,
        "the wall it clung to is gone"
    );
}

#[test]
fn c4_waits_for_the_remote_and_every_charge_goes_off_at_once() {
    let mut world = arena();
    world.bombs = vec![
        charge(0, ExplosiveKind::C4, 12.5, 31.0, REMOTE_FUSE),
        charge(1, ExplosiveKind::C4, 20.5, 31.0, REMOTE_FUSE),
        charge(2, ExplosiveKind::C4, 28.5, 31.0, REMOTE_FUSE),
    ];
    world.next_bomb_id = 3;
    let events = advance(&mut world, 600);
    assert_eq!(world.bombs.len(), 3, "C4 never times out");
    assert_eq!(detonations(&events, ExplosiveKind::C4), 0);
    assert!(world.bombs.iter().all(PlacedBomb::awaiting_remote));
    assert_eq!(world.waiting_remote_charges(PLAYER), 3);
    let events = act(
        &mut world,
        PLAYER,
        PlayerCommand::RemoteDetonate { sequenced: false },
    );
    assert!(events.contains(&GameplayEventKind::RemoteDetonation { player_id: PLAYER }));
    assert_eq!(
        detonations(&events, ExplosiveKind::C4),
        3,
        "all in the same tick"
    );
    assert!(world.bombs.is_empty());
    assert_eq!(world.waiting_remote_charges(PLAYER), 0);
    // With nothing left to trigger, the detonator does nothing.
    let events = act(
        &mut world,
        PLAYER,
        PlayerCommand::RemoteDetonate { sequenced: false },
    );
    assert!(!events.contains(&GameplayEventKind::RemoteDetonation { player_id: PLAYER }));
}

#[test]
fn sequenced_remote_fires_charges_one_after_another_in_placement_order() {
    let mut world = arena();
    // Far enough apart that no blast reaches the next charge.
    world.bombs = vec![
        charge(5, ExplosiveKind::C4, 30.5, 31.0, REMOTE_FUSE),
        charge(3, ExplosiveKind::C4, 10.5, 31.0, REMOTE_FUSE),
        charge(4, ExplosiveKind::C4, 20.5, 31.0, REMOTE_FUSE),
    ];
    world.next_bomb_id = 6;
    let mut order = Vec::new();
    let mut ticks = Vec::new();
    let mut events = act(
        &mut world,
        PLAYER,
        PlayerCommand::RemoteDetonate { sequenced: true },
    );
    for tick in 0..120 {
        for event in &events {
            if let GameplayEventKind::ExplosiveDetonated { bomb_id, .. } = event {
                order.push(*bomb_id);
                ticks.push(tick);
            }
        }
        events = advance(&mut world, 1);
    }
    assert_eq!(order, vec![3, 4, 5], "placement (id) order");
    assert!(ticks[0] < ticks[1] && ticks[1] < ticks[2], "{ticks:?}");
    let gap = world
        .timing
        .ticks_for_millis(crate::explosive::SEQUENCE_STEP_MILLIS);
    assert_eq!(u64::try_from(ticks[1] - ticks[0]).unwrap(), gap);
}

#[test]
fn only_the_owner_can_trigger_c4_and_a_departing_owner_disarms_it() {
    let mut world = arena();
    world.add_player(OTHER);
    let mut theirs = charge(0, ExplosiveKind::C4, 12.5, 31.0, REMOTE_FUSE);
    theirs.owner = Some(OTHER);
    world.bombs = vec![
        theirs,
        charge(1, ExplosiveKind::C4, 28.5, 31.0, REMOTE_FUSE),
    ];
    world.next_bomb_id = 2;
    let events = act(
        &mut world,
        PLAYER,
        PlayerCommand::RemoteDetonate { sequenced: false },
    );
    assert_eq!(detonations(&events, ExplosiveKind::C4), 1);
    assert_eq!(world.bombs.len(), 1);
    assert_eq!(world.bombs[0].owner, Some(OTHER));
    world.remove_player(OTHER);
    assert!(world.bombs.is_empty(), "nobody can trigger it any more");
}

#[test]
fn a_blast_sets_off_nearby_c4_early() {
    let mut world = arena();
    world.bombs = vec![
        charge(0, ExplosiveKind::Bomb, 20.5, 31.0, SimulationTick::new(1)),
        charge(1, ExplosiveKind::C4, 21.5, 31.0, REMOTE_FUSE),
    ];
    world.bombs[1].stuck = true;
    let events = advance(&mut world, 60);
    assert!(world.bombs.is_empty());
    assert_eq!(detonations(&events, ExplosiveKind::C4), 1);
}

#[test]
fn a_cluster_bomb_splits_into_a_fan_of_bomblets_that_all_detonate() {
    let mut world = arena();
    world.bombs = vec![charge(
        0,
        ExplosiveKind::Cluster,
        20.5,
        20.5,
        SimulationTick::new(1),
    )];
    world.next_bomb_id = 1;
    let mut replay = world.clone();
    let events = advance(&mut world, 1);
    assert_eq!(detonations(&events, ExplosiveKind::Cluster), 1);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::ClusterSplit { bomb_id: 0, .. }))
    );
    assert_eq!(world.bombs.len(), CLUSTER_BOMBLETS);
    assert!(
        world
            .bombs
            .iter()
            .all(|bomb| bomb.kind == ExplosiveKind::Bomblet)
    );
    let ids: Vec<u64> = world.bombs.iter().map(|bomb| bomb.id).collect();
    assert_eq!(ids, (1..=6).collect::<Vec<_>>());
    // The fan spreads both ways and upwards.
    assert!(world.bombs.iter().any(|bomb| bomb.velocity_x < -100.0));
    assert!(world.bombs.iter().any(|bomb| bomb.velocity_x > 100.0));
    assert!(world.bombs.iter().all(|bomb| bomb.velocity_y < 0.0));
    let rest = advance(&mut world, 240);
    assert!(world.bombs.is_empty());
    assert_eq!(detonations(&rest, ExplosiveKind::Bomblet), CLUSTER_BOMBLETS);
    let mut all = events;
    all.extend(rest);
    assert_eq!(all, advance(&mut replay, 241), "deterministic");
    assert_eq!(
        world.simulation_state_hash(),
        replay.simulation_state_hash()
    );
}

#[test]
fn a_shaped_charge_cuts_a_long_narrow_tunnel_into_the_surface_it_stuck_to() {
    let mut world = arena();
    fill(&mut world, 8..=40, 16..=31, TileKind::Stone);
    world.bombs = vec![charge(0, ExplosiveKind::Shaped, 20.5, 15.0, far_future())];
    // Settles onto the stone floor and sticks facing down.
    advance(&mut world, 60);
    assert!(world.bombs[0].stuck);
    assert_eq!(world.bombs[0].direction, BlastDirection::Down);
    world.bombs[0].detonation_tick = world.simulation_tick().next();
    let events = advance(&mut world, 1);
    assert_eq!(detonations(&events, ExplosiveKind::Shaped), 1);
    // Ten tiles straight down, three wide; untouched beside the cut.
    for y in 16..=25 {
        for x in 19..=21 {
            assert_eq!(kind_at(&world, x, y), TileKind::Air, "({x},{y})");
        }
        assert_eq!(kind_at(&world, 18, y), TileKind::Stone);
        assert_eq!(kind_at(&world, 22, y), TileKind::Stone);
    }
    assert_eq!(
        kind_at(&world, 20, 27),
        TileKind::Stone,
        "not deeper than its length"
    );
}

#[test]
fn a_shaped_charge_on_a_wall_cuts_sideways_and_stops_at_bedrock() {
    let mut world = arena();
    fill(&mut world, 24..=40, 8..=31, TileKind::Stone);
    fill(&mut world, 30..=30, 8..=31, TileKind::Bedrock);
    park(&mut world, PLAYER, 21.5, 20.5);
    world
        .player_mut(PLAYER)
        .unwrap()
        .add_explosives(ExplosiveKind::Shaped, 1);
    act(
        &mut world,
        PLAYER,
        PlayerCommand::DeployExplosive {
            kind: ExplosiveKind::Shaped,
            aim: ThrowAim::RIGHT,
        },
    );
    advance(&mut world, 30);
    assert!(world.bombs[0].stuck);
    assert_eq!(world.bombs[0].direction, BlastDirection::Right);
    #[allow(clippy::cast_possible_truncation, reason = "a tile row")]
    let row = (world.bombs[0].y / TILE_SIZE).floor() as i32;
    advance(&mut world, 120);
    assert!(world.bombs.is_empty());
    for x in 24..=29 {
        assert_eq!(kind_at(&world, x, row), TileKind::Air, "x {x}");
    }
    assert_eq!(kind_at(&world, 30, row), TileKind::Bedrock);
    assert_eq!(
        kind_at(&world, 31, row),
        TileKind::Stone,
        "nothing behind the bedrock"
    );
    assert_eq!(kind_at(&world, 26, row - 3), TileKind::Stone, "narrow");
}

#[test]
fn a_mining_charge_clears_wide_and_pops_ore_out_intact() {
    let mut world = arena();
    fill(&mut world, 8..=40, 21..=31, TileKind::Stone);
    let _ = world.terrain_mut().set_kind(
        TilePosition { x: 22, y: 22 },
        TileKind::Ore(MineralKind::Gold),
    );
    world.bombs = vec![charge(
        0,
        ExplosiveKind::Mining,
        20.5,
        20.5,
        SimulationTick::new(1),
    )];
    advance(&mut world, 1);
    // WP-DROPS: the ore breaks cleanly and drops as loot (a plain blast shatters some).
    assert_eq!(kind_at(&world, 22, 22), TileKind::Air);
    assert!(world.drops().any(|(_, _, drop, _)| drop.stack
        == crate::item::ItemStack::new(crate::item::ItemKind::Mineral(MineralKind::Gold), 1)));
    assert_eq!(kind_at(&world, 20, 24), TileKind::Air, "radius four");
    assert_eq!(kind_at(&world, 21, 22), TileKind::Air);
}

#[test]
fn an_incendiary_burns_soft_ground_in_spreading_rings_but_not_stone() {
    let mut world = arena();
    fill(&mut world, 8..=40, 8..=31, TileKind::Dirt);
    fill(&mut world, 23..=23, 8..=31, TileKind::Stone);
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: 20, y: 20 }, TileKind::Air);
    // Resting on a bedrock nub, so the fire's centre stays put.
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: 20, y: 21 }, TileKind::Bedrock);
    world.bombs = vec![charge(
        0,
        ExplosiveKind::Incendiary,
        20.5,
        20.5,
        SimulationTick::new(1),
    )];
    world.bombs[0].y = 21.0f32.mul_add(TILE_SIZE, -super::bombs::BOMB_RADIUS - 1.0);
    let events = advance(&mut world, 1);
    assert_eq!(detonations(&events, ExplosiveKind::Incendiary), 1);
    assert_eq!(world.bombs.len(), 1, "still burning");
    assert_eq!(
        kind_at(&world, 20, 23),
        TileKind::Dirt,
        "fire has not reached ring three yet"
    );
    let events = advance(&mut world, 200);
    assert!(world.bombs.is_empty(), "burnt out");
    let fires = events
        .iter()
        .filter(|event| matches!(event, GameplayEventKind::FireSpread { .. }))
        .count();
    assert_eq!(fires, usize::from(crate::explosive::INCENDIARY_RINGS));
    assert_eq!(kind_at(&world, 20, 23), TileKind::Air);
    assert_eq!(kind_at(&world, 17, 20), TileKind::Air);
    assert_eq!(kind_at(&world, 23, 20), TileKind::Stone);
    assert_eq!(kind_at(&world, 20, 24), TileKind::Dirt, "only three rings");
}

#[test]
fn a_flare_lights_up_charts_the_area_for_its_owner_and_burns_out_without_a_blast() {
    let mut world = arena();
    fill(&mut world, 8..=40, 21..=31, TileKind::Stone);
    world.bombs = vec![charge(
        0,
        ExplosiveKind::Flare,
        30.5,
        20.5,
        SimulationTick::new(1),
    )];
    let before = world.discovered_tile_count(PLAYER);
    let events = advance(&mut world, 1);
    assert_eq!(detonations(&events, ExplosiveKind::Flare), 1);
    assert!(world.bombs[0].burning());
    assert!(world.discovered_tile_count(PLAYER) > before);
    assert_eq!(kind_at(&world, 30, 21), TileKind::Stone, "no blast");
    let hz = u64::from(world.tick_rate_hz());
    advance(&mut world, hz * 45);
    assert!(world.bombs.is_empty(), "burnt out");
}

#[test]
fn an_emp_pulses_without_touching_terrain() {
    let mut world = arena();
    fill(&mut world, 8..=40, 21..=31, TileKind::Stone);
    world.bombs = vec![charge(
        0,
        ExplosiveKind::Emp,
        20.5,
        20.5,
        SimulationTick::new(1),
    )];
    let events = advance(&mut world, 1);
    assert_eq!(detonations(&events, ExplosiveKind::Emp), 1);
    assert!(world.bombs.is_empty());
    assert_eq!(kind_at(&world, 20, 21), TileKind::Stone);
}

#[test]
fn blasts_push_rigs_away_but_not_through_rock() {
    let mut world = arena();
    park(&mut world, PLAYER, 22.0, 20.5);
    world.bombs = vec![charge(
        0,
        ExplosiveKind::Bomb,
        20.5,
        20.5,
        SimulationTick::new(1),
    )];
    advance(&mut world, 1);
    let player = world.player(PLAYER).unwrap();
    assert!(
        player.velocity_x > 50.0,
        "pushed right: {}",
        player.velocity_x
    );
    assert!(player.velocity_y < 0.0, "and lifted");
    assert!(player.velocity_y >= -260.0, "never enough to crash-land");
    assert!(
        (player.hull - player.max_hull()).abs() < f32::EPSILON,
        "no friendly fire"
    );

    let mut shielded = arena();
    fill(&mut shielded, 21..=21, 8..=31, TileKind::Stone);
    park(&mut shielded, PLAYER, 22.5, 20.5);
    shielded.bombs = vec![charge(
        0,
        ExplosiveKind::Bomb,
        20.5,
        20.5,
        SimulationTick::new(1),
    )];
    advance(&mut shielded, 1);
    assert!(shielded.player(PLAYER).unwrap().velocity_x.abs() < f32::EPSILON);
}

#[test]
fn being_blown_into_a_wall_or_the_floor_never_damages_the_rig() {
    // Full ticks (movement and collision damage included), the rig holding no controls.
    let run = |world: &mut WorldState, ticks: u64| {
        for _ in 0..ticks {
            let tick = world.simulation_tick();
            let _ = world.simulate_tick(
                tick,
                &TickInputs::held(PLAYER, crate::ControlState::default()),
            );
        }
    };
    // Point-blank C4 beside a rig parked against a bedrock wall.
    let mut wall = arena();
    fill(&mut wall, 25..=25, 8..=31, TileKind::Bedrock);
    park(&mut wall, PLAYER, 24.5, 31.5);
    let next = wall.simulation_tick().next();
    wall.bombs = vec![charge(0, ExplosiveKind::C4, 23.9, 31.5, next)];
    let hull = wall.player(PLAYER).unwrap().hull;
    run(&mut wall, 1);
    let pushed = wall.player(PLAYER).unwrap().velocity_x;
    assert!(pushed > 0.0, "pushed into the wall: {pushed}");
    run(&mut wall, 30);
    assert!(
        (wall.player(PLAYER).unwrap().hull - hull).abs() < f32::EPSILON,
        "wall impact from a blast"
    );

    // A rig already falling fast, with C4 going off just above it.
    let mut fall = arena();
    park(&mut fall, PLAYER, 20.5, 30.5);
    fall.player_mut(PLAYER).unwrap().velocity_y = 280.0;
    let next = fall.simulation_tick().next();
    fall.bombs = vec![charge(0, ExplosiveKind::C4, 20.5, 30.2, next)];
    let hull = fall.player(PLAYER).unwrap().hull;
    run(&mut fall, 1);
    assert!(fall.player(PLAYER).unwrap().velocity_y <= 300.0 + 1.0);
    run(&mut fall, 120);
    assert!(
        (fall.player(PLAYER).unwrap().hull - hull).abs() < f32::EPSILON,
        "crash landing from a blast"
    );
}

#[test]
fn a_stuck_charge_falls_once_the_surface_holding_it_is_blasted() {
    let mut world = arena();
    fill(&mut world, 16..=16, 8..=31, TileKind::Stone);
    let mut sticky = charge(0, ExplosiveKind::C4, 15.5, 20.5, REMOTE_FUSE);
    sticky.x = 16.0f32.mul_add(TILE_SIZE, -super::bombs::BOMB_RADIUS - 0.5);
    sticky.stuck = true;
    sticky.direction = BlastDirection::Right;
    world.bombs = vec![sticky];
    advance(&mut world, 30);
    let y = world.bombs[0].y;
    assert!(world.bombs[0].stuck);
    let _ = world.detonate_bomb_at(TilePosition { x: 16, y: 20 }, 1);
    advance(&mut world, 30);
    assert!(!world.bombs[0].stuck);
    assert!(world.bombs[0].y > y + TILE_SIZE);
}

#[test]
fn deploying_needs_a_carried_charge_and_bomblets_cannot_be_deployed() {
    let mut world = arena();
    park(&mut world, PLAYER, 20.5, 20.5);
    for kind in ExplosiveKind::ALL {
        let events = act(
            &mut world,
            PLAYER,
            PlayerCommand::DeployExplosive {
                kind,
                aim: ThrowAim::DOWN,
            },
        );
        assert!(
            events.is_empty()
                || !events.iter().any(|event| matches!(
                    event,
                    GameplayEventKind::ExplosiveDeployed { .. }
                        | GameplayEventKind::BombPlaced { .. }
                )),
            "{kind:?}"
        );
    }
    assert!(world.bombs.is_empty());
    world
        .player_mut(PLAYER)
        .unwrap()
        .add_explosives(ExplosiveKind::Bomblet, 3);
    act(
        &mut world,
        PLAYER,
        PlayerCommand::DeployExplosive {
            kind: ExplosiveKind::Bomblet,
            aim: ThrowAim::DOWN,
        },
    );
    assert!(world.bombs.is_empty());
    for kind in ExplosiveKind::SELECTABLE {
        world.player_mut(PLAYER).unwrap().add_explosives(kind, 1);
        act(
            &mut world,
            PLAYER,
            PlayerCommand::DeployExplosive {
                kind,
                aim: ThrowAim(14),
            },
        );
        assert_eq!(
            world.player(PLAYER).unwrap().explosive_count(kind),
            0,
            "{kind:?}"
        );
        let latest = world.bombs.iter().max_by_key(|bomb| bomb.id).unwrap();
        assert_eq!(latest.kind, kind);
        assert_eq!(latest.owner, Some(PLAYER));
        assert_eq!(latest.awaiting_remote(), kind == ExplosiveKind::C4);
    }
}

#[test]
fn the_shack_sells_each_pack_at_its_listed_price_and_level() {
    let mut world = arena();
    world.player_mut(PLAYER).unwrap().credits = 1_000;
    let buy = |world: &mut WorldState, kind| {
        world.apply_service_request(PLAYER, SessionServiceRequest::BuyExplosives { kind })
    };
    buy(&mut world, ExplosiveKind::Sticky);
    let offer = explosive_offer(ExplosiveKind::Sticky).unwrap();
    let player = world.player(PLAYER).unwrap();
    assert_eq!(player.explosive_count(ExplosiveKind::Sticky), offer.count);
    assert_eq!(player.credits, 1_000 - offer.cost);
    // Shaped charges need a better stocked shack; bombs are not sold this way.
    buy(&mut world, ExplosiveKind::Shaped);
    buy(&mut world, ExplosiveKind::Bomb);
    buy(&mut world, ExplosiveKind::Bomblet);
    let player = world.player(PLAYER).unwrap();
    assert_eq!(player.explosive_count(ExplosiveKind::Shaped), 0);
    assert_eq!(player.credits, 1_000 - offer.cost);
}

#[test]
fn mixed_charges_survive_save_and_replication_mid_flight() {
    let mut original = arena();
    fill(&mut original, 8..=40, 26..=31, TileKind::Stone);
    original.bombs = vec![
        charge(0, ExplosiveKind::C4, 12.5, 25.0, REMOTE_FUSE),
        charge(
            1,
            ExplosiveKind::Cluster,
            20.5,
            15.5,
            SimulationTick::new(40),
        ),
        charge(
            2,
            ExplosiveKind::Incendiary,
            30.5,
            20.5,
            SimulationTick::new(10),
        ),
        charge(3, ExplosiveKind::Flare, 35.5, 12.5, SimulationTick::new(5)),
        charge(
            4,
            ExplosiveKind::Shaped,
            16.5,
            20.5,
            SimulationTick::new(90),
        ),
    ];
    original.bombs[1].velocity_x = 140.0;
    original.next_bomb_id = 5;
    advance(&mut original, 45);
    assert!(
        original
            .bombs
            .iter()
            .any(|bomb| bomb.kind == ExplosiveKind::Bomblet)
    );
    assert!(original.bombs.iter().any(PlacedBomb::burning));
    let json = serde_json::to_string(&original).expect("save");
    let mut restored: WorldState = serde_json::from_str(&json).expect("load");
    let wire = postcard::to_allocvec(&original.live_entities()).expect("encode");
    let mut replica = restored.clone();
    replica.bombs.clear();
    replica.next_bomb_id = 0;
    replica.apply_live_entities(postcard::from_bytes(&wire).expect("decode"));
    assert_eq!(
        original.simulation_state_hash(),
        restored.simulation_state_hash()
    );
    assert_eq!(
        original.simulation_state_hash(),
        replica.simulation_state_hash()
    );
    for world in [&mut original, &mut restored, &mut replica] {
        advance(world, 60);
        let _ = world.remote_detonate(PLAYER, true);
        advance(world, 120);
    }
    assert_eq!(
        original.simulation_state_hash(),
        restored.simulation_state_hash()
    );
    assert_eq!(
        original.simulation_state_hash(),
        replica.simulation_state_hash()
    );
}

#[test]
fn old_bombs_without_explosive_fields_load_as_plain_unowned_bombs() {
    let json = r#"{"id":7,"x":10.0,"y":12.0,"velocity_x":0.0,"velocity_y":0.0,"previous_x":10.0,"previous_y":12.0,"angle":0.0,"detonation_tick":30,"chain_triggered":false,"timer_seconds":0.5}"#;
    let bomb: PlacedBomb = serde_json::from_str(json).expect("old bomb");
    assert_eq!(bomb.kind, ExplosiveKind::Bomb);
    assert_eq!(bomb.owner, None);
    assert!(!bomb.stuck && !bomb.burning());
    let player: crate::Player = serde_json::from_value({
        let mut value = serde_json::to_value(crate::Player::new(0.0, 0.0)).unwrap();
        value.as_object_mut().unwrap().remove("explosives");
        value
    })
    .expect("old player");
    assert!(player.explosives.is_empty());
}

#[test]
fn mixed_simultaneous_blasts_are_independent_of_storage_order() {
    let mut first = arena();
    fill(&mut first, 8..=40, 24..=31, TileKind::Dirt);
    first.bombs = vec![
        charge(
            0,
            ExplosiveKind::Cluster,
            20.5,
            22.5,
            SimulationTick::new(1),
        ),
        charge(1, ExplosiveKind::Sticky, 21.5, 22.5, SimulationTick::new(1)),
        charge(2, ExplosiveKind::C4, 22.5, 23.0, REMOTE_FUSE),
        charge(
            3,
            ExplosiveKind::Incendiary,
            18.5,
            22.5,
            SimulationTick::new(30),
        ),
    ];
    first.next_bomb_id = 4;
    park(&mut first, PLAYER, 23.5, 21.5);
    let mut second = first.clone();
    second.bombs.reverse();
    assert_eq!(advance(&mut first, 240), advance(&mut second, 240));
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
}

#[test]
fn the_preview_follows_the_real_throw() {
    let mut world = arena();
    fill(&mut world, 8..=40, 25..=31, TileKind::Stone);
    park(&mut world, PLAYER, 14.5, 22.5);
    let path = world.preview_explosive_path(PLAYER, ExplosiveKind::Sticky, ThrowAim(15));
    assert!(path.len() > 3);
    world
        .player_mut(PLAYER)
        .unwrap()
        .add_explosives(ExplosiveKind::Sticky, 1);
    act(
        &mut world,
        PLAYER,
        PlayerCommand::DeployExplosive {
            kind: ExplosiveKind::Sticky,
            aim: ThrowAim(15),
        },
    );
    advance(&mut world, u64::try_from(path.len()).unwrap());
    let (end_x, end_y) = *path.last().unwrap();
    let bomb = world.bombs[0];
    assert!(bomb.stuck);
    assert!((bomb.x - end_x).abs() < 0.01 && (bomb.y - end_y).abs() < 0.01);
}

#[test]
fn every_pushing_explosive_throws_loose_drops_but_emp_and_flares_do_not() {
    use crate::item::{ItemKind, ItemStack};
    let copper = ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 1);
    for kind in ExplosiveKind::ALL {
        let mut world = arena();
        let drop =
            world.spawn_item_drop(22.0 * TILE_SIZE, 20.5 * TILE_SIZE, (0.0, 0.0), copper, None);
        world.bombs = vec![charge(0, kind, 20.5, 20.5, SimulationTick::new(1))];
        advance(&mut world, 1);
        let body = world.entities().get(drop).expect("drop").body;
        if kind.push_reach() > 0.0 {
            assert!(
                body.velocity_x > 50.0 && body.velocity_y < 0.0 && !body.asleep,
                "{kind:?} throws the drop: {body:?}"
            );
        } else {
            assert!(
                body.velocity_x.abs() < f32::EPSILON,
                "{kind:?} pushes nothing: {body:?}"
            );
        }
    }
}
