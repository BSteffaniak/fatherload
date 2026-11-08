//! Per-chunk terrain revision tracking for terrain change replication.

use std::collections::{BTreeMap, BTreeSet};

use crate::terrain::TilePosition;

pub const TERRAIN_CHUNK_SIZE_TILES: i32 = 16;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TerrainChunkPosition {
    pub x: i32,
    pub y: i32,
}

impl TerrainChunkPosition {
    #[must_use]
    pub const fn from_tile(position: TilePosition) -> Self {
        Self {
            x: position.x.div_euclid(TERRAIN_CHUNK_SIZE_TILES),
            y: position.y.div_euclid(TERRAIN_CHUNK_SIZE_TILES),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerrainChunkRevision {
    pub position: TerrainChunkPosition,
    pub revision: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerrainRevisionTracker {
    chunk_revisions: BTreeMap<TerrainChunkPosition, u64>,
}

impl TerrainRevisionTracker {
    pub fn mark_tiles_changed<I>(&mut self, positions: I) -> Vec<TerrainChunkRevision>
    where
        I: IntoIterator<Item = TilePosition>,
    {
        let changed_chunks = positions
            .into_iter()
            .map(TerrainChunkPosition::from_tile)
            .collect::<BTreeSet<_>>();

        changed_chunks
            .into_iter()
            .map(|position| {
                let revision = self.chunk_revisions.entry(position).or_insert(0);
                *revision = revision.saturating_add(1);
                TerrainChunkRevision {
                    position,
                    revision: *revision,
                }
            })
            .collect()
    }

    #[must_use]
    pub fn revision(&self, position: TerrainChunkPosition) -> u64 {
        self.chunk_revisions.get(&position).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn revised_chunk_positions(&self) -> Vec<TerrainChunkPosition> {
        self.chunk_revisions.keys().copied().collect()
    }
}
