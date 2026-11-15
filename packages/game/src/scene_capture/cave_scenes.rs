//! Capture scenes for cave ecosystems (WP-CAVES): a rig inside one generated cave of every
//! kind, and a pitch-dark grand cavern lit by the rig's lamp alone and then by flares.

use fatherload_simulation::{
    ExplosiveKind, PlacedBomb, PlayerId, SimulationTick, WorldState,
    cave::CaveKind,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{FeatureKind, TileRect, WorldFeature},
};

use super::{SceneStage, place_rig, reveal};

/// Columns searched for a cave of each kind (either side of the town).
const SEARCH_COLUMNS: i32 = 3_000;

/// The cave of `kind` nearest the town's shallowest rows (stable for the default seed).
fn find_cave(world: &WorldState, kind: CaveKind) -> Option<WorldFeature> {
    let terrain = world.terrain();
    let area = TileRect {
        min: TilePosition {
            x: -SEARCH_COLUMNS,
            y: 0,
        },
        max: TilePosition {
            x: SEARCH_COLUMNS,
            y: terrain.height() - 1,
        },
    };
    terrain
        .features_in(&area, Some(&[FeatureKind::Cave]))
        .into_iter()
        .filter(|feature| feature.detail.cave() == Some(kind))
        .min_by_key(|feature| (feature.center.y + feature.center.x.abs() / 4, feature.id))
}

/// An open tile with ground below inside `cave`, nearest its centre shifted by `offset`.
fn floor_tile(world: &WorldState, cave: &WorldFeature, offset: i32) -> TilePosition {
    let terrain = world.terrain();
    let target = TilePosition {
        x: cave.center.x + offset,
        y: cave.center.y,
    };
    cave.bounds
        .positions()
        .filter(|tile| {
            let open = |t: TilePosition| {
                terrain
                    .tile(t)
                    .is_some_and(|found| found.kind == TileKind::Air)
            };
            terrain.cave_at(*tile).is_some_and(|sample| sample.interior)
                && open(*tile)
                && open(TilePosition {
                    x: tile.x,
                    y: tile.y - 1,
                })
                && !open(TilePosition {
                    x: tile.x,
                    y: tile.y + 1,
                })
        })
        .min_by_key(|tile| {
            (
                (tile.x - target.x).pow(2) + (tile.y - target.y).pow(2) * 3,
                tile.y,
                tile.x,
            )
        })
        .unwrap_or(cave.center)
}

/// Puts `player` in `cave` and charts the cave (and a margin) for them.
fn enter_cave(
    world: &mut WorldState,
    player: PlayerId,
    cave: &WorldFeature,
    offset: i32,
) -> TilePosition {
    let spot = floor_tile(world, cave, offset);
    place_rig(world, player, spot.x, spot.y);
    let bounds = cave.bounds.expanded(6);
    reveal(
        world,
        player,
        bounds.min.x..bounds.max.x + 1,
        bounds.min.y..bounds.max.y + 1,
    );
    spot
}

/// A rig inside the nearest cave of `kind`, its critters out (the population refreshes twice
/// a second).
pub(super) fn cave_of_kind(stage: &mut SceneStage<'_>, kind: CaveKind) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        if let Some(cave) = find_cave(world, kind) {
            let _ = enter_cave(world, player, &cave, 0);
        }
    });
    stage.settle(70);
}

#[allow(clippy::cast_precision_loss, reason = "small tile indices")]
fn tile_centre(index: i32) -> f32 {
    (index as f32 + 0.5) * TILE_SIZE
}

/// A pitch-dark grand cavern: the rig's lamp alone (`flares`: false), or with two lit flares
/// thrown out across the floor.
pub(super) fn dark_cavern(stage: &mut SceneStage<'_>, flares: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(cave) = find_cave(world, CaveKind::GrandCavern) else {
            return;
        };
        let spot = enter_cave(world, player, &cave, -8);
        if !flares {
            return;
        }
        let now = world.simulation_tick().get();
        let charges: Vec<PlacedBomb> = [7, 15]
            .into_iter()
            .zip(0_u64..)
            .map(|(dx, id)| {
                let floor = floor_tile(world, &cave, spot.x + dx - cave.center.x);
                let (x, y) = (
                    tile_centre(floor.x),
                    TILE_SIZE.mul_add(0.3, tile_centre(floor.y)),
                );
                PlacedBomb {
                    id,
                    x,
                    y,
                    previous_x: x,
                    previous_y: y,
                    detonation_tick: SimulationTick::new(now + 900),
                    kind: ExplosiveKind::Flare,
                    owner: Some(player),
                    stage: 3,
                    ..PlacedBomb::default()
                }
            })
            .collect();
        let mut entities = world.live_entities();
        entities.next_bomb_id = 2;
        entities.bombs = charges;
        world.apply_live_entities(entities);
    });
    stage.settle(70);
}
