//! Combines local scan + Web API owned games + cached metadata into the list the picker uses.

use crate::source::Game;
use crate::steam::collections::Collections;
use crate::steam::store::{AppMeta, MetaStatus};
use crate::steam::webapi::OwnedGame;
use crate::steam::{self, SteamInfo};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// App IDs that are never games.
const EXCLUDED_APP_IDS: &[u32] = &[
    228980, // Steamworks Common Redistributables
];

/// Whole-word phrases in a name that mark tools/runtimes rather than games.
const EXCLUDED_NAME_PHRASES: &[&str] = &[
    "proton",
    "steam linux runtime",
    "dedicated server",
    "sdk",
    "soundtrack",
    "redistributable",
    "redistributables",
];

/// How the owned-games list was obtained.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ApiStatus {
    /// No Web API key: installed games only.
    NoKey,
    /// Key set but no Steam account found to query.
    NoAccount,
    Ok { count: usize },
    /// Live call failed; showing the last good list.
    Cached { count: usize, fetched_at: u64, message: String },
    Error { message: String },
}

impl ApiStatus {
    /// True when uninstalled games are included.
    pub fn has_owned(&self) -> bool {
        matches!(self, ApiStatus::Ok { .. } | ApiStatus::Cached { .. })
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MetaState {
    /// Not fetched yet.
    Pending,
    Known,
    /// Store had nothing (delisted etc.): tags unknown.
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGame {
    #[serde(flatten)]
    pub game: Game,
    pub meta: MetaState,
    pub genres: Vec<String>,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiddenGame {
    pub app_id: u32,
    pub name: String,
    pub reason: String,
}

/// A Steam collection as the UI shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionInfo {
    pub id: String,
    pub name: String,
    /// Members that are in the picker's game list.
    pub count: usize,
    pub dynamic: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    pub games: Vec<LibraryGame>,
    pub hidden: Vec<HiddenGame>,
    pub collections: Vec<CollectionInfo>,
    /// Games hidden in Steam's own library.
    pub steam_hidden: Vec<u32>,
    pub steam: SteamInfo,
    pub api: ApiStatus,
    pub warnings: Vec<String>,
}

/// Raw inputs kept in memory between view rebuilds.
#[derive(Debug, Clone)]
pub struct RawLibrary {
    pub info: SteamInfo,
    pub local: Vec<Game>,
    pub owned: Option<Vec<OwnedGame>>,
    pub collections: Collections,
    pub api: ApiStatus,
    pub warnings: Vec<String>,
}

/// Union of the members of the collections with these IDs. `None` when no IDs are given.
pub fn collection_members(collections: &Collections, ids: &[String]) -> Option<HashSet<u32>> {
    if ids.is_empty() {
        return None;
    }
    Some(
        collections
            .list
            .iter()
            .filter(|c| ids.contains(&c.id))
            .flat_map(|c| c.app_ids.iter().copied())
            .collect(),
    )
}

/// Owned games are the base list; local manifests add install state and fill gaps
/// (e.g. family-shared or free games the API omits). API playtime wins when present.
pub fn merge(local: &[Game], owned: Option<&[OwnedGame]>) -> Vec<Game> {
    let Some(owned) = owned else {
        return local.to_vec();
    };
    let local_by_id: HashMap<u32, &Game> = local.iter().map(|g| (g.app_id, g)).collect();
    let mut out: Vec<Game> = owned
        .iter()
        .map(|o| {
            let l = local_by_id.get(&o.appid);
            let name = if o.name.trim().is_empty() {
                l.map(|g| g.name.clone()).unwrap_or_else(|| format!("App {}", o.appid))
            } else {
                o.name.clone()
            };
            Game {
                source: steam::SOURCE_ID,
                app_id: o.appid,
                name,
                installed: l.is_some_and(|g| g.installed),
                install_path: l.and_then(|g| g.install_path.clone()),
                size_on_disk: l.and_then(|g| g.size_on_disk),
                playtime_minutes: o.playtime_forever.max(l.map_or(0, |g| g.playtime_minutes)),
                last_played: Some(o.rtime_last_played)
                    .filter(|&t| t > 0)
                    .or_else(|| l.and_then(|g| g.last_played)),
            }
        })
        .collect();
    let owned_ids: std::collections::HashSet<u32> = owned.iter().map(|o| o.appid).collect();
    out.extend(local.iter().filter(|g| !owned_ids.contains(&g.app_id)).cloned());
    out.sort_by_cached_key(|g| g.name.to_lowercase());
    out
}

/// Lowercase words separated by single spaces, padded so phrase matching is whole-word.
fn word_key(name: &str) -> String {
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    format!(" {} ", words.join(" "))
}

/// Reason a game should be hidden based on its ID/name alone.
pub fn excluded_by_name(app_id: u32, name: &str) -> Option<String> {
    if EXCLUDED_APP_IDS.contains(&app_id) {
        return Some("Steam redistributable".into());
    }
    let key = word_key(name);
    EXCLUDED_NAME_PHRASES
        .iter()
        .find(|p| key.contains(&format!(" {p} ")))
        .map(|p| format!("Name matches \"{p}\""))
}

pub fn build_view(raw: &RawLibrary, meta: &HashMap<u32, AppMeta>) -> LibraryView {
    let mut games = Vec::new();
    let mut hidden = Vec::new();
    for g in merge(&raw.local, raw.owned.as_deref()) {
        let m = meta.get(&g.app_id);
        let reason = excluded_by_name(g.app_id, &g.name).or_else(|| {
            m.filter(|m| !m.is_game())
                .map(|m| format!("Store type is \"{}\"", m.app_type.as_deref().unwrap_or("?")))
        });
        if let Some(reason) = reason {
            hidden.push(HiddenGame { app_id: g.app_id, name: g.name, reason });
            continue;
        }
        let (state, genres, categories, tags) = match m {
            None => (MetaState::Pending, vec![], vec![], vec![]),
            Some(m) if m.status == MetaStatus::Unknown => (MetaState::Unknown, vec![], vec![], vec![]),
            Some(m) => (MetaState::Known, m.genres.clone(), m.categories.clone(), m.tags.clone()),
        };
        games.push(LibraryGame { game: g, meta: state, genres, categories, tags });
    }
    let ids: HashSet<u32> = games.iter().map(|g| g.game.app_id).collect();
    let collections = raw
        .collections
        .list
        .iter()
        .map(|c| CollectionInfo {
            id: c.id.clone(),
            name: c.name.clone(),
            count: c.app_ids.iter().filter(|a| ids.contains(a)).count(),
            dynamic: c.dynamic,
        })
        .collect();
    LibraryView {
        games,
        hidden,
        collections,
        steam_hidden: raw.collections.hidden.clone(),
        steam: raw.info.clone(),
        api: raw.api.clone(),
        warnings: raw.warnings.clone(),
    }
}

/// App IDs whose metadata should be fetched: games never seen before first (so new
/// purchases and installs are filterable right away), then games whose tags were incomplete.
pub fn ids_needing_meta(raw: &RawLibrary, meta: &HashMap<u32, AppMeta>) -> Vec<u32> {
    let (mut new, retry): (Vec<u32>, Vec<u32>) = merge(&raw.local, raw.owned.as_deref())
        .into_iter()
        .filter(|g| excluded_by_name(g.app_id, &g.name).is_none())
        .filter(|g| match meta.get(&g.app_id) {
            None => true,
            Some(m) => m.status == MetaStatus::Known && m.is_game() && !m.tags_fetched,
        })
        .map(|g| g.app_id)
        .partition(|id| !meta.contains_key(id));
    new.extend(retry);
    new
}
