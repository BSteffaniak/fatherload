//! Contracts, expeditions, world events, and legendary blueprints.

use serde::{Deserialize, Serialize};

use crate::{player::Player, terrain::TileKind};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LegendaryBlueprint {
    StarPlating,
    VoidTank,
    RelicSorter,
}

impl LegendaryBlueprint {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::StarPlating => "Star Plating",
            Self::VoidTank => "Void Tank",
            Self::RelicSorter => "Relic Sorter",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum SideContractKind {
    #[default]
    Cargo,
    DepthSurvey,
    HazardScan,
    Rush,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct SideContract {
    pub kind: SideContractKind,
    pub target: TileKind,
    pub required: u32,
    #[serde(default)]
    pub expires_day: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum WorldEventKind {
    MarketCrash,
    MarketBoom,
    RareBuyer,
    FuelShortage,
    RepairBacklog,
    HeatWave,
    CollapseSurge,
    DeepPressureStorm,
    GasBloom,
    Earthquake,
    MeteorShower,
    RivalClaims,
    AncientMachine,
}

impl WorldEventKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::MarketCrash => "Market crash",
            Self::MarketBoom => "Market boom",
            Self::RareBuyer => "Rare buyer",
            Self::FuelShortage => "Fuel shortage",
            Self::RepairBacklog => "Repair backlog",
            Self::HeatWave => "Heat wave",
            Self::CollapseSurge => "Collapse surge",
            Self::DeepPressureStorm => "Deep pressure storm",
            Self::GasBloom => "Gas bloom",
            Self::Earthquake => "Earthquake",
            Self::MeteorShower => "Meteor shower",
            Self::RivalClaims => "Rival claims",
            Self::AncientMachine => "Ancient machine",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ActiveWorldEvent {
    pub kind: WorldEventKind,
    pub days_remaining: u32,
    pub severity: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ExpeditionObjectiveKind {
    #[default]
    ReachDepth,
    DeliverCargo,
    ScanHazards,
    BuildPumpStations,
    RecoverProbe,
    MineVein,
    ScanAnomaly,
    RescueMiner,
    DeliverExplosives,
    StabilizeCollapse,
    RetrieveArtifact,
    ReachSignal,
    NoDamageReturn,
    FastReturn,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Expedition {
    pub kind: ExpeditionObjectiveKind,
    pub target: TileKind,
    pub required: u32,
    pub reward: u32,
    pub expires_day: u32,
}

impl Expedition {
    #[must_use]
    pub fn title(self) -> String {
        match self.kind {
            ExpeditionObjectiveKind::ReachDepth => format!("Survey {}m claim", self.required),
            ExpeditionObjectiveKind::DeliverCargo => {
                format!("Deliver {} x{}", self.target.name(), self.required)
            }
            ExpeditionObjectiveKind::ScanHazards => format!("Map {} hazards", self.required),
            ExpeditionObjectiveKind::BuildPumpStations => {
                format!("Install {} pump station(s)", self.required)
            }
            ExpeditionObjectiveKind::RecoverProbe => "Recover a lost probe".to_owned(),
            ExpeditionObjectiveKind::MineVein => {
                format!("Mine {} vein x{}", self.target.name(), self.required)
            }
            ExpeditionObjectiveKind::ScanAnomaly => format!("Scan {} anomaly", self.target.name()),
            ExpeditionObjectiveKind::RescueMiner => "Rescue trapped miner signal".to_owned(),
            ExpeditionObjectiveKind::DeliverExplosives => {
                format!("Deliver {} charges underground", self.required)
            }
            ExpeditionObjectiveKind::StabilizeCollapse => "Stabilize collapse zone".to_owned(),
            ExpeditionObjectiveKind::RetrieveArtifact => {
                format!("Retrieve ancient {}", self.target.name())
            }
            ExpeditionObjectiveKind::ReachSignal => {
                "Reach temporary signal before expiry".to_owned()
            }
            ExpeditionObjectiveKind::NoDamageReturn => "Return with no hull damage".to_owned(),
            ExpeditionObjectiveKind::FastReturn => "Return before deadline".to_owned(),
        }
    }
}

pub fn consume_expedition_delivery(expedition: Expedition, player: &mut Player) {
    if expedition.kind != ExpeditionObjectiveKind::DeliverCargo {
        return;
    }
    match expedition.target {
        TileKind::Ore(mineral) => {
            player.cargo_bay.remove(mineral, expedition.required);
        }
        TileKind::Artifact(artifact) => {
            player.cargo_bay.remove(artifact, expedition.required);
        }
        _ => {}
    }
}

pub fn consume_side_contract_cargo(contract: SideContract, player: &mut Player) {
    match contract.target {
        TileKind::Ore(mineral) => {
            player.cargo_bay.remove(mineral, contract.required);
        }
        TileKind::Artifact(artifact) => {
            player.cargo_bay.remove(artifact, contract.required);
        }
        _ => {}
    }
}
