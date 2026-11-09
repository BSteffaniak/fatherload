//! Rig parts, rig slots, and drill state.

use serde::{Deserialize, Serialize};

use crate::terrain::TilePosition;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DrillDirection {
    Down,
    Left,
    Right,
    /// Boring into the ceiling (needs the [`crate::economy::UpgradeKind::DrillMount`] crown
    /// bore tier).
    Up,
}

impl DrillDirection {
    /// Whether the drill points left or right.
    #[must_use]
    pub const fn is_sideways(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct DrillState {
    pub target: TilePosition,
    pub direction: DrillDirection,
    pub progress: f32,
    pub initial_durability: u8,
    pub seconds_per_chip: f32,
    pub sound_timer: f32,
    pub dust_timer: f32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum RigSlot {
    DrillHead,
    Engine,
    HullPlating,
    Radiator,
    CargoModule,
    ScannerModule,
    UtilityModule,
    EmergencySystem,
}

impl RigSlot {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::DrillHead => "Drill Head",
            Self::Engine => "Engine",
            Self::HullPlating => "Hull Plating",
            Self::Radiator => "Radiator",
            Self::CargoModule => "Cargo Module",
            Self::ScannerModule => "Scanner Module",
            Self::UtilityModule => "Utility Module",
            Self::EmergencySystem => "Emergency System",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum RigPartKind {
    TitanDrill,
    NeedleDrill,
    ResonanceDrill,
    LightweightEngine,
    HaulerEngine,
    BurstEngine,
    ThermalHull,
    ImpactHull,
    PressureHull,
    ProspectorScanner,
    HazardScanner,
    RelicScanner,
    CargoBalloon,
    ArmoredCargoBay,
    SortedCargoRack,
}

impl RigPartKind {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::TitanDrill => "Titan Drill",
            Self::NeedleDrill => "Needle Drill",
            Self::ResonanceDrill => "Resonance Drill",
            Self::LightweightEngine => "Lightweight Engine",
            Self::HaulerEngine => "Hauler Engine",
            Self::BurstEngine => "Burst Engine",
            Self::ThermalHull => "Thermal Hull",
            Self::ImpactHull => "Impact Hull",
            Self::PressureHull => "Pressure Hull",
            Self::ProspectorScanner => "Prospector Scanner",
            Self::HazardScanner => "Hazard Scanner",
            Self::RelicScanner => "Relic Scanner",
            Self::CargoBalloon => "Cargo Balloon",
            Self::ArmoredCargoBay => "Armored Cargo Bay",
            Self::SortedCargoRack => "Sorted Cargo Rack",
        }
    }

    #[must_use]
    pub const fn slot(self) -> RigSlot {
        match self {
            Self::TitanDrill | Self::NeedleDrill | Self::ResonanceDrill => RigSlot::DrillHead,
            Self::LightweightEngine | Self::HaulerEngine | Self::BurstEngine => RigSlot::Engine,
            Self::ThermalHull | Self::ImpactHull | Self::PressureHull => RigSlot::HullPlating,
            Self::ProspectorScanner | Self::HazardScanner | Self::RelicScanner => {
                RigSlot::ScannerModule
            }
            Self::CargoBalloon | Self::ArmoredCargoBay | Self::SortedCargoRack => {
                RigSlot::CargoModule
            }
        }
    }
}
