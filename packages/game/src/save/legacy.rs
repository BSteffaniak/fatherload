//! Readers for older save formats (versions 1-5), migrated to a canonical [`WorldState`].
//!
//! * v1: `{ version, game }` — the whole legacy single-player `GameState`.
//! * v2: `{ version, world: { session, world, game, roster... } }` — legacy game plus persisted
//!   per-player state and world objects.
//! * v3: `{ version, world: { canonical_world, shell: { game } } }` — the canonical world plus the
//!   legacy shell state.
//! * v4: `{ version, save: { world, play_seconds } }` — the current envelope, but the world still
//!   carried a derived `authoritative_summary` (counts plus the `won_game` flag) instead of
//!   `won_game` itself.
//! * v5: the current envelope and world, but the terrain wrote out every tile ([`FullTerrain`])
//!   instead of its generation settings plus the tiles that differ from generation.
//!
//! Every pre-v6 terrain is read as a [`FullTerrain`] and converted; re-saving writes the compact
//! form.
//!
//! Only the fields the migration reads are declared; serde ignores the rest.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use drillgame_simulation::{
    ActiveWorldEvent, DrillState, Expedition, FallingBoulder, HazardCloud, PlacedBomb,
    PlacedInfrastructure, Player, PlayerId, RigPartKind, RigSlot, ScanMarker, SideContract,
    SideContractKind, SimulationTick, WorldImport, WorldState,
    contract::ContractLog,
    economy::{DeepClaimStatus, TownDevelopment},
    progression::LegendaryBlueprint,
    terrain::{ArtifactKind, FullTerrain, MineralKind, StrategicResourceKind, Terrain, TileKind},
};

use super::WorldSave;

/// The single-player rig id every legacy save used.
pub const LEGACY_PLAYER_ID: PlayerId = PlayerId::new(1);
/// Height every legacy world was extended to on load.
const LEGACY_WORLD_HEIGHT: i32 = 220;
/// The legacy `GameState::save_version` that needs no field migration.
const LEGACY_CURRENT_GAME_VERSION: u32 = 2;

const fn legacy_current_game_version() -> u32 {
    LEGACY_CURRENT_GAME_VERSION
}

/// Reads a pre-v6 terrain, which wrote out every tile.
fn full_terrain<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Terrain, D::Error> {
    FullTerrain::deserialize(deserializer).map(Terrain::from)
}

/// Rewrites a pre-v6 world's full-tile `terrain` in the current compact form, so the rest of
/// the world deserializes as a current [`WorldState`].
fn compact_terrain(world: &mut Value) -> Result<(), serde_json::Error> {
    if let Some(terrain) = world.get_mut("terrain") {
        let full: FullTerrain = serde_json::from_value(terrain.take())?;
        *terrain = serde_json::to_value(Terrain::from(full))?;
    }
    Ok(())
}

/// The parts of the legacy `GameState` that carry gameplay state.
#[derive(Clone, Debug, Deserialize)]
pub struct LegacyGame {
    #[serde(deserialize_with = "full_terrain")]
    terrain: Terrain,
    player: Player,
    #[serde(default = "legacy_current_game_version")]
    save_version: u32,
    #[serde(default = "ContractLog::new")]
    contracts: ContractLog,
    #[serde(default)]
    won_game: bool,
    #[serde(default)]
    deep_claim_status: DeepClaimStatus,
    #[serde(default)]
    town_development: TownDevelopment,
    #[serde(default)]
    escape_sequence_seconds: f32,
    #[serde(default)]
    deepest_tile_reached: i32,
    #[serde(default)]
    total_earnings: u32,
    #[serde(default)]
    expeditions_completed: u32,
    #[serde(default)]
    play_seconds: f32,
    #[serde(default)]
    legendary_blueprints: BTreeSet<LegendaryBlueprint>,
    #[serde(default)]
    rig_part_inventory: BTreeSet<RigPartKind>,
    #[serde(default)]
    equipped_rig_parts: BTreeMap<RigSlot, RigPartKind>,
    #[serde(default)]
    rescue_count: u32,
    #[serde(default)]
    trip_seconds: f32,
    #[serde(default)]
    deep_instability: f32,
    #[serde(default)]
    deep_reward_milestone: i32,
    #[serde(default)]
    lost_cargo_x: Option<f32>,
    #[serde(default)]
    lost_cargo_y: Option<f32>,
    #[serde(default)]
    lost_cargo_count: u32,
    #[serde(default)]
    lost_minerals: BTreeMap<MineralKind, u32>,
    #[serde(default)]
    lost_artifacts: BTreeMap<ArtifactKind, u32>,
    #[serde(default)]
    active_drill: Option<DrillState>,
    #[serde(default)]
    hazard_clouds: Vec<HazardCloud>,
    #[serde(default)]
    placed_bombs: Vec<PlacedBomb>,
    #[serde(default)]
    infrastructure: Vec<PlacedInfrastructure>,
    #[serde(default)]
    market_salt: u32,
    #[serde(default)]
    market_history: Vec<u32>,
    #[serde(default)]
    mineral_market_history: BTreeMap<MineralKind, Vec<u32>>,
    #[serde(default)]
    scanner_cooldown_seconds: f32,
    #[serde(default)]
    town_event_day: u32,
    #[serde(default)]
    active_world_events: Vec<ActiveWorldEvent>,
    #[serde(default)]
    scan_markers: Vec<ScanMarker>,
    #[serde(default)]
    collapse_warnings: Vec<drillgame_simulation::terrain::TilePosition>,
    #[serde(default)]
    side_contract_active: bool,
    #[serde(default)]
    side_contract_kind: SideContractKind,
    #[serde(default)]
    side_contract_target: Option<TileKind>,
    #[serde(default)]
    side_contract_required: u32,
    #[serde(default)]
    active_side_contracts: Vec<SideContract>,
    #[serde(default)]
    expedition_offers: Vec<Expedition>,
    #[serde(default)]
    active_expeditions: Vec<Expedition>,
    #[serde(default)]
    falling_boulders: Vec<FallingBoulder>,
}

const fn scanner_can_mark(kind: TileKind, scanner_level: u8) -> bool {
    match kind {
        TileKind::Ore(_) => scanner_level >= 1,
        TileKind::Gas
        | TileKind::Lava
        | TileKind::MagmaVent
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket => scanner_level >= 2,
        TileKind::Artifact(_) => scanner_level >= 3,
        _ => false,
    }
}

impl LegacyGame {
    /// The legacy load-time fixups (`migrate_after_load` + `migrate_loaded_state`).
    fn migrate(&mut self) {
        self.terrain.ensure_depth(LEGACY_WORLD_HEIGHT);
        self.contracts.migrate_after_load();
        if self.save_version < LEGACY_CURRENT_GAME_VERSION {
            let scanner_level = self.player.scanner_level;
            self.scan_markers
                .retain(|marker| scanner_can_mark(marker.kind, scanner_level));
            if self.side_contract_active && self.side_contract_required == 0 {
                self.side_contract_active = false;
            }
            if self.won_game {
                self.deep_claim_status = DeepClaimStatus::Unlocked;
            }
            self.save_version = LEGACY_CURRENT_GAME_VERSION;
        }
        if self.side_contract_active
            && self.active_side_contracts.is_empty()
            && let Some(target) = self.side_contract_target
        {
            self.active_side_contracts.push(SideContract {
                kind: self.side_contract_kind,
                target,
                required: self.side_contract_required.max(1),
                expires_day: None,
            });
        }
    }

    fn world_import(&self) -> WorldImport {
        let fresh = WorldImport::new_game(LEGACY_PLAYER_ID);
        WorldImport {
            player_id: LEGACY_PLAYER_ID,
            player: self.player.clone(),
            won_game: self.won_game,
            equipped_rig_parts: self.equipped_rig_parts.values().copied().collect(),
            active_world_events: self.active_world_events.clone(),
            trip_seconds: self.trip_seconds,
            deep_instability: self.deep_instability,
            lost_cargo_x: self.lost_cargo_x,
            lost_cargo_y: self.lost_cargo_y,
            lost_cargo_count: self.lost_cargo_count,
            lost_minerals: self.lost_minerals.clone(),
            lost_artifacts: self.lost_artifacts.clone(),
            rescue_count: self.rescue_count,
            contracts: self.contracts.clone(),
            deep_claim_status: self.deep_claim_status,
            escape_sequence_seconds: self.escape_sequence_seconds,
            deep_reward_milestone: self.deep_reward_milestone,
            town_development: self.town_development.clone(),
            active_side_contracts: self.active_side_contracts.clone(),
            expedition_offers: self.expedition_offers.clone(),
            active_expeditions: self.active_expeditions.clone(),
            legendary_blueprints: self.legendary_blueprints.clone(),
            rig_part_inventory: self.rig_part_inventory.clone(),
            town_event_day: self.town_event_day,
            market_salt: self.market_salt,
            market_history: if self.market_history.is_empty() {
                fresh.market_history
            } else {
                self.market_history.clone()
            },
            mineral_market_history: if self.mineral_market_history.is_empty() {
                fresh.mineral_market_history
            } else {
                self.mineral_market_history.clone()
            },
            deepest_tile_reached: self.deepest_tile_reached,
            total_earnings: self.total_earnings,
            expeditions_completed: self.expeditions_completed,
            scan_markers: self.scan_markers.clone(),
            collapse_warnings: self.collapse_warnings.clone(),
            hazard_clouds: self.hazard_clouds.clone(),
            falling_boulders: self.falling_boulders.clone(),
            terrain: self.terrain.clone(),
            placed_bombs: self.placed_bombs.clone(),
            infrastructure: self.infrastructure.clone(),
            active_drill: self.active_drill,
            scanner_cooldown_seconds: self.scanner_cooldown_seconds,
        }
    }
}

/// v1: the legacy game alone.
#[derive(Debug, Deserialize)]
pub struct V1SaveFile {
    game: LegacyGame,
}

impl V1SaveFile {
    pub fn migrate(self) -> WorldSave {
        let mut game = self.game;
        game.migrate();
        WorldSave {
            world: WorldState::from_import(&game.world_import()),
            play_seconds: game.play_seconds,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[allow(
    clippy::struct_field_names,
    reason = "field names mirror the persisted v2 JSON"
)]
struct PersistentPlayerState {
    player_id: PlayerId,
    x: f32,
    y: f32,
    velocity_x: f32,
    velocity_y: f32,
    credits: u32,
    #[serde(default)]
    cargo: BTreeMap<MineralKind, u32>,
    #[serde(default)]
    artifacts: BTreeMap<ArtifactKind, u32>,
    #[serde(default)]
    materials: BTreeMap<StrategicResourceKind, u32>,
    cargo_capacity: u32,
    fuel: f32,
    #[serde(default)]
    fuel_capacity: f32,
    hull: f32,
    #[serde(default)]
    fuel_tank_level: u8,
    #[serde(default)]
    cargo_bay_level: u8,
    #[serde(default)]
    drill_strength: u8,
    #[serde(default)]
    engine_level: u8,
    #[serde(default)]
    hull_level: u8,
    #[serde(default)]
    radiator_level: u8,
    #[serde(default)]
    scanner_level: u8,
    #[serde(default)]
    bombs: u32,
    #[serde(default)]
    loan_debt: u32,
    #[serde(default)]
    insured: bool,
    #[serde(default)]
    insurance_tier: u8,
    #[serde(default)]
    crafted_bulkheads: u8,
    #[serde(default)]
    crafted_sorters: u8,
    #[serde(default)]
    signal_relay_kits: u32,
    #[serde(default)]
    survey_drone_kits: u32,
    #[serde(default)]
    cargo_lift_kits: u32,
    #[serde(default)]
    tunnel_support_kits: u32,
    #[serde(default)]
    pump_station_kits: u32,
    #[serde(default)]
    ore_processor_kits: u32,
}

impl PersistentPlayerState {
    fn apply_to_player(&self, player: &mut Player) {
        player.x = self.x;
        player.y = self.y;
        player.velocity_x = self.velocity_x;
        player.velocity_y = self.velocity_y;
        player.credits = self.credits;
        player.cargo.clone_from(&self.cargo);
        player.artifacts.clone_from(&self.artifacts);
        player.materials.clone_from(&self.materials);
        player.cargo_capacity = self.cargo_capacity;
        player.fuel = self.fuel;
        if self.fuel_capacity > 0.0 {
            player.fuel_capacity = self.fuel_capacity;
        }
        player.hull = self.hull;
        for (target, value) in [
            (&mut player.fuel_tank_level, self.fuel_tank_level),
            (&mut player.cargo_bay_level, self.cargo_bay_level),
            (&mut player.drill_strength, self.drill_strength),
            (&mut player.engine_level, self.engine_level),
            (&mut player.hull_level, self.hull_level),
            (&mut player.radiator_level, self.radiator_level),
        ] {
            if value > 0 {
                *target = value;
            }
        }
        player.scanner_level = self.scanner_level;
        player.bombs = self.bombs;
        player.loan_debt = self.loan_debt;
        player.insured = self.insured;
        player.insurance_tier = self.insurance_tier;
        player.crafted_bulkheads = self.crafted_bulkheads;
        player.crafted_sorters = self.crafted_sorters;
        player.signal_relay_kits = self.signal_relay_kits;
        player.survey_drone_kits = self.survey_drone_kits;
        player.cargo_lift_kits = self.cargo_lift_kits;
        player.tunnel_support_kits = self.tunnel_support_kits;
        player.pump_station_kits = self.pump_station_kits;
        player.ore_processor_kits = self.ore_processor_kits;
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
struct PersistentSessionState {
    #[serde(default)]
    simulation_tick: SimulationTick,
    #[serde(default)]
    players: Vec<PersistentPlayerState>,
}

#[derive(Clone, Debug, Deserialize)]
struct PersistentWorldObjects {
    #[serde(deserialize_with = "full_terrain")]
    terrain: Terrain,
    #[serde(default)]
    hazards: Vec<HazardCloud>,
    #[serde(default)]
    bombs: Vec<PlacedBomb>,
    #[serde(default)]
    infrastructure: Vec<PlacedInfrastructure>,
    #[serde(default)]
    active_drills: BTreeMap<PlayerId, DrillState>,
    #[serde(default)]
    scanner_cooldowns: BTreeMap<PlayerId, f32>,
}

#[derive(Debug, Deserialize)]
struct V2WorldSave {
    #[serde(default)]
    session: PersistentSessionState,
    #[serde(default)]
    world: Option<PersistentWorldObjects>,
    game: LegacyGame,
}

/// v2: legacy game plus persisted session players and world objects.
#[derive(Debug, Deserialize)]
pub struct V2SaveFile {
    world: V2WorldSave,
}

impl V2SaveFile {
    pub fn migrate(self) -> WorldSave {
        let V2WorldSave {
            session,
            world: objects,
            game,
        } = self.world;
        let mut game = game;
        game.migrate();
        let mut world = WorldState::from_import(&game.world_import());
        if let Some(objects) = objects {
            world.restore_static_world_state(
                objects.terrain,
                objects.hazards,
                objects.bombs,
                objects.infrastructure,
                objects.active_drills,
                objects.scanner_cooldowns,
            );
        }
        world.set_simulation_tick(session.simulation_tick);
        for state in &session.players {
            if let Some(player) = world.player_mut(state.player_id) {
                state.apply_to_player(player);
            } else {
                let mut player = game.player.clone();
                state.apply_to_player(&mut player);
                world.insert_player(state.player_id, player);
            }
        }
        WorldSave {
            world,
            play_seconds: game.play_seconds,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct V3ShellGame {
    #[serde(default)]
    play_seconds: f32,
}

#[derive(Debug, Default, Deserialize)]
struct V3Shell {
    #[serde(default)]
    game: V3ShellGame,
}

#[derive(Debug, Deserialize)]
struct V3WorldSave {
    canonical_world: Value,
    #[serde(default)]
    shell: V3Shell,
}

/// v3: the canonical world plus the legacy shell.
#[derive(Debug, Deserialize)]
pub struct V3SaveFile {
    world: V3WorldSave,
}

impl V3SaveFile {
    pub fn migrate(self) -> Result<WorldSave, serde_json::Error> {
        Ok(WorldSave {
            world: world_with_summary(self.world.canonical_world)?,
            play_seconds: self.world.shell.game.play_seconds,
        })
    }
}

#[derive(Debug, Deserialize)]
struct V4Save {
    world: Value,
    #[serde(default)]
    play_seconds: f32,
}

/// v4: the current envelope around a world that still has the derived summary.
#[derive(Debug, Deserialize)]
pub struct V4SaveFile {
    save: V4Save,
}

impl V4SaveFile {
    pub fn migrate(self) -> Result<WorldSave, serde_json::Error> {
        Ok(WorldSave {
            world: world_with_summary(self.save.world)?,
            play_seconds: self.save.play_seconds,
        })
    }
}

/// The only part of the old derived world summary that was real state.
#[derive(Debug, Default, Deserialize)]
struct LegacySummary {
    #[serde(default)]
    won_game: bool,
}

/// v5: the current envelope and world around a full-tile terrain.
#[derive(Debug, Deserialize)]
pub struct V5SaveFile {
    save: V4Save,
}

impl V5SaveFile {
    pub fn migrate(self) -> Result<WorldSave, serde_json::Error> {
        let mut world = self.save.world;
        compact_terrain(&mut world)?;
        Ok(WorldSave {
            world: serde_json::from_value(world)?,
            play_seconds: self.save.play_seconds,
        })
    }
}

/// Reads a v3/v4 world: lifts `authoritative_summary.won_game` to `won_game` and drops the rest
/// of the summary (counts the world derives from its own collections).
fn world_with_summary(mut world: Value) -> Result<WorldState, serde_json::Error> {
    compact_terrain(&mut world)?;
    if let Some(object) = world.as_object_mut()
        && let Some(summary) = object.remove("authoritative_summary")
    {
        let summary: LegacySummary = serde_json::from_value(summary).unwrap_or_default();
        object
            .entry("won_game")
            .or_insert(Value::Bool(summary.won_game));
    }
    serde_json::from_value(world)
}
