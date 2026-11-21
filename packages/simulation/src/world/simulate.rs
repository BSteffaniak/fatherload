//! The fixed-tick [`WorldState::simulate_tick`] transition.

use std::mem;

use crate::{
    entity::{EntityKind, EntityPhase, EntityState},
    event::{ActionOutcome, GameplayEventKind, TickOutput, gameplay_event_for_applied_command},
    ids::{PlayerId, SimulationTick},
    infrastructure::InfrastructureKind,
    input::{ControlState, TickInputs},
    progression::WorldEventKind,
    protection::{HEAT_DAMAGE_PER_EXCESS_UNIT, heat_excess},
    quantize::quantize_simulation_scalar,
    rig::RigPartKind,
    summary::PlayerScopedCommandOutcome,
    terrain::{
        ArtifactKind, MineResult, MineralKind, StrategicResourceKind, TILE_SIZE, TileKind,
        TilePosition,
    },
};

use super::{
    ActiveDrillTimer, WorldState,
    movement::{AUTHORITATIVE_BOULDER_SPAWN_CHANCE, boulder_spawn_chance},
    quests::QuestNotes,
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
    pub fn simulate_tick(&mut self, tick: SimulationTick, inputs: &TickInputs) -> TickOutput {
        let mut output = TickOutput {
            tick,
            ..TickOutput::default()
        };
        let timing = self.timing;
        let delta_seconds = timing.delta_seconds;
        self.terrain.start_journal();
        // Collapsed ground whose hold has run out may move again.
        self.progression.settled_ground.prune(self.simulation_tick);
        let mut quest_notes = QuestNotes::default();
        for (&player_id, input) in &inputs.players {
            // On foot the held controls drive the pilot (`advance_pilots`); the parked rig
            // gets neutral ones.
            let on_foot = self.is_on_foot(player_id);
            // WP-PILOT: a rig-linked rig's gun covers its pilot (the sentry).
            let sentry = if on_foot {
                self.rig_sentry_control(player_id)
            } else {
                None
            };
            if let Some(controls) = input.controls.map(|controls| {
                if on_foot {
                    ControlState::default().with_weapon(sentry.unwrap_or_default())
                } else {
                    controls
                }
            }) {
                let previous_target = self.active_drills.get(&player_id).map(|drill| drill.target);
                let _drill = self.apply_drill_intent(
                    player_id,
                    controls.horizontal(),
                    controls.drill_down,
                    controls.thrust,
                );
                let target = self.active_drills.get(&player_id).map(|drill| drill.target);
                if target.is_some() && target != previous_target {
                    output.push_event(GameplayEventKind::DrillStarted { player_id });
                }
                let fuel_before = self.players.get(&player_id).map(|player| player.fuel);
                let position_before = self
                    .players
                    .get(&player_id)
                    .map(|player| (player.x, player.y));
                let no_clip = self.no_clip_active();
                // WP-PILOT: a recalled rig flies itself to its pilot instead.
                let recalled = on_foot && self.step_rig_recall(player_id, &mut output);
                let _step = if recalled {
                    None
                } else if no_clip {
                    // Test-mode no-clip: thrust flies up, drill flies down.
                    let vertical =
                        f32::from(i8::from(controls.drill_down) - i8::from(controls.thrust));
                    self.step_no_clip_flight(player_id, controls.horizontal(), vertical)
                } else {
                    self.step_authoritative_movement(
                        player_id,
                        controls.horizontal(),
                        controls.thrust,
                    )
                };
                if !no_clip {
                    // A tether harpoon's line holds the rig (T21).
                    self.apply_tether(player_id, controls.thrust);
                }
                if let Some(before) = position_before {
                    // Sky bands, updrafts and icing (F6, V8 skies).
                    self.push_altitude_events(player_id, before, &mut output);
                    // Vacuum warnings and the first Moon landing (WP-MOON).
                    self.push_moon_events(player_id, before, &mut output);
                }
                self.update_weapon(player_id, controls.weapon, tick.next(), &mut output);
                if let Some(fuel_before) = fuel_before
                    && !no_clip
                {
                    // Lift ladders in bases (WP-BASE).
                    self.apply_base_lift(player_id, fuel_before, controls.thrust);
                }
            }
            for action in &input.actions {
                let outcome = if !self.players.contains_key(&player_id) {
                    PlayerScopedCommandOutcome::UnknownPlayer
                } else if !self.command_allowed_here(player_id, &action.command) {
                    // Building services only work at the building (validated here, inside the
                    // authoritative tick, so it holds for remote clients too).
                    PlayerScopedCommandOutcome::IgnoredUnavailable
                } else if let Some(outcome) =
                    self.pilot_command(player_id, &action.command, &mut output)
                {
                    outcome
                } else if let Some(outcome) =
                    self.tomb_command(player_id, &action.command, &mut output)
                {
                    // Tomb runs (WP-TOMBS, `world/tombs.rs`): relic picks.
                    outcome
                } else if let crate::PlayerCommand::Moon(command) = action.command {
                    // The Moon (WP-MOON, `world/moon.rs`): it reports its own events.
                    self.apply_moon_command(player_id, command, &mut output)
                } else if let Some((outcome, events)) =
                    self.quest_command(player_id, &action.command)
                {
                    for event in events {
                        output.push_event(event);
                    }
                    outcome
                } else if let Some((outcome, events)) =
                    self.people_command(player_id, &action.command)
                {
                    // WP-PEOPLE: talk, trade and gifts (`world/people.rs`).
                    for event in events {
                        output.push_event(event);
                    }
                    outcome
                } else if let Some((outcome, events)) =
                    self.home_command(player_id, &action.command)
                {
                    // WP-HOME: plots, contracts, decorating (`world/homes.rs`).
                    for event in events {
                        output.push_event(event);
                    }
                    outcome
                } else {
                    self.apply_player_command(player_id, &action.command)
                };
                output.action_outcomes.push(ActionOutcome {
                    player_id,
                    sequence: action.sequence,
                    outcome,
                });
                if outcome == PlayerScopedCommandOutcome::Applied {
                    if let Some(event) =
                        gameplay_event_for_applied_command(player_id, &action.command)
                    {
                        output.push_event(event);
                    }
                    // The shop's purchase moment (WP-FEEL part 2).
                    if let Some(event) = self.upgrade_installed_event(player_id, &action.command) {
                        output.push_event(event);
                    }
                    // A lunar uplink's beam (WP-MOON).
                    if let Some(event) = self.uplink_event(player_id, &action.command) {
                        output.push_event(event);
                    }
                }
            }
        }
        let next_tick = tick.next();
        // Pilots on foot (Part II F5) move with their owners' held controls.
        self.advance_pilots(inputs, next_tick, &mut output);
        // Ruin pressure plates under their feet (WP-PILOT).
        self.advance_plates(next_tick, &mut output);
        // Tomb runs (WP-TOMBS): ambushes, the guardian, traps, revives, picks, wipes.
        self.advance_tombs(next_tick, &mut output);
        let recovery_player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in recovery_player_ids {
            if self.recover_lost_cargo_for_player(player_id) {
                output.push_event(GameplayEventKind::CargoChanged { player_id });
            }
        }
        for (player_id, deadline) in &self.scanner_cooldown_deadlines {
            self.scanner_cooldowns
                .insert(*player_id, timing.rate.seconds_until(next_tick, *deadline));
        }

        self.advance_entities(EntityPhase::Early, next_tick, &mut output);
        let player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in player_ids {
            let in_cloud = self.players.get(&player_id).is_some_and(|player| {
                self.entities
                    .of_kind(EntityKind::GasCloud)
                    .any(|entity| match entity.state {
                        EntityState::GasCloud(cloud) => {
                            if cloud.life >= 6.0 {
                                return false;
                            }
                            let dx = player.x - entity.body.x;
                            let dy = player.y - entity.body.y;
                            dx.mul_add(dx, dy * dy) <= cloud.radius * cloud.radius
                        }
                        _ => false,
                    })
            });
            let damage = self.players.get(&player_id).map_or(0.0, |player| {
                player.hazard_gear.gas_damage(4.0 * delta_seconds)
            });
            if in_cloud && self.damage_player(player_id, damage) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
        }

        let player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in player_ids {
            let (player_x, player_y, radiator_level, gear) = self
                .players
                .get(&player_id)
                .map(|player| {
                    (
                        player.x,
                        player.y,
                        player.radiator_level,
                        player.hazard_gear,
                    )
                })
                .expect("collected player id remains valid");
            let parts = self
                .equipped_rig_parts
                .get(&player_id)
                .cloned()
                .unwrap_or_default();
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
            #[allow(
                clippy::cast_possible_truncation,
                reason = "bounded player coordinates intentionally map to terrain tiles"
            )]
            let center = TilePosition {
                x: (player_x / TILE_SIZE).floor() as i32,
                y: (player_y / TILE_SIZE).floor() as i32,
            };
            // The pressure hull pays off in the deep strata, wherever this world's wavy
            // boundary puts them.
            if parts.contains(&RigPartKind::PressureHull)
                && self.terrain.deep_stratum_at(center).is_some()
            {
                heat_multiplier *= 0.72;
            }
            // Band heat: each band runs a heat level hotter than the one above it, and the
            // radiator plus heat shielding cover a number of heat levels (see
            // `crate::protection`), so heat follows the strata wherever they lie.
            let excess = heat_excess(
                self.terrain.band_at(center),
                radiator_level,
                gear.heat_shield,
            );
            let depth_damage = if excess > 0 {
                HEAT_DAMAGE_PER_EXCESS_UNIT * f32::from(excess) * heat_multiplier * delta_seconds
            } else {
                0.0
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
                gear.lava_damage(
                    (if pump_protected {
                        2.5
                    } else {
                        LAVA_PROXIMITY_DAMAGE_PER_SECOND
                    }) * heat_multiplier
                        * delta_seconds,
                )
            } else {
                0.0
            };
            // Altitude cold (F6, V8 skies): the reverse of band heat, covered by the de-icer.
            let cold =
                crate::environment::environment_at(self.terrain.settings(), player_x, player_y)
                    .cold;
            let cold_excess = gear.cold_excess(cold);
            let cold_damage = if cold_excess > 0 {
                crate::environment::COLD_DAMAGE_PER_EXCESS_LEVEL
                    * f32::from(cold_excess)
                    * delta_seconds
            } else {
                0.0
            };
            // Vacuum (WP-MOON): no pressurized hull or no life support in orbit and on the Moon.
            let vacuum_damage = self.vacuum_damage(player_id, delta_seconds);
            let total_damage = depth_damage + lava_damage + cold_damage + vacuum_damage;
            if self.damage_player(player_id, total_damage) {
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.hull = quantize_simulation_scalar(player.hull);
                }
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
        }

        // Blasts push rigs and entities inside the bomb pass (`push_from_blasts`).
        self.advance_bombs(next_tick, &mut quest_notes, &mut output);

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
            let Some((result, damaged)) = self.chip_active_drill_target(player_id) else {
                continue;
            };
            if damaged {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
            if matches!(
                result,
                MineResult::Blocked
                    | MineResult::TooDangerous
                    | MineResult::Exploded
                    | MineResult::Blast
                    | MineResult::Mined(_)
                    | MineResult::Activated(_)
            ) {
                self.active_drill_timers.remove(&player_id);
            }
            if let MineResult::Mined(kind) = result {
                quest_notes.mined.push((player_id, target, kind));
            }
            match result {
                // Ore and artifacts now drop: the pickup pass reports them (`ItemsCollected`).
                MineResult::Mined(TileKind::Artifact(ArtifactKind::StarCore)) => {
                    self.start_escape(next_tick, player_id, &mut output);
                }
                MineResult::Mined(TileKind::Stone | TileKind::HardRock) => {
                    let chance = self
                        .players
                        .get(&player_id)
                        .map_or(AUTHORITATIVE_BOULDER_SPAWN_CHANCE, |player| {
                            boulder_spawn_chance(player.drill_strength)
                        });
                    if self.loosen_mining_boulder(target, chance) {
                        output.push_event(GameplayEventKind::BouldersChanged);
                    }
                }
                MineResult::TooDangerous => {
                    if self.damage_player(player_id, 8.0) {
                        output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                    }
                }
                MineResult::Exploded => {
                    output.push_event(GameplayEventKind::HazardChanged);
                }
                MineResult::Blast => {
                    if let Some(player) = self.players.get(&player_id) {
                        let (player_x, player_y) = (player.x, player.y);
                        let _loose = self.spawn_cave_in(player_x, player_y);
                    }
                    output.push_event(GameplayEventKind::HazardChanged);
                    output.push_event(GameplayEventKind::BouldersChanged);
                }
                MineResult::Mined(TileKind::Poi(tile)) => {
                    self.open_poi_tile(player_id, target, tile, &mut output);
                }
                MineResult::Mined(TileKind::Cave(tile)) if tile.is_cache() => {
                    self.open_cave_cache(player_id, target, tile, &mut output);
                }
                MineResult::Mined(TileKind::Sky(tile)) if tile.is_cache() => {
                    self.open_sky_cache(player_id, target, tile, &mut output);
                }
                MineResult::Activated(lit) => {
                    self.activate_vault_lock(target, lit, next_tick, &mut output);
                }
                MineResult::Blocked | MineResult::Chipped | MineResult::Mined(_) => {}
            }
        }
        self.advance_canonical_day_if_due(next_tick, &mut output);
        let clock_before = self.clock();
        self.advance_clock(clock_before, next_tick, &mut output);
        self.advance_escape_sequence(next_tick, &mut output);
        self.advance_progression_depth(&mut output);
        let pressure_player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in pressure_player_ids {
            self.advance_world_event_hazards(player_id, next_tick, &mut output);
            self.advance_deep_run_pressure(player_id, next_tick, &mut output);
        }
        self.trigger_stalactites(&mut output);
        self.advance_enemy_director(next_tick);
        // The Warden's arena and the core (WP-MOON).
        self.advance_moon(&mut output);
        self.advance_night(next_tick, &mut output);
        self.advance_entities(EntityPhase::Late, next_tick, &mut output);
        self.advance_pilot_pickups(&mut output);
        self.advance_critters(next_tick);
        self.advance_people(next_tick);
        // WP-HOME: construction in world time, site crews, garage bays.
        self.advance_homes(next_tick, &mut output);
        self.advance_bases(next_tick);
        self.advance_vault_relays(next_tick, &mut output);
        self.track_poi_visits(&mut output);
        self.track_cave_discoveries(&mut output);
        self.advance_quests(next_tick, &quest_notes, &mut output);
        self.reveal_tiles_near_players();
        self.discover_villages(&mut output);
        self.advance_milestones(&mut output);
        self.apply_test_mode_refills();
        self.advance_sky_isles(&mut output);
        output.terrain_ops = self.terrain.finish_journal();
        self.wake_entities_near(&output.terrain_ops);
        output.discoveries = self.take_discoveries();
        self.stream_terrain_near_players();
        self.set_simulation_tick(next_tick);
        output
    }

    /// Keeps a streamed world's chunks loaded around every rig and unloads the rest (edits are
    /// kept). Loading never changes a tile, so this has no effect on the simulation's results.
    pub fn stream_terrain_near_players(&mut self) {
        if !self.terrain.is_streamed() {
            return;
        }
        let mut centers: Vec<TilePosition> = self
            .players
            .values()
            .map(|player| player.tile_position(TILE_SIZE))
            .collect();
        // A pilot walking away from its rig keeps its own surroundings loaded.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "bounded world pixels address terrain tiles"
        )]
        centers.extend(self.pilots().map(|(_, body, _)| TilePosition {
            x: (body.x / TILE_SIZE).floor() as i32,
            y: (body.y / TILE_SIZE).floor() as i32,
        }));
        self.terrain.stream_around(&centers);
    }

    /// Breaking the Star Core out of the rock (drilled, or freed by a blast) starts the escape
    /// countdown.
    pub(super) fn start_escape(
        &mut self,
        next_tick: SimulationTick,
        player_id: PlayerId,
        output: &mut TickOutput,
    ) {
        self.progression.escape_deadline =
            Some(SimulationTick::new(next_tick.get().saturating_add(
                self.timing.ticks_for_millis(ESCAPE_DURATION_MILLIS),
            )));
        output.push_event(GameplayEventKind::EscapeStarted { player_id });
    }

    pub(super) fn advance_escape_sequence(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(deadline) = self.progression.escape_deadline else {
            return;
        };
        if next_tick >= deadline {
            let immune = self.damage_immune();
            for (player_id, player) in &mut self.players {
                if immune {
                    continue;
                }
                player.hull = 0.0;
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
            if self.spawn_cave_in(x, y) {
                output.push_event(GameplayEventKind::BouldersChanged);
            }
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
                .clamp(self.terrain.min_x() + 1, self.terrain.max_x() - 1),
            y: depth.clamp(10, self.terrain.height() - 2),
        };
        // Reward tiers read classic nominal depths.
        let tier_depth = self.terrain.nominal_row(depth);
        let reward_tile = if tier_depth >= 120 {
            let material = match tier_depth {
                180.. => StrategicResourceKind::RadiantFossil,
                160..=179 => StrategicResourceKind::VoidGlass,
                140..=159 => StrategicResourceKind::MachineRelic,
                _ => StrategicResourceKind::PressurePearl,
            };
            player.add_material(material, 1);
            player.add_material(StrategicResourceKind::CoreShard, 1);
            let blueprint = if tier_depth >= 160 {
                LegendaryBlueprint::RelicSorter
            } else if tier_depth >= 140 {
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
        let _placed = self.terrain.set_kind(position, reward_tile);
        output.push_event(GameplayEventKind::DeepRewardGranted { player_id });
    }

    pub(super) fn advance_progression_depth(&mut self, output: &mut TickOutput) {
        if self.is_test_mode() {
            // Depth records and deep-reward milestones do not count in test mode.
            return;
        }
        let player_depths = self
            .players
            .iter()
            .map(|(player_id, player)| (*player_id, player.tile_position(TILE_SIZE).y))
            .collect::<Vec<_>>();
        for (player_id, depth) in player_depths {
            self.progression.deepest_tile_reached =
                self.progression.deepest_tile_reached.max(depth);
            // Milestones every 20 classic nominal rows from the first deep stratum.
            let nominal = self.terrain.nominal_row(depth);
            let milestone = nominal / 20;
            let previous = self
                .progression
                .deep_reward_milestones
                .get(&player_id)
                .copied()
                .unwrap_or(0);
            if nominal >= 90 && milestone > previous {
                self.progression
                    .deep_reward_milestones
                    .insert(player_id, milestone);
                self.grant_deep_reward(player_id, depth, output);
            }
        }
    }
}

/// Hull damage per second from lava within two tiles of the rig (before lava plating).
pub const LAVA_PROXIMITY_DAMAGE_PER_SECOND: f32 = 9.0;
