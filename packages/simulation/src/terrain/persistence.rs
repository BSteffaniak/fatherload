//! Compact terrain for saves and network keyframes (plan §4.6).
//!
//! A terrain is a pure function of its [`WorldSettings`] and dimensions plus whatever players
//! changed since. [`TerrainSnapshot`] stores exactly that: the settings, the size, a digest of
//! the full tiles and the tiles that differ from the freshly generated terrain
//! ([`TileDiffs`], row-major order). [`Terrain::from_snapshot`] regenerates the base terrain,
//! applies the diffs and checks the digest, so a snapshot from a different generator build is
//! rejected instead of silently loading different terrain.
//!
//! The generated base terrain is memoized per process ([`generated_base`]): generation is a pure
//! function of `(settings, width, height)`, and a host, its clients and tests rebuild the same
//! world many times. Chunk refreshes use the same base: [`Terrain::chunk_diffs`] sends only the
//! chunk's tiles that differ from generation.
//!
//! [`FullTerrain`] is the older full-tile form, kept for migrating saves written before v6.

mod packed;
#[cfg(test)]
mod tests;

pub use packed::PackedChunks;

use std::{
    fmt,
    sync::{Arc, Mutex, OnceLock, PoisonError},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use super::{
    ArtifactKind, CHUNK_SIZE_TILES, ChunkPosition, MineralKind, Slot, Storage, SurfaceMaterial,
    Terrain, Tile, TileChange, TileKind, TilePosition, legacy_tile_kind, position_digest,
    tile_durability,
};
use crate::{
    poi::PoiTile,
    worldgen::{self, FeatureIndex, GeneratorVersion, WorldSettings},
};

/// Largest accepted width or height of restored terrain, in tiles.
pub const MAX_TERRAIN_SIDE_TILES: i32 = 4_096;
/// Largest accepted tile count of restored terrain.
const MAX_TERRAIN_TILES: i64 = 1 << 22;
/// Distinct `(settings, width, height)` bases kept in memory.
const BASE_CACHE_CAPACITY: usize = 8;

/// The freshly generated terrain of one `(settings, width, height)`.
#[derive(Debug)]
pub(super) struct GeneratedBase {
    pub tiles: Vec<Tile>,
    pub features: FeatureIndex,
}

type BaseKey = (WorldSettings, i32, i32);
type BaseSlot = Arc<OnceLock<Arc<GeneratedBase>>>;

/// The generated base terrain for `settings` at `width` x `height`, memoized per process.
///
/// Concurrent callers asking for the same base wait for one generation instead of each running
/// it; different bases generate in parallel.
pub(super) fn generated_base(
    settings: &WorldSettings,
    width: i32,
    height: i32,
) -> Arc<GeneratedBase> {
    static CACHE: Mutex<Vec<(BaseKey, BaseSlot)>> = Mutex::new(Vec::new());
    let key = (*settings, width, height);
    let slot = {
        let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
        let slot = cache
            .iter()
            .position(|(cached, _)| *cached == key)
            .map_or_else(BaseSlot::default, |index| cache.remove(index).1);
        cache.push((key, Arc::clone(&slot)));
        if cache.len() > BASE_CACHE_CAPACITY {
            cache.remove(0);
        }
        slot
    };
    Arc::clone(slot.get_or_init(|| Arc::new(build_base(settings, width, height))))
}

/// Generates the base terrain.
///
/// Tiles inside the generator's own world come from it; tiles outside it (legacy worlds of any
/// size, or worlds extended past the generated depth) come from the legacy generator, exactly as
/// [`Terrain::new_seeded`] and [`Terrain::ensure_depth`] always built them.
fn build_base(settings: &WorldSettings, width: i32, height: i32) -> GeneratedBase {
    let generated =
        (settings.generator != GeneratorVersion::Legacy).then(|| worldgen::generate(settings));
    if generated
        .as_ref()
        .is_some_and(|world| world.width == width && world.height == height)
        && let Some(world) = generated
    {
        return GeneratedBase {
            tiles: world.tiles,
            features: world.features,
        };
    }
    let mut tiles =
        Vec::with_capacity(usize::try_from(i64::from(width) * i64::from(height)).unwrap_or(0));
    for y in 0..height {
        for x in 0..width {
            let kind = generated
                .as_ref()
                .and_then(|world| {
                    (x < world.width && y < world.height).then(|| {
                        usize::try_from(y * world.width + x)
                            .ok()
                            .and_then(|index| world.tiles.get(index))
                            .map_or(TileKind::Air, |tile| tile.kind)
                    })
                })
                .unwrap_or_else(|| legacy_tile_kind(x, y, settings.seed));
            tiles.push(Tile {
                kind,
                durability: tile_durability(kind),
            });
        }
    }
    GeneratedBase {
        tiles,
        features: generated.map(|world| world.features).unwrap_or_default(),
    }
}

/// Tiles that differ from generated terrain, by index, in ascending index order.
///
/// For a whole terrain the index is row-major (`y * width + x`); for a chunk
/// ([`Terrain::chunk_diffs`]) it is [`ChunkPosition::local_index`].
///
/// Serialized as runs of consecutive indices, each `(gap, [tile codes])` where `gap` counts the
/// unchanged tiles since the previous run and a code is [`Tile::code`]: a dug tunnel row is one
/// short run of zeros.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TileDiffs {
    entries: Vec<(u32, Tile)>,
}

impl TileDiffs {
    /// Diffs from `(index, tile)` pairs in any order. When an index repeats, the last pair wins.
    #[must_use]
    pub fn new(entries: impl IntoIterator<Item = (u32, Tile)>) -> Self {
        let mut entries: Vec<(u32, Tile)> = entries.into_iter().collect();
        // A stable sort keeps repeated indices in input order, so `dedup` below can keep the
        // last one.
        entries.sort_by_key(|(index, _)| *index);
        let mut deduped: Vec<(u32, Tile)> = Vec::with_capacity(entries.len());
        for entry in entries {
            match deduped.last_mut() {
                Some(last) if last.0 == entry.0 => *last = entry,
                _ => deduped.push(entry),
            }
        }
        Self { entries: deduped }
    }

    /// The `(index, tile)` pairs in ascending index order.
    #[must_use]
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (u32, Tile)> + ExactSizeIterator + '_ {
        self.entries.iter().copied()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn to_runs(&self) -> Vec<(u32, Vec<u64>)> {
        let mut runs: Vec<(u32, Vec<u64>)> = Vec::new();
        let mut next = 0_u32;
        for &(index, tile) in &self.entries {
            match runs.last_mut() {
                Some((_, codes)) if index == next && !codes.is_empty() => codes.push(tile.code()),
                _ => runs.push((index - next, vec![tile.code()])),
            }
            next = index + 1;
        }
        runs
    }

    fn from_runs(runs: Vec<(u32, Vec<u64>)>) -> Result<Self, String> {
        let mut entries = Vec::with_capacity(runs.iter().map(|(_, codes)| codes.len()).sum());
        let mut next = 0_u32;
        for (gap, codes) in runs {
            let mut index = next
                .checked_add(gap)
                .ok_or_else(|| "tile diff index overflows".to_owned())?;
            for code in codes {
                let tile =
                    Tile::from_code(code).ok_or_else(|| format!("invalid tile code {code:#x}"))?;
                entries.push((index, tile));
                index = index
                    .checked_add(1)
                    .ok_or_else(|| "tile diff index overflows".to_owned())?;
            }
            next = index;
        }
        Ok(Self { entries })
    }
}

impl Serialize for TileDiffs {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_runs().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TileDiffs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let runs = Vec::<(u32, Vec<u64>)>::deserialize(deserializer)?;
        Self::from_runs(runs).map_err(D::Error::custom)
    }
}

/// A terrain as settings, size, digest and the tiles that differ from generation.
///
/// This is how [`Terrain`] serializes (saves from v6 and every network keyframe).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerrainSnapshot {
    /// How the base terrain is generated (seed, size, richness, hazards, generator version).
    pub settings: WorldSettings,
    pub width: i32,
    pub height: i32,
    /// [`Terrain::digest`] of the full terrain, checked after the diffs are applied.
    pub digest: u64,
    /// Tiles that differ from the generated base, by row-major index (bounded terrain).
    pub diffs: TileDiffs,
    /// Streamed terrain as saves before format 13 stored it: every chunk with edits and its
    /// tiles that differ from generation, by [`ChunkPosition::local_index`]. Still read; new
    /// snapshots write [`Self::packed`] instead and leave this empty.
    #[serde(default)]
    pub chunks: Vec<(ChunkPosition, TileDiffs)>,
    /// The world's local pixel frame (F7, [`crate::position::Frame`]); missing in older saves,
    /// whose positions are world pixels: the fixed frame at the origin.
    #[serde(default)]
    pub frame: crate::position::Frame,
    /// A raised column limit of a far test or tool world
    /// ([`Terrain::set_column_limit_for_tests`]); `None` for every real world.
    #[serde(default)]
    pub column_limit: Option<i32>,
    /// Streamed terrain: every chunk with edits and its diffs, compressed ([`PackedChunks`]:
    /// runs of equal tiles in one byte stream, base64 text in JSON). Empty for bounded terrain
    /// (and missing in older saves).
    #[serde(default)]
    pub packed: PackedChunks,
}

/// Why a [`TerrainSnapshot`] could not be restored.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerrainRestoreError {
    /// The dimensions are not positive or too large.
    InvalidSize { width: i32, height: i32 },
    /// A diff points outside the terrain.
    DiffOutOfBounds { index: u64 },
    /// A full-tile terrain does not hold exactly one tile per position.
    TileCountMismatch { expected: u64, actual: u64 },
    /// The rebuilt terrain does not match the digest: the snapshot was made by a generator that
    /// produces different terrain for the same settings, or it is corrupted.
    DigestMismatch {
        generator: GeneratorVersion,
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for TerrainRestoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize { width, height } => {
                write!(formatter, "terrain size {width}x{height} is out of range")
            }
            Self::DiffOutOfBounds { index } => {
                write!(
                    formatter,
                    "terrain tile diff {index} is outside the terrain"
                )
            }
            Self::TileCountMismatch { expected, actual } => write!(
                formatter,
                "terrain holds {actual} tiles but its size needs {expected}"
            ),
            Self::DigestMismatch {
                generator,
                expected,
                actual,
            } => write!(
                formatter,
                "terrain does not match world generator {}: digest {expected:#018x} expected, \
                 {actual:#018x} rebuilt (made by a different generator build, or corrupted)",
                generator.name()
            ),
        }
    }
}

impl std::error::Error for TerrainRestoreError {}

/// The full-tile terrain form saves used before v6 (every tile written out).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FullTerrain {
    pub width: i32,
    pub height: i32,
    pub seed: u64,
    pub tiles: Vec<Tile>,
    /// Missing in saves written before seeded generation: those are legacy worlds.
    #[serde(default)]
    pub settings: Option<WorldSettings>,
}

impl TryFrom<FullTerrain> for Terrain {
    type Error = TerrainRestoreError;

    /// Checks the size and that there is exactly one tile per position (a corrupted old save
    /// is an error, not a terrain whose tile count disagrees with its size).
    fn try_from(full: FullTerrain) -> Result<Self, Self::Error> {
        let FullTerrain {
            width,
            height,
            seed,
            tiles,
            settings,
        } = full;
        validate_size(width, height)?;
        let expected = usize::try_from(i64::from(width) * i64::from(height)).unwrap_or(usize::MAX);
        if tiles.len() != expected {
            return Err(TerrainRestoreError::TileCountMismatch {
                expected: u64::try_from(expected).unwrap_or(u64::MAX),
                actual: u64::try_from(tiles.len()).unwrap_or(u64::MAX),
            });
        }
        Ok(Self::from_tiles(
            settings.unwrap_or_else(|| WorldSettings::legacy(seed)),
            width,
            height,
            tiles,
        ))
    }
}

impl From<&Terrain> for FullTerrain {
    /// The full-tile form of a bounded terrain (a streamed terrain has no finite grid: it
    /// converts to no tiles).
    fn from(terrain: &Terrain) -> Self {
        let tiles = terrain
            .bounded_tiles()
            .map(<[Tile]>::to_vec)
            .unwrap_or_default();
        Self {
            width: if terrain.is_streamed() {
                0
            } else {
                terrain.width()
            },
            height: terrain.height,
            seed: terrain.settings.seed,
            tiles,
            settings: Some(terrain.settings),
        }
    }
}

impl Serialize for Terrain {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.snapshot().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Terrain {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let snapshot = TerrainSnapshot::deserialize(deserializer)?;
        Self::from_snapshot(snapshot).map_err(D::Error::custom)
    }
}

const fn validate_size(width: i32, height: i32) -> Result<(), TerrainRestoreError> {
    if width <= 0
        || height <= 0
        || width > MAX_TERRAIN_SIDE_TILES
        || height > MAX_TERRAIN_SIDE_TILES
        || (width as i64) * (height as i64) > MAX_TERRAIN_TILES
    {
        return Err(TerrainRestoreError::InvalidSize { width, height });
    }
    Ok(())
}

/// Chunks whose generated tiles fingerprint a streamed world's generator build (see
/// [`streamed_generator_probe`]): the town, both sides of it, and the deep strata.
const STREAMED_PROBE_CHUNKS: [ChunkPosition; 5] = [
    ChunkPosition { x: 0, y: 0 },
    ChunkPosition { x: 6, y: 1 },
    ChunkPosition { x: -9, y: 3 },
    ChunkPosition { x: 23, y: 12 },
    ChunkPosition { x: -4, y: 40 },
];

/// A fingerprint of the generated tiles of a few fixed chunks. A streamed snapshot stores no
/// generated tiles, so its digest folds this in: restoring it under a generator build that
/// produces different terrain for the same settings fails instead of silently loading
/// different terrain.
fn streamed_generator_probe(settings: &WorldSettings) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for chunk in STREAMED_PROBE_CHUNKS {
        for tile in worldgen::streamed::generated_chunk(settings, chunk).tiles() {
            for byte in tile.code().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01B3);
            }
        }
    }
    // A sky world (V8) also fingerprints its sky: a chunk of low isles and the Moon's top.
    if let Some((moon, bands)) = worldgen::sky::moon(settings).zip(settings.generator.sky_bands()) {
        let sky_chunks = [
            ChunkPosition::of(crate::terrain::TilePosition {
                x: 5 * CHUNK_SIZE_TILES,
                y: worldgen::sky::row_of(bands.low_isles + 150),
            }),
            ChunkPosition::of(crate::terrain::TilePosition {
                x: moon.center_x,
                y: moon.top_row,
            }),
        ];
        for chunk in sky_chunks {
            for tile in worldgen::streamed::generated_chunk(settings, chunk).tiles() {
                for byte in tile.code().to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(0x0100_0000_01B3);
                }
            }
        }
    }
    hash
}

impl Terrain {
    /// The freshly generated terrain of `settings` at `width` x `height` (no player changes).
    /// Streamed settings ignore the size (the world has no width and its own height).
    ///
    /// # Errors
    ///
    /// Returns an error if the size is not positive or too large.
    pub fn regenerate(
        settings: &WorldSettings,
        width: i32,
        height: i32,
    ) -> Result<Self, TerrainRestoreError> {
        if settings.generator.is_streamed() {
            return Ok(Self::streamed(*settings));
        }
        validate_size(width, height)?;
        Ok(Self::from_base(
            *settings,
            width,
            height,
            generated_base(settings, width, height),
        ))
    }

    /// The compact form: settings, size, digest and the tiles that differ from generation.
    #[must_use]
    pub fn snapshot(&self) -> TerrainSnapshot {
        match &self.storage {
            Storage::Bounded(bounded) => {
                let base = self.base();
                TerrainSnapshot {
                    settings: self.settings,
                    width: bounded.width,
                    height: self.height,
                    digest: self.digest,
                    diffs: TileDiffs {
                        entries: bounded
                            .tiles
                            .iter()
                            .zip(&base.tiles)
                            .enumerate()
                            .filter(|(_, (tile, generated))| tile != generated)
                            .map(|(index, (tile, _))| {
                                (u32::try_from(index).unwrap_or(u32::MAX), *tile)
                            })
                            .collect(),
                    },
                    chunks: Vec::new(),
                    frame: self.frame,
                    column_limit: self.column_limit_override,
                    packed: PackedChunks::default(),
                }
            }
            Storage::Streamed(streamed) => TerrainSnapshot {
                settings: self.settings,
                width: 0,
                height: self.height,
                digest: self
                    .digest
                    .wrapping_add(streamed_generator_probe(&self.settings)),
                diffs: TileDiffs::default(),
                chunks: Vec::new(),
                packed: PackedChunks(
                    streamed
                        .edits
                        .iter()
                        .map(|(chunk, edits)| {
                            (
                                *chunk,
                                TileDiffs {
                                    entries: edits
                                        .iter()
                                        .map(|(local, tile)| (u32::from(*local), *tile))
                                        .collect(),
                                },
                            )
                        })
                        .collect(),
                ),
                frame: self.frame,
                column_limit: self.column_limit_override,
            },
        }
    }

    /// Rebuilds a terrain from its [`Self::snapshot`]: regenerates the base, applies the diffs
    /// and checks the digest.
    ///
    /// # Errors
    ///
    /// Returns an error if the size is out of range, a diff lies outside the terrain, or the
    /// rebuilt terrain does not match the snapshot's digest.
    pub fn from_snapshot(snapshot: TerrainSnapshot) -> Result<Self, TerrainRestoreError> {
        let TerrainSnapshot {
            settings,
            width,
            height,
            digest,
            diffs,
            mut chunks,
            packed,
            frame,
            column_limit,
        } = snapshot;
        if settings.generator.is_streamed() {
            chunks.extend(packed.0);
            let mut terrain =
                Self::from_streamed_snapshot(settings, height, digest, chunks, column_limit)?;
            terrain.frame = frame;
            return Ok(terrain);
        }
        let mut terrain = Self::regenerate(&settings, width, height)?;
        let tile_count = terrain.stored_tile_count();
        for (index, tile) in diffs.iter() {
            let slot = usize::try_from(index)
                .ok()
                .filter(|slot| *slot < tile_count)
                .ok_or_else(|| TerrainRestoreError::DiffOutOfBounds {
                    index: u64::from(index),
                })?;
            let position = terrain.position_of(slot);
            terrain.write_tile(Slot::Bounded(slot), position, tile);
        }
        if terrain.digest != digest {
            return Err(TerrainRestoreError::DigestMismatch {
                generator: settings.generator,
                expected: digest,
                actual: terrain.digest,
            });
        }
        terrain.frame = frame;
        Ok(terrain)
    }

    /// [`Self::from_snapshot`] for a streamed world: the edits are restored as they are (no
    /// chunk is generated until it is read), then checked against the digest.
    fn from_streamed_snapshot(
        settings: WorldSettings,
        height: i32,
        digest: u64,
        chunks: Vec<(ChunkPosition, TileDiffs)>,
        column_limit: Option<i32>,
    ) -> Result<Self, TerrainRestoreError> {
        if height != settings.height_tiles() {
            return Err(TerrainRestoreError::InvalidSize { width: 0, height });
        }
        let mut terrain = Self::streamed(settings);
        if let Some(limit) = column_limit {
            terrain.set_column_limit_for_tests(limit);
        }
        let limit = terrain.column_limit();
        let Storage::Streamed(streamed) = &mut terrain.storage else {
            return Err(TerrainRestoreError::InvalidSize { width: 0, height });
        };
        let mut sum = 0_u64;
        for (chunk, diffs) in chunks {
            let origin = chunk.origin();
            let top = worldgen::streamed::top_row(&settings).div_euclid(CHUNK_SIZE_TILES)
                * CHUNK_SIZE_TILES;
            if origin.x < -limit || origin.x >= limit || origin.y < top || origin.y >= height {
                return Err(TerrainRestoreError::DiffOutOfBounds {
                    index: u64::from(chunk.x.unsigned_abs()),
                });
            }
            let edits = streamed.edits.entry(chunk).or_default();
            for (local, tile) in diffs.iter() {
                let local_index = u16::try_from(local)
                    .ok()
                    .filter(|local| usize::from(*local) < worldgen::streamed::CHUNK_TILES)
                    .ok_or_else(|| TerrainRestoreError::DiffOutOfBounds {
                        index: u64::from(local),
                    })?;
                let position = TilePosition {
                    x: origin.x + i32::from(local_index) % CHUNK_SIZE_TILES,
                    y: origin.y + i32::from(local_index) / CHUNK_SIZE_TILES,
                };
                if position.y >= height {
                    return Err(TerrainRestoreError::DiffOutOfBounds {
                        index: u64::from(local),
                    });
                }
                if let Some(previous) = edits.insert(local_index, tile) {
                    sum = sum.wrapping_sub(position_digest(position, previous));
                }
                sum = sum.wrapping_add(position_digest(position, tile));
            }
            if edits.is_empty() {
                streamed.edits.remove(&chunk);
            }
        }
        terrain.digest = sum;
        let actual = sum.wrapping_add(streamed_generator_probe(&settings));
        if actual != digest {
            return Err(TerrainRestoreError::DigestMismatch {
                generator: settings.generator,
                expected: digest,
                actual,
            });
        }
        Ok(terrain)
    }

    /// Every tile that differs from the freshly generated terrain: row-major for bounded
    /// terrain, chunk by chunk for streamed terrain.
    #[must_use]
    pub fn generation_diffs(&self) -> Vec<TileChange> {
        match &self.storage {
            Storage::Bounded(_) => self
                .snapshot()
                .diffs
                .iter()
                .filter_map(|(index, tile)| {
                    usize::try_from(index).ok().map(|slot| TileChange {
                        position: self.position_of(slot),
                        tile,
                    })
                })
                .collect(),
            Storage::Streamed(streamed) => streamed
                .edits
                .iter()
                .flat_map(|(chunk, edits)| {
                    let origin = chunk.origin();
                    edits.iter().map(move |(local, tile)| TileChange {
                        position: TilePosition {
                            x: origin.x + i32::from(*local) % CHUNK_SIZE_TILES,
                            y: origin.y + i32::from(*local) / CHUNK_SIZE_TILES,
                        },
                        tile: *tile,
                    })
                })
                .collect(),
        }
    }

    /// Regenerates terrain from settings and applies `diffs` (as [`Self::generation_diffs`]
    /// returns them; any order, since each names a distinct tile).
    ///
    /// # Errors
    ///
    /// Returns an error if the size is out of range or a diff lies outside the terrain.
    pub fn from_generation_diffs(
        settings: &WorldSettings,
        width: i32,
        height: i32,
        diffs: &[TileChange],
    ) -> Result<Self, TerrainRestoreError> {
        let mut terrain = Self::regenerate(settings, width, height)?;
        for change in diffs {
            if !terrain.apply_tile_change(change) {
                let TilePosition { x, y } = change.position;
                return Err(TerrainRestoreError::DiffOutOfBounds {
                    index: u64::try_from(i64::from(y) * i64::from(width) + i64::from(x))
                        .unwrap_or(u64::MAX),
                });
            }
        }
        Ok(terrain)
    }

    /// The freshly generated tile at an in-world `slot`.
    fn generated(&self, slot: Slot) -> Tile {
        match slot {
            Slot::Bounded(index) => self.base().tiles[index],
            Slot::Streamed(chunk, local) => match &self.storage {
                Storage::Streamed(streamed) => streamed.loaded.get(&chunk).map_or_else(
                    || worldgen::streamed::generated_chunk(&self.settings, chunk).tile(local),
                    |loaded| loaded.base.tile(local),
                ),
                Storage::Bounded(_) => Tile {
                    kind: TileKind::Bedrock,
                    durability: 0,
                },
            },
        }
    }

    /// The tiles of `chunk` that differ from generation, by [`ChunkPosition::local_index`]
    /// (refreshing a client that skipped the chunk's op log).
    #[must_use]
    pub fn chunk_diffs(&self, chunk: ChunkPosition) -> TileDiffs {
        if let Storage::Streamed(streamed) = &self.storage {
            return TileDiffs {
                entries: streamed
                    .edits
                    .get(&chunk)
                    .map(|edits| {
                        edits
                            .iter()
                            .map(|(local, tile)| (u32::from(*local), *tile))
                            .collect()
                    })
                    .unwrap_or_default(),
            };
        }
        TileDiffs::new(self.chunk_positions(chunk).filter_map(|position| {
            let slot = self.slot(position)?;
            let tile = self.read(slot);
            (tile != self.generated(slot)).then(|| {
                (
                    u32::try_from(ChunkPosition::local_index(position)).unwrap_or(u32::MAX),
                    tile,
                )
            })
        }))
    }

    /// Overwrites `chunk` with its generated tiles plus a [`Self::chunk_diffs`] copy. Diffs
    /// outside the chunk are ignored.
    pub fn apply_chunk_diffs(&mut self, chunk: ChunkPosition, diffs: &TileDiffs) {
        if let Storage::Streamed(streamed) = &self.storage
            && diffs.is_empty()
            && !streamed.edits.contains_key(&chunk)
        {
            // Nothing edited here and nothing to apply: the chunk is already generation.
            return;
        }
        let mut pending = diffs.iter().peekable();
        let mut positions: Vec<TilePosition> = self.chunk_positions(chunk).collect();
        positions.sort_by_key(|position| ChunkPosition::local_index(*position));
        for position in positions {
            let Some(slot) = self.slot(position) else {
                continue;
            };
            let local = u32::try_from(ChunkPosition::local_index(position)).unwrap_or(u32::MAX);
            while pending.next_if(|(diff, _)| *diff < local).is_some() {}
            let tile = pending
                .next_if(|(diff, _)| *diff == local)
                .map_or_else(|| self.generated(slot), |(_, tile)| tile);
            if self.read(slot) != tile {
                self.write_tile(slot, position, tile);
            }
        }
    }

    /// The generated base terrain of this bounded terrain's settings and size.
    pub(super) fn base(&self) -> &Arc<GeneratedBase> {
        static EMPTY: OnceLock<Arc<GeneratedBase>> = OnceLock::new();
        match &self.storage {
            Storage::Bounded(bounded) => bounded
                .base
                .get_or_init(|| generated_base(&self.settings, bounded.width, self.height)),
            Storage::Streamed(_) => EMPTY.get_or_init(|| {
                Arc::new(GeneratedBase {
                    tiles: Vec::new(),
                    features: FeatureIndex::default(),
                })
            }),
        }
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "slots index a validated terrain"
    )]
    fn position_of(&self, slot: usize) -> TilePosition {
        let width = match &self.storage {
            Storage::Bounded(bounded) => bounded.width.max(1),
            Storage::Streamed(_) => 1,
        };
        let slot = slot as i32;
        TilePosition {
            x: slot % width,
            y: slot / width,
        }
    }
}

impl Tile {
    /// Inverse of [`Tile::code`], or `None` for a code no tile has.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm per tile family")]
    pub const fn from_code(code: u64) -> Option<Self> {
        const MINERALS: [MineralKind; 12] = [
            MineralKind::Copper,
            MineralKind::Iron,
            MineralKind::Silver,
            MineralKind::Gold,
            MineralKind::Emerald,
            MineralKind::Ruby,
            MineralKind::Diamond,
            MineralKind::Platinum,
            MineralKind::Uranium,
            MineralKind::Mythril,
            MineralKind::Skystone,
            MineralKind::CloudSilver,
        ];
        const ARTIFACTS: [ArtifactKind; 18] = [
            ArtifactKind::Fossil,
            ArtifactKind::OldCircuit,
            ArtifactKind::BuriedIdol,
            ArtifactKind::StarCore,
            ArtifactKind::CrystalPrism,
            ArtifactKind::RadiantShell,
            ArtifactKind::PressureGlyph,
            ArtifactKind::ReactorBloom,
            ArtifactKind::MachineSigil,
            ArtifactKind::VoidPearl,
            ArtifactKind::MantleHeart,
            ArtifactKind::WindAstrolabe,
            ArtifactKind::CloudPearl,
            ArtifactKind::HarpyEgg,
            ArtifactKind::SkyRelic,
            ArtifactKind::LunarRelic,
            ArtifactKind::WardenEye,
            ArtifactKind::ClayPot,
        ];
        #[allow(clippy::cast_possible_truncation, reason = "masked to a byte")]
        let durability = (code & 0xFF) as u8;
        let kind_code = code >> 8;
        let variant = (kind_code & 0xFF) as usize;
        let kind = match kind_code {
            0 => TileKind::Air,
            1 => TileKind::Dirt,
            2 => TileKind::Clay,
            3 => TileKind::Stone,
            4 => TileKind::HardRock,
            5 => TileKind::Foundation,
            6 => TileKind::Lava,
            7 => TileKind::Gas,
            8 => TileKind::ExplosivePocket,
            9 => TileKind::PressurePocket,
            10 => TileKind::MagmaVent,
            11 => TileKind::Bedrock,
            0x100..=0x1FF if variant < MINERALS.len() => TileKind::Ore(MINERALS[variant]),
            0x200..=0x2FF if variant < ARTIFACTS.len() => TileKind::Artifact(ARTIFACTS[variant]),
            0x300..=0x3FF => match PoiTile::from_code(variant as u64) {
                Some(tile) => TileKind::Poi(tile),
                None => return None,
            },
            0x400..=0x4FF => match crate::cave::CaveTile::from_code(variant as u64) {
                Some(tile) => TileKind::Cave(tile),
                None => return None,
            },
            0xB00..=0xBFF => match crate::base::BuiltTile::from_code(variant as u64) {
                Some(tile) => TileKind::Built(tile),
                None => return None,
            },
            0x500..=0x5FF if variant < SurfaceMaterial::ALL.len() => {
                TileKind::Surface(SurfaceMaterial::ALL[variant])
            }
            0x600..=0x6FF => match crate::sky::SkyTile::from_code(variant as u64) {
                Some(tile) => TileKind::Sky(tile),
                None => return None,
            },
            0x700..=0x7FF => match crate::moon::LunarTile::from_code(variant as u64) {
                Some(tile) => TileKind::Lunar(tile),
                None => return None,
            },
            0xC00..=0xCFF => match crate::crust::CrustTile::from_code(variant as u64) {
                Some(tile) => TileKind::Crust(tile),
                None => return None,
            },
            0xA00..=0xAFF => match crate::fluid::FluidKind::from_code(variant as u64) {
                Some(fluid) => TileKind::Fluid(fluid),
                None => return None,
            },
            0xD00..=0xDFF => match crate::old_deep::DeepTile::from_code(variant as u64) {
                Some(tile) => TileKind::Deep(tile),
                None => return None,
            },
            0x900..=0x9FF => match crate::hollow::HollowTile::from_code(variant as u64) {
                Some(tile) => TileKind::Hollow(tile),
                None => return None,
            },
            0x1600..=0x16FF => match crate::distortion::WarpedTile::from_code(variant as u64) {
                Some(tile) => TileKind::Warped(tile),
                None => return None,
            },
            0x1C00..=0x1CFF => match crate::ancient_city::CityTile::from_code(variant as u64) {
                Some(tile) => TileKind::City(tile),
                None => return None,
            },
            0x1A00..=0x1AFF => match crate::heart::CoreTile::from_code(variant as u64) {
                Some(tile) => TileKind::Core(tile),
                None => return None,
            },
            _ => return None,
        };
        Some(Self { kind, durability })
    }
}
