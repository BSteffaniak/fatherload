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

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PlacedBomb {
    pub x: f32,
    pub y: f32,
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
