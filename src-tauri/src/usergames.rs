//! Games the user adds by hand (any launcher) and their edits to Steam games.
//!
//! Stored in `games.json` in the config dir. Images the user picks are copied into
//! `art\` there, so moving or deleting the original doesn't break them.
//!
//! Custom games get IDs from `CUSTOM_ID_BASE` up, a range Steam app IDs never reach,
//! so the blacklist, reroll lock and collections work for them unchanged.

use crate::jsonfile;
use crate::picker::Mood;
use crate::steam::art::ArtSource;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const SOURCE_ID: &str = "custom";
/// First custom game ID. Steam app IDs are far below this.
pub const CUSTOM_ID_BASE: u32 = 2_000_000_000;
const FILE: &str = "games.json";
const ART_DIR: &str = "art";
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

pub fn is_custom(id: u32) -> bool {
    id >= CUSTOM_ID_BASE
}

/// How a custom game starts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Launch {
    /// A program (or shortcut) with optional arguments, e.g. Battle.net.exe `--exec="launch WoW"`.
    Program {
        path: String,
        #[serde(default)]
        args: String,
    },
    /// A link a launcher understands, e.g. `com.epicgames.launcher://apps/...`.
    Link { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomGame {
    pub id: u32,
    pub name: String,
    pub launch: Launch,
    /// File name in the art folder, or an https URL.
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub moods: Vec<Mood>,
    #[serde(default)]
    pub favorite: bool,
    /// `None` = unknown: never counts as "Something New".
    #[serde(default)]
    pub playtime_minutes: Option<u32>,
}

/// The user's edits to a Steam game. Unset fields keep Steam's data.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GameOverride {
    pub name: Option<String>,
    /// File name in the art folder, or an https URL.
    pub image: Option<String>,
    /// Replaces Steam's tags/categories for mood filtering.
    pub moods: Option<Vec<Mood>>,
    /// Adds to (or, when false, removes from) Steam's Favorites.
    pub favorite: Option<bool>,
}

impl GameOverride {
    pub fn is_empty(&self) -> bool {
        *self == GameOverride::default()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct UserGames {
    pub custom: Vec<CustomGame>,
    pub overrides: HashMap<u32, GameOverride>,
}

impl UserGames {
    pub fn load(config_dir: &Path) -> Self {
        jsonfile::read(&config_dir.join(FILE)).unwrap_or_default()
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        jsonfile::write(&config_dir.join(FILE), self)
    }

    pub fn next_id(&self) -> u32 {
        self.custom.iter().map(|g| g.id + 1).max().unwrap_or(CUSTOM_ID_BASE).max(CUSTOM_ID_BASE)
    }

    pub fn custom(&self, id: u32) -> Option<&CustomGame> {
        self.custom.iter().find(|g| g.id == id)
    }

    /// The user's image for a game (custom or overridden), if any.
    pub fn image(&self, id: u32) -> Option<&str> {
        match self.custom(id) {
            Some(g) => g.image.as_deref(),
            None => self.overrides.get(&id).and_then(|o| o.image.as_deref()),
        }
    }

    /// Steam's Favorites adjusted by the user's edits, plus favorite custom games.
    pub fn favorites(&self, steam_favorites: &[u32]) -> Vec<u32> {
        let mut out: Vec<u32> = steam_favorites
            .iter()
            .copied()
            .filter(|id| self.overrides.get(id).and_then(|o| o.favorite) != Some(false))
            .collect();
        out.extend(self.overrides.iter().filter(|(_, o)| o.favorite == Some(true)).map(|(id, _)| *id));
        out.extend(self.custom.iter().filter(|g| g.favorite).map(|g| g.id));
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Tags and categories that make the picker's mood matching select exactly these moods.
pub fn mood_tags(moods: &[Mood]) -> (Vec<String>, Vec<String>) {
    let mut tags = Vec::new();
    let mut categories = Vec::new();
    for m in moods {
        match m {
            Mood::Action => tags.push("Action"),
            Mood::Adventure => tags.push("Adventure"),
            Mood::Puzzle => tags.push("Puzzle"),
            Mood::Rpg => tags.push("RPG"),
            Mood::Strategy => tags.push("Strategy"),
            Mood::Multiplayer => categories.push("Multi-player"),
            Mood::Controller => categories.push("Full controller support"),
            Mood::Vr => categories.push("VR Supported"),
        }
    }
    (tags.into_iter().map(String::from).collect(), categories.into_iter().map(String::from).collect())
}

pub fn art_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(ART_DIR)
}

/// Where a stored image reference points: a copied file in the art folder, or a URL.
pub fn image_source(config_dir: &Path, image: &str) -> ArtSource {
    if image.starts_with("https://") || image.starts_with("http://") {
        ArtSource::Url(image.to_string())
    } else {
        ArtSource::File(art_dir(config_dir).join(image).display().to_string())
    }
}

/// Copies an image the user picked into the art folder; returns the stored file name.
pub fn import_image(config_dir: &Path, source: &Path, id: u32) -> Result<String, String> {
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .filter(|e| IMAGE_EXTENSIONS.contains(&e.as_str()))
        .ok_or("Pick a JPG, PNG, WebP, GIF or BMP image.")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let name = format!("{id}-{stamp}.{ext}");
    let dir = art_dir(config_dir);
    fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    fs::copy(source, dir.join(&name)).map_err(|e| format!("Couldn't copy the image: {e}"))?;
    Ok(name)
}

/// Deletes a copied image that's no longer used (URLs and missing files are ignored).
pub fn remove_image(config_dir: &Path, image: Option<&str>) {
    if let Some(name) = image.filter(|i| !i.contains("://") && !i.contains(['/', '\\'])) {
        let _ = fs::remove_file(art_dir(config_dir).join(name));
    }
}

/// What to do with a game's image when saving an edit.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImageChange {
    #[default]
    Keep,
    /// Copy this local file into the art folder.
    File { path: String },
    Url { url: String },
    Clear,
}

/// Applies an image change, importing files and cleaning up the replaced copy.
fn apply_image(config_dir: &Path, current: Option<String>, change: &ImageChange, id: u32) -> Result<Option<String>, String> {
    let next = match change {
        ImageChange::Keep => return Ok(current),
        ImageChange::Clear => None,
        ImageChange::File { path } => Some(import_image(config_dir, Path::new(path.trim().trim_matches('"')), id)?),
        ImageChange::Url { url } => {
            let url = url.trim();
            if !url.starts_with("https://") {
                return Err("Image links must start with https://".into());
            }
            Some(url.to_string())
        }
    };
    remove_image(config_dir, current.as_deref());
    Ok(next)
}

/// A custom game as the editor sends it. `id: None` adds a new game.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomGameInput {
    pub id: Option<u32>,
    pub name: String,
    pub launch: Launch,
    #[serde(default)]
    pub image: ImageChange,
    #[serde(default)]
    pub moods: Vec<Mood>,
    #[serde(default)]
    pub favorite: bool,
    /// Hours; `None` = unknown.
    #[serde(default)]
    pub playtime_hours: Option<f64>,
}

fn validate_launch(launch: &Launch) -> Result<Launch, String> {
    match launch {
        Launch::Program { path, args } => {
            let path = path.trim().trim_matches('"').to_string();
            if path.is_empty() {
                return Err("Choose the program that starts the game.".into());
            }
            Ok(Launch::Program { path, args: args.trim().to_string() })
        }
        Launch::Link { url } => {
            let url = url.trim().to_string();
            if !url.contains("://") {
                return Err("A launch link looks like something://… (for example com.epicgames.launcher://…).".into());
            }
            Ok(Launch::Link { url })
        }
    }
}

impl UserGames {
    /// Adds or updates a custom game; returns its ID.
    pub fn save_custom(&mut self, config_dir: &Path, input: CustomGameInput) -> Result<u32, String> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err("Give the game a name.".into());
        }
        let launch = validate_launch(&input.launch)?;
        let playtime_minutes = match input.playtime_hours {
            Some(h) if !h.is_finite() || h < 0.0 => return Err("Hours played can't be negative.".into()),
            Some(h) => Some((h * 60.0).round().min(u32::MAX as f64) as u32),
            None => None,
        };
        let mut moods: Vec<Mood> = Vec::new();
        for m in input.moods {
            if !moods.contains(&m) {
                moods.push(m);
            }
        }

        let id = match input.id {
            Some(id) => {
                if self.custom(id).is_none() {
                    return Err("That game no longer exists.".into());
                }
                id
            }
            None => self.next_id(),
        };
        let current = self.custom(id).and_then(|g| g.image.clone());
        let image = apply_image(config_dir, current, &input.image, id)?;
        let game = CustomGame { id, name, launch, image, moods, favorite: input.favorite, playtime_minutes };
        match self.custom.iter_mut().find(|g| g.id == id) {
            Some(existing) => *existing = game,
            None => self.custom.push(game),
        }
        Ok(id)
    }

    pub fn delete_custom(&mut self, config_dir: &Path, id: u32) {
        if let Some(pos) = self.custom.iter().position(|g| g.id == id) {
            let game = self.custom.remove(pos);
            remove_image(config_dir, game.image.as_deref());
        }
    }

    /// Saves edits to a Steam game. An override with nothing set is removed.
    pub fn save_override(&mut self, config_dir: &Path, app_id: u32, input: OverrideInput) -> Result<(), String> {
        let current = self.overrides.get(&app_id).cloned().unwrap_or_default();
        let image = apply_image(config_dir, current.image, &input.image, app_id)?;
        let o = GameOverride {
            name: input.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
            image,
            moods: input.moods,
            favorite: input.favorite,
        };
        if o.is_empty() {
            self.overrides.remove(&app_id);
        } else {
            self.overrides.insert(app_id, o);
        }
        Ok(())
    }

    /// Drops all edits to a Steam game (including its copied image).
    pub fn reset_override(&mut self, config_dir: &Path, app_id: u32) {
        if let Some(o) = self.overrides.remove(&app_id) {
            remove_image(config_dir, o.image.as_deref());
        }
    }

    /// Sets a game's favorite state (custom or Steam) given whether Steam has it in Favorites.
    pub fn set_favorite(&mut self, app_id: u32, favorite: bool, steam_favorite: bool) {
        if let Some(g) = self.custom.iter_mut().find(|g| g.id == app_id) {
            g.favorite = favorite;
            return;
        }
        let o = self.overrides.entry(app_id).or_default();
        // Matching Steam needs no override.
        o.favorite = (favorite != steam_favorite).then_some(favorite);
        if o.is_empty() {
            self.overrides.remove(&app_id);
        }
    }
}

/// Edits to a Steam game as the editor sends them. `None` keeps Steam's value.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OverrideInput {
    pub name: Option<String>,
    pub image: ImageChange,
    pub moods: Option<Vec<Mood>>,
    pub favorite: Option<bool>,
}
