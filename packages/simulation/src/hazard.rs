//! Live hazard entities (gas clouds, falling boulders, placed bombs, scanner markers) and the
//! ground that settles after a collapse.

use serde::{Deserialize, Serialize};

use crate::{
    explosive::{BlastDirection, ExplosiveKind, REMOTE_FUSE},
    ids::PlayerId,
    terrain::{TileKind, TilePosition},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct HazardCloud {
    pub x: f32,
    pub y: f32,
    pub life: f32,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct PlacedBomb {
    /// Stable session identity; retained in replication and saves.
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    /// Previous tick position for presentation-only interpolation.
    pub previous_x: f32,
    pub previous_y: f32,
    pub angle: f32,
    pub detonation_tick: crate::SimulationTick,
    pub chain_triggered: bool,
    pub timer_seconds: f32,
    /// What kind of explosive this is (older saves and replicas: the plain timed bomb).
    #[serde(default)]
    pub kind: ExplosiveKind,
    /// The player who deployed it (who may trigger it remotely). `None` for charges from older
    /// saves and for scenario bombs.
    #[serde(default)]
    pub owner: Option<PlayerId>,
    /// Stuck to a surface: no gravity, no push, until the tile holding it is gone.
    #[serde(default)]
    pub stuck: bool,
    /// Which way a shaped charge cuts (into the surface it stuck to, or its throw direction).
    #[serde(default)]
    pub direction: BlastDirection,
    /// Lifecycle stage after the first detonation: an incendiary's fire ring, a flare's pulse.
    /// 0 until the fuse first runs out.
    #[serde(default)]
    pub stage: u8,
}

impl PlacedBomb {
    /// Whether this is a remote charge still waiting for its owner's detonator.
    #[must_use]
    pub fn awaiting_remote(&self) -> bool {
        self.kind.remote() && self.detonation_tick == REMOTE_FUSE
    }

    /// Whether the charge has gone off already and is burning (a lit flare, a spreading fire).
    #[must_use]
    pub const fn burning(&self) -> bool {
        self.stage > 0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct FallingBoulder {
    pub x: f32,
    pub y: f32,
    pub velocity_y: f32,
    pub warning_seconds: f32,
    pub life: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ScanMarker {
    pub position: TilePosition,
    pub kind: TileKind,
}

/// Ground that came down recently and holds for a while.
///
/// Every loosened boulder, cave-in or stalactite settles its cell of [`SettledGround::CELL_TILES`] square tiles until a deadline,
/// and no random collapse (mining boulders, collapse surges, deep-run pressure, earthquakes,
/// stalactites) starts in or next to a settled cell. This is what keeps one unstable spot from
/// raining boulders over and over.
///
/// Kept sorted by cell; expired cells are pruned every tick.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SettledGround {
    cells: Vec<SettledCell>,
}

/// One settled cell (cell coordinates are tile coordinates divided by
/// [`SettledGround::CELL_TILES`], rounding down) and the tick it may move again.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SettledCell {
    pub x: i32,
    pub y: i32,
    pub until: crate::SimulationTick,
}

impl SettledGround {
    /// Side of a settled cell, in tiles.
    pub const CELL_TILES: i32 = 8;

    /// The cell holding `tile`.
    #[must_use]
    pub const fn cell_of(tile: TilePosition) -> (i32, i32) {
        (
            tile.x.div_euclid(Self::CELL_TILES),
            tile.y.div_euclid(Self::CELL_TILES),
        )
    }

    /// Whether nothing is settled.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The settled cells, sorted by cell.
    #[must_use]
    pub fn cells(&self) -> &[SettledCell] {
        &self.cells
    }

    /// Whether `tile`'s cell or one of its eight neighbours is still settled at `now`.
    #[must_use]
    pub fn holds(&self, tile: TilePosition, now: crate::SimulationTick) -> bool {
        let (x, y) = Self::cell_of(tile);
        self.cells
            .iter()
            .any(|cell| (cell.x - x).abs() <= 1 && (cell.y - y).abs() <= 1 && now < cell.until)
    }

    /// Settles `tile`'s cell until `until` (keeping a later deadline already there).
    pub fn settle(&mut self, tile: TilePosition, until: crate::SimulationTick) {
        let (x, y) = Self::cell_of(tile);
        match self
            .cells
            .binary_search_by(|cell| (cell.x, cell.y).cmp(&(x, y)))
        {
            Ok(index) => {
                let cell = &mut self.cells[index];
                cell.until = cell.until.max(until);
            }
            Err(index) => self.cells.insert(index, SettledCell { x, y, until }),
        }
    }

    /// Forgets cells whose deadline has passed.
    pub fn prune(&mut self, now: crate::SimulationTick) {
        self.cells.retain(|cell| now < cell.until);
    }
}
