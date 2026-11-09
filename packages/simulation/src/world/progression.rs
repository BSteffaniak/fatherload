//! Canonical economy, market days, contracts, expeditions, and town services.

use crate::{
    economy::DeepClaimStatus,
    event::{GameplayEventKind, TickOutput},
    hazard::HazardCloud,
    ids::{PlayerId, SimulationTick},
    infrastructure::{InfrastructureKind, PlacedInfrastructure},
    player::Player,
    progression::{Expedition, SideContract, WorldEventKind},
    rig::RigPartKind,
    service::{PlayerServiceTransaction, PlayerTransactionKind},
    summary::PlayerScopedCommandOutcome,
    terrain::{
        ArtifactKind, MineralKind, StrategicResourceKind, TILE_SIZE, TileKind, TilePosition,
    },
};

use super::{
    WorldState, canonical_market_factor, canonical_mineral_market_factor, canonical_minerals,
    record_service_transaction,
};

/// Length of one market day.
pub(super) const MARKET_DAY_MILLIS: u64 = 5 * 60 * 1_000;
/// Scanner lockout caused by an ancient machine event.
const ANCIENT_MACHINE_SCANNER_LOCKOUT_MILLIS: u64 = 6_000;

impl WorldState {
    pub(super) fn schedule_canonical_world_event(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if !self.progression.town_event_day.is_multiple_of(3) {
            return;
        }
        let kind = match (self.progression.market_salt + self.progression.town_event_day) % 13 {
            0 => WorldEventKind::MarketCrash,
            1 => WorldEventKind::MarketBoom,
            2 => WorldEventKind::RareBuyer,
            3 => WorldEventKind::FuelShortage,
            4 => WorldEventKind::RepairBacklog,
            5 => WorldEventKind::HeatWave,
            6 => WorldEventKind::CollapseSurge,
            7 => WorldEventKind::DeepPressureStorm,
            8 => WorldEventKind::GasBloom,
            9 => WorldEventKind::Earthquake,
            10 => WorldEventKind::MeteorShower,
            11 => WorldEventKind::RivalClaims,
            _ => WorldEventKind::AncientMachine,
        };
        if self.world_event_severity.contains_key(&kind) {
            return;
        }
        let severity = 1 + (self.progression.town_event_day + self.progression.market_salt) % 3;
        self.world_event_severity.insert(kind, severity);
        self.world_event_days_remaining
            .insert(kind, 2 + severity.min(2));
        match kind {
            WorldEventKind::GasBloom => {
                if let Some(player) = self.players.values().next() {
                    self.hazards.push(HazardCloud {
                        x: player.x,
                        y: player.y + TILE_SIZE,
                        life: 8.0 + f32::from(u16::try_from(severity).unwrap_or(u16::MAX)),
                        radius: 12.0 + f32::from(u16::try_from(severity).unwrap_or(u16::MAX)) * 2.0,
                    });
                    output.push_event(GameplayEventKind::HazardChanged);
                }
            }
            WorldEventKind::MeteorShower => {
                let _outcome = self.start_canonical_side_contract();
                self.progression.contracts.active.reward =
                    self.progression.contracts.active.reward.saturating_add(75);
            }
            WorldEventKind::RivalClaims => {
                self.progression.expedition_offers.clear();
                self.refresh_canonical_expedition_offers();
            }
            WorldEventKind::AncientMachine => {
                for (player_id, player) in &mut self.players {
                    player.add_material(StrategicResourceKind::CoreShard, severity.max(1));
                    self.scanner_cooldown_deadlines.insert(
                        *player_id,
                        SimulationTick::new(
                            next_tick.get().saturating_add(
                                self.timing
                                    .ticks_for_millis(ANCIENT_MACHINE_SCANNER_LOCKOUT_MILLIS),
                            ),
                        ),
                    );
                }
            }
            WorldEventKind::Earthquake => {
                if let Some(player) = self.players.values().next() {
                    let x = player.x;
                    let y = player.y;
                    self.spawn_cave_in(x, y);
                }
                output.push_event(GameplayEventKind::BouldersChanged);
            }
            WorldEventKind::MarketCrash
            | WorldEventKind::MarketBoom
            | WorldEventKind::RareBuyer
            | WorldEventKind::FuelShortage
            | WorldEventKind::RepairBacklog
            | WorldEventKind::HeatWave
            | WorldEventKind::CollapseSurge
            | WorldEventKind::DeepPressureStorm => {}
        }
    }

    pub(super) fn advance_canonical_day_if_due(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if next_tick < self.progression.next_day_tick {
            return;
        }
        self.advance_canonical_market_day();
        for days in self.world_event_days_remaining.values_mut() {
            *days = days.saturating_sub(1);
        }
        self.world_event_days_remaining.retain(|_, days| *days > 0);
        self.world_event_severity
            .retain(|kind, _| self.world_event_days_remaining.contains_key(kind));
        self.schedule_canonical_world_event(next_tick, output);
        let dividend = u32::from(self.progression.town_development.bank_level) * 12;
        if dividend > 0 {
            for player in self.players.values_mut() {
                player.credits = player.credits.saturating_add(dividend);
            }
            self.progression.total_earnings = self.progression.total_earnings.saturating_add(
                dividend.saturating_mul(u32::try_from(self.players.len()).unwrap_or(u32::MAX)),
            );
        }
        if self.progression.town_event_day % 5 == 3 {
            for item in &mut self.infrastructure {
                if item.kind == InfrastructureKind::PumpStation {
                    item.durability = item.durability.saturating_sub(35);
                }
            }
            self.infrastructure.retain(|item| item.durability > 0);
        }
        self.progression.next_day_tick = SimulationTick::new(
            self.progression
                .next_day_tick
                .get()
                .saturating_add(self.timing.ticks_for_millis(MARKET_DAY_MILLIS)),
        );
    }

    pub(super) fn canonical_mineral_market_value(&self, mineral: MineralKind) -> u32 {
        let mut factor = canonical_mineral_market_factor(
            self.progression.market_salt,
            self.progression.town_event_day,
            mineral,
        ) + u32::from(self.progression.town_development.depot_level) * 3;
        if self
            .world_event_severity
            .contains_key(&WorldEventKind::MarketBoom)
        {
            factor += 25;
        }
        if self
            .world_event_severity
            .contains_key(&WorldEventKind::RareBuyer)
            && matches!(
                mineral,
                MineralKind::Diamond | MineralKind::Mythril | MineralKind::Uranium
            )
        {
            factor += 45;
        }
        if self
            .world_event_severity
            .contains_key(&WorldEventKind::MarketCrash)
        {
            factor = factor.saturating_sub(25).max(45);
        }
        mineral.value().saturating_mul(factor) / 100
    }

    pub(super) fn sell_canonical_cargo(
        &mut self,
        player_id: PlayerId,
    ) -> PlayerScopedCommandOutcome {
        let Some(player_snapshot) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let mineral_payout = player_snapshot
            .cargo
            .iter()
            .map(|(mineral, count)| {
                self.canonical_mineral_market_value(*mineral)
                    .saturating_mul(*count)
            })
            .sum::<u32>();
        let artifact_payout = player_snapshot
            .artifacts
            .iter()
            .map(|(artifact, count)| artifact.value().saturating_mul(*count))
            .sum::<u32>();
        let payout = mineral_payout.saturating_add(artifact_payout);
        let payout = payout.saturating_add(
            payout.saturating_mul(
                self.quest_rebate_percent(player_id, crate::quest::QuestGiver::Kade),
            ) / 100,
        );
        if payout == 0 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let credits_before = player.credits;
        let cargo_before = player.cargo_used();
        player.cargo.clear();
        player.artifacts.clear();
        player.credits = player.credits.saturating_add(payout);
        self.progression.total_earnings = self.progression.total_earnings.saturating_add(payout);
        record_service_transaction(
            &mut self.service_transactions,
            PlayerServiceTransaction {
                player_id,
                kind: PlayerTransactionKind::SellCargo,
                credits_before,
                credits_after: player.credits,
                cargo_before,
                cargo_after: 0,
            },
        );
        PlayerScopedCommandOutcome::Applied
    }

    pub(super) fn advance_canonical_market_day(&mut self) {
        self.progression.town_event_day = self.progression.town_event_day.saturating_add(1);
        self.progression.market_salt = self.progression.market_salt.wrapping_add(1);
        self.progression
            .market_history
            .push(canonical_market_factor(
                self.progression.market_salt,
                self.progression.town_event_day,
            ));
        if self.progression.market_history.len() > 7 {
            self.progression.market_history.remove(0);
        }
        for mineral in canonical_minerals() {
            let history = self
                .progression
                .mineral_market_history
                .entry(mineral)
                .or_default();
            history.push(canonical_mineral_market_factor(
                self.progression.market_salt,
                self.progression.town_event_day,
                mineral,
            ));
            if history.len() > 7 {
                history.remove(0);
            }
        }
        self.progression.expedition_offers.clear();
    }

    pub(super) fn refresh_canonical_expedition_offers(&mut self) {
        use crate::ExpeditionObjectiveKind;
        if self.progression.deep_claim_status != DeepClaimStatus::Unlocked
            || !self.progression.expedition_offers.is_empty()
        {
            return;
        }
        let day = self.progression.town_event_day;
        let depth = u32::try_from(self.progression.deepest_tile_reached.max(80)).unwrap_or(80);
        self.progression.expedition_offers = vec![
            Expedition {
                kind: ExpeditionObjectiveKind::ReachDepth,
                target: TileKind::Stone,
                required: depth.saturating_add(15).min(140),
                reward: 320 + day * 12,
                expires_day: day + 4,
            },
            Expedition {
                kind: ExpeditionObjectiveKind::DeliverCargo,
                target: TileKind::Ore(MineralKind::Platinum),
                required: 2,
                reward: 420 + day * 14,
                expires_day: day + 3,
            },
            Expedition {
                kind: ExpeditionObjectiveKind::ScanHazards,
                target: TileKind::Gas,
                required: 4,
                reward: 360 + day * 10,
                expires_day: day + 3,
            },
            Expedition {
                kind: ExpeditionObjectiveKind::BuildPumpStations,
                target: TileKind::MagmaVent,
                required: 2,
                reward: 460 + day * 12,
                expires_day: day + 5,
            },
        ];
        let kind = match day % 10 {
            0 => ExpeditionObjectiveKind::RecoverProbe,
            1 => ExpeditionObjectiveKind::MineVein,
            2 => ExpeditionObjectiveKind::ScanAnomaly,
            3 => ExpeditionObjectiveKind::RescueMiner,
            4 => ExpeditionObjectiveKind::DeliverExplosives,
            5 => ExpeditionObjectiveKind::StabilizeCollapse,
            6 => ExpeditionObjectiveKind::RetrieveArtifact,
            7 => ExpeditionObjectiveKind::ReachSignal,
            8 => ExpeditionObjectiveKind::NoDamageReturn,
            _ => ExpeditionObjectiveKind::FastReturn,
        };
        self.progression.expedition_offers.push(Expedition {
            kind,
            target: TileKind::Artifact(ArtifactKind::OldCircuit),
            required: 1 + day % 3,
            reward: 520 + day * 16,
            expires_day: day + 2,
        });
    }

    pub(crate) fn accept_canonical_expedition(
        &mut self,
        index: usize,
    ) -> PlayerScopedCommandOutcome {
        self.refresh_canonical_expedition_offers();
        if self.progression.active_expeditions.len() >= 3
            || index >= self.progression.expedition_offers.len()
        {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let expedition = self.progression.expedition_offers.remove(index);
        self.progression.active_expeditions.push(expedition);
        PlayerScopedCommandOutcome::Applied
    }

    pub(super) fn apply_canonical_expedition_special_reward(
        &mut self,
        player_id: PlayerId,
        expedition: Expedition,
    ) {
        use crate::{ExpeditionObjectiveKind, LegendaryBlueprint};
        match expedition.kind {
            ExpeditionObjectiveKind::RetrieveArtifact => {
                self.progression
                    .legendary_blueprints
                    .insert(LegendaryBlueprint::StarPlating);
                self.progression
                    .rig_part_inventory
                    .insert(RigPartKind::ResonanceDrill);
            }
            ExpeditionObjectiveKind::RecoverProbe | ExpeditionObjectiveKind::ReachSignal => {
                self.progression
                    .rig_part_inventory
                    .insert(RigPartKind::HazardScanner);
            }
            _ => {}
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        match expedition.kind {
            ExpeditionObjectiveKind::ScanAnomaly | ExpeditionObjectiveKind::ScanHazards => {
                player.add_material(StrategicResourceKind::CrystalLens, expedition.required);
            }
            ExpeditionObjectiveKind::MineVein => {
                player.add_material(StrategicResourceKind::AncientAlloy, 1);
            }
            _ => {}
        }
    }

    pub(crate) fn start_canonical_side_contract(&mut self) -> PlayerScopedCommandOutcome {
        use crate::SideContractKind;
        if self.progression.active_side_contracts.len() >= 3 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let kind = match self.progression.town_event_day % 4 {
            0 => SideContractKind::Cargo,
            1 => SideContractKind::DepthSurvey,
            2 => SideContractKind::HazardScan,
            _ => SideContractKind::Rush,
        };
        let (target, required) = match kind {
            SideContractKind::Cargo => (TileKind::Ore(MineralKind::Gold), 2),
            SideContractKind::DepthSurvey => (TileKind::Ore(MineralKind::Platinum), 65),
            SideContractKind::HazardScan => (TileKind::Gas, 3),
            SideContractKind::Rush => (TileKind::Ore(MineralKind::Ruby), 1),
        };
        self.progression.active_side_contracts.push(SideContract {
            kind,
            target,
            required,
            expires_day: (kind == SideContractKind::Rush)
                .then_some(self.progression.town_event_day.saturating_add(2)),
        });
        PlayerScopedCommandOutcome::Applied
    }

    pub(super) fn canonical_side_contract_satisfied(
        &self,
        player_id: PlayerId,
        contract: SideContract,
        player: &Player,
    ) -> bool {
        use crate::SideContractKind;
        if contract
            .expires_day
            .is_some_and(|deadline| self.progression.town_event_day > deadline)
        {
            return false;
        }
        match contract.kind {
            SideContractKind::Cargo | SideContractKind::Rush => match contract.target {
                TileKind::Ore(mineral) => {
                    player.cargo.get(&mineral).copied().unwrap_or(0) >= contract.required
                }
                TileKind::Artifact(artifact) => {
                    player.artifacts.get(&artifact).copied().unwrap_or(0) >= contract.required
                }
                _ => false,
            },
            SideContractKind::DepthSurvey => {
                u32::try_from(self.progression.deepest_tile_reached).unwrap_or(0)
                    >= contract.required
            }
            SideContractKind::HazardScan => {
                let count = self
                    .discovered_tiles
                    .get(&player_id)
                    .into_iter()
                    .flat_map(super::DiscoveredTiles::iter)
                    .filter(|position| {
                        self.terrain.tile(*position).is_some_and(|tile| {
                            matches!(
                                tile.kind,
                                TileKind::Gas
                                    | TileKind::Lava
                                    | TileKind::MagmaVent
                                    | TileKind::ExplosivePocket
                                    | TileKind::PressurePocket
                            )
                        })
                    })
                    .count();
                count >= usize::try_from(contract.required).unwrap_or(usize::MAX)
            }
        }
    }

    pub(crate) fn complete_canonical_side_contract(
        &mut self,
        player_id: PlayerId,
    ) -> PlayerScopedCommandOutcome {
        let Some(player_snapshot) = self.players.get(&player_id).cloned() else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        self.progression.active_side_contracts.retain(|contract| {
            contract
                .expires_day
                .is_none_or(|deadline| self.progression.town_event_day <= deadline)
        });
        let Some(index) = self
            .progression
            .active_side_contracts
            .iter()
            .position(|contract| {
                self.canonical_side_contract_satisfied(player_id, *contract, &player_snapshot)
            })
        else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let contract = self.progression.active_side_contracts.remove(index);
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let credits_before = player.credits;
        let cargo_before = player.cargo_used();
        if matches!(
            contract.kind,
            crate::SideContractKind::Cargo | crate::SideContractKind::Rush
        ) {
            crate::progression::consume_side_contract_cargo(contract, player);
        }
        let reward = 420_u32.saturating_add(contract.required.min(10).saturating_mul(80));
        player.credits = player.credits.saturating_add(reward);
        self.progression.total_earnings = self.progression.total_earnings.saturating_add(reward);
        record_service_transaction(
            &mut self.service_transactions,
            PlayerServiceTransaction {
                player_id,
                kind: PlayerTransactionKind::CompleteContract,
                credits_before,
                credits_after: player.credits,
                cargo_before,
                cargo_after: player.cargo_used(),
            },
        );
        PlayerScopedCommandOutcome::Applied
    }

    pub(crate) fn apply_canonical_salvage_service(
        &mut self,
        player_id: PlayerId,
        request: crate::SessionServiceRequest,
    ) -> Option<PlayerScopedCommandOutcome> {
        let low_grade_values = [
            self.canonical_mineral_market_value(MineralKind::Copper),
            self.canonical_mineral_market_value(MineralKind::Iron),
        ];
        let player = self.players.get_mut(&player_id)?;
        let level = self.progression.town_development.salvage_yard_level;
        let applied = match request {
            crate::SessionServiceRequest::SalvageRecoverWreckedPart => {
                if level < 3 {
                    false
                } else {
                    let count = self.rescue_counts.get(&player_id).copied().unwrap_or(0);
                    let part = match count % 4 {
                        0 => RigPartKind::ImpactHull,
                        1 => RigPartKind::ArmoredCargoBay,
                        2 => RigPartKind::HazardScanner,
                        _ => RigPartKind::NeedleDrill,
                    };
                    self.progression.rig_part_inventory.insert(part);
                    true
                }
            }
            crate::SessionServiceRequest::SalvageClearCollapseZones => {
                if level < 4 || self.collapse_warnings.is_empty() {
                    false
                } else {
                    let cleared = u32::try_from(self.collapse_warnings.len()).unwrap_or(u32::MAX);
                    self.collapse_warnings.clear();
                    let payout = cleared.saturating_mul(45);
                    player.credits = player.credits.saturating_add(payout);
                    self.progression.total_earnings =
                        self.progression.total_earnings.saturating_add(payout);
                    true
                }
            }
            crate::SessionServiceRequest::SalvageSellScrapTip => {
                player.credits = player.credits.saturating_add(35);
                self.progression.total_earnings =
                    self.progression.total_earnings.saturating_add(35);
                true
            }
            crate::SessionServiceRequest::SellScanData => {
                if self.progression.town_development.scanner_lab_level == 0
                    || self.scan_markers.is_empty()
                {
                    false
                } else {
                    let count = u32::try_from(self.scan_markers.len()).unwrap_or(u32::MAX);
                    let payout = count.saturating_mul(
                        12 + u32::from(self.progression.town_development.scanner_lab_level) * 4,
                    );
                    self.scan_markers.clear();
                    player.credits = player.credits.saturating_add(payout);
                    self.progression.total_earnings =
                        self.progression.total_earnings.saturating_add(payout);
                    true
                }
            }
            crate::SessionServiceRequest::AutoSortLowGradeCargo => {
                let mut payout = 0_u32;
                for (index, mineral) in [MineralKind::Copper, MineralKind::Iron]
                    .into_iter()
                    .enumerate()
                {
                    if let Some(count) = player.cargo.remove(&mineral) {
                        payout =
                            payout.saturating_add(low_grade_values[index].saturating_mul(count));
                    }
                }
                if payout == 0 {
                    false
                } else {
                    player.credits = player.credits.saturating_add(payout);
                    self.progression.total_earnings =
                        self.progression.total_earnings.saturating_add(payout);
                    true
                }
            }
            _ => return None,
        };
        Some(if applied {
            PlayerScopedCommandOutcome::Applied
        } else {
            PlayerScopedCommandOutcome::IgnoredUnavailable
        })
    }

    #[allow(
        clippy::too_many_lines,
        reason = "canonical progression service migration keeps transactional rules together"
    )]
    pub(crate) fn apply_canonical_progression_service(
        &mut self,
        player_id: PlayerId,
        request: crate::SessionServiceRequest,
    ) -> Option<PlayerScopedCommandOutcome> {
        let player = self.players.get_mut(&player_id)?;
        let credits_before = player.credits;
        let cargo_before = player.cargo_used();
        let applied = match request {
            crate::SessionServiceRequest::Finance => {
                if player.loan_debt == 0 {
                    let level = self.progression.town_development.bank_level;
                    let advance = 250 + u32::from(level) * 150;
                    let risk_premium = 50 + u32::from(level) * 25;
                    player.credits = player.credits.saturating_add(advance);
                    player.loan_debt = advance.saturating_add(risk_premium);
                } else {
                    let payment = player.loan_debt.min(player.credits);
                    player.credits -= payment;
                    player.loan_debt -= payment;
                }
                true
            }
            crate::SessionServiceRequest::BuyInsurance => {
                if player.insured {
                    false
                } else {
                    let bank_level = self.progression.town_development.bank_level;
                    let max_tier = 1_u8.saturating_add(bank_level).min(4);
                    let next_tier = player.insurance_tier.saturating_add(1).min(max_tier);
                    let cost = (70 + u32::from(next_tier) * 55)
                        .saturating_sub(u32::from(bank_level).saturating_mul(15));
                    if player.insurance_tier >= max_tier || player.credits < cost {
                        false
                    } else {
                        player.credits -= cost;
                        player.insured = true;
                        player.insurance_tier = next_tier;
                        true
                    }
                }
            }
            crate::SessionServiceRequest::CraftRecipe { recipe } => {
                if recipe.cost().iter().any(|(material, required)| {
                    player.materials.get(material).copied().unwrap_or(0) < *required
                }) {
                    false
                } else {
                    for (material, required) in recipe.cost() {
                        if let Some(count) = player.materials.get_mut(material) {
                            *count = count.saturating_sub(*required);
                        }
                    }
                    player.materials.retain(|_, count| *count > 0);
                    match recipe {
                        crate::RecipeKind::ReinforcedBulkhead => {
                            player.crafted_bulkheads = player.crafted_bulkheads.saturating_add(1);
                            player.hull = player.max_hull();
                        }
                        crate::RecipeKind::AuxiliaryTank => {
                            player.fuel_capacity += 20.0;
                            player.fuel = player.fuel_capacity;
                        }
                        crate::RecipeKind::ExpandedSorter => {
                            player.crafted_sorters = player.crafted_sorters.saturating_add(1);
                            player.cargo_capacity = player.cargo_capacity.saturating_add(6);
                        }
                        crate::RecipeKind::SignalRelayKit => {
                            player.signal_relay_kits = player.signal_relay_kits.saturating_add(1);
                        }
                        crate::RecipeKind::CargoLiftKit => {
                            player.cargo_lift_kits = player.cargo_lift_kits.saturating_add(1);
                        }
                        crate::RecipeKind::TunnelSupportKit => {
                            player.tunnel_support_kits =
                                player.tunnel_support_kits.saturating_add(1);
                        }
                        crate::RecipeKind::PumpStationKit => {
                            player.pump_station_kits = player.pump_station_kits.saturating_add(1);
                        }
                        crate::RecipeKind::SurveyDroneKit => {
                            player.survey_drone_kits = player.survey_drone_kits.saturating_add(1);
                        }
                        crate::RecipeKind::OreProcessorKit => {
                            player.ore_processor_kits = player.ore_processor_kits.saturating_add(1);
                        }
                    }
                    true
                }
            }
            crate::SessionServiceRequest::SalvageRecoverLostCargo
            | crate::SessionServiceRequest::SalvageLaunchDrone => {
                let launch_drone =
                    matches!(request, crate::SessionServiceRequest::SalvageLaunchDrone);
                if launch_drone && self.progression.town_development.salvage_yard_level < 2 {
                    false
                } else if let Some(site) = self.lost_cargo_sites.remove(&player_id) {
                    let recovered = site.count();
                    let discount =
                        u32::from(self.progression.town_development.salvage_yard_level) * 3;
                    let fee = recovered
                        .saturating_mul(12)
                        .saturating_sub(recovered.saturating_mul(discount))
                        .min(player.credits);
                    player.credits -= fee;
                    for (mineral, count) in &site.minerals {
                        *player.cargo.entry(*mineral).or_default() += *count;
                    }
                    for (artifact, count) in &site.artifacts {
                        *player.artifacts.entry(*artifact).or_default() += *count;
                    }
                    if launch_drone {
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "bounded salvage site coordinates intentionally map to terrain tiles"
                        )]
                        let position = TilePosition {
                            x: (site.x / TILE_SIZE).floor() as i32,
                            y: (site.y / TILE_SIZE).floor() as i32,
                        };
                        self.infrastructure.push(PlacedInfrastructure {
                            kind: InfrastructureKind::SurveyDrone,
                            position,
                            durability: crate::default_infrastructure_durability(),
                        });
                    }
                    true
                } else {
                    false
                }
            }
            crate::SessionServiceRequest::UpgradeTownBuilding { building } => {
                let cost = self.progression.town_development.upgrade_cost(building);
                let material_gate = self.progression.town_development.level(building) >= 1;
                let has_material = player
                    .materials
                    .get(&StrategicResourceKind::AncientAlloy)
                    .copied()
                    .unwrap_or(0)
                    > 0;
                if player.credits < cost || (material_gate && !has_material) {
                    false
                } else {
                    player.credits -= cost;
                    if material_gate {
                        if let Some(count) = player
                            .materials
                            .get_mut(&StrategicResourceKind::AncientAlloy)
                        {
                            *count = count.saturating_sub(1);
                        }
                        player.materials.retain(|_, count| *count > 0);
                    }
                    *self.progression.town_development.level_mut(building) += 1;
                    self.progression.town_development.reputation = self
                        .progression
                        .town_development
                        .reputation
                        .saturating_add(1);
                    true
                }
            }
            _ => return None,
        };
        if !applied {
            return Some(PlayerScopedCommandOutcome::IgnoredUnavailable);
        }
        record_service_transaction(
            &mut self.service_transactions,
            PlayerServiceTransaction {
                player_id,
                kind: PlayerTransactionKind::BuyUpgrade,
                credits_before,
                credits_after: player.credits,
                cargo_before,
                cargo_after: player.cargo_used(),
            },
        );
        Some(PlayerScopedCommandOutcome::Applied)
    }

    pub(super) fn canonical_expedition_satisfied(
        &self,
        expedition: Expedition,
        player: &Player,
    ) -> bool {
        use crate::ExpeditionObjectiveKind;
        let progress = match expedition.kind {
            ExpeditionObjectiveKind::ReachDepth => {
                u32::try_from(self.progression.deepest_tile_reached).unwrap_or(0)
            }
            ExpeditionObjectiveKind::DeliverCargo => match expedition.target {
                TileKind::Ore(mineral) => player.cargo.get(&mineral).copied().unwrap_or(0),
                TileKind::Artifact(artifact) => {
                    player.artifacts.get(&artifact).copied().unwrap_or(0)
                }
                _ => 0,
            },
            ExpeditionObjectiveKind::BuildPumpStations => u32::try_from(
                self.infrastructure
                    .iter()
                    .filter(|item| item.kind == InfrastructureKind::PumpStation)
                    .count(),
            )
            .unwrap_or(u32::MAX),
            ExpeditionObjectiveKind::MineVein => player.cargo_used(),
            ExpeditionObjectiveKind::DeliverExplosives => player.bombs,
            ExpeditionObjectiveKind::RetrieveArtifact => player.artifacts.values().copied().sum(),
            ExpeditionObjectiveKind::NoDamageReturn => u32::from(player.hull >= player.max_hull()),
            ExpeditionObjectiveKind::FastReturn => {
                u32::from(self.progression.town_event_day <= expedition.expires_day)
            }
            ExpeditionObjectiveKind::RecoverProbe
            | ExpeditionObjectiveKind::ScanAnomaly
            | ExpeditionObjectiveKind::RescueMiner
            | ExpeditionObjectiveKind::StabilizeCollapse
            | ExpeditionObjectiveKind::ReachSignal => u32::from(
                self.progression.deepest_tile_reached
                    >= i32::try_from(60 + expedition.required * 10).unwrap_or(i32::MAX),
            ),
            ExpeditionObjectiveKind::ScanHazards => 0,
        };
        progress >= expedition.required
    }

    pub(crate) fn complete_canonical_progression_for_player(
        &mut self,
        player_id: PlayerId,
    ) -> PlayerScopedCommandOutcome {
        let Some(player_snapshot) = self.players.get(&player_id).cloned() else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if let Some(index) = self
            .progression
            .active_expeditions
            .iter()
            .position(|expedition| {
                expedition.expires_day >= self.progression.town_event_day
                    && self.canonical_expedition_satisfied(*expedition, &player_snapshot)
            })
        {
            let expedition = self.progression.active_expeditions.remove(index);
            let (credits_before, cargo_before, credits_after, cargo_after) = {
                let Some(player) = self.players.get_mut(&player_id) else {
                    return PlayerScopedCommandOutcome::UnknownPlayer;
                };
                let credits_before = player.credits;
                let cargo_before = player.cargo_used();
                crate::progression::consume_expedition_delivery(expedition, player);
                player.credits = player.credits.saturating_add(expedition.reward);
                (
                    credits_before,
                    cargo_before,
                    player.credits,
                    player.cargo_used(),
                )
            };
            self.apply_canonical_expedition_special_reward(player_id, expedition);
            self.progression.total_earnings = self
                .progression
                .total_earnings
                .saturating_add(expedition.reward);
            self.progression.expeditions_completed =
                self.progression.expeditions_completed.saturating_add(1);
            self.progression.town_development.reputation = self
                .progression
                .town_development
                .reputation
                .saturating_add(1);
            record_service_transaction(
                &mut self.service_transactions,
                PlayerServiceTransaction {
                    player_id,
                    kind: PlayerTransactionKind::CompleteContract,
                    credits_before,
                    credits_after,
                    cargo_before,
                    cargo_after,
                },
            );
            return PlayerScopedCommandOutcome::Applied;
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let credits_before = player.credits;
        let cargo_before = player.cargo_used();
        let before_completed = self.progression.contracts.completed;
        let Some(completion) = self.progression.contracts.try_complete(player) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let escrow =
            completion.reward * u32::from(self.progression.town_development.bank_level) / 20;
        player.credits = player.credits.saturating_add(escrow);
        self.progression.total_earnings = self
            .progression
            .total_earnings
            .saturating_add(completion.reward.saturating_add(escrow));
        if completion.finished_story {
            self.progression.deep_claim_status = DeepClaimStatus::Unlocked;
            self.won_game = true;
        }
        debug_assert_eq!(self.progression.contracts.completed, before_completed + 1);
        record_service_transaction(
            &mut self.service_transactions,
            PlayerServiceTransaction {
                player_id,
                kind: if completion.finished_story {
                    PlayerTransactionKind::WinGame
                } else {
                    PlayerTransactionKind::CompleteContract
                },
                credits_before,
                credits_after: player.credits,
                cargo_before,
                cargo_after: player.cargo_used(),
            },
        );
        PlayerScopedCommandOutcome::Applied
    }
}
