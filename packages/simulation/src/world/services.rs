//! Town service transactions submitted as [`crate::PlayerCommand::Service`] actions.
//!
//! Every building menu (fuel, repair, depot, shop, bank, explosives, salvage, HQ, crafting, town
//! development, expedition board) resolves here, inside the fixed tick, so the transaction is
//! authoritative, replicated and replayable like any other action.

use crate::{
    command::PlayerCommand, ids::PlayerId, service::SessionServiceRequest,
    summary::PlayerScopedCommandOutcome,
};

use super::WorldState;

/// Bomb bundles sold by the explosives shack: `(charges, credits)`.
pub const BOMB_BUNDLES: [(u32, u32); 2] = [(3, 55), (7, 120)];
/// Price of the shaped-charge mining rocket pack.
pub const MINING_ROCKETS_COST: u32 = 180;
/// Charges in one mining rocket pack.
pub const MINING_ROCKETS_COUNT: u32 = 4;
/// Explosives shack level that unlocks mining rockets.
pub const MINING_ROCKETS_SHACK_LEVEL: u8 = 4;

impl WorldState {
    /// Resolves one town service transaction for `player_id`.
    pub fn apply_service_request(
        &mut self,
        player_id: PlayerId,
        request: SessionServiceRequest,
    ) -> PlayerScopedCommandOutcome {
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        let direct = match request {
            SessionServiceRequest::Refuel { .. } => Some(PlayerCommand::Refuel),
            SessionServiceRequest::Repair { .. } => Some(PlayerCommand::Repair),
            SessionServiceRequest::SellCargo => Some(PlayerCommand::SellCargo),
            SessionServiceRequest::BuyUpgrade { index } => {
                Some(PlayerCommand::BuyUpgrade { index })
            }
            _ => None,
        };
        if let Some(command) = direct {
            return self.apply_player_command(player_id, &command);
        }
        if let Some(outcome) = self.apply_canonical_progression_service(player_id, request) {
            return outcome;
        }
        if let Some(outcome) = self.apply_canonical_salvage_service(player_id, request) {
            return outcome;
        }
        self.apply_shop_and_board_service(player_id, request)
    }

    fn apply_shop_and_board_service(
        &mut self,
        player_id: PlayerId,
        request: SessionServiceRequest,
    ) -> PlayerScopedCommandOutcome {
        let shack_level = self.progression.town_development.explosives_shack_level;
        match request {
            SessionServiceRequest::BuyBombBundle { count, cost } => {
                if !BOMB_BUNDLES.contains(&(count, cost)) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let Some(player) = self.players.get_mut(&player_id) else {
                    return PlayerScopedCommandOutcome::UnknownPlayer;
                };
                if player.credits < cost {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                player.credits -= cost;
                let bonus = u32::from(shack_level / 2);
                player.bombs = player.bombs.saturating_add(count + bonus);
                PlayerScopedCommandOutcome::Applied
            }
            SessionServiceRequest::BuyMiningRockets => {
                let Some(player) = self.players.get_mut(&player_id) else {
                    return PlayerScopedCommandOutcome::UnknownPlayer;
                };
                if shack_level < MINING_ROCKETS_SHACK_LEVEL || player.credits < MINING_ROCKETS_COST
                {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                player.credits -= MINING_ROCKETS_COST;
                player.bombs = player.bombs.saturating_add(MINING_ROCKETS_COUNT);
                PlayerScopedCommandOutcome::Applied
            }
            SessionServiceRequest::ClaimFreeTestCharge => {
                let Some(player) = self.players.get_mut(&player_id) else {
                    return PlayerScopedCommandOutcome::UnknownPlayer;
                };
                player.bombs = player.bombs.saturating_add(1);
                PlayerScopedCommandOutcome::Applied
            }
            SessionServiceRequest::SalvagePatchHull => {
                let Some(player) = self.players.get_mut(&player_id) else {
                    return PlayerScopedCommandOutcome::UnknownPlayer;
                };
                let patch = (player.max_hull() * 0.12).ceil();
                player.hull = (player.hull + patch).min(player.max_hull());
                PlayerScopedCommandOutcome::Applied
            }
            SessionServiceRequest::CompleteDepotWork => {
                let outcome = self.complete_canonical_progression_for_player(player_id);
                if outcome == PlayerScopedCommandOutcome::IgnoredUnavailable {
                    self.complete_canonical_side_contract(player_id)
                } else {
                    outcome
                }
            }
            SessionServiceRequest::StartSideContract => self.start_canonical_side_contract(),
            SessionServiceRequest::AcceptExpedition { index } => {
                self.accept_canonical_expedition(index)
            }
            SessionServiceRequest::AbandonExpedition { index } => {
                if index < self.progression.active_expeditions.len() {
                    self.progression.active_expeditions.remove(index);
                    PlayerScopedCommandOutcome::Applied
                } else {
                    PlayerScopedCommandOutcome::IgnoredUnavailable
                }
            }
            // Handled by `apply_service_request` or the canonical progression/salvage services.
            SessionServiceRequest::Refuel { .. }
            | SessionServiceRequest::Repair { .. }
            | SessionServiceRequest::SellCargo
            | SessionServiceRequest::BuyUpgrade { .. }
            | SessionServiceRequest::Finance
            | SessionServiceRequest::BuyInsurance
            | SessionServiceRequest::CraftRecipe { .. }
            | SessionServiceRequest::UpgradeTownBuilding { .. }
            | SessionServiceRequest::SalvageRecoverLostCargo
            | SessionServiceRequest::SalvageLaunchDrone
            | SessionServiceRequest::SalvageRecoverWreckedPart
            | SessionServiceRequest::SalvageClearCollapseZones
            | SessionServiceRequest::SalvageSellScrapTip
            | SessionServiceRequest::SellScanData
            | SessionServiceRequest::AutoSortLowGradeCargo => {
                PlayerScopedCommandOutcome::IgnoredUnavailable
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        InputSequence, PlayerCommand, PlayerId, PlayerScopedCommandOutcome, SequencedPlayerCommand,
        SessionServiceRequest, SimulationTick, TerrainRevisionTracker, TickInputs, WorldImport,
        WorldState, input::PlayerTickInput, terrain::TILE_SIZE,
    };

    const PLAYER: PlayerId = PlayerId::new(1);

    fn world_with_credits(credits: u32) -> WorldState {
        let mut import = WorldImport::new_game(PLAYER);
        import.player.credits = credits;
        WorldState::from_import(&import)
    }

    #[test]
    fn bomb_bundles_charge_listed_prices_only() {
        let mut world = world_with_credits(200);
        assert_eq!(
            world.apply_service_request(
                PLAYER,
                SessionServiceRequest::BuyBombBundle { count: 3, cost: 55 }
            ),
            PlayerScopedCommandOutcome::Applied
        );
        let player = world.player(PLAYER).expect("player");
        assert_eq!((player.credits, player.bombs), (145, 3));
        assert_eq!(
            world.apply_service_request(
                PLAYER,
                SessionServiceRequest::BuyBombBundle { count: 99, cost: 1 }
            ),
            PlayerScopedCommandOutcome::IgnoredUnavailable
        );
        assert_eq!(world.player(PLAYER).expect("player").bombs, 3);
    }

    #[test]
    fn service_actions_resolve_inside_the_fixed_tick() {
        let mut world = world_with_credits(500);
        world.player_mut(PLAYER).expect("player").fuel = 10.0;
        let tick = world.simulation_tick();
        let mut inputs = TickInputs::default();
        inputs.players.insert(
            PLAYER,
            PlayerTickInput {
                controls: None,
                actions: vec![SequencedPlayerCommand {
                    player_id: PLAYER,
                    sequence: InputSequence::new(0),
                    target_tick: tick,
                    command: PlayerCommand::Service(SessionServiceRequest::Refuel { menu_item: 2 }),
                }],
            },
        );
        let output = world.simulate_tick(tick, &inputs, &mut TerrainRevisionTracker::default());
        assert_eq!(
            output.action_outcomes[0].outcome,
            PlayerScopedCommandOutcome::Applied
        );
        let player = world.player(PLAYER).expect("player");
        assert!(player.fuel > 10.0);
        assert!(player.credits < 500);
        assert_eq!(world.simulation_tick(), SimulationTick::new(tick.get() + 1));
    }

    #[test]
    fn side_contracts_and_expedition_board_go_through_service_requests() {
        let mut world = world_with_credits(0);
        assert_eq!(
            world.apply_service_request(PLAYER, SessionServiceRequest::StartSideContract),
            PlayerScopedCommandOutcome::Applied
        );
        assert_eq!(world.progression().active_side_contracts.len(), 1);
        assert_eq!(
            world.apply_service_request(
                PLAYER,
                SessionServiceRequest::AbandonExpedition { index: 0 }
            ),
            PlayerScopedCommandOutcome::IgnoredUnavailable
        );
    }

    #[test]
    fn ticks_reveal_tiles_around_every_rig() {
        let mut world = WorldState::new_game(PLAYER);
        let tile = world
            .player(PLAYER)
            .expect("player")
            .tile_position(TILE_SIZE);
        assert_eq!(world.discovered_tile_count(PLAYER), 0);
        let tick = world.simulation_tick();
        let _output = world.simulate_tick(
            tick,
            &TickInputs::default(),
            &mut TerrainRevisionTracker::default(),
        );
        assert!(world.discovered_tile_count(PLAYER) > 0);
        assert!(world.discovered_tiles_snapshot(PLAYER).contains(&tile));
    }
}
