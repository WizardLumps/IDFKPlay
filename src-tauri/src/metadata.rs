//! Disk-cached store/SteamSpy metadata, filled by a throttled background task.
//!
//! The store API allows ~200 requests / 5 min, so requests go out at most every
//! 1.5 s per host. Results are saved every few apps so progress survives a quit.

use crate::jsonfile;
use crate::steam::store::{self, AppMeta, MetaStatus, StoreResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const CACHE_FILE: &str = "metadata-cache.json";
const CACHE_VERSION: u32 = 1;
const REQUEST_INTERVAL: Duration = Duration::from_millis(1500);
const THROTTLE_WAIT: Duration = Duration::from_secs(60);
const THROTTLE_RETRIES: u32 = 4;
const MAX_NETWORK_FAILURES: u32 = 3;
const SAVE_EVERY: usize = 10;
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

pub struct MetaStore {
    path: PathBuf,
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

        let mut store_gate = Gate { last: None };
        let mut spy_gate = Gate { last: None };
        let mut failures = 0;

        for (i, &id) in ids.iter().enumerate() {
            if self.cancelled(gen) {
                return;
            }
            let cached = lock(&self.apps).get(&id).cloned();
            let mut meta = match cached {
                Some(m) if m.status == MetaStatus::Known => m,
                _ => match self.fetch_store(&app, &client, &mut store_gate, id, gen).await {
                    Ok(m) => m,
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
                        self.update(&app, gen, |p| p.done = i + 1);
                        continue;
                    }
                },
            };
            failures = 0;

            if meta.status == MetaStatus::Known && meta.is_game() && !meta.tags_fetched {
                spy_gate.wait().await;
                if let Ok(tags) = fetch_steamspy(&client, id).await {
                    meta.tags = tags;
                    meta.tags_fetched = true;
                }
                // On failure tags stay empty and are retried on a later run.
            }

            lock(&self.apps).insert(id, meta);
            if (i + 1) % SAVE_EVERY == 0 {
                self.save();
            }
            self.update(&app, gen, |p| {
                p.done = i + 1;
                p.note = None;
            });
        }

        self.save();
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

    async fn fetch_store(
        &self,
        app: &AppHandle,
        client: &reqwest::Client,
        gate: &mut Gate,
        id: u32,
        gen: u64,
    ) -> Result<AppMeta, FetchError> {
        for attempt in 0..=THROTTLE_RETRIES {
            if self.cancelled(gen) {
                return Err(FetchError::Cancelled);
            }
            gate.wait().await;
            let resp = client
                .get(store::appdetails_url(id))
                .send()
                .await
                .map_err(|e| FetchError::Network(e.without_url().to_string()))?;
            let status = resp.status().as_u16();
            let result = if status == 429 || status == 403 {
                StoreResult::Throttled
            } else if status >= 500 {
                return Err(FetchError::Network(format!("HTTP {status}")));
            } else if !resp.status().is_success() {
                StoreResult::NotFound
            } else {
                let text = resp
                    .text()
                    .await
                    .map_err(|e| FetchError::Network(e.without_url().to_string()))?;
                store::parse_appdetails(id, &text).unwrap_or(StoreResult::NotFound)
            };

            let (status, app_type, genres, categories) = match result {
                StoreResult::Found { app_type, genres, categories } => (MetaStatus::Known, app_type, genres, categories),
                StoreResult::NotFound => (MetaStatus::Unknown, None, vec![], vec![]),
                StoreResult::Throttled => {
                    if attempt == THROTTLE_RETRIES {
                        break;
                    }
                    self.update(app, gen, |p| {
                        p.note = Some("Steam asked us to slow down. Waiting a minute…".into())
                    });
                    tokio::time::sleep(THROTTLE_WAIT).await;
                    continue;
                }
            };
            return Ok(AppMeta {
                status,
                app_type,
                genres,
                categories,
                tags: vec![],
                // Nothing to fetch from SteamSpy for unknown apps.
                tags_fetched: status == MetaStatus::Unknown,
                fetched_at: now_secs(),
            });
        }
        Err(FetchError::Throttled)
    }
}

async fn fetch_steamspy(client: &reqwest::Client, id: u32) -> Result<Vec<String>, String> {
    let resp = client
        .get(store::steamspy_url(id))
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| e.without_url().to_string())?;
    store::parse_steamspy_tags(&text)
}
