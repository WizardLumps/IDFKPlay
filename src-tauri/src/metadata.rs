//! Disk-cached store metadata (type, categories, tags), filled by a background task.
//!
//! Uses Steam's batch store service: ~100 apps per request, about a second apart, so
//! even large libraries are learned in seconds. Tag names come from Steam's public tag
//! list, cached for a month. Results are saved after every batch so progress survives a quit.

use crate::jsonfile;
use crate::steam::store::{AppMeta, MetaStatus};
use crate::steam::storebrowse::{self, Item};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const CACHE_FILE: &str = "metadata-cache.json";
/// v1: per-app store lookups + SteamSpy. v2: batch service. v3: adds store art URLs.
const CACHE_VERSION: u32 = 3;
const TAG_NAMES_FILE: &str = "tag-names.json";
const TAG_NAMES_MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60;
const REQUEST_INTERVAL: Duration = Duration::from_secs(1);
const THROTTLE_WAIT: Duration = Duration::from_secs(60);
const THROTTLE_RETRIES: u32 = 4;
const MAX_NETWORK_FAILURES: u32 = 3;
pub const PROGRESS_EVENT: &str = "metadata-progress";

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub running: bool,
    /// Transient status, e.g. waiting out a rate limit.
    pub note: Option<String>,
    /// Why the last run stopped early.
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    apps: HashMap<u32, AppMeta>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TagNamesFile {
    fetched_at: u64,
    tags: HashMap<u32, String>,
}

pub struct MetaStore {
    path: PathBuf,
    tag_names_path: PathBuf,
    apps: Mutex<HashMap<u32, AppMeta>>,
    progress: Mutex<Progress>,
    /// Bumped to cancel the running fetch task.
    generation: AtomicU64,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Turns one batch result into cached metadata.
pub fn to_meta(item: &Item, names: &HashMap<u32, String>, now: u64) -> AppMeta {
    if !item.found {
        return AppMeta {
            status: MetaStatus::Unknown,
            app_type: None,
            genres: vec![],
            categories: vec![],
            tags: vec![],
            tags_fetched: true,
            fetched_at: now,
            assets: None,
        };
    }
    let (tags, unknown) = storebrowse::tag_names(&item.tag_ids, names);
    AppMeta {
        status: MetaStatus::Known,
        app_type: item.app_type.map(String::from),
        genres: vec![],
        categories: item.categories.clone(),
        tags,
        // Tag IDs we couldn't name yet are retried on a later run.
        tags_fetched: unknown == 0,
        fetched_at: now,
        assets: item.assets.clone(),
    }
}

/// Spaces out requests to one host.
struct Gate {
    last: Option<Instant>,
}

impl Gate {
    async fn wait(&mut self) {
        if let Some(last) = self.last {
            let next = last + REQUEST_INTERVAL;
            let now = Instant::now();
            if next > now {
                tokio::time::sleep(next - now).await;
            }
        }
        self.last = Some(Instant::now());
    }
}

enum FetchError {
    Network(String),
    Throttled,
    Cancelled,
}

impl MetaStore {
    pub fn load(config_dir: &Path) -> Self {
        let path = config_dir.join(CACHE_FILE);
        let apps = jsonfile::read::<CacheFile>(&path)
            .filter(|c| c.version == CACHE_VERSION)
            .map(|c| c.apps)
            .unwrap_or_default();
        MetaStore {
            path,
            tag_names_path: config_dir.join(TAG_NAMES_FILE),
            apps: Mutex::new(apps),
            progress: Mutex::new(Progress::default()),
            generation: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> HashMap<u32, AppMeta> {
        lock(&self.apps).clone()
    }

    pub fn progress(&self) -> Progress {
        lock(&self.progress).clone()
    }

    pub fn is_running(&self) -> bool {
        lock(&self.progress).running
    }

    fn save(&self) {
        let file = CacheFile { version: CACHE_VERSION, apps: self.snapshot() };
        if let Err(e) = jsonfile::write(&self.path, &file) {
            eprintln!("metadata: couldn't save cache to {}: {e}", self.path.display());
        }
    }

    /// Stops any running fetch and empties the cache.
    pub fn clear(&self, app: &AppHandle) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        lock(&self.apps).clear();
        *lock(&self.progress) = Progress::default();
        self.save();
        let _ = app.emit(PROGRESS_EVENT, self.progress());
    }

    /// Starts fetching `ids` in the background, cancelling any previous run.
    pub fn start(self: &Arc<Self>, app: AppHandle, client: reqwest::Client, ids: Vec<u32>) {
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if ids.is_empty() {
            *lock(&self.progress) = Progress::default();
            let _ = app.emit(PROGRESS_EVENT, self.progress());
            return;
        }
        let store = Arc::clone(self);
        tauri::async_runtime::spawn(async move { store.run(app, client, ids, gen).await });
    }

    fn cancelled(&self, gen: u64) -> bool {
        self.generation.load(Ordering::SeqCst) != gen
    }

    fn update(&self, app: &AppHandle, gen: u64, f: impl FnOnce(&mut Progress)) {
        if self.cancelled(gen) {
            return;
        }
        let snapshot = {
            let mut p = lock(&self.progress);
            f(&mut p);
            p.clone()
        };
        let _ = app.emit(PROGRESS_EVENT, snapshot);
    }

    async fn run(self: Arc<Self>, app: AppHandle, client: reqwest::Client, ids: Vec<u32>, gen: u64) {
        let total = ids.len();
        self.update(&app, gen, |p| *p = Progress { total, running: true, ..Default::default() });

        let (mut names, mut names_fresh) = self.tag_names(&client).await;
        let mut gate = Gate { last: None };
        let mut failures = 0;
        let mut done = 0;

        for chunk in ids.chunks(storebrowse::BATCH_SIZE) {
            if self.cancelled(gen) {
                return;
            }
            let items = match self.fetch_batch(&app, &client, &mut gate, chunk, gen).await {
                Ok(items) => items,
                Err(FetchError::Cancelled) => return,
                Err(FetchError::Throttled) => {
                    return self.stop(&app, gen, "Steam's store is rate-limiting us. Tag learning will pick up next time.");
                }
                Err(FetchError::Network(e)) => {
                    failures += 1;
                    if failures >= MAX_NETWORK_FAILURES {
                        return self.stop(
                            &app,
                            gen,
                            &format!("Couldn't reach the Steam store ({e}). Tag learning will resume next time you're online."),
                        );
                    }
                    // Skipped apps stay uncached and are retried next run.
                    done += chunk.len();
                    self.update(&app, gen, |p| p.done = done);
                    continue;
                }
            };
            failures = 0;

            // Tag IDs we can't name mean Steam added tags since the list was cached: refresh once.
            let unnamed = items.iter().flat_map(|i| &i.tag_ids).any(|t| !names.contains_key(t));
            if unnamed && !names_fresh {
                if let Ok(fresh) = self.fetch_tag_names(&client).await {
                    names = fresh;
                }
                names_fresh = true;
            }

            let now = now_secs();
            {
                // Released before save(), which takes this lock again.
                let mut apps = lock(&self.apps);
                for item in &items {
                    apps.insert(item.app_id, to_meta(item, &names, now));
                }
            }
            self.save();
            done += chunk.len();
            self.update(&app, gen, |p| {
                p.done = done;
                p.note = None;
            });
        }

        self.update(&app, gen, |p| {
            p.running = false;
            p.note = None;
        });
    }

    fn stop(&self, app: &AppHandle, gen: u64, error: &str) {
        self.save();
        self.update(app, gen, |p| {
            p.running = false;
            p.note = None;
            p.error = Some(error.to_string());
        });
    }

    /// Cached tag names, fetched fresh when missing or a month old. Returns whether they're fresh.
    async fn tag_names(&self, client: &reqwest::Client) -> (HashMap<u32, String>, bool) {
        let cached = jsonfile::read::<TagNamesFile>(&self.tag_names_path);
        if let Some(c) = &cached {
            if now_secs().saturating_sub(c.fetched_at) < TAG_NAMES_MAX_AGE_SECS && !c.tags.is_empty() {
                return (c.tags.clone(), false);
            }
        }
        match self.fetch_tag_names(client).await {
            Ok(tags) => (tags, true),
            // Offline: an old list is better than none.
            Err(_) => (cached.map(|c| c.tags).unwrap_or_default(), false),
        }
    }

    async fn fetch_tag_names(&self, client: &reqwest::Client) -> Result<HashMap<u32, String>, String> {
        let resp = client
            .get(storebrowse::TAG_LIST_URL)
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let text = resp.text().await.map_err(|e| e.without_url().to_string())?;
        let tags = storebrowse::parse_tag_list(&text)?;
        let file = TagNamesFile { fetched_at: now_secs(), tags };
        if let Err(e) = jsonfile::write(&self.tag_names_path, &file) {
            eprintln!("metadata: couldn't save tag names: {e}");
        }
        Ok(file.tags)
    }

    async fn fetch_batch(
        &self,
        app: &AppHandle,
        client: &reqwest::Client,
        gate: &mut Gate,
        ids: &[u32],
        gen: u64,
    ) -> Result<Vec<Item>, FetchError> {
        let url = storebrowse::items_url(ids).map_err(FetchError::Network)?;
        for attempt in 0..=THROTTLE_RETRIES {
            if self.cancelled(gen) {
                return Err(FetchError::Cancelled);
            }
            gate.wait().await;
            let resp = client
                .get(url.clone())
                .send()
                .await
                .map_err(|e| FetchError::Network(e.without_url().to_string()))?;
            let status = resp.status().as_u16();
            if status == 429 || status == 403 {
                if attempt == THROTTLE_RETRIES {
                    break;
                }
                self.update(app, gen, |p| p.note = Some("Steam asked us to slow down. Waiting a minute…".into()));
                tokio::time::sleep(THROTTLE_WAIT).await;
                continue;
            }
            if !resp.status().is_success() {
                return Err(FetchError::Network(format!("HTTP {status}")));
            }
            let text = resp
                .text()
                .await
                .map_err(|e| FetchError::Network(e.without_url().to_string()))?;
            return storebrowse::parse_items(&text).map_err(FetchError::Network);
        }
        Err(FetchError::Throttled)
    }
}
