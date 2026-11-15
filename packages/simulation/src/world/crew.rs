//! The world side of persistent crew identity (WP-PERSIST, see [`crate::crew`]): crew records for
//! live rigs, parking players outside the live world and restoring them.
//!
//! Like `add_player` / `remove_player`, these are roster operations the session runs between
//! ticks (a player joining or leaving), never gameplay inside [`WorldState::simulate_tick`].

use crate::{
    crew::{CrewMember, CrewRoster, ParkedPlayer, ProfileId},
    ids::PlayerId,
};

use super::{WorldState, movement::authoritative_player_collides};

impl WorldState {
    /// The crew roster (identities and parked players).
    #[must_use]
    pub const fn crew(&self) -> &CrewRoster {
        &self.crew
    }

    /// The identity record of a live player.
    #[must_use]
    pub fn crew_member(&self, player_id: PlayerId) -> Option<&CrewMember> {
        self.crew.members.get(&player_id)
    }

    /// Records who plays a live player (replacing any earlier record). Ignored for players not
    /// in the live world.
    pub fn set_crew_member(&mut self, player_id: PlayerId, member: CrewMember) {
        if self.players.contains_key(&player_id) {
            self.crew.members.insert(player_id, member);
        }
    }

    /// The player (live or parked) that belongs to `profile`, preferring a live one.
    #[must_use]
    pub fn player_for_profile(&self, profile: ProfileId) -> Option<PlayerId> {
        self.crew
            .members
            .iter()
            .filter(|(player_id, _)| self.players.contains_key(player_id))
            .chain(
                self.crew
                    .parked
                    .iter()
                    .map(|(player_id, parked)| (player_id, &parked.member)),
            )
            .find(|(_, member)| member.profile == Some(profile))
            .map(|(player_id, _)| *player_id)
    }

    /// Parked players in id order.
    pub fn parked_players(&self) -> impl Iterator<Item = (PlayerId, &ParkedPlayer)> + '_ {
        self.crew
            .parked
            .iter()
            .map(|(player_id, parked)| (*player_id, parked))
    }

    /// A parked player.
    #[must_use]
    pub fn parked_player(&self, player_id: PlayerId) -> Option<&ParkedPlayer> {
        self.crew.parked.get(&player_id)
    }

    /// Whether `player_id` is parked.
    #[must_use]
    pub fn is_parked(&self, player_id: PlayerId) -> bool {
        self.crew.parked.contains_key(&player_id)
    }

    /// Every player id in use, live or parked (new players must not reuse a parked id).
    pub fn all_player_ids(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.players.keys().chain(self.crew.parked.keys()).copied()
    }

    /// Moves a live player and all of their per-player state out of the live world into the
    /// crew roster: the rig is no longer simulated, rendered or replicated, but saves with the
    /// world and comes back exactly with [`Self::unpark_player`]. Transient state (an active
    /// drill, scanner cooldown, momentum) is dropped. A player without a crew record is parked
    /// with an unnamed one. Returns `false` if the player is not live.
    pub fn park_player(&mut self, player_id: PlayerId) -> bool {
        let Some(mut player) = self.players.get(&player_id).cloned() else {
            return false;
        };
        player.velocity_x = 0.0;
        player.velocity_y = 0.0;
        let member = self.crew.members.remove(&player_id).unwrap_or_default();
        // A player on foot parks both bodies: the pilot leaves the world with the rig.
        let pilot = self.take_pilot_for_parking(player_id);
        let parked = ParkedPlayer {
            member,
            player,
            equipped_rig_parts: self
                .equipped_rig_parts
                .get(&player_id)
                .cloned()
                .unwrap_or_default(),
            mechanic_level: self
                .mechanic_levels
                .get(&player_id)
                .copied()
                .unwrap_or_default(),
            deep_run: self.deep_run_states.get(&player_id).copied(),
            lost_cargo: self.lost_cargo_sites.get(&player_id).cloned(),
            rescue_count: self.rescue_counts.get(&player_id).copied().unwrap_or(0),
            deep_reward_milestone: self
                .progression
                .deep_reward_milestones
                .get(&player_id)
                .copied(),
            crew_quests: self
                .progression
                .crew_quests
                .active
                .iter()
                .filter(|quest| quest.owner == player_id)
                .cloned()
                .collect(),
            discovered: self
                .discovered_tiles
                .get(&player_id)
                .cloned()
                .unwrap_or_default(),
            parked_at: self.simulation_tick,
            pilot,
        };
        // Parked players keep their placed C4 armed for when they return (remove_player would
        // disarm it). Their bases (claims, home points) stay keyed by their id, and everything
        // else of theirs from the wave 2 tracks (hopper, collect toggle, weapons, explosive
        // items, village and cave journals, cosmetics) rides inside `Player`.
        self.remove_player_with(player_id, false);
        self.crew.parked.insert(player_id, parked);
        true
    }

    /// Puts a parked player back into the live world with everything they had, where they left
    /// their rig (or at a free spawn slot if the terrain there has since been filled in).
    /// Returns `false` if the player is not parked or the id is live.
    pub fn unpark_player(&mut self, player_id: PlayerId) -> bool {
        if self.players.contains_key(&player_id) {
            return false;
        }
        let Some(parked) = self.crew.parked.remove(&player_id) else {
            return false;
        };
        let ParkedPlayer {
            member,
            mut player,
            equipped_rig_parts,
            mechanic_level,
            deep_run,
            lost_cargo,
            rescue_count,
            deep_reward_milestone,
            crew_quests,
            discovered,
            parked_at: _,
            pilot,
        } = parked;
        if authoritative_player_collides(&self.terrain, player.x, player.y) {
            let (x, y) = self.free_spawn_position(self.player_template.x, self.player_template.y);
            player.x = x;
            player.x_remainder = 0.0;
            player.y = y;
        }
        self.insert_player(player_id, player);
        self.equipped_rig_parts
            .insert(player_id, equipped_rig_parts);
        self.mechanic_levels.insert(player_id, mechanic_level);
        if let Some(deep_run) = deep_run {
            self.deep_run_states.insert(player_id, deep_run);
        }
        if let Some(site) = lost_cargo {
            self.lost_cargo_sites.insert(player_id, site);
        }
        if rescue_count > 0 {
            self.rescue_counts.insert(player_id, rescue_count);
        }
        if let Some(milestone) = deep_reward_milestone {
            self.progression
                .deep_reward_milestones
                .insert(player_id, milestone);
        }
        self.progression.crew_quests.active.extend(crew_quests);
        self.discovered_tiles.insert(player_id, discovered);
        self.crew.members.insert(player_id, member);
        if let Some(pilot) = pilot {
            self.restore_parked_pilot(pilot);
        }
        true
    }

    /// Forgets a parked player for good. Returns whether one was removed.
    pub fn discard_parked_player(&mut self, player_id: PlayerId) -> bool {
        self.crew.parked.remove(&player_id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        PlayerId, WorldState,
        crew::{CrewMember, ProfileId},
        terrain::{MineralKind, TILE_SIZE},
    };

    const HOST: PlayerId = PlayerId::new(0);
    const ALEX: PlayerId = PlayerId::new(1);

    fn alex() -> CrewMember {
        CrewMember {
            profile: Some(ProfileId::new(0xa1e8)),
            name: "Alex".to_owned(),
            seat: None,
        }
    }

    #[test]
    fn parking_moves_a_player_out_and_unparking_restores_everything() {
        let mut world = WorldState::new_game(HOST);
        world.add_player(ALEX);
        world.set_crew_member(ALEX, alex());
        {
            let rig = world.player_mut(ALEX).expect("alex");
            rig.credits = 7_777;
            rig.cargo_bay.insert(MineralKind::Gold, 3);
            rig.drill_strength = 4;
            rig.x = TILE_SIZE.mul_add(10.0, rig.x);
        }
        let before = world.player(ALEX).cloned().expect("alex");
        let hash_before = world.simulation_state_hash();

        assert!(world.park_player(ALEX));
        assert!(world.player(ALEX).is_none(), "not in the live world");
        assert_eq!(world.player_ids().collect::<Vec<_>>(), [HOST]);
        assert!(world.is_parked(ALEX));
        assert_eq!(world.player_for_profile(ProfileId::new(0xa1e8)), Some(ALEX));
        assert_eq!(world.all_player_ids().collect::<Vec<_>>(), [HOST, ALEX]);
        assert!(!world.park_player(ALEX), "already parked");

        // Survives a save round trip.
        let json = serde_json::to_string(&world).expect("serialize");
        let mut world: WorldState = serde_json::from_str(&json).expect("deserialize");

        assert!(world.unpark_player(ALEX));
        let after = world.player(ALEX).expect("alex is back");
        assert_eq!(after.credits, 7_777);
        assert_eq!(after.cargo_bay, before.cargo_bay);
        assert_eq!(after.drill_strength, 4);
        assert!((after.x - before.x).abs() < f32::EPSILON);
        assert_eq!(world.crew_member(ALEX), Some(&alex()));
        assert_eq!(world.simulation_state_hash(), hash_before);
        assert!(!world.unpark_player(ALEX));
    }

    #[test]
    fn a_rig_parked_high_in_the_sky_comes_back_where_it_was() {
        // No flight ceiling (R17): a rig parked thousands of tiles up keeps its altitude
        // through a save round trip and unparks there, not at a spawn slot.
        let mut world = WorldState::new_game(HOST);
        assert!(world.world_settings().generator.has_biomes());
        world.add_player(ALEX);
        let altitude = -5_000.0 * TILE_SIZE;
        if let Some(rig) = world.player_mut(ALEX) {
            rig.y = altitude;
            rig.velocity_y = -300.0;
        }
        let x = world.player(ALEX).expect("alex").x;
        assert!(world.park_player(ALEX));
        let json = serde_json::to_string(&world).expect("serialize");
        let mut world: WorldState = serde_json::from_str(&json).expect("deserialize");
        assert!(world.unpark_player(ALEX));
        let rig = world.player(ALEX).expect("alex is back");
        assert!((rig.y - altitude).abs() < f32::EPSILON, "y {}", rig.y);
        assert!((rig.x - x).abs() < f32::EPSILON);
        assert!(rig.velocity_y.abs() < f32::EPSILON, "parked at rest");
    }

    #[test]
    fn redacted_keyframes_never_carry_the_crew_roster() {
        let mut world = WorldState::new_game(HOST);
        world.add_player(ALEX);
        world.set_crew_member(ALEX, alex());
        world.add_player(PlayerId::new(2));
        assert!(world.park_player(PlayerId::new(2)));
        let redacted = world.redacted_for(Some(ALEX));
        assert!(redacted.crew().is_empty(), "profile ids are credentials");
        assert_eq!(
            redacted.visible_state_hash(Some(ALEX)),
            world.visible_state_hash(Some(ALEX))
        );
    }

    #[test]
    fn profile_seats_are_distinct_and_stable() {
        let base = ProfileId::new(42);
        assert_eq!(base.for_seat(0), base);
        let seats: std::collections::BTreeSet<_> = (0..4).map(|seat| base.for_seat(seat)).collect();
        assert_eq!(seats.len(), 4);
        assert_eq!(base.for_seat(3), ProfileId::new(42).for_seat(3));
        let text = serde_json::to_string(&base.for_seat(2)).expect("json");
        assert_eq!(
            serde_json::from_str::<ProfileId>(&text).expect("parse"),
            base.for_seat(2)
        );
        assert_eq!(
            ProfileId::parse("00000000000000ff"),
            Some(ProfileId::new(255))
        );
        assert_eq!(ProfileId::parse("not hex"), None);
    }
}
