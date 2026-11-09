//! Gameplay events and per-tick simulation output with stable `(tick, ordinal)` identity.

use serde::{Deserialize, Serialize};

use crate::{
    command::PlayerCommand,
    ids::{InputSequence, PlayerId, SimulationTick},
    poi::{PoiKind, PoiReward},
    quest::{BlueprintId, QuestFailure, QuestId},
    summary::PlayerScopedCommandOutcome,
    worldgen::FeatureId,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameplayEventId {
    pub tick: SimulationTick,
    pub ordinal: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum GameplayEventKind {
    DrillStarted {
        player_id: PlayerId,
    },
    PurchaseCompleted {
        player_id: PlayerId,
    },
    CargoChanged {
        player_id: PlayerId,
    },
    BombPlaced {
        player_id: PlayerId,
    },
    InfrastructurePlaced {
        player_id: PlayerId,
    },
    ScannerUsed {
        player_id: PlayerId,
    },
    EscapeStarted {
        player_id: PlayerId,
    },
    EscapeTunnelChanged,
    DeepRewardGranted {
        player_id: PlayerId,
    },
    RescueTriggered {
        player_id: PlayerId,
    },
    PlayerDamaged {
        player_id: PlayerId,
    },
    HazardChanged,
    BouldersChanged,
    BombDetonated {
        bomb_id: u64,
        x: i32,
        y: i32,
    },
    BombBounced {
        bomb_id: u64,
        x: i32,
        y: i32,
    },
    /// A player opened a POI tile (a crate, a derelict's core, a vault cache, an outpost
    /// locker) and received `reward`.
    PoiOpened {
        player_id: PlayerId,
        poi: PoiKind,
        reward: PoiReward,
    },
    /// A player entered a point of interest for the first time.
    PoiDiscovered {
        player_id: PlayerId,
        feature: FeatureId,
        poi: PoiKind,
    },
    /// A vault's lock opened (its doors are gone).
    VaultOpened {
        feature: FeatureId,
    },
    /// A vault's lock reset (wrong switch, or a relay cooled down).
    VaultLockReset {
        feature: FeatureId,
    },
    /// A player changed a test-mode toggle or warped (test-mode worlds only).
    TestModeChanged {
        player_id: PlayerId,
    },
    /// A player took on a quest at its giver's counter.
    QuestAccepted {
        player_id: PlayerId,
        quest: QuestId,
    },
    /// A quest moved on to its next stage.
    QuestStageDone {
        player_id: PlayerId,
        quest: QuestId,
    },
    /// Every stage of a quest is done: it can be turned in.
    QuestReady {
        player_id: PlayerId,
        quest: QuestId,
    },
    /// A quest was turned in and paid (the completion fanfare).
    QuestCompleted {
        player_id: PlayerId,
        quest: QuestId,
    },
    /// A quest failed (its twist caught the player out).
    QuestFailed {
        player_id: PlayerId,
        quest: QuestId,
        reason: QuestFailure,
    },
    /// A player finished assembling a blueprint from its fragments.
    BlueprintCompleted {
        player_id: PlayerId,
        blueprint: BlueprintId,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameplayEvent {
    pub id: GameplayEventId,
    pub kind: GameplayEventKind,
}

/// How one one-shot action submitted for a tick was resolved.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionOutcome {
    pub player_id: PlayerId,
    pub sequence: InputSequence,
    pub outcome: PlayerScopedCommandOutcome,
}

/// Everything one tick produced besides the state change itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TickOutput {
    pub tick: SimulationTick,
    pub events: Vec<GameplayEvent>,
    /// Every action in the tick's inputs, in input order, with its outcome.
    pub action_outcomes: Vec<ActionOutcome>,
    /// Every terrain tile write this tick, in order (the op-log replication section).
    pub terrain_ops: Vec<crate::terrain::TileChange>,
    /// Tiles each player newly discovered this tick (op log, private to that player).
    pub discoveries: Vec<crate::world::Discovery>,
}

impl TickOutput {
    pub(crate) fn push_event(&mut self, kind: GameplayEventKind) {
        let ordinal = u32::try_from(self.events.len()).unwrap_or(u32::MAX);
        self.events.push(GameplayEvent {
            id: GameplayEventId {
                tick: self.tick,
                ordinal,
            },
            kind,
        });
    }
}

pub(crate) const fn gameplay_event_for_applied_command(
    player_id: PlayerId,
    command: &PlayerCommand,
) -> GameplayEventKind {
    match command {
        PlayerCommand::Refuel
        | PlayerCommand::Repair
        | PlayerCommand::BuyUpgrade { .. }
        | PlayerCommand::Service(_) => GameplayEventKind::PurchaseCompleted { player_id },
        PlayerCommand::SellCargo => GameplayEventKind::CargoChanged { player_id },
        PlayerCommand::PlaceBomb => GameplayEventKind::BombPlaced { player_id },
        PlayerCommand::PlaceInfrastructure { .. } => {
            GameplayEventKind::InfrastructurePlaced { player_id }
        }
        PlayerCommand::UseScanner => GameplayEventKind::ScannerUsed { player_id },
        PlayerCommand::Rescue => GameplayEventKind::RescueTriggered { player_id },
        PlayerCommand::TestMode(_) => GameplayEventKind::TestModeChanged { player_id },
    }
}
