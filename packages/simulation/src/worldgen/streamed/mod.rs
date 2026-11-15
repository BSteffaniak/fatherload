//! Streamed, horizontally unbounded world generation ([`GeneratorVersion::V5`]).
//!
//! A V5 world has the V4 band stack and bedrock floor but no width: any chunk
//! ([`ChunkPosition`], [`CHUNK_SIZE_TILES`] square) can be generated on its own, on demand, as a
//! pure function of `(settings, chunk)` ([`generated_chunk`]). Generation is chunk-local:
//!
//! * the **base layer** ([`base`]) is evaluated per tile: the surface heightmap hook
//!   ([`surface_row`]), bedrock, the town's foundations and each band's rock mass;
//! * everything with extent (worm tunnels, chambers, points of interest, hazard clusters, ore
//!   pods, motherlodes, artifact beds) is placed per region of a fixed grid
//!   ([`regions`]), from a per-region RNG and the layers below it only, and reaches a bounded
//!   distance past its region. A chunk consults exactly the regions that can reach it, so chunk
//!   borders are seamless and the result never depends on which chunks were generated first;
//! * per-band ore budgets are met on average by the region densities rather than topped up per
//!   world (there is no "whole world" to total), and the starter field near spawn is a fixed
//!   set of pods that always pays for the first upgrades.
//!
//! The town stays on its classic columns with flat ground around it
//! ([`TOWN_FLAT_FIRST_COLUMN`] to [`TOWN_FLAT_LAST_COLUMN`]); the world extends without bound
//! to both sides (in practice to [`STREAMED_COLUMN_LIMIT`]). Rig positions stay `f32` world
//! pixels; far from spawn, where `f32` no longer holds the simulation's sub-pixel grid, the
//! movement step keeps the rest of the exact position in `Player::x_remainder`, so travel 50,000
//! tiles out is identical to travel near spawn.
//!
//! The feature index is regional ([`index`]): features carry ids that name their region, area
//! queries gather regions, and nearest-feature queries search outward.
//!
//! Every region output and generated chunk is memoized per process (see [`cache`]); a cache
//! miss only costs time, never changes a result.
//!
//! [`GeneratorVersion::V5`]: super::GeneratorVersion::V5

mod assemble;
mod base;
mod cache;
pub mod cave_biomes;
#[cfg(test)]
mod cave_tests;
pub mod index;
mod regions;
#[cfg(test)]
mod tests;
mod window;

use std::sync::Arc;

pub use assemble::CHUNK_TILES;
pub use base::{MAX_SURFACE_DIP_ROWS, TOWN_FLAT_FIRST_COLUMN, TOWN_FLAT_LAST_COLUMN, surface_row};
pub use regions::{POI_REGION_WIDTH, TRANSITION_REGION_WIDTH};

pub use cave_biomes::{BiomeCell, CAVE_REGION, CaveBiome, CaveRegion};

use crate::{
    cave::CaveSample,
    terrain::{CHUNK_SIZE_TILES, ChunkPosition, Tile, TilePosition},
    worldgen::{
        FeatureId, FeatureIndex, FeatureKind, GeneratedWorld, WorldSettings,
        features::{TileRect, WorldFeature},
    },
};

use base::Ctx;
use cache::MemoCache;

/// Furthest column (either side of 0) a streamed world extends to.
///
/// Rig positions are `f32` world pixels plus an exact remainder (see the module docs); out to
/// this column the stored `f32` still resolves a quarter pixel, so collision, rendering and
/// every other reader of a position behave as near spawn.
pub const STREAMED_COLUMN_LIMIT: i32 = 131_072;

/// One generated chunk: its tiles in [`ChunkPosition::local_index`] order.
#[derive(Debug)]
pub struct GeneratedChunk {
    tiles: Vec<Tile>,
}

impl GeneratedChunk {
    #[must_use]
    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }

    /// The tile at `local` ([`ChunkPosition::local_index`]).
    #[must_use]
    pub fn tile(&self, local: usize) -> Tile {
        self.tiles[local]
    }
}

static CONTEXTS: MemoCache<Ctx> = MemoCache::new(16);
static CHUNKS: MemoCache<GeneratedChunk> = MemoCache::new(4_096);

fn ctx(settings: &WorldSettings) -> Arc<Ctx> {
    CONTEXTS.get_or_compute((*settings, 0, 0), || Ctx::new(settings))
}

/// The freshly generated tiles of `chunk` (memoized per process).
#[must_use]
pub fn generated_chunk(settings: &WorldSettings, chunk: ChunkPosition) -> Arc<GeneratedChunk> {
    CHUNKS.get_or_compute((*settings, chunk.x, chunk.y), || GeneratedChunk {
        tiles: assemble::assemble(&ctx(settings), chunk),
    })
}

/// The generated tile at `position` (rows outside the world read as bedrock).
#[must_use]
pub fn generated_tile(settings: &WorldSettings, position: TilePosition) -> Tile {
    generated_chunk(settings, ChunkPosition::of(position))
        .tile(ChunkPosition::local_index(position))
}

/// Columns `first_column..first_column + width` of the world, every row, as a
/// [`GeneratedWorld`] (previews, reports and tests). Its features are those whose centre lies
/// in the window.
#[must_use]
pub fn generate_window(settings: &WorldSettings, first_column: i32, width: i32) -> GeneratedWorld {
    let height = settings.height_tiles();
    let width = width.max(1);
    let mut tiles = Vec::with_capacity(usize::try_from(width * height).unwrap_or(0));
    for y in 0..height {
        for x in first_column..first_column + width {
            tiles.push(generated_tile(settings, TilePosition { x, y }));
        }
    }
    let rect = TileRect {
        min: TilePosition {
            x: first_column,
            y: 0,
        },
        max: TilePosition {
            x: first_column + width - 1,
            y: height - 1,
        },
    };
    GeneratedWorld {
        origin_x: first_column,
        width,
        height,
        tiles,
        features: FeatureIndex::from_features(features_in(settings, &rect, None)),
    }
}

/// Every feature (of `kinds`, or every kind) whose centre lies in `rect`, in id order.
#[must_use]
pub fn features_in(
    settings: &WorldSettings,
    rect: &TileRect,
    kinds: Option<&[FeatureKind]>,
) -> Vec<WorldFeature> {
    index::features_in(&ctx(settings), rect, kinds)
}

/// The points of interest whose bounds overlap `rect`, in id order.
#[must_use]
pub fn pois_touching(settings: &WorldSettings, rect: &TileRect) -> Vec<WorldFeature> {
    index::pois_touching(&ctx(settings), rect)
}

/// The feature with id `id` (`None` for ids no region of this world holds).
#[must_use]
pub fn feature(settings: &WorldSettings, id: FeatureId) -> Option<WorldFeature> {
    index::feature(&ctx(settings), id)
}

/// The feature of `kind` accepted by `accept` whose centre is nearest `from` (ties: lowest
/// id), searching outward at most `max_columns` columns either side.
pub fn nearest_feature(
    settings: &WorldSettings,
    from: TilePosition,
    kind: FeatureKind,
    max_columns: i32,
    accept: impl FnMut(&WorldFeature) -> bool,
) -> Option<WorldFeature> {
    index::nearest_feature(&ctx(settings), from, kind, max_columns, accept)
}

/// The cave-ecosystem sample at `position` (`None` outside every cave, and on generators
/// without cave ecosystems). See [`crate::cave`].
#[must_use]
pub fn cave_at(settings: &WorldSettings, position: TilePosition) -> Option<CaveSample> {
    if !settings.generator.has_cave_biomes() {
        return None;
    }
    let (region, cell) = cave_biomes::cave_cell_at(&ctx(settings), position.x, position.y)?;
    let biome = region.biome.as_ref()?;
    Some(CaveSample {
        kind: biome.kind,
        feature: biome.feature.id,
        interior: matches!(cell, BiomeCell::Open | BiomeCell::Water),
        water: cell == BiomeCell::Water,
    })
}

/// The caves whose footprints overlap `rect` (their rasters: open, water and wall cells).
#[must_use]
pub fn caves_touching(settings: &WorldSettings, rect: &TileRect) -> Vec<Arc<CaveRegion>> {
    cave_biomes::caves_in(&ctx(settings), rect)
}

/// Forgets every memoized region and chunk (tests).
#[cfg(test)]
pub(crate) fn clear_caches() {
    CHUNKS.clear();
    regions::clear_caches();
    cave_biomes::clear_caches();
}

/// The chunk rows a streamed world of `settings` has (its height in chunks, rounded up).
#[must_use]
pub const fn chunk_rows(settings: &WorldSettings) -> i32 {
    (settings.height_tiles() + CHUNK_SIZE_TILES - 1) / CHUNK_SIZE_TILES
}
