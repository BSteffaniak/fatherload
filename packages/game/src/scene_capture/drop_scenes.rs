//! Capture scenes for WP-DROPS: a bomb blasting an ore vein into a spray of loot, the loot
//! landed and stacked, the magnet hoovering a field of drops into the rig, a full cargo bay
//! turning drops away, and the inventory's block hopper section.

use fatherload_simulation::{
    ExplosiveKind, PlacedBomb, PlayerId, SimulationTick, WorldState,
    item::{BlockKind, ItemKind, ItemStack},
    terrain::{ArtifactKind, MineralKind, TILE_SIZE, TileKind, TilePosition},
};

use super::{SceneStage, carve, place_rig, reveal};
use crate::shell::ModalScreen;

#[allow(clippy::cast_precision_loss, reason = "small tile indices")]
fn tile(index: i32) -> f32 {
    index as f32 * TILE_SIZE
}

/// A cavern (tiles `98..122` x `20..29`) cut into stone, its floor at row 29, explored.
fn cavern(world: &mut WorldState, player: PlayerId) {
    for y in 16..40 {
        for x in 94..126 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Stone);
        }
    }
    carve(world, 98..122, 20..29);
    reveal(world, player, 92..128, 14..40);
}

/// A rich seam around tile `(cx, cy)`: gold and silver specks, a ruby pod, an idol, dirt and
/// clay pockets in the stone.
fn seam(world: &mut WorldState, cx: i32, cy: i32) {
    for y in cy - 3..=cy + 3 {
        for x in cx - 3..=cx + 3 {
            let kind = match (x - cx + 7 * (y - cy)).rem_euclid(9) {
                0 | 4 => TileKind::Ore(MineralKind::Gold),
                2 => TileKind::Ore(MineralKind::Silver),
                6 => TileKind::Ore(MineralKind::Ruby),
                3 | 7 => TileKind::Dirt,
                5 => TileKind::Clay,
                _ => TileKind::Stone,
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    let _ = world.terrain_mut().set_kind(
        TilePosition {
            x: cx + 1,
            y: cy + 1,
        },
        TileKind::Artifact(ArtifactKind::BuriedIdol),
    );
}

/// A charge of `kind` on the cavern floor against a rich seam wall, set to blow `fuse` ticks
/// after the scene starts; the rig waits out of magnet reach.
fn charge_at_seam(world: &mut WorldState, player: PlayerId, kind: ExplosiveKind, fuse: u64) {
    cavern(world, player);
    // The seam forms the cavern's right wall and the floor under it.
    seam(world, 115, 26);
    seam(world, 112, 31);
    // A pocket cut into the seam's foot holds the charge, so its blast is mostly rock.
    carve(world, 111..114, 28..29);
    let (x, y) = (TILE_SIZE.mul_add(0.5, tile(113)), tile(29) - 7.0);
    let mut entities = world.live_entities();
    entities.bombs.push(PlacedBomb {
        id: 0,
        x,
        y,
        previous_x: x,
        previous_y: y,
        timer_seconds: 1.0,
        detonation_tick: SimulationTick::new(world.simulation_tick().get() + fuse),
        owner: Some(player),
        kind,
        ..PlacedBomb::default()
    });
    entities.next_bomb_id = 1;
    world.apply_live_entities(entities);
    place_rig(world, player, 101, 28);
}

/// A charge going off in the seam: its ore, an idol and blocks fountain out as drops.
pub(super) fn bomb_spray(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        charge_at_seam(world, seats[0].player, ExplosiveKind::C4, 20);
    });
    stage.settle(37);
}

/// A mining charge's loot a moment later: every ore tile popped out intact, landed and merged
/// into stacks in the crater.
pub(super) fn bomb_loot(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        charge_at_seam(world, seats[0].player, ExplosiveKind::Mining, 20);
    });
    stage.settle(150);
}

/// A field of drops on the cavern floor: gold, gems, an artifact and blocks.
fn drop_field(world: &mut WorldState) {
    let loot = [
        (ItemKind::Mineral(MineralKind::Gold), 3),
        (ItemKind::Block(BlockKind::Dirt), 7),
        (ItemKind::Mineral(MineralKind::Emerald), 1),
        (ItemKind::Block(BlockKind::Stone), 12),
        (ItemKind::Mineral(MineralKind::Diamond), 2),
        (ItemKind::Artifact(ArtifactKind::Fossil), 1),
        (ItemKind::Block(BlockKind::Clay), 4),
        (ItemKind::Mineral(MineralKind::Copper), 5),
        (ItemKind::Mineral(MineralKind::Mythril), 1),
        (ItemKind::Block(BlockKind::HardRock), 2),
        (ItemKind::Mineral(MineralKind::Ruby), 1),
        (ItemKind::Mineral(MineralKind::Iron), 2),
    ];
    for (index, (kind, count)) in loot.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "a dozen drops")]
        let x = (index as f32 * TILE_SIZE).mul_add(1.15, tile(103));
        let _ = world.spawn_item_drop(
            x,
            tile(29) - 6.0,
            (0.0, 0.0),
            ItemStack::new(kind, count),
            None,
        );
    }
}

/// The rig sweeping low over a field of drops: the near ones fly in, the magnet tugs the next.
pub(super) fn magnet_pickup(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        cavern(world, player);
        drop_field(world);
        // Hovering low over the middle of the field the moment it arrives.
        place_rig(world, player, 108, 27);
        if let Some(rig) = world.player_mut(player) {
            // A bigger bay: a longer magnet.
            rig.cargo_bay_level = 6;
            rig.cargo_capacity = 80;
            rig.y = TILE_SIZE.mul_add(-0.4, rig.y);
        }
    });
    stage.settle(11);
}

/// A full cargo bay: the drops in reach stay put, marked, and the rig says so.
pub(super) fn bay_full(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        cavern(world, player);
        drop_field(world);
        place_rig(world, player, 106, 28);
        if let Some(rig) = world.player_mut(player) {
            let capacity = rig.cargo_capacity;
            rig.cargo_bay.insert(MineralKind::Copper, capacity);
        }
    });
    stage.settle(50);
}

/// The inventory's block hopper: fill, the collect toggle and dump rows.
pub(super) fn hopper_inventory(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        cavern(world, player);
        place_rig(world, player, 104, 28);
        if let Some(rig) = world.player_mut(player) {
            rig.block_hopper.insert(BlockKind::Dirt, 31);
            rig.block_hopper.insert(BlockKind::Stone, 18);
            rig.block_hopper.insert(BlockKind::Clay, 6);
            rig.cargo_bay.insert(MineralKind::Gold, 3);
            rig.cargo_bay.insert(MineralKind::Silver, 5);
        }
    });
    stage.settle(10);
    stage.seat(0).modal = Some(ModalScreen::Inventory);
    stage.seat(0).selected_menu_item = 1;
}
