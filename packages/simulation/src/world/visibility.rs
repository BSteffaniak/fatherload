//! What one client may see of the world.
//!
//! Every client receives the shared world (terrain, entities, economy, contracts, ...), every
//! relevant player's per-tick state and every player's public [`super::RigBuild`]. A player's
//! private state — the [`super::PlayerLoadout`] section (inventory, credits, upgrades, kits, rig
//! parts, salvage, rescue count, deep reward milestone, service transactions) and their map
//! discovery — goes only to the client playing that player ([`super::SectionKey::visible_to`],
//! [`WorldState::redacted_for`] for keyframes).
//!
//! A replica therefore holds other players' private state only as the defaults it created them
//! with, so replica checks compare [`WorldState::visible_state_hash`] for the client's own
//! player: the host computes it for each client and that client computes it over its
//! authority; they match exactly when replication is correct.

use crate::{hash::SimulationStateHash, ids::PlayerId};

use super::{
    WorldState,
    discovery::DiscoveredTiles,
    player_sections::{join_player, split_player},
};

impl WorldState {
    /// Hash of the state a client playing `viewer` holds (`None`: no private state at all).
    #[must_use]
    pub fn visible_state_hash(&self, viewer: Option<PlayerId>) -> SimulationStateHash {
        self.hash_visible(|player_id| Some(player_id) == viewer)
    }

    /// A copy of the world with every other player's private state reset to the defaults a
    /// replica gives a player it only knows from the roster: the keyframe sent to `viewer`.
    #[must_use]
    pub fn redacted_for(&self, viewer: Option<PlayerId>) -> Self {
        let mut world = self.clone();
        let others: Vec<PlayerId> = world
            .players
            .keys()
            .copied()
            .filter(|player_id| Some(*player_id) != viewer)
            .collect();
        for player_id in others {
            world.redact_player(player_id);
        }
        world
    }

    fn redact_player(&mut self, player_id: PlayerId) {
        // Exhaustive so a new per-player field must be classified here too.
        let Self {
            timing: _,
            player_template,
            simulation_tick: _,
            won_game: _,
            players,
            equipped_rig_parts,
            mechanic_levels,
            world_event_severity: _,
            world_event_days_remaining: _,
            deep_run_states: _,
            lost_cargo_sites,
            rescue_counts,
            progression,
            scan_markers: _,
            collapse_warnings: _,
            hazards: _,
            falling_boulders: _,
            boulder_warning_deadlines: _,
            boulder_expiry_ticks: _,
            terrain: _,
            bombs: _,
            infrastructure: _,
            active_drills: _,
            active_drill_timers: _,
            scanner_cooldowns: _,
            scanner_cooldown_deadlines: _,
            next_bomb_id: _,
            rng: _,
            discovered_tiles,
            service_transactions,
            test_mode: _,
        } = self;
        if let Some(player) = players.get_mut(&player_id) {
            let (rig, build, _) = split_player(player);
            let (_, _, default_inventory) = split_player(player_template);
            *player = join_player(rig, build, default_inventory);
        }
        equipped_rig_parts.insert(player_id, std::collections::BTreeSet::new());
        mechanic_levels.insert(player_id, 0);
        lost_cargo_sites.remove(&player_id);
        rescue_counts.remove(&player_id);
        progression.deep_reward_milestones.remove(&player_id);
        discovered_tiles.insert(player_id, DiscoveredTiles::default());
        service_transactions.retain(|transaction| transaction.player_id != player_id);
    }
}

#[cfg(test)]
mod tests {
    use crate::{PlayerId, SectionKey, WorldState};

    const OWNER: PlayerId = PlayerId::new(1);
    const OTHER: PlayerId = PlayerId::new(2);

    fn two_player_world() -> WorldState {
        let mut world = WorldState::new_game(OWNER);
        world.add_player(OTHER);
        let other = world.player_mut(OTHER).expect("other");
        other.credits = 999;
        other.hull_level = 3;
        world
    }

    #[test]
    fn loadouts_are_private_and_rig_builds_public() {
        assert!(SectionKey::Loadout(OWNER).visible_to(Some(OWNER)));
        assert!(!SectionKey::Loadout(OWNER).visible_to(Some(OTHER)));
        assert!(!SectionKey::Loadout(OWNER).visible_to(None));
        assert!(SectionKey::RigBuild(OWNER).visible_to(Some(OTHER)));
        assert!(SectionKey::Economy.visible_to(None));
    }

    #[test]
    fn redaction_hides_private_state_but_keeps_the_visible_hash() {
        let world = two_player_world();
        let redacted = world.redacted_for(Some(OWNER));
        let other = redacted.player(OTHER).expect("other stays in the roster");
        assert_eq!(other.credits, world.player_template().credits);
        assert_eq!(other.hull_level, 3, "the rig build is public");
        assert_eq!(
            redacted.visible_state_hash(Some(OWNER)),
            world.visible_state_hash(Some(OWNER))
        );
        assert_ne!(
            redacted.simulation_state_hash(),
            world.simulation_state_hash()
        );
        // The owner's own private state is part of its visible hash.
        let mut richer = world.clone();
        richer.player_mut(OWNER).expect("owner").credits += 1;
        assert_ne!(
            richer.visible_state_hash(Some(OWNER)),
            world.visible_state_hash(Some(OWNER))
        );
        // Another player's private state is not.
        let mut other_richer = world.clone();
        other_richer.player_mut(OTHER).expect("other").credits += 1;
        assert_eq!(
            other_richer.visible_state_hash(Some(OWNER)),
            world.visible_state_hash(Some(OWNER))
        );
        assert_eq!(
            world.visible_state_hash(Some(OWNER)),
            world.visible_state_hash(Some(OWNER))
        );
    }
}
