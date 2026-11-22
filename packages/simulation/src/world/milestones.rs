//! Milestones inside the tick ([`crate::milestone`]): per-player records (depth, altitude,
//! kills, artifacts, upgrades), first-time milestones and their cosmetic rewards, and the
//! upgrade-installed event the shop's purchase moment plays from.

use crate::{
    command::PlayerCommand,
    economy::{UpgradeKind, current_level},
    event::{GameplayEventKind, TickOutput},
    ids::PlayerId,
    item::ItemKind,
    milestone::{Milestone, MilestoneFacts},
    service::SessionServiceRequest,
};

use super::WorldState;

impl WorldState {
    /// The [`GameplayEventKind::UpgradeInstalled`] for an applied `command`, if it bought an
    /// upgrade (read after the purchase, so `level` is the tier now fitted).
    pub(super) fn upgrade_installed_event(
        &self,
        player_id: PlayerId,
        command: &PlayerCommand,
    ) -> Option<GameplayEventKind> {
        let index = match command {
            PlayerCommand::BuyUpgrade { index }
            | PlayerCommand::Service(SessionServiceRequest::BuyUpgrade { index }) => *index,
            _ => return None,
        };
        let upgrade = *UpgradeKind::ALL.get(index)?;
        let player = self.players.get(&player_id)?;
        Some(GameplayEventKind::UpgradeInstalled {
            player_id,
            upgrade,
            level: current_level(player, upgrade),
        })
    }

    /// Updates every player's records from this tick's state and events, then records newly
    /// met milestones (unlocking their rewards). A player's first check is silent (see
    /// [`crate::milestone`]). Test-mode worlds count nothing.
    pub(super) fn advance_milestones(&mut self, output: &mut TickOutput) {
        if self.is_test_mode() {
            return;
        }
        let tick = self.simulation_tick.get();
        // Event-driven records first: kills, artifact pickups and upgrades bought this tick.
        let mut tallies: Vec<(PlayerId, u32, u32, u32)> = Vec::new();
        let mut tally = |player_id: PlayerId, kills: u32, artifacts: u32, upgrades: u32| {
            if let Some(entry) = tallies.iter_mut().find(|entry| entry.0 == player_id) {
                entry.1 += kills;
                entry.2 += artifacts;
                entry.3 += upgrades;
            } else {
                tallies.push((player_id, kills, artifacts, upgrades));
            }
        };
        for event in &output.events {
            match event.kind {
                GameplayEventKind::EnemyKilled {
                    by: Some(player_id),
                    ..
                } => tally(player_id, 1, 0, 0),
                GameplayEventKind::ItemsCollected {
                    player_id,
                    item: ItemKind::Artifact(_),
                    count,
                    ..
                } => tally(player_id, 0, count, 0),
                GameplayEventKind::UpgradeInstalled { player_id, .. } => tally(player_id, 0, 0, 1),
                _ => {}
            }
        }
        for (player_id, kills, artifacts, upgrades) in tallies {
            if let Some(player) = self.players.get_mut(&player_id) {
                let records = &mut player.milestones.records;
                records.enemies_defeated = records.enemies_defeated.saturating_add(kills);
                records.artifacts_found = records.artifacts_found.saturating_add(artifacts);
                records.upgrades_bought = records.upgrades_bought.saturating_add(upgrades);
            }
        }
        let player_ids: Vec<PlayerId> = self.players.keys().copied().collect();
        let sky_bands = self.terrain.settings().generator.sky_bands();
        for player_id in player_ids {
            let Some(player) = self.players.get(&player_id) else {
                continue;
            };
            let position = player.tile_in(self.frame());
            // The heightmap and band lookups run only when a record could move: a row deeper
            // than the depth record, or flight above the classic surface row (row 0, below
            // every mountain top) for the altitude record. Most ticks skip both.
            let records = player.milestones.records;
            let deeper = position.y > records.deepest_row;
            let surface = (deeper || position.y < 0).then(|| self.terrain.surface_row(position.x));
            let underground = surface.is_some_and(|surface| position.y >= surface);
            let altitude = match surface {
                Some(surface) if position.y < 0 => (surface - position.y - 1).max(0),
                _ => 0,
            };
            let band = if deeper && underground {
                u8::try_from(self.terrain.band_at(position).index()).unwrap_or(u8::MAX)
            } else {
                0
            };
            let owns_claim = !player.milestones.has(Milestone::FirstClaim)
                && self.bases.claims().any(|claim| claim.owner == player_id);
            let Some(player) = self.players.get_mut(&player_id) else {
                continue;
            };
            let records = &mut player.milestones.records;
            records.highest_altitude = records.highest_altitude.max(altitude);
            if deeper && underground {
                records.deepest_row = records.deepest_row.max(position.y);
                records.deepest_band =
                    Some(records.deepest_band.map_or(band, |best| best.max(band)));
            }
            let facts = MilestoneFacts {
                deepest_band: records.deepest_band.map(usize::from),
                owns_claim,
                sky_bands,
            };
            let silent = !player.milestones.primed;
            player.milestones.primed = true;
            for milestone in Milestone::newly_met(player, &facts) {
                if player.earn_milestone(milestone, tick) && !silent {
                    output.push_event(GameplayEventKind::MilestoneEarned {
                        player_id,
                        milestone,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CosmeticItem, GameplayEventKind, PlayerCommand, PlayerId, SectionKey,
        SessionServiceRequest, TickInputs, WorldState,
        cosmetics::{Decal, RoofAttachment},
        economy::{SurfaceZone, UpgradeKind},
        milestone::Milestone,
        surface::SURFACE_BUILDINGS,
        terrain::TILE_SIZE,
    };

    const PLAYER: PlayerId = PlayerId::new(1);
    const OTHER: PlayerId = PlayerId::new(2);

    fn step(world: &mut WorldState, inputs: &TickInputs) -> Vec<GameplayEventKind> {
        let tick = world.simulation_tick();
        world
            .simulate_tick(tick, inputs)
            .events
            .into_iter()
            .map(|event| event.kind)
            .collect()
    }

    fn idle(world: &mut WorldState) -> Vec<GameplayEventKind> {
        step(world, &TickInputs::default())
    }

    fn at_shop(world: &mut WorldState) {
        let building = SURFACE_BUILDINGS
            .iter()
            .find(|building| building.zone == SurfaceZone::Shop)
            .expect("shop");
        let rig = world.player_mut(PLAYER).expect("player");
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinate")]
        let x = (crate::position::Frame::FIXED.rel_col(building.tile_x) + 2.5) * TILE_SIZE;
        rig.x = x;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }

    fn earned(events: &[GameplayEventKind]) -> Vec<Milestone> {
        events
            .iter()
            .filter_map(|event| match event {
                GameplayEventKind::MilestoneEarned { milestone, .. } => Some(*milestone),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn buying_the_first_upgrade_installs_it_and_earns_a_milestone() {
        let mut world = WorldState::new_game(PLAYER);
        idle(&mut world);
        at_shop(&mut world);
        world.player_mut(PLAYER).expect("player").credits = 10_000;
        let inputs = TickInputs::default().act(
            PLAYER,
            PlayerCommand::Service(SessionServiceRequest::BuyUpgrade { index: 0 }),
        );
        let events = step(&mut world, &inputs);
        assert!(events.contains(&GameplayEventKind::UpgradeInstalled {
            player_id: PLAYER,
            upgrade: UpgradeKind::Drill,
            level: 2,
        }));
        assert_eq!(earned(&events), vec![Milestone::FirstUpgrade]);
        let player = world.player(PLAYER).expect("player");
        assert_eq!(player.milestones.records.upgrades_bought, 1);
        assert!(
            player
                .cosmetic_unlocks
                .has(CosmeticItem::Decal(Decal::Wrench))
        );
        assert!(earned(&idle(&mut world)).is_empty(), "earned once");
    }

    #[test]
    fn the_first_check_is_silent_for_rigs_from_old_saves() {
        let mut world = WorldState::new_game(PLAYER);
        {
            let rig = world.player_mut(PLAYER).expect("player");
            rig.drill_strength = 10;
            rig.milestones = crate::milestone::MilestoneLog::default();
        }
        let events = idle(&mut world);
        assert!(earned(&events).is_empty(), "no toasts for old progress");
        let player = world.player(PLAYER).expect("player");
        assert!(player.milestones.has(Milestone::DrillTier10));
        assert!(player.milestones.primed);
        // New progress after that is announced.
        world.player_mut(PLAYER).expect("player").engine_level = 5;
        // (A tier 5 engine also lifts the stock tank into the low isles of the V8 sky.)
        assert!(earned(&idle(&mut world)).contains(&Milestone::EngineTier5));
    }

    #[test]
    fn deep_dives_set_records_and_award_the_depth_trophy() {
        let mut world = WorldState::new_game(PLAYER);
        idle(&mut world);
        let row = world.terrain().scaled_row(100);
        let rig = world.player_mut(PLAYER).expect("player");
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinate")]
        let y = (crate::position::Frame::FIXED.rel_row(row) + 0.5) * TILE_SIZE;
        rig.y = y;
        let events = idle(&mut world);
        let milestones = earned(&events);
        assert!(
            milestones.contains(&Milestone::ReachedStone),
            "{milestones:?}"
        );
        assert!(
            milestones.contains(&Milestone::FirstDeepLayer),
            "{milestones:?}"
        );
        let player = world.player(PLAYER).expect("player");
        assert!(player.milestones.records.deepest_row >= row - 1);
        assert!(
            player
                .cosmetic_unlocks
                .has(CosmeticItem::Roof(RoofAttachment::Trophy))
        );
    }

    #[test]
    fn test_mode_counts_no_milestones() {
        let mut world = WorldState::new_game(PLAYER);
        world.enable_test_mode();
        idle(&mut world);
        let player = world.player(PLAYER).expect("player");
        assert!(player.milestones.earned.is_empty());
        assert!(!player.milestones.primed);
    }

    #[test]
    fn milestones_replicate_privately_and_survive_saves_and_parking() {
        let mut authority = WorldState::new_game(PLAYER);
        authority.add_player(OTHER);
        idle(&mut authority);
        authority.player_mut(PLAYER).expect("player").engine_level = 5;
        idle(&mut authority);
        assert!(
            authority
                .player(PLAYER)
                .expect("player")
                .milestones
                .has(Milestone::EngineTier5)
        );
        // The owner's loadout carries the log; other players never see it.
        let mut replica = WorldState::new_game(PLAYER);
        let key = SectionKey::Loadout(PLAYER);
        assert!(!key.visible_to(Some(OTHER)));
        replica.apply_section(key, authority.section(key).expect("loadout"));
        assert_eq!(
            replica.player(PLAYER).expect("player").milestones,
            authority.player(PLAYER).expect("player").milestones
        );
        let redacted = authority.redacted_for(Some(OTHER));
        assert!(
            redacted
                .player(PLAYER)
                .expect("player")
                .milestones
                .earned
                .is_empty()
        );
        // Saves keep it (serde round trip of the whole world).
        let json = serde_json::to_string(&authority).expect("serialize");
        let loaded: WorldState = serde_json::from_str(&json).expect("load");
        assert_eq!(
            loaded.player(PLAYER).expect("player").milestones,
            authority.player(PLAYER).expect("player").milestones
        );
        // A parked player keeps it.
        assert!(authority.park_player(PLAYER));
        assert!(authority.unpark_player(PLAYER));
        assert!(
            authority
                .player(PLAYER)
                .expect("player")
                .milestones
                .has(Milestone::EngineTier5)
        );
    }
}
