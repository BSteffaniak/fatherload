//! A small, deterministic, platform-independent hasher for state hashes.

use std::hash::{Hash, Hasher};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64-bit hasher with fixed little-endian integer encoding.
///
/// Unlike `std`'s default hasher it is unkeyed and stable across runs, and
/// unlike a naive `Hasher` it encodes `usize` lengths as 64-bit and all
/// integers as little-endian, so a 32-bit WASM client and a 64-bit native host
/// produce the same hash for the same state.
#[derive(Clone, Copy, Debug)]
pub struct StableHasher {
    state: u64,
}

impl StableHasher {
    /// Creates a hasher with the standard FNV offset basis.
    #[must_use]
    pub const fn new() -> Self {
        Self { state: FNV_OFFSET }
    }
}

impl Default for StableHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
    }

    fn write_u16(&mut self, i: u16) {
        self.write(&i.to_le_bytes());
    }

    fn write_u32(&mut self, i: u32) {
        self.write(&i.to_le_bytes());
    }

    fn write_u64(&mut self, i: u64) {
        self.write(&i.to_le_bytes());
    }

    fn write_u128(&mut self, i: u128) {
        self.write(&i.to_le_bytes());
    }

    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }

    fn write_i16(&mut self, i: i16) {
        self.write(&i.to_le_bytes());
    }

    fn write_i32(&mut self, i: i32) {
        self.write(&i.to_le_bytes());
    }

    fn write_i64(&mut self, i: i64) {
        self.write(&i.to_le_bytes());
    }

    fn write_i128(&mut self, i: i128) {
        self.write(&i.to_le_bytes());
    }

    fn write_isize(&mut self, i: isize) {
        self.write_i64(i as i64);
    }
}

/// Hashes any `Hash` value with [`StableHasher`].
#[must_use]
pub fn stable_hash<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = StableHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
