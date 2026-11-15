//! Process-wide memo caches for streamed generation (region outputs and generated chunks).
//!
//! Every cached value is a pure function of its key, so the cache only saves work: what is
//! evicted is recomputed bit-identically on the next request. Values are computed outside the
//! lock, so a computation may consult other caches (or, rarely, two threads may both compute
//! the same entry; the first one stored wins and both are identical).

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, PoisonError},
};

use crate::worldgen::WorldSettings;

/// A cache key: the world and two integer coordinates (a region or a chunk).
pub type CacheKey = (WorldSettings, i32, i32);

struct Entries<T> {
    map: BTreeMap<CacheKey, (u64, Arc<T>)>,
    clock: u64,
}

/// A bounded least-recently-used map from [`CacheKey`] to shared values.
pub struct MemoCache<T> {
    entries: Mutex<Entries<T>>,
    capacity: usize,
}

impl<T> MemoCache<T> {
    pub const fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(Entries {
                map: BTreeMap::new(),
                clock: 0,
            }),
            capacity,
        }
    }

    /// The cached value for `key`, computing (and caching) it on a miss.
    pub fn get_or_compute(&self, key: CacheKey, compute: impl FnOnce() -> T) -> Arc<T> {
        {
            let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            entries.clock += 1;
            let clock = entries.clock;
            if let Some(entry) = entries.map.get_mut(&key) {
                entry.0 = clock;
                return Arc::clone(&entry.1);
            }
        }
        let value = Arc::new(compute());
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.clock += 1;
        let clock = entries.clock;
        let stored = Arc::clone(&entries.map.entry(key).or_insert((clock, value)).1);
        if entries.map.len() > self.capacity {
            // Drop the least recently used quarter in one sweep (amortized O(1) per insert).
            let mut stamps: Vec<u64> = entries.map.values().map(|(stamp, _)| *stamp).collect();
            stamps.sort_unstable();
            let cutoff = stamps[stamps.len() / 4];
            entries.map.retain(|_, (stamp, _)| *stamp > cutoff);
        }
        stored
    }

    /// Forgets every entry (tests: proves results do not depend on what was cached).
    #[cfg(test)]
    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .map
            .clear();
    }
}
