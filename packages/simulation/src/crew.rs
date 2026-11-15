//! Persistent crew identity (WP-PERSIST): who each rig belongs to across sessions.
//!
//! A [`ProfileId`] is a random id generated once per install (the game keeps it in its profile
//! file; in the browser that file lives in `IndexedDB`). Every rig in a world can carry a
//! [`CrewMember`] record naming the profile it belongs to, its display name and, for rigs played
//! from a local split-screen seat on the host, which seat. The records save with the world, so a
//! host that resumes a multiplayer save recognizes every returning player.
//!
//! Players who are not connected are **parked** ([`ParkedPlayer`]): their rig and every piece of
//! per-player state move out of the live world (not simulated, not rendered, not replicated) into
//! the world's [`CrewRoster`], which saves with the world. Unparking puts all of it back exactly,
//! under the same [`crate::PlayerId`], so nothing keyed by player id needs remapping.
//!
//! See `WorldState::park_player`, `WorldState::unpark_player`, `WorldState::player_for_profile`
//! and `WorldState::set_crew_member` (in `world/crew.rs`).

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    ids::{PlayerId, SimulationTick},
    player::Player,
    quest::ActiveQuest,
    rig::RigPartKind,
    world::{DeepRunState, DiscoveredTiles, LostCargoSite},
};

/// A persistent, install-wide player identity. Serialized as 16 hex digits.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProfileId(u64);

impl ProfileId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The identity of local split-screen seat `seat` on the install whose profile is `self`:
    /// seat 0 is the profile itself, every further seat a distinct id derived from it, so a host's
    /// second, third and fourth seats are recognized across sessions just like its first.
    #[must_use]
    pub const fn for_seat(self, seat: u32) -> Self {
        if seat == 0 {
            return self;
        }
        // splitmix64 finalizer over the profile and the seat.
        let mut z = self
            .0
            .wrapping_add((seat as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        Self(z ^ (z >> 31))
    }

    /// Parses 1-16 hex digits.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() || text.len() > 16 {
            return None;
        }
        u64::from_str_radix(text, 16).ok().map(Self)
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl Serialize for ProfileId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_str(self)
        } else {
            serializer.serialize_u64(self.0)
        }
    }
}

impl<'de> Deserialize<'de> for ProfileId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let text = String::deserialize(deserializer)?;
            Self::parse(&text)
                .ok_or_else(|| serde::de::Error::custom(format!("invalid profile id {text:?}")))
        } else {
            u64::deserialize(deserializer).map(Self)
        }
    }
}

/// Who a rig belongs to.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CrewMember {
    /// The install profile that plays this rig (`None`: unknown, e.g. a rig from a save written
    /// before persistent identities; it can still be claimed).
    #[serde(default)]
    pub profile: Option<ProfileId>,
    /// Display name, as last sent by the player.
    #[serde(default)]
    pub name: String,
    /// The host's local split-screen seat index this rig was played from (`None` for remote
    /// players). Lets a host that moved to another install rebind its seats.
    #[serde(default)]
    pub seat: Option<u32>,
}

impl CrewMember {
    /// The name to show, falling back to a rig number for unnamed rigs.
    #[must_use]
    pub fn display_name(&self, player_id: PlayerId) -> String {
        if self.name.trim().is_empty() {
            format!("Rig {}", player_id.get() + 1)
        } else {
            self.name.clone()
        }
    }
}

/// A player who is not in the live world, with every piece of their per-player state.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ParkedPlayer {
    pub member: CrewMember,
    pub player: Player,
    #[serde(default)]
    pub equipped_rig_parts: BTreeSet<RigPartKind>,
    #[serde(default)]
    pub mechanic_level: u8,
    #[serde(default)]
    pub deep_run: Option<DeepRunState>,
    #[serde(default)]
    pub lost_cargo: Option<LostCargoSite>,
    #[serde(default)]
    pub rescue_count: u32,
    #[serde(default)]
    pub deep_reward_milestone: Option<i32>,
    /// Crew-mode quests this player owned.
    #[serde(default)]
    pub crew_quests: Vec<ActiveQuest>,
    #[serde(default)]
    pub discovered: DiscoveredTiles,
    /// World tick when the player was parked.
    #[serde(default)]
    pub parked_at: SimulationTick,
    /// Their pilot, if they were on foot (Part II F5): it comes back where it stood.
    #[serde(default)]
    pub pilot: Option<crate::world::pilot::ParkedPilot>,
}

/// Identity of every rig in the world plus the parked players (saved with the world, never sent
/// to clients).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct CrewRoster {
    /// Identity of live players.
    #[serde(default)]
    pub(crate) members: BTreeMap<PlayerId, CrewMember>,
    /// Players parked outside the live world.
    #[serde(default)]
    pub(crate) parked: BTreeMap<PlayerId, ParkedPlayer>,
}

impl CrewRoster {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty() && self.parked.is_empty()
    }
}
