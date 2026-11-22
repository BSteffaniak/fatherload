//! Maps saved as lists of `(key, value)` pairs (`#[serde(with = "crate::pair_list")]`).
//!
//! Saves are JSON, and JSON object keys must be strings: a map keyed by a struct or a tuple
//! (a [`crate::terrain::TilePosition`], a `(tick, tile)` pair) fails to save as a map. As a list
//! of pairs it saves in JSON and travels the same in postcard.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Writes `map` as a sequence of its `(key, value)` pairs, in key order.
///
/// # Errors
///
/// When the serializer fails.
pub fn serialize<S: Serializer, K: Serialize, V: Serialize>(
    map: &BTreeMap<K, V>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(map.iter())
}

/// Reads a map written by [`serialize`] (a later pair for the same key wins).
///
/// # Errors
///
/// When the input is not a sequence of pairs.
pub fn deserialize<'de, D: Deserializer<'de>, K: Deserialize<'de> + Ord, V: Deserialize<'de>>(
    deserializer: D,
) -> Result<BTreeMap<K, V>, D::Error> {
    Ok(Vec::<(K, V)>::deserialize(deserializer)?
        .into_iter()
        .collect())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Serialize};

    use crate::terrain::TilePosition;

    #[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
    struct Keyed {
        #[serde(default, with = "super")]
        tiles: BTreeMap<TilePosition, u8>,
        #[serde(default, with = "super")]
        pairs: BTreeMap<(u64, TilePosition), bool>,
    }

    #[test]
    fn struct_and_tuple_keyed_maps_survive_json_and_postcard() {
        let mut keyed = Keyed::default();
        keyed.tiles.insert(TilePosition { x: -3, y: 9 }, 4);
        keyed.tiles.insert(TilePosition { x: 7, y: 2 }, 1);
        keyed.pairs.insert((12, TilePosition { x: 0, y: -1 }), true);
        let json = serde_json::to_string(&keyed).expect("json save");
        assert_eq!(
            serde_json::from_str::<Keyed>(&json).expect("json load"),
            keyed
        );
        let bytes = postcard::to_stdvec(&keyed).expect("encode");
        assert_eq!(
            postcard::from_bytes::<Keyed>(&bytes).expect("decode"),
            keyed
        );
        // A missing field still loads empty.
        assert_eq!(
            serde_json::from_str::<Keyed>("{}").expect("old save"),
            Keyed::default()
        );
    }
}
