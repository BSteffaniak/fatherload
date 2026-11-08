//! World-event hazards, deep-run pressure, and falling boulders.

use std::mem;

use crate::{
    event::{GameplayEventKind, TickOutput},
    hazard::{FallingBoulder, HazardCloud},
    ids::{PlayerId, SimulationTick},
    progression::WorldEventKind,
    quantize::quantize_simulation_scalar,
    terrain::{MineralKind, TILE_SIZE, TilePosition},
};

use super::{
    WorldState,
    movement::{
        AUTHORITATIVE_CAVE_IN_LIFE_MILLIS, AUTHORITATIVE_CAVE_IN_WARNING_MILLIS,
        AUTHORITATIVE_CAVE_IN_WARNING_STEP_MILLIS, AUTHORITATIVE_GRAVITY,
        AUTHORITATIVE_PLAYER_RADIUS,
    },
};

/// Period of collapse-surge cave-ins near a deep rig.
const COLLAPSE_SURGE_PERIOD_MILLIS: u64 = 3_000;
/// Period of gas-bloom clouds near a deep rig.
const GAS_BLOOM_PERIOD_MILLIS: u64 = 4_000;
/// Period of gas clouds released by rare cargo on a deep run.
const RARE_CARGO_GAS_PERIOD_MILLIS: u64 = 4_000;
/// Scanner lockout refreshed every tick while below the pressure depth.
const DEEP_PRESSURE_SCANNER_LOCKOUT_MILLIS: u64 = 1_200;

impl WorldState {
    pub(super) fn advance_world_event_hazards(
        &mut self,
        player_id: PlayerId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        if player.y < TILE_SIZE * 8.0 {
            return;
        }
        let (player_x, player_y) = (player.x, player.y);
        if self
            .world_event_severity
            .contains_key(&WorldEventKind::CollapseSurge)
            && self
                .timing
                .rate
                .schedule_fires(next_tick, COLLAPSE_SURGE_PERIOD_MILLIS, 0)
        {
            self.spawn_cave_in(player_x, player_y);
            output.push_event(GameplayEventKind::BouldersChanged);
        }
        if self
            .world_event_severity
            .contains_key(&WorldEventKind::GasBloom)
            && self
                .timing
                .rate
                .schedule_fires(next_tick, GAS_BLOOM_PERIOD_MILLIS, 0)
        {
            self.hazards.push(HazardCloud {
                x: player_x,
                y: player_y + TILE_SIZE,
                life: 6.0,
                radius: 12.0,
            });
            output.push_event(GameplayEventKind::HazardChanged);
        }
    }

    pub(super) fn advance_deep_run_pressure(
        &mut self,
        player_id: PlayerId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let depth = player.tile_position(TILE_SIZE).y;
        let rare_cargo = player
            .cargo
            .iter()
            .filter(|(mineral, _)| {
                matches!(
                    mineral,
                    MineralKind::Diamond
                        | MineralKind::Platinum
                        | MineralKind::Uranium
                        | MineralKind::Mythril
                )
            })
            .map(|(_, count)| *count)
            .sum::<u32>()
            .saturating_add(player.artifacts.values().copied().sum());
        let timing = self.timing;
        let delta_seconds = timing.delta_seconds;
        let state = self.deep_run_states.entry(player_id).or_default();
        if depth < 80 {
            state.trip_ticks = 0;
            state.instability = 2.0f32.mul_add(-delta_seconds, state.instability).max(0.0);
            return;
        }
        state.trip_ticks = state.trip_ticks.saturating_add(1);
        let trip_seconds = timing.rate.seconds_for_ticks(state.trip_ticks);
        #[allow(
            clippy::cast_precision_loss,
            reason = "bounded depth and cargo counts intentionally feed gameplay pressure"
        )]
        let pressure = ((depth - 70) as f32).mul_add(
            0.006,
            trip_seconds.mul_add(0.0006, rare_cargo as f32 * 0.02),
        );
        state.instability = delta_seconds.mul_add(pressure, state.instability);
        if depth >= 105 {
            if let Some(player) = self.players.get_mut(&player_id) {
                player.fuel =
                    quantize_simulation_scalar(delta_seconds.mul_add(-0.25, player.fuel).max(0.0));
            }
            let deadline = SimulationTick::new(
                next_tick
                    .get()
                    .saturating_add(timing.ticks_for_millis(DEEP_PRESSURE_SCANNER_LOCKOUT_MILLIS)),
            );
            self.scanner_cooldown_deadlines
                .entry(player_id)
                .and_modify(|current| *current = (*current).max(deadline))
                .or_insert(deadline);
        }
        if rare_cargo > 0
            && timing
                .rate
                .schedule_fires(next_tick, RARE_CARGO_GAS_PERIOD_MILLIS, 0)
        {
            let player = self.players.get(&player_id).expect("player remains valid");
            self.hazards.push(HazardCloud {
                x: player.x,
                y: player.y + TILE_SIZE,
                life: 5.0,
                radius: 10.0 + f32::from(u16::try_from(rare_cargo).unwrap_or(u16::MAX)).min(4.0),
            });
            output.push_event(GameplayEventKind::HazardChanged);
        }
        if state.instability >= 100.0 {
            state.instability = 45.0;
            let player = self.players.get(&player_id).expect("player remains valid");
            let (player_x, player_y) = (player.x, player.y);
            self.spawn_cave_in(player_x, player_y);
            let _snapshot = self.apply_hazard_damage(player_id, 4.0);
            output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            output.push_event(GameplayEventKind::BouldersChanged);
        }
    }

    /// Spawns three boulders above `(x, y)`: the centre one first, the outer ones a little later.
    pub(super) fn spawn_cave_in(&mut self, x: f32, y: f32) {
        for offset in -1_i16..=1 {
            self.spawn_falling_boulder(
                f32::from(offset).mul_add(TILE_SIZE, x),
                TILE_SIZE.mul_add(-2.0, y),
                AUTHORITATIVE_CAVE_IN_WARNING_MILLIS
                    + u64::from(offset.unsigned_abs()) * AUTHORITATIVE_CAVE_IN_WARNING_STEP_MILLIS,
                AUTHORITATIVE_CAVE_IN_LIFE_MILLIS,
            );
        }
    }

    pub(super) fn spawn_falling_boulder(
        &mut self,
        x: f32,
        y: f32,
        warning_millis: u64,
        life_millis: u64,
    ) {
        let rate = self.timing.rate;
        let warning_ticks = rate.ticks_for_millis(warning_millis);
        let life_ticks = rate.ticks_for_millis(life_millis);
        self.falling_boulders.push(FallingBoulder {
            x,
            y,
            velocity_y: 0.0,
            warning_seconds: rate.seconds_for_ticks(warning_ticks),
            life: rate.seconds_for_ticks(life_ticks),
        });
        self.boulder_warning_deadlines.push(SimulationTick::new(
            self.simulation_tick.get().saturating_add(warning_ticks),
        ));
        self.boulder_expiry_ticks.push(SimulationTick::new(
            self.simulation_tick.get().saturating_add(life_ticks),
        ));
    }

    pub(super) fn advance_falling_boulders(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let timing = self.timing;
        let boulders = mem::take(&mut self.falling_boulders);
        let warning_deadlines = mem::take(&mut self.boulder_warning_deadlines);
        let expiry_ticks = mem::take(&mut self.boulder_expiry_ticks);
        for (index, mut boulder) in boulders.into_iter().enumerate() {
            let warning_deadline = warning_deadlines.get(index).copied().unwrap_or(next_tick);
            let expiry_tick = expiry_ticks.get(index).copied().unwrap_or(next_tick);
            if next_tick >= expiry_tick {
                continue;
            }
            if next_tick >= warning_deadline {
                boulder.velocity_y = quantize_simulation_scalar(
                    (AUTHORITATIVE_GRAVITY * 0.8)
                        .mul_add(timing.delta_seconds, boulder.velocity_y)
                        .min(520.0),
                );
                boulder.y = quantize_simulation_scalar(
                    boulder.velocity_y.mul_add(timing.delta_seconds, boulder.y),
                );
                let hit_player = self.players.iter().find_map(|(player_id, player)| {
                    let dx = player.x - boulder.x;
                    let dy = player.y - boulder.y;
                    (dx.mul_add(dx, dy * dy) <= (AUTHORITATIVE_PLAYER_RADIUS + 8.0).powi(2))
                        .then_some(*player_id)
                });
                if let Some(player_id) = hit_player {
                    let _snapshot = self.apply_hazard_damage(player_id, 8.0);
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                    continue;
                }
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "bounded boulder coordinates intentionally map to terrain tiles"
                )]
                let tile = TilePosition {
                    x: (boulder.x / TILE_SIZE).floor() as i32,
                    y: (boulder.y / TILE_SIZE).floor() as i32,
                };
                if self.terrain.is_solid_at(tile) {
                    continue;
                }
            }
            boulder.warning_seconds = timing.rate.seconds_until(next_tick, warning_deadline);
            boulder.life = timing.rate.seconds_until(next_tick, expiry_tick);
            self.falling_boulders.push(boulder);
            self.boulder_warning_deadlines.push(warning_deadline);
            self.boulder_expiry_ticks.push(expiry_tick);
        }
    }
}
