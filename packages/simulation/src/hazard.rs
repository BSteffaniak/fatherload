//! Live hazard entities: gas clouds, falling boulders, placed bombs, and scanner markers.

use serde::{Deserialize, Serialize};

use crate::{
    explosive::{BlastDirection, ExplosiveKind, REMOTE_FUSE},
    ids::PlayerId,
    terrain::{TileKind, TilePosition},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct HazardCloud {
    pub x: f32,
    pub y: f32,
    pub life: f32,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct PlacedBomb {
    /// Stable session identity; retained in replication and saves.
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    /// Previous tick position for presentation-only interpolation.
    pub previous_x: f32,
    pub previous_y: f32,
    pub angle: f32,
    pub detonation_tick: crate::SimulationTick,
    pub chain_triggered: bool,
    pub timer_seconds: f32,
    /// What kind of explosive this is (older saves and replicas: the plain timed bomb).
    #[serde(default)]
    pub kind: ExplosiveKind,
    /// The player who deployed it (who may trigger it remotely). `None` for charges from older
    /// saves and for scenario bombs.
    #[serde(default)]
    pub owner: Option<PlayerId>,
    /// Stuck to a surface: no gravity, no push, until the tile holding it is gone.
    #[serde(default)]
    pub stuck: bool,
    /// Which way a shaped charge cuts (into the surface it stuck to, or its throw direction).
    #[serde(default)]
    pub direction: BlastDirection,
    /// Lifecycle stage after the first detonation: an incendiary's fire ring, a flare's pulse.
    /// 0 until the fuse first runs out.
    #[serde(default)]
    pub stage: u8,
}

impl PlacedBomb {
    /// Whether this is a remote charge still waiting for its owner's detonator.
    #[must_use]
    pub fn awaiting_remote(&self) -> bool {
        self.kind.remote() && self.detonation_tick == REMOTE_FUSE
    }

    /// Whether the charge has gone off already and is burning (a lit flare, a spreading fire).
    #[must_use]
    pub const fn burning(&self) -> bool {
        self.stage > 0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct FallingBoulder {
    pub x: f32,
    pub y: f32,
    pub velocity_y: f32,
    pub warning_seconds: f32,
    pub life: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ScanMarker {
    pub position: TilePosition,
    pub kind: TileKind,
}
