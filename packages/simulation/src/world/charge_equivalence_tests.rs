//! Equivalence of the explosive charges before and after their move onto the entity store.
//!
//! Each scenario runs full simulation ticks and folds a trace (every event, every charge's
//! complete state, every rig's pose and velocity, every drop's position and every changed tile)
//! into a digest. The digests were recorded with the original `Vec<PlacedBomb>` implementation
//! (its own physics in `world/bombs.rs`); the entity-store charges must reproduce them bit for
//! bit, chain reactions and stagger included.

use std::fmt::Write as _;

use super::WorldImport;
use crate::{
    ExplosiveKind, PlacedBomb, PlayerCommand, PlayerId, SimulationTick, ThrowAim, TickInputs,
    WorldState,
    item::{ItemKind, ItemStack},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);

/// FNV-1a over the trace text: stable across platforms and runs.
fn digest(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

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
    park(&mut world, PLAYER, 12.5, 31.5);
    world
}

fn park(world: &mut WorldState, player_id: PlayerId, tile_x: f32, tile_y: f32) {
    let player = world.player_mut(player_id).expect("player");
    player.x = tile_x * TILE_SIZE;
    player.y = tile_y * TILE_SIZE;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

fn fill(world: &mut WorldState, x: std::ops::RangeInclusive<i32>, y: i32, kind: TileKind) {
    for column in x {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x: column, y }, kind);
    }
}

/// Places a scenario charge directly (the only helper that knows how charges are stored).
fn place(world: &mut WorldState, bomb: PlacedBomb) {
    world.insert_charge(bomb);
}

fn loose(id: u64, kind: ExplosiveKind, tile_x: f32, tile_y: f32, deadline: u64) -> PlacedBomb {
    let x = tile_x * TILE_SIZE;
    let y = tile_y * TILE_SIZE;
    PlacedBomb {
        id,
        x,
        y,
        previous_x: x,
        previous_y: y,
        detonation_tick: SimulationTick::new(deadline),
        timer_seconds: 1.0,
        kind,
        owner: Some(PLAYER),
        ..PlacedBomb::default()
    }
}

fn give_all(world: &mut WorldState, player_id: PlayerId) {
    let player = world.player_mut(player_id).expect("player");
    player.bombs = 20;
    for kind in ExplosiveKind::SELECTABLE {
        player.add_explosives(kind, 6);
    }
}

/// Runs `ticks` full ticks, issuing `script` commands on their tick offsets, and returns the
/// trace digest.
fn run(world: &mut WorldState, ticks: u64, script: &[(u64, PlayerId, PlayerCommand)]) -> u64 {
    let mut trace = String::new();
    for offset in 0..ticks {
        let mut inputs = TickInputs::default();
        for (at, player, command) in script {
            if *at == offset {
                inputs = inputs.act(*player, command.clone());
            }
        }
        let output = world.simulate_tick(world.simulation_tick(), &inputs);
        let _ = writeln!(trace, "tick {}", output.tick.get());
        for event in &output.events {
            let _ = writeln!(trace, "  event {:?}", event.kind);
        }
        for outcome in &output.action_outcomes {
            let _ = writeln!(trace, "  outcome {:?}", outcome.outcome);
        }
        let bombs = world.bombs();
        for bomb in &bombs {
            let _ = writeln!(trace, "  charge {bomb:?}");
        }
        for (id, player) in world.players() {
            let _ = writeln!(
                trace,
                "  rig {id:?} {} {} {} {}",
                player.x, player.y, player.velocity_x, player.velocity_y
            );
        }
        for entity in world.entities().of_kind(crate::entity::EntityKind::Drop) {
            let _ = writeln!(trace, "  drop {} {}", entity.body.x, entity.body.y);
        }
        for op in &output.terrain_ops {
            let _ = writeln!(trace, "  op {op:?}");
        }
    }
    if let Ok(dir) = std::env::var("CHARGE_TRACE_DIR") {
        let name = format!("{dir}/{}.txt", digest(&trace));
        let _ = std::fs::write(name, &trace);
    }
    digest(&trace)
}

fn drops(world: &mut WorldState) {
    for index in 0..6_u8 {
        let _ = world.spawn_item_drop(
            f32::from(index).mul_add(2.5, 14.0) * TILE_SIZE,
            31.4 * TILE_SIZE,
            (0.0, 0.0),
            ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 1),
            None,
        );
    }
}

fn pile_and_chains() -> u64 {
    let mut world = arena();
    drops(&mut world);
    // A coincident pile, a spread chain and an occluded bomb behind a wall.
    for id in 0..5 {
        place(&mut world, loose(id, ExplosiveKind::Bomb, 20.5, 30.5, 40));
    }
    fill(&mut world, 22..=30, 27, TileKind::Clay);
    for (index, x) in [23.5_f32, 25.5, 27.5, 29.5].into_iter().enumerate() {
        let id = 10 + u64::try_from(index).expect("small");
        place(&mut world, loose(id, ExplosiveKind::Bomb, x, 26.5, 400));
    }
    place(&mut world, loose(20, ExplosiveKind::Mining, 22.5, 24.5, 60));
    fill(&mut world, 31..=31, 25, TileKind::Stone);
    fill(&mut world, 31..=31, 26, TileKind::Stone);
    place(&mut world, loose(21, ExplosiveKind::Bomb, 32.5, 26.5, 500));
    run(&mut world, 240, &[])
}

#[allow(clippy::too_many_lines, reason = "one scripted scenario")]
fn every_kind_deployed() -> u64 {
    let mut world = arena();
    drops(&mut world);
    world.add_player(OTHER);
    park(&mut world, OTHER, 30.5, 20.5);
    give_all(&mut world, PLAYER);
    give_all(&mut world, OTHER);
    // A ceiling and a wall to stick to; ore for the mining charge to spare.
    fill(&mut world, 8..=40, 12, TileKind::Stone);
    fill(&mut world, 36..=40, 20, TileKind::Clay);
    let _ = world.terrain_mut().set_kind(
        TilePosition { x: 15, y: 31 },
        TileKind::Ore(MineralKind::Gold),
    );
    let script = [
        (0, PLAYER, PlayerCommand::PlaceBomb),
        (
            5,
            PLAYER,
            PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::Sticky,
                aim: ThrowAim::UP,
            },
        ),
        (
            9,
            PLAYER,
            PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::Cluster,
                aim: ThrowAim(14),
            },
        ),
        (
            12,
            OTHER,
            PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::C4,
                aim: ThrowAim::RIGHT,
            },
        ),
        (
            13,
            OTHER,
            PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::C4,
                aim: ThrowAim::UP,
            },
        ),
        (
            15,
            OTHER,
            PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::Shaped,
                aim: ThrowAim::RIGHT,
            },
        ),
        (
            20,
            PLAYER,
            PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::Mining,
                aim: ThrowAim::DOWN,
            },
        ),
        (
            24,
            PLAYER,
            PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::Incendiary,
                aim: ThrowAim(15),
            },
        ),
        (
            30,
            PLAYER,
            PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::Emp,
                aim: ThrowAim(13),
            },
        ),
        (
            34,
            PLAYER,
            PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::Flare,
                aim: ThrowAim(11),
            },
        ),
        (
            40,
            PLAYER,
            PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::C4,
                aim: ThrowAim::DOWN,
            },
        ),
        (
            130,
            OTHER,
            PlayerCommand::RemoteDetonate { sequenced: true },
        ),
        (
            200,
            PLAYER,
            PlayerCommand::RemoteDetonate { sequenced: false },
        ),
    ];
    run(&mut world, 420, &script)
}

fn support_removed_and_rig_pushed() -> u64 {
    let mut world = arena();
    // Bombs resting on a shelf that a blast below removes; the rig stands beside the blast.
    fill(&mut world, 16..=24, 22, TileKind::Dirt);
    fill(&mut world, 20..=23, 24, TileKind::Stone);
    place(&mut world, loose(0, ExplosiveKind::Bomb, 20.5, 21.5, 900));
    place(&mut world, loose(1, ExplosiveKind::Bomb, 17.5, 21.5, 900));
    place(&mut world, loose(2, ExplosiveKind::Bomb, 21.5, 23.5, 30));
    park(&mut world, PLAYER, 19.5, 31.5);
    let mut stuck = loose(3, ExplosiveKind::Sticky, 24.5, 22.8, 2_000);
    stuck.velocity_y = -50.0;
    place(&mut world, stuck);
    let mut thrown = loose(4, ExplosiveKind::Bomb, 12.5, 14.5, 120);
    thrown.velocity_x = 560.0;
    thrown.velocity_y = -300.0;
    place(&mut world, thrown);
    run(&mut world, 200, &[])
}

#[test]
fn entity_store_charges_reproduce_the_original_bomb_traces() {
    let digests = [
        pile_and_chains(),
        every_kind_deployed(),
        support_removed_and_rig_pushed(),
    ];
    assert_eq!(digests, GOLDEN, "{digests:#x?}");
}

/// Recorded with the original `Vec<PlacedBomb>` charges. The second scenario (which fires the
/// remote detonator) was re-recorded when the detonator gained its arming delay
/// (`REMOTE_ARMING_MILLIS`), a deliberate behaviour change after the move; the others are the
/// original recordings.
const GOLDEN: [u64; 3] = [
    0x0061_6a47_494e_3417,
    0x3905_8a11_9ecb_234b,
    0xe701_07ea_06e3_40c5,
];
