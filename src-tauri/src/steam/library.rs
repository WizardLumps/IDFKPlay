use super::locate::normalize;
use crate::vdf::{self, ParseError, Value};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// `StateFlags` bit meaning "fully installed".
const STATE_FULLY_INSTALLED: u32 = 4;

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum LibraryOrigin {
    Steam,
    Settings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    pub path: String,
    pub origin: LibraryOrigin,
    pub reachable: bool,
    pub game_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppManifest {
    pub app_id: u32,
    pub name: String,
    pub state_flags: u32,
    pub install_dir: Option<String>,
    pub size_on_disk: Option<u64>,
}

impl AppManifest {
    pub fn is_installed(&self) -> bool {
        self.state_flags & STATE_FULLY_INSTALLED != 0
    }
}

/// Library paths from `libraryfolders.vdf`. Supports the current format
/// (`"0" { "path" "..." }`) and the legacy one (`"1" "D:\\SteamLibrary"`).
pub fn parse_library_folders(text: &str) -> Result<Vec<PathBuf>, ParseError> {
    let doc = vdf::parse(text)?;
    let Some(root) = doc.get_obj("libraryfolders") else {
        return Ok(Vec::new());
    };
    Ok(root
        .iter()
        .filter(|(k, _)| k.parse::<u32>().is_ok())
        .filter_map(|(_, v)| match v {
            Value::Obj(o) => o.get_str("path"),
            Value::Str(s) => Some(s.as_str()),
        })
        .filter(|p| !p.trim().is_empty())
        .map(normalize)
        .collect())
}

/// Parses one `appmanifest_<id>.acf`. Returns `None` if it has no usable appid.
pub fn parse_app_manifest(text: &str) -> Result<Option<AppManifest>, ParseError> {
    let doc = vdf::parse(text)?;
    let Some(state) = doc.get_obj("AppState") else {
        return Ok(None);
    };
    let Some(app_id) = state.get_str("appid").and_then(|s| s.trim().parse().ok()) else {
        return Ok(None);
    };
    let name = state
        .get_str("name")
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("App {app_id}"));
    Ok(Some(AppManifest {
        app_id,
        name,
        state_flags: state.get_str("StateFlags").and_then(|s| s.trim().parse().ok()).unwrap_or(0),
        install_dir: state.get_str("installdir").map(String::from).filter(|s| !s.is_empty()),
        size_on_disk: state.get_str("SizeOnDisk").and_then(|s| s.trim().parse().ok()),
    }))
}

/// Case-insensitive, slash-agnostic key for de-duplicating library paths.
/// Resolved on disk when possible so short (8.3) and long spellings match.
pub fn path_key(p: &Path) -> String {
    super::locate::display_case(p)
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Every library: the Steam folder itself, those in libraryfolders.vdf, then extras from Settings.
pub fn collect_libraries(
    steam: &Path,
    extra: &[String],
    warnings: &mut Vec<String>,
) -> Vec<(PathBuf, LibraryOrigin)> {
    let mut libs = vec![(steam.to_path_buf(), LibraryOrigin::Steam)];

    let vdf_path = steam.join("steamapps").join("libraryfolders.vdf");
    match fs::read_to_string(&vdf_path) {
        Ok(text) => match parse_library_folders(&text) {
            Ok(paths) => libs.extend(paths.into_iter().map(|p| (p, LibraryOrigin::Steam))),
            Err(e) => warnings.push(format!(
                "Couldn't read Steam's library list ({e}). Only the main Steam folder was scanned."
            )),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warnings.push(format!("Couldn't open {}: {e}", vdf_path.display())),
    }

    libs.extend(
        extra
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| (normalize(s), LibraryOrigin::Settings)),
    );

    let mut seen = std::collections::HashSet::new();
    libs.retain(|(p, _)| seen.insert(path_key(p)));
    libs
}

/// Reads every appmanifest in `<library>\steamapps`. Bad files become warnings, not errors.
pub fn scan_library(library: &Path, warnings: &mut Vec<String>) -> std::io::Result<Vec<AppManifest>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(library.join("steamapps"))? {
        let Ok(entry) = entry else { continue };
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy().to_lowercase();
        if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
            continue;
        }
        let path = entry.path();
        let parsed = fs::read(&path).map_err(|e| e.to_string()).and_then(|bytes| {
            // Steam occasionally leaves manifests zero-filled after a crash or power loss.
            if bytes.iter().all(|&b| b == 0 || b.is_ascii_whitespace()) {
                return Err("the file is empty or zero-filled; verifying the game in Steam usually fixes it".into());
            }
            parse_app_manifest(&String::from_utf8_lossy(&bytes)).map_err(|e| e.to_string())
        });
        match parsed {
            Ok(Some(m)) => out.push(m),
            Ok(None) => {}
            Err(e) => warnings.push(format!("Skipped a broken game manifest ({}): {e}", path.display())),
        }
    }
    Ok(out)
}
