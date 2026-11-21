//! Crypt layouts: every plan of every tier fits the slot, every room and fixture is reachable
//! from the start, pit floors keep a safe lane, gates hang under lintels in reach of their
//! lever, spawns stand on clear floor, and the layout is pure.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::*;
use crate::{
    mechanism::{REACH_COLUMNS, REACH_ROWS},
    tomb::{SiteStyle, floor::plan_floor},
};

fn plans() -> Vec<FloorPlan> {
    let mut plans = Vec::new();
    for tier in 1..=5 {
        let site = crate::tomb::floor::tests::site(SiteStyle::TopDown, tier);
        for run in 0..6 {
            for floor in 0..site.floors() {
                plans.push(plan_floor(&site, run, floor));
            }
        }
    }
    plans
}

/// Open tiles reachable from `start` (gates count as open: their lever opens them), avoiding
/// tiles `avoid` says no to.
fn reachable(
    cells: &BTreeMap<TilePosition, TileKind>,
    start: TilePosition,
    avoid: impl Fn(TilePosition) -> bool,
) -> BTreeSet<TilePosition> {
    let passable = |tile: TilePosition| {
        matches!(
            cells.get(&tile),
            Some(TileKind::Air | TileKind::Built(BuiltTile::Gate))
        ) && !avoid(tile)
    };
    let mut seen = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(at) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = TilePosition {
                x: at.x + dx,
                y: at.y + dy,
            };
            if passable(next) && seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    seen
}

fn beside(reach: &BTreeSet<TilePosition>, tile: TilePosition) -> bool {
    [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
        reach.contains(&TilePosition {
            x: tile.x + dx,
            y: tile.y + dy,
        })
    })
}

#[test]
fn every_crypt_floor_fits_and_everything_is_reachable() {
    for plan in plans() {
        let floor = layout(&plan);
        let realized = &floor.realized;
        assert!(realized.size.0 <= MAX_FLOOR_SIZE.0 && realized.size.1 <= MAX_FLOOR_SIZE.1);
        let cells: BTreeMap<TilePosition, TileKind> = realized.cells.iter().copied().collect();
        for tile in cells.keys() {
            assert!(
                tile.x >= 0
                    && tile.y >= 0
                    && tile.x < MAX_FLOOR_SIZE.0
                    && tile.y < MAX_FLOOR_SIZE.1,
                "{tile:?}"
            );
        }
        let reach = reachable(&cells, realized.start, |_| false);
        for room in &realized.rooms {
            assert!(
                reach.contains(&room.interior.center())
                    || cells
                        .get(&room.interior.center())
                        .is_some_and(|kind| *kind != TileKind::Air),
                "room {:?} unreachable (floor {} seed {})",
                room.kind,
                plan.floor,
                plan.seed
            );
        }
        for (tile, kind) in &cells {
            if let TileKind::Built(
                BuiltTile::TombShrine { .. }
                | BuiltTile::TombStairs
                | BuiltTile::TombCache { .. }
                | BuiltTile::TombExit
                | BuiltTile::Lever { .. },
            ) = kind
            {
                assert!(beside(&reach, *tile), "{kind:?} at {tile:?} unreachable");
            }
        }
        // Every gate's lintel is right above its column, and its lever is in reach.
        for (tile, kind) in &cells {
            if let TileKind::Built(BuiltTile::GateLintel(channel)) = kind {
                assert_eq!(
                    cells.get(&TilePosition {
                        x: tile.x,
                        y: tile.y + 1
                    }),
                    Some(&TileKind::Built(BuiltTile::Gate))
                );
                assert!(cells.iter().any(|(lever, kind)| {
                    matches!(kind, TileKind::Built(BuiltTile::Lever { channel: c, .. }) if c == channel)
                        && (lever.x - tile.x).abs() <= REACH_COLUMNS
                        && (lever.y - tile.y).abs() <= REACH_ROWS
                }));
            }
        }
        // Traps lie on open floor, one per tile, in tile order.
        for pair in floor.traps.windows(2) {
            assert!(pair[0].at < pair[1].at);
        }
        for trap in &floor.traps {
            assert_eq!(cells.get(&trap.at), Some(&TileKind::Air), "{trap:?}");
        }
        // Spawns stand on clear open floor.
        for spawn in &realized.spawns {
            assert_eq!(cells.get(&spawn.at), Some(&TileKind::Air));
            assert!(
                floor.traps.iter().all(|trap| trap.at != spawn.at),
                "{spawn:?}"
            );
        }
        // Pure.
        assert_eq!(floor, layout(&plan));
    }
}

#[test]
fn pit_floors_keep_a_safe_lane_and_every_trap_kind_shows_up() {
    let mut kinds = BTreeSet::new();
    let mut pit_halls = 0;
    for plan in plans() {
        let floor = layout(&plan);
        let realized = &floor.realized;
        let cells: BTreeMap<TilePosition, TileKind> = realized.cells.iter().copied().collect();
        kinds.extend(floor.traps.iter().map(|trap| trap.kind));
        let pits: BTreeSet<TilePosition> = floor
            .traps
            .iter()
            .filter(|trap| matches!(trap.kind, CryptTrapKind::Pit | CryptTrapKind::CrackedFloor))
            .map(|trap| trap.at)
            .collect();
        if pits.is_empty() {
            continue;
        }
        pit_halls += 1;
        // Without stepping on a crack or into a pit, the stairs are still reachable.
        let reach = reachable(&cells, realized.start, |tile| pits.contains(&tile));
        let stairs = cells
            .iter()
            .find(|(_, kind)| **kind == TileKind::Built(BuiltTile::TombStairs))
            .map(|(tile, _)| *tile)
            .expect("stairs");
        assert!(
            beside(&reach, stairs),
            "seed {} floor {}",
            plan.seed,
            plan.floor
        );
    }
    assert!(pit_halls > 3, "{pit_halls}");
    for kind in [
        CryptTrapKind::Spikes,
        CryptTrapKind::ArrowPlate,
        CryptTrapKind::BoulderChute,
        CryptTrapKind::FlameJet,
        CryptTrapKind::CrackedFloor,
        CryptTrapKind::Pit,
    ] {
        assert!(kinds.contains(&kind), "{kind:?} never placed");
    }
}

#[test]
fn floors_wind_over_the_grid_and_the_boss_hall_is_two_by_two() {
    let mut rows = BTreeSet::new();
    for plan in plans() {
        let floor = layout(&plan);
        for (index, cells) in floor.cells.iter().enumerate() {
            rows.extend(cells.iter().map(|cell| cell.1));
            let expected = if plan.rooms[index].kind == RoomKind::Boss {
                4
            } else {
                1
            };
            assert_eq!(cells.len(), expected);
        }
    }
    assert_eq!(rows.len(), usize::try_from(GRID.1).unwrap_or(0));
}
