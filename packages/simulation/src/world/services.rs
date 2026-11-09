//! Town service transactions submitted as [`crate::PlayerCommand::Service`] actions.
//!
//! Every building menu (fuel, repair, depot, shop, bank, explosives, salvage, HQ, crafting, town
//! development, expedition board) resolves here, inside the fixed tick, so the transaction is
//! authoritative, replicated and replayable like any other action.

use crate::{
    command::PlayerCommand, economy::SurfaceZone, ids::PlayerId, service::SessionServiceRequest,
    summary::PlayerScopedCommandOutcome, surface::surface_zone_at, terrain::TILE_SIZE,
};

use super::WorldState;

/// The surface buildings at which a command may be used, or `None` for commands that are not
/// bound to a building (movement, bombs, scanner, infrastructure, rescue).
///
/// A request offered by several menus (for example finance at both HQ and the bank) lists every
/// building that offers it.
#[must_use]
pub const fn command_service_zones(command: &PlayerCommand) -> Option<&'static [SurfaceZone]> {
    match command {
        PlayerCommand::Refuel => Some(&[SurfaceZone::Fuel]),
        PlayerCommand::Repair => Some(&[SurfaceZone::Repair]),
        PlayerCommand::SellCargo => Some(&[SurfaceZone::Depot]),
        PlayerCommand::BuyUpgrade { .. } => Some(&[SurfaceZone::Shop]),
        PlayerCommand::Service(request) => Some(service_request_zones(*request)),
        PlayerCommand::UseScanner
        | PlayerCommand::PlaceBomb
        | PlayerCommand::PlaceInfrastructure { .. }
        | PlayerCommand::Rescue
        | PlayerCommand::TestMode(_) => None,
    }
}

/// The surface buildings whose menus offer `request`.
#[must_use]
pub const fn service_request_zones(request: SessionServiceRequest) -> &'static [SurfaceZone] {
    match request {
        SessionServiceRequest::Refuel { .. } => &[SurfaceZone::Fuel],
        SessionServiceRequest::Repair { .. } => &[SurfaceZone::Repair],
        SessionServiceRequest::SellCargo
        | SessionServiceRequest::AutoSortLowGradeCargo
        | SessionServiceRequest::SellScanData => &[SurfaceZone::Depot],
        SessionServiceRequest::CompleteDepotWork => {
            &[SurfaceZone::Depot, SurfaceZone::Headquarters]
        }
        SessionServiceRequest::BuyUpgrade { .. } => &[SurfaceZone::Shop],
        SessionServiceRequest::Finance => &[SurfaceZone::Headquarters, SurfaceZone::Bank],
        SessionServiceRequest::BuyInsurance | SessionServiceRequest::StartSideContract => {
            &[SurfaceZone::Bank]
        }
        SessionServiceRequest::BuyBombBundle { .. }
        | SessionServiceRequest::BuyMiningRockets
        | SessionServiceRequest::ClaimFreeTestCharge => &[SurfaceZone::Explosives],
        SessionServiceRequest::SalvagePatchHull
        | SessionServiceRequest::SalvageRecoverLostCargo
        | SessionServiceRequest::SalvageLaunchDrone
        | SessionServiceRequest::SalvageRecoverWreckedPart
        | SessionServiceRequest::SalvageClearCollapseZones
        | SessionServiceRequest::SalvageSellScrapTip => &[SurfaceZone::Salvage],
        SessionServiceRequest::CraftRecipe { .. }
        | SessionServiceRequest::UpgradeTownBuilding { .. }
        | SessionServiceRequest::AcceptExpedition { .. }
        | SessionServiceRequest::AbandonExpedition { .. } => &[SurfaceZone::Headquarters],
    }
}

/// Bomb bundles sold by the explosives shack: `(charges, credits)`.
pub const BOMB_BUNDLES: [(u32, u32); 2] = [(3, 55), (7, 120)];
/// Price of the shaped-charge mining rocket pack.
pub const MINING_ROCKETS_COST: u32 = 180;
/// Charges in one mining rocket pack.
pub const MINING_ROCKETS_COUNT: u32 = 4;
/// Explosives shack level that unlocks mining rockets.
pub const MINING_ROCKETS_SHACK_LEVEL: u8 = 4;

impl WorldState {
    /// The surface building zone `player_id`'s rig stands at, if any.
    #[must_use]
    pub fn player_surface_zone(&self, player_id: PlayerId) -> Option<SurfaceZone> {
        let player = self.players.get(&player_id)?;
        surface_zone_at(player.x, player.y, TILE_SIZE)
    }

    /// Whether `player_id` may issue `command` from where its rig is now.
    ///
    /// Building-bound commands (every service transaction plus the legacy direct refuel, repair,
    /// sell and buy commands) are only allowed while the rig stands at one of the buildings that
    /// offer them. The fixed tick checks this for every submitted action, so a remote client
    /// cannot buy, sell or refuel from anywhere in the world.
    #[must_use]
    pub fn command_allowed_here(&self, player_id: PlayerId, command: &PlayerCommand) -> bool {
        command_service_zones(command).is_none_or(|zones| {
            self.player_surface_zone(player_id)
                .is_some_and(|zone| zones.contains(&zone))
        })
    }

    /// Resolves one town service transaction for `player_id`.
    pub(crate) fn apply_service_request(
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
        PlayerCommand, PlayerId, PlayerScopedCommandOutcome, SessionServiceRequest, SimulationTick,
        TickInputs, WorldImport, WorldState,
        economy::SurfaceZone,
        surface::SURFACE_BUILDINGS,
        terrain::{MineralKind, TILE_SIZE},
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

    /// Moves the rig to the door of `zone`'s building (on the surface, inside its tile span).
    fn stand_at(world: &mut WorldState, zone: SurfaceZone) {
        let building = SURFACE_BUILDINGS
            .iter()
            .find(|building| building.zone == zone)
            .expect("building exists");
        let rig = world.player_mut(PLAYER).expect("player");
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinate")]
        let x = (building.tile_x as f32 + 2.5) * TILE_SIZE;
        rig.x = x;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }

    /// Runs one tick with `commands` for `PLAYER` and returns each command's outcome.
    fn tick_with(
        world: &mut WorldState,
        commands: Vec<PlayerCommand>,
    ) -> Vec<PlayerScopedCommandOutcome> {
        let tick = world.simulation_tick();
        let inputs = commands
            .into_iter()
            .fold(TickInputs::default(), |inputs, command| {
                inputs.act(PLAYER, command)
            });
        let output = world.simulate_tick(tick, &inputs);
        output
            .action_outcomes
            .iter()
            .map(|outcome| outcome.outcome)
            .collect()
    }

    #[test]
    fn service_actions_resolve_inside_the_fixed_tick() {
        let mut world = world_with_credits(500);
        stand_at(&mut world, SurfaceZone::Fuel);
        world.player_mut(PLAYER).expect("player").fuel = 10.0;
        let tick = world.simulation_tick();
        let outcomes = tick_with(
            &mut world,
            vec![PlayerCommand::Service(SessionServiceRequest::Refuel {
                menu_item: 2,
            })],
        );
        assert_eq!(outcomes, [PlayerScopedCommandOutcome::Applied]);
        let player = world.player(PLAYER).expect("player");
        assert!(player.fuel > 10.0);
        assert!(player.credits < 500);
        assert_eq!(world.simulation_tick(), SimulationTick::new(tick.get() + 1));
    }

    #[test]
    fn service_actions_away_from_their_building_are_rejected_without_effect() {
        let mut world = world_with_credits(500);
        world.player_mut(PLAYER).expect("player").fuel = 10.0;
        world
            .player_mut(PLAYER)
            .expect("player")
            .cargo
            .insert(MineralKind::Gold, 2);
        // At the repair garage, not the fuel station or the depot.
        stand_at(&mut world, SurfaceZone::Repair);
        let outcomes = tick_with(
            &mut world,
            vec![
                PlayerCommand::Service(SessionServiceRequest::Refuel { menu_item: 2 }),
                PlayerCommand::Refuel,
                PlayerCommand::SellCargo,
                PlayerCommand::Service(SessionServiceRequest::SellCargo),
                PlayerCommand::BuyUpgrade { index: 0 },
                PlayerCommand::Service(SessionServiceRequest::ClaimFreeTestCharge),
            ],
        );
        assert_eq!(
            outcomes,
            vec![PlayerScopedCommandOutcome::IgnoredUnavailable; 6],
            "every building-bound action is refused away from its building"
        );
        let player = world.player(PLAYER).expect("player");
        assert!(player.fuel < 10.5, "no refuel: {}", player.fuel);
        assert_eq!(player.credits, 500);
        assert_eq!(player.cargo.get(&MineralKind::Gold), Some(&2));
        assert_eq!(player.bombs, 0);
    }

    #[test]
    fn deep_rigs_cannot_use_surface_services() {
        let mut world = world_with_credits(500);
        stand_at(&mut world, SurfaceZone::Explosives);
        world.player_mut(PLAYER).expect("player").y = 40.0 * TILE_SIZE;
        assert_eq!(world.player_surface_zone(PLAYER), None);
        let outcomes = tick_with(
            &mut world,
            vec![PlayerCommand::Service(
                SessionServiceRequest::ClaimFreeTestCharge,
            )],
        );
        assert_eq!(outcomes, [PlayerScopedCommandOutcome::IgnoredUnavailable]);
    }

    #[test]
    fn services_offered_by_several_buildings_work_at_each_of_them() {
        for zone in [SurfaceZone::Headquarters, SurfaceZone::Bank] {
            let mut world = world_with_credits(0);
            stand_at(&mut world, zone);
            assert_eq!(world.player_surface_zone(PLAYER), Some(zone));
            assert!(world.command_allowed_here(
                PLAYER,
                &PlayerCommand::Service(SessionServiceRequest::Finance)
            ));
        }
        let mut world = world_with_credits(0);
        stand_at(&mut world, SurfaceZone::Shop);
        assert!(!world.command_allowed_here(
            PLAYER,
            &PlayerCommand::Service(SessionServiceRequest::Finance)
        ));
    }

    #[test]
    fn tools_and_rescue_are_not_bound_to_buildings() {
        let mut world = world_with_credits(0);
        world.player_mut(PLAYER).expect("player").y = 40.0 * TILE_SIZE;
        for command in [
            PlayerCommand::PlaceBomb,
            PlayerCommand::UseScanner,
            PlayerCommand::PlaceInfrastructure { slot: 0 },
            PlayerCommand::Rescue,
        ] {
            assert!(world.command_allowed_here(PLAYER, &command), "{command:?}");
        }
    }

    #[test]
    fn every_service_request_names_at_least_one_building() {
        for request in [
            SessionServiceRequest::Refuel { menu_item: 0 },
            SessionServiceRequest::Repair { menu_item: 0 },
            SessionServiceRequest::SellCargo,
            SessionServiceRequest::BuyUpgrade { index: 0 },
            SessionServiceRequest::BuyBombBundle { count: 3, cost: 55 },
            SessionServiceRequest::BuyMiningRockets,
            SessionServiceRequest::ClaimFreeTestCharge,
            SessionServiceRequest::SalvagePatchHull,
            SessionServiceRequest::Finance,
            SessionServiceRequest::BuyInsurance,
            SessionServiceRequest::SalvageRecoverLostCargo,
            SessionServiceRequest::SalvageLaunchDrone,
            SessionServiceRequest::SalvageRecoverWreckedPart,
            SessionServiceRequest::SalvageClearCollapseZones,
            SessionServiceRequest::SalvageSellScrapTip,
            SessionServiceRequest::SellScanData,
            SessionServiceRequest::AutoSortLowGradeCargo,
            SessionServiceRequest::CompleteDepotWork,
            SessionServiceRequest::StartSideContract,
            SessionServiceRequest::AcceptExpedition { index: 0 },
            SessionServiceRequest::AbandonExpedition { index: 0 },
        ] {
            assert!(!super::service_request_zones(request).is_empty());
        }
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
        let _output = world.simulate_tick(tick, &TickInputs::default());
        assert!(world.discovered_tile_count(PLAYER) > 0);
        assert!(
            world
                .discovered_tiles(PLAYER)
                .is_some_and(|tiles| tiles.contains(tile))
        );
    }
}
