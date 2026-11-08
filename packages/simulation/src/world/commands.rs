//! Player command application and drilling.

use std::collections::BTreeSet;

use crate::{
    command::PlayerCommand,
    economy::{buy_upgrade, refuel_amount, repair_amount},
    hazard::{HazardCloud, PlacedBomb},
    ids::{PlayerId, SimulationTick},
    infrastructure::{InfrastructureKind, PlacedInfrastructure},
    player::Player,
    progression::WorldEventKind,
    quantize::quantize_simulation_scalar,
    rig::{DrillDirection, DrillState, RigPartKind},
    service::{PlayerServiceTransaction, PlayerTransactionKind},
    summary::{PlayerScopedCommandOutcome, PlayerScopedGameplayProof},
    terrain::{DeepStratum, MineResult, TILE_SIZE, TileKind, TilePosition, deep_stratum_at_depth},
};

use super::{WorldState, record_service_transaction};

/// Scanner cooldown after a scan.
const SCANNER_COOLDOWN_MILLIS: u64 = 1_000;
/// Fuse of a placed bomb.
const BOMB_FUSE_MILLIS: u64 = 2_400;
/// [`BOMB_FUSE_MILLIS`] in seconds, for the bomb's presentation timer.
const BOMB_FUSE_SECONDS: f32 = 2.4;

impl WorldState {
    pub fn player_scoped_gameplay_proof(
        &mut self,
        first_player_id: PlayerId,
        second_player_id: PlayerId,
    ) -> Option<PlayerScopedGameplayProof> {
        self.active_drills.remove(&first_player_id);
        self.active_drills.remove(&second_player_id);
        let first_before = self.players.get(&first_player_id)?.clone();
        let second_before = self.players.get(&second_player_id)?.clone();

        let movement_scoped = self.apply_player_command(
            first_player_id,
            &PlayerCommand::Movement {
                horizontal: 0.75,
                thrust: true,
                drill_down: false,
            },
        ) == PlayerScopedCommandOutcome::Applied
            && (self.players.get(&first_player_id)?.velocity_x - first_before.velocity_x).abs()
                > f32::EPSILON
            && (self.players.get(&second_player_id)?.velocity_x - second_before.velocity_x).abs()
                < f32::EPSILON;

        let drilling_scoped = self.apply_player_command(
            first_player_id,
            &PlayerCommand::Movement {
                horizontal: 0.0,
                thrust: false,
                drill_down: true,
            },
        ) == PlayerScopedCommandOutcome::Applied
            && self.active_drills.contains_key(&first_player_id)
            && !self.active_drills.contains_key(&second_player_id);

        let inventory_before = self.player_inventory_summary(second_player_id)?;
        let scanner_before = self
            .scanner_cooldown_seconds(second_player_id)
            .unwrap_or(0.0);
        let bomb_count_before = self.bombs.len();
        let infrastructure_count_before = self.infrastructure.len();
        let transaction_count_before = self.service_transactions.len();

        let second = self.players.get_mut(&second_player_id)?;
        second.fuel = 0.0;
        second.hull = 0.0;
        second.credits = second.credits.saturating_add(500);
        second.bombs = second.bombs.saturating_add(1);
        second.signal_relay_kits = second.signal_relay_kits.saturating_add(1);
        let survival_before = self.player_survival_snapshot(second_player_id)?;

        let survival_scoped = self.apply_player_command(second_player_id, &PlayerCommand::Rescue)
            == PlayerScopedCommandOutcome::Applied
            && self.player_survival_snapshot(second_player_id)? != survival_before
            && self.player_survival_snapshot(first_player_id)?.player_id == first_player_id;
        self.players
            .get_mut(&second_player_id)?
            .cargo
            .insert(crate::terrain::MineralKind::Copper, 1);
        let scanner_scoped = self
            .apply_player_command(second_player_id, &PlayerCommand::UseScanner)
            == PlayerScopedCommandOutcome::Applied
            && self
                .scanner_cooldown_seconds(second_player_id)
                .unwrap_or(0.0)
                > scanner_before
            && self
                .scanner_cooldown_seconds(first_player_id)
                .unwrap_or(0.0)
                .abs()
                < f32::EPSILON;
        let bomb_scoped = self.apply_player_command(second_player_id, &PlayerCommand::PlaceBomb)
            == PlayerScopedCommandOutcome::Applied
            && self.bombs.len() == bomb_count_before + 1;
        let infrastructure_scoped = self.apply_player_command(
            second_player_id,
            &PlayerCommand::PlaceInfrastructure { slot: 0 },
        ) == PlayerScopedCommandOutcome::Applied
            && self.infrastructure.len() == infrastructure_count_before + 1;
        let economy_scoped = self.apply_player_command(second_player_id, &PlayerCommand::SellCargo)
            == PlayerScopedCommandOutcome::Applied
            && self.service_transactions.len() > transaction_count_before;
        let inventory_scoped = self.player_inventory_summary(second_player_id)? != inventory_before;

        Some(PlayerScopedGameplayProof {
            movement_scoped,
            drilling_scoped,
            inventory_scoped,
            survival_scoped,
            scanner_scoped,
            bomb_scoped,
            infrastructure_scoped,
            economy_scoped,
        })
    }

    pub fn chip_active_drill_target(&mut self, player_id: PlayerId) -> Option<MineResult> {
        let target = self.active_drills.get(&player_id)?.target;
        let result = self.terrain_mut().chip(target);
        if matches!(
            result,
            MineResult::Blocked
                | MineResult::TooDangerous
                | MineResult::Exploded
                | MineResult::Blast
                | MineResult::Mined(_)
        ) {
            self.active_drills.remove(&player_id);
        }
        let player = self.players.get_mut(&player_id)?;
        match result {
            MineResult::Mined(mined) => {
                player.fuel = quantize_simulation_scalar((player.fuel - 0.45).max(0.0));
                match mined {
                    TileKind::Ore(mineral) => {
                        let _added = player.add_cargo(mineral);
                    }
                    TileKind::Artifact(artifact) => {
                        let _added = player.add_artifact(artifact);
                    }
                    TileKind::PressurePocket => {
                        player.velocity_y = -360.0;
                        player.velocity_x = quantize_simulation_scalar(player.velocity_x * 1.4);
                        player.hull = quantize_simulation_scalar((player.hull - 10.0).max(0.0));
                    }
                    _ => {}
                }
            }
            MineResult::Exploded => {
                player.fuel = quantize_simulation_scalar((player.fuel - 0.45).max(0.0));
                player.velocity_x = quantize_simulation_scalar(player.velocity_x * -0.25);
                player.velocity_y = -90.0;
                let protected = self.infrastructure.iter().any(|item| {
                    item.kind == InfrastructureKind::PumpStation
                        && (item.position.x - target.x).abs() <= 5
                        && (item.position.y - target.y).abs() <= 5
                });
                if !protected {
                    self.hazards.push(HazardCloud {
                        x: player.x,
                        y: player.y + TILE_SIZE,
                        life: 8.0,
                        radius: 10.0,
                    });
                }
            }
            MineResult::Blast => {
                player.fuel = quantize_simulation_scalar((player.fuel - 0.9).max(0.0));
                player.hull = quantize_simulation_scalar((player.hull - 24.0).max(0.0));
                player.velocity_x = quantize_simulation_scalar(player.velocity_x * -0.7);
                player.velocity_y = -260.0;
            }
            MineResult::Blocked | MineResult::TooDangerous | MineResult::Chipped => {}
        }
        Some(result)
    }

    /// Updates a player's drill target from held controls (no velocity change).
    pub fn apply_drill_intent(
        &mut self,
        player_id: PlayerId,
        horizontal: f32,
        drill_down: bool,
    ) -> PlayerScopedCommandOutcome {
        let parts = self
            .equipped_rig_parts
            .get(&player_id)
            .cloned()
            .unwrap_or_default();
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if let Some((target, direction)) = authoritative_mine_target(player, horizontal, drill_down)
        {
            let Some(tile) = self.terrain.tile(target) else {
                self.set_active_drill(player_id, None);
                return PlayerScopedCommandOutcome::Applied;
            };
            if tile.kind == TileKind::Air {
                self.set_active_drill(player_id, None);
                return PlayerScopedCommandOutcome::Applied;
            }
            let effective_drill_strength = if parts.contains(&RigPartKind::TitanDrill)
                || parts.contains(&RigPartKind::ResonanceDrill)
            {
                player.drill_strength.saturating_add(1)
            } else {
                player.drill_strength
            };
            if self
                .terrain
                .hardness_at(target)
                .is_some_and(|hardness| hardness > effective_drill_strength)
            {
                self.set_active_drill(player_id, None);
                return PlayerScopedCommandOutcome::Applied;
            }
            let initial_durability = tile.durability.max(1);
            let seconds_per_chip = authoritative_drill_seconds_per_chip(
                tile.kind,
                player.drill_strength,
                direction,
                &parts,
                target,
            );
            let drill = self
                .active_drills
                .get(&player_id)
                .filter(|drill| drill.target == target && drill.direction == direction)
                .map_or_else(
                    || {
                        authoritative_drill_state(
                            target,
                            direction,
                            initial_durability,
                            seconds_per_chip,
                        )
                    },
                    |drill| DrillState {
                        initial_durability,
                        seconds_per_chip,
                        ..*drill
                    },
                );
            self.set_active_drill(player_id, Some(drill));
        } else {
            self.set_active_drill(player_id, None);
        }
        PlayerScopedCommandOutcome::Applied
    }

    #[allow(
        clippy::too_many_lines,
        reason = "compatibility command bridge covers all player-scoped intents until real systems split out"
    )]
    pub fn apply_player_command(
        &mut self,
        player_id: PlayerId,
        command: &PlayerCommand,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };

        match *command {
            PlayerCommand::Movement {
                horizontal,
                thrust,
                drill_down,
            } => {
                // Legacy direct-command bridge: `simulate_tick` integrates held controls with
                // momentum instead (see `apply_drill_intent` and `step_authoritative_movement`).
                player.velocity_x = horizontal;
                if thrust {
                    player.velocity_y = -1.0;
                }
                self.apply_drill_intent(player_id, horizontal, drill_down)
            }
            PlayerCommand::Refuel => {
                let credits_before = player.credits;
                let cargo_before = player.cargo_used();
                refuel_amount(player, 1.0);
                record_service_transaction(
                    &mut self.service_transactions,
                    PlayerServiceTransaction {
                        player_id,
                        kind: PlayerTransactionKind::Refuel,
                        credits_before,
                        credits_after: player.credits,
                        cargo_before,
                        cargo_after: player.cargo_used(),
                    },
                );
                PlayerScopedCommandOutcome::Applied
            }
            PlayerCommand::Repair => {
                let credits_before = player.credits;
                let cargo_before = player.cargo_used();
                repair_amount(player, 1.0);
                record_service_transaction(
                    &mut self.service_transactions,
                    PlayerServiceTransaction {
                        player_id,
                        kind: PlayerTransactionKind::Repair,
                        credits_before,
                        credits_after: player.credits,
                        cargo_before,
                        cargo_after: player.cargo_used(),
                    },
                );
                PlayerScopedCommandOutcome::Applied
            }
            PlayerCommand::SellCargo => self.sell_canonical_cargo(player_id),
            PlayerCommand::UseScanner => {
                if self
                    .world_event_severity
                    .contains_key(&WorldEventKind::DeepPressureStorm)
                {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let deadline = SimulationTick::new(
                    self.simulation_tick
                        .get()
                        .saturating_add(self.timing.ticks_for_millis(SCANNER_COOLDOWN_MILLIS)),
                );
                self.scanner_cooldowns.insert(player_id, 1.0);
                self.scanner_cooldown_deadlines.insert(player_id, deadline);
                let center = player.tile_position(TILE_SIZE);
                let _revealed = self.reveal_scanner_area(player_id, center, 4);
                PlayerScopedCommandOutcome::Applied
            }
            PlayerCommand::PlaceBomb => {
                if player.bombs == 0 {
                    PlayerScopedCommandOutcome::IgnoredUnavailable
                } else {
                    player.bombs -= 1;
                    self.bombs.push(PlacedBomb {
                        x: player.x,
                        y: TILE_SIZE.mul_add(0.4, player.y),
                        timer_seconds: BOMB_FUSE_SECONDS,
                    });
                    self.bomb_detonation_ticks.push(SimulationTick::new(
                        self.simulation_tick
                            .get()
                            .saturating_add(self.timing.ticks_for_millis(BOMB_FUSE_MILLIS)),
                    ));
                    self.authoritative_summary.bomb_count = self.bombs.len();
                    PlayerScopedCommandOutcome::Applied
                }
            }
            PlayerCommand::PlaceInfrastructure { slot } => {
                let Some(kind) = infrastructure_kind_for_slot(slot) else {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                };
                if !consume_infrastructure_kit(player, kind) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                self.infrastructure.push(PlacedInfrastructure {
                    kind,
                    position: player.tile_position(TILE_SIZE),
                    durability: 100,
                });
                self.authoritative_summary.infrastructure_count = self.infrastructure.len();
                PlayerScopedCommandOutcome::Applied
            }
            PlayerCommand::BuyUpgrade { index } => {
                let credits_before = player.credits;
                let cargo_before = player.cargo_used();
                if buy_upgrade(player, index).is_err() {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                record_service_transaction(
                    &mut self.service_transactions,
                    PlayerServiceTransaction {
                        player_id,
                        kind: PlayerTransactionKind::BuyUpgrade,
                        credits_before,
                        credits_after: player.credits,
                        cargo_before,
                        cargo_after: player.cargo_used(),
                    },
                );
                PlayerScopedCommandOutcome::Applied
            }
            PlayerCommand::Rescue => self.canonical_rescue_player(player_id),
            PlayerCommand::Service(request) => self.apply_service_request(player_id, request),
            PlayerCommand::Interact
            | PlayerCommand::Cancel
            | PlayerCommand::Confirm
            | PlayerCommand::SelectUpgrade { .. } => PlayerScopedCommandOutcome::IgnoredUnavailable,
        }
    }
}

#[must_use]
pub fn authoritative_mine_target(
    player: &Player,
    horizontal: f32,
    drill_down: bool,
) -> Option<(TilePosition, DrillDirection)> {
    if !drill_down && horizontal.abs() <= f32::EPSILON {
        return None;
    }

    let current_tile = player.tile_position(TILE_SIZE);
    if drill_down {
        Some((
            TilePosition {
                x: current_tile.x,
                y: current_tile.y + 1,
            },
            DrillDirection::Down,
        ))
    } else if horizontal < 0.0 {
        Some((
            TilePosition {
                x: current_tile.x - 1,
                y: current_tile.y,
            },
            DrillDirection::Left,
        ))
    } else {
        Some((
            TilePosition {
                x: current_tile.x + 1,
                y: current_tile.y,
            },
            DrillDirection::Right,
        ))
    }
}

pub(super) fn authoritative_drill_seconds_per_chip(
    kind: TileKind,
    drill_strength: u8,
    direction: DrillDirection,
    parts: &BTreeSet<RigPartKind>,
    target: TilePosition,
) -> f32 {
    let base = match kind {
        TileKind::Air => 0.0,
        TileKind::Dirt => 0.09,
        TileKind::Clay => 0.12,
        TileKind::Stone => 0.15,
        TileKind::HardRock | TileKind::Foundation => 0.19,
        TileKind::Lava
        | TileKind::Gas
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket
        | TileKind::MagmaVent => 0.08,
        TileKind::Ore(_) => 0.16,
        TileKind::Artifact(_) => 0.21,
    };
    let effective_drill_strength = if parts.contains(&RigPartKind::TitanDrill)
        || parts.contains(&RigPartKind::ResonanceDrill)
    {
        drill_strength.saturating_add(1)
    } else {
        drill_strength
    };
    let drill_bonus = 1.0 + f32::from(effective_drill_strength.saturating_sub(1)) * 0.4;
    let direction_penalty = if direction == DrillDirection::Down {
        1.0
    } else {
        1.45
    };
    let mut seconds = base * direction_penalty / drill_bonus;
    if parts.contains(&RigPartKind::NeedleDrill) {
        seconds *= match kind {
            TileKind::Dirt | TileKind::Clay => 0.72,
            TileKind::Stone | TileKind::HardRock => 1.25,
            _ => 0.95,
        };
    }
    if parts.contains(&RigPartKind::TitanDrill) {
        seconds *= 1.10;
    }
    if parts.contains(&RigPartKind::ResonanceDrill)
        && matches!(
            deep_stratum_at_depth(target.y),
            Some(DeepStratum::CrystalFaults | DeepStratum::AncientMachineLayer)
        )
    {
        seconds *= 0.65;
    }
    seconds.max(0.045)
}

pub(super) const fn authoritative_drill_state(
    target: TilePosition,
    direction: DrillDirection,
    initial_durability: u8,
    seconds_per_chip: f32,
) -> DrillState {
    DrillState {
        target,
        direction,
        progress: 0.0,
        initial_durability,
        seconds_per_chip,
        sound_timer: 0.0,
        dust_timer: 0.0,
    }
}

pub(super) const fn bomb_tile_position(bomb: PlacedBomb) -> TilePosition {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "bomb world positions intentionally convert to terrain tile coordinates"
    )]
    let position = TilePosition {
        x: (bomb.x / TILE_SIZE).floor() as i32,
        y: (bomb.y / TILE_SIZE).floor() as i32,
    };
    position
}

pub(super) const fn infrastructure_kind_for_slot(slot: u8) -> Option<InfrastructureKind> {
    match slot {
        0 => Some(InfrastructureKind::SignalRelay),
        1 => Some(InfrastructureKind::SurveyDrone),
        2 => Some(InfrastructureKind::CargoLift),
        3 => Some(InfrastructureKind::TunnelSupport),
        4 => Some(InfrastructureKind::PumpStation),
        5 => Some(InfrastructureKind::OreProcessor),
        _ => None,
    }
}

pub(super) const fn consume_infrastructure_kit(
    player: &mut Player,
    kind: InfrastructureKind,
) -> bool {
    let kit_count = match kind {
        InfrastructureKind::SignalRelay => &mut player.signal_relay_kits,
        InfrastructureKind::SurveyDrone => &mut player.survey_drone_kits,
        InfrastructureKind::CargoLift => &mut player.cargo_lift_kits,
        InfrastructureKind::TunnelSupport => &mut player.tunnel_support_kits,
        InfrastructureKind::PumpStation => &mut player.pump_station_kits,
        InfrastructureKind::OreProcessor => &mut player.ore_processor_kits,
    };
    if *kit_count == 0 {
        return false;
    }
    *kit_count -= 1;
    true
}
