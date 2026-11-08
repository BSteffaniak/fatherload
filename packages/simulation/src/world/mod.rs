//! The canonical [`WorldState`] and its fixed-tick transition, split by gameplay domain.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    contract::ContractLog,
    economy::{DeepClaimStatus, TownDevelopment},
    hazard::{FallingBoulder, HazardCloud, PlacedBomb, ScanMarker},
    ids::{PlayerId, SimulationTick},
    infrastructure::PlacedInfrastructure,
    player::Player,
    progression::{Expedition, SideContract, WorldEventKind},
    rig::{DrillState, RigPartKind},
    rng::DeterministicRngState,
    service::PlayerServiceTransaction,
    summary::{AuthoritativeWorldSummary, PlayerInventorySummary, PlayerSurvivalSnapshot},
    terrain::{ArtifactKind, MineralKind, Terrain, TilePosition},
    tick::simulation_ticks_for_seconds,
};

mod commands;
mod discovery;
mod hazards;
mod import;
mod movement;
mod progression;
mod rescue;
mod simulate;
mod state_hash;
#[cfg(test)]
mod tests;

pub use commands::authoritative_mine_target;
pub use import::{
    NEW_WORLD_HEIGHT_TILES, NEW_WORLD_SEED, NEW_WORLD_SPAWN, NEW_WORLD_WIDTH_TILES, WorldImport,
};
pub use movement::{
    AUTHORITATIVE_AIR_DRAG_PER_TICK, AUTHORITATIVE_BOULDER_LIFE_TICKS,
    AUTHORITATIVE_BOULDER_SPAWN_CHANCE, AUTHORITATIVE_BOULDER_WARNING_TICKS,
    AUTHORITATIVE_CRASH_DAMAGE_SCALE, AUTHORITATIVE_FUEL_BURN_PER_SECOND, AUTHORITATIVE_GRAVITY,
    AUTHORITATIVE_GROUND_DRAG_PER_TICK, AUTHORITATIVE_HORIZONTAL_ACCELERATION,
    AUTHORITATIVE_HORIZONTAL_SPEED, AUTHORITATIVE_MAX_FALL_SPEED,
    AUTHORITATIVE_MAX_HORIZONTAL_SPEED, AUTHORITATIVE_MIN_PLAYER_Y, AUTHORITATIVE_PLAYER_RADIUS,
    AUTHORITATIVE_SAFE_LANDING_SPEED, AUTHORITATIVE_SKY_FLIGHT_HEIGHT_TILES,
    AUTHORITATIVE_THRUST_SPEED, AuthoritativeMovementStep,
};
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
struct ActiveDrillTimer {
    next_chip_tick: SimulationTick,
    period_ticks: u64,
}

const fn canonical_minerals() -> [MineralKind; 10] {
    [
        MineralKind::Copper,
        MineralKind::Iron,
        MineralKind::Silver,
        MineralKind::Gold,
        MineralKind::Emerald,
        MineralKind::Ruby,
        MineralKind::Diamond,
        MineralKind::Platinum,
        MineralKind::Uranium,
        MineralKind::Mythril,
    ]
}

const fn canonical_market_factor(salt: u32, day: u32) -> u32 {
    let base = 85 + salt.wrapping_mul(37).wrapping_add(11) % 41;
    if day % 5 == 1 { base + 20 } else { base }
}

const fn canonical_mineral_market_factor(salt: u32, day: u32, mineral: MineralKind) -> u32 {
    let base = canonical_market_factor(salt.wrapping_add(mineral.value().wrapping_mul(13)), day);
    match (day % 5, mineral) {
        (1, MineralKind::Gold | MineralKind::Silver | MineralKind::Platinum) => base + 16,
        (0, MineralKind::Copper | MineralKind::Iron) => base + 8,
        (4, MineralKind::Ruby | MineralKind::Emerald | MineralKind::Diamond) => base + 10,
        _ => base,
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
struct DeepRunState {
    trip_ticks: u64,
    instability: f32,
}

/// Cargo dropped by a rescued rig, recoverable by returning to the drop site.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LostCargoSite {
    pub x: f32,
    pub y: f32,
    pub minerals: BTreeMap<MineralKind, u32>,
    pub artifacts: BTreeMap<ArtifactKind, u32>,
}

impl LostCargoSite {
    #[must_use]
    pub fn count(&self) -> u32 {
        self.minerals
            .values()
            .chain(self.artifacts.values())
            .copied()
            .sum()
    }
}

/// Shared progression, economy, contract, and expedition state owned by the world.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CanonicalProgressionState {
    pub contracts: ContractLog,
    pub deep_claim_status: DeepClaimStatus,
    pub escape_deadline: Option<SimulationTick>,
    pub deep_reward_milestones: BTreeMap<PlayerId, i32>,
    pub town_development: TownDevelopment,
    pub active_side_contracts: Vec<SideContract>,
    pub expedition_offers: Vec<Expedition>,
    pub active_expeditions: Vec<Expedition>,
    pub legendary_blueprints: BTreeSet<crate::LegendaryBlueprint>,
    pub rig_part_inventory: BTreeSet<RigPartKind>,
    pub town_event_day: u32,
    pub market_salt: u32,
    pub market_history: Vec<u32>,
    pub mineral_market_history: BTreeMap<MineralKind, Vec<u32>>,
    pub deepest_tile_reached: i32,
    pub total_earnings: u32,
    pub expeditions_completed: u32,
    pub next_day_tick: SimulationTick,
}

/// Compatibility world wrapper used to introduce explicit player identity before the legacy
/// monolithic `GameState` is fully split into authoritative world state and local client state.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorldState {
    simulation_tick: SimulationTick,
    authoritative_summary: AuthoritativeWorldSummary,
    players: BTreeMap<PlayerId, Player>,
    equipped_rig_parts: BTreeMap<PlayerId, BTreeSet<RigPartKind>>,
    mechanic_levels: BTreeMap<PlayerId, u8>,
    world_event_severity: BTreeMap<WorldEventKind, u32>,
    world_event_days_remaining: BTreeMap<WorldEventKind, u32>,
    deep_run_states: BTreeMap<PlayerId, DeepRunState>,
    lost_cargo_sites: BTreeMap<PlayerId, LostCargoSite>,
    rescue_counts: BTreeMap<PlayerId, u32>,
    progression: CanonicalProgressionState,
    scan_markers: Vec<ScanMarker>,
    collapse_warnings: Vec<TilePosition>,
    hazards: Vec<HazardCloud>,
    falling_boulders: Vec<FallingBoulder>,
    boulder_warning_deadlines: Vec<SimulationTick>,
    boulder_expiry_ticks: Vec<SimulationTick>,
    terrain: Terrain,
    bombs: Vec<PlacedBomb>,
    infrastructure: Vec<PlacedInfrastructure>,
    active_drills: BTreeMap<PlayerId, DrillState>,
    active_drill_timers: BTreeMap<PlayerId, ActiveDrillTimer>,
    scanner_cooldowns: BTreeMap<PlayerId, f32>,
    scanner_cooldown_deadlines: BTreeMap<PlayerId, SimulationTick>,
    bomb_detonation_ticks: Vec<SimulationTick>,
    rng: DeterministicRngState,
    discovered_tiles: BTreeMap<PlayerId, BTreeSet<TilePosition>>,
    service_transactions: Vec<PlayerServiceTransaction>,
}

impl WorldState {
    #[must_use]
    pub const fn progression(&self) -> &CanonicalProgressionState {
        &self.progression
    }

    pub const fn progression_mut(&mut self) -> &mut CanonicalProgressionState {
        &mut self.progression
    }

    #[must_use]
    pub const fn lost_cargo_sites(&self) -> &BTreeMap<PlayerId, LostCargoSite> {
        &self.lost_cargo_sites
    }

    pub const fn lost_cargo_sites_mut(&mut self) -> &mut BTreeMap<PlayerId, LostCargoSite> {
        &mut self.lost_cargo_sites
    }

    pub const fn hazards_mut(&mut self) -> &mut Vec<HazardCloud> {
        &mut self.hazards
    }

    #[must_use]
    pub fn falling_boulders(&self) -> &[FallingBoulder] {
        &self.falling_boulders
    }

    #[must_use]
    pub const fn players(&self) -> &BTreeMap<PlayerId, Player> {
        &self.players
    }

    #[must_use]
    pub const fn active_drills(&self) -> &BTreeMap<PlayerId, DrillState> {
        &self.active_drills
    }

    #[must_use]
    pub const fn discovered_tiles_by_player(&self) -> &BTreeMap<PlayerId, BTreeSet<TilePosition>> {
        &self.discovered_tiles
    }

    #[must_use]
    pub fn has_scanner_and_discovery_state(&self, player_id: PlayerId) -> bool {
        self.scanner_cooldowns.contains_key(&player_id)
            && self.discovered_tiles.contains_key(&player_id)
    }

    pub const fn set_authoritative_summary(&mut self, summary: AuthoritativeWorldSummary) {
        self.authoritative_summary = summary;
    }

    #[must_use]
    pub const fn simulation_tick(&self) -> SimulationTick {
        self.simulation_tick
    }

    pub const fn set_simulation_tick(&mut self, tick: SimulationTick) {
        self.simulation_tick = tick;
        self.authoritative_summary.tick = tick;
    }

    #[must_use]
    pub const fn authoritative_summary(&self) -> &AuthoritativeWorldSummary {
        &self.authoritative_summary
    }

    #[must_use]
    pub const fn hazard_count(&self) -> usize {
        self.hazards.len()
    }

    #[must_use]
    pub const fn bomb_count(&self) -> usize {
        self.bombs.len()
    }

    #[must_use]
    pub const fn infrastructure_count(&self) -> usize {
        self.infrastructure.len()
    }

    #[must_use]
    pub fn hazards(&self) -> &[HazardCloud] {
        &self.hazards
    }

    #[must_use]
    pub const fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    pub const fn terrain_mut(&mut self) -> &mut Terrain {
        &mut self.terrain
    }

    #[must_use]
    pub fn bombs(&self) -> &[PlacedBomb] {
        &self.bombs
    }

    #[must_use]
    pub fn infrastructure(&self) -> &[PlacedInfrastructure] {
        &self.infrastructure
    }

    #[must_use]
    pub fn service_transactions(&self) -> &[PlayerServiceTransaction] {
        &self.service_transactions
    }

    #[must_use]
    pub fn active_drill(&self, player_id: PlayerId) -> Option<&DrillState> {
        self.active_drills.get(&player_id)
    }

    pub fn active_drill_mut(&mut self, player_id: PlayerId) -> Option<&mut DrillState> {
        self.active_drills.get_mut(&player_id)
    }

    pub fn set_active_drill(&mut self, player_id: PlayerId, drill: Option<DrillState>) {
        if let Some(drill) = drill {
            let period_ticks = simulation_ticks_for_seconds(drill.seconds_per_chip);
            let period_u16 = u16::try_from(period_ticks).unwrap_or(u16::MAX).max(1);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped drill presentation progress intentionally maps onto a bounded tick period"
            )]
            let elapsed_ticks =
                u64::from((drill.progress.clamp(0.0, 1.0) * f32::from(period_u16)).floor() as u16);
            self.active_drill_timers.insert(
                player_id,
                ActiveDrillTimer {
                    next_chip_tick: SimulationTick::new(
                        self.simulation_tick
                            .get()
                            .saturating_add(period_ticks.saturating_sub(elapsed_ticks).max(1)),
                    ),
                    period_ticks,
                },
            );
            self.active_drills.insert(player_id, drill);
        } else {
            self.active_drills.remove(&player_id);
            self.active_drill_timers.remove(&player_id);
        }
    }

    #[must_use]
    pub fn scanner_cooldown_seconds(&self, player_id: PlayerId) -> Option<f32> {
        self.scanner_cooldowns.get(&player_id).copied()
    }

    pub fn set_scanner_cooldown_seconds(&mut self, player_id: PlayerId, seconds: f32) {
        let seconds = seconds.max(0.0);
        self.scanner_cooldowns.insert(player_id, seconds);
        self.scanner_cooldown_deadlines.insert(
            player_id,
            SimulationTick::new(
                self.simulation_tick
                    .get()
                    .saturating_add(simulation_ticks_for_seconds(seconds)),
            ),
        );
    }

    #[must_use]
    pub fn discovered_tile_count(&self, player_id: PlayerId) -> usize {
        self.discovered_tiles
            .get(&player_id)
            .map_or(0, BTreeSet::len)
    }

    #[must_use]
    pub fn discovered_tiles_snapshot(&self, player_id: PlayerId) -> BTreeSet<TilePosition> {
        self.discovered_tiles
            .get(&player_id)
            .cloned()
            .unwrap_or_default()
    }

    #[must_use]
    pub const fn won_game(&self) -> bool {
        self.authoritative_summary.won_game
    }

    #[must_use]
    pub fn player(&self, player_id: PlayerId) -> Option<&Player> {
        self.players.get(&player_id)
    }

    pub fn player_ids(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.players.keys().copied()
    }

    pub fn insert_player(&mut self, player_id: PlayerId, player: Player) {
        self.players.insert(player_id, player);
        self.equipped_rig_parts.entry(player_id).or_default();
        self.mechanic_levels.entry(player_id).or_default();
        self.scanner_cooldowns.entry(player_id).or_insert(0.0);
        self.scanner_cooldown_deadlines
            .entry(player_id)
            .or_insert(self.simulation_tick);
        self.discovered_tiles.entry(player_id).or_default();
        self.authoritative_summary.player_count = self.players.len();
    }

    pub fn player_mut(&mut self, player_id: PlayerId) -> Option<&mut Player> {
        self.players.get_mut(&player_id)
    }

    pub fn set_player_movement_modifiers(
        &mut self,
        player_id: PlayerId,
        mechanic_level: u8,
        parts: impl IntoIterator<Item = RigPartKind>,
    ) {
        self.mechanic_levels.insert(player_id, mechanic_level);
        self.equipped_rig_parts
            .insert(player_id, parts.into_iter().collect());
    }

    #[must_use]
    pub fn player_inventory_summary(&self, player_id: PlayerId) -> Option<PlayerInventorySummary> {
        self.player(player_id)
            .map(PlayerInventorySummary::from_player)
    }

    #[must_use]
    pub fn player_survival_snapshot(&self, player_id: PlayerId) -> Option<PlayerSurvivalSnapshot> {
        self.player(player_id).map(|player| PlayerSurvivalSnapshot {
            player_id,
            fuel: player.fuel,
            hull: player.hull,
            disabled: player.hull <= 0.0,
            stranded: player.fuel <= 0.0,
        })
    }

    #[must_use]
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    #[must_use]
    pub fn active_drills_snapshot(&self) -> BTreeMap<PlayerId, DrillState> {
        self.active_drills.clone()
    }

    #[must_use]
    pub fn scanner_cooldowns_snapshot(&self) -> BTreeMap<PlayerId, f32> {
        self.scanner_cooldowns.clone()
    }

    pub fn restore_static_world_state(
        &mut self,
        terrain: Terrain,
        hazards: Vec<HazardCloud>,
        bombs: Vec<PlacedBomb>,
        infrastructure: Vec<PlacedInfrastructure>,
        active_drills: BTreeMap<PlayerId, DrillState>,
        scanner_cooldowns: BTreeMap<PlayerId, f32>,
    ) {
        self.terrain = terrain;
        self.hazards = hazards;
        self.bombs = bombs;
        self.infrastructure = infrastructure;
        self.active_drills = active_drills;
        self.scanner_cooldowns = scanner_cooldowns;
        self.authoritative_summary.terrain_width = self.terrain.width();
        self.authoritative_summary.terrain_height = self.terrain.height();
        self.authoritative_summary.hazard_count = self.hazards.len();
        self.authoritative_summary.bomb_count = self.bombs.len();
        self.authoritative_summary.infrastructure_count = self.infrastructure.len();
    }
}
