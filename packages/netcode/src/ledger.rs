//! Exactly-once action bookkeeping.

use std::collections::BTreeSet;

use crate::ids::ActionId;

/// Tracks which [`ActionId`]s of one client session have been applied.
///
/// Ids are assigned sequentially from 1 by the client. The ledger keeps a
/// contiguous watermark (every id `<= watermark` is settled) plus the sparse
/// set of applied ids above it, so memory stays bounded as long as the client
/// keeps resending unacknowledged actions (it does) or raises its floor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActionLedger {
    watermark: u64,
    above: BTreeSet<u64>,
}

impl ActionLedger {
    /// Creates an empty ledger.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            watermark: 0,
            above: BTreeSet::new(),
        }
    }

    /// Marks `id` applied. Returns `false` if it was already applied (or settled).
    pub fn try_apply(&mut self, id: ActionId) -> bool {
        if id.0 <= self.watermark || !self.above.insert(id.0) {
            return false;
        }
        self.compact();
        true
    }

    /// Whether `id` has been applied or settled.
    #[must_use]
    pub fn contains(&self, id: ActionId) -> bool {
        id.0 <= self.watermark || self.above.contains(&id.0)
    }

    /// The client declares that every id below `floor` is settled (applied,
    /// acknowledged or abandoned, e.g. after an epoch change).
    pub fn raise_floor(&mut self, floor: ActionId) {
        let settled = floor.0.saturating_sub(1);
        if settled > self.watermark {
            self.watermark = settled;
            self.above.retain(|id| *id > settled);
            self.compact();
        }
    }

    /// Highest id such that every id at or below it is settled.
    #[must_use]
    pub const fn watermark(&self) -> ActionId {
        ActionId(self.watermark)
    }

    /// Number of applied ids above the watermark (bounded in practice).
    #[must_use]
    pub fn backlog(&self) -> usize {
        self.above.len()
    }

    fn compact(&mut self) {
        while self.above.remove(&(self.watermark + 1)) {
            self.watermark += 1;
        }
    }
}
