pub mod backdrop;
pub mod error;
pub mod jsonfile;
pub mod library;
pub mod metadata;
pub mod picker;
pub mod secrets;
pub mod settings;
pub mod source;
pub mod steam;
pub mod vdf;

use library::{ApiStatus, LibraryGame, LibraryView, RawLibrary};
use metadata::{MetaStore, Progress};
use picker::{PickDecision, PickKind, PickRequest, PickRules, Session};
use serde::{Deserialize, Serialize};
use settings::{AfterLaunch, Settings};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use steam::webapi::{self, OwnedGame};
use steam::SteamSource;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

const OWNED_CACHE_FILE: &str = "owned-games.json";

struct AppState {
    config_dir: PathBuf,
    settings: Mutex<Settings>,
    http: reqwest::Client,
    /// Last scan; views are rebuilt from it as metadata arrives.
    library: Mutex<Option<RawLibrary>>,
    meta: Arc<MetaStore>,
    /// Reroll budget and the current pick for this app session.
    session: Mutex<Session>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl AppState {
    fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }
}

/// Last good owned-games response, so uninstalled games survive going offline.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OwnedCache {
    steam_id: String,
    fetched_at: u64,
    games: Vec<OwnedGame>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

async fn owned_games(state: &AppState, steam_id: Option<&str>) -> (Option<Vec<OwnedGame>>, ApiStatus) {
    let Some(key) = secrets::load_api_key(&state.config_dir) else {
        return (None, ApiStatus::NoKey);
    };
    let Some(steam_id) = steam_id else {
        return (None, ApiStatus::NoAccount);
    };
    let cache_path = state.config_dir.join(OWNED_CACHE_FILE);
    match webapi::fetch_owned_games(&state.http, &key, steam_id).await {
        Ok(games) => {
            let cache = OwnedCache { steam_id: steam_id.to_string(), fetched_at: now_secs(), games };
            if let Err(e) = jsonfile::write(&cache_path, &cache) {
                eprintln!("couldn't cache owned games: {e}");
            }
            let count = cache.games.len();
            (Some(cache.games), ApiStatus::Ok { count })
        }
        Err(e) => match jsonfile::read::<OwnedCache>(&cache_path).filter(|c| c.steam_id == steam_id) {
            Some(c) => {
                let status = ApiStatus::Cached { count: c.games.len(), fetched_at: c.fetched_at, message: e.to_string() };
                (Some(c.games), status)
            }
            None => (None, ApiStatus::Error { message: e.to_string() }),
        },
    }
}

async fn scan_library(state: &AppState) -> Result<RawLibrary, String> {
    let source = SteamSource::new(state.settings());
    let scan = tauri::async_runtime::spawn_blocking(move || source.scan_detailed())
        .await
        .map_err(|e| format!("The library scan stopped unexpectedly: {e}"))??;
    let (owned, api) = owned_games(state, scan.info.selected_account.as_deref()).await;
    Ok(RawLibrary {
        info: scan.info,
        local: scan.games,
        owned,
        collections: scan.collections,
        api,
        warnings: scan.warnings,
    })
}

/// The last scan, scanning first if there is none or `refresh` is set.
async fn current_library(app: AppHandle, state: &AppState, refresh: bool) -> Result<RawLibrary, String> {
    if !refresh {
        if let Some(raw) = lock(&state.library).clone() {
            return Ok(raw);
        }
    }
    let raw = scan_library(state).await?;
    *lock(&state.library) = Some(raw.clone());
    // The UI may load cached art from Steam's library cache, and nothing else on disk.
    let cache = steam::art::library_cache_dir(Path::new(&raw.info.steam_path));
    if let Err(e) = app.asset_protocol_scope().allow_directory(&cache, true) {
        eprintln!("couldn't allow art folder {}: {e}", cache.display());
    }
    // Only (re)start metadata fetching after a scan, so view rebuilds can't loop.
    let needed = library::ids_needing_meta(&raw, &state.meta.snapshot());
    state.meta.start(app, state.http.clone(), needed);
    Ok(raw)
}

/// The picker's game list. Scans on first call or when `refresh` is set; otherwise
/// rebuilds from the last scan plus whatever metadata has arrived since.
#[tauri::command]
async fn get_library(app: AppHandle, state: State<'_, AppState>, refresh: bool) -> Result<LibraryView, String> {
    let raw = current_library(app, &state, refresh).await?;
    Ok(library::build_view(&raw, &state.meta.snapshot()))
}

/// Tiles shown in the spin before landing on the pick.
const REEL_LEN: usize = 14;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReelItem {
    app_id: u32,
    name: String,
    portrait: Vec<steam::art::ArtSource>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PickOutcome {
    game: Option<LibraryGame>,
    /// Art for `game`.
    art: Option<steam::art::GameArt>,
    /// Other eligible games to spin through (may be empty).
    reel: Vec<ReelItem>,
    /// How many games matched (before excluding already-shown ones).
    eligible: usize,
    /// `None` = unlimited.
    rerolls_left: Option<u32>,
    /// Rerolls spent this session (drives the escalating snark).
    rerolls_used: u32,
    /// This pick spent a reroll.
    charged: bool,
    /// Out of rerolls: `game` is the same game as before.
    locked: bool,
}

#[tauri::command]
async fn pick_game(
    app: AppHandle,
    state: State<'_, AppState>,
    mut request: PickRequest,
    kind: PickKind,
) -> Result<PickOutcome, String> {
    let raw = current_library(app, &state, false).await?;
    // Without owned games, uninstalled ones aren't known, so the toggle is forced on.
    if !raw.api.has_owned() {
        request.installed_only = true;
    }
    let view = library::build_view(&raw, &state.meta.snapshot());
    let settings = state.settings();
    let members = library::collection_members(&raw.collections, &request.collections);
    let rules = PickRules {
        something_new_minutes: (settings.something_new_hours * 60.0).round() as u32,
        blacklist: &settings.blacklist,
        collection_members: members.as_ref(),
        hidden: if settings.skip_steam_hidden { &raw.collections.hidden } else { &[] },
    };
    let pool = picker::eligible(&view.games, &request, &rules);
    let limit = settings.rerolls_per_session;

    // Nothing matches: say so without spending a reroll.
    let (game, charged, locked) = if pool.is_empty() {
        (None, false, false)
    } else {
        let mut session = lock(&state.session);
        let decision = session.decide(kind, limit);
        // A locked game that's gone since (e.g. blacklisted) falls back to a normal pick.
        let locked_game = match decision {
            PickDecision::Locked(id) => view.games.iter().find(|g| g.game.app_id == id && !settings.blacklist.contains(&id)),
            _ => None,
        };
        let (game, locked) = match locked_game {
            Some(g) => (Some(g.clone()), true),
            None => (picker::choose(&pool, &request.exclude, picker::random_u64()).cloned(), false),
        };
        if let Some(g) = &game {
            session.shown(g.game.app_id);
        }
        (game, decision == PickDecision::Charged, locked)
    };

    let steam_path = PathBuf::from(&raw.info.steam_path);
    let art = game.as_ref().map(|g| steam::art::game_art(Some(&steam_path), g.game.app_id));
    let reel = game
        .as_ref()
        .map(|g| picker::reel(&pool, g.game.app_id, REEL_LEN, picker::random_u64()))
        .unwrap_or_default()
        .into_iter()
        .map(|g| ReelItem {
            app_id: g.game.app_id,
            name: g.game.name.clone(),
            portrait: steam::art::game_art(Some(&steam_path), g.game.app_id).portrait,
        })
        .collect();
    Ok(PickOutcome {
        game,
        art,
        reel,
        eligible: pool.len(),
        rerolls_left: lock(&state.session).left(limit),
        rerolls_used: lock(&state.session).used(),
        charged,
        locked,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackdropItem {
    app_id: u32,
    portrait: Vec<steam::art::ArtSource>,
}

/// Covers for the home-screen backdrop: a fresh random mix, favorites and played games
/// first, locally cached art preferred. Skips blacklisted and Steam-hidden games.
#[tauri::command]
async fn get_backdrop(app: AppHandle, state: State<'_, AppState>, count: usize) -> Result<Vec<BackdropItem>, String> {
    let raw = current_library(app, &state, false).await?;
    let settings = state.settings();
    let view = library::build_view(&raw, &state.meta.snapshot());
    let steam_path = PathBuf::from(&raw.info.steam_path);
    let cache = steam::art::library_cache_dir(&steam_path);
    let favorites: Vec<u32> = raw
        .collections
        .list
        .iter()
        .find(|c| c.id == steam::collections::FAVORITES_ID)
        .map(|c| c.app_ids.clone())
        .unwrap_or_default();

    let candidates: Vec<backdrop::Candidate> = view
        .games
        .iter()
        .map(|g| &g.game)
        .filter(|g| !settings.blacklist.contains(&g.app_id) && !raw.collections.hidden.contains(&g.app_id))
        .map(|g| backdrop::Candidate {
            app_id: g.app_id,
            has_local_art: steam::art::find_local(&cache, g.app_id, steam::art::ArtKind::Portrait).is_some(),
            favorite: favorites.contains(&g.app_id),
            played: g.playtime_minutes > 0,
        })
        .collect();

    Ok(backdrop::choose(&candidates, count.min(60), backdrop::seed())
        .into_iter()
        .map(|id| BackdropItem { app_id: id, portrait: steam::art::game_art(Some(&steam_path), id).portrait })
        .collect())
}

/// Art candidates for a game, local cache first then CDN.
#[tauri::command]
fn get_art(state: State<'_, AppState>, app_id: u32) -> steam::art::GameArt {
    let steam_path = lock(&state.library).as_ref().map(|r| PathBuf::from(&r.info.steam_path));
    steam::art::game_art(steam_path.as_deref(), app_id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchOutcome {
    /// The app is about to close (after-launch setting).
    closing: bool,
}

/// Hands the game to Steam: run if installed, otherwise Steam's install dialog.
#[tauri::command]
async fn launch_game(app: AppHandle, state: State<'_, AppState>, app_id: u32) -> Result<LaunchOutcome, String> {
    let raw = current_library(app.clone(), &state, false).await?;
    let installed = library::merge(&raw.local, raw.owned.as_deref())
        .iter()
        .find(|g| g.app_id == app_id)
        .map(|g| g.installed)
        .ok_or("That game isn't in your library anymore. Refresh and try again.")?;
    let url = if installed {
        format!("steam://rungameid/{app_id}")
    } else {
        format!("steam://install/{app_id}")
    };
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("Couldn't hand the game to Steam ({e}). Is Steam installed?"))?;
    // The cycle is done: the next pick is a fresh, free one.
    lock(&state.session).launched();

    let closing = state.settings().after_launch == AfterLaunch::Close;
    if closing {
        // Let the response reach the UI before exiting.
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            handle.exit(0);
        });
    }
    Ok(LaunchOutcome { closing })
}

#[tauri::command]
fn set_blacklisted(state: State<'_, AppState>, app_id: u32, blacklisted: bool) -> Result<Settings, String> {
    let mut settings = state.settings();
    settings.blacklist.retain(|&id| id != app_id);
    if blacklisted {
        settings.blacklist.push(app_id);
    }
    store_settings(&state, settings)
}

#[tauri::command]
fn get_metadata_progress(state: State<'_, AppState>) -> Progress {
    state.meta.progress()
}

/// Clears the tag cache and re-learns everything for the current library.
#[tauri::command]
fn rebuild_metadata(app: AppHandle, state: State<'_, AppState>) {
    state.meta.clear(&app);
    if let Some(raw) = lock(&state.library).clone() {
        let needed = library::ids_needing_meta(&raw, &state.meta.snapshot());
        state.meta.start(app, state.http.clone(), needed);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyStatus {
    has_key: bool,
    storage: Option<secrets::KeyStorage>,
}

fn key_status(dir: &Path, storage: Option<secrets::KeyStorage>) -> ApiKeyStatus {
    ApiKeyStatus { has_key: secrets::load_api_key(dir).is_some(), storage }
}

#[tauri::command]
fn get_api_key_status(state: State<'_, AppState>) -> ApiKeyStatus {
    key_status(&state.config_dir, None)
}

/// Saves the key (or removes it with `null`). Call `get_library(refresh)` afterwards.
#[tauri::command]
fn set_api_key(state: State<'_, AppState>, key: Option<String>) -> Result<ApiKeyStatus, String> {
    let storage = secrets::store_api_key(&state.config_dir, key.as_deref())?;
    Ok(key_status(&state.config_dir, storage))
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

fn store_settings(state: &AppState, settings: Settings) -> Result<Settings, String> {
    let settings = settings.normalized();
    settings::save(&state.config_dir, &settings)
        .map_err(|e| format!("Couldn't save settings to {}: {e}", state.config_dir.display()))?;
    *lock(&state.settings) = settings.clone();
    Ok(settings)
}

/// Saves and returns the normalized settings. Call `get_library(refresh)` after Steam-related changes.
#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    store_settings(&state, settings)
}

/// Rerolls left this session under the current setting (`None` = unlimited).
#[tauri::command]
fn rerolls_left(state: State<'_, AppState>) -> Option<u32> {
    lock(&state.session).left(state.settings().rerolls_per_session)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Remembers window size/position between launches.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let config_dir = settings::config_dir(app.path().app_config_dir()?);
            let http = reqwest::Client::builder()
                .user_agent(concat!("IDFKPlay/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(20))
                .build()?;
            app.manage(AppState {
                settings: Mutex::new(settings::load(&config_dir)),
                meta: Arc::new(MetaStore::load(&config_dir)),
                library: Mutex::new(None),
                session: Mutex::new(Session::default()),
                http,
                config_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_library,
            pick_game,
            get_art,
            get_backdrop,
            launch_game,
            set_blacklisted,
            rerolls_left,
            get_metadata_progress,
            rebuild_metadata,
            get_api_key_status,
            set_api_key,
            get_settings,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
