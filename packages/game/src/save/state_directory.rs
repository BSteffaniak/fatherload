//! Non-destructive import of the former Drillgame state directory.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

static STATE_DIR: OnceLock<PathBuf> = OnceLock::new();
static IMPORT_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn default_dir() -> PathBuf {
    STATE_DIR
        .get_or_init(|| {
            let lookup = |key: &str| env::var_os(key).map(PathBuf::from);
            if let Some(path) = override_dir(&lookup) {
                return path;
            }
            let root = default_root(&lookup, cfg!(windows));
            let destination = root.join("fatherload");
            let legacy = root.join("drillgame");
            if let Err(error) = import_legacy(&legacy, &destination) {
                // Keep one consistent location for this process; never split saves across roots
                // after a partial import. The old files are still intact.
                eprintln!(
                    "Fatherload state import failed: {error}; continuing in {}",
                    legacy.display()
                );
                return legacy;
            }
            destination
        })
        .clone()
}

fn override_dir(lookup: &impl Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    lookup("FATHERLOAD_STATE_DIR").or_else(|| lookup("DRILLGAME_STATE_DIR"))
}

fn default_root(lookup: &impl Fn(&str) -> Option<PathBuf>, windows: bool) -> PathBuf {
    lookup("XDG_STATE_HOME")
        .or_else(|| windows.then(|| lookup("LOCALAPPDATA")).flatten())
        .or_else(|| lookup("HOME").map(|home| home.join(".local/state")))
        .unwrap_or_else(env::temp_dir)
}

fn import_legacy(legacy: &Path, destination: &Path) -> io::Result<()> {
    if destination.try_exists()? || !legacy.try_exists()? {
        return Ok(());
    }
    let root = destination
        .parent()
        .ok_or_else(|| io::Error::other("missing state parent"))?;
    // Serialize concurrent first launches. OS file locks release even if a process crashes.
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".fatherload-import.lock"))?;
    lock.lock()?;
    if destination.try_exists()? {
        return Ok(());
    }
    // Each attempt gets a fresh staging directory. An interrupted copy is never mistaken
    // for a completed import, and leftover staging directories are not deleted blindly.
    let stage = loop {
        let id = IMPORT_ID.fetch_add(1, Ordering::Relaxed);
        let stage = root.join(format!(".fatherload-import-{}-{id}", std::process::id()));
        match fs::create_dir(&stage) {
            Ok(()) => break stage,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    };
    for name in std::iter::once(super::SETTINGS_FILE_NAME.to_owned())
        .chain(std::iter::once(super::SAVE_FILE_NAME.to_owned()))
        .chain((1..=super::SAVE_SLOTS).map(|slot| format!("save-slot-{slot}.json")))
    {
        let source = legacy.join(&name);
        if source.try_exists()? {
            fs::copy(source, stage.join(name))?;
        }
    }
    if destination.try_exists()? {
        return Ok(());
    }
    fs::rename(stage, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let id = IMPORT_ID.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "fatherload-import-test-{}-{id}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("test root");
            Self(path)
        }
        fn legacy(&self) -> PathBuf {
            self.0.join("drillgame")
        }
        fn destination(&self) -> PathBuf {
            self.0.join("fatherload")
        }
        fn populate(&self) {
            fs::create_dir(self.legacy()).expect("legacy root");
            for name in [
                "save.json",
                "settings.json",
                "save-slot-1.json",
                "save-slot-2.json",
                "save-slot-3.json",
            ] {
                fs::write(self.legacy().join(name), name).expect("legacy file");
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn imports_all_known_files_without_modifying_legacy() {
        let fixture = Fixture::new();
        fixture.populate();
        import_legacy(&fixture.legacy(), &fixture.destination()).expect("import");
        for entry in fs::read_dir(fixture.legacy()).expect("files") {
            let entry = entry.expect("entry");
            assert_eq!(
                fs::read(entry.path()).unwrap(),
                fs::read(fixture.destination().join(entry.file_name())).unwrap()
            );
        }
    }

    #[test]
    fn existing_destination_wins_even_if_empty() {
        let fixture = Fixture::new();
        fixture.populate();
        fs::create_dir(fixture.destination()).unwrap();
        import_legacy(&fixture.legacy(), &fixture.destination()).unwrap();
        assert!(!fixture.destination().join("save.json").exists());
        fs::write(fixture.destination().join("save.json"), "new save").unwrap();
        import_legacy(&fixture.legacy(), &fixture.destination()).unwrap();
        assert_eq!(
            fs::read_to_string(fixture.destination().join("save.json")).unwrap(),
            "new save"
        );
    }

    #[test]
    fn fresh_install_does_not_create_state_until_needed() {
        let fixture = Fixture::new();
        import_legacy(&fixture.legacy(), &fixture.destination()).unwrap();
        assert!(!fixture.destination().exists());
    }

    #[test]
    fn interrupted_staging_is_not_published_or_reused() {
        let fixture = Fixture::new();
        fixture.populate();
        let stage = fixture.0.join(".fatherload-import-interrupted");
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("save.json"), "partial").unwrap();
        import_legacy(&fixture.legacy(), &fixture.destination()).unwrap();
        assert_eq!(
            fs::read_to_string(fixture.destination().join("save.json")).unwrap(),
            "save.json"
        );
        assert!(stage.exists());
    }

    #[test]
    fn failed_copy_never_publishes_partial_state_and_can_retry() {
        let fixture = Fixture::new();
        fixture.populate();
        fs::remove_file(fixture.legacy().join("save-slot-3.json")).unwrap();
        fs::create_dir(fixture.legacy().join("save-slot-3.json")).unwrap();
        assert!(import_legacy(&fixture.legacy(), &fixture.destination()).is_err());
        assert!(!fixture.destination().exists());
        fs::remove_dir(fixture.legacy().join("save-slot-3.json")).unwrap();
        import_legacy(&fixture.legacy(), &fixture.destination()).unwrap();
        assert!(fixture.destination().join("save.json").exists());
    }

    #[test]
    fn explicit_overrides_prefer_new_name_and_skip_default_roots() {
        let lookup = |key: &str| match key {
            "FATHERLOAD_STATE_DIR" => Some(PathBuf::from("new-custom")),
            "DRILLGAME_STATE_DIR" => Some(PathBuf::from("old-custom")),
            _ => None,
        };
        assert_eq!(override_dir(&lookup), Some(PathBuf::from("new-custom")));
        assert_eq!(
            override_dir(&|key| (key == "DRILLGAME_STATE_DIR").then(|| PathBuf::from("old-custom"))),
            Some(PathBuf::from("old-custom"))
        );
        assert_eq!(
            default_root(
                &|key| (key == "LOCALAPPDATA").then(|| PathBuf::from("windows")),
                true
            ),
            PathBuf::from("windows")
        );
        assert_eq!(
            default_root(&|key| (key == "HOME").then(|| PathBuf::from("home")), false),
            PathBuf::from("home/.local/state")
        );
    }
}
