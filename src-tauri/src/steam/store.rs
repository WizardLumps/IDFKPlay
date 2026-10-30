//! Cached per-app store metadata (type, categories, tags). Fetched in batches by
//! `metadata.rs` via `storebrowse.rs`.

use super::art::StoreAssets;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MetaStatus {
    /// Store data was fetched.
    Known,
    /// The store doesn't know the app (delisted, region-locked…): kept, but tags are unknown.
    Unknown,
}

/// Cached metadata for one app.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppMeta {
    pub status: MetaStatus,
    pub app_type: Option<String>,
    /// Store genres. Not provided by the batch service; genre-like tags cover them.
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    /// Community tags, most-voted first.
    #[serde(default)]
    pub tags: Vec<String>,
    /// False if some tag names weren't known yet; retried on a later run.
    #[serde(default)]
    pub tags_fetched: bool,
    pub fetched_at: u64,
    /// Exact art URLs from the store (newer games aren't on the classic CDN paths).
    #[serde(default)]
    pub assets: Option<StoreAssets>,
}

impl AppMeta {
    pub fn is_game(&self) -> bool {
        match (&self.status, &self.app_type) {
            (MetaStatus::Known, Some(t)) => t.eq_ignore_ascii_case("game"),
            // Unknown type: give it the benefit of the doubt.
            _ => true,
        }
    }
}
