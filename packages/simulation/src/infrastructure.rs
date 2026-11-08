//! Placeable underground infrastructure.

use serde::{Deserialize, Serialize};

use crate::terrain::TilePosition;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum InfrastructureKind {
    SignalRelay,
    SurveyDrone,
    CargoLift,
    TunnelSupport,
    PumpStation,
    OreProcessor,
}

impl InfrastructureKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SignalRelay => "Signal Relay",
            Self::SurveyDrone => "Survey Drone",
            Self::CargoLift => "Cargo Lift",
            Self::TunnelSupport => "Tunnel Support",
            Self::PumpStation => "Pump Station",
            Self::OreProcessor => "Ore Processor",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PlacedInfrastructure {
    pub kind: InfrastructureKind,
    pub position: TilePosition,
    #[serde(default = "default_infrastructure_durability")]
    pub durability: u8,
}

#[must_use]
pub const fn default_infrastructure_durability() -> u8 {
    100
}
