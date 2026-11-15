//! WP-DROPS: drops from drilling and blasts, magnet pickup, fairness, full bays, merging,
//! despawn, caps, the collect toggle, dumping and selling blocks, determinism and replication.

use super::{
    WorldImport,
    drops::{
        BLOCK_DROP_LIFETIME_MILLIS, DROP_COLLECT_RADIUS, DROPS_PER_CELL_CAP, MAX_DROPS_PER_BLAST,
        block_hopper_capacity_for_level, magnet_radius, ore_survives_blast,
    },
};
use crate::{
    ControlState, ExplosiveKind, GameplayEventKind, PlacedBomb, PlayerCommand, PlayerId,
    SessionServiceRequest, SimulationTick, TickInputs, TickOutput, WorldState,
    entity::{EntityKind, EntityState},
    item::{BlockKind, ItemKind, ItemStack, Stowage},
    terrain::{ArtifactKind, MineralKind, TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);

/// An open air box (tiles 8..=40 x 8..=32) over a bedrock floor at row 32, the rig parked at
/// the far top right.
fn arena() -> WorldState {
    let mut world = WorldState::from_import(&WorldImport::new_game(PLAYER));
    fill(&mut world, 8..=40, 8..=31, TileKind::Air);
    fill(&mut world, 8..=40, 32..=32, TileKind::Bedrock);
    park(&mut world, PLAYER, 39.5, 9.5);
    world
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

fn park(world: &mut WorldState, player_id: PlayerId, tile_x: f32, tile_y: f32) {
    let player = world.player_mut(player_id).expect("player");
    player.x = tile_x * TILE_SIZE;
    player.y = tile_y * TILE_SIZE;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

fn tick(world: &mut WorldState) -> TickOutput {
    tick_with(world, &TickInputs::default())
}

fn tick_with(world: &mut WorldState, inputs: &TickInputs) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, inputs)
}

fn run(world: &mut WorldState, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(tick(world).events.into_iter().map(|event| event.kind));
    }
    events
}

fn px(tile: f32) -> f32 {
    tile * TILE_SIZE
}

fn stack(kind: impl Into<ItemKind>, count: u32) -> ItemStack {
    ItemStack::new(kind.into(), count)
}

fn drop_count(world: &WorldState) -> usize {
    world.entities().count_of(EntityKind::Drop)
}

fn total_dropped(world: &WorldState, kind: ItemKind) -> u32 {
    world
        .drops()
        .filter(|(_, _, drop, _)| drop.stack.kind == kind)
        .map(|(_, _, drop, _)| drop.stack.count)
        .sum()
}

/// A rig resting on a dirt floor at row 20 (columns 18..=22), drilling down into `below`.
fn drill_rig(world: &mut WorldState, below: TileKind) -> TilePosition {
    fill(world, 16..=24, 20..=24, TileKind::Dirt);
    let target = TilePosition { x: 20, y: 20 };
    let _ = world.terrain_mut().set_kind(target, below);
    let player = world.player_mut(PLAYER).expect("player");
    player.drill_strength = 8;
    player.fuel = player.fuel_capacity;
    player.x = px(20.5);
    player.y = px(20.0) - 11.0;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    target
}

fn drill_until_broken(world: &mut WorldState, target: TilePosition) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::held(PLAYER, ControlState::new(0.0, false, true));
    let mut events = Vec::new();
    for _ in 0..600 {
        events.extend(tick_with(world, &inputs).events.into_iter().map(|e| e.kind));
        if world.terrain().tile(target).map(|tile| tile.kind) == Some(TileKind::Air) {
            return events;
        }
    }
    panic!("the drill never broke {target:?}");
}

#[test]
fn drilled_ore_reaches_the_bay_on_the_tick_it_breaks() {
    let mut world = arena();
    let target = drill_rig(&mut world, TileKind::Ore(MineralKind::Gold));
    let events = drill_until_broken(&mut world, target);
    let player = world.player(PLAYER).expect("player");
    assert_eq!(player.cargo_bay.mineral(MineralKind::Gold), 1);
    assert_eq!(drop_count(&world), 0, "collected, not floating");
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::ItemsCollected {
            player_id: PLAYER,
            item: ItemKind::Mineral(MineralKind::Gold),
            count: 1,
            ..
        }
    )));
}

#[test]
fn drilled_dirt_fills_the_block_hopper_not_the_bay() {
    let mut world = arena();
    let target = drill_rig(&mut world, TileKind::Clay);
    let _ = drill_until_broken(&mut world, target);
    let player = world.player(PLAYER).expect("player");
    assert_eq!(player.block_hopper.count(BlockKind::Clay), 1);
    assert_eq!(player.cargo_used(), 0);
}

#[test]
fn a_full_bay_leaves_ore_floating_warns_once_and_collects_after_selling() {
    let mut world = arena();
    let target = drill_rig(&mut world, TileKind::Ore(MineralKind::Ruby));
    {
        let player = world.player_mut(PLAYER).expect("player");
        let capacity = player.cargo_capacity;
        player.cargo_bay.add(MineralKind::Copper, capacity);
    }
    let mut events = drill_until_broken(&mut world, target);
    events.extend(run(&mut world, 120));
    assert_eq!(
        total_dropped(&world, ItemKind::Mineral(MineralKind::Ruby)),
        1,
        "the ruby floats, it is not lost"
    );
    let warnings = events
        .iter()
        .filter(|event| {
            **event
                == GameplayEventKind::PickupBlocked {
                    player_id: PLAYER,
                    stowage: Stowage::CargoBay,
                }
        })
        .count();
    assert_eq!(warnings, 1, "one warning, not one per tick");
    world
        .player_mut(PLAYER)
        .expect("player")
        .cargo_bay
        .clear_ore_and_artifacts();
    let _ = run(&mut world, 30);
    assert_eq!(
        world
            .player(PLAYER)
            .expect("player")
            .cargo_bay
            .mineral(MineralKind::Ruby),
        1
    );
    assert_eq!(drop_count(&world), 0);
}

#[test]
fn the_magnet_pulls_visible_drops_in_and_ignores_drops_behind_rock() {
    let mut world = arena();
    park(&mut world, PLAYER, 20.5, 30.5);
    let reach = magnet_radius(world.player(PLAYER).expect("player"));
    assert!(reach > DROP_COLLECT_RADIUS * 1.5);
    // In the open, two and a half tiles away: pulled in within half a second.
    let open = world.spawn_item_drop(
        px(23.0),
        px(30.5),
        (0.0, 0.0),
        stack(MineralKind::Iron, 2),
        None,
    );
    // Same distance, but walled off.
    fill(&mut world, 16..=16, 26..=31, TileKind::Stone);
    let walled = world.spawn_item_drop(
        px(18.0) - 20.0,
        px(31.0) - 6.0,
        (0.0, 0.0),
        stack(MineralKind::Silver, 1),
        None,
    );
    let mut pulled = false;
    for _ in 0..30 {
        let _ = tick(&mut world);
        pulled |= world.entities().get(open).is_some_and(|entity| {
            matches!(entity.state, EntityState::Drop(drop) if drop.pulled_by == Some(PLAYER))
        });
    }
    assert!(pulled, "the magnet pulled it");
    assert!(world.entities().get(open).is_none(), "collected");
    let player = world.player(PLAYER).expect("player");
    assert_eq!(player.cargo_bay.mineral(MineralKind::Iron), 2);
    assert!(
        world.entities().get(walled).is_some(),
        "rock blocks the magnet"
    );
}

#[test]
fn a_pull_lets_go_when_the_rig_leaves_magnet_reach() {
    let mut world = arena();
    park(&mut world, PLAYER, 20.5, 30.5);
    let id = world.spawn_item_drop(
        px(23.0),
        px(30.5),
        (0.0, 0.0),
        stack(MineralKind::Gold, 1),
        None,
    );
    let _ = tick(&mut world);
    let pulled = |world: &WorldState| {
        world.entities().get(id).is_some_and(|entity| {
            matches!(entity.state, EntityState::Drop(drop) if drop.pulled_by == Some(PLAYER))
        })
    };
    assert!(pulled(&world));
    // Rescued to the far side of the arena: the drop drops.
    park(&mut world, PLAYER, 38.5, 9.5);
    let _ = run(&mut world, 60);
    assert!(!pulled(&world));
    let entity = world.entities().get(id).expect("still there");
    assert!(
        entity.body.x < px(30.0),
        "it did not fly through the arena after the rig"
    );
}

#[test]
fn the_nearest_rig_with_room_gets_the_drop() {
    let mut world = arena();
    world.add_player(OTHER);
    park(&mut world, PLAYER, 19.5, 30.5);
    park(&mut world, OTHER, 22.5, 30.5);
    let id = world.spawn_item_drop(
        px(21.5),
        px(30.5),
        (0.0, 0.0),
        stack(MineralKind::Gold, 1),
        None,
    );
    let _ = run(&mut world, 30);
    assert!(world.entities().get(id).is_none());
    assert_eq!(
        world
            .player(OTHER)
            .expect("other")
            .cargo_bay
            .mineral(MineralKind::Gold),
        1,
        "the nearer rig"
    );
    // With the nearer rig full, the farther one gets it instead.
    {
        let other = world.player_mut(OTHER).expect("other");
        let capacity = other.cargo_capacity;
        other.cargo_bay.add(MineralKind::Copper, capacity);
    }
    let _ = world.spawn_item_drop(
        px(21.5),
        px(30.5),
        (0.0, 0.0),
        stack(MineralKind::Gold, 1),
        None,
    );
    let _ = run(&mut world, 60);
    assert_eq!(
        world
            .player(PLAYER)
            .expect("player")
            .cargo_bay
            .mineral(MineralKind::Gold),
        1
    );
}

#[test]
fn resting_drops_merge_into_one_stack_and_blocks_despawn_before_ore() {
    let mut world = arena();
    for offset in 0..5_u8 {
        let x = f32::from(offset).mul_add(4.0, px(12.0));
        let _ = world.spawn_item_drop(
            x,
            px(29.0),
            (0.0, 0.0),
            stack(BlockKind::Stone, 3),
            Some(BLOCK_DROP_LIFETIME_MILLIS),
        );
    }
    let ore = world.spawn_item_drop(
        px(30.0),
        px(29.0),
        (0.0, 0.0),
        stack(MineralKind::Copper, 1),
        super::drops::drop_lifetime_millis(ItemKind::Mineral(MineralKind::Copper)),
    );
    let _ = run(&mut world, 60);
    let stones: Vec<u32> = world
        .drops()
        .filter(|(_, _, drop, _)| drop.stack.kind == ItemKind::Block(BlockKind::Stone))
        .map(|(_, _, drop, _)| drop.stack.count)
        .collect();
    assert_eq!(stones, vec![15], "five stacks became one");
    let ticks =
        u32::try_from(world.timing().ticks_for_millis(BLOCK_DROP_LIFETIME_MILLIS)).expect("ticks");
    let _ = run(&mut world, ticks);
    assert_eq!(total_dropped(&world, ItemKind::Block(BlockKind::Stone)), 0);
    assert!(world.entities().get(ore).is_some(), "ore lasts far longer");
}

#[test]
fn a_crowded_cell_folds_new_drops_into_stacks() {
    let mut world = arena();
    let spawned = u32::try_from(DROPS_PER_CELL_CAP).expect("small cap") + 30;
    for index in 0..spawned {
        #[allow(clippy::cast_precision_loss, reason = "small test index")]
        let x = ((index % 14) as f32).mul_add(30.0, px(17.0));
        let kind = if index % 2 == 0 {
            ItemKind::Block(BlockKind::Dirt)
        } else {
            ItemKind::Mineral(MineralKind::Copper)
        };
        let _ = world.spawn_loot(x, px(20.0), (0.0, 0.0), stack(kind, 1), None);
    }
    let cell_drops = world
        .drops()
        .filter(|(_, body, _, _)| body.x < px(32.0))
        .count();
    assert!(cell_drops <= DROPS_PER_CELL_CAP, "{cell_drops}");
    assert_eq!(
        total_dropped(&world, ItemKind::Mineral(MineralKind::Copper)),
        spawned / 2,
        "ore is merged, never deleted"
    );
}

/// A ring of ore and dirt around a charge at tile (20, 20), the rig parked far away.
fn loaded_blast(kind: ExplosiveKind) -> WorldState {
    let mut world = arena();
    fill(&mut world, 14..=26, 14..=26, TileKind::Dirt);
    for y in 16..=24 {
        for x in 16..=24 {
            if (x + y) % 2 == 0 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Ore(MineralKind::Gold));
            }
        }
    }
    let _ = world.terrain_mut().set_kind(
        TilePosition { x: 20, y: 21 },
        TileKind::Artifact(ArtifactKind::Fossil),
    );
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: 20, y: 20 }, TileKind::Air);
    let (x, y) = (px(20.5), px(20.5));
    world.insert_charge(PlacedBomb {
        id: 1,
        x,
        y,
        previous_x: x,
        previous_y: y,
        detonation_tick: SimulationTick::new(1),
        kind,
        owner: Some(PLAYER),
        ..PlacedBomb::default()
    });
    world
}

fn gold_tiles_in_shape(world: &WorldState, kind: ExplosiveKind) -> (u32, u32) {
    let seed = world.terrain().seed();
    let radius = match kind.blast() {
        crate::explosive::BlastShape::Diamond(r) | crate::explosive::BlastShape::LootSafe(r) => r,
        _ => 0,
    };
    let (mut total, mut intact) = (0, 0);
    for y in 20 - radius..=20 + radius {
        for x in 20 - radius..=20 + radius {
            if (x - 20).abs() + (y - 20).abs() > radius || (x + y) % 2 != 0 || (x, y) == (20, 20) {
                continue;
            }
            if (x, y) == (20, 21) {
                continue;
            }
            total += 1;
            let item = ItemKind::Mineral(MineralKind::Gold);
            if ore_survives_blast(kind, item, seed, TilePosition { x, y }) {
                intact += 1;
            }
        }
    }
    (total, intact)
}

#[test]
fn bombs_spray_loot_and_mining_charges_keep_most_ore() {
    for kind in [
        ExplosiveKind::Bomb,
        ExplosiveKind::C4,
        ExplosiveKind::Mining,
    ] {
        let mut world = loaded_blast(kind);
        let output = tick(&mut world);
        assert!(
            output.events.iter().any(|event| matches!(
                event.kind,
                GameplayEventKind::ExplosiveDetonated { .. }
                    | GameplayEventKind::BombDetonated { .. }
            )),
            "{kind:?} went off"
        );
        let (total, intact) = gold_tiles_in_shape(&world, kind);
        let gold = total_dropped(&world, ItemKind::Mineral(MineralKind::Gold));
        assert_eq!(gold, intact, "{kind:?}: intact ore drops");
        assert_eq!(
            total_dropped(&world, ItemKind::Artifact(ArtifactKind::Fossil)),
            1,
            "{kind:?}: artifacts always survive"
        );
        assert!(
            total_dropped(&world, ItemKind::Block(BlockKind::Dirt)) > 0,
            "{kind:?}: plain ground drops blocks"
        );
        if kind == ExplosiveKind::Mining {
            assert!(
                intact * 6 >= total * 4,
                "mining charges shatter little: {intact}/{total}"
            );
        } else {
            assert!(intact < total, "{kind:?}: {intact}/{total}");
        }
        assert!(drop_count(&world) <= MAX_DROPS_PER_BLAST + 1);
        // The spray flies outward and up.
        let moving = world
            .drops()
            .filter(|(_, body, _, _)| body.velocity_y < 0.0 || body.velocity_x != 0.0)
            .count();
        assert!(moving * 2 >= drop_count(&world), "{kind:?}: a spray");
        // Everything lands and stays collectible: hoover it up.
        let _ = run(&mut world, 120);
        park(&mut world, PLAYER, 20.5, 20.5);
        let _ = run(&mut world, 30);
        for (x, y) in [(17.5, 20.5), (23.5, 20.5), (20.5, 23.5), (20.5, 17.5)] {
            park(&mut world, PLAYER, x, y);
            let _ = run(&mut world, 30);
        }
        let player = world.player(PLAYER).expect("player");
        assert!(player.cargo_bay.mineral(MineralKind::Gold) > 0, "{kind:?}");
    }
}

#[test]
fn blasting_the_star_core_free_starts_the_escape() {
    let mut world = loaded_blast(ExplosiveKind::Mining);
    let _ = world.terrain_mut().set_kind(
        TilePosition { x: 21, y: 20 },
        TileKind::Artifact(ArtifactKind::StarCore),
    );
    assert!(world.progression().escape_deadline.is_none());
    let output = tick(&mut world);
    assert!(
        output
            .events
            .iter()
            .any(|event| event.kind == GameplayEventKind::EscapeStarted { player_id: PLAYER }),
        "freeing the core is no shortcut past the escape"
    );
    assert!(world.progression().escape_deadline.is_some());
    assert_eq!(
        total_dropped(&world, ItemKind::Artifact(ArtifactKind::StarCore)),
        1,
        "the core drops intact"
    );
}

#[test]
fn plain_blasts_shatter_about_a_third_of_their_ore() {
    let seed = arena().terrain().seed();
    let item = ItemKind::Mineral(MineralKind::Gold);
    let mut intact = 0;
    for y in 0..60 {
        for x in -30..30 {
            if ore_survives_blast(ExplosiveKind::Bomb, item, seed, TilePosition { x, y }) {
                intact += 1;
            }
        }
    }
    assert!(
        (2_200..=2_600).contains(&intact),
        "{intact} of 3600 survive"
    );
    let gentle = (0..60)
        .flat_map(|y| (-30..30).map(move |x| TilePosition { x, y }))
        .filter(|tile| ore_survives_blast(ExplosiveKind::Mining, item, seed, *tile))
        .count();
    assert!(
        (2_850..=3_150).contains(&gentle),
        "{gentle} of 3600 survive a mining charge"
    );
    assert!(ore_survives_blast(
        ExplosiveKind::Bomb,
        ItemKind::Artifact(ArtifactKind::Fossil),
        seed,
        TilePosition { x: 0, y: 0 }
    ));
}

#[test]
fn incendiary_fire_and_shattering_boulders_drop_blocks() {
    let mut world = loaded_blast(ExplosiveKind::Incendiary);
    let _ = run(&mut world, 240);
    assert!(total_dropped(&world, ItemKind::Block(BlockKind::Dirt)) > 0);

    let mut world = arena();
    fill(&mut world, 8..=40, 25..=25, TileKind::Stone);
    world.spawn_falling_boulder(px(20.5), px(18.0), 0, 10_000);
    let _ = run(&mut world, 120);
    assert_eq!(
        total_dropped(&world, ItemKind::Block(BlockKind::Stone)),
        1,
        "a boulder leaves rubble"
    );
}

#[test]
fn the_collect_toggle_dump_and_depot_sale_handle_blocks() {
    let mut world = arena();
    world.add_player(OTHER);
    park(&mut world, PLAYER, 20.5, 30.5);
    park(&mut world, OTHER, 34.5, 30.5);
    // Toggle off: blocks are left alone, ore still comes in.
    let output = tick_with(
        &mut world,
        &TickInputs::default().act(PLAYER, PlayerCommand::SetCollectBlocks { enabled: false }),
    );
    assert!(output.events.iter().any(|event| event.kind
        == GameplayEventKind::CollectBlocksToggled {
            player_id: PLAYER,
            enabled: false
        }));
    let _ = world.spawn_item_drop(
        px(21.0),
        px(30.5),
        (0.0, 0.0),
        stack(BlockKind::Dirt, 5),
        None,
    );
    let _ = world.spawn_item_drop(
        px(19.5),
        px(30.5),
        (0.0, 0.0),
        stack(MineralKind::Iron, 1),
        None,
    );
    let _ = run(&mut world, 30);
    let player = world.player(PLAYER).expect("player");
    assert_eq!(player.block_hopper.total(), 0);
    assert_eq!(player.cargo_bay.mineral(MineralKind::Iron), 1);
    // Toggle on: the blocks come in.
    let _ = tick_with(
        &mut world,
        &TickInputs::default().act(PLAYER, PlayerCommand::SetCollectBlocks { enabled: true }),
    );
    let _ = run(&mut world, 30);
    assert_eq!(
        world
            .player(PLAYER)
            .expect("player")
            .block_hopper
            .count(BlockKind::Dirt),
        5
    );
    // Dumped blocks are not sucked straight back in; another rig can take them.
    let output = tick_with(
        &mut world,
        &TickInputs::default().act(PLAYER, PlayerCommand::DumpBlocks { block: None }),
    );
    assert!(
        output
            .events
            .iter()
            .any(|event| event.kind == GameplayEventKind::BlocksDumped { player_id: PLAYER })
    );
    let _ = run(&mut world, 120);
    assert_eq!(
        world.player(PLAYER).expect("player").block_hopper.total(),
        0
    );
    assert_eq!(total_dropped(&world, ItemKind::Block(BlockKind::Dirt)), 5);
    park(&mut world, OTHER, 20.5, 30.5);
    let _ = run(&mut world, 30);
    assert_eq!(
        world
            .player(OTHER)
            .expect("other")
            .block_hopper
            .count(BlockKind::Dirt),
        5
    );
    // The depot buys blocks.
    let other = world.player_mut(OTHER).expect("other");
    other.block_hopper.add(BlockKind::HardRock, 10);
    let credits = other.credits;
    assert_eq!(
        world.apply_service_request(OTHER, SessionServiceRequest::SellBlocks),
        crate::PlayerScopedCommandOutcome::Applied
    );
    let other = world.player(OTHER).expect("other");
    assert_eq!(other.block_hopper.total(), 0);
    assert_eq!(other.credits, credits + 5 + 40);
}

#[test]
fn cargo_bay_upgrades_grow_the_hopper_and_the_magnet() {
    let mut world = arena();
    let before = magnet_radius(world.player(PLAYER).expect("player"));
    let player = world.player_mut(PLAYER).expect("player");
    player.credits = 1_000_000;
    let index = crate::economy::UpgradeKind::ALL
        .iter()
        .position(|kind| *kind == crate::economy::UpgradeKind::CargoBay)
        .expect("cargo bay line");
    let _ = crate::economy::buy_upgrade(player, index);
    assert_eq!(player.cargo_bay_level, 2);
    assert_eq!(
        player.block_hopper_capacity,
        block_hopper_capacity_for_level(2)
    );
    assert!(magnet_radius(player) > before);
}

/// A busy scene: a blast, two rigs hoovering, a full bay and a dump.
fn busy_scene() -> (WorldState, Vec<TickInputs>) {
    let mut world = loaded_blast(ExplosiveKind::Bomb);
    world.add_player(OTHER);
    park(&mut world, OTHER, 25.5, 27.5);
    world.player_mut(OTHER).expect("other").cargo_capacity = 2;
    let mut inputs = Vec::new();
    for step in 0..240_u32 {
        let mut input = TickInputs::held(PLAYER, ControlState::new(-0.6, step % 50 < 20, false));
        input = input.hold(OTHER, ControlState::new(-0.3, step % 40 < 10, false));
        if step == 200 {
            input = input.act(OTHER, PlayerCommand::DumpBlocks { block: None });
        }
        inputs.push(input);
    }
    park(&mut world, PLAYER, 26.5, 26.5);
    (world, inputs)
}

#[test]
fn drops_and_pickups_are_deterministic_and_replicate() {
    let (mut a, inputs) = busy_scene();
    let (mut b, _) = busy_scene();
    let mut replica = a.clone();
    let mut baseline = replica.entity_revisions(None, None);
    let mut events_a = Vec::new();
    for input in &inputs {
        let out_a = tick_with(&mut a, input);
        let out_b = tick_with(&mut b, input);
        assert_eq!(out_a.events, out_b.events);
        events_a.extend(out_a.events.into_iter().map(|event| event.kind));
        // Host -> client entity deltas (what netcode sends).
        let current = a.entity_revisions(None, None);
        let updates = current
            .iter()
            .filter(|pair| !baseline.contains(pair))
            .filter_map(|(id, _)| a.entity_record(*id))
            .collect();
        let despawned = baseline
            .iter()
            .filter(|(id, _)| !current.iter().any(|(other, _)| other == id))
            .map(|(id, _)| *id)
            .collect();
        replica.apply_entity_delta(crate::entity::EntityDelta { updates, despawned });
        baseline = current;
    }
    assert_eq!(a.simulation_state_hash(), b.simulation_state_hash());
    assert_eq!(replica.entities(), a.entities());
    assert!(
        events_a
            .iter()
            .any(|event| matches!(event, GameplayEventKind::ItemsCollected { .. })),
        "the scene collected loot"
    );
    // Saves keep drops (and their pull / dump marks).
    let json = serde_json::to_string(&a).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.entities(), a.entities());
}
