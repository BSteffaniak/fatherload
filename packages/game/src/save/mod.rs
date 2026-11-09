//! Settings and world saves.
//!
//! A save is the host's [`WorldState`] plus a little session metadata (format version 5). Older
//! formats (1-4) are migrated on load by [`legacy`]. Loading always hands the runtime a fresh
//! world, which starts a new session epoch.

mod legacy;

use std::{
    env,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use drillgame_simulation::{WorldState, economy::DeepClaimStatus, terrain::TILE_SIZE};

pub use legacy::LEGACY_PLAYER_ID;

const SETTINGS_FILE_NAME: &str = "settings.json";
const SAVE_FILE_NAME: &str = "save.json";
const SAVE_SLOTS: usize = 3;
/// Current save format.
pub const SAVE_VERSION: u32 = 5;

/// Persisted user settings.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct SettingsFile {
    pub master_volume: f32,
    pub fullscreen: bool,
    /// Windowed size and maximized state; missing in settings written before it existed.
    #[serde(default)]
    pub window: WindowSettings,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            fullscreen: false,
            window: WindowSettings::default(),
        }
    }
}

/// The desktop window's restored (non-maximized, non-fullscreen) size in logical screen points,
/// and whether it was maximized.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WindowSettings {
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            width: crate::render_snapshot::SCREEN_WIDTH,
            height: crate::render_snapshot::SCREEN_HEIGHT,
            maximized: false,
        }
    }
}

/// Writes settings.
///
/// # Errors
///
/// Returns an error if the settings cannot be serialized or written.
pub fn save_settings(settings: SettingsFile) -> Result<(), SaveError> {
    let json = serde_json::to_string_pretty(&settings).map_err(SaveError::Serialize)?;
    write_state_file(settings_path(), json)
}

/// Reads settings, falling back to defaults.
#[must_use]
pub fn load_settings() -> SettingsFile {
    let Ok(json) = fs::read_to_string(settings_path()) else {
        return SettingsFile::default();
    };
    serde_json::from_str(&json).unwrap_or_default()
}

/// What a save holds: the authoritative world and session metadata.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorldSave {
    /// The host's world.
    pub world: WorldState,
    /// Total play time of the run, for the title/slot summary.
    #[serde(default)]
    pub play_seconds: f32,
}

#[derive(Debug, Deserialize, Serialize)]
struct SaveFile {
    version: u32,
    save: WorldSave,
}

#[derive(Debug, Deserialize)]
struct VersionProbe {
    version: u32,
}

/// Summary shown on the title screen and slot menus.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveSlotMetadata {
    pub depth: i32,
    pub credits: u32,
    pub cargo_used: u32,
    pub cargo_capacity: u32,
    pub contracts_completed: u32,
    pub play_seconds: f32,
    pub total_earnings: u32,
    pub deep_claim_unlocked: bool,
    pub modified_unix_seconds: Option<u64>,
    pub won_game: bool,
}

impl SaveSlotMetadata {
    #[must_use]
    pub fn from_save(save: &WorldSave, modified_unix_seconds: Option<u64>) -> Self {
        let world = &save.world;
        let player = world.player_ids().next().and_then(|id| world.player(id));
        #[allow(
            clippy::cast_possible_truncation,
            reason = "save slot depth is displayed as an integral tile depth"
        )]
        let depth = player.map_or(0, |player| (player.y / TILE_SIZE).floor() as i32);
        Self {
            depth,
            credits: player.map_or(0, |player| player.credits),
            cargo_used: player.map_or(0, drillgame_simulation::Player::cargo_used),
            cargo_capacity: player.map_or(0, |player| player.cargo_capacity),
            contracts_completed: world.progression().contracts.completed,
            play_seconds: save.play_seconds,
            total_earnings: world.progression().total_earnings,
            deep_claim_unlocked: world.progression().deep_claim_status == DeepClaimStatus::Unlocked,
            modified_unix_seconds,
            won_game: world.won_game(),
        }
    }
}

/// Saves to the main save file.
///
/// # Errors
///
/// Returns an error if the save cannot be serialized or written.
pub fn save_world(save: &WorldSave) -> Result<(), SaveError> {
    save_world_to_path(save, save_path())
}

/// Saves to a slot.
///
/// # Errors
///
/// Returns an error if the save cannot be serialized or written.
pub fn save_world_slot(save: &WorldSave, slot: usize) -> Result<(), SaveError> {
    save_world_to_path(save, slot_path(slot))
}

/// Saves to an explicit path.
///
/// # Errors
///
/// Returns an error if the save cannot be serialized or written.
pub fn save_world_to_path(save: &WorldSave, path: impl AsRef<Path>) -> Result<(), SaveError> {
    let file = SaveFile {
        version: SAVE_VERSION,
        save: save.clone(),
    };
    let json = serde_json::to_string(&file).map_err(SaveError::Serialize)?;
    write_state_file(path, json)
}

/// Loads the main save file.
///
/// # Errors
///
/// Returns an error if no save exists or it cannot be read or migrated.
pub fn load_world() -> Result<WorldSave, SaveError> {
    load_world_from_path(save_path())
}

/// Loads a slot.
///
/// # Errors
///
/// Returns an error if the slot is empty or cannot be read or migrated.
pub fn load_world_slot(slot: usize) -> Result<WorldSave, SaveError> {
    load_world_from_path(slot_path(slot))
}

/// Loads the most recently written save (main file or any slot).
///
/// # Errors
///
/// Returns an error if no save exists or it cannot be read or migrated.
pub fn load_latest_world() -> Result<WorldSave, SaveError> {
    let path = latest_save_path().ok_or(SaveError::NoSaveFound)?;
    load_world_from_path(path)
}

/// Loads and migrates a save of any supported version.
///
/// # Errors
///
/// Returns an error if the file cannot be read, has an unsupported version, or does not match
/// any known format.
pub fn load_world_from_path(path: impl AsRef<Path>) -> Result<WorldSave, SaveError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(SaveError::NoSaveFound);
    }
    let json = fs::read_to_string(path).map_err(SaveError::Io)?;
    parse_save(&json)
}

fn parse_save(json: &str) -> Result<WorldSave, SaveError> {
    let probe: VersionProbe = serde_json::from_str(json).map_err(SaveError::Serialize)?;
    match probe.version {
        SAVE_VERSION => serde_json::from_str::<SaveFile>(json)
            .map(|file| file.save)
            .map_err(SaveError::Serialize),
        4 => serde_json::from_str::<legacy::V4SaveFile>(json)
            .and_then(legacy::V4SaveFile::migrate)
            .map_err(SaveError::Serialize),
        1..=3 => {
            if let Ok(file) = serde_json::from_str::<legacy::V3SaveFile>(json) {
                return file.migrate().map_err(SaveError::Serialize);
            }
            if let Ok(file) = serde_json::from_str::<legacy::V2SaveFile>(json) {
                return Ok(file.migrate());
            }
            serde_json::from_str::<legacy::V1SaveFile>(json)
                .map(legacy::V1SaveFile::migrate)
                .map_err(SaveError::Serialize)
        }
        other => Err(SaveError::UnsupportedVersion(other)),
    }
}

/// Metadata for a slot, if it holds a readable save.
#[must_use]
pub fn save_slot_metadata(slot: usize) -> Option<SaveSlotMetadata> {
    save_metadata_from_path(slot_path(slot))
}

fn save_metadata_from_path(path: impl AsRef<Path>) -> Option<SaveSlotMetadata> {
    let path = path.as_ref();
    let modified_unix_seconds = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    let save = load_world_from_path(path).ok()?;
    Some(SaveSlotMetadata::from_save(&save, modified_unix_seconds))
}

/// Whether the main save file exists.
#[must_use]
pub fn save_exists() -> bool {
    save_path().exists()
}

/// Whether any save (main or slot) exists.
#[must_use]
pub fn saves_exist() -> bool {
    save_exists() || (0..SAVE_SLOTS).any(save_slot_exists)
}

/// Metadata of the most recent save.
#[must_use]
pub fn latest_save_summary() -> Option<SaveSlotMetadata> {
    latest_save_path().and_then(save_metadata_from_path)
}

/// Whether a slot file exists.
#[must_use]
pub fn save_slot_exists(slot: usize) -> bool {
    slot_path(slot).exists()
}

/// Number of save slots.
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
    // Windows: %LOCALAPPDATA%\drillgame (HOME is usually unset, and temp gets cleaned).
    if cfg!(windows)
        && let Ok(local_app_data) = env::var("LOCALAPPDATA")
    {
        return PathBuf::from(local_app_data).join("drillgame");
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("drillgame");
    }
    env::temp_dir().join("drillgame")
}

/// Save/load failures.
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
    use drillgame_simulation::{PlayerId, SimulationTick, terrain::MineralKind};

    use super::*;

    const V1: &str = include_str!("fixtures/v1.json");
    const V2: &str = include_str!("fixtures/v2.json");
    const V3: &str = include_str!("fixtures/v3.json");

    fn player(save: &WorldSave) -> &drillgame_simulation::Player {
        save.world.player(LEGACY_PLAYER_ID).expect("legacy rig")
    }

    #[test]
    fn settings_without_window_state_keep_their_values() {
        let settings: SettingsFile =
            serde_json::from_str(r#"{"master_volume":0.25,"fullscreen":true}"#)
                .expect("old settings parse");
        assert!((settings.master_volume - 0.25).abs() < f32::EPSILON);
        assert!(settings.fullscreen);
        assert_eq!(settings.window, WindowSettings::default());

        let round_trip: SettingsFile = serde_json::from_str(
            &serde_json::to_string(&SettingsFile {
                window: WindowSettings {
                    width: 1600,
                    height: 900,
                    maximized: true,
                },
                ..SettingsFile::default()
            })
            .expect("serialize"),
        )
        .expect("parse");
        assert_eq!(round_trip.window.width, 1600);
        assert!(round_trip.window.maximized);
    }

    #[test]
    fn version_one_game_state_migrates_to_a_world() {
        let save = parse_save(V1).expect("v1 migrates");
        assert_eq!(player(&save).credits, 1234);
        assert_eq!(player(&save).cargo.get(&MineralKind::Gold), Some(&3));
        assert_eq!(player(&save).bombs, 2);
        assert!((save.play_seconds - 321.0).abs() < f32::EPSILON);
        assert_eq!(save.world.progression().total_earnings, 900);
        assert_eq!(save.world.progression().town_development.depot_level, 2);
        assert_eq!(save.world.terrain().width(), 12);
        assert!(
            save.world.terrain().height() >= 220,
            "legacy depth extension"
        );
    }

    #[test]
    fn version_two_envelope_restores_session_tick_and_objects() {
        let save = parse_save(V2).expect("v2 migrates");
        assert_eq!(save.world.simulation_tick(), SimulationTick::new(27));
        assert_eq!(player(&save).credits, 1234);
        assert!((save.play_seconds - 321.0).abs() < f32::EPSILON);
    }

    #[test]
    fn version_three_canonical_world_loads_as_is() {
        let save = parse_save(V3).expect("v3 loads");
        assert_eq!(player(&save).credits, 4321);
        assert_eq!(save.world.simulation_tick(), SimulationTick::new(27));
        assert!((save.play_seconds - 321.0).abs() < f32::EPSILON);
    }

    #[test]
    fn version_four_lifts_the_won_flag_out_of_the_derived_summary() {
        let v3: serde_json::Value = serde_json::from_str(V3).expect("v3 json");
        let mut world = v3["world"]["canonical_world"].clone();
        world["authoritative_summary"]["won_game"] = serde_json::Value::Bool(true);
        let v4 = serde_json::json!({
            "version": 4,
            "save": { "world": world, "play_seconds": 9.0 },
        });
        let save = parse_save(&v4.to_string()).expect("v4 migrates");
        assert!(save.world.won_game());
        assert_eq!(player(&save).credits, 4321);
        assert!((save.play_seconds - 9.0).abs() < f32::EPSILON);
    }

    #[test]
    fn current_format_round_trips_the_world_exactly() {
        let mut world = WorldState::new_game(PlayerId::new(1));
        world.player_mut(PlayerId::new(1)).expect("rig").credits = 77;
        world.set_simulation_tick(SimulationTick::new(500));
        let save = WorldSave {
            world,
            play_seconds: 12.5,
        };
        let path = std::env::temp_dir().join(format!(
            "drillgame-save-roundtrip-{}.json",
            std::process::id()
        ));
        save_world_to_path(&save, &path).expect("write");
        let loaded = load_world_from_path(&path).expect("read");
        let _ignored = fs::remove_file(&path);
        assert_eq!(
            loaded.world.simulation_state_hash(),
            save.world.simulation_state_hash()
        );
        assert!((loaded.play_seconds - 12.5).abs() < f32::EPSILON);
        let metadata = SaveSlotMetadata::from_save(&loaded, None);
        assert_eq!(metadata.credits, 77);
    }

    #[test]
    fn world_settings_survive_a_save_and_older_v5_saves_load_as_legacy_worlds() {
        use drillgame_simulation::{
            SimulationParams,
            worldgen::{GeneratorVersion, HazardIntensity, ResourceRichness, WorldSettings},
        };
        let settings = WorldSettings {
            richness: ResourceRichness::Lean,
            hazards: HazardIntensity::Brutal,
            ..WorldSettings::new(0x5EED_CAFE)
        };
        let world = WorldState::new_game_with_settings(
            PlayerId::new(1),
            &settings,
            SimulationParams::default(),
        );
        let json = serde_json::to_value(SaveFile {
            version: SAVE_VERSION,
            save: WorldSave {
                world: world.clone(),
                play_seconds: 0.0,
            },
        })
        .expect("serialize");
        let loaded = parse_save(&json.to_string()).expect("current save loads");
        assert_eq!(*loaded.world.world_settings(), settings);
        assert_eq!(
            loaded.world.simulation_state_hash(),
            world.simulation_state_hash()
        );
        assert_eq!(loaded.world.world_features(), world.world_features());

        // A v5 save written before world settings existed: same tiles, legacy settings.
        let mut old = json;
        old["save"]["world"]["terrain"]
            .as_object_mut()
            .expect("terrain object")
            .remove("settings");
        let loaded = parse_save(&old.to_string()).expect("pre-settings v5 save loads");
        assert_eq!(
            loaded.world.world_settings().generator,
            GeneratorVersion::Legacy
        );
        assert_eq!(loaded.world.world_settings().seed, 0x5EED_CAFE);
        assert_eq!(
            loaded.world.terrain().digest(),
            world.terrain().digest(),
            "the saved tiles load unchanged"
        );
        assert!(loaded.world.world_features().is_empty());
    }

    #[test]
    fn unknown_versions_are_rejected() {
        assert!(matches!(
            parse_save(r#"{"version": 99}"#),
            Err(SaveError::UnsupportedVersion(99))
        ));
    }
}
