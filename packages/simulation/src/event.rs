//! Gameplay events and per-tick simulation output with stable `(tick, ordinal)` identity.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    command::{PlayerCommand, SequencedPlayerCommand},
    ids::{InputSequence, PlayerId, SimulationTick},
    summary::PlayerScopedCommandOutcome,
    terrain::TilePosition,
    terrain_revision::TerrainChunkRevision,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameplayEventId {
    pub tick: SimulationTick,
    pub ordinal: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum GameplayEventKind {
    DrillStarted { player_id: PlayerId },
    PurchaseCompleted { player_id: PlayerId },
    CargoChanged { player_id: PlayerId },
    BombPlaced { player_id: PlayerId },
    InfrastructurePlaced { player_id: PlayerId },
    ScannerUsed { player_id: PlayerId },
    EscapeStarted { player_id: PlayerId },
    EscapeTunnelChanged,
    DeepRewardGranted { player_id: PlayerId },
    RescueTriggered { player_id: PlayerId },
    PlayerDamaged { player_id: PlayerId },
    HazardChanged,
    BouldersChanged,
    BombDetonated,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TickOutput {
    pub tick: SimulationTick,
    pub command_count: usize,
    pub changed_players: Vec<PlayerId>,
    pub latest_sequences_by_player: BTreeMap<PlayerId, InputSequence>,
    pub events: Vec<GameplayEvent>,
    pub terrain_changed_tiles: Vec<TilePosition>,
    pub terrain_revisions: Vec<TerrainChunkRevision>,
    /// Every action in the tick's inputs, in input order, with its outcome.
    pub action_outcomes: Vec<ActionOutcome>,
    /// Every terrain tile write this tick, in order (the op-log replication section).
    pub terrain_ops: Vec<crate::terrain::TileChange>,
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
    command: &SequencedPlayerCommand,
) -> Option<GameplayEventKind> {
    let player_id = command.player_id;
    match command.command {
        PlayerCommand::Movement {
            drill_down: true, ..
        } => Some(GameplayEventKind::DrillStarted { player_id }),
        PlayerCommand::Refuel
        | PlayerCommand::Repair
        | PlayerCommand::BuyUpgrade { .. }
        | PlayerCommand::Service(_) => Some(GameplayEventKind::PurchaseCompleted { player_id }),
        PlayerCommand::SellCargo => Some(GameplayEventKind::CargoChanged { player_id }),
        PlayerCommand::PlaceBomb => Some(GameplayEventKind::BombPlaced { player_id }),
        PlayerCommand::PlaceInfrastructure { .. } => {
            Some(GameplayEventKind::InfrastructurePlaced { player_id })
        }
        PlayerCommand::UseScanner => Some(GameplayEventKind::ScannerUsed { player_id }),
        PlayerCommand::Rescue => Some(GameplayEventKind::RescueTriggered { player_id }),
        PlayerCommand::Movement { .. }
        | PlayerCommand::Interact
        | PlayerCommand::Cancel
        | PlayerCommand::Confirm
        | PlayerCommand::SelectUpgrade { .. } => None,
    }
}
