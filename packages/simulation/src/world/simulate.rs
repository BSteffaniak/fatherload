//! The fixed-tick [`WorldState::simulate_tick`] transition.

use std::{collections::BTreeSet, mem};

use crate::{
    command::PlayerCommand,
    event::{ActionOutcome, GameplayEventKind, TickOutput, gameplay_event_for_applied_command},
    ids::{PlayerId, SimulationTick},
    infrastructure::InfrastructureKind,
    input::TickInputs,
    progression::WorldEventKind,
    quantize::quantize_simulation_scalar,
    rig::RigPartKind,
    summary::PlayerScopedCommandOutcome,
    terrain::{
        ArtifactKind, MineResult, MineralKind, StrategicResourceKind, TILE_SIZE, TileKind,
        TilePosition,
    },
    terrain_revision::TerrainRevisionTracker,
};

use super::{
    ActiveDrillTimer, WorldState,
    commands::bomb_tile_position,
    movement::{
        AUTHORITATIVE_BOULDER_LIFE_MILLIS, AUTHORITATIVE_BOULDER_SPAWN_CHANCE,
        AUTHORITATIVE_BOULDER_WARNING_MILLIS,
    },
};

/// Time the rigs have to escape after the star core is mined.
const ESCAPE_DURATION_MILLIS: u64 = 120_000;
/// Period of the cave-ins chasing the rigs during the escape.
const ESCAPE_CAVE_IN_PERIOD_MILLIS: u64 = 750;
/// Period of the escape-tunnel collapse cycle.
const ESCAPE_COLLAPSE_PERIOD_MILLIS: u64 = 1_500;
/// Point in each collapse cycle at which the next collapse is announced.
const ESCAPE_COLLAPSE_WARNING_PHASE_MILLIS: u64 = 1_250;

impl WorldState {
    /// Advances the canonical world exactly one fixed tick.
    ///
    /// # Panics
    ///
    /// Panics only if internal player bookkeeping is inconsistent (a player id collected from the
    /// world during this tick disappears before it is processed), which would be a simulation bug.
    #[allow(
        clippy::too_many_lines,
        reason = "canonical transition keeps tick ordering explicit while gameplay domains are extracted"
    )]
    pub fn simulate_tick(
        &mut self,
        tick: SimulationTick,
        inputs: &TickInputs,
        terrain_revisions: &mut TerrainRevisionTracker,
    ) -> TickOutput {
        let mut output = TickOutput {
            tick,
            command_count: inputs.command_count(),
            ..TickOutput::default()
        };
        let timing = self.timing;
        let delta_seconds = timing.delta_seconds;
        self.terrain.start_journal();
        let mut changed_players = BTreeSet::new();
        for (&player_id, input) in &inputs.players {
            if let Some((sequence, controls)) = input.controls {
                if self.apply_drill_intent(player_id, controls.horizontal(), controls.drill_down)
                    == PlayerScopedCommandOutcome::Applied
                {
                    changed_players.insert(player_id);
                }
                let _step = self.step_authoritative_movement(
                    player_id,
                    controls.horizontal(),
                    controls.thrust,
                );
                output
                    .latest_sequences_by_player
                    .insert(player_id, sequence);
            }
            for action in &input.actions {
                let outcome = if matches!(action.command, PlayerCommand::Movement { .. }) {
                    // Held movement travels in `controls`; a movement action is not a one-shot.
                    PlayerScopedCommandOutcome::IgnoredUnavailable
                } else {
                    self.apply_player_command(player_id, &action.command)
                };
                output.action_outcomes.push(ActionOutcome {
                    player_id,
                    sequence: action.sequence,
                    outcome,
                });
                if outcome == PlayerScopedCommandOutcome::Applied {
                    changed_players.insert(player_id);
                    if let Some(event) = gameplay_event_for_applied_command(action) {
                        output.push_event(event);
                    }
                }
                output
                    .latest_sequences_by_player
                    .entry(player_id)
                    .and_modify(|sequence| *sequence = (*sequence).max(action.sequence))
                    .or_insert(action.sequence);
            }
        }
        let next_tick = tick.next();
        let recovery_player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in recovery_player_ids {
            if self.recover_lost_cargo_for_player(player_id) {
                changed_players.insert(player_id);
                output.push_event(GameplayEventKind::CargoChanged { player_id });
            }
        }
        for (player_id, deadline) in &self.scanner_cooldown_deadlines {
            self.scanner_cooldowns
                .insert(*player_id, timing.rate.seconds_until(next_tick, *deadline));
        }

        for cloud in &mut self.hazards {
            cloud.life -= delta_seconds;
            cloud.radius = 8.0f32.mul_add(delta_seconds, cloud.radius);
        }
        self.hazards.retain(|cloud| cloud.life > 0.0);
        let player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in player_ids {
            let damaged = self.players.get(&player_id).is_some_and(|player| {
                self.hazards.iter().any(|cloud| {
                    if cloud.life >= 6.0 {
                        return false;
                    }
                    let dx = player.x - cloud.x;
                    let dy = player.y - cloud.y;
                    dx.mul_add(dx, dy * dy) <= cloud.radius * cloud.radius
                })
            });
            if damaged {
                let _snapshot = self.apply_hazard_damage(player_id, 4.0 * delta_seconds);
                changed_players.insert(player_id);
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
        }
        self.authoritative_summary.hazard_count = self.hazards.len();

        let player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in player_ids {
            let (player_x, player_y, radiator_level) = self
                .players
                .get(&player_id)
                .map(|player| (player.x, player.y, player.radiator_level))
                .expect("collected player id remains valid");
            let parts = self
                .equipped_rig_parts
                .get(&player_id)
                .cloned()
                .unwrap_or_default();
            let safe_depth = 18.0f32.mul_add(
                TILE_SIZE,
                f32::from(radiator_level.saturating_sub(1)) * 12.0 * TILE_SIZE,
            );
            #[allow(
                clippy::cast_precision_loss,
                reason = "small bounded event severity intentionally scales heat damage"
            )]
            let severity = self
                .world_event_severity
                .get(&WorldEventKind::HeatWave)
                .copied()
                .unwrap_or(0) as f32;
            let mut heat_multiplier = severity.mul_add(0.18, 1.0);
            if parts.contains(&RigPartKind::ThermalHull) {
                heat_multiplier *= 0.55;
            }
            if parts.contains(&RigPartKind::ImpactHull) {
                heat_multiplier *= 1.18;
            }
            if parts.contains(&RigPartKind::PressureHull) && player_y >= 90.0 * TILE_SIZE {
                heat_multiplier *= 0.72;
            }
            let depth_damage = if player_y > safe_depth {
                let depth_factor = ((player_y - safe_depth) / (12.0 * TILE_SIZE)).max(1.0);
                3.5 * depth_factor * heat_multiplier * delta_seconds
            } else {
                0.0
            };
            #[allow(
                clippy::cast_possible_truncation,
                reason = "bounded player coordinates intentionally map to terrain tiles"
            )]
            let center = TilePosition {
                x: (player_x / TILE_SIZE).floor() as i32,
                y: (player_y / TILE_SIZE).floor() as i32,
            };
            let near_lava = (-2..=2).any(|dy| {
                (-2..=2).any(|dx| {
                    self.terrain.is_lava_at(TilePosition {
                        x: center.x + dx,
                        y: center.y + dy,
                    })
                })
            });
            let pump_protected = self.infrastructure.iter().any(|item| {
                item.kind == InfrastructureKind::PumpStation
                    && (item.position.x - center.x).abs() <= 4
                    && (item.position.y - center.y).abs() <= 4
            });
            let lava_damage = if near_lava {
                (if pump_protected { 2.5 } else { 9.0 }) * heat_multiplier * delta_seconds
            } else {
                0.0
            };
            let total_damage = depth_damage + lava_damage;
            if total_damage > 0.0 {
                let _snapshot = self.apply_hazard_damage(player_id, total_damage);
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.hull = quantize_simulation_scalar(player.hull);
                }
                changed_players.insert(player_id);
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
        }

        let bombs = mem::take(&mut self.bombs);
        let deadlines = mem::take(&mut self.bomb_detonation_ticks);
        for (index, bomb) in bombs.into_iter().enumerate() {
            let deadline = deadlines.get(index).copied().unwrap_or_else(|| {
                SimulationTick::new(
                    tick.get()
                        .saturating_add(timing.rate.ticks_for_seconds(bomb.timer_seconds)),
                )
            });
            if next_tick >= deadline {
                let blast = self.detonate_bomb_at(bomb_tile_position(bomb), 2);
                output.terrain_changed_tiles.extend(blast.changed_tiles);
                output.push_event(GameplayEventKind::BombDetonated);
            } else {
                let mut bomb = bomb;
                bomb.timer_seconds = timing.rate.seconds_until(next_tick, deadline);
                self.bombs.push(bomb);
                self.bomb_detonation_ticks.push(deadline);
            }
        }
        self.authoritative_summary.bomb_count = self.bombs.len();
        if !output.terrain_changed_tiles.is_empty() {
            output.terrain_changed_tiles.sort_unstable();
            output.terrain_changed_tiles.dedup();
            output.terrain_revisions.extend(
                terrain_revisions.mark_tiles_changed(output.terrain_changed_tiles.iter().copied()),
            );
        }

        let active_players: Vec<PlayerId> = self.active_drills.keys().copied().collect();
        for player_id in active_players {
            let period_ticks = self.active_drills.get(&player_id).map_or(1, |drill| {
                timing.rate.ticks_for_seconds(drill.seconds_per_chip)
            });
            let timer = self
                .active_drill_timers
                .entry(player_id)
                .or_insert_with(|| ActiveDrillTimer {
                    next_chip_tick: SimulationTick::new(tick.get().saturating_add(period_ticks)),
                    period_ticks,
                });
            timer.period_ticks = period_ticks;
            let ticks_until_chip = timer.next_chip_tick.get().saturating_sub(next_tick.get());
            if let Some(drill) = self.active_drills.get_mut(&player_id) {
                let elapsed = period_ticks.saturating_sub(ticks_until_chip.min(period_ticks));
                let elapsed_u16 = u16::try_from(elapsed).unwrap_or(u16::MAX);
                let period_u16 = u16::try_from(period_ticks).unwrap_or(u16::MAX).max(1);
                drill.progress = f32::from(elapsed_u16) / f32::from(period_u16);
            }
            if next_tick < timer.next_chip_tick {
                continue;
            }
            timer.next_chip_tick = SimulationTick::new(
                timer
                    .next_chip_tick
                    .get()
                    .saturating_add(timer.period_ticks),
            );
            if let Some(drill) = self.active_drills.get_mut(&player_id) {
                drill.progress = 0.0;
            }
            let Some(target) = self.active_drill(player_id).map(|drill| drill.target) else {
                continue;
            };
            let Some(result) = self.chip_active_drill_target(player_id) else {
                continue;
            };
            if matches!(
                result,
                MineResult::Blocked
                    | MineResult::TooDangerous
                    | MineResult::Exploded
                    | MineResult::Blast
                    | MineResult::Mined(_)
            ) {
                self.active_drill_timers.remove(&player_id);
            }
            output.terrain_changed_tiles.push(target);
            let revisions = terrain_revisions.mark_tiles_changed([target]);
            output.terrain_revisions.extend(revisions);
            match result {
                MineResult::Mined(TileKind::Ore(_)) => {
                    output.push_event(GameplayEventKind::CargoChanged { player_id });
                }
                MineResult::Mined(TileKind::Artifact(ArtifactKind::StarCore)) => {
                    self.progression.escape_deadline =
                        Some(SimulationTick::new(next_tick.get().saturating_add(
                            timing.ticks_for_millis(ESCAPE_DURATION_MILLIS),
                        )));
                    output.push_event(GameplayEventKind::CargoChanged { player_id });
                    output.push_event(GameplayEventKind::EscapeStarted { player_id });
                }
                MineResult::Mined(TileKind::Stone | TileKind::HardRock) => {
                    #[allow(
                        clippy::cast_precision_loss,
                        clippy::cast_sign_loss,
                        clippy::suboptimal_flops,
                        reason = "terrain coordinates deterministically reproduce the legacy hazard schedule"
                    )]
                    let (value, boulder_x, boulder_y) = (
                        self.terrain.seed()
                            ^ ((target.x as u64).wrapping_mul(0x9E37))
                            ^ ((target.y as u64).wrapping_mul(0x85EB)),
                        target.x as f32 * TILE_SIZE + TILE_SIZE * 0.5,
                        (target.y as f32 - 1.0) * TILE_SIZE,
                    );
                    if value.is_multiple_of(AUTHORITATIVE_BOULDER_SPAWN_CHANCE) {
                        self.spawn_falling_boulder(
                            boulder_x,
                            boulder_y,
                            AUTHORITATIVE_BOULDER_WARNING_MILLIS,
                            AUTHORITATIVE_BOULDER_LIFE_MILLIS,
                        );
                        output.push_event(GameplayEventKind::BouldersChanged);
                    }
                }
                MineResult::TooDangerous => {
                    let _snapshot = self.apply_hazard_damage(player_id, 8.0);
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                }
                MineResult::Exploded => {
                    output.push_event(GameplayEventKind::HazardChanged);
                }
                MineResult::Blast => {
                    if let Some(player) = self.players.get(&player_id) {
                        let (player_x, player_y) = (player.x, player.y);
                        self.spawn_cave_in(player_x, player_y);
                    }
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                    output.push_event(GameplayEventKind::HazardChanged);
                    output.push_event(GameplayEventKind::BouldersChanged);
                }
                MineResult::Blocked | MineResult::Chipped | MineResult::Mined(_) => {}
            }
        }
        self.advance_canonical_day_if_due(next_tick, &mut output, &mut changed_players);
        self.advance_escape_sequence(next_tick, &mut output, &mut changed_players);
        self.advance_progression_depth(&mut output, &mut changed_players);
        let pressure_player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in pressure_player_ids {
            self.advance_world_event_hazards(player_id, next_tick, &mut output);
            self.advance_deep_run_pressure(player_id, next_tick, &mut output, &mut changed_players);
        }
        self.advance_falling_boulders(next_tick, &mut output, &mut changed_players);
        output.changed_players = changed_players.into_iter().collect();
        output.terrain_ops = self.terrain.finish_journal();
        self.set_simulation_tick(next_tick);
        output
    }

    pub(super) fn advance_escape_sequence(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
        changed_players: &mut BTreeSet<PlayerId>,
    ) {
        let Some(deadline) = self.progression.escape_deadline else {
            return;
        };
        if next_tick >= deadline {
            for (player_id, player) in &mut self.players {
                player.hull = 0.0;
                changed_players.insert(*player_id);
                output.push_event(GameplayEventKind::PlayerDamaged {
                    player_id: *player_id,
                });
            }
            self.progression.escape_deadline = None;
            return;
        }
        let rate = self.timing.rate;
        if rate.schedule_fires(next_tick, ESCAPE_CAVE_IN_PERIOD_MILLIS, 0)
            && let Some(player) = self.players.values().next()
        {
            let (x, y) = (player.x, player.y);
            self.spawn_cave_in(x, y);
            output.push_event(GameplayEventKind::BouldersChanged);
        }
        if rate.schedule_fires(
            next_tick,
            ESCAPE_COLLAPSE_PERIOD_MILLIS,
            ESCAPE_COLLAPSE_WARNING_PHASE_MILLIS,
        ) {
            self.collapse_warnings.clear();
            if let Some(player) = self.players.values().next() {
                let center = player.tile_position(TILE_SIZE);
                for dx in -2..=2 {
                    let position = TilePosition {
                        x: center.x + dx,
                        y: center.y + 5,
                    };
                    if position.y > 7
                        && !self.terrain.is_solid_at(position)
                        && !self.infrastructure.iter().any(|item| {
                            item.kind == InfrastructureKind::TunnelSupport
                                && (item.position.x - position.x).abs() <= 3
                                && (item.position.y - position.y).abs() <= 3
                        })
                    {
                        self.collapse_warnings.push(position);
                    }
                }
            }
        } else if rate.schedule_fires(next_tick, ESCAPE_COLLAPSE_PERIOD_MILLIS, 0) {
            for position in mem::take(&mut self.collapse_warnings) {
                if self.terrain.set_kind(position, TileKind::Stone) {
                    output.terrain_changed_tiles.push(position);
                    self.scan_markers
                        .retain(|marker| marker.position != position);
                }
            }
            output.push_event(GameplayEventKind::EscapeTunnelChanged);
        }
    }

    pub(super) fn grant_deep_reward(
        &mut self,
        player_id: PlayerId,
        depth: i32,
        output: &mut TickOutput,
        changed_players: &mut BTreeSet<PlayerId>,
    ) {
        use crate::LegendaryBlueprint;
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let position = TilePosition {
            x: player
                .tile_position(TILE_SIZE)
                .x
                .saturating_add(2)
                .clamp(1, self.terrain.width() - 2),
            y: depth.clamp(10, self.terrain.height() - 2),
        };
        let reward_tile = if depth >= 120 {
            let material = match depth {
                180.. => StrategicResourceKind::RadiantFossil,
                160..=179 => StrategicResourceKind::VoidGlass,
                140..=159 => StrategicResourceKind::MachineRelic,
                _ => StrategicResourceKind::PressurePearl,
            };
            player.add_material(material, 1);
            player.add_material(StrategicResourceKind::CoreShard, 1);
            let blueprint = if depth >= 160 {
                LegendaryBlueprint::RelicSorter
            } else if depth >= 140 {
                LegendaryBlueprint::VoidTank
            } else {
                LegendaryBlueprint::StarPlating
            };
            self.progression.legendary_blueprints.insert(blueprint);
            match blueprint {
                LegendaryBlueprint::StarPlating => {
                    self.progression
                        .rig_part_inventory
                        .insert(RigPartKind::PressureHull);
                    player.crafted_bulkheads = player.crafted_bulkheads.saturating_add(1);
                    player.hull = player.max_hull();
                }
                LegendaryBlueprint::VoidTank => {
                    self.progression
                        .rig_part_inventory
                        .insert(RigPartKind::HaulerEngine);
                    player.fuel_capacity += 20.0;
                    player.fuel = player.fuel_capacity;
                }
                LegendaryBlueprint::RelicSorter => {
                    self.progression
                        .rig_part_inventory
                        .insert(RigPartKind::RelicScanner);
                    self.progression
                        .rig_part_inventory
                        .insert(RigPartKind::CargoBalloon);
                    player.cargo_capacity = player.cargo_capacity.saturating_add(4);
                }
            }
            TileKind::Artifact(ArtifactKind::OldCircuit)
        } else {
            TileKind::Ore(MineralKind::Mythril)
        };
        if self.terrain.set_kind(position, reward_tile) {
            output.terrain_changed_tiles.push(position);
        }
        changed_players.insert(player_id);
        output.push_event(GameplayEventKind::DeepRewardGranted { player_id });
    }

    pub(super) fn advance_progression_depth(
        &mut self,
        output: &mut TickOutput,
        changed_players: &mut BTreeSet<PlayerId>,
    ) {
        let player_depths = self
            .players
            .iter()
            .map(|(player_id, player)| (*player_id, player.tile_position(TILE_SIZE).y))
            .collect::<Vec<_>>();
        for (player_id, depth) in player_depths {
            self.progression.deepest_tile_reached =
                self.progression.deepest_tile_reached.max(depth);
            let milestone = depth / 20;
            let previous = self
                .progression
                .deep_reward_milestones
                .get(&player_id)
                .copied()
                .unwrap_or(0);
            if depth >= 90 && milestone > previous {
                self.progression
                    .deep_reward_milestones
                    .insert(player_id, milestone);
                self.grant_deep_reward(player_id, depth, output, changed_players);
            }
        }
    }
}
