//! Live hazard entities: gas clouds, falling boulders, placed bombs, and scanner markers.

use serde::{Deserialize, Serialize};

use crate::terrain::{TileKind, TilePosition};

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
