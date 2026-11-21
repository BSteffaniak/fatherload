//! Settings and world saves.
//!
//! A save is the host's [`WorldState`] plus a little session metadata (format version 10). The
//! world's terrain is stored compactly: its generation settings (seed, size, richness, hazards,
//! generator version), a digest and only the tiles that differ from freshly generated terrain;
//! loading regenerates the rest and checks the digest. Rigs carry their goods in item containers
//! (cargo bay, block hopper) and live world objects such as gas clouds and falling boulders are
//! entities in the world's entity store, explosive charges included. Version 8 also adds the
//! crew roster (WP-PERSIST): who plays each rig (persistent profile, name, local seat) and the
//! players parked outside the live world, so a resumed multiplayer save remembers everyone; a v7
//! save loads with an empty roster and its rigs beyond the host's seats are parked when it is
//! hosted. Older formats (1-5, which wrote out every tile, 6, from before the item and entity
//! models, and 7, with a separate charge list and explosive count maps) are migrated on load by
//! [`legacy`]. Loading always hands the runtime a fresh world, which
//! starts a new session epoch.

mod legacy;

use std::{
    env,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use fatherload_simulation::{
    WorldState, economy::DeepClaimStatus, terrain::TILE_SIZE, worldgen::GeneratorVersion,
};

pub use legacy::LEGACY_PLAYER_ID;

const SETTINGS_FILE_NAME: &str = "settings.json";
const SAVE_FILE_NAME: &str = "save.json";
const SAVE_SLOTS: usize = 3;
/// Current save format.
///
/// 8 (wave 2, one bump for every wave 2 track): explosive charges are entities and special
/// explosives cargo bay items (T11; version 7 files migrate through `legacy::V7SaveFile`), plus
/// fields with serde defaults: drop magnet / dump / "blocked" marks and the `collect_blocks`
/// toggle (T7), enemies and the enemy director (T10), cave and village journals (T13, T9),
/// player bases (T12) and the crew roster of parked players and profiles (WP-PERSIST).
///
/// 9 (expansion wave 3), one bump for all tracks; every new field has a serde default, so a
/// version 8 file loads as is (same layout) and the bump makes older builds refuse the file:
///
/// * T14: the world clock (day length, skipped nights, the sleep rule and who is in bed),
///   loading as the standard 48 minute day;
/// * T15 pilot on foot: pilot entities in the entity store, pilot gear and the personal pack on
///   players, parked pilots in the crew roster, vent grates and ropes;
/// * T17 people in the world: NPC entities and the world's people ledger (every character's
///   memory of every player, conversations, bought offers, perks);
/// * T18 upgrade reward moments: each player's milestone log and records and the
///   milestone-reward cosmetics.
///
/// 10 (expansion wave 4; the integrator collapses the wave's bumps into one); every new field
/// has a serde default, so version 8 and 9 files load as is:
///
/// * T21: generator V9 worlds (sky continent content), new sky tile, artifact, material, enemy
///   and weapon variants, and the tether line in each rig's weapon state;
/// * T23 pilot content: the pilot arsenal (personal arms and ammo) on players, the new pilot
///   gear tiers, pilot entities' arms / rope / recall state, pressure-plate timers in the bases,
///   ruin tiles in the terrain diffs and `GeneratorVersion::V9` worlds;
/// * T24 contractor homes: the world's `homes` (plots, deeds, contracts, finished homes and
///   their decor, furniture stocks, home points), site-crew NPCs (`NpcState::site`) and home
///   beds (`BedSpot::Home`);
/// * T25 night: night monster kinds and wispfire shots in the entity store, night materials in
///   cargo bays, palisade tiles and the night ledger inside the enemy director;
/// * T26 story arcs: each player's arc log in the quest log (the open offer, running arcs,
///   history, promised follow-ups) and the people ledger's story marks (prosperity shifts,
///   changed relationships, unlocked offers).
///
/// 11 (expansion wave 5; the integrator collapses the wave's bumps into one); every new field
/// has a serde default, so version 8 to 10 files load as is:
///
/// * T27 tombs core: `GeneratorVersion::V10` worlds (tomb sites and the run vault), the
///   world's `tombs` (live runs with their party, relics, loot and downed members, each site's
///   run counter and seal, every player's tomb unlocks and last run summary), the tomb tiles
///   in the terrain diffs (entrances, and the floor of a run saved mid-floor), pilot entities'
///   view and relic boons, projectiles' bounces and the tomb key in packs and bays. A run in
///   progress saves and loads as it stands (the game also saves at every shrine).
/// * T29 top-down crypts: a crypt run's trap overlays and their state (cracked floors giving
///   way, pressed plates), pilot entities' roll and heading, crypt creatures' phase and
///   pattern.
pub const SAVE_VERSION: u32 = 11;

/// Persisted user settings.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct SettingsFile {
    pub master_volume: f32,
    pub fullscreen: bool,
    /// Windowed size and maximized state; missing in settings written before it existed.
    #[serde(default)]
    pub window: WindowSettings,
    /// Touch control preferences; missing in settings written before they existed.
    #[serde(default)]
    pub touch: crate::input::touch::TouchSettings,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            fullscreen: false,
            window: WindowSettings::default(),
            touch: crate::input::touch::TouchSettings::default(),
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

/// Just the generator version of a v5/v6 save's terrain, read before the world itself so a save
/// from a newer generator fails with a clear error.
#[derive(Debug, Default, Deserialize)]
struct GeneratorProbe {
    #[serde(default)]
    save: GeneratorProbeSave,
}

#[derive(Debug, Default, Deserialize)]
struct GeneratorProbeSave {
    #[serde(default)]
    world: GeneratorProbeWorld,
}

#[derive(Debug, Default, Deserialize)]
struct GeneratorProbeWorld {
    #[serde(default)]
    terrain: GeneratorProbeTerrain,
}

#[derive(Debug, Default, Deserialize)]
struct GeneratorProbeTerrain {
    #[serde(default)]
    settings: Option<GeneratorProbeSettings>,
}

#[derive(Debug, Deserialize)]
struct GeneratorProbeSettings {
    generator: serde_json::Value,
}

/// Rejects a save whose terrain names a world generator this build does not have.
fn check_generator(json: &str) -> Result<(), SaveError> {
    let probe: GeneratorProbe = serde_json::from_str(json).unwrap_or_default();
    let Some(settings) = probe.save.world.terrain.settings else {
        return Ok(());
    };
    if serde_json::from_value::<GeneratorVersion>(settings.generator.clone()).is_err() {
        return Err(SaveError::UnknownGenerator(match settings.generator {
            serde_json::Value::String(name) => name,
            other => other.to_string(),
        }));
    }
    Ok(())
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
    /// The world is a test-mode world.
    pub test_mode: bool,
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
            cargo_used: player.map_or(0, fatherload_simulation::Player::cargo_used),
            cargo_capacity: player.map_or(0, |player| player.cargo_capacity),
            contracts_completed: world.progression().contracts.completed,
            play_seconds: save.play_seconds,
            total_earnings: world.progression().total_earnings,
            deep_claim_unlocked: world.progression().deep_claim_status == DeepClaimStatus::Unlocked,
            modified_unix_seconds,
            won_game: world.won_game(),
            test_mode: world.is_test_mode(),
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
    parse_save_file(json).map(|mut save| {
        // Rigs from the old five-tier upgrade curves catch up to the current tank and bay sizes.
        save.world.catch_up_capacity_curves();
        save
    })
}

fn parse_save_file(json: &str) -> Result<WorldSave, SaveError> {
    let probe: VersionProbe = serde_json::from_str(json).map_err(SaveError::Serialize)?;
    if matches!(probe.version, 5..=SAVE_VERSION) {
        check_generator(json)?;
    }
    match probe.version {
        // Version 8 to 10 files have the current layout minus serde-defaulted wave 3, 4 and
        // 5 fields.
        8..=10 | SAVE_VERSION => serde_json::from_str::<SaveFile>(json)
            .map(|file| file.save)
            .map_err(SaveError::Serialize),
        7 => serde_json::from_str::<legacy::V7SaveFile>(json)
            .and_then(legacy::V7SaveFile::migrate)
            .map_err(SaveError::Serialize),
        6 => serde_json::from_str::<legacy::V6SaveFile>(json)
            .and_then(legacy::V6SaveFile::migrate)
            .map_err(SaveError::Serialize),
        5 => serde_json::from_str::<legacy::V5SaveFile>(json)
            .and_then(legacy::V5SaveFile::migrate)
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

/// Writes a file in (or under) the state directory; in the browser it also queues the
/// `IndexedDB` flush.
pub fn write_state_file(path: impl AsRef<Path>, contents: String) -> Result<(), SaveError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(SaveError::Io)?;
    }
    fs::write(path, contents).map_err(SaveError::Io)?;
    #[cfg(target_os = "emscripten")]
    crate::browser::persist_saves();
    Ok(())
}

/// Path of a file named `name` in the state directory.
pub fn state_file_path(name: &str) -> PathBuf {
    state_file_path_in(&default_state_dir(), name)
}

/// Path of a file named `name` in the state directory `dir`.
pub fn state_file_path_in(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
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
    if let Ok(path) = env::var("FATHERLOAD_STATE_DIR") {
        return PathBuf::from(path);
    }
    let root = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            cfg!(windows)
                .then(|| env::var_os("LOCALAPPDATA").map(PathBuf::from))
                .flatten()
        })
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(env::temp_dir);
    root.join("fatherload")
}

/// Save/load failures.
#[derive(Debug)]
pub enum SaveError {
    Io(io::Error),
    Serialize(serde_json::Error),
    UnsupportedVersion(u32),
    /// The save's world was generated by a world generator this build does not have.
    UnknownGenerator(String),
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
            Self::UnknownGenerator(name) => write!(
                formatter,
                "the save's world comes from an unknown world generator ({name}); it needs a \
                 newer version of the game"
            ),
            Self::NoSaveFound => formatter.write_str("no save file found"),
            Self::SaveDenied(reason) => write!(formatter, "save denied: {reason}"),
        }
    }
}

impl Error for SaveError {}

#[cfg(test)]
mod cross_version_tests;

#[cfg(test)]
mod tests {
    use fatherload_simulation::{
        PlayerId, SimulationParams, SimulationTick,
        terrain::{FullTerrain, MineralKind},
        worldgen::WorldSettings,
    };

    use super::*;

    const V1: &str = include_str!("fixtures/v1.json");
    const V2: &str = include_str!("fixtures/v2.json");
    const V3: &str = include_str!("fixtures/v3.json");

    fn player(save: &WorldSave) -> &fatherload_simulation::Player {
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
        assert_eq!(player(&save).cargo_bay.get(MineralKind::Gold), Some(&3));
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
    fn save_load_preserves_an_airborne_chain_triggered_bomb_and_its_future() {
        use fatherload_simulation::{PlacedBomb, TickInputs};
        let mut world = fatherload_simulation::WorldState::new_game(LEGACY_PLAYER_ID);
        world.insert_charge(PlacedBomb {
            id: 81,
            x: 500.0,
            y: 50.0,
            previous_x: 499.0,
            previous_y: 52.0,
            velocity_x: 110.0,
            velocity_y: -140.0,
            angle: 0.4,
            chain_triggered: true,
            detonation_tick: SimulationTick::new(30),
            timer_seconds: 0.5,
            ..PlacedBomb::default()
        });
        let save = WorldSave {
            world,
            play_seconds: 4.0,
        };
        let json = serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            save: save.clone(),
        })
        .unwrap();
        let mut restored = parse_save(&json).unwrap();
        let mut original = save;
        for _ in 0..60 {
            let tick = original.world.simulation_tick();
            let output = original.world.simulate_tick(tick, &TickInputs::default());
            let replayed = restored.world.simulate_tick(tick, &TickInputs::default());
            assert_eq!(output, replayed);
            assert_eq!(
                original.world.simulation_state_hash(),
                restored.world.simulation_state_hash()
            );
        }
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
            "fatherload-save-roundtrip-{}.json",
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
    fn version_7_saves_without_drop_marks_load_with_defaults() {
        /// Removes what version 8 added, as a version 7 game wrote it.
        fn strip(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    map.remove("collect_blocks");
                    if map.contains_key("stack") {
                        for key in ["pulled_by", "ignored_by", "blocked"] {
                            map.remove(key);
                        }
                    }
                    map.values_mut().for_each(strip);
                }
                serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
                _ => {}
            }
        }
        let player = PlayerId::new(1);
        let mut world = WorldState::new_game(player);
        let _ = world.spawn_item_drop(
            100.0,
            40.0,
            (0.0, 0.0),
            fatherload_simulation::item::ItemStack::new(
                fatherload_simulation::item::ItemKind::Mineral(
                    fatherload_simulation::terrain::MineralKind::Gold,
                ),
                2,
            ),
            None,
        );
        let save = WorldSave {
            world,
            play_seconds: 1.0,
        };
        let mut json: serde_json::Value = serde_json::from_str(
            &serde_json::to_string(&SaveFile {
                version: SAVE_VERSION,
                save: save.clone(),
            })
            .expect("json"),
        )
        .expect("value");
        // Strip what version 8 added, as a version 7 game wrote it.
        strip(&mut json);
        json["version"] = 7.into();
        let text = json.to_string();
        assert!(!text.contains("collect_blocks") && !text.contains("pulled_by"));
        let loaded = parse_save_file(&text).expect("a v7 save loads");
        assert_eq!(
            loaded.world.simulation_state_hash(),
            save.world.simulation_state_hash()
        );
        assert!(loaded.world.player(player).expect("rig").collect_blocks);
    }

    #[test]
    fn the_crew_roster_saves_and_v7_saves_load_with_every_rig() {
        use fatherload_simulation::{CrewMember, ProfileId};
        let mut world = WorldState::new_game(PlayerId::new(1));
        world.add_player(PlayerId::new(2));
        world.add_player(PlayerId::new(3));
        world.set_crew_member(
            PlayerId::new(2),
            CrewMember {
                profile: Some(ProfileId::new(0xa1e8)),
                name: "Alex".to_owned(),
                seat: None,
            },
        );
        world.player_mut(PlayerId::new(2)).expect("alex").credits = 2_222;
        assert!(world.park_player(PlayerId::new(2)));
        let save = WorldSave {
            world,
            play_seconds: 3.0,
        };
        let json = serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            save,
        })
        .expect("serialize");
        assert!(json.contains("\"crew\""), "the roster is in the file");
        let loaded = parse_save(&json).expect("v8 loads");
        let parked = loaded
            .world
            .parked_player(PlayerId::new(2))
            .expect("Alex stays parked in the save");
        assert_eq!(parked.member.name, "Alex");
        assert_eq!(parked.player.credits, 2_222);
        assert_eq!(
            loaded.world.player_for_profile(ProfileId::new(0xa1e8)),
            Some(PlayerId::new(2))
        );

        // A v7 save (no roster) keeps every rig live; hosting it parks the extras.
        let mut v7: serde_json::Value = serde_json::from_str(&json).expect("json");
        v7["version"] = serde_json::json!(7);
        v7["save"]["world"]
            .as_object_mut()
            .expect("world object")
            .remove("crew");
        let loaded = parse_save(&v7.to_string()).expect("v7 loads");
        assert!(loaded.world.crew().is_empty());
        assert_eq!(
            loaded.world.player_ids().collect::<Vec<_>>(),
            [PlayerId::new(1), PlayerId::new(3)]
        );
    }

    #[test]
    fn people_memories_survive_a_save_and_v8_saves_load_without_them() {
        use fatherload_simulation::{
            PlayerCommand, SimulationParams, SimulationTick, TickInputs,
            entity::{EntityKind, EntityState},
            people::{DayTime, PeopleCommand, TalkTopic, TickClock},
            village::{self, placement},
            worldgen::GeneratorVersion,
        };
        let settings = WorldSettings {
            generator: GeneratorVersion::V7,
            ..WorldSettings::new(0x5EED_0017)
        };
        let player = PlayerId::new(1);
        let mut world =
            WorldState::new_game_with_settings(player, &settings, SimulationParams::new(60));
        let found = village::village(&settings, placement::surface_id(1)).expect("village");
        let tick = TickClock::tick_at(DayTime::at(1, 10, 0), 60, 48);
        world.set_simulation_tick(SimulationTick::new(tick));
        {
            let rig = world.player_mut(player).expect("rig");
            #[allow(clippy::cast_precision_loss, reason = "small tiles")]
            {
                rig.x = (i32::midpoint(found.plaza.0, found.plaza.1) as f32 + 0.5)
                    * fatherload_simulation::TILE_SIZE;
                rig.y = (found.ground_row as f32 - 0.5) * fatherload_simulation::TILE_SIZE;
            }
        }
        for _ in 0..40 {
            let tick = world.simulation_tick();
            let _ = world.simulate_tick(tick, &TickInputs::default());
        }
        let (npc, x) = world
            .entities()
            .of_kind(EntityKind::Npc)
            .find_map(|entity| match entity.state {
                EntityState::Npc(state) if state.inside.is_none() => {
                    Some((entity.id, state.x_at(world.simulation_tick(), 0.0)))
                }
                _ => None,
            })
            .expect("someone outdoors");
        world.player_mut(player).expect("rig").x = x;
        let mut inputs = TickInputs::default();
        inputs.players.entry(player).or_default().actions.push(
            fatherload_simulation::command::TickAction {
                sequence: fatherload_simulation::InputSequence::new(0),
                command: PlayerCommand::People(PeopleCommand::Talk {
                    npc,
                    topic: TalkTopic::Chat,
                }),
            },
        );
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
        assert!(!world.people().is_empty(), "the talk was remembered");
        let save = WorldSave {
            world,
            play_seconds: 9.0,
        };
        let json = serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            save,
        })
        .expect("serialize");
        let loaded = parse_save(&json).expect("v9 loads");
        let original: serde_json::Value = serde_json::from_str(&json).expect("json");
        assert_eq!(
            serde_json::to_value(loaded.world.people()).expect("people"),
            original["save"]["world"]["people"]
        );
        // A version 8 file has no people: it loads with nobody remembering anything.
        let mut v8 = original;
        v8["version"] = serde_json::json!(8);
        v8["save"]["world"]
            .as_object_mut()
            .expect("world object")
            .remove("people");
        let loaded = parse_save(&v8.to_string()).expect("v8 loads");
        assert!(loaded.world.people().is_empty());
    }

    #[test]
    fn world_settings_survive_a_save_and_older_v5_saves_load_as_legacy_worlds() {
        use fatherload_simulation::worldgen::{
            GeneratorVersion, HazardIntensity, ResourceRichness,
        };
        let settings = WorldSettings {
            richness: ResourceRichness::Lean,
            hazards: HazardIntensity::Brutal,
            ..WorldSettings::bounded(0x5EED_CAFE)
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
        let mut old = v5_json(&WorldSave {
            world: world.clone(),
            play_seconds: 0.0,
        });
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

    /// A v5 save of `save`: the same envelope with the full-tile terrain v5 wrote.
    fn v5_json(save: &WorldSave) -> serde_json::Value {
        let mut world = serde_json::to_value(&save.world).expect("world json");
        world["terrain"] =
            serde_json::to_value(FullTerrain::from(save.world.terrain())).expect("full terrain");
        serde_json::json!({
            "version": 5,
            "save": { "world": world, "play_seconds": save.play_seconds },
        })
    }

    fn v6_json(save: &WorldSave) -> String {
        serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            save: save.clone(),
        })
        .expect("serialize")
    }

    /// Rewrites a current save as format 6 wrote it: separate rig cargo maps, lost-cargo
    /// mineral/artifact maps, and gas cloud / boulder lists instead of entities.
    fn as_v6_json(save: &WorldSave) -> serde_json::Value {
        use fatherload_simulation::{
            entity::{EntityKind, EntityState},
            item::ItemContainer,
        };
        fn v6_player(player: &mut serde_json::Value) {
            let object = player.as_object_mut().expect("player object");
            let bay: ItemContainer =
                serde_json::from_value(object.remove("cargo_bay").expect("bay")).expect("bay");
            object.remove("block_hopper");
            object.remove("block_hopper_capacity");
            object.insert("cargo".into(), serde_json::json!(bay.mineral_map()));
            object.insert("artifacts".into(), serde_json::json!(bay.artifact_map()));
            object.insert("materials".into(), serde_json::json!(bay.material_map()));
        }
        let mut world = serde_json::to_value(&save.world).expect("world json");
        let object = world.as_object_mut().expect("world object");
        v6_player(object.get_mut("player_template").expect("template"));
        for player in object
            .get_mut("players")
            .and_then(serde_json::Value::as_object_mut)
            .expect("players")
            .values_mut()
        {
            v6_player(player);
        }
        for site in object
            .get_mut("lost_cargo_sites")
            .and_then(serde_json::Value::as_object_mut)
            .expect("sites")
            .values_mut()
        {
            let site = site.as_object_mut().expect("site");
            let items: ItemContainer =
                serde_json::from_value(site.remove("items").expect("items")).expect("items");
            site.insert("minerals".into(), serde_json::json!(items.mineral_map()));
            site.insert("artifacts".into(), serde_json::json!(items.artifact_map()));
        }
        object.remove("entities");
        let entities = save.world.entities();
        let boulders =
            entities
                .of_kind(EntityKind::Boulder)
                .filter_map(|entity| match entity.state {
                    EntityState::Boulder(boulder) => Some(boulder),
                    _ => None,
                });
        let (warnings, expiries): (Vec<_>, Vec<_>) = boulders
            .map(|boulder| (boulder.warning_deadline, boulder.expiry_tick))
            .unzip();
        object.insert("hazards".into(), serde_json::json!(save.world.hazards()));
        object.insert(
            "falling_boulders".into(),
            serde_json::json!(save.world.falling_boulders()),
        );
        object.insert(
            "boulder_warning_deadlines".into(),
            serde_json::json!(warnings),
        );
        object.insert("boulder_expiry_ticks".into(), serde_json::json!(expiries));
        serde_json::json!({
            "version": 6,
            "save": { "world": world, "play_seconds": save.play_seconds },
        })
    }

    #[test]
    fn version_six_cargo_maps_and_live_object_lists_migrate_to_items_and_entities() {
        use fatherload_simulation::{
            FallingBoulder, HazardCloud, LostCargoSite, SectionKey, SectionPayload,
            terrain::{ArtifactKind, StrategicResourceKind},
        };
        let mut world = WorldState::new_game(LEGACY_PLAYER_ID);
        world.set_simulation_tick(SimulationTick::new(40));
        let rig = world.player_mut(LEGACY_PLAYER_ID).expect("rig");
        rig.cargo_bay.add(MineralKind::Gold, 3);
        rig.cargo_bay.add(MineralKind::Copper, 5);
        rig.cargo_bay.add(ArtifactKind::Fossil, 1);
        rig.cargo_bay.add(StrategicResourceKind::CoreShard, 2);
        let Some(SectionPayload::Loadout(mut loadout)) =
            world.section(SectionKey::Loadout(LEGACY_PLAYER_ID))
        else {
            panic!("loadout section");
        };
        loadout.lost_cargo_site = Some(LostCargoSite {
            x: 320.0,
            y: 900.0,
            items: [
                (MineralKind::Iron.into(), 4),
                (ArtifactKind::OldCircuit.into(), 1),
            ]
            .into_iter()
            .collect(),
        });
        world.apply_section(
            SectionKey::Loadout(LEGACY_PLAYER_ID),
            SectionPayload::Loadout(loadout),
        );
        world.restore_legacy_live_entities(
            vec![HazardCloud {
                x: 3_000.0,
                y: 400.0,
                life: 4.0,
                radius: 12.0,
            }],
            vec![FallingBoulder {
                x: 3_100.0,
                y: 380.0,
                velocity_y: 0.0,
                warning_seconds: 1.0,
                life: 6.0,
            }],
            &[SimulationTick::new(100)],
            &[SimulationTick::new(400)],
        );
        let save = WorldSave {
            world,
            play_seconds: 7.0,
        };
        let v6 = as_v6_json(&save);
        assert!(v6["save"]["world"]["players"]["1"]["cargo"]["Gold"] == 3);
        let migrated = parse_save(&v6.to_string()).expect("v6 migrates");
        assert_eq!(
            migrated.world.simulation_state_hash(),
            save.world.simulation_state_hash(),
            "items and entities migrate exactly"
        );
        let rig = player(&migrated);
        assert_eq!(rig.cargo_bay.mineral(MineralKind::Gold), 3);
        assert_eq!(rig.cargo_bay.material(StrategicResourceKind::CoreShard), 2);
        assert_eq!(rig.cargo_used(), 9, "materials still take no slots");
        assert_eq!(migrated.world.hazards().len(), 1);
        assert_eq!(migrated.world.falling_boulders().len(), 1);
        assert_eq!(
            migrated
                .world
                .lost_cargo_sites()
                .get(&LEGACY_PLAYER_ID)
                .map(LostCargoSite::count),
            Some(5)
        );
    }

    #[test]
    fn version_seven_charge_lists_and_explosive_maps_migrate_to_entities_and_items() {
        use fatherload_simulation::{ExplosiveKind, PlacedBomb, entity::EntityKind};
        let mut world = WorldState::new_game(LEGACY_PLAYER_ID);
        world.set_simulation_tick(SimulationTick::new(12));
        let rig = world.player_mut(LEGACY_PLAYER_ID).expect("rig");
        rig.bombs = 4;
        rig.add_explosives(ExplosiveKind::C4, 2);
        rig.add_explosives(ExplosiveKind::Flare, 5);
        rig.cargo_bay.add(MineralKind::Gold, 1);
        for (id, kind, x) in [
            (3_u64, ExplosiveKind::Bomb, 490.0),
            (5, ExplosiveKind::C4, 550.0),
        ] {
            world.insert_charge(PlacedBomb {
                id,
                x,
                y: 60.0,
                previous_x: 400.0,
                previous_y: 60.0,
                velocity_x: 20.0,
                detonation_tick: SimulationTick::new(90),
                timer_seconds: 1.2,
                kind,
                owner: Some(LEGACY_PLAYER_ID),
                ..PlacedBomb::default()
            });
        }
        let save = WorldSave {
            world,
            play_seconds: 9.0,
        };
        // Format 7: charges as a world list, special explosives as a count map per rig.
        let mut world_json = serde_json::to_value(&save.world).expect("world json");
        let object = world_json.as_object_mut().expect("object");
        object.insert("bombs".into(), serde_json::json!(save.world.bombs()));
        let entities = object.get_mut("entities").expect("entities");
        entities["entities"] = serde_json::json!(
            save.world
                .entities()
                .iter()
                .filter(|entity| entity.kind() != EntityKind::Charge)
                .collect::<Vec<_>>()
        );
        let player =
            &mut object.get_mut("players").expect("players")[LEGACY_PLAYER_ID.get().to_string()];
        player["cargo_bay"] = serde_json::json!([{ "kind": { "Mineral": "Gold" }, "count": 1 }]);
        player["explosives"] = serde_json::json!({ "C4": 2, "Flare": 5 });
        let v7 = serde_json::json!({
            "version": 7,
            "save": { "world": world_json, "play_seconds": save.play_seconds },
        });
        let migrated = parse_save(&v7.to_string()).expect("v7 migrates");
        assert_eq!(
            format!("{:?}", migrated.world.bombs()),
            format!("{:?}", save.world.bombs()),
            "charges become entities with their whole state"
        );
        let rig = player_of(&migrated);
        assert_eq!(rig.bombs, 4);
        assert_eq!(rig.explosive_count(ExplosiveKind::C4), 2);
        assert_eq!(rig.explosive_count(ExplosiveKind::Flare), 5);
        assert_eq!(rig.cargo_bay.mineral(MineralKind::Gold), 1);
        assert_eq!(rig.cargo_used(), 1, "explosives take no cargo slots");
        // Re-saved as the current format and loaded again: unchanged.
        let json = serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            save: migrated.clone(),
        })
        .expect("serialize");
        let again = parse_save(&json).expect("current format");
        assert_eq!(
            again.world.simulation_state_hash(),
            migrated.world.simulation_state_hash()
        );
    }

    fn player_of(save: &WorldSave) -> &fatherload_simulation::Player {
        save.world.player(LEGACY_PLAYER_ID).expect("player")
    }

    /// A world after a long scripted dig (drilling, bombs, infrastructure, scanning) plus
    /// hundreds of blast craters and tunnels across the whole map.
    fn heavily_dug_world() -> WorldState {
        heavily_dug_world_with(&WorldSettings::bounded(
            fatherload_simulation::worldgen::DEFAULT_WORLD_SEED,
        ))
    }

    fn heavily_dug_world_with(settings: &WorldSettings) -> WorldState {
        use fatherload_simulation::{
            ControlState, PlayerCommand, PlayerTickInput, TickAction, TickInputs,
            terrain::TilePosition,
        };
        let id = PlayerId::new(1);
        let mut world =
            WorldState::new_game_with_settings(id, settings, SimulationParams::default());
        let rig = world.player_mut(id).expect("rig");
        rig.credits = 5_000;
        rig.bombs = 40;
        rig.survey_drone_kits = 4;
        rig.tunnel_support_kits = 4;
        rig.signal_relay_kits = 4;
        for tick in 0..4_000_u64 {
            let (horizontal, thrust, drill) = match tick % 600 {
                0..200 => (0.0, false, true),
                200..320 => (1.0, false, false),
                320..420 => (-0.6, true, false),
                420..520 => (-1.0, false, true),
                _ => (0.25, true, false),
            };
            let mut commands = Vec::new();
            if tick % 97 == 0 {
                commands.push(PlayerCommand::PlaceBomb);
            }
            if tick % 131 == 0 {
                commands.push(PlayerCommand::UseScanner);
            }
            if tick % 409 == 0 {
                commands.push(PlayerCommand::PlaceInfrastructure {
                    slot: u8::try_from(tick % 6).expect("small"),
                });
            }
            let actions = (0_u32..)
                .zip(commands)
                .map(|(sequence, command)| TickAction {
                    sequence: fatherload_simulation::InputSequence::new(sequence),
                    command,
                })
                .collect();
            let mut inputs = TickInputs::default();
            inputs.players.insert(
                id,
                PlayerTickInput {
                    controls: Some(ControlState::new(horizontal, thrust, drill)),
                    actions,
                },
            );
            let _output = world.simulate_tick(SimulationTick::new(tick), &inputs);
        }
        let terrain = world.terrain_mut();
        let (width, height) = (terrain.width().min(240), terrain.height());
        for step in 0..400_i32 {
            let x = (step * 37).rem_euclid(width);
            let y = 6 + (step * 53).rem_euclid(height - 6);
            let _ = terrain.blast_radius(TilePosition { x, y }, 3);
            for dx in 0..10 {
                let _ = terrain.chip(TilePosition {
                    x: x + dx,
                    y: y + 4,
                });
            }
        }
        world
    }

    /// A streamed (V5) world saves only its settings and the edited chunks' diffs, loads back
    /// identical, and stays small however far apart the digging was.
    #[test]
    fn streamed_worlds_save_settings_and_edited_chunks_only() {
        use fatherload_simulation::terrain::{TileKind, TilePosition};
        let mut world = heavily_dug_world_with(&WorldSettings::new(0x0005_5AFE));
        assert!(world.terrain().is_streamed());
        for x in [-50_000, -1_234, 7_777, 50_000] {
            for dx in 0..20 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x: x + dx, y: 300 }, TileKind::Air);
            }
        }
        let edited_chunks = world.terrain().edited_chunk_count();
        let save = WorldSave {
            world,
            play_seconds: 12.0,
        };
        let json = v6_json(&save);
        let loaded = parse_save(&json).expect("v6 streamed save loads");
        assert_eq!(
            loaded.world.simulation_state_hash(),
            save.world.simulation_state_hash()
        );
        assert_eq!(loaded.world.terrain().edited_chunk_count(), edited_chunks);
        assert_eq!(
            loaded.world.terrain().generation_diffs(),
            save.world.terrain().generation_diffs()
        );
        for x in [-50_000, 50_019, 3] {
            let position = TilePosition { x, y: 300 };
            assert_eq!(
                loaded.world.terrain().tile(position),
                save.world.terrain().tile(position)
            );
        }
        // Settings plus edits: far smaller than one bounded world's tiles.
        assert!(
            json.len() < 400_000,
            "streamed save is {} bytes",
            json.len()
        );
    }

    #[test]
    fn heavily_dug_worlds_round_trip_and_v5_saves_migrate_to_v6() {
        let world = heavily_dug_world();
        assert!(!world.infrastructure().is_empty(), "infrastructure placed");
        let diffs = world.terrain().generation_diffs().len();
        assert!(diffs > 5_000, "heavily dug: {diffs} tiles differ");
        let save = WorldSave {
            world,
            play_seconds: 99.0,
        };

        // Current format: complete state hash equality.
        let loaded = parse_save(&v6_json(&save)).expect("v6 loads");
        assert_eq!(
            loaded.world.simulation_state_hash(),
            save.world.simulation_state_hash()
        );
        assert_eq!(
            loaded.world.terrain().bounded_tiles(),
            save.world.terrain().bounded_tiles()
        );

        // A v5 save of the same world (full tiles) migrates to the same world, and re-saving
        // writes v6.
        let v5 = v5_json(&save).to_string();
        let migrated = parse_save(&v5).expect("v5 migrates");
        assert_eq!(
            migrated.world.simulation_state_hash(),
            save.world.simulation_state_hash()
        );
        let path = std::env::temp_dir().join(format!(
            "fatherload-save-v5-migration-{}.json",
            std::process::id()
        ));
        save_world_to_path(&migrated, &path).expect("write");
        let written = fs::read_to_string(&path).expect("read back");
        let _ignored = fs::remove_file(&path);
        let probe: VersionProbe = serde_json::from_str(&written).expect("probe");
        assert_eq!(probe.version, SAVE_VERSION);
        assert!(written.len() * 5 < v5.len(), "v6 is much smaller than v5");
        let reloaded = parse_save(&written).expect("re-saved v6 loads");
        assert_eq!(
            reloaded.world.simulation_state_hash(),
            save.world.simulation_state_hash()
        );
    }

    #[test]
    fn legacy_fixture_worlds_round_trip_through_the_compact_form() {
        let v3: serde_json::Value = serde_json::from_str(V3).expect("v3 json");
        let v4 = serde_json::json!({
            "version": 4,
            "save": { "world": v3["world"]["canonical_world"].clone(), "play_seconds": 1.0 },
        })
        .to_string();
        for (version, json) in [(1, V1), (2, V2), (3, V3), (4, v4.as_str())] {
            let save = parse_save(json).unwrap_or_else(|error| panic!("v{version}: {error}"));
            let reloaded = parse_save(&v6_json(&save)).expect("re-saved loads");
            assert_eq!(
                reloaded.world.simulation_state_hash(),
                save.world.simulation_state_hash(),
                "v{version}"
            );
            // The same world written as v5 (full tiles) migrates identically.
            let migrated = parse_save(&v5_json(&save).to_string()).expect("as v5");
            assert_eq!(
                migrated.world.simulation_state_hash(),
                save.world.simulation_state_hash(),
                "v{version} as v5"
            );
        }
    }

    #[test]
    fn saves_from_unknown_generators_or_with_corrupted_terrain_are_rejected() {
        let save = WorldSave {
            world: heavily_dug_world(),
            play_seconds: 0.0,
        };
        let mut v6: serde_json::Value = serde_json::from_str(&v6_json(&save)).expect("json");
        v6["save"]["world"]["terrain"]["settings"]["generator"] =
            serde_json::Value::String("V99".to_owned());
        let error = parse_save(&v6.to_string()).expect_err("unknown generator");
        assert!(
            matches!(&error, SaveError::UnknownGenerator(name) if name == "V99"),
            "{error}"
        );
        assert!(error.to_string().contains("unknown world generator"));

        let mut v5 = v5_json(&save);
        v5["save"]["world"]["terrain"]["settings"]["generator"] = serde_json::Value::from(3);
        assert!(matches!(
            parse_save(&v5.to_string()),
            Err(SaveError::UnknownGenerator(_))
        ));

        // A save whose diffs do not rebuild its terrain (corrupted, or written by a different
        // build of the same generator version) fails the digest check with an error.
        let mut corrupted: serde_json::Value = serde_json::from_str(&v6_json(&save)).expect("json");
        corrupted["save"]["world"]["terrain"]["digest"] = serde_json::Value::from(12_345);
        let error = parse_save(&corrupted.to_string()).expect_err("digest mismatch");
        assert!(
            error.to_string().contains("does not match world generator"),
            "{error}"
        );
        let mut truncated: serde_json::Value = serde_json::from_str(&v6_json(&save)).expect("json");
        truncated["save"]["world"]["terrain"]["diffs"] = serde_json::json!([]);
        assert!(parse_save(&truncated.to_string()).is_err());

        // A v5 save whose full tiles do not fill its terrain fails cleanly too.
        let mut short_v5 = v5_json(&save);
        short_v5["save"]["world"]["terrain"]["tiles"]
            .as_array_mut()
            .expect("tiles")
            .truncate(10);
        let error = parse_save(&short_v5.to_string()).expect_err("short v5 tiles");
        assert!(
            error.to_string().contains("tiles but its size needs"),
            "{error}"
        );
    }

    /// Save file sizes, v5 (full tiles) against v6 (diffs from generation).
    #[test]
    fn save_size_report() {
        let fresh = WorldSave {
            world: WorldState::new_game_with_settings(
                PlayerId::new(1),
                &WorldSettings::bounded(fatherload_simulation::worldgen::DEFAULT_WORLD_SEED),
                SimulationParams::default(),
            ),
            play_seconds: 0.0,
        };
        let dug = WorldSave {
            world: heavily_dug_world(),
            play_seconds: 0.0,
        };
        for (label, save) in [("fresh", &fresh), ("heavily dug", &dug)] {
            let v5 = v5_json(save).to_string().len();
            let v6 = v6_json(save).len();
            println!(
                "save size {label}: v5 {v5} bytes, v6 {v6} bytes ({} tile diffs)",
                save.world.terrain().generation_diffs().len()
            );
            assert!(v6 * 5 < v5, "{label}: v6 {v6} vs v5 {v5}");
        }
    }

    #[test]
    fn enemies_and_the_enemy_setting_save_and_version_seven_files_still_load() {
        use fatherload_simulation::{EnemyDifficulty, EnemyKind, entity::EntityKind};
        let mut world = WorldState::new_game(PlayerId::new(1));
        let plain = WorldSave {
            world: world.clone(),
            play_seconds: 3.0,
        };
        world.set_enemy_difficulty(EnemyDifficulty::Hard);
        let _bat = world.spawn_enemy(EnemyKind::CaveBat, 3_000.0, 6_000.0);
        let _turret = world.spawn_enemy(EnemyKind::SentryTurret, 3_100.0, 6_000.0);
        let save = WorldSave {
            world,
            play_seconds: 3.0,
        };
        let json = serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            save: save.clone(),
        })
        .expect("serialize");
        let loaded = parse_save(&json).expect("v8 loads");
        assert_eq!(loaded.world.enemy_difficulty(), EnemyDifficulty::Hard);
        assert_eq!(loaded.world.entities().count_of(EntityKind::Enemy), 2);
        assert_eq!(
            loaded.world.simulation_state_hash(),
            save.world.simulation_state_hash()
        );
        // A version 7 file (no enemies, no director) loads as a normal-difficulty world.
        let mut v7 = serde_json::to_value(&SaveFile {
            version: 7,
            save: plain.clone(),
        })
        .expect("serialize");
        let _director = v7["save"]["world"]
            .as_object_mut()
            .expect("world object")
            .remove("enemies")
            .expect("the director is saved");
        let loaded = parse_save(&v7.to_string()).expect("v7 loads");
        assert_eq!(loaded.world.enemy_difficulty(), EnemyDifficulty::Normal);
        assert_eq!(
            loaded.world.simulation_state_hash(),
            plain.world.simulation_state_hash()
        );
    }

    #[test]
    fn the_world_clock_saves_and_version_eight_files_get_the_standard_day() {
        use fatherload_simulation::clock::{DayLength, DayPhase};
        let mut world = WorldState::new_game(PlayerId::new(1));
        let plain = WorldSave {
            world: world.clone(),
            play_seconds: 1.0,
        };
        world.set_day_length(DayLength::Short);
        world.set_world_time_millis(DayLength::Short.millis() * 2 + 1_000);
        let json = serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            save: WorldSave {
                world: world.clone(),
                play_seconds: 1.0,
            },
        })
        .expect("serialize");
        let loaded = parse_save(&json).expect("v9 loads");
        assert_eq!(loaded.world.day_length(), DayLength::Short);
        assert_eq!(loaded.world.clock(), world.clock());
        assert_eq!(loaded.world.clock().day(), 3);
        // A version 8 file has no clock: it loads on the standard day.
        let mut v8 = serde_json::to_value(&SaveFile {
            version: 8,
            save: plain.clone(),
        })
        .expect("serialize");
        let _clock = v8["save"]["world"]
            .as_object_mut()
            .expect("world object")
            .remove("clock")
            .expect("the clock is saved");
        let loaded = parse_save(&v8.to_string()).expect("v8 loads");
        assert_eq!(loaded.world.day_length(), DayLength::Standard);
        assert_eq!(loaded.world.clock().phase(), DayPhase::Day);
        assert_eq!(
            loaded.world.simulation_state_hash(),
            plain.world.simulation_state_hash()
        );
    }

    #[test]
    fn milestones_save_and_version_eight_files_load_without_them() {
        use fatherload_simulation::milestone::Milestone;
        /// Removes the milestone logs, as a version 8 game wrote the file.
        fn strip(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    map.remove("milestones");
                    map.values_mut().for_each(strip);
                }
                serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
                _ => {}
            }
        }
        let player = PlayerId::new(1);
        let mut world = WorldState::new_game(player);
        {
            let rig = world.player_mut(player).expect("rig");
            rig.milestones.primed = true;
            rig.earn_milestone(Milestone::FirstDeepLayer, 40);
            rig.milestones.records.deepest_row = 321;
        }
        let save = WorldSave {
            world,
            play_seconds: 3.0,
        };
        let text = serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            save: save.clone(),
        })
        .expect("json");
        let loaded = parse_save_file(&text).expect("current save loads");
        assert_eq!(
            loaded.world.player(player).expect("rig").milestones,
            save.world.player(player).expect("rig").milestones
        );
        assert_eq!(
            loaded.world.simulation_state_hash(),
            save.world.simulation_state_hash()
        );
        // A version 8 file has no milestone log: it loads with an unprimed one (the first
        // tick then records what the rig already met, silently).
        let mut json: serde_json::Value = serde_json::from_str(&text).expect("value");
        json["version"] = 8.into();
        strip(&mut json);
        let old = parse_save_file(&json.to_string()).expect("a v8 save loads");
        let rig = old.world.player(player).expect("rig");
        assert!(rig.milestones.earned.is_empty() && !rig.milestones.primed);
        assert!(
            rig.cosmetic_unlocks
                .has(Milestone::FirstDeepLayer.rewards()[0]),
            "unlocks already earned stay"
        );
    }

    #[test]
    fn night_monsters_and_the_night_ledger_save_and_version_nine_files_load() {
        use fatherload_simulation::{EnemyKind, entity::EntityKind};
        let mut world = WorldState::new_game(PlayerId::new(1));
        let plain = WorldSave {
            world: world.clone(),
            play_seconds: 2.0,
        };
        let _hound = world.spawn_enemy(EnemyKind::ShadowHound, 3_000.0, 60.0);
        let _moths = world.spawn_enemy(EnemyKind::MothSwarm, 3_100.0, 20.0);
        let save = WorldSave {
            world,
            play_seconds: 2.0,
        };
        let json = serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            save: save.clone(),
        })
        .expect("serialize");
        let loaded = parse_save(&json).expect("v10 loads");
        assert_eq!(loaded.world.entities().count_of(EntityKind::Enemy), 2);
        assert_eq!(loaded.world.night_ledger(), save.world.night_ledger());
        assert_eq!(
            loaded.world.simulation_state_hash(),
            save.world.simulation_state_hash()
        );
        // A version 9 file (no night ledger in the director) loads as is.
        let mut v9 = serde_json::to_value(&SaveFile {
            version: 9,
            save: plain.clone(),
        })
        .expect("serialize");
        if let Some(director) = v9["save"]["world"]["enemies"].as_object_mut() {
            director.remove("night");
        }
        let loaded = parse_save(&v9.to_string()).expect("v9 loads");
        assert_eq!(loaded.world.night_ledger().nights_survived, 0);
        assert_eq!(
            loaded.world.simulation_state_hash(),
            plain.world.simulation_state_hash()
        );
    }

    #[test]
    fn unknown_versions_are_rejected() {
        assert!(matches!(
            parse_save(r#"{"version": 99}"#),
            Err(SaveError::UnsupportedVersion(99))
        ));
    }
}
