//! Steam library collections (Favorites, Hidden, user collections).
//!
//! Modern Steam keeps them in `userdata\<accountid>\config\cloudstorage\cloud-storage-namespace-1.json`:
//! a JSON array of `[key, entry]` pairs. Collection keys are `user-collections.<id>` and
//! `entry.value` is itself a JSON string: `{"id","name","added":[appids],"removed":[appids],"filterSpec"?}`.
//! Dynamic collections (with `filterSpec`) are computed by Steam from rules; we use the
//! app list Steam stores with them and don't re-evaluate the rules.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const FAVORITES_ID: &str = "favorite";
pub const HIDDEN_ID: &str = "hidden";
const KEY_PREFIX: &str = "user-collections.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub app_ids: Vec<u32>,
    /// Rule-based in Steam; `app_ids` is the list Steam stored, not a re-evaluation.
    pub dynamic: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Collections {
    /// Favorites first, then user collections by name. Excludes Hidden.
    pub list: Vec<Collection>,
    /// Apps hidden in Steam's library.
    pub hidden: Vec<u32>,
}

pub fn cloud_storage_path(steam: &Path, account_id: u32) -> PathBuf {
    steam
        .join("userdata")
        .join(account_id.to_string())
        .join("config")
        .join("cloudstorage")
        .join("cloud-storage-namespace-1.json")
}

fn app_ids(v: Option<&Value>) -> Vec<u32> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_u64).filter_map(|n| u32::try_from(n).ok()).collect())
        .unwrap_or_default()
}

pub fn parse_collections(json: &str) -> Result<Collections, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let entries = root.as_array().ok_or("expected a JSON array")?;
    let mut out = Collections::default();

    for pair in entries {
        let (Some(key), Some(entry)) = (pair.get(0).and_then(Value::as_str), pair.get(1)) else {
            continue;
        };
        let Some(id) = key.strip_prefix(KEY_PREFIX) else { continue };
        if entry.get("is_deleted").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(value) = entry
            .get("value")
            .and_then(Value::as_str)
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
        else {
            continue;
        };

        let removed = app_ids(value.get("removed"));
        let mut members: Vec<u32> = app_ids(value.get("added")).into_iter().filter(|a| !removed.contains(a)).collect();
        members.sort_unstable();
        members.dedup();

        if id == HIDDEN_ID {
            out.hidden = members;
            continue;
        }
        let name = value.get("name").and_then(Value::as_str).map(str::trim).unwrap_or("");
        let name = match (id, name) {
            (FAVORITES_ID, "") => "Favorites".to_string(),
            (_, "") => continue,
            (_, n) => n.to_string(),
        };
        out.list.push(Collection {
            id: id.to_string(),
            name,
            app_ids: members,
            dynamic: value.get("filterSpec").is_some_and(|f| !f.is_null()),
        });
    }

    out.list.sort_by(|a, b| {
        (b.id == FAVORITES_ID)
            .cmp(&(a.id == FAVORITES_ID))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}
