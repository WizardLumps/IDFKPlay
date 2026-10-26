//! Game art: Steam's local library cache first, then Steam's public CDN.
//!
//! Local layouts seen in the wild:
//! - old flat:  `librarycache\<appid>_library_600x900.jpg`
//! - per-app:   `librarycache\<appid>\library_600x900.jpg`, with some images
//!   (hero, header) one level down in hash-named folders.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const CDN: &str = "https://cdn.cloudflare.steamstatic.com/steam/apps";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArtKind {
    /// 600x900 cover.
    Portrait,
    /// 460x215 banner.
    Header,
    /// Wide background.
    Hero,
}

impl ArtKind {
    /// File names to look for locally, best first.
    fn local_names(self) -> &'static [&'static str] {
        match self {
            ArtKind::Portrait => &["library_600x900_2x.jpg", "library_600x900.jpg"],
            ArtKind::Header => &["header.jpg", "library_header.jpg"],
            ArtKind::Hero => &["library_hero.jpg"],
        }
    }

    fn cdn_name(self) -> &'static str {
        match self {
            ArtKind::Portrait => "library_600x900.jpg",
            ArtKind::Header => "header.jpg",
            ArtKind::Hero => "library_hero.jpg",
        }
    }
}

/// One image to try: a local file (served via the asset protocol) or a URL.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum ArtSource {
    File(String),
    Url(String),
}

/// Candidates per slot, best first. The UI falls through on load errors.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GameArt {
    pub portrait: Vec<ArtSource>,
    pub header: Vec<ArtSource>,
    pub hero: Vec<ArtSource>,
}

pub fn library_cache_dir(steam: &Path) -> PathBuf {
    steam.join("appcache").join("librarycache")
}

fn non_empty_file(p: &Path) -> bool {
    fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0)
}

/// Finds a cached image of `kind` for `app_id`, checking both layouts.
pub fn find_local(cache: &Path, app_id: u32, kind: ArtKind) -> Option<PathBuf> {
    let app_dir = cache.join(app_id.to_string());
    for name in kind.local_names() {
        let direct = app_dir.join(name);
        if non_empty_file(&direct) {
            return Some(direct);
        }
    }
    // Newer layout nests some images one level down in hash-named folders.
    if let Ok(entries) = fs::read_dir(&app_dir) {
        let subdirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        for name in kind.local_names() {
            if let Some(p) = subdirs.iter().map(|d| d.join(name)).find(|p| non_empty_file(p)) {
                return Some(p);
            }
        }
    }
    kind.local_names()
        .iter()
        .map(|name| cache.join(format!("{app_id}_{name}")))
        .find(|p| non_empty_file(p))
}

pub fn cdn_url(app_id: u32, kind: ArtKind) -> String {
    format!("{CDN}/{app_id}/{}", kind.cdn_name())
}

pub fn game_art(steam: Option<&Path>, app_id: u32) -> GameArt {
    let cache = steam.map(library_cache_dir);
    let slot = |kind| {
        let mut v = Vec::new();
        if let Some(p) = cache.as_deref().and_then(|c| find_local(c, app_id, kind)) {
            v.push(ArtSource::File(p.display().to_string()));
        }
        v.push(ArtSource::Url(cdn_url(app_id, kind)));
        v
    };
    GameArt { portrait: slot(ArtKind::Portrait), header: slot(ArtKind::Header), hero: slot(ArtKind::Hero) }
}
