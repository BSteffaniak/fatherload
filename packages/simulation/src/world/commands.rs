//! Player command application and drilling.

use std::collections::BTreeSet;

use crate::{
    command::PlayerCommand,
    economy::{buy_upgrade, refuel_amount, repair_amount, upgrade_locked_by, upgrade_offers},
    hazard::{HazardCloud, PlacedBomb},
    ids::{PlayerId, SimulationTick},
    infrastructure::{InfrastructureKind, PlacedInfrastructure},
    player::Player,
    progression::WorldEventKind,
    quantize::quantize_simulation_scalar,
    quest::QuestGiver,
    rig::{DrillDirection, DrillState, RigPartKind},
    service::{PlayerServiceTransaction, PlayerTransactionKind},
    summary::PlayerScopedCommandOutcome,
    terrain::{DeepStratum, MineResult, TILE_SIZE, Terrain, TileKind, TilePosition},
};

use super::{WorldState, movement::authoritative_player_collides, record_service_transaction};

/// Scanner cooldown after a scan.
const SCANNER_COOLDOWN_MILLIS: u64 = 1_000;
/// Fuse of a placed bomb.
const BOMB_FUSE_MILLIS: u64 = 2_400;
/// [`BOMB_FUSE_MILLIS`] in seconds, for the bomb's presentation timer.
const BOMB_FUSE_SECONDS: f32 = 2.4;

impl WorldState {
    pub(crate) fn chip_active_drill_target(&mut self, player_id: PlayerId) -> Option<MineResult> {
        let target = self.active_drills.get(&player_id)?.target;
        let result = self.terrain_mut().chip(target);
        if matches!(
            result,
            MineResult::Blocked
                | MineResult::TooDangerous
                | MineResult::Exploded
                | MineResult::Blast
                | MineResult::Mined(_)
                | MineResult::Activated(_)
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
            MineResult::Activated(_) => {
                player.fuel = quantize_simulation_scalar((player.fuel - 0.45).max(0.0));
            }
            MineResult::Blocked | MineResult::TooDangerous | MineResult::Chipped => {}
        }
        Some(result)
    }

    /// Updates a player's drill target from held controls (no velocity change).
    ///
    /// The stock rig only drills while resting on solid ground: down into the tile below, or
    /// sideways. The drill mount upgrade adds sideways drilling in flight (gimbal) and drilling
    /// up into a ceiling the rig thrusts against (crown bore).
    pub(crate) fn apply_drill_intent(
        &mut self,
        player_id: PlayerId,
        horizontal: f32,
        drill_down: bool,
        thrust: bool,
    ) -> PlayerScopedCommandOutcome {
        let parts = self
            .equipped_rig_parts
            .get(&player_id)
            .cloned()
            .unwrap_or_default();
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let contact = DrillContact::of(&self.terrain, player);
        let intent = DrillIntent {
            horizontal,
            drill_down,
            thrust: thrust && player.fuel > 0.0,
        };
        let mut chosen = authoritative_mine_target(player, intent, contact);
        if chosen.is_some_and(|(target, direction)| {
            direction.is_sideways()
                && self
                    .terrain
                    .tile(target)
                    .is_none_or(|tile| tile.kind == TileKind::Air)
        }) {
            // Steering past open air: a crown bore pressed into the ceiling still bores upward
            // (an analog stick is rarely dead centre while thrusting).
            chosen = authoritative_mine_target(
                player,
                DrillIntent {
                    horizontal: 0.0,
                    ..intent
                },
                contact,
            );
        }
        if let Some((target, direction)) = chosen {
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
                &self.terrain,
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
    pub(crate) fn apply_player_command(
        &mut self,
        player_id: PlayerId,
        command: &PlayerCommand,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };

        match *command {
            PlayerCommand::Refuel => {
                let credits_before = player.credits;
                let cargo_before = player.cargo_used();
                let spent = refuel_amount(player, 1.0);
                let rebate = spent.saturating_mul(rebate_percent(player, QuestGiver::Pip)) / 100;
                player.credits = player.credits.saturating_add(rebate);
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
                let spent = repair_amount(player, 1.0);
                let rebate = spent.saturating_mul(rebate_percent(player, QuestGiver::Iona)) / 100;
                player.credits = player.credits.saturating_add(rebate);
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
                let _contacts = self.ping_pois(player_id, center);
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
                PlayerScopedCommandOutcome::Applied
            }
            PlayerCommand::BuyUpgrade { index } => {
                if upgrade_offers(player).get(index).is_some_and(|offer| {
                    upgrade_locked_by(
                        offer,
                        self.terrain.settings(),
                        self.progression.deepest_tile_reached,
                    )
                    .is_some()
                }) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
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
            PlayerCommand::TestMode(action) => self.apply_test_mode_action(player_id, action),
        }
    }
}

/// Held drilling controls for one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrillIntent {
    /// Horizontal stick, `-1..=1`.
    pub horizontal: f32,
    pub drill_down: bool,
    /// Thrust held with fuel to burn (the "up" control).
    pub thrust: bool,
}

/// What the rig is touching this tick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrillContact {
    /// Resting on solid ground.
    pub grounded: bool,
    /// Pressed against a solid ceiling.
    pub ceiling: bool,
}

impl DrillContact {
    /// The contacts of `player`'s rig in `terrain`.
    #[must_use]
    pub fn of(terrain: &Terrain, player: &Player) -> Self {
        Self {
            grounded: authoritative_player_collides(terrain, player.x, player.y + CONTACT_PROBE),
            ceiling: authoritative_player_collides(terrain, player.x, player.y - CONTACT_PROBE),
        }
    }
}

/// How far (px) below or above the rig's collision box a solid tile still counts as touching.
const CONTACT_PROBE: f32 = 2.0;

/// The tile `player` drills this tick, and in which direction, from the held `intent` and the
/// rig's `contact`s.
///
/// Down needs ground contact and wins over sideways; sideways needs ground contact unless the
/// rig has the gimbal mount; up needs the crown bore and a ceiling the rig is thrusting into.
#[must_use]
pub fn authoritative_mine_target(
    player: &Player,
    intent: DrillIntent,
    contact: DrillContact,
) -> Option<(TilePosition, DrillDirection)> {
    let current_tile = player.tile_position(TILE_SIZE);
    let at = |dx: i32, dy: i32| TilePosition {
        x: current_tile.x + dx,
        y: current_tile.y + dy,
    };
    if intent.drill_down {
        return contact.grounded.then(|| (at(0, 1), DrillDirection::Down));
    }
    if intent.horizontal.abs() > f32::EPSILON
        && (contact.grounded || player.drills_sideways_in_flight())
    {
        return Some(if intent.horizontal < 0.0 {
            (at(-1, 0), DrillDirection::Left)
        } else {
            (at(1, 0), DrillDirection::Right)
        });
    }
    if intent.thrust && contact.ceiling && player.drills_upward() {
        return Some((at(0, -1), DrillDirection::Up));
    }
    None
}

pub(super) fn authoritative_drill_seconds_per_chip(
    kind: TileKind,
    drill_strength: u8,
    direction: DrillDirection,
    parts: &BTreeSet<RigPartKind>,
    terrain: &Terrain,
    target: TilePosition,
) -> f32 {
    let base = match kind {
        TileKind::Air => 0.0,
        TileKind::Dirt => 0.09,
        TileKind::Clay => 0.12,
        TileKind::Stone => 0.15,
        TileKind::HardRock | TileKind::Foundation | TileKind::Bedrock => 0.19,
        TileKind::Lava
        | TileKind::Gas
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket
        | TileKind::MagmaVent => 0.08,
        TileKind::Ore(_) => 0.16,
        TileKind::Artifact(_) => 0.21,
        TileKind::Poi(tile) => match tile {
            crate::poi::PoiTile::SupplyCrate
            | crate::poi::PoiTile::VaultSwitch { .. }
            | crate::poi::PoiTile::VaultRelay { .. } => 0.12,
            crate::poi::PoiTile::GeodeShell => 0.2,
            _ => 0.16,
        },
    };
    let effective_drill_strength = if parts.contains(&RigPartKind::TitanDrill)
        || parts.contains(&RigPartKind::ResonanceDrill)
    {
        drill_strength.saturating_add(1)
    } else {
        drill_strength
    };
    let drill_bonus = 1.0 + f32::from(effective_drill_strength.saturating_sub(1)) * 0.4;
    let direction_penalty = match direction {
        DrillDirection::Down => 1.0,
        DrillDirection::Left | DrillDirection::Right => 1.45,
        // Boring overhead fights gravity and falling spoil.
        DrillDirection::Up => 1.7,
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
            terrain.deep_stratum_at(target),
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

/// The reputation rebate (percent) `giver` hands `player`.
fn rebate_percent(player: &crate::player::Player, giver: QuestGiver) -> u32 {
    giver.rebate_percent(player.quests.rank(giver))
}
