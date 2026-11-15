//! Capture scenes for the explosive kinds: a C4 chain on the remote detonator, a cluster bomb
//! splitting, a shaped charge's tunnel, the throw preview with a lit flare, and every charge
//! side by side.

use fatherload_simulation::{
    BlastDirection, ExplosiveKind, PlacedBomb, PlayerId, SimulationTick, ThrowAim, WorldState,
    explosive::REMOTE_FUSE,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{SceneStage, carve, place_rig, reveal};
use crate::{input::PlayerInput, input_mapping::ExplosiveSelection};

/// Collision half-size of a charge (`world::bombs::BOMB_RADIUS`).
const CHARGE_RADIUS: f32 = 6.0;

/// A stone chamber (tiles `xs` by `ys` hollow, stone two tiles around), explored by `player`.
fn chamber(
    world: &mut WorldState,
    player: PlayerId,
    xs: std::ops::Range<i32>,
    ys: std::ops::Range<i32>,
) {
    for y in ys.start - 2..ys.end + 14 {
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
        ys.start - 4..ys.end + 14,
    );
}

#[allow(clippy::cast_precision_loss, reason = "small tile indices")]
fn tile(index: i32) -> f32 {
    index as f32 * TILE_SIZE
}

/// A charge of `kind` stuck to the surface on side `direction` of tile `(x, y)` (an air tile).
fn stuck_charge(
    id: u64,
    kind: ExplosiveKind,
    owner: PlayerId,
    (x, y): (i32, i32),
    direction: BlastDirection,
    detonation_tick: SimulationTick,
) -> PlacedBomb {
    let centre = (
        TILE_SIZE.mul_add(0.5, tile(x)),
        TILE_SIZE.mul_add(0.5, tile(y)),
    );
    let gap = CHARGE_RADIUS + 1.0;
    let (px, py) = match direction {
        BlastDirection::Down => (centre.0, tile(y + 1) - gap),
        BlastDirection::Up => (centre.0, tile(y) + gap),
        BlastDirection::Left => (tile(x) + gap, centre.1),
        BlastDirection::Right => (tile(x + 1) - gap, centre.1),
    };
    PlacedBomb {
        id,
        x: px,
        y: py,
        previous_x: px,
        previous_y: py,
        detonation_tick,
        timer_seconds: 2.0,
        kind,
        owner: Some(owner),
        stuck: true,
        direction,
        ..PlacedBomb::default()
    }
}

fn add_charges(world: &mut WorldState, charges: Vec<PlacedBomb>) {
    let mut entities = world.live_entities();
    entities.next_bomb_id = charges.iter().map(|bomb| bomb.id + 1).max().unwrap_or(0);
    entities.bombs = charges;
    world.apply_live_entities(entities);
}

fn select(stage: &mut SceneStage<'_>, selected: ExplosiveKind) {
    stage.seat(0).explosives = ExplosiveSelection {
        selected,
        ..ExplosiveSelection::new()
    };
}

/// C4 on the floor, a wall and the ceiling of a chamber, waiting for the detonator (`fire`:
/// fired in sequence and caught mid-chain).
pub(super) fn c4_chain(stage: &mut SceneStage<'_>, fire: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        chamber(world, player, 98..120, 22..29);
        place_rig(world, player, 102, 28);
        if let Some(rig) = world.player_mut(player) {
            rig.add_explosives(ExplosiveKind::C4, 2);
        }
        let charges = [
            ((105, 28), BlastDirection::Down),
            ((108, 28), BlastDirection::Down),
            ((111, 22), BlastDirection::Up),
            ((114, 28), BlastDirection::Down),
            ((119, 25), BlastDirection::Right),
        ]
        .into_iter()
        .zip(0_u64..)
        .map(|((at, side), id)| stuck_charge(id, ExplosiveKind::C4, player, at, side, REMOTE_FUSE))
        .collect();
        add_charges(world, charges);
    });
    select(stage, ExplosiveKind::C4);
    stage.settle(8);
    if fire {
        stage.hold(
            1,
            PlayerInput {
                detonate_sequence: true,
                ..PlayerInput::default()
            },
        );
        stage.settle(40);
    }
}

/// A cluster bomb lobbed across a tall cavern, caught as it bursts (`late`: as the bomblets
/// land and pop).
pub(super) fn cluster_split(stage: &mut SceneStage<'_>, late: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        chamber(world, player, 98..123, 17..29);
        place_rig(world, player, 100, 28);
        let x = tile(104);
        let y = tile(26);
        let now = world.simulation_tick().get();
        add_charges(
            world,
            vec![PlacedBomb {
                id: 0,
                x,
                y,
                previous_x: x,
                previous_y: y,
                velocity_x: 240.0,
                velocity_y: -230.0,
                detonation_tick: SimulationTick::new(now + 30),
                timer_seconds: 0.7,
                kind: ExplosiveKind::Cluster,
                owner: Some(player),
                ..PlacedBomb::default()
            }],
        );
    });
    stage.settle(if late { 84 } else { 40 });
}

/// A shaped charge stuck on a chamber floor, caught as its jet cuts (`after`: the finished
/// tunnel).
pub(super) fn shaped_tunnel(stage: &mut SceneStage<'_>, after: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        chamber(world, player, 98..121, 20..26);
        place_rig(world, player, 101, 25);
        let now = world.simulation_tick().get();
        add_charges(
            world,
            vec![stuck_charge(
                0,
                ExplosiveKind::Shaped,
                player,
                (110, 25),
                BlastDirection::Down,
                SimulationTick::new(now + 10),
            )],
        );
    });
    stage.settle(if after { 60 } else { 15 });
}

/// The throw aim preview of a sticky bomb arcing at a wall, with a lit flare lighting the far
/// end of the cavern.
pub(super) fn throw_preview(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        chamber(world, player, 98..123, 21..29);
        place_rig(world, player, 102, 28);
        if let Some(rig) = world.player_mut(player) {
            rig.add_explosives(ExplosiveKind::Sticky, 3);
            rig.add_explosives(ExplosiveKind::Flare, 4);
        }
        let flare_x = TILE_SIZE.mul_add(0.5, tile(118));
        let flare_y = tile(29) - CHARGE_RADIUS - 1.0;
        let now = world.simulation_tick().get();
        add_charges(
            world,
            vec![PlacedBomb {
                id: 0,
                x: flare_x,
                y: flare_y,
                previous_x: flare_x,
                previous_y: flare_y,
                detonation_tick: SimulationTick::new(now + 600),
                kind: ExplosiveKind::Flare,
                owner: Some(player),
                stage: 3,
                ..PlacedBomb::default()
            }],
        );
    });
    stage.seat(0).explosives = ExplosiveSelection {
        selected: ExplosiveKind::Sticky,
        facing: 1,
        last_aim: ThrowAim(14),
    };
    stage.hold(
        10,
        PlayerInput {
            aim_x: 0.75,
            aim_y: -0.65,
            ..PlayerInput::default()
        },
    );
}

/// One of every charge resting in a row (stuck ones on the floor), for reviewing the art.
pub(super) fn lineup(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        chamber(world, player, 98..123, 23..29);
        place_rig(world, player, 100, 28);
        let now = world.simulation_tick().get();
        let later = SimulationTick::new(now + 2_000);
        let kinds = [
            ExplosiveKind::Bomb,
            ExplosiveKind::Sticky,
            ExplosiveKind::Cluster,
            ExplosiveKind::Bomblet,
            ExplosiveKind::C4,
            ExplosiveKind::Shaped,
            ExplosiveKind::Mining,
            ExplosiveKind::Incendiary,
            ExplosiveKind::Emp,
            ExplosiveKind::Flare,
        ];
        let charges = kinds
            .into_iter()
            .zip(0_u64..)
            .map(|(kind, id)| {
                let column = 103 + i32::try_from(id).unwrap_or(0) * 2;
                let fuse = if kind == ExplosiveKind::C4 {
                    REMOTE_FUSE
                } else {
                    later
                };
                let mut charge =
                    stuck_charge(id, kind, player, (column, 28), BlastDirection::Down, fuse);
                charge.stuck = kind.sticks();
                charge.timer_seconds = 30.0;
                charge
            })
            .collect();
        add_charges(world, charges);
    });
    stage.settle(90);
}
