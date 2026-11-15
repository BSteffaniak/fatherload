//! Capture scenes for the hardpoint weapons: each weapon firing at practice targets, the aim
//! reticles of a split-screen pair (free stick aim and an auto-aim lock), the shack's arms
//! rack and the explosives inventory (every gun on a rig: the `rig_sheet_weapons` scene).

use fatherload_simulation::{
    AmmoKind, ExplosiveKind, PlayerId, WeaponKind, WorldState,
    economy::SurfaceZone,
    item::{ItemKind, ItemStack},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    weapon::{MAX_BATTERY_LEVEL, MUZZLE_RISE},
};

use super::{ARROWS, SceneStage, WASD, XBOX, carve, place_rig, put_seat_inside, reveal};
use crate::{input::PlayerInput, shell::ModalScreen};

#[allow(clippy::cast_precision_loss, reason = "small tile indices")]
fn tile(index: i32) -> f32 {
    index as f32 * TILE_SIZE
}

/// A stone gallery (tiles `xs` by `ys` hollow) explored by `player`.
fn gallery(
    world: &mut WorldState,
    player: PlayerId,
    xs: std::ops::Range<i32>,
    ys: std::ops::Range<i32>,
) {
    for y in ys.start - 2..ys.end + 3 {
        for x in xs.start - 2..xs.end + 2 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Stone);
        }
    }
    carve(world, xs.clone(), ys.clone());
    reveal(
        world,
        player,
        xs.start - 6..xs.end + 6,
        ys.start - 4..ys.end + 6,
    );
}

/// Arms `player` with `weapon` at `mk` (mounted), a full big capacitor and rockets.
fn arm(world: &mut WorldState, player: PlayerId, weapon: WeaponKind, mk: u8) {
    if let Some(rig) = world.player_mut(player) {
        let _ = rig.receive_weapon(weapon);
        rig.weapon_rack.owned.insert(weapon, mk);
        let _ = rig.mount_weapon(Some(weapon));
        rig.weapon_rack.battery_level = MAX_BATTERY_LEVEL;
        rig.weapon.energy = rig.battery_capacity();
        rig.cargo_bay.add(AmmoKind::Rocket.item(), 12);
    }
}

/// A practice target standing on floor row `floor` (the tile below is solid) at column `x`,
/// its bullseye level with a rig's muzzle.
fn target(world: &mut WorldState, x: f32, floor: i32, health: f32) {
    let y = TILE_SIZE.mul_add(0.5, tile(floor)) - MUZZLE_RISE;
    let _ = world.spawn_target_dummy(x, y, health, 0.0);
}

/// Holding the trigger, aiming with a stick deflection `(x, y)`.
fn firing(aim_x: f32, aim_y: f32) -> PlayerInput {
    PlayerInput {
        fire: true,
        ..stick(aim_x, aim_y)
    }
}

/// Aiming with a stick deflection `(x, y)` without firing.
fn stick(aim_x: f32, aim_y: f32) -> PlayerInput {
    PlayerInput {
        aim_x,
        aim_y,
        ..PlayerInput::default()
    }
}

/// The standard range: a gallery at rows 22..29 with the rig at its west end and targets down
/// range; `extra` dresses it per weapon.
fn range_scene(
    stage: &mut SceneStage<'_>,
    weapon: WeaponKind,
    extra: impl FnOnce(&mut WorldState, PlayerId),
) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        gallery(world, player, 98..124, 22..29);
        place_rig(world, player, 101, 28);
        arm(world, player, weapon, 1);
        extra(world, player);
    });
    stage.use_devices(&[XBOX]);
}

/// Rivets streaming at a pair of targets.
pub(super) fn rivet_gun(stage: &mut SceneStage<'_>) {
    range_scene(stage, WeaponKind::RivetGun, |world, _| {
        target(world, tile(110), 28, 60.0);
        target(world, tile(116), 28, 60.0);
    });
    stage.settle(4);
    stage.hold(26, firing(1.0, 0.0));
}

/// A scatter blast fanning out at a cluster of targets.
pub(super) fn scatter_blaster(stage: &mut SceneStage<'_>) {
    range_scene(stage, WeaponKind::ScatterBlaster, |world, _| {
        target(world, tile(106), 28, 80.0);
        target(world, tile(108), 28, 80.0);
    });
    stage.settle(4);
    stage.hold(41, firing(1.0, -0.15));
    stage.hold(4, stick(1.0, -0.15));
}

/// The plasma cutter's beam slicing into a stone pillar.
pub(super) fn plasma_cutter(stage: &mut SceneStage<'_>) {
    range_scene(stage, WeaponKind::PlasmaCutter, |world, _| {
        for y in 24..=28 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x: 106, y }, TileKind::Stone);
        }
        target(world, tile(112), 28, 60.0);
    });
    stage.settle(4);
    stage.hold(40, firing(1.0, -0.1));
}

/// A rocket in flight toward a wall, a second one bursting against it.
pub(super) fn rocket_launcher(stage: &mut SceneStage<'_>) {
    range_scene(stage, WeaponKind::RocketLauncher, |world, _| {
        for y in 22..=28 {
            for x in 118..=121 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Stone);
            }
        }
        target(world, tile(116), 28, 200.0);
    });
    stage.settle(4);
    stage.hold(1, firing(1.0, -0.05));
    stage.hold(48, stick(1.0, -0.05));
    stage.hold(1, firing(1.0, -0.05));
    stage.hold(14, stick(1.0, -0.05));
}

/// A harpoon on its line flying at a target (it also reels in loose loot).
pub(super) fn harpoon(stage: &mut SceneStage<'_>) {
    range_scene(stage, WeaponKind::Harpoon, |world, _| {
        let _ = world.spawn_item_drop(
            tile(108),
            tile(29) - 6.0,
            (0.0, 0.0),
            ItemStack::new(ItemKind::Mineral(MineralKind::Gold), 3),
            None,
        );
        target(world, tile(115), 28, 60.0);
    });
    stage.settle(30);
    stage.hold(1, firing(1.0, -0.12));
    stage.hold(16, stick(1.0, -0.12));
}

/// A railgun slug piercing two targets and punching through a dirt wall.
pub(super) fn railgun(stage: &mut SceneStage<'_>) {
    range_scene(stage, WeaponKind::Railgun, |world, _| {
        target(world, tile(106), 28, 300.0);
        target(world, tile(109), 28, 300.0);
        for y in 22..=28 {
            for x in 113..=114 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Dirt);
            }
        }
        target(world, tile(119), 28, 300.0);
    });
    stage.settle(4);
    stage.hold(1, firing(1.0, 0.0));
    stage.hold(6, stick(1.0, 0.0));
}

/// Split-screen: player 1 aiming freely with a stick (crosshair), player 2 on the arrow keys
/// with auto-aim locked onto a target (brackets).
pub(super) fn split_reticles(stage: &mut SceneStage<'_>) {
    stage.play(2, |world, seats| {
        let first = seats[0].player;
        gallery(world, first, 98..124, 22..29);
        place_rig(world, first, 101, 28);
        arm(world, first, WeaponKind::RivetGun, 2);
        target(world, tile(112), 28, 60.0);
        if let Some(second) = seats.get(1) {
            reveal(world, second.player, 92..130, 18..35);
            place_rig(world, second.player, 120, 28);
            arm(world, second.player, WeaponKind::PlasmaCutter, 0);
        }
        target(world, tile(108), 28, 60.0);
    });
    stage.use_devices(&[XBOX, ARROWS]);
    let arrows = PlayerInput {
        auto_aim: true,
        ..stick(0.0, 0.0)
    };
    let face_left = PlayerInput {
        horizontal: -1.0,
        ..arrows
    };
    // Player 2 turns to face west, then holds still with auto-aim on.
    stage.hold_seats(2, &[stick(0.8, -0.6), face_left]);
    stage.hold_seats(20, &[stick(0.8, -0.6), arrows]);
}

/// The Explosives Shack counter scrolled to the arms rack.
pub(super) fn arms_rack(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 6_500;
            let _ = rig.receive_weapon(WeaponKind::RivetGun);
            rig.weapon_rack.owned.insert(WeaponKind::RivetGun, 1);
            let _ = rig.mount_weapon(Some(WeaponKind::RivetGun));
        }
    });
    stage.settle(5);
    put_seat_inside(
        stage,
        0,
        SurfaceZone::Explosives,
        Some(ModalScreen::Explosives),
    );
    stage.seat(0).selected_menu_item = crate::shell::arms::ARMS_FIRST_ROW + 2;
    stage.use_devices(&[WASD]);
}

/// The inventory listing every explosive type, some carried.
pub(super) fn explosives_inventory(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        place_rig(world, player, 97, 4);
        if let Some(rig) = world.player_mut(player) {
            rig.bombs = 7;
            for (kind, count) in [
                (ExplosiveKind::Sticky, 3),
                (ExplosiveKind::C4, 5),
                (ExplosiveKind::Flare, 9),
                (ExplosiveKind::Smoke, 4),
                (ExplosiveKind::Scare, 2),
            ] {
                rig.add_explosives(kind, count);
            }
            rig.cargo_bay.add(MineralKind::Silver, 4);
        }
    });
    stage.settle(10);
    stage.seat(0).modal = Some(ModalScreen::Inventory);
    stage.use_devices(&[WASD]);
}
