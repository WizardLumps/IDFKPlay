pub mod accounts;
pub mod art;
pub mod collections;
pub mod library;
pub mod locate;
pub mod playtime;
pub mod store;
pub mod storebrowse;
pub mod webapi;

use crate::error::AppError;
use crate::settings::Settings;
use crate::source::{Game, GameSource, SourceScan};
use accounts::SteamAccount;
use library::LibraryInfo;
use locate::PathSource;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;

pub const SOURCE_ID: &str = "steam";

/// What a scan learned about the Steam install itself (shown in Settings/debug UI).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamInfo {
    pub steam_path: String,
    pub steam_path_source: PathSource,
    pub libraries: Vec<LibraryInfo>,
    pub accounts: Vec<SteamAccount>,
    /// SteamID64 of the account playtime was read from.
    pub selected_account: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SteamScan {
    pub info: SteamInfo,
    /// Games with a local manifest, with local playtime.
    pub games: Vec<Game>,
    /// Steam library collections of the selected account.
    pub collections: collections::Collections,
    pub warnings: Vec<String>,
}

pub struct SteamSource {
    pub settings: Settings,
}

impl SteamSource {
    pub fn new(settings: Settings) -> Self {
        SteamSource { settings }
    }

    pub fn scan_detailed(&self) -> Result<SteamScan, AppError> {
        let located = locate::resolve(self.settings.steam_path.as_deref())?;
        let steam = located.path;
        let mut warnings: Vec<String> = located.warning.into_iter().collect();

        // Installed games from every library.
        let mut games: HashMap<u32, Game> = HashMap::new();
        let mut libraries = Vec::new();
        for (lib, origin) in library::collect_libraries(&steam, &self.settings.extra_libraries, &mut warnings) {
            let manifests = match library::scan_library(&lib, &mut warnings) {
                Ok(m) => Some(m),
                Err(_) => {
                    let msg = if origin == library::LibraryOrigin::Settings {
                        format!("The extra library folder {} isn't reachable.", lib.display())
                    } else {
                        format!("Steam library {} isn't reachable (drive unplugged?).", lib.display())
                    };
                    warnings.push(msg);
                    None
                }
            };
            libraries.push(LibraryInfo {
                path: lib.display().to_string(),
                origin,
                reachable: manifests.is_some(),
                game_count: manifests.as_ref().map_or(0, Vec::len),
            });
            for m in manifests.into_iter().flatten() {
                let installed = m.is_installed();
                // The same app can appear in two libraries (e.g. a leftover); prefer the installed copy.
                if games.get(&m.app_id).is_some_and(|g| g.installed || !installed) {
                    continue;
                }
                let install_path = m
                    .install_dir
                    .as_ref()
                    .map(|d| lib.join("steamapps").join("common").join(d).display().to_string());
                games.insert(
                    m.app_id,
                    Game {
                        source: SOURCE_ID,
                        app_id: m.app_id,
                        name: m.name,
                        installed,
                        install_path,
                        size_on_disk: m.size_on_disk.filter(|&s| s > 0),
                        playtime_minutes: 0,
                        last_played: None,
                    },
                );
            }
        }
        if !libraries.iter().any(|l| l.reachable) {
            return Err(AppError::NoLibraries(steam));
        }

        // Accounts.
        let accounts = match fs::read_to_string(steam.join("config").join("loginusers.vdf")) {
            Ok(text) => accounts::parse_login_users(&text).unwrap_or_else(|e| {
                warnings.push(format!("Couldn't read Steam's account list ({e})."));
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        let preferred = self.settings.steam_id.as_deref();
        let selected = accounts::pick_account(&accounts, preferred);
        match (selected, preferred) {
            (None, _) => warnings.push(
                "No Steam account found on this PC, so playtime is unknown. Log in to Steam once and refresh.".into(),
            ),
            (Some(a), Some(p)) if a.steam_id != p => warnings.push(format!(
                "The account chosen in Settings isn't on this PC anymore. Using {} instead.",
                a.persona_name
            )),
            _ => {}
        }

        // Local playtime for the selected account.
        if let Some(account) = selected {
            let path = playtime::localconfig_path(&steam, account.account_id);
            match fs::read_to_string(&path) {
                Ok(text) => match playtime::parse_local_config(&text) {
                    Ok(stats) => {
                        for (id, s) in stats {
                            if let Some(g) = games.get_mut(&id) {
                                g.playtime_minutes = s.playtime_minutes;
                                g.last_played = s.last_played;
                            }
                        }
                    }
                    Err(e) => warnings.push(format!("Couldn't read local playtime ({e}). Playtimes show as 0.")),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => warnings.push(format!(
                    "No local playtime data for {} yet. Playtimes show as 0.",
                    account.persona_name
                )),
                Err(e) => warnings.push(format!("Couldn't open local playtime file: {e}")),
            }
        }

        // Collections (Favorites, user collections, Hidden) for the selected account.
        let collections = selected
            .map(|account| collections::cloud_storage_path(&steam, account.account_id))
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|text| {
                collections::parse_collections(&text).unwrap_or_else(|e| {
                    warnings.push(format!("Couldn't read your Steam collections ({e})."));
                    collections::Collections::default()
                })
            })
            .unwrap_or_default();

        let mut games: Vec<Game> = games.into_values().collect();
        games.sort_by_cached_key(|g| g.name.to_lowercase());

        Ok(SteamScan {
            info: SteamInfo {
                steam_path: steam.display().to_string(),
                steam_path_source: located.source,
                libraries,
                selected_account: selected.map(|a| a.steam_id.clone()),
                accounts,
            },
            games,
            collections,
            warnings,
        })
    }
}

impl GameSource for SteamSource {
    fn id(&self) -> &'static str {
        SOURCE_ID
    }

    fn scan(&self) -> Result<SourceScan, AppError> {
        let s = self.scan_detailed()?;
        Ok(SourceScan { games: s.games, warnings: s.warnings })
    }
}
