//! Compressed storage of a streamed world's modified chunks (Wave 7 F8 streaming budget).
//!
//! A Deep Descent world is about fifteen times deeper than a V10 world, so a long game edits
//! many more chunks. [`PackedChunks`] stores every edited chunk's diffs in one compact byte
//! stream instead of nested lists of tile codes:
//!
//! * chunks in order, each as the zigzag-varint step from the previous chunk's position;
//! * per chunk, runs of **equal** tiles: `(gap, length, code)` varints, where `gap` counts the
//!   unchanged tiles since the previous run (and code 0 repeats the previous run's tile), in
//!   row-major or column-major order, whichever is shorter: a tunnel dug across a chunk is
//!   one run, and so is a shaft dug straight down it.
//!
//! Human-readable formats (the JSON saves) carry the stream as base64 text, binary formats (the
//! network keyframes) as raw bytes. A tunnel 13,000 rows deep packs into a few kilobytes.

use std::fmt;

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};

use super::{ChunkPosition, Tile, TileDiffs};

/// Format byte that starts every packed stream.
const FORMAT: u8 = 1;
/// Most chunks or runs a packed stream may declare (a corrupt length fails fast instead of
/// allocating).
const MAX_ITEMS: u64 = 1 << 24;

/// Every edited chunk of a streamed world and its diffs, in chunk order, stored compressed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PackedChunks(pub Vec<(ChunkPosition, TileDiffs)>);

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = u8::try_from(value & 0x7F).unwrap_or(0);
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

const fn zigzag(value: i32) -> u64 {
    ((value << 1) ^ (value >> 31)) as u32 as u64
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "zigzag decoding of a 32-bit value"
)]
const fn unzigzag(value: u64) -> i32 {
    let value = value as u32;
    ((value >> 1) as i32) ^ -((value & 1) as i32)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn varint(&mut self) -> Result<u64, String> {
        let mut value = 0_u64;
        for shift in (0..64).step_by(7) {
            let byte = *self
                .bytes
                .get(self.at)
                .ok_or_else(|| "packed chunks end early".to_owned())?;
            self.at += 1;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("packed chunks hold an overlong number".to_owned())
    }

    fn count(&mut self) -> Result<usize, String> {
        let count = self.varint()?;
        if count > MAX_ITEMS {
            return Err(format!("packed chunks declare {count} items"));
        }
        usize::try_from(count).map_err(|_| "packed chunk count overflows".to_owned())
    }
}

impl PackedChunks {
    /// The packed byte stream.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![FORMAT];
        put_varint(&mut out, self.0.len() as u64);
        let mut previous = ChunkPosition { x: 0, y: 0 };
        for (chunk, diffs) in &self.0 {
            put_varint(&mut out, zigzag(chunk.x.wrapping_sub(previous.x)));
            put_varint(&mut out, zigzag(chunk.y.wrapping_sub(previous.y)));
            previous = *chunk;
            let rows: Vec<(u32, u64)> = diffs
                .iter()
                .map(|(index, tile)| (index, tile.code()))
                .collect();
            let row_runs = encode_runs(&rows);
            // A shaft dug straight down packs best column by column.
            if rows.iter().all(|(index, _)| *index < CHUNK_AREA) {
                let mut columns: Vec<(u32, u64)> = rows
                    .iter()
                    .map(|(index, code)| (transpose(*index), *code))
                    .collect();
                columns.sort_unstable_by_key(|(index, _)| *index);
                let column_runs = encode_runs(&columns);
                if column_runs.len() < row_runs.len() {
                    out.push(MODE_COLUMNS);
                    out.extend(column_runs);
                    continue;
                }
            }
            out.push(MODE_ROWS);
            out.extend(row_runs);
        }
        out
    }

    /// Unpacks a byte stream written by [`Self::to_bytes`].
    ///
    /// # Errors
    ///
    /// Returns a message for an unknown format, a truncated stream, an invalid tile code or an
    /// index that overflows.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() {
            return Ok(Self::default());
        }
        if bytes[0] != FORMAT {
            return Err(format!("unknown packed chunk format {}", bytes[0]));
        }
        let mut reader = Reader { bytes, at: 1 };
        let chunk_count = reader.count()?;
        let mut chunks = Vec::with_capacity(chunk_count.min(4_096));
        let mut previous = ChunkPosition { x: 0, y: 0 };
        for _ in 0..chunk_count {
            let dx = unzigzag(reader.varint()?);
            let dy = unzigzag(reader.varint()?);
            let chunk = ChunkPosition {
                x: previous.x.wrapping_add(dx),
                y: previous.y.wrapping_add(dy),
            };
            previous = chunk;
            let mode = reader.varint()?;
            let mut entries = decode_runs(&mut reader)?;
            match u8::try_from(mode) {
                Ok(MODE_ROWS) => {}
                Ok(MODE_COLUMNS) => {
                    if entries.iter().any(|(index, _)| *index >= CHUNK_AREA) {
                        return Err("a column run leaves its chunk".to_owned());
                    }
                    for entry in &mut entries {
                        entry.0 = transpose(entry.0);
                    }
                    entries.sort_unstable_by_key(|(index, _)| *index);
                }
                _ => return Err(format!("unknown packed chunk mode {mode}")),
            }
            chunks.push((chunk, TileDiffs { entries }));
        }
        if reader.at != bytes.len() {
            return Err("packed chunks carry trailing bytes".to_owned());
        }
        Ok(Self(chunks))
    }
}

/// Tiles in a chunk.
const CHUNK_AREA: u32 = super::super::CHUNK_SIZE_TILES.unsigned_abs().pow(2);
/// A chunk's runs follow row-major tile order.
const MODE_ROWS: u8 = 0;
/// A chunk's runs follow column-major tile order.
const MODE_COLUMNS: u8 = 1;

/// Swaps a chunk-local index between row-major and column-major order (self-inverse).
const fn transpose(index: u32) -> u32 {
    let side = super::super::CHUNK_SIZE_TILES.unsigned_abs();
    (index % side) * side + index / side
}

/// `(index, code)` pairs in ascending index order as runs of equal codes: a run count, then
/// per run its gap, length and code (0 for the previous run's code, else the code plus one).
fn encode_runs(entries: &[(u32, u64)]) -> Vec<u8> {
    let mut runs: Vec<(u32, u32, u64)> = Vec::new();
    let mut next = 0_u32;
    for &(index, code) in entries {
        match runs.last_mut() {
            Some((_, length, last)) if index == next && *last == code => *length += 1,
            _ => runs.push((index - next, 1, code)),
        }
        next = index + 1;
    }
    let mut out = Vec::new();
    put_varint(&mut out, runs.len() as u64);
    let mut previous = None;
    for (gap, length, code) in runs {
        put_varint(&mut out, u64::from(gap));
        put_varint(&mut out, u64::from(length));
        if previous == Some(code) {
            put_varint(&mut out, 0);
        } else {
            put_varint(&mut out, code.saturating_add(1));
        }
        previous = Some(code);
    }
    out
}

/// The `(index, tile)` pairs of runs written by [`encode_runs`].
fn decode_runs(reader: &mut Reader<'_>) -> Result<Vec<(u32, Tile)>, String> {
    let run_count = reader.count()?;
    let mut entries = Vec::new();
    let mut next = 0_u32;
    let mut previous: Option<u64> = None;
    for _ in 0..run_count {
        let gap = u32::try_from(reader.varint()?).map_err(|_| "gap overflows")?;
        let length = u32::try_from(reader.varint()?).map_err(|_| "run overflows")?;
        let code = match reader.varint()? {
            0 => previous.ok_or_else(|| "a run repeats a code before any".to_owned())?,
            code => code - 1,
        };
        previous = Some(code);
        let tile = Tile::from_code(code).ok_or_else(|| format!("invalid tile code {code:#x}"))?;
        let mut index = next
            .checked_add(gap)
            .ok_or_else(|| "tile diff index overflows".to_owned())?;
        if length > CHUNK_AREA {
            return Err(format!("a run of {length} tiles is longer than a chunk"));
        }
        for _ in 0..length {
            entries.push((index, tile));
            index = index
                .checked_add(1)
                .ok_or_else(|| "tile diff index overflows".to_owned())?;
        }
        next = index;
    }
    Ok(entries)
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_base64(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let value = (u32::from(group[0]) << 16)
            | (u32::from(*group.get(1).unwrap_or(&0)) << 8)
            | u32::from(*group.get(2).unwrap_or(&0));
        for (position, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if position <= group.len() {
                text.push(char::from(BASE64[((value >> shift) & 0x3F) as usize]));
            } else {
                text.push('=');
            }
        }
    }
    text
}

fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
    let digit = |byte: u8| -> Result<u32, String> {
        BASE64
            .iter()
            .position(|each| *each == byte)
            .and_then(|position| u32::try_from(position).ok())
            .ok_or_else(|| format!("invalid base64 character {:?}", char::from(byte)))
    };
    let trimmed = text.trim_end_matches('=');
    let mut out = Vec::with_capacity(trimmed.len() * 3 / 4);
    for group in trimmed.as_bytes().chunks(4) {
        if group.len() < 2 {
            return Err("truncated base64".to_owned());
        }
        let mut value = 0_u32;
        for (position, byte) in group.iter().enumerate() {
            value |= digit(*byte)? << (18 - 6 * position);
        }
        out.push(u8::try_from(value >> 16).unwrap_or(0));
        if group.len() > 2 {
            out.push(u8::try_from((value >> 8) & 0xFF).unwrap_or(0));
        }
        if group.len() > 3 {
            out.push(u8::try_from(value & 0xFF).unwrap_or(0));
        }
    }
    Ok(out)
}

impl Serialize for PackedChunks {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let bytes = self.to_bytes();
        if serializer.is_human_readable() {
            serializer.serialize_str(&encode_base64(&bytes))
        } else {
            serializer.serialize_bytes(&bytes)
        }
    }
}

struct PackedVisitor;

impl<'de> Visitor<'de> for PackedVisitor {
    type Value = PackedChunks;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("packed chunk bytes or their base64 text")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Self::Value, E> {
        let bytes = decode_base64(text).map_err(E::custom)?;
        PackedChunks::from_bytes(&bytes).map_err(E::custom)
    }

    fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
        PackedChunks::from_bytes(bytes).map_err(E::custom)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut bytes = Vec::new();
        while let Some(byte) = seq.next_element::<u8>()? {
            bytes.push(byte);
        }
        PackedChunks::from_bytes(&bytes).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for PackedChunks {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_str(PackedVisitor)
        } else {
            deserializer.deserialize_bytes(PackedVisitor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TileKind;

    fn tile(kind: TileKind) -> Tile {
        Tile {
            kind,
            durability: crate::terrain::tile_durability(kind),
        }
    }

    #[test]
    fn packed_chunks_round_trip_through_bytes_json_and_postcard() {
        let packed = PackedChunks(vec![
            (
                ChunkPosition { x: -3, y: 700 },
                TileDiffs::new((0..16).map(|index| (index, tile(TileKind::Air)))),
            ),
            (
                ChunkPosition { x: 4, y: 2 },
                TileDiffs::new([
                    (3, tile(TileKind::Air)),
                    (4, tile(TileKind::Stone)),
                    (200, tile(TileKind::Ore(crate::terrain::MineralKind::Gold))),
                    (255, tile(TileKind::Air)),
                ]),
            ),
        ]);
        assert_eq!(
            PackedChunks::from_bytes(&packed.to_bytes()),
            Ok(packed.clone())
        );
        let json = serde_json::to_string(&packed).expect("json");
        assert!(json.starts_with('"'), "base64 text in JSON: {json}");
        assert_eq!(
            serde_json::from_str::<PackedChunks>(&json).expect("json"),
            packed
        );
        let bytes = postcard::to_allocvec(&packed).expect("postcard");
        assert_eq!(
            postcard::from_bytes::<PackedChunks>(&bytes).expect("postcard"),
            packed
        );
        assert_eq!(PackedChunks::from_bytes(&[]), Ok(PackedChunks::default()));
        // A row of air packs into one run.
        let row = PackedChunks(vec![(
            ChunkPosition { x: 0, y: 0 },
            TileDiffs::new((16..32).map(|index| (index, tile(TileKind::Air)))),
        )]);
        assert!(row.to_bytes().len() <= 9, "{:?}", row.to_bytes());
        // So does a column of a shaft.
        let column = PackedChunks(vec![(
            ChunkPosition { x: 0, y: 0 },
            TileDiffs::new((0..16).map(|row| (row * 16 + 5, tile(TileKind::Air)))),
        )]);
        assert!(column.to_bytes().len() <= 9, "{:?}", column.to_bytes());
        assert_eq!(PackedChunks::from_bytes(&column.to_bytes()), Ok(column));
    }

    #[test]
    fn corrupt_packed_chunks_are_rejected() {
        assert!(PackedChunks::from_bytes(&[9]).is_err());
        assert!(PackedChunks::from_bytes(&[FORMAT, 1, 0]).is_err());
        assert!(PackedChunks::from_bytes(&[FORMAT, 0, 7]).is_err());
        assert!(serde_json::from_str::<PackedChunks>("\"%%%\"").is_err());
        // A run longer than a chunk.
        assert!(PackedChunks::from_bytes(&[FORMAT, 1, 0, 0, 0, 1, 0, 0xFF, 0x7F, 1]).is_err());
        // An unknown mode, and a repeated code before any.
        assert!(PackedChunks::from_bytes(&[FORMAT, 1, 0, 0, 7, 0]).is_err());
        assert!(PackedChunks::from_bytes(&[FORMAT, 1, 0, 0, 0, 1, 0, 1, 0]).is_err());
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        for (bytes, text) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode_base64(bytes), text);
            assert_eq!(decode_base64(text).expect("decodes"), bytes);
        }
    }
}
