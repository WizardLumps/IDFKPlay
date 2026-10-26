use crate::error::AppError;
use serde::Serialize;
use std::path::{Path, PathBuf};

const DEFAULT_STEAM_DIR: &str = r"C:\Program Files (x86)\Steam";

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PathSource {
    Settings,
    Registry,
    Default,
}

#[derive(Debug)]
pub struct Located {
    pub path: PathBuf,
    pub source: PathSource,
    pub warning: Option<String>,
}

pub fn is_steam_dir(p: &Path) -> bool {
    p.join("steamapps").is_dir() || p.join("steam.exe").is_file()
}

/// Steam stores some paths with forward slashes and lowercase; make them Windows-shaped.
pub fn normalize(p: &str) -> PathBuf {
    PathBuf::from(p.trim().replace('/', "\\"))
}

/// The path with its real on-disk capitalization (the registry stores Steam's path lowercased).
/// Falls back to the input if it can't be resolved.
pub fn display_case(p: &Path) -> PathBuf {
    let Ok(canon) = std::fs::canonicalize(p) else {
        return p.to_path_buf();
    };
    // canonicalize returns `\\?\C:\...`; drop the verbatim prefix for plain drive paths only.
    let s = canon.display().to_string();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => p.to_path_buf(),
    }
}

/// Manual setting first (if valid), then registry, then the default install folder.
pub fn resolve(manual: Option<&str>) -> Result<Located, AppError> {
    let mut warning = None;
    if let Some(m) = manual.map(str::trim).filter(|m| !m.is_empty()) {
        let p = normalize(m);
        if is_steam_dir(&p) {
            return Ok(Located { path: display_case(&p), source: PathSource::Settings, warning: None });
        }
        warning = Some(format!(
            "The Steam folder in Settings ({}) doesn't look like a Steam install, so it was auto-detected instead.",
            p.display()
        ));
    }

    let found = registry_candidates()
        .into_iter()
        .map(|p| (p, PathSource::Registry))
        .chain(std::iter::once((PathBuf::from(DEFAULT_STEAM_DIR), PathSource::Default)))
        .find(|(p, _)| is_steam_dir(p));

    match found {
        Some((path, source)) => Ok(Located { path: display_case(&path), source, warning }),
        None if manual.is_some_and(|m| !m.trim().is_empty()) => {
            Err(AppError::SteamPathInvalid(normalize(manual.unwrap_or_default())))
        }
        None => Err(AppError::SteamNotFound),
    }
}

fn registry_candidates() -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let read = |hive, subkey: &str, value: &str| -> Option<PathBuf> {
        RegKey::predef(hive)
            .open_subkey(subkey)
            .and_then(|k| k.get_value::<String, _>(value))
            .ok()
            .map(|s| normalize(&s))
    };

    [
        read(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
        read(HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
    ]
    .into_iter()
    .flatten()
    .collect()
}
