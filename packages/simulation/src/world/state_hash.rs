//! Canonical simulation state hashing.

use crate::hash::{SimulationStateHash, StateHasher};

use super::WorldState;

impl WorldState {
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "stable hash intentionally enumerates all canonical gameplay state"
    )]
    pub fn simulation_state_hash(&self) -> SimulationStateHash {
        let mut hash = StateHasher::new();
        hash.write_u64(self.simulation_tick.get());
        hash.write_u64(u64::try_from(self.players.len()).unwrap_or(u64::MAX));
        for (player_id, player) in &self.players {
            hash.write_u64(player_id.get());
            hash.write_serialized(player);
        }
        for (player_id, parts) in &self.equipped_rig_parts {
            hash.write_u64(player_id.get());
            hash.write_serialized(parts);
        }
        for (player_id, mechanic_level) in &self.mechanic_levels {
            hash.write_u64(player_id.get());
            hash.write_u64(u64::from(*mechanic_level));
        }
        for (kind, severity) in &self.world_event_severity {
            hash.write_serialized(kind);
            hash.write_u64(u64::from(*severity));
        }
        for (kind, days) in &self.world_event_days_remaining {
            hash.write_serialized(kind);
            hash.write_u64(u64::from(*days));
        }
        for (player_id, state) in &self.deep_run_states {
            hash.write_u64(player_id.get());
            hash.write_u64(state.trip_ticks);
            hash.write_f32(state.instability);
        }
        for (player_id, site) in &self.lost_cargo_sites {
            hash.write_u64(player_id.get());
            hash.write_f32(site.x);
            hash.write_f32(site.y);
            hash.write_serialized(&site.minerals);
            hash.write_serialized(&site.artifacts);
        }
        for (player_id, count) in &self.rescue_counts {
            hash.write_u64(player_id.get());
            hash.write_u64(u64::from(*count));
        }
        hash.write_serialized(&self.progression.contracts);
        hash.write_serialized(&self.progression.deep_claim_status);
        if let Some(deadline) = self.progression.escape_deadline {
            hash.write_u64(deadline.get());
        }
        hash.write_serialized(&self.progression.deep_reward_milestones);
        hash.write_serialized(&self.progression.town_development);
        hash.write_serialized(&self.progression.active_side_contracts);
        hash.write_serialized(&self.progression.expedition_offers);
        hash.write_serialized(&self.progression.active_expeditions);
        hash.write_serialized(&self.progression.legendary_blueprints);
        hash.write_serialized(&self.progression.rig_part_inventory);
        hash.write_u64(u64::from(self.progression.town_event_day));
        hash.write_u64(u64::from(self.progression.market_salt));
        hash.write_serialized(&self.progression.market_history);
        hash.write_serialized(&self.progression.mineral_market_history);
        hash.write_i32(self.progression.deepest_tile_reached);
        hash.write_u64(u64::from(self.progression.total_earnings));
        hash.write_u64(u64::from(self.progression.expeditions_completed));
        hash.write_u64(self.progression.next_day_tick.get());
        hash.write_serialized(&self.scan_markers);
        hash.write_serialized(&self.collapse_warnings);
        hash.write_i32(self.terrain.width());
        hash.write_i32(self.terrain.height());
        hash.write_u64(self.terrain.seed());
        hash.write_u64(u64::try_from(self.terrain.tiles().len()).unwrap_or(u64::MAX));
        for tile in self.terrain.tiles() {
            hash.write_serialized(tile);
        }
        hash.write_u64(u64::try_from(self.hazards.len()).unwrap_or(u64::MAX));
        for hazard in &self.hazards {
            hash.write_f32(hazard.x);
            hash.write_f32(hazard.y);
            hash.write_f32(hazard.life);
            hash.write_f32(hazard.radius);
        }
        hash.write_u64(u64::try_from(self.falling_boulders.len()).unwrap_or(u64::MAX));
        for boulder in &self.falling_boulders {
            hash.write_f32(boulder.x);
            hash.write_f32(boulder.y);
            hash.write_f32(boulder.velocity_y);
        }
        for deadline in &self.boulder_warning_deadlines {
            hash.write_u64(deadline.get());
        }
        for deadline in &self.boulder_expiry_ticks {
            hash.write_u64(deadline.get());
        }
        hash.write_u64(u64::try_from(self.bombs.len()).unwrap_or(u64::MAX));
        for bomb in &self.bombs {
            hash.write_f32(bomb.x);
            hash.write_f32(bomb.y);
            hash.write_f32(bomb.timer_seconds);
        }
        hash.write_u64(u64::try_from(self.infrastructure.len()).unwrap_or(u64::MAX));
        for infrastructure in &self.infrastructure {
            hash.write_serialized(&infrastructure.kind);
            hash.write_serialized(&infrastructure.position);
            hash.write_u64(u64::from(infrastructure.durability));
        }
        hash.write_u64(u64::try_from(self.active_drills.len()).unwrap_or(u64::MAX));
        for (player_id, drill) in &self.active_drills {
            hash.write_u64(player_id.get());
            hash.write_serialized(drill);
        }
        for (player_id, timer) in &self.active_drill_timers {
            hash.write_u64(player_id.get());
            hash.write_u64(timer.next_chip_tick.get());
            hash.write_u64(timer.period_ticks);
        }
        for (player_id, cooldown) in &self.scanner_cooldowns {
            hash.write_u64(player_id.get());
            hash.write_f32(*cooldown);
        }
        for (player_id, deadline) in &self.scanner_cooldown_deadlines {
            hash.write_u64(player_id.get());
            hash.write_u64(deadline.get());
        }
        for deadline in &self.bomb_detonation_ticks {
            hash.write_u64(deadline.get());
        }
        hash.write_u64(self.rng.state());
        for (player_id, discovered) in &self.discovered_tiles {
            hash.write_u64(player_id.get());
            hash.write_u64(u64::try_from(discovered.len()).unwrap_or(u64::MAX));
            for position in discovered {
                hash.write_i32(position.x);
                hash.write_i32(position.y);
            }
        }
        for transaction in &self.service_transactions {
            hash.write_u64(transaction.player_id.get());
            hash.write_serialized(&transaction.kind);
            hash.write_u64(u64::from(transaction.credits_before));
            hash.write_u64(u64::from(transaction.credits_after));
            hash.write_u64(u64::from(transaction.cargo_before));
            hash.write_u64(u64::from(transaction.cargo_after));
        }
        hash.finish()
    }
}
