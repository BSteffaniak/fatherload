#![allow(
    dead_code,
    reason = "persistent world save migration APIs are kept while production online save boundaries are wired"
)]

use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use crate::{
    game_state::{DrillState, GameState, HazardCloud, PlacedBomb, PlacedInfrastructure},
    multiplayer::{LOCAL_PLAYER_ID, PlayerId, SimulationTick},
    player::Player,
    session::WorldState,
    terrain::{ArtifactKind, MineralKind, StrategicResourceKind, Terrain},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionShellSave {
    pub game: GameState,
}

impl SessionShellSave {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        Self {
            game: game.clone_for_save(),
        }
    }

    #[must_use]
    pub fn restore_game(&self) -> GameState {
        let mut game = self.game.clone();
        game.migrate_after_load();
        game
    }
}

const SETTINGS_FILE_NAME: &str = "settings.json";

const SAVE_FILE_NAME: &str = "save.json";
const SAVE_SLOTS: usize = 3;
const SAVE_VERSION: u32 = 3;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct SettingsFile {
    pub master_volume: f32,
    pub fullscreen: bool,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            fullscreen: false,
        }
    }
}

pub fn save_settings(settings: SettingsFile) -> Result<(), SaveError> {
    let json = serde_json::to_string_pretty(&settings).map_err(SaveError::Serialize)?;
    write_state_file(settings_path(), json)
}

#[must_use]
pub fn load_settings() -> SettingsFile {
    let Ok(json) = fs::read_to_string(settings_path()) else {
        return SettingsFile::default();
    };
    serde_json::from_str(&json).unwrap_or_default()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SaveSlotMetadata {
    pub depth: i32,
    pub credits: u32,
    pub cargo_used: u32,
    pub cargo_capacity: u32,
    pub contracts_completed: u32,
    pub play_seconds: f32,
    pub total_earnings: u32,
    pub mode: String,
    pub deep_claim_unlocked: bool,
    pub modified_unix_seconds: Option<u64>,
    pub won_game: bool,
}

impl SaveSlotMetadata {
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "save slot depth is displayed as an integral tile depth"
    )]
    pub fn from_persistent_world(
        save: &PersistentWorldSave,
        modified_unix_seconds: Option<u64>,
    ) -> Self {
        let default_player = save.default_player();
        let shell = &save.shell.game;
        let shell_player = &shell.player;
        let depth_y = default_player.map_or(shell_player.y, |player| player.y);
        Self {
            depth: (depth_y / crate::game_state::TILE_SIZE).floor() as i32,
            credits: default_player.map_or(shell_player.credits, |player| player.credits),
            cargo_used: default_player.map_or_else(
                || shell_player.cargo_used(),
                crate::player::Player::cargo_used,
            ),
            cargo_capacity: default_player
                .map_or(shell_player.cargo_capacity, |player| player.cargo_capacity),
            contracts_completed: shell.contracts.completed,
            play_seconds: shell.play_seconds,
            total_earnings: shell.total_earnings,
            mode: save_mode_label(shell).to_owned(),
            deep_claim_unlocked: shell.deep_claim_status
                == crate::economy::DeepClaimStatus::Unlocked,
            modified_unix_seconds,
            won_game: shell.won_game,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct SaveFile {
    version: u32,
    world: PersistentWorldSave,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum SaveAuthority {
    #[default]
    LocalSinglePlayer,
    HostOwnedSession,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum SaveSessionKind {
    #[default]
    LocalOnly,
    HostOwned,
    DedicatedServer,
    CloudSession,
}

impl From<SaveAuthority> for SaveSessionKind {
    fn from(authority: SaveAuthority) -> Self {
        match authority {
            SaveAuthority::LocalSinglePlayer => Self::LocalOnly,
            SaveAuthority::HostOwnedSession => Self::HostOwned,
        }
    }
}

fn default_player_roster() -> Vec<PlayerId> {
    vec![LOCAL_PLAYER_ID]
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PersistentPlayerState {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub credits: u32,
    #[serde(default)]
    pub cargo: BTreeMap<MineralKind, u32>,
    #[serde(default)]
    pub artifacts: BTreeMap<ArtifactKind, u32>,
    #[serde(default)]
    pub materials: BTreeMap<StrategicResourceKind, u32>,
    pub cargo_used: u32,
    pub cargo_capacity: u32,
    pub fuel: f32,
    #[serde(default)]
    pub fuel_capacity: f32,
    pub hull: f32,
    #[serde(default)]
    pub fuel_tank_level: u8,
    #[serde(default)]
    pub cargo_bay_level: u8,
    #[serde(default)]
    pub drill_strength: u8,
    #[serde(default)]
    pub engine_level: u8,
    #[serde(default)]
    pub hull_level: u8,
    #[serde(default)]
    pub radiator_level: u8,
    #[serde(default)]
    pub scanner_level: u8,
    #[serde(default)]
    pub bombs: u32,
    #[serde(default)]
    pub loan_debt: u32,
    #[serde(default)]
    pub insured: bool,
    #[serde(default)]
    pub insurance_tier: u8,
    #[serde(default)]
    pub crafted_bulkheads: u8,
    #[serde(default)]
    pub crafted_sorters: u8,
    #[serde(default)]
    pub signal_relay_kits: u32,
    #[serde(default)]
    pub survey_drone_kits: u32,
    #[serde(default)]
    pub cargo_lift_kits: u32,
    #[serde(default)]
    pub tunnel_support_kits: u32,
    #[serde(default)]
    pub pump_station_kits: u32,
    #[serde(default)]
    pub ore_processor_kits: u32,
}

impl PersistentPlayerState {
    #[must_use]
    pub fn local_from_game(game: &GameState) -> Self {
        Self {
            player_id: LOCAL_PLAYER_ID,
            x: game.player.x,
            y: game.player.y,
            velocity_x: game.player.velocity_x,
            velocity_y: game.player.velocity_y,
            credits: game.player.credits,
            cargo: game.player.cargo.clone(),
            artifacts: game.player.artifacts.clone(),
            materials: game.player.materials.clone(),
            cargo_used: game.player.cargo_used(),
            cargo_capacity: game.player.cargo_capacity,
            fuel: game.player.fuel,
            fuel_capacity: game.player.fuel_capacity,
            hull: game.player.hull,
            fuel_tank_level: game.player.fuel_tank_level,
            cargo_bay_level: game.player.cargo_bay_level,
            drill_strength: game.player.drill_strength,
            engine_level: game.player.engine_level,
            hull_level: game.player.hull_level,
            radiator_level: game.player.radiator_level,
            scanner_level: game.player.scanner_level,
            bombs: game.player.bombs,
            loan_debt: game.player.loan_debt,
            insured: game.player.insured,
            insurance_tier: game.player.insurance_tier,
            crafted_bulkheads: game.player.crafted_bulkheads,
            crafted_sorters: game.player.crafted_sorters,
            signal_relay_kits: game.player.signal_relay_kits,
            survey_drone_kits: game.player.survey_drone_kits,
            cargo_lift_kits: game.player.cargo_lift_kits,
            tunnel_support_kits: game.player.tunnel_support_kits,
            pump_station_kits: game.player.pump_station_kits,
            ore_processor_kits: game.player.ore_processor_kits,
        }
    }

    #[must_use]
    pub fn from_world_player(player_id: PlayerId, world: &WorldState) -> Option<Self> {
        let player = world.player(player_id)?;
        Some(Self::from_player(player_id, player))
    }

    #[must_use]
    pub fn from_player(player_id: PlayerId, player: &Player) -> Self {
        Self {
            player_id,
            x: player.x,
            y: player.y,
            velocity_x: player.velocity_x,
            velocity_y: player.velocity_y,
            credits: player.credits,
            cargo: player.cargo.clone(),
            artifacts: player.artifacts.clone(),
            materials: player.materials.clone(),
            cargo_used: player.cargo_used(),
            cargo_capacity: player.cargo_capacity,
            fuel: player.fuel,
            fuel_capacity: player.fuel_capacity,
            hull: player.hull,
            fuel_tank_level: player.fuel_tank_level,
            cargo_bay_level: player.cargo_bay_level,
            drill_strength: player.drill_strength,
            engine_level: player.engine_level,
            hull_level: player.hull_level,
            radiator_level: player.radiator_level,
            scanner_level: player.scanner_level,
            bombs: player.bombs,
            loan_debt: player.loan_debt,
            insured: player.insured,
            insurance_tier: player.insurance_tier,
            crafted_bulkheads: player.crafted_bulkheads,
            crafted_sorters: player.crafted_sorters,
            signal_relay_kits: player.signal_relay_kits,
            survey_drone_kits: player.survey_drone_kits,
            cargo_lift_kits: player.cargo_lift_kits,
            tunnel_support_kits: player.tunnel_support_kits,
            pump_station_kits: player.pump_station_kits,
            ore_processor_kits: player.ore_processor_kits,
        }
    }

    pub fn apply_to_player(&self, player: &mut Player) {
        player.x = self.x;
        player.y = self.y;
        player.velocity_x = self.velocity_x;
        player.velocity_y = self.velocity_y;
        player.credits = self.credits;
        player.cargo = self.cargo.clone();
        player.artifacts = self.artifacts.clone();
        player.materials = self.materials.clone();
        player.cargo_capacity = self.cargo_capacity;
        player.fuel = self.fuel;
        if self.fuel_capacity > 0.0 {
            player.fuel_capacity = self.fuel_capacity;
        }
        player.hull = self.hull;
        if self.fuel_tank_level > 0 {
            player.fuel_tank_level = self.fuel_tank_level;
        }
        if self.cargo_bay_level > 0 {
            player.cargo_bay_level = self.cargo_bay_level;
        }
        if self.drill_strength > 0 {
            player.drill_strength = self.drill_strength;
        }
        if self.engine_level > 0 {
            player.engine_level = self.engine_level;
        }
        if self.hull_level > 0 {
            player.hull_level = self.hull_level;
        }
        if self.radiator_level > 0 {
            player.radiator_level = self.radiator_level;
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

const fn default_persistent_players() -> Vec<PersistentPlayerState> {
    Vec::new()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PersistentSessionState {
    pub simulation_tick: SimulationTick,
    pub players: Vec<PersistentPlayerState>,
}

impl PersistentSessionState {
    #[must_use]
    pub fn local_from_game(game: &GameState) -> Self {
        Self {
            simulation_tick: SimulationTick::default(),
            players: vec![PersistentPlayerState::local_from_game(game)],
        }
    }

    #[must_use]
    pub fn from_world(world: &WorldState) -> Self {
        Self {
            simulation_tick: world.simulation_tick(),
            players: world
                .player_ids()
                .filter_map(|player_id| PersistentPlayerState::from_world_player(player_id, world))
                .collect(),
        }
    }
}

impl Default for PersistentSessionState {
    fn default() -> Self {
        Self {
            simulation_tick: SimulationTick::default(),
            players: default_persistent_players(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PersistentWorldRestoreSummary {
    pub simulation_tick: SimulationTick,
    pub roster_players: usize,
    pub persistent_players: usize,
    pub default_player_present: bool,
}

impl PersistentWorldRestoreSummary {
    #[must_use]
    pub fn from_save(save: &PersistentWorldSave) -> Self {
        Self {
            simulation_tick: save.canonical_world.simulation_tick(),
            roster_players: save.player_roster.len(),
            persistent_players: save.restored_player_count(),
            default_player_present: save.player_roster.contains(&save.default_player_id),
        }
    }

    #[must_use]
    pub const fn roster_matches_persistent_players(&self) -> bool {
        self.roster_players == self.persistent_players
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PersistentWorldObjectCounts {
    pub terrain_width: i32,
    pub terrain_height: i32,
    pub hazards: usize,
    pub bombs: usize,
    pub infrastructure: usize,
    pub active_drills: usize,
    pub scanner_cooldowns: usize,
}

impl PersistentWorldObjectCounts {
    #[must_use]
    pub const fn has_authoritative_terrain(self) -> bool {
        self.terrain_width > 0 && self.terrain_height > 0
    }

    #[must_use]
    pub const fn has_session_objects(self) -> bool {
        self.hazards > 0
            || self.bombs > 0
            || self.infrastructure > 0
            || self.active_drills > 0
            || self.scanner_cooldowns > 0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PersistentWorldObjects {
    pub terrain: Terrain,
    #[serde(default)]
    pub hazards: Vec<HazardCloud>,
    #[serde(default)]
    pub bombs: Vec<PlacedBomb>,
    #[serde(default)]
    pub infrastructure: Vec<PlacedInfrastructure>,
    #[serde(default)]
    pub active_drills: BTreeMap<PlayerId, DrillState>,
    #[serde(default)]
    pub scanner_cooldowns: BTreeMap<PlayerId, f32>,
}

impl PersistentWorldObjects {
    #[must_use]
    pub fn from_default_game() -> Self {
        Self::from_legacy_game(&GameState::new())
    }

    #[must_use]
    pub fn from_legacy_game(game: &GameState) -> Self {
        let mut active_drills = BTreeMap::new();
        if let Some(drill) = game.active_drill {
            active_drills.insert(LOCAL_PLAYER_ID, drill);
        }
        Self {
            terrain: game.terrain.clone(),
            hazards: game.hazard_clouds.clone(),
            bombs: game.placed_bombs.clone(),
            infrastructure: game.infrastructure.clone(),
            active_drills,
            scanner_cooldowns: BTreeMap::from([(LOCAL_PLAYER_ID, game.scanner_cooldown_seconds)]),
        }
    }

    #[must_use]
    pub fn from_world(world: &WorldState) -> Self {
        Self {
            terrain: world.terrain().clone(),
            hazards: world.hazards().to_vec(),
            bombs: world.bombs().to_vec(),
            infrastructure: world.infrastructure().to_vec(),
            active_drills: world.active_drills_snapshot(),
            scanner_cooldowns: world.scanner_cooldowns_snapshot(),
        }
    }

    #[must_use]
    pub fn object_counts(&self) -> PersistentWorldObjectCounts {
        PersistentWorldObjectCounts {
            terrain_width: self.terrain.width(),
            terrain_height: self.terrain.height(),
            hazards: self.hazards.len(),
            bombs: self.bombs.len(),
            infrastructure: self.infrastructure.len(),
            active_drills: self.active_drills.len(),
            scanner_cooldowns: self.scanner_cooldowns.len(),
        }
    }

    pub fn restore_into_world(&self, world: &mut WorldState) {
        world.restore_static_world_state(
            self.terrain.clone(),
            self.hazards.clone(),
            self.bombs.clone(),
            self.infrastructure.clone(),
            self.active_drills.clone(),
            self.scanner_cooldowns.clone(),
        );
    }

    pub fn restore_into_game(&self, game: &mut GameState) {
        game.terrain = self.terrain.clone();
        game.hazard_clouds.clone_from(&self.hazards);
        game.placed_bombs.clone_from(&self.bombs);
        game.infrastructure.clone_from(&self.infrastructure);
        game.active_drill = self.active_drills.get(&LOCAL_PLAYER_ID).copied();
        game.scanner_cooldown_seconds = self
            .scanner_cooldowns
            .get(&LOCAL_PLAYER_ID)
            .copied()
            .unwrap_or(game.scanner_cooldown_seconds);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PersistentWorldSave {
    #[serde(default)]
    pub save_authority: SaveAuthority,
    #[serde(default)]
    pub session_kind: SaveSessionKind,
    #[serde(default = "default_player_roster")]
    pub player_roster: Vec<PlayerId>,
    pub default_player_id: PlayerId,
    pub canonical_world: WorldState,
    pub shell: SessionShellSave,
}

impl PersistentWorldSave {
    #[must_use]
    pub fn from_session(world: &WorldState, shell: &GameState) -> Self {
        let mut player_roster = world.player_ids().collect::<Vec<_>>();
        if player_roster.is_empty() {
            player_roster = default_player_roster();
        } else {
            player_roster.sort();
            player_roster.dedup();
        }
        Self {
            save_authority: if shell.online_host_owns_save {
                SaveAuthority::HostOwnedSession
            } else {
                SaveAuthority::LocalSinglePlayer
            },
            session_kind: if shell.online_host_owns_save {
                SaveSessionKind::HostOwned
            } else {
                SaveSessionKind::LocalOnly
            },
            default_player_id: player_roster.first().copied().unwrap_or(LOCAL_PLAYER_ID),
            player_roster,
            canonical_world: world.clone(),
            shell: SessionShellSave::from_game(shell),
        }
    }

    #[must_use]
    fn migrate_legacy(legacy: LegacyPersistentWorldSave) -> Self {
        let mut shell = legacy.game;
        shell.migrate_after_load();
        let mut canonical_world = WorldState::from_legacy_game(&shell);
        legacy.world.restore_into_world(&mut canonical_world);
        canonical_world.set_simulation_tick(legacy.session.simulation_tick);
        for player_state in &legacy.session.players {
            if let Some(player) = canonical_world.player_mut(player_state.player_id) {
                player_state.apply_to_player(player);
            } else {
                let mut player = shell.player.clone();
                player_state.apply_to_player(&mut player);
                canonical_world.insert_player(player_state.player_id, player);
            }
        }
        let mut player_roster = legacy.player_roster;
        if player_roster.is_empty() {
            player_roster.extend(canonical_world.player_ids());
        }
        player_roster.sort();
        player_roster.dedup();
        let default_player_id = if player_roster.contains(&legacy.default_player_id) {
            legacy.default_player_id
        } else {
            player_roster.first().copied().unwrap_or(LOCAL_PLAYER_ID)
        };
        Self {
            save_authority: legacy.save_authority,
            session_kind: legacy.session_kind,
            player_roster,
            default_player_id,
            canonical_world,
            shell: SessionShellSave::from_game(&shell),
        }
    }

    #[must_use]
    pub fn default_player(&self) -> Option<&Player> {
        self.canonical_world
            .player(self.default_player_id)
            .or_else(|| {
                self.canonical_world
                    .player_ids()
                    .next()
                    .and_then(|id| self.canonical_world.player(id))
            })
    }

    #[must_use]
    pub fn default_player_roster_contains_state(&self) -> bool {
        self.canonical_world
            .player(self.default_player_id)
            .is_some()
            && self.player_roster.contains(&self.default_player_id)
    }

    #[must_use]
    pub fn world_object_counts(&self) -> PersistentWorldObjectCounts {
        PersistentWorldObjects::from_world(&self.canonical_world).object_counts()
    }

    #[must_use]
    pub fn restore_shell_game(&self) -> GameState {
        self.shell.restore_game()
    }

    #[must_use]
    pub fn restored_player_count(&self) -> usize {
        self.canonical_world.player_ids().count()
    }

    #[must_use]
    pub fn restores_world_state_without_legacy_conversion(&self) -> bool {
        self.default_player_roster_contains_state() && self.restored_player_count() > 0
    }

    #[must_use]
    pub fn restore_world(&self) -> WorldState {
        self.canonical_world.clone()
    }

    #[must_use]
    pub fn restore_summary(&self) -> PersistentWorldRestoreSummary {
        PersistentWorldRestoreSummary::from_save(self)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LegacyPersistentWorldSave {
    #[serde(default)]
    save_authority: SaveAuthority,
    #[serde(default)]
    session_kind: SaveSessionKind,
    #[serde(default = "default_player_roster")]
    player_roster: Vec<PlayerId>,
    default_player_id: PlayerId,
    #[serde(default)]
    session: PersistentSessionState,
    #[serde(default = "PersistentWorldObjects::from_default_game")]
    world: PersistentWorldObjects,
    game: GameState,
}

#[derive(Debug, Deserialize, Serialize)]
struct LegacyWorldSaveFile {
    version: u32,
    world: LegacyPersistentWorldSave,
}

#[derive(Debug, Deserialize, Serialize)]
struct LegacySaveFile {
    version: u32,
    game: GameState,
}

pub fn save_legacy_shell_game(game: &GameState) -> Result<(), SaveError> {
    enforce_shell_local_save_authority(game)?;
    save_legacy_shell_game_to_path(game, save_path())
}

pub fn save_game_slot(game: &GameState, slot: usize) -> Result<(), SaveError> {
    enforce_shell_local_save_authority(game)?;
    save_legacy_shell_game_to_path(game, slot_path(slot))
}

fn enforce_shell_local_save_authority(game: &GameState) -> Result<(), SaveError> {
    if game.can_write_local_save() {
        Ok(())
    } else {
        Err(SaveError::SaveDenied(
            "Joined online clients cannot write local save files while the remote host owns the session save.".to_owned(),
        ))
    }
}

pub fn save_world_session(world: &WorldState, shell: &GameState) -> Result<(), SaveError> {
    save_world_session_to_path(world, shell, save_path())
}

pub fn save_world_session_slot(
    world: &WorldState,
    shell: &GameState,
    slot: usize,
) -> Result<(), SaveError> {
    save_world_session_to_path(world, shell, slot_path(slot))
}

pub fn save_world_session_to_path(
    world: &WorldState,
    shell: &GameState,
    path: impl AsRef<Path>,
) -> Result<(), SaveError> {
    let save = SaveFile {
        version: SAVE_VERSION,
        world: PersistentWorldSave::from_session(world, shell),
    };
    let json = serde_json::to_string_pretty(&save).map_err(SaveError::Serialize)?;
    write_state_file(path, json)
}

#[must_use]
pub fn persistent_world_from_session(world: &WorldState, shell: &GameState) -> PersistentWorldSave {
    PersistentWorldSave::from_session(world, shell)
}

pub fn load_persistent_world() -> Result<PersistentWorldSave, SaveError> {
    load_persistent_world_from_path(save_path())
}

pub fn load_persistent_world_slot(slot: usize) -> Result<PersistentWorldSave, SaveError> {
    load_persistent_world_from_path(slot_path(slot))
}

pub fn load_latest_persistent_world() -> Result<PersistentWorldSave, SaveError> {
    let Some(path) = latest_save_path() else {
        return Err(SaveError::NoSaveFound);
    };
    load_persistent_world_from_path(path)
}

pub fn load_persistent_world_from_path(
    path: impl AsRef<Path>,
) -> Result<PersistentWorldSave, SaveError> {
    let json = fs::read_to_string(path).map_err(SaveError::Io)?;
    if let Ok(save) = serde_json::from_str::<SaveFile>(&json) {
        validate_save_version(save.version)?;
        return Ok(save.world);
    }

    if let Ok(save) = serde_json::from_str::<LegacyWorldSaveFile>(&json) {
        validate_save_version(save.version)?;
        return Ok(PersistentWorldSave::migrate_legacy(save.world));
    }

    let legacy_save: LegacySaveFile = serde_json::from_str(&json).map_err(SaveError::Serialize)?;
    validate_save_version(legacy_save.version)?;
    Ok(PersistentWorldSave::migrate_legacy(
        LegacyPersistentWorldSave {
            save_authority: SaveAuthority::LocalSinglePlayer,
            session_kind: SaveSessionKind::LocalOnly,
            player_roster: default_player_roster(),
            default_player_id: LOCAL_PLAYER_ID,
            session: PersistentSessionState::local_from_game(&legacy_save.game),
            world: PersistentWorldObjects::from_legacy_game(&legacy_save.game),
            game: legacy_save.game,
        },
    ))
}

fn save_legacy_shell_game_to_path(
    game: &GameState,
    path: impl AsRef<Path>,
) -> Result<(), SaveError> {
    let world = WorldState::from_legacy_game(game);
    save_world_session_to_path(&world, game, path)
}

pub fn load_game_slot(slot: usize) -> Result<GameState, SaveError> {
    load_game_from_path(slot_path(slot))
}

pub fn load_game() -> Result<GameState, SaveError> {
    load_game_from_path(save_path())
}

pub fn load_latest_game() -> Result<GameState, SaveError> {
    let Some(path) = latest_save_path() else {
        return Err(SaveError::NoSaveFound);
    };
    load_game_from_path(path)
}

fn load_game_from_path(path: impl AsRef<Path>) -> Result<GameState, SaveError> {
    let save = load_persistent_world_from_path(path)?;
    Ok(save.restore_shell_game())
}

const fn validate_save_version(version: u32) -> Result<(), SaveError> {
    if version > SAVE_VERSION || version == 0 {
        return Err(SaveError::UnsupportedVersion(version));
    }
    Ok(())
}

const fn save_mode_label(game: &GameState) -> &'static str {
    if game.game_over {
        "rescue"
    } else if game.won_game {
        "deep claim"
    } else {
        match game.run_mode {
            crate::game_state::RunMode::Title => "title",
            crate::game_state::RunMode::Playing => "playing",
            crate::game_state::RunMode::Interior => "interior",
            crate::game_state::RunMode::Paused => "paused",
        }
    }
}

#[must_use]
pub fn save_slot_metadata(slot: usize) -> Option<SaveSlotMetadata> {
    save_metadata_from_path(slot_path(slot))
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "save slot depth is displayed as an integral tile depth"
)]
fn save_metadata_from_path(path: impl AsRef<Path>) -> Option<SaveSlotMetadata> {
    let path = path.as_ref();
    let modified_unix_seconds = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    let save = load_persistent_world_from_path(path).ok()?;
    Some(SaveSlotMetadata::from_persistent_world(
        &save,
        modified_unix_seconds,
    ))
}

#[must_use]
pub fn save_exists() -> bool {
    save_path().exists()
}

#[must_use]
pub fn saves_exist() -> bool {
    save_exists() || (0..SAVE_SLOTS).any(save_slot_exists)
}

#[must_use]
pub fn latest_save_summary() -> Option<SaveSlotMetadata> {
    latest_save_path().and_then(save_metadata_from_path)
}

#[must_use]
pub fn save_slot_exists(slot: usize) -> bool {
    slot_path(slot).exists()
}

#[must_use]
pub const fn save_slot_count() -> usize {
    SAVE_SLOTS
}

fn write_state_file(path: impl AsRef<Path>, contents: String) -> Result<(), SaveError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(SaveError::Io)?;
    }
    fs::write(path, contents).map_err(SaveError::Io)
}

pub fn online_host_descriptor_path() -> PathBuf {
    default_state_dir().join("drillgame-online-host.json")
}

pub fn online_join_descriptor_path() -> PathBuf {
    default_state_dir().join("drillgame-online-join.json")
}

fn settings_path() -> PathBuf {
    default_state_dir().join(SETTINGS_FILE_NAME)
}

fn save_path() -> PathBuf {
    default_state_dir().join(SAVE_FILE_NAME)
}

fn slot_path(slot: usize) -> PathBuf {
    default_state_dir().join(format!("save-slot-{}.json", slot + 1))
}

fn latest_save_path() -> Option<PathBuf> {
    std::iter::once(save_path())
        .chain((0..SAVE_SLOTS).map(slot_path))
        .filter_map(|path| {
            let modified = fs::metadata(&path).ok()?.modified().ok()?;
            Some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn default_state_dir() -> PathBuf {
    if let Ok(path) = env::var("DRILLGAME_STATE_DIR") {
        return PathBuf::from(path);
    }
    if let Ok(state_home) = env::var("XDG_STATE_HOME") {
        return PathBuf::from(state_home).join("drillgame");
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("drillgame");
    }
    env::temp_dir().join("drillgame")
}

#[derive(Debug)]
pub enum SaveError {
    Io(io::Error),
    Serialize(serde_json::Error),
    UnsupportedVersion(u32),
    NoSaveFound,
    SaveDenied(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Serialize(error) => write!(formatter, "serialization error: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported save version: {version}")
            }
            Self::NoSaveFound => formatter.write_str("no save file found"),
            Self::SaveDenied(reason) => write!(formatter, "save denied: {reason}"),
        }
    }
}

impl Error for SaveError {}

#[cfg(test)]
mod tests {
    use crate::{
        game_state::GameState,
        multiplayer::{LOCAL_PLAYER_ID, PlayerId, SimulationTick},
        save::{
            LegacyPersistentWorldSave, LegacySaveFile, LegacyWorldSaveFile, PersistentSessionState,
            PersistentWorldObjects, PersistentWorldSave, SaveAuthority, SaveFile, SaveSessionKind,
            load_persistent_world_from_path, save_game_slot,
        },
        session::WorldState,
        terrain::{ArtifactKind, MineralKind, StrategicResourceKind, TilePosition},
    };

    #[test]
    fn game_state_round_trips_through_versioned_json() {
        let game = GameState::new();
        let save = SaveFile {
            version: super::SAVE_VERSION,
            world: PersistentWorldSave::from_session(&WorldState::from_legacy_game(&game), &game),
        };
        let json = serde_json::to_string(&save).expect("serialize game");
        let loaded: SaveFile = serde_json::from_str(&json).expect("deserialize game");

        assert_eq!(loaded.version, super::SAVE_VERSION);
        assert_eq!(
            loaded.world.save_authority,
            SaveAuthority::LocalSinglePlayer
        );
        assert_eq!(loaded.world.session_kind, SaveSessionKind::LocalOnly);
        assert_eq!(loaded.world.default_player_id, LOCAL_PLAYER_ID);
        assert_eq!(loaded.world.player_roster, vec![LOCAL_PLAYER_ID]);
        assert_eq!(
            loaded.world.canonical_world.simulation_tick(),
            SimulationTick::default()
        );
        assert_eq!(
            loaded
                .world
                .canonical_world
                .player_ids()
                .next()
                .expect("saved player"),
            LOCAL_PLAYER_ID
        );
        assert_eq!(
            loaded.world.shell.game.player.cargo_capacity,
            game.player.cargo_capacity
        );
        assert_eq!(
            loaded.world.canonical_world.terrain().width(),
            game.terrain.width()
        );
    }

    #[test]
    fn legacy_save_shape_still_deserializes_for_migration() {
        let game = GameState::new();
        let legacy = LegacySaveFile {
            version: 2,
            game: game.clone_for_save(),
        };
        let json = serde_json::to_string(&legacy).expect("serialize legacy game");
        let path = std::env::temp_dir().join(format!(
            "drillgame-legacy-save-migration-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, json).expect("write legacy save");

        let loaded = load_persistent_world_from_path(&path).expect("migrate legacy save");
        let _ignored = std::fs::remove_file(path);

        assert_eq!(loaded.save_authority, SaveAuthority::LocalSinglePlayer);
        assert_eq!(loaded.session_kind, SaveSessionKind::LocalOnly);
        assert_eq!(loaded.default_player_id, LOCAL_PLAYER_ID);
        assert_eq!(
            loaded
                .default_player()
                .expect("migrated player")
                .cargo_capacity,
            game.player.cargo_capacity
        );
    }

    #[test]
    fn version_two_world_envelope_migrates_to_canonical_world() {
        let mut game = GameState::new();
        game.player.credits = 91;
        let mut world = WorldState::from_legacy_game(&game);
        world.set_simulation_tick(SimulationTick::new(27));
        world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("player exists")
            .credits = 314;
        let legacy = LegacyWorldSaveFile {
            version: 2,
            world: LegacyPersistentWorldSave {
                save_authority: SaveAuthority::HostOwnedSession,
                session_kind: SaveSessionKind::HostOwned,
                player_roster: vec![LOCAL_PLAYER_ID],
                default_player_id: LOCAL_PLAYER_ID,
                session: PersistentSessionState::from_world(&world),
                world: PersistentWorldObjects::from_world(&world),
                game: game.clone_for_save(),
            },
        };
        let path = std::env::temp_dir().join(format!(
            "drillgame-world-save-migration-{}.json",
            std::process::id()
        ));
        std::fs::write(
            &path,
            serde_json::to_string(&legacy).expect("serialize legacy world save"),
        )
        .expect("write legacy world save");

        let loaded = load_persistent_world_from_path(&path).expect("migrate world envelope");
        let _ignored = std::fs::remove_file(path);

        assert_eq!(
            loaded.canonical_world.simulation_tick(),
            SimulationTick::new(27)
        );
        assert_eq!(
            loaded.default_player().expect("migrated player").credits,
            314
        );
        assert_eq!(loaded.shell.game.player.credits, 91);
    }

    #[test]
    fn persistent_session_state_preserves_local_player_metadata() {
        let game = GameState::new();
        let session = super::PersistentSessionState::local_from_game(&game);

        assert_eq!(session.simulation_tick, SimulationTick::default());
        assert_eq!(session.players.len(), 1);
        assert_eq!(session.players[0].player_id, LOCAL_PLAYER_ID);
        assert_eq!(
            session.players[0].cargo_capacity,
            game.player.cargo_capacity
        );
    }

    #[test]
    fn persistent_world_restore_keeps_shell_and_canonical_world_separate() {
        let mut game = GameState::new();
        game.player.x = 5.0;
        game.player.credits = 10;
        let mut world = WorldState::from_legacy_game(&game);
        let player_id = LOCAL_PLAYER_ID;
        {
            let player = world.player_mut(player_id).expect("player exists");
            player.x = 123.0;
            player.credits = 456;
        }
        let target = TilePosition { x: 0, y: 1 };
        world
            .terrain_mut()
            .set_kind(target, crate::terrain::TileKind::Air);
        world.set_scanner_cooldown_seconds(player_id, 7.5);

        let save = PersistentWorldSave::from_session(&world, &game);
        let restored_shell = save.restore_shell_game();
        let restored_world = save.restore_world();
        let restored_player = restored_world.player(player_id).expect("player restored");

        assert!(save.restores_world_state_without_legacy_conversion());
        assert!((restored_shell.player.x - 5.0).abs() < f32::EPSILON);
        assert_eq!(restored_shell.player.credits, 10);
        assert!((restored_player.x - 123.0).abs() < f32::EPSILON);
        assert_eq!(restored_player.credits, 456);
        assert_eq!(
            restored_world.scanner_cooldown_seconds(player_id),
            Some(7.5)
        );
        assert_eq!(
            restored_world
                .terrain()
                .tile(target)
                .expect("tile exists")
                .kind,
            crate::terrain::TileKind::Air
        );
        assert_eq!(save.restored_player_count(), 1);
    }

    #[test]
    fn persistent_world_restore_into_game_restores_world_objects() {
        let mut game = GameState::new();
        game.hazard_clouds.push(crate::game_state::HazardCloud {
            x: 1.0,
            y: 2.0,
            life: 5.0,
            radius: 3.0,
        });
        game.scanner_cooldown_seconds = 2.5;
        let world_objects = super::PersistentWorldObjects::from_legacy_game(&game);
        let mut restored = GameState::new();

        world_objects.restore_into_game(&mut restored);

        assert_eq!(restored.hazard_clouds.len(), 1);
        assert!((restored.scanner_cooldown_seconds - 2.5).abs() < f32::EPSILON);
    }

    #[test]
    fn persistent_world_save_can_be_built_from_world_state() {
        let mut game = GameState::new();
        game.player.credits = 321;
        let mut world = WorldState::from_legacy_game(&game);
        world.set_simulation_tick(SimulationTick::new(42));
        world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .fuel = 12.0;

        game.online_host_owns_save = true;
        let save = PersistentWorldSave::from_session(&world, &game);
        assert_eq!(save.save_authority, SaveAuthority::HostOwnedSession);
        assert_eq!(save.session_kind, SaveSessionKind::HostOwned);
        assert_eq!(save.player_roster, vec![LOCAL_PLAYER_ID]);
        assert_eq!(
            save.canonical_world.simulation_tick(),
            SimulationTick::new(42)
        );
        assert_eq!(save.default_player().expect("saved player").credits, 321);
        assert!((save.default_player().expect("saved player").fuel - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn persistent_world_save_restores_authoritative_world_player_state() {
        let mut game = GameState::new();
        game.player.credits = 111;
        let mut world = WorldState::from_legacy_game(&game);
        world.set_simulation_tick(SimulationTick::new(13));
        {
            let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
            player.credits = 222;
            player.x = 123.0;
            player.y = 234.0;
            player.velocity_x = 5.0;
            player.velocity_y = -6.0;
            player.fuel = 33.0;
            player.hull = 44.0;
            player.cargo_capacity = 55;
            player.cargo.insert(MineralKind::Copper, 3);
            player.artifacts.insert(ArtifactKind::Fossil, 2);
            player.materials.insert(StrategicResourceKind::CoreShard, 4);
            player.fuel_capacity = 150.0;
            player.fuel_tank_level = 3;
            player.cargo_bay_level = 4;
            player.drill_strength = 5;
            player.engine_level = 6;
            player.hull_level = 7;
            player.radiator_level = 8;
            player.scanner_level = 2;
            player.bombs = 9;
            player.loan_debt = 1234;
            player.insured = true;
            player.insurance_tier = 2;
            player.crafted_bulkheads = 3;
            player.crafted_sorters = 4;
            player.signal_relay_kits = 5;
            player.survey_drone_kits = 6;
            player.cargo_lift_kits = 7;
            player.tunnel_support_kits = 8;
            player.pump_station_kits = 9;
            player.ore_processor_kits = 10;
        }
        let save = PersistentWorldSave::from_session(&world, &game);
        let restored_world = save.restore_world();

        let restored_player = restored_world
            .player(LOCAL_PLAYER_ID)
            .expect("restored player exists");
        assert_eq!(restored_world.simulation_tick(), SimulationTick::new(13));
        assert_eq!(restored_player.credits, 222);
        assert!((restored_player.x - 123.0).abs() < f32::EPSILON);
        assert!((restored_player.y - 234.0).abs() < f32::EPSILON);
        assert!((restored_player.velocity_x - 5.0).abs() < f32::EPSILON);
        assert!((restored_player.velocity_y + 6.0).abs() < f32::EPSILON);
        assert!((restored_player.fuel - 33.0).abs() < f32::EPSILON);
        assert!((restored_player.hull - 44.0).abs() < f32::EPSILON);
        assert_eq!(restored_player.cargo_capacity, 55);
        assert_eq!(
            restored_player.cargo.get(&MineralKind::Copper).copied(),
            Some(3)
        );
        assert_eq!(
            restored_player
                .artifacts
                .get(&ArtifactKind::Fossil)
                .copied(),
            Some(2)
        );
        assert_eq!(
            restored_player
                .materials
                .get(&StrategicResourceKind::CoreShard)
                .copied(),
            Some(4)
        );
        assert!((restored_player.fuel_capacity - 150.0).abs() < f32::EPSILON);
        assert_eq!(restored_player.fuel_tank_level, 3);
        assert_eq!(restored_player.cargo_bay_level, 4);
        assert_eq!(restored_player.drill_strength, 5);
        assert_eq!(restored_player.engine_level, 6);
        assert_eq!(restored_player.hull_level, 7);
        assert_eq!(restored_player.radiator_level, 8);
        assert_eq!(restored_player.scanner_level, 2);
        assert_eq!(restored_player.bombs, 9);
        assert_eq!(restored_player.loan_debt, 1234);
        assert!(restored_player.insured);
        assert_eq!(restored_player.insurance_tier, 2);
        assert_eq!(restored_player.crafted_bulkheads, 3);
        assert_eq!(restored_player.crafted_sorters, 4);
        assert_eq!(restored_player.signal_relay_kits, 5);
        assert_eq!(restored_player.survey_drone_kits, 6);
        assert_eq!(restored_player.cargo_lift_kits, 7);
        assert_eq!(restored_player.tunnel_support_kits, 8);
        assert_eq!(restored_player.pump_station_kits, 9);
        assert_eq!(restored_player.ore_processor_kits, 10);
    }

    #[test]
    fn persistent_world_save_preserves_multi_player_roster_and_restore_order() {
        let game = GameState::new();
        let mut world = WorldState::from_legacy_game(&game);
        let second_player_id = PlayerId::new(2);
        let mut second_player = game.player.clone();
        second_player.x = 222.0;
        second_player.y = 333.0;
        second_player.credits = 444;
        world.insert_player(second_player_id, second_player);
        world.set_simulation_tick(SimulationTick::new(99));
        world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .credits = 123;

        let save = PersistentWorldSave::from_session(&world, &game);
        let json = serde_json::to_string(&save).expect("serialize multi-player save");
        let loaded: PersistentWorldSave =
            serde_json::from_str(&json).expect("deserialize multi-player save");
        let restored_world = loaded.restore_world();

        assert_eq!(
            loaded.player_roster,
            vec![LOCAL_PLAYER_ID, second_player_id]
        );
        assert_eq!(loaded.default_player_id, LOCAL_PLAYER_ID);
        assert_eq!(restored_world.simulation_tick(), SimulationTick::new(99));
        assert_eq!(
            restored_world
                .player(LOCAL_PLAYER_ID)
                .expect("local player restored")
                .credits,
            123
        );
        let restored_second = restored_world
            .player(second_player_id)
            .expect("second player restored");
        assert_eq!(restored_second.credits, 444);
        assert!((restored_second.x - 222.0).abs() < f32::EPSILON);
        assert!((restored_second.y - 333.0).abs() < f32::EPSILON);
    }

    #[test]
    fn world_save_restore_summary_tracks_roster_and_session_state() {
        let mut game = GameState::new();
        game.player.credits = 654;
        let mut world = WorldState::from_legacy_game(&game);
        world.set_simulation_tick(SimulationTick::new(77));

        let save = PersistentWorldSave::from_session(&world, &game);
        let summary = save.restore_summary();

        assert_eq!(summary.simulation_tick, SimulationTick::new(77));
        assert_eq!(summary.roster_players, 1);
        assert_eq!(summary.persistent_players, 1);
        assert!(summary.default_player_present);
        assert!(summary.roster_matches_persistent_players());
    }

    #[test]
    fn canonical_world_save_round_trip_preserves_complete_simulation_hash() {
        let mut game = GameState::new();
        let mut world = WorldState::from_legacy_game(&game);
        world.set_simulation_tick(SimulationTick::new(71));
        world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("player exists")
            .credits = 808;
        world.set_scanner_cooldown_seconds(LOCAL_PLAYER_ID, 4.25);
        game.message = "shell-only message".to_owned();
        let expected_hash = world.simulation_state_hash();
        let path = std::env::temp_dir().join(format!(
            "drillgame-canonical-save-round-trip-{}.json",
            std::process::id()
        ));

        super::save_world_session_to_path(&world, &game, &path).expect("save canonical world");
        let loaded = load_persistent_world_from_path(&path).expect("load canonical world");
        let _ignored = std::fs::remove_file(path);

        assert_eq!(
            loaded.canonical_world.simulation_state_hash(),
            expected_hash
        );
        assert_eq!(
            loaded.canonical_world.simulation_tick(),
            SimulationTick::new(71)
        );
        assert_eq!(
            loaded.default_player().expect("restored player").credits,
            808
        );
        assert_eq!(loaded.shell.game.message, "shell-only message");
    }

    #[test]
    fn save_file_regression_round_trips_versioned_world_envelope() {
        let mut game = GameState::new();
        game.player.credits = 777;
        game.player.fuel = 42.0;
        let save = SaveFile {
            version: super::SAVE_VERSION,
            world: PersistentWorldSave::from_session(&WorldState::from_legacy_game(&game), &game),
        };

        let json = serde_json::to_string(&save).expect("serialize save");
        let loaded: SaveFile = serde_json::from_str(&json).expect("deserialize save");

        assert_eq!(loaded.version, super::SAVE_VERSION);
        assert_eq!(
            loaded.world.default_player().expect("saved player").credits,
            777
        );
        assert!((loaded.world.shell.game.player.fuel - 42.0).abs() < f32::EPSILON);
    }

    #[test]
    fn joined_online_client_shell_save_is_denied_even_without_ui_blocker() {
        let mut game = GameState::new();
        game.online_session_state = crate::game_state::OnlineSessionUxState::Connected;
        game.online_host_owns_save = false;
        game.online_player_slot = Some(2);

        let result = save_game_slot(&game, 0);

        assert!(matches!(result, Err(crate::save::SaveError::SaveDenied(_))));
    }

    #[test]
    fn host_owned_online_shell_save_is_allowed_by_save_authority_policy() {
        let mut game = GameState::new();
        game.online_session_state = crate::game_state::OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);

        let result = super::enforce_shell_local_save_authority(&game);

        assert!(result.is_ok());
    }
}
