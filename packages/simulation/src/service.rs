//! Town services, crafting recipes, and the transactions they record.

use serde::{Deserialize, Serialize};

use crate::{economy::TownBuilding, ids::PlayerId, quest::QuestId, terrain::StrategicResourceKind};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RecipeKind {
    ReinforcedBulkhead,
    AuxiliaryTank,
    ExpandedSorter,
    SignalRelayKit,
    SurveyDroneKit,
    CargoLiftKit,
    TunnelSupportKit,
    PumpStationKit,
    OreProcessorKit,
}

impl RecipeKind {
    pub const ALL: [Self; 9] = [
        Self::ReinforcedBulkhead,
        Self::AuxiliaryTank,
        Self::ExpandedSorter,
        Self::SignalRelayKit,
        Self::SurveyDroneKit,
        Self::CargoLiftKit,
        Self::TunnelSupportKit,
        Self::PumpStationKit,
        Self::OreProcessorKit,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ReinforcedBulkhead => "Reinforced Bulkhead",
            Self::AuxiliaryTank => "Auxiliary Tank",
            Self::ExpandedSorter => "Expanded Sorter",
            Self::SignalRelayKit => "Signal Relay Kit",
            Self::SurveyDroneKit => "Survey Drone Kit",
            Self::CargoLiftKit => "Cargo Lift Kit",
            Self::TunnelSupportKit => "Tunnel Support Kit",
            Self::PumpStationKit => "Pump Station Kit",
            Self::OreProcessorKit => "Ore Processor Kit",
        }
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::ReinforcedBulkhead => "+15 max hull rig part",
            Self::AuxiliaryTank => "+20 fuel capacity rig part",
            Self::ExpandedSorter => "+4 cargo capacity rig part",
            Self::SignalRelayKit => "crafted infrastructure item",
            Self::SurveyDroneKit => "reveals nearby map over time",
            Self::CargoLiftKit => "sends cargo upward from a station",
            Self::TunnelSupportKit => "protects nearby tunnel from collapses",
            Self::PumpStationKit => "suppresses nearby gas and heat hazards",
            Self::OreProcessorKit => "refines cheap ore into strategic materials",
        }
    }

    #[must_use]
    pub const fn cost(self) -> &'static [(StrategicResourceKind, u32)] {
        match self {
            Self::ReinforcedBulkhead => &[(StrategicResourceKind::AncientAlloy, 2)],
            Self::AuxiliaryTank => &[
                (StrategicResourceKind::AncientAlloy, 1),
                (StrategicResourceKind::CrystalLens, 1),
            ],
            Self::ExpandedSorter => &[
                (StrategicResourceKind::AncientAlloy, 1),
                (StrategicResourceKind::CoreShard, 1),
            ],
            Self::SignalRelayKit => &[(StrategicResourceKind::CoreShard, 2)],
            Self::SurveyDroneKit => &[
                (StrategicResourceKind::CrystalLens, 1),
                (StrategicResourceKind::CoreShard, 1),
            ],
            Self::CargoLiftKit => &[
                (StrategicResourceKind::AncientAlloy, 2),
                (StrategicResourceKind::CoreShard, 1),
            ],
            Self::TunnelSupportKit => &[(StrategicResourceKind::AncientAlloy, 1)],
            Self::PumpStationKit => &[
                (StrategicResourceKind::AncientAlloy, 1),
                (StrategicResourceKind::CrystalLens, 1),
                (StrategicResourceKind::CoreShard, 1),
            ],
            Self::OreProcessorKit => &[
                (StrategicResourceKind::AncientAlloy, 2),
                (StrategicResourceKind::CrystalLens, 1),
            ],
        }
    }
}

/// A town service transaction requested from a building menu.
///
/// Submitted as [`crate::PlayerCommand::Service`] so it goes through the fixed tick like every
/// other gameplay action (and is never predicted by clients).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SessionServiceRequest {
    Refuel {
        menu_item: usize,
    },
    Repair {
        menu_item: usize,
    },
    SellCargo,
    BuyUpgrade {
        index: usize,
    },
    BuyBombBundle {
        count: u32,
        cost: u32,
    },
    BuyMiningRockets,
    ClaimFreeTestCharge,
    SalvagePatchHull,
    Finance,
    BuyInsurance,
    CraftRecipe {
        recipe: RecipeKind,
    },
    UpgradeTownBuilding {
        building: TownBuilding,
    },
    SalvageRecoverLostCargo,
    SalvageLaunchDrone,
    SalvageRecoverWreckedPart,
    SalvageClearCollapseZones,
    SalvageSellScrapTip,
    SellScanData,
    AutoSortLowGradeCargo,
    CompleteDepotWork,
    StartSideContract,
    AcceptExpedition {
        index: usize,
    },
    AbandonExpedition {
        index: usize,
    },
    /// Take on a quest at its giver's counter (`crew`: share it with every rig when the quest
    /// has co-op objectives and others are playing; otherwise its solo variant).
    AcceptQuest {
        quest: QuestId,
        crew: bool,
    },
    /// Hand in a finished quest at its giver's counter.
    TurnInQuest {
        quest: QuestId,
    },
    /// Drop an active quest at its giver's counter.
    AbandonQuest {
        quest: QuestId,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PlayerTransactionKind {
    BuyUpgrade,
    Refuel,
    Repair,
    SellCargo,
    Rescue,
    CompleteContract,
    StartExpedition,
    RepayDebt,
    WinGame,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlayerServiceTransaction {
    pub player_id: PlayerId,
    pub kind: PlayerTransactionKind,
    pub credits_before: u32,
    pub credits_after: u32,
    pub cargo_before: u32,
    pub cargo_after: u32,
}
