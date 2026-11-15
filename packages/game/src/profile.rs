//! The install's persistent player profile (WP-PERSIST): a random [`ProfileId`] generated once
//! per install plus a display name.
//!
//! It lives in `profile.json` in the state directory (desktop: next to settings and saves; in
//! the browser the state directory is the `IndexedDB`-backed `/fatherload` mount, and writing it
//! queues the same `IndexedDB` flush the saves use). The id is sent in `Hello` when joining, so a
//! host whose world saved this player's rig hands it back, and it names this install's local
//! seats in the worlds it hosts (seat `n` uses [`ProfileId::for_seat`]). The name can be edited
//! in the file; `FATHERLOAD_PLAYER_NAME` overrides it for one run.

use std::{fs, path::Path};

use fatherload_simulation::ProfileId;
use serde::{Deserialize, Serialize};

use crate::save::{self, SaveError};

/// File name of the profile in the state directory.
pub const PROFILE_FILE_NAME: &str = "profile.json";
/// Longest display name kept.
const MAX_NAME_CHARS: usize = 24;

/// This install's identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlayerProfile {
    pub id: ProfileId,
    pub name: String,
}

impl PlayerProfile {
    /// The netcode form of the id (sent in `Hello`).
    #[must_use]
    pub const fn net_id(&self) -> fatherload_netcode::ProfileId {
        fatherload_netcode::world::from_sim_profile(self.id)
    }

    /// A fixed profile for tests and scripted scenes (never written anywhere).
    #[cfg(test)]
    #[must_use]
    pub fn fixed(id: u64, name: &str) -> Self {
        Self {
            id: ProfileId::new(id),
            name: name.to_owned(),
        }
    }
}

/// Loads the install's profile, creating (and persisting) it on first run. A storage failure
/// never blocks play: the profile is then used for this run only and a later run retries.
#[must_use]
pub fn load_or_create() -> PlayerProfile {
    let path = save::state_file_path(PROFILE_FILE_NAME);
    let (mut profile, result) =
        load_or_create_at(&path, fresh_profile_id, crate::online::player_name);
    if let Err(error) = result {
        eprintln!("Could not save the player profile ({error}); using it for this run only");
    }
    if let Some(name) = name_override() {
        profile.name = name;
    }
    profile
}

/// Reads the profile at `path`, or makes one from `fresh_id` and `default_name` and writes it
/// there (an unreadable or corrupted file is replaced). Returns the profile and the result of
/// writing it (`Ok` when nothing needed writing).
pub fn load_or_create_at(
    path: &Path,
    fresh_id: impl FnOnce() -> u64,
    default_name: impl FnOnce() -> String,
) -> (PlayerProfile, Result<(), SaveError>) {
    if let Some(profile) = fs::read_to_string(path)
        .ok()
        .and_then(|json| serde_json::from_str::<PlayerProfile>(&json).ok())
    {
        let name = clean_name(&profile.name);
        if name == profile.name {
            return (profile, Ok(()));
        }
        let profile = PlayerProfile { name, ..profile };
        let result = write_profile(path, &profile);
        return (profile, result);
    }
    let profile = PlayerProfile {
        id: ProfileId::new(fresh_id()),
        name: clean_name(&default_name()),
    };
    let result = write_profile(path, &profile);
    (profile, result)
}

fn write_profile(path: &Path, profile: &PlayerProfile) -> Result<(), SaveError> {
    let json = serde_json::to_string_pretty(profile).map_err(SaveError::Serialize)?;
    save::write_state_file(path, json)
}

/// A random profile id from OS randomness (falls back like the reconnect-token seed).
fn fresh_profile_id() -> u64 {
    crate::online::random_token_seed()
}

fn name_override() -> Option<String> {
    std::env::var("FATHERLOAD_PLAYER_NAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .map(|name| clean_name(&name))
}

const DEFAULT_NAME: &str = "Miner";

/// A display name trimmed to one line of at most [`MAX_NAME_CHARS`] printable characters.
fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_NAME_CHARS)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        DEFAULT_NAME.to_owned()
    } else {
        cleaned.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fatherload-profile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_first_run_creates_a_profile_and_later_runs_keep_it() {
        let dir = temp_dir("first-run");
        let path = dir.join(PROFILE_FILE_NAME);
        let (created, result) = load_or_create_at(&path, || 0xdead_beef, || "Alex".to_owned());
        assert!(result.is_ok());
        assert_eq!(created, PlayerProfile::fixed(0xdead_beef, "Alex"));
        assert!(path.exists(), "written on first run");
        let (again, result) = load_or_create_at(
            &path,
            || panic!("an existing profile is never regenerated"),
            || "Someone else".to_owned(),
        );
        assert!(result.is_ok());
        assert_eq!(again, created);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_corrupted_profile_is_replaced_and_names_are_cleaned() {
        let dir = temp_dir("corrupt");
        let path = dir.join(PROFILE_FILE_NAME);
        fs::create_dir_all(&dir).expect("dir");
        fs::write(&path, "{ not json").expect("write");
        let (profile, result) = load_or_create_at(&path, || 7, || "  \tSam\n ".to_owned());
        assert!(result.is_ok());
        assert_eq!(profile, PlayerProfile::fixed(7, "Sam"));
        let (long, _) = load_or_create_at(&dir.join("other.json"), || 8, || "x".repeat(80));
        assert_eq!(long.name.chars().count(), MAX_NAME_CHARS);
        let (blank, _) = load_or_create_at(&dir.join("blank.json"), || 9, String::new);
        assert_eq!(blank.name, DEFAULT_NAME);
        let _ = fs::remove_dir_all(dir);
    }

    /// The browser path: the state directory is the `IndexedDB`-backed mount
    /// (`FATHERLOAD_STATE_DIR=/fatherload`, see `web/bootstrap.js`) and the profile is a file in
    /// it, written through the same `write_state_file` that queues the `IndexedDB` flush for
    /// saves. A cleared site (empty mount) makes a new profile; a restored mount keeps the id.
    #[test]
    fn the_browser_profile_lives_in_the_persisted_state_directory() {
        let mount = temp_dir("idbfs-mount");
        let path = mount.join(PROFILE_FILE_NAME);
        assert_eq!(
            save::state_file_path_in(&mount, PROFILE_FILE_NAME),
            path,
            "the profile sits beside the saves in the persisted mount"
        );
        let (first, result) = load_or_create_at(&path, || 0x1111, || "Browser".to_owned());
        assert!(result.is_ok());
        // The tab closes and IndexedDB restores the mount: same identity.
        let restored = fs::read_to_string(&path).expect("persisted");
        assert!(restored.contains("0000000000001111"), "{restored}");
        let (second, _) = load_or_create_at(&path, || 0x2222, || "Browser".to_owned());
        assert_eq!(second.id, first.id);
        // Site data cleared: a brand-new identity (the host offers "claim a saved rig").
        fs::remove_dir_all(&mount).expect("clear");
        let (fresh, _) = load_or_create_at(&path, || 0x2222, || "Browser".to_owned());
        assert_ne!(fresh.id, first.id);
        let _ = fs::remove_dir_all(mount);
    }
}
