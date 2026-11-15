//! World-event hazards, deep-run pressure, and falling boulders.

use crate::{
    event::{GameplayEventKind, TickOutput},
    hazard::{FallingBoulder, HazardCloud},
    ids::{PlayerId, SimulationTick},
    progression::WorldEventKind,
    quantize::quantize_simulation_scalar,
    terrain::{DeepStratum, MineralKind, TILE_SIZE},
    worldgen::Band,
};

use super::{
    WorldState,
    movement::{
        AUTHORITATIVE_CAVE_IN_LIFE_MILLIS, AUTHORITATIVE_CAVE_IN_WARNING_MILLIS,
        AUTHORITATIVE_CAVE_IN_WARNING_STEP_MILLIS,
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
/// Instability per second added for each band below the slate shelves (a full meter
/// triggers a cave-in).
const DEEP_PRESSURE_PER_BAND: f32 = 0.05;
/// Hull damage of a falling boulder hitting a rig (before the boulder deflector).
pub const BOULDER_HIT_DAMAGE: f32 = 8.0;

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
        // World-event hazards strike underground only: within three rows of the local surface
        // (a mountain slope, a lake bed) a rig is still out in the open.
        let tile = player.tile_position(TILE_SIZE);
        if tile.y < self.terrain.surface_row(tile.x) + 3 {
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
            self.spawn_gas_cloud(HazardCloud {
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
        let band = self.terrain.band_at(player.tile_position(TILE_SIZE));
        let rare_cargo = player
            .cargo_bay
            .minerals()
            .filter(|(mineral, _)| {
                matches!(
                    mineral,
                    MineralKind::Diamond
                        | MineralKind::Platinum
                        | MineralKind::Uranium
                        | MineralKind::Mythril
                )
            })
            .map(|(_, count)| count)
            .sum::<u32>()
            .saturating_add(player.cargo_bay.artifact_total());
        let timing = self.timing;
        let delta_seconds = timing.delta_seconds;
        let state = self.deep_run_states.entry(player_id).or_default();
        // Instability builds from the basalt deeps down, wherever this world's strata put them.
        let bands_deep = band.index().saturating_sub(Band::Slate.index());
        if bands_deep == 0 {
            state.trip_ticks = 0;
            state.instability = 2.0f32.mul_add(-delta_seconds, state.instability).max(0.0);
            return;
        }
        state.trip_ticks = state.trip_ticks.saturating_add(1);
        let trip_seconds = timing.rate.seconds_for_ticks(state.trip_ticks);
        #[allow(
            clippy::cast_precision_loss,
            reason = "bounded band and cargo counts intentionally feed gameplay pressure"
        )]
        let pressure = (bands_deep as f32).mul_add(
            DEEP_PRESSURE_PER_BAND,
            trip_seconds.mul_add(0.0003, rare_cargo as f32 * 0.01),
        );
        state.instability = delta_seconds.mul_add(pressure, state.instability);
        if band.index() >= Band::Deep(DeepStratum::FossilOceans).index() {
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
            let cloud = HazardCloud {
                x: player.x,
                y: player.y + TILE_SIZE,
                life: 5.0,
                radius: 10.0 + f32::from(u16::try_from(rare_cargo).unwrap_or(u16::MAX)).min(4.0),
            };
            super::entities::spawn_gas_cloud_into(&mut self.entities, self.simulation_tick, cloud);
            output.push_event(GameplayEventKind::HazardChanged);
        }
        if state.instability >= 100.0 {
            state.instability = 45.0;
            let player = self.players.get(&player_id).expect("player remains valid");
            let (player_x, player_y) = (player.x, player.y);
            let damage = player.hazard_gear.boulder_damage(4.0);
            self.spawn_cave_in(player_x, player_y);
            if self.damage_player(player_id, damage) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
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
        self.spawn_boulder(
            FallingBoulder {
                x,
                y,
                velocity_y: 0.0,
                warning_seconds: rate.seconds_for_ticks(warning_ticks),
                life: rate.seconds_for_ticks(life_ticks),
            },
            SimulationTick::new(self.simulation_tick.get().saturating_add(warning_ticks)),
            SimulationTick::new(self.simulation_tick.get().saturating_add(life_ticks)),
        );
    }
}
