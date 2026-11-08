//! Per-player map discovery and scanner reveals.
//!
//! Each player's discovered tiles are a [`DiscoveredTiles`] set of chunk bitsets. Discovery only
//! ever grows, so it replicates as an op log ([`Discovery`] entries in
//! [`crate::TickOutput::discoveries`]) instead of resending whole sets: a reveal that finds
//! nothing new costs no allocation and no bandwidth.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    ids::PlayerId,
    player::Player,
    terrain::{CHUNK_SIZE_TILES, ChunkPosition, TILE_SIZE, Terrain, TilePosition},
};

use super::WorldState;

/// Half-width and half-height, in tiles, of the area a rig reveals around itself every tick.
const REVEAL_RADIUS_TILES: (i32, i32) = (4, 3);

/// 64-bit words per chunk bitset.
const CHUNK_WORDS: usize = (CHUNK_SIZE_TILES * CHUNK_SIZE_TILES) as usize / 64;

type ChunkBits = [u64; CHUNK_WORDS];

/// Tiles newly discovered by one player during one tick (op-log replication, sent only to that
/// player).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Discovery {
    pub player_id: PlayerId,
    pub tiles: Vec<TilePosition>,
}

/// The set of tiles one player has discovered.
///
/// Stored as [`CHUNK_SIZE_TILES`]-square chunk bitsets: membership and insertion are O(log
/// chunks) and allocation-free once a chunk exists. Human-readable formats (saves) serialize it as
/// a list of tile positions; binary formats (keyframes) as the chunk bitsets.
#[derive(Clone, Debug, Default)]
pub struct DiscoveredTiles {
    chunks: BTreeMap<ChunkPosition, ChunkBits>,
    count: usize,
    /// Tiles revealed by the simulation since the last [`Self::take_revealed`]; never serialized,
    /// hashed or compared.
    revealed: Vec<TilePosition>,
}

impl PartialEq for DiscoveredTiles {
    fn eq(&self, other: &Self) -> bool {
        self.count == other.count && self.chunks == other.chunks
    }
}

impl Eq for DiscoveredTiles {}

const fn bit(tile: TilePosition) -> (usize, u64) {
    let index = ChunkPosition::local_index(tile);
    (index / 64, 1 << (index % 64))
}

impl DiscoveredTiles {
    /// Whether `tile` is discovered.
    #[must_use]
    pub fn contains(&self, tile: TilePosition) -> bool {
        let (word, mask) = bit(tile);
        self.chunks
            .get(&ChunkPosition::of(tile))
            .is_some_and(|bits| bits[word] & mask != 0)
    }

    /// Marks `tile` discovered; returns whether it was new.
    pub fn insert(&mut self, tile: TilePosition) -> bool {
        let (word, mask) = bit(tile);
        let bits = self
            .chunks
            .entry(ChunkPosition::of(tile))
            .or_insert([0; CHUNK_WORDS]);
        if bits[word] & mask != 0 {
            return false;
        }
        bits[word] |= mask;
        self.count += 1;
        true
    }

    /// Number of discovered tiles.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.count
    }

    /// Whether nothing is discovered.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Every discovered tile, in deterministic (chunk, then row-major) order.
    pub fn iter(&self) -> impl Iterator<Item = TilePosition> + '_ {
        self.chunks
            .iter()
            .flat_map(|(chunk, bits)| chunk_tiles(*chunk, *bits))
    }

    /// Tiles discovered here but not in `older` (an earlier copy of this set).
    pub fn newly_discovered_since<'a>(
        &'a self,
        older: &'a Self,
    ) -> impl Iterator<Item = TilePosition> + 'a {
        self.chunks.iter().flat_map(move |(chunk, bits)| {
            let old = older.chunks.get(chunk).copied().unwrap_or([0; CHUNK_WORDS]);
            let mut fresh = [0; CHUNK_WORDS];
            for (index, word) in fresh.iter_mut().enumerate() {
                *word = bits[index] & !old[index];
            }
            chunk_tiles(*chunk, fresh)
        })
    }

    /// Inserts and journals a newly revealed tile for [`Self::take_revealed`].
    fn reveal(&mut self, tile: TilePosition) {
        if self.insert(tile) {
            self.revealed.push(tile);
        }
    }

    /// Tiles revealed since the previous call.
    fn take_revealed(&mut self) -> Vec<TilePosition> {
        std::mem::take(&mut self.revealed)
    }

    fn hash_into(&self, hasher: &mut crate::hash::StateHasher) {
        hasher.write_u64(u64::try_from(self.count).unwrap_or(u64::MAX));
        for (chunk, bits) in &self.chunks {
            hasher.write_i32(chunk.x);
            hasher.write_i32(chunk.y);
            for word in bits {
                hasher.write_u64(*word);
            }
        }
    }
}

fn chunk_tiles(chunk: ChunkPosition, bits: ChunkBits) -> impl Iterator<Item = TilePosition> {
    let origin = chunk.origin();
    bits.into_iter()
        .enumerate()
        .flat_map(move |(word, mut set)| {
            std::iter::from_fn(move || {
                if set == 0 {
                    return None;
                }
                let offset = set.trailing_zeros();
                set &= set - 1;
                let index =
                    i32::try_from(word * 64).unwrap_or(0) + i32::try_from(offset).unwrap_or(0);
                Some(TilePosition {
                    x: origin.x + index % CHUNK_SIZE_TILES,
                    y: origin.y + index / CHUNK_SIZE_TILES,
                })
            })
        })
}

impl FromIterator<TilePosition> for DiscoveredTiles {
    fn from_iter<I: IntoIterator<Item = TilePosition>>(iter: I) -> Self {
        let mut tiles = Self::default();
        for tile in iter {
            tiles.insert(tile);
        }
        tiles
    }
}

impl Serialize for DiscoveredTiles {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_seq(self.iter())
        } else {
            serializer.collect_seq(&self.chunks)
        }
    }
}

impl<'de> Deserialize<'de> for DiscoveredTiles {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            Ok(Vec::<TilePosition>::deserialize(deserializer)?
                .into_iter()
                .collect())
        } else {
            let chunks = BTreeMap::<ChunkPosition, ChunkBits>::deserialize(deserializer)?;
            let count = chunks
                .values()
                .flatten()
                .map(|word| word.count_ones() as usize)
                .sum();
            Ok(Self {
                chunks,
                count,
                revealed: Vec::new(),
            })
        }
    }
}

/// Reveals the clamped rectangle around a rig.
fn reveal_around(discovered: &mut DiscoveredTiles, terrain: &Terrain, player: &Player) {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "world positions are converted to terrain tile coordinates"
    )]
    let center = TilePosition {
        x: (player.x / TILE_SIZE).floor() as i32,
        y: (player.y / TILE_SIZE).floor() as i32,
    };
    let (radius_x, radius_y) = REVEAL_RADIUS_TILES;
    let x_range = (center.x - radius_x).max(0)..=(center.x + radius_x).min(terrain.width() - 1);
    for y in (center.y - radius_y).max(0)..=(center.y + radius_y).min(terrain.height() - 1) {
        for x in x_range.clone() {
            discovered.reveal(TilePosition { x, y });
        }
    }
}

impl WorldState {
    /// Reveals the diamond of `radius` tiles around `center` for a scanner pulse (clamped to the
    /// map). Returns how many tiles were new.
    pub(crate) fn reveal_scanner_area(
        &mut self,
        player_id: PlayerId,
        center: TilePosition,
        radius: i32,
    ) -> usize {
        let discovered = self.discovered_tiles.entry(player_id).or_default();
        let before = discovered.len();
        let radius = radius.max(0);
        let (width, height) = (self.terrain.width(), self.terrain.height());
        for y in (center.y - radius).max(0)..=(center.y + radius).min(height - 1) {
            for x in (center.x - radius).max(0)..=(center.x + radius).min(width - 1) {
                if (x - center.x).abs() + (y - center.y).abs() <= radius {
                    discovered.reveal(TilePosition { x, y });
                }
            }
        }
        discovered.len() - before
    }

    /// Reveals the area around every rig (once per tick). Allocation-free unless a tile is new.
    pub(super) fn reveal_tiles_near_players(&mut self) {
        let Self {
            players,
            discovered_tiles,
            terrain,
            ..
        } = self;
        for (player_id, player) in players.iter() {
            reveal_around(
                discovered_tiles.entry(*player_id).or_default(),
                terrain,
                player,
            );
        }
    }

    /// Drains this tick's reveals into op-log entries.
    pub(super) fn take_discoveries(&mut self) -> Vec<Discovery> {
        self.discovered_tiles
            .iter_mut()
            .filter(|(_, tiles)| !tiles.revealed.is_empty())
            .map(|(player_id, tiles)| Discovery {
                player_id: *player_id,
                tiles: tiles.take_revealed(),
            })
            .collect()
    }

    /// Applies one discovery op-log entry (a replica catching up with the authority).
    pub fn apply_discovery(&mut self, discovery: &Discovery) {
        if !self.players.contains_key(&discovery.player_id) {
            return;
        }
        let discovered = self
            .discovered_tiles
            .entry(discovery.player_id)
            .or_default();
        for tile in &discovery.tiles {
            discovered.insert(*tile);
        }
    }

    /// The tiles `player_id` has discovered.
    #[must_use]
    pub fn discovered_tiles(&self, player_id: PlayerId) -> Option<&DiscoveredTiles> {
        self.discovered_tiles.get(&player_id)
    }

    pub(super) fn hash_discovery(
        &self,
        player_id: PlayerId,
        hasher: &mut crate::hash::StateHasher,
    ) {
        if let Some(tiles) = self.discovered_tiles.get(&player_id) {
            hasher.write_u64(player_id.get());
            tiles.hash_into(hasher);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DiscoveredTiles, Discovery};
    use crate::{
        ControlState, PlayerId, SimulationTick, TickInputs, WorldState, terrain::TilePosition,
    };

    #[test]
    fn chunk_bitsets_behave_like_a_set() {
        let mut tiles = DiscoveredTiles::default();
        for position in [(0, 0), (15, 15), (16, 0), (-1, -1), (200, 37), (0, 0)] {
            let _ = tiles.insert(TilePosition {
                x: position.0,
                y: position.1,
            });
        }
        assert_eq!(tiles.len(), 5);
        assert!(tiles.contains(TilePosition { x: -1, y: -1 }));
        assert!(!tiles.contains(TilePosition { x: 1, y: 0 }));
        let listed: Vec<_> = tiles.iter().collect();
        assert_eq!(listed.len(), 5);
        assert!(listed.iter().all(|tile| tiles.contains(*tile)));
        let older: DiscoveredTiles = listed[..2].iter().copied().collect();
        assert_eq!(tiles.newly_discovered_since(&older).count(), 3);
    }

    #[test]
    fn both_serde_forms_round_trip() {
        let tiles: DiscoveredTiles = (0..40)
            .map(|index| TilePosition {
                x: index * 7 - 30,
                y: index * 3,
            })
            .collect();
        let json: DiscoveredTiles =
            serde_json::from_str(&serde_json::to_string(&tiles).expect("json")).expect("json");
        let binary: DiscoveredTiles =
            postcard::from_bytes(&postcard::to_stdvec(&tiles).expect("bin")).expect("bin");
        assert_eq!(json, tiles);
        assert_eq!(binary, tiles);
        assert_eq!(binary.len(), 40);
        // Saves keep the plain position list.
        let list: Vec<TilePosition> =
            serde_json::from_str(&serde_json::to_string(&tiles).expect("json")).expect("list");
        assert_eq!(list.len(), 40);
    }

    #[test]
    fn reveals_are_clamped_to_the_map_and_reported_once() {
        let player = PlayerId::new(1);
        let mut world = WorldState::new_game(player);
        let rig = world.player_mut(player).expect("rig");
        rig.x = 0.0;
        rig.y = 0.0;
        let tick = SimulationTick::new(0);
        let output = world.simulate_tick(tick, &TickInputs::default());
        let tiles = world.discovered_tiles(player).expect("discovery");
        assert!(!tiles.is_empty());
        assert!(tiles.iter().all(|tile| tile.x >= 0
            && tile.y >= 0
            && tile.x < world.terrain().width()
            && tile.y < world.terrain().height()));
        let reported: usize = output
            .discoveries
            .iter()
            .map(|discovery| discovery.tiles.len())
            .sum();
        assert_eq!(reported, tiles.len());

        // Standing still discovers nothing new and reports nothing.
        let output = world.simulate_tick(tick.next(), &TickInputs::default());
        assert!(output.discoveries.is_empty());

        // Moving on reports exactly the tiles that became discovered.
        let initial = tiles_of(&world, player);
        let mut tick = tick.next();
        for _ in 0..180 {
            tick = tick.next();
            let before = tiles_of(&world, player);
            let output = world.simulate_tick(
                tick,
                &TickInputs::held(player, ControlState::new(1.0, false, false)),
            );
            let reported: usize = output
                .discoveries
                .iter()
                .map(|discovery| discovery.tiles.len())
                .sum();
            assert_eq!(reported, tiles_of(&world, player) - before);
        }
        assert!(tiles_of(&world, player) > initial);
    }

    fn tiles_of(world: &WorldState, player: PlayerId) -> usize {
        world
            .discovered_tiles(player)
            .map_or(0, DiscoveredTiles::len)
    }

    #[test]
    fn a_replica_catches_up_from_discovery_ops() {
        let player = PlayerId::new(1);
        let mut authority = WorldState::new_game(player);
        let mut replica = authority.clone();
        for tick in 0..240 {
            let inputs = TickInputs::held(player, ControlState::new(1.0, tick % 60 < 20, false));
            let output = authority.simulate_tick(SimulationTick::new(tick), &inputs);
            for discovery in &output.discoveries {
                replica.apply_discovery(discovery);
            }
        }
        assert!(tiles_of(&authority, player) > 0);
        assert_eq!(
            authority.discovered_tiles(player),
            replica.discovered_tiles(player)
        );
        let unknown = Discovery {
            player_id: PlayerId::new(99),
            tiles: vec![TilePosition { x: 1, y: 1 }],
        };
        replica.apply_discovery(&unknown);
        assert!(replica.discovered_tiles(PlayerId::new(99)).is_none());
    }
}
