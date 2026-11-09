//! Canonical simulation state hashing.
//!
//! [`WorldState::simulation_state_hash`] covers the whole world.
//! [`WorldState::hash_visible`] covers what one client can see (see [`super::visibility`]):
//! every player's per-tick state and [`super::RigBuild`] are always included, while a player's
//! private state (inventory, credits, kits, rig parts, mechanic level, salvage, rescue count,
//! deep reward milestone, discovery and service transactions) is included only for players the
//! predicate accepts. Per-player data is hashed player by player, so the result does not depend
//! on how entries of different players interleave.

use std::collections::BTreeSet;

use crate::{
    hash::{SimulationStateHash, StateHasher},
    ids::PlayerId,
};

use super::{WorldState, player_sections::split_player};

impl WorldState {
    /// Hash of the whole world (determinism checks, local replays).
    #[must_use]
    pub fn simulation_state_hash(&self) -> SimulationStateHash {
        self.hash_visible(|_| true)
    }

    /// Hash of the world with private per-player state included only where `sees_private`
    /// accepts the player.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "stable hash intentionally enumerates all canonical gameplay state"
    )]
    pub(super) fn hash_visible(
        &self,
        sees_private: impl Fn(PlayerId) -> bool,
    ) -> SimulationStateHash {
        let mut hash = StateHasher::new();
        hash.write_serialized(&self.timing);
        hash.write_serialized(&self.player_template);
        hash.write_u64(self.simulation_tick.get());
        hash.write_u64(u64::from(self.won_game));
        hash.write_u64(u64::try_from(self.players.len()).unwrap_or(u64::MAX));
        for (player_id, player) in &self.players {
            hash.write_u64(player_id.get());
            let (rig, build, inventory) = split_player(player);
            hash.write_serialized(&rig);
            hash.write_serialized(&build);
            if sees_private(*player_id) {
                hash.write_serialized(&inventory);
            }
        }
        for (player_id, parts) in &self.equipped_rig_parts {
            if sees_private(*player_id) {
                hash.write_u64(player_id.get());
                hash.write_serialized(parts);
            }
        }
        for (player_id, mechanic_level) in &self.mechanic_levels {
            if sees_private(*player_id) {
                hash.write_u64(player_id.get());
                hash.write_u64(u64::from(*mechanic_level));
            }
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
            if !sees_private(*player_id) {
                continue;
            }
            hash.write_u64(player_id.get());
            hash.write_f32(site.x);
            hash.write_f32(site.y);
            hash.write_serialized(&site.minerals);
            hash.write_serialized(&site.artifacts);
        }
        for (player_id, count) in &self.rescue_counts {
            if sees_private(*player_id) {
                hash.write_u64(player_id.get());
                hash.write_u64(u64::from(*count));
            }
        }
        hash.write_serialized(&self.progression.contracts);
        hash.write_serialized(&self.progression.deep_claim_status);
        if let Some(deadline) = self.progression.escape_deadline {
            hash.write_u64(deadline.get());
        }
        for (player_id, milestone) in &self.progression.deep_reward_milestones {
            if sees_private(*player_id) {
                hash.write_u64(player_id.get());
                hash.write_i32(*milestone);
            }
        }
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
        hash.write_serialized(&self.progression.vault_relay_deadlines);
        hash.write_serialized(&self.scan_markers);
        hash.write_serialized(&self.collapse_warnings);
        hash.write_i32(self.terrain.width());
        hash.write_i32(self.terrain.height());
        hash.write_serialized(self.terrain.settings());
        hash.write_u64(u64::try_from(self.terrain.tiles().len()).unwrap_or(u64::MAX));
        hash.write_u64(self.terrain.digest());
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
        hash.write_serialized(&self.test_mode);
        for player_id in self.discovered_tiles.keys() {
            if sees_private(*player_id) {
                self.hash_discovery(*player_id, &mut hash);
            }
        }
        let transacting: BTreeSet<PlayerId> = self
            .service_transactions
            .iter()
            .map(|transaction| transaction.player_id)
            .filter(|player_id| sees_private(*player_id))
            .collect();
        for player_id in transacting {
            for transaction in self
                .service_transactions
                .iter()
                .filter(|transaction| transaction.player_id == player_id)
            {
                hash.write_u64(transaction.player_id.get());
                hash.write_serialized(&transaction.kind);
                hash.write_u64(u64::from(transaction.credits_before));
                hash.write_u64(u64::from(transaction.credits_after));
                hash.write_u64(u64::from(transaction.cargo_before));
                hash.write_u64(u64::from(transaction.cargo_after));
            }
        }
        hash.finish()
    }
}
