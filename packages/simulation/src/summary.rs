//! A rig's survival state and the outcome of a player-scoped command.

use serde::{Deserialize, Serialize};

use crate::ids::PlayerId;

/// Fuel and hull of one rig, and whether it is disabled or stranded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerSurvivalSnapshot {
    pub player_id: PlayerId,
    pub fuel: f32,
    pub hull: f32,
    pub disabled: bool,
    pub stranded: bool,
}

/// How the world resolved one player's command.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PlayerScopedCommandOutcome {
    Applied,
    IgnoredUnavailable,
    UnknownPlayer,
}
