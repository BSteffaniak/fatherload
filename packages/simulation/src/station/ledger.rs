//! The crew's station state: a world field in the shared `SectionKey::Stations` section, saved
//! with serde defaults and hashed only once something happened (older worlds keep their
//! hashes).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{StationId, bounty::Mission, market::Good, market::MarketShift};
use crate::{ids::PlayerId, space::SpaceBodyId};

/// One ship's hold of trade goods.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Hold {
    #[serde(default, with = "crate::pair_list")]
    pub goods: BTreeMap<Good, u32>,
}

impl Hold {
    /// Units aboard.
    #[must_use]
    pub fn used(&self) -> u32 {
        self.goods.values().sum()
    }

    /// Units of `good` aboard.
    #[must_use]
    pub fn count(&self, good: Good) -> u32 {
        self.goods.get(&good).copied().unwrap_or(0)
    }

    pub fn add(&mut self, good: Good, units: u32) {
        if units > 0 {
            *self.goods.entry(good).or_insert(0) += units;
        }
    }

    /// Takes up to `units` of `good`; returns how many.
    pub fn take(&mut self, good: Good, units: u32) -> u32 {
        let Some(count) = self.goods.get_mut(&good) else {
            return 0;
        };
        let taken = units.min(*count);
        *count -= taken;
        if *count == 0 {
            self.goods.remove(&good);
        }
        taken
    }
}

/// The crew's station state (see the module docs).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct StationLedger {
    /// Each player's hold.
    #[serde(default, with = "crate::pair_list")]
    pub holds: BTreeMap<PlayerId, Hold>,
    /// Each player's missions under way.
    #[serde(default, with = "crate::pair_list")]
    pub missions: BTreeMap<PlayerId, Vec<Mission>>,
    /// Each board's slot serials (a slot rolls its next mission when taken).
    #[serde(default, with = "crate::pair_list")]
    pub boards: BTreeMap<StationId, [u16; 3]>,
    /// The crew's trades moving each station's prices.
    #[serde(default, with = "crate::pair_list")]
    pub market: BTreeMap<(StationId, Good), MarketShift>,
    /// Derelict stations already stripped.
    #[serde(default)]
    pub salvaged: BTreeSet<StationId>,
    /// Rumours heard (station, patron).
    #[serde(default)]
    pub heard: BTreeSet<(StationId, u8)>,
    /// Bodies the crew scanned.
    #[serde(default)]
    pub scanned: BTreeSet<SpaceBodyId>,
    /// Missions each player completed.
    #[serde(default, with = "crate::pair_list")]
    pub completed: BTreeMap<PlayerId, u32>,
    /// The hazards each player's rig was warned about on the body it is on (bits of
    /// [`super::PlanetHazard::bit`]), so the warning fires once a landing.
    #[serde(default, with = "crate::pair_list")]
    pub warned: BTreeMap<PlayerId, (SpaceBodyId, u8)>,
}

impl StationLedger {
    /// Whether nothing was ever recorded (the state hash skips it).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.holds.is_empty()
            && self.missions.is_empty()
            && self.boards.is_empty()
            && self.market.is_empty()
            && self.salvaged.is_empty()
            && self.heard.is_empty()
            && self.scanned.is_empty()
            && self.completed.is_empty()
            && self.warned.is_empty()
    }

    /// Forgets a player who left (their hold and missions go with them).
    pub fn forget(&mut self, player: PlayerId) {
        self.holds.remove(&player);
        self.missions.remove(&player);
        self.completed.remove(&player);
        self.warned.remove(&player);
    }

    /// `player`'s hold.
    #[must_use]
    pub fn hold(&self, player: PlayerId) -> Option<&Hold> {
        self.holds.get(&player)
    }

    /// `player`'s missions under way.
    #[must_use]
    pub fn missions_of(&self, player: PlayerId) -> &[Mission] {
        self.missions.get(&player).map_or(&[], Vec::as_slice)
    }

    /// Hold room `player` uses: goods plus deliveries consigned.
    #[must_use]
    pub fn hold_used(&self, player: PlayerId) -> u32 {
        self.holds.get(&player).map_or(0, Hold::used)
            + self
                .missions_of(player)
                .iter()
                .map(|mission| mission.kind.hold_units())
                .sum::<u32>()
    }

    /// A board's slot serials.
    #[must_use]
    pub fn serials(&self, station: StationId) -> [u16; 3] {
        self.boards.get(&station).copied().unwrap_or_default()
    }

    /// The crew's shift on `good` at `station`.
    #[must_use]
    pub fn shift(&self, station: StationId, good: Good) -> MarketShift {
        self.market
            .get(&(station, good))
            .copied()
            .unwrap_or_default()
    }
}
