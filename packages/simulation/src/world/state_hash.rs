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
        self.hash_visible(|_| true, |_| true)
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
        sees_store: impl Fn(crate::place::StoreKey) -> bool,
    ) -> SimulationStateHash {
        // F11: a client holds the ground (tiles and entities) of its own store only.
        let sees_home = sees_store(crate::place::StoreKey::Home);
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
            hash.write_serialized(&site.items);
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
        hash.write_serialized(&self.progression.crew_quests);
        // Absent until ground first comes down, so older worlds keep their hashes.
        if !self.progression.settled_ground.is_empty() {
            hash.write_serialized(&self.progression.settled_ground);
        }
        hash.write_serialized(&self.scan_markers);
        hash.write_serialized(&self.collapse_warnings);
        hash.write_i32(self.terrain.width());
        hash.write_i32(self.terrain.height());
        hash.write_serialized(self.terrain.settings());
        // F7: the local pixel frame, absent for the fixed frame at the origin (V1-V10 worlds
        // keep their hashes).
        if self.terrain.frame() != crate::position::Frame::FIXED {
            hash.write_serialized(&self.terrain.frame());
        }
        // Wave 11: a planet's ground params (absent for every other world).
        if let Some(planet) = self.terrain.planet_ground() {
            // Unsettled bodies hash as they did before the aliens' `folk` byte (Wave 12).
            let params = planet.params();
            if params.folk == 0 {
                hash.write_serialized(&(
                    params.seed,
                    params.radius,
                    params.gravity_milli,
                    params.theme,
                ));
            } else {
                hash.write_serialized(params);
            }
        }
        if sees_home {
            hash.write_u64(u64::try_from(self.terrain.stored_tile_count()).unwrap_or(u64::MAX));
            hash.write_u64(self.terrain.digest());
        }
        hash.write_u64(self.entities.next_id());
        hash.write_u64(self.entities.next_revision());
        if sees_home {
            hash.write_u64(u64::try_from(self.entities.len()).unwrap_or(u64::MAX));
            for entity in self.entities.iter() {
                hash.write_serialized(entity);
            }
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
        hash.write_u64(self.next_bomb_id);
        hash.write_u64(self.rng.state());
        hash.write_serialized(&self.test_mode);
        // Skipped while untouched, so worlds from before enemies hash exactly as they did.
        if !self.enemies.is_default() {
            hash.write_serialized(&self.enemies);
        }
        // Absent until something is built, so worlds without bases keep their hashes.
        if !self.bases.is_empty() {
            hash.write_serialized(&self.bases);
        }
        // Skipped while it is a new world's standard clock, so older worlds keep their hashes.
        if !self.clock.is_default() {
            hash.write_serialized(&self.clock);
        }
        // Absent until someone talks to a person, so worlds without people keep their hashes.
        if !self.people.is_empty() {
            hash.write_serialized(&self.people);
        }
        // Absent until a plot is bought, so worlds without homes keep their hashes.
        if !self.homes.is_empty() {
            hash.write_serialized(&self.homes);
        }
        // Absent until someone opens a tomb (WP-TOMBS), so older worlds keep their hashes.
        if !self.tombs.is_empty() {
            hash.write_serialized(&self.tombs);
        }
        // Absent until a field zone is placed, so older worlds keep their hashes.
        if !self.fields.is_empty() {
            hash.write_serialized(&self.fields);
        }
        // Absent on a classic world that never used a lift, beacon or outpost (Part III F10),
        // so older worlds keep their hashes.
        if !self.logistics.is_empty() {
            hash.write_serialized(&self.logistics);
        }
        // Absent until something happens in a Deep Descent world's Crust (Wave 8).
        if !self.crust.is_empty() {
            hash.write_serialized(&self.crust);
        }
        // Absent until something of the Old Deep happens (Wave 8), so older worlds keep their
        // hashes.
        if !self.old_deep.is_empty() {
            hash.write_serialized(&self.old_deep);
        }
        // Absent until something happens in the Living Dark (Wave 8), so older worlds keep
        // their hashes.
        if !self.living_dark.is_empty() {
            hash.write_serialized(&self.living_dark);
        }
        // Absent until the crew finds something of the Fathers' expedition (Wave 8 story), so
        // older worlds keep their hashes.
        if !self.expedition.is_empty() {
            hash.write_serialized(&self.expedition);
        }
        // Absent until something happens in the Drowned and Frozen (Wave 9), so older worlds
        // keep their hashes.
        if !self.drowned.is_empty() {
            hash.write_serialized(&self.drowned);
        }
        // Absent until something happens in the Hollow World (Wave 9), so older worlds keep
        // their hashes.
        if !self.hollow.is_empty() {
            hash.write_serialized(&self.hollow);
        }
        // Absent until something happens in the distortion strata (Wave 9), so older worlds
        // keep their hashes.
        if !self.distortion.is_empty() {
            hash.write_serialized(&self.distortion);
        }
        // Absent until something happens in the far Strange Layers (Wave 9), so older worlds
        // keep their hashes.
        if !self.far_strange.is_empty() {
            hash.write_serialized(&self.far_strange);
        }
        // Absent until someone meets an alien species (Wave 12), so older worlds keep their
        // hashes.
        if !self.aliens.is_empty() {
            hash.write_serialized(&self.aliens);
        }
        // Absent until something happens in the buried city (Wave 10), so older worlds keep
        // their hashes.
        if !self.ancient_city.is_empty() {
            hash.write_serialized(&self.ancient_city);
        }
        // Absent until something happens in Descent VII part B (Wave 10), so older worlds keep
        // their hashes.
        if !self.garrison.is_empty() {
            hash.write_serialized(&self.garrison);
        }
        // Absent until something happens in Descent VIII (Wave 10), so older worlds keep their
        // hashes.
        if !self.heart.is_empty() {
            hash.write_serialized(&self.heart);
        }
        // Absent until someone flies in space (Wave 11), so older worlds keep their hashes.
        if !self.flight.is_empty() {
            hash.write_serialized(&self.flight);
        }
        // Absent until the crew mines a rock, gathers comet dust or flies a wormhole (Wave 12).
        if !self.space_systems.is_empty() {
            hash.write_serialized(&self.space_systems);
        }
        // Absent until someone used a station (Wave 12), so older worlds keep their hashes.
        if !self.stations.is_empty() {
            hash.write_serialized(&self.stations);
        }
        // Absent until something hostile happens in space (Wave 12), so older worlds keep
        // their hashes.
        if !self.hostiles.is_empty() {
            hash.write_serialized(&self.hostiles);
        }
        // Absent until the endgame began (Wave 13), so older worlds keep their hashes.
        if !self.endgame.is_empty() {
            hash.write_serialized(&self.endgame);
        }
        // Absent until the galaxy story begins (Wave 13), so older worlds keep their hashes.
        if !self.galaxy_story.is_empty() {
            hash.write_serialized(&self.galaxy_story);
        }
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
        // Every player's place and the ground of the stores this view holds (F11; absent
        // until someone leaves the home store, so older worlds keep their hashes).
        self.hash_places(&mut hash, &sees_store);
        hash.finish()
    }
}
