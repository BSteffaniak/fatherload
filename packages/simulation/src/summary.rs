//! Read-only summaries of canonical world and player state.

use serde::{Deserialize, Serialize};

use crate::{
    ids::{PlayerId, SimulationTick},
    player::Player,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuthoritativeWorldSummary {
    pub tick: SimulationTick,
    pub player_count: usize,
    pub terrain_width: i32,
    pub terrain_height: i32,
    pub hazard_count: usize,
    pub bomb_count: usize,
    pub infrastructure_count: usize,
    pub active_contract_count: usize,
    pub expedition_count: usize,
    pub market_salt: u32,
    pub won_game: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerInventorySummary {
    pub cargo_used: u32,
    pub cargo_capacity: u32,
    pub material_count: u32,
    pub artifact_count: u32,
    pub credits: u32,
    pub upgrade_level_total: u32,
}

impl PlayerInventorySummary {
    #[must_use]
    pub fn from_player(player: &Player) -> Self {
        Self {
            cargo_used: player.cargo_used(),
            cargo_capacity: player.cargo_capacity,
            material_count: player.materials.values().sum(),
            artifact_count: player.artifacts.values().sum(),
            credits: player.credits,
            upgrade_level_total: u32::from(player.fuel_tank_level)
                + u32::from(player.cargo_bay_level)
                + u32::from(player.drill_strength)
                + u32::from(player.engine_level)
                + u32::from(player.hull_level)
                + u32::from(player.radiator_level)
                + u32::from(player.scanner_level),
        }
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "proof object intentionally records independent player-scoped gameplay domains"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerScopedGameplayProof {
    pub movement_scoped: bool,
    pub drilling_scoped: bool,
    pub inventory_scoped: bool,
    pub survival_scoped: bool,
    pub scanner_scoped: bool,
    pub bomb_scoped: bool,
    pub infrastructure_scoped: bool,
    pub economy_scoped: bool,
}

impl PlayerScopedGameplayProof {
    #[must_use]
    pub const fn complete(self) -> bool {
        self.movement_scoped
            && self.drilling_scoped
            && self.inventory_scoped
            && self.survival_scoped
            && self.scanner_scoped
            && self.bomb_scoped
            && self.infrastructure_scoped
            && self.economy_scoped
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerSurvivalSnapshot {
    pub player_id: PlayerId,
    pub fuel: f32,
    pub hull: f32,
    pub disabled: bool,
    pub stranded: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerScopedCommandOutcome {
    Applied,
    IgnoredUnavailable,
    UnknownPlayer,
}
