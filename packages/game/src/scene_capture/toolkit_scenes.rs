//! The god-mode toolkit's scenes (Wave 14, W14-A): the toolkit dialog on a Deep Descent
//! test world (its supplies and progress rows, then its creature rows with a boss picked), a
//! creature spawned beside the rig, and a fully stocked rig's inventory.

use fatherload_simulation::{
    PlayerCommand, TestModeAction, TickInputs,
    enemy::EnemyKind,
    terrain::Terrain,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::{Scene, SceneStage, sink_rig};
use crate::shell::{ModalScreen, RunMode, test_toolkit::ToolkitRow};

/// Seed of every toolkit scene world.
const SCENE_SEED: u64 = 0x0057_0C4E;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// A stocked Deep Descent test world with the rig sunk near the surface.
fn stocked(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        *world.terrain_mut() = Terrain::generate(&settings());
        world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
        world.enable_test_mode();
        sink_rig(world, seats[0].player, 97, 12);
    });
    stage.settle(20);
}

/// The toolkit open on `row`.
fn toolkit_on(stage: &mut SceneStage<'_>, row: ToolkitRow) {
    stocked(stage);
    // The road's first word from the archivist (space opens with the stock) is not the subject.
    stage.seat(0).message.clear();
    let shell = stage.shell();
    shell.message.clear();
    shell.run_mode = RunMode::Paused;
    shell.modal = Some(ModalScreen::TestToolkit);
    shell.selected_menu_item = ToolkitRow::rows(true)
        .iter()
        .position(|candidate| *candidate == row)
        .unwrap_or(0);
}

/// The index of `kind` in the registry.
fn creature_index(kind: EnemyKind) -> usize {
    EnemyKind::ALL
        .iter()
        .position(|candidate| *candidate == kind)
        .unwrap_or(0)
}

pub(super) const TOOLKIT_SCENES: &[Scene] = &[
    Scene {
        name: "test_toolkit",
        summary: "The God-Mode Toolkit: supplies and progress switches in a Deep Descent test world",
        setup: |stage| toolkit_on(stage, ToolkitRow::Gate(2)),
    },
    Scene {
        name: "test_toolkit_creatures",
        summary: "The God-Mode Toolkit scrolled to its creature tools with a boss picked",
        setup: |stage| {
            toolkit_on(stage, ToolkitRow::Creature);
            stage.shell().toolkit_creature = creature_index(EnemyKind::Sporemother);
        },
    },
    Scene {
        name: "test_toolkit_spawned",
        summary: "A creature spawned by the toolkit beside the rig, its AI frozen",
        setup: |stage| {
            stocked(stage);
            let kind = EnemyKind::ALL
                .into_iter()
                .find(|kind| kind.name() == "Cave Bat")
                .unwrap_or(EnemyKind::ALL[0]);
            stage.play_on(|world| {
                let Some(player) = world.players().keys().next().copied() else {
                    return;
                };
                for action in [
                    TestModeAction::SetAiFrozen(true),
                    TestModeAction::SpawnCreature(kind),
                ] {
                    let tick = world.simulation_tick();
                    let mut inputs = TickInputs::default();
                    inputs.act_as(player, PlayerCommand::TestMode(action));
                    let _ = world.simulate_tick(tick, &inputs);
                }
            });
            stage.settle(10);
            stage.shell().message.clear();
            stage.seat(0).message.clear();
        },
    },
    Scene {
        name: "stocked_inventory",
        summary: "A fully stocked test rig's inventory: every charge, ammo, kit and supply",
        setup: |stage| {
            stocked(stage);
            stage.shell().message.clear();
            let seat = stage.seat(0);
            seat.message.clear();
            seat.modal = Some(ModalScreen::Inventory);
            // The last row (depth logistics): the list scrolls down to everything below it, the
            // consumables and field kits.
            seat.selected_menu_item = 2;
        },
    },
];
