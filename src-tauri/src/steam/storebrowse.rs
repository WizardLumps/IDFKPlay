//! Steam's batch store service (`IStoreBrowseService/GetItems`): type, categories and
//! tags for ~100 apps per request, so a whole library is learned in seconds.
//! Tags and categories arrive as numeric IDs; tag names come from `IStoreService/GetTagList`.

use serde_json::{json, Value};
use std::collections::HashMap;

const GET_ITEMS_URL: &str = "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/";
pub const TAG_LIST_URL: &str = "https://api.steampowered.com/IStoreService/GetTagList/v1/?language=english";
/// Apps per request: comfortably inside URL length limits.
pub const BATCH_SIZE: usize = 100;
/// GetItems returns a game's top tags only; more aren't available.
const TAG_COUNT: u32 = 20;

/// `success` value for an app the store knows.
const SUCCESS: u64 = 1;

/// Store app types (EStoreAppType), as the names the rest of the app uses.
pub fn app_type_name(t: u64) -> &'static str {
    match t {
        0 => "game",
        1 => "demo",
        2 => "mod",
        3 => "movie",
        4 => "dlc",
        5 => "guide",
        6 => "software",
        7 => "video",
        8 => "series",
        9 => "episode",
        10 => "hardware",
        11 => "music",
        12 => "beta",
        13 => "tool",
        14 => "advertising",
        _ => "other",
    }
}

/// Store category IDs the picker uses, with the store's own descriptions.
/// Other categories (achievements, cloud saves…) aren't needed and are dropped.
pub fn category_name(id: u64) -> Option<&'static str> {
    Some(match id {
        1 => "Multi-player",
        2 => "Single-player",
        9 => "Co-op",
        18 => "Partial Controller Support",
        20 => "MMO",
        24 => "Shared/Split Screen",
        27 => "Cross-Platform Multiplayer",
        28 => "Full controller support",
        31 => "VR Support",
        36 => "Online PvP",
        37 => "Shared/Split Screen PvP",
        38 => "Online Co-op",
        39 => "Shared/Split Screen Co-op",
        47 => "LAN PvP",
        48 => "LAN Co-op",
        49 => "PvP",
        53 => "VR Supported",
        54 => "VR Only",
        _ => return None,
    })
}

/// One app from a GetItems response.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub app_id: u32,
    /// The store knows the app. `false` = delisted / region-locked / not a store item.
    pub found: bool,
    pub app_type: Option<&'static str>,
    pub categories: Vec<String>,
    /// Most-voted first.
    pub tag_ids: Vec<u32>,
}

pub fn items_url(app_ids: &[u32]) -> Result<reqwest::Url, String> {
    let input = json!({
        "ids": app_ids.iter().map(|id| json!({ "appid": id })).collect::<Vec<_>>(),
        // English names and a fixed country so results don't depend on the user's region.
        "context": { "language": "english", "country_code": "US" },
        "data_request": { "include_tag_count": TAG_COUNT },
    });
    reqwest::Url::parse_with_params(GET_ITEMS_URL, &[("input_json", input.to_string())]).map_err(|e| e.to_string())
}

fn u32s(v: Option<&Value>) -> impl Iterator<Item = u32> + '_ {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .filter_map(|n| u32::try_from(n).ok())
}

pub fn parse_items(json: &str) -> Result<Vec<Item>, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let items = root
        .pointer("/response/store_items")
        .and_then(Value::as_array)
        .ok_or("missing response.store_items")?;

    Ok(items
        .iter()
        .filter_map(|it| {
            // Not-found entries come back with appid 0 but keep the requested id in `id`.
            let app_id = it
                .get("appid")
                .and_then(Value::as_u64)
                .filter(|&a| a != 0)
                .or_else(|| it.get("id").and_then(Value::as_u64))
                .and_then(|a| u32::try_from(a).ok())?;
            if it.get("success").and_then(Value::as_u64) != Some(SUCCESS) {
                return Some(Item { app_id, found: false, app_type: None, categories: vec![], tag_ids: vec![] });
            }

            let mut categories: Vec<String> = Vec::new();
            if let Some(cats) = it.get("categories").and_then(Value::as_object) {
                for ids in cats.values() {
                    for name in u32s(Some(ids)).filter_map(|id| category_name(id.into())) {
                        if !categories.iter().any(|c| c == name) {
                            categories.push(name.to_string());
                        }
                    }
                }
            }

            // `tags` carries weights; fall back to the bare `tagids` list.
            let mut weighted: Vec<(u32, u64)> = it
                .get("tags")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|t| {
                    let id = u32::try_from(t.get("tagid")?.as_u64()?).ok()?;
                    Some((id, t.get("weight").and_then(Value::as_u64).unwrap_or(0)))
                })
                .collect();
            weighted.sort_by(|a, b| b.1.cmp(&a.1));
            let mut tag_ids: Vec<u32> = weighted.into_iter().map(|(id, _)| id).collect();
            if tag_ids.is_empty() {
                tag_ids = u32s(it.get("tagids")).collect();
            }

            Some(Item {
                app_id,
                found: true,
                app_type: it.get("type").and_then(Value::as_u64).map(app_type_name),
                categories,
                tag_ids,
            })
        })
        .collect())
}

pub fn parse_tag_list(json: &str) -> Result<HashMap<u32, String>, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tags = root.pointer("/response/tags").and_then(Value::as_array).ok_or("missing response.tags")?;
    Ok(tags
        .iter()
        .filter_map(|t| {
            let id = u32::try_from(t.get("tagid")?.as_u64()?).ok()?;
            let name = t.get("name")?.as_str()?.trim();
            (!name.is_empty()).then(|| (id, name.to_string()))
        })
        .collect())
}

/// Tag names for `ids`, in order, plus how many IDs had no known name.
pub fn tag_names(ids: &[u32], names: &HashMap<u32, String>) -> (Vec<String>, usize) {
    let known: Vec<String> = ids.iter().filter_map(|id| names.get(id).cloned()).collect();
    let unknown = ids.len() - known.len();
    (known, unknown)
}
