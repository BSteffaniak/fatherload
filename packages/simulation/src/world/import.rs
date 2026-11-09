//! Construction of canonical worlds from seconds-based imports.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    contract::ContractLog,
    economy::{DeepClaimStatus, TownDevelopment},
    hazard::{FallingBoulder, HazardCloud, PlacedBomb, ScanMarker},
    ids::{PlayerId, SimulationTick},
    infrastructure::PlacedInfrastructure,
    player::Player,
    progression::{ActiveWorldEvent, Expedition, LegendaryBlueprint, SideContract},
    rig::{DrillState, RigPartKind},
    rng::DeterministicRngState,
    terrain::{ArtifactKind, MineralKind, TILE_SIZE, Terrain, TilePosition},
    tick::SimulationParams,
    worldgen::WorldSettings,
};

use super::{
    ActiveDrillTimer, CanonicalProgressionState, DeepRunState, LostCargoSite, TickTiming,
    WorldState, canonical_market_factor, canonical_mineral_market_factor, canonical_minerals,
    discovery::DiscoveredTiles, progression::MARKET_DAY_MILLIS,
};

/// Seconds-based description of a single-rig world used to construct a canonical [`WorldState`].
///
/// This is the import boundary for callers that still hold gameplay state outside the simulation
/// (for example the legacy desktop shell and old save formats). Timers are expressed in seconds
/// and converted to fixed simulation ticks on import.
#[allow(
    clippy::struct_excessive_bools,
    reason = "import mirrors the legacy shell's independent progression fields"
)]
#[derive(Clone, Debug)]
pub struct WorldImport {
    pub player_id: PlayerId,
    pub player: Player,
    pub won_game: bool,
    pub equipped_rig_parts: BTreeSet<RigPartKind>,
    pub active_world_events: Vec<ActiveWorldEvent>,
    pub trip_seconds: f32,
    pub deep_instability: f32,
    pub lost_cargo_x: Option<f32>,
    pub lost_cargo_y: Option<f32>,
    pub lost_cargo_count: u32,
    pub lost_minerals: BTreeMap<MineralKind, u32>,
    pub lost_artifacts: BTreeMap<ArtifactKind, u32>,
    pub rescue_count: u32,
    pub contracts: ContractLog,
    pub deep_claim_status: DeepClaimStatus,
    pub escape_sequence_seconds: f32,
    pub deep_reward_milestone: i32,
    pub town_development: TownDevelopment,
    pub active_side_contracts: Vec<SideContract>,
    pub expedition_offers: Vec<Expedition>,
    pub active_expeditions: Vec<Expedition>,
    pub legendary_blueprints: BTreeSet<LegendaryBlueprint>,
    pub rig_part_inventory: BTreeSet<RigPartKind>,
    pub town_event_day: u32,
    pub market_salt: u32,
    pub market_history: Vec<u32>,
    pub mineral_market_history: BTreeMap<MineralKind, Vec<u32>>,
    pub deepest_tile_reached: i32,
    pub total_earnings: u32,
    pub expeditions_completed: u32,
    pub scan_markers: Vec<ScanMarker>,
    pub collapse_warnings: Vec<TilePosition>,
    pub hazard_clouds: Vec<HazardCloud>,
    pub falling_boulders: Vec<FallingBoulder>,
    pub terrain: Terrain,
    pub placed_bombs: Vec<PlacedBomb>,
    pub infrastructure: Vec<PlacedInfrastructure>,
    pub active_drill: Option<DrillState>,
    pub scanner_cooldown_seconds: f32,
}

impl WorldState {
    /// Builds a canonical world from a seconds-based single-rig import at the default tick rate.
    #[must_use]
    pub fn from_import(import: &WorldImport) -> Self {
        Self::from_import_with_params(import, SimulationParams::default())
    }

    /// Builds a canonical world from a seconds-based single-rig import for a session's
    /// parameters. Every per-tick constant and tick deadline is derived from `params` here.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "import enumerates canonical fields explicitly"
    )]
    pub fn from_import_with_params(import: &WorldImport, params: SimulationParams) -> Self {
        let player_id = import.player_id;
        let timing = TickTiming::new(params);
        let ticks = |seconds: f32| timing.rate.ticks_for_seconds(seconds);
        Self {
            timing,
            player_template: Player::new(NEW_WORLD_SPAWN.0, NEW_WORLD_SPAWN.1),
            simulation_tick: SimulationTick::default(),
            won_game: import.won_game,
            players: BTreeMap::from([(player_id, import.player.clone())]),
            equipped_rig_parts: BTreeMap::from([(player_id, import.equipped_rig_parts.clone())]),
            mechanic_levels: BTreeMap::from([(player_id, import.town_development.mechanic_level)]),
            world_event_severity: import
                .active_world_events
                .iter()
                .map(|event| (event.kind, event.severity))
                .collect(),
            world_event_days_remaining: import
                .active_world_events
                .iter()
                .map(|event| (event.kind, event.days_remaining))
                .collect(),
            deep_run_states: BTreeMap::from([(
                player_id,
                DeepRunState {
                    trip_ticks: ticks(import.trip_seconds),
                    instability: import.deep_instability,
                },
            )]),
            lost_cargo_sites: match (
                import.lost_cargo_x,
                import.lost_cargo_y,
                import.lost_cargo_count,
            ) {
                (Some(x), Some(y), count) if count > 0 => BTreeMap::from([(
                    player_id,
                    LostCargoSite {
                        x,
                        y,
                        minerals: if import.lost_minerals.is_empty()
                            && import.lost_artifacts.is_empty()
                        {
                            BTreeMap::from([(MineralKind::Iron, count)])
                        } else {
                            import.lost_minerals.clone()
                        },
                        artifacts: import.lost_artifacts.clone(),
                    },
                )]),
                _ => BTreeMap::new(),
            },
            rescue_counts: BTreeMap::from([(player_id, import.rescue_count)]),
            progression: CanonicalProgressionState {
                contracts: import.contracts.clone(),
                deep_claim_status: import.deep_claim_status,
                escape_deadline: (import.escape_sequence_seconds > 0.0)
                    .then(|| SimulationTick::new(ticks(import.escape_sequence_seconds))),
                deep_reward_milestones: BTreeMap::from([(player_id, import.deep_reward_milestone)]),
                town_development: import.town_development.clone(),
                active_side_contracts: import.active_side_contracts.clone(),
                expedition_offers: import.expedition_offers.clone(),
                active_expeditions: import.active_expeditions.clone(),
                legendary_blueprints: import.legendary_blueprints.clone(),
                rig_part_inventory: import.rig_part_inventory.clone(),
                town_event_day: import.town_event_day,
                market_salt: import.market_salt,
                market_history: import.market_history.clone(),
                mineral_market_history: import.mineral_market_history.clone(),
                deepest_tile_reached: import.deepest_tile_reached,
                total_earnings: import.total_earnings,
                expeditions_completed: import.expeditions_completed,
                next_day_tick: SimulationTick::new(timing.ticks_for_millis(MARKET_DAY_MILLIS)),
            },
            scan_markers: import.scan_markers.clone(),
            collapse_warnings: import.collapse_warnings.clone(),
            hazards: import.hazard_clouds.clone(),
            falling_boulders: import.falling_boulders.clone(),
            boulder_warning_deadlines: import
                .falling_boulders
                .iter()
                .map(|boulder| SimulationTick::new(ticks(boulder.warning_seconds)))
                .collect(),
            boulder_expiry_ticks: import
                .falling_boulders
                .iter()
                .map(|boulder| SimulationTick::new(ticks(boulder.life)))
                .collect(),
            terrain: import.terrain.clone(),
            bombs: import.placed_bombs.clone(),
            infrastructure: import.infrastructure.clone(),
            active_drills: import
                .active_drill
                .map(|drill| BTreeMap::from([(player_id, drill)]))
                .unwrap_or_default(),
            active_drill_timers: import
                .active_drill
                .map(|drill| {
                    let period_ticks = ticks(drill.seconds_per_chip);
                    BTreeMap::from([(
                        player_id,
                        ActiveDrillTimer {
                            next_chip_tick: SimulationTick::new(period_ticks),
                            period_ticks,
                        },
                    )])
                })
                .unwrap_or_default(),
            scanner_cooldowns: BTreeMap::from([(player_id, import.scanner_cooldown_seconds)]),
            scanner_cooldown_deadlines: BTreeMap::from([(
                player_id,
                SimulationTick::new(ticks(import.scanner_cooldown_seconds)),
            )]),
            bomb_detonation_ticks: import
                .placed_bombs
                .iter()
                .map(|bomb| SimulationTick::new(ticks(bomb.timer_seconds)))
                .collect(),
            rng: DeterministicRngState::new(import.terrain.seed()),
            discovered_tiles: BTreeMap::from([(player_id, DiscoveredTiles::default())]),
            service_transactions: Vec::new(),
        }
    }
}

/// Width in tiles of a freshly generated single-player world.
pub const NEW_WORLD_WIDTH_TILES: i32 = 240;
/// Height in tiles of a freshly generated single-player world.
pub const NEW_WORLD_HEIGHT_TILES: i32 = 220;
/// Seed of the default world ([`WorldState::new_game`]; tests, fixtures and scripted scenes).
pub const NEW_WORLD_SEED: u64 = crate::worldgen::DEFAULT_WORLD_SEED;
/// Rig spawn position (world pixels) of a freshly generated single-player world.
pub const NEW_WORLD_SPAWN: (f32, f32) = (97.0 * TILE_SIZE, 4.0 * TILE_SIZE);

impl WorldImport {
    /// Describes a brand-new single-rig world from the default settings (fixed seed).
    #[must_use]
    pub fn new_game(player_id: PlayerId) -> Self {
        Self::new_game_with_settings(player_id, &WorldSettings::default())
    }

    /// Describes a brand-new single-rig world generated from `settings`.
    #[must_use]
    pub fn new_game_with_settings(player_id: PlayerId, settings: &WorldSettings) -> Self {
        let terrain = if *settings == WorldSettings::default() {
            default_terrain()
        } else {
            Terrain::generate(settings)
        };
        Self {
            player_id,
            player: Player::new(NEW_WORLD_SPAWN.0, NEW_WORLD_SPAWN.1),
            won_game: false,
            equipped_rig_parts: BTreeSet::new(),
            active_world_events: Vec::new(),
            trip_seconds: 0.0,
            deep_instability: 0.0,
            lost_cargo_x: None,
            lost_cargo_y: None,
            lost_cargo_count: 0,
            lost_minerals: BTreeMap::new(),
            lost_artifacts: BTreeMap::new(),
            rescue_count: 0,
            contracts: ContractLog::new(),
            deep_claim_status: DeepClaimStatus::Locked,
            escape_sequence_seconds: 0.0,
            deep_reward_milestone: 0,
            town_development: TownDevelopment::default(),
            active_side_contracts: Vec::new(),
            expedition_offers: Vec::new(),
            active_expeditions: Vec::new(),
            legendary_blueprints: BTreeSet::new(),
            rig_part_inventory: BTreeSet::from([
                RigPartKind::TitanDrill,
                RigPartKind::NeedleDrill,
                RigPartKind::ResonanceDrill,
                RigPartKind::LightweightEngine,
                RigPartKind::HaulerEngine,
                RigPartKind::BurstEngine,
                RigPartKind::ThermalHull,
                RigPartKind::ImpactHull,
                RigPartKind::PressureHull,
                RigPartKind::ProspectorScanner,
                RigPartKind::HazardScanner,
                RigPartKind::RelicScanner,
                RigPartKind::CargoBalloon,
                RigPartKind::ArmoredCargoBay,
                RigPartKind::SortedCargoRack,
            ]),
            town_event_day: 0,
            market_salt: 0,
            market_history: vec![canonical_market_factor(0, 0)],
            mineral_market_history: canonical_minerals()
                .into_iter()
                .map(|mineral| {
                    (
                        mineral,
                        vec![canonical_mineral_market_factor(0, 0, mineral)],
                    )
                })
                .collect(),
            deepest_tile_reached: 0,
            total_earnings: 0,
            expeditions_completed: 0,
            scan_markers: Vec::new(),
            collapse_warnings: Vec::new(),
            hazard_clouds: Vec::new(),
            falling_boulders: Vec::new(),
            terrain,
            placed_bombs: Vec::new(),
            infrastructure: Vec::new(),
            active_drill: None,
            scanner_cooldown_seconds: 0.0,
        }
    }
}

impl WorldState {
    /// Builds a brand-new single-rig world (see [`WorldImport::new_game`]).
    #[must_use]
    pub fn new_game(player_id: PlayerId) -> Self {
        Self::from_import(&WorldImport::new_game(player_id))
    }

    /// Builds a brand-new single-rig world for a session's parameters.
    #[must_use]
    pub fn new_game_with_params(player_id: PlayerId, params: SimulationParams) -> Self {
        Self::from_import_with_params(&WorldImport::new_game(player_id), params)
    }

    /// Builds a brand-new single-rig world generated from `settings` for a session's
    /// parameters.
    #[must_use]
    pub fn new_game_with_settings(
        player_id: PlayerId,
        settings: &WorldSettings,
        params: SimulationParams,
    ) -> Self {
        Self::from_import_with_params(
            &WorldImport::new_game_with_settings(player_id, settings),
            params,
        )
    }

    /// How this world was generated (seed, size, richness, hazards, generator version).
    #[must_use]
    pub const fn world_settings(&self) -> &WorldSettings {
        self.terrain.settings()
    }

    /// The generator's index of notable places in this world (empty for legacy worlds).
    #[must_use]
    pub fn world_features(&self) -> &crate::worldgen::FeatureIndex {
        self.terrain.features()
    }
}

/// The default world's terrain, generated once per process (generation is a pure function of
/// the settings, and tests and the title screen build the default world many times).
fn default_terrain() -> Terrain {
    static DEFAULT: std::sync::OnceLock<Terrain> = std::sync::OnceLock::new();
    DEFAULT
        .get_or_init(|| Terrain::generate(&WorldSettings::default()))
        .clone()
}
