//! Wave 10 polish scenes: the deep supplies at work (a Flare Buoy lighting the Pearl Abyss and
//! dazzling its lure angler, Beast Bait drawing the fangrunners off in the jungle). The deep
//! settlements' own styles, the lifted Glow Sea and the larger far creatures show in the
//! existing scenes (`drowned_merrow_city`, `hollow_sunfolk_village`,
//! `distortion_parallax_station`, `living_glow_sea`, `far_*`).

use fatherload_simulation::{terrain::TilePosition, workshop::DeepSupply};

use super::{Scene, SceneStage};

/// Lays `supply` near the seat's rig, `dx` tiles off.
fn lay(stage: &mut SceneStage<'_>, supply: DeepSupply, dx: i32) {
    stage.play_on(move |world| {
        let Some(player) = world.player_ids().next() else {
            return;
        };
        if let Some(tile) = world.actor_tile(player) {
            world.stage_supply_lure(
                supply,
                player,
                TilePosition {
                    x: tile.x + dx,
                    y: tile.y,
                },
            );
        }
    });
}

/// The Pearl Abyss with a Flare Buoy burning beside the submarine.
fn flare_buoy(stage: &mut SceneStage<'_>) {
    super::drowned_scenes::pearl_abyss(stage);
    lay(stage, DeepSupply::FlareBuoy, 2);
    stage.settle(40);
}

/// The jungle with Beast Bait thrown past the fangrunners.
fn beast_bait(stage: &mut SceneStage<'_>) {
    super::hollow_scenes::jungle(stage, true);
    lay(stage, DeepSupply::BeastBait, -9);
    stage.settle(90);
}

pub const POLISH_SCENES: &[Scene] = &[
    Scene {
        name: "polish_flare_buoy",
        summary: "A Flare Buoy burning in the Pearl Abyss: its warm light pushes the black water back and dazzles the lure angler",
        setup: flare_buoy,
    },
    Scene {
        name: "polish_beast_bait",
        summary: "Beast Bait lying in the Megafauna Jungle, its scent curling up, the fangrunners turned to it",
        setup: beast_bait,
    },
];
