//! Gameplay commands submitted by players (intent, not raw input).

use serde::{Deserialize, Serialize};

use crate::{ids::InputSequence, service::SessionServiceRequest};

/// One-shot gameplay commands submitted by a player.
///
/// This represents gameplay intent rather than keyboard/gamepad state, so local seats, online
/// clients, replays and bots all submit the same commands. Held movement is not a command: it
/// travels in [`crate::ControlState`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PlayerCommand {
    UseScanner,
    PlaceBomb,
    PlaceInfrastructure {
        slot: u8,
    },
    BuyUpgrade {
        index: usize,
    },
    Refuel,
    Repair,
    SellCargo,
    Rescue,
    /// A town service transaction (shop, bank, salvage, contracts, expeditions, ...).
    Service(SessionServiceRequest),
}

/// A command submitted for one tick, with its position in that player's input for the tick.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TickAction {
    pub sequence: InputSequence,
    pub command: PlayerCommand,
}
