use crate::jsonfile;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const SETTINGS_FILE: &str = "settings.json";
const MIN_NEW_HOURS: f64 = 0.1;
const MAX_NEW_HOURS: f64 = 1000.0;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AfterLaunch {
    #[default]
    Close,
    StayOpen,
}

/// User settings. Missing fields take their defaults so old or partial files still load.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Manual Steam folder. `None` = auto-detect.
    pub steam_path: Option<String>,
    /// Extra library folders to scan in addition to the ones Steam knows about.
    pub extra_libraries: Vec<String>,
    /// Chosen account (SteamID64). `None` = most recently logged-in account.
    pub steam_id: Option<String>,
    /// "Something New" = played less than this many hours.
    pub something_new_hours: f64,
    pub after_launch: AfterLaunch,
    /// Main-screen toggle. `None` = default (off with an API key, forced on without).
    pub installed_only: Option<bool>,
    /// App IDs never to pick.
    pub blacklist: Vec<u32>,
    /// The "add an API key" banner was dismissed.
    pub hide_api_banner: bool,
    /// Never pick games hidden in Steam's library.
    pub skip_steam_hidden: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            steam_path: None,
            extra_libraries: Vec::new(),
            steam_id: None,
            something_new_hours: 2.0,
            after_launch: AfterLaunch::Close,
            installed_only: None,
            blacklist: Vec::new(),
            hide_api_banner: false,
            skip_steam_hidden: true,
        }
    }
}

impl Settings {
    /// Clamps and tidies values coming from the UI or a hand-edited file.
    pub fn normalized(mut self) -> Self {
        if !self.something_new_hours.is_finite() {
            self.something_new_hours = Settings::default().something_new_hours;
        }
        self.something_new_hours = self.something_new_hours.clamp(MIN_NEW_HOURS, MAX_NEW_HOURS);
        let blank = |s: &Option<String>| s.as_deref().is_none_or(|v| v.trim().is_empty());
        if blank(&self.steam_path) {
            self.steam_path = None;
        }
        if blank(&self.steam_id) {
            self.steam_id = None;
        }
        self.extra_libraries = self
            .extra_libraries
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self.extra_libraries.dedup();
        self.blacklist.sort_unstable();
        self.blacklist.dedup();
        self
    }
}

const CONFIG_DIR_NAME: &str = "IDFKPlay";
/// Folder used by builds before the rename; moved to `CONFIG_DIR_NAME` on first run.
const LEGACY_CONFIG_DIR_NAME: &str = "gamepicker";

/// `%APPDATA%\IDFKPlay`, falling back to `fallback` (Tauri's config dir) if APPDATA is unset.
pub fn config_dir(fallback: PathBuf) -> PathBuf {
    std::env::var_os("APPDATA")
        .map(|appdata| {
            let appdata = PathBuf::from(appdata);
            let dir = appdata.join(CONFIG_DIR_NAME);
            migrate_legacy_dir(&appdata.join(LEGACY_CONFIG_DIR_NAME), &dir);
            dir
        })
        .unwrap_or(fallback)
}

/// Moves settings and caches from the pre-rename folder, once. Failures just mean a fresh start.
pub fn migrate_legacy_dir(legacy: &Path, dir: &Path) {
    if dir.exists() || !legacy.is_dir() {
        return;
    }
    if let Err(e) = std::fs::rename(legacy, dir) {
        eprintln!("couldn't move {} to {}: {e}", legacy.display(), dir.display());
    }
}

/// Loads settings; a missing or corrupt file yields defaults rather than an error.
pub fn load(dir: &Path) -> Settings {
    jsonfile::read::<Settings>(&dir.join(SETTINGS_FILE)).unwrap_or_default().normalized()
}

pub fn save(dir: &Path, settings: &Settings) -> std::io::Result<()> {
    jsonfile::write(&dir.join(SETTINGS_FILE), settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_keeps_defaults() {
        let s: Settings = serde_json::from_str(r#"{"blacklist":[5,3,5]}"#).unwrap();
        let s = s.normalized();
        assert_eq!(s.something_new_hours, 2.0);
        assert_eq!(s.after_launch, AfterLaunch::Close);
        assert_eq!(s.blacklist, [3, 5]);
    }

    #[test]
    fn migrates_legacy_folder_once() {
        let root = std::env::temp_dir().join(format!("idfkplay-migrate-{}", std::process::id()));
        let legacy = root.join("gamepicker");
        let dir = root.join("IDFKPlay");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("settings.json"), "{}").unwrap();

        migrate_legacy_dir(&legacy, &dir);
        assert!(dir.join("settings.json").is_file());
        assert!(!legacy.exists());

        // A legacy folder appearing later never overwrites the current one.
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("settings.json"), "old").unwrap();
        migrate_legacy_dir(&legacy, &dir);
        assert_eq!(std::fs::read_to_string(dir.join("settings.json")).unwrap(), "{}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn normalizes_bad_values() {
        let s = Settings {
            something_new_hours: -4.0,
            steam_path: Some("  ".into()),
            extra_libraries: vec![" D:\\Games ".into(), "".into()],
            ..Default::default()
        }
        .normalized();
        assert_eq!(s.something_new_hours, MIN_NEW_HOURS);
        assert_eq!(s.steam_path, None);
        assert_eq!(s.extra_libraries, ["D:\\Games"]);
    }
}
