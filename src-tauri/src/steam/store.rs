//! Per-app metadata from the Steam store (type, genres, categories) and SteamSpy (community tags).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn appdetails_url(app_id: u32) -> String {
    // l=english keeps genre/category names stable regardless of the user's region.
    format!("https://store.steampowered.com/api/appdetails?appids={app_id}&l=english")
}

pub fn steamspy_url(app_id: u32) -> String {
    format!("https://steamspy.com/api.php?request=appdetails&appid={app_id}")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MetaStatus {
    /// Store data was fetched.
    Known,
    /// Store says `success: false` (delisted, region-locked…): kept, but tags are unknown.
    Unknown,
}

/// Cached metadata for one app.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppMeta {
    pub status: MetaStatus,
    pub app_type: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    /// SteamSpy community tags, most-voted first.
    #[serde(default)]
    pub tags: Vec<String>,
    /// False if SteamSpy failed; retried on a later run.
    #[serde(default)]
    pub tags_fetched: bool,
    pub fetched_at: u64,
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

#[derive(Debug, Clone, PartialEq)]
pub enum StoreResult {
    Found { app_type: Option<String>, genres: Vec<String>, categories: Vec<String> },
    NotFound,
    /// Body was `null`: the store does this when throttling.
    Throttled,
}

/// `description` of each item, trimmed and de-duplicated (the store repeats some categories).
fn descriptions(v: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in v.and_then(Value::as_array).into_iter().flatten() {
        let Some(d) = item.get("description").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if !d.is_empty() && !out.iter().any(|o| o == d) {
            out.push(d.to_string());
        }
    }
    out
}

pub fn parse_appdetails(app_id: u32, json: &str) -> Result<StoreResult, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if root.is_null() {
        return Ok(StoreResult::Throttled);
    }
    let Some(entry) = root.get(app_id.to_string()) else {
        return Ok(StoreResult::NotFound);
    };
    if entry.get("success").and_then(Value::as_bool) != Some(true) {
        return Ok(StoreResult::NotFound);
    }
    let Some(data) = entry.get("data") else {
        return Ok(StoreResult::NotFound);
    };
    Ok(StoreResult::Found {
        app_type: data.get("type").and_then(Value::as_str).map(str::to_lowercase),
        genres: descriptions(data.get("genres")),
        categories: descriptions(data.get("categories")),
    })
}

/// SteamSpy `tags` is `{ "Puzzle": 1234, ... }` or `[]` when there are none.
pub fn parse_steamspy_tags(json: &str) -> Result<Vec<String>, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let Some(tags) = root.get("tags").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    let mut pairs: Vec<(&String, u64)> = tags.iter().map(|(k, v)| (k, v.as_u64().unwrap_or(0))).collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    Ok(pairs.into_iter().map(|(k, _)| k.clone()).collect())
}
