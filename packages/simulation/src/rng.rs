//! Deterministic xorshift RNG whose state is part of the canonical world.

use serde::{Deserialize, Serialize};

const DEFAULT_SIMULATION_RNG_SEED: u64 = 0xD111_6A4E_C0DE_5EED;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeterministicRngState(u64);

impl DeterministicRngState {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            DEFAULT_SIMULATION_RNG_SEED
        } else {
            seed
        })
    }

    #[must_use]
    pub const fn state(self) -> u64 {
        self.0
    }

    pub const fn next_u64(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }
}
