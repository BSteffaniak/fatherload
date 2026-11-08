//! Gameplay commands submitted by players (intent, not raw input).

use serde::{Deserialize, Serialize};

use crate::ids::{InputSequence, PlayerId, SimulationTick};

/// Authoritative gameplay commands submitted by a player.
///
/// This intentionally represents gameplay intent rather than keyboard/gamepad state so the same
/// path can be used by local input, split-screen clients, online clients, replay, or AI.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum PlayerCommand {
    Movement {
        horizontal: f32,
        thrust: bool,
        drill_down: bool,
    },
    Interact,
    Cancel,
    Confirm,
    UseScanner,
    PlaceBomb,
    PlaceInfrastructure {
        slot: u8,
    },
    SelectUpgrade {
        index: usize,
    },
    BuyUpgrade {
        index: usize,
    },
    Refuel,
    Repair,
    SellCargo,
    Rescue,
}

/// Command packet metadata needed for future network acknowledgement and reconciliation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SequencedPlayerCommand {
    pub player_id: PlayerId,
    pub sequence: InputSequence,
    pub target_tick: SimulationTick,
    pub command: PlayerCommand,
}
